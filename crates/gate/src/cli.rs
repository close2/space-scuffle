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
//! - `--jobs` defaults to 1, the sequential run of `cargo test` (blueprint §2.6).

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

options:
  --base <rev>        compare with this revision (default: `git merge-base HEAD <main>`)
  --main <ref>        the main branch (default: main)
  --jobs <n>          concurrent test executables (default 1; GPU targets get their own lane)
  --log-dir <dir>     logs (default ~/scratch/logs/gate/<tree-hash>/)
  --stamp-dir <dir>   stamps (default ~/scratch/gate-stamps/)
  --no-run            select and build only
  --from-log <file>   timings and suspects only: read a `cargo test` log";

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
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            base: None,
            main_ref: "main".to_string(),
            jobs: 1,
            log_dir: None,
            stamp_dir: None,
            no_run: false,
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
}

/// Parses `args` (without the program name).
///
/// # Errors
/// [`GateError::Usage`] for anything that is not one of the forms in [`USAGE`].
pub fn parse_args(args: &[String]) -> Result<Command, GateError> {
    let usage = |message: String| GateError::Usage(message);
    let (word, rest) = args
        .split_first()
        .ok_or_else(|| usage("no command given".to_string()))?;
    let mut options = RunOptions::default();
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
            }
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
        parse_args(&args)
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
