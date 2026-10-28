//! Pure protocol regressions; these are not substitutes for native phase evidence.

use super::interval::Collector;
use super::model::*;

fn config() -> AuditConfig {
    AuditConfig {
        max_views: 2,
        max_items: 8,
        max_epochs: 8,
        max_bytes: 1_048_576,
        max_source_bytes: 1_048_576,
        fixture: None,
    }
}
pub(super) fn frame(epoch: u64) -> EpochEvidence {
    EpochEvidence {
        epoch,
        source: SourceFrame {
            elapsed_nanos: 5_000_000_000,
            wrapped_time_bits: 5.0_f32.to_bits(),
            delta_bits: 0,
            scene: "fixture".into(),
            environment: vec![],
            items: vec![],
            limitations: vec![],
            assets: vec![],
            instantiated_swarm_shells: 0,
            fixture: None,
        },
        views: vec![ViewEvidence {
            render_entity: 2,
            main_entity: 1,
            world_from_view: [0; 16],
            clip_from_view: [0; 16],
            clip_from_world: None,
            viewport: [0, 0, 1280, 720],
            hdr: true,
            color_grading: "actual fixture description".into(),
            lighting: vec![],
            queued: vec![],
            original_queue: None,
            sorted: vec![],
        }],
        ready: true,
        pending: vec![],
        mist: None,
    }
}

fn entry(entity: bevy::prelude::Entity, logical: u32, depth: f32) -> PhaseEntry {
    PhaseEntry {
        ordinal: 0,
        render_entity: entity.to_bits(),
        association: RenderAssociation {
            visible_pair_confirmed: false,
            temporary_marker: false,
            render_main_entity: None,
            kind: AssociationKind::Unsupported("CPU fixture"),
        },
        main_entity: entity.to_bits(),
        logical: Some(LogicalIdentity {
            domain: "Swarm",
            entity: logical,
            generation: 0,
        }),
        distance_bits: depth.to_bits(),
        pipeline: "phase algorithm fixture".into(),
        pipeline_description: "not a shader test".into(),
        draw_function: "no draw submitted".into(),
        material: MaterialSource::Unsupported {
            reason: "CPU phase contract fixture",
        },
        mesh: None,
        batch_range: [0, 1],
        extra_index: "none".into(),
    }
}

#[test]
fn declared_fixture_uses_real_phase_sort_and_rejects_near_ties(
) -> Result<(), Box<dyn std::error::Error>> {
    use bevy::core_pipeline::core_3d::Transparent3d;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use bevy::render::render_phase::{
        sort_phase_system, DrawFunctions, PhaseItemExtraIndex, RenderCommandState,
        ViewSortedRenderPhases,
    };
    use bevy::render::render_resource::CachedRenderPipelineId;
    for order in [FixtureOrder::Forward, FixtureOrder::Reverse] {
        let mut world = World::new();
        let functions = DrawFunctions::<Transparent3d>::default();
        let command = RenderCommandState::<Transparent3d, ()>::new(&mut world);
        let draw = functions.write().add(command);
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let view = world.spawn_empty().id();
        let mut phases = ViewSortedRenderPhases::<Transparent3d>::default();
        phases.insert_or_clear(view);
        for entity in [b, a] {
            phases
                .get_mut(&view)
                .ok_or("phase absent")?
                .items
                .push(Transparent3d {
                    distance: -10.0,
                    pipeline: CachedRenderPipelineId::INVALID,
                    entity: (entity, entity.into()),
                    draw_function: draw,
                    batch_range: 0..1,
                    extra_index: PhaseItemExtraIndex::NONE,
                });
        }
        world.insert_resource(phases);
        let mut evidence = frame(1);
        evidence.views[0].render_entity = view.to_bits();
        evidence.views[0].queued = vec![entry(b, 9, -10.0), entry(a, 4, -10.0)];
        let spec = FixtureSpec {
            participants: [4, 9],
            positions: [[0.0; 3]; 2],
            order,
        };
        assert!(super::fixture::declare(&mut world, &evidence, &spec)?);
        world.run_system_once(sort_phase_system::<Transparent3d>)?;
        let actual: Vec<_> = world
            .resource::<ViewSortedRenderPhases<Transparent3d>>()
            .get(&view)
            .ok_or("phase disappeared")?
            .items
            .iter()
            .map(|item| item.entity.0)
            .collect();
        assert_eq!(
            actual,
            match order {
                FixtureOrder::Forward => vec![a, b],
                FixtureOrder::Reverse => vec![b, a],
            }
        );
        evidence.views[0].queued[0].distance_bits = (-10.0_f32).to_bits() + 1;
        assert!(super::fixture::declare(&mut world, &evidence, &spec).is_err());
        evidence.views[0].queued[0].distance_bits = (-0.0_f32).to_bits();
        evidence.views[0].queued[1].distance_bits = 0.0_f32.to_bits();
        assert!(super::fixture::declare(&mut world, &evidence, &spec).is_err());
    }
    Ok(())
}

