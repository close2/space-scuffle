//! The follow order (plan `2026-11-26-follow-order`; B-163, B-166's remaining half, B-187).
//!
//! # Responsibility
//! Checks `UnitOrder::Follow` end to end through `PlayerCommand`s: its admission and typed
//! refusals (P-2), and the takers it filters; following a walking friend within the dead band
//! (P-3); rest, slop and resume (P-4); how a follow ends (P-5); a group bunching round its
//! target (P-6); and resting followers yielding as idle bodies (P-8), against constructed worlds
//! on open ground.
//!
//! # Invariants
//! Archetypes are chosen by capability (mobility, the A command of the default stance, transport
//! capacity, the structure shape), never by identity. Radii, speeds and margins are read from the
//! catalog, never from a seed's outcome.

use sim_core::movement::{checked_distance, MovementAdmissionError, MovementReceipt};
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AnimState, AttackCommand, EntityId, FollowKind, MoverType, PlayerCommand, PlayerId, ShapeKind,
    SimScalar, SimVec2, StanceOrders, Tick, TickBundle, UnitOrder, UnitTypeId,
};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);
const THEM: PlayerId = PlayerId(1);
const SIDE: u32 = 48;

/// The movement tuning's `ARRIVAL_TOUCH_MARGIN`, 0.15 world units, in raw Q32.32: the rest band
/// is the radius sum plus this (plan D-4 rule 1).
const TOUCH_MARGIN_RAW: i128 = (15_i128 << 32) / 100;
/// The movement tuning's `STALL_ABANDON_TICKS`: ten seconds of 25 ticks.
const STALL_ABANDON_TICKS: u32 = 250;
/// The movement tuning's `STALL_REPATH_TICKS`: one second of 25 ticks.
const STALL_REPATH_TICKS: u32 = 25;
/// The contact rules' `dead_band_permille` reader: D-α's resting bound, in thousandths of the
/// radius sum.
fn resting_bound_raw(world: &SimWorld, radius_sum: i128) -> i128 {
    radius_sum * i128::from(world.catalog.movement_rules().contact().dead_band_permille) / 1000
}

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

/// The archetypes the tests use, each found by its capabilities.
struct Kinds {
    /// A mobile ground unit whose default stance's A command is Attack, carrying no one.
    soldier: UnitTypeId,
    /// A mobile unit whose default stance's A command is Scan Move (unarmed), carrying no one.
    scanner: UnitTypeId,
    /// A mobile carrier.
    carrier: UnitTypeId,
    /// A structure.
    structure: UnitTypeId,
    /// A mobile ground unit that reaches its top speed and stops within one tick (its
    /// acceleration and deceleration are at least its top speed), so a follower of its own kind
    /// never lags by acceleration.
    sprinter: UnitTypeId,
    /// Another mobile armed ground archetype, its default stance rewritten by the fixture to carry
    /// no A command.
    unarmed_no_a: UnitTypeId,
}

struct Fixture {
    world: SimWorld,
    kinds: Kinds,
}

fn find(world: &SimWorld, pick: impl Fn(&sim_core::UnitArchetype) -> bool) -> Option<UnitTypeId> {
    world
        .catalog
        .archetypes()
        .find(|archetype| pick(archetype))
        .map(|archetype| archetype.unit_type_id)
}

fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let map = GameMap::new(SIDE, SIDE, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 41);
    let mobile = |a: &sim_core::UnitArchetype| {
        !a.is_immobile_by_default() && a.capabilities.transport_capacity == 0
    };
    let soldier = find(&world, |a| {
        mobile(a)
            && a.mover_type == MoverType::Ground
            && a.stance_orders(None).attack == AttackCommand::Attack
    })
    .ok_or("no mobile armed ground unit")?;
    let scanner = find(&world, |a| {
        mobile(a) && a.stance_orders(None).attack == AttackCommand::ScanMove
    })
    .ok_or("no mobile unarmed unit")?;
    let carrier = find(&world, |a| {
        !a.is_immobile_by_default() && a.capabilities.transport_capacity > 0
    })
    .ok_or("no carrier")?;
    let sprinter = find(&world, |a| {
        mobile(a)
            && a.mover_type == MoverType::Ground
            && a.acceleration >= a.max_speed
            && a.deceleration >= a.max_speed
            && a.stance_orders(None).attack == AttackCommand::Attack
    })
    .ok_or("no instant ground unit")?;
    let structure = find(&world, |a| {
        matches!(a.shape, ShapeKind::BuildingDome { .. })
    })
    .ok_or("no structure")?;
    let other = find(&world, |a| {
        mobile(a)
            && a.unit_type_id != soldier
            && a.mover_type == MoverType::Ground
            && a.stance_orders(None).attack == AttackCommand::Attack
    })
    .ok_or("no second mobile armed ground unit")?;
    let unarmed_no_a = with_no_a_command(&mut world, other)?;
    Ok(Fixture {
        world,
        kinds: Kinds {
            soldier,
            scanner,
            carrier,
            structure,
            sprinter,
            unarmed_no_a,
        },
    })
}

/// Rewrites `base`'s default stance in `world`'s catalog to carry no A command, and returns it.
fn with_no_a_command(world: &mut SimWorld, base: UnitTypeId) -> Result<UnitTypeId, Box<dyn Error>> {
    let archetype = world.catalog.get_mut(base)?;
    let orders = archetype.stance_orders(None);
    archetype.default_stance_orders = Some(StanceOrders {
        attack: AttackCommand::None,
        ..orders
    });
    Ok(base)
}

impl Fixture {
    fn spawn(
        &mut self,
        unit: UnitTypeId,
        owner: PlayerId,
        pos: SimVec2,
    ) -> Result<EntityId, Box<dyn Error>> {
        Ok(self.world.spawn_unit(unit, owner, pos)?)
    }

    fn command(
        &mut self,
        player: PlayerId,
        entities: Vec<EntityId>,
        order: UnitOrder,
        queue: bool,
    ) {
        let tick = self.world.current_tick;
        self.world.step(&TickBundle {
            tick: Tick(tick),
            commands: vec![PlayerCommand {
                player_id: player,
                entities,
                order,
                queue,
            }],
            state_checksum: None,
        });
    }

    fn follow(&mut self, entities: Vec<EntityId>, target: EntityId, kind: FollowKind, queue: bool) {
        self.command(ME, entities, UnitOrder::Follow { target, kind }, queue);
    }

    /// Admits `order` directly, between ticks, through the public movement admission: a unit
    /// destroyed this tick is still in the table there, as it is when its tick's commands run.
    fn admit(
        &mut self,
        player: PlayerId,
        entities: &[EntityId],
        order: &UnitOrder,
    ) -> Result<MovementReceipt, MovementAdmissionError> {
        self.world
            .try_issue_movement(player, entities, order, false)
    }

    fn outcome(&self) -> Result<MovementReceipt, MovementAdmissionError> {
        self.world
            .movement_outcomes()
            .last()
            .map_or(Err(MovementAdmissionError::EmptySelection), |o| {
                o.result.clone()
            })
    }

    fn idle(&mut self, ticks: u32) {
        for _ in 0..ticks {
            let tick = self.world.current_tick;
            self.world.step(&TickBundle {
                tick: Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            });
            assert_eq!(self.world.diagnostics(), []);
        }
    }

    fn front(&self, id: EntityId) -> Option<UnitOrder> {
        self.world.entities[&id].order_queue.front().cloned()
    }

    /// `id`'s collision radius, raw Q32.32.
    fn radius(&self, id: EntityId) -> i128 {
        let kind = self.world.entities[&id].unit_type_id;
        self.world
            .catalog
            .get(kind)
            .map_or(0, |a| i128::from(a.collision_radius.raw()))
    }

