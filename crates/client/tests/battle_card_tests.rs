//! The command card, the queue panel and the placement ghost, played with real input.
//!
//! # Responsibility
//! Boots the production client on the shipped skirmish, presses what `HudGeometry` says was drawn
//! and the keys a player would, and asserts on the commands captured and on what the simulation
//! then did. What appears on a card for which selection is tested beside the derivation, in
//! `battle_hud::card`. This file proves that a press reaches it.
//!
//! # Limits
//! Shares the limits of `support/battle_harness.rs`: no window and no renderer, so the ghost is
//! checked by its visibility, transform and material handle.
//!
//! # Invariants
//! - Setup may fund a player or spawn a unit through `world_mut`, as the harness allows. Every
//!   order under test arrives through an input event.
//! - No test here writes into the shipped root: the media is the shipped media, as a player gets
//!   it. The two tests of a card entry without media boot a temporary root of links to the
//!   shipped files with one raised structure's atlas left out, because every structure the
//!   worker raises now ships media (Veo batch 4).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy_egui::egui;
use client::battle_hud::card::{
    CardAction, CardLabel, CardLock, CardPage, CommandCard, NO_MEDIA_TEXT,
};
use client::battle_hud::feed::BattleFeed;
use client::battle_hud::model::HudModel;
use client::battle_hud::placement::{PlacementGhost, PlacementGhostAssets, PlacementPreview};
use client::battle_hud::queue::QueuePanel;
use client::battle_hud::HudGeometry;
use client::battle_input::command_mode::{placement_cell, CommandMode};
use client::battle_pipeline::session::BattleSession;
use client::input_bindings::{
    CANCEL_KEY, CONTROL_MODIFIERS, HOLD_POSITION_KEY, SHIFT_MODIFIERS, STOP_KEY,
};
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::{ClientMode, LabState};
use sim_core::production::construction::CONSTRUCTION_CANCEL_REFUND_PERCENT;
use sim_types::{
    EntityId, MatchResult, PlayerCommand, PlayerId, SiteRefusal, Tick, UnitOrder, UnitTypeId,
};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{assets, Harness, LOCAL_PLAYER};

/// A structure with shipped media whose only prerequisite is the starting base.
const STRUCTURE: &str = "assembly_bay_dome";

/// A unit with an overclock that must be researched.
const OVERCLOCKED: &str = "pulse_bot";

/// Minimum corner of open plateau in the skirmish, clear of the home player's nodes.
const SITE_CORNER: u32 = 2;

/// More minerals than any test can spend.
const RICH: u32 = 100_000;

/// Frames a builder gets to reach a site, at 25 ticks a second.
const WALK_FRAMES: usize = 900;

const ENEMY: PlayerId = PlayerId(1);

fn skirmish() -> Harness {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = worker(&harness);
    let position = harness.simulated_position(worker);
    harness.look_at(position);
    harness
}

/// The local player's first worker.
fn worker(harness: &Harness) -> EntityId {
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
        .expect("the skirmish starts with workers")
        .id
}

fn slug(harness: &Harness, slug: &str) -> UnitTypeId {
    harness
        .world()
        .catalog
        .id_by_slug(slug)
        .expect("the catalog ships this identity")
}

fn minerals(harness: &Harness) -> u32 {
    harness
        .world()
        .player_resources
        .get(LOCAL_PLAYER)
        .expect("invariant: LOCAL_PLAYER has a player slot")
        .minerals
}

fn fund(harness: &mut Harness) {
    harness
        .world_mut()
        .player_resources
        .get_mut(LOCAL_PLAYER)
        .expect("invariant: LOCAL_PLAYER has a player slot")
        .minerals = RICH;
}

fn card(harness: &Harness) -> &CommandCard {
    harness.app.world().resource::<CommandCard>()
}

fn mode(harness: &Harness) -> CommandMode {
    *harness.app.world().resource::<CommandMode>()
}

fn preview(harness: &Harness) -> &PlacementPreview {
    harness.app.world().resource::<PlacementPreview>()
}

fn panel(harness: &Harness) -> &QueuePanel {
    harness.app.world().resource::<QueuePanel>()
}

fn geometry(harness: &Harness) -> &HudGeometry {
    harness.app.world().resource::<HudGeometry>()
}

