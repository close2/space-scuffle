//! Steady-state proof for the client's synchronization systems (remediation Phase 6, exit 1).
//!
//! # Responsibility
//! Proves, through the production [`client::ClientAppPlugin`], that a frame on which nothing new
//! happens creates, modifies and removes no asset, spawns and despawns no entity, and keeps every
//! retained buffer of the sync systems at the same address and capacity. A moving frame may only
//! modify the bubble material of a unit whose heading changed (C-5). A frame of steady combat
//! modifies and removes no combat effect material, and adds one only while the pool of effect
//! slots grows (C-17).
//!
//! # How it fits
//! This is layers 1 and 2 of blueprint D-2 (`docs/plans/2026-11-03-close-phase-6-7-8-exits.md`).
//! It boots the client with `support/battle_harness.rs`, which has no renderer, and adds an
//! observation of its own:
//! - one `AssetEvent<T>` tally per asset type the App registers, read in `Last` after Bevy's
//!   `AssetEvents` set. Bevy raises an event for every creation, mutable access and removal through
//!   `Assets<T>`, so a zero tally is complete evidence of no asset churn;
//! - the entity count after each frame;
//! - the `(name, address, capacity)` of every buffer the sync systems retain in a resource.
//!
//! # Limits
//! - Same address and capacity show reuse. They do not show that no temporary is allocated and
//!   freed within a frame, nor that a buffer freed and allocated again landed on the same block.
//!   Layer 3 (an external allocation count, WP3) covers both.
//! - Buffers held in a system's `Local` (the smog scratch lists, the lab lineup) cannot be read
//!   from outside the system, so they are not listed. Their systems are still covered by the asset
//!   tally and the entity census.
//! - The render world is outside the harness: smog field uploads go through
//!   `Queue::write_texture` there (`smog_bouncing_bubbles_tests.rs` covers them on a GPU).
//!
//! # Invariants
//! - The first test fails when the App registers an asset type that is not watched, so a new
//!   asset type cannot escape the tally.
//! - Every window is preceded by a warm-up, and every test checks that its window was real: the
//!   battle was ready, and on moving windows the simulation ticked.

#![allow(clippy::expect_used, clippy::panic, clippy::cast_precision_loss)]

use std::any::type_name;
use std::collections::{BTreeMap, BTreeSet};

use bevy::animation::graph::AnimationGraph;
use bevy::animation::AnimationClip;
use bevy::asset::{AssetEvents, LoadedFolder, LoadedUntypedAsset};
use bevy::gizmos::LineGizmo;
use bevy::gltf::{Gltf, GltfMesh, GltfNode, GltfPrimitive, GltfSkin};
use bevy::prelude::*;
use bevy::render::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::render::render_resource::Shader;
use bevy::render::storage::ShaderStorageBuffer;
use bevy::scene::{DynamicScene, Scene};
use bevy::sprite::{ColorMaterial, TextureAtlasLayout};
use bevy::text::Font;
use client::battle_hud::card::CommandCard;
use client::battle_hud::feed::BattleFeed;
use client::battle_hud::model::HudModel;
use client::battle_hud::queue::QueuePanel;
use client::battle_hud::result::ResultScreen;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_presentation::BattlePresentation;
use client::battle_selection::BattleSelection;
use client::bubble_shader::{
    still_frame_lanes, BubbleDomeMaterial, BubbleUniformGpu, FlipbookRegistry,
};
use client::indicators::SelectionRingMaterial;
use client::player_presentation::PlayerStyle;
use client::runtime_battle::{MapPresentation, SKIRMISH_SCENARIO};
use client::smog::SmogVolumeMaterial;
use client::terrain::material::TerrainMaterial;
use client::terrain::{MesherScratch, TerrainChunks};
use client::vfx::{CombatFxMaterial, CombatFxMaterialPool, CombatLightEffect};
use client::{ClientMode, CombatVisualEvent, SmogGrid3d, SwarmBubble};
use sim_types::snapshot::BubbleRenderInstance;
use sim_types::{EntityId, RenderSnapshot};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{Harness, LOCAL_PLAYER};

