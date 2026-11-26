//! The air shadow and tether follow the ground and the altitude every frame (plan
//! `docs/plans/2026-11-30-terrain-and-air-indicators.md` §5.3, B-238 and B-244).
//!
//! # Responsibility
//! An air unit's shadow lies on the ground below it, [`SHADOW_LIFT`] above the record's
//! `ground_z`, and its tether runs from the bubble's centre down to the shadow. Both are shown
//! exactly while the unit is airborne. They follow the record every frame, written by the
//! production `update_indicators_system`, and a frame that changes nothing writes nothing.
//!
//! # Tests
//! - U-2 [`the_shadow_follows_a_change_of_altitude`]: a minimal app with the production indicator
//!   systems; the test writes the record and the bubble's transform as the live synchroniser does
//!   through a cliff crossing, a climb, a landing below the airborne height and a take-off, then a
//!   steady frame that must flag no change on the shadow or the tether.
//! - I-1 [`an_air_unit_crossing_a_cliff_keeps_its_shadow_on_the_ground`]: the production client
//!   headless on the shipped skirmish (`support/battle_harness.rs`); an air unit, chosen by
//!   catalog property, is ordered by right click across a cliff, and every frame its shadow and
//!   tether are checked against its record. This is the whole path: snapshot, synchroniser,
//!   indicators.
//!
//! The WP2 review captures of the same crossing are `air_indicator_review_gpu_tests.rs`.
//!
//! # Invariants verified
//! World heights agree within [`HEIGHT_TOLERANCE`]: a few `f32` roundings of the transform
//! product at altitudes below 16 world units.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/air_crossing.rs"]
mod air_crossing;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use air_crossing::{smallest_air_unit, Crossing, PAST_CELLS, START_CELLS};
use battle_harness::{Harness, LOCAL_PLAYER};
use bevy::prelude::*;
use client::battle_selection::BattleSelection;
use client::bubble_shader::BubbleInstance;
use client::indicators::{
    attach_bubble_indicators, setup_indicator_assets_system, update_indicators_system,
    AirShadowMarker, AirTetherMarker, IndicatorAssets, SelectionRingMaterial, SHADOW_LIFT,
};
use client::player_presentation::{PlayerStyle, PresentationStatus};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::scene_controller::{LiveBattleBubble, RenderEntityMap};
use sim_types::snapshot::BubbleRenderInstance;
use sim_types::{EntityId, ShapeKind};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Agreement of composed world heights, world units.
const HEIGHT_TOLERANCE: f32 = 1e-4;

/// Height of the tether line mesh (`Cylinder::new(0.02, 1.0)`), in its local units.
const TETHER_MESH_HEIGHT: f32 = 1.0;

/// The drawn state of one bubble's air indicators.
#[derive(Debug, Clone, Copy)]
struct AirDrawn {
    /// World centre of the bubble.
    centre: Vec3,
    /// World centre of the shadow disc, and its world scale.
    shadow: Vec3,
    shadow_scale: Vec3,
    /// World altitudes of the tether's two ends, top then bottom.
    tether: (f32, f32),
    /// Whether the shadow and the tether are shown.
    shadow_shown: bool,
    tether_shown: bool,
}

