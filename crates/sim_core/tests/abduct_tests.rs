//! # Integration Test: Abduct (plan `2026-11-10-spellcaster-mechanics` WP3)
//!
//! The shipped Viper analogue's Abduct, cast through `SimWorld::execute_ability` and played
//! through `SimWorld::step`, against the `StarCraft` II 5.0.14 rules of plan §3.2, §6.2 and D-8:
//! a tongue homing on the target at 1.05 world units a tick; at contact the target's orders are
//! cancelled, a burrowed target is unburrowed, and the `abducted` status stuns, reveals and
//! lifts collision and boarding; two ticks later the target flies 18 ticks in a straight line to
//! the landing point fixed at the cast, 0.1 ahead of the caster and within 2 of it, where the
//! status is removed. The caster pays 75 energy on the cast and is locked for 14 ticks.
//!
//! The tests read the shipped catalog (`units.ron`, `ability_effects.ron`, `statuses.ron`), so a
//! data edit that breaks a rule fails here. Units are named by slug in this test only (charter
//! §3.3 governs production code). The Viper analogue has no weapon, so it never hits a victim.

#![allow(clippy::expect_used, clippy::too_many_lines)]

#[path = "support/effect_world.rs"]
mod effect_support;

use sim_core::SimWorld;
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySource, EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder,
};

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The ability's name in the shipped catalog.
const ABDUCT: &str = "Abduct";

/// Ticks from contact to the start of the flight (`YoinkDelayPersistent`, one pulse of 1.79).
const DELAY: u32 = 2;
/// Ticks of the flight (`YoinkMover` `FlightTime` 1 Normal s = 17.86).
const FLIGHT: u32 = 18;

fn flat_map() -> GameMap {
    GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0))).expect("invariant: a flat map")
}

