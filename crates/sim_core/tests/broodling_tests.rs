//! Strike magazines, impact spawns and timed life through `SimWorld` (plan
//! `2026-11-28-broodlings` §6.1, B-1 to B-9; B-10's validation and encoding are unit tests of
//! `catalog::strike_validation` and `checksum::encoding`).
//!
//! No shipped row carries the mechanisms in WP1, so every test authors them on the loaded
//! catalog of its own world, on units found by property, never by slug (charter §3.3): the first
//! plain armed air unit becomes the "lord", a ground-attacking air unit whose missile weapon has
//! a strike magazine `(2, 2, 45, 9)` and an impact spawn; the second plain armed ground unit
//! becomes the "mite", a small supply-free melee unit the spawn creates with a timed life
//! `(89, 5)`; the first plain ground unit becomes an unarmed, durable dummy. The values are the
//! 5.0.14 Brood Lord's (plan §2.2): 2 escorts, 2 at the start, one rebuilt per 45 ticks, armed 9
//! ticks after its build, a missile of two strikes of 20 every 45 ticks, and a Broodling of 5
//! Normal s (89 ticks) plus 0 to 5 at random per escort.

use std::error::Error;

use sim_core::catalog::UnitArchetype;
use sim_core::{SimWorld, SimulationDiagnostic, WeaponPhase};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, AttackVisibility, AttributeFlags, CombatEventVisual, EntityId, ImpactSpawn,
    MissileValidation, MoverType, PlayerCommand, PlayerId, ProjectileTargetLossBehavior,
    RenderSnapshot, ShapeKind, SimAngle, SimScalar, SimVec2, SpawnOrder, SplashCliff, SplashImpact,
    SplashShape, SplashSpec, SplashZone, StrikeMagazineSpec, Tick, TickBundle, TimedLifeSpec,
    UnitOrder, UnitTypeId, WeaponSpec, WeaponVisualKind,
};

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);
const FOE: PlayerId = PlayerId(1);

/// The Brood Lord's magazine (plan §2.2 rule 1): 2 escorts, 2 at the start, 45 ticks a build,
/// armed 9 ticks after it.
const MAGAZINE: StrikeMagazineSpec = StrikeMagazineSpec {
    capacity: 2,
    start: 2,
    build_ticks: 45,
    arming_ticks: 9,
};

/// The Broodling's clock (plan §2.2 rule 7): 89 ticks plus 0 to 5.
const LIFE: TimedLifeSpec = TimedLifeSpec {
    ticks: 89,
    random_extra_ticks: 5,
};

/// A world with the authored mechanisms: the lord's, the mite's and the dummy's types.
struct Nest {
    world: SimWorld,
    lord: UnitTypeId,
    mite: UnitTypeId,
    dummy: UnitTypeId,
    tick: u32,
    /// The highest projectile id seen, so a new one is a release.
    seen_projectile: Option<u32>,
}

/// One release of a lord's weapon: the tick it left and the strikes its missile carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Release {
    tick: u32,
    strikes: u8,
}

fn is_plain_unit(arch: &UnitArchetype) -> bool {
    !arch.attributes.contains(AttributeFlags::STRUCTURE)
        && arch.max_speed > SimScalar::ZERO
        && arch.capabilities.transport_capacity == 0
        && arch.capabilities.hangar.is_none()
        && arch.harvester.is_none()
        && arch.capabilities.modes.is_empty()
        && arch.modes.is_empty()
}

/// The first plain armed air unit and the first two plain armed ground units, by id.
fn pick(world: &SimWorld) -> Result<[UnitTypeId; 3], Box<dyn Error>> {
    let mut armed: Vec<&UnitArchetype> = world
        .catalog
        .archetypes()
        .filter(|arch| is_plain_unit(arch) && arch.weapon.is_some())
        .collect();
    armed.sort_by_key(|arch| arch.unit_type_id);
    let air = armed.iter().find(|a| a.mover_type == MoverType::Air);
    let mut ground = armed.iter().filter(|a| a.mover_type == MoverType::Ground);
    match (air, ground.next(), ground.next()) {
        (Some(lord), Some(dummy), Some(mite)) => {
            Ok([lord.unit_type_id, dummy.unit_type_id, mite.unit_type_id])
        }
        _ => Err("the shipped catalog has a plain armed air unit and two ground units".into()),
    }
}

