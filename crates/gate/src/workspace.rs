//! The workspace's crates, their test targets and their dependency graph, from `cargo metadata`.
//!
//! # Responsibility
//! Turns `cargo metadata --no-deps` JSON into [`Workspace`]: every member crate with its
//! directory, its workspace-internal dependencies and its test targets (the ones `cargo test`
//! runs: libraries, binaries and integration tests whose `test` flag is set). It also answers
//! which crate owns a path and which crates a change in one crate can reach.
//!
//! # How it fits
//! The selection resolves catalog references against these targets, the runner builds them by
//! name, and the catalog test compares the catalog with them. Reading Cargo's own description
//! keeps the gate free of any dependency on the crates it tests.
//!
//! # Invariants
//! - Paths are relative to the repository root and use `/` on every host.
//! - Examples and benchmarks are not test targets: `cargo test` builds examples but runs neither,
//!   and both have `test = false` in this workspace.
//! - [`Workspace::reverse_dependencies`] follows normal (and build) edges transitively, then one
//!   final dev-dependency edge: a crate's tests see its dev-dependencies, but its dependents do
//!   not.

use crate::GateError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

/// What kind of test executable a target produces.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetKind {
    /// The library's unit tests (`--lib`).
    Lib,
    /// A binary's unit tests (`--bin <name>`).
    Bin(String),
    /// An integration test file (`--test <name>`).
    Test(String),
}

/// One test target: a crate and a target in it.
///
/// Displayed `crate/lib`, `crate/bin/<name>` or `crate/<test name>`, the spelling of the catalog.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetId {
    /// The crate's package name.
    pub package: String,
    /// Which target of the crate.
    pub kind: TargetKind,
}

impl TargetId {
    /// The library unit tests of `package`.
    #[must_use]
    pub fn lib(package: &str) -> Self {
        Self {
            package: package.to_string(),
            kind: TargetKind::Lib,
        }
    }

    /// The integration test `name` of `package`.
    #[must_use]
    pub fn test(package: &str, name: &str) -> Self {
        Self {
            package: package.to_string(),
            kind: TargetKind::Test(name.to_string()),
        }
    }

    /// The unit tests of binary `name` of `package`.
    #[must_use]
    pub fn bin(package: &str, name: &str) -> Self {
        Self {
            package: package.to_string(),
            kind: TargetKind::Bin(name.to_string()),
        }
    }

    /// The Cargo filter that builds only this target: `--lib`, `--bin <n>` or `--test <n>`.
    #[must_use]
    pub fn cargo_filter(&self) -> Vec<String> {
        match &self.kind {
            TargetKind::Lib => vec!["--lib".to_string()],
            TargetKind::Bin(name) => vec!["--bin".to_string(), name.clone()],
            TargetKind::Test(name) => vec!["--test".to_string(), name.clone()],
        }
    }
}

impl fmt::Display for TargetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TargetKind::Lib => write!(f, "{}/lib", self.package),
            TargetKind::Bin(name) => write!(f, "{}/bin/{name}", self.package),
            TargetKind::Test(name) => write!(f, "{}/{name}", self.package),
        }
    }
}

/// One test target and its root source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The target's identity.
    pub id: TargetId,
    /// The crate root of the target (`src/lib.rs`, `tests/x.rs`), relative to the repository.
    pub src_path: String,
}

/// How one workspace crate depends on another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyKind {
    /// A normal or build dependency: the dependent's library uses it.
    Normal,
    /// A dev-dependency: only the dependent's tests, examples and benches use it.
    Dev,
}

/// One workspace crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    /// The package name.
    pub name: String,
    /// The package version, given to test executables as `CARGO_PKG_VERSION`.
    pub version: String,
    /// The crate's directory (where `Cargo.toml` is), relative to the repository root.
    pub dir: String,
    /// Workspace-internal dependencies, by name.
    pub dependencies: BTreeMap<String, DependencyKind>,
    /// Test targets, in metadata order.
    pub targets: Vec<Target>,
}

/// Every workspace crate, by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Workspace {
    /// The crates, keyed by package name.
    pub packages: BTreeMap<String, Package>,
    /// Cargo's target directory (`target_directory` of `cargo metadata`), where the compiler
    /// crash retry finds the incremental caches it clears; empty when unknown.
    pub target_directory: PathBuf,
}