/// Reads the air indicators of the bubble `parent`.
fn drawn(world: &mut World, parent: Entity) -> AirDrawn {
    let centre = world
        .get::<GlobalTransform>(parent)
        .expect("the bubble has a world transform")
        .translation();
    let children: Vec<Entity> = world
        .get::<Children>(parent)
        .expect("the bubble has indicator children")
        .to_vec();
    let find = |world: &World, shadow: bool| {
        children
            .iter()
            .copied()
            .find(|child| {
                if shadow {
                    world.get::<AirShadowMarker>(*child).is_some()
                } else {
                    world.get::<AirTetherMarker>(*child).is_some()
                }
            })
            .expect("every bubble has an air shadow and a tether")
    };
    let (shadow, tether) = (find(world, true), find(world, false));
    let shadow_world = world
        .get::<GlobalTransform>(shadow)
        .expect("a shadow transform")
        .compute_transform();
    let tether_world = world
        .get::<GlobalTransform>(tether)
        .expect("a tether transform")
        .compute_transform();
    let half = 0.5 * TETHER_MESH_HEIGHT * tether_world.scale.y;
    let shown = |entity: Entity| {
        world
            .get::<Visibility>(entity)
            .is_some_and(|visibility| *visibility != Visibility::Hidden)
    };
    AirDrawn {
        centre,
        shadow: shadow_world.translation,
        shadow_scale: shadow_world.scale,
        tether: (
            tether_world.translation.y + half,
            tether_world.translation.y - half,
        ),
        shadow_shown: shown(shadow),
        tether_shown: shown(tether),
    }
}

/// Checks `drawn` against plan D-5 for a record on `ground_z`: the shadow `SHADOW_LIFT` above
/// the ground under the bubble, the tether from the centre to the shadow, both shown iff
/// `airborne`. Returns a description of every difference.
fn mismatches(drawn: &AirDrawn, ground_z: f32, airborne: bool) -> Vec<String> {
    let mut found = Vec::new();
    let ground = ground_z + SHADOW_LIFT;
    if (drawn.shadow.y - ground).abs() > HEIGHT_TOLERANCE {
        found.push(format!(
            "shadow at {:.4}, ground + lift {ground:.4} ({:+.4})",
            drawn.shadow.y,
            drawn.shadow.y - ground
        ));
    }
    if (drawn.shadow.x - drawn.centre.x).abs() > HEIGHT_TOLERANCE
        || (drawn.shadow.z - drawn.centre.z).abs() > HEIGHT_TOLERANCE
    {
        found.push(format!(
            "shadow at {} is not below the centre {}",
            drawn.shadow, drawn.centre
        ));
    }
    let (top, bottom) = drawn.tether;
    if (top - drawn.centre.y).abs() > HEIGHT_TOLERANCE
        || (bottom - drawn.shadow.y).abs() > HEIGHT_TOLERANCE
    {
        found.push(format!(
            "tether {bottom:.4}..{top:.4}, centre {:.4}, shadow {:.4}",
            drawn.centre.y, drawn.shadow.y
        ));
    }
    if drawn.shadow_shown != airborne || drawn.tether_shown != airborne {
        found.push(format!(
            "shadow shown {}, tether shown {}, airborne {airborne}",
            drawn.shadow_shown, drawn.tether_shown
        ));
    }
    found
}

// --- U-2 -----------------------------------------------------------------------------------------

/// The record and style a U-2 app spawns its bubble from.
#[derive(Resource)]
struct Fixture(BubbleRenderInstance, PlayerStyle);

/// The bubble's transform as the live synchroniser writes it (`LiveBubbleFrame::new`): a
/// sphere's centre one radius above the record's altitude, scaled by its radius.
fn synchronised_transform(instance: &BubbleRenderInstance) -> Transform {
    Transform::from_xyz(
        instance.world_pos[0],
        instance.world_pos[2] + instance.radius,
        instance.world_pos[1],
    )
    .with_scale(Vec3::splat(instance.radius))
}

fn spawn_fixture(
    mut commands: Commands<'_, '_>,
    fixture: Res<'_, Fixture>,
    assets: Res<'_, IndicatorAssets>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    mut rings: ResMut<'_, Assets<SelectionRingMaterial>>,
) {
    let Fixture(instance, style) = fixture.into_inner();
    let parent = commands
        .spawn((
            BubbleInstance(instance.clone()),
            synchronised_transform(instance),
        ))
        .id();
    attach_bubble_indicators(
        &mut commands,
        parent,
        instance,
        assets.into_inner(),
        &mut materials,
        &mut rings,
        *style,
    );
}