/// The lord's missile: two strikes of 20 every 45 ticks (damage point 3, backswing 9, no random
/// delay) from range 10 at a constant 0.9 per tick, following its target through anything but a
/// teleport and landing at the last point when it loses it, as the shipped Brood Lord row's.
fn lord_weapon(base: &WeaponSpec, spawn: Option<ImpactSpawn>) -> WeaponSpec {
    let mut weapon = base.clone();
    weapon.visual_kind = WeaponVisualKind::Projectile;
    weapon.target_flags = AttributeFlags::all();
    weapon.min_range = SimScalar::ZERO;
    weapon.max_range = SimScalar::from_i32(10);
    weapon.firing_arc = SimAngle::DEG_180;
    weapon.acquire_arc = SimAngle::DEG_180;
    weapon.has_turret = false;
    weapon.base_damage = SimScalar::from_i32(20);
    weapon.attribute_bonuses = sim_types::AttributeBonuses::default();
    weapon.attacks_count = 2;
    weapon.first_strike_delay_ticks = 0;
    weapon.strike_interval_ticks = 0;
    weapon.period_ticks = 45;
    weapon.damage_point_ticks = 3;
    weapon.backswing_ticks = 9;
    weapon.random_delay_min = SimScalar::ZERO;
    weapon.random_delay_max = SimScalar::ZERO;
    weapon.splash = None;
    weapon.projectile_speed = Some(SimScalar::from_num(0.9));
    weapon.missile_movers = Vec::new();
    weapon.magazine_launch = None;
    weapon.target_loss_behavior = ProjectileTargetLossBehavior::ImpactLastKnownPosition;
    weapon.missile_validation = MissileValidation {
        weapon: false,
        teleport: true,
    };
    weapon.attack_visibility = AttackVisibility::Visible { reveal_ticks: 54 };
    weapon.launch_visibility = AttackVisibility::Snapshot;
    weapon.impact_spawn = spawn;
    weapon
}

/// The mite's claws: 4 every 14 ticks at melee range, ground only.
fn mite_weapon(base: &WeaponSpec) -> WeaponSpec {
    let mut weapon = base.clone();
    weapon.visual_kind = WeaponVisualKind::None;
    weapon.target_flags = AttributeFlags::all();
    weapon.min_range = SimScalar::ZERO;
    weapon.max_range = SimScalar::from_num(0.1);
    weapon.base_damage = SimScalar::from_i32(4);
    weapon.attacks_count = 1;
    weapon.first_strike_delay_ticks = 0;
    weapon.strike_interval_ticks = 0;
    weapon.period_ticks = 14;
    weapon.splash = None;
    weapon.projectile_speed = None;
    weapon.missile_movers = Vec::new();
    weapon.magazine_launch = None;
    weapon.melee = true;
    weapon.impact_spawn = None;
    weapon.missile_validation = MissileValidation::default();
    weapon.launch_visibility = AttackVisibility::Snapshot;
    weapon
}

/// The impact spawn the tests author: one mite per strike within 5, with the Broodling's clock
/// and order.
fn spawn_of(mite: UnitTypeId) -> ImpactSpawn {
    ImpactSpawn {
        unit: mite,
        per_strike: 1,
        search_range: SimScalar::from_i32(5),
        timed_life: Some(LIFE),
        order: SpawnOrder::AttackTargetOrPoint,
    }
}

/// A world whose lord has `magazine` and, with `spawns`, the impact spawn of the mite.
fn nest_with(
    magazine: Option<StrikeMagazineSpec>,
    spawns: bool,
    seed: u64,
) -> Result<Nest, Box<dyn Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let [lord, dummy, mite] = pick(&world)?;
    let base = world
        .catalog
        .get(lord)?
        .weapon
        .clone()
        .ok_or("the lord is armed")?;
    let spawn = spawns.then(|| spawn_of(mite));
    let lord_arch = world.catalog.get_mut(lord)?;
    let weapon = lord_weapon(&base, spawn);
    lord_arch.weapon = Some(weapon.clone());
    lord_arch.ground_weapon = Some(weapon);
    lord_arch.air_weapon = None;
    lord_arch.capabilities.strike_magazine = magazine;
    lord_arch.max_hp = SimScalar::from_i32(5000);
    let claws = mite_weapon(&base);
    let mite_arch = world.catalog.get_mut(mite)?;
    mite_arch.weapon = Some(claws.clone());
    mite_arch.ground_weapon = Some(claws);
    mite_arch.air_weapon = None;
    mite_arch.supply_cost = 0;
    mite_arch.max_hp = SimScalar::from_i32(20);
    let dummy_arch = world.catalog.get_mut(dummy)?;
    dummy_arch.weapon = None;
    dummy_arch.ground_weapon = None;
    dummy_arch.air_weapon = None;
    dummy_arch.max_hp = SimScalar::from_i32(5000);
    Ok(Nest {
        world,
        lord,
        mite,
        dummy,
        tick: 0,
        seen_projectile: None,
    })
}

fn nest() -> Result<Nest, Box<dyn Error>> {
    nest_with(Some(MAGAZINE), true, 0xB2_00D)
}

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::from_i32(x, y)
}

impl Nest {
    fn step(&mut self) -> RenderSnapshot {
        self.tick += 1;
        let mut out = RenderSnapshot::default();
        self.world.step_into(
            &TickBundle {
                tick: Tick(self.tick),
                commands: Vec::new(),
                state_checksum: None,
            },
            &mut out,
        );
        out
    }

