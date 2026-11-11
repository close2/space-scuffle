//! What changed in `Cargo.lock`: external packages, or only workspace members' dependency lists.
//!
//! # Responsibility
//! Parses two versions of the lockfile into entries keyed by (name, version, source) and
//! classifies their difference (plan 2026-11-11-faster-gates §4.4): any external package
//! (an entry with a `source`) added, removed, or changed in checksum or dependencies; workspace
//! members added or removed; or only members' `dependencies` arrays changed, naming those
//! members.
//!
//! # How it fits
//! `select.rs` calls [`LockDiff::between`] for a changed `Cargo.lock` whose two versions are
//! known. An external or membership change selects the complete suite; a member-only change
//! adds nothing beyond the members' own manifest changes, which must then be dev-only
//! (`manifest_diff.rs`), or the complete suite is selected. The module is pure.
//!
//! # Invariants
//! - Entries are compared by value: their order and the file's formatting are invisible.
//! - A lockfile that does not parse, or whose top level (format version, metadata) changed, is
//!   reported as [`LockDiff::Unreadable`] or [`LockDiff::Other`], never as member-only.

use crate::manifest_diff::{parse_toml, TomlValue};
use std::collections::{BTreeMap, BTreeSet};

/// One `[[package]]` entry of the lockfile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockEntry {
    /// The package name.
    pub name: String,
    /// Its exact version.
    pub version: String,
    /// Where it comes from (`registry+...`, `git+...`); `None` for a workspace member or a
    /// path dependency.
    pub source: Option<String>,
    /// The registry checksum, if any.
    pub checksum: Option<String>,
    /// Its dependencies as written: `"name"`, `"name version"` or `"name version (source)"`.
    pub dependencies: Vec<String>,
}

impl LockEntry {
    /// Whether the entry is an external package (it has a `source`).
    #[must_use]
    pub fn is_external(&self) -> bool {
        self.source.is_some()
    }
}

/// The key that identifies an entry.
type EntryKey = (String, String, Option<String>);

/// A parsed lockfile: its entries, and everything else at the top level.
#[derive(Clone, Debug, PartialEq)]
struct Lockfile {
    entries: BTreeMap<EntryKey, LockEntry>,
    rest: BTreeMap<String, TomlValue>,
}

/// A string field of an entry table.
fn text(table: &BTreeMap<String, TomlValue>, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(TomlValue::as_str)
        .map(str::to_string)
}

/// Parses lockfile text; `None` when it is not a lockfile.
fn parse(text_of_file: &str) -> Option<Lockfile> {
    let mut root = parse_toml(text_of_file).ok()?;
    let packages = match root.remove("package") {
        Some(TomlValue::Array(items)) => items,
        None => Vec::new(),
        Some(_) => return None,
    };
    let mut entries = BTreeMap::new();
    for item in &packages {
        let table = item.as_table()?;
        let mut dependencies = Vec::new();
        for dependency in table
            .get("dependencies")
            .and_then(TomlValue::as_array)
            .unwrap_or_default()
        {
            dependencies.push(dependency.as_str()?.to_string());
        }
        dependencies.sort();
        let entry = LockEntry {
            name: text(table, "name")?,
            version: text(table, "version")?,
            source: text(table, "source"),
            checksum: text(table, "checksum"),
            dependencies,
        };
        let key = (
            entry.name.clone(),
            entry.version.clone(),
            entry.source.clone(),
        );
        entries.insert(key, entry);
    }
    Some(Lockfile {
        entries,
        rest: root,
    })
}

/// How the lockfile changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockDiff {
    /// One version does not parse as a lockfile.
    Unreadable,
    /// An external package was added, removed or changed; the first one, as `name version`.
    External(String),
    /// A workspace member was added or removed (by name).
    Membership(String),
    /// The top level outside the package list changed (format version, metadata).
    Other,
    /// Only these members' `dependencies` arrays changed (possibly none at all).
    MembersOnly(BTreeSet<String>),
}