    /// `id`'s top speed, world units per tick, raw Q32.32.
    fn speed(&self, id: EntityId) -> i128 {
        let kind = self.world.entities[&id].unit_type_id;
        self.world
            .catalog
            .get(kind)
            .map_or(0, |a| i128::from(a.max_speed.raw()))
    }

    /// The edge gap between `a` and `b`: centre distance less both radii, raw Q32.32.
    fn gap(&self, a: EntityId, b: EntityId) -> i128 {
        let (pa, pb) = (
            self.world.entities[&a].position,
            self.world.entities[&b].position,
        );
        let centre = checked_distance(pa, pb).map_or(i128::MAX, |d| i128::from(d.raw()));
        centre - self.radius(a) - self.radius(b)
    }

    /// The catalog's follow slop, raw Q32.32.
    fn slop(&self) -> i128 {
        i128::from(
            self.world
                .catalog
                .movement_rules()
                .follow_range_slop()
                .raw(),
        )
    }

    /// Whether `id` rests under its follow.
    fn rests(&self, id: EntityId) -> bool {
        self.world.entities[&id].movement.follow_rest.is_some()
    }

    /// Steps until `done` holds, at most `limit` ticks; returns the ticks taken.
    fn idle_until(&mut self, limit: u32, done: impl Fn(&Self) -> bool) -> Option<u32> {
        for tick in 1..=limit {
            self.idle(1);
            if done(self) {
                return Some(tick);
            }
        }
        None
    }

    fn walk(&mut self, id: EntityId, to: SimVec2, queue: bool) {
        self.command(ME, vec![id], UnitOrder::Move { target: to }, queue);
    }

    fn teleport(&mut self, id: EntityId, to: SimVec2) {
        if let Some(entity) = self.world.entities.get_mut(&id) {
            entity.teleport_to(to);
            entity.velocity = SimVec2::ZERO;
        }
    }
}

fn follow_of(target: EntityId, kind: FollowKind) -> UnitOrder {
    UnitOrder::Follow { target, kind }
}

/// P-2: every refusal of the target is typed and leaves every recipient's orders alone.
#[test]
fn a_follow_refuses_a_missing_dead_carried_housed_or_structure_target() -> TestResult {
    let mut f = fixture()?;
    let a = f.spawn(f.kinds.soldier, ME, at(10, 10))?;
    let dead = f.spawn(f.kinds.soldier, ME, at(14, 10))?;
    let housed = f.spawn(f.kinds.soldier, ME, at(16, 10))?;
    let carrier = f.spawn(f.kinds.carrier, ME, at(30, 30))?;
    let passenger = f.spawn(f.kinds.soldier, ME, at(30, 32))?;
    let dome = f.world.spawn_structure(f.kinds.structure, ME, 2, 30)?;
    f.command(ME, vec![passenger], UnitOrder::Load { carrier }, false);
    f.idle(300);
    assert_eq!(
        f.world.entities[&passenger].transport_carrier(),
        Some(carrier),
        "the passenger boards first"
    );
    if let Some(entity) = f.world.entities.get_mut(&dead) {
        entity.hp = SimScalar::ZERO;
    }
    if let Some(entity) = f.world.entities.get_mut(&housed) {
        entity.task.housed_in = Some(dome);
    }
    let cases = [
        (
            EntityId(9_999),
            MovementAdmissionError::FollowTargetMissing(EntityId(9_999)),
        ),
        (dead, MovementAdmissionError::FollowTargetDead(dead)),
        (
            passenger,
            MovementAdmissionError::FollowTargetAway(passenger),
        ),
        (housed, MovementAdmissionError::FollowTargetAway(housed)),
        (dome, MovementAdmissionError::FollowTargetImmobile(dome)),
    ];
    for (target, refusal) in cases {
        let order = follow_of(target, FollowKind::Move);
        assert_eq!(
            f.admit(ME, &[a], &order),
            Err(refusal.clone()),
            "{target:?}"
        );
        assert_eq!(f.front(a), None, "a refused follow changes nothing");
    }
    Ok(())
}