/// Frames a battle gets to load its media and map presentation before a test gives up.
const READY_FRAMES: usize = 600;

/// Frames run after readiness and before a window, so first-use work is excluded.
const WARM_FRAMES: usize = 30;

/// Length of a paused or swarm window, in frames.
const PAUSED_FRAMES: usize = 64;

/// Ticks of warm-up before a moving window, and the window's length, in simulation ticks.
const WARM_TICKS: u32 = 30;
const MOVING_TICKS: u32 = 200;

/// Frames of combat before the effect window, so the pool of effect materials can reach its size.
const COMBAT_WARM_FRAMES: usize = 120;

/// Length of the combat effect window, in frames.
const COMBAT_FRAMES: usize = 240;

/// Fewest combat events the effect window must hold to show steady combat.
const MIN_COMBAT_EVENTS: u64 = 100;

/// A marching window shorter than this does not show steady movement.
const MIN_MARCH_TICKS: u32 = 50;

/// Orders the local town hall takes in the skirmish test: more than the queue panel lists, so
/// its overflow text is written on every tick.
const QUEUED_JOBS: usize = 8;

/// More minerals than the queued jobs cost.
const RICH: u32 = 100_000;

// --- Observation ---------------------------------------------------------------------------------

/// What one event says happened to an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EventKind {
    Added,
    Modified,
    Removed,
    Unused,
    LoadedWithDependencies,
}

impl EventKind {
    fn of<T: Asset>(event: &AssetEvent<T>) -> Self {
        match event {
            AssetEvent::Added { .. } => Self::Added,
            AssetEvent::Modified { .. } => Self::Modified,
            AssetEvent::Removed { .. } => Self::Removed,
            AssetEvent::Unused { .. } => Self::Unused,
            AssetEvent::LoadedWithDependencies { .. } => Self::LoadedWithDependencies,
        }
    }
}

/// Asset events seen since the last reset, by `Assets<T>` type name and kind.
#[derive(Resource, Debug, Default)]
struct AssetEventTally {
    counts: BTreeMap<(&'static str, EventKind), u64>,
    /// Combat events the client published, so a window can end before contact.
    combat_events: u64,
    /// Combat effect entities spawned, counted when their [`CombatLightEffect`] is added.
    effects_spawned: u64,
}

impl AssetEventTally {
    fn count(&self, asset: &'static str, kind: EventKind) -> u64 {
        self.counts.get(&(asset, kind)).copied().unwrap_or(0)
    }

