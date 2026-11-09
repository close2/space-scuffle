//! The attack cycle published in the render snapshot (`BubbleRenderInstance::attack_swing`).
//!
//! # Responsibility
//! Pins the contract of plan `2026-11-04-battle-flipbook-animation` D-7 through the public tick
//! API: from the tick a swing's wind-up starts until the weapon is ready again, the snapshot
//! carries `elapsed_ticks` counted from that start, the resolved damage point and the resolved
//! period, all integers derived from the weapon state machine with no new simulation state.
//!
//! # How it fits
//! The client times the Attack flipbook from this record (the release frame shows on the
//! release tick), so the record must agree tick for tick with what combat does. Both read the
//! weapon through one function, `SimWorld::resolved_weapon`; these tests observe the agreement
//! from outside: the published period is the measured distance between swing starts, and the
//! release tick (the target loses life) is published as `elapsed_ticks == damage_point_ticks`.
//!
//! # Invariants
//! - Archetypes are chosen by capability and weapon shape, never by name (charter §3.3).
//! - Publishing the record changes no checksum: the snapshot is not part of the world state.

#![allow(clippy::expect_used, clippy::panic)]

use sim_core::catalog::UnitArchetype;
use sim_core::{SimWorld, WeaponPhase};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AnimState, AttackSwing, EntityId, MoverType, PlayerCommand, PlayerId, RenderSnapshot,
    ShapeKind, SimScalar, SimVec2, Tick, TickBundle, UnitOrder, WeaponSpec,
};

/// Hit points given to both duellists so that neither dies inside a test window.
const DURABLE_HP: i32 = 100_000;

/// What one tick published and left behind, for the attacker and its target.
#[derive(Clone, Debug)]
struct Observation {
    /// The tick just stepped.
    tick: u32,
    /// The attacker's weapon phase after the tick.
    phase: WeaponPhase,
    /// The attacker's cooldown after the tick, in ticks.
    cooldown: u16,
    /// The attacker's published swing.
    swing: Option<AttackSwing>,
    /// The attacker's published action.
    anim: AnimState,
    /// The target's life plus shields after the tick, or `None` once it is gone.
    target_life: Option<SimScalar>,
}

/// One attacker and one target standing in range on a flat arena.
struct Duel {
    world: SimWorld,
    attacker: EntityId,
    target: EntityId,
    /// The attacker's catalog weapon against a ground target.
    weapon: WeaponSpec,
    /// The next tick to step.
    next_tick: u32,
}

/// The weapon `arch` uses against a ground target, without siege.
fn ground_weapon(arch: &UnitArchetype) -> Option<&WeaponSpec> {
    arch.weapon_against(MoverType::Ground, None)
}

/// A mobile ground archetype with the overclock capability and an instant (hitscan) ground
/// weapon that winds up and follows through for at least two ticks: its release is visible as
/// a life loss on the release tick, and a backswing can be interrupted after it.
fn duellist(world: &SimWorld) -> UnitArchetype {
    world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.overclock.is_some()
                && arch.mover_type == MoverType::Ground
                && matches!(arch.shape, ShapeKind::UnitSphere)
                && ground_weapon(arch).is_some_and(|weapon| {
                    weapon.damage_point_ticks >= 1
                        && weapon.backswing_ticks >= 2
                        && weapon.projectile_speed.is_none()
                        && weapon.splash.is_none()
                        && !weapon.suicide_on_attack
                })
        })
        .cloned()
        .expect("the shipped catalog has an overclock-capable hitscan ground unit")
}

