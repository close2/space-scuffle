//! Shared helpers for `asset_baker` integration tests that exercise real external tools.
//!
//! # Fixture policy
//! `tests/fixtures/luma_ramp_4s.mp4` is a *test fixture*, permitted by owner decision D6.
//! It is synthetic and never reaches a runtime asset path. It is a real H.264 MP4 of the
//! same length as a default Veo clip, so the real FFmpeg decoder and the real packing code
//! run against it. Its luminance rises linearly from black to white over four seconds,
//! which lets tests measure *which moment of the clip* a baked frame came from.
//!
//! Regenerate it with:
//! ```text
//! ffmpeg -f lavfi -i "color=c=black:s=64x64:r=15:d=4,format=yuv420p,geq=lum='16+219*T/4':cb=128:cr=128" \
//!     -c:v libx264 -preset veryslow -crf 28 -pix_fmt yuv420p -g 15 -bf 0 -movflags +faststart \
//!     -map_metadata -1 -fflags +bitexact -flags:v +bitexact tests/fixtures/luma_ramp_4s.mp4
//! ```
//!
//! # No silent skips
//! A missing fixture or missing FFmpeg is a test *failure*, never an early `return`. The
//! suite must not be able to report success without having decoded a real video. CI
//! installs FFmpeg, and no opt-out exists.
//!
//! # HTTP behaviour
//! [`FakeServer`] and [`CurlShim`] let the real curl executable talk to a scripted loopback
//! server. Nothing here can reach a remote service.

#![allow(dead_code, clippy::doc_markdown, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

pub mod cli;
pub mod loop_clip;

/// Returns the fixture clip, resolved from `CARGO_MANIFEST_DIR` so that the result does not
/// depend on the test working directory, after asserting that the real tools it needs exist.
///
/// # Panics
/// Panics with instructions if FFmpeg or the committed fixture is missing.
#[must_use]
pub fn fixture_clip() -> PathBuf {
    assert!(
        Path::new("/usr/bin/ffmpeg").is_file(),
        "/usr/bin/ffmpeg is missing: video tests cannot run and must not be skipped. Install ffmpeg."
    );
    let clip = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/luma_ramp_4s.mp4");
    assert!(
        clip.is_file(),
        "committed fixture clip is missing at {}",
        clip.display()
    );
    clip
}

