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
//! (W-9). Plan `2026-11-28-missile-movers-b52` D-2 adds target-relative ends: a phase ends once
//! the path left to the impact is within its end, checked after each step against the target
//! where it now stands (M-2, M-3) and once at the release (M-4), and the renderer event
//! integrates the same rule (M-7).
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
    AbilityTargetKind, AttackVisibility, CombatEventVisual, EntityId, MissileMover, MissilePhase,
    MoverType, PhaseEnd, PlayerId, PlayerView, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId, TICKS_PER_SECOND,
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

/// A phase with the given values, world units per tick and per tick squared; its end, if any,
/// a path flown.
fn phase(speed: Option<f64>, acceleration: f64, max: f64, ends: Option<f64>) -> MissilePhase {
    MissilePhase {
        speed: speed.map(scalar),
        acceleration: scalar(acceleration),
        max_speed: scalar(max),
        ends: ends.map(|after| PhaseEnd::Flown(scalar(after))),
    }
}

/// A phase ending once at most `within` world units are left to the impact.
fn phase_within(speed: Option<f64>, acceleration: f64, max: f64, within: f64) -> MissilePhase {
    MissilePhase {
        ends: Some(PhaseEnd::Remaining(scalar(within))),
        ..phase(speed, acceleration, max, None)
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

/// Whether `phase`'s end is due after `travelled` of `path`, a target standing still (plan
/// `2026-11-28-missile-movers-b52` D-2).
fn due(phase: &MissilePhase, travelled: f64, path: f64) -> bool {
    match phase.ends {
        Some(PhaseEnd::Flown(after)) => travelled >= value(after),
        Some(PhaseEnd::Remaining(within)) => path - travelled <= value(within),
        None => false,
    }
}

/// The oracle of D-2's integration in `f64` (exact for the dyadic values used here): the tick
/// on which a missile flying `phases` covers `path` towards a target standing still, the speed
/// and phase rules as the plans state them (the ends checked once at the release and after
/// each step), and the speed it reaches on each tick.
fn oracle(phases: &[MissilePhase], path: f64) -> (u32, Vec<f64>) {
    let mut index = 0;
    let mut speed = phases[0].speed.map_or(0.0, value);
    let mut travelled = 0.0;
    let mut speeds = Vec::new();
    while index + 1 < phases.len() && due(&phases[index], travelled, path) {
        index += 1;
        if let Some(start) = phases[index].speed {
            speed = value(start);
        }
    }
    for tick in 1..=10_000 {
        let p = &phases[index];
        let next = (speed + value(p.acceleration)).min(value(p.max_speed));
        let step = f64::midpoint(speed, next);
        if path - travelled <= step {
            return (tick, speeds);
        }
        travelled += step;
        speed = next;
        while index + 1 < phases.len() && due(&phases[index], travelled, path) {
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

/// The two-phase mover of M-2 to M-4 and M-7: a constant 0.25 until at most 4 world units are
/// left to the impact, then accelerating from the speed carried in.
fn kill_phase_mover() -> Vec<MissilePhase> {
    vec![
        phase_within(Some(0.25), 0.0, 0.25, 4.0),
        phase(None, 0.25, 2.0, None),
    ]
}

/// M-2 (plan `2026-11-28-missile-movers-b52` D-2): at a target standing still, the missile
/// switches on the first tick after whose step the path left is at most the end's 4 world
/// units, and lands on the oracle's tick, for a table of paths around the end.
#[test]
fn a_phase_ends_once_the_path_left_is_within_its_end() -> TestResult {
    let phases = kill_phase_mover();
    let probe = bench(0x5A0, scalar(6.0), only(phases.clone()), |_| {})?;
    let radius = value(probe.shooter_radius);
    // Paths 4.125 to 9: the switch at the release tick's step, a few ticks in, and far in.
    for (seed, path) in (0x5A1_u64..).zip([4.125, 4.25, 4.5, 4.75, 5.0, 5.5, 6.25, 9.0]) {
        let edge = path - radius;
        let mut bench = bench(seed, scalar(edge), only(phases.clone()), |_| {})?;
        let (expected, speeds) = oracle(&phases, path);
        let flight = bench.fly(|_| Ok(()))?;
        assert_eq!(flight.ticks(), Some(expected), "path {path}");
        for (index, state) in flight.states.iter().enumerate() {
            let left = path - value(state.travelled);
            assert_eq!(
                state.phase,
                u8::from(left <= 4.0),
                "path {path}, tick {index} of the flight, {left} left"
            );
            assert_eq!(
                value(state.speed),
                speeds[index],
                "path {path}, tick {index}"
            );
        }
        assert_eq!(
            flight.states.first().map(|first| first.travelled),
            Some(scalar(0.25)),
            "path {path}: released outside the end, its first step is the first phase's"
        );
    }
    Ok(())
}

/// The tick of the flight (0 the release tick's step) on which `flight`'s missile entered its
/// second phase.
fn switch_tick(flight: &Flight) -> Option<usize> {
    flight.states.iter().position(|state| state.phase == 1)
}

/// M-3: the end follows the target. Released 9 world units of path away, the same missile at a
/// target walking towards the launcher switches earlier, and at one walking away later, than
/// at a target standing still; the band chosen at the release is kept throughout (W-4's rule).
#[test]
fn a_target_relative_end_follows_a_moving_target() -> TestResult {
    let movers = vec![
        MissileMover {
            up_to_range: Some(SimScalar::ONE),
            phases: vec![phase(Some(1.0), 0.0, 1.0, None)],
        },
        MissileMover {
            up_to_range: None,
            phases: kill_phase_mover(),
        },
    ];
    let mut switches = Vec::new();
    // Towards a point in front of the launcher, or away to the bench's far side.
    for (seed, walk) in (0x5B0_u64..).zip([None, Some(SHOOTER.0 + 3), Some(30)]) {
        let mut bench = bench(seed, scalar(8.5), movers.clone(), |_| {})?;
        let flight = bench.fly(|bench| {
            let Some(x) = walk else {
                return Ok(());
            };
            let to = SimVec2::from_i32(x, SHOOTER.1);
            bench
                .world
                .entities
                .get_mut(&bench.target)
                .ok_or("the target exists")?
                .issue_order(UnitOrder::Move { target: to }, false)
                .map_err(|error| format!("{error:?}"))?
                .then_some(())
                .ok_or_else(|| "the target took its move".to_owned())
        })?;
        assert!(
            flight.states.iter().all(|state| state.mover == Some(1)),
            "walk {walk:?}: the band chosen at the release is kept"
        );
        switches.push(switch_tick(&flight).ok_or("the missile reached its kill phase")?);
    }
    let (still, towards, away) = (switches[0], switches[1], switches[2]);
    assert!(towards < still, "towards the launcher: {towards} < {still}");
    assert!(away > still, "away from the launcher: {away} > {still}");
    Ok(())
}

/// M-4: a missile released with at most the end's distance left starts in the next phase, its
/// first step that phase's integration from the speed carried in; a positive `Flown` end is
/// never entered at the release, however short.
#[test]
fn the_release_enters_a_phase_whose_end_is_already_due() -> TestResult {
    let mut inside = bench(0x5C0, scalar(1.0), only(kill_phase_mover()), |_| {})?;
    let flight = inside.fly(|_| Ok(()))?;
    let first = flight.states.first().ok_or("the missile flew a tick")?;
    // From 0.25 carried in, accelerating 0.25: the release tick's step is 0.375.
    assert_eq!(
        (first.phase, first.travelled, first.speed),
        (1, scalar(0.375), scalar(0.5)),
        "it starts in its kill phase"
    );
    let shortest = vec![
        phase(Some(0.25), 0.0, 0.25, Some(1.0 / 64.0)),
        phase(Some(1.0), 0.0, 1.0, None),
    ];
    let mut flown = bench(0x5C1, scalar(1.0), only(shortest), |_| {})?;
    let flight = flown.fly(|_| Ok(()))?;
    let first = flight.states.first().ok_or("the missile flew a tick")?;
    assert_eq!(
        first.travelled,
        scalar(0.25),
        "the first step is the first phase's"
    );
    assert_eq!(
        (first.phase, first.speed),
        (1, SimScalar::ONE),
        "then its path-flown end is due"
    );
    Ok(())
}

/// M-7: the renderer event of a missile with a target-relative end lasts the flight the same
/// integration gives over the drawn path, a launch inside the end distance included.
#[test]
fn a_target_relative_missiles_event_lasts_its_integrated_flight() -> TestResult {
    let phases = kill_phase_mover();
    for (seed, edge) in (0x5D0_u64..).zip([0.5, 2.0, 3.5, 4.0, 4.5, 5.0, 9.0]) {
        let mut bench = bench(seed, scalar(edge), only(phases.clone()), |_| {})?;
        let (ticks, _) = oracle(&phases, edge.max(0.05));
        let flight = bench.fly(|_| Ok(()))?;
        #[expect(clippy::cast_precision_loss, reason = "a small tick count")]
        let expected = ticks.max(1) as f32 / f32::from(TICKS_PER_SECOND);
        assert_eq!(flight.duration_secs, Some(expected), "edge {edge}");
    }
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

// ---------------------------------------------------------------------------------------------
// The shipped weapons (W-11 to W-16, plan `2026-11-27-weapon-delivery` WP2)
// ---------------------------------------------------------------------------------------------

/// Registers a copy of `arch` under the next free identity, changed by `edit`.
fn register(
    world: &mut SimWorld,
    arch: &UnitArchetype,
    edit: impl FnOnce(&mut UnitArchetype),
) -> UnitTypeId {
    let id = free_type(world);
    let mut copy = arch.clone();
    copy.unit_type_id = id;
    format!("scratch_mover_{}", id.0).clone_into(&mut copy.slug);
    edit(&mut copy);
    world.catalog.register(copy);
    id
}

/// Strips `arch` of its weapons and gives it `sight` and a deep pool of life, so it neither
/// fights back nor dies of the strikes a test counts.
fn unarmed(sight: SimScalar) -> impl FnOnce(&mut UnitArchetype) {
    move |copy: &mut UnitArchetype| {
        copy.weapon = None;
        copy.ground_weapon = None;
        copy.air_weapon = None;
        copy.sight_radius = sight;
        copy.detection_radius = None;
    }
}

/// The shipped mobile air unit whose air weapon flies movers: the Phoenix analogue.
fn air_mover_unit(world: &SimWorld) -> Result<UnitArchetype, String> {
    world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.mover_type == MoverType::Air
                && arch.max_speed > SimScalar::ZERO
                && arch
                    .air_weapon
                    .as_ref()
                    .is_some_and(|w| !w.missile_movers.is_empty())
        })
        .cloned()
        .ok_or_else(|| "the shipped catalog has an air weapon with movers".to_owned())
}

/// The shipped structure whose ground weapon flies movers: the Spine Crawler analogue.
fn ground_mover_structure(world: &SimWorld) -> Result<UnitArchetype, String> {
    world
        .catalog
        .archetypes()
        .find(|arch| {
            matches!(arch.shape, ShapeKind::BuildingDome { .. })
                && arch
                    .ground_weapon
                    .as_ref()
                    .is_some_and(|w| !w.missile_movers.is_empty())
        })
        .cloned()
        .ok_or_else(|| "the shipped catalog has a structure ground weapon with movers".to_owned())
}

/// A duel on open ground between the shipped `attacker` (a structure when it has a dome) of
/// player 0 and an unarmed copy of `victim` of player 1 seeing `sight`, `edge` world units
/// apart edge to edge along the x axis; `edit` changes the attacker's copy first.
fn shipped_duel(
    seed: u64,
    attacker: &UnitArchetype,
    victim: &UnitArchetype,
    sight: SimScalar,
    edge: SimScalar,
    edit: impl FnOnce(&mut UnitArchetype),
) -> Result<Bench, Box<dyn std::error::Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let shooter_type = register(&mut world, attacker, edit);
    let target_type = register(&mut world, victim, unarmed(sight));
    let shooter = if matches!(attacker.shape, ShapeKind::BuildingDome { .. }) {
        world.spawn_structure(shooter_type, PlayerId(0), 6, 15)?
    } else {
        world.spawn_unit(shooter_type, PlayerId(0), SimVec2::from_i32(7, 16))?
    };
    let origin = world.entities[&shooter].position;
    let at = SimVec2::new(
        origin.x + attacker.collision_radius + victim.collision_radius + edge,
        origin.y,
    );
    let target = world.spawn_unit(target_type, PlayerId(1), at)?;
    let entity = world.entities.get_mut(&target).ok_or("spawned")?;
    entity.max_hp = SimScalar::from_i32(100_000);
    entity.hp = entity.max_hp;
    // It holds, so it neither flees nor is pushed out of reach.
    entity
        .issue_order(UnitOrder::HoldPosition, false)
        .map_err(|error| format!("{error:?}"))?;
    let speed = attacker
        .weapon_against(victim.mover_type, None)
        .and_then(|w| w.projectile_speed)
        .ok_or("the attacker's weapon against the victim is a missile")?;
    Ok(Bench {
        world,
        shooter,
        target,
        shooter_radius: attacker.collision_radius,
        target_radius: victim.collision_radius,
        speed,
    })
}

/// The images `viewer` is listed, by attacker.
fn images(world: &SimWorld, viewer: u8) -> Vec<EntityId> {
    let mut view = PlayerView::default();
    world.write_player_view(PlayerId(viewer), &mut view);
    view.attack_snapshots
        .iter()
        .map(|entry| entry.entity)
        .collect()
}

/// W-11: the Phoenix analogue's release strikes nothing; both strikes land together on the
/// tick of §2.3's table (2 ticks at edge 0.5, 8 at 3, 10 at 5, the release tick counted), and
/// its `Visible` damage reveals it on that tick, not at the release.
#[test]
fn the_air_mover_weapon_lands_both_strikes_after_its_flight() -> TestResult {
    let world = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    let phoenix = air_mover_unit(&world)?;
    for (seed, (edge, expected)) in (0x500_u64..).zip([(0.5, 2), (3.0, 8), (5.0, 10)]) {
        let mut bench = shipped_duel(seed, &phoenix, &phoenix, scalar(10.0), scalar(edge), |_| {})?;
        let flight = bench.fly(|_| Ok(()))?;
        let first = flight.states.first().ok_or("the missile flew a tick")?;
        assert_eq!(first.strikes, 2, "one missile carries both strikes");
        assert!(first.mover.is_some(), "it flies a mover");
        assert_eq!(flight.ticks(), Some(expected), "edge {edge}");
        assert!(
            bench.world.entities[&bench.shooter].is_attack_revealed_to(PlayerId(1)),
            "edge {edge}: the impact reveals the attacker"
        );
    }
    // The reveal starts on the landing tick: stepped tick by tick, not before.
    let mut bench = shipped_duel(0x510, &phoenix, &phoenix, scalar(10.0), scalar(5.0), |_| {})?;
    let full = bench.vitals(bench.target);
    let mut released = None;
    let mut ticks = Vec::new();
    for tick in 1..=120 {
        if tick == 1 {
            bench
                .world
                .entities
                .get_mut(&bench.shooter)
                .ok_or("the shooter exists")?
                .issue_order(UnitOrder::attack(bench.target), false)
                .map_err(|error| format!("{error:?}"))?;
        }
        bench.world.step(&bundle(tick));
        if released.is_none() && !bench.world.projectiles.is_empty() {
            released = Some(tick);
        }
        let shown = bench.world.entities[&bench.shooter].is_attack_revealed_to(PlayerId(1));
        ticks.push((tick, shown, bench.vitals(bench.target) != full));
        if bench.vitals(bench.target) != full {
            break;
        }
    }
    let released = released.ok_or("released")?;
    for &(tick, shown, struck) in &ticks {
        assert_eq!(
            shown, struck,
            "tick {tick} (released {released}): revealed only once struck"
        );
    }
    assert_eq!(ticks.last().map(|t| t.0), Some(released + 9));
    Ok(())
}

/// W-12: the Spine Crawler analogue's tentacle lands on the tick of §2.3's table; the short band
/// (edge 3 or less) is the slower one.
#[test]
fn the_structure_mover_weapon_lands_after_its_band_s_flight() -> TestResult {
    let world = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    let spine = ground_mover_structure(&world)?;
    let victim = missile_archetype(&world)?;
    for (seed, (edge, expected)) in
        (0x520_u64..).zip([(0.5, 6), (2.0, 7), (3.0, 7), (3.5, 4), (5.0, 4), (7.0, 5)])
    {
        let mut bench = shipped_duel(seed, &spine, &victim, scalar(10.0), scalar(edge), |_| {})?;
        let flight = bench.fly(|_| Ok(()))?;
        let first = flight.states.first().ok_or("the missile flew a tick")?;
        assert_eq!(
            first.mover,
            Some(u8::from(edge > 3.0)),
            "edge {edge}: its band"
        );
        assert_eq!(flight.ticks(), Some(expected), "edge {edge}");
    }
    Ok(())
}

/// What a target-loss case does to the bench right after the release.
type Loss = fn(&mut Bench) -> Result<(), String>;

fn dies(bench: &mut Bench) -> Result<(), String> {
    bench
        .world
        .entities
        .get_mut(&bench.target)
        .ok_or("target")?
        .hp = SimScalar::ZERO;
    Ok(())
}

fn turns_invulnerable(bench: &mut Bench) -> Result<(), String> {
    bench
        .world
        .entities
        .get_mut(&bench.target)
        .ok_or("target")?
        .is_invulnerable = true;
    Ok(())
}

fn cloaks(bench: &mut Bench) -> Result<(), String> {
    bench
        .world
        .entities
        .get_mut(&bench.target)
        .ok_or("target")?
        .is_cloaked = true;
    Ok(())
}

fn blinks(bench: &mut Bench) -> Result<(), String> {
    let entity = bench
        .world
        .entities
        .get_mut(&bench.target)
        .ok_or("target")?;
    let away = SimVec2::new(
        entity.position.x,
        entity.position.y + SimScalar::from_i32(6),
    );
    entity.teleport_to(away);
    Ok(())
}

fn loads(bench: &mut Bench) -> Result<(), String> {
    let kind = bench.world.entities[&bench.target].unit_type_id;
    let carrier = bench
        .world
        .spawn_unit(kind, PlayerId(1), SimVec2::from_i32(28, 28))
        .map_err(|error| format!("{error:?}"))?;
    bench
        .world
        .try_load_unit_into_transport(bench.target, carrier)
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
}

/// Plays `loss` on `bench` after the release: the missile flies on to the target's point at the
/// release and lands there, and the target's vitals do not change after the loss.
fn lands_on_the_last_point(mut bench: Bench, loss: Loss, case: &str) -> TestResult {
    let last = bench.world.entities[&bench.target].position;
    let mut after_loss = None;
    let flight = bench.fly(|bench| {
        loss(bench)?;
        after_loss = Some(bench.vitals(bench.target));
        Ok(())
    })?;
    let lost = flight.states.last().ok_or("the missile flew on")?;
    assert_eq!(lost.target_id, None, "{case}: the target is lost");
    assert_eq!(lost.target_pos, last, "{case}: it flies to the last point");
    if let (Some(before), Some(now)) = (after_loss.flatten(), bench.vitals(bench.target)) {
        assert_eq!(now, before, "{case}: the lost missile strikes no unit");
    }
    Ok(())
}

/// W-13: for both weapons, a target that dies, turns invulnerable, cloaks undetected or blinks
/// in flight, or (the ground target) is loaded, is lost: the missile lands on the last point and
/// damages nothing (§2.2 rule 3: `ValidateWeapon`, `ValidateTeleport`, `PointFallback`).
#[test]
fn a_mover_weapons_lost_target_is_missed_on_the_last_point() -> TestResult {
    let world = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    let phoenix = air_mover_unit(&world)?;
    let spine = ground_mover_structure(&world)?;
    let victim = missile_archetype(&world)?;
    let air: [(&str, Loss); 4] = [
        ("dies", dies),
        ("invulnerable", turns_invulnerable),
        ("cloaks", cloaks),
        ("blinks", blinks),
    ];
    for (seed, (case, loss)) in (0x530_u64..).zip(air) {
        let bench = shipped_duel(seed, &phoenix, &phoenix, scalar(10.0), scalar(5.0), |_| {})?;
        lands_on_the_last_point(bench, loss, &format!("air, {case}"))?;
    }
    let ground: [(&str, Loss); 5] = [
        ("dies", dies),
        ("loaded", loads),
        ("invulnerable", turns_invulnerable),
        ("cloaks", cloaks),
        ("blinks", blinks),
    ];
    for (seed, (case, loss)) in (0x540_u64..).zip(ground) {
        let bench = shipped_duel(seed, &spine, &victim, scalar(10.0), scalar(2.0), |_| {})?;
        lands_on_the_last_point(bench, loss, &format!("ground, {case}"))?;
    }
    Ok(())
}

/// W-14: both weapons release again one period after a release, whatever the flight between
/// (the random delay cleared so the period is exact).
#[test]
fn a_mover_weapons_cooldown_runs_from_the_release() -> TestResult {
    let world = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    let phoenix = air_mover_unit(&world)?;
    let spine = ground_mover_structure(&world)?;
    let victim = missile_archetype(&world)?;
    let no_delay = |copy: &mut UnitArchetype| {
        for weapon in [
            &mut copy.weapon,
            &mut copy.ground_weapon,
            &mut copy.air_weapon,
        ]
        .into_iter()
        .flatten()
        {
            weapon.random_delay_min = SimScalar::ZERO;
            weapon.random_delay_max = SimScalar::ZERO;
        }
    };
    for (seed, (attacker, target, edge)) in
        (0x550_u64..).zip([(&phoenix, &phoenix, 5.0), (&spine, &victim, 2.0)])
    {
        let mut bench = shipped_duel(seed, attacker, target, scalar(10.0), scalar(edge), no_delay)?;
        let period = attacker
            .weapon_against(target.mover_type, None)
            .map(|w| u32::from(w.period_ticks))
            .ok_or("armed")?;
        bench
            .world
            .entities
            .get_mut(&bench.shooter)
            .ok_or("the shooter exists")?
            .issue_order(UnitOrder::attack(bench.target), false)
            .map_err(|error| format!("{error:?}"))?;
        let mut releases = Vec::new();
        for tick in 1..=200 {
            let snapshot = bench.world.step(&bundle(tick));
            if snapshot
                .combat_events
                .iter()
                .any(|event| matches!(event, CombatEventVisual::ProjectileVisual { .. }))
            {
                releases.push(tick);
            }
            if releases.len() == 3 {
                break;
            }
        }
        assert_eq!(releases.len(), 3, "{}: three releases", attacker.slug);
        assert_eq!(releases[1] - releases[0], period, "{}", attacker.slug);
        assert_eq!(releases[2] - releases[1], period, "{}", attacker.slug);
    }
    Ok(())
}

/// W-15: a missile weapon with a `Hidden` launch, released at a unit whose owner does not see
/// it, leaves that owner no image (D-7); its `Visible` impact reveals it as before. The shipped
/// catalog's one such weapon, the Mothership Carrier analogue's salvo, became a launch weapon
/// (plan `2026-11-28-carrier-interceptors` WP3, B-22), so the rule is checked on a copy of the
/// shipped ground missile weapon with its launch made `Hidden`.
#[test]
fn the_launchless_salvo_shows_nothing_at_its_release() -> TestResult {
    let world = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    let victim = missile_archetype(&world)?;
    let mut carrier = victim.clone();
    for weapon in [
        carrier.weapon.as_mut(),
        carrier.ground_weapon.as_mut(),
        carrier.air_weapon.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        weapon.launch_visibility = AttackVisibility::Hidden;
    }
    let mut bench = shipped_duel(
        0x560,
        &carrier,
        &victim,
        SimScalar::ONE,
        scalar(6.0),
        |_| {},
    )?;
    let mut listed = Vec::new();
    let mut revealed_at_release = None;
    let flight = bench.fly(|bench| {
        listed = images(&bench.world, 1);
        revealed_at_release =
            Some(bench.world.entities[&bench.shooter].is_attack_revealed_to(PlayerId(1)));
        Ok(())
    })?;
    assert!(
        !listed.contains(&bench.shooter),
        "no image at the release: {listed:?}"
    );
    assert_eq!(revealed_at_release, Some(false));
    assert!(flight.landed.is_some(), "the salvo lands");
    if let AttackVisibility::Visible { .. } = carrier
        .weapon_against(MoverType::Ground, None)
        .map_or(AttackVisibility::Snapshot, |w| w.attack_visibility)
    {
        assert!(
            bench.world.entities[&bench.shooter].is_attack_revealed_to(PlayerId(1)),
            "its impact reveals it"
        );
    }
    Ok(())
}

/// W-16: a release of either mover weapon at a unit whose owner does not see the attacker lists
/// the attacker's image for that owner on the release tick (owner answer Q0128 1(a)).
#[test]
fn a_mover_weapons_release_shows_its_attacker() -> TestResult {
    let world = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    let phoenix = air_mover_unit(&world)?;
    let spine = ground_mover_structure(&world)?;
    let victim = missile_archetype(&world)?;
    for (seed, (attacker, target, edge)) in
        (0x570_u64..).zip([(&phoenix, &phoenix, 5.0), (&spine, &victim, 5.0)])
    {
        let mut bench = shipped_duel(seed, attacker, target, SimScalar::ONE, scalar(edge), |_| {})?;
        let mut listed = Vec::new();
        bench.fly(|bench| {
            listed = images(&bench.world, 1);
            Ok(())
        })?;
        assert!(
            listed.contains(&bench.shooter),
            "{}: image at the release",
            attacker.slug
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The High Templar analogue's bolt (M-9, plan `2026-11-28-missile-movers-b52` WP2)
// ---------------------------------------------------------------------------------------------

/// The shipped mobile ground caster whose only weapon is a constant missile and which casts an
/// energy-costing spell at a position: the High Templar analogue (Psionic Storm). The Ravager
/// analogue's bile costs no energy and the Queen analogue has an air weapon block.
fn bolt_caster(world: &SimWorld) -> Result<UnitArchetype, String> {
    let found: Vec<&UnitArchetype> = world
        .catalog
        .archetypes()
        .filter(|arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && arch.air_weapon.is_none()
                && arch
                    .weapon
                    .as_ref()
                    .is_some_and(|w| w.projectile_speed.is_some() && w.missile_movers.is_empty())
                && arch.abilities.iter().any(|ability| {
                    ability.target_kind == AbilityTargetKind::TargetPosition
                        && ability.energy_cost > SimScalar::ZERO
                })
        })
        .collect();
    match found.as_slice() {
        [one] => Ok((*one).clone()),
        other => Err(format!("one bolt caster, found {}", other.len())),
    }
}

/// M-9: the High Templar analogue's bolt, released at a held target 0.63 and 5.9 world units away
/// edge to edge, lands after `ceil(path / 1.05)` ticks, `MissileDefault`'s 18.75 (S12), one tick
/// before a bolt of 1.0 per tick would (§2.4: paths 1.005 and 6.275 with its radius 0.375).
#[test]
fn the_bolt_flies_missile_defaults_speed() -> TestResult {
    let world = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    let caster = bolt_caster(&world)?;
    let victim = missile_archetype(&world)?;
    for (seed, edge) in (0x600_u64..).zip([0.63, 5.9]) {
        let mut bench = shipped_duel(seed, &caster, &victim, scalar(10.0), scalar(edge), |_| {})?;
        let path = bench.shooter_radius + scalar(edge);
        let ticks = |speed: SimScalar| (path / speed).saturating_ceil().saturating_to_u32();
        assert!(
            (value(bench.speed) - 1.05).abs() < 1e-6,
            "MissileDefault's 18.75 x 0.056, not {:?}",
            bench.speed
        );
        let flight = bench.fly(|_| Ok(()))?;
        // At 0.63 the bolt lands on its release tick, so no state of its flight remains.
        assert!(
            flight.states.iter().all(|state| state.mover.is_none()),
            "a constant missile"
        );
        assert_eq!(flight.ticks(), Some(ticks(bench.speed)), "edge {edge}");
        assert_eq!(
            flight.ticks(),
            Some(ticks(SimScalar::ONE) - 1),
            "edge {edge}: one tick before a bolt of 1.0"
        );
    }
    Ok(())
}
