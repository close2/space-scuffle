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

## 3. WP4: the zealot's weapon (Q0036)

**Commit.** The data commit of WP4 on the WP4 branch, on `main` at `49177b4` (WP0, WP1 and WP2 merged), after the
test-only commit `5a798a0` (D-1 item 5). `psi_grey`'s weapon takes StarCraft II's values, as the owner chose
(Q0036, option 2), from the 5.0.14 game data cited in the battle 33 investigation §1.5:

| Field | Before | After | Reference |
|---|---|---|---|
| `damage_point_ticks` | 4 | **0** | 0 (`liberty` WeaponData:150) |
| `period_ticks` | 22 | **21** | 1.2 Normal s = 0.857 Faster s = 21.43 ticks, nearest whole tick (D-3) |
| `max_range` | 0.3 | **0.1** | 0.1 (WeaponData) |
| `backswing_ticks` | 6 | **9** | 0.5 Normal s = 8.93 ticks (`core` default) |

With WP2's rule a damage point of 0 releases on the tick the swing starts, so the zealot's cycle is its period, 21
ticks (it was 22). Doc 07's roster row now gives range 0.1 (it said 0.2; period 21 already). The attributes
(Q0040) are WP5's and are not part of this step. Outputs under `~/scratch/combat_wp4/` on the development server
(`before/` is the state at `49177b4`, byte-identical to WP2's §2.1 `matrix.csv`; `after/` the committed state;
`field/` the scratch catalogs).

### 3.1 Whole matrix, 20 seeds

Compared with the state after WP2 (§2.1). **7 battles moved, exactly the plan's list (§5.1, WP4 step 5): 7, 8,
23a, 23b, 25a, 25b and 33. No other battle moved. No battle except 33 changed its winner on any seed**, and none of
the six others needed its expectation changed. Expectation failures: 0 on the seeds of record after battle 33's
re-recording (§3.2), 6 over all 20 seeds, all battle 33 (seeds 1, 5, 6, 12, 16: marine wins with 7 to 9 left,
above the new range; seed 13: a zealot win with 4 left). Before WP4 it was 0 and 2 (battle 33, seeds 1 and 6).

| Battle | Seeds moved | Survivors blue – red, before → after | End tick, before → after | Lead hull, before → after |
|---|---|---|---|---|
| 7_zealot_vs_roach | 20 | 1.00 – 0.00 → same | 253.3 → 240.9 | blue 12.00 → 12.00 |
| 8_zealot_vs_stalker | 20 | 1.00 – 0.00 → same | 253.8 → 241.5 | blue 51.00 → 57.00 |
| 23a_zealot_vs_6_lings | 19 | 0.00 – 2.10 → 0.00 – 2.00 | 266.2 → 269.2 | — |
| 23b_upgraded_zealot_vs_6_lings | 20 | 1.00 – 0.00 → same | 259.7 → 245.6 | blue 53.20 → 60.00 |
| 25a_2_marines_vs_zealot | 20 | 0.00 – 1.00 → same | 152.9 → 145.6 | red 61.75 → 62.00 |
| 25b_upgraded_marines_vs_zealot | 20 | 0.00 – 1.00 → same | 148.9 → 141.4 | red 23.50 → 32.00 (range 5 to 45) |
| 33_24_marines_vs_12_zealots | 20 | 7.65 – 0.20 → **3.80 – 1.05** | 421.1 → 457.5 | blue 38.60 (15) → 34.33 (3) |

