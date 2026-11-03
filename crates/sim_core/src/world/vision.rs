//! Tick phase 3: watchtower control, fog of war and remembered structures.
//!
//! # Responsibility
//! Assigns watchtowers, keeps each player's [`sim_map::PlayerVisionMap`] in step with the sight
//! sources of this tick, and updates each player's memory of blocked structure cells.
//!
//! # How it fits
//! The third phase of `step_into`, after unit tasks and before target acquisition, which reads
//! the vision maps it leaves. Its bookkeeping is derived state: the checksum reads
//! `vision_maps` and `known_blocked_cells`, never the counted-source lists.
//!
//! # Invariants
//! - Incremental recasting leaves every counter and state exactly as a full recount would.
//! - Sight lists are sorted by ID, with watchtowers at the top of the ID range.

use crate::SimWorld;
use sim_map::{watchtower_source, CellFlags, VisionSource};
use sim_types::{EntityId, PerPlayer, PlayerId, PlayerSlot, SimVec2, VisibilityState};

/// One sight source already counted into a player's vision map.
#[derive(Clone, Copy)]
struct AppliedSight {
    id: EntityId,
    owner: PlayerSlot,
    source: VisionSource,
}

/// Derived vision bookkeeping; excluded from the checksum and rebuilt by a full recount.
///
/// Holds the sources already counted into `SimWorld::vision_maps` and the revisions they were
/// counted against, so phase 3 can recast only what changed since the previous tick.
#[derive(Default)]
pub(crate) struct VisionWorkspace {
    /// Ground units able to hold a watchtower this tick, as `(owner, position)`.
    watchtower_units: Vec<(PlayerId, SimVec2)>,
    /// This tick's sight sources, sorted by ID; reused across ticks.
    sights: Vec<AppliedSight>,
    /// Sources currently included in `vision_maps`, sorted by ID.
    counted: Vec<AppliedSight>,
    /// Map cell revision `counted` was cast on; `None` until the first count.
    counted_cell_revision: Option<u64>,
    /// Each vision map's recount revision when `counted` was last in step with it.
    counted_recounts: PerPlayer<u64>,
    /// Indices of the cells set in each player's `known_blocked_cells`.
    known_blocked_lists: PerPlayer<Vec<usize>>,
}

impl SimWorld {
    /// Assigns each watchtower from the ground units standing on the map this tick.
    ///
    /// Air units overfly towers, and structures and transported cargo do not occupy them.
    // PERFORMANCE RATIONALE:
    // Why: This runs every tick, and most maps have few towers while battles have many units.
    // What: Skip entirely on towerless maps; otherwise reuse one scratch buffer of positions.
    // Trade-off: One retained allocation in exchange for no per-tick heap use.
    fn update_watchtower_control(&mut self) {
        if self.map.watchtowers().is_empty() {
            return;
        }
        self.vision.watchtower_units.clear();
        for entity in self.entities.values() {
            if entity.is_loaded_in_transport() || !entity.is_completed {
                continue;
            }
            let occupies = self.catalog.get(entity.unit_type_id).is_ok_and(|arch| {
                matches!(
                    arch.mover_type,
                    sim_types::MoverType::Ground | sim_types::MoverType::CliffJumper
                ) && !arch.is_immobile_by_default()
            });
            if occupies {
                self.vision
                    .watchtower_units
                    .push((entity.owner, entity.position));
            }
        }
        self.map
            .update_watchtower_capture(&self.vision.watchtower_units);
    }

    /// Lists every sight source for this tick in ID order, watchtowers after all entities.
    fn collect_sights(&self, sights: &mut Vec<AppliedSight>) {
        sights.clear();
        for entity in self.entities.values() {
            if entity.is_loaded_in_transport() {
                continue;
            }
            let Some(owner) = entity.owner.slot() else {
                continue;
            };
            if let Ok(arch) = self.catalog.get(entity.unit_type_id) {
                sights.push(AppliedSight {
                    id: entity.id,
                    owner,
                    source: VisionSource {
                        pos: entity.position,
                        mover_type: arch.mover_type,
                        sight_radius: Self::sight_radius_from_upgrades(
                            &self.player_upgrades,
                            arch,
                            entity.owner,
                        ),
                        is_detector: arch.is_detector,
                    },
                });
            }
        }
        // Towers take the top of the ID range, in ascending order to keep the list sorted.
        for (index, tower) in self.map.watchtowers().iter().enumerate().rev() {
            let Some(owner) = tower.controlling_player.and_then(PlayerId::slot) else {
                continue;
            };
            let Ok(offset) = u32::try_from(index) else {
                continue;
            };
            sights.push(AppliedSight {
                id: EntityId(u32::MAX - offset),
                owner,
                source: watchtower_source(tower),
            });
        }
    }

    /// Whether two sources see exactly the same cells, so nothing need be recast.
    ///
    /// Height only varies inside a ramp cell, so elsewhere the cell stands in for the position.
    fn sees_the_same(&self, a: &VisionSource, b: &VisionSource) -> bool {
        if (a.mover_type, a.sight_radius, a.is_detector)
            != (b.mover_type, b.sight_radius, b.is_detector)
        {
            return false;
        }
        if a.pos == b.pos {
            return true;
        }
        let cell = self.map.world_to_cell(a.pos);
        cell.is_ok()
            && cell == self.map.world_to_cell(b.pos)
            && cell.is_ok_and(|(x, y)| self.map.cell(x, y).is_ok_and(|c| c.ramp.is_none()))
    }

