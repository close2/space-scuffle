//! Hangars, launch weapons and launched members through `SimWorld` and `PlayerCommand` (plan
//! `2026-11-28-carrier-interceptors` §6.2, H-2 to H-9 and H-11's determinism).
//!
//! No shipped row carries a hangar until WP3, so every test authors one on the loaded catalog of
//! its own world: a host air unit gets a hangar and a launch weapon, a member air unit an instant
//! beam, both found by property, never by slug (charter §3.3). The authored values are the 5.0.14
//! Carrier's (plan §2.2): 8 members, 4 at the start, 214 ticks and 15 minerals each, a queue of 5,
//! leash 12, 8 tries 7 ticks apart; the beam strikes twice for 5 from range 2 every 54 ticks.

use std::error::Error;

use sim_core::catalog::UnitArchetype;
use sim_core::economy::GameCommandError;
use sim_core::hangar::HangarRejection;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackVisibility, AttributeFlags, CombatEventVisual, EntityId, HangarDeath, HangarSpec,
    MagazineLaunch, MoverType, PlayerCommand, PlayerId, PlayerSlot, RenderSnapshot, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId, WeaponSpec, WeaponVisualKind,
};

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);
const FOE: PlayerId = PlayerId(1);
const LAUNCH: MagazineLaunch = MagazineLaunch {
    tries: 8,
    interval_ticks: 7,
};

/// A world with an authored hangar: the host's and the member's types, and the passive ground
/// target type and the air-capable killer type the tests spawn as enemies.
struct Bay {
    world: SimWorld,
    host: UnitTypeId,
    member: UnitTypeId,
    dummy: UnitTypeId,
    killer: UnitTypeId,
    tick: u32,
}

fn is_plain_unit(arch: &UnitArchetype) -> bool {
    !arch.attributes.contains(AttributeFlags::STRUCTURE)
        && arch.max_speed > SimScalar::ZERO
        && arch.capabilities.transport_capacity == 0
        && arch.harvester.is_none()
        && arch.capabilities.modes.is_empty()
        && arch.modes.is_empty()
}

/// The first two plain armed air units and the first two plain ground units, by id.
fn pick(world: &SimWorld) -> Result<[UnitTypeId; 4], Box<dyn Error>> {
    let mut ids: Vec<&UnitArchetype> = world
        .catalog
        .archetypes()
        .filter(|arch| is_plain_unit(arch) && arch.weapon.is_some())
        .collect();
    ids.sort_by_key(|arch| arch.unit_type_id);
    let air: Vec<_> = ids
        .iter()
        .filter(|a| a.mover_type == MoverType::Air)
        .collect();
    let ground: Vec<_> = ids
        .iter()
        .filter(|a| a.mover_type == MoverType::Ground)
        .collect();
    match (air.as_slice(), ground.as_slice()) {
        ([host, member, ..], [dummy, killer, ..]) => Ok([
            host.unit_type_id,
            member.unit_type_id,
            dummy.unit_type_id,
            killer.unit_type_id,
        ]),
        _ => Err("the shipped catalog has two plain armed air and ground units".into()),
    }
}

/// An instant weapon of every plane: `base` with the given range, strikes, damage and timing.
fn instant(base: &WeaponSpec, range: i32, strikes: u8, damage: i32, period: u16) -> WeaponSpec {
    let mut weapon = base.clone();
    weapon.visual_kind = WeaponVisualKind::LaserBeam;
    weapon.target_flags = AttributeFlags::all();
    weapon.min_range = SimScalar::ZERO;
    weapon.max_range = SimScalar::from_i32(range);
    weapon.base_damage = SimScalar::from_i32(damage);
    weapon.attribute_bonuses = sim_types::AttributeBonuses::default();
    weapon.attacks_count = strikes;
    weapon.strike_interval_ticks = 0;
    weapon.first_strike_delay_ticks = 0;
    weapon.period_ticks = period;
    weapon.damage_point_ticks = 3;
    weapon.backswing_ticks = 9;
    weapon.random_delay_min = SimScalar::ZERO;
    weapon.random_delay_max = SimScalar::ZERO;
    weapon.splash = None;
    weapon.projectile_speed = None;
    weapon.missile_movers = Vec::new();
    weapon.magazine_launch = None;
    weapon.firing_arc = sim_types::SimAngle::from_raw(u16::MAX);
    weapon.acquire_arc = sim_types::SimAngle::from_raw(u16::MAX);
    weapon.attack_visibility = AttackVisibility::Visible { reveal_ticks: 54 };
    weapon.launch_visibility = AttackVisibility::Snapshot;
    weapon
}

fn arm(arch: &mut UnitArchetype, weapon: WeaponSpec) {
    arch.weapon = Some(weapon.clone());
    arch.ground_weapon = Some(weapon.clone());
    arch.air_weapon = Some(weapon);
}

fn bay_with(spec: impl Fn(UnitTypeId) -> HangarSpec) -> Result<Bay, Box<dyn Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 0x0B_A7);
    let [host, member, dummy, killer] = pick(&world)?;
    let base = world
        .catalog
        .get(host)?
        .weapon
        .clone()
        .ok_or("the host is armed")?;
    let mut launch = instant(&base, 8, 1, 0, 9);
    launch.visual_kind = WeaponVisualKind::None;
    launch.damage_point_ticks = 0;
    launch.backswing_ticks = 0;
    launch.attack_visibility = AttackVisibility::Hidden;
    launch.launch_visibility = AttackVisibility::Hidden;
    launch.magazine_launch = Some(LAUNCH);
    let beam = instant(&base, 2, 2, 5, 54);
    let host_arch = world.catalog.get_mut(host)?;
    arm(host_arch, launch);
    host_arch.capabilities.hangar = Some(spec(member));
    let member_arch = world.catalog.get_mut(member)?;
    arm(member_arch, beam.clone());
    member_arch.selectable = false;
    member_arch.order_targetable = false;
    member_arch.supply_cost = 0;
    member_arch.mineral_cost = 15;
    member_arch.gas_cost = 0;
    let dummy_arch = world.catalog.get_mut(dummy)?;
    dummy_arch.weapon = None;
    dummy_arch.ground_weapon = None;
    dummy_arch.air_weapon = None;
    dummy_arch.max_hp = SimScalar::from_i32(2000);
    let killer_arch = world.catalog.get_mut(killer)?;
    arm(killer_arch, instant(&base, 6, 1, 60, 10));
    for slot in PlayerSlot::ALL {
        world.player_resources[slot].minerals = 0;
        world.player_resources[slot].gas = 0;
    }
    Ok(Bay {
        world,
        host,
        member,
        dummy,
        killer,
        tick: 0,
    })
}

fn carrier_spec(member: UnitTypeId) -> HangarSpec {
    HangarSpec {
        member,
        max_count: 8,
        start_count: 4,
        build_ticks: 214,
        queue_size: 5,
        auto_build: true,
        auto_build_toggle: true,
        leash: SimScalar::from_i32(12),
        on_death: HangarDeath::KillMembers,
    }
}

