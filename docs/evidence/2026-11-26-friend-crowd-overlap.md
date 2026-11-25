# Evidence: Dense Friendly Crowds and the Contact Solver's Crowd Extension (B-219)

Measurements of plan [`2026-11-26-friend-crowd-overlap`](../plans/2026-11-26-friend-crowd-overlap.md): WP0's
measurement repair, base of record and close-battle list; WP1's latent extension C-3′, its red runs, its derived cap
and its byte identity; and WP2's switch-over, measured against the WP0 base and stopped by the plan's pre-registered
rules (§5.6). Scratch material (trees, binaries, worlds, scripts, logs, red runs) is under
`~/scratch/friend-crowd/wp/` and `~/scratch/friend-crowd/red/`, never committed.

## 1. Method

- **Trees.** Each arm is a `git archive` copy of a commit under `~/scratch/friend-crowd/wp/trees/<tag>`, built in a
  target directory of its own (`scripts/build.sh`: release `matchup_trace`, `movement_harness`, `route_trace`, and one
  single-scenario throughput binary per scenario, 400 `pulse_bot` bodies for 300 ticks). Each arm's assets are copied
  twice, once per pipeline (`scripts/assets.sh`).
- **Worlds** (`scripts/measure.sh`, the collision-classes plan's script): the whole matrix on the four seeds of record
  and the tool's first 20 others (24 seeds; `--seeds 20`), both pipelines; battle 31 on 100 seeds and battle 33 on 64;
  the showcase and the showcase-effects scenario on the default seed and seeds 1 to 8 (1,500 ticks); the harness
  families gap, arc, cross, corridor, corner (development seeds 1 to 1,000, both pipelines, and relabelled) and mirror.
  Held-out (`measure.sh <arm> <arm> held`): the matrix on the tool's 21st to 44th non-record seeds (23 to 32 and 34 to
  47), both pipelines, and the harness on 9,000,001 to 9,001,000. Every command runs under `flock ~/scratch/gate.lock`.
