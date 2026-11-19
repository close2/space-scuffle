# Evidence: Crowd-Edge Steering 2 (B-103)

Measurements for plan [`2026-11-19-crowd-edge-steering-2`](../plans/2026-11-19-crowd-edge-steering-2.md), as
amended (Amendment 1: OQ-1 and OQ-2 decided yes). Each work package appends its section. Figures are counts over
the runs named; "of record" is the matrix's four seeds of record per battle (232 runs), "all seeds" its 20 seeds per
battle (1,160 runs).

## How to reproduce

All measurements use release builds of the two trace tools of `sim_core`:

```sh
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
```

The measurement (`~/scratch/crowd-edge-2/scripts/measure2.sh <bindir> <outdir> [<basedir>]`, run from the
repository root, the matrix and battle runs serialised under `flock ~/scratch/crowd-edge-2/measure.lock`) is:

```sh
matchup_trace --all-battles --seeds 16 --jobs 8 --out <out>/matrix --compare <base>/matrix/matrix.csv
matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <out>/b33
route_trace --scenario scenarios/showcase.ron --ticks 3000 --out <out>/route            # default seed
route_trace --scenario scenarios/showcase.ron --seed <s> --ticks 3000 --out <out>/route-s<s>   # s = 1 to 8
matchup_trace --battle 31_15_marauders_vs_15_stalkers --seeds 96 --no-unit-csv --out <out>/b31
```

`--seeds 16` adds 16 seeds to the four of record (20 runs per battle); `--seeds 96` gives battle 31 its 100 runs
(the four seeds of record and seeds 1 to 96). `~/scratch/crowd-edge-2/scripts/summary.py <out> <base>` prints the
totals, the battles' overall winners and flips, battle 33, the nine showcase runs and battle 31 on 100 runs. The
reproductions are `cargo test --release --locked -p sim_core --test second_line_tests --test crowd_edge_tests
--no-fail-fast -- --include-ignored --nocapture`. Reversal classes come from the scratch probe tree
`~/scratch/crowd-edge-2/probe` (`scripts/probe.sh <name> <candidate.diff>`, the candidate being the diff of
`crates/sim_core/src` against `76a7ac67`).

**Stop rule 1 for a battle decided by one seed (Amendment 1, OQ-1).** A matrix battle whose 20-run base split has
at least one decided run and differs between blue and red wins by at most one is judged on its 100 runs: the rule
holds when the 100-run winner is unchanged and the winner's share does not fall by more than the placebo band.

**The placebo band.** Three placebo builds each add one raw unit (2⁻³²) to one movement constant
(`~/scratch/crowd-edge-2/patches/patches-placebo{1,2,3}.diff`): `PUSH_RELAXATION`, `FIXED_BODY_TANGENT_WEIGHT`,
`STEERING_LOOKAHEAD`. The range of their matrix totals of record is the band; a reversal effect counts only below
its lower edge.

## WP1: the measurement method on the WP1 tree (bit-identical)

The WP1 tree (WP0 on `b0600041`) has `crates/`, `Cargo.lock`, `assets/` and `scenarios/` identical to `76a7ac67`.
Its tools were rebuilt and measured (`~/scratch/crowd-edge-2/wp1/`):

