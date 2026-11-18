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
//! - An offscreen child (plan `2026-11-17-gpu-test-readback`, D-7, T-5) starts with no display
//!   wrapper and none of the display variables, keeps the driver, the lock and the test-display
//!   flag, and is recorded as `display=offscreen`; a windowed child keeps its virtual display.

#![allow(clippy::expect_used, clippy::panic)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/shader_ir.rs"]
mod shader_ir;

use gpu_run::{
    AdapterEvidence, AllowedValidationId, ChildDisplay, DisplayRequest, DisplaySelectionError,
    DisplayServer, DriverRequest, DriverSelectionError, FindingSeverity, GpuCommand, GpuDriver,
    GpuLock, GpuLockError, HostDisplay, HostGpu, MissingDisplayTool, MissingPrerequisite,
    RenderNode, ValidationReport, VALIDATION_ALLOWLIST,
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

/// Every effect shader (plan `2026-11-09-effect-visualization` WP4) composes with its shared
/// import and validates strictly, single-sampled and multisampled; the shared import declares
/// every function the plan names.
#[test]
fn effect_shaders_compose_and_validate() {
    let read = |name: &str| shader_ir::shipped_source(&format!("effects/{name}"));
    let common = read("common.wgsl");
    for function in [
        "fn ramp_lookup",
        "fn erosion_step",
        "fn soft_depth_fade",
        "fn flipbook_sample",
        "fn premultiply",
    ] {
        assert!(
            common.contains(function),
            "common.wgsl must define {function}"
        );
    }
    let modules = [("effects/common.wgsl", common)];
    let mut failures = Vec::new();
    for shader in ["sprite.wgsl", "decal.wgsl", "distortion.wgsl"] {
        let source = read(shader);
        for defs in [&[][..], &["MULTISAMPLED"][..]] {
            match shader_ir::compose_with_modules(shader, &source, defs, &modules) {
                Ok(validated) => {
                    validated.entry_point(naga::ShaderStage::Vertex, "vertex");
                    validated.entry_point(naga::ShaderStage::Fragment, "fragment");
                }
                Err(error) => failures.push(error),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "effect shaders failed:\n{}",
        failures.join("\n")
    );
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

/// F2 of the WP2 review: a command built for inspection never runs, so the command tests'
/// fixed driver cannot become a child with the wrong lock.
#[test]
#[should_panic(expected = "for inspection and never runs")]
fn an_inspected_command_never_runs() {
    let command = GpuCommand::inspect(
        Path::new("/bin/true"),
        &ChildDisplay::None,
        GpuDriver::Lavapipe,
    );
    command.run("inspected command");
}

/// What a command does with one variable of the child's environment.
#[derive(Debug, PartialEq, Eq)]
enum Setting {
    /// Sets it to this value.
    Set(String),
    /// Removes it.
    Removed,
    /// Leaves it as this process has it.
    Inherited,
}

/// What `command` does with `key`.
fn variable(command: &GpuCommand, key: &str) -> Setting {
    match command
        .environment()
        .into_iter()
        .find(|(name, _)| name == key)
    {
        Some((_, Some(value))) => Setting::Set(value),
        Some((_, None)) => Setting::Removed,
        None => Setting::Inherited,
    }
}

/// T-5: an offscreen child runs with no `xvfb-run`, with `DISPLAY`, `WAYLAND_DISPLAY` and
/// `MESA_VK_WSI_DEBUG` removed, and with the driver, the lock and the test-display flag.
#[test]
fn an_offscreen_child_has_no_display_and_no_software_present() {
    for driver in [GpuDriver::Radv, GpuDriver::Lavapipe] {
        let command = GpuCommand::inspect(Path::new("/fixture/child"), &ChildDisplay::None, driver);
        assert_eq!(
            command.command_line(),
            [
                "timeout".to_owned(),
                format!("--kill-after={}", gpu_run::KILL_GRACE_SECS),
                driver.run_limit_secs().to_string(),
                "/fixture/child".to_owned(),
            ],
            "{driver}"
        );
        for removed in ["DISPLAY", "WAYLAND_DISPLAY", "MESA_VK_WSI_DEBUG"] {
            assert_eq!(
                variable(&command, removed),
                Setting::Removed,
                "{driver}: {removed}"
            );
        }
        for (key, value) in [
            ("SPACE_SCUFFLE_VIRTUAL_GPU", "1"),
            ("WGPU_BACKEND", "vulkan"),
            ("VK_ICD_FILENAMES", gpu_run::INSPECTION_MANIFEST),
            ("WGPU_ADAPTER_NAME", driver.adapter_driver()),
        ] {
            assert_eq!(
                variable(&command, key),
                Setting::Set(value.to_owned()),
                "{driver}: {key}"
            );
        }
        assert_eq!(
            variable(&command, "WINIT_UNIX_BACKEND"),
            Setting::Inherited,
            "{driver}"
        );
        assert_eq!(command.display_name(), "offscreen");
        assert_eq!(command.driver(), Some(driver));
    }
}

/// T-5: an Xvfb child keeps `xvfb-run` and radv's software present, as before the `KWin`
/// display existed; the display name says which. A host child has no wrapper.
#[test]
fn an_xvfb_child_keeps_its_virtual_display() {
    let display = ChildDisplay::Xvfb {
        screen: Some("1600x900x24".into()),
    };
    let radv = GpuCommand::inspect(Path::new("/fixture/child"), &display, GpuDriver::Radv);
    let lavapipe = GpuCommand::inspect(Path::new("/fixture/child"), &display, GpuDriver::Lavapipe);
    assert_eq!(
        variable(&radv, "MESA_VK_WSI_DEBUG"),
        Setting::Set("sw".into())
    );
    assert_eq!(variable(&lavapipe, "MESA_VK_WSI_DEBUG"), Setting::Removed);
    for command in [&radv, &lavapipe] {
        assert_eq!(
            variable(command, "DISPLAY"),
            Setting::Inherited,
            "the display is the wrapper's"
        );
        assert_eq!(variable(command, "WAYLAND_DISPLAY"), Setting::Removed);
        assert_eq!(
            variable(command, "WINIT_UNIX_BACKEND"),
            Setting::Set("x11".into())
        );
        let line = command.command_line();
        assert_eq!(line.get(3).map(String::as_str), Some("/usr/bin/xvfb-run"));
        assert!(line.contains(&"-screen 0 1600x900x24 +extension GLX".to_owned()));
        assert_eq!(line.last().map(String::as_str), Some("/fixture/child"));
    }
    assert_eq!(radv.display_name(), "xvfb-sw");
    assert_eq!(lavapipe.display_name(), "xvfb");
    let host = GpuCommand::inspect(
        Path::new("/fixture/child"),
        &ChildDisplay::Host,
        GpuDriver::Radv,
    );
    assert_eq!(
        host.command_line().len(),
        4,
        "no wrapper on the host display"
    );
    assert_eq!(host.display_name(), "host");
}

/// The `KWin` display of the fixture commands, in a runtime directory nobody has.
fn fixture_kwin(screen: Option<&str>) -> ChildDisplay {
    ChildDisplay::Kwin {
        screen: screen.map(str::to_owned),
        runtime_dir: PathBuf::from("/fixture/run"),
    }
}

/// T-5 (`KWin`): the command is `timeout`, then `kwin_wayland` with the headless flags, the
/// screen, a socket and the session string, which runs the status shim, its status file, the
/// program and its arguments. Both display variables and radv's software present are removed.
#[test]
fn a_kwin_child_runs_its_session_under_timeout() {
    let mut command = GpuCommand::inspect(
        Path::new("/fixture/child"),
        &fixture_kwin(Some("1600x900x24")),
        GpuDriver::Radv,
    );
    command.args(["--ignored", "--exact", "a_child::name"]);
    let line = command.command_line();
    let socket = line[line
        .iter()
        .position(|part| part == "--socket")
        .expect("a socket")
        + 1]
    .clone();
    assert!(
        socket.starts_with(&format!("ss-gpu-{}-", std::process::id())),
        "{socket}"
    );
    let directory = format!("/fixture/run/{socket}.session");
    assert_eq!(
        line,
        [
            "timeout".to_owned(),
            format!("--kill-after={}", gpu_run::KILL_GRACE_SECS),
            GpuDriver::Radv.run_limit_secs().to_string(),
            "kwin_wayland".to_owned(),
            "--virtual".to_owned(),
            "--xwayland".to_owned(),
            "--no-lockscreen".to_owned(),
            "--no-global-shortcuts".to_owned(),
            "--width".to_owned(),
            "1600".to_owned(),
            "--height".to_owned(),
            "900".to_owned(),
            "--socket".to_owned(),
            socket.clone(),
            "--exit-with-session".to_owned(),
            format!(
                "{directory}/session.sh {directory}/status /fixture/child --ignored --exact \
                 a_child::name"
            ),
        ]
    );
    for removed in ["DISPLAY", "WAYLAND_DISPLAY", "MESA_VK_WSI_DEBUG"] {
        assert_eq!(variable(&command, removed), Setting::Removed, "{removed}");
    }
    for (key, value) in [
        ("XDG_RUNTIME_DIR", "/fixture/run"),
        ("WINIT_UNIX_BACKEND", "x11"),
        ("SPACE_SCUFFLE_VIRTUAL_GPU", "1"),
        ("WGPU_ADAPTER_NAME", "radv"),
    ] {
        assert_eq!(
            variable(&command, key),
            Setting::Set(value.to_owned()),
            "{key}"
        );
    }
    assert_eq!(command.display_name(), "kwin");
    let default = GpuCommand::inspect(Path::new("/c"), &fixture_kwin(None), GpuDriver::Radv);
    let line = default.command_line();
    assert_eq!(&line[8..12], ["--width", "1280", "--height", "720"]);
}

/// T-5 (`KWin`): every run has its own socket, so concurrent runs never share a server.
#[test]
fn kwin_sockets_are_unique_per_run() {
    let socket = |command: &GpuCommand| {
        let line = command.command_line();
        line[line
            .iter()
            .position(|part| part == "--socket")
            .expect("a socket")
            + 1]
        .clone()
    };
    let sockets: std::collections::BTreeSet<String> = (0..8)
        .map(|_| {
            socket(&GpuCommand::inspect(
                Path::new("/c"),
                &fixture_kwin(None),
                GpuDriver::Radv,
            ))
        })
        .collect();
    assert_eq!(sockets.len(), 8, "{sockets:?}");
}

/// T-5 (`KWin`): `KWin` splits the session string like a shell, so an argument with whitespace is
/// refused with its name instead of being split.
#[test]
#[should_panic(expected = "cannot carry \"two words\"")]
fn a_kwin_argument_with_whitespace_is_refused() {
    let mut command = GpuCommand::inspect(Path::new("/c"), &fixture_kwin(None), GpuDriver::Radv);
    command.arg("two words");
    command.command_line();
}

/// T-5 (`KWin`): quotes and other shell characters are refused too; plain paths and test names
/// pass.
#[test]
fn session_words_are_plain() {
    assert_eq!(
        gpu_run::session_word("/tmp/.tmpAb12/x_y-z.ron"),
        "/tmp/.tmpAb12/x_y-z.ron"
    );
    assert_eq!(gpu_run::session_word("mod::child"), "mod::child");
    for bad in ["a'b", "a\"b", "a\\b", "a$b", "a;b", "a\tb", ""] {
        assert!(
            std::panic::catch_unwind(|| gpu_run::session_word(bad)).is_err(),
            "{bad:?} must be refused"
        );
    }
}

/// T-5 (`KWin`): the status file is the verdict, a missing one is a failure, and `timeout`'s code
/// is a timeout whatever the file says.
#[test]
fn kwin_session_verdicts() {
    use gpu_run::SessionVerdict;
    let verdict = gpu_run::kwin_session_verdict;
    assert_eq!(verdict(Some(0), Some("0\n")), SessionVerdict::Exited(0));
    assert_eq!(
        verdict(Some(0), Some("3\n")),
        SessionVerdict::Exited(3),
        "KWin's 0 is ignored"
    );
    assert_eq!(verdict(Some(3), Some("3\n")), SessionVerdict::Exited(3));
    assert_eq!(
        verdict(Some(0), None),
        SessionVerdict::NoStatus,
        "a missing file fails"
    );
    assert_eq!(verdict(None, None), SessionVerdict::NoStatus);
    assert_eq!(verdict(Some(0), Some("")), SessionVerdict::NoStatus);
    assert_eq!(verdict(Some(124), Some("0\n")), SessionVerdict::TimedOut);
    assert_eq!(verdict(Some(124), None), SessionVerdict::TimedOut);
}

/// `KWin`'s output size comes from the caller's screen.
#[test]
fn kwin_screens_are_parsed() {
    assert_eq!(gpu_run::kwin_screen(None), (1280, 720));
    assert_eq!(gpu_run::kwin_screen(Some("1600x900x24")), (1600, 900));
    assert_eq!(gpu_run::kwin_screen(Some("1280x720")), (1280, 720));
    for bad in ["1600", "0x900", "axb", "1600x"] {
        assert!(
            std::panic::catch_unwind(|| gpu_run::kwin_screen(Some(bad))).is_err(),
            "{bad:?}"
        );
    }
}

/// Amendment 3: a window fits the output of the screen its command states, and `KWin`'s default
/// 1280×720 when it states none; a larger window is refused with the missing size named.
#[test]
fn a_window_must_fit_the_stated_screen() {
    assert_eq!(gpu_run::window_fits_screen((1280, 720), None), Ok(()));
    assert_eq!(
        gpu_run::window_fits_screen((1600, 900), Some("1600x900x24")),
        Ok(())
    );
    assert_eq!(
        gpu_run::window_fits_screen((640, 360), Some("1600x900")),
        Ok(())
    );
    let refused = gpu_run::window_fits_screen((1600, 900), None).expect_err("1600x900 > 1280x720");
    assert_eq!(refused.window, (1600, 900));
    assert_eq!(refused.output, (1280, 720));
    let message = refused.to_string();
    for part in [
        "1600×900 window",
        "states no screen",
        "1280×720",
        "Some(\"1600x900x24\")",
    ] {
        assert!(message.contains(part), "{part:?} missing from {message:?}");
    }
    // One side too large is enough.
    assert!(gpu_run::window_fits_screen((1281, 720), None).is_err());
    assert!(gpu_run::window_fits_screen((1280, 721), None).is_err());
    let narrow = gpu_run::window_fits_screen((1600, 900), Some("1280x1024x24"))
        .expect_err("1600 is wider than 1280");
    assert!(narrow
        .to_string()
        .contains("states the screen \"1280x1024x24\""));
}

/// Amendment 3: the run itself refuses an oversized window before the lock, on `KWin` and on
/// Xvfb, ahead of every other check (the inspected command would otherwise refuse to run).
#[test]
fn an_oversized_window_is_refused_before_the_run() {
    for display in [fixture_kwin(None), ChildDisplay::Xvfb { screen: None }] {
        let mut command = GpuCommand::inspect(Path::new("/bin/true"), &display, GpuDriver::Radv);
        command.window(1600, 900);
        let refusal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            command.run("oversized window");
        }))
        .expect_err("the oversized window is refused");
        let message = refusal
            .downcast_ref::<String>()
            .expect("invariant: the refusal formats its message");
        assert!(
            message.starts_with("oversized window: the child asks for a 1600×900 window"),
            "{message}"
        );
    }
    // A window that fits reaches the next check, the inspection refusal.
    let mut fitting = GpuCommand::inspect(
        Path::new("/bin/true"),
        &fixture_kwin(Some("1600x900x24")),
        GpuDriver::Radv,
    );
    fitting.window(1600, 900);
    let refusal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fitting.run("fitting window");
    }))
    .expect_err("an inspected command never runs");
    let message = refusal
        .downcast_ref::<String>()
        .expect("invariant: the refusal formats its message");
    assert!(
        message.contains("for inspection and never runs"),
        "{message}"
    );
}

