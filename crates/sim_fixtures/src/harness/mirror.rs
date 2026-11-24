//! The mirror family of the movement harness: point-symmetric battles of shipped armies (M10).
//!
//! # Responsibility
//! [`generate`] draws a random composition of one or two shipped ground archetypes (each of
//! supply 1 to 4) from a seed and places it on the west half of a 64 × 64 map; the east side gets
//! the same army at the point reflection of every position (plan `2026-11-20-contact-solver`
//! §6.2). Blue, spawned first, has the lower entity ids. [`run`] plays the battle: both sides
//! attack-move to the other's centroid, again every 100 ticks, for at most 3,000 ticks, with the
//! random weapon delays removed, and reports who won, the visible reversals (M1) and D-R10, the
//! unit-ticks spent standing on an automatic chase while another enemy had room (plan Amendment
//! 17).
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
use sim_core::{SimWorld, UnitCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2,
    Tick, TickBundle, UnitOrder, UnitTypeId,
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
    /// D-R10 (plan `2026-11-20-contact-solver` Amendment 17): unit-ticks a unit stood on an
    /// automatic chase out of reach of its target while another enemy in its scan range had room
    /// on its ring ([`stands_with_room`]). Reported, not a threshold.
    pub dr10: u32,
}

impl MirrorRow {
    /// The CSV header, in [`Self::to_csv`] order.
    pub const HEADER: &'static str =
        "family,seed,winner,ticks,blue_left,red_left,rev_amp,diagnostics,dr10";

    /// The row as one CSV line.
    #[must_use]
    pub fn to_csv(&self) -> String {
        format!(
            "mirror,{},{},{},{},{},{},{},{}",
            self.seed,
            self.winner,
            self.ticks,
            self.blue_left,
            self.red_left,
            self.rev_amp,
            self.diagnostics,
            self.dr10
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
        row.dr10 = row.dr10.saturating_add(standing_with_room(world, &last));
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
    row.winner = match (row.blue_left > 0, row.red_left > 0) {
        (true, false) => 0,
        (false, true) => 1,
        _ => 2,
    };
    row
}

/// One living unit at the start of the tick just played: id, owner, centre (world units) and
/// collision radius.
type Body = (EntityId, PlayerId, Point, f64);

/// D-R10 for the tick just played: how many units stood on an automatic chase with room
/// elsewhere ([`stands_with_room`]). `last` holds every unit's centre at the tick's start.
fn standing_with_room(world: &SimWorld, last: &[(EntityId, Point, Option<Point>, f64)]) -> u32 {
    let bodies: Vec<Body> = last
        .iter()
        .filter_map(|&(id, before, _, _)| {
            let entity = world.entities.get(&id).filter(|e| e.is_alive())?;
            let radius = world
                .catalog
                .get(entity.unit_type_id)
                .ok()?
                .collision_radius;
            Some((id, entity.owner, before, to_f64(radius)))
        })
        .collect();
    let standing = bodies
        .iter()
        .filter(|&&body| stands_with_room(world, &bodies, body))
        .count();
    u32::try_from(standing).unwrap_or(u32::MAX)
}

/// The band's outer edge (centre to centre, world units) at which `entity` reaches `enemy`, or
/// `None` when it has no weapon against it.
fn band_against(
    world: &SimWorld,
    entity: &sim_core::Entity,
    enemy: &sim_core::Entity,
) -> Option<f64> {
    let arch = world.catalog.get(entity.unit_type_id).ok()?;
    let enemy_arch = world.catalog.get(enemy.unit_type_id).ok()?;
    let weapon = arch.weapon_against(enemy_arch.mover_type, entity.active_siege_weapon())?;
    Some(to_f64(
        weapon.max_range + arch.collision_radius + enemy_arch.collision_radius,
    ))
}

/// Whether `body` stood this tick (it did not move and has no velocity) on an automatic chase
/// (`AttackTarget` with `AttackOrigin::Acquired`) whose target lies beyond its band, while some
/// other enemy within its scan range (edge to edge, at least its weapon's range) had room on the
/// ring the ring rule reads round it (radius band − r / 2), all at the tick's start positions.
fn stands_with_room(world: &SimWorld, bodies: &[Body], body: Body) -> bool {
    let (id, owner, before, r) = body;
    let Some(entity) = world.entities.get(&id) else {
        return false;
    };
    let Some(&UnitOrder::AttackTarget {
        target,
        origin: AttackOrigin::Acquired,
    }) = entity.order_queue.front()
    else {
        return false;
    };
    if point(entity.position) != before || entity.velocity != SimVec2::ZERO {
        return false;
    }
    let distance = |a: Point, b: Point| (a.0 - b.0).hypot(a.1 - b.1);
    let start = |wanted: EntityId| bodies.iter().find(|b| b.0 == wanted).copied();
    let (Some(target_body), Some(target_entity)) = (start(target), world.entities.get(&target))
    else {
        return false;
    };
    if band_against(world, entity, target_entity)
        .is_none_or(|band| distance(before, target_body.2) <= band)
    {
        return false;
    }
    let Ok(arch) = world.catalog.get(entity.unit_type_id) else {
        return false;
    };
    bodies
        .iter()
        .filter(|other| other.1 != owner && other.0 != target)
        .any(|&(enemy, _, at, radius)| {
            let Some(band) = world
                .entities
                .get(&enemy)
                .and_then(|e| band_against(world, entity, e))
            else {
                return false;
            };
            let reach = band - r - radius;
            let scan = to_f64(arch.scan_range).max(reach);
            if distance(before, at) - r - radius > scan {
                return false;
            }
            let others: Vec<(Point, f64)> = bodies
                .iter()
                .filter(|b| b.0 != enemy && b.0 != id)
                .map(|b| (b.2, b.3))
                .collect();
            super::room::ring_has_room(at, band - r / 2.0, r, &others)
        })
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
