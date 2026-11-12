//! # Outer-Ring Placement of Starting Units
//!
//! Owns where a player's starting units go once rings `d = 1, 2, 3` around its town hall are
//! full: on rings `d = 4, 5, …`, each in [`ring_perimeter`] order, until every unit is placed
//! or the hall's ground region is exhausted. Plan:
//! `docs/plans/2026-11-11-scenario-outer-rings.md` (owner's answer to Q0018, option (b)).
//!
//! ## How it fits
//! [`super::build_world_with_catalogs`] builds one [`OuterRingRules`] per build from the map
//! document. `place_ring_units` in the parent module places rings 1 to 3 with its own loop and
//! predicate *P* (in bounds, same cliff level as the hall's anchor, ground-pathable, not abyss,
//! not a ramp, no structure, not claimed) and calls [`place_outer_rings`] only when units remain.
//! Nothing here runs for a loadout that rings 1 to 3 hold.
//!
//! ## Invariants
//! - **Rings 1 to 3 are untouched.** This module never places a unit within Chebyshev distance
//!   [`INNER_RING_COUNT`] of the hall, so every placement on rings 1 to 3, and with it every
//!   existing scenario's checksum, is the one of the parent's unchanged loop (decision D-1).
//! - **Other starts' inner zones are reserved (D-2).** No outer-ring unit lands on another start
//!   location's footprint grown by [`INNER_RING_COUNT`] cells, so one player's outer rings never
//!   change another player's hall or rings 1 to 3, whatever the slot order.
//! - **The hall's ground region bounds the rings (D-3).** Outer-ring cells belong to the
//!   4-connected region of cells that satisfy the terrain part of *P* and are not reserved,
//!   seeded at the hall's edge, so units never cross a moat or cliff onto a separate plateau of
//!   the same level. The region depends on terrain and structures only, not on claims.
//! - **Points are skipped (D-4).** Outer-ring units never start on a resource node or watchtower
//!   cell.
//! - **Termination and no truncation (D-5).** The rings stop at the region's largest Chebyshev
//!   distance from the hall, at most the map's larger dimension. A loadout that does not fit is
//!   [`ScenarioError::NotEnoughSpawnCells`] with the counts; no world is returned.
//! - Integer cell arithmetic only; the region is a `Vec<bool>` and a `VecDeque`, built at most
//!   once per player at setup and never during a tick.

use std::collections::{BTreeSet, VecDeque};

use sim_map::{CellFlags, MapDocument, MapEntityId, TerrainKind};
use sim_types::{PlayerId, SimScalar, SimVec2, UnitTypeId};

use super::{ring_perimeter, PlacedHall, ScenarioError, INNER_RING_COUNT};
use crate::world::SimWorld;

/// An inclusive rectangle of cells in signed cell coordinates; it may extend past the map edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellRect {
    x_min: i32,
    x_max: i32,
    y_min: i32,
    y_max: i32,
}

impl CellRect {
    /// Whether cell `(x, y)` lies inside the rectangle, edges included.
    fn contains(self, x: i32, y: i32) -> bool {
        (self.x_min..=self.x_max).contains(&x) && (self.y_min..=self.y_max).contains(&y)
    }
}

/// Converts a cell coordinate or size to `i32`. Map dimensions are bounded by
/// `sim_map::MAX_MAP_DIMENSION`, so the saturating fallback is never reached on a valid map.
fn signed(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// The outer-ring rules that come from the map document and hold for every player of one build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OuterRingRules {
    /// Every start location's id with its footprint grown by [`INNER_RING_COUNT`] cells on each
    /// side, in ascending id order. A player's own entry is skipped when it is consulted.
    inner_zones: Vec<(MapEntityId, CellRect)>,
    /// Cells holding a resource node or a watchtower, `(x, y)`.
    point_cells: BTreeSet<(u32, u32)>,
}

