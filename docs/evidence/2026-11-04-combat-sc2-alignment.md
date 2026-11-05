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

## 4. WP5: the zealot's attributes (Q0040)

**Commit.** The data commit of WP5 on `main` at `be5a476` (WP0, WP1, WP2 and WP4 merged). `psi_grey` is tagged
**Light, Biological** (`attributes: (bits: 5)`) instead of Light, Psionic (`bits: 17`), as in the StarCraft II
5.0.14 game data and Liquipedia (battle 33 investigation §1.5), the owner's choice (Q0040, option 2, following
Q0008). Outputs under `~/scratch/combat_wp56/` on the development server (`base/` the state at `be5a476`, whose
`matrix.csv` is byte-identical to WP4's §3.1 output; `wp5/` the committed state).

**Inertness, checked before the edit (plan WP5 step 1).**

- **Rules.** No code in `sim_core`, `sim_map` or `sim_types` reads `BIOLOGICAL` or `PSIONIC` for a unit. The only
  reads of a unit's attributes in the simulation are the Armored and Light damage bonuses (`combat.rs`), the
  Structure check of the threat priority (`compute_threat_priority` in `combat.rs`), and
  `target_flags.intersects(attributes)` in target acquisition and combat resolution (`target_acquisition.rs`,
  `combat_resolution.rs`). `catalog.rs` sets `PSIONIC` only on structures. `sim_fixtures` reads Light and Armored
  only. The client's Visual Lab (`lab_ui.rs`) displays the Biological tag, which is presentation.
- **Data.** Every weapon in `units.ron` and `structures.ron` has `target_flags: (bits: 255)`, so the intersection is
  non-empty for any unit with an attribute, and the zealot keeps Light; no catalog file has an ability or bonus
  that filters on attributes.
- **State.** Attributes are catalog data, not entity state, so the checksum does not hash them.

**Measured.**

- **Whole matrix, 20 seeds: bit-identical** to the state before (all 1000 rows of `matrix.csv`), with the tool built
  on the committed catalog and, before the edit, with a scratch catalog carrying only this change. Expectation
  failures stay 0 on the seeds of record and 6 over all seeds (battle 33, as in §3.1).
- **Battle 33, 68 seeds: bit-identical** to §3.2: `seeds.csv` and all 68 `seed_<n>_ticks.csv` files are
  byte-identical. Marine wins / zealot wins / unresolved 30 / 24 / 14, marine survivors 3.09 (3.01), K over the
  first 5 s 4.18 (0.32), seeds of record 0–3, 6–0, 3–1, 4–0, all unchanged.
- **Golden replays: both unchanged.** `golden_replay.ron` and `golden_replay_orders.ron` pass as recorded in debug,
  release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`, as CI).

The tag starts to matter only when an ability filters on Biological or Psionic units, as several StarCraft II
abilities do; none exists in the catalog today.

## 5. WP6: the marine's damage point and backswing (Q0038, step 2)

**Commit.** The data commit of WP6, after WP5 (§4), on `main` at `be5a476` (WP0, WP1, WP2 and WP4 merged).
`pulse_bot`'s ground and air weapons take StarCraft II's timing, the second step of the owner's answer to Q0038
(option 2; the rule step was WP1, `7353aad`):

| Field (both weapons) | Before | After | Reference |
|---|---|---|---|
| `damage_point_ticks` | 2 | **1** | 0.05 Normal s = 0.0357 Faster s = 0.89 ticks (battle 33 investigation §1.5) |
| `backswing_ticks` | 4 | **13** | 0.75 Normal s = 0.536 Faster s = 13.39 ticks (the same) |

With WP1's rule the backswing locks orders but never delays the next attack, so the marine's cycle stays its period
(15 ticks, 10 stimmed). Doc 07's roster row has no damage point or backswing column, so it needs no correction.
`stutter_step_tests.rs` passes untouched (it reads the swing from the catalog since WP1). Outputs under
`~/scratch/combat_wp56/` on the development server (`wp5/` the state before, `wp6/` the committed state, `field/`
the scratch catalogs).

### 5.1 Whole matrix, 20 seeds

Compared with the state after WP5 (§4, bit-identical to §3.1). **10 battles moved, all of them battles with
marines: 5a, 5b, 5c, 11a, 15, 16a, 16b, 21, 32 and 33.** The plan expected battle 33 within noise and "marine
battles may shift by a tick" (WP6 step 3); its probe (§5.1, marine column, which also carried the marine's speed
and Stimpack duration) had listed 5c, 16b, 21, 32 and 33 among them. **No battle except 32 and 33 changed its
winner on any seed, and none but 33 needed its expectation changed.** Expectation failures: 0 on the seeds of record
after battle 33's re-recording (§5.2), 3 over all 20 seeds, all battle 33 (seeds 5, 6 and 9: marine wins with 8 or 9
left, above the new range). After WP5 it was 0 and 6.

| Battle | Seeds moved | Survivors blue – red, before → after | End tick, before → after | Lead hull, before → after |
|---|---|---|---|---|
| 5a_hydra_vs_marine | 20 | 1.00 – 0.00 → same | 52.0 → 52.0 | blue 66.98 → 67.00 |
| 5b_hydra_vs_2_marines | 20 | 1.00 – 0.00 → same | 112.0 → 112.0 | blue 20.18 → 20.20 |
| 5c_hydra_vs_3_marines | 20 | 0.00 – 2.00 → same | 78.0 → 77.0 | red 40.33 (18) → 40.33 (18) |
| 11a_2_banelings_vs_5_marines | 1 | 0.00 – 2.25 → same | 9.2 → 9.1 | red 10.00 (9) → 10.00 (9) |
| 15_plated_ultralisk_vs_8_marines | 20 | 1.00 – 0.00 → same | 111.0 → 111.0 | blue 424.06 → 415.28 |
| 16a_muta_vs_marine | 20 | 1.00 – 0.00 → same | 115.0 → 115.0 | blue 74.24 → 74.26 |
| 16b_muta_vs_2_marines | 20 | 0.00 – 1.00 → same | 183.0 → 182.0 | red 27.00 (11) → 27.00 (11) |
| 21_lurker_vs_6_marines | 20 | 1.00 – 0.00 → same | 99.2 → 99.2 | blue 82.92 → 82.94 |
| 32_30_marines_vs_15_banelings | 14 | 0.85 – 0.80 → 1.10 – 0.80 | 53.5 → 52.9 | blue 10.00 (6) → 18.75 (4) |
| 33_24_marines_vs_12_zealots | 19 | 3.80 – 1.05 → **4.35 – 0.85** | 457.5 → 449.5 | blue 34.33 (3) → 39.00 (8) |

- **Each marine shot lands one tick sooner.** Where the marines lose to zerg units (5a, 5b, 16a, 21) the winner,
  the counts and the end tick stay, and the zerg survivor ends with 0.02 more life: the marines' last shot lands a
  tick sooner, so it regenerates for one more tick (`hp_regen_per_tick: 0.02`). Where the marines win (5c, 16b)
  the fight ends a tick sooner. In 15 the eight marines deal 16 more damage to the ultralisk before they fall on 11
  of the 20 seeds (the other 9: one more tick of regeneration), so its mean life left goes from 424.06 to 415.28.
  In 11a one seed ends a tick sooner.
- **Battle 32** (30 marines against 15 banelings) is a coin flip by design (`blue: (0, 15), red: (0, 999)`): on 4 of
  its 20 seeds the outcome changes between a marine win and both sides gone (seeds 1, 13 and 22 now both gone,
  seed 8 now a marine win), so blue wins 7 → 5 and both gone 4 → 6, with more marines left in the wins.
- **Battles 25a and 25b do not move** (their `matrix.csv` rows are identical). A trace of 25b on its seeds of record
  shows each marine shot landing one tick sooner, with the same 22 shots in all and the same end tick, so the end
  state is the same: **25b's zealot ends with 32 life on all 20 seeds, inside its 5 to 45 range**. Before WP1's rule
  the investigation measured 62 with backswing 13 (its §5.7, on the zealot of that time).
- **The backswing alone is inert** on shipped data (§5.3): the whole matrix and battle 33 are bit-identical with
  backswing 13 and damage point 2. After WP1 a backswing never delays the next attack; it is a lock on orders, and
  no battle of the matrix turns on an order given during one.

The whole matrix after this step (means over the 20 seeds; lead hull as in §1.1):

| Battle | Seeds | Blue wins | Red wins | Unresolved | Both gone | Survivors blue – red | End tick | Blue lead hull | Red lead hull | Failures of record | Failures, all seeds |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1a_stalker_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 281.0 | 5.00 (20) | — | 0 | 0 |
| 1b_stalker_vs_2_roaches | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 159.8 | — | 106.75 (20) | 0 | 0 |
| 2a_immortal_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 61.0 | 200.00 (20) | — | 0 | 0 |
| 2b_immortal_vs_2_roaches | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 145.0 | 174.00 (20) | — | 0 | 0 |
| 3_immortal_vs_tank | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 89.0 | 200.00 (20) | — | 0 | 0 |
| 4_marauder_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 190.1 | 20.00 (20) | — | 0 | 0 |
| 5a_hydra_vs_marine | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 52.0 | 67.00 (20) | — | 0 | 0 |
| 5b_hydra_vs_2_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 112.0 | 20.20 (20) | — | 0 | 0 |
| 5c_hydra_vs_3_marines | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 77.0 | — | 40.33 (18) | 0 | 0 |
| 6a_roach_vs_hydra | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 149.7 | 35.00 (20) | — | 0 | 0 |
| 6b_2_roaches_vs_hydra | 20 | 20 | 0 | 0 | 0 | 2.00 – 0.00 | 65.9 | 118.60 (20) | — | 0 | 0 |
| 6c_roach_vs_2_hydras | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 96.3 | — | 61.98 (20) | 0 | 0 |
| 7_zealot_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 240.9 | 12.00 (20) | — | 0 | 0 |
| 8_zealot_vs_stalker | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 241.5 | 57.00 (20) | — | 0 | 0 |
| 9a_hellion_vs_4_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 91.0 | 27.80 (20) | — | 0 | 0 |
| 9b_hellion_vs_roach | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 148.0 | — | 117.00 (20) | 0 | 0 |
| 10_2_hellions_vs_8_lings | 20 | 20 | 0 | 0 | 0 | 2.00 – 0.00 | 55.0 | 70.00 (20) | — | 0 | 0 |
| 11a_2_banelings_vs_5_marines | 20 | 0 | 9 | 0 | 11 | 0.00 – 2.25 | 9.1 | — | 10.00 (9) | 0 | 0 |
| 11b_2_banelings_vs_marauder | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 9.1 | — | 95.00 (20) | 0 | 0 |
| 12a_colossus_vs_6_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 78.0 | 243.80 (20) | — | 0 | 0 |
| 12b_colossus_vs_3_marauders | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 164.0 | — | 67.53 (19) | 0 | 0 |
| 12c_viking_vs_colossus | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 474.0 | 125.00 (20) | — | 0 | 0 |
| 13_storm_vs_held_lings | 20 | 0 | 0 | 0 | 20 | 0.00 – 0.00 | 70.0 | — | — | 0 | 0 |
| 14_thor_vs_ultralisk | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 187.0 | 37.00 (20) | — | 0 | 0 |
| 15_plated_ultralisk_vs_8_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 111.0 | 415.28 (20) | — | 0 | 0 |
| 16a_muta_vs_marine | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 115.0 | 74.26 (20) | — | 0 | 0 |
| 16b_muta_vs_2_marines | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 182.0 | — | 27.00 (11) | 0 | 0 |
| 17_void_ray_vs_muta | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 181.0 | 150.00 (20) | — | 0 | 0 |
| 18_void_ray_vs_corruptor | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 226.0 | 110.00 (20) | — | 0 | 0 |
| 19_viking_vs_brood_lord | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 293.0 | 125.00 (20) | — | 0 | 0 |
| 20_brood_lord_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 142.2 | 225.00 (20) | — | 0 | 0 |
| 21_lurker_vs_6_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 99.2 | 82.94 (20) | — | 0 | 0 |
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
| 32_30_marines_vs_15_banelings | 20 | 5 | 9 | 0 | 6 | 1.10 – 0.80 | 52.9 | 18.75 (4) | — | 0 | 0 |
| 33_24_marines_vs_12_zealots | 20 | 11 | 4 | 5 | 0 | 4.35 – 0.85 | 449.5 | 39.00 (8) | — | 0 | 3 |
| 34_20_hydras_vs_12_mutas | 20 | 20 | 0 | 0 | 0 | 19.10 – 0.00 | 192.6 | 90.00 (20) | — | 0 | 0 |
| 35_12_vikings_vs_6_colossi | 20 | 19 | 0 | 1 | 0 | 12.00 – 0.05 | 309.2 | 125.00 (20) | — | 0 | 0 |

### 5.2 Battle 33, 68 seeds, and its re-recorded expectation

| Measure | Before (§4, = §3.2) | After WP6 |
|---|---|---|
| Marine wins / zealot wins / unresolved at 500 ticks | 30 / 24 / 14 | **44 / 12 / 12** |
| Zealots left in their wins | 2 to 5 | 2 to 5 |
| Unresolved | 14 seeds, 1 to 4 marines against 1 or 2 zealots | 12 seeds, 1 to 3 marines against 1 or 2 zealots |
| Marine survivors, mean (SD), median, range | 3.09 (3.01), 3, 0 to 9 | **4.56** (3.18), 4.5, 0 to 11 |
| Zealot survivors, mean, range | 1.24, 0 to 5 | 0.74, 0 to 5 |
| End tick, mean (SD), range | 458.1 (34.4), 386 to 500 | 448.8 (37.7), 368 to 500 |
| K over the first 5 s, mean (SD), range | 4.18 (0.32), 3.45 to 4.74 | 4.17 (0.32), 3.26 to 4.75 |
| Seeds of record 3203386110, 11, 22, 33 (marines – zealots) | 0–3, 6–0, 3–1, 4–0 | **7–0**, **7–0**, **0–3**, 6–0 |

Zealot wins after WP6 (seed: zealots left): 22: 3, 2: 2, 12: 3, 13: 3, 20: 3, 44: 2, 45: 3, 51: 3, 53: 4, 57: 2,
60: 2, 63: 5.

- **The expectation `blue: (0, 6), red: (0, 3)` fails on two seeds of record** (3203386110 and 11, marine wins with 7
  left). Per Q0044 (option 1, plan D-6) it is re-recorded to the smallest ranges that hold on the four seeds of
  record: **`blue: (0, 7), red: (0, 3)`**. The claim ("A close fight: unkited zealots and marines trade evenly,
  either side may win narrowly") still holds and is unchanged; the RON comment above the battle now states this
  distribution and points here. Seeds and setup are unchanged. On the 20 matrix seeds the new range holds on 17 and
  fails on 3 (marine wins with 8 or 9 left).
- **More than the plan expected.** The plan expected battle 33 "within noise" (WP6 step 3), from the investigation's
  +0.09 marine survivors (SE 0.39) for the same two values. Measured here: **+1.47 (paired SE 0.41)**. The
  investigation measured on the zealot of that time (damage point 4, period 22), against which the marines won 61 of
  68; since WP4 the fight is close, so a small gain for the marines decides more seeds. The gain is the damage point
  alone (§5.3). Per seed, paired over the 68: the marines' cancelled swings fall from 6.53 to 3.84 (−2.69, SE 0.37),
  they release 7.8 more shots (335.2 → 343.0, SE 2.4), and the zealots swing 3.5 times fewer (64.1 → 60.6, SE 1.0);
  first contact is unchanged (77.6 and 116.3 ticks). A marine's wind-up is now one tick, so fewer of its swings are
  cancelled by a target that dies or leaves the band, and each lands one tick sooner, which matters most while the
  zealots close in.

### 5.3 Each field alone (D-5 item 5)

Each value applied alone to a scratch copy of the post-WP5 catalog (`matchup_trace --catalog-dir`), against the
post-WP5 state. "Moved" counts battles whose rows differ on the 20 matrix seeds; failures are against the old
expectation `blue: (0, 6), red: (0, 3)`.

| Scratch catalog | Matrix battles moved | Battle 33, 20 seeds: survivors, failures of record / all | Battle 33, 68 seeds: marine wins / zealot wins / unresolved | Marine survivors, mean (SD) | Seeds of record |
|---|---|---|---|---|---|
| Before WP6 | — | 3.80 – 1.05, 0 / 6 | 30 / 24 / 14 | 3.09 (3.01) | 0–3, 6–0, 3–1, 4–0 |
| Damage point 1 | the 10 of §5.1 | 4.35 – 0.85, 2 / 6 | 44 / 12 / 12 | 4.56 (3.18) | 7–0, 7–0, 0–3, 6–0 |
| Backswing 13 | none (bit-identical) | 3.80 – 1.05, 0 / 6 | 30 / 24 / 14 | 3.09 (3.01) | 0–3, 6–0, 3–1, 4–0 |
| Both (= the commit) | the 10 of §5.1 | 4.35 – 0.85, 2 / 6 | 44 / 12 / 12 | 4.56 (3.18) | 7–0, 7–0, 0–3, 6–0 |

The damage point is the whole effect: the "both" catalog's `matrix.csv` is byte-identical to the damage-point-only
one, and so is its battle 33 output. The committed catalog reproduces the "both" scratch catalog in every outcome
column (only battle 33's `expectation met` column differs, by the re-recording).

### 5.4 Golden replays

- **`golden_replay.ron`: re-recorded.** Its 16 `blue_infantry` marines fight. Checkpoints 1 to 3 (ticks 1 to 150)
  are unchanged; the first differing checkpoint is **4 (ticks 151 to 200)**, and every later checkpoint (4 to 20)
  changed.
- **`golden_replay_orders.ron`: re-recorded.** Its marines (`blue_line`, `blue_column`) shoot the zealots on the
  ledge. Checkpoints 1 to 6 (ticks 1 to 60) are unchanged; the first differing checkpoint is **7 (ticks 61 to
  70)**; checkpoints 7 to 30 changed, and from checkpoint 31 (ticks 301 to 310) on the checksums equal the old list
  again: the ledge fight is over and no state the marine's timing touched remains. Both phase-order situations the
  fixture exists for are still found by the test.
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`, as CI) reproduce both fixtures.

