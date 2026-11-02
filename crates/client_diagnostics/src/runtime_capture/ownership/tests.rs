//! Validates ownership evidence against real event spawning and lifetime systems.
//! These CPU tests establish identity/lifecycle rules; native images remain required.

use super::*;
use bevy::render::mesh::MeshAabb;
use bevy::time::TimeUpdateStrategy;
use client::battle_pipeline::{commands::SessionGeneration, session::CommittedBattleMetadata};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::vfx::{self, CombatVisualEvent};
use sim_types::{CombatEventVisual, PlayerId};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn app() -> Result<(App, OwnershipState), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let definition = BattleDefinition::load(&root, DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut simulation = definition.instantiate()?;
    let session = BattleSession::prepare(
        SessionGeneration::FIRST,
        &mut simulation,
        CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(definition.palette)
        .insert_resource(session)
        .insert_resource(client::LabState {
            current_tab: client::LabTab::LiveBattle,
            sim_world: Some(simulation),
            ..default()
        })
        .init_resource::<client::player_presentation::PresentationStatus>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<CombatFxMaterial>>()
        .add_event::<CombatVisualEvent>()
        .add_event::<AppExit>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .add_systems(Startup, vfx::setup_vfx_assets_system)
        .add_systems(
            Update,
            (
                vfx::process_combat_events_system,
                vfx::update_beams_system,
                vfx::update_projectiles_system,
                vfx::update_melee_slashes_system,
                vfx::update_shatter_system,
            )
                .chain(),
        );
    app.update();
    app.world_mut()
        .resource_mut::<Events<CombatVisualEvent>>()
        .send(CombatVisualEvent(CombatEventVisual::HitscanTracer {
            from: [1.0, 2.0, 0.5],
            to: [4.0, 5.0, 0.5],
            owner: PlayerId(0),
        }));
    app.update();
    let mut evidence = OwnershipState::default();
    evidence.observe(app.world_mut(), View::Battle)?;
    evidence.remember_combat();
    Ok((app, evidence))
}

#[test]
fn retained_effect_keeps_source_generation_and_owner_through_tabs() -> TestResult {
    let (mut app, mut evidence) = app()?;
    let original = evidence
        .effects
        .values()
        .next()
        .ok_or("actual spawned effect")?
        .clone();
    assert_eq!(original.source_generation, Some(1));
    app.world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = client::LabTab::VisualLab;
    app.update();
    evidence.observe(app.world_mut(), View::Lab)?;
    app.world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = client::LabTab::LiveBattle;
    app.update();
    evidence.observe(app.world_mut(), View::Return)?;
    let retained = evidence
        .effects
        .get(&original.entity)
        .ok_or("retained effect")?;
    assert_eq!(retained.source_generation, original.source_generation);
    assert_eq!(retained.material, original.material);
    assert_eq!(retained.owner, original.owner);
    assert_eq!(retained.ended, None);
    for mut owner in app
        .world_mut()
        .query::<&mut CombatEffectOwner>()
        .iter_mut(app.world_mut())
    {
        owner.0 = PlayerId(1);
    }
    assert!(evidence.observe(app.world_mut(), View::Return).is_err());
    Ok(())
}

#[test]
fn missing_owner_and_unexplained_disappearance_fail_closed() -> TestResult {
    let (mut app, mut evidence) = app()?;
    let id = app
        .world_mut()
        .query::<(Entity, &CombatEffectOwner)>()
        .iter(app.world())
        .next()
        .ok_or("actual effect")?
        .0;
    app.world_mut().entity_mut(id).remove::<CombatEffectOwner>();
    assert!(evidence.observe(app.world_mut(), View::Battle).is_err());
    app.world_mut()
        .entity_mut(id)
        .insert(CombatEffectOwner(PlayerId(0)));
    app.world_mut().despawn(id);
    assert!(evidence.observe(app.world_mut(), View::Lab).is_err());
    Ok(())
}

#[test]
fn production_lifetime_expiry_is_recorded_without_relabeling_origin() -> TestResult {
    let (mut app, mut evidence) = app()?;
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        200,
    )));
    app.update();
    evidence.observe(app.world_mut(), View::Battle)?;
    let proof = evidence.effects.values().next().ok_or("effect history")?;
    assert_eq!(proof.ended, Some("production lifetime expired"));
    assert_eq!(proof.source_generation, Some(1));
    Ok(())
}

