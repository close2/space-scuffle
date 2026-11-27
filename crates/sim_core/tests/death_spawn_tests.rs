//! Death spawns through `SimWorld` (plan `2026-12-01-death-spawns-and-hangar-commands` §6.1,
//! DS-1 to DS-12; the countdown order and the checksum's field coverage are unit tests of
//! `world::death_spawns`, the placement search and clock draw of `world::spawns`).
//!
//! Every test authors the mechanism on the loaded catalog of its own world, on archetypes found
//! by property, never by slug (charter §3.3): the spawned unit is the unit the shipped impact
//! spawn creates (the Broodling analogue); hosts are weaponless structures of the footprint the
//! test needs. The authored spec is 5.0.14's `ZergBuildingDies9` (plan §2.2): 9 units, 9 ticks
//! after the death, at the death point with no search, each living 143 ticks plus 0 to 5, and
//! nothing from a structure under construction.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

use sim_core::catalog::UnitArchetype;
use sim_core::{Entity, SimWorld, SimulationDiagnostic};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, CombatEventVisual, DeathSpawnSpec, EntityId, MatchResult, PlayerCommand,
    PlayerId, PlayerSlot, RenderSnapshot, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    TickInputBundle, TimedLifeSpec, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);
const FOE: PlayerId = PlayerId(1);

/// 5.0.14 `ZergBuildingDies9` (plan §2.2 rules 1 to 4).
const COUNT: u8 = 9;
const DELAY: u16 = 9;
const LIFE: TimedLifeSpec = TimedLifeSpec {
    ticks: 143,
    random_extra_ticks: 5,
};

/// The unit the shipped impact spawn creates: the Broodling analogue, found by property.
fn spawned_unit(world: &SimWorld) -> Result<UnitTypeId, Box<dyn Error>> {
    let mut units: Vec<UnitTypeId> = world
        .catalog
        .archetypes()
        .flat_map(|a| [&a.weapon, &a.ground_weapon, &a.air_weapon])
        .flatten()
        .filter_map(|weapon| weapon.impact_spawn.map(|spawn| spawn.unit))
        .collect();
    units.sort();
    units
        .first()
        .copied()
        .ok_or_else(|| "the shipped catalog has an impact spawn".into())
}

/// The footprint of a structure archetype, in cells.
fn footprint(arch: &UnitArchetype) -> Option<(u32, u32)> {
    match arch.shape {
        ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
            ..
        } => Some((u32::from(footprint_w), u32::from(footprint_h))),
        ShapeKind::UnitSphere => None,
    }
}

/// The first weaponless structure (by id) of footprint `size` that is no gas extractor.
fn host_of(world: &SimWorld, size: (u32, u32)) -> Result<UnitTypeId, Box<dyn Error>> {
    let mut hosts: Vec<UnitTypeId> = world
        .catalog
        .archetypes()
        .filter(|a| {
            footprint(a) == Some(size)
                && a.weapon.is_none()
                && a.ground_weapon.is_none()
                && a.air_weapon.is_none()
                && !a.capabilities.is_gas_extractor
        })
        .map(|a| a.unit_type_id)
        .collect();
    hosts.sort();
    hosts
        .first()
        .copied()
        .ok_or_else(|| format!("the shipped catalog has a plain {size:?} structure").into())
}

/// The spec the tests author, creating `unit`.
fn brood(unit: UnitTypeId) -> DeathSpawnSpec {
    DeathSpawnSpec {
        unit,
        count: COUNT,
        delay_ticks: DELAY,
        search_range: SimScalar::ZERO,
        timed_life: Some(LIFE),
        disabled_while_constructing: true,
    }
}

/// A flat world of `side` cells with the spawned unit's type and a host type of footprint
/// `size` carrying [`brood`].
struct Field {
    world: SimWorld,
    unit: UnitTypeId,
    host: UnitTypeId,
}

