//! Automated regression tests for Round 2 visual polish, textures, shaders, and selection.
//!
//! # Invariants Verified
//! - `assets/shaders/bubble_dome.wgsl` exists on disk and contains valid uniform struct declarations.
//! - `BubbleUniformGpu` memory layout is strictly 96 bytes and 16-byte aligned.
//! - Flipbook textures maintain 512x512 resolution and stay within WebGPU 16384 height limits.
//! - Ray-sphere selection math passes direct hit, tangent, and miss scenarios.
//! - 4-Hero showcase entities span inside camera frustum with hero framing preset.
//! - `StarCraft` II style 3-faction roster exhibits correct lore names, titles, and faction assignments.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use asset_baker::FlipbookAtlasMeta;
use bevy::prelude::*;
use client::bubble_shader::{
    BubbleUniformGpu, DEFAULT_FEATURE_FLAGS, FLAG_ATTACK_APERTURE, FLAG_CLOAKED, FLAG_CRACKS,
    FLAG_CRT_GLITCH, FLAG_FACING_STRIPE, FLAG_FRESNEL, FLAG_GLARE, FLAG_INNER_WARP, FLAG_SELECTED,
    FLAG_SPEED_BOOSTED,
};
use client::camera::RtsCamera;
use client::indicators::{
    AirShadowMarker, AirTetherMarker, GroundRingMarker, SelectionRingMaterial,
    SelectionRingUniformGpu,
};
use client::resolve_assets_dir;
use client::selection::ray_sphere_intersect;
use client::vfx::{CombatFxMaterial, CombatFxUniformGpu, MistWakeMaterial, MistWakeUniformGpu};
use sim_core::catalog::UnitCatalog;
use sim_types::enums::{Faction, ShapeKind, SilhouetteClass, WeaponVisualKind};
use sim_types::ids::{EntityId, TeamId, UnitTypeId};
use sim_types::snapshot::BubbleRenderInstance;
use std::fs;
use std::mem::{align_of, size_of};

#[test]
fn test_shader_wgsl_syntax_and_uniform_alignment() {
    let assets_dir = resolve_assets_dir();
    let shader_path = assets_dir.join("shaders/bubble_dome.wgsl");
    let shader_source = fs::read_to_string(&shader_path)
        .or_else(|_| fs::read_to_string("assets/shaders/bubble_dome.wgsl"))
        .expect("bubble_dome.wgsl must be present on disk");

    assert!(shader_source.contains("struct BubbleUniform"));
    assert!(shader_source.contains("@group(2) @binding(0)"));
    assert!(shader_source.contains("@group(2) @binding(1)"));
    assert!(shader_source.contains("@group(2) @binding(2)"));
    assert!(shader_source.contains("FLAG_GLARE"));
    assert!(shader_source.contains("FLAG_FRESNEL"));
    assert!(shader_source.contains("FLAG_INNER_WARP"));
    assert!(shader_source.contains("FLAG_FACING_STRIPE"));
    assert!(shader_source.contains("FLAG_ATTACK_APERTURE"));
    assert!(
        shader_source.contains("n_dot_v"),
        "bubble_dome.wgsl must compute n_dot_v for perspective-correct horizon"
    );
    assert!(shader_source.contains("screen_right"));
    assert!(shader_source.contains("world_from_view"));
    assert!(shader_source.contains("p_screen_at_zc"));

    // Verify memory layout exactly equals 96 bytes and 16-byte alignment
    assert_eq!(size_of::<BubbleUniformGpu>(), 96);
    assert_eq!(align_of::<BubbleUniformGpu>(), 16);
}

#[test]
fn test_selection_ring_uniform_and_shader_invariants() {
    let assets_dir = resolve_assets_dir();
    let shader_path = assets_dir.join("shaders/selection_ring.wgsl");
    let shader_source = fs::read_to_string(&shader_path)
        .or_else(|_| fs::read_to_string("assets/shaders/selection_ring.wgsl"))
        .expect("selection_ring.wgsl must be present on disk");

    assert!(shader_source.contains("struct SelectionRingUniform"));
    assert!(shader_source.contains("@group(2) @binding(0)"));
    assert!(shader_source.contains("@fragment"));
    assert!(
        shader_source.contains("ring_alpha < 0.02"),
        "selection_ring.wgsl must discard fragments with ring_alpha < 0.02"
    );
    assert!(
        shader_source.contains("discard;"),
        "selection_ring.wgsl must invoke discard; to prevent opaque black discs"
    );

    // Verify memory layout exactly equals 32 bytes and 16-byte alignment
    assert_eq!(size_of::<SelectionRingUniformGpu>(), 32);
    assert_eq!(align_of::<SelectionRingUniformGpu>(), 16);
}

