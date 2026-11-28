# Evidence: Collision Classes as Data (B-209, B-212)

Measurements of plan [`2026-11-25-collision-classes`](../plans/2026-11-25-collision-classes.md). Branch
`collision-classes-plan`, whose head `0d3c6d0b` merges `main` `f390c4a2` (B-208, the solver pair cache, landed):
stop rule 1 is met. Scratch material (tree copies, binaries, worlds, probes, scripts, red runs) lives under
`~/scratch/collision-classes/` and is never committed.

## 1. WP0: base, close-battle list and census

### 1.1 The base

**WP0's tree** is `0d3c6d0b` (checksum schema 47, the contact solver the default pipeline). Its measurement copy is a
`git archive` of that commit; a second copy carries the scratch probes of plan §10.1 (a) and (c), which only print
lines to standard error and change no column of `matrix.csv`. Each arm builds in its own target directory
(`/home/spacescuffle/cargo-target/collision-classes-<arm>`); the scripts are under
`~/scratch/collision-classes/scripts/`:

```bash
TD=/home/spacescuffle/cargo-target/collision-classes-wp0 scripts/build.sh wp0 trees/wp0 trees/wp0-probe
scripts/assets.sh wp0 trees/wp0           # assets-wp0-solver, assets-wp0-legacy: the pipeline set in movement_rules.ron
scripts/measure.sh wp0 wp0                # §10.2's worlds, one run at a time under ~/scratch/gate.lock
python3 -I scripts/close_list.py trees/wp0/crates/sim_core/tests/fixtures/matchup_matrix.ron \
    worlds/wp0/matrix-solver/matrix.csv
```

