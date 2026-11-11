//! # Starting-Unit Placement Properties and the Three-Ring Oracle
//!
//! Property tests of where the scenario builder places a player's starting units, on generated
//! maps, through the public builder (`build_world_with_catalogs`). Plan:
//! `docs/plans/2026-11-11-scenario-outer-rings.md` §5.2.
//!
//! ## The oracle
//! The file carries a reference implementation of the placement rule as it was before outer
//! rings existed: rings `d = 1, 2, 3` around the town hall, each in `ring_perimeter` order, on
//! cells satisfying predicate *P* (in bounds, the hall's cliff level, ground-pathable, not abyss,
//! not a ramp, no structure, not claimed). Every existing scenario was placed by that rule, so
//! the builder must reproduce it cell for cell wherever it applies; that is what keeps every
//! existing scenario's checksum unchanged. `shipped_scenarios_match_the_three_ring_oracle`
//! checks it on every scenario under `assets/scenarios/`.
//!
//! ## Properties
//! - **P1 (compatibility):** the first `min(n, oracle capacity)` placements equal the oracle's.
//! - **P2 (validity):** distinct cells, all satisfying *P*; beyond the oracle's capacity also
//!   *Q* (in the hall's ground region, outside other starts' inner zones, not a resource or
//!   watchtower cell).
//! - **P3 (order):** ring index never decreases, and within a ring cells follow
//!   `ring_perimeter`.
//! - **P4 (completeness):** a loadout either loads whole, or fails with `requested == n` and
//!   `placed < n`, and then the world of `placed` units leaves no eligible cell unclaimed. Counts
//!   of 850 to 1,100 units on 32x32 maps exhaust the region, so this half is exercised.
//! - **P5 (prefix):** the placement of `n` units is a prefix of the placement of `n + k`.
//! - **P6 (determinism):** two builds of the same input have equal checksums.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "test assertions report failures by panicking"
)]

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use sim_core::scenario::{
    build_world_with_catalogs, load_scenario_world, ring_perimeter, LoadoutUnit, PlayerSetup,
    ScenarioError, StartLoadout, StartLoadoutCatalog,
};
use sim_core::SimWorld;
use sim_map::{
    validate_map, CellFlags, CliffLevel, MapCellSpec, MapDocument, MapEntityId, MapMetadata,
    MapResource, MapStartLocation, ResourceKind, TerrainKind,
};
use sim_types::{ShapeKind, UnitTypeId};

/// Inner rings whose placement is pinned for compatibility.
const INNER_RINGS: i32 = 3;
/// Footprint side of both generated start locations, in cells.
const FOOTPRINT: u8 = 5;

/// An inclusive cell rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x_min: i32,
    x_max: i32,
    y_min: i32,
    y_max: i32,
}

impl Rect {
    fn grown(self, d: i32) -> Self {
        Self {
            x_min: self.x_min - d,
            x_max: self.x_max + d,
            y_min: self.y_min - d,
            y_max: self.y_max + d,
        }
    }

    fn contains(self, x: i32, y: i32) -> bool {
        (self.x_min..=self.x_max).contains(&x) && (self.y_min..=self.y_max).contains(&y)
    }

    /// Chebyshev distance from `(x, y)` to the rectangle; 0 inside.
    fn ring_of(self, x: i32, y: i32) -> i32 {
        (self.x_min - x)
            .max(x - self.x_max)
            .max(self.y_min - y)
            .max(y - self.y_max)
            .max(0)
    }
}

fn signed(value: u32) -> i32 {
    i32::try_from(value).expect("invariant: map coordinates are at most 256")
}

/// The shipped archetypes the tests use: a dome town hall of exactly 5x5 cells and the mobile
/// units, both chosen by shape from the shipped catalog.
struct Roster {
    hall: String,
    mobile: Vec<String>,
}

