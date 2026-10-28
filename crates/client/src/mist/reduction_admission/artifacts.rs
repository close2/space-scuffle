//! Bounded, non-overwriting evidence publication for the full CPU admission corpus.
//!
//! Every window receives one streamed RON record. Full trace/history evidence is
//! retained for the first failure in each finite category; omissions are explicit.

use super::{
    trace::{
        self, AuthoritativeTrace, FailureCategory, WindowFailure, WindowResult, WindowSummary,
    },
    AdmissionCase, AdmissionCorpus, AdmissionError, UnsupportedFamily,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

/// Completed full-matrix accounting. Any failed window keeps representation admission open.
#[derive(Clone, Debug, Serialize)]
pub struct AdmissionSummary {
    /// Schema version of this evidence document.
    pub format_version: u32,
    /// Planned windows from actual loaded inventory and complete authored matrix.
    pub planned_windows: u64,
    /// All accounted windows, including producer refusals with zero executed frames.
    pub attempted_windows: u64,
    /// Windows meeting their observed control and capability contract.
    pub passed_windows: u64,
    /// Windows with an explicit non-success result.
    pub failed_windows: u64,
    /// Total actual display observations, not the virtual requested frame count.
    pub display_frames: u64,
    /// Complete per-frame records; partial failed updates are included only in display_frames.
    pub completed_display_frames: u64,
    /// Actual authoritative steps used to generate the reused traces.
    pub generated_ticks: u64,
    /// Additional real authoritative steps in explicitly identified no-cast controls.
    pub control_ticks: u64,
    /// Actual number of records in the original selected mobile inventory.
    pub mobile_archetypes: usize,
    /// Executable capability-selected families before rate/speed/phase expansion.
    pub executable_families: usize,
    /// Optional families unavailable in original authored data.
    pub unsupported_families: usize,
    /// Windows observing each typed category; one window may contribute to several.
    pub failures_by_category: BTreeMap<FailureCategory, u64>,
    /// Categories whose first complete failing window was retained.
    pub retained_failure_categories: BTreeSet<FailureCategory>,
    /// Bytes written before the final summary, including exact copied inputs.
    pub artifact_bytes_before_summary: u64,
    /// Largest serialized single evidence record in bytes.
    pub maximum_record_bytes: usize,
    /// Wall duration of the complete diagnostic run; not a rendering benchmark.
    pub elapsed_seconds: f64,
    /// Build assertion setting; external preservation records the actual Cargo profile/toolchain.
    pub debug_assertions: bool,
    /// BLAKE3-256 of the executable actually running this corpus.
    pub executable_blake3_256: String,
    /// Input files were re-read and their digests matched at completion.
    pub inputs_unchanged: bool,
    /// Control admission never certifies optical mass, temporal field behavior or art.
    pub field_and_art_accepted: bool,
}

#[derive(Clone, Debug, Serialize)]
struct InputIdentity {
    relative_path: PathBuf,
    bytes: u64,
    blake3_256: String,
}

#[derive(Serialize)]
struct RunInputs<'a> {
    format_version: u32,
    root: PathBuf,
    spec: &'a super::AdmissionSpec,
    cases: &'a [AdmissionCase],
    unsupported: &'a [UnsupportedFamily],
    block_ns: u64,
    horizon_ns: u64,
    identities: &'a [InputIdentity],
    invoked_arguments: Vec<String>,
    checksum_scope: &'static str,
}

#[derive(Serialize)]
struct TickEvidence<'a> {
    bundle: &'a sim_types::TickBundle,
    output: &'a sim_types::RenderSnapshot,
    direct_checksum: u64,
    movement_outcomes: String,
    production_outcomes: String,
    diagnostics: String,
}

#[derive(Serialize)]
struct TraceEvidence<'a> {
    case: &'a AdmissionCase,
    speed: u32,
    subject: sim_types::EntityId,
    other: Option<sim_types::EntityId>,
    map_dimensions: [u32; 2],
    initial: &'a sim_types::RenderSnapshot,
    initial_direct_checksum: u64,
    generation_elapsed_seconds: f64,
    control_ticks: u32,
    blink_transaction: Option<&'a trace::BlinkTransaction>,
    blink_tick: Option<u32>,
    blink_displacement: Option<[f32; 2]>,
    ticks: Vec<TickEvidence<'a>>,
}

#[derive(Serialize)]
struct FailureEvidence<'a> {
    summary: &'a WindowSummary,
    source: Option<TraceEvidence<'a>>,
    frames: &'a [trace::DisplayFrame],
    retention_policy: &'static str,
}

struct Publisher {
    directory: PathBuf,
    windows: BufWriter<File>,
    bytes: u64,
    largest: usize,
    retained: BTreeSet<FailureCategory>,
}

