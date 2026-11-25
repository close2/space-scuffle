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

IDENTITY_PLACEHOLDER

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
