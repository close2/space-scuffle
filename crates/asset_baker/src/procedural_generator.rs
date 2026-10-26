//! Pure-Rust procedural animation synthesizer for holographic unit bubbles and building domes.
//!
//! This module provides a complete, deterministic, zero-dependency offline rendering engine
//! that generates stylized, high-contrast sprite animation sequences for all starter units
//! and structures. The resulting vertical strips and `.flipbook.ron` manifests ensure
//! that Space Scuffle possesses a complete visual presentation without external video assets.
//!
//! # Archetypes Implemented:
//! - **Mobile Units (192x192, 8 frames/state, 15 FPS):**
//!   - `Marine`: Breathing visor (Idle), marching stride (Walk), muzzle blast & recoil (Attack), static dissolution (Die).
//!   - `Marauder`: Hydraulic vent pulse (Idle), heavy stride (Walk), dual concussive burst (Attack), core overload (Die).
//!   - `Zealot`: Psionic blade hum (Idle), forward dash aura (Walk), cross-slashing psi-arcs (Attack), psi dissipation (Die).
//!   - `SiegeTank`: Sweeping turret scan (Idle), tread rotation (Walk), heavy cannon recoil blast (Attack), chassis rupture (Die).
//!   - `SkyScout`: Hover bob & anti-grav ring (Idle), forward bank & plasma trails (Walk), twin lasers (Attack), spiraling collapse (Die).
//!   - `Worker`: Fusion drill spin (Idle), tilted flight (Walk), welding electric spark arc (Attack), pop & fizzle (Die).
//! - **Building Domes (256x256, 12 frames/state, 15 FPS):**
//!   - `CommandDome`: Wireframe scanline rise (Constructing), 360° radar beacon (Idle), robotic arm welder (Cast), implosion (Die).
//!   - `BarracksDome`: Pylons raising (Constructing), glowing barracks bay lights (Idle), hologram unit synthesis (Cast), collapse (Die).
//!   - `SupplyDome`: Telescoping dome rings (Constructing), rotating energy prism (Idle), core containment rupture (Die).
//!   - `LaserTurretDome`: Barrel mount (Constructing), sweeping green targeting laser (Idle), continuous red beam discharge (Attack), blowout (Die).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::many_single_char_names,
    clippy::too_many_lines
)]

use crate::error::BakerError;
use crate::manifest::{ActionSlice, FlipbookAtlasMeta, FlipbookLayout};
use image::{Rgba, RgbaImage};
use sim_core::{UnitArchetype, UnitCatalog};
use sim_types::enums::{AnimState, ShapeKind, SilhouetteClass};
use std::collections::BTreeMap;
use std::f32::consts::PI;

/// Container holding the generated metadata and packed image for a procedural atlas.
#[derive(Debug, Clone)]
pub struct ProceduralAtlasOutput {
    /// Generated and validated flipbook atlas metadata.
    pub meta: FlipbookAtlasMeta,
    /// Assembled vertical strip image containing all animation frames stacked on the Y axis.
    pub image: RgbaImage,
}

fn resolve_archetype<'a>(
    name: &str,
    catalog: &'a UnitCatalog,
) -> Result<&'a UnitArchetype, BakerError> {
    let clean = name.to_lowercase().replace(['-', ' '], "_");
    if let Some(arch) = catalog
        .get_by_slug(&clean)
        .or_else(|| catalog.get_by_name(&clean))
    {
        return Ok(arch);
    }
    let clean_alnum: String = clean.chars().filter(|c| c.is_alphanumeric()).collect();
    let words: Vec<&str> = clean.split('_').filter(|w| !w.is_empty()).collect();
    if let Some(arch) = catalog.archetypes().find(|a| {
        let slug_alnum: String = a.slug.chars().filter(|c| c.is_alphanumeric()).collect();
        if slug_alnum == clean_alnum {
            return true;
        }
        let display_lower = a.display_name.to_lowercase();
        let sc2_lower = a.sc2_inspiration.as_deref().unwrap_or("").to_lowercase();
        let sc2_alnum: String = sc2_lower.chars().filter(|c| c.is_alphanumeric()).collect();
        if !clean_alnum.is_empty() && sc2_alnum.contains(&clean_alnum) {
            return true;
        }
        if clean_alnum == "worker" && a.capabilities.harvester.is_some() {
            return true;
        }
        if !words.is_empty()
            && words
                .iter()
                .all(|&w| sc2_lower.contains(w) || display_lower.contains(w) || a.slug.contains(w))
        {
            return true;
        }
        false
    }) {
        return Ok(arch);
    }
    Err(BakerError::UnknownArchetype {
        name: name.to_string(),
        valid: catalog.archetypes().map(|a| a.slug.clone()).collect(),
    })
}

