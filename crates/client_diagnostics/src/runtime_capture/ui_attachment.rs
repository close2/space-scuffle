//! Connects the existing egui renderer to the camera's actual screenshot attachment.
//!
//! Bevy 0.15 redirects camera output for screenshots, while bevy_egui 0.33 reads
//! the extracted window view directly. The capture-only adapter shares that output
//! view; the acquired native surface texture and production UI pipeline stay intact.
//! Window extraction drops this borrowed view at the start of the following frame.

use super::pipelines::PipelineEvidence;
use bevy::prelude::*;
use bevy::render::camera::{ExtractedCamera, NormalizedRenderTarget};
use bevy::render::view::{ExtractedWindows, ViewTarget};

pub(super) fn align_ui_attachment(
    cameras: Query<'_, '_, (&ExtractedCamera, &ViewTarget)>,
    mut windows: ResMut<'_, ExtractedWindows>,
    evidence: Res<'_, PipelineEvidence>,
) {
    for (window_id, window) in &mut windows.windows {
        let mut matching = cameras.iter().filter(|(camera, _)| {
            matches!(&camera.target, Some(NormalizedRenderTarget::Window(target)) if target.entity() == *window_id)
        });
        let Some((_, target)) = matching.next() else {
            // The first extraction can precede camera creation; readiness cannot
            // pass before a real camera, propagated transforms and subjects exist.
            continue;
        };
        if matching.next().is_some() {
            evidence.report_failure("ambiguous window camera mapping for capture UI attachment");
            return;
        }
        window.swap_chain_texture_view = Some(target.out_texture().clone());
    }
}