fn field(side: u32, size: (u32, u32), seed: u64) -> Result<Field, Box<dyn Error>> {
    let map = GameMap::new(side, side, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let unit = spawned_unit(&world)?;
    let host = host_of(&world, size)?;
    world.catalog.get_mut(host)?.capabilities.death_spawn = Some(brood(unit));
    Ok(Field { world, unit, host })
}

impl Field {
    fn step_with(&mut self, commands: Vec<PlayerCommand>) -> RenderSnapshot {
        let tick = self.world.current_tick;
        self.world.step(&TickBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        })
    }

    fn step(&mut self) -> RenderSnapshot {
        self.step_with(Vec::new())
    }

    /// The living units of the spawned type, ascending by id.
    fn spawned(&self) -> Vec<&Entity> {
        self.world
            .entities
            .values()
            .filter(|e| e.unit_type_id == self.unit && e.is_alive())
            .collect()
    }

    /// Places a host of `owner` at cell `(x, y)` and returns its id and centre.
    fn host_at(
        &mut self,
        owner: PlayerId,
        x: u32,
        y: u32,
    ) -> Result<(EntityId, SimVec2), Box<dyn Error>> {
        let id = self.world.spawn_structure(self.host, owner, x, y)?;
        Ok((id, self.world.entities[&id].position))
    }

    /// Sets `id`'s life to 0, so the next step's reap removes it.
    fn kill(&mut self, id: EntityId) -> TestResult {
        self.world.entities.get_mut(&id).ok_or("a living unit")?.hp = SimScalar::ZERO;
        Ok(())
    }
}

/// Whether every order of `unit` is an attack automatic targeting chose: a created unit gets no
/// order of its own (S5), so any order it holds comes from acquisition.
fn only_acquired_orders(unit: &Entity) -> bool {
    unit.order_queue.iter().all(|order| {
        matches!(
            order,
            UnitOrder::AttackTarget {
                origin: AttackOrigin::Acquired,
                ..
            }
        )
    })
}

/// The first armed ground unit other than `not`, by id.
fn armed_ground(world: &SimWorld, not: UnitTypeId) -> Result<UnitTypeId, Box<dyn Error>> {
    let mut armed: Vec<UnitTypeId> = world
        .catalog
        .archetypes()
        .filter(|a| {
            footprint(a).is_none()
                && a.unit_type_id != not
                && a.mover_type == sim_types::MoverType::Ground
                && a.max_speed > SimScalar::ZERO
                && a.weapon.is_some()
                && a.capabilities.hangar.is_none()
                && a.capabilities.transport_capacity == 0
                && a.harvester.is_none()
        })
        .map(|a| a.unit_type_id)
        .collect();
    armed.sort();
    armed
        .first()
        .copied()
        .ok_or_else(|| "the shipped catalog has an armed ground unit".into())
}

/// DS-1: a structure killed on tick D by a player's attack leaves exactly 9 units of its spawn
/// archetype on D + 9 (none on D + 8), its owner's, with no order of their own, created at its
/// centre; one pending entry from D's reap to D + 8, none after.
#[test]
fn ds1_nine_units_appear_nine_ticks_after_the_death() -> TestResult {
    let mut f = field(64, (3, 3), 0xD5_01)?;
    let (host, centre) = f.host_at(FOE, 30, 30)?;
    f.world.entities.get_mut(&host).ok_or("host")?.hp = SimScalar::from_i32(30);
    let attacker_type = armed_ground(&f.world, f.unit)?;
    let attacker = f
        .world
        .spawn_unit(attacker_type, ME, centre - SimVec2::from_i32(3, 0))?;
    f.step();
    f.step_with(vec![PlayerCommand {
        player_id: ME,
        entities: vec![attacker],
        order: UnitOrder::attack(host),
        queue: false,
    }]);
    let mut death = None;
    for _ in 0..2000 {
        f.step();
        if !f.world.entities.contains_key(&host) {
            death = Some(f.world.current_tick);
            break;
        }
        assert_eq!(f.world.pending_death_spawns(), []);
    }
    let death = death.ok_or("the attack killed the host")?;
    for after in 0..u32::from(DELAY) {
        assert_eq!(f.world.current_tick, death + after);
        assert_eq!(
            f.world.pending_death_spawns().len(),
            1,
            "pending on D + {after}"
        );
        assert_eq!(f.spawned().len(), 0, "nothing yet on D + {after}");
        f.step();
    }
    assert_eq!(f.world.current_tick, death + u32::from(DELAY));
    assert_eq!(f.world.pending_death_spawns(), []);
    let created = f.spawned();
    assert_eq!(created.len(), usize::from(COUNT));
    for unit in created {
        assert_eq!(unit.owner, FOE);
        assert_eq!(unit.pos_at_tick_start, centre, "created at the centre");
        assert!(only_acquired_orders(unit), "no order of its own");
    }
    Ok(())
}

