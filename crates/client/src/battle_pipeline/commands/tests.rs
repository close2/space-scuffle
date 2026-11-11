//! Verifies captured recipients, queue ordering and generation replacement without
//! advancing the simulation from submission or no-tick presentation frames.

use super::*;
use sim_core::production::{ProductionItem, ProductionJob};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{MatchResult, MoverType, PlayerSlot, ResourceNodeId, ShapeKind, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const ME: PlayerId = PlayerId(0);
const THEM: PlayerId = PlayerId(1);

fn fixture(
) -> Result<(SimWorld, BattleCommandQueue, EntityId, PlayerId), Box<dyn std::error::Error>> {
    let world = sim_fixtures::default_battle_sandbox()?;
    let actor = world
        .entities
        .values()
        .find(|e| !e.is_immobile)
        .ok_or("mobile fixture absent")?;
    let (id, owner) = (actor.id, actor.owner);
    let mut queue = BattleCommandQueue::default();
    queue.activate(SessionGeneration::FIRST)?;
    Ok((world, queue, id, owner))
}

fn command(id: EntityId, owner: PlayerId, order: UnitOrder, queued: bool) -> PlayerCommand {
    PlayerCommand {
        player_id: owner,
        entities: vec![id],
        order,
        queue: queued,
    }
}

fn cast_ability() -> UnitOrder {
    UnitOrder::CastAbility {
        ability_index: 0,
        target_pos: None,
        target_entity: None,
    }
}

fn is_dome(archetype: &UnitArchetype) -> bool {
    matches!(archetype.shape, ShapeKind::BuildingDome { .. })
}

/// The first catalog identity `wanted` accepts; roles are found by capability, never by name.
fn type_where(
    world: &SimWorld,
    wanted: impl Fn(&UnitArchetype) -> bool,
) -> Result<UnitTypeId, Box<dyn std::error::Error>> {
    Ok(world
        .catalog
        .archetypes()
        .find(|archetype| wanted(archetype))
        .map(|archetype| archetype.unit_type_id)
        .ok_or("the catalog has no such role")?)
}

/// An empty flat world with a funded player, so nothing but the order under test can refuse.
fn role_fixture() -> Result<(SimWorld, BattleCommandQueue), Box<dyn std::error::Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 7);
    world.player_resources[PlayerSlot::ALL[0]].minerals = 10_000;
    world.player_resources[PlayerSlot::ALL[0]].gas = 10_000;
    let mut queue = BattleCommandQueue::default();
    queue.activate(SessionGeneration::FIRST)?;
    Ok((world, queue))
}

/// Spawns a mobile unit of `ME` in the middle of the map's lower half, spaced by `slot`.
fn unit_where(
    world: &mut SimWorld,
    slot: i32,
    wanted: impl Fn(&UnitArchetype) -> bool,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let unit_type = type_where(world, |a| !is_dome(a) && wanted(a))?;
    let position = SimVec2::new(SimScalar::from_i32(8 + 3 * slot), SimScalar::from_i32(10));
    Ok(world.spawn_unit(unit_type, ME, position)?)
}

fn builder(world: &mut SimWorld, slot: i32) -> Result<EntityId, Box<dyn std::error::Error>> {
    unit_where(world, slot, |a| a.capabilities.can_construct)
}

/// A mobile unit that gathers, builds and carries nothing.
fn soldier(world: &mut SimWorld, slot: i32) -> Result<EntityId, Box<dyn std::error::Error>> {
    unit_where(world, slot, |a| {
        is_armed(a)
            && a.capabilities.harvester.is_none()
            && !a.capabilities.can_construct
            && a.capabilities.transport_capacity == 0
            && a.mover_type == MoverType::Ground
    })
}

/// A structure of `builder`'s faction that the player can start at once on open ground.
fn buildable(
    world: &SimWorld,
    builder: EntityId,
) -> Result<UnitTypeId, Box<dyn std::error::Error>> {
    let faction = world
        .catalog
        .get(
            world
                .entities
                .get(&builder)
                .ok_or("builder absent")?
                .unit_type_id,
        )?
        .faction;
    type_where(world, |a| {
        is_dome(a)
            && a.faction == faction
            && a.requires_structures.is_empty()
            && !a.capabilities.is_gas_extractor
    })
}

