//! Helpers for tests that run the built `asset_baker` binary against a fake Veo service.
//!
//! # Responsibility
//! Builds a baker command that cannot reach a real key or a real service: the key variables are
//! removed from its environment, credentials come only from a temporary directory the test
//! writes, and curl is a [`super::CurlShim`] aimed at a loopback [`super::FakeServer`].
//!
//! # Invariants
//! - Identities are chosen from the shipped catalog by capability, never by name.
//! - The fake keys are obviously fake and long enough that any leaked fragment is detectable.
//! - A generation run uses Vertex AI by default ([`RunDirs::new`]), as production does: only
//!   Vertex accepts an end frame (`lastFrame`), which the shipped loop policy pins for every
//!   looping state. [`RunDirs::gemini`] selects the Gemini API.

use super::{CurlShim, FakeServer, Reply};
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fake Gemini API key. It must never appear in a ledger, a log or a clip record.
pub const FAKE_KEY: &str = "AIzaFAKE-wp1-leak-probe-7f3c9e21b4d6a8f0-not-a-credential";

/// A fake Vertex AI Express key (`AQ.` prefix). It must never appear in a ledger, a log or a
/// clip record.
pub const FAKE_VERTEX_KEY: &str = "AQ.FAKE-loops-leak-probe-3b8e0d5a9c71f246-not-a-credential";

/// The project a fake Vertex key is paired with.
pub const FAKE_PROJECT: &str = "fixture-project-000";

/// The shipped asset bundle (catalogs, media style and budget data).
#[must_use]
pub fn shipped_assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// The shipped unit catalog, loaded the way the baker loads it.
#[must_use]
pub fn shipped_catalog() -> sim_core::UnitCatalog {
    asset_baker::load_catalogs(&shipped_assets())
        .expect("shipped catalogs load")
        .units
}

/// The baker binary with the key variables removed and `--assets` set to the shipped bundle.
#[must_use]
pub fn baker() -> Command {
    baker_with_assets(&shipped_assets())
}

/// The baker binary with the key variables removed and `--assets` set to `bundle`.
#[must_use]
pub fn baker_with_assets(bundle: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_asset_baker"));
    command
        .env_remove("GEMINI_API_KEY")
        .env_remove("GOOGLE_API_KEY")
        .env_remove("GOOGLE_CLOUD_PROJECT")
        .arg("--assets")
        .arg(bundle);
    command
}

/// Writes `inner/.env` holding [`FAKE_KEY`] under `root` and returns `inner`.
///
/// The key lives one level down so that the resolver's parent search stays inside `root`.
#[must_use]
pub fn fake_credentials(root: &Path) -> PathBuf {
    let inner = root.join("credentials").join("inner");
    std::fs::create_dir_all(&inner).expect("credentials dir");
    std::fs::write(inner.join(".env"), format!("GEMINI_API_KEY={FAKE_KEY}\n")).expect("env");
    inner
}

/// Writes `inner/.env` holding [`FAKE_VERTEX_KEY`] and [`FAKE_PROJECT`] under `root` and
/// returns `inner`.
#[must_use]
pub fn fake_vertex_credentials(root: &Path) -> PathBuf {
    let inner = root.join("vertex-credentials").join("inner");
    std::fs::create_dir_all(&inner).expect("credentials dir");
    std::fs::write(
        inner.join(".env"),
        format!("GOOGLE_API_KEY={FAKE_VERTEX_KEY}\nGOOGLE_CLOUD_PROJECT={FAKE_PROJECT}\n"),
    )
    .expect("env");
    inner
}

/// A credentials directory whose `.env` is a directory, so any attempt to read it fails.
///
/// A run that succeeds with this directory provably read no credential file.
#[must_use]
pub fn trapped_credentials(root: &Path) -> PathBuf {
    let inner = root.join("trap").join("inner");
    std::fs::create_dir_all(inner.join(".env")).expect("trap .env");
    std::fs::create_dir_all(inner.join("tmp").join("api-key.txt")).expect("trap key file");
    inner
}