/// The first builder and a 2×2 or 3×3 weaponless structure of its faction it may raise with no
/// prerequisite, carrying [`brood`]: a world where a worker lays a foundation of it at (20, 20).
fn foundation_field(
    seed: u64,
    disabled: bool,
) -> Result<(Field, EntityId, EntityId), Box<dyn Error>> {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let unit = spawned_unit(&world)?;
    let builder = world
        .catalog
        .archetypes()
        .filter(|a| a.capabilities.can_construct)
        .min_by_key(|a| a.unit_type_id)
        .ok_or("a builder")?
        .clone();
    let host = world
        .catalog
        .archetypes()
        .filter(|a| {
            builder.can_raise(a)
                && matches!(footprint(a), Some((2, 2) | (3, 3)))
                && a.ground_weapon.is_none()
                && !a.capabilities.is_gas_extractor
        })
        .map(|a| a.unit_type_id)
        .min()
        .ok_or("a structure the builder raises")?;
    let arch = world.catalog.get_mut(host)?;
    arch.requires_structures.clear();
    arch.capabilities.death_spawn = Some(DeathSpawnSpec {
        disabled_while_constructing: disabled,
        ..brood(unit)
    });
    let worker = world.spawn_unit(builder.unit_type_id, ME, SimVec2::from_i32(14, 14))?;
    let slot = ME.slot().ok_or("a player")?;
    world.player_resources[slot].minerals = 5000;
    world.player_resources[slot].gas = 5000;
    let mut f = Field { world, unit, host };
    f.step_with(vec![PlayerCommand {
        player_id: ME,
        entities: vec![worker],
        order: UnitOrder::Build {
            structure_type: host,
            cell_x: 20,
            cell_y: 20,
        },
        queue: false,
    }]);
    for _ in 0..600 {
        let site = f
            .world
            .entities
            .values()
            .find(|e| e.unit_type_id == host)
            .map(|e| e.id);
        if let Some(site) = site {
            return Ok((f, worker, site));
        }
        f.step();
    }
    Err("the worker laid the foundation".into())
}

/// DS-2: a foundation killed queues and creates nothing; with the construction rule off
/// (scratch), it creates its units.
#[test]
fn ds2_a_foundation_leaves_nothing_unless_the_rule_is_off() -> TestResult {
    for (disabled, expected) in [(true, 0), (false, usize::from(COUNT))] {
        let (mut f, _, site) = foundation_field(0xD5_02, disabled)?;
        assert!(f.world.entities[&site].construction.is_some(), "premise");
        f.kill(site)?;
        f.step();
        assert!(!f.world.entities.contains_key(&site));
        assert_eq!(f.world.pending_death_spawns().len(), expected.min(1));
        for _ in 0..u32::from(DELAY) + 5 {
            f.step();
        }
        assert_eq!(f.spawned().len(), expected, "disabled: {disabled}");
    }
    Ok(())
}

/// DS-3: a cancelled foundation creates nothing. A cancel removes the foundation through the
/// reap, as a death (5.0.14's `Cancel` death type is not the `Remove` that `SDeathResponse`
/// leaves out, S2), so it is the construction rule of the authored spec (`DisableWhileUnder
/// Construction`, S1) that leaves nothing.
#[test]
fn ds3_a_cancelled_foundation_creates_nothing() -> TestResult {
    let (mut f, _, site) = foundation_field(0xD5_03, true)?;
    f.step_with(vec![PlayerCommand {
        player_id: ME,
        entities: vec![site],
        order: UnitOrder::CancelConstruction,
        queue: false,
    }]);
    assert!(!f.world.entities.contains_key(&site));
    assert_eq!(f.world.pending_death_spawns(), []);
    for _ in 0..u32::from(DELAY) + 5 {
        f.step();
    }
    assert_eq!(f.spawned().len(), 0);
    Ok(())
}

