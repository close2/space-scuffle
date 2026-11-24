//! An air-plane attacker is revealed to the player it strikes (B-55).
//!
//! # Responsibility
//! Pins plan `2026-11-23-vision-and-detection` D-4 to D-6: a strike by a weapon whose attack
//! visibility is `Visible` (`StarCraft` II 5.0.14's `CEffectDamage` `Visibility Visible`, set on
//! every damage effect of air units and the Colossus) reveals its attacker to the struck unit's
//! owner for the weapon's reveal ticks (`AttackReveal`, `core` BehaviorData:237–240, 3 Normal s),
//! so that player may target it; a `Snapshot` or `Hidden` weapon reveals nothing.
//!
//! # How it fits
//! The counters live on each entity (`Entity::attack_reveal_ticks`), are set by the combat phase
//! and counted down by the vision phase; target acquisition, retaliation, casts and attack
//! dashes read them through one predicate, and `PlayerView::attack_revealed` publishes them.
//!
//! # Invariants
//! - No unit is named: archetypes and weapons are found by property (charter §3.3).

use sim_core::{SimWorld, SplashSpec, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackVisibility, EntityId, MoverType, PlayerId, PlayerSlot, PlayerView, SimScalar, SimVec2,
    SplashCliff, SplashImpact, SplashShape, SplashZone, Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// D-5: a new unit is revealed to no player, and no view lists it.
#[test]
fn a_fresh_unit_is_revealed_to_nobody() -> TestResult {
    let mut world: SimWorld = sim_fixtures::world(
        GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    let unit_type = world
        .catalog
        .archetypes()
        .next()
        .map(|arch| arch.unit_type_id)
        .ok_or("the shipped catalog has archetypes")?;
    let unit = world.spawn_unit(unit_type, PlayerId(0), SimVec2::from_i32(8, 8))?;
    let entity = &world.entities[&unit];
    for slot in PlayerSlot::ALL {
        assert_eq!(entity.attack_reveal_ticks[slot], 0);
        assert!(!entity.is_attack_revealed_to(PlayerId(u8::try_from(slot.index())?)));
    }
    let mut view = PlayerView::default();
    world.write_player_view(PlayerId(1), &mut view);
    assert_eq!(
        view.attack_revealed,
        Vec::new(),
        "no view lists a fresh unit"
    );
    Ok(())
}

/// The plateau's first column: every cell with x at or above it is one cliff level up.
const PLATEAU: u32 = 16;
/// Row of every unit's centre.
const ROW: f64 = 16.5;

/// A 32x32 map whose cells with x >= [`PLATEAU`] stand one cliff level up, with no ramp.
fn plateau_map() -> Result<GameMap, sim_map::MapError> {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..32 {
        for x in PLATEAU..32 {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
        }
    }
    Ok(map)
}

fn step(world: &mut SimWorld, tick: u32) {
    world.step(&TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    });
}

fn slot(player: u8) -> PlayerSlot {
    PlayerSlot::ALL[usize::from(player)]
}

/// The first archetype passing `keep`, with the weapon `pick` chooses from it.
fn find(
    world: &SimWorld,
    keep: impl Fn(&UnitArchetype) -> bool,
    pick: impl Fn(&UnitArchetype) -> Option<&WeaponSpec>,
) -> Option<(UnitArchetype, WeaponSpec)> {
    world
        .catalog
        .archetypes()
        .filter(|arch| keep(arch))
        .find_map(|arch| pick(arch).map(|weapon| (arch.clone(), weapon.clone())))
}

/// A shipped air archetype whose weapon against ground is `Visible` and hits light ground units,
/// with that weapon.
fn visible_air_attacker(world: &SimWorld) -> Option<(UnitArchetype, WeaponSpec)> {
    find(
        world,
        |arch| arch.mover_type == MoverType::Air && arch.max_speed > SimScalar::ZERO,
        |arch| {
            arch.weapon_against(MoverType::Ground, None).filter(|w| {
                matches!(w.attack_visibility, AttackVisibility::Visible { .. })
                    && w.splash.is_none()
            })
        },
    )
}

/// A shipped mobile ground archetype, not a detector, whose weapon hits air units.
fn ground_anti_air(world: &SimWorld) -> Option<(UnitArchetype, WeaponSpec)> {
    find(
        world,
        |arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
        },
        |arch| {
            arch.weapon_against(MoverType::Air, None)
                .filter(|w| w.max_range >= SimScalar::from_i32(4))
        },
    )
}