/// Runs prepared commands through the real tick path.
fn run_tick(world: &mut SimWorld, queue: &mut BattleCommandQueue) -> TestResult {
    let (mut input, mut sequences, mut rejected) = (bundle(), Vec::new(), Vec::new());
    queue.prepare_tick(world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(rejected, []);
    world.step(&input);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// Orders a real foundation, ticks until its builder has laid it, and returns both.
fn lay_foundation(
    world: &mut SimWorld,
    queue: &mut BattleCommandQueue,
) -> Result<(EntityId, EntityId), Box<dyn std::error::Error>> {
    let worker = builder(world, 0)?;
    let structure_type = buildable(world, worker)?;
    let order = UnitOrder::Build {
        structure_type,
        cell_x: 20,
        cell_y: 20,
    };
    queue.submit(world, command(worker, ME, order, false))?;
    for _ in 0..1200 {
        run_tick(world, queue)?;
        let laid = world.entities.values().find(|e| e.construction.is_some());
        if let Some(foundation) = laid {
            assert!(!foundation.is_completed);
            return Ok((worker, foundation.id));
        }
    }
    Err("the builder never laid the foundation".into())
}

/// `incapable` is refused alone, and left out of a selection that also holds `capable`.
fn assert_only_capable_receives(
    world: &SimWorld,
    queue: &mut BattleCommandQueue,
    capable: EntityId,
    incapable: EntityId,
    order: &UnitOrder,
) -> TestResult {
    assert_eq!(
        queue.submit(world, command(incapable, ME, order.clone(), false)),
        Err(SubmissionError::Capability(incapable))
    );
    queue.submit_selection(world, ME, &[incapable, capable], order.clone(), false)?;
    let (_, sent) = queue.pending_commands().last().ok_or("nothing queued")?;
    assert_eq!(sent.entities, [capable]);
    assert_eq!(&sent.order, order);
    Ok(())
}

fn bundle() -> TickBundle {
    TickBundle {
        tick: Tick(0),
        commands: Vec::new(),
        state_checksum: None,
    }
}

#[test]
fn submit_never_steps_and_prepared_bundle_preserves_order_and_queue_flag() -> TestResult {
    let (world, mut queue, id, owner) = fixture()?;
    let checksum = world.compute_checksum();
    let a = queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    let b = queue.submit(
        &world,
        command(
            id,
            owner,
            UnitOrder::Move {
                target: SimVec2::new(SimScalar::from_i32(4), SimScalar::from_i32(4)),
            },
            true,
        ),
    )?;
    assert_eq!(world.current_tick, 0);
    assert_eq!(world.compute_checksum(), checksum);
    assert_eq!(queue.pending_len(), 2);
    let mut input = bundle();
    let mut sequences = Vec::new();
    let mut rejected = Vec::new();
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(sequences, [a, b]);
    assert_eq!(input.commands.len(), 2);
    assert!(!input.commands[0].queue);
    assert!(input.commands[1].queue);
    assert_eq!(input.commands[0].order, UnitOrder::HoldPosition);
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(rejected, []);
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(input.commands, []);
    assert_eq!(sequences, []);
    Ok(())
}

#[test]
fn an_immediate_refusal_outlives_a_later_submission() -> TestResult {
    let (world, mut queue, id, owner) = fixture()?;
    assert_eq!(queue.latest_refusal(), None);

    queue.reject(&SubmissionError::NoDestination);
    assert_eq!(
        queue.latest_refusal(),
        Some((1, &SubmissionError::NoDestination))
    );
    assert_eq!(
        queue.status(),
        "Order rejected: there is no walkable ground there"
    );

    queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    assert_eq!(queue.status(), "1 order queued");
    assert_eq!(
        queue.latest_refusal(),
        Some((1, &SubmissionError::NoDestination)),
        "routine progress must not hide a refusal"
    );

    queue.reject(&SubmissionError::NoDestination);
    assert_eq!(
        queue.latest_refusal(),
        Some((2, &SubmissionError::NoDestination)),
        "a repeated refusal is a new one"
    );
    Ok(())
}

#[test]
fn a_revalidation_failure_is_reported_per_player_and_not_as_a_refusal() -> TestResult {
    let (mut world, mut queue, id, owner) = fixture()?;
    queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    world.match_state.result = MatchResult::Draw { tick: Tick(0) };

    let (mut input, mut sequences, mut rejected) = (bundle(), Vec::new(), Vec::new());
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;

    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].player, owner);
    assert_eq!(queue.status(), "Order rejected: the match is over");
    assert_eq!(queue.latest_refusal(), None);
    Ok(())
}

#[test]
fn a_new_session_forgets_its_refusals() -> TestResult {
    let (_, mut queue, _, _) = fixture()?;
    queue.reject(&SubmissionError::NoDestination);
    queue.activate(SessionGeneration::FIRST.next()?)?;
    assert_eq!(queue.latest_refusal(), None);
    Ok(())
}

#[test]
fn dead_or_foreign_captured_actor_is_rejected_without_substitution() -> TestResult {
    for foreign in [false, true] {
        let (mut world, mut queue, id, owner) = fixture()?;
        let sequence = queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
        let actor = world.entities.get_mut(&id).ok_or("actor absent")?;
        if foreign {
            actor.owner = PlayerId(owner.0 + 1);
        } else {
            actor.hp = SimScalar::ZERO;
        }
        let mut input = bundle();
        let mut sequences = Vec::new();
        let mut rejected = Vec::new();
        queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
        assert_eq!(input.commands, []);
        assert_eq!(sequences, []);
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].sequence, sequence);
        assert_eq!(
            rejected[0].reason,
            if foreign {
                SubmissionError::ForeignRecipient(id)
            } else {
                SubmissionError::UnavailableRecipient(id)
            }
        );
        assert_eq!(world.current_tick, 0);
    }
    Ok(())
}

#[test]
fn replacement_discards_pending_old_generation_and_identity_overflow_is_typed() -> TestResult {
    let (world, mut queue, id, owner) = fixture()?;
    queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    queue.activate(SessionGeneration::FIRST.next()?)?;
    assert_eq!(queue.pending_len(), 0);
    queue.next_sequence = u64::MAX;
    assert_eq!(
        queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false)),
        Err(SubmissionError::SequenceExhausted)
    );
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(
        SessionGeneration(u64::MAX).next(),
        Err(SubmissionError::GenerationExhausted)
    );
    Ok(())
}

#[test]
fn absent_session_and_unsupported_capability_are_explicit() -> TestResult {
    let (world, _, id, owner) = fixture()?;
    let mut queue = BattleCommandQueue::default();
    assert_eq!(
        queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false)),
        Err(SubmissionError::NotReady)
    );
    queue.activate(SessionGeneration::FIRST)?;
    assert_eq!(
        queue.submit(&world, command(id, owner, cast_ability(), false)),
        Err(SubmissionError::Capability(id)),
        "a cast that is no targeted effect-tree ability"
    );
    assert_eq!(queue.pending_len(), 0);
    Ok(())
}

#[test]
fn reused_generation_does_not_discard_captured_intent() -> TestResult {
    let (world, mut queue, id, owner) = fixture()?;
    queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    assert_eq!(
        queue.activate(SessionGeneration::FIRST),
        Err(SubmissionError::StaleGeneration)
    );
    assert_eq!(queue.pending_len(), 1);
    Ok(())
}

#[test]
fn stationary_armed_actor_can_target_hostile_but_cannot_attack_move() -> TestResult {
    let (mut world, mut queue, _, _) = fixture()?;
    let actor = world
        .entities
        .values()
        .find(|entity| {
            world
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|a| a.weapon.is_some() || a.ground_weapon.is_some())
        })
        .ok_or("armed actor absent")?;
    let (id, owner) = (actor.id, actor.owner);
    let target = world
        .entities
        .values()
        .find(|e| e.owner != owner)
        .ok_or("enemy absent")?
        .id;
    world
        .entities
        .get_mut(&id)
        .ok_or("actor absent")?
        .is_immobile = true;
    queue.submit(&world, command(id, owner, UnitOrder::attack(target), false))?;
    assert_eq!(
        queue.submit(
            &world,
            command(
                id,
                owner,
                UnitOrder::AttackMove {
                    target: SimVec2::ZERO
                },
                false
            )
        ),
        Err(SubmissionError::Capability(id))
    );
    assert_eq!(
        queue.submit(&world, command(id, owner, UnitOrder::attack(id), false)),
        Err(SubmissionError::InvalidTarget(id))
    );
    world.entities.get_mut(&target).ok_or("target absent")?.hp = SimScalar::ZERO;
    let mut input = bundle();
    let mut sequences = Vec::new();
    let mut rejected = Vec::new();
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(input.commands, []);
    assert_eq!(rejected[0].reason, SubmissionError::InvalidTarget(target));
    Ok(())
}

