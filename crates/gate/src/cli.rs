//! The command line of `cargo gate`.
//!
//! # Responsibility
//! Parses the arguments into a [`Command`] (blueprint §5.2), with typed options and a usage error
//! for anything it does not understand.
//!
//! # How it fits
//! `src/main.rs` parses with [`parse_args`] and hands the command to the runner. Parsing is a pure
//! function so every form is unit-tested; nothing here touches Git or Cargo.
//!
//! # Invariants
//! - An unknown subcommand, option or extra argument is an error, never ignored.
//! - Lanes come from `--jobs`, else [`JOBS_VAR`], else 1, the sequential run of `cargo test`
//!   (blueprint §2.6; plan 2026-11-11-faster-gates §3.2). The environment is read once, by
//!   [`parse_args`]; [`parse_args_with`] takes it as a value, so parsing stays testable.

use crate::tier::Tier;
use crate::GateError;
use std::path::PathBuf;

/// The usage text printed with every command-line error.
pub const USAGE: &str = "\
usage:
  cargo gate explain [<base> | <from>..<to>] [--tier quick|package|full]
                                  print the selection and the rule behind each target; run nothing
  cargo gate quick                optional inner loop: fmt, clippy on changed crates, their fast tests
  cargo gate package              required once before hand-back; writes a stamp
  cargo gate merge <branch>       orchestrator, on main after `git merge --no-ff <branch>`
  cargo gate full                 the complete suite and the golden replays; writes a stamp
  cargo gate timings [--from-log <file>]
                                  targets whose recorded time contradicts their class and
                                  repeated flakes; with --from-log, the Markdown report of one
                                  `cargo test` log (CI's job summary)
  cargo gate suspects [<target>...] [--from-log <file>] [--base <rev>]
                                  the red rule's suspect list for failing targets, named or read
                                  from a `cargo test` log (a red CI run)
  cargo gate stop                 ask the gate holding the gate lock to kill its running tests,
                                  skip the rest and exit red without a stamp; never waits

options:
  --base <rev>        compare with this revision (default: `git merge-base HEAD <main>`)
  --main <ref>        the main branch (default: main)
  --jobs <n>          concurrent test executables: one GPU lane and n-1 general lanes (default
                      $SPACE_SCUFFLE_GATE_JOBS, else 1)
  --memory-budget-gib <x>   most the running targets' recorded peaks may sum to (default 40)
  --memory-ceiling-gib <x>  most the slice's live anon plus a starting target's peak may reach
                      (default: the cgroup limit minus 10 GiB)
  --no-reuse          test even when a green stamp of this tree exists
  --log-dir <dir>     logs (default ~/scratch/logs/gate/<tree-hash>/)
  --stamp-dir <dir>   stamps (default ~/scratch/gate-stamps/)
  --no-run            select and build only
  --from-log <file>   timings and suspects only: read a `cargo test` log";

/// The environment variable that sets the lanes when `--jobs` is absent.
pub const JOBS_VAR: &str = "SPACE_SCUFFLE_GATE_JOBS";

/// The lanes when neither `--jobs` nor [`JOBS_VAR`] sets them.
pub const DEFAULT_JOBS: usize = 1;

/// The memory budget when `--memory-budget-gib` is absent, in MiB (plan §3.2).
pub const DEFAULT_MEMORY_BUDGET_MIB: u64 = 40 * 1024;

/// What the live ceiling stays below the cgroup limit when `--memory-ceiling-gib` is absent, in
/// MiB (plan §3.2).
pub const CEILING_MARGIN_MIB: u64 = 10 * 1024;

/// Options shared by the commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOptions {
    /// The revision to compare with, if given.
    pub base: Option<String>,
    /// The main branch.
    pub main_ref: String,
    /// Concurrent test executables.
    pub jobs: usize,
    /// Where logs go, if not the default.
    pub log_dir: Option<PathBuf>,
    /// Where stamps go, if not the default.
    pub stamp_dir: Option<PathBuf>,
    /// Build without running.
    pub no_run: bool,
    /// The memory budget of admission in MiB, if not the default.
    pub memory_budget_mib: Option<u64>,
    /// The live memory ceiling of admission in MiB, if not the cgroup limit minus the margin.
    pub memory_ceiling_mib: Option<u64>,
    /// Test even when a green stamp of the same tree could be reused.
    pub no_reuse: bool,
    /// The stop request file the gate polls; set by `main.rs` once it holds the gate lock.
    pub stop_file: Option<PathBuf>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            base: None,
            main_ref: "main".to_string(),
            jobs: DEFAULT_JOBS,
            log_dir: None,
            stamp_dir: None,
            no_run: false,
            memory_budget_mib: None,
            memory_ceiling_mib: None,
            no_reuse: false,
            stop_file: None,
        }
    }
}

