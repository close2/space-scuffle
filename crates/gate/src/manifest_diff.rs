//! Whether a member manifest (`crates/<c>/Cargo.toml`) changed only in its dev sections.
//!
//! # Responsibility
//! Parses two versions of a member manifest into a normalized value tree ([`TomlValue`]) and
//! classifies the change (plan 2026-11-11-faster-gates §4.4): **dev-only** when the two trees
//! are equal once the dev sections are removed, else **runtime**, naming the first section that
//! differs. The dev sections are `[dev-dependencies]`, every `[target.<cfg>.dev-dependencies]`,
//! and the `[[test]]`, `[[bench]]` and `[[example]]` arrays. For a dev-only change it also names
//! the `[[test]]` targets whose entry differs.
//!
//! # How it fits
//! `select.rs` calls [`classify`] for a changed member manifest whose two versions are known; a
//! dev-only change selects the crate's own `fast` and `slow` targets (and changed `[[test]]`
//! targets) but no downstream contract, a runtime change the complete suite. `lockfile.rs` reuses
//! [`parse_toml`]. The module is pure: it reads no file.
//!
//! # Invariants
//! - Whitespace, comments, key order and the spelling of a table (`[a] b = 1`, `a = { b = 1 }`,
//!   `a.b = 1`) are invisible: only values are compared.
//! - Text that does not parse is never dev-only.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A TOML value with its formatting removed, comparable by value.
#[derive(Clone, Debug, PartialEq)]
pub enum TomlValue {
    /// A string.
    String(String),
    /// An integer.
    Integer(i64),
    /// A float.
    Float(f64),
    /// A boolean.
    Boolean(bool),
    /// A date-time, as written in its canonical form.
    Datetime(String),
    /// An array, including an array of tables.
    Array(Vec<TomlValue>),
    /// A table or inline table, by key.
    Table(BTreeMap<String, TomlValue>),
}

impl TomlValue {
    /// The table this value is, if it is one.
    #[must_use]
    pub fn as_table(&self) -> Option<&BTreeMap<String, TomlValue>> {
        match self {
            Self::Table(table) => Some(table),
            _ => None,
        }
    }

    /// The string this value is, if it is one.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// The array this value is, if it is one.
    #[must_use]
    pub fn as_array(&self) -> Option<&[TomlValue]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }
}

/// Why a TOML text could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TomlError(pub String);

impl fmt::Display for TomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Normalizes one `toml_edit` value.
fn from_value(value: &toml_edit::Value) -> TomlValue {
    match value {
        toml_edit::Value::String(text) => TomlValue::String(text.value().clone()),
        toml_edit::Value::Integer(number) => TomlValue::Integer(*number.value()),
        toml_edit::Value::Float(number) => TomlValue::Float(*number.value()),
        toml_edit::Value::Boolean(flag) => TomlValue::Boolean(*flag.value()),
        toml_edit::Value::Datetime(time) => TomlValue::Datetime(time.value().to_string()),
        toml_edit::Value::Array(items) => TomlValue::Array(items.iter().map(from_value).collect()),
        toml_edit::Value::InlineTable(table) => TomlValue::Table(
            table
                .iter()
                .map(|(key, value)| (key.to_string(), from_value(value)))
                .collect(),
        ),
    }
}

/// Normalizes one `toml_edit` table.
fn from_table(table: &toml_edit::Table) -> BTreeMap<String, TomlValue> {
    table
        .iter()
        .filter_map(|(key, item)| from_item(item).map(|value| (key.to_string(), value)))
        .collect()
}

/// Normalizes one `toml_edit` item; `None` for an empty slot.
fn from_item(item: &toml_edit::Item) -> Option<TomlValue> {
    match item {
        toml_edit::Item::None => None,
        toml_edit::Item::Value(value) => Some(from_value(value)),
        toml_edit::Item::Table(table) => Some(TomlValue::Table(from_table(table))),
        toml_edit::Item::ArrayOfTables(tables) => Some(TomlValue::Array(
            tables
                .iter()
                .map(|table| TomlValue::Table(from_table(table)))
                .collect(),
        )),
    }
}

/// Parses TOML text into its root table.
///
/// # Errors
/// [`TomlError`] with the parser's message when the text is not valid TOML.
pub fn parse_toml(text: &str) -> Result<BTreeMap<String, TomlValue>, TomlError> {
    let document: toml_edit::DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| TomlError(error.to_string()))?;
    Ok(from_table(document.as_table()))
}

/// The top-level arrays of a manifest that only tests, benches and examples use.
const DEV_ARRAYS: [&str; 3] = ["test", "bench", "example"];

/// The spellings of the dev-dependency table Cargo accepts.
const DEV_TABLES: [&str; 2] = ["dev-dependencies", "dev_dependencies"];

/// The manifest without its dev sections.
fn without_dev(manifest: &BTreeMap<String, TomlValue>) -> BTreeMap<String, TomlValue> {
    let mut out = manifest.clone();
    for key in DEV_ARRAYS.iter().chain(DEV_TABLES.iter()) {
        out.remove(*key);
    }
    if let Some(TomlValue::Table(targets)) = out.get_mut("target") {
        for platform in targets.values_mut() {
            if let TomlValue::Table(sections) = platform {
                for key in DEV_TABLES {
                    sections.remove(key);
                }
            }
        }
        targets.retain(|_, platform| platform.as_table().is_none_or(|t| !t.is_empty()));
        if targets.is_empty() {
            out.remove("target");
        }
    }
    out
}

