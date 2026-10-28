# Round 1 implementation and verification — serialization and runtime assets

## Exact scalar persistence (plan 23)

Implemented `q32.32:v1:<signed raw i64>` string output for human-readable Serde and raw `i64` output for binary formats. Existing integer/decimal catalog input remains supported through checked conversions. Non-finite and overflowing numbers, malformed/unknown tagged strings, and empty/overlong scalar tuples return deserialization errors. New writes require the updated reader; old numeric RON remains readable.

Evidence:

- `cargo test -p sim_types --offline`: passed, including full-width seeded 10,000-value RON/bincode round-trips, extrema and power-of-two boundaries, nested player command round-trip, compatibility inputs, and direct numeric visitor unit tests. Log: `/tmp/space-scuffle-r1-serialization-targeted.log`.
- `cargo test -p sim_core --test catalog_serialization_compatibility --offline`: passed, reading the actual checked-in units and structures catalogs. Log: `/tmp/space-scuffle-r1-serialization-catalog.log`.
- Added cached `bincode 1.3.3` as a test-only dependency to verify a real non-self-describing binary codec rather than a mock serializer.

## Checked runtime asset root (plan 26)

Implemented typed fallible root discovery, strict explicit overrides, marker-file validation, canonical absolute paths, and path-context I/O errors. Normal graphical startup now fails before GPU initialization when assets are invalid. Bevy and manual bubble texture imports receive the same `ResolvedAssetsRoot`, so a nearby stale `assets` directory cannot redirect one loader. Non-Unicode filesystem paths remain intact in discovery and produce an explicit error at Bevy's string-only API boundary.

Evidence:

- `cargo test -p client --test runtime_assets_tests --offline`: all 10 tests passed, including the real binary failing with an invalid override and no display, real repository shader-path reading, stale-nearby-directory regression, symlink canonicalization, override precedence, invalid marker types, and non-Unicode handling. Log: `/tmp/space-scuffle-r1-assets-targeted.log`.
- The old infallible resolver remains for untouched editor/direct embedding consumers. Direct plugin embedding without the new resource retains legacy behavior. This is explicitly partial migration of F-14, not a claim that every legacy consumer is fixed.
- No editor files, shader code, image packing, or visual appearance were changed. The bubble shader module change only adds an optional resource parameter and routes its filesystem path through the shared-root helper.

## Shared quality gates

`cargo clippy --workspace --all-targets -- -D warnings` passed after fixing the owned warnings. Log: `/tmp/space-scuffle-r1-serialization-clippy.log`.

The workspace test gate was attempted but encountered concurrently edited ramp/checksum integration test compilation failures. Formatting checks likewise observed in-progress changes in other tracks. Final workspace test and formatting acceptance must be rerun after the orchestrator's edit freeze; these rounds are not reported as fully gate-complete yet. Logs: `/tmp/space-scuffle-r1-serialization-workspace-test.log` and `/tmp/space-scuffle-r1-serialization-fmt.log`.
