//! # Integration Test: Psionic Storm (plan `2026-11-10-spellcaster-mechanics` WP2)
//!
//! The shipped High Templar analogue's Psionic Storm, cast through `SimWorld::execute_ability`
//! and played through `SimWorld::step`, against the `StarCraft` II 5.0.14 rules of plan §3.1 and
//! §6.1: a point persistent searching 1.5 around its centre on the cast tick and six more
//! times 10.2 ticks apart, applying one non-stacking status that deals 10 at once and 10 every
//! 10.196 ticks for 11.9 ticks, as spell damage (shields first, armour ignored), to every unit
//! of any owner, ground or air, cloaked or burrowed, but never to structures or carried units.
//!
//! The tests read the shipped catalog (`units.ron`, `ability_effects.ron`, `statuses.ron`,
//! `upgrades.ron`), so a data edit that breaks a rule fails here. Units are named by slug in
//! this test only (charter §3.3 governs production code).

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySource, EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder,
};

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The storm's ability name in the shipped catalog, which the upgrade unlocks by name.
const STORM: &str = "Psionic Storm";

/// A 64 × 64 flat world on the shipped catalogs, with Psionic Storm researched for blue.
fn world() -> SimWorld {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a flat map");
    let mut world = sim_fixtures::world(map, 0x5707);
    world
        .research_upgrade(BLUE, "psionic_storm")
        .expect("the shipped research exists");
    world
}

fn at(x: f32, y: f32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn n(value: i32) -> SimScalar {
    SimScalar::from_i32(value)
}

/// Spawns the shipped unit `slug` for `owner` at `(x, y)`.
fn spawn(world: &mut SimWorld, slug: &str, owner: PlayerId, x: f32, y: f32) -> EntityId {
    let kind = world.catalog.id_by_slug(slug).expect("a shipped unit");
    world
        .spawn_unit(kind, owner, at(x, y))
        .expect("invariant: a free spot")
}

/// Spawns `slug` for `owner` at `(x, y)` holding position, so it neither flees nor walks.
fn held(world: &mut SimWorld, slug: &str, owner: PlayerId, x: f32, y: f32) -> EntityId {
    let id = spawn(world, slug, owner, x, y);
    world
        .entities
        .get_mut(&id)
        .expect("spawned")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold admitted");
    id
}

/// Spawns a High Templar analogue for blue at `(x, y)` with full energy.
fn templar(world: &mut SimWorld, x: f32, y: f32) -> EntityId {
    let id = spawn(world, "cerebral_elder", BLUE, x, y);
    world.entities.get_mut(&id).expect("spawned").energy = n(200);
    id
}

/// The index of the storm in `caster`'s archetype.
fn storm_index(world: &SimWorld, caster: EntityId) -> u8 {
    let kind = world.entities[&caster].unit_type_id;
    let arch = world.catalog.get(kind).expect("registered");
    let index = arch
        .abilities
        .iter()
        .position(|ability| ability.name == STORM)
        .expect("the High Templar analogue has Psionic Storm");
    u8::try_from(index).expect("few abilities")
}

/// Casts the storm by `caster` at `(x, y)` now; whether it was admitted.
fn cast(world: &mut SimWorld, caster: EntityId, x: f32, y: f32) -> bool {
    let index = storm_index(world, caster);
    world.execute_ability(caster, index, Some(at(x, y)), None)
}

/// Steps one tick with `commands`; panics on a diagnostic.
fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands,
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

fn step(world: &mut SimWorld) {
    step_with(world, Vec::new());
}

/// Life plus shields of `id`, 0 once it is gone.
fn vitals(world: &SimWorld, id: EntityId) -> SimScalar {
    world
        .entities
        .get(&id)
        .map_or(SimScalar::ZERO, |e| e.hp + e.shields)
}

/// Steps `ticks` ticks and returns `(ticks since start, loss)` of every tick on which `id`
/// lost life or shields.
fn losses(world: &mut SimWorld, id: EntityId, ticks: u32) -> Vec<(u32, SimScalar)> {
    let start = world.current_tick;
    let mut last = vitals(world, id);
    let mut out = Vec::new();
    for _ in 0..ticks {
        step(world);
        let now = vitals(world, id);
        if now < last {
            out.push((world.current_tick - start, last - now));
        }
        last = now;
    }
    out
}

/// The pulse ticks of plan §3.1 after the initial damage on the cast tick.
const PULSES: [u32; 7] = [10, 20, 31, 41, 51, 61, 71];

fn ten_at(ticks: &[u32]) -> Vec<(u32, SimScalar)> {
    ticks.iter().map(|&tick| (tick, n(10))).collect()
}

#[test]
fn a_unit_inside_throughout_takes_80_on_the_storm_ticks_armour_ignored() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    // Armour 1 and no regeneration: every point shows.
    let victim = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    let start = vitals(&world, victim);
    assert!(cast(&mut world, caster, 16.0, 20.0));
    assert_eq!(start - vitals(&world, victim), n(10), "10 on the cast tick");
    assert_eq!(losses(&mut world, victim, 100), ten_at(&PULSES));
    assert_eq!(start - vitals(&world, victim), n(80));
    assert!(world.effects().statuses().is_empty(), "the status expired");
}

