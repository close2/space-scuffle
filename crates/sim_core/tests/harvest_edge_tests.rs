//! Harvesting from the footprint's edge, and a mined-out field leaving the map (plan
//! `2026-11-20-resource-footprints` WP6, D-7 and D-8).
//!
//! # Responsibility
//! Proves that a worker gathers when its body touches the field and delivers when it touches the
//! town hall (contact: at most its collision radius from the shape, at least its terrain radius,
//! at a chamfered corner too); that a back field of the showcase's two-row line is mined through
//! its one-cell notch; that the showcase's income lies in the band plan
//! `2026-11-22-harvest-timing` §6.2 derives from `StarCraft` II's measured rates; that two and three
//! workers a field earn the saturation curve plan `2026-11-23-harvest-occupancy` §6 derives from
//! the one-worker take periods (one gatherer at a time, B-61); that a field leaves the map on the take that empties it (its cells
//! walkable, the mesh rebuilt, a unit able to walk through) while a geyser stays; and that a field
//! that has left applies no ring.
//!
//! # How it fits
//! Worlds come from `sim_fixtures` with the shipped catalog; the worker and the hall are found by
//! capability. Behaviour is driven by `PlayerCommand`s only; setup may place entities.
//!
//! # Invariants
//! Positions are fixed-point world units; distances to shapes are measured with the map's own
//! exact predicates, never with floats.

#![allow(
    clippy::expect_used,
    reason = "a broken harvest law must fail the test loudly"
)]

use std::error::Error;

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell, MapFile, PlacedFootprint, ResourceNode};
use sim_types::{
    EntityId, PlacementFlag, PlayerCommand, PlayerId, PlayerSlot, ResourceKind, ResourceNodeId,
    SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);
const SHOWCASE: &str = include_str!("../../../assets/maps/showcase_canyon.scmap.ron");

fn node(kind: ResourceKind, cell: (u32, u32), remaining: u32) -> ResourceNode {
    let centre = |v: u32| SimScalar::from_num(v) + SimScalar::HALF;
    ResourceNode {
        pos: SimVec2::new(centre(cell.0), centre(cell.1)),
        kind,
        remaining,
        cell,
        placed: None,
    }
}

fn flat(nodes: Vec<ResourceNode>) -> GameMap {
    GameMap::from_cells(
        40,
        40,
        vec![MapCell::default_ground(CliffLevel(0)); 1600],
        nodes,
        Vec::new(),
    )
    .expect("invariant: 40 × 40 cells")
}

/// The worker archetype (a harvester) and the town hall analogue (a depot with no prerequisite).
fn kinds(world: &SimWorld) -> Result<(UnitTypeId, UnitTypeId), Box<dyn Error>> {
    let worker = world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.harvester.is_some())
        .ok_or("a harvester")?
        .unit_type_id;
    let hall = world
        .catalog
        .archetypes()
        .find(|a| a.is_resource_dropoff && a.requires_structures.is_empty())
        .ok_or("a depot")?
        .unit_type_id;
    Ok((worker, hall))
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
    assert_eq!(world.diagnostics(), []);
}

fn harvest(world: &mut SimWorld, workers: Vec<EntityId>, target: ResourceNodeId) {
    step(
        world,
        vec![PlayerCommand {
            player_id: ME,
            entities: workers,
            order: UnitOrder::Harvest { target },
            queue: false,
        }],
    );
}

/// Whether the body of `radius` centred at `at` comes no nearer `shape` than `radius`: touching
/// it at most at `radius`, as the map's capsule predicate decides on the Q16 grid.
fn within(shape: &PlacedFootprint, at: SimVec2, radius: SimScalar) -> bool {
    shape.capsule_meets(at, at, radius + SimScalar::from_raw(1 << 16))
}

/// The worker's collision and terrain radii.
fn radii(world: &SimWorld, worker: UnitTypeId) -> Result<(SimScalar, SimScalar), Box<dyn Error>> {
    let arch = world.catalog.get(worker)?;
    Ok((arch.collision_radius, arch.terrain_radius))
}

