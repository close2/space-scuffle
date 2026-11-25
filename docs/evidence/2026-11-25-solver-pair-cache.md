# Evidence: The Contact Solver's Pair Cache Keyed by Move Count (K-1, B-208)

Measurements of the defect fix K-1 (B-208), landed on its own from the parked enemy-contact plan
(`2026-11-25-enemy-contact-and-ring-retarget`, branch `enemy-contact-plan`, head `7f686d23`; its WP1). Branch
`solver-pair-cache`, cut from `main` `28d875fb` with two commits of that plan: `e1b63a88` (its WP0 diagnostics, no
behaviour change) and `1a1c1d9b` (K-1 and its test K1-T1); `main` `be58ebc7` and `9ae5a023` merged afterwards
(documents and presentation only). Scratch material (trees, binaries, matrix tables, perf runs, scripts, the red
run) is under `~/scratch/pair-cache/`, never committed.

## 1. Decision

The pre-registered checks of §4 below failed in four readings, and the work stopped and was reported. The orchestrator
then decided:

> Orchestrator decision, 2026-11-25: K-1 fixes a code defect (B-208: the enemy pass read stale pair corrections;
> K1-T1 shows -0.1000 reused where the fresh value is -0.0437). It is not a behaviour change, so it is judged on
> correctness. The strict per-battle maximum of B3 failed in 3 of 16 battle and seed-set readings: battle 28 dev
> 1.345 → 1.375 %, battle 26 held-out 2.05 → 2.72 %, battle 29 held-out 1.95 → 2.46 %. The cause is trajectory
> divergence: a different seed sets the maximum. Tick counts stayed within the base's noise. The largest overlap fell
> in the other battles, and no outcome of record moved. B7′ in battle 26 missed by 0.001 (2.130 against
> [2.131, 2.144]) and passes on the held-out seeds. The check definitions are unchanged. These figures are recorded
> as failures that the orchestrator accepted for a defect fix, not as passes.

## 2. The checks and the diagnostics

