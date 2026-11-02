//! CPU mapping/permutation negatives; real material registration is tested on the renderer.

use super::super::model::{LogicalIdentity, PhaseEntry};
use super::*;
use bevy::render::render_phase::RenderCommandState;
use std::any::TypeId;

fn registry(world: &mut World) -> DrawFunctionId {
    let functions = DrawFunctions::<Transparent3d>::default();
    // This CPU fixture tests lookup/admission, not execution of this placeholder
    // command. The real renderer regression checks actual plugin registration.
    let command = RenderCommandState::<Transparent3d, ()>::new(world);
    let id = functions
        .write()
        .add_with::<MaterialDraw<StandardMaterial>, _>(command);
    world.insert_resource(functions);
    id
}
fn visible(pairs: &[(Entity, MainEntity)]) -> RenderVisibleEntities {
    let mut visible = RenderVisibleEntities::default();
    visible
        .entities
        .insert(TypeId::of::<With<Mesh3d>>(), pairs.to_vec());
    visible
}
fn material() -> MaterialSource {
    MaterialSource::Standard {
        id: "CPU adapter fixture".into(),
        description: "no draw submitted".into(),
    }
}

#[test]
fn actual_pairs_markers_and_registered_adapter_are_all_required(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::new();
    let draw = registry(&mut world);
    let main = world.spawn_empty().id();
    let proxy = world.spawn(TemporaryRenderEntity).id();
    let pairs = VisiblePairs::new(&visible(&[(proxy, main.into())]), 8)?;
    let proof = pairs.inspect(&world, proxy, main.into(), draw, &material())?;
    assert_eq!(
        proof.kind,
        AssociationKind::Temporary(DrawAdapter::Standard)
    );
    assert_eq!(proof.render_main_entity, None);
    let wrong = world.spawn_empty().id();
    assert!(pairs
        .inspect(&world, proxy, wrong.into(), draw, &material())
        .is_err());
    assert!(VisiblePairs::new(&visible(&[(proxy, main.into()), (proxy, main.into())]), 8).is_err());
    assert!(
        VisiblePairs::new(&visible(&[(proxy, main.into()), (proxy, wrong.into())]), 8).is_err()
    );
    let empty = VisiblePairs::new(&RenderVisibleEntities::default(), 8)?;
    assert!(empty
        .inspect(&world, proxy, main.into(), draw, &material())
        .is_err());
    world.entity_mut(proxy).insert(MainEntity::from(wrong));
    assert!(pairs
        .inspect(&world, proxy, main.into(), draw, &material())
        .is_err());
    world
        .entity_mut(proxy)
        .remove::<MainEntity>()
        .remove::<TemporaryRenderEntity>();
    assert!(matches!(
        pairs
            .inspect(&world, proxy, main.into(), draw, &material())?
            .kind,
        AssociationKind::Unsupported(_)
    ));
    world.entity_mut(proxy).insert(MainEntity::from(main));
    assert_eq!(
        pairs
            .inspect(&world, proxy, main.into(), draw, &material())?
            .kind,
        AssociationKind::Synchronized(DrawAdapter::Standard)
    );
    world.despawn(proxy);
    assert!(pairs
        .inspect(&world, proxy, main.into(), draw, &material())
        .is_err());
    let replacement = world.spawn(TemporaryRenderEntity).id();
    assert_ne!(replacement.to_bits(), proxy.to_bits());
    assert!(pairs
        .inspect(&world, replacement, main.into(), draw, &material())
        .is_err());
    Ok(())
}

