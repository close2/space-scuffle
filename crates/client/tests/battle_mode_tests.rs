//! Mode commands (Siege Mode, Burrow and their way back) played with real input.
//!
//! # Responsibility
//! Boots the production client on the shipped skirmish and switches unit modes with the keys and
//! clicks a player would use, then asserts on the commands captured and on what the simulation
//! did: a tank sieges and unsieges, a unit armed only in its mode burrows and kills, a walking
//! unit drops its walk when it switches, Shift queues a switch, a unit leaving its mode walks
//! after the channel, a mixed selection switches each unit its own way, and a switching unit
//! says why it cannot switch again. This is Q0070's proof that the Lurker analogue can fight in a
//! skirmish (plan `2026-11-09-client-mode-commands` WP5, D-10).
//!
//! Stop and Hold Position on anchored units, by the stance's data (plan
//! `2026-11-23-anchored-commands` P-5, P-6): `X` and the card's Stop clear an anchored unit's
//! focused target, which then picks its target as it would idle, and leave a switch in progress
//! running; `H` holds a stance that keeps Hold Position, which then fights an enemy walking into
//! range; `H` reaches nothing in a stance without it, alone or beside a unit that takes it.
//!
//! # Limits
//! Shares the limits of `support/battle_harness.rs`: no window and no renderer. Mode entries are
//! drawn by the generic slot code, which the HUD window GPU tests render.
//!
//! # Invariants
//! - Setup may spawn units or structures, lower an enemy's life, or (for the unit armed only in
//!   its mode, see below) edit the session's catalog through `world_mut`, as the harness allows.
//!   Every order under test arrives through an input event.
//! - Archetypes are chosen by capability (a mode ability; armed by default or not) among those
//!   the client can present, never by name. The client stops a battle that would show a unit
//!   without media, so a test may only spawn presentable identities.
//! - The shipped unit armed only in its mode (the Lurker analogue) has no media yet. Until it
//!   has, `a_unit_armed_only_in_its_mode_burrows_and_kills` takes the presentable mode unit and
//!   removes its own weapons from the session's catalog, so the unit is armed only in its mode;
//!   the moment a presentable archetype of that kind ships, the test uses it unchanged.
//! - Every test fails on any simulation diagnostic, as `skirmish_match_tests.rs` does.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use bevy::prelude::*;
use bevy_egui::egui;
use client::battle_hud::card::{CardAction, CardLock, CommandCard};
use client::battle_hud::model::HudModel;
use client::battle_hud::HudGeometry;
use client::battle_pipeline::commands::{is_armed, mode_of_ability, ModeDirection};
use client::battle_pipeline::session::BattleSession;
use client::input_bindings::{HOLD_POSITION_KEY, SHIFT_MODIFIERS, STOP_KEY};
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::ClientMode;
use sim_core::UnitArchetype;
use sim_types::{EntityId, PlayerCommand, PlayerId, ShapeKind, SimScalar, UnitOrder, UnitTypeId};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{Harness, LOCAL_PLAYER};

const ENEMY: PlayerId = PlayerId(1);

/// Ticks within which every shipped mode channel ends (the longest is 81).
const CHANNEL_TICKS: u32 = 120;

fn skirmish() -> Harness {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = worker(&harness);
    let position = harness.simulated_position(worker);
    harness.look_at(position);
    harness
}

/// The local player's first worker, whose position is open ground in view.
fn worker(harness: &Harness) -> EntityId {
    let world = harness.world();
    world
        .entities
        .values()
        .find(|unit| {
            unit.owner == LOCAL_PLAYER
                && world
                    .catalog
                    .get(unit.unit_type_id)
                    .is_ok_and(|archetype| archetype.capabilities.can_construct)
        })
        .expect("the skirmish starts with workers")
        .id
}

/// The first mode ability's mode of `archetype`, if it has one.
fn mode_of(archetype: &UnitArchetype) -> Option<&str> {
    archetype
        .abilities
        .iter()
        .find_map(|ability| mode_of_ability(archetype, ability))
}

/// Whether the client can present `unit_type`: the HUD catalog records why it cannot.
fn presentable(harness: &Harness, unit_type: UnitTypeId) -> bool {
    harness
        .app
        .world()
        .resource::<HudModel>()
        .catalog
        .archetype(unit_type)
        .is_some_and(|commands| commands.unpresentable.is_none())
}

/// Whether `archetype`'s mode replaces its ground weapon with one.
fn mode_arms(archetype: &UnitArchetype) -> bool {
    mode_of(archetype).is_some_and(|mode| {
        archetype
            .get_mode(mode)
            .is_some_and(|spec| matches!(spec.ground_weapon_override, Some(Some(_))))
    })
}

/// A presentable mobile archetype with a mode ability, armed by default exactly when `armed`;
/// when not armed, its mode must arm it.
fn find_mode_type(harness: &Harness, armed: bool) -> Option<UnitTypeId> {
    harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && is_armed(archetype) == armed
                && mode_of(archetype).is_some()
                && (armed || mode_arms(archetype))
                && presentable(harness, archetype.unit_type_id)
        })
        .map(|archetype| archetype.unit_type_id)
}

/// A presentable mobile archetype armed by default with a mode ability.
fn mode_type(harness: &Harness) -> UnitTypeId {
    find_mode_type(harness, true).expect("the catalog ships a presentable armed mode unit")
}

/// A presentable mobile archetype armed only in its mode: a shipped one if one is presentable,
/// otherwise the presentable mode unit whose mode arms it, its own weapons removed from the
/// session's catalog (see the module invariants).
fn armed_only_in_mode_type(harness: &mut Harness) -> UnitTypeId {
    if let Some(unit_type) = find_mode_type(harness, false) {
        return unit_type;
    }
    let unit_type = harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && mode_arms(archetype)
                && presentable(harness, archetype.unit_type_id)
        })
        .expect("the catalog ships a presentable unit whose mode arms it")
        .unit_type_id;
    let archetype = harness.world_mut().catalog.get_mut(unit_type).unwrap();
    archetype.weapon = None;
    archetype.ground_weapon = None;
    archetype.air_weapon = None;
    unit_type
}

