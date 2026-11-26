//! The render-entity pool of the live battle, headless (plan `2026-11-29-render-entity-pool`
//! R-3 to R-7 and R-11).
//!
//! # Responsibility
//! Proves through the production client (`support/battle_harness.rs`, no renderer) that a unit
//! arriving after another left is drawn by the departed unit's parked entity exactly as a fresh
//! spawn would draw it (R-3), that nothing picks, selects or presents a parked entity (R-4, R-7),
//! that a departed unit's looks and sustained beam are released and never reach the reused
//! entity's new unit (R-5; the beam is held by a Carrier's interceptor),
//! that a battle beyond the pool's bound still draws every unit and says so (R-6), and that every
//! way a battle ends leaves no bubble behind (R-11).
//!
//! # How it fits
//! The pixel half of R-3 is `render_pool_gpu_tests.rs` (R-8); the steady-state and allocation
//! halves are `sync_steady_state_tests.rs` (R-1, R-2) and `sync_allocation_count_tests.rs`
//! (R-1v); the map's own decisions and the bound are unit-tested in `client::render_pool`.
//!
//! # Invariants verified
//! - Every unit is found by catalog property and media present, never by slug.
//! - A reused bubble is compared with a fresh one field by field, children included, after the
//!   frame on which the stance system first saw it.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::float_cmp
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/reuse_stage.rs"]
mod reuse_stage;

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/unit_cycling.rs"]
mod unit_cycling;

use std::time::Duration;

use battle_harness::{sim_position, Harness, LOCAL_PLAYER};
use bevy::prelude::*;
use bevy::render::batching::NoAutomaticBatching;
use client::battle_pipeline::session::BattleSession;
use client::battle_selection::BattleSelection;
use client::bubble_shader::{
    BubbleDomeMaterial, BubbleInstance, BubbleUniformGpu, ShellUniformGpu,
};
use client::effects::runtime::EffectRuntime;
use client::effects::stance::StanceTrack;
use client::flipbook_clock::FlipbookPhase;
use client::indicators::{
    AirShadowMarker, AirTetherMarker, GroundRingMarker, OverheadBarRootMarker, OverheadHpBarMarker,
    OverheadShieldBarMarker, SelectionRingMaterial, SelectionRingUniformGpu,
};
use client::scene_controller::{LiveBattleBubble, RenderEntityMap};
use client::{ClientMode, LabTab, ParkedBattleBubble};
use reuse_stage::{Arrival, Decoy, Kinds, Script};
use sim_types::snapshot::BubbleRenderInstance;
use sim_types::{EntityId, PlayerCommand, SimVec2, UnitOrder};

/// Frames a battle gets to load its media and present its first snapshot.
const READY_FRAMES: usize = 600;

/// Ticks the decoy fights before it leaves, so it is mid-attack, cloaked and its shell moved.
const DECOY_TICKS: u32 = 30;

// --- Driving ------------------------------------------------------------------------------------

/// Steps until the battle is ready and its first snapshot is presented with its media loaded.
fn ready(harness: &mut Harness) {
    for _ in 0..READY_FRAMES {
        let world = harness.app.world();
        let images = world.resource::<Assets<Image>>();
        let registry = world.resource::<client::FlipbookRegistry>();
        let loaded = !registry.textures_by_id.is_empty()
            && registry
                .textures_by_id
                .values()
                .all(|image| images.contains(image));
        if loaded
            && world.resource::<BattleSession>().is_ready()
            && !world.resource::<RenderEntityMap>().is_empty()
        {
            return;
        }
        harness.step(1);
    }
    panic!("the battle never became ready");
}

/// Steps one frame with the battle running, which runs exactly one tick.
fn tick(harness: &mut Harness) {
    let before = harness.world().current_tick;
    harness.resume();
    harness.step(1);
    assert_eq!(harness.world().current_tick, before + 1, "one tick a frame");
}

/// Runs `ticks` ticks, one a frame.
fn run_ticks(harness: &mut Harness, ticks: u32) {
    for _ in 0..ticks {
        tick(harness);
    }
}

/// The map.
fn map(harness: &Harness) -> &RenderEntityMap {
    harness.app.world().resource::<RenderEntityMap>()
}

/// The render entity of unit `id`.
fn entity_of(harness: &Harness, id: EntityId) -> Entity {
    map(harness).get(id).expect("the unit has a render entity")
}