## 6. WP7: the marine's speed (Q0039)

**Commit.** The data commit of WP7, on `main` at `95456b3` (WP0 to WP6 merged; the commits after `317dcc3` change
no simulation code or catalog data, and the tool built there reproduces §5's `matrix.csv` byte for byte).
`pulse_bot` walks at StarCraft II's speed, converted from Normal to Faster time, one of the four values of the
owner's answer to Q0039 (option 3):

| Field | Before | After | Reference |
|---|---|---|---|
| `max_speed` | 0.09 | **0.126** | 2.25 Normal = 3.15 Faster per s = 0.126 per tick (`liberty` UnitData:4821; audit row, battle 33 investigation §1.5) |
| `acceleration`, `deceleration` | 0.09 | **0.126** | the catalog's convention (they copy `max_speed`); parsed and unused by the simulation (audit class E) |

Doc 07's roster row already says 0.1260. Outputs under `~/scratch/combat_wp7/` on the development server (`base/`
the state before, `wp7/` the edit, `wp7c/` the committed state with battle 33 re-recorded).

### 6.1 Whole matrix, 20 seeds

Compared with the state after WP6 (§5.1). **5 battles moved: 12a, 21, 25a, 32 and 33.** The plan expected 5c, 12a,
16b, 21, 32 and 33 (WP7 step 2), from a probe that combined WP6 and WP7: **5c and 16b did not move here** (they
moved with WP6's damage point, §5.1), and **25a moved although the plan did not list it** (accepted by the
orchestrator as the same cause; see below). **No winner changed except in battles 32 and 33, and none but 33 needed
its expectation changed.** Expectation failures: 0 on the seeds of record after battle 33's re-recording (§6.2), 7
over all 20 seeds, all battle 33. After WP6 it was 0 and 3.

| Battle | Seeds moved | Survivors blue – red, before → after | End tick, before → after | Lead hull, before → after |
|---|---|---|---|---|
| 12a_colossus_vs_6_marines | 9 | 1.00 – 0.00 → same | 78.0 → 80.7 | blue 243.80 → 238.15 |
| 21_lurker_vs_6_marines | 18 | 1.00 – 0.00 → same | 99.2 → 106.4 | blue 82.94 → 71.59 |
| 25a_2_marines_vs_zealot | 11 | 0.00 – 1.00 → same | 145.6 → 145.1 | red 62.00 → 62.00 |
| 32_30_marines_vs_15_banelings | 6 | 1.10 – 0.80 → 0.90 – 0.90 | 52.9 → 52.8 | blue 18.75 (4) → 18.75 (4) |
| 33_24_marines_vs_12_zealots | 20 | 4.35 – 0.85 → **4.50 – 0.60** | 449.5 → 440.8 | blue 39.00 (8) → 36.27 (11) |

- **The marines close the distance sooner.** Against the colossus (12a) and the lurker (21), which outrange them, the
  marines spend fewer ticks walking under fire, so they deal more damage before they fall: the colossus ends with
  5.65 less life on average (9 seeds moved; on 2 the fight lasts one colossus cycle longer), the lurker with 11.35
  less (18 seeds; on 3 the fight lasts one or two lurker cycles longer, on 1 one shorter). The winner and the survivor counts are
  unchanged.
- **Battle 25a** (two marines against a zealot): on 11 of the 20 seeds the fight ends one tick sooner (146 → 145 on
  10 seeds, 145 → 144 on one). The zealot wins on every seed and ends with 62 life, as before; the expectation
  holds. The faster marines meet the zealot a tick earlier. **Battle 25b does not move** (its rows are identical):
  its marines are stimmed and the fight is decided at contact.
- **Battle 32** (30 marines against 15 banelings) is a coin flip by design (`blue: (0, 15), red: (0, 999)`): on 6
  seeds the outcome changes (seed 1 both gone → marine win, 12 marine win → baneling win, 8 marine win → both gone,
  3 baneling win → both gone; on seeds 9 and 22 only the counts or the end tick change), so blue wins 5 → 4,
  baneling wins stay 9 and both gone 6 → 7.
- **Battle 33** changes winner on 10 of the 20 seeds, both ways; §6.2 has the distribution.

The whole matrix after this step (means over the 20 seeds; lead hull as in §1.1):

| Battle | Seeds | Blue wins | Red wins | Unresolved | Both gone | Survivors blue – red | End tick | Blue lead hull | Red lead hull | Failures of record | Failures, all seeds |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1a_stalker_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 281.0 | 5.00 (20) | — | 0 | 0 |
| 1b_stalker_vs_2_roaches | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 159.8 | — | 106.75 (20) | 0 | 0 |
| 2a_immortal_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 61.0 | 200.00 (20) | — | 0 | 0 |
| 2b_immortal_vs_2_roaches | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 145.0 | 174.00 (20) | — | 0 | 0 |
| 3_immortal_vs_tank | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 89.0 | 200.00 (20) | — | 0 | 0 |
| 4_marauder_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 190.1 | 20.00 (20) | — | 0 | 0 |
| 5a_hydra_vs_marine | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 52.0 | 67.00 (20) | — | 0 | 0 |
| 5b_hydra_vs_2_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 112.0 | 20.20 (20) | — | 0 | 0 |
| 5c_hydra_vs_3_marines | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 77.0 | — | 40.33 (18) | 0 | 0 |
| 6a_roach_vs_hydra | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 149.7 | 35.00 (20) | — | 0 | 0 |
| 6b_2_roaches_vs_hydra | 20 | 20 | 0 | 0 | 0 | 2.00 – 0.00 | 65.9 | 118.60 (20) | — | 0 | 0 |
| 6c_roach_vs_2_hydras | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 96.3 | — | 61.98 (20) | 0 | 0 |
| 7_zealot_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 240.9 | 12.00 (20) | — | 0 | 0 |
| 8_zealot_vs_stalker | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 241.5 | 57.00 (20) | — | 0 | 0 |
| 9a_hellion_vs_4_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 91.0 | 27.80 (20) | — | 0 | 0 |
| 9b_hellion_vs_roach | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 148.0 | — | 117.00 (20) | 0 | 0 |
| 10_2_hellions_vs_8_lings | 20 | 20 | 0 | 0 | 0 | 2.00 – 0.00 | 55.0 | 70.00 (20) | — | 0 | 0 |
| 11a_2_banelings_vs_5_marines | 20 | 0 | 9 | 0 | 11 | 0.00 – 2.25 | 9.1 | — | 10.00 (9) | 0 | 0 |
| 11b_2_banelings_vs_marauder | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 9.1 | — | 95.00 (20) | 0 | 0 |
| 12a_colossus_vs_6_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 80.7 | 238.15 (20) | — | 0 | 0 |
| 12b_colossus_vs_3_marauders | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 164.0 | — | 67.53 (19) | 0 | 0 |
| 12c_viking_vs_colossus | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 474.0 | 125.00 (20) | — | 0 | 0 |
| 13_storm_vs_held_lings | 20 | 0 | 0 | 0 | 20 | 0.00 – 0.00 | 70.0 | — | — | 0 | 0 |
| 14_thor_vs_ultralisk | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 187.0 | 37.00 (20) | — | 0 | 0 |
| 15_plated_ultralisk_vs_8_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 111.0 | 415.28 (20) | — | 0 | 0 |
| 16a_muta_vs_marine | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 115.0 | 74.26 (20) | — | 0 | 0 |
| 16b_muta_vs_2_marines | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 182.0 | — | 27.00 (11) | 0 | 0 |
| 17_void_ray_vs_muta | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 181.0 | 150.00 (20) | — | 0 | 0 |
| 18_void_ray_vs_corruptor | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 226.0 | 110.00 (20) | — | 0 | 0 |
| 19_viking_vs_brood_lord | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 293.0 | 125.00 (20) | — | 0 | 0 |
| 20_brood_lord_vs_roach | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 142.2 | 225.00 (20) | — | 0 | 0 |
| 21_lurker_vs_6_marines | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 106.4 | 71.59 (20) | — | 0 | 0 |
| 22a_reaper_vs_2_workers | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 102.0 | 40.63 (20) | — | 0 | 0 |
| 22b_reaper_vs_stalker | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 178.0 | — | 80.00 (20) | 0 | 0 |
| 23a_zealot_vs_6_lings | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 269.2 | — | — | 0 | 0 |
| 23b_upgraded_zealot_vs_6_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 245.6 | 60.00 (20) | — | 0 | 0 |
| 24a_roach_vs_5_lings | 20 | 0 | 20 | 0 | 0 | 0.00 – 2.00 | 296.6 | — | — | 0 | 0 |
| 24b_upgraded_roach_vs_5_lings | 20 | 20 | 0 | 0 | 0 | 1.00 – 0.00 | 255.6 | 85.00 (20) | — | 0 | 0 |
| 25a_2_marines_vs_zealot | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 145.1 | — | 62.00 (20) | 0 | 0 |
| 25b_upgraded_marines_vs_zealot | 20 | 0 | 20 | 0 | 0 | 0.00 – 1.00 | 141.4 | — | 32.00 (20) | 0 | 0 |
| 26_100_lings_vs_6_colossi | 20 | 0 | 20 | 0 | 0 | 0.00 – 6.00 | 114.8 | — | 250.00 (20) | 0 | 0 |
| 27_100_lings_vs_8_ultralisks | 20 | 0 | 20 | 0 | 0 | 0.00 – 8.00 | 203.1 | — | 432.59 (20) | 0 | 0 |
| 28_100_lings_vs_10_immortals | 20 | 20 | 0 | 0 | 0 | 27.30 – 0.00 | 670.5 | 35.00 (3) | — | 0 | 0 |
| 29_100_lings_vs_12_stalkers | 20 | 20 | 0 | 0 | 0 | 71.50 – 0.00 | 440.4 | 34.41 (18) | — | 0 | 0 |
| 30_20_roaches_vs_10_immortals | 20 | 0 | 20 | 0 | 0 | 0.00 – 9.95 | 263.9 | — | 128.58 (19) | 0 | 0 |
| 31_15_marauders_vs_15_stalkers | 20 | 15 | 5 | 0 | 0 | 2.65 – 0.70 | 661.3 | 88.17 (6) | — | 0 | 0 |
| 32_30_marines_vs_15_banelings | 20 | 4 | 9 | 0 | 7 | 0.90 – 0.90 | 52.8 | 18.75 (4) | — | 0 | 0 |
| 33_24_marines_vs_12_zealots | 20 | 15 | 4 | 1 | 0 | 4.50 – 0.60 | 440.8 | 36.27 (11) | — | 0 | 7 |
| 34_20_hydras_vs_12_mutas | 20 | 20 | 0 | 0 | 0 | 19.10 – 0.00 | 192.6 | 90.00 (20) | — | 0 | 0 |
| 35_12_vikings_vs_6_colossi | 20 | 19 | 0 | 1 | 0 | 12.00 – 0.05 | 309.2 | 125.00 (20) | — | 0 | 0 |

### 6.2 Battle 33, 68 seeds, and its re-recorded expectation

| Measure | Before (§5.2) | After WP7 |
|---|---|---|
| Marine wins / zealot wins / unresolved at 500 ticks | 44 / 12 / 12 | **42 / 18 / 8** |
| Zealots left in their wins | 2 to 5 | 2 to 4 |
| Unresolved | 12 seeds, 1 to 3 marines against 1 or 2 zealots | 8 seeds, 1 to 3 marines against 1 or 2 zealots |
| Marine survivors, mean (SD), median, range | 4.56 (3.18), 4.5, 0 to 11 | **4.03** (3.26), 4, 0 to 11 |
| Zealot survivors, mean, range | 0.74, 0 to 5 | 0.84, 0 to 4 |
| End tick, mean (SD), range | 448.8 (37.7), 368 to 500 | 441.2 (39.2), 370 to 500 |
| K over the first 5 s, mean (SD), range | 4.17 (0.32), 3.26 to 4.75 | 4.16 (0.32), 3.34 to 4.90 |
| First contact, marines / zealots (mean tick) | 77.6 / 116.4 | 66.6 / 105.4 |
| Seeds of record 3203386110, 11, 22, 33 (marines – zealots) | 7–0, 7–0, 0–3, 6–0 | 7–0, **3–0**, **9–0**, 6–0 |

Zealot wins after WP7 (seed: zealots left): 6: 3, 8: 2, 10: 4, 14: 2, 19: 3, 30: 2, 34: 3, 35: 2, 36: 4, 37: 2,
41: 2, 42: 3, 45: 4, 46: 2, 47: 3, 60: 2, 66: 2, 67: 2.

- **The expectation `blue: (0, 7), red: (0, 3)` fails on seed 22** (a marine win with 9 left). Per Q0044 (option 1,
  plan D-6) it is re-recorded to the smallest ranges that hold on the four seeds of record: **`blue: (0, 9), red:
  (0, 0)`**. All four seeds of record are marine wins now, so the range admits only marine wins of up to 9 on them.
  The claim ("A close fight: … either side may win narrowly") is unchanged: on 68 seeds the zealots win 18. The RON
  comment above the battle states this distribution and points here. Seeds and setup are unchanged. On the 20 matrix
  seeds the new range holds on 13 and fails on 7 (zealot wins on seeds 6, 8, 10 and 14, the unresolved seed 3, and
  marine wins with 10 and 11 left on seeds 5 and 13).
