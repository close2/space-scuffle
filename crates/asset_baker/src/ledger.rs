//! The append-only ledger of every paid Veo request (plan `2026-11-04-phase-8-5-veo-media.md`,
//! decision D-3).
//!
//! # Responsibility
//! Records each event of a generation run as one JSON object per line, and reads a ledger back
//! into a [`LedgerHistory`]: what each clip's last request achieved, which operations can be
//! resumed, which downloads await review, which failure kinds were seen, and how many
//! generations were started in a time window.
//!
//! # How it fits
//! `generate-veo` writes `request_started` before it starts a paid request, so a crash cannot
//! hide a spend. The plan (`veo_plan`) reads the history to decide which clips to reuse, resume,
//! leave for review or generate; the budget check (`budget`) sums its starts; `install-clip`
//! accepts only a clip the ledger says was downloaded. The plan commits the ledger to the
//! repository (`docs/evidence/veo/ledger.jsonl`): operation names are not secrets.
//!
//! # Invariants
//! - The writer takes typed [`LedgerEvent`]s, never free text from a request, so a key, a
//!   header or a curl command line has no field to travel in. Text that came from the network
//!   is redacted by the client before it becomes an event.
//! - Lines are only ever appended, each with one `write` and a `sync_data`, and never rewritten.
//! - A line that cannot be parsed stops the reader with its line number: a ledger that cannot
//!   be read cannot prove what was spent, so no run may start on it.

use crate::veo_client::VeoParameters;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sim_types::enums::AnimState;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Seconds in one day, for ledger time windows.
pub const SECONDS_PER_DAY: u64 = 86_400;

/// Failures to read or write the ledger.
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// The ledger file or its directory could not be read or written.
    #[error("ledger I/O error at '{path}': {source}")]
    Io {
        /// The ledger path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A line is not a ledger record.
    #[error("ledger '{path}' line {line} is not a valid record: {source}. No run may start on a ledger that cannot be read")]
    Malformed {
        /// The ledger path.
        path: PathBuf,
        /// One-based line number.
        line: usize,
        /// Parser error.
        #[source]
        source: serde_json::Error,
    },
    /// An event could not be encoded as JSON.
    #[error("cannot encode a ledger event: {0}")]
    Encode(#[source] serde_json::Error),
}

/// Which Veo API a request went to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    /// The Gemini API (`generativelanguage.googleapis.com`).
    GeminiApi,
    /// Vertex AI in express mode (`us-central1-aiplatform.googleapis.com`).
    VertexExpress,
}

impl ApiMode {
    /// A human-readable name.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::GeminiApi => "Gemini API",
            Self::VertexExpress => "Vertex AI Express",
        }
    }
}

/// What a finished operation returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationOutcome {
    /// The video bytes were in the response.
    VideoInline,
    /// The video is downloadable from an HTTPS URI (recorded without its query string).
    VideoUri {
        /// Download URI without query or fragment.
        uri: String,
    },
    /// The video was written to Cloud Storage, which this pipeline cannot download.
    GcsUri {
        /// The `gs://` URI.
        uri: String,
    },
    /// The service filtered the result and returned no video.
    Filtered {
        /// The reasons the service gave.
        reason: String,
    },
    /// The operation failed.
    Failed {
        /// The error code the service gave, if any.
        code: Option<i64>,
        /// The error message, with the key redacted.
        message: String,
    },
}

impl OperationOutcome {
    /// The failure kind used by the "stop on a new kind of failure" rule, or `None` if this
    /// outcome is not a failure.
    #[must_use]
    pub fn failure_kind(&self) -> Option<String> {
        match self {
            Self::Failed { code, .. } => Some(match code {
                Some(code) => format!("failed:{code}"),
                None => "failed:none".to_string(),
            }),
            _ => None,
        }
    }

    /// Whether the outcome carries a video this pipeline can download.
    #[must_use]
    pub fn has_video(&self) -> bool {
        matches!(self, Self::VideoInline | Self::VideoUri { .. })
    }
}

