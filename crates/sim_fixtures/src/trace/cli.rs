//! The battle trace runner: command-line arguments, the seed list, and writing the outputs.
//!
//! # Responsibility
//! Parses the arguments of `crates/sim_core/examples/matchup_trace.rs`. In battle mode
//! (`--battle <name>`) it plays the selected matrix battle on each selected seed with
//! [`BattleRun`], traces every tick with [`BattleTrace`], and writes per seed
//! `seed_<n>_ticks.csv`, `seed_<n>_units.csv` and `seed_<n>.md`, plus `seeds.csv` and
//! `summary.md` for all seeds, into `<out>/<battle>/`. Whole-matrix mode (`--all-battles`) is
//! [`super::matrix`]; this module parses its flags and provides the shared catalog, fixture and
//! file helpers.
//!
//! # How it fits
//! The example is a thin `main` around [`TraceArgs::parse`], [`run`] and
//! [`super::matrix::run_matrix`], so the whole runner is tested here. It reads the matrix fixture (the shipped one by default, or `--fixture <file>`)
//! and the catalog (the shipped one by default, or `--catalog-dir <dir>`), so it names no unit in
//! Rust (charter §3.3): counterfactuals are scratch catalogs and scratch fixtures.
//!
//! # Invariants
//! - The battles are played exactly as the matrix test plays them (same setup code, same seeds,
//!   same jitter rule), so a seed of record reproduces the matrix result.
//! - A simulation diagnostic during a traced battle is an error, as it is in the matrix test.
//! - Seeds are distinct and played in the order listed.
//! - Exactly one of `--battle` and `--all-battles` is given; `--compare` and `--jobs` belong to
//!   the whole-matrix mode.

use super::report::{
    aggregate_markdown, run_markdown, seeds_csv, ticks_csv, units_csv, SeedResult,
};
use super::{BattleTrace, TraceError};
use crate::matchup::{BattleRun, MatchupError, Matrix, MATRIX_RON};
use sim_core::catalog::{CatalogLoadError, RuntimeCatalogs};
use std::path::{Path, PathBuf};

/// The usage text printed on an argument error.
pub const USAGE: &str = "usage: matchup_trace --battle <name> --out <dir> [--seed <n>]... \
[--seeds <count>] [--catalog-dir <dir>] [--fixture <file>] [--no-unit-csv]
       matchup_trace --all-battles --out <dir> [--seed <n>]... [--seeds <count>] \
[--catalog-dir <dir>] [--fixture <file>] [--compare <matrix.csv>] [--jobs <n>]
  --battle <name>    trace one battle tick by tick into <dir>/<name>/
  --all-battles      play every battle of the fixture and write <dir>/matrix.csv and
                     <dir>/matrix.md (outcomes only, no per-tick files)
  --seed <n>         play world seed n (repeatable)
  --seeds <count>    play the fixture's seeds of record plus the first <count> positive
                     integers that are not seeds of record
  (neither)          play the fixture's seeds of record
  --catalog-dir      catalog directory to load instead of the shipped catalog
  --fixture          matchup matrix file to read instead of the shipped one
  --no-unit-csv      skip the per-unit CSV files (battle mode)
  --compare <file>   matrix.csv of an earlier --all-battles run: report the battles whose
                     rows differ, or that nothing moved
  --jobs <n>         battles played at once in --all-battles mode (default 4)";

/// Battles played at once in whole-matrix mode when `--jobs` is not given; modest because the
/// tool runs on shared machines.
pub const DEFAULT_JOBS: usize = 4;

