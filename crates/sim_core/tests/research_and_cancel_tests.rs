//! Research orders and queue cancellation over the shared production transaction.
//!
//! # Responsibility
//! Checks cost, time, prerequisites, idempotency, tiers and refund for research through
//! `UnitOrder::Research`, and the refund of `UnitOrder::CancelProduction` for both kinds of
//! job.
//!
//! # Invariants
//! Setup places completed structures directly, so these tests cover the queue alone; the
//! whole path from an empty base is `full_match_tests`.

use sim_core::economy::{GameCommandError, GameCommandReceipt};
use sim_core::production::{ProductionAdmissionError, ProductionItem};
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, Tick, TickBundle, UnitOrder, UnitTypeId,
};
use std::error::Error;

const ME: PlayerId = PlayerId(0);

type TestResult = Result<(), Box<dyn Error>>;

struct Fixture {
    world: SimWorld,
    lab: EntityId,
    stim: u32,
    stim_price: (u32, u32),
    stim_ticks: u32,
}

fn structure(world: &SimWorld, slug: &str) -> Result<UnitTypeId, Box<dyn Error>> {
    Ok(world.catalog.id_by_slug(slug).ok_or(slug.to_owned())?)
}

fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::new(map, 21);
    world.spawn_structure(structure(&world, "foundry_core_dome")?, ME, 2, 2)?;
    let lab = world.spawn_structure(structure(&world, "assembly_bay_dome")?, ME, 12, 2)?;
    world.spawn_structure(structure(&world, "tech_module_pod")?, ME, 18, 2)?;
    world.player_resources[0].minerals = 1000;
    world.player_resources[0].gas = 500;
    let def = world
        .upgrade_catalog
        .get_by_slug("overclock_stim")
        .ok_or("shipped upgrade")?;
    let (stim, stim_price, stim_ticks) = (
        def.upgrade_id,
        (def.mineral_cost, def.gas_cost),
        def.research_ticks,
    );
    Ok(Fixture {
        world,
        lab,
        stim,
        stim_price,
        stim_ticks,
    })
}

impl Fixture {
    fn order(&mut self, entities: Vec<EntityId>, order: UnitOrder) {
        let tick = self.world.current_tick;
        self.world.step(&TickBundle {
            tick: Tick(tick),
            commands: vec![PlayerCommand {
                player_id: ME,
                entities,
                order,
                queue: false,
            }],
            state_checksum: None,
        });
    }

    fn research(&mut self, upgrade: u32) -> Result<(), ProductionAdmissionError> {
        let lab = self.lab;
        self.order(vec![lab], UnitOrder::Research { upgrade });
        self.world.production_outcomes()[0]
            .result
            .clone()
            .map(|_| ())
    }

    fn idle(&mut self, ticks: u32) {
        for _ in 0..ticks {
            let tick = self.world.current_tick;
            self.world.step(&TickBundle {
                tick: Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            });
            assert_eq!(self.world.diagnostics(), []);
        }
    }

    fn has_stim(&self) -> bool {
        self.world.player_upgrades[0]
            .completed_upgrade_slugs
            .contains("overclock_stim")
    }
}

#[test]
fn research_costs_its_price_and_takes_its_time() -> TestResult {
    let mut f = fixture()?;
    let bank = f.world.player_resources[0];
    let supply = bank.supply_used;
    f.research(f.stim)?;
    assert_eq!(
        f.world.player_resources[0].minerals,
        bank.minerals - f.stim_price.0
    );
    assert_eq!(f.world.player_resources[0].gas, bank.gas - f.stim_price.1);
    assert_eq!(f.world.player_resources[0].supply_used, supply);
    let job = f.world.entities[&f.lab].production_queue[0];
    assert_eq!(job.item, ProductionItem::Research(f.stim));

    f.idle(f.stim_ticks - 2);
    assert!(!f.has_stim(), "research must take its authored time");
    f.idle(2);
    assert!(f.has_stim());
    assert!(f.world.entities[&f.lab].production_queue.is_empty());
    Ok(())
}