impl Duel {
    /// Spawns the duel; with `overclock`, the attacker is overclocked before its first swing.
    fn new(overclock: bool) -> Self {
        let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat 64x64 map is valid");
        let mut world = sim_fixtures::world(map, 0x0A77_AC4E);
        // The duel pins the published period; the random firing delay (B-3) would spread it.
        sim_fixtures::without_random_delays(&mut world.catalog);
        let arch = duellist(&world);
        let weapon = ground_weapon(&arch)
            .cloned()
            .expect("invariant: duellist() requires a ground weapon");
        let at = |x: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(32));
        let attacker = world
            .spawn_unit(arch.unit_type_id, PlayerId(0), at(30))
            .expect("the attacker spawns");
        let target = world
            .spawn_unit(arch.unit_type_id, PlayerId(1), at(33))
            .expect("the target spawns");
        for id in [attacker, target] {
            let entity = world
                .entities
                .get_mut(&id)
                .expect("invariant: spawned just above");
            entity.max_hp = SimScalar::from_i32(DURABLE_HP);
            entity.hp = SimScalar::from_i32(DURABLE_HP);
        }
        if overclock {
            let spec = arch
                .overclock
                .expect("invariant: duellist() requires overclock");
            let applied = world
                .entities
                .get_mut(&attacker)
                .expect("invariant: spawned just above")
                .apply_overclock(spec)
                .expect("the authored overclock is well formed");
            assert!(applied, "a durable attacker can pay for its overclock");
        }
        Self {
            world,
            attacker,
            target,
            weapon,
            next_tick: 0,
        }
    }

    /// Steps one tick with `commands` and observes the result.
    fn step(&mut self, commands: Vec<PlayerCommand>) -> Observation {
        let tick = self.next_tick;
        let snapshot = self.world.step(&TickBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        });
        self.next_tick += 1;
        let attacker = self
            .world
            .entities
            .get(&self.attacker)
            .expect("the durable attacker survives");
        let bubble = published(&snapshot, self.attacker);
        Observation {
            tick,
            phase: attacker.weapon_phase.clone(),
            cooldown: attacker.weapon_cooldown_remaining,
            swing: bubble.attack_swing,
            anim: bubble.anim_state,
            target_life: self
                .world
                .entities
                .get(&self.target)
                .map(|target| target.hp + target.shields),
        }
    }

    /// The command that sends the attacker at its target.
    fn attack_order(&self) -> PlayerCommand {
        PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![self.attacker],
            order: UnitOrder::attack(self.target),
            queue: false,
        }
    }

    /// Orders the attack, then runs `ticks` more ticks; returns every observation.
    fn fight(&mut self, ticks: u32) -> Vec<Observation> {
        let order = self.attack_order();
        let mut seen = vec![self.step(vec![order])];
        seen.extend((0..ticks).map(|_| self.step(Vec::new())));
        seen
    }
}

/// The record of `id` in `snapshot`, which orders records by entity ID.
fn published(snapshot: &RenderSnapshot, id: EntityId) -> sim_types::BubbleRenderInstance {
    let index = snapshot
        .bubbles
        .binary_search_by_key(&id, |bubble| bubble.entity_id)
        .expect("the attacker is published");
    snapshot.bubbles[index].clone()
}

/// The ticks on which a wind-up began: the phase is `Windup` with the whole damage point left.
fn swing_starts(seen: &[Observation], damage_point: u16) -> Vec<u32> {
    seen.iter()
        .filter(|o| {
            matches!(o.phase, WeaponPhase::Windup { remaining_ticks, .. } if remaining_ticks == damage_point)
        })
        .map(|o| o.tick)
        .collect()
}

/// The ticks on which the target lost life.
fn release_ticks(seen: &[Observation]) -> Vec<u32> {
    seen.windows(2)
        .filter(|pair| pair[1].target_life < pair[0].target_life)
        .map(|pair| pair[1].tick)
        .collect()
}

/// The observation of `tick`.
fn at(seen: &[Observation], tick: u32) -> Observation {
    seen.iter()
        .find(|o| o.tick == tick)
        .cloned()
        .expect("the tick was observed")
}

/// Checks that every tick from `start` to `end` (exclusive) publishes the swing begun at
/// `start`, with `elapsed_ticks` counting from it.
fn assert_counts_from(seen: &[Observation], start: u32, end: u32, damage_point: u16, period: u16) {
    for tick in start..end {
        let expected = AttackSwing {
            elapsed_ticks: u16::try_from(tick - start).expect("a test window fits u16"),
            damage_point_ticks: damage_point,
            period_ticks: period,
        };
        let o = at(seen, tick);
        assert_eq!(
            o.swing,
            Some(expected),
            "tick {tick} of the swing begun at {start} (phase {:?}, cooldown {})",
            o.phase,
            o.cooldown
        );
    }
}