impl OuterRingRules {
    /// Builds the rules of one build from `doc`'s start locations, resources and watchtowers.
    pub(super) fn from_document(doc: &MapDocument) -> Self {
        let mut inner_zones: Vec<(MapEntityId, CellRect)> = doc
            .start_locations
            .iter()
            .map(|start| {
                let (x, y) = (signed(start.cell.0), signed(start.cell.1));
                let (w, h) = (i32::from(start.footprint.0), i32::from(start.footprint.1));
                let zone = CellRect {
                    x_min: x - INNER_RING_COUNT,
                    x_max: x + w - 1 + INNER_RING_COUNT,
                    y_min: y - INNER_RING_COUNT,
                    y_max: y + h - 1 + INNER_RING_COUNT,
                };
                (start.id, zone)
            })
            .collect();
        inner_zones.sort_by_key(|(id, _)| *id);
        let point_cells = doc
            .resources
            .iter()
            .map(|resource| resource.cell)
            .chain(doc.watchtowers.iter().map(|tower| tower.cell))
            .collect();
        Self {
            inner_zones,
            point_cells,
        }
    }

    /// Whether `(x, y)` lies in the inner zone of a start location other than `own_start`.
    fn reserved(&self, own_start: MapEntityId, x: i32, y: i32) -> bool {
        self.inner_zones
            .iter()
            .any(|(id, zone)| *id != own_start && zone.contains(x, y))
    }
}

/// Chebyshev distance from cell `(x, y)` to the hall's rectangle; 0 inside it.
fn ring_of(hall: &PlacedHall, x: i32, y: i32) -> i32 {
    (hall.x_min - x)
        .max(x - hall.x_max)
        .max(hall.y_min - y)
        .max(y - hall.y_max)
        .max(0)
}

/// The hall's ground region (D-3): which cells belong to it and how far it reaches.
struct HallRegion {
    /// Map width in cells; row stride of `member`.
    width: u32,
    /// Map height in cells.
    height: u32,
    /// `member[y * width + x]` is true for the cells of the region.
    member: Vec<bool>,
    /// Largest Chebyshev distance from the hall's rectangle to a cell of the region; 0 when the
    /// region is empty.
    max_ring: i32,
}

impl HallRegion {
    /// Index of `(x, y)` in `member`, or `None` off the map.
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let ux = u32::try_from(x).ok().filter(|&ux| ux < self.width)?;
        let uy = u32::try_from(y).ok().filter(|&uy| uy < self.height)?;
        usize::try_from(u64::from(uy) * u64::from(self.width) + u64::from(ux)).ok()
    }

    /// The unsigned cell `(x, y)` when it belongs to the region.
    fn member_cell(&self, x: i32, y: i32) -> Option<(u32, u32)> {
        let index = self.index(x, y)?;
        if !self.member.get(index).copied().unwrap_or(false) {
            return None;
        }
        Some((u32::try_from(x).ok()?, u32::try_from(y).ok()?))
    }
}

/// Whether cell `(x, y)` may join the hall's region: on the map, on the hall's cliff level,
/// ground-pathable, not abyss, not a ramp, holding no structure, and not in another start's
/// inner zone (`hall.start` is the player's own). This is the terrain part of *P* plus D-2's reservation; claims are not consulted.
fn admits(world: &SimWorld, hall: &PlacedHall, rules: &OuterRingRules, x: i32, y: i32) -> bool {
    let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) else {
        return false;
    };
    let Ok(cell) = world.map.cell(ux, uy) else {
        return false;
    };
    cell.cliff_level == hall.cliff_level
        && cell.flags.contains(CellFlags::GROUND_PATHABLE)
        && cell.terrain_kind != TerrainKind::Abyss
        && cell.ramp.is_none()
        && !cell.flags.contains(CellFlags::RAMP)
        && cell.occupying_structure.is_none()
        && !rules.reserved(hall.start, x, y)
}

/// Breadth-first fill of the hall's ground region (D-3), seeded by every admitted cell
/// 4-adjacent to the hall's rectangle and grown through admitted cells in the fixed neighbour
/// order east, north, west, south.
fn hall_region(world: &SimWorld, hall: &PlacedHall, rules: &OuterRingRules) -> HallRegion {
    let width = world.map.width();
    let height = world.map.height();
    let cell_count = usize::try_from(u64::from(width) * u64::from(height)).unwrap_or(0);
    let mut region = HallRegion {
        width,
        height,
        member: vec![false; cell_count],
        max_ring: 0,
    };
    let mut queue = VecDeque::new();
    let visit = |region: &mut HallRegion, queue: &mut VecDeque<(i32, i32)>, x: i32, y: i32| {
        let Some(index) = region.index(x, y) else {
            return;
        };
        if region.member[index] || !admits(world, hall, rules, x, y) {
            return;
        }
        region.member[index] = true;
        region.max_ring = region.max_ring.max(ring_of(hall, x, y));
        queue.push_back((x, y));
    };
    // Ring 1 without its four corners is exactly the cells 4-adjacent to the rectangle.
    for (x, y) in ring_perimeter(
        hall.x_min - 1,
        hall.x_max + 1,
        hall.y_min - 1,
        hall.y_max + 1,
    ) {
        let corner = (x == hall.x_min - 1 || x == hall.x_max + 1)
            && (y == hall.y_min - 1 || y == hall.y_max + 1);
        if !corner {
            visit(&mut region, &mut queue, x, y);
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            visit(&mut region, &mut queue, x + dx, y + dy);
        }
    }
    region
}

