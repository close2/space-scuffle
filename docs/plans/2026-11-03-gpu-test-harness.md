# GPU test harness on the physical GPU, one lock per test child

Date: 2026-11-03. Status: Done (CI run 41643796170 on `main` at `b95d179` is green). Host: the shared Debian 13 VM (16 cores of an AMD EPYC 7301, load average
about 30 during the probes), AMD Radeon RX 580 on `radv` and `lavapipe`, both Mesa 25.0.7, Vulkan validation
layers 1.4.309. CI: GitHub `ubuntu-latest`, no GPU, `lavapipe` on Mesa 25.2.8.

## 1. Goal and owner instruction

The owner wrote, on 2026-11-03:

> please use the GPU, do not try to avoid it! All other processes on this server need it maybe 1% of the time. Just
> give other processes a chance to get a short slot between your tasks. (I.e. don't block the GPU for hours). the
> other processes do the same. So it is possible (but probably even unlikely) that you might have to wait a few
> seconds for a GPU task, but do not avoid using the GPU.

AGENTS.md section 7 already follows this for ad hoc work. Its bullet "The test harness still pins GPU tests to
`lavapipe`" names this plan as the remaining step. After this plan:

- On this server, `cargo test --workspace` runs every client GPU test child on the RX 580 through `radv`.
- Each child takes the shared GPU lock just before it starts and releases it when it exits. No lock is held during
  compilation, between children, or across a test binary.
- CI, which has no GPU, keeps running the same tests on `lavapipe` with no lock. An explicit variable forces either
  driver, so this server can reproduce a CI run exactly.

## 2. Current state

### 2.1 The harness

`crates/client/tests/support/gpu_run.rs` (485 lines) owns the launch environment. `virtual_gpu_command` builds
`timeout --kill-after=10 300 xvfb-run -n <display> -a [-s <screen>] <binary>` and, whenever a `lavapipe` manifest
exists, sets `VK_ICD_FILENAMES` to it, `WGPU_ADAPTER_NAME=llvmpipe` and `LIBGL_ALWAYS_SOFTWARE=1`. It returns a
plain `std::process::Command`, and each caller runs it itself.

`allow_for_hex_tiling_driver_defect` sets `SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY=1` whenever the installed `lavapipe`
is older than Mesa 25.1 ([Q0009](../questions/closed/0009-terrain-anisotropy-test-override.md)). It asks
`vulkaninfo` about the `lavapipe` manifest whatever driver the child will really use. Once a child runs on `radv`,
that check would wrongly lower anisotropy, because this server's `lavapipe` is 25.0.7.

### 2.2 The GPU children

Ten test binaries include `gpu_run.rs`. Twelve tests start a GPU child:

| Binary | Test | Child | How it runs the child | Calls the anisotropy override |
|---|---|---|---|---|
| `battle_hud_window_tests` | `the_skirmish_window_gives_the_camera_only_what_the_panels_leave` | self, `windowed_hud_child` | `command.output()` | no |
| `battle_result_window_tests` | `the_exit_button_ends_the_real_client_successfully` | self, `windowed_result_child` | `command.output()` | yes |
| `dome_fill_tests` | `a_foundation_fills_from_rim_to_apex_on_the_real_renderer` | self, `windowed_dome_child` | `run_validated` | no |
| `editor_app_tests` | `test_render_verification_under_xvfb` | `map_editor --verify-render`, 1600x900 | `assert_passes_validation` | yes |
| `gpu_validation_tests` | `real_layer_findings_reach_the_gate` | `client --lab`, best practices on | `run_validated` | no |
| `gpu_validation_tests` | `live_battle_passes_vulkan_validation` | `client --battle` | `assert_passes_validation` | no |
| `gpu_validation_tests` | `skirmish_passes_vulkan_validation` | `client --skirmish` | `assert_passes_validation` | yes |
| `lab_scene_tests` | `smog_does_not_wash_out_bubbles_beyond_the_volume_centre` | `client --swarm` | `assert_passes_validation` | no |
| `round2_visual_tests` | `test_real_gpu_startup_and_frame_limit_30` | `client --lab` | `assert_passes_validation` | no |
| `skirmish_match_tests` | `the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window` | self, `windowed_skirmish_child` | `assert_passes_validation` | yes |
| `smog_bouncing_bubbles_tests` | `smog_steps_reach_the_gpu_through_queue_writes` | `client --lab` | `command.output()` | no |
| `terrain_tests` | `test_terrain_preview_renders` | `terrain_preview` | `assert_passes_validation` | yes |

Three callers run the child with `command.output()` directly. A lock taken inside `run_validated` alone would
therefore miss them, which is why the decisions below take the plain `Command` away from callers.

### 2.3 CI

`.github/workflows/ci.yml` installs `mesa-vulkan-drivers`, `vulkan-validationlayers` and `xvfb`, sets
`SPACE_SCUFFLE_VIRTUAL_GPU=1`, and runs `cargo test --workspace --locked` with `timeout-minutes: 60`. The runner has
no `/dev/dri` render node, no `radeon_icd.json` worth using, and no `with-gpu-lock`.

### 2.4 The lock

`/usr/local/bin/with-gpu-lock` is `exec flock --timeout "${GPU_LOCK_TIMEOUT:-600}" /run/lock/gpu.lock "$@"`.
`/run/lock/gpu.lock` is mode 0666. `flock(1)` uses `flock(2)`, so any process that calls `flock(2)` on that file
takes part in the same protocol. Rust's `std::fs::File::lock` and `File::try_lock` (stable since 1.89; the
toolchain is 1.99.0) call `flock(2)` on Linux.

## 3. Probe results