fn world() -> SimWorld {
    sim_fixtures::world(flat_map(), 0xAB_D0C7)
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

/// Spawns `slug` for `owner` at `(x, y)` holding position.
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

/// Spawns a Viper analogue for `owner` at `(x, y)` with full energy.
fn viper(world: &mut SimWorld, owner: PlayerId, x: f32, y: f32) -> EntityId {
    let id = spawn(world, "tongue_chameleon", owner, x, y);
    world.entities.get_mut(&id).expect("spawned").energy = n(200);
    id
}

/// The index of Abduct in `caster`'s archetype.
fn abduct_index(world: &SimWorld, caster: EntityId) -> u8 {
    let kind = world.entities[&caster].unit_type_id;
    let arch = world.catalog.get(kind).expect("registered");
    let index = arch
        .abilities
        .iter()
        .position(|ability| ability.name == ABDUCT)
        .expect("the Viper analogue has Abduct");
    u8::try_from(index).expect("few abilities")
}

/// Casts Abduct by `caster` on `target` now; whether it was admitted.
fn abduct(world: &mut SimWorld, caster: EntityId, target: EntityId) -> bool {
    let index = abduct_index(world, caster);
    world.execute_ability(caster, index, None, Some(target))
}

/// The order that casts Abduct by `caster` on `target`.
fn abduct_order(world: &SimWorld, caster: EntityId, target: EntityId) -> UnitOrder {
    UnitOrder::CastAbility {
        ability_index: abduct_index(world, caster),
        target_pos: None,
        target_entity: Some(target),
    }
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

fn steps(world: &mut SimWorld, count: u32) {
    for _ in 0..count {
        step(world);
    }
}

/// Whether `id` carries the `abducted` status.
fn abducted(world: &SimWorld, id: EntityId) -> bool {
    let status = world
        .catalog
        .spells()
        .status_id("abducted")
        .expect("the shipped abducted status");
    world.effects().has_status(id, status)
}

/// Steps until `id` is abducted (at most 60 ticks); returns the tick that contact ran on.
fn step_to_contact(world: &mut SimWorld, id: EntityId) -> u32 {
    (0..60)
        .find_map(|_| {
            let tick = world.current_tick;
            step(world);
            abducted(world, id).then_some(tick)
        })
        .expect("the tongue reaches the target within 60 ticks")
}

/// The collision radius of `id`'s archetype.
fn radius(world: &SimWorld, id: EntityId) -> SimScalar {
    let kind = world.entities[&id].unit_type_id;
    world
        .catalog
        .get(kind)
        .expect("registered")
        .collision_radius
}

/// The index of `id`'s ability that switches it into a mode for which `pick` holds.
fn mode_index(world: &SimWorld, id: EntityId, burrowed: bool) -> u8 {
    let kind = world.entities[&id].unit_type_id;
    let arch = world.catalog.get(kind).expect("registered");
    let index = arch
        .abilities
        .iter()
        .position(|ability| match &ability.effect {
            AbilityEffectSpec::SwitchMode { mode_name } => arch
                .get_mode(mode_name)
                .is_some_and(|mode| mode.burrowed == burrowed),
            _ => false,
        })
        .expect("a mode ability");
    u8::try_from(index).expect("few abilities")
}

/// Starts `id`'s mode switch and steps until it has settled.
fn settle_mode(world: &mut SimWorld, id: EntityId, burrowed: bool) {
    let index = mode_index(world, id, burrowed);
    assert!(world.execute_ability(id, index, None, None), "mode switch");
    let settled = (0..120).any(|_| {
        step(world);
        let entity = &world.entities[&id];
        entity.active_mode.is_some() && entity.channeling_remaining_ticks == 0
    });
    assert!(settled, "the mode settles within 120 ticks");
}

#[test]
fn the_tongue_lands_ceil_gap_over_speed_later_and_the_pull_ends_twenty_ticks_after() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let target = held(&mut world, "shock_golem", RED, 18.0, 20.0);
    steps(&mut world, 2);
    let cast_position = world.entities[&caster].position;
    let start = world.entities[&target].position;
    let cast_tick = world.current_tick;
    assert!(abduct(&mut world, caster, target));
    assert_eq!(
        world.entities[&caster].energy,
        n(125),
        "75 paid on the cast"
    );
    // The tongue leaves the caster's centre and stops at the target's edge, 1.05 a tick.
    let gap = n(8) - radius(&world, target);
    let speed = SimScalar::from_num(1.05);
    let flight = gap
        .saturating_div(speed)
        .saturating_ceil()
        .saturating_to_u32();
    // A cast between ticks is first stepped on the tick numbered `cast_tick` (spellcaster
    // D-16 item 3), and the tongue impacts on the step that reaches contact (plan
    // `2026-11-11-contact-and-missiles` D-6, B-115): its `flight`-th step, which runs at
    // `cast_tick + flight - 1`. It used to impact one step later.
    let contact = step_to_contact(&mut world, target);
    assert_eq!(
        contact - cast_tick + 1,
        flight,
        "contact on the tongue's ceil(gap / 1.05)-th step"
    );
    // Two ticks of delay: the target is held in place, stunned.
    for _ in 0..DELAY {
        assert_eq!(world.entities[&target].position, start);
        assert!(world.entities[&target].is_action_locked());
        step(&mut world);
    }
    // Eighteen ticks of flight in a straight line to the point fixed at the cast.
    let mut previous = world.entities[&target].position;
    for flown in 1..FLIGHT {
        step(&mut world);
        let now = world.entities[&target].position;
        assert!(
            now.x < previous.x,
            "flying toward the caster on tick {flown}"
        );
        assert_eq!(now.y, start.y, "a straight line");
        assert!(world.entities[&target].displacement.is_some());
        previous = now;
    }
    step(&mut world);
    let landed = &world.entities[&target];
    assert!(landed.displacement.is_none(), "landed on contact + 20");
    assert!(
        !abducted(&world, target),
        "the status is removed at landing"
    );
    assert!(
        landed.position.distance(cast_position) <= SimScalar::from_num(0.11),
        "lands 0.1 ahead of the caster's cast position: {:?}",
        landed.position
    );
    assert_eq!(world.current_tick - 1, contact + DELAY + FLIGHT);
}

#[test]
fn the_landing_point_is_fixed_at_the_cast_even_if_the_caster_moves_or_dies() {
    let mut world = world();
    let mover = viper(&mut world, BLUE, 10.0, 20.0);
    let dier = viper(&mut world, BLUE, 10.0, 40.0);
    let first = held(&mut world, "shock_golem", RED, 17.0, 20.0);
    let second = held(&mut world, "shock_golem", RED, 17.0, 40.0);
    let origins = [mover, dier].map(|id| world.entities[&id].position);
    steps(&mut world, 2);
    assert!(abduct(&mut world, mover, first));
    assert!(abduct(&mut world, dier, second));
    step_to_contact(&mut world, first);
    world.entities.get_mut(&dier).expect("alive").hp = SimScalar::ZERO;
    // The mover flies off as soon as its 14-tick lock ends.
    step_with(
        &mut world,
        vec![PlayerCommand {
            player_id: BLUE,
            entities: vec![mover],
            order: UnitOrder::Move {
                target: at(10.0, 4.0),
            },
            queue: false,
        }],
    );
    steps(&mut world, 30);
    assert!(!world.entities.contains_key(&dier), "the caster died");
    assert!(
        world.entities[&mover].position.distance(origins[0]) > n(1),
        "the caster moved"
    );
    for (target, origin) in [first, second].into_iter().zip(origins) {
        let entity = &world.entities[&target];
        assert!(entity.displacement.is_none() && !abducted(&world, target));
        assert!(
            entity.position.distance(origin) <= SimScalar::from_num(0.11),
            "landed at the cast-time point: {:?} vs {origin:?}",
            entity.position
        );
    }
}

#[test]
fn a_ground_unit_pulled_across_a_cliff_lands_on_the_casters_level() {
    // High ground (level 1) for x < 16, low ground beyond.
    let width = 64;
    let mut cells = vec![MapCell::default_ground(CliffLevel(0)); width * width];
    for y in 0..width {
        for x in 0..16 {
            cells[y * width + x] = MapCell::default_ground(CliffLevel(1));
        }
    }
    let map =
        GameMap::from_cells(64, 64, cells, Vec::new(), Vec::new()).expect("invariant: a valid map");
    let mut world = sim_fixtures::world(map, 7);
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let target = held(&mut world, "shock_golem", RED, 19.0, 20.0);
    steps(&mut world, 2);
    assert!(abduct(&mut world, caster, target));
    steps(&mut world, 40);
    let landed = world.entities[&target].position;
    let (cx, cy) = world.map.world_to_cell(landed).expect("on the map");
    assert_eq!(
        world.map.cell(cx, cy).expect("a cell").cliff_level,
        CliffLevel(1),
        "on the high ground at {landed:?}"
    );
}

#[test]
fn an_air_unit_is_pulled_and_stays_air_and_a_massive_unit_is_pulled() {
    let mut world = world();
    let first = viper(&mut world, BLUE, 10.0, 20.0);
    let second = viper(&mut world, BLUE, 10.0, 40.0);
    // An air unit without a weapon against air, and the Ultralisk analogue (Massive).
    let flier = spawn(&mut world, "amber_pollen_dome", RED, 17.0, 20.0);
    let massive = held(&mut world, "stag_behemoth", RED, 17.0, 40.0);
    let massive_kind = world.entities[&massive].unit_type_id;
    assert!(
        world
            .catalog
            .get(massive_kind)
            .expect("registered")
            .attributes
            .contains(sim_types::AttributeFlags::MASSIVE),
        "the Ultralisk analogue is massive"
    );
    steps(&mut world, 2);
    assert!(abduct(&mut world, first, flier), "air targets are allowed");
    assert!(abduct(&mut world, second, massive), "massive is allowed");
    steps(&mut world, 40);
    for (target, caster_at) in [(flier, at(10.0, 20.0)), (massive, at(10.0, 40.0))] {
        let entity = &world.entities[&target];
        assert!(
            entity.position.distance(caster_at) <= n(2),
            "{target:?} pulled to {:?}",
            entity.position
        );
    }
    let flier_kind = world.entities[&flier].unit_type_id;
    assert_eq!(
        world
            .catalog
            .get(flier_kind)
            .expect("registered")
            .mover_type,
        sim_types::MoverType::Air
    );
}

#[test]
fn own_units_can_be_pulled_but_not_the_caster_nor_a_structure() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let own = held(&mut world, "razor_mantis", BLUE, 16.0, 20.0);
    let dome = world
        .catalog
        .id_by_slug("capacitor_dome")
        .expect("a shipped structure");
    let structure = world
        .spawn_structure(dome, RED, 16, 26)
        .expect("a free site");
    steps(&mut world, 2);
    let index = abduct_index(&world, caster);
    assert!(
        !world.would_accept_cast_order(caster, index, None, Some(caster)),
        "not itself"
    );
    assert!(
        !world.would_accept_cast_order(caster, index, None, Some(structure)),
        "not a structure"
    );
    assert!(abduct(&mut world, caster, own), "own units are allowed");
    steps(&mut world, 40);
    assert!(world.entities[&own].position.distance(at(10.0, 20.0)) <= n(2));
}

