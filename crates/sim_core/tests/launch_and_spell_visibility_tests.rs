//! A missile's launch and a spell's damage show their caster (B-205, B-206).
//!
//! # Responsibility
//! Pins plan `2026-11-27-launch-and-spell-visibility` V-1 to V-16 through the public interface
//! (`SimWorld`, `PlayerCommand`, `run_effect`, `write_player_view`). In `StarCraft` II 5.0.14
//! `Visibility` is a field of `CEffectLaunchMissile` and of `CEffectDamage`, both `Snapshot` by
//! default (`core` EffectData:62, :102): a weapon missile's release shows its attacker to the
//! player whose unit it is aimed at, a flight time before the impact does, and a spell's damage
//! and its unit-aimed missile show the caster as a weapon's strike shows the attacker. The image
//! is the attack snapshot of plan `2026-11-24-attacker-snapshot`, with its concealment rule and
//! its cap; a `Visible` cause reveals the caster through the same counter a weapon's does.
//!
//! # How it fits
//! Combat records the launch's image when it pushes the projectile (phase 7); the effect engine
//! records a spell's image when a `Damage` node runs and when a `LaunchMissile` node aimed at a
//! unit launches. The vision phase drops images, and `PlayerView::attack_snapshots` publishes
//! them; the client draws them with the snapshot pool.
//!
//! # Invariants
//! - No unit or spell is named: archetypes are found by property (a missile weapon, an ability
//!   whose effect tree holds a `Damage` or `LaunchMissile`) and copied under fresh identities
//!   (charter §3.3).
//! - Every map is built through `sim_map`: a plateau with no ramp, so the low ground never sees
//!   up and a ground unit never climbs it.
//! - Unless a test is about the cap, it lifts the cap (`UnitCatalog::set_attack_snapshot_cap`),
//!   so only sight ends an image there.
//! - A spell is started with `SimWorld::run_effect`, the walk a cast starts, between two ticks:
//!   its test reads the vision of the tick before, as a cast committed in phase 1 does.

#![allow(clippy::expect_used, clippy::too_many_lines)]

#[path = "support/scratch_storm.rs"]
mod scratch_storm;

use scratch_storm::ScratchStorm;
use sim_core::catalog::{EffectNode, SpellCatalog};
use sim_core::{SimWorld, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySource, AttackVisibility, EffectAnchor, EffectContext, EffectId,
    EffectTarget, EntityId, MoverType, PlayerCommand, PlayerId, PlayerView, ShapeKind, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId, VisibilityState, TICKS_PER_SECOND,
};
use std::collections::BTreeSet;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Map width in cells.
const WIDTH: u32 = 64;
/// Map height in cells.
const HEIGHT: u32 = 32;
/// The plateau's first column: every cell with x at or above it is one cliff level up.
const PLATEAU: u32 = 16;
/// Row of the attacker's and the victim's centres.
const ROW: f64 = 16.5;
/// Ticks every wait below is allowed.
const LIMIT: u32 = 600;
/// The shipped cap of an image, 1 second (owner answer Q0112 2(c)).
const CAP: u32 = TICKS_PER_SECOND as u32;
/// The reveal of a `Visible` scratch value, the shipped air-plane weapons' (`AttackReveal`,
/// 3 Normal s, B-55).
const REVEAL: u16 = 54;