fn roster() -> Roster {
    let catalog = sim_fixtures::shipped_catalogs().units;
    let hall = catalog
        .archetypes()
        .find_map(|a| match a.shape {
            ShapeKind::BuildingDome {
                footprint_w: 5,
                footprint_h: 5,
            } => Some(a.slug.clone()),
            _ => None,
        })
        .expect("the shipped catalog has a 5x5 dome");
    let mobile: Vec<String> = catalog
        .archetypes()
        .filter(|a| matches!(a.shape, ShapeKind::UnitSphere))
        .map(|a| a.slug.clone())
        .take(4)
        .collect();
    assert!(!mobile.is_empty(), "the shipped catalog has mobile units");
    Roster { hall, mobile }
}

/// A loadout of `n` units: entries of up to 37 units cycling through the roster's mobile units.
fn loadout(roster: &Roster, slug: &str, n: usize) -> StartLoadout {
    let mut units = Vec::new();
    let mut left = n;
    let mut kind = 0;
    while left > 0 {
        let count = left.min(37);
        units.push(LoadoutUnit {
            slug: roster.mobile[kind % roster.mobile.len()].clone(),
            count: u8::try_from(count).expect("invariant: count is at most 37"),
        });
        left -= count;
        kind += 1;
    }
    StartLoadout {
        slug: slug.to_string(),
        display_name: slug.to_string(),
        editor_default: slug == "p0",
        town_hall: roster.hall.clone(),
        units,
        starting_resources: None,
    }
}

/// The archetype of every unit of `loadout`, in loadout order.
fn expanded(world: &SimWorld, loadout: &StartLoadout) -> Vec<UnitTypeId> {
    loadout
        .units
        .iter()
        .flat_map(|entry| {
            let id = world
                .catalog
                .get_by_slug(&entry.slug)
                .expect("loadout slugs come from the catalog")
                .unit_type_id;
            std::iter::repeat_n(id, usize::from(entry.count))
        })
        .collect()
}

/// Builds the world of `doc` with player `p` fielding `counts[p]` units.
fn build(
    roster: &Roster,
    doc: &MapDocument,
    counts: [usize; 2],
) -> (Result<SimWorld, ScenarioError>, [StartLoadout; 2]) {
    let loadouts = [
        loadout(roster, "p0", counts[0]),
        loadout(roster, "p1", counts[1]),
    ];
    let catalog = StartLoadoutCatalog {
        loadouts: loadouts.to_vec(),
    };
    let players = [
        PlayerSetup {
            player_slot: 0,
            loadout_slug: "p0".to_string(),
        },
        PlayerSetup {
            player_slot: 1,
            loadout_slug: "p1".to_string(),
        },
    ];
    let catalogs = sim_fixtures::shipped_catalogs();
    let world = build_world_with_catalogs(
        doc,
        &catalog,
        &players,
        11,
        catalogs.units,
        catalogs.upgrades,
    );
    (world, loadouts)
}

/// The hall rectangle of `slot` in `world`, with its anchor cliff level.
fn hall_of(world: &SimWorld, slot: u8) -> Option<(Rect, CliffLevel)> {
    world.map.structures().values().find_map(|placed| {
        let owner = world.entities.get(&placed.id)?.owner.0;
        if owner != slot {
            return None;
        }
        let x = signed(placed.top_left.0);
        let y = signed(placed.top_left.1);
        let level = world
            .map
            .cell(placed.top_left.0, placed.top_left.1)
            .ok()?
            .cliff_level;
        Some((
            Rect {
                x_min: x,
                x_max: x + i32::from(placed.width) - 1,
                y_min: y,
                y_max: y + i32::from(placed.height) - 1,
            },
            level,
        ))
    })
}

/// The units of `slot` in spawn order, as `(archetype, cell)`.
fn units_of(world: &SimWorld, slot: u8) -> Vec<(UnitTypeId, (i32, i32))> {
    world
        .entities
        .values()
        .filter(|e| e.owner.0 == slot && !world.map.structures().contains_key(&e.id))
        .map(|e| {
            let x = i32::try_from(e.position.x.saturating_to_u32()).unwrap_or(i32::MAX);
            let y = i32::try_from(e.position.y.saturating_to_u32()).unwrap_or(i32::MAX);
            (e.unit_type_id, (x, y))
        })
        .collect()
}