#[derive(Resource)]
struct Preparation {
    evidence: OwnershipState,
    kind: EffectKind,
    pending_with_projectile: usize,
    quiescent_frames: usize,
    spawn_delta: Option<Duration>,
    focus: Option<Vec2>,
    failure: Option<String>,
}

fn prepare_before_production_vfx(world: &mut World) {
    world.resource_scope(|world, mut preparation: Mut<'_, Preparation>| {
        *world.resource_mut::<TimeUpdateStrategy>() =
            TimeUpdateStrategy::ManualDuration(Duration::ZERO);
        let old_projectile = !preparation.evidence.effect_emitted
            && world
                .query::<&ProjectileVisual>()
                .iter(world)
                .next()
                .is_some();
        let was_emitted = preparation.evidence.effect_emitted;
        let kind = preparation.kind;
        match preparation.evidence.settle(world, View::Effect(kind)) {
            Ok(focus) => preparation.focus = focus,
            Err(error) => preparation.failure = Some(error.to_string()),
        }
        if preparation.focus.is_none() {
            if old_projectile {
                preparation.pending_with_projectile += 1;
            } else if !world.resource::<Time>().delta().is_zero() {
                preparation.quiescent_frames += 1;
            }
        }
        if !was_emitted && preparation.evidence.effect_emitted {
            preparation.spawn_delta = Some(world.resource::<Time>().delta());
        }
    });
}

pub(in crate::runtime_capture) fn preparation_app(
    kind: EffectKind,
) -> Result<App, Box<dyn std::error::Error>> {
    let (mut app, mut evidence) = app()?;
    let mut actor = app
        .world_mut()
        .resource_mut::<client::LabState>()
        .sim_world
        .as_mut()
        .ok_or("real simulation")?
        .build_render_snapshot()
        .bubbles
        .into_iter()
        .next()
        .ok_or("actual actor")?;
    actor.selected = true;
    let mesh = Mesh::from(Sphere::new(actor.radius));
    let bounds = mesh.compute_aabb().ok_or("real sphere bounds")?;
    let global = GlobalTransform::from_translation(Vec3::new(
        actor.world_pos[0],
        actor.radius,
        actor.world_pos[1],
    ));
    app.world_mut().spawn((
        client::LabPreviewBubble,
        client::BubbleInstance(actor),
        bounds,
        global,
    ));
    app.world_mut()
        .resource_mut::<Events<CombatVisualEvent>>()
        .send(CombatVisualEvent(CombatEventVisual::ProjectileVisual {
            pos: [1.0, 2.0, 0.5],
            vel: [2.0, 0.0, 0.0],
            owner: PlayerId(0),
            duration_secs: Some(1.2),
        }));
    app.update();
    evidence.apply(
        app.world_mut(),
        View::Effect(kind),
        Duration::from_millis(40),
    )?;
    app.insert_resource(Preparation {
        evidence,
        kind,
        pending_with_projectile: 0,
        quiescent_frames: 0,
        spawn_delta: None,
        focus: None,
        failure: None,
    });
    app.add_systems(
        Update,
        prepare_before_production_vfx.before(vfx::process_combat_events_system),
    );
    Ok(app)
}

struct ActualAge {
    elapsed: Duration,
    duration: Duration,
    shader_age: f32,
}

fn actual_timers(world: &mut World) -> Result<Vec<ActualAge>, Box<dyn std::error::Error>> {
    let mut query = world.query::<(
        &MeshMaterial3d<CombatFxMaterial>,
        Option<&BeamVisualComponent>,
        Option<&ProjectileVisual>,
        Option<&MeleeSlashComponent>,
        Option<&ShatterShard>,
    )>();
    query
        .iter(world)
        .map(|(handle, beam, projectile, melee, shard)| {
            let timer = beam
                .map(|value| &value.lifetime)
                .or_else(|| projectile.map(|value| &value.lifetime))
                .or_else(|| melee.map(|value| &value.lifetime))
                .or_else(|| shard.map(|value| &value.lifetime))
                .ok_or("actual lifetime")?;
            let material = world
                .resource::<Assets<CombatFxMaterial>>()
                .get(&handle.0)
                .ok_or("actual material")?;
            Ok(ActualAge {
                elapsed: timer.elapsed(),
                duration: timer.duration(),
                shader_age: material.uniform.age,
            })
        })
        .collect()
}

