//! Authored placement rules that belong to no single archetype: `assets/catalog/placement_rules.ron`.
//!
//! # Responsibility
//! The file holds, per resource kind, the node's 5.0.14 footprint: its `Place` rows (the cells
//! the node takes, `x`, and the `NearResources` ring around them, `o`) with the anchor cell the map
//! file names, its pathing `Shape`, and whether it leaves the map when empty
//! (`FootprintMineralsRounded`, `FootprintGeyserRounded`; plans `2026-11-19-footprint-flags`
//! D-2 and `2026-11-20-resource-footprints` D-1). Each row also carries the resource's harvest
//! data from the same 5.0.14 `CBehaviorResource` record as its removal flag: how long a gather
//! takes, how much it yields and how long the worker waits at the node before it returns
//! (`HarvestTime`, `HarvestAmount`, `ReturnDelay`; plan `2026-11-22-harvest-timing` D-2).
//! Loading parses the rows once into sorted cell offsets ([`ResourceReach`], the cells on which
//! the node applies its flags: `x` and `o`), into a [`ResourceFootprints`] table (the `x`
//! rectangle, the shape in quarter cells and the removal flag) that `sim_map` places nodes by,
//! and into one [`ResourceHarvest`] per kind that the economy reads. The site check reads them
//! without allocating.
//!
//! # How it fits
//! [`RuntimeCatalogs::load_from_directory`](super::RuntimeCatalogs::load_from_directory) reads the
//! file beside `movement_rules.ron` and stores the result in the [`UnitCatalog`](super::UnitCatalog)
//! ([`UnitCatalog::placement_rules`](super::UnitCatalog::placement_rules)). A catalog built from
//! unit and structure text alone carries [`PlacementRules::NONE`], under which no resource applies
//! a flag and no kind has a footprint or a harvest. Like every catalog it is not hashed; lockstep
//! peers must load equal files.
//!
//! # Invariants
//! - Each resource kind has exactly one row in a loaded file, holding its node's own cells as one
//!   rectangle of at most 4 × 4 cells with the anchor among them.
//! - A reach applies at least one flag and never [`PlacementFlag::Fogged`], an engine state flag.
//! - Offsets are `(dx, dy)` in grid cells from the anchor cell, `dy` growing with world `y`,
//!   sorted by `dy` then `dx`, without repeats.
//! - A shape lies in the own cells, every value is a multiple of 1/4 world unit (exact in quarter
//!   cells), and an outline is simple with 3 to 16 vertices and edges along axes and diagonals.
//! - A harvest takes at least one tick and yields at least one unit, so a worker never loops
//!   without earning; its return delay may be zero. All three are whole ticks or units, read once.

use crate::error::{SimCoreError, SimResult};
use serde::Deserialize;
use sim_map::{ResourceFootprint, ResourceFootprints};
use sim_types::{
    FootprintShape, PlacementFlag, PlacementFlags, QuarterOutline, ResourceKind, SimScalar,
};

/// The largest side of an authored mask, in cells.
pub const MAX_REACH_SIDE: usize = 15;

/// The largest side of a node's own cells, in cells.
pub const MAX_RESOURCE_SIDE: usize = 4;

/// The cells around one kind of resource node on which it applies placement flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceReach {
    /// The kind of node this reach belongs to.
    pub kind: ResourceKind,
    /// The flags the node applies on every cell of the reach.
    pub applies: PlacementFlags,
    /// The reach's cells (the node's own and its ring) as `(dx, dy)` offsets in grid cells from
    /// the node's anchor cell, sorted by `dy` then `dx`; `(0, 0)` is always present.
    pub cells: Vec<(i8, i8)>,
}

impl ResourceReach {
    /// Whether the cell `(dx, dy)` grid cells from the node's anchor cell lies in this reach.
    #[must_use]
    pub fn covers(&self, dx: i64, dy: i64) -> bool {
        let (Ok(dx), Ok(dy)) = (i8::try_from(dx), i8::try_from(dy)) else {
            return false;
        };
        self.cells
            .binary_search_by(|&(x, y)| (y, x).cmp(&(dy, dx)))
            .is_ok()
    }
}

