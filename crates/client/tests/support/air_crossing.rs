//! A straight cliff crossing for an air unit on the shipped battle map, and the air unit that flies
//! it (plan `2026-11-30-terrain-and-air-indicators` §5.3, I-1, and the WP2 review captures).
//!
//! # Responsibility
//! - [`Crossing::find`]: a row along which cells `x - START_CELLS ..= x` are one plateau and cells
//!   `x + 1 ..= x + 1 + PAST_CELLS` another of a different level, with no ramp near it and no unit
//!   of another player near it.
//! - [`smallest_air_unit`]: the selectable air archetype with media and the smallest drawn
//!   radius, chosen by catalog property, never by slug (charter C5).
//!
//! # How tests use it
//! Include it with `#[path = "support/air_crossing.rs"]` next to `battle_harness.rs`.

#![allow(clippy::panic, clippy::expect_used, clippy::cast_precision_loss)]

use bevy::prelude::Vec2;
use sim_map::grid::GameMap;
use sim_types::{MoverType, ShapeKind, SimScalar, UnitTypeId};

use crate::battle_harness::{self, Harness};

/// Cells the unit starts before the cliff.
pub const START_CELLS: u32 = 2;

/// Cells the unit flies past the cliff.
pub const PAST_CELLS: u32 = 3;

/// Cells around the crossing that must hold no ramp.
const RAMP_CLEARANCE: u32 = 2;

/// Least distance in cells between the crossing and any unit of another player.
const ENEMY_CLEARANCE: f32 = 16.0;

/// A cliff the unit crosses along +X on row `y`: from cell `x - START_CELLS` to cell
/// `x + 1 + PAST_CELLS`, where cells `x` and `x + 1` are plateaus of different level.
#[derive(Debug, Clone, Copy)]
pub struct Crossing {
    /// The cell on the near side of the edge.
    pub x: u32,
    /// The row crossed.
    pub y: u32,
}

impl Crossing {
    /// The first row-wise crossing of `map` with plain ground on its whole path, no ramp near it
    /// and no unit of another player near it.
    pub fn find(map: &GameMap, enemies: &[Vec2]) -> Self {
        let level = |x: u32, y: u32| {
            let cell = map.cell(x, y).ok()?;
            let ground = cell.terrain_kind != sim_map::grid::TerrainKind::Abyss
                && map.ramp_profile(x, y).is_none();
            ground.then_some(cell.cliff_level.0)
        };
        let clear_of_ramps = |x: u32, y: u32| {
            (x.saturating_sub(START_CELLS + RAMP_CLEARANCE)..=x + PAST_CELLS + RAMP_CLEARANCE + 1)
                .all(|cx| {
                    (y.saturating_sub(RAMP_CLEARANCE)..=y + RAMP_CLEARANCE)
                        .all(|cy| map.ramp_profile(cx, cy).is_none())
                })
        };
        for y in RAMP_CLEARANCE..map.height().saturating_sub(RAMP_CLEARANCE) {
            for x in START_CELLS..map.width().saturating_sub(PAST_CELLS + 1) {
                let path = (x - START_CELLS..=x + 1 + PAST_CELLS).map(|cx| level(cx, y));
                let Some(levels) = path.collect::<Option<Vec<u8>>>() else {
                    continue;
                };
                let near = levels[..=START_CELLS as usize]
                    .iter()
                    .all(|l| *l == levels[0]);
                let far = levels[START_CELLS as usize + 1..]
                    .iter()
                    .all(|l| *l == levels[levels.len() - 1]);
                let centre = Vec2::new(x as f32 + 1.0, y as f32 + 0.5);
                if near
                    && far
                    && levels[0] != levels[levels.len() - 1]
                    && clear_of_ramps(x, y)
                    && enemies.iter().all(|e| e.distance(centre) > ENEMY_CLEARANCE)
                {
                    return Self { x, y };
                }
            }
        }
        panic!("the shipped battle map has a straight cliff crossing clear of ramps and enemies");
    }

    /// World centre of cell `(x, self.y)`.
    pub fn at(self, x: u32) -> Vec2 {
        Vec2::new(x as f32 + 0.5, self.y as f32 + 0.5)
    }
}

/// The selectable air archetype with media and the smallest drawn radius, by catalog property.
pub fn smallest_air_unit(harness: &Harness) -> UnitTypeId {
    let assets = battle_harness::assets();
    harness
        .world()
        .catalog
        .archetypes()
        .filter(|a| {
            a.mover_type == MoverType::Air
                && a.selectable
                && a.max_speed > SimScalar::ZERO
                && matches!(a.shape, ShapeKind::UnitSphere)
                && battle_harness::presentable(&assets, &a.slug)
        })
        .min_by_key(|a| (a.drawn_radius(), a.unit_type_id))
        .expect("the catalog ships a selectable air unit with media")
        .unit_type_id
}
