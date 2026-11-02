//! Writes what one player may know into a [`PlayerView`], and answers site questions for them.
//!
//! # Responsibility
//! The simulation's only viewer-scoped output. It decides what a player's interface may read:
//! fog policy and tech rules live here, where a headless test can reach them, rather than in a
//! client copy that would drift.
//!
//! # How it fits
//! The client writes a view once per executed tick and its HUD reads that record, never the
//! world. The placement ghost is the one exception: it calls [`SimWorld::preview_site`], because
//! its input is a cursor cell and not a tick. Both share the admission rules in
//! `production::construction`.
//!
//! # Invariants
//! - Both entry points take `&self`, so reading them cannot change a checksum.
//! - Hidden state does not reach the output: entities are filtered by owner, nodes by `Visible`
//!   cells, and a site is judged with [`Knowledge::Player`].
//! - A warmed [`PlayerView`] is refilled without allocating unless a list grows.
//! - A viewer outside the player slots gets an empty view and no panic.

use crate::production::construction::{BuildRejection, Knowledge};
use crate::production::{ProductionBlock, ProductionItem};
use crate::upgrades::OVERCLOCK_ABILITY_SLUG;
use crate::{Entity, SimWorld};
use sim_map::PlacementFailureReason;
use sim_types::{
    CarrierView, CommandOptionView, CommandSubject, PlayerId, PlayerStatusView, PlayerView,
    QueuedItem, QueuedJobView, ResourceNodeId, ShapeKind, SiteRefusal, StructureQueueView, Tick,
    UnitTypeId, VisibilityState, VisibleNodeView,
};

impl SimWorld {
    /// Fills `out` with what `viewer` may know. Reads no hidden state and allocates only on growth.
    ///
    /// Everything in `out` is replaced. A viewer outside the player slots yields an empty view.
    pub fn write_player_view(&self, viewer: PlayerId, out: &mut PlayerView) {
        out.clear();
        out.tick = Tick(self.current_tick);
        out.viewer = viewer;
        let slot = usize::from(viewer.0);
        let (Some(resources), Some(vision)) =
            (self.player_resources.get(slot), self.vision_maps.get(slot))
        else {
            return;
        };
        out.status = PlayerStatusView {
            minerals: resources.minerals,
            gas: resources.gas,
            supply_used: resources.supply_used,
            supply_max: resources.supply_max,
            defeated: self.match_state.defeated & (1 << slot) != 0,
        };
        out.map_width = self.map.width();
        out.map_height = self.map.height();
        out.visibility.extend_from_slice(vision.states());

        for entity in self.entities.values().filter(|entity| {
            entity.owner == viewer && entity.is_alive() && !entity.is_loaded_in_transport()
        }) {
            self.record_entity(entity, out);
        }
        out.completed_types.sort_unstable();
        out.completed_types.dedup();
        self.record_options(out);
        for (node, index) in self.map.resources().iter().zip(0_u32..) {
            if vision.visibility_at(node.cell.0, node.cell.1) == VisibilityState::Visible {
                out.visible_nodes.push(VisibleNodeView {
                    id: ResourceNodeId(index),
                    remaining: node.remaining,
                });
            }
        }
    }

    /// Adds one of the viewer's entities to the per-entity lists of `out`.
    fn record_entity(&self, entity: &Entity, out: &mut PlayerView) {
        if !entity.production_queue.is_empty() {
            let first_job = u32::try_from(out.jobs.len()).unwrap_or(u32::MAX);
            out.jobs
                .extend(entity.production_queue.iter().map(|job| QueuedJobView {
                    item: match job.item {
                        ProductionItem::Unit(unit_type) => QueuedItem::Unit(unit_type),
                        ProductionItem::Research(upgrade) => QueuedItem::Research(upgrade),
                    },
                    remaining_ticks: job.remaining_ticks,
                    total_ticks: job.total_ticks,
                }));
            out.queues.push(StructureQueueView {
                entity: entity.id,
                first_job,
                job_count: u32::try_from(entity.production_queue.len()).unwrap_or(u32::MAX),
                exit_blocked: self.production_blocks().iter().any(|block| {
                    matches!(block, ProductionBlock::ExitObstructed { producer }
                        if *producer == entity.id)
                }),
            });
        }
        if entity.cargo.is_some() {
            out.laden.push(entity.id);
        }
        if let Ok(arch) = self.catalog.get(entity.unit_type_id) {
            let capacity = arch.capabilities.transport_capacity;
            if capacity > 0 {
                out.carriers.push(CarrierView {
                    entity: entity.id,
                    slots_used: self.carrier_slots_used(entity.id),
                    capacity,
                });
            }
        }
        if entity.is_completed {
            out.completed_types.push(entity.unit_type_id);
        }
    }

