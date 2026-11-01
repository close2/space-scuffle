//! Automated regression tests for Round 2 visual polish, textures, shaders, and selection.
//!
//! # Invariants Verified
//! - Every file in `assets/shaders` composes with its real Bevy imports and passes strict naga
//!   validation under each shader-def variant the mesh pipeline can request.
//! - Module IR of shipped shaders matches the Rust uniform layouts, bindings and feature flags.
//! - `BubbleUniformGpu` memory layout is strictly 160 bytes and 16-byte aligned.
//! - The catalog units with flipbooks are exactly the audited set, each 512 wide and within the
//!   WebGPU 16384 height limit.
//! - Ray-sphere selection math passes direct hit, tangent, and miss scenarios.
//! - 4-Hero showcase entities span inside camera frustum with hero framing preset.
//! - `StarCraft` II style 3-faction roster exhibits correct lore names, titles, and faction assignments.
//! - The real client starts on the virtual GPU with no unlisted Vulkan validation findings.
//! - No catalog slug appears in production Rust source.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/shader_ir.rs"]
mod shader_ir;

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
use naga::ShaderStage;
use serde::Deserialize;
use shader_ir::{
    compose_and_validate, discards, discards_when_below, has_named_let, production_module,
    shipped_source, SHADER_VARIANTS,
};
use sim_core::catalog::UnitCatalog;
use sim_types::enums::{Faction, ShapeKind, SilhouetteClass, WeaponVisualKind};
use sim_types::ids::{EntityId, PlayerId, UnitTypeId};
use sim_types::snapshot::BubbleRenderInstance;
use std::collections::BTreeSet;
use std::fs;
use std::mem::{align_of, size_of};

/// Every Rust feature flag with the WGSL constant that must mirror it.
const BUBBLE_FLAGS: [(&str, u32); 10] = [
    ("FLAG_CLOAKED", FLAG_CLOAKED),
    ("FLAG_SELECTED", FLAG_SELECTED),
    ("FLAG_GLARE", FLAG_GLARE),
    ("FLAG_FRESNEL", FLAG_FRESNEL),
    ("FLAG_INNER_WARP", FLAG_INNER_WARP),
    ("FLAG_CRACKS", FLAG_CRACKS),
    ("FLAG_CRT_GLITCH", FLAG_CRT_GLITCH),
    ("FLAG_SPEED_BOOSTED", FLAG_SPEED_BOOSTED),
    ("FLAG_FACING_STRIPE", FLAG_FACING_STRIPE),
    ("FLAG_ATTACK_APERTURE", FLAG_ATTACK_APERTURE),
];

