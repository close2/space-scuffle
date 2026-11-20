//! Runs real client binaries on a test GPU, one locked child at a time, and turns Vulkan
//! validation output into a verdict.
//!
//! # Responsibility
//! Owns everything about how a GPU test child is launched:
//! - **Driver selection.** [`select_driver`] chooses `radv` (the physical GPU) or `lavapipe`
//!   (software Vulkan) once per test process from `SPACE_SCUFFLE_GPU_TEST_DRIVER` and the host's
//!   prerequisites, and never falls back from a forced choice.
//! - **Display selection.** [`select_display`] chooses where windowed children present, once per
//!   test process, from `SPACE_SCUFFLE_GPU_TEST_DISPLAY` (`auto`, `kwin`, `xvfb`): a headless
//!   `KWin` on `radv` where the host has `kwin_wayland`, `Xwayland` and `XDG_RUNTIME_DIR`;
//!   otherwise `xvfb-run` with `MESA_VK_WSI_DEBUG=sw`, said on the selection line and in the
//!   ledger. `lavapipe` always stays on Xvfb, because `KWin` would composite on the physical GPU
//!   without the lock. A forced choice never falls back.
//! - **The shared GPU lock.** On `radv`, [`GpuLock`] takes `/run/lock/gpu.lock`, the file
//!   `with-gpu-lock` locks, immediately before the child starts and releases it when the child
//!   exits. Nothing else holds it, so other users of the server get the GPU between children.
//!   When the host's `with-gpu-lock` declares the fair queue ([`GPU_QUEUE_DIR`], B-136), the
//!   harness takes the lock in order of arrival with every other taker that queues
//!   ([`GpuLock::acquire_queued`], [`Ticket`]), so a series of short holds cannot starve a child.
//! - **The launch environment**, the run limit, the evidence of which adapter rendered, the
//!   opt-out policy, and the gate on Vulkan validation findings (remediation finding Q-7).
//!
//! The decisions and their measurements are in `docs/plans/2026-11-03-gpu-test-harness.md`
//! (sections 3 and 4, decisions D1 to D9).
//!
//! # How a child runs
//! 1. [`virtual_gpu_command`] builds a [`GpuCommand`] on the selected display, with the selected
//!    driver's environment: GNU `timeout`, then `xvfb-run`, then the binary; or GNU `timeout`,
//!    then `kwin_wayland --virtual --xwayland ... --exit-with-session "<shim> <status file>
//!    <binary> <args>"` (plan `docs/plans/2026-11-17-gpu-test-readback.md`, D-4). `KWin` runs the
//!    binary through a status shim, because the status file, not `KWin`'s exit code, is the
//!    child's verdict ([`kwin_session_verdict`]); and it splits the session string like a shell,
//!    so a path or argument it cannot carry as one word is refused ([`session_word`]).
//!    [`virtual_gpu_command_with`] forces a display for one command. [`offscreen_gpu_command`]
//!    builds one with
//!    no display at all, for a child that renders into an image and reads only its own
//!    pictures (`client::offscreen`; plan `docs/plans/2026-11-17-gpu-test-readback.md`, D-7).
//!    Callers add arguments and variables.
//! 2. [`GpuCommand::run`], [`GpuCommand::run_validated`] or
//!    [`GpuCommand::assert_passes_validation`] is the only way to start it. A `GpuCommand`
//!    exposes no `output`, `spawn` or `status`, so no caller can run a GPU child without the
//!    lock.
//! 3. The run reads the child's executable into the page cache ([`preload_executable`]), takes
//!    the lock (radv only), spawns the child, waits for it, releases the lock, then
//!    checks the exit status and the adapter line, and appends a line to the ledger if
//!    `SPACE_SCUFFLE_GPU_RUN_LEDGER` names a file (`cargo gate` names one per run). The line
//!    records the wait for the lock (`lock_wait_s`) and the run (`run_s`); a child whose wait
//!    expired is recorded too, with `exit=lock-timeout`, before the run fails. The line names the child's display
//!    (`display=offscreen`, `kwin`, `xvfb-sw`, `xvfb`, `host` or `none`). The child's own log
//!    timestamps split its run into phases ([`ChildTimeline`]); the ledger records them, and a
//!    timeout names the slowest.
//!    `SPACE_SCUFFLE_GPU_RUN_LOG_DIR` keeps each child's whole log for a timing investigation.
//!
//! `SPACE_SCUFFLE_VIRTUAL_GPU=1`, which every child receives, means "a test display": the client
//! keeps its window off the desktop. It does not mean a software GPU.
//!
//! # How validation reaches this module
//! 1. `WGPU_VALIDATION=1` makes `wgpu_hal` load `VK_LAYER_KHRONOS_validation`, and `WGPU_DEBUG=1`
//!    makes it enable `VK_EXT_debug_utils` and register a debug messenger. The layer and the
//!    loader write through that messenger, never to a file. A debug build sets both bits by
//!    default, a release build neither (wgpu-types' `InstanceFlags::from_build_config`, which
//!    Bevy extends with `with_env`), so the harness sets both: with the layer but no messenger,
//!    a release child never logs the loader's "inserted" line and fails as unvalidated (plan
//!    `2026-11-20-gpu-lane` D-5, B-157).
//! 2. The messenger forwards each message to the `log` crate under the target
//!    `wgpu_hal::vulkan::instance`, as `<TYPE> [<ID> (0x<number>)]` plus a tab-indented body.
//! 3. Bevy's `LogPlugin` writes those records to stderr, subject to `RUST_LOG`. Bevy also logs
//!    the adapter it chose as `AdapterInfo { name: .., driver: .., driver_info: .. }` under
//!    `bevy_render::renderer` at `info`, which [`AdapterEvidence::parse`] reads.
//! 4. `xvfb-run` passes that stream on. Debian 12 and Arch keep it on stderr, but Ubuntu 24.04 runs
//!    the command as `"$@" 2>&1`, which moves it to stdout, so [`captured_log`] reads both.
//!
//! The binary must be a child process because a Bevy app owns its global log subscriber, and its
//! log output is the only interface here that needs no production change.
//!
//! # Limits of the gate
//! - `wgpu_hal` drops three message IDs before logging them:
//!   `VUID-VkSwapchainCreateInfoKHR-pNext-07781`,
//!   `VUID-vkCmdEndDebugUtilsLabelEXT-commandBuffer-01912` on layers 1.3.240 to 1.3.250, and
//!   `VUID-VkRenderPassBeginInfo-framebuffer-04627` when the OBS layer is present. This gate
//!   cannot see them.
//! - Only code paths the binary reaches during the run are validated.
//! - On `lavapipe` before Mesa 25.1, the runs that draw hex-tiled ground render without
//!   anisotropic filtering, because that driver's sampler does not finish such a frame (see
//!   [`GpuCommand::allow_for_hex_tiling_driver_defect`]). `radv` always uses the production
//!   sampler.
//! - On `radv` under the headless `KWin`, windowed children present through DRI3 to `KWin`'s
//!   Xwayland, as on a desktop, and `KWin` composites on the same GPU inside the child's lock.
//!   Where `KWin` is missing, they present with `MESA_VK_WSI_DEBUG=sw` under Xvfb, which has no
//!   DRI3, so the GPU renders every frame but the DRI3 present path is not exercised. On
//!   `lavapipe` the software present is the only one. Offscreen children present nothing, so
//!   they exercise no present path at all.
//! - A `KWin` child's output includes `KWin`'s and Xwayland's own messages. They have no
//!   `tracing` timestamp, so neither the validation parser nor the timeline reads them, and
//!   `KWin`'s start (about 1 s) counts toward the launch phase.
//! - If the test process itself is killed with `SIGKILL`, its child keeps running without the
//!   lock for at most the run limit plus the kill grace ([`GpuDriver::run_limit_secs`],
//!   [`KILL_GRACE_SECS`]).
//! - On Linux, `timeout` leads its own process group. When it exits, the harness gives the group
//!   [`GROUP_EXIT_GRACE`] to empty on its own (Xvfb removing its lock file and socket), then kills
//!   what remains and waits until every member is gone, before it reads the output to its end and
//!   releases the lock ([`run_to_end`]). So a process that outlives `KWin` or the program (one
//!   that ignores `SIGTERM`, a late Xwayland) can hold neither the pipes nor the lock. Elsewhere
//!   the run waits for the pipes only, as `Command::output` does; the `KWin` display needs Linux.
//!
//! # Invariants
//! - A run counts as validated only if the loader reported inserting the layer, so a missing layer
//!   or a broken log path fails instead of looking clean.
//! - A run passes only if its adapter line names the selected driver, so no test can pass on the
//!   wrong GPU.
//! - The shared lock is held only for the lifetime of one child, never across compilation, two
//!   children or a test binary. Self-tests of the lock use a private lock file.
//! - `crates/client/tests/gpu_validation_tests.rs` proves the gate on real layer output by enabling
//!   best-practices checks and requiring findings.

#![allow(clippy::expect_used, clippy::panic)]

use std::ffi::OsStr;
use std::fmt;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

/// The lock file `with-gpu-lock` takes with `flock(1)`, shared by every user of the server's GPU.
///
/// `flock(1)` calls `flock(2)`, and so does [`File::try_lock`] on Linux, so both take part in the
/// same protocol. `the_host_lock_tool_uses_the_harness_lock_file` fails if the host script names
/// another file.
pub const GPU_LOCK_PATH: &str = "/run/lock/gpu.lock";

/// The queue directory of the fair lock protocol: one ticket file per waiter, served in order of
/// arrival ([`Ticket`]; plan `docs/plans/2026-11-17-gpu-test-readback.md`, Amendment 5, B-136).
///
/// The harness queues there only when the host's `with-gpu-lock` names this directory
/// ([`shared_queue`]), so both kinds of taker start queueing together.
pub const GPU_QUEUE_DIR: &str = "/run/lock/gpu.queue";

/// The log `with-gpu-lock` appends its `ACQUIRE`, `RELEASE` and `TIMEOUT` lines to (Q0060).
pub const LOCK_TOOL_LOG: &str = "/run/lock/gpu-lock.log";

/// Default wait for the shared lock, in seconds (4 min 50 s), when `GPU_LOCK_TIMEOUT` is unset.
///
/// The owner's decision in `docs/questions/closed/0012-gpu-lock-peer-and-wait-limit.md`: every
/// user of the GPU now locks per device use, so a wait should last seconds; 290 s tolerates a
/// slow peer while failing a gate that is stuck behind a suite-wide hold. A single long command
/// that needs more sets `GPU_LOCK_TIMEOUT`.
pub const DEFAULT_LOCK_WAIT_SECS: u64 = 290;

/// Variable that overrides [`DEFAULT_LOCK_WAIT_SECS`], in whole seconds, as for `with-gpu-lock`.
pub const LOCK_WAIT_VAR: &str = "GPU_LOCK_TIMEOUT";

/// Interval between two looks at the queue, or two attempts to take a busy lock.
///
/// Without the queue, `with-gpu-lock` peers block in the kernel and are woken the moment the lock
/// is released, so a harness that polls lets a waiting peer win the next slot (plan decision D2).
/// That courtesy is what lets a series of short holds starve a test child (B-136); the queue
/// ([`GPU_QUEUE_DIR`]) replaces it with arrival order.
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Variable that forces a driver: `radv`, `lavapipe` or `auto`. Unset means `auto`.
pub const DRIVER_VAR: &str = "SPACE_SCUFFLE_GPU_TEST_DRIVER";

/// Variable naming a file that receives one line per child run (plan decision D6).
pub const LEDGER_VAR: &str = "SPACE_SCUFFLE_GPU_RUN_LEDGER";

/// Variable naming a directory that receives each child's whole log, for timing investigations.
pub const RUN_LOG_DIR_VAR: &str = "SPACE_SCUFFLE_GPU_RUN_LOG_DIR";

/// Program whose presence on `PATH` declares that a host runs the shared lock protocol.
pub const LOCK_TOOL: &str = "with-gpu-lock";

/// Installed manifests of the `radv` driver, in the order they are tried.
const RADV_MANIFESTS: [&str; 2] = [
    "/usr/share/vulkan/icd.d/radeon_icd.json",
    "/usr/share/vulkan/icd.d/radeon_icd.x86_64.json",
];

/// Installed manifests of the `lavapipe` driver, in the order they are tried.
const LAVAPIPE_MANIFESTS: [&str; 2] = [
    "/usr/share/vulkan/icd.d/lvp_icd.json",
    "/usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
];

/// Directory of the DRM device nodes.
const DRM_DIRECTORY: &str = "/dev/dri";

/// Name prefix of a DRM render node, such as `renderD128`.
const RENDER_NODE_PREFIX: &str = "renderD";

/// Run limit on `radv`, in seconds: about three times the slowest child measured on the RX 580
/// (39.8 s, the windowed skirmish; plan section 3.2), so a hung child holds the GPU at most
/// 142 s: the limit, [`KILL_GRACE_SECS`], [`GROUP_EXIT_GRACE`], [`GROUP_KILL_WAIT`] and
/// [`OUTPUT_DRAIN_SECS`] ([`run_to_end`]).
const RADV_RUN_LIMIT_SECS: u32 = 120;

/// Run limit on `lavapipe`, in seconds.
///
/// Debug builds on lavapipe, on 16 cores of a loaded EPYC 7301, take up to 117.5 s for
/// `client --swarm` and about 80 s for `map_editor --verify-render` (plan section 3.5). The limit
/// leaves room for a machine shared with other work.
const LAVAPIPE_RUN_LIMIT_SECS: u32 = 300;

/// Run limit of a lock self-test child, in seconds. Those children only run `sh` and `flock`.
const LOCK_PROBE_RUN_LIMIT_SECS: u32 = 60;

/// Seconds GNU `timeout` waits after its signal before it kills the child outright.
pub const KILL_GRACE_SECS: u32 = 10;

/// Seconds the harness waits, once the run's process group is gone, for the child's output pipes
/// to close ([`run_to_end`]). Only a process that left the group can still hold them open.
pub const OUTPUT_DRAIN_SECS: u64 = 5;

/// How long the run's process group may take to empty on its own after `timeout` exits, before
/// the harness kills it ([`run_to_end`]). `xvfb-run` signals Xvfb and exits without waiting, and
/// Xvfb then removes its lock file and socket; a kill at once would leave both behind.
pub const GROUP_EXIT_GRACE: Duration = Duration::from_secs(2);

/// How long the harness waits, after its `SIGKILL`, for every process of the group to be gone
/// before it releases the lock anyway and fails the run ([`run_to_end`]). Only a process stuck in
/// the kernel (uninterruptible sleep in a driver) outlives a `SIGKILL` that long.
pub const GROUP_KILL_WAIT: Duration = Duration::from_secs(5);

/// How often [`run_to_end`] looks at `timeout` and at its process group.
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Terrain sampler override read by `client::terrain::theme_gpu`.
const TERRAIN_ANISOTROPY_OVERRIDE_ENV: &str = "SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY";

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

/// `RUST_LOG` directive that keeps Bevy's `AdapterInfo` line.
const ADAPTER_LOG_DIRECTIVE: &str = "bevy_render::renderer=info";

/// `RUST_LOG` for validated runs. The `info` level for `wgpu_hal` keeps [`LAYER_ACTIVE_MARKER`],
/// and the one for `bevy_render::renderer` keeps the adapter line.
const VALIDATED_RUST_LOG: &str = "warn,wgpu_hal=info,bevy_render::renderer=info";

/// Text that starts Bevy's adapter record.
const ADAPTER_MARKER: &str = "AdapterInfo { name: \"";

// ---------------------------------------------------------------------------------------------
// Driver selection (plan decision D1)
// ---------------------------------------------------------------------------------------------

/// The Vulkan driver a GPU test child runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuDriver {
    /// Mesa's driver for the physical AMD GPU. Takes the shared lock.
    Radv,
    /// Mesa's software Vulkan driver (`llvmpipe`). Takes no lock.
    Lavapipe,
}

impl GpuDriver {
    /// The name used in `SPACE_SCUFFLE_GPU_TEST_DRIVER`, messages and the ledger.
    pub fn name(self) -> &'static str {
        match self {
            Self::Radv => "radv",
            Self::Lavapipe => "lavapipe",
        }
    }

    /// The `driver` field of the `AdapterInfo` the driver reports to wgpu.
    pub fn adapter_driver(self) -> &'static str {
        match self {
            Self::Radv => "radv",
            Self::Lavapipe => "llvmpipe",
        }
    }

    /// The run limit of one child on this driver, in seconds (plan decision D4).
    pub fn run_limit_secs(self) -> u32 {
        match self {
            Self::Radv => RADV_RUN_LIMIT_SECS,
            Self::Lavapipe => LAVAPIPE_RUN_LIMIT_SECS,
        }
    }
}

