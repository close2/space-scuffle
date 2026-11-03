# Q0013: Where should the debug-info compression flag live, and should the client tests be merged into one binary?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** The flag `-Wl,--compress-debug-sections=zlib` lives in the repository's
  `.cargo/config.toml`, so local builds and CI build the same binaries. The client integration tests stay as 33
  separate binaries; no consolidation is planned.

## Question

CI run 41607070459 failed because the runner's disk filled while the `client` test binaries linked: `target/`
reaches 20.4 GiB cold, and `rust-lld` died with SIGBUS. The plan `docs/plans/2026-11-03-ci-disk-budget.md` asks the
linker to zlib-compress the DWARF sections, which brings `target/` to about 9 to 10 GiB, and adds disk guards to
`ci.yml`. Two parts of it are yours to decide.

1. **Repository config or CI only?**
   - **Repository `.cargo/config.toml` (default, recommended).** CI and the local gate (charter §4) build the same
     binaries, and local `target/` directories shrink too (the main checkout's is 59 GiB). The cost is one full
     rebuild in every existing target directory and worktree, once.
   - **CI only,** through `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS` on the CI job. Local builds stay as they
     are, but CI then builds different binaries from the local gate.
2. **Consolidate the client integration tests later (plan option F)?** Merging the 33 files under
   `crates/client/tests/` into one test binary would cut 32 Bevy links, save about 11 GiB more on top of
   compression, and shorten every gate run. It is a sizeable refactor: the GPU child processes that rerun their own
   binary, the `CARGO_BIN_EXE_*` users and the per-file `support/` includes all need rework. Options: yes, plan it
   now; yes, but only if the disk guard trips again; or no.

## Answer

