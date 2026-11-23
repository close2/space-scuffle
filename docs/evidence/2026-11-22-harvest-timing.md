# Evidence: harvest timing per resource (B-150, B-151)

Plan: [`docs/plans/2026-11-22-harvest-timing.md`](../plans/2026-11-22-harvest-timing.md). Measured on the package
branch `harvest-timing-plan` on top of `main` `dd17f4c5`, 2026-11-22, on the shared VM (AMD Radeon RX 580). WP1 is
`adb849ea`, WP2 `22df1d99`. Scratch material lives under `~/scratch/harvest-timing/`
(`wp1/`, `wp2/`, `client/`, `scripts/`).

## 1. What the data says (plan §2.3)

| Resource kind | `HarvestTime` | Ticks | `HarvestAmount` | `ReturnDelay` | Ticks | Source |
|---|---|---|---|---|---|---|
| Mineral | 2.786 Normal s | 49.75 → **50** | **5** | 0.5 Normal s | 8.93 → **9** | liberty BehaviorData:743–756 |
| Gas | 1.981 Normal s | 35.375 → **35** | **4** | none | **0** | liberty BehaviorData:801–852 |

As shipped in `assets/catalog/placement_rules.ron` (WP2), pinned by
`catalog_reference_values_tests::the_shipped_resource_harvests_are_5_0_14`. The worker rows of `units.ron` read
`harvester: Some(())` (WP1).

## 2. WP1: the data moves, bit-identical (plan §8 rule 1)

WP1 authored both rows with the workers' former values (50 / 5 / 0).

