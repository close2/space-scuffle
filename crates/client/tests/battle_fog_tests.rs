//! Fog of war as the player meets it, proved on the real client with real input events.
//!
//! # Responsibility
//! Boots the production client on the shipped skirmish, as `client --skirmish` does, and proves what
//! reaches the screen under `BattleProfile::Player`: enemy bubbles outside `Visible` cells are not
//! drawn and not pickable, their combat events are not delivered, resource nodes keep their last
//! seen state, and a unit that has left sight leaves the selection panel. The same gestures under
//! `Sandbox` must change nothing.
//!
//! # How it fits
//! The policy of a single cell, node or event lives in `battle_pipeline::fog`, tested there.
//! This file proves the policy reaches Bevy: every claim about the picture is read from
//! `InheritedVisibility`, the component the renderer and the picker both obey, and never from the
//! fog's own data.
//!
//! # Limits
//! Shares the limits of `support/battle_harness.rs`: no window and no renderer. No test here can
//! see a pixel, so "drawn" means Bevy would submit the entity to a view.
//!
//! # Invariants
//! - Setup may write the world through `world_mut`, as the harness allows, and may order the
//!   enemy through the battle queue. An order under test arrives through an input event.
//! - Each negative claim has a control in the same file showing that the gesture or the event does
//!   work when the enemy is in sight, so no test passes because nothing was happening.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use bevy::prelude::*;
use client::battle_hud::model::HudModel;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::scene_controller::RenderEntityMap;
use client::smog::SmogStats;
use client::ClientMode;
use sim_types::{AttackOrigin, EntityId, PlayerCommand, PlayerId, UnitOrder, UnitTypeId};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{sim_position, Harness, LOCAL_PLAYER};

const ENEMY: PlayerId = PlayerId(1);

/// A soldier with shipped media and a weapon.
const SOLDIER: &str = "pulse_bot";

/// Frames allowed for any walk here, at one tick a frame.
const WALK_FRAMES: usize = 3_000;

/// More than a soldier's sight, in world units, so its spawn shows nothing of the target.
const OUT_OF_SIGHT: f32 = 14.0;

/// Boots the skirmish and runs the first ticks, because vision is first computed by a tick.
fn skirmish() -> Harness {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    run_ticks(&mut harness, 2);
    harness
}

/// The same map and armies under the developer profile.
fn sandbox_skirmish() -> Harness {
    Harness::boot_scenario(ClientMode::LiveBattle, SKIRMISH_SCENARIO)
}

/// `owner`'s mobile units, ordered by ID.
fn mobile(harness: &Harness, owner: PlayerId) -> Vec<EntityId> {
    harness
        .world()
        .entities
        .values()
        .filter(|unit| unit.owner == owner && !unit.is_immobile)
        .map(|unit| unit.id)
        .collect()
}

fn model(harness: &Harness) -> &HudModel {
    harness.app.world().resource::<HudModel>()
}

fn soldier_type(harness: &Harness) -> UnitTypeId {
    harness
        .world()
        .catalog
        .id_by_slug(SOLDIER)
        .expect("the catalog ships this unit")
}

/// Spawns a soldier of `owner` at `place`, and runs the ticks that show its bubble.
fn spawn_soldier(harness: &mut Harness, owner: PlayerId, place: Vec2) -> EntityId {
    let unit_type = soldier_type(harness);
    let soldier = harness
        .world_mut()
        .spawn_unit(unit_type, owner, sim_position(place))
        .expect("open ground");
    run_ticks(harness, 2);
    soldier
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
}

/// Steps until `done`, one frame at a time, and fails rather than wait forever.
fn run_until(harness: &mut Harness, what: &str, done: impl Fn(&mut Harness) -> bool) {
    harness.resume();
    for _ in 0..WALK_FRAMES {
        if done(harness) {
            harness.pause();
            return;
        }
        harness.step(1);
    }
    panic!("gave up waiting until {what}");
}

/// The bubble entity of unit `id`.
fn bubble(harness: &Harness, id: EntityId) -> Entity {
    *harness
        .app
        .world()
        .resource::<RenderEntityMap>()
        .get(&id)
        .expect("unit must have a bubble")
}

/// Whether Bevy would draw and pick the bubble of unit `id`.
fn drawn(harness: &Harness, id: EntityId) -> bool {
    harness
        .app
        .world()
        .get::<InheritedVisibility>(bubble(harness, id))
        .expect("a bubble has visibility")
        .get()
}

/// Asserts that neither the bubble of `id` nor anything hanging from it would be drawn.
fn assert_nothing_of_it_is_drawn(harness: &Harness, id: EntityId) {
    let world = harness.app.world();
    let mut pending = vec![bubble(harness, id)];
    let mut checked = 0;
    while let Some(entity) = pending.pop() {
        checked += 1;
        assert!(
            !world.get::<InheritedVisibility>(entity).unwrap().get(),
            "entity {checked} of {id:?} would be drawn"
        );
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter().copied());
        }
    }
    assert!(checked > 1, "a bubble carries its ring and bars");
}