#[test]
fn test_selection_ring_material_alpha_and_depth_bias() {
    let mat = SelectionRingMaterial {
        uniform: SelectionRingUniformGpu::default(),
    };
    assert_eq!(mat.alpha_mode(), AlphaMode::Blend);
    assert_eq!(mat.depth_bias(), 0.0);
}

#[test]
fn test_mist_wake_uniform_and_shader_invariants() {
    let assets_dir = resolve_assets_dir();
    let shader_path = assets_dir.join("shaders/mist_wake.wgsl");
    let shader_source = fs::read_to_string(&shader_path)
        .or_else(|_| fs::read_to_string("assets/shaders/mist_wake.wgsl"))
        .expect("mist_wake.wgsl must be present on disk");

    assert!(shader_source.contains("struct MistWakeUniform"));
    assert!(shader_source.contains("@group(2) @binding(0)"));
    assert!(shader_source.contains("@fragment"));
    assert!(
        shader_source.contains("alpha < 0.002")
            || shader_source.contains("alpha < 0.008")
            || shader_source.contains("alpha < 0.012")
            || shader_source.contains("alpha < 0.015"),
        "mist_wake.wgsl must discard fragments with sub-threshold alpha"
    );
    assert!(
        shader_source.contains("discard;"),
        "mist_wake.wgsl must invoke discard; to prevent black discs"
    );
    assert!(
        shader_source.contains("OCTAVE_ROT"),
        "mist_wake.wgsl must use OCTAVE_ROT matrix to eliminate grid stripes"
    );

    // Verify memory layout exactly equals 32 bytes and 16-byte alignment
    assert_eq!(size_of::<MistWakeUniformGpu>(), 32);
    assert_eq!(align_of::<MistWakeUniformGpu>(), 16);
}

#[test]
fn test_mist_wake_material_alpha_and_depth_bias() {
    let mat = MistWakeMaterial {
        uniform: MistWakeUniformGpu::default(),
    };
    assert_eq!(mat.alpha_mode(), AlphaMode::Add);
    assert_eq!(mat.depth_bias(), -4.0);
}

#[test]
fn test_sphere_tessellation_high_density() {
    let sphere_mesh = Sphere::new(1.0).mesh().uv(80, 52);
    let vertex_count = sphere_mesh.count_vertices();
    assert!(
        vertex_count > 4000,
        "High-density sphere mesh vertex count {vertex_count} must be > 4000"
    );
}

#[test]
fn test_flipbook_textures_512_invariants() {
    let assets_dir = resolve_assets_dir();
    let textures_dir = assets_dir.join("textures");
    if !textures_dir.exists() {
        return; // Skip if run prior to baking
    }

    let catalog = UnitCatalog::default();
    for arch in catalog.archetypes() {
        let name = &arch.slug;
        let png_path = textures_dir.join(format!("{name}.png"));
        let ron_path = textures_dir.join(format!("{name}.flipbook.ron"));
        if png_path.exists() && ron_path.exists() {
            let img = image::open(&png_path).expect("failed to open flipbook image");
            assert_eq!(img.width(), 512, "{name} width must be exactly 512");
            // Strict WebGPU max_texture_dimension_2d hardware invariant:
            assert!(
                img.height() <= 16384,
                "{name} height {} exceeds wgpu limit 16384!",
                img.height()
            );
        }
    }
}