/// Serializes an [`AnimState`] as its lowercase file-name stem (`idle`, `walk`, ...).
pub mod state_name {
    use serde::{Deserialize, Deserializer, Serializer};
    use sim_types::enums::AnimState;

    /// Writes the state's stem.
    ///
    /// # Errors
    /// Only the serializer's own errors.
    pub fn serialize<S: Serializer>(state: &AnimState, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(crate::veo_plan::state_slug(*state))
    }

    /// Reads a state's stem.
    ///
    /// # Errors
    /// A string that names no state.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<AnimState, D::Error> {
        let text = String::deserialize(deserializer)?;
        crate::veo_plan::parse_state(&text).map_err(serde::de::Error::custom)
    }
}

/// One ledger event. See the module docs for when each is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LedgerEvent {
    /// A paid start request is about to be sent.
    RequestStarted {
        /// `<run id>/<sequence>`, linking the events of one request.
        request_id: String,
        /// Catalog slug.
        slug: String,
        /// Animation state.
        #[serde(with = "state_name")]
        state: AnimState,
        /// Model identifier.
        model: String,
        /// API the request goes to.
        mode: ApiMode,
        /// SHA-256 of the prompt text, lowercase hex.
        prompt_sha256: String,
        /// The prompt text.
        prompt: String,
        /// SHA-256 of the keyframe image sent, if any.
        keyframe_sha256: Option<String>,
        /// SHA-256 of the end frame sent as `lastFrame`, if any. A pinned loop sends its start
        /// picture again, so this equals `keyframe_sha256`. Absent from lines written before
        /// end frames existed, and not written when no end frame was sent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_frame_sha256: Option<String>,
        /// The generation parameters sent.
        parameters: VeoParameters,
    },
    /// The service named the operation of a started request.
    OperationNamed {
        /// The request this operation belongs to.
        request_id: String,
        /// Catalog slug.
        slug: String,
        /// Animation state.
        #[serde(with = "state_name")]
        state: AnimState,
        /// Model the operation runs on, which polling needs.
        model: String,
        /// Operation resource name.
        operation: String,
    },
    /// An operation finished.
    OperationDone {
        /// Catalog slug.
        slug: String,
        /// Animation state.
        #[serde(with = "state_name")]
        state: AnimState,
        /// Operation resource name.
        operation: String,
        /// What it returned.
        outcome: OperationOutcome,
    },
    /// A clip was downloaded (or imported) and validated.
    Downloaded {
        /// Catalog slug.
        slug: String,
        /// Animation state.
        #[serde(with = "state_name")]
        state: AnimState,
        /// Operation that produced the clip.
        operation: String,
        /// File size in bytes.
        bytes: u64,
        /// SHA-256 of the file, lowercase hex.
        sha256: String,
        /// Container duration measured by ffprobe, in seconds.
        duration_seconds: f64,
        /// Video packets read by ffprobe.
        frames: u64,
        /// Frame width in pixels.
        width: u32,
        /// Frame height in pixels.
        height: u32,
        /// Where the clip was stored.
        path: String,
    },
    /// A start request was refused for quota. It cost nothing.
    QuotaRefused {
        /// The refused request.
        request_id: String,
        /// Catalog slug.
        slug: String,
        /// Animation state.
        #[serde(with = "state_name")]
        state: AnimState,
        /// HTTP status of the refusal.
        http_status: u16,
        /// The error's `status` field, such as `RESOURCE_EXHAUSTED`.
        status: Option<String>,
        /// The error's reason or message.
        reason: Option<String>,
    },
    /// A start request failed in a way that may have started paid work.
    StartFailed {
        /// The failed request.
        request_id: String,
        /// Catalog slug.
        slug: String,
        /// Animation state.
        #[serde(with = "state_name")]
        state: AnimState,
        /// The error, with the key redacted.
        error: String,
    },
    /// A reviewer rejected a downloaded clip.
    RejectedInReview {
        /// Catalog slug.
        slug: String,
        /// Animation state.
        #[serde(with = "state_name")]
        state: AnimState,
        /// SHA-256 of the rejected clip.
        sha256: String,
        /// Why it was rejected.
        reason: String,
    },
}

