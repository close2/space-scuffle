# Plan Status Index

Every implementation plan in this directory, with what became of it. Check C18 in
`crates/workspace_checks` keeps this table complete: each plan file has exactly one row, and the build fails
otherwise.

**Rules for new plans**

- Name the file `YYYY-MM-DD-slug.md` (the date the plan was written, then a lowercase hyphenated slug). The
  numbered series (`01` to `55`) is closed; no new numbered plan may be added.
- Add the plan's row to this table in the same commit that adds the plan, and update its row in the commit that
  changes its status.
- The ID of a numbered plan is its number; the ID of a dated plan is its file name without `.md`.
- Status is one of `Done`, `Partly done`, `Superseded`, `Rejected`, `Active`, `Proposed`. Evidence names a
  commit, the superseding plan or a decision of the remediation plan, and says what is missing when a plan is
  `Partly done`.
- Independent reviews are not plans. They live in [`docs/reviews/`](../reviews/); the five that once shared a
  plan number there carry a `planNN-` prefix.

**How the statuses were assigned (2026-11-03).** In this order: the remediation plan
([`2026-11-01-codebase-remediation.md`](2026-11-01-codebase-remediation.md)), whose decision D2 retired the
runtime mist (deleted in `0f31027`) and D3 the mist reducer, and whose §6 lists the rounds left out of scope; then
`git log` on the code each plan names; then the plan's own status line and its verification review. Where these
disagree, the row says `Partly done` and names what is missing. Commits before 2026-11-01 are coarse: rounds 1 to
11 landed together on 2026-10-28 (`ecc574d` to `443052c`), so their rows cite the round's verification review.

D2 = remediation decision D2 (keep `smog/`, delete runtime `mist/`). D3 = decision D3 (delete reducer research).
D6 = decision D6 (procedural flipbooks are test fixtures only). D7 = decision D7 (media scope, still open as
[Q0001](../questions/open/0001-media-scope.md)).