fn bay() -> Result<Bay, Box<dyn Error>> {
    bay_with(carrier_spec)
}

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::from_i32(x, y)
}

impl Bay {
    fn minerals(&mut self, player: PlayerId, minerals: u32) -> TestResult {
        let slot = player.slot().ok_or("a player")?;
        self.world.player_resources[slot].minerals = minerals;
        Ok(())
    }

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

    fn step(&mut self) -> RenderSnapshot {
        self.step_with(Vec::new())
    }

    fn steps(&mut self, n: u32) {
        for _ in 0..n {
            self.step();
        }
    }

    /// The living members linked to `host`, ascending.
    fn members(&self, host: EntityId) -> Vec<EntityId> {
        self.world
            .entities
            .values()
            .filter(|e| e.hangar_link().is_some_and(|link| link.hangar == host))
            .map(|e| e.id)
            .collect()
    }

    fn docked(&self, host: EntityId) -> usize {
        self.world.entities[&host]
            .hangar()
            .map_or(0, |state| state.docked.len())
    }

    fn attack(&mut self, unit: EntityId, target: EntityId) -> TestResult {
        let entity = self.world.entities.get_mut(&unit).ok_or("unit")?;
        entity.issue_order(UnitOrder::attack(target), false)?;
        Ok(())
    }

    /// A host of `ME` at (20, 32) attacking a passive enemy at (26, 32), in launch range.
    fn engaged(&mut self) -> Result<(EntityId, EntityId), Box<dyn Error>> {
        let host = self.world.spawn_unit(self.host, ME, at(20, 32))?;
        let target = self.world.spawn_unit(self.dummy, FOE, at(26, 32))?;
        self.attack(host, target)?;
        Ok((host, target))
    }
}

fn command(player: PlayerId, entities: Vec<EntityId>, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities,
        order,
        queue: false,
    }
}

/// H-2: a spawned hangar unit has its free start members docked at full vitals; none is an
/// entity, and nothing is launched or queued.
#[test]
fn a_new_hangar_unit_docks_its_start_members_at_full_vitals() -> TestResult {
    let mut bay = bay()?;
    let host = bay.world.spawn_unit(bay.host, ME, at(20, 32))?;
    let member = bay.world.catalog.get(bay.member)?;
    let (max_hp, max_shields) = (member.max_hp, member.max_shields);
    let state = bay.world.entities[&host].hangar().ok_or("a hangar")?;
    assert_eq!(state.docked.len(), 4);
    assert!(state
        .docked
        .iter()
        .all(|record| record.hp == max_hp && record.shields == max_shields));
    assert_eq!(
        (state.launched, state.queued, state.build_progress),
        (0, 0, 0)
    );
    assert_eq!(bay.members(host), Vec::<EntityId>::new());
    assert!(bay
        .world
        .entities
        .values()
        .all(|e| e.unit_type_id != bay.member));
    Ok(())
}

/// H-3: with resources the hangar queues to `min(queue_size, max_count - docked - launched)`,
/// paying each build at queueing, completes one every `build_ticks`, and never exceeds
/// `max_count`; without resources it waits for the first tick the owner can pay.
#[test]
fn auto_build_queues_pays_and_completes_one_member_per_build_time() -> TestResult {
    let mut bay = bay()?;
    bay.minerals(ME, 1000)?;
    let host = bay.world.spawn_unit(bay.host, ME, at(20, 32))?;
    bay.step();
    let state = bay.world.entities[&host].hangar().ok_or("a hangar")?;
    assert_eq!(state.queued, 4, "8 - 4 docked, below the queue of 5");
    let slot = ME.slot().ok_or("slot")?;
    assert_eq!(bay.world.player_resources[slot].minerals, 1000 - 4 * 15);
    let mut completions = Vec::new();
    for _ in 0..900 {
        let before = bay.docked(host);
        bay.step();
        if bay.docked(host) > before {
            completions.push(bay.tick);
        }
        let state = bay.world.entities[&host].hangar().ok_or("a hangar")?;
        assert!(state.total() <= 8);
    }
    assert_eq!(completions, vec![215, 429, 643, 857]);
    assert_eq!(bay.docked(host), 8);
    assert_eq!(bay.world.player_resources[slot].minerals, 1000 - 4 * 15);

    let mut poor = bay_with(carrier_spec)?;
    let host = poor.world.spawn_unit(poor.host, ME, at(20, 32))?;
    poor.steps(30);
    assert_eq!(
        poor.world.entities[&host].hangar().ok_or("hangar")?.queued,
        0
    );
    poor.minerals(ME, 15)?;
    poor.step();
    assert_eq!(
        poor.world.entities[&host].hangar().ok_or("hangar")?.queued,
        1
    );
    assert_eq!(poor.world.player_resources[slot].minerals, 0);
    Ok(())
}

/// H-3: a hangar that does not build by itself queues nothing, whatever its owner holds.
#[test]
fn a_hangar_without_auto_build_queues_nothing() -> TestResult {
    let mut bay = bay_with(|member| HangarSpec {
        auto_build: false,
        ..carrier_spec(member)
    })?;
    bay.minerals(ME, 1000)?;
    let host = bay.world.spawn_unit(bay.host, ME, at(20, 32))?;
    bay.steps(300);
    let state = bay.world.entities[&host].hangar().ok_or("hangar")?;
    assert_eq!((state.queued, state.docked.len()), (0, 4));
    Ok(())
}

/// H-4: a release at an enemy in range launches the first member on the release tick at the
/// hangar unit's centre and the next every 7 ticks, never more than are docked; the weapon's
/// releases during the running channel (every 9 ticks) start nothing.
#[test]
fn a_release_launches_one_member_per_interval_from_the_centre() -> TestResult {
    let mut bay = bay()?;
    let (host, _) = bay.engaged()?;
    let mut launches = Vec::new();
    for _ in 0..80 {
        let before = bay.members(host);
        bay.step();
        let after = bay.members(host);
        for id in after.iter().filter(|id| !before.contains(id)) {
            launches.push(bay.tick);
            // Where the member stood when the movement phase began (a launch in the hangar
            // phase) or stands now (the release's launch in the weapon phase): the hangar
            // unit's centre then.
            let (home, unit) = (&bay.world.entities[&host], &bay.world.entities[id]);
            assert!(
                unit.pos_at_tick_start == home.pos_at_tick_start || unit.position == home.position,
                "launched at the centre"
            );
        }
    }
    assert_eq!(launches.len(), 4, "only the four docked launch");
    let first = launches[0];
    assert_eq!(launches, vec![first, first + 7, first + 14, first + 21]);
    Ok(())
}

