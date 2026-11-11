//! The selection: which test targets and commands a change needs, per tier, and why.
//!
//! # Responsibility
//! Implements blueprint §3.3 and §4 as one pure function, [`select`]: from the catalog, the
//! workspace's targets, the targets' source files and the changed paths, it computes the
//! [`Selection`] for a tier, recording for every target the [`Reason`]s that chose it and for
//! every changed path the catalog entries that cover it.
//!
//! # How it fits
//! `explain` prints a selection; the runner builds and runs it. Because the function reads no
//! file and starts no process, the tier semantics are unit-tested on synthetic catalogs here and
//! on the real catalog by `tests/selection_tests.rs`.
//!
//! Some files are judged by their content, not their path (plan 2026-11-11-faster-gates §4):
//! the suite catalog `crates/gate/suites.ron` selects what its semantic diff
//! (`catalog_diff.rs`) changes; a member manifest changed only in its dev sections
//! (`manifest_diff.rs`) selects its crate without downstream contracts; `Cargo.lock` changed only
//! in members' dependency lists (`lockfile.rs`) adds nothing beyond those members' dev-only
//! manifest changes. Their two versions arrive in a [`Change`]; the runner fills them from Git,
//! so this module still reads no file.
//!
//! # Invariants
//! - Uncertain means more (P3): a changed path that no `global`, `docs`, crate, rule, trigger,
//!   command or own-file entry covers selects the complete suite, and so does any `global` path.
//! - A file judged by content whose two versions are not both known (the path-only form
//!   [`select`], an unreadable base) selects the complete suite.
//! - An external package change in `Cargo.lock`, and any change of a member manifest outside its
//!   dev sections, selects the complete suite.
//! - A crate change selects the crate's `fast` and `slow` targets and its downstream contracts;
//!   `gpu` and `valgrind` targets only when a changed path is one of their own files or matches
//!   one of their triggers, a path rule names them, or a downstream contract does.
//! - A path matching a `docs` glob never counts as a crate change. A change made only of such
//!   paths is docs-only: fmt and the `always` targets, no clippy.
//! - `full` selects every test target of the workspace and every catalog command.

use crate::catalog::{SuiteCatalog, TargetRef, TestClass, CATALOG_PATH};
use crate::catalog_diff::CatalogDiff;
use crate::lockfile::LockDiff;
use crate::manifest_diff::{classify_manifest, ManifestChange};
use crate::sources::SourceIndex;
use crate::tier::Tier;
use crate::workspace::{TargetId, Workspace};
use crate::GateError;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Why one target or command was selected.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// The catalog's `always` list (the charter checks).
    Always,
    /// Part of the complete suite.
    Complete,
    /// A file of this crate changed.
    Crate(String),
    /// A downstream contract of this changed crate.
    Downstream(String),
    /// The computed `@callers` contract of this changed crate: the target's code names it.
    Callers(String),
    /// The path rule of this name matched a changed path.
    Rule(String),
    /// This trigger glob (of a target or a command) matched a changed path.
    Trigger(String),
    /// One of the target's own source files changed.
    OwnFile(String),
    /// A difference of the suite catalog against its base, as `explain` describes it.
    Catalog(String),
    /// A dev-only change of the crate's manifest touched this target's `[[test]]` entry.
    Manifest(String),
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Always => f.write_str("always"),
            Self::Complete => f.write_str("complete suite"),
            Self::Crate(name) => write!(f, "crate {name} changed"),
            Self::Downstream(name) => write!(f, "downstream of {name}"),
            Self::Callers(name) => write!(f, "downstream of {name} (calls {name}::)"),
            Self::Rule(name) => write!(f, "rule `{name}`"),
            Self::Trigger(glob) => write!(f, "trigger `{glob}`"),
            Self::OwnFile(path) => write!(f, "own file {path}"),
            Self::Catalog(what) => write!(f, "suites.ron: {what}"),
            Self::Manifest(path) => write!(f, "[[test]] entry changed in {path}"),
        }
    }
}