    /// The releases of `lord` this step: the projectiles it launched that were not there before.
    fn releases(&mut self, lord: EntityId) -> Vec<Release> {
        let seen = self.seen_projectile;
        let fresh: Vec<Release> = self
            .world
            .projectiles
            .iter()
            .filter(|p| p.attacker_id == Some(lord) && seen.is_none_or(|s| p.id > s))
            .map(|p| Release {
                tick: self.world.current_tick,
                strikes: p.strikes,
            })
            .collect();
        if let Some(max) = self.world.projectiles.iter().map(|p| p.id).max() {
            self.seen_projectile = Some(seen.map_or(max, |s| s.max(max)));
        }
        fresh
    }

    /// Steps `n` ticks and returns every release of `lord` in them.
    fn run(&mut self, lord: EntityId, n: u32) -> Vec<Release> {
        let mut all = Vec::new();
        for _ in 0..n {
            self.step();
            all.extend(self.releases(lord));
        }
        all
    }

    /// The living mites, ascending by id.
    fn mites(&self) -> Vec<EntityId> {
        self.world
            .entities
            .values()
            .filter(|e| e.unit_type_id == self.mite && e.is_alive())
            .map(|e| e.id)
            .collect()
    }

    fn attack(&mut self, unit: EntityId, target: EntityId) -> TestResult {
        let entity = self.world.entities.get_mut(&unit).ok_or("unit")?;
        entity.issue_order(UnitOrder::attack(target), false)?;
        Ok(())
    }

    /// A lord of `ME` at `from` attacking a dummy of `FOE` at `to`, in range.
    fn engaged(
        &mut self,
        from: SimVec2,
        to: SimVec2,
    ) -> Result<(EntityId, EntityId), Box<dyn Error>> {
        let lord = self.world.spawn_unit(self.lord, ME, from)?;
        let target = self.world.spawn_unit(self.dummy, FOE, to)?;
        self.world
            .entities
            .get_mut(&target)
            .ok_or("target")?
            .issue_order(UnitOrder::HoldPosition, false)?;
        self.attack(lord, target)?;
        Ok((lord, target))
    }

    /// Steps until `lord` releases, at most `limit` ticks.
    fn until_release(&mut self, lord: EntityId, limit: u32) -> Result<Release, Box<dyn Error>> {
        for _ in 0..limit {
            self.step();
            if let Some(release) = self.releases(lord).first() {
                return Ok(*release);
            }
        }
        Err("the lord released within the limit".into())
    }

    /// Steps until the projectiles of `lord` have all landed, at most `limit` ticks.
    fn until_landed(&mut self, lord: EntityId, limit: u32) -> Result<(), Box<dyn Error>> {
        for _ in 0..limit {
            if !self
                .world
                .projectiles
                .iter()
                .any(|p| p.attacker_id == Some(lord))
            {
                return Ok(());
            }
            self.step();
        }
        Err("the missiles landed within the limit".into())
    }
}

/// §2.4's strike schedule from a first release at `first`: 2 strikes, then 1 at `first + 57`,
/// `first + 102`, … until `total` strikes.
fn schedule(first: u32, total: u8) -> Vec<Release> {
    let mut releases = vec![Release {
        tick: first,
        strikes: 2,
    }];
    let mut tick = first + 57;
    let mut landed = 2;
    while landed < total {
        releases.push(Release { tick, strikes: 1 });
        tick += 45;
        landed += 1;
    }
    releases
}

/// B-1: against a held, unarmed target in range, the first release carries 2 strikes and then
/// one release of 1 strike follows at R + 57, R + 102, R + 147, …, §2.4's ticks, over 20
/// strikes.
#[test]
fn b1_the_magazine_throws_two_then_one_per_rebuild() -> TestResult {
    let mut nest = nest_with(Some(MAGAZINE), false, 0xB2_00D)?;
    let (lord, _) = nest.engaged(at(20, 32), at(25, 32))?;
    let first = nest.until_release(lord, 60)?;
    let mut releases = vec![first];
    releases.extend(nest.run(lord, 830));
    let strikes: u32 = releases.iter().map(|r| u32::from(r.strikes)).sum();
    assert!(
        strikes >= 20,
        "premise: 20 strikes in the window ({strikes})"
    );
    releases.truncate(19);
    assert_eq!(releases, schedule(first.tick, 20), "§2.4's release ticks");
    Ok(())
}

