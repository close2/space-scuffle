//! # Integration Test: An Automatic Chase Leaves a Full Ring (R10, G-6′, deviation D-ε)
//!
//! Plan `2026-11-20-contact-solver` Amendment 17, §17.3. Under the solver pipeline an attacker whose
//! target's ring has no free angle stands (G-6). G-6′: when that chase is automatic (an attack
//! acquired on its own, never a player's `Attack`), the unit first re-runs acquisition for itself
//! and takes, in the normal acquisition order, the first other enemy whose ring has room. These
//! tests pin, through public APIs:
//! - a block of two touching columns, approached by twice as many attackers as the near column's
//!   rings hold, is attacked on its far column too, and no attacker stands at a full near ring
//!   while a far ring within its scan range has a free angle (mirrored and relabelled alike);
//! - a player's `Attack` on a full ring still stands and keeps its order (a guard);
//! - a retarget never reaches beyond the chase's leash, and keeps where the leash is measured
//!   from;
//! - acquisition keeps the target G-6′ chose until a candidate outranks it;
//! - with `contact.retarget_from_full_ring` off the simulation is the plan head's, bit for bit.
//!
//! The scene's geometry is derived from the radii and the harness's ring capacity
//! `C = ⌊π / asin(r / (R + r + range))⌋` (plan §4), not from any catalog unit; the archetypes are
//! scratch archetypes (C5). Positions are converted to `f64` for measurement only.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::panic
)]

use sim_core::catalog::{ContactRules, MovementPipeline, MovementRules};
use sim_core::{SimWorld, UnitCatalog};
use sim_fixtures::harness::corpus::{Lattice, ScratchArchetype, LATTICE};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The attackers' radius, sixty-fourths (0.375).
const ATTACKER_RADIUS: Lattice = 24;
/// The block bodies' radius, sixty-fourths (0.75).
const BODY_RADIUS: Lattice = 48;
/// The attackers' weapon range, world units: melee.
const RANGE: f64 = 0.1;
/// The attackers' top speed, world units per tick.
const SPEED: f64 = 0.125;
/// The scratch archetypes' scan range, world units, edge to edge (`ScratchArchetype::ron`).
const SCAN_RANGE: f64 = 5.5;
/// The block's centre, world units.
const CENTRE: (f64, f64) = (32.0, 32.0);

const ATTACKER: u16 = 1;
const BODY: u16 = 2;
const THREAT: u16 = 3;

fn radius_of(lattice: Lattice) -> f64 {
    lattice as f64 / LATTICE as f64
}

/// The scene's derived sizes, world units.
struct Geometry {
    /// Attacker radius `r`.
    r: f64,
    /// Block body radius `R`.
    big: f64,
    /// The band's outer edge against a block body: `R + r + range`, centre to centre.
    band: f64,
    /// The ring the rule reads: `band − r / 2`.
    rho: f64,
    /// The ring capacity `C` of one block body.
    capacity: usize,
}

