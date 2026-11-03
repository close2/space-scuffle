//! Everything the HUD draws, refreshed only when what it depends on changes.
//!
//! # Responsibility
//! Owns [`HudModel`]: one viewer's [`PlayerView`], the session's [`CommandCatalog`], the selection
//! summarized by type, and display text cached as strings.
//!
//! # How it fits
//! [`refresh_hud_model`] runs in `PreparePresentation`, after the tick driver and scene sync, and
//! the panels read the result on the next frame. This is the only place the HUD reads the world.
//!
//! # Invariants
//! - `view` holds only what its viewer may know, because `sim_core` wrote it.
//! - The selection summary is rebuilt from the presented snapshot, so it never names a unit the
//!   scene does not hold.
//! - A frame on which neither tick, viewer nor selection changed rewrites nothing and allocates
//!   nothing. A rewrite reuses every retained buffer.
//! - [`HudModel::selected_facts`] holds the selection in its order, less any unit not presented.

use std::fmt::Write;

use bevy::prelude::*;
use sim_types::snapshot::{BubbleRenderInstance, PlayerStatusView};
use sim_types::{AnimState, EntityId, PlayerId, PlayerView, UnitTypeId};

use super::catalog::CommandCatalog;
use super::feed::BattleFeed;
use crate::battle_input::command_mode::orders_open;
use crate::battle_pipeline::clock::SIM_TICK_PERIOD;
use crate::battle_pipeline::commands::SessionGeneration;
use crate::battle_pipeline::session::BattleSession;
use crate::battle_presentation::BattlePresentation;
use crate::battle_selection::BattleSelection;
use crate::bubble_shader::FlipbookRegistry;
use crate::media::ClientMediaStatus;

/// Selected units of one type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionGroup {
    /// The type all of these units share.
    pub unit_type: UnitTypeId,
    /// How many are selected.
    pub count: usize,
    /// Cached display text.
    pub label: String,
}

/// One selected unit, as the command card and the queue panel read it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectedFact {
    /// The unit.
    pub id: EntityId,
    /// Its catalog identity.
    pub unit_type: UnitTypeId,
    /// Whether it is an unfinished structure, read from its presented action.
    pub foundation: bool,
    /// A foundation's share done, from 0.0 to 1.0, and otherwise 0.0.
    pub build_progress: f32,
}

/// The one unit a selection is looking at, for its name and bars.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusedUnit {
    /// Its catalog identity.
    pub unit_type: UnitTypeId,
    /// Health as a ratio of its maximum, from 0.0 to 1.0.
    pub health: f32,
    /// Shield as a ratio of its maximum, from 0.0 to 1.0. `None` for a unit of another player,
    /// and for a type with no shield.
    pub shield: Option<f32>,
}

/// What the HUD draws, as of the latest refresh.
#[derive(Resource, Debug, Default)]
pub struct HudModel {
    /// What the viewer may know.
    pub view: PlayerView,
    /// Labels and capabilities of this session.
    pub catalog: CommandCatalog,
    /// Whose HUD this is. `None` until a battle is committed, and the HUD then draws no panels.
    pub viewer: Option<PlayerId>,
    /// Whether the match still takes orders.
    pub orders_open: bool,
    /// The single selected or inspected unit, if there is exactly one.
    pub focus: Option<FocusedUnit>,
    /// Cached bank and supply text, rewritten only when its numbers change.
    pub resource_line: String,
    /// Cached match clock, rewritten only when its seconds change.
    pub clock_line: String,
    generation: Option<SessionGeneration>,
    /// Viewer and tick `view` was written for.
    written: Option<(PlayerId, u32)>,
    last_selection: Vec<EntityId>,
    last_inspected: Option<EntityId>,
    /// Retained slots, of which the first `group_count` are live.
    groups: Vec<SelectionGroup>,
    group_count: usize,
    /// Scratch list of selected types, sorted during a rebuild.
    selected_types: Vec<UnitTypeId>,
    /// The selection, unit by unit.
    facts: Vec<SelectedFact>,
    /// How many times the selection has changed.
    selection_changes: u64,
    shown_status: Option<PlayerStatusView>,
    shown_seconds: Option<u64>,
    refills: u64,
}

