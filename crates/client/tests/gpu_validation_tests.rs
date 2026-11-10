//! Behavioural tests of the Vulkan validation gate in `support/gpu_run.rs` (remediation Q-7).
//!
//! # Invariants Verified
//! - The parser extracts severity and message ID from real `wgpu_hal` log lines.
//! - The gate fails on an unlisted ID, a missing layer, or an `ERROR` from another subsystem.
//! - Real validation output reaches the gate: best-practices checks produce findings in the same
//!   run path as the clean runs. Without this control a broken capture would look like a clean run.
//! - Live battle mode, which builds a different pipeline set from the lab, is clean.
//! - Driver selection (`docs/plans/2026-11-03-gpu-test-harness.md`, decision D1) picks `radv` only
//!   with every host prerequisite, and a forced driver never falls back.
//! - The GPU lock (decision D2) is held by a child exactly while it runs, a busy lock times out
//!   without starting the child, and a lock held by an ancestor fails at once. These tests lock a
//!   private file and run on every host, CI included.
//! - `radv` never lowers terrain anisotropy (decision D7), and every run proves the adapter it
//!   rendered on (decision D6).

#![allow(clippy::expect_used, clippy::panic)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use gpu_run::{
    AdapterEvidence, AllowedValidationId, DriverRequest, DriverSelectionError, FindingSeverity,
    GpuCommand, GpuDriver, GpuLock, GpuLockError, HostGpu, MissingPrerequisite, RenderNode,
    ValidationReport, VALIDATION_ALLOWLIST,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

/// Stderr excerpt in the exact form produced by the real client, including ANSI colours.
const ACTIVE_HEADER: &str = "\u{1b}[2m2026-11-01T22:32:19.649733Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \
\u{1b}[2mwgpu_hal::vulkan::instance\u{1b}[0m\u{1b}[2m:\u{1b}[0m GENERAL [Loader Message (0x0)]\n\
\tInsert instance layer \"VK_LAYER_KHRONOS_validation\" (/usr/lib/libVkLayer_khronos_validation.so)\n";

/// A real finding as logged at `WARN`, with the `objects:` record that follows it.
const WARNING_FINDING: &str = "\u{1b}[2m2026-11-01T22:33:29.407577Z\u{1b}[0m \u{1b}[33m WARN\u{1b}[0m \
\u{1b}[2mwgpu_hal::vulkan::instance\u{1b}[0m\u{1b}[2m:\u{1b}[0m \
PERFORMANCE [BestPractices-vkCreateRenderPass-image-requires-memory (0x3c9ca6b2)]\n\
\tvkCreateRenderPass(): Attachment 0 uses LOAD_OP_CLEAR\n\
2026-11-01T22:33:29.407580Z  WARN wgpu_hal::vulkan::instance: \tobjects: (type: DEVICE, hndl: 0x1, name: ?)\n";

/// Allowlist for tests of the matching logic, independent of the real one.
const TEST_ALLOWLIST: &[AllowedValidationId] = &[AllowedValidationId {
    id: "BestPractices-vkCreateRenderPass-image-requires-memory",
    reason: "fixture entry for the allowlist matching test",
}];

#[test]
fn clean_output_with_active_layer_has_no_violations() {
    let report = ValidationReport::parse(ACTIVE_HEADER);
    assert!(report.layer_active);
    assert_eq!(
        report.violations(VALIDATION_ALLOWLIST),
        Vec::<String>::new()
    );
}

#[test]
fn stderr_without_the_layer_fails_even_when_it_has_no_findings() {
    let report = ValidationReport::parse("2026-11-01T22:32:19.6Z  INFO client: started\n");
    assert!(!report.layer_active);
    let violations = report.violations(VALIDATION_ALLOWLIST);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("was not inserted"), "{violations:?}");
}

#[test]
fn warning_finding_is_parsed_once_and_detail_records_are_not_findings() {
    let report = ValidationReport::parse(&format!("{ACTIVE_HEADER}{WARNING_FINDING}"));
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!(finding.severity, FindingSeverity::Warning);
    assert_eq!(
        finding.id,
        "BestPractices-vkCreateRenderPass-image-requires-memory"
    );
    assert_eq!(
        finding.message,
        "vkCreateRenderPass(): Attachment 0 uses LOAD_OP_CLEAR"
    );
}

#[test]
fn error_finding_with_validation_id_is_an_unlisted_violation() {
    let stderr = format!(
        "{ACTIVE_HEADER}2026-11-01T22:40:00.000000Z ERROR wgpu_hal::vulkan::instance: \
         VALIDATION [VUID-vkCmdDraw-None-08600 (0x4768cf39)]\n\tvkCmdDraw(): descriptor not bound\n"
    );
    let report = ValidationReport::parse(&stderr);
    assert_eq!(report.findings[0].severity, FindingSeverity::Error);
    assert_eq!(report.findings[0].id, "VUID-vkCmdDraw-None-08600");
    let violations = report.violations(VALIDATION_ALLOWLIST);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("VUID-vkCmdDraw-None-08600"));
}