/// Steps until `done`, at most `limit` ticks; returns whether it happened.
fn until(world: &mut SimWorld, limit: u32, done: impl Fn(&SimWorld) -> bool) -> bool {
    for _ in 0..limit {
        if done(world) {
            return true;
        }
        step(world, Vec::new());
    }
    done(world)
}

#[test]
fn a_worker_gathers_touching_the_field() -> TestResult {
    let mut world =
        sim_fixtures::try_world(flat(vec![node(ResourceKind::Mineral, (14, 10), 1_500)]), 3)?;
    let (worker_type, hall_type) = kinds(&world)?;
    world.spawn_structure(hall_type, ME, 2, 2)?;
    let worker = world.spawn_unit(
        worker_type,
        ME,
        SimVec2::new(SimScalar::from_num(9), SimScalar::from_num(6)),
    )?;
    let (collision, terrain) = radii(&world, worker_type)?;
    let field = world.map.resources()[0]
        .placed
        .ok_or("the field stands")?
        .footprint;
    harvest(&mut world, vec![worker], ResourceNodeId(0));
    assert!(
        until(&mut world, 400, |w| w.entities[&worker].task.gather_ticks
            > 0),
        "the worker starts gathering"
    );
    let at = world.entities[&worker].position;
    assert!(
        within(&field, at, collision),
        "it touches the field: {at:?}"
    );
    assert!(
        !field.capsule_meets(at, at, terrain),
        "it stands no nearer than its terrain radius: {at:?}"
    );
    Ok(())
}

#[test]
fn a_worker_delivers_touching_the_hall() -> TestResult {
    // A field north-east of the hall, so the worker comes home to the hall's cut corner.
    let mut world =
        sim_fixtures::try_world(flat(vec![node(ResourceKind::Mineral, (12, 12), 1_500)]), 3)?;
    let (worker_type, hall_type) = kinds(&world)?;
    let hall = world.spawn_structure(hall_type, ME, 2, 2)?;
    let worker = world.spawn_unit(
        worker_type,
        ME,
        SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
    )?;
    let (collision, terrain) = radii(&world, worker_type)?;
    let hall_shape = world.map.structures()[&hall].footprint();
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    harvest(&mut world, vec![worker], ResourceNodeId(0));
    let mut delivered_at = None;
    for _ in 0..800 {
        let before = world.player_resources[PlayerSlot::ALL[0]].minerals;
        step(&mut world, Vec::new());
        if world.player_resources[PlayerSlot::ALL[0]].minerals > before {
            delivered_at = Some(world.entities[&worker].position);
            break;
        }
    }
    let at = delivered_at.ok_or("a delivery")?;
    assert!(
        within(&hall_shape, at, collision),
        "it touches the hall: {at:?}"
    );
    assert!(!hall_shape.capsule_meets(at, at, terrain), "{at:?}");
    // The hall's shape is cut at its corners; the worker came to the north-east one.
    let corner = SimVec2::new(SimScalar::from_num(7), SimScalar::from_num(7));
    assert!(
        at.x > SimScalar::from_num(6) && at.y > SimScalar::from_num(6),
        "{at:?} by {corner:?}"
    );
    Ok(())
}

