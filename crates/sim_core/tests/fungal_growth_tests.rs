//! # Integration Test: Fungal Growth (plan `2026-11-10-spellcaster-mechanics` WP4)
//!
//! The shipped Infestor analogue's Fungal Growth, cast through `SimWorld::execute_ability` and
//! played through `SimWorld::step`, against the `StarCraft` II 5.0.14 rules of plan §3.3 and
//! §6.3: a missile flying 0.672 world units a tick to the target point, whose impact searches
//! 2.25 around it for enemies of the caster only (ground and air, never structures or carried
//! units) and applies two statuses to each, `fungal_growth` (1.5625 every 4.464 ticks, 16 pulses
//! for 25, revealing cloaked and burrowed units, forbidding Blink) and `fungal_growth_slow`
//! (movement × 0.25, no boarding), both for 71.43 ticks; a recast refreshes them with their phase
//! kept. The caster pays 75 energy; there is no cooldown and no lock.
//!
//! The tests read the shipped catalog (`units.ron`, `ability_effects.ron`, `statuses.ron`), so a
//! data edit that breaks a rule fails here. Units are named by slug in this test only (charter
//! §3.3 governs production code). The Infestor analogue has no weapon, so it never hits a victim.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySource, AbilitySpec, AbilityTargetKind, EntityId, PlayerCommand,
    PlayerId, SimScalar, SimVec2, StatusKind, Tick, TickBundle, UnitOrder,
};

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The ability's name in the shipped catalog.
const FUNGAL: &str = "Fungal Growth";

/// The pulse ticks of plan §3.3, counted from the impact: `round_half_up(k × 4.464286)`.
const PULSES: [u32; 16] = [4, 9, 13, 18, 22, 27, 31, 36, 40, 45, 49, 54, 58, 63, 67, 71];

/// Ticks after the impact on which both statuses are gone (71.43 elapsed, so the 72nd).
const EXPIRY: u32 = 72;

fn world() -> SimWorld {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a flat map");
    sim_fixtures::world(map, 0xF0_6A1)
}

