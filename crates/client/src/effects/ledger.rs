//! The budget ledger of the live effects (plan `2026-11-09-effect-visualization` §5.6,
//! Amendment 7): how many effects of each class still burn, and how many decal slots the
//! live effects hold.
//!
//! # Responsibility
//! Owns [`BudgetLedger`]: the per-class count of effects whose fire is alive (their
//! non-decal layers still draw and their detail is not retired) and the decal slots the live
//! effects reserved at birth, with the share of them held by marks that are eroding out to
//! make room. Pure counting with the admission rules ([`BudgetLedger::grant`],
//! [`BudgetLedger::needs_room`]); the runtime decides which instance gives way.
//!
//! # How it fits
//! [`super::runtime::EffectRuntime`] recounts the ledger once per frame from its pool and
//! updates it as it spawns and retires between frames. A class cap counts an effect only while
//! its fire is alive: a flame line's lasting band holds a decal reservation, not a `Major`
//! slot (independent review, scale-out round 3). Decal slots are reserved whole at birth and
//! kept to the effect's end, so the per-frame write order never decides which lasting marks
//! draw and no effect draws part of its marks.
//!
//! # Invariants
//! - `marks_retiring <= marks_reserved <= capacity`: a grant never takes the reserved slots
//!   past the decal pool, so every reserved decal finds room in the frame.
//! - Above [`MARK_PRESSURE`] of the pool, room is made by retiring the oldest marks before a
//!   grant, so the pool is rarely full.
//! - Nothing here allocates.

use super::catalog::schema::EffectClass;

/// Share of the decal pool above which the oldest lasting marks begin eroding out to make
/// room for new ones, so the pool itself is rarely reached.
pub const MARK_PRESSURE: f32 = 0.85;

/// Seconds over which a lasting mark retired to make room erodes out.
pub const MARK_RETIRE_SECS: f32 = 0.25;

/// Per-class fire counts and decal reservations of the live effects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BudgetLedger {
    /// Effects per class (by [`EffectClass::index`]) whose fire is alive.
    burning: [u32; 3],
    /// Decal slots held by live effects.
    marks_reserved: u32,
    /// Of those, the slots of marks eroding out (freed when their erosion ends).
    marks_retiring: u32,
}

impl BudgetLedger {
    /// Effects of `class` whose fire is alive.
    #[must_use]
    pub const fn burning(&self, class: EffectClass) -> u32 {
        self.burning[class.index()]
    }

    /// Decal slots held by live effects.
    #[must_use]
    pub const fn marks_reserved(&self) -> u32 {
        self.marks_reserved
    }

    /// Decal slots held by marks eroding out.
    #[must_use]
    pub const fn marks_retiring(&self) -> u32 {
        self.marks_retiring
    }

    /// Starts a recount.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Counts one effect: of `class`, burning or not, holding `slots` decal slots, of which
    /// all are retiring when `retiring`.
    pub fn count(&mut self, class: EffectClass, burning: bool, slots: u32, retiring: bool) {
        if burning {
            self.burning[class.index()] += 1;
        }
        self.marks_reserved += slots;
        if retiring {
            self.marks_retiring += slots;
        }
    }

    /// An effect of `class` stopped burning (its detail was retired).
    pub fn quench(&mut self, class: EffectClass) {
        let count = &mut self.burning[class.index()];
        *count = count.saturating_sub(1);
    }

    /// `slots` held slots began eroding out.
    pub fn retire_marks(&mut self, slots: u32) {
        self.marks_retiring = (self.marks_retiring + slots).min(self.marks_reserved);
    }

    /// Whether `need` more slots would take the marks that are not already eroding out past
    /// [`MARK_PRESSURE`] of a pool of `capacity`: room should be made first.
    #[must_use]
    pub fn needs_room(&self, need: u32, capacity: u32) -> bool {
        let settled = self.marks_reserved - self.marks_retiring;
        #[expect(clippy::cast_precision_loss, reason = "pool sizes are far below 2^24")]
        let pressure = capacity as f32 * MARK_PRESSURE;
        #[expect(clippy::cast_precision_loss, reason = "pool sizes are far below 2^24")]
        let wanted = (settled + need) as f32;
        wanted > pressure
    }

    /// Reserves `need` slots of a pool of `capacity` for a new effect of `class` that burns
    /// when `burning`: all of them, or none when they would not fit beside the slots held
    /// (an effect draws its marks whole or not at all). Returns the slots granted.
    pub fn grant(&mut self, class: EffectClass, burning: bool, need: u32, capacity: u32) -> u32 {
        if burning {
            self.burning[class.index()] += 1;
        }
        if need == 0 || self.marks_reserved + need > capacity {
            return 0;
        }
        self.marks_reserved += need;
        need
    }
}

/// Opacity of a lasting mark `retired_for` seconds after it was retired to make room: 1 down
/// to 0 over [`MARK_RETIRE_SECS`].
#[must_use]
pub fn mark_fade(retired_for: f32) -> f32 {
    (1.0 - retired_for / MARK_RETIRE_SECS).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_are_whole_or_nothing_and_never_pass_the_pool() {
        let mut ledger = BudgetLedger::default();
        assert_eq!(ledger.grant(EffectClass::Minor, false, 6, 10), 6);
        assert_eq!(
            ledger.grant(EffectClass::Minor, false, 5, 10),
            0,
            "5 do not fit beside 6"
        );
        assert_eq!(ledger.grant(EffectClass::Minor, false, 4, 10), 4);
        assert_eq!(ledger.marks_reserved(), 10);
        assert_eq!(ledger.grant(EffectClass::Minor, false, 0, 10), 0);
    }

    #[test]
    fn pressure_asks_for_room_before_the_pool_is_full() {
        let mut ledger = BudgetLedger::default();
        ledger.count(EffectClass::Minor, false, 80, false);
        assert!(
            !ledger.needs_room(5, 100),
            "85 of 100 is at the pressure mark"
        );
        assert!(ledger.needs_room(6, 100), "86 of 100 is above it");
        ledger.retire_marks(10);
        assert!(
            !ledger.needs_room(6, 100),
            "eroding marks are already making room"
        );
        assert_eq!(ledger.marks_retiring(), 10);
    }

    #[test]
    fn burning_effects_are_counted_per_class_and_quenched() {
        let mut ledger = BudgetLedger::default();
        ledger.count(EffectClass::Major, true, 0, false);
        ledger.count(EffectClass::Major, false, 3, false);
        assert_eq!(
            ledger.burning(EffectClass::Major),
            1,
            "a lasting mark does not burn"
        );
        assert_eq!(ledger.grant(EffectClass::Major, true, 0, 10), 0);
        assert_eq!(ledger.burning(EffectClass::Major), 2);
        ledger.quench(EffectClass::Major);
        assert_eq!(ledger.burning(EffectClass::Major), 1);
        assert_eq!(ledger.burning(EffectClass::Minor), 0);
    }

    #[test]
    fn a_retired_mark_erodes_out_without_a_pop() {
        assert!((mark_fade(0.0) - 1.0).abs() < f32::EPSILON);
        assert!((mark_fade(MARK_RETIRE_SECS * 0.5) - 0.5).abs() < 1e-6);
        assert!(mark_fade(MARK_RETIRE_SECS).abs() < f32::EPSILON);
    }
}