/// Generates a procedural vertical strip atlas for a mobile unit archetype.
///
/// Mobile units use 8 frames per animation state (`Idle`, `Walk`, `Attack`, `Die`).
/// Default resolution is 192x192 at 15 FPS.
///
/// # Errors
/// Returns [`BakerError::UnknownArchetype`] if the archetype is not a recognized mobile unit,
/// or [`BakerError::ValidationFailed`] if metadata invariants fail.
pub fn generate_procedural_unit_atlas(
    unit_name: &str,
    frame_size: u32,
    fps: u32,
) -> Result<ProceduralAtlasOutput, BakerError> {
    let catalog = UnitCatalog::default();
    let arch = resolve_archetype(unit_name, &catalog)?;

    if matches!(arch.shape, ShapeKind::BuildingDome { .. }) {
        return Err(BakerError::UnknownArchetype {
            name: unit_name.to_string(),
            valid: crate::default_unit_archetypes(),
        });
    }

    let states = [
        AnimState::Idle,
        AnimState::Walk,
        AnimState::Attack,
        AnimState::Die,
    ];
    let frames_per_state = 8u32;
    let total_frames = frames_per_state.saturating_mul(states.len() as u32);
    let mut strip = RgbaImage::new(frame_size, frame_size.saturating_mul(total_frames));
    let mut actions = BTreeMap::new();

    for (state_idx, &state) in states.iter().enumerate() {
        let start_frame = (state_idx as u32).saturating_mul(frames_per_state);
        actions.insert(
            state,
            ActionSlice {
                start_frame,
                frame_count: frames_per_state,
                looping: state != AnimState::Die,
                hold_last_frame: state == AnimState::Die,
            },
        );

        for frame_idx in 0..frames_per_state {
            let t = frame_idx as f32 / frames_per_state as f32;
            let frame = render_unit_frame(arch, state, t, frame_size);
            let y_offset = (start_frame + frame_idx).saturating_mul(frame_size);
            image::imageops::overlay(&mut strip, &frame, 0, i64::from(y_offset));
        }
    }

    let meta = FlipbookAtlasMeta {
        name: arch.slug.clone(),
        frame_width: frame_size,
        frame_height: frame_size,
        fps,
        total_frames,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };
    meta.validate()?;

    Ok(ProceduralAtlasOutput { meta, image: strip })
}

/// Generates a procedural vertical strip atlas for a building dome archetype.
///
/// Building domes use 12 frames per animation state.
/// Default resolution is 256x256 at 15 FPS.
///
/// # Errors
/// Returns [`BakerError::UnknownArchetype`] if the archetype is not a recognized building dome,
/// or [`BakerError::ValidationFailed`] if metadata invariants fail.
pub fn generate_procedural_dome_atlas(
    dome_name: &str,
    frame_size: u32,
    fps: u32,
) -> Result<ProceduralAtlasOutput, BakerError> {
    let catalog = UnitCatalog::default();
    let arch = resolve_archetype(dome_name, &catalog)?;

    if !matches!(arch.shape, ShapeKind::BuildingDome { .. }) {
        return Err(BakerError::UnknownArchetype {
            name: dome_name.to_string(),
            valid: crate::default_dome_archetypes(),
        });
    }

    let mut states = vec![AnimState::Constructing, AnimState::Idle];
    if arch.weapon.is_some() {
        states.push(AnimState::Attack);
    } else if arch.capabilities.is_resource_dropoff
        || arch.capabilities.harvester.is_some()
        || arch.supply_provided == 0
    {
        states.push(AnimState::Cast);
    }
    states.push(AnimState::Die);

    let frames_per_state = 12u32;
    let total_frames = frames_per_state.saturating_mul(states.len() as u32);
    let mut strip = RgbaImage::new(frame_size, frame_size.saturating_mul(total_frames));
    let mut actions = BTreeMap::new();

    for (state_idx, &state) in states.iter().enumerate() {
        let start_frame = (state_idx as u32).saturating_mul(frames_per_state);
        let looping = state != AnimState::Die && state != AnimState::Constructing;
        let hold_last_frame = state == AnimState::Die || state == AnimState::Constructing;

        actions.insert(
            state,
            ActionSlice {
                start_frame,
                frame_count: frames_per_state,
                looping,
                hold_last_frame,
            },
        );

        for frame_idx in 0..frames_per_state {
            let t = frame_idx as f32 / frames_per_state as f32;
            let frame = render_dome_frame(arch, state, t, frame_size);
            let y_offset = (start_frame + frame_idx).saturating_mul(frame_size);
            image::imageops::overlay(&mut strip, &frame, 0, i64::from(y_offset));
        }
    }

    let meta = FlipbookAtlasMeta {
        name: arch.slug.clone(),
        frame_width: frame_size,
        frame_height: frame_size,
        fps,
        total_frames,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };
    meta.validate()?;

    Ok(ProceduralAtlasOutput { meta, image: strip })
}

/// Dispatches generation for either a unit or building dome based on the archetype identifier.
///
/// Automatically uses standard defaults if size/fps are not specified:
/// - Mobile units: 192x192 @ 15 FPS
/// - Building domes: 256x256 @ 15 FPS
///
/// # Errors
/// Returns [`BakerError::UnknownArchetype`] if the archetype is not recognized,
/// or [`BakerError::ValidationFailed`] if metadata invariants fail.
pub fn generate_procedural_atlas(
    archetype: &str,
    frame_size: Option<u32>,
    fps: Option<u32>,
) -> Result<ProceduralAtlasOutput, BakerError> {
    let catalog = UnitCatalog::default();
    let arch = resolve_archetype(archetype, &catalog)?;

    if matches!(arch.shape, ShapeKind::BuildingDome { .. }) {
        let size = frame_size.unwrap_or(256);
        let rate = fps.unwrap_or(15);
        generate_procedural_dome_atlas(archetype, size, rate)
    } else {
        let size = frame_size.unwrap_or(192);
        let rate = fps.unwrap_or(15);
        generate_procedural_unit_atlas(archetype, size, rate)
    }
}