fn run_ticks(harness: &mut Harness, ticks: u32) {
    let goal = harness.world().current_tick + ticks;
    harness.resume();
    let mut frames = 0;
    while harness.world().current_tick < goal {
        harness.step(1);
        frames += 1;
        assert!(frames < 10_000, "the battle stopped ticking");
    }
    harness.pause();
    assert_no_diagnostics(harness);
}

fn assert_no_diagnostics(harness: &Harness) {
    let session = harness.app.world().resource::<BattleSession>();
    assert_eq!(session.diagnostics, [], "simulation diagnostics");
}

/// Spawns a unit of `owner` on open ground `distance` from `near`, with its bubble presented.
fn spawn_at(
    harness: &mut Harness,
    near: Vec2,
    distance: f32,
    unit_type: UnitTypeId,
    owner: PlayerId,
) -> EntityId {
    let at = harness.open_ground(near, distance);
    let unit = harness
        .world_mut()
        .spawn_unit(unit_type, owner, battle_harness::sim_position(at))
        .expect("open ground");
    run_ticks(harness, 1);
    unit
}

/// A unit of `unit_type` for the local player beside its first worker.
fn spawn_mine(harness: &mut Harness, unit_type: UnitTypeId) -> EntityId {
    let near = harness.simulated_position(worker(harness));
    spawn_at(harness, near, 4.0, unit_type, LOCAL_PLAYER)
}

/// An enemy structure that dies to one hit, about `distance` world units from `unit`: the
/// smallest presentable structure, which neither fights back nor walks out of range.
fn frail_enemy(harness: &mut Harness, unit: EntityId, distance: f32) -> EntityId {
    let enemy = enemy_structure(harness, unit, distance);
    harness.world_mut().entities.get_mut(&enemy).unwrap().hp = SimScalar::ONE;
    run_ticks(harness, 1);
    enemy
}

/// An enemy structure at its full life, about `distance` world units from `unit`: the smallest
/// presentable unarmed structure, which neither fights back nor walks out of range.
fn enemy_structure(harness: &mut Harness, unit: EntityId, distance: f32) -> EntityId {
    let structure = harness
        .world()
        .catalog
        .archetypes()
        .filter(|archetype| {
            matches!(archetype.shape, ShapeKind::BuildingDome { .. })
                && !is_armed(archetype)
                && presentable(harness, archetype.unit_type_id)
        })
        .min_by_key(|archetype| match archetype.shape {
            ShapeKind::BuildingDome {
                footprint_w,
                footprint_h,
                ..
            } => u32::from(footprint_w) * u32::from(footprint_h),
            ShapeKind::UnitSphere => u32::MAX,
        })
        .expect("the catalog ships a presentable unarmed structure")
        .unit_type_id;
    let near = harness.simulated_position(unit);
    let enemy = (0..16)
        .find_map(|step| {
            let at = near + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 16.0) * distance;
            let (x, y) = harness
                .world()
                .map
                .world_to_cell(battle_harness::sim_position(at))
                .ok()?;
            harness
                .world_mut()
                .spawn_structure(structure, ENEMY, x, y)
                .ok()
        })
        .expect("open ground for a structure near the unit");
    run_ticks(harness, 1);
    enemy
}

fn card(harness: &Harness) -> &CommandCard {
    harness.app.world().resource::<CommandCard>()
}

fn pending(harness: &Harness) -> Vec<PlayerCommand> {
    harness
        .app
        .world()
        .resource::<BattleSession>()
        .commands
        .pending_commands()
        .map(|(_, command)| command.clone())
        .collect()
}

/// The position of the mode entry in `direction`, if the card shows one.
fn mode_slot(harness: &Harness, direction: ModeDirection) -> Option<usize> {
    card(harness).slots.iter().position(|slot| {
        slot.as_ref().is_some_and(|slot| {
            matches!(slot.action, CardAction::SwitchMode { direction: d, .. } if d == direction)
        })
    })
}

fn key_at(harness: &Harness, position: usize) -> KeyCode {
    card(harness).slots[position]
        .as_ref()
        .expect("a filled slot")
        .key
}

/// Taps the key of the mode entry in `direction` and returns its position.
fn tap_mode(harness: &mut Harness, direction: ModeDirection) -> usize {
    harness.step(1);
    let position = mode_slot(harness, direction)
        .unwrap_or_else(|| panic!("no {direction:?} entry: {:?}", card(harness).slots));
    let key = key_at(harness, position);
    harness.tap_key(key);
    position
}

/// The ability index of `unit`'s mode ability.
fn ability_index(harness: &Harness, unit: EntityId) -> u8 {
    let world = harness.world();
    let archetype = world
        .catalog
        .get(world.entities[&unit].unit_type_id)
        .unwrap();
    let mode = mode_of(archetype).unwrap();
    client::battle_pipeline::commands::mode_ability_index(archetype, mode).unwrap()
}

/// Whether `unit` is in its mode now, not switching.
fn in_mode(harness: &Harness, unit: EntityId) -> bool {
    let entity = &harness.world().entities[&unit];
    entity.active_mode.is_some() && entity.channeling_remaining_ticks == 0
}

fn is_cast(command: &PlayerCommand, index: u8) -> bool {
    matches!(command.order, UnitOrder::CastAbility { ability_index, .. } if ability_index == index)
}

fn point(rect: egui::Rect) -> Vec2 {
    Vec2::new(rect.center().x, rect.center().y)
}

