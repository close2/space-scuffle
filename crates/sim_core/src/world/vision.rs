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
//! The phase also counts down every entity's attack-reveal counters (plan
//! `2026-11-23-vision-and-detection` D-5) before anything reads vision: a reveal set by a strike
//! in phase 7 of tick t is seen by that tick's retaliation and by acquisition on the following
//! `reveal_ticks - 1` ticks, so it is in force for exactly `reveal_ticks` ticks.
//!
//! Once this tick's vision maps are final, the phase drops each player's attack snapshots that
//! player now sees again: its spot is `Visible`, or the attacker itself is shown (plan
//! `2026-11-24-attacker-snapshot` D-2). It also drops each snapshot whose strike is the catalog's
//! `attack_snapshot_cap_ticks` or more ticks old (plan Amendment 1, owner answer Q0112 2(c)): a
//! snapshot struck in phase 7 of tick t is listed after ticks t to t + cap - 1 and dropped on
//! tick t + cap. A strike of phase 7 records a snapshot on the same vision, so no snapshot is
//! ever recorded for an attacker this phase showed.
//!
//! # Invariants
//! - Incremental recasting leaves every counter and state exactly as a full recount would.
//! - Sight lists are sorted by ID, with watchtowers at the top of the ID range.

use crate::SimWorld;
use sim_map::{watchtower_source, CellFlags, VisionSource};
use sim_types::{EntityId, PerPlayer, PlayerId, PlayerSlot, SimScalar, SimVec2, VisibilityState};

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
    /// Air units overfly towers, and structures and hidden units (transported cargo, gatherers
    /// inside a structure) do not occupy them.
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
            if entity.is_hidden() || !entity.is_completed {
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

    /// Takes one tick off every entity's nonzero attack-reveal counters (plan
    /// `2026-11-23-vision-and-detection` D-5).
    fn count_down_attack_reveals(&mut self) {
        for entity in self.entities.values_mut() {
            for ticks in &mut entity.attack_reveal_ticks {
                *ticks = ticks.saturating_sub(1);
            }
        }
    }

    /// Lists every sight source for this tick in ID order, watchtowers after all entities. A
    /// hidden unit sees nothing: cargo, and a gatherer inside a structure (plan
    /// `2026-11-24-hidden-harvesters` D-6, Q0107's default (2a)).
    fn collect_sights(&self, sights: &mut Vec<AppliedSight>) {
        sights.clear();
        for entity in self.entities.values() {
            if entity.is_hidden() {
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
                        // A detector detects only once its construction completes (owner
                        // answer Q0101 3(b); 5.0.14 hides the `Detector` button until then,
                        // `Requirements="NotUnderConstruction"`; plan
                        // `2026-11-24-owner-answers-cliff-and-vision` D-5, B-184); its sight is
                        // unchanged meanwhile. The radius changes at completion, so
                        // `sees_the_same` recasts the source on that tick. A status may
                        // suppress detection (plan `2026-11-10-spellcaster-mechanics` §6.4).
                        // Sight upgrades leave the detection radius as authored (plan
                        // `2026-11-23-vision-and-detection` D-2).
                        detection_radius: arch
                            .detection_radius
                            .filter(|_| {
                                entity.is_completed && !entity.status_modifiers().suppress_detection
                            })
                            .unwrap_or(SimScalar::ZERO),
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
        if (a.mover_type, a.sight_radius, a.detection_radius)
            != (b.mover_type, b.sight_radius, b.detection_radius)
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
        self.count_down_attack_reveals();
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
        self.prune_attack_snapshots();
    }

    /// Drops every attack snapshot its viewer sees again: the cell of its spot is `Visible` to
    /// the viewer, or the attacker lives, is not hidden and is shown to the viewer
    /// ([`Self::shown_to`]). The attacker's death alone drops nothing; its image stays until
    /// its spot is seen (plan `2026-11-24-attacker-snapshot` D-2, rule 6). Also drops every
    /// snapshot whose last strike is at least the catalog's cap old
    /// ([`crate::UnitCatalog::attack_snapshot_cap`], plan Amendment 1), whether seen or not.
    fn prune_attack_snapshots(&mut self) {
        if !self.presentation.has_attack_snapshots() {
            return;
        }
        let (map, vision_maps, entities) = (&self.map, &self.vision_maps, &self.entities);
        let (now, cap) = (self.current_tick, self.catalog.attack_snapshot_cap());
        self.presentation.prune_attack_snapshots(|record| {
            if cap.is_some_and(|cap| now.wrapping_sub(record.struck.0) >= cap.get()) {
                return false;
            }
            let Some(vision) = vision_maps.get(record.viewer) else {
                return false;
            };
            let spot_seen = map
                .world_to_cell(record.pose.position)
                .is_ok_and(|(cx, cy)| vision.visibility_at(cx, cy) == VisibilityState::Visible);
            !spot_seen && !Self::snapshot_attacker_shown(map, vision_maps, entities, record)
        });
    }

    /// Whether the attacker of `record` lives, is not hidden and is shown to the record's
    /// viewer ([`Self::shown_to`]): the viewer then sees the unit itself, and its snapshot is
    /// dropped here and left out of the viewer's view (plan `2026-11-24-attacker-snapshot` D-2).
    pub(crate) fn snapshot_attacker_shown(
        map: &sim_map::SimMap,
        vision_maps: &PerPlayer<sim_map::PlayerVisionMap>,
        entities: &std::collections::BTreeMap<EntityId, crate::Entity>,
        record: &crate::world::presentation::AttackSnapshotRecord,
    ) -> bool {
        entities.get(&record.attacker).is_some_and(|attacker| {
            attacker.is_alive()
                && !attacker.is_hidden()
                && Self::shown_to(map, vision_maps, record.viewer, attacker)
        })
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::world::presentation::{AttackSnapshotRecord, SnapshotPose};
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{MoverType, ShapeKind, SimAngle, Tick, UnitTypeId};

    fn at(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    /// A flat 64x64 world, a mobile ground unit type of the shipped catalog, and its attacker
    /// of player 0 at `attacker_at`, whose snapshot player 1 keeps at `spot`, struck on the
    /// world's current tick.
    fn snapshot_world(attacker_at: SimVec2, spot: SimVec2) -> (SimWorld, EntityId, UnitTypeId) {
        let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat map");
        let mut world = crate::test_support::world(map, 0x183);
        let kind = world
            .catalog
            .archetypes()
            .find(|arch| {
                matches!(arch.shape, ShapeKind::UnitSphere)
                    && arch.mover_type == MoverType::Ground
                    && !arch.is_detector()
                    && arch.max_speed > SimScalar::ZERO
            })
            .map(|arch| arch.unit_type_id)
            .expect("the shipped catalog has a mobile ground unit");
        let attacker = world
            .spawn_unit(kind, PlayerId(0), attacker_at)
            .expect("invariant: a free spot");
        world
            .presentation
            .record_attack_snapshot(AttackSnapshotRecord {
                viewer: PlayerId(1),
                attacker,
                struck: Tick(world.current_tick),
                pose: SnapshotPose {
                    unit_type: kind,
                    owner: PlayerId(0),
                    position: spot,
                    heading: SimAngle::ZERO,
                    turret: SimAngle::ZERO,
                    mode: None,
                    hp_fraction: SimScalar::ONE,
                    shield_fraction: SimScalar::ZERO,
                },
            });
        (world, attacker, kind)
    }

    fn kept(world: &SimWorld) -> usize {
        world.presentation.attack_snapshots_of(PlayerId(1)).count()
    }

    /// Plan `2026-11-24-attacker-snapshot` D-2, drop: an unseen snapshot is kept; it is dropped
    /// once its viewer sees its spot, though the attacker stands far from it.
    #[test]
    fn a_snapshot_is_dropped_when_its_spot_is_seen() {
        let spot = at(12.5, 12.5);
        let (mut world, _, kind) = snapshot_world(at(52.5, 52.5), spot);
        world.update_vision();
        assert_eq!(kept(&world), 1, "unseen: kept");
        world
            .spawn_unit(kind, PlayerId(1), at(14.5, 12.5))
            .expect("invariant: a free spot");
        world.update_vision();
        assert_eq!(kept(&world), 0, "the spot is seen");
    }

    /// D-2, drop: a snapshot is dropped once its viewer sees the attacker, wherever it stands.
    #[test]
    fn a_snapshot_is_dropped_when_its_attacker_is_seen() {
        let (mut world, _, kind) = snapshot_world(at(52.5, 52.5), at(12.5, 12.5));
        world
            .spawn_unit(kind, PlayerId(1), at(52.5, 50.5))
            .expect("invariant: a free spot");
        world.update_vision();
        assert_eq!(kept(&world), 0, "the attacker is seen");
    }

    /// D-2, rule 6: the attacker's death drops nothing, even where its viewer sees its body; only
    /// the spot's sight drops the image.
    #[test]
    fn a_snapshot_outlives_its_attacker() {
        let (mut world, attacker, kind) = snapshot_world(at(52.5, 52.5), at(12.5, 12.5));
        world.entities.get_mut(&attacker).expect("spawned").hp = SimScalar::ZERO;
        world
            .spawn_unit(kind, PlayerId(1), at(52.5, 50.5))
            .expect("invariant: a free spot");
        world.update_vision();
        assert_eq!(kept(&world), 1, "a dead attacker is not shown");
        world.entities.remove(&attacker);
        world.update_vision();
        assert_eq!(kept(&world), 1, "nor a reaped one");
    }

    /// Plan Amendment 1: an unseen snapshot is kept while its strike is younger than the
    /// catalog's cap and dropped on the tick it reaches it.
    #[test]
    fn an_unseen_snapshot_is_dropped_when_its_cap_is_reached() {
        let (mut world, _, _) = snapshot_world(at(52.5, 52.5), at(12.5, 12.5));
        let cap = world
            .catalog
            .attack_snapshot_cap()
            .expect("the shipped catalog authors a cap")
            .get();
        world.current_tick += cap - 1;
        world.update_vision();
        assert_eq!(kept(&world), 1, "one tick short of the cap: kept");
        world.current_tick += 1;
        world.update_vision();
        assert_eq!(kept(&world), 0, "the cap is reached");
    }
}