impl fmt::Display for GpuDriver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// What `SPACE_SCUFFLE_GPU_TEST_DRIVER` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverRequest {
    /// `radv` when the host has every prerequisite, `lavapipe` otherwise.
    Auto,
    /// `radv` or an error; never a fallback.
    Radv,
    /// `lavapipe` with no lock, exactly as CI runs.
    Lavapipe,
}

impl DriverRequest {
    /// Parses the variable's value; `None` (unset) means [`Self::Auto`].
    ///
    /// # Errors
    /// [`DriverSelectionError::InvalidRequest`] for any value but `auto`, `radv` and `lavapipe`.
    pub fn parse(value: Option<&str>) -> Result<Self, DriverSelectionError> {
        match value {
            None | Some("auto") => Ok(Self::Auto),
            Some("radv") => Ok(Self::Radv),
            Some("lavapipe") => Ok(Self::Lavapipe),
            Some(other) => Err(DriverSelectionError::InvalidRequest(other.to_owned())),
        }
    }
}

/// The first DRM render node of the host, and whether this user may use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderNode {
    /// `/dev/dri` has no `renderD*` node.
    Missing,
    /// The node exists, but this user cannot open it for reading and writing.
    Inaccessible(PathBuf),
    /// The node exists and opened for reading and writing.
    Usable(PathBuf),
}

/// What the host offers a GPU test, as found by [`HostGpu::probe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostGpu {
    /// The installed `radv` manifest, if any.
    pub radv_manifest: Option<PathBuf>,
    /// The first render node in name order.
    pub render_node: RenderNode,
    /// `with-gpu-lock` on `PATH`, if found.
    pub lock_tool: Option<PathBuf>,
    /// The installed `lavapipe` manifest, if any.
    pub lavapipe_manifest: Option<PathBuf>,
}

/// A host prerequisite of `radv` that is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingPrerequisite {
    /// Neither `radeon_icd.json` nor `radeon_icd.x86_64.json` is installed.
    RadvManifest,
    /// No `/dev/dri/renderD*` node exists.
    RenderNode,
    /// The render node exists, but this user cannot open it for reading and writing.
    RenderNodeAccess(PathBuf),
    /// `with-gpu-lock` is not on `PATH`, so the host does not declare the lock protocol.
    LockTool,
}

impl fmt::Display for MissingPrerequisite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RadvManifest => write!(
                formatter,
                "no radv manifest ({})",
                RADV_MANIFESTS.join(" or ")
            ),
            Self::RenderNode => write!(formatter, "no render node {DRM_DIRECTORY}/renderD*"),
            Self::RenderNodeAccess(node) => write!(
                formatter,
                "{} cannot be opened for reading and writing by this user",
                node.display()
            ),
            Self::LockTool => write!(formatter, "{LOCK_TOOL} is not on PATH"),
        }
    }
}

impl HostGpu {
    /// Inspects the real host: manifests, `/dev/dri` and `PATH`. Touches no GPU state beyond
    /// opening and closing the render node.
    pub fn probe() -> Self {
        Self {
            radv_manifest: first_existing(&RADV_MANIFESTS),
            render_node: probe_render_node(Path::new(DRM_DIRECTORY)),
            lock_tool: find_on_path(LOCK_TOOL),
            lavapipe_manifest: first_existing(&LAVAPIPE_MANIFESTS),
        }
    }

    /// Lists every `radv` prerequisite this host lacks. Empty means `auto` selects `radv`.
    pub fn missing_radv_prerequisites(&self) -> Vec<MissingPrerequisite> {
        let mut missing = Vec::new();
        if self.radv_manifest.is_none() {
            missing.push(MissingPrerequisite::RadvManifest);
        }
        match &self.render_node {
            RenderNode::Missing => missing.push(MissingPrerequisite::RenderNode),
            RenderNode::Inaccessible(node) => {
                missing.push(MissingPrerequisite::RenderNodeAccess(node.clone()));
            }
            RenderNode::Usable(_) => {}
        }
        if self.lock_tool.is_none() {
            missing.push(MissingPrerequisite::LockTool);
        }
        missing
    }
}

/// The driver a test process uses, with the manifest it loads and why it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuSelection {
    /// The selected driver.
    pub driver: GpuDriver,
    /// The Vulkan manifest every child receives as `VK_ICD_FILENAMES`.
    pub manifest: PathBuf,
    /// How the choice was made, such as `forced` or the `auto` evidence.
    pub reason: String,
}

impl fmt::Display for GpuSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "driver {} ({})", self.driver, self.reason)
    }
}

/// Why no driver could be selected. Each variant fails the test; there is no fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverSelectionError {
    /// The variable holds a value other than `auto`, `radv` and `lavapipe`.
    InvalidRequest(String),
    /// `radv` was forced, and these prerequisites are missing.
    RadvUnavailable(Vec<MissingPrerequisite>),
    /// `lavapipe` was forced, and no `lavapipe` manifest is installed.
    LavapipeMissing,
    /// `auto` found neither every `radv` prerequisite nor a `lavapipe` manifest.
    NoDriver(Vec<MissingPrerequisite>),
}

impl fmt::Display for DriverSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |missing: &[MissingPrerequisite]| {
            missing
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        };
        match self {
            Self::InvalidRequest(value) => write!(
                formatter,
                "{DRIVER_VAR}={value:?} is not a driver; use radv, lavapipe or auto"
            ),
            Self::RadvUnavailable(missing) => write!(
                formatter,
                "{DRIVER_VAR}=radv, but this host cannot run GPU tests on radv: {}. \
                 The harness never falls back to another driver.",
                list(missing)
            ),
            Self::LavapipeMissing => write!(
                formatter,
                "{DRIVER_VAR}=lavapipe, but no lavapipe manifest is installed ({}); install \
                 mesa-vulkan-drivers",
                LAVAPIPE_MANIFESTS.join(" or ")
            ),
            Self::NoDriver(missing) => write!(
                formatter,
                "no GPU test driver: radv is unavailable ({}) and no lavapipe manifest is \
                 installed ({}). Install mesa-vulkan-drivers, or set \
                 SPACE_SCUFFLE_SKIP_GPU_TESTS=1 locally.",
                list(missing),
                LAVAPIPE_MANIFESTS.join(" or ")
            ),
        }
    }
}

/// Chooses the driver for `request` on `host`. A pure function: it reads nothing but its inputs.
///
/// # Errors
/// [`DriverSelectionError`] when the requested driver is not available. A forced driver never
/// falls back to the other one.
pub fn select_driver(
    request: DriverRequest,
    host: &HostGpu,
) -> Result<GpuSelection, DriverSelectionError> {
    let missing = host.missing_radv_prerequisites();
    let radv = |reason: String| -> Option<GpuSelection> {
        host.radv_manifest.as_ref().map(|manifest| GpuSelection {
            driver: GpuDriver::Radv,
            manifest: manifest.clone(),
            reason,
        })
    };
    let lavapipe = |reason: &str| -> Option<GpuSelection> {
        host.lavapipe_manifest
            .as_ref()
            .map(|manifest| GpuSelection {
                driver: GpuDriver::Lavapipe,
                manifest: manifest.clone(),
                reason: reason.to_owned(),
            })
    };
    match request {
        DriverRequest::Radv if missing.is_empty() => {
            radv("forced".to_owned()).ok_or(DriverSelectionError::RadvUnavailable(missing))
        }
        DriverRequest::Radv => Err(DriverSelectionError::RadvUnavailable(missing)),
        DriverRequest::Lavapipe => lavapipe("forced").ok_or(DriverSelectionError::LavapipeMissing),
        DriverRequest::Auto if missing.is_empty() => {
            radv(auto_radv_evidence(host)).ok_or(DriverSelectionError::NoDriver(missing))
        }
        DriverRequest::Auto => {
            let reason = format!(
                "auto: radv unavailable, {}",
                missing
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            lavapipe(&reason).ok_or(DriverSelectionError::NoDriver(missing))
        }
    }
}

/// The evidence line of an `auto` choice of `radv`, such as
/// `auto: radeon_icd.json, /dev/dri/renderD128, with-gpu-lock on PATH`.
fn auto_radv_evidence(host: &HostGpu) -> String {
    let manifest = host
        .radv_manifest
        .as_deref()
        .and_then(Path::file_name)
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let node = match &host.render_node {
        RenderNode::Usable(node) | RenderNode::Inaccessible(node) => node.display().to_string(),
        RenderNode::Missing => String::new(),
    };
    format!("auto: {manifest}, {node}, {LOCK_TOOL} on PATH")
}

/// The driver of this test process, chosen once from [`DRIVER_VAR`] and [`HostGpu::probe`].
///
/// Prints one line naming the choice to stderr the first time.
///
/// # Panics
/// Panics with the [`DriverSelectionError`] when no driver can be selected.
pub fn selected_driver() -> &'static GpuSelection {
    static SELECTION: OnceLock<Result<GpuSelection, DriverSelectionError>> = OnceLock::new();
    let selection = SELECTION.get_or_init(|| {
        let value = std::env::var(DRIVER_VAR).ok();
        let selection = DriverRequest::parse(value.as_deref())
            .and_then(|request| select_driver(request, &HostGpu::probe()));
        if let Ok(selection) = &selection {
            eprintln!("gpu_run: {selection}");
        }
        selection
    });
    match selection {
        Ok(selection) => selection,
        Err(error) => panic!("gpu_run: {error}"),
    }
}

/// Returns the first of `paths` that exists.
fn first_existing(paths: &[&str]) -> Option<PathBuf> {
    paths.iter().map(PathBuf::from).find(|path| path.exists())
}

/// Finds the first render node in `directory` in name order, and tries to open it read-write.
fn probe_render_node(directory: &Path) -> RenderNode {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return RenderNode::Missing;
    };
    let mut nodes: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(RENDER_NODE_PREFIX)
        })
        .map(|entry| entry.path())
        .collect();
    nodes.sort();
    match nodes.into_iter().next() {
        None => RenderNode::Missing,
        Some(node) => {
            if OpenOptions::new()
                .read(true)
                .write(true)
                .open(&node)
                .is_ok()
            {
                RenderNode::Usable(node)
            } else {
                RenderNode::Inaccessible(node)
            }
        }
    }
}

/// Finds an executable file called `program` in a directory of `PATH`.
pub fn find_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| is_executable(candidate))
}

/// Returns `true` for a regular file with an execute bit.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

/// Returns `true` for a regular file. Without Unix permissions every file counts.
#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

// ---------------------------------------------------------------------------------------------
// The shared lock (plan decision D2)
// ---------------------------------------------------------------------------------------------

/// The process that holds a lock, as `/proc/locks` and `/proc/<pid>/cmdline` name it.
///
/// `/proc/locks` names the process that placed the lock, which need not be the one holding it
/// now: `with-gpu-lock` opens the file in its shell and runs `flock 200`, so the PID listed is that
/// short-lived `flock(1)` helper, gone by the time anyone looks, while the lock lives on in the
/// shell's descriptor (found while diagnosing B-136). [`Self::running`] tells the two apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockHolder {
    /// Process ID that placed the lock, or the ancestor that has the lock file open.
    pub pid: u32,
    /// Its command line, arguments joined by spaces, or empty if unreadable.
    pub command_line: String,
    /// Whether that process still ran when it was looked up.
    pub running: bool,
}

impl fmt::Display for LockHolder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.running {
            write!(formatter, "PID {}: `{}`", self.pid, self.command_line)
        } else {
            write!(
                formatter,
                "PID {}, which no longer runs: the lock lives on in a descriptor it handed on, as \
                 the `flock(1)` helper of `{LOCK_TOOL}` leaves it with the script's shell",
                self.pid
            )
        }
    }
}

/// Why the GPU lock was not taken.
#[derive(Debug)]
pub enum GpuLockError {
    /// The lock file could not be opened or created.
    Open {
        /// The lock file.
        path: PathBuf,
        /// The operating system's reason.
        source: std::io::Error,
    },
    /// Another process held the lock for the whole wait.
    TimedOut {
        /// The lock file.
        path: PathBuf,
        /// How long the harness waited.
        waited: Duration,
        /// The holder at expiry, when `/proc/locks` names one.
        holder: Option<LockHolder>,
        /// The last `ACQUIRE` line of [`LOCK_TOOL_LOG`] with no `RELEASE` after it, when the lock
        /// is the shared one and the log shows such a hold ([`open_tool_holds`]).
        open_tool_hold: Option<String>,
    },
    /// The queue directory could not be created, read or written ([`GPU_QUEUE_DIR`]).
    Queue {
        /// The queue directory.
        dir: PathBuf,
        /// The operating system's reason.
        source: std::io::Error,
    },
    /// An ancestor of this process holds the lock, so waiting could never succeed.
    HeldByAncestor {
        /// The lock file.
        path: PathBuf,
        /// The ancestor that holds it.
        holder: LockHolder,
    },
    /// `flock(2)` failed for a reason other than contention.
    Io {
        /// The lock file.
        path: PathBuf,
        /// The operating system's reason.
        source: std::io::Error,
    },
}

impl fmt::Display for GpuLockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { path, source } => {
                write!(
                    formatter,
                    "cannot open the GPU lock {}: {source}",
                    path.display()
                )
            }
            Self::TimedOut {
                path,
                waited,
                holder,
                open_tool_hold,
            } => {
                write!(
                    formatter,
                    "the GPU lock {} stayed busy for {:.1} s",
                    path.display(),
                    waited.as_secs_f64()
                )?;
                if let Some(holder) = holder {
                    write!(formatter, ", held by {holder}")?;
                }
                if let Some(line) = open_tool_hold {
                    write!(
                        formatter,
                        "; {LOCK_TOOL_LOG} shows a hold with no release: {line}"
                    )?;
                }
                write!(
                    formatter,
                    ". Raise {LOCK_WAIT_VAR} to wait longer, or set {DRIVER_VAR}=lavapipe to run \
                     the GPU tests without the GPU. The harness never falls back by itself."
                )
            }
            Self::HeldByAncestor { path, holder } => write!(
                formatter,
                "the GPU lock {} is held by an ancestor process ({holder}); run `cargo test` \
                 without {LOCK_TOOL}, because the harness locks each GPU child itself",
                path.display()
            ),
            Self::Io { path, source } => {
                write!(formatter, "cannot lock {}: {source}", path.display())
            }
            Self::Queue { dir, source } => write!(
                formatter,
                "cannot queue for the GPU lock in {}: {source}",
                dir.display()
            ),
        }
    }
}

impl std::error::Error for GpuLockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } | Self::Io { source, .. } | Self::Queue { source, .. } => {
                Some(source)
            }
            Self::TimedOut { .. } | Self::HeldByAncestor { .. } => None,
        }
    }
}

/// An exclusive `flock(2)` on a lock file. Dropping it closes the descriptor, which releases it.
#[derive(Debug)]
pub struct GpuLock {
    /// The open lock file; the lock lives exactly as long as this descriptor.
    _file: File,
}

impl GpuLock {
    /// Takes the exclusive lock on `path` with no queue, retrying every [`LOCK_POLL_INTERVAL`]
    /// for up to `wait`. As [`Self::acquire_queued`] with no queue.
    ///
    /// # Errors
    /// As [`Self::acquire_queued`].
    pub fn acquire(path: &Path, wait: Duration) -> Result<(Self, Duration), GpuLockError> {
        Self::acquire_queued(path, None, wait)
    }