/// How one kind of resource is harvested (`StarCraft` II 5.0.14's `CBehaviorResource`; plan
/// `2026-11-22-harvest-timing` D-2). A worker's harvest of a node of the kind is these values
/// unchanged: the game's workers set no per-resource multiplier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceHarvest {
    /// Ticks a worker gathers at the node before it takes its load (`HarvestTime`); at least 1.
    pub time_ticks: u16,
    /// Resource units one gather takes from the node (`HarvestAmount`), or what is left if
    /// less; at least 1.
    pub amount: u32,
    /// Ticks a worker that has just taken its load waits at the node, holding it, before it
    /// walks back (`ReturnDelay`); 0 for none.
    pub return_delay_ticks: u16,
}

/// The placement rules loaded from `placement_rules.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlacementRules {
    reaches: Vec<ResourceReach>,
    footprints: ResourceFootprints,
    harvests: Vec<(ResourceKind, ResourceHarvest)>,
}

/// The file's top level.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlacementRulesFile {
    pub(super) resource_footprints: Vec<ResourceFootprintDef>,
}

/// One authored resource footprint.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceFootprintDef {
    /// The node kind the row belongs to.
    pub(super) kind: ResourceKind,
    /// The flags the node applies on its `x` and `o` cells.
    pub(super) applies: Vec<PlacementFlag>,
    /// `(column, row)` of the node's anchor cell in `rows`: the cell the map file names.
    pub(super) anchor: (u32, u32),
    /// The mask; `rows[0]` is the row of lowest `y`. `x` is an own cell, `o` a ring cell, `.`
    /// neither.
    pub(super) rows: Vec<String>,
    /// The pathing shape in world units from the centre of the `x` cells.
    pub(super) shape: ResourceShapeDef,
    /// Whether a node of the kind leaves the map when it empties.
    pub(super) remove_when_empty: bool,
    /// Ticks a gather takes (`HarvestTime`, converted at 25 ticks per Faster second).
    pub(super) harvest_time_ticks: u16,
    /// Units one gather yields (`HarvestAmount`).
    pub(super) harvest_amount: u32,
    /// Ticks the worker waits at the node with its load before returning (`ReturnDelay`).
    pub(super) return_delay_ticks: u16,
}

/// An authored resource shape, in world units from the centre of the node's own cells.
#[derive(Debug, Clone, Deserialize)]
pub(super) enum ResourceShapeDef {
    /// A box of half extents `half_width` by `half_height` with corners cut by `chamfer`.
    Box {
        /// Half the east-west extent.
        half_width: f64,
        /// Half the north-south extent.
        half_height: f64,
        /// How far each corner is cut along each side.
        chamfer: f64,
    },
    /// A closed polygon through these vertices `(x, y)`, in order round the outline.
    Outline(Vec<(f64, f64)>),
}

impl ResourceShapeDef {
    /// Every authored value, in the order written.
    pub(super) fn values(&self) -> Vec<f64> {
        match self {
            Self::Box {
                half_width,
                half_height,
                chamfer,
            } => vec![*half_width, *half_height, *chamfer],
            Self::Outline(vertices) => vertices.iter().flat_map(|&(x, y)| [x, y]).collect(),
        }
    }

    /// The shape in quarter cells, or `None` when a value is not finite, not a multiple of 1/4
    /// or out of range (a box value negative or above `u16`, an outline vertex beyond `i16`, more
    /// than 16 vertices).
    pub(super) fn resolve(&self) -> Option<FootprintShape> {
        let quarters = |value: f64| -> Option<i64> {
            let exact = value.is_finite() && (value * 4.0).fract() == 0.0 && value.abs() < 4096.0;
            exact.then(|| SimScalar::from_num(value).raw() >> 30)
        };
        match self {
            Self::Box {
                half_width,
                half_height,
                chamfer,
            } => {
                let unsigned = |value: f64| quarters(value).and_then(|q| u16::try_from(q).ok());
                Some(FootprintShape::Box {
                    half_width_q: unsigned(*half_width)?,
                    half_height_q: unsigned(*half_height)?,
                    chamfer_q: unsigned(*chamfer)?,
                })
            }
            Self::Outline(vertices) => {
                let signed = |value: f64| quarters(value).and_then(|q| i16::try_from(q).ok());
                let points = vertices
                    .iter()
                    .map(|&(x, y)| Some((signed(x)?, signed(y)?)))
                    .collect::<Option<Vec<_>>>()?;
                QuarterOutline::new(&points).map(FootprintShape::Outline)
            }
        }
    }
}

