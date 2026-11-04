//! The red rule's suspect list: which changes on `main` could have broken a failing target.
//!
//! # Responsibility
//! Implements step 1 of the red rule (blueprint §3.5). For a red `full` run, or a red CI run
//! whose log names the failing targets, it lists for each failing target the first-parent
//! changes of `main` since the last green `full` stamp whose `package` selection includes that
//! target, newest first. The pure part ([`suspects_for`], [`last_green_full`]) works on plain
//! data; [`suspect_report`] reads Git, the stamps and the catalog.
//!
//! # How it fits
//! The runner appends the report to a red `full` run; `cargo gate suspects` prints it for a CI
//! run. The orchestrator then stops merging packages that select the failing crate, and fixes
//! forward or reverts the suspect within an hour (charter §4.3). A failing target with no
//! suspect is a catalog miss or a flake: the report says so, because then the catalog needs a
//! rule (blueprint §3.5, step 4).
//!
//! # Invariants
//! - A change is judged by the same `package` selection an agent's gate would have made for it
//!   (`select` on `<change>^1..<change>`), so the suspect list and the catalog cannot disagree.
//! - A root commit, which has no parent to compare with, is a suspect for every target.
//! - Without a green `full` stamp that is an ancestor of `HEAD`, the search covers the
//!   [`HISTORY_LIMIT`] newest first-parent changes and says so.

use crate::changes::ChangeSpec;
use crate::runner::Context;
use crate::select::select;
use crate::stamp::Stamp;
use crate::tier::Tier;
use crate::workspace::TargetId;
use crate::GateError;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// How many first-parent changes are searched when no green `full` stamp bounds the search.
pub const HISTORY_LIMIT: usize = 50;

/// One first-parent change of `main` and the targets its `package` selection runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The commit (a merge, or a direct commit on `main`).
    pub commit: String,
    /// Its subject line.
    pub subject: String,
    /// The targets `cargo gate package` selects for `<commit>^1..<commit>`.
    pub selected: BTreeSet<TargetId>,
}

/// For each of `failing`, the changes (ordered newest first, as `changes` is) whose selection
/// includes it. Every failing target has an entry, possibly empty.
#[must_use]
pub fn suspects_for<'a>(
    failing: &BTreeSet<TargetId>,
    changes: &'a [Change],
) -> BTreeMap<TargetId, Vec<&'a Change>> {
    failing
        .iter()
        .map(|target| {
            let suspects = changes
                .iter()
                .filter(|change| change.selected.contains(target))
                .collect();
            (target.clone(), suspects)
        })
        .collect()
}

/// The newest green `full` stamp among `stamps` (oldest first) whose commit `is_ancestor` of the
/// checked-out commit.
///
/// # Errors
/// Whatever `is_ancestor` returns.
pub fn last_green_full(
    stamps: &[Stamp],
    mut is_ancestor: impl FnMut(&str) -> Result<bool, GateError>,
) -> Result<Option<&Stamp>, GateError> {
    for stamp in stamps.iter().rev().filter(|stamp| stamp.tier == Tier::Full) {
        if stamp.targets.iter().all(|target| target.passed) && is_ancestor(&stamp.commit)? {
            return Ok(Some(stamp));
        }
    }
    Ok(None)
}

/// The first-parent changes of `HEAD` after `since` (or the [`HISTORY_LIMIT`] newest), newest
/// first, each with its `package` selection.
///
/// # Errors
/// Git, catalog and selection errors.
pub fn changes_since(context: &Context, since: Option<&str>) -> Result<Vec<Change>, GateError> {
    let git = &context.git;
    let every: BTreeSet<TargetId> = context
        .workspace
        .test_targets()
        .map(|target| target.id.clone())
        .collect();
    let mut changes = Vec::new();
    for (commit, subject) in git.first_parent_history(since, "HEAD", HISTORY_LIMIT)? {
        let selected = match git.commit(&format!("{commit}^1")) {
            Ok(parent) => {
                let paths = git.changed_paths(&ChangeSpec::Between(parent, commit.clone()))?;
                select(Tier::Package, context.inputs(), &paths)?
                    .targets
                    .into_keys()
                    .collect()
            }
            Err(GateError::CommandFailed { .. }) => every.clone(),
            Err(error) => return Err(error),
        };
        changes.push(Change {
            commit,
            subject,
            selected,
        });
    }
    Ok(changes)
}

