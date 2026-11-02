//! Runtime battle scene ownership, map-object snapshots and game/lab visibility.
//!
//! A replacement scene is prepared before changing the active simulation. Terrain
//! and authored object images share the loaded map's coordinates; per-frame updates
//! mutate reusable snapshot records and billboards without rebuilding scene assets.

use crate::lab_stage::LabStageMarker;

use super::BattleDefinition;
use crate::camera::RtsCamera;
use crate::scene_controller::{LiveBattleBubble, RenderEntityMap};
use crate::terrain::{spawn_terrain_owned, TerrainMaterial};
use crate::{LabState, LabTab, ResolvedAssetsRoot, SelectionState};
use bevy::prelude::*;
use sim_map::ResourceKind;
use sim_types::PlayerId;

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

/// Immutable-to-consumers presentation record for a resource or observation tower.
#[derive(Debug, Clone)]
pub struct MapObjectSnapshot {
    /// Ground-anchored center in Bevy world coordinates, in world units.
    pub position: Vec3,
    /// Whether this object remains present in the simulation.
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

/// Installs or resets the selected battle transactionally using the same loaded definition.
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
        || ((lab.sim_world.is_some() || lab.battle_failure.is_some())
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
            error!("Battle initialization failed: {error}");
            lab.battle_failure = Some(error.to_string());
            return;
        }
    };
    let approaches = match definition.start_approaches(&world) {
        Ok(positions) => positions,
        Err(error) => {
            error!("Battle approach validation failed: {error}");
            lab.battle_failure = Some(error.to_string());
            return;
        }
    };
    let prepared_session = (|| {
        let generation = session.next_generation()?;
        let metadata =
            crate::battle_pipeline::session::CommittedBattleMetadata::from_definition(&definition)?;
        crate::battle_pipeline::session::BattleSession::prepare(generation, &mut world, metadata)
    })();
    let prepared_session = match prepared_session {
        Ok(prepared) => prepared,
        Err(error) => {
            error!("Battle pipeline preparation failed: {error}");
            lab.battle_failure = Some(error.to_string());
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
        error!("Battle terrain initialization failed: {error}");
        lab.battle_failure = Some(error.to_string());
        return;
    }

    let mineral = load_map_sprite(
        &asset_server,
        &mut meshes,
        &mut materials,
        &definition.visuals.mineral,
    );
    let gas = load_map_sprite(
        &asset_server,
        &mut meshes,
        &mut materials,
        &definition.visuals.gas,
    );
    let tower_sprite = load_map_sprite(
        &asset_server,
        &mut meshes,
        &mut materials,
        &definition.visuals.watchtower,
    );
    let mut presentation = MapPresentation {
        sprite_images: vec![
            mineral.image.clone(),
            gas.image.clone(),
            tower_sprite.image.clone(),
        ],
        ..default()
    };
    for node in world.map.resources() {
        let visual = match node.kind {
            ResourceKind::Mineral => &mineral,
            ResourceKind::Gas => &gas,
        };
        let ground = world.map.terrain_altitude_scalar(node.pos).to_f32();
        let position = Vec3::new(
            node.pos.x.to_f32(),
            ground + visual.size * 0.5,
            node.pos.y.to_f32(),
        );
        spawn_map_object(
            &mut commands,
            new_root,
            presentation.objects.len(),
            visual,
            position,
        );
        presentation.objects.push(MapObjectSnapshot {
            position,
            visible: node.remaining > 0,
            owner: None,
            ownership_tint: false,
        });
    }
    for tower in world.map.watchtowers() {
        // A tower's ownership tint is independent; geometry and texture stay shared.
        let visual = MapSpriteAssets {
            material: materials.add(tower_sprite.base_material.clone()),
            ..tower_sprite.clone()
        };
        let ground = world.map.terrain_altitude_scalar(tower.pos).to_f32();
        let position = Vec3::new(
            tower.pos.x.to_f32(),
            ground + visual.size * 0.5,
            tower.pos.y.to_f32(),
        );
        spawn_map_object(
            &mut commands,
            new_root,
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

    for root in &scenes {
        commands.entity(root).despawn_recursive();
    }
    for bubble in &bubbles {
        commands.entity(bubble).despawn_recursive();
    }
    for effect in &effects {
        commands.entity(effect).despawn_recursive();
    }
    render_entities.0.clear();
    selection.deselect();
    if let Some(focus) = approaches.values().next() {
        for mut camera in &mut cameras {
            camera.target = Vec2::new(focus.x.to_f32(), focus.y.to_f32());
            camera.look_at_y = world.map.terrain_altitude_scalar(*focus).to_f32() + 1.2;
            camera.distance = 22.0;
        }
    }
    lab.battle_start_positions = approaches;
    lab.last_checksum_hex.clear();
    *session = prepared_session;
    lab.battle_failure = None;
    lab.sim_world = Some(world);
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

/// Copies dynamic map-object state at the snapshot boundary and updates billboards.
pub fn update_map_presentation(
    lab: Res<'_, LabState>,
    mut presentation: Option<ResMut<'_, MapPresentation>>,
    session: Res<'_, crate::battle_pipeline::session::BattleSession>,
    mut owners: crate::player_presentation::PresentationContext<'_>,
    mut exit: EventWriter<'_, AppExit>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    cameras: Query<'_, '_, &GlobalTransform, With<Camera3d>>,
    mut objects: MapObjectQuery<'_, '_>,
) {
    let (Some(world), Some(presentation)) = (lab.sim_world.as_ref(), presentation.as_mut()) else {
        return;
    };
    if presentation.tick != Some(world.current_tick) {
        for (record, node) in presentation.objects.iter_mut().zip(world.map.resources()) {
            record.visible = node.remaining > 0;
        }
        for (record, tower) in presentation
            .objects
            .iter_mut()
            .skip(world.map.resources().len())
            .zip(world.map.watchtowers())
        {
            record.owner = tower.controlling_player;
        }
        presentation.tick = Some(world.current_tick);
    }
    let Ok(camera) = cameras.get_single() else {
        return;
    };
    for (object, material_handle, mut transform, mut visibility) in &mut objects {
        let Some(record) = presentation.objects.get(object.index) else {
            continue;
        };
        *visibility = if record.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
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
        if let Some(material) = materials.get_mut(&material_handle.0) {
            if material.base_color != tint {
                material.base_color = tint;
            }
        }
        transform.translation = record.position;
        let facing = camera.translation() - record.position;
        transform.rotation = Quat::from_rotation_y(facing.x.atan2(facing.z));
    }
}
