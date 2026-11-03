# Q0012: Should the other GPU user lock per process, and how long should a GPU test wait for the lock?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** GPU test children wait up to 1200 s for `/run/lock/gpu.lock` (overridable per run with
  `GPU_LOCK_TIMEOUT`), and this repository does nothing about the other user's lock pattern.

## Question

Since `docs/plans/2026-11-03-gpu-test-harness.md`, `cargo test --workspace` runs every client GPU test child on the
RX 580. Each child takes the shared GPU lock just before it starts and releases it when it exits, so a child holds
the GPU for 10 to 40 s and other users get it between children.

Two things are yours to decide:

1. **The other user's suite-wide lock.** While the plan was measured, the other agent user (`quorra`) held the lock
   around a whole `cargo test --release` (`flock --timeout 1200 /run/lock/gpu.lock cargo test ...`), compilation
   included, and this project's children waited up to 351 s for it. This project's harness works either way; it only
   waits longer. Fixing it is outside this repository: you could ask the other agent to lock each GPU process instead
   of its whole test run, as this project now does. Recommendation: ask it to.
2. **How long a child waits.** The default is 1200 s, so that a peer's legitimate long command (the charter allows
   one to hold the lock for up to 1200 s, for example a frame-budget measurement) does not turn this project's gate
   red. A child that waits longer fails loudly, names the holder, and suggests `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe`;
   it never falls back by itself. If you prefer the gate to fail sooner, the default is one constant
   (`DEFAULT_LOCK_WAIT_SECS` in `crates/client/tests/support/gpu_run.rs`), and `GPU_LOCK_TIMEOUT` overrides it per run.
   Recommendation: keep 1200 s.

## Answer
Owner, in chat on 2026-11-03 (copied by the orchestrating agent): the other process will fix its locking. It now uses
a no-op hook in its device code that its tests and gates fill with the lock, so it locks per GPU use instead of
around a whole test run. 

Part 2, owner in chat on 2026-11-03: wait 4 minutes 50 seconds. Once the peer's locking is fixed a wait should be a
few seconds; waiting up to 290 s does no harm.

## Resolution

Part 1 is fixed outside this repository by the other user, as the owner described. Part 2: `DEFAULT_LOCK_WAIT_SECS` in
`crates/client/tests/support/gpu_run.rs` is now 290 s, and AGENTS.md §7 says so. `GPU_LOCK_TIMEOUT` still overrides it
per run.
