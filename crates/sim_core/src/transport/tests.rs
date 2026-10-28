//! Deliberate crate-private relation corruption and validation-boundary evidence.
//! Public integration tests use real transport transactions; only this module forges
//! links to prove that malformed worlds never masquerade as ordinary loaded state.

#![allow(clippy::expect_used)]

use super::*;
use crate::movement::MovementAdmissionError;
use crate::production::{ProductionAdmissionError, ProductionInvariant, ProductionJob};
use crate::{SimWorld, SimulationDiagnostic};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerCommand, PlayerId, SimScalar, TickBundle, UnitOrder, UnitTypeId};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn fixture() -> TestResult<(SimWorld, [EntityId; 3])> {
    let map = GameMap::new(40, 40, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::new(map, 51);
    let kind = world
        .catalog
        .archetypes()
        .find(|a| !a.is_immobile_by_default() && !matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .ok_or("mobile catalog entry missing")?
        .unit_type_id;
    let mut ids = [EntityId(0); 3];
    for (index, id) in ids.iter_mut().enumerate() {
        let coordinate = [4, 24, 32][index];
        *id = world.spawn_unit(
            kind,
            PlayerId(0),
            SimVec2::new(
                SimScalar::from_num(coordinate),
                SimScalar::from_num(coordinate),
            ),
        )?;
    }
    world.try_load_unit_into_transport(ids[0], ids[1])?;
    Ok((world, ids))
}

#[test]
fn malformed_links_block_tick_before_any_gameplay_side_effect() -> TestResult {
    type Corrupt = fn(&mut SimWorld, [EntityId; 3]);
    let cases: &[(&str, Corrupt)] = &[
        ("dangling", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[0])
                .expect("fixture contains every spawned participant");
            e.transport_carrier = Some(EntityId(u32::MAX));
        }),
        ("self", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[0])
                .expect("fixture contains every spawned participant");
            e.transport_carrier = Some(ids[0]);
        }),
        ("nested", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[1])
                .expect("fixture contains every spawned participant");
            e.transport_carrier = Some(ids[2]);
        }),
        ("owner", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[0])
                .expect("fixture contains every spawned participant");
            e.owner = PlayerId(1);
        }),
        ("owner slot", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[1])
                .expect("fixture contains every spawned participant");
            e.owner = PlayerId(255);
        }),
        ("identity", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[0])
                .expect("fixture contains every spawned participant");
            e.id = ids[2];
        }),
        ("completion", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[1])
                .expect("fixture contains every spawned participant");
            e.is_completed = false;
        }),
        ("catalog", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[1])
                .expect("fixture contains every spawned participant");
            e.unit_type_id = UnitTypeId(u16::MAX);
        }),
        ("channel", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[0])
                .expect("fixture contains every spawned participant");
            e.channeling_remaining_ticks = 1;
        }),
        ("pending", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[0])
                .expect("fixture contains every spawned participant");
            e.pending_ability = Some((0, None, None));
        }),
        ("job", |w, ids| {
            let e = w
                .entities
                .get_mut(&ids[0])
                .expect("fixture contains every spawned participant");
            e.production_queue.push_back(ProductionJob {
                product: e.unit_type_id,
                remaining_ticks: 2,
                reserved_supply: 0,
            });
        }),
    ];
    for (name, corrupt) in cases {
        let (mut world, ids) = fixture()?;
        corrupt(&mut world, ids);
        assert!(world.validate_transport_links().is_err(), "{name}");
        let entities = world.entities.clone();
        let resources = world.player_resources;
        let rng = world.rng.get_word_pos();
        let input = TickBundle {
            tick: Tick(0),
            state_checksum: None,
            commands: vec![PlayerCommand {
                player_id: PlayerId(0),
                entities: vec![ids[2]],
                order: UnitOrder::Move {
                    target: SimVec2::ZERO,
                },
                queue: false,
            }],
        };
        let output = world.step(&input);
        assert_eq!(world.current_tick, 1);
        assert_eq!(world.entities, entities, "{name}");
        assert_eq!(world.player_resources, resources, "{name}");
        assert_eq!(world.rng.get_word_pos(), rng, "{name}");
        assert_eq!(world.movement_outcomes(), []);
        assert_eq!(world.production_outcomes(), []);
        assert!(
            matches!(
                world.diagnostics(),
                [SimulationDiagnostic::TransportInvariant { .. }]
            ),
            "{name}"
        );
        assert_eq!(output.checksum, world.compute_checksum());
    }
    Ok(())
}