    /// Takes the exclusive lock on `path`, in turn when `queue` names a queue directory, waiting
    /// up to `wait` in all. Returns the guard and the time spent waiting.
    ///
    /// Opens the file for reading and writing, creating it with mode 0666 (subject to the umask)
    /// as `flock(1)` does. Before it waits, it fails at once if an ancestor of this process holds
    /// the lock ([`ancestor_holder`]), since that wait could never end. With a queue, it puts a
    /// [`Ticket`] in the directory and tries the lock only while that ticket is the queue's head
    /// ([`queue_head`]), every [`LOCK_POLL_INTERVAL`]; it removes the ticket once it holds the
    /// lock, or when it gives up. Without one, it tries the lock every [`LOCK_POLL_INTERVAL`].
    ///
    /// # Errors
    /// [`GpuLockError`]: the file cannot be opened, an ancestor holds the lock, the queue cannot
    /// be used, `wait` expired, or `flock(2)` failed.
    pub fn acquire_queued(
        path: &Path,
        queue: Option<&Path>,
        wait: Duration,
    ) -> Result<(Self, Duration), GpuLockError> {
        let file = open_lock_file(path).map_err(|source| GpuLockError::Open {
            path: path.to_owned(),
            source,
        })?;
        let started = Instant::now();
        if let Some(holder) = ancestor_holder(&file) {
            return Err(GpuLockError::HeldByAncestor {
                path: path.to_owned(),
                holder,
            });
        }
        let queue_error = |dir: &Path| {
            let dir = dir.to_owned();
            move |source| GpuLockError::Queue { dir, source }
        };
        let ticket = match queue {
            Some(dir) => Some(QueueTicket::join(dir, wait).map_err(queue_error(dir))?),
            None => None,
        };
        loop {
            let in_turn = match &ticket {
                Some(ticket) => ticket.is_head().map_err(queue_error(&ticket.dir))?,
                None => true,
            };
            if in_turn {
                match file.try_lock() {
                    Ok(()) => {
                        // The ticket goes once the lock is held, so the next waiter becomes the
                        // head and waits at the lock itself for this hold to end.
                        drop(ticket);
                        return Ok((Self { _file: file }, started.elapsed()));
                    }
                    Err(TryLockError::WouldBlock) => {}
                    Err(TryLockError::Error(source)) => {
                        return Err(GpuLockError::Io {
                            path: path.to_owned(),
                            source,
                        })
                    }
                }
            }
            let waited = started.elapsed();
            if waited >= wait {
                let open_tool_hold = (path == Path::new(GPU_LOCK_PATH))
                    .then(|| std::fs::read_to_string(LOCK_TOOL_LOG).ok())
                    .flatten()
                    .and_then(|log| open_tool_holds(&log).pop());
                return Err(GpuLockError::TimedOut {
                    path: path.to_owned(),
                    waited,
                    holder: lock_holder(&file),
                    open_tool_hold,
                });
            }
            std::thread::sleep(LOCK_POLL_INTERVAL.min(wait.saturating_sub(waited)));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The fair queue (plan `2026-11-17-gpu-test-readback`, Amendment 5; B-136)
// ---------------------------------------------------------------------------------------------

/// One waiter's place in the queue for the GPU lock: an empty file in the queue directory whose
/// name carries everything a peer needs, so the queue is read with one directory listing.
///
/// The name is `<arrival_ns>.<pid>.<start_ticks>.<deadline_s>`, with the arrival zero-padded to
/// 20 digits, so that the byte order of the names is the order of arrival; ties (the same
/// nanosecond) go by the rest of the name, byte-wise. That byte order is the queue order, for this
/// harness ([`Ord`]) and for `with-gpu-lock`'s sorted glob alike. A ticket counts only while it is live
/// ([`Self::is_live`]): its process runs, with the same start time, and its deadline has not
/// passed. Every taker reads the same rule, so a dead, reused or expired ticket blocks nobody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticket {
    /// Wall-clock time the waiter joined, in nanoseconds since the Unix epoch.
    pub arrival_ns: u128,
    /// Process ID of the waiter (the process that will hold the lock).
    pub pid: u32,
    /// The waiter's start time, in clock ticks since boot (`/proc/<pid>/stat` field 22), which
    /// tells a reused PID from the waiter.
    pub start_ticks: u64,
    /// The second since the Unix epoch after which the waiter has given up.
    pub deadline_s: u64,
}

impl Ord for Ticket {
    /// The byte order of the file names: arrival first, then the rest of the name.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.file_name().cmp(&other.file_name())
    }
}

impl PartialOrd for Ticket {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ticket {
    /// The ticket's file name in the queue directory.
    pub fn file_name(&self) -> String {
        format!(
            "{:020}.{}.{}.{}",
            self.arrival_ns, self.pid, self.start_ticks, self.deadline_s
        )
    }

    /// Reads a ticket file name; `None` for any other name.
    pub fn parse(name: &str) -> Option<Self> {
        let mut fields = name.split('.');
        let ticket = Self {
            arrival_ns: fields.next()?.parse().ok()?,
            pid: fields.next()?.parse().ok()?,
            start_ticks: fields.next()?.parse().ok()?,
            deadline_s: fields.next()?.parse().ok()?,
        };
        fields.next().is_none().then_some(ticket)
    }

    /// Whether the ticket still waits at `now_s` (seconds since the epoch): before its deadline,
    /// and its PID runs with its start time, as `start_ticks_of` reports it.
    pub fn is_live(&self, now_s: u64, start_ticks_of: impl Fn(u32) -> Option<u64>) -> bool {
        now_s < self.deadline_s && start_ticks_of(self.pid) == Some(self.start_ticks)
    }
}

/// The head of a queue: the earliest live ticket among the file `names`, ignoring names that are
/// not tickets. `None` when no ticket is live.
pub fn queue_head<S: AsRef<str>>(
    names: impl IntoIterator<Item = S>,
    now_s: u64,
    start_ticks_of: impl Fn(u32) -> Option<u64>,
) -> Option<Ticket> {
    names
        .into_iter()
        .filter_map(|name| Ticket::parse(name.as_ref()))
        .filter(|ticket| ticket.is_live(now_s, &start_ticks_of))
        .min()
}

/// Reads a process's start time in clock ticks since boot (field 22) from the text of
/// `/proc/<pid>/stat`. The fields are counted after the last `)`, as in [`stat_parent_pid`].
pub fn stat_start_ticks(stat: &str) -> Option<u64> {
    let (_, after_name) = stat.rsplit_once(')')?;
    after_name.split_whitespace().nth(19)?.parse().ok()
}

/// The start time of process `pid`, or `None` when it does not run (or `/proc` is absent).
fn process_start_ticks(pid: u32) -> Option<u64> {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .as_deref()
        .and_then(stat_start_ticks)
}

/// Seconds since the Unix epoch, now.
fn epoch_now() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
}

/// This process's ticket file in a queue directory; dropping it removes the file.
#[derive(Debug)]
struct QueueTicket {
    /// The queue directory.
    dir: PathBuf,
    /// The ticket.
    ticket: Ticket,
}

impl QueueTicket {
    /// Joins the queue in `dir` for a wait of up to `wait`, creating the directory (mode 1777, so
    /// every user of the GPU can queue) if it is absent.
    fn join(dir: &Path, wait: Duration) -> std::io::Result<Self> {
        ensure_queue_dir(dir)?;
        let pid = std::process::id();
        let start_ticks = process_start_ticks(pid).ok_or_else(|| {
            std::io::Error::other("cannot read this process's start time from /proc/self/stat")
        })?;
        loop {
            let now = epoch_now();
            // Two seconds past the wait (one for the whole seconds cut off, one of margin), so
            // the ticket outlives every check this waiter makes before it gives up.
            let deadline_s = now.as_secs() + wait.as_secs() + 2;
            let ticket = Ticket {
                arrival_ns: now.as_nanos(),
                pid,
                start_ticks,
                deadline_s,
            };
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(ticket.file_name()))
            {
                Ok(_) => {
                    return Ok(Self {
                        dir: dir.to_owned(),
                        ticket,
                    })
                }
                // Another thread of this process joined in the same nanosecond: take the next.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// Whether this ticket is the queue's head now. Removes, on the way, the tickets that are no
    /// longer live and belong to this user ([`Self::sweep`]).
    fn is_head(&self) -> std::io::Result<bool> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            names.push(entry?.file_name().to_string_lossy().into_owned());
        }
        let now_s = epoch_now().as_secs();
        self.sweep(&names, now_s);
        let head = queue_head(names, now_s, process_start_ticks);
        Ok(head == Some(self.ticket))
    }

    /// Removes the tickets among `names` that are not live at `now_s` and are owned by the user
    /// who owns this ticket: a waiter of this user that crashed leaves one behind. The sticky
    /// queue directory lets only a ticket's owner remove it, so another user's stale tickets stay
    /// and are ignored. A removal that fails (a peer of this user removed it first) is harmless.
    #[cfg(unix)]
    fn sweep(&self, names: &[String], now_s: u64) {
        use std::os::unix::fs::MetadataExt as _;
        let Ok(own) = std::fs::metadata(self.dir.join(self.ticket.file_name())) else {
            return;
        };
        for name in names {
            let Some(ticket) = Ticket::parse(name) else {
                continue;
            };
            if ticket.is_live(now_s, process_start_ticks) {
                continue;
            }
            let path = self.dir.join(name);
            if std::fs::metadata(&path).is_ok_and(|metadata| metadata.uid() == own.uid()) {
                // Losing the race to a peer that removed it first is the only expected failure.
                std::fs::remove_file(&path).ok();
            }
        }
    }

    /// Without Unix ownership no ticket is known to be this user's.
    #[cfg(not(unix))]
    fn sweep(&self, _names: &[String], _now_s: u64) {}
}

impl Drop for QueueTicket {
    /// Removes the ticket file. A failure cannot be reported from `drop`; a ticket left behind
    /// expires at its deadline and is ignored by every peer from then on.
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(self.dir.join(self.ticket.file_name())) {
            eprintln!(
                "cannot remove the GPU queue ticket {} in {}: {error}; it expires at its deadline",
                self.ticket.file_name(),
                self.dir.display()
            );
        }
    }
}

/// Creates the queue directory with mode 1777 (world-writable, sticky like `/run/lock`) if it is
/// absent, and checks that it is a directory and not a symbolic link.
fn ensure_queue_dir(dir: &Path) -> std::io::Result<()> {
    match std::fs::create_dir(dir) {
        Ok(()) => set_queue_mode(dir)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = std::fs::symlink_metadata(dir)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "{} is not a directory",
            dir.display()
        )))
    }
}

/// Sets mode 1777 on a queue directory this process created; the umask narrowed `create_dir`'s.
#[cfg(unix)]
fn set_queue_mode(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o1777))
}

/// Without Unix modes the directory keeps its default permissions.
#[cfg(not(unix))]
fn set_queue_mode(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Whether a `with-gpu-lock` script takes part in the queue: it names [`GPU_QUEUE_DIR`].
pub fn tool_declares_queue(script: &str) -> bool {
    script.contains(GPU_QUEUE_DIR)
}

/// The queue the shared lock uses: [`GPU_QUEUE_DIR`] when the host's `with-gpu-lock` declares
/// it ([`tool_declares_queue`]), else none. Read once per test process.
pub fn shared_queue() -> Option<&'static Path> {
    static DECLARED: OnceLock<bool> = OnceLock::new();
    let declared = *DECLARED.get_or_init(|| {
        find_on_path(LOCK_TOOL)
            .and_then(|tool| std::fs::read_to_string(tool).ok())
            .is_some_and(|script| tool_declares_queue(&script))
    });
    declared.then_some(Path::new(GPU_QUEUE_DIR))
}

/// The `ACQUIRE` lines of a `with-gpu-lock` log with no `RELEASE` or `TIMEOUT` of the same PID
/// after them, in log order. The last one is the hold most likely still running; one whose
/// script was killed before it could log its release stays open here too.
pub fn open_tool_holds(log: &str) -> Vec<String> {
    let mut open: Vec<(String, &str)> = Vec::new();
    for line in log.lines() {
        let mut words = line.split_whitespace();
        let (Some(_time), Some(event)) = (words.next(), words.next()) else {
            continue;
        };
        let Some(pid) = words.find_map(|word| word.strip_prefix("pid=")) else {
            continue;
        };
        match event {
            "ACQUIRE" => open.push((pid.to_owned(), line)),
            "RELEASE" | "TIMEOUT" => open.retain(|(held, _)| held != pid),
            _ => {}
        }
    }
    open.into_iter().map(|(_, line)| line.to_owned()).collect()
}

/// Opens `path` read-write, creating it with mode 0666 if it is absent.
#[cfg(unix)]
fn open_lock_file(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o666)
        .open(path)
}

/// Opens `path` read-write, creating it if it is absent.
#[cfg(not(unix))]
fn open_lock_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

/// The wait for the shared lock: [`LOCK_WAIT_VAR`] in seconds, or [`DEFAULT_LOCK_WAIT_SECS`].
///
/// # Panics
/// Panics when the variable is set but is not a whole number of seconds.
pub fn shared_lock_wait() -> Duration {
    match std::env::var(LOCK_WAIT_VAR) {
        Err(_) => Duration::from_secs(DEFAULT_LOCK_WAIT_SECS),
        Ok(text) => Duration::from_secs(text.trim().parse().unwrap_or_else(|_| {
            panic!("{LOCK_WAIT_VAR}={text:?} is not a whole number of seconds")
        })),
    }
}

/// The process holding `file`'s lock, read from `/proc/locks`, or `None` if it cannot be named.
fn lock_holder(file: &File) -> Option<LockHolder> {
    let (major, minor, inode) = device_and_inode(file)?;
    let locks = std::fs::read_to_string("/proc/locks").ok()?;
    let pid = flock_holder_pid(&locks, major, minor, inode)?;
    Some(describe_process(pid))
}

/// Names process `pid`: its command line, and whether it runs.
fn describe_process(pid: u32) -> LockHolder {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline"));
    LockHolder {
        pid,
        running: raw.is_ok(),
        command_line: raw.map_or_default(|raw| command_line_text(&raw)),
    }
}

/// The ancestor of this process that holds `file`'s lock, if the lock is held and one does.
///
/// The lock is held by an ancestor when `/proc/locks` names one as its holder, as for
/// `flock <file> cargo test`, or when an ancestor has the lock file open, as for
/// `with-gpu-lock cargo test`: that script locks through a `flock(1)` helper that exits at once
/// and leaves the lock in the shell's descriptor, so `/proc/locks` names a process that is gone.
/// Descriptors are read from `/proc/<pid>/fd`, which only the same user can read; the ancestors
/// of a test process are that user's.
fn ancestor_holder(file: &File) -> Option<LockHolder> {
    let (major, minor, inode) = device_and_inode(file)?;
    let locks = std::fs::read_to_string("/proc/locks").ok()?;
    let placed_by = flock_holder_pid(&locks, major, minor, inode)?;
    let ancestors = strict_ancestors();
    if ancestors.contains(&placed_by) {
        return Some(describe_process(placed_by));
    }
    ancestors
        .into_iter()
        .find(|&pid| has_open(pid, (major, minor, inode)))
        .map(describe_process)
}

/// Whether process `pid` has a descriptor open on the file `(major, minor, inode)`.
#[cfg(unix)]
fn has_open(pid: u32, (major, minor, inode): (u64, u64, u64)) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    entries.flatten().any(|entry| {
        std::fs::metadata(entry.path()).is_ok_and(|metadata| {
            metadata.ino() == inode && split_device(metadata.dev()) == (major, minor)
        })
    })
}

/// Without `/proc` no descriptor can be seen.
#[cfg(not(unix))]
fn has_open(_pid: u32, _file: (u64, u64, u64)) -> bool {
    false
}

/// The `(major, minor, inode)` of `file`, as `/proc/locks` prints them.
#[cfg(unix)]
fn device_and_inode(file: &File) -> Option<(u64, u64, u64)> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = file.metadata().ok()?;
    let (major, minor) = split_device(metadata.dev());
    Some((major, minor, metadata.ino()))
}

/// Without Unix metadata the holder cannot be found.
#[cfg(not(unix))]
fn device_and_inode(_file: &File) -> Option<(u64, u64, u64)> {
    None
}

/// Splits a Linux `st_dev` into `(major, minor)`, as glibc's `major` and `minor` macros do.
pub fn split_device(device: u64) -> (u64, u64) {
    let major = ((device & 0x0000_0000_000f_ff00) >> 8) | ((device & 0xffff_f000_0000_0000) >> 32);
    let minor = (device & 0x0000_0000_0000_00ff) | ((device & 0x0000_0fff_fff0_0000) >> 12);
    (major, minor)
}

