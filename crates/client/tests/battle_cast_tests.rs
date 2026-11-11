//! Targeted effect-tree casts played with real input (plan `2026-11-10-spellcaster-mechanics`
//! D-10, §8, work package 1e).
//!
//! # Responsibility
//! Boots the production client on the shipped skirmish, gives a scratch caster a point cast and a
//! unit cast, and casts them with the keys and clicks a player would use. Asserts on the card
//! entry, the command captured, the simulation's effect, the locks (research, energy, cooldown),
//! Shift queueing, leaving the mode, and that a selection of two casters casts once.
//!
//! # Limits
//! Shares the limits of `support/battle_harness.rs`: no window and no renderer. Cast entries are
//! drawn by the generic slot code, which the HUD window GPU tests render. The targeting ring is
//! not drawn yet (D-11, Q0080), so only its radius is checked.
//!
//! # Invariants
//! - No shipped ability uses the effect tree yet, so the abilities are scratch entries added to
//!   a shipped archetype picked by capability (presentable, mobile, few abilities), and the
//!   spell tables are a scratch `SpellCatalog` installed in the session's world. The HUD's
//!   command catalog is rebuilt from that world, as a new session would. Every order under test
//!   arrives through an input event.
//! - No unit, ability or spell of the shipped catalog is named.
//! - Every test fails on any simulation diagnostic.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use bevy::prelude::*;
use client::battle_hud::card::{CardLabel, CardLock, CardSlot, CommandCard};
use client::battle_hud::catalog::CommandCatalog;
use client::battle_hud::model::HudModel;
use client::battle_input::command_mode::CommandMode;
use client::battle_pipeline::session::BattleSession;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::ClientMode;
use sim_core::catalog::SpellCatalog;
use sim_core::{UpgradeCatalog, UpgradeDef};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, EntityId, PlayerCommand, PlayerId,
    ShapeKind, SimScalar, UnitOrder, UnitTypeId, UpgradeEffect,
};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{Harness, LOCAL_PLAYER};

const ENEMY: PlayerId = PlayerId(1);

/// Names of the scratch abilities; only these strings are compared, never shipped ones.
const ZONE: &str = "Scratch zone";
const BOLT: &str = "Scratch bolt";

/// The scratch spell tables: `zone` hurts every unit within 3 of a point, `bolt` one unit. The
/// damage is far beyond any shipped life, so a hit is a kill.
const EFFECTS: &str = r#"(effects: [
    (name: "zone", node: Search((radius: 3.0, child: Damage((amount: 100000.0))))),
    (name: "bolt", node: Damage((amount: 100000.0))),
])"#;

const STATUSES: &str = "(statuses: [])";

/// Energy one cast costs, and ticks it cools down.
const COST: i32 = 50;
const COOLDOWN: u16 = 100;

/// Range of both casts, in world units.
const RANGE: i32 = 6;

/// Ticks within which a cast in range, or after a short walk, lands.
const CAST_TICKS: u32 = 120;

fn scratch(name: &str, kind: AbilityTargetKind, root: &str) -> AbilitySpec {
    AbilitySpec {
        name: name.to_owned(),
        target_kind: kind,
        energy_cost: SimScalar::from_i32(COST),
        cooldown_ticks: COOLDOWN,
        cast_range: SimScalar::from_i32(RANGE),
        effect: AbilityEffectSpec::Effect(root.to_owned()),
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }
}

/// The skirmish with the camera on the local player's first worker.
fn skirmish() -> Harness {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = worker(&harness);
    let position = harness.simulated_position(worker);
    harness.look_at(position);
    harness
}

/// The local player's first worker, whose position is open ground in view.
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

fn presentable(harness: &Harness, unit_type: UnitTypeId) -> bool {
    harness
        .app
        .world()
        .resource::<HudModel>()
        .catalog
        .archetype(unit_type)
        .is_some_and(|commands| commands.unpresentable.is_none())
}

