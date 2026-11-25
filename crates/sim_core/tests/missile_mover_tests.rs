//! A weapon missile flies its weapon's movers: a range band chosen at the release, whose speed
//! phases it integrates exactly per tick (plan `2026-11-27-weapon-delivery` D-2).
//!
//! # Responsibility
//! Pins the mechanism of WP1 through `SimWorld`: a missile without movers flies as before, byte
//! for byte (W-1); one phase integrates constant acceleration exactly up to its cap (W-2); a
//! phase ends once the path flown reaches its end, the next one restarting at its own speed or
//! carrying the speed in (W-3); the band is the first whose bound admits the edge distance at the
//! release, and it is kept when the target closes in (W-4); a mover missile fuels out as a
//! constant one does (W-7) and, when its target is lost, flies its profile on to the last point
//! (W-8); its renderer event lasts the flight the same integration gives over the drawn path
//! (W-9).
//!
//! # How it fits
//! The weapon is a scratch copy of a shipped ground missile weapon found by property (a ground
//! weapon with a projectile speed and no splash), widened so that nobody walks, with one strike
//! per release and scratch movers. Expected ticks come from an independent oracle in `f64`
//! (every value is a dyadic fraction, so `f64` is exact) or from the closed form of constant
//! acceleration, never from the code under test.
//!
//! # Invariants
//! - No unit is named: archetypes and weapons are found by their properties (charter §3.3).
//! - Every world is flat and every flight runs along the x axis, so fixed-point lengths and
//!   directions are exact.

use sim_core::{ActiveProjectile, SimWorld, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    CombatEventVisual, EntityId, MissileMover, MissilePhase, MoverType, PlayerId, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId, TICKS_PER_SECOND,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The shooter's position, in world units.
const SHOOTER: (i32, i32) = (6, 16);

fn bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

fn scalar(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

/// The exact value of `s` (every value here is a dyadic fraction well inside `f64`'s mantissa).
fn value(s: SimScalar) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "raw values here stay below 2^53"
    )]
    let raw = s.raw() as f64;
    raw / 4_294_967_296.0
}

/// A phase with the given values, world units per tick and per tick squared.
fn phase(speed: Option<f64>, acceleration: f64, max: f64, ends: Option<f64>) -> MissilePhase {
    MissilePhase {
        speed: speed.map(scalar),
        acceleration: scalar(acceleration),
        max_speed: scalar(max),
        ends_after: ends.map(scalar),
    }
}

/// One mover admitting any distance with `phases`.
fn only(phases: Vec<MissilePhase>) -> Vec<MissileMover> {
    vec![MissileMover {
        up_to_range: None,
        phases,
    }]
}