/// The extras drawn for the units of 23 hosts killed together on one tick (207 units), in
/// creation order, with each unit's death tick checked against its clock.
fn clock_extras(seed: u64) -> Result<Vec<u32>, Box<dyn Error>> {
    let mut f = field(96, (3, 3), seed)?;
    let mut hosts = Vec::new();
    for i in 0..23_u32 {
        hosts.push(f.host_at(FOE, 4 + (i % 6) * 15, 4 + (i / 6) * 15)?.0);
    }
    for host in hosts {
        f.kill(host)?;
    }
    f.step();
    let death = f.world.current_tick;
    let born = death + u32::from(DELAY);
    while f.world.current_tick < born {
        f.step();
    }
    let created: Vec<(EntityId, u32)> = f
        .spawned()
        .iter()
        .map(|e| Ok((e.id, u32::from(e.timed_life().ok_or("a clock")?))))
        .collect::<Result<_, Box<dyn Error>>>()?;
    assert_eq!(created.len(), 23 * usize::from(COUNT));
    let mut deaths: BTreeMap<EntityId, u32> = BTreeMap::new();
    let ids: BTreeSet<EntityId> = created.iter().map(|(id, _)| *id).collect();
    while f.world.current_tick < born + 143 + 6 {
        let out = f.step();
        for id in &ids {
            if !f.world.entities.contains_key(id) && !deaths.contains_key(id) {
                deaths.insert(*id, f.world.current_tick);
            }
        }
        if f.world.current_tick == born + 143 {
            let shattered = out.combat_events.iter().any(|event| {
                matches!(event, CombatEventVisual::BubbleShatter { unit_type, .. }
                    if *unit_type == f.unit)
            });
            assert!(shattered, "the clock's deaths are ordinary deaths");
        }
    }
    let mut extras = Vec::new();
    for (id, life) in created {
        assert!(life >= 143, "a clock of at least 143");
        assert_eq!(deaths.get(&id), Some(&(born + life)), "dies on its tick");
        extras.push(life - 143);
    }
    Ok(extras)
}

/// DS-4: each created unit dies on D + 9 + 143 + d, d in 0 to 5, with no damage source (no
/// enemy is on the map) and its ordinary death; over 207 units every d of 0 to 5 occurs and no
/// other; the same seed gives the same d's.
#[test]
fn ds4_the_clock_kills_on_its_tick_with_every_extra_drawn() -> TestResult {
    let extras = clock_extras(0xD5_04)?;
    for d in 0..=5 {
        assert!(extras.contains(&d), "the extra {d} occurs");
    }
    assert!(extras.iter().all(|d| *d <= 5));
    assert_eq!(extras, clock_extras(0xD5_04)?);
    Ok(())
}

/// Two hosts dying on one tick and a third three ticks later: (creation tick, host centre) per
/// created unit, ascending by id, and each unit's clock.
/// One created unit: its id, creation tick, creation point and clock.
type Created = (EntityId, u32, SimVec2, u16);

fn ordering(seed: u64) -> Result<Vec<Created>, Box<dyn Error>> {
    let mut f = field(64, (3, 3), seed)?;
    let (low, _) = f.host_at(FOE, 10, 10)?;
    let (high, _) = f.host_at(FOE, 30, 10)?;
    let (late, _) = f.host_at(FOE, 10, 30)?;
    f.kill(high)?;
    f.kill(low)?;
    f.step();
    let first = f.world.current_tick;
    f.step();
    f.step();
    f.kill(late)?;
    f.step();
    assert_eq!(f.world.current_tick, first + 3);
    let mut seen = BTreeMap::new();
    while f.world.current_tick < first + 3 + u32::from(DELAY) + 2 {
        f.step();
        for unit in f.spawned() {
            seen.entry(unit.id).or_insert((
                f.world.current_tick,
                unit.pos_at_tick_start,
                unit.timed_life().ok_or("a clock")?,
            ));
        }
    }
    Ok(seen
        .into_iter()
        .map(|(id, (tick, point, life))| (id, tick, point, life))
        .collect())
}

