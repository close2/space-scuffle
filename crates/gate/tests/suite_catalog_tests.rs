//! Charter check C22: `crates/gate/suites.ron` describes this repository.
//!
//! # Responsibility
//! Holds the suite catalog to the workspace (blueprint §5.4): every target it names exists and
//! every class group has a member; every file that runs GPU children or valgrind is classed
//! `gpu` or `valgrind`, and every `gpu` target really uses the harness; every downstream contract
//! points into a reverse dependency; the `sim_fixtures` contract is the computed `@callers`, and
//! the computation equals an independent scan of the targets whose code calls `sim_fixtures::`; every glob matches a tracked file; every tracked path is covered
//! by some entry; and `full` selects every test target `cargo metadata` lists.
//!
//! # How it fits
//! The selection trusts the catalog; this test is why it may. A new GPU test file, a renamed
//! target or a new top-level directory fails here, in the same `cargo test` run that would
//! otherwise silently select too little or the complete suite.
//!
//! # Invariants
//! - Every test reports all of its findings at once, one per line.
//! - The repository is the checkout this test was built from (`CARGO_MANIFEST_DIR`).

use gate::{
    build_invocations, invokes_valgrind, mentions_path_root, select, uses_gpu_harness,
    CompleteCause, GateError, Git, Inputs, SourceIndex, SuiteCatalog, TargetId, TargetRef,
    TestClass, Tier, Workspace, CALLERS, CATALOG_PATH,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The repository root, two levels above this crate.
fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

/// The repository's catalog, workspace, sources and tracked files.
struct Repo {
    root: PathBuf,
    catalog: SuiteCatalog,
    workspace: Workspace,
    sources: SourceIndex,
    tracked: BTreeSet<String>,
}

impl Repo {
    fn load() -> Result<Self, GateError> {
        let root = root();
        let workspace = Workspace::load(&root)?;
        let sources = SourceIndex::build(&root, &workspace)?;
        let tracked = Git::discover(&root)?.tracked_files()?;
        Ok(Self {
            catalog: SuiteCatalog::load(&root)?,
            workspace,
            sources,
            tracked,
            root,
        })
    }

    fn inputs(&self) -> Inputs<'_> {
        Inputs {
            catalog: &self.catalog,
            workspace: &self.workspace,
            sources: &self.sources,
        }
    }

    /// Every reference the catalog makes, with where it makes it.
    fn references(&self) -> Vec<(String, TargetRef)> {
        let mut out: Vec<(String, TargetRef)> = Vec::new();
        out.extend(
            self.catalog
                .always
                .iter()
                .map(|r| ("always".to_string(), r.clone())),
        );
        out.extend(
            self.catalog
                .targets
                .keys()
                .map(|id| ("targets".to_string(), TargetRef::Target(id.clone()))),
        );
        for rule in &self.catalog.rules {
            let place = format!("rule `{}`", rule.name);
            out.extend(rule.targets.iter().map(|r| (place.clone(), r.clone())));
        }
        for (krate, list) in &self.catalog.downstream {
            let place = format!("downstream `{krate}`");
            out.extend(
                list.iter()
                    .filter(|r| **r != TargetRef::Callers)
                    .map(|r| (place.clone(), r.clone())),
            );
        }
        out
    }
}

/// Whether `path` is a Rust source file.
fn is_rust(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|ext| ext == "rs")
}

/// Fails with every finding, or passes when there are none.
fn assert_clean(what: &str, findings: &[String]) {
    assert!(
        findings.is_empty(),
        "C22 {what}: {} finding(s)\n{}",
        findings.len(),
        findings.join("\n")
    );
}

#[test]
fn every_named_target_and_crate_exists_and_every_class_group_has_a_member() -> Result<(), GateError>
{
    let repo = Repo::load()?;
    let mut findings = Vec::new();
    for (place, reference) in repo.references() {
        match repo.catalog.resolve(&reference, &repo.workspace) {
            Ok(ids) if ids.is_empty() => {
                findings.push(format!(
                    "{place}: `{reference}` names a class with no member"
                ));
            }
            Ok(_) => {}
            Err(error) => findings.push(format!("{place}: {error}")),
        }
    }
    let crates = repo.catalog.rules.iter().flat_map(|rule| {
        rule.crates
            .iter()
            .map(move |krate| (format!("rule `{}`", rule.name), krate))
    });
    for (place, krate) in crates {
        if repo.workspace.package(krate).is_none() {
            findings.push(format!("{place}: `{krate}` is not a workspace crate"));
        }
    }
    assert_clean("names", &findings);
    Ok(())
}