    /// Every non-zero count other than `except`.
    fn others(&self, except: &[(&'static str, EventKind)]) -> Vec<(&'static str, EventKind, u64)> {
        self.counts
            .iter()
            .filter(|(key, count)| **count > 0 && !except.contains(key))
            .map(|((asset, kind), count)| (*asset, *kind, *count))
            .collect()
    }
}

/// Counts the events of asset type `T`. Runs in `Last`, after Bevy's `AssetEvents` set.
fn tally<T: Asset>(
    mut events: EventReader<'_, '_, AssetEvent<T>>,
    mut tally: ResMut<'_, AssetEventTally>,
) {
    for event in events.read() {
        *tally
            .counts
            .entry((type_name::<Assets<T>>(), EventKind::of(event)))
            .or_default() += 1;
    }
}

/// Counts the combat events the client published to its effects.
fn count_combat_events(
    mut events: EventReader<'_, '_, CombatVisualEvent>,
    mut tally: ResMut<'_, AssetEventTally>,
) {
    tally.combat_events += events.read().count() as u64;
}

/// Counts the combat effect entities spawned since the last frame.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn count_spawned_effects(
    spawned: Query<'_, '_, (), Added<CombatLightEffect>>,
    mut tally: ResMut<'_, AssetEventTally>,
) {
    tally.effects_spawned += spawned.iter().count() as u64;
}

/// Declares the watched asset types: [`add_tallies`] adds one tally system per type, and
/// [`watched`] names them as `Assets<T>` for the inventory test.
macro_rules! watched_assets {
    ($($asset:ty),+ $(,)?) => {
        fn add_tallies(app: &mut App) {
            $(app.add_systems(Last, tally::<$asset>.after(AssetEvents));)+
        }

        fn watched() -> BTreeSet<&'static str> {
            BTreeSet::from([$(type_name::<Assets<$asset>>(),)+])
        }
    };
}

// The client's own asset types, then every type Bevy's plugins register, so an asset change of
// any kind is seen.
watched_assets!(
    BubbleDomeMaterial,
    SelectionRingMaterial,
    StandardMaterial,
    CombatFxMaterial,
    SmogVolumeMaterial,
    TerrainMaterial,
    Image,
    Mesh,
    (),
    AnimationClip,
    AnimationGraph,
    LoadedUntypedAsset,
    LoadedFolder,
    LineGizmo,
    Gltf,
    GltfMesh,
    GltfNode,
    GltfPrimitive,
    GltfSkin,
    SkinnedMeshInverseBindposes,
    Shader,
    ShaderStorageBuffer,
    DynamicScene,
    Scene,
    ColorMaterial,
    TextureAtlasLayout,
    Font,
);

/// Starts the observation on a booted client.
fn observe(harness: &mut Harness) {
    let app = &mut harness.app;
    app.init_resource::<AssetEventTally>()
        .add_systems(Last, (count_combat_events, count_spawned_effects));
    add_tallies(app);
    // The tally systems read every event still buffered, so the observation starts clean.
    harness.step(1);
    reset(harness);
}

fn reset(harness: &mut Harness) {
    *harness.app.world_mut().resource_mut::<AssetEventTally>() = AssetEventTally::default();
}

fn tallies(harness: &Harness) -> &AssetEventTally {
    harness.app.world().resource::<AssetEventTally>()
}

/// Names of every `Assets<T>` resource present in `world`.
fn registered(world: &World) -> BTreeSet<String> {
    world
        .components()
        .iter()
        .filter(|info| info.name().starts_with("bevy_asset::assets::Assets<"))
        .filter(|info| world.get_resource_by_id(info.id()).is_some())
        .map(|info| info.name().to_owned())
        .collect()
}

fn census(harness: &Harness) -> u32 {
    harness.app.world().entities().len()
}

/// One retained buffer: what it is, where its storage starts, and how many elements fit.
type Buffer = (&'static str, usize, usize);

fn string(name: &'static str, text: &str, capacity: usize) -> Buffer {
    (name, text.as_ptr().addr(), capacity)
}

fn vector<T>(name: &'static str, items: &Vec<T>) -> Buffer {
    (name, items.as_ptr().addr(), items.capacity())
}

/// Every buffer the sync systems retain in a resource of `world`, in a fixed order.
fn retained_buffers(world: &World) -> Vec<Buffer> {
    let mut found = Vec::new();
    if let Some(model) = world.get_resource::<HudModel>() {
        found.extend(model.retained_buffers());
    }
    if let Some(card) = world.get_resource::<CommandCard>() {
        for text in card.text.iter().chain(&card.fixed_text) {
            found.push(string(
                "card.caption",
                &text.caption,
                text.caption.capacity(),
            ));
            found.push(string(
                "card.tooltip",
                &text.tooltip,
                text.tooltip.capacity(),
            ));
        }
    }
    if let Some(panel) = world.get_resource::<QueuePanel>() {
        found.extend(panel.retained_buffers());
    }
    if let Some(feed) = world.get_resource::<BattleFeed>() {
        found.extend(feed.retained_buffers());
    }
    if let Some(screen) = world.get_resource::<ResultScreen>() {
        found.extend(screen.retained_buffers());
    }
    if let Some(fog) = world.get_resource::<BattleFog>() {
        found.extend(fog.retained_buffers());
    }
    if let Some(selection) = world.get_resource::<BattleSelection>() {
        found.extend(selection.retained_buffers());
    }
    if let Some(session) = world.get_resource::<BattleSession>() {
        found.extend(session.frames.retained_buffers());
    }
    if let Some(presentation) = world.get_resource::<BattlePresentation>() {
        found.push(string(
            "presentation.map_title",
            &presentation.map_title,
            presentation.map_title.capacity(),
        ));
        found.push(vector("presentation.players", &presentation.players));
        for player in &presentation.players {
            found.push(vector("presentation.roster", &player.roster));
            for entry in &player.roster {
                found.push(string(
                    "presentation.roster.label",
                    &entry.label,
                    entry.label.capacity(),
                ));
            }
        }
    }
    if let Some(map) = world.get_resource::<MapPresentation>() {
        found.push(vector("map.objects", &map.objects));
        found.push(vector("map.sprite_images", &map.sprite_images));
    }
    let smog = world.resource::<SmogGrid3d>();
    found.extend(smog.retained_byte_buffers());
    if let Some(scratch) = world.get_resource::<MesherScratch>() {
        found.push(vector("mesher.positions", &scratch.positions));
        found.push(vector("mesher.normals", &scratch.normals));
        found.push(vector("mesher.uv0", &scratch.uv0));
        found.push(vector("mesher.uv1", &scratch.uv1));
        found.push(vector("mesher.indices", &scratch.indices));
    }
    if let Some(chunks) = world.get_resource::<TerrainChunks>() {
        found.push(vector("terrain.chunks", &chunks.chunks));
    }
    found
}

/// Every buffer of `before` whose address or capacity is not what it was, with both values.
fn moved(before: &[Buffer], after: &[Buffer]) -> Vec<(Buffer, Buffer)> {
    assert_eq!(
        before.iter().map(|buffer| buffer.0).collect::<Vec<_>>(),
        after.iter().map(|buffer| buffer.0).collect::<Vec<_>>(),
        "the set of retained buffers must not change"
    );
    before
        .iter()
        .zip(after)
        .filter(|(a, b)| a != b)
        .map(|(a, b)| (*a, *b))
        .collect()
}

// --- Driving ------------------------------------------------------------------------------------

/// Why the scene is not yet ready to observe, or `None` once its media is loaded and, for a
/// battle, the session is committed and its map presentation is drawn.
fn unready(harness: &Harness, battle: bool) -> Option<String> {
    let world = harness.app.world();
    let images = world.resource::<Assets<Image>>();
    let registry = world.resource::<FlipbookRegistry>();
    let map = world.get_resource::<MapPresentation>();
    let pending = registry
        .textures_by_id
        .values()
        .chain(registry.portrait_images_by_id.values())
        .chain(registry.textures.values())
        .chain(registry.portrait_images.values())
        .chain(map.iter().flat_map(|map| &map.sprite_images))
        .filter(|image| !images.contains(*image))
        .count();
    if registry.textures_by_id.is_empty() {
        Some("no flipbook is registered".to_owned())
    } else if pending > 0 {
        Some(format!("{pending} images are still loading"))
    } else if !battle {
        None
    } else if !world.resource::<BattleSession>().is_ready() {
        Some("the session is not ready".to_owned())
    } else if map.is_none_or(|map| map.objects.is_empty()) {
        Some("the map presentation is empty".to_owned())
    } else {
        None
    }
}

/// Steps until the battle is ready, then warms for [`WARM_FRAMES`] and starts the observation.
///
/// `battle` says whether a battle is on the field, which a swarm scene is not.
fn settle(harness: &mut Harness, battle: bool) {
    let mut frames = 0;
    while let Some(reason) = unready(harness, battle) {
        harness.step(1);
        frames += 1;
        assert!(
            frames < READY_FRAMES,
            "the battle never became ready: {reason}"
        );
    }
    harness.step(WARM_FRAMES);
    observe(harness);
}

/// Steps until the simulation has run `ticks` more ticks.
fn run_ticks(harness: &mut Harness, ticks: u32) {
    let goal = harness.world().current_tick + ticks;
    let mut frames = 0;
    while harness.world().current_tick < goal {
        harness.step(1);
        frames += 1;
        assert!(frames < 10_000, "the battle stopped ticking");
    }
}

/// Asserts that [`PAUSED_FRAMES`] paused frames change nothing.
fn assert_paused_frames_change_nothing(harness: &mut Harness) {
    let tick = harness.world().current_tick;
    let entities = census(harness);
    let buffers = retained_buffers(harness.app.world());
    harness.step(PAUSED_FRAMES);
    assert_eq!(
        harness.world().current_tick,
        tick,
        "the battle stayed paused"
    );
    assert_eq!(tallies(harness).others(&[]), Vec::new(), "asset events");
    assert_eq!(census(harness), entities, "entities spawned or despawned");
    assert_eq!(
        moved(&buffers, &retained_buffers(harness.app.world())),
        Vec::new(),
        "retained buffers that moved or grew"
    );
}

/// Boots the skirmish with the local workers in view.
fn skirmish() -> Harness {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = local_workers(&harness)[0];
    let position = harness.simulated_position(worker);
    harness.look_at(position);
    harness
}

/// The local player's harvesters, ordered by ID.
fn local_workers(harness: &Harness) -> Vec<EntityId> {
    let world = harness.world();
    world
        .entities
        .values()
        .filter(|unit| {
            unit.owner == LOCAL_PLAYER
                && world
                    .catalog
                    .get(unit.unit_type_id)
                    .is_ok_and(|archetype| archetype.capabilities.harvester.is_some())
        })
        .map(|unit| unit.id)
        .collect()
}

/// Sends every local worker to the nearest mineral with no unit in front of it, through the
/// mouse, while paused.
fn send_workers_to_harvest(harness: &mut Harness) {
    let workers = local_workers(harness);
    let first = harness.simulated_position(workers[0]);
    let world = harness.world();
    let clear = |at: Vec2| {
        world.entities.values().all(|unit| {
            Vec2::new(unit.position.x.to_f32(), unit.position.y.to_f32()).distance(at) > 3.0
        })
    };
    let (node, _) = world
        .map
        .resources()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == sim_map::ResourceKind::Mineral)
        .map(|(index, node)| (index, Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32())))
        .filter(|(_, at)| clear(*at))
        .min_by(|a, b| a.1.distance(first).total_cmp(&b.1.distance(first)))
        .expect("the skirmish has a mineral away from the start");
    for worker in &workers {
        harness.select(*worker);
        let position = harness.app.world().resource::<MapPresentation>().objects[node].position;
        let point = harness.screen_position(position);
        harness.click(MouseButton::Right, point);
    }
    assert_eq!(
        harness.pending_orders(),
        workers.len(),
        "one harvest order per worker: {}",
        harness.status()
    );
}

