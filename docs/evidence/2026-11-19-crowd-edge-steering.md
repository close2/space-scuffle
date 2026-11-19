# Evidence: Crowd-Edge Steering (B-103)

Measurements and test results of plan [`2026-11-19-crowd-edge-steering`](../plans/2026-11-19-crowd-edge-steering.md),
one section per work package.

## How to reproduce

Each tree builds its release tools into its own target directory and copies them aside, so later builds cannot
change an earlier step's binaries (charter §7):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/crowd-edge-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} ~/scratch/crowd-edge/bin-<wp>/
```

Then, from the repository root, `~/scratch/crowd-edge/scripts/measure.sh <bin> <dir> [<base dir>]` runs, under
`flock ~/scratch/gate.lock` for the two heavy ones:

```bash
<bin>/matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv
<bin>/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 3000 --out <dir>/route
for s in 1 2 3 4 5 6 7 8; do
  <bin>/route_trace --scenario scenarios/showcase.ron --seed $s --ticks 3000 --out <dir>/route-s$s
done
```

`~/scratch/crowd-edge/scripts/showcase.py <dir> [<base dir>]` tabulates the nine showcase runs (outcome, reversals
split into proposal- and contact-caused, give-ups, deeper unit-ticks), and `matrix.py <matrix.csv> [<base csv>]`
sums the matrix and lists every battle whose overall winner (more seeds won) changed.

**The class probe (§2.1 of the plan).** A scratch worktree (`~/scratch/crowd-edge/probe-tree`, target directory
`~/cargo-target/crowd-edge-probe`) carries the tree under test plus the planner's instrumentation
(`~/scratch/crowd-edge-plan/scripts/probe.diff`, `probe.rs`) and driver (`zz_crowd_probe.rs`, writing under
`$HOME/$PROBE_OUT`):

```bash
PROBE_OUT=scratch/crowd-edge/probe/<wp> cargo test --release -p sim_core --test zz_crowd_probe -- \
  zz_probe_showcase zz_probe_arc zz_probe_corridor zz_probe_walkers