`measure.sh` plays the matrix on 24 seeds (the four of record and the tool's first 20 others) under both pipelines,
battle 31 on 100 seeds and battle 33 on 64, the showcases (`showcase.ron`, `showcase_effects.ron`; default seed and
1 to 8, 1,500 ticks, `route_trace`), and the harness families (gap, arc, cross, corridor, corner, mirror; development
seeds 1 to 1,000, original and relabelled). The base matrix under the solver: 1,464 runs, expectation failures 2 on
the seeds of record (battle 28, B-192) and 32 over all seeds, as the K-1 arm of evidence
`2026-11-25-solver-pair-cache` §4.

### 1.2 The close-battle list (plan §7.3), fixed before WP1's first build

Derived from WP0's 24-seed base under the solver and nothing else:

1. **Minority side wins at least 3 of 24:** 14 (6 / 18), 26 (20 / 4), 28 (6 / 18), 31 (20 / 4).
2. **Named close or restated:** the plan's list from the fixture (10, 27, 33, 35, 36a, 36b, 38b, 39a, and 26), and
   the enemy-contact base's list (11a, 14, 21, 22a, 26, 28, 31, 32, 33, 35, 37a, 37b, 38b, 39b, 42, 38a). The
   script's grep of `close|[Rr]estated` also hits the claims of 8 ("a zealot closes") and 9b ("as the zerglings
   close around them"), where the word means approach; neither names the battle close, so neither is listed.
3. No other.

**Class R** (21 battles; blue / red wins of 24 on the base):

| Battle | Wins | Why |
|---|---|---|
| 10_2_hellions_vs_8_lings | 0 / 24 | plan §7.3 |
| 11a_2_banelings_vs_5_marines | 0 / 10 | enemy-contact list |
| 14_thor_vs_ultralisk | 6 / 18 | minority 6; enemy-contact list |
| 21_lurker_vs_6_marines | 11 / 0 | enemy-contact list |
| 22a_reaper_vs_2_workers | 0 / 0 | enemy-contact list |
| 26_100_lings_vs_6_colossi | 20 / 4 | minority 4; fixture (restated); plan §7.3; enemy-contact list |
| 27_100_lings_vs_8_ultralisks | 0 / 24 | plan §7.3 |
| 28_100_lings_vs_10_immortals | 6 / 18 | minority 6; enemy-contact list (`known_bug: Some("B-192")`; stop rule 5) |
| 31_15_marauders_vs_15_stalkers | 20 / 4 | minority 4; fixture ("close"); enemy-contact list |
| 32_30_marines_vs_15_banelings | 17 / 2 | enemy-contact list |
| 33_24_marines_vs_12_zealots | 24 / 0 | fixture; plan §7.3; enemy-contact list |
| 35_12_vikings_vs_6_colossi | 2 / 0 | fixture; plan §7.3; enemy-contact list |
| 36a_storm_turns_marines | 24 / 0 | plan §7.3 |
| 36b_marines_beat_zealots_without_storm | 0 / 24 | fixture; plan §7.3 |
| 37a_abduct_sieged_tank | 0 / 0 | enemy-contact list |
| 37b_sieged_tank_survives_without_abduct | 0 / 0 | enemy-contact list |
| 38a_fungal_turns_zerglings | 24 / 0 | enemy-contact list |
| 38b_marines_beat_zerglings_without_fungal | 0 / 7 | fixture; plan §7.3; enemy-contact list |
| 39a_cloud_on_tanks | 24 / 0 | fixture; plan §7.3 |
| 39b_tanks_beat_roaches_without_cloud | 0 / 0 | enemy-contact list |
| 42_sieged_tank_vs_fleeing_workers | 0 / 0 | enemy-contact list |

**Class D** is every other battle of the matrix (40). The list is not edited after WP1 starts. The colossus battles
outside it are **12a** (24 / 0), **12b** (0 / 24) and **12c** (24 / 0): an outcome move there is stop rule 4.

### 1.3 Census (plan §10.1 (c))

| World | Colossus analogue | Burrowed body | Producer |
|---|---|---|---|
| Matrix | 12a, 12b, 12c, 26, 35 (fixture) | 21 only (probe: the most burrowed units at once is above 0 in battle 21 alone, every seed) | none (no structure in the fixture) |
| Battles 31, 33 | no | no | no |
| Scenarios `showcase`, `showcase_effects`, `default_battle`, `skirmish` | no | no (no Lurker analogue) | structures of the start loadouts |
| `golden_replay.ron` | no | no | one structure (`foundry_core_dome`) |
| `golden_replay_orders.ron` | no | no | no |
| Harness families | no (scratch archetypes and the mirror's shipped picks by property; byte identity is checked) | — | no |

So WP2 (exits by class) can move no measured world: no producer stands beside a burrowed body or an authored class
(stop rule 3). WP3 can move only 12a, 12b, 12c, 26 and 35.

## 2. WP1 and WP2: latent

WP1 (`6bcb6f8b`, collision classes with no class authored) and WP2 (`82723f29`, exits by class) were each built from a
`git archive` copy in a fresh target directory, with the same scratch probes (and probe (b), the count of pairs either
pipeline corrects across classes), and measured on every world of §1.1 against WP0 with
`scripts/compare.sh` (every output file byte for byte, paths normalised, and the probe lines).

| World (both pipelines) | WP1 | WP2 | WP3 |
|---|---|---|---|
| Matrix, 24 seeds (`matrix.csv` with its check columns, `matrix.md`, probe lines) | identical | identical | battle 26 only (§3) |
| Battle 31 (100 seeds), battle 33 (64 seeds) | identical | identical | identical |
| Showcases `showcase`, `showcase_effects` (default seed, 1 to 8; `route_trace`, 1,500 ticks) | identical | identical | identical |
| Harness families gap, arc, cross, corridor, corner (original and relabelled), mirror; seeds 1 to 1,000 | identical | identical | identical |
| Probe (b): pairs corrected across classes | 0 | 0 | 0 |

The only bytes that differ in WP1's and WP2's output are the catalog directory named in the summaries' header line
(`assets-wp0-…` against `assets-wpN-…`); `compare.sh` normalises it. Stop rules 2 and 3 hold. WP3's full run reproduces
its early matrix run byte for byte.

Tests, red first (logs under `~/scratch/collision-classes/red/`):

- **A-1 to A-8** (`collision_class_tests.rs`) on WP0's tree: the file does not compile (no `CollisionClass`,
  `SimWorld::collision_class`, `UnitCatalog::collision_classes`, `UnitModeSpec::collision_class`, typed errors). Its
  behavioural tests alone (A-4 to A-8, the scratch bodies' `collision_class` ignored by WP0's loader): A-4, A-6, A-7 and
  A-8's melee half fail, A-5 and A-8's line half pass (they hold on WP0 too). All green on WP1.
- **A-9** (`trace_tests.rs`) on WP0's tree: fails (the meter counts the cross-class overlap); green on WP1.
- **A-10** (`production::exits` unit test) on WP1's tree: fails (a burrowed occupant blocks a ground product's exit,
  B-212); green on WP2.