/// The local player's bank of minerals.
fn minerals(harness: &Harness) -> u32 {
    harness
        .world()
        .player_resources
        .get(LOCAL_PLAYER)
        .expect("invariant: LOCAL_PLAYER has a player slot")
        .minerals
}

/// Selects the local town hall and queues [`QUEUED_JOBS`] jobs on it with the card's train key.
fn queue_jobs_at_town_hall(harness: &mut Harness) -> EntityId {
    harness
        .world_mut()
        .player_resources
        .get_mut(LOCAL_PLAYER)
        .expect("invariant: LOCAL_PLAYER has a player slot")
        .minerals = RICH;
    let base = harness.local_structure();
    harness.look_at_unit(base);
    harness.select(base);
    assert_eq!(harness.selection().ids(), [base]);
    let key = harness
        .app
        .world()
        .resource::<CommandCard>()
        .slots
        .iter()
        .flatten()
        .find(|slot| matches!(slot.action, client::battle_hud::card::CardAction::Train(_)))
        .expect("a base trains something")
        .key;
    for _ in 0..QUEUED_JOBS {
        harness.tap_key(key);
    }
    base
}

/// Bubbles of `before` whose record in `after` would draw differently, matched by entity.
///
/// C-5 says a unit may only touch its material when it turns. The uniform also carries the
/// flipbook frame, the action, the bars and the hit flashes, which change on a tick without a
/// turn, so the rule this file holds the sync to is the general one: no modification unless the
/// shader would read a different value. The packing is the production
/// [`BubbleUniformGpu::from_instance`] with the battle's still-frame lanes, and
/// [`BubbleUniformGpu::renders_same_as`] ignores the one lane the shader never reads (position).
/// Ownership and selection do not change within a window, so one fixed style stands for every
/// owner.
fn redrawn(before: &RenderSnapshot, after: &RenderSnapshot, registry: &FlipbookRegistry) -> u64 {
    let style = PlayerStyle {
        srgb: [1.0; 3],
        linear: Vec4::ONE,
        ui: bevy_egui::egui::Color32::WHITE,
    };
    let pack = |bubble: &BubbleRenderInstance| {
        BubbleUniformGpu::from_instance(
            bubble,
            style,
            still_frame_lanes(registry.get_meta(bubble.unit_type), bubble.anim_state),
        )
    };
    let drawn: BTreeMap<EntityId, BubbleUniformGpu> = before
        .bubbles
        .iter()
        .map(|bubble| (bubble.entity_id, pack(bubble)))
        .collect();
    after
        .bubbles
        .iter()
        .filter(|bubble| {
            drawn
                .get(&bubble.entity_id)
                .is_some_and(|uniform| !uniform.renders_same_as(&pack(bubble)))
        })
        .count() as u64
}