/// A host for display selection with the tools `has` names (`kwin`, `xwayland`, `runtime`,
/// `xvfb`, `desktop`), at this server's paths.
fn display_host(has: &[&str]) -> HostDisplay {
    let path = |tool: &str, path: &str| has.contains(&tool).then(|| PathBuf::from(path));
    HostDisplay {
        kwin: path("kwin", "/usr/bin/kwin_wayland"),
        xwayland: path("xwayland", "/usr/bin/Xwayland"),
        runtime_dir: path("runtime", "/run/user/1002"),
        xvfb_run: path("xvfb", "/usr/bin/xvfb-run"),
        desktop: has.contains(&"desktop"),
    }
}

/// D-4: `auto` picks `KWin` only on radv with every prerequisite, falls back to Xvfb (saying so)
/// or the host display, keeps lavapipe on Xvfb, and a forced display never falls back.
#[test]
fn display_selection_matrix() {
    use DisplayRequest::{Auto, Kwin, Xvfb};
    use GpuDriver::{Lavapipe, Radv};
    let full = display_host(&["kwin", "xwayland", "runtime", "xvfb"]);
    let server = |request, driver, host: &HostDisplay| {
        gpu_run::select_display(request, driver, host).map(|selection| selection.server)
    };
    assert_eq!(server(Auto, Radv, &full), Ok(DisplayServer::Kwin));
    let kwin = gpu_run::select_display(Auto, Radv, &full).expect("KWin");
    assert_eq!(kwin.runtime_dir, Some(PathBuf::from("/run/user/1002")));
    assert!(!kwin.fell_back);
    assert_eq!(server(Auto, Lavapipe, &full), Ok(DisplayServer::Xvfb));
    let lavapipe = gpu_run::select_display(Auto, Lavapipe, &full).expect("Xvfb");
    assert!(
        !lavapipe.fell_back && lavapipe.runtime_dir.is_none(),
        "{lavapipe:?}"
    );
    for (host, missing) in [
        (
            display_host(&["xwayland", "runtime", "xvfb"]),
            MissingDisplayTool::Kwin,
        ),
        (
            display_host(&["kwin", "runtime", "xvfb"]),
            MissingDisplayTool::Xwayland,
        ),
        (
            display_host(&["kwin", "xwayland", "xvfb"]),
            MissingDisplayTool::RuntimeDir,
        ),
    ] {
        let fallback = gpu_run::select_display(Auto, Radv, &host).expect("a fallback");
        assert_eq!(fallback.server, DisplayServer::Xvfb);
        assert!(fallback.fell_back);
        assert!(
            fallback.reason.contains(&missing.to_string()),
            "{}",
            fallback.reason
        );
        assert!(
            fallback.reason.contains("MESA_VK_WSI_DEBUG=sw"),
            "{}",
            fallback.reason
        );
        assert_eq!(
            server(Kwin, Radv, &host),
            Err(DisplaySelectionError::KwinUnavailable(vec![missing]))
        );
    }
    let bare_desktop = display_host(&["desktop"]);
    assert_eq!(server(Auto, Radv, &bare_desktop), Ok(DisplayServer::Host));
    assert_eq!(
        server(Auto, Lavapipe, &bare_desktop),
        Ok(DisplayServer::Host)
    );
    let nothing = display_host(&[]);
    assert_eq!(
        server(Auto, Radv, &nothing),
        Err(DisplaySelectionError::NoDisplay)
    );
    assert_eq!(
        server(Kwin, Lavapipe, &full),
        Err(DisplaySelectionError::KwinOnLavapipe)
    );
    assert_eq!(server(Kwin, Radv, &full), Ok(DisplayServer::Kwin));
    assert_eq!(server(Xvfb, Radv, &full), Ok(DisplayServer::Xvfb));
    assert_eq!(server(Xvfb, Lavapipe, &full), Ok(DisplayServer::Xvfb));
    assert_eq!(
        server(Xvfb, Radv, &bare_desktop),
        Err(DisplaySelectionError::XvfbUnavailable)
    );
    assert_eq!(DisplayRequest::parse(None), Ok(Auto));
    assert_eq!(DisplayRequest::parse(Some("kwin")), Ok(Kwin));
    assert_eq!(DisplayRequest::parse(Some("xvfb")), Ok(Xvfb));
    assert_eq!(
        DisplayRequest::parse(Some("weston")),
        Err(DisplaySelectionError::InvalidRequest("weston".into()))
    );
}

