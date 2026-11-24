//! Stages an attack snapshot on the shipped skirmish (plan `2026-11-24-attacker-snapshot`).
//!
//! # Responsibility
//! Finds a cliff edge of the shipped skirmish map, far from the local player's units, and places
//! a ranged enemy soldier on its high side and a tough local soldier holding below it, in the
//! enemy's weapon reach: the enemy strikes from where the local player does not see it, which
//! leaves the local player an attack snapshot (`PlayerView::attack_snapshots`).
//!
//! # How tests use it
//! Include it with `#[path = "support/attack_snapshot_stage.rs"]` and call [`stage`] on the
//! session's world (through the harness's `world_mut`, as the harness allows, or on a world
//! before its session is prepared). It is setup: no order under test is issued here.
//!
//! # Invariants
//! - The spot is found by reading the map (cliff levels and open ground), never by coordinates
//!   written here, and the unit by its catalog slug in test data, never in production code.

#![allow(clippy::expect_used, clippy::cast_precision_loss)]

use bevy::prelude::*;
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, UnitOrder};

/// The local player of the skirmish: its first participant.
pub const LOCAL_PLAYER: PlayerId = PlayerId(0);

/// The enemy of the local player in the skirmish.
pub const ENEMY: PlayerId = PlayerId(1);

/// A ranged soldier with shipped media; its weapon keeps the default `Snapshot` visibility.
pub const SOLDIER: &str = "pulse_bot";

/// Cells between the high and the low soldier: inside the soldier's reach (5 from edge to edge).
const GAP: i32 = 5;

/// Cells of open ground of one level each soldier keeps round it, so neither stands at a cliff's
/// foot or brink, outside its terrain-radius movement domain.
const CLEARANCE: i32 = 2;

/// The staged fight.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotStage {
    /// The enemy soldier on the high ground.
    pub attacker: EntityId,
    /// The local soldier below it.
    pub victim: EntityId,
    /// Map position of the attacker, world units.
    pub high: Vec2,
    /// Map position of the victim, world units.
    pub low: Vec2,
}

/// A map position as the simulation's fixed point.
fn sim_at(position: Vec2) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(position.x),
        SimScalar::from_num(position.y),
    )
}

/// The cliff level of the cells within [`CLEARANCE`] of `(x, y)`, if all are open, pathable
/// ground of one level and no ramp.
fn open_level(world: &SimWorld, x: i32, y: i32) -> Option<u8> {
    let map = &world.map;
    let mut level = None;
    for dy in -CLEARANCE..=CLEARANCE {
        for dx in -CLEARANCE..=CLEARANCE {
            let (cx, cy) = (u32::try_from(x + dx).ok()?, u32::try_from(y + dy).ok()?);
            let cell = map.cell(cx, cy).ok()?;
            let open = cell.flags.contains(CellFlags::GROUND_PATHABLE)
                && !cell
                    .flags
                    .intersects(CellFlags::OCCUPIED_BY_STRUCTURE | CellFlags::UNPASSABLE_CLIFF)
                && cell.ramp.is_none();
            if !open || level.is_some_and(|level| level != cell.cliff_level.0) {
                return None;
            }
            level = Some(cell.cliff_level.0);
        }
    }
    level
}

/// The centre of cell `(x, y)`.
fn centre(x: i32, y: i32) -> Vec2 {
    Vec2::new(x as f32 + 0.5, y as f32 + 0.5)
}

/// The high and low cells of the cliff edge farthest from every local unit: open ground on both,
/// the high one level above, [`GAP`] cells apart along an axis.
fn cliff_edge(world: &SimWorld) -> (Vec2, Vec2) {
    let locals: Vec<Vec2> = world
        .entities
        .values()
        .filter(|unit| unit.owner == LOCAL_PLAYER)
        .map(|unit| Vec2::new(unit.position.x.to_f32(), unit.position.y.to_f32()))
        .collect();
    let (width, height) = (
        i32::try_from(world.map.width()).expect("invariant: a map fits i32"),
        i32::try_from(world.map.height()).expect("invariant: a map fits i32"),
    );
    let mut best: Option<(f32, Vec2, Vec2)> = None;
    for y in 0..height {
        for x in 0..width {
            let Some(high) = open_level(world, x, y) else {
                continue;
            };
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (lx, ly) = (x + dx * GAP, y + dy * GAP);
                if open_level(world, lx, ly).is_none_or(|low| low >= high) {
                    continue;
                }
                let (h, l) = (centre(x, y), centre(lx, ly));
                let distance = locals
                    .iter()
                    .map(|unit| unit.distance(h).min(unit.distance(l)))
                    .fold(f32::MAX, f32::min);
                if best.is_none_or(|(far, ..)| distance > far) {
                    best = Some((distance, h, l));
                }
            }
        }
    }
    let (_, high, low) = best.expect("the skirmish map has a cliff edge with open ground");
    (high, low)
}

/// Places the fight in `world`: an enemy soldier on the high side ordered to attack a local
/// soldier below, which holds and has health enough for every test. Runs no tick.
pub fn stage(world: &mut SimWorld) -> SnapshotStage {
    let (high, low) = cliff_edge(world);
    let kind = world
        .catalog
        .id_by_slug(SOLDIER)
        .expect("the catalog ships this unit");
    let attacker = world
        .spawn_unit(kind, ENEMY, sim_at(high))
        .expect("open high ground");
    let victim = world
        .spawn_unit(kind, LOCAL_PLAYER, sim_at(low))
        .expect("open low ground");
    let unit = world.entities.get_mut(&victim).expect("spawned");
    unit.max_hp = SimScalar::from_i32(100_000);
    unit.hp = unit.max_hp;
    unit.issue_order(UnitOrder::HoldPosition, false)
        .expect("a hold is admitted");
    world
        .entities
        .get_mut(&attacker)
        .expect("spawned")
        .issue_order(UnitOrder::attack(victim), false)
        .expect("an attack is admitted");
    SnapshotStage {
        attacker,
        victim,
        high,
        low,
    }
}
