//! Attack alerts, minimap pings and the struck flash (plan `2026-12-01-minimap` D-13).
//!
//! # Responsibility
//! [`AlertLog`] keeps three bounded, retained lists:
//! - **alerts**: "under attack" alerts, newest last, each a [`AlertKind`] (a structure struck,
//!   5.0.14's `AttackTown`, or a unit, `AttackUnit`; `core` `AlertData:93–115`), the struck
//!   position and the presented tick it was raised on, pruned after `life_ticks`;
//! - **pings**: rings on the minimap, an alert's red one or the player's own, pruned after their
//!   `ping_ticks`;
//! - **struck**: each of the viewer's units struck within `flash_ticks`, sorted by entity, so its
//!   square flashes `attacked_color` (`GameUIData:654 UnitDamageFlashDuration`).
//!
//! # The overlap rule
//! 5.0.14's `Attack` alert (`core` `AlertData:22–35`) is not raised again while, among the
//! alerts raised within `overlap_ticks`, at least `overlap_global` exist anywhere, or at least
//! `overlap_local` lie within `overlap_radius` world units of the new strike. The test is
//! O(live alerts), and at most `overlap_global` alerts are raised per window, so the lists stay a
//! handful long.
//!
//! # Time
//! Everything runs on the **presented simulation tick**, not on wall time: a paused game holds its
//! pings, the speed controls scale them, and tests are deterministic. A strike is judged on the
//! tick of the frame that presents its visual event (the event carries no tick of its own), so at
//! a high game speed several ticks' strikes share the frame's tick.
//!
//! # Invariants
//! - Every list keeps its capacity: a steady state allocates nothing.
//! - Only strikes on the viewer's own units are read; nothing of another player raises an alert.

use bevy::prelude::*;
use sim_types::EntityId;

use super::style::{AlertStyle, MinimapStyle};

/// Which "under attack" alert a strike raised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// One of the viewer's structures was struck (5.0.14's `AttackTown`).
    Town,
    /// One of the viewer's units was struck (5.0.14's `AttackUnit`).
    Unit,
}

/// One live alert.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alert {
    /// Structure or unit.
    pub kind: AlertKind,
    /// Where the struck unit stood, map point in world units.
    pub at: Vec2,
    /// Presented tick the alert was raised on.
    pub tick: u32,
}

/// One ring on the minimap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ping {
    /// Its centre, map point in world units.
    pub at: Vec2,
    /// Presented tick it started on.
    pub tick: u32,
    /// How many ticks it plays.
    pub ticks: u32,
    /// Linear RGBA.
    pub color: [f32; 4],
}

/// The viewer's alerts, pings and struck units (see the module docs).
///
// PERFORMANCE RATIONALE:
// Why: Strikes arrive every tick of a fight, and the charter forbids steady-state allocation.
// What: Three retained vectors, pruned in place; the struck list is sorted by entity so an upsert
// and a lookup are binary searches.
// Trade-off: `struck` holds at most the viewer's units, a few hundred pairs of 8 bytes.
#[derive(Resource, Debug, Default)]
pub struct AlertLog {
    alerts: Vec<Alert>,
    pings: Vec<Ping>,
    struck: Vec<(EntityId, u32)>,
    /// The alert Space jumps to next, counted back from the newest.
    cursor: Option<usize>,
    /// Armed by the ping key: the next minimap left press pings.
    ping_armed: bool,
    /// Bumped whenever a list changed, so the marks rebuild.
    generation: u64,
}

impl AlertLog {
    /// The live alerts, oldest first.
    #[must_use]
    pub fn alerts(&self) -> &[Alert] {
        &self.alerts
    }

    /// The live pings, oldest first.
    #[must_use]
    pub fn pings(&self) -> &[Ping] {
        &self.pings
    }

