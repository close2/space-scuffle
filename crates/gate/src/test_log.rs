//! Reading a `cargo test` log: which test binaries ran, for how long, and which failed.
//!
//! # Responsibility
//! Parses the text `cargo test` prints ([`parse_test_log`]) into one [`LoggedBinary`] per
//! `Running …` or `Doc-tests …` block, with the time libtest reports and the result, and maps
//! each block to its workspace target ([`LoggedBinary::target`]). [`render_log_timings`] turns
//! that into the Markdown class report CI writes into its job summary (blueprint §9.3:
//! `cargo gate timings --from-log`), and [`failing_targets`] gives the red rule its failing
//! targets when the red run was CI's (blueprint §3.5: `cargo gate suspects --from-log`).
//!
//! # How it fits
//! CI keeps running plain `cargo test --workspace --locked` and never depends on the selection
//! (blueprint §9.3, D2); this module only reads what that command printed, after the fact. The
//! VM's own runs record the same numbers in stamps (`timings.rs`).
//!
//! # Invariants
//! - A block's time and result come from the **last** `test result:` line before the next block,
//!   so a GPU child's output that a parent test prints inside its failure message cannot be
//!   mistaken for the parent's result.
//! - A block whose binary is not a workspace test target (or whose crate cannot be told apart)
//!   is reported under its executable name, never dropped.
//! - Parsing is pure and total: any text parses, unknown lines are ignored.

use crate::catalog::SuiteCatalog;
use crate::stamp::{Stamp, StampTarget};
use crate::tier::Tier;
use crate::timings::class_report;
use crate::workspace::{TargetId, TargetKind, Workspace};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// One test binary (or doc-test run) as a `cargo test` log reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedBinary {
    /// The source Cargo names (`src/lib.rs`, `tests/x.rs`), or `None` for a doc-test run.
    pub source: Option<String>,
    /// The executable's crate name (`sim_types` from `deps/sim_types-0123abcd`), or for a
    /// doc-test run the crate Cargo names.
    pub stem: String,
    /// The time libtest reports (`finished in 1.23s`), in milliseconds, if the block has a
    /// result line.
    pub millis: Option<u64>,
    /// `Some(true)` for `test result: ok`, `Some(false)` for `FAILED`, `None` without a result.
    pub passed: Option<bool>,
}

impl LoggedBinary {
    /// The workspace target this block ran. Two crates can have a test file of the same name
    /// (`runtime_catalog_tests`); then the target in `package`, the crate of the block before,
    /// is chosen, since Cargo runs one crate's binaries together. `None` if no target, or more
    /// than one and none in `package`, matches.
    #[must_use]
    pub fn target(&self, workspace: &Workspace, package: Option<&str>) -> Option<TargetId> {
        let source = self.source.as_deref()?;
        let suffix = format!("/{source}");
        let found: Vec<&TargetId> = workspace
            .test_targets()
            .filter(|target| {
                (target.src_path == source || target.src_path.ends_with(&suffix))
                    && crate_name(&target.id) == self.stem
            })
            .map(|target| &target.id)
            .collect();
        match found.as_slice() {
            [only] => Some((*only).clone()),
            several => several
                .iter()
                .find(|id| Some(id.package.as_str()) == package)
                .map(|id| (*id).clone()),
        }
    }

    /// The name the report shows: the target, `<crate>/doc-tests`, or the executable's name.
    #[must_use]
    pub fn label(&self, target: Option<&TargetId>) -> String {
        match (&self.source, target) {
            (None, _) => format!("{}/doc-tests", self.stem),
            (Some(_), Some(id)) => id.to_string(),
            (Some(source), None) => format!("{} ({source})", self.stem),
        }
    }
}

/// The target of every block of `binaries` (in log order), each resolved with the crate of the
/// last resolved block before it (see [`LoggedBinary::target`]).
#[must_use]
pub fn resolve_targets(binaries: &[LoggedBinary], workspace: &Workspace) -> Vec<Option<TargetId>> {
    let mut package: Option<String> = None;
    binaries
        .iter()
        .map(|binary| {
            let target = binary.target(workspace, package.as_deref());
            if let Some(id) = &target {
                package = Some(id.package.clone());
            }
            target
        })
        .collect()
}

/// The crate name rustc gives a target's executable: the package or target name with `-` as `_`.
fn crate_name(id: &TargetId) -> String {
    match &id.kind {
        TargetKind::Lib => id.package.replace('-', "_"),
        TargetKind::Bin(name) | TargetKind::Test(name) => name.replace('-', "_"),
    }
}