/// B-2: with no armed charge the weapon starts no swing, keeps its target and does not move; it
/// starts its swing on the tick a charge arms, its cooldown being over.
#[test]
fn b2_an_empty_magazine_holds_the_swing_until_a_charge_arms() -> TestResult {
    let mut nest = nest_with(Some(MAGAZINE), false, 0xB2_00D)?;
    let (lord, target) = nest.engaged(at(20, 32), at(25, 32))?;
    let first = nest.until_release(lord, 60)?;
    let position = nest.world.entities[&lord].position;
    for tick in first.tick + 1..first.tick + 54 {
        nest.step();
        let unit = &nest.world.entities[&lord];
        assert_eq!(nest.world.current_tick, tick);
        assert!(
            !matches!(unit.weapon_phase, WeaponPhase::Windup { .. }),
            "tick {tick}: no swing without an armed charge"
        );
        assert_eq!(
            unit.strike_magazine().map(|m| m.armed),
            Some(0),
            "tick {tick}: nothing armed yet"
        );
        assert!(
            matches!(unit.order_queue.front(), Some(UnitOrder::AttackTarget { target: t, .. }) if *t == target),
            "tick {tick}: the order keeps its target"
        );
        assert_eq!(
            unit.position, position,
            "tick {tick}: the lord does not move"
        );
    }
    nest.step();
    let unit = &nest.world.entities[&lord];
    assert_eq!(nest.world.current_tick, first.tick + 54);
    assert_eq!(
        unit.strike_magazine().map(|m| m.armed),
        Some(1),
        "armed at R + 54"
    );
    assert!(
        matches!(unit.weapon_phase, WeaponPhase::Windup { target_id, .. } if target_id == target),
        "the swing starts on the tick the charge arms"
    );
    Ok(())
}

/// B-3: after an idle spell of at least 108 ticks (two builds and an arming) the next release
/// carries 2 strikes again.
#[test]
fn b3_an_idle_spell_refills_the_magazine() -> TestResult {
    let mut nest = nest_with(Some(MAGAZINE), false, 0xB2_00D)?;
    let (lord, target) = nest.engaged(at(20, 32), at(25, 32))?;
    let first = nest.until_release(lord, 60)?;
    let second = nest.until_release(lord, 60)?;
    assert_eq!(
        (first.strikes, second.strikes),
        (2, 1),
        "premise: two then one"
    );
    // The target cloaks, undetected, so the stopped lord acquires nothing while idle.
    nest.world
        .entities
        .get_mut(&target)
        .ok_or("target")?
        .is_cloaked = true;
    nest.world
        .entities
        .get_mut(&lord)
        .ok_or("lord")?
        .issue_order(UnitOrder::Stop, false)?;
    let idle = nest.run(lord, 108);
    assert!(idle.is_empty(), "no release while idle: {idle:?}");
    assert_eq!(
        nest.world.entities[&lord]
            .strike_magazine()
            .map(|m| m.armed),
        Some(2),
        "both charges armed again"
    );
    nest.world
        .entities
        .get_mut(&target)
        .ok_or("target")?
        .is_cloaked = false;
    nest.attack(lord, target)?;
    let again = nest.until_release(lord, 60)?;
    assert_eq!(again.strikes, 2, "a refilled magazine throws two");
    Ok(())
}

/// B-4: each impact creates `strikes × per_strike` units at the target's position, owned by the
/// missile's owner, facing the throw, each ordered to attack the target (`Explicit`).
#[test]
fn b4_an_impact_creates_one_unit_per_strike_at_the_target() -> TestResult {
    let mut nest = nest()?;
    let (lord, target) = nest.engaged(at(20, 32), at(24, 35))?;
    let first = nest.until_release(lord, 60)?;
    assert_eq!(first.strikes, 2);
    nest.until_landed(lord, 40)?;
    let mites = nest.mites();
    assert_eq!(mites.len(), 2, "one unit per strike");
    let throw = at(24, 35) - at(20, 32);
    let heading = SimAngle::atan2(throw.y, throw.x);
    let target_pos = nest.world.entities[&target].position;
    for id in mites {
        let mite = &nest.world.entities[&id];
        assert_eq!(mite.owner, ME, "the missile's owner owns it");
        assert_eq!(mite.position, target_pos, "created at the target");
        assert_eq!(mite.heading, heading, "facing the throw");
        assert_eq!(
            mite.order_queue.front(),
            Some(&UnitOrder::AttackTarget {
                target,
                origin: AttackOrigin::Explicit
            }),
            "ordered to attack the missile's target"
        );
        let life = mite.timed_life().ok_or("a clock")?;
        assert!((89..=94).contains(&life), "89 + 0..=5 ticks: {life}");
    }
    Ok(())
}

/// How a missile in flight loses its target in B-5.
#[derive(Debug, Clone, Copy)]
enum Loss {
    Dies,
    CloaksUndetected,
    Teleports,
}

/// B-5: a target that dies, cloaks undetected or teleports while the missile flies takes no
/// strike; the units are created where the missile finished and attack-move there.
#[test]
fn b5_a_lost_target_still_yields_units_that_attack_move_to_the_finish() -> TestResult {
    for loss in [Loss::Dies, Loss::CloaksUndetected, Loss::Teleports] {
        let mut nest = nest()?;
        let (lord, target) = nest.engaged(at(20, 32), at(29, 32))?;
        nest.until_release(lord, 60)?;
        let before = nest.world.entities[&target].hp;
        let point = nest.world.entities[&target].position;
        let victim = nest.world.entities.get_mut(&target).ok_or("target")?;
        match loss {
            Loss::Dies => victim.hp = SimScalar::ZERO,
            Loss::CloaksUndetected => victim.is_cloaked = true,
            Loss::Teleports => victim.teleport_count += 1,
        }
        nest.until_landed(lord, 40)?;
        if let Some(victim) = nest.world.entities.get(&target) {
            if !matches!(loss, Loss::Dies) {
                assert_eq!(victim.hp, before, "{loss:?}: no strike landed");
            }
        }
        let mites = nest.mites();
        assert_eq!(mites.len(), 2, "{loss:?}: the units are still created");
        for id in mites {
            let mite = &nest.world.entities[&id];
            assert_eq!(mite.position, point, "{loss:?}: at the finish point");
            assert_eq!(
                mite.order_queue.front(),
                Some(&UnitOrder::AttackMove { target: point }),
                "{loss:?}: attack-moving to the finish point"
            );
        }
    }
    Ok(())
}

