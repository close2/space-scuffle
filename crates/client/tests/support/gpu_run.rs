//! Runs real client binaries on the virtual GPU and turns Vulkan validation output into a verdict.
//!
//! # Responsibility
//! Owns the launch environment for windowed GPU tests, the opt-out policy, and the gate on Vulkan
//! validation findings (remediation finding Q-7).
//!
//! # How validation reaches this module
//! 1. `WGPU_VALIDATION=1` makes `wgpu_hal` load `VK_LAYER_KHRONOS_validation` and register a debug
//!    messenger. The layer writes through that messenger, never to a file.
//! 2. The messenger forwards each message to the `log` crate under the target
//!    `wgpu_hal::vulkan::instance`, as `<TYPE> [<ID> (0x<number>)]` plus a tab-indented body.
//! 3. Bevy's `LogPlugin` writes those records to stderr, subject to `RUST_LOG`.
//!
//! The binary must be a child process because a Bevy app owns its global log subscriber, and stderr
//! is the only interface here that needs no production change.
//!
//! # Limits of the gate
//! - `wgpu_hal` drops three message IDs before logging them: `VUID-VkSwapchainCreateInfoKHR-pNext-07781`,
//!   `VUID-vkCmdEndDebugUtilsLabelEXT-commandBuffer-01912` on layers 1.3.240 to 1.3.250, and
//!   `VUID-VkRenderPassBeginInfo-framebuffer-04627` when the OBS layer is present. This gate
//!   cannot see them.
//! - Only code paths the binary reaches during the run are validated.
//!
//! # Invariants
//! - A run counts as validated only if the loader reported inserting the layer, so a missing layer
//!   or a broken log path fails instead of looking clean.
//! - `crates/client/tests/gpu_validation_tests.rs` proves the gate on real layer output by enabling
//!   best-practices checks and requiring findings.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// Wall-clock limit for one GPU run, in seconds. Debug builds on lavapipe finish in under 60.
const GPU_RUN_TIMEOUT_SECS: u32 = 300;

/// Exit status of GNU `timeout` when the deadline expired.
const TIMEOUT_EXIT_CODE: i32 = 124;

/// Lowest display number used, clear of the `:99` default that plain `xvfb-run -a` starts at.
const FIRST_DISPLAY: u32 = 200;

/// Process-ID slots in the display range. With [`DISPLAYS_PER_PROCESS`] this stays below the
/// X11 limit of display 59535 (TCP port 65535).
const DISPLAY_PROCESS_SLOTS: u32 = 9_000;

/// Concurrent runs one test process can start without sharing a search start.
const DISPLAYS_PER_PROCESS: u32 = 4;

/// Log target of the `wgpu_hal` debug messenger callback.
const MESSENGER_TARGET: &str = "wgpu_hal::vulkan::instance";

/// Loader message that proves the validation layer was inserted into the instance.
const LAYER_ACTIVE_MARKER: &str = "Insert instance layer \"VK_LAYER_KHRONOS_validation\"";

/// `RUST_LOG` for validated runs. The `info` level for `wgpu_hal` keeps [`LAYER_ACTIVE_MARKER`].
const VALIDATED_RUST_LOG: &str = "warn,wgpu_hal=info";

/// A validation message ID that is deliberately tolerated, with the evidence for doing so.
#[derive(Debug, Clone, Copy)]
pub struct AllowedValidationId {
    /// Exact message ID name, such as `VUID-vkCmdDraw-None-08600`.
    pub id: &'static str,
    /// Why the message is not a defect in this project, with a link to the upstream report.
    pub reason: &'static str,
}

/// Validation IDs that do not fail a run.
///
/// Add an entry only for a message shown to be outside this project: a defect with a named upstream
/// report, or a capability notice from the host. Matching is exact, so a listed line never hides
/// a different finding.
pub const VALIDATION_ALLOWLIST: &[AllowedValidationId] = &[AllowedValidationId {
    id: "Unable to find extension: VK_EXT_swapchain_colorspace",
    reason: "Not a validation finding. `desired_extensions` in wgpu-hal 23.0.1 \
             (src/vulkan/instance.rs) logs this for an optional instance extension the driver does \
             not offer, drops the extension and continues. Lavapipe in Mesa 22.3.6 does not offer \
             it. The extension only adds wide-gamut swapchain colour spaces.",
}];

/// Severity of a validation message, as mapped by the `wgpu_hal` messenger callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingSeverity {
    /// `VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT`.
    Warning,
    /// `VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT`.
    Error,
}

/// One warning or error emitted through the Vulkan messenger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationFinding {
    /// Message severity.
    pub severity: FindingSeverity,
    /// Message ID name, or the whole line when `wgpu_hal` logged an unstructured warning.
    pub id: String,
    /// First line of the message body.
    pub message: String,
}