- **Checks** (`scripts/checks.py`): F-1, F-2 and B1 to B8 of plan §5.2 and §5.3, as pre-registered, read from the
  matrix columns WP0 adds; "beyond noise" is the candidate's per-seed mean outside the base's 95 % bootstrap interval
  of the per-seed mean (2,000 resamples, seed 7), on the worse side. Harness T1 to T9: `movement_harness --aggregate`
  (the solver plan's §6.5 verdicts), the base against itself and the candidate against the base.
- **Throughput:** `perf stat -e instructions:u`, three runs per scenario and pipeline (`scripts/perf.sh`). The host's
  load average was 20 to 130 throughout (another user's builds), so ticks per second are not figures of record and
  C-0 could not be measured (§5.4).

## 2. WP0: the measurement repair (commit `a4262f83`)

`ContactMeter` (`crates/sim_fixtures/src/trace/contact_checks.rs`) now counts B6 from the first stepped tick; the
spawn-inclusive maximum moved to the new column `friend_overlap_with_spawn`. It adds B6 over the pairs the solver
corrects (friend pairs not both holding: `friend_overlap_corrected`, `friend_overlap_corrected_ticks`), B8's probe
of the collision-classes plan §10.1 (a) per side (`blue/red_standing_able`, and its cross-class part
`blue/red_standing_able_cross`, a body whose collision class does not touch the unit's), and C-3′'s counters read
from the world (`solver_extended_ticks`, `solver_extended_iterations`: 0 on any tree without the extension). The nine
columns come last in `matrix.csv`; a table written before them (`CONTACT_CSV_HEADER`) still parses.

**Red runs** (`~/scratch/friend-crowd/red/wp0-red.txt`). The three new tests in `crates/sim_fixtures/tests/trace_tests.rs`
were run against the meter with `main`'s counting restored (the spawn observation counted, every friend pair counted
as corrected, B8 not requiring the weapon still ready after the tick):

| Test | Red against `main`'s counting | Green |
|---|---|---|
| W0-T8 `friend_overlap_is_counted_from_the_first_stepped_tick` | 1 pair-tick, maximum 30.0 % (the spawn) | 0 pair-ticks, maximum below 5 %; 30.0 % in the spawn column |
| W0-T9 `holding_friends_count_in_the_all_pairs_columns_only` | 13 corrected pair-ticks (the spawn tick and 12 ticks of two holding friends) | 12 all-pairs pair-ticks, 0 corrected |
| B8 `standing_while_able_counts_idle_ticks_and_not_swings` | a tick in which the unit swung counted (2 against 1) | swings never count |

**No world moved.** WP0's 24-seed matrix (`trace-wp0`) against `main`'s (the collision-classes agent's
`worlds/main`, built from a tree identical to `39010a68`'s `crates/` and `assets/`; `scripts/wp0_identity.py`):
1,464 rows in each pipeline, every cell equal but `friend_overlap` and `friend_overlap_ticks` (changed in 703 solver
rows and 696 Legacy rows), and `main`'s spawn-inclusive `friend_overlap` equal to WP0's `friend_overlap_with_spawn` in
every row. Every harness CSV (six families and the relabelled runs) is byte-identical to `main`'s.

## 3. WP0: the base of record

WP0's tree, measured as §1 says (`worlds/wp0/`). The matrix's expectation failures: solver 2 on the seeds of record,
32 over all seeds; Legacy 3 and 22; held-out solver 0 and 20, Legacy 0 and 30.

**Friend overlap from the first stepped tick, over pairs the solver corrects** (24 seeds; the battles of F-1's set
and those the plan's prediction named):

| Battle | Seeds | Blue / red wins | Corrected pair-ticks > 5 % per seed | Largest corrected | All-pairs pair-ticks per seed | Largest all pairs | Largest with spawn | B8 blue / red per seed |
|---|---|---|---|---|---|---|---|---|
| 11a_2_banelings_vs_5_marines | 24 | 0 / 10 | 0.0 | 0.4 % | 72.6 | 94.5 % | 94.5 % | 0.0 / 0.0 |
| 13_storm_vs_held_lings | 24 | 24 / 0 | 0.0 | 0.0 % | 536.0 | 73.1 % | 73.1 % | 9.0 / 0.0 |
| 26_100_lings_vs_6_colossi | 24 | 20 / 4 | 1008.4 | 43.6 % | 1008.4 | 43.6 % | 43.6 % | 0.0 / 0.0 |
| 27_100_lings_vs_8_ultralisks | 24 | 0 / 24 | 413.4 | 28.9 % | 413.4 | 28.9 % | 37.0 % | 0.0 / 57.4 |
| 28_100_lings_vs_10_immortals | 24 | 6 / 18 | 253.2 | 28.4 % | 253.2 | 28.4 % | 37.0 % | 0.0 / 0.0 |
| 29_100_lings_vs_12_stalkers | 24 | 24 / 0 | 514.0 | 26.7 % | 514.0 | 26.7 % | 37.0 % | 0.0 / 0.0 |
| 30_20_roaches_vs_10_immortals | 24 | 0 / 24 | 1.8 | 11.7 % | 1.8 | 11.7 % | 13.6 % | 0.0 / 0.0 |
| 31_15_marauders_vs_15_stalkers | 24 | 20 / 4 | 2.1 | 7.7 % | 2.1 | 7.7 % | 12.5 % | 0.0 / 0.0 |
| 33_24_marines_vs_12_zealots | 24 | 24 / 0 | 0.4 | 6.0 % | 0.4 | 6.0 % | 23.1 % | 0.0 / 0.0 |
| 34_20_hydras_vs_12_mutas | 24 | 24 / 0 | 3.4 | 9.8 % | 3.4 | 9.8 % | 37.9 % | 0.0 / 0.0 |
| 35_12_vikings_vs_6_colossi | 24 | 2 / 0 | 0.2 | 5.7 % | 0.2 | 5.7 % | 13.7 % | 93.3 / 0.0 |
| 38a_fungal_turns_zerglings | 24 | 24 / 0 | 0.0 | 5.4 % | 390.6 | 22.2 % | 22.2 % | 0.0 / 0.0 |
| 38b_marines_beat_zerglings_without_fungal | 24 | 0 / 7 | 0.1 | 5.9 % | 627.4 | 22.2 % | 22.2 % | 0.0 / 0.0 |

Every other battle has 0.0 corrected pair-ticks per seed. **F-1's set** (at least 10 corrected pair-ticks above 5 %
per seed) is battles **26, 27, 28 and 29**; the plan's prediction of 13 battles was read from the all-pairs,
spawn-inclusive meter, and 11a, 13, 38a, 38b, 39a, 39b and 40 are pairs of holding friends (C-2's "none", which no
solver iteration touches). On the held-out seeds F-1's set is the same: 26 (1,099.2 per seed, largest 34.6 %), 27
(351.1, 26.0 %), 28 (314.6, 25.2 %), 29 (530.3, 29.3 %).

**Close-battle list (Class R), fixed from this base before WP1's first build** (the collision-classes rule:
`~/scratch/collision-classes/scripts/close_list.py` on this base): 8, 9b, 10, 11a, 14, 21, 22a, 26, 27, 28, 31, 32,
33, 35, 36a, 36b, 37a, 37b, 38a, 38b, 39a, 39b, 42 (23 of 61 battles). Every other battle is Class D, battle 29
among them.

**Harness base** (solver pipeline, development seeds; scenes whose `fo_move_max` exceeds 5 %): gap 365, arc 17,
cross 8, corridor 0, corner 145. The base itself fails T1 (arc, cross, gap), T4 (every family) and T7 (arc, corner,
gap); it meets T2, T3, T5, T6, T8 and T9.

**Throughput base** (instructions, three runs; ticks per second at load 20 to 130, not of record):

| Scenario | Solver | Legacy | Solver / Legacy |
|---|---:|---:|---:|
| 64 × 64 | 16.367 G | 9.797 G | +67.1 % |
| 128 × 128 | 10.971 G | 10.199 G | +7.6 % |
| canyon | 27.320 G | 17.397 G | +57.0 % |

## 4. WP1: C-3′ latent (commit `9ca4631b`)

`ContactSolver::solve` runs C-3's K averaged iterations as before; past K it keeps iterating while the friends'
summed excess over D-α's moving bound is positive and fell in the last iteration, up to `contact.crowd_iterations`.
The excess is summed inside `accumulate` (a cached correction carries its pair's excess). The shipped
`movement_rules.ron` has `crowd_iterations: 21` (equal to `iterations`: off) and `crowd_depth: 12`.

**The derived cap (P-1, C-3′a).** The shipped catalog's crowd, 12 rows deep (`crowd_depth`) and 12 wide in a
channel of terrain, radius 0.375 (the smallest ground radius), every walker stepped by `ρ · r` (ρ = 0.56, C-3's
`jet_jumper` pair) toward a row of 12 holding bodies for 100 ticks, judged from tick 25: K = 21 alone leaves
**59.2 %**; the smallest passing cap is **151** (worst 4.99 %; 150 gives 5.05 %). P-6: with the validation's largest
cap (1,000) the scene's ticks use 149.5 iterations on average and 151 at most. The value was re-derived after
merging `main` `c5f55598` (collision classes): 151 again.

**Red runs** (`~/scratch/friend-crowd/red/`):

| Mutation of the WP1 solver | Tests that fail |
|---|---|
| No extension (the loop ends at K: WP0's solver) | P-1 "no cap up to 1000 keeps the crowd within the bound"; P-2 (the crowd ends above the bound); P-3 (no solve extended) |
| Extension without the progress test | P-7: 30 iterations against K = 21 on the collinear wedge |

**Byte identity, extension off.** Goldens and the battle sandbox green on native debug and release and on
`wasm32-wasip1` release and debug (`scripts/proof.sh` on the WP1 tree: `golden_replay_tests` 4 of 4,
`fixture_tests` 3 of 3 in each). `contact_solver_tests::the_catalogs_crowd_cap_reaches_the_solver`: battle 26's
first 150 ticks with a test catalog whose cap equals K give, tick by tick, the shipped catalog's checksums.

**F-4, first half** (a world in which the extension never runs is byte-identical to the base): in the switched-on
matrix of §5, all 1,320 rows whose `solver_extended_ticks` is 0 are identical to the base in every column; the
harness corridor family (at K on 74 % of its ticks, all within the bound) is byte-identical; every Legacy world is
byte-identical.

## 5. WP2: the switch-over, measured and stopped

Q0118 was answered (a) on 2026-11-25. WP2's tree is the branch head `ba3e639b` (`main` `b4a99fae` merged) with
`crowd_iterations: 151`, the value P-1 derives. Its worlds were first measured from the WP1 binaries with a scratch
catalog holding the same value (`wp1on`); WP2's own tree is measured in §5.5.

### 5.1 F-1 to F-3 (§5.2)

| Battle | Wins base → C-3′ | Corrected pair-ticks > 5 % per seed | Largest | Extended ticks per seed | Verdict |
|---|---|---|---|---|---|
| 26 | 20 / 4 → 17 / 7 | 1,008.4 → 10.1 (−99.0 %) | 43.6 → 7.9 % | 79.8 | F-1, F-2 pass |
| 27 | 0 / 24 → 0 / 24 | 413.4 → 4.6 (−98.9 %) | 28.9 → **11.0 %** | 45.1 | **F-2 fails** (above 10 %) |
| 28 | 6 / 18 → 8 / 16 | 253.2 → 3.3 (−98.7 %) | 28.4 → 8.8 % | 40.5 | F-1, F-2 pass |
| 29 | 24 / 0 → 24 / 0 | 514.0 → 5.7 (−98.9 %) | 26.7 → 7.3 % | 59.1 | F-1, F-2 pass |

No other battle's corrected pair-ticks or largest overlap rose. **Stop rule 3's one amendment does not apply:** the
matrix with the cap at 1,000 (`worlds/cap1000a`) is byte-identical to the matrix at 151, so no tick reached the cap
and battle 27's 11.0 % is left where the progress test ended a solve, not by an under-derived `crowd_depth`.

**F-3, harness** (scenes with `fo_move_max` above 5 %, base → C-3′): gap 365 → 281, arc 17 → 8 (below the base's
interval), cross 8 → 1, corridor 0 → 0 (byte-identical), corner 145 → 70 (below). F-3 passes.

### 5.2 B1 to B8 (§5.3) and the battles' outcomes

| Check | Battle | Base mean [95 % interval] | C-3′ |
|---|---|---|---|
| **B1** free, blue | 27 | 602.3 [455.7, 766.5] | 980.8 |
| **B1** blocked, blue | 27 | 1,000.1 [800.0, 1,211.1] | 1,403.3 |
| **B1** blocked, red | 31 | 39.1 [11.3, 76.5] | 96.6 |
| **B4** unexplained churn, blue | 28 | 3.67 [2.75, 4.62] | 4.79 |
| **B7′** shots per engaged second, blue | 31 | 0.9835 [0.9791, 0.9877] | 0.9777 |
| **B7′** shots per engaged second, blue | 34 | 1.7287 [1.7205, 1.7375] | 1.7202 |

B2, B3, B5 and B8 pass in every battle; B8's cross-class part is 0 everywhere; B9 is 0 by construction.

**Outcomes** (rows of the outcome columns that differ from the base): 26 (24 seeds), 27 (24), 28 (23), 29 (24), 30
(4), 31 (9), 33 (5), 34 (22), 38a (1), 38b (1). Winners changed in 26 (blue 20 → 17 of 24), 28 (6 → 8) and 31
(20 → 19). Battles 29, 30 and 34 are **Class D and moved** (their winners and mean survivors stay within the base's
intervals: 29 blue survivors 63.5 → 64.4, 30 unchanged, 34 19.1 → 18.8).

**Harness T1 to T9:** the base meets T2, T3, T5, T6, T8 and T9, and C-3′ meets them. Of those the base fails, T4's
corner family gets worse (deadlocks and give-ups 7 → 9) and T7's largest friend overlap rises in arc (49.7 → 51.8 %)
and corner (52.0 → 69.1 %); T1 and the other T4 and T7 figures stay level (gap's give-ups fall 50 → 43). The arc
family's idle reversals (E4) rise 4.65 → 12.24 per 1,000 idle moving unit-ticks (reported).

**Showcases:** the solver pipeline's showcase moves on 7 of 9 seeds (a crowd at the west ramp's foot); the
showcase-effects scenario and every Legacy route are byte-identical.

### 5.3 Verdict: stop

By §5.6 and the orchestrator's instruction (any failed check, or a move outside Class R, is a stop):

1. **Stop rule 3: F-2 fails in battle 27** (11.0 % against 10 %). The only allowed amendment, raising `crowd_depth`,
   does not apply: the cap is never reached in the matrix.
2. **Stop rule 4: B1 (27, 31), B4 (28) and B7′ (31, 34) fail;** B7′'s two failures fall below the base's lower bound by 0.0013 and 0.0003
   shots per engaged second, intervals only 0.009 and 0.017 wide.
3. **Stop rule 4: Class D battles 29, 30 and 34 moved.**
4. **Harness:** T4 (corner) and T7's maxima (arc, corner) worsen where the base already fails them.

So the switch-over is **not committed**: the shipped `movement_rules.ron` keeps `crowd_iterations: 21`, no claim is
restated, no golden is re-recorded, B-219 stays open, and WP3 (which the plan runs after WP2 is on `main`) was not
started. Q0118 stays open: its answer (a) is not carried out yet.

### 5.4 Cost of the switch-over (for the owner's canyon headroom)

| Scenario | WP0 base | C-3′ (151) | Change | Solver / Legacy |
|---|---:|---:|---:|---:|
| 64 × 64 | 16.367 G | 24.091 G | **+47.2 %** | +145.9 % |
| 128 × 128 | 10.971 G | 10.976 G | +0.04 % | +7.6 % |
| canyon | 27.320 G | 39.047 G | **+42.9 %** | +124.4 % |

C-1 (at most +60 % on 64 × 64 and canyon, +2 % on 128 × 128) holds. **C-0 could not be measured:** the host's load
average stayed between 20 and 130. Projected from the solver plan's quiet-host figures (111, 183 and 80 ticks per
second) and the instruction ratios: 64 × 64 about **75**, 128 × 128 about 183, canyon about **56** ticks per second,
a margin of about **6** ticks per second (12 %) over the floor of 50 on canyon. The measurement of record needs a
host below load 8.

### 5.5 WP2's own tree

WP2's tree (`crowd_iterations: 151` on `ba3e639b`, `main` `c5f55598` and `b4a99fae` merged) built and run on the
24-seed solver matrix: `matrix.csv` is byte-identical to the switched-on arm's, so §5.1 to §5.3 are WP2's figures.

## 6. Byte identity against `main` `c5f55598` (collision classes)

`main` `c5f55598` merged collision classes, which change the solver's pair builder. Its 24-seed solver and Legacy
matrices are byte-identical to the earlier `main`'s, and so equal to WP0's in every column `main` writes
(`scripts/wp0_identity.py`: 0 cells moved). The merged head `da000a17` (WP0, WP1 latent, `c5f55598`) was then
measured in full (§1, without the held-out runs) and compared file by file with WP0's base (`scripts/compare.sh wp0
head`): **979 files, 0 differences** (matrix CSVs with every WP0 column, battles 31 and 33, 18 showcase routes per
pipeline, every harness CSV of both pipelines). Goldens and the sandbox pass on the merged tree, and `cargo gate quick`
is green. The latent extension leaves every world of the new `main` byte-identical.

Instructions on the head (three runs): 64 × 64 16.437 G (+0.43 % over WP0), 128 × 128 11.002 G (+0.28 %), canyon
27.374 G (+0.20 %); Legacy rises alike (+0.45 %, +0.28 %, +0.37 %), so the change is the merged collision classes',
not the latent extension's (which adds one addition per corrected pair).

## 7. Deviations from the plan, recorded (Q0002)

- **Test places.** P-1, P-2, P-3, P-5, P-6 and P-7 are unit tests of `movement::solver` (the plan names
  `contact_solver_tests` for P-1 and P-5): `ContactSolver` is crate-private, as is C-3's own derivation test.
  `contact_solver_tests.rs` holds two world-level tests instead: the extension ships off, and a test catalog's cap
  reaches a running world's solver (off equals shipped tick by tick).
- **P-1's crowd stands in a terrain channel** 12 bodies wide, so that it stays `crowd_depth` deep; without side walls
  a pressed crowd spreads sideways and is no longer the depth the cap is derived for. It is judged over pairs the
  solver corrects (not both holding), as WP0's columns.
- **P-5 accepts the switched-off value** (`crowd_iterations` equal to `iterations`); with the extension on it requires
  P-1's scene to pass at the shipped cap. Shipped latent, it would otherwise fail until WP2.
- **P-7 is a collinear wedge** (holder, walker, presser, holder on one line with too little room): the plan's
  two-dimensional wedge passed with the progress test removed (the early stop ended it), so it did not test the
  progress test. On a line the overlaps always sum to the same length, so the excess cannot fall; the extension
  stops at K + 1, and without the progress test it runs to 30.
- **Red runs by mutation:** the tests need WP0's and WP1's new API, so their red runs are the new tests against the
  new code with `main`'s behaviour restored (§2, §4), not against `main` itself.
- **C-3′'s counters are WP0 columns,** read from `SimWorld::contact_iterations` (0 extended ticks on any tree without
  the extension), so the matrix header changes once.
- **B3's enemy columns still count the starting state;** the plan's repair names B6 only.
- **With the extension on, a tick that reaches K runs one more accumulation** to learn the excess after iteration K;
  when no pair is above the bound its corrections are discarded (positions unchanged; the cached pairs it leaves are
  exact for those positions, so the hard passes are unchanged). Its cost is inside §5.4's figures.
- **B-219 and B-220 were registered as provisional in WP0** (the plan registers them in WP2).
