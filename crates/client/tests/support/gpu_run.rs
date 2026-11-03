//! Runs real client binaries on a test GPU, one locked child at a time, and turns Vulkan
//! validation output into a verdict.
//!
//! # Responsibility
//! Owns everything about how a GPU test child is launched:
//! - **Driver selection.** [`select_driver`] chooses `radv` (the physical GPU) or `lavapipe`
//!   (software Vulkan) once per test process from `SPACE_SCUFFLE_GPU_TEST_DRIVER` and the host's
//!   prerequisites, and never falls back from a forced choice.
//! - **The shared GPU lock.** On `radv`, [`GpuLock`] takes `/run/lock/gpu.lock`, the file
//!   `with-gpu-lock` locks, immediately before the child starts and releases it when the child
//!   exits. Nothing else holds it, so other users of the server get the GPU between children.
//! - **The launch environment**, the run limit, the evidence of which adapter rendered, the
//!   opt-out policy, and the gate on Vulkan validation findings (remediation finding Q-7).
//!
//! The decisions and their measurements are in `docs/plans/2026-11-03-gpu-test-harness.md`
//! (sections 3 and 4, decisions D1 to D9).
//!
//! # How a child runs
//! 1. [`virtual_gpu_command`] builds a [`GpuCommand`]: GNU `timeout`, then `xvfb-run`, then the
//!    binary, with the selected driver's environment. Callers add arguments and variables.
//! 2. [`GpuCommand::run`], [`GpuCommand::run_validated`] or [`GpuCommand::assert_passes_validation`]
//!    is the only way to start it. A `GpuCommand` exposes no `output`, `spawn` or `status`, so no
//!    caller can run a GPU child without the lock.
//! 3. The run takes the lock (radv only), spawns the child, waits for it, releases the lock, then
//!    checks the exit status and the adapter line, and appends a line to the ledger if
//!    `SPACE_SCUFFLE_GPU_RUN_LEDGER` names a file.
//!
//! `SPACE_SCUFFLE_VIRTUAL_GPU=1`, which every child receives, means "a test display": the client
//! keeps its window off the desktop. It does not mean a software GPU.
//!
//! # How validation reaches this module
//! 1. `WGPU_VALIDATION=1` makes `wgpu_hal` load `VK_LAYER_KHRONOS_validation` and register a debug
//!    messenger. The layer writes through that messenger, never to a file.
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
//! - `wgpu_hal` drops three message IDs before logging them: `VUID-VkSwapchainCreateInfoKHR-pNext-07781`,
//!   `VUID-vkCmdEndDebugUtilsLabelEXT-commandBuffer-01912` on layers 1.3.240 to 1.3.250, and
//!   `VUID-VkRenderPassBeginInfo-framebuffer-04627` when the OBS layer is present. This gate
//!   cannot see them.
//! - Only code paths the binary reaches during the run are validated.
//! - On `lavapipe` before Mesa 25.1, the runs that draw hex-tiled ground render without
//!   anisotropic filtering, because that driver's sampler does not finish such a frame (see
//!   [`GpuCommand::allow_for_hex_tiling_driver_defect`]). `radv` always uses the production sampler.
//! - On `radv`, presentation uses `MESA_VK_WSI_DEBUG=sw`, because Xvfb has no DRI3. The GPU
//!   renders every frame, but the DRI3 present path a desktop uses is not exercised.
//! - If the test process itself is killed with `SIGKILL`, its child keeps running without the
//!   lock for at most the run limit plus the kill grace ([`GpuDriver::run_limit_secs`],
//!   [`KILL_GRACE_SECS`]).
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
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The lock file `with-gpu-lock` takes with `flock(1)`, shared by every user of the server's GPU.
///
/// `flock(1)` calls `flock(2)`, and so does [`File::try_lock`] on Linux, so both take part in the
/// same protocol. `the_host_lock_tool_uses_the_harness_lock_file` fails if the host script names
/// another file.
pub const GPU_LOCK_PATH: &str = "/run/lock/gpu.lock";

/// Default wait for the shared lock, in seconds (4 min 50 s), when `GPU_LOCK_TIMEOUT` is unset.
///
/// The owner's decision in `docs/questions/closed/0012-gpu-lock-peer-and-wait-limit.md`: every
/// user of the GPU now locks per device use, so a wait should last seconds; 290 s tolerates a
/// slow peer while failing a gate that is stuck behind a suite-wide hold. A single long command
/// that needs more sets `GPU_LOCK_TIMEOUT`.
pub const DEFAULT_LOCK_WAIT_SECS: u64 = 290;

