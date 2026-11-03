//! The middle column of the command panel: what the selected structures are making.
//!
//! # Responsibility
//! Owns [`QueuePanel`] and [`derive_queue`], the pure rule from the HUD's facts to rows of a queue,
//! a foundation's progress, or a summary of several structures. Drawing belongs to
//! [`panels`](super::panels), and cancelling to [`actions`](super::actions).
//!
//! # How it fits
//! Production progress appears here and nowhere else (D-11), so the world view cannot show an
//! enemy's queue, and everything here comes from the viewer's own
//! [`PlayerView`](sim_types::PlayerView). The panel is refreshed with the card, on a change of the
//! model.
//!
//! # Invariants
//! - A row's `job_index` is its position in the structure's queue, as `CancelProduction` means it.
//! - Rows beyond [`QUEUE_ROWS`] are counted in `overflow`, never dropped silently.
//! - Refreshing allocates nothing once warm: text is rewritten only when its numbers change.

use std::fmt::Write;

use bevy::prelude::*;
use sim_types::{EntityId, QueuedItem, QueuedJobView};

use super::model::{HudModel, SelectedFact};

/// Most jobs the panel lists for one structure.
pub const QUEUE_ROWS: usize = 5;

/// One job on show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueueRow {
    /// What the job delivers.
    pub item: QueuedItem,
    /// Its position in the structure's queue.
    pub job_index: u32,
    /// Share done, from 0.0 to 1.0.
    pub progress: f32,
}

/// One line of the summary of several structures.
#[derive(Debug, Default)]
pub struct StructureLine {
    entity: Option<EntityId>,
    jobs: u32,
    /// Cached display text.
    pub text: String,
}

/// The queue column, as of the latest change of the model.
#[derive(Resource, Debug, Default)]
pub struct QueuePanel {
    /// The one structure whose queue is listed, if any.
    pub structure: Option<EntityId>,
    /// Its jobs, front first.
    pub rows: [Option<QueueRow>; QUEUE_ROWS],
    /// Whether a finished job at the front is waiting for a free exit.
    pub exit_blocked: bool,
    /// Cached "+N more" text, empty when every job is listed.
    pub overflow: String,
    /// The single selected foundation's share done, from 0.0 to 1.0.
    pub foundation: Option<f32>,
    overflow_count: usize,
    /// Retained lines, of which the first `line_count` are live.
    lines: Vec<StructureLine>,
    line_count: usize,
    seen_refills: u64,
}

impl QueuePanel {
    /// One line per selected structure with a queue, when more than one unit is selected.
    #[must_use]
    pub fn lines(&self) -> &[StructureLine] {
        &self.lines[..self.line_count]
    }

    fn clear_rows(&mut self) {
        self.structure = None;
        self.rows = [None; QUEUE_ROWS];
        self.exit_blocked = false;
        self.overflow_count = 0;
        self.overflow.clear();
    }

    fn list_queue(&mut self, model: &HudModel, structure: EntityId) {
        let view = &model.view;
        let Some(queue) = view.queue_of(structure) else {
            return;
        };
        let jobs = view.jobs_of(queue);
        self.structure = Some(structure);
        self.exit_blocked = queue.exit_blocked;
        for ((row, job), job_index) in self.rows.iter_mut().zip(jobs).zip(0_u32..) {
            *row = Some(QueueRow {
                item: job.item,
                job_index,
                progress: progress(job),
            });
        }
        let hidden = jobs.len().saturating_sub(QUEUE_ROWS);
        if hidden != self.overflow_count {
            self.overflow_count = hidden;
            self.overflow.clear();
            if hidden > 0 {
                let _ = write!(self.overflow, "+{hidden} more");
            }
        }
    }

    fn summarize(&mut self, model: &HudModel, facts: &[SelectedFact]) {
        self.line_count = 0;
        for fact in facts {
            let Some(queue) = model.view.queue_of(fact.id) else {
                continue;
            };
            if self.line_count == self.lines.len() {
                self.lines.push(StructureLine::default());
            }
            let line = &mut self.lines[self.line_count];
            if line.entity != Some(fact.id) || line.jobs != queue.job_count {
                line.entity = Some(fact.id);
                line.jobs = queue.job_count;
                line.text.clear();
                let _ = write!(
                    line.text,
                    "{}: {} queued",
                    model.catalog.label(fact.unit_type),
                    queue.job_count
                );
            }
            self.line_count += 1;
        }
    }
}

/// The share of `job` that is done, from 0.0 to 1.0.
#[must_use]
pub fn progress(job: &QueuedJobView) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "tick counts of a job are far below f32's exact range"
    )]
    let (done, total) = (
        job.total_ticks.saturating_sub(job.remaining_ticks) as f32,
        job.total_ticks.max(1) as f32,
    );
    done / total
}

/// The display name of what `item` delivers.
#[must_use]
pub fn item_label(model: &HudModel, item: QueuedItem) -> &str {
    match item {
        QueuedItem::Unit(unit_type) => model.catalog.label(unit_type),
        QueuedItem::Research(upgrade) => model
            .catalog
            .upgrade(upgrade)
            .map_or("", |upgrade| upgrade.label.as_str()),
    }
}