/// P-2: the selection is checked as for a move, before the target.
#[test]
fn a_follow_refuses_a_foreign_dead_or_carried_recipient_and_an_empty_selection() -> TestResult {
    let mut f = fixture()?;
    let target = f.spawn(f.kinds.soldier, ME, at(20, 20))?;
    let mine = f.spawn(f.kinds.soldier, ME, at(10, 10))?;
    let theirs = f.spawn(f.kinds.soldier, THEM, at(40, 40))?;
    let dead = f.spawn(f.kinds.soldier, ME, at(12, 10))?;
    let carrier = f.spawn(f.kinds.carrier, ME, at(30, 30))?;
    let passenger = f.spawn(f.kinds.soldier, ME, at(30, 32))?;
    f.command(ME, vec![passenger], UnitOrder::Load { carrier }, false);
    f.idle(300);
    if let Some(entity) = f.world.entities.get_mut(&dead) {
        entity.hp = SimScalar::ZERO;
    }
    let cases = [
        (
            vec![mine, theirs],
            MovementAdmissionError::ForeignRecipient(theirs),
        ),
        (
            vec![mine, dead],
            MovementAdmissionError::DeadRecipient(dead),
        ),
        (
            vec![mine, passenger],
            MovementAdmissionError::LoadedRecipient(passenger),
        ),
        (Vec::new(), MovementAdmissionError::EmptySelection),
        (
            vec![mine, EntityId(9_999)],
            MovementAdmissionError::MissingRecipient(EntityId(9_999)),
        ),
    ];
    let order = follow_of(target, FollowKind::Move);
    for (selection, refusal) in cases {
        assert_eq!(
            f.admit(ME, &selection, &order),
            Err(refusal.clone()),
            "{refusal:?}"
        );
        assert_eq!(f.front(mine), None, "a refused follow changes nothing");
    }
    assert_eq!(
        f.admit(PlayerId(9), &[mine], &order),
        Err(MovementAdmissionError::InvalidPlayer(PlayerId(9)))
    );
    f.command(ME, vec![mine, theirs], order, false);
    assert_eq!(
        f.outcome(),
        Err(MovementAdmissionError::ForeignRecipient(theirs)),
        "a refusal through a player command is reported as its outcome"
    );
    Ok(())
}

/// P-2: any owner's unit may be followed; the target itself and immobile members are skipped,
/// and the receipt counts the takers.
#[test]
fn the_target_and_immobile_members_are_skipped_and_the_receipt_counts_the_takers() -> TestResult {
    let mut f = fixture()?;
    let target = f.spawn(f.kinds.soldier, ME, at(20, 20))?;
    let enemy = f.spawn(f.kinds.soldier, THEM, at(40, 40))?;
    let a = f.spawn(f.kinds.soldier, ME, at(10, 10))?;
    let b = f.spawn(f.kinds.scanner, ME, at(12, 10))?;
    let anchored = f.spawn(f.kinds.soldier, ME, at(14, 10))?;
    if let Some(entity) = f.world.entities.get_mut(&anchored) {
        entity.is_immobile = true;
    }
    f.follow(
        vec![a, b, anchored, target],
        target,
        FollowKind::Move,
        false,
    );
    assert_eq!(f.outcome(), Ok(MovementReceipt { recipient_count: 2 }));
    for id in [a, b] {
        assert_eq!(f.front(id), Some(follow_of(target, FollowKind::Move)));
    }
    assert_eq!(f.front(target), None, "the clicked unit keeps its orders");
    assert_eq!(f.front(anchored), None, "an anchored member is skipped");

    f.follow(vec![a], enemy, FollowKind::Move, false);
    assert_eq!(f.outcome(), Ok(MovementReceipt { recipient_count: 1 }));
    assert_eq!(f.front(a), Some(follow_of(enemy, FollowKind::Move)));

    f.follow(vec![target], target, FollowKind::Move, false);
    assert_eq!(
        f.outcome(),
        Ok(MovementReceipt { recipient_count: 0 }),
        "a follow of oneself alone is admitted with no taker"
    );
    assert_eq!(f.front(target), None);
    Ok(())
}