/// An air record of the shipped skirmish's first unit sphere, its owner the local player.
fn air_record(definition: &BattleDefinition) -> TestResult<BubbleRenderInstance> {
    let mut instance = definition
        .instantiate()?
        .build_render_snapshot()
        .bubbles
        .into_iter()
        .find(|bubble| matches!(bubble.shape, ShapeKind::UnitSphere))
        .ok_or("the shipped skirmish has a unit sphere")?;
    instance.owner = LOCAL_PLAYER;
    instance.radius = 0.625;
    instance.ground_z = 3.0;
    instance.world_pos[2] = instance.ground_z + 2.5;
    Ok(instance)
}

/// Writes `instance` into the bubble and its transform as the synchroniser does, and runs a
/// frame.
fn advance(app: &mut App, parent: Entity, instance: &BubbleRenderInstance) {
    let world = app.world_mut();
    let target = synchronised_transform(instance);
    let mut transform = world
        .get_mut::<Transform>(parent)
        .expect("the bubble transform");
    transform.set_if_neq(target);
    world
        .get_mut::<BubbleInstance>(parent)
        .expect("the bubble record")
        .0
        .clone_from(instance);
    app.update();
}

/// The change ticks of the shadow's and the tether's transform and visibility.
fn indicator_ticks(world: &World, parent: Entity) -> Vec<(u32, u32)> {
    let children = world.get::<Children>(parent).expect("children");
    children
        .iter()
        .filter(|child| {
            world.get::<AirShadowMarker>(**child).is_some()
                || world.get::<AirTetherMarker>(**child).is_some()
        })
        .map(|child| {
            let entity = world.entity(*child);
            let transform = entity.get_change_ticks::<Transform>().expect("a transform");
            let visibility = entity
                .get_change_ticks::<Visibility>()
                .expect("a visibility");
            (transform.changed.get(), visibility.changed.get())
        })
        .collect()
}

/// U-2: the production indicator system moves the shadow and the tether with the record's
/// ground and altitude every frame, and a steady frame writes nothing.
#[test]
fn the_shadow_follows_a_change_of_altitude() -> TestResult {
    let definition =
        BattleDefinition::load(&battle_harness::assets(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut instance = air_record(&definition)?;
    let style = definition.palette.resolve(instance.owner)?;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, HierarchyPlugin))
        .insert_resource(definition.palette.clone())
        .insert_resource(Fixture(instance.clone(), style))
        .init_resource::<PresentationStatus>()
        .add_event::<AppExit>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<SelectionRingMaterial>>()
        .add_systems(
            Startup,
            (setup_indicator_assets_system, spawn_fixture).chain(),
        )
        .add_systems(Update, update_indicators_system);
    app.update();
    let parent = app
        .world_mut()
        .query_filtered::<Entity, With<BubbleInstance>>()
        .single(app.world());
    let arrival = drawn(app.world_mut(), parent);

    // (step, ground_z, height above it, airborne)
    let steps = [
        ("arrival", 3.0, 2.5, true),
        ("cliff crossing", 0.0, 2.5, true),
        ("climb", 0.0, 4.0, true),
        ("landing below the airborne height", 0.0, 0.3, false),
        ("take-off", 0.0, 2.5, true),
    ];
    let mut failures = Vec::new();
    for (name, ground_z, height, airborne) in steps {
        instance.ground_z = ground_z;
        instance.world_pos[2] = ground_z + height;
        advance(&mut app, parent, &instance);
        let now = drawn(app.world_mut(), parent);
        for mismatch in mismatches(&now, ground_z, airborne) {
            failures.push(format!("{name}: {mismatch}"));
        }
        if (now.shadow_scale - arrival.shadow_scale)
            .abs()
            .max_element()
            > HEIGHT_TOLERANCE
        {
            failures.push(format!(
                "{name}: the shadow's size changed from {} to {}",
                arrival.shadow_scale, now.shadow_scale
            ));
        }
    }

    // A steady frame: the record and the bubble do not change, and neither may the indicators.
    let before = indicator_ticks(app.world(), parent);
    advance(&mut app, parent, &instance);
    let after = indicator_ticks(app.world(), parent);
    if before != after {
        failures.push(format!(
            "a steady frame flagged the shadow or tether changed: ticks {before:?} -> {after:?}"
        ));
    }
    assert!(
        failures.is_empty(),
        "the air indicators do not follow the record:\n{}",
        failures.join("\n")
    );
    Ok(())
}