Every probe used the main checkout's debug binaries from `main` at `d469e16`, ran one command at a time, and wrapped
each GPU command in its own `with-gpu-lock`. Logs are in `~/scratch/logs/gpu-plan/`. Test children ran through a
probe-only `timeout` shim placed first on `PATH`. The shim replaced the harness's `lavapipe` variables with
`VK_ICD_FILENAMES=.../radeon_icd.json` and `MESA_VK_WSI_DEBUG=sw`, removed `WGPU_ADAPTER_NAME`,
`LIBGL_ALWAYS_SOFTWARE` and the anisotropy override, took the lock with `with-gpu-lock`, and recorded lock wait
and child time separately. The parent test's own checks (validation gate, pixel thresholds) ran unchanged.

### 3.1 Validation on `radv` with `MESA_VK_WSI_DEBUG=sw` is clean, and 08739 is not real

| Run (harness environment: `WGPU_VALIDATION=1`, `RUST_LOG=warn,wgpu_hal=info,bevy_render::renderer=info`) | Exit | Run time | Layer inserted | WARN or ERROR lines |
|---|---|---|---|---|
| `client --battle --max-frames 60` | 0 | 23.0 s | yes | 0 |
| `client --skirmish --max-frames 60` | 0 | about 24 s | yes | 0 |
| `client --swarm --max-frames 40` | 0 | under 33 s with lock wait | yes | 0 |
| `client --lab --max-frames 30` | 0 | under 126 s with lock wait | yes | 0 |
| `client --skirmish`, without `MESA_VK_WSI_DEBUG=sw` | 139 (SIGSEGV) | 20.2 s | yes | panic in `create_surfaces`: "No supported formats for surface", after "vulkan: No DRI3 support detected" |

The adapter line in every clean run is `AdapterInfo { name: "AMD Radeon RX 580 Series (RADV POLARIS10)", ...,
driver: "radv", driver_info: "Mesa 25.0.7-2+deb13u1", backend: Vulkan }`.

**`VUID-VkShaderModuleCreateInfo-pCode-08739` is an artefact.** In the orchestrator's first log
(`~/scratch/logs/radv-xvfb-probe.log`) all 18 occurrences come after the `create_surfaces` panic, in a run whose
log has no layer-insertion line. The message says the `Shader` capability is not supported by Vulkan. Every Vulkan
version requires that capability, so the layer was reporting on a device whose state the panic had already torn
down. None of the thirteen validated runs on `radv` that passed reported it. That is the four probes above, plus nine
test children in 3.2 whose gate fails on any unlisted finding: dome, editor, battle, skirmish, swarm, lab,
`terrain_preview`, and the windowed skirmish twice. It gets no allowlist entry. If it ever appears in a run that did not panic, it is a finding.

**Why `sw` is needed.** Xvfb has no DRI3, and `radv`'s X11 presentation requires DRI3 unless
`MESA_VK_WSI_DEBUG=sw` selects its software presentation path. With `sw`, `radv` still renders on the GPU and only
copies finished images to the X server through the CPU.

### 3.2 Which GPU tests pass on `radv` today

Child time is the time the child held the lock. The `lavapipe` column is from section 3.5.

| Test | `radv` result | `radv` child | Lock wait | `lavapipe` child |
|---|---|---|---|---|
| `battle_hud_window_tests` | pass | 19.1 s | 0.01 s | 20.3 s |
| `battle_result_window_tests` (anisotropy 16) | pass | 22.2 s | 0.02 s | 26.6 s (anisotropy 1) |
| `dome_fill_tests` | pass | 26.4 s | 0.01 s | 30.8 s |
| `editor_app_tests` `map_editor --verify-render` (anisotropy 16) | pass | 38.2 s | 0.02 s | 81.0 s (anisotropy 1) |
| `gpu_validation_tests` `real_layer_findings_reach_the_gate` | pass, best-practices findings reach the gate | 16.3 s | 336.7 s | 40.5 s |
| `gpu_validation_tests` `live_battle_passes_vulkan_validation` | pass | 20.5 s | 0.2 s | 72.0 s |
| `gpu_validation_tests` `skirmish_passes_vulkan_validation` (anisotropy 16) | pass | 20.0 s | 207.3 s | 54.0 s (anisotropy 1) |
| `lab_scene_tests` | pass | 17.7 s | 176.3 s | 117.5 s |
| `round2_visual_tests` | pass | 26.6 s | 0.03 s | 38.5 s |
| `skirmish_match_tests` (anisotropy 16) | **intermittent**: failed the first run, passed the second (3.3) | 22.9 s and 39.8 s | 86.4 s and 350.6 s | 50.2 s, pass (anisotropy 1) |
| `smog_bouncing_bubbles_tests` | pass | 19.0 s | 87.4 s | 55.8 s |
| `terrain_tests` `terrain_preview` (anisotropy 16) | pass | 11.4 s | 96.3 s | 47.9 s (anisotropy 1) |

A first `terrain_preview` attempt failed with "Path not found: .../target/debug/assets/shaders/terrain.wgsl". The
probe had run the test binary directly, so `CARGO_MANIFEST_DIR`, which `cargo test` sets, was missing. That says
nothing about `radv`, and the rerun above set the variable.

Eleven of the twelve tests pass on `radv` today with no change except the environment. On `radv`, the five children
that draw hex-tiled terrain ran at the production anisotropy of 16, and all five finished in 11 to 40 s. So `radv`
does not need the Q0009 override.

### 3.3 Driver-specific assumptions in the tests

**`skirmish_match_tests` assumes a slow frame. This is a real defect, and `radv` exposes it.** Its child
`stage_shots` sets the foundation's remaining ticks, unpauses the battle for exactly one frame ("let one tick
publish it"), then pauses again. A simulation tick is 40 ms (`battle_pipeline/clock.rs`). A `lavapipe` frame of the
shipped map takes seconds, so that one frame always contained a tick. A `radv` frame is far shorter than 40 ms, so
whether a tick lands inside that one frame is a race. The first run lost it, and the second shot presented the
first share again: "the shares rose: 0.25390625 then 0.25390625". The second run won it and passed, with crossed
rings changing by 69 to 84 levels against at most 10.1 for rings built in both shots (`CROSSED_FLOOR` 10,
`CROSSED_RATIO` 3). The funding step ("Funded on a running frame, so a tick publishes the bank to the HUD;
stopped on the next") makes the same assumption. The other window children (`windowed_hud_child`,
`windowed_result_child`, `windowed_dome_child`) passed on `radv`, but WP3 audits them for the same pattern.