/// Finds the PID holding a `FLOCK` on the file `major:minor:inode` in `/proc/locks` text.
///
/// Lines look like `3: FLOCK  ADVISORY  WRITE 2533986 08:10:87952619 0 EOF`, with the device in
/// hexadecimal and the inode in decimal. Waiters (`3: -> FLOCK ...`) and other lock kinds are
/// skipped.
pub fn flock_holder_pid(locks: &str, major: u64, minor: u64, inode: u64) -> Option<u32> {
    locks.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let _ordinal = fields.next()?;
        if fields.next()? != "FLOCK" {
            return None;
        }
        let pid: u32 = fields.nth(2)?.parse().ok()?;
        let mut file = fields.next()?.split(':');
        let line_major = u64::from_str_radix(file.next()?, 16).ok()?;
        let line_minor = u64::from_str_radix(file.next()?, 16).ok()?;
        let line_inode: u64 = file.next()?.parse().ok()?;
        (line_major == major && line_minor == minor && line_inode == inode).then_some(pid)
    })
}

/// Reads the parent PID from the text of `/proc/<pid>/stat`.
///
/// The command name in parentheses may itself contain spaces and parentheses, so the fields are
/// read after the last `)`: state, then the parent PID.
pub fn stat_parent_pid(stat: &str) -> Option<u32> {
    let (_, after_name) = stat.rsplit_once(')')?;
    let mut fields = after_name.split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse().ok()
}

/// The PIDs from `start` up to the root, following `parent_of` until it fails or reaches 0.
///
/// The walk stops after 4096 steps, more than any real process tree, so a cycle in broken input
/// cannot loop forever.
pub fn ancestor_chain(start: u32, parent_of: impl Fn(u32) -> Option<u32>) -> Vec<u32> {
    let mut chain = Vec::new();
    let mut pid = start;
    while pid != 0 && chain.len() < 4_096 {
        chain.push(pid);
        match parent_of(pid) {
            Some(parent) => pid = parent,
            None => break,
        }
    }
    chain
}

/// The strict ancestors of this process, from its parent up. This process itself is not one:
/// another test thread of the same process may hold the lock, and that one is worth waiting for.
fn strict_ancestors() -> Vec<u32> {
    #[cfg(unix)]
    {
        let parent = std::os::unix::process::parent_id();
        ancestor_chain(parent, |child| {
            std::fs::read_to_string(format!("/proc/{child}/stat"))
                .ok()
                .as_deref()
                .and_then(stat_parent_pid)
        })
    }
    #[cfg(not(unix))]
    {
        Vec::new()
    }
}

/// Turns the NUL-separated bytes of `/proc/<pid>/cmdline` into one line.
fn command_line_text(raw: &[u8]) -> String {
    raw.split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(String::from_utf8_lossy)
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------------------------
// Adapter evidence (plan decision D6)
// ---------------------------------------------------------------------------------------------

/// The adapter a child rendered on, from Bevy's `AdapterInfo` log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterEvidence {
    /// Adapter name, such as `AMD Radeon RX 580 Series (RADV POLARIS10)`.
    pub name: String,
    /// Driver name, such as `radv` or `llvmpipe`.
    pub driver: String,
    /// Driver version text, such as `Mesa 25.0.7-2+deb13u1`.
    pub driver_info: String,
}

impl AdapterEvidence {
    /// Reads the first `AdapterInfo { .. }` record in `log`, ANSI colours included.
    pub fn parse(log: &str) -> Option<Self> {
        log.lines().map(strip_ansi).find_map(|line| {
            let record = &line[line.find(ADAPTER_MARKER)?..];
            Some(Self {
                name: quoted_field(record, "AdapterInfo { name")?,
                driver: quoted_field(record, ", driver")?,
                driver_info: quoted_field(record, ", driver_info")?,
            })
        })
    }

    /// The violation, if any, of a run that should have rendered on `driver`.
    pub fn check(evidence: Option<&Self>, driver: GpuDriver) -> Option<String> {
        match evidence {
            None => Some(format!(
                "no AdapterInfo line: the run cannot show that it rendered on {driver}"
            )),
            Some(adapter) if adapter.driver != driver.adapter_driver() => Some(format!(
                "rendered on {} (driver {:?}), not on {driver} (driver {:?})",
                adapter.name,
                adapter.driver,
                driver.adapter_driver()
            )),
            Some(_) => None,
        }
    }
}

impl fmt::Display for AdapterEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}, {}", self.name, self.driver_info)
    }
}

/// Extracts the Debug-quoted string after `<key>: "` in `record`, honouring `\"` escapes.
fn quoted_field(record: &str, key: &str) -> Option<String> {
    let start = record.find(&format!("{key}: \""))? + key.len() + 3;
    let mut value = String::new();
    let mut chars = record[start..].chars();
    while let Some(character) = chars.next() {
        match character {
            '"' => return Some(value),
            '\\' => value.push(chars.next()?),
            other => value.push(other),
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Q0009: the terrain anisotropy override (plan decision D7)
// ---------------------------------------------------------------------------------------------

/// The terrain anisotropy override a child on `driver` needs, given the installed `lavapipe`
/// release `(major, minor)`. A pure function.
///
/// `radv` never needs it. `lavapipe` needs `"1"` before Mesa 25.1
/// ([`client::render_compatibility::FIRST_BOUNDED_ANISOTROPY_MESA`]); an
/// unknown release keeps the production sampler, so an affected driver fails by timeout rather
/// than quietly losing coverage.
pub fn anisotropy_override(
    driver: GpuDriver,
    lavapipe_release: Option<(u32, u32)>,
) -> Option<&'static str> {
    match driver {
        GpuDriver::Radv => None,
        GpuDriver::Lavapipe => {
            client::render_compatibility::lavapipe_anisotropy_override(lavapipe_release)
        }
    }
}

/// Extracts `(major, minor)` from `vulkaninfo --summary` text; the rule lives in
/// [`client::render_compatibility::mesa_release`].
pub fn mesa_release(summary: &str) -> Option<(u32, u32)> {
    client::render_compatibility::mesa_release(summary)
}

/// Returns `true` when this Mesa release cannot finish a frame of hex-tiled terrain; the rule
/// lives in [`client::render_compatibility::driver_stalls_on_anisotropic_hex_tiling`].
pub fn driver_stalls_on_anisotropic_hex_tiling(summary: &str) -> bool {
    client::render_compatibility::driver_stalls_on_anisotropic_hex_tiling(summary)
}

/// Asks the `lavapipe` driver in `manifest` for its Mesa release, once per test process.
///
/// Only called when the selected driver is `lavapipe`, so `vulkaninfo` never loads `radv`.
/// Returns `None` if `vulkaninfo` cannot answer.
fn lavapipe_release(manifest: &Path) -> Option<(u32, u32)> {
    static RELEASE: OnceLock<Option<(u32, u32)>> = OnceLock::new();
    *RELEASE.get_or_init(|| {
        let summary = Command::new("vulkaninfo")
            .arg("--summary")
            .env("VK_ICD_FILENAMES", manifest)
            .output();
        match summary {
            Ok(output) if output.status.success() => {
                let release = mesa_release(&String::from_utf8_lossy(&output.stdout));
                if anisotropy_override(GpuDriver::Lavapipe, release).is_some() {
                    eprintln!(
                        "WARNING: this Mesa cannot finish hex-tiled terrain with anisotropic \
                         filtering; terrain renders on lavapipe run at anisotropy 1 (Q0009)"
                    );
                }
                release
            }
            other => {
                eprintln!("WARNING: vulkaninfo gave no driver version: {other:?}");
                None
            }
        }
    })
}

// ---------------------------------------------------------------------------------------------
// Opt-out and displays
// ---------------------------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------------------------
// Display selection (plan `2026-11-17-gpu-test-readback`, decision D-4)
// ---------------------------------------------------------------------------------------------

/// Variable that chooses the display of windowed children: `kwin`, `xvfb` or `auto`. Unset
/// means `auto`.
pub const DISPLAY_VAR: &str = "SPACE_SCUFFLE_GPU_TEST_DISPLAY";

/// The `xvfb-run` wrapper, which starts a virtual X server for one command.
const XVFB_RUN: &str = "/usr/bin/xvfb-run";

/// The compositor of the headless display: `kwin_wayland --virtual --xwayland`.
const KWIN: &str = "kwin_wayland";

/// The X server `KWin` starts for X11 clients; the children's windows use it.
const XWAYLAND: &str = "Xwayland";

/// The per-user runtime directory, which `KWin` needs for its Wayland socket. The session files of
/// a `KWin` child are written there too.
const RUNTIME_DIR_VAR: &str = "XDG_RUNTIME_DIR";

/// Width and height of `KWin`'s virtual output when the caller names no screen: the 1280×720 of
/// the children's windows.
const DEFAULT_KWIN_SCREEN: (u32, u32) = (1280, 720);

/// What `SPACE_SCUFFLE_GPU_TEST_DISPLAY` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayRequest {
    /// `KWin` on `radv` where the host has it, otherwise Xvfb; the choice and its reason are
    /// printed and recorded in the ledger.
    Auto,
    /// The headless `KWin`, or an error; never a fallback.
    Kwin,
    /// `xvfb-run` (on `radv` with `MESA_VK_WSI_DEBUG=sw`), or an error.
    Xvfb,
}

impl DisplayRequest {
    /// Parses the variable's value; `None` (unset) means [`Self::Auto`].
    ///
    /// # Errors
    /// [`DisplaySelectionError::InvalidRequest`] for any value but `auto`, `kwin` and `xvfb`.
    pub fn parse(value: Option<&str>) -> Result<Self, DisplaySelectionError> {
        match value {
            None | Some("auto") => Ok(Self::Auto),
            Some("kwin") => Ok(Self::Kwin),
            Some("xvfb") => Ok(Self::Xvfb),
            Some(other) => Err(DisplaySelectionError::InvalidRequest(other.to_owned())),
        }
    }
}

/// The display server windowed children of a test process use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    /// A headless `KWin` per child (`kwin_wayland --virtual --xwayland`): `radv` presents through
    /// DRI3, and `KWin` composites on the same GPU inside the child's lock.
    Kwin,
    /// An Xvfb server per child from `xvfb-run`; `radv` needs `MESA_VK_WSI_DEBUG=sw` there.
    Xvfb,
    /// The display this test process has, where `xvfb-run` is missing.
    Host,
}

/// What the host offers a windowed child, as found by [`HostDisplay::probe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDisplay {
    /// `kwin_wayland` on `PATH`, if found.
    pub kwin: Option<PathBuf>,
    /// `Xwayland` on `PATH`, if found.
    pub xwayland: Option<PathBuf>,
    /// `XDG_RUNTIME_DIR`, if set to an existing directory.
    pub runtime_dir: Option<PathBuf>,
    /// `/usr/bin/xvfb-run`, if installed.
    pub xvfb_run: Option<PathBuf>,
    /// Whether this process has a display of its own (`DISPLAY` or `WAYLAND_DISPLAY`).
    pub desktop: bool,
}

/// A prerequisite of the `KWin` display that the host lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingDisplayTool {
    /// `kwin_wayland` is not on `PATH`.
    Kwin,
    /// `Xwayland` is not on `PATH`, so `KWin` cannot serve the children's X11 windows.
    Xwayland,
    /// `XDG_RUNTIME_DIR` is unset or not a directory, so `KWin` has nowhere for its socket.
    RuntimeDir,
}

impl fmt::Display for MissingDisplayTool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kwin => write!(formatter, "{KWIN} is not on PATH"),
            Self::Xwayland => write!(formatter, "{XWAYLAND} is not on PATH"),
            Self::RuntimeDir => write!(formatter, "{RUNTIME_DIR_VAR} is not an existing directory"),
        }
    }
}

impl HostDisplay {
    /// Inspects the real host: `PATH`, `XDG_RUNTIME_DIR`, `xvfb-run` and this process's display.
    pub fn probe() -> Self {
        let set = |key: &str| std::env::var_os(key).is_some_and(|value| !value.is_empty());
        Self {
            kwin: find_on_path(KWIN),
            xwayland: find_on_path(XWAYLAND),
            runtime_dir: std::env::var_os(RUNTIME_DIR_VAR)
                .map(PathBuf::from)
                .filter(|directory| directory.is_dir()),
            xvfb_run: Some(PathBuf::from(XVFB_RUN)).filter(|path| path.exists()),
            desktop: set("DISPLAY") || set("WAYLAND_DISPLAY"),
        }
    }

    /// Lists every `KWin` prerequisite this host lacks. Empty means `KWin` can run.
    pub fn missing_kwin_prerequisites(&self) -> Vec<MissingDisplayTool> {
        let mut missing = Vec::new();
        if self.kwin.is_none() {
            missing.push(MissingDisplayTool::Kwin);
        }
        if self.xwayland.is_none() {
            missing.push(MissingDisplayTool::Xwayland);
        }
        if self.runtime_dir.is_none() {
            missing.push(MissingDisplayTool::RuntimeDir);
        }
        missing
    }
}

/// The display windowed children use, with why it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplaySelection {
    /// The chosen server.
    pub server: DisplayServer,
    /// The runtime directory `KWin`'s socket and session files go in; `Some` exactly for
    /// [`DisplayServer::Kwin`].
    pub runtime_dir: Option<PathBuf>,
    /// How the choice was made, such as `forced` or the `auto` evidence.
    pub reason: String,
    /// Whether `auto` fell back from `KWin` on `radv`; the selection line then starts with a
    /// warning.
    pub fell_back: bool,
}

impl fmt::Display for DisplaySelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let server = match self.server {
            DisplayServer::Kwin => "kwin",
            DisplayServer::Xvfb => "xvfb",
            DisplayServer::Host => "host",
        };
        write!(formatter, "display {server} ({})", self.reason)
    }
}

/// Why no display could be selected. Each variant fails the test; a forced choice never falls
/// back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplaySelectionError {
    /// The variable holds a value other than `auto`, `kwin` and `xvfb`.
    InvalidRequest(String),
    /// `kwin` was forced with `lavapipe`: `KWin` would composite on the physical GPU, and a
    /// `lavapipe` child takes no lock.
    KwinOnLavapipe,
    /// `kwin` was forced, and these prerequisites are missing.
    KwinUnavailable(Vec<MissingDisplayTool>),
    /// `xvfb` was forced, and `xvfb-run` is not installed.
    XvfbUnavailable,
    /// Neither `KWin`, nor `xvfb-run`, nor a display of this process exists.
    NoDisplay,
}

impl fmt::Display for DisplaySelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(value) => write!(
                formatter,
                "{DISPLAY_VAR}={value:?} is not a display; use kwin, xvfb or auto"
            ),
            Self::KwinOnLavapipe => write!(
                formatter,
                "{DISPLAY_VAR}=kwin with lavapipe: KWin would composite on the physical GPU, and \
                 a lavapipe child takes no lock. Use xvfb or auto with lavapipe."
            ),
            Self::KwinUnavailable(missing) => write!(
                formatter,
                "{DISPLAY_VAR}=kwin, but this host cannot start a headless KWin: {}. The \
                 harness never falls back to another display.",
                missing
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Self::XvfbUnavailable => {
                write!(formatter, "{DISPLAY_VAR}=xvfb, but {XVFB_RUN} is missing")
            }
            Self::NoDisplay => write!(
                formatter,
                "CRITICAL: no display for a windowed GPU child: no headless KWin, {XVFB_RUN} is \
                 missing, and this process has no DISPLAY or WAYLAND_DISPLAY. Install xvfb, or set \
                 SPACE_SCUFFLE_SKIP_GPU_TESTS=1 locally."
            ),
        }
    }
}