- **Paired over the 68 seeds**, marine survivors change by −0.53 (SE 0.45), zealot survivors by +0.10 (SE 0.19), and
  the end tick by −7.6 (SE 5.0): within noise of no change in the mean, while the outcome of single seeds moves a
  lot. Both sides meet 11 ticks sooner (marines' first contact −11.01, SE 0.03); the marines release 1.1 fewer shots
  (SE 2.4) and the zealots swing 0.9 more times (SE 1.1). The marines walk into the zealots instead of waiting
  for them, so contact starts sooner.

### 6.3 Golden replays

- **`golden_replay.ron`: re-recorded.** Its 16 `blue_infantry` marines start moving at tick 5, so every checkpoint
  (1 to 20) changed, from **checkpoint 1 (ticks 1 to 50)**.
- **`golden_replay_orders.ron`: re-recorded.** Checkpoints 1 and 2 (ticks 1 to 20) are unchanged; from **checkpoint
  3 (ticks 21 to 30)**, the first after the marines' move order at tick 20, all 58 later checkpoints changed. Both
  phase-order situations the fixture exists for are still found by the test. The fixture's header notes that the
  ticks named in its scenario text are from the first recording.
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`, as CI) reproduce both fixtures.

### 6.4 Tests restated before this step, and finding M-19

Three movement tests measured a march over a tick count that only made sense at 0.09; they failed at 0.126 and
were restated test-only in the commit before this one (plan D-1 item 5), each green on both catalogs:
`test_move_command_units_fan_out_and_surround_target_point` (march 7.2 world units instead of 80 ticks; at 0.126
the spread was 2.30 < 3), `test_live_battle_scenario_dual_ramp_traversal` (each unit judged on its first tick on the
enemy plateau, as the reverse march already was; at 0.126 marines arrived early and were lost there by tick 980), and
`battlefield_bubble_trajectories_are_smooth_without_micro_jitter` (window of 16.2 world units for the fastest
combatant instead of 180 ticks; thresholds unchanged). A fourth, in the client, was restated in its own test-only
commit for the same reason: `sync_steady_state_tests::combat_frames_reuse_effect_materials` required 100 combat
events in one window placed 120 frames after first contact; at 0.126 that window falls into a lull between two waves
(77 events; 170 in the next window), so the test now takes the first of up to four consecutive windows that holds
100 events and applies the material rules to every window it judges (160 events in the first window on the old
catalog).

The last one hid a real defect, recorded as **M-19** in
[`2026-11-01-codebase-remediation`](../plans/2026-11-01-codebase-remediation.md) §4.2: on `showcase_canyon` with
loadout `battle_automaton` (seed 1592639215, Player 0's 8 `pulse_bot` and 2 `rhino_beetle` attack-moving to (52.5,
52.5)), the marines jostle in a crowd near (28 to 30, 10 to 12), reversing heading by 8° to 175° on consecutive ticks
at speeds of 0.01 to 0.09. **It is not a mixed-speed effect:** with every combatant at 0.09 (the catalog before this
step) it happens too, from tick 216, just past the test's old 180-tick window; at 0.126 the group reaches the crowd
at tick 152. A per-tick trace at 0.126 over ticks 1 to 180 showed 16 marine zig-zags (entities 5 to 9, ticks 152 to
170) and one `rhino_beetle` zig-zag (entity 13, tick 179). Reproduction: set `JITTER_MARCH_DISTANCE` in
`crates/sim_core/tests/collision_and_immobility_tests.rs` to `36.0` (400 ticks at 0.09) and run

```bash
cargo test --locked -p sim_core --test collision_and_immobility_tests battlefield
```

which fails with `Units must have zero sign-reversing velocity zig-zag oscillations (> 8 deg) while marching (got
40)` on the catalog before this step (zig-zags in ticks 216 to 287, all `pulse_bot`) and `(got 20)` on this step's
catalog (window 286 ticks). The finding is open, for a later movement plan.

### 6.5 Gate note: a GPU test near its threshold

In this step's first `cargo test --workspace` run, `client` `skirmish_match_tests::
the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window` failed on the physical GPU (radv): a crossed ring
changed by 66.57 where it needed more than 3 × 23.47 (the largest change of a ring built in both shots). Rerun alone
on the same build it passed (67.10 against 3 × 20.90). The test drives workers to raise a foundation and involves no
`pulse_bot`; it is a margin flake unrelated to this step, reported and left unchanged.

## 7. WP8: the Stimpack's duration (Q0039)

**Commit.** The data commit of WP8, after WP7 (§6). `pulse_bot`'s Overclock lasts StarCraft II's Stimpack duration,
converted from Normal to Faster time, the second of the four values of Q0039 (option 3):

| Field | Before | After | Reference |
|---|---|---|---|
| `pulse_bot` `capabilities.overclock.duration_ticks` | 250 | **268** | `Stimpack` Duration 15 Normal s = 10.71 Faster s = 267.9 ticks (`liberty` BehaviorData:684, audit row; battle 33 investigation §1.6) |

`rhino_beetle`'s 20-life Overclock is not touched; WP15 removes it and gives `shock_golem` its own at 268 (plan
D-9). Doc 08's `overclock_stim` row said "for 178 ticks (7.1s)" and now says 268 ticks (10.7 s). Outputs under
`~/scratch/combat_wp7/wp8/` on the development server.

- **Whole matrix, 20 seeds: bit-identical** to the state after WP7 (all 1000 rows of `matrix.csv`), as the plan
  expected: no matrix battle has a stim last past 250 ticks. Expectation failures stay 0 on the seeds of record and 7
  over all seeds (battle 33, §6.1).
- **Battle 33, 68 seeds: bit-identical** to §6.2 (`seeds.csv` and all 68 `seed_<n>_ticks.csv` files byte-identical;
  nobody stims in it).
- **`golden_replay.ron`: re-recorded**, but not where the plan expected. The plan expected changes after tick 510
  (stim at tick 260 plus 250). The Overclock's remaining ticks are part of the hashed world state (`ActiveBuff` in
  `checksum/encoding.rs`), so the checksum differs from the stim on: **checkpoints 6 and 7 (ticks 251 to 350)
  changed, and only they.** From checkpoint 8 (ticks 351 to 400) on, the checksums equal WP7's again: by tick 400 no
  unit carries the Overclock (the stimmed marines have died), so the 18 extra ticks never take effect in this replay.
- **`golden_replay_orders.ron`: unchanged** (nobody stims in it).
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`) reproduce both fixtures.
- **Not changed:** `Entity::apply_stimpack` in `crates/sim_core/src/entity.rs`, the legacy path that an immediate
  `UseStim` given straight to an entity (not through a `PlayerCommand`) takes, still applies a hard-coded 10-life,
  250-tick Overclock. A `UseStim` player command never reaches it (`world/commands.rs` routes it to `apply_stim`,
  which uses the catalog's Overclock and the unlock); only tests that call `Entity::issue_order` directly do. It is
  outside this data step and is reported for a later clean-up.

## 8. WP15: the Overclock moves to the Marauder analogue (Q0045 3b)

**Commit.** The data commit of WP15, after WP8 (§7). The 20-life Overclock leaves `rhino_beetle` (the Roach, which
has no Stimpack in StarCraft II) and `shock_golem` (the Marauder) gets its own, the owner's answer 3(b) to Q0045
(plan D-9). Each value was checked against the 5.0.14 game data (`liberty` layer; no later layer overrides any of
them):

| Field | `rhino_beetle` before → after | `shock_golem` before → after | Reference (`StimpackMarauder`) |
|---|---|---|---|
| `capabilities.overclock` | 20 life, 250 ticks, ×1.5, ×1.5 → **None** | None → **20 life, 268 ticks, ×1.5, ×1.5** | life cost 20 (AbilData:988), Duration 15 Normal s = 267.9 ticks (BehaviorData:604), MoveSpeedMultiplier and AttackSpeedMultiplier 1.5 (BehaviorData:605) |
| `overclock_stim` effects (`upgrades.ron`) | unlocks `pulse_bot` | **also unlocks `shock_golem`** | one research for both: the `Stimpack` and `StimpackMarauder` commands share `Requirements="UseStimpack"` (AbilData:1236 and :994) |

Doc 07's `shock_golem` row now names the Overclock (the `rhino_beetle` row never did). Doc 08's `overclock_stim` row
already named `pulse_bot` and `shock_golem` (line 175) and needed no change. Outputs under `~/scratch/combat_wp7/wp15/`
on the development server.

**Tests first (plan WP15 steps 1 and 2).** No test stims `rhino_beetle` or asserts its overclock-readiness: the
tests that name it use it as a target or a body (`combat_correctness_tests.rs`, `spells_and_abilities_tests.rs`,
`determinism_lockstep_tests.rs`, which stims entities 1 to 16, its marines), and the client's battle HUD and card
tests read the archetype or use `pulse_bot`. One client test relied on `rhino_beetle` without naming it:
`battle_pipeline::commands::tests::stim_goes_only_to_units_whose_overclock_is_unlocked` (G-8) needed a shipped
archetype whose Overclock no research unlocks, and `rhino_beetle` was the only one. It was restated test-only in the
commit before this one (it now gives the locked archetype's Overclock to an armed archetype no upgrade names) and
passes before and after. The new test
`combat_correctness_tests.rs::overclock_research_stims_the_marauder_analogue_and_not_the_roach_analogue` sits beside
S-11's unlock test: after `overclock_stim`, a `UseStim` to a `shock_golem` and a `rhino_beetle` stims the golem
(life minus the catalog cost, one Overclock buff for the catalog duration) and leaves the beetle untouched. Before
the data edit it failed with `the Marauder analogue carries the Overclock`; after it, it passes.

**Measured.**

- **Whole matrix, 20 seeds: bit-identical** to the state after WP8 (all 1000 rows of `matrix.csv`). Expectation
  failures stay 0 on the seeds of record and 7 over all seeds.
- **Battle 33, 68 seeds: bit-identical** to §6.2 and §7 (all output files byte-identical).
- **`golden_replay.ron`: re-recorded at every checkpoint (1 to 20), as plan Amendment 3 expects.** The replay
  completes `overclock_stim` for player 0 before tick 1, and each player's `unlocked_abilities` map is hashed
  (`PlayerUpgradeState` in `checksum/encoding.rs`), so the new `shock_golem` entry changes every checksum. **No
  behaviour changed:** with only the `units.ron` part of this commit (the `upgrades.ron` line left out), both golden
  fixtures pass as recorded for WP8, on this branch as on the pre-WP7 probe of 2026-11-04. The replay's
  `rhino_beetle` squad is never ordered to stim.
- **`golden_replay_orders.ron`: unchanged** (it researches nothing).
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`) reproduce both fixtures.

## 9. WP9: the shield-regeneration delay comes from the catalog (D-4)

**Commit.** The code commit of WP9, on `main` at `ab2042c` (WP15 and flipbook WP2 merged). The delay that
`world/upkeep.rs` hard-coded as 250 ticks is now the per-archetype field `shield_regen_delay_ticks: Option<u16>`
of `units.ron` and `structures.ron`, carried into `UnitArchetype`. Validation (`catalog/validation.rs`, typed
`CatalogInvalidValue` naming the field) requires it exactly when `max_shields > 0`. All 29 shielded archetypes (14
units, 15 structures) carry `Some(250)`, today's value; WP11 converts it. A new entity's
`ticks_since_last_damage` starts at `u16::MAX` ("never damaged", ready for any authored delay) instead of the
literal 250. Outputs under `~/scratch/combat_wp9/` on the development server (`base/` the state before, `wp9/` this
commit).

**Tests first.** Two validation unit tests (a shielded unit or structure without the field, an unshielded one with
it, each rejected with the field named) and an integration test in `damage_and_shields_tests.rs` (a scratch
archetype with delay 10 regenerates on the 10th tick after damage and not before) were written first. Before the
field existed they did not compile; with the field added but `upkeep_entity` still reading 250, the scratch test
failed with `Shields must begin regenerating on tick 10 after the damage`. The former
`test_shield_regeneration_250_tick_delay` now reads the delay from the archetype
(`test_shield_regeneration_waits_for_the_catalog_delay`).

**Spawn counter.** The plan preferred keeping the checksums if possible (D-4). Spawning with the archetype's own
delay would keep them for shielded units only; an unshielded archetype has no delay, and any value chosen for it
would be a new magic number, so the plan's `u16::MAX` was used.

**Measured.**

- **Whole matrix, 20 seeds: bit-identical** to the state before (all 1000 rows of `matrix.csv`; the state before
  is itself byte-identical to WP15's, §8). Expectation failures stay 0 on the seeds of record and 7 over all seeds.
- **Battle 33, 68 seeds: bit-identical** (all output files byte-identical).
- **Both golden replays: re-recorded at every checkpoint** (`golden_replay.ron` 1 to 20, `golden_replay_orders.ron`
  1 to 60; the first differing checkpoint is the first), only through the hashed spawn counter: with this commit's
  spawn value set back to 250 and everything else kept, both fixtures pass as recorded before it.
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`) reproduce both fixtures.
- **`sim_fixtures` `DEFAULT_SANDBOX_CHECKSUM` (`fixture_tests.rs`): re-recorded** for the same reason: with the spawn
  value set back to 250 the test computes the old checksum again.

## 10. WP10: the shield-regeneration rate (Q0039)

**Commit.** The data commit of WP10, after WP9 (§9). Every shielded archetype regenerates shields at StarCraft II's
rate converted from Normal to Faster time, the third of the four values of Q0039 (option 3):

| Field | Before | After | Reference |
|---|---|---|---|
| `shield_regen_per_tick`, all 29 shielded archetypes (14 units in `units.ron`, 15 structures in `structures.ron`) | 0.08 | **0.112** | `ShieldRegenRate 2` Normal per s = 2.8 Faster per s = 0.112 per tick; every one confirmed by the audit, per identity with its source line ([`2026-11-04-catalog-time-audit.md`](2026-11-04-catalog-time-audit.md) §1 item 3 and group WP10 in §4; for example `psi_grey` `liberty` UnitData:3176) |

No shielded archetype differed from `ShieldRegenRate 2` (audit §1 item 3), so none is left to WP12. Doc 03 §1.1
already says 2.8 per second; doc 07 gives no regeneration rate. Outputs under `~/scratch/combat_wp9/wp10/`.

- **Whole matrix, 20 seeds: bit-identical** to the state after WP9 (all 1000 rows of `matrix.csv`: outcome, end
  tick, lead-unit life). Expectation failures stay 0 on the seeds of record and 7 over all seeds (battle 33).
- **Where shields do regenerate in the matrix.** `matrix.csv` does not record shields, so every battle was also traced
  on the same 20 seeds (`--battle`, per-tick `*_health_regained`; the sum script `regen_sum.py` is a throwaway beside
  the outputs). Shields regenerate in two battles only, and more with this step: **31** (15 marauders against 15
  stalkers; red regains 24.64 → 34.50 over the same 308 ticks, seeds 8 and 14) and **35** (12 vikings against 6
  colossi; red 36.24 → 50.74 over the same 453 ticks, seed of record 3203386110, which ends unresolved). In neither
  does a survivor count, end tick or lead-unit life move. Every other
  regained life in the matrix is Zerg or Reaper life regeneration and is identical.
- **Battle 33, 68 seeds: bit-identical** to §6.2 (all output files byte-identical; 42 / 18 / 8, marine survivors
  4.03, SD 3.26; seeds of record 7–0, 3–0, 9–0, 6–0), as the investigation found for the zealot's rate alone. The
  battle 33 comment in `matchup_matrix.ron` is unchanged.
- **`golden_replay.ron`: re-recorded from checkpoint 13** (ticks 601 to 650): checkpoints 1 to 12 are unchanged, and
  13 to 20 changed, from the first checkpoint in which a damaged `red_psi` zealot has gone 250 ticks unhit.
- **`golden_replay_orders.ron`: unchanged**: its shielded units (`flying_saucer`, `psi_grey`) never regenerate
  shields in its 600 ticks.
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`) reproduce both fixtures.

## 11. WP11: the shield-regeneration delay (Q0039)

**Commit.** The data commit of WP11, after WP10 (§10). Shields start regenerating after StarCraft II's delay,
converted from Normal to Faster time, the last of the four values of Q0039 (option 3):

| Field | Before | After | Reference |
|---|---|---|---|
| `shield_regen_delay_ticks`, all 29 shielded archetypes (14 units, 15 structures) | 250 | **179** | `ShieldRegenDelay 10` Normal s = 7.14 Faster s = 178.57 ticks, rounded halves up (`liberty` UnitData:3166 for the zealot, battle 33 investigation §1.5; every shielded archetype confirmed by the audit, §1 item 3) |

The spawn counter starts at `u16::MAX` since WP9, so this value changes no spawned state. Doc 03 §1.1 already
states the delay in game seconds (10). Outputs under `~/scratch/combat_wp9/wp11/` and `regen_wp11/`.

- **Whole matrix, 20 seeds: bit-identical** to the state after WP10 (all 1000 rows of `matrix.csv`). The plan
  expected that long Protoss fights such as battles 28 and 29 might move: they do not: their immortals and stalkers
  regenerate no shields on any seed, before or after. Expectation failures stay 0 on
  the seeds of record and 7 over all seeds (battle 33).
- **Where shields regenerate** (per-tick traces as in §10): battle **31** now regenerates on 7 seeds instead of 2 (2,
  4, 8, 11, 14, 15, 22; red regains 34.50 → 65.97 over 589 ticks instead of 308) and battle **35** on its seed of
  record for 524 ticks instead of 453 (50.74 → 58.69). No survivor count, end tick or lead-unit life moves. No other
  battle regenerates shields.
- **Battle 33, 68 seeds: bit-identical** to §6.2 (all output files byte-identical; no zealot regenerates any shield
  on any of the 68 seeds, before or after: `red_health_regained` is 0 throughout). Its comment in
  `matchup_matrix.ron` is unchanged (D-6 applies to commits that move it).
- **`golden_replay.ron`: re-recorded from checkpoint 12** (ticks 551 to 600): checkpoints 1 to 11 are unchanged, and
  12 to 20 changed, where the damaged `red_psi` zealot of §10 now starts regenerating 71 ticks sooner.
- **`golden_replay_orders.ron`: unchanged.**
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`) reproduce both fixtures.