#[test]
fn every_gpu_and_valgrind_file_is_classed_and_every_gpu_target_uses_the_harness(
) -> Result<(), GateError> {
    let repo = Repo::load()?;
    let mut findings = Vec::new();
    let mut harness_users: BTreeSet<TargetId> = BTreeSet::new();
    for file in repo.sources.all_files().iter().filter(|f| is_rust(f)) {
        let full = repo.root.join(file);
        let text = std::fs::read_to_string(&full).map_err(|source| GateError::Io {
            path: full.clone(),
            source,
        })?;
        let gpu = uses_gpu_harness(&text);
        let valgrind = invokes_valgrind(&text);
        for target in repo.sources.targets_of(file) {
            let class = repo.catalog.class_of(&target);
            if gpu {
                harness_users.insert(target.clone());
            }
            if gpu && class != TestClass::Gpu {
                findings.push(format!(
                    "{file} runs GPU children, so `{target}` must be classed Gpu, not {class}"
                ));
            }
            if valgrind && class != TestClass::Valgrind {
                findings.push(format!(
                    "{file} starts valgrind, so `{target}` must be classed Valgrind, not {class}"
                ));
            }
        }
    }
    for (target, entry) in &repo.catalog.targets {
        if entry.class == TestClass::Gpu && !harness_users.contains(target) {
            findings.push(format!(
                "`{target}` is classed Gpu, but none of its files uses the GPU harness"
            ));
        }
    }
    assert_clean("classes", &findings);
    Ok(())
}

#[test]
fn downstream_contracts_point_into_reverse_dependencies() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let mut findings = Vec::new();
    for (krate, list) in &repo.catalog.downstream {
        if repo.workspace.package(krate).is_none() {
            findings.push(format!("downstream key `{krate}` is not a workspace crate"));
            continue;
        }
        let reverse = repo.workspace.reverse_dependencies(krate);
        let mut packages: BTreeSet<String> = BTreeSet::new();
        for reference in list {
            match reference {
                TargetRef::Callers => packages.extend(
                    repo.sources
                        .callers_of(krate)
                        .into_iter()
                        .map(|id| id.package),
                ),
                other => packages.extend(other.package().map(ToString::to_string)),
            }
        }
        for package in packages {
            if !reverse.contains(&package) {
                findings.push(format!(
                    "downstream `{krate}`: `{package}` does not depend on it"
                ));
            }
        }
    }
    assert_clean("downstream", &findings);
    Ok(())
}

#[test]
fn the_sim_fixtures_contract_is_computed_from_its_callers() -> Result<(), GateError> {
    let repo = Repo::load()?;
    assert_eq!(
        repo.catalog.downstream.get("sim_fixtures"),
        Some(&vec![TargetRef::Callers]),
        "downstream `sim_fixtures` must be [\"{CALLERS}\"]"
    );
    // The oracle: an independent scan of every target's files, not the index's own pass.
    let mut scanned = BTreeSet::new();
    for target in repo.workspace.test_targets() {
        if target.id.package == "sim_fixtures" {
            continue;
        }
        for file in repo.sources.files_of(&target.id) {
            let full = repo.root.join(&file);
            let text = std::fs::read_to_string(&full).unwrap_or_default();
            if is_rust(&file) && mentions_path_root(&text, "sim_fixtures") {
                scanned.insert(target.id.clone());
            }
        }
    }
    let computed = repo.sources.callers_of("sim_fixtures");
    let mut findings: Vec<String> = scanned
        .difference(&computed)
        .map(|id| format!("`{id}` calls sim_fixtures:: but `callers_of` omits it"))
        .collect();
    findings.extend(
        computed
            .difference(&scanned)
            .map(|id| format!("`callers_of` lists `{id}`, which does not call sim_fixtures::")),
    );
    assert_clean("sim_fixtures contract", &findings);
    assert!(
        scanned.len() >= 50,
        "about 80 targets call sim_fixtures, found {}",
        scanned.len()
    );
    Ok(())
}

#[test]
fn callers_is_accepted_only_as_a_downstream_value() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let full = repo.root.join(CATALOG_PATH);
    let text = std::fs::read_to_string(&full).map_err(|source| GateError::Io {
        path: full.clone(),
        source,
    })?;
    let path = Path::new(CATALOG_PATH);
    let in_always = text.replacen(
        "\"workspace_checks/@fast\",\n    ],",
        "\"workspace_checks/@fast\",\n        \"@callers\",\n    ],",
        1,
    );
    assert_ne!(in_always, text, "the `always` list must be found");
    assert!(matches!(
        SuiteCatalog::parse(&in_always, path),
        Err(GateError::Catalog(_))
    ));
    let in_rule = text.replacen("targets: [\"client/@gpu\"]", "targets: [\"@callers\"]", 1);
    assert_ne!(in_rule, text, "a rule's targets must be found");
    assert!(matches!(
        SuiteCatalog::parse(&in_rule, path),
        Err(GateError::Catalog(_))
    ));
    for (krate, list) in &repo.catalog.downstream {
        if list.contains(&TargetRef::Callers) {
            assert_eq!(
                krate, "sim_fixtures",
                "`@callers` is not offered for `{krate}`: every client target names it"
            );
        }
    }
    Ok(())
}