impl LedgerEvent {
    /// The clip this event concerns.
    #[must_use]
    pub fn clip(&self) -> (&str, AnimState) {
        match self {
            Self::RequestStarted { slug, state, .. }
            | Self::OperationNamed { slug, state, .. }
            | Self::OperationDone { slug, state, .. }
            | Self::Downloaded { slug, state, .. }
            | Self::QuotaRefused { slug, state, .. }
            | Self::StartFailed { slug, state, .. }
            | Self::RejectedInReview { slug, state, .. } => (slug, *state),
        }
    }
}

/// One line of the ledger: when and in which run an event happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerRecord {
    /// UTC time, `YYYY-MM-DDThh:mm:ssZ`.
    pub time_utc: String,
    /// The same time in seconds since the Unix epoch.
    pub unix_seconds: u64,
    /// Identifier of the run that wrote the line.
    pub run_id: String,
    /// The event.
    #[serde(flatten)]
    pub event: LedgerEvent,
}

/// An open ledger that appends records for one run.
#[derive(Debug)]
pub struct Ledger {
    path: PathBuf,
    run_id: String,
    file: std::fs::File,
    requests: u32,
}

impl Ledger {
    /// Opens `path` for appending, creating it and its directory if missing.
    ///
    /// # Errors
    /// [`LedgerError::Io`] if the file or directory cannot be created or opened.
    pub fn open(path: &Path, run_id: String) -> Result<Self, LedgerError> {
        let io = |source| LedgerError::Io {
            path: path.to_path_buf(),
            source,
        };
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(io)?;
        Ok(Self {
            path: path.to_path_buf(),
            run_id,
            file,
            requests: 0,
        })
    }

    /// The ledger file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The identifier written on every line of this run.
    #[must_use]
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// A fresh request identifier, `<run id>/<n>`, counting from 1 within the run.
    pub fn next_request_id(&mut self) -> String {
        self.requests += 1;
        format!("{}/{}", self.run_id, self.requests)
    }

    /// Appends `event`, stamped with the current time, as one line, and syncs it to disk.
    ///
    /// # Errors
    /// [`LedgerError::Encode`] or [`LedgerError::Io`].
    pub fn append(&mut self, event: LedgerEvent) -> Result<LedgerRecord, LedgerError> {
        let unix_seconds = now_unix_seconds();
        let record = LedgerRecord {
            time_utc: format_utc(unix_seconds),
            unix_seconds,
            run_id: self.run_id.clone(),
            event,
        };
        let mut line = serde_json::to_string(&record).map_err(LedgerError::Encode)?;
        line.push('\n');
        let io = |source| LedgerError::Io {
            path: self.path.clone(),
            source,
        };
        self.file.write_all(line.as_bytes()).map_err(io)?;
        self.file.sync_data().map_err(io)?;
        Ok(record)
    }
}

/// One ledger record with the one-based line number it was read from.
///
/// Provenance files cite the `downloaded` line of a clip by this number (plan D-6).
#[derive(Debug, Clone, PartialEq)]
pub struct NumberedRecord {
    /// One-based line number in the ledger file.
    pub line: usize,
    /// The record on that line.
    pub record: LedgerRecord,
}

/// Reads every record of the ledger at `path`; a missing file is an empty ledger.
///
/// # Errors
/// [`LedgerError::Io`] if the file exists but cannot be read, or [`LedgerError::Malformed`]
/// naming the first line that is not a record. Blank lines are skipped.
pub fn read_ledger(path: &Path) -> Result<Vec<LedgerRecord>, LedgerError> {
    Ok(read_ledger_numbered(path)?
        .into_iter()
        .map(|numbered| numbered.record)
        .collect())
}

