//! Wall-overlap meter of the route trace (plan `2026-11-11-contact-and-missiles` WP2, D-8): how
//! far ground bodies' discs reach into walls during a battle, and how often a tick presses one
//! deeper.
//!
//! # Responsibility
//! After each tick, for every walking ground body, asks `Pathfinder::wall_contact` for the
//! nearest wall within the body's collision radius under its physical request (true terrain,
//! every structure, no vision) and records its depth, `radius − distance`. Per owner it counts
//! the unit-ticks with a depth above zero, the deepest overlap with its entity, tick and
//! position, and the unit-ticks on which a body's depth rose above its depth at the tick's start
//! by more than [`DEPTH_TOLERANCE`]. Each overlapping unit-tick is also listed for `overlap.csv`.
//!
//! # How it fits
//! A meter of [`super::route::run_route_trace`], beside the blocking meter. Bug B-113's fix (the
//! non-deepening contact rule) is judged by its third figure: no tick may press a body deeper.
//!
//! # Invariants
//! - The meter only reads positions; the query's flood buffer is the pathfinder's own scratch,
//!   which no simulation state depends on, so recording a battle does not change it.
//! - Depths are converted to `f64` for reporting only.

use sim_core::SimWorld;
use sim_map::PathRequestWithVision;
use sim_types::{EntityId, MoverType, SimVec2};
use std::collections::BTreeMap;

/// A rise in depth smaller than this, in world units (2⁻²⁰), is fixed-point rounding.
pub const DEPTH_TOLERANCE: f64 = 1.0 / 1_048_576.0;

/// One unit-tick on which a body's disc overlapped a wall.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlapSample {
    /// The tick just stepped.
    pub tick: u32,
    /// Raw entity ID.
    pub entity: u32,
    /// Owner slot.
    pub owner: u8,
    /// Centre, world units.
    pub position: (f64, f64),
    /// Overlap depth, world units: collision radius minus distance to the nearest wall.
    pub depth: f64,
    /// Whether the depth rose above the depth at the tick's start by more than the tolerance.
    pub rose: bool,
}

/// One owner's overlap figures over a run.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OverlapMetrics {
    /// Unit-ticks with a depth above zero.
    pub overlapping_ticks: u32,
    /// Unit-ticks on which a body's depth rose (see [`OverlapSample::rose`]).
    pub rising_ticks: u32,
    /// The deepest overlap: `(depth, entity, tick, position)`.
    pub deepest: Option<(f64, u32, u32, (f64, f64))>,
}

/// Accumulates wall overlap tick by tick.
#[derive(Debug, Default)]
pub struct OverlapMeter {
    /// Each ground body's depth after the last observed tick.
    last: BTreeMap<EntityId, f64>,
    /// Figures per owner slot.
    owners: BTreeMap<u8, OverlapMetrics>,
    /// Every overlapping unit-tick, in tick then entity order.
    samples: Vec<OverlapSample>,
}

impl OverlapMeter {
    /// An empty meter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Measures every walking ground body of `world` after tick `tick`.
    pub fn observe(&mut self, world: &mut SimWorld, tick: u32) {
        let mut bodies = Vec::new();
        for (id, entity) in &world.entities {
            if entity.is_immobile
                || !entity.is_alive()
                || entity.is_loaded_in_transport()
                || entity.displacement.is_some()
            {
                continue;
            }
            let Ok(arch) = world.catalog.get(entity.unit_type_id) else {
                continue;
            };
            if arch.mover_type == MoverType::Ground {
                bodies.push((*id, entity.owner.0, entity.position, arch.collision_radius));
            }
        }
        let mut seen = BTreeMap::new();
        for (id, owner, position, radius) in bodies {
            let request = PathRequestWithVision {
                start: position,
                goal: position,
                radius,
                vision: None,
                block_unexplored_ramps: false,
                known_structures: None,
            };
            let depth = world
                .pathfinder
                .wall_contact(&world.map, &request, position)
                .map_or(0.0, |contact| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "reporting only: the depth drives no simulation decision"
                    )]
                    let distance = (contact.distance_sq as f64).sqrt() / 4_294_967_296.0;
                    (f64::from(radius.to_f32()) - distance).max(0.0)
                });
            let before = self.last.get(&id).copied().unwrap_or(depth);
            seen.insert(id, depth);
            if depth <= 0.0 {
                continue;
            }
            let rose = depth > before + DEPTH_TOLERANCE;
            let point = point(position);
            let figures = self.owners.entry(owner).or_default();
            figures.overlapping_ticks += 1;
            figures.rising_ticks += u32::from(rose);
            if figures.deepest.is_none_or(|(deepest, ..)| depth > deepest) {
                figures.deepest = Some((depth, id.0, tick, point));
            }
            self.samples.push(OverlapSample {
                tick,
                entity: id.0,
                owner,
                position: point,
                depth,
                rose,
            });
        }
        self.last = seen;
    }

    /// The figures of owner slot `owner`.
    #[must_use]
    pub fn of_owner(&self, owner: u8) -> OverlapMetrics {
        self.owners.get(&owner).copied().unwrap_or_default()
    }

    /// Every overlapping unit-tick, in tick then entity order.
    #[must_use]
    pub fn samples(&self) -> &[OverlapSample] {
        &self.samples
    }
}

fn point(position: SimVec2) -> (f64, f64) {
    (
        f64::from(position.x.to_f32()),
        f64::from(position.y.to_f32()),
    )
}

/// `overlap.csv`: one row per overlapping unit-tick.
#[must_use]
pub fn overlap_csv(samples: &[OverlapSample]) -> String {
    let mut out = String::from("tick,entity,owner,x,y,depth,rose\n");
    for s in samples {
        out.extend([format!(
            "{},{},{},{:.4},{:.4},{:.6},{}\n",
            s.tick, s.entity, s.owner, s.position.0, s.position.1, s.depth, s.rose
        )]);
    }
    out
}

/// The summary table: one row per owner slot of `owners`.
#[must_use]
pub fn overlap_markdown(owners: &[(u8, OverlapMetrics)]) -> Vec<String> {
    let mut rows = vec![
        "| Owner | Overlapping unit-ticks | Deeper unit-ticks | Max depth (entity, tick, position) |"
            .to_owned(),
        "|---:|---:|---:|---|".to_owned(),
    ];
    for &(owner, figures) in owners {
        let deepest = figures.deepest.map_or_else(
            || "—".to_owned(),
            |(depth, entity, tick, (x, y))| {
                format!("{depth:.3} ({entity}, {tick}, ({x:.2}, {y:.2}))")
            },
        );
        rows.push(format!(
            "| {owner} | {} | {} | {deepest} |",
            figures.overlapping_ticks, figures.rising_ticks
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_csv_and_the_table_list_what_was_recorded() {
        let samples = vec![OverlapSample {
            tick: 7,
            entity: 3,
            owner: 1,
            position: (1.5, 2.25),
            depth: 0.075,
            rose: true,
        }];
        assert_eq!(
            overlap_csv(&samples),
            "tick,entity,owner,x,y,depth,rose\n7,3,1,1.5000,2.2500,0.075000,true\n"
        );
        let figures = OverlapMetrics {
            overlapping_ticks: 4,
            rising_ticks: 1,
            deepest: Some((0.075, 3, 7, (1.5, 2.25))),
        };
        let rows = overlap_markdown(&[(1, figures), (0, OverlapMetrics::default())]);
        assert_eq!(rows[2], "| 1 | 4 | 1 | 0.075 (3, 7, (1.50, 2.25)) |");
        assert_eq!(rows[3], "| 0 | 0 | 0 | — |");
    }
}