/// Writes the local player's selection.
fn selection_mut(harness: &mut Harness) -> Mut<'_, BattleSelection> {
    harness.app.world_mut().resource_mut::<BattleSelection>()
}

/// Despawns the pool's parked entities, so the next arrival is spawned fresh.
fn release_parked(harness: &mut Harness) {
    let world = harness.app.world_mut();
    world.resource_scope(|world, mut map: Mut<'_, RenderEntityMap>| {
        let mut commands = world.commands();
        map.release_parked(&mut commands);
    });
    world.flush();
}

/// The map's centre in map units.
fn centre(harness: &Harness) -> Vec2 {
    let at = unit_cycling::map_centre(harness.world());
    Vec2::new(at.x.to_f32(), at.y.to_f32())
}

// --- What a bubble draws -----------------------------------------------------------------------

/// The colour of a standard material.
fn colour(world: &World, handle: &Handle<StandardMaterial>) -> (Color, LinearRgba) {
    let material = world
        .resource::<Assets<StandardMaterial>>()
        .get(handle)
        .expect("a standard material");
    (material.base_color, material.emissive)
}

/// What one indicator child (or grandchild) draws.
#[derive(Debug, PartialEq)]
struct Child {
    /// Its marker, or `"background"` for the bar's unmarked background.
    role: &'static str,
    transform: Transform,
    visibility: Option<Visibility>,
    /// The ring's uniform.
    ring: Option<SelectionRingUniformGpu>,
    /// A standard material's colour.
    colour: Option<(Color, LinearRgba)>,
}

/// The role of indicator entity `entity`.
fn role(world: &World, entity: Entity) -> &'static str {
    let has = |id: std::any::TypeId| {
        world
            .components()
            .get_id(id)
            .is_some_and(|component| world.entity(entity).contains_id(component))
    };
    [
        (std::any::TypeId::of::<GroundRingMarker>(), "ring"),
        (std::any::TypeId::of::<OverheadBarRootMarker>(), "bar"),
        (std::any::TypeId::of::<OverheadHpBarMarker>(), "hp"),
        (std::any::TypeId::of::<OverheadShieldBarMarker>(), "shield"),
        (std::any::TypeId::of::<AirShadowMarker>(), "shadow"),
        (std::any::TypeId::of::<AirTetherMarker>(), "tether"),
    ]
    .into_iter()
    .find(|(id, _)| has(*id))
    .map_or("background", |(_, name)| name)
}

/// What `entity`, an indicator, draws.
fn child(world: &World, entity: Entity) -> Child {
    let ring = world
        .get::<MeshMaterial3d<SelectionRingMaterial>>(entity)
        .map(|handle| {
            world
                .resource::<Assets<SelectionRingMaterial>>()
                .get(&handle.0)
                .expect("a ring material")
                .uniform
        });
    Child {
        role: role(world, entity),
        transform: *world.get::<Transform>(entity).expect("a transform"),
        visibility: world.get::<Visibility>(entity).copied(),
        ring,
        colour: world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .map(|handle| colour(world, &handle.0)),
    }
}

/// What one bubble draws, children included.
#[derive(Debug, PartialEq)]
struct Drawn {
    instance: BubbleRenderInstance,
    transform: Transform,
    mesh: AssetId<Mesh>,
    phase: FlipbookPhase,
    visibility: Visibility,
    track: Option<StanceTrack>,
    unbatched: bool,
    live: Option<LiveBattleBubble>,
    parked: bool,
    uniform: BubbleUniformGpu,
    shell: ShellUniformGpu,
    texture: Option<AssetId<Image>>,
    children: Vec<Child>,
}

/// What `entity` draws.
fn drawn(harness: &Harness, entity: Entity) -> Drawn {
    let world = harness.app.world();
    let material = world
        .get::<MeshMaterial3d<BubbleDomeMaterial>>(entity)
        .expect("a bubble material");
    let material = world
        .resource::<Assets<BubbleDomeMaterial>>()
        .get(&material.0)
        .expect("the bubble material asset");
    let mut children = Vec::new();
    for &first in world.get::<Children>(entity).expect("indicator children") {
        children.push(child(world, first));
        for &second in world.get::<Children>(first).into_iter().flatten() {
            children.push(child(world, second));
        }
    }
    children.sort_by_key(|child| child.role);
    Drawn {
        instance: world
            .get::<BubbleInstance>(entity)
            .expect("a bubble")
            .0
            .clone(),
        transform: *world.get::<Transform>(entity).expect("a transform"),
        mesh: world.get::<Mesh3d>(entity).expect("a mesh").0.id(),
        phase: *world.get::<FlipbookPhase>(entity).expect("a phase"),
        visibility: *world.get::<Visibility>(entity).expect("a visibility"),
        track: world.get::<StanceTrack>(entity).copied(),
        unbatched: world.get::<NoAutomaticBatching>(entity).is_some(),
        live: world.get::<LiveBattleBubble>(entity).copied(),
        parked: world.get::<ParkedBattleBubble>(entity).is_some(),
        uniform: material.uniform,
        shell: material.shell,
        texture: material.video_texture.as_ref().map(Handle::id),
        children,
    }
}