/// The scratch caster: a presentable mobile ground archetype with at most two abilities, so the
/// two scratch abilities get indices that carry a cooldown. Returns it with the indices of the
/// zone and the bolt.
fn install_caster(harness: &mut Harness) -> (UnitTypeId, u8, u8) {
    let caster = harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            !matches!(archetype.shape, ShapeKind::BuildingDome { .. })
                && archetype.max_speed > SimScalar::ZERO
                && archetype.abilities.len() <= 2
                && !archetype.capabilities.can_construct
                && presentable(harness, archetype.unit_type_id)
        })
        .expect("the catalog ships a presentable mobile unit with few abilities")
        .unit_type_id;
    let spells = SpellCatalog::from_ron_strs(STATUSES, EFFECTS).expect("valid scratch tables");
    let world = harness.world_mut();
    world
        .catalog
        .set_spell_catalog(spells)
        .expect("the shipped abilities name no effect");
    let archetype = world.catalog.get_mut(caster).unwrap();
    archetype.max_energy = SimScalar::from_i32(200);
    archetype.energy_regen_per_tick = SimScalar::ZERO;
    archetype
        .abilities
        .push(scratch(ZONE, AbilityTargetKind::TargetPosition, "zone"));
    archetype
        .abilities
        .push(scratch(BOLT, AbilityTargetKind::TargetEntity, "bolt"));
    let bolt = u8::try_from(archetype.abilities.len() - 1).unwrap();
    rebuild_hud_catalog(harness);
    (caster, bolt - 1, bolt)
}

/// Rebuilds the HUD's command catalog from the edited world, as a new session would, keeping
/// each identity's media verdict.
fn rebuild_hud_catalog(harness: &mut Harness) {
    let app = harness.app.world();
    let world = app.resource::<BattleSession>().world().unwrap();
    let old = &app.resource::<HudModel>().catalog;
    let catalog = CommandCatalog::build(&world.catalog, &world.upgrade_catalog, |unit_type| {
        old.archetype(unit_type)
            .and_then(|commands| commands.unpresentable.clone())
    });
    harness.app.world_mut().resource_mut::<HudModel>().catalog = catalog;
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
    let session = harness.app.world().resource::<BattleSession>();
    assert_eq!(session.diagnostics, [], "simulation diagnostics");
}

/// A caster of `unit_type` for the local player, `distance` from its first worker, with
/// `energy`.
fn spawn_caster(
    harness: &mut Harness,
    unit_type: UnitTypeId,
    distance: f32,
    energy: i32,
) -> EntityId {
    let near = harness.simulated_position(worker(harness));
    let at = harness.open_ground(near, distance);
    let unit = harness
        .world_mut()
        .spawn_unit(unit_type, LOCAL_PLAYER, battle_harness::sim_position(at))
        .expect("open ground");
    harness.world_mut().entities.get_mut(&unit).unwrap().energy = SimScalar::from_i32(energy);
    run_ticks(harness, 1);
    unit
}

/// An enemy structure about `distance` world units from `unit`: the smallest presentable one,
/// which neither fights back nor walks away.
fn enemy_structure(harness: &mut Harness, unit: EntityId, distance: f32) -> EntityId {
    let structure = harness
        .world()
        .catalog
        .archetypes()
        .filter(|archetype| {
            matches!(archetype.shape, ShapeKind::BuildingDome { .. })
                && archetype.weapon.is_none()
                && archetype.ground_weapon.is_none()
                && archetype.air_weapon.is_none()
                && presentable(harness, archetype.unit_type_id)
        })
        .min_by_key(|archetype| match archetype.shape {
            ShapeKind::BuildingDome {
                footprint_w,
                footprint_h,
            } => u32::from(footprint_w) * u32::from(footprint_h),
            ShapeKind::UnitSphere => u32::MAX,
        })
        .expect("the catalog ships a presentable unarmed structure")
        .unit_type_id;
    let near = harness.simulated_position(unit);
    let enemy = (0..16)
        .find_map(|step| {
            let at = near + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 16.0) * distance;
            let (x, y) = harness
                .world()
                .map
                .world_to_cell(battle_harness::sim_position(at))
                .ok()?;
            harness
                .world_mut()
                .spawn_structure(structure, ENEMY, x, y)
                .ok()
        })
        .expect("open ground for a structure near the unit");
    run_ticks(harness, 1);
    enemy
}