**Pixel and luminance thresholds.** `dome_fill_tests` (`BUILT_LIMIT` 2.0, `UNBUILT_FLOOR` 10.0),
`lab_scene_tests` (`TOLERANCE` 8 levels) and the terrain luminance bounds passed on `radv` unchanged. Their doc
comments cite values measured on `lavapipe` only. `skirmish_match_tests`' `CROSSED_FLOOR` and `CROSSED_RATIO`
hold on `radv` when the share does rise (69 to 84 crossed, at most 10.1 built), with about 2.3 times headroom on
the ratio. No threshold needs to change, but WP3 records the `radv` measurements next to the `lavapipe` ones.

**Validation allowlist.** `VALIDATION_ALLOWLIST` has one entry, `Unable to find extension:
VK_EXT_swapchain_colorspace`. On this server both drivers now offer that extension (`vulkaninfo` lists it once for
each ICD), so the entry no longer applies here. It stays for older `lavapipe` hosts, and matching is exact. `radv`
needs no entry. `dome_fill_tests`' local copy (`MISSING_COLORSPACE_EXTENSION`) says "every windowed run currently
logs" the warning on `lavapipe`, which is no longer true.

**Not driver-run.** `support/shader_ir.rs`' `STORAGE_BUFFER_BINDINGS` is "reported by lavapipe" but feeds an
offline `naga` composition, not a GPU run. It is out of scope.

### 3.4 Lock behaviour

| Observation | Value |
|---|---|
| Lock waits seen by 18 locked children and probes | 0.01 s to 350.6 s; 9 under 1 s, 1 of 9 s, 7 between 86 s and 351 s, 1 timed out at 120 s |
| `with-gpu-lock` with `GPU_LOCK_TIMEOUT=120` while another process held the lock | exit 1 after 120.0 s, no output |
| Holders during the long waits | `quorra`: `flock --timeout 1200 /run/lock/gpu.lock cargo test --release ... --test launch_path`, which is one lock around a whole `cargo test`, compilation included. Another `spacescuffle` session: `with-gpu-lock xvfb-run ... battle_capture` |

Three facts follow:

- **The owner's "a few seconds" does not hold today.** A peer that runs a whole `cargo test` under one lock holds
  it for minutes. The harness must therefore wait for minutes without failing, and it must start its run deadline
  only after it has the lock.
- **`flock(1)`'s timeout exit status is 1**, the same as an ordinary child failure. A harness that wrapped the child
  in `with-gpu-lock` could not tell "never got the GPU" from "the client failed", without a marker hack.
- **A lock wait inside GNU `timeout` would eat the run budget.** In the probes the lock was taken outside the
  `timeout`, which is why 337 s of waiting did not kill the 300 s run.

### 3.5 `lavapipe` per-child times

Measured right after the `radv` runs with the unchanged harness: one parent test at a time, no lock, the wall time
of the parent test (which is the child plus well under a second of parent work). The load average was 40 to 45, against
about 30 during the `radv` runs. The figures are therefore an upper bound, but the ratio is clear.

| | Sum of the twelve children | Slowest child |
|---|---|---|
| `lavapipe` (terrain children at anisotropy 1) | 635 s | 117.5 s (`client --swarm`) |
| `radv` (terrain children at anisotropy 16) | 277 s, or 260 s with the faster skirmish run | 39.8 s (windowed skirmish) |

On `radv`, each child spends most of its time in CPU work that does not depend on the driver: a debug build loading
assets, composing shaders and building pipelines. So the speed-up is about 2.3 times overall, not the orders of
magnitude a GPU suggests. The short window children barely change (`battle_hud_window_tests`: 19.1 s against 20.3 s). What matters more is that `radv` renders the
production sampler, and the GPU work no longer competes for the 16 cores.

## 4. Decisions

### D1. Driver selection

```text
SPACE_SCUFFLE_GPU_TEST_DRIVER = radv | lavapipe | auto (unset means auto)
```

- **`auto`** chooses `radv` when all three host prerequisites hold, and `lavapipe` otherwise:
  1. a `radv` manifest exists: `/usr/share/vulkan/icd.d/radeon_icd.json` or `radeon_icd.x86_64.json`;
  2. a DRM render node `/dev/dri/renderD*` exists and this user can open it read-write (the first one in name
     order is used for the check);
  3. `with-gpu-lock` is found on `PATH`. Its presence is how a host declares that it runs the shared lock protocol.
     The harness does not execute it (D2).
- **`radv`** forces the physical GPU. If any prerequisite is missing, the selection panics and names every missing
  one. It never falls back.
- **`lavapipe`** forces software Vulkan with no lock, exactly as CI runs. If no `lavapipe` manifest exists, the
  selection panics.
- Any other value panics and names the accepted values.
- Selection runs once per test process (`OnceLock`) and prints one line to stderr, such as
  `gpu_run: driver radv (auto: radeon_icd.json, /dev/dri/renderD128, with-gpu-lock on PATH)`.
- Selection is a pure function, `select_driver(request: DriverRequest, host: &HostGpu) ->
  Result<GpuDriver, DriverSelectionError>`, over a `HostGpu` value that the probe of the real host fills in. Unit
  tests cover the whole matrix without touching the host.
- `.github/workflows/ci.yml` sets `SPACE_SCUFFLE_GPU_TEST_DRIVER: lavapipe` at the workflow level. CI does not
  depend on `auto` guessing correctly. A future runner with a GPU then fails loudly or stays on `lavapipe`, and
  never changes driver silently.