/// The suspect list for `failing`, searching from `base` if given, else from the last green
/// `full` stamp in `stamps`, else over the newest [`HISTORY_LIMIT`] changes.
///
/// # Errors
/// Git, catalog and selection errors.
pub fn suspect_report(
    context: &Context,
    failing: &BTreeSet<TargetId>,
    base: Option<&str>,
    stamps: &[Stamp],
) -> Result<String, GateError> {
    let green = match base {
        Some(_) => None,
        None => last_green_full(stamps, |commit| context.git.is_ancestor(commit, "HEAD"))?,
    };
    let since = base.or(green.map(|stamp| stamp.commit.as_str()));
    let scope = match (base, green) {
        (Some(base), _) => format!("changes on the first-parent line after {base}"),
        (None, Some(stamp)) => format!(
            "changes since the last green full stamp, at {}",
            short(&stamp.commit)
        ),
        (None, None) => format!(
            "no green full stamp is an ancestor of HEAD: the {HISTORY_LIMIT} newest changes"
        ),
    };
    let changes = changes_since(context, since)?;
    Ok(render_suspects(&suspects_for(failing, &changes), &scope))
}

/// The first 12 characters of a hash.
fn short(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

/// The suspect list as text, with the red rule's next steps.
#[must_use]
pub fn render_suspects(suspects: &BTreeMap<TargetId, Vec<&Change>>, scope: &str) -> String {
    let mut out = format!("\nRed rule, suspects ({scope}; newest first):\n");
    for (target, changes) in suspects {
        let _ = writeln!(out, "  {target}");
        if changes.is_empty() {
            out.push_str(
                "    no change selects it: a catalog miss (add the rule to crates/gate/suites.ron \
                 with the fix) or a flake\n",
            );
        }
        for change in changes {
            let _ = writeln!(out, "    {}  {}", short(&change.commit), change.subject);
        }
    }
    out.push_str(
        "Stop merging packages whose selection includes a failing crate; fix forward or \
         `git revert -m 1 <merge>` within an hour (charter §4.3).\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TestClass;
    use crate::stamp::StampTarget;

    fn change(commit: &str, targets: &[TargetId]) -> Change {
        Change {
            commit: commit.to_string(),
            subject: format!("Merge {commit}"),
            selected: targets.iter().cloned().collect(),
        }
    }

    #[test]
    fn each_failing_target_lists_the_changes_that_select_it_newest_first() {
        let core = TargetId::test("core", "a_tests");
        let app = TargetId::lib("app");
        let lost = TargetId::test("app", "lost_tests");
        let changes = [
            change("c3", std::slice::from_ref(&core)),
            change("c2", std::slice::from_ref(&app)),
            change("c1", &[core.clone(), app.clone()]),
        ];
        let failing = BTreeSet::from([core.clone(), app.clone(), lost.clone()]);
        let found = suspects_for(&failing, &changes);
        let commits =
            |id: &TargetId| -> Vec<&str> { found[id].iter().map(|c| c.commit.as_str()).collect() };
        assert_eq!(commits(&core), ["c3", "c1"]);
        assert_eq!(commits(&app), ["c2", "c1"]);
        assert_eq!(found[&lost], Vec::<&Change>::new());
        let text = render_suspects(&found, "test");
        assert!(text.contains("  app/lost_tests\n    no change selects it: a catalog miss"));
        assert!(
            text.contains("    c3  Merge c3\n    c1  Merge c1\n"),
            "{text}"
        );
    }

    fn full(commit: &str, finished: u64, passed: bool) -> Stamp {
        Stamp {
            tier: Tier::Full,
            tree: format!("tree-{commit}"),
            commit: commit.to_string(),
            base: None,
            host: String::new(),
            finished_unix_s: finished,
            targets: vec![StampTarget {
                target: "a/lib".into(),
                class: TestClass::Fast,
                wall_ms: 1,
                cpu_ms: None,
                passed,
            }],
            commands: Vec::new(),
            flakes: Vec::new(),
        }
    }

    #[test]
    fn the_last_green_full_stamp_must_be_an_ancestor() -> Result<(), GateError> {
        let mut package = full("p", 5, true);
        package.tier = Tier::Package;
        let stamps = [
            full("old", 1, true),
            full("other-branch", 2, true),
            full("red", 3, false),
            package,
        ];
        let ancestors = ["old", "red", "p"];
        let found = last_green_full(&stamps, |commit| Ok(ancestors.contains(&commit)))?;
        assert_eq!(found.map(|s| s.commit.as_str()), Some("old"));
        assert_eq!(last_green_full(&stamps, |_| Ok(false))?, None);
        Ok(())
    }
}
