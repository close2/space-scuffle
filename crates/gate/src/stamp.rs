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
//! stamp for the class boundaries of blueprint §4.4. `merge` and `full` find, through
//! [`Stamp::reusable`], a green stamp of the same clean tree that lets them pass without
//! building (Amendment 3, "test a tree once"). Stamps are local evidence, never committed.
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
    /// The peak anonymous memory of its process tree in MiB, as the lane sampler measured it
    /// (plan 2026-11-11-faster-gates §3.3). Stamps written before this field existed read as
    /// `None`.
    #[serde(default)]
    pub peak_anon_mib: Option<u64>,
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
    /// Steps whose first attempt crashed the compiler and that passed on their one retry on a
    /// cleared incremental cache (blueprint Amendment 1, D5). Stamps written before this field
    /// existed read as none.
    #[serde(default)]
    pub retries: Vec<String>,
    /// The file name of the green stamp of the same tree this run reused instead of building and
    /// testing (blueprint Amendment 3, "test a tree once"); `None` for a run that tested. Stamps
    /// written before this field existed read as `None`.
    #[serde(default)]
    pub reused: Option<String>,
    /// The largest `anon` of the user slice during the test phase, in MiB. Older stamps read as
    /// `None`.
    #[serde(default)]
    pub slice_peak_anon_mib: Option<u64>,
    /// The lanes the run used (`--jobs`). Older stamps read as `None`.
    #[serde(default)]
    pub jobs: Option<usize>,
    /// The test phase's wall time in milliseconds (the lane run, without reruns). Older stamps
    /// read as `None`.
    #[serde(default)]
    pub test_wall_ms: Option<u64>,
    /// The flaky targets of [`Stamp::flakes`] whose failed run overlapped other targets on the
    /// lanes. Older stamps read as none.
    #[serde(default)]
    pub overlapped_flakes: Vec<String>,
}

/// How many recent runs of a target on this host its memory estimate looks back over.
pub const ESTIMATE_RUNS: usize = 5;

/// Each target's memory estimate in MiB: the largest `peak_anon_mib` among its
/// [`ESTIMATE_RUNS`] most recent recorded peaks in stamps of `host` (plan §3.3). Targets with
/// no recorded peak on this host are absent.
#[must_use]
pub fn peak_estimates(stamps: &[Stamp], host: &str) -> std::collections::BTreeMap<String, u64> {
    let mut ours: Vec<&Stamp> = stamps.iter().filter(|s| s.host == host).collect();
    ours.sort_by_key(|s| std::cmp::Reverse(s.finished_unix_s));
    let mut seen: std::collections::BTreeMap<String, (usize, u64)> =
        std::collections::BTreeMap::new();
    for stamp in ours {
        for target in &stamp.targets {
            let Some(peak) = target.peak_anon_mib else {
                continue;
            };
            let (runs, largest) = seen.entry(target.target.clone()).or_insert((0, 0));
            if *runs < ESTIMATE_RUNS {
                *runs += 1;
                *largest = (*largest).max(peak);
            }
        }
    }
    seen.into_iter()
        .map(|(target, (_, largest))| (target, largest))
        .collect()
}

/// A green stamp of the tree at `HEAD` that a `merge` or `full` run may stand on (blueprint
/// Amendment 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reuse {
    /// The reused stamp's file.
    pub path: PathBuf,
    /// The reused stamp.
    pub stamp: Stamp,
    /// The clean `HEAD` it covers.
    pub head: StampTree,
}

impl Reuse {
    /// The line the tier prints: which stamp it reuses for which tree.
    #[must_use]
    pub fn message(&self, tier: Tier) -> String {
        format!(
            "cargo gate {tier}: tree {} already has a green {} stamp ({}, commit {}); reusing it, \
             nothing is built or run (test a tree once, Q0055)",
            self.head.tree,
            self.stamp.tier,
            self.path.display(),
            self.stamp.commit
        )
    }
}

