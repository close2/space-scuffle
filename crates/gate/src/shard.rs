//! Shards of the `full` tier: the complete suite split across parallel CI jobs.
//!
//! # Responsibility
//! Parses a shard `<k>/<n>` ([`Shard`]) and narrows the selection of `full` to the targets that
//! shard runs ([`shard_selection`]). The catalog assigns a target to a shard `k < n` with the
//! `shard: Some(k)` field of its `targets` entry; the last shard, `n`, runs every target the
//! catalog assigns to no other shard, and alone runs clippy, the doc tests and the catalog
//! commands (the golden replays). The `n` shards together run the complete suite, each target
//! once.
//!
//! # How it fits
//! CI runs one job per shard (`.github/workflows/ci.yml`), each `cargo gate full` with
//! [`crate::SHARD_VAR`] set to `<k>/<n>`: on one 4-CPU runner the lavapipe GPU tests alone
//! outgrew the job's time limit. `runner.rs` narrows the selection after selecting and before
//! building, so a shard builds only its own targets; a shard writes no stamp, because no single
//! shard tested the whole tree. A shard red only by a flake repeats itself once, as a whole
//! `full` run does.
//!
//! # Invariants
//! - S-1: `1 <= k <= n`.
//! - S-2: every shard number the catalog assigns is below `n`, and every shard below `n` has at
//!   least one assigned target. Anything else is an error, never a shard that silently runs
//!   nothing or an assigned target that no shard runs.
//! - S-3: the shards partition the selection: each selected target is in exactly one shard.

use crate::catalog::SuiteCatalog;
use crate::select::{ClippyScope, Selection};
use crate::GateError;
use std::collections::BTreeSet;
use std::fmt;

/// One shard of a sharded `full` run: shard `index` of `count`, both counted from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shard {
    /// Which shard, from 1.
    index: u32,
    /// How many shards the complete suite is split into.
    count: u32,
}

impl Shard {
    /// Parses `<k>/<n>` with `1 <= k <= n`.
    ///
    /// # Errors
    /// [`GateError::Usage`] for any other text.
    pub fn parse(text: &str) -> Result<Self, GateError> {
        let invalid =
            || GateError::Usage(format!("a shard is <k>/<n> with 1 <= k <= n, not `{text}`"));
        let (index, count) = text.split_once('/').ok_or_else(invalid)?;
        let index: u32 = index.parse().map_err(|_| invalid())?;
        let count: u32 = count.parse().map_err(|_| invalid())?;
        if index == 0 || index > count {
            return Err(invalid());
        }
        Ok(Self { index, count })
    }

    /// Which shard this is, from 1.
    #[must_use]
    pub fn index(self) -> u32 {
        self.index
    }

    /// How many shards there are.
    #[must_use]
    pub fn count(self) -> u32 {
        self.count
    }

    /// Whether this is the last shard: the one that runs every unassigned target and the steps.
    #[must_use]
    pub fn is_last(self) -> bool {
        self.index == self.count
    }

    /// Whether this shard runs a target the catalog assigns to `assigned` (`None`: no shard).
    #[must_use]
    pub fn runs(self, assigned: Option<u32>) -> bool {
        assigned.map_or(self.is_last(), |shard| shard == self.index)
    }
}

impl fmt::Display for Shard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.index, self.count)
    }
}

/// Checks invariant S-2 for a run of `count` shards.
fn check_assignments(catalog: &SuiteCatalog, count: u32) -> Result<(), GateError> {
    let assigned: BTreeSet<u32> = catalog
        .targets
        .values()
        .filter_map(|entry| entry.shard)
        .collect();
    if let Some(beyond) = assigned.iter().find(|&&shard| shard >= count) {
        return Err(GateError::Catalog(format!(
            "the catalog assigns targets to shard {beyond}, but a run of {count} shard(s) \
             assigns targets only to shards 1 to {}; shard {count} runs every unassigned target",
            count - 1
        )));
    }
    if let Some(empty) = (1..count).find(|shard| !assigned.contains(shard)) {
        return Err(GateError::Catalog(format!(
            "shard {empty} of {count} would run nothing: the catalog assigns it no target"
        )));
    }
    Ok(())
}