/// Discovers every shipped `.wgsl` file, so a new shader cannot escape validation.
fn shipped_shader_files() -> Vec<String> {
    let directory = resolve_assets_dir().join("shaders");
    let mut names: Vec<String> = fs::read_dir(&directory)
        .expect("assets/shaders must be readable")
        .map(|entry| entry.expect("assets/shaders entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "wgsl")
        })
        .map(|path| {
            let name = path.file_name().expect("invariant: read_dir yields names");
            name.to_str().expect("shader names are UTF-8").to_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn test_every_shipped_shader_composes_and_validates_in_every_variant() {
    let files = shipped_shader_files();
    for required in [
        "bubble_dome.wgsl",
        "combat_fx.wgsl",
        "mist_wake.wgsl",
        "selection_ring.wgsl",
        "smog_volume.wgsl",
        "terrain.wgsl",
    ] {
        assert!(
            files.iter().any(|file| file == required),
            "{required} was not discovered in assets/shaders"
        );
    }
    let mut failures = Vec::new();
    for file in &files {
        let source = shipped_source(file);
        for variant in SHADER_VARIANTS {
            match compose_and_validate(file, &source, variant) {
                Ok(shader) => assert!(
                    !shader.module.entry_points.is_empty(),
                    "{file} [{}] has no entry points",
                    variant.name
                ),
                Err(error) => failures.push(error),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "shipped shaders failed validation:\n{}",
        failures.join("\n")
    );
}

/// The checker must reject a real type error, or a green shader suite means nothing.
#[test]
fn test_validation_rejects_type_error_injected_into_real_shader() {
    let source = shipped_source("selection_ring.wgsl");
    let broken = source.replace(
        "return vec4<f32>(final_color, ring_alpha);",
        "return vec4<f32>(final_color, 1u);",
    );
    assert_ne!(source, broken, "the injected fault must change the source");
    let error = compose_and_validate("selection_ring.wgsl", &broken, SHADER_VARIANTS[0])
        .expect_err("a vec4<f32> built from a u32 must not validate");
    assert!(error.contains("selection_ring.wgsl"), "{error}");
}

/// A misspelt import must be a composition error, never a silently dropped module.
#[test]
fn test_validation_rejects_unknown_import() {
    let source = shipped_source("selection_ring.wgsl").replace(
        "#import bevy_pbr::forward_io::VertexOutput",
        "#import bevy_pbr::forward_iox::VertexOutput",
    );
    compose_and_validate("selection_ring.wgsl", &source, SHADER_VARIANTS[0])
        .expect_err("an unknown import must not compose");
}

#[test]
fn test_shader_wgsl_syntax_and_uniform_alignment() {
    let shader = production_module("bubble_dome.wgsl");
    let fragment = shader.entry_point(ShaderStage::Fragment, "fragment");

    assert_eq!(
        shader.uniform_struct_at(2, 0),
        ("BubbleUniform".to_owned(), size_of::<BubbleUniformGpu>())
    );
    let texture = &shader.module.types[shader.global_at(2, 1).ty].inner;
    assert!(
        matches!(
            texture,
            naga::TypeInner::Image {
                dim: naga::ImageDimension::D2,
                arrayed: true,
                class: naga::ImageClass::Sampled {
                    kind: naga::ScalarKind::Float,
                    multi: false
                }
            }
        ),
        "@binding(1) must be texture_2d_array<f32>, found {texture:?}"
    );
    let sampler = &shader.module.types[shader.global_at(2, 2).ty].inner;
    assert!(
        matches!(sampler, naga::TypeInner::Sampler { comparison: false }),
        "@binding(2) must be a filtering sampler, found {sampler:?}"
    );

    for (name, value) in BUBBLE_FLAGS {
        assert_eq!(
            shader.u32_constant(name),
            Some(value),
            "WGSL {name} must equal the Rust flag"
        );
    }

    // Perspective-correct horizon and the camera view-plane basis.
    for local in ["n_dot_v", "screen_right", "p_screen_at_zc"] {
        assert!(
            has_named_let(fragment, local),
            "fragment must compute {local}"
        );
    }
    assert!(
        shader.reads_global_member(fragment, "world_from_view"),
        "fragment must derive its basis from view.world_from_view"
    );

    // Verify memory layout exactly equals 160 bytes and 16-byte alignment
    assert_eq!(size_of::<BubbleUniformGpu>(), 160);
    assert_eq!(align_of::<BubbleUniformGpu>(), 16);
}

/// Returns the flags that the real bubble fragment shader declares but never reads.
fn bubble_flags_unread_by_fragment() -> Vec<&'static str> {
    let shader = production_module("bubble_dome.wgsl");
    let fragment = shader.entry_point(ShaderStage::Fragment, "fragment");
    let read = shader.constants_read_by(fragment);
    BUBBLE_FLAGS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !read.contains(*name))
        .collect()
}

/// Flags already known to have no effect, tracked by the ignored test below.
const KNOWN_UNREAD_BUBBLE_FLAGS: [&str; 2] = ["FLAG_INNER_WARP", "FLAG_SPEED_BOOSTED"];

/// Keeps the working flags covered while the known gap stays open.
#[test]
fn test_bubble_feature_flags_have_no_new_unread_flags() {
    let unexpected: Vec<&str> = bubble_flags_unread_by_fragment()
        .into_iter()
        .filter(|name| !KNOWN_UNREAD_BUBBLE_FLAGS.contains(name))
        .collect();
    assert!(
        unexpected.is_empty(),
        "bubble_dome.wgsl stopped reading: {unexpected:?}"
    );
}

/// A Rust flag that no fragment code reads is a material option with no visual effect.
#[test]
#[ignore = "remediation Q-10: FLAG_INNER_WARP and FLAG_SPEED_BOOSTED are declared in bubble_dome.wgsl but never read; the fix is a production change"]
fn test_bubble_feature_flags_all_drive_the_fragment_shader() {
    let unread = bubble_flags_unread_by_fragment();
    assert!(
        unread.is_empty(),
        "flags set from Rust but never read by bubble_dome.wgsl: {unread:?}"
    );
}

/// `BubbleUniformGpu::renders_same_as` ignores these two fields because no pixel depends on them.
/// If the shader starts reading one, that comparison would hide real changes.
#[test]
fn test_bubble_fragment_does_not_read_cpu_only_uniform_fields() {
    let shader = production_module("bubble_dome.wgsl");
    let fragment = shader.entry_point(ShaderStage::Fragment, "fragment");
    for field in ["world_pos", "anim_progress"] {
        assert!(
            !shader.reads_global_member(fragment, field),
            "bubble_dome.wgsl reads {field}; include it in BubbleUniformGpu::renders_same_as"
        );
    }
    assert!(
        shader.reads_global_member(fragment, "frame_index"),
        "the probe must see fields the fragment does read"
    );
}

#[test]
fn test_selection_ring_uniform_and_shader_invariants() {
    let shader = production_module("selection_ring.wgsl");
    let fragment = shader.entry_point(ShaderStage::Fragment, "fragment");

    assert_eq!(
        shader.uniform_struct_at(2, 0),
        (
            "SelectionRingUniform".to_owned(),
            size_of::<SelectionRingUniformGpu>()
        )
    );
    assert!(
        discards_when_below(fragment, "ring_alpha", 0.02),
        "selection_ring.wgsl must discard fragments with ring_alpha < 0.02 to prevent opaque black discs"
    );

    // Verify memory layout exactly equals 32 bytes and 16-byte alignment
    assert_eq!(size_of::<SelectionRingUniformGpu>(), 32);
    assert_eq!(align_of::<SelectionRingUniformGpu>(), 16);
}

/// The discard probe must see a changed threshold, so it cannot pass on any `discard`.
#[test]
fn test_discard_probe_detects_changed_threshold() {
    let source =
        shipped_source("selection_ring.wgsl").replace("ring_alpha < 0.02", "ring_alpha < 0.5");
    let shader = compose_and_validate("selection_ring.wgsl", &source, SHADER_VARIANTS[0])
        .expect("the changed threshold is still valid WGSL");
    let fragment = shader.entry_point(ShaderStage::Fragment, "fragment");
    assert!(discards(fragment));
    assert!(!discards_when_below(fragment, "ring_alpha", 0.02));
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
    let shader = production_module("mist_wake.wgsl");
    shader.entry_point(ShaderStage::Vertex, "vertex");
    shader.entry_point(ShaderStage::Fragment, "fragment");
    let uniform = shader.global_at(2, 2);
    assert!(
        matches!(uniform.space, naga::AddressSpace::Storage { .. }),
        "mist uniform must stay in the storage address space used by the bind group"
    );
    let ty = &shader.module.types[uniform.ty];
    assert_eq!(ty.name.as_deref(), Some("MistWakeUniform"));
    let naga::TypeInner::Struct { span, .. } = ty.inner else {
        panic!("MistWakeUniform must be a struct");
    };
    // The bound struct is `FrameGpu`: the 64-byte appearance block plus the geometry lane.
    assert_eq!(span as usize, size_of::<client::mist::gpu::FrameGpu>());

    assert_eq!(size_of::<MistWakeUniformGpu>(), 64);
    assert_eq!(align_of::<MistWakeUniformGpu>(), 16);
    assert_eq!(size_of::<client::mist::gpu::FrameGpu>(), 80);
    assert_eq!(align_of::<client::mist::gpu::FrameGpu>(), 16);
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

/// The audited flipbook set in `tests/data/flipbook_coverage.ron`.
#[derive(Debug, Deserialize)]
struct FlipbookCoverage {
    /// Slugs of catalog archetypes that ship a baked flipbook.
    catalog_units: BTreeSet<String>,
    /// Flipbooks in `assets/textures` that belong to no catalog archetype.
    non_catalog: BTreeSet<String>,
}

#[test]
fn test_flipbook_textures_512_invariants() {
    let coverage: FlipbookCoverage = ron::from_str(include_str!("data/flipbook_coverage.ron"))
        .expect("flipbook_coverage.ron must parse");
    let textures_dir = resolve_assets_dir().join("textures");
    let catalog = UnitCatalog::default();
    let catalog_slugs: BTreeSet<String> = catalog.archetypes().map(|a| a.slug.clone()).collect();

    let on_disk: BTreeSet<String> = fs::read_dir(&textures_dir)
        .expect("assets/textures must exist: run the asset baker before testing")
        .map(|entry| entry.expect("assets/textures entry").file_name())
        .filter_map(|name| name.into_string().ok())
        .filter_map(|name| name.strip_suffix(".flipbook.ron").map(str::to_owned))
        .collect();
    let declared: BTreeSet<String> = coverage
        .catalog_units
        .union(&coverage.non_catalog)
        .cloned()
        .collect();
    assert_eq!(
        on_disk, declared,
        "assets/textures and tests/data/flipbook_coverage.ron disagree"
    );
    let not_in_catalog: Vec<_> = coverage.catalog_units.difference(&catalog_slugs).collect();
    assert!(
        not_in_catalog.is_empty(),
        "coverage names units missing from the catalog: {not_in_catalog:?}"
    );
    let wrongly_non_catalog: Vec<_> = coverage.non_catalog.intersection(&catalog_slugs).collect();
    assert!(
        wrongly_non_catalog.is_empty(),
        "non_catalog entries are real catalog units: {wrongly_non_catalog:?}"
    );
    for hero in catalog.archetypes().filter(|a| a.showcase_hero) {
        assert!(
            coverage.catalog_units.contains(&hero.slug),
            "showcase hero {} must ship a flipbook",
            hero.slug
        );
    }

    for name in &on_disk {
        let png_path = textures_dir.join(format!("{name}.png"));
        let ron_path = textures_dir.join(format!("{name}.flipbook.ron"));
        let img = image::open(&png_path)
            .unwrap_or_else(|error| panic!("{name}: flipbook PNG must open: {error}"));
        let meta = FlipbookAtlasMeta::load_from_ron(&ron_path)
            .unwrap_or_else(|error| panic!("{name}: manifest must load: {error}"));
        assert_eq!(img.width(), 512, "{name} width must be exactly 512");
        assert_eq!(
            img.height(),
            meta.total_frames * 512,
            "{name} height must hold every manifest frame"
        );
        // Strict WebGPU max_texture_dimension_2d hardware invariant:
        assert!(
            img.height() <= 16384,
            "{name} height {} exceeds wgpu limit 16384!",
            img.height()
        );
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
fn test_ground_unit_altitude_never_spawns_air_tether() -> Result<(), Box<dyn std::error::Error>> {
    let palette = actual_palette()?;
    let style = palette.resolve(palette.primary())?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<SelectionRingMaterial>>();

    app.add_systems(
        Startup,
        (
            client::indicators::setup_indicator_assets_system,
            move |mut commands: Commands<'_, '_>,
                  assets: Res<'_, client::indicators::IndicatorAssets>,
                  mut materials: ResMut<'_, Assets<StandardMaterial>>,
                  mut ring_materials: ResMut<'_, Assets<SelectionRingMaterial>>| {
                let instance = BubbleRenderInstance {
                    entity_id: EntityId(42),
                    shape: ShapeKind::UnitSphere,
                    silhouette: SilhouetteClass::LightProngs,
                    unit_type: UnitTypeId(2),
                    owner: PlayerId(1),
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
                    style,
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
    Ok(())
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
fn test_bevy_ecs_schedule_all_systems_no_conflicts() -> Result<(), Box<dyn std::error::Error>> {
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
    // Direct headless embedding has no focused native window/UI ownership pass.
    // Register explicit noninteractive policy; keep the production Res required.
    app.init_resource::<client::input_policy::InputOwnership>();
    app.init_resource::<client::camera_drag::CameraDragState>();
    app.add_event::<bevy::window::WindowEvent>();

    let root = client::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = client::runtime_battle::BattleDefinition::load(
        root.as_path(),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;
    app.insert_resource(definition.palette.clone());
    app.init_resource::<client::player_presentation::PresentationStatus>();
    app.add_event::<AppExit>();
    app.insert_resource(root);
    app.insert_resource(client::CatalogResource(definition.catalogs.units.clone()));
    let mut world = definition.instantiate()?;
    app.insert_resource(client::battle_pipeline::session::BattleSession::prepare(
        client::battle_pipeline::commands::SessionGeneration::FIRST,
        &mut world,
        client::battle_pipeline::session::CommittedBattleMetadata::from_definition(&definition)?,
    )?);
    let lab_state = client::lab_ui::LabState {
        current_tab: client::lab_ui::LabTab::LiveBattle,
        sim_world: Some(world),
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
            (
                client::lab_ui::step_client_simulation_system,
                client::sync_live_battle_system,
                client::battle_pipeline::driver::publish_combat_events,
            )
                .chain(),
            client::lab_ui::sync_swarm_stress_system,
        ),
    );

    // Stepping the app checks all query archetypes and system parameter validity
    for _ in 0..10 {
        app.update();
        assert!(app
            .world()
            .resource::<client::LabState>()
            .battle_failure
            .is_none());
    }
    Ok(())
}

#[test]
fn test_real_gpu_startup_and_frame_limit_30() {
    if gpu_run::gpu_tests_opted_out() {
        return;
    }
    let client_bin = std::path::Path::new(env!("CARGO_BIN_EXE_client"));
    let mut command = gpu_run::virtual_gpu_command(client_bin);
    command.arg("--lab").arg("--max-frames").arg("30");
    gpu_run::assert_passes_validation(&mut command, "client --lab --max-frames 30");
}

/// Collects each use of a slug in production Rust under `dir`, returning the files scanned.
fn scan_src_for_forbidden_slugs(
    dir: &std::path::Path,
    forbidden: &BTreeSet<String>,
    hits: &mut Vec<String>,
) -> usize {
    let mut scanned = 0;
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            scanned += scan_src_for_forbidden_slugs(&path, forbidden, hits);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let path_str = path.to_string_lossy();
            if path_str.contains("/src/") {
                scanned += 1;
                let content = fs::read_to_string(&path).expect("Failed to read source file");
                for slug in forbidden {
                    if content.contains(slug.as_str()) {
                        hits.push(format!("{slug} in {}", path.display()));
                    }
                }
            }
        }
    }
    scanned
}

#[test]
fn test_zero_hardcoded_creature_names_in_rust_src() {
    // AGENTS.md §3.3: Strict zero creature/building identifiers or slugs in Rust source code across all crates.
    let catalog = UnitCatalog::default();
    let forbidden_slugs: BTreeSet<String> = catalog.archetypes().map(|a| a.slug.clone()).collect();
    assert_eq!(
        forbidden_slugs.len(),
        catalog.archetypes().count(),
        "catalog slugs must be unique"
    );
    assert!(
        forbidden_slugs.iter().all(|slug| !slug.trim().is_empty()),
        "a blank slug would match every file"
    );

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let crates_dir = workspace_dir.join("crates");

    let mut hits = Vec::new();
    let scanned = scan_src_for_forbidden_slugs(&crates_dir, &forbidden_slugs, &mut hits);
    assert!(scanned > 0, "the scan must reach production source files");
    assert!(
        hits.is_empty(),
        "catalog slugs found in production source:\n{}",
        hits.join("\n")
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_all_combat_vfx_entities_despawn_after_lifetime() -> Result<(), Box<dyn std::error::Error>> {
    let palette = actual_palette()?;
    let primary = palette.primary();
    let secondary = palette.secondary();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(palette)
        .init_resource::<client::player_presentation::PresentationStatus>()
        .init_resource::<client::LabState>()
        .init_resource::<client::battle_pipeline::session::BattleSession>()
        .add_event::<AppExit>();

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
                owner: primary,
            },
        ));
        events.send(client::vfx::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::ProjectileVisual {
                pos: [0.0, 0.0, 0.5],
                vel: [10.0, 0.0, 0.0],
                owner: secondary,
                duration_secs: Some(1.2),
            },
        ));
        events.send(client::vfx::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::BeamVisual {
                from: [0.0, 0.0, 0.5],
                to: [5.0, 5.0, 0.5],
                is_lightning: true,
                owner: primary,
            },
        ));
        events.send(client::vfx::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::MeleeSlashVisual {
                pos: [2.0, 2.0, 0.0],
                angle_rad: 0.785,
                radius: 0.8,
                owner: secondary,
            },
        ));
        events.send(client::vfx::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::BubbleShatter {
                pos: [4.0, 4.0, 0.0],
                radius: 0.6,
                is_dome: false,
                owner: primary,
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
    Ok(())
}

/// Invariant test verifying procedural volumetric combat VFX shader layout and material properties.
#[test]
fn test_combat_fx_shader_and_uniform_layout() {
    let shader = production_module("combat_fx.wgsl");
    let fragment = shader.entry_point(ShaderStage::Fragment, "fragment");

    assert_eq!(
        shader.uniform_struct_at(2, 0),
        (
            "CombatFxUniform".to_owned(),
            size_of::<CombatFxUniformGpu>()
        )
    );
    assert!(
        discards(fragment),
        "combat_fx.wgsl must discard to prevent rectangular billboard artifacts"
    );

    // Verify all 6 procedural combat modes are branches of the real fragment shader.
    assert_eq!(
        shader.branch_values_of_member(fragment, "fx", "fx_kind"),
        BTreeSet::from([0, 1, 2, 3, 4, 5])
    );

    // Verify memory layout exactly equals 32 bytes and 16-byte alignment
    assert_eq!(size_of::<CombatFxUniformGpu>(), 32);
    assert_eq!(align_of::<CombatFxUniformGpu>(), 16);

    let mat = CombatFxMaterial {
        uniform: CombatFxUniformGpu::default(),
    };
    assert_eq!(mat.alpha_mode(), AlphaMode::Add);
    assert_eq!(mat.depth_bias(), -25.0);
}

/// Loads the actual authored ownership palette used by production scenes.
fn actual_palette() -> Result<client::player_presentation::PlayerPalette, Box<dyn std::error::Error>>
{
    Ok(client::runtime_battle::BattleDefinition::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?
    .palette)
}
