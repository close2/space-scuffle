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

## 2. WP2: D-1, the walk never steps back — stopped (carried from `crowd-edge-wp2-wip`, documentation only)

**Verdict: WP2 stops (plan D-8 stop rule 7).** D-1 implemented as written is correct by its own unit tests where
they hold (the walk step keeps no backward component), but WP2's acceptance is not reached: the walkers test keeps
2 proposal reversals (0 required) and 16 of 17 attackers in range at tick 300 (17 required, plan K-2), CE-1 has one
contact reversal (0 required), and the guard CE-2 moves. Nothing of WP2 is committed on `crowd-edge-plan`; the
work is on branch `crowd-edge-wp2-wip`. The stop rules of the orchestrator's brief (winner, give-ups, deeper
unit-ticks, reversals) hold, apart from the matrix's all-seeds give-ups (below).

**What was built** (branch `crowd-edge-wp2-wip`). `steering::remove_blocked_velocity_ahead` projects the walk's
across step cyclically onto the bodies' half-planes, `H₀ = { v : v · ahead ≥ 0 }` last in each pass, until a pass
changes nothing and at most for "as many passes as there are constraints in reach, plus one" (read as the bodies
within reach, plus `H₀`, plus one). `surround_walk` takes its walk step from it; the forward step and steering are
unchanged (D-1.3). The per-body filter and room of `remove_blocked_velocity` were factored out (`blocks`,
`room_before`) without changing it.

**Control (stop rule 6).** The candidate with `surround_walk` calling `remove_blocked_velocity` (a scratch
environment flag, not committed) reproduces WP1 byte for byte: the matrix is "bit-identical to
…/wp1/matrix/matrix.csv", battle 33's `seeds.csv` and the showcase's `units.csv` on all nine seeds are identical,
and the walkers layout gives WP1's per-walker reversals (1, 6, 2, 3, 0, 6, 7, 0, 0, 2, 3, 1) and 17 / 17.

### 2a. Measurements of the candidate (`~/scratch/crowd-edge/wp2`, base WP1)

**Showcase:**

| Seed | Outcome (base) | Reversals P0 / P1 (proposal + contact) | Give-ups | Deeper | Base reversals |
|---|---|---|---|---|---|
| default | P1 at 1,948 (1,946) | 11 (3 + 8) / 5 (1 + 4) | 0 / 0 | 0 / 0 | 13 / 10 |
| 1 | P1 at 1,741 (1,928) | 10 (2 + 8) / 8 (3 + 5) | 0 / 0 | 0 / 0 | 10 / 17 |
| 2 | P1 at 1,907 (1,913) | 11 (1 + 10) / 5 (2 + 3) | 0 / 0 | 0 / 0 | 12 / 20 |
| 3 | P1 at 2,123 (2,194) | 11 (1 + 10) / 5 (2 + 3) | 0 / 0 | 0 / 0 | 12 / 10 |
| 4 | **not over at 3,000** (P1 at 2,616) | 11 (0 + 11) / 7 (1 + 6) | 0 / 0 | 0 / 0 | 18 / 11 |
| 5 | P1 at 1,908 (1,924) | 13 (1 + 12) / 7 (1 + 6) | 0 / 0 | 0 / 0 | 12 / 10 |
| 6 | P1 at 2,227 (2,175) | 9 (0 + 9) / 5 (1 + 4) | 0 / 0 | 0 / 0 | 12 / 15 |
| 7 | P1 at 1,959 (2,187) | 12 (0 + 12) / 4 (1 + 3) | 0 / 0 | 0 / 0 | 17 / 5 |
| 8 | P1 at 2,145 (1,921) | 11 (1 + 10) / 4 (1 + 3) | 0 / 0 | 0 / 0 | 12 / 19 |

**Matrix:** no battle's overall winner changes (23 battles moved). Seeds of record: 2,877 reversals (base 6,154),
of which 318 proposal (base 2,918) and 2,559 contact (base 3,236); give-ups 2 (base 2). All seeds: 16,023 reversals
(base 30,642), give-ups **8 (base 6)**: battle 35 seed 17 (not of record) gives up 2 Viking chases. Battles 26 to 29:
578, 623, 583, 464 reversals (base 1,024, 1,033, 1,393, 1,047), winners unchanged; battle 35: 71 (base 668).
**Battle 33** moves: blue wins 53 of the 64 seeds (base 47), no flip.

