//! Gas, research and transport through the card, on the shipped assets (D-9).
//!
//! # Responsibility
//! The extractor, the tech module and the only carrier ship real, verified media (Veo batch 1,
//! `docs/evidence/veo/2026-11-04-batch-1.md`), and so do the structures on the way to the
//! carrier. These tests play each path through the command card, as a player gets the game.
//!
//! # How it fits
//! Each test boots the real loader on the real assets root, the one the binary uses (charter
//! §1.2). They replace the Q0002 tests that ran the same paths on procedural stand-in atlases
//! in a temporary root before the media existed.
//!
//! # Invariants
//! - No test here writes a file: this is the shipped media, as a player gets it.
//! - Setup may spawn units and fund a player through `world_mut`. Every order under test arrives
//!   through an input event.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use bevy::prelude::*;
use bevy_egui::egui;
use client::battle_hud::card::{CardAction, CardLabel, CardLock, CommandCard};
use client::battle_hud::model::HudModel;
use client::battle_hud::HudGeometry;
use client::battle_input::command_mode::CommandMode;
use client::battle_pipeline::session::BattleSession;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::{ClientMode, LabState};
use sim_types::{
    EntityId, OptionLock, PlayerCommand, PlayerSlot, ResourceKind, UnitOrder, UnitTypeId,
};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{sim_position, Harness, LOCAL_PLAYER};

const EXTRACTOR: &str = "plasma_siphon_dome";
const MODULE: &str = "tech_module_pod";
const RESEARCHER: &str = "assembly_bay_dome";
const CARRIER: &str = "repair_shuttle";
/// The structures a player raises on the way to the carrier, which must be buildable too.
const CARRIER_PATH: [&str; 2] = ["machining_plant_dome", "aerospace_hangar_dome"];
const OVERCLOCKED: &str = "pulse_bot";
const OVERCLOCK_UPGRADE: &str = "overclock_stim";

/// The gas node the skirmish's home player starts beside, and the extractor corner that covers it.
const GAS_CELL: (u32, u32) = (14, 6);
const EXTRACTOR_CORNER: (u32, u32) = (13, 5);

/// Minimum corners of open plateau, clear of the home player's nodes.
const RESEARCHER_CORNER: u32 = 2;
const MODULE_CORNER: (u32, u32) = (2, 7);

/// More of either resource than any test can spend.
const RICH: u32 = 100_000;

/// The most frames a test may wait for the simulation.
const PATIENCE: usize = 4_000;

/// The fastest speed the HUD offers, which keeps long waits short.
const FAST: u32 = 4;

/// The shipped skirmish, paused, with the local player funded beyond any test's spending.
fn boot() -> Harness {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let bank = harness
        .world_mut()
        .player_resources
        .get_mut(LOCAL_PLAYER)
        .expect("invariant: LOCAL_PLAYER has a player slot");
    bank.minerals = RICH;
    bank.gas = RICH;
    harness
}

fn slug(harness: &Harness, slug: &str) -> UnitTypeId {
    harness
        .world()
        .catalog
        .id_by_slug(slug)
        .expect("the catalog ships this identity")
}

fn unit_of(harness: &Harness, unit_type: UnitTypeId) -> EntityId {
    harness
        .world()
        .entities
        .values()
        .find(|unit| unit.owner == LOCAL_PLAYER && unit.unit_type_id == unit_type)
        .unwrap_or_else(|| panic!("no unit of type {unit_type:?}"))
        .id
}

fn a_worker(harness: &Harness) -> EntityId {
    let world = harness.world();
    world
        .entities
        .values()
        .find(|unit| {
            unit.owner == LOCAL_PLAYER
                && world
                    .catalog
                    .get(unit.unit_type_id)
                    .is_ok_and(|archetype| archetype.capabilities.can_construct)
        })
        .expect("a worker")
        .id
}

fn card(harness: &Harness) -> &CommandCard {
    harness.app.world().resource::<CommandCard>()
}

fn model(harness: &Harness) -> &HudModel {
    harness.app.world().resource::<HudModel>()
}

fn pending(harness: &Harness) -> Vec<PlayerCommand> {
    harness
        .app
        .world()
        .resource::<BattleSession>()
        .commands
        .pending_commands()
        .map(|(_, command)| command.clone())
        .collect()
}

fn position_of_label(harness: &Harness, label: CardLabel) -> usize {
    card(harness)
        .slots
        .iter()
        .position(|slot| slot.as_ref().is_some_and(|slot| slot.label == label))
        .unwrap_or_else(|| panic!("{label:?} is not on the card: {:?}", card(harness).slots))
}

fn tap_label(harness: &mut Harness, label: CardLabel) {
    let position = position_of_label(harness, label);
    let key = card(harness).slots[position].as_ref().unwrap().key;
    harness.tap_key(key);
}