// =========================================================================
// Mobile Unit Rendering Subroutines
// =========================================================================

fn render_unit_frame(arch: &UnitArchetype, state: AnimState, t: f32, size: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(size, size, Rgba([12, 14, 20, 255]));
    let c = size as f32 * 0.5;

    // Outer dark containment boundary vignette for bubble spherical projection
    draw_circle(&mut img, c, c, c * 0.94, Rgba([20, 24, 34, 255]));
    draw_circle(&mut img, c, c, c * 0.88, Rgba([28, 34, 48, 255]));

    match arch.silhouette {
        SilhouetteClass::LightProngs => {
            if arch.capabilities.harvester.is_some() || arch.weapon.is_none() {
                render_utility_hover_harvester(&mut img, state, t, c);
            } else {
                render_light_prongs_infantry(&mut img, state, t, c);
            }
        }
        SilhouetteClass::HeavyHex => {
            if arch.weapon.as_ref().is_some_and(|w| w.has_turret) {
                render_tracked_siege_turret(&mut img, state, t, c);
            } else {
                render_heavy_hex_biped(&mut img, state, t, c);
            }
        }
        SilhouetteClass::MeleeSpikes => {
            render_melee_spikes_warrior(&mut img, state, t, c);
        }
        SilhouetteClass::CasterHalo => {
            render_lenticular_aerial_craft(&mut img, state, t, c);
        }
        SilhouetteClass::StructureBase => {
            render_generic_mobile_unit(&mut img, state, t, c);
        }
    }

    // Apply death static noise / disintegration if state is Die
    if state == AnimState::Die {
        apply_death_dissolution(&mut img, t);
    }

    img
}

fn render_generic_mobile_unit(img: &mut RgbaImage, state: AnimState, t: f32, c: f32) {
    let pulse = ((t * 2.0 * PI).sin() * 0.5 + 0.5) * 8.0;
    draw_circle(img, c, c, 20.0 + pulse, Rgba([80, 180, 255, 255]));
    draw_circle(img, c, c, 12.0, Rgba([255, 255, 255, 255]));
    if state == AnimState::Attack {
        draw_line(img, c, c, c + 35.0, c - 10.0, Rgba([255, 100, 100, 255]));
    }
}

fn render_light_prongs_infantry(img: &mut RgbaImage, state: AnimState, t: f32, c: f32) {
    let bob = match state {
        AnimState::Idle => (t * 2.0 * PI).sin() * 3.0,
        AnimState::Walk => (t * 4.0 * PI).sin().abs() * 6.0,
        _ => 0.0,
    };

    let kick = if state == AnimState::Attack && t < 0.35 {
        (1.0 - t / 0.35) * 9.0
    } else {
        0.0
    };

    // Marine torso & pauldrons
    let body_color = Rgba([60, 110, 190, 255]);
    let armor_dark = Rgba([35, 65, 120, 255]);
    let visor_color = match state {
        AnimState::Idle => {
            let pulse = ((t * 2.0 * PI).sin() * 40.0 + 215.0) as u8;
            Rgba([100, pulse, 255, 255])
        }
        AnimState::Attack => Rgba([255, 200, 80, 255]),
        _ => Rgba([120, 200, 255, 255]),
    };

    // Shoulders
    draw_filled_rect(img, c - 28.0 - kick, c - 8.0 - bob, 56.0, 16.0, armor_dark);
    // Torso armor plate
    draw_filled_rect(img, c - 18.0 - kick, c - 6.0 - bob, 36.0, 36.0, body_color);
    // Helmet
    draw_circle(img, c - kick, c - 18.0 - bob, 14.0, armor_dark);
    // Visor slit
    draw_filled_rect(img, c + 2.0 - kick, c - 21.0 - bob, 14.0, 6.0, visor_color);

    // Rifle barrel
    let gun_color = Rgba([40, 45, 55, 255]);
    draw_filled_rect(img, c + 10.0 - kick, c + 2.0 - bob, 28.0, 8.0, gun_color);

    // Leg struts
    if state == AnimState::Walk {
        let leg_swing = (t * 2.0 * PI).sin() * 12.0;
        draw_line(
            img,
            c - 10.0,
            c + 30.0,
            c - 10.0 + leg_swing,
            c + 50.0,
            armor_dark,
        );
        draw_line(
            img,
            c + 10.0,
            c + 30.0,
            c + 10.0 - leg_swing,
            c + 50.0,
            armor_dark,
        );
    } else {
        draw_line(img, c - 10.0, c + 30.0, c - 10.0, c + 48.0, armor_dark);
        draw_line(img, c + 10.0, c + 30.0, c + 10.0, c + 48.0, armor_dark);
    }

    // Muzzle blast
    if state == AnimState::Attack && t < 0.4 {
        let progress = t / 0.4;
        let star_r = (1.0 - progress) * 26.0;
        let flash_x = c + 40.0 - kick;
        let flash_y = c + 6.0 - bob;
        draw_circle(img, flash_x, flash_y, star_r, Rgba([255, 230, 60, 255]));
        draw_circle(
            img,
            flash_x,
            flash_y,
            star_r * 0.5,
            Rgba([255, 255, 240, 255]),
        );
        draw_line(
            img,
            flash_x,
            flash_y - star_r * 1.5,
            flash_x,
            flash_y + star_r * 1.5,
            Rgba([255, 255, 200, 255]),
        );
        draw_line(
            img,
            flash_x - star_r * 1.5,
            flash_y,
            flash_x + star_r * 1.5,
            flash_y,
            Rgba([255, 255, 200, 255]),
        );
    }
}