/// Terrain part of predicate *P*: on the map, the hall's cliff level, ground-pathable, not abyss,
/// not a ramp, no structure.
fn terrain_p(world: &SimWorld, level: CliffLevel, x: i32, y: i32) -> bool {
    let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) else {
        return false;
    };
    let Ok(cell) = world.map.cell(ux, uy) else {
        return false;
    };
    cell.cliff_level == level
        && cell.flags.contains(CellFlags::GROUND_PATHABLE)
        && cell.terrain_kind != TerrainKind::Abyss
        && cell.ramp.is_none()
        && !cell.flags.contains(CellFlags::RAMP)
        && cell.occupying_structure.is_none()
}

/// The three-ring oracle: the cells today's rule visits for a hall, in order, with no claims of
/// other players (other players' inner rings cannot reach them by the separation rule).
fn oracle_cells(world: &SimWorld, hall: Rect, level: CliffLevel) -> Vec<(i32, i32)> {
    (1..=INNER_RINGS)
        .flat_map(|d| {
            let r = hall.grown(d);
            ring_perimeter(r.x_min, r.x_max, r.y_min, r.y_max)
        })
        .filter(|&(x, y)| terrain_p(world, level, x, y))
        .collect()
}

/// Other starts' inner zones: every start's footprint but `own`'s, grown by three cells.
fn other_zones(doc: &MapDocument, own: u8) -> Vec<Rect> {
    doc.start_locations
        .iter()
        .filter(|s| s.player_slot != own)
        .map(|s| {
            let x = signed(s.cell.0);
            let y = signed(s.cell.1);
            Rect {
                x_min: x,
                x_max: x + i32::from(s.footprint.0) - 1,
                y_min: y,
                y_max: y + i32::from(s.footprint.1) - 1,
            }
            .grown(INNER_RINGS)
        })
        .collect()
}

/// Resource and watchtower cells of `doc`.
fn point_cells(doc: &MapDocument) -> BTreeSet<(i32, i32)> {
    doc.resources
        .iter()
        .map(|r| r.cell)
        .chain(doc.watchtowers.iter().map(|t| t.cell))
        .map(|(x, y)| (signed(x), signed(y)))
        .collect()
}

/// The hall's ground region, recomputed independently: 4-connected terrain-*P* cells outside
/// other starts' zones, seeded next to the hall's rectangle.
fn region(
    world: &SimWorld,
    doc: &MapDocument,
    slot: u8,
    hall: Rect,
    level: CliffLevel,
) -> BTreeSet<(i32, i32)> {
    let zones = other_zones(doc, slot);
    let admitted =
        |x: i32, y: i32| terrain_p(world, level, x, y) && !zones.iter().any(|z| z.contains(x, y));
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    let edge = hall.grown(1);
    for (x, y) in ring_perimeter(edge.x_min, edge.x_max, edge.y_min, edge.y_max) {
        let corner = (x == edge.x_min || x == edge.x_max) && (y == edge.y_min || y == edge.y_max);
        if !corner && admitted(x, y) && seen.insert((x, y)) {
            queue.push_back((x, y));
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
            if admitted(nx, ny) && seen.insert((nx, ny)) {
                queue.push_back((nx, ny));
            }
        }
    }
    seen
}

/// Position of `(x, y)` within ring `d` of `hall` in `ring_perimeter` order.
fn perimeter_index(hall: Rect, d: i32, cell: (i32, i32)) -> usize {
    let r = hall.grown(d);
    ring_perimeter(r.x_min, r.x_max, r.y_min, r.y_max)
        .position(|c| c == cell)
        .expect("a placed cell lies on its own ring")
}