/// The first structure archetype of the catalog, by id.
fn a_structure(world: &SimWorld) -> Result<(UnitTypeId, u32), Box<dyn Error>> {
    let mut domes: Vec<&UnitArchetype> = world
        .catalog
        .archetypes()
        .filter(|arch| matches!(arch.shape, ShapeKind::BuildingDome { .. }))
        .collect();
    domes.sort_by_key(|arch| arch.unit_type_id);
    let dome = domes.first().ok_or("the catalog has a structure")?;
    let ShapeKind::BuildingDome { footprint_w, .. } = dome.shape else {
        return Err("a dome".into());
    };
    Ok((dome.unit_type_id, u32::from(footprint_w)))
}

/// B-6: at a structure target the units appear at the nearest valid point within 5 (outside the
/// footprint); with a search range of 0 the blocked centre creates nothing and reports
/// `ImpactSpawnBlocked`.
#[test]
fn b6_a_structure_target_moves_the_units_out_or_blocks_them() -> TestResult {
    for range in [5, 0] {
        let mut nest = nest()?;
        if range == 0 {
            let arch = nest.world.catalog.get_mut(nest.lord)?;
            for weapon in [&mut arch.weapon, &mut arch.ground_weapon]
                .into_iter()
                .flatten()
            {
                if let Some(spawn) = weapon.impact_spawn.as_mut() {
                    spawn.search_range = SimScalar::ZERO;
                }
            }
        }
        let (dome, width) = a_structure(&nest.world)?;
        let structure = nest.world.spawn_structure(dome, FOE, 30, 30)?;
        let centre = nest.world.entities[&structure].position;
        let lord = nest.world.spawn_unit(nest.lord, ME, at(25, 32))?;
        nest.attack(lord, structure)?;
        nest.until_release(lord, 80)?;
        let mut blocked = 0;
        for _ in 0..40 {
            nest.step();
            blocked += nest
                .world
                .diagnostics()
                .iter()
                .filter(|d| matches!(d, SimulationDiagnostic::ImpactSpawnBlocked { .. }))
                .count();
            if !nest
                .world
                .projectiles
                .iter()
                .any(|p| p.attacker_id == Some(lord))
            {
                break;
            }
        }
        let mites = nest.mites();
        if range == 0 {
            assert!(mites.is_empty(), "nothing created inside the footprint");
            assert_eq!(blocked, 2, "each blocked unit reported");
            continue;
        }
        assert_eq!(mites.len(), 2);
        let half = SimScalar::from_num(f64::from(width) / 2.0);
        for id in mites {
            let p = nest.world.entities[&id].position;
            let d = p.distance(centre);
            assert!(
                d <= SimScalar::from_i32(5),
                "within the search range: {d:?}"
            );
            assert!(d >= half, "outside the footprint: {d:?}");
        }
        assert_eq!(blocked, 0);
    }
    Ok(())
}

/// Two lords at equal range from one dummy, released together, whose missiles splash their own
/// side within 2: the mites one missile creates are not struck by the other's splash of the
/// same tick.
/// One created unit as B-7 compares it: id, position, heading, clock and life.
type Created = (EntityId, SimVec2, SimAngle, Option<u16>, SimScalar);

fn twin_volley(seed: u64) -> Result<Vec<Created>, Box<dyn Error>> {
    let mut nest = nest_with(Some(MAGAZINE), true, seed)?;
    let splash = SplashSpec {
        zones: vec![SplashZone {
            radius: SimScalar::from_i32(2),
            fraction: SimScalar::ONE,
        }],
        shape: SplashShape::Circle,
        friendly_fire: true,
        offset_by_unit_radius: false,
        impact: SplashImpact::Target,
        target_radius_below: None,
        cliff: SplashCliff::Same,
        strike_target_on_other_level: false,
    };
    let arch = nest.world.catalog.get_mut(nest.lord)?;
    for weapon in [&mut arch.weapon, &mut arch.ground_weapon]
        .into_iter()
        .flatten()
    {
        weapon.splash = Some(splash.clone());
    }
    let (a, target) = nest.engaged(at(25, 32), at(32, 32))?;
    let b = nest.world.spawn_unit(nest.lord, ME, at(39, 32))?;
    nest.attack(b, target)?;
    nest.until_release(a, 60)?;
    let a_flight: Vec<u32> = nest.world.projectiles.iter().map(|p| p.id).collect();
    assert_eq!(
        a_flight.len(),
        2,
        "premise: both lords released on one tick"
    );
    for _ in 0..40 {
        nest.step();
        if !nest.mites().is_empty() {
            break;
        }
    }
    assert!(
        nest.world.projectiles.is_empty(),
        "premise: both landed on one tick"
    );
    assert_eq!(
        nest.mites().len(),
        4,
        "premise: both missiles finished on one tick"
    );
    Ok(nest
        .mites()
        .into_iter()
        .map(|id| {
            let mite = &nest.world.entities[&id];
            (id, mite.position, mite.heading, mite.timed_life(), mite.hp)
        })
        .collect())
}