/// H-5: launched members attack the hangar's target with their own weapon (two strikes of 5 on
/// one tick); the struck player sees the members, never the hangar unit.
#[test]
fn members_strike_the_target_and_reveal_themselves_not_the_hangar() -> TestResult {
    let mut bay = bay()?;
    let (host, target) = bay.engaged()?;
    let full = bay.world.entities[&target].hp + bay.world.entities[&target].shields;
    bay.steps(150);
    let left = bay.world.entities[&target].hp + bay.world.entities[&target].shields;
    let lost = full - left;
    assert!(lost > SimScalar::ZERO, "members strike");
    assert_eq!(
        lost.raw() % SimScalar::from_i32(10).raw(),
        0,
        "every hit is two strikes of 5 on one tick (armour 0 premise): {lost:?}"
    );
    assert!(bay
        .members(host)
        .iter()
        .any(|m| bay.world.entities[m].is_attack_revealed_to(FOE)));
    assert!(!bay.world.entities[&host].is_attack_revealed_to(FOE));
    Ok(())
}

/// H-6: when the hangar's target dies and no new one comes, every member returns and docks on
/// the tick it touches the hangar unit, its vitals stored.
#[test]
fn members_return_and_dock_when_the_target_is_gone() -> TestResult {
    let mut bay = bay()?;
    let (host, target) = bay.engaged()?;
    bay.steps(40);
    assert_eq!(bay.members(host).len(), 4);
    bay.world.entities.get_mut(&target).ok_or("target")?.hp = SimScalar::ZERO;
    // The target's own death shatters; a dock never does (D-11 item 3: docking is not dying).
    let mut shatters = 0;
    for _ in 0..200 {
        let out = bay.step();
        shatters += out
            .combat_events
            .iter()
            .filter(|event| matches!(event, CombatEventVisual::BubbleShatter { .. }))
            .count();
        if bay.members(host).is_empty() {
            break;
        }
    }
    assert_eq!(shatters, 1, "only the target shatters");
    assert!(bay.members(host).is_empty(), "every member docked");
    let state = bay.world.entities[&host].hangar().ok_or("hangar")?;
    assert_eq!((state.docked.len(), state.launched), (4, 0));
    assert_eq!(state.target, None);
    Ok(())
}

/// H-6: the hangar unit's next release at a new target sends the launched members at it.
#[test]
fn a_new_release_sends_the_members_at_the_new_target() -> TestResult {
    let mut bay = bay()?;
    let (host, first) = bay.engaged()?;
    bay.steps(40);
    let second = bay.world.spawn_unit(bay.dummy, FOE, at(20, 38))?;
    bay.world.entities.get_mut(&first).ok_or("first")?.hp = SimScalar::ZERO;
    bay.attack(host, second)?;
    bay.steps(12);
    let members = bay.members(host);
    assert_ne!(members, Vec::<EntityId>::new());
    for member in members {
        assert!(matches!(
            bay.world.entities[&member].order_queue.front(),
            Some(UnitOrder::AttackTarget { target, .. }) if *target == second
        ));
    }
    Ok(())
}

/// H-7: a Move given to the hangar unit alone recalls nothing; once it is beyond the leash a
/// member returns.
#[test]
fn the_leash_recalls_members_and_a_move_alone_does_not() -> TestResult {
    let mut bay = bay()?;
    let (host, _) = bay.engaged()?;
    bay.steps(40);
    let away = at(60, 32);
    bay.step_with(vec![command(
        ME,
        vec![host],
        UnitOrder::Move { target: away },
    )]);
    let members = bay.members(host);
    assert_ne!(members, Vec::<EntityId>::new());
    assert!(members.iter().all(|m| bay.world.entities[m]
        .hangar_link()
        .is_some_and(|link| !link.returning)));
    let mut leashed = false;
    for _ in 0..600 {
        bay.step();
        // The positions the hangar phase read: it runs before the movement phase.
        let home = bay.world.entities[&host].pos_at_tick_start;
        for m in bay.members(host) {
            let member = &bay.world.entities[&m];
            if member.pos_at_tick_start.distance(home) > SimScalar::from_i32(12) {
                leashed = true;
                assert!(member.hangar_link().is_some_and(|link| link.returning));
                assert!(matches!(
                    member.order_queue.front(),
                    Some(UnitOrder::Follow { target, .. }) if *target == host
                ));
            }
        }
    }
    assert!(leashed, "the host outran its members");
    Ok(())
}

/// H-8: the hangar unit dies: every launched member dies on the same tick with its own shatter;
/// docked and queued members go with the hangar.
#[test]
fn members_die_with_their_hangar_unit() -> TestResult {
    let mut bay = bay()?;
    let (host, _) = bay.engaged()?;
    bay.steps(30);
    let members = bay.members(host);
    assert!(members.len() >= 2);
    bay.world.entities.get_mut(&host).ok_or("host")?.hp = SimScalar::ZERO;
    let out = bay.step();
    assert!(!bay.world.entities.contains_key(&host));
    for member in &members {
        assert!(!bay.world.entities.contains_key(member));
    }
    let shatters = out
        .combat_events
        .iter()
        .filter(|event| matches!(event, CombatEventVisual::BubbleShatter { .. }))
        .count();
    assert_eq!(shatters, members.len() + 1);
    Ok(())
}

/// H-8: a member killed is replaced by auto-build, at cost.
#[test]
fn a_dead_member_is_rebuilt_at_cost() -> TestResult {
    let mut bay = bay_with(|member| HangarSpec {
        max_count: 4,
        ..carrier_spec(member)
    })?;
    let (host, _) = bay.engaged()?;
    bay.steps(30);
    let member = *bay.members(host).first().ok_or("a member")?;
    bay.world.entities.get_mut(&member).ok_or("member")?.hp = SimScalar::ZERO;
    bay.minerals(ME, 100)?;
    bay.step();
    bay.step();
    let state = bay.world.entities[&host].hangar().ok_or("hangar")?;
    assert_eq!(state.queued, 1);
    let slot = ME.slot().ok_or("slot")?;
    assert_eq!(bay.world.player_resources[slot].minerals, 85);
    Ok(())
}

/// H-9: a command naming only members is refused; one naming the hangar unit and a member
/// reaches the hangar unit alone; an order at a member is refused; an enemy with no orders
/// acquires and kills members.
#[test]
fn members_take_no_player_orders_but_enemies_shoot_them() -> TestResult {
    let mut bay = bay()?;
    let (host, _) = bay.engaged()?;
    bay.steps(30);
    let member = *bay.members(host).first().ok_or("a member")?;
    let away = at(10, 10);
    bay.step_with(vec![command(
        ME,
        vec![member],
        UnitOrder::Move { target: away },
    )]);
    assert!(bay
        .world
        .game_command_outcomes()
        .iter()
        .any(|o| o.result == Err(GameCommandError::UnselectableRecipient(member))));
    let killer = bay.world.spawn_unit(bay.killer, FOE, at(30, 32))?;
    bay.step_with(vec![command(FOE, vec![killer], UnitOrder::attack(member))]);
    assert!(bay
        .world
        .game_command_outcomes()
        .iter()
        .any(|o| o.result == Err(GameCommandError::UntargetableTarget(member))));
    let before = bay.members(host).len();
    bay.steps(120);
    let killed = bay
        .world
        .entities
        .get(&member)
        .is_none_or(|m| !m.is_alive());
    assert!(
        killed || bay.members(host).len() < before,
        "the enemy kills members"
    );
    Ok(())
}

