//! Group-routing measures of the route trace (plan `2026-11-10-group-routing`, WP4 and D-8).
//!
//! **Responsibility.** Watches a battle tick by tick and records, per route group, its members
//! and lane, and every `GroupLane` route event (own length, lane length, adoption); per unit, the
//! x where it first crosses a horizontal lane line. The measures are derived from the public
//! world state and route events only.
//!
//! **Place in the system.** [`super::route`] owns the run loop and calls
//! [`GroupMeter::observe`] after every tick; this module formats `groups.md` and `lanes.csv`.
//!
//! **Invariants.** Groups are listed in id order and events in tick order; a unit has at most one
//! lane crossing, its first.

use sim_core::movement::groups::{LaneState, RouteGroupId};
use sim_core::movement::RouteOutcomeKind;
use sim_core::SimWorld;
use std::collections::BTreeMap;

/// What the trace knows of one route group.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupRecord {
    /// Group id.
    pub id: u32,
    /// Owner slot of the command that formed it.
    pub player: u8,
    /// Members when the group was first seen.
    pub members: usize,
    /// The lane once computed: leader id and lane word letters; `None` while pending or when the
    /// leader had no route.
    pub lane: Option<(u32, Vec<i32>)>,
    /// Whether the leader's search found no route (lane state `None`).
    pub no_lane: bool,
}

/// One `GroupLane` route event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaneEvent {
    /// Tick just stepped.
    pub tick: u32,
    /// Raw entity id.
    pub entity: u32,
    /// Group id.
    pub group: u32,
    /// Own route length, world units.
    pub own_length: f64,
    /// Lane route length, world units; `None` when the lane admitted no route.
    pub lane_length: Option<f64>,
    /// Whether the member took the lane route.
    pub adopted: bool,
}

impl LaneEvent {
    /// Lane length over own length, when the lane admitted a route.
    #[must_use]
    pub fn ratio(&self) -> Option<f64> {
        self.lane_length
            .filter(|_| self.own_length > 0.0)
            .map(|lane| lane / self.own_length)
    }
}

/// One unit's first crossing of the lane line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaneCrossing {
    /// Raw entity id.
    pub entity: u32,
    /// Owner slot.
    pub owner: u8,
    /// Tick of the first sample past the line.
    pub tick: u32,
    /// x of the crossing, world units, interpolated between the two samples.
    pub x: f64,
}

/// Accumulates the group measures of one run.
#[derive(Debug, Clone, Default)]
pub struct GroupMeter {
    groups: BTreeMap<u32, GroupRecord>,
    events: Vec<LaneEvent>,
    lane_line: Option<f64>,
    last: BTreeMap<u32, (u8, f64, f64)>,
    crossings: BTreeMap<u32, LaneCrossing>,
}

impl GroupMeter {
    /// A meter that also records crossings of the horizontal line `y = lane_line`, world units.
    #[must_use]
    pub fn new(lane_line: Option<f64>) -> Self {
        Self {
            lane_line,
            ..Self::default()
        }
    }

    /// Records the groups, this tick's `GroupLane` events and the lane-line crossings of `world`
    /// after the tick `tick` has been stepped.
    pub fn observe(&mut self, world: &SimWorld, tick: u32) {
        for group in world.route_groups() {
            let record = self
                .groups
                .entry(group.id.0)
                .or_insert_with(|| GroupRecord {
                    id: group.id.0,
                    player: group.player.0,
                    members: group.members.len(),
                    lane: None,
                    no_lane: false,
                });
            match &group.lane {
                LaneState::Lane(lane) => {
                    record.lane = Some((lane.leader.0, lane.word.letters().to_vec()));
                }
                LaneState::None => record.no_lane = true,
                LaneState::Pending => {}
            }
        }
        for outcome in world.route_outcomes() {
            if let RouteOutcomeKind::GroupLane {
                group: RouteGroupId(group),
                own_length,
                lane_length,
                adopted,
            } = outcome.kind
            {
                self.events.push(LaneEvent {
                    tick,
                    entity: outcome.entity.0,
                    group,
                    own_length: f64::from(own_length.to_f32()),
                    lane_length: lane_length.map(|length| f64::from(length.to_f32())),
                    adopted,
                });
            }
        }
        if let Some(line) = self.lane_line {
            self.observe_crossings(world, tick, line);
        }
    }

    fn observe_crossings(&mut self, world: &SimWorld, tick: u32, line: f64) {
        for (id, entity) in &world.entities {
            if entity.is_immobile {
                continue;
            }
            let (x, y) = (
                f64::from(entity.position.x.to_f32()),
                f64::from(entity.position.y.to_f32()),
            );
            if let Some(&(owner, px, py)) = self.last.get(&id.0) {
                let crossed = (py - line) * (y - line) <= 0.0 && (py - y).abs() > f64::EPSILON;
                if crossed && !self.crossings.contains_key(&id.0) {
                    let t = (line - py) / (y - py);
                    self.crossings.insert(
                        id.0,
                        LaneCrossing {
                            entity: id.0,
                            owner,
                            tick,
                            x: px + (x - px) * t,
                        },
                    );
                }
            }
            self.last.insert(id.0, (entity.owner.0, x, y));
        }
    }

