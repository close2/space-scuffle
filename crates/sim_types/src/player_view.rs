//! What one player may know of the match, as a plain record for that player's interface.
//!
//! # Responsibility
//! Defines [`PlayerView`], the viewer-scoped counterpart of the global
//! [`RenderSnapshot`](crate::RenderSnapshot), and the facts a command card and placement ghost
//! need: queues, cargo, carriers, what can be made and why not, and which nodes are in sight.
//!
//! # How it fits
//! `sim_core` fills the record through `SimWorld::write_player_view`, and the client reads it.
//! It lives here because the client and the simulation both name it, and `sim_types` is the only
//! crate between them. It is derived output: it is not checksummed, and it is not an input to
//! the simulation.
//!
//! # Invariants
//! - Everything hidden from the viewer is absent by construction, so a reader cannot leak it.
//! - Integers and enums only. Nothing here crosses the fixed-point boundary.
//! - Every list has a documented sort order, so equal worlds give equal views.
//! - `visibility.len() == map_width * map_height`, in row-major order.

use crate::enums::VisibilityState;
use crate::ids::{EntityId, PlayerId, ResourceNodeId, Tick, UnitTypeId};
use crate::snapshot::PlayerStatusView;
use serde::{Deserialize, Serialize};

/// One player's knowledge of the match at one executed tick.
///
/// A record is meant to be reused: the writer clears and refills it in place. A viewer outside
/// the supported player slots gets an empty record.
// PERFORMANCE RATIONALE:
// Why: The HUD reads queues every frame and they are rewritten every tick.
// What: One flat job slab addressed by range, refilled in place.
// Trade-off: Readers slice by range rather than holding a nested Vec.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerView {
    /// Executed tick this view describes.
    pub tick: Tick,
    /// The player this view was written for.
    pub viewer: PlayerId,
    /// The viewer's economy and standing.
    pub status: PlayerStatusView,
    /// Map width in grid cells.
    pub map_width: u32,
    /// Map height in grid cells.
    pub map_height: u32,
    /// Row-major: `y * map_width + x`.
    pub visibility: Vec<VisibilityState>,
    /// The viewer's structures with a non-empty queue, by entity ID.
    pub queues: Vec<StructureQueueView>,
    /// Jobs of all `queues`, flat. Each queue addresses a range of this.
    pub jobs: Vec<QueuedJobView>,
    /// The viewer's carriers, by entity ID.
    pub carriers: Vec<CarrierView>,
    /// The viewer's units holding cargo, sorted.
    pub laden: Vec<EntityId>,
    /// Types of which the viewer has a completed, living entity outside any carrier, sorted.
    ///
    /// This is the set prerequisites are judged against.
    pub completed_types: Vec<UnitTypeId>,
    /// Everything the viewer's entities could make or raise, sorted by subject.
    pub options: Vec<CommandOptionView>,
    /// Types the viewer owns whose overclock is researched, sorted.
    pub overclock_ready: Vec<UnitTypeId>,
    /// Nodes in `Visible` cells, by node ID.
    pub visible_nodes: Vec<VisibleNodeView>,
    /// The viewer's living, uncarried units whose archetype declares modes, by entity ID.
    pub mode_states: Vec<ModeStateView>,
    /// Abilities of the viewer's completed types that still need research, sorted by type and
    /// then ability index.
    pub ability_locks: Vec<AbilityLockView>,
    /// Concealed entities of other players that the viewer detects now, sorted: each stands in
    /// a cell that is `Visible` to the viewer and covered by one of the viewer's detectors. A
    /// concealed entity not listed is hidden from the viewer (plan
    /// `2026-11-09-effect-visualization` D-5). Never holds the viewer's own entities.
    #[serde(default)]
    pub revealed: Vec<EntityId>,
}

impl PlayerView {
    /// Empties the record, keeping the capacity of every buffer.
    pub fn clear(&mut self) {
        self.tick = Tick::default();
        self.viewer = PlayerId::default();
        self.status = PlayerStatusView::default();
        self.map_width = 0;
        self.map_height = 0;
        self.visibility.clear();
        self.queues.clear();
        self.jobs.clear();
        self.carriers.clear();
        self.laden.clear();
        self.completed_types.clear();
        self.options.clear();
        self.overclock_ready.clear();
        self.visible_nodes.clear();
        self.mode_states.clear();
        self.ability_locks.clear();
        self.revealed.clear();
    }

    /// The jobs of `queue`, front of the queue first.
    ///
    /// Returns an empty slice for a queue that does not belong to this record.
    #[must_use]
    pub fn jobs_of(&self, queue: &StructureQueueView) -> &[QueuedJobView] {
        let first = queue.first_job as usize;
        first
            .checked_add(queue.job_count as usize)
            .and_then(|end| self.jobs.get(first..end))
            .unwrap_or_default()
    }

