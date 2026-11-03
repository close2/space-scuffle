//! The state machine of orders that need a place on the map before they can be issued.
//!
//! # Responsibility
//! Owns [`CommandMode`], what a left click means while one is armed, and the arithmetic that turns
//! a click into a building site. It issues nothing itself:
//! [`collect_pointer_intent`](super::collect_pointer_intent) reads the gesture and submits the
//! order [`CommandMode::order_at`] names.
//!
//! # How it fits
//! The command card enters a mode. This module's input side has no key of its own, so a mode is
//! left only by finishing it, by the right button or [`CANCEL_KEY`](crate::input_bindings::CANCEL_KEY),
//! or when [`CommandMode::reconcile`] finds nothing left to carry it out.
//!
//! # Invariants
//! - `Normal` is the only mode that lets a left click select.
//! - A mode survives only while the match accepts orders and a selected unit can carry it out.
//! - A site is judged by [`SimWorld::preview_site`], so what a click reveals is what the player
//!   already knows. Only public facts, terrain and the position of nodes, steer the click.
//! - Nothing here allocates, so reconciling on every frame costs no heap.

use bevy::math::Vec3;
use bevy::prelude::Resource;
use sim_core::{SimWorld, UnitArchetype};
use sim_types::{
    EntityId, PlayerId, ResourceKind, ShapeKind, SimScalar, SimVec2, SiteRefusal, UnitOrder,
    UnitTypeId,
};

use super::context_order::sim_point;
use super::order_marker::OrderMarkerKind;
use crate::battle_pipeline::commands::{any_recipient, SubmissionError};

/// How far from a click an extractor looks for a gas node to cover, in cells on either axis.
pub const EXTRACTOR_SNAP_CELLS: u32 = 3;

/// What the next left click on the world means.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandMode {
    /// A left click selects.
    #[default]
    Normal,
    /// The next left click on the world orders an attack-move there.
    AttackMove,
    /// The next left click places this structure.
    Place(UnitTypeId),
}

impl CommandMode {
    /// Returns to `Normal` unless `owner` can still finish this mode.
    ///
    /// A missing `world` or `owner`, a closed match, and a selection with no capable member all
    /// end it. That covers a new session too, since replacing a session empties the selection.
    pub fn reconcile(
        &mut self,
        world: Option<&SimWorld>,
        owner: Option<PlayerId>,
        selection: &[EntityId],
    ) {
        let Some(order) = self.eligibility_order() else {
            return;
        };
        let viable = world.zip(owner).is_some_and(|(world, owner)| {
            orders_open(world) && any_recipient(world, owner, selection, &order)
        });
        if !viable {
            *self = Self::Normal;
        }
    }

    /// An order of this mode's kind, for asking who could carry it out. Its place is irrelevant.
    const fn eligibility_order(self) -> Option<UnitOrder> {
        match self {
            Self::Normal => None,
            Self::AttackMove => Some(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::ZERO, SimScalar::ZERO),
            }),
            Self::Place(structure_type) => Some(UnitOrder::Build {
                structure_type,
                cell_x: 0,
                cell_y: 0,
            }),
        }
    }

    /// The order a left click at `ground`, a Bevy world point, means for `owner` in this mode.
    ///
    /// Returns the order, how to mark it, and the Bevy world point the marker belongs at.
    ///
    /// # Errors
    /// [`SubmissionError::SiteRefused`] for a site that `owner` knows cannot take the structure,
    /// [`SubmissionError::InvalidStructure`] for a structure the catalog does not hold, and
    /// [`SubmissionError::UnsupportedOrder`] in `Normal`, which targets nothing.
    pub fn order_at(
        self,
        world: &SimWorld,
        owner: PlayerId,
        ground: Vec3,
    ) -> Result<(UnitOrder, OrderMarkerKind, Vec3), SubmissionError> {
        match self {
            Self::Normal => Err(SubmissionError::UnsupportedOrder),
            Self::AttackMove => {
                let target = sim_point(ground);
                Ok((
                    UnitOrder::AttackMove { target },
                    OrderMarkerKind::Attack,
                    ground,
                ))
            }
            Self::Place(structure_type) => {
                let structure = world
                    .catalog
                    .get(structure_type)
                    .map_err(|_| SubmissionError::InvalidStructure(structure_type))?;
                let (cell_x, cell_y) = placement_cell(world, structure, ground.x, ground.z)
                    .ok_or(SubmissionError::SiteRefused(SiteRefusal::OutOfBounds))?;
                world
                    .preview_site(owner, structure_type, cell_x, cell_y)
                    .map_err(SubmissionError::SiteRefused)?;
                let order = UnitOrder::Build {
                    structure_type,
                    cell_x,
                    cell_y,
                };
                let center = site_center(world, structure, (cell_x, cell_y)).unwrap_or(ground);
                Ok((order, OrderMarkerKind::Move, center))
            }
        }
    }
}

