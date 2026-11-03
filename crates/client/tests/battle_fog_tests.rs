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
use client::battle_hud::card::CommandCard;
use client::battle_hud::model::HudModel;
use client::battle_hud::result::ResultScreen;
use client::battle_hud::HudGeometry;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_presentation::{BattlePresentation, PlayerPresentation};
use client::runtime_battle::{MapObject, MapPresentation, SKIRMISH_SCENARIO};
use client::scene_controller::RenderEntityMap;
use client::smog::SmogStats;
use client::vfx::{
    BeamVisualComponent, CombatVisualEvent, MeleeSlashComponent, ProjectileVisual, ShatterShard,
};
use client::{ClientMode, LabState};
use sim_types::{
    AttackOrigin, CombatEventVisual, EntityId, PlayerCommand, PlayerId, ResourceKind,
    ResourceNodeId, SimScalar, UnitOrder, UnitTypeId,
};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{sim_position, Harness, LOCAL_PLAYER};

const ENEMY: PlayerId = PlayerId(1);

/// A slot the skirmish does not seat, for fights the viewer takes no part in.
const THIRD_PARTY: PlayerId = PlayerId(2);

/// A soldier with shipped media and a weapon.
const SOLDIER: &str = "pulse_bot";

/// Frames allowed for any walk here, at one tick a frame.
const WALK_FRAMES: usize = 3_000;

/// More than a soldier's sight, in world units, so its spawn shows nothing of the target.
const OUT_OF_SIGHT: f32 = 14.0;

/// Nodes closer together than this, in world units, are one mineral line.
const MINERAL_LINE_REACH: f32 = 4.0;

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
    let card = harness.app.world().resource::<CommandCard>();
    assert!(
        card.slots.iter().chain(&card.fixed).all(Option::is_none),
        "the card offers nothing for an enemy the fog took away"
    );
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

// --- Combat events (G-22) -------------------------------------------------------------------------

/// What reached the visual layer, recorded on every frame.
#[derive(Resource, Default)]
struct Delivered {
    events: Vec<CombatEventVisual>,
    /// Largest number of live effect entities seen on any frame.
    peak_effects: usize,
}

type EffectFilter = Or<(
    With<ProjectileVisual>,
    With<BeamVisualComponent>,
    With<MeleeSlashComponent>,
    With<ShatterShard>,
)>;

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn record_delivery(
    mut events: EventReader<'_, '_, CombatVisualEvent>,
    effects: Query<'_, '_, (), EffectFilter>,
    mut delivered: ResMut<'_, Delivered>,
) {
    delivered
        .events
        .extend(events.read().map(|event| event.0.clone()));
    delivered.peak_effects = delivered.peak_effects.max(effects.iter().count());
}

fn recording(mut harness: Harness) -> Harness {
    harness
        .app
        .init_resource::<Delivered>()
        .add_systems(PostUpdate, record_delivery);
    harness
}

fn delivered(harness: &Harness) -> &Delivered {
    harness.app.world().resource::<Delivered>()
}

/// Has an enemy soldier kill a soldier of a third player at `place`, by the real weapon path.
///
/// Neither party is the viewer, so the viewer's own units cannot lend the fight any sight. The
/// skirmish seats two players, so setup seats the third in the session's metadata.
fn fight_at(harness: &mut Harness, place: Vec2) {
    let mut session = harness.app.world_mut().resource_mut::<BattleSession>();
    if session.metadata.players.len() == 2 {
        let mut third = session.metadata.players[1].clone();
        third.owner = THIRD_PARTY;
        session.metadata.players.push(third.clone());
        let mut presentation = harness.app.world_mut().resource_mut::<BattlePresentation>();
        presentation.players.push(PlayerPresentation {
            owner: THIRD_PARTY,
            label: third.label,
            color: third.color,
            roster: Vec::new(),
            mobile: 0,
            armed: 0,
            overclock: 0,
        });
    }
    let beside = harness.open_ground(place, 2.0);
    let shooter = spawn_soldier(harness, ENEMY, place);
    let victim = spawn_soldier(harness, THIRD_PARTY, beside);
    let unit = harness.world_mut().entities.get_mut(&victim).unwrap();
    unit.hp = SimScalar::from_i32(1);
    submit(
        harness,
        ENEMY,
        &[shooter],
        UnitOrder::AttackTarget {
            target: victim,
            origin: AttackOrigin::Explicit,
        },
    );
    run_until(harness, "the victim dies", |harness| {
        !harness.world().entities.contains_key(&victim)
    });
    run_ticks(harness, 3);
}

