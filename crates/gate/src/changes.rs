//! Changed paths, tree hashes and history, read from Git.
//!
//! # Responsibility
//! Wraps the few Git questions the gate asks: the repository root, the merge base with `main`,
//! the paths a change touches (committed, uncommitted and untracked), whether the working tree is
//! clean, the tree hash that names what was tested, and the commits of a branch.
//!
//! # How it fits
//! The selection consumes the changed paths as plain strings; stamps are keyed by the tree hash;
//! the `merge` tier walks a branch's commits. Every call is a `git` process in the repository
//! root; nothing is cached, so the answers always describe the current checkout.
//!
//! # Invariants
//! - Paths are relative to the repository root with `/` separators, as Git prints them.
//! - Renames count as a deletion and an addition (`--no-renames`), so both paths are judged.
//! - A failing Git command is an error, never an empty change set.

use crate::GateError;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Cargo executable: `$CARGO` when running under Cargo, else `cargo` from `PATH`.
#[must_use]
pub fn cargo_program() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// Runs `program args` in `dir` and returns its standard output.
///
/// # Errors
/// [`GateError::Spawn`] if it cannot start; [`GateError::CommandFailed`] if it exits unsuccessfully.
pub fn capture(program: &str, args: &[&str], dir: &Path) -> Result<String, GateError> {
    let output = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|source| GateError::Spawn {
            program: program.to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(GateError::CommandFailed {
            command: format!("{program} {}", args.join(" ")),
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Which changes to judge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeSpec {
    /// Everything that differs from this revision: commits, uncommitted edits, untracked files.
    Since(String),
    /// Only what differs between two revisions (`a..b`), as for a past merge.
    Between(String, String),
}

impl ChangeSpec {
    /// Parses `a..b` as [`ChangeSpec::Between`] and anything else as [`ChangeSpec::Since`].
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.split_once("..") {
            Some((from, to)) if !from.is_empty() && !to.is_empty() => {
                Self::Between(from.to_string(), to.to_string())
            }
            _ => Self::Since(text.to_string()),
        }
    }
}

/// A Git repository, addressed by its root directory.
#[derive(Clone, Debug)]
pub struct Git {
    /// The repository's top-level directory.
    root: PathBuf,
}

impl Git {
    /// The repository containing `dir`.
    ///
    /// # Errors
    /// As [`capture`], when `dir` is not inside a Git working tree.
    pub fn discover(dir: &Path) -> Result<Self, GateError> {
        let top = capture("git", &["rev-parse", "--show-toplevel"], dir)?;
        Ok(Self {
            root: PathBuf::from(top.trim()),
        })
    }

    /// The repository's root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Runs `git args` in the root and returns its output.
    fn git(&self, args: &[&str]) -> Result<String, GateError> {
        capture("git", args, &self.root)
    }

    /// Non-empty output lines, as a sorted set.
    fn lines(text: &str) -> BTreeSet<String> {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// The full commit hash `rev` names.
    ///
    /// # Errors
    /// As [`capture`], when `rev` names no commit.
    pub fn commit(&self, rev: &str) -> Result<String, GateError> {
        let spec = format!("{rev}^{{commit}}");
        Ok(self
            .git(&["rev-parse", "--verify", &spec])?
            .trim()
            .to_string())
    }

    /// The tree hash of commit `rev`.
    ///
    /// # Errors
    /// As [`capture`], when `rev` names no commit.
    pub fn tree(&self, rev: &str) -> Result<String, GateError> {
        let spec = format!("{rev}^{{tree}}");
        Ok(self
            .git(&["rev-parse", "--verify", &spec])?
            .trim()
            .to_string())
    }

    /// The best common ancestor of `a` and `b`.
    ///
    /// # Errors
    /// As [`capture`], when either names no commit or they share no history.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<String, GateError> {
        Ok(self.git(&["merge-base", a, b])?.trim().to_string())
    }

    /// Whether the working tree has no staged, unstaged or untracked (unignored) change.
    ///
    /// # Errors
    /// As [`capture`].
    pub fn is_clean(&self) -> Result<bool, GateError> {
        Ok(self
            .git(&["status", "--porcelain", "--untracked-files=normal"])?
            .trim()
            .is_empty())
    }

    /// The paths `spec` covers, sorted and without duplicates.
    ///
    /// # Errors
    /// As [`capture`], when a revision is unknown.
    pub fn changed_paths(&self, spec: &ChangeSpec) -> Result<BTreeSet<String>, GateError> {
        match spec {
            ChangeSpec::Since(base) => {
                let mut paths =
                    Self::lines(&self.git(&["diff", "--name-only", "--no-renames", base])?);
                paths.extend(Self::lines(&self.git(&[
                    "ls-files",
                    "--others",
                    "--exclude-standard",
                ])?));
                Ok(paths)
            }
            ChangeSpec::Between(from, to) => Ok(Self::lines(&self.git(&[
                "diff",
                "--name-only",
                "--no-renames",
                from,
                to,
            ])?)),
        }
    }

    /// Every tracked path, sorted.
    ///
    /// # Errors
    /// As [`capture`].
    pub fn tracked_files(&self) -> Result<BTreeSet<String>, GateError> {
        Ok(Self::lines(&self.git(&["ls-files"])?))
    }

    /// The commits reachable from `head` but not from `exclude`, newest first.
    ///
    /// # Errors
    /// As [`capture`], when a revision is unknown.
    pub fn commits_between(&self, exclude: &str, head: &str) -> Result<Vec<String>, GateError> {
        let range = format!("{exclude}..{head}");
        Ok(self
            .git(&["rev-list", &range])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// The first-parent history of `head` back to `exclude` (or the `limit` newest commits when
    /// `exclude` is `None`), newest first, as `(hash, subject)`. On `main` these are its merges
    /// and direct commits: the changes the red rule (blueprint §3.5) can suspect.
    ///
    /// # Errors
    /// As [`capture`], when a revision is unknown.
    pub fn first_parent_history(
        &self,
        exclude: Option<&str>,
        head: &str,
        limit: usize,
    ) -> Result<Vec<(String, String)>, GateError> {
        let range = exclude.map_or_else(|| head.to_string(), |base| format!("{base}..{head}"));
        let count = format!("--max-count={limit}");
        let text = self.git(&["log", "--first-parent", &count, "--format=%H%x09%s", &range])?;
        Ok(text
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(hash, subject)| (hash.to_string(), subject.to_string()))
            .collect())
    }

    /// Whether commit `ancestor` is reachable from `descendant` (a commit is its own ancestor).
    ///
    /// # Errors
    /// As [`capture`], for an unknown revision; a commit that is simply not an ancestor is
    /// `Ok(false)`.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, GateError> {
        match self.git(&["merge-base", "--is-ancestor", ancestor, descendant]) {
            Ok(_) => Ok(true),
            Err(GateError::CommandFailed { status, .. }) if status.code() == Some(1) => Ok(false),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_is_between_and_anything_else_is_since() {
        assert_eq!(
            ChangeSpec::parse("abc^1..abc"),
            ChangeSpec::Between("abc^1".to_string(), "abc".to_string())
        );
        assert_eq!(
            ChangeSpec::parse("main"),
            ChangeSpec::Since("main".to_string())
        );
        assert_eq!(
            ChangeSpec::parse("..x"),
            ChangeSpec::Since("..x".to_string())
        );
    }

    #[test]
    fn first_parent_history_and_ancestry_follow_the_commits() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(|source| GateError::Io {
            path: "tempdir".into(),
            source,
        })?;
        let root = dir.path();
        let git = |args: &[&str]| capture("git", args, root);
        git(&["init", "--quiet"])?;
        for subject in ["one", "two", "three"] {
            git(&[
                "-c",
                "user.name=gate",
                "-c",
                "user.email=gate@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                subject,
            ])?;
        }
        let repo = Git::discover(root)?;
        let first = repo.commit("HEAD~2")?;
        let subjects = |history: Vec<(String, String)>| -> Vec<String> {
            history.into_iter().map(|(_, subject)| subject).collect()
        };
        assert_eq!(
            subjects(repo.first_parent_history(None, "HEAD", 2)?),
            ["three", "two"]
        );
        assert_eq!(
            subjects(repo.first_parent_history(Some(&first), "HEAD", 10)?),
            ["three", "two"]
        );
        assert!(repo.is_ancestor(&first, "HEAD")?);
        assert!(!repo.is_ancestor("HEAD", &first)?);
        assert!(repo.is_ancestor("no-such-ref", "HEAD").is_err());
        Ok(())
    }

    #[test]
    fn a_failing_command_is_an_error_not_empty_output() {
        let result = capture(
            "git",
            &["rev-parse", "--verify", "no-such-ref^{commit}"],
            Path::new("/"),
        );
        assert!(result.is_err());
    }
}