/// One invocation of `cargo gate`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Print the selection of `tier` for a change; run nothing.
    Explain {
        /// `<base>` or `<from>..<to>`; `None` means the merge base with main.
        spec: Option<String>,
        /// The tier to explain.
        tier: Tier,
        /// Shared options.
        options: RunOptions,
    },
    /// Run `quick`, `package` or `full`.
    Run {
        /// The tier.
        tier: Tier,
        /// Shared options.
        options: RunOptions,
    },
    /// Run the `merge` tier for a merged branch.
    Merge {
        /// The branch that was merged.
        branch: String,
        /// Shared options.
        options: RunOptions,
    },
    /// Print the class and flake report from the stamps, or the report of one log.
    Timings {
        /// A `cargo test` log to report on instead of the stamps.
        from_log: Option<PathBuf>,
        /// Shared options.
        options: RunOptions,
    },
    /// Print the red rule's suspect list (blueprint §3.5).
    Suspects {
        /// Failing targets named on the command line (`crate/test_name`, `crate/lib`).
        targets: Vec<String>,
        /// A `cargo test` log whose failing targets are added.
        from_log: Option<PathBuf>,
        /// Shared options; `--base` starts the search instead of the last green full stamp.
        options: RunOptions,
    },
    /// Ask the gate that holds the gate lock to stop (plan §3.4).
    Stop,
}

/// A positive number of GiB as MiB.
fn gib_option(name: &str, text: &str) -> Result<u64, GateError> {
    text.parse::<f64>()
        .ok()
        .filter(|gib| gib.is_finite() && *gib > 0.0 && *gib < 1.0e6)
        .map(|gib| {
            // The filter bounds the value, so the rounded MiB fit a u64.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let mib = (gib * 1024.0).round() as u64;
            mib.max(1)
        })
        .ok_or_else(|| GateError::Usage(format!("{name} needs a positive number, not `{text}`")))
}

/// The lanes [`JOBS_VAR`] asks for, if it is set to a positive number.
fn jobs_from_env(value: Option<&str>) -> Result<Option<usize>, GateError> {
    value
        .map(|text| {
            text.trim()
                .parse()
                .ok()
                .filter(|jobs| *jobs > 0)
                .ok_or_else(|| {
                    GateError::Usage(format!(
                        "{JOBS_VAR} must be a positive number, not `{text}`"
                    ))
                })
        })
        .transpose()
}

/// The options and positional arguments after the command word.
struct Flags {
    options: RunOptions,
    jobs_given: bool,
    tier: Tier,
    from_log: Option<PathBuf>,
    positional: Vec<String>,
}