#[test]
fn all_subtypes_spawn_after_quiescence_and_age_exactly_one_controlled_frame() -> TestResult {
    let age = Duration::from_millis(40);
    for kind in [
        EffectKind::Hitscan,
        EffectKind::Projectile,
        EffectKind::Laser,
        EffectKind::Lightning,
        EffectKind::Melee,
        EffectKind::Shatter,
    ] {
        let mut app = preparation_app(kind)?;
        let mut observed_zero = false;
        let mut reached_age = false;
        for _ in 0..32 {
            app.update();
            if !app
                .world()
                .resource::<Preparation>()
                .evidence
                .effect_emitted
            {
                continue;
            }
            let timers = actual_timers(app.world_mut())?;
            assert_eq!(
                timers.len(),
                if kind == EffectKind::Shatter { 17 } else { 1 }
            );
            assert!(
                timers.iter().all(|timer| timer.elapsed <= age),
                "{kind:?} received duplicate/residual aging"
            );
            observed_zero |= timers.iter().all(|timer| timer.elapsed.is_zero());
            if timers.iter().all(|timer| timer.elapsed == age) {
                reached_age = true;
                break;
            }
        }
        assert!(
            observed_zero && reached_age,
            "{kind:?} must spawn at zero then reach exactly40ms"
        );
        let preparation = app.world().resource::<Preparation>();
        assert_eq!(preparation.failure, None);
        assert!(preparation.pending_with_projectile > 4);
        assert!(preparation.quiescent_frames > 0);
        assert_eq!(preparation.spawn_delta, Some(Duration::ZERO));
        assert!(preparation.focus.is_some());
        for _ in 0..4 {
            app.update();
            let timers = actual_timers(app.world_mut())?;
            assert_eq!(
                timers.len(),
                if kind == EffectKind::Shatter { 17 } else { 1 }
            );
            for timer in timers {
                assert_eq!(timer.elapsed, age);
                assert!(
                    (timer.shader_age - age.as_secs_f32() / timer.duration.as_secs_f32()).abs()
                        < 0.000_001
                );
            }
        }
    }
    Ok(())
}

/// Reads the test's actual preparation state after the production VFX schedule.
pub(in crate::runtime_capture) fn sample_readiness(
    world: &mut World,
) -> Result<(Option<Vec2>, bool), CaptureError> {
    world.resource_scope(|world, preparation: Mut<'_, Preparation>| {
        if let Some(error) = &preparation.failure {
            return Err(failure(error));
        }
        Ok((
            preparation.focus,
            preparation.evidence.sample_ready(world, preparation.kind)?,
        ))
    })
}

#[test]
fn lightning_samples_preserve_identity_stage_and_exact_wrapped_time_freeze() -> TestResult {
    let mut app = preparation_app(EffectKind::Lightning)?;
    let mut identity = None;
    let mut stage = None;
    for milliseconds in [40, 80, 120] {
        let elapsed = Duration::from_millis(milliseconds);
        if milliseconds != 40 {
            app.world_mut()
                .resource_scope(|world, mut preparation: Mut<'_, Preparation>| {
                    preparation
                        .evidence
                        .apply(world, View::Effect(EffectKind::Lightning), elapsed)
                })?;
        }
        let mut ready = false;
        for _ in 0..40 {
            app.update();
            if sample_readiness(app.world_mut())?.1 {
                ready = true;
                break;
            }
        }
        assert!(ready);
        let actual = Fingerprint::capture(app.world_mut())?;
        let effect = actual
            .effects
            .first()
            .ok_or("actual lightning fingerprint")?;
        assert_eq!(actual.effects.len(), 1);
        assert_eq!(u128::from(effect.elapsed_ns), elapsed.as_nanos());
        let current = (
            effect.entity,
            effect.material.clone(),
            effect.owner,
            effect.seed_bits,
            effect.duration_ns,
        );
        if let Some(original) = &identity {
            assert_eq!(&current, original);
        } else {
            identity = Some(current);
        }
        let current_stage = app
            .world()
            .resource::<Preparation>()
            .evidence
            .stage
            .ok_or("actual stage")?;
        let current_stage = (
            current_stage.origin.map(f32::to_bits),
            current_stage.focus.to_array().map(f32::to_bits),
        );
        if let Some(original) = &stage {
            assert_eq!(&current_stage, original);
        } else {
            stage = Some(current_stage);
        }
        for _ in 0..4 {
            app.update();
            actual.verify(&Fingerprint::capture(app.world_mut())?)?;
        }
    }
    Ok(())
}