#[test]
fn prepared_sequence_indices_match_real_game_outcomes() -> TestResult {
    use sim_types::RenderSnapshot;
    let (mut world, mut queue, mover, owner) = fixture()?;
    let producer = world
        .entities
        .values()
        .find(|entity| {
            entity.owner == owner
                && world
                    .catalog
                    .get(entity.unit_type_id)
                    .is_ok_and(|a| !a.trains_units.is_empty())
        })
        .ok_or("authored producer absent")?
        .id;
    // Invalid product admission is deliberately left to the authoritative phase.
    let production = queue.submit(
        &world,
        command(
            producer,
            owner,
            UnitOrder::Produce {
                unit_type: UnitTypeId(u16::MAX),
            },
            true,
        ),
    )?;
    // A task order with nowhere to carry its load is still answered by the simulation.
    world.entities.get_mut(&mover).ok_or("mover absent")?.cargo = Some(sim_core::entity::Cargo {
        kind: sim_types::ResourceKind::Mineral,
        amount: 5,
    });
    let game = queue.submit(&world, command(mover, owner, UnitOrder::ReturnCargo, false))?;
    let movement = queue.submit(
        &world,
        command(
            mover,
            owner,
            UnitOrder::Move {
                target: SimVec2::new(SimScalar::from_i32(4), SimScalar::from_i32(4)),
            },
            false,
        ),
    )?;
    let mut input = bundle();
    let mut sequences = Vec::new();
    let mut rejected = Vec::new();
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    let mut snapshot = RenderSnapshot::default();
    world.step_into(&input, &mut snapshot);
    let production_outcomes: Vec<_> = world.drain_production_outcomes().collect();
    let movement_outcomes: Vec<_> = world.drain_movement_outcomes().collect();
    let game_outcomes: Vec<_> = world.drain_game_command_outcomes().collect();
    assert_eq!(production_outcomes.len(), 1);
    assert_eq!(movement_outcomes.len(), 1);
    assert_eq!(game_outcomes.len(), 1);
    assert_eq!(sequences[game_outcomes[0].command_index], game);
    assert_eq!(game_outcomes[0].tick, Tick(1));
    assert_eq!(sequences[production_outcomes[0].command_index], production);
    assert_eq!(sequences[movement_outcomes[0].command_index], movement);
    assert_eq!(production_outcomes[0].tick, Tick(1));
    assert_eq!(movement_outcomes[0].tick, Tick(1));
    assert!(production_outcomes[0].result.is_err());
    assert_eq!(world.diagnostics(), []);
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    world.step_into(&input, &mut snapshot);
    assert_eq!(world.production_outcomes(), []);
    assert_eq!(world.movement_outcomes(), []);
    assert_eq!(world.game_command_outcomes(), []);
    assert_eq!(production_outcomes.len(), 1);
    assert_eq!(movement_outcomes.len(), 1);
    Ok(())
}

#[test]
fn fatal_catalog_revalidation_preserves_pending_batch_and_existing_output() -> TestResult {
    let (mut world, mut queue, id, owner) = fixture()?;
    queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    world
        .entities
        .get_mut(&id)
        .ok_or("actor absent")?
        .unit_type_id = UnitTypeId(u16::MAX);
    let mut input = bundle();
    input.tick = Tick(17);
    input
        .commands
        .push(command(id, owner, UnitOrder::Stop, false));
    let before = input.clone();
    let mut sequences = vec![SubmissionSequence(99)];
    let mut rejected = Vec::new();
    assert_eq!(
        queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected),
        Err(SubmissionError::MissingArchetype(id))
    );
    assert_eq!(input, before);
    assert_eq!(sequences, [SubmissionSequence(99)]);
    assert_eq!(rejected, []);
    assert_eq!(queue.pending_len(), 1);
    Ok(())
}

#[test]
fn target_availability_matches_simulation_at_submission_and_execution() -> TestResult {
    use sim_map::VisionSource;
    let (mut world, mut queue, id, owner) = fixture()?;
    let target = world
        .entities
        .values()
        .find(|e| e.owner != owner)
        .ok_or("hostile target absent")?
        .id;
    let target_pos = world.entities.get(&target).ok_or("target absent")?.position;
    let order = UnitOrder::attack(target);
    world
        .entities
        .get_mut(&target)
        .ok_or("target absent")?
        .is_invulnerable = true;
    assert!(!world.is_entity_available_for_combat(owner, target));
    assert_eq!(
        queue.submit(&world, command(id, owner, order.clone(), false)),
        Err(SubmissionError::InvalidTarget(target))
    );
    world
        .entities
        .get_mut(&target)
        .ok_or("target absent")?
        .is_invulnerable = false;
    world
        .entities
        .get_mut(&target)
        .ok_or("target absent")?
        .is_cloaked = true;
    let owner_slot = owner.slot().ok_or("owner has no player slot")?;
    for detector in [false, true] {
        let vision = &mut world.vision_maps[owner_slot];
        vision.begin_tick_update();
        vision.apply_vision_source(
            &world.map,
            &VisionSource {
                pos: target_pos,
                mover_type: MoverType::Air,
                sight_radius: SimScalar::from_i32(3),
                is_detector: detector,
            },
        );
        vision.end_tick_update();
        assert_eq!(
            world.is_entity_available_for_combat(owner, target),
            detector
        );
        let submitted = queue.submit(&world, command(id, owner, order.clone(), false));
        if detector {
            assert!(submitted.is_ok());
        } else {
            assert_eq!(submitted, Err(SubmissionError::InvalidTarget(target)));
        }
    }
    world.vision_maps[owner_slot].begin_tick_update();
    world.vision_maps[owner_slot].end_tick_update();
    let mut input = bundle();
    let mut sequences = Vec::new();
    let mut rejected = Vec::new();
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(input.commands, []);
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].reason, SubmissionError::InvalidTarget(target));
    world
        .entities
        .get_mut(&target)
        .ok_or("target absent")?
        .is_cloaked = false;
    assert!(world.is_entity_available_for_combat(owner, target));
    queue.submit(&world, command(id, owner, order, false))?;
    Ok(())
}