~/scratch/crowd-edge/scripts/classes.sh ~/scratch/crowd-edge/probe/<wp>/probe_*.txt
```

`classes.sh` runs the planner's `classify.py` and `contact.py` and maps their categories onto the plan's classes:
a reversal whose route changed is R-F; a proposal reversal with a surround-walk step on either tick is R-A, any
other proposal reversal R-E; a contact reversal whose overlap came from a receding allowance is R-B, from a wall
slide R-C, and every other contact reversal (a partner's step in, an overlap standing on both ticks, another step
in) R-D. The mapping differs from the planner's hand count by a reversal or two per run (the corridor's R-A is 18
here, 17 in plan §2.3).

Reproductions: `cargo test --release -p sim_core --test second_line_tests -- --include-ignored`.

## 0. WP0: records

**B-145's cause, verified (plan §2.6, "verify").** The plan supposed that player 1's unit 25 reverses on tick 2
because it takes its group's lane on the order's second tick. It does not. On every seed the showcase's two route
groups have an empty lane word and no `GroupLane` event (`groups.md` of the base run). The probe's trace of the
default seed (`probe_showcase_default.txt`, the reversal at tick 2) shows instead:

- **Tick 1.** The unit has a fresh route whose first corner is (58.51, 56.71). Its friend 26 stands at (58.5, 57.5),
  still and under an order, so it is parked on that corner (`parked_on_corner`), and the unit aims past it on the
  outside of the turn, at (59.6, 56.5) (`aim_past_parked_body`, plan `2026-11-09-sc2-pathfinding` D-9 A3). It steps
  east, (0.126, 0).
- **Tick 2.** Friend 26 now walks (velocity 0.126), so it is no longer parked. The unit aims at its route again,
  (58.39, 56.89), and steps (−0.066, 0.108): a reversal.

So R-F is a change of aim or route between two ticks under the same order. B-145 is written with this cause, and
with the planner's second case (a chase's first route after a refused step).

**Records.** `DESIGN.md` §4: B-103 restated with the figures of plan §2.2 and the named causes; B-144 and B-145
added. `docs/plans/README.md`: the plan's row (`Proposed`). Commit `c44a13cb`.

## 1. WP1: measurement, bit-identical

**Change.** `SimWorld::proposed_step(id)` returns the velocity of the motion proposal `id` committed in the latest
movement phase, read from a retained buffer of `MovementWorkspace` that the simulation never reads. The blocking
meter splits each reversal into proposal-caused (the proposed steps reverse by the same test) and contact-caused;
`reversals` is their sum. `matrix.csv` gains the columns `proposal_reversals` and `contact_reversals` (a table
without them is still read, its reversals unsplit), `route_trace`'s summary gains the two columns and
`reversals.csv` a `cause` column. In `second_line_tests.rs`, `ranged_block_reaches_the_arc` is un-ignored, the
showcase test is renamed `showcase_crowd_edges_do_not_reverse` and restated per plan D-6, and the corridor test is
restated per D-6 (standing within `STANDING_DRIFT`, exact zero reported); both stay ignored with the figures below.

**Bit-identical** to `720c6971` (`~/scratch/crowd-edge/base`, tools built from the WP0 tree):

- matrix: "bit-identical to …/base/matrix/matrix.csv" (1,160 runs, 0 expectation failures of record);
- battle 33: `seeds.csv` identical (64 seeds);
- showcase: `units.csv` identical on the default seed and seeds 1 to 8;
- the probe's logs on WP1's tree equal the planner's on `720c6971`, apart from the meter's own summary line.

`STANDING_DRIFT` is `pub(crate)` in `sim_core` and has no re-export (plan D-6, "verify"), so
`sim_fixtures::trace::blocking::STANDING_DRIFT` names the same 1/64, with a doc pointing to the constant.

### 1a. The base of every later WP

**Showcase** (3,000 ticks at most; every run ends earlier with player 1's victory):

| Seed | Outcome | Reversals P0 / P1 (proposal + contact) | Give-ups P0 / P1 | Deeper P0 / P1 |
|---|---|---|---|---|
| default | player 1 wins at tick 1946 | 13 (3 + 10) / 10 (5 + 5) | 0 / 0 | 0 / 0 |
| 1 | player 1 wins at tick 1928 | 10 (4 + 6) / 17 (4 + 13) | 0 / 0 | 0 / 0 |
| 2 | player 1 wins at tick 1913 | 12 (2 + 10) / 20 (6 + 14) | 0 / 0 | 0 / 0 |
| 3 | player 1 wins at tick 2194 | 12 (2 + 10) / 10 (7 + 3) | 0 / 0 | 0 / 0 |
| 4 | player 1 wins at tick 2616 | 18 (6 + 12) / 11 (6 + 5) | 0 / 0 | 0 / 0 |
| 5 | player 1 wins at tick 1924 | 12 (2 + 10) / 10 (5 + 5) | 0 / 0 | 0 / 0 |
| 6 | player 1 wins at tick 2175 | 12 (7 + 5) / 15 (9 + 6) | 0 / 0 | 0 / 0 |
| 7 | player 1 wins at tick 2187 | 17 (6 + 11) / 5 (3 + 2) | 0 / 0 | 0 / 0 |
| 8 | player 1 wins at tick 1921 | 12 (2 + 10) / 19 (5 + 14) | 0 / 0 | 0 / 0 |

Attackers in range at tick 900 (reported, not asserted, D-6): player 0 has none left, player 1 has 0 of 5.

**Reproductions** (release, `--include-ignored`):

| Test | Result |
|---|---|
| `ranged_block_reaches_the_arc` | Passes (un-ignored): 10 / 10 in range at tick 96, 0 reversals, 0 give-ups |
| `no_room_means_waiting_still` | Fails (ignored): 2 of 6 in range; 21 reversals (18 proposal, 3 contact; per unit 0, 1, 3, 7, 1, 9), 0 give-ups. Standing within `STANDING_DRIFT` / exactly still / ticks since the first stop, per unit in file order: 346 / 105 / 347, 298 / 139 / 340, 292 / 49 / 332, 225 / 150 / 324, 269 / 52 / 269, 206 / 94 / 316 (the rear units 3, 4, 5, 7 stand on 88, 88, 69, 65 %) |
| `walkers_at_the_crowds_edge_do_not_toggle` | Passes its bound (≤ 10 per walker): 31 reversals (23 proposal, 8 contact), per walker 1, 6, 2, 3, 0, 6, 7, 0, 0, 2, 3, 1; 17 / 17 in range |
| `showcase_crowd_edges_do_not_reverse` | Fails (ignored): 13 and 10 reversals against a bound of 0 and 3; 0 give-ups; the battle ends at tick 1,946 |

**Probe classes** (WP1's tree):

| Run | R-A | R-B | R-C | R-D | R-E | R-F | Proposal / contact |
|---|---:|---:|---:|---:|---:|---:|---|
| Showcase, default seed | 5 | 12 | 1 | 2 | 1 | 2 | 8 / 15 |
| Arc | 0 | 0 | 0 | 0 | 0 | 0 | 0 / 0 |
| Corridor | 18 | 1 | 0 | 2 | 0 | 0 | 18 / 3 |
| Walkers | 21 | 2 | 0 | 6 | 2 | 0 | 23 / 8 |

The showcase's out-of-scope residual on the default seed is none for player 0 and three for player 1 (unit 25 at
tick 2 and unit 23 at tick 438, R-F; unit 24 at tick 951, R-E). `showcase_crowd_edges_do_not_reverse` takes
`[0, 3]` as its bound; WP4 re-measures it.

**Matrix** (16 seeds, 1,160 runs): seeds of record 6,154 reversals (2,918 proposal, 3,236 contact), 2 give-ups;
all seeds 30,642 (15,230 proposal, 15,412 contact), 6 give-ups. Battles 26 to 29 (100 zerglings): 1,024, 1,033,
1,393 and 1,047 reversals on the seeds of record, 0 give-ups; battle 35: 668 reversals, 2 give-ups.

**Throughput** (`throughput` bench of WP1's tree, three runs under the gate lock, each the bench's median of three
repeats; the server was shared and loaded, so runs spread widely): every scenario stays far above 25 ticks/s.

| Map | Units | Ticks/s, median (runs) |
|---|---:|---:|
| 64x64 | 0 | 2550 (2550, 2652, 2415) |
| 64x64 | 100 | 508 (623, 508, 499) |
| 64x64 | 400 | 106 (106, 101, 115) |
| 128x128 | 400 | 128 (93, 128, 131) |
| 256x256 | 0 | 162 (139, 162, 187) |
| showcase_canyon | 400 | 91 (57, 91, 95) |
| 64x64, 67 casters | 400 | 200 (176, 205, 200) |

**Tests.** `sim_fixtures` trace unit tests (50, including `reversals_are_split_into_proposal_and_contact_caused`
and `an_unsplit_row_reads_with_its_reversals_and_compares_on_its_outcome`), `sim_core`
`the_proposed_step_is_the_committed_proposal_and_is_not_state`, `second_line_tests` as above, `workspace_checks`.