/// Bubbles of `before` whose heading differs in `after`, matched by entity. Reported beside
/// [`redrawn`] so a failure shows how much of the change is turning.
fn turned(before: &RenderSnapshot, after: &RenderSnapshot) -> u64 {
    let headings: BTreeMap<EntityId, u32> = before
        .bubbles
        .iter()
        .map(|bubble| (bubble.entity_id, bubble.heading_angle_rad.to_bits()))
        .collect();
    after
        .bubbles
        .iter()
        .filter(|bubble| {
            headings
                .get(&bubble.entity_id)
                .is_some_and(|heading| *heading != bubble.heading_angle_rad.to_bits())
        })
        .count() as u64
}

/// Counts of (unit, frame) pairs over a moving window.
#[derive(Debug, Clone, Copy, Default)]
struct Motion {
    turned: u64,
    redrawn: u64,
}

/// Steps one frame and adds its changed bubbles to `motion`.
fn step_counting(harness: &mut Harness, motion: &mut Motion) {
    let before = presented(harness);
    harness.step(1);
    let after = presented(harness);
    let registry = harness.app.world().resource::<FlipbookRegistry>();
    motion.turned += turned(&before, &after);
    motion.redrawn += redrawn(&before, &after, registry);
}

fn presented(harness: &Harness) -> RenderSnapshot {
    harness
        .app
        .world()
        .resource::<BattleSession>()
        .frames
        .presented()
        .clone()
}