#[test]
fn return_cargo_needs_cargo() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let laden = soldier(&mut world, 0)?;
    let empty = soldier(&mut world, 1)?;
    world.entities.get_mut(&laden).ok_or("laden absent")?.cargo = Some(sim_core::entity::Cargo {
        kind: sim_types::ResourceKind::Mineral,
        amount: 5,
    });
    assert_only_capable_receives(&world, &mut queue, laden, empty, &UnitOrder::ReturnCargo)
}

#[test]
fn harvest_needs_a_harvester() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let worker = unit_where(&mut world, 0, |a| a.capabilities.harvester.is_some())?;
    let other = soldier(&mut world, 1)?;
    let order = UnitOrder::Harvest {
        target: ResourceNodeId(0),
    };
    assert_only_capable_receives(&world, &mut queue, worker, other, &order)
}

#[test]
fn roster_orders_reach_only_capable_units() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let worker = unit_where(&mut world, 0, |a| a.capabilities.harvester.is_some())?;
    soldier(&mut world, 1)?;
    let order = UnitOrder::Harvest {
        target: ResourceNodeId(0),
    };
    queue.submit_roster_order(&world, ME, order, false)?;
    let (_, sent) = queue.pending_commands().last().ok_or("nothing queued")?;
    assert_eq!(sent.entities, [worker]);
    Ok(())
}

#[test]
fn build_goes_only_to_units_that_can_raise_it() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let worker = builder(&mut world, 0)?;
    let other = soldier(&mut world, 1)?;
    let structure_type = buildable(&world, worker)?;
    let order = UnitOrder::Build {
        structure_type,
        cell_x: 20,
        cell_y: 20,
    };
    assert_only_capable_receives(&world, &mut queue, worker, other, &order)?;

    let own_faction = world.catalog.get(structure_type)?.faction;
    let foreign = type_where(&world, |a| is_dome(a) && a.faction != own_faction)?;
    let mobile = type_where(&world, |a| !is_dome(a) && a.faction == own_faction)?;
    for structure_type in [foreign, mobile] {
        let order = UnitOrder::Build {
            structure_type,
            cell_x: 20,
            cell_y: 20,
        };
        assert_eq!(
            queue.submit(&world, command(worker, ME, order, false)),
            Err(SubmissionError::Capability(worker))
        );
    }
    Ok(())
}

#[test]
fn build_of_an_unknown_structure_is_a_typed_rejection() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let worker = builder(&mut world, 0)?;
    let unknown = UnitTypeId(u16::MAX);
    let order = UnitOrder::Build {
        structure_type: unknown,
        cell_x: 20,
        cell_y: 20,
    };
    let expected = Err(SubmissionError::InvalidStructure(unknown));
    assert_eq!(
        queue.submit(&world, command(worker, ME, order.clone(), false)),
        expected
    );
    assert_eq!(
        queue.submit_selection(&world, ME, &[worker], order, false),
        expected
    );
    assert!(!SubmissionError::InvalidStructure(unknown).is_fatal());
    Ok(())
}

#[test]
fn construct_goes_only_to_units_that_can_raise_the_foundation() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (worker, foundation) = lay_foundation(&mut world, &mut queue)?;
    let other = soldier(&mut world, 3)?;
    let order = UnitOrder::Construct { foundation };
    assert_only_capable_receives(&world, &mut queue, worker, other, &order)
}

#[test]
fn construct_refuses_a_foundation_that_is_foreign_or_dead() -> TestResult {
    for dead in [false, true] {
        let (mut world, mut queue) = role_fixture()?;
        let (worker, foundation) = lay_foundation(&mut world, &mut queue)?;
        let target = world
            .entities
            .get_mut(&foundation)
            .ok_or("foundation absent")?;
        if dead {
            target.hp = SimScalar::ZERO;
        } else {
            target.owner = THEM;
        }
        assert_eq!(
            queue.submit(
                &world,
                command(worker, ME, UnitOrder::Construct { foundation }, false)
            ),
            Err(SubmissionError::InvalidTarget(foundation))
        );
    }
    Ok(())
}

#[test]
fn research_and_production_are_left_to_the_simulation() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let unit = soldier(&mut world, 0)?;
    for order in [
        UnitOrder::Research { upgrade: u32::MAX },
        UnitOrder::Produce {
            unit_type: UnitTypeId(u16::MAX),
        },
    ] {
        queue.submit(&world, command(unit, ME, order, false))?;
    }
    run_tick(&mut world, &mut queue)?;
    assert_eq!(world.production_outcomes().len(), 2);
    assert!(world
        .production_outcomes()
        .iter()
        .all(|o| o.result.is_err()));
    Ok(())
}

#[test]
fn cancel_production_needs_a_queued_job() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let hall = type_where(&world, is_dome)?;
    let busy = world.spawn_structure(hall, ME, 10, 20)?;
    let quiet = world.spawn_structure(hall, ME, 30, 20)?;
    world
        .entities
        .get_mut(&busy)
        .ok_or("structure absent")?
        .production_queue
        .push_back(ProductionJob {
            item: ProductionItem::Unit(UnitTypeId(0)),
            remaining_ticks: 10,
            total_ticks: 10,
            reserved_supply: 0,
            paid_minerals: 0,
            paid_gas: 0,
        });
    let order = UnitOrder::CancelProduction { job_index: 0 };
    assert_only_capable_receives(&world, &mut queue, busy, quiet, &order)
}

#[test]
fn load_needs_a_mobile_ground_unit_other_than_the_carrier() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let carrier = unit_where(&mut world, 0, |a| a.capabilities.transport_capacity > 0)?;
    let passenger = soldier(&mut world, 1)?;
    let flyer = unit_where(&mut world, 2, |a| a.mover_type == MoverType::Air)?;
    let order = UnitOrder::Load { carrier };
    assert_only_capable_receives(&world, &mut queue, passenger, flyer, &order)?;

    let hall = type_where(&world, is_dome)?;
    let structure = world.spawn_structure(hall, ME, 30, 20)?;
    assert_eq!(
        queue.submit(&world, command(structure, ME, order.clone(), false)),
        Err(SubmissionError::Capability(structure))
    );

    // Every shipped carrier flies, so the rule is exercised on a ground copy of one.
    let entity = world.entities.get(&carrier).ok_or("carrier absent")?;
    let mut ground = world.catalog.get(entity.unit_type_id)?.clone();
    ground.mover_type = MoverType::Ground;
    assert!(!capable(&world, entity, &ground, &order, None));
    assert!(capable(
        &world,
        entity,
        &ground,
        &UnitOrder::Load { carrier: passenger },
        None
    ));
    Ok(())
}