#[test]
fn swing_counts_from_the_windup_through_the_release_to_the_next_swing() {
    let mut duel = Duel::new(false);
    let damage_point = duel.weapon.damage_point_ticks;
    let period = duel.weapon.period_ticks;
    let seen = duel.fight(u32::from(period) * 4);

    let starts = swing_starts(&seen, damage_point);
    assert!(starts.len() >= 3, "the duel swings repeatedly: {starts:?}");
    let releases = release_ticks(&seen);
    for pair in starts.windows(2) {
        let (start, next) = (pair[0], pair[1]);
        assert_eq!(
            next - start,
            u32::from(period),
            "swings start one period apart"
        );
        assert_counts_from(&seen, start, next, damage_point, period);
        assert_eq!(
            at(&seen, start).swing.map(|s| s.elapsed_ticks),
            Some(0),
            "the wind-up's first tick is elapsed 0"
        );
        let release = start + u32::from(damage_point);
        assert!(
            releases.contains(&release),
            "the target loses life on the release tick {release}: {releases:?}"
        );
        assert_eq!(
            at(&seen, release).swing.map(|s| s.elapsed_ticks),
            Some(damage_point),
            "the release tick is elapsed = damage point"
        );
        assert_eq!(
            at(&seen, next - 1).swing.map(|s| s.elapsed_ticks),
            Some(period - 1),
            "the tick before the next wind-up is elapsed = period - 1"
        );
    }
    assert!(
        seen.iter()
            .take_while(|o| o.tick < starts[0])
            .all(|o| o.swing.is_none()),
        "no swing is published before the first wind-up"
    );

    // Leave the target with no life to spare: the next shot kills it, and the weapon runs its
    // cooldown out with nothing to attack.
    let target = duel
        .world
        .entities
        .get_mut(&duel.target)
        .expect("the durable target is alive");
    target.hp = SimScalar::HALF;
    target.shields = SimScalar::ZERO;
    let tail: Vec<Observation> = (0..u32::from(period) * 3)
        .map(|_| duel.step(Vec::new()))
        .collect();
    let killed = tail
        .iter()
        .find(|o| o.target_life.is_none())
        .expect("the weakened target dies");
    let last_start = *swing_starts(&seen, damage_point)
        .iter()
        .chain(swing_starts(&tail, damage_point).iter())
        .filter(|&&start| start <= killed.tick)
        .max()
        .expect("a swing killed the target");
    let ready = tail
        .iter()
        .find(|o| o.tick > killed.tick && o.phase == WeaponPhase::Ready && o.cooldown == 0)
        .expect("the weapon returns to Ready once its cooldown ends");
    let combined: Vec<Observation> = seen.iter().chain(tail.iter()).cloned().collect();
    assert_counts_from(&combined, last_start, ready.tick, damage_point, period);
    assert_eq!(ready.tick - last_start, u32::from(period));
    assert_eq!(
        ready.swing, None,
        "a ready weapon with no target publishes no swing"
    );
    assert!(
        tail.iter()
            .filter(|o| o.tick >= ready.tick)
            .all(|o| o.swing.is_none()),
        "nothing is published after the cycle ends"
    );
}

#[test]
fn overclock_shortens_the_published_period_to_the_cycle_combat_runs() {
    let mut plain = Duel::new(false);
    let mut overclocked = Duel::new(true);
    let damage_point = plain.weapon.damage_point_ticks;
    let base_period = plain.weapon.period_ticks;
    let window = u32::from(base_period) * 4;
    let plain_seen = plain.fight(window);
    let fast_seen = overclocked.fight(window);

    let plain_starts = swing_starts(&plain_seen, damage_point);
    let published_plain = at(&plain_seen, plain_starts[0])
        .swing
        .expect("a wind-up publishes its swing")
        .period_ticks;
    assert_eq!(published_plain, base_period, "no buff: the catalog period");

    let fast_starts = swing_starts(&fast_seen, damage_point);
    assert!(
        fast_starts.len() >= 4,
        "the overclocked duel swings: {fast_starts:?}"
    );
    let published_fast = at(&fast_seen, fast_starts[0])
        .swing
        .expect("a wind-up publishes its swing")
        .period_ticks;
    assert!(
        published_fast < base_period,
        "overclock shortens the published period: {published_fast} vs {base_period}"
    );
    for pair in fast_starts.windows(2) {
        assert_eq!(
            pair[1] - pair[0],
            u32::from(published_fast),
            "combat runs the cycle the snapshot publishes"
        );
        assert_counts_from(&fast_seen, pair[0], pair[1], damage_point, published_fast);
    }
}

