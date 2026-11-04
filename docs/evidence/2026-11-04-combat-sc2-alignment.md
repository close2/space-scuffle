# Combat SC2 alignment: measurements per step

This file records the measurement of every step of plan
[`2026-11-04-combat-sc2-alignment`](../plans/2026-11-04-combat-sc2-alignment.md) (§D-5): the whole matchup matrix on
20 seeds compared with the previous step, battle 33 on 68 seeds, and the golden replays. WP0 creates it with the
baseline (section 1); every later package appends its own section in the commit that makes the change.

## How to reproduce

The tool is `crates/sim_core/examples/matchup_trace.rs` (`sim_fixtures::trace`). Always a release build:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<wp>
cargo build --release --locked -p sim_core --example matchup_trace
TOOL=$CARGO_TARGET_DIR/release/examples/matchup_trace
# Whole matrix: the four seeds of record plus the 16 smallest positive integers that are not
# (1 to 19 without 11, 22 and 33), so 20 seeds per battle; outcomes only.
$TOOL --all-battles --seeds 16 --jobs 8 --out ~/scratch/combat_align/<step>/matrix \
    [--compare ~/scratch/combat_align/<prev>/matrix/matrix.csv]
# Battle 33: the seeds of record plus 64 more (1 to 67 without 11, 22 and 33), 68 seeds.
$TOOL --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv \
    --out ~/scratch/combat_align/<step>/b33