/// H-9: a command naming the hangar unit and a member reaches the hangar unit alone.
#[test]
fn a_command_naming_a_member_reaches_only_the_others() -> TestResult {
    let mut bay = bay()?;
    let (host, _) = bay.engaged()?;
    bay.steps(30);
    let member = *bay.members(host).first().ok_or("a member")?;
    let away = at(40, 50);
    bay.step_with(vec![command(
        ME,
        vec![host, member],
        UnitOrder::Move { target: away },
    )]);
    assert!(matches!(
        bay.world.entities[&host].order_queue.front(),
        Some(UnitOrder::Move { .. })
    ));
    assert!(!matches!(
        bay.world.entities[&member].order_queue.front(),
        Some(UnitOrder::Move { .. })
    ));
    Ok(())
}

/// H-11: two runs of the same constructed fight are byte-identical, tick by tick.
#[test]
fn a_hangar_fight_is_deterministic() -> TestResult {
    let run = || -> Result<Vec<u64>, Box<dyn Error>> {
        let mut bay = bay()?;
        bay.minerals(ME, 500)?;
        bay.engaged()?;
        bay.world.spawn_unit(bay.killer, FOE, at(32, 30))?;
        let mut sums = Vec::new();
        for _ in 0..400 {
            bay.step();
            sums.push(bay.world.last_checksum());
        }
        Ok(sums)
    };
    assert_eq!(run()?, run()?);
    Ok(())
}

/// The shipped hangar unit and its member (found by property: the archetype with a hangar).
fn shipped_hangar(world: &SimWorld) -> Result<(UnitTypeId, HangarSpec), Box<dyn Error>> {
    world
        .catalog
        .archetypes()
        .find_map(|arch| {
            arch.capabilities
                .hangar
                .map(|spec| (arch.unit_type_id, spec))
        })
        .ok_or_else(|| "the shipped catalog has a hangar unit".into())
}

/// A shipped world with a passive air target: the first plain armed air unit, by id, that is
/// neither the hangar unit nor its member, disarmed and given life enough to outlast the test.
fn shipped_fight(
    catalogs: sim_core::catalog::RuntimeCatalogs,
) -> Result<(SimWorld, EntityId, EntityId), Box<dyn Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::with_catalogs(map, 0x0B_A7, catalogs.units, catalogs.upgrades)?;
    let (host, spec) = shipped_hangar(&world)?;
    let mut air: Vec<&UnitArchetype> = world
        .catalog
        .archetypes()
        .filter(|a| is_plain_unit(a) && a.mover_type == MoverType::Air)
        // An armed one, so no permanently cloaked detector-only unit is picked.
        .filter(|a| a.weapon.is_some() || a.air_weapon.is_some())
        .filter(|a| a.unit_type_id != host && a.unit_type_id != spec.member)
        .collect();
    air.sort_by_key(|a| a.unit_type_id);
    let target_type = air.first().ok_or("a plain air unit")?.unit_type_id;
    let target_arch = world.catalog.get_mut(target_type)?;
    target_arch.weapon = None;
    target_arch.ground_weapon = None;
    target_arch.air_weapon = None;
    target_arch.max_hp = SimScalar::from_i32(5000);
    let slot = ME.slot().ok_or("slot")?;
    world.player_resources[slot].minerals = 60;
    let carrier = world.spawn_unit(host, ME, at(10, 32))?;
    // In the Carrier's sight (12) and out of its launch range (8, edge to edge).
    let target = world.spawn_unit(target_type, FOE, at(21, 32))?;
    world
        .entities
        .get_mut(&carrier)
        .ok_or("carrier")?
        .issue_order(UnitOrder::attack(target), false)?;
    Ok((world, carrier, target))
}

/// Launch and build ticks of a shipped fight over `ticks` ticks: the tick of every new member
/// and of every member docked by a completed build.
fn launches_and_builds(
    world: &mut SimWorld,
    carrier: EntityId,
    target: EntityId,
    ticks: u32,
) -> (Vec<(u32, SimScalar)>, Vec<u32>) {
    let mut launches = Vec::new();
    let mut builds = Vec::new();
    for tick in 1..=ticks {
        let before: Vec<EntityId> = linked(world, carrier);
        let (built_before, queued_before) = world.entities[&carrier].hangar().map_or((0, 0), |s| {
            (s.docked.len() + usize::from(s.launched), s.queued)
        });
        let mut out = RenderSnapshot::default();
        world.step_into(
            &TickBundle {
                tick: Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            },
            &mut out,
        );
        let home = &world.entities[&carrier];
        let edge = home.position.distance(world.entities[&target].position);
        for _ in linked(world, carrier)
            .iter()
            .filter(|id| !before.contains(id))
        {
            launches.push((tick, edge));
        }
        if let Some(state) = home.hangar() {
            if state.queued < queued_before
                && state.docked.len() + usize::from(state.launched) > built_before
            {
                builds.push(tick);
            }
        }
    }
    (launches, builds)
}

fn linked(world: &SimWorld, carrier: EntityId) -> Vec<EntityId> {
    world
        .entities
        .values()
        .filter(|e| e.hangar_link().is_some_and(|l| l.hangar == carrier))
        .map(|e| e.id)
        .collect()
}

/// H-14: the shipped Carrier closes on an air target to its launch range 8, launches its four
/// start members 7 ticks apart, and builds the other four, 214 ticks and 15 minerals each.
#[test]
fn the_shipped_carrier_launches_four_and_builds_four() -> TestResult {
    let (mut world, carrier, target) = shipped_fight(sim_fixtures::shipped_catalogs())?;
    let (launches, builds) = launches_and_builds(&mut world, carrier, target, 1000);
    let radii = {
        let arch = |id: EntityId| world.catalog.get(world.entities[&id].unit_type_id);
        arch(carrier)?.collision_radius + arch(target)?.collision_radius
    };
    let (first, centre) = *launches.first().ok_or("a launch")?;
    assert!(
        centre - radii <= SimScalar::from_num(8.5),
        "launched within range 8 (plus the release slop): edge {:?}",
        centre - radii
    );
    let ticks: Vec<u32> = launches.iter().take(4).map(|(t, _)| *t).collect();
    assert_eq!(ticks, vec![first, first + 7, first + 14, first + 21]);
    assert_eq!(builds.len(), 4, "the other four are built: {builds:?}");
    assert!(builds.windows(2).all(|w| w[1] - w[0] == 214), "{builds:?}");
    let slot = ME.slot().ok_or("slot")?;
    assert_eq!(
        world.player_resources[slot].minerals, 0,
        "4 x 15 minerals spent"
    );
    Ok(())
}