fn at(x: f32, y: f32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn n(value: i32) -> SimScalar {
    SimScalar::from_i32(value)
}

/// One pulse of `FungalGrowthDamage` (`voidmulti` EffectData:2389).
fn pulse() -> SimScalar {
    SimScalar::from_num(1.5625)
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

/// Spawns an Infestor analogue for blue at `(x, y)` with full energy.
fn infestor(world: &mut SimWorld, x: f32, y: f32) -> EntityId {
    let id = spawn(world, "mycelium_shaman", BLUE, x, y);
    world.entities.get_mut(&id).expect("spawned").energy = n(200);
    id
}

/// The index of Fungal Growth in `caster`'s archetype.
fn fungal_index(world: &SimWorld, caster: EntityId) -> u8 {
    let kind = world.entities[&caster].unit_type_id;
    let arch = world.catalog.get(kind).expect("registered");
    let index = arch
        .abilities
        .iter()
        .position(|ability| ability.name == FUNGAL)
        .expect("the Infestor analogue has Fungal Growth");
    u8::try_from(index).expect("few abilities")
}

/// Casts Fungal Growth by `caster` at `(x, y)` now; whether it was admitted.
fn fungal(world: &mut SimWorld, caster: EntityId, x: f32, y: f32) -> bool {
    let index = fungal_index(world, caster);
    world.execute_ability(caster, index, Some(at(x, y)), None)
}

/// Steps one tick; panics on a diagnostic.
fn step(world: &mut SimWorld) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands: Vec::new(),
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

fn steps(world: &mut SimWorld, count: u32) {
    for _ in 0..count {
        step(world);
    }
}

/// Whether `id` carries the shipped status `name`.
fn has(world: &SimWorld, id: EntityId, name: &str) -> bool {
    let status = world
        .catalog
        .spells()
        .status_id(name)
        .expect("a shipped status");
    world.effects().has_status(id, status)
}

fn fungaled(world: &SimWorld, id: EntityId) -> bool {
    has(world, id, "fungal_growth")
}

/// Steps until `id` carries `fungal_growth` (at most 40 ticks); returns the steps taken.
fn step_to_impact(world: &mut SimWorld, id: EntityId) -> u32 {
    (1..=40)
        .find(|_| {
            step(world);
            fungaled(world, id)
        })
        .expect("the missile lands within 40 ticks")
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

fn pulses_at(ticks: &[u32]) -> Vec<(u32, SimScalar)> {
    ticks.iter().map(|&tick| (tick, pulse())).collect()
}

#[test]
fn the_missile_lands_ceil_distance_over_speed_later_and_is_published_while_it_flies() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    let index = fungal_index(&world, caster);
    let kind = world.entities[&caster].unit_type_id;
    let victim = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    assert_eq!(world.effects().missiles().len(), 1, "launched on the cast");
    let snapshot = world.build_render_snapshot();
    let missile = snapshot
        .spell_missiles
        .first()
        .expect("a published missile");
    assert_eq!(missile.owner, BLUE);
    assert_eq!(
        missile.source,
        AbilitySource {
            unit_type: kind,
            ability: index,
        }
    );
    // 8.5 at 0.672 a tick (FungalGrowthWeapon MaxSpeed 12, void MoverData:18): 12.65, so 13
    // ticks after the cast tick. A cast between ticks is stepped first in the next tick, as a
    // cast in that tick's phase 1 would be: that tick is the cast tick, so the impact comes on
    // the 14th step (the convention of `effect_tree_tests.rs`).
    assert_eq!(step_to_impact(&mut world, victim), 1 + 13);
    assert!(world.effects().missiles().is_empty(), "the missile is gone");
    assert!(
        world.build_render_snapshot().spell_missiles.is_empty(),
        "no longer published"
    );
}

#[test]
fn a_unit_inside_takes_25_in_16_pulses_on_the_fungal_ticks_armour_ignored() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    // Armour 1 and no regeneration: every point shows.
    let victim = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    let start = vitals(&world, victim);
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    step_to_impact(&mut world, victim);
    assert_eq!(vitals(&world, victim), start, "no damage at the impact");
    assert_eq!(losses(&mut world, victim, 100), pulses_at(&PULSES));
    assert_eq!(start - vitals(&world, victim), n(25));
    assert!(
        world.effects().statuses().is_empty(),
        "both statuses expired"
    );
}

#[test]
fn the_search_takes_enemies_within_2_25_ground_and_air_but_no_own_unit_or_structure() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    // Units without weapons, so nothing but the spell touches them.
    let inside = held(&mut world, "mycelium_shaman", RED, 18.2, 20.0);
    let outside = held(&mut world, "mycelium_shaman", RED, 13.7, 20.0);
    let own = held(&mut world, "mycelium_shaman", BLUE, 16.0, 21.2);
    let air = held(&mut world, "repair_shuttle", RED, 16.0, 20.0);
    let dome = world
        .catalog
        .id_by_slug("capacitor_dome")
        .expect("a shipped structure");
    // A 2 × 2 dome on cells 15 and 16, 17 and 18: its centre (16, 18) lies 2 from the impact.
    let structure = world
        .spawn_structure(dome, RED, 15, 17)
        .expect("a free site");
    let starts = [inside, outside, own, structure].map(|id| vitals(&world, id));
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    step_to_impact(&mut world, inside);
    assert!(fungaled(&world, air), "air units");
    assert!(has(&world, inside, "fungal_growth_slow"), "2.2 away: hit");
    for id in [outside, own, structure] {
        assert!(
            world.effects().statuses_of(id).is_empty(),
            "{id:?} untouched"
        );
    }
    steps(&mut world, 80);
    // The Infestor analogue regenerates between pulses; it took the 25.
    assert!(starts[0] - vitals(&world, inside) > n(23), "2.2 away: hit");
    assert_eq!(vitals(&world, outside), starts[1], "2.3 away: missed");
    assert_eq!(vitals(&world, own), starts[2], "own units never");
    assert_eq!(vitals(&world, structure), starts[3], "structures never");
}

