//! Stages an attack snapshot on the shipped skirmish (plans `2026-11-24-attacker-snapshot` and
//! `2026-11-27-launch-and-spell-visibility`).
//!
//! # Responsibility
//! Finds a cliff edge of the shipped skirmish map, far from the local player's units, and places
//! a ranged enemy soldier on its high side and a tough local soldier holding below it, in the
//! enemy's weapon reach: the enemy strikes from where the local player does not see it, which
//! leaves the local player an attack snapshot (`PlayerView::attack_snapshots`). Two variants
//! stage the other causes of an image: [`stage_missile`] places a missile soldier, whose release
//! lists it before its missile lands, and [`stage_caster`] places a holding caster out of its
//! weapon's reach of the local soldier, whose storm ([`cast_storm`]) lists it.
//!
//! # How tests use it
//! Include it with `#[path = "support/attack_snapshot_stage.rs"]` and call [`stage`] (or a
//! variant) on the session's world (through the harness's `world_mut`, as the harness allows, or
//! on a world before its session is prepared). It is setup: no order under test is issued here.
//!
//! # Invariants
//! - The spot is found by reading the map (cliff levels and open ground), never by coordinates
//!   written here, and the unit by its catalog slug in test data, never in production code.

#![allow(clippy::expect_used, clippy::cast_precision_loss)]

use bevy::prelude::*;
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{
    AbilitySource, EffectContext, EffectTarget, EntityId, PlayerId, SimScalar, SimVec2, UnitOrder,
};

/// The local player of the skirmish: its first participant.
pub const LOCAL_PLAYER: PlayerId = PlayerId(0);

/// The enemy of the local player in the skirmish.
pub const ENEMY: PlayerId = PlayerId(1);

/// A ranged soldier with shipped media; its weapon keeps the default `Snapshot` visibility.
pub const SOLDIER: &str = "pulse_bot";

/// A missile soldier with shipped media; its release keeps the default `Snapshot` launch
/// visibility, and its missile flies several ticks across [`GAP`].
pub const MISSILE_SOLDIER: &str = "rhino_beetle";

/// The caster of the staged storm: a unit with shipped media. No shipped spellcaster has media
/// yet, and the client refuses to present a unit without it, so the storm's walk is started with
/// this soldier as its caster; the image shows the caster whatever made it (plan
/// `2026-11-27-launch-and-spell-visibility` D-3), so the pool is tested all the same.
pub const CASTER: &str = SOLDIER;

/// The storm's effect key in `ability_effects.ron`.
pub const STORM: &str = "psionic_storm";

/// Cells between the high and the low soldier: inside each soldier's reach (5 from edge to edge
/// for [`SOLDIER`], 4 for [`MISSILE_SOLDIER`]).
const GAP: i32 = 5;

/// Cells between the caster and the soldier below it: outside the caster's weapon reach (5 from
/// edge to edge), so it never strikes; only its storm leaves an image.
const CASTER_GAP: i32 = 8;

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
/// the high one level above, `gap` cells apart along an axis.
fn cliff_edge(world: &SimWorld, gap: i32) -> (Vec2, Vec2) {
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
                let (lx, ly) = (x + dx * gap, y + dy * gap);
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
    stage_with(world, SOLDIER)
}

/// [`stage`] with a [`MISSILE_SOLDIER`] on the high side: its release lists it to the local
/// player before its missile lands (plan `2026-11-27-launch-and-spell-visibility` D-2).
pub fn stage_missile(world: &mut SimWorld) -> SnapshotStage {
    stage_with(world, MISSILE_SOLDIER)
}

/// [`stage`] with the enemy unit `slug` on the high side.
fn stage_with(world: &mut SimWorld, slug: &str) -> SnapshotStage {
    let (high, low) = cliff_edge(world, GAP);
    let attacker_kind = world
        .catalog
        .id_by_slug(slug)
        .expect("the catalog ships this unit");
    let victim_kind = world
        .catalog
        .id_by_slug(SOLDIER)
        .expect("the catalog ships this unit");
    let attacker = world
        .spawn_unit(attacker_kind, ENEMY, sim_at(high))
        .expect("open high ground");
    let victim = world
        .spawn_unit(victim_kind, LOCAL_PLAYER, sim_at(low))
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

/// Places a holding enemy [`CASTER`] on the high side of a cliff edge, out of its weapon's reach
/// of a tough local soldier holding below it, which does not see it. Runs no tick; the caster
/// casts nothing until [`cast_storm`].
pub fn stage_caster(world: &mut SimWorld) -> SnapshotStage {
    let (high, low) = cliff_edge(world, CASTER_GAP);
    let caster_kind = world
        .catalog
        .id_by_slug(CASTER)
        .expect("the catalog ships this unit");
    let victim_kind = world
        .catalog
        .id_by_slug(SOLDIER)
        .expect("the catalog ships this unit");
    let attacker = world
        .spawn_unit(caster_kind, ENEMY, sim_at(high))
        .expect("open high ground");
    let victim = world
        .spawn_unit(victim_kind, LOCAL_PLAYER, sim_at(low))
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
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("a hold is admitted");
    SnapshotStage {
        attacker,
        victim,
        high,
        low,
    }
}

/// Starts the [`STORM`] of the staged caster on the local soldier now, the walk a cast starts:
/// its first hit lists the caster to the local player (plan
/// `2026-11-27-launch-and-spell-visibility` D-2).
pub fn cast_storm(world: &mut SimWorld, staged: &SnapshotStage) {
    let root = world
        .catalog
        .spells()
        .effect_id(STORM)
        .expect("the catalog ships the storm");
    let caster = world.entities[&staged.attacker].position;
    world.run_effect(
        root,
        EffectContext {
            owner: ENEMY,
            caster: Some(staged.attacker),
            ability: AbilitySource::default(),
            origin: caster,
            source_point: caster,
            target: EffectTarget::Point(sim_at(staged.low)),
        },
    );
}
