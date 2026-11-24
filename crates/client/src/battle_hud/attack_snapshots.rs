//! Draws the attack snapshots the viewer keeps: still images of attackers that struck it unseen.
//!
//! # Responsibility
//! Owns [`AttackSnapshotPool`] and [`sync_attack_snapshots`], which show each entry of
//! [`BattleFog::attack_snapshots`] on a pooled bubble entity: the attacker's own flipbook, held on
//! the first layer of its idle clip, at the pose of its last strike, with the catalog's snapshot
//! look (`effects.ron` `defaults.snapshot`) written into the bubble's shell. This is `StarCraft`
//! II's damage `Visibility Snapshot` in the fog (plan `2026-11-24-attacker-snapshot` D-5); the
//! minimap half waits for a minimap (B-204).
//!
//! # How it fits
//! Runs in `PreparePresentation` after [`refresh_battle_fog`](super::fog::refresh_battle_fog),
//! under `BattleProfile::Player` only: the sandbox, the editor and the headless check have no
//! fog, so no pool and nothing changes for them. A pooled entity carries [`AttackSnapshotBubble`]
//! and never `LiveBattleBubble`, so picking, box selection, inspection, the selection panel, the
//! health bars, the stance and effect systems and the command path, which all query
//! `With<LiveBattleBubble>`, never meet it, and the smog excludes it by its marker. It has no
//! selection ring, no bars and no indicator children. A right-click on it finds no bubble and
//! issues the point order bare ground issues.
//!
//! # Invariants
//! - Entry `i` of the fog's list is shown on pooled entity `i`; every entity past the list's end
//!   is hidden on the same frame.
//! - An entity is restyled only when the `(session, attacker, strike tick)` it shows changes, so
//!   a held snapshot modifies no material and no component.
//! - The pool grows by spawning only when a list is longer than every list before it; in steady
//!   state it spawns nothing and allocates nothing.
//! - No colour or number of the look is written here: the look is data (`effects.ron`).

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::batching::NoAutomaticBatching;
use sim_types::{AnimState, AttackSnapshotView, EntityId, ShapeKind, Tick};

use crate::battle_pipeline::commands::SessionGeneration;
use crate::battle_pipeline::fog::BattleFog;
use crate::battle_pipeline::session::BattleSession;
use crate::bubble_shader::{
    sync_bubble_material, sync_bubble_shell, BubbleDomeMaterial, BubbleInstance,
    BubbleSharedMeshes, BubbleUniformGpu, FlipbookRegistry, ShellUniformGpu,
};
use crate::effects::catalog::{EffectCatalog, ShellLook};
use crate::effects::stance::{casts_own_shadow, ShellSum};
use crate::flipbook_clock::{shown_stance, FlipbookLanes, FlipbookLanesError, FlipbookSpan};
use crate::player_presentation::{PlayerStyle, PresentationContext, PresentationError};

/// Marker of a pooled attack-snapshot bubble: drawn, never a unit. Nothing that picks, selects
/// or orders queries it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackSnapshotBubble;

/// What a pooled entity shows: the session, the attacker and the tick of the strike its image
/// is from. A new session restarts entity IDs, so the session is part of the identity.
type Shown = (Option<SessionGeneration>, EntityId, Tick);

/// One pooled entity and the image it was last styled for.
#[derive(Debug, Clone, Copy)]
struct PoolSlot {
    /// The bubble entity.
    entity: Entity,
    /// The image it shows (or showed last, while hidden); `None` forces a restyle.
    shown: Option<Shown>,
}

/// The pooled bubble entities that draw the viewer's attack snapshots (plan
/// `2026-11-24-attacker-snapshot` D-5).
// PERFORMANCE RATIONALE:
// Why: A snapshot appears, refreshes and disappears with strikes; spawning and despawning an
// entity and a material for each would churn assets and the allocator during every fight.
// What: Entities are kept and reused by index, hidden when unused, and restyled only when the
// image they show changes.
// Trade-off: The pool holds its high-water mark of entities and materials for the session.
#[derive(Resource, Debug, Default)]
pub struct AttackSnapshotPool {
    /// Pooled entities, in list order.
    slots: Vec<PoolSlot>,
    /// The catalog's snapshot look, read when the catalog was last added or changed.
    look: Option<ShellLook>,
}

impl AttackSnapshotPool {
    /// The pooled entities, in list order: entry `i` of the fog's list is drawn on entity `i`.
    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.slots.iter().map(|slot| slot.entity)
    }

    /// Every growable buffer the pool retains, as `(name, address, capacity in elements)`, for
    /// the steady-state tests. Not part of the pool's interface.
    #[doc(hidden)]
    pub fn retained_buffers(&self) -> impl Iterator<Item = (&'static str, usize, usize)> + '_ {
        std::iter::once((
            "attack_snapshots.slots",
            self.slots.as_ptr().addr(),
            self.slots.capacity(),
        ))
    }

    /// The shell the look gives a bubble of `radius` whose ground lies at height `ground`.
    fn shell(&self, radius: f32, ground: f32) -> ShellUniformGpu {
        let mut sum = ShellSum::default();
        if let Some(look) = &self.look {
            sum.add_look(look, 1.0);
        }
        sum.uniform(radius, ground)
    }
}

