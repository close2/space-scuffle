//! # Integration Test: the attack dash (B-43)
//!
//! `StarCraft` II's Charge as a capability any archetype may carry
//! (`ArchetypeCapabilities::attack_dash`): while a unit attacks a visible enemy and does not hold
//! position, the dash starts once the target's gap exceeds `min_gap` and its centre is closer
//! than `max_distance`; it multiplies speed for `duration_ticks` or until the weapon starts a
//! swing, and `cooldown_ticks` must pass before the next (`liberty` AbilData:3825-3834,
//! EffectData:2062-2068, BehaviorData:967-975, ValidatorData:183-190, :1697-1704,
//! :3007-3011). These tests drive real orders through `SimWorld::step` on scratch archetypes
//! cloned from a shipped unit.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]

use sim_core::{SimWorld, UnitArchetype, UpgradeCatalog, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackDashSpec, AttributeBonuses, AttributeFlags, EntityId, PlayerId, SimAngle, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId, WeaponVisualKind,
};

const DASHER: &str = "dash_test_runner";

/// A 64 × 64 flat world on the shipped catalog.
fn flat_world() -> SimWorld {
    let cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, cell).expect("invariant: valid map");
    sim_fixtures::world(map, 0x0b43)
}

/// A melee weapon (range 0.1) dealing 5 every 20 ticks, releasing on the tick it starts.
fn blades() -> WeaponSpec {
    WeaponSpec {
        name: "DashTestBlades".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(0.1),
        firing_arc: SimAngle::from_degrees(360),
        acquire_arc: SimAngle::from_degrees(360),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        turret_idle: sim_types::TurretIdle::Hold,
        base_damage: SimScalar::from_num(5),
        attribute_bonuses: AttributeBonuses::NONE,
        attacks_count: 1,
        strike_interval_ticks: 0,
        followup_max_distance: None,
        period_ticks: 20,
        damage_point_ticks: 0,
        backswing_ticks: 0,
        random_delay_min: SimScalar::ZERO,
        random_delay_max: SimScalar::ZERO,
        splash: None,
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
        suicide_on_attack: false,
        structure_strike: None,
    }
}

