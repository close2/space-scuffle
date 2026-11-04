//! Simulation, preview, and swarm scene controller systems for the Space Scuffle client.
//!
//! # Architectural Responsibilities
//! This module decouples Bevy ECS simulation synchronization and 3D scene lifecycle management
//! from `egui` UI presentation:
//! - **4-Hero Visual Lab Preview:** Dynamically queries `CatalogResource` for hero showcase archetypes,
//!   animating and rendering them with zero hardcoded unit IDs or creature names.
//! - **Live Combat Sandbox Synchronizer:** Applies immutable displayed snapshots and synchronizes
//!   3D Bevy entities using retained snapshots and persistent [`RenderEntityMap`] storage.
//! - **200-Bubble Swarm Stress Test:** High-density collision and rendering stress test.
//!
//! # Invariants
//! - Conversion from deterministic fixed-point to GPU floats happens strictly at snapshot boundaries.
//! - Live steady-state synchronization reuses storage; spawn/despawn and preview work have separate costs.
//! - Components (`Transform`, `BubbleInstance`) are mutated in place without issuing `commands.insert`.
//! - A live battle bubble's [`FlipbookPhase`] is inserted with it and changes only when its
//!   displayed action does; its material's flipbook lanes come from the presented record and that
//!   phase alone, so time passing modifies no material.
//! - The Lab preview and the swarm play their flipbooks on the same clock and lanes
//!   ([`scene_lanes`]); a preview actor's [`PreviewPhase`] changes only when its shown state
//!   does, and a swarm bubble's material handle changes only when it bounces, so neither scene
//!   modifies a material while time passes (plan `2026-11-04-battle-flipbook-animation`, D-9).

use crate::battle_selection::BattleSelection;
use crate::bubble_shader::{
    sync_bubble_material, BubbleDomeMaterial, BubbleInstance, BubbleSharedMeshes, BubbleUniformGpu,
    FlipbookRegistry, FLAG_ATTACK_APERTURE, FLAG_CLOAKED, FLAG_CRACKS, FLAG_CRT_GLITCH,
    FLAG_FACING_STRIPE, FLAG_FRAME_BLEND, FLAG_FRESNEL, FLAG_GLARE, FLAG_INNER_WARP, FLAG_SELECTED,
    FLAG_SPEED_BOOSTED,
};
use crate::flipbook_clock::{
    battle_uniform, displayed_action, elapsed_clock, scene_lanes, FlipbookPhase,
};
use crate::indicators::{attach_bubble_indicators, IndicatorAssets, SelectionRingMaterial};
use crate::lab_ui::{LabState, LabTab};
use crate::selection::{HeroLineupIndex, SelectionState};
use crate::CatalogResource;
use bevy::app::AppExit;
use bevy::prelude::*;
use sim_core::catalog::{UnitArchetype, UnitCatalog};
use sim_types::enums::{AnimState, ShapeKind, WeaponVisualKind};
use sim_types::ids::{EntityId, PlayerId, Tick, UnitTypeId};
use sim_types::snapshot::BubbleRenderInstance;
use std::collections::BTreeMap;

/// Marker component for close-up hero showcase preview bubbles in the Visual Lab.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabPreviewBubble;

/// Marker component for live battle simulation bubble entities.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveBattleBubble {
    /// Authoritative simulation `EntityId`.
    pub entity_id: EntityId,
}

/// Marker component for swarm stress test bubble entities.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwarmBubble;

/// The two shared materials of a swarm bubble's group, one per heading.
///
/// Every bubble of a group holds the same pair; a wall bounce points the bubble's
/// `MeshMaterial3d` at the material of its new heading (D-9).
// PERFORMANCE RATIONALE:
// Why: One material per group must carry one heading, but a group's bubbles bounce at different
// times. Rewriting the shared material for whichever bubble came last modified it on every frame
// once headings were mixed, and each modification prepares the material again.
// What: Each group has a material per heading, built once with constant flipbook lanes; a bounce
// swaps the bubble's handle (an `Arc` count change, no allocation).
// Trade-off: Four materials instead of two, and the bubble is re-batched on the frame it bounces.
#[derive(Component, Debug, Clone)]
pub struct SwarmHeadingMaterials {
    /// Material of a bubble walking towards +X (heading 0).
    pub eastward: Handle<BubbleDomeMaterial>,
    /// Material of a bubble walking towards -X (heading π).
    pub westward: Handle<BubbleDomeMaterial>,
}

impl SwarmHeadingMaterials {
    /// The material of a bubble moving at `speed_x` world units per second along X: westward for a
    /// negative speed, else eastward (the heading [`swarm_instance`] gives it).
    #[must_use]
    pub fn facing(&self, speed_x: f32) -> &Handle<BubbleDomeMaterial> {
        if speed_x < 0.0 {
            &self.westward
        } else {
            &self.eastward
        }
    }
}

/// The state a Lab preview actor shows, and the clock time at which it began to show.
///
/// Inserted with the actor and overwritten in place by `sync_lab_preview_system` only, when the
/// shown state changes, so a free loop or a one-shot clip starts at its first frame (D-5, D-9).
// PERFORMANCE RATIONALE:
// Why: The lanes must not depend on the current time, or the actor's material would be prepared
// again on every frame; but a state's clip must start where the state starts.
// What: The start is remembered on the actor and written with `set_if_neq`, so a frame on which the
// Lab's controls do not change writes nothing.
// Trade-off: Eight bytes of presentation state per preview actor.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewPhase {
    /// State the actor shows.
    pub action: AnimState,
    /// Flipbook clock time at which `action` began to show, in clock units.
    pub since: u32,
}

impl PreviewPhase {
    /// A phase that begins `action` at clock time `now`.
    #[must_use]
    pub const fn new(action: AnimState, now: u32) -> Self {
        Self { action, since: now }
    }

    /// The phase after the actor shows `action` at clock time `now`: unchanged while the action
    /// lasts, restarted at `now` when it changes.
    #[must_use]
    pub fn observed(self, action: AnimState, now: u32) -> Self {
        if self.action == action {
            self
        } else {
            Self::new(action, now)
        }
    }
}