/// The part of `selection` that `shard` runs: the targets assigned to it, or for the last shard
/// every target assigned to no other shard together with clippy and the catalog commands.
///
/// # Errors
/// [`GateError::Catalog`] when the catalog's assignments break invariant S-2 for `shard`'s
/// count.
pub fn shard_selection(
    mut selection: Selection,
    catalog: &SuiteCatalog,
    shard: Shard,
) -> Result<Selection, GateError> {
    check_assignments(catalog, shard.count)?;
    selection
        .targets
        .retain(|id, _| shard.runs(catalog.shard_of(id)));
    if !shard.is_last() {
        selection.commands.clear();
        selection.clippy = ClippyScope::Skip;
    }
    Ok(selection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TestClass;
    use crate::select::{Reason, Selected};
    use crate::tier::Tier;
    use crate::workspace::TargetId;
    use std::collections::BTreeMap;
    use std::path::Path;

    const CATALOG: &str = r#"SuiteCatalog(
        global: ["Cargo.toml"],
        docs: ["docs/**"],
        targets: {
            "app/big_gpu_tests": (class: Gpu, shard: Some(1)),
            "app/other_gpu_tests": (class: Gpu, shard: Some(2)),
            "app/small_gpu_tests": (class: Gpu),
            "app/lib": (class: Slow),
        },
        commands: [(name: "golden", paths: ["crates/core/**"], program: "cargo", args: ["test"])],
    )"#;

    fn catalog(text: &str) -> Result<SuiteCatalog, GateError> {
        SuiteCatalog::parse(text, Path::new("suites.ron"))
    }

    fn full_selection() -> Selection {
        let target = |name: &str, class| {
            let id = if name == "lib" {
                TargetId::lib("app")
            } else {
                TargetId::test("app", name)
            };
            let reasons = BTreeSet::from([Reason::Complete]);
            (id, Selected { class, reasons })
        };
        Selection {
            tier: Tier::Full,
            paths: Vec::new(),
            complete: None,
            docs_only: false,
            changed_crates: BTreeSet::new(),
            targets: BTreeMap::from([
                target("big_gpu_tests", TestClass::Gpu),
                target("other_gpu_tests", TestClass::Gpu),
                target("small_gpu_tests", TestClass::Gpu),
                target("lib", TestClass::Slow),
                target("fast_tests", TestClass::Fast),
            ]),
            commands: BTreeMap::from([("golden".to_string(), BTreeSet::from([Reason::Complete]))]),
            clippy: ClippyScope::Workspace,
        }
    }

    fn names(selection: &Selection) -> Vec<String> {
        selection.targets.keys().map(ToString::to_string).collect()
    }

    #[test]
    fn shards_parse_and_print_as_k_of_n() -> Result<(), GateError> {
        let shard = Shard::parse("2/3")?;
        assert_eq!((shard.index(), shard.count()), (2, 3));
        assert_eq!(shard.to_string(), "2/3");
        assert!(!shard.is_last() && Shard::parse("3/3")?.is_last());
        assert!(Shard::parse("1/1")?.is_last());
        for bad in [
            "0/3", "4/3", "1/0", "-1/3", "a/3", "1/b", "13", "", "1/2/3", " 1/2",
        ] {
            assert!(
                matches!(Shard::parse(bad), Err(GateError::Usage(_))),
                "{bad:?} must be rejected"
            );
        }
        Ok(())
    }

    #[test]
    fn assigned_targets_run_in_their_shard_and_the_rest_in_the_last() -> Result<(), GateError> {
        let catalog = catalog(CATALOG)?;
        let shards: Vec<Selection> = (1..=3)
            .map(|k| shard_selection(full_selection(), &catalog, Shard { index: k, count: 3 }))
            .collect::<Result<_, _>>()?;
        assert_eq!(names(&shards[0]), ["app/big_gpu_tests"]);
        assert_eq!(names(&shards[1]), ["app/other_gpu_tests"]);
        assert_eq!(
            names(&shards[2]),
            ["app/lib", "app/fast_tests", "app/small_gpu_tests"]
        );
        // S-3: together the shards run every selected target exactly once.
        let mut union: Vec<String> = shards.iter().flat_map(names).collect();
        union.sort();
        let mut whole = names(&full_selection());
        whole.sort();
        assert_eq!(union, whole);
        Ok(())
    }

    #[test]
    fn only_the_last_shard_keeps_clippy_and_the_commands() -> Result<(), GateError> {
        let catalog = catalog(CATALOG)?;
        let first = shard_selection(full_selection(), &catalog, Shard::parse("1/3")?)?;
        assert!(first.commands.is_empty());
        assert_eq!(first.clippy, ClippyScope::Skip);
        let last = shard_selection(full_selection(), &catalog, Shard::parse("3/3")?)?;
        assert!(last.commands.contains_key("golden"));
        assert_eq!(last.clippy, ClippyScope::Workspace);
        Ok(())
    }

    #[test]
    fn a_shard_count_that_does_not_fit_the_catalog_is_an_error() -> Result<(), GateError> {
        let assigned = catalog(CATALOG)?;
        // Shard 2 is assigned, so two shards would leave `other_gpu_tests` in none of them.
        assert!(matches!(
            shard_selection(full_selection(), &assigned, Shard::parse("1/2")?),
            Err(GateError::Catalog(why)) if why.contains("shard 2")
        ));
        // Four shards would leave shard 3 empty.
        assert!(matches!(
            shard_selection(full_selection(), &assigned, Shard::parse("4/4")?),
            Err(GateError::Catalog(why)) if why.contains("shard 3 of 4")
        ));
        // One shard is the whole suite only when the catalog assigns nothing.
        let unassigned = catalog(
            &CATALOG
                .replace(", shard: Some(1)", "")
                .replace(", shard: Some(2)", ""),
        )?;
        let whole = shard_selection(full_selection(), &unassigned, Shard::parse("1/1")?)?;
        assert_eq!(whole, full_selection());
        Ok(())
    }
}