// --- R-3: a reused bubble is a fresh one --------------------------------------------------------

/// Ticks before its arrival a fresh script's decoy dies, so it has left the presented snapshot
/// (which lags the simulation by a tick) and its entity is parked before the release.
const FRESH_LEAD_TICKS: u32 = 3;

/// Frames an arrival may take to be presented.
const ARRIVAL_FRAMES: usize = 8;

/// Steps frames, with the battle running, until unit `arrival` is presented; with `decoy`, also
/// asserts that the decoy is presented until that frame and gone on it (a same-frame departure).
fn step_until_presented(harness: &mut Harness, arrival: EntityId, decoy: Option<EntityId>) {
    harness.resume();
    for _ in 0..ARRIVAL_FRAMES {
        harness.step(1);
        if map(harness).get(arrival).is_some() {
            if let Some(decoy) = decoy {
                assert!(
                    map(harness).get(decoy).is_none(),
                    "the decoy leaves on the arrival's frame"
                );
            }
            return;
        }
        if let Some(decoy) = decoy {
            assert!(
                map(harness).get(decoy).is_some(),
                "the decoy leaves on the arrival's frame, not before"
            );
        }
    }
    panic!("the arrival was never presented");
}

/// Runs `script` on the default battle with every mobile unit killed. With `fresh`, the pool's
/// parked entities are released before the arrival, so it is spawned; a fresh script's decoy dies
/// at least [`FRESH_LEAD_TICKS`] before the arrival, so it is parked and released by then. The
/// arrival is spawned on the same tick in both, [`DECOY_TICKS`] after the decoy. Returns the
/// harness after the frame that follows the arrival's first, the arrival, the decoy's render
/// entity and the pool's counters before the arrival.
fn run_script(script: Script, fresh: bool) -> (Harness, EntityId, Entity, client::RenderPoolStats) {
    let mut harness = Harness::battle();
    ready(&mut harness);
    let assets = battle_harness::assets();
    let kinds = Kinds::find(harness.world_mut(), &assets);
    unit_cycling::kill_mobile_units(harness.world_mut());
    run_ticks(&mut harness, 2);
    let staged = script.stage_decoy(harness.world_mut(), &kinds);
    selection_mut(&mut harness).select_only(staged.decoy);
    let lead = if fresh {
        script.hidden_ticks.max(FRESH_LEAD_TICKS)
    } else {
        script.hidden_ticks
    };
    run_ticks(&mut harness, DECOY_TICKS - lead);
    let decoy_entity = entity_of(&harness, staged.decoy);
    Script::kill_decoy(harness.world_mut(), staged);
    run_ticks(&mut harness, lead);
    if lead > 0 {
        assert!(map(&harness).get(staged.decoy).is_none(), "the decoy left");
    }
    if fresh {
        release_parked(&mut harness);
    }
    let before = map(&harness).stats();
    let arrival = script.arrive(harness.world_mut(), &kinds);
    // The arrival is the enemy's: it is inspected, so it draws selected from its first frame.
    selection_mut(&mut harness).inspect(arrival);
    step_until_presented(&mut harness, arrival, (lead == 0).then_some(staged.decoy));
    // The frame after: the stance system has seen the bubble and its track is in place.
    harness.pause();
    (harness, arrival, decoy_entity, before)
}