/// P-2: `AcquireMove` reaches only the members whose settled stance has an A command.
#[test]
fn acquire_move_skips_members_without_an_a_command() -> TestResult {
    let mut f = fixture()?;
    let target = f.spawn(f.kinds.scanner, ME, at(20, 20))?;
    let no_a = f.spawn(f.kinds.unarmed_no_a, ME, at(10, 10))?;
    let scanner = f.spawn(f.kinds.scanner, ME, at(12, 10))?;
    f.follow(vec![no_a, scanner], target, FollowKind::AcquireMove, false);
    assert_eq!(f.outcome(), Ok(MovementReceipt { recipient_count: 1 }));
    assert_eq!(
        f.front(scanner),
        Some(follow_of(target, FollowKind::AcquireMove))
    );
    assert_eq!(f.front(no_a), None);
    f.follow(vec![no_a], target, FollowKind::Move, false);
    assert_eq!(f.outcome(), Ok(MovementReceipt { recipient_count: 1 }));
    assert_eq!(
        f.front(no_a),
        Some(follow_of(target, FollowKind::Move)),
        "a plain follow needs no A command"
    );
    Ok(())
}

/// P-2: with Shift a follow is queued behind a move, and is the front order once the move ends.
/// A follow is no route order, so it leaves the route-order count alone.
#[test]
fn shift_queues_a_follow_behind_a_move() -> TestResult {
    let mut f = fixture()?;
    let target = f.spawn(f.kinds.soldier, ME, at(30, 10))?;
    let a = f.spawn(f.kinds.soldier, ME, at(10, 10))?;
    f.command(ME, vec![a], UnitOrder::Move { target: at(14, 10) }, false);
    let issued = f.world.entities[&a].route_orders_issued;
    f.follow(vec![a], target, FollowKind::Move, true);
    assert_eq!(f.outcome(), Ok(MovementReceipt { recipient_count: 1 }));
    assert_eq!(f.world.entities[&a].order_queue.len(), 2);
    assert_eq!(f.world.entities[&a].route_orders_issued, issued);
    f.idle(200);
    assert_eq!(f.front(a), Some(follow_of(target, FollowKind::Move)));
    Ok(())
}

/// P-3: a follower of a friend walking a polyline at its own top speed stays within the dead
/// band: from the first tick it comes within the rest band until the target stops, its edge gap
/// is at most the slop plus the target's step and the touch margin (D-3, D-4). Both are of a kind
/// that reaches its top speed and stops within one tick, so no acceleration lag enters the bound.
#[test]
fn a_follower_keeps_within_the_dead_band_of_a_walking_friend() -> TestResult {
    let mut f = fixture()?;
    let target = f.spawn(f.kinds.sprinter, ME, at(10, 12))?;
    let follower = f.spawn(f.kinds.sprinter, ME, at(5, 12))?;
    f.follow(vec![follower], target, FollowKind::Move, false);
    f.walk(target, at(24, 12), false);
    f.walk(target, at(24, 30), true);
    f.walk(target, at(40, 30), true);
    let bound = f.slop() + f.speed(target) + TOUCH_MARGIN_RAW;
    let mut touched = false;
    for tick in 0..900 {
        f.idle(1);
        let walking = !f.world.entities[&target].order_queue.is_empty();
        let gap = f.gap(follower, target);
        touched |= gap <= TOUCH_MARGIN_RAW;
        if touched && walking {
            assert!(gap <= bound, "tick {tick}: gap {gap} above {bound}");
        }
        assert_eq!(
            f.front(follower),
            Some(follow_of(target, FollowKind::Move)),
            "tick {tick}"
        );
        if !walking && touched {
            return Ok(());
        }
    }
    Err("the target never finished its walk with the follower in the band".into())
}