fn point(rect: egui::Rect) -> Vec2 {
    Vec2::new(rect.center().x, rect.center().y)
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

fn feed_lines(harness: &Harness) -> Vec<String> {
    harness
        .app
        .world()
        .resource::<BattleFeed>()
        .lines()
        .map(|line| line.text.clone())
        .collect()
}

/// The position of the slot `wanted` accepts, on the card on show.
fn position_of(harness: &Harness, wanted: impl Fn(&CardAction) -> bool) -> Option<usize> {
    card(harness)
        .slots
        .iter()
        .position(|slot| slot.as_ref().is_some_and(|slot| wanted(&slot.action)))
}

fn position_of_label(harness: &Harness, label: CardLabel) -> usize {
    card(harness)
        .slots
        .iter()
        .position(|slot| slot.as_ref().is_some_and(|slot| slot.label == label))
        .unwrap_or_else(|| panic!("{label:?} is not on the card: {:?}", card(harness).slots))
}

fn key_at(harness: &Harness, position: usize) -> KeyCode {
    card(harness).slots[position]
        .as_ref()
        .expect("a filled slot")
        .key
}

/// Clicks the button drawn at card position `position`.
///
/// A selection made on the latest frame is drawn on the next, so the card gets that frame first.
fn click_slot(harness: &mut Harness, position: usize) {
    harness.step(1);
    let rect = geometry(harness).card[position].expect("the slot was drawn");
    harness.click(MouseButton::Left, point(rect));
}

/// Taps the key of the slot labelled `label`.
fn tap_label(harness: &mut Harness, label: CardLabel) {
    let key = key_at(harness, position_of_label(harness, label));
    harness.tap_key(key);
}

/// Shows the page of the build list holding `structure`, by key, and returns its position.
fn turn_to(harness: &mut Harness, structure: UnitTypeId) -> usize {
    let wanted = CardAction::Mode(CommandMode::Place(structure));
    loop {
        if let Some(position) = position_of(harness, |action| *action == wanted) {
            return position;
        }
        tap_label(harness, CardLabel::More);
    }
}

/// Selects a funded worker with the free site in view, and returns the worker and the site.
fn worker_and_site(harness: &mut Harness) -> (EntityId, Vec2) {
    let worker = worker(harness);
    fund(harness);
    let site = Vec2::new(SITE_CORNER as f32 + 1.5, SITE_CORNER as f32 + 1.5);
    let worker_at = harness.simulated_position(worker);
    harness.look_at((worker_at + site) * 0.5);
    harness.select(worker);
    assert_eq!(harness.selection().ids(), [worker]);
    (worker, site)
}

/// Runs the battle until a foundation stands.
fn run_until_laid(harness: &mut Harness) -> EntityId {
    harness.resume();
    for _ in 0..WALK_FRAMES {
        harness.step(1);
        let laid = harness
            .world()
            .entities
            .values()
            .find(|unit| unit.construction.is_some())
            .map(|unit| unit.id);
        if let Some(foundation) = laid {
            harness.pause();
            return foundation;
        }
    }
    panic!("no foundation was laid: {}", harness.status());
}

fn run_ticks(harness: &mut Harness, ticks: u32) {
    let goal = harness.world().current_tick + ticks;
    harness.resume();
    let mut frames = 0;
    while harness.world().current_tick < goal {
        harness.step(1);
        frames += 1;
        assert!(frames < 10_000, "the battle stopped ticking");
    }
    harness.pause();
}

fn assert_build_at_site(harness: &Harness, worker: EntityId, structure: UnitTypeId) {
    let orders = pending(harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [worker]);
    assert_eq!(
        orders[0].order,
        UnitOrder::Build {
            structure_type: structure,
            cell_x: SITE_CORNER,
            cell_y: SITE_CORNER,
        }
    );
}

fn assert_foundation_paid(harness: &mut Harness, structure: UnitTypeId) {
    let foundation = run_until_laid(harness);
    let unit = &harness.world().entities[&foundation];
    assert_eq!(unit.unit_type_id, structure);
    let cost = harness.world().catalog.get(structure).unwrap().mineral_cost;
    assert_eq!(minerals(harness), RICH - cost, "paid on arrival");
    let bubble = harness
        .world()
        .entities
        .get(&foundation)
        .is_some_and(|unit| !unit.is_completed);
    assert!(bubble, "a foundation is unfinished");
}

/// Spawns a unit of `owner` on open ground beside `near`, with its bubble presented.
fn spawn_beside(
    harness: &mut Harness,
    near: EntityId,
    unit_type: UnitTypeId,
    owner: PlayerId,
) -> EntityId {
    let at = harness.open_ground(harness.simulated_position(near), 3.0);
    let unit = harness
        .world_mut()
        .spawn_unit(unit_type, owner, battle_harness::sim_position(at))
        .expect("open ground beside a unit");
    run_ticks(harness, 1);
    unit
}

/// Sends keyboard events that all arrive within one frame.
fn chord(harness: &mut Harness, events: &[(KeyCode, ButtonState)]) {
    let window = harness.window();
    for (key_code, state) in events {
        harness.app.world_mut().send_event(KeyboardInput {
            key_code: *key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: *state,
            repeat: false,
            window,
        });
    }
    harness.step(1);
}

fn tap_events(key: KeyCode) -> [(KeyCode, ButtonState); 2] {
    [(key, ButtonState::Pressed), (key, ButtonState::Released)]
}

// --- Building, by key and by mouse --------------------------------------------------------------

#[test]
fn build_key_then_structure_key_then_click_raises_a_building() {
    let mut harness = skirmish();
    let (worker, site) = worker_and_site(&mut harness);
    let structure = slug(&harness, STRUCTURE);

    tap_label(&mut harness, CardLabel::Build);
    assert_eq!(card(&harness).page, CardPage::Build { page: 0 });
    let position = turn_to(&mut harness, structure);
    harness.tap_key(key_at(&harness, position));
    assert_eq!(mode(&harness), CommandMode::Place(structure));

    let on_screen = harness.ground_on_screen(site);
    harness.click(MouseButton::Left, on_screen);

    assert_build_at_site(&harness, worker, structure);
    assert_eq!(mode(&harness), CommandMode::Normal);
    assert_foundation_paid(&mut harness, structure);
}

#[test]
fn the_same_flow_works_by_mouse_alone() {
    let mut harness = skirmish();
    let (worker, site) = worker_and_site(&mut harness);
    let structure = slug(&harness, STRUCTURE);

    let build = position_of_label(&harness, CardLabel::Build);
    click_slot(&mut harness, build);
    assert_eq!(card(&harness).page, CardPage::Build { page: 0 });
    harness.step(1);
    let wanted = CardAction::Mode(CommandMode::Place(structure));
    let position = position_of(&harness, |action| *action == wanted)
        .expect("the structure opens the list, which is in catalog order");
    click_slot(&mut harness, position);
    assert_eq!(mode(&harness), CommandMode::Place(structure));

    let on_screen = harness.ground_on_screen(site);
    harness.click(MouseButton::Left, on_screen);

    assert_build_at_site(&harness, worker, structure);
    assert_foundation_paid(&mut harness, structure);
}

#[test]
fn a_whole_build_chord_in_one_frame_reaches_the_structure_page() {
    let mut harness = skirmish();
    let (_, _) = worker_and_site(&mut harness);
    let structure = slug(&harness, STRUCTURE);
    let build_key = key_at(&harness, position_of_label(&harness, CardLabel::Build));

    // Learn the structure's key, then come home.
    harness.tap_key(build_key);
    let position = turn_to(&mut harness, structure);
    let structure_key = key_at(&harness, position);
    assert_eq!(card(&harness).page, CardPage::Build { page: 0 });
    harness.tap_key(CANCEL_KEY);
    assert_eq!(card(&harness).page, CardPage::HOME);

    let mut events = tap_events(build_key).to_vec();
    events.extend(tap_events(structure_key));
    chord(&mut harness, &events);

    assert_eq!(mode(&harness), CommandMode::Place(structure));
}

#[test]
fn a_key_after_a_group_recall_in_one_frame_meets_the_recalled_selection() {
    let mut harness = skirmish();
    let worker = worker(&harness);
    harness.select(worker);
    harness.with_key_held(CONTROL_MODIFIERS[0], |harness| {
        harness.tap_key(KeyCode::Digit1);
    });
    let build_key = key_at(&harness, position_of_label(&harness, CardLabel::Build));
    let base = harness.local_structure();
    harness.select(base);
    assert_eq!(
        position_of(&harness, |a| *a == CardAction::OpenBuildMenu),
        None
    );

    let mut events = tap_events(KeyCode::Digit1).to_vec();
    events.extend(tap_events(build_key));
    chord(&mut harness, &events);
    harness.step(2);

    assert_eq!(harness.selection().ids(), [worker]);
    assert_eq!(
        card(&harness).page,
        CardPage::Build { page: 0 },
        "the refresh after the keys must not close the page they opened"
    );
}

#[test]
fn a_click_on_a_card_button_does_not_reach_the_world() {
    let mut harness = skirmish();
    let worker = worker(&harness);
    harness.select(worker);
    let build = position_of_label(&harness, CardLabel::Build);

    click_slot(&mut harness, build);

    assert_eq!(
        harness.selection().ids(),
        [worker],
        "nothing was deselected"
    );
    assert_eq!(harness.pending_orders(), 0);
}

// --- Pages and modes ----------------------------------------------------------------------------

#[test]
fn escape_leaves_a_mode_before_it_leaves_a_page() {
    let mut harness = skirmish();
    let (_, _) = worker_and_site(&mut harness);

    // Attack is armed from the root, and the list opens on top of it.
    tap_label(&mut harness, CardLabel::Attack);
    assert_eq!(mode(&harness), CommandMode::AttackMove);
    tap_label(&mut harness, CardLabel::Build);

    harness.tap_key(CANCEL_KEY);
    assert_eq!(mode(&harness), CommandMode::Normal);
    assert_eq!(card(&harness).page, CardPage::Build { page: 0 });

    harness.tap_key(CANCEL_KEY);
    assert_eq!(card(&harness).page, CardPage::HOME);
}

#[test]
fn more_and_back_walk_the_build_list() {
    let mut harness = skirmish();
    let (_, _) = worker_and_site(&mut harness);
    tap_label(&mut harness, CardLabel::Build);

    tap_label(&mut harness, CardLabel::More);
    assert_eq!(card(&harness).page, CardPage::Build { page: 1 });
    tap_label(&mut harness, CardLabel::Back);
    assert_eq!(card(&harness).page, CardPage::Build { page: 0 });
    tap_label(&mut harness, CardLabel::Back);
    assert_eq!(card(&harness).page, CardPage::HOME);
}

#[test]
fn a_new_selection_returns_the_card_to_its_root() {
    let mut harness = skirmish();
    let (_, _) = worker_and_site(&mut harness);
    tap_label(&mut harness, CardLabel::Build);

    let base = harness.local_structure();
    harness.look_at_unit(base);
    harness.select(base);

    assert_eq!(harness.selection().ids(), [base]);
    assert_eq!(card(&harness).page, CardPage::HOME);
}

#[test]
fn a_finished_placement_returns_the_card_to_its_root() {
    let mut harness = skirmish();
    let (_, site) = worker_and_site(&mut harness);
    let structure = slug(&harness, STRUCTURE);
    tap_label(&mut harness, CardLabel::Build);
    let position = turn_to(&mut harness, structure);
    harness.tap_key(key_at(&harness, position));

    let on_screen = harness.ground_on_screen(site);
    harness.click(MouseButton::Left, on_screen);
    harness.step(1);

    assert_eq!(card(&harness).page, CardPage::HOME);
}

#[test]
fn ctrl_with_a_card_key_does_nothing() {
    let mut harness = skirmish();
    let worker = worker(&harness);
    harness.select(worker);
    let build_key = key_at(&harness, position_of_label(&harness, CardLabel::Build));

    harness.with_key_held(CONTROL_MODIFIERS[0], |harness| {
        harness.tap_key(build_key);
    });

    assert_eq!(card(&harness).page, CardPage::HOME);
}

#[test]
fn a_new_session_ends_a_mode_with_no_reset_of_its_own() {
    let mut harness = skirmish();
    let (_, _) = worker_and_site(&mut harness);
    tap_label(&mut harness, CardLabel::Attack);
    assert_eq!(mode(&harness), CommandMode::AttackMove);

    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .battle_reset_requested = true;
    harness.step(4);

    assert_eq!(harness.selection().ids().len(), 0);
    assert_eq!(mode(&harness), CommandMode::Normal);
    assert!(card(&harness).slots.iter().all(Option::is_none));
}

// --- Who gets a card ----------------------------------------------------------------------------

#[test]
fn an_inspected_enemy_gives_an_empty_card() {
    let mut harness = skirmish();
    let worker = worker(&harness);
    let enemy_type = harness.world().entities[&worker].unit_type_id;
    let spawned = spawn_beside(&mut harness, worker, enemy_type, ENEMY);

    harness.select(spawned);

    assert_eq!(harness.selection().inspected(), Some(spawned));
    assert!(card(&harness).slots.iter().all(Option::is_none));
    assert!(card(&harness).fixed.iter().all(Option::is_none));
}

#[test]
fn a_finished_match_hides_the_card_and_keys_do_nothing() {
    let mut harness = skirmish();
    let worker = worker(&harness);
    harness.select(worker);
    let build_key = key_at(&harness, position_of_label(&harness, CardLabel::Build));
    assert!(card(&harness).slots.iter().any(Option::is_some));

    harness.world_mut().match_state.result = MatchResult::Draw { tick: Tick(1) };
    run_ticks(&mut harness, 1);

    assert!(card(&harness).slots.iter().all(Option::is_none));
    harness.tap_key(build_key);
    assert_eq!(card(&harness).page, CardPage::HOME);
    assert_eq!(harness.pending_orders(), 0);
    assert_eq!(geometry(&harness).card, [None; 15]);
}

#[test]
fn the_sandbox_has_no_card_keys() {
    let mut harness = Harness::battle();
    let army = harness.local_army();
    harness.select(army[0]);

    harness.tap_key(KeyCode::KeyQ);

    assert_eq!(harness.pending_orders(), 0);
    assert_eq!(mode(&harness), CommandMode::Normal);
    assert!(card(&harness).slots.iter().all(Option::is_none));
}

// --- Locks --------------------------------------------------------------------------------------

/// Whether the shipped root holds no atlas manifest for `structure`.
fn ships_no_atlas(harness: &Harness, structure: UnitTypeId) -> bool {
    let slug = &harness
        .world()
        .catalog
        .get(structure)
        .expect("the worker raises catalog identities")
        .slug;
    !assets()
        .join("textures")
        .join(format!("{slug}.flipbook.ron"))
        .exists()
}

/// Every structure the selected worker can raise, from the HUD's catalog.
fn raised_by(harness: &Harness, worker: EntityId) -> Vec<UnitTypeId> {
    let raiser = harness.world().entities[&worker].unit_type_id;
    harness
        .app
        .world()
        .resource::<HudModel>()
        .catalog
        .archetype(raiser)
        .expect("the HUD catalog lists the worker")
        .raises
        .clone()
}

/// The skirmish booted on a temporary root that mirrors the shipped one by symbolic links,
/// with the atlas of the first structure the worker raises that the scenario does not start
/// with left out, and that structure.
///
/// Every structure the worker raises ships media, so the test makes its own root instead of
/// naming an identity (Veo plan D-9: the catalog and the root decide, not the test). The
/// directory must outlive the harness. Unix only: the mirror is made of symbolic links.
#[cfg(unix)]
fn skirmish_with_one_structure_unshipped() -> (tempfile::TempDir, Harness, UnitTypeId) {
    let shipped = skirmish();
    // A structure already standing in the scenario must be presentable, or the battle stops.
    let standing = |structure: &UnitTypeId| {
        shipped
            .world()
            .entities
            .values()
            .any(|entity| entity.unit_type_id == *structure)
    };
    let structure = raised_by(&shipped, worker(&shipped))
        .into_iter()
        .find(|structure| !standing(structure))
        .expect("the worker raises a structure the scenario does not start with");
    let slug = shipped
        .world()
        .catalog
        .get(structure)
        .expect("the worker raises catalog identities")
        .slug
        .clone();
    drop(shipped);

    let directory = tempfile::tempdir().expect("temporary root");
    let textures = directory.path().join("textures");
    std::fs::create_dir(&textures).expect("temporary textures directory");
    for entry in std::fs::read_dir(assets()).expect("shipped root") {
        let entry = entry.expect("shipped root entry");
        if entry.file_name() != "textures" {
            std::os::unix::fs::symlink(entry.path(), directory.path().join(entry.file_name()))
                .expect("link to a shipped directory");
        }
    }
    let unshipped = format!("{slug}.");
    for entry in std::fs::read_dir(assets().join("textures")).expect("shipped textures") {
        let entry = entry.expect("shipped texture");
        if !entry.file_name().to_string_lossy().starts_with(&unshipped) {
            std::os::unix::fs::symlink(entry.path(), textures.join(entry.file_name()))
                .expect("link to a shipped texture");
        }
    }

    let mut harness = Harness::boot_root(directory.path(), ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let position = harness.simulated_position(worker(&harness));
    harness.look_at(position);
    assert!(
        harness
            .app
            .world()
            .resource::<HudModel>()
            .catalog
            .archetype(structure)
            .is_some_and(|entry| entry.unpresentable.is_some()),
        "{slug} must be unpresentable without its atlas"
    );
    (directory, harness, structure)
}

#[test]
fn exactly_the_structures_without_shipped_media_are_unpresentable() {
    let mut harness = skirmish();
    let (worker, _) = worker_and_site(&mut harness);
    let raised = raised_by(&harness, worker);
    assert_ne!(raised.len(), 0, "the worker raises no structure");
    let model = harness.app.world().resource::<HudModel>();
    for structure in raised {
        let entry = model.catalog.archetype(structure).expect("a raised entry");
        assert_eq!(
            entry.unpresentable.is_some(),
            ships_no_atlas(&harness, structure),
            "{:?}: {:?}",
            structure,
            entry.unpresentable
        );
    }
}

#[cfg(unix)]
#[test]
fn a_card_entry_without_media_is_disabled_for_want_of_media() {
    let (_root, mut harness, structure) = skirmish_with_one_structure_unshipped();
    worker_and_site(&mut harness);
    tap_label(&mut harness, CardLabel::Build);

    let position = turn_to(&mut harness, structure);

    let slot = card(&harness).slots[position].as_ref().unwrap();
    assert!(!slot.enabled);
    assert_eq!(slot.lock, Some(CardLock::NoMedia));
    assert_eq!(card(&harness).text[position].tooltip, NO_MEDIA_TEXT);
}

#[cfg(unix)]
#[test]
fn a_disabled_entry_issues_nothing_and_says_why() {
    let (_root, mut harness, structure) = skirmish_with_one_structure_unshipped();
    worker_and_site(&mut harness);
    tap_label(&mut harness, CardLabel::Build);
    let position = turn_to(&mut harness, structure);
    assert!(!card(&harness).slots[position].as_ref().unwrap().enabled);
    let reason = card(&harness).text[position].tooltip.clone();
    assert_eq!(reason, NO_MEDIA_TEXT);

    // By mouse: a disabled button takes no click, and its tooltip is the explanation.
    click_slot(&mut harness, position);
    assert_eq!(mode(&harness), CommandMode::Normal);
    assert_eq!(harness.pending_orders(), 0);

    // By key: the same sentence reaches the feed.
    harness.tap_key(key_at(&harness, position));
    harness.step(1);
    assert_eq!(mode(&harness), CommandMode::Normal);
    assert_eq!(harness.pending_orders(), 0);
    assert!(harness.status().contains(&reason), "{}", harness.status());
    assert!(
        feed_lines(&harness)
            .iter()
            .any(|line| line.contains(&reason)),
        "{:?}",
        feed_lines(&harness)
    );
}

#[test]
fn overclock_before_research_issues_nothing_by_mouse_or_key() {
    let mut harness = skirmish();
    let worker = worker(&harness);
    let unit_type = slug(&harness, OVERCLOCKED);
    let unit = spawn_beside(&mut harness, worker, unit_type, LOCAL_PLAYER);
    harness.select(unit);
    assert_eq!(harness.selection().ids(), [unit]);

    let position = position_of_label(&harness, CardLabel::Overclock);
    let slot = card(&harness).slots[position].as_ref().unwrap();
    assert!(!slot.enabled);
    assert_eq!(slot.action, CardAction::Order(UnitOrder::UseStim));
    let reason = card(&harness).text[position].tooltip.clone();
    assert!(reason.starts_with("Requires "), "{reason}");

    click_slot(&mut harness, position);
    assert_eq!(harness.pending_orders(), 0, "the mouse path");

    harness.tap_key(key_at(&harness, position));
    assert_eq!(harness.pending_orders(), 0, "the hotkey path");
    assert!(harness.status().contains(&reason), "{}", harness.status());
}

// --- Stop and Hold ------------------------------------------------------------------------------

#[test]
fn stop_and_hold_appear_and_their_keys_still_work() {
    let mut harness = skirmish();
    let worker = worker(&harness);
    harness.select(worker);
    harness.step(1);
    let fixed = geometry(&harness).card_fixed;
    let (stop, hold) = (fixed[0].expect("Stop"), fixed[1].expect("Hold"));

    harness.click(MouseButton::Left, point(stop));
    harness.click(MouseButton::Left, point(hold));
    let by_mouse: Vec<_> = pending(&harness).into_iter().map(|c| c.order).collect();
    assert_eq!(by_mouse, [UnitOrder::Stop, UnitOrder::HoldPosition]);
    assert_eq!(harness.selection().ids(), [worker]);

    run_ticks(&mut harness, 1);
    harness.tap_key(STOP_KEY);
    harness.tap_key(HOLD_POSITION_KEY);
    let by_key: Vec<_> = pending(&harness).into_iter().map(|c| c.order).collect();
    assert_eq!(
        by_key,
        [UnitOrder::Stop, UnitOrder::HoldPosition],
        "each key issues its order once"
    );
}

#[test]
fn a_card_order_is_queued_with_shift() {
    let mut harness = skirmish();
    let worker = worker(&harness);
    harness.select(worker);
    harness.step(1);
    let stop = geometry(&harness).card_fixed[0].expect("Stop");

    harness.with_key_held(SHIFT_MODIFIERS[0], |harness| {
        harness.click(MouseButton::Left, point(stop));
    });

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1);
    assert!(orders[0].queue);
}

// --- Training and the queue panel ---------------------------------------------------------------

/// Selects the base with a funded player, and returns it with the position of its first product.
fn base_and_product(harness: &mut Harness) -> (EntityId, usize) {
    fund(harness);
    let base = harness.local_structure();
    harness.look_at_unit(base);
    harness.select(base);
    assert_eq!(harness.selection().ids(), [base]);
    let position = position_of(harness, |action| matches!(action, CardAction::Train(_)))
        .expect("a base trains something");
    (base, position)
}

#[test]
fn train_cancel_and_refund_through_the_queue_panel() {
    let mut harness = skirmish();
    let (base, position) = base_and_product(&mut harness);

    click_slot(&mut harness, position);
    assert_eq!(pending(&harness)[0].entities, [base]);
    run_ticks(&mut harness, 1);

    let CardAction::Train(product) = card(&harness).slots[position].as_ref().unwrap().action else {
        panic!("the slot changed");
    };
    let cost = harness.world().catalog.get(product).unwrap().mineral_cost;
    assert_eq!(minerals(&harness), RICH - cost);
    assert_eq!(panel(&harness).structure, Some(base));
    harness.step(1);
    let row = geometry(&harness).queue[0].expect("the job has a row");

    harness.click(MouseButton::Left, point(row));
    assert_eq!(
        pending(&harness)[0].order,
        UnitOrder::CancelProduction { job_index: 0 }
    );
    assert_eq!(pending(&harness)[0].entities, [base]);
    assert_eq!(harness.selection().ids(), [base], "a row is not the world");

    // While it waits for its tick, a second click cannot address a stale index.
    harness.click(MouseButton::Left, point(row));
    assert_eq!(pending(&harness).len(), 1);

    run_ticks(&mut harness, 1);
    assert_eq!(minerals(&harness), RICH, "a cancelled job refunds in full");
    assert_eq!(panel(&harness).structure, None);
    assert_eq!(geometry(&harness).queue[0], None);
}

#[test]
fn a_train_key_issues_exactly_one_job() {
    let mut harness = skirmish();
    let (base, position) = base_and_product(&mut harness);

    harness.tap_key(key_at(&harness, position));

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [base]);
    assert!(matches!(orders[0].order, UnitOrder::Produce { .. }));
}

