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

#![allow(dead_code, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

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