/// Whether the match still takes orders. Looking around outlives it.
#[must_use]
pub fn orders_open(world: &SimWorld) -> bool {
    !world.match_state.result.is_over()
}

/// The sides of `structure`'s footprint in cells, or `None` for a non-structure.
#[must_use]
pub const fn footprint(structure: &UnitArchetype) -> Option<(u32, u32)> {
    match structure.shape {
        ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
        } => Some((footprint_w as u32, footprint_h as u32)),
        ShapeKind::UnitSphere => None,
    }
}

/// The minimum corner, on one axis, of a footprint of `side` cells around `cell`, kept on the map.
///
/// An even side cannot centre on a cell, so it sits half a cell towards the higher coordinate.
const fn corner_around(cell: u32, side: u32, map_side: u32) -> u32 {
    let corner = cell.saturating_sub(side.saturating_sub(1) / 2);
    let last = map_side.saturating_sub(side);
    if corner < last {
        corner
    } else {
        last
    }
}

/// The minimum corner a `Place` click at world point `(x, z)` means, if on the map.
///
/// `x` and `z` are Bevy world units, which are map cells. Centres the footprint on the point, kept
/// on the map. An extractor (`capabilities.is_gas_extractor`) snaps to cover the nearest gas node
/// within [`EXTRACTOR_SNAP_CELLS`] of the point. Returns `None` for a point off the map or an
/// archetype with no footprint.
#[must_use]
pub fn placement_cell(
    world: &SimWorld,
    structure: &UnitArchetype,
    x: f32,
    z: f32,
) -> Option<(u32, u32)> {
    let (width, height) = footprint(structure)?;
    let clicked = world
        .map
        .world_to_cell(sim_point(Vec3::new(x, 0.0, z)))
        .ok()?;
    let center = if structure.capabilities.is_gas_extractor {
        nearest_gas_cell(world, clicked).unwrap_or(clicked)
    } else {
        clicked
    };
    Some((
        corner_around(center.0, width, world.map.width()),
        corner_around(center.1, height, world.map.height()),
    ))
}

/// The cell of the gas node nearest `cell`, if one lies within [`EXTRACTOR_SNAP_CELLS`].
fn nearest_gas_cell(world: &SimWorld, cell: (u32, u32)) -> Option<(u32, u32)> {
    world
        .map
        .resources()
        .iter()
        .filter(|node| node.kind == ResourceKind::Gas)
        .map(|node| {
            let reach = node
                .cell
                .0
                .abs_diff(cell.0)
                .max(node.cell.1.abs_diff(cell.1));
            (reach, node.cell)
        })
        .filter(|(reach, _)| *reach <= EXTRACTOR_SNAP_CELLS)
        .min_by_key(|(reach, _)| *reach)
        .map(|(_, cell)| cell)
}