/// Reads every record of the ledger at `path` with its line number, as [`read_ledger`].
///
/// # Errors
/// As [`read_ledger`].
pub fn read_ledger_numbered(path: &Path) -> Result<Vec<NumberedRecord>, LedgerError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(LedgerError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line)
                .map(|record| NumberedRecord {
                    line: index + 1,
                    record,
                })
                .map_err(|source| LedgerError::Malformed {
                    path: path.to_path_buf(),
                    line: index + 1,
                    source,
                })
        })
        .collect()
}

/// An operation that was named and has not reached a terminal result or a download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingOperation {
    /// Operation resource name.
    pub operation: String,
    /// Model it runs on.
    pub model: String,
}

/// A downloaded clip that has not been rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedClip {
    /// Where it was stored.
    pub path: PathBuf,
    /// Its SHA-256.
    pub sha256: String,
    /// The operation that produced it.
    pub operation: String,
}

/// Everything the ledger says about one clip (`slug`, `state`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipHistory {
    /// Start requests that may have cost a generation.
    pub paid_attempts: u32,
    /// Attempts that ended without a usable clip: failed, filtered, Cloud Storage only, an
    /// ambiguous start failure, or rejected in review.
    pub unusable_results: u32,
    /// The operation to resume, if one is outstanding.
    pub pending: Option<PendingOperation>,
    /// The latest download that was not rejected.
    pub downloaded: Option<DownloadedClip>,
    /// Every operation ever named for this clip.
    pub named_operations: BTreeSet<String>,
    /// SHA-256 of every clip downloaded for this clip.
    pub downloaded_sha256: BTreeSet<String>,
    /// SHA-256 of every clip rejected in review.
    pub rejected_sha256: BTreeSet<String>,
}

/// What a ledger says, folded per clip. See the module docs.
#[derive(Debug, Clone, Default)]
pub struct LedgerHistory {
    clips: BTreeMap<(String, AnimState), ClipHistory>,
    failure_kinds: BTreeSet<String>,
    /// `(unix seconds, request id)` of every start request.
    starts: Vec<(u64, String)>,
    quota_refused: BTreeSet<String>,
}

impl LedgerHistory {
    /// Folds `records` in order.
    #[must_use]
    pub fn from_records(records: &[LedgerRecord]) -> Self {
        let mut history = Self::default();
        for record in records {
            history.apply(record);
        }
        history
    }

    /// Reads and folds the ledger at `path`.
    ///
    /// # Errors
    /// As [`read_ledger`].
    pub fn load(path: &Path) -> Result<Self, LedgerError> {
        Ok(Self::from_records(&read_ledger(path)?))
    }

    fn apply(&mut self, record: &LedgerRecord) {
        let (slug, state) = record.event.clip();
        let clip = self.clips.entry((slug.to_string(), state)).or_default();
        match &record.event {
            LedgerEvent::RequestStarted { request_id, .. } => {
                clip.paid_attempts += 1;
                self.starts.push((record.unix_seconds, request_id.clone()));
            }
            LedgerEvent::OperationNamed {
                operation, model, ..
            } => {
                clip.named_operations.insert(operation.clone());
                clip.pending = Some(PendingOperation {
                    operation: operation.clone(),
                    model: model.clone(),
                });
            }
            LedgerEvent::OperationDone { outcome, .. } => {
                if !outcome.has_video() {
                    clip.pending = None;
                    clip.unusable_results += 1;
                }
                if let Some(kind) = outcome.failure_kind() {
                    self.failure_kinds.insert(kind);
                }
            }
            LedgerEvent::Downloaded {
                sha256,
                path,
                operation,
                ..
            } => {
                clip.pending = None;
                clip.downloaded_sha256.insert(sha256.clone());
                if !clip.rejected_sha256.contains(sha256) {
                    clip.downloaded = Some(DownloadedClip {
                        path: PathBuf::from(path),
                        sha256: sha256.clone(),
                        operation: operation.clone(),
                    });
                }
            }
            LedgerEvent::QuotaRefused { request_id, .. } => {
                clip.paid_attempts = clip.paid_attempts.saturating_sub(1);
                self.quota_refused.insert(request_id.clone());
            }
            LedgerEvent::StartFailed { .. } => clip.unusable_results += 1,
            LedgerEvent::RejectedInReview { sha256, .. } => {
                clip.rejected_sha256.insert(sha256.clone());
                clip.unusable_results += 1;
                if clip
                    .downloaded
                    .as_ref()
                    .is_some_and(|d| &d.sha256 == sha256)
                {
                    clip.downloaded = None;
                }
            }
        }
    }