- The zealot kills sooner everywhere it fights: its first strike lands on contact instead of 4 ticks later, and it
  strikes every 21 ticks instead of 22. Battles 7, 8, 23b, 25a and 25b end 7 to 14 ticks sooner with the same
  winner; the zealot keeps more life in 8 and 23b, and in 25b ends with 32 (all 20 seeds), inside its `red_lead_hp`
  range of 5 to 45. In 23a the zealot still loses, now always to 2 zerglings: on seeds 10 and 12 it kills
  a fourth before it falls (3 left before), and on the other seeds only the end tick moves (19 seeds' rows differ);
  inside `red: (1, 999)`.
- The plan's probe (§5.1, "Zealot weapon + attributes + regeneration rate") agrees on every battle but 33 to within
  1.5 ticks of end tick (7 242 → 240.9, 8 242 → 241.5, 23b 247 → 245.6, 25a 147 → 145.6, 25b 142 → 141.4); the
  probe kept a one-tick wind-up (§3.4).
- **Benchmarks** (`sc2_battle_benchmarks_tests.rs`, read with a throwaway print, not committed): test 2 (zealot
  against 2 marines) leaves the zealot 67 life (was 62; range 60 to 95); test 3 (against a marauder) 73 (was 73),
  over 198 ticks (was 209; at least 160); test 5 (four zerglings in turn) 52 life and shields (was 52; range 35 to
  80); six zerglings still win. Test 1 now reads the timing from the catalog (commit `5a798a0`): the swings land on
  ticks 0, 21 and 42.

The whole matrix after this step (means over the 20 seeds; lead hull as in §1.1):

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
| 7_zealot_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 240.9 | 12.00 (20) | — | 0 | 0 |
| 8_zealot_vs_stalker | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 241.5 | 57.00 (20) | — | 0 | 0 |
| 9a_hellion_vs_4_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 91.0 | 27.80 (20) | — | 0 | 0 |
| 9b_hellion_vs_roach | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 148.0 | — | 117.00 (20) | 0 | 0 |
| 10_2_hellions_vs_8_lings | 20 | 20 | 0 | 0 | 0 | 2.00 – 0.00 | 55.0 | 70.00 (20) | — | 0 | 0 |
| 11a_2_banelings_vs_5_marines | 20 | 0 | 9 | 0 | 11 | 0.00 – 2.25 | 9.2 | — | 10.00 (9) | 0 | 0 |
| 11b_2_banelings_vs_marauder | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 9.1 | — | 95.00 (20) | 0 | 0 |
| 12a_colossus_vs_6_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 78.0 | 243.80 (20) | — | 0 | 0 |
| 12b_colossus_vs_3_marauders | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 164.0 | — | 67.53 (19) | 0 | 0 |
| 12c_viking_vs_colossus | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 474.0 | 125.00 (20) | — | 0 | 0 |
| 13_storm_vs_held_lings | 20 | 0 | 0 | 0 | 20 | 0.00 – 0.00 | 70.0 | — | — | 0 | 0 |
| 14_thor_vs_ultralisk | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 187.0 | 37.00 (20) | — | 0 | 0 |
| 15_plated_ultralisk_vs_8_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 111.0 | 424.06 (20) | — | 0 | 0 |
| 16a_muta_vs_marine | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 115.0 | 74.24 (20) | — | 0 | 0 |
| 16b_muta_vs_2_marines | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 183.0 | — | 27.00 (11) | 0 | 0 |
| 17_void_ray_vs_muta | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 181.0 | 150.00 (20) | — | 0 | 0 |
| 18_void_ray_vs_corruptor | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 226.0 | 110.00 (20) | — | 0 | 0 |
| 19_viking_vs_brood_lord | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 293.0 | 125.00 (20) | — | 0 | 0 |
| 20_brood_lord_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 142.2 | 225.00 (20) | — | 0 | 0 |
| 21_lurker_vs_6_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 99.2 | 82.92 (20) | — | 0 | 0 |
| 22a_reaper_vs_2_workers | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 102.0 | 40.63 (20) | — | 0 | 0 |
| 22b_reaper_vs_stalker | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 178.0 | — | 80.00 (20) | 0 | 0 |
| 23a_zealot_vs_6_lings | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 269.2 | — | — | 0 | 0 |
| 23b_upgraded_zealot_vs_6_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 245.6 | 60.00 (20) | — | 0 | 0 |
| 24a_roach_vs_5_lings | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 296.6 | — | — | 0 | 0 |
| 24b_upgraded_roach_vs_5_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 255.6 | 85.00 (20) | — | 0 | 0 |
| 25a_2_marines_vs_zealot | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 145.6 | — | 62.00 (20) | 0 | 0 |
| 25b_upgraded_marines_vs_zealot | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 141.4 | — | 32.00 (20) | 0 | 0 |
| 26_100_lings_vs_6_colossi | 20 | 0 | 20 | 0 | 0 | 0.00 – 6.00 | 114.8 | — | 250.00 (20) | 0 | 0 |
| 27_100_lings_vs_8_ultralisks | 20 | 0 | 20 | 0 | 0 | 0.00 – 8.00 | 203.1 | — | 432.59 (20) | 0 | 0 |
| 28_100_lings_vs_10_immortals | 20 | 20 | 0 | 0 | 0 | 27.30 – 0.00 | 670.5 | 35.00 (3) | — | 0 | 0 |
| 29_100_lings_vs_12_stalkers | 20 | 20 | 0 | 0 | 0 | 71.50 – 0.00 | 440.4 | 34.41 (18) | — | 0 | 0 |
| 30_20_roaches_vs_10_immortals | 20 | 0 | 20 | 0 | 0 | 0.00 – 9.95 | 263.9 | — | 128.58 (19) | 0 | 0 |
| 31_15_marauders_vs_15_stalkers | 20 | 15 | 5 | 0 | 0 | 2.65 – 0.70 | 661.3 | 88.17 (6) | — | 0 | 0 |
| 32_30_marines_vs_15_banelings | 20 | 7 | 9 | 0 | 4 | 0.85 – 0.80 | 53.5 | 10.00 (6) | — | 0 | 0 |
| 33_24_marines_vs_12_zealots | 20 | 9 | 4 | 7 | 0 | 3.80 – 1.05 | 457.5 | 34.33 (3) | — | 0 | 6 |
| 34_20_hydras_vs_12_mutas | 20 | 20 | 0 | 0 | 0 | 19.10 – 0.00 | 192.6 | 90.00 (20) | — | 0 | 0 |
| 35_12_vikings_vs_6_colossi | 20 | 19 | 0 | 1 | 0 | 12.00 – 0.05 | 309.2 | 125.00 (20) | — | 0 | 0 |

### 3.2 Battle 33, 68 seeds, and its re-recorded expectation

| Measure | Before (§2.2) | After WP4 |
|---|---|---|
| Marine wins / zealot wins / unresolved at 500 ticks | 61 / 6 / 1 | **30 / 24 / 14** |
| Zealots left in their wins | 2 or 3 | 2 to 5 |
| Unresolved | seed 19, 3 – 1 | 14 seeds, 1 to 4 marines against 1 or 2 zealots |
| Marine survivors, mean (SD), median, range | 7.41 (3.34), 8, 0 to 14 | **3.09** (3.01), 3, 0 to 9 |
| Zealot survivors, mean, range | 0.25, 0 to 3 | 1.24, 0 to 5 |
| End tick, mean (SD), range | 420.4 (36.6), 351 to 500 | 458.1 (34.4), 386 to 500 |
| K over the first 5 s, mean (SD), range | 4.25 (0.40), 3.23 to 4.97 | 4.18 (0.32), 3.45 to 4.74 |
| Seeds of record 3203386110, 11, 22, 33 (marines – zealots) | 8–0, 6–0, 9–0, 13–0 | **0–3**, 6–0, **3–1**, 4–0 |

Zealot wins after WP4 (seed: zealots left): 3203386110: 3, 8: 2, 10: 3, 13: 4, 18: 2, 19: 2, 20: 2, 24: 2, 28: 3,
34: 4, 39: 4, 40: 2, 42: 4, 43: 2, 44: 2, 45: 2, 46: 2, 48: 2, 50: 4, 51: 3, 57: 2, 58: 5, 60: 3, 63: 2.

- **The expectation `blue: (4, 16), red: (0, 0)` fails on two seeds of record** (3203386110, a zealot win with 3
  left, and 22, unresolved at 3 – 1), as the plan foresaw (D-6, "one or two"). Per Q0044 (option 1) it is
  re-recorded to the smallest ranges that hold on the four seeds of record: **`blue: (0, 6), red: (0, 3)`**. The
  claim now reads "A close fight: unkited zealots and marines trade evenly, either side may win narrowly", and the
  RON comment above the battle (Q0041's) states this distribution and points here. Seeds and setup are unchanged.
- **What the range admits and rejects.** On the 20 matrix seeds it holds on 14 and fails on 6: five clear marine
  wins (7 to 9 left) and one zealot win with 4 left. So it fails if either side wins clearly, as Q0044 option 1 said.
- **Why the battle closes.** K, the zealots in contact over the first 5 s, hardly moves (4.25 → 4.18): as many
  zealots reach the marines as before. Each of them deals more: its first strike lands on contact instead of 4
  ticks later (a wind-up the marines' fire could cancel by killing it), and it strikes every 21 ticks instead of 22.
  The per-field runs (§3.3) show the damage point is the large part.

### 3.3 Each field alone (D-5 item 5)

Each value applied alone to a scratch copy of the pre-WP4 catalog (`matchup_trace --catalog-dir`), against the
pre-WP4 state. "Moved" counts battles whose rows differ on the 20 matrix seeds.

| Scratch catalog | Matrix battles moved | Battle 33, 20 seeds: survivors, failures of record / all (old expectation) | Battle 33, 68 seeds: marine wins / zealot wins / unresolved | Marine survivors, mean (SD) | Seeds of record |
|---|---|---|---|---|---|
| Before WP4 | — | 7.65 – 0.20, 0 / 2 | 61 / 6 / 1 | 7.41 (3.34) | 8–0, 6–0, 9–0, 13–0 |
| Damage point 0 | the 7 of §3.1 | 4.75 – 0.65, 3 / 8 | 35 / 17 / 16 | 3.82 (3.10) | 3–1, 0–3, 3–1, 4–0 |
| Period 21 | the 7 of §3.1 | 6.15 – 0.40, 0 / 3 | 49 / 9 / 10 | 5.62 (3.52) | 7–0, 6–0, 5–0, 11–0 |
| Range 0.1 | the 7 of §3.1; **battle 7 fails** (see below) | 8.45 – 0.00, 0 / 0 | 66 / 0 / 2 | 8.24 (2.47) | 8–0, 11–0, 7–0, 10–0 |
| Backswing 9 | 33 only (4 seeds) | 7.90 – 0.10, 0 / 1 | 62 / 5 / 1 | 7.29 (3.26) | 8–0, 6–0, 9–0, 13–0 |
| All four (= the commit) | the 7 of §3.1 | 3.80 – 1.05, 2 / 12 | 30 / 24 / 14 | 3.09 (3.01) | 0–3, 6–0, 3–1, 4–0 |

- The damage point is the large step (−3.59 marine survivors on 68 seeds), then the period (−1.79); the shorter
  range favours the marines (+0.83: fewer zealots fit round a marine), and the backswing is noise (−0.12). The
  investigation measured −2.82, −1.79, +0.82 and −0.12 for the same four (its §5.3, damage point 0 then still with
  the one-tick wait of the old rule): the period, range and backswing agree, and the damage point now weighs more
  because the strike lands on the contact tick (D-3).
- **Range 0.1 alone breaks battle 7** (zealot against roach): with the old 4-tick wind-up and the shorter reach,
  zealot and roach kill each other on the same tick on 8 of 20 seeds (both gone at tick 255) and the roach wins on 4,
  failing on 3 seeds of record. This is a property of that intermediate catalog only: with all four values (the
  commit) battle 7 is a zealot win on all 20 seeds, 12 ticks sooner than before.

### 3.4 Why battle 33 is closer than the plan's probe

The plan's probe (§5.2) predicted 38 / 13 / 17 and 4.07 marine survivors for the zealot's changes; the commit gives
30 / 24 / 14 and 3.09. The probe could not run WP2's rule and approximated the zealot as damage point 0 with period
20 on the old code, which held a one-tick wind-up before each strike (plan §5, "both proxies differ from the real
fixes by the one-tick wind-up"). That proxy is the same as damage point **1**, period 21 on today's code. A scratch
catalog with exactly that (damage point 1, period 21, range 0.1, backswing 9) reproduces the probe to the seed:
**38 / 13 / 17, 4.07 (3.15), seeds of record 3–0, 6–0, 3–1, 7–0**. So the whole difference is the one tick by which
the real zealot strikes earlier on contact, on every swing.

### 3.5 Golden replays

- **`golden_replay.ron`: re-recorded.** Its 12 `red_psi` zealots fight the marines. Checkpoints 1 to 4 (ticks 1 to
  200) are unchanged; the first differing checkpoint is **5 (ticks 201 to 250)**, and every later checkpoint (5 to
  20) changed.
- **`golden_replay_orders.ron`: unchanged**, against the plan's expectation (§1, WP4: "both golden replays"). Its
  four `red_ledge` zealots hold position on the high-ground lip and are shot down from below; no enemy comes within
  their reach, so no zealot strike lands in either run and all 60 checksums are identical. Both phase-order
  situations are still found by the test.
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`, as CI) reproduce both fixtures.