#[test]
fn combat_out_of_sight_sends_no_visual_events_and_the_queue_still_drains() {
    let mut harness = recording(skirmish());
    let foe = mobile(&harness, ENEMY)[0];
    let far = harness.open_ground(harness.simulated_position(foe), 5.0);
    fight_at(&mut harness, far);

    assert_eq!(delivered(&harness).events, []);
    assert_eq!(delivered(&harness).peak_effects, 0, "tracers and shatters");
    let session = harness.app.world().resource::<BattleSession>();
    assert_eq!(session.frames.pending_event_count(), 0);
}

#[test]
fn combat_in_sight_is_delivered() {
    let mut harness = recording(skirmish());
    let place = harness.open_ground(home(&harness), 4.0);
    fight_at(&mut harness, place);

    let seen = delivered(&harness);
    assert!(
        seen.events
            .iter()
            .any(|event| matches!(event, CombatEventVisual::BubbleShatter { .. })),
        "{:?}",
        seen.events
    );
    assert!(seen.peak_effects > 0, "the effects must have been spawned");
}

/// Publishes `events` through the world's real queue and returns what was delivered of them.
fn deliver(harness: &mut Harness, events: &[CombatEventVisual]) -> Vec<CombatEventVisual> {
    harness.world_mut().pending_events.extend_from_slice(events);
    run_ticks(harness, 3);
    let session = harness.app.world().resource::<BattleSession>();
    assert_eq!(session.frames.pending_event_count(), 0, "the queue drains");
    harness
        .app
        .world()
        .resource::<Delivered>()
        .events
        .iter()
        .filter(|event| events.contains(event))
        .cloned()
        .collect()
}

#[test]
fn each_kind_of_event_is_judged_where_it_lands() {
    let mut harness = recording(skirmish());
    let seen = home(&harness);
    let fog = harness.simulated_position(mobile(&harness, ENEMY)[0]);
    let foe_tracer = |from: Vec2, to: Vec2| CombatEventVisual::HitscanTracer {
        from: [from.x, from.y, 0.5],
        to: [to.x, to.y, 0.5],
        owner: ENEMY,
    };
    let foe_beam = |to: Vec2| CombatEventVisual::BeamVisual {
        from: [fog.x, fog.y, 0.5],
        to: [to.x, to.y, 0.5],
        is_lightning: false,
        owner: ENEMY,
    };
    let foe_shatter = |at: Vec2| CombatEventVisual::BubbleShatter {
        pos: [at.x, at.y, 0.5],
        radius: 0.5,
        is_dome: false,
        owner: ENEMY,
    };
    let foe_slash = |at: Vec2| CombatEventVisual::MeleeSlashVisual {
        pos: [at.x, at.y, 0.5],
        angle_rad: 0.0,
        radius: 0.5,
        owner: ENEMY,
    };
    let foe_shot = |from: Vec2, to: Vec2| CombatEventVisual::ProjectileVisual {
        pos: [from.x, from.y, 0.5],
        vel: [(to.x - from.x) * 2.0, (to.y - from.y) * 2.0, 0.0],
        owner: ENEMY,
        duration_secs: Some(0.5),
    };
    let own_tracer = CombatEventVisual::HitscanTracer {
        from: [seen.x, seen.y, 0.5],
        to: [fog.x, fog.y, 0.5],
        owner: LOCAL_PLAYER,
    };
    let landing_in_sight = vec![
        foe_tracer(fog, seen),
        foe_beam(seen),
        foe_shatter(seen),
        foe_slash(seen),
        foe_shot(fog, seen),
        own_tracer,
    ];
    let landing_in_fog = vec![
        foe_tracer(seen, fog),
        foe_beam(fog),
        foe_shatter(fog),
        foe_slash(fog),
        foe_shot(seen, fog),
    ];
    let mut all = landing_in_fog;
    all.extend_from_slice(&landing_in_sight);
    assert_eq!(deliver(&mut harness, &all), landing_in_sight);
}

// --- Resource nodes (G-23) -------------------------------------------------------------------------

