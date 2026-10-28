//! Capture-only measurements of projected real shell and media pixels.
//!
//! Ray/sphere intersections exclude background and terrain-hidden dome pixels. Statistics
//! describe display-encoded screenshots, not scene radiance or automatic artistic acceptance.

use bevy::prelude::*;
use sim_types::ShapeKind;
use std::fmt::Write;

/// Computes per-shell projected bounds, saturation and inner-media luminance variation.
///
/// Sampling excludes the outer two percent of the silhouette to avoid antialiased terrain.
/// Overlapping shells are evaluated in close captures; gameplay images require manual review.
pub(super) fn measure(
    pixels: &image::RgbImage,
    camera: &Camera,
    transform: &GlobalTransform,
    shells: &[(Vec3, f32, ShapeKind)],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::from("index,x0,y0,x1,y1,shell_pixels,near_white_pixels,white_fraction,inner_pixels,inner_luminance_stddev\n");
    for (index, &(center, radius, shape)) in shells.iter().enumerate() {
        let screen = camera
            .world_to_viewport(transform, center)
            .map_err(|error| format!("shell center projection failed: {error:?}"))?;
        let edge = camera
            .world_to_viewport(transform, center + transform.right() * radius)
            .map_err(|error| format!("shell radius projection failed: {error:?}"))?;
        let projected_radius = screen.distance(edge);
        let bounds_radius = projected_radius * 1.1;
        let x0 = (screen.x - bounds_radius).max(0.0) as u32;
        let x1 = ((screen.x + bounds_radius).ceil() as u32).min(pixels.width());
        let y0 = (screen.y - bounds_radius).max(0.0) as u32;
        let y1 = ((screen.y + bounds_radius).ceil() as u32).min(pixels.height());
        let mut shell_pixels = 0_u32;
        let mut white_pixels = 0_u32;
        let mut inner_pixels = 0_u32;
        let mut mean = 0.0_f64;
        let mut squared_deviation = 0.0_f64;
        for y in y0..y1 {
            for x in x0..x1 {
                let sample = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let ray = camera
                    .viewport_to_world(transform, sample)
                    .map_err(|error| format!("pixel ray projection failed: {error:?}"))?;
                let relative = ray.origin - center;
                let along = relative.dot(*ray.direction);
                let discriminant = along * along - (relative.length_squared() - radius * radius);
                if discriminant < 0.0 {
                    continue;
                }
                let point = ray.origin + ray.direction * (-along - discriminant.sqrt());
                if matches!(shape, ShapeKind::BuildingDome { .. }) && point.y < center.y {
                    continue;
                }
                let radial = if shape == ShapeKind::UnitSphere {
                    sample.distance(screen) / projected_radius
                } else {
                    let normal = (point - center) / radius;
                    let facing = normal.dot(-*ray.direction).clamp(0.0, 1.0);
                    (1.0 - facing * facing).sqrt()
                };
                let rgb = pixels.get_pixel(x, y).0;
                if (0.8..0.98).contains(&radial) {
                    shell_pixels += 1;
                    if rgb.iter().all(|value| *value >= 250) {
                        white_pixels += 1;
                    }
                } else if radial < 0.65 {
                    inner_pixels += 1;
                    let luminance = f64::from(rgb[0]) * 0.2126
                        + f64::from(rgb[1]) * 0.7152
                        + f64::from(rgb[2]) * 0.0722;
                    let delta = luminance - mean;
                    mean += delta / f64::from(inner_pixels);
                    squared_deviation += delta * (luminance - mean);
                }
            }
        }
        let white_fraction = f64::from(white_pixels) / f64::from(shell_pixels.max(1));
        let deviation = (squared_deviation / f64::from(inner_pixels.max(1))).sqrt();
        writeln!(output, "{index},{x0},{y0},{x1},{y1},{shell_pixels},{white_pixels},{white_fraction:.6},{inner_pixels},{deviation:.6}")?;
    }
    Ok(output)
}