#[test]
fn contact_cancels_orders_hold_and_the_swing_and_stuns_without_attack_or_boarding() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let target = spawn(&mut world, "pulse_bot", RED, 17.0, 20.0);
    // A blue victim the marine shoots, and a red shuttle it could board.
    let victim = held(&mut world, "shock_golem", BLUE, 17.0, 23.0);
    let shuttle = held(&mut world, "repair_shuttle", RED, 19.0, 20.0);
    {
        let entity = world.entities.get_mut(&target).expect("alive");
        entity
            .issue_order(UnitOrder::HoldPosition, false)
            .expect("hold");
        entity
            .issue_order(
                UnitOrder::Move {
                    target: at(40.0, 20.0),
                },
                true,
            )
            .expect("a queued move");
    }
    steps(&mut world, 20);
    assert!(
        world.entities[&victim].hp
            < world
                .catalog
                .get(world.entities[&victim].unit_type_id)
                .expect("kind")
                .max_hp,
        "the marine shoots before the pull"
    );
    assert!(abduct(&mut world, caster, target));
    step_to_contact(&mut world, target);
    let entity = &world.entities[&target];
    assert!(entity.order_queue.is_empty(), "orders cancelled");
    assert!(entity.current_target.is_none(), "target cleared");
    assert!(entity.is_action_locked(), "stunned");
    assert!(entity.status_modifiers().suppress_collision);
    assert!(
        world.try_load_unit_into_transport(target, shuttle).is_err(),
        "no boarding while abducted"
    );
    let victim_hp = world.entities[&victim].hp;
    steps(&mut world, DELAY + FLIGHT - 1);
    assert_eq!(
        world.entities[&victim].hp, victim_hp,
        "no attack during the pull"
    );
    assert!(
        world.entities[&target].displacement.is_some(),
        "still flying"
    );
}