/// Why the complete suite was selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompleteCause {
    /// The tier always runs everything (`full`).
    Tier,
    /// A workspace-wide input changed.
    Global {
        /// The changed path.
        path: String,
        /// The `global` glob it matched.
        glob: String,
    },
    /// A changed path no catalog entry covers.
    Unmatched {
        /// The changed path.
        path: String,
    },
    /// A file judged by its content changed in a way that reaches every crate, or its versions
    /// are not both known.
    Content {
        /// The changed path.
        path: String,
        /// Why its change selects everything, for example `catalog scope changed`.
        why: String,
    },
}

impl fmt::Display for CompleteCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tier => f.write_str("the tier runs the complete suite"),
            Self::Global { path, glob } => write!(f, "{path} matches global `{glob}`"),
            Self::Unmatched { path } => write!(f, "no catalog entry covers {path}"),
            Self::Content { path, why } => write!(f, "{path}: {why}"),
        }
    }
}

/// One selected target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selected {
    /// The target's class.
    pub class: TestClass,
    /// Every reason that selected it.
    pub reasons: BTreeSet<Reason>,
}

/// One changed path and the catalog entries that cover it, for `explain`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathMatch {
    /// The changed path.
    pub path: String,
    /// What covers it, as readable labels; empty when nothing does.
    pub covered_by: Vec<String>,
}

/// Which crates clippy checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClippyScope {
    /// No clippy run (docs-only change).
    Skip,
    /// `cargo clippy -p <each> --all-targets`.
    Packages(BTreeSet<String>),
    /// `cargo clippy --workspace --all-targets`.
    Workspace,
}

/// What one tier runs for one change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    /// The tier the selection is for.
    pub tier: Tier,
    /// Every changed path with what covers it.
    pub paths: Vec<PathMatch>,
    /// Set when the complete suite was selected, with the first cause.
    pub complete: Option<CompleteCause>,
    /// The change touches only `docs` paths.
    pub docs_only: bool,
    /// Crates with a changed (non-docs) file.
    pub changed_crates: BTreeSet<String>,
    /// The selected targets.
    pub targets: BTreeMap<TargetId, Selected>,
    /// The selected catalog commands, by name.
    pub commands: BTreeMap<String, BTreeSet<Reason>>,
    /// The clippy run.
    pub clippy: ClippyScope,
}

impl Selection {
    /// Crates with at least one selected target.
    #[must_use]
    pub fn crates(&self) -> BTreeSet<String> {
        self.targets.keys().map(|id| id.package.clone()).collect()
    }
}

/// Accumulates targets and commands with their reasons.
struct Builder<'a> {
    catalog: &'a SuiteCatalog,
    workspace: &'a Workspace,
    targets: BTreeMap<TargetId, Selected>,
    commands: BTreeMap<String, BTreeSet<Reason>>,
    /// Crates whose manifest changed only in its dev sections.
    dev_only: BTreeSet<String>,
}

impl Builder<'_> {
    fn add(&mut self, id: TargetId, reason: Reason) {
        let class = self.catalog.class_of(&id);
        self.targets
            .entry(id)
            .or_insert_with(|| Selected {
                class,
                reasons: BTreeSet::new(),
            })
            .reasons
            .insert(reason);
    }

    fn add_ref(&mut self, reference: &TargetRef, reason: &Reason) -> Result<(), GateError> {
        for id in self.catalog.resolve(reference, self.workspace)? {
            self.add(id, reason.clone());
        }
        Ok(())
    }

    /// Adds the `fast` and `slow` targets of `krate`.
    fn add_crate(&mut self, krate: &str, reason: &Reason) -> Result<(), GateError> {
        let package = self
            .workspace
            .package(krate)
            .ok_or_else(|| GateError::Catalog(format!("`{krate}` names no workspace crate")))?;
        for target in &package.targets {
            if self.catalog.class_of(&target.id).runs_on_crate_change() {
                self.add(target.id.clone(), reason.clone());
            }
        }
        Ok(())
    }
}

/// One file's text on both sides of a change; `None` where the file does not exist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileVersions {
    /// The text at the base revision.
    pub before: Option<String>,
    /// The text after the change (the working tree, or the second revision).
    pub after: Option<String>,
}

/// A change: the changed paths, and the two versions of every changed file the selection judges
/// by content ([`is_judged_by_content`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    /// Every changed path, relative to the repository root.
    pub paths: BTreeSet<String>,
    /// The versions of the changed files judged by content; a file missing here selects the
    /// complete suite.
    pub contents: BTreeMap<String, FileVersions>,
}