#[test]
fn train_goes_to_the_shorter_queue() {
    let mut harness = skirmish();
    let (first, position) = base_and_product(&mut harness);
    let base_type = harness.world().entities[&first].unit_type_id;
    let at = harness.world().entities[&first].position;
    let (x, y) = harness.world().map.world_to_cell(at).unwrap();
    let second = harness
        .world_mut()
        .spawn_structure(base_type, LOCAL_PLAYER, x + 8, y)
        .expect("open ground beside the base");
    run_ticks(&mut harness, 1);
    let spot = harness.unit_on_screen(second);
    harness.with_key_held(SHIFT_MODIFIERS[0], |harness| {
        harness.click(MouseButton::Left, spot);
    });
    assert_eq!(harness.selection().ids().len(), 2);
    let (low, high) = (first.min(second), first.max(second));

    harness.tap_key(key_at(&harness, position));
    run_ticks(&mut harness, 1);
    harness.tap_key(key_at(&harness, position));
    harness.tap_key(key_at(&harness, position));

    let chosen: Vec<_> = pending(&harness).iter().map(|c| c.entities[0]).collect();
    assert_eq!(
        chosen,
        [high, low],
        "the empty queue first, then the tie goes to the lower ID"
    );
    assert!(!panel(&harness).lines().is_empty());
}

