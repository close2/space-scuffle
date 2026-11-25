//! The follow order (plan `2026-11-26-follow-order`; B-163, B-166's remaining half, B-187).
//!
//! # Responsibility
//! Checks `UnitOrder::Follow` end to end through `PlayerCommand`s: its admission and typed
//! refusals (P-2), and the takers it filters, against constructed worlds on open ground.
//!
//! # Invariants
//! Archetypes are chosen by capability (mobility, the A command of the default stance, transport
//! capacity, the structure shape), never by identity. Radii, speeds and margins are read from the
//! catalog, never from a seed's outcome.

use sim_core::movement::{MovementAdmissionError, MovementReceipt};
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackCommand, EntityId, FollowKind, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar,
    SimVec2, StanceOrders, Tick, TickBundle, UnitOrder, UnitTypeId,
};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);
const THEM: PlayerId = PlayerId(1);
const SIDE: u32 = 48;

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
    /// The soldier's archetype copied with no A command, added to the catalog by the fixture.
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
    let structure = find(&world, |a| {
        matches!(a.shape, ShapeKind::BuildingDome { .. })
    })
    .ok_or("no structure")?;
    let unarmed_no_a = with_no_a_command(&mut world, soldier)?;
    Ok(Fixture {
        world,
        kinds: Kinds {
            soldier,
            scanner,
            carrier,
            structure,
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
