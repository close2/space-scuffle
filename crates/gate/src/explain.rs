//! `cargo gate explain`: the selection as text, with the rule behind every target.
//!
//! # Responsibility
//! Renders a [`Selection`] for a reader: the changed paths and what covers each, why the complete
//! suite was chosen if it was, the clippy scope, every target with its class and reasons, and the
//! extra commands.
//!
//! # How it fits
//! `explain` runs nothing; it is how an agent checks what `package` will run before running it,
//! and how the history replay of WP4 compares the catalog with past red runs. The runner prints
//! the same text before it starts.
//!
//! # Invariants
//! - The output is a pure function of the selection; targets are listed in catalog name order
//!   within each class, classes from `fast` to `valgrind`.

use crate::catalog::TestClass;
use crate::select::{ClippyScope, Selection};
use std::fmt::Write as _;

/// The selection as a report; `what` names the change (for example `changes since main`).
#[must_use]
pub fn render_selection(selection: &Selection, what: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "cargo gate: {} tier, {what} ({} changed path(s))",
        selection.tier,
        selection.paths.len()
    );
    if !selection.paths.is_empty() {
        out.push_str("\nChanged paths:\n");
        for path in &selection.paths {
            let _ = writeln!(out, "  {}  <- {}", path.path, path.covered_by.join("; "));
        }
    }
    out.push('\n');
    if let Some(cause) = &selection.complete {
        let _ = writeln!(out, "Complete suite: {cause}.");
    } else if selection.docs_only {
        out.push_str("Docs only: fmt and the charter checks.\n");
    }
    if !selection.changed_crates.is_empty() {
        let crates: Vec<&str> = selection
            .changed_crates
            .iter()
            .map(String::as_str)
            .collect();
        let _ = writeln!(out, "Changed crates: {}.", crates.join(", "));
    }
    let clippy = match &selection.clippy {
        ClippyScope::Skip => "none".to_string(),
        ClippyScope::Workspace => "whole workspace, all targets".to_string(),
        ClippyScope::Packages(crates) => {
            let list: Vec<&str> = crates.iter().map(String::as_str).collect();
            format!("{}, all targets", list.join(", "))
        }
    };
    let _ = writeln!(out, "Clippy: {clippy}.");
    let _ = writeln!(out, "\nTargets ({}):", selection.targets.len());
    let width = selection
        .targets
        .keys()
        .map(|id| id.to_string().len())
        .max()
        .unwrap_or(0);
    for class in TestClass::ALL {
        for (id, selected) in selection.targets.iter().filter(|(_, s)| s.class == class) {
            let reasons: Vec<String> = selected.reasons.iter().map(ToString::to_string).collect();
            let _ = writeln!(
                out,
                "  {:<width$}  {:<8}  {}",
                id.to_string(),
                class.name(),
                reasons.join("; ")
            );
        }
    }
    if !selection.commands.is_empty() {
        out.push_str("\nCommands:\n");
        for (name, reasons) in &selection.commands {
            let reasons: Vec<String> = reasons.iter().map(ToString::to_string).collect();
            let _ = writeln!(out, "  {name}  {}", reasons.join("; "));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::{PathMatch, Reason, Selected};
    use crate::tier::Tier;
    use crate::workspace::TargetId;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn the_report_names_paths_classes_and_reasons() {
        let selection = Selection {
            tier: Tier::Package,
            paths: vec![PathMatch {
                path: "crates/core/src/a.rs".into(),
                covered_by: vec!["crate core".into()],
            }],
            complete: None,
            docs_only: false,
            changed_crates: BTreeSet::from(["core".to_string()]),
            targets: BTreeMap::from([
                (
                    TargetId::test("app", "gpu_tests"),
                    Selected {
                        class: TestClass::Gpu,
                        reasons: BTreeSet::from([Reason::Downstream("core".into())]),
                    },
                ),
                (
                    TargetId::lib("core"),
                    Selected {
                        class: TestClass::Fast,
                        reasons: BTreeSet::from([Reason::Crate("core".into())]),
                    },
                ),
            ]),
            commands: BTreeMap::new(),
            clippy: ClippyScope::Workspace,
        };
        let text = render_selection(&selection, "changes since main");
        assert!(text.contains("package tier, changes since main (1 changed path(s))"));
        assert!(text.contains("crates/core/src/a.rs  <- crate core"));
        let fast = text.find("core/lib").unwrap_or(usize::MAX);
        let gpu = text.find("app/gpu_tests").unwrap_or(0);
        assert!(
            fast < gpu,
            "fast targets are listed before gpu ones:\n{text}"
        );
        assert!(text.contains("downstream of core"));
        assert!(text.contains("Clippy: whole workspace"));
    }
}