#[test]
fn orders_given_during_the_pull_wait_for_the_landing() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let target = held(&mut world, "shock_golem", RED, 17.0, 20.0);
    steps(&mut world, 2);
    assert!(abduct(&mut world, caster, target));
    let contact = step_to_contact(&mut world, target);
    step_with(
        &mut world,
        vec![PlayerCommand {
            player_id: RED,
            entities: vec![target],
            order: UnitOrder::Move {
                target: at(10.0, 50.0),
            },
            queue: false,
        }],
    );
    while world.current_tick <= contact + DELAY + FLIGHT {
        assert!(
            world.entities[&target].position.y <= n(20),
            "no walk of its own"
        );
        step(&mut world);
    }
    steps(&mut world, 20);
    assert!(
        world.entities[&target].position.y > SimScalar::from_num(20.5),
        "the move starts after landing"
    );
}

#[test]
fn a_sieged_tank_lands_sieged_and_a_siege_in_progress_completes() {
    let mut world = world();
    let first = viper(&mut world, BLUE, 10.0, 20.0);
    let second = viper(&mut world, BLUE, 10.0, 40.0);
    let sieged = held(&mut world, "railgun_crawler", RED, 18.0, 20.0);
    let sieging = held(&mut world, "railgun_crawler", RED, 18.0, 40.0);
    settle_mode(&mut world, sieged, false);
    let mode = world.entities[&sieged].active_mode.clone();
    assert!(mode.is_some());
    assert!(
        abduct(&mut world, first, sieged),
        "a sieged tank is abductable at 5.0.14"
    );
    let index = mode_index(&world, sieging, false);
    assert!(
        world.execute_ability(sieging, index, None, None),
        "siege starts"
    );
    assert!(abduct(&mut world, second, sieging));
    steps(&mut world, 120);
    for (tank, caster_at) in [(sieged, at(10.0, 20.0)), (sieging, at(10.0, 40.0))] {
        let entity = &world.entities[&tank];
        assert!(
            entity.position.distance(caster_at) <= n(2),
            "{tank:?} pulled to {:?}",
            entity.position
        );
        assert_eq!(
            entity.active_mode, mode,
            "{tank:?} sieged after landing (sieged {sieged:?}, sieging {sieging:?}): pending {:?} channel {}",
            entity.pending_ability, entity.channeling_remaining_ticks
        );
        assert_eq!(entity.channeling_remaining_ticks, 0);
    }
}

#[test]
fn a_detected_burrowed_unit_is_unburrowed_at_contact_and_revealed_while_abducted() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let burrower = spawn(&mut world, "burrow_scorpion", RED, 17.0, 20.0);
    settle_mode(&mut world, burrower, true);
    assert!(world.entities[&burrower].is_concealed(), "burrowed");
    let index = abduct_index(&world, caster);
    assert!(
        !world.would_accept_cast_order(caster, index, None, Some(burrower)),
        "undetected: not a valid target"
    );
    spawn(&mut world, "omni_probe_drone", BLUE, 14.0, 20.0);
    steps(&mut world, 2);
    assert!(world.would_accept_cast_order(caster, index, None, Some(burrower)));
    assert!(abduct(&mut world, caster, burrower));
    step_to_contact(&mut world, burrower);
    let entity = &world.entities[&burrower];
    assert!(entity.active_mode.is_none(), "unburrowed at once");
    assert!(!entity.is_concealed(), "revealed");
    steps(&mut world, 30);
    assert!(world.entities[&burrower].position.distance(at(10.0, 20.0)) <= n(2));
}