#[test]
fn cancelling_a_foundation_refunds_three_quarters() {
    let mut harness = skirmish();
    let (_, site) = worker_and_site(&mut harness);
    let structure = slug(&harness, STRUCTURE);
    tap_label(&mut harness, CardLabel::Build);
    let position = turn_to(&mut harness, structure);
    harness.tap_key(key_at(&harness, position));
    let on_screen = harness.ground_on_screen(site);
    harness.click(MouseButton::Left, on_screen);
    let foundation = run_until_laid(&mut harness);
    let cost = harness.world().catalog.get(structure).unwrap().mineral_cost;
    assert_eq!(minerals(&harness), RICH - cost);

    harness.look_at_unit(foundation);
    harness.select(foundation);
    assert_eq!(harness.selection().ids(), [foundation]);
    assert!(panel(&harness).foundation.is_some());
    harness.step(1);
    let cancel = geometry(&harness)
        .queue_cancel
        .expect("the panel offers it");
    harness.click(MouseButton::Left, point(cancel));
    assert_eq!(
        pending(&harness)[0].order,
        UnitOrder::CancelConstruction,
        "status: {}",
        harness.status()
    );
    run_ticks(&mut harness, 1);

    let refund = cost * CONSTRUCTION_CANCEL_REFUND_PERCENT / 100;
    assert_eq!(minerals(&harness), RICH - cost + refund);
    assert!(!harness.world().entities.contains_key(&foundation));
}