#[test]
fn siege_and_tank_mode_by_key() {
    let mut harness = skirmish();
    let tank_type = mode_type(&harness);
    let tank = spawn_mine(&mut harness, tank_type);
    harness.select(tank);
    assert_eq!(harness.selection().ids(), [tank]);

    let position = tap_mode(&mut harness, ModeDirection::Enter);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [tank]);
    assert!(is_cast(&orders[0], ability_index(&harness, tank)));
    assert!(!orders[0].queue);

    run_ticks(&mut harness, CHANNEL_TICKS);
    assert!(in_mode(&harness, tank), "sieged");
    harness.step(1);
    assert_eq!(
        mode_slot(&harness, ModeDirection::Leave),
        Some(position),
        "the leave entry takes the enter entry's place"
    );
    assert_eq!(mode_slot(&harness, ModeDirection::Enter), None);

    harness.tap_key(key_at(&harness, position));
    assert_eq!(pending(&harness).len(), 1);
    run_ticks(&mut harness, CHANNEL_TICKS);
    let entity = &harness.world().entities[&tank];
    assert_eq!(entity.active_mode, None, "back in tank mode");
    assert!(!entity.is_immobile, "and mobile");
}

#[test]
fn a_unit_armed_only_in_its_mode_burrows_and_kills() {
    let mut harness = skirmish();
    let lurker_type = armed_only_in_mode_type(&mut harness);
    let lurker = spawn_mine(&mut harness, lurker_type);
    let enemy = frail_enemy(&mut harness, lurker, 4.0);
    harness.select(lurker);
    assert_eq!(harness.selection().ids(), [lurker]);
    run_ticks(&mut harness, 50);
    assert!(
        harness.world().entities.contains_key(&enemy),
        "unburrowed, it has no weapon"
    );

    tap_mode(&mut harness, ModeDirection::Enter);
    assert_eq!(pending(&harness).len(), 1, "status: {}", harness.status());
    run_ticks(&mut harness, CHANNEL_TICKS + 50);
    assert!(in_mode(&harness, lurker), "burrowed");
    assert!(
        harness
            .world()
            .entities
            .get(&enemy)
            .is_none_or(|entity| !entity.is_alive()),
        "the burrowed unit killed the enemy"
    );
}

#[test]
fn a_walking_unit_switches_and_drops_its_move() {
    let mut harness = skirmish();
    let tank_type = mode_type(&harness);
    let tank = spawn_mine(&mut harness, tank_type);
    let enemy = frail_enemy(&mut harness, tank, 9.0);
    harness.select(tank);
    let far = harness.open_ground(harness.simulated_position(tank), 20.0);
    let far_on_screen = harness.ground_on_screen(far);
    harness.click(MouseButton::Right, far_on_screen);
    run_ticks(&mut harness, 2);
    assert!(
        matches!(
            harness.world().entities[&tank].order_queue.front(),
            Some(UnitOrder::Move { .. })
        ),
        "the tank walks"
    );

    tap_mode(&mut harness, ModeDirection::Enter);
    run_ticks(&mut harness, 1);
    assert!(
        harness.world().entities[&tank].order_queue.is_empty(),
        "the switch replaced the walk"
    );
    run_ticks(&mut harness, CHANNEL_TICKS + 60);
    assert!(in_mode(&harness, tank));
    assert!(
        harness
            .world()
            .entities
            .get(&enemy)
            .is_none_or(|entity| !entity.is_alive()),
        "the sieged tank fired at the enemy in range"
    );
}

#[test]
fn shift_queues_the_switch_behind_a_move() {
    let mut harness = skirmish();
    let tank_type = mode_type(&harness);
    let tank = spawn_mine(&mut harness, tank_type);
    harness.select(tank);
    let start = harness.simulated_position(tank);
    let goal = harness.open_ground(start, 6.0);
    let goal_on_screen = harness.ground_on_screen(goal);
    harness.click(MouseButton::Right, goal_on_screen);
    harness.with_key_held(SHIFT_MODIFIERS[0], |harness| {
        tap_mode(harness, ModeDirection::Enter);
    });
    let orders = pending(&harness);
    assert_eq!(orders.len(), 2, "status: {}", harness.status());
    assert!(matches!(orders[0].order, UnitOrder::Move { .. }));
    assert!(is_cast(&orders[1], ability_index(&harness, tank)));
    assert!(orders[1].queue, "Shift queues");

    run_ticks(&mut harness, 1);
    assert!(
        !harness.world().entities[&tank].is_immobile,
        "the tank walks first"
    );
    run_ticks(&mut harness, 200 + CHANNEL_TICKS);
    assert!(in_mode(&harness, tank), "then switches");
    let at = harness.simulated_position(tank);
    assert!(
        at.distance(goal) < at.distance(start),
        "it switched at the end of the walk: {at} from {start} to {goal}"
    );
}

#[test]
fn leave_then_right_click_walks_after_the_channel() {
    let mut harness = skirmish();
    let tank_type = mode_type(&harness);
    let tank = spawn_mine(&mut harness, tank_type);
    harness.select(tank);
    tap_mode(&mut harness, ModeDirection::Enter);
    run_ticks(&mut harness, CHANNEL_TICKS);
    assert!(in_mode(&harness, tank));

    tap_mode(&mut harness, ModeDirection::Leave);
    run_ticks(&mut harness, 1);
    assert!(harness.world().entities[&tank].channeling_remaining_ticks > 0);
    let start = harness.simulated_position(tank);
    let goal = harness.open_ground(start, 6.0);
    let goal_on_screen = harness.ground_on_screen(goal);
    harness.click(MouseButton::Right, goal_on_screen);
    let orders = pending(&harness);
    assert_eq!(
        orders.len(),
        1,
        "the move is admitted: {}",
        harness.status()
    );
    assert!(matches!(orders[0].order, UnitOrder::Move { .. }));

    run_ticks(&mut harness, CHANNEL_TICKS + 100);
    let entity = &harness.world().entities[&tank];
    assert_eq!(entity.active_mode, None);
    let at = harness.simulated_position(tank);
    assert!(
        at.distance(goal) < 1.5,
        "it walked once the channel ended: {at}, goal {goal}"
    );
}

