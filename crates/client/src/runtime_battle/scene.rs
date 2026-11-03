//! Runtime battle scene ownership, map-object snapshots and game/lab visibility.
//!
//! A replacement scene is prepared before changing the active simulation. Terrain
//! and authored object images share the loaded map's coordinates; per-frame updates
//! mutate reusable snapshot records and billboards without rebuilding scene assets.

use crate::lab_stage::LabStageMarker;

use super::BattleDefinition;
use crate::battle_pipeline::fog::BattleFog;
use crate::camera::RtsCamera;
use crate::scene_controller::{LiveBattleBubble, RenderEntityMap};
use crate::terrain::{spawn_terrain_owned, TerrainMaterial};
use crate::{LabState, LabTab, ResolvedAssetsRoot, SelectionState};
use bevy::prelude::*;
use sim_map::ResourceKind;
use sim_types::{PlayerId, SimVec2};
use std::collections::BTreeMap;

type BattleEffectFilter = Or<(
    With<crate::vfx::ProjectileVisual>,
    With<crate::vfx::BeamVisualComponent>,
    With<crate::vfx::MeleeSlashComponent>,
    With<crate::vfx::ShatterShard>,
)>;
type MapObjectQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static MapObject,
        &'static MeshMaterial3d<StandardMaterial>,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (Without<BattleSceneRoot>, Without<LabStageMarker>),
>;

/// Root owning precisely one loaded game terrain, light rig and authored map objects.
#[derive(Component)]
pub struct BattleSceneRoot;

/// Runtime billboard referencing one reusable map presentation record.
#[derive(Component)]
pub struct MapObject {
    index: usize,
}

impl MapObject {
    /// Position of this billboard's record in [`MapPresentation::objects`].
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
}

/// Immutable-to-consumers presentation record for a resource or observation tower.
#[derive(Debug, Clone)]
pub struct MapObjectSnapshot {
    /// Ground-anchored center in Bevy world coordinates, in world units.
    pub position: Vec3,
    /// Whether this object is shown and can be picked: present in the simulation or, under fog,
    /// present when the viewer last saw it.
    pub visible: bool,
    /// Watchtower owner, or None for neutral towers and natural resources.
    pub owner: Option<PlayerId>,
    /// True for ownership-bearing towers; natural resource artwork stays untinted.
    pub ownership_tint: bool,
}

/// Reused map-entity render snapshot, refreshed only when the simulation tick changes.
#[derive(Resource, Default)]
pub struct MapPresentation {
    /// Resource records followed by watchtower records, in authoritative map order.
    pub objects: Vec<MapObjectSnapshot>,
    /// Last simulation tick copied into the reusable presentation buffer.
    pub tick: Option<u32>,
    /// Real authored sprite images that must finish loading before render verification.
    pub sprite_images: Vec<Handle<Image>>,
}

/// Camera presets retained independently while switching among game and showcase tabs.
#[derive(Default)]
pub struct SceneCameraState {
    last_tab: Option<LabTab>,
    battle: Option<RtsCamera>,
    lab: Option<RtsCamera>,
    swarm: Option<RtsCamera>,
}

/// Restores each tab's camera without allowing a battle map to displace hero framing.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn sync_scene_camera(
    lab: Res<'_, LabState>,
    mut saved: Local<'_, SceneCameraState>,
    mut cameras: Query<'_, '_, &mut RtsCamera>,
) {
    if saved.last_tab == Some(lab.current_tab) {
        return;
    }
    let Ok(mut camera) = cameras.get_single_mut() else {
        return;
    };
    match saved.last_tab {
        Some(LabTab::LiveBattle) => saved.battle = Some(camera.clone()),
        Some(LabTab::VisualLab) => saved.lab = Some(camera.clone()),
        Some(LabTab::SwarmStressTest) => saved.swarm = Some(camera.clone()),
        None => {}
    }
    let next = match lab.current_tab {
        LabTab::LiveBattle => saved.battle.clone(),
        LabTab::VisualLab => Some(saved.lab.clone().unwrap_or_else(RtsCamera::hero_framing)),
        LabTab::SwarmStressTest => Some(saved.swarm.clone().unwrap_or_default()),
    };
    if let Some(next) = next {
        *camera = next;
    }
    saved.last_tab = Some(lab.current_tab);
}