#[test]
fn research_is_idempotent() -> TestResult {
    let mut f = fixture()?;
    f.research(f.stim)?;
    assert_eq!(
        f.research(f.stim),
        Err(ProductionAdmissionError::UpgradeAlreadyQueued(f.stim))
    );
    f.idle(f.stim_ticks);
    let bank = f.world.player_resources[0];
    assert_eq!(
        f.research(f.stim),
        Err(ProductionAdmissionError::UpgradeAlreadyResearched(f.stim))
    );
    assert_eq!(f.world.player_resources[0], bank);
    Ok(())
}

#[test]
fn research_is_refused_without_its_requirements() -> TestResult {
    let mut f = fixture()?;
    assert_eq!(
        f.research(9_999),
        Err(ProductionAdmissionError::UnknownUpgrade(9_999))
    );

    f.world.player_resources[0].gas = 0;
    assert!(matches!(
        f.research(f.stim),
        Err(ProductionAdmissionError::Gas { available: 0, .. })
    ));
    f.world.player_resources[0].gas = 500;
    f.world.player_resources[0].minerals = 0;
    assert!(matches!(
        f.research(f.stim),
        Err(ProductionAdmissionError::Minerals { available: 0, .. })
    ));
    f.world.player_resources[0].minerals = 1000;

    // Wrong structure: the hall researches nothing.
    let hall = f
        .world
        .entities
        .values()
        .find(|e| e.id != f.lab)
        .map(|e| e.id)
        .ok_or("hall")?;
    let upgrade = f.stim;
    f.order(vec![hall], UnitOrder::Research { upgrade });
    assert_eq!(
        f.world.production_outcomes()[0].result,
        Err(ProductionAdmissionError::NoEligibleResearcher(upgrade))
    );

    // The prerequisite structure has to stand.
    let module = f
        .world
        .entities
        .values()
        .max_by_key(|e| e.id)
        .map(|e| e.id)
        .ok_or("module")?;
    let module_type = f.world.entities[&module].unit_type_id;
    f.world.entities.get_mut(&module).ok_or("module")?.hp = SimScalar::ZERO;
    f.idle(1);
    assert_eq!(
        f.research(f.stim),
        Err(ProductionAdmissionError::MissingPrerequisite(module_type))
    );
    Ok(())
}

#[test]
fn an_unfinished_structure_cannot_research() -> TestResult {
    let mut f = fixture()?;
    let lab = f.lab;
    f.world.entities.get_mut(&lab).ok_or("lab")?.is_completed = false;
    let upgrade = f.stim;
    assert_eq!(
        f.research(upgrade),
        Err(ProductionAdmissionError::NoEligibleResearcher(upgrade))
    );
    Ok(())
}

#[test]
fn tiers_are_researched_in_order() -> TestResult {
    let mut f = fixture()?;
    let lab = f
        .world
        .spawn_structure(structure(&f.world, "mechanics_lab_dome")?, ME, 12, 8)?;
    f.world
        .spawn_structure(structure(&f.world, "weapons_arsenal_dome")?, ME, 12, 14)?;
    f.lab = lab;
    let first = f
        .world
        .upgrade_catalog
        .get_by_slug("infantry_weapons_1")
        .ok_or("tier")?;
    let (first, ticks) = (first.upgrade_id, first.research_ticks);
    let second = f
        .world
        .upgrade_catalog
        .get_by_slug("infantry_weapons_2")
        .ok_or("tier")?
        .upgrade_id;
    assert_eq!(
        f.research(second),
        Err(ProductionAdmissionError::UpgradeTierLocked(second))
    );
    f.research(first)?;
    f.idle(ticks);
    f.research(second)?;
    Ok(())
}