#[test]
fn overlapping_storms_do_not_stack() {
    let mut world = world();
    let first = templar(&mut world, 7.5, 20.0);
    let second = templar(&mut world, 7.5, 22.0);
    let victim = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    let start = vitals(&world, victim);
    assert!(cast(&mut world, first, 16.0, 20.0));
    step(&mut world);
    step(&mut world);
    assert!(cast(&mut world, second, 16.5, 20.0));
    assert_eq!(world.effects().persistents().len(), 2);
    assert_eq!(
        start - vitals(&world, victim),
        n(10),
        "no second initial damage"
    );
    let rest = losses(&mut world, victim, 120);
    // Counted from the second cast, two ticks after the first. The second storm's searches only
    // refresh the one status, which keeps the first application's phase: its pulses at 10, 20,
    // 31, …, 71 after the first cast. The second storm's last search (tick 63) keeps it until
    // 74.9, so the pulse at 81 never comes: one storm's 80.
    assert_eq!(rest, ten_at(&[8, 18, 29, 39, 49, 59, 69]));
    assert_eq!(start - vitals(&world, victim), n(80));
}

#[test]
fn a_unit_that_leaves_after_the_first_search_takes_20() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    let victim = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    let start = vitals(&world, victim);
    assert!(cast(&mut world, caster, 16.0, 20.0));
    // Carried out of the radius at once: the status outlives the search by 1.7 ticks, so its
    // first pulse lands ("a unit can't receive only a single tick", plan §3.1).
    world.entities.get_mut(&victim).expect("alive").position = at(30.0, 20.0);
    assert_eq!(losses(&mut world, victim, 100), ten_at(&[10]));
    assert_eq!(start - vitals(&world, victim), n(20));
}

#[test]
fn shields_go_first_and_armour_never_applies() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    // 100 life, 50 shields, life armour 1.
    let victim = held(&mut world, "psi_grey", RED, 16.0, 20.0);
    let (life, shields) = {
        let e = &world.entities[&victim];
        (e.hp, e.shields)
    };
    assert!(cast(&mut world, caster, 16.0, 20.0));
    losses(&mut world, victim, 100);
    let e = &world.entities[&victim];
    assert_eq!(e.shields, shields - n(50).min(shields));
    assert_eq!(e.hp, life - (n(80) - shields));
}

#[test]
fn own_and_air_units_are_hit_and_structures_are_not() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    let own = held(&mut world, "shock_golem", BLUE, 15.5, 19.5);
    let air = held(&mut world, "vector_interceptor", RED, 16.5, 20.5);
    let dome = world
        .catalog
        .id_by_slug("capacitor_dome")
        .expect("a shipped structure");
    // A 2 × 2 dome on cells 16 and 17: its centre (17, 21) lies 1.1 from the storm's.
    let structure = world
        .spawn_structure(dome, BLUE, 16, 20)
        .expect("a free site");
    let starts = [own, air, structure].map(|id| vitals(&world, id));
    assert!(cast(&mut world, caster, 16.5, 20.0));
    for _ in 0..100 {
        step(&mut world);
    }
    assert_eq!(starts[0] - vitals(&world, own), n(80), "friendly fire");
    assert_eq!(starts[1] - vitals(&world, air), n(80), "air units");
    assert_eq!(vitals(&world, structure), starts[2], "structures untouched");
}