/// Registers a clone of the first shipped armed mobile unit under a free id, changed by `edit`:
/// radius 0.5, speed 0.2 per tick reached at once, no turning delay.
fn register(world: &mut SimWorld, slug: &str, edit: impl FnOnce(&mut UnitArchetype)) -> UnitTypeId {
    let base = world
        .catalog
        .archetypes()
        .find(|arch| arch.weapon.is_some() && arch.max_speed > SimScalar::ZERO)
        .expect("a shipped armed unit")
        .clone();
    let free = (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("a free unit type id");
    let mut arch = base;
    arch.unit_type_id = free;
    arch.slug = slug.to_string();
    arch.name = slug.to_string();
    arch.collision_radius = SimScalar::from_num(0.5);
    arch.max_speed = SimScalar::from_num(0.2);
    arch.acceleration = SimScalar::from_num(10);
    arch.deceleration = SimScalar::from_num(10);
    arch.turn_rate = SimAngle::from_degrees(180);
    arch.stationary_turn_rate = SimAngle::from_degrees(180);
    arch.turn_before_move = false;
    arch.capabilities.overclock = None;
    arch.capabilities.barrier = None;
    arch.capabilities.attack_dash = None;
    edit(&mut arch);
    world.catalog.register(arch);
    free
}

/// A dash doubling speed for `duration` ticks, every `cooldown` ticks, within gap 0.6 to
/// centre distance 4.
fn dash(duration: u16, cooldown: u16) -> AttackDashSpec {
    AttackDashSpec {
        speed_multiplier: SimScalar::from_num(2),
        duration_ticks: duration,
        cooldown_ticks: cooldown,
        min_gap: SimScalar::from_num(0.6),
        max_distance: SimScalar::from_num(4),
    }
}

/// The dashing attacker, armed with [`blades`].
fn register_dasher(world: &mut SimWorld, spec: Option<AttackDashSpec>) -> UnitTypeId {
    register(world, DASHER, |arch| {
        arch.weapon = Some(blades());
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.capabilities.attack_dash = spec;
    })
}

/// A passive, unarmed target with 1000 life and no regeneration.
fn register_target(world: &mut SimWorld) -> UnitTypeId {
    register(world, "dash_test_target", |arch| {
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.max_hp = SimScalar::from_num(1000);
        arch.max_shields = SimScalar::ZERO;
        arch.shield_regen_per_tick = SimScalar::ZERO;
        arch.shield_regen_delay_ticks = None;
        arch.hp_regen_per_tick = SimScalar::ZERO;
    })
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn empty(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// A dasher at (10, 10) and a holding target at (`target_x`, 10), the dasher given `order`
/// (built from the target's id).
struct Duel {
    world: SimWorld,
    dasher: EntityId,
    target: EntityId,
    tick: u32,
}

impl Duel {
    fn new(
        spec: Option<AttackDashSpec>,
        target_x: f64,
        order: impl FnOnce(EntityId) -> UnitOrder,
    ) -> Self {
        let mut world = flat_world();
        let dasher_type = register_dasher(&mut world, spec);
        let target_type = register_target(&mut world);
        let dasher = world
            .spawn_unit(dasher_type, PlayerId(0), at(10.0, 10.0))
            .expect("spawn dasher");
        let target = world
            .spawn_unit(target_type, PlayerId(1), at(target_x, 10.0))
            .expect("spawn target");
        world
            .entities
            .get_mut(&target)
            .expect("target exists")
            .issue_order(UnitOrder::HoldPosition, false)
            .expect("hold admitted");
        world
            .entities
            .get_mut(&dasher)
            .expect("dasher exists")
            .issue_order(order(target), false)
            .expect("order admitted");
        Self {
            world,
            dasher,
            target,
            tick: 0,
        }
    }

    /// Steps one tick; returns the dasher's displacement along x, whether it is dashing after
    /// the tick, and whether the target lost life on it.
    fn step(&mut self) -> (f32, bool, bool) {
        let x = self.world.entities[&self.dasher].position.x;
        let hp = self.world.entities[&self.target].hp;
        self.world.step(&empty(self.tick));
        self.tick += 1;
        let dasher = &self.world.entities[&self.dasher];
        let hit = self.world.entities[&self.target].hp < hp;
        (
            (dasher.position.x - x).to_f32(),
            dasher.is_attack_dashing(),
            hit,
        )
    }

    fn steps(&mut self, ticks: u32) -> Vec<(f32, bool, bool)> {
        (0..ticks).map(|_| self.step()).collect()
    }
}

fn close(left: f32, right: f32) -> bool {
    (left - right).abs() < 1e-4
}

#[test]
fn b43_a_dash_in_its_window_doubles_speed_until_the_weapon_swings() {
    // Centre 3.5 apart (< 4), gap 2.5 (> 0.6).
    let mut duel = Duel::new(Some(dash(40, 100)), 13.5, UnitOrder::attack);
    let trace = duel.steps(30);
    assert!(trace[0].1, "dashing from the first tick: {trace:?}");
    assert!(close(trace[0].0, 0.4), "at twice 0.2 per tick: {trace:?}");
    let first_hit = trace
        .iter()
        .position(|&(_, _, hit)| hit)
        .expect("the dasher reaches and hits its target");
    // 2.4 to cover at 0.4 a tick: in contact after 6 ticks, and the swing within a few more
    // as the mover settles, well inside the 40-tick dash.
    assert!(first_hit <= 10, "{first_hit}: {trace:?}");
    assert!(
        trace[..first_hit].iter().all(|&(_, dashing, _)| dashing),
        "{trace:?}"
    );
    assert!(
        trace[first_hit..].iter().all(|&(_, dashing, _)| !dashing),
        "the swing ends the dash and the cooldown holds the next: {trace:?}"
    );
    let cooldown = duel.world.entities[&duel.dasher].attack_dash_cooldown;
    assert_eq!(
        u32::from(cooldown),
        100 - 29,
        "the cooldown runs from the dash's start"
    );
}

#[test]
fn b43_without_the_capability_nothing_dashes() {
    let mut duel = Duel::new(None, 13.5, UnitOrder::attack);
    let trace = duel.steps(20);
    assert!(trace.iter().all(|&(_, dashing, _)| !dashing), "{trace:?}");
    assert!(close(trace[0].0, 0.2), "{trace:?}");
    assert_eq!(duel.world.entities[&duel.dasher].attack_dash_cooldown, 0);
}

#[test]
fn b43_no_dash_at_or_beyond_the_maximum_distance_until_the_target_is_nearer() {
    // Centre 4.5 apart: walk 0.2 a tick until the centre distance drops below 4.
    let mut duel = Duel::new(Some(dash(40, 100)), 14.5, UnitOrder::attack);
    let trace = duel.steps(10);
    let first_dash = trace
        .iter()
        .position(|&(_, dashing, _)| dashing)
        .expect("a dash once within 4");
    assert!(
        first_dash >= 2,
        "4.5 needs more than 0.5 of walking: {trace:?}"
    );
    assert!(
        trace[..first_dash]
            .iter()
            .all(|&(dx, dashing, _)| !dashing && close(dx, 0.2)),
        "{trace:?}"
    );
    assert!(close(trace[first_dash].0, 0.4), "{trace:?}");
}

#[test]
fn b43_no_dash_within_the_minimum_gap() {
    // Centre 1.55 apart, gap 0.55 (not above 0.6): the dasher walks in without dashing.
    let mut duel = Duel::new(Some(dash(40, 100)), 11.55, UnitOrder::attack);
    let trace = duel.steps(15);
    assert!(trace.iter().all(|&(_, dashing, _)| !dashing), "{trace:?}");
    assert!(trace.iter().any(|&(_, _, hit)| hit), "{trace:?}");
}

#[test]
fn b43_a_dash_runs_out_after_its_duration_and_waits_for_its_cooldown() {
    // Centre 3.9 apart, gap 2.9: a 3-tick dash covers 1.2, then walking resumes.
    let mut duel = Duel::new(Some(dash(3, 30)), 13.9, UnitOrder::attack);
    let trace = duel.steps(5);
    let dashing: Vec<bool> = trace.iter().map(|&(_, dashing, _)| dashing).collect();
    assert_eq!(dashing, [true, true, true, false, false], "{trace:?}");
    assert!(
        close(trace[2].0, 0.4) && close(trace[3].0, 0.2),
        "{trace:?}"
    );

    // Let the dasher fight, then put the target back at 3.5 before the cooldown is over.
    duel.steps(20);
    let cooldown = duel.world.entities[&duel.dasher].attack_dash_cooldown;
    assert_eq!(
        cooldown, 6,
        "30 ticks from the dash's start on the first tick"
    );
    let place = |duel: &mut Duel| {
        let x = duel.world.entities[&duel.dasher].position.x + SimScalar::from_num(3.5);
        let target = duel.world.entities.get_mut(&duel.target).expect("target");
        target.position = SimVec2::new(x, target.position.y);
        target.pos_at_tick_start = target.position;
    };
    place(&mut duel);
    let waiting = duel.steps(4);
    assert!(
        waiting.iter().all(|&(_, dashing, _)| !dashing),
        "{waiting:?}"
    );
    place(&mut duel);
    let ready = duel.steps(2);
    assert!(
        ready[1].1,
        "dashing again once the cooldown is over: {ready:?}"
    );
}

#[test]
fn b43_holding_position_never_dashes() {
    let mut duel = Duel::new(Some(dash(40, 100)), 13.5, |_| UnitOrder::HoldPosition);
    let trace = duel.steps(10);
    assert!(trace.iter().all(|&(_, dashing, _)| !dashing), "{trace:?}");
}

#[test]
fn b43_a_move_order_never_dashes_and_an_attack_move_does() {
    let mut walk = Duel::new(Some(dash(40, 100)), 13.5, |_| UnitOrder::Move {
        target: at(10.0, 20.0),
    });
    let trace = walk.steps(10);
    assert!(trace.iter().all(|&(_, dashing, _)| !dashing), "{trace:?}");

    let mut charge = Duel::new(Some(dash(40, 100)), 13.5, |_| UnitOrder::AttackMove {
        target: at(20.0, 10.0),
    });
    let trace = charge.steps(10);
    assert!(
        trace.iter().any(|&(_, dashing, _)| dashing),
        "an acquired target is attacked, so it dashes: {trace:?}"
    );
}

#[test]
fn b43_a_dash_an_upgrade_unlocks_waits_for_the_research() {
    let upgrades = format!(
        r#"(upgrades: [(
            upgrade_id: 1, slug: "learn_dash", name: "learn_dash", display_name: "Learn",
            faction: "automaton_directorate", level: 1, mineral_cost: 0, gas_cost: 0,
            research_ticks: 1, researched_at: "foundry_core_dome",
            effects: [UnlockAbility(target_slug: "{DASHER}", ability_slug: "attack_dash")],
        )])"#
    );
    let catalog = UpgradeCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &upgrades)
        .expect("scratch upgrade catalog");

    let mut locked = Duel::new(Some(dash(40, 100)), 13.5, UnitOrder::attack);
    locked.world.upgrade_catalog = catalog.clone();
    let trace = locked.steps(10);
    assert!(trace.iter().all(|&(_, dashing, _)| !dashing), "{trace:?}");

    let mut learned = Duel::new(Some(dash(40, 100)), 13.5, UnitOrder::attack);
    learned.world.upgrade_catalog = catalog;
    learned
        .world
        .research_upgrade(PlayerId(0), "learn_dash")
        .expect("research");
    let trace = learned.steps(2);
    assert!(trace[0].1, "{trace:?}");
}