/// A shipped mobile ground archetype whose ground weapon is a missile without an area.
fn missile_archetype(world: &SimWorld) -> Result<UnitArchetype, String> {
    world
        .catalog
        .archetypes()
        .filter(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
        .find(|arch| {
            arch.weapon_against(MoverType::Ground, None)
                .is_some_and(|w| w.projectile_speed.is_some() && w.splash.is_none())
        })
        .cloned()
        .ok_or_else(|| "the shipped catalog has a ground missile weapon".to_owned())
}

/// The next free archetype identity.
fn free_type(world: &SimWorld) -> UnitTypeId {
    UnitTypeId(
        world
            .catalog
            .archetypes()
            .map(|arch| arch.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    )
}

/// The bench: a flat world, a shooter carrying the scratch weapon and an unarmed target on the
/// shooter's x axis, and the collision radii of both.
struct Bench {
    world: SimWorld,
    shooter: EntityId,
    target: EntityId,
    shooter_radius: SimScalar,
    target_radius: SimScalar,
    /// The scratch weapon's `projectile_speed`.
    speed: SimScalar,
}

/// The shipped missile weapon reaching across the bench with one strike per release; its
/// movers are `movers`, and its speed their top speed (or the shipped constant speed).
fn scratch_weapon(base: &UnitArchetype, movers: Vec<MissileMover>) -> Result<WeaponSpec, String> {
    let mut weapon = base
        .weapon_against(MoverType::Ground, None)
        .cloned()
        .ok_or("invariant: the archetype was chosen for its ground weapon")?;
    weapon.max_range = SimScalar::from_i32(20);
    weapon.min_range = SimScalar::ZERO;
    weapon.attacks_count = 1;
    weapon.first_strike_delay_ticks = 0;
    weapon.strike_interval_ticks = 0;
    if let Some(top) = movers
        .iter()
        .flat_map(|mover| mover.phases.iter().map(|phase| phase.max_speed))
        .max()
    {
        weapon.projectile_speed = Some(top);
    }
    weapon.missile_movers = movers;
    Ok(weapon)
}

/// Builds the bench: the target stands `edge` world units from the shooter, edge to edge (the
/// centre distance less both collision radii), and the weapon is changed by `edit` last.
fn bench(
    seed: u64,
    edge: SimScalar,
    movers: Vec<MissileMover>,
    edit: impl FnOnce(&mut WeaponSpec),
) -> Result<Bench, Box<dyn std::error::Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let base = missile_archetype(&world)?;
    let mut weapon = scratch_weapon(&base, movers)?;
    edit(&mut weapon);
    let speed = weapon
        .projectile_speed
        .ok_or("the scratch weapon is a missile")?;
    let shooter_type = free_type(&world);
    let mut shooter_arch = base.clone();
    shooter_arch.unit_type_id = shooter_type;
    "scratch_mover_shooter".clone_into(&mut shooter_arch.slug);
    // It sees the whole bench, so that every target the weapon reaches is in sight.
    shooter_arch.sight_radius = SimScalar::from_i32(24);
    shooter_arch.weapon = Some(weapon.clone());
    shooter_arch.ground_weapon = Some(weapon);
    shooter_arch.air_weapon = None;
    world.catalog.register(shooter_arch);
    let dummy = UnitTypeId(shooter_type.0 + 1);
    let mut dummy_arch = base.clone();
    dummy_arch.unit_type_id = dummy;
    "scratch_mover_target".clone_into(&mut dummy_arch.slug);
    dummy_arch.weapon = None;
    dummy_arch.ground_weapon = None;
    dummy_arch.air_weapon = None;
    world.catalog.register(dummy_arch);
    let radius = base.collision_radius;
    let origin = SimVec2::from_i32(SHOOTER.0, SHOOTER.1);
    let shooter = world.spawn_unit(shooter_type, PlayerId(0), origin)?;
    let at = SimVec2::new(origin.x + radius + radius + edge, origin.y);
    let target = world.spawn_unit(dummy, PlayerId(1), at)?;
    Ok(Bench {
        world,
        shooter,
        target,
        shooter_radius: radius,
        target_radius: radius,
        speed,
    })
}

/// What one flight did, in ticks of the world.
struct Flight {
    /// The tick whose step released the missile.
    released: u32,
    /// The tick whose step landed it (its target's vitals changed), if it did.
    landed: Option<u32>,
    /// The tick whose step removed the missile.
    gone: u32,
    /// The renderer event's flight time, seconds.
    duration_secs: Option<f32>,
    /// The missile as it stood after each step of its flight, release tick first.
    states: Vec<ActiveProjectile>,
}

impl Flight {
    /// Advances of the missile up to its landing, the release tick's included: the flight in
    /// ticks (§2.3's count).
    fn ticks(&self) -> Option<u32> {
        self.landed.map(|landed| landed - self.released + 1)
    }
}

impl Bench {
    fn vitals(&self, id: EntityId) -> Option<(SimScalar, SimScalar)> {
        self.world.entities.get(&id).map(|e| (e.hp, e.shields))
    }

    /// Orders the attack and steps until the first missile is released, then until it is gone;
    /// `mid_flight` runs after the release tick's step.
    fn fly(
        &mut self,
        mid_flight: impl FnOnce(&mut Self) -> Result<(), String>,
    ) -> Result<Flight, String> {
        self.world
            .entities
            .get_mut(&self.shooter)
            .ok_or("the shooter exists")?
            .issue_order(UnitOrder::attack(self.target), false)
            .map_err(|error| format!("{error:?}"))?;
        let full = self.vitals(self.target);
        let mut released = None;
        let mut duration_secs = None;
        for tick in 1..=80 {
            let snapshot = self.world.step(&bundle(tick));
            if let Some(secs) = snapshot.combat_events.iter().find_map(|event| match event {
                CombatEventVisual::ProjectileVisual { duration_secs, .. } => Some(*duration_secs),
                _ => None,
            }) {
                released = Some(tick);
                duration_secs = secs;
                break;
            }
        }
        let released = released.ok_or("the missile was released within 80 ticks")?;
        let mut flight = Flight {
            released,
            landed: (self.vitals(self.target) != full).then_some(released),
            gone: released,
            duration_secs,
            states: self
                .world
                .projectiles
                .first()
                .cloned()
                .into_iter()
                .collect(),
        };
        if self.world.projectiles.is_empty() {
            return Ok(flight);
        }
        mid_flight(self)?;
        for tick in released + 1..released + 300 {
            self.world.step(&bundle(tick));
            if flight.landed.is_none() && self.vitals(self.target) != full {
                flight.landed = Some(tick);
            }
            match self.world.projectiles.first() {
                Some(missile) if missile.id == flight.states[0].id => {
                    flight.states.push(missile.clone());
                }
                _ => {
                    flight.gone = tick;
                    return Ok(flight);
                }
            }
        }
        Err("the missile was gone within 300 ticks".to_owned())
    }
}

/// The oracle of D-2's integration in `f64` (exact for the dyadic values used here): the tick
/// on which a missile flying `phases` covers `path`, the speed and phase rules as the plan
/// states them, and the speed it reaches on each tick.
fn oracle(phases: &[MissilePhase], path: f64) -> (u32, Vec<f64>) {
    let mut index = 0;
    let mut speed = phases[0].speed.map_or(0.0, value);
    let mut travelled = 0.0;
    let mut speeds = Vec::new();
    for tick in 1..=10_000 {
        let p = &phases[index];
        let next = (speed + value(p.acceleration)).min(value(p.max_speed));
        let step = f64::midpoint(speed, next);
        if path - travelled <= step {
            return (tick, speeds);
        }
        travelled += step;
        speed = next;
        while index + 1 < phases.len()
            && phases[index]
                .ends_after
                .is_some_and(|e| travelled >= value(e))
        {
            index += 1;
            if let Some(start) = phases[index].speed {
                speed = value(start);
            }
        }
        speeds.push(speed);
    }
    (u32::MAX, speeds)
}

/// The path a missile from the shooter's centre covers to land on a target `edge` away: the
/// centre distance less the target's radius.
fn path(bench: &Bench, edge: SimScalar) -> f64 {
    value(bench.shooter_radius + edge)
}

/// W-1: a missile without movers flies as `main`'s code flies it, its position on each tick
/// and its landing tick those of the old step (kept here), and its mover fields untouched.
#[test]
fn a_missile_without_movers_flies_as_before() -> TestResult {
    let mut bench = bench(0x3D1, scalar(5.25), Vec::new(), |_| {})?;
    let flight = bench.fly(|_| Ok(()))?;
    let first = flight.states.first().ok_or("the missile flew a tick")?;
    let speed = first.speed;
    let target = first.target_pos;
    let radius = bench.target_radius;
    let mut position = first.source_pos;
    let mut landed = None;
    for (advance, tick) in (flight.released..).enumerate() {
        let delta = target - position;
        if delta.length() <= speed + radius {
            landed = Some(tick);
            break;
        }
        position += delta.normalize_or_zero() * speed;
        let state = flight.states.get(advance).ok_or("the missile flew on")?;
        assert_eq!(state.current_pos, position, "tick {tick}");
        assert_eq!(
            (state.mover, state.phase, state.travelled),
            (None, 0, SimScalar::ZERO)
        );
        assert_eq!(state.speed, speed, "a constant missile keeps its speed");
    }
    assert_eq!(flight.landed, landed);
    Ok(())
}

/// W-2: one phase integrates constant acceleration exactly: uncapped, the missile lands on the
/// first tick `k` whose path `v₀k + ak²/2` reaches its target; capped, on the oracle's tick;
/// and its speed never exceeds the cap.
#[test]
fn one_phase_integrates_its_acceleration_exactly() -> TestResult {
    let (v0, a) = (0.25, 0.125);
    for (seed, edge) in (0x3D2_u64..).zip([3.0, 4.5, 6.75, 9.0]) {
        let phases = vec![phase(Some(v0), a, 64.0, None)];
        let mut bench = bench(seed, scalar(edge), only(phases), |_| {})?;
        let p = path(&bench, scalar(edge));
        let expected = (1_u32..=10_000)
            .find(|&k| {
                let k = f64::from(k);
                v0 * k + a * k * k / 2.0 >= p
            })
            .ok_or("a tick reaches the path")?;
        let flight = bench.fly(|_| Ok(()))?;
        assert_eq!(flight.ticks(), Some(expected), "edge {edge}");
    }
    for (seed, edge) in (0x3E0_u64..).zip([3.0, 7.5, 12.25]) {
        let phases = vec![phase(Some(0.125), 0.25, 0.75, None)];
        let mut bench = bench(seed, scalar(edge), only(phases.clone()), |_| {})?;
        let (expected, _) = oracle(&phases, path(&bench, scalar(edge)));
        let flight = bench.fly(|_| Ok(()))?;
        assert_eq!(flight.ticks(), Some(expected), "capped, edge {edge}");
        for state in &flight.states {
            assert!(
                state.speed <= scalar(0.75),
                "{:?} above the cap",
                state.speed
            );
        }
    }
    Ok(())
}

/// W-3: a two-phase mover switches on the first tick whose path flown reaches the first
/// phase's end; a second phase with a speed restarts at it, one without carries the speed in.
#[test]
fn a_phase_ends_once_its_path_is_flown() -> TestResult {
    for (seed, restart) in (0x3F0_u64..).zip([None, Some(1.0)]) {
        let phases = vec![
            phase(Some(0.25), 0.0, 0.25, Some(1.0)),
            phase(restart, 0.25, 2.0, None),
        ];
        let mut bench = bench(seed, scalar(9.0), only(phases.clone()), |_| {})?;
        let (expected, speeds) = oracle(&phases, path(&bench, scalar(9.0)));
        let flight = bench.fly(|_| Ok(()))?;
        assert_eq!(flight.ticks(), Some(expected), "restart {restart:?}");
        // Four ticks at 0.25 reach the end at 1.0: the fourth tick's step switches.
        for (index, state) in flight.states.iter().enumerate() {
            let switched = index >= 3;
            assert_eq!(
                state.phase,
                u8::from(switched),
                "tick {index} of the flight"
            );
            assert_eq!(state.mover, Some(0));
            assert_eq!(value(state.speed), speeds[index], "tick {index}");
        }
        let at_switch = flight.states.get(3).ok_or("the missile flew four ticks")?;
        assert_eq!(at_switch.speed, scalar(restart.unwrap_or(0.25)));
        assert_eq!(at_switch.travelled, SimScalar::ONE);
    }
    Ok(())
}

/// W-4: the band is the first whose bound admits the edge distance at the release (inclusive);
/// a target that walks closer in flight does not change it.
#[test]
fn the_band_is_chosen_once_at_the_release() -> TestResult {
    let bound = SimScalar::from_i32(2);
    let movers = vec![
        MissileMover {
            up_to_range: Some(bound),
            phases: vec![phase(Some(0.5), 0.0, 0.5, None)],
        },
        MissileMover {
            up_to_range: None,
            phases: vec![phase(Some(1.0), 0.0, 1.0, None)],
        },
    ];
    let epsilon = SimScalar::from_raw(1 << 16);
    for (seed, (edge, chosen)) in
        (0x400_u64..).zip([(bound - epsilon, 0), (bound, 0), (bound + epsilon, 1)])
    {
        let mut bench = bench(seed, edge, movers.clone(), |_| {})?;
        let flight = bench.fly(|_| Ok(()))?;
        let first = flight.states.first().ok_or("the missile flew a tick")?;
        assert_eq!(first.mover, Some(chosen), "edge {edge:?}");
    }
    let mut bench = bench(0x410, SimScalar::from_i32(6), movers, |_| {})?;
    let shooter_at = bench.world.entities[&bench.shooter].position;
    let flight = bench.fly(|bench| {
        bench
            .world
            .entities
            .get_mut(&bench.target)
            .ok_or("the target exists")?
            .issue_order(UnitOrder::Move { target: shooter_at }, false)
            .map_err(|error| format!("{error:?}"))?
            .then_some(())
            .ok_or_else(|| "the target took its move".to_owned())
    })?;
    assert!(flight.states.len() > 2, "the missile flew several ticks");
    assert!(
        flight.states.iter().all(|state| state.mover == Some(1)),
        "the band chosen at the release is kept"
    );
    Ok(())
}

/// W-7: a mover missile whose flight outlasts the weapon's lifetime fuels out on the tick a
/// constant one does, and strikes nothing.
#[test]
fn a_mover_missile_fuels_out_as_a_constant_one_does() -> TestResult {
    let mut gone = Vec::new();
    for (seed, movers) in
        (0x420_u64..).zip([Vec::new(), only(vec![phase(Some(0.125), 0.0, 0.125, None)])])
    {
        let mut bench = bench(seed, scalar(8.0), movers, |weapon| {
            weapon.projectile_speed = Some(scalar(0.125));
            weapon.max_projectile_lifetime_ticks = 10;
        })?;
        let flight = bench.fly(|_| Ok(()))?;
        assert_eq!(flight.landed, None, "a fueled-out missile strikes nothing");
        gone.push(flight.gone - flight.released);
    }
    assert_eq!(gone, vec![9, 9], "both fuel out on their tenth advance");
    Ok(())
}

/// W-8: the target dies during the first phase; the missile flies its profile on to the last
/// point, switching phase on its path as before, lands there and damages no unit.
#[test]
fn a_mover_missile_whose_target_dies_flies_its_profile_to_the_last_point() -> TestResult {
    let phases = vec![
        phase(Some(0.25), 0.0, 0.25, Some(1.0)),
        phase(None, 0.25, 2.0, None),
    ];
    let mut bench = bench(0x430, scalar(9.0), only(phases.clone()), |_| {})?;
    let shooter_full = bench.vitals(bench.shooter);
    let last = bench.world.entities[&bench.target].position;
    let flight = bench.fly(|bench| {
        bench
            .world
            .entities
            .get_mut(&bench.target)
            .ok_or("the target exists")?
            .hp = SimScalar::ZERO;
        Ok(())
    })?;
    // Landing on a point: no target radius, so the path is the centre distance.
    let source = flight
        .states
        .first()
        .ok_or("the missile flew a tick")?
        .source_pos;
    let centre = value((last - source).length());
    let (expected, speeds) = oracle(&phases, centre);
    assert_eq!(
        flight.gone - flight.released + 1,
        expected,
        "it lands on the last point"
    );
    for (index, state) in flight.states.iter().enumerate().skip(1) {
        assert_eq!(state.target_id, None, "tick {index}: the target is lost");
        assert_eq!(state.target_pos, last);
        assert_eq!(value(state.speed), speeds[index], "tick {index}");
    }
    assert!(
        flight.states.iter().any(|state| state.phase == 1),
        "it reached its second phase"
    );
    assert_eq!(
        bench.vitals(bench.shooter),
        shooter_full,
        "no unit takes damage"
    );
    Ok(())
}

/// W-9: the renderer event of a mover missile lasts the flight the same integration gives over
/// the drawn path (the centre distance less both radii, at least 0.05), in seconds of 25 ticks.
#[test]
fn a_mover_missiles_event_lasts_its_integrated_flight() -> TestResult {
    let phases = vec![
        phase(Some(0.25), 0.0, 0.25, Some(1.0)),
        phase(None, 0.25, 2.0, None),
    ];
    for (seed, edge) in (0x440_u64..).zip([0.5, 2.0, 5.0, 9.0]) {
        let mut bench = bench(seed, scalar(edge), only(phases.clone()), |_| {})?;
        let (ticks, _) = oracle(&phases, edge.max(0.05));
        let flight = bench.fly(|_| Ok(()))?;
        #[expect(clippy::cast_precision_loss, reason = "a small tick count")]
        let expected = ticks.max(1) as f32 / f32::from(TICKS_PER_SECOND);
        assert_eq!(flight.duration_secs, Some(expected), "edge {edge}");
    }
    Ok(())
}

/// W-9, constant half: a missile without movers keeps `main`'s event, `ceil(path / speed)`
/// ticks over the drawn path, computed here as `main` computes it.
#[test]
fn a_constant_missiles_event_lasts_as_before() -> TestResult {
    for (seed, edge) in (0x450_u64..).zip([0.0, 0.5, 2.0, 5.0, 9.0]) {
        let mut bench = bench(seed, scalar(edge), Vec::new(), |_| {})?;
        let flight = bench.fly(|_| Ok(()))?;
        let speed = bench.speed;
        let drawn = scalar(edge).max(scalar(0.05));
        let ticks = (drawn / speed).saturating_ceil().saturating_to_u32().max(1);
        #[expect(clippy::cast_precision_loss, reason = "a small tick count")]
        let expected = ticks as f32 / f32::from(TICKS_PER_SECOND);
        assert_eq!(flight.duration_secs, Some(expected), "edge {edge}");
    }
    Ok(())
}