/// Registers `arch` with `weapon` as its only weapon, under the next free identity.
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
    format!("scratch_reveal_{}", id.0).clone_into(&mut copy.slug);
    copy.weapon.clone_from(&weapon);
    copy.ground_weapon.clone_from(&weapon);
    copy.air_weapon = weapon.filter(|_| arch.air_weapon.is_some());
    copy.detection_radius = None;
    world.catalog.register(copy);
    id
}

/// The bench: an attacker of player 0 over the plateau's edge, a victim of player 1 below it,
/// in each other's weapon reach, and the attacker ordered to attack the victim, which holds.
struct Bench {
    world: SimWorld,
    attacker: EntityId,
    victim: EntityId,
    /// The ticks one strike of the attacker's weapon reveals it for, if it is `Visible`.
    reveal: Option<u16>,
}

/// Builds the bench with the attacker's weapon `weapon` changed by `edit` and the attacker of
/// archetype `attacker` (an air unit unless stated otherwise).
fn bench(
    attacker: impl Fn(&SimWorld) -> Option<(UnitArchetype, WeaponSpec)>,
    edit: impl FnOnce(&mut WeaponSpec),
) -> Result<Bench, Box<dyn std::error::Error>> {
    let mut world = sim_fixtures::world(plateau_map()?, 0x55);
    let (attacker_arch, mut weapon) =
        attacker(&world).ok_or("a shipped attacker with the wanted weapon")?;
    let (victim_arch, victim_weapon) =
        ground_anti_air(&world).ok_or("a shipped ground unit that hits air")?;
    edit(&mut weapon);
    let reveal = match weapon.attack_visibility {
        AttackVisibility::Visible { reveal_ticks } => Some(reveal_ticks),
        _ => None,
    };
    let reach = weapon.max_range.min(victim_weapon.max_range)
        + attacker_arch.collision_radius
        + victim_arch.collision_radius
        - SimScalar::from_num(0.25);
    let attacker_type = register(&mut world, &attacker_arch, Some(weapon));
    let attacker_x = SimScalar::from_num(f64::from(PLATEAU) + 0.5);
    let attacker = world.spawn_unit(
        attacker_type,
        PlayerId(0),
        SimVec2::new(attacker_x, SimScalar::from_num(ROW)),
    )?;
    let victim = world.spawn_unit(
        victim_arch.unit_type_id,
        PlayerId(1),
        SimVec2::new(attacker_x - reach, SimScalar::from_num(ROW)),
    )?;
    world
        .entities
        .get_mut(&victim)
        .ok_or("spawned")?
        .issue_order(UnitOrder::HoldPosition, false)
        .map_err(|error| format!("{error:?}"))?;
    world
        .entities
        .get_mut(&attacker)
        .ok_or("spawned")?
        .issue_order(UnitOrder::attack(victim), false)
        .map_err(|error| format!("{error:?}"))?;
    Ok(Bench {
        world,
        attacker,
        victim,
        reveal,
    })
}

impl Bench {
    fn vitals(&self, id: EntityId) -> Option<(SimScalar, SimScalar)> {
        self.world.entities.get(&id).map(|e| (e.hp, e.shields))
    }

    /// Steps from `from` until the victim is first struck; returns that tick.
    fn until_struck(&mut self, from: u32) -> Result<u32, String> {
        let full = self.vitals(self.victim);
        for tick in from..from + 200 {
            step(&mut self.world, tick);
            if self.vitals(self.victim) != full {
                return Ok(tick);
            }
        }
        Err("the victim was struck within 200 ticks".to_owned())
    }

    fn counter(&self, id: EntityId, player: u8) -> u16 {
        self.world
            .entities
            .get(&id)
            .map_or(0, |e| e.attack_reveal_ticks[slot(player)])
    }

    fn victim_target(&self) -> Option<EntityId> {
        self.world.entities.get(&self.victim)?.current_target
    }
}