/// Chooses the display of windowed children for `request`, `driver` and `host`. A pure function.
///
/// - `kwin`: `KWin`, refused on `lavapipe` and where a prerequisite is missing.
/// - `xvfb`: `xvfb-run`, refused where it is missing.
/// - `auto`: `KWin` on `radv` with every prerequisite; otherwise `xvfb-run` (on `lavapipe` always,
///   because `KWin` would composite on the physical GPU without the lock); otherwise this process's
///   own display.
///
/// # Errors
/// [`DisplaySelectionError`] when the requested display is not available.
pub fn select_display(
    request: DisplayRequest,
    driver: GpuDriver,
    host: &HostDisplay,
) -> Result<DisplaySelection, DisplaySelectionError> {
    let missing = host.missing_kwin_prerequisites();
    let selection = |server, reason: String, fell_back| DisplaySelection {
        server,
        runtime_dir: (server == DisplayServer::Kwin)
            .then(|| host.runtime_dir.clone())
            .flatten(),
        reason,
        fell_back,
    };
    match request {
        DisplayRequest::Kwin if driver == GpuDriver::Lavapipe => {
            Err(DisplaySelectionError::KwinOnLavapipe)
        }
        DisplayRequest::Kwin if missing.is_empty() => {
            Ok(selection(DisplayServer::Kwin, "forced".to_owned(), false))
        }
        DisplayRequest::Kwin => Err(DisplaySelectionError::KwinUnavailable(missing)),
        DisplayRequest::Xvfb if host.xvfb_run.is_some() => {
            Ok(selection(DisplayServer::Xvfb, "forced".to_owned(), false))
        }
        DisplayRequest::Xvfb => Err(DisplaySelectionError::XvfbUnavailable),
        DisplayRequest::Auto if driver == GpuDriver::Radv && missing.is_empty() => Ok(selection(
            DisplayServer::Kwin,
            format!("auto: {KWIN}, {XWAYLAND} and {RUNTIME_DIR_VAR} present"),
            false,
        )),
        DisplayRequest::Auto => {
            let why = if driver == GpuDriver::Lavapipe {
                "lavapipe stays off KWin, which would composite on the physical GPU without the \
                 lock"
                    .to_owned()
            } else {
                format!(
                    "no headless KWin ({})",
                    missing
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            };
            let fell_back = driver == GpuDriver::Radv;
            if host.xvfb_run.is_some() {
                let present = if fell_back {
                    ", so radv presents with MESA_VK_WSI_DEBUG=sw"
                } else {
                    ""
                };
                Ok(selection(
                    DisplayServer::Xvfb,
                    format!("auto: {why}{present}"),
                    fell_back,
                ))
            } else if host.desktop {
                Ok(selection(
                    DisplayServer::Host,
                    format!("auto: {why}, and {XVFB_RUN} is missing; this process's display"),
                    fell_back,
                ))
            } else {
                Err(DisplaySelectionError::NoDisplay)
            }
        }
    }
}

/// The display of this test process's windowed children, chosen once from [`DISPLAY_VAR`], the
/// selected driver and [`HostDisplay::probe`].
///
/// Prints one line naming the choice to stderr the first time, starting with `WARNING:` when
/// `auto` fell back from `KWin` on `radv`.
///
/// # Panics
/// Panics with the [`DisplaySelectionError`] when no display can be selected, or when no driver
/// can.
pub fn selected_display() -> &'static DisplaySelection {
    static SELECTION: OnceLock<Result<DisplaySelection, DisplaySelectionError>> = OnceLock::new();
    let driver = selected_driver().driver;
    let selection = SELECTION.get_or_init(|| {
        let value = std::env::var(DISPLAY_VAR).ok();
        let selection = DisplayRequest::parse(value.as_deref())
            .and_then(|request| select_display(request, driver, &HostDisplay::probe()));
        match &selection {
            Ok(selection) if selection.fell_back => eprintln!("WARNING: gpu_run: {selection}"),
            Ok(selection) => eprintln!("gpu_run: {selection}"),
            Err(_) => {}
        }
        selection
    });
    match selection {
        Ok(selection) => selection,
        Err(error) => panic!("gpu_run: {error}"),
    }
}

// ---------------------------------------------------------------------------------------------
// The child command (plan decisions D3 to D6)
// ---------------------------------------------------------------------------------------------

/// Which lock a child takes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LockPolicy {
    /// No lock: `lavapipe` uses no shared hardware.
    None,
    /// [`GPU_LOCK_PATH`], waiting up to [`shared_lock_wait`]: `radv`.
    Shared,
    /// A private file, waiting up to the given time: self-tests of the lock only.
    Private(PathBuf, Duration),
}

impl LockPolicy {
    /// The ledger's name for this policy; `queued` says the shared lock is taken in turn
    /// ([`shared_queue`]).
    fn name(&self, queued: bool) -> &'static str {
        match self {
            Self::None => "none",
            Self::Shared if queued => "shared-queued",
            Self::Shared => "shared",
            Self::Private(..) => "private lock",
        }
    }
}

/// What a child is, which decides the checks its run gets.
#[derive(Debug, Clone, Copy)]
enum ChildKind {
    /// A renderer on the selected driver; its adapter line is checked.
    Renderer(GpuDriver),
    /// A self-test of the lock that renders nothing.
    LockProbe,
}

/// Where a renderer child shows its frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildDisplay {
    /// No display: the child renders into an image (`client::offscreen`) and presents nothing.
    /// `DISPLAY` and `WAYLAND_DISPLAY` are removed, so a window opened by mistake fails loudly
    /// instead of reaching a desktop.
    None,
    /// An Xvfb server from `xvfb-run`, of `screen` (such as `1600x900x24`) when given.
    Xvfb {
        /// The screen `xvfb-run` starts, or `None` for its default.
        screen: Option<String>,
    },
    /// A headless `KWin` of `screen`'s width and height (1280×720 when `None`), whose session runs
    /// the child; its socket and session files go in `runtime_dir`.
    Kwin {
        /// The screen, as for Xvfb; only its width and height are used.
        screen: Option<String>,
        /// `XDG_RUNTIME_DIR`, passed to `KWin`.
        runtime_dir: PathBuf,
    },
    /// The display this test process has, where `xvfb-run` is missing.
    Host,
}

impl ChildDisplay {
    /// The display of a windowed child on `selection`, of `screen` when given.
    ///
    /// # Panics
    /// Panics when a `KWin` selection carries no runtime directory, which [`select_display`]
    /// never produces.
    pub fn windowed(selection: &DisplaySelection, screen: Option<&str>) -> Self {
        let screen = screen.map(str::to_owned);
        match selection.server {
            DisplayServer::Kwin => Self::Kwin {
                screen,
                runtime_dir: selection
                    .runtime_dir
                    .clone()
                    .expect("invariant: select_display gives every KWin selection its directory"),
            },
            DisplayServer::Xvfb => Self::Xvfb { screen },
            DisplayServer::Host => Self::Host,
        }
    }

    /// The ledger's `display=` value for a child on `driver`.
    fn ledger_name(&self, driver: GpuDriver) -> &'static str {
        match (self, driver) {
            (Self::None, _) => "offscreen",
            (Self::Xvfb { .. }, GpuDriver::Radv) => "xvfb-sw",
            (Self::Xvfb { .. }, GpuDriver::Lavapipe) => "xvfb",
            (Self::Kwin { .. }, _) => "kwin",
            (Self::Host, _) => "host",
        }
    }
}

/// The width and height of a screen such as `1600x900x24` or `1600x900`; `None` gives
/// `KWin`'s default of 1280×720.
///
/// # Panics
/// Panics when `screen` is not `<width>x<height>[x<depth>]` with positive whole numbers.
pub fn kwin_screen(screen: Option<&str>) -> (u32, u32) {
    let Some(screen) = screen else {
        return DEFAULT_KWIN_SCREEN;
    };
    let mut sides = screen.split('x').map(str::parse::<u32>);
    match (sides.next(), sides.next()) {
        (Some(Ok(width)), Some(Ok(height))) if width > 0 && height > 0 => (width, height),
        _ => panic!("the screen {screen:?} is not <width>x<height>[x<depth>]"),
    }
}

/// A window a GPU child asks for that is larger than the output its command states
/// ([`window_fits_screen`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowTooLarge {
    /// The window's width and height in physical pixels, as the test declared it.
    pub window: (u32, u32),
    /// The output's width and height ([`kwin_screen`] of the stated screen).
    pub output: (u32, u32),
    /// The screen the command states, or `None` when it states none.
    pub screen: Option<String>,
}

impl fmt::Display for WindowTooLarge {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (width, height) = self.window;
        let (output_width, output_height) = self.output;
        let stated = self.screen.as_deref().map_or_else(
            || "states no screen".to_owned(),
            |screen| format!("states the screen {screen:?}"),
        );
        write!(
            formatter,
            "the child asks for a {width}×{height} window, but its command {stated}, so KWin's \
             output is {output_width}×{output_height} and KWin would shrink the window to fit. \
             State the screen size, such as \
             `virtual_gpu_command_on_screen(binary, Some(\"{width}x{height}x24\"))`"
        )
    }
}

/// Checks that a `window` of physical pixels fits the output of `screen` as `KWin` makes it
/// ([`kwin_screen`]: the stated size, or 1280×720 when `None`).
///
/// `KWin` shrinks a window larger than its output to fit it, silently. Xvfb never resizes an X
/// window, so the same missing screen passes there and only fails later under `KWin`. The
/// harness therefore applies this check on every windowed display, Xvfb included, so the
/// lavapipe runs of CI catch it too.
///
/// # Errors
/// [`WindowTooLarge`] when the window is wider or taller than the output.
///
/// # Panics
/// As [`kwin_screen`], when `screen` is malformed.
pub fn window_fits_screen(window: (u32, u32), screen: Option<&str>) -> Result<(), WindowTooLarge> {
    let output = kwin_screen(screen);
    if window.0 > output.0 || window.1 > output.1 {
        return Err(WindowTooLarge {
            window,
            output,
            screen: screen.map(str::to_owned),
        });
    }
    Ok(())
}

/// The per-run files and names of one `KWin` child (plan `2026-11-17-gpu-test-readback`, D-4).
///
/// `KWin` exits with a status of its own, so the session runs the child through a shim that
/// writes the child's exit status to a file; that file is the verdict, and a missing file is a
/// failure ([`kwin_session_verdict`]).
#[derive(Debug)]
struct KwinSession {
    /// `KWin`'s Wayland socket name, `ss-gpu-<pid>-<nanoseconds>-<n>`, unique per run on the host.
    socket: String,
    /// The directory of the shim and the status file, `<runtime dir>/<socket>.session`.
    directory: PathBuf,
    /// Width of the virtual output, in pixels.
    width: u32,
    /// Height of the virtual output, in pixels.
    height: u32,
}

/// The status shim a `KWin` session runs: `session.sh <status file> <program> [args...]`. It runs
/// the program with `KWin`'s environment and output, then writes its exit status to the file
/// (through a temporary name, so a half-written file is never read) and exits with it.
const SESSION_SHIM: &str = "#!/bin/sh\n\
# Written by crates/client/tests/support/gpu_run.rs for one GPU test child under KWin.\n\
status_file=$1\n\
shift\n\
\"$@\"\n\
code=$?\n\
printf '%s\\n' \"$code\" > \"$status_file.part\" && mv \"$status_file.part\" \"$status_file\"\n\
exit \"$code\"\n";

impl KwinSession {
    /// A session of `screen`'s size whose files go under `runtime_dir`, named uniquely for this
    /// process and run.
    fn new(runtime_dir: &Path, screen: Option<&str>) -> Self {
        static NEXT_SESSION: AtomicU32 = AtomicU32::new(0);
        // The nonce keeps a stale directory of a long-dead process with a reused PID from
        // colliding with this run's; an existing directory is still refused.
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let socket = format!(
            "ss-gpu-{}-{nonce}-{}",
            std::process::id(),
            NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
        );
        let (width, height) = kwin_screen(screen);
        Self {
            // Not the socket's own name: KWin creates its socket there.
            directory: runtime_dir.join(format!("{socket}.session")),
            socket,
            width,
            height,
        }
    }

    /// The shim's path.
    fn shim(&self) -> PathBuf {
        self.directory.join("session.sh")
    }

    /// The status file's path.
    fn status_file(&self) -> PathBuf {
        self.directory.join("status")
    }

    /// `KWin`'s Wayland socket, next to the session directory in the runtime directory. Its lock
    /// file is the same path with `.lock` appended.
    fn socket_path(&self) -> PathBuf {
        self.directory.with_file_name(&self.socket)
    }

    /// `KWin`'s arguments for running `child`: the headless flags, the output size, the socket
    /// and the session string.
    ///
    /// # Panics
    /// As [`session_word`], for any path or argument of the session string.
    fn arguments(&self, child: &Command) -> Vec<String> {
        let mut words = vec![
            session_word(&self.shim().to_string_lossy()),
            session_word(&self.status_file().to_string_lossy()),
            session_word(&child.get_program().to_string_lossy()),
        ];
        words.extend(
            child
                .get_args()
                .map(|arg| session_word(&arg.to_string_lossy())),
        );
        [
            "--virtual",
            "--xwayland",
            "--no-lockscreen",
            "--no-global-shortcuts",
        ]
        .into_iter()
        .map(str::to_owned)
        .chain([
            "--width".to_owned(),
            self.width.to_string(),
            "--height".to_owned(),
            self.height.to_string(),
            "--socket".to_owned(),
            self.socket.clone(),
            "--exit-with-session".to_owned(),
            words.join(" "),
        ])
        .collect()
    }

    /// Creates the session directory, which must not exist yet, and writes the shim into it,
    /// executable. A directory left by an earlier run could hold a stale status file that would
    /// pass a failed run, so an existing one is an error. The returned guard removes the
    /// directory, and `KWin`'s socket and its lock file, when dropped.
    fn prepare(&self) -> std::io::Result<SessionFiles> {
        std::fs::create_dir(&self.directory)?;
        let guard = SessionFiles {
            directory: self.directory.clone(),
            socket: self.socket_path(),
        };
        std::fs::write(self.shim(), SESSION_SHIM)?;
        make_executable(&self.shim())?;
        Ok(guard)
    }

    /// Replaces `KWin`'s exit status in `output` with the child's, from the status file.
    ///
    /// # Panics
    /// Panics when the session left no status file and the run did not time out: the child's
    /// verdict would be unknown.
    fn settle(&self, output: Output, label: &str) -> Output {
        let status = std::fs::read_to_string(self.status_file()).ok();
        match kwin_session_verdict(output.status.code(), status.as_deref()) {
            SessionVerdict::TimedOut => output,
            SessionVerdict::Exited(code) => Output {
                status: exit_status(code),
                ..output
            },
            SessionVerdict::NoStatus => panic!(
                "{label}: the KWin session left no status file, so the child's exit status is \
                 unknown (KWin exited with {:?}). Last output:\n{}",
                output.status.code(),
                tail(&captured_log(&output))
            ),
        }
    }
}

/// Removes a `KWin` session's directory, and the socket and socket lock a killed `KWin` leaves,
/// when dropped, whatever the run's outcome.
#[derive(Debug)]
struct SessionFiles {
    /// The directory to remove.
    directory: PathBuf,
    /// `KWin`'s socket; the run's process group is gone, so no `KWin` uses it any more.
    socket: PathBuf,
}

impl Drop for SessionFiles {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.directory) {
            eprintln!(
                "WARNING: gpu_run could not remove the KWin session directory {}: {error}",
                self.directory.display()
            );
        }
        let mut lock = self.socket.clone().into_os_string();
        lock.push(".lock");
        for file in [self.socket.clone(), PathBuf::from(lock)] {
            match std::fs::remove_file(&file) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => eprintln!(
                    "WARNING: gpu_run could not remove KWin's {}: {error}",
                    file.display()
                ),
            }
        }
    }
}

/// Sets the execute bits of the file at `path`.
#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

/// Without Unix permissions every file is executable.
#[cfg(not(unix))]
fn make_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// The exit status of a process that exited with `code`.
#[cfg(unix)]
fn exit_status(code: i32) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt as _;
    ExitStatus::from_raw((code & 0xff) << 8)
}

/// The exit status of a process that exited with `code`.
#[cfg(windows)]
fn exit_status(code: i32) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt as _;
    ExitStatus::from_raw(code.unsigned_abs())
}

/// Returns `word` if `KWin`'s session string can carry it as one word.
///
/// `KWin` splits `--exit-with-session` like a shell command line, so a word with whitespace, a
/// quote, a backslash or another shell metacharacter would be split or changed. Such a word is
/// refused rather than escaped: the children's paths and arguments never need one.
///
/// # Panics
/// Panics naming `word` when it holds any character other than ASCII letters, digits and
/// `-_./=,:+@%`.
pub fn session_word(word: &str) -> String {
    let plain =
        |character: char| character.is_ascii_alphanumeric() || "-_./=,:+@%".contains(character);
    if let Some(bad) = word.chars().find(|character| !plain(*character)) {
        panic!(
            "the KWin session string cannot carry {word:?}: it holds {bad:?}, and KWin splits \
             the session string like a shell command line"
        );
    }
    assert!(
        !word.is_empty(),
        "the KWin session string cannot carry an empty word"
    );
    word.to_owned()
}