/// B-7: a splash resolving on the impact tick strikes no unit created that tick; two impacts of
/// one tick create ids in the impact-resolution order (descending launch order: the later
/// launch first); the same seed gives the same ids and clocks.
#[test]
fn b7_spawns_commit_after_every_impact_in_impact_order() -> TestResult {
    let first = twin_volley(7)?;
    assert_eq!(first.len(), 4, "two units per missile");
    for (id, _, _, _, hp) in &first {
        assert_eq!(
            *hp,
            SimScalar::from_i32(20),
            "{id:?}: untouched by the same tick's splash"
        );
    }
    // The later launch (the lord with the higher id, thrown along -x) resolves first.
    let headings: Vec<SimAngle> = first.iter().map(|m| m.2).collect();
    assert_eq!(
        headings,
        [
            SimAngle::DEG_180,
            SimAngle::DEG_180,
            SimAngle::ZERO,
            SimAngle::ZERO
        ],
        "ids in impact-resolution order"
    );
    assert_eq!(first, twin_volley(7)?, "the same seed gives the same units");
    Ok(())
}

/// B-8: a created unit dies on tick T + 89 + d, d in 0..=5, with no damage taken, and its death
/// shows its shatter; over 200 spawns every d from 0 to 5 occurs and no other; the same seed
/// gives the same draws.
#[test]
fn b8_the_clock_kills_on_its_tick_with_every_extra_drawn() -> TestResult {
    let draws = clock_draws(0x5EED)?;
    assert!(draws.len() >= 200, "premise: 200 spawns ({})", draws.len());
    for d in 0..=5 {
        assert!(draws.contains(&d), "the extra {d} occurs");
    }
    assert!(draws.iter().all(|d| *d <= 5), "no extra beyond 5");
    assert_eq!(
        draws,
        clock_draws(0x5EED)?,
        "the same seed draws the same extras"
    );
    expiry_tick()
}

/// The extra ticks drawn for the first 200 units a fast, ungated lord creates.
fn clock_draws(seed: u64) -> Result<Vec<u16>, Box<dyn Error>> {
    let mut nest = nest_with(None, true, seed)?;
    nest.world.catalog.get_mut(nest.dummy)?.max_hp = SimScalar::from_i32(1_000_000);
    let arch = nest.world.catalog.get_mut(nest.lord)?;
    for weapon in [&mut arch.weapon, &mut arch.ground_weapon]
        .into_iter()
        .flatten()
    {
        weapon.period_ticks = 12;
    }
    nest.engaged(at(20, 32), at(25, 32))?;
    let mut draws = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..2000 {
        nest.step();
        for id in nest.mites() {
            if seen.insert(id) {
                let life = nest.world.entities[&id].timed_life().ok_or("a clock")?;
                draws.push(life - 89);
            }
        }
        if draws.len() >= 200 {
            break;
        }
    }
    draws.truncate(200);
    Ok(draws)
}

/// One created unit, followed from its creation on tick T: alive and undamaged through
/// T + 88 + d, gone with its shatter on T + 89 + d.
fn expiry_tick() -> TestResult {
    let mut nest = nest()?;
    let (lord, _) = nest.engaged(at(20, 32), at(25, 32))?;
    nest.until_release(lord, 60)?;
    let mut created = None;
    for _ in 0..40 {
        nest.step();
        if let Some(&id) = nest.mites().first() {
            created = Some((id, nest.world.current_tick));
            break;
        }
    }
    let (id, born) = created.ok_or("a unit was created")?;
    let life = nest.world.entities[&id].timed_life().ok_or("a clock")?;
    let dies = born + u32::from(life);
    // The lord's later volleys would spawn more; stop it so only this unit's clock matters.
    nest.world
        .entities
        .get_mut(&lord)
        .ok_or("lord")?
        .issue_order(UnitOrder::Stop, false)?;
    while nest.world.current_tick < dies - 1 {
        nest.step();
        let unit = nest
            .world
            .entities
            .get(&id)
            .ok_or("alive before its tick")?;
        assert!(unit.is_alive());
    }
    let out = nest.step();
    assert_eq!(nest.world.current_tick, dies);
    assert!(!nest.world.entities.contains_key(&id), "dead on T + 89 + d");
    let shattered = out.combat_events.iter().any(|event| {
        matches!(event, CombatEventVisual::BubbleShatter { unit_type, .. } if *unit_type == nest.mite)
    });
    assert!(shattered, "its ordinary death");
    Ok(())
}