#[test]
fn a_mixed_selection_switches_each_unit_its_way() {
    let mut harness = skirmish();
    let tank_type = mode_type(&harness);
    let sieged = spawn_mine(&mut harness, tank_type);
    let near = harness.simulated_position(sieged);
    let fresh = spawn_at(&mut harness, near, 2.5, tank_type, LOCAL_PLAYER);
    harness.select(sieged);
    tap_mode(&mut harness, ModeDirection::Enter);
    run_ticks(&mut harness, CHANNEL_TICKS);
    assert!(in_mode(&harness, sieged));

    let corners = [
        harness.unit_on_screen(sieged),
        harness.unit_on_screen(fresh),
    ];
    let from = corners[0].min(corners[1]) - Vec2::splat(12.0);
    let to = corners[0].max(corners[1]) + Vec2::splat(12.0);
    harness.drag(from, to);
    let selected = harness.selection().ids().to_vec();
    assert!(
        selected.contains(&sieged) && selected.contains(&fresh),
        "{selected:?}"
    );

    tap_mode(&mut harness, ModeDirection::Enter);
    tap_mode(&mut harness, ModeDirection::Leave);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 2, "status: {}", harness.status());
    assert_eq!(
        orders[0].entities,
        [fresh],
        "Siege Mode reaches the unsieged"
    );
    assert_eq!(orders[1].entities, [sieged], "Tank Mode reaches the sieged");
    run_ticks(&mut harness, CHANNEL_TICKS);
    assert!(in_mode(&harness, fresh));
    assert_eq!(harness.world().entities[&sieged].active_mode, None);
}

#[test]
fn a_key_while_transforming_says_why() {
    let mut harness = skirmish();
    let tank_type = mode_type(&harness);
    let tank = spawn_mine(&mut harness, tank_type);
    harness.select(tank);
    tap_mode(&mut harness, ModeDirection::Enter);
    run_ticks(&mut harness, 2);
    assert!(harness.world().entities[&tank].channeling_remaining_ticks > 0);
    harness.step(1);

    let position = mode_slot(&harness, ModeDirection::Leave).expect("the switching unit's entry");
    let slot = card(&harness).slots[position].as_ref().unwrap();
    assert!(!slot.enabled);
    assert_eq!(slot.lock, Some(CardLock::Transforming));
    let reason = card(&harness).text[position].tooltip.clone();
    assert_eq!(reason, client::battle_hud::card::TRANSFORMING_TEXT);

    harness.tap_key(key_at(&harness, position));
    assert_eq!(harness.pending_orders(), 0, "nothing is queued");
    assert!(harness.status().contains(&reason), "{}", harness.status());
}

#[test]
fn a_click_on_the_mode_slot_does_the_same_as_its_key() {
    let mut harness = skirmish();
    let tank_type = mode_type(&harness);
    let tank = spawn_mine(&mut harness, tank_type);
    harness.select(tank);
    harness.step(1);
    let position = mode_slot(&harness, ModeDirection::Enter).expect("the enter entry");
    harness.step(1);
    let rect = harness.app.world().resource::<HudGeometry>().card[position].expect("drawn");
    harness.click(MouseButton::Left, point(rect));
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [tank]);
    assert!(is_cast(&orders[0], ability_index(&harness, tank)));
    assert_eq!(
        harness.selection().ids(),
        [tank],
        "the click kept the selection"
    );
    run_ticks(&mut harness, CHANNEL_TICKS);
    assert!(in_mode(&harness, tank));
}

// --- Stop and Hold on anchored units (plan `2026-11-23-anchored-commands` P-5, P-6) ------------

/// A presentable mobile archetype with a mode ability whose mode anchors it, arms it, accepts
/// Stop, and accepts Hold Position exactly when `hold`; found by the mode's data.
fn anchoring_type(harness: &Harness, hold: bool) -> UnitTypeId {
    harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && mode_arms(archetype)
                && mode_of(archetype)
                    .and_then(|mode| archetype.get_mode(mode))
                    .is_some_and(|mode| {
                        mode.is_immobile && mode.orders.stop && mode.orders.hold_position == hold
                    })
                && presentable(harness, archetype.unit_type_id)
        })
        .map_or_else(
            || panic!("a presentable anchoring archetype, hold {hold}"),
            |archetype| archetype.unit_type_id,
        )
}

/// A unit of `unit_type` of the local player, selected and switched into its mode by its key.
fn anchored(harness: &mut Harness, unit_type: UnitTypeId) -> EntityId {
    let unit = spawn_mine(harness, unit_type);
    harness.select(unit);
    assert_eq!(harness.selection().ids(), [unit]);
    tap_mode(harness, ModeDirection::Enter);
    run_ticks(harness, CHANNEL_TICKS);
    assert!(in_mode(harness, unit), "anchored");
    unit
}

/// The rejection status a refused press writes.
fn refusal_status(error: &client::battle_pipeline::commands::SubmissionError) -> String {
    let mut status = String::new();
    client::battle_pipeline::status::write_submission_rejection(&mut status, error);
    status
}

/// P-5: a sieged tank told to attack one enemy and then given Stop by `press` has an empty
/// queue after the tick and picks the target it picked idle at that spot.
fn stop_clears_a_focused_target(press: impl FnOnce(&mut Harness)) {
    let mut harness = skirmish();
    let tank = {
        let unit_type = anchoring_type(&harness, false);
        anchored(&mut harness, unit_type)
    };
    let near = enemy_structure(&mut harness, tank, 6.0);
    let far = enemy_structure(&mut harness, tank, 10.0);
    run_ticks(&mut harness, 10);
    let idle_choice = harness.world().entities[&tank]
        .current_target
        .expect("idle, it acquires a target by itself");
    assert!(idle_choice == near || idle_choice == far);
    let other = if idle_choice == near { far } else { near };

    let on_screen = harness.unit_on_screen(other);
    harness.click(MouseButton::Right, on_screen);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::attack(other));
    run_ticks(&mut harness, 2);
    assert_eq!(harness.world().entities[&tank].current_target, Some(other));

    harness.step(1);
    press(&mut harness);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::Stop);
    assert_eq!(orders[0].entities, [tank]);
    run_ticks(&mut harness, 1);
    // The focused order is gone. The tick's own acquisition pass may already have put an
    // `Acquired` attack in front, which is the unit picking its next target by itself.
    let queue = &harness.world().entities[&tank].order_queue;
    assert!(
        queue.iter().all(|order| matches!(
            order,
            UnitOrder::AttackTarget {
                origin: sim_types::AttackOrigin::Acquired,
                ..
            }
        )),
        "{queue:?}"
    );
    run_ticks(&mut harness, 10);
    let entity = &harness.world().entities[&tank];
    assert_eq!(
        entity.current_target,
        Some(idle_choice),
        "it picks by the ordinary ranking again"
    );
    assert!(in_mode(&harness, tank), "still anchored");
}