#[test]
fn carried_units_are_untouched_and_their_carrier_is_hit() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    let carrier = held(&mut world, "repair_shuttle", RED, 16.0, 20.0);
    let passenger = spawn(&mut world, "pulse_bot", RED, 17.0, 20.0);
    world
        .try_load_unit_into_transport(passenger, carrier)
        .expect("the shuttle carries the bot");
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    step_to_impact(&mut world, carrier);
    assert!(
        world.effects().statuses_of(passenger).is_empty(),
        "carried: hidden from the search"
    );
}

#[test]
fn movement_is_slowed_to_a_quarter_over_a_straight_walk() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 30.0);
    let slowed = spawn(&mut world, "pulse_bot", RED, 14.0, 25.0);
    let free = spawn(&mut world, "pulse_bot", RED, 14.0, 40.0);
    for (id, y) in [(slowed, 25.0), (free, 40.0)] {
        world
            .entities
            .get_mut(&id)
            .expect("spawned")
            .issue_order(
                UnitOrder::Move {
                    target: at(60.0, y),
                },
                false,
            )
            .expect("a move");
    }
    // Up to speed, then a cast where the walker will be when the missile lands.
    steps(&mut world, 10);
    let x = world.entities[&slowed].position.x.to_f32();
    assert!(fungal(&mut world, caster, x + 1.5, 25.0));
    step_to_impact(&mut world, slowed);
    // Deceleration settles within a few ticks; measure ten ticks after it.
    steps(&mut world, 5);
    let start = [slowed, free].map(|id| world.entities[&id].position);
    steps(&mut world, 10);
    let [walked_slowed, walked_free] = [slowed, free].map(|id| {
        world.entities[&id]
            .position
            .distance(start[usize::from(id == free)])
    });
    let ratio = walked_slowed.to_f32() / walked_free.to_f32();
    assert!(
        (ratio - 0.25).abs() < 0.01,
        "speed x0.25 (FungalGrowthSlowMovement): {walked_slowed:?} against {walked_free:?}"
    );
    let snapshot = world.build_render_snapshot();
    let status = snapshot
        .bubbles
        .iter()
        .find(|bubble| bubble.entity_id == slowed)
        .expect("published")
        .status;
    assert!(status.has(StatusKind::Slowed) && status.has(StatusKind::DamageOverTime));
    assert!(status.has(StatusKind::Revealed));
}

/// Gives every archetype of `slug` a scratch Blink of range 8 (no shipped unit has one); returns
/// its ability index.
fn scratch_blink(world: &mut SimWorld, slug: &str) -> u8 {
    let kind = world.catalog.id_by_slug(slug).expect("a shipped unit");
    let arch = world.catalog.get_mut(kind).expect("listed");
    arch.abilities.push(AbilitySpec {
        name: "Scratch Blink".into(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 0,
        cast_range: n(8),
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: n(8),
            cooldown_ticks: 0,
        },
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    });
    u8::try_from(arch.abilities.len() - 1).expect("few abilities")
}