/// Persistent mapping from authoritative simulation [`EntityId`] to Bevy render [`Entity`].
///
/// PERFORMANCE RATIONALE:
/// Why: Allocating a `HashMap` on every frame produces GC-like cache churn and memory fragmentation.
/// What: We retain a persistent map in a Bevy resource, inserting only when entities spawn and
/// removing only when entities despawn, resulting in zero allocations during steady-state ticks.
/// Trade-off: Minimal persistent resource state in exchange for zero heap allocations per frame.
#[derive(Resource, Default, Debug, Deref, DerefMut)]
pub struct RenderEntityMap(pub BTreeMap<EntityId, Entity>);

/// Catalog identities flagged `showcase_hero`, sorted by `showcase_order`.
///
// PERFORMANCE RATIONALE:
// Why: The preview synchronizer and the unit HUD both need the lineup on every frame.
// What: Each keeps one sorted list in system-local state and rebuilds it when the catalog changes.
// Trade-off: A catalog lookup per actor in exchange for no per-frame allocation or sort.
#[derive(Debug, Default)]
pub struct ShowcaseLineup {
    ids: Vec<UnitTypeId>,
}

impl ShowcaseLineup {
    /// Rebuilds the lineup from `catalog` if `catalog_changed`, reusing its storage.
    ///
    /// The owning system must call this on every run, or it can miss the only change signal.
    pub fn refresh(&mut self, catalog: &UnitCatalog, catalog_changed: bool) {
        if !catalog_changed {
            return;
        }
        self.ids.clear();
        self.ids.extend(
            catalog
                .archetypes()
                .filter(|archetype| archetype.showcase_hero)
                .map(|archetype| archetype.unit_type_id),
        );
        self.ids.sort_by_key(|id| {
            catalog
                .get(*id)
                .ok()
                .and_then(|archetype| archetype.showcase_order)
                .unwrap_or(u8::MAX)
        });
    }

    /// Number of identities in the lineup.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether the catalog flags no showcase identity.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Identities in lineup order.
    #[must_use]
    pub fn ids(&self) -> &[UnitTypeId] {
        &self.ids
    }

    /// Archetypes in lineup order. An identity missing from `catalog` is skipped.
    pub fn archetypes<'a>(
        &'a self,
        catalog: &'a UnitCatalog,
    ) -> impl Iterator<Item = &'a UnitArchetype> + 'a {
        self.ids.iter().filter_map(|id| catalog.get(*id).ok())
    }
}

/// World position and radius of lineup slot `index`, in world units.
fn preview_placement(index: usize, is_dome: bool) -> (Vec3, f32) {
    let x = match index {
        0 => -3.6,
        1 => -1.2,
        2 => 1.2,
        _ => 4.2,
    };
    let radius = if is_dome { 1.35 } else { 0.85 };
    let y = if is_dome { 0.0 } else { radius };
    (Vec3::new(x, y, 0.0), radius)
}

/// Builds the snapshot of one preview actor. Lab controls act on the selected actor only.
///
/// `pos` is in Bevy world space. The record carries no animation phase; the caller packs the
/// preview's flipbook lanes from its [`PreviewPhase`].
#[expect(
    clippy::cast_possible_truncation,
    reason = "presentation values are bounded by frame, atlas and window sizes"
)]
fn preview_instance(
    lab: &LabState,
    archetype: &UnitArchetype,
    index: usize,
    owner: PlayerId,
    pos: Vec3,
    radius: f32,
    is_selected: bool,
) -> BubbleRenderInstance {
    let selected_or = |value: f32, otherwise: f32| if is_selected { value } else { otherwise };
    let anim_state = preview_action(lab, is_selected);
    BubbleRenderInstance {
        entity_id: EntityId(100 + index as u32),
        shape: archetype.shape,
        silhouette: archetype.silhouette,
        unit_type: archetype.unit_type_id,
        owner,
        world_pos: [pos.x, pos.z, 0.0],
        ground_z: 0.0,
        radius,
        velocity: [selected_or(lab.speed, 0.0), 0.0],
        heading_angle_rad: selected_or(lab.heading_angle_deg.to_radians(), 0.0),
        aim_angle_rad: selected_or(lab.aim_angle_deg.to_radians(), 0.0),
        is_attacking: is_selected && lab.anim_state == AnimState::Attack,
        weapon_visual: WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state,
        attack_swing: None,
        hp_ratio: selected_or(lab.hp_ratio, 1.0),
        shield_ratio: selected_or(lab.shield_ratio, 1.0),
        shield_hit_flash: selected_or(lab.shield_hit_flash, 0.0),
        hull_hit_angle_rad: selected_or(lab.hit_angle_deg.to_radians(), 0.0),
        hull_hit_flash: selected_or(lab.hull_hit_flash, 0.0),
        build_or_queue_progress: Some(selected_or(lab.build_progress, 1.0)),
        selected: is_selected,
        cloaked: is_selected && lab.cloaked,
        speed_boosted: is_selected && lab.speed_boosted,
    }
}

/// The state a lineup actor shows: the Lab's selected state for the selected actor, else Idle.
fn preview_action(lab: &LabState, is_selected: bool) -> AnimState {
    if is_selected {
        lab.anim_state
    } else {
        AnimState::Idle
    }
}

/// Packs the Lab's optical toggles and the actor's own state into shader feature flags.
fn preview_feature_flags(lab: &LabState, instance: &BubbleRenderInstance, selected: bool) -> u32 {
    [
        (lab.opt_glare, FLAG_GLARE),
        (lab.opt_fresnel, FLAG_FRESNEL),
        (lab.opt_inner_warp, FLAG_INNER_WARP),
        (lab.opt_cracks, FLAG_CRACKS),
        (lab.opt_crt_glitch, FLAG_CRT_GLITCH),
        (lab.opt_facing_chevron, FLAG_FACING_STRIPE),
        (lab.opt_focal_lens, FLAG_ATTACK_APERTURE),
        (lab.opt_frame_blend, FLAG_FRAME_BLEND),
        (selected, FLAG_SELECTED),
        (instance.cloaked, FLAG_CLOAKED),
        (instance.speed_boosted, FLAG_SPEED_BOOSTED),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .fold(0, |flags, (_, flag)| flags | flag)
}

/// Builds the snapshot of one walking swarm bubble.
///
/// `world_xz` is in Bevy world units, and `speed_x` is in world units per second.
fn swarm_instance(
    archetype: &UnitArchetype,
    entity_id: u32,
    owner: PlayerId,
    world_xz: Vec2,
    speed_x: f32,
) -> BubbleRenderInstance {
    BubbleRenderInstance {
        entity_id: EntityId(entity_id),
        shape: ShapeKind::UnitSphere,
        silhouette: archetype.silhouette,
        unit_type: archetype.unit_type_id,
        owner,
        world_pos: [world_xz.x, world_xz.y, 0.5],
        ground_z: 0.0,
        radius: 0.45,
        velocity: [speed_x, 0.0],
        heading_angle_rad: if speed_x < 0.0 {
            std::f32::consts::PI
        } else {
            0.0
        },
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state: AnimState::Walk,
        attack_swing: None,
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: None,
        selected: false,
        cloaked: false,
        speed_boosted: false,
    }
}

/// Read-only inputs that every preview actor of one frame shares.
struct PreviewFrame<'a> {
    /// Lab controls; `selected_hero_index` names the actor they act on.
    lab: &'a LabState,
    /// Authored preview owners and their styles.
    palette: &'a crate::player_presentation::PlayerPalette,
    /// Loaded flipbooks.
    registry: &'a FlipbookRegistry,
    /// Flipbook clock time of the frame, in clock units ([`elapsed_clock`] of Bevy's time).
    now: u32,
}