#[test]
fn stop_by_key_clears_a_sieged_tanks_focused_target() {
    stop_clears_a_focused_target(|harness| harness.tap_key(STOP_KEY));
}

#[test]
fn stop_by_card_clears_a_sieged_tanks_focused_target() {
    stop_clears_a_focused_target(|harness| {
        let rect = harness.app.world().resource::<HudGeometry>().card_fixed[0]
            .expect("the card draws Stop for a sieged tank");
        harness.click(MouseButton::Left, point(rect));
    });
}

#[test]
fn stop_while_sieging_leaves_the_switch_running() {
    let mut harness = skirmish();
    let tank = {
        let unit_type = anchoring_type(&harness, false);
        spawn_mine(&mut harness, unit_type)
    };
    harness.select(tank);
    tap_mode(&mut harness, ModeDirection::Enter);
    run_ticks(&mut harness, 2);
    assert!(harness.world().entities[&tank].channeling_remaining_ticks > 0);

    harness.tap_key(STOP_KEY);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::Stop);
    run_ticks(&mut harness, CHANNEL_TICKS);
    assert!(in_mode(&harness, tank), "the switch ran to its end");
}

#[test]
fn hold_keeps_a_burrowed_unit_fighting_an_enemy_that_walks_into_range() {
    let mut harness = skirmish();
    let lurker = {
        let unit_type = anchoring_type(&harness, true);
        anchored(&mut harness, unit_type)
    };
    let place = harness.simulated_position(lurker);

    harness.tap_key(HOLD_POSITION_KEY);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::HoldPosition);
    assert_eq!(orders[0].entities, [lurker]);
    run_ticks(&mut harness, 1);
    assert_eq!(
        harness.world().entities[&lurker].order_queue.front(),
        Some(&UnitOrder::HoldPosition)
    );

    let walker = harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && archetype.mover_type == sim_types::MoverType::Ground
                && archetype.modes.is_empty()
                && !archetype.is_detector()
                && presentable(&harness, archetype.unit_type_id)
        })
        .expect("a presentable ground walker")
        .unit_type_id;
    let enemy = spawn_at(&mut harness, place, 14.0, walker, ENEMY);
    let goal = battle_harness::sim_position(place);
    let world = harness.world_mut();
    world.entities.get_mut(&enemy).unwrap().hp = SimScalar::ONE;
    world
        .entities
        .get_mut(&enemy)
        .unwrap()
        .issue_order(UnitOrder::Move { target: goal }, false)
        .expect("the enemy walks");
    run_ticks(&mut harness, 300);
    assert!(
        harness
            .world()
            .entities
            .get(&enemy)
            .is_none_or(|entity| !entity.is_alive()),
        "the holding unit killed the enemy that walked into range"
    );
    let entity = &harness.world().entities[&lurker];
    assert_eq!(entity.order_queue.front(), Some(&UnitOrder::HoldPosition));
    assert!(harness.simulated_position(lurker).distance(place) < 0.01);
}

#[test]
fn hold_on_a_sieged_tank_alone_is_refused_with_its_status() {
    let mut harness = skirmish();
    let tank = {
        let unit_type = anchoring_type(&harness, false);
        anchored(&mut harness, unit_type)
    };
    harness.step(1);
    assert!(
        harness.app.world().resource::<HudGeometry>().card_fixed[1].is_none(),
        "no Hold on the card"
    );
    harness.tap_key(HOLD_POSITION_KEY);
    assert_eq!(harness.pending_orders(), 0, "nothing is queued");
    let status =
        refusal_status(&client::battle_pipeline::commands::SubmissionError::EmptySelection);
    assert_eq!(harness.status(), status);
    assert!(harness.world().entities[&tank].order_queue.is_empty());
}

#[test]
fn hold_on_a_sieged_tank_beside_a_walker_reaches_the_walker_only() {
    let mut harness = skirmish();
    let tank = {
        let unit_type = anchoring_type(&harness, false);
        anchored(&mut harness, unit_type)
    };
    let walker_type = harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && is_armed(archetype)
                && archetype.modes.is_empty()
                && archetype.capabilities.harvester.is_none()
                && presentable(&harness, archetype.unit_type_id)
        })
        .expect("a presentable armed walker without modes")
        .unit_type_id;
    let near = harness.simulated_position(tank);
    let walker = spawn_at(&mut harness, near, 2.5, walker_type, LOCAL_PLAYER);
    let corners = [harness.unit_on_screen(tank), harness.unit_on_screen(walker)];
    let from = corners[0].min(corners[1]) - Vec2::splat(12.0);
    let to = corners[0].max(corners[1]) + Vec2::splat(12.0);
    harness.drag(from, to);
    let selected = harness.selection().ids().to_vec();
    assert!(
        selected.contains(&tank) && selected.contains(&walker),
        "{selected:?}"
    );

    harness.tap_key(HOLD_POSITION_KEY);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::HoldPosition);
    assert_eq!(
        orders[0].entities,
        [walker],
        "the sieged tank takes no Hold"
    );
    run_ticks(&mut harness, 1);
    assert!(harness.world().entities[&tank].order_queue.is_empty());
    assert_eq!(
        harness.world().entities[&walker].order_queue.front(),
        Some(&UnitOrder::HoldPosition)
    );
}