/// Why a trace run failed.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The arguments are invalid.
    #[error("{0}\n{USAGE}")]
    Usage(String),
    /// The fixture or the battle setup failed.
    #[error(transparent)]
    Matchup(#[from] MatchupError),
    /// The catalog directory could not be loaded.
    #[error(transparent)]
    Catalog(#[from] CatalogLoadError),
    /// The trace could not record the battle.
    #[error(transparent)]
    Trace(#[from] TraceError),
    /// The simulation raised a diagnostic during the battle.
    #[error("seed {seed}, tick {tick}: simulation diagnostics {diagnostics}")]
    Diagnostics {
        /// The seed being played.
        seed: u64,
        /// The tick just stepped.
        tick: u32,
        /// The diagnostics, formatted.
        diagnostics: String,
    },
    /// A battle of the whole-matrix mode failed.
    #[error("battle {battle}, seed {seed}: {source}")]
    Battle {
        /// The battle's name.
        battle: String,
        /// The seed being played.
        seed: u64,
        /// What went wrong.
        #[source]
        source: Box<RunError>,
    },
    /// A `matrix.csv` (to write or to compare with) is not in the tool's format.
    #[error("{path}: line {line}: {reason}")]
    Csv {
        /// The file (the output path when writing).
        path: PathBuf,
        /// The 1-based line number.
        line: usize,
        /// What is wrong with it.
        reason: String,
    },
    /// A file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The file or directory.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// The parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceArgs {
    /// Name of the matrix battle to trace (battle mode); `None` in whole-matrix mode.
    pub battle: Option<String>,
    /// Whether to play every battle of the fixture (whole-matrix mode).
    pub all_battles: bool,
    /// Output directory: files go to `<out>/<battle>/` in battle mode, `<out>/` in whole-matrix
    /// mode.
    pub out: PathBuf,
    /// Seeds given with `--seed`.
    pub seeds: Vec<u64>,
    /// Count given with `--seeds`.
    pub extra_seeds: Option<u64>,
    /// Catalog directory, if not the shipped catalog.
    pub catalog_dir: Option<PathBuf>,
    /// Matrix file, if not the shipped fixture.
    pub fixture: Option<PathBuf>,
    /// Whether to write the per-unit CSV files.
    pub unit_csv: bool,
    /// An earlier `matrix.csv` to compare the whole-matrix run with.
    pub compare: Option<PathBuf>,
    /// Battles played at once in whole-matrix mode; at least 1.
    pub jobs: usize,
}

impl TraceArgs {
    /// Parses the arguments after the program name.
    ///
    /// # Errors
    /// [`RunError::Usage`] for an unknown flag, a missing or malformed value, a missing
    /// `--out`, not exactly one of `--battle` and `--all-battles`, `--compare` or `--jobs`
    /// outside whole-matrix mode, or `--jobs 0`.
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Self, RunError> {
        let mut args = args.into_iter();
        let mut out = None;
        let mut jobs = None;
        let mut parsed = Self {
            battle: None,
            all_battles: false,
            out: PathBuf::new(),
            seeds: Vec::new(),
            extra_seeds: None,
            catalog_dir: None,
            fixture: None,
            unit_csv: true,
            compare: None,
            jobs: DEFAULT_JOBS,
        };
        while let Some(flag) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| RunError::Usage(format!("{flag} needs a value")))
            };
            match flag.as_str() {
                "--battle" => parsed.battle = Some(value()?),
                "--all-battles" => parsed.all_battles = true,
                "--compare" => parsed.compare = Some(PathBuf::from(value()?)),
                "--jobs" => jobs = Some(parse_number(&flag, &value()?)?),
                "--out" => out = Some(PathBuf::from(value()?)),
                "--seed" => parsed.seeds.push(parse_number(&flag, &value()?)?),
                "--seeds" => parsed.extra_seeds = Some(parse_number(&flag, &value()?)?),
                "--catalog-dir" => parsed.catalog_dir = Some(PathBuf::from(value()?)),
                "--fixture" => parsed.fixture = Some(PathBuf::from(value()?)),
                "--no-unit-csv" => parsed.unit_csv = false,
                _ => return Err(RunError::Usage(format!("unknown argument {flag}"))),
            }
        }
        parsed.out = out.ok_or_else(|| RunError::Usage("--out is required".into()))?;
        match (&parsed.battle, parsed.all_battles) {
            (None, false) => {
                return Err(RunError::Usage(
                    "--battle <name> or --all-battles is required".into(),
                ))
            }
            (Some(_), true) => {
                return Err(RunError::Usage(
                    "--battle and --all-battles exclude each other".into(),
                ))
            }
            (Some(_), false) if parsed.compare.is_some() || jobs.is_some() => {
                return Err(RunError::Usage(
                    "--compare and --jobs need --all-battles".into(),
                ))
            }
            _ => {}
        }
        if let Some(jobs) = jobs {
            parsed.jobs = usize::try_from(jobs)
                .ok()
                .filter(|&jobs| jobs > 0)
                .ok_or_else(|| RunError::Usage(format!("--jobs needs 1 or more, got {jobs}")))?;
        }
        Ok(parsed)
    }

    /// The seeds to play, in order and without repeats: the `--seed` values; then, with
    /// `--seeds <count>`, the seeds of record followed by the first `count` positive integers
    /// that are not seeds of record; with neither, the seeds of record.
    #[must_use]
    pub fn seed_list(&self, seeds_of_record: &[u64]) -> Vec<u64> {
        let mut list = self.seeds.clone();
        if self.extra_seeds.is_some() || self.seeds.is_empty() {
            list.extend_from_slice(seeds_of_record);
        }
        let mut wanted = self.extra_seeds.unwrap_or(0);
        let mut next = 1_u64;
        while wanted > 0 {
            if !seeds_of_record.contains(&next) {
                list.push(next);
                wanted -= 1;
            }
            next += 1;
        }
        let mut seen = std::collections::BTreeSet::new();
        list.retain(|seed| seen.insert(*seed));
        list
    }
}