#[test]
fn unload_needs_a_carrier() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let carrier = unit_where(&mut world, 0, |a| a.capabilities.transport_capacity > 0)?;
    let other = soldier(&mut world, 1)?;
    assert_only_capable_receives(&world, &mut queue, carrier, other, &UnitOrder::UnloadAll)
}

#[test]
fn cancel_construction_reaches_an_unfinished_foundation() -> TestResult {
    use sim_core::economy::GameCommandReceipt;
    let (mut world, mut queue) = role_fixture()?;
    let (worker, foundation) = lay_foundation(&mut world, &mut queue)?;
    assert_only_capable_receives(
        &world,
        &mut queue,
        foundation,
        worker,
        &UnitOrder::CancelConstruction,
    )?;
    run_tick(&mut world, &mut queue)?;
    assert!(matches!(
        world.game_command_outcomes()[0].result,
        Ok(GameCommandReceipt::Refunded { .. })
    ));
    Ok(())
}

#[test]
fn every_other_order_refuses_a_foundation() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (worker, foundation) = lay_foundation(&mut world, &mut queue)?;
    let structure_type = world
        .entities
        .get(&foundation)
        .ok_or("foundation absent")?
        .unit_type_id;
    let enemy_type = type_where(&world, |a| !is_dome(a))?;
    let enemy = world.spawn_unit(
        enemy_type,
        THEM,
        SimVec2::new(SimScalar::from_i32(9), SimScalar::from_i32(9)),
    )?;
    let target = SimVec2::new(SimScalar::from_i32(4), SimScalar::from_i32(4));
    for order in [
        UnitOrder::Idle,
        UnitOrder::Move { target },
        UnitOrder::AttackMove { target },
        UnitOrder::attack(enemy),
        UnitOrder::HoldPosition,
        UnitOrder::Stop,
        UnitOrder::UseStim,
        UnitOrder::Produce {
            unit_type: structure_type,
        },
        UnitOrder::Harvest {
            target: ResourceNodeId(0),
        },
        UnitOrder::ReturnCargo,
        UnitOrder::Build {
            structure_type,
            cell_x: 30,
            cell_y: 30,
        },
        UnitOrder::Construct { foundation },
        UnitOrder::Research { upgrade: 0 },
        UnitOrder::CancelProduction { job_index: 0 },
        UnitOrder::Load { carrier: worker },
        UnitOrder::UnloadAll,
    ] {
        assert_eq!(
            queue.submit(&world, command(foundation, ME, order.clone(), false)),
            Err(SubmissionError::UnavailableRecipient(foundation)),
            "{order:?}"
        );
    }
    Ok(())
}

#[test]
fn a_foundation_finished_while_its_cancel_waits_is_a_capability_rejection() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (_, foundation) = lay_foundation(&mut world, &mut queue)?;
    let sequence = queue.submit(
        &world,
        command(foundation, ME, UnitOrder::CancelConstruction, false),
    )?;
    let finished = world
        .entities
        .get_mut(&foundation)
        .ok_or("foundation absent")?;
    finished.construction = None;
    finished.is_completed = true;
    let (mut input, mut sequences, mut rejected) = (bundle(), Vec::new(), Vec::new());
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(input.commands, []);
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].sequence, sequence);
    assert_eq!(rejected[0].reason, SubmissionError::Capability(foundation));
    Ok(())
}

#[test]
fn commands_after_the_match_are_refused_at_submission_and_at_preparation() -> TestResult {
    type Close = fn(&mut SimWorld);
    let closes: [(Close, SubmissionError); 2] = [
        (
            |world| {
                world.match_state.result = MatchResult::Draw { tick: Tick(1) };
            },
            SubmissionError::MatchOver,
        ),
        (
            |world| world.match_state.defeated = 1,
            SubmissionError::PlayerDefeated(ME),
        ),
    ];
    for (close, expected) in closes {
        let (mut world, mut queue) = role_fixture()?;
        let unit = soldier(&mut world, 0)?;
        let sequence = queue.submit(&world, command(unit, ME, UnitOrder::Stop, false))?;
        close(&mut world);
        assert!(!expected.is_fatal());

        // Prepared first: the pending order would otherwise be executed against a closed match.
        let (mut input, mut sequences, mut rejected) = (bundle(), Vec::new(), Vec::new());
        queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
        assert_eq!(input.commands, []);
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].sequence, sequence);
        assert_eq!(rejected[0].reason, expected);

        // A closed match outranks every other answer, including an unknown structure.
        let unknown = UnitOrder::Build {
            structure_type: UnitTypeId(u16::MAX),
            cell_x: 1,
            cell_y: 1,
        };
        assert_eq!(
            queue.submit(&world, command(unit, ME, UnitOrder::Stop, false)),
            Err(expected.clone())
        );
        assert_eq!(
            queue.submit_selection(&world, ME, &[unit], unknown, false),
            Err(expected.clone())
        );
        assert_eq!(
            queue.submit_roster_order(&world, ME, UnitOrder::Stop, false),
            Err(expected.clone())
        );
        assert_eq!(queue.pending_len(), 0);
    }
    Ok(())
}

#[test]
fn a_defeat_closes_the_match_only_for_that_player() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let enemy_type = type_where(&world, |a| !is_dome(a))?;
    let enemy = world.spawn_unit(enemy_type, THEM, SimVec2::ZERO)?;
    world.match_state.defeated = 1;
    queue.submit(&world, command(enemy, THEM, UnitOrder::Stop, false))?;
    Ok(())
}

#[test]
fn arming_is_any_weapon_slot() -> TestResult {
    let (world, _) = role_fixture()?;
    let armed = world
        .catalog
        .archetypes()
        .find(|a| a.weapon.is_some())
        .ok_or("no armed archetype")?;
    assert!(is_armed(armed));
    let mut unarmed = armed.clone();
    unarmed.weapon = None;
    unarmed.ground_weapon = None;
    unarmed.air_weapon = None;
    assert!(!is_armed(&unarmed));
    unarmed.air_weapon = armed.weapon.clone();
    assert!(is_armed(&unarmed));
    Ok(())
}

