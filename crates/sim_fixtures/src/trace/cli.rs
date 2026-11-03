//! The battle trace runner: command-line arguments, the seed list, and writing the outputs.
//!
//! # Responsibility
//! Parses the arguments of `crates/sim_core/examples/matchup_trace.rs`, plays the selected matrix
//! battle on each selected seed with [`BattleRun`], traces every tick with [`BattleTrace`], and
//! writes per seed `seed_<n>_ticks.csv`, `seed_<n>_units.csv` and `seed_<n>.md`, plus
//! `seeds.csv` and `summary.md` for all seeds, into `<out>/<battle>/`.
//!
//! # How it fits
//! The example is a thin `main` around [`TraceArgs::parse`] and [`run`], so the whole runner is
//! tested here. It reads the matrix fixture (the shipped one by default, or `--fixture <file>`)
//! and the catalog (the shipped one by default, or `--catalog-dir <dir>`), so it names no unit in
//! Rust (charter §3.3): counterfactuals are scratch catalogs and scratch fixtures.
//!
//! # Invariants
//! - The battles are played exactly as the matrix test plays them (same setup code, same seeds,
//!   same jitter rule), so a seed of record reproduces the matrix result.
//! - A simulation diagnostic during a traced battle is an error, as it is in the matrix test.
//! - Seeds are distinct and played in the order listed.

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
  --seed <n>         trace world seed n (repeatable)
  --seeds <count>    trace the fixture's seeds of record plus the first <count> positive
                     integers that are not seeds of record
  (neither)          trace the fixture's seeds of record
  --catalog-dir      catalog directory to load instead of the shipped catalog
  --fixture          matchup matrix file to read instead of the shipped one
  --no-unit-csv      skip the per-unit CSV files";

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
    /// Name of the matrix battle.
    pub battle: String,
    /// Output directory; files go to `<out>/<battle>/`.
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
}

impl TraceArgs {
    /// Parses the arguments after the program name.
    ///
    /// # Errors
    /// [`RunError::Usage`] for an unknown flag, a missing or malformed value, or a missing
    /// `--battle` or `--out`.
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Self, RunError> {
        let mut args = args.into_iter();
        let (mut battle, mut out) = (None, None);
        let mut parsed = Self {
            battle: String::new(),
            out: PathBuf::new(),
            seeds: Vec::new(),
            extra_seeds: None,
            catalog_dir: None,
            fixture: None,
            unit_csv: true,
        };
        while let Some(flag) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| RunError::Usage(format!("{flag} needs a value")))
            };
            match flag.as_str() {
                "--battle" => battle = Some(value()?),
                "--out" => out = Some(PathBuf::from(value()?)),
                "--seed" => parsed.seeds.push(parse_number(&flag, &value()?)?),
                "--seeds" => parsed.extra_seeds = Some(parse_number(&flag, &value()?)?),
                "--catalog-dir" => parsed.catalog_dir = Some(PathBuf::from(value()?)),
                "--fixture" => parsed.fixture = Some(PathBuf::from(value()?)),
                "--no-unit-csv" => parsed.unit_csv = false,
                _ => return Err(RunError::Usage(format!("unknown argument {flag}"))),
            }
        }
        parsed.battle = battle.ok_or_else(|| RunError::Usage("--battle is required".into()))?;
        parsed.out = out.ok_or_else(|| RunError::Usage("--out is required".into()))?;
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

/// Plays and traces the battle of `args` on every selected seed and writes the outputs.
///
/// # Errors
/// Every [`RunError`].
pub fn run(args: &TraceArgs) -> Result<RunOutput, RunError> {
    let fixture_text = match &args.fixture {
        Some(path) => read(path)?,
        None => MATRIX_RON.to_owned(),
    };
    let matrix = Matrix::from_ron_str(&fixture_text)?;
    let battle = matrix.battle(&args.battle)?;
    let catalogs = match &args.catalog_dir {
        Some(dir) => RuntimeCatalogs::load_from_directory(dir)?,
        None => crate::shipped_catalogs(),
    };
    let directory = args.out.join(&battle.name);
    std::fs::create_dir_all(&directory).map_err(|source| RunError::Io {
        path: directory.clone(),
        source,
    })?;
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
        "Claim: {}\n\nCatalog: {}. Fixture: {}. Seeds: {}.",
        battle.claim,
        args.catalog_dir
            .as_ref()
            .map_or_else(|| "shipped".to_owned(), |d| d.display().to_string()),
        args.fixture.as_ref().map_or_else(
            || "shipped matchup matrix".to_owned(),
            |f| f.display().to_string()
        ),
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

fn read(path: &Path) -> Result<String, RunError> {
    std::fs::read_to_string(path).map_err(|source| RunError::Io {
        path: path.to_owned(),
        source,
    })
}

fn write(path: &Path, text: &str) -> Result<(), RunError> {
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
        assert_eq!(parsed.battle, "b");
        assert_eq!(parsed.out, PathBuf::from("o"));
        assert_eq!(parsed.seeds, vec![5, 6]);
        assert_eq!(parsed.extra_seeds, Some(3));
        assert_eq!(parsed.catalog_dir, Some(PathBuf::from("c")));
        assert_eq!(parsed.fixture, Some(PathBuf::from("f")));
        assert!(!parsed.unit_csv);
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
