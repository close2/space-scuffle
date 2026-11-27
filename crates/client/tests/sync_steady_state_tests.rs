//! Steady-state proof for the client's synchronization systems (remediation Phase 6, exit 1).
//!
//! # Responsibility
//! Proves, through the production [`client::ClientAppPlugin`], that a frame on which nothing new
//! happens creates, modifies and removes no asset, spawns and despawns no entity, and keeps every
//! retained buffer of the sync systems at the same address and capacity. A moving frame may only
//! modify the bubble material of a unit whose heading changed (C-5); a swarm frame modifies no
//! material at all, even while bubbles bounce (plan `2026-11-04-battle-flipbook-animation`, D-9).
//! A frame of steady combat
//! modifies and removes no combat effect material, and adds one only while the pool of effect
//! slots grows (C-17). A battle in which units arrive and leave on their own clocks (Broodlings
//! expiring) or at a test's cadence (air and ground units killed and spawned) creates and
//! removes no asset and spawns no entity once the render pool has reached its regime (plan
//! `2026-11-29-render-entity-pool` R-1, R-2), and so does one in which two Carriers cycle their
//! interceptors through launch, attack, dock and relaunch (plan `2026-11-28-carrier-interceptors`
//! H-20).
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

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]

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
use client::bubble_shader::{BubbleDomeMaterial, BubbleUniformGpu, FlipbookRegistry};
use client::effects::runtime::EffectRuntime;
use client::effects::systems::{EffectCatalogSource, EffectLightPool};
use client::flipbook_clock::{battle_uniform, FlipbookClock, FlipbookPhase};
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

#[path = "support/effect_fixtures.rs"]
mod effect_fixtures;

use battle_harness::{Harness, LOCAL_PLAYER};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/attack_snapshot_stage.rs"]
mod attack_snapshot_stage;

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/unit_cycling.rs"]
mod unit_cycling;

use client::battle_hud::attack_snapshots::{AttackSnapshotBubble, AttackSnapshotPool};

/// Frames a battle gets to load its media and map presentation before a test gives up.
const READY_FRAMES: usize = 600;

/// Frames run after readiness and before a window, so first-use work is excluded.
const WARM_FRAMES: usize = 30;

/// Length of a paused window, in frames.
const PAUSED_FRAMES: usize = 64;

/// Length of the swarm window, in 40 ms frames: 13.2 s, longer than the 12 s a swarm bubble takes
/// to cross between its walls at 1.5 world units per second, so every bubble bounces in it.
const SWARM_FRAMES: usize = 330;

/// Bubbles in the swarm.
const SWARM_BUBBLES: usize = 200;

/// Shared swarm materials: two groups, each with one material per heading.
const SWARM_MATERIALS: usize = 4;

/// Ticks of warm-up before a moving window, and the window's length, in simulation ticks.
const WARM_TICKS: u32 = 30;
const MOVING_TICKS: u32 = 200;

/// Frames of combat before the effect window, so the pool of effect materials can reach its size.
const COMBAT_WARM_FRAMES: usize = 120;

/// Length of the combat effect window, in frames.
const COMBAT_FRAMES: usize = 240;

/// Fewest combat events the effect window must hold to show steady combat.
const MIN_COMBAT_EVENTS: u64 = 100;