/// Checks P1, P2 and P3 for `slot` in `world`, and returns the oracle's capacity.
fn check_placement(
    world: &SimWorld,
    doc: &MapDocument,
    slot: u8,
    expected_types: &[UnitTypeId],
) -> usize {
    let (hall, level) = hall_of(world, slot).expect("every player has a hall");
    let units = units_of(world, slot);
    assert_eq!(units.len(), expected_types.len(), "slot {slot}: unit count");
    let types: Vec<UnitTypeId> = units.iter().map(|(t, _)| *t).collect();
    assert_eq!(types, expected_types, "slot {slot}: loadout order");

    // P1: the inner prefix equals the oracle's.
    let oracle = oracle_cells(world, hall, level);
    let shared = units.len().min(oracle.len());
    let cells: Vec<(i32, i32)> = units.iter().map(|(_, c)| *c).collect();
    assert_eq!(cells[..shared], oracle[..shared], "slot {slot}: P1");

    // P2: distinct cells satisfying P, and Q beyond the oracle.
    let all_units: BTreeSet<(i32, i32)> = [0u8, 1]
        .iter()
        .flat_map(|&s| units_of(world, s).into_iter().map(|(_, c)| c))
        .collect();
    let distinct: BTreeSet<(i32, i32)> = cells.iter().copied().collect();
    assert_eq!(distinct.len(), cells.len(), "slot {slot}: distinct cells");
    let region = region(world, doc, slot, hall, level);
    let points = point_cells(doc);
    for (index, &(x, y)) in cells.iter().enumerate() {
        assert!(
            terrain_p(world, level, x, y),
            "slot {slot}: P at ({x}, {y})"
        );
        if index >= oracle.len() {
            assert!(
                hall.ring_of(x, y) > INNER_RINGS,
                "slot {slot}: outer unit on an inner ring"
            );
            assert!(
                region.contains(&(x, y)),
                "slot {slot}: Q region at ({x}, {y})"
            );
            assert!(
                !points.contains(&(x, y)),
                "slot {slot}: Q point at ({x}, {y})"
            );
        }
    }
    assert_eq!(
        all_units.len(),
        units_of(world, 0).len() + units_of(world, 1).len(),
        "no shared cell"
    );

    // P3: rings never decrease, and within a ring cells follow the perimeter.
    let mut last = (0, 0usize);
    for &(x, y) in &cells {
        let d = hall.ring_of(x, y);
        let at = (d, perimeter_index(hall, d, (x, y)));
        assert!(at > last || last == (0, 0), "slot {slot}: P3 at ({x}, {y})");
        last = at;
    }
    oracle.len()
}

/// P4's second half: `world` leaves no eligible cell unclaimed for `slot`.
fn check_exhausted(world: &SimWorld, doc: &MapDocument, slot: u8) {
    let (hall, level) = hall_of(world, slot).expect("every player has a hall");
    let claimed: BTreeSet<(i32, i32)> = [0u8, 1]
        .iter()
        .flat_map(|&s| units_of(world, s).into_iter().map(|(_, c)| c))
        .collect();
    for cell in oracle_cells(world, hall, level) {
        assert!(
            claimed.contains(&cell),
            "slot {slot}: inner cell {cell:?} left free"
        );
    }
    let points = point_cells(doc);
    for &(x, y) in &region(world, doc, slot, hall, level) {
        if hall.ring_of(x, y) > INNER_RINGS && !points.contains(&(x, y)) {
            assert!(
                claimed.contains(&(x, y)),
                "slot {slot}: region cell ({x}, {y}) left free"
            );
        }
    }
}

/// A generated map: `size`, the two starts' top-left cells, blockers and resource points.
#[derive(Debug, Clone)]
struct Generated {
    size: u32,
    starts: [(u32, u32); 2],
    blockers: Vec<(u32, u32, u8)>,
    resources: Vec<(u32, u32)>,
}

