# Space Scuffle — Design and Current State

This document says what Space Scuffle is, how the code is put together, and what state each subsystem is in
today. It states what exists; anything planned but not built is labelled **intent, not built**. The rules for
working on the code are in the charter, [`AGENTS.md`](AGENTS.md). Check C17
(`crates/workspace_checks/tests/charter.rs`) keeps this document's links, evidence paths and tick rate true.

---

## 1. What Space Scuffle Is

Space Scuffle is a real-time strategy game in the spirit of *StarCraft II*: deterministic simulation, crisp unit
control, high-ground vision, turn rates and group movement. Instead of animated 3D models, every mobile unit is a
glass **bubble** and every structure a grounded **dome**, each showing a looping video of its creature or
building, with shader effects for team, facing, attack, damage and death.

**Pillars**

1. **Responsive, deterministic play.** A fixed-point simulation that produces the same result on every platform,
   with weapon wind-up and cancelable backswing for stutter-step micro, turn rates and high-ground vision.
2. **The bubble and dome look.** Units and structures read at a glance through radius, rim, altitude, team tint
   and video state rather than through meshes. Background:
   [`docs/02-bubble-visual-design.md`](docs/02-bubble-visual-design.md).
3. **Data, not code, defines the roster.** Factions, units, structures, upgrades, start loadouts and scenarios are
   RON files under [`assets/`](assets/), read at run time.
4. **Rust on Linux first.** Rust and Bevy (`wgpu`), developed on Linux; the simulation crates also build for
   WebAssembly.

**Intent, not built:** multiplayer over deterministic lockstep, recorded replays of real matches, and a Bot API
for third-party bots running headless matches. Nothing of these exists beyond the determinism they would rely on
(see §4).

---

## 2. Architecture

The workspace has eight crates. Charter §5 draws their dependency graph, and check C1 keeps the drawing equal to
Cargo's.

- **`sim_types`** — the shared contract: fixed-point math (`SimScalar`, `SimVec2`, `SimAngle`), IDs, player
  commands (`PlayerCommand`, `TickBundle`), and the presentation outputs `RenderSnapshot` and `PlayerView`. It also
  owns the tick rate, `sim_types::TICKS_PER_SECOND`.
- **`sim_map`** — terrain: cliff levels, ramps, the map file format, line of sight with high ground, the
  clearance field, A* corridors and the funnel.
- **`sim_core`** — the deterministic tick (`SimWorld`): commands, economy, production and research,
  construction, transport, abilities, vision, targeting, movement, collision and push, weapons, the match result,
  the checksum and the catalogs.
- **`asset_baker`** — generates Veo clips under spending caps, turns them into flipbook atlases through ffmpeg,
  bakes terrain textures, and audits media. It reads the unit catalog from `sim_core` to know which archetypes
  need media and which actions each publishes. A paid run goes `validate-prompts`, `generate-veo --dry-run`
  (both free, no credential read), `veo-preflight` (one free authenticated GET), `generate-veo --max-generations N
  --ledger docs/evidence/veo/ledger.jsonl --clips-dir ~/scratch/veo/clips`, `review`, `install-clip`, then
  `bake-unit-videos`; prompts and the house style are data (`assets/catalog/media_style.ron`), the caps are
  `assets/media/veo_budget.ron`, and exit code 3 means a quota refusal ended the run.
- **`client`** — the Bevy game: renderer, bubble and dome shaders, smog fog presentation, HUD and command card,
  selection and input, result screen, the Visual Lab, and the map editor.
- **`client_diagnostics`** — capture, audit and probe tools built on the client; nothing in the game depends on
  it.
- **`sim_fixtures`** — test-only worlds and catalogs embedded from the shipped data.
- **`workspace_checks`** — the executable charter checks (C1 to C21).
- **`gate`** — `cargo gate`: selects the tests a change can break from the catalog `crates/gate/suites.ron`
  and runs them in the tiers of charter §4.3 (`quick`, `package`, `merge`, `full`), with stamps, flake reruns,
  the red rule's suspect list and the CI timing summary; its catalog check is C22. It depends on no other
  workspace crate.

**Data flow of a match.** The client turns input into `PlayerCommand`s and submits them in a `TickBundle`.
`SimWorld::step_into` advances one tick and writes a `RenderSnapshot`; `PlayerView` gives each player what they
may know. The client presents both. Catalogs (`assets/catalog/`) and scenarios (`assets/scenarios/`) are loaded
from disk at start-up, so changing the roster needs no recompilation (charter §3.3).