/// One preview actor's snapshot and the GPU state derived from it.
struct PreviewActor {
    /// The actor's snapshot, as its `BubbleInstance` holds it.
    instance: BubbleRenderInstance,
    /// Shader uniform, with the Lab's feature flags applied.
    uniform: BubbleUniformGpu,
    /// Flipbook atlas the material samples, if one is loaded.
    texture: Option<Handle<Image>>,
    /// Owner style, also used by the actor's indicators.
    style: crate::player_presentation::PlayerStyle,
    /// Actor centre in Bevy world space.
    translation: Vec3,
}

impl PreviewFrame<'_> {
    /// Builds lineup actor `index` of `archetype`.
    ///
    /// With `allow_concept_video`, the selected actor samples the concept video when the Lab
    /// asks for it; without, every actor samples its own flipbook. `phase` is the actor's shown
    /// state and its start, from which its flipbook lanes are packed ([`scene_lanes`]).
    ///
    /// # Errors
    /// [`PresentationError`](crate::player_presentation::PresentationError) if the actor's
    /// authored owner is not a preview role, or if its slice cannot be played by the clock.
    fn actor(
        &self,
        archetype: &UnitArchetype,
        index: usize,
        allow_concept_video: bool,
        phase: PreviewPhase,
    ) -> Result<PreviewActor, crate::player_presentation::PresentationError> {
        let is_selected = index == self.lab.selected_hero_index;
        let is_dome = matches!(archetype.shape, ShapeKind::BuildingDome { .. });
        let (translation, radius) = preview_placement(index, is_dome);
        let owner = self.palette.showcase(index);
        let (texture, meta) = if allow_concept_video && is_selected && self.lab.use_concept_video {
            (
                self.registry.get_texture_by_name("concept_video"),
                self.registry.get_meta_by_name("concept_video"),
            )
        } else {
            (
                self.registry.get_texture(archetype.unit_type_id),
                self.registry.get_meta(archetype.unit_type_id),
            )
        };
        let instance = preview_instance(
            self.lab,
            archetype,
            index,
            owner,
            translation,
            radius,
            is_selected,
        );
        let style = self.palette.preview_style(instance.owner)?;
        let lanes = scene_lanes(
            meta,
            phase.action,
            instance.build_or_queue_progress,
            phase.since,
        )
        .map_err(
            |source| crate::player_presentation::PresentationError::Flipbook {
                unit_type: archetype.unit_type_id,
                action: phase.action,
                source,
            },
        )?;
        let mut uniform = BubbleUniformGpu::from_instance(&instance, style, lanes);
        uniform.feature_flags = preview_feature_flags(self.lab, &instance, is_selected);
        Ok(PreviewActor {
            instance,
            uniform,
            texture,
            style,
            translation,
        })
    }
}