fn parse_number(flag: &str, text: &str) -> Result<u64, RunError> {
    text.parse()
        .map_err(|_| RunError::Usage(format!("{flag} needs a non-negative integer, got {text}")))
}

/// What a run wrote.
#[derive(Debug, Clone)]
pub struct RunOutput {
    /// The directory holding every file.
    pub directory: PathBuf,
    /// One result per seed, in play order.
    pub results: Vec<SeedResult>,
}

/// The matchup matrix `args` names: `--fixture`, or the shipped one.
///
/// # Errors
/// [`RunError::Io`] if the file cannot be read, [`RunError::Matchup`] if it does not parse.
pub fn load_matrix(args: &TraceArgs) -> Result<Matrix, RunError> {
    let fixture_text = match &args.fixture {
        Some(path) => read(path)?,
        None => MATRIX_RON.to_owned(),
    };
    Ok(Matrix::from_ron_str(&fixture_text)?)
}

/// The catalogs `args` names: `--catalog-dir`, or the shipped ones.
///
/// # Errors
/// [`RunError::Catalog`] if the directory does not load.
pub fn load_catalogs(args: &TraceArgs) -> Result<RuntimeCatalogs, RunError> {
    Ok(match &args.catalog_dir {
        Some(dir) => RuntimeCatalogs::load_from_directory(dir)?,
        None => crate::shipped_catalogs(),
    })
}

/// A line naming the catalog and the fixture `args` played, for the reports.
#[must_use]
pub fn source_line(args: &TraceArgs) -> String {
    format!(
        "Catalog: {}. Fixture: {}.",
        args.catalog_dir
            .as_ref()
            .map_or_else(|| "shipped".to_owned(), |d| d.display().to_string()),
        args.fixture.as_ref().map_or_else(
            || "shipped matchup matrix".to_owned(),
            |f| f.display().to_string()
        ),
    )
}

/// Plays and traces the battle of `args` on every selected seed and writes the outputs.
///
/// # Errors
/// Every [`RunError`]; [`RunError::Usage`] if `args` selects no single battle.
pub fn run(args: &TraceArgs) -> Result<RunOutput, RunError> {
    let name = args
        .battle
        .as_deref()
        .ok_or_else(|| RunError::Usage("battle mode needs --battle <name>".into()))?;
    let matrix = load_matrix(args)?;
    let battle = matrix.battle(name)?;
    let catalogs = load_catalogs(args)?;
    let directory = args.out.join(&battle.name);
    create_dir(&directory)?;
    let mut results = Vec::new();
    for seed in args.seed_list(&matrix.seeds) {
        let jitter = matrix.jitter_for(seed);
        let trace = trace_battle(battle, catalogs.clone(), seed, jitter)?;
        let summary = trace.summary();
        let stem = directory.join(format!("seed_{seed}"));
        write(
            &stem.with_extension("md"),
            &run_markdown(
                &format!("{} — seed {seed} (jitter {jitter:.2})", battle.name),
                &summary,
            ),
        )?;
        write(&path_with_suffix(&stem, "_ticks.csv"), &ticks_csv(&trace))?;
        if args.unit_csv {
            write(&path_with_suffix(&stem, "_units.csv"), &units_csv(&trace))?;
        }
        results.push(SeedResult {
            seed,
            jitter,
            summary,
        });
    }
    let source = format!(
        "Claim: {}\n\n{} Seeds: {}.",
        battle.claim,
        source_line(args),
        results.len()
    );
    write(&directory.join("seeds.csv"), &seeds_csv(&results))?;
    write(
        &directory.join("summary.md"),
        &aggregate_markdown(&battle.name, &source, &results),
    )?;
    Ok(RunOutput { directory, results })
}