    /// The queue of `entity`, if the viewer owns it and it holds a job.
    #[must_use]
    pub fn queue_of(&self, entity: EntityId) -> Option<&StructureQueueView> {
        self.queues
            .binary_search_by_key(&entity, |queue| queue.entity)
            .ok()
            .and_then(|index| self.queues.get(index))
    }
}

/// One structure's production queue, as a range of [`PlayerView::jobs`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureQueueView {
    /// The producing structure.
    pub entity: EntityId,
    /// Index of the front job in [`PlayerView::jobs`].
    pub first_job: u32,
    /// Number of jobs in the range; never zero.
    pub job_count: u32,
    /// Whether a finished job at the front is waiting for a free exit.
    pub exit_blocked: bool,
}

/// One job in a production queue.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedJobView {
    /// What the job delivers.
    pub item: QueuedItem,
    /// Production ticks left, at 25 Hz; zero means ready.
    pub remaining_ticks: u32,
    /// Ticks the job took at admission, at 25 Hz.
    pub total_ticks: u32,
}

/// What a queued job delivers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueuedItem {
    /// A mobile unit of this type.
    Unit(UnitTypeId),
    /// The upgrade with this catalog ID.
    Research(u32),
}

/// One of the viewer's carriers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarrierView {
    /// The carrier.
    pub entity: EntityId,
    /// Passenger slots in use.
    pub slots_used: u32,
    /// Passenger slots in all.
    pub capacity: u32,
}

/// The stance of one of the viewer's units whose archetype declares modes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeStateView {
    /// The unit.
    pub entity: EntityId,
    /// Index into the archetype's `modes` of the mode the unit is in or, while it channels a
    /// switch, the mode it will be in when the channel ends (`Entity::settled_mode`); `None`
    /// for the default stance.
    pub settled_mode: Option<u8>,
    /// Whether it is channelling a switch now.
    pub channeling: bool,
}

/// One of the viewer's types whose ability at `ability_index` still needs research.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AbilityLockView {
    /// The type.
    pub unit_type: UnitTypeId,
    /// Index into the type's `abilities`.
    pub ability_index: u8,
}

/// One resource node in sight of the viewer.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibleNodeView {
    /// The node.
    pub id: ResourceNodeId,
    /// Units left to gather.
    pub remaining: u32,
}

/// Something a player could order made.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CommandSubject {
    /// Train this mobile unit at a structure.
    Train(UnitTypeId),
    /// Raise this structure with a builder.
    Raise(UnitTypeId),
    /// Research the upgrade with this catalog ID.
    Research(u32),
}

/// Why a [`CommandSubject`] cannot be ordered at all right now.
///
/// Shortage of money or supply is not a lock: income can change between frames, and the
/// simulation's typed rejection explains it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionLock {
    /// The viewer has no completed structure of this type.
    MissingStructure(UnitTypeId),
    /// The preceding tier of this upgrade has not finished.
    TierLocked,
    /// The viewer already owns this upgrade.
    Researched,
    /// This upgrade is already in one of the viewer's queues.
    Researching,
}

/// One entry of what the viewer's entities can do, with its price.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandOptionView {
    /// What would be made.
    pub subject: CommandSubject,
    /// Why it cannot be ordered, or `None` if only money or supply could refuse it.
    pub lock: Option<OptionLock>,
    /// Mineral cost.
    pub minerals: u32,
    /// Gas cost.
    pub gas: u32,
    /// Supply the job reserves; zero for structures and research.
    pub supply: u32,
}

