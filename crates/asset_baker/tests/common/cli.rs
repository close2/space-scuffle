//! Helpers for tests that run the built `asset_baker` binary against a fake Veo service.
//!
//! # Responsibility
//! Builds a baker command that cannot reach a real key or a real service: the key variables are
//! removed from its environment, credentials come only from a temporary directory the test
//! writes, and curl is a [`super::CurlShim`] aimed at a loopback [`super::FakeServer`].
//!
//! # Invariants
//! - Identities are chosen from the shipped catalog by capability, never by name.
//! - The fake key is obviously fake and long enough that any leaked fragment is detectable.

use super::{CurlShim, FakeServer, Reply};
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fake Gemini API key. It must never appear in a ledger, a log or a clip record.
pub const FAKE_KEY: &str = "AIzaFAKE-wp1-leak-probe-7f3c9e21b4d6a8f0-not-a-credential";

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

/// Asserts that no 12-character window of [`FAKE_KEY`] occurs in `haystack`.
pub fn assert_no_key(haystack: &str, what: &str) {
    let key: Vec<char> = FAKE_KEY.chars().collect();
    for window in key.windows(12) {
        let fragment: String = window.iter().collect();
        assert!(
            !haystack.contains(&fragment),
            "a fragment of the API key ({fragment}) leaked into {what}:\n{haystack}"
        );
    }
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
/// poll with the fixture clip inline.
pub fn healthy(clip: Vec<u8>) -> impl Fn(&super::ReceivedRequest, usize) -> Reply + Send {
    move |request, _| {
        if request.path.contains(":predictLongRunning") {
            Reply::json(&format!(
                r#"{{"name":"operations/{}-op"}}"#,
                state_of_body(&request.body)
            ))
        } else {
            let name = request
                .path
                .rsplit('/')
                .next()
                .unwrap_or("unknown")
                .to_string();
            Reply::json(&inline_done(&format!("operations/{name}"), &clip))
        }
    }
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
    /// Creates a fresh set of directories with the fake key.
    #[must_use]
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let credentials = fake_credentials(root.path());
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
        let mut command = baker();
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