/// G-8: in a mixed selection, only units whose overclock is unlocked receive it. One overclock
/// type needs research and another does not, so the order is admitted, and before this rule the
/// locked units received it and silently did nothing. The shipped catalog need not have an
/// overclock that no research unlocks, so the test gives one to an armed type that no upgrade
/// names.
#[test]
fn stim_goes_only_to_units_whose_overclock_is_unlocked() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let locks = |world: &SimWorld, a: &UnitArchetype| {
        world
            .upgrade_catalog
            .unlocks_ability(&a.slug, OVERCLOCK_ABILITY_SLUG)
    };
    let locked_type = type_where(&world, |a| {
        !is_dome(a) && a.overclock.is_some() && locks(&world, a)
    })?;
    let free_type = type_where(&world, |a| {
        !is_dome(a) && a.weapon.is_some() && a.overclock.is_none() && !locks(&world, a)
    })?;
    world.catalog.get_mut(free_type)?.overclock = world.catalog.get(locked_type)?.overclock;
    let locked = unit_where(&mut world, 0, |a| a.unit_type_id == locked_type)?;
    let free = unit_where(&mut world, 1, |a| a.unit_type_id == free_type)?;

    queue.submit_selection(&world, ME, &[locked, free], UnitOrder::UseStim, false)?;
    let (_, sent) = queue.pending_commands().last().ok_or("nothing queued")?;
    assert_eq!(sent.entities, [free], "the locked unit cannot stim yet");
    assert_eq!(
        queue.submit(&world, command(locked, ME, UnitOrder::UseStim, false)),
        Err(SubmissionError::Capability(locked))
    );
    assert_eq!(
        queue.submit_selection(&world, ME, &[locked], UnitOrder::UseStim, false),
        Err(SubmissionError::EmptySelection)
    );

    let locked_slug = world.catalog.get(locked_type)?.slug.clone();
    let unlock = world
        .upgrade_catalog
        .upgrades()
        .find(|upgrade| {
            upgrade.effects.iter().any(|effect| {
                matches!(effect, sim_types::UpgradeEffect::UnlockAbility { target_slug, ability_slug }
                    if *target_slug == locked_slug && ability_slug == OVERCLOCK_ABILITY_SLUG)
            })
        })
        .ok_or("the catalog names the unlocking upgrade")?
        .slug
        .clone();
    world.research_upgrade(ME, &unlock)?;
    queue.submit_selection(&world, ME, &[locked, free], UnitOrder::UseStim, false)?;
    let (_, sent) = queue.pending_commands().last().ok_or("nothing queued")?;
    assert_eq!(sent.entities, [locked, free]);
    Ok(())
}

/// Q0053 / B-33: the client offers a stim only when the simulation would admit it, so a unit
/// whose life does not exceed the life cost is not sent `UseStim`.
#[test]
fn stim_is_not_offered_without_more_life_than_its_cost() -> TestResult {
    let (mut world, _queue) = role_fixture()?;
    let stim_type = type_where(&world, |a| !is_dome(a) && a.overclock.is_some())?;
    let unit = unit_where(&mut world, 0, |a| a.unit_type_id == stim_type)?;
    let archetype = world.catalog.get(stim_type)?.clone();
    let cost = archetype.overclock.ok_or("overclock")?.hp_cost;
    let unlock_all = |world: &mut SimWorld| -> TestResult {
        let unlocks: Vec<String> = world
            .upgrade_catalog
            .upgrades()
            .filter(|upgrade| {
                upgrade.effects.iter().any(|effect| {
                    matches!(effect, sim_types::UpgradeEffect::UnlockAbility { ability_slug, .. }
                        if ability_slug == OVERCLOCK_ABILITY_SLUG)
                })
            })
            .map(|upgrade| upgrade.slug.clone())
            .collect();
        for slug in unlocks {
            world.research_upgrade(ME, &slug)?;
        }
        Ok(())
    };
    unlock_all(&mut world)?;
    for (life, offered) in [
        (cost - SimScalar::ONE, false),
        (cost, false),
        (cost + SimScalar::ONE, true),
    ] {
        world.entities.get_mut(&unit).ok_or("unit absent")?.hp = life;
        let entity = world.entities.get(&unit).ok_or("unit absent")?;
        assert_eq!(
            can_overclock(&world, entity, &archetype),
            offered,
            "stim offered at {life:?} life"
        );
    }
    Ok(())
}

/// The inspector's roster `Overclock` goes through the same rule.
#[test]
fn roster_stim_skips_units_whose_overclock_is_locked() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let locked_type = type_where(&world, |a| {
        !is_dome(a)
            && a.overclock.is_some()
            && world
                .upgrade_catalog
                .unlocks_ability(&a.slug, OVERCLOCK_ABILITY_SLUG)
    })?;
    let locked = unit_where(&mut world, 0, |a| a.unit_type_id == locked_type)?;
    let entity = world.entities.get(&locked).ok_or("unit absent")?;
    assert!(!can_overclock(
        &world,
        entity,
        world.catalog.get(locked_type)?
    ));
    assert_eq!(
        queue.submit_roster_order(&world, ME, UnitOrder::UseStim, false),
        Err(SubmissionError::EmptySelection)
    );
    assert_eq!(queue.pending_len(), 0);
    Ok(())
}

// --- Mode switches (plan `2026-11-09-client-mode-commands` WP3) --------------------------------

/// Whether `archetype` has a mode ability; roles are found by capability, never by name.
fn has_mode_ability(archetype: &UnitArchetype) -> bool {
    archetype
        .abilities
        .iter()
        .any(|ability| mode_of_ability(archetype, ability).is_some())
}

/// A mobile unit of `ME` that is armed by default and can switch a mode, and that mode's name.
fn moder(
    world: &mut SimWorld,
    slot: i32,
) -> Result<(EntityId, String), Box<dyn std::error::Error>> {
    let unit = unit_where(world, slot, |a| {
        has_mode_ability(a) && is_armed(a) && a.max_speed > SimScalar::ZERO
    })?;
    let archetype = world.catalog.get(world.entities[&unit].unit_type_id)?;
    let mode = archetype
        .abilities
        .iter()
        .find_map(|ability| mode_of_ability(archetype, ability))
        .ok_or("a mode ability")?
        .to_owned();
    Ok((unit, mode))
}

/// Steps `world` without commands until no unit channels, at most 200 ticks.
fn settle(world: &mut SimWorld) -> TestResult {
    for _ in 0..200 {
        if world
            .entities
            .values()
            .all(|entity| entity.channeling_remaining_ticks == 0)
        {
            return Ok(());
        }
        let mut input = bundle();
        input.tick = Tick(world.current_tick);
        world.step(&input);
    }
    Err("a channel did not end".into())
}

