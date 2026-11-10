//! Group-consistent routing (plan `docs/plans/2026-11-10-group-routing.md`): route groups,
//! their order-ordinal binding, the movement rules file and the lane rule, through the public
//! command path.
//!
//! The tests build worlds from the shipped catalog files and set the group-routing tolerance in
//! code where they need a value other than the shipped one, so each test states the tolerance it
//! exercises.

use sim_core::catalog::{CatalogLoadError, MovementRules, RuntimeCatalogs};
use sim_core::movement::groups::{GroupLane, LaneState, RouteGroup, RouteGroupId, RouteGroupKind};
use sim_core::movement::RouteOutcomeKind;
use sim_core::scenario::{
    apply_initial_orders, build_world_with_catalogs, InitialOrder, ScenarioManifest,
    StartLoadoutCatalog,
};
use sim_core::{SimCoreError, SimWorld};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, MapFile};
use sim_types::{
    AttackOrigin, EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2,
    Tick, TickBundle, UnitOrder, UnitTypeId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const SEED: u64 = 0x6e_2026_1110;

fn catalog_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog")
}

fn center(x: i32, y: i32) -> SimVec2 {
    SimVec2::from_i32(x, y) + SimVec2::new(SimScalar::HALF, SimScalar::HALF)
}

fn flat() -> Result<GameMap, Box<dyn std::error::Error>> {
    Ok(GameMap::new(
        64,
        64,
        MapCell::default_ground(CliffLevel(0)),
    )?)
}

/// A world on `map` at tolerance `permille`, with an unarmed mobile archetype of `layer`.
fn world(
    map: GameMap,
    permille: u32,
    layer: MoverType,
) -> Result<(SimWorld, UnitTypeId), Box<dyn std::error::Error>> {
    let mut catalogs = RuntimeCatalogs::load_from_directory(catalog_directory())?;
    catalogs
        .units
        .set_movement_rules(MovementRules::with_lane_tolerance(permille)?);
    let kind = catalogs
        .units
        .archetypes()
        .find(|a| {
            a.mover_type == layer
                && !a.is_immobile_by_default()
                && !matches!(a.shape, ShapeKind::BuildingDome { .. })
        })
        .ok_or("catalog requires a mobile archetype")?
        .unit_type_id;
    let arch = catalogs.units.get_mut(kind)?;
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    let world = SimWorld::with_catalogs(map, SEED, catalogs.units, catalogs.upgrades);
    Ok((world, kind))
}

fn command(ids: &[EntityId], order: UnitOrder, queue: bool) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(0),
        entities: ids.to_vec(),
        order,
        queue,
    }
}