---

## 3. Simulation

**Tick rate:** 25 Hz, one tick per 40 ms of game time (`sim_types::TICKS_PER_SECOND`). Every duration the
simulation stores is a whole number of ticks.

**The phases of `SimWorld::step_into`, in order:**

1. Validate transport links (a broken link reports a diagnostic and ends the tick after the checksum and snapshot).
2. Apply the tick's commands.
3. Production and research timers, buffs, regeneration and hazard zones.
4. Unit tasks: every unit whose front order is a task, such as harvesting or building, acts on it.
5. Vision for every player.
6. Target acquisition.
7. Movement and steering.
8. Collision and push.
9. Weapons and projectiles.
10. Reap destroyed entities and their transport groups, and update the match state.
11. Compute the state checksum.
12. Write the `RenderSnapshot`.

**Fixed-point model.** Authoritative positions, velocities, angles, health and timers are fixed-point
(`SimScalar`, 32.32). Floats appear in the simulation crates only where they produce presentation output or read
authored decimal data (check C7). Trigonometry uses a lookup table so every platform computes the same bits.

**Checksum and golden replay.** Each tick ends with a 64-bit checksum of the persistent state, including the RNG
position. A golden replay
([`crates/sim_core/tests/fixtures/golden_replay.ron`](crates/sim_core/tests/fixtures/golden_replay.ron)) must
reproduce every recorded checksum on Linux x86_64, Windows x86_64, macOS ARM64 and `wasm32-wasip1`, in debug and
release; CI's determinism jobs run it.

---

## 4. Subsystem Status

Status is `Working` (built, tested, in the shipped game), `Partial` (built but limited or not all shipped),
`Not built` or `Removed`. Evidence is the test that shows the subsystem working. Open owner questions are in
[`docs/questions/open/`](docs/questions/open/).