/// Whether this process's windowed children use `KWin`; when not, says why and checks that a
/// lavapipe process never does.
fn kwin_is_selected(test: &str) -> bool {
    let display = gpu_run::selected_display();
    if display.server == DisplayServer::Kwin {
        return true;
    }
    eprintln!("{test}: no KWin run, because the {display}");
    if gpu_run::selected_driver().driver == GpuDriver::Lavapipe {
        assert_eq!(
            display.server,
            DisplayServer::Xvfb,
            "lavapipe stays on Xvfb"
        );
    }
    false
}

/// Writes an executable shell script `name` with `body` into `directory`.
fn script(directory: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;
    let path = directory.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// The message of a caught panic.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
        })
        .unwrap_or_default()
}

/// T-8: a `KWin` session whose program exits 3 fails the run with that status, through the
/// status shim, on the real `KWin` and the shared lock.
#[test]
fn a_failing_kwin_session_fails_the_run() -> Result<(), Box<dyn std::error::Error>> {
    if gpu_run::gpu_tests_opted_out() || !kwin_is_selected("a_failing_kwin_session_fails_the_run") {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let program = script(directory.path(), "exit3.sh", "echo kwin-probe-ran; exit 3")?;
    let command = gpu_run::virtual_gpu_command_with(&program, None, DisplayRequest::Kwin);
    assert_eq!(command.display_name(), "kwin");
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        command.run("kwin exit 3 probe")
    }));
    let Err(payload) = caught else {
        panic!("a session that exits 3 must fail the run");
    };
    let message = panic_text(payload.as_ref());
    assert!(message.contains("exited with Some(3)"), "{message}");
    assert!(
        message.contains("kwin-probe-ran"),
        "the program's output is captured: {message}"
    );
    Ok(())
}