/// Whether the preview lineup is drawn: the bouncing smog demo hides it.
fn preview_visibility(lab: &LabState) -> Visibility {
    let show_preview_heroes = !(lab.smog_bouncing_active && lab.opt_smog_volume);
    if show_preview_heroes {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

/// Checks that every lineup actor's flipbook, and the concept video if the selected actor
/// uses it, was admitted at startup. Successful lookups read cached evidence and allocate nothing.
///
/// `selected` is the lineup index the Lab controls act on.
///
/// # Errors
/// The first [`ClientMediaError`](crate::media::ClientMediaError) found, in lineup order.
fn admit_lineup_media<'a>(
    heroes: impl Iterator<Item = &'a UnitArchetype>,
    selected: usize,
    lab: &LabState,
    registry: &FlipbookRegistry,
    media_status: &crate::media::ClientMediaStatus,
) -> Result<(), crate::media::ClientMediaError> {
    for (index, archetype) in heroes.enumerate() {
        let action = if index == selected {
            lab.anim_state
        } else {
            AnimState::Idle
        };
        media_status.require(registry, archetype.unit_type_id, action)?;
        if index == selected && lab.use_concept_video {
            media_status.require_concept(registry, action)?;
        }
    }
    Ok(())
}

/// Spawns lineup actor `index` with its material and indicators, selecting it if it is the
/// Lab's actor and nothing is selected yet.
///
/// # Errors
/// As [`PreviewFrame::actor`]; nothing is spawned then.
fn spawn_preview_actor(
    commands: &mut Commands<'_, '_>,
    assets: &mut BubbleSceneAssets<'_>,
    selection_state: &mut SelectionState,
    frame: &PreviewFrame<'_>,
    archetype: &UnitArchetype,
    index: usize,
) -> Result<(), crate::player_presentation::PresentationError> {
    let phase = PreviewPhase::new(
        preview_action(frame.lab, index == frame.lab.selected_hero_index),
        frame.now,
    );
    let actor = frame.actor(archetype, index, false, phase)?;
    let material = BubbleDomeMaterial::new(actor.uniform, actor.texture);
    let components = (
        preview_visibility(frame.lab),
        LabPreviewBubble,
        HeroLineupIndex(index),
        phase,
    );
    let entity = assets.spawn_bubble(
        commands,
        &actor.instance,
        material,
        actor.translation,
        components,
        actor.style,
    );
    if index == frame.lab.selected_hero_index && selection_state.selected_entity.is_none() {
        selection_state.select(entity, Some(actor.instance.entity_id), Some(index));
    }
    Ok(())
}

/// One spawned preview actor's components, as the lineup query yields them.
struct PreviewActorComponents<'a, 'w> {
    /// The actor's material handle.
    material: &'a MeshMaterial3d<BubbleDomeMaterial>,
    /// The actor's snapshot.
    bubble: &'a mut BubbleInstance,
    /// The actor's placement.
    transform: &'a mut Mut<'w, Transform>,
    /// The actor's shown state and its start.
    phase: &'a mut Mut<'w, PreviewPhase>,
}

/// Brings spawned lineup actor `index` up to `frame` in place, without issuing any command.
///
/// The actor's phase restarts when its shown state changes ([`PreviewPhase::observed`]).
///
/// # Errors
/// As [`PreviewFrame::actor`]; the actor is left unchanged then, except for its phase.
fn update_preview_actor(
    frame: &PreviewFrame<'_>,
    archetype: &UnitArchetype,
    index: usize,
    components: PreviewActorComponents<'_, '_>,
    bubble_materials: &mut Assets<BubbleDomeMaterial>,
) -> Result<(), crate::player_presentation::PresentationError> {
    let PreviewActorComponents {
        material,
        bubble,
        transform,
        phase,
    } = components;
    let action = preview_action(frame.lab, index == frame.lab.selected_hero_index);
    let observed = phase.observed(action, frame.now);
    phase.set_if_neq(observed);
    let actor = frame.actor(archetype, index, true, observed)?;
    sync_bubble_material(bubble_materials, &material.0, &actor.uniform, actor.texture);
    // Writing through `Mut` flags change detection even for an equal value.
    transform.set_if_neq(
        transform
            .with_translation(actor.translation)
            .with_scale(Vec3::splat(actor.instance.radius)),
    );
    // Mutate component in place without issuing commands.entity(e).insert(...)
    bubble.0 = actor.instance;
    Ok(())
}

/// Synchronizes the 4-Hero Showcase Lineup in the Visual Lab with GUI state and selection.
///
/// Queries `CatalogResource` dynamically for all archetypes flagged with `showcase_hero: true`
/// sorted by `showcase_order`. Contains zero hardcoded creature names or unit type IDs.
///
/// Each frame admits the lineup's media ([`admit_lineup_media`]), then either respawns the
/// whole lineup when an actor is missing ([`spawn_preview_actor`]) or updates every actor in
/// place ([`update_preview_actor`]).
#[expect(
    clippy::type_complexity,
    reason = "a Bevy query declares its whole data access in one type"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn sync_lab_preview_system(
    mut owners: crate::player_presentation::PresentationContext<'_>,
    mut commands: Commands<'_, '_>,
    mut exit_events: EventWriter<'_, AppExit>,
    time: Res<'_, Time>,
    mut lab_state: ResMut<'_, LabState>,
    mut selection_state: ResMut<'_, SelectionState>,
    catalog: Res<'_, CatalogResource>,
    mut lineup: Local<'_, ShowcaseLineup>,
    registry: Res<'_, FlipbookRegistry>,
    mut media_status: ResMut<'_, crate::media::ClientMediaStatus>,
    mut assets: BubbleSceneAssets<'_>,
    mut query: Query<
        '_,
        '_,
        (
            Entity,
            &MeshMaterial3d<BubbleDomeMaterial>,
            &HeroLineupIndex,
            &mut BubbleInstance,
            &mut Transform,
            &mut Visibility,
            &mut PreviewPhase,
        ),
        With<LabPreviewBubble>,
    >,
) {
    // Runs before any early return, because `is_changed` reports a change only once.
    lineup.refresh(&catalog.0, catalog.is_changed());

    if lab_state.current_tab != LabTab::VisualLab {
        // Despawn preview bubbles when switching tabs
        for (entity, _, _, _, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
            if selection_state.is_selected(entity) {
                selection_state.deselect();
            }
        }
        return;
    }

    // Decay damage flash intensities
    let dt = time.delta_secs();
    lab_state.shield_hit_flash = (lab_state.shield_hit_flash - dt * 3.0).max(0.0);
    lab_state.hull_hit_flash = (lab_state.hull_hit_flash - dt * 3.0).max(0.0);

    let hero_count = query.iter().count();
    let expected_heroes = lineup.len().min(4);

    if media_status.retain_failure(&mut lab_state) {
        return;
    }
    let selected = selection_state
        .selected_hero_index
        .unwrap_or(lab_state.selected_hero_index)
        .min(expected_heroes.saturating_sub(1));
    // Admit the entire requested lineup before creating any material.
    let heroes = lineup.archetypes(&catalog.0).take(expected_heroes);
    if let Err(error) = admit_lineup_media(heroes, selected, &lab_state, &registry, &media_status) {
        crate::media::reject(error, &mut media_status, &mut lab_state, &mut exit_events);
        return;
    }

    if hero_count < expected_heroes {
        for (entity, _, _, _, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }
        let frame = PreviewFrame {
            lab: &lab_state,
            palette: &owners.palette,
            registry: &registry,
            now: elapsed_clock(time.elapsed()),
        };
        for (index, archetype) in lineup.archetypes(&catalog.0).take(4).enumerate() {
            let spawned = spawn_preview_actor(
                &mut commands,
                &mut assets,
                &mut selection_state,
                &frame,
                archetype,
                index,
            );
            if let Err(error) = spawned {
                owners.reject(error, &mut exit_events);
                return;
            }
        }
        return;
    }

    // If already spawned, update each hero in place.
    lab_state.selected_hero_index = selected;
    let target_vis = preview_visibility(&lab_state);
    let frame = PreviewFrame {
        lab: &lab_state,
        palette: &owners.palette,
        registry: &registry,
        now: elapsed_clock(time.elapsed()),
    };
    for (_, material, hero_idx, mut bubble, mut transform, mut vis, mut phase) in &mut query {
        if *vis != target_vis {
            *vis = target_vis;
        }
        let Some(archetype) = lineup
            .ids()
            .get(hero_idx.0)
            .and_then(|id| catalog.0.get(*id).ok())
        else {
            continue;
        };
        let updated = update_preview_actor(
            &frame,
            archetype,
            hero_idx.0,
            PreviewActorComponents {
                material,
                bubble: &mut bubble,
                transform: &mut transform,
                phase: &mut phase,
            },
            &mut assets.bubble_materials,
        );
        if let Err(error) = updated {
            owners.reject(error, &mut exit_events);
            return;
        }
    }
}

type LiveBattleQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static mut BubbleInstance,
        &'static mut Mesh3d,
        &'static MeshMaterial3d<BubbleDomeMaterial>,
        &'static mut FlipbookPhase,
    ),
    With<LiveBattleBubble>,