/// What a `KWin` child's run comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionVerdict {
    /// GNU `timeout` ended the run; the harness reports a timeout.
    TimedOut,
    /// The child exited with this status, as its status file says.
    Exited(i32),
    /// The session left no readable status: a failure, whatever `KWin`'s own status.
    NoStatus,
}

/// The verdict of a `KWin` child from `KWin`'s exit code (`None` when killed by a signal) and the
/// text of its status file (`None` when the file is missing). A pure function.
///
/// The status file is the source of truth; `KWin`'s own code counts only as `timeout`'s
/// [`TIMEOUT_EXIT_CODE`]. `KWin` 6.3.6 happens to pass the session's code on, but earlier releases
/// exited 0 after a failed session (readback evidence, P-K).
pub fn kwin_session_verdict(kwin_exit: Option<i32>, status: Option<&str>) -> SessionVerdict {
    if kwin_exit == Some(TIMEOUT_EXIT_CODE) {
        return SessionVerdict::TimedOut;
    }
    status
        .and_then(|text| text.trim().parse().ok())
        .map_or(SessionVerdict::NoStatus, SessionVerdict::Exited)
}

/// A GPU child under construction: binary, display, driver environment, run limit and lock.
///
/// It runs as `timeout`, then the display wrapper (`xvfb-run`, or `kwin_wayland` whose session
/// runs the child through a status shim), then the binary and its arguments.
///
/// It deliberately offers no way to start the child except through the lock: [`Self::run`],
/// [`Self::try_run`], [`Self::run_validated`] and [`Self::assert_passes_validation`].
#[derive(Debug)]
pub struct GpuCommand {
    /// The program, its arguments and the variables it gets. The command that runs wraps it in
    /// `timeout` and the display ([`Self::launch_command`]).
    command: Command,
    /// The display the child shows its frames on.
    shown_on: ChildDisplay,
    /// The display number `xvfb-run` searches from ([`display_search_start`]); unused elsewhere.
    xvfb_start: u32,
    /// The `KWin` session's names, for a child on [`ChildDisplay::Kwin`].
    session: Option<KwinSession>,
    /// Renderer on a driver, or lock self-test.
    kind: ChildKind,
    /// The lock the run takes around the spawn.
    lock: LockPolicy,
    /// The run limit given to `timeout`, in seconds.
    run_limit_secs: u32,
    /// The executable the child runs, which [`preload_executable`] reads before the lock.
    program: PathBuf,
    /// The ledger's name of the child's display ([`GpuCommand::display_name`]).
    display: &'static str,
    /// `false` for a command built by [`GpuCommand::inspect`], which never runs.
    runnable: bool,
    /// The window the child asks for, in physical pixels, when the test declares it
    /// ([`GpuCommand::window`]).
    window: Option<(u32, u32)>,
}

/// The manifest an inspected command names ([`GpuCommand::inspect`]); no driver is installed
/// there, so even a command that escaped the check in [`GpuCommand::try_run`] loads no driver.
pub const INSPECTION_MANIFEST: &str = "/nonexistent/space-scuffle-inspection/icd.json";

/// Builds a [`GpuCommand`] that runs `binary` on a virtual display with the selected driver: the
/// headless `KWin` or Xvfb, as [`selected_display`] chose.
///
/// # Panics
/// Panics when no driver or no display can be selected. A GPU test must not pass without
/// running.
pub fn virtual_gpu_command(binary: &Path) -> GpuCommand {
    virtual_gpu_command_on_screen(binary, None)
}

/// Runs the ignored libtest entry `test_name` of this test binary as a GPU child, and fails unless
/// it passes on the selected driver.
///
/// A test that builds a real render device in its own process would pick an adapter with no
/// driver selection and no lock (Vulkan or, without `WGPU_BACKEND`, even OpenGL on the physical
/// GPU). Such a test is split instead: its body becomes an `#[ignore]`d child, and its `#[test]`
/// calls this. `test_name` is the full libtest name, with its module path inside the crate, such
/// as `shell_order_audit::gpu_tests::actual_renderer_child`. Check C21 in
/// `crates/workspace_checks` fails the gate on an in-process render device that is not split
/// this way.
///
/// # Panics
/// As [`GpuCommand::run`], and when this test binary's path is unknown.
pub fn run_test_child(test_name: &str, label: &str) -> GpuRun {
    let binary = std::env::current_exe()
        .unwrap_or_else(|error| panic!("{label}: this test binary's path is unknown: {error}"));
    let mut command = virtual_gpu_command(&binary);
    command.args(["--ignored", "--exact", "--nocapture", test_name]);
    command.run(label)
}

/// Builds a [`GpuCommand`] that runs `binary` with the selected driver and no display: GNU
/// `timeout`, then the binary. The child must render into an image and read only its own
/// pictures (`client::offscreen`); it keeps `SPACE_SCUFFLE_VIRTUAL_GPU=1`, the driver, the lock
/// and the adapter check of a windowed child.
///
/// # Panics
/// Panics when no driver can be selected.
pub fn offscreen_gpu_command(binary: &Path) -> GpuCommand {
    GpuCommand::renderer(binary, &ChildDisplay::None, selected_driver())
}

/// As [`run_test_child`], for a child that renders offscreen ([`offscreen_gpu_command`]).
///
/// # Panics
/// As [`run_test_child`].
pub fn run_offscreen_test_child(test_name: &str, label: &str) -> GpuRun {
    let binary = std::env::current_exe()
        .unwrap_or_else(|error| panic!("{label}: this test binary's path is unknown: {error}"));
    let mut command = offscreen_gpu_command(&binary);
    command.args(["--ignored", "--exact", "--nocapture", test_name]);
    command.run(label)
}

/// Builds a [`virtual_gpu_command`] whose display has the size `screen`, such as `1600x900x24`.
///
/// # Panics
/// As [`virtual_gpu_command`].
pub fn virtual_gpu_command_on_screen(binary: &Path, screen: Option<&str>) -> GpuCommand {
    let display = ChildDisplay::windowed(selected_display(), screen);
    GpuCommand::renderer(binary, &display, selected_driver())
}

/// Builds a windowed command as [`virtual_gpu_command_on_screen`] does, on the display `request`
/// selects for this one command instead of the process's [`selected_display`]. The equivalence
/// tests use it to run one child on Xvfb and on `KWin`.
///
/// # Panics
/// Panics when no driver can be selected, or when `request` cannot be met
/// ([`select_display`]): a forced display never falls back.
pub fn virtual_gpu_command_with(
    binary: &Path,
    screen: Option<&str>,
    request: DisplayRequest,
) -> GpuCommand {
    let selection = selected_driver();
    let display = select_display(request, selection.driver, &HostDisplay::probe())
        .unwrap_or_else(|error| panic!("gpu_run: {error}"));
    GpuCommand::renderer(binary, &ChildDisplay::windowed(&display, screen), selection)
}

impl GpuCommand {
    /// Builds a command of `binary` on `display` for `driver` that can be inspected
    /// ([`Self::command_line`], [`Self::environment`], [`Self::display_name`]) but never run:
    /// [`Self::try_run`] refuses it. Its manifest is [`INSPECTION_MANIFEST`].
    ///
    /// The tests of the command builder use it, so they need no host driver. A runnable renderer
    /// comes only from [`selected_driver`], which pairs the driver with its own manifest and so
    /// with its lock: a caller cannot pair `lavapipe`'s missing lock with `radv`'s manifest.
    pub fn inspect(binary: &Path, display: &ChildDisplay, driver: GpuDriver) -> Self {
        let selection = GpuSelection {
            driver,
            manifest: PathBuf::from(INSPECTION_MANIFEST),
            reason: "inspection only".to_owned(),
        };
        let mut command = Self::renderer(binary, display, &selection);
        command.runnable = false;
        command
    }

    /// Builds a renderer child of `binary` on `display` for `selection`. GNU `timeout` wraps the
    /// run, because [`Command::output`] has no deadline and a client that loses its display can
    /// spin forever. It signals the whole process group, including the client under `xvfb-run`,
    /// and exits with [`TIMEOUT_EXIT_CODE`]. The lock is taken before `timeout` starts, so
    /// waiting for it never eats the run limit.
    ///
    /// # Panics
    /// Panics when `display` is virtual and neither `xvfb-run` nor a display exists.
    fn renderer(binary: &Path, display: &ChildDisplay, selection: &GpuSelection) -> Self {
        let driver = selection.driver;
        let kwin = matches!(display, ChildDisplay::Kwin { .. });
        assert!(
            !(kwin && driver == GpuDriver::Lavapipe),
            "invariant: select_display never puts a lavapipe child on KWin, which would \
             composite on the physical GPU without the lock"
        );
        let mut command = Command::new(binary);
        // The test must stay on its own display, or on none, and never appear on the user's
        // desktop. KWin sets both display variables for its session.
        command.env_remove("WAYLAND_DISPLAY");
        match display {
            ChildDisplay::None => {
                command.env_remove("DISPLAY");
            }
            ChildDisplay::Kwin { runtime_dir, .. } => {
                command.env_remove("DISPLAY");
                // KWin would otherwise register on the developer's session bus.
                command.env_remove("DBUS_SESSION_BUS_ADDRESS");
                command.env(RUNTIME_DIR_VAR, runtime_dir);
                command.env("WINIT_UNIX_BACKEND", "x11");
            }
            ChildDisplay::Xvfb { .. } | ChildDisplay::Host => {
                command.env("WINIT_UNIX_BACKEND", "x11");
            }
        }
        command.env("WGPU_BACKEND", "vulkan");
        command.env("SPACE_SCUFFLE_VIRTUAL_GPU", "1");
        command.env("VK_ICD_FILENAMES", &selection.manifest);
        command.env("WGPU_ADAPTER_NAME", driver.adapter_driver());
        let lock = match driver {
            GpuDriver::Radv => {
                if matches!(display, ChildDisplay::Xvfb { .. } | ChildDisplay::Host) {
                    // Xvfb has no DRI3, and radv's X11 presentation needs it unless `sw` selects
                    // the software present path. Rendering stays on the GPU (plan section 3.1).
                    command.env("MESA_VK_WSI_DEBUG", "sw");
                } else {
                    // Offscreen presents nothing; KWin's Xwayland offers DRI3, so radv presents
                    // as on a desktop.
                    command.env_remove("MESA_VK_WSI_DEBUG");
                }
                command.env_remove("LIBGL_ALWAYS_SOFTWARE");
                LockPolicy::Shared
            }
            GpuDriver::Lavapipe => {
                command.env_remove("MESA_VK_WSI_DEBUG");
                command.env("LIBGL_ALWAYS_SOFTWARE", "1");
                LockPolicy::None
            }
        };
        let session = match display {
            ChildDisplay::Kwin {
                screen,
                runtime_dir,
            } => Some(KwinSession::new(runtime_dir, screen.as_deref())),
            _ => None,
        };
        let xvfb_start = match display {
            ChildDisplay::Xvfb { .. } => display_search_start(),
            _ => 0,
        };
        Self {
            command,
            shown_on: display.clone(),
            xvfb_start,
            session,
            kind: ChildKind::Renderer(driver),
            lock,
            run_limit_secs: driver.run_limit_secs(),
            program: binary.to_owned(),
            display: display.ledger_name(driver),
            runnable: true,
            window: None,
        }
    }

    /// Builds a self-test child of the lock: `program` under `timeout`, with no display and no
    /// driver, holding the private lock file `lock` for its run after waiting up to `wait`.
    ///
    /// Only the harness's own lock tests use this, so they run on every host, CI included, and
    /// never touch [`GPU_LOCK_PATH`].
    pub fn lock_probe(program: &Path, lock: &Path, wait: Duration) -> Self {
        Self {
            command: Command::new(program),
            shown_on: ChildDisplay::None,
            xvfb_start: 0,
            session: None,
            kind: ChildKind::LockProbe,
            lock: LockPolicy::Private(lock.to_owned(), wait),
            run_limit_secs: LOCK_PROBE_RUN_LIMIT_SECS,
            program: program.to_owned(),
            display: "none",
            runnable: true,
            window: None,
        }
    }