/// A process as `/proc/<pid>/stat` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessEntry {
    /// Process ID.
    pid: u32,
    /// Parent process ID.
    parent: u32,
    /// Command name, in parentheses in the stat line.
    comm: String,
    /// Start time in clock ticks after boot, which tells a reused PID from the original.
    start: u64,
    /// `true` for a zombie (state `Z`), which has exited and only waits to be reaped.
    zombie: bool,
}

/// Every process of the host, from `/proc/<pid>/stat`, which is readable even for the
/// non-dumpable `kwin_wayland` (Debian gives it file capabilities, so its `environ` is not).
fn processes() -> Vec<ProcessEntry> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            let (head, rest) = stat.rsplit_once(')')?;
            let comm = head.split_once('(')?.1.to_owned();
            let fields: Vec<&str> = rest.split_whitespace().collect();
            Some(ProcessEntry {
                pid,
                parent: fields.get(1)?.parse().ok()?,
                comm,
                start: fields.get(19)?.parse().ok()?,
                zombie: *fields.first()? == "Z",
            })
        })
        .collect()
}

/// The live processes whose environment holds `tag`; the run's `timeout` is one of them.
fn tagged_roots(tag: &str) -> Vec<u32> {
    processes()
        .into_iter()
        .filter(|process| {
            std::fs::read(format!("/proc/{}/environ", process.pid)).is_ok_and(|environ| {
                environ
                    .split(|byte| *byte == 0)
                    .any(|entry| entry == tag.as_bytes())
            })
        })
        .map(|process| process.pid)
        .collect()
}