`SPACE_SCUFFLE_SKIP_GPU_TESTS=1` keeps its meaning and its CI refusal.

### D2. The lock is taken in Rust, per child, around the spawn

`GpuLock::acquire(path, wait) -> Result<(GpuLock, Duration), GpuLockError>` opens `/run/lock/gpu.lock` (read and
write, created with mode 0666 if absent, as `flock(1)` would). It calls `File::try_lock` every 250 ms until it
succeeds or `wait` expires. The guard holds the `File`. Dropping it, which closes the descriptor, releases the
lock. `GpuCommand::run` acquires the guard immediately before `Command::output()` and drops it immediately after.
Nothing else in the harness holds it.

Why this and not wrapping the child in `with-gpu-lock`:

| | Rust guard around the spawn | `with-gpu-lock` as the outermost process |
|---|---|---|
| Lock timeout distinguishable from a child failure | yes, a typed `GpuLockError::TimedOut` | no, both exit 1 (3.4) |
| Lock wait measured and reported | yes | only by a shell marker |
| Run deadline starts after acquisition | yes | yes, if `timeout` is inside |
| Testable on a private lock file, without touching the shared one | yes, the path is a parameter | no, the script hardcodes the path |
| Lock survives the test process being `SIGKILL`ed mid-run | no: the child runs on unlocked for at most the run limit | yes |
| Literal match with the charter's "wrap each command in `with-gpu-lock`" | no; same lock file, same `flock(2)` | yes |

The first four rows decide it. The charter text changes with this plan (section 8) to say that the harness takes
the same lock itself. To keep the duplicated path honest, a host-contract test reads `with-gpu-lock` from `PATH`
when it exists. It asserts that the script contains `flock` and the harness's `GPU_LOCK_PATH`, so a change to the
host script fails the gate instead of splitting the protocol.

**Release between children and fairness.** Peers wait in the kernel (`flock(1)` blocks in `flock(2)`). When the
harness closes its descriptor, the kernel wakes those waiters at once. The harness's next child only polls every
250 ms, so a blocked peer normally wins the next slot. That is the short slot between tasks the owner asked for.
The harness needs no extra courtesy delay, and it never holds the lock across two children: the guard's scope is
one `output()` call.

**Self-deadlock guard.** `with-gpu-lock cargo test` would make every child wait on a lock that its own ancestor
holds, until the wait limit expires. On the first `WouldBlock`, the harness reads `/proc/locks`, finds the holder's
PID for the lock file's inode, and walks its own ancestors through `/proc/<pid>/stat`. If the holder is an ancestor,
it panics at once: "the GPU lock is held by an ancestor process (PID n: `cmdline`); run `cargo test` without
`with-gpu-lock`, because the harness locks each GPU child itself". The parsing is a pure function with unit tests
on fixture text, and one integration test proves it on a real `flock(1)` parent (section 6).

**What `lavapipe` does.** No lock, no `/proc` reads, no waiting, exactly as today.

### D3. `GpuCommand` replaces the plain `Command`

`virtual_gpu_command` and `virtual_gpu_command_on_screen` return a `GpuCommand`, which does not expose
`Command::output`, `spawn` or `status`. The only ways to run it are `run`, `run_validated` and
`assert_passes_validation`, and all three go through the lock. A caller cannot bypass the lock without building
its own `Command` from scratch, which would visibly drop the harness.

```rust
/// The Vulkan driver a GPU test child runs on.
pub enum GpuDriver { Radv, Lavapipe }

/// A GPU child under construction: binary, display, driver environment, run limit.
pub struct GpuCommand { command: Command, driver: GpuDriver }

impl GpuCommand {
    pub fn new(binary: &Path) -> Self;
    pub fn on_screen(binary: &Path, screen: &str) -> Self;
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self;
    pub fn args<I, S>(&mut self, args: I) -> &mut Self where I: IntoIterator<Item = S>, S: AsRef<OsStr>;
    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self;
    pub fn driver(&self) -> GpuDriver;
    /// Lowers terrain anisotropy only for `lavapipe` before Mesa 25.1 (D7).
    pub fn allow_for_hex_tiling_driver_defect(&mut self) -> &mut Self;
    /// Takes the GPU lock (radv), runs the child, releases the lock, checks the adapter (D6).
    pub fn run(self, label: &str) -> GpuRun;
    pub fn run_validated(self, label: &str) -> (GpuRun, ValidationReport);
    pub fn assert_passes_validation(self, label: &str) -> GpuRun;
}

/// What one child did, with the evidence of where it ran.
pub struct GpuRun {
    pub output: Output,
    pub driver: GpuDriver,
    pub adapter: Option<AdapterEvidence>,
    /// `None` on lavapipe, which takes no lock.
    pub lock_wait: Option<Duration>,
    pub run_time: Duration,
}
```

The lock a `GpuCommand` takes is a field, `lock: LockPolicy`, with the variants `None` (`lavapipe`),
`Shared` (`radv`, `GPU_LOCK_PATH`) and `Private(PathBuf)`. Only R2 to R4 use `with_private_lock(path)`, which sets
`Private` on any driver. The lock tests therefore run on CI too, and never touch the shared lock file.

The existing free functions (`run_validated`, `assert_passes_validation`, `allow_for_hex_tiling_driver_defect`)
become these methods, and all twelve call sites in 2.2 move to them. The names are a sketch. The implementer may
adjust them but must keep the property that no caller can run a GPU child without the lock.

### D4. Timeouts

| Limit | `radv` | `lavapipe` | Where |
|---|---|---|---|
| Lock wait | 1200 s, or `GPU_LOCK_TIMEOUT` if set | none | `GpuLock::acquire`, before the spawn |
| Run (GNU `timeout`) | 120 s | 300 s (unchanged) | inside the spawned command, so it starts after acquisition |
| Kill grace | 10 s | 10 s | `timeout --kill-after=10` |

