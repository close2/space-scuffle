//! Pass records ("stamps"): which tier passed on which Git tree, with the targets it ran.
//!
//! # Responsibility
//! Defines [`Stamp`], its RON file format and its place on disk:
//! `<stamp dir>/<tree hash>.<tier>.ron`, by default under `~/scratch/gate-stamps/` (blueprint
//! §3.4, charter §7). Only a clean working tree can be stamped: its tree hash then names exactly
//! the files that were tested ([`StampTree::of_clean_head`]).
//!
//! # How it fits
//! The runner writes a stamp when a `package`, `merge` or `full` run is green. The `merge` tier
//! looks up a branch's newest `package` stamp by tree hash, and the timing report reads every
//! stamp for the class boundaries of blueprint §4.4. Stamps are local evidence, never committed.
//!
//! # Invariants
//! - A stamp is keyed by tree hash, not commit: two commits with the same files share it, and a
//!   rebase that changes no file keeps it valid.
//! - A dirty working tree is never stamped.
//! - Reading a stamp that does not exist is `Ok(None)`; reading a malformed one is an error.

use crate::catalog::TestClass;
use crate::changes::Git;
use crate::tier::Tier;
use crate::GateError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One target's result inside a stamp.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StampTarget {
    /// The target, spelt as in the catalog (`client/lib`).
    pub target: String,
    /// Its class at the time of the run.
    pub class: TestClass,
    /// Wall time in milliseconds.
    pub wall_ms: u64,
    /// CPU time of the executable and its children in milliseconds, where the host reports it.
    pub cpu_ms: Option<u64>,
    /// Whether it passed.
    pub passed: bool,
}

/// The record of one green tier run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// The tier that passed.
    pub tier: Tier,
    /// The tree hash of the tested working tree (`git rev-parse HEAD^{tree}`).
    pub tree: String,
    /// The commit that was checked out.
    pub commit: String,
    /// The `main` commit the branch was merged with (`git merge-base HEAD main`), if known.
    pub base: Option<String>,
    /// The machine that ran it.
    pub host: String,
    /// When the run finished, in seconds since the Unix epoch.
    pub finished_unix_s: u64,
    /// Every target run, in run order.
    pub targets: Vec<StampTarget>,
    /// The catalog commands run.
    pub commands: Vec<String>,
    /// Targets that failed and then passed on an immediate rerun in an earlier attempt of this
    /// run, which was therefore repeated (blueprint §3.5). Stamps written before this field
    /// existed read as none.
    #[serde(default)]
    pub flakes: Vec<String>,
}

/// The tree and commit of a clean working tree: what a stamp may name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StampTree {
    /// The tree hash of `HEAD`.
    pub tree: String,
    /// The `HEAD` commit.
    pub commit: String,
}

impl StampTree {
    /// The tree of `HEAD`, if the working tree has no uncommitted or untracked change.
    ///
    /// # Errors
    /// [`GateError::DirtyTree`] when it has one; Git errors otherwise.
    pub fn of_clean_head(git: &Git) -> Result<Self, GateError> {
        if !git.is_clean()? {
            return Err(GateError::DirtyTree(git.root().display().to_string()));
        }
        Ok(Self {
            tree: git.tree("HEAD")?,
            commit: git.commit("HEAD")?,
        })
    }
}

impl Stamp {
    /// The file name of the stamp for `tree` and `tier`.
    #[must_use]
    pub fn file_name(tree: &str, tier: Tier) -> String {
        format!("{tree}.{}.ron", tier.name())
    }

    /// The stamp as RON text.
    ///
    /// # Errors
    /// [`GateError::StampWrite`] if serialization fails.
    pub fn to_ron(&self) -> Result<String, GateError> {
        Ok(ron::ser::to_string_pretty(
            self,
            ron::ser::PrettyConfig::default(),
        )?)
    }