/// Most consecutive effect windows judged while looking for one with steady combat.
const MAX_COMBAT_WINDOWS: usize = 4;

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
    if let Some(pool) = world.get_resource::<AttackSnapshotPool>() {
        found.extend(pool.retained_buffers());
    }
    if let Some(selection) = world.get_resource::<BattleSelection>() {
        found.extend(selection.retained_buffers());
    }
    if let Some(map) = world.get_resource::<client::RenderEntityMap>() {
        found.extend(map.retained_buffers());
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
    if let Some(beams) = world.get_resource::<client::effects::beams::HeldBeams>() {
        found.extend(beams.buffers());
    }
    if let Some(runtime) = world.get_resource::<EffectRuntime>() {
        let names = [
            "effects.cards",
            "effects.depth",
            "effects.order",
            "effects.sorted",
            "effects.aged_decals",
            "effects.lights",
            "effects.distortions",
        ];
        for (name, (address, capacity)) in names.into_iter().zip(runtime.frame.buffers()) {
            found.push((name, address, capacity));
        }
        // The retained decal table and the spawn path's indices (plan
        // 2026-11-19-retained-effect-instances D-4, D-11).
        let table = [
            "effects.table.records",
            "effects.table.keys",
            "effects.table.next",
            "effects.table.head",
            "effects.table.free",
            "effects.table.order",
            "effects.table.dirty_bits",
            "effects.table.dirty",
            "effects.table.expiry",
            "effects.table.patches",
        ];
        for (name, (address, capacity)) in table.into_iter().zip(runtime.decal_table().buffers()) {
            found.push((name, address, capacity));
        }
        let indices = [
            "effects.lines",
            "effects.fire.major",
            "effects.fire.minor",
            "effects.fire.ambient",
            "effects.marks",
            "effects.detail",
        ];
        for (name, (address, capacity)) in indices.into_iter().zip(runtime.index_buffers()) {
            found.push((name, address, capacity));
        }
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
/// flipbook lanes, the action, the bars and the hit flashes, which change on a tick without a
/// turn, so the rule this file holds the sync to is the general one: no modification unless the
/// shader would read a different value. The packing is the production [`battle_uniform`], from
/// the presented record and the bubble's [`FlipbookPhase`] on that frame, and
/// [`BubbleUniformGpu::renders_same_as`] ignores the one lane the shader never reads (position).
/// Ownership and selection do not change within a window, so one fixed style stands for every
/// owner.
fn redrawn(before: &Presented, after: &Presented, registry: &FlipbookRegistry) -> u64 {
    let style = PlayerStyle {
        srgb: [1.0; 3],
        linear: Vec4::ONE,
        ui: bevy_egui::egui::Color32::WHITE,
    };
    let pack = |presented: &Presented, bubble: &BubbleRenderInstance| {
        let phase = presented
            .phases
            .get(&bubble.entity_id)
            .copied()
            .expect("every presented record has a live bubble");
        battle_uniform(
            registry.get_meta(bubble.unit_type),
            bubble,
            style,
            presented.snapshot.tick,
            phase,
        )
        .expect("the shipped media is playable")
    };
    let drawn: BTreeMap<EntityId, BubbleUniformGpu> = before
        .snapshot
        .bubbles
        .iter()
        .map(|bubble| (bubble.entity_id, pack(before, bubble)))
        .collect();
    after
        .snapshot
        .bubbles
        .iter()
        .filter(|bubble| {
            drawn
                .get(&bubble.entity_id)
                .is_some_and(|uniform| !uniform.renders_same_as(&pack(after, bubble)))
        })
        .count() as u64
}

/// Bubbles of `before` whose heading differs in `after`, matched by entity. Reported beside
/// [`redrawn`] so a failure shows how much of the change is turning.
fn turned(before: &Presented, after: &Presented) -> u64 {
    let (before, after) = (&before.snapshot, &after.snapshot);
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

/// One frame's presented snapshot and the flipbook phase each live bubble held after it.
struct Presented {
    snapshot: RenderSnapshot,
    phases: BTreeMap<EntityId, FlipbookPhase>,
}

fn presented(harness: &Harness) -> Presented {
    let world = harness.app.world();
    let phases = world
        .resource::<client::RenderEntityMap>()
        .iter()
        .map(|(id, entity)| {
            let phase = *world
                .get::<FlipbookPhase>(entity)
                .expect("a live bubble has a flipbook phase");
            (id, phase)
        })
        .collect();
    Presented {
        snapshot: world.resource::<BattleSession>().frames.presented().clone(),
        phases,
    }
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

/// A resumed skirmish whose units all stand idle: the clock advances on every frame, and yet no
/// bubble material is modified and no storage buffer is touched (plan D-2, D-3: time alone moves
/// only the shared clock, written in place during extraction).
#[test]
fn running_idle_army_modifies_no_bubble_material() {
    let mut harness = skirmish();
    settle(&mut harness, true);
    harness.resume();
    harness.step(WARM_FRAMES);
    let presented = presented(&harness);
    let idle = |presented: &Presented| {
        presented
            .snapshot
            .bubbles
            .iter()
            .all(|bubble| bubble.anim_state == sim_types::AnimState::Idle)
    };
    assert!(idle(&presented), "every unit of the skirmish starts idle");
    let start = harness.world().current_tick;
    let clock = harness.app.world().resource::<FlipbookClock>().now;
    let entities = census(&harness);
    reset(&mut harness);
    for _ in 0..MOVING_TICKS {
        harness.step(1);
        assert!(idle(&self::presented(&harness)), "the army stays idle");
    }
    assert_eq!(
        harness.world().current_tick,
        start + MOVING_TICKS,
        "one tick per frame"
    );
    assert_eq!(
        harness.app.world().resource::<FlipbookClock>().now - clock,
        MOVING_TICKS * client::flipbook_clock::CLOCK_UNITS_PER_TICK,
        "the clock advanced with every presented tick"
    );
    let tally = tallies(&harness);
    assert_eq!(
        tally.count(
            type_name::<Assets<BubbleDomeMaterial>>(),
            EventKind::Modified
        ),
        0,
        "bubble materials modified while the idle army animated"
    );
    let buffers = type_name::<Assets<ShaderStorageBuffer>>();
    assert_eq!(
        [
            EventKind::Added,
            EventKind::Modified,
            EventKind::Removed,
            EventKind::Unused,
            EventKind::LoadedWithDependencies,
        ]
        .map(|kind| tally.count(buffers, kind)),
        [0; 5],
        "storage buffer events"
    );
    assert_eq!(tally.others(&[]), Vec::new(), "asset events");
    assert_eq!(census(&harness), entities, "entities spawned or despawned");
}

#[test]
fn selected_structure_keeps_card_queue_and_feed_buffers_on_every_tick() {
    let mut harness = skirmish();
    settle(&mut harness, true);
    send_workers_to_harvest(&mut harness);
    // The lists of laden workers grow to their working size as the workers' trips settle: one
    // at a time gathers a field, so the workers sent together spread over the line and carry
    // out of phase (plan `2026-11-23-harvest-occupancy` D-2, D-3). That growth is a buffer's
    // first use, not churn, so the window starts once every worker has delivered at least once
    // (the plan's Amendment 1), read from the simulation.
    harness.resume();
    let crew = local_workers(&harness);
    let mut carried = vec![false; crew.len()];
    let mut delivered = vec![false; crew.len()];
    let mut frames = 0;
    while !delivered.iter().all(|&done| done) {
        harness.step(1);
        let world = harness.world();
        for ((worker, carried), delivered) in crew.iter().zip(&mut carried).zip(&mut delivered) {
            let carrying = world
                .entities
                .get(worker)
                .is_some_and(|e| e.cargo.is_some());
            *delivered |= *carried && !carrying;
            *carried = carrying;
        }
        frames += 1;
        assert!(
            frames < READY_FRAMES * 4,
            "not every worker delivered: {delivered:?}"
        );
    }
    harness.pause();
    let base = queue_jobs_at_town_hall(&mut harness);
    // Plan `2026-11-25-rallies` D-8: the base's rally is a route of three waypoints, two points
    // before its auto-rallied field, so the window draws three reticles and three chained lines.
    let centre = harness.simulated_position(base);
    let points = [
        harness.open_ground(centre, 6.0),
        harness.open_ground(centre, 9.0),
    ];
    let world = harness.world_mut();
    let route = world
        .entities
        .get_mut(&base)
        .and_then(|entity| entity.rallies.first_mut())
        .expect("the base auto-rallied");
    let field = route.waypoints[0];
    route.waypoints = points
        .iter()
        .map(|&point| sim_types::RallyTarget::Point(battle_harness::sim_position(point)))
        .chain([field])
        .collect();
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
    // Plans `2026-11-23-production-queue` D-10 and `2026-11-25-rallies` D-8: the selected base's
    // three-waypoint rally is drawn from the pool, which was filled before the window, so the
    // census above holds with it shown.
    let world = harness.app.world_mut();
    let shown = world
        .query::<(
            &client::battle_input::rally_marker::RallyMarker,
            &Visibility,
        )>()
        .iter(world)
        .filter(|(_, visibility)| **visibility != Visibility::Hidden)
        .count();
    assert_eq!(shown, 6, "the base's three rally reticles and three lines");
}

/// Places a completed extractor that hides its harvesters, of the local player, over the gas
/// node nearest the first worker, and orders that worker to gather from it through the battle's
/// command queue; returns the worker.
fn send_worker_to_gas(harness: &mut Harness) -> EntityId {
    let worker = local_workers(harness)[0];
    let at = sim_types::SimVec2::new(
        sim_types::SimScalar::from_num(harness.simulated_position(worker).x),
        sim_types::SimScalar::from_num(harness.simulated_position(worker).y),
    );
    let world = harness.world();
    let extractor = world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.is_gas_extractor && a.capabilities.hides_harvesters)
        .expect("the catalog ships an extractor that hides its harvesters")
        .unit_type_id;
    let mut nodes: Vec<(sim_types::SimScalar, usize, (u32, u32))> = world
        .map
        .resources()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == sim_map::ResourceKind::Gas)
        .map(|(index, node)| (node.pos.distance_sq(at), index, node.cell))
        .collect();
    nodes.sort_unstable();
    let index = nodes
        .into_iter()
        .find(|(_, _, cell)| {
            harness
                .world_mut()
                .spawn_structure(extractor, LOCAL_PLAYER, cell.0 - 1, cell.1 - 1)
                .is_ok()
        })
        .map(|(_, index, _)| index)
        .expect("a gas node near home takes an extractor");
    let target = sim_types::ResourceNodeId(u32::try_from(index).expect("few nodes"));
    let mut session = harness.app.world_mut().resource_mut::<BattleSession>();
    let (world, queue) = session.world_and_commands();
    queue
        .submit_selection(
            world.expect("committed battle"),
            LOCAL_PLAYER,
            &[worker],
            sim_types::UnitOrder::Harvest { target },
            false,
        )
        .expect("the client admits a harvest order");
    worker
}

/// Steps, counting motion, until `worker` has gone inside its extractor and come out again.
/// Returns how often its presence changed (twice): on each such frame the stance look writes the
/// bubble's shell once (`effects::stance`), as it does for a unit that cloaks.
fn through_a_gas_trip(harness: &mut Harness, worker: EntityId, motion: &mut Motion) -> u64 {
    let housed = |harness: &Harness| harness.world().entities[&worker].is_housed();
    let mut frames = 0;
    let mut changes = 0;
    for inside in [true, false] {
        while housed(harness) != inside {
            step_counting(harness, motion);
            frames += 1;
            assert!(
                frames < READY_FRAMES * 4,
                "the worker never went in and out"
            );
        }
        changes += 1;
    }
    changes
}

/// Plan `2026-11-24-hidden-harvesters` P-10: a gas worker that goes inside its extractor and
/// comes out spawns and despawns nothing, its bubble keeps its render entity, no asset other
/// than the materials of redrawn bubbles is touched, and no retained buffer moves or grows. The
/// window starts after one whole trip, so the first use of any buffer is outside it.
#[test]
fn a_gas_trip_in_and_out_of_the_extractor_spawns_nothing() {
    let mut harness = skirmish();
    settle(&mut harness, true);
    let worker = send_worker_to_gas(&mut harness);
    harness.resume();
    let mut warm = Motion::default();
    through_a_gas_trip(&mut harness, worker, &mut warm);
    run_ticks(&mut harness, WARM_TICKS);
    let render = harness
        .app
        .world()
        .resource::<client::RenderEntityMap>()
        .get(worker);
    let entities = census(&harness);
    let buffers = retained_buffers(harness.app.world());
    reset(&mut harness);
    let mut motion = Motion::default();
    let presence_changes = through_a_gas_trip(&mut harness, worker, &mut motion);
    assert_eq!(
        harness
            .app
            .world()
            .resource::<client::RenderEntityMap>()
            .get(worker),
        render,
        "one render entity through the trip"
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
        modified <= motion.redrawn + presence_changes,
        "{modified} bubble materials modified; (unit, frame) pairs: {motion:?}, and \
         {presence_changes} presence changes"
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

/// Every swarm bubble's heading, by entity, and the materials the swarm draws with.
fn swarm_state(
    harness: &mut Harness,
) -> (BTreeMap<Entity, u32>, BTreeSet<AssetId<BubbleDomeMaterial>>) {
    let world = harness.app.world_mut();
    let mut headings = BTreeMap::new();
    let mut materials = BTreeSet::new();
    for (entity, bubble, material) in world
        .query_filtered::<(
            Entity,
            &client::BubbleInstance,
            &MeshMaterial3d<BubbleDomeMaterial>,
        ), With<SwarmBubble>>()
        .iter(world)
    {
        headings.insert(entity, bubble.0.heading_angle_rad.to_bits());
        materials.insert(material.0.id());
    }
    (headings, materials)
}

/// D-9: the swarm walks on the shared flipbook clock, so its shared materials are never rewritten
/// for time passing, and a wall bounce swaps a bubble to its group's material for the new heading
/// instead of rewriting the one the group shares. The window is long enough for every bubble to
/// bounce.
///
/// The one modification a material may take is its first: the optics system
/// (`bubble_optics::sync_optics`) fills the optics lanes of a material the first frame an entity
/// draws with it, and a group's second heading is first drawn at the group's first bounce.
#[test]
fn swarm_stress_frames_create_modify_and_remove_nothing() {
    let mut harness = Harness::boot(ClientMode::SwarmStressTest);
    settle(&mut harness, false);
    let entities = census(&harness);
    let buffers = retained_buffers(harness.app.world());
    let mut flips = 0_usize;
    let (mut headings, drawn_before) = swarm_state(&mut harness);
    let mut drawn = drawn_before.clone();
    for _ in 0..SWARM_FRAMES {
        harness.step(1);
        let (next, materials) = swarm_state(&mut harness);
        flips += next
            .iter()
            .filter(|(entity, heading)| headings.get(entity) != Some(heading))
            .count();
        headings = next;
        drawn.extend(materials);
    }
    let first_drawn = drawn.difference(&drawn_before).count();
    assert!(
        harness
            .app
            .world()
            .resource::<client::LabState>()
            .swarm_bubble_count
            > 0,
        "the swarm must be on the field"
    );
    assert!(
        flips >= SWARM_BUBBLES,
        "every bubble must bounce in the window: {flips} heading flips"
    );
    assert_eq!(
        drawn.len(),
        SWARM_MATERIALS,
        "the swarm draws with one material per group and heading"
    );
    let bubbles = type_name::<Assets<BubbleDomeMaterial>>();
    let tally = tallies(&harness);
    let modified = tally.count(bubbles, EventKind::Modified);
    eprintln!(
        "swarm window: {SWARM_FRAMES} frames, {flips} heading flips, {} materials drawn, \
         {first_drawn} first drawn in the window, {modified} material modifications",
        drawn.len()
    );
    assert_eq!(
        tally.others(&[(bubbles, EventKind::Modified)]),
        Vec::new(),
        "asset events other than bubble material modifications"
    );
    assert!(
        modified <= first_drawn as u64,
        "{modified} bubble materials modified over {SWARM_FRAMES} frames with {flips} heading \
         flips; only the {first_drawn} materials first drawn in the window may take their optics"
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
///
/// The battle's pace follows the catalog (unit speeds decide when each wave meets), so the test
/// does not assume steady combat at a fixed frame: it judges consecutive windows of
/// [`COMBAT_FRAMES`] after the warm-up, every one of them for the material rules, until one holds
/// at least [`MIN_COMBAT_EVENTS`] combat events, and checks slot reuse on that one.
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
    for window in 1..=MAX_COMBAT_WINDOWS {
        reset(&mut harness);
        let created_before = pool_created(&harness);
        harness.step(COMBAT_FRAMES);
        let grown = pool_created(&harness) - created_before;
        let tally = tallies(&harness);
        let materials = type_name::<Assets<CombatFxMaterial>>();
        let added = tally.count(materials, EventKind::Added);
        eprintln!(
            "combat window {window}: {} combat events, {} effects spawned, {added} materials \
             added, pool grew by {grown}",
            tally.combat_events, tally.effects_spawned
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
        if tally.combat_events >= MIN_COMBAT_EVENTS {
            // Plan WP11 (Amendment 24), re-baselined with that cause only: every shipped unit
            // names its weapons' looks, so the effect system draws the battle's shots and deaths
            // and the legacy cards spawn none; the pool then adds no material at all.
            assert!(
                added == 0 || added < tally.effects_spawned,
                "{added} effect materials added for {} effects spawned: slots are not reused",
                tally.effects_spawned
            );
            return;
        }
    }
    panic!(
        "no window of {COMBAT_FRAMES} frames in the first {MAX_COMBAT_WINDOWS} held steady combat \
         ({MIN_COMBAT_EVENTS} combat events)"
    );
}

/// Effects every impact and splash of the default battle draws in the effect-stream window:
/// a card, a ring, a decal, a light and a particle burst.
const STREAM_EFFECTS: &str = r#"
    "spark": (class: Minor, event: Point, layers: [
        (block: Sprite(texture: "flip", ramp: "red_add"), size: (scale: 0.4), duration: 0.3,
         priority: Critical),
        (block: Ring(texture: "solid", ramp: "green", orient: Ground, thickness: 0.3),
         radius: (from: (scale: 0.2), to: (scale: 0.8)), duration: 0.25),
        (block: Decal(texture: "soft_disc", ramp: "blue"), size: (scale: 0.5), duration: 1.5),
        (block: Light(color: (1.0, 0.6, 0.3), intensity: 400.0), size: (scale: 2.0),
         duration: 0.2, curve: "fade"),
        (block: Particles(emitter: "chips")),
    ]),
    "burst": (class: Major, event: Splash, layers: [
        (block: Sprite(texture: "soft_disc", ramp: "white"), size: (bind: SplashReach, scale: 1.0),
         duration: 0.5, priority: Critical),
    ]),
    "beam": (class: Minor, event: Shot, sustain: Some((grace: 0.12)), layers: [
        (block: Ribbon(texture: "solid", ramp: "red_add", shape: Radial), size: (scale: 0.05),
         duration: 0.04, held: true, exit: 0.08, priority: Critical),
    ]),
    "bolt": (class: Minor, event: Shot, layers: [
        (block: Sprite(texture: "soft_disc", ramp: "white", ride: true, stretch: 2.0),
         size: (scale: 0.1), duration: 0.2, lasts: FlightTime, priority: Critical),
        (block: Ribbon(texture: "solid", ramp: "red_add", shape: Radial, trail: Some(0.5)),
         size: (scale: 0.03), duration: 0.2, lasts: FlightTime),
    ])
"#;

/// The emitter of [`STREAM_EFFECTS`].
const STREAM_EMITTERS: &str = r#""chips": (spawn: Burst(count: (base: 6, max: 6)), speed: (1.0, 2.0),
    cone_deg: 45.0, gravity: 9.0, lifetime: (0.2, 0.5), size: (0.03, 0.06), texture: "soft_disc",
    orient: VelocityStretched(1.0), ramp: "red_add")"#;

/// Plan `2026-11-09-effect-visualization` WP4 (D-7): steady combat with an effect on every
/// impact and splash keeps every effect buffer at its address and capacity and keeps the
/// pooled light entities (the effects draw from retained GPU buffers, never assets or entities).
#[test]
fn combat_frames_with_an_effect_stream_reuse_every_effect_buffer() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let manifest = effect_fixtures::write_fixture_set(directory.path());
    let vocabulary = directory.path().join("effects.ron");
    std::fs::write(
        &vocabulary,
        effect_fixtures::vocabulary(
            &manifest,
            STREAM_EMITTERS,
            STREAM_EFFECTS,
            r#"impact: "spark", splash: "burst""#,
        ),
    )
    .expect("the vocabulary is written");
    let mut harness = Harness::boot_root_with(
        &battle_harness::assets(),
        ClientMode::LiveBattle,
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO,
        |app| {
            app.insert_resource(EffectCatalogSource(vocabulary.clone()));
            // The shipped presentation blocks name shipped effects this test vocabulary does
            // not define; the stream comes from its defaults instead, and every weapon shoots
            // the test's `bolt` along its path (plan WP11: a shot's travel in steady combat).
            if let Some(mut units) = app
                .world_mut()
                .get_resource_mut::<client::CatalogResource>()
            {
                let ids: Vec<_> = units
                    .0
                    .archetypes()
                    .map(|archetype| archetype.unit_type_id)
                    .collect();
                // An air weapon's travel is a sustained beam, held while a beam weapon keeps
                // striking (plan `2026-11-17-effect-snapshot-identity` WP7).
                let shot = |weapon| sim_core::catalog::WeaponEffects {
                    weapon,
                    mode: None,
                    launch: None,
                    travel: Some(
                        if weapon == sim_types::WeaponSlot::Air {
                            "beam"
                        } else {
                            "bolt"
                        }
                        .to_owned(),
                    ),
                    impact: Some("spark".to_owned()),
                    splash: Some("burst".to_owned()),
                };
                for id in ids {
                    if let Ok(archetype) = units.0.get_mut(id) {
                        archetype.effects = sim_core::catalog::ArchetypeEffects {
                            weapons: vec![
                                shot(sim_types::WeaponSlot::Primary),
                                shot(sim_types::WeaponSlot::Ground),
                                shot(sim_types::WeaponSlot::Air),
                            ],
                            ..sim_core::catalog::ArchetypeEffects::default()
                        };
                    }
                }
            }
        },
    );
    harness.frame_army();
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
    let lights = harness
        .app
        .world()
        .resource::<EffectLightPool>()
        .lights
        .clone();
    for window in 1..=MAX_COMBAT_WINDOWS {
        reset(&mut harness);
        let spawned_before = harness
            .app
            .world()
            .resource::<EffectRuntime>()
            .stats
            .spawned;
        let runtime = harness.app.world().resource::<EffectRuntime>();
        let buffers = (
            runtime.frame.buffers(),
            runtime.decal_table().buffers(),
            runtime.index_buffers(),
        );
        harness.step(COMBAT_FRAMES);
        let runtime = harness.app.world().resource::<EffectRuntime>();
        let spawned = runtime.stats.spawned - spawned_before;
        eprintln!(
            "effect window {window}: {} combat events, {spawned} effects spawned, {} live, {} particles",
            tallies(&harness).combat_events,
            runtime.pool().live_total(),
            runtime.particles.len()
        );
        assert_eq!(
            (
                runtime.frame.buffers(),
                runtime.decal_table().buffers(),
                runtime.index_buffers()
            ),
            buffers,
            "an effect buffer, the decal table or a spawn-path index moved or grew"
        );
        assert_eq!(
            harness.app.world().resource::<EffectLightPool>().lights,
            lights,
            "the light pool's entities changed"
        );
        for entity in &lights {
            assert!(
                harness.app.world().get_entity(*entity).is_ok(),
                "a pooled light was despawned"
            );
        }
        if spawned >= MIN_COMBAT_EVENTS {
            return;
        }
    }
    panic!("no window of {COMBAT_FRAMES} frames spawned {MIN_COMBAT_EVENTS} effects");
}

// --- Attack snapshots (plan 2026-11-24-attacker-snapshot P-16) -----------------------------------

/// The local view's attack snapshots, as `(attacker, strike tick)`.
fn snapshots_listed(harness: &Harness) -> Vec<(EntityId, sim_types::Tick)> {
    harness
        .app
        .world()
        .resource::<HudModel>()
        .view
        .attack_snapshots
        .iter()
        .map(|entry| (entry.entity, entry.struck))
        .collect()
}

/// Stages an unseen attacker on the settled skirmish and runs until the local view lists its
/// snapshot.
fn hold_a_snapshot(harness: &mut Harness) -> attack_snapshot_stage::SnapshotStage {
    let staged = attack_snapshot_stage::stage(harness.world_mut());
    harness.resume();
    let mut frames = 0;
    while snapshots_listed(harness).is_empty() {
        harness.step(1);
        frames += 1;
        assert!(frames < 2_000, "the attacker never struck unseen");
    }
    staged
}

/// The pooled snapshot entities.
fn pooled(harness: &Harness) -> usize {
    harness
        .app
        .world()
        .resource::<AttackSnapshotPool>()
        .entities()
        .count()
}

/// P-16: with a snapshot held and its list unchanged, running frames create, modify and remove
/// no asset, spawn nothing, and keep the fog's and the pool's buffers. The catalog's snapshot
/// cap (1 second, plan Amendment 1) is lifted, so the image no strike refreshes is held through
/// the windows: this test measures a held image, not its lifetime, which `sim_core`'s
/// `attack_snapshot_tests` pins.
#[test]
fn a_held_attack_snapshot_changes_nothing_on_running_frames() {
    let mut harness = skirmish();
    settle(&mut harness, true);
    harness.world_mut().catalog.set_attack_snapshot_cap(None);
    let staged = hold_a_snapshot(&mut harness);
    // No more strikes: the victim leaves every weapon's filters.
    harness
        .world_mut()
        .entities
        .get_mut(&staged.victim)
        .expect("the victim lives")
        .is_invulnerable = true;
    run_ticks(&mut harness, WARM_TICKS);
    let held = snapshots_listed(&harness);
    assert_eq!(held.len(), 1, "the snapshot is held");
    assert_eq!(pooled(&harness), 1);
    let entities = census(&harness);
    let buffers = retained_buffers(harness.app.world());
    reset(&mut harness);
    run_ticks(&mut harness, MOVING_TICKS);
    assert_eq!(snapshots_listed(&harness), held, "the list is unchanged");
    assert_eq!(tallies(&harness).others(&[]), Vec::new(), "asset events");
    assert_eq!(census(&harness), entities, "entities spawned or despawned");
    assert_eq!(
        moved(&buffers, &retained_buffers(harness.app.world())),
        Vec::new(),
        "retained buffers that moved or grew"
    );
}

/// P-16: a snapshot refreshed by later strikes of the same attacker spawns no snapshot entity
/// (the strikes' own effects may spawn theirs). Under the shipped cap: each strike comes before
/// the cap ends the image.
#[test]
fn a_refreshed_attack_snapshot_spawns_nothing() {
    let mut harness = skirmish();
    settle(&mut harness, true);
    hold_a_snapshot(&mut harness);
    harness.step(1);
    let first = snapshots_listed(&harness);
    let snapshot_entities = |harness: &mut Harness| {
        let world = harness.app.world_mut();
        world
            .query_filtered::<Entity, With<AttackSnapshotBubble>>()
            .iter(world)
            .count()
    };
    assert_eq!(snapshot_entities(&mut harness), 1);
    run_ticks(&mut harness, MOVING_TICKS);
    let later = snapshots_listed(&harness);
    assert_eq!(later.len(), 1, "one snapshot");
    assert!(
        later[0].1 > first[0].1,
        "premise: later strikes refreshed it"
    );
    assert_eq!(pooled(&harness), 1, "the pool did not grow");
    assert_eq!(
        snapshot_entities(&mut harness),
        1,
        "no snapshot entity spawned"
    );
}

// --- Launch and spell images (plan 2026-11-27-launch-and-spell-visibility V-25) ----------------

/// Asserts that, with the one listed image held unchanged, running frames create, modify and
/// remove no asset, spawn nothing, and keep the fog's and the pool's buffers (P-16's window).
fn assert_held_image_is_steady(harness: &mut Harness) {
    run_ticks(harness, WARM_TICKS);
    let held = snapshots_listed(harness);
    assert_eq!(held.len(), 1, "the image is held");
    assert_eq!(pooled(harness), 1);
    let entities = census(harness);
    let buffers = retained_buffers(harness.app.world());
    reset(harness);
    run_ticks(harness, MOVING_TICKS);
    assert_eq!(snapshots_listed(harness), held, "the list is unchanged");
    assert_eq!(tallies(harness).others(&[]), Vec::new(), "asset events");
    assert_eq!(census(harness), entities, "entities spawned or despawned");
    assert_eq!(
        moved(&buffers, &retained_buffers(harness.app.world())),
        Vec::new(),
        "retained buffers that moved or grew"
    );
}

/// V-25: a missile's launch image held unchanged (its target then made invulnerable, so the
/// missile lands on nothing and no strike refreshes it; the cap lifted) is as steady as a
/// strike's: running frames create no asset event, spawn nothing and keep every buffer.
#[test]
fn a_held_launch_image_changes_nothing_on_running_frames() {
    let mut harness = skirmish();
    settle(&mut harness, true);
    harness.world_mut().catalog.set_attack_snapshot_cap(None);
    let staged = attack_snapshot_stage::stage_missile(harness.world_mut());
    harness.resume();
    let mut frames = 0;
    while snapshots_listed(&harness).is_empty() {
        harness.step(1);
        frames += 1;
        assert!(frames < 2_000, "the missile soldier never released unseen");
    }
    let released = snapshots_listed(&harness);
    assert!(
        harness
            .world()
            .projectiles
            .iter()
            .any(|missile| missile.attacker_id == Some(staged.attacker)),
        "listed at the release, while its missile flies"
    );
    harness
        .world_mut()
        .entities
        .get_mut(&staged.victim)
        .expect("the victim lives")
        .is_invulnerable = true;
    // The attacker holds, so it neither chases nor turns once its target is gone.
    harness
        .world_mut()
        .entities
        .get_mut(&staged.attacker)
        .expect("the attacker lives")
        .issue_order(sim_types::UnitOrder::HoldPosition, false)
        .expect("a hold is admitted");
    run_ticks(&mut harness, 400);
    assert_held_image_is_steady(&mut harness);
    assert_eq!(
        snapshots_listed(&harness),
        released,
        "the launch's own image"
    );
}

/// V-25: a storm caster's image held unchanged (the storm's pulses show nothing, and the window
/// opens after the storm has ended; the cap lifted) is as steady as a strike's.
#[test]
fn a_held_spell_image_changes_nothing_on_running_frames() {
    let mut harness = skirmish();
    settle(&mut harness, true);
    harness.world_mut().catalog.set_attack_snapshot_cap(None);
    let staged = attack_snapshot_stage::stage_caster(harness.world_mut());
    harness.resume();
    run_ticks(&mut harness, 2);
    attack_snapshot_stage::cast_storm(harness.world_mut(), &staged);
    let cast = snapshots_listed(&harness);
    let mut frames = 0;
    while !harness.world().effects().persistents().is_empty()
        || !harness
            .world()
            .effects()
            .statuses_of(staged.victim)
            .is_empty()
    {
        harness.step(1);
        frames += 1;
        assert!(frames < 2_000, "the storm ended");
    }
    let listed = snapshots_listed(&harness);
    assert_eq!(listed.len(), 1, "the caster's image outlived the storm");
    assert_eq!(listed[0].0, staged.attacker);
    assert!(
        cast.is_empty() || cast == listed,
        "the pulses refreshed nothing"
    );
    assert_held_image_is_steady(&mut harness);
}

// --- Units that come and go (plan 2026-11-29-render-entity-pool R-1, R-2) ----------------------

/// How the render pool was used in the session.
fn pool_stats(harness: &Harness) -> client::RenderPoolStats {
    harness
        .app
        .world()
        .resource::<client::RenderEntityMap>()
        .stats()
}

/// Render entities the pool has spawned in the session.
fn pool_spawned(harness: &Harness) -> u32 {
    pool_stats(harness).spawned
}

/// What a cycling window saw, counted from consecutive presented snapshots.
#[derive(Debug, Clone, Copy, Default)]
struct Membership {
    /// Records that entered the snapshot, of any kind.
    arrivals: u64,
    /// Records that left it, of any kind.
    departures: u64,
    /// Arrivals of the kind under test.
    kind_arrivals: u64,
    /// Departures of the kind under test.
    kind_departures: u64,
}

/// Steps one frame, adding the frame's redrawn bubbles to `motion` and its arrivals and
/// departures to `membership`; `kind` tells the records of the kind under test.
fn step_membership(
    harness: &mut Harness,
    motion: &mut Motion,
    membership: &mut Membership,
    kind: &dyn Fn(&BubbleRenderInstance) -> bool,
) {
    let before = presented(harness);
    harness.step(1);
    let after = presented(harness);
    let registry = harness.app.world().resource::<FlipbookRegistry>();
    motion.turned += turned(&before, &after);
    motion.redrawn += redrawn(&before, &after, registry);
    let ids = |presented: &Presented| -> BTreeMap<EntityId, bool> {
        presented
            .snapshot
            .bubbles
            .iter()
            .map(|bubble| (bubble.entity_id, kind(bubble)))
            .collect()
    };
    let (was, is) = (ids(&before), ids(&after));
    for (id, of_kind) in &is {
        if !was.contains_key(id) {
            membership.arrivals += 1;
            membership.kind_arrivals += u64::from(*of_kind);
        }
    }
    for (id, of_kind) in &was {
        if !is.contains_key(id) {
            membership.departures += 1;
            membership.kind_departures += u64::from(*of_kind);
        }
    }
}

/// Runs the cycling battle until the pool's spawn count has not changed for `2 × period` ticks
/// (the regime: every arrival finds a parked entity), failing after `10 × period` ticks (stop
/// rule 6). `drive` runs before every frame.
fn warm_to_regime(harness: &mut Harness, period: u32, drive: &mut dyn FnMut(&mut Harness)) {
    let start = harness.world().current_tick;
    let mut since = start;
    let mut spawned = pool_spawned(harness);
    loop {
        drive(harness);
        harness.step(1);
        let tick = harness.world().current_tick;
        let now = pool_spawned(harness);
        if now != spawned {
            spawned = now;
            since = tick;
        }
        if tick - since >= 2 * period {
            return;
        }
        assert!(
            tick - start < 10 * period,
            "the pool kept spawning for {} ticks (10 × {period}): {spawned} spawned",
            tick - start
        );
    }
}

/// The window of a cycling test: `3 × period` ticks after [`warm_to_regime`], asserting R-1's
/// rules (plan §6.1): no asset added or removed, only the bubble, ring and standard materials of
/// redrawn or arriving units modified, no entity spawned or despawned, no retained buffer moved,
/// and at least two arrivals and two departures of the kind under test.
fn assert_cycling_window_is_steady(
    harness: &mut Harness,
    period: u32,
    drive: &mut dyn FnMut(&mut Harness),
    kind: &dyn Fn(&BubbleRenderInstance) -> bool,
) {
    let entities = census(harness);
    let buffers = retained_buffers(harness.app.world());
    let before = pool_stats(harness);
    reset(harness);
    let mut motion = Motion::default();
    let mut membership = Membership::default();
    let goal = harness.world().current_tick + 3 * period;
    while harness.world().current_tick < goal {
        drive(harness);
        step_membership(harness, &mut motion, &mut membership, kind);
    }
    assert!(
        membership.kind_arrivals >= 2 && membership.kind_departures >= 2,
        "units of the kind came and went in the window: {membership:?}"
    );
    let tally = tallies(harness);
    let bubbles = type_name::<Assets<BubbleDomeMaterial>>();
    let rings = type_name::<Assets<SelectionRingMaterial>>();
    let standard = type_name::<Assets<StandardMaterial>>();
    assert_eq!(
        tally.others(&[
            (bubbles, EventKind::Modified),
            (rings, EventKind::Modified),
            (standard, EventKind::Modified),
        ]),
        Vec::new(),
        "asset events other than modified bubble, ring and standard materials \
         ({membership:?}, {motion:?})"
    );
    let arrivals = membership.arrivals;
    let modified = tally.count(bubbles, EventKind::Modified);
    assert!(
        modified <= motion.redrawn + arrivals,
        "{modified} bubble materials modified; (unit, frame) pairs: {motion:?}, {arrivals} \
         arrivals"
    );
    for asset in [rings, standard] {
        let modified = tally.count(asset, EventKind::Modified);
        assert!(
            modified <= arrivals,
            "{modified} of {asset} modified for {arrivals} arrivals"
        );
    }
    assert_eq!(census(harness), entities, "entities spawned or despawned");
    assert_eq!(
        moved(&buffers, &retained_buffers(harness.app.world())),
        Vec::new(),
        "retained buffers that moved or grew"
    );
    let after = pool_stats(harness);
    assert_eq!(
        (after.spawned, u64::from(after.reused - before.reused)),
        (before.spawned, arrivals),
        "every arrival of the window reused a parked entity ({before:?} then {after:?})"
    );
}

/// R-1 = B-19 (plan `2026-11-28-broodlings` D-12 item 3, plan `2026-11-29-render-entity-pool`
/// §6.1): a running battle in which the only fight is a Brood Lord analogue (found by its strike
/// magazine) attacking a durable, unarmed enemy, so Broodlings are created and expire on their
/// own clocks, creates and removes no asset and spawns no entity once the render pool has
/// reached its regime: the render entities of the units that come and go are reused. Warm-up
/// and window are multiples of the Broodling's longest life, read from the data.
#[test]
fn broodlings_cycling_create_and_remove_no_bubble_material() {
    let mut harness = Harness::battle();
    settle(&mut harness, true);
    let stage = unit_cycling::stage_brood(harness.world_mut());
    harness.resume();
    let mut drive = |_: &mut Harness| {};
    warm_to_regime(&mut harness, stage.lifetime, &mut drive);
    let spawned = stage.spawned;
    assert_cycling_window_is_steady(&mut harness, stage.lifetime, &mut drive, &|bubble| {
        bubble.unit_type == spawned
    });
}

/// R-2 (plan `2026-11-29-render-entity-pool` §6.1): an unarmed air unit with media of each
/// player is killed and spawned again at a fixed cadence, and every other cycle a ground unit
/// replaces it on the same tick, so render entities cycle through air and ground arrivals and
/// across the two layouts; after the regime, the same rules as R-1 hold.
#[test]
fn air_units_cycling_create_and_remove_nothing() {
    let mut harness = Harness::battle();
    settle(&mut harness, true);
    let assets = battle_harness::assets();
    let mut cycle = unit_cycling::stage_air_cycle(harness.world_mut(), &assets);
    harness.resume();
    let mut last_tick = harness.world().current_tick;
    let (air, ground) = (cycle.air, cycle.ground);
    let mut drive = move |harness: &mut Harness| {
        cycle.advance(harness.world_mut(), &mut last_tick);
    };
    let period = 2 * unit_cycling::AIR_CYCLE_TICKS;
    warm_to_regime(&mut harness, period, &mut drive);
    assert_cycling_window_is_steady(&mut harness, period, &mut drive, &|bubble| {
        bubble.unit_type == air || bubble.unit_type == ground
    });
}

/// Ticks a hangar test drives the battle to complete its warm-up cycles before giving up.
const HANGAR_CYCLE_LIMIT_TICKS: u32 = 20_000;

/// Least window of H-20, in ticks (plan `2026-11-28-carrier-interceptors` §6.4: 600 ticks).
const HANGAR_WINDOW_TICKS: u32 = 600;

/// H-20 (plan `2026-11-28-carrier-interceptors` §6.4, plan `2026-11-29-render-entity-pool`
/// §6.5): two hangar units (the Carrier analogue, found by its hangar) of the local player cycle
/// their members through launch, attack, dock and relaunch against a durable unarmed enemy, the
/// cycle driven by the presented snapshot (every member launched, then the target gone until
/// every member has docked, then a new target). After the render pool reaches its regime
/// (warm-up and window are multiples of the time one cycle took, measured), a window of at least
/// 600 ticks creates and removes no asset and spawns no entity, with R-1's rules.
#[test]
fn hangar_members_cycling_create_and_remove_nothing() {
    let mut harness = Harness::battle();
    settle(&mut harness, true);
    let assets = battle_harness::assets();
    let stage = unit_cycling::stage_hangars(harness.world_mut(), &assets);
    let cycle = std::cell::Cell::new(stage);
    harness.resume();
    let mut drive = |harness: &mut Harness| {
        let mut state = cycle.get();
        let members = state.members_in(
            &harness
                .app
                .world()
                .resource::<BattleSession>()
                .frames
                .presented()
                .bubbles,
        );
        state.advance(harness.world_mut(), members);
        cycle.set(state);
    };
    let start = harness.world().current_tick;
    while cycle.get().cycles < 2 {
        drive(&mut harness);
        harness.step(1);
        assert!(
            harness.world().current_tick - start < HANGAR_CYCLE_LIMIT_TICKS,
            "two hangar cycles within {HANGAR_CYCLE_LIMIT_TICKS} ticks: {:?}",
            cycle.get()
        );
    }
    let period = cycle.get().last_cycle_ticks;
    assert!(period > 0, "a cycle takes time");
    warm_to_regime(&mut harness, period, &mut drive);
    let (member, target) = (stage.member, stage.target_kind);
    let window = period.max(HANGAR_WINDOW_TICKS.div_ceil(3));
    assert_cycling_window_is_steady(&mut harness, window, &mut drive, &|bubble| {
        bubble.unit_type == member || bubble.unit_type == target
    });
    assert!(
        cycle.get().cycles >= 3,
        "the window ran whole cycles: {:?}",
        cycle.get()
    );
    eprintln!(
        "H-20: one cycle took {period} ticks at warm-up; window {} ticks; {} cycles in all; \
         pool {:?}",
        3 * window,
        cycle.get().cycles,
        pool_stats(&harness)
    );
}

// --- A mixed right click (plan 2026-12-01-rallies-and-smart-command P-6) ------------------------

/// Frames of the window after a mixed right click.
const SMART_MIXED_FRAMES: usize = 60;

/// Least distance, in world units, from each soldier to the transport they board: their walk
/// outlasts the warm-up and the window.
const MIXED_CARRIER_GAP: f32 = 15.0;

/// A presentable mobile unit type that `wanted` accepts, chosen by capability and shipped media.
fn presentable_type(
    harness: &Harness,
    wanted: impl Fn(&sim_core::UnitArchetype) -> bool,
) -> sim_types::UnitTypeId {
    let assets = battle_harness::assets();
    harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            !archetype.is_immobile_by_default()
                && battle_harness::presentable(&assets, &archetype.slug)
                && wanted(archetype)
        })
        .map(|archetype| archetype.unit_type_id)
        .expect("the catalog ships a presentable unit of this role")
}

/// The group of the mixed click: two ground soldiers and an armed air unit beside the local
/// workers, and the transport well away from them, so the walk to board lasts past the window.
struct MixedGroup {
    soldiers: [EntityId; 2],
    flyer: EntityId,
    carrier: EntityId,
}

/// Spawns the [`MixedGroup`] for the local player, before the scene settles.
fn stage_mixed_group(harness: &mut Harness) -> MixedGroup {
    let origin = harness.simulated_position(local_workers(harness)[0]);
    let soldier_type = presentable_type(harness, |a| {
        a.mover_type == sim_types::MoverType::Ground
            && a.capabilities.harvester.is_none()
            && !a.capabilities.can_construct
            && a.capabilities.transport_capacity == 0
    });
    let carrier_type = presentable_type(harness, |a| a.capabilities.transport_capacity > 0);
    let flyer_type = presentable_type(harness, |a| {
        a.mover_type == sim_types::MoverType::Air
            && a.capabilities.transport_capacity == 0
            && client::battle_pipeline::commands::is_armed(a)
    });
    // Each body on its own open ground, clear of structures and cliffs, and the transport at
    // least `MIXED_CARRIER_GAP` from both soldiers.
    let mut spots = [5.0, 6.5, 8.0]
        .map(|distance| harness.open_ground(origin, distance))
        .to_vec();
    let far = [16.0, 20.0, 24.0, 28.0, 32.0]
        .into_iter()
        .map(|distance| harness.open_ground(spots[0], distance))
        .find(|at| {
            spots[..2]
                .iter()
                .all(|soldier| soldier.distance(*at) >= MIXED_CARRIER_GAP)
        })
        .expect("open ground far from the soldiers");
    spots.push(far);
    let mut spawn = |unit_type, at: Vec2| {
        harness
            .world_mut()
            .spawn_unit(unit_type, LOCAL_PLAYER, battle_harness::sim_position(at))
            .expect("open ground for the group")
    };
    let group = MixedGroup {
        soldiers: [spawn(soldier_type, spots[0]), spawn(soldier_type, spots[1])],
        flyer: spawn(flyer_type, spots[2]),
        carrier: spawn(carrier_type, spots[3]),
    };
    // The skirmish opens paused: a few running frames publish the new bubbles.
    harness.resume();
    harness.step(3);
    harness.pause();
    group
}

/// The marker pool's entity count.
fn marker_pool(harness: &mut Harness) -> usize {
    let world = harness.app.world_mut();
    world
        .query::<&client::battle_input::order_marker::OrderMarker>()
        .iter(world)
        .count()
}

/// P-6: a selection of two soldiers, an armed air unit and the base right-clicks the transport:
/// the soldiers board it, the air unit follows it and the base rallies to it, three commands in
/// one tick (D-1 gives at most two unit entries on any shipped target, so the third is the
/// rally). After the marker has shown, [`SMART_MIXED_FRAMES`] running frames, in which the
/// boarders walk and the follower flies, add, modify (beyond the bubble materials of redrawn
/// units) and remove no asset, spawn and despawn no entity, keep every retained buffer, and keep
/// the order marker pool at its size.
#[test]
fn frames_after_a_mixed_right_click_create_modify_and_remove_nothing() {
    let mut harness = skirmish();
    let group = stage_mixed_group(&mut harness);
    settle(&mut harness, true);
    let base = harness.local_structure();
    let members = [group.soldiers[0], group.soldiers[1], group.flyer, base];
    harness.look_at(harness.simulated_position(group.soldiers[0]));
    harness.select(members[0]);
    for member in &members[1..] {
        harness.look_at(harness.simulated_position(*member));
        let at = harness.unit_on_screen(*member);
        harness.with_key_held(KeyCode::ShiftLeft, |harness| {
            harness.click(MouseButton::Left, at);
        });
    }
    assert_eq!(
        harness.selection().ids().len(),
        members.len(),
        "the group is selected"
    );
    harness.look_at(harness.simulated_position(group.carrier));
    let at = harness.unit_on_screen(group.carrier);
    harness.click(MouseButton::Right, at);
    assert_eq!(
        harness.pending_orders(),
        3,
        "board, follow and rally: {}",
        harness.status()
    );
    harness.look_at(harness.simulated_position(group.soldiers[0]));
    harness.resume();
    run_ticks(&mut harness, WARM_TICKS);

    let pool = marker_pool(&mut harness);
    let entities = census(&harness);
    let buffers = retained_buffers(harness.app.world());
    let before: Vec<_> = members[..3]
        .iter()
        .map(|unit| harness.world().entities[unit].position)
        .collect();
    reset(&mut harness);
    let mut motion = Motion::default();
    for _ in 0..SMART_MIXED_FRAMES {
        step_counting(&mut harness, &mut motion);
    }

    let world = harness.world();
    for (unit, start) in members[..3].iter().zip(&before) {
        let entity = &world.entities[unit];
        assert_ne!(entity.position, *start, "{unit:?} moved in the window");
        assert!(
            !entity.is_loaded_in_transport(),
            "{unit:?} still on its way"
        );
    }
    for soldier in group.soldiers {
        assert_eq!(
            world.entities[&soldier].order_queue.front(),
            Some(&sim_types::UnitOrder::Load {
                carrier: group.carrier
            })
        );
    }
    assert!(matches!(
        world.entities[&group.flyer].order_queue.front(),
        Some(sim_types::UnitOrder::Follow { target, .. }) if *target == group.carrier
    ));
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
    assert_eq!(
        marker_pool(&mut harness),
        pool,
        "the marker pool keeps its size"
    );
}