/// P-10: one strike of a `Visible` weapon reveals the attacker to the struck unit's owner for
/// exactly its reveal ticks: the counter holds the full value after the striking tick, one less
/// after each later tick, and the victim, which could not see the attacker before, acquires it
/// on the next tick. Its own player and a third player are unaffected.
#[test]
fn a_visible_strike_reveals_the_attacker_for_its_ticks() -> TestResult {
    let mut bench = bench(visible_air_attacker, |_| {})?;
    let reveal = bench.reveal.ok_or("the shipped air weapon is Visible")?;
    step(&mut bench.world, 0);
    assert_eq!(
        bench.victim_target(),
        None,
        "premise: the attacker is unseen"
    );
    let struck = bench.until_struck(1)?;
    assert_eq!(
        bench.counter(bench.attacker, 1),
        reveal,
        "the full reveal after the strike"
    );
    assert_eq!(bench.counter(bench.attacker, 0), 0, "not to its own player");
    assert_eq!(bench.counter(bench.attacker, 2), 0, "not to a third player");
    // Take the victim out of the attacker's filters so that no later strike refreshes it.
    bench
        .world
        .entities
        .get_mut(&bench.victim)
        .ok_or("alive")?
        .is_invulnerable = true;
    step(&mut bench.world, struck + 1);
    assert_eq!(
        bench.victim_target(),
        Some(bench.attacker),
        "the victim acquires the revealed attacker on the next tick"
    );
    for k in 1..u32::from(reveal) {
        assert_eq!(
            u32::from(bench.counter(bench.attacker, 1)),
            u32::from(reveal) - k,
            "after tick t+{k}"
        );
        assert!(bench.world.entities[&bench.attacker].is_attack_revealed_to(PlayerId(1)));
        step(&mut bench.world, struck + 1 + k);
    }
    assert_eq!(
        bench.counter(bench.attacker, 1),
        0,
        "revealed for exactly its ticks"
    );
    assert!(!bench.world.entities[&bench.attacker].is_attack_revealed_to(PlayerId(1)));
    Ok(())
}

/// P-10: each strike refreshes the reveal to its full value and never shortens a longer one.
#[test]
fn each_strike_refreshes_the_reveal() -> TestResult {
    let mut bench = bench(visible_air_attacker, |_| {})?;
    let reveal = bench.reveal.ok_or("the shipped air weapon is Visible")?;
    let first = bench.until_struck(0)?;
    let second = bench.until_struck(first + 1)?;
    assert_eq!(
        bench.counter(bench.attacker, 1),
        reveal,
        "refreshed by the strike at {second}"
    );
    bench
        .world
        .entities
        .get_mut(&bench.attacker)
        .ok_or("alive")?
        .attack_reveal_ticks[slot(1)] = u16::MAX;
    let third = bench.until_struck(second + 1)?;
    assert!(
        bench.counter(bench.attacker, 1) >= u16::MAX - u16::try_from(third - second)?,
        "a strike never shortens a longer reveal"
    );
    Ok(())
}

/// P-10: an area strike reveals the attacker to the owner of each unit it strikes.
#[test]
fn an_area_strike_reveals_to_each_struck_owner() -> TestResult {
    let mut bench = bench(visible_air_attacker, |weapon| {
        weapon.splash = Some(SplashSpec {
            zones: vec![SplashZone {
                radius: SimScalar::from_i32(2),
                fraction: SimScalar::ONE,
            }],
            shape: SplashShape::Circle,
            friendly_fire: false,
            offset_by_unit_radius: false,
            impact: SplashImpact::Target,
            target_radius_below: None,
            cliff: SplashCliff::Same,
            strike_target_on_other_level: false,
        });
    })?;
    let victim = &bench.world.entities[&bench.victim];
    let (kind, at) = (victim.unit_type_id, victim.position);
    // Unarmed, so that the two struck players do not fight each other.
    let arch = bench.world.catalog.get(kind)?.clone();
    let kind = register(&mut bench.world, &arch, None);
    let beside =
        bench
            .world
            .spawn_unit(kind, PlayerId(2), SimVec2::new(at.x, at.y + SimScalar::ONE))?;
    bench
        .world
        .entities
        .get_mut(&beside)
        .ok_or("spawned")?
        .issue_order(UnitOrder::HoldPosition, false)
        .map_err(|error| format!("{error:?}"))?;
    let reveal = bench.reveal.ok_or("the shipped air weapon is Visible")?;
    bench.until_struck(0)?;
    assert_eq!(
        bench.counter(bench.attacker, 1),
        reveal,
        "the target's owner"
    );
    assert_eq!(
        bench.counter(bench.attacker, 2),
        reveal,
        "the area's other owner"
    );
    assert_eq!(
        bench.counter(bench.attacker, 3),
        0,
        "a player it did not strike"
    );
    Ok(())
}

/// P-10: a reveal grants visibility, not detection: a cloaked attacker stays untargetable.
#[test]
fn a_cloaked_attacker_stays_hidden_without_detection() -> TestResult {
    let mut bench = bench(visible_air_attacker, |_| {})?;
    bench
        .world
        .entities
        .get_mut(&bench.attacker)
        .ok_or("spawned")?
        .is_cloaked = true;
    let reveal = bench.reveal.ok_or("the shipped air weapon is Visible")?;
    let struck = bench.until_struck(0)?;
    assert_eq!(
        bench.counter(bench.attacker, 1),
        reveal,
        "the reveal is recorded"
    );
    for tick in struck + 1..struck + u32::from(reveal) {
        step(&mut bench.world, tick);
        assert_eq!(bench.victim_target(), None, "tick {tick}: never targeted");
    }
    let mut view = PlayerView::default();
    bench.world.write_player_view(PlayerId(1), &mut view);
    assert!(!view.revealed.contains(&bench.attacker), "not detected");
    Ok(())
}

