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
- **`asset_baker`** — turns Veo videos into flipbook atlases through ffmpeg, bakes terrain textures, and audits
  media. It reads the unit catalog from `sim_core` to know which archetypes need media.
- **`client`** — the Bevy game: renderer, bubble and dome shaders, smog fog presentation, HUD and command card,
  selection and input, result screen, the Visual Lab, and the map editor.
- **`client_diagnostics`** — capture, audit and probe tools built on the client; nothing in the game depends on
  it.
- **`sim_fixtures`** — test-only worlds and catalogs embedded from the shipped data.
- **`workspace_checks`** — the executable charter checks (C1 to C21).

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
| Cross-platform determinism | Working | Golden replay on three native targets and wasm32 in CI | `crates/sim_core/tests/golden_replay_tests.rs` | The replay does not pin every phase order (two swaps pass it; Phase 9 blueprint §9) |
| Combat | Working | Weapon timing with turn, damage point, cancelable backswing and cooldown; turrets; shields and armour; splash; projectiles | `crates/sim_core/tests/stutter_step_tests.rs`, `crates/sim_core/tests/combat_correctness_tests.rs`, `crates/sim_core/tests/damage_and_shields_tests.rs` | Changed outcomes await the owner (Q0007) |
| Movement and pathfinding | Working | Grid clearance field, A* corridor and funnel, steering, push priority and Hold Position | `crates/sim_map/tests/funnel_tests.rs`, `crates/sim_map/tests/clearance_field_tests.rs`, `crates/sim_core/tests/push_priority_tests.rs` | A move to an unreachable point walks to the nearest reachable one (Q0006, r09) |
| Terrain, cliffs and ramps | Working | Cliff levels, ramps, the map format and its validation, and the client's terrain mesher | `crates/sim_map/tests/ramp_traversal_tests.rs`, `crates/sim_map/tests/validation_tests.rs`, `crates/client/tests/mesher_golden_tests.rs` | `MapDocument::encode` panics on a document whose cell count does not match its size; the client leaves the last dual column or row unmeshed when a map side is a multiple of 16 (Phase 9 blueprint §9) |
| Simulation vision and watchtowers | Working | Per-player vision with high ground, air reveal and watchtowers | `crates/sim_map/tests/vision_tests.rs`, `crates/sim_core/tests/watchtower_tick_tests.rs` | none known |
| Economy and gas | Partial | Harvesting and income for minerals and gas | `crates/sim_core/tests/harvest_income_tests.rs` | Gas is disabled in the shipped game for want of media (Q0001) |
| Construction | Working | Transactional build admission and a construction lifecycle | `crates/sim_core/tests/construction_lifecycle_tests.rs` | none known |
| Production and research | Partial | Production and research queues, cancel and refund | `crates/sim_core/tests/production_transactions_tests.rs`, `crates/sim_core/tests/research_transaction_tests.rs` | Research is disabled in the shipped game (Q0001) |
| Transport | Partial | Load and unload orders with ownership checks | `crates/sim_core/tests/transport_ownership_tests.rs`, `crates/sim_core/tests/load_orders_tests.rs` | Disabled in the shipped game (Q0001) |
| Abilities and hazard zones | Working | Data-defined abilities, modes and hazard zones | `crates/sim_core/tests/spells_and_abilities_tests.rs` | none known |
| Match result | Working | Victory and defeat from the game rules | `crates/sim_core/tests/match_result_tests.rs` | Rules await the owner (Q0008) |
| `PlayerView` | Working | Per-player knowledge for the HUD and admission | `crates/sim_core/tests/player_view_tests.rs` | none known |
| Catalog data | Working | Factions, units, structures, upgrades, start loadouts and scenarios read at run time | `crates/sim_core/tests/runtime_catalog_tests.rs`, `crates/sim_core/tests/faction_catalog_tests.rs` | Data choices await the owner (Q0007) |
| Bubble and dome rendering | Working | Instanced bubbles and domes with optics, dome fill and validated shaders | `crates/client/tests/gpu_validation_tests.rs`, `crates/client/tests/bubble_optics_tests.rs`, `crates/client/tests/dome_fill_tests.rs` | none known |
| Smog fog presentation | Working | Smog field driven by the player's vision | `crates/client/tests/smog_golden_tests.rs`, `crates/client/tests/battle_fog_tests.rs` | Golden hashes name the host `libm` functions they use (Q0014); above 512 bubbles the reported peer-bounce count is 0 (Phase 9 blueprint §9) |
| Runtime mist | Removed | Deleted in `0f31027` (remediation decision D2) | none | none |
| HUD and command card | Working | Resources, selection card, command card and production queue | `crates/client/tests/battle_hud_tests.rs`, `crates/client/tests/battle_card_tests.rs` | none known |
| Selection and input | Working | Selection, box select, control groups, right-click orders; one declared writer per client concern; input ownership sampled after the inspector draws | `crates/client/tests/battle_input_tests.rs`, `crates/client/tests/battle_context_order_tests.rs`, `crates/client/tests/system_ownership_tests.rs` | none known |
| Result screen | Working | Victory and defeat screen | `crates/client/tests/battle_result_tests.rs`, [`docs/evidence/2026-11-03-result-window-victory.md`](docs/evidence/2026-11-03-result-window-victory.md) | none known |
| Whole skirmish | Working | A skirmish won headless through input events alone | `crates/client/tests/skirmish_match_tests.rs` | none known |
| Map editor | Working | Paint, place, save, playtest and return; a load requested by the UI applies in the same frame | `crates/client/tests/editor_app_tests.rs` (including `document_load_applies_after_the_editor_ui_requests_it`), `crates/client/tests/editor_core_tests.rs` | none known |
| Asset baker | Working | Veo download, ffmpeg import, flipbook atlases, terrain textures, media audit | `crates/asset_baker/tests/bake_integrity_tests.rs`, `crates/asset_baker/tests/ffmpeg_tests.rs` | none known |
| Media coverage | Partial | Real media for part of the roster; missing media fails loudly | `crates/client/tests/runtime_media_tests.rs` | Paid generation awaits the owner (Q0001) |
| Terrain anisotropy on old lavapipe | Partial | Terrain sampling validated on `radv`; a test override for old lavapipe | `crates/client/tests/gpu_validation_tests.rs` | Override kept until the owner decides (Q0009) |
| Networking | Not built | Nothing; intent is deterministic lockstep | none | all |
| Replays | Partial | The golden replay fixture only | `crates/sim_core/tests/golden_replay_tests.rs` | No recording of real matches |
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

**Frame budget.** Not yet measured at 200 and 400 units; that is the exit criterion of remediation Phase 6
([`docs/plans/2026-11-01-codebase-remediation.md`](docs/plans/2026-11-01-codebase-remediation.md)).

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