// --- Attack by stance (plan `2026-11-23-stance-checked-orders` D-4, D-6, D-7; P-7) -------------

/// The armed command mode.
fn command_mode(harness: &Harness) -> client::battle_input::command_mode::CommandMode {
    *harness
        .app
        .world()
        .resource::<client::battle_input::command_mode::CommandMode>()
}

/// Arms the Attack mode by the key of the card's Attack entry.
fn arm_attack(harness: &mut Harness) {
    use client::battle_input::command_mode::CommandMode;
    harness.step(1);
    let position = card(harness)
        .slots
        .iter()
        .position(|slot| {
            slot.as_ref()
                .is_some_and(|slot| slot.action == CardAction::Mode(CommandMode::AttackMove))
        })
        .unwrap_or_else(|| panic!("no Attack entry: {:?}", card(harness).slots));
    let key = key_at(harness, position);
    harness.tap_key(key);
    assert_eq!(command_mode(harness), CommandMode::AttackMove);
}

/// Whether `unit` carries an attack it was told to make on `target`.
fn attacks(harness: &Harness, unit: EntityId, target: EntityId) -> bool {
    harness.world().entities[&unit]
        .order_queue
        .iter()
        .any(|order| {
            *order
                == UnitOrder::AttackTarget {
                    target,
                    origin: sim_types::AttackOrigin::Explicit,
                }
        })
}

/// An anchored unit of `unit_type`: `A` arms Attack, a click on a visible enemy in range
/// attacks it, and a click on the ground is refused with the `EmptySelection` status while the
/// mode stays armed (Q0098's default, D-7).
fn anchored_unit_attacks_by_click(hold: bool) {
    use client::battle_input::command_mode::CommandMode;
    let mut harness = skirmish();
    let unit = {
        let unit_type = anchoring_type(&harness, hold);
        anchored(&mut harness, unit_type)
    };
    let enemy = enemy_structure(&mut harness, unit, 5.0);
    harness.select(unit);
    arm_attack(&mut harness);
    let on_screen = harness.unit_on_screen(enemy);
    harness.click(MouseButton::Left, on_screen);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::attack(enemy));
    assert_eq!(orders[0].entities, [unit]);
    assert_eq!(
        command_mode(&harness),
        CommandMode::Normal,
        "the order ends the mode"
    );
    run_ticks(&mut harness, 1);
    assert!(
        attacks(&harness, unit, enemy) || !harness.world().entities.contains_key(&enemy),
        "an explicit attack on the clicked enemy"
    );

    arm_attack(&mut harness);
    let ground = harness.open_ground(harness.simulated_position(unit), 4.0);
    let on_screen = harness.ground_on_screen(ground);
    harness.click(MouseButton::Left, on_screen);
    assert_eq!(harness.pending_orders(), 0, "nothing is queued");
    let status =
        refusal_status(&client::battle_pipeline::commands::SubmissionError::EmptySelection);
    assert_eq!(harness.status(), status);
    assert_eq!(
        command_mode(&harness),
        CommandMode::AttackMove,
        "the mode stays armed"
    );
    assert!(in_mode(&harness, unit), "still anchored");
}

#[test]
fn a_sieged_tank_attacks_a_clicked_enemy_and_refuses_the_ground() {
    anchored_unit_attacks_by_click(false);
}

#[test]
fn a_burrowed_unit_shows_attack_and_attacks_a_clicked_enemy() {
    anchored_unit_attacks_by_click(true);
}

#[test]
fn attack_on_the_ground_with_a_sieged_tank_and_a_walker_moves_the_walker_only() {
    let mut harness = skirmish();
    let tank = {
        let unit_type = anchoring_type(&harness, false);
        anchored(&mut harness, unit_type)
    };
    let walker_type = harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && is_armed(archetype)
                && archetype.modes.is_empty()
                && archetype.capabilities.harvester.is_none()
                && presentable(&harness, archetype.unit_type_id)
        })
        .expect("a presentable armed walker without modes")
        .unit_type_id;
    let near = harness.simulated_position(tank);
    let walker = spawn_at(&mut harness, near, 2.5, walker_type, LOCAL_PLAYER);
    let corners = [harness.unit_on_screen(tank), harness.unit_on_screen(walker)];
    let from = corners[0].min(corners[1]) - Vec2::splat(12.0);
    let to = corners[0].max(corners[1]) + Vec2::splat(12.0);
    harness.drag(from, to);
    let selected = harness.selection().ids().to_vec();
    assert!(
        selected.contains(&tank) && selected.contains(&walker),
        "{selected:?}"
    );

    arm_attack(&mut harness);
    let ground = harness.open_ground(near, 6.0);
    let on_screen = harness.ground_on_screen(ground);
    harness.click(MouseButton::Left, on_screen);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(
        matches!(orders[0].order, UnitOrder::AttackMove { .. }),
        "{:?}",
        orders[0].order
    );
    assert_eq!(
        orders[0].entities,
        [walker],
        "the sieged tank takes no attack-move"
    );
}

// --- The A command by stance (plan `2026-11-24-attack-orders` P-7; B-174, B-175, B-186) -----

/// A presentable mobile archetype whose default stance has the A command `attack` and which
/// carries no weapon in it: Scan Move for the Medivac class, Attack for the unburrowed Lurker
/// analogue (S6, S7); found by the stance's data, never by name.
fn unarmed_with(harness: &Harness, attack: sim_types::AttackCommand) -> UnitTypeId {
    harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            !archetype.is_immobile_by_default()
                && !is_armed(archetype)
                && archetype.stance_orders(None).attack == attack
                && presentable(harness, archetype.unit_type_id)
        })
        .unwrap_or_else(|| panic!("the catalog ships a presentable unarmed unit with {attack:?}"))
        .unit_type_id
}

