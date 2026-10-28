//! Combat event integration coverage for additive light-card shadow classification.
//!
//! Runs the production asset setup, spawning and lifetime systems. Every actual
//! material branch must produce non-shadow-casting cards; ordinary opaque meshes
//! retain Bevy's default shadow behavior. Native captures verify the final image.

use std::time::Duration;

use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use client::vfx::{
    process_combat_events_system, setup_vfx_assets_system, update_beams_system,
    update_melee_slashes_system, update_projectiles_system, update_shatter_system,
    CombatFxMaterial, CombatLightEffect, CombatVisualEvent, VfxSharedAssets,
};
use sim_types::ids::PlayerId;
use sim_types::snapshot::CombatEventVisual;

fn combat_events(owner: PlayerId) -> [CombatEventVisual; 6] {
    let from = [2.0, 3.0, 1.0];
    let to = [8.0, 7.0, 1.0];
    [
        CombatEventVisual::HitscanTracer { from, to, owner },
        CombatEventVisual::ProjectileVisual {
            pos: from,
            vel: [3.0, 0.0, 0.0],
            owner,
            duration_secs: Some(0.5),
        },
        CombatEventVisual::BeamVisual {
            from,
            to,
            owner,
            is_lightning: false,
        },
        CombatEventVisual::BeamVisual {
            from,
            to,
            owner,
            is_lightning: true,
        },
        CombatEventVisual::MeleeSlashVisual {
            pos: from,
            angle_rad: 0.5,
            radius: 0.8,
            owner,
        },
        CombatEventVisual::BubbleShatter {
            pos: to,
            radius: 0.8,
            is_dome: false,
            owner,
        },
    ]
}

fn assert_light_cards(world: &mut World) -> Result<(), &'static str> {
    let mut query = world.query::<(
        &MeshMaterial3d<CombatFxMaterial>,
        Has<CombatLightEffect>,
        Has<NotShadowCaster>,
    )>();
    let materials = world.resource::<Assets<CombatFxMaterial>>();
    let mut kinds = [0; 6];
    for (handle, classified, shadowless) in query.iter(world) {
        assert!(
            classified,
            "combat card is missing its light classification"
        );
        assert!(shadowless, "additive card would cast an opaque quad shadow");
        let material = materials
            .get(&handle.0)
            .ok_or("production combat card references a missing material")?;
        assert_eq!(material.alpha_mode(), AlphaMode::Add);
        kinds[material.uniform.fx_kind as usize] += 1;
    }
    // Laser, hitscan, lightning, slash, projectile, central burst + 16 shards.
    assert_eq!(kinds, [1, 1, 1, 1, 1, 17]);
    Ok(())
}

#[test]
fn every_production_light_card_remains_shadowless_through_update(
) -> Result<(), Box<dyn std::error::Error>> {
    let definition = client::runtime_battle::BattleDefinition::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;
    let owner = definition.palette.primary();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(definition.palette)
        .init_resource::<client::player_presentation::PresentationStatus>()
        .init_resource::<client::LabState>()
        .init_resource::<client::battle_pipeline::session::BattleSession>()
        .add_event::<AppExit>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<CombatFxMaterial>>()
        .add_event::<CombatVisualEvent>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .add_systems(Startup, setup_vfx_assets_system)
        .add_systems(
            Update,
            (
                process_combat_events_system,
                update_projectiles_system,
                update_beams_system,
                update_melee_slashes_system,
                update_shatter_system,
            )
                .chain(),
        );
    app.world_mut()
        .spawn((Camera3d::default(), Transform::from_xyz(10.0, 18.0, 20.0)));
    app.update();
    let mesh = app
        .world()
        .resource::<VfxSharedAssets>()
        .tracer_mesh
        .clone();
    let material = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial::default());
    let opaque = app
        .world_mut()
        .spawn((Mesh3d(mesh), MeshMaterial3d(material)))
        .id();
    app.world_mut()
        .resource_mut::<Events<CombatVisualEvent>>()
        .send_batch(combat_events(owner).map(CombatVisualEvent));
    app.update();
    assert_light_cards(app.world_mut())?;
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        40,
    )));
    app.update();
    assert_light_cards(app.world_mut())?;
    assert!(!app.world().entity(opaque).contains::<NotShadowCaster>());
    assert!(!app.world().entity(opaque).contains::<CombatLightEffect>());
    Ok(())
}
