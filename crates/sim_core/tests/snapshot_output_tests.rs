//! Checks reusable snapshot extraction against the compatibility API and real
//! lifecycle events. Allocation capacity/identity checks are scoped to these
//! buffers, not a claim that every simulation phase is allocation-free.

use sim_core::SimWorld;
use sim_types::{RenderSnapshot, SimScalar, Tick, TickBundle};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn destroy_first_live_units(world: &mut SimWorld, count: usize) {
    for entity in world.entities.values_mut().take(count) {
        entity.hp = SimScalar::ZERO;
    }
    world.reap_dead_and_emit_shatter();
}

#[test]
fn reusable_and_compatibility_ticks_match_through_combat_and_deaths() -> TestResult {
    let mut allocating = SimWorld::new_default_battle_sandbox();
    let mut reusable = SimWorld::new_default_battle_sandbox();
    let mut output = RenderSnapshot::default();
    let mut observed_events = 0;
    for tick in 0..120 {
        if tick == 40 {
            destroy_first_live_units(&mut allocating, 3);
            destroy_first_live_units(&mut reusable, 3);
        }
        let bundle = TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        };
        let expected = allocating.step(&bundle);
        reusable.step_into(&bundle, &mut output);
        assert_eq!(
            output, expected,
            "output boundary mismatch at input tick {tick}"
        );
        assert_eq!(ron::to_string(&output)?, ron::to_string(&expected)?);
        assert_eq!(output.tick, Tick(tick + 1));
        assert_eq!(reusable.last_checksum(), allocating.last_checksum());
        assert!(
            reusable.diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            reusable.diagnostics()
        );
        observed_events += output.combat_events.len();
    }
    assert!(observed_events >= 3, "real death events must reach output");
    Ok(())
}

#[test]
fn extraction_drains_once_and_preserves_output_and_producer_capacity() -> TestResult {
    let mut world = SimWorld::new_default_battle_sandbox();
    let mut output = RenderSnapshot::default();
    destroy_first_live_units(&mut world, 3);
    let checksum_before = world.compute_checksum();
    let tick_before = world.current_tick;
    let pending_capacity = world.pending_events.capacity();
    let pending_storage = world.pending_events.as_ptr();
    world.write_render_snapshot(&mut output);
    assert_eq!(output.combat_events.len(), 3);
    assert_eq!(world.pending_events, []);
    let bubble_storage = output.bubbles.as_ptr();
    let event_storage = output.combat_events.as_ptr();
    let capacities = (output.bubbles.capacity(), output.combat_events.capacity());
    for _ in 0..64 {
        world.write_render_snapshot(&mut output);
        assert_eq!(output.combat_events, []);
        assert_eq!(output.bubbles.as_ptr(), bubble_storage);
        assert_eq!(output.combat_events.as_ptr(), event_storage);
        assert_eq!(
            (output.bubbles.capacity(), output.combat_events.capacity()),
            capacities
        );
        assert_eq!(world.pending_events.capacity(), pending_capacity);
        assert_eq!(world.pending_events.as_ptr(), pending_storage);
    }
    assert_eq!(world.compute_checksum(), checksum_before);
    assert_eq!(world.current_tick, tick_before);
    let instance = output
        .bubbles
        .first()
        .ok_or("fixture must retain live entities")?;
    let entity = world
        .entities
        .get(&instance.entity_id)
        .ok_or("snapshot entity missing")?;
    assert_eq!(instance.world_pos[0], entity.position.x.to_f32());
    assert_eq!(instance.world_pos[1], entity.position.y.to_f32());
    Ok(())
}