/// The crate name in an executable path: `…/deps/foo_tests-0123abcd(.exe)` gives `foo_tests`.
fn executable_stem(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let file = file.strip_suffix(".exe").unwrap_or(file);
    file.rsplit_once('-')
        .map_or(file, |(stem, _hash)| stem)
        .to_string()
}

/// Parses a `Running …` line's rest: `unittests src/lib.rs (…/deps/x-hash)` or
/// `tests/x.rs (…/deps/x-hash)`.
fn running(rest: &str) -> Option<LoggedBinary> {
    let rest = rest.strip_prefix("unittests ").unwrap_or(rest);
    let open = rest.rfind(" (")?;
    let source = rest[..open].trim().to_string();
    let path = rest[open + 2..].trim_end().strip_suffix(')')?;
    Some(LoggedBinary {
        source: Some(source),
        stem: executable_stem(path),
        millis: None,
        passed: None,
    })
}

/// Parses the `finished in 1.23s` of a `test result:` line, in whole milliseconds (further
/// digits are cut off, not rounded).
fn finished_millis(line: &str) -> Option<u64> {
    let text = line
        .rsplit_once("finished in ")?
        .1
        .trim()
        .strip_suffix('s')?;
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || !(fraction.is_empty() || digits(fraction)) {
        return None;
    }
    let mut millis: String = fraction.chars().chain("000".chars()).take(3).collect();
    millis.insert_str(0, whole);
    millis.parse().ok()
}

/// Every test binary and doc-test run `log` reports, in log order.
#[must_use]
pub fn parse_test_log(log: &str) -> Vec<LoggedBinary> {
    let mut binaries: Vec<LoggedBinary> = Vec::new();
    for line in log.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Running ") {
            if let Some(binary) = running(rest) {
                binaries.push(binary);
            }
        } else if let Some(krate) = trimmed.strip_prefix("Doc-tests ") {
            binaries.push(LoggedBinary {
                source: None,
                stem: krate.trim().to_string(),
                millis: None,
                passed: None,
            });
        } else if let Some(result) = trimmed.strip_prefix("test result: ") {
            if let Some(current) = binaries.last_mut() {
                current.passed = Some(result.starts_with("ok"));
                current.millis = finished_millis(result);
            }
        }
    }
    binaries
}

/// The targets `log` reports as failing: blocks with a `FAILED` result, and every target Cargo
/// names in `error: test failed, to rerun pass -p <crate> --lib|--bin <n>|--test <n>` (which
/// also covers a binary that crashed before printing a result).
#[must_use]
pub fn failing_targets(log: &str, workspace: &Workspace) -> BTreeSet<TargetId> {
    let binaries = parse_test_log(log);
    let mut failing: BTreeSet<TargetId> = binaries
        .iter()
        .zip(resolve_targets(&binaries, workspace))
        .filter(|(binary, _)| binary.passed == Some(false))
        .filter_map(|(_, target)| target)
        .collect();
    for line in log.lines() {
        let Some((_, rest)) = line.split_once("to rerun pass `") else {
            continue;
        };
        let words: Vec<&str> = rest.trim_end_matches('`').split_whitespace().collect();
        let id = match words.as_slice() {
            ["-p", package, "--lib", ..] => Some(TargetId::lib(package)),
            ["-p", package, "--bin", name, ..] => Some(TargetId::bin(package, name)),
            ["-p", package, "--test", name, ..] => Some(TargetId::test(package, name)),
            _ => None,
        };
        if let Some(id) = id {
            failing.insert(id);
        }
    }
    failing
}

/// Seconds with one decimal.
fn seconds(millis: u64) -> String {
    format!(
        "{:.1}",
        std::time::Duration::from_millis(millis).as_secs_f64()
    )
}