#[test]
fn research_applies_no_effect_twice_with_two_labs() -> TestResult {
    let mut f = fixture()?;
    let other = f
        .world
        .spawn_structure(structure(&f.world, "assembly_bay_dome")?, ME, 24, 2)?;
    f.research(f.stim)?;
    f.lab = other;
    assert_eq!(
        f.research(f.stim),
        Err(ProductionAdmissionError::UpgradeAlreadyQueued(f.stim))
    );
    Ok(())
}

#[test]
fn cancelling_research_refunds_it_and_it_can_run_again() -> TestResult {
    let mut f = fixture()?;
    let bank = f.world.player_resources[0];
    let lab = f.lab;
    f.research(f.stim)?;
    f.idle(100);
    f.order(vec![lab], UnitOrder::CancelProduction { job_index: 0 });
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Ok(GameCommandReceipt::Refunded {
            minerals: f.stim_price.0,
            gas: f.stim_price.1,
        })
    );
    assert_eq!(f.world.player_resources[0], bank);
    assert!(f.world.entities[&lab].production_queue.is_empty());
    f.research(f.stim)?;
    assert_eq!(
        f.world.entities[&lab].production_queue[0].remaining_ticks,
        f.stim_ticks - 1,
        "a restart begins from the full time and has run one tick"
    );
    Ok(())
}

#[test]
fn cancelling_training_refunds_payment_and_supply() -> TestResult {
    let mut f = fixture()?;
    let lab = f.lab;
    let soldier = structure(&f.world, "pulse_bot")?;
    let price = f.world.catalog.get(soldier)?.clone();
    let bank = f.world.player_resources[0];
    f.order(vec![lab], UnitOrder::Produce { unit_type: soldier });
    f.order(vec![lab], UnitOrder::Produce { unit_type: soldier });
    assert_eq!(f.world.entities[&lab].production_queue.len(), 2);
    assert_eq!(
        f.world.player_resources[0].supply_used,
        bank.supply_used + 2 * price.supply_cost
    );
    f.idle(10);

    // Cancel the waiting job: the one in progress keeps its countdown.
    let running = f.world.entities[&lab].production_queue[0].remaining_ticks;
    f.order(vec![lab], UnitOrder::CancelProduction { job_index: 1 });
    assert!(f.world.game_command_outcomes()[0].result.is_ok());
    assert_eq!(f.world.entities[&lab].production_queue.len(), 1);
    assert_eq!(
        f.world.entities[&lab].production_queue[0].remaining_ticks,
        running - 1
    );

    f.order(vec![lab], UnitOrder::CancelProduction { job_index: 0 });
    assert_eq!(f.world.player_resources[0], bank);
    f.world.validate_supply_ledger()?;
    f.idle(price.build_time_ticks + 2);
    assert_eq!(
        f.world
            .entities
            .values()
            .filter(|e| e.unit_type_id == soldier)
            .count(),
        0
    );
    Ok(())
}

#[test]
fn cancel_refuses_what_it_cannot_do() -> TestResult {
    let mut f = fixture()?;
    let lab = f.lab;
    let hall = f
        .world
        .entities
        .values()
        .find(|e| e.id != lab)
        .map(|e| e.id)
        .ok_or("hall")?;
    f.order(vec![lab], UnitOrder::CancelProduction { job_index: 0 });
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::NoSuchJob {
            producer: lab,
            job_index: 0
        })
    );
    f.order(
        vec![lab, hall],
        UnitOrder::CancelProduction { job_index: 0 },
    );
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::SingleRecipientRequired)
    );
    Ok(())
}

#[test]
fn snapshots_publish_queue_progress() -> TestResult {
    let mut f = fixture()?;
    let lab = f.lab;
    f.research(f.stim)?;
    f.idle(f.stim_ticks / 2);
    let snapshot = f.world.build_render_snapshot();
    let progress = snapshot
        .bubbles
        .iter()
        .find(|bubble| bubble.entity_id == lab)
        .and_then(|bubble| bubble.build_or_queue_progress)
        .ok_or("progress is published")?;
    assert!((0.45..=0.55).contains(&progress), "{progress}");
    Ok(())
}