    /// Bumped whenever an alert, a ping or a struck unit was added or pruned.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the ping key armed a ping for the next minimap press.
    #[must_use]
    pub const fn ping_armed(&self) -> bool {
        self.ping_armed
    }

    /// Arms (or disarms) a ping for the next minimap press.
    pub fn arm_ping(&mut self, armed: bool) {
        self.ping_armed = armed;
    }

    /// Whether `id` was struck within `flash_ticks` before `tick`.
    #[must_use]
    pub fn flashes(&self, id: EntityId, tick: u32, flash_ticks: u32) -> bool {
        self.struck
            .binary_search_by_key(&id, |(entity, _)| *entity)
            .ok()
            .and_then(|index| self.struck.get(index))
            .is_some_and(|(_, struck)| tick.saturating_sub(*struck) < flash_ticks)
    }

    /// Forgets everything, for a new session.
    pub fn clear(&mut self) {
        self.alerts.clear();
        self.pings.clear();
        self.struck.clear();
        self.cursor = None;
        self.ping_armed = false;
        self.generation += 1;
    }

    /// Records a strike on the viewer's unit `id` at `at` on presented tick `tick`, and raises
    /// an alert of `kind` with its ping unless the overlap rule holds it back. Returns whether an
    /// alert was raised.
    pub fn record_strike(
        &mut self,
        style: &AlertStyle,
        id: EntityId,
        kind: AlertKind,
        at: Vec2,
        tick: u32,
    ) -> bool {
        match self.struck.binary_search_by_key(&id, |(entity, _)| *entity) {
            Ok(index) => self.struck[index].1 = tick,
            Err(index) => self.struck.insert(index, (id, tick)),
        }
        self.generation += 1;
        let attack = &style.attack;
        let window = self
            .alerts
            .iter()
            .filter(|alert| tick.saturating_sub(alert.tick) < attack.overlap_ticks);
        let (mut global, mut local) = (0_u32, 0_u32);
        for alert in window {
            global += 1;
            local += u32::from(alert.at.distance(at) <= attack.overlap_radius);
        }
        if global >= attack.overlap_global || local >= attack.overlap_local {
            return false;
        }
        self.alerts.push(Alert { kind, at, tick });
        self.pings.push(Ping {
            at,
            tick,
            ticks: attack.ping_ticks,
            color: attack.color.linear,
        });
        self.cursor = None;
        true
    }

    /// Adds the player's own ping at `at` on tick `tick`.
    pub fn ping(&mut self, style: &AlertStyle, at: Vec2, tick: u32) {
        self.pings.push(Ping {
            at,
            tick,
            ticks: style.ping.ping_ticks,
            color: style.ping.color.linear,
        });
        self.generation += 1;
    }

    /// Drops what ended by presented tick `tick`: alerts older than `life_ticks`, finished pings
    /// and struck units past `flash_ticks`.
    pub fn prune(&mut self, style: &MinimapStyle, tick: u32) {
        let (before_alerts, before_pings, before_struck) =
            (self.alerts.len(), self.pings.len(), self.struck.len());
        let life = style.alerts.attack.life_ticks;
        self.alerts
            .retain(|alert| tick.saturating_sub(alert.tick) < life);
        self.pings
            .retain(|ping| tick.saturating_sub(ping.tick) < ping.ticks);
        let flash = style.flash_ticks;
        self.struck
            .retain(|(_, struck)| tick.saturating_sub(*struck) < flash);
        if (self.alerts.len(), self.pings.len(), self.struck.len())
            != (before_alerts, before_pings, before_struck)
        {
            self.generation += 1;
            if self
                .cursor
                .is_some_and(|cursor| cursor >= self.alerts.len())
            {
                self.cursor = None;
            }
        }
    }

    /// Where Space jumps: the newest live alert, then each older one on further presses,
    /// wrapping (S10). `None` when no alert lives.
    pub fn next_jump(&mut self) -> Option<Vec2> {
        let count = self.alerts.len();
        if count == 0 {
            return None;
        }
        let step = self.cursor.map_or(0, |cursor| (cursor + 1) % count);
        self.cursor = Some(step);
        self.alerts.get(count - 1 - step).map(|alert| alert.at)
    }