impl Change {
    /// A change known only by its paths: every file judged by content selects the complete
    /// suite.
    #[must_use]
    pub fn paths_only(paths: BTreeSet<String>) -> Self {
        Self {
            paths,
            contents: BTreeMap::new(),
        }
    }
}

/// Where the lockfile lives, relative to the repository root.
pub const LOCKFILE_PATH: &str = "Cargo.lock";

/// The workspace crate whose manifest `path` is, if it is one.
fn manifest_owner<'a>(path: &str, workspace: &'a Workspace) -> Option<&'a str> {
    workspace
        .package_of_path(path)
        .filter(|package| path == format!("{}/Cargo.toml", package.dir))
        .map(|package| package.name.as_str())
}

/// Whether the selection judges a change to `path` by its content rather than its path: the
/// suite catalog, the lockfile, and the manifest of a workspace crate.
#[must_use]
pub fn is_judged_by_content(path: &str, workspace: &Workspace) -> bool {
    path == CATALOG_PATH || path == LOCKFILE_PATH || manifest_owner(path, workspace).is_some()
}

/// Inputs of [`select`] that describe the repository.
#[derive(Clone, Copy, Debug)]
pub struct Inputs<'a> {
    /// The suite catalog.
    pub catalog: &'a SuiteCatalog,
    /// The workspace's crates and targets.
    pub workspace: &'a Workspace,
    /// The targets' source files.
    pub sources: &'a SourceIndex,
}

/// Adds what the rules, commands, triggers and own files covering `path` select, and records
/// each in `covered_by`.
fn cover_by_entries(
    builder: &mut Builder<'_>,
    sources: &SourceIndex,
    path: &str,
    covered_by: &mut Vec<String>,
) -> Result<(), GateError> {
    let catalog = builder.catalog;
    for rule in &catalog.rules {
        if rule.paths.iter().any(|g| g.matches(path)) {
            covered_by.push(format!("rule `{}`", rule.name));
            let reason = Reason::Rule(rule.name.clone());
            for reference in &rule.targets {
                builder.add_ref(reference, &reason)?;
            }
            for krate in &rule.crates {
                builder.add_crate(krate, &reason)?;
            }
        }
    }
    for command in &catalog.commands {
        if let Some(glob) = command.paths.iter().find(|g| g.matches(path)) {
            covered_by.push(format!("command `{}`", command.name));
            let reasons = builder.commands.entry(command.name.clone()).or_default();
            reasons.insert(Reason::Trigger(glob.to_string()));
        }
    }
    for (id, entry) in &catalog.targets {
        if let Some(glob) = entry.triggers.iter().find(|g| g.matches(path)) {
            covered_by.push(format!("trigger of {id}"));
            builder.add(id.clone(), Reason::Trigger(glob.to_string()));
        }
    }
    for id in sources.targets_of(path) {
        if !catalog.class_of(&id).runs_on_crate_change() {
            covered_by.push(format!("own file of {id}"));
            builder.add(id, Reason::OwnFile(path.to_string()));
        }
    }
    Ok(())
}