/// Standard output and standard error of a finished command, as text.
#[must_use]
pub fn text(output: &Output) -> String {
    format!(
        "status: {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Asserts that no 12-character window of [`FAKE_KEY`] or [`FAKE_VERTEX_KEY`] occurs in
/// `haystack`.
pub fn assert_no_key(haystack: &str, what: &str) {
    for key in [FAKE_KEY, FAKE_VERTEX_KEY] {
        let key: Vec<char> = key.chars().collect();
        for window in key.windows(12) {
            let fragment: String = window.iter().collect();
            assert!(
                !haystack.contains(&fragment),
                "a fragment of the API key ({fragment}) leaked into {what}:\n{haystack}"
            );
        }
    }
}

/// Writes a `width`×`height` PNG with a diagonal gradient to `path` and returns its bytes.
/// A 1280×720 picture passes the pin rules; other sizes test them.
pub fn write_picture(path: &Path, width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        let level = |v: u32, span: u32| u8::try_from(v * 255 / span.max(1)).unwrap_or(u8::MAX);
        image::Rgb([level(x, width), level(y, height), 96])
    });
    image
        .save_with_format(path, image::ImageFormat::Png)
        .expect("write picture");
    std::fs::read(path).expect("read picture")
}

/// The `instances[0]` object of a start request's JSON body.
#[must_use]
pub fn instance_of(body: &[u8]) -> serde_json::Value {
    let json: serde_json::Value = serde_json::from_slice(body).expect("JSON body");
    json["instances"][0].clone()
}

/// The first catalog identity whose published actions are exactly `wanted`, in any order,
/// that satisfies `extra` and has an authored prompt for each of them when `authored` is set.
#[must_use]
pub fn identity_with_actions(
    catalog: &sim_core::UnitCatalog,
    wanted: &[AnimState],
    authored: bool,
    extra: impl Fn(&sim_core::UnitArchetype) -> bool,
) -> String {
    catalog
        .archetypes()
        .find(|archetype| {
            let published: Vec<AnimState> = catalog.publishable_actions(archetype).iter().collect();
            published.len() == wanted.len()
                && wanted.iter().all(|state| published.contains(state))
                && (!authored || wanted.iter().all(|s| archetype.veo_prompt(*s).is_some()))
                && extra(archetype)
        })
        .map_or_else(
            || panic!("the shipped catalog has an identity publishing {wanted:?}"),
            |archetype| archetype.slug.clone(),
        )
}

/// An armed mobile unit with authored Idle, Walk and Attack prompts.
#[must_use]
pub fn combat_unit(catalog: &sim_core::UnitCatalog) -> String {
    identity_with_actions(
        catalog,
        &[AnimState::Idle, AnimState::Walk, AnimState::Attack],
        true,
        |_| true,
    )
}

/// A second such unit, different from `other`.
#[must_use]
pub fn another_combat_unit(catalog: &sim_core::UnitCatalog, other: &str) -> String {
    identity_with_actions(
        catalog,
        &[AnimState::Idle, AnimState::Walk, AnimState::Attack],
        true,
        |archetype| archetype.slug != other,
    )
}

/// The state whose action description a fake-server request body carries.
#[must_use]
pub fn state_of_body(body: &[u8]) -> &'static str {
    let body = String::from_utf8_lossy(body).to_lowercase();
    ["constructing", "idle", "walk", "attack", "cast"]
        .into_iter()
        .find(|state| body.contains(&format!("{state} animation")))
        .unwrap_or("unknown")
}

/// A finished Gemini operation carrying `clip` inline.
#[must_use]
pub fn inline_done(name: &str, clip: &[u8]) -> String {
    use base64::prelude::*;
    serde_json::json!({
        "name": name,
        "done": true,
        "response": { "generateVideoResponse": { "generatedSamples": [
            { "video": { "bytesBase64Encoded": BASE64_STANDARD.encode(clip) } }
        ] } }
    })
    .to_string()
}

/// A healthy fake API: each start names `operations/<state>-op`, which finishes at the first
/// poll with `clip` inline. A Gemini poll names the operation in its path, a Vertex poll
/// (`:fetchPredictOperation`) in its body.
pub fn healthy(clip: Vec<u8>) -> impl Fn(&super::ReceivedRequest, usize) -> Reply + Send {
    move |request, _| {
        if request.path.contains(":predictLongRunning") {
            Reply::json(&format!(
                r#"{{"name":"operations/{}-op"}}"#,
                state_of_body(&request.body)
            ))
        } else {
            Reply::json(&inline_done(&polled_operation(request), &clip))
        }
    }
}