/// H-15: changing the shipped hangar in a catalog directory copy changes the fight in the same
/// binary: `max_count: 4` builds nothing; `leash: 6.0` recalls members earlier; a Carrier
/// without its hangar is refused, since its launch weapon has nothing to launch.
#[test]
fn the_hangar_is_data_changed_without_recompiling() -> TestResult {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
    let copy = std::env::temp_dir().join(format!("space-scuffle-hangar-{}", std::process::id()));
    std::fs::create_dir_all(&copy)?;
    for entry in std::fs::read_dir(&source)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "ron") {
            std::fs::copy(&path, copy.join(path.file_name().ok_or("name")?))?;
        }
    }
    let units = std::fs::read_to_string(source.join("units.ron"))?;
    let load = |text: String| -> Result<sim_core::catalog::RuntimeCatalogs, Box<dyn Error>> {
        std::fs::write(copy.join("units.ron"), text)?;
        Ok(sim_core::catalog::RuntimeCatalogs::load_from_directory(
            &copy,
        )?)
    };
    let four = load(units.replacen("max_count: 8,", "max_count: 4,", 1))?;
    let (mut world, carrier, target) = shipped_fight(four)?;
    let (launches, builds) = launches_and_builds(&mut world, carrier, target, 600);
    assert_eq!((launches.len(), builds.len()), (4, 0));

    let recall = |catalogs| -> Result<u32, Box<dyn Error>> {
        let (mut world, carrier, target) = shipped_fight(catalogs)?;
        launches_and_builds(&mut world, carrier, target, 60);
        let mut out = RenderSnapshot::default();
        let away = SimVec2::from_i32(2, 60);
        for tick in 61..600 {
            let commands = if tick == 61 {
                vec![command(ME, vec![carrier], UnitOrder::Move { target: away })]
            } else {
                Vec::new()
            };
            world.step_into(
                &TickBundle {
                    tick: Tick(tick),
                    commands,
                    state_checksum: None,
                },
                &mut out,
            );
            let returning = linked(&world, carrier).iter().any(|id| {
                world.entities[id]
                    .hangar_link()
                    .is_some_and(|link| link.returning)
            });
            if returning {
                return Ok(tick);
            }
        }
        Err("no member returned".into())
    };
    let twelve = recall(load(units.clone())?)?;
    let six = recall(load(units.replacen("leash: 12.0,", "leash: 6.0,", 1))?)?;
    assert!(
        six < twelve,
        "leash 6 recalls at {six}, leash 12 at {twelve}"
    );

    let start = units.find("hangar: Some((").ok_or("the shipped hangar")?;
    let end = start
        + units[start..]
            .find("on_death: KillMembers)),")
            .ok_or("its end")?
        + "on_death: KillMembers)),".len();
    let mut without = units.clone();
    without.replace_range(start..end, "");
    assert!(
        load(without).is_err(),
        "a launch weapon without a hangar is refused"
    );
    std::fs::remove_dir_all(&copy)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Plan `2026-12-01-death-spawns-and-hangar-commands` §6.4, HC-1 to HC-10: the hangar's player
// commands on the shipped Carrier analogue, found by property (an archetype with a hangar and a
// launch weapon), through `SimWorld::step_into` and `PlayerCommand`.
// ---------------------------------------------------------------------------------------------

/// A shipped world for the hangar commands: the hangar unit's type, its hangar and the
/// member's cost; every player starts with nothing.
struct Deck {
    world: SimWorld,
    host: UnitTypeId,
    spec: HangarSpec,
    cost: u32,
    tick: u32,
}

/// The shipped hangar unit with a launch weapon, found by property.
fn shipped_launcher(world: &SimWorld) -> Result<(UnitTypeId, HangarSpec), Box<dyn Error>> {
    world
        .catalog
        .archetypes()
        .filter(|arch| arch.launch_weapon().is_some())
        .find_map(|arch| {
            arch.capabilities
                .hangar
                .map(|spec| (arch.unit_type_id, spec))
        })
        .ok_or_else(|| "the shipped catalog has a hangar unit with a launch weapon".into())
}

fn deck_from(world: SimWorld) -> Result<Deck, Box<dyn Error>> {
    let mut world = world;
    let (host, spec) = shipped_launcher(&world)?;
    let cost = world.catalog.get(spec.member)?.mineral_cost;
    for slot in PlayerSlot::ALL {
        world.player_resources[slot].minerals = 0;
        world.player_resources[slot].gas = 0;
    }
    Ok(Deck {
        world,
        host,
        spec,
        cost,
        tick: 0,
    })
}

fn new_deck() -> Result<Deck, Box<dyn Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    deck_from(sim_fixtures::world(map, 0x0B_A8))
}

impl Deck {
    fn minerals(&self, player: PlayerId) -> Result<u32, Box<dyn Error>> {
        Ok(self.world.player_resources[player.slot().ok_or("slot")?].minerals)
    }

    fn set_minerals(&mut self, player: PlayerId, minerals: u32) -> TestResult {
        self.world.player_resources[player.slot().ok_or("slot")?].minerals = minerals;
        Ok(())
    }

    fn carrier(&mut self, owner: PlayerId, x: i32) -> Result<EntityId, Box<dyn Error>> {
        Ok(self.world.spawn_unit(self.host, owner, at(x, 32))?)
    }

    fn state(&self, id: EntityId) -> Result<sim_core::hangar::HangarState, Box<dyn Error>> {
        Ok(self.world.entities[&id].hangar().ok_or("a hangar")?.clone())
    }

    /// Steps one tick with `commands` and returns their game-command outcomes.
    fn step(
        &mut self,
        commands: Vec<PlayerCommand>,
    ) -> Vec<Result<sim_core::economy::GameCommandReceipt, GameCommandError>> {
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
        self.world
            .game_command_outcomes()
            .iter()
            .map(|outcome| outcome.result.clone())
            .collect()
    }

    fn order(
        &mut self,
        entities: Vec<EntityId>,
        order: UnitOrder,
    ) -> Result<sim_core::economy::GameCommandReceipt, GameCommandError> {
        let mut outcomes = self.step(vec![command(ME, entities, order)]);
        assert_eq!(outcomes.len(), 1, "one outcome per command");
        outcomes.remove(0)
    }

    fn auto_off(&mut self, entities: Vec<EntityId>) -> TestResult {
        self.order(entities, UnitOrder::SetHangarAutoBuild { enabled: false })?;
        Ok(())
    }
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "compared with a command's outcome, which is a `Result`"
)]
fn accepted(recipients: usize) -> Result<sim_core::economy::GameCommandReceipt, GameCommandError> {
    Ok(sim_core::economy::GameCommandReceipt::Accepted { recipients })
}

fn refused(
    reason: HangarRejection,
) -> Result<sim_core::economy::GameCommandReceipt, GameCommandError> {
    Err(GameCommandError::Hangar(reason))
}

