# Round 4 production capture verification

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: capture acquisition, repeatability and negative verification complete; source frozen for shared gates. Visual product quality is **not accepted** by this report.

## Implemented contract

`battle_capture` uses `build_graphical_app` and the actual production `ClientAppPlugin`, external scenario/catalog/assets, camera, tab lifecycle, reset, shaders and GPU readback. Normal startup defaults remain unchanged; the ordinary graphical entry point now returns `AppExit` so diagnosed renderer failures are observable after orderly shutdown. Capture controls set existing production state and explicitly do not verify OS keyboard/mouse adapters.

The harness records real entity/checksum state, every sampled ramp position, correlated hostile source/victim, camera poses and projections, scene/light/effect counts, exact shader-handle readiness counts, image metrics, and callback completion. It freezes simulation and world visual time while waiting. UI startup uses two configured egui animation durations on a separate deterministic startup clock so tick-zero capture does not preserve an unreadable initial fade. Evidence allocations and checks are capture-tool overhead; results are not game performance benchmarks.

The egui screenshot adapter follows the approved plan34 amendment. The existing egui renderer writes into the actual camera output attachment used by Bevy screenshots. It does not change the acquired native surface texture, UI geometry, shaders, or ordinary client registration.

## Actual native evidence

All images were generated with Vulkan on the real AMD Radeon 890M/RADV device and opened for visual inspection. No synthetic scene or alternate shader was used.

- Run01: exited 1 because the selected center overview did not show intended units; rejected before screenshot. The final script frames a regional starting base rather than claiming a full-map overview.
- Run02: seven logical checkpoints completed, but all screenshots lacked UI. Rejected; inspection identified the bevy_egui window-attachment mismatch.
- Run03: seven UI-inclusive images at 1280×720. Rejected initial UI fade and recorded genuine default-client defects: the Visual Lab panel extends below the viewport, hides the selected first hero, and overlaps portrait/HUD content. Preserved artifacts in `/tmp/space-scuffle-r4-runtime-captures-run03`; widening later evidence does not fix these product defects.
- Run04: seven UI-inclusive images at the explicitly configured 1920×900 viewport. Directory `/tmp/space-scuffle-r4-runtime-captures-run04`; log `/tmp/space-scuffle-r4-capture-run04.log`. Renderer teardown reports zero pending workers and no GPU validation error.

| Image | Actual evidence | Visual assessment |
| --- | --- | --- |
| 01-overview | Tick 0; 26 entities; actual starting army/base/resources and cliff boundary | Army/base visible outside UI; regional composition, not whole map. Strong tile repetition, sharp billboard-resource shadows and oversized environmental empty space remain product-art issues. |
| 02-ramp-entry | Tick 162; tracked entity 13 entered authored ramp from level 2 | Same actor visible at slope, nearby upper/lower surfaces and following units readable. |
| 03-ramp-exit | Tick 195; same entity 13 exits at level 1 after uninterrupted adjacent samples | Actor visibly beyond descending slope; no later unrelated elevation change accepted. |
| 04-combat | Tick 614; source/victim matched by hostile owner, actual acquired target, exact snapshot impact/origin and real HP/shield decrease | Both sides and actual weapon streak are visible outside UI. Team readability and small combat silhouettes remain weak. |
| 05-visual-lab | Battle checksum/tick retained; hero camera restored; battle root hidden; actual lab objects present | All four main subjects visible at wide viewport; portrait/HUD still overlaps inspector. Shell/media treatment remains artistically rejected by the independent optics review. |
| 06-battle-return | Same saved battle camera, tick 614 and checksum restored | Same combat region restored; movement wake is cleared by existing tab lifecycle rather than incorrectly claimed to be persistent state. |
| 07-reset | Tick 0, initial checksum/full entity state/roster restored; one battle root; original light count; no stale effects | Same regional overview and army arrangement restored without accumulated entities/lights. UI width persists from lab; camera/world composition matches. |

The screenshots are reliable evidence of existing product behavior, not high-quality game art approval. Stale squad descriptors, default viewport UI occlusion, opaque/glossy creature media and weak team readability remain separate tracked work. No editor changes were made.

## Focused verification