## 12. WP16: `flying_saucer` becomes the Phoenix (Q0045 4b)

**Commit.** The data commit of WP16, after WP11 (§11). The owner's answer 4(b) to Q0045 makes the Phoenix the
counterpart of `flying_saucer` (plan D-10). Its audit against the Phoenix (plan WP16 step 1), with the chain, a
citation for each value and the class each had before, is in
[`2026-11-04-catalog-time-audit.md`](2026-11-04-catalog-time-audit.md) §6.1 and its rows in §4. What changed:

| Field | Before | After | Reference (5.0.14 game data) |
|---|---|---|---|
| `sc2_analogue` | "Viking / Scout (…)" | "Phoenix (…)" | doc 07 row 12 |
| `mineral_cost` / `gas_cost` | 125 / 75 | **150 / 100** | `liberty` UnitData:3734, :3735 |
| `build_time_ticks` | 850 | **625** | 35 Normal s (`swarm` AbilData:1164) = 25 Faster s |
| `attributes` | Light, Psionic (17) | **Light, Mechanical (9)** | `liberty` UnitData:3723, :3724 |
| `max_energy` / `energy_regen_per_tick` | 0 / 0.000 | **200 / 0.0315** | `liberty` UnitData:3728, :3729 (0.5625 Normal per s) |
| `max_speed`, `acceleration`, `deceleration` | 0.15 | **0.238** | `Speed` 4.25 Normal (`liberty` UnitData:3685) = 5.95 Faster |
| `sight_radius` | 11 | **10** | `liberty` UnitData:3708 |
| `weapon` (ground) | 10 (+4 light), range 6 | **None** | `IonCannons` targets air only (`liberty` WeaponData:189) |
| `air_weapon`: damage, light bonus, strikes | 10, +4, 1 | **5, +5, 2** | `liberty` EffectData:1985, :1986, :1961 |
| `air_weapon.max_range` | 6 | **5** | `void` WeaponData:214 |
| `air_weapon.damage_point_ticks` / `backswing_ticks` | 0 / 2 | **3 / 9** | `CWeapon` defaults 0.167 / 0.5 Normal s (`core` WeaponData:17, :18) |
| `air_weapon.bonus_upgrade_per_level` | 1 | **0** | the air weapon upgrades raise only `Amount` (`swarm` UpgradeData:167–194) |

Unchanged and matching: life 120, shields 60, armour 0, supply 2, period 20 (1.1 Normal s = 19.64 ticks),
`upgrade_bonus_per_level` 1, the shield values of WP10 and WP11, and `anion_pulse_crystals` (+2 range). Unchanged by
the plan's scope: `turn_rate_deg` (999 here; WP17 sets 84.0° per tick), the weapon's turret and arc (WP17), radii,
`scan_range` (WP14) and `push_priority`. Projectile speed stays `None` (three-phase missile, class E). Not modelled
(plan F-5): Graviton Beam and firing while moving. Doc 07 row 12 now gives the weapon's timings and names what is not
modelled; doc 08's Air Weapons row no longer claims a light-bonus upgrade for it. No Phoenix look or media change is
part of this package: the name, lore, visual kind and Veo prompts stay; a later media pass may revisit them.

**Users checked first (plan WP16 step 2).**

- **`golden_replay_orders.ron`.** A throwaway observer (not committed) of the replay logged the spotter's shots, its
  position and the phase-order evidence. Before: the spotter flies at 0.15 from tick 10, reaches its goal at tick 120,
  and fires **9 shots at the `psi_grey` zealots on the ledge** (ticks 121 to 281, every 20). The same-tick reveal and
  acquisition (6 acquisitions) happens at loop tick 63. After: it flies at 0.238, arrives at tick 80, fires **no shot**
  (no ground weapon), and the reveal and acquisition happen at loop tick 44. The mid-route repaths are at the same
  ticks in both runs, and 27 entities survive at tick 600 in both.
- **`high_ground_combat_tests.rs`** uses the saucer as an air spotter spawned on the marine, 2 cells from the tile it
  must reveal: sight 10 still covers it, and the test never relies on its weapon. It passes unchanged.
- **Client and other tests.** No client test or fixture names `flying_saucer` (`crates/client/tests/data/`
  included); `sc2_analogue` is read by no Rust code. The tests that pick "the first air archetype" from the catalog
  strip its weapons or need only an air body. The common gate covers them. No test was restated.

**Measured.**

- **Whole matrix, 20 seeds: bit-identical** to the state after WP11 (all 1000 rows of `matrix.csv`; no battle uses
  `flying_saucer`). The pre-change run was itself bit-identical to WP11's recorded `matrix.csv`. Expectation failures
  stay 0 on the seeds of record and 7 over all seeds (battle 33).
- **Battle 33, 68 seeds: bit-identical** (all output files byte-identical): 42 / 18 / 8, marine survivors 4.03
  (SD 3.26); seeds of record 7–0, 3–0, 9–0, 6–0.
- **`golden_replay.ron`: unchanged** (it spawns no `flying_saucer`).
- **`golden_replay_orders.ron`: re-recorded at every checkpoint (1 to 60).** Checkpoint 1 (ticks 1 to 10, before
  the spotter moves) changes because the spotter's energy (200, was 0) is hashed from spawn; from tick 10 its speed,
  then the earlier reveal and the absent shots change the rest. Both phase-order situations are still found by the
  test.
- Debug, release and `wasm32-wasip1` (release, under `/usr/local/bin/wasmtime`) reproduce both fixtures.

Outputs under `~/scratch/combat_wp16/` on the development server (`before/`, `after/`).

## 13. WP12: the audit's corrections and the worker's backswing (Q0039, Q0045 2b)

**Package.** One data commit per group of the audit's §5
([`2026-11-04-catalog-time-audit.md`](2026-11-04-catalog-time-audit.md)), in the plan's order (WP12 step 1), each
compared with the step before it; the first is compared with the state after WP16 (`7b91323`), whose matrix and
battle 33 this branch reproduces byte for byte. Every value is the audit's reference (§4, with its source line):
ticks rounded to the nearest tick, halves up; speeds and rates as the audit's reference column. The audit's §7 has the
measurement of all groups together, made first (WP3 step 5). Outputs under `~/scratch/combat_wp12/<group>/` on the
development server; the throwaway script that writes each group from the audit's tables is
`~/scratch/combat_wp12/scripts/apply_group.py`. Groups 1 to 8 touch no unit a matrix battle spawns, so each was
expected to leave the matrix and battle 33 bit-identical; the golden replays complete their research and place their
structures before tick 1, so production and research times were expected not to reach them either.

### 13.1 P1: three build times copied in Normal time (class B)

| Identity | Field | Before | After | Source (Normal s) |
|---|---|---|---|---|
| `psi_grey` | `build_time_ticks` | 950 | **679** | 38 s, `swarm` AbilData:1068 (`GatewayTrain` Train1) |
| `railgun_crawler` | `build_time_ticks` | 1125 | **804** | 45 s, `liberty` AbilData:1649 (`FactoryTrain` Train2) |
| `planetary_bulwark_dome` | `build_time_ticks` | 1250 | **893** | 50 s, `liberty` AbilData:3163 (`UpgradeToPlanetaryFortress`, Stats delay) |

Measured: whole matrix bit-identical (all 1000 rows), battle 33 bit-identical, both golden replays unchanged in debug,
release and `wasm32-wasip1`. Doc 07 rows (`psi_grey`, `railgun_crawler`) and doc 08 (the Planetary Fortress row and
§2.1's example list) now state the new ticks.

### 13.2 P2: production and research times rounded to Liquipedia's whole seconds

127 values (31 unit and 34 structure `build_time_ticks`, 62 `research_ticks`) were Liquipedia's whole Faster
seconds × 25. Each now is the game data's Normal seconds ÷ 1.4 × 25, to the nearest tick: 65 rise and 62 fall, by
4, 7, 9 or 11 ticks (for example `pulse_bot` 450 → 446, 25 Normal s, `liberty` AbilData:1630; `foundry_core_dome`
1775 → 1786, 100 Normal s; level 1 weapon and armour upgrades 2850 → 2857, 160 Normal s). Every value with its
source line is in the audit's group P2 (§5) and §4.

Measured: whole matrix bit-identical (all 1000 rows), battle 33 bit-identical, both golden replays unchanged in
debug, release and `wasm32-wasip1` (their research is completed and their structures placed before tick 1). The doc
07 roster and doc 08 cells that state these ticks are corrected, and doc 08 §2.1 now gives the conversion from
Normal seconds with examples in place of the whole-second list.

### 13.3 P3: production and research times that differed otherwise

31 values (5 unit and 12 structure `build_time_ticks`, 14 `research_ticks`) were older patch values or unsourced
numbers. Each is now the game data's 5.0.14 value, Normal seconds ÷ 1.4 × 25 to the nearest tick, with the source
line in the audit (§5 group P3, §4). The largest: the Spawning Pool analogue `spawning_hollow_dome` 525 → 1161
(65 Normal s, `liberty` AbilData:2206), the Spire and Greater Spire analogues 1075 and 900 → 1786 (100 Normal s), the
Barracks Reactor analogue `duplicator_module_pod` 450 → 893 (50 Normal s), the Roach 750 → 482 (27 Normal s,
`liberty` AbilData:2510), the Stimpack 3025 → 2500 (140 Normal s, `voidmulti` AbilData:48) and Grooved Spines
1775 → 1250 (70 Normal s, `voidmulti` AbilData:528; patch 5.0.12, audit §2.2).

Measured: whole matrix bit-identical (all 1000 rows), battle 33 bit-identical, both golden replays unchanged in
debug, release and `wasm32-wasip1`. The doc 07 and doc 08 cells that state these ticks are corrected.

### 13.4 U1: upgrade speed bonuses

| Upgrade | `move_speed_delta` before | After | Source (Normal units per s, × 1.4 ÷ 25) |
|---|---|---|---|
| `charge` | 0.038 | **0.063** | +1.125, `voidmulti` UpgradeData:69 |
| `gravitic_drive` | 0.0496 | **0.049** | +0.875, `liberty` UpgradeData:1823 |
| `flux_vanes` | 0.0308 | **0.0319** | 2.75 → 3.3203, `voidmulti` UpgradeData:348 and UnitData:2025 |
| `metabolic_boost` | 0.0826 | **0.0978** | +1.746, `liberty` UpgradeData:1856 |
| `glial_reconstitution` | 0.0400 | **0.042** | +0.75, `liberty` UpgradeData:1925 |
| `muscular_augments` | 0.0126 | **0.0392** | +0.7 off creep, `voidmulti` UpgradeData:743 |
| `pneumatized_carapace` | 0.0576 | **0.0691** | 0.6445 → 1.879, `void` UpgradeData:621 and `voidmulti` UnitData:1194 |
| `anabolic_synthesis` | 0.0228 | **0.0236** | +0.421871 off creep, `voidmulti` UpgradeData:47 |

No matrix battle researches any of these (the matrix applies `chitinous_plating`, `ground_weapons_1`,
`missile_attacks_1`, `combat_shield` and `overclock_stim` only), and neither golden replay does. Measured: whole matrix
and battle 33 bit-identical, both golden replays unchanged in debug, release and `wasm32-wasip1`. Doc 08's upgrade rows
(and its 5.0.14 notes for Muscular Augments and Anabolic Synthesis) now state the bonus per tick with its source in
place of percentages of speeds that were not the catalog's.

**Test restated first.** `upgrades_and_tech_tree_tests.rs::test_metabolic_boost_speed` restated the old delta 0.0826
(it failed with 0.2630 against 0.2478). The commit before this one makes it read `metabolic_boost`'s delta for
`razor_mantis` from the shipped catalog; it passes on the catalog before and after.

### 13.5 U2: Adrenal Glands

`adrenal_glands` `attack_cooldown_delta` −5 → **−3**: the zergling's period 0.696 Normal s (`liberty` WeaponData:362,
12.43 ticks, 12 in the catalog) is divided by 1.4 under two `RateMultiplier +0.2` entries (`liberty` UpgradeData:1846,
`void` UpgradeData:886), 8.88 ticks: 9, three less than 12. Liquipedia's cooldown 0.497 with the upgrade agrees
(audit §9). No matrix battle or golden replay researches it. Measured: whole
matrix and battle 33 bit-identical, both golden replays unchanged in debug, release and `wasm32-wasip1`. Doc 08's
Adrenal Glands row said 12 → 7 and now says 12 → 9.

### 13.6 M1: the workers' mining time

