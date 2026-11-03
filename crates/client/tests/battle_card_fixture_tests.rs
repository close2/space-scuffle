//! Gas, research and transport through the card, on a temporary assets root (D-9).
//!
//! # Responsibility
//! The shipped media stops at the mineral path: the extractor, the tech module and the only
//! carrier have none, so their entries are disabled for a player. These tests prove the entries
//! work the moment real media arrives, with no code change.
//!
//! # How it fits
//! Each test boots the real loader on a temporary directory holding the real files plus procedural
//! atlases for the identities it needs. The atlases are stand-ins for loading only: nothing here
//! judges how anything looks.
//!
//! # Invariants
//! - Everything is written beneath `tempfile::tempdir`, and each test asserts that `assets/textures`
//!   is unchanged. Procedural atlases never reach a runtime path.
//! - Setup may spawn units and fund a player through `world_mut`. Every order under test arrives
//!   through an input event.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use std::path::Path;

use bevy::prelude::*;
use bevy_egui::egui;
use client::battle_hud::card::{CardAction, CardLabel, CardLock, CommandCard};
use client::battle_hud::model::HudModel;
use client::battle_hud::HudGeometry;
use client::battle_input::command_mode::CommandMode;
use client::battle_pipeline::session::BattleSession;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::{ClientMode, LabState};
use sim_types::{EntityId, OptionLock, PlayerCommand, ResourceKind, UnitOrder, UnitTypeId};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{fixture_root, shipped_textures, sim_position, Harness, LOCAL_PLAYER};

const EXTRACTOR: &str = "plasma_siphon_dome";
const MODULE: &str = "tech_module_pod";
const RESEARCHER: &str = "assembly_bay_dome";
const CARRIER: &str = "repair_shuttle";
const OVERCLOCKED: &str = "pulse_bot";
const OVERCLOCK_UPGRADE: &str = "overclock_stim";

/// The scenario of the transport test, written into its temporary root.
const CARRIER_SCENARIO: &str = "scenarios/carrier_start.ron";

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

fn boot(root: &Path, scenario: &str) -> Harness {
    let mut harness = Harness::boot_root(root, ClientMode::Skirmish, scenario);
    let bank = &mut harness.world_mut().player_resources[usize::from(LOCAL_PLAYER.0)];
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

/// A fresh root whose shipped textures are asserted unchanged when `play` ends.
fn on_fixture(identities: &[&str], play: impl FnOnce(&Path)) {
    let before = shipped_textures();
    let root = fixture_root(identities);
    play(root.path());
    assert_eq!(
        shipped_textures(),
        before,
        "assets/textures must not change"
    );
}

#[test]
fn a_fixture_root_lifts_the_media_gate_for_exactly_its_identities() {
    on_fixture(&[EXTRACTOR], |root| {
        let harness = boot(root, SKIRMISH_SCENARIO);
        let extractor = slug(&harness, EXTRACTOR);
        let module = slug(&harness, MODULE);
        let catalog = &model(&harness).catalog;
        assert_eq!(catalog.archetype(extractor).unwrap().unpresentable, None);
        assert!(catalog.archetype(module).unwrap().unpresentable.is_some());
    });
}

#[test]
fn gas_through_the_card_extractor_snap_harvest_and_income() {
    on_fixture(&[EXTRACTOR], |root| {
        let mut harness = boot(root, SKIRMISH_SCENARIO);
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
        let gas_before = harness.world().player_resources[0].gas;
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
            harness.world().player_resources[0].gas > gas_before
        });
    });
}

#[test]
fn research_through_the_card_then_overclock_works() {
    on_fixture(&[MODULE], |root| {
        let mut harness = boot(root, SKIRMISH_SCENARIO);
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
    });
}

/// Writes a scenario that starts the local player with a carrier and passengers for it.
fn write_carrier_scenario(root: &Path) {
    let path = root.join("catalog/start_loadouts.ron");
    let loadouts = std::fs::read_to_string(&path).expect("the copied loadouts");
    let extended = loadouts.replacen(
        "loadouts: [",
        &format!(
            r#"loadouts: [
        (
            slug: "carrier_fixture",
            display_name: "Test fixture",
            town_hall: "foundry_core_dome",
            units: [
                (slug: "{CARRIER}", count: 1),
                (slug: "{OVERCLOCKED}", count: 2),
            ],
        ),"#
        ),
        1,
    );
    std::fs::write(&path, extended).expect("a temporary directory");
    std::fs::write(
        root.join(CARRIER_SCENARIO),
        r#"(
    format_version: 1,
    map: "maps/showcase_canyon.scmap.ron",
    seed: 7,
    players: [
        (player_slot: 0, loadout_slug: "carrier_fixture"),
        (player_slot: 1, loadout_slug: "standard_automaton"),
    ],
    initial_orders: [],
)
"#,
    )
    .expect("a temporary directory");
}

#[test]
fn load_and_unload_through_right_click_and_the_card() {
    on_fixture(&[CARRIER], |root| {
        write_carrier_scenario(root);
        let mut harness = boot(root, CARRIER_SCENARIO);
        let carrier = unit_of(&harness, slug(&harness, CARRIER));
        let passenger = unit_of(&harness, slug(&harness, OVERCLOCKED));
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
    });
}