/// P-10: a `Snapshot` or `Hidden` weapon reveals nothing.
#[test]
fn a_snapshot_or_hidden_weapon_reveals_nothing() -> TestResult {
    for visibility in [AttackVisibility::Snapshot, AttackVisibility::Hidden] {
        let mut bench = bench(visible_air_attacker, |weapon| {
            weapon.attack_visibility = visibility;
        })?;
        let struck = bench.until_struck(0)?;
        for tick in struck + 1..struck + 20 {
            step(&mut bench.world, tick);
            assert_eq!(bench.counter(bench.attacker, 1), 0, "{visibility:?}");
            assert_eq!(bench.victim_target(), None, "{visibility:?}");
        }
    }
    Ok(())
}

/// P-10: an attacker that died before its strike landed reveals nothing: a missile of a
/// `Visible` weapon whose attacker died in flight strikes, and no unit is revealed.
#[test]
fn a_dead_attacker_reveals_nothing() -> TestResult {
    let mut bench = bench(visible_air_attacker, |weapon| {
        weapon.projectile_speed = Some(SimScalar::from_num(0.25));
    })?;
    let full = bench.vitals(bench.victim);
    let mut tick = 0;
    while bench.world.projectiles.is_empty() {
        step(&mut bench.world, tick);
        tick += 1;
        assert!(tick < 200, "the missile launched");
    }
    bench
        .world
        .entities
        .get_mut(&bench.attacker)
        .ok_or("alive")?
        .hp = SimScalar::ZERO;
    while !bench.world.projectiles.is_empty() {
        step(&mut bench.world, tick);
        tick += 1;
        assert!(tick < 400, "the missile landed");
    }
    assert_ne!(bench.vitals(bench.victim), full, "the missile struck");
    assert!(
        bench
            .world
            .entities
            .values()
            .all(|e| e.attack_reveal_ticks.iter().all(|&ticks| ticks == 0)),
        "nobody is revealed"
    );
    Ok(())
}

/// P-12: a ground attacker on the plateau striking a unit below is never seen by the struck
/// player, before and after this plan (its weapon is a `Snapshot`, 5.0.14's default).
#[test]
fn a_ground_attacker_above_stays_unseen() -> TestResult {
    let ground_attacker = |world: &SimWorld| {
        find(
            world,
            |arch| {
                arch.mover_type == MoverType::Ground
                    && arch.max_speed > SimScalar::ZERO
                    && !arch.is_detector()
            },
            |arch| {
                arch.weapon_against(MoverType::Ground, None).filter(|w| {
                    w.max_range >= SimScalar::from_i32(4)
                        && w.splash.is_none()
                        && w.attack_visibility == AttackVisibility::Snapshot
                })
            },
        )
    };
    let mut bench = bench(ground_attacker, |_| {})?;
    let struck = bench.until_struck(0)?;
    for tick in struck + 1..struck + 60 {
        step(&mut bench.world, tick);
        assert_eq!(bench.counter(bench.attacker, 1), 0);
        assert_eq!(bench.victim_target(), None, "tick {tick}: never seen");
    }
    Ok(())
}

/// D-6: the view lists the units its attacks reveal to the viewer, by ID, and never the
/// viewer's own units.
#[test]
fn the_view_lists_attack_revealed_enemies_in_order() -> TestResult {
    let mut bench = bench(visible_air_attacker, |_| {})?;
    bench.until_struck(0)?;
    let mut view = PlayerView::default();
    bench.world.write_player_view(PlayerId(1), &mut view);
    assert_eq!(view.attack_revealed, vec![bench.attacker]);
    bench.world.write_player_view(PlayerId(0), &mut view);
    assert_eq!(
        view.attack_revealed,
        Vec::new(),
        "never the viewer's own units"
    );
    // A second revealed attacker sorts by ID.
    let (kind, at) = {
        let attacker = &bench.world.entities[&bench.attacker];
        (attacker.unit_type_id, attacker.position)
    };
    let other =
        bench
            .world
            .spawn_unit(kind, PlayerId(0), SimVec2::new(at.x, at.y + SimScalar::ONE))?;
    bench
        .world
        .entities
        .get_mut(&other)
        .ok_or("spawned")?
        .attack_reveal_ticks[slot(1)] = 5;
    bench.world.write_player_view(PlayerId(1), &mut view);
    assert_eq!(view.attack_revealed, vec![bench.attacker, other]);
    Ok(())
}