## 3. WP3: the colossus analogue's class — stopped

WP3 (`d0c67ceb`): `tripod_annihilator` authors `collision_class: Some("tall_walker")`. A-11 and A-12 fail on WP2's
tree (no archetype authors a class) and pass on WP3's.

**Matrix, 24 seeds, against WP0.** Only battle 26 moves, under both pipelines; 12a, 12b, 12c, 35 and every other
battle are byte-identical (CSV and probe lines), battle 28 included (stop rule 5 holds). Probe (b): no pair of
different classes is corrected anywhere (B9 = 0, both pipelines).

| Battle 26 | Lings / colossi wins (24) | Mean end tick | Mean lings left | Expectation failures (record / all) |
|---|---|---|---|---|
| Solver, WP0 | 20 / 4 | 439 | 14.9 | 0 / 1 |
| Solver, WP3 | 24 / 0 | 322 | 33.2 | 2 / 13 |
| Legacy, WP0 | 2 / 22 | 422 | 0.6 | 3 / 19 |
| Legacy, WP3 | 0 / 24 | 420 | 0.0 | 2 / 19 |

Seeds of record under the solver: 3203386110 colossi 2 left → lings 29 left; 11 lings 19 → 27; 22 lings 12 → **40**
(outside the range 0 to 31); 33 colossi 2 left → lings **45** (outside).

**Checks B1 to B9 (plan §7.2), battle 26, solver against WP0 (the default pipeline):**