/// A world on the showcase with slot 0's town hall at its start and `count` workers beside it.
fn showcase_start(count: usize) -> Result<(SimWorld, Vec<EntityId>), Box<dyn Error>> {
    let doc = MapFile::from_ron_str(SHOWCASE)?.decode()?;
    let start = doc
        .start_locations
        .iter()
        .find(|s| s.player_slot == 0)
        .ok_or("slot 0")?
        .clone();
    let mut world = sim_fixtures::try_world(doc.to_game_map()?, 7)?;
    let (worker_type, hall_type) = kinds(&world)?;
    world.spawn_structure(hall_type, ME, start.cell.0, start.cell.1)?;
    let workers = (0..count)
        .map(|i| {
            let x = SimScalar::from_num(start.cell.0 + 6)
                + SimScalar::from_raw(i64::try_from(i).unwrap_or(0) << 30);
            world.spawn_unit(
                worker_type,
                ME,
                SimVec2::new(x, SimScalar::from_num(start.cell.1 + 6)),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    Ok((world, workers))
}

/// The showcase's slot-0 mineral fields, in node order.
fn slot0_fields(world: &SimWorld) -> Vec<ResourceNodeId> {
    (0_u32..)
        .zip(world.map.resources())
        .filter(|(_, n)| n.kind == ResourceKind::Mineral && n.cell.0 < 32)
        .map(|(i, _)| ResourceNodeId(i))
        .collect()
}

#[test]
fn a_back_field_is_mined_through_its_notch() -> TestResult {
    let (mut world, workers) = showcase_start(1)?;
    // The back field at (4, 15) sits behind the front fields at (3, 14) and (6, 14): its only
    // open face from the base is the one-cell notch at x 5.
    let back = (0_u32..)
        .zip(world.map.resources())
        .find(|(_, n)| n.cell == (4, 15))
        .map(|(i, _)| ResourceNodeId(i))
        .ok_or("the back field at (4, 15)")?;
    harvest(&mut world, workers.clone(), back);
    assert!(
        until(&mut world, 1_500, |w| w.player_resources
            [PlayerSlot::ALL[0]]
            .minerals
            > 0),
        "a delivery from the back field"
    );
    let field = world.map.resources()[back.0 as usize]
        .placed
        .ok_or("it stands")?
        .footprint;
    harvest(&mut world, workers.clone(), back);
    assert!(until(&mut world, 400, |w| w.entities[&workers[0]]
        .task
        .gather_ticks
        > 0));
    let at = world.entities[&workers[0]].position;
    // It gathers from below, in the notch: x within the notch cell, y below the field.
    assert!(
        at.x > SimScalar::from_num(5)
            && at.x < SimScalar::from_num(6)
            && at.y < SimScalar::from_num(15),
        "{at:?}"
    );
    let (collision, _) = radii(&world, world.entities[&workers[0]].unit_type_id)?;
    assert!(within(&field, at, collision));
    Ok(())
}

/// The band of plan `2026-11-22-harvest-timing` §6.2 (reference test), now that `ReturnDelay` is
/// modelled: on the showcase's slot-0 start, one worker per field in turn, the trip-period rate
/// (the mineral row's amount per mean delivery-to-delivery period of ticks 750 to 3,750, per
/// minute): the mean of the eight fields in [54.9, 60.7], every field in [50.9, 64.3].
/// `StarCraft` II's measured rates (Liquipedia: 61.22 near, 53.57 far, 57.8 a base) ±5 %. The
/// window count is not used: at 2.5 a minute per delivery its quantization alone can cross an
/// edge (§6.3, A-3).
#[test]
fn income_lies_in_the_band() -> TestResult {
    let mut rates = Vec::new();
    let fields = slot0_fields(&showcase_start(1)?.0);
    assert_eq!(fields.len(), 8);
    for field in fields {
        let (mut world, workers) = showcase_start(1)?;
        let amount = world
            .catalog
            .placement_rules()
            .harvest(ResourceKind::Mineral)
            .ok_or("the mineral row's harvest")?
            .amount;
        let worker = workers[0];
        harvest(&mut world, workers, field);
        for _ in 0..750 {
            step(&mut world, Vec::new());
        }
        let mut held = world.entities[&worker].cargo.is_some();
        let mut deliveries = Vec::new();
        for tick in 0_u64..3_000 {
            step(&mut world, Vec::new());
            let carrying = world.entities[&worker].cargo.is_some();
            if held && !carrying {
                deliveries.push(tick);
            }
            held = carrying;
        }
        let (Some(first), Some(last)) = (deliveries.first(), deliveries.last()) else {
            return Err(format!("field {field:?}: no delivery").into());
        };
        let trips = u64::try_from(deliveries.len() - 1)?;
        assert!(trips > 0 && last > first, "field {field:?}: {deliveries:?}");
        // Units per minute (1,500 ticks) in hundredths: amount × 1,500 × 100 × trips / span.
        let earned = u64::from(amount) * 150_000 * trips;
        let span = last - first;
        assert!(
            5_090 * span <= earned && earned <= 6_430 * span,
            "field {field:?}: {earned} / {span} hundredths of a mineral per minute"
        );
        rates.push(earned / span);
    }
    let mean_x8: u64 = rates.iter().sum();
    assert!(
        (5_490 * 8..=6_070 * 8).contains(&mean_x8),
        "mean {} hundredths per minute",
        mean_x8 / 8
    );
    Ok(())
}

#[test]
fn a_mined_out_field_leaves_the_map() -> TestResult {
    let mut world = sim_fixtures::try_world(
        flat(vec![
            node(ResourceKind::Mineral, (12, 4), 5),
            node(ResourceKind::Mineral, (12, 8), 1_500),
        ]),
        3,
    )?;
    let (worker_type, hall_type) = kinds(&world)?;
    world.spawn_structure(hall_type, ME, 2, 2)?;
    let worker = world.spawn_unit(
        worker_type,
        ME,
        SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(4)),
    )?;
    let version = world.pathfinder.nav_version();
    harvest(&mut world, vec![worker], ResourceNodeId(0));
    assert!(until(&mut world, 600, |w| w.map.resources()[0].remaining == 0));
    // On the take that emptied it, the field left the map.
    let field = &world.map.resources()[0];
    assert!(field.placed.is_none(), "the field left the map");
    for x in 12..14 {
        let cell = world.map.cell(x, 4)?;
        assert!(cell.occupying_resource.is_none());
        assert!(cell.flags.contains(sim_map::CellFlags::GROUND_PATHABLE));
    }
    assert_ne!(
        world.pathfinder.nav_version(),
        version,
        "the mesh was rebuilt"
    );
    // A walker now crosses where the field stood.
    let walker = world.spawn_unit(
        worker_type,
        ME,
        SimVec2::new(
            SimScalar::from_num(13),
            SimScalar::HALF * SimScalar::from_num(5),
        ),
    )?;
    let across = SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(6));
    assert!(world.is_position_pathable_from(world.entities[&walker].position, across));
    // The other field stands.
    assert!(world.map.resources()[1].placed.is_some());
    Ok(())
}

#[test]
fn a_geyser_stays_when_empty() -> TestResult {
    let mut world = sim_fixtures::try_world(flat(vec![node(ResourceKind::Gas, (14, 10), 5)]), 3)?;
    let (worker_type, hall_type) = kinds(&world)?;
    world.spawn_structure(hall_type, ME, 2, 2)?;
    let extractor = world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.is_gas_extractor)
        .ok_or("an extractor")?
        .unit_type_id;
    world.spawn_structure(extractor, ME, 13, 9)?;
    let worker = world.spawn_unit(
        worker_type,
        ME,
        SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(6)),
    )?;
    harvest(&mut world, vec![worker], ResourceNodeId(0));
    assert!(until(&mut world, 800, |w| w.map.resources()[0].remaining == 0));
    assert!(
        world.map.resources()[0].placed.is_some(),
        "the geyser stays"
    );
    assert_eq!(
        world.map.cell(14, 10)?.occupying_resource,
        Some(ResourceNodeId(0))
    );
    Ok(())
}