/// B-9: a `Fizzle` missile whose target is lost creates nothing.
#[test]
fn b9_a_fizzled_missile_creates_nothing() -> TestResult {
    let mut nest = nest()?;
    let arch = nest.world.catalog.get_mut(nest.lord)?;
    for weapon in [&mut arch.weapon, &mut arch.ground_weapon]
        .into_iter()
        .flatten()
    {
        weapon.target_loss_behavior = ProjectileTargetLossBehavior::Fizzle;
    }
    let (lord, target) = nest.engaged(at(20, 32), at(29, 32))?;
    nest.until_release(lord, 60)?;
    nest.world.entities.get_mut(&target).ok_or("target")?.hp = SimScalar::ZERO;
    nest.until_landed(lord, 40)?;
    assert!(
        nest.mites().is_empty(),
        "a missile that never arrived finishes nowhere"
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The shipped rows (plan `2026-11-28-broodlings` §6.2, WP3 and WP4)
// ---------------------------------------------------------------------------------------------

/// The one shipped archetype with a strike magazine: the Brood Lord analogue, found by property.
fn shipped_lord(world: &SimWorld) -> Result<UnitTypeId, Box<dyn Error>> {
    let lords: Vec<UnitTypeId> = world
        .catalog
        .archetypes()
        .filter(|arch| arch.capabilities.strike_magazine.is_some())
        .map(|arch| arch.unit_type_id)
        .collect();
    match lords.as_slice() {
        [lord] => Ok(*lord),
        _ => Err(format!("one shipped unit with a strike magazine, found {lords:?}").into()),
    }
}

/// A world of the shipped catalog: its Brood Lord analogue unchanged, the unarmed durable dummy
/// of [`nest_with`], and the shipped spawned unit (the dummy's type while none is shipped).
fn shipped_nest(seed: u64) -> Result<Nest, Box<dyn Error>> {
    let mut nest = nest_with(None, false, seed)?;
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let dummy_arch = world.catalog.get_mut(nest.dummy)?;
    dummy_arch.weapon = None;
    dummy_arch.ground_weapon = None;
    dummy_arch.air_weapon = None;
    dummy_arch.max_hp = SimScalar::from_i32(100_000);
    nest.lord = shipped_lord(&world)?;
    nest.mite = world
        .catalog
        .get(nest.lord)?
        .ground_weapon
        .as_ref()
        .and_then(|w| w.impact_spawn)
        .map_or(nest.dummy, |spawn| spawn.unit);
    nest.world = world;
    Ok(nest)
}

/// B-12 (WP3): the shipped Brood Lord analogue's magazine is 5.0.14's `BroodLordHangar` (S3:
/// `CountStart 2`, `Ammo1 Time 2.5` = 45 ticks; S4: `QueueSize 2`, one at a time; S6:
/// `BroodlingAttackDelay 0.5` = 9 ticks; S20: two at most), and the shipped unit reproduces B-1
/// to B-3: two strikes on its first release, one on R + 57, then one per rebuild (45 ticks, or the
/// period with the weapon's random delay when that is longer), no swing while nothing is armed,
/// and two again after an idle spell.
#[test]
fn b12_the_shipped_brood_lord_throws_its_escorts_on_the_plan_ticks() -> TestResult {
    let mut nest = shipped_nest(0xB12)?;
    assert_eq!(
        nest.world
            .catalog
            .get(nest.lord)?
            .capabilities
            .strike_magazine,
        Some(MAGAZINE),
        "BroodLordHangar, liberty AbilData:3808-3824; BroodLordQueue2 :2077-2082; \
         BroodlingAttackDelay, liberty BehaviorData:1361-1368"
    );
    let (lord, target) = nest.engaged(at(20, 32), at(25, 32))?;
    let first = nest.until_release(lord, 60)?;
    let mut releases = vec![first];
    for _ in first.tick + 1..first.tick + 54 {
        nest.step();
        releases.extend(nest.releases(lord));
        assert!(
            !matches!(
                nest.world.entities[&lord].weapon_phase,
                WeaponPhase::Windup { .. }
            ),
            "no swing while nothing is armed"
        );
    }
    releases.extend(nest.run(lord, 790));
    releases.truncate(19);
    let plan = schedule(first.tick, 20);
    assert_eq!(releases.len(), plan.len(), "20 strikes: {releases:?}");
    // The shipped weapon draws its random delay (-1.1161 to 2.2321 ticks, B-3) into the cooldown
    // of each release, so a swing may wait for its cooldown past the tick its escort arms: after
    // the first two releases, which the magazine alone places, each follows the last by the
    // escort's 45 ticks or the period with its delay, 43 to 48 ticks.
    assert_eq!(
        &releases[..2],
        &plan[..2],
        "the magazine's first two releases"
    );
    for pair in releases[1..].windows(2) {
        assert_eq!(pair[1].strikes, 1, "one escort per rebuild: {pair:?}");
        assert!(
            (43..=48).contains(&(pair[1].tick - pair[0].tick)),
            "one escort per rebuild or per delayed period: {pair:?}"
        );
    }
    // B-3: an idle spell refills both escorts.
    nest.world
        .entities
        .get_mut(&target)
        .ok_or("target")?
        .is_cloaked = true;
    nest.world
        .entities
        .get_mut(&lord)
        .ok_or("lord")?
        .issue_order(UnitOrder::Stop, false)?;
    nest.run(lord, 120);
    nest.world
        .entities
        .get_mut(&target)
        .ok_or("target")?
        .is_cloaked = false;
    nest.attack(lord, target)?;
    assert_eq!(nest.until_release(lord, 60)?.strikes, 2, "refilled");
    Ok(())
}

impl Nest {
    /// Steps with `commands`, as [`Self::step`].
    fn step_with(&mut self, commands: Vec<PlayerCommand>) -> RenderSnapshot {
        self.tick += 1;
        let mut out = RenderSnapshot::default();
        self.world.step_into(
            &TickBundle {
                tick: Tick(self.tick),
                commands,
                state_checksum: None,
            },
            &mut out,
        );
        out
    }
}

/// B-13 (WP4): the shipped Brood Lord analogue's escorts land as the shipped spawned archetype,
/// one per strike, with 89 + 0..=5 ticks of life and the attack order; the created unit fights,
/// its owner selects and commands it (a `PlayerCommand` move is admitted), and an enemy's
/// automatic targeting attacks it.
#[test]
fn b13_the_shipped_escorts_land_as_broodlings_that_fight_and_obey() -> TestResult {
    let mut nest = shipped_nest(0xB13)?;
    assert_ne!(
        nest.mite, nest.dummy,
        "premise: a spawned archetype is shipped"
    );
    let (lord, target) = nest.engaged(at(20, 32), at(25, 32))?;
    let first = nest.until_release(lord, 60)?;
    assert_eq!(first.strikes, 2);
    nest.until_landed(lord, 40)?;
    let mites = nest.mites();
    assert_eq!(mites.len(), 2, "one per escort");
    for id in &mites {
        let mite = &nest.world.entities[id];
        let life = mite.timed_life().ok_or("a clock")?;
        assert!((89..=94).contains(&life), "89 + 0..=5: {life}");
        assert_eq!(
            mite.order_queue.front(),
            Some(&UnitOrder::AttackTarget {
                target,
                origin: AttackOrigin::Explicit
            })
        );
    }
    let arch = nest.world.catalog.get(nest.mite)?;
    assert!(arch.selectable && arch.order_targetable, "an ordinary unit");
    // Without the lord, the target's life falls by the Broodlings' claws alone.
    nest.world.entities.get_mut(&lord).ok_or("lord")?.hp = SimScalar::ZERO;
    nest.step();
    let before = nest.world.entities[&target].hp;
    for _ in 0..30 {
        nest.step();
    }
    assert!(
        nest.world.entities[&target].hp < before,
        "the Broodlings fight"
    );
    // Their owner commands one.
    let mover = mites[0];
    let destination = at(10, 10);
    nest.step_with(vec![PlayerCommand {
        player_id: ME,
        entities: vec![mover],
        order: UnitOrder::Move {
            target: destination,
        },
        queue: false,
    }]);
    assert_eq!(
        nest.world.entities[&mover].order_queue.front(),
        Some(&UnitOrder::Move {
            target: destination
        }),
        "the owner's move is admitted"
    );
    // An armed enemy acquires one by itself.
    let [_, _, armed] = pick(&nest.world)?;
    let hunter = nest.world.spawn_unit(armed, FOE, at(27, 32))?;
    let mut acquired = false;
    for _ in 0..20 {
        nest.step();
        acquired |= nest
            .world
            .entities
            .get(&hunter)
            .is_some_and(|h| h.current_target.is_some_and(|t| mites.contains(&t)));
    }
    assert!(
        acquired,
        "an enemy's automatic targeting takes the Broodling"
    );
    Ok(())
}

/// B-15 (WP4): spawns capture no supply; the supply ledger validates on every tick of a
/// 600-tick Brood Lord fight and the owner's supply never moves.
#[test]
fn b15_spawns_capture_no_supply() -> TestResult {
    let mut nest = shipped_nest(0xB15)?;
    let (lord, _) = nest.engaged(at(20, 32), at(25, 32))?;
    let slot = ME.slot().ok_or("a player")?;
    let used = nest.world.player_resources[slot].supply_used;
    let mut created = std::collections::BTreeSet::new();
    for _ in 0..600 {
        nest.step();
        nest.world.validate_supply_ledger()?;
        assert_eq!(nest.world.player_resources[slot].supply_used, used);
        created.extend(nest.mites());
    }
    assert!(nest.world.entities.contains_key(&lord));
    assert!(
        created.len() >= 10,
        "Broodlings came and went: {}",
        created.len()
    );
    Ok(())
}