/// DS-5: two hosts dying on one tick create in reap order (the lower host id's units first);
/// one dying three ticks later creates three ticks later; the same seed gives the same ids and
/// draws.
#[test]
fn ds5_spawns_create_in_reap_order_with_deterministic_ids_and_draws() -> TestResult {
    let created = ordering(0xD5_05)?;
    assert_eq!(created.len(), 3 * usize::from(COUNT));
    let n = usize::from(COUNT);
    let first = created[0].1;
    let centres: Vec<SimVec2> = created.iter().map(|c| c.2).collect();
    assert!(created[..2 * n].iter().all(|c| c.1 == first));
    assert!(created[2 * n..].iter().all(|c| c.1 == first + 3));
    assert!(centres[..n].iter().all(|c| *c == centres[0]));
    assert!(centres[n..2 * n].iter().all(|c| *c == centres[n]));
    assert!(
        centres[0].x < centres[n].x,
        "the lower host id (placed first, at x = 10) creates first"
    );
    assert_eq!(created, ordering(0xD5_05)?);
    Ok(())
}

/// DS-6: for footprints 2×2, 3×3 and 5×5, a host killed on open ground creates every unit at
/// its centre with no `DeathSpawnBlocked`; a host whose centre is walled off during the delay
/// reports `DeathSpawnBlocked` and creates nothing.
#[test]
fn ds6_units_are_placed_at_the_centre_or_reported_blocked() -> TestResult {
    for size in [(2, 2), (3, 3), (5, 5)] {
        let mut f = field(48, size, 0xD5_06)?;
        let (host, centre) = f.host_at(FOE, 20, 20)?;
        f.kill(host)?;
        let mut blocked = false;
        for _ in 0..=DELAY {
            f.step();
            blocked |= f
                .world
                .diagnostics()
                .iter()
                .any(|d| matches!(d, SimulationDiagnostic::DeathSpawnBlocked { .. }));
        }
        assert!(!blocked, "{size:?}: the centre is free");
        let created = f.spawned();
        assert_eq!(created.len(), usize::from(COUNT), "{size:?}");
        assert!(created.iter().all(|u| u.pos_at_tick_start == centre));
        assert!(
            created.iter().all(|u| u.order_queue.is_empty()),
            "no enemy, no order"
        );
    }
    let mut f = field(48, (3, 3), 0xD5_06)?;
    let (host, _) = f.host_at(FOE, 20, 20)?;
    f.kill(host)?;
    f.step();
    // A wall over the dead host's cells during the delay: another host on the same cells.
    f.host_at(ME, 20, 20)?;
    let mut reports = 0;
    for _ in 0..DELAY {
        f.step();
        reports += f
            .world
            .diagnostics()
            .iter()
            .filter(|d| matches!(d, SimulationDiagnostic::DeathSpawnBlocked { .. }))
            .count();
    }
    assert_eq!(
        reports,
        usize::from(COUNT),
        "one report per unit not created"
    );
    assert_eq!(f.spawned().len(), 0);
    Ok(())
}

/// DS-7: nine coincident created units on open ground are pairwise apart (edge distance at
/// least −0.01) within 25 ticks, with no movement invariant reported.
#[test]
fn ds7_nine_coincident_units_separate_within_25_ticks() -> TestResult {
    let mut f = field(48, (3, 3), 0xD5_07)?;
    let (host, _) = f.host_at(FOE, 20, 20)?;
    f.kill(host)?;
    for _ in 0..=DELAY {
        f.step();
    }
    assert_eq!(f.spawned().len(), usize::from(COUNT), "premise");
    let radius = f.world.catalog.get(f.unit)?.collision_radius;
    let tolerance = SimScalar::from_num(-0.01);
    let mut apart_at = None;
    for tick in 0..25 {
        f.step();
        assert!(
            !f.world
                .diagnostics()
                .iter()
                .any(|d| matches!(d, SimulationDiagnostic::MovementInvariant { .. })),
            "no movement invariant on tick {tick}"
        );
        let units = f.spawned();
        let apart = units.iter().enumerate().all(|(i, a)| {
            units[i + 1..]
                .iter()
                .all(|b| a.position.distance(b.position) - radius - radius >= tolerance)
        });
        if apart && apart_at.is_none() {
            apart_at = Some(tick + 1);
        }
    }
    let apart_at = apart_at.ok_or("the nine separated within 25 ticks")?;
    assert!(apart_at <= 25, "apart after {apart_at} ticks");
    Ok(())
}