/// The bytes of a 16:9 clip of 1280×720 at 24 fps and 4 s, the shape Veo returns, made by the
/// real FFmpeg on first use and kept for the life of the test process.
///
/// It fades linearly from black (frame 0) to white (the last frame), so a test can tell which
/// moment of the clip a chained keyframe came from: frame 0 is black. Its frames satisfy the
/// pin rules (16:9, at least 1280×720), so a run can chain a pinned state from it. It is made
/// at test time, as `ffmpeg_tests.rs` makes its fixtures, so no large binary is committed.
///
/// # Panics
/// Panics if FFmpeg is missing or fails: video tests must not be skipped.
#[must_use]
pub fn wide_fixture_bytes() -> Vec<u8> {
    static BYTES: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    BYTES
        .get_or_init(|| {
            assert!(
                Path::new("/usr/bin/ffmpeg").is_file(),
                "/usr/bin/ffmpeg is missing: video tests cannot run and must not be skipped"
            );
            let dir = tempfile::tempdir().expect("fixture tempdir");
            let clip = dir.path().join("wide_fade_4s.mp4");
            let output = std::process::Command::new("/usr/bin/ffmpeg")
                .args(["-v", "error", "-f", "lavfi", "-i"])
                .arg("color=c=white:s=1280x720:r=24:d=4,fade=t=in:st=0:d=4,format=yuv420p")
                .args([
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-pix_fmt",
                    "yuv420p",
                ])
                .arg(&clip)
                .output()
                .expect("run ffmpeg");
            assert!(
                output.status.success(),
                "ffmpeg could not make the wide fixture: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            std::fs::read(&clip).expect("read wide fixture")
        })
        .clone()
}

/// One HTTP request as received by [`FakeServer`].
#[derive(Debug, Clone)]
pub struct ReceivedRequest {
    /// HTTP method, such as `GET`.
    pub method: String,
    /// Request target. [`CurlShim`] prefixes it with the original HTTPS host.
    pub path: String,
    /// Header lines, lowercased, in `name: value` form.
    pub headers: Vec<String>,
    /// Raw request body.
    pub body: Vec<u8>,
}

impl ReceivedRequest {
    /// Whether any header line contains `needle`, compared case-insensitively.
    #[must_use]
    pub fn header_contains(&self, needle: &str) -> bool {
        let needle = needle.to_lowercase();
        self.headers.iter().any(|line| line.contains(&needle))
    }
}

/// A scripted [`FakeServer`] response.
pub enum Reply {
    /// Sends a complete response and closes the connection.
    Respond {
        /// HTTP status code.
        status: u16,
        /// Extra `name: value` header lines.
        headers: Vec<String>,
        /// Response body.
        body: Vec<u8>,
    },
    /// Accepts the request and then never answers.
    Hang,
}

impl Reply {
    /// A `200 OK` JSON reply.
    #[must_use]
    pub fn json(body: &str) -> Self {
        Self::Respond {
            status: 200,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    /// A reply with the given status and text body.
    #[must_use]
    pub fn status(status: u16, body: &str) -> Self {
        Self::Respond {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    /// A `302 Found` redirect.
    #[must_use]
    pub fn redirect(location: &str) -> Self {
        Self::Respond {
            status: 302,
            headers: vec![format!("Location: {location}")],
            body: Vec::new(),
        }
    }

    /// A `200 OK` binary reply.
    #[must_use]
    pub fn bytes(body: Vec<u8>) -> Self {
        Self::Respond {
            status: 200,
            headers: Vec::new(),
            body,
        }
    }
}

/// A loopback-only HTTP/1.1 server that records requests and answers from a script.
///
/// The handler receives each request and its zero-based arrival index.
pub struct FakeServer {
    port: u16,
    requests: Arc<Mutex<Vec<ReceivedRequest>>>,
}

impl FakeServer {
    /// Binds an ephemeral loopback port and serves on a background thread until process exit.
    #[must_use]
    pub fn start<H>(handler: H) -> Self
    where
        H: Fn(&ReceivedRequest, usize) -> Reply + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let port = listener.local_addr().expect("listener address").port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        std::thread::spawn(move || {
            let mut hanging = Vec::new();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                let index = {
                    let mut log = recorded.lock().expect("request log poisoned");
                    log.push(request.clone());
                    log.len() - 1
                };
                match handler(&request, index) {
                    Reply::Hang => hanging.push(stream),
                    Reply::Respond {
                        status,
                        headers,
                        body,
                    } => {
                        let mut head = format!(
                            "HTTP/1.1 {status} Scripted\r\nContent-Length: {}\r\nConnection: close\r\n",
                            body.len()
                        );
                        for line in headers {
                            head.push_str(&line);
                            head.push_str("\r\n");
                        }
                        head.push_str("\r\n");
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(&body);
                    }
                }
            }
        });
        Self { port, requests }
    }

    /// The bound loopback port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// A snapshot of every request received so far.
    #[must_use]
    pub fn requests(&self) -> Vec<ReceivedRequest> {
        self.requests.lock().expect("request log poisoned").clone()
    }
}

fn read_request(stream: &mut TcpStream) -> Option<ReceivedRequest> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let line = line.trim_end().to_lowercase();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("content-length:") {
            content_length = value.trim().parse().ok()?;
        }
        headers.push(line);
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).ok()?;
    Some(ReceivedRequest {
        method,
        path,
        headers,
        body,
    })
}

/// Serialises every test that uses a [`CurlShim`].
///
/// A script that was just written can fail to execute with `ETXTBSY` if another thread forks
/// while its write descriptor is open, so shim tests never run concurrently.
static SHIM_LOCK: Mutex<()> = Mutex::new(());

/// A `curl` stand-in that records its invocation and delegates to the real `/usr/bin/curl`.
///
/// Every `https://host/path` argument is rewritten to `http://127.0.0.1:port/host/path`, so
/// production code keeps its strict HTTPS host allowlist, the real curl argument handling is
/// exercised, and tests cannot reach a remote service. `--proto` is dropped only because the
/// rewritten loopback URL is plain HTTP. Tests assert on the recording, which holds the
/// arguments exactly as production supplied them.
///
/// Rewriting the arguments alone is not containment: a `Location` header is followed by curl
/// itself if production ever passes `-L`. The shim therefore always appends two guards that
/// hold even for such a regression. `--max-redirs 0` stops curl following anything, and
/// `--connect-to ::127.0.0.1:port` sends every connection to the loopback server whatever
/// host a URL names.
#[cfg(unix)]
pub struct CurlShim {
    dir: tempfile::TempDir,
    path: PathBuf,
    _serial: MutexGuard<'static, ()>,
}

#[cfg(unix)]
impl CurlShim {
    /// Installs a shim that targets `server`.
    #[must_use]
    pub fn new(server: &FakeServer) -> Self {
        let serial = SHIM_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().expect("shim tempdir");
        let path = dir.path().join("curl");
        let script = format!(
            r#"#!/bin/sh
dir='{dir}'
port={port}
unset http_proxy https_proxy all_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY
{{ for arg in "$@"; do printf '%s\n' "$arg"; done; echo '--end-of-call--'; }} >> "$dir/argv.log"
cat > "$dir/stdin.current"
cat "$dir/stdin.current" >> "$dir/stdin.log"
remaining=$#
while [ "$remaining" -gt 0 ]; do
    arg=$1
    shift
    remaining=$((remaining - 1))
    case "$arg" in
        --proto) shift; remaining=$((remaining - 1)) ;;
        https://*) set -- "$@" "http://127.0.0.1:$port/${{arg#https://}}" ;;
        *) set -- "$@" "$arg" ;;
    esac
done
exec /usr/bin/curl "$@" --max-redirs 0 --connect-to "::127.0.0.1:$port" < "$dir/stdin.current"
"#,
            dir = dir.path().display(),
            port = server.port(),
        );
        std::fs::write(&path, script).expect("write curl shim");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("mark curl shim executable");
        Self {
            dir,
            path,
            _serial: serial,
        }
    }

    /// Path to pass as the client's curl executable.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every recorded argument across all calls, one per line.
    #[must_use]
    pub fn argv(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("argv.log")).unwrap_or_default()
    }

    /// All bytes that production wrote to curl's standard input, across all calls.
    #[must_use]
    pub fn stdin(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("stdin.log")).unwrap_or_default()
    }
}