/// Commands waiting in the queue, while the battle is paused.
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

/// Queues `order` for `recipients` of `player`, as that player's own interface would.
fn submit(harness: &mut Harness, player: PlayerId, recipients: &[EntityId], order: UnitOrder) {
    let mut session = harness.app.world_mut().resource_mut::<BattleSession>();
    let (world, queue) = session.world_and_commands();
    queue
        .submit_selection(
            world.expect("committed battle"),
            player,
            recipients,
            order,
            false,
        )
        .expect("the client must admit this order");
}

/// Map position of the local player's first worker, the middle of home.
fn home(harness: &Harness) -> Vec2 {
    let worker = mobile(harness, LOCAL_PLAYER)[0];
    harness.simulated_position(worker)
}

// --- Bubbles (G-20, G-21) ------------------------------------------------------------------------

#[test]
fn an_enemy_outside_vision_is_hidden_and_cannot_be_picked() {
    let mut harness = skirmish();
    let foes = mobile(&harness, ENEMY);
    assert!(!foes.is_empty(), "the scenario gives the enemy workers");
    for foe in &foes {
        assert_nothing_of_it_is_drawn(&harness, *foe);
    }
    for own in mobile(&harness, LOCAL_PLAYER) {
        assert!(drawn(&harness, own), "the player sees their own units");
    }

    let foe = foes[0];
    let at = harness.simulated_position(foe);
    harness.look_at(at);
    harness.select(foe);
    assert_eq!(harness.selection().inspected(), None);
    assert_eq!(model(&harness).focus, None);
}

#[test]
fn an_enemy_structure_outside_vision_is_hidden_too() {
    let harness = skirmish();
    let world = harness.world();
    let base = world
        .entities
        .values()
        .find(|unit| unit.owner == ENEMY && unit.is_immobile)
        .expect("the scenario gives each player a base")
        .id;
    assert_nothing_of_it_is_drawn(&harness, base);
}

#[test]
fn an_enemy_born_in_the_fog_is_hidden_from_its_first_frame() {
    let mut harness = skirmish();
    let foe = mobile(&harness, ENEMY)[0];
    let place = harness.open_ground(harness.simulated_position(foe), 4.0);
    let unit_type = soldier_type(&harness);
    let born = harness
        .world_mut()
        .spawn_unit(unit_type, ENEMY, sim_position(place))
        .expect("open ground");
    run_until(&mut harness, "the new unit has a bubble", |harness| {
        harness
            .app
            .world()
            .resource::<RenderEntityMap>()
            .contains_key(&born)
    });
    assert_eq!(
        *harness
            .app
            .world()
            .get::<Visibility>(bubble(&harness, born))
            .unwrap(),
        Visibility::Hidden,
        "a frame with this bubble shown would flash the enemy"
    );
}

#[test]
fn an_enemy_in_vision_appears_and_can_be_attacked() {
    let mut harness = skirmish();
    let foe = mobile(&harness, ENEMY)[0];
    let at = harness.simulated_position(foe);
    let start = harness.open_ground(at, OUT_OF_SIGHT);
    let soldier = spawn_soldier(&mut harness, LOCAL_PLAYER, start);
    assert!(!drawn(&harness, foe), "the walk starts out of sight");

    // Walk there by input.
    harness.look_at((start + at) * 0.5);
    harness.select(soldier);
    assert_eq!(harness.selection().ids(), [soldier]);
    let ground = harness.ground_on_screen(at.lerp(start, 0.3));
    harness.click(MouseButton::Right, ground);
    run_until(&mut harness, "the enemy comes into view", |harness| {
        drawn(harness, foe)
    });

    let at = harness.simulated_position(foe);
    harness.look_at(at);
    let position = harness.unit_on_screen(foe);
    harness.click(MouseButton::Right, position);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(
        orders[0].order,
        UnitOrder::AttackTarget {
            target: foe,
            origin: AttackOrigin::Explicit,
        }
    );

    let health = harness.world().entities[&foe].hp;
    run_until(&mut harness, "the soldier hits", |harness| {
        harness
            .world()
            .entities
            .get(&foe)
            .is_none_or(|unit| unit.hp < health)
    });
}

#[test]
fn a_right_click_on_a_hidden_enemy_moves_there() {
    let mut harness = skirmish();
    let own = mobile(&harness, LOCAL_PLAYER)[0];
    let foe = mobile(&harness, ENEMY)[0];
    harness.look_at(home(&harness));
    harness.select(own);
    assert_eq!(harness.selection().ids(), [own]);

    let at = harness.simulated_position(foe);
    harness.look_at(at);
    let position = harness.unit_on_screen(foe);
    harness.click(MouseButton::Right, position);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(
        matches!(orders[0].order, UnitOrder::Move { .. }),
        "a click on fog is a click on the ground: {:?}",
        orders[0].order
    );
}

#[test]
fn own_units_stay_drawn_wherever_they_walk() {
    let mut harness = skirmish();
    let soldier = spawn_soldier(&mut harness, LOCAL_PLAYER, Vec2::new(32.0, 20.0));
    for _ in 0..4 {
        run_ticks(&mut harness, 5);
        assert!(drawn(&harness, soldier));
    }
}