fn io(path: &Path, source: std::io::Error) -> AdmissionError {
    AdmissionError::Io {
        path: path.to_owned(),
        source,
    }
}

fn count_bytes(total: &mut u64, bytes: usize) -> Result<(), AdmissionError> {
    *total = total
        .checked_add(
            u64::try_from(bytes).map_err(|_| AdmissionError::Specification("artifact size"))?,
        )
        .ok_or(AdmissionError::Specification("total artifact size"))?;
    Ok(())
}

fn new_file(path: &Path) -> Result<File, AdmissionError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| io(path, source))
}

fn digest_file(path: &Path) -> Result<(u64, String), AdmissionError> {
    let mut input = File::open(path).map_err(|source| io(path, source))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 16_384];
    let mut total = 0_u64;
    loop {
        let count = input.read(&mut buffer).map_err(|source| io(path, source))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        count_bytes(&mut total, count)?;
    }
    Ok((total, hasher.finalize().to_hex().to_string()))
}

impl Publisher {
    fn new(directory: &Path) -> Result<Self, AdmissionError> {
        std::fs::create_dir(directory).map_err(|source| io(directory, source))?;
        let windows = BufWriter::new(new_file(&directory.join("windows.ronl"))?);
        Ok(Self {
            directory: directory.to_owned(),
            windows,
            bytes: 0,
            largest: 0,
            retained: BTreeSet::new(),
        })
    }

    fn write(&mut self, name: &str, value: &impl Serialize) -> Result<(), AdmissionError> {
        let bytes =
            ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())?.into_bytes();
        let path = self.directory.join(name);
        let mut file = new_file(&path)?;
        file.write_all(&bytes).map_err(|source| io(&path, source))?;
        file.flush().map_err(|source| io(&path, source))?;
        self.largest = self.largest.max(bytes.len());
        count_bytes(&mut self.bytes, bytes.len())
    }

    fn record(
        &mut self,
        result: &WindowResult,
        source: Option<&AuthoritativeTrace>,
    ) -> Result<(), AdmissionError> {
        let record = ron::to_string(&result.summary)?;
        self.windows
            .write_all(record.as_bytes())
            .and_then(|()| self.windows.write_all(b"\n"))
            .map_err(|source| io(&self.directory.join("windows.ronl"), source))?;
        count_bytes(&mut self.bytes, record.len() + 1)?;
        self.largest = self.largest.max(record.len());
        for failure in &result.summary.failures {
            if self.retained.insert(failure.category) {
                let source = source.map(|trace| TraceEvidence {
                    case: &trace.case,
                    speed: trace.speed,
                    subject: trace.subject,
                    other: trace.other,
                    map_dimensions: trace.dimensions,
                    initial: &trace.initial,
                    initial_direct_checksum: trace.initial_direct_checksum,
                    generation_elapsed_seconds: trace.elapsed_seconds,
                    control_ticks: trace.control_ticks,
                    blink_transaction: trace.blink_transaction.as_ref(),
                    blink_tick: trace.blink_tick,
                    blink_displacement: trace.blink_displacement,
                    ticks: trace
                        .ticks
                        .iter()
                        .map(|tick| TickEvidence {
                            bundle: &tick.bundle,
                            output: &tick.output,
                            direct_checksum: tick.direct_checksum,
                            movement_outcomes: format!("{:?}", tick.movement),
                            production_outcomes: format!("{:?}", tick.production),
                            diagnostics: format!("{:?}", tick.diagnostics),
                        })
                        .collect(),
                });
                self.write(&format!("first-{:?}.ron", failure.category), &FailureEvidence { summary: &result.summary, source, frames: &result.frames, retention_policy: "first complete window per observed typed failure category; every later window still appears in windows.ronl" })?;
            }
        }
        Ok(())
    }
}

fn preserve_inputs(
    corpus: &AdmissionCorpus,
    publisher: &mut Publisher,
) -> Result<Vec<InputIdentity>, AdmissionError> {
    let paths = [
        PathBuf::from("catalog/units.ron"),
        PathBuf::from("catalog/structures.ron"),
        PathBuf::from("catalog/upgrades.ron"),
        PathBuf::from("catalog/mist.ron"),
        corpus.source_spec.clone(),
        corpus.spec.ramp_map.clone(),
    ];
    let destination = publisher.directory.join("inputs");
    std::fs::create_dir(&destination).map_err(|source| io(&destination, source))?;
    let mut identities = Vec::new();
    for (index, relative_path) in paths.into_iter().enumerate() {
        let path = corpus.root.as_path().join(&relative_path);
        let bytes = std::fs::read(&path).map_err(|source| io(&path, source))?;
        let digest = blake3::hash(&bytes).to_hex().to_string();
        if corpus
            .loaded_input_digests
            .iter()
            .find(|(path, _)| *path == relative_path)
            .map(|(_, expected)| expected)
            != Some(&digest)
        {
            return Err(AdmissionError::Specification(
                "input no longer matches the actual loaded catalog/style/spec",
            ));
        }
        let file = destination.join(format!("{index}.ron"));
        new_file(&file)?
            .write_all(&bytes)
            .map_err(|source| io(&file, source))?;
        count_bytes(&mut publisher.bytes, bytes.len())?;
        identities.push(InputIdentity {
            relative_path,
            bytes: u64::try_from(bytes.len())
                .map_err(|_| AdmissionError::Specification("input size"))?,
            blake3_256: digest,
        });
    }
    Ok(identities)
}