#[test]
fn unstructured_wgpu_hal_warning_is_not_dropped() {
    let stderr = format!(
        "{ACTIVE_HEADER}2026-11-01T22:40:00.000000Z  WARN wgpu_hal::vulkan::instance: \
         InstanceFlags::VALIDATION requested, but unable to find layer: VK_LAYER_KHRONOS_validation\n"
    );
    let report = ValidationReport::parse(&stderr);
    assert_eq!(report.violations(VALIDATION_ALLOWLIST).len(), 1);
}

#[test]
fn allowlist_matches_the_exact_id_only() {
    let report = ValidationReport::parse(&format!("{ACTIVE_HEADER}{WARNING_FINDING}"));
    assert_eq!(report.unlisted(TEST_ALLOWLIST).count(), 0);
    assert_eq!(report.violations(TEST_ALLOWLIST), Vec::<String>::new());

    let near_miss = [AllowedValidationId {
        id: "BestPractices-vkCreateRenderPass",
        reason: "a prefix must not match",
    }];
    assert_eq!(report.unlisted(&near_miss).count(), 1);
}

#[test]
fn error_from_another_subsystem_fails_the_run() {
    let stderr = format!(
        "{ACTIVE_HEADER}2026-11-01T22:40:00.000000Z ERROR bevy_render::render_resource::pipeline_cache: \
         failed to process shader\n"
    );
    let report = ValidationReport::parse(&stderr);
    assert_eq!(report.findings, Vec::new());
    assert_eq!(report.violations(VALIDATION_ALLOWLIST).len(), 1);
}

#[test]
fn warning_from_another_subsystem_is_not_a_validation_finding() {
    let stderr = format!(
        "{ACTIVE_HEADER}2026-11-01T22:32:19.6Z  WARN bevy_render::renderer: software adapter\n"
    );
    assert_eq!(
        ValidationReport::parse(&stderr).violations(VALIDATION_ALLOWLIST),
        Vec::<String>::new()
    );
}

#[test]
fn allowlist_entries_carry_a_reason() {
    for entry in VALIDATION_ALLOWLIST {
        assert_ne!(entry.id, "", "allowlist IDs must be exact");
        assert_ne!(
            entry.reason.trim(),
            "",
            "{} needs a documented reason",
            entry.id
        );
    }
}

/// The notice is tolerated by its exact text, so losing a different extension still fails.
#[test]
fn only_the_listed_missing_extension_is_tolerated() {
    let missing = |extension: &str| {
        ValidationReport::parse(&format!(
            "{ACTIVE_HEADER}2026-11-02T22:54:28.319598Z  WARN wgpu_hal::vulkan::instance: \
             Unable to find extension: {extension}\n"
        ))
        .violations(VALIDATION_ALLOWLIST)
    };
    assert_eq!(missing("VK_EXT_swapchain_colorspace"), Vec::<String>::new());
    let violations = missing("VK_EXT_debug_utils");
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("VK_EXT_debug_utils"));
}

/// Tolerating the notice must not hide a real error reported in the same run.
#[test]
fn listed_notice_does_not_hide_a_real_validation_error() {
    let stderr = format!(
        "{ACTIVE_HEADER}2026-11-02T22:54:28.319598Z  WARN wgpu_hal::vulkan::instance: \
         Unable to find extension: VK_EXT_swapchain_colorspace\n\
         2026-11-02T22:54:29.000000Z ERROR wgpu_hal::vulkan::instance: \
         VALIDATION [VUID-vkCmdDraw-None-08600 (0x4768cf39)]\n\tvkCmdDraw(): descriptor not bound\n"
    );
    let violations = ValidationReport::parse(&stderr).violations(VALIDATION_ALLOWLIST);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("VUID-vkCmdDraw-None-08600"));
}

/// Ubuntu 24.04's `xvfb-run` runs the command as `"$@" 2>&1`, so all logs arrive on stdout.
#[test]
fn log_delivered_on_stdout_is_validated_like_stderr() {
    let log = format!("{ACTIVE_HEADER}{WARNING_FINDING}");
    let from_stderr = ValidationReport::parse(&gpu_run::merge_streams(b"", log.as_bytes()));
    let from_stdout = ValidationReport::parse(&gpu_run::merge_streams(log.as_bytes(), b""));
    assert!(from_stdout.layer_active);
    assert_eq!(from_stdout.findings.len(), 1);
    assert_eq!(from_stdout, from_stderr);
}

