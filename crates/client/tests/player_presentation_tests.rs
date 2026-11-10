//! Verifies actual catalog ownership through simulation snapshots, prepared UI
//! colors, shell uniforms, indicator materials and every combat-effect branch.
//! Snapshot edits model presentation changes without mutating live ownership ledgers.

use bevy::prelude::*;
use client::battle_pipeline::session::{
    BattleSession, CommittedBattleMetadata, ParticipantMetadata,
};
use client::bubble_shader::{BubbleInstance, BubbleUniformGpu};
use client::indicators::{self, AirTetherMarker, GroundRingMarker, SelectionRingMaterial};
use client::player_presentation::{PlayerPalette, PlayerStyle, PresentationStatus};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::vfx::{self, CombatFxMaterial, CombatVisualEvent};
use sim_types::{BubbleRenderInstance, CombatEventVisual, PlayerId, ShapeKind, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn definition() -> Result<BattleDefinition, Box<dyn std::error::Error>> {
    Ok(BattleDefinition::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
        DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?)
}

fn snapshot_fixture(
    definition: &BattleDefinition,
) -> Result<BubbleRenderInstance, Box<dyn std::error::Error>> {
    definition
        .instantiate()?
        .build_render_snapshot()
        .bubbles
        .into_iter()
        .find(|bubble| matches!(bubble.shape, ShapeKind::UnitSphere))
        .ok_or_else(|| "actual scenario has no unit sphere".into())
}

#[test]
fn simulation_sparse_owners_and_labels_match_shell_identity() -> TestResult {
    let definition = definition()?;
    let mut world = definition.instantiate()?;
    let unit = definition
        .catalogs
        .units
        .archetypes()
        .find(|arch| matches!(arch.shape, ShapeKind::UnitSphere))
        .ok_or("mobile catalog unit")?
        .unit_type_id;
    let mut identities = Vec::new();
    for owner in [PlayerId(0), PlayerId(2), PlayerId(7)] {
        let id = world.spawn_unit(
            unit,
            owner,
            SimVec2::from_i32(4 + i32::from(owner.0) * 2, 4),
        )?;
        identities.push((id, owner));
    }
    let checksum = world.compute_checksum();
    let snapshot = world.build_render_snapshot();
    for (id, owner) in identities {
        let bubble = snapshot
            .bubbles
            .iter()
            .find(|bubble| bubble.entity_id == id)
            .ok_or("snapshot identity")?;
        assert_eq!(bubble.owner, owner);
        let style = definition.palette.resolve(owner)?;
        let uniform = BubbleUniformGpu::from_instance(
            bubble,
            style,
            client::bubble_shader::still_frame_lanes(None, bubble.anim_state),
        );
        assert_eq!(uniform.team_color, style.linear);
    }
    for participant in CommittedBattleMetadata::from_definition(&definition)?.players {
        assert_eq!(
            participant.color,
            definition.palette.resolve(participant.owner)?.ui
        );
    }
    assert_eq!(world.compute_checksum(), checksum);
    Ok(())
}

#[derive(Resource)]
struct IndicatorFixture(BubbleRenderInstance, PlayerStyle);

fn spawn_indicator_fixture(
    mut commands: Commands<'_, '_>,
    fixture: Res<'_, IndicatorFixture>,
    assets: Res<'_, indicators::IndicatorAssets>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    mut rings: ResMut<'_, Assets<SelectionRingMaterial>>,
) {
    let fixture = fixture.into_inner();
    let assets = assets.into_inner();
    let instance = &fixture.0;
    let style = fixture.1;
    let parent = commands
        .spawn((BubbleInstance(instance.clone()), Transform::IDENTITY))
        .id();
    indicators::attach_bubble_indicators(
        &mut commands,
        parent,
        instance,
        assets,
        &mut materials,
        &mut rings,
        style,
    );
}

fn assert_indicators(app: &mut App, style: PlayerStyle) -> TestResult {
    let world = app.world_mut();
    let mut rings =
        world.query_filtered::<&MeshMaterial3d<SelectionRingMaterial>, With<GroundRingMarker>>();
    let handle = rings.single(world);
    assert_eq!(
        world
            .resource::<Assets<SelectionRingMaterial>>()
            .get(&handle.0)
            .ok_or("ring material")?
            .uniform
            .team_color,
        style.linear
    );
    let mut tethers =
        world.query_filtered::<&MeshMaterial3d<StandardMaterial>, With<AirTetherMarker>>();
    let handle = tethers.single(world);
    let material = world
        .resource::<Assets<StandardMaterial>>()
        .get(&handle.0)
        .ok_or("tether material")?;
    assert_eq!(material.base_color.to_linear(), style.color().to_linear());
    assert_eq!(material.emissive, style.color().to_linear() * 1.5);
    Ok(())
}

#[test]
fn indicator_ownership_changes_refresh_retained_material_handles() -> TestResult {
    let definition = definition()?;
    let mut instance = snapshot_fixture(&definition)?;
    instance.owner = PlayerId(0);
    instance.world_pos[2] = 2.0;
    instance.ground_z = 0.0;
    instance.selected = true;
    let palette = definition.palette;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(palette.clone())
        .insert_resource(IndicatorFixture(instance, palette.resolve(PlayerId(0))?))
        .init_resource::<PresentationStatus>()
        .add_event::<AppExit>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<SelectionRingMaterial>>()
        .add_systems(
            Startup,
            (
                indicators::setup_indicator_assets_system,
                spawn_indicator_fixture,
            )
                .chain(),
        )
        .add_systems(Update, indicators::update_indicators_system);
    app.update();
    let ring_count = app
        .world()
        .resource::<Assets<SelectionRingMaterial>>()
        .len();
    for owner in [PlayerId(0), PlayerId(2), PlayerId(7), PlayerId::NEUTRAL] {
        for mut bubble in app
            .world_mut()
            .query::<&mut BubbleInstance>()
            .iter_mut(app.world_mut())
        {
            bubble.0.owner = owner;
        }
        app.update();
        assert_indicators(&mut app, palette.resolve(owner)?)?;
        assert_eq!(
            app.world()
                .resource::<Assets<SelectionRingMaterial>>()
                .len(),
            ring_count
        );
    }
    Ok(())
}

fn events(owner: PlayerId) -> [CombatEventVisual; 5] {
    let from = [1.0, 2.0, 0.5];
    let to = [4.0, 5.0, 0.5];
    [
        CombatEventVisual::HitscanTracer {
            from,
            to,
            owner,
            source: sim_types::EffectSource::default(),
        },
        CombatEventVisual::ProjectileVisual {
            pos: from,
            vel: [1.0, 0.0, 0.0],
            owner,
            duration_secs: Some(0.5),
            source: sim_types::EffectSource::default(),
        },
        CombatEventVisual::BeamVisual {
            from,
            to,
            owner,
            is_lightning: true,
            source: sim_types::EffectSource::default(),
        },
        CombatEventVisual::MeleeSlashVisual {
            pos: from,
            angle_rad: 0.75,
            radius: 0.5,
            owner,
            source: sim_types::EffectSource::default(),
        },
        CombatEventVisual::BubbleShatter {
            pos: to,
            radius: 0.5,
            is_dome: false,
            owner,
            unit_type: sim_types::UnitTypeId::default(),
        },
    ]
}

fn effect_app(palette: &PlayerPalette, owner: PlayerId) -> App {
    let mut session = BattleSession::default();
    session.metadata.players.push(ParticipantMetadata {
        owner,
        label: String::new(),
        color: bevy_egui::egui::Color32::WHITE,
    });
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(palette.clone())
        .insert_resource(session)
        .insert_resource(client::LabState {
            current_tab: client::LabTab::LiveBattle,
            ..default()
        })
        .init_resource::<PresentationStatus>()
        .add_event::<AppExit>()
        .add_event::<CombatVisualEvent>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<CombatFxMaterial>>()
        .init_resource::<vfx::CombatFxMaterialPool>()
        .add_systems(Startup, vfx::setup_vfx_assets_system)
        .add_systems(Update, vfx::process_combat_events_system);
    app
}

#[test]
fn all_effects_preserve_sparse_source_hue_and_packed_angle() -> TestResult {
    let palette = definition()?.palette;
    for owner in [PlayerId(0), PlayerId(2), PlayerId(7), PlayerId::NEUTRAL] {
        let mut app = effect_app(&palette, owner);
        app.update();
        app.world_mut()
            .resource_mut::<Events<CombatVisualEvent>>()
            .send_batch(events(owner).map(CombatVisualEvent));
        app.update();
        let mut query = app
            .world_mut()
            .query::<(&vfx::CombatFxInstance, &vfx::CombatEffectOwner)>();
        let mut count = 0;
        for (instance, captured_owner) in query.iter(app.world()) {
            assert_eq!(captured_owner.0, owner);
            let uniform = &instance.uniform;
            assert_eq!(
                uniform.primary_color.truncate(),
                palette.resolve(owner)?.linear.truncate()
            );
            let expected_metadata = if uniform.fx_kind == 3 { 0.75_f32 } else { 0.0 };
            assert_eq!(
                uniform.primary_color.w.to_bits(),
                expected_metadata.to_bits()
            );
            count += 1;
        }
        assert_eq!(count, 21);
    }
    Ok(())
}

#[test]
fn unsupported_event_owner_has_visible_failure_and_nonzero_exit() -> TestResult {
    let palette = definition()?.palette;
    let mut app = effect_app(&palette, PlayerId(0));
    app.update();
    app.world_mut()
        .resource_mut::<Events<CombatVisualEvent>>()
        .send(CombatVisualEvent(events(PlayerId(8))[0].clone()));
    app.update();
    assert!(app
        .world()
        .resource::<PresentationStatus>()
        .failure_text()
        .ok_or("missing failure")?
        .contains('8'));
    assert!(app
        .world()
        .resource::<Events<AppExit>>()
        .get_cursor()
        .read(app.world().resource::<Events<AppExit>>())
        .any(AppExit::is_error));
    assert_eq!(
        app.world_mut()
            .query::<&MeshMaterial3d<CombatFxMaterial>>()
            .iter(app.world())
            .count(),
        0
    );
    Ok(())
}

#[test]
fn removed_simulation_source_still_supplies_every_shatter_child_owner() -> TestResult {
    let definition = definition()?;
    let mut simulation = definition.instantiate()?;
    let (&source, actor) = simulation.entities.iter().next().ok_or("real actor")?;
    let owner = actor.owner;
    simulation.entities.get_mut(&source).ok_or("source")?.hp = sim_types::SimScalar::ZERO;
    simulation.reap_dead_and_emit_shatter();
    assert!(!simulation.entities.contains_key(&source));
    let events = simulation.build_render_snapshot().combat_events;
    assert_eq!(events.len(), 1);
    let mut app = effect_app(&definition.palette, owner);
    app.update();
    app.world_mut()
        .resource_mut::<Events<CombatVisualEvent>>()
        .send_batch(events.into_iter().map(CombatVisualEvent));
    app.update();
    let mut query = app.world_mut().query::<&vfx::CombatEffectOwner>();
    let captured = query
        .iter(app.world())
        .map(|entry| entry.0)
        .collect::<Vec<_>>();
    assert_eq!(captured, vec![owner; 17]);
    Ok(())
}