#[test]
fn blink_and_boarding_are_refused_until_both_statuses_expire_on_the_same_tick() {
    let mut world = world();
    let blink = scratch_blink(&mut world, "pulse_bot");
    let caster = infestor(&mut world, 7.5, 20.0);
    let victim = held(&mut world, "pulse_bot", RED, 16.0, 20.0);
    let other = held(&mut world, "pulse_bot", RED, 16.0, 30.0);
    // 3.5 from the impact: outside the search.
    let shuttle = held(&mut world, "repair_shuttle", RED, 16.0, 23.5);
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    step_to_impact(&mut world, victim);
    assert!(has(&world, victim, "fungal_growth_slow"));
    assert!(
        world.effects().statuses_of(shuttle).is_empty(),
        "the shuttle is outside the search"
    );
    assert!(
        !world.execute_ability(victim, blink, Some(at(20.0, 20.0)), None),
        "no Blink (CasterNotFungalGrowthed, swarm EffectData:2180-2182)"
    );
    assert!(world.execute_ability(other, blink, Some(at(20.0, 30.0)), None));
    assert!(
        world.try_load_unit_into_transport(victim, shuttle).is_err(),
        "no boarding (SuppressPassenger, voidmulti BehaviorData:698)"
    );
    // Both last 71.43 ticks from the impact: present on the 71st, gone on the 72nd.
    steps(&mut world, EXPIRY - 1);
    assert!(fungaled(&world, victim) && has(&world, victim, "fungal_growth_slow"));
    step(&mut world);
    assert!(!fungaled(&world, victim) && !has(&world, victim, "fungal_growth_slow"));
    assert!(world.execute_ability(victim, blink, Some(at(20.0, 20.0)), None));
    let blinked = world.entities[&victim].position;
    assert!(
        world.try_load_unit_into_transport(victim, shuttle).is_ok(),
        "{blinked:?}"
    );
}

#[test]
fn a_cloaked_enemy_is_revealed_for_the_status_and_hidden_again_after() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    let hidden = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    world.entities.get_mut(&hidden).expect("alive").is_cloaked = true;
    // A blue marine 5 away: in range, without detection.
    held(&mut world, "pulse_bot", BLUE, 16.0, 25.0);
    steps(&mut world, 2);
    let start = vitals(&world, hidden);
    assert!(world.entities[&hidden].is_concealed());
    steps(&mut world, 10);
    assert_eq!(vitals(&world, hidden), start, "unseen, unshot");
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    step_to_impact(&mut world, hidden);
    assert!(!world.entities[&hidden].is_concealed(), "revealed");
    steps(&mut world, EXPIRY - 1);
    assert!(
        start - vitals(&world, hidden) > n(25),
        "the marine shoots the revealed unit"
    );
    step(&mut world);
    assert!(world.entities[&hidden].is_concealed(), "hidden again");
}

#[test]
fn a_burrowed_enemy_is_revealed() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    let burrower = spawn(&mut world, "burrow_scorpion", RED, 16.0, 20.0);
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
    steps(&mut world, 40);
    assert!(world.entities[&burrower].is_concealed(), "burrowed");
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    step_to_impact(&mut world, burrower);
    let entity = &world.entities[&burrower];
    assert!(
        entity.active_mode.is_some() && !entity.is_concealed(),
        "still burrowed, revealed (SU-7)"
    );
}

#[test]
fn a_recast_refreshes_the_statuses_with_their_phase_and_repeats_no_damage() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    let victim = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    let start = vitals(&world, victim);
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    step_to_impact(&mut world, victim);
    // Ticks counted from the first impact; the second cast goes out after tick 30.
    let mut last = vitals(&world, victim);
    let mut seen = Vec::new();
    let mut second = None;
    for tick in 1..=200 {
        if tick == 31 {
            assert!(fungal(&mut world, caster, 16.0, 20.0));
        }
        step(&mut world);
        let now = vitals(&world, victim);
        if now < last {
            seen.push((tick, last - now));
        }
        last = now;
        if tick > 31 && second.is_none() && world.effects().missiles().is_empty() {
            second = Some(tick);
        }
    }
    let second = second.expect("the second missile lands");
    // From the refresh the statuses run 71.43 ticks again, their pulses still on the first
    // impact's phase: pulse k at round_half_up(k × 4.464286) while that is within the new end.
    let due: Vec<u32> = (1..=40u32)
        .map(pulse_offset)
        .filter(|&tick| f64::from(tick) <= f64::from(second) + 71.428_571)
        .collect();
    assert_eq!(
        seen,
        pulses_at(&due),
        "one pulse at a time, on the first phase"
    );
    assert!(due.len() > PULSES.len(), "the refresh extends the damage");
    assert_eq!(
        start - vitals(&world, victim),
        pulse() * SimScalar::from_num(due.len()),
        "no damage at the second impact"
    );
}