fn render_heavy_hex_biped(img: &mut RgbaImage, state: AnimState, t: f32, c: f32) {
    let heavy_bob = match state {
        AnimState::Idle => (t * 2.0 * PI).sin() * 2.0,
        AnimState::Walk => (t * 4.0 * PI).sin().abs() * 8.0,
        _ => 0.0,
    };

    let kick = if state == AnimState::Attack && t < 0.4 {
        (1.0 - t / 0.4) * 12.0
    } else {
        0.0
    };

    let chassis = Rgba([80, 85, 95, 255]);
    let chassis_dark = Rgba([45, 48, 55, 255]);
    let vent_color = {
        let intensity = ((t * 4.0 * PI).sin() * 50.0 + 205.0) as u8;
        Rgba([intensity, 90, 30, 255])
    };

    // Heavy shoulder bulwarks
    draw_filled_rect(
        img,
        c - 36.0 - kick,
        c - 14.0 - heavy_bob,
        72.0,
        22.0,
        chassis_dark,
    );
    // Torso mass
    draw_filled_rect(
        img,
        c - 24.0 - kick,
        c - 8.0 - heavy_bob,
        48.0,
        44.0,
        chassis,
    );
    // Hydraulic exhaust vents
    draw_circle(img, c - 24.0 - kick, c - 14.0 - heavy_bob, 6.0, vent_color);
    draw_circle(img, c + 24.0 - kick, c - 14.0 - heavy_bob, 6.0, vent_color);

    // Twin Grenade Launcher Barrels
    draw_filled_rect(
        img,
        c + 14.0 - kick,
        c + 2.0 - heavy_bob,
        26.0,
        10.0,
        chassis_dark,
    );
    draw_filled_rect(
        img,
        c + 14.0 - kick,
        c + 18.0 - heavy_bob,
        26.0,
        10.0,
        chassis_dark,
    );

    // Concussive burst smoke rings
    if state == AnimState::Attack && t < 0.5 {
        let progress = t / 0.5;
        let ring_r = progress * 32.0;
        let alpha = ((1.0 - progress) * 255.0) as u8;
        let burst_x = c + 44.0 - kick;
        draw_circle_outline(
            img,
            burst_x,
            c + 7.0,
            ring_r,
            3.0,
            Rgba([255, 140, 40, alpha]),
        );
        draw_circle_outline(
            img,
            burst_x + 8.0,
            c + 23.0,
            ring_r * 0.8,
            3.0,
            Rgba([255, 180, 70, alpha]),
        );
    }
}

fn render_melee_spikes_warrior(img: &mut RgbaImage, state: AnimState, t: f32, c: f32) {
    let forward_lean = match state {
        AnimState::Walk => 14.0,
        _ => 0.0,
    };

    let gold_armor = Rgba([215, 175, 55, 255]);
    let dark_bronze = Rgba([140, 105, 30, 255]);
    let psi_cyan = {
        let pulse = ((t * 4.0 * PI).sin() * 40.0 + 215.0) as u8;
        Rgba([40, pulse, 255, 255])
    };

    // Slender athletic torso
    draw_filled_rect(
        img,
        c - 14.0 + forward_lean,
        c - 10.0,
        28.0,
        40.0,
        gold_armor,
    );
    draw_filled_rect(
        img,
        c - 10.0 + forward_lean,
        c - 6.0,
        20.0,
        32.0,
        dark_bronze,
    );
    // Protoss head crest
    draw_circle(img, c + forward_lean, c - 20.0, 12.0, gold_armor);
    draw_circle(img, c + 4.0 + forward_lean, c - 22.0, 4.0, psi_cyan);

    // Psionic blades
    let blade_len = match state {
        AnimState::Attack => 46.0,
        _ => 30.0 + (t * 2.0 * PI).sin() * 5.0,
    };

    if state == AnimState::Attack {
        // Cross-slashing twin psi-arcs
        let sweep = (t * PI).sin() * 40.0;
        draw_line(
            img,
            c - 10.0,
            c - 10.0 + sweep,
            c + 40.0,
            c + 20.0 - sweep,
            psi_cyan,
        );
        draw_line(
            img,
            c - 10.0,
            c + 20.0 - sweep,
            c + 40.0,
            c - 10.0 + sweep,
            psi_cyan,
        );
        draw_circle(img, c + 15.0, c + 5.0, 12.0, Rgba([180, 240, 255, 255]));
    } else {
        // Ambient wrist blades extending forward
        draw_line(
            img,
            c + 10.0 + forward_lean,
            c + 8.0,
            c + 10.0 + forward_lean + blade_len,
            c - 2.0,
            psi_cyan,
        );
        draw_line(
            img,
            c + 10.0 + forward_lean,
            c + 22.0,
            c + 10.0 + forward_lean + blade_len,
            c + 32.0,
            psi_cyan,
        );
    }

    if state == AnimState::Walk {
        // Cyan energy motion wake behind the zealot
        let trail_x = c - 20.0;
        draw_circle(img, trail_x, c + 5.0, 14.0, Rgba([30, 120, 220, 140]));
        draw_circle(img, trail_x - 12.0, c + 8.0, 8.0, Rgba([20, 80, 180, 80]));
    }
}