/// Judges one changed path: what covers it, and what it selects.
fn judge_path(
    builder: &mut Builder<'_>,
    inputs: Inputs<'_>,
    change: &Change,
    path: &str,
    changed_crates: &mut BTreeSet<String>,
) -> Result<(PathMatch, Option<CompleteCause>, bool), GateError> {
    let catalog = builder.catalog;
    let mut covered_by = Vec::new();
    let mut complete = None;
    if let Some(glob) = catalog.global.iter().find(|g| g.matches(path)) {
        covered_by.push(format!("global `{glob}`: complete suite"));
        complete = Some(CompleteCause::Global {
            path: path.to_string(),
            glob: glob.to_string(),
        });
    }
    if is_judged_by_content(path, builder.workspace) {
        let why = judge_content(builder, inputs, change, path, &mut covered_by);
        if let (None, Some(why)) = (&complete, why) {
            covered_by.push(format!("{why}: complete suite"));
            complete = Some(CompleteCause::Content {
                path: path.to_string(),
                why,
            });
        }
    }
    let docs = catalog.docs.iter().find(|g| g.matches(path));
    if let Some(glob) = docs {
        covered_by.push(format!("docs `{glob}`"));
    } else if let Some(package) = builder.workspace.package_of_path(path) {
        let dev_only = manifest_owner(path, builder.workspace)
            .is_some_and(|owner| builder.dev_only.contains(owner));
        if dev_only {
            covered_by.push(format!(
                "crate {} (dev-only manifest change: no downstream contract)",
                package.name
            ));
        } else {
            covered_by.push(format!("crate {}", package.name));
            changed_crates.insert(package.name.clone());
        }
    }
    cover_by_entries(builder, inputs.sources, path, &mut covered_by)?;
    if covered_by.is_empty() {
        covered_by.push("nothing: complete suite".to_string());
        complete = Some(CompleteCause::Unmatched {
            path: path.to_string(),
        });
    }
    let is_docs = docs.is_some() && complete.is_none();
    let entry = PathMatch {
        path: path.to_string(),
        covered_by,
    };
    Ok((entry, complete, is_docs))
}

/// Judges a file by its two versions: adds what its difference selects, records each
/// difference in `covered_by`, and returns why the complete suite is needed, if it is.
fn judge_content(
    builder: &mut Builder<'_>,
    inputs: Inputs<'_>,
    change: &Change,
    path: &str,
    covered_by: &mut Vec<String>,
) -> Option<String> {
    let Some(versions) = change.contents.get(path) else {
        return Some("no base version to compare".to_string());
    };
    if path == CATALOG_PATH {
        judge_catalog(builder, inputs, versions, covered_by)
    } else if path == LOCKFILE_PATH {
        judge_lockfile(inputs.workspace, change, versions, covered_by)
    } else {
        judge_manifest(builder, path, versions, covered_by)
    }
}

/// How a member manifest changed, when both of its versions are known.
fn manifest_change(versions: &FileVersions) -> Option<ManifestChange> {
    match (&versions.before, &versions.after) {
        (Some(before), Some(after)) => Some(classify_manifest(before, after)),
        _ => None,
    }
}

/// Judges a change of the member manifest `path` (plan §4.4): a dev-only change selects the
/// crate without its downstream contracts, plus every `[[test]]` target whose entry changed.
fn judge_manifest(
    builder: &mut Builder<'_>,
    path: &str,
    versions: &FileVersions,
    covered_by: &mut Vec<String>,
) -> Option<String> {
    let owner = manifest_owner(path, builder.workspace)?.to_string();
    match manifest_change(versions) {
        None => Some("manifest added or removed".to_string()),
        Some(ManifestChange::Runtime { section }) => Some(format!("{section} changed")),
        Some(ManifestChange::DevOnly { tests }) => {
            covered_by.push("dev sections changed only".to_string());
            for name in tests {
                let id = TargetId::test(&owner, &name);
                if builder.workspace.test_targets().any(|t| t.id == id) {
                    builder.add(id, Reason::Manifest(path.to_string()));
                }
            }
            builder.dev_only.insert(owner);
            None
        }
    }
}

/// Judges a change of `Cargo.lock` (plan §4.4): only members' dependency lists may differ, and
/// each such member's manifest must have changed in its dev sections only.
fn judge_lockfile(
    workspace: &Workspace,
    change: &Change,
    versions: &FileVersions,
    covered_by: &mut Vec<String>,
) -> Option<String> {
    let (Some(before), Some(after)) = (&versions.before, &versions.after) else {
        return Some("lockfile added or removed".to_string());
    };
    let members = match LockDiff::between(before, after) {
        LockDiff::Unreadable => return Some("lockfile unreadable".to_string()),
        LockDiff::External(package) => {
            return Some(format!("external package {package} changed"));
        }
        LockDiff::Membership(member) => {
            return Some(format!("workspace member {member} added or removed"));
        }
        LockDiff::Other => return Some("lockfile format or metadata changed".to_string()),
        LockDiff::MembersOnly(members) => members,
    };
    for member in &members {
        let dev_only = workspace
            .package(member)
            .map(|package| format!("{}/Cargo.toml", package.dir))
            .and_then(|manifest| change.contents.get(&manifest))
            .and_then(manifest_change)
            .is_some_and(|change| matches!(change, ManifestChange::DevOnly { .. }));
        if !dev_only {
            return Some(format!(
                "{member} dependencies changed without a dev-only manifest change"
            ));
        }
    }
    let names: Vec<&str> = members.iter().map(String::as_str).collect();
    covered_by.push(format!(
        "members' dependency lists only ({}), each with a dev-only manifest change",
        names.join(", ")
    ));
    None
}