#[test]
fn cloaked_and_burrowed_units_are_hit_without_detection() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    let cloaked = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    world.entities.get_mut(&cloaked).expect("alive").is_cloaked = true;
    let burrower = spawn(&mut world, "burrow_scorpion", RED, 16.5, 21.0);
    let burrow = {
        let kind = world.entities[&burrower].unit_type_id;
        let arch = world.catalog.get(kind).expect("registered");
        let index = arch
            .abilities
            .iter()
            .position(|ability| match &ability.effect {
                AbilityEffectSpec::SwitchMode { mode_name } => {
                    arch.get_mode(mode_name).is_some_and(|mode| mode.burrowed)
                }
                _ => false,
            })
            .expect("a burrowing ability");
        u8::try_from(index).expect("few abilities")
    };
    assert!(world.execute_ability(burrower, burrow, None, None));
    for _ in 0..40 {
        step(&mut world);
    }
    assert!(world.entities[&burrower].is_concealed(), "burrowed");
    assert!(world.entities[&cloaked].is_concealed(), "cloaked");
    let starts = [cloaked, burrower].map(|id| vitals(&world, id));
    assert!(cast(&mut world, caster, 16.0, 20.5));
    for _ in 0..100 {
        step(&mut world);
    }
    assert_eq!(starts[0] - vitals(&world, cloaked), n(80));
    // The burrowed unit regenerates a little between pulses; it took the storm.
    assert!(starts[1] - vitals(&world, burrower) > n(78));
}

#[test]
fn carried_units_are_untouched_and_their_carrier_is_hit() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    let carrier = held(&mut world, "repair_shuttle", RED, 16.0, 20.0);
    let passenger = spawn(&mut world, "pulse_bot", RED, 17.0, 20.0);
    world
        .try_load_unit_into_transport(passenger, carrier)
        .expect("the shuttle carries the bot");
    let starts = [carrier, passenger].map(|id| vitals(&world, id));
    assert!(cast(&mut world, caster, 16.0, 20.0));
    for _ in 0..100 {
        step(&mut world);
    }
    assert_eq!(starts[0] - vitals(&world, carrier), n(80));
    assert_eq!(vitals(&world, passenger), starts[1]);
    assert_eq!(world.effects().statuses_of(passenger).len(), 0);
}

#[test]
fn a_barrier_absorbs_storm_damage_first() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    // The shipped Immortal analogue: 100 absorbed within 54 ticks of the first hit.
    let victim = held(&mut world, "void_goliath", RED, 16.0, 20.0);
    let start = vitals(&world, victim);
    assert!(cast(&mut world, caster, 16.0, 20.0));
    assert_eq!(vitals(&world, victim), start, "the first pulse is absorbed");
    let rest = losses(&mut world, victim, 100);
    // Pulses at 0 … 51 (60 in all) fall inside the raised barrier; 61 and 71 land.
    assert_eq!(rest, ten_at(&[61, 71]));
}

#[test]
fn held_units_stay_and_an_idle_unit_that_cannot_answer_flees_the_centre() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    let holder = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    // No weapon: it cannot strike back at the caster.
    let idle = spawn(&mut world, "repair_shuttle", RED, 16.5, 20.0);
    let holder_start = world.entities[&holder].position;
    assert!(cast(&mut world, caster, 16.0, 20.0));
    for _ in 0..30 {
        step(&mut world);
    }
    assert_eq!(world.entities[&holder].position, holder_start);
    let fled = world.entities[&idle].position;
    assert!(
        fled.distance(at(16.0, 20.0)) > SimScalar::from_num(1.5),
        "fled out of the radius to {fled:?}"
    );
    assert!(fled.x > SimScalar::from_num(16.5), "away from the centre");
}

#[test]
fn an_armed_idle_unit_strikes_back_at_the_caster() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    let victim = spawn(&mut world, "pulse_bot", RED, 16.0, 20.0);
    // One tick so red sees the caster (an answer needs a visible attacker).
    step(&mut world);
    assert!(cast(&mut world, caster, 16.0, 20.0));
    let entity = &world.entities[&victim];
    assert!(
        entity.current_target == Some(caster)
            || matches!(entity.order_queue.front(), Some(UnitOrder::AttackTarget { target, .. }) if *target == caster),
        "the victim answers the caster: {:?} {:?}",
        entity.current_target,
        entity.order_queue.front()
    );
}