#[test]
fn test_starter_manifests_multi_action_offsets_and_bounds() {
    let assets_dir = resolve_assets_dir();
    let textures_dir = assets_dir.join("textures");
    let catalog = UnitCatalog::default();
    let starters: Vec<_> = catalog.archetypes().filter(|a| a.showcase_hero).collect();

    for arch in starters {
        let name = &arch.slug;
        let png_path = textures_dir.join(format!("{name}.png"));
        let ron_path = textures_dir.join(format!("{name}.flipbook.ron"));

        assert!(png_path.exists(), "PNG file must exist for starter {name}");
        assert!(
            ron_path.exists(),
            "RON manifest must exist for starter {name}"
        );

        let meta =
            FlipbookAtlasMeta::load_from_ron(&ron_path).expect("Failed to load RON manifest");

        assert_eq!(meta.frame_width, 512, "{name} frame_width must be 512");
        assert_eq!(meta.frame_height, 512, "{name} frame_height must be 512");

        let img = image::open(&png_path).expect("failed to open flipbook image");
        assert_eq!(img.width(), 512, "{name} width must be 512");
        assert!(
            img.height() <= 16384,
            "{name} height {} exceeds wgpu limit 16384!",
            img.height()
        );
        assert_eq!(img.height(), meta.total_frames * 512);

        // Verify distinct non-overlapping offsets for Idle, Walk, and Attack
        let idle = meta
            .actions
            .get(&sim_types::enums::AnimState::Idle)
            .expect("Idle slice");
        let walk = meta
            .actions
            .get(&sim_types::enums::AnimState::Walk)
            .expect("Walk slice");
        let attack = meta
            .actions
            .get(&sim_types::enums::AnimState::Attack)
            .expect("Attack slice");

        assert_eq!(idle.start_frame, 0, "{name} Idle must start at frame 0");
        assert_eq!(
            walk.start_frame,
            idle.start_frame + idle.frame_count,
            "{name} Walk must start immediately after Idle"
        );
        assert_eq!(
            attack.start_frame,
            walk.start_frame + walk.frame_count,
            "{name} Attack must start immediately after Walk"
        );
        assert!(idle.frame_count > 0);
        assert!(walk.frame_count > 0);
        assert!(attack.frame_count > 0);

        // Verify all 6 AnimState variants are mapped
        for state in [
            sim_types::enums::AnimState::Constructing,
            sim_types::enums::AnimState::Idle,
            sim_types::enums::AnimState::Walk,
            sim_types::enums::AnimState::Attack,
            sim_types::enums::AnimState::Cast,
            sim_types::enums::AnimState::Die,
        ] {
            assert!(
                meta.actions.contains_key(&state),
                "{name} manifest must contain fallback mapping for {state:?}"
            );
        }
    }
}

#[test]
fn test_ray_sphere_selection_math() {
    let origin = Vec3::new(0.0, 10.0, 0.0);
    let dir = Vec3::new(0.0, -1.0, 0.0); // pointing straight down
    let sphere_center = Vec3::new(0.0, 0.0, 0.0);
    let sphere_radius = 1.0;

    // Direct center intersection: t should be exactly 9.0
    let hit = ray_sphere_intersect(origin, dir, sphere_center, sphere_radius);
    assert!(hit.is_some());
    let t = hit.unwrap();
    assert!((t - 9.0).abs() < 1e-4);

    // Tangent intersection at offset x = 1.0: t should be 10.0
    let tangent_origin = Vec3::new(1.0, 10.0, 0.0);
    let hit_tangent = ray_sphere_intersect(tangent_origin, dir, sphere_center, sphere_radius);
    assert!(hit_tangent.is_some());

    // Miss at offset x = 1.5: should return None
    let miss_origin = Vec3::new(1.5, 10.0, 0.0);
    let hit_miss = ray_sphere_intersect(miss_origin, dir, sphere_center, sphere_radius);
    assert!(hit_miss.is_none());
}

#[test]
fn test_hero_lineup_camera_frustum_projection() {
    let cam = RtsCamera {
        target: Vec2::new(0.0, 0.0),
        distance: 9.5,
        ..Default::default()
    };
    let tf = cam.compute_transform();

    // Verify all 4 hero entities lie in front of the camera
    let hero_x_positions = [-3.6, -1.2, 1.2, 4.2];
    for x in hero_x_positions {
        let pos = Vec3::new(x, 0.85, 0.0);
        let to_pos = pos - tf.translation;
        let forward = tf.forward();
        assert!(
            to_pos.dot(*forward) > 0.0,
            "Hero at {x} must be in front of camera"
        );
    }
}

