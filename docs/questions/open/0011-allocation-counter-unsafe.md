# Q0011: How may a test count heap allocations when `unsafe_code` is forbidden?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** Phase 9 WP5 lands its function splits without `sync_allocation_tests.rs`. The charter's
  §3.3 claim that battle sync systems allocate nothing in steady state stays without a CI check. WP13 lists it as
  "Review" instead of "CI" until this is answered.

## Question

Charter §3.3 says per-frame client synchronization never allocates. The Phase 9 blueprint
(`docs/plans/2026-11-03-phase-9-structure-and-docs.md`, WP5 step 5) proves it with a test that installs a counting
`#[global_allocator]`. The allocator counts only while a flag is set around `sync_live_battle_system`, and the test
requires zero allocations over 64 unchanged frames.

That test cannot be written as planned. The workspace sets `unsafe_code = "forbid"` (charter §1.1), and implementing
`std::alloc::GlobalAlloc` needs `unsafe impl`. A `forbid` cannot be relaxed in a test file. No crate in `Cargo.lock`
provides a counting allocator.

The existing evidence is weaker. `battle_hud_tests.rs` and `battle_result_tests.rs` check that retained buffers keep
their address and capacity. That misses a temporary allocation freed in the same frame. The blueprint's red run, a
`Vec::with_capacity(1)` planted in `update_live_instance`, would pass those checks unnoticed.

The options:

1. **A dev-only workspace crate with one audited `unsafe impl` (recommended).** For example `crates/alloc_probe`,
   `publish = false`, a dev-dependency of `client` only, so it is never linked into a shipped binary. It holds one
   stateless allocator that forwards to `std::alloc::System` and calls a counting hook that the test defines. The
   crate would use `unsafe_code = "deny"` with one `#[expect(unsafe_code, reason = ...)]`, and every other crate keeps
   `forbid`. The charter §1.1 text would name the exception, and §5 would draw the crate.
2. **A crates.io counting allocator as a dev-dependency**, such as `stats_alloc`. Our code would need no `unsafe`,
   so `forbid` holds everywhere. It adds a third-party download. It also counts every thread, so the test must run
   alone in its binary to avoid noise from other tests.
3. **Keep the weaker evidence.** The §3.3 claim is narrowed to "retained buffers are reused", which CI already
   checks.

## Answer

Owner (2026-11-03): Valgrind sounds good (keep the `valgrind --tool=dhat` test in `crates/client/tests/sync_allocation_count_tests.rs` and tag the charter §3.3 claim as CI-checked).

## Note from an agent

2026-11-03, exits WP3 of `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`. This note does not answer the question
and does not change its default.

A fourth way to count was built that needs no `unsafe` and no new dependency:
`crates/client/tests/sync_allocation_count_tests.rs`. Its parent test re-runs its own test binary under
`valgrind --tool=dhat`, which replaces `malloc` from outside the process and records each heap block with the stack
that allocated it. No code of ours replaces the allocator, so `unsafe_code = "forbid"` holds everywhere. CI installs
`valgrind` from Ubuntu's archive.

What it measures: the production live synchroniser (`sync_live_battle_system` with the battle driver) on the real
default battle, paused, with `MinimalPlugins` and the single-threaded executor. It counts only the blocks allocated
inside the measured frames (by call stack), at 64 and at 128 frames, and asserts that the client's systems add
nothing per frame beyond a baseline app without them. A planted control (one `Vec` per frame) must count exactly one
allocation per frame. Measured on 2026-11-03: 0 blocks in the window for the live sync and for the baseline, 64 and
128 for the control. Planting `Vec::with_capacity(1)` in `update_live_instance` fails the test with 1,664 blocks
over 64 frames (one per bubble per frame).

What it does not measure: the whole client frame (renderer, egui, the simulation tick), and Bevy's multi-threaded
executor. That executor allocates about 18 blocks a frame for its own task spawning (`bevy_tasks`,
`async_executor`), and its count varies with thread timing. Whole-process totals also vary by about 2 blocks between
identical runs, because Bevy builds schedule graphs with `std`'s random `RandomState`; that is why the test counts by
call stack instead of using memcheck's `total heap usage`.

The owner can still choose option 1 or 2. Either would replace how the test counts, not what it asserts.