/// `path` relative to `root`, with `/` separators.
fn relative(root: &Path, path: &str) -> String {
    let path = Path::new(path);
    let inner = path.strip_prefix(root).unwrap_or(path);
    inner
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The test target `target` (one entry of a package's `targets`) describes, if it is one.
fn parse_target(root: &Path, package: &str, target: &Value) -> Result<Option<Target>, GateError> {
    let name = target["name"]
        .as_str()
        .ok_or(GateError::MetadataShape("targets[].name"))?;
    let kinds = target["kind"]
        .as_array()
        .ok_or(GateError::MetadataShape("targets[].kind"))?;
    let src = target["src_path"]
        .as_str()
        .ok_or(GateError::MetadataShape("targets[].src_path"))?;
    if !target["test"].as_bool().unwrap_or(true) {
        return Ok(None);
    }
    let has = |wanted: &[&str]| {
        kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| wanted.contains(&kind))
    };
    let kind = if has(&["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"]) {
        TargetKind::Lib
    } else if has(&["bin"]) {
        TargetKind::Bin(name.to_string())
    } else if has(&["test"]) {
        TargetKind::Test(name.to_string())
    } else {
        return Ok(None);
    };
    Ok(Some(Target {
        id: TargetId {
            package: package.to_string(),
            kind,
        },
        src_path: relative(root, src),
    }))
}

impl Workspace {
    /// Builds the workspace from `cargo metadata --no-deps --format-version 1` output for the
    /// repository at `root`.
    ///
    /// # Errors
    /// [`GateError::MetadataShape`] when a field the gate relies on is missing.
    pub fn from_metadata(root: &Path, metadata: &Value) -> Result<Self, GateError> {
        let packages = metadata["packages"]
            .as_array()
            .ok_or(GateError::MetadataShape("packages"))?;
        let names: BTreeSet<&str> = packages
            .iter()
            .filter_map(|package| package["name"].as_str())
            .collect();
        let mut workspace = Self {
            target_directory: metadata["target_directory"]
                .as_str()
                .map_or_else(|| root.join("target"), PathBuf::from),
            ..Self::default()
        };
        for package in packages {
            let name = package["name"]
                .as_str()
                .ok_or(GateError::MetadataShape("packages[].name"))?;
            let manifest = package["manifest_path"]
                .as_str()
                .ok_or(GateError::MetadataShape("packages[].manifest_path"))?;
            let manifest = relative(root, manifest);
            let dir = manifest
                .strip_suffix("/Cargo.toml")
                .unwrap_or("")
                .to_string();
            let mut dependencies = BTreeMap::new();
            for dependency in package["dependencies"]
                .as_array()
                .ok_or(GateError::MetadataShape("packages[].dependencies"))?
            {
                let dep = dependency["name"]
                    .as_str()
                    .ok_or(GateError::MetadataShape("dependencies[].name"))?;
                if !names.contains(dep) {
                    continue;
                }
                let kind = if dependency["kind"].as_str() == Some("dev") {
                    DependencyKind::Dev
                } else {
                    DependencyKind::Normal
                };
                let entry = dependencies.entry(dep.to_string()).or_insert(kind);
                *entry = (*entry).min(kind);
            }
            let mut targets = Vec::new();
            for target in package["targets"]
                .as_array()
                .ok_or(GateError::MetadataShape("packages[].targets"))?
            {
                targets.extend(parse_target(root, name, target)?);
            }
            let version = package["version"].as_str().unwrap_or("0.0.0").to_string();
            workspace.packages.insert(
                name.to_string(),
                Package {
                    name: name.to_string(),
                    version,
                    dir,
                    dependencies,
                    targets,
                },
            );
        }
        Ok(workspace)
    }

    /// Runs `cargo metadata --no-deps` in `root` and builds the workspace from it.
    ///
    /// # Errors
    /// As [`crate::changes::capture`] and [`Workspace::from_metadata`].
    pub fn load(root: &Path) -> Result<Self, GateError> {
        let cargo = crate::changes::cargo_program();
        let text = crate::changes::capture(
            &cargo,
            &["metadata", "--no-deps", "--format-version", "1", "--locked"],
            root,
        )?;
        let json: Value = serde_json::from_str(&text)?;
        Self::from_metadata(root, &json)
    }

    /// The crate called `name`.
    #[must_use]
    pub fn package(&self, name: &str) -> Option<&Package> {
        self.packages.get(name)
    }

    /// Every test target of every crate, in crate and metadata order.
    pub fn test_targets(&self) -> impl Iterator<Item = &Target> {
        self.packages
            .values()
            .flat_map(|package| package.targets.iter())
    }

    /// The crate whose directory holds `path` (relative to the repository root), if any.
    #[must_use]
    pub fn package_of_path(&self, path: &str) -> Option<&Package> {
        self.packages
            .values()
            .filter(|package| {
                !package.dir.is_empty()
                    && path.len() > package.dir.len()
                    && path.starts_with(&package.dir)
                    && path.as_bytes()[package.dir.len()] == b'/'
            })
            .max_by_key(|package| package.dir.len())
    }

    /// Every crate a change in `name` can affect: crates depending on it through normal edges,
    /// transitively, and crates whose tests (dev-dependencies) use one of those or `name`.
    /// `name` itself is not included.
    #[must_use]
    pub fn reverse_dependencies(&self, name: &str) -> BTreeSet<String> {
        let mut reached: BTreeSet<String> = BTreeSet::from([name.to_string()]);
        let mut frontier = vec![name.to_string()];
        while let Some(current) = frontier.pop() {
            for package in self.packages.values() {
                if package.dependencies.get(&current) == Some(&DependencyKind::Normal)
                    && reached.insert(package.name.clone())
                {
                    frontier.push(package.name.clone());
                }
            }
        }
        let mut out = reached.clone();
        for package in self.packages.values() {
            if package
                .dependencies
                .iter()
                .any(|(dep, kind)| *kind == DependencyKind::Dev && reached.contains(dep))
            {
                out.insert(package.name.clone());
            }
        }
        out.remove(name);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A synthetic metadata document: `(name, [(dependency, kind)])` per crate.
    fn metadata(crates: &[(&str, &[(&str, &str)])]) -> Value {
        let packages: Vec<Value> = crates
            .iter()
            .map(|(name, deps)| {
                json!({
                    "name": name,
                    "version": "0.1.0",
                    "manifest_path": format!("/r/crates/{name}/Cargo.toml"),
                    "dependencies": deps.iter().map(|(dep, kind)| json!({
                        "name": dep,
                        "kind": if *kind == "dev" { json!("dev") } else { Value::Null },
                    })).collect::<Vec<_>>(),
                    "targets": [
                        {"name": name, "kind": ["lib"], "src_path": format!("/r/crates/{name}/src/lib.rs"), "test": true},
                        {"name": "it_tests", "kind": ["test"], "src_path": format!("/r/crates/{name}/tests/it_tests.rs"), "test": true},
                        {"name": "bench", "kind": ["bench"], "src_path": format!("/r/crates/{name}/benches/b.rs"), "test": false},
                    ],
                })
            })
            .collect();
        json!({ "packages": packages })
    }

    fn graph() -> Result<Workspace, GateError> {
        Workspace::from_metadata(
            Path::new("/r"),
            &metadata(&[
                ("types", &[("serde", "")]),
                ("map", &[("types", "")]),
                ("core", &[("types", ""), ("map", ""), ("fixtures", "dev")]),
                ("fixtures", &[("types", ""), ("core", "")]),
                ("baker", &[("core", ""), ("fixtures", "dev")]),
                ("app", &[("baker", "")]),
                ("checks", &[("types", "dev")]),
                ("lonely", &[]),
            ]),
        )
    }

    #[test]
    fn targets_and_paths_are_read_relative_to_the_root() -> Result<(), GateError> {
        let workspace = graph()?;
        let core = workspace
            .package("core")
            .ok_or(GateError::MetadataShape("core"))?;
        assert_eq!(core.dir, "crates/core");
        assert_eq!(core.targets.len(), 2, "the bench has test = false");
        assert_eq!(core.targets[0].src_path, "crates/core/src/lib.rs");
        assert_eq!(core.targets[1].id.to_string(), "core/it_tests");
        assert!(!core.dependencies.contains_key("serde"));
        Ok(())
    }

    #[test]
    fn the_target_directory_comes_from_metadata_else_the_default() -> Result<(), GateError> {
        assert_eq!(graph()?.target_directory, Path::new("/r/target"));
        let mut with = metadata(&[("lonely", &[])]);
        with["target_directory"] = json!("/scratch/cargo-target/wt");
        let workspace = Workspace::from_metadata(Path::new("/r"), &with)?;
        assert_eq!(
            workspace.target_directory,
            Path::new("/scratch/cargo-target/wt")
        );
        Ok(())
    }

    #[test]
    fn reverse_dependencies_follow_normal_edges_then_one_dev_edge() -> Result<(), GateError> {
        let workspace = graph()?;
        let set = |items: &[&str]| {
            items
                .iter()
                .map(ToString::to_string)
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            workspace.reverse_dependencies("types"),
            set(&["app", "baker", "checks", "core", "fixtures", "map"])
        );
        assert_eq!(workspace.reverse_dependencies("baker"), set(&["app"]));
        // `fixtures` reaches `core` and `baker` only through their dev-dependencies, and does not
        // continue from there: `app` uses `baker`'s library, which never sees `fixtures`.
        assert_eq!(
            workspace.reverse_dependencies("fixtures"),
            set(&["baker", "core"])
        );
        assert!(workspace.reverse_dependencies("lonely").is_empty());
        assert!(workspace.reverse_dependencies("app").is_empty());
        Ok(())
    }

    #[test]
    fn paths_belong_to_the_crate_whose_directory_holds_them() -> Result<(), GateError> {
        let workspace = graph()?;
        let owner = |path: &str| workspace.package_of_path(path).map(|p| p.name.clone());
        assert_eq!(owner("crates/core/src/a.rs"), Some("core".to_string()));
        assert_eq!(owner("crates/core"), None);
        assert_eq!(owner("crates/corex/a.rs"), None);
        assert_eq!(owner("docs/a.md"), None);
        Ok(())
    }

    #[test]
    fn missing_fields_are_shape_errors() {
        let bad = json!({"packages": [{"name": "x"}]});
        assert!(matches!(
            Workspace::from_metadata(Path::new("/r"), &bad),
            Err(GateError::MetadataShape(_))
        ));
    }
}