    /// Adds one argument for the binary.
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.command.arg(arg);
        self
    }

    /// Adds arguments for the binary.
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(args);
        self
    }

    /// Panics, naming `label` and the missing screen size, when the declared window is larger
    /// than the output of a windowed display's stated screen ([`window_fits_screen`]).
    fn refuse_an_oversized_window(&self, label: &str) {
        let screen = match &self.shown_on {
            ChildDisplay::Kwin { screen, .. } | ChildDisplay::Xvfb { screen } => screen.as_deref(),
            ChildDisplay::None | ChildDisplay::Host => return,
        };
        if let Some(window) = self.window {
            if let Err(error) = window_fits_screen(window, screen) {
                panic!("{label}: {error}");
            }
        }
    }

    /// The `KWin` child's Wayland socket path (its lock file is the same path with `.lock`) and
    /// its session directory, or `None` for a child not on `KWin`. The self-tests check that a
    /// run leaves neither behind.
    pub fn kwin_session_paths(&self) -> Option<(PathBuf, PathBuf)> {
        self.session
            .as_ref()
            .map(|session| (session.socket_path(), session.directory.clone()))
    }

    /// Declares the window the child asks for, `width`×`height` physical pixels. [`Self::try_run`]
    /// refuses the run, before the lock, when the window is larger than the output this
    /// command's screen gives under `KWin` ([`window_fits_screen`]); offscreen children have no
    /// window and are not checked.
    pub fn window(&mut self, width: u32, height: u32) -> &mut Self {
        self.window = Some((width, height));
        self
    }

    /// Sets an environment variable for the child.
    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.command.env(key, value);
        self
    }

    /// The ledger's name of this child's display: `offscreen` (no display), `kwin` (a headless
    /// `KWin`), `xvfb-sw` (`xvfb-run` with radv's software present), `xvfb` (`xvfb-run` on
    /// lavapipe), `host` (the display this process has, where `xvfb-run` is missing) or `none`
    /// (a lock self-test).
    pub fn display_name(&self) -> &'static str {
        self.display
    }

    /// The command line the run executes: `timeout` and its arguments, the display wrapper if
    /// any, the program and its arguments.
    ///
    /// # Panics
    /// As [`session_word`], for a `KWin` child whose program or arguments `KWin` cannot carry.
    pub fn command_line(&self) -> Vec<String> {
        let launch = self.launch_command();
        std::iter::once(launch.get_program())
            .chain(launch.get_args())
            .map(|part| part.to_string_lossy().into_owned())
            .collect()
    }

    /// Every variable the command sets (`Some`) or removes (`None`) for the child, in the order
    /// the command lists them; variables it leaves alone are inherited.
    pub fn environment(&self) -> Vec<(String, Option<String>)> {
        self.command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    /// The driver this child renders on, or `None` for a lock self-test.
    pub fn driver(&self) -> Option<GpuDriver> {
        match self.kind {
            ChildKind::Renderer(driver) => Some(driver),
            ChildKind::LockProbe => None,
        }
    }

    /// Lowers terrain anisotropy to 1 only where [`anisotropy_override`] says the driver needs it:
    /// `lavapipe` before Mesa 25.1, never `radv` (Q0009).
    ///
    /// Call this only for runs that draw hex-tiled ground. Other runs keep the production sampler,
    /// so `client --battle` still creates it under validation on an affected host.
    pub fn allow_for_hex_tiling_driver_defect(&mut self) -> &mut Self {
        if self.driver() == Some(GpuDriver::Lavapipe) {
            let release = lavapipe_release(&selected_driver().manifest);
            if let Some(value) = anisotropy_override(GpuDriver::Lavapipe, release) {
                self.command.env(TERRAIN_ANISOTROPY_OVERRIDE_ENV, value);
            }
        }
        self
    }

    /// Takes the lock, runs the child, releases the lock, and checks the run.
    ///
    /// `label` names the run in failure messages and the ledger.
    ///
    /// # Panics
    /// Panics if the lock is not taken, the child cannot start, it hits its run limit, it exits
    /// unsuccessfully, or a renderer's adapter line is missing or names another driver. Every
    /// message starts with the label, the driver and the adapter.
    pub fn run(self, label: &str) -> GpuRun {
        self.try_run(label)
            .unwrap_or_else(|error| panic!("{label}: {error}"))
    }

    /// As [`Self::run`], but returns a lock failure instead of panicking on it. The child does not
    /// start in that case.
    ///
    /// # Errors
    /// [`GpuLockError`] when the lock could not be taken.
    ///
    /// # Panics
    /// When the declared window does not fit the stated screen ([`Self::window`]), when the
    /// command was built by [`Self::inspect`], and as [`Self::run`] for everything after the lock.
    pub fn try_run(mut self, label: &str) -> Result<GpuRun, GpuLockError> {
        self.refuse_an_oversized_window(label);
        assert!(
            self.runnable,
            "{label}: a command built by GpuCommand::inspect is for inspection and never runs"
        );
        self.keep_adapter_line();
        let mut launch = self.launch_command();
        let session_files = self.session.as_ref().map(|session| {
            session.prepare().unwrap_or_else(|error| {
                panic!(
                    "{label}: cannot write the KWin session files in {}: {error}",
                    session.directory.display()
                )
            })
        });
        let preload = preload_executable(&self.program).unwrap_or_else(|error| {
            panic!("{label}: cannot read {}: {error}", self.program.display())
        });
        let queue = match &self.lock {
            LockPolicy::Shared => shared_queue(),
            LockPolicy::None | LockPolicy::Private(..) => None,
        };
        let lock_name = self.lock.name(queue.is_some());
        let acquired = match &self.lock {
            LockPolicy::None => Ok(None),
            LockPolicy::Shared => {
                GpuLock::acquire_queued(Path::new(GPU_LOCK_PATH), queue, shared_lock_wait())
                    .map(Some)
            }
            LockPolicy::Private(path, wait) => GpuLock::acquire(path, *wait).map(Some),
        };
        let (guard, lock_wait) = match acquired {
            Ok(Some((guard, waited))) => (Some(guard), Some(waited)),
            Ok(None) => (None, None),
            Err(error) => {
                if let GpuLockError::TimedOut { waited, .. } = &error {
                    append_to_ledger(&lock_timeout_ledger_line(
                        label,
                        self.driver(),
                        self.display,
                        lock_name,
                        preload,
                        *waited,
                    ));
                }
                return Err(error);
            }
        };
        let spawned = SystemTime::now();
        let started = Instant::now();
        let output = run_to_end(&mut launch);
        let run_time = started.elapsed();
        drop(guard);
        let output =
            output.unwrap_or_else(|error| panic!("{label}: running {launch:?} failed: {error}"));
        let output = match &self.session {
            Some(session) => session.settle(output, label),
            None => output,
        };
        drop(session_files);
        let driver = self.driver();
        let log = captured_log(&output);
        let adapter = AdapterEvidence::parse(&log);
        let timeline = ChildTimeline::read(&log, spawned, run_time);
        let mut run = GpuRun {
            output,
            driver,
            adapter,
            preload,
            lock_wait,
            run_time,
            timeline,
            ledger: String::new(),
        };
        run.ledger = run.ledger_line(label, lock_name, self.display);
        append_to_ledger(&run.ledger);
        keep_run_log(label, &log);
        run.assert_completed(label, self.run_limit_secs);
        Ok(run)
    }

    /// Runs the child under validation and returns the run with the parsed report, whose
    /// violations include the adapter check. Exit status is checked as in [`Self::run`].
    ///
    /// # Panics
    /// As [`Self::run`].
    pub fn run_validated(self, label: &str) -> (GpuRun, ValidationReport) {
        let validated = self.validated();
        let driver = validated.driver();
        let run = validated.run(label);
        let mut report = ValidationReport::parse(&run.log());
        report.expected_driver = driver;
        (run, report)
    }

    /// The command with the validation environment [`Self::run_validated`] runs it under:
    /// `WGPU_VALIDATION=1` requests the layer, `WGPU_DEBUG=1` the debug messenger through which
    /// the layer's and the loader's messages reach the log (plan `2026-11-20-gpu-lane` D-5,
    /// B-157: a release build sets neither bit by default), and `RUST_LOG` is replaced so a
    /// developer's own filter cannot hide findings.
    #[must_use]
    pub fn validated(mut self) -> Self {
        self.command.env("WGPU_VALIDATION", "1");
        self.command.env("WGPU_DEBUG", "1");
        self.command.env("RUST_LOG", VALIDATED_RUST_LOG);
        self
    }

    /// Runs the child and fails on any validation violation, the adapter check included.
    ///
    /// # Panics
    /// As [`Self::run`], and when the report has a violation of [`VALIDATION_ALLOWLIST`].
    pub fn assert_passes_validation(self, label: &str) -> GpuRun {
        let (run, report) = self.run_validated(label);
        let violations = report.violations(VALIDATION_ALLOWLIST);
        assert!(
            violations.is_empty(),
            "{} failed Vulkan validation:\n{}\nLast output:\n{}",
            run.context(label),
            violations.join("\n"),
            tail(&run.log())
        );
        run
    }

    /// Lowers the run limit to `secs` seconds, for the harness's own timeout self-tests only
    /// (`a_timed_out_kwin_session_leaves_nothing_running`). No GPU test may use it: the limits of
    /// [`GpuDriver::run_limit_secs`] are the gate's.
    pub fn set_run_limit_for_self_test(&mut self, secs: u32) -> &mut Self {
        self.run_limit_secs = secs;
        self
    }

    /// The command that runs: `timeout --kill-after=<grace> <limit>`, then `xvfb-run` and its
    /// arguments, or `kwin_wayland` with the session string, or nothing, then the program and
    /// its arguments, with every variable of [`Self::environment`].
    ///
    /// # Panics
    /// As [`session_word`], for a `KWin` child whose program or arguments `KWin` cannot carry.
    fn launch_command(&self) -> Command {
        let mut launch = timeout_command(self.run_limit_secs);
        let child = || {
            std::iter::once(self.command.get_program().to_os_string())
                .chain(self.command.get_args().map(OsStr::to_os_string))
        };
        match (&self.shown_on, &self.session) {
            (ChildDisplay::Kwin { .. }, Some(session)) => {
                launch.arg(KWIN).args(session.arguments(&self.command));
            }
            (ChildDisplay::Xvfb { screen }, _) => {
                // `-n` must precede `-a`, which reads the starting number as it is parsed.
                launch
                    .arg(XVFB_RUN)
                    .arg("-n")
                    .arg(self.xvfb_start.to_string())
                    .arg("-a");
                if let Some(screen) = screen {
                    launch
                        .arg("-s")
                        .arg(format!("-screen 0 {screen} +extension GLX"));
                }
                launch.args(child());
            }
            _ => {
                launch.args(child());
            }
        }
        for (key, value) in self.command.get_envs() {
            match value {
                Some(value) => launch.env(key, value),
                None => launch.env_remove(key),
            };
        }
        launch
    }

    /// Makes sure the child logs Bevy's adapter line under any `RUST_LOG` it will see.
    ///
    /// With `RUST_LOG` unset, Bevy's default `info` level already logs it. With a filter set on
    /// the command or inherited from this process, [`ADAPTER_LOG_DIRECTIVE`] is appended.
    fn keep_adapter_line(&mut self) {
        if !matches!(self.kind, ChildKind::Renderer(_)) {
            return;
        }
        let explicit = self
            .command
            .get_envs()
            .find(|(key, _)| *key == OsStr::new("RUST_LOG"))
            .map(|(_, value)| value.map(OsStr::to_os_string));
        let filter = match explicit {
            Some(Some(value)) => Some(value),
            Some(None) => None,
            None => std::env::var_os("RUST_LOG"),
        };
        if let Some(filter) = filter.map(|value| value.to_string_lossy().into_owned()) {
            if !filter.contains(ADAPTER_LOG_DIRECTIVE) {
                self.command
                    .env("RUST_LOG", format!("{filter},{ADAPTER_LOG_DIRECTIVE}"));
            }
        }
    }
}

/// Runs `launch` (which starts with `timeout`) as the leader of its own process group, and
/// returns its exit status and both output streams once every process of that group is gone.
///
/// `Command::output` waits for both pipes to close, and any process holding one keeps it waiting:
/// a session child that ignores `SIGTERM` survives `timeout`'s signal after `KWin` itself has
/// exited, and `timeout` sends its `SIGKILL` only while its own child still runs. So when
/// `timeout` exits:
/// 1. the group gets [`GROUP_EXIT_GRACE`] to empty on its own, so a server that cleans up after
///    its signal (Xvfb removing `/tmp/.X<n>-lock` and its socket) can finish;
/// 2. what remains is killed with `SIGKILL`, and the run waits up to [`GROUP_KILL_WAIT`] until no
///    member is left, because `kill(2)` returns once the signal is sent, not once the process is
///    gone;
/// 3. then `timeout` is reaped, and the pipes are read, waiting at most [`OUTPUT_DRAIN_SECS`].
///
/// `timeout` stays an unreaped zombie until step 3, which keeps its PID, and so the group's ID,
/// from being reused while the group is watched and signalled. When the drain times out, the
/// group is already empty: the pipe's holder left the group (`setsid`), the harness cannot name
/// it, and the run fails with the lock released.
///
/// # Errors
/// When the command cannot start, its exit cannot be awaited, the group cannot be signalled, a
/// member survives [`GROUP_KILL_WAIT`] after its `SIGKILL`, or a process outside the group still
/// holds an output pipe after [`OUTPUT_DRAIN_SECS`].
#[cfg(target_os = "linux")]
pub fn run_to_end(launch: &mut Command) -> std::io::Result<Output> {
    use std::os::unix::process::CommandExt as _;
    launch
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = launch.spawn()?;
    let group = child.id();
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    while !has_exited(group) {
        std::thread::sleep(EXIT_POLL_INTERVAL);
    }
    if !group_empties_within(group, GROUP_EXIT_GRACE) {
        kill_process_group(group)?;
        if !group_empties_within(group, GROUP_KILL_WAIT) {
            let survivors = group_members(group);
            // Reap `timeout` so it is not left a zombie; the survivors keep the group's ID.
            child.wait()?;
            return Err(std::io::Error::other(format!(
                "processes {survivors:?} of the run's group survived SIGKILL for {} s",
                GROUP_KILL_WAIT.as_secs()
            )));
        }
    }
    let status: ExitStatus = child.wait()?;
    let deadline = Instant::now() + Duration::from_secs(OUTPUT_DRAIN_SECS);
    Ok(Output {
        status,
        stdout: received_by(&stdout, deadline)?,
        stderr: received_by(&stderr, deadline)?,
    })
}

/// Runs `launch` as `Command::output` does: the output once both pipes close. Without `/proc`,
/// the harness cannot watch a process group, so the bounds of the Linux version do not hold; the
/// `KWin` display, whose leftovers they are for, runs only on Linux.
///
/// # Errors
/// When the command cannot start or be awaited.
#[cfg(not(target_os = "linux"))]
pub fn run_to_end(launch: &mut Command) -> std::io::Result<Output> {
    launch.stdin(Stdio::null()).output()
}

/// Reads `stream` to its end on a thread of its own and sends the bytes; a stream that was not
/// captured sends nothing at once.
#[cfg(target_os = "linux")]
fn drain(
    stream: Option<impl std::io::Read + Send + 'static>,
) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let Some(mut stream) = stream else {
        drop(sender.send(Vec::new()));
        return receiver;
    };
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Err(error) = stream.read_to_end(&mut bytes) {
            bytes.extend_from_slice(
                format!("\ngpu_run: reading the child's output failed: {error}\n").as_bytes(),
            );
        }
        // The receiver is gone only after its deadline passed; then nobody wants the bytes.
        drop(sender.send(bytes));
    });
    receiver
}

/// The bytes `receiver`'s reader sends before `deadline`.
///
/// # Errors
/// [`std::io::ErrorKind::TimedOut`] when the pipe is still open at `deadline`.
#[cfg(target_os = "linux")]
fn received_by(
    receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
    deadline: Instant,
) -> std::io::Result<Vec<u8>> {
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "a process outside the run's process group still held its output \
                     {OUTPUT_DRAIN_SECS} s after the group was gone"
                ),
            )
        })
}

/// Whether the child `pid` has exited: it is a zombie, or already gone.
#[cfg(target_os = "linux")]
fn has_exited(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |stat| {
        stat_fields(&stat)
            .and_then(|fields| fields.first().copied())
            .is_none_or(|state| state == "Z")
    })
}

/// The fields of a `/proc/<pid>/stat` line after the command name: state, parent PID, process
/// group, and so on.
#[cfg(target_os = "linux")]
fn stat_fields(stat: &str) -> Option<Vec<&str>> {
    stat.rsplit_once(')')
        .map(|(_, rest)| rest.split_whitespace().collect())
}

/// The live members of process group `group` other than its leader, which stays an unreaped
/// zombie while [`run_to_end`] watches the group. Zombies have exited and hold nothing.
#[cfg(target_os = "linux")]
pub fn group_members(group: u32) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| *pid != group)
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
                stat_fields(&stat).is_some_and(|fields| {
                    fields.first() != Some(&"Z")
                        && fields.get(2).and_then(|pgrp| pgrp.parse::<u32>().ok()) == Some(group)
                })
            })
        })
        .collect()
}

/// Whether process group `group` has no live member but its leader within `wait`.
#[cfg(target_os = "linux")]
fn group_empties_within(group: u32, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    loop {
        if group_members(group).is_empty() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(EXIT_POLL_INTERVAL);
    }
}