/// Variable that overrides [`DEFAULT_LOCK_WAIT_SECS`], in whole seconds, as for `with-gpu-lock`.
pub const LOCK_WAIT_VAR: &str = "GPU_LOCK_TIMEOUT";

/// Interval between two attempts to take a busy lock.
///
/// Peers block in the kernel and are woken the moment the lock is released, so a harness that
/// polls lets a waiting peer win the next slot (plan decision D2).
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Variable that forces a driver: `radv`, `lavapipe` or `auto`. Unset means `auto`.
pub const DRIVER_VAR: &str = "SPACE_SCUFFLE_GPU_TEST_DRIVER";

/// Variable naming a file that receives one line per child run (plan decision D6).
pub const LEDGER_VAR: &str = "SPACE_SCUFFLE_GPU_RUN_LEDGER";

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
/// 130 s with [`KILL_GRACE_SECS`].
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

/// Terrain sampler override read by `client::terrain::theme_gpu`.
const TERRAIN_ANISOTROPY_OVERRIDE_ENV: &str = "SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY";

/// First Mesa release, as `(major, minor)`, whose llvmpipe bounds anisotropic sampling.
///
/// The bounded algorithm from the Vulkan specification is absent from the 25.0.0 source of
/// `lp_bld_sample_soa.c` and present in 25.1.0.
const FIRST_BOUNDED_ANISOTROPY_MESA: (u32, u32) = (25, 1);

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockHolder {
    /// Process ID that placed the lock.
    pub pid: u32,
    /// Its command line, arguments joined by spaces, or empty if unreadable.
    pub command_line: String,
}

impl fmt::Display for LockHolder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "PID {}: `{}`", self.pid, self.command_line)
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
    /// Takes the exclusive lock on `path`, retrying every [`LOCK_POLL_INTERVAL`] for up to `wait`.
    ///
    /// Opens the file for reading and writing, creating it with mode 0666 (subject to the umask)
    /// as `flock(1)` does. On the first refusal it checks whether an ancestor of this process is
    /// the holder, and fails at once if so. Returns the guard and the time spent waiting.
    ///
    /// # Errors
    /// [`GpuLockError`]: the file cannot be opened, an ancestor holds the lock, `wait` expired,
    /// or `flock(2)` failed.
    pub fn acquire(path: &Path, wait: Duration) -> Result<(Self, Duration), GpuLockError> {
        let file = open_lock_file(path).map_err(|source| GpuLockError::Open {
            path: path.to_owned(),
            source,
        })?;
        let started = Instant::now();
        let mut checked_ancestors = false;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok((Self { _file: file }, started.elapsed())),
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(source)) => {
                    return Err(GpuLockError::Io {
                        path: path.to_owned(),
                        source,
                    })
                }
            }
            if !checked_ancestors {
                checked_ancestors = true;
                if let Some(holder) = lock_holder(&file).filter(|h| is_ancestor(h.pid)) {
                    return Err(GpuLockError::HeldByAncestor {
                        path: path.to_owned(),
                        holder,
                    });
                }
            }
            let waited = started.elapsed();
            if waited >= wait {
                return Err(GpuLockError::TimedOut {
                    path: path.to_owned(),
                    waited,
                    holder: lock_holder(&file),
                });
            }
            std::thread::sleep(LOCK_POLL_INTERVAL.min(wait.saturating_sub(waited)));
        }
    }
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
    let command_line = std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|raw| command_line_text(&raw))
        .unwrap_or_default();
    Some(LockHolder { pid, command_line })
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

/// Returns `true` if `pid` is a strict ancestor of this process. This process itself is not:
/// another test thread of the same process may hold the lock, and that one is worth waiting for.
fn is_ancestor(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let parent = std::os::unix::process::parent_id();
        ancestor_chain(parent, |child| {
            std::fs::read_to_string(format!("/proc/{child}/stat"))
                .ok()
                .as_deref()
                .and_then(stat_parent_pid)
        })
        .contains(&pid)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
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
/// `radv` never needs it. `lavapipe` needs `"1"` before [`FIRST_BOUNDED_ANISOTROPY_MESA`]; an
/// unknown release keeps the production sampler, so an affected driver fails by timeout rather
/// than quietly losing coverage.
pub fn anisotropy_override(
    driver: GpuDriver,
    lavapipe_release: Option<(u32, u32)>,
) -> Option<&'static str> {
    match driver {
        GpuDriver::Radv => None,
        GpuDriver::Lavapipe => lavapipe_release
            .filter(|release| *release < FIRST_BOUNDED_ANISOTROPY_MESA)
            .map(|_| "1"),
    }
}

