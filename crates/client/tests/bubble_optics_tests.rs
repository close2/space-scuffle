//! Authored optical settings, packed GPU offsets and real plugin schedule contracts.
//!
//! Renderer color correctness is evaluated by actual capture cases; these tests establish
//! that the production configuration reaches materials without changing snapshot or media state.

use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use client::bubble_optics::{BubbleOptics, BubbleOpticsPlugin, OpticsError};
use client::{BubbleDomeMaterial, BubbleUniformGpu, ResolvedAssetsRoot};
use std::mem::{align_of, offset_of, size_of};

fn root() -> Result<ResolvedAssetsRoot, Box<dyn std::error::Error>> {
    Ok(ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .canonicalize()?,
    )?)
}

#[test]
fn shipped_style_and_exact_uniform_offsets_match() -> Result<(), Box<dyn std::error::Error>> {
    let style = BubbleOptics::load(&root()?.as_path().join("catalog/bubble_optics.ron"))?;
    let mut uniform = BubbleUniformGpu::default();
    style.apply(&mut uniform, Vec4::new(0.0, 1.0, 0.0, 0.5));
    assert_eq!(
        style.uniform_lanes(),
        [
            uniform.optics_media,
            uniform.optics_glare,
            uniform.optics_signals
        ]
    );
    assert_eq!(size_of::<BubbleUniformGpu>(), 160);
    assert_eq!(align_of::<BubbleUniformGpu>(), 16);
    assert_eq!(offset_of!(BubbleUniformGpu, frame_index), 92);
    assert_eq!(offset_of!(BubbleUniformGpu, optics_media), 96);
    assert_eq!(offset_of!(BubbleUniformGpu, optics_glare), 112);
    assert_eq!(offset_of!(BubbleUniformGpu, optics_signals), 128);
    assert_eq!(offset_of!(BubbleUniformGpu, optics_light), 144);
    Ok(())
}

#[test]
fn invalid_authored_settings_fail_explicitly() {
    let style = BubbleOptics {
        reflectance: f32::NAN,
        ..default()
    };
    assert!(matches!(
        style.validate(),
        Err(OpticsError::Invalid("reflectance"))
    ));
    let style = BubbleOptics {
        hit_spread_radians: 0.0,
        ..default()
    };
    assert!(style.validate().is_err());
    let style = BubbleOptics {
        media_exposure_stops: 3.0,
        ..default()
    };
    assert!(style.validate().is_err());
    let missing = std::path::Path::new("/nonexistent/space-scuffle-optics.ron");
    assert!(matches!(
        BubbleOptics::load(missing),
        Err(OpticsError::Read { .. })
    ));
    assert!(ron::from_str::<BubbleOptics>("(invented: 1.0)").is_err());
}

#[derive(Resource)]
struct ReplacedMaterial(Handle<BubbleDomeMaterial>);

// Bevy system parameters must be owned SystemParam values.
#[allow(clippy::needless_pass_by_value)]
fn replace_snapshot(
    target: Res<'_, ReplacedMaterial>,
    mut materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
) {
    if let Some(material) = materials.get_mut(&target.0) {
        material.uniform = BubbleUniformGpu {
            frame_index: 7,
            selected: 1,
            shield_hit_flash: 0.5,
            ..default()
        };
    }
}