    /// Phase 3: Update per-player Fog of War visibility maps and remembered obstacle perception.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    // PERFORMANCE RATIONALE:
    // Why: Casting every unit's sight each tick was the largest cost of a 400-unit battle, and
    // settling and resetting eight maps cost O(map area) even with no units at all.
    // What: A unit's sight only changes when it enters a new cell, about one tick in ten. Keep
    // the sources already counted and recast only those that changed, as a removal plus an
    // addition, settling just the cells they touch.
    // Trade-off: One retained list of sources, and a full recount whenever terrain or a map is
    // edited outside this function. Counters and states end exactly as a full recount leaves them.
    pub fn update_vision(&mut self) {
        self.update_watchtower_control();

        let mut sights = std::mem::take(&mut self.vision.sights);
        self.collect_sights(&mut sights);
        let mut counted = std::mem::take(&mut self.vision.counted);
        let in_step = self.vision.counted_cell_revision == Some(self.map.cell_revision())
            && PlayerSlot::ALL.iter().all(|&p| {
                self.vision_maps[p].recount_revision() == self.vision.counted_recounts[p]
            });
        if in_step {
            self.recast_changed_sights(&counted, &sights);
        } else {
            for vision in &mut self.vision_maps {
                vision.begin_tick_update();
            }
            for sight in &sights {
                self.vision_maps[sight.owner].apply_vision_source(&self.map, &sight.source);
            }
            for vision in &mut self.vision_maps {
                vision.end_tick_update();
            }
            self.vision.counted_cell_revision = Some(self.map.cell_revision());
            for p in PlayerSlot::ALL {
                self.vision.counted_recounts[p] = self.vision_maps[p].recount_revision();
            }
        }
        std::mem::swap(&mut counted, &mut sights);
        self.vision.counted = counted;
        self.vision.sights = sights;

        self.update_known_structures();
    }

    /// Walks the old and new ID-sorted source lists together, recasting only the differences.
    fn recast_changed_sights(&mut self, counted: &[AppliedSight], sights: &[AppliedSight]) {
        let (mut old, mut new) = (counted.iter().peekable(), sights.iter().peekable());
        loop {
            match (old.peek().copied(), new.peek().copied()) {
                (Some(gone), next) if next.is_none_or(|n| gone.id < n.id) => {
                    self.vision_maps[gone.owner].remove_vision_source(&self.map, &gone.source);
                    old.next();
                }
                (Some(before), Some(after)) if before.id == after.id => {
                    if before.owner != after.owner
                        || !self.sees_the_same(&before.source, &after.source)
                    {
                        self.vision_maps[before.owner]
                            .remove_vision_source(&self.map, &before.source);
                        self.vision_maps[after.owner].add_vision_source(&self.map, &after.source);
                    }
                    old.next();
                    new.next();
                }
                (_, Some(added)) => {
                    self.vision_maps[added.owner].add_vision_source(&self.map, &added.source);
                    new.next();
                }
                _ => return,
            }
        }
    }

    /// Updates each player's memory of structures from the cells it sees now.
    // PERFORMANCE RATIONALE:
    // Why: Scanning every visible cell of every player is O(map area) per tick, for a record that
    // can only change at a structure's footprint or at a cell already remembered as blocked.
    // What: Visit those two sets alone, tracked per player in `VisionWorkspace::known_blocked_lists`.
    // Trade-off: One retained index list per player, bounded by the structure cells on the map.
    fn update_known_structures(&mut self) {
        let width = self.map.width() as usize;
        let total_cells = width * self.map.height() as usize;
        let mut active_players = PerPlayer::<bool>::default();
        for ent in self.entities.values() {
            if let Some(active) = active_players.get_mut(ent.owner) {
                *active = true;
            }
        }
        for (p, &is_active) in active_players.enumerate() {
            if !is_active {
                continue;
            }
            if self.known_blocked_cells[p].len() != total_cells {
                self.known_blocked_cells[p].resize(total_cells, false);
            }
            for structure in self.map.structures().values() {
                let (left, top) = structure.top_left;
                for y in top..top + u32::from(structure.height) {
                    for x in left..left + u32::from(structure.width) {
                        let idx = y as usize * width + x as usize;
                        if self.vision_maps[p].visibility_at(x, y) == VisibilityState::Visible
                            && !self.known_blocked_cells[p][idx]
                        {
                            self.known_blocked_cells[p][idx] = true;
                            self.vision.known_blocked_lists[p].push(idx);
                            self.movement.recheck_routes[p] = true;
                        }
                    }
                }
            }
            let mut list = std::mem::take(&mut self.vision.known_blocked_lists[p]);
            list.retain(|&idx| {
                let (Ok(x), Ok(y)) = (u32::try_from(idx % width), u32::try_from(idx / width))
                else {
                    return false;
                };
                let stale = self.vision_maps[p].visibility_at(x, y) == VisibilityState::Visible
                    && self
                        .map
                        .cell(x, y)
                        .is_ok_and(|c| !c.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE));
                if stale {
                    self.known_blocked_cells[p][idx] = false;
                    self.movement.recheck_routes[p] = true;
                }
                !stale
            });
            self.vision.known_blocked_lists[p] = list;
        }
    }
}