fn document(generated: &Generated) -> MapDocument {
    let size = generated.size;
    let ground = MapCellSpec {
        level: CliffLevel(0),
        ramp: None,
        terrain: TerrainKind::Grass,
        flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
    };
    let mut cells = vec![ground; usize::try_from(size * size).expect("invariant: small map")];
    let footprints: Vec<Rect> = generated
        .starts
        .iter()
        .map(|&(x, y)| Rect {
            x_min: signed(x),
            x_max: signed(x) + i32::from(FOOTPRINT) - 1,
            y_min: signed(y),
            y_max: signed(y) + i32::from(FOOTPRINT) - 1,
        })
        .collect();
    for &(x, y, kind) in &generated.blockers {
        if footprints.iter().any(|f| f.contains(signed(x), signed(y))) {
            continue;
        }
        let cell = &mut cells[usize::try_from(y * size + x).expect("invariant: small map")];
        match kind {
            0 => cell.flags = CellFlags::empty(),
            // Map validation requires abyss to be untraversable.
            1 => {
                cell.terrain = TerrainKind::Abyss;
                cell.flags = CellFlags::empty();
            }
            _ => cell.level = CliffLevel(1),
        }
    }
    let mut doc = MapDocument {
        metadata: MapMetadata {
            title: "Placement properties".to_string(),
            author: "scenario_placement_properties".to_string(),
            description: "Generated".to_string(),
            theme: "orbital_foundry".to_string(),
        },
        width: size,
        height: size,
        cells,
        resources: Vec::new(),
        watchtowers: Vec::new(),
        start_locations: Vec::new(),
    };
    for (slot, &cell) in generated.starts.iter().enumerate() {
        let id = doc.next_entity_id();
        doc.start_locations.push(MapStartLocation {
            id,
            player_slot: u8::try_from(slot).expect("invariant: two slots"),
            cell,
            footprint: (FOOTPRINT, FOOTPRINT),
        });
    }
    for &cell in &generated.resources {
        let id: MapEntityId = doc.next_entity_id();
        doc.resources.push(MapResource {
            id,
            kind: ResourceKind::Mineral,
            cell,
            amount: 1500,
        });
    }
    doc
}

/// Two starts at legal separation: their top-left cells differ by at least 16 on one axis (the
/// footprints are equal, so that is the Chebyshev distance of their centres), with the axis and
/// the slot order drawn too.
fn starts(span: u32) -> impl Strategy<Value = [(u32, u32); 2]> {
    let separation = sim_map::MIN_START_SEPARATION_CELLS;
    (
        0..=span - separation,
        0..=span,
        0..=span,
        any::<bool>(),
        any::<bool>(),
    )
        .prop_flat_map(move |(near, y0, y1, transpose, swap)| {
            (near + separation..=span).prop_map(move |far| {
                let (a, b) = if transpose {
                    ((y0, near), (y1, far))
                } else {
                    ((near, y0), (far, y1))
                };
                if swap {
                    [b, a]
                } else {
                    [a, b]
                }
            })
        })
}

fn generated() -> impl Strategy<Value = Generated> {
    prop_oneof![2 => Just(32u32), 1 => Just(48u32)].prop_flat_map(|size| {
        let span = size - u32::from(FOOTPRINT);
        (
            Just(size),
            starts(span),
            prop::collection::vec((0..size, 0..size, 0u8..3), 0..40),
            prop::collection::vec((0..size, 0..size), 0..4),
        )
            .prop_map(|(size, starts, blockers, resources)| Generated {
                size,
                starts,
                blockers,
                resources,
            })
    })
}