/// The tiers whose green stamp `tier` accepts for the same tree, in order of preference:
/// `full` takes `full` or `package`; `merge` takes `merge`, `full` or `package`; `quick` and
/// `package` take none, since an agent's `package` run is the test of record.
#[must_use]
pub fn accepted_tiers(tier: Tier) -> &'static [Tier] {
    match tier {
        Tier::Full => &[Tier::Full, Tier::Package],
        Tier::Merge => &[Tier::Merge, Tier::Full, Tier::Package],
        Tier::Quick | Tier::Package => &[],
    }
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
    /// Whether every target the stamp records passed.
    #[must_use]
    pub fn is_green(&self) -> bool {
        self.targets.iter().all(|target| target.passed)
    }

    /// The green stamp in `dir` that `tier` may reuse for the clean `HEAD` of `repo`, if any.
    ///
    /// A dirty working tree has no reusable stamp, because its tree hash does not name what
    /// would be tested. A stamp of another tree or with a failed target is never reused.
    ///
    /// # Errors
    /// Repository errors other than a dirty tree; [`Stamp::read`] errors for a candidate file.
    pub fn reusable(repo: &Git, dir: &Path, tier: Tier) -> Result<Option<Reuse>, GateError> {
        let accepted = accepted_tiers(tier);
        if accepted.is_empty() {
            return Ok(None);
        }
        let head = match StampTree::of_clean_head(repo) {
            Ok(head) => head,
            Err(GateError::DirtyTree(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        for candidate in accepted {
            let Some(stamp) = Self::read(dir, &head.tree, *candidate)? else {
                continue;
            };
            if stamp.tree == head.tree && stamp.tier == *candidate && stamp.is_green() {
                return Ok(Some(Reuse {
                    path: dir.join(Self::file_name(&head.tree, *candidate)),
                    stamp,
                    head,
                }));
            }
        }
        Ok(None)
    }

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
                peak_anon_mib: None,
            }],
            commands: vec!["golden-replay-release".into()],
            flakes: vec!["client/x_tests".into()],
            retries: vec!["clippy".into()],
            reused: None,
            slice_peak_anon_mib: None,
            jobs: None,
            test_wall_ms: None,
            overlapped_flakes: Vec::new(),
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
    fn a_stamp_without_flakes_or_retries_reads_as_none() -> Result<(), GateError> {
        let mut stamp = sample();
        stamp.flakes.clear();
        stamp.retries.clear();
        let text = stamp.to_ron()?;
        let old = text
            .replace("overlapped_flakes: [],", "")
            .replace("flakes: [],", "")
            .replace("retries: [],", "");
        assert!(!old.contains("flakes"), "{old}");
        assert!(!old.contains("retries"), "{old}");
        assert_eq!(Stamp::from_ron(&old, Path::new("old.ron"))?, stamp);
        Ok(())
    }

    #[test]
    fn a_stamp_without_peaks_still_parses() -> Result<(), GateError> {
        let stamp = sample();
        let text = stamp.to_ron()?;
        let old = text
            .replace("slice_peak_anon_mib: None,", "")
            .replace("peak_anon_mib: None,", "")
            .replace("jobs: None,", "")
            .replace("test_wall_ms: None,", "")
            .replace("overlapped_flakes: [],", "");
        assert!(!old.contains("peak"), "{old}");
        assert_eq!(Stamp::from_ron(&old, Path::new("old.ron"))?, stamp);
        Ok(())
    }

    #[test]
    fn the_estimate_is_the_largest_peak_of_the_last_five_runs_of_this_host() {
        let run = |host: &str, finished: u64, peak: Option<u64>| {
            let mut stamp = sample();
            stamp.host = host.to_string();
            stamp.finished_unix_s = finished;
            for target in &mut stamp.targets {
                target.peak_anon_mib = peak;
            }
            stamp
        };
        let name = sample().targets[0].target.clone();
        // Oldest first: the 9 GiB peak is the sixth most recent, so it has aged out.
        let mut stamps = vec![run("here", 1, Some(9_000))];
        for (finished, peak) in [(2, 3_000), (3, 4_000), (4, 3_500), (5, 2_000), (6, 1_000)] {
            stamps.push(run("here", finished, Some(peak)));
        }
        stamps.push(run("here", 7, None));
        stamps.push(run("elsewhere", 8, Some(20_000)));
        let estimates = peak_estimates(&stamps, "here");
        assert_eq!(estimates.get(&name), Some(&4_000));
        assert_eq!(
            peak_estimates(&stamps, "nowhere").get(&name),
            None,
            "another host's stamps are ignored"
        );
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

    /// A repository in a temporary directory with one commit of `a.txt`.
    fn repository() -> Result<(tempfile::TempDir, Git), GateError> {
        let dir = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        let root = dir.path();
        let run = |args: &[&str]| capture("git", args, root);
        run(&["init", "--quiet"])?;
        let file = root.join("a.txt");
        std::fs::write(&file, "one").map_err(io(&file))?;
        run(&["add", "a.txt"])?;
        run(&[
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
        Ok((dir, repo))
    }

    /// A green stamp of `tier` for `tree`.
    fn green(tier: Tier, tree: &str) -> Stamp {
        Stamp {
            tier,
            tree: tree.to_string(),
            ..sample()
        }
    }

    #[test]
    fn a_green_stamp_of_the_same_tree_is_reused_by_merge_and_full() -> Result<(), GateError> {
        let (_repo_dir, repo) = repository()?;
        let stamps = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        let tree = repo.tree("HEAD")?;
        let path = green(Tier::Package, &tree).write(stamps.path())?;
        for tier in [Tier::Full, Tier::Merge] {
            let reuse = Stamp::reusable(&repo, stamps.path(), tier)?;
            assert!(
                reuse.is_some(),
                "{tier} must reuse the package stamp of its tree"
            );
            let Some(reuse) = reuse else { continue };
            assert_eq!(reuse.path, path);
            assert_eq!(reuse.stamp.tier, Tier::Package);
            assert_eq!(reuse.head.tree, tree);
            let message = reuse.message(tier);
            assert!(
                message.contains(&path.display().to_string()) && message.contains(&tree),
                "the reuse message must name the stamp and the tree: {message}"
            );
        }
        let merge = green(Tier::Merge, &tree).write(stamps.path())?;
        let reuse = Stamp::reusable(&repo, stamps.path(), Tier::Merge)?;
        assert_eq!(reuse.map(|reuse| reuse.path), Some(merge));
        Ok(())
    }

    #[test]
    fn merge_reuses_a_full_stamp_but_full_does_not_reuse_a_merge_stamp() -> Result<(), GateError> {
        let (_repo_dir, repo) = repository()?;
        let tree = repo.tree("HEAD")?;
        let stamps = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        green(Tier::Merge, &tree).write(stamps.path())?;
        assert_eq!(Stamp::reusable(&repo, stamps.path(), Tier::Full)?, None);
        green(Tier::Full, &tree).write(stamps.path())?;
        let full = Stamp::reusable(&repo, stamps.path(), Tier::Full)?;
        assert_eq!(full.map(|reuse| reuse.stamp.tier), Some(Tier::Full));
        for tier in [Tier::Quick, Tier::Package] {
            assert_eq!(Stamp::reusable(&repo, stamps.path(), tier)?, None, "{tier}");
        }
        Ok(())
    }

    #[test]
    fn another_tree_a_red_stamp_or_a_dirty_tree_is_not_reused() -> Result<(), GateError> {
        let (repo_dir, repo) = repository()?;
        let tree = repo.tree("HEAD")?;
        let stamps = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        green(Tier::Package, "0123456789abcdef").write(stamps.path())?;
        assert_eq!(
            Stamp::reusable(&repo, stamps.path(), Tier::Full)?,
            None,
            "a stamp of another tree"
        );
        let mut red = green(Tier::Package, &tree);
        red.targets[0].passed = false;
        red.write(stamps.path())?;
        assert_eq!(
            Stamp::reusable(&repo, stamps.path(), Tier::Merge)?,
            None,
            "a red stamp"
        );
        green(Tier::Package, &tree).write(stamps.path())?;
        assert!(Stamp::reusable(&repo, stamps.path(), Tier::Merge)?.is_some());
        let file = repo_dir.path().join("a.txt");
        std::fs::write(&file, "changed").map_err(io(&file))?;
        assert_eq!(
            Stamp::reusable(&repo, stamps.path(), Tier::Merge)?,
            None,
            "a dirty working tree"
        );
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