fn click_label(harness: &mut Harness, label: CardLabel) {
    harness.step(1);
    let position = position_of_label(harness, label);
    let rect =
        harness.app.world().resource::<HudGeometry>().card[position].expect("the slot was drawn");
    harness.click(MouseButton::Left, center(rect));
}

fn center(rect: egui::Rect) -> Vec2 {
    Vec2::new(rect.center().x, rect.center().y)
}

/// Runs the battle at speed until `done`, and pauses.
fn run_until(harness: &mut Harness, what: &str, done: impl Fn(&Harness) -> bool) {
    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .simulation_speed = FAST;
    harness.resume();
    for _ in 0..PATIENCE {
        harness.step(1);
        if done(harness) {
            harness.pause();
            return;
        }
    }
    panic!("{what} never happened: {}", harness.status());
}

fn present(harness: &mut Harness) {
    harness.resume();
    harness.step(3);
    harness.pause();
}

#[test]
fn the_shipped_media_lifts_the_gate_for_gas_research_and_transport() {
    let harness = boot();
    let catalog = &model(&harness).catalog;
    for identity in [EXTRACTOR, MODULE, CARRIER].into_iter().chain(CARRIER_PATH) {
        let id = slug(&harness, identity);
        let entry = catalog
            .archetype(id)
            .expect("the HUD catalog lists every identity");
        assert_eq!(entry.unpresentable, None, "{identity} must ship its media");
    }
}

#[test]
fn gas_through_the_card_extractor_snap_harvest_and_income() {
    let mut harness = boot();
    let extractor = slug(&harness, EXTRACTOR);
    let worker = a_worker(&harness);
    let gas_at = Vec2::new(GAS_CELL.0 as f32 + 0.5, GAS_CELL.1 as f32 + 0.5);
    harness.look_at(gas_at);
    harness.select(worker);

    tap_label(&mut harness, CardLabel::Build);
    let mut guard = 0;
    while !card(&harness)
        .slots
        .iter()
        .flatten()
        .any(|slot| slot.action == CardAction::Mode(CommandMode::Place(extractor)))
    {
        tap_label(&mut harness, CardLabel::More);
        guard += 1;
        assert!(guard < 4, "the extractor is on no page");
    }
    click_label(&mut harness, CardLabel::Unit(extractor));
    assert_eq!(
        *harness.app.world().resource::<CommandMode>(),
        CommandMode::Place(extractor),
        "an extractor with media is enabled"
    );

    // A click a little off the node snaps onto it.
    let beside = harness.ground_on_screen(gas_at + Vec2::new(2.0, 0.0));
    harness.click(MouseButton::Left, beside);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(
        orders[0].order,
        UnitOrder::Build {
            structure_type: extractor,
            cell_x: EXTRACTOR_CORNER.0,
            cell_y: EXTRACTOR_CORNER.1,
        }
    );

    run_until(&mut harness, "the extractor's completion", |harness| {
        harness
            .world()
            .entities
            .values()
            .any(|unit| unit.unit_type_id == extractor && unit.is_completed)
    });
    let building = unit_of(&harness, extractor);
    let gas_before = harness.world().player_resources[PlayerSlot::ALL[0]].gas;
    harness.look_at(gas_at);
    harness.select(worker);
    assert_eq!(harness.selection().ids(), [worker]);
    let on_dome = harness.unit_on_screen(building);
    harness.click(MouseButton::Right, on_dome);

    let target = harness
        .world()
        .map
        .resources()
        .iter()
        .position(|node| node.kind == ResourceKind::Gas && node.cell == GAS_CELL)
        .expect("the skirmish map holds this node");
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(
        matches!(orders[0].order, UnitOrder::Harvest { target: node } if node.0 as usize == target),
        "{:?}",
        orders[0].order
    );

    run_until(&mut harness, "gas income", |harness| {
        harness.world().player_resources[PlayerSlot::ALL[0]].gas > gas_before
    });
}