/// The Bevy world point on the ground at the middle of a site with minimum corner `cell`.
#[must_use]
pub fn site_center(world: &SimWorld, structure: &UnitArchetype, cell: (u32, u32)) -> Option<Vec3> {
    let (width, height) = footprint(structure)?;
    let center = SimVec2::new(
        SimScalar::from_num(cell.0) + SimScalar::from_num(width) * SimScalar::HALF,
        SimScalar::from_num(cell.1) + SimScalar::from_num(height) * SimScalar::HALF,
    );
    let ground = world.map.terrain_altitude_scalar(center);
    Some(Vec3::new(
        center.x.to_f32(),
        ground.to_f32(),
        center.y.to_f32(),
    ))
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    reason = "cell numbers in these tests are tiny"
)]
mod tests {
    use super::*;
    use sim_map::{CliffLevel, GameMap, MapCell, ResourceNode};
    use sim_types::{MatchResult, Tick};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const ME: PlayerId = PlayerId(0);
    const SIDE: u32 = 64;
    const GAS_CELL: (u32, u32) = (30, 30);
    const MINERAL_CELL: (u32, u32) = (50, 12);

    fn node(kind: ResourceKind, cell: (u32, u32)) -> ResourceNode {
        ResourceNode {
            pos: SimVec2::new(
                SimScalar::from_num(cell.0) + SimScalar::HALF,
                SimScalar::from_num(cell.1) + SimScalar::HALF,
            ),
            kind,
            remaining: 100,
            cell,
        }
    }

    fn flat_world() -> Result<SimWorld, Box<dyn std::error::Error>> {
        let map = GameMap::from_cells(
            SIDE,
            SIDE,
            vec![MapCell::default_ground(CliffLevel(0)); (SIDE * SIDE) as usize],
            vec![
                node(ResourceKind::Gas, GAS_CELL),
                node(ResourceKind::Mineral, MINERAL_CELL),
            ],
            Vec::new(),
        )?;
        Ok(SimWorld::new(map, 13))
    }