fn render_tracked_siege_turret(img: &mut RgbaImage, state: AnimState, t: f32, c: f32) {
    let chassis_color = Rgba([90, 100, 115, 255]);
    let steel_dark = Rgba([40, 45, 55, 255]);
    let recoil = if state == AnimState::Attack && t < 0.3 {
        (1.0 - t / 0.3) * 16.0
    } else {
        0.0
    };

    // Heavy tracked treads
    let tread_offset = if state == AnimState::Walk {
        (t * 8.0) as i32 % 8
    } else {
        0
    };
    draw_filled_rect(img, c - 40.0, c + 18.0, 80.0, 20.0, steel_dark);
    for i in 0..8 {
        let x = c - 36.0 + (i as f32 * 10.0) + tread_offset as f32;
        if x < c + 38.0 {
            draw_line(img, x, c + 19.0, x, c + 37.0, Rgba([70, 75, 85, 255]));
        }
    }

    // Tank hull body
    draw_filled_rect(img, c - 32.0, c - 2.0, 64.0, 24.0, chassis_color);

    // Rotating or recoiling cannon turret
    let turret_angle = match state {
        AnimState::Idle => (t * 2.0 * PI).sin() * 0.2,
        _ => 0.0,
    };
    let barrel_len = 46.0;
    let b_end_x = c + turret_angle.cos() * barrel_len - recoil;
    let b_end_y = c - 10.0 + turret_angle.sin() * barrel_len;

    draw_circle(img, c - recoil, c - 8.0, 16.0, steel_dark);
    draw_line(img, c - recoil, c - 8.0, b_end_x, b_end_y, steel_dark);
    draw_circle(img, c - recoil, c - 8.0, 8.0, chassis_color);

    // Massive siege cannon blast
    if state == AnimState::Attack && t < 0.4 {
        let progress = t / 0.4;
        let blast_r = progress * 34.0;
        let muzzle_x = c + barrel_len - recoil;
        draw_circle(
            img,
            muzzle_x,
            c - 8.0,
            blast_r * 0.7,
            Rgba([255, 200, 50, 255]),
        );
        draw_circle(
            img,
            muzzle_x,
            c - 8.0,
            blast_r * 0.4,
            Rgba([255, 255, 255, 255]),
        );
        draw_circle_outline(
            img,
            muzzle_x + 8.0,
            c - 8.0,
            blast_r,
            3.0,
            Rgba([255, 120, 20, 200]),
        );
    }
}

fn render_lenticular_aerial_craft(img: &mut RgbaImage, state: AnimState, t: f32, c: f32) {
    let hover = match state {
        AnimState::Idle => (t * 4.0 * PI).sin() * 6.0,
        AnimState::Walk => (t * 2.0 * PI).sin() * 3.0,
        _ => 0.0,
    };
    let tilt = match state {
        AnimState::Walk => 12.0,
        _ => 0.0,
    };

    let hull_color = Rgba([70, 130, 200, 255]);
    let cockpit_glass = Rgba([140, 240, 255, 255]);
    let engine_plasma = {
        let pulse = ((t * 6.0 * PI).sin() * 50.0 + 205.0) as u8;
        Rgba([pulse, 60, 240, 255])
    };

    // Anti-gravity spinning propulsion ring
    let ring_w = (t * 2.0 * PI).cos().abs() * 36.0 + 12.0;
    draw_circle_outline(img, c, c + 14.0 - hover, ring_w, 2.0, engine_plasma);

    // Delta-wing aerodynamic hull
    draw_filled_rect(
        img,
        c - 24.0 + tilt,
        c - 12.0 - hover,
        48.0,
        20.0,
        hull_color,
    );
    draw_circle(img, c + 4.0 + tilt, c - 8.0 - hover, 14.0, cockpit_glass);

    // Dual wingtip pulse laser attack
    if state == AnimState::Attack {
        let laser_flicker = if ((t * 8.0) as i32) % 2 == 0 { 255 } else { 80 };
        draw_line(
            img,
            c + 20.0,
            c - 16.0,
            c + 60.0,
            c - 16.0,
            Rgba([80, 240, 255, laser_flicker]),
        );
        draw_line(
            img,
            c + 20.0,
            c + 4.0,
            c + 60.0,
            c + 4.0,
            Rgba([80, 240, 255, laser_flicker]),
        );
    }

    if state == AnimState::Walk {
        // Dual wingtip trails
        draw_line(
            img,
            c - 24.0,
            c - 12.0 - hover,
            c - 50.0,
            c - 8.0 - hover,
            Rgba([60, 160, 255, 120]),
        );
        draw_line(
            img,
            c - 24.0,
            c + 4.0 - hover,
            c - 50.0,
            c + 8.0 - hover,
            Rgba([60, 160, 255, 120]),
        );
    }
}