impl HudModel {
    /// One entry per distinct selected type, sorted by type.
    #[must_use]
    pub fn selection_groups(&self) -> &[SelectionGroup] {
        &self.groups[..self.group_count]
    }

    /// One fact per selected unit the scene holds. Empty for an inspected enemy.
    #[must_use]
    pub fn selected_facts(&self) -> &[SelectedFact] {
        &self.facts
    }

    /// How many times this model has been rewritten: the change detection of what derives from it.
    #[must_use]
    pub const fn refills(&self) -> u64 {
        self.refills
    }

    /// How many times the selection has changed, so a reader can tell that from a new tick.
    #[must_use]
    pub const fn selection_changes(&self) -> u64 {
        self.selection_changes
    }

    /// Whether the summary still describes `selection`.
    fn selection_is_current(&self, selection: &BattleSelection) -> bool {
        selection.ids() == self.last_selection && selection.inspected() == self.last_inspected
    }

    /// Catches the summary up with `selection` between two refreshes.
    ///
    /// A key that follows a click or a group recall can arrive before the next refresh, and it
    /// must meet the selection the player just made.
    pub fn sync_selection(
        &mut self,
        selection: &BattleSelection,
        bubbles: &[BubbleRenderInstance],
    ) {
        if !self.selection_is_current(selection) {
            self.rebuild_selection(selection, bubbles);
            self.refills += 1;
        }
    }

    /// Rewrites the cached lines whose numbers changed.
    fn rewrite_lines(&mut self) {
        let status = self.view.status;
        if self.shown_status != Some(status) {
            self.shown_status = Some(status);
            self.resource_line.clear();
            let _ = write!(
                self.resource_line,
                "Minerals {}    Gas {}    Supply {} / {}",
                status.minerals, status.gas, status.supply_used, status.supply_max
            );
        }
        let seconds = (SIM_TICK_PERIOD * self.view.tick.0).as_secs();
        if self.shown_seconds != Some(seconds) {
            self.shown_seconds = Some(seconds);
            self.clock_line.clear();
            let _ = write!(self.clock_line, "{}:{:02}", seconds / 60, seconds % 60);
        }
    }

    /// Rebuilds the summary of `selection` from `bubbles`, which are sorted by entity ID.
    fn rebuild_selection(&mut self, selection: &BattleSelection, bubbles: &[BubbleRenderInstance]) {
        if !self.selection_is_current(selection) {
            self.selection_changes += 1;
        }
        self.last_selection.clear();
        self.last_selection.extend_from_slice(selection.ids());
        self.last_inspected = selection.inspected();
        self.facts.clear();
        self.facts.extend(selection.ids().iter().filter_map(|id| {
            let bubble = bubble_of(bubbles, *id)?;
            let foundation = bubble.anim_state == AnimState::Constructing;
            Some(SelectedFact {
                id: *id,
                unit_type: bubble.unit_type,
                foundation,
                build_progress: bubble
                    .build_or_queue_progress
                    .filter(|_| foundation)
                    .unwrap_or_default(),
            })
        }));
        self.selected_types.clear();
        self.selected_types.extend(
            selection
                .ids()
                .iter()
                .filter_map(|id| bubble_of(bubbles, *id))
                .map(|bubble| bubble.unit_type),
        );
        self.selected_types.sort_unstable();
        // PERFORMANCE RATIONALE:
        // Why: With units selected this runs on every tick, and a label is a heap string.
        // What: Slots outlive a rebuild, and a slot's text is rewritten only when its type or
        // count changed.
        // Trade-off: `group_count` must be respected, so readers go through `selection_groups`.
        self.group_count = 0;
        let mut start = 0;
        while let Some(&unit_type) = self.selected_types.get(start) {
            let run = self.selected_types[start..]
                .iter()
                .take_while(|candidate| **candidate == unit_type)
                .count();
            if self.group_count == self.groups.len() {
                self.groups.push(SelectionGroup::default());
            }
            let label = self.catalog.label(unit_type);
            let group = &mut self.groups[self.group_count];
            if group.unit_type != unit_type || group.count != run {
                group.unit_type = unit_type;
                group.count = run;
                group.label.clear();
                let _ = write!(group.label, "{run} × {label}");
            }
            self.group_count += 1;
            start += run;
        }
    }