/// `roots` and every live descendant of them.
fn process_tree(roots: &[u32]) -> Vec<ProcessEntry> {
    let all = processes();
    let mut tree: Vec<ProcessEntry> = all
        .iter()
        .filter(|process| roots.contains(&process.pid))
        .cloned()
        .collect();
    let mut grown = true;
    while grown {
        grown = false;
        for process in &all {
            let known = tree.iter().any(|member| member.pid == process.pid);
            if !known && tree.iter().any(|member| member.pid == process.parent) {
                tree.push(process.clone());
                grown = true;
            }
        }
    }
    tree.retain(|process| !process.zombie);
    tree
}

/// The members of `sample` that still run: same PID, same start time, not a zombie.
fn still_running(sample: &[ProcessEntry]) -> Vec<ProcessEntry> {
    let now = processes();
    sample
        .iter()
        .filter(|old| {
            now.iter()
                .any(|new| new.pid == old.pid && new.start == old.start && !new.zombie)
        })
        .cloned()
        .collect()
}

/// T-8: a `KWin` session that outlives its run limit is ended by `timeout`, and nothing of the
/// run remains: no `kwin_wayland`, `Xwayland` or program. During the run, a sampler finds the
/// run's `timeout` by a variable only this run sets and records it with all its descendants;
/// after the run, none of those processes (same PID and start time) may still run.
#[test]
fn a_timed_out_kwin_session_leaves_nothing_running() -> Result<(), Box<dyn std::error::Error>> {
    if gpu_run::gpu_tests_opted_out()
        || !kwin_is_selected("a_timed_out_kwin_session_leaves_nothing_running")
    {
        return Ok(());
    }
    let tag = format!(
        "SPACE_SCUFFLE_KWIN_PROBE={}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos()
    );
    let (key, value) = tag.split_once('=').expect("invariant: the tag has an =");
    // `KWin` starts Xwayland only when an X client connects, so the session first connects one
    // where `xdpyinfo` is installed, then sleeps past the limit.
    let x_client = gpu_run::find_on_path("xdpyinfo");
    let directory = tempfile::tempdir()?;
    let connect = x_client.as_ref().map_or_else(String::new, |client| {
        format!("{} > /dev/null\n", client.display())
    });
    let program = script(
        directory.path(),
        "sleeper.sh",
        &format!("{connect}exec /bin/sleep 600"),
    )?;
    let mut expected = vec!["timeout", "kwin_wayland", "sleep"];
    if x_client.is_some() {
        expected.push("Xwayland");
    }
    let mut command = gpu_run::virtual_gpu_command_with(&program, None, DisplayRequest::Kwin);
    command.env(key, value).set_run_limit_for_self_test(20);
    let sampler = {
        let tag = tag.clone();
        let expected: Vec<String> = expected.iter().map(|name| (*name).to_owned()).collect();
        std::thread::spawn(move || {
            // The lock wait comes first, then the 20 s run and its grace.
            let deadline = Instant::now() + Duration::from_secs(400);
            let mut widest = Vec::new();
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(250));
                let tree = process_tree(&tagged_roots(&tag));
                let complete = expected
                    .iter()
                    .all(|name| tree.iter().any(|process| &process.comm == name));
                if tree.len() > widest.len() || complete {
                    widest = tree;
                }
                if complete {
                    break;
                }
            }
            widest
        })
    };
    let started = Instant::now();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        command.run("kwin timeout probe")
    }));
    let elapsed = started.elapsed();
    let Err(payload) = caught else {
        panic!("a session that sleeps 600 s must hit the 20 s limit");
    };
    let message = panic_text(payload.as_ref());
    assert!(message.contains("did not finish within 20 s"), "{message}");
    let during = sampler.join().map_err(|_| "the sampler panicked")?;
    let names: Vec<&str> = during.iter().map(|process| process.comm.as_str()).collect();
    for name in &expected {
        assert!(
            names.contains(name),
            "{name} runs during the session: {names:?}"
        );
    }
    let mut left = still_running(&during);
    let settle = Instant::now();
    while !left.is_empty() && settle.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(250));
        left = still_running(&during);
    }
    assert!(left.is_empty(), "the timed-out run left {left:?}");
    eprintln!(
        "kwin timeout probe: ended after {:.1} s; during the run {names:?}; nothing left",
        elapsed.as_secs_f64()
    );
    Ok(())
}