/// R-3 for one script: the reused bubble draws exactly what a fresh spawn of the same arrival
/// draws, on every field of the bubble and its children.
fn assert_reuse_is_fresh(script: Script) {
    let (reused, arrival, decoy_entity, before) = run_script(script, false);
    let reused_entity = entity_of(&reused, arrival);
    assert_eq!(
        reused_entity, decoy_entity,
        "{script:?}: the arrival reuses the decoy's entity"
    );
    let stats = map(&reused).stats();
    assert_eq!(
        (stats.reused - before.reused, stats.spawned - before.spawned),
        (1, 0),
        "{script:?}: one reuse, no spawn"
    );
    let (fresh, fresh_arrival, fresh_decoy, fresh_before) = run_script(script, true);
    assert_eq!(
        fresh_arrival, arrival,
        "both harnesses give the arrival the same id"
    );
    let fresh_entity = entity_of(&fresh, fresh_arrival);
    assert_ne!(
        fresh_entity, fresh_decoy,
        "{script:?}: the fresh arrival is spawned"
    );
    assert_eq!(
        map(&fresh).stats().spawned - fresh_before.spawned,
        1,
        "{script:?}: one spawn"
    );
    let reused_drawn = drawn(&reused, reused_entity);
    let fresh_drawn = drawn(&fresh, fresh_entity);
    assert!(reused_drawn.live.is_some() && !reused_drawn.parked);
    assert!(
        reused_drawn.instance.selected,
        "the arrival is drawn selected"
    );
    assert_eq!(
        reused_drawn, fresh_drawn,
        "{script:?}: reused and fresh bubbles differ"
    );
}

/// The default script: decoy and arrival a few map units apart on open ground at the centre.
fn script(decoy: Decoy, arrival: Arrival, hidden_ticks: u32) -> Script {
    Script {
        decoy,
        arrival,
        hidden_ticks,
        decoy_at: SimVec2::from_i32(0, 0),
        arrival_at: SimVec2::from_i32(0, 0),
        decoy_owner: reuse_stage::LOCAL,
        arrival_owner: reuse_stage::ENEMY,
    }
}

/// Places `script` at the centre of the default battle's map.
fn at_centre(mut script: Script) -> Script {
    let harness = Harness::battle();
    let at = unit_cycling::map_centre(harness.world());
    script.decoy_at = at;
    script.arrival_at = at + SimVec2::from_i32(-4, 3);
    script
}

#[test]
fn a_bubble_reused_on_the_frame_its_unit_left_draws_as_a_fresh_one() {
    assert_reuse_is_fresh(at_centre(script(Decoy::Ground, Arrival::Ground, 0)));
}

#[test]
fn a_bubble_reused_after_five_hidden_ticks_draws_as_a_fresh_one() {
    assert_reuse_is_fresh(at_centre(script(Decoy::Ground, Arrival::Ground, 5)));
}

#[test]
fn a_ground_bubble_reused_by_an_air_unit_draws_as_a_fresh_one() {
    assert_reuse_is_fresh(at_centre(script(Decoy::Ground, Arrival::Air, 5)));
}

#[test]
fn an_air_bubble_reused_by_a_ground_unit_draws_as_a_fresh_one() {
    assert_reuse_is_fresh(at_centre(script(Decoy::Air, Arrival::Ground, 0)));
}

#[test]
fn a_destroyed_structures_dome_reused_by_a_unit_draws_as_a_fresh_sphere() {
    assert_reuse_is_fresh(at_centre(script(Decoy::Dome, Arrival::Ground, 5)));
}

// --- R-4: picking and selection never see a parked entity --------------------------------------

/// Commands waiting in the queue.
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

/// Drops every queued command, so the next gesture's order is the only one.
fn drain(harness: &mut Harness) {
    run_ticks(harness, 2);
    harness.pause();
    assert!(pending(harness).is_empty(), "the queue drained");
}