/// Judges a change of the suite catalog by its semantic diff (plan §4.2).
fn judge_catalog(
    builder: &mut Builder<'_>,
    inputs: Inputs<'_>,
    versions: &FileVersions,
    covered_by: &mut Vec<String>,
) -> Option<String> {
    let parse = |text: &Option<String>| {
        text.as_deref()
            .and_then(|text| SuiteCatalog::parse(text, std::path::Path::new(CATALOG_PATH)).ok())
    };
    let Some(before) = parse(&versions.before) else {
        return Some("base catalog unreadable".to_string());
    };
    let Some(after) = parse(&versions.after) else {
        return Some("catalog unreadable".to_string());
    };
    let diff = CatalogDiff::between(&before, &after);
    covered_by.extend(diff.descriptions.iter().cloned());
    if !diff.scope.is_empty() {
        return Some(format!("catalog scope changed ({})", diff.scope.join(", ")));
    }
    let edit = |what: &str| Reason::Catalog(what.to_string());
    for id in &diff.targets {
        if inputs.workspace.test_targets().any(|t| &t.id == id) {
            builder.add(id.clone(), edit("targets entry changed"));
        }
    }
    for rule in &diff.rules {
        let reason = edit(&format!("rule `{}` changed", rule.name));
        for reference in &rule.targets {
            for id in inputs
                .catalog
                .resolve(reference, inputs.workspace)
                .unwrap_or_default()
            {
                builder.add(id, reason.clone());
            }
        }
        for krate in &rule.crates {
            if inputs.workspace.package(krate).is_some() {
                // A crate the workspace has always resolves.
                builder.add_crate(krate, &reason).ok();
            }
        }
    }
    for (key, added) in &diff.downstream_added {
        let reason = edit(&format!("downstream `{key}` gained it"));
        for reference in added {
            let ids = inputs
                .catalog
                .resolve_downstream(key, reference, inputs.workspace, inputs.sources)
                .unwrap_or_default();
            for id in ids {
                builder.add(id, reason.clone());
            }
        }
    }
    for name in &diff.commands {
        if inputs.catalog.commands.iter().any(|c| &c.name == name) {
            let reasons = builder.commands.entry(name.clone()).or_default();
            reasons.insert(edit("command changed"));
        }
    }
    None
}

/// The selection of `tier` for the changed paths `changed`, known by path only: a change to a
/// file judged by content selects the complete suite ([`Change::paths_only`]).
///
/// [`Tier::Merge`] selects like [`Tier::Package`]: the `merge` tier decides separately whether
/// to run that selection at all (`merge.rs`).
///
/// # Errors
/// [`GateError::Catalog`] when the catalog names a crate or target the workspace lacks.
pub fn select(
    tier: Tier,
    inputs: Inputs<'_>,
    changed: &BTreeSet<String>,
) -> Result<Selection, GateError> {
    select_change(tier, inputs, &Change::paths_only(changed.clone()))
}

/// Adds the downstream contracts of the changed crate `krate`.
fn add_downstream(
    builder: &mut Builder<'_>,
    inputs: Inputs<'_>,
    krate: &str,
) -> Result<(), GateError> {
    for reference in inputs.catalog.downstream.get(krate).into_iter().flatten() {
        let reason = if *reference == TargetRef::Callers {
            Reason::Callers(krate.to_string())
        } else {
            Reason::Downstream(krate.to_string())
        };
        let ids = inputs.catalog.resolve_downstream(
            krate,
            reference,
            inputs.workspace,
            inputs.sources,
        )?;
        for id in ids {
            builder.add(id, reason.clone());
        }
    }
    Ok(())
}