// --- I-1 -----------------------------------------------------------------------------------------

/// Frames the crossing may take before the test gives up.
const CROSSING_FRAMES: usize = 1500;

/// The bubble entity drawing unit `id`, and its record.
fn bubble_of(harness: &mut Harness, id: EntityId) -> (Entity, BubbleRenderInstance) {
    let world = harness.app.world_mut();
    world
        .query_filtered::<(Entity, &BubbleInstance), With<LiveBattleBubble>>()
        .iter(world)
        .find(|(_, bubble)| bubble.0.entity_id == id)
        .map(|(entity, bubble)| (entity, bubble.0.clone()))
        .expect("the unit has a bubble")
}

/// I-1: through the whole production path, an air unit flying across a cliff keeps its shadow
/// on the ground under it and its tether from its centre to the shadow, every frame.
#[test]
fn an_air_unit_crossing_a_cliff_keeps_its_shadow_on_the_ground() {
    let mut harness = Harness::boot(client::ClientMode::LiveBattle);
    let enemies: Vec<Vec2> = harness
        .enemy_units()
        .iter()
        .map(|id| harness.simulated_position(*id))
        .collect();
    let crossing = Crossing::find(&harness.world().map, &enemies);
    let unit_type = smallest_air_unit(&harness);
    let start = crossing.at(crossing.x - START_CELLS);
    let goal = crossing.at(crossing.x + 1 + PAST_CELLS);
    let id = harness
        .world_mut()
        .spawn_unit(unit_type, LOCAL_PLAYER, battle_harness::sim_position(start))
        .expect("an air unit can be placed over any cell");
    // One tick puts the unit in the snapshot, so its bubble is drawn before the order.
    harness.resume();
    harness.step(1);
    harness.pause();
    assert!(
        harness
            .app
            .world()
            .resource::<RenderEntityMap>()
            .get(id)
            .is_some(),
        "the staged unit is drawn"
    );
    {
        let mut selection = harness.app.world_mut().resource_mut::<BattleSelection>();
        selection.clear();
        selection.extend([id]);
    }
    harness.look_at((start + goal) * 0.5);
    let target = harness.ground_on_screen(goal);
    harness.click(MouseButton::Right, target);
    harness.resume();

    let edge = crossing.x as f32 + 1.0;
    let mut failures = Vec::new();
    let mut grounds = Vec::new();
    let mut frames = 0;
    while harness.simulated_position(id).x < edge + PAST_CELLS as f32 {
        assert!(
            frames < CROSSING_FRAMES,
            "the unit did not cross the cliff in {CROSSING_FRAMES} frames"
        );
        harness.step(1);
        frames += 1;
        let (entity, record) = bubble_of(&mut harness, id);
        if !grounds.contains(&record.ground_z.to_bits()) {
            grounds.push(record.ground_z.to_bits());
        }
        let now = drawn(harness.app.world_mut(), entity);
        for mismatch in mismatches(&now, record.ground_z, true) {
            failures.push(format!(
                "frame {frames} at x {:.3} on ground {:.3}: {mismatch}",
                record.world_pos[0], record.ground_z
            ));
        }
    }
    assert!(
        grounds.len() >= 2,
        "the crossing at {crossing:?} changed the record's ground: {} grounds seen",
        grounds.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {frames} frames draw the air indicators off the ground, e.g.:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