#[test]
fn a_parked_bubble_is_never_picked_or_selected() {
    let mut harness = Harness::battle();
    ready(&mut harness);
    let assets = battle_harness::assets();
    let kinds = Kinds::find(harness.world_mut(), &assets);
    unit_cycling::kill_mobile_units(harness.world_mut());
    run_ticks(&mut harness, 2);
    let at = centre(&harness);
    let world = harness.world_mut();
    let victim = world
        .spawn_unit(kinds.ground[0], LOCAL_PLAYER, sim_position(at))
        .expect("open ground");
    let army = world
        .spawn_unit(
            kinds.ground[1],
            LOCAL_PLAYER,
            sim_position(at + Vec2::new(-6.0, 0.0)),
        )
        .expect("open ground");
    run_ticks(&mut harness, 2);
    harness.pause();
    harness.look_at(at);
    let pointer = harness.select(victim);
    assert_eq!(harness.selection().ids(), [victim]);
    let parked = entity_of(&harness, victim);

    // The selected unit dies with the pointer on its last position.
    harness
        .world_mut()
        .entities
        .get_mut(&victim)
        .expect("the victim")
        .hp = sim_types::SimScalar::ZERO;
    run_ticks(&mut harness, 2);
    harness.pause();
    assert!(map(&harness).get(victim).is_none(), "the unit left");
    assert!(
        harness
            .app
            .world()
            .get::<ParkedBattleBubble>(parked)
            .is_some(),
        "its entity is parked"
    );
    assert!(
        !harness.selection().contains(victim),
        "the selection forgot it"
    );

    // A left click and a box there select nothing.
    harness.click(MouseButton::Left, pointer);
    assert!(
        harness.selection().ids().is_empty(),
        "a click picks nothing"
    );
    harness.drag(pointer - Vec2::splat(30.0), pointer + Vec2::splat(30.0));
    assert!(harness.selection().ids().is_empty(), "a box picks nothing");

    // A right click there with an army selected issues the point order bare ground issues.
    drain(&mut harness);
    harness.select(army);
    assert_eq!(harness.selection().ids(), [army]);
    harness.click(MouseButton::Right, pointer);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "one order: {}", harness.status());
    assert!(
        matches!(orders[0].order, UnitOrder::Move { .. }),
        "a click where a parked bubble stands is a click on the ground: {:?}",
        orders[0].order
    );

    // An arrival elsewhere reuses the entity, and the old position still picks nothing.
    drain(&mut harness);
    let elsewhere = harness
        .world_mut()
        .spawn_unit(
            kinds.ground[0],
            LOCAL_PLAYER,
            sim_position(at + Vec2::new(6.0, 4.0)),
        )
        .expect("open ground");
    run_ticks(&mut harness, 1);
    harness.pause();
    assert_eq!(
        entity_of(&harness, elsewhere),
        parked,
        "the arrival reused the entity"
    );
    selection_mut(&mut harness).clear_all();
    harness.click(MouseButton::Left, pointer);
    assert!(
        harness.selection().ids().is_empty(),
        "the old position picks nothing"
    );
}

// --- R-5: effects released and not reassigned --------------------------------------------------

/// Whether the effect runtime still holds `look` live and unreleased.
fn holds(harness: &Harness, look: client::effects::stance::HeldLook) -> bool {
    harness
        .app
        .world()
        .resource::<EffectRuntime>()
        .held(look.handle)
        .is_some_and(|instance| instance.released_at.is_none())
}

/// Every look `track` holds.
fn looks(track: &StanceTrack) -> Vec<client::effects::stance::HeldLook> {
    let named = track.named.iter().flatten().map(|(_, look)| *look);
    [
        track.stance,
        track.morph.map(|morph| morph.look),
        track.cover,
    ]
    .into_iter()
    .chain(track.status)
    .flatten()
    .chain(named)
    .collect()
}

#[test]
fn a_departed_units_looks_are_released_and_never_reach_the_reused_bubble() {
    let mut harness = Harness::battle();
    ready(&mut harness);
    let assets = battle_harness::assets();
    let kinds = Kinds::find(harness.world_mut(), &assets);
    unit_cycling::kill_mobile_units(harness.world_mut());
    run_ticks(&mut harness, 2);
    let at = centre(&harness);
    let cloaked = harness
        .world_mut()
        .spawn_unit(kinds.ground[0], LOCAL_PLAYER, sim_position(at))
        .expect("open ground");
    harness
        .world_mut()
        .entities
        .get_mut(&cloaked)
        .expect("the unit")
        .is_cloaked = true;
    run_ticks(&mut harness, 10);
    let entity = entity_of(&harness, cloaked);
    let track = *harness
        .app
        .world()
        .get::<StanceTrack>(entity)
        .expect("a presented bubble has a track");
    let held = looks(&track);
    assert!(!held.is_empty(), "the cloaked unit holds a look");
    assert!(held.iter().all(|look| holds(&harness, *look)));

    // It departs and a plain unit arrives on the same frame, taking its entity.
    harness
        .world_mut()
        .entities
        .get_mut(&cloaked)
        .expect("the cloaked unit")
        .hp = sim_types::SimScalar::ZERO;
    let arrival = harness
        .world_mut()
        .spawn_unit(
            kinds.ground[1],
            LOCAL_PLAYER,
            sim_position(at + Vec2::new(4.0, 0.0)),
        )
        .expect("open ground");
    step_until_presented(&mut harness, arrival, Some(cloaked));
    assert_eq!(
        entity_of(&harness, arrival),
        entity,
        "the arrival reused the entity"
    );
    assert!(
        held.iter().all(|look| !holds(&harness, *look)),
        "the departed unit's looks are released"
    );
    harness.pause();
    let new_track = *harness
        .app
        .world()
        .get::<StanceTrack>(entity)
        .expect("the reused bubble has a new track");
    assert!(
        looks(&new_track).iter().all(|look| !held.contains(look)),
        "the new unit holds none of the departed unit's looks"
    );
    assert!(
        looks(&new_track).is_empty(),
        "a plain unit holds no look: {new_track:?}"
    );
}

