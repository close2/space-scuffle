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
use client::input_bindings::SHIFT_MODIFIERS;
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
    harness.world_mut().entities.get_mut(&enemy).unwrap().hp = SimScalar::ONE;
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