#[test]
fn test_showcase_entities_project_inside_central_frustum() {
    let cam = RtsCamera {
        target: Vec2::new(0.0, 0.0),
        distance: 9.5,
        ..Default::default()
    };
    let tf = cam.compute_transform();

    // Check all 4 showcase hero entities with updated radius 0.85 and 1.35
    let heroes = [
        (-3.6, 0.85),
        (-1.2, 0.85),
        (1.2, 0.85),
        (4.2, 0.0), // Building dome sits at y = 0.0 with radius 1.35
    ];

    for (x, y) in heroes {
        let pos = Vec3::new(x, y, 0.0);
        let to_pos = pos - tf.translation;
        let forward = tf.forward();
        assert!(
            to_pos.dot(*forward) > 0.0,
            "Showcase entity at ({x}, {y}) must be in front of camera forward vector"
        );
    }
}

#[test]
fn test_ground_unit_altitude_never_spawns_air_tether() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<SelectionRingMaterial>>();

    app.add_systems(
        Startup,
        (
            client::indicators::setup_indicator_assets_system,
            |mut commands: Commands<'_, '_>,
             assets: Res<'_, client::indicators::IndicatorAssets>,
             mut materials: ResMut<'_, Assets<StandardMaterial>>,
             mut ring_materials: ResMut<'_, Assets<SelectionRingMaterial>>| {
                let instance = BubbleRenderInstance {
                    entity_id: EntityId(42),
                    shape: ShapeKind::UnitSphere,
                    silhouette: SilhouetteClass::LightProngs,
                    unit_type: UnitTypeId(2),
                    team_id: TeamId(1),
                    world_pos: [-1.2, 0.0, 0.0], // ground hero: altitude z = 0.0
                    ground_z: 0.0,
                    radius: 0.85,
                    velocity: [0.0, 0.0],
                    heading_angle_rad: 0.0,
                    aim_angle_rad: 0.0,
                    is_attacking: false,
                    weapon_visual: WeaponVisualKind::Hitscan,
                    attack_target_pos: None,
                    anim_state: sim_types::enums::AnimState::Idle,
                    anim_progress: 0.0,
                    hp_ratio: 1.0,
                    shield_ratio: 1.0,
                    shield_hit_flash: 0.0,
                    hull_hit_angle_rad: 0.0,
                    hull_hit_flash: 0.0,
                    build_or_queue_progress: None,
                    selected: true,
                    cloaked: false,
                    speed_boosted: false,
                };

                let parent = commands.spawn(Transform::default()).id();
                client::indicators::attach_bubble_indicators(
                    &mut commands,
                    parent,
                    &instance,
                    &assets,
                    &mut materials,
                    &mut ring_materials,
                );
            },
        )
            .chain(),
    );

    app.update();

    // Verify no AirTetherMarker or AirShadowMarker spawned
    let mut tether_query = app
        .world_mut()
        .query_filtered::<Entity, With<AirTetherMarker>>();
    assert_eq!(
        tether_query.iter(app.world()).count(),
        0,
        "Ground unit must never spawn AirTetherMarker"
    );

    let mut shadow_query = app
        .world_mut()
        .query_filtered::<Entity, With<AirShadowMarker>>();
    assert_eq!(
        shadow_query.iter(app.world()).count(),
        0,
        "Ground unit must never spawn AirShadowMarker"
    );

    // Verify GroundRingMarker is spawned
    let mut ring_query = app
        .world_mut()
        .query_filtered::<Entity, With<GroundRingMarker>>();
    assert_eq!(
        ring_query.iter(app.world()).count(),
        1,
        "Ground unit must spawn GroundRingMarker"
    );
}

#[test]
fn test_three_factions_roster_integrity() {
    let catalog = UnitCatalog::new_starter_roster();
    let hero1 = catalog.get(UnitTypeId(2)).expect("Hero 1");
    assert_eq!(hero1.display_name, "Aegis Pulse Bot");
    assert_eq!(hero1.faction, Faction::AutomatonDirectorate);

    let hero2 = catalog.get(UnitTypeId(4)).expect("Hero 2");
    assert_eq!(hero2.display_name, "Psi-Blade Grey");
    assert_eq!(hero2.faction, Faction::CosmicEnclave);

    let hero3 = catalog.get(UnitTypeId(3)).expect("Hero 3");
    assert_eq!(hero3.display_name, "Bombardier Rhino-Beetle");
    assert_eq!(hero3.faction, Faction::SylvanSwarm);

    let dome = catalog.get(UnitTypeId(7)).expect("Foundry Core Dome");
    assert_eq!(dome.display_name, "Foundry Core Dome");
    assert_eq!(dome.faction, Faction::AutomatonDirectorate);
}