/// R-5's beam half: a Carrier's interceptor (a hangar member, whose weapon holds a sustained
/// beam) that holds a beam departs, and a plain unit arrives on the same tick: the beam ends, and
/// no beam names the arrival, which is drawn by a parked entity.
#[test]
fn a_departed_members_beam_ends_and_never_follows_the_reused_bubble() {
    let mut harness = Harness::battle();
    ready(&mut harness);
    let assets = battle_harness::assets();
    let kinds = Kinds::find(harness.world_mut(), &assets);
    let cycle = unit_cycling::stage_hangars(harness.world_mut(), &assets);
    let member_kind = cycle.member;
    let beams = |harness: &Harness| -> Vec<EntityId> {
        let world = harness.world();
        harness
            .app
            .world()
            .resource::<client::effects::beams::HeldBeams>()
            .slots()
            .iter()
            .map(|slot| slot.attacker)
            .filter(|attacker| {
                world
                    .entities
                    .get(attacker)
                    .is_some_and(|unit| unit.unit_type_id == member_kind)
            })
            .collect()
    };
    let mut beaming = Vec::new();
    for _ in 0..1_000 {
        tick(&mut harness);
        beaming = beams(&harness);
        if !beaming.is_empty() {
            break;
        }
    }
    let member = *beaming.first().expect("an interceptor holds a beam");
    let reused_before = map(&harness).stats().reused;
    harness
        .world_mut()
        .entities
        .get_mut(&member)
        .expect("the member")
        .hp = sim_types::SimScalar::ZERO;
    let home = harness.simulated_position(harness.local_structure());
    let place = harness.open_ground(home, 8.0);
    let arrival = harness
        .world_mut()
        .spawn_unit(kinds.ground[1], LOCAL_PLAYER, sim_position(place))
        .expect("open ground");
    step_until_presented(&mut harness, arrival, Some(member));
    run_ticks(&mut harness, 5);
    let held = harness
        .app
        .world()
        .resource::<client::effects::beams::HeldBeams>()
        .slots()
        .iter()
        .map(|slot| slot.attacker)
        .collect::<Vec<_>>();
    assert!(!held.contains(&member), "the departed member's beam ended");
    assert!(!held.contains(&arrival), "no beam names the arrival");
    assert!(
        map(&harness).stats().reused > reused_before,
        "the arrival was drawn by a parked entity"
    );
}

// --- R-7: consumers leave a parked entity alone -----------------------------------------------

#[test]
fn every_consumer_leaves_a_parked_bubble_alone() {
    let mut harness = Harness::boot_scenario(
        ClientMode::Skirmish,
        client::runtime_battle::SKIRMISH_SCENARIO,
    );
    ready(&mut harness);
    let assets = battle_harness::assets();
    let kinds = Kinds::find(harness.world_mut(), &assets);
    let home = harness.simulated_position(harness.local_structure());
    let place = harness.open_ground(home, 8.0);
    let victim = harness
        .world_mut()
        .spawn_unit(kinds.ground[0], LOCAL_PLAYER, sim_position(place))
        .expect("open ground");
    run_ticks(&mut harness, 3);
    selection_mut(&mut harness).select_only(victim);
    run_ticks(&mut harness, 2);
    let parked = entity_of(&harness, victim);
    assert!(
        harness
            .app
            .world()
            .resource::<client::LabState>()
            .opt_smog_volume,
        "the smog runs"
    );
    let sources_alive = harness
        .app
        .world()
        .resource::<client::SmogStats>()
        .active_bubbles;
    harness
        .world_mut()
        .entities
        .get_mut(&victim)
        .expect("the victim")
        .hp = sim_types::SimScalar::ZERO;
    run_ticks(&mut harness, 2);
    assert!(
        harness
            .app
            .world()
            .get::<ParkedBattleBubble>(parked)
            .is_some(),
        "the entity is parked"
    );
    let before = drawn(&harness, parked);
    run_ticks(&mut harness, 20);
    let after = drawn(&harness, parked);
    // The fog (which writes every live bubble's visibility) left it hidden, and the indicators
    // (which write the ring, bars and tether of every bubble they see) changed none of its
    // children.
    assert_eq!(
        after.visibility,
        Visibility::Hidden,
        "the fog left it hidden"
    );
    assert_eq!(
        after, before,
        "nothing wrote the parked bubble or its children"
    );
    // Hidden, it is neither extracted nor drawn, and the capture verifier, which skips bubbles
    // whose view visibility is false, skips it; so does every child.
    let world = harness.app.world();
    let mut pending = vec![parked];
    while let Some(entity) = pending.pop() {
        assert!(
            !world
                .get::<InheritedVisibility>(entity)
                .expect("a visibility")
                .get(),
            "a parked entity or child would be drawn"
        );
        pending.extend(world.get::<Children>(entity).into_iter().flatten().copied());
    }
    // The smog takes no source from it: one source fewer than while the unit lived.
    assert_eq!(
        world.resource::<client::SmogStats>().active_bubbles + 1,
        sources_alive,
        "the smog takes the parked bubble as a source"
    );
}