#[test]
fn extra_actual_time_after_acceptance_rejects_visual_readback_without_sim_tick_change() -> TestResult
{
    let mut app = preparation_app(EffectKind::Lightning)?;
    for _ in 0..40 {
        app.update();
        if sample_readiness(app.world_mut())?.1 {
            break;
        }
    }
    let accepted = Fingerprint::capture(app.world_mut())?;
    assert_eq!(
        accepted.effects.first().ok_or("actual sample")?.elapsed_ns,
        40_000_000
    );
    let before = app
        .world()
        .resource::<client::LabState>()
        .sim_world
        .as_ref()
        .ok_or("world")?
        .compute_checksum();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        40,
    )));
    app.update();
    let after = app
        .world()
        .resource::<client::LabState>()
        .sim_world
        .as_ref()
        .ok_or("world")?
        .compute_checksum();
    assert_eq!(before, after);
    assert!(accepted
        .verify(&Fingerprint::capture(app.world_mut())?)
        .is_err());
    let id = Entity::from_bits(accepted.effects[0].entity);
    app.world_mut().entity_mut(id).remove::<CombatEffectOwner>();
    assert!(Fingerprint::capture(app.world_mut()).is_err());
    Ok(())
}

#[test]
fn same_handle_shader_age_mutation_cannot_pass_frozen_timer_evidence() -> TestResult {
    let mut app = preparation_app(EffectKind::Lightning)?;
    for _ in 0..40 {
        app.update();
        if sample_readiness(app.world_mut())?.1 {
            break;
        }
    }
    let accepted = Fingerprint::capture(app.world_mut())?;
    let entity = Entity::from_bits(accepted.effects.first().ok_or("sample")?.entity);
    let handle = app
        .world()
        .get::<MeshMaterial3d<CombatFxMaterial>>(entity)
        .ok_or("actual material")?
        .0
        .clone();
    app.world_mut()
        .resource_mut::<Assets<CombatFxMaterial>>()
        .get_mut(&handle)
        .ok_or("loaded material")?
        .uniform
        .age = 0.0;
    assert!(Fingerprint::capture(app.world_mut()).is_err());
    assert!(sample_readiness(app.world_mut()).is_err());
    Ok(())
}

#[test]
fn actual_fingerprint_ron_roundtrip_preserves_nanoseconds_above_u32() -> TestResult {
    let (mut app, _) = app()?;
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        250,
    )));
    for _ in 0..20 {
        app.update();
    }
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.world_mut()
        .resource_mut::<Events<CombatVisualEvent>>()
        .send(CombatVisualEvent(CombatEventVisual::HitscanTracer {
            from: [1.0, 2.0, 0.5],
            to: [4.0, 5.0, 0.5],
            owner: PlayerId(0),
        }));
    app.update();
    let actual = Fingerprint::capture(app.world_mut())?;
    assert!(actual.presentation_elapsed_ns > u64::from(u32::MAX));
    assert_eq!(actual.effects.len(), 1);
    let encoded = ron::ser::to_string_pretty(&actual, ron::ser::PrettyConfig::default())?;
    let decoded: Fingerprint = ron::from_str(&encoded)?;
    assert_eq!(decoded, actual);
    actual.verify(&decoded)?;
    Ok(())
}