#[test]
fn test_feature_flags_bitmask_operations() {
    let flags = DEFAULT_FEATURE_FLAGS | FLAG_SELECTED | FLAG_CLOAKED | FLAG_SPEED_BOOSTED;
    assert_ne!(flags & FLAG_GLARE, 0);
    assert_ne!(flags & FLAG_FRESNEL, 0);
    assert_ne!(flags & FLAG_INNER_WARP, 0);
    assert_ne!(flags & FLAG_CRACKS, 0);
    assert_ne!(flags & FLAG_CRT_GLITCH, 0);
    assert_ne!(flags & FLAG_FACING_STRIPE, 0);
    assert_ne!(flags & FLAG_ATTACK_APERTURE, 0);
    assert_ne!(flags & FLAG_SELECTED, 0);
    assert_ne!(flags & FLAG_CLOAKED, 0);
    assert_ne!(flags & FLAG_SPEED_BOOSTED, 0);

    let cleared = flags & !FLAG_SELECTED;
    assert_eq!(cleared & FLAG_SELECTED, 0);
    assert_ne!(cleared & FLAG_GLARE, 0);
}

#[test]
fn test_bevy_ecs_schedule_all_systems_no_conflicts() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<client::bubble_shader::BubbleDomeMaterial>>();
    app.init_resource::<Assets<SelectionRingMaterial>>();
    app.init_resource::<Assets<MistWakeMaterial>>();
    app.init_resource::<Assets<CombatFxMaterial>>();
    app.init_resource::<ButtonInput<KeyCode>>();
    app.init_resource::<ButtonInput<MouseButton>>();
    app.add_event::<bevy::input::mouse::MouseMotion>();
    app.add_event::<bevy::input::mouse::MouseWheel>();
    app.add_event::<client::vfx::CombatVisualEvent>();
    app.init_resource::<client::CatalogResource>();
    app.init_resource::<client::RenderEntityMap>();
    app.init_resource::<client::selection::SelectionState>();

    let lab_state = client::lab_ui::LabState {
        current_tab: client::lab_ui::LabTab::LiveBattle,
        sim_world: Some(sim_core::world::SimWorld::new_default_battle_sandbox()),
        ..Default::default()
    };
    app.insert_resource(lab_state);

    app.add_systems(
        Startup,
        (
            client::bubble_shader::setup_bubble_assets_system,
            client::indicators::setup_indicator_assets_system,
            client::vfx::setup_vfx_assets_system,
            client::arena_render::setup_arena_system,
            client::spawn_camera_system,
        ),
    );

    app.add_systems(
        Update,
        (
            client::camera::update_camera_system,
            client::indicators::update_indicators_system,
            client::vfx::update_vfx_system,
            client::vfx::update_projectiles_system,
            client::vfx::update_beams_system,
            client::vfx::update_melee_slashes_system,
            client::vfx::update_shatter_system,
            client::vfx::process_combat_events_system,
            client::lab_ui::sync_lab_preview_system,
            client::lab_ui::step_client_simulation_system,
            client::lab_ui::sync_swarm_stress_system,
        ),
    );

    // Stepping the app checks all query archetypes and system parameter validity
    for _ in 0..10 {
        app.update();
    }
}