#[test]
fn an_undetected_cloaked_enemy_cannot_be_targeted() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let hidden = held(&mut world, "shock_golem", RED, 16.0, 20.0);
    world.entities.get_mut(&hidden).expect("alive").is_cloaked = true;
    steps(&mut world, 2);
    assert!(!abduct(&mut world, caster, hidden));
    assert_eq!(world.entities[&caster].energy, n(200), "no payment");
    spawn(&mut world, "omni_probe_drone", BLUE, 13.0, 21.0);
    steps(&mut world, 2);
    assert!(abduct(&mut world, caster, hidden), "detected: allowed");
}

#[test]
fn a_tongue_at_a_target_that_boards_a_transport_is_dropped() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let target = held(&mut world, "pulse_bot", RED, 18.0, 20.0);
    let shuttle = held(&mut world, "repair_shuttle", RED, 19.0, 20.0);
    steps(&mut world, 2);
    assert!(abduct(&mut world, caster, target));
    step(&mut world);
    assert_eq!(world.effects().missiles().len(), 1, "the tongue flies");
    world
        .try_load_unit_into_transport(target, shuttle)
        .expect("boards before contact");
    steps(&mut world, 30);
    assert!(
        world.effects().missiles().is_empty(),
        "the tongue is dropped"
    );
    assert!(!abducted(&world, target));
    // 125 after the cast plus 31 ticks of regeneration (0.0315 a tick): no refund.
    assert!(
        world.entities[&caster].energy < n(127),
        "energy stays spent"
    );
}

#[test]
fn refused_without_a_landing_point_and_without_payment() {
    // Unpathable ground 12 cells wide around (30, 30): no ground point within 2 of a caster
    // hovering over its middle.
    let mut map = flat_map();
    for y in 24..36 {
        for x in 24..36 {
            map.cell_mut(x, y)
                .expect("inside")
                .flags
                .remove(CellFlags::GROUND_PATHABLE);
        }
    }
    let mut world = sim_fixtures::world(map, 3);
    let caster = viper(&mut world, BLUE, 30.0, 30.0);
    let target = held(&mut world, "shock_golem", RED, 30.0, 21.0);
    steps(&mut world, 2);
    assert!(!abduct(&mut world, caster, target));
    assert_eq!(world.entities[&caster].energy, n(200), "no payment");
    assert_eq!(world.effects().missiles(), [], "no tongue");
}

#[test]
fn the_landing_point_is_found_within_two_of_the_start() {
    // Unpathable cells 9 to 11 around the start point: the target's footprint first fits 2 away.
    let mut map = flat_map();
    for y in 19..22 {
        for x in 9..12 {
            map.cell_mut(x, y)
                .expect("inside")
                .flags
                .remove(CellFlags::GROUND_PATHABLE);
        }
    }
    let mut world = sim_fixtures::world(map, 5);
    let caster = viper(&mut world, BLUE, 10.5, 20.5);
    let target = held(&mut world, "razor_mantis", RED, 17.0, 20.5);
    steps(&mut world, 2);
    let start = world.entities[&caster].position;
    assert!(abduct(&mut world, caster, target));
    steps(&mut world, 40);
    let landed = world.entities[&target].position;
    let distance = landed.distance(start);
    assert!(
        distance > SimScalar::from_num(1.5) && distance <= SimScalar::from_num(2.11),
        "on the ring within 2: {landed:?} ({distance})"
    );
    let (cx, cy) = world.map.world_to_cell(landed).expect("on the map");
    assert!(world
        .map
        .cell(cx, cy)
        .expect("a cell")
        .flags
        .contains(CellFlags::GROUND_PATHABLE));
}

#[test]
fn units_do_not_block_the_landing_point() {
    // `PlacementIgnoreBlockers` (swarm EffectData:1453): a ground unit under the hovering caster
    // does not move the landing point.
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    held(&mut world, "shock_golem", BLUE, 10.1, 20.0);
    let target = held(&mut world, "razor_mantis", RED, 17.0, 20.0);
    let start = world.entities[&caster].position;
    steps(&mut world, 2);
    assert!(abduct(&mut world, caster, target));
    let origin = world.effects().missiles()[0].context.origin;
    assert!(
        origin.distance(start) <= SimScalar::from_num(0.11),
        "the start point, though a unit stands on it: {origin:?}"
    );
}