/// Plays `battle` with `catalogs` on `seed` with `jitter`, tracing every tick.
///
/// # Errors
/// [`RunError::Matchup`] for a setup failure, [`RunError::Trace`] if the trace fails, and
/// [`RunError::Diagnostics`] if the simulation reports a diagnostic.
pub fn trace_battle(
    battle: &crate::matchup::Battle,
    catalogs: RuntimeCatalogs,
    seed: u64,
    jitter: f32,
) -> Result<BattleTrace, RunError> {
    let mut run = BattleRun::new(battle, catalogs, seed, jitter)?;
    let mut trace = BattleTrace::new(&run.world, &run.blue, &run.red)?;
    while !run.is_finished() {
        let diagnostics = run.step();
        if !diagnostics.is_empty() {
            return Err(RunError::Diagnostics {
                seed,
                tick: run.ticks_played(),
                diagnostics: format!("{diagnostics:?}"),
            });
        }
        trace.record(&run.world)?;
    }
    Ok(trace)
}

fn path_with_suffix(stem: &Path, suffix: &str) -> PathBuf {
    let mut name = stem.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Creates `directory` and its parents.
///
/// # Errors
/// [`RunError::Io`] if it cannot be created.
pub fn create_dir(directory: &Path) -> Result<(), RunError> {
    std::fs::create_dir_all(directory).map_err(|source| RunError::Io {
        path: directory.to_owned(),
        source,
    })
}

/// The text of the file at `path`.
///
/// # Errors
/// [`RunError::Io`] if it cannot be read.
pub fn read(path: &Path) -> Result<String, RunError> {
    std::fs::read_to_string(path).map_err(|source| RunError::Io {
        path: path.to_owned(),
        source,
    })
}

/// Writes `text` to the file at `path`.
///
/// # Errors
/// [`RunError::Io`] if it cannot be written.
pub fn write(path: &Path, text: &str) -> Result<(), RunError> {
    std::fs::write(path, text).map_err(|source| RunError::Io {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Result<TraceArgs, RunError> {
        TraceArgs::parse(text.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn parses_every_flag() -> Result<(), RunError> {
        let parsed = args(
            "--battle b --out o --seed 5 --seed 6 --seeds 3 --catalog-dir c --fixture f \
             --no-unit-csv",
        )?;
        assert_eq!(parsed.battle.as_deref(), Some("b"));
        assert!(!parsed.all_battles);
        assert_eq!(parsed.jobs, DEFAULT_JOBS);
        assert_eq!(parsed.out, PathBuf::from("o"));
        assert_eq!(parsed.seeds, vec![5, 6]);
        assert_eq!(parsed.extra_seeds, Some(3));
        assert_eq!(parsed.catalog_dir, Some(PathBuf::from("c")));
        assert_eq!(parsed.fixture, Some(PathBuf::from("f")));
        assert!(!parsed.unit_csv);
        let matrix = args("--all-battles --out o --compare m.csv --jobs 8")?;
        assert_eq!(matrix.battle, None);
        assert!(matrix.all_battles);
        assert_eq!(matrix.compare, Some(PathBuf::from("m.csv")));
        assert_eq!(matrix.jobs, 8);
        Ok(())
    }

    #[test]
    fn rejects_missing_values_unknown_flags_and_missing_requirements() {
        for bad in [
            "--battle",
            "--battle b",
            "--out o",
            "--battle b --out o --seed x",
            "--battle b --out o --frobnicate",
            "--battle b --all-battles --out o",
            "--battle b --out o --jobs 2",
            "--all-battles --out o --jobs 0",
            "--all-battles --out o --jobs x",
        ] {
            assert!(matches!(args(bad), Err(RunError::Usage(_))), "{bad}");
        }
    }

    #[test]
    fn seeds_default_to_the_record_and_extend_past_it_without_repeats() -> Result<(), RunError> {
        let record = [3_203_386_110, 2, 4];
        assert_eq!(
            args("--battle b --out o")?.seed_list(&record),
            record.to_vec()
        );
        assert_eq!(
            args("--battle b --out o --seed 9")?.seed_list(&record),
            vec![9]
        );
        assert_eq!(
            args("--battle b --out o --seeds 4")?.seed_list(&record),
            vec![3_203_386_110, 2, 4, 1, 3, 5, 6]
        );
        assert_eq!(
            args("--battle b --out o --seed 2 --seeds 1")?.seed_list(&record),
            vec![2, 3_203_386_110, 4, 1]
        );
        Ok(())
    }
}