#[test]
fn an_emptied_field_no_longer_applies_its_ring() -> TestResult {
    let mut world =
        sim_fixtures::try_world(flat(vec![node(ResourceKind::Mineral, (20, 20), 5)]), 3)?;
    let (worker_type, hall_type) = kinds(&world)?;
    assert!(world
        .catalog
        .get(hall_type)?
        .placement_excludes
        .has(PlacementFlag::NearResources));
    world.spawn_structure(hall_type, ME, 2, 2)?;
    // A site two cells east of the field's east cell lies in its ring.
    let site = (24, 18);
    assert_eq!(
        world.preview_site(ME, hall_type, site.0, site.1),
        Err(sim_types::SiteRefusal::NearResources)
    );
    let worker = world.spawn_unit(
        worker_type,
        ME,
        SimVec2::new(SimScalar::from_num(18), SimScalar::from_num(19)),
    )?;
    harvest(&mut world, vec![worker], ResourceNodeId(0));
    assert!(until(&mut world, 1_000, |w| w.map.resources()[0].remaining == 0));
    assert_eq!(world.preview_site(ME, hall_type, site.0, site.1), Ok(()));
    Ok(())
}

/// One node's takes in a window: the window tick of each and the units it took.
type Takes = Vec<(u64, u32)>;