#[test]
fn production_schedule_reapplies_authored_lanes_after_snapshot_replacement(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_event::<AppExit>()
        .insert_resource(root()?)
        .init_resource::<Assets<BubbleDomeMaterial>>()
        .add_plugins(BubbleOpticsPlugin)
        .add_systems(Update, replace_snapshot);
    let texture = Handle::<Image>::weak_from_u128(32);
    let make_material = || BubbleDomeMaterial {
        uniform: BubbleUniformGpu::default(),
        video_texture: Some(texture.clone()),
        alpha_mode: AlphaMode::Blend,
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<BubbleDomeMaterial>>()
        .add(make_material());
    app.world_mut().spawn(MeshMaterial3d(handle.clone()));
    app.insert_resource(ReplacedMaterial(handle.clone()));
    app.update();
    app.world_mut()
        .resource_mut::<BubbleOptics>()
        .media_exposure_stops = -0.5;
    let second = app
        .world_mut()
        .resource_mut::<Assets<BubbleDomeMaterial>>()
        .add(make_material());
    app.world_mut().spawn(MeshMaterial3d(second.clone()));
    for _ in 0..5 {
        app.update();
        let materials = app.world().resource::<Assets<BubbleDomeMaterial>>();
        assert_eq!(materials.len(), 2);
        let first = materials.get(&handle).ok_or("first material lost")?;
        assert_eq!(first.uniform.optics_media.x, -0.5);
        assert_eq!(first.uniform.frame_index, 7);
        assert_eq!(first.uniform.selected, 1);
        assert_eq!(first.uniform.shield_hit_flash, 0.5);
        assert_eq!(first.video_texture.as_ref(), Some(&texture));
        assert_eq!(
            materials
                .get(&second)
                .ok_or("second material lost")?
                .uniform
                .optics_media
                .x,
            -0.5
        );
    }
    Ok(())
}

#[test]
fn unchanged_shared_materials_emit_no_spurious_asset_changes(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_event::<AppExit>()
        .add_event::<AssetEvent<BubbleDomeMaterial>>()
        .insert_resource(root()?)
        .init_resource::<Assets<BubbleDomeMaterial>>()
        .add_plugins(BubbleOpticsPlugin)
        .add_systems(Last, Assets::<BubbleDomeMaterial>::asset_events);
    let handle = app
        .world_mut()
        .resource_mut::<Assets<BubbleDomeMaterial>>()
        .add(BubbleDomeMaterial {
            uniform: BubbleUniformGpu::default(),
            video_texture: None,
            alpha_mode: AlphaMode::Blend,
        });
    app.world_mut().spawn(MeshMaterial3d(handle.clone()));
    app.world_mut().spawn(MeshMaterial3d(handle.clone()));
    app.update();
    app.world_mut()
        .resource_mut::<Events<AssetEvent<BubbleDomeMaterial>>>()
        .clear();
    app.update();
    assert!(app
        .world()
        .resource::<Events<AssetEvent<BubbleDomeMaterial>>>()
        .is_empty());
    app.world_mut()
        .resource_mut::<BubbleOptics>()
        .media_exposure_stops = -0.25;
    app.update();
    let events = app
        .world()
        .resource::<Events<AssetEvent<BubbleDomeMaterial>>>();
    let mut cursor = events.get_cursor();
    assert_eq!(
        cursor
            .read(events)
            .filter(|event| matches!(event, AssetEvent::Modified { .. }))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn parented_directional_light_uses_propagated_world_orientation(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .add_event::<AppExit>()
        .insert_resource(root()?)
        .init_resource::<Assets<BubbleDomeMaterial>>()
        .add_plugins(BubbleOpticsPlugin);
    let rotation = Quat::from_rotation_y(0.7);
    let mut visible = ViewVisibility::HIDDEN;
    visible.set();
    app.world_mut()
        .spawn(Transform::from_rotation(rotation))
        .with_children(|parent| {
            parent.spawn((
                DirectionalLight {
                    illuminance: 10_000.0,
                    ..default()
                },
                Transform::default(),
                visible,
            ));
        });
    let handle = app
        .world_mut()
        .resource_mut::<Assets<BubbleDomeMaterial>>()
        .add(BubbleDomeMaterial {
            uniform: BubbleUniformGpu::default(),
            video_texture: None,
            alpha_mode: AlphaMode::Blend,
        });
    app.world_mut().spawn(MeshMaterial3d(handle.clone()));
    // A hidden brighter light and a visible light on another render layer must not
    // replace the light that actually illuminates this layer-zero shell.
    app.world_mut().spawn((
        DirectionalLight {
            illuminance: 100_000.0,
            ..default()
        },
        Transform::default(),
        ViewVisibility::HIDDEN,
    ));
    app.world_mut().spawn((
        DirectionalLight {
            illuminance: 100_000.0,
            ..default()
        },
        Transform::default(),
        visible,
        RenderLayers::layer(1),
    ));
    app.update();
    let actual = app
        .world()
        .resource::<Assets<BubbleDomeMaterial>>()
        .get(&handle)
        .ok_or("material lost")?
        .uniform
        .optics_light;
    assert!((actual.truncate() - rotation * Vec3::Z).length() < 1.0e-6);
    assert_eq!(actual.w, 0.5);
    Ok(())
}

#[test]
fn shipped_wgsl_uniform_member_offsets_match_rust_layout() -> Result<(), Box<dyn std::error::Error>>
{
    // Parse the real declaration, not a handwritten WGSL copy. Imports/functions are
    // exercised by native pipeline captures; this test checks the declaration's ABI.
    let shader = std::fs::read_to_string(root()?.as_path().join("shaders/bubble_dome.wgsl"))?;
    let (_, body) = shader
        .split_once("struct BubbleUniform {")
        .ok_or("missing real BubbleUniform declaration")?;
    let (body, _) = body
        .split_once("};")
        .ok_or("unterminated real BubbleUniform declaration")?;
    let declaration = format!("struct BubbleUniform {{ {body} }};");
    let module = naga::front::wgsl::parse_str(&declaration)?;
    let uniform_type = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some("BubbleUniform"))
        .ok_or("parsed declaration missing")?;
    let naga::TypeInner::Struct { members, span } = &uniform_type.1.inner else {
        return Err("uniform is not a struct".into());
    };
    macro_rules! offsets {
        ($($field:ident),+ $(,)?) => { [$( (stringify!($field), offset_of!(BubbleUniformGpu, $field)) ),+] };
    }
    let expected = offsets![
        team_color,
        world_pos,
        radius,
        shape_kind,
        heading_angle_rad,
        aim_angle_rad,
        is_attacking,
        anim_state,
        anim_progress,
        hp_ratio,
        shield_ratio,
        shield_hit_flash,
        hull_hit_angle_rad,
        hull_hit_flash,
        build_progress,
        selected,
        cloaked,
        feature_flags,
        frame_index,
        optics_media,
        optics_glare,
        optics_signals,
        optics_light
    ];
    assert_eq!(*span as usize, size_of::<BubbleUniformGpu>());
    assert_eq!(members.len(), expected.len());
    for (member, (name, offset)) in members.iter().zip(expected) {
        assert_eq!(member.name.as_deref(), Some(name));
        assert_eq!(
            member.offset as usize, offset,
            "WGSL/Rust offset mismatch for {name}"
        );
    }
    Ok(())
}