/// The mineral nearest home that the player cannot see, and every unseen mineral beside it.
///
/// Billboards of one mineral line overlap on screen, so a click cannot tell them apart, and
/// the tests exhaust a whole line. The first entry is the nearest.
fn unseen_mineral_line(harness: &Harness) -> Vec<ResourceNodeId> {
    let world = harness.world();
    let view = &model(harness).view;
    let origin = home(harness);
    let position =
        |node: &sim_map::ResourceNode| Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32());
    let unseen: Vec<(ResourceNodeId, Vec2)> = (0_u32..)
        .zip(world.map.resources())
        .filter(|(index, node)| {
            node.kind == ResourceKind::Mineral
                && view.visible_nodes.iter().all(|seen| seen.id.0 != *index)
        })
        .map(|(index, node)| (ResourceNodeId(index), position(node)))
        .collect();
    let (nearest, at) = *unseen
        .iter()
        .min_by(|a, b| a.1.distance(origin).total_cmp(&b.1.distance(origin)))
        .expect("the map has minerals beyond the start");
    let mut line = vec![nearest];
    line.extend(
        unseen
            .iter()
            .filter(|(id, position)| *id != nearest && position.distance(at) < MINERAL_LINE_REACH)
            .map(|(id, _)| *id),
    );
    line
}

/// Whether Bevy would draw the billboard of `node`.
fn billboard_drawn(harness: &mut Harness, node: ResourceNodeId) -> bool {
    let world = harness.app.world_mut();
    world
        .query::<(&MapObject, &InheritedVisibility)>()
        .iter(world)
        .find(|(object, _)| object.index() == node.0 as usize)
        .expect("every node has a billboard")
        .1
        .get()
}

fn billboard_position(harness: &Harness, node: ResourceNodeId) -> Vec3 {
    harness.app.world().resource::<MapPresentation>().objects[node.0 as usize].position
}

fn exhaust(harness: &mut Harness, nodes: &[ResourceNodeId]) {
    for node in nodes {
        harness.world_mut().map.resources_mut()[node.0 as usize].remaining = 0;
    }
    run_ticks(harness, 2);
}

/// Brings sight over `node` by standing a soldier beside it.
fn watch(harness: &mut Harness, node: ResourceNodeId) {
    let position = billboard_position(harness, node);
    let place = harness.open_ground(Vec2::new(position.x, position.z), 4.0);
    spawn_soldier(harness, LOCAL_PLAYER, place);
    run_ticks(harness, 2);
}

#[test]
fn a_node_exhausted_out_of_sight_stays_on_screen_until_seen() {
    let mut harness = skirmish();
    let line = unseen_mineral_line(&harness);
    exhaust(&mut harness, &line);
    assert!(
        billboard_drawn(&mut harness, line[0]),
        "the far node must look as it was last seen"
    );

    watch(&mut harness, line[0]);
    assert!(
        !billboard_drawn(&mut harness, line[0]),
        "now it has been seen"
    );
}

#[test]
fn a_hidden_exhausted_node_stays_pickable_and_the_worker_finds_out_on_arrival() {
    let mut harness = skirmish();
    let worker = mobile(&harness, LOCAL_PLAYER)[0];
    let line = unseen_mineral_line(&harness);
    let node = line[0];
    exhaust(&mut harness, &line);

    harness.look_at(home(&harness));
    harness.select(worker);
    assert_eq!(harness.selection().ids(), [worker]);
    let billboard = billboard_position(&harness, node);
    let node_at = Vec2::new(billboard.x, billboard.z);
    let start = harness.simulated_position(worker).distance(node_at);
    harness.look_at(node_at);
    let on_screen = harness.screen_position(billboard);
    harness.click(MouseButton::Right, on_screen);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    let UnitOrder::Harvest { target } = orders[0].order else {
        panic!(
            "the billboard on screen must take the order: {:?}",
            orders[0].order
        );
    };
    assert!(line.contains(&target), "{target:?} is not in {line:?}");

    // The admission of WP1 lets the order stand, so the worker goes and looks.
    run_ticks(&mut harness, 2);
    assert_eq!(
        harness.world().entities[&worker].order_queue.front(),
        Some(&UnitOrder::Harvest { target }),
        "the worker must not be redirected before it has seen the node"
    );
    assert!(billboard_drawn(&mut harness, target));
    run_until(&mut harness, "the worker sees the node", |harness| {
        !billboard_drawn(harness, target)
    });
    let walked_to = harness.simulated_position(worker).distance(node_at);
    assert!(
        walked_to < start - 4.0,
        "the worker walked from {start} to {walked_to}, and only then was the node known"
    );

    // A node seen empty offers nothing, whatever stands around it.
    run_until(&mut harness, "the whole line is seen", |harness| {
        line.iter().all(|node| !billboard_drawn(harness, *node))
    });
    harness.look_at(node_at);
    let at_ground = harness.screen_position(billboard);
    harness.click(MouseButton::Right, at_ground);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(
        matches!(orders[0].order, UnitOrder::Move { .. }),
        "{:?}",
        orders[0].order
    );
}