- Five capture unit tests pass: hostile contact correlation rejects unrelated/friendly events, ramp continuity/landing rejection, real-wall/frame deadline failures with no virtual clock, stale callback token rejection, and gameplay-region blank-image detection.
- Five capture integration tests pass: shipped script/order, invalid controls/budgets, strict unknown-field rejection, typed malformed/missing script paths, and headless builder rejection.
- `cargo clippy -p client --all-targets -- -D warnings`: passed; `/tmp/space-scuffle-r4-capture-clippy-final.log`. The patched third-party Naga dependency still prints its upstream warnings; client targets pass strict linting.
- `cargo fmt --all -- --check`: passed; `/tmp/space-scuffle-r4-capture-fmt.log`.
- Shared workspace acceptance belongs to the orchestrator's frozen-source gate. This report does not substitute targeted checks for that result.

## Completed repeat and failure verification

Run05 completed all seven checkpoints with exactly the same logical manifest as Run04 after excluding image metrics/output directory. Ticks, full checksums, tracked source/victim, camera poses/projections, every ramp sample, scene counts and pipeline evidence match. Four PNGs were byte-identical; three moving/effect views differed slightly and were reopened visually. The contract promises reproducible logical evidence, not universal byte-identical rendering.

The isolated missing-media negative initially **incorrectly exited 0** because the existing product loader aliased another archetype's texture and metadata into the missing unit's registry entry. Preserved rejected evidence: `/tmp/space-scuffle-r4-runtime-captures-missing-media`. The approved exact-media amendment now validates the exact slug's actual PNG and metadata against the registered GPU array and the visible material's actual binding, with typed path-preserving failures. It changes neither art nor production fallback behavior.

- Guarded missing-PNG run: exit **1**, `complete: false`, no accepted checkpoints; error identifies substituted metadata at the exact archetype path. `/tmp/space-scuffle-r4-runtime-captures-missing-media-guarded`, log `/tmp/space-scuffle-r4-negative-missing-media-guarded.log`.
- Unreachable ramp-entry condition with a one-tick budget: exit **1** after the overview; incomplete manifest records unmet condition. `/tmp/space-scuffle-r4-runtime-captures-unreachable`, log `/tmp/space-scuffle-r4-negative-unreachable.log`.
- Existing output directory and unavailable `/proc` output destination: both exit **1** with exact I/O paths, never reuse prior screenshots. Logs `/tmp/space-scuffle-r4-negative-existing-output.log` and `/tmp/space-scuffle-r4-negative-unwritable-output.log`.
- Valid guarded Run06: exit **0**, all seven checkpoints, all logical fields identical to Run05, and **all seven PNG files byte-identical** to the already reviewed Run05 images. `/tmp/space-scuffle-r4-runtime-captures-run06`, log `/tmp/space-scuffle-r4-capture-run06.log`. Comparison record `/tmp/space-scuffle-r4-capture-reproducibility.log`.

Eight capture unit tests now pass, including exact real media/array matching, missing PNG, malformed/false-identity metadata, changed image bytes/dimensions, and material-binding rejection after cache population. `/tmp/space-scuffle-r4-capture-exact-media-tests02.log`. Strict client all-target Clippy passes `/tmp/space-scuffle-r4-capture-exact-media-clippy02.log`. An intermediate full fmt invocation was blocked by missing `third_party/naga/tests/root.rs` declared by the dependency manifest; after the dependency owner repaired that declaration, full fmt passes again (`/tmp/space-scuffle-r4-startup-contract-fmt.log`). Shared full gates remain separately reported by the orchestrator.

Capture acquisition/behavior verification is complete. Product visual quality remains rejected for the issues above; the wider capture viewport is evidence tooling, not a repair of default-client layout.


The shared-builder change also required updating the real invalid-asset-root subprocess test from the old diagnostic prefix to the formatted typed `ClientStartupError::Assets` boundary. It still requires nonzero exit, the exact absent path/cause and no DISPLAY/WAYLAND. The subsequent preflight passed all remaining requested client integration suites: runtime_assets 10, runtime_battle 9, runtime_capture 5, simulation_diagnostics 1, terrain 13 including four actual software-rendered screenshots and clean shutdown. `/tmp/space-scuffle-r4-capture-remaining-client-preflight.log`; targeted strict Clippy and full fmt both pass afterward. This is preflight evidence, not a substitute for the final complete workspace rerun.

A further concrete team-readability defect is recorded for separate planning: authoritative snapshots use `TeamId(owner.0)` for default players 0/1, while the bubble palette maps 1 to blue and 2 to red, leaving player 0 gray. The UI labels those players Blue/Red. Separate indicator/VFX mappings also need an audited common palette contract; this capture/media work does not edit them.