// --- Tests ---------------------------------------------------------------------------------------

#[test]
fn asset_types_under_watch_are_every_asset_the_client_registers() {
    for mode in [ClientMode::LiveBattle, ClientMode::Skirmish] {
        let mut harness = Harness::boot(mode);
        observe(&mut harness);
        let registered = registered(harness.app.world());
        let watched: BTreeSet<String> = watched().into_iter().map(str::to_owned).collect();
        assert_eq!(
            registered.difference(&watched).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "{mode:?}: asset types registered but not watched"
        );
        assert_eq!(
            watched.difference(&registered).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "{mode:?}: asset types watched but not registered"
        );
    }
}

#[test]
fn paused_battle_frames_create_modify_and_remove_nothing() {
    let mut harness = Harness::battle();
    settle(&mut harness, true);
    assert_paused_frames_change_nothing(&mut harness);

    // The player's profile adds the HUD's sync systems, so it is held to the same rule.
    let mut harness = skirmish();
    let worker = local_workers(&harness)[0];
    harness.select(worker);
    settle(&mut harness, true);
    assert_paused_frames_change_nothing(&mut harness);
}

#[test]
fn selected_structure_keeps_card_queue_and_feed_buffers_on_every_tick() {
    let mut harness = skirmish();
    settle(&mut harness, true);
    send_workers_to_harvest(&mut harness);
    // The first delivery grows the lists of laden workers to their working size, which is a
    // buffer's first use, not churn, so the window starts after it.
    harness.resume();
    let unpaid = minerals(&harness);
    let mut frames = 0;
    while minerals(&harness) <= unpaid {
        harness.step(1);
        frames += 1;
        assert!(frames < READY_FRAMES * 4, "the workers never delivered");
    }
    harness.pause();
    let base = queue_jobs_at_town_hall(&mut harness);
    harness.resume();
    run_ticks(&mut harness, WARM_TICKS);
    let workers = local_workers(&harness);
    let positions = |harness: &Harness| -> Vec<Vec2> {
        workers
            .iter()
            .map(|id| harness.simulated_position(*id))
            .collect()
    };
    assert_eq!(
        harness.app.world().resource::<QueuePanel>().overflow,
        format!(
            "+{} more",
            QUEUED_JOBS - client::battle_hud::queue::QUEUE_ROWS
        ),
        "the queue must overflow its rows, so its text is written on every tick"
    );
    let entities = census(&harness);
    let buffers = retained_buffers(harness.app.world());
    reset(&mut harness);
    let mut motion = Motion::default();
    let mut walked = vec![false; workers.len()];
    let goal = harness.world().current_tick + MOVING_TICKS;
    while harness.world().current_tick < goal {
        let before = positions(&harness);
        step_counting(&mut harness, &mut motion);
        for ((walked, before), after) in walked.iter_mut().zip(before).zip(positions(&harness)) {
            *walked |= before != after;
        }
    }
    assert_eq!(harness.selection().ids(), [base]);
    assert_eq!(
        walked,
        vec![true; workers.len()],
        "every worker must move in the window"
    );
    let tally = tallies(&harness);
    let bubbles = type_name::<Assets<BubbleDomeMaterial>>();
    assert_eq!(
        tally.others(&[(bubbles, EventKind::Modified)]),
        Vec::new(),
        "asset events other than bubble materials of redrawn units"
    );
    let modified = tally.count(bubbles, EventKind::Modified);
    assert!(
        modified <= motion.redrawn,
        "{modified} bubble materials modified; (unit, frame) pairs: {motion:?}"
    );
    assert_eq!(census(&harness), entities, "entities spawned or despawned");
    assert_eq!(
        moved(&buffers, &retained_buffers(harness.app.world())),
        Vec::new(),
        "retained buffers that moved or grew"
    );
}