/// A presentable armed ground walker without modes that is no worker (the marine class).
fn marine_type(harness: &Harness) -> UnitTypeId {
    harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && is_armed(archetype)
                && archetype.modes.is_empty()
                && archetype.capabilities.harvester.is_none()
                && archetype.mover_type == sim_types::MoverType::Ground
                && presentable(harness, archetype.unit_type_id)
        })
        .expect("a presentable armed walker without modes")
        .unit_type_id
}

/// The label of the card's A slot, if it shows one.
fn a_slot_label(harness: &Harness) -> Option<client::battle_hud::card::CardLabel> {
    use client::battle_input::command_mode::CommandMode;
    card(harness).slots.iter().flatten().find_map(|slot| {
        (slot.action == CardAction::Mode(CommandMode::AttackMove)).then_some(slot.label)
    })
}

/// Selects `units` by clicking the first and Shift-clicking the rest.
fn select_all(harness: &mut Harness, units: &[EntityId]) {
    harness.select(units[0]);
    for unit in &units[1..] {
        let on_screen = harness.unit_on_screen(*unit);
        harness.with_key_held(KeyCode::ShiftLeft, |harness| {
            harness.step(1);
            harness.click(MouseButton::Left, on_screen);
            harness.step(1);
        });
    }
    harness.step(1);
    let mut selected = harness.selection().ids().to_vec();
    selected.sort_unstable();
    let mut wanted = units.to_vec();
    wanted.sort_unstable();
    assert_eq!(selected, wanted, "the selection");
}

/// Arms the A mode and left-clicks `unit`; returns the captured commands.
fn a_click_on_unit(harness: &mut Harness, unit: EntityId) -> Vec<PlayerCommand> {
    arm_attack(harness);
    let on_screen = harness.unit_on_screen(unit);
    harness.click(MouseButton::Left, on_screen);
    pending(harness)
}

/// Arms the A mode and left-clicks open ground `distance` from `near`; returns the commands.
fn a_click_on_ground(harness: &mut Harness, near: EntityId, distance: f32) -> Vec<PlayerCommand> {
    arm_attack(harness);
    let ground = harness.open_ground(harness.simulated_position(near), distance);
    let on_screen = harness.ground_on_screen(ground);
    harness.click(MouseButton::Left, on_screen);
    pending(harness)
}

/// Whether `order` is an attack-move to within half a world unit of `unit`'s position.
fn attack_moves_to(harness: &Harness, order: &UnitOrder, unit: EntityId) -> bool {
    let at = harness.world().entities[&unit].position;
    matches!(order, UnitOrder::AttackMove { target } if target.distance(at) < SimScalar::from_num(0.5))
}

/// P-7: a Medivac-class unit alone shows Scan Move; `A` then a ground click attack-moves it,
/// and `A` then a click on an enemy attack-moves it to the point under the enemy.
#[test]
fn a_scan_mover_shows_scan_move_and_attack_moves_on_ground_and_on_an_enemy() {
    use client::battle_hud::card::CardLabel;
    let mut harness = skirmish();
    let kind = unarmed_with(&harness, sim_types::AttackCommand::ScanMove);
    let unit = spawn_mine(&mut harness, kind);
    harness.select(unit);
    harness.step(1);
    assert_eq!(a_slot_label(&harness), Some(CardLabel::ScanMove));

    let orders = a_click_on_ground(&mut harness, unit, 5.0);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(
        matches!(orders[0].order, UnitOrder::AttackMove { .. }),
        "{:?}",
        orders[0].order
    );
    assert_eq!(orders[0].entities, [unit]);
    run_ticks(&mut harness, 1);

    let enemy = enemy_structure(&mut harness, unit, 6.0);
    let orders = a_click_on_unit(&mut harness, enemy);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(
        attack_moves_to(&harness, &orders[0].order, enemy),
        "{:?}",
        orders[0].order
    );
    assert_eq!(orders[0].entities, [unit]);
    assert_no_diagnostics(&harness);
}

/// Plan `2026-11-26-follow-order` P-11 (D-10, B-187): the mixed army A-clicked on an enemy air
/// unit: the Medivac-class unit follows it with Scan Move, the marine-class unit attacks it when
/// its weapons reach the air (else it attack-moves to the point, Q0105), and the unburrowed
/// Lurker analogue, which cannot attack it, attack-moves to the point under it.
#[test]
fn a_mixed_army_a_clicked_on_an_air_unit_follows_attacks_and_walks() {
    let mut harness = skirmish();
    let [marine, medivac, lurker] = mixed_army(&mut harness);
    let flyer = harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.mover_type == sim_types::MoverType::Air
                && !archetype.is_immobile_by_default()
                && presentable(&harness, archetype.unit_type_id)
        })
        .expect("a presentable air unit")
        .unit_type_id;
    let near = harness.simulated_position(marine);
    let enemy = spawn_at(&mut harness, near, 10.0, flyer, ENEMY);
    let reaches_air = {
        let world = harness.world();
        let (unit, target) = (&world.entities[&marine], &world.entities[&enemy]);
        unit.can_attack(
            world.catalog.get(unit.unit_type_id).unwrap(),
            target,
            world.catalog.get(target.unit_type_id).unwrap(),
        )
    };
    let orders = a_click_on_unit(&mut harness, enemy);
    let follow = UnitOrder::Follow {
        target: enemy,
        kind: sim_types::FollowKind::AcquireMove,
    };
    let find = |order: &dyn Fn(&UnitOrder) -> bool| {
        orders
            .iter()
            .find(|command| order(&command.order))
            .map(|command| command.entities.clone())
    };
    assert_eq!(
        find(&|order| *order == follow),
        Some(vec![medivac]),
        "{orders:?}"
    );
    let walkers = find(&|order| attack_moves_to(&harness, order, enemy)).unwrap_or_default();
    if reaches_air {
        assert_eq!(
            find(&|order| *order == UnitOrder::attack(enemy)),
            Some(vec![marine])
        );
        assert_eq!(walkers, [lurker]);
    } else {
        let mut expected = vec![marine, lurker];
        expected.sort_unstable();
        let mut walkers = walkers;
        walkers.sort_unstable();
        assert_eq!(walkers, expected);
    }
    run_ticks(&mut harness, 1);
    assert!(
        harness.world().entities[&medivac].order_queue.front() == Some(&follow)
            || !harness.world().entities.contains_key(&enemy),
        "the Medivac-class unit follows"
    );
}

