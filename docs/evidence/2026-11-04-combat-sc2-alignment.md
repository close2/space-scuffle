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