#[test]
fn a_stutter_step_walks_while_the_swing_runs_on_in_cooldown() {
    let mut duel = Duel::new(false);
    let damage_point = duel.weapon.damage_point_ticks;
    let period = duel.weapon.period_ticks;
    let order = duel.attack_order();
    let mut seen = vec![duel.step(vec![order])];
    while !matches!(
        seen.last().map(|o| &o.phase),
        Some(WeaponPhase::Backswing { .. })
    ) {
        assert!(seen.len() < 200, "the attacker releases a shot");
        seen.push(duel.step(Vec::new()));
    }
    let start = *swing_starts(&seen, damage_point)
        .last()
        .expect("the released swing wound up first");

    // Step away during the backswing.
    let step_away = PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![duel.attacker],
        order: UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_i32(10), SimScalar::from_i32(32)),
        },
        queue: false,
    };
    seen.push(duel.step(vec![step_away]));
    let cancelled = seen.last().cloned().expect("just pushed");
    assert_eq!(
        cancelled.phase,
        WeaponPhase::Cooldown,
        "the move cancels the backswing into cooldown"
    );
    while seen.last().is_some_and(|o| o.cooldown > 0) {
        seen.push(duel.step(Vec::new()));
    }
    let ready = seen.last().cloned().expect("just pushed");
    assert_eq!(ready.tick - start, u32::from(period));
    assert_counts_from(&seen, start, ready.tick, damage_point, period);
    assert!(
        seen.iter()
            .any(|o| o.tick < ready.tick && o.anim == AnimState::Walk && o.swing.is_some()),
        "the stutter step publishes Walk while the swing is still published"
    );
    assert_eq!(ready.swing, None, "the walking unit's cycle has ended");
}

/// Runs the default sandbox battle for 1000 ticks; with `extra_snapshots`, also builds two
/// more snapshots after every tick. Returns the checksums and the number of published swings.
fn sandbox_battle(extra_snapshots: bool) -> (Vec<u64>, usize) {
    let mut world =
        sim_fixtures::default_battle_sandbox().expect("invariant: the default sandbox builds");
    let charge = |world: &SimWorld, player: PlayerId, x: i32| PlayerCommand {
        player_id: player,
        entities: world
            .entities
            .values()
            .filter(|e| e.owner == player && !e.is_immobile)
            .map(|e| e.id)
            .collect(),
        order: UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(32)),
        },
        queue: false,
    };
    let opening = vec![
        charge(&world, PlayerId(0), 50),
        charge(&world, PlayerId(1), 10),
    ];
    let mut checksums = Vec::new();
    let mut swings = 0;
    for tick in 0..1000_u32 {
        let commands = if tick == 0 {
            opening.clone()
        } else {
            Vec::new()
        };
        let snapshot = world.step(&TickBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        });
        swings += snapshot
            .bubbles
            .iter()
            .filter(|b| b.attack_swing.is_some())
            .count();
        if extra_snapshots {
            for _ in 0..2 {
                let again = world.build_render_snapshot();
                assert_eq!(again.bubbles, snapshot.bubbles, "a snapshot is a pure read");
            }
        }
        checksums.push(world.last_checksum());
    }
    (checksums, swings)
}

#[test]
fn publishing_swings_changes_no_checksum_over_a_thousand_ticks() {
    let (plain, swings) = sandbox_battle(false);
    let (observed, _) = sandbox_battle(true);
    assert!(swings > 0, "the sandbox battle fights");
    assert_eq!(plain, observed);
}