fn render_utility_hover_harvester(img: &mut RgbaImage, state: AnimState, t: f32, c: f32) {
    let hover = (t * 3.0 * PI).sin() * 4.0;
    let chassis = Rgba([220, 150, 40, 255]);
    let chassis_dark = Rgba([120, 75, 15, 255]);

    // Rounded utility chassis
    draw_circle(img, c, c - hover, 20.0, chassis);
    draw_circle(img, c, c - hover, 12.0, chassis_dark);
    draw_circle(
        img,
        c + 4.0,
        c - 4.0 - hover,
        5.0,
        Rgba([100, 220, 255, 255]),
    );

    // Rotating fusion drill / mining arm
    let drill_angle = t * 6.0 * PI;
    let arm_x = c + 16.0;
    let arm_y = c + 6.0 - hover;
    let tip_x = arm_x + drill_angle.cos() * 18.0;
    let tip_y = arm_y + drill_angle.sin() * 8.0;
    draw_line(img, arm_x, arm_y, tip_x, tip_y, Rgba([200, 200, 210, 255]));

    // Welding electrical arc sparks
    if state == AnimState::Attack {
        let spark_color = Rgba([120, 220, 255, 255]);
        for i in 0..5 {
            let angle = (i as f32 * 1.3) + t * 10.0;
            let sp_x = tip_x + angle.cos() * 16.0;
            let sp_y = tip_y + angle.sin() * 16.0;
            draw_line(img, tip_x, tip_y, sp_x, sp_y, spark_color);
        }
    }
}

// =========================================================================
// Building Dome Rendering Subroutines
// =========================================================================

fn render_dome_frame(arch: &UnitArchetype, state: AnimState, t: f32, size: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(size, size, Rgba([10, 12, 16, 255]));
    let c = size as f32 * 0.5;

    // Dome base foundation plate
    let base_plate = Rgba([30, 36, 48, 255]);
    draw_filled_rect(
        &mut img,
        c - c * 0.85,
        c + c * 0.5,
        c * 1.7,
        24.0,
        base_plate,
    );

    // Hemisphere outline
    let dome_radius = c * 0.75;
    let dome_center_y = c + c * 0.5;

    if arch.weapon.is_some() {
        render_defensive_spire_dome(&mut img, state, t, c, dome_center_y, dome_radius);
    } else if arch.capabilities.is_resource_dropoff {
        render_command_hub_dome(&mut img, state, t, c, dome_center_y, dome_radius);
    } else if arch.supply_provided > 0 {
        render_energy_storage_dome(&mut img, state, t, c, dome_center_y, dome_radius);
    } else {
        render_production_hangar_dome(&mut img, state, t, c, dome_center_y, dome_radius);
    }

    // If constructing, overlay holographic scanlines rising vertically
    if state == AnimState::Constructing {
        apply_construction_wireframe(&mut img, t, c, dome_center_y, dome_radius);
    } else if state == AnimState::Die {
        apply_death_dissolution(&mut img, t);
    }

    img
}

fn render_command_hub_dome(
    img: &mut RgbaImage,
    state: AnimState,
    t: f32,
    c: f32,
    base_y: f32,
    radius: f32,
) {
    let dome_color = Rgba([50, 70, 95, 255]);
    let beacon_color = {
        let intensity = ((t * 4.0 * PI).sin() * 50.0 + 205.0) as u8;
        Rgba([intensity, 210, 60, 255])
    };

    draw_hemisphere(img, c, base_y, radius, dome_color);

    // 360° Rotating Radar Dish on Apex
    let apex_y = base_y - radius;
    let dish_sweep = t * 2.0 * PI;
    let dish_w = dish_sweep.cos() * 24.0;
    draw_filled_rect(
        img,
        c - 4.0,
        apex_y - 12.0,
        8.0,
        12.0,
        Rgba([80, 90, 110, 255]),
    );
    draw_line(
        img,
        c - dish_w,
        apex_y - 18.0,
        c + dish_w,
        apex_y - 18.0,
        Rgba([180, 200, 220, 255]),
    );
    draw_circle(img, c, apex_y - 20.0, 4.0, beacon_color);

    // Worker production robotics welding inside
    if state == AnimState::Cast {
        let weld_t = (t * 8.0 * PI).sin().abs();
        let glow = (weld_t * 180.0 + 75.0) as u8;
        draw_circle(
            img,
            c,
            base_y - radius * 0.4,
            20.0,
            Rgba([60, glow, 255, 255]),
        );
        draw_circle(
            img,
            c,
            base_y - radius * 0.4,
            10.0,
            Rgba([255, 255, 255, 255]),
        );
    }
}

