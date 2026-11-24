//! The mirror family of the movement harness: point-symmetric battles of shipped armies (M10).
//!
//! # Responsibility
//! [`generate`] draws a random composition of one or two shipped ground archetypes (each of
//! supply 1 to 4) from a seed and places it on the west half of a 64 × 64 map; the east side gets
//! the same army at the point reflection of every position (plan `2026-11-20-contact-solver`
//! §6.2). Blue, spawned first, has the lower entity ids. [`run`] plays the battle: both sides
//! attack-move to the other's centroid, again every 100 ticks, for at most 3,000 ticks, with the
//! random weapon delays removed, and reports who won, the visible reversals (M1), and two contact
//! checks of plan `2026-11-24-solver-retarget-and-shoving` (§5.1): D-SHOVE, the distance units
//! were moved by enemy contact while proposing no motion, and D-R10, the unit-ticks spent standing
//! on an automatic chase while another enemy had room, in its four columns
//! ([`crate::trace::contact_checks`]); and, appended last, D-SHOVE-E of plan
//! `2026-11-25-enemy-contact-and-ring-retarget` §6, the outward part of that distance only.
//!
//! # How it fits
//! Archetypes are chosen by their properties from the shipped catalog, never by name (charter
//! §3.3). [`super::report`] reads blue's share of the decisive games with its Wilson interval
//! (T9): a pipeline that does not depend on entity order gives blue half of them.
//!
//! # Invariants
//! - `generate(seed, catalog)` depends on nothing else.
//! - Red's units are blue's, in the same order, at `(64, 64) − p`.

use super::corpus::{permute, Lattice, SplitMix64, LATTICE};
use super::metrics::{point, reverses, to_f64, Point};
use super::{Family, HarnessError, RunOptions};
use crate::trace::contact_checks::ContactMeter;
use sim_core::{SimWorld, UnitCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId,
};

/// The map's side, cells.
const SIDE: u32 = 64;

/// Ticks between re-issued attack-moves.
const REISSUE_TICKS: u32 = 100;

/// The battle's tick limit.
pub const LIMIT_TICKS: u32 = 3000;

/// One drawn mirror battle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorScene {
    /// The seed.
    pub seed: u64,
    /// Blue's units in spawn order: archetype and centre in sixty-fourths. Red's are their point
    /// reflections.
    pub blue: Vec<(UnitTypeId, (Lattice, Lattice))>,
}

/// One mirror battle's line of the harness's CSV.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MirrorRow {
    /// The seed.
    pub seed: u64,
    /// 0 when blue won, 1 when red won, 2 for a draw (both died, or the limit).
    pub winner: u8,
    /// Ticks played.
    pub ticks: u32,
    /// Blue's living units at the end.
    pub blue_left: u32,
    /// Red's living units at the end.
    pub red_left: u32,
    /// M1: visible reversals over both sides.
    pub rev_amp: u32,
    /// Simulation diagnostics raised.
    pub diagnostics: u32,
    /// D-SHOVE over both sides, world units. Reported, not a threshold.
    pub shove: f64,
    /// D-R10 over both sides, unit-ticks: leash, terrain, unruled, free
    /// ([`crate::harness::room::RoomClass`]). Reported, not a threshold.
    pub dr10: [u32; 4],
    /// D-SHOVE-E over both sides, world units. Reported, not a threshold.
    pub shove_e: f64,
}

impl MirrorRow {
    /// The CSV header, in [`Self::to_csv`] order.
    pub const HEADER: &'static str =
        "family,seed,winner,ticks,blue_left,red_left,rev_amp,diagnostics,shove,dr10_leash,\
dr10_terrain,dr10_unruled,dr10_free,shove_e";

    /// The row as one CSV line.
    #[must_use]
    pub fn to_csv(&self) -> String {
        format!(
            "mirror,{},{},{},{},{},{},{},{:.4},{},{},{},{},{:.4}",
            self.seed,
            self.winner,
            self.ticks,
            self.blue_left,
            self.red_left,
            self.rev_amp,
            self.diagnostics,
            self.shove,
            self.dr10[0],
            self.dr10[1],
            self.dr10[2],
            self.dr10[3],
            self.shove_e
        )
    }
}