#[test]
fn a_hidden_enemy_stirs_no_smog() {
    let mut harness = skirmish();
    let foe = mobile(&harness, ENEMY)[0];
    let at = harness.simulated_position(foe);
    harness.look_at(at);
    let away = harness.open_ground(at, 6.0);
    submit(
        &mut harness,
        ENEMY,
        &[foe],
        UnitOrder::Move {
            target: sim_position(away),
        },
    );
    run_ticks(&mut harness, 10);
    assert_eq!(
        harness.app.world().resource::<SmogStats>().active_bubbles,
        0,
        "only enemy workers stand under this camera, and none may stir the fog volume"
    );

    // Control: the developer view of the same place does stir it.
    let mut sandbox = sandbox_skirmish();
    let foe = mobile(&sandbox, ENEMY)[0];
    let at = sandbox.simulated_position(foe);
    sandbox.look_at(at);
    run_ticks(&mut sandbox, 10);
    assert!(sandbox.app.world().resource::<SmogStats>().active_bubbles > 0);
}

// --- Selection and focus ---------------------------------------------------------------------------

/// An enemy soldier beside the local army, inspected by a click, and where it stands.
fn inspect_a_foe(harness: &mut Harness) -> (EntityId, Vec2) {
    let place = harness.open_ground(home(harness), 4.0);
    let foe = spawn_soldier(harness, ENEMY, place);
    harness.look_at(place);
    assert!(drawn(harness, foe), "the enemy stands beside the workers");
    harness.select(foe);
    assert_eq!(harness.selection().inspected(), Some(foe));
    assert!(model(harness).focus.is_some(), "the panel shows the enemy");
    // Armed workers would chase it, and their own sight would keep it on screen until it died.
    let own = mobile(harness, LOCAL_PLAYER);
    submit(harness, LOCAL_PLAYER, &own, UnitOrder::HoldPosition);
    (foe, place)
}

/// Sends `foe` to `target`, and asserts on every frame that hidden means gone from the panel.
fn walk_foe_until(harness: &mut Harness, foe: EntityId, target: Vec2, wanted_in_view: bool) {
    submit(
        harness,
        ENEMY,
        &[foe],
        UnitOrder::Move {
            target: sim_position(target),
        },
    );
    harness.resume();
    for _ in 0..WALK_FRAMES {
        harness.step(1);
        if !drawn(harness, foe) {
            assert_eq!(harness.selection().inspected(), None);
            assert_eq!(model(harness).focus, None, "the panel kept a hidden enemy");
            assert!(!harness.selection().contains(foe));
        }
        if drawn(harness, foe) == wanted_in_view {
            harness.pause();
            return;
        }
    }
    panic!("the enemy never reached the wanted side of the fog");
}

#[test]
fn an_inspected_enemy_that_leaves_vision_leaves_the_panel() {
    let mut harness = skirmish();
    let (foe, _) = inspect_a_foe(&mut harness);
    let enemy_base = harness.simulated_position(mobile(&harness, ENEMY)[0]);
    walk_foe_until(&mut harness, foe, enemy_base, false);
    assert_eq!(harness.selection().inspected(), None);
    assert_eq!(model(&harness).focus, None);
    assert_eq!(model(&harness).selection_groups().len(), 0);
}

#[test]
fn an_enemy_that_returns_to_vision_is_not_inspected_again() {
    let mut harness = skirmish();
    let (foe, place) = inspect_a_foe(&mut harness);
    let enemy_base = harness.simulated_position(mobile(&harness, ENEMY)[0]);
    walk_foe_until(&mut harness, foe, enemy_base, false);
    walk_foe_until(&mut harness, foe, place, true);
    assert_eq!(
        harness.selection().inspected(),
        None,
        "following a unit back out of the fog would reveal it was being tracked"
    );
    assert_eq!(model(&harness).focus, None);
}

#[test]
fn an_enemy_in_sight_stays_inspected() {
    let mut harness = skirmish();
    let (foe, _) = inspect_a_foe(&mut harness);
    run_ticks(&mut harness, 25);
    assert_eq!(harness.selection().inspected(), Some(foe));
    assert!(model(&harness).focus.is_some());
}

// --- No allocation on the per-frame path (charter 3.3) -------------------------------------------

#[test]
fn the_fog_rewrites_only_on_a_tick_and_reuses_its_buffers() {
    let mut harness = skirmish();
    let fog = |harness: &Harness| {
        let fog = harness.app.world().resource::<BattleFog>();
        (fog.buffers(), fog.refills())
    };
    run_ticks(&mut harness, 5);
    let (warmed, refills) = fog(&harness);

    harness.step(100);
    assert_eq!(
        fog(&harness),
        (warmed, refills),
        "a paused frame writes nothing"
    );

    run_ticks(&mut harness, 100);
    let (buffers, ticking) = fog(&harness);
    assert_eq!(buffers, warmed, "a tick reuses every buffer");
    assert_eq!(ticking, refills + 100, "one refill for each tick");
}