#[test]
fn marching_frames_touch_only_turning_units() {
    let mut harness = Harness::battle();
    settle(&mut harness, true);
    let start = harness.world().current_tick;
    let entities = census(&harness);
    harness.resume();
    // The state after the latest frame without a combat event: the window ends there, because
    // contact spawns effects by design.
    let mut motion = Motion::default();
    let (counts, census_before_contact, tick, motion) = loop {
        let quiet = (
            tallies(&harness).counts.clone(),
            census(&harness),
            harness.world().current_tick,
            motion,
        );
        step_counting(&mut harness, &mut motion);
        if tallies(&harness).combat_events > 0 {
            break quiet;
        }
        assert!(
            harness.world().current_tick < start + 2_000,
            "the armies never met"
        );
    };
    assert!(
        tick >= start + MIN_MARCH_TICKS,
        "contact came at tick {tick}, too early for a marching window"
    );
    let bubbles = type_name::<Assets<BubbleDomeMaterial>>();
    let others: Vec<_> = counts
        .iter()
        .filter(|(key, count)| **count > 0 && **key != (bubbles, EventKind::Modified))
        .collect();
    assert_eq!(
        others,
        Vec::<(&(&str, EventKind), &u64)>::new(),
        "asset events while marching"
    );
    let modified = counts
        .get(&(bubbles, EventKind::Modified))
        .copied()
        .unwrap_or(0);
    assert!(
        modified <= motion.redrawn,
        "{modified} bubble materials modified; (unit, frame) pairs: {motion:?}"
    );
    assert!(motion.turned > 0, "the armies must turn on their way");
    assert_eq!(
        census_before_contact, entities,
        "entities spawned or despawned"
    );
}