/// The lanes of a still image: the first layer of the idle clip of `meta`, held for good (layer
/// 0 without an atlas or an idle slice, as a live bubble without one shows).
///
/// # Errors
/// As [`FlipbookSpan::new`].
fn still_lanes(
    meta: Option<&asset_baker::manifest::FlipbookAtlasMeta>,
) -> Result<FlipbookLanes, FlipbookLanesError> {
    let Some(slice) = meta.and_then(|meta| meta.actions.get(&AnimState::Idle)) else {
        return Ok(FlipbookLanes::fixed(0));
    };
    let span = FlipbookSpan::new(slice.start_frame, 1, 1)?;
    Ok(FlipbookLanes::once(span, 0, true))
}

/// What one snapshot entry looks like on its pooled entity.
struct SnapshotFrame {
    /// Main uniform: the image's record with its still lanes.
    uniform: BubbleUniformGpu,
    /// The unit type's atlas (of the mode it was in), if loaded.
    texture: Option<Handle<Image>>,
    /// The snapshot look.
    shell: ShellUniformGpu,
    /// Bubble centre in Bevy world space: spheres rest on the ground, domes sit in it.
    translation: Vec3,
    /// Whether the image is a building dome.
    is_dome: bool,
}

impl SnapshotFrame {
    /// Resolves `entry`'s atlas, uniform, shell and pose. Pure: touches no ECS state.
    ///
    /// # Errors
    /// [`PresentationError::Flipbook`] when the idle slice cannot be held by the flipbook clock.
    fn new(
        entry: &AttackSnapshotView,
        registry: &FlipbookRegistry,
        style: PlayerStyle,
        pool: &AttackSnapshotPool,
    ) -> Result<Self, PresentationError> {
        let bubble = &entry.bubble;
        let stance = shown_stance(bubble, |mode| {
            registry.get_mode(bubble.unit_type, mode).is_some()
        });
        let (meta, texture) = match stance
            .atlas_mode()
            .and_then(|mode| registry.get_mode(bubble.unit_type, mode))
        {
            Some(atlas) => (Some(&atlas.meta), Some(atlas.texture.clone())),
            None => (
                registry.get_meta(bubble.unit_type),
                registry.get_texture(bubble.unit_type),
            ),
        };
        let lanes = still_lanes(meta).map_err(|source| PresentationError::Flipbook {
            unit_type: bubble.unit_type,
            action: AnimState::Idle,
            source,
        })?;
        let is_dome = matches!(bubble.shape, ShapeKind::BuildingDome { .. });
        let lift = if is_dome { 0.0 } else { bubble.radius };
        Ok(Self {
            uniform: BubbleUniformGpu::from_instance(bubble, style, lanes),
            texture,
            shell: pool.shell(bubble.radius, bubble.ground_z),
            translation: Vec3::new(
                bubble.world_pos[0],
                bubble.world_pos[2] + lift,
                bubble.world_pos[1],
            ),
            is_dome,
        })
    }
}

/// A pooled entity's components the pool writes.
type SnapshotQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut BubbleInstance,
        &'static mut Mesh3d,
        &'static MeshMaterial3d<BubbleDomeMaterial>,
        &'static mut Visibility,
        Has<NoAutomaticBatching>,
    ),
    With<AttackSnapshotBubble>,
>;

/// The stores a pooled entity is spawned and restyled from.
#[derive(bevy::ecs::system::SystemParam)]
pub struct SnapshotAssets<'w> {
    registry: Res<'w, FlipbookRegistry>,
    meshes: Res<'w, BubbleSharedMeshes>,
    materials: ResMut<'w, Assets<BubbleDomeMaterial>>,
}

impl SnapshotAssets<'_> {
    /// The shared mesh for a dome (`is_dome`) or a sphere.
    fn mesh(&self, is_dome: bool) -> &Handle<Mesh> {
        if is_dome {
            &self.meshes.dome_mesh
        } else {
            &self.meshes.sphere_mesh
        }
    }
}