| Subsystem | Status | What exists | Evidence | Known gaps |
|---|---|---|---|---|
| Fixed-point math | Working | `SimScalar`, `SimVec2`, `SimAngle`, table trigonometry, checked overflow | `crates/sim_types/tests/math_identities.rs`, `crates/sim_types/tests/math_properties.rs` | none known |
| Tick and checksum | Working | The phase pipeline of §3; versioned checksum over persistent state and RNG | `crates/sim_core/tests/checksum_state_tests.rs` | none known |
| Cross-platform determinism | Working | Golden replay on three native targets and wasm32 in CI; a second fixture pins vision before acquisition and route upkeep before arrival (`c271226`) | `crates/sim_core/tests/golden_replay_tests.rs` (`golden_replay_orders_reproduces_every_checkpoint`) | none known |
| Combat | Working | Weapon timing with turn, damage point, cancelable backswing and cooldown; turrets; shields and armour; splash; projectiles | `crates/sim_core/tests/stutter_step_tests.rs`, `crates/sim_core/tests/combat_correctness_tests.rs`, `crates/sim_core/tests/damage_and_shields_tests.rs` | Changed outcomes await the owner (Q0007); battle 33 investigated in [`docs/evidence/2026-11-03-battle-33-investigation.md`](docs/evidence/2026-11-03-battle-33-investigation.md), changes proposed in Q0035 to Q0041 |
| Movement and pathfinding | Working | Grid clearance field, A* corridor and funnel, steering, push priority and Hold Position | `crates/sim_map/tests/funnel_tests.rs`, `crates/sim_map/tests/clearance_field_tests.rs`, `crates/sim_core/tests/push_priority_tests.rs` | A move to an unreachable point walks to the nearest reachable one, by the owner's choice (Q0006) |
| Terrain, cliffs and ramps | Working | Cliff levels, ramps, the map format and its validation, and the client's terrain mesher; `MapDocument::encode` returns a typed error (`f53f73b`, tests `encode_rejects_*`); the terrain chunks cover the last dual column and row (`94130d5`, test `spawned_chunks_cover_every_dual_cell`) | `crates/sim_map/tests/ramp_traversal_tests.rs`, `crates/sim_map/tests/validation_tests.rs`, `crates/sim_map/tests/map_format_tests.rs`, `crates/client/tests/mesher_golden_tests.rs`, `crates/client/tests/terrain_tests.rs` | none known |
| Simulation vision and watchtowers | Working | Per-player vision with high ground, air reveal and watchtowers | `crates/sim_map/tests/vision_tests.rs`, `crates/sim_core/tests/watchtower_tick_tests.rs` | none known |
| Economy and gas | Working | Harvesting and income for minerals and gas; gas is enabled in the shipped game since its media was baked (Veo batch 1, `2c5ed5e`) | `crates/sim_core/tests/harvest_income_tests.rs`, `crates/client/tests/battle_card_path_tests.rs` (`gas_through_the_card_extractor_snap_harvest_and_income`) | none known |
| Construction | Working | Transactional build admission and a construction lifecycle | `crates/sim_core/tests/construction_lifecycle_tests.rs` | none known |
| Production and research | Working | Production and research queues, cancel and refund | `crates/sim_core/tests/production_transactions_tests.rs`, `crates/sim_core/tests/research_transaction_tests.rs`, `crates/client/tests/battle_card_path_tests.rs` (`research_through_the_card_then_overclock_works`) | none known; research is enabled in the shipped game since Veo batch 1 (`2c5ed5e`) |
| Transport | Working | Load and unload orders with ownership checks; enabled in the shipped game since Veo batch 1 (`2c5ed5e`) | `crates/sim_core/tests/transport_ownership_tests.rs`, `crates/sim_core/tests/load_orders_tests.rs`, `crates/client/tests/battle_card_path_tests.rs` (`load_and_unload_through_right_click_and_the_card`) | none known |
| Abilities and hazard zones | Working | Data-defined abilities, modes and hazard zones | `crates/sim_core/tests/spells_and_abilities_tests.rs` | none known |
| Match result | Working | Victory and defeat from the game rules | `crates/sim_core/tests/match_result_tests.rs` | Rules accepted by the owner (Q0008); where the spec is silent, they follow StarCraft II (charter §8) |
| `PlayerView` | Working | Per-player knowledge for the HUD and admission | `crates/sim_core/tests/player_view_tests.rs` | none known |
| Catalog data | Working | Factions, units, structures, upgrades, start loadouts and scenarios read at run time | `crates/sim_core/tests/runtime_catalog_tests.rs`, `crates/sim_core/tests/faction_catalog_tests.rs` | Data choices await the owner (Q0007) |
| Bubble and dome rendering | Working | Instanced bubbles and domes with optics, dome fill and validated shaders | `crates/client/tests/gpu_validation_tests.rs`, `crates/client/tests/bubble_optics_tests.rs`, `crates/client/tests/dome_fill_tests.rs` | none known |
| Smog fog presentation | Working | Smog field driven by the player's vision; the all-pairs kinematics fallback above 512 bubbles reports its peer bounces (`9987863`, unit test `all_pairs_fallback_reports_its_contacts`, `kinematics_all_pairs_fallback_golden`) | `crates/client/tests/smog_golden_tests.rs`, `crates/client/tests/battle_fog_tests.rs` | Golden hashes name the host `libm` functions they use (Q0014); the solver costs about 6 ms per frame during a 400-unit fight (C-18, partly fixed; §5) |
| Runtime mist | Removed | Deleted in `0f31027` (remediation decision D2) | none | none |
| HUD and command card | Working | Resources, selection card, command card and production queue | `crates/client/tests/battle_hud_tests.rs`, `crates/client/tests/battle_card_tests.rs` | none known |
| Selection and input | Working | Selection, box select, control groups, right-click orders; one declared writer per client concern; input ownership sampled after the inspector draws | `crates/client/tests/battle_input_tests.rs`, `crates/client/tests/battle_context_order_tests.rs`, `crates/client/tests/system_ownership_tests.rs` | none known |
| Result screen | Working | Victory and defeat screen | `crates/client/tests/battle_result_tests.rs`, [`docs/evidence/2026-11-03-result-window-victory.md`](docs/evidence/2026-11-03-result-window-victory.md) | none known |
| Whole skirmish | Working | A skirmish won headless through input events alone | `crates/client/tests/skirmish_match_tests.rs` | none known |
| Map editor | Working | Paint, place, save, playtest and return; a load requested by the UI applies in the same frame | `crates/client/tests/editor_app_tests.rs` (including `document_load_applies_after_the_editor_ui_requests_it`), `crates/client/tests/editor_core_tests.rs` | none known |
| Asset baker | Working | Veo generation of exactly the published actions with prompts from data, a request ledger, per-run and rolling caps, a quota stop, a free preflight, keyframe chaining, review and install; ffmpeg import, flipbook atlases, terrain textures, media audit | `crates/asset_baker/tests/veo_cli_tests.rs`, `crates/asset_baker/tests/bake_integrity_tests.rs`, `crates/asset_baker/tests/ffmpeg_tests.rs` | none known; used live for 41 generations ($16.40 at list price) with loop pinning (`lastFrame`), seam repair, `loop-check`, letterboxing and provenance ([batch 1](docs/evidence/veo/2026-11-04-batch-1.md), [batch 2](docs/evidence/veo/2026-11-05-batch-2.md)) |
| Media coverage | Partial | Real Veo media for 12 of 91 catalog identities; the required set of 10 (`assets/media/required_set.ron`: the skirmish's mineral, gas, research and transport paths) passes `audit-media --strict` with provenance and the request ledger (remediation step 8.5, B-1 fixed, `473b977`); no atlas is a copy of `concept_video.png`; missing media fails loudly | `crates/client/tests/runtime_media_tests.rs`, `crates/asset_baker/tests/media_tests.rs`, [`docs/evidence/veo/2026-11-05-batch-2.md`](docs/evidence/veo/2026-11-05-batch-2.md) | The other 79 identities have none: the rest of the Automaton faction waits for the Veo plan's WP6, the Enclave and Swarm are out of scope; `psi_grey` and `rhino_beetle` keep legacy atlases without provenance |
| Terrain anisotropy on old lavapipe | Partial | Terrain sampling validated on `radv`; a test override for old lavapipe | `crates/client/tests/gpu_validation_tests.rs` | Override kept until this server's Mesa is 25.1 or later (Debian 13 ships 25.0.7), then deleted (Q0009) |
| Networking | Not built | Nothing; intent is deterministic lockstep | none | all |
| Replays | Partial | The two golden replay fixtures only | `crates/sim_core/tests/golden_replay_tests.rs` | No recording of real matches |
| Bot API | Not built | Nothing | none | all |

---

## 5. Performance

**Simulation throughput.** `cargo bench -p sim_core --bench throughput` runs each scenario of
[`crates/sim_core/benches/throughput_scenarios.ron`](crates/sim_core/benches/throughput_scenarios.ron): a flat map,
two armies of the same unit in opposite corners attack-moving across it, 300 ticks per repeat, median of three
repeats. Measured on 2026-11-03 on the development server (16 cores, shared, load average 7 to 11 during the
runs), release build of `56f243f` plus Phase 9 WP13, which changes no simulation behaviour; each figure is the
median of three runs of the benchmark:

| Map | Units | Ticks/s |
|---|---:|---:|
| 64x64 | 0 | 3395 |
| 64x64 | 100 | 1187 |
| 64x64 | 400 | 200 |
| 128x128 | 400 | 225 |
| 256x256 | 0 | 213 |

The slowest scenario runs at about eight times real time. CI's `throughput` job runs the benchmark with
`--floor 25`, the tick rate: a scenario below real time fails the job, and every run's table is published in the
job summary (charter §3.1).

The handover's higher figures (11,114 ticks/s on the empty map, 639 at 400 units) came from the owner's notebook
and were never reproduced on this server at any commit. The 3 to 4 % residual at 400 units since `b263734` is code
layout (branch prediction), not a regression in the simulation
([`docs/evidence/2026-11-03-throughput-history.md`](docs/evidence/2026-11-03-throughput-history.md)).

**Frame budget.** `frame_budget` (`crates/client_diagnostics`) runs the real client on a flat 128×128 arena where
eight players attack-move across a circle with 0, 25 or 50 units each (Q0018, option (a), chosen by the owner). It measures 1,500 ticks
(60 s of game time) after a warm-up. Measured on 2026-11-03 at `c918aa9` on the RX 580 through a headless KWin
compositor, with hardware present, 1280×720 and vsync off, and no monitor (the owner's choice, Q0019). Release build, 16
dedicated cores, load average 1.6 to 3.9 (one run of the 0 scenario at 8.5). Each figure is the median of five
interleaved runs, in ms:

| Units | Frame interval p50 | p95 | p99 | max | Main update p50 | Simulation p50 |
|---:|---:|---:|---:|---:|---:|---:|
| 0 | 12.3 | 15.2 | 16.6 | 22.6 | 10.8 | 0.07 |
| 200 | 15.6 | 19.0 | 20.6 | 44.7 | 13.9 | 0.09 |
| 400 | 16.3 | 20.9 | 23.6 | 68.7 | 14.1 | 0.10 |

Reference lines (Q0015, no pass or fail): 16.7 ms is 60 frames per second, 33.3 ms is 30. The maximum at 200 and
400 units is the single frame of the first tick. The client is CPU-bound in the main world: a quarter of the pixels
changes the 400 median by 3 %, and the GPU passes Bevy times add up to 3.6 ms. Commands, every run, the attribution
runs and why Xvfb's figures are not the game's are in
[`docs/evidence/2026-11-03-frame-budget.md`](docs/evidence/2026-11-03-frame-budget.md).

**Since that measurement.** The table above is the figure of record at `c918aa9`; it has not been re-measured with
the five-run protocol. At `c918aa9` the largest cost was the smog solver, about 8.2 ms of every frame at any unit
count (finding C-18). Plan `2026-11-04-open-findings-c17-c18-p9-gaps` pooled the combat-effect materials (C-17,
`690be35`) and made the solver skip resting bricks and bring a still field to rest (C-18, WP7 to WP9, last
`3f3ab76`). An A/B against the plan's base `e85f79f`, on the RX 580 through headless KWin, medians of three
interleaved runs at 400 units: frame interval p50 16.75 → 12.03 ms, p95 36.30 → 20.92 ms, main update p50
14.97 → 9.82 ms. The solver's own cost per frame (`perf`) fell from 5.0 to 0.3 ms at 0 units and from 7.8 to 6.1 ms
at 400 units. The 400-unit fight cost stays above the plan's 2 ms trigger, so C-18 is partly fixed and a lower
fixed smog rate or a worker thread is left for a later plan. These runs are attribution, not figures of record:
[`docs/evidence/2026-11-04-c18-smog-solver.md`](docs/evidence/2026-11-04-c18-smog-solver.md) §7 and §8, and
[`docs/evidence/2026-11-03-c17-combat-fx-pool.md`](docs/evidence/2026-11-03-c17-combat-fx-pool.md).

**Battle flipbook animation.** Plan `2026-11-04-battle-flipbook-animation` added the GPU flipbook clock and frame
blending (`4bd48cf`, `ec04bf4`, `d5779f7`). An A/B against `d5779f7` with those three merges reverted, at 400 units on
the RX 580 through headless KWin, medians of five interleaved runs: frame interval p50 12.13 → 12.03 ms, main
update p50 9.90 → 9.73 ms. The bubble pass costs 0.458 → 0.460 ms of GPU time. The animation has no measurable
frame-time cost. Two captures of the default script are byte-identical. These are attribution runs on a loaded
machine, not figures of record:
[`docs/evidence/2026-11-04-battle-flipbook-animation.md`](docs/evidence/2026-11-04-battle-flipbook-animation.md).

---

## 6. Documentation Map

- [`AGENTS.md`](AGENTS.md) — the engineering charter and how each rule is checked.
- [`docs/plans/README.md`](docs/plans/README.md) — every implementation plan and what became of it.
- [`docs/reviews/`](docs/reviews/) — independent reviews and findings.
- [`docs/questions/`](docs/questions/) — decisions waiting for the owner.
- [`docs/evidence/`](docs/evidence/) — kept evidence such as captures.
- `docs/01` to `docs/08` — background research written before the code, not a statement of current state:
  [engine evaluation](docs/01-engine-evaluation.md), [bubble visual design](docs/02-bubble-visual-design.md),
  [StarCraft II mechanics](docs/03-sc2-mechanics-reference.md),
  [pathfinding, networking and bot API ideas](docs/04-pathfinding-networking-bot-api.md),
  [early parallel plans](docs/05-parallel-implementation-plans.md),
  [factions and video style](docs/06-factions-and-video-style-guide.md),
  [roster mapping](docs/07-sc2-patch-5-0-14-roster-and-faction-mapping.md),
  [structures and tech tree](docs/08-sc2-structures-tech-tree-and-upgrades.md).
