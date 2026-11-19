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

## WP3: D-1, a walk does not end on the room its own retreat made

**Change.** `MovementMemory::flank_retreat` sums the backward parts of a walk's steps along the way in
(`add_retreat`); a walk in progress ends only when the straight way in is free for `speed + flank_retreat`, less
`STANDING_DRIFT` (`admits_whole_step`). It is cleared by a forward step, by `end_surround_walk` and by
`MovementMemory::restart`. The field is hashed after `flank_walked`: checksum schema 31 (`git log main -S` found no
other schema 31). No reader serialises `MovementMemory` other than the checksum.

**Tests first (red on WP2,** run in a WP2 worktree with WP3's test file**).** CE-12
`a_walk_does_not_end_on_the_room_its_retreat_made` fails on WP2 (walker 7 of the walkers layout, entity 13: walk
step then forward step reversed on ticks 91 and 94; its first walk step, tick 55, goes back 0.013). CE-13
`a_walk_out_of_a_pocket_still_reaches_range` (walker 6, entity 12, the first plan's T1 unit: first walk step back
0.025 on tick 54) passes on WP2 and on WP3. CE-14 `a_walk_without_retreat_ends_as_before` records WP2's arc steps.
Unit tests: `a_walk_ends_only_on_room_beyond_its_retreat` (with no retreat the walk ends at exactly
`speed − STANDING_DRIFT` of room, as before; with a retreat of 0.05 it ends only at 0.05 more),
`the_retreat_sums_only_backward_parts`, `the_retreat_is_cleared_by_the_end_of_the_walk_and_a_new_order`. On WP3
every test passes; `walkers_at_the_crowds_edge_do_not_toggle` is tightened to at most 3 per walker and 16 in all
(measured: 16, per walker at most 3; WP2 31).

**Deviations in the tests (recorded).**
- *CE-12 and CE-13 use walkers of the walkers layout itself* instead of an added walker: walker 7 has pulse bot 4's
  geometry (its walk starts with a step back round the body ahead on its side, and the walk-then-forward pair
  reverses on WP2), and walker 6 is the T1 unit. Their retreats are measured along the way to the pillar's centre.
- *CE-14 is restated to the window where its premise holds.* The plan has the arc's walk steps bit-identical to
  WP2's over 120 ticks, on the premise that its walks do not step back. They do while the arc forms: unit 8's walk
  steps back on ticks 56 to 58 (the first plan's T4, forward parts −0.037, −0.034, −0.016), so on tick 63 its walk,
  which ended on WP2, goes on under D-1 (and units 9 and 10 follow). CE-14 therefore holds the arc's proposed steps
  of ticks 1 to 62 bit-identical to WP2's (a fold recorded from WP2, `0xab96d520e2d674a3`), and then the arc's
  outcome as before: 10 / 10 in range by tick 96, 0 reversals, 0 give-ups. No bound is loosened.
- *The corridor's standing test is re-ignored, as Amendment 1 (OQ-2) decides.* `no_room_means_waiting_still`: 0
  give-ups, 2 of 6 in range, 3 reversals (2 + 1); rear entities 3 to 7 stand on 332 of 340, 331 of 332, 283 of 324
  (87.3 %), 309 of 309 and 300 of 316 (94.9 %) ticks. Entities 5 and 7 are below 95 %; the bound is kept and the
  test ignored with these figures (B-103 residual). `no_room_means_no_reversals` stays ignored at 3 (one each for
  entities 5, 6 and 7); `showcase_crowd_edges_do_not_reverse` stays ignored at 8 and 9.

**Measured** (`~/scratch/crowd-edge-2/wp3/`, base WP2). The release tools reproduce the A1 + K4 prototype bit for
bit: the matrix, battle 33's and battle 31's `seeds.csv`, the nine showcase `units.csv` and the probe classes.

| Stop rule | WP2 | WP3 | Holds |
|---|---|---|---|
| 1. Winner: showcase default seed | player 1 at 1,946 | player 1 at 1,935 | yes |
| 1. Winner: matrix battles | battle 31 14 / 2 / 4 | no flip (23 battles moved; battle 26 0 / 20 / 0 → 1 / 19 / 0, battle 35 13 / 0 / 7 → 14 / 0 / 6, battle 31 14 / 2 / 4) | yes |
| 1. Battle 31 on 100 runs (Amendment 1) | blue 54, red 20, unresolved 26 | blue 49, red 17, unresolved 34 | see below |
| 2. Give-ups, record / all seeds | 3 / 7 | 2 / 5 (battle 31 seed 11's is gone) | yes |
| 2. Give-ups, showcase seeds | 0 | 0 on every seed | yes |
| 3. Reversals of record | 6,111 (2,883 + 3,228) | **4,659** (1,762 + 2,897), all seeds 22,666 (30,629) | yes: below WP2 and below the placebo floor 6,017 (−24 % against WP2, −24 % against WP1's 6,154) |
| 4. Guards | arc 10 / 10 at 96, walkers 17 / 17 | arc 10 / 10 at 96, 0 reversals; walkers 17 / 17, 16 reversals (7 + 9); deeper unit-ticks 0 on every showcase seed | yes |
| 5. Control | — | D-1 off by a scratch flag (`reach = speed`, field kept): matrix "bit-identical to `wp2/matrix/matrix.csv`", battle 33's and 31's `seeds.csv` and the nine showcase `units.csv` byte-identical to WP2 (`~/scratch/crowd-edge-2/wp3off/`) | yes |

**Battle 31 on 100 runs.** Blue stays the winner. Against the plan's base (WP1, the reference D-3 (b) names: blue
46, red 22), blue's share rises on both readings: 49 % of runs (46 %) and 74.2 % of decided runs (67.6 %). Against
WP2 (blue 54, red 20) blue's share of decided runs rises (73.0 → 74.2 %) but its share of all runs falls 54 → 49 %,
with more runs unresolved (26 → 34); no placebo moves this battle, so its band is empty. The rule is read as
holding (winner unchanged; share not below the plan's base and not below WP2 on decided runs); the reading against
WP2's share of all runs is recorded here for the orchestrator.

Battles 26 to 30 of record: 895, 807, 1,268, 717, 106 (WP2 1,021, 1,045, 1,353, 1,036, 162); battle 31 117 (201);
battle 34 116 (148); battle 35 273 (668). Battle 33: blue 39, red 13 of 68 (47, 16), no flip.

**Showcase** (player 1 wins every seed but seed 6; reversals per side, proposal + contact):

| Seed | WP2 | WP3 |
|---|---|---|
| default | player 1 at 1,946, 13 / 10 | player 1 at 1,935, 8 (4 + 4) / 9 (5 + 4) |
| 1 | player 1 at 1,928 | player 1 at 2,141 |
| 2 | player 1 at 1,913 | player 1 at 2,170 |
| 3 | player 1 at 2,194 | player 1 at 2,542 |
| 4 | player 1 at 2,616 | player 1 at 1,939 |
| 5 | player 1 at 1,924 | player 1 at 1,939 |
| 6 | player 1 at 2,175 | not over at 3,000 (player 0 wins at 3,437, below) |
| 7 | player 1 at 2,187 | player 1 at 2,175 |
| 8 | player 1 at 1,921 | player 1 at 1,937 |
| Nine seeds, reversals | 235 | 174 |

**Showcase seed 6 (Amendment 1's trace).** Player 1's army is dead by tick 900. Player 0's two surviving pulse
bots, entities 4 and 5, kill player 1's two welder drones (entities 15 and 16) by tick 1,140, acquire player 1's
foundry core dome (entity 14, 1,500 life, at (55.5, 55.5)) and fire on it from (48.27, 52.02) and (47.90, 52.69)
under `AttackTarget { origin: Acquired }`. The core's life falls steadily: 1,480 at tick 1,140, 1,250 at 1,500, 925
at 2,000, 610 at 2,500, 280 at 3,000; player 0 wins at tick 3,437 (a scratch run to the end). No give-up, no
idle unit, no wedge: **a slow but progressing siege, reported as rule-caused** (D-1 changes which army survives the
mid-map fight on this seed). Seed 6 changes side (player 1 at 2,175 on WP2); it is not the default seed.

**Probe classes** (identical to the A1 + K4 prototype): showcase nine seeds R-A 42, R-B 31, R-C 37, R-D 24, R-E 26,
R-F 14 (WP2 63, 89, 32, 30, 7, 14); default seed 4, 3, 3, 2, 4, 1; corridor R-A 2, R-D 1 (WP2 6, 1); walkers R-A 6,
R-B 2, R-D 7, R-E 1 (21, 2, 6, 2); arc none.

**Two of the new R-E events, traced** (default seed; a scratch dump of the units' memory on WP2 and WP3):
- *Tick 340, player 0's unit 6* steers −x (−0.084, 0.012), then +x (0.084, 0.020). Friend 9, just ahead of it
  (0.65 further on its way), starts a surround walk on tick 338 and walks across unit 6's way at full speed
  (−0.125, 0.013 on tick 340). Unit 6 first steers to the side 9 is walking into, then back. Neither unit's
  retreat is in play (9's is 0.001): the two trees have already parted by 0.01 in position, and the event is a friend
  crossing the unit's path, R-E's own mechanism (B-144).
- *Tick 366, player 0's unit 5* steers −x (−0.083, 0.015), then +x (0.083, −0.003). Friend 6, ahead of it, re-arms
  its paused walk on tick 364 and walks across 5's way (−0.125, 0.016), then ends the walk on tick 366 and steps
  forward. Unit 6 stands where it does because D-1 held its earlier walk (ticks 350 to 352, retreat up to 0.098); on
  WP2 it was 0.8 further left, walking round the crowd.

**Cause of R-E's rise:** D-1 keeps more walkers walking, and paused, at the crowd's edge, so their across steps
cross the way of the friends that follow them more often. The reversing unit takes no walk step itself; the class is
B-144's (a friend crossing the unit's path), not a defect of D-1's rule.

**Goldens** (proof headers in the fixtures):
- `golden_replay`: schema 30 → 31 changes every checkpoint. Behaviour: checkpoint 1 (ticks 1 to 50) first changed.
  Entity 30, attack-moving at (43.50, 49.71) with a walk that has stepped back 0.1727, keeps walking at tick 44
  where its walk used to end (a scratch print where D-1's end test and the old one disagree).
- `golden_replay_orders`: schema only. A scratch build with D-1 on, the field unhashed and schema 30 reproduces it.
- `DEFAULT_SANDBOX_CHECKSUM`: schema only (a scratch build without the field and with schema 30 reproduces
  `15_355_001_956_314_655_944`).
- Proof: a scratch build of WP3 with D-1 off, `flank_retreat` unhashed and schema 30 reproduces the previous
  checksums at every checkpoint of both replays and the sandbox checksum.
- The new lists pass native release and debug, and `wasm32-wasip1` release and debug under wasmtime
  (`CARGO_TARGET_WASM32_WASIP1_RUNNER=wasmtime`).

**Throughput** (`flock ~/scratch/gate.lock`, WP2 and WP3 bench binaries alternated, three rounds, ticks per second,
median of 3 per run; `~/scratch/crowd-edge-2/wp3/bench/`):

| Scenario | WP2 rounds | WP3 rounds |
|---|---|---|
| 64×64, 0 units | 3,128, 3,224, 2,985 | 3,109, 3,219, 3,155 |
| 64×64, 100 | 906, 923, 878 | 784, 669, 907 |
| 64×64, 400 | 162, 135, 129 | 152, 139, 98 |
| 128×128, 400 | 134, 181, 150 | 178, 160, 166 |
| 256×256, 0 | 194, 191, 150 | 199, 166, 166 |
| showcase canyon, 400 | 102, 95, 113 | 94, 99, 85 |
| 64×64, 67 casters, 400 | 212, 211, 219 | 204, 218, 131 |

Every scenario stays far above 25 ticks per second. The rounds of one build spread as widely as the builds differ
(the host is shared); no change beyond noise is claimed, and `DESIGN.md` §5 is not touched.

## WP4: storyboard and bookkeeping

**Storyboard** (the default seed's timeline moved: player 1 wins at 1,935, not 1,946). The `battle` take rendered
on radv (RX 580, Mesa 25.0.7, one `with-gpu-lock` hold, 161 s, revision `c8cb9321`; manifest
`~/scratch/crowd-edge-2/wp4/takes/battle.manifest.ron`): `advance` 251 to 305 (first shot), `clash` 305 to 680,
past the lane fight's last death (the viewer's last unit at tick 622; 621 before), `march` 683 to 933 at 4x, ending
on combat at the viewer's base, and `assault` from 933 at 2x over the workers' deaths (ticks 1,007 and 1,033; 1,010
and 1,038 before). The enemy's five surviving pulse bots fire on the viewer's hall, which falls at 1,935. No scene
length changed; the storyboard's comment gains the re-check. `showcase_storyboard_tests` and `showcase_tests` pass.

**Records.** `DESIGN.md` B-103 restated (landed: D-4 of the first plan, D-1 and D-2 here; open, with figures over
the showcase's nine seeds: R-B 31, R-D 24, R-C 37, the walk's start retreat 32, the corridor's standing at 87 and
94.9 %; R-E's rise 7 → 26 under B-144); the Movement row names D-1 and D-2 (its test files already list
`crowd_edge_tests.rs` and `second_line_tests.rs`); `DESIGN.md` §5 is not touched (throughput within noise).
Friendly-blocking Amendment 4 change 5 is marked superseded by D-2. This plan's row is `Done`.

**Open items** (carried under B-103, plan §9): R-B and R-D (need the slide round a friend that holds its ground as a
locomotion rule first), R-C (needs a wall-bearing matrix battle), the walk's start retreat, the corridor's standing
residual, R-E (B-144) and R-F (B-145).