#[test]
fn the_caster_is_locked_for_fourteen_ticks_after_the_cast() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let target = held(&mut world, "shock_golem", RED, 18.0, 20.0);
    steps(&mut world, 2);
    let order = abduct_order(&world, caster, target);
    step_with(
        &mut world,
        vec![PlayerCommand {
            player_id: BLUE,
            entities: vec![caster],
            order,
            queue: false,
        }],
    );
    assert_eq!(world.effects().missiles().len(), 1, "cast on the tick");
    let start = world.entities[&caster].position;
    step_with(
        &mut world,
        vec![PlayerCommand {
            player_id: BLUE,
            entities: vec![caster],
            order: UnitOrder::Move {
                target: at(10.0, 50.0),
            },
            queue: false,
        }],
    );
    // A lock of 14 set on tick t holds t … t + 13 (plan Amendment 2).
    steps(&mut world, 12);
    assert_eq!(world.entities[&caster].position, start, "locked");
    steps(&mut world, 2);
    assert_ne!(
        world.entities[&caster].position, start,
        "free after the lock"
    );
}

#[test]
fn cast_range_is_nine_edge_to_edge() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let target = held(&mut world, "shock_golem", RED, 30.0, 20.0);
    steps(&mut world, 2);
    let index = abduct_index(&world, caster);
    // 9 + the caster's radius 1 + the target's radius.
    let reach = n(9) + radius(&world, caster) + radius(&world, target);
    let place = |world: &mut SimWorld, x: SimScalar| {
        world.entities.get_mut(&target).expect("alive").position = SimVec2::new(x, n(20));
    };
    place(&mut world, n(10) + reach + SimScalar::from_num(0.01));
    assert!(!world.would_admit_cast(caster, index, None, Some(target)));
    place(&mut world, n(10) + reach);
    assert!(world.would_admit_cast(caster, index, None, Some(target)));
    assert!(abduct(&mut world, caster, target));
}

#[test]
fn a_second_abduct_on_an_abducted_unit_is_refused() {
    let mut world = world();
    let first = viper(&mut world, BLUE, 10.0, 20.0);
    let second = viper(&mut world, BLUE, 10.0, 24.0);
    let target = held(&mut world, "shock_golem", RED, 17.0, 22.0);
    steps(&mut world, 2);
    assert!(abduct(&mut world, first, target));
    step_to_contact(&mut world, target);
    assert!(!abduct(&mut world, second, target), "NoYoink");
    assert_eq!(world.entities[&second].energy, n(200));
}

#[test]
fn the_tether_is_a_link_from_launch_to_landing_without_a_gap() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let target = held(&mut world, "shock_golem", RED, 18.0, 20.0);
    steps(&mut world, 2);
    let index = abduct_index(&world, caster);
    let kind = world.entities[&caster].unit_type_id;
    assert!(abduct(&mut world, caster, target));
    let mut shown = 0;
    loop {
        let snapshot = world.build_render_snapshot();
        let Some(link) = snapshot.links.first() else {
            break;
        };
        assert_eq!(snapshot.links.len(), 1);
        assert_eq!((link.from, link.to, link.owner), (caster, target, BLUE));
        assert_eq!(
            link.source,
            AbilitySource {
                unit_type: kind,
                ability: index,
            }
        );
        shown += 1;
        step(&mut world);
        assert!(shown < 60, "the tether ends");
    }
    assert!(
        world.entities[&target].displacement.is_none() && !abducted(&world, target),
        "the tether ends with the landing"
    );
    let flight = (n(8) - radius(&world, target)).saturating_div(SimScalar::from_num(1.05));
    // The tongue when cast and after each of its steps before the one that reaches contact
    // (ceil(gap / 1.05) snapshots: it impacts on its last step, plan
    // `2026-11-11-contact-and-missiles` D-6), the two ticks of delay, then the eighteen of
    // flight; the landing tick shows none.
    let expected = flight.saturating_ceil().saturating_to_u32() + DELAY + FLIGHT;
    assert_eq!(shown, expected, "every tick from the cast to the landing");
}

/// Whether `id` carries the shipped status `name`.
fn has_status(world: &SimWorld, id: EntityId, name: &str) -> bool {
    let status = world
        .catalog
        .spells()
        .status_id(name)
        .expect("a shipped status");
    world.effects().has_status(id, status)
}

/// Steps once and checks the rule bug B-114 broke: a unit in flight carries `abducted`.
fn step_checking_flight(world: &mut SimWorld, target: EntityId, label: &str) {
    step(world);
    let flying = world.entities[&target].displacement.is_some();
    assert!(
        !flying || abducted(world, target),
        "{label}, tick {}: the unit flies without the abducted status",
        world.current_tick
    );
}