/// Everything a run's stderr says about validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    /// Whether the loader reported inserting the Khronos validation layer.
    pub layer_active: bool,
    /// All warnings and errors from the Vulkan messenger, in order.
    pub findings: Vec<ValidationFinding>,
    /// `ERROR` lines from any other target, such as a failed Bevy shader compilation.
    pub error_lines: Vec<String>,
}

impl ValidationReport {
    /// Parses captured stderr from a child that used [`VALIDATED_RUST_LOG`].
    pub fn parse(stderr: &str) -> Self {
        let mut report = Self::default();
        let lines: Vec<String> = stderr.lines().map(strip_ansi).collect();
        for (index, line) in lines.iter().enumerate() {
            report.layer_active |= line.contains(LAYER_ACTIVE_MARKER);
            let Some(record) = LogRecord::parse(line) else {
                continue;
            };
            let severity = match record.level {
                "ERROR" => FindingSeverity::Error,
                "WARN" => FindingSeverity::Warning,
                _ => continue,
            };
            if record.target == MESSENGER_TARGET {
                // Label, queue and object details follow a finding on their own tab-led records.
                if record.message.starts_with('\t') {
                    continue;
                }
                let body = lines.get(index + 1).filter(|next| next.starts_with('\t'));
                report.findings.push(ValidationFinding {
                    severity,
                    id: message_id(record.message)
                        .unwrap_or(record.message)
                        .to_owned(),
                    message: body.map_or("", |next| next.trim()).to_owned(),
                });
            } else if severity == FindingSeverity::Error {
                report.error_lines.push(line.clone());
            }
        }
        report
    }

    /// Returns findings whose ID is absent from `allowlist`.
    pub fn unlisted<'a>(
        &'a self,
        allowlist: &'a [AllowedValidationId],
    ) -> impl Iterator<Item = &'a ValidationFinding> {
        self.findings
            .iter()
            .filter(|finding| !allowlist.iter().any(|allowed| allowed.id == finding.id))
    }

    /// Lists every reason this report does not pass, using `allowlist`. Empty means clean.
    pub fn violations(&self, allowlist: &[AllowedValidationId]) -> Vec<String> {
        let mut violations = Vec::new();
        if !self.layer_active {
            violations.push(
                "VK_LAYER_KHRONOS_validation was not inserted, so nothing was validated. \
                 Install the validation layers (Arch: vulkan-validation-layers, \
                 Debian and Ubuntu: vulkan-validationlayers)."
                    .to_owned(),
            );
        }
        violations.extend(
            self.unlisted(allowlist)
                .map(|f| format!("{:?} [{}] {}", f.severity, f.id, f.message)),
        );
        violations.extend(self.error_lines.iter().cloned());
        violations
    }
}

/// Fields of one line of Bevy's default log format.
struct LogRecord<'a> {
    level: &'a str,
    target: &'a str,
    message: &'a str,
}

impl<'a> LogRecord<'a> {
    /// Parses `<timestamp> <LEVEL> <target>: <message>`, or returns `None` for any other line.
    fn parse(line: &'a str) -> Option<Self> {
        let (timestamp, rest) = line.split_once(' ')?;
        if !timestamp.ends_with('Z') || !timestamp.contains('T') {
            return None;
        }
        let (level, rest) = rest.trim_start().split_once(' ')?;
        if !matches!(level, "ERROR" | "WARN" | "INFO" | "DEBUG" | "TRACE") {
            return None;
        }
        let (target, message) = rest.split_once(": ")?;
        Some(Self {
            level,
            target,
            message,
        })
    }
}

/// Extracts `<ID>` from a messenger header of the form `<TYPE> [<ID> (0x<number>)]`.
fn message_id(header: &str) -> Option<&str> {
    let (_, bracketed) = header.split_once('[')?;
    let (id, _) = bracketed.rsplit_once(" (0x")?;
    header.ends_with(")]").then_some(id)
}

/// Removes ANSI colour sequences, which Bevy writes even when stderr is a pipe.
fn strip_ansi(line: &str) -> String {
    let mut plain = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            for terminator in chars.by_ref() {
                if terminator.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(character);
        }
    }
    plain
}

/// Returns `true` when an explicit local opt-out asks to skip GPU tests.
///
/// `SPACE_SCUFFLE_SKIP_GPU_TESTS=1` exists for developers without a usable GPU stack. It panics
/// when the `CI` variable is set, so a CI run can never report skipped GPU work as passed.
pub fn gpu_tests_opted_out() -> bool {
    let requested = std::env::var("SPACE_SCUFFLE_SKIP_GPU_TESTS").as_deref() == Ok("1");
    if requested {
        assert!(
            std::env::var_os("CI").is_none(),
            "SPACE_SCUFFLE_SKIP_GPU_TESTS=1 is rejected on CI: GPU tests must run there"
        );
        eprintln!("WARNING: SPACE_SCUFFLE_SKIP_GPU_TESTS=1 is set; skipping windowed GPU test");
    }
    requested
}