fn alive(harness: &Harness, id: EntityId) -> bool {
    harness
        .world()
        .entities
        .get(&id)
        .is_some_and(sim_core::Entity::is_alive)
}

fn card(harness: &Harness) -> &CommandCard {
    harness.app.world().resource::<CommandCard>()
}

fn mode(harness: &Harness) -> CommandMode {
    *harness.app.world().resource::<CommandMode>()
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

/// The position and slot of the cast entry labelled `name`, if the card shows one.
fn cast_slot(harness: &Harness, name: &str) -> Option<(usize, CardSlot)> {
    let card = card(harness);
    card.slots
        .iter()
        .zip(&card.text)
        .enumerate()
        .find_map(|(position, (slot, text))| {
            let slot = slot.as_ref()?;
            (matches!(slot.label, CardLabel::Cast(_)) && text.caption.ends_with(name))
                .then(|| (position, slot.clone()))
        })
}

/// Taps the key of the cast entry labelled `name`.
fn tap_cast(harness: &mut Harness, name: &str) {
    harness.step(1);
    let (_, slot) = cast_slot(harness, name)
        .unwrap_or_else(|| panic!("no {name} entry: {:?}", card(harness).slots));
    harness.tap_key(slot.key);
}

fn energy(harness: &Harness, id: EntityId) -> SimScalar {
    harness.world().entities[&id].energy
}

#[test]
fn a_point_cast_is_aimed_by_key_and_click_and_lands_after_a_walk() {
    let mut harness = skirmish();
    let (caster_type, zone, _) = install_caster(&mut harness);
    let caster = spawn_caster(&mut harness, caster_type, 4.0, 100);
    // Beyond the cast range: the click is accepted and the caster walks.
    let enemy = enemy_structure(&mut harness, caster, 9.0);
    harness.select(caster);

    let (_, slot) = cast_slot(&harness, ZONE).expect("the card offers the point cast");
    assert!(slot.enabled, "{:?}", slot.lock);
    tap_cast(&mut harness, ZONE);
    let armed = mode(&harness);
    assert!(matches!(armed, CommandMode::CastAtPoint(_)), "{armed:?}");
    let radius = armed.targeting_radius(harness.world());
    assert_eq!(
        radius,
        Some(SimScalar::from_i32(3)),
        "the zone's search radius"
    );

    let target = harness.simulated_position(enemy);
    let on_screen = harness.ground_on_screen(target);
    harness.click(MouseButton::Left, on_screen);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [caster]);
    assert!(!orders[0].queue);
    let UnitOrder::CastAbility {
        ability_index,
        target_pos: Some(at),
        target_entity: None,
    } = orders[0].order
    else {
        panic!("a point cast: {:?}", orders[0].order);
    };
    assert_eq!(ability_index, zone);
    let at = Vec2::new(at.x.to_f32(), at.y.to_f32());
    assert!(at.distance(target) < 0.5, "aimed at {at}, clicked {target}");
    assert_eq!(
        mode(&harness),
        CommandMode::Normal,
        "an accepted cast ends the mode"
    );

    run_ticks(&mut harness, CAST_TICKS);
    assert!(!alive(&harness, enemy), "the zone landed on the structure");
    assert_eq!(energy(&harness, caster), SimScalar::from_i32(100 - COST));
}