// --- The ghost ----------------------------------------------------------------------------------

fn ghost(harness: &mut Harness) -> (Visibility, Transform, Handle<StandardMaterial>) {
    let world = harness.app.world_mut();
    let (visibility, transform, material) = world
        .query_filtered::<(&Visibility, &Transform, &MeshMaterial3d<StandardMaterial>), With<PlacementGhost>>()
        .single(world);
    (*visibility, *transform, material.0.clone())
}

fn assert_ghost_is(harness: &mut Harness, legal: bool) {
    let (visibility, _, material) = ghost(harness);
    assert_ne!(visibility, Visibility::Hidden);
    let assets = harness.app.world().resource::<PlacementGhostAssets>();
    let wanted = if legal {
        &assets.legal
    } else {
        &assets.refused
    };
    assert_eq!(material, *wanted);
}

/// Arms placement of `structure` through the card.
fn arm_placement(harness: &mut Harness, structure: UnitTypeId) {
    tap_label(harness, CardLabel::Build);
    let position = turn_to(harness, structure);
    harness.tap_key(key_at(harness, position));
    assert_eq!(mode(harness), CommandMode::Place(structure));
}

#[test]
fn the_ghost_follows_the_pointer_and_hides_with_the_mode() {
    let mut harness = skirmish();
    let (_, site) = worker_and_site(&mut harness);
    let structure = slug(&harness, STRUCTURE);
    assert_eq!(ghost(&mut harness).0, Visibility::Hidden);
    arm_placement(&mut harness, structure);

    let on_screen = harness.ground_on_screen(site);
    harness.move_cursor(on_screen);
    harness.step(1);

    assert_eq!(preview(&harness).cell, Some((SITE_CORNER, SITE_CORNER)));
    assert_eq!(preview(&harness).verdict, Some(Ok(())));
    assert_ghost_is(&mut harness, true);
    let (_, transform, _) = ghost(&mut harness);
    assert_eq!(transform.scale.x, 3.0);
    assert_eq!(transform.scale.z, 3.0);

    harness.tap_key(CANCEL_KEY);
    harness.step(1);
    assert_eq!(ghost(&mut harness).0, Visibility::Hidden);
    assert_eq!(preview(&harness).cell, None);
}

