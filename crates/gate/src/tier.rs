//! The four gate tiers and their names on the command line.
//!
//! # Responsibility
//! Owns [`Tier`], the closed set of gates `cargo gate` knows (blueprint §3.3), and its spelling
//! as a command-line word and in stamps.
//!
//! # How it fits
//! The selection (`select.rs`) decides per tier which targets run; the runner (`runner.rs`)
//! decides per tier which fixed steps run (fmt, clippy, doc tests, catalog commands). The charter
//! check C16 reads this enum as text (blueprint §9.2), so its variants stay one per
//! line with their lower-case names in [`Tier::name`].
//!
//! # Invariants
//! - [`Tier::name`] and [`Tier::parse`] are inverse on every variant.

use serde::{Deserialize, Serialize};
use std::fmt;

/// One gate tier, from the cheapest to the complete suite (blueprint §3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    /// Optional inner loop: fmt, clippy on the changed crates, their `fast` targets.
    Quick,
    /// Required once per implementation package: the catalog's selection for the change.
    Package,
    /// The orchestrator's check of one merge on `main`.
    Merge,
    /// The complete suite, as `cargo test --workspace` runs it, plus the golden replays.
    Full,
}

impl Tier {
    /// Every tier, cheapest first.
    pub const ALL: [Self; 4] = [Self::Quick, Self::Package, Self::Merge, Self::Full];

    /// The tier's command-line word (`quick`, `package`, `merge`, `full`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Package => "package",
            Self::Merge => "merge",
            Self::Full => "full",
        }
    }

    /// The tier a command-line word names, or `None`.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tier| tier.name() == word)
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_parse_are_inverse() {
        for tier in Tier::ALL {
            assert_eq!(Tier::parse(tier.name()), Some(tier));
        }
        assert_eq!(Tier::parse("everything"), None);
    }
}