/// The Markdown report of `cargo gate timings --from-log`: totals per class, every binary by
/// time, and the targets whose time in this one run contradicts their class (blueprint §4.4).
#[must_use]
pub fn render_log_timings(
    binaries: &[LoggedBinary],
    workspace: &Workspace,
    catalog: &SuiteCatalog,
) -> String {
    let mut rows: Vec<(u64, String, String, &'static str)> = Vec::new();
    let mut per_class: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let mut stamp_targets = Vec::new();
    for (binary, target) in binaries.iter().zip(resolve_targets(binaries, workspace)) {
        let millis = binary.millis.unwrap_or(0);
        let class = match (&binary.source, &target) {
            (None, _) => "doc".to_string(),
            (Some(_), Some(id)) => catalog.class_of(id).name().to_string(),
            (Some(_), None) => "unknown".to_string(),
        };
        let result = match binary.passed {
            Some(true) => "ok",
            Some(false) => "FAILED",
            None => "no result",
        };
        let entry = per_class.entry(class.clone()).or_default();
        entry.0 += 1;
        entry.1 += millis;
        if let (Some(id), Some(ms)) = (&target, binary.millis) {
            stamp_targets.push(StampTarget {
                target: id.to_string(),
                class: catalog.class_of(id),
                wall_ms: ms,
                cpu_ms: None,
                passed: binary.passed == Some(true),
            });
        }
        rows.push((millis, binary.label(target.as_ref()), class, result));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let total: u64 = rows.iter().map(|row| row.0).sum();
    let mut out = String::from("## Test timings\n\n");
    let _ = writeln!(
        out,
        "`cargo gate timings --from-log`: {} test runs, {} s of test run time (libtest's \
         `finished in`; build and process start excluded).\n",
        rows.len(),
        seconds(total)
    );
    out.push_str("| Class | Runs | Seconds |\n|---|---|---|\n");
    for (class, (count, millis)) in &per_class {
        let _ = writeln!(out, "| {class} | {count} | {} |", seconds(*millis));
    }
    out.push_str("\n| Target | Class | Seconds | Result |\n|---|---|---|---|\n");
    for (millis, label, class, result) in &rows {
        let _ = writeln!(
            out,
            "| `{label}` | {class} | {} | {result} |",
            seconds(*millis)
        );
    }
    out.push_str(&render_contradictions(stamp_targets));
    out
}

/// The class findings of one logged run, as a Markdown section.
fn render_contradictions(targets: Vec<StampTarget>) -> String {
    let run = Stamp {
        tier: Tier::Full,
        tree: String::new(),
        commit: String::new(),
        base: None,
        host: String::new(),
        finished_unix_s: 0,
        targets,
        commands: Vec::new(),
        flakes: Vec::new(),
        retries: Vec::new(),
        reused: None,
    };
    let findings = class_report(&[run]);
    let mut out = String::from(
        "\n### Classes this run contradicts\n\nOne run only: `fast` above 30 s, `slow` under 10 s. \
         The catalog changes on the median of ten runs (`cargo gate timings` on the VM's stamps).\n\n",
    );
    if findings.is_empty() {
        out.push_str("None.\n");
    }
    for finding in findings {
        let _ = writeln!(
            out,
            "- `{}`: {} at {} s",
            finding.target,
            finding.class.name(),
            seconds(finding.median_ms)
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{Package, Target};

    const LOG: &str = "\
   Compiling x v0.1.0
     Running unittests src/lib.rs (target/debug/deps/sim_core-0123abcd)

running 2 tests
test a ... ok
test b ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s

     Running tests/gpu_tests.rs (/abs/target/debug/deps/gpu_tests-99aa)

running 1 test
child said:
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
test parent ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 45.25s

error: test failed, to rerun pass `-p client --test gpu_tests`
     Running tests/crash.rs (target\\debug\\deps\\crash-77.exe)
error: test failed, to rerun pass `-p client --test crash`
   Doc-tests sim_core

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
";

    #[test]
    fn blocks_take_their_last_result_and_executable_names_lose_their_hash() {
        let binaries = parse_test_log(LOG);
        assert_eq!(
            binaries,
            vec![
                LoggedBinary {
                    source: Some("src/lib.rs".into()),
                    stem: "sim_core".into(),
                    millis: Some(520),
                    passed: Some(true),
                },
                LoggedBinary {
                    source: Some("tests/gpu_tests.rs".into()),
                    stem: "gpu_tests".into(),
                    millis: Some(45_250),
                    passed: Some(false),
                },
                LoggedBinary {
                    source: Some("tests/crash.rs".into()),
                    stem: "crash".into(),
                    millis: None,
                    passed: None,
                },
                LoggedBinary {
                    source: None,
                    stem: "sim_core".into(),
                    millis: Some(0),
                    passed: Some(true),
                },
            ]
        );
    }

    #[test]
    fn unparsable_times_and_stray_lines_are_ignored() {
        assert_eq!(finished_millis("ok. finished in soon"), None);
        assert_eq!(finished_millis("ok. finished in 2.0009s"), Some(2_000));
        assert_eq!(finished_millis("ok. finished in 12s"), Some(12_000));
        assert_eq!(finished_millis("ok. finished in -1.5s"), None);
        assert_eq!(
            parse_test_log("test result: ok. finished in 1s\nnoise"),
            Vec::new()
        );
        assert_eq!(executable_stem("deps/a-b_tests-0f"), "a-b_tests");
    }

    #[test]
    fn rerun_hints_name_failing_targets_even_without_a_result() {
        let workspace = Workspace::default();
        let failing = failing_targets(LOG, &workspace);
        assert_eq!(
            failing,
            BTreeSet::from([
                TargetId::test("client", "gpu_tests"),
                TargetId::test("client", "crash"),
            ])
        );
    }

    fn package(name: &str, targets: &[(TargetId, &str)]) -> (String, Package) {
        let package = Package {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            dir: format!("crates/{name}"),
            dependencies: BTreeMap::new(),
            targets: targets
                .iter()
                .map(|(id, src)| Target {
                    id: id.clone(),
                    src_path: format!("crates/{name}/{src}"),
                })
                .collect(),
        };
        (name.to_string(), package)
    }

    #[test]
    fn blocks_resolve_to_targets_by_source_and_crate_name() -> Result<(), crate::GateError> {
        let workspace = Workspace {
            packages: BTreeMap::from([
                package("sim_core", &[(TargetId::lib("sim_core"), "src/lib.rs")]),
                package(
                    "client",
                    &[
                        (TargetId::lib("client"), "src/lib.rs"),
                        (TargetId::test("client", "gpu_tests"), "tests/gpu_tests.rs"),
                    ],
                ),
            ]),
            ..Workspace::default()
        };
        let binaries = parse_test_log(LOG);
        let targets = resolve_targets(&binaries, &workspace);
        let labels: Vec<String> = binaries
            .iter()
            .zip(&targets)
            .map(|(b, t)| b.label(t.as_ref()))
            .collect();
        assert_eq!(
            labels,
            [
                "sim_core/lib",
                "client/gpu_tests",
                "crash (tests/crash.rs)",
                "sim_core/doc-tests"
            ]
        );
        let catalog = SuiteCatalog::parse(
            "SuiteCatalog(global: [], docs: [], always: [], targets: {\"client/gpu_tests\": \
             (class: Gpu, triggers: [])}, rules: [], downstream: {}, commands: [])",
            std::path::Path::new("suites.ron"),
        )?;
        let report = render_log_timings(&binaries, &workspace, &catalog);
        assert!(
            report.contains("| `client/gpu_tests` | gpu | 45.2 | FAILED |"),
            "{report}"
        );
        assert!(report.contains("| `crash (tests/crash.rs)` | unknown | 0.0 | no result |"));
        assert!(report.contains("| gpu | 1 | 45.2 |"), "{report}");
        assert!(
            report.contains("None."),
            "a 0.5 s fast target contradicts nothing: {report}"
        );
        Ok(())
    }

    #[test]
    fn a_test_name_two_crates_share_resolves_to_the_crate_running_before_it() {
        let workspace = Workspace {
            packages: BTreeMap::from([
                package(
                    "asset_baker",
                    &[
                        (TargetId::lib("asset_baker"), "src/lib.rs"),
                        (
                            TargetId::test("asset_baker", "dup_tests"),
                            "tests/dup_tests.rs",
                        ),
                    ],
                ),
                package(
                    "sim_core",
                    &[
                        (TargetId::lib("sim_core"), "src/lib.rs"),
                        (
                            TargetId::test("sim_core", "dup_tests"),
                            "tests/dup_tests.rs",
                        ),
                    ],
                ),
            ]),
            ..Workspace::default()
        };
        let log = "\
     Running unittests src/lib.rs (deps/asset_baker-01)
     Running tests/dup_tests.rs (deps/dup_tests-02)
     Running unittests src/lib.rs (deps/sim_core-03)
     Running tests/dup_tests.rs (deps/dup_tests-04)
";
        let binaries = parse_test_log(log);
        assert_eq!(
            resolve_targets(&binaries, &workspace),
            [
                Some(TargetId::lib("asset_baker")),
                Some(TargetId::test("asset_baker", "dup_tests")),
                Some(TargetId::lib("sim_core")),
                Some(TargetId::test("sim_core", "dup_tests")),
            ]
        );
        assert_eq!(binaries[1].target(&workspace, None), None);
    }
}