- The lock wait does not count against the run limit, because the spawn happens only after acquisition (3.4).
- Its default is 1200 s, not `with-gpu-lock`'s 600 s. The charter allows a single long command, such as a
  frame-budget measurement, to hold the lock for up to 1200 s, and 351 s waits were already seen. A peer's
  legitimate use must not turn the gate red. `GPU_LOCK_TIMEOUT` has the same meaning as for `with-gpu-lock`, so
  one variable controls both.
- **Superseded by the owner (Q0012, 2026-11-03):** the default is now 290 s (4 min 50 s). Every GPU user locks per
  device use, so a wait should last seconds; a single long command that needs more sets `GPU_LOCK_TIMEOUT`.
- On expiry, the test fails with `GpuLockError::TimedOut { path, waited }`. The message names the waited time, the
  holder's PID and command line from `/proc/locks` when readable, and says that `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe`
  runs the tests without the GPU. It never falls back by itself.
- On `radv`, the run limit is 120 s, about three times the slowest child measured (39.8 s, the windowed skirmish). A hung
  child then holds the GPU for at most 130 s, not 310 s. `lavapipe` keeps 300 s, which its measured children need.
- The worst case for one child on `radv` is 1200 + 120 + 10 s. CI is unaffected: it never waits for a lock, and its
  `timeout-minutes: 60` comment stays true.
- A timeout failure names the driver and the limit that applied: "did not finish within 120 s on radv".

### D5. The `radv` child environment

| Variable | `radv` | `lavapipe` (unchanged) |
|---|---|---|
| `VK_ICD_FILENAMES` | the `radeon_icd` manifest found in D1 | the `lvp_icd` manifest |
| `WGPU_ADAPTER_NAME` | `radv` (wgpu matches it case-insensitively against "... (RADV POLARIS10)") | `llvmpipe` |
| `MESA_VK_WSI_DEBUG` | `sw` (3.1: Xvfb has no DRI3) | removed |
| `LIBGL_ALWAYS_SOFTWARE` | removed | `1` |
| `WGPU_BACKEND`, `WINIT_UNIX_BACKEND`, `SPACE_SCUFFLE_VIRTUAL_GPU`, `WAYLAND_DISPLAY` removal | unchanged | unchanged |

`xvfb-run`, the display numbering and the screen option are unchanged. The test still never appears on a
desktop. `SPACE_SCUFFLE_VIRTUAL_GPU=1` now means "a test display", not "a software GPU". Its single reader
(`client/src/lib.rs`, window visibility) already means that, so it keeps its name. The module docs say so.

### D6. Evidence: every run records the adapter it rendered on

- `VALIDATED_RUST_LOG` becomes `warn,wgpu_hal=info,bevy_render::renderer=info`, so the child logs Bevy's
  `AdapterInfo` line. If the caller set its own `RUST_LOG` (`smog_bouncing_bubbles_tests` does), `run` appends
  `,bevy_render::renderer=info`. If it is unset, Bevy's default level already logs the line.
- `AdapterEvidence { name, driver, driver_info }` is parsed from that line by a pure function with unit tests on
  the two real lines in section 3 and on the `lavapipe` line.
- `GpuRun` and `ValidationReport` carry it. `ValidationReport::violations` takes the expected driver and adds a
  violation when the line is missing or names another driver (`radv` must say `driver: "radv"`, `lavapipe` must say
  `driver: "llvmpipe"`). `run` applies the same check to unvalidated runs. A test cannot pass on the wrong GPU.
- Every failure message starts with the driver and the adapter name. For example: "client --battle on radv (AMD
  Radeon RX 580 Series (RADV POLARIS10), Mesa 25.0.7) exited with 1".
- **Ledger.** If `SPACE_SCUFFLE_GPU_RUN_LEDGER` names a file, `run` appends one line per child: label, driver,
  adapter, `driver_info`, lock wait, run time, exit status. Opened with `O_APPEND`, each line is one `write`. The
  gate (section 7) uses it to show that every child ran where it should, and how long each held the GPU.

### D7. Q0009 on `radv`

- `allow_for_hex_tiling_driver_defect` depends on the selected driver. On `radv` it does nothing. On `lavapipe` it
  keeps today's rule (Mesa older than 25.1). The decision is a pure function,
  `anisotropy_override(driver, lavapipe_release) -> Option<&'static str>`, and `vulkaninfo` is asked only when the
  driver is `lavapipe`.
- Effect on this server: hex-tiled ground under anisotropy 16 is rendered again by five tests (2.2), on the
  physical GPU. That is Q0009's option 3, now the default because of the owner's GPU instruction.
- The override and `SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY` stay in `client::terrain::theme_gpu`. Their only
  remaining consumer is a forced `lavapipe` run on a host with Mesa older than 25.1, which means this server's
  CI-parity runs. Deleting them is the owner's call under Q0009 (section 10).
- WP4 adds an "Update 2026-11-03" paragraph to Q0009 that states this and changes its default line to "keep the
  override; on this server GPU tests run on `radv`, which never applies it".

### D8. Allowlist and validation

- No new allowlist entry. 08739 is explicitly not allowlisted (3.1).
- The colorspace entry stays. Its reason gains one sentence: Mesa 25.0.7 offers the extension on both drivers, so
  the entry applies only to older `lavapipe`.
- `dome_fill_tests`' comment on `MISSING_COLORSPACE_EXTENSION` becomes "logged by `lavapipe` before Mesa 25"
  rather than "every windowed run".

### D9. Tests that assume a slow frame

