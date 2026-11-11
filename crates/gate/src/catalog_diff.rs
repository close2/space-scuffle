//! The semantic difference between two versions of the suite catalog.
//!
//! # Responsibility
//! Compares two parsed [`SuiteCatalog`]s (the base revision's and the change's) and lists what
//! differs in the terms the selection needs (plan 2026-11-11-faster-gates §4.2): a changed
//! scope (`global`, `docs`, `always`), changed `targets` entries, changed rules with both
//! versions, downstream references gained, and changed commands. Comments, ordering and
//! formatting are invisible, because only parsed values are compared.
//!
//! # How it fits
//! `select.rs` calls [`CatalogDiff::between`] when `crates/gate/suites.ron` changed and its base
//! text is known, and turns each difference into selected targets; `explain` prints
//! [`CatalogDiff::descriptions`]. The module is pure: it reads no file.
//!
//! # Invariants
//! - Two catalogs that parse to the same values have an empty diff.
//! - A rule is matched by name: a renamed rule is one removal plus one addition.
//! - A downstream reference that was removed, or a key that was removed, is not a difference
//!   that selects anything; removing a contract cannot make a test fail.

use crate::catalog::{PathRule, SuiteCatalog, TargetEntry, TargetRef};
use crate::workspace::TargetId;
use std::collections::{BTreeMap, BTreeSet};

/// One rule that was added, removed or changed, with every reference either version makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleChange {
    /// The rule's name.
    pub name: String,
    /// The union of the old and new versions' `targets`.
    pub targets: Vec<TargetRef>,
    /// The union of the old and new versions' `crates`.
    pub crates: BTreeSet<String>,
}

/// What differs between two catalogs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogDiff {
    /// The scope lists that changed (`global`, `docs`, `always`); any one selects the complete
    /// suite.
    pub scope: Vec<&'static str>,
    /// Targets whose `targets` entry was added, removed or changed.
    pub targets: BTreeSet<TargetId>,
    /// Rules added, removed or changed.
    pub rules: Vec<RuleChange>,
    /// References a downstream contract gained, per key (a new key counts with all its values).
    pub downstream_added: BTreeMap<String, Vec<TargetRef>>,
    /// Commands added or changed, by name.
    pub commands: BTreeSet<String>,
    /// One readable line per difference, for `explain`.
    pub descriptions: Vec<String>,
}

/// A target entry's settings as text, for descriptions.
fn entry_text(entry: Option<&TargetEntry>) -> String {
    entry.map_or_else(
        || "absent".to_string(),
        |entry| {
            let triggers: Vec<String> = entry.triggers.iter().map(ToString::to_string).collect();
            let threads = entry
                .test_threads
                .map_or_else(|| "default".to_string(), |n| n.to_string());
            format!(
                "class {}, test_threads {threads}, triggers [{}]",
                entry.class,
                triggers.join(", ")
            )
        },
    )
}

/// The union of two optional rules' references, in first-seen order.
fn rule_change(name: &str, old: Option<&PathRule>, new: Option<&PathRule>) -> RuleChange {
    let mut targets: Vec<TargetRef> = Vec::new();
    let mut crates = BTreeSet::new();
    for rule in [old, new].into_iter().flatten() {
        for reference in &rule.targets {
            if !targets.contains(reference) {
                targets.push(reference.clone());
            }
        }
        crates.extend(rule.crates.iter().cloned());
    }
    RuleChange {
        name: name.to_string(),
        targets,
        crates,
    }
}

impl CatalogDiff {
    /// The difference from `before` to `after`.
    #[must_use]
    pub fn between(before: &SuiteCatalog, after: &SuiteCatalog) -> Self {
        let mut diff = Self::default();
        diff.compare_scope(before, after);
        diff.compare_targets(before, after);
        diff.compare_rules(before, after);
        diff.compare_downstream(before, after);
        diff.compare_commands(before, after);
        diff
    }

