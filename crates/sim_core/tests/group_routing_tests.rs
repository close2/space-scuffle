//! Group-consistent routing (plan `docs/plans/2026-11-10-group-routing.md`): route groups,
//! their order-ordinal binding, the movement rules file and the lane rule, through the public
//! command path.
//!
//! The tests build worlds from the shipped catalog files and set the group-routing tolerance in
//! code where they need a value other than the shipped one, so each test states the tolerance it
//! exercises.

use sim_core::catalog::{CatalogLoadError, MovementRules, RuntimeCatalogs};
use sim_core::movement::groups::{RouteGroupId, RouteGroupKind};
use sim_core::{SimCoreError, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2,
    Tick, TickBundle, UnitOrder, UnitTypeId,
};
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
    let ids = |r: &(Vec<sim_core::movement::groups::RouteGroup>, u64)| {
        r.0.iter().map(|g| g.id).collect::<Vec<_>>()
    };
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
