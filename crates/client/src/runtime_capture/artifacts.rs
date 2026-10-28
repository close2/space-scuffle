//! Fresh GPU readback artifacts and auditable, atomic capture manifests.
//!
//! Image metrics reject blank gameplay regions, never certify artistic quality.
//! Success is recorded only after acknowledged readback, validation and PNG writes.

use super::config::{CaptureError, Goal};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub(super) struct ImageMetrics {
    pub luminance_range: u8,
    pub occupied_rgb_bins: usize,
    pub samples: usize,
}

pub(super) fn measure(
    image: &image::RgbImage,
    region: [f32; 4],
) -> Result<ImageMetrics, CaptureError> {
    let [x, y, w, h] = region;
    let (width, height) = image.dimensions();
    let bounds = [
        (x * width as f32) as u32,
        (y * height as f32) as u32,
        ((x + w) * width as f32) as u32,
        ((y + h) * height as f32) as u32,
    ];
    let mut occupied = [false; 512];
    let mut low = u8::MAX;
    let mut high = 0;
    let mut samples = 0;
    for row in bounds[1]..bounds[3].min(height) {
        for col in bounds[0]..bounds[2].min(width) {
            let [r, g, b] = image.get_pixel(col, row).0;
            let luminance =
                ((u32::from(r) * 54 + u32::from(g) * 183 + u32::from(b) * 19) >> 8) as u8;
            low = low.min(luminance);
            high = high.max(luminance);
            occupied[usize::from(r >> 5) * 64 + usize::from(g >> 5) * 8 + usize::from(b >> 5)] =
                true;
            samples += 1;
        }
    }
    let metrics = ImageMetrics {
        luminance_range: high.saturating_sub(low),
        occupied_rgb_bins: occupied.into_iter().filter(|v| *v).count(),
        samples,
    };
    // Corruption/blank-frame thresholds: at least 8/256 luminance spread and
    // four occupied 3-bit RGB bins. These are not aesthetic acceptance scores.
    if samples == 0 || metrics.luminance_range < 8 || metrics.occupied_rgb_bins < 4 {
        return Err(CaptureError::Readback(format!(
            "blank gameplay region: {metrics:?}"
        )));
    }
    Ok(metrics)
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct RampSample {
    pub tick: u32,
    pub cell: [u32; 2],
    pub level: u8,
    pub ramp: bool,
    pub position_raw: [i64; 2],
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct Record {
    pub shell_audit: Option<PathBuf>,
    pub shell_fixture: Option<crate::shell_order_audit::FixtureSpec>,
    pub name: String,
    pub goal: Goal,
    pub tick: u32,
    pub presented_tick: u32,
    pub session_generation: u64,
    pub command_outcomes: u64,
    pub combat_tick: Option<u32>,
    /// Generation of retained historical combat evidence, distinct from this view.
    pub combat_generation: Option<u64>,
    /// Generation of retained ramp samples, including when later reset views retain history.
    pub ramp_generation: Option<u64>,
    pub checksum: u64,
    pub virtual_seconds: f64,
    pub camera_target: [f32; 2],
    pub camera_distance: f32,
    pub camera_translation: [f32; 3],
    pub subject: Option<u32>,
    pub subject_pixel: Option<[f32; 2]>,
    pub ramp_entry_level: Option<u8>,
    pub ramp_exit_level: Option<u8>,
    pub ramp_trace: Vec<RampSample>,
    pub combat_source: Option<u32>,
    pub entities: usize,
    pub lights: usize,
    pub combat_effects: usize,
    pub source_pixel: Option<[f32; 2]>,
    pub visible_units: usize,
    pub scene_roots: usize,
    pub terrain_chunks: usize,
    pub required_shader_mask: u8,
    pub ready_pipeline_counts: [usize; 4],
    pub image: PathBuf,
    pub pixels: Option<ImageMetrics>,
    pub transport: Option<super::transport::TransportProof>,
    pub ownership: Option<super::ownership::OwnershipProof>,
}

#[derive(Serialize)]
pub(super) struct Manifest<'a> {
    pub format_version: u32,
    pub complete: bool,
    pub failure: Option<&'a str>,
    pub scenario: &'a Path,
    pub script: &'a Path,
    pub assets_root: &'a Path,
    pub resolution: [u32; 2],
    pub audio_policy: crate::presentation::AudioPolicy,
    pub shader_paths: [&'static str; 4],
    pub checkpoints: &'a [Record],
    pub control_contract: &'static str,
}

pub(super) fn write_manifest(
    directory: &Path,
    manifest: &Manifest<'_>,
) -> Result<(), CaptureError> {
    let path = directory.join("manifest.ron");
    let temporary = directory.join("manifest.pending");
    let content = ron::ser::to_string_pretty(manifest, ron::ser::PrettyConfig::default())
        .map_err(|error| CaptureError::Verification(error.to_string()))?;
    std::fs::write(&temporary, content).map_err(|source| CaptureError::Io {
        path: temporary.clone(),
        source,
    })?;
    std::fs::rename(&temporary, &path).map_err(|source| CaptureError::Io { path, source })
}

#[cfg(test)]
mod tests {
    use super::measure;

    #[test]
    fn gameplay_region_rejects_blank_pixels_even_when_ui_region_has_detail() {
        let mut image = image::RgbImage::from_pixel(64, 64, image::Rgb([8, 8, 8]));
        for y in 0..64 {
            for x in 0..32 {
                image.put_pixel(
                    x,
                    y,
                    if (x + y) % 2 == 0 {
                        image::Rgb([255, 255, 255])
                    } else {
                        image::Rgb([20, 100, 220])
                    },
                );
            }
        }
        assert!(measure(&image, [0.5, 0.0, 0.5, 1.0]).is_err());
        for y in 0..64 {
            for x in 32..64 {
                image.put_pixel(x, y, image::Rgb([(x * 4) as u8, (y * 4) as u8, 80]));
            }
        }
        let metrics = measure(&image, [0.5, 0.0, 0.5, 1.0]);
        assert!(
            metrics.is_ok(),
            "synthetic pixels test metric algorithm only"
        );
    }
}