// --- R-6: beyond the bound -----------------------------------------------------------------------

#[test]
fn a_battle_beyond_its_bound_draws_every_unit_and_says_so_once(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = client::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = client::runtime_battle::BattleDefinition::load(
        root.as_path(),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;
    let mut simulation = definition.instantiate()?;
    // Every trained unit and every host costs the whole cap: A = 1, T = the most members one
    // host keeps.
    let cap = sim_core::production::STANDARD_SUPPLY_CAP;
    let trained: Vec<_> = simulation
        .catalog
        .archetypes()
        .flat_map(|a| a.trains_units.iter().cloned())
        .collect();
    let hosts: Vec<_> = simulation
        .catalog
        .archetypes()
        .filter(|a| a.capabilities.hangar.is_some() || a.capabilities.strike_magazine.is_some())
        .map(|a| a.unit_type_id)
        .collect();
    for slug in trained {
        if let Some(id) = simulation.catalog.id_by_slug(&slug) {
            simulation.catalog.get_mut(id)?.supply_cost = cap;
        }
    }
    for id in hosts {
        simulation.catalog.get_mut(id)?.supply_cost = cap;
    }
    let assets = battle_harness::assets();
    let kinds = Kinds::find(&mut simulation, &assets);
    let session = BattleSession::prepare(
        client::battle_pipeline::commands::SessionGeneration::FIRST,
        &mut simulation,
        client::battle_pipeline::session::CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: root.as_path().to_string_lossy().into_owned(),
            ..default()
        },
    ))
    .add_event::<AppExit>()
    .add_event::<client::CombatVisualEvent>()
    .init_asset::<Mesh>()
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    .init_asset::<BubbleDomeMaterial>()
    .init_asset::<SelectionRingMaterial>()
    .init_resource::<RenderEntityMap>()
    .init_resource::<BattleSelection>()
    .insert_resource(definition.palette.clone())
    .init_resource::<client::player_presentation::PresentationStatus>()
    .insert_resource(root)
    .insert_resource(client::CatalogResource(definition.catalogs.units.clone()))
    .insert_resource(session.with_world(simulation))
    .insert_resource(client::LabState {
        current_tab: LabTab::LiveBattle,
        battle_paused: false,
        simulation_speed: 1,
        ..default()
    })
    .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(40),
    ))
    .add_systems(
        Startup,
        (
            client::setup_bubble_assets_system,
            client::setup_indicator_assets_system,
        ),
    )
    .add_systems(
        Update,
        (
            client::battle_pipeline::driver::advance_battle_ticks,
            client::sync_live_battle_system,
            client::battle_pipeline::driver::publish_combat_events,
        )
            .chain(),
    );
    for _ in 0..4 {
        app.update();
    }
    let bound = app
        .world()
        .resource::<RenderEntityMap>()
        .bound()
        .ok_or("the session is bound")?;
    assert_eq!(bound.army, 1, "the cheapest trained unit costs the cap");
    let held = |app: &App| {
        let map = app.world().resource::<RenderEntityMap>();
        map.len() + map.parked().len()
    };
    let extra = bound.total() - held(&app) + 3;
    let centre = {
        let session = app.world().resource::<BattleSession>();
        unit_cycling::map_centre(session.world().ok_or("a world")?)
    };
    let mut spawned = 0;
    'spawn: for row in 0..40 {
        for column in 0..40 {
            if spawned == extra {
                break 'spawn;
            }
            let mut session = app.world_mut().resource_mut::<BattleSession>();
            let world = session.world_mut().ok_or("a world")?;
            let at = centre + SimVec2::from_i32(column - 20, row - 20);
            if world.spawn_unit(kinds.dummy, LOCAL_PLAYER, at).is_ok() {
                spawned += 1;
            }
            if spawned % 8 == 0 {
                app.update();
            }
        }
    }
    assert_eq!(spawned, extra, "room for every extra unit");
    for _ in 0..4 {
        app.update();
    }
    if let Some(failure) = &app.world().resource::<client::LabState>().battle_failure {
        return Err(failure.clone().into());
    }
    let map = app.world().resource::<RenderEntityMap>();
    let snapshot = &app
        .world()
        .resource::<BattleSession>()
        .frames
        .presented()
        .bubbles;
    assert_eq!(map.len(), snapshot.len(), "every record is drawn");
    for record in snapshot {
        let entity = map.get(record.entity_id).ok_or("a drawn record")?;
        assert_ne!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Hidden),
            "a drawn record is visible"
        );
    }
    assert_eq!(
        map.stats().beyond_bound,
        3,
        "{:?} over {bound:?}",
        map.stats()
    );
    assert!(map.has_warned(), "the warning was given");
    Ok(())
}