#[test]
fn refused_without_research_or_energy_and_during_the_36_tick_cooldown() {
    let mut world = world();
    let unresearched = spawn(&mut world, "cerebral_elder", RED, 40.0, 40.0);
    world
        .entities
        .get_mut(&unresearched)
        .expect("spawned")
        .energy = n(200);
    assert!(
        !cast(&mut world, unresearched, 44.0, 40.0),
        "needs the research"
    );

    let caster = templar(&mut world, 7.5, 20.0);
    world.entities.get_mut(&caster).expect("spawned").energy = SimScalar::from_num(74.9);
    assert!(!cast(&mut world, caster, 16.0, 20.0), "needs 75 energy");
    world.entities.get_mut(&caster).expect("spawned").energy = n(150);
    assert!(cast(&mut world, caster, 16.0, 20.0));
    let entity = &world.entities[&caster];
    assert_eq!(entity.energy, n(75), "75 paid");
    let index = usize::from(storm_index(&world, caster));
    assert_eq!(entity.ability_cooldowns[index], 36);
    for _ in 0..35 {
        step(&mut world);
        let index = storm_index(&world, caster);
        assert!(!world.would_admit_cast(caster, index, Some(at(16.0, 22.0)), None));
    }
    step(&mut world);
    assert!(cast(&mut world, caster, 16.0, 22.0), "ready after 36 ticks");
}

#[test]
fn cast_range_is_nine_from_the_casters_edge() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    let index = storm_index(&world, caster);
    // 9 + the caster's radius 0.375.
    let reach = SimScalar::from_num(9.375);
    let inside = SimVec2::new(SimScalar::from_num(7.5) + reach, n(20));
    let outside = SimVec2::new(inside.x + SimScalar::from_num(0.01), n(20));
    assert!(!world.would_admit_cast(caster, index, Some(outside), None));
    assert!(world.would_admit_cast(caster, index, Some(inside), None));
    assert!(world.execute_ability(caster, index, Some(inside), None));
}

#[test]
fn the_caster_is_locked_for_nine_ticks_after_the_cast() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    let index = storm_index(&world, caster);
    step_with(
        &mut world,
        vec![PlayerCommand {
            player_id: BLUE,
            entities: vec![caster],
            order: UnitOrder::CastAbility {
                ability_index: index,
                target_pos: Some(at(16.0, 20.0)),
                target_entity: None,
            },
            queue: false,
        }],
    );
    assert_eq!(world.effects().persistents().len(), 1, "cast on the tick");
    let start = world.entities[&caster].position;
    step_with(
        &mut world,
        vec![PlayerCommand {
            player_id: BLUE,
            entities: vec![caster],
            order: UnitOrder::Move {
                target: at(10.0, 40.0),
            },
            queue: false,
        }],
    );
    // The cast tick and the eight after it are locked (plan Amendment 2: a lock of 9 set on
    // tick t holds t … t + 8); the move starts on the tenth tick.
    for _ in 2..9 {
        step(&mut world);
    }
    assert_eq!(world.entities[&caster].position, start, "locked");
    step(&mut world);
    step(&mut world);
    assert_ne!(
        world.entities[&caster].position, start,
        "free after the lock"
    );
}

#[test]
fn the_storm_is_an_area_from_its_cast_to_its_last_search() {
    let mut world = world();
    let caster = templar(&mut world, 7.5, 20.0);
    let index = storm_index(&world, caster);
    let kind = world.entities[&caster].unit_type_id;
    assert!(cast(&mut world, caster, 16.0, 20.0));
    let mut published = 0;
    for _ in 0..80 {
        let snapshot = world.build_render_snapshot();
        let Some(area) = snapshot.areas.first() else {
            break;
        };
        assert!((area.radius - 1.5).abs() < 1e-6);
        assert_eq!(area.total_ticks, 61);
        assert_eq!(area.owner, BLUE);
        assert_eq!(area.center[..2], [16.0, 20.0]);
        assert_eq!(
            area.source,
            Some(AbilitySource {
                unit_type: kind,
                ability: index,
            })
        );
        published += 1;
        step(&mut world);
    }
    assert_eq!(
        published, 61,
        "shown from the cast tick until it closes at 61"
    );
}
