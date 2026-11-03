# Round 2 catalog and normal-startup verification

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Reviewed 2026-10-28 in a read-only verification round against plans 28 and 29. No implementation changes were made during this review.

## Verified

- `RuntimeCatalogs::load_from_directory` reads the selected units, structures and upgrades files, preserves the failing path in typed errors, and has no embedded fallback.
- Every float DTO field feeding fixed-point conversion is covered by finite/range validation, including nested mode weapons, splash, optional projectile speed and overclock fields. Existing `SimScalar` ability values use the checked deserializer. This establishes safe decoding, not complete balance or arithmetic-domain validation.
- Authored duplicate IDs/slugs are rejected; staged structure additions leave the original catalog unchanged on a late failure. Sparse upgrade IDs use ordered sparse storage.
- The ASCII-folded sorted slug index preserves the old lookup equivalence and minimum-storage-ID tie behavior. `get_mut` and `register` invalidate it, clones retain coherent state, and lookup addresses storage slots rather than mutable identity fields. Loaded indexes are prepared before return.
- Normal startup follows `run_app_with_scenario` → `BattleDefinition::load` → external `RuntimeCatalogs`. It inserts that loaded `CatalogResource` before plugin initialization, so `init_resource` does not replace it with the embedded default.
- `BattleDefinition::instantiate` calls `build_world_with_catalogs`; reset uses the same loaded catalog snapshot and scenario. Restart reloads files; an in-session reset intentionally does not hot-reload definitions. Legacy embedded/headless fixture constructors remain separate.
- `cargo test -p sim_core --test runtime_catalog_tests`: 8 passed, including same-binary external edits and actual injected spawn values. Log: `/tmp/space-scuffle-r2-catalog-verification-tests.log`.
- `cargo test -p client --test runtime_battle_tests`: 8 passed, including actual combat, deterministic reset, terrain identity and scene lifecycle. Log: `/tmp/space-scuffle-r2-catalog-verification-client.log`.

## Concrete findings

1. **Camera tab restoration was implemented but unscheduled; corrected during review.** `runtime_battle::sync_scene_camera` now runs before scene management and camera controls. The expanded lifecycle test verifies lab framing and restored battle target/distance and passed independently. The new full production-plugin startup test exposed a different blocker: real mist style parsing fails at 11:17 with `Expected opening (` and emits `AppExit`. The rerun has 8 passes and 1 failure; log `/tmp/space-scuffle-r2-catalog-verification-camera.log`. Reported to the root/runtime owner during the concurrent mist migration. Full startup acceptance remains pending its correction and rerun.
2. **Pre-existing production timer narrowing remains visible through external authored edits.** `UnitArchetype.build_time_ticks` is `u32`, but `SimWorld::process_commands` casts it to `u16` for `Entity.production_queue`. An independent temporary Rust fixture loads the actual unit catalog with a duration of 65,536 ticks and reports `loaded external build_time_ticks=65536`, then `queued production ticks=Some(0)`. Evidence: `/tmp/space-scuffle-catalog-duration-review.rs`. This is a simulation timer representation defect, not a failure of the new safe-decoding checks. Reported for a separate planned widening of production timers, checksum schema update and timing-boundary tests; it is not silently treated as correct.

## Acceptance limits

No new catalog parser/index defect was found in this pass. The normal startup and reset injection paths use the requested external rules. Catalog compatibility fingerprints remain excluded from the dynamic digest, as documented in plan 24. Full workspace gates and screenshot acceptance are coordinated by the root agent; the focused tests above are not substitutes for those gates or evidence of visual quality. Full production startup needs the reported mist parsing correction and a passing rerun, plus visual acceptance.