/// `round_half_up(k × 4.464286)`, the tick of pulse `k` after the application (plan D-4).
fn pulse_offset(k: u32) -> u32 {
    let exact = SimScalar::from_num(4.464_286) * SimScalar::from_num(k);
    u32::try_from((exact + SimScalar::HALF).raw() >> 32).expect("a small tick")
}

#[test]
fn an_armed_idle_enemy_answers_the_caster() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    let victim = spawn(&mut world, "pulse_bot", RED, 16.0, 20.0);
    // One tick so red sees the caster (an answer needs a visible attacker).
    step(&mut world);
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    step_to_impact(&mut world, victim);
    let entity = &world.entities[&victim];
    assert!(
        entity.current_target == Some(caster)
            || matches!(entity.order_queue.front(), Some(UnitOrder::AttackTarget { target, .. }) if *target == caster),
        "FungalGrowthSearchDummy's Acquire: {:?} {:?}",
        entity.current_target,
        entity.order_queue.front()
    );
}

#[test]
fn the_missile_flies_on_after_the_caster_dies() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    let victim = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    world.entities.get_mut(&caster).expect("alive").hp = SimScalar::ZERO;
    step(&mut world);
    assert!(!world.entities.contains_key(&caster), "the caster is gone");
    step_to_impact(&mut world, victim);
}

#[test]
fn costs_75_energy_with_no_cooldown_and_no_lock() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    world.entities.get_mut(&caster).expect("spawned").energy = SimScalar::from_num(74.9);
    assert!(!fungal(&mut world, caster, 16.0, 20.0), "needs 75 energy");
    world.entities.get_mut(&caster).expect("spawned").energy = n(150);
    assert!(fungal(&mut world, caster, 16.0, 20.0));
    let entity = &world.entities[&caster];
    assert_eq!(entity.energy, n(75), "75 paid");
    assert_eq!(entity.cast_lock_ticks, 0, "no CastOutroTime in the chain");
    let index = usize::from(fungal_index(&world, caster));
    assert_eq!(entity.ability_cooldowns[index], 0);
    assert!(fungal(&mut world, caster, 16.0, 24.0), "again at once");
    assert_eq!(world.effects().missiles().len(), 2);
}

#[test]
fn cast_range_is_ten_from_the_casters_edge() {
    let mut world = world();
    let caster = infestor(&mut world, 7.5, 20.0);
    let index = fungal_index(&world, caster);
    // 10 (void AbilData:674) + the caster's radius 0.625.
    let reach = SimScalar::from_num(10.625);
    let inside = SimVec2::new(SimScalar::from_num(7.5) + reach, n(20));
    let outside = SimVec2::new(inside.x + SimScalar::from_num(0.01), n(20));
    assert!(!world.would_admit_cast(caster, index, Some(outside), None));
    assert!(world.would_admit_cast(caster, index, Some(inside), None));
    assert!(world.execute_ability(caster, index, Some(inside), None));
}

#[test]
fn a_cast_order_out_of_range_walks_into_range_then_casts() {
    let mut world = world();
    let caster = infestor(&mut world, 4.0, 20.0);
    let index = fungal_index(&world, caster);
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands: vec![PlayerCommand {
            player_id: BLUE,
            entities: vec![caster],
            order: UnitOrder::CastAbility {
                ability_index: index,
                target_pos: Some(at(20.0, 20.0)),
                target_entity: None,
            },
            queue: false,
        }],
        state_checksum: None,
    });
    assert!(
        world.effects().missiles().is_empty(),
        "out of range: approach"
    );
    let cast = (0..60).any(|_| {
        step(&mut world);
        !world.effects().missiles().is_empty()
    });
    assert!(cast, "cast once in range");
    let distance = world.entities[&caster].position.distance(at(20.0, 20.0));
    assert!(distance <= SimScalar::from_num(10.625));
    assert!(
        distance > SimScalar::from_num(10.0),
        "cast as soon as in range"
    );
}
