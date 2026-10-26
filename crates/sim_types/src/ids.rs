//! Strongly-typed newtype identifiers for entities, players, teams, and ticks.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Monotonic tick counter for the simulation.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct Tick(pub u32);

impl Tick {
    /// Initial zero tick.
    pub const ZERO: Self = Self(0);

    /// Advance tick by one.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// Ticks elapsed between `self` and an earlier tick.
    #[must_use]
    pub const fn duration_since(self, earlier: Self) -> u32 {
        self.0.saturating_sub(earlier.0)
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tick({})", self.0)
    }
}

/// Unique deterministic entity handle.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct EntityId(pub u32);

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entity({})", self.0)
    }
}

/// Zero-indexed player slot identifier (0..=15, 255 = Neutral/Hostile).
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct PlayerId(pub u8);

impl PlayerId {
    /// Slot assigned to neutral map entities (minerals, destructible rocks, critters).
    pub const NEUTRAL: Self = Self(255);
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Player({})", self.0)
    }
}

/// Alliance team group identifier (0 = Neutral, 1 = Team 1, 2 = Team 2).
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct TeamId(pub u8);

impl TeamId {
    /// Neutral team.
    pub const NEUTRAL: Self = Self(0);
}

impl fmt::Display for TeamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Team({})", self.0)
    }
}

/// Unit archetype identifier indexing into the static Unit Catalog.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct UnitTypeId(pub u16);

impl fmt::Display for UnitTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UnitType({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_id_traits() {
        let e1 = EntityId(1);
        let e2 = EntityId(2);
        assert!(e1 < e2);

        let mut set = BTreeSet::new();
        set.insert(e2);
        set.insert(e1);
        let ordered: Vec<_> = set.into_iter().collect();
        assert_eq!(ordered, vec![e1, e2]);

        assert_eq!(e1.to_string(), "Entity(1)");
        assert_eq!(PlayerId(3).to_string(), "Player(3)");
        assert_eq!(PlayerId::NEUTRAL.to_string(), "Player(255)");
        assert_eq!(TeamId(1).to_string(), "Team(1)");
        assert_eq!(TeamId::NEUTRAL.to_string(), "Team(0)");
        assert_eq!(UnitTypeId(42).to_string(), "UnitType(42)");
    }

    #[test]
    fn test_tick_math() {
        let t0 = Tick::ZERO;
        assert_eq!(t0.0, 0);

        let t1 = t0.next();
        assert_eq!(t1.0, 1);
        assert_eq!(t1.duration_since(t0), 1);
        assert_eq!(t0.duration_since(t1), 0); // saturating
    }
}