/// The selection of `tier` for `change`, judging the catalog by its content where both of its
/// versions are known.
///
/// # Errors
/// As [`select`].
pub fn select_change(
    tier: Tier,
    inputs: Inputs<'_>,
    change: &Change,
) -> Result<Selection, GateError> {
    let mut builder = Builder {
        catalog: inputs.catalog,
        workspace: inputs.workspace,
        targets: BTreeMap::new(),
        commands: BTreeMap::new(),
        dev_only: BTreeSet::new(),
    };
    for reference in &inputs.catalog.always {
        builder.add_ref(reference, &Reason::Always)?;
    }
    let mut complete = (tier == Tier::Full).then_some(CompleteCause::Tier);
    let mut docs_only = !change.paths.is_empty();
    let mut changed_crates = BTreeSet::new();
    let mut paths = Vec::new();
    for path in &change.paths {
        let (entry, cause, is_docs) =
            judge_path(&mut builder, inputs, change, path, &mut changed_crates)?;
        paths.push(entry);
        docs_only &= is_docs;
        if complete.is_none() {
            complete = cause;
        }
    }
    for krate in &changed_crates {
        builder.add_crate(krate, &Reason::Crate(krate.clone()))?;
        add_downstream(&mut builder, inputs, krate)?;
    }
    // A dev-dependency never reaches a dependent: no downstream contract (plan §4.4).
    for krate in std::mem::take(&mut builder.dev_only) {
        builder.add_crate(&krate, &Reason::Crate(krate.clone()))?;
        changed_crates.insert(krate);
    }
    if complete.is_some() {
        docs_only = false;
        for target in inputs.workspace.test_targets() {
            builder.add(target.id.clone(), Reason::Complete);
        }
        for command in &inputs.catalog.commands {
            let reasons = builder.commands.entry(command.name.clone()).or_default();
            reasons.insert(Reason::Complete);
        }
    }
    let mut selection = Selection {
        tier,
        paths,
        complete,
        docs_only,
        changed_crates,
        targets: builder.targets,
        commands: builder.commands,
        clippy: if docs_only {
            ClippyScope::Skip
        } else {
            ClippyScope::Workspace
        },
    };
    if tier == Tier::Quick {
        narrow_to_quick(&mut selection);
    }
    Ok(selection)
}