/// Why a site cannot be ordered, by what the viewer knows. Carries no entity identity.
///
/// The terrain variants flatten the map crate's placement reasons, because `sim_types` cannot
/// name `sim_map`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SiteRefusal {
    /// The viewer slot or the structure type is not valid. A caller's mistake, not a property of
    /// the site.
    InvalidRequest,
    /// The footprint leaves the map.
    OutOfBounds,
    /// A cell of the footprint is not buildable ground.
    UnbuildableTerrain,
    /// The footprint spans more than one cliff level.
    CliffLevelMismatch,
    /// The footprint overlaps a ramp.
    OverlapsRamp,
    /// The viewer owns, or remembers, a structure on the footprint.
    KnownStructure,
    /// The footprint would bury a resource node.
    CoversResource,
    /// An extractor must stand on a gas node and on no mineral node.
    ExtractorNeedsGas,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn populated() -> PlayerView {
        PlayerView {
            tick: Tick(90),
            viewer: PlayerId(1),
            status: PlayerStatusView {
                minerals: 150,
                gas: 25,
                supply_used: 7,
                supply_max: 15,
                defeated: false,
            },
            map_width: 2,
            map_height: 1,
            visibility: vec![VisibilityState::Visible, VisibilityState::Unexplored],
            queues: vec![
                StructureQueueView {
                    entity: EntityId(3),
                    first_job: 0,
                    job_count: 1,
                    exit_blocked: true,
                },
                StructureQueueView {
                    entity: EntityId(5),
                    first_job: 1,
                    job_count: 2,
                    exit_blocked: false,
                },
            ],
            jobs: vec![
                QueuedJobView {
                    item: QueuedItem::Unit(UnitTypeId(4)),
                    remaining_ticks: 0,
                    total_ticks: 100,
                },
                QueuedJobView {
                    item: QueuedItem::Research(2),
                    remaining_ticks: 80,
                    total_ticks: 200,
                },
                QueuedJobView {
                    item: QueuedItem::Unit(UnitTypeId(4)),
                    remaining_ticks: 100,
                    total_ticks: 100,
                },
            ],
            carriers: vec![CarrierView {
                entity: EntityId(9),
                slots_used: 2,
                capacity: 8,
            }],
            laden: vec![EntityId(11)],
            completed_types: vec![UnitTypeId(4), UnitTypeId(6)],
            options: vec![
                CommandOptionView {
                    subject: CommandSubject::Train(UnitTypeId(4)),
                    lock: None,
                    minerals: 50,
                    gas: 0,
                    supply: 1,
                },
                CommandOptionView {
                    subject: CommandSubject::Raise(UnitTypeId(6)),
                    lock: Some(OptionLock::MissingStructure(UnitTypeId(2))),
                    minerals: 150,
                    gas: 0,
                    supply: 0,
                },
                CommandOptionView {
                    subject: CommandSubject::Research(2),
                    lock: Some(OptionLock::TierLocked),
                    minerals: 100,
                    gas: 100,
                    supply: 0,
                },
            ],
            overclock_ready: vec![UnitTypeId(4)],
            visible_nodes: vec![VisibleNodeView {
                id: ResourceNodeId(1),
                remaining: 900,
            }],
            mode_states: vec![ModeStateView {
                entity: EntityId(12),
                settled_mode: Some(0),
                channeling: true,
            }],
            ability_locks: vec![AbilityLockView {
                unit_type: UnitTypeId(4),
                ability_index: 1,
            }],
            revealed: vec![EntityId(14), EntityId(20)],
        }
    }

    #[test]
    fn a_view_written_before_revealed_existed_still_reads() {
        let mut view = populated();
        view.revealed.clear();
        let text = ron::to_string(&view).expect("the view has no unserializable field");
        let legacy = text.replace(",revealed:[]", "");
        assert_ne!(legacy, text, "the field was removed from the text");
        let back: PlayerView = ron::from_str(&legacy).expect("a missing list reads as empty");
        assert_eq!(back, view);
    }

    #[test]
    fn a_view_survives_a_ron_round_trip() {
        let view = populated();
        let text = ron::to_string(&view).expect("the view has no unserializable field");
        let back: PlayerView = ron::from_str(&text).expect("text was just written by ron");
        assert_eq!(back, view);
    }

    #[test]
    fn jobs_are_addressed_by_range() {
        let view = populated();
        let first = view.queue_of(EntityId(3)).expect("queue listed above");
        let second = view.queue_of(EntityId(5)).expect("queue listed above");
        assert_eq!(view.jobs_of(first), &view.jobs[..1]);
        assert_eq!(view.jobs_of(second), &view.jobs[1..]);
        assert!(view.queue_of(EntityId(4)).is_none());
    }

    #[test]
    fn a_range_outside_the_record_is_empty() {
        let view = populated();
        let foreign = StructureQueueView {
            entity: EntityId(5),
            first_job: u32::MAX,
            job_count: u32::MAX,
            exit_blocked: false,
        };
        assert_eq!(view.jobs_of(&foreign).len(), 0);
    }

    #[test]
    fn clearing_keeps_capacity_and_leaves_a_default_record() {
        let mut view = populated();
        let capacity = view.jobs.capacity();
        view.clear();
        assert_eq!(view, PlayerView::default());
        assert_eq!(view.jobs.capacity(), capacity);
    }

    #[test]
    fn subjects_sort_by_kind_then_identity() {
        let mut subjects = [
            CommandSubject::Research(1),
            CommandSubject::Raise(UnitTypeId(2)),
            CommandSubject::Train(UnitTypeId(9)),
            CommandSubject::Train(UnitTypeId(3)),
        ];
        subjects.sort_unstable();
        assert_eq!(
            subjects,
            [
                CommandSubject::Train(UnitTypeId(3)),
                CommandSubject::Train(UnitTypeId(9)),
                CommandSubject::Raise(UnitTypeId(2)),
                CommandSubject::Research(1),
            ]
        );
    }
}