/// HC-1: a build with 4 docked queues one and charges the member's cost; the member docks
/// `build_ticks` hangar phases later, counting the tick it was queued in; with auto-build off,
/// repeated builds queue to `min(queue_size, max_count - total)`, then `Full`, and, with no
/// start members (a scratch value), to `queue_size`, then `QueueFull`.
#[test]
fn hc1_a_build_queues_one_paid_member_until_the_hangar_or_queue_is_full() -> TestResult {
    let mut deck = new_deck()?;
    deck.set_minerals(ME, 1000)?;
    let carrier = deck.carrier(ME, 20)?;
    deck.auto_off(vec![carrier])?;
    assert_eq!(deck.state(carrier)?.docked.len(), 4);
    assert_eq!(
        deck.order(vec![carrier], UnitOrder::HangarBuild),
        accepted(1)
    );
    let queued_on = deck.tick;
    assert_eq!(deck.state(carrier)?.queued, 1);
    assert_eq!(deck.minerals(ME)?, 1000 - deck.cost);
    while deck.state(carrier)?.docked.len() == 4 {
        assert!(deck.tick < queued_on + 400, "the build completes");
        deck.step(Vec::new());
    }
    assert_eq!(
        deck.tick,
        queued_on + u32::from(deck.spec.build_ticks) - 1,
        "built after build_ticks hangar phases, the queueing tick's included"
    );
    assert_eq!(deck.state(carrier)?.queued, 0);

    let room = usize::from(deck.spec.max_count) - deck.state(carrier)?.total();
    let expected = room.min(usize::from(deck.spec.queue_size));
    for _ in 0..expected {
        assert_eq!(
            deck.order(vec![carrier], UnitOrder::HangarBuild),
            accepted(1)
        );
    }
    assert_eq!(usize::from(deck.state(carrier)?.queued), expected);
    assert_eq!(
        deck.order(vec![carrier], UnitOrder::HangarBuild),
        refused(HangarRejection::Full)
    );

    let mut bare = new_deck()?;
    bare.world
        .catalog
        .get_mut(bare.host)?
        .capabilities
        .hangar
        .as_mut()
        .ok_or("a hangar")?
        .start_count = 0;
    bare.set_minerals(ME, 1000)?;
    let empty = bare.carrier(ME, 20)?;
    bare.auto_off(vec![empty])?;
    for _ in 0..bare.spec.queue_size {
        assert_eq!(bare.order(vec![empty], UnitOrder::HangarBuild), accepted(1));
    }
    assert_eq!(
        bare.order(vec![empty], UnitOrder::HangarBuild),
        refused(HangarRejection::QueueFull)
    );
    assert_eq!(
        bare.minerals(ME)?,
        1000 - u32::from(bare.spec.queue_size) * bare.cost
    );
    Ok(())
}

/// HC-2: a build goes to every selected hangar unit: with one full, the other queues and the
/// receipt counts 1; with both full it is refused `Full`; with money for one, the lower id
/// queues and the receipt counts 1.
#[test]
fn hc2_a_build_goes_to_every_selected_hangar_unit() -> TestResult {
    let mut deck = new_deck()?;
    deck.set_minerals(ME, 1000)?;
    let a = deck.carrier(ME, 16)?;
    let b = deck.carrier(ME, 28)?;
    deck.auto_off(vec![a, b])?;
    while deck.state(a)?.total() < usize::from(deck.spec.max_count) {
        assert_eq!(deck.order(vec![a], UnitOrder::HangarBuild), accepted(1));
    }
    let before = deck.state(b)?.queued;
    assert_eq!(deck.order(vec![b, a], UnitOrder::HangarBuild), accepted(1));
    assert_eq!(deck.state(b)?.queued, before + 1);
    while deck.state(b)?.total() < usize::from(deck.spec.max_count) {
        assert_eq!(deck.order(vec![b], UnitOrder::HangarBuild), accepted(1));
    }
    assert_eq!(
        deck.order(vec![a, b], UnitOrder::HangarBuild),
        refused(HangarRejection::Full)
    );

    let mut poor = new_deck()?;
    let a = poor.carrier(ME, 16)?;
    let b = poor.carrier(ME, 28)?;
    poor.auto_off(vec![a, b])?;
    poor.set_minerals(ME, poor.cost)?;
    assert_eq!(poor.order(vec![b, a], UnitOrder::HangarBuild), accepted(1));
    assert_eq!((poor.state(a)?.queued, poor.state(b)?.queued), (1, 0));
    assert_eq!(poor.minerals(ME)?, 0);
    assert_eq!(
        poor.order(vec![a, b], UnitOrder::HangarBuild),
        refused(HangarRejection::Minerals),
        "the first hangar in id order names the reason"
    );
    Ok(())
}

/// HC-3: a cancel removes the newest build and refunds its cost in full; the build in progress
/// keeps its progress while builds remain, and loses it with the last; with nothing queued it
/// is refused `NothingQueued`.
#[test]
fn hc3_a_cancel_refunds_the_newest_build() -> TestResult {
    let mut deck = new_deck()?;
    deck.set_minerals(ME, 1000)?;
    let carrier = deck.carrier(ME, 20)?;
    deck.auto_off(vec![carrier])?;
    for _ in 0..3 {
        deck.order(vec![carrier], UnitOrder::HangarBuild)?;
    }
    for _ in 0..20 {
        deck.step(Vec::new());
    }
    let progress = deck.state(carrier)?.build_progress;
    assert!(progress > 0, "premise: the first build runs");
    let minerals = deck.minerals(ME)?;
    assert_eq!(
        deck.order(vec![carrier], UnitOrder::HangarCancelLast),
        accepted(1)
    );
    let state = deck.state(carrier)?;
    assert_eq!(state.queued, 2);
    assert_eq!(
        state.build_progress,
        progress + 1,
        "the build in progress kept its progress and ran this tick"
    );
    assert_eq!(deck.minerals(ME)?, minerals + deck.cost);
    deck.order(vec![carrier], UnitOrder::HangarCancelLast)?;
    let minerals = deck.minerals(ME)?;
    assert_eq!(
        deck.order(vec![carrier], UnitOrder::HangarCancelLast),
        accepted(1)
    );
    let state = deck.state(carrier)?;
    assert_eq!((state.queued, state.build_progress), (0, 0));
    assert_eq!(deck.minerals(ME)?, minerals + deck.cost);
    assert_eq!(
        deck.order(vec![carrier], UnitOrder::HangarCancelLast),
        refused(HangarRejection::NothingQueued)
    );
    assert_eq!(deck.minerals(ME)?, minerals + deck.cost);
    Ok(())
}