>;

/// One live bubble's components, as [`LiveBattleQuery`] yields them.
type LiveBattleItem<'a> = (
    Entity,
    Mut<'a, Transform>,
    Mut<'a, BubbleInstance>,
    Mut<'a, Mesh3d>,
    &'a MeshMaterial3d<BubbleDomeMaterial>,
    Mut<'a, FlipbookPhase>,
);

/// Shared meshes and material stores a synchronizer needs to spawn or restyle a bubble.
///
/// Grouping them keeps each synchronizer's declared data access readable and lets the
/// per-instance helpers take one argument instead of five.
#[derive(bevy::ecs::system::SystemParam)]
pub struct BubbleSceneAssets<'w> {
    shared_meshes: Res<'w, BubbleSharedMeshes>,
    indicator_assets: Res<'w, IndicatorAssets>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    bubble_materials: ResMut<'w, Assets<BubbleDomeMaterial>>,
    ring_materials: ResMut<'w, Assets<SelectionRingMaterial>>,
}

impl BubbleSceneAssets<'_> {
    /// The shared mesh for a dome (`is_dome`) or a sphere.
    fn mesh(&self, is_dome: bool) -> &Handle<Mesh> {
        if is_dome {
            &self.shared_meshes.dome_mesh
        } else {
            &self.shared_meshes.sphere_mesh
        }
    }

    /// Spawns one bubble with its own material and indicators, and returns its entity.
    ///
    /// `translation` is in Bevy world space, `instance.radius` scales the unit mesh, and
    /// `components` carries the scene's own markers and any extra components.
    fn spawn_bubble(
        &mut self,
        commands: &mut Commands<'_, '_>,
        instance: &BubbleRenderInstance,
        material: BubbleDomeMaterial,
        translation: Vec3,
        components: impl Bundle,
        style: crate::player_presentation::PlayerStyle,
    ) -> Entity {
        let is_dome = matches!(instance.shape, ShapeKind::BuildingDome { .. });
        let mesh = self.mesh(is_dome).clone();
        let material = self.bubble_materials.add(material);
        let entity = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_translation(translation).with_scale(Vec3::splat(instance.radius)),
                BubbleInstance(instance.clone()),
                components,
            ))
            .id();
        attach_bubble_indicators(
            commands,
            entity,
            instance,
            &self.indicator_assets,
            &mut self.materials,
            &mut self.ring_materials,
            style,
        );
        entity
    }
}

/// What one snapshot record contributes to its live render entity this frame.
struct LiveBubbleFrame<'a> {
    /// The published record, as the simulation produced it.
    instance: &'a BubbleRenderInstance,
    /// Shader uniform for the record, with its battle flipbook lanes and selection applied.
    uniform: BubbleUniformGpu,
    /// What the bubble shows and since which presented tick.
    phase: FlipbookPhase,
    /// Flipbook atlas of the record's unit type, if one is loaded.
    texture: Option<Handle<Image>>,
    /// Whether [`BattleSelection`] holds this entity.
    selected: bool,
    /// Bubble centre in Bevy world space: spheres rest on the ground, domes sit in it.
    translation: Vec3,
    /// Whether the record is a building dome rather than a unit sphere.
    is_dome: bool,
}

impl<'a> LiveBubbleFrame<'a> {
    /// Resolves the record's flipbook, uniform and pose. Pure: touches no ECS state.
    ///
    /// `phase` is the bubble's [`FlipbookPhase`] before this frame (`None` for a new bubble);
    /// the frame carries it advanced to the record's displayed action at `presented`.
    ///
    /// # Errors
    /// [`PresentationError::Flipbook`](crate::player_presentation::PresentationError::Flipbook)
    /// when the displayed slice cannot be played by the flipbook clock.
    fn new(
        instance: &'a BubbleRenderInstance,
        registry: &FlipbookRegistry,
        style: crate::player_presentation::PlayerStyle,
        selected: bool,
        presented: Tick,
        phase: Option<FlipbookPhase>,
    ) -> Result<Self, crate::player_presentation::PresentationError> {
        let meta = registry.get_meta(instance.unit_type);
        let texture = registry.get_texture(instance.unit_type);
        let action = displayed_action(meta, instance);
        let phase = phase.map_or_else(
            || FlipbookPhase::new(action, presented),
            |phase| phase.observed(action, presented),
        );
        let uniform = battle_uniform(meta, instance, style, presented, phase)
            .map_err(
                |source| crate::player_presentation::PresentationError::Flipbook {
                    unit_type: instance.unit_type,
                    action,
                    source,
                },
            )?
            .with_selection(selected);
        let is_dome = matches!(instance.shape, ShapeKind::BuildingDome { .. });
        let altitude_offset = if is_dome { 0.0 } else { instance.radius };
        let translation = Vec3::new(
            instance.world_pos[0],
            instance.world_pos[2] + altitude_offset,
            instance.world_pos[1],
        );
        Ok(Self {
            instance,
            uniform,
            phase,
            texture,
            selected,
            translation,
            is_dome,
        })
    }
}

/// Brings an existing live bubble up to `frame` in place, without issuing any command.
///
/// Every write is compared first, so a frame that changes nothing flags no component or
/// material as changed. `sync_allocation_tests.rs` proves this path allocates nothing.
fn update_live_instance(
    item: LiveBattleItem<'_>,
    frame: LiveBubbleFrame<'_>,
    assets: &mut BubbleSceneAssets<'_>,
) {
    let (_, mut transform, mut bubble, mut mesh, material, mut phase) = item;
    phase.set_if_neq(frame.phase);
    sync_bubble_material(
        &mut assets.bubble_materials,
        &material.0,
        &frame.uniform,
        frame.texture,
    );
    // Writing through `Mut` flags change detection even for an equal value, and a
    // changed `Mesh3d` makes Bevy re-extract the instance.
    transform.set_if_neq(
        transform
            .with_translation(frame.translation)
            .with_scale(Vec3::splat(frame.instance.radius)),
    );
    let target_mesh = assets.mesh(frame.is_dome);
    if mesh.0 != *target_mesh {
        mesh.0.clone_from(target_mesh);
    }
    bubble.0.clone_from(frame.instance);
    bubble.0.selected = frame.selected;
}