#[test]
fn log_split_across_both_streams_keeps_every_record() {
    let log = gpu_run::merge_streams(
        WARNING_FINDING.as_bytes(),
        ACTIVE_HEADER.trim_end().as_bytes(),
    );
    let report = ValidationReport::parse(&log);
    assert!(report.layer_active);
    assert_eq!(report.findings.len(), 1);
}

#[test]
fn mesa_release_is_read_from_the_vulkaninfo_summary() {
    let summary = "\tdeviceName         = llvmpipe (LLVM 15.0.6, 256 bits)\n\
                   \tdriverInfo         = Mesa 22.3.6 (LLVM 15.0.6)\n";
    assert_eq!(gpu_run::mesa_release(summary), Some((22, 3)));
    assert_eq!(
        gpu_run::mesa_release("driverInfo = Mesa 25.2.8-0ubuntu0.24.04.4 (LLVM 20.1.2)"),
        Some((25, 2))
    );
    assert_eq!(gpu_run::mesa_release("driverInfo = 535.183.01"), None);
    assert_eq!(gpu_run::mesa_release("no driver line"), None);
}

/// The production sampler must stay in use on every driver that can run it.
#[test]
fn only_mesa_before_the_sampler_rewrite_lowers_anisotropy() {
    let stalls = |version: &str| {
        gpu_run::driver_stalls_on_anisotropic_hex_tiling(&format!(
            "driverInfo = Mesa {version} (LLVM 15.0.6)"
        ))
    };
    assert!(stalls("22.3.6"));
    assert!(stalls("25.0.7"));
    assert!(!stalls("25.1.0"));
    assert!(!stalls("25.2.8"));
    assert!(!stalls("26.0.1"));
    assert!(!gpu_run::driver_stalls_on_anisotropic_hex_tiling(
        "driverInfo = 535.183.01"
    ));
}

#[test]
fn best_practices_ids_are_recognised_in_both_layer_naming_forms() {
    assert!(gpu_run::is_best_practices_id(
        "BestPractices-vkCreateRenderPass-image-requires-memory"
    ));
    assert!(gpu_run::is_best_practices_id(
        "UNASSIGNED-BestPractices-vkCreateRenderPass-image-requires-memory"
    ));
    for other in [
        "UNASSIGNED-khronos-validation-createinstance-status-message",
        "VUID-vkCmdDraw-None-08600",
        "Unable to find extension: VK_EXT_swapchain_colorspace",
        "NotBestPractices-vkCreateRenderPass",
        "UNASSIGNED-UNASSIGNED-BestPractices-vkCreateRenderPass",
    ] {
        assert!(!gpu_run::is_best_practices_id(other), "{other}");
    }
}

/// Positive control: the gate must fail on findings the real layer emits in a real run.
///
/// Best-practices checks are off in normal runs and are known to report on Bevy's render passes,
/// so a run with them on has to produce findings. If this test fails, the clean results from every
/// other GPU test are unproven.
#[test]
fn real_layer_findings_reach_the_gate() {
    if gpu_run::gpu_tests_opted_out() {
        return;
    }
    let mut command = gpu_run::virtual_gpu_command(Path::new(env!("CARGO_BIN_EXE_client")));
    command.args(["--lab", "--max-frames", "30"]);
    command.env(
        "VK_LAYER_ENABLES",
        "VK_VALIDATION_FEATURE_ENABLE_BEST_PRACTICES_EXT",
    );
    let (_, report) = command.run_validated("control run, client --lab with best practices");
    assert!(report.layer_active, "control run must load the layer");
    assert!(
        report
            .unlisted(VALIDATION_ALLOWLIST)
            .any(|finding| gpu_run::is_best_practices_id(&finding.id)),
        "best-practices findings did not reach the gate: {:?}",
        report.findings
    );
}

#[test]
fn live_battle_passes_vulkan_validation() {
    if gpu_run::gpu_tests_opted_out() {
        return;
    }
    let mut command = gpu_run::virtual_gpu_command(Path::new(env!("CARGO_BIN_EXE_client")));
    command.args(["--battle", "--max-frames", "60"]);
    command.assert_passes_validation("client --battle --max-frames 60");
}

/// The command line a player uses: `client --skirmish`, with the HUD and fog of the `Player`
/// profile, through the real binary and Vulkan validation.
#[test]
fn skirmish_passes_vulkan_validation() {
    if gpu_run::gpu_tests_opted_out() {
        return;
    }
    let mut command = gpu_run::virtual_gpu_command(Path::new(env!("CARGO_BIN_EXE_client")));
    command.args(["--skirmish", "--max-frames", "60"]);
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("client --skirmish --max-frames 60");
}

// ---------------------------------------------------------------------------------------------
// Driver selection, the GPU lock and adapter evidence (plan 2026-11-03-gpu-test-harness)
// ---------------------------------------------------------------------------------------------