/// Places `units[placed..]` for player `slot` on rings `INNER_RING_COUNT + 1, …` around `hall`,
/// each ring in [`ring_perimeter`] order, on cells of the hall's ground region that hold no
/// resource node or watchtower and are not in `claimed_cells`. Units keep loadout order and are
/// spawned at the cell centre; every used cell is added to `claimed_cells`.
///
/// `placed` is the number of units rings 1 to 3 already placed. The inner zone of the hall's own
/// start location (`hall.start`) is not reserved against it.
///
/// # Errors
/// [`ScenarioError::NotEnoughSpawnCells`] with `requested = units.len()` and the number placed
/// when the region is exhausted, and [`ScenarioError::Spawn`] when a unit cannot be spawned.
pub(super) fn place_outer_rings(
    world: &mut SimWorld,
    slot: u8,
    hall: &PlacedHall,
    units: &[UnitTypeId],
    mut placed: usize,
    claimed_cells: &mut BTreeSet<(u32, u32)>,
    rules: &OuterRingRules,
) -> Result<(), ScenarioError> {
    let region = hall_region(world, hall, rules);
    for d in (INNER_RING_COUNT + 1)..=region.max_ring {
        let ring = ring_perimeter(
            hall.x_min - d,
            hall.x_max + d,
            hall.y_min - d,
            hall.y_max + d,
        );
        for (x, y) in ring {
            let Some(&unit_type) = units.get(placed) else {
                return Ok(());
            };
            let Some(cell) = region.member_cell(x, y) else {
                continue;
            };
            if rules.point_cells.contains(&cell) || claimed_cells.contains(&cell) {
                continue;
            }
            let pos = SimVec2::new(
                SimScalar::from_num(cell.0) + SimScalar::HALF,
                SimScalar::from_num(cell.1) + SimScalar::HALF,
            );
            world.spawn_unit(unit_type, PlayerId(slot), pos)?;
            claimed_cells.insert(cell);
            placed += 1;
        }
    }
    if placed < units.len() {
        return Err(ScenarioError::NotEnoughSpawnCells {
            slot,
            requested: units.len(),
            placed,
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeSet;

    use sim_map::{
        CellFlags, CliffLevel, MapCellSpec, MapDocument, MapEntityId, MapMetadata, MapResource,
        MapStartLocation, MapWatchtower, RampDir, ResourceKind, TerrainKind,
    };
    use sim_types::{PlayerId, ShapeKind, UnitTypeId};

    use super::super::{
        build_world_with_catalogs, place_ring_units, ring_perimeter, LoadoutUnit, PlacedHall,
        PlayerSetup, ScenarioError, StartLoadout, StartLoadoutCatalog,
    };
    use super::OuterRingRules;
    use crate::world::SimWorld;

    /// The hall of slot 0 on every map here: a 5x5 dome on the 5x5 start at `(6, 6)`.
    const HALL: (i32, i32, i32, i32) = (6, 10, 6, 10);

    /// A 5x5 dome and two mobile archetypes of the shipped catalog, chosen by shape.
    struct Roster {
        hall: String,
        hall_type: UnitTypeId,
        first: String,
        second: String,
        first_type: UnitTypeId,
        second_type: UnitTypeId,
    }

    fn roster() -> Roster {
        let catalog = crate::test_support::shipped_catalogs().units;
        let hall = catalog
            .archetypes()
            .find(|a| {
                matches!(
                    a.shape,
                    ShapeKind::BuildingDome {
                        footprint_w: 5,
                        footprint_h: 5
                    }
                )
            })
            .expect("invariant: the shipped catalog has a 5x5 dome");
        let mut mobile = catalog
            .archetypes()
            .filter(|a| matches!(a.shape, ShapeKind::UnitSphere));
        let first = mobile
            .next()
            .expect("invariant: the shipped catalog has mobile units");
        let second = mobile
            .next()
            .expect("invariant: the shipped catalog has two mobile units");
        Roster {
            hall: hall.slug.clone(),
            hall_type: hall.unit_type_id,
            first: first.slug.clone(),
            second: second.slug.clone(),
            first_type: first.unit_type_id,
            second_type: second.unit_type_id,
        }
    }

    /// A 32x32 open grass map with 5x5 starts for slots 0 and 1 at the given top-left cells.
    fn document(slot_one_at: (u32, u32)) -> MapDocument {
        let cell = MapCellSpec {
            level: CliffLevel(0),
            ramp: None,
            terrain: TerrainKind::Grass,
            flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
        };
        let start = |id: u32, player_slot: u8, at: (u32, u32)| MapStartLocation {
            id: MapEntityId(id),
            player_slot,
            cell: at,
            footprint: (5, 5),
        };
        MapDocument {
            metadata: MapMetadata {
                title: "Outer rings".to_string(),
                author: "placement tests".to_string(),
                description: "Open ground".to_string(),
                theme: "orbital_foundry".to_string(),
            },
            width: 32,
            height: 32,
            cells: vec![cell; 32 * 32],
            resources: Vec::new(),
            watchtowers: Vec::new(),
            start_locations: vec![start(1, 0, (6, 6)), start(2, 1, slot_one_at)],
        }
    }

    /// A loadout of `first` units of the roster's first archetype, then `second` of its second,
    /// in entries of at most 200.
    fn loadout(roster: &Roster, slug: &str, first: usize, second: usize) -> StartLoadout {
        let mut units = Vec::new();
        for (unit, mut left) in [(&roster.first, first), (&roster.second, second)] {
            while left > 0 {
                let count = left.min(200);
                units.push(LoadoutUnit {
                    slug: unit.clone(),
                    count: u8::try_from(count).expect("invariant: count is at most 200"),
                });
                left -= count;
            }
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

    /// Builds `doc` with each `(slot, units)` of `fielded` fielding that many first-archetype units.
    fn build(
        roster: &Roster,
        doc: &MapDocument,
        fielded: &[(u8, usize)],
    ) -> Result<SimWorld, ScenarioError> {
        let loadouts = StartLoadoutCatalog {
            loadouts: fielded
                .iter()
                .map(|&(slot, n)| loadout(roster, &format!("p{slot}"), n, 0))
                .collect(),
        };
        let players: Vec<PlayerSetup> = fielded
            .iter()
            .map(|&(slot, _)| PlayerSetup {
                player_slot: slot,
                loadout_slug: format!("p{slot}"),
            })
            .collect();
        let catalogs = crate::test_support::shipped_catalogs();
        build_world_with_catalogs(
            doc,
            &loadouts,
            &players,
            3,
            catalogs.units,
            catalogs.upgrades,
        )
    }

    /// The units of `slot` in spawn order, as `(archetype, cell)`.
    fn units_of(world: &SimWorld, slot: u8) -> Vec<(UnitTypeId, (i32, i32))> {
        world
            .entities
            .values()
            .filter(|e| e.owner == PlayerId(slot) && !world.map.structures().contains_key(&e.id))
            .map(|e| {
                let x = i32::try_from(e.position.x.saturating_to_u32()).unwrap_or(i32::MAX);
                let y = i32::try_from(e.position.y.saturating_to_u32()).unwrap_or(i32::MAX);
                (e.unit_type_id, (x, y))
            })
            .collect()
    }

    fn cells_of(world: &SimWorld, slot: u8) -> Vec<(i32, i32)> {
        units_of(world, slot).into_iter().map(|(_, c)| c).collect()
    }

    /// Cells of rings `rings` around slot 0's hall in placement order.
    fn rings(rings: std::ops::RangeInclusive<i32>) -> Vec<(i32, i32)> {
        let (x0, x1, y0, y1) = HALL;
        rings
            .flat_map(|d| ring_perimeter(x0 - d, x1 + d, y0 - d, y1 + d))
            .collect()
    }

    fn ring_of(cell: (i32, i32)) -> i32 {
        let (x0, x1, y0, y1) = HALL;
        (x0 - cell.0)
            .max(cell.0 - x1)
            .max(y0 - cell.1)
            .max(cell.1 - y1)
            .max(0)
    }

    /// Slot 0's hall alone on [`document`], with no unit, ready for a direct call of
    /// `place_ring_units` after the test has changed the map.
    fn bare_hall(roster: &Roster) -> (SimWorld, PlacedHall) {
        let world = build(roster, &document((24, 24)), &[(0, 0)])
            .expect("invariant: an open map with no unit builds");
        let (x_min, x_max, y_min, y_max) = HALL;
        let hall = PlacedHall {
            x_min,
            x_max,
            y_min,
            y_max,
            cliff_level: CliffLevel(0),
            start: MapEntityId(1),
        };
        (world, hall)
    }

    fn cell_mut(world: &mut SimWorld, x: u32, y: u32) -> &mut sim_map::MapCell {
        world
            .map
            .cell_mut(x, y)
            .expect("invariant: test cells are on the map")
    }

    #[test]
    fn units_beyond_ring_three_continue_on_ring_four_in_perimeter_order(
    ) -> Result<(), ScenarioError> {
        let roster = roster();
        let loadouts = StartLoadoutCatalog {
            loadouts: vec![loadout(&roster, "p0", 98, 2)],
        };
        let players = [PlayerSetup {
            player_slot: 0,
            loadout_slug: "p0".to_string(),
        }];
        let catalogs = crate::test_support::shipped_catalogs();
        let world = build_world_with_catalogs(
            &document((24, 24)),
            &loadouts,
            &players,
            3,
            catalogs.units,
            catalogs.upgrades,
        )?;
        let expected: Vec<(UnitTypeId, (i32, i32))> = rings(1..=4)
            .into_iter()
            .take(100)
            .enumerate()
            .map(|(index, cell)| {
                let unit = if index < 98 {
                    roster.first_type
                } else {
                    roster.second_type
                };
                (unit, cell)
            })
            .collect();
        let placed = units_of(&world, 0);
        assert_eq!(placed, expected);
        assert_eq!(rings(1..=3).len(), 96, "rings 1 to 3 of a 5x5 hall");
        assert_eq!(
            placed[96..].iter().map(|(_, c)| *c).collect::<Vec<_>>(),
            vec![(2, 2), (3, 2), (4, 2), (5, 2)]
        );
        Ok(())
    }

    #[test]
    fn outer_rings_skip_blocked_cells() -> Result<(), ScenarioError> {
        let roster = roster();
        let (mut world, hall) = bare_hall(&roster);
        // Ring 4's south edge is y = 2, x = 2..=14; its east edge is x = 14, y = 3..=14.
        cell_mut(&mut world, 3, 2).flags = CellFlags::empty();
        cell_mut(&mut world, 4, 2).terrain_kind = TerrainKind::Abyss;
        cell_mut(&mut world, 5, 2).flags |= CellFlags::RAMP;
        cell_mut(&mut world, 6, 2).ramp = Some(RampDir::North);
        cell_mut(&mut world, 7, 2).cliff_level = CliffLevel(1);
        // A structure covering x = 14..=18, y = 6..=10: ring 4's east edge from y = 6 to 10.
        world.spawn_structure(roster.hall_type, PlayerId(1), 14, 6)?;
        let mut doc = document((24, 24));
        doc.resources.push(MapResource {
            id: MapEntityId(3),
            kind: ResourceKind::Mineral,
            cell: (8, 2),
            amount: 1500,
        });
        doc.watchtowers.push(MapWatchtower {
            id: MapEntityId(4),
            cell: (9, 2),
            capture_radius_tenths: 30,
            reveal_radius_tenths: 120,
        });
        let rules = OuterRingRules::from_document(&doc);
        let units = vec![roster.first_type; 106];
        let mut claimed = BTreeSet::new();
        place_ring_units(&mut world, 0, &hall, &units, &mut claimed, &rules)?;

        let cells = cells_of(&world, 0);
        assert_eq!(cells[..96], rings(1..=3)[..], "rings 1 to 3 are unaffected");
        assert_eq!(
            cells[96..],
            [
                (2, 2),
                (10, 2),
                (11, 2),
                (12, 2),
                (13, 2),
                (14, 2),
                (14, 3),
                (14, 4),
                (14, 5),
                (14, 11),
            ]
        );
        Ok(())
    }

    #[test]
    fn outer_rings_never_enter_another_starts_inner_zone() -> Result<(), ScenarioError> {
        let roster = roster();
        // Slot 1's footprint x = 22..=26, y = 6..=10: 16 cells from slot 0's, the minimum.
        let doc = document((22, 6));
        let zone = |(x, y): (i32, i32)| (19..=29).contains(&x) && (3..=13).contains(&y);

        let crowded = build(&roster, &doc, &[(0, 500), (1, 40)])?;
        let slot_zero = cells_of(&crowded, 0);
        assert_eq!(slot_zero.len(), 500);
        assert!(
            slot_zero.iter().any(|&c| ring_of(c) >= 9),
            "slot 0's rings reach past slot 1's zone edge"
        );
        assert!(
            !slot_zero.iter().copied().any(zone),
            "no slot 0 unit in slot 1's inner zone"
        );

        let sparse = build(&roster, &doc, &[(0, 96), (1, 40)])?;
        assert_eq!(cells_of(&crowded, 1), cells_of(&sparse, 1));
        Ok(())
    }

    /// Makes every cell at Chebyshev distance `d` from slot 0's hall non-pathable.
    fn moat(world: &mut SimWorld, d: i32) {
        let (x0, x1, y0, y1) = HALL;
        for (x, y) in ring_perimeter(x0 - d, x1 + d, y0 - d, y1 + d) {
            if let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) {
                cell_mut(world, ux, uy).flags = CellFlags::empty();
            }
        }
    }

    #[test]
    fn outer_rings_do_not_cross_onto_a_separate_plateau() -> Result<(), ScenarioError> {
        let roster = roster();
        let rules = OuterRingRules::from_document(&document((24, 24)));

        // Rings 1 to 4 hold 24 + 32 + 40 + 48 = 144 cells; ring 5 is a moat.
        let (mut world, hall) = bare_hall(&roster);
        moat(&mut world, 5);
        let units = vec![roster.first_type; 144];
        place_ring_units(&mut world, 0, &hall, &units, &mut BTreeSet::new(), &rules)?;
        let cells = cells_of(&world, 0);
        assert_eq!(cells, rings(1..=4));

        let (mut world, hall) = bare_hall(&roster);
        moat(&mut world, 5);
        let units = vec![roster.first_type; 150];
        let result = place_ring_units(&mut world, 0, &hall, &units, &mut BTreeSet::new(), &rules);
        assert!(matches!(
            result,
            Err(ScenarioError::NotEnoughSpawnCells {
                slot: 0,
                requested: 150,
                placed: 144
            })
        ));
        assert!(
            cells_of(&world, 0).iter().all(|&c| ring_of(c) <= 4),
            "no unit beyond the moat"
        );
        Ok(())
    }

    #[test]
    fn exhausted_region_is_a_typed_error() {
        let roster = roster();
        let rules = OuterRingRules::from_document(&document((24, 24)));
        let (mut world, hall) = bare_hall(&roster);
        let (x0, x1, y0, y1) = HALL;
        for (x, y) in ring_perimeter(x0 - 6, x1 + 6, y0 - 6, y1 + 6) {
            if let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) {
                cell_mut(&mut world, ux, uy).terrain_kind = TerrainKind::Abyss;
            }
        }
        // Three abyss cells inside the enclosure, on ring 5.
        for x in [3, 4, 5] {
            cell_mut(&mut world, x, 1).terrain_kind = TerrainKind::Abyss;
        }
        // Rings 1 to 5 hold 200 cells; three are abyss.
        let free = 200 - 3;
        let units = vec![roster.first_type; 250];
        let result = place_ring_units(&mut world, 0, &hall, &units, &mut BTreeSet::new(), &rules);
        assert!(
            matches!(
                result,
                Err(ScenarioError::NotEnoughSpawnCells {
                    slot: 0,
                    requested: 250,
                    placed,
                }) if placed == free
            ),
            "expected NotEnoughSpawnCells with {free} placed, got {result:?}"
        );
        assert_eq!(cells_of(&world, 0).len(), free);
    }

    #[test]
    fn two_hundred_units_fill_rings_one_to_five_exactly() -> Result<(), ScenarioError> {
        let roster = roster();
        let world = build(&roster, &document((24, 24)), &[(0, 200)])?;
        let cells = cells_of(&world, 0);
        let expected = rings(1..=5);
        assert_eq!(expected.len(), 200, "rings 1 to 5 of a 5x5 hall");
        assert_eq!(cells, expected);
        assert!(cells.iter().all(|&c| ring_of(c) <= 5));
        Ok(())
    }
}