#[test]
fn every_glob_matches_a_tracked_file() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let catalog = &repo.catalog;
    let mut globs: Vec<(String, &gate::Glob)> = Vec::new();
    globs.extend(catalog.global.iter().map(|g| ("global".to_string(), g)));
    globs.extend(catalog.docs.iter().map(|g| ("docs".to_string(), g)));
    for (id, entry) in &catalog.targets {
        globs.extend(
            entry
                .triggers
                .iter()
                .map(|g| (format!("triggers of {id}"), g)),
        );
    }
    for rule in &catalog.rules {
        globs.extend(
            rule.paths
                .iter()
                .map(|g| (format!("rule `{}`", rule.name), g)),
        );
    }
    for command in &catalog.commands {
        globs.extend(
            command
                .paths
                .iter()
                .map(|g| (format!("command `{}`", command.name), g)),
        );
    }
    let findings: Vec<String> = globs
        .into_iter()
        .filter(|(_, glob)| !repo.tracked.iter().any(|path| glob.matches(path)))
        .map(|(place, glob)| format!("{place}: `{glob}` matches no tracked file"))
        .collect();
    assert_clean("globs", &findings);
    Ok(())
}

#[test]
fn every_tracked_path_is_covered_by_a_catalog_entry() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let mut findings = Vec::new();
    for path in &repo.tracked {
        let selection = select(
            Tier::Package,
            repo.inputs(),
            &BTreeSet::from([path.clone()]),
        )?;
        if let Some(CompleteCause::Unmatched { .. }) = selection.complete {
            findings.push(format!(
                "{path}: no catalog entry covers it, so a change to it selects the complete suite"
            ));
        }
    }
    assert_clean("coverage", &findings);
    Ok(())
}

#[test]
fn full_selects_every_test_target_cargo_lists() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let full = select(Tier::Full, repo.inputs(), &BTreeSet::new())?;
    let listed: BTreeSet<TargetId> = repo
        .workspace
        .test_targets()
        .map(|t| t.id.clone())
        .collect();
    let selected: BTreeSet<TargetId> = full.targets.keys().cloned().collect();
    assert_eq!(selected, listed);
    assert!(
        listed.len() >= 150,
        "the workspace has about 150 test targets, found {}",
        listed.len()
    );
    assert_eq!(full.commands.len(), repo.catalog.commands.len());
    let builds = build_invocations(&selected, full.complete.is_some());
    assert_eq!(builds.len(), 1);
    assert!(builds[0].contains(&"--workspace".to_string()));
    Ok(())
}

/// The `exclusive: true` entries of catalog `text` whose line, or the comment lines right above
/// it, name no evidence: a `docs/evidence/` file, a `.log` or a `lanes.tsv`.
fn exclusive_without_evidence(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let names_evidence = |line: &str| {
        line.split_once("//").is_some_and(|(_, comment)| {
            comment.contains("docs/evidence/")
                || comment.contains(".log")
                || comment.contains("lanes.tsv")
        })
    };
    let mut findings = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !line
            .split("//")
            .next()
            .unwrap_or("")
            .contains("exclusive: true")
        {
            continue;
        }
        let mut comments_above = lines[..index]
            .iter()
            .rev()
            .take_while(|above| above.trim_start().starts_with("//"));
        if !names_evidence(line) && !comments_above.any(|above| names_evidence(above)) {
            findings.push(format!("line {}: {}", index + 1, line.trim()));
        }
    }
    findings
}

#[test]
fn every_exclusive_target_names_its_evidence_in_a_comment() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let full = repo.root.join(CATALOG_PATH);
    let text = std::fs::read_to_string(&full).map_err(|source| GateError::Io {
        path: full.clone(),
        source,
    })?;
    assert_clean("exclusive evidence", &exclusive_without_evidence(&text));
    let bare = "    \"a/x_tests\": (class: Slow, exclusive: true),\n";
    assert_eq!(
        exclusive_without_evidence(bare).len(),
        1,
        "the check finds a bare mark"
    );
    let cited = "    // Fails under load: docs/evidence/2026-11-12-gate-lanes.md.\n    \"a/x_tests\": (class: Slow, exclusive: true),\n";
    assert_eq!(exclusive_without_evidence(cited), Vec::<String>::new());
    Ok(())
}