/// A host with the given prerequisites, with the paths this server uses.
fn host(radv_manifest: bool, render_node: RenderNode, lock_tool: bool, lavapipe: bool) -> HostGpu {
    HostGpu {
        radv_manifest: radv_manifest
            .then(|| PathBuf::from("/usr/share/vulkan/icd.d/radeon_icd.json")),
        render_node,
        lock_tool: lock_tool.then(|| PathBuf::from("/usr/local/bin/with-gpu-lock")),
        lavapipe_manifest: lavapipe.then(|| PathBuf::from("/usr/share/vulkan/icd.d/lvp_icd.json")),
    }
}

/// The render node of this server, usable.
fn usable_node() -> RenderNode {
    RenderNode::Usable(PathBuf::from("/dev/dri/renderD128"))
}

/// R1: `auto` picks `radv` only with all three prerequisites, a forced driver never falls back,
/// and every missing prerequisite is named.
#[test]
fn driver_selection_matrix() {
    let full = host(true, usable_node(), true, true);
    let selection = gpu_run::select_driver(DriverRequest::Auto, &full).expect("radv host");
    assert_eq!(selection.driver, GpuDriver::Radv);
    assert_eq!(
        selection.manifest,
        PathBuf::from("/usr/share/vulkan/icd.d/radeon_icd.json")
    );
    assert_eq!(
        selection.to_string(),
        "driver radv (auto: radeon_icd.json, /dev/dri/renderD128, with-gpu-lock on PATH)"
    );

    let inaccessible = RenderNode::Inaccessible(PathBuf::from("/dev/dri/renderD128"));
    let lacking_one = [
        (
            host(false, usable_node(), true, true),
            MissingPrerequisite::RadvManifest,
        ),
        (
            host(true, RenderNode::Missing, true, true),
            MissingPrerequisite::RenderNode,
        ),
        (
            host(true, inaccessible, true, true),
            MissingPrerequisite::RenderNodeAccess(PathBuf::from("/dev/dri/renderD128")),
        ),
        (
            host(true, usable_node(), false, true),
            MissingPrerequisite::LockTool,
        ),
    ];
    for (host, missing) in &lacking_one {
        let auto = gpu_run::select_driver(DriverRequest::Auto, host).expect("lavapipe fallback");
        assert_eq!(auto.driver, GpuDriver::Lavapipe, "{host:?}");
        assert!(auto.reason.contains(&missing.to_string()), "{auto}");
        assert_eq!(
            gpu_run::select_driver(DriverRequest::Radv, host),
            Err(DriverSelectionError::RadvUnavailable(vec![missing.clone()])),
            "forced radv must fail, not fall back: {host:?}"
        );
    }

    let bare = host(false, RenderNode::Missing, false, true);
    let Err(error) = gpu_run::select_driver(DriverRequest::Radv, &bare) else {
        panic!("forced radv on a host without a GPU must fail");
    };
    let message = error.to_string();
    for named in [
        "radv manifest",
        "render node",
        "with-gpu-lock is not on PATH",
    ] {
        assert!(message.contains(named), "{message} must name {named}");
    }
    assert!(message.contains("never falls back"), "{message}");

    let forced = gpu_run::select_driver(DriverRequest::Lavapipe, &full).expect("lavapipe");
    assert_eq!(forced.driver, GpuDriver::Lavapipe);
    assert_eq!(forced.to_string(), "driver lavapipe (forced)");
    assert_eq!(
        gpu_run::select_driver(
            DriverRequest::Lavapipe,
            &host(true, usable_node(), true, false)
        ),
        Err(DriverSelectionError::LavapipeMissing)
    );
    assert!(matches!(
        gpu_run::select_driver(
            DriverRequest::Auto,
            &host(false, RenderNode::Missing, false, false)
        ),
        Err(DriverSelectionError::NoDriver(missing)) if missing.len() == 3
    ));

    assert_eq!(DriverRequest::parse(None), Ok(DriverRequest::Auto));
    assert_eq!(DriverRequest::parse(Some("auto")), Ok(DriverRequest::Auto));
    assert_eq!(DriverRequest::parse(Some("radv")), Ok(DriverRequest::Radv));
    assert_eq!(
        DriverRequest::parse(Some("lavapipe")),
        Ok(DriverRequest::Lavapipe)
    );
    for invalid in ["", "RADV", "llvmpipe", "nvidia"] {
        let error = DriverRequest::parse(Some(invalid)).expect_err(invalid);
        assert!(
            error.to_string().contains("use radv, lavapipe or auto"),
            "{error}"
        );
    }
}