/// Keeps what `quick` runs: the `fast` targets the change selects directly (its crates, its
/// path rules, the `always` list), no commands, and clippy on the changed crates only.
fn narrow_to_quick(selection: &mut Selection) {
    selection.targets.retain(|_, selected| {
        selected.class == TestClass::Fast
            && selected.reasons.iter().any(|reason| {
                matches!(
                    reason,
                    Reason::Always | Reason::Complete | Reason::Crate(_) | Reason::Rule(_)
                )
            })
    });
    selection.commands.clear();
    selection.clippy = if selection.complete.is_some() {
        ClippyScope::Workspace
    } else if selection.changed_crates.is_empty() {
        ClippyScope::Skip
    } else {
        ClippyScope::Packages(selection.changed_crates.clone())
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{DependencyKind, Package, Target, TargetKind};
    use std::path::Path;

    const CATALOG: &str = r#"SuiteCatalog(
        global: ["Cargo.toml", "crates/*/Cargo.toml"],
        docs: ["docs/**", "**/*.md"],
        always: ["checks/@fast"],
        targets: {
            "app/lib": (class: Slow),
            "app/gpu_tests": (class: Gpu, triggers: ["crates/app/src/render/**"]),
            "app/alloc_tests": (class: Valgrind),
        },
        rules: [(name: "shaders", paths: ["assets/shaders/**"], targets: ["app/@gpu"]),
                (name: "catalog", paths: ["assets/catalog/**"], crates: ["core"])],
        downstream: {"core": ["app/headless_tests"]},
        commands: [(name: "golden", paths: ["crates/core/**"], program: "cargo", args: ["test"])],
    )"#;

    fn package(name: &str, tests: &[&str]) -> Package {
        let mut targets = vec![Target {
            id: TargetId::lib(name),
            src_path: format!("crates/{name}/src/lib.rs"),
        }];
        targets.extend(tests.iter().map(|test| Target {
            id: TargetId::test(name, test),
            src_path: format!("crates/{name}/tests/{test}.rs"),
        }));
        Package {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            dir: format!("crates/{name}"),
            dependencies: BTreeMap::from([("core".to_string(), DependencyKind::Normal)]),
            targets,
        }
    }

    struct Fixture {
        catalog: SuiteCatalog,
        workspace: Workspace,
        sources: SourceIndex,
    }

    impl Fixture {
        fn new() -> Result<Self, GateError> {
            let mut workspace = Workspace::default();
            for p in [
                package("core", &["unit_tests"]),
                package("app", &["gpu_tests", "alloc_tests", "headless_tests"]),
                package("checks", &["charter"]),
            ] {
                workspace.packages.insert(p.name.clone(), p);
            }
            let sources = SourceIndex::from_files(BTreeMap::from([(
                TargetId::test("app", "gpu_tests"),
                BTreeSet::from([
                    "crates/app/tests/gpu_tests.rs".to_string(),
                    "crates/app/tests/support/gpu.rs".to_string(),
                ]),
            )]));
            Ok(Self {
                catalog: SuiteCatalog::parse(CATALOG, Path::new("suites.ron"))?,
                workspace,
                sources,
            })
        }

        fn select(&self, tier: Tier, paths: &[&str]) -> Result<Selection, GateError> {
            let changed = paths.iter().map(ToString::to_string).collect();
            let inputs = Inputs {
                catalog: &self.catalog,
                workspace: &self.workspace,
                sources: &self.sources,
            };
            select(tier, inputs, &changed)
        }
    }

    /// Selected target names, alphabetically.
    fn names(selection: &Selection) -> Vec<String> {
        let mut names: Vec<String> = selection.targets.keys().map(ToString::to_string).collect();
        names.sort();
        names
    }

    #[test]
    fn a_crate_change_selects_fast_slow_and_downstream_but_not_gpu() -> Result<(), GateError> {
        let selection = Fixture::new()?.select(Tier::Package, &["crates/core/src/a.rs"])?;
        assert_eq!(
            names(&selection),
            [
                "app/headless_tests",
                "checks/charter",
                "checks/lib",
                "core/lib",
                "core/unit_tests"
            ]
        );
        assert!(selection.commands.contains_key("golden"));
        assert_eq!(selection.clippy, ClippyScope::Workspace);
        assert!(selection.complete.is_none());
        Ok(())
    }

    #[test]
    fn gpu_targets_run_on_their_triggers_and_own_files() -> Result<(), GateError> {
        let fixture = Fixture::new()?;
        let plain = fixture.select(Tier::Package, &["crates/app/src/input.rs"])?;
        assert!(!plain
            .targets
            .contains_key(&TargetId::test("app", "gpu_tests")));
        assert!(
            plain.targets.contains_key(&TargetId::lib("app")),
            "slow runs"
        );
        let render = fixture.select(Tier::Package, &["crates/app/src/render/pass.rs"])?;
        let gpu = &render.targets[&TargetId::test("app", "gpu_tests")];
        assert!(gpu
            .reasons
            .contains(&Reason::Trigger("crates/app/src/render/**".into())));
        let support = fixture.select(Tier::Package, &["crates/app/tests/support/gpu.rs"])?;
        assert!(support
            .targets
            .contains_key(&TargetId::test("app", "gpu_tests")));
        let shader = fixture.select(Tier::Package, &["assets/shaders/a.wgsl"])?;
        assert!(shader
            .targets
            .contains_key(&TargetId::test("app", "gpu_tests")));
        assert!(!shader.targets.contains_key(&TargetId::lib("app")));
        Ok(())
    }

    #[test]
    fn global_and_unmatched_paths_select_the_complete_suite() -> Result<(), GateError> {
        let fixture = Fixture::new()?;
        let all = fixture.workspace.test_targets().count();
        let global = fixture.select(Tier::Package, &["crates/core/Cargo.toml"])?;
        assert!(matches!(
            global.complete,
            Some(CompleteCause::Global { .. })
        ));
        assert_eq!(global.targets.len(), all);
        let unknown = fixture.select(Tier::Package, &["tools/new_thing.sh"])?;
        assert_eq!(
            unknown.complete,
            Some(CompleteCause::Unmatched {
                path: "tools/new_thing.sh".into()
            })
        );
        assert_eq!(unknown.targets.len(), all);
        assert!(unknown.commands.contains_key("golden"));
        Ok(())
    }

    #[test]
    fn docs_only_changes_run_the_checks_and_skip_clippy() -> Result<(), GateError> {
        let fixture = Fixture::new()?;
        let docs = fixture.select(Tier::Package, &["docs/plans/x.md", "crates/core/README.md"])?;
        assert!(docs.docs_only);
        assert_eq!(docs.clippy, ClippyScope::Skip);
        assert_eq!(names(&docs), ["checks/charter", "checks/lib"]);
        let mixed = fixture.select(Tier::Package, &["docs/x.md", "crates/core/src/a.rs"])?;
        assert!(!mixed.docs_only);
        Ok(())
    }

    #[test]
    fn path_rules_select_crates_and_class_groups() -> Result<(), GateError> {
        let selection = Fixture::new()?.select(Tier::Package, &["assets/catalog/units.ron"])?;
        assert!(selection.targets.contains_key(&TargetId::lib("core")));
        assert!(selection.changed_crates.is_empty());
        let rule = Reason::Rule("catalog".into());
        assert!(selection.targets[&TargetId::lib("core")]
            .reasons
            .contains(&rule));
        Ok(())
    }

    #[test]
    fn quick_keeps_direct_fast_targets_and_narrows_clippy() -> Result<(), GateError> {
        let fixture = Fixture::new()?;
        let quick = fixture.select(Tier::Quick, &["crates/core/src/a.rs"])?;
        assert_eq!(
            names(&quick),
            [
                "checks/charter",
                "checks/lib",
                "core/lib",
                "core/unit_tests"
            ]
        );
        assert!(quick.commands.is_empty());
        assert_eq!(
            quick.clippy,
            ClippyScope::Packages(BTreeSet::from(["core".to_string()]))
        );
        let app = fixture.select(Tier::Quick, &["crates/app/src/x.rs"])?;
        assert!(
            !app.targets.contains_key(&TargetId::lib("app")),
            "slow is not quick"
        );
        Ok(())
    }

    #[test]
    fn full_selects_every_target_and_command() -> Result<(), GateError> {
        let fixture = Fixture::new()?;
        let full = fixture.select(Tier::Full, &[])?;
        assert_eq!(full.complete, Some(CompleteCause::Tier));
        let every: Vec<String> = fixture
            .workspace
            .test_targets()
            .map(|t| t.id.to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(names(&full), every);
        assert_eq!(full.commands.len(), 1);
        assert!(matches!(
            full.targets[&TargetId::test("app", "alloc_tests")].class,
            TestClass::Valgrind
        ));
        assert!(matches!(
            TargetId::test("app", "x").kind,
            TargetKind::Test(_)
        ));
        Ok(())
    }

    #[test]
    fn a_callers_contract_selects_the_computed_callers_with_their_reason() -> Result<(), GateError>
    {
        let mut fixture = Fixture::new()?;
        fixture
            .catalog
            .downstream
            .insert("core".into(), vec![TargetRef::Callers]);
        let caller = TargetId::test("app", "alloc_tests");
        fixture.sources = fixture
            .sources
            .clone()
            .with_callers("core", BTreeSet::from([caller.clone()]));
        let selection = fixture.select(Tier::Package, &["crates/core/src/a.rs"])?;
        assert!(selection.targets[&caller]
            .reasons
            .contains(&Reason::Callers("core".into())));
        assert!(!selection
            .targets
            .contains_key(&TargetId::test("app", "headless_tests")));
        assert_eq!(
            Reason::Callers("core".into()).to_string(),
            "downstream of core (calls core::)"
        );
        Ok(())
    }

    #[test]
    fn unknown_catalog_targets_are_errors_not_silence() -> Result<(), GateError> {
        let mut fixture = Fixture::new()?;
        fixture
            .catalog
            .downstream
            .insert("core".into(), vec![TargetRef::parse("app/missing_tests")?]);
        assert!(matches!(
            fixture.select(Tier::Package, &["crates/core/src/a.rs"]),
            Err(GateError::Catalog(_))
        ));
        Ok(())
    }
}