/// Parses the options and positional arguments after the command word.
fn parse_flags(rest: &[String]) -> Result<Flags, GateError> {
    let usage = |message: String| GateError::Usage(message);
    let mut options = RunOptions::default();
    let mut jobs_given = false;
    let mut tier = Tier::Package;
    let mut from_log = None;
    let mut positional = Vec::new();
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| usage(format!("{name} needs a value")))
        };
        match arg.as_str() {
            "--base" => options.base = Some(value("--base")?),
            "--main" => options.main_ref = value("--main")?,
            "--jobs" => {
                let text = value("--jobs")?;
                options.jobs = text.parse().ok().filter(|jobs| *jobs > 0).ok_or_else(|| {
                    usage(format!("--jobs needs a positive number, not `{text}`"))
                })?;
                jobs_given = true;
            }
            "--memory-budget-gib" => {
                let text = value("--memory-budget-gib")?;
                options.memory_budget_mib = Some(gib_option("--memory-budget-gib", &text)?);
            }
            "--memory-ceiling-gib" => {
                let text = value("--memory-ceiling-gib")?;
                options.memory_ceiling_mib = Some(gib_option("--memory-ceiling-gib", &text)?);
            }
            "--no-reuse" => options.no_reuse = true,
            "--log-dir" => options.log_dir = Some(PathBuf::from(value("--log-dir")?)),
            "--stamp-dir" => options.stamp_dir = Some(PathBuf::from(value("--stamp-dir")?)),
            "--no-run" => options.no_run = true,
            "--from-log" => from_log = Some(PathBuf::from(value("--from-log")?)),
            "--tier" => {
                let text = value("--tier")?;
                tier = Tier::parse(&text)
                    .filter(|t| *t != Tier::Merge)
                    .ok_or_else(|| {
                        usage(format!("--tier: `{text}` is not quick, package or full"))
                    })?;
            }
            other if other.starts_with("--") => {
                return Err(usage(format!("unknown option {other}")))
            }
            other => positional.push(other.to_string()),
        }
    }
    Ok(Flags {
        options,
        jobs_given,
        tier,
        from_log,
        positional,
    })
}

/// Parses `args` (without the program name), reading [`JOBS_VAR`] from the environment.
///
/// # Errors
/// [`GateError::Usage`] for anything that is not one of the forms in [`USAGE`].
pub fn parse_args(args: &[String]) -> Result<Command, GateError> {
    parse_args_with(args, std::env::var(JOBS_VAR).ok().as_deref())
}