impl PlacementRules {
    /// No resource applies any flag and no kind has a footprint or a harvest: the rules of a
    /// catalog that has not loaded the file.
    pub const NONE: Self = Self {
        reaches: Vec::new(),
        footprints: ResourceFootprints::NONE,
        harvests: Vec::new(),
    };

    /// Parses and validates the text of `placement_rules.ron`.
    ///
    /// # Errors
    /// [`SimCoreError::RonDeserializationError`] for malformed RON, unknown or missing fields and
    /// unknown flag or kind names; [`SimCoreError::CatalogInvalidValue`] for a mask, shape, flag
    /// list or harvest that breaks the module's invariants, or a kind without a row.
    pub fn from_ron_str(ron_data: &str) -> SimResult<Self> {
        let file: PlacementRulesFile = ron::from_str(ron_data)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))?;
        file.validate()?;
        let reaches = file
            .resource_footprints
            .iter()
            .map(ResourceFootprintDef::to_reach)
            .collect::<SimResult<Vec<_>>>()?;
        let footprints = file
            .resource_footprints
            .iter()
            .map(|row| Ok((row.kind, row.to_footprint()?)))
            .collect::<SimResult<Vec<_>>>()?;
        let harvests = file
            .resource_footprints
            .iter()
            .map(|row| (row.kind, row.to_harvest()))
            .collect();
        Ok(Self {
            reaches,
            footprints: ResourceFootprints::new(footprints),
            harvests,
        })
    }

    /// The reach of nodes of `kind`, or `None` when that kind applies nothing.
    #[must_use]
    pub fn reach(&self, kind: ResourceKind) -> Option<&ResourceReach> {
        self.reaches.iter().find(|reach| reach.kind == kind)
    }

    /// Every authored reach, in the file's order.
    #[must_use]
    pub fn reaches(&self) -> &[ResourceReach] {
        &self.reaches
    }

    /// Every resource kind's footprint (plan `2026-11-20-resource-footprints` D-1): empty for
    /// [`Self::NONE`].
    #[must_use]
    pub const fn resource_footprints(&self) -> &ResourceFootprints {
        &self.footprints
    }

    /// How nodes of `kind` are harvested (plan `2026-11-22-harvest-timing` D-2), or `None` for
    /// [`Self::NONE`]. A loaded file has a row, and so a harvest, for every kind.
    #[must_use]
    pub fn harvest(&self, kind: ResourceKind) -> Option<&ResourceHarvest> {
        self.harvests
            .iter()
            .find(|(row_kind, _)| *row_kind == kind)
            .map(|(_, harvest)| harvest)
    }
}

impl ResourceFootprintDef {
    /// The row's harvest data, as authored; validation has checked the time and amount.
    const fn to_harvest(&self) -> ResourceHarvest {
        ResourceHarvest {
            time_ticks: self.harvest_time_ticks,
            amount: self.harvest_amount,
            return_delay_ticks: self.return_delay_ticks,
        }
    }

    /// The offset of mask position `at` from the anchor's `anchor`.
    fn offset(&self, at: usize, anchor: u32) -> SimResult<i8> {
        let at = i64::try_from(at).ok();
        at.and_then(|at| i8::try_from(at - i64::from(anchor)).ok())
            .ok_or_else(|| SimCoreError::CatalogInvalidValue {
                definition: format!("{:?}", self.kind),
                field: "rows".to_owned(),
                reason: "must be at most 15 cells a side",
            })
    }