#[test]
fn growth_and_later_shrink_replace_membership_without_replaying_events() -> TestResult {
    let mut world = SimWorld::new_default_battle_sandbox();
    let mut output = world.build_render_snapshot();
    let old_len = output.bubbles.len();
    let actor = world
        .entities
        .values()
        .find(|entity| !entity.is_immobile)
        .ok_or("mobile fixture missing")?;
    let (archetype, owner, position) = (actor.unit_type_id, actor.owner, actor.position);
    let id = world.spawn_unit(archetype, owner, position)?;
    world.write_render_snapshot(&mut output);
    assert_eq!(output.bubbles.len(), old_len + 1);
    assert!(output
        .bubbles
        .iter()
        .any(|instance| instance.entity_id == id));
    world.entities.get_mut(&id).ok_or("spawn missing")?.hp = SimScalar::ZERO;
    world.reap_dead_and_emit_shatter();
    world.write_render_snapshot(&mut output);
    assert_eq!(output.bubbles.len(), old_len);
    assert_eq!(output.combat_events.len(), 1);
    assert!(!output
        .bubbles
        .iter()
        .any(|instance| instance.entity_id == id));
    world.write_render_snapshot(&mut output);
    assert_eq!(output.combat_events, []);
    world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn command_trace_preserves_paid_jobs_and_per_tick_production_movement_outcomes() -> TestResult {
    use sim_types::{PlayerCommand, SimVec2, UnitOrder, UnitTypeId};
    let mut allocating = SimWorld::new_default_battle_sandbox();
    let mut reusable = SimWorld::new_default_battle_sandbox();
    let (producer, owner, product) = allocating
        .entities
        .values()
        .find_map(|entity| {
            let archetype = allocating.catalog.get(entity.unit_type_id).ok()?;
            let product = archetype
                .trains_units
                .iter()
                .find_map(|slug| allocating.catalog.get_by_slug(slug))?;
            Some((entity.id, entity.owner, product.unit_type_id))
        })
        .ok_or("authored producer fixture missing")?;
    let mover = allocating
        .entities
        .values()
        .find(|entity| entity.owner == owner && !entity.is_immobile)
        .ok_or("mobile recipient missing")?
        .id;
    let bundle = TickBundle {
        tick: Tick(0),
        commands: vec![
            PlayerCommand {
                player_id: owner,
                entities: vec![producer],
                order: UnitOrder::Produce { unit_type: product },
                queue: true,
            },
            PlayerCommand {
                player_id: owner,
                entities: vec![producer],
                order: UnitOrder::Produce {
                    unit_type: UnitTypeId(u16::MAX),
                },
                queue: true,
            },
            PlayerCommand {
                player_id: owner,
                entities: vec![mover],
                order: UnitOrder::Move {
                    target: SimVec2::new(SimScalar::from_i32(4), SimScalar::from_i32(4)),
                },
                queue: true,
            },
        ],
        state_checksum: None,
    };
    let expected = allocating.step(&bundle);
    let mut output = RenderSnapshot::default();
    reusable.step_into(&bundle, &mut output);
    assert_eq!(output, expected);
    let expected_production: Vec<_> = allocating.drain_production_outcomes().collect();
    let production: Vec<_> = reusable.drain_production_outcomes().collect();
    assert_eq!(production, expected_production);
    assert_eq!(production.len(), 2);
    assert!(
        production[0].result.is_ok(),
        "real paid production must be accepted: {:?}",
        production[0]
    );
    assert!(production[1].result.is_err());
    assert_eq!(
        (production[0].tick, production[0].command_index),
        (Tick(1), 0)
    );
    assert_eq!(
        (production[1].tick, production[1].command_index),
        (Tick(1), 1)
    );
    let expected_movement: Vec<_> = allocating.drain_movement_outcomes().collect();
    let movement: Vec<_> = reusable.drain_movement_outcomes().collect();
    assert_eq!(movement, expected_movement);
    assert_eq!(movement.len(), 1);
    assert_eq!((movement[0].tick, movement[0].command_index), (Tick(1), 2));
    for tick in 1..64 {
        let bundle = TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        };
        let expected = allocating.step(&bundle);
        reusable.step_into(&bundle, &mut output);
        assert_eq!(output, expected);
        assert_eq!(reusable.production_outcomes(), []);
        assert_eq!(reusable.movement_outcomes(), []);
        assert_eq!(reusable.compute_checksum(), allocating.compute_checksum());
    }
    Ok(())
}