/// R2: a child run through `GpuCommand` sees its lock held, and the lock is free once `run`
/// returns. The child asks real `flock(1)`, so this is the protocol `with-gpu-lock` speaks.
///
/// The check runs in a single-threaded re-execution of this binary, [`lock_release_child`].
/// In this process, other test threads spawn children concurrently, and a child spawned while the
/// guard is open inherits a duplicate of its descriptor until that child calls `exec`, which
/// closes it (`O_CLOEXEC`). An `flock -n` made in that window finds the lock held, although the
/// harness has already released it. A process with no other spawning thread removes the window,
/// so the assertion stays exact: free at once, not eventually.
#[test]
fn a_child_holds_the_gpu_lock_only_while_it_runs() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
            "lock_release_child",
        ])
        .output()?;
    let log = gpu_run::captured_log(&output);
    assert!(
        output.status.success(),
        "the single-threaded lock release child failed:\n{}",
        gpu_run::tail(&log)
    );
    assert!(log.contains("1 passed"), "the child test must run:\n{log}");
    Ok(())
}

/// The child of [`a_child_holds_the_gpu_lock_only_while_it_runs`], run alone in its own process.
#[test]
#[ignore = "launched by a_child_holds_the_gpu_lock_only_while_it_runs with one test thread"]
fn lock_release_child() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let lock = directory.path().join("gpu.lock");
    let out = directory.path().join("out");
    let mut command = GpuCommand::lock_probe(Path::new("/bin/sh"), &lock, Duration::from_secs(5));
    command
        .args(["-c", r#"flock -n "$LOCK" true; echo $? > "$OUT""#])
        .env("LOCK", &lock)
        .env("OUT", &out);
    let run = command.run("lock probe");
    assert_eq!(
        std::fs::read_to_string(&out)?.trim(),
        "1",
        "flock -n inside the child must find the lock held"
    );
    assert!(run.lock_wait.is_some(), "a locked run reports its wait");
    let after = Command::new("flock")
        .arg("-n")
        .arg(&lock)
        .arg("true")
        .status()?;
    assert!(after.success(), "the lock must be free once run returns");
    Ok(())
}

/// R3: with the lock held elsewhere, `try_run` times out after its wait and never starts the
/// child.
#[test]
fn a_busy_lock_times_out_without_starting_the_child() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let lock = directory.path().join("gpu.lock");
    let marker = directory.path().join("started");
    let holder = std::fs::File::create(&lock)?;
    holder.lock()?;
    let mut command = GpuCommand::lock_probe(Path::new("/bin/sh"), &lock, Duration::from_secs(1));
    command
        .args(["-c", r#"touch "$MARKER""#])
        .env("MARKER", &marker);
    let started = Instant::now();
    let result = command.try_run("busy lock probe");
    let elapsed = started.elapsed();
    let Err(GpuLockError::TimedOut { waited, holder, .. }) = result else {
        panic!("a busy lock must time out, got {result:?}");
    };
    assert!(waited >= Duration::from_secs(1), "waited {waited:?}");
    assert!(elapsed < Duration::from_secs(10), "took {elapsed:?}");
    assert!(
        !marker.exists(),
        "the child must not start without the lock"
    );
    if let Some(holder) = holder {
        assert_eq!(holder.pid, std::process::id(), "this process holds it");
    }
    Ok(())
}

/// Variable naming the private lock file of [`ancestor_lock_child`].
const ANCESTOR_LOCK_VAR: &str = "SPACE_SCUFFLE_ANCESTOR_LOCK";

/// R4: a lock held by an ancestor fails at once instead of waiting out the wait limit. The test
/// re-executes itself under real `flock(1)`, the way `with-gpu-lock cargo test` would run.
#[test]
fn an_ancestor_holding_the_lock_fails_fast() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let lock = directory.path().join("gpu.lock");
    let output = Command::new("flock")
        .arg(&lock)
        .arg(std::env::current_exe()?)
        .args(["--ignored", "--exact", "--nocapture", "ancestor_lock_child"])
        .env(ANCESTOR_LOCK_VAR, &lock)
        .output()?;
    let log = gpu_run::captured_log(&output);
    assert!(
        output.status.success(),
        "the child under flock must see HeldByAncestor at once:\n{}",
        gpu_run::tail(&log)
    );
    assert!(log.contains("1 passed"), "the child test must run:\n{log}");
    Ok(())
}

/// The child of [`an_ancestor_holding_the_lock_fails_fast`], run under `flock <lock>`.
#[test]
#[ignore = "launched by an_ancestor_holding_the_lock_fails_fast under flock(1)"]
fn ancestor_lock_child() {
    let lock = std::env::var_os(ANCESTOR_LOCK_VAR).expect("the parent names the lock");
    let started = Instant::now();
    let result = GpuLock::acquire(Path::new(&lock), Duration::from_secs(60));
    let elapsed = started.elapsed();
    let Err(GpuLockError::HeldByAncestor { holder, .. }) = &result else {
        panic!("expected HeldByAncestor after {elapsed:?}, got {result:?}");
    };
    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
    assert!(holder.command_line.contains("flock"), "{holder}");
    let message = result.expect_err("checked above").to_string();
    assert!(message.contains("without with-gpu-lock"), "{message}");
}

/// The `/proc/locks` and `/proc/<pid>/stat` parsers behind the ancestor check, on fixture text.
#[test]
fn proc_locks_and_stat_lines_are_parsed() {
    let locks = "1: FLOCK  ADVISORY  WRITE 2533986 08:10:87952619 0 EOF\n\
                 1: -> FLOCK  ADVISORY  WRITE 2600000 08:10:87952619 0 EOF\n\
                 2: POSIX  ADVISORY  WRITE 2415563 00:1a:42 0 EOF\n\
                 3: FLOCK  ADVISORY  WRITE 2415564 00:1a:42 0 EOF\n";
    assert_eq!(
        gpu_run::flock_holder_pid(locks, 8, 16, 87_952_619),
        Some(2_533_986)
    );
    assert_eq!(gpu_run::flock_holder_pid(locks, 0, 26, 42), Some(2_415_564));
    assert_eq!(gpu_run::flock_holder_pid(locks, 8, 16, 1), None);
    assert_eq!(gpu_run::flock_holder_pid("garbage\n\n", 8, 16, 1), None);

    assert_eq!(
        gpu_run::stat_parent_pid("4242 (cargo) S 4100 4242 4100 0 -1"),
        Some(4100)
    );
    assert_eq!(
        gpu_run::stat_parent_pid("77 (odd) name (x)) R 12 77 12 0"),
        Some(12)
    );
    assert_eq!(gpu_run::stat_parent_pid("77 no parentheses"), None);

    let parents = |pid| match pid {
        30 => Some(20),
        20 => Some(10),
        10 => Some(1),
        1 => Some(0),
        _ => None,
    };
    assert_eq!(gpu_run::ancestor_chain(30, parents), [30, 20, 10, 1]);
    assert_eq!(gpu_run::ancestor_chain(5, Some).len(), 4_096);

    // 8:16 as glibc's makedev encodes it, and a major and minor beyond the old 8-bit fields.
    assert_eq!(gpu_run::split_device(0x0810), (8, 16));
    let wide = (0x1000_u64 << 32) | (0x234 << 8) | (0x0005_6700 << 12) | 0x89;
    assert_eq!(gpu_run::split_device(wide), (0x1234, 0x0005_6789));
}

/// R5: `radv` never lowers terrain anisotropy; `lavapipe` does only before Mesa 25.1 (Q0009).
#[test]
fn radv_never_lowers_terrain_anisotropy() {
    for release in [Some((22, 3)), Some((25, 0)), Some((25, 1)), None] {
        assert_eq!(gpu_run::anisotropy_override(GpuDriver::Radv, release), None);
    }
    assert_eq!(
        gpu_run::anisotropy_override(GpuDriver::Lavapipe, Some((25, 0))),
        Some("1")
    );
    assert_eq!(
        gpu_run::anisotropy_override(GpuDriver::Lavapipe, Some((22, 3))),
        Some("1")
    );
    assert_eq!(
        gpu_run::anisotropy_override(GpuDriver::Lavapipe, Some((25, 1))),
        None
    );
    assert_eq!(
        gpu_run::anisotropy_override(GpuDriver::Lavapipe, None),
        None
    );
}

/// Bevy's adapter line from a `radv` run on this server's RX 580, ANSI colours included (plan
/// section 3.1).
const RADV_ADAPTER_LINE: &str = "\u{1b}[2m2026-11-03T08:43:02.757205Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \
\u{1b}[2mbevy_render::renderer\u{1b}[0m\u{1b}[2m:\u{1b}[0m AdapterInfo { name: \"AMD Radeon RX 580 Series \
(RADV POLARIS10)\", vendor: 4098, device: 26591, device_type: DiscreteGpu, driver: \"radv\", \
driver_info: \"Mesa 25.0.7-2+deb13u1\", backend: Vulkan }\n";

/// Bevy's adapter line from a `lavapipe` run of `client --lab` on this server.
const LAVAPIPE_ADAPTER_LINE: &str = "\u{1b}[2m2026-11-03T09:51:52.365083Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \
\u{1b}[2mbevy_render::renderer\u{1b}[0m\u{1b}[2m:\u{1b}[0m AdapterInfo { name: \"llvmpipe (LLVM 19.1.7, 256 \
bits)\", vendor: 65541, device: 0, device_type: Cpu, driver: \"llvmpipe\", driver_info: \"Mesa \
25.0.7-2+deb13u1 (LLVM 19.1.7)\", backend: Vulkan }\n";

/// R6: both real adapter lines parse, and a validated run whose line is missing or names another
/// driver has a violation.
#[test]
fn adapter_lines_are_parsed_and_checked() {
    let radv = AdapterEvidence::parse(RADV_ADAPTER_LINE).expect("the radv line parses");
    assert_eq!(radv.name, "AMD Radeon RX 580 Series (RADV POLARIS10)");
    assert_eq!(radv.driver, "radv");
    assert_eq!(radv.driver_info, "Mesa 25.0.7-2+deb13u1");
    let lavapipe = AdapterEvidence::parse(LAVAPIPE_ADAPTER_LINE).expect("the lavapipe line parses");
    assert_eq!(lavapipe.driver, "llvmpipe");
    assert!(lavapipe.name.starts_with("llvmpipe"), "{lavapipe:?}");
    assert!(
        lavapipe.driver_info.starts_with("Mesa 25.0.7"),
        "{lavapipe:?}"
    );
    assert_eq!(AdapterEvidence::parse(ACTIVE_HEADER), None);

    let on_radv = ValidationReport::parse(&format!("{ACTIVE_HEADER}{RADV_ADAPTER_LINE}"));
    assert_eq!(on_radv.adapter.as_ref(), Some(&radv));
    assert_eq!(
        on_radv
            .clone()
            .expecting(GpuDriver::Radv)
            .violations(VALIDATION_ALLOWLIST),
        Vec::<String>::new()
    );
    let mismatched = on_radv
        .expecting(GpuDriver::Lavapipe)
        .violations(VALIDATION_ALLOWLIST);
    assert_eq!(mismatched.len(), 1, "{mismatched:?}");
    assert!(mismatched[0].contains("not on lavapipe"), "{mismatched:?}");

    let missing = ValidationReport::parse(ACTIVE_HEADER)
        .expecting(GpuDriver::Radv)
        .violations(VALIDATION_ALLOWLIST);
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(missing[0].contains("no AdapterInfo line"), "{missing:?}");
    assert_eq!(
        ValidationReport::parse(&format!("{ACTIVE_HEADER}{LAVAPIPE_ADAPTER_LINE}"))
            .expecting(GpuDriver::Lavapipe)
            .violations(VALIDATION_ALLOWLIST),
        Vec::<String>::new()
    );
}

/// R7: the driver the harness selected is the one that rendered. On this server with `auto` that
/// is `radv`; with `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` (CI) it is `llvmpipe`.
#[test]
fn the_selected_driver_is_the_one_that_rendered() {
    if gpu_run::gpu_tests_opted_out() {
        return;
    }
    let request = DriverRequest::parse(std::env::var(gpu_run::DRIVER_VAR).ok().as_deref())
        .expect("a valid driver request");
    let expected = match request {
        DriverRequest::Radv => GpuDriver::Radv,
        DriverRequest::Auto if HostGpu::probe().missing_radv_prerequisites().is_empty() => {
            GpuDriver::Radv
        }
        DriverRequest::Lavapipe | DriverRequest::Auto => GpuDriver::Lavapipe,
    };
    let mut command = gpu_run::virtual_gpu_command(Path::new(env!("CARGO_BIN_EXE_client")));
    command.args(["--lab", "--max-frames", "10"]);
    let run = command.run("client --lab --max-frames 10");
    let rendered = run
        .adapter
        .as_ref()
        .map_or("nothing", |adapter| adapter.driver.as_str());
    assert_eq!(
        rendered,
        expected.adapter_driver(),
        "expected {expected}, rendered on {rendered}"
    );
    assert_eq!(gpu_run::selected_driver().driver, expected);
}

/// R8: the host's `with-gpu-lock`, when present, locks the same file as the harness, so a change
/// to the host script fails the gate instead of splitting the protocol.
#[test]
fn the_host_lock_tool_uses_the_harness_lock_file() -> Result<(), Box<dyn std::error::Error>> {
    let Some(tool) = gpu_run::find_on_path(gpu_run::LOCK_TOOL) else {
        eprintln!(
            "{} is not on PATH; this host does not declare the lock protocol",
            gpu_run::LOCK_TOOL
        );
        return Ok(());
    };
    let script = std::fs::read_to_string(&tool)?;
    assert!(
        script.contains("flock"),
        "{} must use flock",
        tool.display()
    );
    assert!(
        script.contains(gpu_run::GPU_LOCK_PATH),
        "{} must lock {}",
        tool.display(),
        gpu_run::GPU_LOCK_PATH
    );
    Ok(())
}

/// A child's log timestamps are read in UTC with their fraction, and lines without one are not
/// records.
#[test]
fn log_record_times_are_read_from_tracing_timestamps() {
    let at = gpu_run::log_record_time(LAVAPIPE_ADAPTER_LINE).expect("a timestamped record");
    // 2026-11-03T09:51:52.365083Z: 20,760 days after 1970-01-01, then 35,512.365083 s.
    assert_eq!(
        at.duration_since(SystemTime::UNIX_EPOCH)
            .expect("after the epoch"),
        Duration::new(20_760 * 86_400 + 35_512, 365_083_000)
    );
    let plain = gpu_run::log_record_time("2000-02-29T00:00:01Z  INFO x: y")
        .expect("a record without a fraction");
    assert_eq!(
        plain
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("after the epoch"),
        Duration::from_secs(951_782_401)
    );
    assert_eq!(gpu_run::log_record_time("\tobjects: (type: DEVICE)"), None);
    assert_eq!(gpu_run::log_record_time("running 1 test"), None);
    assert_eq!(gpu_run::log_record_time("2026-13-01T00:00:00Z"), None);
}

/// A timeline splits the run at the first record, the adapter line, the window and the last
/// record, and a timeout names its slowest phase.
#[test]
fn a_child_timeline_names_its_slowest_phase() {
    let spawned = gpu_run::log_record_time("2026-11-10T00:50:47.000000Z").expect("spawn time");
    let log = "\u{1b}[2m2026-11-10T00:50:49.000000Z\u{1b}[0m INFO SystemInfo { os: \"Linux\" }\n\
               \tcontinued record without a timestamp\n\
               2026-11-10T00:50:58.000000Z INFO bevy_render::renderer: AdapterInfo { name: \"AMD\" }\n\
               2026-11-10T00:51:29.500000Z INFO bevy_winit::system: Creating new window \"x\"\n\
               2026-11-10T00:51:59.500000Z INFO client::media: Media unapproved\n";
    let timeline = gpu_run::ChildTimeline::read(log, spawned, Duration::from_secs(80));
    let expected = [
        ("launch", Duration::from_secs(2)),
        ("render device", Duration::from_secs(9)),
        ("app build", Duration::from_millis(31_500)),
        ("startup and frames", Duration::from_secs(30)),
        ("after the last log record", Duration::from_millis(7_500)),
    ];
    assert_eq!(timeline.phases, expected);
    assert_eq!(
        timeline.slowest(),
        Some(("app build", Duration::from_millis(31_500)))
    );
    assert_eq!(
        timeline.to_string(),
        "launch 2.0 s, render device 9.0 s, app build 31.5 s, startup and frames 30.0 s, \
         after the last log record 7.5 s"
    );

    // A child that logs nothing spends its whole run in the last phase.
    let silent = gpu_run::ChildTimeline::read("no records\n", spawned, Duration::from_secs(5));
    assert_eq!(
        silent.phases,
        [("after the last log record", Duration::from_secs(5))]
    );
}

/// The executable pre-read reads a file larger than one read, stops at the end of the mapped part
/// of an ELF executable, and fails on a missing file instead of starting a child that cannot run;
/// every run records it.
#[test]
fn the_executable_is_read_before_the_lock_and_recorded() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("program");
    std::fs::write(&file, vec![7_u8; 3 << 20])?;
    gpu_run::preload_executable(&file)?;
    let missing = gpu_run::preload_executable(&directory.path().join("absent"))
        .expect_err("a missing executable is an error");
    assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);

    // This test executable: the mapped part ends inside the file, before the debug sections.
    let exe = std::fs::File::open(std::env::current_exe()?)?;
    let size = exe.metadata()?.len();
    let read_at = |offset: u64, bytes: usize| {
        let mut buffer = vec![0_u8; bytes];
        std::os::unix::fs::FileExt::read_exact_at(&exe, &mut buffer, offset).ok()?;
        Some(buffer)
    };
    let header = read_at(0, 64).expect("a 64-byte header");
    let extent = gpu_run::loaded_extent(&header, read_at).expect("an ELF executable");
    assert!(
        extent > 1 << 20 && extent < size,
        "mapped part {extent} of {size} bytes"
    );
    // Not ELF, a 32-bit header, and a program header table past the end of the file.
    assert_eq!(gpu_run::loaded_extent(&[0_u8; 64], read_at), None);
    let mut elf32 = header.clone();
    elf32[4] = 1;
    assert_eq!(gpu_run::loaded_extent(&elf32, read_at), None);
    let mut beyond = header;
    beyond[32..40].copy_from_slice(&size.to_le_bytes());
    assert_eq!(gpu_run::loaded_extent(&beyond, read_at), None);

    let lock = directory.path().join("gpu.lock");
    let mut command = GpuCommand::lock_probe(Path::new("/bin/sh"), &lock, Duration::from_secs(5));
    command.args(["-c", "true"]);
    let run = command.run("pre-read probe");
    assert!(
        run.preload < Duration::from_secs(10),
        "reading /bin/sh took {:?}",
        run.preload
    );
    Ok(())
}