/// Makes `id` take 100,000 life, so a test's fight never kills it.
fn durable(world: &mut SimWorld, id: EntityId) -> TestResult {
    let unit = world.entities.get_mut(&id).ok_or("a living unit")?;
    unit.max_hp = SimScalar::from_i32(100_000);
    unit.hp = unit.max_hp;
    Ok(())
}

/// DS-8: the created units acquire an enemy in their scan range on their creation tick or the
/// next and attack it; an enemy's automatic targeting attacks them; their owner's move order to
/// one of them is admitted.
#[test]
fn ds8_created_units_fight_and_obey() -> TestResult {
    let mut f = field(64, (3, 3), 0xD5_08)?;
    let (host, centre) = f.host_at(FOE, 30, 30)?;
    let enemy_type = armed_ground(&f.world, f.unit)?;
    let enemy = f
        .world
        .spawn_unit(enemy_type, ME, centre + SimVec2::from_i32(5, 0))?;
    f.world
        .entities
        .get_mut(&enemy)
        .ok_or("enemy")?
        .issue_order(UnitOrder::HoldPosition, false)?;
    durable(&mut f.world, enemy)?;
    f.kill(host)?;
    for _ in 0..DELAY {
        f.step();
        assert_eq!(f.spawned().len(), 0);
    }
    f.step();
    let born = f.world.current_tick;
    let mut acquired_by = None;
    let mut targeted = false;
    for _ in 0..60 {
        let ids: Vec<EntityId> = f.spawned().iter().map(|u| u.id).collect();
        if acquired_by.is_none() && f.spawned().iter().all(|u| u.current_target == Some(enemy)) {
            acquired_by = Some(f.world.current_tick);
        }
        targeted |= f.world.entities[&enemy]
            .current_target
            .is_some_and(|t| ids.contains(&t));
        f.step();
    }
    let acquired_by = acquired_by.ok_or("the created units acquired the enemy")?;
    assert!(
        acquired_by <= born + 1,
        "acquired on {acquired_by}, born {born}"
    );
    assert!(
        f.world.entities[&enemy].hp < SimScalar::from_i32(100_000),
        "they attacked it"
    );
    assert!(targeted, "the enemy's automatic targeting takes them");
    let unit = f.spawned().first().map(|u| u.id).ok_or("a created unit")?;
    f.step_with(vec![PlayerCommand {
        player_id: FOE,
        entities: vec![unit],
        order: UnitOrder::Move {
            target: centre - SimVec2::from_i32(10, 0),
        },
        queue: false,
    }]);
    assert!(matches!(
        f.world
            .entities
            .get(&unit)
            .and_then(|u| u.order_queue.front()),
        Some(UnitOrder::Move { .. })
    ));
    Ok(())
}

/// DS-9: the host is its owner's last structure: the owner is defeated at D's reap, the units are
/// still created on D + 9, and the result does not change.
#[test]
fn ds9_an_owner_defeated_by_the_death_still_gets_the_units() -> TestResult {
    let mut f = field(64, (3, 3), 0xD5_09)?;
    let (host, _) = f.host_at(FOE, 30, 30)?;
    f.host_at(ME, 4, 4)?;
    f.step();
    assert_eq!(f.world.match_state.participants.count_ones(), 2, "premise");
    f.kill(host)?;
    f.step();
    let result = f.world.match_state.result;
    assert!(
        matches!(result, MatchResult::Victory { winner, .. } if winner == ME),
        "{result:?}"
    );
    for _ in 0..DELAY {
        f.step();
    }
    assert_eq!(f.spawned().len(), usize::from(COUNT));
    assert!(f.spawned().iter().all(|u| u.owner == FOE));
    assert_eq!(f.world.match_state.result, result);
    Ok(())
}