/// Spawns `k` workers per slot-0 field of the showcase at the hall's edge facing that field and
/// orders each group to its field on one tick; steps the warm-up (1,500 ticks), then the window
/// (3,000 ticks), and returns per field the window ticks of its takes and the amount of each.
fn line_takes(k: usize) -> Result<Vec<Takes>, Box<dyn Error>> {
    let (mut world, _) = showcase_start(0)?;
    let doc = MapFile::from_ron_str(SHOWCASE)?.decode()?;
    let start = doc
        .start_locations
        .iter()
        .find(|s| s.player_slot == 0)
        .ok_or("slot 0")?
        .cell;
    let (worker_type, _) = kinds(&world)?;
    let fields = slot0_fields(&world);
    let half = SimScalar::from_num(5) / SimScalar::from_num(2);
    let hall = SimVec2::new(
        SimScalar::from_num(start.0) + half,
        SimScalar::from_num(start.1) + half,
    );
    let mut commands = Vec::new();
    for (lane, &field) in (0_u32..).zip(&fields) {
        let direction = (world.map.resources()[field.0 as usize].pos - hall).normalize_or_zero();
        // Lanes 3.2, 3.6 and 4.0 out from the hall's centre, workers 0.3 apart across a lane.
        let edge =
            hall + direction * (SimScalar::from_num(16 + 2 * (lane % 3)) / SimScalar::from_num(5));
        let across = SimVec2::new(-direction.y, direction.x);
        let group = (0_u32..u32::try_from(k)?)
            .map(|i| {
                let offset = SimScalar::from_num(3 * i) / SimScalar::from_num(10);
                world.spawn_unit(worker_type, ME, edge + across * offset)
            })
            .collect::<Result<Vec<_>, _>>()?;
        commands.push(PlayerCommand {
            player_id: ME,
            entities: group,
            order: UnitOrder::Harvest { target: field },
            queue: false,
        });
    }
    step(&mut world, commands);
    for _ in 0..1_500 {
        step(&mut world, Vec::new());
    }
    let remaining = |world: &SimWorld| -> Vec<u32> {
        fields
            .iter()
            .map(|f| world.map.resources()[f.0 as usize].remaining)
            .collect()
    };
    let mut last = remaining(&world);
    let mut takes = vec![Vec::new(); fields.len()];
    for tick in 0_u64..3_000 {
        step(&mut world, Vec::new());
        let now = remaining(&world);
        for ((field, before), after) in takes.iter_mut().zip(&last).zip(&now) {
            if after < before {
                field.push((tick, before - after));
            }
        }
        last = now;
    }
    Ok(takes)
}

/// A field's take-period rate in hundredths of a unit a minute: the units taken after its first
/// take in the window, times 1,500 ticks a minute, over the span from its first take to its last.
/// Returns the numerator and the span, so a comparison can cross-multiply exactly.
fn take_period(takes: &[(u64, u32)]) -> Result<(u64, u64), Box<dyn Error>> {
    let (Some(first), Some(last)) = (takes.first(), takes.last()) else {
        return Err("no take in the window".into());
    };
    let units: u64 = takes
        .iter()
        .skip(1)
        .map(|&(_, amount)| u64::from(amount))
        .sum();
    let span = last.0 - first.0;
    if span == 0 {
        return Err("one take in the window".into());
    }
    Ok((units * 150_000, span))
}