#[test]
fn swarm_stress_frames_create_modify_and_remove_nothing() {
    let mut harness = Harness::boot(ClientMode::SwarmStressTest);
    settle(&mut harness, false);
    let entities = census(&harness);
    let buffers = retained_buffers(harness.app.world());
    harness.step(PAUSED_FRAMES);
    let world = harness.app.world_mut();
    let shared: BTreeSet<AssetId<BubbleDomeMaterial>> = world
        .query_filtered::<&MeshMaterial3d<BubbleDomeMaterial>, With<SwarmBubble>>()
        .iter(world)
        .map(|material| material.0.id())
        .collect();
    assert!(
        harness
            .app
            .world()
            .resource::<client::LabState>()
            .swarm_bubble_count
            > 0,
        "the swarm must be on the field"
    );
    // The swarm animates: each frame may rewrite each shared material once, with its group's
    // flipbook frame and heading, and nothing else.
    let bubbles = type_name::<Assets<BubbleDomeMaterial>>();
    let tally = tallies(&harness);
    assert_eq!(
        tally.others(&[(bubbles, EventKind::Modified)]),
        Vec::new(),
        "asset events other than the shared swarm materials"
    );
    let modified = tally.count(bubbles, EventKind::Modified);
    assert!(
        modified <= (shared.len() * PAUSED_FRAMES) as u64,
        "{modified} bubble materials modified over {PAUSED_FRAMES} frames, {} shared",
        shared.len()
    );
    assert_eq!(census(&harness), entities, "entities spawned or despawned");
    assert_eq!(
        moved(&buffers, &retained_buffers(harness.app.world())),
        Vec::new(),
        "retained buffers that moved or grew"
    );
}

/// Material slots the effect pool has created since the client started.
fn pool_created(harness: &Harness) -> u64 {
    harness
        .app
        .world()
        .resource::<CombatFxMaterialPool>()
        .created()
}

/// C-17: steady combat reuses pooled effect materials and never re-prepares one.
///
/// Every effect writes its shader data into its own component and the render world copies it into
/// the slot's existing GPU buffer, so the window may add materials only while the pool grows, and
/// must modify and remove none.
#[test]
fn combat_frames_reuse_effect_materials() {
    let mut harness = Harness::battle();
    settle(&mut harness, true);
    harness.resume();
    let start = harness.world().current_tick;
    while tallies(&harness).combat_events == 0 {
        harness.step(1);
        assert!(
            harness.world().current_tick < start + 2_000,
            "the armies never met"
        );
    }
    harness.step(COMBAT_WARM_FRAMES);
    reset(&mut harness);
    let created_before = pool_created(&harness);
    harness.step(COMBAT_FRAMES);
    let grown = pool_created(&harness) - created_before;
    let tally = tallies(&harness);
    let materials = type_name::<Assets<CombatFxMaterial>>();
    let added = tally.count(materials, EventKind::Added);
    eprintln!(
        "combat window: {} combat events, {} effects spawned, {added} materials added, pool grew by \
         {grown}",
        tally.combat_events, tally.effects_spawned
    );
    assert!(
        tally.combat_events >= MIN_COMBAT_EVENTS,
        "the window must hold steady combat: {} combat events",
        tally.combat_events
    );
    assert_eq!(
        tally.count(materials, EventKind::Modified),
        0,
        "effect materials modified"
    );
    assert_eq!(
        tally.count(materials, EventKind::Removed),
        0,
        "effect materials removed"
    );
    assert!(
        added <= grown,
        "{added} effect materials added while the pool created {grown}"
    );
    assert!(
        added < tally.effects_spawned,
        "{added} effect materials added for {} effects spawned: slots are not reused",
        tally.effects_spawned
    );
}