// --- R-11: every way a battle ends leaves no bubble -------------------------------------------

/// Live and parked bubble entities in the app.
fn bubbles_left(harness: &mut Harness) -> (usize, usize) {
    let world = harness.app.world_mut();
    let live = world
        .query_filtered::<Entity, With<LiveBattleBubble>>()
        .iter(world)
        .count();
    let parked = world
        .query_filtered::<Entity, With<ParkedBattleBubble>>()
        .iter(world)
        .count();
    (live, parked)
}

/// A battle with a parked entity: a unit spawned and killed.
fn battle_with_a_parked_bubble() -> Harness {
    let mut harness = Harness::battle();
    ready(&mut harness);
    let assets = battle_harness::assets();
    let kinds = Kinds::find(harness.world_mut(), &assets);
    let at = centre(&harness);
    let id = harness
        .world_mut()
        .spawn_unit(kinds.ground[0], LOCAL_PLAYER, sim_position(at))
        .expect("open ground");
    run_ticks(&mut harness, 2);
    harness
        .world_mut()
        .entities
        .get_mut(&id)
        .expect("the unit")
        .hp = sim_types::SimScalar::ZERO;
    run_ticks(&mut harness, 2);
    harness.pause();
    assert!(!map(&harness).parked().is_empty(), "a parked entity");
    harness
}

#[test]
fn a_battle_reset_retires_every_bubble_and_binds_a_new_bound() {
    let mut harness = battle_with_a_parked_bubble();
    let generation = map(&harness).generation();
    let old: Vec<Entity> = map(&harness)
        .iter()
        .map(|(_, entity)| entity)
        .chain(map(&harness).parked().iter().copied())
        .collect();
    harness
        .app
        .world_mut()
        .resource_mut::<client::LabState>()
        .battle_reset_requested = true;
    harness.step(4);
    ready(&mut harness);
    assert!(
        old.iter()
            .all(|entity| harness.app.world().get_entity(*entity).is_err()),
        "every entity of the old battle is despawned"
    );
    assert_ne!(map(&harness).generation(), generation, "a new session");
    assert!(map(&harness).bound().is_some(), "with a new bound");
    assert_eq!(map(&harness).parked().len(), 0);
    assert_eq!(map(&harness).stats().reused, 0, "fresh counters");
}

#[test]
fn a_tab_change_retires_every_bubble() {
    let mut harness = battle_with_a_parked_bubble();
    harness
        .app
        .world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = LabTab::VisualLab;
    harness.step(2);
    assert_eq!(bubbles_left(&mut harness), (0, 0), "no bubble is left");
    assert!(map(&harness).is_empty() && map(&harness).parked().is_empty());
    assert_eq!(
        map(&harness).bound(),
        None,
        "unbound until the battle is shown again"
    );
    harness
        .app
        .world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = LabTab::LiveBattle;
    harness.step(2);
    assert!(map(&harness).bound().is_some(), "bound again");
    assert!(!map(&harness).is_empty(), "drawn again");
}