/// Plan `2026-11-23-harvest-occupancy` §6 (A-1, A-5), re-derived by plan
/// `2026-11-23-harvest-handoff` §6.1: with `k` workers on every showcase slot-0 field, each
/// field's take-period rate is `R_f(k) = min(k × amount × 1,500 / T_f, amount × 1,500 / (H +
/// h))`, where `T_f` is the field's one-worker take period measured here at `k` = 1, `H` the
/// mineral row's `harvest_time_ticks` and `h` its `handoff_ticks`: a closed cycle of `k` workers
/// through one server whose period is the gather plus the hand-off to the next worker. Within one
/// load a field over the window (±2.5 a minute, ±20 for the line); a saturated field is exact (it
/// is taken every `H + h` ticks). The near geyser (10, 1) with three workers is taken every gas
/// `harvest_time_ticks + handoff_ticks`: `amount × 1,500 / (H + h)` exactly.
#[test]
fn saturation_follows_the_derivation() -> TestResult {
    let probe = showcase_start(0)?.0;
    let rules = probe.catalog.placement_rules();
    let mineral = *rules
        .harvest(ResourceKind::Mineral)
        .ok_or("the mineral row")?;
    let gas = *rules.harvest(ResourceKind::Gas).ok_or("the gas row")?;
    let (amount, time) = (
        u64::from(mineral.amount),
        u64::from(mineral.time_ticks) + u64::from(mineral.handoff_ticks),
    );
    // Hundredths a minute: amount × 150,000 / (H + h), the field's cap.
    let cap = (amount * 150_000, time);
    let one = line_takes(1)?;
    for k in [2_u64, 3] {
        let line = line_takes(usize::try_from(k)?)?;
        let (mut measured_sum, mut derived_sum) = (0_u64, 0_u64);
        for (field, (single, takes)) in one.iter().zip(&line).enumerate() {
            let (single_units, single_span) = take_period(single)?;
            // k workers' walk allows k × the one-worker rate; the server allows the cap.
            let walk = (k * single_units, single_span);
            let saturated = cap.0 * walk.1 <= walk.0 * cap.1;
            let derived = if saturated { cap } else { walk };
            let measured = take_period(takes)?;
            if saturated {
                assert_eq!(
                    measured.0 * derived.1,
                    derived.0 * measured.1,
                    "k = {k}, field {field}: saturated, taken every {time} ticks ({takes:?})"
                );
            }
            let (m, d) = (measured.0 / measured.1, derived.0 / derived.1);
            assert!(
                m.abs_diff(d) <= 250,
                "k = {k}, field {field}: {m} against {d} hundredths"
            );
            measured_sum += m;
            derived_sum += d;
        }
        assert!(
            measured_sum.abs_diff(derived_sum) <= 2_000,
            "k = {k}: line {measured_sum} against {derived_sum} hundredths"
        );
        if k == 3 {
            assert_eq!(
                derived_sum,
                8 * cap.0 / cap.1,
                "every showcase field saturates at 3"
            );
        }
    }

    let (mut world, workers) = showcase_start(3)?;
    let extractor = world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.is_gas_extractor)
        .ok_or("an extractor")?
        .unit_type_id;
    let geyser = (0_u32..)
        .zip(world.map.resources())
        .find(|(_, n)| n.kind == ResourceKind::Gas && n.cell == (10, 1))
        .map(|(i, _)| ResourceNodeId(i))
        .ok_or("the near geyser")?;
    world.spawn_structure(extractor, ME, 9, 0)?;
    harvest(&mut world, workers, geyser);
    for _ in 0..750 {
        step(&mut world, Vec::new());
    }
    let mut last = world.map.resources()[geyser.0 as usize].remaining;
    let mut takes = Vec::new();
    for tick in 0_u64..3_000 {
        step(&mut world, Vec::new());
        let now = world.map.resources()[geyser.0 as usize].remaining;
        if now < last {
            takes.push((tick, last - now));
        }
        last = now;
    }
    let (units, span) = take_period(&takes)?;
    assert_eq!(
        units * (u64::from(gas.time_ticks) + u64::from(gas.handoff_ticks)),
        u64::from(gas.amount) * 150_000 * span,
        "the extractor is taken every gather and hand-off: {takes:?}"
    );
    Ok(())
}