`harvest_rate_ticks` 60 → **50** on the three workers (`welder_drone`, `grav_harvester`, `nectar_worker`): a worker
occupies a mineral field for 1.99 Faster s (Liquipedia, [Resources](https://liquipedia.net/starcraft2/Resources)),
49.75 ticks. The game data holds no mining time (`SCVHarvest`, `ProbeHarvest`, `DroneHarvest` set none; audit §8), so
this group is sourced from the secondary source, as the audit records; it is one value, not a range, so D-7 gives no
ground to defer it. No matrix battle and neither golden replay harvests. Measured: whole matrix and battle 33
bit-identical, both golden replays unchanged in debug, release and `wasm32-wasip1`. No doc 07 or doc 08 cell states
the mining time.

### 13.7 M2: Siege Mode (one value applied, one deferred)

- **Applied:** `railgun_crawler`'s Siege Mode ability `cooldown_ticks` 75 → **0**. The game's `SiegeMode` morph has
  no cooldown (`liberty` AbilData:1308 sets no `Cost/Cooldown`, and no later layer adds one). No mechanism is
  missing: validation accepts a cooldown of 0, and a unit that is still channelling its transformation refuses
  every cast (`world/abilities.rs`, the admission check on `channeling_remaining_ticks`), so the toggle cannot be
  re-issued mid-transformation, as in the game.
- **Deferred (D-7, second ground: the verification failed):** the mode's `cast_ticks` 71 stays. The audit's 72 was
  marked **verify** (§8): it adds the morph's `Stats` delay 0.5 and duration 3.5417 Normal s. Checked against the
  data within the effort bound, the verification fails rather than confirms: the same morph also has
  `RandomDelayMax="0.5"` (`liberty` AbilData:1308, `InfoArray`), a random extra delay of up to 0.5 Normal s, so
  sieging takes 72 to 81 ticks and has no single reference value; and unsieging (`Unsiege`, `liberty`
  AbilData:1328) has no 0.5 delay at all (3.5417 Normal s, 63 ticks), while the catalog has one `cast_ticks` for
  the mode. Liquipedia's Siege Tank page gives no figure. Which single value our one field should carry is a
  modelling choice, not a correction; it is listed with the package's deferrals (§13 summary).

Measured: whole matrix and battle 33 bit-identical (no battle orders Siege Mode), both golden replays unchanged in
debug, release and `wasm32-wasip1`. No doc 07 or doc 08 cell states the cooldown.

### 13.8 R3: energy regeneration

`energy_regen_per_tick` 0.056 → **0.0315** on the nine casters (`specter_android`, `repair_shuttle`,
`cloak_gunship`, `omni_probe_drone`, `prism_weaver`, `cerebral_elder`, `hive_matron`, `mycelium_shaman`,
`tongue_chameleon`): `EnergyRegenRate` 0.5625 Normal per s × 1.4 ÷ 25 (for example `liberty` UnitData:4989 for the
Ghost; each source line in the audit's group R3). The Phoenix already has 0.0315 (WP16). No matrix battle or golden
replay spawns one of the nine. Measured: whole matrix and battle 33 bit-identical, both golden replays unchanged in
debug, release and `wasm32-wasip1`. Doc 03 §1 already gives 0.7875 per Faster second; doc 07 and doc 08 state no rate.

### 13.9 S1: five speeds copied in Normal time (class B)

| Identity | `max_speed` before | After | Source (Normal units per s, × 1.4 ÷ 25) |
|---|---|---|---|
| `welder_drone`, `grav_harvester`, `nectar_worker` | 0.1124 | **0.1575** | 2.8125, `liberty` UnitData:4731, :3074, :7040 |
| `rhino_beetle` | 0.09 | **0.126** | 2.25, `liberty` UnitData:7186 |
| `railgun_crawler` | 0.09 | **0.126** | 2.25, `liberty` UnitData:5075 |

`acceleration` and `deceleration` follow `max_speed`, the catalog's convention (parsed and unused, audit class E),
as WP7 did for the marine.

**Test restated first.** `combat_correctness_tests.rs::s2_explicit_focus_fire_survives_nearby_enemy` orders a marine
onto a far `rhino_beetle` beside a zealot and reads the order after 30 ticks. The faster Roach walked into its own
range sooner and, with the zealot, killed the marine before the check (`no entry found for key`). The test is about
the order surviving a nearby enemy, not about the target's speed; in the commit before this one the far target holds
position. Same assertion and run length; it passes before and after.

**Whole matrix, 20 seeds.** Compared with the state after R3: **8 battles moved, no winner changed, no expectation
failed** (0 on the seeds of record; the 7 over all seeds are battle 33's, unchanged).

| Battle | Seeds moved | Survivors blue – red, before → after | End tick, before → after | Lead hull, before → after |
|---|---|---|---|---|
| 1b_stalker_vs_2_roaches | 20 | 0.00 – 2.00 → same | 159.8 → 156.6 | red 106.75 → same |
| 6a_roach_vs_hydra | 3 | 1.00 – 0.00 → same | 149.7 → 149.5 | blue 35.00 → same |
| 6b_2_roaches_vs_hydra | 1 | 2.00 – 0.00 → same | 65.9 → 65.8 | blue 118.60 → same |
| 6c_roach_vs_2_hydras | 2 | 0.00 – 2.00 → same | 96.3 → 96.2 | red 61.98 → same |
| 22a_reaper_vs_2_workers | 20 | 1.00 – 0.00 → same | 102.0 → 102.0 | blue 40.63 → 37.24 |
| 24a_roach_vs_5_lings | 11 | 0.00 – 2.00 → same | 296.6 → 295.6 | — |
| 24b_upgraded_roach_vs_5_lings | 17 | 1.00 – 0.00 → same | 255.6 → 255.8 | blue 85.00 → 82.40 |
| 30_20_roaches_vs_10_immortals | 20 | 0.00 – 9.95 → 0.00 – 10.00 | 263.9 → 248.3 | red 128.58 → 113.80 |

- **Roach battles** (1b, 6a to 6c, 24a, 24b, 30): the roaches close the distance sooner, so fights start and end a
  few ticks earlier; against the ranged stalker and hydras the outcome and the survivor's life do not move. In 30 the
  fight ends 15.6 ticks sooner on average; the immortals' first unit ends with less life (128.58 → 113.80) and lives
  on all 20 seeds (19 before), and on one seed one more immortal survives (9.95 → 10.00 mean).
- **22a**: the workers reach the reaper sooner and land a few more blows (reaper 40.63 → 37.24 life); the reaper
  still wins on every seed.
- No matrix battle spawns `railgun_crawler` with room to walk before the fight decides (battle 3's tank starts in
  range), so its speed moves nothing there.

**Battle 33, 68 seeds: bit-identical.**

**Golden replays re-recorded.** `golden_replay.ron` from checkpoint 1 (its `rhino_beetle` squad moves at tick 5, its
`railgun_crawler` squad at tick 120); `golden_replay_orders.ron` from checkpoint 3 (ticks 21 to 30; its `rhino_beetle`
squad moves at tick 20). The phase-order test still finds both situations. Debug, release and `wasm32-wasip1`
reproduce both fixtures. Doc 07's speed cells for the five units and §2.2's worker speed (2.81 tiles/s, the Normal
value) are corrected.

### 13.10 S2: five speeds that differed otherwise

| Identity | `max_speed` before | After | Source (Normal units per s, × 1.4 ÷ 25) |
|---|---|---|---|
| `omni_probe_drone` (Raven) | 0.154 | **0.1652** | 2.9492, `voidmulti` UnitData:2660 |
| `prism_weaver` (Sentry) | 0.126 | **0.14** | 2.5, `voidmulti` UnitData:1552 |
| `cerebral_elder` (High Templar) | 0.1048 | **0.1129** | 2.0156, `voidmulti` UnitData:685 |
| `stealth_eye_orb` (Observer) | 0.1052 | **0.1129** | 2.0156, `voidmulti` UnitData:1134 |
| `spore_matriarch` (Brood Lord) | 0.0788 | **0.105** | 1.875, `voidmulti` UnitData:143 |

`acceleration` and `deceleration` follow, as in S1. Measured: **whole matrix bit-identical** (battles 19 and 20
spawn the Brood Lord; its speed moves no outcome, end tick or lead-unit life there), battle 33 bit-identical,
both golden replays unchanged in debug, release and `wasm32-wasip1`. Doc 07's five speed cells and doc 08's
Gravitic Boosters row (which restated the Observer's old base speed) are corrected.

### 13.11 R1: the reaper's life regeneration (deferred)

**Deferred on D-7's third ground: the reference needs a mechanism we do not model.** `jet_jumper`'s
`hp_regen_per_tick` 0.08 is the Reaper's `LifeRegenRate` 2 per Normal s (`void` UnitData:1476) copied in Normal
time; converted it is 0.112. But the game regenerates the Reaper's life only after 10 Normal s (179 ticks) without
damage (`LifeRegenDelay`, `void` UnitData:1475), and our life regeneration runs on every tick (only shields wait,
since WP9). The audit's row already noted "not modelled". The whole-matrix measurement of all groups (audit §7,
left out one group at a time) shows the consequence: with 0.112 applied always, `22b_reaper_vs_stalker` ends
unresolved on all 20 seeds, the reaper out-healing the stalker in combat, which it never does in the game. The rate
alone would move the catalog away from StarCraft II's behaviour, not towards it, so it waits for the delay.
[Q0049](../questions/open/0049-reaper-life-regeneration-delay.md) asks the owner whether to add a per-archetype
life-regeneration delay (the default keeps 0.08 and the deferral). No catalog value, test or golden changes here.

### 13.12 R2: Zerg life regeneration

`hp_regen_per_tick` on 32 Zerg archetypes (15 units, 17 structures), each with its source line in the audit's group
R2: **0.02 → 0.01531** for 30 of them (`LifeRegenRate` 0.2734 Normal per s × 1.4 ÷ 25; for example `void`
UnitData:1127), **0 → 0.01531** for `rhino_beetle` (the Roach regenerates like every Zerg unit, `liberty`
UnitData:7184; its faster burrowed regeneration is not modelled), and **0.02 → 0.056** for `sonic_bat` (the
Mutalisk's `LifeRegenRate` 1, `void` UnitData:3094).

**Whole matrix, 20 seeds.** Compared with the state after S2: **16 battles moved, no winner changed, no expectation
failed** (0 on the seeds of record; 7 over all seeds, battle 33's, unchanged).

| Battle | Seeds moved | Survivors blue – red | End tick, before → after | Lead hull, before → after |
|---|---|---|---|---|
| 1b_stalker_vs_2_roaches | 9 | 0.00 – 2.00, same | 156.6, same | red 106.75 → 107.77 |
| 5a_hydra_vs_marine | 20 | 1.00 – 0.00, same | 52.0, same | blue 67.00 → 66.77 |
| 5b_hydra_vs_2_marines | 20 | 1.00 – 0.00, same | 112.0, same | blue 20.20 → 19.68 |
| 6a_roach_vs_hydra | 20 | 1.00 – 0.00, same | 149.5, same | blue 35.00 → 37.20 |
| 6b_2_roaches_vs_hydra | 10 | 2.00 – 0.00, same | 65.8, same | blue 118.60 → 119.06 |
| 6c_roach_vs_2_hydras | 9 | 0.00 – 2.00, same | 96.2, same | red 61.98 → 61.80 |
| 9b_hellion_vs_roach | 20 | 0.00 – 1.00, same | 148.0, same | red 117.00 → 119.25 |
| 15_plated_ultralisk_vs_8_marines | 20 | 1.00 – 0.00, same | 111.0, same | blue 415.28 → 414.77 |
| 16a_muta_vs_marine | 20 | 1.00 – 0.00, same | 115.0, same | blue 74.26 → 78.33 |
| 16b_muta_vs_2_marines | 20 | 0.00 – 1.00, same | 182.0 → 197.0 | red 27.00 → 18.00 |
| 17_void_ray_vs_muta | 20 | 1.00 – 0.00, same | 181.0 → 190.0 | blue 150.00, same |
| 21_lurker_vs_6_marines | 20 | 1.00 – 0.00, same | 106.4, same | blue 71.59 → 71.10 |
| 24a_roach_vs_5_lings | 20 | 0.00 – 2.00, same | 295.6 → 305.4 | — |
| 24b_upgraded_roach_vs_5_lings | 20 | 1.00 – 0.00, same | 255.8, same | blue 82.40 → 85.93 |
| 27_100_lings_vs_8_ultralisks | 20 | 0.00 – 8.00, same | 203.1, same | red 432.59 → 432.04 |
| 29_100_lings_vs_12_stalkers | 1 | 71.50 – 0.00, same | 440.4, same | blue 34.41 → 34.38 |

- **The Roach now regenerates** (1b, 6a to 6c, 9b, 24a, 24b): a surviving roach ends with more life, and a losing one
  lasts longer (24a: the zerglings need 9.8 more ticks on average to finish the roach).
- **Slower Zerg regeneration elsewhere** (5a, 5b, 15, 21, 27, 29): the surviving hydralisk, ultralisk, lurker or
  zergling ends with up to half a point less life.
- **The Mutalisk regenerates 2.8 times faster** (16a, 16b, 17): the lone mutalisk of 16a ends with 4.07 more life; in
  16b the mutalisk holds out 15 ticks longer against the two marines and the surviving marine ends with 9 less life;
  the void ray of 17 needs 9 more ticks to kill the mutalisk.

**Battle 33, 68 seeds: bit-identical.** **`golden_replay.ron` re-recorded** at checkpoints 6 and 8 to 11 (its
`rhino_beetle` squad regenerates now); `golden_replay_orders.ron` unchanged. Debug,
release and `wasm32-wasip1` reproduce both fixtures. Doc 03 already states 0.38 per Faster second; doc 07 and doc 08
state no rate.

### 13.13 W1: two weapon periods copied in Normal time (class B)

| Identity | Field | Before | After | Source (Normal s ÷ 1.4 × 25) |
|---|---|---|---|---|
| `railgun_crawler` (Siege Tank, tank mode) | `weapon.period_ticks` | 26 | **19** | 1.04, `liberty` WeaponData:242 (18.57) |
| `planetary_bulwark_dome` (Planetary Fortress) | `ground_weapon.period_ticks` | 50 | **36** | 2, `liberty` WeaponData:139 (35.71) |

**Test restated first.** `sc2_battle_benchmarks_tests.rs::test_railgun_crawler_declarative_modes_and_siege_splash`
asserted the period 26 and read two marines' life after 30 ticks, assuming one tank-mode shot; at 19 ticks two shots
fit. In the commit before this one it records both lives on the tick the first shot lands (same window, same
assertions: 45 → 30 and 45) and asserts that tank mode fires faster than the 54-tick siege cannon. It passes before
and after.

**Whole matrix, 20 seeds.** Compared with the state after R2: **1 battle moved, and its expectation was
re-recorded.**

| Battle | Seeds moved | Survivors blue – red | End tick | Blue lead hull, before → after | Failures of record / all |
|---|---|---|---|---|---|
| 3_immortal_vs_tank | 20 | 1.00 – 0.00, same | 89.0, same | 200.00 → **176.00** | 0 / 0 → 4 / 20 before re-recording, 0 / 0 after |

The tank now lands five shots (25 each; the immortal's 100 shields take four) before the immortal kills it, so the
fifth reaches its life: 25 − 1 armour = 24, 200 → 176 on every seed. The end tick does not move: the immortal's
cadence decides it. In StarCraft II the immortal would keep its life: its Barrier, autocast on taking damage, absorbs
up to 100 damage for 3 Normal s (`void` BehaviorData:97–101, `voidmulti` BehaviorData:278, cooldown 45 s in
`voidmulti` AbilData:591). Barrier is not modelled; that is a real gap, not a property of this value, so it is
recorded as finding **M-20** of the remediation plan (§4.2) rather than masked. The expectation of battle 3 is
re-recorded to the measured outcome (`blue_lead_hp: (176, 176)`, the smallest range on the seeds of record; it holds
on all 20), and its claim now says the immortal loses its shields and one shot of life because Barrier is missing.
Expectation failures after re-recording: 0 on the seeds of record; 7 over all seeds (battle 33's, unchanged). The
Planetary Fortress is in no battle.

**Battle 33, 68 seeds: bit-identical.** **`golden_replay.ron` re-recorded from checkpoint 4** (ticks 151 to 200,
when its `railgun_crawler` squad first fights); `golden_replay_orders.ron` unchanged. Debug, release and
`wasm32-wasip1` reproduce both fixtures. Doc 07's Siege Tank weapon cell (18 → 19 ticks) and doc 08's Planetary
Fortress row (period 50 → 36 ticks) are corrected.

### 13.14 W2: weapon periods that differed by more than rounding

| Identity | Field | Before | After | Source (Normal s ÷ 1.4 × 25) |
|---|---|---|---|---|
| `welder_drone`, `grav_harvester`, `nectar_worker` (workers) | `weapon.period_ticks` | 19, 15, 15 | **27** | 1.5, `liberty` WeaponData:200, :147, :354 |
| `rhino_beetle` (Roach) | `weapon.period_ticks` | 28 | **36** | 2, `liberty` WeaponData:345 |
| `shock_golem` (Marauder) | `weapon.period_ticks` | 26 | **27** | 1.5, `liberty` WeaponData:249 |
| `colossus_mech` (Thor) | `air_weapon.period_ticks` | 36 | **23** | 1.28, `voidmulti` WeaponData:129 |
| `sky_bastion` (Liberator) | `ground_weapon` / `air_weapon`, `weapon` | 28 / 25, 25 | **29 / 32, 32** | 1.6, `void` WeaponData:339; 1.8, :327 |
| `warp_strider` (Stalker) | `weapon`, `air_weapon` | 34 | **33** | 1.87, `voidmulti` WeaponData:201 |
| `void_goliath` (Immortal) | `weapon.period_ticks` | 28 | **29** | 1.6, `voidmulti` WeaponData:208 |
| `volatile_tick` (Baneling) | `weapon.period_ticks` | 10 | **15** | 0.833, `liberty` WeaponData:468 |
| `mortar_stag` (Ravager) | `weapon.period_ticks` | 28 | **29** | 1.6, `voidmulti` WeaponData:214 |

**Test restated first.** `collision_and_immobility_tests.rs::test_converging_units_respect_speed_cap_and_do_not_vibrate`
uses the catalog's first mobile melee archetype, `welder_drone`, and allowed 2 direction reversals per unit in 120
ticks. Every unit already reversed twice: once its swing no longer anchors it, the crowd at the contact ring pushes it
in by about 0.08 and it steps back by about 0.015 on the next tick, once per weapon cycle (identical traces up to tick
32 before and after). With the SCV's longer cooldown entity 6 does it a third time at tick 40. That jostle is a
movement defect, recorded as **finding M-21** (remediation plan §4.2, beside M-19). The test now judges vibration up
to each unit's first strike, the convergence it was written for, with the same threshold; the speed cap over 120
ticks and the damage check are unchanged. It passes before and after.

**Whole matrix, 20 seeds.** Compared with the state after W1 (re-recorded): **21 battles moved; winners changed only
in battle 31 (4 seeds), within its expectation; no expectation failed** (0 on the seeds of record; 7 over all seeds,
battle 33's, unchanged).

| Battle | Seeds moved | Survivors blue – red, before → after | End tick, before → after | Lead hull, before → after |
|---|---|---|---|---|
| 1a_stalker_vs_roach | 20 | 1.00 – 0.00, same | 281.0 → 273.0 | blue 5.00 → 35.00 |
| 1b_stalker_vs_2_roaches | 20 | 0.00 – 2.00, same | 156.6 → 196.6 | red 107.77 → 100.39 |
| 2a_immortal_vs_roach | 20 | 1.00 – 0.00, same | 61.0 → 63.0 | blue 200.00, same |
| 2b_immortal_vs_2_roaches | 20 | 1.00 – 0.00, same | 145.0 → 150.0 | blue 174.00 → 200.00 |
| 3_immortal_vs_tank | 20 | 1.00 – 0.00, same | 89.0 → 92.0 | blue 176.00, same |
| 4_marauder_vs_roach | 20 | 1.00 – 0.00, same | 190.1 → 197.1 | blue 20.00 → 35.00 |
| 6a_roach_vs_hydra | 20 | 1.00 – 0.00, same | 149.5 → 189.5 | blue 37.20 → 4.81 |
| 6b_2_roaches_vs_hydra | 20 | 2.00 – 0.00, same | 65.8 → 81.8 | blue 119.06 → 112.58 |
| 6c_roach_vs_2_hydras | 9 | 0.00 – 2.00, same | 96.2, same | red 61.80 → 69.00 |
| 7_zealot_vs_roach | 20 | 1.00 – 0.00, same | 240.9, same | blue 12.00 → 42.00 |
| 8_zealot_vs_stalker | 10 | 1.00 – 0.00, same | 241.5, same | blue 57.00 → 51.00 |
| 9b_hellion_vs_roach | 20 | 0.00 – 1.00, same | 148.0 → 188.0 | red 119.25 → 112.86 |
| 12b_colossus_vs_3_marauders | 20 | 0.00 – 2.00, same | 164.0 → 170.0 | red 67.53, same |
| 22a_reaper_vs_2_workers | 20 | 1.00 – 0.00, same | 102.0, same | blue 37.24 → 46.24 |
| 22b_reaper_vs_stalker | 20 | 0.00 – 1.00, same | 178.0 → 173.0 | red 80.00, same |
| 24a_roach_vs_5_lings | 20 | 0.00 – 2.00 → 0.00 – 3.00 | 305.4 → 285.4 | — |
| 24b_upgraded_roach_vs_5_lings | 20 | 1.00 – 0.00, same | 255.8 → 327.9 | blue 85.93 → 23.03 |
| 28_100_lings_vs_10_immortals | 20 | 27.30 – 0.00 → 30.10 – 0.00 | 670.5 → 660.4 | blue 35.00 (3) → — |
| 29_100_lings_vs_12_stalkers | 20 | 71.50 – 0.00 → 69.70 – 0.00 | 440.4 → 442.3 | blue 34.38 → 33.09 |
| 30_20_roaches_vs_10_immortals | 20 | 0.00 – 10.00, same | 248.3 → 255.1 | red 113.80 → 132.15 |
| 31_15_marauders_vs_15_stalkers | 20 | 2.65 – 0.70 → 1.70 – 1.35 | 661.3 → 700.2 | blue 88.17 (6) → 69.75 (4) |

- **The Roach fires 22 % less often** (28 → 36 ticks; 1a, 1b, 2a, 2b, 4, 6a to 6c, 7, 9b, 24a, 24b, 30): its
  opponents keep more life (the stalker of 1a 5 → 35, the marauder of 4 20 → 35, the zealot of 7 12 → 42, the
  immortal of 2b 174 → 200, the immortals of 30 more), and fights the roaches win take longer and cost them more
  (1b, 6a, 6b, 9b, 24b: the roach of 24b ends at 23 instead of 86, of 6a at 4.8 instead of 37). In 24a one more
  zergling survives on average.
- **The Stalker fires a tick sooner** (34 → 33; 1a, 8, 22b, 29, 31): 22b ends 5 ticks sooner, the zealot of 8 keeps
  6 less life, the zerglings of 29 lose 1.8 more.
- **The Immortal fires a tick later** (28 → 29; 2a, 2b, 3, 28, 30): fights end later by 2 to 7 ticks; in 28 the
  zerglings keep 2.8 more alive.
- **The Marauder fires a tick later** (26 → 27; 4, 12b, 31): battle 31 (15 marauders against 15 stalkers, with the
  Stalker a tick faster) tips further to the stalkers: 4 of 20 seeds change from a marauder win to a stalker win
  (survivors 2.65 – 0.70 → 1.70 – 1.35); its expectation, written as a close fight, holds on every seed.
- **The workers swing every 27 ticks** (22a): the reaper keeps 9 more life against two SCVs.
- The Thor's air period, the Liberator's, the Baneling's (it dies on its first attack) and the Ravager's move no
  matrix battle.

**Battle 33, 68 seeds: bit-identical.** **`golden_replay.ron` re-recorded from checkpoint 4** (ticks 151 to 200, the
`rhino_beetle` squad's first fight); `golden_replay_orders.ron` unchanged. Debug, release and `wasm32-wasip1`
reproduce both fixtures. Doc 07's period cells for the eleven identities are corrected (the Roach's already said 36).

### 13.15 W3: the damage point copied in Normal time (class B)

`damage_point_ticks` 4 → **3** on `welder_drone` (weapon) and `railgun_crawler` (tank-mode weapon and the Siege
Mode override): the `CWeapon` default 0.167 Normal s (`core` WeaponData:17) is 2.98 ticks; 4 was 0.167 × 25, the
"0.16 s" frontswing plan 17 took as Faster seconds. Measured: **one matrix battle moved, no winner and no expectation
changed**: `22a_reaper_vs_2_workers` (the workers' blows land a tick sooner in each swing; the reaper ends with 46.32
life instead of 46.24, on all 20 seeds). Battle 3's tank releases each shot a tick sooner after its swing starts but
lands the same five shots (bit-identical). Battle 33 bit-identical. **`golden_replay.ron` re-recorded from checkpoint 4** (ticks 151 to
200, the `railgun_crawler` squad's first fight); `golden_replay_orders.ron` unchanged. Debug, release and
`wasm32-wasip1` reproduce both. No doc 07 or doc 08 cell states the damage point.

### 13.16 W4: damage points that were never sourced

40 values on 29 identities (the audit's group W4, every one with its source line): most weapons go to the `CWeapon`
default 0.167 Normal s = **3 ticks** (`core` WeaponData:17), from 0 to 2; those whose weapon sets its own value follow
it: **0** for the Marauder, Reaper, Baneling, Lurker, Mutalisk (`shock_golem`, `jet_jumper`, `volatile_tick`,
`burrow_scorpion`, `sonic_bat`; 1 before), **4** for the Hellion (`napalm_wheeler`, 0.25 s; 0 before) and the Ravager
(`mortar_stag`, 0.2 s), **2** for the Liberator's Defender mode (`sky_bastion` ground, 0.125 s), **1** for the
Colossus (`tripod_annihilator`, 0.0832 s; 0 before), **6** for the Dark Templar, the Ultralisk and the Spine Crawler
(`shadow_visitor` 0.3613 s, `stag_behemoth` 0.3332 s, `thorn_lasher_dome` 0.3332 s) and **15** for the Thor's ground
weapon (`colossus_mech`, 0.831 s; 2 before). Every new damage point is below its weapon's period, also under the
shipped attack-speed modifiers (the trace's release inference, plan §7). One commit for the whole group: the cause is
the same for every value, and the measurement can be explained by identity (plan WP12 step 4 allows a per-faction
split; it was not needed).

**Whole matrix, 20 seeds.** Compared with the state after W3: **37 battles moved; no expectation failed** (0 on the
seeds of record; 7 over all seeds, battle 33's, unchanged); winners changed only in the coin-flip battles 31 (7 seeds,
6 towards the marauders: the Marauder now releases on the tick its swing starts) and 32 (5 seeds; its expectation
allows either side).

| Battle | Seeds moved | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|
| 1b_stalker_vs_2_roaches | 20 | 0.00 – 2.00, same | 196.6 → 197.6 | — / 100.39 (20) → — / 100.40 (20) |
| 2a_immortal_vs_roach | 20 | 1.00 – 0.00, same | 63.0 → 65.0 | 200.00 (20) / —, same |
| 2b_immortal_vs_2_roaches | 20 | 1.00 – 0.00, same | 150.0 → 152.0 | 200.00 (20) / —, same |
| 3_immortal_vs_tank | 20 | 1.00 – 0.00, same | 92.0 → 94.0 | 176.00 (20) / —, same |
| 4_marauder_vs_roach | 20 | 1.00 – 0.00, same | 197.1 → 196.1 | 35.00 (20) / —, same |
| 5a_hydra_vs_marine | 20 | 1.00 – 0.00, same | 52.0 → 54.0 | 66.77 (20) / — → 66.80 (20) / — |
| 5b_hydra_vs_2_marines | 20 | 1.00 – 0.00, same | 112.0 → 114.0 | 19.68 (20) / — → 19.71 (20) / — |
| 6a_roach_vs_hydra | 20 | 1.00 – 0.00, same | 189.5 → 190.5 | 4.81 (20) / — → 4.79 (20) / — |
| 6b_2_roaches_vs_hydra | 20 | 2.00 – 0.00, same | 81.8 → 82.8 | 112.58 (20) / — → 113.67 (20) / — |
| 6c_roach_vs_2_hydras | 20 | 0.00 – 2.00, same | 96.2 → 98.2 | — / 69.00 (20), same |
| 9a_hellion_vs_4_lings | 20 | 1.00 – 0.00, same | 91.0 → 95.0 | 27.80 (20) / — → 23.80 (20) / — |
| 9b_hellion_vs_roach | 20 | 0.00 – 1.00, same | 188.0 → 189.0 | — / 112.86 (20) → — / 112.82 (20) |
| 10_2_hellions_vs_8_lings | 20 | 2.00 – 0.00, same | 55.0 → 59.0 | 70.00 (20) / —, same |
| 11a_2_banelings_vs_5_marines | 20 | 0.00 – 2.25, same | 9.1 → 8.1 | — / 10.00 (9), same |
| 11b_2_banelings_vs_marauder | 20 | 0.00 – 1.00, same | 9.1 → 8.0 | — / 95.00 (20), same |
| 12a_colossus_vs_6_marines | 20 | 1.00 – 0.00, same | 80.7 → 81.7 | 238.15 (20) / —, same |
| 12b_colossus_vs_3_marauders | 20 | 0.00 – 2.00, same | 170.0 → 169.0 | — / 67.53 (19), same |
| 14_thor_vs_ultralisk | 20 | 1.00 – 0.00, same | 187.0 → 200.0 | 37.00 (20) / —, same |
| 15_plated_ultralisk_vs_8_marines | 20 | 1.00 – 0.00, same | 111.0 → 116.0 | 414.77 (20) / — → 413.25 (20) / — |
| 16a_muta_vs_marine | 20 | 1.00 – 0.00, same | 115.0 → 114.0 | 78.33 (20) / — → 78.27 (20) / — |
| 17_void_ray_vs_muta | 20 | 1.00 – 0.00, same | 190.0 → 193.0 | 150.00 (20) / —, same |
| 18_void_ray_vs_corruptor | 20 | 1.00 – 0.00, same | 226.0 → 229.0 | 110.00 (20) / —, same |
| 20_brood_lord_vs_roach | 20 | 1.00 – 0.00, same | 142.2 → 143.2 | 225.00 (20) / —, same |
| 21_lurker_vs_6_marines | 20 | 1.00 – 0.00, same | 106.4 → 103.6 | 71.10 (20) / — → 72.81 (20) / — |
| 22a_reaper_vs_2_workers | 20 | 1.00 – 0.00, same | 102.0 → 101.0 | 46.32 (20) / — → 46.24 (20) / — |
| 23a_zealot_vs_6_lings | 20 | 0.00 – 2.00, same | 269.2 → 271.3 | — / —, same |
| 23b_upgraded_zealot_vs_6_lings | 14 | 1.00 – 0.00, same | 245.6, same | 60.00 (20) / — → 62.80 (20) / — |
| 24a_roach_vs_5_lings | 20 | 0.00 – 3.00, same | 285.4 → 286.7 | — / —, same |
| 24b_upgraded_roach_vs_5_lings | 20 | 1.00 – 0.00, same | 327.9 → 328.8 | 23.03 (20) / — → 24.03 (20) / — |
| 26_100_lings_vs_6_colossi | 18 | 0.00 – 6.00, same | 114.8 → 116.1 | — / 250.00 (20), same |
| 27_100_lings_vs_8_ultralisks | 20 | 0.00 – 8.00, same | 203.1 → 210.9 | — / 432.04 (20) → — / 423.59 (20) |
| 28_100_lings_vs_10_immortals | 20 | 30.10 – 0.00 → 30.45 – 0.00 | 660.4 → 666.1 | — / — → 30.05 (4) / — |
| 29_100_lings_vs_12_stalkers | 19 | 69.70 – 0.00 → 69.50 – 0.00 | 442.3 → 445.9 | 33.09 (19) / — → 31.90 (20) / — |
| 30_20_roaches_vs_10_immortals | 20 | 0.00 – 10.00, same | 255.1 → 259.4 | — / 132.15 (20) → — / 133.10 (20) |
| 31_15_marauders_vs_15_stalkers | 20 | 1.70 – 1.35 → 2.65 – 0.75 | 700.2 → 680.5 | 69.75 (4) / — → 68.90 (10) / — |
| 32_30_marines_vs_15_banelings | 19 | 0.90 – 0.90 → 1.45 – 0.95 | 52.8 → 49.9 | 18.75 (4) / — → 27.50 (4) / — |
| 34_20_hydras_vs_12_mutas | 18 | 19.10 – 0.00 → 19.05 – 0.00 | 192.6 → 195.3 | 90.00 (20) / —, same |

By cause (a longer wind-up holds a unit still for longer and delays its first hit; a damage point of 0 releases on the
tick the swing starts, D-3):

- **Roach 2 → 3, Immortal 1 → 3, Hydralisk 1 → 3, Zergling 2 → 3, Brood Lord 2 → 3, Void Ray 0 → 3** (1b, 2a, 2b, 3,
  5a, 5b, 6a to 6c, 9b, 17, 18, 20, 23a, 23b, 24a, 24b, 26 to 30, 34): first hits land one to three ticks later, so
  fights end later by 1 to 6 ticks; winners do not move, survivor means by at most 0.35 and lead-unit life by at
  most 2.8 (the zealot of 23b, which the slower zerglings hit later).
- **Hellion 0 → 4** (9a, 9b, 10): the hellions hold still four ticks per swing; against zerglings (9a, 10) fights end
  4 ticks later and the hellion of 9a ends with 23.80 life instead of 27.80.
- **Thor 2 → 15 and Ultralisk 1 → 6** (14, 15, 27): 14 now runs to its 200-tick limit (187 before) with the same
  winner and life; the ultralisks of 15 and 27 end with 1 to 9 less life, fights 5 to 8 ticks longer.
- **Colossus 0 → 1** (12a, 12b, 26): one tick per swing; ends move by about 1 tick.
- **Marauder, Reaper, Baneling, Lurker, Mutalisk 1 → 0** (4, 11a, 11b, 16a, 21, 22a, 31, 32): these release a tick
  sooner. The banelings of 11a, 11b and 32 burst a tick sooner (end tick 9.1 → 8.1); the lurker of 21 ends with 1.71
  more life; in 31 the marauders win on 6 more seeds and lose 1 (survivors 1.70 – 1.35 → 2.65 – 0.75), within the
  battle's close-fight expectation; in 32 (a coin flip by design) 5 seeds change between a marine win, a baneling win
  and both gone.

**Battle 33, 68 seeds: bit-identical.** **`golden_replay.ron` re-recorded from checkpoint 4** (ticks 151 to 200, the
`rhino_beetle` squad's first fight; the Roach's damage point is 3); `golden_replay_orders.ron` unchanged. Debug,
release and `wasm32-wasip1` reproduce both. No doc 07 or doc 08 cell states a damage point.

### 13.17 W5: backswings that were never sourced

48 values on 35 identities (the audit's group W5, each with its source line): most weapons go to the `CWeapon`
default 0.5 Normal s = **9 ticks** (`core` WeaponData:18), from 1 to 5; those whose weapon sets its own value follow
it: **0** for the Marauder (`shock_golem`), **13** for the Reaper, the Hellion and the Void Ray (0.75 s), **10** for
the Hydralisk (0.5607 s), **21** for the Ghost (1.167 s) and **24** for the Dark Templar (1.333 s). After WP1 a
backswing never delays the next attack: it locks out new orders and anchors the unit until it ends or a new swing
starts. One commit for the group (one cause; the measurement is small enough to explain per identity).

**Whole matrix, 20 seeds.** Compared with the state after W4: **7 battles moved, no expectation failed** (0 on the
seeds of record; 7 over all seeds, battle 33's, unchanged); winners changed only in battle 31 (4 seeds, 2 each way).

| Battle | Seeds moved | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|
| 10_2_hellions_vs_8_lings | 1 | 2.00 – 0.00, same | 59.0 → 61.2 | 70.00 (20) / —, same |
| 27_100_lings_vs_8_ultralisks | 15 | 0.00 – 8.00, same | 210.9 → 212.0 | — / 423.59 (20) → — / 425.25 (20) |
| 28_100_lings_vs_10_immortals | 20 | 30.45 – 0.00 → 30.05 – 0.00 | 666.1 → 662.7 | 30.05 (4) / — → 17.93 (2) / — |
| 29_100_lings_vs_12_stalkers | 20 | 69.50 – 0.00 → 69.90 – 0.00 | 445.9 → 447.6 | 31.90 (20) / — → 30.82 (17) / — |
| 30_20_roaches_vs_10_immortals | 2 | 0.00 – 10.00, same | 259.4 → 258.6 | — / 133.10 (20) → — / 134.60 (20) |
| 31_15_marauders_vs_15_stalkers | 15 | 2.65 – 0.75 → 2.95 – 0.55 | 680.5 → 682.6 | 68.90 (10) / — → 62.67 (9) / — |
| 34_20_hydras_vs_12_mutas | 1 | 19.05 – 0.00, same | 195.3 → 195.1 | 90.00 (20) / —, same |

- **Crowds** (27, 28, 29, zerglings against ultralisks, immortals and stalkers, every one now anchored 9 ticks after
  each strike instead of 2 to 4): 28 ends with 30.05 zerglings instead of 30.45, 29 with 69.90 instead of 69.50, the
  ultralisks of 27 with 1.66 more life; winners do not move.
- **Battle 31** (15 marauders against 15 stalkers): the Marauder's backswing goes to 0 and the Stalker's to 9; the
  marauders win 2 more seeds and lose 2 (survivors 2.65 – 0.75 → 2.95 – 0.55), within the close-fight expectation.
- **Single seeds** in 10 (hellion backswing 2 → 13: the hellions stand a little longer after a shot), 30 and 34;
  winners and survivors do not move.

Most weapons keep their outcome: after WP1 the backswing only matters where an anchored unit's position or its
lock on new orders changes who reaches whom.

**Battle 33, 68 seeds: bit-identical.** **`golden_replay.ron` re-recorded from checkpoint 4** (ticks 151 to 200,
its `rhino_beetle` and `railgun_crawler` squads' first fights); `golden_replay_orders.ron` unchanged. Debug, release
and `wasm32-wasip1` reproduce both. No doc 07 or doc 08 cell states a backswing.

### 13.18 D2: the worker's backswing (Q0045 2b, class D)

`welder_drone` `backswing_ticks` 6 → **9**: the `CWeapon` default 0.5 Normal s (`core` WeaponData:18), 8.93 ticks.
Plan 17 had chosen 6 for a readable animation (audit §6 item 2); the owner answered Q0045 item 2 with (b), 9. The
other two workers already received 9 in W5. Measured: **whole matrix bit-identical** (battle 22a's two SCVs keep their
outcome: after WP1 the backswing never delays the next swing, and anchoring them three more ticks changes nothing
there), battle 33 bit-identical, both golden replays unchanged in debug, release and `wasm32-wasip1`. No doc 07 or
doc 08 cell states the backswing.

### 13.19 W6: the Stalker's projectile speed copied in Normal time (class B)

`warp_strider` `projectile_speed` (ground and air weapon) 0.75 → **1.05**: the Stalker's missile mover `MaxSpeed`
18.75 Normal units per s × 1.4 ÷ 25 (audit group W6 and §4); 0.75 was 18.75 ÷ 25. Its shots reach their target
sooner.

**Whole matrix, 20 seeds.** Compared with the state after D2: **5 battles moved, no expectation failed** (0 on the
seeds of record; 7 over all seeds, battle 33's, unchanged).

| Battle | Seeds moved | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|
| 1a_stalker_vs_roach | 20 | 1.00 – 0.00, same | 273.0 → 272.0 | 35.00 (20) / —, same |
| 1b_stalker_vs_2_roaches | 9 | 0.00 – 2.00, same | 197.6, same | — / 100.40 (20) → — / 100.41 (20) |
| 22b_reaper_vs_stalker | 20 | 0.00 – 1.00, same | 173.0 → 172.0 | — / 80.00 (20), same |
| 29_100_lings_vs_12_stalkers | 20 | 69.90 – 0.00 → 69.10 – 0.00 | 447.6 → 444.9 | 30.82 (17) / — → 31.49 (14) / — |
| 31_15_marauders_vs_15_stalkers | 20 | 2.95 – 0.55 → 1.25 – 2.05 | 682.6 → 670.4 | 62.67 (9) / — → 31.50 (2) / — |

- **Battle 31** (15 marauders against 15 stalkers) tips to the stalkers: their damage lands earlier in every
  exchange, and on 11 of 20 seeds the winner changes (10 to the stalkers, 1 to the marauders; two seeds of record,
  3203386110 and 33, among them); survivors 2.95 – 0.55 → 1.25 – 2.05. The battle's claim is that equal supply is
  close and its expectation allows either side (`blue: (0, 15), red: (0, 15)`), so it holds on every seed; the
  winner on the seeds of record was never part of it.
- **1a, 1b, 22b, 29**: a tick sooner at most, or a slightly different crowd (29: 69.10 zerglings left instead of
  69.90); winners do not move.

**Battle 33, 68 seeds: bit-identical.** Both golden replays unchanged (neither spawns a stalker) in debug, release and
`wasm32-wasip1`. No doc 07 or doc 08 cell states the projectile speed.

### 13.20 W7: projectile speeds that were never sourced

21 values on 15 identities (the audit's group W7, each with its mover and source line): a missile's speed is its
mover's `MaxSpeed` × 1.4 ÷ 25, and a weapon whose effect chain launches no missile is instant (`None`).

| Change | Identities |
|---|---|
| faster missile | `rhino_beetle` 0.64 → 1.05, `shock_golem` 0.64 → 1.12, `astral_vagrant` 1.0 → 1.05, `mortar_stag` 1.0 → 1.05, `thorn_viper` 0.90 → 2.24 (ground and air), `sonic_bat` 0.50 → 1.05 (ground and air), `spore_spire_dome` 0.80 → 1.05 |
| slower missile | `sky_leviathan` 1.2 → 1.05 (ground and air), `flak_array_dome` 0.80 → 0.56 |
| missile becomes instant (`None`) | `jet_jumper` (weapon), `colossus_mech` (air weapon), `void_goliath` (weapon) |
| instant becomes a missile | `sky_bastion` ground 2.24, `hive_matron` ground and `weapon` 2.24 (air 1.2 → 2.24), `tesla_sentinel_dome` ground and air 1.008 |

**Tests restated first.** Two tests named values this group changes, and pass before and after since the commit
before this one: `combat_correctness_tests.rs::s4_projectile_weapon_applies_every_strike` used the Reaper as "a
two-strike projectile weapon" (its pistols are instant in the game) and now takes the first shipped archetype with a
multi-strike projectile weapon without splash or bonus; `weapon_catalog_rules_tests.rs::s16_projectile_speed_must_be_positive`
edited the text `projectile_speed: Some(0.80)` and now edits the first projectile speed a shipped structure states.

**Whole matrix, 20 seeds.** Compared with the state after W6: **19 battles moved, no expectation failed** (0 on the
seeds of record; 7 over all seeds, battle 33's, unchanged); winners changed only in battle 31 (10 seeds, both ways).

| Battle | Seeds moved | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|
| 1b_stalker_vs_2_roaches | 20 | 0.00 – 2.00, same | 197.6 → 194.9 | — / 100.41 (20) → — / 100.39 (20) |
| 2a_immortal_vs_roach | 20 | 1.00 – 0.00, same | 65.0 → 62.0 | 200.00 (20) / —, same |
| 2b_immortal_vs_2_roaches | 20 | 1.00 – 0.00, same | 152.0 → 149.0 | 200.00 (20) / —, same |
| 3_immortal_vs_tank | 20 | 1.00 – 0.00, same | 94.0 → 91.0 | 176.00 (20) / —, same |
| 4_marauder_vs_roach | 20 | 1.00 – 0.00, same | 196.1 → 193.1 | 35.00 (20) / —, same |
| 5a_hydra_vs_marine | 20 | 1.00 – 0.00, same | 54.0 → 51.0 | 66.80 (20) / — → 66.75 (20) / — |
| 5b_hydra_vs_2_marines | 20 | 1.00 – 0.00, same | 114.0 → 111.0 | 19.71 (20) / — → 19.67 (20) / — |
| 6a_roach_vs_hydra | 20 | 1.00 – 0.00, same | 190.5 → 188.0 | 4.79 (20) / — → 4.80 (20) / — |
| 6b_2_roaches_vs_hydra | 20 | 2.00 – 0.00, same | 82.8 → 80.0 | 113.67 (20) / — → 112.57 (20) / — |
| 6c_roach_vs_2_hydras | 20 | 0.00 – 2.00, same | 98.2 → 95.5 | — / 69.00 (20), same |
| 9b_hellion_vs_roach | 20 | 0.00 – 1.00, same | 189.0 → 187.0 | — / 112.82 (20) → — / 112.79 (20) |
| 12b_colossus_vs_3_marauders | 20 | 0.00 – 2.00, same | 169.0 → 166.0 | — / 67.53 (19), same |
| 16a_muta_vs_marine | 20 | 1.00 – 0.00, same | 114.0 → 111.0 | 78.27 (20) / — → 78.10 (20) / — |
| 24a_roach_vs_5_lings | 17 | 0.00 – 3.00, same | 286.7 → 288.5 | — / —, same |
| 24b_upgraded_roach_vs_5_lings | 17 | 1.00 – 0.00, same | 328.8 → 328.0 | 24.03 (20) / — → 25.42 (20) / — |
| 28_100_lings_vs_10_immortals | 20 | 30.05 – 0.00 → 27.00 – 0.00 | 662.7 → 683.4 | 17.93 (2) / — → 30.66 (4) / — |
| 30_20_roaches_vs_10_immortals | 20 | 0.00 – 10.00, same | 258.6 → 236.9 | — / 134.60 (20) → — / 147.15 (20) |
| 31_15_marauders_vs_15_stalkers | 20 | 1.25 – 2.05 → 1.80 – 1.90 | 670.4 → 666.9 | 31.50 (2) / — → 80.80 (5) / — |
| 34_20_hydras_vs_12_mutas | 20 | 19.05 – 0.00 → 19.40 – 0.00 | 195.1 → 183.7 | 90.00 (20) / —, same |

- **Faster missiles** of the Roach, Marauder, Hydralisk and Mutalisk (1b, 2a, 2b, 4, 5a, 5b, 6a to 6c, 9b, 12b, 16a,
  24a, 24b, 30, 34): the first hits land 2 to 3 ticks sooner, so fights end sooner by about that; winners and
  survivors do not move except 34 (20 hydralisks against 12 mutalisks: 0.35 more hydralisks survive, the fight ends
  11 ticks sooner) and 30 (roaches against immortals: 21.7 ticks sooner, the immortals' lead unit keeps 12.6 more
  life).
- **The Immortal's shot becomes instant** (2a, 2b, 3, 28, 30): fights end sooner (3: 94 → 91); in 28 (100 zerglings
  against 10 immortals) the immortals kill faster and 3.05 fewer zerglings survive (30.05 → 27.00).
- **Battle 31**: the Marauder's faster missile tips it back towards the marauders on 10 seeds, both ways (survivors
  1.25 – 2.05 → 1.80 – 1.90), within its close-fight expectation.
- The Reaper's instant pistols, the Thor's instant air weapon, and the Liberator, Queen, Corruptor (`sky_leviathan`), Spore Crawler,
  Missile Turret and Photon Cannon speeds move no matrix battle; battles 22a and 22b keep their outcome.

With this group the matrix equals the all-corrections measurement of the audit's §7 in every battle except 22a and
22b, the two reaper battles, which differ only through the deferred R1 (§13.11). **Battle 33, 68 seeds:
bit-identical.** **`golden_replay.ron` re-recorded from checkpoint 5** (ticks 201 to 250; its `rhino_beetle` squad's
acid now flies at 1.05); `golden_replay_orders.ron` unchanged. Debug, release and `wasm32-wasip1` reproduce both. No
doc 07 or doc 08 cell states a projectile speed.

### 13.21 WP12 summary: what was applied, what was deferred

- **Applied:** every class B and C value of the audit's groups P1, P2, P3, U1, U2, M1, R3, S1, S2, R2 and W1 to W7,
  one of M2's two values, and the class D backswing of `welder_drone` (D2). W8 moved to WP17 (plan Amendment 1).
- **Deferred, each on a ground of D-7:**
  - **R1** (`jet_jumper` `hp_regen_per_tick` 0.08 → 0.112): the reference needs a mechanism we do not model, a
    life-regeneration delay (§13.11; owner question
    [Q0049](../questions/open/0049-reaper-life-regeneration-delay.md), default: keep 0.08).
  - **M2, `railgun_crawler` Siege Mode `cast_ticks`** (71; the audit's 72): the **verify** failed: the morph has a
    random delay (72 to 81 ticks) and unsieging takes 63, so there is no single reference for our one field (§13.7).

  No value was deferred because it moved an expectation.
- **Expectations re-recorded:** battle `3_immortal_vs_tank` only (W1: blue lead hull 200 → 176, claim rewritten).
  Battle 33 is bit-identical through the whole package (no corrected value belongs to `pulse_bot` or `psi_grey`), so
  its comment is unchanged.
- **Golden replays re-recorded:** `golden_replay.ron` in S1, R2, W1, W2, W3, W4, W5 and W7; `golden_replay_orders.ron`
  in S1. Each re-recording is described in the fixture's header and reproduced in debug, release and `wasm32-wasip1`.
- **Tests restated** in a test-only commit before the data commit that needed it, each passing before and after:
  `upgrades_and_tech_tree_tests::test_metabolic_boost_speed` (U1),
  `combat_correctness_tests::s2_explicit_focus_fire_survives_nearby_enemy` (S1),
  `sc2_battle_benchmarks_tests::test_railgun_crawler_declarative_modes_and_siege_splash` (W1),
  `collision_and_immobility_tests::test_converging_units_respect_speed_cap_and_do_not_vibrate` (W2, with M-21),
  `combat_correctness_tests::s4_projectile_weapon_applies_every_strike` and
  `weapon_catalog_rules_tests::s16_projectile_speed_must_be_positive` (W7).
- **Findings** (remediation plan §4.2): **M-20**, the Immortal's Barrier is not modelled (battle 3); **M-21**, melee
  units waiting out their cooldown at a contact ring are jostled once per weapon cycle.
- The final matrix equals the all-corrections measurement of the audit's §7 in every battle except 22a and 22b, the
  two reaper battles that only the deferred R1 moves.

## 14. WP17: hull turning at StarCraft II rates (Q0045 1b)

Plan D-8 and WP17. Angles in the simulation are `SimAngle` binary angles: 65 536 units per turn, counter-clockwise
from +x in the world frame; rates are authored in degrees per tick and loaded as binary units per tick
(round(degrees × 65 536 ÷ 360), halves up). The research is the audit's §10
([`2026-11-04-catalog-time-audit.md`](2026-11-04-catalog-time-audit.md)); the decisions it led to are plan D-8 item 7.
The baseline is the state after WP12 (`~/scratch/combat_wp17/base/`): 1000 runs, expectation failures 0 on the seeds
of record and 7 over all seeds (all battle 33's); battle 33 on 68 seeds 42 / 18 / 8 (marine wins / zealot wins /
unresolved), marine survivors 4.03 (SD 3.26).

### 14.1 Commit a: the unit and range of both rates

- **Change.** `turn_rate_deg` → `turn_rate_deg_per_tick` and `turret_turn_rate_deg` →
  `turret_turn_rate_deg_per_tick`, without a default, so a catalog with an old name fails to load naming the new
  one. Both rates are degrees per tick, validated to `0 < rate ≤ 180` (a turret rate exactly 0 on a weapon without a
  turret), with typed errors instead of the former silent clamp. Both use one conversion, rounded instead of the hull
  rate's truncation to whole degrees. The turret branch that read values above 360 as degrees per second is gone.
- **Data, rewritten to the same behaviour.** Hull 999, 360 and 280 → 180 (42 units; 180 reaches any facing in one
  tick, as the clamp did). Structure turrets 360 → 180 (6 weapons): raw 65 535 (the saturating cast of 360°) →
  32 768; both turn any facing in one tick. The three unit turrets keep 21.6 (3 932 units, unchanged).
- **Measurement.** Matrix bit-identical to the baseline; battle 33 bit-identical; both golden replays unchanged in
  debug, release and `wasm32-wasip1` (no structure turret's `WeaponSpec` is hashed in either replay: no structure
  fires a projectile there).

### 14.2 Commit b: one hull rotation per tick

- **Change.** `Entity::turn_hull_towards` turns the heading by at most the tick's `hull_turn_budget` (binary units),
  which `SimWorld::step_movement_and_steering` sets to the archetype's `turn_rate` before any system turns a hull.
  The three hull writers of plan §2.5 call it: the motion commit (facing the velocity), `aim_if_in_range` (facing the
  goal, through the same commit) and `slew_towards` for a hull-mounted weapon, which no longer reads the weapon's
  turret rate. The movement phase spends first, the weapon phase gets the rest. The budget is left out of the
  checksum (`fields!(Entity { … } unhashed { hull_turn_budget })`): it is reset before its first read every tick.
  Plan D-8 item 7: `aim_if_in_range` also snapped a turret to its goal; it no longer writes the turret, so the
  weapon phase alone turns a turret, at its rate.
- **Tests first** (`hull_turning_tests.rs`, scratch archetypes): on the old rule 5 of 7 failed (the swing of a unit
  facing away started on tick 2, not 9; one tick turned 32 768 units where the rate allows 3 670, 7 340 or a turret's
  1 820); the 180° guard and the determinism test passed. All 7 pass with the rule.
- **Measurement.** Matrix bit-identical to commit a; battle 33 bit-identical. Every shipped hull rate is 180° per
  tick, so the budget never binds.
- **`golden_replay.ron` re-recorded:** checkpoints 4 to 6 and 9 (ticks 151 to 300 and 401 to 450) changed, because
  the turret angles of its three `railgun_crawler`s (21.6° per tick) now turn at their rate instead of snapping in the
  movement phase, and turret angles are hashed. No behaviour changed: with `turret_angle` left out of the encoding in
  both, every checkpoint of both fixtures on this commit equals commit a's. The siege tank's arc is unrestricted, so
  its turret's angle never gated a shot. `golden_replay_orders.ron` unchanged. Debug, release and `wasm32-wasip1`
  reproduce both.

### 14.3 Commit c: matrix battles spawn facing each other

- **Change.** `SimWorld::spawn_unit_facing(unit_type, owner, pos, heading)` (heading a binary angle,
  counter-clockwise from +x, world frame; hull and turret both start there); `spawn_unit` calls it with heading 0.
  `sim_fixtures::matchup` spawns every unit of a two-sided battle facing the centre of the other side's authored
  grid (before jitter, `side_centre`). The golden replays keep heading 0 (plan D-8 item 5).
- **Test first:** `a_two_group_battle_spawns_each_group_facing_the_others_centre` and
  `a_side_centre_is_the_mean_of_its_grid_points` (`sim_fixtures::matchup`), red until `side_centre` existed.
- **Measurement.** Matrix bit-identical to commit b in outcomes (every hull still turns 180° per tick); battle 33
  bit-identical; both golden replays unchanged in debug, release and `wasm32-wasip1`.

### 14.4 Commit d, hull-mounted arcs: not latent, measured

- **Change.** 40 hull-mounted weapons take the reference tolerance `Arc` ÷ 2 + `ArcSlop` (audit §10.3): 11.25° for
  36, 39.99° for the Void Ray analogue's two (`ArcSlop` 39.9902), 45° for the Mutalisk analogue's two (`ArcSlop` 45).
  Before: 15° for 27, 180° (every direction) for 10 (the melee units, the workers and the Queen's ground weapon),
  360° for 3. Firing half angles now round to the nearest binary unit like the rates (11.25° = 2 048 units; 15° =
  2 731, was 2 730 truncated); the six weapons whose mount differs keep their arcs (plan D-8 item 7). Test first:
  `firing_half_angles_round_to_the_nearest_binary_unit` (`catalog.rs`). Three arc tests were restated first, in the
  commit before (they named `flak_array_dome`'s 360, a weapon at exactly 15° and the text `firing_arc_deg: 360.0`).
- **Not latent, against the plan.** D-8 item 6 expected the hull arcs to change nothing while every hull turns 180°
  per tick. They do change results, for two reasons that the plan did not see:
  1. A weapon that is `Ready` and not aligned enters `RotatingToTarget` and turns, but starts its swing only on the
     next tick, even when one tick's turn aligned it (`step_ready_weapon` slews without checking again;
     `step_rotating_weapon` slews and checks). A melee unit with the old 180° arc never needed to face its target;
     now each new target outside 11.25° costs it at least that tick. This is finding **M-22** (remediation plan
     §4.2): it delays every target switch by one tick beyond the turn itself.
  2. The hull budget is shared (commit b): a unit that turned to face its velocity in the movement phase has less
     than 180° left for the weapon phase in the same tick.

  The cause of every move is still this commit's one change (the arc values); it is measured and re-recorded here.
- **Whole matrix, 20 seeds**, against commit c: 18 battles moved; no winner changed except in battle 31 (a close
  fight with mixed winners) and battle 33 (one seed of record).

| Battle | Seeds moved | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|
| 6b_2_roaches_vs_hydra | 3 | 2.00 – 0.00, same | 80.0 → 80.2 | 112.57 → 112.58 / — |
| 9a_hellion_vs_4_lings | 7 | 1.00 – 0.00, same | 95.0, same | 23.80 → 25.40 / — |
| 10_2_hellions_vs_8_lings | 1 | 2.00 – 0.00, same | 61.2, same | 70.00 → 70.40 / — |
| 11a_2_banelings_vs_5_marines | 3 | 0.00 – 2.25, same | 8.1 → 8.2 | — / 10.00, same |
| 11b_2_banelings_vs_marauder | 9 | 0.00 – 1.00, same | 8.0 → 8.4 | — / 95.00, same |
| 22a_reaper_vs_2_workers | 5 | 1.00 – 0.00, same | 101.0, same | 46.24 → 46.22 / — |
| 23a_zealot_vs_6_lings | 20 | 0.00 – 2.00, same | 271.3 → 275.0 | — |
| 24a_roach_vs_5_lings | 20 | 0.00 – 3.00, same | 288.5 → 289.8 | — |
| 24b_upgraded_roach_vs_5_lings | 6 | 1.00 – 0.00, same | 328.0, same | 25.42 → 26.62 / — |
| 26_100_lings_vs_6_colossi | 1 | 0.00 – 6.00, same | 116.1 → 116.0 | — / 250.00, same |
| 27_100_lings_vs_8_ultralisks | 20 | 0.00 – 8.00, same | 212.0 → 213.2 | — / 425.25 → 427.67 |
| 28_100_lings_vs_10_immortals | 20 | 27.00 → 25.95 – 0.00 | 683.4 → 687.0 | 30.66 (4) → 35.00 (3) / — |
| 29_100_lings_vs_12_stalkers | 20 | 69.10 → 69.20 – 0.00 | 444.9 → 449.0 | 31.49 (14) → 35.00 (16) / — |
| 30_20_roaches_vs_10_immortals | 1 | 0.00 – 10.00, same | 236.9 → 237.0 | — / 147.15 → 147.90 |
| 31_15_marauders_vs_15_stalkers | 15 | 1.80 – 1.90 → 1.85 – 1.45 | 666.9 → 669.4 | 80.80 (5) → 57.00 (4) / — |
| 32_30_marines_vs_15_banelings | 1 | 1.45 – 0.95, same | 49.9 → 50.0 | 27.50, same / — |
| 33_24_marines_vs_12_zealots | 15 | 4.50 – 0.60 → 4.75 – 0.75 | 440.8 → 429.6 | 36.27 (11) → 39.67 (9) / — |
| 34_20_hydras_vs_12_mutas | 4 | 19.40 → 19.45 – 0.00 | 183.7 → 183.3 | 90.00, same / — |

  - The zerglings (23a, 24a, 24b, 26 to 29) and the zealot now turn to each new target before they strike: their
    fights last 1 to 4 ticks longer, and in 28 the immortals kill one more zergling on average.
  - The banelings (11a, 11b) turn before they burst, a fraction of a tick later on average.
  - Battle 31 stays a close fight with both winners (its expectation holds on every seed).
- **Battle 33, 68 seeds:** 43 / 16 / 9 (marine wins / zealot wins / unresolved; was 42 / 18 / 8), marine survivors
  4.38 (SD 3.39; was 4.03, SD 3.26). Seeds of record 7-0, 1-1, 9-0, 6-0 (marines – zealots; seed 11 was 3-0 and is
  now unresolved). Re-recorded per D-6 to the smallest ranges that hold on them, `blue: (1, 9), red: (0, 1)`, and its
  comment rewritten with this distribution.
- **`golden_replay.ron` re-recorded** from checkpoint 4 (ticks 151 to 200, the first fight): its marines, roaches and
  zealots fire within 11.25° (were 15°, 15° and 180°). `golden_replay_orders.ron` unchanged. Debug, release and
  `wasm32-wasip1` reproduce both.

### 14.5 Commit d, turreted arcs

- **Change.** 7 turreted weapons take `Arc` ÷ 2 + `ArcSlop` (audit §10.3): `railgun_crawler`'s Siege Mode override
  (`CrucioShockCannon`, no `Arc`) 360 → 11.25°; `planetary_bulwark_dome` (`TwinIbiksCannon`, `Arc` 90, `liberty`
  WeaponData:138) 360 → 56.25°; `flak_array_dome`, `tesla_sentinel_dome` (ground and air), `thorn_lasher_dome` and
  `spore_spire_dome` 360 → 11.25°. The unsieged tank's `90mmCannons` keeps 360 (`Arc` 360, class A).
- **Measurement.** Matrix bit-identical to the hull-arc commit (no matrix battle sieges a tank or has a structure);
  battle 33 bit-identical; both golden replays unchanged in debug, release and `wasm32-wasip1` (their tanks never
  siege, and no structure fires).

### 14.6 Commit e: turret rates (audit group W8)

- **Change** (degrees per tick, audit §10.4): `railgun_crawler`'s turret 21.6 → 40.32 (`SiegeTank`, `YawRate` 720
  through `FreeRotate`) and its Siege Mode turret 21.6 → 20.16 (`SiegeTankSieged`, 360, `liberty` TurretData:82);
  `planetary_bulwark_dome` 180 → 5.04 (90, `liberty` TurretData:89); `flak_array_dome` and `tesla_sentinel_dome`
  (ground and air) 180 → 40.32; `thorn_lasher_dome` and `spore_spire_dome` 180 → 20.16 (360, `liberty`
  TurretData:62, :70). `colossus_mech`'s air turret keeps 21.6 (no counterpart turret).
- **Measurement.** Matrix bit-identical to the turreted-arc commit, against the plan's expectation that battles with
  `railgun_crawler` would move: the unsieged tank's arc is unrestricted (`Arc` 360, class A), so its turret angle
  never gates a shot, and no matrix battle sieges a tank or has a structure. Battle 33 bit-identical.
- **`golden_replay.ron` re-recorded:** only checkpoint 4 (ticks 151 to 200) changed, because its three tanks' turret
  angles (hashed) now turn at 40.32° per tick. No behaviour changed: with `turret_angle` left out of the encoding,
  every checkpoint of both fixtures equals the turreted-arc commit's. `golden_replay_orders.ron` unchanged. Debug,
  release and `wasm32-wasip1` reproduce both.
