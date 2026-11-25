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
    for _ in 0..200 {
        bay.step();
        if bay.members(host).is_empty() {
            break;
        }
    }
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