    fn dome_where(
        world: &SimWorld,
        wanted: impl Fn(&UnitArchetype) -> bool,
    ) -> Result<UnitArchetype, Box<dyn std::error::Error>> {
        Ok(world
            .catalog
            .archetypes()
            .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }) && wanted(a))
            .ok_or("the catalog has no such structure")?
            .clone())
    }

    fn extractor(world: &SimWorld) -> Result<UnitArchetype, Box<dyn std::error::Error>> {
        dome_where(world, |a| a.capabilities.is_gas_extractor)
    }

    fn plain_dome(world: &SimWorld) -> Result<UnitArchetype, Box<dyn std::error::Error>> {
        dome_where(world, |a| !a.capabilities.is_gas_extractor)
    }

    /// A copy of `structure` with the given footprint.
    fn sized(structure: &UnitArchetype, width: u8, height: u8) -> UnitArchetype {
        let mut sized = structure.clone();
        sized.shape = ShapeKind::BuildingDome {
            footprint_w: width,
            footprint_h: height,
        };
        sized
    }

    /// A completed builder of `structure`, or any unit that can raise it.
    fn builder_for(
        world: &mut SimWorld,
        structure: &UnitArchetype,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let unit_type = world
            .catalog
            .archetypes()
            .find(|a| a.can_raise(structure) && a.max_speed > SimScalar::ZERO)
            .ok_or("nothing raises this structure")?
            .unit_type_id;
        let at = SimVec2::new(SimScalar::from_i32(8), SimScalar::from_i32(8));
        Ok(world.spawn_unit(unit_type, ME, at)?)
    }

    #[test]
    fn an_odd_footprint_centres_on_the_clicked_cell() -> TestResult {
        let world = flat_world()?;
        let dome = sized(&plain_dome(&world)?, 3, 3);
        assert_eq!(
            placement_cell(&world, &dome, 20.5, 40.25),
            Some((19, 39)),
            "cell (20, 40) sits in the middle of a 3x3 at (19, 39)"
        );
        Ok(())
    }

    #[test]
    fn an_even_footprint_contains_the_clicked_cell() -> TestResult {
        let world = flat_world()?;
        let dome = sized(&plain_dome(&world)?, 4, 2);
        assert_eq!(placement_cell(&world, &dome, 20.5, 40.5), Some((19, 40)));
        Ok(())
    }

    #[test]
    fn a_click_by_the_edge_keeps_the_footprint_on_the_map() -> TestResult {
        let world = flat_world()?;
        let dome = sized(&plain_dome(&world)?, 5, 5);
        assert_eq!(placement_cell(&world, &dome, 0.5, 0.5), Some((0, 0)));
        let far = SIDE as f32 - 0.5;
        assert_eq!(
            placement_cell(&world, &dome, far, far),
            Some((SIDE - 5, SIDE - 5))
        );
        Ok(())
    }

    #[test]
    fn a_point_off_the_map_has_no_cell() -> TestResult {
        let world = flat_world()?;
        let dome = plain_dome(&world)?;
        assert_eq!(placement_cell(&world, &dome, -0.5, 4.5), None);
        assert_eq!(placement_cell(&world, &dome, 4.5, SIDE as f32 + 1.0), None);
        Ok(())
    }

    #[test]
    fn something_with_no_footprint_cannot_be_placed() -> TestResult {
        let world = flat_world()?;
        let unit = world
            .catalog
            .archetypes()
            .find(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
            .ok_or("the catalog has no mobile unit")?;
        assert_eq!(placement_cell(&world, unit, 4.5, 4.5), None);
        Ok(())
    }

    #[test]
    fn an_extractor_snaps_to_cover_a_nearby_gas_node() -> TestResult {
        let world = flat_world()?;
        let extractor = sized(&extractor(&world)?, 3, 3);
        let reach = EXTRACTOR_SNAP_CELLS as f32;
        let (x, z) = (GAS_CELL.0 as f32 + 0.5 + reach, GAS_CELL.1 as f32 + 0.5);
        assert_eq!(
            placement_cell(&world, &extractor, x, z),
            Some((GAS_CELL.0 - 1, GAS_CELL.1 - 1)),
            "the node must end up under the middle of the footprint"
        );
        Ok(())
    }

    #[test]
    fn an_extractor_beyond_snap_range_stays_where_it_was_clicked() -> TestResult {
        let world = flat_world()?;
        let extractor = sized(&extractor(&world)?, 3, 3);
        let x = GAS_CELL.0 as f32 + 1.5 + EXTRACTOR_SNAP_CELLS as f32;
        let z = GAS_CELL.1 as f32 + 0.5;
        assert_eq!(
            placement_cell(&world, &extractor, x, z),
            Some((GAS_CELL.0 + EXTRACTOR_SNAP_CELLS, GAS_CELL.1 - 1))
        );
        Ok(())
    }

    #[test]
    fn only_an_extractor_snaps() -> TestResult {
        let world = flat_world()?;
        let dome = sized(&plain_dome(&world)?, 3, 3);
        let (x, z) = (GAS_CELL.0 as f32 + 2.5, GAS_CELL.1 as f32 + 0.5);
        assert_eq!(
            placement_cell(&world, &dome, x, z),
            Some((GAS_CELL.0 + 1, GAS_CELL.1 - 1))
        );
        Ok(())
    }

    #[test]
    fn a_mineral_node_does_not_attract_an_extractor() -> TestResult {
        let world = flat_world()?;
        let extractor = sized(&extractor(&world)?, 3, 3);
        let (x, z) = (MINERAL_CELL.0 as f32 + 0.5, MINERAL_CELL.1 as f32 + 1.5);
        assert_eq!(
            placement_cell(&world, &extractor, x, z),
            Some((MINERAL_CELL.0 - 1, MINERAL_CELL.1))
        );
        Ok(())
    }

    #[test]
    fn the_marker_of_a_placement_sits_on_the_middle_of_the_site() -> TestResult {
        let world = flat_world()?;
        let dome = plain_dome(&world)?;
        let (order, kind, at) = CommandMode::Place(dome.unit_type_id).order_at(
            &world,
            ME,
            Vec3::new(20.5, 0.0, 20.5),
        )?;
        assert_eq!(kind, OrderMarkerKind::Move);
        let UnitOrder::Build { cell_x, cell_y, .. } = order else {
            return Err("placing must build".into());
        };
        let (width, height) = footprint(&dome).ok_or("a dome has a footprint")?;
        assert_eq!(at.x, cell_x as f32 + width as f32 * 0.5);
        assert_eq!(at.z, cell_y as f32 + height as f32 * 0.5);
        Ok(())
    }

    #[test]
    fn a_site_on_a_node_is_refused_by_what_the_player_knows() -> TestResult {
        let world = flat_world()?;
        let dome = plain_dome(&world)?;
        let at = Vec3::new(
            MINERAL_CELL.0 as f32 + 0.5,
            0.0,
            MINERAL_CELL.1 as f32 + 0.5,
        );
        assert_eq!(
            CommandMode::Place(dome.unit_type_id).order_at(&world, ME, at),
            Err(SubmissionError::SiteRefused(SiteRefusal::CoversResource))
        );
        Ok(())
    }

    #[test]
    fn a_click_off_the_map_refuses_the_site_as_out_of_bounds() -> TestResult {
        let world = flat_world()?;
        let dome = plain_dome(&world)?;
        assert_eq!(
            CommandMode::Place(dome.unit_type_id).order_at(&world, ME, Vec3::new(-3.0, 0.0, 3.0)),
            Err(SubmissionError::SiteRefused(SiteRefusal::OutOfBounds))
        );
        Ok(())
    }

    #[test]
    fn an_unknown_structure_is_named_as_such() -> TestResult {
        let world = flat_world()?;
        let unknown = UnitTypeId(u16::MAX);
        assert_eq!(
            CommandMode::Place(unknown).order_at(&world, ME, Vec3::ZERO),
            Err(SubmissionError::InvalidStructure(unknown))
        );
        Ok(())
    }

    #[test]
    fn normal_targets_nothing() -> TestResult {
        let world = flat_world()?;
        assert_eq!(
            CommandMode::Normal.order_at(&world, ME, Vec3::ZERO),
            Err(SubmissionError::UnsupportedOrder)
        );
        Ok(())
    }

    #[test]
    fn a_mode_survives_while_a_selected_unit_can_carry_it_out() -> TestResult {
        let mut world = flat_world()?;
        let dome = plain_dome(&world)?;
        let builder = builder_for(&mut world, &dome)?;
        let mut mode = CommandMode::Place(dome.unit_type_id);

        mode.reconcile(Some(&world), Some(ME), &[builder]);
        assert_eq!(mode, CommandMode::Place(dome.unit_type_id));

        mode.reconcile(Some(&world), Some(ME), &[]);
        assert_eq!(mode, CommandMode::Normal);
        Ok(())
    }

    #[test]
    fn a_mode_ends_with_its_owner_its_world_or_its_match() -> TestResult {
        let mut world = flat_world()?;
        let dome = plain_dome(&world)?;
        let builder = builder_for(&mut world, &dome)?;
        let armed = CommandMode::Place(dome.unit_type_id);

        let mut mode = armed;
        mode.reconcile(Some(&world), None, &[builder]);
        assert_eq!(mode, CommandMode::Normal, "no controlled player");

        let mut mode = armed;
        mode.reconcile(None, Some(ME), &[builder]);
        assert_eq!(mode, CommandMode::Normal, "no world");

        let mut mode = armed;
        mode.reconcile(Some(&world), Some(PlayerId(1)), &[builder]);
        assert_eq!(mode, CommandMode::Normal, "a selection of someone else");

        world.match_state.result = MatchResult::Draw { tick: Tick(1) };
        let mut mode = armed;
        mode.reconcile(Some(&world), Some(ME), &[builder]);
        assert_eq!(mode, CommandMode::Normal, "a closed match");
        assert!(!orders_open(&world));
        Ok(())
    }

    #[test]
    fn normal_needs_nothing_to_stay() {
        let mut mode = CommandMode::Normal;
        mode.reconcile(None, None, &[]);
        assert_eq!(mode, CommandMode::Normal);
    }
}