fn producer_refusal(
    case: &AdmissionCase,
    speed: u32,
    rate: u32,
    phase: i64,
    error: &AdmissionError,
) -> WindowResult {
    WindowResult {
        summary: WindowSummary {
            case: case.clone(),
            speed,
            rate,
            phase_offset_ns: phase,
            frames: 0,
            attempted_frames: 0,
            completed_horizon: false,
            frame_bound: 0,
            ticks: 0,
            moving_histories: 0,
            maximum_controls: 0,
            maximum_error_ratio: 0.0,
            relative_arc_deficit_range: None,
            absolute_arc_deficit_range: None,
            source_evaluations: 0,
            presented_events: 0,
            displayed_travel: 0.0,
            ground_range: None,
            boosted_frames: 0,
            discarded_ns: 0,
            coverage: None,
            blink_transaction: None,
            failure: Some(WindowFailure {
                category: FailureCategory::Producer,
                frame: 0,
                diagnostic: format!("{error:?}"),
            }),
            failures: vec![WindowFailure {
                category: FailureCategory::Producer,
                frame: 0,
                diagnostic: format!("{error:?}"),
            }],
        },
        frames: Vec::new(),
    }
}

pub(super) fn run(
    corpus: &AdmissionCorpus,
    output: &Path,
) -> Result<AdmissionSummary, AdmissionError> {
    let started = Instant::now();
    let mut publisher = Publisher::new(output)?;
    let identities = preserve_inputs(corpus, &mut publisher)?;
    publisher.write("inputs.ron", &RunInputs { format_version: 1, root: corpus.root.as_path().to_owned(), spec: &corpus.spec, cases: &corpus.cases, unsupported: &corpus.unsupported, block_ns: u64::try_from(corpus.block.as_nanos()).map_err(|_| AdmissionError::Specification("block duration"))?, horizon_ns: u64::try_from(corpus.horizon.as_nanos()).map_err(|_| AdmissionError::Specification("horizon duration"))?, identities: &identities, invoked_arguments: std::env::args().collect(), checksum_scope: "initial direct and cached published checksums are separate; reused output checksums establish playback provenance, not independent authoritative determinism" })?;
    let executable =
        std::env::current_exe().map_err(|source| io(Path::new("current executable"), source))?;
    let mut summary = AdmissionSummary {
        format_version: 1,
        planned_windows: corpus.window_count()?,
        attempted_windows: 0,
        passed_windows: 0,
        failed_windows: 0,
        display_frames: 0,
        completed_display_frames: 0,
        generated_ticks: 0,
        control_ticks: 0,
        mobile_archetypes: corpus
            .catalogs
            .units
            .archetypes()
            .filter(|a| !a.is_immobile_by_default())
            .count(),
        executable_families: corpus.cases.len(),
        unsupported_families: corpus.unsupported.len(),
        failures_by_category: BTreeMap::new(),
        retained_failure_categories: BTreeSet::new(),
        artifact_bytes_before_summary: 0,
        maximum_record_bytes: 0,
        elapsed_seconds: 0.0,
        debug_assertions: cfg!(debug_assertions),
        executable_blake3_256: digest_file(&executable)?.1,
        inputs_unchanged: false,
        field_and_art_accepted: false,
    };
    for case in &corpus.cases {
        for &speed in &corpus.spec.battle_speeds {
            let generated = trace::generate(corpus, case, speed);
            if let Ok(trace) = &generated {
                summary.control_ticks += u64::from(trace.control_ticks);
                summary.generated_ticks += u64::try_from(trace.ticks.len())
                    .map_err(|_| AdmissionError::Specification("generated ticks"))?;
            }
            for &rate in &corpus.spec.display_rates {
                for &phase in &corpus.spec.first_frame_offsets_ns {
                    let result = match &generated {
                        Ok(trace) => match trace::replay(corpus, trace, rate, phase, false, None) {
                            Ok(window) => window,
                            Err(error) => producer_refusal(case, speed, rate, phase, &error),
                        },
                        Err(error) => producer_refusal(case, speed, rate, phase, error),
                    };
                    summary.attempted_windows += 1;
                    summary.display_frames += u64::from(result.summary.attempted_frames);
                    summary.completed_display_frames += u64::from(result.summary.frames);
                    if result.summary.failure.is_some() {
                        summary.failed_windows += 1;
                        for observed in &result.summary.failures {
                            *summary
                                .failures_by_category
                                .entry(observed.category)
                                .or_default() += 1;
                        }
                    } else {
                        summary.passed_windows += 1;
                    }
                    publisher.record(&result, generated.as_ref().ok())?;
                }
            }
        }
    }
    publisher
        .windows
        .flush()
        .map_err(|source| io(&output.join("windows.ronl"), source))?;
    for identity in &identities {
        let actual = digest_file(&corpus.root.as_path().join(&identity.relative_path))?;
        if actual != (identity.bytes, identity.blake3_256.clone()) {
            return Err(AdmissionError::Specification(
                "runtime input changed during admission; reject moving-source evidence",
            ));
        }
    }
    if summary.attempted_windows != summary.planned_windows {
        return Err(AdmissionError::Specification(
            "full matrix accounting mismatch",
        ));
    }
    summary.inputs_unchanged = true;
    summary.elapsed_seconds = started.elapsed().as_secs_f64();
    summary
        .retained_failure_categories
        .clone_from(&publisher.retained);
    summary.artifact_bytes_before_summary = publisher.bytes;
    summary.maximum_record_bytes = publisher.largest;
    publisher.write("summary.ron", &summary)?;
    if summary.failed_windows > 0 {
        return Err(AdmissionError::Rejected {
            attempted: summary.attempted_windows,
            failed: summary.failed_windows,
            output: output.to_owned(),
        });
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn corpus() -> Result<AdmissionCorpus, AdmissionError> {
        AdmissionCorpus::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            Path::new("captures/mist_reduction.ron"),
        )
    }

    #[test]
    fn every_observed_category_gets_one_retained_window_without_overwrite() -> TestResult {
        let corpus = corpus()?;
        let case = corpus.cases.first().ok_or("actual corpus empty")?;
        let temporary = tempfile::tempdir()?;
        let output = temporary.path().join("evidence");
        let mut publisher = Publisher::new(&output)?;
        let mut result = producer_refusal(case, 1, 30, -1, &AdmissionError::Specification("first"));
        result.summary.failures.push(WindowFailure {
            category: FailureCategory::Discontinuity,
            frame: 9,
            diagnostic: "later distinct category".into(),
        });
        publisher.record(&result, None)?;
        let first = std::fs::read(output.join("first-Producer.ron"))?;
        let second = std::fs::read(output.join("first-Discontinuity.ron"))?;
        result.summary.failures[0].diagnostic =
            "subsequent window must not replace original".into();
        publisher.record(&result, None)?;
        publisher.windows.flush()?;
        assert_eq!(std::fs::read(output.join("first-Producer.ron"))?, first);
        assert_eq!(
            std::fs::read(output.join("first-Discontinuity.ron"))?,
            second
        );
        assert_eq!(publisher.retained.len(), 2);
        assert_eq!(
            std::fs::read_to_string(output.join("windows.ronl"))?
                .lines()
                .count(),
            2
        );
        assert!(matches!(
            Publisher::new(&output),
            Err(AdmissionError::Io { .. })
        ));
        Ok(())
    }

    #[test]
    fn input_mutation_after_load_is_rejected_before_matrix_execution() -> TestResult {
        let mut corpus = corpus()?;
        let temporary = tempfile::tempdir()?;
        let asset_root = temporary.path().join("assets");
        for relative in corpus
            .loaded_input_digests
            .iter()
            .map(|(path, _)| path.as_path())
            .chain([Path::new("shaders/bubble_dome.wgsl")])
        {
            let target = asset_root.join(relative);
            std::fs::create_dir_all(target.parent().ok_or("actual input parent absent")?)?;
            std::fs::copy(corpus.root.as_path().join(relative), target)?;
        }
        corpus.root = crate::ResolvedAssetsRoot::from_directory(&asset_root)?;
        let mut changed = OpenOptions::new()
            .append(true)
            .open(asset_root.join("catalog/mist.ron"))?;
        changed.write_all(b"\n")?;
        changed.flush()?;
        let output = temporary.path().join("evidence");
        let mut publisher = Publisher::new(&output)?;
        assert!(matches!(
            preserve_inputs(&corpus, &mut publisher),
            Err(AdmissionError::Specification(
                "input no longer matches the actual loaded catalog/style/spec"
            ))
        ));
        assert!(!output.join("summary.ron").exists());
        Ok(())
    }
}