#[test]
fn callback_requires_following_extraction_and_keeps_inflight_candidates() -> Result<(), AuditError>
{
    let mut collector = Collector::new(config())?;
    let epoch = collector.extract_epoch()?;
    collector.complete(frame(epoch))?;
    let nonce = collector.arm()?;
    collector.bind(nonce, 17)?;
    let inflight = collector.extract_epoch()?;
    collector.callback(17)?;
    collector.complete(frame(inflight))?;
    assert!(collector.finish()?.is_none());
    let closing = collector.extract_epoch()?;
    collector.complete(frame(closing))?;
    let report = collector
        .finish()?
        .ok_or(AuditError::Lifecycle("report absent"))?;
    assert_eq!(
        report
            .epochs
            .iter()
            .map(|frame| frame.epoch)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(report.classification.starts_with("stable"));
    Ok(())
}

#[test]
fn variation_and_pending_epochs_are_retained_and_inconclusive() -> Result<(), AuditError> {
    for pending in [false, true] {
        let mut collector = Collector::new(config())?;
        let first = collector.extract_epoch()?;
        collector.complete(frame(first))?;
        let nonce = collector.arm()?;
        collector.bind(nonce, 17)?;
        collector.callback(17)?;
        let second = collector.extract_epoch()?;
        let mut changed = frame(second);
        if pending {
            changed.ready = false;
            changed.pending.push("pipeline".into());
        } else {
            changed.source.wrapped_time_bits = 7.0_f32.to_bits();
        }
        collector.complete(changed)?;
        let report = collector
            .finish()?
            .ok_or(AuditError::Lifecycle("report absent"))?;
        assert_eq!(report.epochs.len(), 2);
        assert!(report.classification.starts_with("inconclusive"));
    }
    Ok(())
}

#[test]
fn foreign_callbacks_missing_epochs_and_budget_are_rejected() -> Result<(), AuditError> {
    let mut collector = Collector::new(config())?;
    assert!(collector.arm().is_err());
    let first = collector.extract_epoch()?;
    collector.complete(frame(first))?;
    let nonce = collector.arm()?;
    collector.bind(nonce, 17)?;
    assert!(collector.bind(nonce, 18).is_err());
    assert!(collector.callback(18).is_err());
    collector.callback(17)?;
    assert!(collector.callback(17).is_err());
    collector.extract_epoch()?;
    let skipped = collector.extract_epoch()?;
    assert!(collector.complete(frame(skipped)).is_err());
    let mut invalid = config();
    invalid.max_views = 5;
    assert!(Collector::new(invalid).is_err());
    Ok(())
}

#[test]
fn whole_interval_capacity_and_partial_source_claims_are_explicit() -> Result<(), AuditError> {
    let mut limits = config();
    limits.max_epochs = 2;
    let mut collector = Collector::new(limits)?;
    let epoch = collector.extract_epoch()?;
    collector.complete(frame(epoch))?;
    let nonce = collector.arm()?;
    collector.bind(nonce, 17)?;
    let epoch = collector.extract_epoch()?;
    collector.complete(frame(epoch))?;
    let epoch = collector.extract_epoch()?;
    assert!(matches!(
        collector.complete(frame(epoch)),
        Err(AuditError::Budget(_))
    ));
    assert_eq!(
        collector
            .incomplete()
            .ok_or(AuditError::Lifecycle("lost incomplete records"))?
            .epochs
            .len(),
        2
    );
    let mut collector = Collector::new(config())?;
    let epoch = collector.extract_epoch()?;
    let mut first = frame(epoch);
    first
        .source
        .limitations
        .push("unobserved relevant material".into());
    collector.complete(first.clone())?;
    let nonce = collector.arm()?;
    collector.bind(nonce, 17)?;
    collector.callback(17)?;
    first.epoch = collector.extract_epoch()?;
    collector.complete(first)?;
    assert!(collector
        .finish()?
        .ok_or(AuditError::Lifecycle("missing result"))?
        .classification
        .starts_with("limited"));
    Ok(())
}

#[test]
fn second_view_change_cannot_hide_behind_first_view_stability() -> Result<(), AuditError> {
    let mut collector = Collector::new(config())?;
    let mut first = frame(collector.extract_epoch()?);
    let mut other = first.views[0].clone();
    other.render_entity = 3;
    first.views.push(other);
    collector.complete(first.clone())?;
    let nonce = collector.arm()?;
    collector.bind(nonce, 17)?;
    collector.callback(17)?;
    first.epoch = collector.extract_epoch()?;
    first.views[1].viewport[0] = 64;
    collector.complete(first)?;
    assert!(collector
        .finish()?
        .ok_or(AuditError::Lifecycle("missing result"))?
        .classification
        .starts_with("inconclusive"));
    Ok(())
}

#[test]
fn unsupported_phase_association_limits_otherwise_stable_sources() -> Result<(), AuditError> {
    let mut collector = Collector::new(config())?;
    let mut first = frame(collector.extract_epoch()?);
    let unknown = entry(bevy::prelude::Entity::from_raw(9), 4, -10.0);
    first.views[0].queued.push(unknown.clone());
    first.views[0].sorted.push(unknown);
    collector.complete(first.clone())?;
    let nonce = collector.arm()?;
    collector.bind(nonce, 17)?;
    collector.callback(17)?;
    first.epoch = collector.extract_epoch()?;
    collector.complete(first)?;
    let report = collector
        .finish()?
        .ok_or(AuditError::Lifecycle("report absent"))?;
    assert!(report.classification.starts_with("limited"));
    assert_eq!(report.epochs[0].source.limitations, Vec::<String>::new());
    Ok(())
}

#[test]
fn partial_asset_accounting_matches_complete_attachment() -> Result<(), AuditError> {
    let mut collector = Collector::new(config())?;
    let epoch = collector.extract_epoch()?;
    collector.complete(frame(epoch))?;
    let nonce = collector.arm()?;
    collector.bind(nonce, 17)?;
    let mut assets = super::assets::Inventory::default();
    assets.serialized_bytes = 123;
    assets.raw_bytes = 456;
    let partial = collector
        .incomplete()
        .ok_or(AuditError::Lifecycle("partial absent"))?;
    let before = partial.retained_bytes;
    let attached = super::attach_assets(partial, &assets)?;
    assert_eq!(attached.retained_bytes, before + 123);
    assert_eq!(attached.raw_source_bytes, 456);
    collector.callback(17)?;
    let epoch = collector.extract_epoch()?;
    collector.complete(frame(epoch))?;
    let complete = collector
        .finish()?
        .ok_or(AuditError::Lifecycle("complete absent"))?;
    let before = complete.retained_bytes;
    let attached = super::attach_assets(complete, &assets)?;
    assert_eq!(attached.retained_bytes, before + 123);
    assert_eq!(attached.raw_source_bytes, 456);
    Ok(())
}

#[test]
fn publication_checks_complete_encoding_not_component_sum() -> Result<(), AuditError> {
    let mut collector = Collector::new(config())?;
    let epoch = collector.extract_epoch()?;
    collector.complete(frame(epoch))?;
    collector.arm()?;
    let report = collector
        .incomplete()
        .ok_or(AuditError::Lifecycle("report absent"))?;
    let encoded = report.encode_bounded(config().max_bytes)?;
    assert!(encoded.len() > report.retained_bytes);
    assert!(matches!(
        report.encode_bounded(report.retained_bytes),
        Err(AuditError::Budget(_))
    ));
    assert_eq!(report.encode_bounded(encoded.len())?, encoded);
    assert!(matches!(
        report.encode_bounded(encoded.len() - 1),
        Err(AuditError::Budget(_))
    ));
    Ok(())
}

#[test]
fn controlled_centers_preserve_distinct_coordinate_contracts() -> Result<(), AuditError> {
    use bevy::prelude::Vec3;
    let center = Vec3::new(15.75, 1.5, 10.0);
    super::fixture::validate_centers(center, [15.75, 10.0, 1.5], center)?;
    assert!(super::fixture::validate_centers(
        center,
        [15.75, 10.0, 1.5],
        Vec3::new(15.75, 10.0, 1.5)
    )
    .is_err());
    assert!(super::fixture::validate_centers(center, [15.75, 1.5, 10.0], center).is_err());
    assert!(super::fixture::validate_centers(Vec3::NAN, [15.75, 10.0, 1.5], center).is_err());
    Ok(())
}

#[test]
fn shipped_audit_profiles_load_through_actual_capture_parser(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/captures");
    for name in [
        "shell_order_natural.ron",
        "shell_order_forward.ron",
        "shell_order_reverse.ron",
    ] {
        let script = crate::runtime_capture::CaptureScript::load(&root.join(name))?;
        assert_eq!(script.checkpoints.len(), 17);
        let audit = script.shell_audit.ok_or("explicit audit absent")?;
        assert_eq!(audit.fixture.is_some(), name != "shell_order_natural.ron");
    }
    assert!(
        crate::runtime_capture::CaptureScript::load(&root.join("player_ownership.ron"))?
            .shell_audit
            .is_none()
    );
    Ok(())
}

#[test]
fn publication_failure_retains_closed_interval_until_explicit_acknowledgement(
) -> Result<(), AuditError> {
    let mut collector = Collector::new(config())?;
    let epoch = collector.extract_epoch()?;
    collector.complete(frame(epoch))?;
    let nonce = collector.arm()?;
    collector.bind(nonce, 17)?;
    collector.callback(17)?;
    let epoch = collector.extract_epoch()?;
    collector.complete(frame(epoch))?;
    let report = collector
        .finish()?
        .ok_or(AuditError::Lifecycle("report absent"))?;
    assert!(report.encode_bounded(1).is_err());
    let retained = collector
        .incomplete()
        .ok_or(AuditError::Lifecycle("failed publication lost interval"))?;
    assert_eq!(retained.epochs, report.epochs);
    assert_eq!(retained.screenshot, Some(17));
    assert!(!collector.ready());
    assert_eq!(
        collector
            .finish()?
            .ok_or(AuditError::Lifecycle("closed report vanished"))?
            .epochs,
        report.epochs
    );
    report.encode_bounded(config().max_bytes)?;
    collector.acknowledge_publication()?;
    assert!(collector.incomplete().is_none());
    assert!(collector.ready());
    assert!(collector.acknowledge_publication().is_err());
    Ok(())
}