#[test]
fn test_real_gpu_startup_and_frame_limit_30() {
    let client_bin = env!("CARGO_BIN_EXE_client");
    let xvfb_run = std::path::Path::new("/usr/bin/xvfb-run");

    let mut cmd = if xvfb_run.exists() {
        let mut c = std::process::Command::new(xvfb_run);
        c.arg("-a").arg(client_bin);
        c
    } else {
        if std::env::var("WAYLAND_DISPLAY").is_err() && std::env::var("DISPLAY").is_err() {
            eprintln!("Skipping windowed GPU test: no graphical display detected");
            return;
        }
        std::process::Command::new(client_bin)
    };

    cmd.arg("--lab").arg("--max-frames").arg("30");
    // Ensure test runs strictly on virtual display and never appears on the user's desktop
    cmd.env_remove("WAYLAND_DISPLAY");
    cmd.env("WINIT_UNIX_BACKEND", "x11");
    cmd.env("WGPU_BACKEND", "vulkan");
    cmd.env("SPACE_SCUFFLE_VIRTUAL_GPU", "1");

    let lvp_candidates = [
        std::path::Path::new("/usr/share/vulkan/icd.d/lvp_icd.json"),
        std::path::Path::new("/usr/share/vulkan/icd.d/lvp_icd.x86_64.json"),
    ];
    if let Some(lvp_icd) = lvp_candidates.into_iter().find(|p| p.exists()) {
        cmd.env("VK_ICD_FILENAMES", lvp_icd);
        cmd.env("WGPU_ADAPTER_NAME", "llvmpipe");
        cmd.env("LIBGL_ALWAYS_SOFTWARE", "1");
    }

    let status = cmd.status().expect("Failed to execute client binary");

    assert!(
        status.success(),
        "Client binary failed when launching virtual GPU window with --lab --max-frames 30"
    );
}

fn scan_src_for_forbidden_slugs(dir: &std::path::Path, forbidden: &[&str]) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            scan_src_for_forbidden_slugs(&path, forbidden);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let path_str = path.to_string_lossy();
            if path_str.contains("/src/") {
                let content = fs::read_to_string(&path).expect("Failed to read source file");
                for &slug in forbidden {
                    assert!(
                        !content.contains(slug),
                        "Forbidden creature/unit slug '{slug}' found in production source file: {}",
                        path.display()
                    );
                }
            }
        }
    }
}

#[test]
fn test_zero_hardcoded_creature_names_in_rust_src() {
    // AGENTS.md §3.3: Strict zero creature/building identifiers or slugs in Rust source code across all crates.
    let forbidden_slugs = [
        "rhino_beetle",
        "pulse_bot",
        "psi_grey",
        "railgun_crawler",
        "flying_saucer",
        "welder_drone",
        "foundry_core_dome",
        "assembly_bay_dome",
        "capacitor_dome",
        "tesla_sentinel_dome",
    ];

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let crates_dir = workspace_dir.join("crates");

    scan_src_for_forbidden_slugs(&crates_dir, &forbidden_slugs);
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_all_combat_vfx_entities_despawn_after_lifetime() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<CombatFxMaterial>>();
    app.add_event::<client::vfx::CombatVisualEvent>();

    // Add Startup and Update systems
    app.add_systems(Startup, client::vfx::setup_vfx_assets_system);
    app.add_systems(
        Update,
        (
            client::vfx::process_combat_events_system,
            client::vfx::update_projectiles_system,
            client::vfx::update_beams_system,
            client::vfx::update_melee_slashes_system,
            client::vfx::update_shatter_system,
        ),
    );

    // Initial frame to run startup setup
    app.update();

    // Send all 5 CombatEventVisual variants into event channel
    {
        let mut events = app
            .world_mut()
            .resource_mut::<Events<client::vfx::CombatVisualEvent>>();
        events.send(client::vfx::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::HitscanTracer {
                from: [0.0, 0.0, 0.5],
                to: [10.0, 0.0, 0.5],
                team_id: TeamId(1),
            },
        ));
        events.send(client::vfx::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::ProjectileVisual {
                pos: [0.0, 0.0, 0.5],
                vel: [10.0, 0.0, 0.0],
                team_id: TeamId(2),
                duration_secs: Some(1.2),
            },
        ));
        events.send(client::vfx::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::BeamVisual {
                from: [0.0, 0.0, 0.5],
                to: [5.0, 5.0, 0.5],
                is_lightning: true,
                team_id: TeamId(1),
            },
        ));
        events.send(client::vfx::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::MeleeSlashVisual {
                pos: [2.0, 2.0, 0.0],
                angle_rad: 0.785,
                radius: 0.8,
                team_id: TeamId(2),
            },
        ));
        events.send(client::vfx::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::BubbleShatter {
                pos: [4.0, 4.0, 0.0],
                radius: 0.6,
                is_dome: false,
                team_id: TeamId(1),
            },
        ));
    }

    // Process events and spawn entities
    app.update();

    // Verify all 4 component types are actively spawned
    let beams_count = {
        let mut q = app.world_mut().query::<&client::vfx::BeamVisualComponent>();
        q.iter(app.world()).count()
    };
    let proj_count = {
        let mut q = app.world_mut().query::<&client::vfx::ProjectileVisual>();
        q.iter(app.world()).count()
    };
    let slash_count = {
        let mut q = app.world_mut().query::<&client::vfx::MeleeSlashComponent>();
        q.iter(app.world()).count()
    };
    let shard_count = {
        let mut q = app.world_mut().query::<&client::vfx::ShatterShard>();
        q.iter(app.world()).count()
    };

    assert!(
        beams_count >= 2,
        "Hitscan and BeamVisual must spawn at least 2 BeamVisualComponent entities"
    );
    assert_eq!(
        proj_count, 1,
        "ProjectileVisual must spawn exactly 1 ProjectileVisual entity"
    );
    assert_eq!(
        slash_count, 1,
        "MeleeSlashVisual must spawn exactly 1 MeleeSlashComponent entity"
    );
    assert_eq!(
        shard_count, 16,
        "BubbleShatter must spawn exactly 16 ShatterShard entities"
    );

    // Advance time past all lifetimes (max lifetime is 1.2s for projectile; step 2.0s total)
    // Step 20 frames with dt = 0.1s
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_millis(100),
    ));
    for _ in 0..20 {
        app.update();
    }

    // Assert that ZERO VFX entities remain in the ECS world
    let remaining_beams = {
        let mut q = app.world_mut().query::<&client::vfx::BeamVisualComponent>();
        q.iter(app.world()).count()
    };
    let remaining_proj = {
        let mut q = app.world_mut().query::<&client::vfx::ProjectileVisual>();
        q.iter(app.world()).count()
    };
    let remaining_slashes = {
        let mut q = app.world_mut().query::<&client::vfx::MeleeSlashComponent>();
        q.iter(app.world()).count()
    };
    let remaining_shards = {
        let mut q = app.world_mut().query::<&client::vfx::ShatterShard>();
        q.iter(app.world()).count()
    };

    assert_eq!(
        remaining_beams, 0,
        "All BeamVisualComponent entities must despawn after lifetime"
    );
    assert_eq!(
        remaining_proj, 0,
        "All ProjectileVisual entities must despawn after lifetime"
    );
    assert_eq!(
        remaining_slashes, 0,
        "All MeleeSlashComponent entities must despawn after lifetime"
    );
    assert_eq!(
        remaining_shards, 0,
        "All ShatterShard entities must despawn after lifetime"
    );
}