#[test]
fn missing_registration_is_pending_and_wrong_tuple_is_unsupported(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::new();
    let draw = registry(&mut world);
    let main = world.spawn_empty().id();
    let proxy = world.spawn(TemporaryRenderEntity).id();
    let pairs = VisiblePairs::new(&visible(&[(proxy, main.into())]), 8)?;
    let functions = world
        .remove_resource::<DrawFunctions<Transparent3d>>()
        .ok_or("registry")?;
    assert_eq!(
        pairs
            .inspect(&world, proxy, main.into(), draw, &material())?
            .kind,
        AssociationKind::Pending
    );
    let command = RenderCommandState::<Transparent3d, ()>::new(&mut world);
    let custom = functions.write().add(command);
    world.insert_resource(functions);
    assert!(matches!(
        pairs
            .inspect(&world, proxy, main.into(), custom, &material())?
            .kind,
        AssociationKind::Unsupported(_)
    ));
    let shell = MaterialSource::Shell {
        id: "fixture".into(),
        uniform: vec![],
        image: "fixture".into(),
    };
    assert_eq!(
        pairs
            .inspect(&world, proxy, main.into(), draw, &shell)?
            .kind,
        AssociationKind::Pending
    );
    Ok(())
}

fn item() -> PhaseEntry {
    PhaseEntry {
        ordinal: 0,
        render_entity: 10,
        main_entity: 20,
        association: RenderAssociation {
            visible_pair_confirmed: true,
            temporary_marker: true,
            render_main_entity: None,
            kind: AssociationKind::Temporary(DrawAdapter::Standard),
        },
        logical: Some(LogicalIdentity {
            domain: "Swarm",
            entity: 1000,
            generation: 0,
        }),
        distance_bits: 0.0_f32.to_bits(),
        pipeline: "pipeline".into(),
        pipeline_description: "descriptor".into(),
        draw_function: "draw".into(),
        material: material(),
        mesh: Some("mesh".into()),
        batch_range: [0, 1],
        extra_index: "none".into(),
    }
}

#[test]
fn only_admitted_temporary_raw_id_can_differ_between_ordered_epochs() {
    let a = item();
    let mut b = a.clone();
    b.render_entity += 1;
    assert_ne!(a, b);
    assert!(a.same_draw(&b));
    let mutations: &[fn(&mut PhaseEntry)] = &[
        |v| v.ordinal += 1,
        |v| v.main_entity += 1,
        |v| {
            if let Some(id) = &mut v.logical {
                id.generation += 1;
            }
        },
        |v| {
            if let Some(id) = &mut v.logical {
                id.entity += 1;
            }
        },
        |v| v.distance_bits = (-0.0_f32).to_bits(),
        |v| v.pipeline.push('x'),
        |v| v.pipeline_description.push('x'),
        |v| v.draw_function.push('x'),
        |v| v.mesh = None,
        |v| v.batch_range[1] += 1,
        |v| v.extra_index.push('x'),
        |v| v.material = MaterialSource::Unsupported { reason: "changed" },
        |v| v.association.temporary_marker = false,
        |v| v.association.visible_pair_confirmed = false,
        |v| v.association.render_main_entity = Some(20),
        |v| v.association.kind = AssociationKind::Synchronized(DrawAdapter::Standard),
    ];
    for mutate in mutations {
        let mut changed = b.clone();
        mutate(&mut changed);
        assert!(
            !a.same_draw(&changed),
            "mutation must remain observable: {changed:?}"
        );
    }
    let mut persistent = a.clone();
    persistent.association.kind = AssociationKind::Synchronized(DrawAdapter::Standard);
    persistent.association.temporary_marker = false;
    let mut replacement = persistent.clone();
    replacement.render_entity += 1;
    assert!(!persistent.same_draw(&replacement));
}

#[test]
fn raw_sort_permutation_does_not_apply_cross_epoch_proxy_exemption(
) -> Result<(), Box<dyn std::error::Error>> {
    let a = item();
    let mut b = a.clone();
    b.render_entity += 1;
    b.main_entity += 1;
    b.ordinal = 1;
    let before = vec![a.clone(), b.clone()];
    let mut after = vec![b, a];
    for (ordinal, item) in after.iter_mut().enumerate() {
        item.ordinal = ordinal;
    }
    verify_permutation(&before, &after)?;
    after[0].render_entity += 1;
    assert!(verify_permutation(&before, &after).is_err());
    assert!(verify_permutation(&before, &before[..1]).is_err());
    assert!(verify_permutation(&before, &[before[0].clone(), before[0].clone()]).is_err());
    Ok(())
}