    /// Every growable buffer, as `(name, address, capacity in elements)`, for the steady-state
    /// tests. Not part of the log's interface.
    #[doc(hidden)]
    pub fn retained_buffers(&self) -> impl Iterator<Item = (&'static str, usize, usize)> + '_ {
        [
            (
                "alerts.alerts",
                self.alerts.as_ptr().addr(),
                self.alerts.capacity(),
            ),
            (
                "alerts.pings",
                self.pings.as_ptr().addr(),
                self.pings.capacity(),
            ),
            (
                "alerts.struck",
                self.struck.as_ptr().addr(),
                self.struck.capacity(),
            ),
        ]
        .into_iter()
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;
    use crate::minimap::style::MINIMAP_STYLE_PATH;

    fn style() -> MinimapStyle {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(MINIMAP_STYLE_PATH);
        let text = std::fs::read_to_string(&path).expect("the shipped style");
        MinimapStyle::from_text(&path, &text).expect("the shipped style is valid")
    }

    #[test]
    fn the_overlap_rule_holds_local_and_global_repeats() {
        let style = style();
        let alerts = &style.alerts;
        let mut log = AlertLog::default();
        let here = Vec2::new(10.0, 10.0);
        assert!(log.record_strike(alerts, EntityId(1), AlertKind::Unit, here, 100));
        let near = here + Vec2::new(alerts.attack.overlap_radius - 1.0, 0.0);
        assert!(!log.record_strike(alerts, EntityId(2), AlertKind::Unit, near, 110));
        let far = here + Vec2::new(alerts.attack.overlap_radius * 3.0, 0.0);
        assert!(log.record_strike(alerts, EntityId(3), AlertKind::Town, far, 120));
        let elsewhere = here + Vec2::new(0.0, alerts.attack.overlap_radius * 3.0);
        assert!(!log.record_strike(alerts, EntityId(4), AlertKind::Unit, elsewhere, 130));
        let later = 100 + alerts.attack.overlap_ticks;
        assert!(log.record_strike(alerts, EntityId(4), AlertKind::Unit, elsewhere, later));
        assert_eq!(log.alerts().len(), 3);
    }

    #[test]
    fn space_cycles_newest_to_oldest_and_wraps() {
        let style = style();
        let mut log = AlertLog::default();
        let (a, b) = (Vec2::new(1.0, 1.0), Vec2::new(60.0, 60.0));
        log.record_strike(&style.alerts, EntityId(1), AlertKind::Unit, a, 0);
        log.record_strike(&style.alerts, EntityId(2), AlertKind::Unit, b, 1);
        assert_eq!(log.next_jump(), Some(b));
        assert_eq!(log.next_jump(), Some(a));
        assert_eq!(log.next_jump(), Some(b));
    }

    #[test]
    fn pings_alerts_and_flashes_end_on_their_ticks() {
        let style = style();
        let mut log = AlertLog::default();
        log.record_strike(&style.alerts, EntityId(5), AlertKind::Unit, Vec2::ZERO, 10);
        assert!(log.flashes(EntityId(5), 10 + style.flash_ticks - 1, style.flash_ticks));
        assert!(!log.flashes(EntityId(5), 10 + style.flash_ticks, style.flash_ticks));
        log.prune(&style, 10 + style.alerts.attack.ping_ticks - 1);
        assert_eq!(log.pings().len(), 1);
        log.prune(&style, 10 + style.alerts.attack.ping_ticks);
        assert_eq!(log.pings(), []);
        assert_eq!(log.alerts().len(), 1);
        log.prune(&style, 10 + style.alerts.attack.life_ticks);
        assert_eq!(log.alerts(), []);
    }
}