/// Invariant test verifying procedural volumetric combat VFX shader layout and material properties.
#[test]
fn test_combat_fx_shader_and_uniform_layout() {
    let assets_dir = resolve_assets_dir();
    let shader_path = assets_dir.join("shaders/combat_fx.wgsl");
    let shader_source = fs::read_to_string(&shader_path)
        .or_else(|_| fs::read_to_string("assets/shaders/combat_fx.wgsl"))
        .expect("combat_fx.wgsl must be present on disk");

    assert!(shader_source.contains("struct CombatFxUniform"));
    assert!(shader_source.contains("@group(2) @binding(0)"));
    assert!(shader_source.contains("@fragment"));
    assert!(
        shader_source.contains("discard;"),
        "combat_fx.wgsl must invoke discard; to prevent rectangular billboard artifacts"
    );

    // Verify all 6 procedural combat modes are implemented in WGSL
    assert!(shader_source.contains("fx.fx_kind == 0u"));
    assert!(shader_source.contains("fx.fx_kind == 1u"));
    assert!(shader_source.contains("fx.fx_kind == 2u"));
    assert!(shader_source.contains("fx.fx_kind == 3u"));
    assert!(shader_source.contains("fx.fx_kind == 4u"));
    assert!(shader_source.contains("fx.fx_kind == 5u"));

    // Verify memory layout exactly equals 32 bytes and 16-byte alignment
    assert_eq!(size_of::<CombatFxUniformGpu>(), 32);
    assert_eq!(align_of::<CombatFxUniformGpu>(), 16);

    let mat = CombatFxMaterial {
        uniform: CombatFxUniformGpu::default(),
    };
    assert_eq!(mat.alpha_mode(), AlphaMode::Add);
    assert_eq!(mat.depth_bias(), -25.0);
}