/// The `[[test]]` entries of a manifest, by name (an entry without a name is keyed `""`).
fn test_entries(manifest: &BTreeMap<String, TomlValue>) -> BTreeMap<String, TomlValue> {
    manifest
        .get("test")
        .and_then(TomlValue::as_array)
        .unwrap_or_default()
        .iter()
        .map(|entry| {
            let name = entry
                .as_table()
                .and_then(|table| table.get("name"))
                .and_then(TomlValue::as_str)
                .unwrap_or_default();
            (name.to_string(), entry.clone())
        })
        .collect()
}

/// How a member manifest changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestChange {
    /// Only dev sections differ.
    DevOnly {
        /// Names of the `[[test]]` targets whose entry was added, removed or changed.
        tests: BTreeSet<String>,
    },
    /// Something a build of the crate itself or of its dependents reads differs.
    Runtime {
        /// The first differing section, written as in the manifest (`[dependencies]`).
        section: String,
    },
}

/// Classifies the change from `before` to `after`.
#[must_use]
pub fn classify_manifest(before: &str, after: &str) -> ManifestChange {
    let (Ok(old), Ok(new)) = (parse_toml(before), parse_toml(after)) else {
        return ManifestChange::Runtime {
            section: "the manifest (unparsable)".to_string(),
        };
    };
    let (old_runtime, new_runtime) = (without_dev(&old), without_dev(&new));
    if old_runtime != new_runtime {
        let keys: BTreeSet<&String> = old_runtime.keys().chain(new_runtime.keys()).collect();
        let section = keys
            .into_iter()
            .find(|key| old_runtime.get(*key) != new_runtime.get(*key))
            .map_or_else(|| "the manifest".to_string(), |key| format!("[{key}]"));
        return ManifestChange::Runtime { section };
    }
    let (old_tests, new_tests) = (test_entries(&old), test_entries(&new));
    let names: BTreeSet<&String> = old_tests.keys().chain(new_tests.keys()).collect();
    let tests = names
        .into_iter()
        .filter(|name| old_tests.get(*name) != new_tests.get(*name))
        .cloned()
        .collect();
    ManifestChange::DevOnly { tests }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
[package]
name = "core"
version.workspace = true

# A comment.
[dependencies]
serde.workspace = true

[features]
default = []

[lints]
workspace = true

[dev-dependencies]
tempfile = "3.17"

[target.'cfg(unix)'.dependencies]
libc = "0.2"

[[test]]
name = "golden_tests"
path = "tests/golden_tests.rs"
"#;

    fn dev_only(tests: &[&str]) -> ManifestChange {
        ManifestChange::DevOnly {
            tests: tests.iter().map(ToString::to_string).collect(),
        }
    }

    fn runtime(section: &str) -> ManifestChange {
        ManifestChange::Runtime {
            section: section.to_string(),
        }
    }

    #[test]
    fn dev_dependency_and_test_changes_are_dev_only() {
        let added = BASE.replace(
            "tempfile = \"3.17\"",
            "tempfile = \"3.17\"\nproptest.workspace = true",
        );
        assert_eq!(classify_manifest(BASE, &added), dev_only(&[]));
        let target_dev = format!("{BASE}\n[target.'cfg(unix)'.dev-dependencies]\nnix = \"0.29\"\n");
        assert_eq!(classify_manifest(BASE, &target_dev), dev_only(&[]));
        let test = BASE.replace(
            "path = \"tests/golden_tests.rs\"",
            "path = \"tests/golden_tests.rs\"\nharness = false",
        );
        assert_eq!(classify_manifest(BASE, &test), dev_only(&["golden_tests"]));
        let bench = format!("{BASE}\n[[bench]]\nname = \"speed\"\nharness = false\n");
        assert_eq!(classify_manifest(BASE, &bench), dev_only(&[]));
    }

    #[test]
    fn runtime_sections_are_named() {
        let dependency = BASE.replace(
            "serde.workspace = true",
            "serde.workspace = true\nron = \"0.8\"",
        );
        assert_eq!(
            classify_manifest(BASE, &dependency),
            runtime("[dependencies]")
        );
        let feature = BASE.replace("default = []", "default = [\"x\"]");
        assert_eq!(classify_manifest(BASE, &feature), runtime("[features]"));
        let lints = BASE.replace(
            "[lints]\nworkspace = true",
            "[lints.rust]\nunsafe_code = \"forbid\"",
        );
        assert_eq!(classify_manifest(BASE, &lints), runtime("[lints]"));
        let target_runtime = BASE.replace("libc = \"0.2\"", "libc = \"0.3\"");
        assert_eq!(
            classify_manifest(BASE, &target_runtime),
            runtime("[target]")
        );
        assert_eq!(
            classify_manifest(BASE, "[package"),
            runtime("the manifest (unparsable)")
        );
    }

    #[test]
    fn whitespace_comments_and_table_spelling_are_invisible() {
        let respelled = BASE
            .replace("# A comment.\n", "")
            .replace(
                "serde.workspace = true",
                "serde = { workspace = true }   # same",
            )
            .replace("version.workspace = true", "version = { workspace = true }");
        assert_eq!(classify_manifest(BASE, &respelled), dev_only(&[]));
    }
}