#[test]
fn the_ghost_keeps_its_cell_while_the_pointer_is_over_a_panel() {
    let mut harness = skirmish();
    let (_, site) = worker_and_site(&mut harness);
    let structure = slug(&harness, STRUCTURE);
    arm_placement(&mut harness, structure);
    let on_screen = harness.ground_on_screen(site);
    harness.move_cursor(on_screen);
    harness.step(1);

    let over_panel = point(geometry(&harness).bottom.unwrap());
    harness.click(MouseButton::Left, over_panel);
    harness.step(1);

    assert_eq!(preview(&harness).cell, Some((SITE_CORNER, SITE_CORNER)));
    assert_eq!(harness.pending_orders(), 0, "a click there places nothing");
    assert_eq!(mode(&harness), CommandMode::Place(structure));
}

#[test]
fn a_site_on_a_cliff_shows_refused_and_issues_nothing() {
    let mut harness = skirmish();
    let (worker, _) = worker_and_site(&mut harness);
    let structure = slug(&harness, STRUCTURE);
    let origin = harness.simulated_position(worker);
    let spot = {
        let world = harness.world();
        let archetype = world.catalog.get(structure).unwrap();
        let (width, height) = (world.map.width(), world.map.height());
        (0..width)
            .flat_map(|x| (0..height).map(move |y| Vec2::new(x as f32 + 0.5, y as f32 + 0.5)))
            .filter(|point| {
                placement_cell(world, archetype, point.x, point.y).is_some_and(|cell| {
                    matches!(
                        world.preview_site(LOCAL_PLAYER, structure, cell.0, cell.1),
                        Err(SiteRefusal::CliffLevelMismatch | SiteRefusal::UnbuildableTerrain)
                    )
                })
            })
            .min_by(|a, b| a.distance(origin).total_cmp(&b.distance(origin)))
            .expect("the skirmish map has cliffs")
    };
    arm_placement(&mut harness, structure);
    harness.look_at(spot);

    let on_screen = harness.ground_on_screen(spot);
    harness.move_cursor(on_screen);
    harness.step(1);

    assert!(
        matches!(preview(&harness).verdict, Some(Err(_))),
        "{:?}",
        preview(&harness).verdict
    );
    assert_ghost_is(&mut harness, false);
    let reason = preview(&harness)
        .refusal()
        .expect("a refused site says why");
    assert!(geometry(&harness).site_note.is_some(), "and that is drawn");

    harness.click(MouseButton::Left, on_screen);
    harness.step(1);

    assert_eq!(harness.pending_orders(), 0);
    assert_eq!(
        mode(&harness),
        CommandMode::Place(structure),
        "try elsewhere"
    );
    assert!(harness.status().contains(reason), "{}", harness.status());
}

#[test]
fn a_hidden_enemy_structure_does_not_turn_the_preview_red() {
    let mut harness = skirmish();
    let (_, _) = worker_and_site(&mut harness);
    let structure = slug(&harness, STRUCTURE);
    let enemy_base = harness
        .world()
        .entities
        .values()
        .find(|unit| unit.owner == ENEMY && unit.is_immobile)
        .expect("the enemy has a base")
        .id;
    let at = harness.simulated_position(enemy_base);
    arm_placement(&mut harness, structure);
    harness.look_at(at);

    let on_screen = harness.ground_on_screen(at);
    harness.move_cursor(on_screen);
    harness.step(1);

    assert_eq!(preview(&harness).verdict, Some(Ok(())));
    assert_ghost_is(&mut harness, true);
    assert_eq!(geometry(&harness).site_note, None);

    // Control: the player's own base, which the player knows of, is refused.
    let home = harness.local_structure();
    let at = harness.simulated_position(home);
    harness.look_at(at);
    let on_screen = harness.ground_on_screen(at);
    harness.move_cursor(on_screen);
    harness.step(1);
    assert_eq!(
        preview(&harness).verdict,
        Some(Err(SiteRefusal::KnownStructure))
    );
    assert_ghost_is(&mut harness, false);
}