/// Spawns the render entity of a record that entered the snapshot, and returns it.
fn spawn_live_instance(
    commands: &mut Commands<'_, '_>,
    assets: &mut BubbleSceneAssets<'_>,
    frame: LiveBubbleFrame<'_>,
    style: crate::player_presentation::PlayerStyle,
) -> Entity {
    let instance = BubbleRenderInstance {
        selected: frame.selected,
        ..frame.instance.clone()
    };
    let material = BubbleDomeMaterial::new(frame.uniform, frame.texture);
    let marker = LiveBattleBubble {
        entity_id: instance.entity_id,
    };
    assets.spawn_bubble(
        commands,
        &instance,
        material,
        frame.translation,
        (marker, frame.phase),
        style,
    )
}

/// Despawns every render entity whose simulation entity left `live`, and forgets its selection.
///
/// `live` must be sorted by `entity_id`, as the authoritative snapshot is.
fn despawn_departed(
    commands: &mut Commands<'_, '_>,
    render_entities: &mut RenderEntityMap,
    selection: &mut BattleSelection,
    live: &[BubbleRenderInstance],
) {
    // PERFORMANCE RATIONALE:
    // Why: Scanning the entire snapshot for each renderer is quadratic.
    // What: Merge the sorted entity map and sorted snapshot membership once.
    // Trade-off: Requires the authoritative snapshot's stable ID ordering.
    let mut live = live.iter().peekable();
    render_entities.0.retain(|&id, &mut entity| {
        while live.peek().is_some_and(|record| record.entity_id < id) {
            live.next();
        }
        let is_alive = live.peek().is_some_and(|record| record.entity_id == id);
        if !is_alive {
            commands.entity(entity).despawn_recursive();
            selection.forget(id);
        }
        is_alive
    });
}

/// Applies the retained displayed snapshot; never executes simulation or consumes output.
/// Spatial transforms and material instances use exactly the same published pose.
///
/// The snapshot has no notion of selection, so this is where [`BattleSelection`] reaches each
/// bubble, its material and its indicators. A unit that left the snapshot is forgotten here too.
///
/// Per record, in snapshot order, an existing entity is updated in place
/// ([`update_live_instance`]) and a new one is spawned ([`spawn_live_instance`]). Entities whose
/// record left the snapshot are then despawned in one sorted merge ([`despawn_departed`]).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn sync_live_battle_system(
    mut owners: crate::player_presentation::PresentationContext<'_>,
    mut commands: Commands<'_, '_>,
    mut exit_events: EventWriter<'_, AppExit>,
    lab_state: Res<'_, LabState>,
    session: Res<'_, crate::battle_pipeline::session::BattleSession>,
    mut selection: ResMut<'_, BattleSelection>,
    mut render_entities: ResMut<'_, RenderEntityMap>,
    registry: Res<'_, FlipbookRegistry>,
    mut assets: BubbleSceneAssets<'_>,
    mut query: LiveBattleQuery<'_, '_>,
) {
    // Entity IDs restart with each session, so this runs before anything reads the selection.
    selection.bind_generation(session.commands.generation());
    if lab_state.current_tab != LabTab::LiveBattle {
        for (entity, ..) in &query {
            commands.entity(entity).despawn_recursive();
        }
        render_entities.0.clear();
        return;
    }
    if !session.is_ready() {
        return;
    }
    let snapshot = session.frames.presented();
    let presented = snapshot.tick;

    // Reuse existing materials and instances; spawn/despawn are separate lifecycle work.
    for instance in &snapshot.bubbles {
        let style = match owners
            .palette
            .battle_style(instance.owner, &session.metadata)
        {
            Ok(style) => style,
            Err(error) => {
                owners.reject(error, &mut exit_events);
                return;
            }
        };
        let selected = selection.contains(instance.entity_id);
        let existing = render_entities
            .0
            .get(&instance.entity_id)
            .and_then(|&entity| query.get_mut(entity).ok());
        let phase = existing.as_ref().map(|item| *item.5);
        let frame =
            match LiveBubbleFrame::new(instance, &registry, style, selected, presented, phase) {
                Ok(frame) => frame,
                Err(error) => {
                    owners.reject(error, &mut exit_events);
                    return;
                }
            };
        if let Some(item) = existing {
            update_live_instance(item, frame, &mut assets);
        } else if let std::collections::btree_map::Entry::Vacant(slot) =
            render_entities.0.entry(instance.entity_id)
        {
            slot.insert(spawn_live_instance(
                &mut commands,
                &mut assets,
                frame,
                style,
            ));
        }
    }
    despawn_departed(
        &mut commands,
        &mut render_entities,
        &mut selection,
        &snapshot.bubbles,
    );
}

/// Smallest world X a swarm bubble centre reaches before bouncing.
pub const SWARM_MIN_X: f32 = 13.0;

/// Largest world X a swarm bubble centre reaches before bouncing.
pub const SWARM_MAX_X: f32 = 31.0;

/// World Z of the swarm's first row. All ten rows stay on open low ground, south of the ramp.
pub const SWARM_MIN_Z: f32 = 3.6;

/// Speed of a swarm bubble along world X, in world units per second.
const SWARM_SPEED: f32 = 1.5;

/// World units between neighbouring swarm rows and columns.
const SWARM_SPACING: f32 = 1.2;

/// World Z of the swarm's last row.
pub const SWARM_MAX_Z: f32 = SWARM_MIN_Z + 9.0 * SWARM_SPACING;

/// World `(x, z)` spawn position of swarm bubble `index`, where `index` is below 200.
#[expect(
    clippy::cast_precision_loss,
    reason = "presentation values are bounded by frame, atlas and window sizes"
)]
fn swarm_spawn_position(index: u32) -> Vec2 {
    let row = (index % 10) as f32;
    let column = ((index % 100) / 10) as f32;
    let x = if index < 100 {
        SWARM_MIN_X + column * SWARM_SPACING
    } else {
        SWARM_MAX_X - column * SWARM_SPACING
    };
    Vec2::new(x, SWARM_MIN_Z + row * SWARM_SPACING)
}

/// Frames the RTS camera on the swarm's whole bounce area.
fn frame_swarm_camera(camera: &mut crate::camera::RtsCamera) {
    camera.target = Vec2::new(
        f32::midpoint(SWARM_MIN_X, SWARM_MAX_X),
        f32::midpoint(SWARM_MIN_Z, SWARM_MAX_Z),
    );
    camera.look_at_y = 0.5;
    camera.distance = 26.0;
}