The diagnostics of `e1b63a88` (`crates/sim_fixtures/src/trace/contact_checks.rs`, `harness/room.rs`, the matrix
columns and the mirror family's columns) port the first attempt's (plan `2026-11-24-solver-retarget-and-shoving`
§5.1, branch `solver-followup`) with the enemy-contact plan's repairs (its §6). They change no behaviour. Per battle
and seed, `matrix.csv` appends:

- **D-SHOVE** and **D-SHOVE-E:** for a unit that proposed no motion and was not holding, per tick, the distance it
  was moved (D-SHOVE), and the largest outward radial part of that displacement over the enemies touching it at the
  tick's start (D-SHOVE-E), summed per side, world units.
- **D-R10** (`free`, `leash`, `terrain`, `unruled`): unit-ticks an automatic chaser stood while another enemy's ring
  had room; **D-CHURN:** returns to an earlier target, explained or not.
- **Engaged unit-ticks:** ticks ending with the unit's attack target in its band, for B7′'s shots per engaged
  unit-second.
- **Enemy overlap:** the largest enemy overlap at the end of a tick, as a fraction of the radius sum, and the
  pair-ticks above 0.5 %; **friend overlap:** the largest, and the pair-ticks above 5 %.

The behaviour checks (the enemy-contact plan's §7.1, pre-registered), per battle over the seeds against the base,
"beyond noise" meaning the candidate's per-seed mean outside the base's 95 % bootstrap interval (2,000 resamples,
seed 7) on the worse side:

| Check | Fails when |
|---|---|
| B1 | D-R10 `free` or blocked unit-ticks per seed above the base beyond noise (not judged for battle 28, B-192) |
| B2 | D-SHOVE-E of either side above Legacy's upper bound on the same seeds |
| B3 | The largest enemy overlap above the base's, or enemy-overlap ticks per seed above the base beyond noise |
| B4 | D-CHURN unexplained per seed above the base beyond noise |
| B5 | Give-ups of automatic chases per seed above the base beyond noise |
| B6 | Friend pair-ticks above 5 % per seed above the base beyond noise, where B3 did not improve |
| B7′ | Shots per engaged unit-second of either side below the base beyond noise |

## 3. K-1 and K1-T1

`ContactSolver::accumulate` reused a pair's cached correction while neither body had `moved` in the latest
application, and `apply` cleared `moved` for every body at every application. After the last averaged iteration
the hard friends' pass cleared the flag, and the enemy and fixed pass reused corrections computed before the
averaged iteration had moved the bodies. K-1: each `BodyStep` counts the applications that moved it (`moves`), each
`Pair` records the two counts its cached correction was computed at (`cached_at`), and a cached correction is reused
only while both are unchanged.

**K1-T1** (`solver::tests::the_enemy_pass_reads_corrections_at_its_own_positions`): red with the fix alone reverted
on this tree (the enemy pass applied −0.1000 where the fresh correction is −0.0437), green with it. The guards
`the_solution_does_not_depend_on_body_order` and `a_crowd_at_rest_within_the_dead_band_does_not_move` stay green.

## 4. Matrix

Arms, each a release build of the matchup trace in its own target directory: **base** `e1b63a88` (`main` `28d875fb`
with the diagnostics) and **K-1** `1a1c1d9b`. Seeds: the four of record and the tool's first 20 others (24 seeds);
held-out: the tool's 21st to 44th others (23 to 32, 34 to 47). The base's table equals the enemy-contact plan's
base of record on `28d875fb` row for row, and K-1's equals that plan's WP1 arm, on their common columns. **Legacy
is bit-identical** between the arms (1,464 rows).

**24 seeds:** failures of record 2 → 2 (battle 28, B-192, which keeps its marker), over all seeds 33 → 32. Battles
that change (every other battle is identical or changes diagnostic columns only):

| Battle | Wins b/r | Largest enemy overlap | Enemy-overlap ticks per seed (base interval) |
|---|---|---|---|
| 10 | 0/24 → 0/24 | 0.44 → 0.00 % | 0.0 (0.0, 0.0) |
| 26 | 19/5 → 20/4 | 2.59 → 1.96 % | 16.3 (11.9, 26.6) |
| 27 | 0/24 → 0/24 | 2.45 → 1.29 % | 2.3 (0.7, 4.2) |
| 28 | 7/17 → 6/18 | **1.345 → 1.375 %** (B3 fails; seed 7, not of record) | 5.1 (1.8, 10.3) |
| 29 | 24/0 → 24/0 | 3.79 → 2.12 % | 14.5 (12.3, 30.5) |
| 38a | 24/0 → 24/0 | 0.91 → 0.56 % | 0.0 (0.0, 0.4) |
| 38b | 0/7 → 0/7 | 0.56 → 0.55 % | 0.0 (0.0, 0.1) |

- **B7′ fails in battle 26:** blue 2.130 shots per engaged second against the base's (2.131, 2.144).
- **B2 fails in every crowd battle on the base already** (B-193, open): the Solver's D-SHOVE-E is above Legacy's
  bound in 9a, 10, 23a, 23b, 24a, 24b, 26, 27, 28, 29 and 33 on the base. K-1's D-SHOVE-E lies inside the base's
  interval on both sides in every battle.
- B1, B4, B5 and B6 pass in every battle.

**Held-out seeds:** failures of record 0 → 0, over all seeds 22 → 20. Largest enemy overlap: 10 0.51 → 0.00 %,
26 **2.05 → 2.72 %** (B3 fails), 27 1.19 → 0.97 %, 28 1.88 → 1.74 %, 29 **1.95 → 2.46 %** (B3 fails), 38a
1.67 → 1.65 %, 38b 3.23 → 3.23 %; every overlap-tick count lies inside the base's interval. Battle 26's wins
20/4 → 23/1. B7′ passes in every battle.

## 5. Throughput

Instructions retired (`perf stat -e instructions:u`) by the single-scenario throughput binaries (400 units, 300
ticks), three runs per arm and pipeline; the runs agree within 0.015 %. Ticks per second are the range of the three
on a shared, loaded machine.

| Scenario | Legacy base → K-1 | Solver base | Solver K-1 | Ticks/s K-1 Solver |
|---|---|---:|---:|---:|
| 64 × 64 | 9.783 G → 9.784 G (noise) | 16.305 G | 16.354 G (+0.30 %) | 99 to 108 |
| 128 × 128 | 10.186 G → 10.187 G (noise) | 10.956 G | 10.958 G (+0.02 %) | 98 to 166 |
| canyon | 17.384 G → 17.384 G (noise) | 27.239 G | 27.308 G (+0.25 %) | 75 to 76 |

Every scenario runs at 50 ticks per second or more, and the Solver costs at most +0.30 %.

## 6. Golden replays and the sandbox

K-1 moves no checkpoint. `golden_replay`, `golden_replay_orders` and `DEFAULT_SANDBOX_CHECKSUM`
(`crates/sim_fixtures/tests/fixture_tests.rs`) reproduce unchanged on the final tree (`7ebabd70`, `main` `9ae5a023`
merged), so nothing is re-recorded. The checksum schema stays 47, since K-1 hashes nothing new: the move counts
live only inside one solve. The proof uses a `git archive` copy of `7ebabd70` built in a fresh target directory,
with `golden_replay_tests` and `fixture_tests` green on native debug and release and on `wasm32-wasip1` release and
debug (`wasmtime` runner). The golden fixture's header records this check. Every test of `sim_core` and
`sim_fixtures` passes in release on the tree, the matchup matrix's seeds of record and every pinned digest included.