/// P-4: by a standing target the follower rests within the band (no velocity after its glide,
/// `Idle`, its order kept); a displacement of the target by exactly the slop keeps it resting,
/// one raw unit more walks it on the next tick, and it rests touching the target again.
#[test]
fn a_follower_rests_by_a_standing_target_and_resumes_only_past_the_slop() -> TestResult {
    let mut f = fixture()?;
    let target = f.spawn(f.kinds.soldier, ME, at(24, 24))?;
    let follower = f.spawn(f.kinds.soldier, ME, at(14, 24))?;
    f.follow(vec![follower], target, FollowKind::Move, false);
    let settled =
        |f: &Fixture| f.rests(follower) && f.world.entities[&follower].velocity == SimVec2::ZERO;
    f.idle_until(400, settled)
        .ok_or("the follower never came to rest")?;
    let entity = &f.world.entities[&follower];
    assert_eq!(entity.current_anim, AnimState::Idle);
    assert_eq!(f.front(follower), Some(follow_of(target, FollowKind::Move)));
    assert!(f.gap(follower, target) <= TOUCH_MARGIN_RAW);
    let anchor = entity.movement.follow_rest.ok_or("rests")?;
    let slop = f.world.catalog.movement_rules().follow_range_slop();

    f.teleport(target, SimVec2::new(anchor.x + slop, anchor.y));
    for tick in 0..50 {
        f.idle(1);
        assert_eq!(
            f.world.entities[&follower].movement.follow_rest,
            Some(anchor),
            "tick {tick}: a drift of exactly the slop keeps the rest"
        );
        assert_eq!(f.world.entities[&follower].velocity, SimVec2::ZERO);
    }

    let beyond = SimScalar::from_raw(slop.raw() + 1);
    f.teleport(target, SimVec2::new(anchor.x + beyond, anchor.y));
    f.idle(1);
    assert!(!f.rests(follower), "one raw unit past the slop walks it");
    assert_ne!(f.world.entities[&follower].velocity, SimVec2::ZERO);
    f.idle_until(200, settled)
        .ok_or("the follower never rested again")?;
    assert!(f.gap(follower, target) <= TOUCH_MARGIN_RAW);
    assert_eq!(f.front(follower), Some(follow_of(target, FollowKind::Move)));
    Ok(())
}

/// P-5: the follow ends when its target dies or boards a carrier, the follower gliding to rest.
#[test]
fn a_follow_ends_when_its_target_dies_or_boards() -> TestResult {
    let mut f = fixture()?;
    let doomed = f.spawn(f.kinds.soldier, ME, at(30, 10))?;
    let mourner = f.spawn(f.kinds.soldier, ME, at(10, 10))?;
    let boarder = f.spawn(f.kinds.soldier, ME, at(30, 30))?;
    let escort = f.spawn(f.kinds.soldier, ME, at(10, 30))?;
    let carrier = f.spawn(f.kinds.carrier, ME, at(36, 36))?;
    f.follow(vec![mourner], doomed, FollowKind::Move, false);
    f.follow(vec![escort], boarder, FollowKind::Move, false);
    f.idle(20);
    assert!(
        f.world.entities[&mourner].velocity != SimVec2::ZERO,
        "it walks"
    );
    if let Some(entity) = f.world.entities.get_mut(&doomed) {
        entity.hp = SimScalar::ZERO;
    }
    f.idle(2);
    assert_eq!(f.front(mourner), None, "the follow ended with its target");
    f.idle_until(25, |f| f.world.entities[&mourner].velocity == SimVec2::ZERO)
        .ok_or("the follower glides to rest")?;

    f.command(ME, vec![boarder], UnitOrder::Load { carrier }, false);
    f.idle_until(400, |f| {
        f.world.entities[&boarder].transport_carrier() == Some(carrier)
    })
    .ok_or("the target boards")?;
    f.idle(1);
    assert_eq!(
        f.front(escort),
        None,
        "the follow ended when its target boarded"
    );
    Ok(())
}