#[test]
fn interval_accepts_proven_proxy_churn_but_keeps_real_order_changes(
) -> Result<(), Box<dyn std::error::Error>> {
    use super::super::{interval::Collector, model::AuditConfig, tests::frame};
    for reorder in [false, true] {
        let config = AuditConfig {
            max_views: 2,
            max_items: 8,
            max_epochs: 8,
            max_bytes: 1_048_576,
            max_source_bytes: 1_048_576,
            fixture: None,
        };
        let mut collector = Collector::new(config)?;
        let mut first = frame(collector.extract_epoch()?);
        let a = item();
        let mut b = a.clone();
        b.main_entity += 1;
        b.render_entity += 1;
        b.ordinal = 1;
        b.logical.as_mut().ok_or("logical")?.entity += 1;
        first.views[0].sorted = vec![a, b];
        collector.complete(first.clone())?;
        let nonce = collector.arm()?;
        collector.bind(nonce, 99)?;
        collector.callback(99)?;
        let mut next = first;
        next.epoch = collector.extract_epoch()?;
        for item in &mut next.views[0].sorted {
            item.render_entity += 100;
        }
        if reorder {
            next.views[0].sorted.reverse();
            for (ordinal, item) in next.views[0].sorted.iter_mut().enumerate() {
                item.ordinal = ordinal;
            }
        }
        collector.complete(next)?;
        let report = collector.finish()?.ok_or("closed report")?;
        assert_eq!(report.epochs.len(), 2);
        assert_eq!(report.classification.starts_with("stable"), !reorder);
    }
    Ok(())
}

#[test]
fn every_actual_material_content_revision_remains_semantically_distinct() {
    let mut baseline = item();
    baseline.material = MaterialSource::Shell {
        id: "same handle".into(),
        uniform: vec![0; 160],
        image: "image A".into(),
    };
    let variants = [
        MaterialSource::Shell {
            id: "other handle".into(),
            uniform: vec![0; 160],
            image: "image A".into(),
        },
        MaterialSource::Shell {
            id: "same handle".into(),
            uniform: vec![1; 160],
            image: "image A".into(),
        },
        MaterialSource::Shell {
            id: "same handle".into(),
            uniform: vec![0; 160],
            image: "image B".into(),
        },
    ];
    for material in variants {
        let mut changed = baseline.clone();
        changed.material = material;
        assert!(!baseline.same_draw(&changed));
    }
    baseline.material = MaterialSource::Standard {
        id: "same".into(),
        description: "source A".into(),
    };
    let mut changed = baseline.clone();
    changed.material = MaterialSource::Standard {
        id: "same".into(),
        description: "source B".into(),
    };
    assert!(!baseline.same_draw(&changed));
}

#[test]
fn unchanged_unsupported_or_pending_association_never_certifies_complete_coverage(
) -> Result<(), Box<dyn std::error::Error>> {
    use super::super::{interval::Collector, model::AuditConfig, tests::frame};
    for kind in [
        AssociationKind::Unsupported("custom command"),
        AssociationKind::Pending,
    ] {
        let mut collector = Collector::new(AuditConfig {
            max_views: 2,
            max_items: 8,
            max_epochs: 8,
            max_bytes: 1_048_576,
            max_source_bytes: 1_048_576,
            fixture: None,
        })?;
        let mut first = frame(collector.extract_epoch()?);
        let mut draw = item();
        draw.association.kind = kind;
        first.views[0].sorted.push(draw);
        collector.complete(first.clone())?;
        let nonce = collector.arm()?;
        collector.bind(nonce, 99)?;
        collector.callback(99)?;
        first.epoch = collector.extract_epoch()?;
        collector.complete(first)?;
        let report = collector.finish()?.ok_or("report")?;
        assert!(report.classification.starts_with("limited"));
    }
    Ok(())
}