**Reproductions** (release, `--include-ignored`):

| Test | Candidate | WP1 |
|---|---|---|
| arc | 10 / 10 at tick 98, 0 reversals | 10 / 10 at tick 96, 0 |
| corridor | 27 reversals (**1 proposal, 26 contact**; unit 5 has 21), 0 give-ups; standing within `STANDING_DRIFT` 333 / 340, 311 / 332, 292 / 320, 291 / 310 for the rear units (98, 94, 91, 94 %) | 21 (18 + 3); 88, 88, 69, 65 % |
| walkers | **16 / 17** in range at tick 300, 14 reversals (**2 proposal**, 12 contact) | 17 / 17, 31 (23 + 8) |
| showcase | 11 / 5 reversals against the bound [0, 3]; ends at 1,948 | 13 / 10 |
| CE-1 | walk steps never retreat (least forward part −4·10⁻¹⁰); **1 contact reversal** | the walk step retreats by 0.054 at tick 4 |
| CE-2 | **moves** (digest 18423052291715427936 against WP1's 2585502486872808864) | — |

**Probe classes:**

| Run | R-A | R-B | R-C | R-D | R-E | R-F |
|---|---:|---:|---:|---:|---:|---:|
| Showcase, default seed | 1 | 3 | 2 | 7 | 2 | 1 |
| Arc | 0 | 0 | 0 | 0 | 0 | 0 |
| Corridor | 0 | 13 | 0 | 13 | 1 | 0 |
| Walkers | 2 | 6 | 0 | 6 | 0 | 0 |

The remaining "R-A" are not retreating walk steps (T2 below): `classes.sh` files any proposal reversal with a walk
step on either tick under R-A.

### 2b. Traces

Every trace compares the candidate with the rule-off control tick by tick (a scratch test, not kept).

- **T1, walkers, unit 12: never in range (rule-caused, K-2).** First divergent tick 54. With the rule off, unit 12
  starts its walk to the right with the step (−0.0249, 0), back from its goal (60, 32). With D-1 that step is cut
  to zero and it waits, then walks (0, −0.073). It later chases the pillar into the second ring and stands pinned at
  (36.28, 31.55) behind the firing ring, its walk closing on both sides; it is not in range by tick 600. With the
  rule off it backs out of that pocket and is in range at tick 209. A retreat out of a pocket between two firing
  friends was its only way in; D-1 forbids exactly that step.
- **T2, walkers, unit 9: the 2 proposal reversals (outside D-1).** Tick 140: steering's step (0.0249, −0.0402), then
  the walk starts on its kept side, (0.0392, 0.1198), opposite to steering's deflection. Tick 144: the walk's across
  step (0.0485, 0.1163), then the paused walk's forward step (0.0909, −0.0872), which steering deflects to the other
  side (D-1.3 leaves the forward step alone). Neither step goes back from the goal; the reversal is between a walk
  step and a steered step. Of the default seed's 4 proposal reversals, one is of this kind (unit 8, tick 392); the
  others are R-E (units 5 and 8, ticks 367 and 368) and R-F (unit 25, tick 2).
- **T3, CE-1 and the corridor: the walk presses into the body ahead (method defect).** The cyclic projection leaves
  `sin²φ` of the across step per pass, `φ` being the body's angle from the across direction. In the R-A geometry
  (plan §2.3) `φ` is about 68 to 73°, so the bounded passes leave 55 to 75 % of the step, and the walk steps across
  *into* the body ahead instead of stopping, as D-1.2 says it would; contact pushes it back. Unit test: a body
  touching at 45° leaves 0.0158 after three passes, just above the waiting share 0.01575, so the side stays open.
  The corridor's contact reversals rise from 3 to 26 this way (R-B 13, R-D 13), while its proposal reversals fall
  from 18 to 1.
- **T4, CE-2: the arc moves (rule-caused; the plan's premise does not hold).** First divergent tick 56, unit 8 (rear
  row): its walk step on WP1 is (−0.0369, −0.1046), a forward part of −0.037, while the arc is still forming. D-1
  holds it, (0.0007, −0.0533). `H₀` binds on the way to a convex arc; the arc still reaches 10 / 10 by tick 98 with
  no reversal.
- **Showcase seed 4** is not over at tick 3,000 (P1 won at 2,616); not traced (seed 4 is not a winner stop rule).

### 2c. The exact projection (scratch variant, not the plan's method)

To separate the rule from its method, a scratch variant computed D-1.1's projection exactly: when the bodies'
projection keeps no backward component it is taken as is; otherwise `H₀` binds and the step lies on the across line,
as long as the bodies allow (up to touching the first). D-1.2 then holds and CE-1 passes with 0 reversals, but:
walkers **9 of 17** in range at tick 300 (0 reversals), the arc **7 of 10** at tick 300 and never all, and the
showcase's default seed ends at tick 2,271 with 393 / 1,660 blocked-attacker ticks (base 201 / 267). That is K-2 in
full: the retreat is the way out of a pocket. The variant is kept at
`~/scratch/crowd-edge/wp2-files/steering-exact.rs`.

A second scratch experiment kept the retreat and changed the end test instead (a walk does not end on a tick whose
last achieved step went back from the goal): walkers 0 proposal and 4 contact reversals but 16 / 17 at tick 300, and
the showcase default seed not over by tick 2,700. Not proposed as it stands.

### 2d. Proposed amendment (for the orchestrator)

D-1 removes R-A's proposal reversals (matrix 2,918 → 318 of record, corridor 18 → 1) but costs reach (T1) and, as
written, moves the cost into contact (T3). Its two parts conflict: the bounded cyclic method does not close the
sides D-1.2 says it closes, and the exact projection closes them but leaves units in pockets they could only leave
backwards. Proposed:

1. **Re-plan D-1** (Stage 1): the cycle of plan §2.3 is "retreat, the straight step frees, the walk ends, step in".
   Forbidding the retreat fixes the cycle but not the pocket. A rule on the *end* of the walk (the walk does not end
   on the room its own retreat made) keeps the way out; the scratch experiment above is a first measurement, not a
   design.
2. **Restate CE-2** as a guard against `H₀` binding *after* the arc has formed (from tick 98), or drop it: its
   premise does not hold while the arc forms (T4).
3. **Restate the walkers acceptance** for walk-against-steering reversals (T2), which D-1 does not address: either a
   rule for the walk's start and end against steering's side, or a bound on proposal reversals that are not
   retreats (R-A proper is 0 on the candidate).
4. WP3 (D-2) does not depend on D-1 and could run first on WP1's tree; D-2 would also absorb T3's contact cost.

## 3. WP3: D-2, no step ends inside a friend that stood — stopped (carried from `crowd-edge-wp3-wip`, documentation only)

Amendment 1 withdrew D-1, so this WP ran on WP1's tree. Amendment 2 withdrew D-2 after this stop.

**Verdict: WP3 stops (plan D-8 stop rule 1).** Matrix battle 31 (`31_15_marauders_vs_15_stalkers`) changes its
overall winner. The work is on branch `crowd-edge-wp3-wip`; nothing of WP3 is on `crowd-edge-plan`.

**What was built.** `movement/body_settle.rs`: a last stage of `advance_checked_movement`, after every proposal and
before collision, as D-2.1 writes it.
- **Movers:** bodies with a proposed step this tick (so not casts) that moved.
- **Settled against:** friends of the same body class that stood when the stage starts, with zero velocity, unless
  they yield (`yields_to`). Enemies, mining or collision-suppressed bodies and structures are left out.
- **Method:** `remove_blocked_velocity` from `pos_at_tick_start` alternates with `settle_on_terrain`, at most
  `WALL_CONTACT_PASSES` times; a step still inside a friend by more than `WALL_CONTACT_TOLERANCE` after that waits.
  The waiting rule uses `WAIT_THRESHOLD × entry.step`.
- **Hull:** a shortened step turns the hull with what is left of the tick's budget. `turn_hull_towards` is not
  idempotent within a tick (plan "verify"): a second call spends only the remaining budget, so the tick's turn stays
  within the moving rate. A `turn_before_move` unit that cannot face the shortened step waits (its hull gate).
- **Unit test:** two movers and one standing friend settle the same in either order (D-2.2).
- **Not written:** CE-3 to CE-6 and the R-B-only control, because the WP stopped at its first measurement.

**Control (stop rule 6).** With the stage switched off (scratch environment flag), the run reproduces WP1 byte for
byte: the matrix ("bit-identical"), battle 33's `seeds.csv` and the showcase's `units.csv` on all nine seeds.

### 3a. Measurements (`~/scratch/crowd-edge/wp3`, base WP1)

| Seed | Outcome (base) | Reversals P0 / P1 (proposal + contact) | Give-ups | Deeper | Base |
|---|---|---|---|---|---|
| default | P1 at 2,216 (1,946) | 11 (8 + 3) / 5 (2 + 3) | 0 / 0 | 0 / 0 | 13 / 10 |
| 1 | **not over at 3,000** (P1 at 1,928) | 7 (5 + 2) / 13 (9 + 4) | 0 / 0 | 0 / 0 | 10 / 17 |
| 2 | **P0** at 2,597 (P1 at 1,913) | 11 (7 + 4) / 7 (5 + 2) | 0 / 0 | 0 / 0 | 12 / 20 |
| 3 | **not over** (P1 at 2,194) | 11 (7 + 4) / 7 (5 + 2) | 0 / 0 | 0 / 0 | 12 / 10 |
| 4 | P1 at 2,205 (2,616) | 11 (8 + 3) / 5 (3 + 2) | 0 / 0 | 0 / 0 | 18 / 11 |
| 5 | P1 at 2,604 (1,924) | 11 (7 + 4) / 6 (4 + 2) | 0 / 0 | 0 / 0 | 12 / 10 |
| 6 | **P0** at 2,602 (P1 at 2,175) | 8 (5 + 3) / 7 (4 + 3) | 0 / 0 | 0 / 0 | 12 / 15 |
| 7 | P1 at 2,191 (2,187) | 9 (6 + 3) / 15 (11 + 4) | 0 / 0 | 0 / 0 | 17 / 5 |
| 8 | **P0** at 2,585 (P1 at 1,921) | 11 (7 + 4) / 8 (6 + 2) | 0 / 0 | 0 / 0 | 12 / 19 |

Showcase contact reversals fall on every seed. The default seed falls from 10 / 5 to 3 / 3, and its winner holds.
But battles run longer, three seeds flip to player 0 and two do not end.

**Matrix.**
- **Winner:** battle 31's overall winner flips (stop rule 1): base blue 6, red 7, unresolved 7; candidate blue 10,
  red 4, unresolved 6.
- **Seeds of record:** 7,013 reversals (base 6,154): contact 3,194 (base 3,236; WP3's own class does not rise),
  proposal 3,819 (base 2,918). Give-ups 2 (base 2).
- **All seeds:** give-ups 8 (base 6).
- **Battles 26 to 29:** reversals rise to 1,105, 1,201, 1,691 and 1,442; winners unchanged.
- **Battle 33:** blue wins 45 of 64 seeds (base 47), no flip.

**Reproductions:**
- arc: passes, 10 / 10.
- walkers: passes, 17 / 17.
- corridor: 7 reversals (4 proposal, 3 contact). **1 of 6 in range (base 2), 2 give-ups (base 0).**
- showcase test: 11 / 5 against `[0, 3]`.

**Probe classes:**

| Run | R-A | R-B | R-C | R-D | R-E | R-F |
|---|---:|---:|---:|---:|---:|---:|
| Showcase, default seed | 7 | 4 | 2 | 0 | 1 | 2 |
| Corridor | 3 | 1 | 2 | 0 | 1 | 0 |
| Walkers | 11 | 2 | 0 | 6 | 2 | 0 |

R-B and R-C are not at 0. The probe labels a contact reversal R-B or R-C from the proposal's tags, before the stage.
A step settled by D-2 can still overlap a partner that moved, which D-2 leaves to collision (D-2.2). This attribution
was not traced further.

### 3b. Trace of the battle-31 flip (rule-caused)

Seed of record 3203386110 goes from base red 0–5 at tick 507 to unresolved 3–1 at tick 1,000.
- **First divergent tick 54, blue unit 9** (a marauder in cooldown, walking). Its achieved step is (0.1260, −0.0019)
  against base (0.1260, 0.0006).
- Its friend 12, standing at a centre distance of 1.129 (touching is 1.125), stood still for the first time that
  tick: velocity 0, firing cooldown.
- The stage removed unit 9's approach into 12, as D-2 means it to.

This is the plan's rule-caused class: "a unit that walked into a standing friend now does not". The divergence then
grows chaotically. Battle 31 is close to even over its seeds (base 6–7–7), and nearly every seed's outcome
reshuffles: 19 of 20 change.

### 3c. Proposed amendment (for the orchestrator)

- **Battle 31.** If the orchestrator accepts the flip as rule-caused, the next stop items are the reach costs: three
  showcase seeds flip to player 0 and two do not end, and the corridor loses a unit in range and gains 2 give-ups
  (WP4's D-4 targets those give-ups). These need a trace before WP4.
- **Proposal reversals.** The matrix's proposal reversals rise by 901, which suggests that shortened steps feed the
  next tick's acceleration and steering (`velocity` read as last tick's achieved step). The R-A residual stays open
  without D-1.

## 4. WP4: D-3 and D-4, the queue stands — stopped (carried from `crowd-edge-wp4-wip`; restated in §5)

Amendments 1 and 2 withdrew D-1 and D-2, so this WP ran on WP1's tree with the main merge (`52cc5efb`). Q0089 is
answered (a).

**Verdict: WP4 stops** on two stop rules, both caused by D-3:
- **Stop rule 1:** matrix battle 31 changes its overall winner.
- **Stop rule 2:** the matrix's give-ups of record rise from 2 to 3.

The work is on branch `crowd-edge-wp4-wip`. Amendment 3 then landed D-4 alone (§5).

**What was built.**
- **D-3.** `FLANK_RETRY_TICKS` and the spent walk's timed retry are removed. A spent walk waits, and it re-arms,
  keeping its side, when `note_progress` resets the stall count (the unit gained ground) or its target changes.
- **D-4.** On a tick where a friend that holds its ground (`anchored`) lies within one step ahead
  (`friend_holds_ahead`, which shares `friend_ahead` with `friend_blocks_ahead`), the stall count does not
  advance and the best distance is kept. This applies to chases and attack-moves. A plain `Move` is unchanged.
- **Tests.**
  - CE-7 `a_queued_chaser_keeps_its_target` is red on WP1: the chaser stands on 386 of 580 ticks and has 17
    reversals.
  - CE-8 `a_spent_walk_resumes_when_the_way_opens` is green on WP1 except its new re-arm assertion.
  - CE-9 `a_blocked_move_is_still_given_up` is green on WP1 and WP4.
  - The corridor test is restated, as below.

**Restatements (Amendment 1 item 2, recorded here).**
- The corridor's 0-reversal bound assumed D-1, which was to remove its 18 R-A reversals (§1a). Its 7 reversals on
  WP4 (6 proposal, 1 contact) are those walk retreats. The bound is not loosened: it moves, unchanged, to a new
  test `no_room_means_no_reversals`, which stays ignored until the re-plan of D-1.
- `no_room_means_waiting_still` keeps 95 % standing, 0 give-ups and units left out, and is un-ignored. Its first
  stop is now the first tick the unit stands within `STANDING_DRIFT`, not the first tick exactly still. On WP4 one
  unit stands on 309 of 309 ticks without ever being exactly still (contact creep, §2.7), so the exact-zero first
  stop never started its count.
- CE-7 and CE-8 use two shooters side by side. One shooter leaves room beside it at the pillar's face, so the
  queued unit reached range there.
- CE-8 runs in the corridor: in the open walkers layout no walk is spent within 600 ticks.

### 4a. Measurements (base WP1)

- **Showcase:** `units.csv` is identical to WP1 on all nine seeds (no showcase unit's walk is spent or queued
  in a way that changes it), so every §1a figure holds.
- **Battle 33:** moves, blue still wins 47 of 64 (no flip).
- **Matrix, 7 battles moved:**
  - seeds of record: 6,121 reversals (base 6,154), 2,911 proposal and 3,210 contact; give-ups **3** (base 2,
    one in battle 31 seed 11);
  - all seeds: give-ups 7 (base 6);
  - **battle 31's overall winner flips:** base blue 6, red 7, unresolved 7; candidate 13, 5, 2;
  - battles 26 to 29: 1,021, 1,045, 1,307 and 1,036 reversals (base 1,024, 1,033, 1,393, 1,047), winners unchanged.

**Reproductions** (release):
- arc 10 / 10;
- walkers 17 / 17, within its bound;
- `no_room_means_waiting_still` passes: 2 of 6 in range, 0 give-ups, the rear units standing on 98, 99.7, 96, 100
  and 97 %;
- `no_room_means_no_reversals` fails as expected (ignored): 7;
- the showcase test is as WP1;
- CE-7, CE-8 and CE-9 pass.

**Goldens.**
- `golden_replay` moves from checkpoint 4. Entity 34, attack-moving behind a friend that holds its ground, keeps
  its stall count at tick 172 (D-4); its pose first differs at tick 397, where it no longer drops its route for a
  stall repath. With D-3 alone off nothing changes; with D-4 off, or both off, the replay reproduces the previous
  checksums.
- Re-recorded with that proof header. It passes native debug and release, and `wasm32-wasip1` release and debug
  under wasmtime.
- `golden_replay_orders` and the sandbox checksum do not move.

### 4b. Trace of the battle-31 flip (rule-caused, D-3)

- **Scratch flags.** With D-3 and D-4 both off, battle 31's seed-of-record trace equals WP1's. With D-3 alone off
  it equals WP1's; with D-4 alone off it equals the candidate's. So D-3 decides.
- **Seed of record 3203386110** goes from base red 0–5 at tick 507 to blue 4–0 at 644. First divergent tick 135,
  red stalker 23 (and 21 a tick later): on WP1 it walks again, (0.066, 0.151), its spent walk retried after 24
  ticks; on WP4 it stands idle, spent, since it has not gained ground. The same holds on seed 11, the seed that
  gives up (first divergence at tick 130).
- **Classification: rule-caused.** It is the plan's K-6: a crowd that changes so that a side opens without the unit
  advancing no longer re-arms the walk. In an open fight the timed retry was how units found a new way round a
  shifting front.
- **D-4 alone** (D-3 off) leaves battle 31's traced seeds, the showcase and the corridor as WP1. The corridor's
  standing comes from D-3: with the retry back, the rear units stand on 88, 88, 69, 65 % again.

### 4c. Proposed amendment (for the orchestrator)

D-3 trades the corridor's standing for open-field reach: battle 31 flips and one more order is given up. Options:

1. Keep the timed retry and re-arm on ground gained as well. D-4 then keeps queued units' orders, and the retry still
   walks them. This is the corridor as WP1 (standing 65 to 88 %), so B-103's corridor item stays open.
2. Re-arm a spent walk when a side opens, not on a timer and not only on ground gained. This is K-6's further re-arm,
   and it needs a measurable "side opens" test: a Stage 1 decision, probably with the new plan for R-A to R-C.
3. Accept battle 31's flip as rule-caused (it is near even over its seeds: base 6–7–7). The extra give-up (seed 11)
   would still need a trace and a decision under stop rule 2.

## 5. WP4 restated (Amendment 3): D-4 alone

D-4 lands alone and the timed retry (`FLANK_RETRY_TICKS`) stays. §4 records the run with D-3, which stopped. Only D-4
is taken from `crowd-edge-wp4-wip`: `friend_holds_ahead` and the queued tick in both progress checks
(`note_queued_progress`).

**Tests.**
- CE-9 `a_blocked_move_is_still_given_up` passes: a plain `Move` keeps S8's give-up.
- D-3's tests stay as ignored tests, with their D-4-alone figures in the ignore reasons, for the new plan:
  - CE-7: the chaser keeps its target, but walks on the retry, standing on 386 of 580 ticks.
  - CE-8: the unit reaches range, but its walk does not re-arm without D-3.
  - `no_room_means_waiting_still`: 0 give-ups and 2 of 6 in range, but the rear units stand on 88, 88, 69, 65 %.
- `no_room_means_no_reversals` stays ignored, as in §4.

**Measurements** (`~/scratch/crowd-edge/wp4b`, base WP1), under the unchanged stop rules:
- **Matrix:** bit-identical to WP1. No battle moves, so there are no flips, give-ups are 2 of record and 6 over
  all seeds, and the reversals are 6,154 of record.
- **Battle 33:** `seeds.csv` identical.
- **Showcase:** `units.csv` identical on all nine seeds, so every §1a figure holds, including 0 give-ups and 0
  deeper unit-ticks.
- **Reproductions:** arc 10 / 10 and walkers 17 / 17, as WP1. The showcase test is as WP1.

**Goldens.**
- `golden_replay` moves from checkpoint 4. Entity 34, attack-moving behind a friend that holds its ground in combat,
  keeps its stall count at tick 172 (0, was 1). Its pose first differs at tick 397, where it no longer drops its
  route for a stall repath.
- Proof: with D-4 off, the replay reproduces the previous checksums at every checkpoint, so its D-4-on checksums
  are the ones recorded with the proof header.
- The replay passes native debug and release, and `wasm32-wasip1` release and debug under wasmtime.
- `golden_replay_orders` and the sandbox checksum do not move.

**Throughput.** The WP1 and D-4 bench binaries were alternated, three rounds each under the gate lock. The server
was loaded by another agent's gates, so single runs range down to half the others. Every scenario stays at or above
57 ticks/s.

The D-3-and-D-4 binaries were alternated with WP1 the same way an hour earlier, under less load. D-4's addition is
one neighbour scan per walking or chasing actor, and that run shows it within noise.

| Map | Units | WP1 | D-4 | WP1 (earlier) | D-3 + D-4 (earlier) |
|---|---:|---:|---:|---:|---:|
| 64x64 | 0 | 2830 | 2433 | 3261 | 3230 |
| 64x64 | 100 | 488 | 504 | 932 | 887 |
| 64x64 | 400 | 96 | 151 | 162 | 161 |
| 128x128 | 400 | 175 | 174 | 187 | 188 |
| 256x256 | 0 | 202 | 189 | 201 | 199 |
| showcase_canyon | 400 | 121 | 62 (117, 62, 57) | 115 | 111 |
| 64x64, 67 casters | 400 | 130 | 116 | 231 | 219 |

Each figure is the median of three runs, and each run is the bench's own median of three repeats.

## 6. WP5: storyboard and bookkeeping

- **Storyboard.** The showcase's default-seed timeline did not move (`units.csv` identical to WP1's), so
  `assets/captures/showcase.ron` needs no re-check and no render was made.
- **`DESIGN.md`.**
  - B-103 is restated. D-4 landed. Open, with the measured residuals: R-A to R-C (showcase default seed 13 and
    10 reversals; the matrix's seeds of record 6,154), the walk's re-arm without the timed retry (D-3, K-6), and
    the corridor's standing (65 to 88 %, 7 to 21 reversals). These go under the new plan still to be written.
  - The Movement row names D-4 and `crowd_edge_tests.rs`.
- **Plans.**
  - This plan's row is `Partly done`, with commits.
  - Friendly-blocking gets a pointer amendment: B-103 is carried here, and its Amendment 4 change 5 (the timed
    retry) stays until the new plan.
- **OQ-1** was filed as Q0089 and answered (a); it is implemented by D-4.