/// Shows the fog's attack snapshots on the pool, on every frame on which the fog was rewritten
/// or the effect catalog changed (plan `2026-11-24-attacker-snapshot` D-5).
///
/// Entry `i` is shown on pooled entity `i`, restyled only when its `(session, attacker, strike
/// tick)` differs from what the entity shows; a list longer than the pool spawns the missing
/// entities; every entity past the list's end is hidden. A player style or flipbook failure is
/// reported as the live sync reports it, and nothing is substituted (charter §1.2).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn sync_attack_snapshots(
    mut owners: PresentationContext<'_>,
    mut exit_events: EventWriter<'_, AppExit>,
    mut commands: Commands<'_, '_>,
    fog: Res<'_, BattleFog>,
    session: Res<'_, BattleSession>,
    catalog: Option<Res<'_, EffectCatalog>>,
    mut pool: ResMut<'_, AttackSnapshotPool>,
    mut assets: SnapshotAssets<'_>,
    mut query: SnapshotQuery<'_, '_>,
) {
    let catalog_changed = catalog.as_ref().is_some_and(DetectChanges::is_changed);
    if catalog_changed {
        pool.look = catalog.as_deref().and_then(EffectCatalog::snapshot_look);
        for slot in &mut pool.slots {
            slot.shown = None;
        }
    }
    if !fog.is_changed() && !catalog_changed {
        return;
    }
    let generation = session.commands.generation();
    let entries = fog.attack_snapshots();
    for (index, entry) in entries.iter().enumerate() {
        let wanted = (generation, entry.entity, entry.struck);
        let slot = pool.slots.get(index).copied();
        if let Some(slot) = slot.filter(|slot| slot.shown == Some(wanted)) {
            if let Ok((.., mut visibility, _)) = query.get_mut(slot.entity) {
                visibility.set_if_neq(Visibility::Inherited);
            }
            continue;
        }
        let style = match owners
            .palette
            .battle_style(entry.bubble.owner, &session.metadata)
        {
            Ok(style) => style,
            Err(error) => {
                owners.reject(error, &mut exit_events);
                return;
            }
        };
        let frame = match SnapshotFrame::new(entry, &assets.registry, style, &pool) {
            Ok(frame) => frame,
            Err(error) => {
                owners.reject(error, &mut exit_events);
                return;
            }
        };
        if let Some(slot) = slot {
            restyle(
                &mut query,
                &mut commands,
                &mut assets,
                slot.entity,
                entry,
                frame,
            );
            pool.slots[index].shown = Some(wanted);
        } else {
            let entity = spawn(&mut commands, &mut assets, entry, frame);
            pool.slots.push(PoolSlot {
                entity,
                shown: Some(wanted),
            });
        }
    }
    for slot in pool.slots.iter().skip(entries.len()) {
        if let Ok((.., mut visibility, _)) = query.get_mut(slot.entity) {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

/// Spawns a pooled entity showing `entry` as `frame`, and returns it.
fn spawn(
    commands: &mut Commands<'_, '_>,
    assets: &mut SnapshotAssets<'_>,
    entry: &AttackSnapshotView,
    frame: SnapshotFrame,
) -> Entity {
    let mesh = assets.mesh(frame.is_dome).clone();
    let material = assets.materials.add(BubbleDomeMaterial {
        shell: frame.shell,
        ..BubbleDomeMaterial::new(frame.uniform, frame.texture)
    });
    let mut spawned = commands.spawn((
        AttackSnapshotBubble,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(frame.translation).with_scale(Vec3::splat(entry.bubble.radius)),
        Visibility::Inherited,
        BubbleInstance(entry.bubble.clone()),
    ));
    // Bevy bins shadow draws by mesh, so a bubble whose shell moves its shadow caster must cast
    // with its own material (`effects::stance::casts_own_shadow`).
    if casts_own_shadow(&frame.shell) {
        spawned.insert(NoAutomaticBatching);
    }
    spawned.id()
}

/// Brings the pooled `entity` up to `entry` as `frame`, shown; every write compares first.
fn restyle(
    query: &mut SnapshotQuery<'_, '_>,
    commands: &mut Commands<'_, '_>,
    assets: &mut SnapshotAssets<'_>,
    entity: Entity,
    entry: &AttackSnapshotView,
    frame: SnapshotFrame,
) {
    let Ok((mut transform, mut bubble, mut mesh, material, mut visibility, unbatched)) =
        query.get_mut(entity)
    else {
        return;
    };
    sync_bubble_material(
        &mut assets.materials,
        &material.0,
        &frame.uniform,
        frame.texture,
    );
    sync_bubble_shell(&mut assets.materials, &material.0, &frame.shell);
    transform.set_if_neq(
        Transform::from_translation(frame.translation).with_scale(Vec3::splat(entry.bubble.radius)),
    );
    let target = assets.mesh(frame.is_dome);
    if mesh.0 != *target {
        mesh.0.clone_from(target);
    }
    bubble.0.clone_from(&entry.bubble);
    visibility.set_if_neq(Visibility::Inherited);
    match (casts_own_shadow(&frame.shell), unbatched) {
        (true, false) => {
            commands.entity(entity).insert(NoAutomaticBatching);
        }
        (false, true) => {
            commands.entity(entity).remove::<NoAutomaticBatching>();
        }
        _ => {}
    }
}
