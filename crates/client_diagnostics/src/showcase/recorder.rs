//! Frame transport of a take: the reorder buffer, the lossless mezzanine and the take manifest.
//!
//! # Responsibility
//! Plan `docs/plans/2026-11-05-ci-showcase-video.md`, decision D-8. Screenshot readbacks arrive
//! asynchronously; [`FrameOrderBuffer`] releases them strictly by frame index and fails on a gap, a
//! duplicate or an overflow. [`Mezzanine`] streams the released RGB frames into an `ffmpeg` child
//! that writes a lossless FFV1 file. [`TakeManifest`] records each frame's tick, simulation checksum
//! and BLAKE3 hash.
//!
//! # Invariants
//! - No frame is dropped or reordered silently: any irregularity is a [`FrameOrderFault`].
//! - The mezzanine holds exactly the engine's pixels; every published file is cut from it.
//! - Wall-clock times in the manifest are reported only; nothing is controlled by them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

/// Most frames the reorder buffer holds while waiting for an earlier one.
pub const REORDER_CAPACITY: usize = 16;

/// Version of the take manifest schema.
pub const MANIFEST_VERSION: u32 = 1;

/// Irregular frame order.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameOrderFault {
    /// A frame arrived twice, or after it was already released.
    #[error("frame {0} arrived twice")]
    Duplicate(u64),
    /// More than [`REORDER_CAPACITY`] frames wait for an earlier one.
    #[error("more than {REORDER_CAPACITY} frames wait for frame {0}")]
    Overflow(u64),
    /// The take ended with frames missing.
    #[error("the take ended with {released} of {expected} frames released")]
    Gap {
        /// Frames released in order.
        released: u64,
        /// Frames the take recorded.
        expected: u64,
    },
}

// PERFORMANCE RATIONALE:
// Why: readbacks of a 1280x720 frame are 2.8 MB and arrive out of order by a few frames.
// What: a bounded map keyed by frame index holds at most REORDER_CAPACITY waiting frames and hands
// each one on as soon as its predecessors were written.
// Trade-off: at most about 44 MB held, in exchange for strictly ordered, never dropped frames.
/// Releases payloads strictly in frame order.
#[derive(Debug)]
pub struct FrameOrderBuffer<T> {
    next: u64,
    waiting: BTreeMap<u64, T>,
}

impl<T> Default for FrameOrderBuffer<T> {
    fn default() -> Self {
        Self {
            next: 0,
            waiting: BTreeMap::new(),
        }
    }
}

impl<T> FrameOrderBuffer<T> {
    /// Accepts frame `index` and returns every frame that can now be released, in order.
    ///
    /// # Errors
    /// [`FrameOrderFault::Duplicate`] or [`FrameOrderFault::Overflow`].
    pub fn push(&mut self, index: u64, payload: T) -> Result<Vec<(u64, T)>, FrameOrderFault> {
        if index < self.next || self.waiting.contains_key(&index) {
            return Err(FrameOrderFault::Duplicate(index));
        }
        if self.waiting.len() >= REORDER_CAPACITY {
            return Err(FrameOrderFault::Overflow(self.next));
        }
        self.waiting.insert(index, payload);
        let mut released = Vec::new();
        while let Some(payload) = self.waiting.remove(&self.next) {
            released.push((self.next, payload));
            self.next += 1;
        }
        Ok(released)
    }

    /// Frames released so far.
    #[must_use]
    pub const fn released(&self) -> u64 {
        self.next
    }

    /// Checks that exactly `expected` frames were released and none waits.
    ///
    /// # Errors
    /// [`FrameOrderFault::Gap`].
    pub fn finish(&self, expected: u64) -> Result<(), FrameOrderFault> {
        if self.next != expected || !self.waiting.is_empty() {
            return Err(FrameOrderFault::Gap {
                released: self.next,
                expected,
            });
        }
        Ok(())
    }
}

/// Failure of the mezzanine encoder.
#[derive(Debug, thiserror::Error)]
pub enum MezzanineError {
    /// `ffmpeg` could not be started.
    #[error("cannot start ffmpeg for {path}: {source}")]
    Spawn {
        /// The mezzanine.
        path: PathBuf,
        /// The error.
        #[source]
        source: std::io::Error,
    },
    /// A frame could not be written to `ffmpeg`.
    #[error("cannot write frame {index} to ffmpeg: {source}")]
    Write {
        /// The frame.
        index: u64,
        /// The error.
        #[source]
        source: std::io::Error,
    },
    /// A frame has the wrong number of bytes.
    #[error("frame {index} has {bytes} bytes, expected {expected}")]
    FrameSize {
        /// The frame.
        index: u64,
        /// Its size.
        bytes: usize,
        /// `width × height × 3`.
        expected: usize,
    },
    /// `ffmpeg` failed.
    #[error("ffmpeg failed writing {path}: {status}")]
    Failed {
        /// The mezzanine.
        path: PathBuf,
        /// Its exit status and the end of its log.
        status: String,
    },
    /// The output exists already.
    #[error("refusing to overwrite {0}")]
    Exists(PathBuf),
}