/// The shipped archetypes a mirror army is drawn from, by id: mobile ground units with a weapon
/// against ground, no harvesting, supply 1 to 4.
#[must_use]
pub fn eligible(catalog: &UnitCatalog) -> Vec<UnitTypeId> {
    let mut ids: Vec<UnitTypeId> = catalog
        .archetypes()
        .filter(|a| {
            a.mover_type == MoverType::Ground
                && !a.is_immobile_by_default()
                && !matches!(a.shape, ShapeKind::BuildingDome { .. })
                && a.harvester.is_none()
                && (1..=4).contains(&a.supply_cost)
                && a.weapon_against(MoverType::Ground, None).is_some()
        })
        .map(|a| a.unit_type_id)
        .collect();
    ids.sort_unstable();
    ids
}

/// The mirror battle of `seed` drawn from `catalog`'s [`eligible`] archetypes.
///
/// # Errors
/// [`HarnessError::Scene`] when the catalog has no eligible archetype.
pub fn generate(seed: u64, catalog: &UnitCatalog) -> Result<MirrorScene, HarnessError> {
    let pool = eligible(catalog);
    if pool.is_empty() {
        return Err(HarnessError::Scene(
            "no shipped archetype is eligible for the mirror",
        ));
    }
    let mut rng = SplitMix64::for_scenario(Family::Mirror, seed, 0);
    let kinds = if pool.len() > 1 && rng.one_in(2) {
        2
    } else {
        1
    };
    let mut chosen = pool.clone();
    permute(&mut chosen, &mut rng);
    chosen.truncate(kinds);
    let mut army = Vec::new();
    let mut largest = SimScalar::ZERO;
    for &kind in &chosen {
        let archetype = catalog
            .get(kind)
            .map_err(|_| HarnessError::Scene("missing archetype"))?;
        let count = ((4 + rng.below(12)) / u64::from(archetype.supply_cost.max(1))).max(1);
        army.extend(std::iter::repeat_n(
            kind,
            usize::try_from(count).unwrap_or(1),
        ));
        largest = largest.max(archetype.collision_radius);
    }
    // Slots on a lattice whose pitch clears the largest body, inside x 10..22, y 20..44.
    let radius = (largest.raw() >> 26) + 1;
    let pitch = 2 * radius + 16;
    let mut slots = Vec::new();
    let mut y = 20 * LATTICE;
    while y <= 44 * LATTICE {
        let mut x = 10 * LATTICE;
        while x <= 22 * LATTICE {
            slots.push((x, y));
            x += pitch;
        }
        y += pitch;
    }
    permute(&mut slots, &mut rng);
    let blue = army.into_iter().zip(slots).collect();
    Ok(MirrorScene { seed, blue })
}

/// A world point from sixty-fourths.
fn at(position: (Lattice, Lattice)) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_raw(position.0 << 26),
        SimScalar::from_raw(position.1 << 26),
    )
}

/// Plays the mirror battle of `seed` and reports it.
///
/// # Errors
/// [`HarnessError`] when the world cannot be built or a unit cannot be spawned.
pub fn run(seed: u64, options: &RunOptions) -> Result<MirrorRow, HarnessError> {
    let mut catalogs = crate::shipped_catalogs();
    crate::without_random_delays(&mut catalogs.units);
    catalogs.units.set_movement_rules(options.rules);
    let scene = generate(seed, &catalogs.units)?;
    let map = GameMap::new(SIDE, SIDE, MapCell::default_ground(CliffLevel(0)))
        .map_err(|e| HarnessError::World(e.to_string()))?;
    let mut world = SimWorld::with_catalogs(map, seed, catalogs.units, catalogs.upgrades)
        .map_err(|e| HarnessError::World(e.to_string()))?;
    let far = Lattice::from(SIDE) * LATTICE;
    let mut sides: [Vec<EntityId>; 2] = [Vec::new(), Vec::new()];
    for (owner, side) in sides.iter_mut().enumerate() {
        for &(kind, (x, y)) in &scene.blue {
            let position = if owner == 0 {
                (x, y)
            } else {
                (far - x, far - y)
            };
            let player = PlayerId(u8::try_from(owner).unwrap_or(0));
            let id = world
                .spawn_unit(kind, player, at(position))
                .map_err(|e| HarnessError::World(e.to_string()))?;
            side.push(id);
        }
    }
    Ok(play(&mut world, &sides, seed))
}

