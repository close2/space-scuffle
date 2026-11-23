# Evidence: resource occupancy, the acquire radius and the held return wait (B-61, B-67, B-160)

Plan: [`docs/plans/2026-11-23-harvest-occupancy.md`](../plans/2026-11-23-harvest-occupancy.md). Measured on the package
branch `harvest-occupancy-plan` on top of `main` `8f91e0d4`, 2026-11-23, on the shared VM. WP0 is `daee4f99`, WP1
`40a4e97e` (today's values), `f5821ef0` (the data's values) and `5d453ae0` (C7's count); WP2 stopped (§3), was
resumed under the plan's Amendment 1 (`8fff9b55`) and landed with it (§3.3). Scratch material lives under
`~/scratch/harvest-occupancy/` (`tool/`, `impl/`, `world-wp1a/`, `world-wp2/`, `bin-wp1/`, `bin-wp2/`).

## 1. WP1: the acquire fields are the worker's data (plan D-4, §8 rule 1)

The first commit authored today's behaviour (`acquire_radius: 12.0`, `acquires: [Mineral, Gas]`); the second the
harvest ability's values (`10.0`, `[Mineral]`).

| Check (first commit, today's values) | Result |
|---|---|
| Matrix (every battle, seeds of record and 16), battle 31 (100 seeds), battle 33 (64 seeds), the showcase on its default seed and seeds 1 to 8 (6,000 ticks) | 420 files (`world-wp1a/`), every output file byte-identical to the prototype's `f399d403` base; the 12 logs differ only in the output path they name |
| Goldens (`golden_replay_tests`) | 4 of 4 on native release and `wasm32-wasip1` release |
| Harvest tests | `harvest_income_tests`, `harvest_edge_tests`, `harvest_timing_tests` pass unchanged |

| Check (second commit, the data's values) | Result |
|---|---|
| `cargo test --release -p sim_types -p sim_core` | every target passes (94); no existing test moved |
| New | `catalog_reference_values_tests::the_shipped_harvesters_acquire_as_5_0_14`, `harvest_income_tests::an_exhausted_geyser_ends_the_order` (red on the first commit) |
| Take periods, every line and geyser of §6 at `k` = 1 (scratch `harvest_occ`, `impl/income/wp1-*`) | identical to the plan's base (§7.1, §7.2): showcase 0 446.44, arena 1 434.69, near geyser 60.61 |

## 2. WP2: income against the derivation (plan §6, A-1 to A-5)

Scratch `harvest_occ` (the prototype's tool, `tool/harvest_occ.rs`; moves are counted as changes of a worker's
`Harvest` target between ticks, since the tree has no prototype counter). Line mode: one world, `k` workers sent to
each node of the line, 1,500 ticks of warm-up, a 3,000-tick window; take-period rate per node, summed. `T_f` is WP1's
one-worker take period (`impl/income/wp1-*-k1`), the derivation `R_f(k) = min(k × 7,500 / T_f, 150)` (gas
`min(k × 6,000 / T, 171.43)`).

### 2.1 Minerals

| Line | `T_f` (WP1) | `k` | Derived | Measured (WP2) | Difference | Worst field | Moves in the window (whole run) |
|---|---|---:|---:|---:|---:|---:|---|
| Showcase 0 | 137, 121, 123, 137, 147, 135, 147, 133 | 1 | 446.44 | 446.44 | 0.00 | 0.00 | 0 (0) |
| | | 2 | 892.87 | 892.62 | −0.25 | 0.68 | 0 (0) |
| | | 3 | 1,200.00 | **1,200.00** | 0.00 | 0.00 | 0 (0) |
| | | 4 | 1,200.00 | 1,200.00 | 0.00 | 0.00 | 0 (0) |
| Showcase 1 | 137, 121, 121, 137, 147, 135, 147, 135 | 1 | 446.61 | 446.61 | 0.00 | 0.00 | 0 (0) |
| | | 2 | 893.22 | 890.78 | −2.44 | 2.02 | 0 (0) |
| | | 3 | 1,200.00 | **1,200.00** | 0.00 | 0.00 | 0 (0) |
| | | 4 | 1,200.00 | 1,200.00 | 0.00 | 0.00 | 0 (0) |
| Arena 1 | 129, 121, 123, 143, 139, 135, 153, 176 | 1 | 434.69 | 434.69 | 0.00 | 0.00 | 0 (0) |
| | | 2 | 869.38 | 871.76 | +2.38 | 0.77 | 0 (0) |
| | | 3 | 1,174.90 | **1,174.80** | −0.10 | 0.10 | 0 (0) |
| | | 4 | 1,200.00 | 1,200.00 | 0.00 | 0.00 | 0 (0) |

- **A-1:** every line within 2.44 of its derivation (bound 20), every field within 2.02 (bound 2.5); the saturated
  lines exact. Showcase slot 1's third and eighth fields (node order) have one-worker periods of 121 and 135, not
  slot 0's 123 and 133, so its derivation is 893.22, not the plan's 892.9 (the plan took slot 0's `T` for the
  mirror); the measured line is within A-1 of either.
- **A-2:** no move in any window, nor in any warm-up.
- **A-4:** `k` = 1 identical to WP1 on every line.
- Every figure equals the prototype's `OCC BAL` column of plan §7.1.

**A-3, the split** (`node` mode, six workers sent to each showcase slot-0 field in turn, `impl/income/wp2-sc0-minnode-k6`):
6 to 8 moves; the bank earns 327.5 to 342.5 a minute, 54.6 to 57.1 per worker, the one-worker rates of the fields
they settle on. Two workers sent to one field (`wp2-sc0-minnode-k2`): one move each, for example (3, 14): 105.0
counted.

### 2.2 Gas (`node` mode, 750 ticks of warm-up)

| Geyser, `T` (WP1) | `k` = 1 derived / measured | 2 | 3 | 4 |
|---|---|---|---|---|
| Showcase (10, 1), 99 | 60.61 / 60.61 | 121.21 / 121.21 | 171.43 / **171.43** | 171.43 / 171.43 |
| Showcase (1, 1), 135 | 44.44 / 44.44 | 88.89 / 88.89 | 133.33 / 133.33 | 171.43 / **171.43** |
| Arena 0 (13, 12), 127 | 47.24 / 47.24 | 94.49 / 95.44 | 141.73 / 141.73 | 171.43 / **171.43** |
| Arena 1 (50, 56), 99 | 60.61 / 60.61 | 121.21 / 121.21 | 171.43 / **171.43** | 171.43 / 171.43 |

**A-1:** exact but one (arena 0, `k` = 2: +0.95, one load's phase; bound 2.0). **A-5:** the near geysers saturate with
3, the corner geyser and arena 0 only with 4.

### 2.3 The game's shape (A-5), reported, not banded

| | Here | `StarCraft` II (Liquipedia, Resources) |
|---|---:|---:|
| One worker a field | 55.8 | 55–60 |
| Two a field | 111.6 | 110–120 |
| Three a field | 150 | 143 (theory 150.75) |
| Showcase line, two a field | 892.6 | about 925 (−3.5 %) |
| Showcase line, three a field | 1,200 | about 1,140 measured (+5.3 %, B-162), 1,206 theory (−0.5 %) |
| Near geyser, 1 / 2 / 3 | 60.6 / 121.2 / 171.4 | 61 / 123 / 163 (cap 169.6) |

### 2.4 Worlds that do not harvest (§8 rule 2)

The matrix, battles 31 and 33 and the showcase on nine seeds (`world-wp2/`, the same commands as §1): every output
file byte-identical to the base; the logs differ only in their output path.

### 2.5 Tests

- `harvest_occupancy_tests` (10 tests) and `harvest_edge_tests::saturation_follows_the_derivation`: every one red on
  WP1's tree, green on WP2's.
- `cargo test --release -p sim_types -p sim_core`: every target passes (95).
- Goldens (`golden_replay_tests`): 4 of 4 on native debug and release, `wasm32-wasip1` release and debug
  (`impl/golden-wp2/`). Schema 33, no re-record.

### 2.6 Client (release, radv, one target at a time under the gate lock; `impl/client-wp2/`)

| Target | Result |
|---|---|
| `skirmish_match_tests` | 7 passed, 1 ignored by design. `a_match_is_won_through_real_input_alone`: **Victory at tick 6,486**, every stage on the prototype base's tick (Mine 0, Raise 22, Site 550, Foundation 586, Train 642, Group 4,502, March 4,526, assaults 4,834, 5,098, 5,362); the windowed HUD child logged `Done` at 115 |
| `battle_hud_tests`, `battle_card_path_tests`, `battle_context_order_tests`, `battle_fog_tests`, `battle_flipbook_tests`, `client --lib battle_pipeline` | pass |
| `sync_steady_state_tests` (before Amendment 1) | **7 passed, 1 failed**: `selected_structure_keeps_card_queue_and_feed_buffers_on_every_tick`, "retained buffers that moved or grew": `hud.view.laden` capacity 4 → 8 in the window. Reproduced twice on WP2's tree; passes on WP1's tree (the same binary with WP2's `sim_core/src` stashed) |

## 3. Stop: WP2 (plan §8 rule 2)

`sync_steady_state_tests::selected_structure_keeps_card_queue_and_feed_buffers_on_every_tick` fails on WP2's tree.
It is not listed in §7.4 (§7.5 ran only `skirmish_match_tests`), so by §8 rule 2 it is a finding and the package
stopped. WP1 was the last passing package; WP2's work was kept on branch `harvest-occupancy-wp2-wip` until §3.3.

### 3.1 Trace

The test sends the local player's six workers to one mineral field, waits for the first delivery ("The first delivery
grows the lists of laden workers to their working size, which is a buffer's first use, not churn, so the window
starts after it"), queues jobs, runs 30 ticks, records every retained buffer and then asserts that none moves or
grows over a 200-tick window. `PlayerView::laden` (the player's workers holding cargo, refilled each tick in
`sim_core::player_view`) is one of them.

A temporary trace in the test (not committed; `impl/client-wp2/sync-trace-wp2.log`) printed per tick the laden count
and each worker's `Harvest` target (`C` carrying, `G` gathering):

| Tick | Laden | Workers (id:node) |
|---|---:|---|
| 60 | 0 | all six on node 0, walking |
| 100 | 0 | 2:0G 3:4G 4:1 5:4 6:1G 7:5 (spreading: node 0 occupied) |
| 149 | 3 | 2:0C 3:4C 4:6 5:2G 6:1C 7:5 (six distinct fields: 0, 4, 6, 2, 1, 5) |
| 167 | 2 | first delivery (bank 50 → 55): the window's premise is set here |
| 197 | 1 | warm-up ends; buffers recorded (laden capacity 4) |
| 287 | **5** | 2:0C 3:4C 4:6C 5:2G 6:1C 7:5C: the list outgrows capacity 4 → 8 |
| 364 | 0 | |

On WP1 (the same instrumented test with WP2's `sim_core/src` stashed, `impl/client-wp2/sync-trace-wp1.log`) the six
workers gather the one field together: the laden count rises 1 to 6 between ticks 119 and 142, before the first
delivery at 167, so the list reaches its working size (6, capacity 8) before the window, where it peaks at 6 again
(tick 279). With occupancy they spread
over six fields (A-3, as S9 describes), and their trips (121 to 147 ticks) put them out of phase: before the first
delivery at most 3 are laden, and the list reaches 5 only 120 ticks later, inside the window. The growth is the
list's first use at its working size, not churn: nothing is allocated again once the high-water mark is reached.
The simulation is as the plan derives; the test's premise that one delivery sets the working size no longer holds.

### 3.2 Proposed amendment (as handed back; the orchestrator's decision is §3.3)

**Amendment 1 (proposed): the steady-state window starts once the laden list has held every worker.** The list can
hold at most the local workers (six), so its working size is reached once it has held them all; with workers out of
phase that is not the first delivery. Proposed change, test only, no bound loosened:
`selected_structure_keeps_card_queue_and_feed_buffers_on_every_tick` warms up until the `hud.view.laden` buffer's
recorded capacity covers `local_workers().len()` (read with the test's own `retained_buffers`), with a frame bound
derived from the slowest one-worker trip of the skirmish's fields (147 ticks here) times the worker count, instead of
until the first delivery; the 200-tick window and its assertions are unchanged. An alternative that keeps the test
untouched is a production change outside this plan's files: `sim_core::player_view` reserving `laden` for the
player's harvesters when it refills the view (a `PERFORMANCE RATIONALE` on the reserve), so the list never grows past
its first fill. The plan's §9 should then list `sync_steady_state_tests` among the client tests WP2 re-derives.

### 3.3 Amendment 1 as decided, and the re-run

The orchestrator chose a test-only change (plan Amendment 1, `8fff9b55`): the test's warm-up runs until every local
worker has completed at least one delivery, read from the simulation (each worker's cargo going from held to none);
the window and its assertions are unchanged; no `player_view` reserve. The bank-of-minerals helper the old warm-up
used was removed with it.

Re-run on WP2's tree with the change (release, radv, one target at a time under the gate lock; `impl/client-wp2b/`):

| Target | Result |
|---|---|
| `skirmish_match_tests` | 7 passed, 1 ignored by design; Victory at tick 6,486, every stage unchanged; HUD child `Done` at 115 |
| `sync_steady_state_tests` | **8 passed** |
| `battle_hud_tests`, `battle_card_path_tests`, `battle_context_order_tests`, `battle_fog_tests`, `battle_flipbook_tests`, `client --lib battle_pipeline` | pass |