/// Builds with `counts`, reducing a player's count to what the builder reports it placed until
/// the build succeeds. Checks P4's error half on the way and returns the final counts and world.
fn build_reducing(
    roster: &Roster,
    doc: &MapDocument,
    mut counts: [usize; 2],
) -> ([usize; 2], [bool; 2], SimWorld, [StartLoadout; 2]) {
    let mut reduced = [false; 2];
    loop {
        match build(roster, doc, counts) {
            (Ok(world), loadouts) => return (counts, reduced, world, loadouts),
            (
                Err(ScenarioError::NotEnoughSpawnCells {
                    slot,
                    requested,
                    placed,
                }),
                _,
            ) => {
                let index = usize::from(slot);
                assert!(!reduced[index], "slot {slot} failed twice");
                assert_eq!(requested, counts[index], "P4: requested");
                assert!(placed < requested, "P4: placed < requested");
                counts[index] = placed;
                reduced[index] = true;
            }
            (Err(other), _) => panic!("unexpected build failure: {other}"),
        }
    }
}

/// Units per player: mostly 0 to 250 as the plan asks, otherwise 850 to 1,100, which exhausts
/// the hall's region on a 32x32 map and so exercises P4's failure half.
fn unit_count() -> impl Strategy<Value = usize> {
    prop_oneof![2 => 0usize..=250, 1 => 850usize..=1100]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn placement_keeps_the_oracle_and_fills_outer_rings_validly(
        generated in generated(),
        n0 in unit_count(),
        n1 in unit_count(),
        k in 1usize..=30,
    ) {
        let doc = document(&generated);
        prop_assume!(validate_map(&doc).errors.is_empty());
        let roster = roster();

        let (counts, reduced, world, loadouts) = build_reducing(&roster, &doc, [n0, n1]);
        for slot in 0..2u8 {
            let index = usize::from(slot);
            check_placement(&world, &doc, slot, &expanded(&world, &loadouts[index]));
            // P4: a reduced player leaves nothing eligible free.
            if reduced[index] {
                check_exhausted(&world, &doc, slot);
            }
        }

        // P6: determinism.
        let (again, _) = build(&roster, &doc, counts);
        prop_assert_eq!(again.expect("the same input builds").last_checksum, world.last_checksum);

        // P5: prefix, for each player growing alone (slot 0 never depends on slot 1, and slot 1
        // is placed after slot 0, so growing slot 1 keeps slot 0's placement fixed).
        let grown_one = build(&roster, &doc, [counts[0], counts[1] + k]).0;
        if let Ok(bigger) = grown_one {
            let small = units_of(&world, 1);
            prop_assert_eq!(&units_of(&bigger, 1)[..small.len()], &small[..]);
            prop_assert_eq!(units_of(&bigger, 0), units_of(&world, 0));
        }
        let grown_zero = build(&roster, &doc, [counts[0] + k, 0]).0;
        if let Ok(bigger) = grown_zero {
            let small = units_of(&world, 0);
            prop_assert_eq!(&units_of(&bigger, 0)[..small.len()], &small[..]);
        }
    }
}

fn assets_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Every shipped scenario, built from the real assets root by the production loader, places
/// each player's units exactly where the three-ring oracle does (plan §4.3).
#[test]
fn shipped_scenarios_match_the_three_ring_oracle() {
    let assets = assets_root();
    let mut scenarios: Vec<PathBuf> = std::fs::read_dir(assets.join("scenarios"))
        .expect("assets/scenarios is readable")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ron"))
        .collect();
    scenarios.sort();
    assert!(!scenarios.is_empty(), "assets/scenarios holds scenarios");
    for path in scenarios {
        let name = path.file_name().expect("a file name");
        let relative = Path::new("scenarios").join(name);
        let world = load_scenario_world(&assets, &relative)
            .unwrap_or_else(|error| panic!("{}: {error}", relative.display()));
        for slot in 0..8u8 {
            let Some((hall, level)) = hall_of(&world, slot) else {
                continue;
            };
            let cells: Vec<(i32, i32)> =
                units_of(&world, slot).into_iter().map(|(_, c)| c).collect();
            let oracle = oracle_cells(&world, hall, level);
            assert!(
                cells.len() <= oracle.len(),
                "{}: slot {slot} reaches beyond ring 3",
                relative.display()
            );
            assert_eq!(
                cells,
                oracle[..cells.len()],
                "{}: slot {slot}",
                relative.display()
            );
        }
    }
}