/// DS-10: created units capture no supply; the supply ledger validates on every tick of a fight
/// in which hosts die and their units fight and expire.
#[test]
fn ds10_created_units_take_no_supply() -> TestResult {
    let mut f = field(64, (3, 3), 0xD5_10)?;
    let slot = PlayerSlot::ALL[1];
    let (a, centre) = f.host_at(FOE, 20, 20)?;
    let (b, _) = f.host_at(FOE, 40, 20)?;
    let enemy_type = armed_ground(&f.world, f.unit)?;
    f.world
        .spawn_unit(enemy_type, ME, centre + SimVec2::from_i32(4, 0))?;
    let used = f.world.player_resources[slot].supply_used;
    f.kill(a)?;
    f.kill(b)?;
    let mut created = BTreeSet::new();
    for _ in 0..200 {
        f.step();
        f.world.validate_supply_ledger()?;
        assert_eq!(f.world.player_resources[slot].supply_used, used);
        created.extend(f.spawned().iter().map(|u| u.id));
    }
    assert_eq!(created.len(), 2 * usize::from(COUNT));
    Ok(())
}

/// DS-11: the shipped catalog validates (an omitted `death_spawn` reads `None` on every row
/// without one; the D-1 rules are refused field by field in the catalog's unit tests).
#[test]
fn ds11_the_shipped_catalog_validates() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    catalogs.units.validate_production_references()?;
    Ok(())
}

/// The per-tick checksums of a constructed fight in which hosts die and their units fight,
/// driven by `bundles`.
fn fight_checksums(bundles: &[TickInputBundle]) -> Result<(Vec<u64>, usize), Box<dyn Error>> {
    let mut f = field(64, (3, 3), 0xD5_12)?;
    let (a, centre) = f.host_at(FOE, 20, 20)?;
    let (b, _) = f.host_at(FOE, 26, 20)?;
    f.host_at(FOE, 40, 40)?;
    f.host_at(ME, 4, 40)?;
    let enemy_type = armed_ground(&f.world, f.unit)?;
    let attacker = f
        .world
        .spawn_unit(enemy_type, ME, centre - SimVec2::from_i32(4, 0))?;
    durable(&mut f.world, attacker)?;
    f.world.entities.get_mut(&a).ok_or("a")?.hp = SimScalar::from_i32(20);
    f.world.entities.get_mut(&b).ok_or("b")?.hp = SimScalar::from_i32(20);
    let mut sums = Vec::new();
    let mut created = BTreeSet::new();
    for bundle in bundles {
        f.world.step(bundle);
        sums.push(f.world.last_checksum());
        created.extend(f.spawned().iter().map(|u| u.id));
    }
    Ok((sums, created.len()))
}

/// DS-12 (second half; the first is `world::death_spawns`' unit test): a constructed fight
/// recorded, serialized to RON, read back and replayed gives the same checksum on every tick.
#[test]
fn ds12_a_recorded_fight_replays_to_the_same_checksums() -> TestResult {
    let ids = {
        let f = field(64, (3, 3), 0xD5_12)?;
        f.world.next_entity_id
    };
    // Entity ids as `fight_checksums` creates them: hosts a, b, c, d, then the attacker.
    let a = EntityId(ids);
    let b = EntityId(ids + 1);
    let attacker = EntityId(ids + 4);
    let mut bundles = Vec::new();
    for tick in 0..400_u32 {
        let commands = match tick {
            2 => vec![PlayerCommand {
                player_id: ME,
                entities: vec![attacker],
                order: UnitOrder::attack(a),
                queue: false,
            }],
            60 => vec![PlayerCommand {
                player_id: ME,
                entities: vec![attacker],
                order: UnitOrder::attack(b),
                queue: true,
            }],
            _ => Vec::new(),
        };
        bundles.push(TickInputBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        });
    }
    let recorded = fight_checksums(&bundles)?;
    assert_eq!(
        recorded.1,
        2 * usize::from(COUNT),
        "premise: both hosts left their units"
    );
    let text = ron::to_string(&bundles)?;
    let read: Vec<TickInputBundle> = ron::from_str(&text)?;
    assert_eq!(read, bundles);
    assert_eq!(fight_checksums(&read)?, recorded);
    Ok(())
}