    /// Points `focus` at the single selected or inspected unit, from `bubbles`.
    fn update_focus(&mut self, selection: &BattleSelection, bubbles: &[BubbleRenderInstance]) {
        let subject = match (selection.inspected(), selection.ids()) {
            (Some(id), _) => Some((id, true)),
            (None, [id]) => Some((*id, false)),
            (None, _) => None,
        };
        self.focus = subject.and_then(|(id, foreign)| {
            let bubble = bubble_of(bubbles, id)?;
            let shielded = self
                .catalog
                .archetype(bubble.unit_type)
                .is_some_and(|archetype| archetype.shielded);
            Some(FocusedUnit {
                unit_type: bubble.unit_type,
                health: bubble.hp_ratio,
                shield: (shielded && !foreign).then_some(bubble.shield_ratio),
            })
        });
    }
}

/// Finds `id` in `bubbles`, which the simulation publishes sorted by entity ID.
fn bubble_of(bubbles: &[BubbleRenderInstance], id: EntityId) -> Option<&BubbleRenderInstance> {
    bubbles
        .binary_search_by_key(&id, |bubble| bubble.entity_id)
        .ok()
        .and_then(|index| bubbles.get(index))
}

// PERFORMANCE RATIONALE:
// Why: This runs every frame, while its inputs change at 25 Hz at most.
// What: Compare tick and selection, then refill retained buffers only on a change.
// Trade-off: A little state in exchange for a frame with no world access or allocation.
/// Brings [`HudModel`] up to date with the session and the selection.
///
/// O(S) in the selection on a frame without change, plus O(log N) in the bubbles for the focused
/// unit's bars.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn refresh_hud_model(
    session: Res<'_, BattleSession>,
    presentation: Res<'_, BattlePresentation>,
    selection: Res<'_, BattleSelection>,
    registry: Res<'_, FlipbookRegistry>,
    media: Res<'_, ClientMediaStatus>,
    mut model: ResMut<'_, HudModel>,
    mut feed: ResMut<'_, BattleFeed>,
) {
    let model = &mut *model;
    let Some(world) = session.world() else {
        model.viewer = None;
        model.focus = None;
        return;
    };
    let generation = session.commands.generation();
    if model.generation != generation {
        model.generation = generation;
        model.written = None;
        model.shown_status = None;
        model.shown_seconds = None;
        model.catalog =
            CommandCatalog::build(&world.catalog, &world.upgrade_catalog, |unit_type| {
                media
                    .presentable(&registry, unit_type)
                    .err()
                    .map(|error| error.to_string())
            });
        feed.clear();
    }
    model.viewer = presentation.controlled;
    let Some(viewer) = model.viewer else {
        return;
    };
    let bubbles = session.frames.presented().bubbles.as_slice();
    let written = Some((viewer, world.current_tick));
    let tick_changed = model.written != written;
    if tick_changed {
        world.write_player_view(viewer, &mut model.view);
        model.orders_open = orders_open(world);
        model.written = written;
        model.rewrite_lines();
    }
    if tick_changed || !model.selection_is_current(&selection) {
        model.rebuild_selection(&selection, bubbles);
        model.refills += 1;
    }
    model.update_focus(&selection, bubbles);
}