#[test]
fn research_through_the_card_then_overclock_works() {
    let mut harness = boot();
    let researcher_type = slug(&harness, RESEARCHER);
    let soldier_type = slug(&harness, OVERCLOCKED);
    let worker = a_worker(&harness);
    let upgrade = harness
        .world()
        .upgrade_catalog
        .get_by_slug(OVERCLOCK_UPGRADE)
        .expect("the catalog ships this upgrade")
        .upgrade_id;
    let researcher = harness
        .world_mut()
        .spawn_structure(
            researcher_type,
            LOCAL_PLAYER,
            RESEARCHER_CORNER,
            RESEARCHER_CORNER,
        )
        .expect("open plateau");
    let site = harness.open_ground(harness.simulated_position(worker), 3.0);
    let soldier = harness
        .world_mut()
        .spawn_unit(soldier_type, LOCAL_PLAYER, sim_position(site))
        .expect("open ground");
    present(&mut harness);

    harness.look_at_unit(researcher);
    harness.select(researcher);
    assert_eq!(harness.selection().ids(), [researcher]);
    let label = CardLabel::Upgrade(upgrade);
    let position = position_of_label(&harness, label);
    let slot = card(&harness).slots[position].clone().unwrap();
    assert!(
        matches!(
            slot.lock,
            Some(CardLock::Option(OptionLock::MissingStructure(_)))
        ),
        "{:?}",
        slot.lock
    );
    assert!(
        card(&harness).text[position]
            .tooltip
            .starts_with("Requires "),
        "{}",
        card(&harness).text[position].tooltip
    );

    let module_type = slug(&harness, MODULE);
    harness
        .world_mut()
        .spawn_structure(module_type, LOCAL_PLAYER, MODULE_CORNER.0, MODULE_CORNER.1)
        .expect("open plateau");
    present(&mut harness);
    harness.step(2);

    assert_eq!(
        card(&harness).slots[position_of_label(&harness, label)]
            .as_ref()
            .unwrap()
            .lock,
        None
    );
    click_label(&mut harness, label);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [researcher]);
    assert_eq!(orders[0].order, UnitOrder::Research { upgrade });
    present(&mut harness);
    harness.step(2);
    assert_eq!(
        card(&harness).slots[position_of_label(&harness, label)]
            .as_ref()
            .unwrap()
            .lock,
        Some(CardLock::Option(OptionLock::Researching))
    );

    run_until(&mut harness, "the research", |harness| {
        model(harness).view.overclock_ready.contains(&soldier_type)
    });

    harness.look_at_unit(soldier);
    harness.select(soldier);
    assert_eq!(harness.selection().ids(), [soldier]);
    let hp_before = harness.world().entities[&soldier].hp;
    let overclock = position_of_label(&harness, CardLabel::Overclock);
    assert!(card(&harness).slots[overclock].as_ref().unwrap().enabled);
    tap_label(&mut harness, CardLabel::Overclock);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::UseStim);
    present(&mut harness);

    assert!(
        harness.world().entities[&soldier].hp < hp_before,
        "an overclock must take effect: it costs its unit health"
    );
}

/// Spawns a carrier and two passengers for it beside the local player's first worker, and
/// returns the carrier and one passenger.
fn spawn_carrier_and_passengers(harness: &mut Harness) -> (EntityId, EntityId) {
    let carrier_type = slug(harness, CARRIER);
    let passenger_type = slug(harness, OVERCLOCKED);
    let origin = harness.simulated_position(a_worker(harness));
    let site = harness.open_ground(origin, 4.0);
    let carrier = harness
        .world_mut()
        .spawn_unit(carrier_type, LOCAL_PLAYER, sim_position(site))
        .expect("open ground");
    let mut passengers = Vec::new();
    for offset in [-1.5, 1.5] {
        let spot = harness.open_ground(site + Vec2::new(offset, 0.0), 1.0);
        let passenger = harness
            .world_mut()
            .spawn_unit(passenger_type, LOCAL_PLAYER, sim_position(spot))
            .expect("open ground");
        passengers.push(passenger);
    }
    present(harness);
    (carrier, passengers[0])
}

#[test]
fn load_and_unload_through_right_click_and_the_card() {
    let mut harness = boot();
    let (carrier, passenger) = spawn_carrier_and_passengers(&mut harness);
    harness.look_at_unit(carrier);

    harness.select(carrier);
    assert!(
        !card(&harness)
            .slots
            .iter()
            .flatten()
            .any(|slot| slot.label == CardLabel::Unload),
        "an empty carrier has nothing to unload"
    );

    harness.select(passenger);
    assert_eq!(harness.selection().ids(), [passenger]);
    let on_carrier = harness.unit_on_screen(carrier);
    harness.click(MouseButton::Right, on_carrier);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::Load { carrier });

    run_until(&mut harness, "boarding", |harness| {
        harness
            .app
            .world()
            .resource::<HudModel>()
            .view
            .carriers
            .iter()
            .any(|view| view.entity == carrier && view.slots_used > 0)
    });

    let on_carrier = harness.unit_on_screen(carrier);
    harness.click(MouseButton::Left, on_carrier);
    assert_eq!(harness.selection().ids(), [carrier]);
    click_label(&mut harness, CardLabel::Unload);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::UnloadAll);

    run_until(&mut harness, "unloading", |harness| {
        harness
            .app
            .world()
            .resource::<HudModel>()
            .view
            .carriers
            .iter()
            .any(|view| view.entity == carrier && view.slots_used == 0)
    });
    assert!(!harness.world().entities[&passenger].is_loaded_in_transport());
}