fn geometry() -> Geometry {
    let r = radius_of(ATTACKER_RADIUS);
    let big = radius_of(BODY_RADIUS);
    let band = big + r + RANGE;
    let capacity = (std::f64::consts::PI / (r / band).asin()).floor() as usize;
    Geometry {
        r,
        big,
        band,
        rho: band - r / 2.0,
        capacity,
    }
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn xy(v: SimVec2) -> (f64, f64) {
    (f64::from(v.x.to_f32()), f64::from(v.y.to_f32()))
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The scratch catalog: the melee attacker, the unarmed block body and an armed threat.
fn scratch_catalog(rules: MovementRules) -> Result<UnitCatalog, Box<dyn std::error::Error>> {
    let factions = sim_fixtures::shipped_factions();
    let faction = factions
        .factions()
        .next()
        .map(|f| f.slug.clone())
        .ok_or("a faction")?;
    let attacker = ScratchArchetype {
        radius: ATTACKER_RADIUS,
        speed: "0.125",
        range: Some("0.1"),
        priority: 10,
    };
    let body = ScratchArchetype {
        radius: BODY_RADIUS,
        speed: "0.125",
        range: None,
        priority: 10,
    };
    let entries = [
        attacker.ron("ring_attacker", ATTACKER, &faction),
        body.ron("ring_body", BODY, &faction),
        attacker.ron("ring_threat", THREAT, &faction),
    ];
    let mut catalog =
        UnitCatalog::from_ron_str(&factions, &format!("(units: [{}])", entries.join(",")))?;
    catalog.set_movement_rules(rules);
    Ok(catalog)
}

/// The shipped movement rules on the solver pipeline, with the full-ring switch as shipped.
fn solver_rules() -> Result<MovementRules, Box<dyn std::error::Error>> {
    let rules = sim_fixtures::harness::shipped_rules();
    Ok(rules.with_contact(ContactRules {
        pipeline: MovementPipeline::Solver,
        ..rules.contact()
    })?)
}

/// One block scene: the world and its units.
struct Block {
    world: SimWorld,
    /// The near column (facing the attackers), north to south.
    near: Vec<EntityId>,
    /// The far column.
    far: Vec<EntityId>,
    attackers: Vec<EntityId>,
}

/// How the block is approached.
#[derive(Clone, Copy)]
struct Approach {
    /// From the east instead of the west.
    mirrored: bool,
    /// Attackers spawned in reverse order, so their ids run the other way.
    relabelled: bool,
}

/// x on the approach side: the west scene's `x`, reflected through the block's centre when
/// mirrored.
fn side_x(approach: Approach, x: f64) -> f64 {
    if approach.mirrored {
        2.0 * CENTRE.0 - x
    } else {
        x
    }
}

/// The block: two touching columns of three bodies (column spacing `2R`), on Hold Position.
fn spawn_block(
    world: &mut SimWorld,
    approach: Approach,
) -> Result<(Vec<EntityId>, Vec<EntityId>), Box<dyn std::error::Error>> {
    let g = geometry();
    let near_x = side_x(approach, CENTRE.0 - g.big);
    let far_x = side_x(approach, CENTRE.0 + g.big);
    let rows = [CENTRE.1 - 2.0 * g.big, CENTRE.1, CENTRE.1 + 2.0 * g.big];
    let mut near = Vec::new();
    let mut far = Vec::new();
    for &y in &rows {
        near.push(world.spawn_unit(sim_types::UnitTypeId(BODY), RED, at(near_x, y))?);
        far.push(world.spawn_unit(sim_types::UnitTypeId(BODY), RED, at(far_x, y))?);
    }
    Ok((near, far))
}

fn hold(units: &[EntityId], player: PlayerId) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities: units.to_vec(),
        order: UnitOrder::HoldPosition,
        queue: false,
    }
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

/// The block with `N = 2 · 3 · C` attackers ten units off on the approach side, attack-moving
/// through it; the first tick (orders) is stepped.
fn block_scene(
    rules: MovementRules,
    approach: Approach,
    seed: u64,
) -> Result<Block, Box<dyn std::error::Error>> {
    let g = geometry();
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::try_world(map, seed)?;
    world.catalog = scratch_catalog(rules)?;
    let (near, far) = spawn_block(&mut world, approach)?;
    let count = 2 * near.len() * g.capacity;
    let spacing = 2.0 * g.r + 0.05;
    let rows = 10;
    let mut places: Vec<(f64, f64)> = (0..count)
        .map(|index| {
            let (column, row) = (index / rows, index % rows);
            let x = CENTRE.0 - 10.0 - spacing * column as f64;
            let y = CENTRE.1 + spacing * (row as f64 - (rows as f64 - 1.0) / 2.0);
            (side_x(approach, x), y)
        })
        .collect();
    if approach.relabelled {
        places.reverse();
    }
    let mut attackers = Vec::with_capacity(count);
    for (x, y) in places {
        attackers.push(world.spawn_unit(sim_types::UnitTypeId(ATTACKER), BLUE, at(x, y))?);
    }
    let goal = at(side_x(approach, CENTRE.0 + 12.0), CENTRE.1);
    let mut blocks = near.clone();
    blocks.extend(&far);
    step(
        &mut world,
        vec![
            hold(&blocks, RED),
            PlayerCommand {
                player_id: BLUE,
                entities: attackers.clone(),
                order: UnitOrder::AttackMove { target: goal },
                queue: false,
            },
        ],
    );
    Ok(Block {
        world,
        near,
        far,
        attackers,
    })
}

/// Every living unit's centre and radius.
fn bodies(world: &SimWorld) -> Vec<(EntityId, (f64, f64), f64)> {
    world
        .entities
        .iter()
        .filter(|(_, e)| e.is_alive())
        .map(|(&id, e)| {
            let radius = world
                .catalog
                .get(e.unit_type_id)
                .map_or(0.0, |a| f64::from(a.collision_radius.to_f32()));
            (id, xy(e.position), radius)
        })
        .collect()
}

/// Whether the ring round `target` has a point an attacker's disc could stand on: a point of the
/// ring (radius `rho`) clear of every other body's disc by a margin, sampled every half degree.
/// Optionally only points within `leash` of `origin` count.
fn ring_has_room(
    snapshot: &[(EntityId, (f64, f64), f64)],
    target: EntityId,
    leash: Option<((f64, f64), f64)>,
) -> bool {
    let g = geometry();
    let Some(&(_, centre, _)) = snapshot.iter().find(|(id, _, _)| *id == target) else {
        return false;
    };
    (0..720).any(|step| {
        let angle = f64::from(step) * std::f64::consts::PI / 360.0;
        let point = (
            centre.0 + g.rho * angle.cos(),
            centre.1 + g.rho * angle.sin(),
        );
        let leashed = leash.is_none_or(|(origin, radius)| dist(origin, point) <= radius - 1e-3);
        leashed
            && snapshot
                .iter()
                .filter(|(id, _, _)| *id != target)
                .all(|&(_, pos, radius)| dist(pos, point) > g.r + radius + 1e-3)
    })
}

/// The target of `unit`'s front attack order and its origin, if it has one.
fn attack_order(world: &SimWorld, unit: EntityId) -> Option<(EntityId, AttackOrigin)> {
    match world.entities.get(&unit)?.order_queue.front() {
        Some(&UnitOrder::AttackTarget { target, origin }) => Some((target, origin)),
        _ => None,
    }
}

/// Whether `unit` is inside its band of `target`.
fn in_band(snapshot: &[(EntityId, (f64, f64), f64)], unit: EntityId, target: EntityId) -> bool {
    let find = |id| snapshot.iter().find(|(e, _, _)| *e == id).map(|b| b.1);
    match (find(unit), find(target)) {
        (Some(a), Some(b)) => dist(a, b) <= geometry().band + 1e-3,
        _ => false,
    }
}

/// The ticks of the check window: from the first tick every near ring is full (the moment R10 can
/// begin), twice the walk round the block (half its perimeter, `4R + 6R`, plus the ring radius)
/// at the attackers' speed.
fn round_trip_ticks() -> u32 {
    let g = geometry();
    (2.0 * (10.0 * g.big + g.rho) / SPEED).ceil() as u32
}

/// Plays `block` until every near ring is full, then the window; checks both assertions of
/// test 1 on every tick of the window.
fn assert_far_column_engaged(mut block: Block, label: &str) {
    let mut start = None;
    for _ in 0..600 {
        step(&mut block.world, vec![]);
        let snapshot = bodies(&block.world);
        if block
            .near
            .iter()
            .all(|&body| !ring_has_room(&snapshot, body, None))
        {
            start = Some(block.world.current_tick);
            break;
        }
    }
    let start = start.unwrap_or_else(|| panic!("{label}: the near rings never filled"));
    let mut before = bodies(&block.world);
    let mut far_contact = false;
    for _ in 0..round_trip_ticks() {
        step(&mut block.world, vec![]);
        let now = bodies(&block.world);
        for &unit in &block.attackers {
            let Some((target, origin)) = attack_order(&block.world, unit) else {
                continue;
            };
            if block.far.contains(&target) && in_band(&now, unit, target) {
                far_contact = true;
            }
            let entity = &block.world.entities[&unit];
            let stood = before
                .iter()
                .find(|(id, _, _)| *id == unit)
                .is_some_and(|b| b.1 == xy(entity.position));
            if origin != AttackOrigin::Acquired
                || !block.near.contains(&target)
                || in_band(&now, unit, target)
                || !stood
            {
                continue;
            }
            // It stood at a full near ring this tick. At the tick's start, was a far ring with
            // room within its scan range?
            let start_pos = before.iter().find(|(id, _, _)| *id == unit).map(|b| b.1);
            let open = block.far.iter().find(|&&body| {
                let Some(&(_, centre, radius)) = before.iter().find(|(id, _, _)| *id == body)
                else {
                    return false;
                };
                start_pos
                    .is_some_and(|p| dist(p, centre) - geometry().r - radius <= SCAN_RANGE - 1e-3)
                    && ring_has_room(&before, body, None)
            });
            if let Some(open) = open {
                panic!(
                    "{label}: tick {}: attacker {unit:?} stands at full ring {target:?} while \
                     far ring {open:?} in its scan range has room (window from tick {start})",
                    block.world.current_tick
                );
            }
        }
        before = now;
    }
    assert!(
        far_contact,
        "{label}: no attacker reached the far column within {} ticks of tick {start}",
        round_trip_ticks()
    );
}

/// Test 1 (§17.3): attackers that saturate the near column's rings are handed to the far
/// column's free rings, from either side and in either id order. Red under G-6 alone, where they
/// stand behind the near column for the whole fight.
#[test]
fn a_full_ring_hands_an_automatic_chaser_to_a_free_one() -> TestResult {
    let rules = solver_rules()?;
    for (mirrored, relabelled) in [(false, false), (true, false), (false, true)] {
        let approach = Approach {
            mirrored,
            relabelled,
        };
        let block = block_scene(rules, approach, 0x5EED_0A17)?;
        assert_far_column_engaged(
            block,
            &format!("mirrored {mirrored}, relabelled {relabelled}"),
        );
    }
    Ok(())
}

/// Test 2 (§17.3, a guard): attackers under a player's `Attack` on one near body stand at its
/// full ring and keep that order; none is handed to another body.
#[test]
fn an_explicit_attack_on_a_full_ring_stands() -> TestResult {
    let approach = Approach {
        mirrored: false,
        relabelled: false,
    };
    let mut block = block_scene(solver_rules()?, approach, 0x5EED_0A17)?;
    let target = block.near[1];
    step(
        &mut block.world,
        vec![PlayerCommand {
            player_id: BLUE,
            entities: block.attackers.clone(),
            order: UnitOrder::AttackTarget {
                target,
                origin: AttackOrigin::Explicit,
            },
            queue: false,
        }],
    );
    for _ in 0..(300 + round_trip_ticks()) {
        step(&mut block.world, vec![]);
        for &unit in &block.attackers {
            assert_eq!(
                attack_order(&block.world, unit),
                Some((target, AttackOrigin::Explicit)),
                "tick {}: attacker {unit:?} left its ordered target",
                block.world.current_tick
            );
        }
    }
    Ok(())
}

/// Fills the rings of `bodies` with friends of the attacker on Hold Position, greedily, every
/// degree, at the band, wherever a disc fits clear of every body and of `keep_clear`.
fn fill_rings(
    world: &mut SimWorld,
    bodies_to_fill: &[EntityId],
    keep_clear: (f64, f64),
) -> Result<Vec<EntityId>, Box<dyn std::error::Error>> {
    let g = geometry();
    let mut friends = Vec::new();
    for &body in bodies_to_fill {
        let centre = xy(world.entities[&body].position);
        for degree in 0..360 {
            let angle = f64::from(degree).to_radians();
            let radius = g.big + g.r + RANGE / 2.0;
            let point = (
                centre.0 + radius * angle.cos(),
                centre.1 + radius * angle.sin(),
            );
            let clear = bodies(world)
                .iter()
                .all(|&(_, pos, r)| dist(pos, point) >= g.r + r + 0.01)
                && dist(point, keep_clear) >= 2.0 * g.r + 0.5;
            if clear {
                friends.push(world.spawn_unit(
                    sim_types::UnitTypeId(ATTACKER),
                    BLUE,
                    at(point.0, point.1),
                )?);
            }
        }
    }
    Ok(friends)
}

/// Test 3 (§17.3): a retarget never reaches beyond the chase's leash. With a leash of 2, a lone
/// attacker 2.5 west of the middle near body, whose near rings friends on Hold Position fill,
/// has the far rings in scan range but every far ring point beyond its leash: it stands at its
/// full ring and keeps its target.
#[test]
fn a_retarget_never_lengthens_the_leash() -> TestResult {
    let rules = solver_rules()?.with_acquire_leash_radius(2)?;
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::try_world(map, 0x5EED_0A18)?;
    world.catalog = scratch_catalog(rules)?;
    let approach = Approach {
        mirrored: false,
        relabelled: false,
    };
    let (near, far) = spawn_block(&mut world, approach)?;
    let start = (CENTRE.0 - geometry().big - 2.5, CENTRE.1);
    let friends = fill_rings(&mut world, &near, start)?;
    let lone = world.spawn_unit(sim_types::UnitTypeId(ATTACKER), BLUE, at(start.0, start.1))?;
    let snapshot = bodies(&world);
    for &body in &near {
        assert!(
            !ring_has_room(&snapshot, body, None),
            "precondition: near ring {body:?} is full"
        );
    }
    for &body in &far {
        assert!(
            !ring_has_room(&snapshot, body, Some((start, 2.0))),
            "precondition: far ring {body:?} has no room within the leash"
        );
        assert!(
            ring_has_room(&snapshot, body, None),
            "precondition: far ring {body:?} has room beyond the leash"
        );
    }
    let mut blocks = near.clone();
    blocks.extend(&far);
    step(
        &mut world,
        vec![
            hold(&blocks, RED),
            hold(&friends, BLUE),
            PlayerCommand {
                player_id: BLUE,
                entities: vec![lone],
                order: UnitOrder::AttackMove {
                    target: at(CENTRE.0 + 12.0, CENTRE.1),
                },
                queue: false,
            },
        ],
    );
    for _ in 0..200 {
        step(&mut world, vec![]);
        if let Some((target, _)) = attack_order(&world, lone) {
            assert!(
                !far.contains(&target),
                "tick {}: the lone attacker took far body {target:?} beyond its leash",
                world.current_tick
            );
        }
    }
    assert!(
        attack_order(&world, lone)
            .is_some_and(|(t, o)| near.contains(&t) && o == AttackOrigin::Acquired),
        "the lone attacker keeps its automatic chase of a near body"
    );
    Ok(())
}

/// Test 3, second case (§17.3): an attacker handed from a near to a far body keeps the point its
/// leash is measured from.
#[test]
fn a_retarget_keeps_the_leash_origin() -> TestResult {
    let approach = Approach {
        mirrored: false,
        relabelled: false,
    };
    let mut block = block_scene(solver_rules()?, approach, 0x5EED_0A17)?;
    let mut handed = 0;
    for _ in 0..800 {
        let before: Vec<_> = block
            .attackers
            .iter()
            .map(|&unit| {
                (
                    attack_order(&block.world, unit),
                    block
                        .world
                        .entities
                        .get(&unit)
                        .map(|e| e.movement.leash_origin),
                )
            })
            .collect();
        step(&mut block.world, vec![]);
        for (index, &unit) in block.attackers.iter().enumerate() {
            let (Some((was, _)), Some(origin)) = before[index] else {
                continue;
            };
            let now = attack_order(&block.world, unit);
            if block.near.contains(&was) && now.is_some_and(|(t, _)| block.far.contains(&t)) {
                handed += 1;
                let kept = block.world.entities[&unit].movement.leash_origin;
                assert!(
                    origin.is_some(),
                    "the chase had begun: its leash origin is set"
                );
                assert_eq!(kept, origin, "attacker {unit:?} kept its leash origin");
            }
        }
    }
    assert!(handed > 0, "some attacker was handed to the far column");
    Ok(())
}

/// Test 4 (§17.3): acquisition keeps the target G-6′ chose (the nearer, full near body is not
/// taken back the next tick), while a candidate that outranks it (a threat) still takes over.
#[test]
fn a_retargeted_chase_keeps_its_target_until_outranked() -> TestResult {
    let approach = Approach {
        mirrored: false,
        relabelled: false,
    };
    let mut block = block_scene(solver_rules()?, approach, 0x5EED_0A17)?;
    let mut handed = None;
    for _ in 0..800 {
        let before: Vec<_> = block
            .attackers
            .iter()
            .map(|&unit| attack_order(&block.world, unit))
            .collect();
        step(&mut block.world, vec![]);
        handed = block
            .attackers
            .iter()
            .enumerate()
            .find_map(|(index, &unit)| {
                let was = before[index]?.0;
                let now = attack_order(&block.world, unit)?.0;
                (block.near.contains(&was) && block.far.contains(&now)).then_some((unit, now))
            });
        if handed.is_some() {
            break;
        }
    }
    let (unit, chosen) = handed.ok_or("some attacker was handed to the far column")?;
    for _ in 0..10 {
        step(&mut block.world, vec![]);
        let now = attack_order(&block.world, unit).map(|(t, _)| t);
        assert_eq!(
            now,
            Some(chosen),
            "tick {}: the chosen far body is kept",
            block.world.current_tick
        );
    }
    // A threat appears two units from the attacker, on the side away from the block.
    let pos = xy(block.world.entities[&unit].position);
    let away = side_x(approach, pos.0 - 2.0);
    let threat = block
        .world
        .spawn_unit(sim_types::UnitTypeId(THREAT), RED, at(away, pos.1))?;
    step(&mut block.world, vec![hold(&[threat], RED)]);
    step(&mut block.world, vec![]);
    assert_eq!(
        attack_order(&block.world, unit).map(|(t, _)| t),
        Some(threat),
        "a candidate of higher rank takes over"
    );
    Ok(())
}

/// FNV-1a over a list of per-tick checksums: one number for a whole run.
fn digest(checksums: &[u64]) -> u64 {
    checksums
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, &value| {
            value.to_le_bytes().iter().fold(hash, |h, &byte| {
                (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
            })
        })
}

/// The shipped rules on the solver pipeline with the full-ring switch off.
fn switched_off() -> Result<MovementRules, Box<dyn std::error::Error>> {
    let rules = solver_rules()?;
    Ok(rules.with_contact(ContactRules {
        retarget_from_full_ring: false,
        ..rules.contact()
    })?)
}

/// Whether any entity's movement memory holds a G-6′ choice.
fn any_retarget(world: &SimWorld) -> bool {
    world
        .entities
        .values()
        .any(|entity| entity.movement.ring_retarget.is_some())
}

/// The test-1 scene with the switch off, 400 ticks: the per-tick checksums' digest.
fn block_digest_switched_off() -> Result<u64, Box<dyn std::error::Error>> {
    let approach = Approach {
        mirrored: false,
        relabelled: false,
    };
    let mut block = block_scene(switched_off()?, approach, 0x5EED_0A17)?;
    let mut checksums = vec![block.world.compute_checksum()];
    for _ in 0..400 {
        step(&mut block.world, vec![]);
        assert!(
            !any_retarget(&block.world),
            "the switch is off: G-6′ writes nothing"
        );
        checksums.push(block.world.compute_checksum());
    }
    Ok(digest(&checksums))
}

/// Battle 28 of the matchup matrix (zerglings against immortals, the battle R10 was found in)
/// on its first seed of record with the switch off: the per-tick checksums' digest.
fn battle_digest_switched_off() -> Result<u64, Box<dyn std::error::Error>> {
    use sim_fixtures::matchup::{BattleRun, Matrix, MATRIX_RON};
    let matrix = Matrix::from_ron_str(MATRIX_RON)?;
    let battle = matrix.battle("28_100_lings_vs_10_immortals")?;
    let seed = *matrix.seeds.first().ok_or("a seed of record")?;
    let mut catalogs = sim_fixtures::shipped_catalogs();
    catalogs.units.set_movement_rules(switched_off()?);
    let mut run = BattleRun::new(battle, catalogs, seed, matrix.jitter_for(seed))?;
    let mut checksums = vec![run.world.compute_checksum()];
    while !run.is_finished() {
        assert!(run.step().is_empty(), "no diagnostic");
        assert!(
            !any_retarget(&run.world),
            "the switch is off: G-6′ writes nothing"
        );
        checksums.push(run.world.compute_checksum());
    }
    Ok(digest(&checksums))
}

/// Test 5 (§17.3): with `contact.retarget_from_full_ring` off the simulation is the plan head's
/// (`9fccdcab`, before G-6′), bit for bit, on the test-1 scene and on a matrix battle. G-6′ writes
/// no memory, and the per-tick checksums match digests pinned under schema 42. Proof that they are
/// the plan head's: a scratch build of this tree that leaves `ring_retarget` unhashed and holds
/// the schema at 40 gives exactly the digests the plan head gives for the same two runs (evidence
/// `docs/evidence/2026-11-20-contact-solver.md` §13.13).
#[test]
fn the_full_ring_switch_off_is_bit_identical() -> TestResult {
    let block = block_digest_switched_off()?;
    let battle = battle_digest_switched_off()?;
    if std::env::var_os("RING_RETARGET_PRINT_DIGESTS").is_some() {
        println!("block digest {block:#018x}, battle digest {battle:#018x}");
    }
    assert_eq!(
        block, BLOCK_DIGEST_SWITCHED_OFF,
        "the test-1 scene, switch off"
    );
    assert_eq!(battle, BATTLE_DIGEST_SWITCHED_OFF, "battle 28, switch off");
    Ok(())
}

/// The pinned digest of the test-1 scene with the switch off (schema 42).
const BLOCK_DIGEST_SWITCHED_OFF: u64 = 0xcc46_f75f_f253_c4fd;
/// The pinned digest of battle 28's first seed of record with the switch off (schema 42).
const BATTLE_DIGEST_SWITCHED_OFF: u64 = 0x819f_469e_e6db_e465;