#[test]
fn a_unit_cast_takes_the_clicked_unit() {
    let mut harness = skirmish();
    let (caster_type, _, bolt) = install_caster(&mut harness);
    let caster = spawn_caster(&mut harness, caster_type, 4.0, 100);
    let enemy = enemy_structure(&mut harness, caster, 4.0);
    harness.select(caster);

    tap_cast(&mut harness, BOLT);
    assert!(matches!(mode(&harness), CommandMode::CastOnUnit(_)));
    // A click on bare ground finds no unit: refused, and the mode stays armed.
    let bare = harness.open_ground(harness.simulated_position(caster), 2.0);
    let bare = harness.ground_on_screen(bare);
    harness.click(MouseButton::Left, bare);
    assert_eq!(pending(&harness), []);
    assert!(
        harness.status().contains("no unit there"),
        "{}",
        harness.status()
    );
    assert!(matches!(mode(&harness), CommandMode::CastOnUnit(_)));

    let on_screen = harness.unit_on_screen(enemy);
    harness.click(MouseButton::Left, on_screen);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(
        orders[0].order,
        UnitOrder::CastAbility {
            ability_index: bolt,
            target_pos: None,
            target_entity: Some(enemy),
        }
    );
    run_ticks(&mut harness, CAST_TICKS);
    assert!(!alive(&harness, enemy), "the bolt landed");
}

#[test]
fn right_click_and_the_cancel_key_leave_a_cast_mode() {
    let mut harness = skirmish();
    let (caster_type, _, _) = install_caster(&mut harness);
    let caster = spawn_caster(&mut harness, caster_type, 4.0, 100);
    harness.select(caster);

    tap_cast(&mut harness, ZONE);
    assert!(matches!(mode(&harness), CommandMode::CastAtPoint(_)));
    let here = harness.unit_on_screen(caster);
    harness.click(MouseButton::Right, here);
    assert_eq!(mode(&harness), CommandMode::Normal);

    tap_cast(&mut harness, ZONE);
    harness.tap_key(client::input_bindings::CANCEL_KEY);
    assert_eq!(mode(&harness), CommandMode::Normal);
    assert_eq!(pending(&harness), []);
}

#[test]
fn energy_cooldown_and_research_lock_the_entry() {
    let mut harness = skirmish();
    let (caster_type, zone, _) = install_caster(&mut harness);
    let caster = spawn_caster(&mut harness, caster_type, 4.0, COST - 1);
    harness.select(caster);
    harness.step(1);
    let lock_of = |harness: &Harness| cast_slot(harness, ZONE).expect("shown, not hidden").1;

    let slot = lock_of(&harness);
    assert_eq!(slot.lock, Some(CardLock::NeedsEnergy));
    assert!(!slot.enabled);
    tap_cast(&mut harness, ZONE);
    assert_eq!(
        mode(&harness),
        CommandMode::Normal,
        "a locked entry arms nothing"
    );
    assert!(
        harness.status().contains("Not enough energy"),
        "{}",
        harness.status()
    );

    let entity = harness.world_mut().entities.get_mut(&caster).unwrap();
    entity.energy = SimScalar::from_i32(100);
    entity.ability_cooldowns[usize::from(zone)] = 2 * COOLDOWN;
    run_ticks(&mut harness, 1);
    harness.step(1);
    assert_eq!(lock_of(&harness).lock, Some(CardLock::Cooldown));

    harness
        .world_mut()
        .entities
        .get_mut(&caster)
        .unwrap()
        .ability_cooldowns[usize::from(zone)] = 0;
    let slug = harness
        .world()
        .catalog
        .get(caster_type)
        .unwrap()
        .slug
        .clone();
    let mut definitions: Vec<UpgradeDef> = harness
        .world()
        .upgrade_catalog
        .upgrades()
        .cloned()
        .collect();
    definitions[0].effects.push(UpgradeEffect::UnlockAbility {
        target_slug: slug,
        ability_slug: ZONE.to_owned(),
    });
    let upgrade = definitions[0].upgrade_id;
    harness.world_mut().upgrade_catalog =
        UpgradeCatalog::from_definitions(definitions).expect("a valid catalog");
    rebuild_hud_catalog(&mut harness);
    run_ticks(&mut harness, 1);
    harness.step(1);
    assert_eq!(
        lock_of(&harness).lock,
        Some(CardLock::NeedsResearch(Some(upgrade)))
    );
    let bolt = cast_slot(&harness, BOLT)
        .expect("the bolt is still offered")
        .1;
    assert!(bolt.enabled, "only the zone needs the research");
}