| ID | File | Title | Status | Evidence |
|---|---|---|---|---|
| 01 | [01-plan-sim-types.md](01-plan-sim-types.md) | `sim_types` and the Cargo workspace | Done | `3d9a9c4`; reworked by remediation Phase 2 (`71c4df0`) |
| 02 | [02-plan-sim-map.md](02-plan-sim-map.md) | `sim_map`: map, cliffs, navigation and high-ground vision | Partly done | `3d9a9c4`; the CDT NavMesh was never built, navigation is a grid with A* (remediation Q-9) |
| 03 | [03-plan-sim-core.md](03-plan-sim-core.md) | Deterministic simulation core | Done | `3d9a9c4`; combat and movement rebuilt by remediation Phases 3 and 4 (`56437da`, `94ca5c7`) |
| 04 | [04-plan-asset-baker-and-client.md](04-plan-asset-baker-and-client.md) | Asset baker and Bevy client (Visual Lab) | Done | `3d9a9c4` |
| 05 | [05-plan-round2-ai-assets-and-visual-polish.md](05-plan-round2-ai-assets-and-visual-polish.md) | Round 2: AI flipbooks, three-faction roster, portrait HUD | Partly done | `3d9a9c4`; real media exists for 4 of 91 identities (D7, Q0001) |
| 06 | [06-plan-round3-veo31-video-generation.md](06-plan-round3-veo31-video-generation.md) | Round 3: Veo 3.1 unit video generation and flipbook packing | Partly done | Pipeline in `3d9a9c4`, hardened by remediation Phase 8 (`b342c77`, `4c425c8`); media for the required set is not generated (D7, Q0001) |
| 07 | [07-plan-ecs-query-conflict-and-gpu-startup-test.md](07-plan-ecs-query-conflict-and-gpu-startup-test.md) | Bevy ECS query conflicts and GPU startup test | Done | `3d9a9c4` |
| 08 | [08-plan-data-driven-unit-catalog.md](08-plan-data-driven-unit-catalog.md) | Data-driven unit catalog | Done | `3d9a9c4`; faction and class identity follow in Phase 9 WP12 (F-13) |
| 09 | [09-plan-full-sc2-roster-catalog.md](09-plan-full-sc2-roster-catalog.md) | Full SC2 5.0.14 roster catalog and procedural atlas baking | Partly done | Roster in `3d9a9c4`; procedural atlases fenced off from runtime paths by D6 (`cf784cd`) |
| 10 | [10-plan-sc2-simulation-mechanics-and-tests.md](10-plan-sc2-simulation-mechanics-and-tests.md) | SC2 parity mechanics and headless tests | Done | `3d9a9c4`; corrected by remediation Phase 3 (`56437da`, `3052128`) |
| 11 | [11-plan-structures-and-upgrades-catalog.md](11-plan-structures-and-upgrades-catalog.md) | Structures, tech tree and upgrades catalog | Done | Catalog in `3d9a9c4`; research and construction wired by remediation Phase 5 (`3a2865e`) |
| 12 | [12-plan-bubble-shader-optics-and-volumetric-mist.md](12-plan-bubble-shader-optics-and-volumetric-mist.md) | Bubble shader optics, direction stripes and volumetric mist | Partly done | Bubble optics in `3d9a9c4`; the mist was retired by D2 (`0f31027`) |
| 13 | [13-plan-fix-selection-ring-depth-and-volumetric-mist-trail.md](13-plan-fix-selection-ring-depth-and-volumetric-mist-trail.md) | Selection ring depth and mist wake | Partly done | Selection ring in `3d9a9c4`; the mist wake was retired by D2 (`0f31027`) |
| 14 | [14-plan-fix-orphaned-melee-slash-discs-and-battle-vfx.md](14-plan-fix-orphaned-melee-slash-discs-and-battle-vfx.md) | Melee slash discs, unit altitude and combat VFX | Done | `3d9a9c4` |
| 15 | [15-plan-fix-live-battle-errant-unit-pathfinding.md](15-plan-fix-live-battle-errant-unit-pathfinding.md) | Errant unit pathfinding in the live battle | Done | `3d9a9c4`; movement later redesigned by remediation Phase 4 (`94ca5c7`) |
| 16 | [16-plan-combat-retaliation-projectiles-and-domain-warped-mist.md](16-plan-combat-retaliation-projectiles-and-domain-warped-mist.md) | Retaliation, projectiles, melee concave and domain-warped mist | Partly done | Combat in `3d9a9c4`, rebuilt by remediation Phase 3 (`56437da`); the mist was retired by D2 (`0f31027`) |
| 17 | [17-plan-realistic-sc2-battle-pacing-and-colorful-glowing-mist.md](17-plan-realistic-sc2-battle-pacing-and-colorful-glowing-mist.md) | Battle pacing, melee pauses and glowing mist | Partly done | Pacing in `3d9a9c4`; the mist was retired by D2 (`0f31027`) |
| 18 | [18-plan-sweating-suction-mist-and-volumetric-combat-vfx-shaders.md](18-plan-sweating-suction-mist-and-volumetric-combat-vfx-shaders.md) | Suction mist wake, combat VFX shaders, anti-vibration | Partly done | VFX in `3d9a9c4`; the mist was retired by D2 (`0f31027`); anti-vibration superseded by remediation Phase 4 (`94ca5c7`) |
| 19 | [19-plan-perfectly-round-bubble-optics-and-real-volumetric-mist.md](19-plan-perfectly-round-bubble-optics-and-real-volumetric-mist.md) | Round bubble optics and curl-warped volumetric mist | Partly done | Bubble optics in `c84545c`; the mist was retired by D2 (`0f31027`) |
| 20 | [20-plan-screen-space-circular-bubbles-and-stronger-mist.md](20-plan-screen-space-circular-bubbles-and-stronger-mist.md) | Screen-space circular bubbles and denser mist | Partly done | Bubble silhouette in `507843f`; the mist was retired by D2 (`0f31027`) |
| 21a | [21a-plan-comprehensive-sc2-who-wins-matchup-benchmarks.md](21a-plan-comprehensive-sc2-who-wins-matchup-benchmarks.md) | Who-wins matchup matrix and swarm benchmarks | Done | Matrix in `9f4a80c`; moved to data on several seeds by remediation Phase 3 (`1349c67`) |
| 21b | [21b-plan-map-editor.md](21b-plan-map-editor.md) | Map editor, declarative map format and terrain renderer | Done | `ec5b92e`; hardened by remediation Phase 7 (`7decbd7`) |
| 22 | [22-plan-sc2-helpless-unit-flee-response.md](22-plan-sc2-helpless-unit-flee-response.md) | Helpless unit flee response | Done | `9f4a80c` (`flee_and_evasion_tests.rs`) |
| 23 | [23-round1-exact-fixed-point-serialization.md](23-round1-exact-fixed-point-serialization.md) | Round 1: exact fixed-point serialization | Done | `7c9ab4a`; [review](../reviews/2026-10-28-r1-serialization-assets.md) |
| 24 | [24-round1-authoritative-checksum.md](24-round1-authoritative-checksum.md) | Round 1: canonical dynamic checksums | Done | `9f4a80c`; [review](../reviews/2026-10-28-r1-serialization-assets.md) |
| 25 | [25-round1-legal-ramp-traversal.md](25-round1-legal-ramp-traversal.md) | Round 1: legal ramp traversal | Done | `60a63be`, `9f4a80c`; [review](../reviews/2026-10-28-r1-serialization-assets.md) |
| 26 | [26-round1-runtime-asset-validation.md](26-round1-runtime-asset-validation.md) | Round 1: fail on an invalid runtime asset root | Done | `8253d42`; [review](../reviews/2026-10-28-r1-serialization-assets.md) |
| 27 | [27-round2-production-mist.md](27-round2-production-mist.md) | Round 2: production directional mist | Superseded | D2: runtime mist deleted (`0f31027`); the art was rejected ([review](../reviews/2026-10-28-round2-mist-progress.md)) |
| 28 | [28-round2-runtime-maps-and-matching-terrain.md](28-round2-runtime-maps-and-matching-terrain.md) | Round 2: playable versioned maps and matching terrain | Done | `60a63be`, `3d97144`; [review](../reviews/2026-10-28-runtime-map-verification.md) |
| 29 | [29-round2-runtime-catalog-injection.md](29-round2-runtime-catalog-injection.md) | Round 2: external catalogs injected into the world | Done | `253f070`; [review](../reviews/round2-catalog-verification.md) |
| 30 | [30-round3-production-timer-width.md](30-round3-production-timer-width.md) | Round 3: lossless production durations | Done | `9f4a80c`; [round log](../reviews/continuous-rounds.md) |
| 31 | [31-round3-renderer-validation-compatibility.md](31-round3-renderer-validation-compatibility.md) | Round 3: validation-clean Vulkan rendering | Done | `ecc574d`; [review](../reviews/2026-10-28-renderer-validation.md) |
| 32 | [32-round3-weapon-period-arithmetic.md](32-round3-weapon-period-arithmetic.md) | Round 3: weapon period modifier arithmetic | Done | `9f4a80c`; [review](../reviews/round3-weapon-period-validation.md) |
| 33 | [33-round3-bubble-optical-hierarchy.md](33-round3-bubble-optical-hierarchy.md) | Round 3: bubble optical hierarchy and output colour | Done | `8253d42`; [review](../reviews/2026-10-28-round3-bubble-optics-progress.md) |
| 34 | [34-round4-reproducible-runtime-captures.md](34-round4-reproducible-runtime-captures.md) | Round 4: reproducible captures of the game | Done | `b165de5`, moved to `client_diagnostics` (`5654fda`); [review](../reviews/round4-runtime-capture-verification.md) |
| 35 | [35-round4-persistent-mist-gpu-storage.md](35-round4-persistent-mist-gpu-storage.md) | Round 4: persistent mist GPU storage | Superseded | D2: runtime mist deleted (`0f31027`) after `c315b05` |
| 36 | [36-round4-native-input-and-scenario-ui.md](36-round4-native-input-and-scenario-ui.md) | Round 4: native input and scenario UI | Done | `5c7d6b9`; [review](../reviews/round6-input-and-ui-verification.md) |
| 37 | [37-round4-non-shadowing-combat-light-effects.md](37-round4-non-shadowing-combat-light-effects.md) | Round 4: non-shadowing combat lights | Done | `bd1888d`; [review](../reviews/2026-10-28-round4-combat-light-shadows.md) |
| 38 | [38-round4-client-command-tick-presentation-pipeline.md](38-round4-client-command-tick-presentation-pipeline.md) | Round 4: one client tick owner and snapshot presentation | Done | `5c7d6b9`; [review](../reviews/round9-client-pipeline-verification.md) |
| 39 | [39-round5-truthful-media-provenance-and-loading.md](39-round5-truthful-media-provenance-and-loading.md) | Round 5: truthful media baking and exact loading | Done | `4b2aad4`, `8253d42`; [review](../reviews/round5-media-integrity-verification.md) |
| 40 | [40-round5-unified-player-presentation.md](40-round5-unified-player-presentation.md) | Round 5: shared player presentation | Done | `8253d42`; [review](../reviews/round10-player-presentation-verification.md) |
| 41 | [41-round5-traversal-predicate-performance.md](41-round5-traversal-predicate-performance.md) | Round 5: traversal predicate performance | Partly done | Stage 1 in `60a63be` ([review](../reviews/round5-combat-shadows-and-path-performance.md)); stage 2 deferred by the plan |
| 42 | [42-round5-explicit-game-audio-policy.md](42-round5-explicit-game-audio-policy.md) | Round 5: explicit game audio policy | Done | `8253d42`; [review](../reviews/2026-10-28-round5-audio-policy.md) |
| 43 | [43-round6-transactional-production-admission.md](43-round6-transactional-production-admission.md) | Round 6: transactional training admission | Done | `9f4a80c`; [review](../reviews/round6-production-transactions.md) |
| 44 | [44-round6-volume-mist-art-direction.md](44-round6-volume-mist-art-direction.md) | Round 6: mist volume and directional flow | Superseded | D2 and D3: runtime mist and reducer deleted (`0f31027`) |
| 45 | [45-round6-authored-movement-layers.md](45-round6-authored-movement-layers.md) | Round 6: authored movement layers | Done | `9f4a80c`; [review](../reviews/round7-movement-and-fixed-bodies.md) |
| 46 | [46-round6-scene-environment-lifecycle.md](46-round6-scene-environment-lifecycle.md) | Round 6: scene environment across tabs | Done | `3d97144`; [review](../reviews/round7-scene-environment-verification.md) |
| 47 | [47-round7-camera-drag-cancellation.md](47-round7-camera-drag-cancellation.md) | Round 7: camera drag cancellation | Done | `5c7d6b9`; [review](../reviews/round8-camera-drag-verification.md) |
| 48 | [48-round7-capability-driven-construction.md](48-round7-capability-driven-construction.md) | Round 7: capability-driven construction | Superseded | Remediation step 5.4 (`3a2865e`) |
| 49 | [49-round7-fixed-body-collision-weights.md](49-round7-fixed-body-collision-weights.md) | Round 7: fixed-body collision weights | Done | `9f4a80c`; [review](../reviews/round7-movement-and-fixed-bodies.md) |
| 50 | [50-round7-checked-movement-arithmetic.md](50-round7-checked-movement-arithmetic.md) | Round 7: checked movement arithmetic | Done | `9f4a80c`; [review](../reviews/round9-checked-movement-arithmetic.md) |
| 51 | [51-round9-transport-ownership.md](51-round9-transport-ownership.md) | Round 9: transport ownership | Done | `9f4a80c`; [review](../reviews/round10-transport-ownership.md) |
| 52 | [52-round10-terrain-aware-cluster-arrival.md](52-round10-terrain-aware-cluster-arrival.md) | Round 10: terrain-connected cluster arrival | Done | `9f4a80c`; [review](../reviews/round11-terrain-connected-arrival.md); arrival redesigned by remediation Phase 4 (`94ca5c7`) |
| 53 | [53-round10-stable-transparent-shell-ties.md](53-round10-stable-transparent-shell-ties.md) | Round 10: exact-depth shell ordering | Partly done | Stage A observation in `6b5906d` ([closeout](../reviews/round11-closeout.md)); stage B is out of scope (remediation §6) |
| 54 | [54-round11-mist-field-representation.md](54-round11-mist-field-representation.md) | Round 11: mist field representation | Superseded | D2 and D3: runtime mist and reducer deleted (`0f31027`) |
| 55 | [55-round11-presentation-discontinuities.md](55-round11-presentation-discontinuities.md) | Round 11: presentation motion discontinuities | Proposed | Never released; out of scope (remediation §6) |
| 2026-10-28-3d-smog-bouncing-bubbles | [2026-10-28-3d-smog-bouncing-bubbles.md](2026-10-28-3d-smog-bouncing-bubbles.md) | 3D smog driven by bouncing bubbles | Done | `ec5b92e`; kept by D2 and reworked by remediation Phase 6 (`52abebb`, `12296f2`) |
| 2026-10-29-unit-stuck-vibration-and-move-arrival-combat | [2026-10-29-unit-stuck-vibration-and-move-arrival-combat.md](2026-10-29-unit-stuck-vibration-and-move-arrival-combat.md) | Unit stuck and vibration, move-arrival combat | Superseded | Remediation Phase 4 (`94ca5c7`), after partial work in `6dd4bea` |
| 2026-10-30-live-battle-low-ground-smog-alignment | [2026-10-30-live-battle-low-ground-smog-alignment.md](2026-10-30-live-battle-low-ground-smog-alignment.md) | Low-ground smog alignment | Done | `ec5b92e`; smog order and bounds fixed by remediation Phase 6 (`12296f2`, `c37943a`) |
| 2026-10-30-sc2-authentic-pathfinding-and-pushing | [2026-10-30-sc2-authentic-pathfinding-and-pushing.md](2026-10-30-sc2-authentic-pathfinding-and-pushing.md) | SC2 pathfinding, flocking and push priority | Superseded | Remediation Phase 4 (`94ca5c7`), after partial work in `6dd4bea` |
| 2026-10-30-smooth-bubble-paths-and-swarm-smog | [2026-10-30-smooth-bubble-paths-and-swarm-smog.md](2026-10-30-smooth-bubble-paths-and-swarm-smog.md) | Smog defaults, swarm smog volume, smooth trajectories | Partly done | Smog in `ec5b92e`; trajectories superseded by remediation Phase 4 (`94ca5c7`) |
| 2026-11-01-codebase-remediation | [2026-11-01-codebase-remediation.md](2026-11-01-codebase-remediation.md) | Codebase remediation plan | Active | Status per step in its §5 tables; Phase 5 exit confirmed by CI on `7c947ad`; Phase 9 in progress |
| 2026-11-02-host-independent-ron | [2026-11-02-host-independent-ron.md](2026-11-02-host-independent-ron.md) | Host-independent RON output | Done | `d720d6d` |
| 2026-11-02-phase-5-playable-client | [2026-11-02-phase-5-playable-client.md](2026-11-02-phase-5-playable-client.md) | Phase 5: the playable client | Done | Exit met, `7a432f9`; CI run 41583874524 on `7c947ad` |
| 2026-11-02-server-gate | [2026-11-02-server-gate.md](2026-11-02-server-gate.md) | Server gate: an honest `cargo test --workspace --locked` | Done | `c4b92a4`, `6043fc6`, `7858dbf`, `8f1a927` |
| 2026-11-03-ci-disk-budget | [2026-11-03-ci-disk-budget.md](2026-11-03-ci-disk-budget.md) | CI disk budget: compressed debug info and a disk guard | Partly done | `43097e9` (`.cargo/config.toml`), `0938a83` (`ci.yml` guards); local results in its §8.1; missing: the CI run figures of §5 steps 5 to 7, recorded after the push; owner decisions in Q0013 |
| 2026-11-03-gpu-test-harness | [2026-11-03-gpu-test-harness.md](2026-11-03-gpu-test-harness.md) | GPU test harness on the physical GPU, one lock per test child | Proposed | Blueprint only; no implementation commit yet |
| 2026-11-03-host-independent-smog-goldens | [2026-11-03-host-independent-smog-goldens.md](2026-11-03-host-independent-smog-goldens.md) | Host-independent smog goldens: no `libm` bits in class-H hashes | Active | Tests in `ceb45d7`; question Q0014; `Done` after a green CI run on `main` |
| 2026-11-03-phase-9-structure-and-docs | [2026-11-03-phase-9-structure-and-docs.md](2026-11-03-phase-9-structure-and-docs.md) | Phase 9: structure and documentation | Active | WP0 merged (`808b6dd`); remediation steps 9.1 to 9.6 |