| Check | Result |
|---|---|
| Matrix (every battle, seeds of record and 16), battle 31 (100 seeds), battle 33 (64 seeds), the showcase on its default seed and seeds 1 to 8 (6,000 ticks) | 420 files, all output files byte-identical to the prototype's `dd17f4c5` base; the logs differ only in their output path |
| Goldens (`golden_replay_tests`) | 4 of 4 on native debug and release, `wasm32-wasip1` release and debug |
| Harvest tests | `harvest_income_tests`, `harvest_edge_tests`, `full_match_tests`, `player_view_tests` pass unchanged in value |
| Income (scratch `harvest_measure`, eleven tables of §6.3's scope) | byte-identical to the prototype's base tables: the `T₀` of §3 |

## 3. WP2: income against the derivation (plan §6)

Scratch `harvest_measure` (the prototype's tool, `~/scratch/harvest-timing/proto/harvest_measure.rs`, not
committed): per node, a fresh world with the hall (and for gas a completed extractor), `k` workers, 750 ticks of
warm-up, then 3,000 ticks. `T₀` is WP1's mean delivery-to-delivery trip, `T` WP2's; derived `T₀ + 9` (minerals) and
`T₀ − 15` (gas); rates per worker-minute. Every table is byte-identical to the prototype's candidate.

### 3.1 Minerals, `showcase_canyon` slot 0, `k` = 1

| Field | `T₀` | Derived | `T` | Legs | Derived rate | Window count (WP1 → WP2) |
|---|---:|---:|---:|---:|---:|---|
| (3, 14) | 128 | 137 | 137 | 78 | 54.74 | 60.0 → 55.0 |
| (6, 14) | 112 | 121 | 121 | 62 | 61.98 | 67.5 → 62.5 |
| (9, 14) | 114 | 123 | 123 | 64 | 60.98 | 65.0 → 60.0 |
| (12, 14) | 128 | 137 | 137 | 78 | 54.74 | 60.0 → 55.0 |
| (4, 15) | 138 | 147 | 147 | 88 | 51.02 | 55.0 → 50.0 |
| (7, 15) | 126 | 135 | 135 | 76 | 55.56 | 60.0 → 55.0 |
| (10, 15) | 138 | 147 | 147 | 88 | 51.02 | 55.0 → 50.0 |
| (13, 13) | 124 | 133 | 133 | 74 | 56.39 | 60.0 → 57.5 |
| **Mean** | | | | | **55.80** | 60.31 → 55.62 |

- `k` = 2 (slot 0): the same trip on every field; window counts 55.0, 62.5, 61.25, 55.0, 50.0, 55.0, 50.0, 56.25.
- Slot 1: slot 0's mirror, field for field.
- `default_arena` slot 1 (radius 15): `T₀` 120, 114, 114, 134, 130, 126, 144, 167; `T` 129, 123, 123, 143, 139, 135,
  153, 176; derived mean 54.21, window mean 58.12 → 54.38.
- `default_arena` slot 0 (radius 18): `T₀` 232, 228.33, 228.33, 232, 254; `T` 241, 237.27, 237.27, 241, 263; window
  mean 32.0 → 30.0.

### 3.2 Gas, `k` = 1

| Map, slot, geyser | `T₀` | Derived | `T` | Legs | Derived rate | Window count (WP1 → WP2) |
|---|---:|---:|---:|---:|---:|---|
| Showcase 0, (10, 1) | 114 | 99 | 99 | 64 | **60.61** | 65.0 → 60.0 |
| Showcase 0, (1, 1) | 150 | 135 | 135 | 100 | 44.44 | 50.0 → 44.0 |
| Showcase 1, (53, 62) | 114 | 99 | 99 | 64 | 60.61 | 65.0 → 60.0 |
| Showcase 1, (62, 62) | 150 | 135 | 135 | 100 | 44.44 | 50.0 → 44.0 |
| Arena 0, (13, 12) | 142 | 127 | 127 | 92 | 47.24 | 52.5 → 48.0 |
| Arena 1, (50, 56) | 114 | 99 | 99 | 64 | 60.61 | 65.0 → 60.0 |

`k` = 2 and 3 on showcase slot 0: per-worker rates identical to `k` = 1 (no extractor occupancy, B-61).

### 3.3 Acceptance (plan §6.3)

- **A-1, the trip:** every measured `T` equals the derived trip within 0.06 ticks (exact on all but the two
  `default_arena` slot-0 fields whose mean spans a drifting arrival, 237.27 against 237.33).
- **A-2, the income:** every window count within 1.86 minerals (bound 2.5) and 0.76 gas (bound 2.0) of the derived
  rate.
- **A-3, the game:** showcase mean trip-period rate 55.80 in [54.9, 60.7]; fields 51.02 to 61.98 in [50.9, 64.3];
  the near geyser 60.61 in [57.95, 64.05] (the game's 61). Pinned by `harvest_edge_tests::income_lies_in_the_band`
  and `harvest_timing_tests::a_near_geyser_earns_the_games_rate`.

## 4. WP2: worlds that do not harvest (plan §7.3)

Matrix, battles 31 and 33, the showcase on nine seeds: 420 files, every output file byte-identical to WP1's; the logs
differ only in their output path. Nothing in these worlds harvests.

## 5. WP2: goldens (plan §5)

| Build | Old fixtures, WP2's rule with `return_ticks` not hashed and schema 32 (scratch switch, `wp2/golden-proof/scratch-switch.diff`) | New fixtures, schema 33 |
|---|---|---|
| Native release | 4 of 4 | 4 of 4 |
| Native debug | 4 of 4 | 4 of 4 |
| `wasm32-wasip1` release | 4 of 4 | 4 of 4 |
| `wasm32-wasip1` debug | 4 of 4 | 4 of 4 |

The same switch reproduces `sim_fixtures`' `DEFAULT_SANDBOX_CHECKSUM` (`6_458_594_406_894_179_644`), re-pinned to
`3_827_951_643_646_962_361` under schema 33 (release and debug). The four `checksum::encoding` unit tests that spell
the schema bytes read 33.

## 6. WP2: client (plan D-7, R-1)

`skirmish_match_tests::a_match_is_won_through_real_input_alone` (release, radv; logs `client/wp1-skirmish.log` and
`client/wp2-skirmish_match_tests.log`): **Victory** on both trees, well within the pilot's `TICK_LIMIT` (12,000).

| Pilot stage | WP1 tick | WP2 tick |
|---|---:|---:|
| Mine, Resume, Speed | 0 | 0 |
| Raise | 22 | 22 |
| Site | 514 | 550 |
| Foundation | 550 | 586 |
| Train | 606 | 642 |
| Group | 4,466 | 4,502 |
| March | 4,490 | 4,526 |
| Assault (first, second, third) | 4,798, 5,062, 5,326 | 4,834, 5,098, 5,362 |
| Done | 6,450 | 6,486 |
| **Victory** | **6,458** | **6,494** |

The slower bank delays the pilot's first building site by 36 ticks; every later stage moves by the same 36 ticks,
so the match is the same match started later.

The other client targets that harvest (plan §9), on WP2's tree, release, one at a time: `battle_card_path_tests` 4,
`battle_context_order_tests` 24, `battle_fog_tests` 25, `battle_hud_tests` 34, `sync_steady_state_tests` 8,
`battle_flipbook_tests` 9, the client library's unit tests 510: all pass, none changed.

## 7. Deviations from the plan

- **`sim_fixtures`' sandbox pin.** `fixture_tests::DEFAULT_SANDBOX_CHECKSUM` hashes a world under the checksum
  schema, so schema 33 moves it; the plan's file list names only the goldens. It is re-pinned with the goldens'
  proof (§5) and a "Re-recorded for" note.
- **The encoding unit tests.** Four `checksum::encoding` tests spell the schema number in their expected bytes;
  they read 33.
- **A kind without a harvest.** Under `PlacementRules::NONE` (no rules loaded) a node's kind has no harvest; a
  `Harvest` order that reaches the gather then ends with `TaskEventKind::NotCapable`, whose doc says so. No shipped
  or fixture world reaches it: every loaded file has a row per kind.
- **Test splices.** The test helpers that replace a whole placement row (`catalog::validation` placement tests,
  `runtime_catalog_tests`, `footprint_flag_tests`) write the harvest fields too, since they are required.
- **WP1's rows** carried a one-line comment naming the moved values; WP2 replaced it with D-2's sourced comments.
- **Client tests** ran in release; the WP1 skirmish baseline was run on WP1's tree for the comparison of §6.