/// An `ffmpeg` child writing raw RGB frames into a lossless FFV1 Matroska file.
#[derive(Debug)]
pub struct Mezzanine {
    path: PathBuf,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    frame_bytes: usize,
}

/// The arguments of the mezzanine encoder for `width`×`height` frames at `rate` fps.
#[must_use]
pub fn mezzanine_arguments(width: u32, height: u32, rate: u32, path: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgb24".into(),
        "-s".into(),
        format!("{width}x{height}"),
        "-r".into(),
        rate.to_string(),
        "-i".into(),
        "-".into(),
        "-c:v".into(),
        "ffv1".into(),
        "-level".into(),
        "3".into(),
        "-g".into(),
        "1".into(),
        "-n".into(),
        path.to_string_lossy().into_owned(),
    ]
}

impl Mezzanine {
    /// Starts `ffmpeg` writing `path`.
    ///
    /// # Errors
    /// [`MezzanineError::Exists`] or [`MezzanineError::Spawn`].
    pub fn start(path: &Path, width: u32, height: u32, rate: u32) -> Result<Self, MezzanineError> {
        if path.exists() {
            return Err(MezzanineError::Exists(path.to_owned()));
        }
        let mut child = Command::new("ffmpeg")
            .args(mezzanine_arguments(width, height, rate, path))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| MezzanineError::Spawn {
                path: path.to_owned(),
                source,
            })?;
        let stdin = child.stdin.take();
        Ok(Self {
            path: path.to_owned(),
            child: Some(child),
            stdin,
            frame_bytes: width as usize * height as usize * 3,
        })
    }

    /// Writes one RGB frame.
    ///
    /// # Errors
    /// [`MezzanineError::FrameSize`] or [`MezzanineError::Write`].
    pub fn write(&mut self, index: u64, rgb: &[u8]) -> Result<(), MezzanineError> {
        if rgb.len() != self.frame_bytes {
            return Err(MezzanineError::FrameSize {
                index,
                bytes: rgb.len(),
                expected: self.frame_bytes,
            });
        }
        let stdin = self.stdin.as_mut().ok_or_else(|| MezzanineError::Write {
            index,
            source: std::io::Error::from(std::io::ErrorKind::BrokenPipe),
        })?;
        stdin
            .write_all(rgb)
            .map_err(|source| MezzanineError::Write { index, source })
    }

    /// Closes the input and waits for `ffmpeg`.
    ///
    /// # Errors
    /// [`MezzanineError::Failed`] if it exits unsuccessfully.
    pub fn finish(mut self) -> Result<(), MezzanineError> {
        drop(self.stdin.take());
        let child = self.child.take().ok_or_else(|| MezzanineError::Failed {
            path: self.path.clone(),
            status: "already finished".into(),
        })?;
        let output = child
            .wait_with_output()
            .map_err(|error| MezzanineError::Failed {
                path: self.path.clone(),
                status: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(MezzanineError::Failed {
                path: self.path.clone(),
                status: format!(
                    "{} {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        Ok(())
    }

    /// Stops `ffmpeg` after a failure; the incomplete file is left for inspection.
    pub fn abort(mut self) {
        drop(self.stdin.take());
        if let Some(mut child) = self.child.take() {
            if let Err(error) = child.kill() {
                bevy::log::warn!("ffmpeg already stopped: {error}");
            }
            if let Err(error) = child.wait() {
                bevy::log::warn!("cannot reap ffmpeg: {error}");
            }
        }
    }
}

/// The adapter that rendered the take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AdapterRecord {
    /// Adapter name, e.g. `llvmpipe (LLVM 19.1.7, 256 bits)`.
    pub name: String,
    /// Driver name.
    pub driver: String,
    /// Driver details, e.g. the Mesa release.
    pub driver_info: String,
    /// Graphics API.
    pub backend: String,
}

/// One scene as rendered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRecord {
    /// Scene name.
    pub name: String,
    /// First frame of the scene in the take.
    pub first_frame: u64,
    /// Frames of the scene.
    pub frames: u64,
    /// Simulation tick shown by its first frame.
    pub start_tick: u32,
}

/// One frame as rendered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameRecord {
    /// Frame index in the take.
    pub index: u64,
    /// Simulation tick shown.
    pub tick: u32,
    /// Simulation checksum at that tick.
    pub checksum: u64,
    /// BLAKE3 of the frame's RGB bytes, lowercase hexadecimal.
    pub blake3: String,
    /// Wall time between this frame and the previous one, milliseconds (reported only).
    pub wall_ms: f32,
}

/// The manifest of one take, written next to its mezzanine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeManifest {
    /// Schema version, [`MANIFEST_VERSION`].
    pub format_version: u32,
    /// Whether every frame was written and `ffmpeg` succeeded.
    pub complete: bool,
    /// Why the take failed, if it did.
    pub failure: Option<String>,
    /// Take name.
    pub take: String,
    /// Storyboard file.
    pub storyboard: PathBuf,
    /// Scenario file.
    pub scenario: PathBuf,
    /// Mezzanine file name, next to the manifest.
    pub mezzanine: String,
    /// Frame size, `(width, height)`.
    pub resolution: (u32, u32),
    /// Frames per second.
    pub frame_rate: u32,
    /// The adapter.
    pub adapter: AdapterRecord,
    /// `SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY`, if set (plan decision D-12, Q0009).
    pub anisotropy_override: Option<String>,
    /// Commit rendered.
    pub revision: String,
    /// Commit date.
    pub revision_date: String,
    /// Seconds from start to the first recorded frame.
    pub readiness_seconds: f32,
    /// Seconds from the first recorded frame to the end of the take.
    pub render_seconds: f32,
    /// Scenes in order.
    pub scenes: Vec<SceneRecord>,
    /// Frames in order.
    pub frames: Vec<FrameRecord>,
}