/// Logs that battle setup `stage` failed with `error`, and records the failure on the Lab.
///
/// A recorded failure stops [`manage_battle_scene`] from retrying until the next reset request.
fn record_battle_failure(lab: &mut LabState, stage: &str, error: &dyn std::fmt::Display) {
    error!("{stage} failed: {error}");
    lab.battle_failure = Some(error.to_string());
}

/// Prepares the next session generation for `world` from `definition`, without installing it.
///
/// # Errors
/// [`PipelineError`](crate::battle_pipeline::driver::PipelineError) if the generation counter is
/// exhausted, the definition's metadata is invalid, or the world cannot be prepared.
fn prepare_session(
    definition: &BattleDefinition,
    current: &crate::battle_pipeline::session::BattleSession,
    world: &mut sim_core::world::SimWorld,
) -> Result<
    crate::battle_pipeline::session::BattleSession,
    crate::battle_pipeline::driver::PipelineError,
> {
    let generation = current.next_generation()?;
    let metadata =
        crate::battle_pipeline::session::CommittedBattleMetadata::from_definition(definition)?;
    crate::battle_pipeline::session::BattleSession::prepare(generation, world, metadata)
}

/// Spawns one billboard per resource node, then one per watchtower, under `root`, and returns
/// the presentation records in the same order.
///
/// Positions are ground-anchored centres in Bevy world space, read from `map`.
fn spawn_map_objects(
    commands: &mut Commands<'_, '_>,
    root: Entity,
    map: &sim_map::GameMap,
    visuals: &super::MapVisualCatalog,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> MapPresentation {
    let mineral = load_map_sprite(asset_server, meshes, materials, &visuals.mineral);
    let gas = load_map_sprite(asset_server, meshes, materials, &visuals.gas);
    let tower_sprite = load_map_sprite(asset_server, meshes, materials, &visuals.watchtower);
    let mut presentation = MapPresentation {
        sprite_images: vec![
            mineral.image.clone(),
            gas.image.clone(),
            tower_sprite.image.clone(),
        ],
        ..default()
    };
    for node in map.resources() {
        let visual = match node.kind {
            ResourceKind::Mineral => &mineral,
            ResourceKind::Gas => &gas,
        };
        let ground = map.terrain_altitude_scalar(node.pos).to_f32();
        let position = Vec3::new(
            node.pos.x.to_f32(),
            ground + visual.size * 0.5,
            node.pos.y.to_f32(),
        );
        spawn_map_object(commands, root, presentation.objects.len(), visual, position);
        presentation.objects.push(MapObjectSnapshot {
            position,
            visible: node.remaining > 0,
            owner: None,
            ownership_tint: false,
        });
    }
    for tower in map.watchtowers() {
        // A tower's ownership tint is independent; geometry and texture stay shared.
        let visual = MapSpriteAssets {
            material: materials.add(tower_sprite.base_material.clone()),
            ..tower_sprite.clone()
        };
        let ground = map.terrain_altitude_scalar(tower.pos).to_f32();
        let position = Vec3::new(
            tower.pos.x.to_f32(),
            ground + visual.size * 0.5,
            tower.pos.y.to_f32(),
        );
        spawn_map_object(
            commands,
            root,
            presentation.objects.len(),
            &visual,
            position,
        );
        presentation.objects.push(MapObjectSnapshot {
            position,
            visible: true,
            owner: tower.controlling_player,
            ownership_tint: true,
        });
    }
    presentation
}

/// Despawns the previous battle's scene roots, unit bubbles and effects, and forgets their
/// render mapping and selection.
fn retire_previous_battle(
    commands: &mut Commands<'_, '_>,
    scenes: &Query<'_, '_, Entity, With<BattleSceneRoot>>,
    bubbles: &Query<'_, '_, Entity, With<LiveBattleBubble>>,
    effects: &Query<'_, '_, Entity, BattleEffectFilter>,
    render_entities: &mut RenderEntityMap,
    selection: &mut SelectionState,
) {
    for root in scenes {
        commands.entity(root).despawn_recursive();
    }
    for bubble in bubbles {
        commands.entity(bubble).despawn_recursive();
    }
    for effect in effects {
        commands.entity(effect).despawn_recursive();
    }
    render_entities.0.clear();
    selection.deselect();
}

/// Points every RTS camera at the first player's start approach, if there is one.
fn focus_battle_cameras(
    cameras: &mut Query<'_, '_, &mut RtsCamera>,
    approaches: &BTreeMap<PlayerId, SimVec2>,
    map: &sim_map::GameMap,
) {
    if let Some(focus) = approaches.values().next() {
        for mut camera in cameras {
            camera.target = Vec2::new(focus.x.to_f32(), focus.y.to_f32());
            camera.look_at_y = map.terrain_altitude_scalar(*focus).to_f32() + 1.2;
            camera.distance = 22.0;
        }
    }
}

/// Installs `prepared` with `world` as the active session and resets the Lab's battle state.
fn commit_battle(
    lab: &mut LabState,
    session: &mut crate::battle_pipeline::session::BattleSession,
    prepared: crate::battle_pipeline::session::BattleSession,
    world: sim_core::world::SimWorld,
    approaches: BTreeMap<PlayerId, SimVec2>,
) {
    lab.battle_start_positions = approaches;
    lab.last_checksum_hex.clear();
    // A replacement is a new match, so it starts unpaused at normal speed, as the first one does
    // from `LabState::default`. The first commit leaves both alone: whoever booted the client
    // paused, such as a capture or a test harness, still finds it paused.
    if session.commands.generation().is_some() {
        let fresh = LabState::default();
        lab.battle_paused = fresh.battle_paused;
        lab.simulation_speed = fresh.simulation_speed;
    }
    *session = prepared.with_world(world);
    lab.battle_failure = None;
}

/// Installs or resets the selected battle transactionally using the same loaded definition.
///
/// Each stage that can fail (world, start approaches, session, terrain) runs before the old
/// scene is touched, so a failure leaves the running battle in place. Only then are the map
/// objects spawned ([`spawn_map_objects`]), the old scene retired ([`retire_previous_battle`]),
/// the cameras focused ([`focus_battle_cameras`]) and the new session installed
/// ([`commit_battle`]).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn manage_battle_scene(
    mut commands: Commands<'_, '_>,
    definition: Option<Res<'_, BattleDefinition>>,
    assets_root: Option<Res<'_, ResolvedAssetsRoot>>,
    asset_server: Res<'_, AssetServer>,
    mut lab: ResMut<'_, LabState>,
    mut render_entities: ResMut<'_, RenderEntityMap>,
    mut selection: ResMut<'_, SelectionState>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut terrain_materials: ResMut<'_, Assets<TerrainMaterial>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    mut images: ResMut<'_, Assets<Image>>,
    scenes: Query<'_, '_, Entity, With<BattleSceneRoot>>,
    bubbles: Query<'_, '_, Entity, With<LiveBattleBubble>>,
    mut cameras: Query<'_, '_, &mut RtsCamera>,
    effects: Query<'_, '_, Entity, BattleEffectFilter>,
    mut session: ResMut<'_, crate::battle_pipeline::session::BattleSession>,
) {
    if lab.current_tab != LabTab::LiveBattle
        || ((session.world().is_some() || lab.battle_failure.is_some())
            && !lab.battle_reset_requested)
    {
        return;
    }
    let (Some(definition), Some(assets_root)) = (definition, assets_root) else {
        return;
    };
    lab.battle_reset_requested = false;
    let mut world = match definition.instantiate() {
        Ok(world) => world,
        Err(error) => {
            record_battle_failure(&mut lab, "Battle initialization", &error);
            return;
        }
    };
    let approaches = match definition.start_approaches(&world) {
        Ok(positions) => positions,
        Err(error) => {
            record_battle_failure(&mut lab, "Battle approach validation", &error);
            return;
        }
    };
    let prepared_session = match prepare_session(&definition, &session, &mut world) {
        Ok(prepared) => prepared,
        Err(error) => {
            record_battle_failure(&mut lab, "Battle pipeline preparation", &error);
            return;
        }
    };
    let new_root = commands
        .spawn((BattleSceneRoot, Transform::IDENTITY, Visibility::Visible))
        .id();
    if let Err(error) = spawn_terrain_owned(
        &mut commands,
        &mut meshes,
        &mut terrain_materials,
        &mut materials,
        &mut images,
        &definition.theme,
        world.map.clone(),
        assets_root.as_path(),
        Some(new_root),
        Some(0),
    ) {
        commands.entity(new_root).despawn_recursive();
        record_battle_failure(&mut lab, "Battle terrain initialization", &error);
        return;
    }

    let presentation = spawn_map_objects(
        &mut commands,
        new_root,
        &world.map,
        &definition.visuals,
        &asset_server,
        &mut meshes,
        &mut materials,
    );
    retire_previous_battle(
        &mut commands,
        &scenes,
        &bubbles,
        &effects,
        &mut render_entities,
        &mut selection,
    );
    focus_battle_cameras(&mut cameras, &approaches, &world.map);
    commit_battle(&mut lab, &mut session, prepared_session, world, approaches);
    commands
        .entity(new_root)
        .insert(super::environment::CommittedBattleEnvironment::from_theme(
            &definition.theme,
        ));
    commands.insert_resource(presentation);
    commands.insert_resource(definition.palette.clone());
}