/// Parses `args` (without the program name) with `jobs_var` as the value of [`JOBS_VAR`].
///
/// # Errors
/// As [`parse_args`].
pub fn parse_args_with(args: &[String], jobs_var: Option<&str>) -> Result<Command, GateError> {
    let usage = |message: String| GateError::Usage(message);
    let (word, rest) = args
        .split_first()
        .ok_or_else(|| usage("no command given".to_string()))?;
    let Flags {
        mut options,
        jobs_given,
        tier,
        from_log,
        positional,
    } = parse_flags(rest)?;
    let extra = |count: usize| {
        if positional.len() > count {
            Err(usage(format!(
                "unexpected argument `{}`",
                positional[count]
            )))
        } else {
            Ok(())
        }
    };
    if !jobs_given {
        if let Some(jobs) = jobs_from_env(jobs_var)? {
            options.jobs = jobs;
        }
    }
    if from_log.is_some() && !matches!(word.as_str(), "timings" | "suspects") {
        return Err(usage(format!(
            "--from-log belongs to timings and suspects, not {word}"
        )));
    }
    match word.as_str() {
        "explain" => {
            extra(1)?;
            Ok(Command::Explain {
                spec: positional.first().cloned().or(options.base.clone()),
                tier,
                options,
            })
        }
        "merge" => {
            extra(1)?;
            let branch = positional
                .first()
                .cloned()
                .ok_or_else(|| usage("merge needs the merged branch".to_string()))?;
            Ok(Command::Merge { branch, options })
        }
        "stop" => {
            extra(0)?;
            if rest.is_empty() {
                Ok(Command::Stop)
            } else {
                Err(usage("stop takes no options".to_string()))
            }
        }
        "timings" => {
            extra(0)?;
            Ok(Command::Timings { from_log, options })
        }
        "suspects" => {
            if positional.is_empty() && from_log.is_none() {
                return Err(usage(
                    "suspects needs failing targets or --from-log <file>".to_string(),
                ));
            }
            Ok(Command::Suspects {
                targets: positional,
                from_log,
                options,
            })
        }
        other => {
            extra(0)?;
            let tier = Tier::parse(other)
                .filter(|t| *t != Tier::Merge)
                .ok_or_else(|| usage(format!("unknown command `{other}`")))?;
            Ok(Command::Run { tier, options })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Command, GateError> {
        let args: Vec<String> = line.split_whitespace().map(ToString::to_string).collect();
        parse_args_with(&args, None)
    }

    fn options_of(command: Result<Command, GateError>) -> Option<RunOptions> {
        match command {
            Ok(Command::Run { options, .. }) => Some(options),
            _ => None,
        }
    }

    #[test]
    fn jobs_come_from_the_flag_then_the_variable_then_the_default() {
        let args = |line: &str| -> Vec<String> {
            line.split_whitespace().map(ToString::to_string).collect()
        };
        let jobs = |line: &str, var: Option<&str>| {
            options_of(parse_args_with(&args(line), var)).map(|o| o.jobs)
        };
        assert_eq!(jobs("full --jobs 3", Some("1")), Some(3));
        assert_eq!(jobs("full", Some("2")), Some(2));
        assert_eq!(jobs("full", None), Some(DEFAULT_JOBS));
        assert!(parse_args_with(&args("full"), Some("zero")).is_err());
        assert!(parse_args_with(&args("full"), Some("0")).is_err());
    }

    #[test]
    fn memory_options_and_no_reuse_parse() {
        let options = options_of(parse(
            "full --memory-budget-gib 40 --memory-ceiling-gib 55.5 --no-reuse",
        ));
        let expected = RunOptions {
            memory_budget_mib: Some(40 * 1024),
            memory_ceiling_mib: Some(56_832),
            no_reuse: true,
            ..RunOptions::default()
        };
        assert_eq!(options, Some(expected));
        for bad in ["0", "-3", "x", "inf"] {
            assert!(parse(&format!("full --memory-budget-gib {bad}")).is_err());
            assert!(parse(&format!("full --memory-ceiling-gib {bad}")).is_err());
        }
    }

    #[test]
    fn stop_parses_with_no_options() -> Result<(), GateError> {
        assert_eq!(parse("stop")?, Command::Stop);
        assert!(parse("stop --jobs 2").is_err());
        assert!(parse("stop now").is_err());
        Ok(())
    }

    #[test]
    fn tiers_parse_with_their_options() -> Result<(), GateError> {
        assert_eq!(
            parse("package --jobs 4 --base origin/main --no-run")?,
            Command::Run {
                tier: Tier::Package,
                options: RunOptions {
                    base: Some("origin/main".into()),
                    jobs: 4,
                    no_run: true,
                    ..RunOptions::default()
                }
            }
        );
        assert!(matches!(
            parse("quick")?,
            Command::Run {
                tier: Tier::Quick,
                ..
            }
        ));
        assert!(matches!(
            parse("full")?,
            Command::Run {
                tier: Tier::Full,
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn explain_takes_a_base_or_a_range_and_a_tier() -> Result<(), GateError> {
        assert_eq!(
            parse("explain abc^1..abc --tier quick")?,
            Command::Explain {
                spec: Some("abc^1..abc".into()),
                tier: Tier::Quick,
                options: RunOptions::default(),
            }
        );
        assert!(matches!(
            parse("explain")?,
            Command::Explain {
                spec: None,
                tier: Tier::Package,
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn merge_needs_its_branch() -> Result<(), GateError> {
        assert!(
            matches!(parse("merge feature")?, Command::Merge { branch, .. } if branch == "feature")
        );
        assert!(parse("merge").is_err());
        Ok(())
    }

    #[test]
    fn timings_and_suspects_read_logs() -> Result<(), GateError> {
        assert_eq!(
            parse("timings --from-log ci.log")?,
            Command::Timings {
                from_log: Some(PathBuf::from("ci.log")),
                options: RunOptions::default(),
            }
        );
        assert_eq!(
            parse("suspects client/lib sim_core/a_tests --base abc")?,
            Command::Suspects {
                targets: vec!["client/lib".into(), "sim_core/a_tests".into()],
                from_log: None,
                options: RunOptions {
                    base: Some("abc".into()),
                    ..RunOptions::default()
                },
            }
        );
        assert!(matches!(
            parse("suspects --from-log ci.log")?,
            Command::Suspects { targets, from_log: Some(_), .. } if targets.is_empty()
        ));
        Ok(())
    }

    #[test]
    fn mistakes_are_usage_errors() {
        for line in [
            "",
            "everything",
            "package --jobs 0",
            "package --jobs",
            "package --frobnicate",
            "package extra",
            "explain a b",
            "explain --tier merge",
            "merge",
            "suspects",
            "package --from-log x.log",
            "timings --from-log",
            "timings extra",
        ] {
            assert!(
                matches!(parse(line), Err(GateError::Usage(_))),
                "`{line}` must be a usage error"
            );
        }
    }
}