/// HC-4: with auto-build off nothing is queued by itself, whatever the owner holds; switched
/// on again it queues on that tick's hangar phase. A hangar that may not be switched (a scratch
/// `auto_build_toggle: false`) refuses `NoAutoBuildSwitch` and keeps building by itself.
#[test]
fn hc4_the_switch_stops_and_restarts_automatic_builds() -> TestResult {
    let mut deck = new_deck()?;
    deck.set_minerals(ME, 1000)?;
    let carrier = deck.carrier(ME, 20)?;
    assert_eq!(
        deck.order(
            vec![carrier],
            UnitOrder::SetHangarAutoBuild { enabled: false }
        ),
        accepted(1)
    );
    for _ in 0..300 {
        deck.step(Vec::new());
    }
    let state = deck.state(carrier)?;
    assert!(!state.auto_build);
    assert_eq!((state.queued, state.docked.len()), (0, 4));
    assert_eq!(deck.minerals(ME)?, 1000);
    assert_eq!(
        deck.order(
            vec![carrier],
            UnitOrder::SetHangarAutoBuild { enabled: true }
        ),
        accepted(1)
    );
    let state = deck.state(carrier)?;
    assert!(state.auto_build);
    assert_eq!(
        state.queued, 4,
        "queued on the switching tick's hangar phase"
    );
    assert_eq!(deck.minerals(ME)?, 1000 - 4 * deck.cost);

    let mut fixed = new_deck()?;
    fixed
        .world
        .catalog
        .get_mut(fixed.host)?
        .capabilities
        .hangar
        .as_mut()
        .ok_or("a hangar")?
        .auto_build_toggle = false;
    fixed.set_minerals(ME, 1000)?;
    let carrier = fixed.carrier(ME, 20)?;
    assert_eq!(
        fixed.order(
            vec![carrier],
            UnitOrder::SetHangarAutoBuild { enabled: false }
        ),
        refused(HangarRejection::NoAutoBuildSwitch)
    );
    assert!(fixed.state(carrier)?.auto_build);
    assert_eq!(fixed.state(carrier)?.queued, 4);
    Ok(())
}

/// HC-5: a hangar unit's switch starts at its spec's `auto_build` (`true` shipped), and at
/// `false` for a scratch spec authoring it off.
#[test]
fn hc5_a_new_hangar_units_switch_is_its_specs() -> TestResult {
    let mut deck = new_deck()?;
    assert!(deck.spec.auto_build, "the shipped Carrier builds by itself");
    assert!(deck.spec.auto_build_toggle, "and may be switched");
    let carrier = deck.carrier(ME, 20)?;
    assert!(deck.state(carrier)?.auto_build);
    deck.world
        .catalog
        .get_mut(deck.host)?
        .capabilities
        .hangar
        .as_mut()
        .ok_or("a hangar")?
        .auto_build = false;
    let off = deck.carrier(ME, 30)?;
    assert!(!deck.state(off)?.auto_build);
    Ok(())
}

/// HC-6 (Q0138's default (a)): with auto-build on, a cancelled build is refunded and queued
/// again, and charged, on the same tick's hangar phase, which runs after the commands; the new
/// build starts from nothing.
#[test]
fn hc6_a_cancel_under_auto_build_is_queued_again() -> TestResult {
    let mut deck = new_deck()?;
    let carrier = deck.carrier(ME, 20)?;
    deck.set_minerals(ME, deck.cost)?;
    deck.step(Vec::new());
    assert_eq!(
        deck.state(carrier)?.queued,
        1,
        "premise: auto-build queued one"
    );
    assert_eq!(deck.minerals(ME)?, 0);
    deck.step(Vec::new());
    assert_eq!(
        deck.order(vec![carrier], UnitOrder::HangarCancelLast),
        accepted(1)
    );
    let state = deck.state(carrier)?;
    assert_eq!(state.queued, 1, "auto-build queued it again");
    assert_eq!(
        state.build_progress, 0,
        "a new build, queued after this tick's progress step: the cancelled one's was lost"
    );
    assert_eq!(deck.minerals(ME)?, 0, "refunded, then charged again");
    Ok(())
}

/// HC-7: the usual admission: a command naming only units without a hangar is refused
/// `NoHangar`; a foreign or dead recipient and one naming only a member (unselectable) get the
/// existing errors; Shift changes nothing (no order is queued); a defeated player's and a closed
/// match's commands are refused as every command is.
#[test]
fn hc7_hangar_commands_are_admitted_like_every_game_command() -> TestResult {
    let mut deck = new_deck()?;
    deck.set_minerals(ME, 1000)?;
    let carrier = deck.carrier(ME, 20)?;
    deck.auto_off(vec![carrier])?;
    let [_, _, plain, _] = pick(&deck.world)?;
    let soldier = deck.world.spawn_unit(plain, ME, at(10, 10))?;
    assert_eq!(
        deck.order(vec![soldier], UnitOrder::HangarBuild),
        refused(HangarRejection::NoHangar)
    );
    let theirs = deck.carrier(FOE, 40)?;
    assert_eq!(
        deck.order(vec![carrier, theirs], UnitOrder::HangarBuild),
        Err(GameCommandError::ForeignRecipient(theirs))
    );
    let dead = deck.carrier(ME, 50)?;
    deck.world.entities.get_mut(&dead).ok_or("dead")?.hp = SimScalar::ZERO;
    assert_eq!(
        deck.order(vec![dead], UnitOrder::HangarCancelLast),
        Err(GameCommandError::DeadRecipient(dead))
    );
    assert_eq!(
        deck.order(vec![carrier, soldier], UnitOrder::HangarBuild),
        accepted(1),
        "a mixed selection reaches its hangar unit"
    );
    let queued = deck.state(carrier)?.queued;
    let outcomes = deck.step(vec![PlayerCommand {
        player_id: ME,
        entities: vec![carrier],
        order: UnitOrder::HangarCancelLast,
        queue: true,
    }]);
    assert_eq!(outcomes, vec![accepted(1)]);
    assert_eq!(
        deck.state(carrier)?.queued + 1,
        queued,
        "Shift acts at once"
    );
    assert!(deck.world.entities[&carrier].order_queue.is_empty());

    // A member, launched at an enemy in range, is unselectable.
    let target = deck.world.spawn_unit(plain, FOE, at(26, 32))?;
    deck.world
        .entities
        .get_mut(&carrier)
        .ok_or("carrier")?
        .issue_order(UnitOrder::attack(target), false)?;
    while linked(&deck.world, carrier).is_empty() {
        assert!(deck.tick < 200, "a member is launched");
        deck.step(Vec::new());
    }
    let member = linked(&deck.world, carrier)[0];
    assert_eq!(
        deck.order(vec![member], UnitOrder::HangarBuild),
        Err(GameCommandError::UnselectableRecipient(member))
    );
    Ok(())
}

