# Guide: GPU and Disk on the Shared Development Server

This guide holds the operating detail of charter §7 (`AGENTS.md`). The charter states the rules; this guide says
how to follow them, and binds like the charter. It was moved out of `AGENTS.md` word for word (Q0084, Q0085).

## The machine

Development runs on a shared VM. Its physical **AMD Radeon RX 580** (Ellesmere, `/dev/dri/renderD128`, driven by
`radv`) is shared with another autonomous agent user, and the machine also has software Vulkan (`lavapipe` /
`llvmpipe`).

## Using the GPU

- **Use the physical GPU.** It is the default for every GPU task: rendering, visual checks, validation runs and GPU
  tests. The other users need it only about 1% of the time, so do not avoid it.
- **Hold the GPU lock per command, never for hours.** Wrap each command that runs on the physical GPU in
  `with-gpu-lock`, and release it between tasks so the other users get a short slot. Lock one test process or one
  capture at a time, not a whole test suite or session. A wait of a few seconds for the lock is normal. Unlocked use
  risks VRAM contention, driver resets, and skewed timings. This includes running `client` or any other binary with
  no driver selected, because Vulkan may then pick the physical GPU.
  ```bash
  WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json with-gpu-lock <command>
  ```
- **The lock's timeout is 600 seconds by default.** `with-gpu-lock` takes an exclusive `flock` on
  `/run/lock/gpu.lock` and releases it when the command exits. It no longer passes the lock to the command it runs,
  so a process the command leaves behind cannot hold it, and it logs every acquisition, timeout and release to
  `/run/lock/gpu-lock.log` (Q0060). Raise it with `GPU_LOCK_TIMEOUT=1200` only for a single long command, such as a
  frame-budget measurement.
- **Software Vulkan is the fallback, not the default.** Use `lavapipe` only where no physical GPU exists (CI) or to
  reproduce a CI result exactly. It needs no lock:
  ```bash
  WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json <command>
  ```
  `WGPU_BACKEND=vulkan` is required in both commands: `VK_ICD_FILENAMES` only filters Vulkan drivers, and without it
  wgpu may fall back to its OpenGL backend, which runs on the physical GPU through `radeonsi` with no lock. The ICD
  files are `radeon_icd.json` and `lvp_icd.json`; the `*.x86_64.json` names some distributions use do not exist on
  this server.
- **GPU tests lock the GPU themselves, one test child at a time.** `crates/client/tests/support/gpu_run.rs` runs
  each GPU test child on the physical GPU (`radv`, with `MESA_VK_WSI_DEBUG=sw` under `xvfb-run`). It takes
  `/run/lock/gpu.lock`, the lock `with-gpu-lock` uses, just before the child starts, and releases it when the child
  exits, so other users get the GPU between children. Run `cargo test --workspace` as it is and never wrap it in
  `with-gpu-lock`; the harness detects that and fails. A child waits up to `GPU_LOCK_TIMEOUT` (default 290 s) for
  the lock and then has 120 s to finish. Where the radeon driver, a usable render node or `with-gpu-lock` is missing,
  as on CI, the harness uses `lavapipe` with no lock. `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` reproduces CI on this
  server, and `=radv` fails instead of falling back.

## Memory and concurrent test runs

- **Run one `cargo gate` or workspace `cargo test` at a time across all worktrees.** `cargo gate` serializes itself
  through the gate lock (`~/scratch/gate.lock`, `crates/gate/src/gate_lock.rs`); wrap any other workspace
  `cargo test` in `flock ~/scratch/gate.lock`. Before other heavy runs, check the `anon` line of
  `/sys/fs/cgroup/user.slice/user-1002.slice/memory.stat` and keep it below about 56 GiB of the 66 GiB limit
  (`memory.current` also counts reclaimable page cache).

## Disk

- **Hot build output goes on the home disk; large files go on `~/scratch`.** The home disk (NVMe, with a per-user
  quota; check it with `quota -s`) holds the repository, active worktrees and hot build output. The main checkout's
  own `target/` stays in the checkout, and every agent worktree builds into a separate target directory,
  `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`. Large logs, captures, rendered sheets, corpora
  and gate stamps belong under `~/scratch`, a slow spinning disk for cold, large, sequential files. Remove a worktree
  and its target directory once its branch is merged.