/// Extracts `(major, minor)` from `vulkaninfo --summary` text with a line such as
/// `driverInfo = Mesa 22.3.6 (LLVM 15.0.6)`, or returns `None` for any other driver.
pub fn mesa_release(summary: &str) -> Option<(u32, u32)> {
    let line = summary.lines().find(|line| line.contains("driverInfo"))?;
    let (_, version) = line.split_once("Mesa ")?;
    let mut parts = version.split(['.', ' ', '-']);
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Returns `true` when this Mesa release cannot finish a frame of hex-tiled terrain.
///
/// Before [`FIRST_BOUNDED_ANISOTROPY_MESA`], llvmpipe's anisotropic sampler ignores explicit
/// gradients and `maxAnisotropy`. Any clamp above 1 selects that path, so a smaller window or
/// a longer deadline does not help: measured at 64x36 the first terrain frame was still
/// unfinished after 120 s. The measurements are in `docs/plans/2026-11-02-server-gate.md`.
pub fn driver_stalls_on_anisotropic_hex_tiling(summary: &str) -> bool {
    mesa_release(summary).is_some_and(|release| release < FIRST_BOUNDED_ANISOTROPY_MESA)
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
    /// The ledger's name for this policy.
    fn name(&self) -> &'static str {
        match self {
            Self::None => "none",
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

/// A GPU child under construction: binary, display, driver environment, run limit and lock.
///
/// It deliberately offers no way to start the child except through the lock: [`Self::run`],
/// [`Self::try_run`], [`Self::run_validated`] and [`Self::assert_passes_validation`].
#[derive(Debug)]
pub struct GpuCommand {
    /// `timeout`, then `xvfb-run` (renderers only), then the program and its arguments.
    command: Command,
    /// Renderer on a driver, or lock self-test.
    kind: ChildKind,
    /// The lock the run takes around the spawn.
    lock: LockPolicy,
    /// The run limit given to `timeout`, in seconds.
    run_limit_secs: u32,
}

/// Builds a [`GpuCommand`] that runs `binary` on a virtual display with the selected driver.
///
/// # Panics
/// Panics when no driver can be selected, or when neither `xvfb-run` nor a display exists. A GPU
/// test must not pass without running.
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

/// Builds a [`virtual_gpu_command`] whose display has the size `screen`, such as `1600x900x24`.
///
/// # Panics
/// As [`virtual_gpu_command`].
pub fn virtual_gpu_command_on_screen(binary: &Path, screen: Option<&str>) -> GpuCommand {
    GpuCommand::renderer(binary, screen, selected_driver())
}

impl GpuCommand {
    /// Builds a renderer child for `selection`. GNU `timeout` wraps the run, because
    /// [`Command::output`] has no deadline and a client that loses its display can spin forever.
    /// It signals the whole process group, including the client under `xvfb-run`, and exits with
    /// [`TIMEOUT_EXIT_CODE`]. The lock is taken before `timeout` starts, so waiting for it never
    /// eats the run limit.
    fn renderer(binary: &Path, screen: Option<&str>, selection: &GpuSelection) -> Self {
        let driver = selection.driver;
        let run_limit_secs = driver.run_limit_secs();
        let mut command = timeout_command(run_limit_secs);
        let xvfb_run = Path::new("/usr/bin/xvfb-run");
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
        command.env("VK_ICD_FILENAMES", &selection.manifest);
        command.env("WGPU_ADAPTER_NAME", driver.adapter_driver());
        let lock = match driver {
            GpuDriver::Radv => {
                // Xvfb has no DRI3, and radv's X11 presentation needs it unless `sw` selects the
                // software present path. Rendering stays on the GPU (plan section 3.1).
                command.env("MESA_VK_WSI_DEBUG", "sw");
                command.env_remove("LIBGL_ALWAYS_SOFTWARE");
                LockPolicy::Shared
            }
            GpuDriver::Lavapipe => {
                command.env_remove("MESA_VK_WSI_DEBUG");
                command.env("LIBGL_ALWAYS_SOFTWARE", "1");
                LockPolicy::None
            }
        };
        Self {
            command,
            kind: ChildKind::Renderer(driver),
            lock,
            run_limit_secs,
        }
    }

    /// Builds a self-test child of the lock: `program` under `timeout`, with no display and no
    /// driver, holding the private lock file `lock` for its run after waiting up to `wait`.
    ///
    /// Only the harness's own lock tests use this, so they run on every host, CI included, and
    /// never touch [`GPU_LOCK_PATH`].
    pub fn lock_probe(program: &Path, lock: &Path, wait: Duration) -> Self {
        let mut command = timeout_command(LOCK_PROBE_RUN_LIMIT_SECS);
        command.arg(program);
        Self {
            command,
            kind: ChildKind::LockProbe,
            lock: LockPolicy::Private(lock.to_owned(), wait),
            run_limit_secs: LOCK_PROBE_RUN_LIMIT_SECS,
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

    /// Sets an environment variable for the child.
    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.command.env(key, value);
        self
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
    /// As [`Self::run`] for everything after the lock.
    pub fn try_run(mut self, label: &str) -> Result<GpuRun, GpuLockError> {
        self.keep_adapter_line();
        let (guard, lock_wait) = match &self.lock {
            LockPolicy::None => (None, None),
            LockPolicy::Shared => {
                let (guard, waited) =
                    GpuLock::acquire(Path::new(GPU_LOCK_PATH), shared_lock_wait())?;
                (Some(guard), Some(waited))
            }
            LockPolicy::Private(path, wait) => {
                let (guard, waited) = GpuLock::acquire(path, *wait)?;
                (Some(guard), Some(waited))
            }
        };
        let started = Instant::now();
        let output = self.command.output();
        let run_time = started.elapsed();
        drop(guard);
        let output = output.unwrap_or_else(|error| {
            panic!("{label}: failed to launch {:?}: {error}", self.command)
        });
        let driver = self.driver();
        let adapter = AdapterEvidence::parse(&captured_log(&output));
        let run = GpuRun {
            output,
            driver,
            adapter,
            lock_wait,
            run_time,
        };
        append_to_ledger(&run.ledger_line(label, self.lock.name()));
        run.assert_completed(label, self.run_limit_secs);
        Ok(run)
    }

    /// Runs the child under validation and returns the run with the parsed report, whose
    /// violations include the adapter check. Exit status is checked as in [`Self::run`].
    ///
    /// # Panics
    /// As [`Self::run`].
    pub fn run_validated(mut self, label: &str) -> (GpuRun, ValidationReport) {
        self.command.env("WGPU_VALIDATION", "1");
        // Replaced so a developer's own filter cannot hide findings.
        self.command.env("RUST_LOG", VALIDATED_RUST_LOG);
        let driver = self.driver();
        let run = self.run(label);
        let mut report = ValidationReport::parse(&run.log());
        report.expected_driver = driver;
        (run, report)
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

/// `timeout --kill-after=<grace> <limit>`, the start of every child command.
fn timeout_command(limit_secs: u32) -> Command {
    let mut command = Command::new("timeout");
    command
        .arg(format!("--kill-after={KILL_GRACE_SECS}"))
        .arg(limit_secs.to_string());
    command
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
    /// Time spent waiting for the lock; `None` on `lavapipe`, which takes no lock.
    pub lock_wait: Option<Duration>,
    /// Time from spawn to exit, during which the lock (if any) was held.
    pub run_time: Duration,
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
        assert_ne!(
            self.output.status.code(),
            Some(TIMEOUT_EXIT_CODE),
            "{context} did not finish within {run_limit_secs} s. Last output:\n{}",
            tail(&self.log())
        );
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

    /// One tab-separated ledger line: label, driver, lock, adapter, `driver_info`, lock wait and
    /// run time in seconds, and exit status.
    fn ledger_line(&self, label: &str, lock: &str) -> String {
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
            "label={label}\tdriver={}\tlock={lock}\tadapter={adapter}\tdriver_info={driver_info}\t\
             lock_wait_s={lock_wait}\trun_s={:.2}\texit={exit}\n",
            self.driver.map_or("none", GpuDriver::name),
            self.run_time.as_secs_f64(),
        )
    }
}

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