/// P-5: with a move queued behind it the follow ends once its target is reached, or at once when
/// the target dies first, and the move runs; alone it lasts.
#[test]
fn a_follow_with_an_order_behind_it_ends_on_reaching_its_target() -> TestResult {
    let mut f = fixture()?;
    let leader_a = f.spawn(f.kinds.soldier, ME, at(20, 10))?;
    let walker = f.spawn(f.kinds.soldier, ME, at(8, 10))?;
    let spot_p = at(20, 30);
    f.follow(vec![walker], leader_a, FollowKind::Move, false);
    f.walk(walker, spot_p, true);
    f.idle_until(300, |f| {
        !matches!(f.front(walker), Some(UnitOrder::Follow { .. }))
    })
    .ok_or("the follow never ended")?;
    assert!(
        f.gap(walker, leader_a) <= TOUCH_MARGIN_RAW + f.speed(walker),
        "it reached A"
    );
    assert_eq!(f.front(walker), Some(UnitOrder::Move { target: spot_p }));
    f.idle_until(400, |f| f.front(walker).is_none())
        .ok_or("the move behind the follow never ended")?;

    let leader_b = f.spawn(f.kinds.soldier, ME, at(40, 40))?;
    let second = f.spawn(f.kinds.soldier, ME, at(8, 40))?;
    let spot_q = at(8, 30);
    f.follow(vec![second], leader_b, FollowKind::Move, false);
    f.walk(second, spot_q, true);
    f.idle(5);
    if let Some(entity) = f.world.entities.get_mut(&leader_b) {
        entity.hp = SimScalar::ZERO;
    }
    f.idle(2);
    assert_eq!(f.front(second), Some(UnitOrder::Move { target: spot_q }));

    let leader = f.spawn(f.kinds.soldier, ME, at(30, 20))?;
    let loyal = f.spawn(f.kinds.soldier, ME, at(36, 20))?;
    f.follow(vec![loyal], leader, FollowKind::Move, false);
    f.walk(leader, at(30, 34), false);
    f.idle_until(300, |f| f.world.entities[&leader].order_queue.is_empty())
        .ok_or("the leader never stopped")?;
    f.idle(500);
    assert_eq!(f.front(loyal), Some(follow_of(leader, FollowKind::Move)));
    assert!(f.rests(loyal));
    Ok(())
}

/// The ids reachable from `root` through the "rests touching" relation of D-4: a follower within
/// the rest band of the target, or of a resting follower.
fn touching_graph(f: &Fixture, root: EntityId, followers: &[EntityId]) -> Vec<EntityId> {
    let mut reached = vec![root];
    let mut frontier = vec![root];
    while let Some(node) = frontier.pop() {
        for &id in followers {
            if !reached.contains(&id) && f.gap(id, node) <= TOUCH_MARGIN_RAW {
                reached.push(id);
                frontier.push(id);
            }
        }
    }
    reached
}

/// The 12 points of a ring of `radius` cells round `centre`, by whole-cell offsets.
fn ring(centre: (i32, i32), radius: i32) -> Vec<SimVec2> {
    let r = radius;
    let h = radius * 7 / 10;
    let q = radius / 2;
    let s = radius * 87 / 100;
    [
        (r, 0),
        (s, q),
        (h, h),
        (q, s),
        (0, r),
        (-q, s),
        (-h, h),
        (-s, q),
        (-r, 0),
        (-s, -q),
        (0, -r),
        (q, -s),
    ]
    .into_iter()
    .map(|(dx, dy)| at(centre.0 + dx, centre.1 + dy))
    .collect()
}

/// Asserts P-6's settled state: every follower rests with its order, the touching graph reaches
/// all of them, and the target stands within D-α's resting bound of `home`.
fn assert_bunched(f: &Fixture, target: EntityId, followers: &[EntityId], home: SimVec2) {
    for &id in followers {
        assert!(f.rests(id), "{id:?} rests");
        assert_eq!(f.front(id), Some(follow_of(target, FollowKind::Move)));
    }
    let reached = touching_graph(f, target, followers);
    assert_eq!(
        reached.len(),
        followers.len() + 1,
        "the rest graph is connected"
    );
    let sum = f.radius(target) + f.radius(followers[0]);
    let moved = checked_distance(f.world.entities[&target].position, home)
        .map_or(i128::MAX, |d| i128::from(d.raw()));
    assert!(
        moved <= resting_bound_raw(&f.world, sum),
        "the target was pushed {moved} raw units"
    );
}