The skirmish child must stop counting frames as ticks. Each place where it unpauses "for one tick" records
`session.published()`'s `tick` before unpausing. It keeps the clock running until the published tick is greater,
and only then pauses and starts `SETTLE_FRAMES`. This covers both the share step and the funding step. The step
moves into one function or system that the windowed child and the headless test R9 share, so the test exercises
the code the child runs. `battle_harness` already fixes frames at 40 ms with `TimeUpdateStrategy::ManualDuration`,
and R9 overrides that to 5 ms. A bounded
frame budget (the existing `CHILD_FRAME_LIMIT`) still fails a child that never ticks. The module doc's sentence "A
lavapipe frame of the shipped map takes seconds" becomes driver-neutral. WP3 audits `windowed_hud_child`,
`windowed_result_child` and `windowed_dome_child` for any frame-equals-tick or frame-equals-time assumption and
fixes each one the same way.

## 5. Work packages

One commit per package, in this order. Each package keeps `cargo test --workspace` green on this server and on
`SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe`.

### WP1. Harness core (`gpu_run.rs`)

1. `GpuDriver`, `DriverRequest`, `HostGpu`, `select_driver`, `DriverSelectionError` (D1), plus the once-per-process
   `selected_driver()`.
2. `GPU_LOCK_PATH`, `GpuLock`, `GpuLockError { Open, TimedOut, HeldByAncestor, Io }` with `Display`, the polling
   acquire, and the `/proc/locks` and ancestor parsing as pure functions (D2).
3. `GpuCommand`, `GpuRun`, `AdapterEvidence`, the per-driver environment (D5), the per-driver run limit (D4), the
   adapter check (D6), the ledger (D6) and `anisotropy_override` (D7).