/// The ability index and recipients of every pending command.
fn pending_casts(queue: &BattleCommandQueue) -> Vec<(u8, Vec<EntityId>, bool)> {
    queue
        .pending_commands()
        .filter_map(|(_, command)| match command.order {
            UnitOrder::CastAbility { ability_index, .. } => {
                Some((ability_index, command.entities.clone(), command.queue))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_mode_switch_goes_each_way_by_the_settled_mode() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (tank, mode) = moder(&mut world, 0)?;
    let index = mode_ability_index(
        world.catalog.get(world.entities[&tank].unit_type_id)?,
        &mode,
    )
    .ok_or("mode ability")?;
    let selection = [tank];
    assert_eq!(
        queue.submit_mode_switch(&world, ME, &selection, &mode, ModeDirection::Leave, false),
        Err(SubmissionError::EmptySelection),
        "a unit in its default stance cannot leave the mode"
    );
    queue.submit_mode_switch(&world, ME, &selection, &mode, ModeDirection::Enter, true)?;
    assert_eq!(
        pending_casts(&queue),
        [(index, vec![tank], true)],
        "Shift queues"
    );
    run_tick(&mut world, &mut queue)?;
    assert!(world.entities[&tank].channeling_remaining_ticks > 0);
    assert_eq!(
        queue.submit_mode_switch(&world, ME, &selection, &mode, ModeDirection::Enter, false),
        Err(SubmissionError::EmptySelection),
        "a channelling unit is no recipient"
    );
    settle(&mut world)?;
    assert_eq!(
        queue.submit_mode_switch(&world, ME, &selection, &mode, ModeDirection::Enter, false),
        Err(SubmissionError::EmptySelection),
        "a unit in the mode cannot enter it"
    );
    queue.submit_mode_switch(&world, ME, &selection, &mode, ModeDirection::Leave, false)?;
    run_tick(&mut world, &mut queue)?;
    settle(&mut world)?;
    assert_eq!(world.entities[&tank].active_mode, None, "left the mode");
    Ok(())
}

#[test]
fn a_mixed_selection_sends_each_direction_only_to_its_units() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (sieged, mode) = moder(&mut world, 0)?;
    let (fresh, _) = moder(&mut world, 1)?;
    let soldier = soldier(&mut world, 2)?;
    let index = mode_ability_index(
        world.catalog.get(world.entities[&sieged].unit_type_id)?,
        &mode,
    )
    .ok_or("mode ability")?;
    assert!(world.execute_ability(sieged, index, None, None));
    settle(&mut world)?;
    let selection = [sieged, fresh, soldier];
    queue.submit_mode_switch(&world, ME, &selection, &mode, ModeDirection::Enter, false)?;
    queue.submit_mode_switch(&world, ME, &selection, &mode, ModeDirection::Leave, false)?;
    assert_eq!(
        pending_casts(&queue),
        [(index, vec![fresh], false), (index, vec![sieged], false)]
    );
    Ok(())
}

#[test]
fn recipients_are_grouped_by_their_own_ability_index() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (tank, mode) = moder(&mut world, 0)?;
    let tank_arch = world
        .catalog
        .get(world.entities[&tank].unit_type_id)?
        .clone();
    let index = mode_ability_index(&tank_arch, &mode).ok_or("mode ability")?;
    // A second archetype holding the same mode behind another ability, at index `index + 1`.
    let other = soldier(&mut world, 1)?;
    let other_type = world.entities[&other].unit_type_id;
    {
        let other_arch = world.catalog.get_mut(other_type)?;
        let mut before = tank_arch.abilities[usize::from(index)].clone();
        before.target_kind = AbilityTargetKind::InstantSelf;
        before.leave_name = None;
        other_arch.abilities = tank_arch.abilities[..=usize::from(index)].to_vec();
        other_arch.abilities.insert(0, before);
        other_arch.modes.clone_from(&tank_arch.modes);
    }
    let selection = [other, tank];
    queue.submit_mode_switch(&world, ME, &selection, &mode, ModeDirection::Enter, false)?;
    assert_eq!(
        pending_casts(&queue),
        [(index, vec![tank], false), (index + 1, vec![other], false)],
        "one command per index, in ascending order"
    );
    run_tick(&mut world, &mut queue)?;
    for unit in [tank, other] {
        assert!(
            world.entities[&unit].channeling_remaining_ticks > 0,
            "{unit:?}"
        );
    }
    Ok(())
}

#[test]
fn a_locked_mode_ability_has_no_recipient() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (tank, mode) = moder(&mut world, 0)?;
    let arch = world.catalog.get(world.entities[&tank].unit_type_id)?;
    let index = mode_ability_index(arch, &mode).ok_or("mode ability")?;
    let ability = arch.abilities[usize::from(index)].name.clone();
    let faction = world
        .catalog
        .factions()
        .faction(arch.faction)
        .ok_or("faction")?
        .slug
        .clone();
    let researched_at = type_where(&world, |a| is_dome(a) && a.faction == arch.faction)?;
    let researched_at = world.catalog.get(researched_at)?.slug.clone();
    let upgrades = format!(
        r#"(upgrades: [(
            upgrade_id: 1, slug: "learn_mode", name: "learn_mode", display_name: "Learn Mode",
            faction: "{faction}", level: 1, mineral_cost: 0, gas_cost: 0,
            research_ticks: 1, researched_at: "{researched_at}",
            effects: [UnlockAbility(target_slug: "{}", ability_slug: "{ability}")],
        )])"#,
        arch.slug
    );
    world.upgrade_catalog =
        sim_core::UpgradeCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &upgrades)?;
    assert_eq!(
        queue.submit_mode_switch(&world, ME, &[tank], &mode, ModeDirection::Enter, false),
        Err(SubmissionError::EmptySelection)
    );
    world.research_upgrade(ME, "learn_mode")?;
    queue.submit_mode_switch(&world, ME, &[tank], &mode, ModeDirection::Enter, false)?;
    assert_eq!(queue.pending_len(), 1);
    Ok(())
}