/// P-7: the unburrowed Lurker analogue shows Attack and attack-moves on the ground.
#[test]
fn the_unburrowed_lurker_analogue_shows_attack_and_attack_moves_on_the_ground() {
    use client::battle_hud::card::CardLabel;
    let mut harness = skirmish();
    let kind = unarmed_with(&harness, sim_types::AttackCommand::Attack);
    let unit = spawn_mine(&mut harness, kind);
    harness.select(unit);
    harness.step(1);
    assert_eq!(a_slot_label(&harness), Some(CardLabel::Attack));
    let orders = a_click_on_ground(&mut harness, unit, 5.0);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(
        matches!(orders[0].order, UnitOrder::AttackMove { .. }),
        "{:?}",
        orders[0].order
    );
    assert_eq!(orders[0].entities, [unit]);
    run_ticks(&mut harness, 1);
    assert!(
        matches!(
            harness.world().entities[&unit].order_queue.front(),
            Some(UnitOrder::AttackMove { .. })
        ),
        "the unarmed attack-mover walks"
    );
}

/// The mixed army of P-7: a marine-class unit, a Medivac-class unit and the unburrowed Lurker
/// analogue, side by side and selected, in that order.
fn mixed_army(harness: &mut Harness) -> [EntityId; 3] {
    let kinds = [
        marine_type(harness),
        unarmed_with(harness, sim_types::AttackCommand::ScanMove),
        unarmed_with(harness, sim_types::AttackCommand::Attack),
    ];
    let marine = spawn_mine(harness, kinds[0]);
    let near = harness.simulated_position(marine);
    let medivac = spawn_at(harness, near, 4.0, kinds[1], LOCAL_PLAYER);
    let lurker = spawn_at(harness, near, 8.0, kinds[2], LOCAL_PLAYER);
    let army = [marine, medivac, lurker];
    select_all(harness, &army);
    army
}

/// P-7: a mixed army A-clicked on the ground sends one attack-move naming all three; A-clicked
/// on an enemy ground unit, it sends an attack to the marine-class unit and an attack-move to
/// the point under the enemy to the other two.
#[test]
fn a_mixed_army_attack_moves_together_and_splits_on_an_enemy() {
    let mut harness = skirmish();
    let [marine, medivac, lurker] = mixed_army(&mut harness);
    let orders = a_click_on_ground(&mut harness, marine, 14.0);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(
        matches!(orders[0].order, UnitOrder::AttackMove { .. }),
        "{:?}",
        orders[0].order
    );
    let mut named = orders[0].entities.clone();
    named.sort_unstable();
    let mut army = vec![marine, medivac, lurker];
    army.sort_unstable();
    assert_eq!(named, army, "one attack-move naming all three");
    run_ticks(&mut harness, 1);

    let enemy = enemy_structure(&mut harness, marine, 16.0);
    let orders = a_click_on_unit(&mut harness, enemy);
    assert_eq!(orders.len(), 2, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::attack(enemy));
    assert_eq!(orders[0].entities, [marine]);
    assert!(
        attack_moves_to(&harness, &orders[1].order, enemy),
        "{:?}",
        orders[1].order
    );
    let mut walkers = orders[1].entities.clone();
    walkers.sort_unstable();
    let mut expected = vec![medivac, lurker];
    expected.sort_unstable();
    assert_eq!(walkers, expected);
    assert!(
        matches!(
            harness.marker(),
            Some((
                client::battle_input::order_marker::OrderMarkerKind::Attack,
                _
            ))
        ),
        "one Attack marker"
    );
    run_ticks(&mut harness, 1);
    assert!(attacks(&harness, marine, enemy) || !harness.world().entities.contains_key(&enemy));
}

/// P-7 (B-175, Q0104's default): A-clicked on one's own unit, the armed member attacks it (an
/// explicit target after the tick), the others attack-move to it, and the clicked unit, though
/// selected, gets neither order. A right click on one's own unit is no attack.
#[test]
fn an_a_click_on_ones_own_unit_attacks_it() {
    let mut harness = skirmish();
    let [marine, medivac, lurker] = mixed_army(&mut harness);
    let near = harness.simulated_position(marine);
    let kind = marine_type(&harness);
    let own = spawn_at(&mut harness, near, 11.0, kind, LOCAL_PLAYER);
    select_all(&mut harness, &[marine, medivac, lurker, own]);
    let orders = a_click_on_unit(&mut harness, own);
    assert_eq!(orders.len(), 2, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::attack(own));
    assert_eq!(orders[0].entities, [marine]);
    assert!(
        attack_moves_to(&harness, &orders[1].order, own),
        "{:?}",
        orders[1].order
    );
    let mut walkers = orders[1].entities.clone();
    walkers.sort_unstable();
    let mut expected = vec![medivac, lurker];
    expected.sort_unstable();
    assert_eq!(walkers, expected, "the clicked unit gets neither order");
    run_ticks(&mut harness, 1);
    assert!(
        attacks(&harness, marine, own),
        "an explicit attack on the own unit"
    );
    assert!(harness.world().entities[&own].order_queue.is_empty());

    harness.select(medivac);
    let on_screen = harness.unit_on_screen(own);
    harness.click(MouseButton::Right, on_screen);
    assert!(
        pending(&harness)
            .iter()
            .all(|command| !matches!(command.order, UnitOrder::AttackTarget { .. })),
        "a right click on an own unit attacks nothing"
    );
    select_all(&mut harness, &[marine]);
    let on_screen = harness.unit_on_screen(own);
    harness.click(MouseButton::Right, on_screen);
    assert!(
        pending(&harness)
            .iter()
            .all(|command| !matches!(command.order, UnitOrder::AttackTarget { .. })),
        "a right click on an own unit attacks nothing"
    );
}