    /// Whether the two catalogs say the same thing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.descriptions.is_empty()
    }

    fn compare_scope(&mut self, before: &SuiteCatalog, after: &SuiteCatalog) {
        let lists = [
            ("global", before.global != after.global),
            ("docs", before.docs != after.docs),
            ("always", before.always != after.always),
        ];
        for (name, changed) in lists {
            if changed {
                self.scope.push(name);
                self.descriptions
                    .push(format!("{name} changed: complete suite"));
            }
        }
    }

    fn compare_targets(&mut self, before: &SuiteCatalog, after: &SuiteCatalog) {
        let ids: BTreeSet<&TargetId> = before.targets.keys().chain(after.targets.keys()).collect();
        for id in ids {
            let (old, new) = (before.targets.get(id), after.targets.get(id));
            if old != new {
                self.targets.insert(id.clone());
                self.descriptions.push(format!(
                    "targets: {id}: {} -> {}",
                    entry_text(old),
                    entry_text(new)
                ));
            }
        }
    }

    fn compare_rules(&mut self, before: &SuiteCatalog, after: &SuiteCatalog) {
        let by_name = |catalog: &SuiteCatalog| -> BTreeMap<String, PathRule> {
            catalog
                .rules
                .iter()
                .map(|rule| (rule.name.clone(), rule.clone()))
                .collect()
        };
        let (old, new) = (by_name(before), by_name(after));
        let names: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
        for name in names {
            let (was, is) = (old.get(name), new.get(name));
            if was != is {
                let change = if was.is_none() {
                    "added"
                } else if is.is_none() {
                    "removed"
                } else {
                    "changed"
                };
                self.descriptions.push(format!("rules: `{name}` {change}"));
                self.rules.push(rule_change(name, was, is));
            }
        }
    }

    fn compare_downstream(&mut self, before: &SuiteCatalog, after: &SuiteCatalog) {
        for (key, list) in &after.downstream {
            let old = before.downstream.get(key);
            let added: Vec<TargetRef> = list
                .iter()
                .filter(|reference| old.is_none_or(|old| !old.contains(reference)))
                .cloned()
                .collect();
            if !added.is_empty() {
                let names: Vec<String> = added.iter().map(ToString::to_string).collect();
                self.descriptions
                    .push(format!("downstream: `{key}` gained {}", names.join(", ")));
                self.downstream_added.insert(key.clone(), added);
            }
        }
        for (key, list) in &before.downstream {
            let new = after.downstream.get(key);
            let removed: Vec<String> = list
                .iter()
                .filter(|reference| new.is_none_or(|new| !new.contains(reference)))
                .map(ToString::to_string)
                .collect();
            if !removed.is_empty() {
                self.descriptions.push(format!(
                    "downstream: `{key}` lost {} (selects nothing)",
                    removed.join(", ")
                ));
            }
        }
    }

    fn compare_commands(&mut self, before: &SuiteCatalog, after: &SuiteCatalog) {
        for command in &after.commands {
            let old = before.commands.iter().find(|c| c.name == command.name);
            if old != Some(command) {
                let change = if old.is_none() { "added" } else { "changed" };
                self.descriptions
                    .push(format!("commands: `{}` {change}", command.name));
                self.commands.insert(command.name.clone());
            }
        }
        for command in &before.commands {
            if !after.commands.iter().any(|c| c.name == command.name) {
                self.descriptions.push(format!(
                    "commands: `{}` removed (selects nothing)",
                    command.name
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GateError;
    use std::path::Path;

    const BASE: &str = r#"SuiteCatalog(
        // a comment
        global: ["Cargo.toml"],
        docs: ["docs/**"],
        always: ["checks/@fast"],
        targets: {
            "app/gpu_tests": (class: Gpu, triggers: ["crates/app/src/render/**"]),
            "app/heavy_tests": (class: Slow, test_threads: Some(2)),
        },
        rules: [
            (name: "shaders", paths: ["assets/shaders/**"], targets: ["app/@gpu"]),
            (name: "catalog", paths: ["assets/catalog/**"], crates: ["core"], targets: ["app/a"]),
        ],
        downstream: {"core": ["app/headless_tests"], "lib": ["app/x"]},
        commands: [(name: "golden", paths: ["crates/core/**"], program: "cargo", args: ["test"])],
    )"#;

    fn parse(text: &str) -> Result<SuiteCatalog, GateError> {
        SuiteCatalog::parse(text, Path::new("suites.ron"))
    }

    fn diff(after: &str) -> Result<CatalogDiff, GateError> {
        Ok(CatalogDiff::between(&parse(BASE)?, &parse(after)?))
    }

    #[test]
    fn comments_order_and_formatting_are_invisible() -> Result<(), GateError> {
        let reordered = BASE
            .replace("// a comment\n", "")
            .replace(
                "\"app/gpu_tests\": (class: Gpu, triggers: [\"crates/app/src/render/**\"]),\n            \"app/heavy_tests\": (class: Slow, test_threads: Some(2)),",
                "\"app/heavy_tests\": (class: Slow,  test_threads: Some(2)),\n\"app/gpu_tests\": (class: Gpu, triggers: [\"crates/app/src/render/**\"]),",
            );
        assert_ne!(reordered, BASE);
        assert!(diff(&reordered)?.is_empty());
        Ok(())
    }

    #[test]
    fn a_scope_change_is_reported_per_list() -> Result<(), GateError> {
        let changed = BASE
            .replace("\"docs/**\"", "\"docs/**\", \"x/**\"")
            .replace("[\"checks/@fast\"]", "[]");
        assert_eq!(diff(&changed)?.scope, vec!["docs", "always"]);
        let global = BASE.replace("[\"Cargo.toml\"]", "[\"Cargo.toml\", \"y\"]");
        assert_eq!(diff(&global)?.scope, vec!["global"]);
        Ok(())
    }

    #[test]
    fn target_entries_added_removed_or_changed_are_named() -> Result<(), GateError> {
        let changed = BASE.replace("Some(2)", "Some(3)").replace(
            "\"app/gpu_tests\": (class: Gpu, triggers: [\"crates/app/src/render/**\"]),",
            "\"app/new_tests\": (class: Slow),",
        );
        let diff = diff(&changed)?;
        assert_eq!(
            diff.targets,
            BTreeSet::from([
                TargetId::test("app", "gpu_tests"),
                TargetId::test("app", "heavy_tests"),
                TargetId::test("app", "new_tests"),
            ])
        );
        assert!(diff
            .descriptions
            .iter()
            .any(|d| d.contains("app/heavy_tests")
                && d.contains("test_threads 2")
                && d.contains("test_threads 3")));
        assert!(diff.scope.is_empty() && diff.rules.is_empty());
        Ok(())
    }

    #[test]
    fn a_changed_rule_carries_both_versions_and_a_rename_is_two_changes() -> Result<(), GateError> {
        let edited = BASE.replace("targets: [\"app/a\"]", "targets: [\"app/b\"]");
        let diff_edit = diff(&edited)?;
        assert_eq!(diff_edit.rules.len(), 1);
        let rule = &diff_edit.rules[0];
        assert_eq!(rule.name, "catalog");
        assert_eq!(
            rule.targets,
            vec![TargetRef::parse("app/a")?, TargetRef::parse("app/b")?]
        );
        assert_eq!(rule.crates, BTreeSet::from(["core".to_string()]));
        let renamed = BASE.replace("name: \"shaders\"", "name: \"wgsl\"");
        let names: Vec<String> = diff(&renamed)?
            .rules
            .into_iter()
            .map(|rule| rule.name)
            .collect();
        assert_eq!(names, ["shaders", "wgsl"]);
        Ok(())
    }

    #[test]
    fn downstream_gains_select_and_losses_only_describe() -> Result<(), GateError> {
        let gained = BASE.replace(
            "{\"core\": [\"app/headless_tests\"], \"lib\": [\"app/x\"]}",
            "{\"core\": [\"app/headless_tests\", \"app/more\"], \"new\": [\"app/y\"]}",
        );
        let diff = diff(&gained)?;
        assert_eq!(
            diff.downstream_added,
            BTreeMap::from([
                ("core".to_string(), vec![TargetRef::parse("app/more")?]),
                ("new".to_string(), vec![TargetRef::parse("app/y")?]),
            ])
        );
        assert!(diff
            .descriptions
            .iter()
            .any(|d| d.contains("`lib` lost app/x")));
        Ok(())
    }

    #[test]
    fn commands_added_or_changed_are_named_and_removed_ones_select_nothing() -> Result<(), GateError>
    {
        let changed = BASE.replace("args: [\"test\"]", "args: [\"test\", \"--release\"]");
        assert_eq!(
            diff(&changed)?.commands,
            BTreeSet::from(["golden".to_string()])
        );
        let removed = BASE.replace(
            "commands: [(name: \"golden\", paths: [\"crates/core/**\"], program: \"cargo\", args: [\"test\"])],",
            "",
        );
        let diff = diff(&removed)?;
        assert!(diff.commands.is_empty());
        assert!(!diff.is_empty());
        Ok(())
    }
}