/// T-5: the ledger line names the display; a lock self-test has none.
#[test]
fn the_ledger_line_names_the_display() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let lock = directory.path().join("gpu.lock");
    let mut command = GpuCommand::lock_probe(Path::new("/bin/sh"), &lock, Duration::from_secs(5));
    command.args(["-c", "true"]);
    assert_eq!(command.display_name(), "none");
    let run = command.run("display probe");
    assert!(
        run.ledger
            .starts_with("label=display probe\tdriver=none\tdisplay=none\tlock=private lock\t"),
        "{}",
        run.ledger
    );
    assert!(run.ledger.ends_with('\n'));
    Ok(())
}

/// T-5: an offscreen child logs no window, so its timeline has no app-build phase and that time
/// counts toward "startup and frames".
#[test]
fn an_offscreen_timeline_has_no_window_phase() {
    let spawned = gpu_run::log_record_time("2026-11-17T10:00:00.000000Z").expect("spawn time");
    let log = "2026-11-17T10:00:01.000000Z INFO bevy_diagnostic: SystemInfo { os: \"Linux\" }\n\
               2026-11-17T10:00:03.000000Z INFO bevy_render::renderer: AdapterInfo { name: \"AMD\" }\n\
               2026-11-17T10:00:09.500000Z INFO client::effects: effect lab done\n";
    let timeline = gpu_run::ChildTimeline::read(log, spawned, Duration::from_secs(10));
    assert_eq!(
        timeline.phases,
        [
            ("launch", Duration::from_secs(1)),
            ("render device", Duration::from_secs(2)),
            ("startup and frames", Duration::from_millis(6_500)),
            ("after the last log record", Duration::from_millis(500)),
        ]
    );
}