fn tick(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

fn spawn_row(
    world: &mut SimWorld,
    kind: UnitTypeId,
    cells: &[(i32, i32)],
) -> Result<Vec<EntityId>, Box<dyn std::error::Error>> {
    cells
        .iter()
        .map(|&(x, y)| Ok(world.spawn_unit(kind, PlayerId(0), center(x, y))?))
        .collect()
}

fn members(world: &SimWorld, group: u32) -> Vec<(EntityId, u32)> {
    world
        .route_groups()
        .iter()
        .find(|g| g.id == RouteGroupId(group))
        .map(|g| g.members.clone())
        .unwrap_or_default()
}

/// G-6a: ordinals through Move A, queued Move B, a chase pushed in front and ended, a retarget,
/// a flee and non-queued replacements; memberships drop exactly when their order is consumed.
#[test]
fn g6a_ordinals_follow_every_insertion_path() -> TestResult {
    let (mut world, kind) = world(flat()?, 75, MoverType::Ground)?;
    let ids = spawn_row(&mut world, kind, &[(10, 10), (12, 10), (14, 10)])?;
    let a = center(12, 13);
    let b = center(40, 40);
    tick(
        &mut world,
        vec![command(&ids, UnitOrder::Move { target: a }, false)],
    );
    tick(
        &mut world,
        vec![command(&ids, UnitOrder::Move { target: b }, true)],
    );
    let expect_a: Vec<_> = ids.iter().map(|&id| (id, 0)).collect();
    let expect_b: Vec<_> = ids.iter().map(|&id| (id, 1)).collect();
    assert_eq!(members(&world, 1), expect_a);
    assert_eq!(members(&world, 2), expect_b);
    let group = &world.route_groups()[0];
    assert_eq!((group.kind, group.target), (RouteGroupKind::Move, a));
    for &id in &ids {
        let unit = world.entities.get(&id).ok_or("unit")?;
        assert_eq!(
            (unit.route_orders_issued, unit.active_route_ordinal()),
            (2, 0)
        );
    }

    // A chase is pushed in front of the route order and ends; a retarget rewrites the front
    // order in place. Neither is a route order entering the queue.
    let unit = world.entities.get_mut(&ids[0]).ok_or("unit")?;
    unit.order_queue.push_front(UnitOrder::AttackTarget {
        target: ids[1],
        origin: AttackOrigin::Acquired,
    });
    assert_eq!(unit.active_route_ordinal(), 0);
    unit.order_queue.pop_front();
    if let Some(UnitOrder::Move { target }) = unit.order_queue.front_mut() {
        *target = a;
    }
    assert_eq!(
        (unit.route_orders_issued, unit.active_route_ordinal()),
        (2, 0)
    );

    // Walk until every unit has consumed Move A: group 1 dissolves, group 2 is active.
    for _ in 0..200 {
        tick(&mut world, Vec::new());
        if world.route_groups().iter().all(|g| g.id != RouteGroupId(1)) {
            break;
        }
    }
    assert_eq!(members(&world, 1), Vec::new(), "Move A was consumed by all");
    assert_eq!(members(&world, 2), expect_b);
    for &id in &ids {
        let unit = world.entities.get(&id).ok_or("unit")?;
        assert_eq!(unit.active_route_ordinal(), 1);
    }

    // A flee clears the queue and enqueues a Move: the flee path's own insertion.
    let unit = world.entities.get_mut(&ids[0]).ok_or("unit")?;
    unit.order_queue.clear();
    unit.enqueue_order(UnitOrder::Move {
        target: center(5, 5),
    });
    assert_eq!(
        (unit.route_orders_issued, unit.active_route_ordinal()),
        (3, 2)
    );
    tick(&mut world, Vec::new());
    assert_eq!(members(&world, 2), expect_b[1..].to_vec());

    // A single-unit non-queued replacement drops that member and forms no group.
    tick(
        &mut world,
        vec![command(&ids[1..2], UnitOrder::Move { target: b }, false)],
    );
    tick(&mut world, Vec::new());
    assert_eq!(members(&world, 2), expect_b[2..].to_vec());
    assert_eq!(world.route_groups().len(), 1);

    // A non-queued group order replaces the rest: group 2 dissolves, group 3 forms.
    tick(
        &mut world,
        vec![command(&ids, UnitOrder::AttackMove { target: a }, false)],
    );
    tick(&mut world, Vec::new());
    assert_eq!(members(&world, 2), Vec::new());
    let third = members(&world, 3);
    assert_eq!(
        third,
        vec![(ids[0], 3), (ids[1], 3), (ids[2], 2)],
        "each member is bound to the ordinal its own counter gave the order"
    );
    assert_eq!(world.route_groups()[0].kind, RouteGroupKind::AttackMove);
    assert_eq!(world.next_route_group(), RouteGroupId(4));
    Ok(())
}

/// G-4a: group ids and memberships do not depend on the order of `PlayerCommand::entities`.
#[test]
fn g4a_groups_do_not_depend_on_selection_order() -> TestResult {
    let mut results = Vec::new();
    for reverse in [false, true] {
        let (mut world, kind) = world(flat()?, 75, MoverType::Ground)?;
        let mut ids = spawn_row(&mut world, kind, &[(10, 10), (12, 10), (14, 10), (16, 10)])?;
        if reverse {
            ids.reverse();
        }
        let (left, right) = ids.split_at(2);
        tick(
            &mut world,
            vec![
                command(
                    left,
                    UnitOrder::Move {
                        target: center(30, 30),
                    },
                    false,
                ),
                command(
                    right,
                    UnitOrder::AttackMove {
                        target: center(31, 30),
                    },
                    false,
                ),
                command(
                    &ids,
                    UnitOrder::Move {
                        target: center(40, 40),
                    },
                    true,
                ),
            ],
        );
        results.push((world.route_groups().to_vec(), world.compute_checksum()));
    }
    // Reversal changes which units each half selects, so compare the full-selection group,
    // which has the same members, and the ids, which follow command order alone.
    let ids = |r: &(Vec<RouteGroup>, u64)| r.0.iter().map(|g| g.id).collect::<Vec<_>>();
    assert_eq!(ids(&results[0]), ids(&results[1]));
    assert_eq!(results[0].0[2], results[1].0[2]);
    Ok(())
}

/// D-1: a command with fewer than two ground recipients forms no group; at tolerance 0 no
/// command forms one, while the order counters advance alike.
#[test]
fn small_commands_and_tolerance_zero_form_no_group() -> TestResult {
    for permille in [0, 75] {
        let (mut world, kind) = world(flat()?, permille, MoverType::Ground)?;
        let ids = spawn_row(&mut world, kind, &[(10, 10), (12, 10)])?;
        tick(
            &mut world,
            vec![command(
                &ids[..1],
                UnitOrder::Move {
                    target: center(30, 30),
                },
                false,
            )],
        );
        assert_eq!(world.route_groups(), &[]);
        tick(
            &mut world,
            vec![command(
                &ids,
                UnitOrder::Move {
                    target: center(30, 30),
                },
                false,
            )],
        );
        assert_eq!(world.route_groups().len(), usize::from(permille > 0));
        let issued = world
            .entities
            .get(&ids[0])
            .ok_or("unit")?
            .route_orders_issued;
        assert_eq!(issued, 2);
    }
    Ok(())
}

/// A catalog directory holding copies of the shipped files, removed on drop.
struct Bundle {
    directory: PathBuf,
}

impl Bundle {
    fn new(label: &str) -> Result<Self, std::io::Error> {
        let directory = std::env::temp_dir().join(format!(
            "space-scuffle-movement-rules-{}-{label}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory)?;
        for entry in std::fs::read_dir(catalog_directory())? {
            let path = entry?.path();
            if let Some(name) = path.file_name() {
                std::fs::copy(&path, directory.join(name))?;
            }
        }
        Ok(Self { directory })
    }

    fn rules_path(&self) -> PathBuf {
        self.directory.join("movement_rules.ron")
    }
}

impl Drop for Bundle {
    fn drop(&mut self) {
        // Cleanup of a temporary directory; a failure leaves a stray directory only.
        drop(std::fs::remove_dir_all(&self.directory));
    }
}

/// G-11: a tolerance above 1000, a missing file and an unparsable file each fail loudly with the
/// path; the shipped file loads.
#[test]
fn g11_movement_rules_fail_loudly_with_the_path() -> TestResult {
    let shipped = RuntimeCatalogs::load_from_directory(catalog_directory())?;
    assert!(shipped.units.movement_rules().lane_tolerance_permille() <= 1000);

    let bundle = Bundle::new("g11")?;
    let path = bundle.rules_path();
    std::fs::write(&path, "(group_routing: (lane_tolerance_permille: 1001))")?;
    assert!(matches!(
        RuntimeCatalogs::load_from_directory(&bundle.directory),
        Err(CatalogLoadError::Invalid { path: failed, source: SimCoreError::CatalogInvalidValue { .. } })
            if failed == path
    ));
    std::fs::write(&path, "(group_routing: (lane_tolerance_permille: 1000))")?;
    let most = RuntimeCatalogs::load_from_directory(&bundle.directory)?;
    assert_eq!(most.units.movement_rules().lane_tolerance_permille(), 1000);
    std::fs::write(&path, "(group_routing:")?;
    assert!(matches!(
        RuntimeCatalogs::load_from_directory(&bundle.directory),
        Err(CatalogLoadError::Invalid { path: failed, source: SimCoreError::RonDeserializationError(_) })
            if failed == path
    ));
    std::fs::remove_file(&path)?;
    let error = RuntimeCatalogs::load_from_directory(&bundle.directory)
        .err()
        .ok_or("a missing movement_rules.ron must fail")?;
    assert!(
        matches!(&error, CatalogLoadError::Io { path: failed, source } if *failed == path && source.kind() == std::io::ErrorKind::NotFound)
    );
    assert!(error.to_string().contains("movement_rules.ron"));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// WP4: the lane rule (D-3, D-4).
// ---------------------------------------------------------------------------------------------

/// The lane word of `id`'s stored route, starting from where it stands.
fn route_word(world: &SimWorld, id: EntityId) -> Option<Vec<i32>> {
    let unit = world.entities.get(&id)?;
    let mut polyline = vec![unit.position];
    polyline.extend_from_slice(&unit.current_funnel_waypoints);
    Some(world.pathfinder.lane_word(&polyline).letters().to_vec())
}

/// Whether `id`'s stored route passes the island of [`island`] on its west side, read where
/// the route crosses the island's middle row y = 24; `None` if it does not cross it.
fn passes_west(world: &SimWorld, id: EntityId) -> Option<bool> {
    let unit = world.entities.get(&id)?;
    let mut polyline = vec![unit.position];
    polyline.extend_from_slice(&unit.current_funnel_waypoints);
    let row = SimScalar::from_num(24);
    polyline.windows(2).find_map(|leg| {
        let (a, b) = (leg[0], leg[1]);
        ((a.y - row) * (b.y - row) <= SimScalar::ZERO && a.y != b.y)
            .then(|| a.x.min(b.x) < SimScalar::from_num(24))
    })
}

fn lane_of(world: &SimWorld, group: u32) -> Option<GroupLane> {
    world
        .route_groups()
        .iter()
        .find(|g| g.id == RouteGroupId(group))
        .and_then(|g| match &g.lane {
            LaneState::Lane(lane) => Some(lane.clone()),
            _ => None,
        })
}

fn lane_events(world: &SimWorld) -> Vec<(EntityId, Option<SimScalar>, bool)> {
    world
        .route_outcomes()
        .iter()
        .filter_map(|outcome| match outcome.kind {
            RouteOutcomeKind::GroupLane {
                lane_length,
                adopted,
                ..
            } => Some((outcome.entity, lane_length, adopted)),
            _ => None,
        })
        .collect()
}

fn checked_tick(world: &mut SimWorld, commands: Vec<PlayerCommand>) -> TestResult {
    tick(world, commands);
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(format!("tick {}: {diagnostics:?}", world.current_tick).into())
    }
}

/// A 48x48 map with a walled island, cells `x in 18..30, y in 18..30`, and, when `west_gap` is
/// given, a wall west of it leaving a passage of that many cells beside the island.
fn island(west_gap: Option<u32>) -> Result<GameMap, Box<dyn std::error::Error>> {
    let mut map = flat()?;
    let mut block = |xs: std::ops::Range<u32>| -> TestResult {
        for y in 18..30 {
            for x in xs.clone() {
                map.cell_mut(x, y)?.flags = CellFlags::empty();
            }
        }
        Ok(())
    };
    block(18..30)?;
    if let Some(gap) = west_gap {
        block(0..18 - gap)?;
    }
    Ok(map)
}

const NORTH: (i32, i32) = (24, 44);
/// The western members of the island tests: they make the group's leader one of them.
const WEST_ROW: [(i32, i32); 5] = [(14, 6), (15, 6), (16, 6), (17, 6), (18, 6)];

/// G-2: a member whose lane route is more than 7.5 % longer keeps its own route and reports
/// `adopted: false`; a member behind the island from the lane is separated and runs no
/// restricted search.
#[test]
fn g2_far_and_separated_members_keep_their_own_routes() -> TestResult {
    let (mut world, kind) = world(island(None)?, 75, MoverType::Ground)?;
    let mut cells = WEST_ROW.to_vec();
    cells.extend([(34, 10), (34, 22)]);
    let ids = spawn_row(&mut world, kind, &cells)?;
    let (far, walled) = (ids[5], ids[6]);
    let target = center(NORTH.0, NORTH.1);
    checked_tick(
        &mut world,
        vec![command(&ids, UnitOrder::Move { target }, false)],
    )?;
    let lane = lane_of(&world, 1).ok_or("the group has a lane")?;
    assert!(
        ids[..5].contains(&lane.leader),
        "the leader is a western member"
    );
    let events = lane_events(&world);
    let far_event = events
        .iter()
        .find(|(id, _, _)| *id == far)
        .ok_or("the far member ran the restricted search")?;
    assert!(far_event.1.is_some(), "the lane admits the far member");
    assert!(!far_event.2, "beyond 7.5 % the lane is not adopted");
    assert!(events.iter().all(|(id, _, _)| *id != walled), "separated");
    assert_eq!(passes_west(&world, far), Some(false));
    assert_eq!(passes_west(&world, walled), Some(false));
    for &id in &ids[..5] {
        assert_eq!(passes_west(&world, id), Some(true), "{id:?}");
    }
    Ok(())
}

/// G-7 and G-9: a member just east of the centre line adopts the western lane within tolerance,
/// re-plans into it after its route is dropped, and resumes it after a chase in front of its
/// attack-move; an air unit in the command forms no membership.
#[test]
fn g7_g9_replans_and_resumed_attack_moves_keep_the_lane() -> TestResult {
    let (mut world, kind) = world(island(None)?, 75, MoverType::Ground)?;
    let (air, air_kind) = {
        let catalogs = RuntimeCatalogs::load_from_directory(catalog_directory())?;
        let air_kind = catalogs
            .units
            .archetypes()
            .find(|a| a.mover_type == MoverType::Air && !a.is_immobile_by_default())
            .ok_or("an air archetype")?
            .unit_type_id;
        (
            world.spawn_unit(air_kind, PlayerId(0), center(10, 6))?,
            air_kind,
        )
    };
    assert_ne!(air_kind, kind);
    let mut cells = WEST_ROW.to_vec();
    cells.push((25, 6));
    let mut ids = spawn_row(&mut world, kind, &cells)?;
    let east = ids[5];
    ids.push(air);
    let target = center(NORTH.0, NORTH.1);
    checked_tick(
        &mut world,
        vec![command(&ids, UnitOrder::AttackMove { target }, false)],
    )?;
    let group = &world.route_groups()[0];
    assert_eq!(group.ordinal_of(air), None, "air units do not path");
    assert_eq!(group.members.len(), 6);
    assert!(lane_of(&world, 1).is_some());
    let word = Some(true);
    let adopted: Vec<_> = lane_events(&world)
        .into_iter()
        .filter(|(id, _, _)| *id == east)
        .collect();
    assert_eq!(adopted.len(), 1);
    assert!(adopted[0].2, "within 7.5 % the lane is adopted");
    assert_eq!(passes_west(&world, east), word);

    // The route is dropped as stale: the re-plan finds the lane again.
    for _ in 0..12 {
        checked_tick(&mut world, Vec::new())?;
    }
    world
        .entities
        .get_mut(&east)
        .ok_or("east")?
        .current_funnel_waypoints
        .clear();
    for _ in 0..12 {
        checked_tick(&mut world, Vec::new())?;
    }
    assert_eq!(passes_west(&world, east), word, "re-plan stays in the lane");

    // A chase pushed in front of the attack-move ends; the attack-move resumes in the lane.
    let unit = world.entities.get_mut(&east).ok_or("east")?;
    unit.order_queue.push_front(UnitOrder::AttackTarget {
        target: ids[0],
        origin: AttackOrigin::Acquired,
    });
    unit.current_funnel_waypoints.clear();
    checked_tick(&mut world, Vec::new())?;
    assert_eq!(
        members(&world, 1).len(),
        6,
        "the chase keeps the membership"
    );
    let unit = world.entities.get_mut(&east).ok_or("east")?;
    if matches!(
        unit.order_queue.front(),
        Some(UnitOrder::AttackTarget { .. })
    ) {
        unit.order_queue.pop_front();
    }
    unit.current_funnel_waypoints.clear();
    for _ in 0..12 {
        checked_tick(&mut world, Vec::new())?;
    }
    assert_eq!(
        passes_west(&world, east),
        word,
        "the resumed attack-move keeps the lane"
    );
    Ok(())
}

/// G-8: a member too large for the lane's narrowest passage gets `NoPathFound` from the
/// restricted search and keeps its own route.
#[test]
fn g8_a_member_too_large_for_the_lane_keeps_its_own_route() -> TestResult {
    let mut catalogs = RuntimeCatalogs::load_from_directory(catalog_directory())?;
    catalogs
        .units
        .set_movement_rules(MovementRules::with_lane_tolerance(75)?);
    let kinds: Vec<UnitTypeId> = catalogs
        .units
        .archetypes()
        .filter(|a| {
            a.mover_type == MoverType::Ground
                && !a.is_immobile_by_default()
                && !matches!(a.shape, ShapeKind::BuildingDome { .. })
        })
        .map(|a| a.unit_type_id)
        .take(2)
        .collect();
    let [small, large] = kinds[..] else {
        return Err("two ground archetypes".into());
    };
    for (kind, radius) in [
        (small, SimScalar::from_num(0.375)),
        (large, SimScalar::from_num(0.75)),
    ] {
        let arch = catalogs.units.get_mut(kind)?;
        arch.collision_radius = radius;
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
    }
    let mut world =
        SimWorld::with_catalogs(island(Some(1))?, SEED, catalogs.units, catalogs.upgrades);
    let mut ids = spawn_row(&mut world, small, &WEST_ROW)?;
    let big = world.spawn_unit(large, PlayerId(0), center(20, 8))?;
    ids.push(big);
    let target = center(NORTH.0, NORTH.1);
    checked_tick(
        &mut world,
        vec![command(&ids, UnitOrder::Move { target }, false)],
    )?;
    let lane = lane_of(&world, 1).ok_or("lane")?;
    assert_ne!(lane.leader, big);
    let events = lane_events(&world);
    let event = events
        .iter()
        .find(|(id, _, _)| *id == big)
        .ok_or("the large member ran the restricted search")?;
    assert_eq!((event.1, event.2), (None, false));
    assert_eq!(passes_west(&world, big), Some(false));
    assert_eq!(passes_west(&world, ids[0]), Some(true));
    Ok(())
}

/// G-10: a group whose target the leader cannot reach has no lane; its members route alone.
#[test]
fn g10_an_unreachable_target_gives_no_lane() -> TestResult {
    let mut map = flat()?;
    for (x, y) in sim_core::scenario::ring_perimeter(38, 43, 38, 43) {
        map.cell_mut(u32::try_from(x)?, u32::try_from(y)?)?.flags = CellFlags::empty();
    }
    let (mut world, kind) = world(map, 75, MoverType::Ground)?;
    let ids = spawn_row(&mut world, kind, &WEST_ROW)?;
    checked_tick(
        &mut world,
        vec![command(
            &ids,
            UnitOrder::Move {
                target: center(40, 40),
            },
            false,
        )],
    )?;
    assert_eq!(world.route_groups()[0].lane, LaneState::None);
    assert_eq!(lane_events(&world), Vec::new());
    Ok(())
}

/// G-6b: queued orders. B's lane is computed when the first member activates B, from that
/// tick's leader; a later non-queued order drops both memberships.
#[test]
fn g6b_a_queued_groups_lane_waits_for_its_order() -> TestResult {
    let (mut world, kind) = world(island(None)?, 75, MoverType::Ground)?;
    let ids = spawn_row(&mut world, kind, &WEST_ROW)?;
    let a = center(16, 9);
    let b = center(NORTH.0, NORTH.1);
    checked_tick(
        &mut world,
        vec![command(&ids, UnitOrder::Move { target: a }, false)],
    )?;
    checked_tick(
        &mut world,
        vec![command(&ids, UnitOrder::Move { target: b }, true)],
    )?;
    assert!(lane_of(&world, 1).is_some());
    assert_eq!(world.route_groups()[1].lane, LaneState::Pending);
    let mut computed = None;
    for _ in 0..200 {
        let active_before: Vec<EntityId> = ids
            .iter()
            .copied()
            .filter(|id| {
                world
                    .entities
                    .get(id)
                    .is_some_and(|e| e.active_route_ordinal() == 1)
            })
            .collect();
        checked_tick(&mut world, Vec::new())?;
        if let Some(lane) = lane_of(&world, 2) {
            computed = Some((lane, active_before));
            break;
        }
    }
    let (lane, active) = computed.ok_or("B's lane is computed once B is active")?;
    assert!(
        !active.is_empty(),
        "computed on the first tick a member has B active"
    );
    assert!(
        active.contains(&lane.leader),
        "the leader is one of that tick's active members"
    );
    checked_tick(
        &mut world,
        vec![command(
            &ids,
            UnitOrder::Move {
                target: center(5, 5),
            },
            false,
        )],
    )?;
    checked_tick(&mut world, Vec::new())?;
    assert_eq!(members(&world, 1), Vec::new());
    assert_eq!(members(&world, 2), Vec::new());
    Ok(())
}

/// The world of the Q0079 split: the shipped showcase with player 1's order before Amendment 5,
/// `AttackMoveToStart` towards player 0, built in memory (never under `assets/`).
fn g1_world(permille: u32) -> Result<SimWorld, Box<dyn std::error::Error>> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut manifest = ScenarioManifest::from_ron_str(&std::fs::read_to_string(
        assets.join("scenarios/showcase.ron"),
    )?)?;
    manifest.initial_orders = vec![
        InitialOrder::AttackMoveToCell {
            player_slot: 0,
            cell: (52, 52),
        },
        InitialOrder::AttackMoveToStart {
            player_slot: 1,
            target_player_slot: 0,
        },
    ];
    let document = MapFile::load(&assets.join(&manifest.map))?.decode()?;
    let loadouts = StartLoadoutCatalog::load(assets.join("catalog/start_loadouts.ron"))?;
    let mut catalogs = RuntimeCatalogs::load_from_directory(assets.join("catalog"))?;
    catalogs
        .units
        .set_movement_rules(MovementRules::with_lane_tolerance(permille)?);
    let mut world = build_world_with_catalogs(
        &document,
        &loadouts,
        &manifest.players,
        manifest.seed,
        catalogs.units,
        catalogs.upgrades,
    )?;
    apply_initial_orders(
        &mut world,
        &document,
        &manifest.players,
        &manifest.initial_orders,
    )?;
    Ok(world)
}

fn player_one_group(world: &SimWorld) -> Result<RouteGroup, Box<dyn std::error::Error>> {
    Ok(world
        .route_groups()
        .iter()
        .find(|g| g.player == PlayerId(1))
        .ok_or("player 1's army is one group")?
        .clone())
}

/// G-1: with the old player 1 order, player 1's ten attackers all cross y = 32 on one side, and
/// each unit's first route has the leader's lane word.
#[test]
fn g1_the_q0079_split_closes() -> TestResult {
    let mut world = g1_world(75)?;
    let group = player_one_group(&world)?;
    assert_eq!(group.members.len(), 10);
    checked_tick(&mut world, Vec::new())?;
    let lane = lane_of(&world, group.id.0).ok_or("lane")?;
    assert!(
        !lane.word.is_empty(),
        "the lane passes a side of the plateau"
    );
    for &(id, _) in &group.members {
        assert_eq!(
            route_word(&world, id),
            Some(lane.word.letters().to_vec()),
            "{id:?}"
        );
    }
    let line = SimScalar::from_num(32);
    let mut crossed: BTreeMap<EntityId, bool> = BTreeMap::new();
    let mut last: BTreeMap<EntityId, SimVec2> = group
        .members
        .iter()
        .filter_map(|&(id, _)| Some((id, world.entities.get(&id)?.position)))
        .collect();
    for _ in 0..600 {
        checked_tick(&mut world, Vec::new())?;
        for (&id, before) in &mut last {
            let Some(now) = world.entities.get(&id).map(|e| e.position) else {
                continue;
            };
            if before.y > line && now.y <= line {
                crossed.entry(id).or_insert(now.x > line);
            }
            *before = now;
        }
        if crossed.len() == group.members.len() {
            break;
        }
    }
    assert_eq!(crossed.len(), 10, "every attacker crossed y = 32");
    let sides: BTreeSet<bool> = crossed.values().copied().collect();
    assert_eq!(sides.len(), 1, "one side: {crossed:?}");
    Ok(())
}

/// G-4b: the G-1 battle run twice gives identical checksum sequences. G-5: the leader killed on
/// the tick after the lane is computed changes neither the lane nor the members' lane words,
/// and nothing is diagnosed.
#[test]
fn g4b_g5_the_g1_battle_is_deterministic_and_survives_its_leader() -> TestResult {
    let mut runs = Vec::new();
    for _ in 0..2 {
        let mut world = g1_world(75)?;
        let mut sums = Vec::new();
        for _ in 0..120 {
            checked_tick(&mut world, Vec::new())?;
            sums.push(world.last_checksum());
        }
        runs.push(sums);
    }
    assert_eq!(runs[0], runs[1]);

    let mut world = g1_world(75)?;
    let group = player_one_group(&world)?;
    checked_tick(&mut world, Vec::new())?;
    let lane = lane_of(&world, group.id.0).ok_or("lane")?;
    world.entities.get_mut(&lane.leader).ok_or("leader")?.hp = SimScalar::ZERO;
    for _ in 0..60 {
        checked_tick(&mut world, Vec::new())?;
        assert_eq!(lane_of(&world, group.id.0).as_ref(), Some(&lane));
    }
    for &(id, _) in &group.members {
        if id != lane.leader {
            assert_eq!(
                route_word(&world, id),
                Some(lane.word.letters().to_vec()),
                "{id:?}"
            );
        }
    }
    Ok(())
}

/// Every unit's state per tick, for comparisons that exclude the hashed groups.
fn positions(world: &SimWorld) -> Vec<(EntityId, SimVec2, Vec<SimVec2>)> {
    world
        .entities
        .iter()
        .map(|(id, e)| (*id, e.position, e.current_funnel_waypoints.clone()))
        .collect()
}

/// G-3: tolerance 0 equals per-unit routing bit-for-bit (checksums every tick) on the canyon,
/// with group, queued and attack commands; at 75 ‰ on a flat arena (no cuts) every position
/// equals the run at 0.
#[test]
fn g3_tolerance_zero_is_per_unit_routing() -> TestResult {
    let canyon = || -> Result<GameMap, Box<dyn std::error::Error>> {
        Ok(MapFile::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/maps/showcase_canyon.scmap.ron"),
        )?
        .decode()?
        .to_game_map()?)
    };
    let script = |ids: &[EntityId]| {
        vec![
            (
                0,
                command(
                    ids,
                    UnitOrder::Move {
                        target: center(32, 58),
                    },
                    false,
                ),
            ),
            (
                1,
                command(
                    &ids[..4],
                    UnitOrder::AttackMove {
                        target: center(8, 8),
                    },
                    true,
                ),
            ),
            (
                90,
                command(
                    &ids[2..],
                    UnitOrder::AttackMove {
                        target: center(56, 8),
                    },
                    false,
                ),
            ),
        ]
    };
    let cells = [
        (28, 4),
        (30, 4),
        (32, 4),
        (34, 4),
        (36, 4),
        (29, 6),
        (33, 6),
    ];
    let run = |map: GameMap,
               permille: u32,
               split: bool,
               compare_positions: bool|
     -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let (mut world, kind) = world(map, permille, MoverType::Ground)?;
        let ids = spawn_row(&mut world, kind, &cells)?;
        let script = script(&ids);
        let mut trace = Vec::new();
        for t in 0..200 {
            let mut commands = Vec::new();
            for (at, cmd) in &script {
                if *at == t {
                    if split {
                        commands.extend(cmd.entities.iter().map(|id| PlayerCommand {
                            entities: vec![*id],
                            ..cmd.clone()
                        }));
                    } else {
                        commands.push(cmd.clone());
                    }
                }
            }
            checked_tick(&mut world, commands)?;
            trace.push(if compare_positions {
                format!("{:?}", positions(&world))
            } else {
                format!("{:#x}", world.last_checksum())
            });
        }
        Ok(trace)
    };
    assert_eq!(
        run(canyon()?, 0, false, false)?,
        run(canyon()?, 0, true, false)?
    );
    assert_eq!(
        run(flat()?, 0, false, true)?,
        run(flat()?, 75, false, true)?
    );
    Ok(())
}