    /// Whether this meter records lane-line crossings.
    #[must_use]
    pub const fn has_lane_line(&self) -> bool {
        self.lane_line.is_some()
    }

    /// The groups seen, in id order.
    #[must_use]
    pub fn groups(&self) -> Vec<GroupRecord> {
        self.groups.values().cloned().collect()
    }

    /// Every `GroupLane` event, in tick order.
    #[must_use]
    pub fn events(&self) -> &[LaneEvent] {
        &self.events
    }

    /// Each unit's first lane-line crossing, in entity order.
    #[must_use]
    pub fn crossings(&self) -> Vec<LaneCrossing> {
        self.crossings.values().copied().collect()
    }
}

/// `groups.md`: per group, its members, lane, events, adoptions and lane-to-own ratios.
#[must_use]
pub fn groups_markdown(groups: &[GroupRecord], events: &[LaneEvent]) -> String {
    let mut out = vec![
        "| Group | Player | Members | Leader | Lane word | Events | Adopted | No lane route | Ratio min / mean / max |\n".to_owned(),
        "|---:|---:|---:|---:|---|---:|---:|---:|---|\n".to_owned(),
    ];
    for group in groups {
        let mine: Vec<&LaneEvent> = events.iter().filter(|e| e.group == group.id).collect();
        let ratios: Vec<f64> = mine.iter().filter_map(|e| e.ratio()).collect();
        let (leader, word) = match (&group.lane, group.no_lane) {
            (Some((leader, word)), _) => (leader.to_string(), format!("{word:?}")),
            (None, true) => ("—".to_owned(), "none".to_owned()),
            (None, false) => ("—".to_owned(), "pending".to_owned()),
        };
        let ratio = if ratios.is_empty() {
            "—".to_owned()
        } else {
            let min = ratios.iter().copied().fold(f64::INFINITY, f64::min);
            let max = ratios.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let mean = ratios.iter().sum::<f64>()
                / f64::from(u32::try_from(ratios.len()).unwrap_or(u32::MAX));
            format!("{min:.4} / {mean:.4} / {max:.4}")
        };
        out.push(format!(
            "| {} | {} | {} | {leader} | {word} | {} | {} | {} | {ratio} |\n",
            group.id,
            group.player,
            group.members,
            mine.len(),
            mine.iter().filter(|e| e.adopted).count(),
            mine.iter().filter(|e| e.lane_length.is_none()).count(),
        ));
    }
    out.push("\n| Tick | Entity | Group | Own | Lane | Ratio | Adopted |\n|---:|---:|---:|---:|---:|---:|---|\n".to_owned());
    for e in events {
        out.push(format!(
            "| {} | {} | {} | {:.4} | {} | {} | {} |\n",
            e.tick,
            e.entity,
            e.group,
            e.own_length,
            e.lane_length
                .map_or_else(|| "—".to_owned(), |l| format!("{l:.4}")),
            e.ratio()
                .map_or_else(|| "—".to_owned(), |r| format!("{r:.4}")),
            e.adopted
        ));
    }
    out.concat()
}

/// `lanes.csv`: each unit's first crossing of the lane line.
#[must_use]
pub fn lanes_csv(crossings: &[LaneCrossing]) -> String {
    let mut out = vec!["entity,owner,tick,x\n".to_owned()];
    for c in crossings {
        out.push(format!("{},{},{},{:.4}\n", c.entity, c.owner, c.tick, c.x));
    }
    out.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_need_a_lane_route_and_a_positive_own_length() {
        let mut event = LaneEvent {
            tick: 0,
            entity: 1,
            group: 1,
            own_length: 10.0,
            lane_length: Some(10.5),
            adopted: true,
        };
        assert_eq!(event.ratio(), Some(1.05));
        event.lane_length = None;
        assert_eq!(event.ratio(), None);
        event.lane_length = Some(1.0);
        event.own_length = 0.0;
        assert_eq!(event.ratio(), None);
    }

    #[test]
    fn markdown_counts_events_per_group() {
        let groups = vec![GroupRecord {
            id: 3,
            player: 1,
            members: 10,
            lane: Some((7, vec![1])),
            no_lane: false,
        }];
        let events = vec![
            LaneEvent {
                tick: 1,
                entity: 7,
                group: 3,
                own_length: 10.0,
                lane_length: Some(10.2),
                adopted: true,
            },
            LaneEvent {
                tick: 2,
                entity: 8,
                group: 3,
                own_length: 10.0,
                lane_length: None,
                adopted: false,
            },
        ];
        let text = groups_markdown(&groups, &events);
        assert!(text.contains("| 3 | 1 | 10 | 7 | [1] | 2 | 1 | 1 | 1.0200 / 1.0200 / 1.0200 |"));
        assert_eq!(lanes_csv(&[]), "entity,owner,tick,x\n");
    }
}