/// Builds one group's pair of shared materials: the walk slice on the shared clock from `since`,
/// facing east and facing west. `instance_at(speed_x)` is the group's representative record
/// walking at `speed_x` world units per second.
///
/// # Errors
/// [`PresentationError::Flipbook`](crate::player_presentation::PresentationError::Flipbook) if
/// the walk slice cannot be played by the flipbook clock.
fn swarm_group_materials(
    archetype: &UnitArchetype,
    instance_at: impl Fn(f32) -> BubbleRenderInstance,
    style: crate::player_presentation::PlayerStyle,
    registry: &FlipbookRegistry,
    since: u32,
    bubble_materials: &mut Assets<BubbleDomeMaterial>,
) -> Result<SwarmHeadingMaterials, crate::player_presentation::PresentationError> {
    let meta = registry.get_meta(archetype.unit_type_id);
    let texture = registry.get_texture(archetype.unit_type_id);
    let lanes = scene_lanes(meta, AnimState::Walk, None, since).map_err(|source| {
        crate::player_presentation::PresentationError::Flipbook {
            unit_type: archetype.unit_type_id,
            action: AnimState::Walk,
            source,
        }
    })?;
    let mut material = |speed_x: f32| {
        let instance = instance_at(speed_x);
        let uniform = BubbleUniformGpu::from_instance(&instance, style, lanes);
        bubble_materials.add(BubbleDomeMaterial::new(uniform, texture.clone()))
    };
    Ok(SwarmHeadingMaterials {
        eastward: material(SWARM_SPEED),
        westward: material(-SWARM_SPEED),
    })
}

/// Spawns the 200 swarm bubbles of `archetype`: 100 primary walking east, 100 secondary
/// walking west. `since` is the flipbook clock time at which their walk starts, in clock units.
///
/// # Errors
/// [`PresentationError`](crate::player_presentation::PresentationError) if either authored
/// preview owner has no style, or if the walk slice cannot be played; nothing is spawned then.
fn spawn_swarm(
    commands: &mut Commands<'_, '_>,
    palette: &crate::player_presentation::PlayerPalette,
    registry: &FlipbookRegistry,
    archetype: &UnitArchetype,
    shared_meshes: &BubbleSharedMeshes,
    bubble_materials: &mut Assets<BubbleDomeMaterial>,
    since: u32,
) -> Result<(), crate::player_presentation::PresentationError> {
    // PERFORMANCE RATIONALE:
    // Why: 200 materials would recreate 200 bind groups per frame and defeat GPU batching.
    // What: Share four Handle<BubbleDomeMaterial> instances, one per group and heading
    // ([`SwarmHeadingMaterials`]).
    // Trade-off: Bubbles of one group and heading cannot differ in any material uniform.
    let primary_owner = palette.primary();
    let secondary_owner = palette.secondary();
    let primary_style = palette.preview_style(primary_owner)?;
    let secondary_style = palette.preview_style(secondary_owner)?;
    let primary = swarm_group_materials(
        archetype,
        |speed_x| {
            swarm_instance(
                archetype,
                1000,
                primary_owner,
                Vec2::new(8.0, 10.0),
                speed_x,
            )
        },
        primary_style,
        registry,
        since,
        bubble_materials,
    )?;
    let secondary = swarm_group_materials(
        archetype,
        |speed_x| {
            swarm_instance(
                archetype,
                1100,
                secondary_owner,
                Vec2::new(24.0, 10.0),
                speed_x,
            )
        },
        secondary_style,
        registry,
        since,
        bubble_materials,
    )?;

    for i in 0..200u32 {
        let primary_group = i < 100;
        let (owner, materials, speed_x) = if primary_group {
            (primary_owner, &primary, SWARM_SPEED)
        } else {
            (secondary_owner, &secondary, -SWARM_SPEED)
        };
        let Vec2 { x, y: z } = swarm_spawn_position(i);
        let instance = swarm_instance(archetype, 1000 + i, owner, Vec2::new(x, z), speed_x);
        commands.spawn((
            Mesh3d(shared_meshes.sphere_mesh.clone()),
            MeshMaterial3d(materials.facing(speed_x).clone()),
            Transform::from_xyz(x, 0.5, z).with_scale(Vec3::splat(0.45)),
            BubbleInstance(instance),
            materials.clone(),
            SwarmBubble,
        ));
    }
    Ok(())
}

/// Moves one swarm bubble by `dt` seconds along world X, bouncing off the swarm's X bounds.
///
/// Updates `transform` and the bubble's velocity, heading and `world_pos` (snapshot axes).
fn bounce_swarm_bubble(instance: &mut BubbleRenderInstance, transform: &mut Transform, dt: f32) {
    let mut vx = instance.velocity[0];
    transform.translation.x += vx * dt;
    if transform.translation.x <= SWARM_MIN_X {
        transform.translation.x = SWARM_MIN_X;
        vx = vx.abs();
        instance.velocity[0] = vx;
        instance.heading_angle_rad = 0.0;
    } else if transform.translation.x >= SWARM_MAX_X {
        transform.translation.x = SWARM_MAX_X;
        vx = -vx.abs();
        instance.velocity[0] = vx;
        instance.heading_angle_rad = std::f32::consts::PI;
    }
    instance.world_pos = [
        transform.translation.x,
        transform.translation.z,
        transform.translation.y,
    ];
}

/// The swarm bubbles, with what a frame moves and the materials a bounce chooses from.
type SwarmQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut BubbleInstance,
        &'static mut Transform,
        &'static mut MeshMaterial3d<BubbleDomeMaterial>,
        &'static SwarmHeadingMaterials,
    ),
    With<SwarmBubble>,
>;