/// Ticks the `YoinkMarker` buff lasts, rounded up: 2 Normal s = 35.71 (`swarm`
/// BehaviorData:1092).
const MARK: u32 = 36;

/// Bug B-114 through the cast: a second Viper ordered on the tick of the first cast, from the
/// far side so that its tongue would chase the unit while it flies away, is refused (the
/// game's `NoYoink`: the first cast marked the target, `swarm` EffectData:1468, :1482-1485), and
/// stays refused on every tick of the pull; once the 2 s mark has expired the second cast is
/// admitted and its pull carries the status throughout.
#[test]
fn a_second_abduct_is_refused_while_the_first_casts_mark_lasts() {
    let mut world = world();
    let near = viper(&mut world, BLUE, 14.0, 20.0);
    let far = viper(&mut world, BLUE, 22.0, 20.0);
    let target = held(&mut world, "shock_golem", RED, 18.0, 20.0);
    steps(&mut world, 2);
    let index = abduct_index(&world, far);
    let cast_tick = world.current_tick;
    assert!(abduct(&mut world, near, target));
    assert!(
        has_status(&world, target, "abduct_mark"),
        "the cast marks its target"
    );
    assert!(
        !abduct(&mut world, far, target),
        "NoYoink on the cast's tick"
    );
    assert_eq!(world.entities[&far].energy, n(200), "nothing paid");
    while world.current_tick < cast_tick + MARK - 1 {
        assert!(
            !world.would_accept_cast_order(far, index, None, Some(target)),
            "tick {}: refused while the mark lasts",
            world.current_tick
        );
        step_checking_flight(&mut world, target, "first pull");
    }
    assert!(
        world.entities[&target].displacement.is_none() && !abducted(&world, target),
        "the first pull landed within the mark"
    );
    while has_status(&world, target, "abduct_mark") {
        step_checking_flight(&mut world, target, "after the landing");
        assert!(
            world.current_tick <= cast_tick + MARK + 1,
            "the mark expires"
        );
    }
    assert!(
        abduct(&mut world, far, target),
        "admitted once the mark ended"
    );
    let contact = step_to_contact(&mut world, target);
    assert!(contact >= cast_tick + MARK - 1);
    for _ in 0..=DELAY + FLIGHT {
        step_checking_flight(&mut world, target, "second pull");
    }
    assert!(
        world.entities[&target].displacement.is_none() && !abducted(&world, target),
        "the second pull landed"
    );
}

/// Bug B-114 in the effect engine, whatever the cast pipeline admits: a second walk of the
/// Abduct tree started on any tick of a first pull is refused by the tree's own validator
/// (`NoYoink` on `YoinkStartCreatePlaceholder`, `swarm` EffectData:1468). The first pull lands
/// where the unit stands, as in the allocation child that found the bug, so the second tongue
/// reaches the unit on a tick the offset chooses: during the first tongue's flight, the 2-tick
/// delay, the 18-tick flight, or on the landing tick itself. There the first landing used to
/// remove the `abducted` status the second contact had just refreshed, and the unit then flew
/// the second pull without it (no stun, collision rule or boarding ban). The unit never flies
/// without the status, and is never pulled to the second walk's origin.
#[test]
fn a_second_tongue_never_meets_a_unit_whose_pull_is_under_way() {
    let mut pulled_twice = Vec::new();
    for offset in 0..MARK {
        let label = format!("second walk {offset} ticks after the first");
        let mut world = world();
        let target = held(&mut world, "shock_golem", RED, 18.0, 20.0);
        steps(&mut world, 2);
        let root = world
            .catalog
            .spells()
            .effect_id("abduct")
            .expect("the shipped Abduct tree");
        let walk = |source: SimVec2, origin: SimVec2| sim_types::EffectContext {
            owner: BLUE,
            caster: None,
            ability: AbilitySource::default(),
            origin,
            source_point: source,
            target: sim_types::EffectTarget::Unit(target),
        };
        world.run_effect(root, walk(at(14.0, 20.0), at(18.0, 20.0)));
        for _ in 0..offset {
            step_checking_flight(&mut world, target, &label);
        }
        world.run_effect(root, walk(at(22.0, 20.0), at(23.0, 20.0)));
        for _ in 0..2 * MARK {
            step_checking_flight(&mut world, target, &label);
        }
        if world.entities[&target].position.x >= n(20) {
            pulled_twice.push(offset);
        }
    }
    assert!(
        pulled_twice.is_empty(),
        "pulled to the second walk's origin after offsets {pulled_twice:?}"
    );
}