    /// Derives `options` and `overclock_ready` from the sorted `completed_types` of `out`.
    // PERFORMANCE RATIONALE:
    // Why: The writer takes `&self` so that reading cannot touch authoritative state, which
    // rules out scratch buffers held in the world, and this runs once per executed tick.
    // What: `completed_types` already holds each type once, and the catalog is walked directly
    // for structures, so `options` needs no scratch list of its own.
    // Trade-off: O(types x catalog) over small sets, in exchange for no allocation when warm.
    fn record_options(&self, out: &mut PlayerView) {
        let PlayerView {
            viewer,
            completed_types,
            options,
            overclock_ready,
            ..
        } = out;
        for &unit_type in completed_types.iter() {
            let Ok(arch) = self.catalog.get(unit_type) else {
                continue;
            };
            let products = arch
                .trains_units
                .iter()
                .filter_map(|slug| self.catalog.get_by_slug(slug))
                .filter(|product| !matches!(product.shape, ShapeKind::BuildingDome { .. }));
            for product in products {
                options.push(CommandOptionView {
                    subject: CommandSubject::Train(product.unit_type_id),
                    lock: self.prerequisite_lock(&product.requires_structures, completed_types),
                    minerals: product.mineral_cost,
                    gas: product.gas_cost,
                    supply: product.supply_cost,
                });
            }
            for def in arch
                .researches_upgrades
                .iter()
                .filter_map(|slug| self.upgrade_catalog.get_by_slug(slug))
            {
                options.push(CommandOptionView {
                    subject: CommandSubject::Research(def.upgrade_id),
                    lock: self.upgrade_lock(*viewer, def, completed_types),
                    minerals: def.mineral_cost,
                    gas: def.gas_cost,
                    supply: 0,
                });
            }
            for structure in self.catalog.archetypes().filter(|s| arch.can_raise(s)) {
                options.push(CommandOptionView {
                    subject: CommandSubject::Raise(structure.unit_type_id),
                    lock: self.prerequisite_lock(&structure.requires_structures, completed_types),
                    minerals: structure.mineral_cost,
                    gas: structure.gas_cost,
                    supply: 0,
                });
            }
            if arch.overclock.is_some()
                && self.is_ability_unlocked(arch, *viewer, OVERCLOCK_ABILITY_SLUG)
            {
                overclock_ready.push(unit_type);
            }
        }
        options.sort_unstable_by_key(|option| option.subject);
        options.dedup_by_key(|option| option.subject);
    }

    /// Whether `viewer` may order `structure` at this minimum corner, by what `viewer` knows.
    ///
    /// Terrain and resource nodes are public. A structure counts only if `viewer` owns it or
    /// remembers it, so the answer can never reveal a hidden one. Money and prerequisites are
    /// not judged here: the command card shows those.
    ///
    /// `cell_x` and `cell_y` are grid cells.
    ///
    /// # Errors
    /// The [`SiteRefusal`] saying why the site cannot be ordered.
    pub fn preview_site(
        &self,
        viewer: PlayerId,
        structure: UnitTypeId,
        cell_x: u32,
        cell_y: u32,
    ) -> Result<(), SiteRefusal> {
        if usize::from(viewer.0) >= self.player_resources.len() {
            return Err(SiteRefusal::InvalidRequest);
        }
        let site = self
            .site_of(structure, cell_x, cell_y)
            .map_err(|rejection| site_refusal(&rejection))?;
        let arch = self
            .catalog
            .get(structure)
            .map_err(|_| SiteRefusal::InvalidRequest)?;
        self.check_site(viewer, arch, site, Knowledge::Player)
            .map_err(|rejection| site_refusal(&rejection))
    }
}

/// The viewer's account of a rejection, without the occupant's identity.
fn site_refusal(rejection: &BuildRejection) -> SiteRefusal {
    match rejection {
        BuildRejection::OutOfBounds
        | BuildRejection::Blocked(PlacementFailureReason::OutOfBounds) => SiteRefusal::OutOfBounds,
        BuildRejection::Blocked(PlacementFailureReason::UnbuildableTerrain) => {
            SiteRefusal::UnbuildableTerrain
        }
        BuildRejection::Blocked(PlacementFailureReason::CliffLevelMismatch) => {
            SiteRefusal::CliffLevelMismatch
        }
        BuildRejection::Blocked(PlacementFailureReason::OverlapsRamp) => SiteRefusal::OverlapsRamp,
        BuildRejection::Blocked(PlacementFailureReason::OccupiedByStructure(_))
        | BuildRejection::RememberedStructure => SiteRefusal::KnownStructure,
        BuildRejection::CoversResource => SiteRefusal::CoversResource,
        BuildRejection::ExtractorNeedsGas => SiteRefusal::ExtractorNeedsGas,
        // Site checks raise none of these except the identity pair, which is the caller's error.
        BuildRejection::UnknownStructure(_)
        | BuildRejection::NotAStructure(_)
        | BuildRejection::MissingPrerequisite(_)
        | BuildRejection::Minerals { .. }
        | BuildRejection::Gas { .. }
        | BuildRejection::Invariant(_) => SiteRefusal::InvalidRequest,
    }
}