/// Sends `SIGKILL` to every process of the group `group` with `kill(1)`; a group with no
/// members left is not an error.
///
/// # Errors
/// When `kill` cannot be started.
#[cfg(target_os = "linux")]
fn kill_process_group(group: u32) -> std::io::Result<()> {
    // `kill` exits 1 when no process is left in the group.
    Command::new("kill")
        .args(["-s", "KILL", "--", &format!("-{group}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| ())
}

/// `timeout --kill-after=<grace> <limit>`, the start of every child command.
fn timeout_command(limit_secs: u32) -> Command {
    let mut command = Command::new("timeout");
    command
        .arg(format!("--kill-after={KILL_GRACE_SECS}"))
        .arg(limit_secs.to_string());
    command
}

/// Reads the part of the executable at `path` that the loader maps, once, in large sequential
/// reads, and returns how long that took, so the child starts with its code in the page cache.
/// The part ends where the last loadable segment ends ([`loaded_extent`]); a file that is not a
/// 64-bit little-endian ELF file is read whole.
///
/// Agent worktrees build their debug executables (about 235 MiB, of which about 88 MiB are mapped)
/// under `~/scratch`, a rotational disk shared with every user of the server, and the user slice's
/// page cache churns at its `memory.high`, so a gate often finds a GPU test's executable no longer
/// cached. The child then loads its code by page faults, one small random read at a time: measured
/// 738 major faults and 89.2 s for the windowed HUD child against 30.0 s warm, with the phases
/// before its first frame growing from 0.5 s to 52.8 s
/// (`docs/evidence/2026-11-10-gpu-child-timeouts.md`).
///
/// PERFORMANCE RATIONALE:
/// Why: Demand paging a cold executable from a busy rotational disk costs a seek per fault.
/// What: Read the mapped part sequentially before the lock, so readahead fetches it in large
///   requests and the child, the GPU lock and the run limit find it cached.
/// Trade-off: Mapped pages the child never touches are read too: a few hundredths of a second
///   when the file is cached, up to tens of seconds when it is cold, recorded as `preload_s`.
///
/// # Errors
/// The I/O error when the file cannot be opened or read; the spawn would fail on it anyway.
pub fn preload_executable(path: &Path) -> std::io::Result<Duration> {
    use std::io::{Read, Seek, SeekFrom};
    let started = Instant::now();
    let mut file = File::open(path)?;
    let read_at = |offset: u64, bytes: usize| -> Option<Vec<u8>> {
        let mut reader = &file;
        reader.seek(SeekFrom::Start(offset)).ok()?;
        let mut buffer = vec![0_u8; bytes];
        reader.read_exact(&mut buffer).ok()?;
        Some(buffer)
    };
    let extent = read_at(0, ELF_HEADER_BYTES).and_then(|header| loaded_extent(&header, read_at));
    let length = match extent {
        Some(end) => end,
        None => file.metadata()?.len(),
    };
    file.rewind()?;
    let mut mapped = file.take(length);
    let mut buffer = vec![0_u8; PRELOAD_CHUNK_BYTES];
    while mapped.read(&mut buffer)? > 0 {}
    Ok(started.elapsed())
}

/// Size of one read of [`preload_executable`], in bytes: 1 MiB.
const PRELOAD_CHUNK_BYTES: usize = 1 << 20;

/// Size of a 64-bit ELF file header, in bytes.
const ELF_HEADER_BYTES: usize = 64;

/// Size of a 64-bit ELF program header, in bytes; a larger `e_phentsize` is accepted.
const ELF_PROGRAM_HEADER_BYTES: usize = 56;

/// `PT_LOAD`, the program header type of a segment the loader maps.
const PT_LOAD: u32 = 1;

/// The file offset where the last loadable segment of a 64-bit little-endian ELF file ends, or
/// `None` if `header` is not such a file's header or its program headers cannot be read.
///
/// `read_at(offset, bytes)` reads `bytes` bytes at `offset` of the same file. Field offsets follow
/// the System V ABI: in the file header `e_phoff` at 32, `e_phentsize` at 54 and `e_phnum` at 56;
/// in a program header `p_type` at 0, `p_offset` at 8 and `p_filesz` at 32.
pub fn loaded_extent(
    header: &[u8],
    read_at: impl Fn(u64, usize) -> Option<Vec<u8>>,
) -> Option<u64> {
    let elf64_little_endian = header.len() >= ELF_HEADER_BYTES
        && header.starts_with(b"\x7fELF")
        && header[4] == 2
        && header[5] == 1;
    if !elf64_little_endian {
        return None;
    }
    let table = le_u64(header, 32)?;
    let entry = usize::from(le_u16(header, 54)?);
    let count = usize::from(le_u16(header, 56)?);
    if entry < ELF_PROGRAM_HEADER_BYTES {
        return None;
    }
    let programs = read_at(table, entry.checked_mul(count)?)?;
    let mut end: Option<u64> = None;
    for program in programs.chunks_exact(entry) {
        if le_u32(program, 0)? == PT_LOAD {
            let segment_end = le_u64(program, 8)?.checked_add(le_u64(program, 32)?)?;
            end = Some(end.map_or(segment_end, |end| end.max(segment_end)));
        }
    }
    end
}

/// The little-endian `u16` at byte `at` of `bytes`.
fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

/// The little-endian `u32` at byte `at` of `bytes`.
fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The little-endian `u64` at byte `at` of `bytes`.
fn le_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// What one child did, with the evidence of where it ran.
#[derive(Debug)]
pub struct GpuRun {
    /// Exit status and both output streams.
    pub output: Output,
    /// The driver it rendered on, or `None` for a lock self-test.
    pub driver: Option<GpuDriver>,
    /// The adapter it reported, if it logged one.
    pub adapter: Option<AdapterEvidence>,
    /// Time spent reading the executable before the lock ([`preload_executable`]).
    pub preload: Duration,
    /// Time spent waiting for the lock; `None` on `lavapipe`, which takes no lock.
    pub lock_wait: Option<Duration>,
    /// Time from spawn to exit, during which the lock (if any) was held.
    pub run_time: Duration,
    /// Where that time went, by phase.
    pub timeline: ChildTimeline,
    /// The line this run appended to the ledger, or would have appended had
    /// [`LEDGER_VAR`] named a file.
    pub ledger: String,
}

impl GpuRun {
    /// Everything the child printed, wherever the wrapper delivered it.
    pub fn log(&self) -> String {
        captured_log(&self.output)
    }

    /// The start of every failure message: `<label> on <driver> (<adapter>)`.
    pub fn context(&self, label: &str) -> String {
        let driver = self.driver.map_or("no driver", GpuDriver::name);
        match &self.adapter {
            Some(adapter) => format!("{label} on {driver} ({adapter})"),
            None => format!("{label} on {driver} (no adapter line)"),
        }
    }

    /// Fails on a timeout, an unsuccessful exit, or a renderer's wrong or missing adapter.
    fn assert_completed(&self, label: &str, run_limit_secs: u32) {
        let context = self.context(label);
        if self.output.status.code() == Some(TIMEOUT_EXIT_CODE) {
            let (slowest, spent) = self
                .timeline
                .slowest()
                .unwrap_or((TAIL_PHASE, self.run_time));
            panic!(
                "{context} did not finish within {run_limit_secs} s; the slowest phase was \
                 {slowest} ({:.1} s). Phases: {}. Reading the executable beforehand took {:.1} s, \
                 outside the limit. Last output:\n{}",
                spent.as_secs_f64(),
                self.timeline,
                self.preload.as_secs_f64(),
                tail(&self.log())
            );
        }
        assert!(
            self.output.status.success(),
            "{context} exited with {:?}. Last output:\n{}",
            self.output.status.code(),
            tail(&self.log())
        );
        if let Some(driver) = self.driver {
            if let Some(violation) = AdapterEvidence::check(self.adapter.as_ref(), driver) {
                panic!(
                    "{context}: {violation}. Last output:\n{}",
                    tail(&self.log())
                );
            }
        }
    }

    /// One tab-separated ledger line: label, driver, display, lock, adapter, `driver_info`,
    /// executable preload, lock wait and run time in seconds, exit status, and the
    /// [`ChildTimeline`].
    fn ledger_line(&self, label: &str, lock: &str, display: &str) -> String {
        let (adapter, driver_info) = self.adapter.as_ref().map_or(("-", "-"), |adapter| {
            (adapter.name.as_str(), adapter.driver_info.as_str())
        });
        let lock_wait = self.lock_wait.map_or_else(
            || "-".to_owned(),
            |wait| format!("{:.2}", wait.as_secs_f64()),
        );
        let exit = self
            .output
            .status
            .code()
            .map_or_else(|| "signal".to_owned(), |code| code.to_string());
        format!(
            "label={label}\tdriver={}\tdisplay={display}\tlock={lock}\tadapter={adapter}\t\
             driver_info={driver_info}\t\
             preload_s={:.2}\tlock_wait_s={lock_wait}\trun_s={:.2}\texit={exit}\tphases={}\n",
            self.driver.map_or("none", GpuDriver::name),
            self.preload.as_secs_f64(),
            self.run_time.as_secs_f64(),
            self.timeline,
        )
    }
}

/// The ledger line of a child that never ran because its wait for the lock expired: the columns of
/// [`GpuRun`]'s line, with no adapter, `run_s=0.00`, `exit=lock-timeout` and no phases, so a gate
/// shows the starved child next to the ones that ran (B-136).
pub fn lock_timeout_ledger_line(
    label: &str,
    driver: Option<GpuDriver>,
    display: &str,
    lock: &str,
    preload: Duration,
    waited: Duration,
) -> String {
    format!(
        "label={label}\tdriver={}\tdisplay={display}\tlock={lock}\tadapter=-\t\
         driver_info=-\tpreload_s={:.2}\tlock_wait_s={:.2}\trun_s=0.00\t\
         exit={LOCK_TIMEOUT_EXIT}\tphases=-\n",
        driver.map_or("none", GpuDriver::name),
        preload.as_secs_f64(),
        waited.as_secs_f64(),
    )
}

/// The ledger's `exit=` value of a child whose wait for the lock expired before it started.
pub const LOCK_TIMEOUT_EXIT: &str = "lock-timeout";

/// Appends `line` to the file named by [`LEDGER_VAR`], if set, in one `O_APPEND` write, so lines
/// from concurrent test processes never interleave.
///
/// # Panics
/// Panics if the ledger cannot be written: a gate that asked for evidence must not lose it.
fn append_to_ledger(line: &str) {
    let Some(path) = std::env::var_os(LEDGER_VAR) else {
        return;
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|error| {
            panic!(
                "cannot open the GPU run ledger {}: {error}",
                Path::new(&path).display()
            )
        });
    let written = file.write(line.as_bytes()).unwrap_or_else(|error| {
        panic!(
            "cannot write the GPU run ledger {}: {error}",
            Path::new(&path).display()
        )
    });
    assert_eq!(written, line.len(), "the GPU run ledger line was cut short");
}

/// Writes `log` to `<dir>/<label>-<pid>-<n>.log` if [`RUN_LOG_DIR_VAR`] names a directory `dir`,
/// with every character of the label that is not alphanumeric replaced by `_`.
///
/// # Panics
/// Panics if the log cannot be written: a run that asked for evidence must not lose it.
fn keep_run_log(label: &str, log: &str) {
    static NEXT_LOG: AtomicU32 = AtomicU32::new(0);
    let Some(directory) = std::env::var_os(RUN_LOG_DIR_VAR) else {
        return;
    };
    let name: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let path = Path::new(&directory).join(format!(
        "{name}-{}-{}.log",
        std::process::id(),
        NEXT_LOG.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, log)
        .unwrap_or_else(|error| panic!("cannot write the GPU run log {}: {error}", path.display()));
}

// ---------------------------------------------------------------------------------------------
// Where a child's time went
// ---------------------------------------------------------------------------------------------

/// Log records that mark the end of a phase of a Bevy child, in the order they occur, with the
/// name of the phase each one ends. The first timestamped record ends the launch phase.
const PHASE_MARKERS: [(&str, &str); 2] = [
    (ADAPTER_MARKER, "render device"),
    ("Creating new window", "app build"),
];

/// Name of the phase from the spawn to the child's first timestamped log record: `xvfb-run`
/// starting its server, the binary loading, and whatever the child does before Bevy's
/// `LogPlugin` (the windowed client loads its battle definition there).
const LAUNCH_PHASE: &str = "launch";

/// Name of the phase from the last milestone to the last timestamped log record.
const RUN_PHASE: &str = "startup and frames";

/// Name of the phase from the last timestamped log record to the child's exit.
const TAIL_PHASE: &str = "after the last log record";

/// Where one child's wall time went, read from the timestamps of its own log records.
///
/// Phases are consecutive and end at milestones: the first timestamped record, the adapter line
/// (render device created), a window being created (windowed children only), the last timestamped
/// record, and the exit the harness observed. A milestone the log does not show is skipped, and
/// its time counts toward the next phase: an offscreen child creates no window, so its app
/// build counts toward "startup and frames".
#[derive(Debug, Clone, PartialEq)]
pub struct ChildTimeline {
    /// Each phase with its duration, in order.
    pub phases: Vec<(&'static str, Duration)>,
}

impl ChildTimeline {
    /// Reads the timeline of a child spawned at `spawned` (wall clock) that ran for `run_time`.
    ///
    /// Log timestamps are the RFC 3339 UTC times `tracing-subscriber` prints, such as
    /// `2026-11-10T00:46:32.893283Z`, possibly wrapped in ANSI colour codes. Records before the
    /// spawn or after the exit (clock steps) are clamped into the run.
    pub fn read(log: &str, spawned: SystemTime, run_time: Duration) -> Self {
        let mut milestones: Vec<(&'static str, Duration)> = Vec::new();
        let mut next_marker = 0;
        let mut last_record = None;
        for line in log.lines() {
            let Some(at) = log_record_time(line) else {
                continue;
            };
            let offset = at
                .duration_since(spawned)
                .unwrap_or(Duration::ZERO)
                .min(run_time);
            if last_record.is_none() {
                milestones.push((LAUNCH_PHASE, offset));
            }
            last_record = Some(offset);
            if let Some(&(marker, phase)) = PHASE_MARKERS.get(next_marker) {
                if line.contains(marker) {
                    milestones.push((phase, offset));
                    next_marker += 1;
                }
            }
        }
        if let Some(last) = last_record {
            milestones.push((RUN_PHASE, last));
        }
        milestones.push((TAIL_PHASE, run_time));
        let mut phases = Vec::with_capacity(milestones.len());
        let mut start = Duration::ZERO;
        for (phase, end) in milestones {
            let end = end.max(start);
            phases.push((phase, end.saturating_sub(start)));
            start = end;
        }
        Self { phases }
    }

    /// The longest phase, which a timeout message names.
    pub fn slowest(&self) -> Option<(&'static str, Duration)> {
        self.phases
            .iter()
            .copied()
            .max_by_key(|&(_, duration)| duration)
    }
}

impl fmt::Display for ChildTimeline {
    /// `launch 2.1 s, render device 6.0 s, ...`, one decimal.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, (phase, duration)) in self.phases.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            write!(formatter, "{phase} {:.1} s", duration.as_secs_f64())?;
        }
        Ok(())
    }
}

/// The wall-clock time of a `tracing-subscriber` record, or `None` for any other line.
///
/// Accepts `YYYY-MM-DDTHH:MM:SS[.fraction]Z` at the start of the line once ANSI escape sequences
/// are removed.
pub fn log_record_time(line: &str) -> Option<SystemTime> {
    let plain = strip_ansi(line);
    let stamp = plain.split_whitespace().next()?.strip_suffix('Z')?;
    let (date, time) = stamp.split_once('T')?;
    let mut date_fields = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (
        date_fields.next()?.ok()?,
        date_fields.next()?.ok()?,
        date_fields.next()?.ok()?,
    );
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut clock_fields = clock.splitn(3, ':').map(str::parse::<u64>);
    let (hour, minute, second) = (
        clock_fields.next()?.ok()?,
        clock_fields.next()?.ok()?,
        clock_fields.next()?.ok()?,
    );
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    let nanos = if fraction.is_empty() {
        0
    } else {
        let digits: String = fraction
            .chars()
            .chain(std::iter::repeat('0'))
            .take(9)
            .collect();
        digits.parse::<u32>().ok()?
    };
    let days = u64::try_from(days_from_civil(year, month, day)).ok()?;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(SystemTime::UNIX_EPOCH + Duration::new(seconds, nanos))
}

/// Days from 1970-01-01 to the proleptic Gregorian date `year-month-day` (H. Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

// ---------------------------------------------------------------------------------------------
// Validation (remediation finding Q-7)
// ---------------------------------------------------------------------------------------------

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
             it. The extension only adds wide-gamut swapchain colour spaces. Mesa 25.0.7 offers it \
             on both radv and lavapipe, so the entry applies only to older lavapipe hosts.",
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

/// Everything a run's log says about validation and the adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    /// Whether the loader reported inserting the Khronos validation layer.
    pub layer_active: bool,
    /// All warnings and errors from the Vulkan messenger, in order.
    pub findings: Vec<ValidationFinding>,
    /// `ERROR` lines from any other target, such as a failed Bevy shader compilation.
    pub error_lines: Vec<String>,
    /// The adapter the run reported, if it logged one.
    pub adapter: Option<AdapterEvidence>,
    /// The driver the run should have rendered on. When set, [`Self::violations`] checks the
    /// adapter against it. [`GpuCommand::run_validated`] sets it; [`Self::parse`] does not.
    pub expected_driver: Option<GpuDriver>,
}

impl ValidationReport {
    /// Parses the [`captured_log`] of a child that used [`VALIDATED_RUST_LOG`].
    pub fn parse(stderr: &str) -> Self {
        let mut report = Self {
            adapter: AdapterEvidence::parse(stderr),
            ..Self::default()
        };
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

    /// The same report, required to have rendered on `driver`.
    #[must_use]
    pub fn expecting(mut self, driver: GpuDriver) -> Self {
        self.expected_driver = Some(driver);
        self
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
    ///
    /// With [`Self::expected_driver`] set, a missing adapter line or one naming another driver is a
    /// violation too.
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
        if let Some(driver) = self.expected_driver {
            violations.extend(AdapterEvidence::check(self.adapter.as_ref(), driver));
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

/// Returns `true` for a best-practices message ID from any supported layer version.
///
/// Layers name these `BestPractices-...`; 1.3.239 and other older ones name them
/// `UNASSIGNED-BestPractices-...`. Other `UNASSIGNED-` IDs are not best-practices findings.
pub fn is_best_practices_id(id: &str) -> bool {
    id.strip_prefix("UNASSIGNED-")
        .unwrap_or(id)
        .starts_with("BestPractices-")
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

/// Joins both output streams of a run, keeping each one's lines whole and in order.
///
/// Lines a binary prints with `println!` are harmless, since the parser accepts only log records.
pub fn merge_streams(stdout: &[u8], stderr: &[u8]) -> String {
    let mut log = String::from_utf8_lossy(stderr).into_owned();
    if !log.is_empty() && !log.ends_with('\n') {
        log.push('\n');
    }
    log.push_str(&String::from_utf8_lossy(stdout));
    log
}

/// Returns everything a run printed, wherever the wrapper delivered its log output.
pub fn captured_log(output: &Output) -> String {
    merge_streams(&output.stdout, &output.stderr)
}

/// Returns the last 30 lines of `text`, enough to diagnose a crash without flooding the report.
pub fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(30)..].join("\n")
}