- the matrix is "bit-identical to `~/scratch/crowd-edge-2/base/matrix/matrix.csv`" (the plan's §2.1 base);
- battle 33's and battle 31's `seeds.csv` and the nine showcase runs' `units.csv` are byte-identical to base.

So the plan's §2.1 base holds: matrix of record 6,154 reversals (2,918 proposal, 3,236 contact), 2 give-ups; all
seeds 30,642 and 6; battle 33 blue 47 of 68; battle 31 6 / 7 / 7 (20 runs) and 46 / 22 / 32 (100 runs); showcase
player 1 on every seed, 235 reversals on nine seeds, default seed player 1 at tick 1,946 with 13 / 10.

**Battles judged on 100 runs.** Over the base matrix, the battles whose 20-run blue and red wins differ by at most
one are `31_15_marauders_vs_15_stalkers` (6 / 7 / 7) and five that end unresolved on all 20 runs (`22a`, `37a`,
`37b`, `38b`, `39b`, 0 / 0 / 20). Those five have no decided run, so their winner is a tie that any decided run
moves; they stay judged on 20 runs. Battle 31 is the only battle the 100-run reading covers.

**The placebo band, re-measured on the WP1 tree** (`~/scratch/crowd-edge-2/wp1-placebo{1,2,3}/`, each built from
the WP1 tree with its placebo patch; each matrix is byte-identical to the plan's §2.2 run on `76a7ac67`):

| Placebo | Battles moved | Reversals of record (proposal + contact) | All seeds | Give-ups (record / all) | Flips | Battle 31, 100 runs | Battle 33 | Showcase, nine seeds |
|---|---:|---|---:|---|---|---|---|---|
| P1 `PUSH_RELAXATION` | 9 | 6,412 (3,056 + 3,356) | 31,392 | 2 / 6 | none | 46 / 22 / 32 | 47 of 68 | 235, unchanged |
| P2 `FIXED_BODY_TANGENT_WEIGHT` | 4 | 6,017 (2,855 + 3,162) | 30,062 | 2 / 6 | none | 46 / 22 / 32 | 47 of 68 | 235, unchanged |
| P3 `STEERING_LOOKAHEAD` | 4 | 6,310 (2,999 + 3,311) | 31,088 | 2 / 6 | none | 46 / 22 / 32 | 47 of 68 | 235, unchanged |

**Band:** matrix reversals of record [6,017, 6,412] (−2.2 % to +4.2 % of base). **Battle 31 on 100 runs:** no
placebo moves it, so its band is empty; the rule of Amendment 1 then reads "blue stays the winner and its share
does not fall": blue 46 of 100 runs, and 46 of the 68 decided runs (67.6 %).

## WP2: D-2, a spent walk re-arms when the side that closed opens, or on ground gained

**Change.** `FLANK_RETRY_TICKS` and the timed retry are removed. A spent walk re-arms when the walk was spent by
closed sides and the kept side admits a whole across step (`closed_side_reopens`, the walk-end test of
friendly-blocking Amendment 7 turned sideways, `admits_whole_step`), when `note_queued_progress` reports ground
gained, or on a new target or order. No new constant and no new state.

**Tests first (red on WP1).** On WP1's code, with WP2's tests: CE-7 `a_queued_chaser_keeps_its_target`, CE-8
`a_spent_walk_resumes_when_the_way_opens` (with its new assertion: the walk re-arms within six ticks of the way
opening), CE-10 `a_spent_walk_re_arms_when_its_closed_side_opens` (re-armed by the timer while its side stood
closed), CE-11 `contact_creep_does_not_re_arm_a_walk` (20 walks spent, 17 re-armed by the timer without ground gained)
and `no_room_means_waiting_still` fail; CE-9 passes. The unit tests `a_walk_spent_by_its_circle_does_not_re_arm_on_an_open_side`
and `a_closed_side_opens_only_for_a_whole_step` test the new function. CE-11 also fails under prototype K3's test (any
admissible across step above the waiting share): 8 re-arms without ground gained. On WP2 every test passes.

- **CE-10**: the walk is spent on tick 26 (sides closed by two friends holding beside the chaser), stays spent for 40
  ticks, and re-arms on tick 68, the first tick whose start leaves the kept side's friend a whole step away
  (gap 0.135 ≥ 0.126 − 1/64), the tick after the friend is ordered away.
- **CE-11**: no walk of the corridor's queue re-arms without ground gained in 400 ticks.

**Measured** (`~/scratch/crowd-edge-2/wp2/`, base WP1). The release tools reproduce the K4 prototype bit for bit:
the matrix, battle 33's and battle 31's `seeds.csv`, the nine showcase `units.csv` (identical to WP1) and the
probe classes.

| Stop rule | WP1 | WP2 | Holds |
|---|---|---|---|
| 1. Winner: showcase default seed | player 1 at 1,946 | player 1 at 1,946 (all nine seeds bit-identical to WP1) | yes |
| 1. Winner: matrix battles | — | no flip but battle 31 on 20 runs: 6 / 7 / 7 → 14 / 2 / 4 | yes |
| 1. Battle 31 on 100 runs (Amendment 1) | blue 46, red 22, unresolved 32 | blue 54, red 20, unresolved 26; blue's share 54 % of runs (46 %), 73 % of decided runs (67.6 %) | yes |
| 2. Give-ups, record / all seeds | 2 / 6 | 3 / 7 (battle 31 seed 11, traced below) | yes (base + 1) |
| 2. Give-ups, showcase seeds | 0 | 0 on every seed | yes |
| 3. Reversals of record | 6,154 (2,918 + 3,236) | 6,111 (2,883 + 3,228); all seeds 30,629 (30,642) | yes (below base; inside the placebo band [6,017, 6,412], so no effect is claimed) |
| 4. Guards | arc 10 / 10 at 96, walkers 17 / 17, deeper 0 | arc 10 / 10 at 96, 0 reversals; walkers 17 / 17, 31 reversals (23 + 8); deeper 0 on every seed | yes |
| 5. Control | — | rule off is WP1's code, whose figures are the base above | yes |

Battles 26 to 30 of record: 1,021, 1,045, 1,353, 1,036, 162 (1,024, 1,033, 1,393, 1,047, 165); battle 31 201 (203),
battle 35 668 (668); 8 battles moved. Battle 33 blue 47 of 68 (47).

**Corridor** (`no_room_means_waiting_still`, un-ignored): 2 of 6 in range, 0 give-ups, 7 reversals (6 + 1, base
21); rear units stand on 332 of 340, 331 of 332, 311 of 324, 309 of 309 and 308 of 316 ticks after they first stand
(97.6, 99.7, 96.0, 100, 97.5 %; base 88, 88, 69, 65 % for the four that moved).

**Probe classes** (identical to K4's): showcase nine seeds R-A 63, R-B 89, R-C 32, R-D 30, R-E 7, R-F 14 (as base);
corridor R-A 6, R-D 1 (base 18, 1, 0, 2); walkers R-A 21, R-B 2, R-D 6, R-E 2 (as base).

**Battle 31 seed 11's give-up** (`~/scratch/crowd-edge-2/wp2/b31s11-giveup-trace.txt`). Blue marauder entity 4,
attack-moving to (45, 48) with no target, starts a surround walk at tick 61 at (26.8, 45.5) and walks its scan
range sideways (6.60) by tick 129, at (27.0, 52.2). The walk is spent by its bound, not by closed sides, so by D-2.1
only ground gained or a target re-arms it; it gains none, sees no enemy, and is not queued behind a friend that
holds its ground (D-4), so its stall count runs from tick 129 and the stall rule gives up the order (`NoProgress`)
at tick 379. Under the timed retry it walked again every 24 ticks. **Classification:** rule-caused, as D-2.1
designs it (a bound-spent walk does not re-arm on an open side, because re-arming would restart the same walk); it
is the case risk K-6 names, and within stop rule 2 (base + 1).

**Goldens.** `golden_replay` and `golden_replay_orders` reproduce every checkpoint on WP2 and
`DEFAULT_SANDBOX_CHECKSUM` holds: D-2 moves neither (no unit in them spends a walk). Nothing is re-recorded.

**Throughput** (`flock ~/scratch/gate.lock`, WP1 and WP2 bench binaries alternated, three rounds, ticks per second,
median of 3 per run; `~/scratch/crowd-edge-2/wp2/bench/`):

| Scenario | WP1 rounds | WP2 rounds |
|---|---|---|
| 64×64, 0 units | 1,990, 2,423, 2,983 | 3,272, 2,382, 3,254 |
| 64×64, 100 | 767, 548, 768 | 945, 538, 916 |
| 64×64, 400 | 154, 120, 121 | 161, 119, 149 |
| 128×128, 400 | 165, 130, 93 | 185, 154, 177 |
| 256×256, 0 | 149, 196, 192 | 202, 178, 198 |
| showcase canyon, 400 | 119, 92, 113 | 119, 101, 109 |
| 64×64, 67 casters, 400 | 237, 124, 225 | 111, 133, 227 |

Every scenario stays far above 25 ticks per second; the spread between rounds of one build (the host is shared)
exceeds any difference between the builds.