| Check | Result | Figures (WP3; WP0's 95 % interval) |
|---|---|---|
| B1 | ok | R10 free 710.5 / seed (2,529.2 to 3,341.0); blocked 2,621.1 (3,759.0 to 4,600.2) |
| B2 | ok | D-SHOVE-E 0.000 both sides (blue 1.817 to 2.413, red 3.870 to 4.668) |
| B3 | ok | largest one-class enemy overlap 0.00 % (base 1.96 %, which counted ling–colossus pairs); ticks 0.0 / seed |
| **B4** | **fails** | D-CHURN unexplained 3.58 / seed (1.50 to 3.04) |
| B5 | ok | give-ups 0 |
| **B6** | **fails** | friend pair-ticks above 5 % 2,359.0 / seed (948.6 to 1,275.4); per 1,000 zergling-ticks 124.1 against 54.8 |
| **B7′** | **fails** | zerglings 2.113 shots per engaged second (2.122 to 2.138); colossi 1.030 (1.006 to 1.030) |
| B8 | ok | standing while able 0.0 / seed (0.0 to 0.0); none beside a body of another class |
| B9 | ok | 0 cross-class corrections |

Under Legacy (reported): B1 fails (R10 free 367.3 against 310.3 to 349.6; blocked 1,104.2 against 932.4 to 1,054.4);
every other check passes.

**First difference** (`firstdiff.py` on `matchup_trace --battle 26_100_lings_vs_6_colossi --seed 33 --seed 1`,
solver): seed 33 (of record) at tick 43, 45 zerglings change position or velocity; seed 1 at tick 41, 20 zerglings.
At the tick's start the nearest of them stands 1.95 (seed 33) and 2.01 (seed 1) from a colossus centre, beyond the
radius sum 1.375, so no contact pair was removed there: the first change is in steering (neighbours) or the attack
rings (a colossus no longer blocks a zergling's ring), both what the plan's stop rule 4 names. No colossus and no
other battle moves.

**Decision: stopped (plan §7.3, stop rule 7; the task's rule).** Battle 26 is Class R, and its outcome moves (the
zerglings now win every seed, two seeds of record above the range), but B4, B6 and B7′ fail, so it is not restated:
the fixture is unchanged and the matrix test fails on battle 26's seeds of record 22 and 33 on WP3's tree. No
amendment was attempted and no tolerance was changed. B-209 stays open.

## 4. Throughput and golden replays

**Throughput** (`perf stat -e instructions:u`, single-scenario throughput binaries, 400 units, 300 ticks, three runs per
arm; WP0 against WP3's tree):

| Scenario | Solver WP0 → WP3 | Legacy WP0 → WP3 (reported) | Ticks/s WP3, solver |
|---|---|---|---|
| 64 × 64 | 16.355 G → 16.263 G (−0.56 %) | 9.784 G → 9.804 G (+0.21 %) | 108 to 109 |
| 128 × 128 | 10.957 G → 10.959 G (+0.02 %) | 10.186 G → 10.199 G (+0.13 %) | 179 to 183 |
| canyon | 27.307 G → 27.231 G (−0.28 %) | 17.384 G → 17.436 G (+0.30 %) | 79 to 82 |

Every scenario runs at 50 ticks per second or more, and the solver stays within +1.0 % of WP0 (stop rule 9 holds).

**Golden replays and the sandbox** on the tree with `main` `91350af1` merged (`8fa6c00d`, schema 48): a `git archive`
copy built in a fresh target directory passes `golden_replay_tests` and `fixture_tests` on native debug and release
and on `wasm32-wasip1` release and debug (`wasmtime --dir=<copy>`). No checkpoint moves; nothing is re-recorded; no
hashed state is added, so the schema stays at main's 48.

**Not run (the package stopped at WP3):** the held-out seeds (§7.4) and WP4's client capture (§9).

## 5. On `Solver` after the redesign (WP-C1, 2026-11-28)

Plan [`2026-12-03-movement-work-on-solver`](../plans/2026-12-03-movement-work-on-solver.md) WP-C1 (this plan's
Amendment 1): the data line re-measured on today's movement, with §7.2's checks unchanged, the owner's contact rules
H-S1 to H-S3 (that plan §4.2) and the held-out set HD. Only the shipped pipeline `Solver` is judged; Legacy is
reported. Owner question [Q0149](../questions/open/0149-colossus-walk-under-without-crowd-fix.md) is unanswered, so
its default (b) applies: the line merges only if every check holds.

### 5.1 Base, candidate and method

- **Base:** `main` `c0d17532` (schema 62), a `git archive` copy built in the fresh target directory
  `/home/spacescuffle/cargo-target/colossus-c1-base`.
- **Candidate:** the base with the data line, branch `colossus-c1-wip` `c43acf19`: `d0c67ceb` re-applied without
  conflict (`tripod_annihilator`'s `collision_class: Some("tall_walker")`, A-11 and A-12), built in
  `colossus-c1-cand`.
- **A-11 and A-12 re-pinned.** Since WP3 parked, the hangar's interceptor authors the air class `flying_escorts` (plan
  `2026-11-28-carrier-interceptors`). "The archetype that authors a class" found it, so A-11 and A-12 passed on the base
  without the line. They now look for a non-air archetype that authors a class (still by property, no slug). With
  that they are red on the base ("a shipped ground archetype authors a collision class") and green on the candidate.
  The other nine tests of `collision_class_tests.rs` pass on both trees.
- **Scripts.** §10.2's scripts are reused by path from `~/scratch/collision-classes/scripts/`, with arms tagged
  `c1base` and `c1cand`.
  - Today's meter carries probe (a) itself: B8 reads the `standing_able` and `standing_able_cross` columns. So the
    probe copy adds only probe (b), B9 (`~/scratch/movement-on-solver/scripts/probe_c1.py`). It prints the line
    `checks.py` reads.
  - Each arm's `measure.sh` ran whole under one hold of `~/scratch/gate.lock` (§10.2), through a copy without its
    inner per-command lock (`c1_measure_nolock.sh`). The worlds and seeds are unchanged.
- **H-S1 to H-S3** come from the matrix columns `*_hold_moved_clean_ticks`, `*_attack_left_band_ticks` and
  `*_shove_e_pair` (`~/scratch/movement-on-solver/scripts/hs.py`), with the same bootstrap as §7.2.
- **HD** is the matrix tool's 141st to 164th non-record seeds (144 to 167), every battle, both pipelines. It was run
  once, after the development seeds.
- Scratch material is under `~/scratch/movement-on-solver/c1/` and `~/scratch/collision-classes/{bin,worlds}/c1*`.

### 5.2 What moves

- **Matrix, 24 seeds.** Only battle 26 moves, under both pipelines. Battles 12a, 12b, 12c and 35, battle 28 (B-192;
  stop rule 5 holds) and every other battle are byte-identical, CSV and probe lines alike. The HD seeds give the same
  result.
- **Other worlds.** Battles 31 and 33, the showcases (default seed and 1 to 8, both pipelines) and the harness
  families (both pipelines, original and relabelled) are byte-identical (`compare.sh c1base c1cand`).
- **Probe (b).** No pair of different classes is corrected anywhere: B9 = 0 under both pipelines, on the development
  seeds and on HD.
- **First difference** (`firstdiff.py`, Solver). Seed 33 (of record) first differs at tick 43, where 45 zerglings
  change, and seed 1 at tick 41, where 20 change, as on 2026-11-25.
  - At the tick's start the nearest of them stands 1.955 (seed 33) and 2.013 (seed 1) from a colossus centre. Both are
    beyond the radius sum 1.375, so no contact pair was removed there.
  - The change is therefore in steering neighbours or the attack rings. Both are causes stop rule 4 names: one cause.
- **Goldens and sandbox.** `golden_replay_tests` and `fixture_tests` pass unchanged on the candidate in `cargo gate
  quick` (native debug). No hashed state is added, so the schema stays at main's 62. The release and `wasm32-wasip1`
  replays belong to the package gate, which this package does not reach.

| Battle 26, Solver | Lings / colossi wins (24) | Mean end tick | Mean lings left | Expectation failures (record / all) |
|---|---|---|---|---|
| Base `c0d17532` | 20 / 4 | 439 | 14.9 | 0 / 1 |
| Candidate | 24 / 0 | 322 | 33.2 | 2 / 13 |
| Base, HD | 16 / 8 | 432 | 16.0 | — |
| Candidate, HD | 24 / 0 | 319 | 32.5 | — |

On the seeds of record the candidate gives:

| Seed | Base | Candidate | Inside the range 0 to 31? |
|---|---|---|---|
| 3203386110 | colossi, 2 left | lings, 29 left | yes |
| 11 | lings, 19 left | lings, 27 left | yes |
| 22 | lings, 12 left | lings, 40 left | **no** |
| 33 | colossi, 2 left | lings, 45 left | **no** |

On HD, 14 of 24 candidate runs leave more than 31 zerglings (base: 5).

### 5.3 Checks, battle 26, Solver against the base

| Check | Development seeds (candidate; base 95 % interval) | HD (candidate; base interval) | Result |
|---|---|---|---|
| B1 | R10 free 710.5 (2,529.2 to 3,341.0); blocked 2,621.1 (3,759.0 to 4,600.2) | 677.8 (2,364.5 to 3,505.5); 2,499.6 (3,751.7 to 4,820.5) | ok |
| B2 | D-SHOVE-E 0.000 both sides (blue 1.817 to 2.413, red 3.870 to 4.668) | 0.000 (1.479 to 2.306; 3.632 to 4.781) | ok |
| B3 | largest one-class enemy overlap 0.00 % (base 1.96 %, ling–colossus pairs); ticks 0.0 | 0.00 % (base 1.91 %) | ok |
| **B4** | D-CHURN unexplained 3.58 / seed (1.50 to 3.04) | 4.04 (1.00 to 2.50) | **fails** |
| B5 | give-ups 0 | 0 | ok |
| **B6** | friend pair-ticks above 5 % 2,261.0 / seed (852.3 to 1,177.7); largest 40.20 % (base 43.63 %) | 2,304.2 (811.3 to 1,398.0) | **fails** |
| **B7′** | zerglings 2.113 shots per engaged second (2.122 to 2.138); colossi 1.030 (1.006 to 1.030) | zerglings 2.111 (2.127 to 2.145) | **fails** |
| B8 | standing while able 0.0 / seed (0.0 to 0.0); none beside a body of another class | 0.0; none | ok |
| B9 | 0 cross-class corrections | 0 | ok |
| H-S1 | Hold Position units moved: 0 (base 0) | 0 (base 0) | ok |
| **H-S2** | zerglings moved by allies out of their band (B-225's count): 43.79 unit-ticks / seed (18.96 to 23.17) | 43.96 (19.79 to 25.46) | **fails** |
| H-S3 | engaged units moved by enemy contact: 0.000 (0.000 to 0.000) | 0.000 (0.000 to 0.000) | ok |

**Legacy (reported).** Only battle 26 moves: the zerglings win 2 of 24 on the base and 0 of 24 on the candidate, and
0 of 24 on HD on both trees.
- B1 fails: R10 free 367.3 (310.3 to 349.6), blocked 1,104.2 (932.4 to 1,054.4).
- H-S2 reads 0.21 against 0.
- Every other check passes.

**HD** confirms each direction of the development seeds and contradicts none (stop rule 7 does not apply). It moves no
Class D battle.

### 5.4 Throughput (§11)

Instructions are from `perf stat -e instructions:u`, three runs per arm, on the single-scenario binaries of
`build.sh` (400 units, 300 ticks).

| Scenario | Solver base → candidate | Legacy base → candidate (reported) | Ticks/s, candidate, Solver (under load) |
|---|---|---|---|
| 64 × 64 | 16.452 G → 16.455 G (+0.016 %) | 9.741 G → 9.744 G (+0.027 %) | 81 to 94 |
| 128 × 128 | 10.952 G → 10.962 G (+0.092 %) | 10.162 G → 10.172 G (+0.099 %) | 98 to 148 |
| canyon | 27.147 G → 27.150 G (+0.009 %) | 17.487 G → 17.489 G (+0.014 %) | 78 to 80 |

- **The whole bench** (`cargo bench -p sim_core --bench throughput`, every scenario, once per arm) uses 434.151 G
  instructions on the base and 434.387 G on the candidate (+0.054 %).
- **Every scenario stays above 50 ticks per second on both arms.** The slowest are the canyon at 64 (base) and 75
  (candidate) and the 400-unit 64 × 64 row at 71 (base) and 104 (candidate).
- **The wall-clock figures are not of record.** They were measured at load average 30 to 115 (§4.7 of the
  movement-on-Solver plan).
- No bench scenario has a colossus. The change is within +1.0 % on every scenario, and the floor holds.

### 5.5 Decision: parked (Q0149 default (b))

B4, B6 and B7′ fail again, as predicted (plan `2026-12-03-movement-work-on-solver` §5), on the development seeds and
on HD. B1 to B3, B5, B8 and B9 hold, and every world without a colossus is byte-identical.

- **H-S2 is new.** It was not measured on 2026-11-25: zerglings that attack beneath and round the colossi are moved by
  allies out of their weapon band about twice as often as on the base. This is the owner's rule "attackers are pushed
  only within their weapon band" (B-225 under `Solver`), and it is a hard check under every answer (stop rule G-2).
- **Not restated.** Battle 26 is Class R and its outcome moves (seeds of record 22 and 33 leave the range), but a check
  fails, so it is not restated. The matrix fixture is unchanged.
- **Where the line stays.** The data line stays parked on `colossus-c1-wip` `c43acf19`, and B-209 stays open with these
  figures.
- **No change was attempted.** No amendment was attempted, no tolerance was changed, and no movement file was
  touched.
- **What Q0149 (a) would need.** Under Q0149 (a), B4, B6 and B7′ would be traced to friend packing (B-219). But H-S2
  binds under (a) too, so the line could merge only once B-225's count beneath a colossus is back within the base, or
  the owner answers [Q0152](../questions/open/0152-colossus-walk-under-attackers-pushed-out-of-band.md).
- **Re-base owed.** While this ran, `main` moved to `08d3d808` (minimap WP6: remembered structures in the player view,
  `world/vision.rs` and `player_view.rs`; wp-i0's docs). The worlds above were not re-measured on it. If an answer lets
  the line merge, §4.4 of the movement-on-Solver plan's single base re-measure comes first.