```

- **`matrix.csv`** has one row per battle and seed: battle, seed, of record, end tick, survivors blue and red, the
  life of each side's first unit (empty when it died) as the exact decimal of the fixed-point value, and whether the
  run meets the battle's expectation. The judgement is `Expected::holds` in `sim_fixtures::matchup`, the same code
  the matrix test (`sc2_who_wins_matchup_matrix_tests.rs`) uses, so a seed of record fails here exactly when the test
  fails.
- **`matrix.md`** has the per-battle summary (wins, unresolved, means, expectation failures of record and over all
  seeds), a table of every failing run, and with `--compare` either "bit-identical to `<file>`" or a "Moved" table
  with every battle whose rows differ, before and after.
- **Battle 33's figures** come from the `--battle` run's `seeds.csv` and `seed_<n>_ticks.csv`. K (zealots in contact)
  over the first 5 s is the mean of the `red_engaged` column over the 125 ticks starting at the seed's
  `red_first_contact` tick, the investigation's definition (§3.5 of
  [`2026-11-03-battle-33-investigation.md`](2026-11-03-battle-33-investigation.md)). The script that computes these
  is a throwaway kept beside the outputs on the development server; it only averages those columns.
- **Release counting.** From WP0 on, the trace reads a shot release from the weapon cooldown rising, not from the
  phase pattern (`trace::swing`), so the counts stay right after WP1 and WP2 change the phases. On the shipped
  catalog this gives the same counts as before: every scripted test of `trace_tests.rs` passes unchanged, and battle
  33 reproduces the investigation below.

## 1. Baseline (WP0)

**Commit.** Measured on the WP0 branch at `4e25bb3` (the tool), whose simulation and catalog are `main` at
`0e5565a`: no rule or data change. WP0 answers no question by itself; it is the reference every later step is
compared with. Outputs on the development server under `~/scratch/combat_wp0/baseline/` (not committed; regenerated
bit for bit by the commands above).

### 1.1 Whole matrix, 20 seeds

- **50 battles, 1000 runs. Expectation failures: 0 on the seeds of record, 2 over all 20 seeds**, both battle 33
  (seeds 1 and 6, zealot wins with 2 zealots left; see §1.2).
- **Lead-unit hull is now checked on every seed**, by the same code as the test. The seven battles with a hull range
  all hold it on all 20 seeds: 2a (200), 3 (200), 11b (95), 12c (125), 15 (424.06 mean), 19 (125) and 25b (the
  zealot ends with 22.0 on the four seeds of record, 23.5 on average over 20; range 5 to 45). The investigation
  (§5.7) had checked hull by hand only.
- **Determinism of the tool.** A second run with `--jobs 4` instead of 8 and `--compare` against the first reported
  "bit-identical" (all 1000 rows), and the two `matrix.csv` files are byte-identical (SHA-256
  `fa7ec1655bdd79000a7dfc65a0db1f4d4972883a83c58610764bc713d804030a`).
- **Agreement with the investigation's control** (§5.7, shipped catalog, same 20 seeds): battle 23a 0 / 2.10, battle
  32 0.85 / 0.80, battle 33 7.65 / 0.20 with 0 / 2 failures; every survivor mean and end tick in the plan's §5.1
  "Shipped" column matches the table below.
- **Cost.** About one minute for the 1000 runs with 4 to 8 worker threads in a release build on this server.

Means over the 20 seeds; lead hull is the first unit's life, averaged over the seeds it lived on (that count in
brackets).

| Battle | Seeds | Blue wins | Red wins | Unresolved | Both gone | Survivors blue – red | End tick | Blue lead hull | Red lead hull | Failures of record | Failures, all seeds |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1a_stalker_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 281.0 | 5.00 (20) | — | 0 | 0 |
| 1b_stalker_vs_2_roaches | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 159.8 | — | 106.75 (20) | 0 | 0 |
| 2a_immortal_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 61.0 | 200.00 (20) | — | 0 | 0 |
| 2b_immortal_vs_2_roaches | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 145.0 | 174.00 (20) | — | 0 | 0 |
| 3_immortal_vs_tank | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 89.0 | 200.00 (20) | — | 0 | 0 |
| 4_marauder_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 190.1 | 20.00 (20) | — | 0 | 0 |
| 5a_hydra_vs_marine | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 52.0 | 66.98 (20) | — | 0 | 0 |
| 5b_hydra_vs_2_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 112.0 | 20.18 (20) | — | 0 | 0 |
| 5c_hydra_vs_3_marines | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 78.0 | — | 40.33 (18) | 0 | 0 |
| 6a_roach_vs_hydra | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 149.7 | 35.00 (20) | — | 0 | 0 |
| 6b_2_roaches_vs_hydra | 20 | 20 | 0 | 0 | 0 | 2.00 – 0.00 | 65.9 | 118.60 (20) | — | 0 | 0 |
| 6c_roach_vs_2_hydras | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 96.3 | — | 61.98 (20) | 0 | 0 |
| 7_zealot_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 253.3 | 12.00 (20) | — | 0 | 0 |
| 8_zealot_vs_stalker | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 253.8 | 51.00 (20) | — | 0 | 0 |
| 9a_hellion_vs_4_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 94.0 | 23.80 (20) | — | 0 | 0 |
| 9b_hellion_vs_roach | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 148.0 | — | 117.00 (20) | 0 | 0 |
| 10_2_hellions_vs_8_lings | 20 | 20 | 0 | 0 | 0 | 2.00 – 0.00 | 57.2 | 69.80 (20) | — | 0 | 0 |
| 11a_2_banelings_vs_5_marines | 20 | 0 | 9 | 0 | 11 | 0.00 – 2.25 | 9.2 | — | 10.00 (9) | 0 | 0 |
| 11b_2_banelings_vs_marauder | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 9.1 | — | 95.00 (20) | 0 | 0 |
| 12a_colossus_vs_6_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 81.8 | 243.70 (20) | — | 0 | 0 |
| 12b_colossus_vs_3_marauders | 20 | 0 | 20 | 0 | 0 | 0.00 – 3.00 | 164.0 | — | 72.80 (20) | 0 | 0 |
| 12c_viking_vs_colossus | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 474.0 | 125.00 (20) | — | 0 | 0 |
| 13_storm_vs_held_lings | 20 | 0 | 0 | 0 | 20 | 0.00 – 0.00 | 70.0 | — | — | 0 | 0 |
| 14_thor_vs_ultralisk | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 187.0 | 37.00 (20) | — | 0 | 0 |
| 15_plated_ultralisk_vs_8_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 111.0 | 424.06 (20) | — | 0 | 0 |
| 16a_muta_vs_marine | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 115.0 | 74.24 (20) | — | 0 | 0 |
| 16b_muta_vs_2_marines | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 183.0 | — | 27.00 (11) | 0 | 0 |
| 17_void_ray_vs_muta | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 202.0 | 150.00 (20) | — | 0 | 0 |
| 18_void_ray_vs_corruptor | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 252.0 | 90.00 (20) | — | 0 | 0 |
| 19_viking_vs_brood_lord | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 293.0 | 125.00 (20) | — | 0 | 0 |
| 20_brood_lord_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 142.2 | 225.00 (20) | — | 0 | 0 |
| 21_lurker_vs_6_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 99.2 | 82.92 (20) | — | 0 | 0 |
| 22a_reaper_vs_2_workers | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 102.0 | 40.63 (20) | — | 0 | 0 |
| 22b_reaper_vs_stalker | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 178.0 | — | 80.00 (20) | 0 | 0 |
| 23a_zealot_vs_6_lings | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.10 | 266.2 | — | — | 0 | 0 |
| 23b_upgraded_zealot_vs_6_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 259.7 | 53.20 (20) | — | 0 | 0 |
| 24a_roach_vs_5_lings | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 296.6 | — | — | 0 | 0 |
| 24b_upgraded_roach_vs_5_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 255.6 | 85.00 (20) | — | 0 | 0 |
| 25a_2_marines_vs_zealot | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 152.9 | — | 61.75 (20) | 0 | 0 |
| 25b_upgraded_marines_vs_zealot | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 148.9 | — | 23.50 (20) | 0 | 0 |
| 26_100_lings_vs_6_colossi | 20 | 0 | 20 | 0 | 0 | 0.00 – 6.00 | 120.0 | — | 250.00 (20) | 0 | 0 |
| 27_100_lings_vs_8_ultralisks | 20 | 0 | 20 | 0 | 0 | 0.00 – 8.00 | 203.1 | — | 432.59 (20) | 0 | 0 |
| 28_100_lings_vs_10_immortals | 20 | 20 | 0 | 0 | 0 | 27.30 – 0.00 | 670.5 | 35.00 (3) | — | 0 | 0 |
| 29_100_lings_vs_12_stalkers | 20 | 20 | 0 | 0 | 0 | 71.50 – 0.00 | 440.4 | 34.41 (18) | — | 0 | 0 |
| 30_20_roaches_vs_10_immortals | 20 | 0 | 20 | 0 | 0 | 0.00 – 9.95 | 263.9 | — | 128.58 (19) | 0 | 0 |
| 31_15_marauders_vs_15_stalkers | 20 | 15 | 5 | 0 | 0 | 2.65 – 0.70 | 661.3 | 88.17 (6) | — | 0 | 0 |
| 32_30_marines_vs_15_banelings | 20 | 7 | 9 | 0 | 4 | 0.85 – 0.80 | 53.5 | 10.00 (6) | — | 0 | 0 |
| 33_24_marines_vs_12_zealots | 20 | 18 | 2 | 0 | 0 | 7.65 – 0.20 | 421.1 | 38.60 (15) | — | 0 | 2 |
| 34_20_hydras_vs_12_mutas | 20 | 20 | 0 | 0 | 0 | 19.10 – 0.00 | 192.6 | 90.00 (20) | — | 0 | 0 |
| 35_12_vikings_vs_6_colossi | 20 | 19 | 0 | 1 | 0 | 12.00 – 0.05 | 309.2 | 125.00 (20) | — | 0 | 0 |

### 1.2 Battle 33, 68 seeds

| Measure | Result | Investigation §4.2, §3.5 |
|---|---|---|
| Marine wins / zealot wins / unresolved at 500 ticks | **61 / 6 / 1** | 61 / 6 / 1 |
| Zealot wins (seeds) | 1, 6, 28, 50, 58, 63 | the same |
| Unresolved | seed 19, 3 marines against 1 zealot | the same |
| Marine survivors, mean (SD), median, range | **7.41** (3.34), 8, 0 to 14 | 7.41 (3.34), 8, 0 to 14 |
| End tick, mean (SD), range | 420.4 (36.6), 351 to 500 | 420.4 (36.6), 351 to 500 |
| K over the first 5 s, mean (SD), range | **4.25** (0.40), 3.23 to 4.97 | 4.25 (0.40), 3.23 to 4.97 |
| Seeds of record 3203386110, 11, 22, 33 (marines – zealots) | 8–0, 6–0, 9–0, 13–0 | the same |

The baseline reproduces the investigation exactly, with the new release inference in the tool. Battle 33 is about a
90 % marine win under jitter; Q0041's answer records that in a comment beside the battle in
`crates/sim_core/tests/fixtures/matchup_matrix.ron`, with no expectation, seed or claim changed.

### 1.3 Golden replays

Not touched by WP0: no simulation code or catalog value changed, and `cargo test --workspace` passes both
`golden_replay.ron` and `golden_replay_orders.ron` unchanged.

### 1.4 Check after merging WP1

WP1 (the backswing rule, `b2beb83`) reached `main` before WP0, so WP0's branch merged it and re-ran both
measurements at `961517c` against the baseline above: the whole matrix on 20 seeds reported **bit-identical** to the
baseline `matrix.csv` (all 1000 rows), and battle 33's 68-seed `seeds.csv` is byte-identical. This is the tool's
"no change" proof that WP1 asked for (plan D-1, item 2); WP1's own section may cite it.

## 2. WP2: a damage point of 0 releases on the tick its swing starts (Q0036, D-3)

**Commit.** The rule commit of WP2 on `main` at `36943e1` (WP0 and WP1 merged). `combat.rs` now has one release
routine (`release_swing`), called by `step_windup` when the wind-up ends and by `start_swing` at once when the
damage point is 0; `step_ready_weapon`, `step_rotating_weapon` and `step_backswing` return whether they released.
A damage-point-0 swing never shows a `Windup`, so `period_ticks` is the attack cycle for every weapon. Before, such a
weapon held `Windup { remaining: 0 }` for one tick and fired every `period + 1` ticks. Answers the cadence part of
Q0036 ("clean up the quirk if appropriate"). Outputs under `~/scratch/combat_wp2/after/` on the development server.

**Who is affected.** The six shipped identities with a damage point of 0 (plan §2.2), each now one tick faster per
cycle, at its authored period:

| Identity | Weapon | Period (cycle now) | Cycle before | In matrix battles |
|---|---|---|---|---|
| `napalm_wheeler` (hellion) | Napalm Emitter | 45 | 46 | 9a, 9b, 10 |
| `tripod_annihilator` (colossus) | Thermal Lance Sweep | 27 | 28 | 12a, 12b, 12c, 26, 35 |
| `prismatic_saucer` (void ray) | Prismatic Beam (both) | 9 | 10 | 17, 18 |
| `flying_saucer` | Disruptor Beam (both) | 20 | 21 | none (orders golden) |
| `sky_bastion` | Concord Cannon | 28 | 29 | none |
| `prism_weaver` | Disruption Beam | 18 | 19 | none |

### 2.1 Whole matrix, 20 seeds

Compared with the state after WP1 (§1.4, bit-identical to the baseline of §1.1). **7 battles moved, exactly the
plan's list (§5.1); no battle changed its winner; expectation failures 0 on the seeds of record and 2 over all
seeds**, battle 33's seeds 1 and 6 as in the baseline. No entry of `matchup_matrix.ron` needed re-recording: every
moved battle stays inside its expectation on every seed.

| Battle | Seeds moved | Survivors blue – red, before → after | End tick, before → after | Lead hull, before → after |
|---|---|---|---|---|
| 9a_hellion_vs_4_lings | 20 | 1.00 – 0.00 → same | 94.0 → 91.0 | blue 23.80 → 27.80 |
| 10_2_hellions_vs_8_lings | 20 | 2.00 – 0.00 → same | 57.2 → 55.0 | blue 69.80 → 70.00 |
| 12a_colossus_vs_6_marines | 20 | 1.00 – 0.00 → same | 81.8 → 78.0 | blue 243.70 → 243.80 |
| 12b_colossus_vs_3_marauders | 20 | 0.00 – 3.00 → **0.00 – 2.00** | 164.0 → 164.0 | red 72.80 (20) → 67.53 (19) |
| 17_void_ray_vs_muta | 20 | 1.00 – 0.00 → same | 202.0 → 181.0 | blue 150.00 → 150.00 |
| 18_void_ray_vs_corruptor | 20 | 1.00 – 0.00 → same | 252.0 → 226.0 | blue 90.00 → 110.00 |
| 26_100_lings_vs_6_colossi | 20 | 0.00 – 6.00 → same | 120.0 → 114.8 | red 250.00 → 250.00 |

- **12b** keeps its expectation `red: (1, 999)`: the colossus now kills one marauder before it falls, on every seed
  (survivors 2 on all 20; the first marauder dies on seed 10). The plan's probe predicted this (3 → 2, still
  passing).
- **Not moved although a damage-point-0 unit fights:** 9b (traced on the seeds of record: the hellion releases 4
  shots before it dies at tick 148, and the end tick and the roach's life are the same before and after) and 12c
  and 35 (traced:
  the colossi release no shot at all against the vikings). Their rows are bit-identical.
- The figures agree with the plan's probe (§5.1, "Cadence fix" column, period − 1 proxy) to within one or two ticks
  of end tick (9a 92 → 91, 10 52 → 55, 12a 79 → 78, 17 182 → 181, 18 227 → 226, 26 116 → 114.8); the proxy kept the
  one-tick wind-up, so it fired one tick later on the first swing.

### 2.2 Battle 33, 68 seeds

**Bit-identical** to §1.2: `seeds.csv` and all 68 `seed_<n>_ticks.csv` files are byte-identical to the post-WP1 run
(no damage-point-0 unit fights in it). Marine wins / zealot wins / unresolved 61 / 6 / 1, marine survivors 7.41
(3.34), K over the first 5 s 4.25 (0.40), seeds of record 8–0, 6–0, 9–0, 13–0, all unchanged.

### 2.3 Golden replays

- **`golden_replay.ron`: unchanged** (no unit in it has a damage point of 0).
- **`golden_replay_orders.ron`: re-recorded.** Its `blue_spotter` (`flying_saucer`, period 20, damage point 0) shoots
  at the zealots on the ledge. A throwaway observer that logs every rise of the spotter's cooldown (not committed)
  showed releases at ticks 122, 143, 164, … (every 21) before and 121, 141, 161, … (every 20) after, the last at 290
  before and 281 after. The first differing checkpoint is **13 (ticks 121 to 130)**; checkpoints 13 to 31 changed,
  and from checkpoint 32 (ticks 311 to 320) on the checksums are identical to the old list again: the ledge fight is
  over and the spotter's cooldown has run out in both runs, so no state the cadence touched remains. Both phase-order
  situations the fixture exists for (same-tick reveal, mid-route repath) are still found by the test.
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`, as CI) all reproduce both fixtures.

### 2.4 The trace after this rule

- **Releases** are read from the cooldown (§ "How to reproduce"), so a swing that starts and releases in one tick is
  counted, whatever phase it came from.
- **Starts.** A damage-point-0 swing publishes no wind-up, so the trace inferred no start for it and
  `swings_started` would have read 0 for such units (WP4 makes the zealot one). A separate commit of WP2 adds
  `SwingEvent::StartedAndReleased`: a release whose previous phase was not a wind-up started its swing in the same
  tick, and counts as both. Test first (`a_swing_that_starts_and_releases_in_one_tick_counts_as_started`, red with 0
  starts against 4 releases). A whole-matrix rerun with that tool, compared with §2.1's `matrix.csv`, is
  bit-identical: the change affects counts only.
- **Remaining limit, documented and not handled.** A unit that dies in the tick its damage-point-0 swing starts and
  releases left no wind-up behind and publishes no cooldown, so that one shot is not counted
  (`BattleTrace::count_releases_of_the_fallen`). Inferring it would need the weapon's target choice and firing arc
  re-derived from the previous samples, which the trace does not mirror. It affects only the trace's counts, never
  an outcome, and only shots fired on a unit's last tick.