#[test]
fn a_unit_that_switched_meanwhile_is_rejected_at_its_tick() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (tank, mode) = moder(&mut world, 0)?;
    let index = mode_ability_index(
        world.catalog.get(world.entities[&tank].unit_type_id)?,
        &mode,
    )
    .ok_or("mode ability")?;
    queue.submit_mode_switch(&world, ME, &[tank], &mode, ModeDirection::Enter, false)?;
    // The unit enters the mode before the tick takes the press.
    assert!(world.execute_ability(tank, index, None, None));
    settle(&mut world)?;
    let (mut input, mut sequences, mut rejected) = (bundle(), Vec::new(), Vec::new());
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(input.commands, []);
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].reason, SubmissionError::Capability(tank));
    world.step(&input);
    assert_eq!(
        world.entities[&tank].active_mode.as_deref(),
        Some(mode.as_str()),
        "not toggled back"
    );
    Ok(())
}

#[test]
fn the_generic_paths_still_refuse_a_mode_switch() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (tank, _) = moder(&mut world, 0)?;
    assert_eq!(
        queue.submit_selection(&world, ME, &[tank], cast_ability(), false),
        Err(SubmissionError::EmptySelection)
    );
    assert_eq!(
        queue.submit(&world, command(tank, ME, cast_ability(), false)),
        Err(SubmissionError::Capability(tank))
    );
    Ok(())
}

/// A scratch point cast of `zone`, which hits every unit within 1 of the point.
fn install_scratch_cast(
    world: &mut SimWorld,
) -> Result<(UnitTypeId, u8), Box<dyn std::error::Error>> {
    // Appended to the shipped tables, which the shipped casters name.
    let spells = sim_fixtures::spells_with(
        "(statuses: [])",
        "(effects: [(name: \"zone\", node: Search((radius: 1.0, child: Damage((amount: 1.0)))))])",
    )?;
    world.catalog.set_spell_catalog(spells)?;
    let kind = type_where(world, |a| {
        !is_dome(a)
            && a.max_speed > SimScalar::ZERO
            && a.abilities.len() <= 3
            && !a.capabilities.can_construct
    })?;
    let archetype = world.catalog.get_mut(kind)?;
    archetype.max_energy = SimScalar::from_i32(200);
    archetype.energy_regen_per_tick = SimScalar::ZERO;
    archetype.abilities.push(AbilitySpec {
        name: "Scratch zone".to_owned(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::from_i32(50),
        cooldown_ticks: 10,
        cast_range: SimScalar::from_i32(5),
        effect: AbilityEffectSpec::Effect("zone".to_owned()),
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    });
    Ok((kind, u8::try_from(archetype.abilities.len() - 1)?))
}

fn scratch_caster(
    world: &mut SimWorld,
    kind: UnitTypeId,
    slot: i32,
    energy: i32,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let at = SimVec2::new(SimScalar::from_i32(8 + 3 * slot), SimScalar::from_i32(10));
    let id = world.spawn_unit(kind, ME, at)?;
    world.entities.get_mut(&id).ok_or("spawned")?.energy = SimScalar::from_i32(energy);
    Ok(id)
}

#[test]
fn a_cast_is_sent_to_the_capable_members_and_revalidated_member_by_member() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (kind, index) = install_scratch_cast(&mut world)?;
    let rich = scratch_caster(&mut world, kind, 0, 100)?;
    let other = scratch_caster(&mut world, kind, 1, 100)?;
    let poor = scratch_caster(&mut world, kind, 2, 10)?;
    let bystander = builder(&mut world, 3)?;
    let key = CastKey {
        holder: kind,
        ability_index: index,
    };
    let at = Some(SimVec2::new(
        SimScalar::from_i32(20),
        SimScalar::from_i32(20),
    ));
    let cast = CastOrder {
        key,
        target_pos: at,
        target_entity: None,
        queued: false,
    };
    assert!(any_caster(&world, ME, &[bystander, poor], key));
    assert!(!any_caster(&world, ME, &[bystander], key));
    queue.submit_cast(&world, ME, &[rich, other, poor, bystander], cast)?;
    let sent: Vec<_> = queue.pending_commands().map(|(_, c)| c.clone()).collect();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].entities,
        [rich, other],
        "the poor caster and the bystander are left out"
    );
    assert_eq!(
        sent[0].order,
        UnitOrder::CastAbility {
            ability_index: index,
            target_pos: at,
            target_entity: None,
        }
    );
    // One caster spends its energy before the tick takes the command: the other still casts.
    world.entities.get_mut(&rich).ok_or("rich")?.energy = SimScalar::ZERO;
    let (mut input, mut sequences, mut rejected) = (bundle(), Vec::new(), Vec::new());
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(rejected, []);
    assert_eq!(input.commands.len(), 1);
    assert_eq!(input.commands[0].entities, [other]);

    assert_eq!(
        queue.submit_cast(&world, ME, &[poor], cast),
        Err(SubmissionError::CastRefused),
        "a holder that cannot cast now"
    );
    assert_eq!(
        queue.submit_cast(&world, ME, &[bystander], cast),
        Err(SubmissionError::EmptySelection),
        "nobody holds the cast"
    );
    Ok(())
}

#[test]
fn a_unit_leaving_its_anchoring_mode_may_be_told_to_move() -> TestResult {
    let (mut world, mut queue) = role_fixture()?;
    let (tank, mode) = moder(&mut world, 0)?;
    let index = mode_ability_index(
        world.catalog.get(world.entities[&tank].unit_type_id)?,
        &mode,
    )
    .ok_or("mode ability")?;
    let target = SimVec2::new(SimScalar::from_i32(30), SimScalar::from_i32(30));
    let walk = UnitOrder::Move { target };
    assert!(world.execute_ability(tank, index, None, None));
    assert_eq!(
        queue.submit_selection(&world, ME, &[tank], walk.clone(), false),
        Err(SubmissionError::EmptySelection),
        "entering an anchoring mode: no move"
    );
    settle(&mut world)?;
    assert_eq!(
        queue.submit_selection(&world, ME, &[tank], walk.clone(), false),
        Err(SubmissionError::EmptySelection),
        "anchored: no move"
    );
    assert!(world.execute_ability(tank, index, None, None));
    assert!(
        world.entities[&tank].is_immobile,
        "anchored while it leaves"
    );
    queue.submit_selection(&world, ME, &[tank], walk, false)?;
    run_tick(&mut world, &mut queue)?;
    settle(&mut world)?;
    for _ in 0..25 {
        let mut input = bundle();
        input.tick = Tick(world.current_tick);
        world.step(&input);
    }
    assert!(
        world.entities[&tank].velocity != SimVec2::ZERO
            || world.entities[&tank].order_queue.is_empty(),
        "it walks once the channel ends"
    );
    Ok(())
}