fn render_production_hangar_dome(
    img: &mut RgbaImage,
    state: AnimState,
    t: f32,
    c: f32,
    base_y: f32,
    radius: f32,
) {
    let steel_blue = Rgba([40, 60, 90, 255]);
    draw_hemisphere(img, c, base_y, radius, steel_blue);

    // Reinforced external pylons
    draw_filled_rect(
        img,
        c - radius * 0.85,
        base_y - radius * 0.6,
        18.0,
        radius * 0.6,
        Rgba([30, 40, 55, 255]),
    );
    draw_filled_rect(
        img,
        c + radius * 0.85 - 18.0,
        base_y - radius * 0.6,
        18.0,
        radius * 0.6,
        Rgba([30, 40, 55, 255]),
    );

    // Bay door light
    let bay_pulse = ((t * 2.0 * PI).sin() * 40.0 + 215.0) as u8;
    let bay_color = Rgba([bay_pulse, 160, 50, 255]);
    draw_filled_rect(
        img,
        c - 28.0,
        base_y - 32.0,
        56.0,
        32.0,
        Rgba([20, 25, 35, 255]),
    );
    draw_filled_rect(img, c - 24.0, base_y - 28.0, 48.0, 6.0, bay_color);

    // Hologram soldier assembly during Cast
    if state == AnimState::Cast {
        let holo_alpha = ((t * 4.0 * PI).sin().abs() * 150.0 + 100.0) as u8;
        draw_circle(
            img,
            c,
            base_y - 18.0,
            12.0,
            Rgba([80, 180, 255, holo_alpha]),
        );
        draw_filled_rect(
            img,
            c - 8.0,
            base_y - 12.0,
            16.0,
            12.0,
            Rgba([80, 180, 255, holo_alpha]),
        );
    }
}

fn render_energy_storage_dome(
    img: &mut RgbaImage,
    _state: AnimState,
    t: f32,
    c: f32,
    base_y: f32,
    radius: f32,
) {
    let dome_color = Rgba([60, 65, 80, 255]);
    draw_hemisphere(img, c, base_y, radius, dome_color);

    // Energy distribution rings stacked along height
    for i in 1..=3 {
        let r_layer = radius * (1.0 - (i as f32) * 0.22);
        let y_layer = base_y - (i as f32 * radius * 0.26);
        let phase = t * 2.0 * PI + (i as f32);
        let ring_pulse = (phase.sin() * 50.0 + 205.0) as u8;
        draw_circle_outline(
            img,
            c,
            y_layer,
            r_layer,
            3.0,
            Rgba([50, ring_pulse, 255, 255]),
        );
    }

    // Central floating power prism
    let prism_pulse = ((t * 3.0 * PI).sin() * 40.0 + 215.0) as u8;
    draw_circle(
        img,
        c,
        base_y - radius * 0.5,
        14.0,
        Rgba([255, prism_pulse, 80, 255]),
    );
}

fn render_defensive_spire_dome(
    img: &mut RgbaImage,
    state: AnimState,
    t: f32,
    c: f32,
    base_y: f32,
    radius: f32,
) {
    let hull_metal = Rgba([55, 60, 70, 255]);
    draw_hemisphere(img, c, base_y, radius, hull_metal);

    let apex_y = base_y - radius;
    let turret_color = Rgba([35, 40, 50, 255]);
    draw_circle(img, c, apex_y, 22.0, turret_color);

    match state {
        AnimState::Idle => {
            // Sweeping green targeting laser line
            let sweep_angle = -PI * 0.5 + (t * 2.0 * PI).sin() * 0.7;
            let beam_len = 65.0;
            let end_x = c + sweep_angle.cos() * beam_len;
            let end_y = apex_y + sweep_angle.sin() * beam_len;
            draw_line(img, c, apex_y, end_x, end_y, Rgba([60, 255, 100, 220]));
            draw_circle(img, end_x, end_y, 4.0, Rgba([120, 255, 150, 255]));
        }
        AnimState::Attack => {
            // Continuous high-energy red laser discharge
            let beam_len = 80.0;
            let end_x = c + beam_len;
            let end_y = apex_y - 10.0;
            draw_line(img, c, apex_y, end_x, end_y, Rgba([255, 40, 40, 255]));
            draw_line(
                img,
                c,
                apex_y - 1.0,
                end_x,
                end_y - 1.0,
                Rgba([255, 200, 200, 255]),
            );
            draw_circle(img, c, apex_y, 10.0, Rgba([255, 80, 80, 255]));
            draw_circle(img, end_x, end_y, 12.0, Rgba([255, 180, 60, 255]));
        }
        _ => {}
    }
}

// =========================================================================
// Graphics Primitives & Shader Filter Helpers
// =========================================================================

fn draw_hemisphere(img: &mut RgbaImage, cx: f32, cy: f32, radius: f32, color: Rgba<u8>) {
    let r2 = radius * radius;
    let min_x = (cx - radius).max(0.0) as u32;
    let max_x = (cx + radius).min(img.width() as f32 - 1.0) as u32;
    let min_y = (cy - radius).max(0.0) as u32;
    let max_y = cy.min(img.height() as f32 - 1.0) as u32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= r2 {
                img.put_pixel(x, y, color);
            }
        }
    }
}