/// Returns a display number to start searching from, distinct per run.
///
/// `xvfb-run -a` tests a lock file and then starts the server, so two runs starting together can
/// both choose `:99`. The first to exit then kills the server under the other, which dies with
/// `X connection broken`. Starting each run at its own number closes that window across test
/// threads and across concurrent test processes on a shared machine.
fn display_search_start() -> u32 {
    static NEXT_RUN: AtomicU32 = AtomicU32::new(0);
    let run = NEXT_RUN.fetch_add(1, Ordering::Relaxed) % DISPLAYS_PER_PROCESS;
    FIRST_DISPLAY + (std::process::id() % DISPLAY_PROCESS_SLOTS) * DISPLAYS_PER_PROCESS + run
}

/// Builds a command that runs `binary` on a virtual display with the lavapipe software adapter.
///
/// GNU `timeout` wraps the run, because [`Command::output`] has no deadline and a client that
/// loses its display can spin forever. It signals the whole process group, including the client
/// under `xvfb-run`, and exits with [`TIMEOUT_EXIT_CODE`].
///
/// # Panics
/// Panics when neither `xvfb-run` nor a display exists. A GPU test must not pass without running.
pub fn virtual_gpu_command(binary: &Path) -> Command {
    virtual_gpu_command_on_screen(binary, None)
}

/// Builds a [`virtual_gpu_command`] whose display has the size `screen`, such as `1600x900x24`.
///
/// # Panics
/// Panics when neither `xvfb-run` nor a display exists. A GPU test must not pass without running.
pub fn virtual_gpu_command_on_screen(binary: &Path, screen: Option<&str>) -> Command {
    let xvfb_run = Path::new("/usr/bin/xvfb-run");
    let mut command = Command::new("timeout");
    command
        .arg("--kill-after=10")
        .arg(GPU_RUN_TIMEOUT_SECS.to_string());
    if xvfb_run.exists() {
        // `-n` must precede `-a`, which reads the starting number as it is parsed.
        command
            .arg(xvfb_run)
            .arg("-n")
            .arg(display_search_start().to_string())
            .arg("-a");
        if let Some(screen) = screen {
            command
                .arg("-s")
                .arg(format!("-screen 0 {screen} +extension GLX"));
        }
    } else {
        assert!(
            !(std::env::var_os("WAYLAND_DISPLAY").is_none()
                && std::env::var_os("DISPLAY").is_none()),
            "CRITICAL: No graphical display detected and /usr/bin/xvfb-run is missing! \
             GPU tests require a display or xvfb-run. Set SPACE_SCUFFLE_SKIP_GPU_TESTS=1 to bypass."
        );
    }
    command.arg(binary);
    // The test must stay on the virtual display and never appear on the user's desktop.
    command.env_remove("WAYLAND_DISPLAY");
    command.env("WINIT_UNIX_BACKEND", "x11");
    command.env("WGPU_BACKEND", "vulkan");
    command.env("SPACE_SCUFFLE_VIRTUAL_GPU", "1");

    let lavapipe_manifests = [
        Path::new("/usr/share/vulkan/icd.d/lvp_icd.json"),
        Path::new("/usr/share/vulkan/icd.d/lvp_icd.x86_64.json"),
    ];
    if let Some(manifest) = lavapipe_manifests.into_iter().find(|path| path.exists()) {
        command.env("VK_ICD_FILENAMES", manifest);
        command.env("WGPU_ADAPTER_NAME", "llvmpipe");
        command.env("LIBGL_ALWAYS_SOFTWARE", "1");
    }
    command
}

/// Adds the environment that forces validation and keeps its output on stderr.
///
/// `RUST_LOG` is replaced so a developer's own filter cannot hide findings.
pub fn enable_validation(command: &mut Command) {
    command.env("WGPU_VALIDATION", "1");
    command.env("RUST_LOG", VALIDATED_RUST_LOG);
}

/// Runs `command` under validation and returns its output with the parsed report.
pub fn run_validated(command: &mut Command) -> (Output, ValidationReport) {
    enable_validation(command);
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to launch {command:?}: {error}"));
    let report = ValidationReport::parse(&String::from_utf8_lossy(&output.stderr));
    (output, report)
}

/// Runs `command` and fails on a nonzero exit or on any validation violation.
///
/// `label` names the run in failure messages.
pub fn assert_passes_validation(command: &mut Command, label: &str) -> Output {
    let (output, report) = run_validated(command);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(TIMEOUT_EXIT_CODE),
        "{label} did not finish within {GPU_RUN_TIMEOUT_SECS} s. Last stderr:\n{}",
        tail(&stderr)
    );
    assert!(
        output.status.success(),
        "{label} exited with {:?}. Last stderr:\n{}",
        output.status.code(),
        tail(&stderr)
    );
    let violations = report.violations(VALIDATION_ALLOWLIST);
    assert!(
        violations.is_empty(),
        "{label} failed Vulkan validation:\n{}",
        violations.join("\n")
    );
    output
}

/// Returns the last 30 lines of `text`, enough to diagnose a crash without flooding the report.
pub fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(30)..].join("\n")
}
