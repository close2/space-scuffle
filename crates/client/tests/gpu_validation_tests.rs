//! Behavioural tests of the Vulkan validation gate in `support/gpu_run.rs` (remediation Q-7).
//!
//! # Invariants Verified
//! - The parser extracts severity and message ID from real `wgpu_hal` log lines.
//! - The gate fails on an unlisted ID, a missing layer, or an `ERROR` from another subsystem.
//! - Real validation output reaches the gate: best-practices checks produce findings in the same
//!   run path as the clean runs. Without this control a broken capture would look like a clean run.
//! - Live battle mode, which builds a different pipeline set from the lab, is clean.

#![allow(clippy::expect_used, clippy::panic)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use gpu_run::{AllowedValidationId, FindingSeverity, ValidationReport, VALIDATION_ALLOWLIST};
use std::path::Path;

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
    let (output, report) = gpu_run::run_validated(&mut command);
    assert!(
        output.status.success(),
        "control run exited with {:?}. Last output:\n{}",
        output.status.code(),
        gpu_run::tail(&gpu_run::captured_log(&output))
    );
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
    gpu_run::assert_passes_validation(&mut command, "client --battle --max-frames 60");
}