#[test]
fn independent_world_ingress_reports_corruption_before_loaded_rejection() -> TestResult {
    let (mut world, ids) = fixture()?;
    world
        .entities
        .get_mut(&ids[0])
        .ok_or("passenger")?
        .transport_carrier = Some(EntityId(u32::MAX));
    let before = world.compute_checksum();
    let kind = world.entities[&ids[2]].unit_type_id;
    assert!(matches!(
        world.try_issue_movement(
            PlayerId(0),
            &[ids[0]],
            &UnitOrder::Move {
                target: SimVec2::ZERO
            },
            false
        ),
        Err(MovementAdmissionError::Invariant(
            MovementInvariant::Transport(_)
        ))
    ));
    assert_eq!(world.drain_diagnostics().count(), 1);
    assert!(matches!(
        world.try_queue_production(PlayerId(0), &[ids[0]], kind),
        Err(ProductionAdmissionError::Invariant(
            ProductionInvariant::Transport(_)
        ))
    ));
    assert_eq!(world.drain_diagnostics().count(), 1);
    assert!(!world.execute_ability(ids[0], 0, None, None));
    assert_eq!(world.drain_diagnostics().count(), 1);
    world.process_commands(&TickBundle {
        tick: Tick(0),
        commands: vec![],
        state_checksum: None,
    });
    assert_eq!(world.drain_diagnostics().count(), 1);
    world.apply_combat_retaliation(ids[0], ids[2]);
    assert_eq!(world.drain_diagnostics().count(), 1);
    assert!(matches!(
        world.compute_entity_flee_destination(ids[0], SimVec2::ZERO, SimScalar::ONE),
        Err(MovementInvariant::Transport(_))
    ));
    assert!(matches!(
        world.validate_displacement_destination(ids[0], SimVec2::ZERO),
        Err(DisplacementAdmissionError::Invariant(
            MovementInvariant::Transport(_)
        ))
    ));
    assert!(world.has_transport_passengers(ids[1]).is_err());
    assert!(!world.load_unit_into_transport(ids[2], ids[1]));
    assert_eq!(world.drain_diagnostics().count(), 1);
    assert!(!world.unload_unit_from_transport(ids[0], SimVec2::ZERO));
    assert_eq!(world.drain_diagnostics().count(), 1);
    assert_eq!(world.compute_checksum(), before);
    Ok(())
}

#[test]
fn normal_batched_tick_validates_at_tick_and_reap_not_per_command_or_caster() -> TestResult {
    let (mut world, ids) = fixture()?;
    let kind = world.entities[&ids[2]].unit_type_id;
    world.catalog.get_mut(kind)?.abilities = vec![sim_types::AbilitySpec {
        name: "transport-validation-fixture".into(),
        target_kind: sim_types::AbilityTargetKind::InstantSelf,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 0,
        cast_range: SimScalar::ZERO,
        effect: sim_types::AbilityEffectSpec::CloakToggle {
            energy_cost_per_second: SimScalar::ZERO,
            is_burrow: false,
        },
    }];
    for id in [ids[1], ids[2]] {
        world.entities.get_mut(&id).ok_or("caster")?.energy = SimScalar::from_num(100);
    }
    let mut commands = Vec::new();
    for _ in 0..15 {
        commands.push(PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![ids[2]],
            queue: false,
            order: UnitOrder::Move {
                target: SimVec2::from_i32(34, 32),
            },
        });
        commands.push(PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![ids[1], ids[2]],
            queue: false,
            order: UnitOrder::CastAbility {
                ability_index: 0,
                target_pos: None,
                target_entity: None,
            },
        });
    }
    world.transport_validation_count.set(0);
    world.step(&TickBundle {
        tick: Tick(0),
        commands,
        state_checksum: None,
    });
    assert_eq!(world.transport_validation_count.get(), 2);
    assert!(world.entities[&ids[1]].is_cloaked);
    assert!(world.entities[&ids[2]].is_cloaked);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

#[test]
fn malformed_graph_precedes_reap_and_never_partially_kills_passengers() -> TestResult {
    let (mut world, ids) = fixture()?;
    world.entities.get_mut(&ids[1]).ok_or("carrier")?.hp = SimScalar::ZERO;
    world
        .entities
        .get_mut(&ids[0])
        .ok_or("passenger")?
        .unit_type_id = UnitTypeId(u16::MAX);
    let before = world.entities.clone();
    let resources = world.player_resources;
    world.reap_dead_and_emit_shatter();
    assert_eq!(world.entities, before);
    assert_eq!(world.player_resources, resources);
    assert_eq!(world.pending_events, []);
    assert!(matches!(
        world.diagnostics(),
        [SimulationDiagnostic::TransportInvariant { .. }]
    ));
    Ok(())
}

#[test]
fn ledger_failure_blocks_every_death_group_before_any_release() -> TestResult {
    let (mut world, ids) = fixture()?;
    world.entities.get_mut(&ids[1]).ok_or("carrier")?.hp = SimScalar::ZERO;
    world.entities.get_mut(&ids[2]).ok_or("unrelated")?.hp = SimScalar::ZERO;
    world.player_resources[0].supply_used += 1;
    let before = world.entities.clone();
    let resources = world.player_resources;
    world.reap_dead_and_emit_shatter();
    assert_eq!(world.entities, before);
    assert_eq!(world.player_resources, resources);
    assert_eq!(world.pending_events, []);
    assert!(matches!(
        world.diagnostics(),
        [SimulationDiagnostic::ProductionInvariant {
            source: ProductionInvariant::LedgerMismatch { .. },
            ..
        }]
    ));
    Ok(())
}