impl LockDiff {
    /// The difference from `before` to `after`.
    #[must_use]
    pub fn between(before: &str, after: &str) -> Self {
        let (Some(old), Some(new)) = (parse(before), parse(after)) else {
            return Self::Unreadable;
        };
        let keys: BTreeSet<&EntryKey> = old.entries.keys().chain(new.entries.keys()).collect();
        let mut members = BTreeSet::new();
        for key in keys {
            let (was, is) = (old.entries.get(key), new.entries.get(key));
            if was == is {
                continue;
            }
            let (name, version, source) = key;
            if source.is_some() {
                return Self::External(format!("{name} {version}"));
            }
            match (was, is) {
                (Some(was), Some(is)) if was.checksum == is.checksum => {
                    members.insert(name.clone());
                }
                (Some(_), Some(_)) => return Self::External(format!("{name} {version}")),
                _ => return Self::Membership(name.clone()),
            }
        }
        if old.rest != new.rest {
            return Self::Other;
        }
        Self::MembersOnly(members)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"# This file is automatically @generated by Cargo.
version = 4

[[package]]
name = "core"
version = "0.1.0"
dependencies = [
 "serde",
]

[[package]]
name = "app"
version = "0.1.0"
dependencies = [
 "core",
 "serde",
 "toml_edit 0.22.27",
]

[[package]]
name = "serde"
version = "1.0.200"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaa"

[[package]]
name = "proptest"
version = "1.5.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "bbbb"
"#;

    #[test]
    fn entries_parse_with_both_dependency_spellings() {
        let lock = parse(BASE);
        let entries = lock.as_ref().map(|lock| &lock.entries);
        let app = entries.and_then(|e| e.get(&("app".to_string(), "0.1.0".to_string(), None)));
        assert_eq!(
            app.map(|app| app.dependencies.clone()),
            Some(vec![
                "core".to_string(),
                "serde".to_string(),
                "toml_edit 0.22.27".to_string()
            ])
        );
        assert_eq!(app.map(LockEntry::is_external), Some(false));
        let serde = entries
            .and_then(|e| e.values().find(|entry| entry.name == "serde"))
            .map(LockEntry::is_external);
        assert_eq!(serde, Some(true));
    }

    #[test]
    fn a_member_only_change_names_the_members() {
        let after = BASE.replace(
            "dependencies = [\n \"serde\",\n]",
            "dependencies = [\n \"proptest\",\n \"serde\",\n]",
        );
        assert_eq!(
            LockDiff::between(BASE, &after),
            LockDiff::MembersOnly(BTreeSet::from(["core".to_string()]))
        );
        assert_eq!(
            LockDiff::between(BASE, BASE),
            LockDiff::MembersOnly(BTreeSet::new())
        );
    }

    #[test]
    fn checksum_version_and_added_external_entries_are_external() {
        let checksum = BASE.replace("checksum = \"aaaa\"", "checksum = \"cccc\"");
        assert_eq!(
            LockDiff::between(BASE, &checksum),
            LockDiff::External("serde 1.0.200".to_string())
        );
        let bumped = BASE.replace("version = \"1.0.200\"", "version = \"1.0.201\"");
        assert!(matches!(
            LockDiff::between(BASE, &bumped),
            LockDiff::External(_)
        ));
        let added = format!(
            "{BASE}\n[[package]]\nname = \"rand\"\nversion = \"0.9.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"dddd\"\n"
        );
        assert_eq!(
            LockDiff::between(BASE, &added),
            LockDiff::External("rand 0.9.0".to_string())
        );
    }

    #[test]
    fn membership_format_and_garbage_are_reported_as_such() {
        let member = format!("{BASE}\n[[package]]\nname = \"tool\"\nversion = \"0.1.0\"\n");
        assert_eq!(
            LockDiff::between(BASE, &member),
            LockDiff::Membership("tool".to_string())
        );
        let format = BASE.replace("version = 4", "version = 3");
        assert_eq!(LockDiff::between(BASE, &format), LockDiff::Other);
        assert_eq!(LockDiff::between(BASE, "[[package"), LockDiff::Unreadable);
    }
}