4. Module docs (`//!`): responsibility now includes driver selection and the lock. "How validation reaches this
   module" gains the adapter line. "Limits of the gate" gains two items. On `radv`, presentation uses
   `MESA_VK_WSI_DEBUG=sw`, so the DRI3 present path is not exercised. If the test process is killed with `SIGKILL`,
   its child can run for up to the run limit without the lock. Every constant documents its unit (seconds) and its
   source (this plan's section 3).
5. Unit and integration tests for all of this (section 6, R1 to R6 and R8).

### WP2. Call sites

Move the twelve tests in 2.2 to `GpuCommand`. The three that call `command.output()` now call `run`.
`dome_fill_tests` keeps its own allowlist through `run_validated` and `report.violations(..)`. `gpu_validation_tests`
extends its unit tests to the new pure functions.

### WP3. Frame-time assumptions (D9) and `radv` measurements

Fix `skirmish_match_tests` and audit the other three window children. Record the `radv` values next to the
`lavapipe` ones in the doc comments of `BUILT_LIMIT`, `UNBUILT_FLOOR`, `CROSSED_FLOOR`, `CROSSED_RATIO` and the lab
`TOLERANCE`, measured from a gate run on each driver. Only the comments change, unless a value fails on one driver;
in that case the fix is the test's, not the threshold's, and it gets its own note in the commit.

### WP4. Documentation and CI

- `.github/workflows/ci.yml`: `SPACE_SCUFFLE_GPU_TEST_DRIVER: lavapipe` in the workflow `env`. The comment above
  `timeout-minutes` keeps "300 s limit" and adds "on lavapipe".
- AGENTS.md section 7: replace the bullet named in section 8. Do not touch section 6.
- `docs/questions/closed/0009-terrain-anisotropy-test-override.md`: the update paragraph of D7.
- `terrain_tests.rs` module doc ("executes under virtual GPU (xvfb + lavapipe)") and the other doc comments that
  say `lavapipe` where they now mean "the test GPU".
- `client::terrain::theme_gpu`'s doc of the override: it is applied only on `lavapipe` before Mesa 25.1, never on
  `radv`.
- This plan's row in `docs/plans/README.md`: `Active` when WP1 starts, then `Done` with the commits.

## 6. Tests, and the red runs that must come first

Each test below is written first and run against the unchanged harness (or the state before its package), where it
must fail for the stated reason. Each red run's log goes to `~/scratch/logs/gpu-harness-red-<id>.log`, and the
commit message quotes the failing assertion. Tests that touch a lock use a private lock file in a `tempfile`
directory. **No test ever locks the shared `/run/lock/gpu.lock` except a real GPU child.**

| ID | Test (in `gpu_validation_tests.rs` unless noted) | What it proves | Red before the change because |
|---|---|---|---|
| R1 | `driver_selection_matrix` | `auto` picks `radv` only with all three prerequisites; forced `radv` errors and names each missing one; forced `lavapipe` needs its manifest; invalid values are rejected | `select_driver` does not exist (written first against a stub that always returns `Lavapipe`, so the `radv` rows fail) |
| R2 | `a_child_holds_the_gpu_lock_only_while_it_runs` | a child `sh -c 'flock -n "$LOCK" true; echo $? > out'` run through `GpuCommand` sees the lock held (`1`); after `run` returns, `flock -n` on the same file succeeds. This is real `flock(1)` interop on a private lock | `run` takes no lock: the child writes `0` |
| R3 | `a_busy_lock_times_out_without_starting_the_child` | with the private lock held by a second descriptor and a 1 s wait, `run` fails with `GpuLockError::TimedOut` after at least 1 s, and the child's marker file does not exist | the child starts at once |
| R4 | `an_ancestor_holding_the_lock_fails_fast` | the test re-executes itself under `flock <private lock> <current_exe> --ignored --exact ancestor_lock_child`. The child's `GpuLock::acquire` returns `HeldByAncestor` in under 2 s although its wait is 60 s. Pure-function tests cover `/proc/locks` lines and `stat` parent fields | no ancestor detection: the child waits 60 s and times out |
| R5 | `radv_never_lowers_terrain_anisotropy` | `anisotropy_override(Radv, Some((25, 0)))` is `None`; `(Lavapipe, Some((25, 0)))` is `Some("1")`; `(Lavapipe, Some((25, 1)))` is `None` | on this server today, the override is set whenever `lavapipe` 25.0.7 is installed, whatever the driver |
| R6 | `adapter_lines_are_parsed_and_checked` | the `radv` and `llvmpipe` `AdapterInfo` lines from section 3 parse; a missing line and a mismatched driver are violations | the report has no adapter field |
| R7 | `the_selected_driver_is_the_one_that_rendered` (real child, `client --lab --max-frames 10`) | on this server with `auto`, the adapter is `radv`; with `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` it is `llvmpipe` | before WP1's environment change, `auto` cannot exist and the child renders on `llvmpipe` (written against the stub of R1, it fails with "expected radv, rendered on llvmpipe") |
| R8 | `the_host_lock_tool_uses_the_harness_lock_file` | if `with-gpu-lock` is on `PATH`, it contains `flock` and `GPU_LOCK_PATH` | the constant does not exist |
| R9 | `a_staged_share_is_published_however_short_the_frame` (`skirmish_match_tests`, headless, new) | the child's staging step, driven through `battle_harness` with `TimeUpdateStrategy::ManualDuration` of 5 ms (an eighth of a tick), still publishes each staged share and the funded bank before it pauses | with today's one-frame unpause, a 5 ms frame runs no tick, so the presented share stays at the first value. This is the deterministic form of the intermittent `radv` failure in 3.3, which cannot serve as a red run because it passed once in two tries |

R2 to R4 use `flock(1)` from util-linux, which the server and `ubuntu-latest` both have. They run on every host,
including CI, because they lock only a private file. R7 runs only when its driver is available. On CI it checks
`lavapipe` (`SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` is set there), so it is never skipped silently.

## 7. Gate

Run in the implementation worktree with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree>`.
`cargo test` is never wrapped in `with-gpu-lock`, and R4 makes that mistake fail loudly.

1. `cargo fmt --all -- --check`.
2. `cargo clippy --workspace --all-targets --locked -- -D warnings`.
3. `SPACE_SCUFFLE_GPU_RUN_LEDGER=~/scratch/logs/gpu-ledger-radv-<commit>.txt cargo test --workspace --locked`, with the
   driver unset, so `auto` selects `radv`. Pass criteria: green; the ledger has one `radv` line naming the RX 580 for each of
   the twelve children in 2.2 and for R7 (R2 to R4 write `private lock` lines, which do not count); no run time over 120 s; lock waits and run times copied into the commit message
   of WP4.
4. `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe SPACE_SCUFFLE_GPU_RUN_LEDGER=~/scratch/logs/gpu-ledger-lvp-<commit>.txt
   cargo test -p client --locked`. Pass criteria: green; every ledger line says `llvmpipe` and has no lock wait;
   the five terrain children log the Q0009 warning (Mesa 25.0.7).
5. `SPACE_SCUFFLE_GPU_TEST_DRIVER=radv`, with a `PATH` that does not contain `with-gpu-lock`, run on one GPU test:
   it fails at once and names the missing prerequisite. This is a manual check of D1's no-fallback rule, recorded
   in the WP1 commit message.
6. `cargo test -p client --locked --test skirmish_match_tests` three more times on `radv`, all green. The
   race of 3.3 lost one run in two before WP3.
7. The CI run on the pushed branch is green, and its log shows `gpu_run: driver lavapipe (forced)`.
8. `docs/questions/open/` is checked before the push (charter section 8).

## 8. AGENTS.md section 7 text

Once WP4 lands, replace the bullet that begins "**The test harness still pins GPU tests to `lavapipe`.**" with:

```markdown
- **GPU tests lock the GPU themselves, one test child at a time.** `crates/client/tests/support/gpu_run.rs` runs
  each GPU test child on the physical GPU (`radv`, with `MESA_VK_WSI_DEBUG=sw` under `xvfb-run`). It takes
  `/run/lock/gpu.lock`, the lock `with-gpu-lock` uses, just before the child starts, and releases it when the
  child exits, so other users get the GPU between children. Run `cargo test --workspace` as it is and never wrap it
  in `with-gpu-lock`; the harness detects that and fails. A child waits up to `GPU_LOCK_TIMEOUT` (default 1200 s)
  for the lock and then has 120 s to finish. Where the radeon driver, a usable render node or `with-gpu-lock` is
  missing, as on CI, the harness uses `lavapipe` with no lock. `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` reproduces
  CI on this server, and `=radv` fails instead of falling back.
```

The bullet "**Software Vulkan is the fallback, not the default.**" stays as it is.

## 9. Risks

- **A peer holds the lock for longer than 1200 s.** The gate fails with a message that names the holder. That is
  correct (it fails loudly), but it is annoying. Section 10 asks the owner about the peer's suite-wide lock.
- **`radv` and `lavapipe` disagree on a pixel test in future.** Both drivers stay in the gate (section 7, step 4),
  so a driver-specific result shows up before CI and not after.
- **GPU hang or reset.** The run limit kills the child, and the guard releases the lock when `output()` returns. A
  device lost through the kernel is outside this harness. The test fails, and the child's log is in the failure
  tail.
- **The test process is killed with `SIGKILL`.** Its child keeps running without the lock for at most 130 s. This
  is documented (WP1, step 4) and not engineered around.
- **`File::try_lock` is used on a platform without `flock(2)`.** The `radv` path is Linux-only by construction (D1
  needs `/dev/dri`). The lock code is behind `cfg(target_os = "linux")`, and the selection reports `radv`
  unavailable elsewhere.

## 10. Decisions for the owner

1. **Q0009.** Keep the anisotropy override for forced-`lavapipe` parity runs on this server (default), or delete it
   now that the server's GPU tests run on `radv`. Deleting it means that a forced-`lavapipe` run of the five terrain
   children here times out, until the server's Mesa is 25.1 or later. Recommendation: keep it until the Mesa
   upgrade, then delete it.
2. **The peer's suite-wide lock.** During the probes, `quorra` held `/run/lock/gpu.lock` around a whole
   `cargo test --release` (`flock --timeout 1200 ... cargo test`), and this plan's children waited up to 351 s. Owner
   action outside this repository: ask the other agent to lock per GPU process, as this plan does. This plan works
   either way; it only waits longer.
3. **Wait limit.** The default lock wait is 1200 s (D4), to tolerate a peer's legitimate long command. If the owner
   prefers the gate to fail sooner, `GPU_LOCK_TIMEOUT` overrides it per run, and the default is one constant.

These are recorded as `docs/questions/open/` entries by WP4 if the owner has not answered in chat (1 updates
Q0009; 2 and 3 get one new question).

## 11. Implementation record: deviations and the in-process scope addition

Added during implementation, 2026-11-03.

### 11.1 Deviations from the sketch in sections 4 to 6

- **WP1 and WP2 are one commit.** WP1 changes the harness API that every WP2 call site uses, so neither builds alone.
- **Lock self-tests use `GpuCommand::lock_probe`.** D3 sketched `with_private_lock(path)` on a renderer command. A
  renderer command needs a selected driver and `xvfb-run`, and its adapter check would fail on `sh`. `lock_probe`
  runs a plain program under `timeout` with a private lock and no adapter check, so R2 to R4 run on every host.
  `GpuRun::driver` is therefore an `Option` (`None` for a lock probe), and the ledger writes `driver=none
  lock=private lock` for those lines.
- **`ValidationReport::violations` keeps its signature.** The report carries `expected_driver`, which
  `run_validated` sets; with it set, `violations` adds the adapter check (D6). The pure parser tests are unchanged.
- **`run` checks the exit status itself.** Every caller expected success, so `run` fails on a timeout, an
  unsuccessful exit and a wrong or missing adapter line, each message starting with label, driver and adapter.
  `try_run` returns a lock failure as `GpuLockError` (R3).
- **No `cfg(target_os = "linux")` around the lock.** `File::try_lock` is portable in std; only the `/proc`
  readers and Unix file modes are behind `cfg(unix)`. `radv` is still Linux-only, because D1 needs `/dev/dri`.
- **Auto with neither driver fails.** `auto` on a host with no `radv` prerequisites and no `lavapipe` manifest
  is a `DriverSelectionError::NoDriver`, not a run on whatever Vulkan offers.

### 11.2 Scope addition: tests that build a render device in their own process

Found by Phase 9 WP8: three tests built a Bevy renderer with real backends inside the test process, without
`gpu_run.rs`:

| Test | File |
|---|---|
| `production_plugin_schedule_initializes_terrain_and_restores_tab_camera` | `crates/client/tests/runtime_battle_tests.rs` |
| `actual_renderer_new_queue_id_is_pending_without_indexing_unpublished_storage` | `crates/client_diagnostics/src/shell_order_audit/gpu_tests.rs` |
| `actual_loaded_scene_validates_temporary_and_synchronized_proxy_lifecycles` | same file |

A search of every test file and `#[cfg(test)]` item under `crates/` for `DefaultPlugins`,
`GamePresentationPlugins`, `RenderPlugin`, `WgpuSettings`, `build_graphical_app*`, `Instance::new` and
`request_adapter` found no others. The battle harness, `shader_ir.rs` and the editor harness use
`WgpuSettings { backends: None }`, which creates no device. `runtime_capture_tests`' `build_graphical_app` call
uses `ClientMode::HeadlessCheck`, which the client refuses before any renderer starts. The `runtime_capture`
ownership tests use `MinimalPlugins` in this tree.

**What those tests did on this server.** With no `WGPU_BACKEND`, Bevy asks every wgpu backend. Run in-process
with `VK_ICD_FILENAMES` pointing at `lavapipe`, the client test chose `AMD Radeon RX 580 Series (radeonsi, ...)`
on the **OpenGL** backend, through EGL, with no lock, and failed with a flood of `wgpu_hal::gles` errors ("view
dimension will be equal to `CubeArray` rather than `D2Array`"), which is the `Some(Error(1))` WP8 saw. The same
test with `WGPU_BACKEND=vulkan` passes on `lavapipe` in 13 s (`~/scratch/logs/gpu-harness-rbt-lvp.log`). So the
failure is not `lavapipe`-specific: it is wgpu's GL backend on the RX 580, reached because the in-process test had
no driver environment. With no variables at all, the loader also offers `radv`, and the test passed in-process
in this plan's first `radv` gate, presumably on the discrete Vulkan adapter, but without the lock.

**Decision (not covered by D1 to D9).** Such a test must not build the device in the test process: the Vulkan
loader reads `VK_ICD_FILENAMES` and wgpu reads `WGPU_BACKEND` and `WGPU_ADAPTER_NAME` only from the process
environment, and setting them from a multi-threaded test process is unsound. Each test is split the way the window
tests already are: its body becomes an `#[ignore]`d child, and the `#[test]` calls `gpu_run::run_test_child`,
which re-executes the test binary through a `GpuCommand`. The child therefore gets driver selection, the driver
environment (including `WGPU_BACKEND=vulkan`), the lock only for its own run, the run limit, the adapter check and
a ledger line. `client_diagnostics` includes the client's `gpu_run.rs` by `#[path]`, so there is one harness.

**Check C21** (`crates/workspace_checks/src/gpu.rs`, run by `tests/charter.rs`) fails the gate on a new unpinned
in-process GPU test. In test code it flags `DefaultPlugins`, `GamePresentationPlugins`, `build_graphical_app`,
`build_graphical_app_off_main_thread`, `build_graphical_app_with_options` and `request_adapter`, unless
`backends: None` follows within 15 lines, `HeadlessCheck` follows within 3 lines, or the file uses `gpu_run` and
the marker is inside an `#[ignore]`d test (a launched child) or a helper that is not a test. Its limit: it does
not follow calls, so a helper is trusted to be called only by launched children. Red run against the three files
as they were: "C21: 3 finding(s)", one per test above (`~/scratch/logs/gpu-harness-red-C21.log`). The ID C21 is
used because the Phase 9 blueprint reserves C20.