impl TakeManifest {
    /// The manifest path of take `name` in `directory`.
    #[must_use]
    pub fn path(directory: &Path, name: &str) -> PathBuf {
        directory.join(format!("{name}.manifest.ron"))
    }

    /// Writes the manifest as RON.
    ///
    /// # Errors
    /// The I/O or serialisation error, as text.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|error| error.to_string())?;
        std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
    }

    /// Reads a manifest.
    ///
    /// # Errors
    /// The I/O or parse error, as text.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        ron::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    /// The scene named `name`.
    #[must_use]
    pub fn scene(&self, name: &str) -> Option<&SceneRecord> {
        self.scenes.iter().find(|scene| scene.name == name)
    }

    /// Median and 95th percentile of the frame wall times, milliseconds, or `None` without frames.
    #[must_use]
    pub fn wall_percentiles(&self) -> Option<(f32, f32)> {
        let mut times: Vec<f32> = self.frames.iter().map(|frame| frame.wall_ms).collect();
        if times.is_empty() {
            return None;
        }
        times.sort_by(f32::total_cmp);
        let at = |fraction: f32| {
            #[expect(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "frame counts are small and the product is clamped to the index range"
            )]
            let index = ((times.len() - 1) as f32 * fraction).round() as usize;
            times[index.min(times.len() - 1)]
        };
        Some((at(0.5), at(0.95)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_order_buffer_rejects_gaps_duplicates_and_overflow() {
        let mut buffer = FrameOrderBuffer::default();
        assert_eq!(buffer.push(1, 'b'), Ok(vec![]));
        assert_eq!(buffer.push(0, 'a'), Ok(vec![(0, 'a'), (1, 'b')]));
        assert_eq!(buffer.push(1, 'x'), Err(FrameOrderFault::Duplicate(1)));
        assert_eq!(buffer.push(3, 'd'), Ok(vec![]));
        assert_eq!(buffer.push(3, 'd'), Err(FrameOrderFault::Duplicate(3)));
        assert_eq!(
            buffer.finish(4),
            Err(FrameOrderFault::Gap {
                released: 2,
                expected: 4
            })
        );
        assert_eq!(buffer.push(2, 'c'), Ok(vec![(2, 'c'), (3, 'd')]));
        assert_eq!(buffer.finish(4), Ok(()));
        assert_eq!(
            buffer.finish(5),
            Err(FrameOrderFault::Gap {
                released: 4,
                expected: 5
            })
        );
        let mut full = FrameOrderBuffer::default();
        for index in 1..=REORDER_CAPACITY as u64 {
            assert_eq!(full.push(index, ()), Ok(vec![]));
        }
        assert_eq!(full.push(99, ()), Err(FrameOrderFault::Overflow(0)));
        assert_eq!(full.released(), 0);
    }

    #[test]
    fn mezzanine_arguments_are_lossless_and_never_overwrite() {
        let arguments = mezzanine_arguments(480, 270, 25, Path::new("/tmp/a.mkv"));
        let joined = arguments.join(" ");
        assert!(joined.contains("-f rawvideo -pix_fmt rgb24 -s 480x270 -r 25 -i -"));
        assert!(joined.contains("-c:v ffv1 -level 3 -g 1"));
        assert!(joined.ends_with("-n /tmp/a.mkv"));
    }

    #[test]
    fn wall_percentiles_pick_median_and_tail() {
        let frame = |wall_ms| FrameRecord {
            index: 0,
            tick: 0,
            checksum: 0,
            blake3: String::new(),
            wall_ms,
        };
        let manifest = TakeManifest {
            format_version: MANIFEST_VERSION,
            complete: true,
            failure: None,
            take: "t".into(),
            storyboard: PathBuf::new(),
            scenario: PathBuf::new(),
            mezzanine: "t.mkv".into(),
            resolution: (320, 180),
            frame_rate: 25,
            adapter: AdapterRecord::default(),
            anisotropy_override: None,
            revision: String::new(),
            revision_date: String::new(),
            readiness_seconds: 0.0,
            render_seconds: 0.0,
            scenes: vec![],
            frames: (1..=21_u16).map(|value| frame(f32::from(value))).collect(),
        };
        assert_eq!(manifest.wall_percentiles(), Some((11.0, 20.0)));
    }
}