/// Plays the battle between `sides` to its end and measures it.
fn play(world: &mut SimWorld, sides: &[Vec<EntityId>; 2], seed: u64) -> MirrorRow {
    let mut row = MirrorRow {
        seed,
        ..MirrorRow::default()
    };
    let mut last: Vec<(EntityId, Point, Option<Point>, f64)> = sides
        .iter()
        .flatten()
        .filter_map(|id| {
            let entity = world.entities.get(id)?;
            let speed = world.catalog.get(entity.unit_type_id).ok()?.max_speed;
            Some((*id, point(entity.position), None, to_f64(speed)))
        })
        .collect();
    let mut contact = ContactMeter::new();
    contact.observe(world);
    for tick in 0..LIMIT_TICKS {
        let living = |side: &[EntityId], world: &SimWorld| -> Vec<EntityId> {
            side.iter()
                .copied()
                .filter(|id| {
                    world
                        .entities
                        .get(id)
                        .is_some_and(sim_core::Entity::is_alive)
                })
                .collect()
        };
        let (blue, red) = (living(&sides[0], world), living(&sides[1], world));
        if blue.is_empty() || red.is_empty() {
            break;
        }
        let mut commands = Vec::new();
        if tick.is_multiple_of(REISSUE_TICKS) {
            for (player, own, other) in [(0_u8, &blue, &red), (1, &red, &blue)] {
                commands.push(PlayerCommand {
                    player_id: PlayerId(player),
                    entities: own.clone(),
                    order: UnitOrder::AttackMove {
                        target: centroid(world, other),
                    },
                    queue: false,
                });
            }
        }
        world.step(&TickBundle {
            tick: Tick(world.current_tick),
            commands,
            state_checksum: None,
        });
        row.ticks += 1;
        row.diagnostics = row
            .diagnostics
            .saturating_add(u32::try_from(world.drain_diagnostics().count()).unwrap_or(u32::MAX));
        contact.observe(world);
        for (id, position, step, speed) in &mut last {
            let Some(entity) = world.entities.get(id).filter(|e| e.is_alive()) else {
                continue;
            };
            let now = point(entity.position);
            let moved = (now.0 - position.0, now.1 - position.1);
            if let Some(before) = *step {
                row.rev_amp += u32::from(reverses(before, moved, *speed / 4.0, -0.5));
            }
            *step = Some(moved);
            *position = now;
        }
    }
    let count = |side: &[EntityId]| {
        u32::try_from(
            side.iter()
                .filter(|id| {
                    world
                        .entities
                        .get(id)
                        .is_some_and(sim_core::Entity::is_alive)
                })
                .count(),
        )
        .unwrap_or(u32::MAX)
    };
    row.blue_left = count(&sides[0]);
    row.red_left = count(&sides[1]);
    let checks = contact.total();
    for side in &checks.sides {
        row.shove += side.shove;
        row.shove_e += side.shove_e;
        for (total, count) in row.dr10.iter_mut().zip(side.r10) {
            *total = total.saturating_add(count);
        }
    }
    row.winner = match (row.blue_left > 0, row.red_left > 0) {
        (true, false) => 0,
        (false, true) => 1,
        _ => 2,
    };
    row
}

/// The centroid of `units`' positions, exact in Q32.32 up to the division's rounding.
fn centroid(world: &SimWorld, units: &[EntityId]) -> SimVec2 {
    let (mut x, mut y, mut n) = (0_i128, 0_i128, 0_i128);
    for id in units {
        if let Some(entity) = world.entities.get(id) {
            x += i128::from(entity.position.x.raw());
            y += i128::from(entity.position.y.raw());
            n += 1;
        }
    }
    if n == 0 {
        return SimVec2::ZERO;
    }
    let raw = |sum: i128| SimScalar::from_raw(i64::try_from(sum / n).unwrap_or(0));
    SimVec2::new(raw(x), raw(y))
}
