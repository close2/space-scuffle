//! Release micro-benchmark of the minimap's CPU work (plan `2026-12-01-minimap` M-B1).
//!
//! # Responsibility
//! Measures, on the shipped skirmish with 400 units added (200 per player) and the local
//! player's real fog:
//! - `rebuild`: one marks rebuild, [`build_marks`], over the presented bubbles, the map's
//!   resources and the fog, the work done once per presented tick;
//! - `fog`: one rebuild of the fog bytes from the fog's cell states, once per refill;
//! - `per_frame`: the work done on every frame whatever happened: the layout for the map and the
//!   image (the camera frustum of WP3 is not built yet).
//!
//! Each is timed over [`ROUNDS`] passes and the median is printed in microseconds, with the
//! plan's bars (0.2 ms per rebuild, 0.02 ms per frame).
//!
//! # Invariants
//! - Wall-clock time is reported, never asserted; the binary fails only on a staging error.
//! - Inputs and outputs pass through `std::hint::black_box`.
//! - Everything is read from the real assets root, as the client reads it; no unit identity is
//!   named here.
//!
//! Run with `cargo bench -p client --bench minimap_prepare`.

use std::error::Error;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use bevy::prelude::*;
use client::battle_pipeline::commands::SessionGeneration;
use client::battle_pipeline::fog::BattleFog;
use client::minimap::layout::MinimapLayout;
use client::minimap::marks::{build_marks, MarkSources};
use client::minimap::style::MinimapStyle;
use client::minimap::ColorMode;
use client::runtime_battle::{BattleDefinition, MapObjectSnapshot, SKIRMISH_SCENARIO};
use sim_core::SimWorld;
use sim_types::{PlayerId, PlayerView, SimScalar, SimVec2, Tick, TickBundle, VisibilityState};

/// Units added per player.
const UNITS_PER_PLAYER: usize = 200;
/// Timed passes; the median is reported.
const ROUNDS: usize = 501;
/// Side of the minimap image, in pixels, as the shipped style draws it at one pixel per point.
const SIDE: f32 = 172.0;

/// Adds [`UNITS_PER_PLAYER`] units of each player's first mobile type on open cells, the
/// first player's from one corner and the second's from the other.
fn add_armies(world: &mut SimWorld) -> Result<usize, Box<dyn Error>> {
    let players = [PlayerId(0), PlayerId(1)];
    let mut added = 0;
    for (index, owner) in players.into_iter().enumerate() {
        let unit_type = world
            .entities
            .values()
            .find(|unit| unit.owner == owner && !unit.is_immobile)
            .ok_or("invariant: every skirmish player starts with mobile units")?
            .unit_type_id;
        let (width, height) = (world.map.width(), world.map.height());
        let mut placed = 0;
        'cells: for row in 0..height {
            for column in 0..width {
                let (x, y) = if index == 0 {
                    (column, row)
                } else {
                    (width - 1 - column, height - 1 - row)
                };
                if placed == UNITS_PER_PLAYER {
                    break 'cells;
                }
                let at = SimVec2::new(
                    SimScalar::from_num(x) + SimScalar::HALF,
                    SimScalar::from_num(y) + SimScalar::HALF,
                );
                if world.spawn_unit(unit_type, owner, at).is_ok() {
                    placed += 1;
                }
            }
        }
        added += placed;
    }
    Ok(added)
}

/// Median of `samples`, in microseconds.
fn median_us(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

/// Times `work` over [`ROUNDS`] passes and returns the median in microseconds.
fn time(mut work: impl FnMut()) -> f64 {
    let samples = (0..ROUNDS)
        .map(|_| {
            let start = Instant::now();
            work();
            start.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    median_us(samples)
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = client::ResolvedAssetsRoot::from_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = BattleDefinition::load(root.as_path(), SKIRMISH_SCENARIO.as_ref())?;
    let mut world = definition.instantiate()?;
    let added = add_armies(&mut world)?;
    let snapshot = world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands: Vec::new(),
        state_checksum: None,
    });
    let viewer = PlayerId(0);
    let mut view = PlayerView::default();
    world.write_player_view(viewer, &mut view);
    let mut fog = BattleFog::default();
    fog.refill(
        SessionGeneration::FIRST,
        &view,
        world.map.resources().iter().map(|node| node.remaining > 0),
    );
    let style = MinimapStyle::load(root.as_path())?;
    let layout = MinimapLayout::new(world.map.width(), world.map.height(), SIDE)
        .ok_or("invariant: the skirmish map has a size")?;
    let objects: Vec<MapObjectSnapshot> = world
        .map
        .resources()
        .iter()
        .map(|node| MapObjectSnapshot {
            position: Vec3::new(node.pos.x.to_f32(), 0.0, node.pos.y.to_f32()),
            visible: node.remaining > 0,
            owner: None,
            ownership_tint: false,
            size: 1.0,
        })
        .collect();
    let sources = MarkSources {
        layout,
        style: &style,
        mode: ColorMode::Player,
        viewer: Some(viewer),
        fog: Some(&fog),
        bubbles: &snapshot.bubbles,
        catalog: Some(&world.catalog),
        palette: Some(&definition.palette),
        objects: &objects,
        nodes: world.map.resources(),
        selected: &[],
    };
    let (mut marks, mut subjects) = (Vec::new(), Vec::new());
    let (marks, subjects) = (black_box(&mut marks), black_box(&mut subjects));
    let rebuild = time(|| build_marks(black_box(&sources), marks, subjects));
    let states = fog.cell_states();
    let darkening = style.fog_darkening;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "levels are validated to [0, 1]"
    )]
    let byte = |level: f32| (level * 255.0).round() as u8;
    let levels = [
        byte(darkening.visible),
        byte(darkening.fogged),
        byte(darkening.unexplored),
    ];
    let mut bytes = Vec::with_capacity(states.len());
    let fog_rebuild = time(|| {
        bytes.clear();
        bytes.extend(black_box(states).iter().map(|state| match state {
            VisibilityState::Visible => levels[0],
            VisibilityState::Fogged => levels[1],
            VisibilityState::Unexplored => levels[2],
        }));
        black_box(&bytes);
    });
    let per_frame = time(|| {
        black_box(MinimapLayout::new(
            black_box(world.map.width()),
            black_box(world.map.height()),
            black_box(SIDE),
        ));
    });
    println!(
        "minimap_prepare: {} bubbles ({added} added), {} in fog of player 0, {} marks",
        snapshot.bubbles.len(),
        states
            .iter()
            .filter(|state| **state != VisibilityState::Visible)
            .count(),
        marks.len()
    );
    println!("| Part | Median (µs) | Bar (µs) |");
    println!("|---|---:|---:|");
    println!("| rebuild (marks, per tick) | {rebuild:.2} | 200 |");
    println!("| fog bytes (per refill) | {fog_rebuild:.2} | — |");
    println!("| per frame (layout) | {per_frame:.3} | 20 |");
    Ok(())
}
