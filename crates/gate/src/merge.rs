//! The `merge` tier's decisions: which stamp a merged branch earned, and whether `main` moved
//! under it.
//!
//! # Responsibility
//! Holds the pure parts of blueprint §3.4: find the newest commit of a branch whose tree has a
//! `package` stamp ([`choose_stamp`]), the crates a range of `main` touched
//! ([`changed_crates`]), and their overlap with the crates the stamp tested
//! ([`overlapping_crates`]).
//!
//! # How it fits
//! The runner's `merge` command reads the branch's commits and the stamps from disk, then asks
//! these functions what to run: the stamp's `fast` and `slow` targets again when `main` changed
//! a crate the branch also tested, the `package` selection on the merge result when the branch
//! has no valid stamp, and only fmt, clippy and the charter checks otherwise.
//!
//! # Invariants
//! - A stamp stays valid only if every later commit on the branch changed docs alone; the
//!   runner checks that with the catalog's `docs` globs.
//! - A path of `main` that is neither docs, nor in a crate, nor under a path rule counts as a
//!   change to every crate (uncertain means more).

use crate::catalog::SuiteCatalog;
use crate::stamp::Stamp;
use crate::workspace::Workspace;
use crate::GateError;
use std::collections::BTreeSet;

/// One commit of a branch and its tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchCommit {
    /// The commit hash.
    pub commit: String,
    /// Its tree hash.
    pub tree: String,
}

/// The newest commit of `commits` (ordered newest first) with a `package` stamp, as its index
/// and the stamp; `find` looks a stamp up by tree hash.
///
/// # Errors
/// Whatever `find` returns.
pub fn choose_stamp(
    commits: &[BranchCommit],
    mut find: impl FnMut(&str) -> Result<Option<Stamp>, GateError>,
) -> Result<Option<(usize, Stamp)>, GateError> {
    for (index, commit) in commits.iter().enumerate() {
        if let Some(stamp) = find(&commit.tree)? {
            return Ok(Some((index, stamp)));
        }
    }
    Ok(None)
}

/// The crates whose tests a change to `paths` can affect: crates holding a changed non-docs
/// file, crates a matching path rule names, and every crate for a path nothing covers.
#[must_use]
pub fn changed_crates(
    paths: &BTreeSet<String>,
    workspace: &Workspace,
    catalog: &SuiteCatalog,
) -> BTreeSet<String> {
    let every: BTreeSet<String> = workspace.packages.keys().cloned().collect();
    let mut out = BTreeSet::new();
    for path in paths {
        if catalog.global.iter().any(|glob| glob.matches(path)) {
            return every;
        }
        if catalog.docs.iter().any(|glob| glob.matches(path)) {
            continue;
        }
        let mut covered = false;
        if let Some(package) = workspace.package_of_path(path) {
            out.insert(package.name.clone());
            covered = true;
        }
        for rule in &catalog.rules {
            if rule.paths.iter().any(|glob| glob.matches(path)) {
                covered = true;
                out.extend(rule.crates.iter().cloned());
                out.extend(rule.targets.iter().map(|r| r.package().to_string()));
            }
        }
        if !covered {
            return every;
        }
    }
    out
}

/// The crates both sets name.
#[must_use]
pub fn overlapping_crates(
    stamped: &BTreeSet<String>,
    main_changed: &BTreeSet<String>,
) -> BTreeSet<String> {
    stamped.intersection(main_changed).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tier::Tier;
    use crate::workspace::Package;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn stamp(tree: &str) -> Stamp {
        Stamp {
            tier: Tier::Package,
            tree: tree.into(),
            commit: String::new(),
            base: None,
            host: String::new(),
            finished_unix_s: 0,
            targets: Vec::new(),
            commands: Vec::new(),
            flakes: Vec::new(),
        }
    }

    #[test]
    fn the_newest_stamped_commit_wins() -> Result<(), GateError> {
        let commits: Vec<BranchCommit> = ["t3", "t2", "t1"]
            .iter()
            .map(|tree| BranchCommit {
                commit: format!("c-{tree}"),
                tree: (*tree).into(),
            })
            .collect();
        let found = choose_stamp(&commits, |tree| {
            Ok((tree == "t2" || tree == "t1").then(|| stamp(tree)))
        })?;
        assert_eq!(found.map(|(i, s)| (i, s.tree)), Some((1, "t2".to_string())));
        assert!(choose_stamp(&commits, |_| Ok(None))?.is_none());
        Ok(())
    }

    #[test]
    fn main_changes_map_to_crates_through_paths_and_rules() -> Result<(), GateError> {
        let catalog = SuiteCatalog::parse(
            r#"SuiteCatalog(global: ["Cargo.lock"], docs: ["docs/**"],
               rules: [(name: "catalog", paths: ["assets/catalog/**"], crates: ["core"],
                        targets: ["app/headless_tests"])])"#,
            Path::new("s"),
        )?;
        let mut workspace = Workspace::default();
        for name in ["core", "app", "tools"] {
            workspace.packages.insert(
                name.into(),
                Package {
                    name: name.into(),
                    version: "0.1.0".into(),
                    dir: format!("crates/{name}"),
                    dependencies: BTreeMap::new(),
                    targets: Vec::new(),
                },
            );
        }
        let set = |items: &[&str]| {
            items
                .iter()
                .map(ToString::to_string)
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            changed_crates(
                &set(&["docs/a.md", "crates/tools/src/x.rs"]),
                &workspace,
                &catalog
            ),
            set(&["tools"])
        );
        assert_eq!(
            changed_crates(&set(&["assets/catalog/units.ron"]), &workspace, &catalog),
            set(&["app", "core"])
        );
        assert_eq!(
            changed_crates(&set(&["Cargo.lock"]), &workspace, &catalog),
            set(&["app", "core", "tools"])
        );
        assert_eq!(
            changed_crates(&set(&["somewhere/else"]), &workspace, &catalog),
            set(&["app", "core", "tools"])
        );
        assert_eq!(
            overlapping_crates(&set(&["core", "app"]), &set(&["tools", "core"])),
            set(&["core"])
        );
        Ok(())
    }
}