/// Ticks within which a homing missile at `speed` closes `gap` on a target receding at
/// `receding` a tick, plus one: `ceil(gap / (speed − receding)) + 1`.
fn closing_bound(gap: SimScalar, speed: SimScalar, receding: SimScalar) -> u32 {
    gap.saturating_div(speed - receding)
        .saturating_ceil()
        .saturating_to_u32()
        + 1
}

/// Orders `id` of `owner` to walk to `(60, 20)` and steps ten ticks so that it walks at speed;
/// returns its `max_speed`.
fn walk_east(world: &mut SimWorld, owner: PlayerId, id: EntityId) -> SimScalar {
    step_with(
        world,
        vec![PlayerCommand {
            player_id: owner,
            entities: vec![id],
            order: UnitOrder::Move {
                target: at(60.0, 20.0),
            },
            queue: false,
        }],
    );
    steps(world, 10);
    let kind = world.entities[&id].unit_type_id;
    world.catalog.get(kind).expect("registered").max_speed
}

/// T-M2 (plan `2026-11-11-contact-and-missiles` WP1), the reproduction of B-115: a tongue
/// reaches a unit walking straight away from the caster at its full speed. The tongue flies
/// 1.05 a tick and the unit at most its `max_speed`, so the gap closes by their difference
/// every tick (`YoinkMissile` is a Guidance mover, which "chases the target, regardless of
/// where it moves"). It used to trail the unit at contact distance until the unit stopped.
#[test]
fn a_tongue_reaches_a_unit_walking_away_from_it() {
    let mut world = world();
    let caster = viper(&mut world, BLUE, 10.0, 20.0);
    let target = spawn(&mut world, "shock_golem", RED, 15.0, 20.0);
    let walk = walk_east(&mut world, RED, target);
    let gap = world.entities[&target]
        .position
        .distance(world.entities[&caster].position)
        - radius(&world, target);
    let bound = closing_bound(gap, SimScalar::from_num(1.05), walk);
    let cast_tick = world.current_tick;
    assert!(abduct(&mut world, caster, target));
    let contact = (0..bound).find_map(|_| {
        let tick = world.current_tick;
        step(&mut world);
        abducted(&world, target).then_some(tick)
    });
    assert!(
        contact.is_some(),
        "contact within {bound} ticks of the cast at {cast_tick}"
    );
    assert!(
        world.entities[&target].position.x < n(50),
        "contact while the unit still walks"
    );
}

/// T-M3 (plan `2026-11-11-contact-and-missiles` WP1): the geometry of the evidence's traced
/// second tongue (spellcaster evidence §7), which B-114's mark now keeps from happening through
/// Abduct, with a scratch missile at the tongue's speed: one target walks away from the
/// launch, one is flown away by a scratch `Displace` at 0.55 a tick. Each is reached within
/// the closing bound, the flown one before its flight ends.
#[test]
fn a_missile_reaches_a_walking_and_a_flown_receding_target() {
    let statuses = r#"(name: "struck", duration_ticks: 100.0)"#;
    let effects = r#"(name: "probe_missile", node: LaunchMissile((to: TargetUnit, speed: 1.05,
            view: Tether, impact: ApplyStatus((status: "struck"))))),
        (name: "shove", node: Displace((flight_ticks: 40)))"#;
    for flown in [false, true] {
        let label = if flown { "flown" } else { "walking" };
        let mut world = effect_support::world(statuses, effects);
        let struck = world
            .catalog
            .spells()
            .status_id("struck")
            .expect("the scratch status");
        let target = effect_support::unit(&mut world, 0, 18, 20);
        let receding = if flown {
            let mut push =
                effect_support::context(&world, None, sim_types::EffectTarget::Unit(target));
            push.origin = SimVec2::from_i32(40, 20);
            effect_support::run(&mut world, "shove", push);
            SimScalar::from_num(22.0 / 40.0)
        } else {
            walk_east(&mut world, PlayerId(0), target)
        };
        let launch = at(10.0, 20.0);
        let gap = world.entities[&target].position.distance(launch) - radius(&world, target);
        let bound = closing_bound(gap, SimScalar::from_num(1.05), receding);
        let mut context =
            effect_support::context(&world, None, sim_types::EffectTarget::Unit(target));
        context.source_point = launch;
        effect_support::run(&mut world, "probe_missile", context);
        let reached = (0..bound).any(|_| {
            step(&mut world);
            world.effects().has_status(target, struck)
        });
        assert!(reached, "{label}: reached within {bound} ticks");
        if flown {
            assert!(
                world.entities[&target].displacement.is_some(),
                "flown: reached during the flight"
            );
        }
    }
}