/// Spawns and animates 200 colliding bubbles for the swarm stress test.
///
/// The first active frame frames the camera and spawns the swarm ([`spawn_swarm`]); every later
/// frame bounces each bubble ([`bounce_swarm_bubble`]) and, when it turns, points it at its
/// group's material for the new heading. The walk plays on the shared flipbook clock, so time
/// passing modifies no material (D-9).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn sync_swarm_stress_system(
    mut owners: crate::player_presentation::PresentationContext<'_>,
    mut commands: Commands<'_, '_>,
    mut exit_events: EventWriter<'_, AppExit>,
    time: Res<'_, Time>,
    mut lab_state: ResMut<'_, LabState>,
    catalog: Res<'_, CatalogResource>,
    registry: Res<'_, FlipbookRegistry>,
    mut media_status: ResMut<'_, crate::media::ClientMediaStatus>,
    shared_meshes: Res<'_, BubbleSharedMeshes>,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut camera_query: Query<'_, '_, &mut crate::camera::RtsCamera>,
    mut query: SwarmQuery<'_, '_>,
) {
    if lab_state.current_tab != LabTab::SwarmStressTest || !lab_state.swarm_active {
        // Despawn swarm when inactive
        for (entity, _, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }
        lab_state.swarm_bubble_count = 0;
        return;
    }

    if media_status.retain_failure(&mut lab_state) {
        return;
    }
    let Some(mobile_arch) = catalog
        .0
        .archetypes()
        .find(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
    else {
        return;
    };
    if let Err(error) = media_status.require(&registry, mobile_arch.unit_type_id, AnimState::Walk) {
        crate::media::reject(error, &mut media_status, &mut lab_state, &mut exit_events);
        return;
    }
    let count = query.iter().count();
    lab_state.swarm_bubble_count = count;

    if count == 0 {
        if let Some(mut cam) = camera_query.iter_mut().next() {
            frame_swarm_camera(&mut cam);
        }
        let spawned = spawn_swarm(
            &mut commands,
            &owners.palette,
            &registry,
            mobile_arch,
            &shared_meshes,
            &mut bubble_materials,
            elapsed_clock(time.elapsed()),
        );
        if let Err(error) = spawned {
            owners.reject(error, &mut exit_events);
            return;
        }
        lab_state.swarm_bubble_count = 200;
    } else {
        // Update 200 entity transforms and velocities with map-plane bouncing; a bubble that
        // turned takes its group's material for the new heading, and no material is modified.
        let dt = time.delta_secs();
        for (_, mut instance, mut tf, mut material, materials) in &mut query {
            bounce_swarm_bubble(&mut instance.0, &mut tf, dt);
            let facing = materials.facing(instance.0.velocity[0]);
            // Reading the handle through `Deref` leaves change detection alone.
            if material.0.id() != facing.id() {
                material.0 = facing.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::enums::SilhouetteClass;

    /// A swarm walker at world X `x` moving at `speed_x` world units per second.
    fn walker(x: f32, speed_x: f32) -> (BubbleRenderInstance, Transform) {
        let instance = BubbleRenderInstance {
            entity_id: EntityId(1000),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::LightProngs,
            unit_type: UnitTypeId(1),
            owner: PlayerId(0),
            world_pos: [x, SWARM_MIN_Z, 0.5],
            ground_z: 0.0,
            radius: 0.45,
            velocity: [speed_x, 0.0],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: AnimState::Walk,
            attack_swing: None,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            cloaked: false,
            speed_boosted: false,
        };
        (instance, Transform::from_xyz(x, 0.5, SWARM_MIN_Z))
    }

    #[test]
    fn a_walker_inside_the_bounds_moves_and_mirrors_its_pose() {
        let (mut instance, mut transform) = walker(20.0, 1.5);
        bounce_swarm_bubble(&mut instance, &mut transform, 2.0);
        assert!((transform.translation.x - 23.0).abs() < 1e-6);
        assert!((instance.velocity[0] - 1.5).abs() < f32::EPSILON);
        assert!(instance.heading_angle_rad.abs() < f32::EPSILON);
        assert_eq!(instance.world_pos, [23.0, SWARM_MIN_Z, 0.5]);
    }

    #[test]
    fn a_walker_past_the_west_bound_is_clamped_and_turns_east() {
        let (mut instance, mut transform) = walker(SWARM_MIN_X + 0.1, -1.5);
        instance.heading_angle_rad = std::f32::consts::PI;
        bounce_swarm_bubble(&mut instance, &mut transform, 1.0);
        assert!((transform.translation.x - SWARM_MIN_X).abs() < f32::EPSILON);
        assert!((instance.velocity[0] - 1.5).abs() < f32::EPSILON);
        assert!(instance.heading_angle_rad.abs() < f32::EPSILON);
        assert!((instance.world_pos[0] - SWARM_MIN_X).abs() < f32::EPSILON);
    }

    #[test]
    fn a_walker_past_the_east_bound_is_clamped_and_turns_west() {
        let (mut instance, mut transform) = walker(SWARM_MAX_X - 0.1, 1.5);
        bounce_swarm_bubble(&mut instance, &mut transform, 1.0);
        assert!((transform.translation.x - SWARM_MAX_X).abs() < f32::EPSILON);
        assert!((instance.velocity[0] + 1.5).abs() < f32::EPSILON);
        assert!((instance.heading_angle_rad - std::f32::consts::PI).abs() < f32::EPSILON);
    }

    #[test]
    fn a_swarm_bubble_takes_the_material_of_its_heading() {
        let mut handles = Assets::<BubbleDomeMaterial>::default();
        let uniform = |heading: f32| {
            let (mut instance, _) = walker(20.0, 1.5);
            instance.heading_angle_rad = heading;
            BubbleUniformGpu::from_instance(
                &instance,
                crate::player_presentation::PlayerStyle {
                    srgb: [0.5; 3],
                    linear: Vec4::ONE,
                    ui: bevy_egui::egui::Color32::GRAY,
                },
                crate::flipbook_clock::FlipbookLanes::fixed(0),
            )
        };
        let materials = SwarmHeadingMaterials {
            eastward: handles.add(BubbleDomeMaterial::new(uniform(0.0), None)),
            westward: handles.add(BubbleDomeMaterial::new(uniform(std::f32::consts::PI), None)),
        };
        assert_eq!(materials.facing(1.5), &materials.eastward);
        assert_eq!(materials.facing(0.0), &materials.eastward);
        assert_eq!(materials.facing(-1.5), &materials.westward);

        // A bounce off the east wall turns the walker west, and so its material.
        let (mut instance, mut transform) = walker(SWARM_MAX_X - 0.1, 1.5);
        bounce_swarm_bubble(&mut instance, &mut transform, 1.0);
        assert_eq!(materials.facing(instance.velocity[0]), &materials.westward);
    }

    #[test]
    fn a_preview_phase_restarts_only_when_its_state_changes() {
        let idle = PreviewPhase::new(AnimState::Idle, 100);
        assert_eq!(idle.observed(AnimState::Idle, 900), idle);
        assert_eq!(
            idle.observed(AnimState::Walk, 900),
            PreviewPhase::new(AnimState::Walk, 900)
        );
    }
}