    /// The `(column, row)` of every mask cell marked `mark`, in row-major order.
    pub(super) fn marked(&self, mark: char) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.rows.iter().enumerate().flat_map(move |(row, text)| {
            text.chars()
                .enumerate()
                .filter(move |&(_, at)| at == mark)
                .map(move |(column, _)| (column, row))
        })
    }

    /// The bounding rectangle `(column, row, width, height)` of the `x` cells, if any.
    pub(super) fn own_rectangle(&self) -> Option<(usize, usize, usize, usize)> {
        let mut own = self.marked('x');
        let (column, row) = own.next()?;
        let (mut low, mut high) = ((column, row), (column, row));
        for (column, row) in own {
            low = (low.0.min(column), low.1.min(row));
            high = (high.0.max(column), high.1.max(row));
        }
        Some((low.0, low.1, high.0 - low.0 + 1, high.1 - low.1 + 1))
    }

    /// The validated mask as sorted offsets from the anchor: its `x` and `o` cells.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] for an offset outside `i8`, which validation's bound
    /// of [`MAX_REACH_SIDE`] cells a side already excludes.
    fn to_reach(&self) -> SimResult<ResourceReach> {
        let mut cells = self
            .marked('x')
            .chain(self.marked('o'))
            .map(|(column, row)| {
                Ok((
                    self.offset(column, self.anchor.0)?,
                    self.offset(row, self.anchor.1)?,
                ))
            })
            .collect::<SimResult<Vec<_>>>()?;
        cells.sort_by_key(|&(dx, dy)| (dy, dx));
        Ok(ResourceReach {
            kind: self.kind,
            applies: PlacementFlags::of(&self.applies),
            cells,
        })
    }

    /// The validated row's footprint: the `x` rectangle from the anchor, the shape in quarter
    /// cells and the removal flag.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] for a row without own cells or a shape that does not
    /// resolve, both of which validation already refuses.
    fn to_footprint(&self) -> SimResult<ResourceFootprint> {
        let invalid = |field: &str, reason: &'static str| SimCoreError::CatalogInvalidValue {
            definition: format!("{:?}", self.kind),
            field: field.to_owned(),
            reason,
        };
        let (column, row, width, height) = self
            .own_rectangle()
            .ok_or_else(|| invalid("rows", "must hold the node's own cells, `x`"))?;
        let size = |cells: usize| {
            u8::try_from(cells).map_err(|_| invalid("rows", "own cells must be at most 4 a side"))
        };
        Ok(ResourceFootprint {
            cells: (
                self.offset(column, self.anchor.0)?,
                self.offset(row, self.anchor.1)?,
                size(width)?,
                size(height)?,
            ),
            shape: self
                .shape
                .resolve()
                .ok_or_else(|| invalid("shape", "must be exact in quarter cells"))?,
            remove_when_empty: self.remove_when_empty,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = include_str!("../../../../assets/catalog/placement_rules.ron");

    #[test]
    fn the_shipped_file_parses() -> SimResult<()> {
        let rules = PlacementRules::from_ron_str(SHIPPED)?;
        assert!(rules.reach(ResourceKind::Mineral).is_some());
        assert!(rules.reach(ResourceKind::Gas).is_some());
        Ok(())
    }

    /// A file with `mineral` as the mineral row and a one-cell gas row.
    fn with_mineral(mineral: &str) -> String {
        format!(
            r#"(resource_footprints: [
                {mineral},
                (kind: Gas, applies: [NearResources], anchor: (0, 0), rows: ["x"],
                    shape: Box(half_width: 0.5, half_height: 0.5, chamfer: 0.0),
                    remove_when_empty: false,
                    harvest_time_ticks: 35, harvest_amount: 4, return_delay_ticks: 0),
            ])"#
        )
    }

    /// A one-cell mineral row whose harvest fields are `harvest`.
    fn mineral_harvesting(harvest: &str) -> String {
        with_mineral(&format!(
            r#"(kind: Mineral, applies: [NearResources], anchor: (0, 0), rows: ["x"],
                shape: Box(half_width: 0.5, half_height: 0.5, chamfer: 0.0),
                remove_when_empty: true, {harvest})"#
        ))
    }

    /// Plan `2026-11-22-harvest-timing` D-2: each row's harvest is read as authored.
    #[test]
    fn the_harvest_is_the_rows() -> SimResult<()> {
        let rules = PlacementRules::from_ron_str(&mineral_harvesting(
            "harvest_time_ticks: 7, harvest_amount: 3, return_delay_ticks: 2",
        ))?;
        assert_eq!(
            rules.harvest(ResourceKind::Mineral),
            Some(&ResourceHarvest {
                time_ticks: 7,
                amount: 3,
                return_delay_ticks: 2
            })
        );
        assert_eq!(
            rules.harvest(ResourceKind::Gas),
            Some(&ResourceHarvest {
                time_ticks: 35,
                amount: 4,
                return_delay_ticks: 0
            })
        );
        Ok(())
    }

    /// The harvest fields are required: a row without one fails to parse, naming it.
    #[test]
    fn a_row_without_a_harvest_time_is_refused() {
        let result = PlacementRules::from_ron_str(&mineral_harvesting(
            "harvest_amount: 5, return_delay_ticks: 9",
        ));
        assert!(
            matches!(
                &result,
                Err(SimCoreError::RonDeserializationError(message))
                    if message.contains("harvest_time_ticks")
            ),
            "{result:?}"
        );
    }

    /// A gather of no time or no yield would loop forever without earning; no delay is allowed.
    #[test]
    fn a_harvest_of_no_time_or_no_amount_is_refused() {
        let cases = [
            (
                "harvest_time_ticks: 0, harvest_amount: 5, return_delay_ticks: 9",
                Some("harvest_time_ticks"),
            ),
            (
                "harvest_time_ticks: 50, harvest_amount: 0, return_delay_ticks: 9",
                Some("harvest_amount"),
            ),
            (
                "harvest_time_ticks: 1, harvest_amount: 1, return_delay_ticks: 0",
                None,
            ),
        ];
        for (harvest, refused) in cases {
            let result = PlacementRules::from_ron_str(&mineral_harvesting(harvest));
            match refused {
                Some(field) => assert!(
                    matches!(
                        &result,
                        Err(SimCoreError::CatalogInvalidValue { field: named, definition, .. })
                            if named == field && definition == "Mineral"
                    ),
                    "{harvest}: {result:?}"
                ),
                None => assert!(result.is_ok(), "{harvest}: {result:?}"),
            }
        }
    }

    #[test]
    fn offsets_are_measured_from_the_anchor() -> SimResult<()> {
        let rules = PlacementRules::from_ron_str(&with_mineral(
            r#"(kind: Mineral, applies: [NearResources], anchor: (1, 0), rows: ["xx.", ".o."],
                shape: Box(half_width: 1.0, half_height: 0.5, chamfer: 0.25),
                remove_when_empty: true,
                harvest_time_ticks: 50, harvest_amount: 5, return_delay_ticks: 9)"#,
        ))?;
        let reach = rules
            .reach(ResourceKind::Mineral)
            .ok_or(SimCoreError::IoError(
                "the mineral reach was loaded".to_owned(),
            ))?;
        assert_eq!(reach.cells, vec![(-1, 0), (0, 0), (0, 1)]);
        assert!(reach.covers(0, 1));
        assert!(!reach.covers(1, 0));
        assert!(!reach.covers(1_000, 0));
        assert_eq!(reach.applies, PlacementFlags::NEAR_RESOURCES);
        let footprint = rules
            .resource_footprints()
            .get(ResourceKind::Mineral)
            .ok_or(SimCoreError::IoError(
                "the mineral footprint was loaded".to_owned(),
            ))?;
        assert_eq!(footprint.cells, (-1, 0, 2, 1));
        assert_eq!(
            footprint.shape,
            FootprintShape::Box {
                half_width_q: 4,
                half_height_q: 2,
                chamfer_q: 1
            }
        );
        assert!(footprint.remove_when_empty);
        Ok(())
    }

    #[test]
    fn no_rules_apply_nothing() {
        assert_eq!(PlacementRules::NONE.reaches(), &[]);
        assert_eq!(
            PlacementRules::NONE.resource_footprints(),
            &ResourceFootprints::NONE
        );
        assert_eq!(PlacementRules::NONE.harvest(ResourceKind::Mineral), None);
        assert_eq!(PlacementRules::NONE.harvest(ResourceKind::Gas), None);
        assert_eq!(PlacementRules::default(), PlacementRules::NONE);
    }
}