    /// Parses RON text; `path` only names the file in errors.
    ///
    /// # Errors
    /// [`GateError::StampSyntax`] for malformed text.
    pub fn from_ron(text: &str, path: &Path) -> Result<Self, GateError> {
        ron::from_str(text).map_err(|source| GateError::StampSyntax {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Writes the stamp into `dir` (created if missing) and returns its path.
    ///
    /// # Errors
    /// [`GateError::Io`] if the directory or file cannot be written.
    pub fn write(&self, dir: &Path) -> Result<PathBuf, GateError> {
        std::fs::create_dir_all(dir).map_err(|source| GateError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = dir.join(Self::file_name(&self.tree, self.tier));
        std::fs::write(&path, self.to_ron()?).map_err(|source| GateError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }

    /// The stamp for `tree` and `tier` in `dir`, if there is one.
    ///
    /// # Errors
    /// [`GateError::Io`] for an unreadable file; [`GateError::StampSyntax`] for a malformed one.
    pub fn read(dir: &Path, tree: &str, tier: Tier) -> Result<Option<Self>, GateError> {
        let path = dir.join(Self::file_name(tree, tier));
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::from_ron(&text, &path).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(GateError::Io { path, source }),
        }
    }

    /// Every stamp in `dir`, oldest first; a missing directory holds none.
    ///
    /// # Errors
    /// As [`Stamp::read`].
    pub fn read_all(dir: &Path) -> Result<Vec<Self>, GateError> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(Vec::new());
        };
        let mut stamps = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| GateError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "ron") {
                let text = std::fs::read_to_string(&path).map_err(|source| GateError::Io {
                    path: path.clone(),
                    source,
                })?;
                stamps.push(Self::from_ron(&text, &path)?);
            }
        }
        stamps.sort_by(|a, b| {
            a.finished_unix_s
                .cmp(&b.finished_unix_s)
                .then_with(|| a.tree.cmp(&b.tree))
        });
        Ok(stamps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::capture;

    fn io(path: &Path) -> impl FnOnce(std::io::Error) -> GateError + '_ {
        move |source| GateError::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    fn sample() -> Stamp {
        Stamp {
            tier: Tier::Package,
            tree: "abc123".into(),
            commit: "def456".into(),
            base: Some("0011".into()),
            host: "vm".into(),
            finished_unix_s: 1_700_000_000,
            targets: vec![StampTarget {
                target: "client/lib".into(),
                class: TestClass::Slow,
                wall_ms: 120_000,
                cpu_ms: Some(400_000),
                passed: true,
            }],
            commands: vec!["golden-replay-release".into()],
            flakes: vec!["client/x_tests".into()],
        }
    }

    #[test]
    fn a_stamp_round_trips_through_its_file() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        let stamp = sample();
        let path = stamp.write(dir.path())?;
        assert!(path.ends_with("abc123.package.ron"));
        assert_eq!(
            Stamp::read(dir.path(), "abc123", Tier::Package)?,
            Some(stamp.clone())
        );
        assert_eq!(Stamp::read(dir.path(), "abc123", Tier::Full)?, None);
        assert_eq!(Stamp::read_all(dir.path())?, vec![stamp]);
        assert_eq!(Stamp::read_all(&dir.path().join("missing"))?, Vec::new());
        Ok(())
    }

    #[test]
    fn a_stamp_without_flakes_reads_as_none() -> Result<(), GateError> {
        let mut stamp = sample();
        stamp.flakes.clear();
        let text = stamp.to_ron()?;
        let old = text.replace("flakes: [],", "");
        assert!(!old.contains("flakes"), "{old}");
        assert_eq!(Stamp::from_ron(&old, Path::new("old.ron"))?, stamp);
        Ok(())
    }

    #[test]
    fn a_malformed_stamp_is_an_error() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        let path = dir.path().join(Stamp::file_name("t", Tier::Full));
        std::fs::write(&path, "Stamp(tier: Full)").map_err(io(&path))?;
        assert!(matches!(
            Stamp::read(dir.path(), "t", Tier::Full),
            Err(GateError::StampSyntax { .. })
        ));
        Ok(())
    }

    #[test]
    fn a_dirty_tree_is_refused_and_a_clean_one_is_named_by_its_tree() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        let root = dir.path();
        let git = |args: &[&str]| capture("git", args, root);
        git(&["init", "--quiet"])?;
        let file = root.join("a.txt");
        std::fs::write(&file, "one").map_err(io(&file))?;
        git(&["add", "a.txt"])?;
        git(&[
            "-c",
            "user.name=gate",
            "-c",
            "user.email=gate@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "one",
        ])?;
        let repo = Git::discover(root)?;
        let clean = StampTree::of_clean_head(&repo)?;
        assert_eq!(clean.tree, repo.tree("HEAD")?);
        std::fs::write(&file, "two").map_err(io(&file))?;
        assert!(matches!(
            StampTree::of_clean_head(&repo),
            Err(GateError::DirtyTree(_))
        ));
        std::fs::write(&file, "one").map_err(io(&file))?;
        std::fs::write(root.join("new.txt"), "x").map_err(io(root))?;
        assert!(
            matches!(
                StampTree::of_clean_head(&repo),
                Err(GateError::DirtyTree(_))
            ),
            "an untracked file is a change too"
        );
        Ok(())
    }
}