/// P-6: twelve followers started on a ring of radius 10 round a standing target all rest within
/// the walk plus the stall limit, connected to the target through touching rests, none giving its
/// order up and none pushing the target beyond D-α's resting bound; after the target walks 20 and
/// stops, they bunch round it again.
#[test]
fn twelve_followers_bunch_round_their_target_and_again_after_it_moves() -> TestResult {
    let mut f = fixture()?;
    let target = f.spawn(f.kinds.soldier, ME, at(24, 14))?;
    let mut followers = Vec::new();
    for point in ring((24, 14), 10) {
        followers.push(f.spawn(f.kinds.soldier, ME, point)?);
    }
    f.follow(followers.clone(), target, FollowKind::Move, false);
    let walk = u32::try_from((10_i128 << 32) / f.speed(target).max(1) + 1)?;
    let budget = walk + STALL_ABANDON_TICKS;
    let all = |f: &Fixture| followers.iter().all(|&id| f.rests(id));
    let home = f.world.entities[&target].position;
    f.idle_until(budget, |f| all(f))
        .ok_or("not every follower rested")?;
    f.idle(25);
    assert_bunched(&f, target, &followers, home);

    f.walk(target, at(24, 34), false);
    f.idle_until(400, |f| f.world.entities[&target].order_queue.is_empty())
        .ok_or("the target never arrived")?;
    let home = f.world.entities[&target].position;
    f.idle_until(budget, |f| all(f))
        .ok_or("not every follower rested again")?;
    f.idle(25);
    assert_bunched(&f, target, &followers, home);
    Ok(())
}

/// P-8: a resting follower stands as an idle body and a walking one does not; a target walking
/// back through two resting followers gets through as through idle units, in at most the ticks
/// of the same walk unobstructed plus `STALL_REPATH_TICKS`.
#[test]
fn resting_followers_yield_to_their_target_walking_back_through_them() -> TestResult {
    let goal = at(6, 24);
    let mut alone = fixture()?;
    let solo = alone.spawn(alone.kinds.soldier, ME, at(24, 24))?;
    alone.walk(solo, goal, false);
    let free = alone
        .idle_until(600, |f| f.world.entities[&solo].order_queue.is_empty())
        .ok_or("the unobstructed walk never ended")?;

    let mut f = fixture()?;
    let target = f.spawn(f.kinds.soldier, ME, at(24, 24))?;
    let first = f.spawn(f.kinds.soldier, ME, at(18, 24))?;
    let second = f.spawn(f.kinds.soldier, ME, at(14, 24))?;
    f.follow(vec![first, second], target, FollowKind::Move, false);
    assert!(
        !f.world.entities[&first].stands_idle(),
        "a walking follower is no idle body"
    );
    f.idle_until(400, |f| f.rests(first) && f.rests(second))
        .ok_or("the followers never rested")?;
    assert!(f.world.entities[&first].stands_idle());
    assert!(f.world.entities[&second].stands_idle());
    assert!(
        !f.world.entities[&first].is_idle(),
        "acquisition still sees an order"
    );

    let start = f.world.entities[&target].position;
    f.walk(target, goal, false);
    let taken = f
        .idle_until(600, |f| f.world.entities[&target].order_queue.is_empty())
        .ok_or("the target never got through")?;
    assert!(
        taken <= free + STALL_REPATH_TICKS,
        "through its followers in {taken} ticks, {free} alone"
    );
    let arrived = f.world.entities[&target].position;
    assert_ne!(arrived, start);
    Ok(())
}