/// A map whose cells with x >= [`PLATEAU`] stand one cliff level up, with no ramp.
fn plateau_map() -> Result<GameMap, sim_map::MapError> {
    let mut map = GameMap::new(WIDTH, HEIGHT, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..HEIGHT {
        for x in PLATEAU..WIDTH {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
        }
    }
    Ok(map)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

/// The plateau's edge on the attacker's row.
fn edge() -> SimVec2 {
    at(f64::from(PLATEAU) + 0.5, ROW)
}

fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

fn step(world: &mut SimWorld) {
    step_with(world, Vec::new());
}

fn order(player: PlayerId, unit: EntityId, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities: vec![unit],
        order,
        queue: false,
    }
}

fn view_of(world: &SimWorld, viewer: u8) -> PlayerView {
    let mut view = PlayerView::default();
    world.write_player_view(PlayerId(viewer), &mut view);
    view
}

/// The images `viewer` is listed: each attacker with the tick of the effect that made it.
fn images(world: &SimWorld, viewer: u8) -> Vec<(EntityId, Tick)> {
    view_of(world, viewer)
        .attack_snapshots
        .iter()
        .map(|entry| (entry.entity, entry.struck))
        .collect()
}

/// Whether `view` has the cell of `world`'s map under `position` `Visible`.
fn sees(world: &SimWorld, view: &PlayerView, position: SimVec2) -> bool {
    world.map.world_to_cell(position).is_ok_and(|(x, y)| {
        let index = y as usize * view.map_width as usize + x as usize;
        view.visibility.get(index) == Some(&VisibilityState::Visible)
    })
}

/// Registers a copy of `arch` with `weapon` as its only weapon and no detection, under the next
/// free identity.
fn register(world: &mut SimWorld, arch: &UnitArchetype, weapon: Option<WeaponSpec>) -> UnitTypeId {
    let id = UnitTypeId(
        world
            .catalog
            .archetypes()
            .map(|a| a.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    let mut copy = arch.clone();
    copy.unit_type_id = id;
    format!("scratch_visibility_{}", id.0).clone_into(&mut copy.slug);
    copy.weapon.clone_from(&weapon);
    copy.ground_weapon.clone_from(&weapon);
    copy.air_weapon = weapon.filter(|_| arch.air_weapon.is_some());
    copy.detection_radius = None;
    world.catalog.register(copy);
    id
}

/// Gives `id` health enough for every test.
fn toughen(world: &mut SimWorld, id: EntityId) -> Result<(), String> {
    let entity = world.entities.get_mut(&id).ok_or("spawned")?;
    entity.max_hp = SimScalar::from_i32(100_000);
    entity.hp = entity.max_hp;
    Ok(())
}

/// Orders `id` to hold, so it neither walks nor flees.
fn hold(world: &mut SimWorld, id: EntityId) -> Result<(), String> {
    world
        .entities
        .get_mut(&id)
        .ok_or("spawned")?
        .issue_order(UnitOrder::HoldPosition, false)
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
}

fn vitals(world: &SimWorld, id: EntityId) -> Option<(SimScalar, SimScalar)> {
    world.entities.get(&id).map(|e| (e.hp, e.shields))
}

/// Life plus shields of `id`, 0 once it is gone: a fall is a hit, a rise regeneration.
fn health(world: &SimWorld, id: EntityId) -> SimScalar {
    world
        .entities
        .get(&id)
        .map_or(SimScalar::ZERO, |e| e.hp + e.shields)
}

/// Whether `attacker` is attack-revealed to player `viewer`.
fn revealed_to(world: &SimWorld, attacker: EntityId, viewer: u8) -> bool {
    world
        .entities
        .get(&attacker)
        .is_some_and(|e| e.is_attack_revealed_to(PlayerId(viewer)))
}

/// A shipped mobile ground archetype, not a detector and without modes, whose weapon against
/// ground launches a missile, keeps the default `Snapshot` damage, strikes one target and
/// reaches at least 4.
fn ground_missile(world: &SimWorld) -> Option<(UnitArchetype, WeaponSpec)> {
    world
        .catalog
        .archetypes()
        .filter(|arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
                && arch.modes.is_empty()
                && matches!(arch.shape, ShapeKind::UnitSphere)
        })
        .find_map(|arch| {
            arch.weapon_against(MoverType::Ground, None)
                .filter(|w| {
                    w.projectile_speed.is_some()
                        && w.attack_visibility == AttackVisibility::Snapshot
                        && w.splash.is_none()
                        && w.max_range >= SimScalar::from_i32(4)
                })
                .map(|w| (arch.clone(), w.clone()))
        })
}

/// A shipped mobile ground archetype, not a detector and without modes, whose weapon against
/// ground strikes at once, keeps the default `Snapshot` damage and reaches at least 4.
fn ground_instant(world: &SimWorld) -> Option<(UnitArchetype, WeaponSpec)> {
    world
        .catalog
        .archetypes()
        .filter(|arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
                && arch.modes.is_empty()
                && matches!(arch.shape, ShapeKind::UnitSphere)
        })
        .find_map(|arch| {
            arch.weapon_against(MoverType::Ground, None)
                .filter(|w| {
                    w.projectile_speed.is_none()
                        && w.attack_visibility == AttackVisibility::Snapshot
                        && w.splash.is_none()
                        && w.max_range >= SimScalar::from_i32(4)
                })
                .map(|w| (arch.clone(), w.clone()))
        })
}

/// A shipped mobile air archetype whose weapon against ground launches a missile whose damage
/// reveals its attacker (an air-plane attacker, `Visible`).
fn air_missile(world: &SimWorld) -> Option<(UnitArchetype, WeaponSpec)> {
    world
        .catalog
        .archetypes()
        .filter(|arch| arch.mover_type == MoverType::Air && arch.max_speed > SimScalar::ZERO)
        .find_map(|arch| {
            arch.weapon_against(MoverType::Ground, None)
                .filter(|w| {
                    w.projectile_speed.is_some()
                        && matches!(w.attack_visibility, AttackVisibility::Visible { .. })
                        && w.splash.is_none()
                })
                .map(|w| (arch.clone(), w.clone()))
        })
}

/// A shipped detector structure, as an unarmed copy under the next free identity.
fn unarmed_detector(world: &mut SimWorld) -> Option<(UnitTypeId, UnitArchetype)> {
    let arch = world
        .catalog
        .archetypes()
        .find(|arch| arch.is_detector() && matches!(arch.shape, ShapeKind::BuildingDome { .. }))?
        .clone();
    let id = UnitTypeId(
        world
            .catalog
            .archetypes()
            .map(|a| a.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    let mut copy = arch.clone();
    copy.unit_type_id = id;
    "scratch_visibility_detector".clone_into(&mut copy.slug);
    copy.weapon = None;
    copy.ground_weapon = None;
    copy.air_weapon = None;
    world.catalog.register(copy);
    Some((id, arch))
}

/// Spawns a detector structure of player 1 below the plateau whose circle holds the plateau's
/// edge on the attacker's row; returns it.
fn detector_below(world: &mut SimWorld) -> Result<EntityId, Box<dyn std::error::Error>> {
    let (detector_type, detector_arch) =
        unarmed_detector(world).ok_or("a shipped detector structure")?;
    let ShapeKind::BuildingDome { footprint_w, .. } = detector_arch.shape else {
        return Err("a structure has a dome".into());
    };
    let detector = world.spawn_structure(
        detector_type,
        PlayerId(1),
        PLATEAU - u32::from(footprint_w),
        19,
    )?;
    let detector_at = world.entities[&detector].position;
    assert!(
        detector_at.distance(edge())
            < detector_arch.detection_radius.ok_or("a detector")? - SimScalar::ONE,
        "premise: the plateau's edge is within the detection radius"
    );
    Ok(detector)
}

// ---------------------------------------------------------------------------------------------
// Weapon launches (V-1 to V-9)
// ---------------------------------------------------------------------------------------------

/// The duel: an attacker of player 0 on the plateau's edge and a victim of player 1 below it,
/// well inside each other's weapon reach, the victim holding and tough, and the attacker ordered
/// to attack it.
struct Duel {
    world: SimWorld,
    attacker: EntityId,
    victim: EntityId,
    /// The attacker's shipped archetype, for unarmed copies of it.
    arch: UnitArchetype,
}

/// The weapon of the duel: the ground missile, or an instant weapon.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Delivery {
    Missile,
    Instant,
}

/// Builds the duel on the shipped catalogs, the attacker's weapon of `delivery` changed by
/// `edit`, its missile slowed to 0.5 a tick so a flight lasts several ticks, and the cap lifted.
fn duel(
    delivery: Delivery,
    edit: impl FnOnce(&mut WeaponSpec),
) -> Result<Duel, Box<dyn std::error::Error>> {
    let mut world = sim_fixtures::try_world(plateau_map()?, 0x1a7)?;
    world.catalog.set_attack_snapshot_cap(None);
    let (arch, mut weapon) = match delivery {
        Delivery::Missile => ground_missile(&world).ok_or("a shipped ground missile")?,
        Delivery::Instant => ground_instant(&world).ok_or("a shipped instant ground weapon")?,
    };
    if delivery == Delivery::Missile {
        weapon.projectile_speed = Some(SimScalar::HALF);
    }
    edit(&mut weapon);
    let reach = weapon.max_range + arch.collision_radius * SimScalar::from_i32(2)
        - SimScalar::from_num(1.5);
    let attacker_type = register(&mut world, &arch, Some(weapon));
    let attacker = world.spawn_unit(attacker_type, PlayerId(0), edge())?;
    let victim = world.spawn_unit(
        arch.unit_type_id,
        PlayerId(1),
        SimVec2::new(edge().x - reach, SimScalar::from_num(ROW)),
    )?;
    toughen(&mut world, victim)?;
    hold(&mut world, victim)?;
    world
        .entities
        .get_mut(&attacker)
        .ok_or("spawned")?
        .issue_order(UnitOrder::attack(victim), false)
        .map_err(|error| format!("{error:?}"))?;
    Ok(Duel {
        world,
        attacker,
        victim,
        arch,
    })
}

impl Duel {
    /// The ids of the attacker's missiles in flight.
    fn missiles(&self) -> BTreeSet<u32> {
        self.world
            .projectiles
            .iter()
            .filter(|p| p.attacker_id == Some(self.attacker))
            .map(|p| p.id)
            .collect()
    }

    /// Steps until the attacker releases a new missile; returns that tick.
    fn until_released(&mut self) -> Result<u32, String> {
        let before = self.missiles();
        for _ in 0..LIMIT {
            step(&mut self.world);
            if !self.missiles().is_subset(&before) {
                return Ok(self.world.current_tick);
            }
        }
        Err(format!("a missile was released within {LIMIT} ticks"))
    }

    /// Steps until `id` is next struck; returns that tick.
    fn until_struck(&mut self, id: EntityId) -> Result<u32, String> {
        let full = vitals(&self.world, id);
        for _ in 0..LIMIT {
            step(&mut self.world);
            if vitals(&self.world, id) != full {
                return Ok(self.world.current_tick);
            }
        }
        Err(format!("{id:?} was struck within {LIMIT} ticks"))
    }

    /// Spawns an unarmed copy of the attacker archetype for `owner` at `position`, holding.
    fn spotter(
        &mut self,
        owner: u8,
        position: SimVec2,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let arch = self.arch.clone();
        let kind = register(&mut self.world, &arch, None);
        let id = self.world.spawn_unit(kind, PlayerId(owner), position)?;
        hold(&mut self.world, id)?;
        Ok(id)
    }

    fn cloak_attacker(&mut self) -> TestResult {
        self.world
            .entities
            .get_mut(&self.attacker)
            .ok_or("spawned")?
            .is_cloaked = true;
        Ok(())
    }
}

/// V-1 (D-2 caller 1): a missile released at a unit whose player does not see the attacker
/// lists the attacker to that player on the release tick, in its release pose, before the
/// victim is struck; the impact a flight later refreshes the image.
#[test]
fn a_missile_release_lists_its_attacker_before_the_impact() -> TestResult {
    let mut duel = duel(Delivery::Missile, |_| {})?;
    let untouched = vitals(&duel.world, duel.victim);
    assert!(
        images(&duel.world, 1).is_empty(),
        "nothing before the release"
    );
    let release = duel.until_released()?;
    assert_eq!(
        vitals(&duel.world, duel.victim),
        untouched,
        "not struck yet"
    );
    let view = view_of(&duel.world, 1);
    let [entry] = view.attack_snapshots.as_slice() else {
        return Err(format!("one image, got {:?}", view.attack_snapshots).into());
    };
    let attacker = &duel.world.entities[&duel.attacker];
    assert_eq!(entry.entity, duel.attacker);
    assert_eq!(entry.struck, Tick(release), "the release's tick");
    assert_eq!(
        [entry.bubble.world_pos[0], entry.bubble.world_pos[1]],
        [attacker.position.x.to_f32(), attacker.position.y.to_f32()],
        "where it released from"
    );
    assert_eq!(
        entry.bubble.heading_angle_rad,
        attacker.heading.to_radians_f32()
    );
    assert!(images(&duel.world, 0).is_empty() && images(&duel.world, 2).is_empty());
    let impact = duel.until_struck(duel.victim)?;
    assert!(
        impact >= release + 3,
        "premise: a flight of at least 3 ticks"
    );
    assert_eq!(
        images(&duel.world, 1),
        [(duel.attacker, Tick(impact))],
        "refreshed by the impact"
    );
    Ok(())
}

/// V-2: an instant weapon launches nothing; its first image comes with its strike.
#[test]
fn an_instant_weapon_lists_its_attacker_at_the_strike_only() -> TestResult {
    let mut duel = duel(Delivery::Instant, |_| {})?;
    let full = vitals(&duel.world, duel.victim);
    for _ in 0..LIMIT {
        step(&mut duel.world);
        if vitals(&duel.world, duel.victim) != full {
            break;
        }
        assert!(
            images(&duel.world, 1).is_empty(),
            "nothing before the strike"
        );
    }
    let struck = duel.world.current_tick;
    assert_eq!(images(&duel.world, 1), [(duel.attacker, Tick(struck))]);
    Ok(())
}

/// V-3: a player who sees the attacker's cell gets no image of its launch.
#[test]
fn a_launch_the_aimed_at_player_sees_records_nothing() -> TestResult {
    let mut duel = duel(Delivery::Missile, |_| {})?;
    duel.spotter(1, at(f64::from(PLATEAU) + 3.5, ROW + 2.0))?;
    step(&mut duel.world);
    assert!(
        sees(&duel.world, &view_of(&duel.world, 1), edge()),
        "premise: the spotter sees the attacker's cell"
    );
    duel.until_released()?;
    assert!(images(&duel.world, 1).is_empty(), "seen: no image");
    Ok(())
}

/// V-4: a missile whose target dies in flight still left its launch's image, which then ends by
/// the usual rules (here the shipped cap, counted from the release).
#[test]
fn a_lost_missiles_launch_image_stays_and_ends_by_the_cap() -> TestResult {
    let mut duel = duel(Delivery::Missile, |_| {})?;
    duel.world
        .catalog
        .set_attack_snapshot_cap(std::num::NonZeroU32::new(CAP));
    let release = duel.until_released()?;
    duel.world.entities.get_mut(&duel.victim).ok_or("alive")?.hp = SimScalar::ZERO;
    assert_eq!(images(&duel.world, 1), [(duel.attacker, Tick(release))]);
    while duel.world.current_tick < release + CAP - 1 {
        step(&mut duel.world);
        assert_eq!(
            images(&duel.world, 1),
            [(duel.attacker, Tick(release))],
            "tick {}",
            duel.world.current_tick
        );
    }
    step(&mut duel.world);
    assert!(images(&duel.world, 1).is_empty(), "gone at the cap");
    Ok(())
}

/// V-5: a `Hidden` launch (a scratch value) lists nothing at the release; the impact records as
/// before.
#[test]
fn a_hidden_launch_lists_nothing_until_the_impact() -> TestResult {
    let mut duel = duel(Delivery::Missile, |weapon| {
        weapon.launch_visibility = AttackVisibility::Hidden;
    })?;
    duel.until_released()?;
    assert!(
        images(&duel.world, 1).is_empty(),
        "Hidden: nothing at the release"
    );
    let impact = duel.until_struck(duel.victim)?;
    assert_eq!(images(&duel.world, 1), [(duel.attacker, Tick(impact))]);
    Ok(())
}

/// V-6: a `Visible` launch (a scratch value) reveals the attacker from the release tick, for the
/// reveal's ticks as a strike's would, and the victim acquires it on the next acquisition phase;
/// with the shipped value it never does.
#[test]
fn a_visible_launch_reveals_its_attacker_and_makes_it_a_target() -> TestResult {
    let mut visible = duel(Delivery::Missile, |weapon| {
        weapon.launch_visibility = AttackVisibility::Visible {
            reveal_ticks: REVEAL,
        };
    })?;
    let release = visible.until_released()?;
    assert_eq!(
        visible.world.entities[&visible.attacker]
            .attack_reveal_ticks
            .get(PlayerId(1)),
        Some(&REVEAL),
        "raised to the reveal on the release tick, as a strike raises it"
    );
    assert!(images(&visible.world, 1).is_empty(), "revealed: no image");
    assert!(view_of(&visible.world, 1)
        .attack_revealed
        .contains(&visible.attacker));
    assert_ne!(
        visible.world.entities[&visible.victim].current_target,
        Some(visible.attacker)
    );
    step(&mut visible.world);
    assert_eq!(visible.world.current_tick, release + 1);
    assert_eq!(
        visible.world.entities[&visible.victim].current_target,
        Some(visible.attacker),
        "acquired on the next acquisition phase"
    );
    let full = vitals(&visible.world, visible.attacker);
    let mut struck = false;
    for _ in 0..60 {
        step(&mut visible.world);
        struck |= vitals(&visible.world, visible.attacker) != full;
    }
    assert!(struck, "the victim fires at it");

    let mut shipped = duel(Delivery::Missile, |_| {})?;
    let release = shipped.until_released()?;
    while shipped.world.current_tick < release + 60 {
        step(&mut shipped.world);
        assert_ne!(
            shipped.world.entities[&shipped.victim].current_target,
            Some(shipped.attacker),
            "shipped: never a target"
        );
        assert!(!revealed_to(&shipped.world, shipped.attacker, 1));
    }
    Ok(())
}

/// V-7: a shipped missile weapon whose damage is `Visible` (an air-plane attacker) shows an image
/// from the release, then the reveal at the impact; no view lists the attacker both revealed or
/// in sight and as an image.
#[test]
fn an_air_plane_attacker_shows_an_image_then_a_reveal() -> TestResult {
    let mut world = sim_fixtures::try_world(plateau_map()?, 0x1a7)?;
    world.catalog.set_attack_snapshot_cap(None);
    let (arch, mut weapon) = air_missile(&world).ok_or("a shipped air-plane missile attacker")?;
    weapon.projectile_speed = Some(SimScalar::HALF);
    let (ground, _) = ground_missile(&world).ok_or("a shipped ground unit")?;
    let reach = weapon.max_range + arch.collision_radius + ground.collision_radius
        - SimScalar::from_num(1.5);
    let attacker_type = register(&mut world, &arch, Some(weapon));
    let victim_type = register(&mut world, &ground, None);
    let attacker = world.spawn_unit(attacker_type, PlayerId(0), edge())?;
    let victim = world.spawn_unit(
        victim_type,
        PlayerId(1),
        SimVec2::new(edge().x - reach, SimScalar::from_num(ROW)),
    )?;
    toughen(&mut world, victim)?;
    hold(&mut world, victim)?;
    world
        .entities
        .get_mut(&attacker)
        .ok_or("spawned")?
        .issue_order(UnitOrder::attack(victim), false)
        .map_err(|error| format!("{error:?}"))?;
    let mut duel = Duel {
        world,
        attacker,
        victim,
        arch,
    };
    let release = duel.until_released()?;
    assert_eq!(
        images(&duel.world, 1),
        [(attacker, Tick(release))],
        "the launch's image"
    );
    assert!(
        !revealed_to(&duel.world, attacker, 1),
        "not revealed before the impact"
    );
    let full = vitals(&duel.world, victim);
    let mut impact = None;
    for _ in 0..LIMIT {
        step(&mut duel.world);
        let view = view_of(&duel.world, 1);
        let position = duel.world.entities[&attacker].position;
        if view.attack_revealed.contains(&attacker) || sees(&duel.world, &view, position) {
            assert!(
                !view
                    .attack_snapshots
                    .iter()
                    .any(|entry| entry.entity == attacker),
                "tick {}: never both the unit and its image",
                duel.world.current_tick
            );
        }
        if impact.is_none() && vitals(&duel.world, victim) != full {
            impact = Some(duel.world.current_tick);
        }
        if impact.is_some_and(|tick| duel.world.current_tick > tick + 3) {
            break;
        }
    }
    let impact = impact.ok_or("the missile landed")?;
    assert!(impact > release, "premise: the missile flew");
    assert!(
        revealed_to(&duel.world, attacker, 1),
        "the impact reveals it"
    );
    Ok(())
}

/// V-8: an undetected cloaked attacker leaves no image at its launch; inside the aimed-at player's
/// detection circle, out of its sight, it leaves one.
#[test]
fn a_concealed_launch_shows_an_image_only_where_detected() -> TestResult {
    let mut undetected = duel(Delivery::Missile, |_| {})?;
    undetected.cloak_attacker()?;
    undetected.until_released()?;
    assert!(
        images(&undetected.world, 1).is_empty(),
        "undetected: nothing"
    );

    let mut detected = duel(Delivery::Missile, |_| {})?;
    detected.cloak_attacker()?;
    detector_below(&mut detected.world)?;
    let release = detected.until_released()?;
    let view = view_of(&detected.world, 1);
    assert!(
        !sees(&detected.world, &view, edge()),
        "premise: out of sight"
    );
    assert_eq!(
        images(&detected.world, 1),
        [(detected.attacker, Tick(release))]
    );
    Ok(())
}

/// V-9 (Q0128 3(a)): under the shipped cap, a launch image no later strike refreshes ends
/// exactly the cap after the release; a flight longer than the cap lets the image lapse, and the
/// impact makes it again.
#[test]
fn the_cap_runs_from_the_launch() -> TestResult {
    let mut alone = duel(Delivery::Missile, |weapon| {
        weapon.attack_visibility = AttackVisibility::Hidden;
    })?;
    alone
        .world
        .catalog
        .set_attack_snapshot_cap(std::num::NonZeroU32::new(CAP));
    let release = alone.until_released()?;
    let far = at(f64::from(WIDTH) - 4.5, ROW);
    step_with(
        &mut alone.world,
        vec![order(
            PlayerId(0),
            alone.attacker,
            UnitOrder::Move { target: far },
        )],
    );
    while alone.world.current_tick < release + CAP - 1 {
        assert_eq!(images(&alone.world, 1), [(alone.attacker, Tick(release))]);
        step(&mut alone.world);
    }
    assert_eq!(images(&alone.world, 1), [(alone.attacker, Tick(release))]);
    step(&mut alone.world);
    assert!(
        images(&alone.world, 1).is_empty(),
        "gone exactly the cap after the release"
    );

    let mut slow = duel(Delivery::Missile, |weapon| {
        weapon.projectile_speed = Some(SimScalar::from_num(0.05));
        weapon.max_projectile_lifetime_ticks = 1_000;
    })?;
    slow.world
        .catalog
        .set_attack_snapshot_cap(std::num::NonZeroU32::new(CAP));
    let release = slow.until_released()?;
    assert_eq!(images(&slow.world, 1), [(slow.attacker, Tick(release))]);
    let impact = slow.until_struck(slow.victim)?;
    assert!(
        impact > release + CAP,
        "premise: the flight outlasts the cap"
    );
    assert_eq!(
        images(&slow.world, 1),
        [(slow.attacker, Tick(impact))],
        "made again by the impact"
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Spells (V-10 to V-16)
// ---------------------------------------------------------------------------------------------

/// Every node a walk from `root` can reach, status effects included, in depth-first order.
fn tree(spells: &SpellCatalog, root: EffectId) -> Vec<EffectNode> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(node) = spells.node(id) else {
            continue;
        };
        out.push(node.kind);
        match node.kind {
            EffectNode::Set { first, len } => {
                stack.extend((0..len).filter_map(|index| spells.set_child(first, index)));
            }
            EffectNode::Search { child, .. } => stack.push(child),
            EffectNode::Persistent {
                initial, periodic, ..
            } => stack.extend(initial.into_iter().chain(periodic)),
            EffectNode::LaunchMissile { impact, .. } => stack.push(impact),
            EffectNode::ApplyStatus { status } => {
                if let Some(status) = spells.status(status) {
                    stack.extend(
                        status
                            .initial_effect
                            .into_iter()
                            .chain(status.periodic_effect),
                    );
                }
            }
            EffectNode::Displace { on_land, .. } => stack.extend(on_land),
            EffectNode::Damage { .. }
            | EffectNode::RemoveStatus { .. }
            | EffectNode::ModifyVitals { .. }
            | EffectNode::CancelOrders { .. } => {}
        }
    }
    out
}

/// The first shipped archetype with an ability whose effect root `root_is` accepts and whose
/// tree holds a node `holds` accepts, with that root.
fn caster_of(
    world: &SimWorld,
    root_is: impl Fn(&EffectNode) -> bool,
    holds: impl Fn(&EffectNode) -> bool,
) -> Option<(UnitArchetype, EffectId)> {
    let spells = world.catalog.spells();
    world.catalog.archetypes().find_map(|arch| {
        arch.abilities.iter().find_map(|ability| {
            let AbilityEffectSpec::Effect(key) = &ability.effect else {
                return None;
            };
            let root = spells.effect_id(key)?;
            let kind = spells.node(root)?.kind;
            (root_is(&kind) && tree(spells, root).iter().any(&holds)).then(|| (arch.clone(), root))
        })
    })
}

fn is_damage(node: &EffectNode) -> bool {
    matches!(node, EffectNode::Damage { .. })
}

/// The storm: a point persistent whose walk deals damage.
fn storm(world: &SimWorld) -> Option<(UnitArchetype, EffectId)> {
    caster_of(
        world,
        |node| {
            matches!(
                node,
                EffectNode::Persistent {
                    anchor: EffectAnchor::TargetPoint,
                    ..
                }
            )
        },
        is_damage,
    )
}

/// Fungal Growth: a missile to a point whose impact deals damage.
fn fungal(world: &SimWorld) -> Option<(UnitArchetype, EffectId)> {
    caster_of(
        world,
        |node| {
            matches!(
                node,
                EffectNode::LaunchMissile {
                    to: EffectAnchor::TargetPoint,
                    ..
                }
            )
        },
        is_damage,
    )
}

/// Abduct: a tree that launches a missile at a unit.
fn abduct(world: &SimWorld) -> Option<(UnitArchetype, EffectId)> {
    caster_of(
        world,
        |_| true,
        |node| {
            matches!(
                node,
                EffectNode::LaunchMissile {
                    to: EffectAnchor::TargetUnit,
                    ..
                }
            )
        },
    )
}

/// The spell stage: a world on the plateau map with the cap lifted, an unarmed copy of the
/// caster archetype of player 0 on the plateau's edge, and an unarmed tough holding victim of
/// player 1 below it at `victim_at`.
struct Stage {
    world: SimWorld,
    caster: EntityId,
    victim: EntityId,
    /// A shipped ground archetype, for more units below.
    ground: UnitArchetype,
}

fn stage(
    world: SimWorld,
    caster_arch: &UnitArchetype,
    victim_owner: u8,
) -> Result<Stage, Box<dyn std::error::Error>> {
    let mut world = world;
    world.catalog.set_attack_snapshot_cap(None);
    let (ground, _) = ground_missile(&world).ok_or("a shipped ground unit")?;
    let caster_type = register(&mut world, caster_arch, None);
    let victim_type = register(&mut world, &ground, None);
    let caster = world.spawn_unit(caster_type, PlayerId(0), edge())?;
    let victim = world.spawn_unit(victim_type, PlayerId(victim_owner), victim_spot())?;
    toughen(&mut world, victim)?;
    hold(&mut world, victim)?;
    hold(&mut world, caster)?;
    // One tick, so every player's vision is computed before a spell reads it.
    step(&mut world);
    Ok(Stage {
        world,
        caster,
        victim,
        ground,
    })
}

/// Where the victim of a spell stands: below the plateau, 3 from its edge.
fn victim_spot() -> SimVec2 {
    at(f64::from(PLATEAU) - 2.5, ROW)
}

impl Stage {
    /// Starts the walk of `root` by the caster at `target` now, as a cast would.
    fn cast(&mut self, root: EffectId, target: EffectTarget) {
        let caster = &self.world.entities[&self.caster];
        let context = EffectContext {
            owner: caster.owner,
            caster: Some(self.caster),
            ability: AbilitySource::default(),
            origin: caster.position,
            source_point: caster.position,
            target,
        };
        self.world.run_effect(root, context);
    }

    /// Spawns an unarmed tough holding ground unit of `owner` at `position`.
    fn bystander(
        &mut self,
        owner: u8,
        position: SimVec2,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let ground = self.ground.clone();
        let kind = register(&mut self.world, &ground, None);
        let id = self.world.spawn_unit(kind, PlayerId(owner), position)?;
        toughen(&mut self.world, id)?;
        hold(&mut self.world, id)?;
        Ok(id)
    }

    /// Spawns an armed holding ground unit of `owner` at `position`, its shipped missile weapon
    /// reaching the plateau's edge.
    fn gunner(
        &mut self,
        owner: u8,
        position: SimVec2,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let (arch, weapon) = ground_missile(&self.world).ok_or("a shipped ground missile")?;
        let kind = register(&mut self.world, &arch, Some(weapon));
        let id = self.world.spawn_unit(kind, PlayerId(owner), position)?;
        toughen(&mut self.world, id)?;
        hold(&mut self.world, id)?;
        Ok(id)
    }
}

/// The shipped storm's stage, the caster unseen by player 1.
fn storm_stage(victim_owner: u8) -> Result<(Stage, EffectId), Box<dyn std::error::Error>> {
    let world = sim_fixtures::try_world(plateau_map()?, 0x5707)?;
    let (arch, root) = storm(&world).ok_or("a shipped storm caster")?;
    Ok((stage(world, &arch, victim_owner)?, root))
}

/// V-10 (D-2 caller 2): the storm's first hit lists its caster once to the struck player; its
/// `Hidden` pulses never refresh the image, so under the shipped cap it is gone the cap after
/// the first hit while the storm still pulses; a unit that walks into the storm later takes its
/// own first hit, which makes the image again.
#[test]
fn a_storms_first_hit_lists_the_caster_and_its_pulses_do_not() -> TestResult {
    let (mut stage, root) = storm_stage(1)?;
    stage
        .world
        .catalog
        .set_attack_snapshot_cap(std::num::NonZeroU32::new(CAP));
    let late = stage.bystander(1, at(4.5, ROW))?;
    let cast = stage.world.current_tick;
    let full = vitals(&stage.world, stage.victim);
    stage.cast(root, EffectTarget::Point(victim_spot()));
    assert_ne!(
        vitals(&stage.world, stage.victim),
        full,
        "premise: the first hit landed"
    );
    assert_eq!(images(&stage.world, 1), [(stage.caster, Tick(cast))]);
    let mut pulses = 0;
    while stage.world.current_tick < cast + CAP - 1 {
        let before = health(&stage.world, stage.victim);
        step(&mut stage.world);
        pulses += usize::from(health(&stage.world, stage.victim) < before);
        assert_eq!(
            images(&stage.world, 1),
            [(stage.caster, Tick(cast))],
            "tick {}: the pulses refresh nothing",
            stage.world.current_tick
        );
    }
    assert!(pulses >= 2, "premise: the storm pulsed");
    step(&mut stage.world);
    assert!(
        images(&stage.world, 1).is_empty(),
        "gone the cap after the first hit"
    );
    assert!(
        !stage.world.effects().persistents().is_empty(),
        "the storm goes on"
    );

    stage.world.entities.get_mut(&late).ok_or("alive")?.position = victim_spot() + at(0.0, 0.5);
    let full = health(&stage.world, late);
    for _ in 0..20 {
        step(&mut stage.world);
        if health(&stage.world, late) < full {
            let hit = stage.world.current_tick;
            assert_eq!(
                images(&stage.world, 1),
                [(stage.caster, Tick(hit))],
                "its own first hit"
            );
            return Ok(());
        }
    }
    Err("the late unit was struck by the storm".into())
}

/// V-11 (D-2 callers 2 and 3, Q0128 2(a)): Fungal Growth's missile flies to a point, so its
/// launch lists nothing; the impact's zero damage lists the caster, and each pulse refreshes it.
#[test]
fn fungal_lists_its_caster_at_the_impact_and_on_every_pulse() -> TestResult {
    let world = sim_fixtures::try_world(plateau_map()?, 0xF06A)?;
    let (arch, root) = fungal(&world).ok_or("a shipped Fungal caster")?;
    let mut stage = stage(world, &arch, 1)?;
    stage.cast(root, EffectTarget::Point(victim_spot()));
    assert!(
        !stage.world.effects().missiles().is_empty(),
        "premise: a missile flies"
    );
    assert!(
        images(&stage.world, 1).is_empty(),
        "a point launch lists nothing"
    );
    let mut impact = None;
    for _ in 0..LIMIT {
        step(&mut stage.world);
        if !stage.world.effects().statuses_of(stage.victim).is_empty() {
            impact = Some(stage.world.current_tick);
            break;
        }
        assert!(images(&stage.world, 1).is_empty(), "nothing while it flies");
    }
    let impact = impact.ok_or("the missile landed")?;
    assert_eq!(
        images(&stage.world, 1),
        [(stage.caster, Tick(impact))],
        "the impact's dummy"
    );
    let mut refreshed = 0;
    for _ in 0..80 {
        let before = health(&stage.world, stage.victim);
        step(&mut stage.world);
        if health(&stage.world, stage.victim) < before {
            refreshed += 1;
            let pulse = stage.world.current_tick;
            assert_eq!(
                images(&stage.world, 1),
                [(stage.caster, Tick(pulse))],
                "a pulse refreshes it"
            );
        }
    }
    assert!(refreshed >= 2, "premise: Fungal pulsed");
    Ok(())
}

/// V-12 (D-2 caller 3): a spell missile aimed at a unit (Abduct's tongue) lists its caster to
/// the target's owner on the launch tick.
#[test]
fn a_unit_aimed_spell_missile_lists_its_caster_at_the_launch() -> TestResult {
    let world = sim_fixtures::try_world(plateau_map()?, 0xAB0C)?;
    let (arch, root) = abduct(&world).ok_or("a shipped Abduct caster")?;
    let mut stage = stage(world, &arch, 1)?;
    let launch = stage.world.current_tick;
    stage.cast(root, EffectTarget::Unit(stage.victim));
    assert!(
        !stage.world.effects().missiles().is_empty(),
        "premise: the missile flies"
    );
    assert_eq!(images(&stage.world, 1), [(stage.caster, Tick(launch))]);
    assert!(images(&stage.world, 0).is_empty() && images(&stage.world, 2).is_empty());
    step(&mut stage.world);
    assert_eq!(
        images(&stage.world, 1),
        [(stage.caster, Tick(launch))],
        "kept while unseen"
    );
    Ok(())
}

/// V-13: a storm on the caster's own player's units lists nothing to anyone.
#[test]
fn a_storm_on_its_own_players_units_lists_nothing() -> TestResult {
    let (mut stage, root) = storm_stage(0)?;
    let full = vitals(&stage.world, stage.victim);
    stage.cast(root, EffectTarget::Point(victim_spot()));
    assert_ne!(
        vitals(&stage.world, stage.victim),
        full,
        "premise: the storm struck"
    );
    for _ in 0..30 {
        for viewer in 0..4 {
            assert!(images(&stage.world, viewer).is_empty(), "player {viewer}");
        }
        step(&mut stage.world);
    }
    Ok(())
}

/// The storm's tables (the scratch storm of 5.0.14's values), appended to the shipped ones, its
/// first hit authoring a `Visible` visibility; its pulses keep the default.
fn scratch_visible_storm(world: &mut SimWorld) -> Result<EffectId, Box<dyn std::error::Error>> {
    let (statuses, effects) = ScratchStorm::GAME.tables();
    let visibility = format!("visibility: Visible(reveal_ticks: {REVEAL})");
    let initial = "responses: (acquire: true, flee: true)";
    if effects.matches(initial).count() != 1 {
        return Err("the scratch storm's first hit is authored as expected".into());
    }
    let effects = effects.replace(initial, &format!("{initial}, {visibility}"));
    let spells = sim_fixtures::spells_with(&statuses, &effects)?;
    world.catalog.set_spell_catalog(spells)?;
    world
        .catalog
        .spells()
        .effect_id("scratch_storm")
        .ok_or_else(|| "the scratch storm is installed".into())
}

/// V-14 (D-2 phase order): a `Visible` storm (a scratch copy) reveals its caster to the struck
/// player from its first hit, for exactly the acquisition phases a weapon's reveal lasts, and a
/// unit of that player in reach acquires the caster; the shipped storm never does. A `Visible`
/// cast by a cloaked caster that player does not detect makes nothing targetable.
#[test]
fn a_visible_spell_reveals_its_caster_and_makes_it_a_target() -> TestResult {
    let gunner_at = at(f64::from(PLATEAU) - 1.5, ROW + 3.0);
    let (mut stage, _) = storm_stage(1)?;
    let root = scratch_visible_storm(&mut stage.world)?;
    let gunner = stage.gunner(1, gunner_at)?;
    stage.cast(root, EffectTarget::Point(victim_spot()));
    assert_eq!(
        stage.world.entities[&stage.caster]
            .attack_reveal_ticks
            .get(PlayerId(1)),
        Some(&REVEAL),
        "raised by the first hit"
    );
    assert!(images(&stage.world, 1).is_empty(), "revealed: no image");
    let mut positive = 0;
    let mut acquired = false;
    for _ in 0..u32::from(REVEAL) + 5 {
        step(&mut stage.world);
        if revealed_to(&stage.world, stage.caster, 1) {
            positive += 1;
        }
        acquired |= stage.world.entities[&gunner].current_target == Some(stage.caster);
    }
    // Raised before this tick's vision phase, the counter is counted down there: positive on the
    // acquisition phases of REVEAL − 1 ticks, the count a weapon's reveal raised in phase 7 gets.
    assert_eq!(positive, u32::from(REVEAL) - 1);
    assert!(
        acquired,
        "a unit of the struck player in reach acquires the caster"
    );

    let (mut shipped, root) = storm_stage(1)?;
    let gunner = shipped.gunner(1, gunner_at)?;
    shipped.cast(root, EffectTarget::Point(victim_spot()));
    for _ in 0..80 {
        step(&mut shipped.world);
        assert_ne!(
            shipped.world.entities[&gunner].current_target,
            Some(shipped.caster)
        );
        assert!(!revealed_to(&shipped.world, shipped.caster, 1));
    }

    let (mut cloaked, _) = storm_stage(1)?;
    let root = scratch_visible_storm(&mut cloaked.world)?;
    let gunner = cloaked.gunner(1, gunner_at)?;
    cloaked
        .world
        .entities
        .get_mut(&cloaked.caster)
        .ok_or("spawned")?
        .is_cloaked = true;
    cloaked.cast(root, EffectTarget::Point(victim_spot()));
    for _ in 0..60 {
        step(&mut cloaked.world);
        assert_ne!(
            cloaked.world.entities[&gunner].current_target,
            Some(cloaked.caster),
            "undetected: not a target"
        );
    }
    Ok(())
}

/// V-15: a caster that dies after the cast leaves no image at a later damage. (A dead caster is
/// reaped at the end of its tick, so no later tick can observe its counter; that a damage or a
/// launch by a dead caster raises no reveal is the dead-attacker row of V-21, for every cause.)
#[test]
fn a_dead_casters_later_damage_shows_nothing() -> TestResult {
    let world = sim_fixtures::try_world(plateau_map()?, 0xF06A)?;
    let (arch, root) = fungal(&world).ok_or("a shipped Fungal caster")?;
    let mut stage = stage(world, &arch, 1)?;
    stage.cast(root, EffectTarget::Point(victim_spot()));
    stage
        .world
        .entities
        .get_mut(&stage.caster)
        .ok_or("alive")?
        .hp = SimScalar::ZERO;
    let mut landed = false;
    for _ in 0..80 {
        step(&mut stage.world);
        landed |= !stage.world.effects().statuses_of(stage.victim).is_empty();
        assert!(
            images(&stage.world, 1).is_empty(),
            "tick {}",
            stage.world.current_tick
        );
    }
    assert!(landed, "premise: the missile landed and its damage ran");

    Ok(())
}

/// V-16: only the struck player gets the image; a third player whose units the spell missed
/// lists nothing, and under shipped data nobody is revealed the caster.
#[test]
fn only_the_struck_player_lists_a_spells_caster() -> TestResult {
    let (mut stage, root) = storm_stage(1)?;
    stage.bystander(2, at(4.5, ROW))?;
    let cast = stage.world.current_tick;
    stage.cast(root, EffectTarget::Point(victim_spot()));
    assert_eq!(images(&stage.world, 1), [(stage.caster, Tick(cast))]);
    for _ in 0..30 {
        assert!(images(&stage.world, 2).is_empty(), "missed: nothing");
        assert!(
            images(&stage.world, 0).is_empty(),
            "its own player: nothing"
        );
        for viewer in 0..4 {
            let view = view_of(&stage.world, viewer);
            assert!(!view.revealed.contains(&stage.caster), "player {viewer}");
            assert!(
                !view.attack_revealed.contains(&stage.caster),
                "player {viewer}"
            );
        }
        step(&mut stage.world);
    }
    Ok(())
}