/// DS-15 (plan §6.2): the shipped structure with the largest death spawn, found by property,
/// killed by a constructed army through its owner's attack order: DS-1 holds with the shipped
/// values (that many units of the shipped spawn, its delay after the death, at the centre, the
/// structure's owner's), and the created units fight the army.
#[test]
fn ds15_a_shipped_structure_leaves_its_broodlings_which_fight() -> TestResult {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let world = sim_fixtures::world(map, 0xD5_15);
    let (host, spec) = world
        .catalog
        .archetypes()
        .filter_map(|a| {
            a.capabilities
                .death_spawn
                .map(|spec| (a.unit_type_id, spec))
        })
        .max_by_key(|(id, spec)| (spec.count, std::cmp::Reverse(*id)))
        .ok_or("a shipped structure with a death spawn")?;
    let mut f = Field {
        world,
        unit: spec.unit,
        host,
    };
    let (structure, centre) = f.host_at(FOE, 28, 28)?;
    f.world.entities.get_mut(&structure).ok_or("host")?.hp = SimScalar::from_i32(60);
    let soldier = armed_ground(&f.world, f.unit)?;
    let mut army = Vec::new();
    for dy in [-2, 0, 2] {
        let at = centre - SimVec2::from_i32(5, dy);
        let id = f.world.spawn_unit(soldier, ME, at)?;
        durable(&mut f.world, id)?;
        army.push(id);
    }
    f.step();
    f.step_with(vec![PlayerCommand {
        player_id: ME,
        entities: army.clone(),
        order: UnitOrder::attack(structure),
        queue: false,
    }]);
    let mut death = None;
    for _ in 0..2000 {
        f.step();
        if !f.world.entities.contains_key(&structure) {
            death = Some(f.world.current_tick);
            break;
        }
    }
    let death = death.ok_or("the army killed the structure")?;
    while f.world.current_tick < death + u32::from(spec.delay_ticks) - 1 {
        f.step();
        assert_eq!(f.spawned().len(), 0);
    }
    f.step();
    let created = f.spawned();
    assert_eq!(created.len(), usize::from(spec.count));
    assert!(created
        .iter()
        .all(|u| u.owner == FOE && u.pos_at_tick_start == centre));
    let mut fought = false;
    for _ in 0..100 {
        f.step();
        fought |= army
            .iter()
            .any(|id| f.world.entities.get(id).is_some_and(|u| u.hp < u.max_hp));
    }
    assert!(fought, "the created units struck the army");
    Ok(())
}

/// DS-6 for the shipped hosts (plan §6.1, once WPA2 authors them): every shipped archetype with
/// a death spawn, found by property, placed on open ground and killed, creates every unit at its
/// centre with no `DeathSpawnBlocked`.
#[test]
fn ds6_every_shipped_host_leaves_its_units_at_its_centre() -> TestResult {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
    let shipped: Vec<(UnitTypeId, DeathSpawnSpec)> = sim_fixtures::world(map, 0)
        .catalog
        .archetypes()
        .filter_map(|a| {
            a.capabilities
                .death_spawn
                .map(|spec| (a.unit_type_id, spec))
        })
        .collect();
    assert!(!shipped.is_empty(), "premise: shipped hosts");
    for (host, spec) in shipped {
        let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
        let mut f = Field {
            world: sim_fixtures::world(map, 0xD5_06),
            unit: spec.unit,
            host,
        };
        let (structure, centre) = f.host_at(FOE, 20, 20)?;
        f.kill(structure)?;
        for _ in 0..=spec.delay_ticks {
            f.step();
            assert!(
                !f.world
                    .diagnostics()
                    .iter()
                    .any(|d| matches!(d, SimulationDiagnostic::DeathSpawnBlocked { .. })),
                "{host:?}: the centre is free"
            );
        }
        let created = f.spawned();
        assert_eq!(created.len(), usize::from(spec.count), "{host:?}");
        assert!(
            created.iter().all(|u| u.pos_at_tick_start == centre),
            "{host:?}"
        );
    }
    Ok(())
}