#[test]
fn a_battle_reset_forgets_what_the_last_one_saw() {
    let mut harness = skirmish();
    let line = unseen_mineral_line(&harness);
    exhaust(&mut harness, &line);
    watch(&mut harness, line[0]);
    assert!(!billboard_drawn(&mut harness, line[0]));

    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .battle_reset_requested = true;
    harness.step(4);
    assert!(
        billboard_drawn(&mut harness, line[0]),
        "a new map, a new node"
    );
    // A new match starts unpaused (Phase 5 addendum A4), so its clock has run since the commit,
    // at most one tick for each of the four frames.
    assert!(harness.world().current_tick <= 4, "a new match's clock");
}

// --- The end of a match ------------------------------------------------------------------------

/// Destroys every structure of `owner`, which ends the match, and waits for the verdict.
fn lose(harness: &mut Harness) {
    for unit in harness.world_mut().entities.values_mut() {
        if unit.owner == LOCAL_PLAYER && unit.is_immobile {
            unit.hp = SimScalar::ZERO;
        }
    }
    run_until(harness, "the verdict shows", |harness| {
        harness
            .app
            .world()
            .resource::<ResultScreen>()
            .verdict()
            .is_some()
    });
    harness.step(2);
}

#[test]
fn a_defeat_shows_no_enemy() {
    let mut harness = skirmish();
    lose(&mut harness);
    for foe in mobile(&harness, ENEMY) {
        assert_nothing_of_it_is_drawn(&harness, foe);
    }
    run_ticks(&mut harness, 25);
    for foe in mobile(&harness, ENEMY) {
        assert_nothing_of_it_is_drawn(&harness, foe);
    }
}

#[test]
fn play_again_starts_with_a_fresh_memory_and_the_enemy_still_hidden() {
    let mut harness = skirmish();
    let line = unseen_mineral_line(&harness);
    exhaust(&mut harness, &line);
    watch(&mut harness, line[0]);
    assert!(!billboard_drawn(&mut harness, line[0]));
    let generation = {
        let session = harness.app.world().resource::<BattleSession>();
        session.commands.generation()
    };

    lose(&mut harness);
    let button = harness
        .app
        .world()
        .resource::<HudGeometry>()
        .play_again
        .expect("the result window offers another match");
    harness.click(
        MouseButton::Left,
        Vec2::new(button.center().x, button.center().y),
    );
    harness.step(2);

    let session = harness.app.world().resource::<BattleSession>();
    assert_ne!(session.commands.generation(), generation, "a new session");
    assert!(
        billboard_drawn(&mut harness, line[0]),
        "a new map, a new node"
    );
    run_ticks(&mut harness, 2);
    assert!(billboard_drawn(&mut harness, line[0]));
    let fog = harness.app.world().resource::<BattleFog>();
    assert!(fog.node_present().iter().all(|present| *present));
    for foe in mobile(&harness, ENEMY) {
        assert_nothing_of_it_is_drawn(&harness, foe);
    }
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

// --- The Sandbox profile must not move -------------------------------------------------------------

#[test]
fn the_sandbox_profile_still_draws_everything() {
    let mut harness = recording(sandbox_skirmish());
    assert!(harness.app.world().get_resource::<BattleFog>().is_none());
    for foe in mobile(&harness, ENEMY) {
        assert!(drawn(&harness, foe), "the developer view shows every unit");
    }

    let foe = mobile(&harness, ENEMY)[0];
    let at = harness.simulated_position(foe);
    harness.look_at(at);
    harness.select(foe);
    assert_eq!(harness.selection().inspected(), Some(foe));

    let far = harness.open_ground(at, 5.0);
    fight_at(&mut harness, far);
    assert!(
        !delivered(&harness).events.is_empty(),
        "combat anywhere is delivered"
    );

    let node = ResourceNodeId(
        harness
            .world()
            .map
            .resources()
            .iter()
            .rposition(|node| node.kind == ResourceKind::Mineral)
            .and_then(|index| u32::try_from(index).ok())
            .expect("the map has minerals"),
    );
    assert!(billboard_drawn(&mut harness, node));
    exhaust(&mut harness, &[node]);
    assert!(
        !billboard_drawn(&mut harness, node),
        "the developer view shows the truth, wherever it is"
    );
}
