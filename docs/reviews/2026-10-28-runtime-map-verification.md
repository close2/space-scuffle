# Runtime map integration verification — 2026-10-28

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Blueprint: `docs/plans/28-round2-runtime-maps-and-matching-terrain.md`.

## Implementation boundary

Normal graphical startup loads `assets/scenarios/default_battle.ron`, its selected versioned map, external unit/upgrade/start-loadout catalogs, the map-selected terrain theme, and authored map-object images. The simulation and terrain consume the same decoded map. Reset reconstructs that loaded scenario with the same seed and catalogs. Combat formations and initial attack orders are declarative; Rust chooses order recipients by movement, weapon, and harvesting capabilities.

The runtime owns a scene parent containing terrain chunks, backdrop, lighting, resource billboards, and watchtowers. Reset replaces that hierarchy; tab switches hide it and restore independent camera presets. Runtime terrain overlays are zero. Editor code and map assets have not been modified by this implementation.

The existing `--headless-check` fixture still exercises the older deterministic sandbox. It is not evidence for new-map startup. New integration tests exercise the real loader and production `ClientAppPlugin` schedule separately.

## Verified behavior

- Both shipped versioned map documents instantiate through the runtime scenario loader.
- The default loaded battle reaches real combat events within a 2,400-tick bound.
- Initial state, repeated stepping, and reset are deterministic; initial attack destinations are legal map-relative approach cells rather than occupied town halls.
- Missing scenario paths, invalid versions, duplicate players, missing destination starts, and worker-only attack orders produce explicit errors.
- The real runtime scene manager creates matching simulation/render terrain, disables overlays, replaces roots and light on reset, and switches visibility without duplicating scene entities.
- A full production-plugin schedule test initializes the Vulkan renderer with actual assets and a hidden Window, then checks terrain creation and both tab camera transitions. It additionally rejects startup application-exit events, so asset errors cannot masquerade as a passing lifecycle test.

Latest focused runtime suite: all nine tests pass in `/tmp/space-scuffle-r2-runtime-scheduler-accepted.log`, including the stricter startup-exit assertion after the mist owner repaired its real catalog. This hidden-window harness checks scheduling, not native presentation. Parent native startup captures still showed black surfaces and remain visually rejected pending diagnosis.

Earlier evidence before final shared gates: `/tmp/space-scuffle-r2-runtime-targeted.log`, `/tmp/space-scuffle-r2-runtime-scheduler-final.log`, and `/tmp/space-scuffle-r2-runtime-build-fixed.log`. The scheduler log predates its stricter startup-exit assertion and records the independently owned mist catalog parse error. That earlier logged startup is rejected; the later focused rerun is the accepted lifecycle evidence.

## Actual launch findings and corrections

The first parent-captured production launch exposed three issues that the isolated scene helper test could not establish:

1. `TerrainPlugin` attempted to access `TerrainState` before deferred runtime scene creation. Terrain updates now run only when the actual state resource exists; no dummy map is inserted.
2. Camera preset preservation existed but was not registered. The production plugin now schedules it before scene creation and camera control, and tests verify restoration.
3. Pipeline inspection classified asynchronous shader imports as terminal failures. Shader-not-loaded/import-not-yet-available, queued, and creating states now remain pending. Actual compilation errors still fail immediately. Frame-limited verification requires nonzero compiled pipelines with none pending, and unresolved readiness has a 30-second bound.

The mist catalog parse failure belongs to the concurrent mist implementation. Its owner repaired it, and the strict production schedule regression subsequently passed.

## Terrain texture correction and visual acceptance

The first baseline screenshot showed fractured deck patterns caused by arbitrary rotations of an already seamless structured texture. The runtime now derives `TerrainParams.regular_tiling_mask` from the theme's declared `SourceSeamless` modes. For `orbital_foundry`, the exact mask is `0b0010` (decimal 2): only deck layer 1 uses aligned sampling. Grass and dirt keep their prior stochastic sampling, and cliffs keep triplanar sampling. Tests verify this selection and the unchanged 64-byte GPU uniform layout.

This is a material-sampling correction, not proof of visual quality. The corrected production binary was built, but final gameplay screenshots, shader compilation under the real camera, object silhouette/readability, terrain exposure, and overall composition still require parent visual evaluation. The initial failed capture is rejected. This round remains visually unaccepted until those actual captures pass.

## Required final gates

A coordinated frozen workspace must pass `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`. Targeted checks do not substitute for those gates. Root coordinates them after concurrent mist and catalog work stabilizes.

## Native capture review

Root's longer cold-start capture reached actual rendering after approximately 32 seconds: `/tmp/space-scuffle-round2-battle-long-startup/battle-1.png` and `battle-2.png`. I independently inspected the first image. The real map is present, deck seams remain coherent, and debug overlays are absent. The composition is rejected: a large portion of the frame is empty backdrop; the base-focused camera misses the advancing armies; the dome has a broad white-clipped rim; authored resource sprites read as faint green fragments; and the backdrop appears coarse and blurred. The UI still describes obsolete fixed squads rather than the loaded declarative roster. These require a dedicated visual/UI blueprint, not a claim of polished completion. The capture log still reports both baseline shader compiler VUIDs; root reports no presentation synchronization errors in this particular run. Plan31 keeps native presentation reproduction separate from compiler fixes.