/// Fills `panel` for the selection described by `facts`. Pure, so it is tested without Bevy.
pub fn derive_queue(model: &HudModel, facts: &[SelectedFact], panel: &mut QueuePanel) {
    panel.clear_rows();
    panel.foundation = None;
    panel.line_count = 0;
    if !model.orders_open {
        return;
    }
    match facts {
        [] => {}
        [single] if single.foundation => panel.foundation = Some(single.build_progress),
        [single] => panel.list_queue(model, single.id),
        several => panel.summarize(model, several),
    }
}

/// Keeps [`QueuePanel`] in step with the model.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn refresh_queue_panel(model: Res<'_, HudModel>, mut panel: ResMut<'_, QueuePanel>) {
    if panel.seen_refills != model.refills() {
        panel.seen_refills = model.refills();
        derive_queue(&model, model.selected_facts(), &mut panel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::{StructureQueueView, UnitTypeId};

    const STRUCTURE: EntityId = EntityId(7);
    const OTHER: EntityId = EntityId(9);
    const ITEM: QueuedItem = QueuedItem::Unit(UnitTypeId(3));

    fn job(remaining_ticks: u32) -> QueuedJobView {
        QueuedJobView {
            item: ITEM,
            remaining_ticks,
            total_ticks: 100,
        }
    }

    fn model_with_jobs(count: u32) -> HudModel {
        let mut model = HudModel::default();
        model.orders_open = true;
        model.view.jobs = (0..count).map(|index| job(100 - index)).collect();
        model.view.queues = vec![StructureQueueView {
            entity: STRUCTURE,
            first_job: 0,
            job_count: count,
            exit_blocked: true,
        }];
        model
    }

    fn fact(id: EntityId, foundation: bool) -> SelectedFact {
        SelectedFact {
            id,
            unit_type: UnitTypeId(1),
            foundation,
            build_progress: 0.25,
        }
    }

    #[test]
    fn a_single_structure_lists_its_jobs_in_order() {
        let model = model_with_jobs(2);
        let mut panel = QueuePanel::default();
        derive_queue(&model, &[fact(STRUCTURE, false)], &mut panel);
        assert_eq!(panel.structure, Some(STRUCTURE));
        assert!(panel.exit_blocked);
        let shown = |row: Option<QueueRow>| row.map(|row| (row.job_index, row.progress));
        assert_eq!(shown(panel.rows[0]), Some((0, 0.0)));
        assert_eq!(shown(panel.rows[1]), Some((1, 0.01)));
        assert_eq!(panel.rows[2], None);
        assert_eq!(panel.overflow, "");
    }

    #[test]
    fn jobs_past_the_rows_are_counted() {
        let model = model_with_jobs(8);
        let mut panel = QueuePanel::default();
        derive_queue(&model, &[fact(STRUCTURE, false)], &mut panel);
        assert!(panel.rows.iter().all(Option::is_some));
        assert_eq!(panel.overflow, "+3 more");

        derive_queue(&model_with_jobs(5), &[fact(STRUCTURE, false)], &mut panel);
        assert_eq!(panel.overflow, "");
    }

    #[test]
    fn a_structure_with_no_queue_lists_nothing() {
        let model = model_with_jobs(1);
        let mut panel = QueuePanel::default();
        derive_queue(&model, &[fact(OTHER, false)], &mut panel);
        assert_eq!(panel.structure, None);
        assert!(panel.rows.iter().all(Option::is_none));
    }

    #[test]
    fn a_foundation_shows_its_progress_and_no_rows() {
        let model = model_with_jobs(1);
        let mut panel = QueuePanel::default();
        derive_queue(&model, &[fact(STRUCTURE, true)], &mut panel);
        assert_eq!(panel.foundation, Some(0.25));
        assert_eq!(panel.structure, None);
    }

    #[test]
    fn several_structures_show_counts_rather_than_rows() {
        let model = model_with_jobs(3);
        let mut panel = QueuePanel::default();
        derive_queue(
            &model,
            &[fact(STRUCTURE, false), fact(OTHER, false)],
            &mut panel,
        );
        assert_eq!(panel.structure, None);
        assert!(panel.rows.iter().all(Option::is_none));
        assert_eq!(panel.lines().len(), 1);
        assert!(panel.lines()[0].text.ends_with(": 3 queued"));
    }

    #[test]
    fn a_closed_match_shows_no_queue() {
        let mut model = model_with_jobs(1);
        model.orders_open = false;
        let mut panel = QueuePanel::default();
        derive_queue(&model, &[fact(STRUCTURE, false)], &mut panel);
        assert_eq!(panel.structure, None);
    }

    #[test]
    fn a_job_of_no_length_counts_as_done_without_dividing_by_zero() {
        let job = QueuedJobView {
            item: ITEM,
            remaining_ticks: 0,
            total_ticks: 0,
        };
        assert_eq!(progress(&job), 0.0);
    }
}