/// HC-7 (closed commands): a defeated player's hangar command is refused `PlayerDefeated`, and
/// every hangar command after the match's end `MatchOver`.
#[test]
fn hc7_a_defeated_player_or_ended_match_takes_no_hangar_command() -> TestResult {
    let mut deck = new_deck()?;
    let mut domes: Vec<&UnitArchetype> = deck
        .world
        .catalog
        .archetypes()
        .filter(|a| {
            a.attributes.contains(AttributeFlags::STRUCTURE)
                && a.capabilities.is_resource_dropoff
                && a.capabilities.death_spawn.is_none()
        })
        .collect();
    domes.sort_by_key(|a| a.unit_type_id);
    let dome = domes.first().ok_or("a shipped town hall")?.unit_type_id;
    let third = PlayerId(2);
    let mine = deck.world.spawn_structure(dome, ME, 2, 2)?;
    deck.world.spawn_structure(dome, FOE, 50, 2)?;
    let last = deck.world.spawn_structure(dome, third, 50, 50)?;
    let carrier = deck.carrier(ME, 20)?;
    deck.step(Vec::new());
    deck.world.entities.get_mut(&mine).ok_or("mine")?.hp = SimScalar::ZERO;
    deck.step(Vec::new());
    assert!(deck.world.match_state.is_defeated(ME));
    assert!(!deck.world.match_state.result.is_over());
    assert_eq!(
        deck.order(vec![carrier], UnitOrder::HangarBuild),
        Err(GameCommandError::PlayerDefeated(ME))
    );
    deck.world.entities.get_mut(&last).ok_or("last")?.hp = SimScalar::ZERO;
    deck.step(Vec::new());
    assert!(deck.world.match_state.result.is_over());
    let theirs = deck.carrier(FOE, 40)?;
    let outcomes = deck.step(vec![command(
        FOE,
        vec![theirs],
        UnitOrder::SetHangarAutoBuild { enabled: false },
    )]);
    assert_eq!(outcomes, vec![Err(GameCommandError::MatchOver)]);
    Ok(())
}

/// HC-8: the player view lists each of the viewer's hangar units, sorted, with its numbers,
/// and none of another player's.
#[test]
fn hc8_the_view_lists_the_viewers_hangar_units() -> TestResult {
    let mut deck = new_deck()?;
    deck.set_minerals(ME, 1000)?;
    let b = deck.carrier(ME, 30)?;
    let a = deck.carrier(ME, 20)?;
    let theirs = deck.carrier(FOE, 40)?;
    deck.order(vec![b], UnitOrder::SetHangarAutoBuild { enabled: false })?;
    deck.order(vec![b], UnitOrder::HangarBuild)?;
    let mut view = sim_types::PlayerView::default();
    deck.world.write_player_view(ME, &mut view);
    let listed: Vec<EntityId> = view.hangars.iter().map(|h| h.entity).collect();
    let mut mine = vec![a, b];
    mine.sort();
    assert_eq!(listed, mine);
    let of_b = view
        .hangars
        .iter()
        .find(|h| h.entity == b)
        .ok_or("b listed")?;
    let state = deck.state(b)?;
    assert_eq!(
        (
            usize::from(of_b.docked),
            of_b.launched,
            of_b.queued,
            of_b.build_progress
        ),
        (
            state.docked.len(),
            state.launched,
            state.queued,
            state.build_progress
        )
    );
    assert_eq!(
        (of_b.max_count, of_b.queue_size, of_b.build_ticks),
        (
            deck.spec.max_count,
            deck.spec.queue_size,
            deck.spec.build_ticks
        )
    );
    assert!(!of_b.auto_build && of_b.auto_build_toggle);
    let of_a = view
        .hangars
        .iter()
        .find(|h| h.entity == a)
        .ok_or("a listed")?;
    assert!(of_a.auto_build);
    deck.world.write_player_view(FOE, &mut view);
    assert_eq!(
        view.hangars.iter().map(|h| h.entity).collect::<Vec<_>>(),
        vec![theirs]
    );
    Ok(())
}

/// HC-9 (the catalog half; the hash half is `hangar::tests::every_hangar_field_is_hashed`): the
/// shipped catalog files validate with the switch authored, and the shipped hangar unit may be
/// switched and starts on.
#[test]
fn hc9_the_shipped_catalog_authors_the_switch() -> TestResult {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
    let catalogs = sim_core::catalog::RuntimeCatalogs::load_from_directory(&source)?;
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let world = SimWorld::with_catalogs(map, 9, catalogs.units, catalogs.upgrades)?;
    let (_, spec) = shipped_launcher(&world)?;
    assert!(spec.auto_build && spec.auto_build_toggle);
    Ok(())
}

/// The per-tick checksums of a constructed hangar fight driven by `bundles`.
fn hangar_fight_checksums(bundles: &[TickBundle]) -> Result<Vec<u64>, Box<dyn Error>> {
    let mut deck = new_deck()?;
    deck.set_minerals(ME, 400)?;
    let a = deck.carrier(ME, 16)?;
    let b = deck.carrier(ME, 24)?;
    let [_, _, plain, _] = pick(&deck.world)?;
    let target = deck.world.spawn_unit(plain, FOE, at(30, 32))?;
    assert_eq!((a, b, target), fight_ids(&deck.world)?, "premise: ids");
    let mut sums = Vec::with_capacity(bundles.len());
    let mut out = RenderSnapshot::default();
    for bundle in bundles {
        deck.world.step_into(bundle, &mut out);
        sums.push(deck.world.last_checksum());
    }
    Ok(sums)
}

/// The ids `hangar_fight_checksums` creates: two hangar units, then the target.
fn fight_ids(world: &SimWorld) -> Result<(EntityId, EntityId, EntityId), Box<dyn Error>> {
    let first = world
        .entities
        .values()
        .find(|e| e.hangar().is_some())
        .ok_or("a hangar unit")?
        .id;
    Ok((first, EntityId(first.0 + 1), EntityId(first.0 + 2)))
}

/// HC-10: a constructed fight using every hangar command, recorded, serialized to RON, read
/// back and replayed, gives the same checksum on every tick. (That a replay written before the
/// commands reads unchanged is `golden_replay_tests.rs`' `golden_replay_orders`, which loads
/// the recorded orders script.)
#[test]
fn hc10_a_recorded_hangar_fight_replays_to_the_same_checksums() -> TestResult {
    let probe = {
        let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
        sim_fixtures::world(map, 0x0B_A8).next_entity_id
    };
    let (a, b, target) = (EntityId(probe), EntityId(probe + 1), EntityId(probe + 2));
    let mut bundles = Vec::new();
    for tick in 1..=500_u32 {
        let commands = match tick {
            1 => vec![command(
                ME,
                vec![a, b],
                UnitOrder::SetHangarAutoBuild { enabled: false },
            )],
            3 => vec![command(ME, vec![b, a], UnitOrder::HangarBuild)],
            5 => vec![command(ME, vec![a], UnitOrder::HangarBuild)],
            8 => vec![command(ME, vec![a, b], UnitOrder::attack(target))],
            40 => vec![command(ME, vec![a], UnitOrder::HangarCancelLast)],
            90 => vec![command(
                ME,
                vec![b],
                UnitOrder::SetHangarAutoBuild { enabled: true },
            )],
            _ => Vec::new(),
        };
        bundles.push(TickBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        });
    }
    let recorded = hangar_fight_checksums(&bundles)?;
    let text = ron::to_string(&bundles)?;
    let read: Vec<TickBundle> = ron::from_str(&text)?;
    assert_eq!(read, bundles);
    assert_eq!(hangar_fight_checksums(&read)?, recorded);
    Ok(())
}