#[derive(Clone)]
struct MapSpriteAssets {
    base_material: StandardMaterial,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    image: Handle<Image>,
    size: f32,
}

fn load_map_sprite(
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    visual: &super::MapVisualEntry,
) -> MapSpriteAssets {
    let image = asset_server.load(visual.sprite.clone());
    let mesh = meshes.add(Rectangle::new(visual.size, visual.size));
    let base_material = StandardMaterial {
        base_color_texture: Some(image.clone()),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        ..default()
    };
    let material = materials.add(base_material.clone());
    MapSpriteAssets {
        base_material,
        mesh,
        material,
        image,
        size: visual.size,
    }
}

fn spawn_map_object(
    commands: &mut Commands<'_, '_>,
    root: Entity,
    index: usize,
    visual: &MapSpriteAssets,
    position: Vec3,
) {
    commands
        .spawn((
            MapObject { index },
            Mesh3d(visual.mesh.clone()),
            MeshMaterial3d(visual.material.clone()),
            Transform::from_translation(position),
            Visibility::Visible,
        ))
        .set_parent(root);
}

/// Applies the session's published map state and updates billboards.
///
/// Under `fog`, nodes show what the viewer last saw instead of the truth, so a node mined out of
/// sight stays on screen, and stays pickable, until someone goes and looks. Watchtowers need no
/// such memory, because nothing changes their owner during a battle.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn update_map_presentation(
    mut presentation: Option<ResMut<'_, MapPresentation>>,
    session: Res<'_, crate::battle_pipeline::session::BattleSession>,
    fog: Option<Res<'_, BattleFog>>,
    mut owners: crate::player_presentation::PresentationContext<'_>,
    mut exit: EventWriter<'_, AppExit>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    cameras: Query<'_, '_, &GlobalTransform, With<Camera3d>>,
    mut objects: MapObjectQuery<'_, '_>,
) {
    let (Some(published), Some(presentation)) = (session.published(), presentation.as_mut()) else {
        return;
    };
    // A fog written after this tick was copied is news even though the tick is not.
    let fog_changed = fog.as_ref().is_some_and(DetectChanges::is_changed);
    if presentation.tick != Some(published.tick) || fog_changed {
        let nodes = fog
            .as_ref()
            .map_or(published.resources_present.as_slice(), |fog| {
                fog.node_present()
            });
        for (record, present) in presentation.objects.iter_mut().zip(nodes) {
            record.visible = *present;
        }
        for (record, owner) in presentation
            .objects
            .iter_mut()
            .skip(published.resources_present.len())
            .zip(&published.watchtower_owners)
        {
            record.owner = *owner;
        }
        presentation.tick = Some(published.tick);
    }
    let Ok(camera) = cameras.get_single() else {
        return;
    };
    for (object, material_handle, mut transform, mut visibility) in &mut objects {
        let Some(record) = presentation.objects.get(object.index) else {
            continue;
        };
        visibility.set_if_neq(if record.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let tint = if record.ownership_tint {
            match owners
                .palette
                .battle_style(record.owner.unwrap_or(PlayerId::NEUTRAL), &session.metadata)
            {
                Ok(style) => style.color(),
                Err(error) => {
                    owners.reject(error, &mut exit);
                    return;
                }
            }
        } else {
            Color::WHITE
        };
        // `get_mut` flags the asset modified, so an unchanged tint must stay on `get`.
        let tint_changed = materials
            .get(&material_handle.0)
            .is_some_and(|material| material.base_color != tint);
        if tint_changed {
            if let Some(material) = materials.get_mut(&material_handle.0) {
                material.base_color = tint;
            }
        }
        let facing = camera.translation() - record.position;
        transform.set_if_neq(Transform {
            translation: record.position,
            rotation: Quat::from_rotation_y(facing.x.atan2(facing.z)),
            ..*transform
        });
    }
}