    /// The history of one clip, if the ledger mentions it.
    #[must_use]
    pub fn clip(&self, slug: &str, state: AnimState) -> Option<&ClipHistory> {
        self.clips.get(&(slug.to_string(), state))
    }

    /// Whether a failure of `kind` (see [`OperationOutcome::failure_kind`]) was recorded.
    #[must_use]
    pub fn knows_failure(&self, kind: &str) -> bool {
        self.failure_kinds.contains(kind)
    }

    /// Generations started at or after `since_unix_seconds`, excluding starts the service
    /// refused for quota (those cost nothing).
    #[must_use]
    pub fn generations_since(&self, since_unix_seconds: u64) -> u32 {
        let count = self
            .starts
            .iter()
            .filter(|(time, id)| *time >= since_unix_seconds && !self.quota_refused.contains(id))
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }
}

/// Seconds since the Unix epoch now, or 0 if the clock is before it.
#[must_use]
pub fn now_unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Formats Unix seconds as `YYYY-MM-DDThh:mm:ssZ` in UTC (proleptic Gregorian calendar).
#[must_use]
pub fn format_utc(unix_seconds: u64) -> String {
    let days = unix_seconds / SECONDS_PER_DAY;
    let rem = unix_seconds % SECONDS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Converts days since 1970-01-01 to a (year, month, day) civil date.
///
/// Howard Hinnant's `civil_from_days`, restricted to dates on or after the epoch so that all
/// arithmetic is unsigned. Eras are 400-year cycles of 146,097 days.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

/// A run identifier: the UTC start time and the process id, `YYYYMMDDThhmmssZ-<pid>`.
#[must_use]
pub fn new_run_id() -> String {
    let stamp: String = format_utc(now_unix_seconds())
        .chars()
        .filter(|c| !matches!(c, '-' | ':'))
        .collect();
    format!("{stamp}-{}", std::process::id())
}

/// SHA-256 of `bytes` as lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write as _;
            // Writing to a String cannot fail.
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// SHA-256 of the file at `path` as lowercase hex.
///
/// # Errors
/// [`LedgerError::Io`] if the file cannot be read.
pub fn sha256_file(path: &Path) -> Result<String, LedgerError> {
    let bytes = std::fs::read(path).map_err(|source| LedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(sha256_hex(&bytes))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn utc_formatting_matches_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_utc(1_709_251_199), "2024-02-29T23:59:59Z");
        assert_eq!(format_utc(1_793_750_400), "2026-11-04T00:00:00Z");
        assert_eq!(format_utc(4_102_444_800), "2100-01-01T00:00:00Z");
    }

    #[test]
    fn sha256_matches_the_fips_test_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn failure_kinds_distinguish_codes() {
        let failed = |code| OperationOutcome::Failed {
            code,
            message: String::new(),
        };
        assert_eq!(
            failed(Some(13)).failure_kind().as_deref(),
            Some("failed:13")
        );
        assert_eq!(failed(None).failure_kind().as_deref(), Some("failed:none"));
        assert_eq!(OperationOutcome::VideoInline.failure_kind(), None);
        assert!(!OperationOutcome::GcsUri { uri: String::new() }.has_video());
    }
}
