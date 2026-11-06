//! Runs a child in its own process group and leaves no process of that group behind.
//!
//! # Responsibility
//! `showcase render` runs each take as `with-gpu-lock xvfb-run ... showcase take ...` on the shared
//! physical GPU. `with-gpu-lock` holds `/run/lock/gpu.lock` through a descriptor every descendant
//! inherits, so a descendant that outlives the take (the X server of `xvfb-run`, an `ffmpeg`
//! encoder, a helper) would keep the GPU locked for the other user (charter §7).
//! [`run_in_own_group`] therefore starts the child as the leader of a new process group, waits for
//! it, then kills the whole group and waits until no live member is left.
//!
//! # How it fits
//! Used by [`render`](super::render) for every take child, on both drivers: on `lavapipe` no lock
//! is involved, but a stray X server or encoder is still a leak.
//!
//! # Invariants
//! - The group is killed after every outcome: success, failure, or a failure to wait.
//! - A run that leaves a live member of its group after the kill is an error naming the survivors,
//!   never a silent pass. Zombies hold no descriptors and are not counted.
//! - No `unsafe`: the group is signalled with `kill(1)`.

use super::ShowcaseError;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// How long the members of a killed group may take to disappear.
pub const GROUP_EXIT_PATIENCE: Duration = Duration::from_secs(10);

/// What a child run in its own process group ended with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupRun {
    /// The child's exit status.
    pub status: ExitStatus,
    /// The process group the child led (its process ID).
    pub group: u32,
}

/// Runs `command` as the leader of a new process group, waits for it, then kills every process
/// left in that group and waits until none is alive.
///
/// # Errors
/// [`ShowcaseError::Render`] if the child cannot start or be waited for, if `kill` cannot run,
/// or if a member of the group is still alive [`GROUP_EXIT_PATIENCE`] after the kill.
pub fn run_in_own_group(command: &mut Command, name: &str) -> Result<GroupRun, ShowcaseError> {
    let mut child = command
        .process_group(0)
        .spawn()
        .map_err(|error| ShowcaseError::Render(format!("cannot start {name}: {error}")))?;
    let group = child.id();
    let waited = child.wait();
    kill_group(group, name)?;
    let status = waited
        .map_err(|error| ShowcaseError::Render(format!("cannot wait for {name}: {error}")))?;
    Ok(GroupRun { status, group })
}

/// Sends `SIGKILL` to every process of `group`, then waits until no live member is left.
fn kill_group(group: u32, name: &str) -> Result<(), ShowcaseError> {
    if !live_members_of_group(group)?.is_empty() {
        // `kill` fails when the group emptied between the scan and the signal; the scan below
        // decides the outcome, so its exit status carries no information.
        Command::new("kill")
            .args(["-KILL", "--", &format!("-{group}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| ShowcaseError::Render(format!("cannot run kill: {error}")))?;
    }
    let deadline = Instant::now() + GROUP_EXIT_PATIENCE;
    loop {
        let survivors = live_members_of_group(group)?;
        if survivors.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ShowcaseError::Render(format!(
                "processes {survivors:?} of {name}'s process group {group} outlived it"
            )));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Live (not zombie) processes whose process group is `group`, read from `/proc`.
///
/// # Errors
/// [`ShowcaseError::Render`] if `/proc` cannot be listed: the group could not be observed.
pub fn live_members_of_group(group: u32) -> Result<Vec<u32>, ShowcaseError> {
    let entries = std::fs::read_dir("/proc").map_err(|error| {
        ShowcaseError::Render(format!(
            "cannot list /proc to observe process groups: {error}"
        ))
    })?;
    let group = group.to_string();
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            // `stat` reads `pid (comm) state ppid pgrp ...`; `comm` may hold spaces and
            // parentheses, so the fields are counted after its last closing parenthesis.
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return false;
            };
            let Some((_, after_comm)) = stat.rsplit_once(')') else {
                return false;
            };
            let fields: Vec<&str> = after_comm.split_whitespace().collect();
            fields.first() != Some(&"Z") && fields.get(2) == Some(&group.as_str())
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shell that leaves two background sleepers behind and exits with `code`.
    fn leaky_shell(code: u8) -> Command {
        let mut command = Command::new("sh");
        command
            .args(["-c", &format!("sleep 60 & sleep 60 & exit {code}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    #[test]
    fn a_take_leaves_no_process_of_its_group_after_success() {
        let run = run_in_own_group(&mut leaky_shell(0), "test child");
        let run = run.map_err(|error| error.to_string());
        assert!(matches!(&run, Ok(run) if run.status.success()), "{run:?}");
        if let Ok(run) = run {
            assert_eq!(
                live_members_of_group(run.group).map_err(|error| error.to_string()),
                Ok(Vec::new())
            );
        }
    }

    #[test]
    fn a_take_leaves_no_process_of_its_group_after_failure() {
        let run = run_in_own_group(&mut leaky_shell(3), "test child");
        let run = run.map_err(|error| error.to_string());
        assert!(
            matches!(&run, Ok(run) if run.status.code() == Some(3)),
            "{run:?}"
        );
        if let Ok(run) = run {
            assert_eq!(
                live_members_of_group(run.group).map_err(|error| error.to_string()),
                Ok(Vec::new())
            );
        }
    }

    #[test]
    fn the_child_leads_its_own_group() {
        let run = run_in_own_group(
            Command::new("sh")
                .args(["-c", "test \"$(ps -o pgid= -p $$ | tr -d ' ')\" = \"$$\""])
                .stdout(Stdio::null()),
            "test child",
        );
        let run = run.map_err(|error| error.to_string());
        assert!(matches!(&run, Ok(run) if run.status.success()), "{run:?}");
    }

    #[test]
    fn a_child_that_cannot_start_is_an_error() {
        let run = run_in_own_group(
            &mut Command::new("/nonexistent/showcase-test-program"),
            "test child",
        );
        assert!(
            matches!(&run, Err(ShowcaseError::Render(reason)) if reason.contains("cannot start")),
            "{run:?}"
        );
    }
}