/// The operation a poll asks about, as `operations/<name>`: from `operationName` in a Vertex
/// body, else from the last segment of a Gemini path.
#[must_use]
pub fn polled_operation(request: &super::ReceivedRequest) -> String {
    let from_body = serde_json::from_slice::<serde_json::Value>(&request.body)
        .ok()
        .and_then(|json| json["operationName"].as_str().map(ToString::to_string));
    let full = from_body.unwrap_or_else(|| request.path.clone());
    let name = full.rsplit('/').next().unwrap_or("unknown");
    format!("operations/{name}")
}

/// Paths of one fake generation run.
pub struct RunDirs {
    /// Temporary root holding everything below.
    pub root: tempfile::TempDir,
    /// Credentials directory holding the fake key.
    pub credentials: PathBuf,
    /// Empty keyframes directory.
    pub keyframes: PathBuf,
    /// Installed clips (`<slug>_<state>.mp4`).
    pub videos: PathBuf,
    /// Downloaded clips awaiting review.
    pub clips: PathBuf,
    /// The ledger file.
    pub ledger: PathBuf,
}

impl RunDirs {
    /// Creates a fresh set of directories with the fake Vertex AI key, the mode production
    /// uses.
    #[must_use]
    pub fn new() -> Self {
        Self::with_credentials(fake_vertex_credentials)
    }

    /// Creates a fresh set of directories with the fake Gemini API key.
    #[must_use]
    pub fn gemini() -> Self {
        Self::with_credentials(fake_credentials)
    }

    fn with_credentials(write: fn(&Path) -> PathBuf) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let credentials = write(root.path());
        let keyframes = root.path().join("keyframes");
        let videos = root.path().join("videos");
        std::fs::create_dir_all(&keyframes).expect("keyframes dir");
        std::fs::create_dir_all(&videos).expect("videos dir");
        Self {
            credentials,
            keyframes,
            videos,
            clips: root.path().join("clips"),
            ledger: root.path().join("evidence").join("ledger.jsonl"),
            root,
        }
    }

    /// `generate-veo` for `units` with every directory set, the shim as curl, and `extra` args.
    #[must_use]
    pub fn generate(&self, shim: &CurlShim, units: &[&str], extra: &[&str]) -> Output {
        self.generate_in(&shipped_assets(), shim, units, extra)
    }

    /// As [`Self::generate`], with the asset bundle `bundle` instead of the shipped one.
    #[must_use]
    pub fn generate_in(
        &self,
        bundle: &Path,
        shim: &CurlShim,
        units: &[&str],
        extra: &[&str],
    ) -> Output {
        let mut command = baker_with_assets(bundle);
        command.arg("generate-veo");
        for unit in units {
            command.args(["--unit", unit]);
        }
        command
            .arg("--keyframes-dir")
            .arg(&self.keyframes)
            .arg("--videos-dir")
            .arg(&self.videos)
            .arg("--clips-dir")
            .arg(&self.clips)
            .arg("--ledger")
            .arg(&self.ledger)
            .arg("--credentials-dir")
            .arg(&self.credentials)
            .arg("--curl-path")
            .arg(shim.path())
            .args(extra)
            .output()
            .expect("run baker")
    }

    /// The ledger's lines parsed as JSON, or none if it does not exist.
    #[must_use]
    pub fn ledger_lines(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.ledger)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).expect("ledger line is JSON"))
            .collect()
    }

    /// The `event` field of every ledger line, in order.
    #[must_use]
    pub fn ledger_events(&self) -> Vec<String> {
        self.ledger_lines()
            .iter()
            .map(|line| line["event"].as_str().unwrap_or("?").to_string())
            .collect()
    }

    /// Every file under the clips directory, as text where it is UTF-8.
    #[must_use]
    pub fn clip_records(&self) -> String {
        let mut all = String::new();
        let mut stack = vec![self.clips.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(content) = std::fs::read_to_string(&path) {
                    all.push_str(&content);
                }
            }
        }
        all
    }
}

impl Default for RunDirs {
    fn default() -> Self {
        Self::new()
    }
}

/// Number of start requests the fake server received.
#[must_use]
pub fn starts(server: &FakeServer) -> usize {
    server
        .requests()
        .iter()
        .filter(|request| request.path.contains(":predictLongRunning"))
        .count()
}