fn draw_circle(img: &mut RgbaImage, cx: f32, cy: f32, radius: f32, color: Rgba<u8>) {
    let r2 = radius * radius;
    let min_x = (cx - radius).max(0.0) as u32;
    let max_x = (cx + radius).min(img.width() as f32 - 1.0) as u32;
    let min_y = (cy - radius).max(0.0) as u32;
    let max_y = (cy + radius).min(img.height() as f32 - 1.0) as u32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= r2 {
                blend_pixel(img, x, y, color);
            }
        }
    }
}

fn draw_circle_outline(
    img: &mut RgbaImage,
    cx: f32,
    cy: f32,
    radius: f32,
    thickness: f32,
    color: Rgba<u8>,
) {
    let r_outer2 = (radius + thickness * 0.5) * (radius + thickness * 0.5);
    let r_inner2 = (radius - thickness * 0.5).max(0.0) * (radius - thickness * 0.5).max(0.0);
    let bound = radius + thickness;
    let min_x = (cx - bound).max(0.0) as u32;
    let max_x = (cx + bound).min(img.width() as f32 - 1.0) as u32;
    let min_y = (cy - bound).max(0.0) as u32;
    let max_y = (cy + bound).min(img.height() as f32 - 1.0) as u32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d2 = dx * dx + dy * dy;
            if d2 <= r_outer2 && d2 >= r_inner2 {
                blend_pixel(img, x, y, color);
            }
        }
    }
}

fn draw_filled_rect(img: &mut RgbaImage, x0: f32, y0: f32, w: f32, h: f32, color: Rgba<u8>) {
    let min_x = x0.max(0.0) as u32;
    let max_x = (x0 + w).min(img.width() as f32 - 1.0) as u32;
    let min_y = y0.max(0.0) as u32;
    let max_y = (y0 + h).min(img.height() as f32 - 1.0) as u32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            blend_pixel(img, x, y, color);
        }
    }
}

fn draw_line(img: &mut RgbaImage, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba<u8>) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let steps = (dx.max(dy) as u32).max(1);

    for s in 0..=steps {
        let frac = s as f32 / steps as f32;
        let px = x0 + (x1 - x0) * frac;
        let py = y0 + (y1 - y0) * frac;
        if px >= 0.0 && py >= 0.0 && px < img.width() as f32 && py < img.height() as f32 {
            blend_pixel(img, px as u32, py as u32, color);
        }
    }
}

fn blend_pixel(img: &mut RgbaImage, x: u32, y: u32, overlay: Rgba<u8>) {
    if overlay[3] == 255 {
        img.put_pixel(x, y, overlay);
        return;
    }
    if overlay[3] == 0 {
        return;
    }

    let base = img.get_pixel(x, y);
    let alpha = f32::from(overlay[3]) / 255.0;
    let inv_alpha = 1.0 - alpha;

    let r = (f32::from(overlay[0]) * alpha + f32::from(base[0]) * inv_alpha) as u8;
    let g = (f32::from(overlay[1]) * alpha + f32::from(base[1]) * inv_alpha) as u8;
    let b = (f32::from(overlay[2]) * alpha + f32::from(base[2]) * inv_alpha) as u8;

    img.put_pixel(x, y, Rgba([r, g, b, 255]));
}

fn apply_construction_wireframe(img: &mut RgbaImage, t: f32, cx: f32, cy: f32, radius: f32) {
    let scanline_y = cy - radius * t;
    let width = img.width();

    for x in 0..width {
        let dx = x as f32 - cx;
        if dx.abs() < radius {
            if scanline_y >= 0.0 && (scanline_y as u32) < img.height() {
                blend_pixel(img, x, scanline_y as u32, Rgba([80, 240, 255, 255]));
            }
            // Mask out regions above the rising scanline
            let min_y = (cy - radius).max(0.0) as u32;
            let current_cutoff = (scanline_y.max(0.0)) as u32;
            for y in min_y..current_cutoff {
                let pixel = img.get_pixel_mut(x, y);
                // Darken & add grid wireframe
                if x % 8 == 0 || y % 8 == 0 {
                    *pixel = Rgba([40, 80, 110, 255]);
                } else {
                    *pixel = Rgba([12, 16, 24, 255]);
                }
            }
        }
    }
}

fn apply_death_dissolution(img: &mut RgbaImage, t: f32) {
    let width = img.width();
    let height = img.height();
    let threshold = (t * 255.0) as u32;

    for y in 0..height {
        for x in 0..width {
            // Deterministic hash noise based on coordinates
            let hash = ((x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263)) >> 24) & 0xFF;
            if hash < threshold {
                let pixel = img.get_pixel_mut(x, y);
                if hash + 30 > threshold {
                    // Burning glowing ember edge
                    *pixel = Rgba([255, 120, 40, 255]);
                } else {
                    // Dissolved void
                    *pixel = Rgba([8, 10, 14, 255]);
                }
            }
        }
    }
}