#[test]
fn shift_queues_a_cast_behind_a_walk_and_keeps_the_mode() {
    let mut harness = skirmish();
    let (caster_type, zone, _) = install_caster(&mut harness);
    let caster = spawn_caster(&mut harness, caster_type, 4.0, 200);
    harness.select(caster);

    // A walk first, by right click; the cast is queued behind it, at bare ground.
    let start = harness.simulated_position(caster);
    let walk_to = harness.open_ground(start, 5.0);
    let walk_on_screen = harness.ground_on_screen(walk_to);
    harness.click(MouseButton::Right, walk_on_screen);
    run_ticks(&mut harness, 1);
    let target = harness.open_ground(walk_to, 3.0);
    let on_screen = harness.ground_on_screen(target);
    harness.with_key_held(KeyCode::ShiftLeft, |harness| {
        tap_cast(harness, ZONE);
        harness.click(MouseButton::Left, on_screen);
    });
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(orders[0].queue, "Shift queues");
    assert!(
        matches!(orders[0].order, UnitOrder::CastAbility { ability_index, .. } if ability_index == zone)
    );
    assert!(
        matches!(mode(&harness), CommandMode::CastAtPoint(_)),
        "Shift keeps the mode for the next target"
    );
    run_ticks(&mut harness, 1);
    let queue = &harness.world().entities[&caster].order_queue;
    assert!(
        matches!(queue.front(), Some(UnitOrder::Move { .. }))
            && matches!(queue.back(), Some(UnitOrder::CastAbility { .. })),
        "the cast waits behind the walk: {queue:?}"
    );
    run_ticks(&mut harness, CAST_TICKS);
    assert_eq!(
        energy(&harness, caster),
        SimScalar::from_i32(200 - COST),
        "the queued cast ran after the walk: {}",
        harness.status()
    );
}

#[test]
fn a_selection_of_two_casters_casts_once() {
    let mut harness = skirmish();
    let (caster_type, zone, _) = install_caster(&mut harness);
    let first = spawn_caster(&mut harness, caster_type, 4.0, 100);
    let second = spawn_caster(&mut harness, caster_type, 6.0, 100);
    let enemy = enemy_structure(&mut harness, first, 4.0);
    harness.select(first);
    let other = harness.unit_on_screen(second);
    harness.with_key_held(KeyCode::ShiftLeft, |harness| {
        harness.click(MouseButton::Left, other);
    });
    assert_eq!(harness.selection().ids().len(), 2);

    tap_cast(&mut harness, ZONE);
    let target = harness.simulated_position(enemy);
    let on_screen = harness.ground_on_screen(target);
    harness.click(MouseButton::Left, on_screen);
    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "one command for one ability index");
    let mut entities = orders[0].entities.clone();
    entities.sort_unstable();
    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(entities, expected, "the whole capable selection is sent");
    assert!(
        matches!(orders[0].order, UnitOrder::CastAbility { ability_index, .. } if ability_index == zone)
    );

    run_ticks(&mut harness, CAST_TICKS);
    assert!(!alive(&harness, enemy));
    let spent = [first, second]
        .iter()
        .filter(|id| energy(&harness, **id) < SimScalar::from_i32(100))
        .count();
    assert_eq!(spent, 1, "the simulation picked one caster");
}
