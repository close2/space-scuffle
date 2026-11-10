# Evidence: Friendly Blocking (plan `2026-11-09-friendly-blocking`)

One section per commit of plan [`2026-11-09-friendly-blocking`](../plans/2026-11-09-friendly-blocking.md). Each
records the whole-matrix comparison (16 seeds beyond the seeds of record, 1,000 runs), battle 33 on 68 seeds (its 4
of record plus 64), the golden replays, and the blocking metrics of WP1 on the matrix and on the showcase battle.
Measured on the release build of each commit with:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/blocking
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --all-battles --seeds 16 --jobs 8 --out <step>/matrix --compare <previous>/matrix/matrix.csv
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <step>/b33
cargo run --release --locked -p sim_core --example route_trace -- \
    --scenario scenarios/showcase.ron --ticks 1500 --out <step>/route
```

The base is `main` at `05d9dda` (pathfinding Part A, checksum schema 22), measured from this branch's own build.

**Showcase metrics.** From `route_trace` (`summary.md`): per player, attackers with their target in range after
tick 900 (`sim_fixtures::trace::blocking::attackers_in_range`), step reversals (D-1.4), unit-ticks blocked, orders
given up (`NoProgress`, `PursuitAbandoned`), the mean total turn of player 0's units at the foot of ramp E (plan
Amendment 1: at most 115°), and the outcome.

## WP1: measurement tooling (test support only)

**What.** `sim_fixtures::trace::blocking` counts, per owner, blocked attacker ticks, reversals under an unchanged
order (D-1.4) and stall give-ups. The whole-matrix mode writes both sides' totals as three new `matrix.csv` columns
and a "Blocking" section in `matrix.md`; tables are still compared on their outcome columns only, and a table from
before the columns is still read. `route_trace` reports the metrics and attackers in range at tick 900 per owner.
`crates/sim_core/tests/second_line_tests.rs` holds the three reproductions of plan WP1, ignored with a reason naming
the package that makes each pass (WP2, WP3), so this commit records them without asserting wrong behaviour.

**Matrix.** Bit-identical to the base (all 1,000 rows). Blocking totals over the matrix, both sides: 1,188,969
blocked attacker ticks, 166,241 reversals, 22 give-ups.

**Battle 33.** 68 seeds, identical files: blue wins 41, red wins 10, 17 unresolved; mean survivors 4.32 – 0.74.

**Goldens.** Not run: no simulation code changed.

**Showcase, before** (the base; the planning probe of §2 ran before pathfinding Part A and differs in detail):

| Player | In range at tick 900 | Blocked attacker ticks | Reversals | Give-ups |
|---|---|---|---|---|
| 0 | 6 of 10 | 82 | 941 | 4 |
| 1 | 5 of 10 | 1,194 | 73 | 7 |

Player 0's pulse bots 4, 5, 8 and 9 reverse 227 to 250 times each from ticks 527 to 667 on; player 1's units
mostly stand behind their line (pulse bot 24: 40 reversals). Ramp E foot, player 0: mean turn 206°, maximum 292°
(walked/chord 1.573). Outcome: player 0 wins at tick 1,355.

**Reproductions, before** (`cargo test -p sim_core --test second_line_tests -- --include-ignored`):

| Test | Before |
|---|---|
| `showcase_second_line_reaches_range` | as the table above |
| `ranged_block_reaches_the_arc` | 6 of 10 in range at tick 300, never all; 324 reversals, 0 give-ups |
| `no_room_means_waiting_still` | 3 of 6 in range; 464 reversals, 3 give-ups; the three rear units stand on 29 %, 42 % and 72 % of their ticks after their first stop |

**M-19 and M-21 under the metric (plan WP1 item 5).**
- **M-19** (the marching crowd near (28 to 30, 10 to 12) on the showcase map, ticks 152 to 180) no longer
  reproduces on the base: over ticks 2 to 500 the showcase run shows 3 reversals in total, none in that window.
  Pathfinding Part A (B-99 to B-101) removed the corner rewrite that crowded the column there. M-19 is closed by
  that plan's commits, not by this one.
- **M-21** (melee units at a contact ring stepping back once per weapon cycle) happens while an enemy touches the
  unit: the enemy's contact push is uncapped by design and moves the unit in, and its own step brings it back. D-1.4
  excludes ticks on which an enemy touches the unit, so M-21 is outside D-a and D-b (steering against friends that
  will not yield) and stays with the deviation plan's WP16 with B-50 (melee packing, Q0035).

## WP4: the push table (rule; landed before WP2)

**Order.** Plan WP4 allows the push table to land before WP2, and it must: R1's waiting leaves a unit standing still
under its order, and without D-4 a standing unit counts as idle, so a friend walking into it pushes it on through the
firing line (plan D-4). Measured on WP2's first draft, a waiting unit that a squadmate walked into stepped in and was
pushed back every tick.

**What.** `SteeringObstacle::idle` and `PushEntityParams::idle` carry whether a body has nothing to do (no order,
`Idle` or `Stop`; `Entity::is_idle`). Only an idle, standing friend steps aside in steering (`yields_to`) or is pushed
sideways in contact; a body standing still under an order separates evenly, as moving friends do. Tests: one per row
of D-4 in `push_priority_tests.rs` (`push_table`), plus enemies never pushing and a waiting unit not pushed.

**Not bit-identical**, against the plan's expectation. The plan's probe saw no waiting units, but standing still under
an order is common today: a whole squad on the first tick after its order, before any of it has moved. Traced on
golden replay: on tick 6, the first tick the `blue_infantry` squad walks after its attack-move of tick 5, its
marines (entities 17 to 22 and on) all stand still under that order; each used to count its neighbours as idle and
walk into them, and now treats them as bodies that will not yield.

**Matrix.** 10 battles moved, **no overall winner changed**: 9a (1 seed), 26 to 30 (17 to 20 seeds each, no single
winner changed), 31 (20 seeds; single-seed winners changed on 3, blue wins 19 → 17 of 20), 33 (12 single-seed
changes, blue wins 12 → 9 of 20, red 1 → 3), 34 (no winner changed), 35 (6 single-seed changes, blue wins 13 → 15).
Blocking totals: 1,173,188 blocked attacker ticks, 166,303 reversals, 18 give-ups.

**Battle 33**, 68 seeds: blue wins 41 → 34, red 10 → 12, unresolved 17 → 22; mean survivors 4.32 – 0.74 →
3.76 – 0.96. Blue still wins overall.

**Goldens.** Both re-recorded with the cause above: `golden_replay` from checkpoint 1 (ticks 1 to 50, tick 6),
`golden_replay_orders` from checkpoint 3 (ticks 21 to 30, tick 21, the first walking tick of `blue_column`). The
sandbox checksum is unchanged (no schema change). Identical in debug, release and on `wasm32-wasip1`.

**Test restated first** (test-only commit): `movement_contact_tests::unreachable_order_reports_retargets_and_terminates`
now allows 1,200 ticks; the last of its 20 units settles on tick 900, one tick past the old bound, because units
waiting behind the crowd at the wall are no longer pushed through it.

**Showcase:**

| Player | In range at tick 900 | Blocked attacker ticks | Reversals | Give-ups |
|---|---|---|---|---|
| 0 | 6 of 10 | 331 | 604 | 5 |
| 1 | 5 of 10 | 1,205 | 455 | 7 |

Ramp E foot, player 0: mean turn 202° (maximum 266°). Player 0 wins at tick 1,361 (was 1,355).

## WP2: R1, waiting (rule)

**What.** `steering::remove_blocked_velocity` repeats its per-body removal until no body that will not yield is
approached (at most one pass per such body in reach), so a step into a corner is removed whole. `steer_body` joins
the nearest standing (anchored, or exactly still) non-yielding bodies on each side of the path into one wall when
their edges are closer than the unit's diameter plus 0.05, follows the wall through the unit's neighbours to its
ends, and rounds the nearer end only when the point beside it lies within `STEERING_LOOKAHEAD` (0.35) of the unit;
otherwise the unit heads straight into the wall. A unit whose admissible step is at most `WAIT_THRESHOLD` (1/8) of
its step waits: no move, no turn, `Idle`, order kept.

**Deviations from D-1.1, recorded (Q0002).** The planner's per-pair rule (the nearer end of the two nearest bodies)
and a memoryless wall end were tried first; both vibrate along a firing line of more than two bodies, because the
nearest pair, and the chain of bodies the unit can see, change with every small step: in the synthetic arc the
reversals rose from 324 to 702 (per pair) and stayed at 220 to 592 (memoryless ends, moving bodies included). Moving
bodies are left out of walls (they leave by themselves; pathfinding Amendment 1's `parked_on_corner` precedent), and
an end is rounded only when it is known to be one. Walking round a crowd on a kept side is D-2's surround walk.

**Tests.** `steering::wall_tests` (a 0.70 gap is a wall, 0.81 is not; rounding at a wall's end; a step into a
right-angle corner is removed whole and a 60° wedge leaves no approach), and the D-1.4 property test
`movement_contact_tests::units_blocked_by_a_holding_line_never_reverse` (eight layouts; a unit nudged by a
squadmate's contact may settle back once, under 0.01 a tick). `no_room_means_waiting_still` stays ignored until WP3:
its chasing units still flip sides every tick through `flank_if_blocked`, defect D-c, which WP3 replaces (the plan
expected R1 alone to end the vibration).

**Matrix.** 17 battles moved, no overall winner changed. Single-seed winners changed in battles 31 (6 seeds), 32 (5;
blue wins 4 → 6 of 20, red 8 → 7), 33 (10; blue wins 9 → 15) and 35 (2). Blocking totals: 1,217,899 blocked
attacker ticks, 105,040 reversals (−37 %), 21 give-ups.

**Battle 33**, 68 seeds: blue wins 34 → 52, red 12 → 9, unresolved 22 → 7; survivors 3.76 – 0.96 → 5.76 – 0.53.

**Goldens.** Re-recorded: `golden_replay` from checkpoint 2 (the second removal pass first changes a step on tick 63,
a `blue_infantry` marine pressed between squadmates, by one raw unit; waiting first moves a unit on tick 73),
`golden_replay_orders` from checkpoint 6 (tick 54, a `blue_column` unit; waiting first on tick 146). Each cause was
traced by switching the three parts of R1 off one at a time in a scratch build.

**Showcase.** **The winner flips**: player 1 wins at tick 1,254 (was player 0 at 1,361).

| Player | In range at tick 900 | Blocked attacker ticks | Reversals | Give-ups |
|---|---|---|---|---|
| 0 | 6 of 10 | 74 | 6 | 4 |
| 1 | 8 of 10 | 673 | 476 | 2 |

Player 0's attack-movers now wait instead of vibrating (941 → 6 reversals) but still cannot reach range; player 1's
chasing units still flank through `flank_if_blocked` and two more of them reach range. Ramp E foot, player 0: mean
turn 211° (maximum 287°).

## WP3: R2 and R4, the surround walk (rule; HELD under the stop rule)

**Latent commit first.** Checksum schema 22 → 23: `MovementMemory` gains `flank_active`, `flank_reversed`,
`flank_spent`, `flank_blocked_ticks`, `flank_walked` and `flank_target`, all hashed. The plan named `flank_active`
only; the kept side's `FLANK_REVERSE_TICKS` count, the one-circle bound of D-3 and the reset on a new target need the
others. Both goldens and the sandbox checksum re-recorded; no behaviour changed.

**What.** `flank_if_blocked` is replaced by `surround_walk` (`movement/locomotion.rs`), for a chase out of range and
for an attack-move without a target, when the archetype's new `surround_walk` field is `true` (the default, Q0074).
It starts when the forward part of the admissible step is below half the step and a standing friend that will not
yield lies just ahead, and goes on until the forward part reaches half the step again. The side comes from
`steering::surround_side` (the wall's nearer end, else the open side, else ID parity) and is kept; a side closed for
`FLANK_REVERSE_TICKS` (12) is swapped once, and when the other closes too, or a full circle round the goal has been
walked, the walk is spent and the unit waits. The sides tried and the distance walked count for the whole approach to
one target (a crowd that opens for one tick must not restart the walk; with a reset on every free tick, corridor
units walked back and forth for ever). A walking unit's stall evidence is cleared each tick (R4).

**Showcase.** **Player 1 still wins**, at tick 1,171.

| Player | In range at tick 900 | Blocked attacker ticks | Reversals | Give-ups |
|---|---|---|---|---|
| 0 | 8 of 10 | 118 | 40 | 1 |
| 1 | 9 of 10 | 603 | 51 | 0 |

Ramp E foot, player 0: mean turn 211° (unchanged; Amendment 1 asks for at most 115°).

**Reproductions:** `ranged_block_reaches_the_arc`: all 10 in range from tick 159, 0 give-ups, but 7 reversals.
`no_room_means_waiting_still`: 5 reversals; the rear units stand on 91 % of their ticks after their first stop; 3
give-ups (waiting counts as stalled, D-3). The three tests stay ignored with these figures.

**Matrix.** 18 battles moved; blocking totals 1,367,407 blocked attacker ticks, 40,069 reversals (−62 % on WP2), 8
give-ups. **Four overall winners flip** (stop rule):

| Battle | Wins blue / red, WP2 → WP3 | Single-seed winner changes |
|---|---|---|
| 9a_hellion_vs_4_lings | 20 / 0 → 6 / 14 | 14 |
| 28_100_lings_vs_10_immortals | 20 / 0 → 0 / 19 | 20 |
| 31_15_marauders_vs_15_stalkers | 17 / 3 → 7 / 13 | 12 |
| 32_30_marines_vs_15_banelings | 6 / 7 → 7 / 7 (tie) | 4 |

In 9a and 28 the zerglings, which used to stand behind those already biting, now walk round and surround their
target, as zerglings do in StarCraft II; whether these expectations or the walk should change is the orchestrator's
and owner's call (Q0074 offers `surround_walk: false` per archetype).

**Battle 33**, 68 seeds: blue wins 52 → 54, red 9 → 8; survivors 5.76 – 0.53 → 5.46 – 0.49.

**Goldens.** Re-recorded on the held branch: `golden_replay` from checkpoint 1 (ticks 1 to 50); `golden_replay_orders`
unchanged (no unit of it is blocked under an attack order). The per-tick cause was not traced, because the commit is
held.

## Round 2: the integration base, WP2 and WP3 released, Amendment 4

**Base.** The integration tree `b7ac748` (main `05d9dda` with the client mode commands and the owner answers to
Q0058, Q0063, Q0065, Q0067 and Q0069; checksum schema 24). The "base" row below is that tree with WP1's tooling only
(its `sim_core` sources); every step was measured again from its own release build with the commands at the top
(`--jobs 8`), nothing compared against round 1.

**Showcase** (`route_trace`, player 0 / player 1):

| Step | In range at tick 900 | Reversals | Give-ups | Ramp E turn, player 0 | Outcome |
|---|---|---|---|---|---|
| base | 6 / 5 of 10 | 941 / 73 | 4 / 7 | 206° | player 0 at tick 1,355 |
| WP4 | 6 / 5 | 604 / 455 | 5 / 7 | 202° | player 0 at 1,361 |
| WP2 | 6 / 8 | 6 / 476 | 4 / 2 | 211° | **player 1** at 1,254 |
| WP3 as held | 8 / 9 | 40 / 51 | 1 / 0 | 211° | **player 1** at 1,171 |
| Amendment 4 | **10 / 10** | 7 / 4 | **0 / 0** | 211° | player 0 at 1,098 |

**Matrix** (1,000 runs; totals of both sides: blocked attacker ticks, reversals, give-ups):

| Step | Battles moved | Overall winner flips (against the step before) | Totals | Expectation failures, of record / all |
|---|---|---|---|---|
| base | — | — | 1,269,043 / 168,790 / 20 | 0 / 0 |
| WP4 | 10 | none | 1,251,351 / 164,377 / 19 | 0 / 2 |
| WP2 | 17 | none | 1,293,482 / 107,338 / 21 | 0 / 1 |
| WP3 as held | 18 | 28 (blue 20/0 → red 0/19), 31 (blue 17/3 → red 7/13) | 1,425,545 / 40,988 / 4 | 4 / 20 |
| Amendment 4 | 18 (19 against the base) | 28 and 31 back to blue; none against the base | 1,204,709 / 52,818 / 0 | 0 / 1 |

Against the base, Amendment 4 changes single-seed winners in battles 21 (blue 8 → 10 wins of 20), 31 (19/0 → 10/4,
6 unresolved), 32 (2/11 → 3/15), 33 (12/1 → 13/3) and 35 (13/0 → 16/0); no overall winner. Battle 28's zerglings
keep 23.6 survivors on average (28.0 on the base, 0.1 with WP3 as held). The one failure over all seeds is battle 33
seed 10 (0 – 4, a non-record seed; the matrix test reads the seeds of record).

**Battle 33**, 68 seeds (blue wins / red wins / unresolved; mean survivors):

| Step | Result |
|---|---|
| base | 41 / 10 / 17; 4.32 – 0.74 |
| WP4 | 34 / 12 / 22; 3.76 – 0.96 |
| WP2 | 52 / 9 / 7; 5.76 – 0.53 |
| WP3 as held | 54 / 8 / 6; 5.46 – 0.49 |
| Amendment 4 | 48 / 9 / 11; 5.34 – 0.57 |

**The flips of WP3 as held, traced** (`matchup_trace --battle`, seeds 11 and 22 of battle 28, 11 and 3203386110 of
battle 31, with a scratch build that logged each unit's walk state; not committed):

- **Battle 28, seed 11.** From tick 160 to 400, 25 to 47 zerglings stood "blocked" behind those biting (2 to 15 with
  WP2), and the zerglings in contact fell from 18 to 20 to 11 to 14. Of the zerglings' walk calls, about 3,000 found
  their way ahead blocked but started no walk: the squadmate in front waited under its order and contact nudged it
  by 0.0003 to 0.003 a tick, so it was not "exactly still". Walks that did start stopped on each free tick and chose
  a side afresh on restart: 2,419 starts in seed 22, 832 of them on the other side. Spent walks stood for good.
  The zerglings lost 19 of 20 seeds.
- **Battle 31, seed 11.** The stalkers' second and third ranks, attack-moving without a target, walked north along
  their own firing line from y 45 to y 60 (ticks 60 to 220): the walk's bound was one circle round the far
  attack-move point. The fight turned 90° and the survivors passed each other out of scan.
- **The showcase** flip came with WP2 (player 0's attack-movers waited instead of reaching range); with Amendment 4
  all 20 attackers are in range at tick 900 and player 0 wins, as on the base.

**What changed** (Amendment 4; each part measured alone on the matrix, the showcase and the reproductions): a
standing body is one that moved by at most 1/64 a tick; the side is chosen once per target; a walk ends only on a
free straight way in, and a paused walk steps straight in; a walk without a target ends after the scan range; a
spent walk retries after 24 ticks. Intermediate measurements: dropping only the standing test left battle 28 at 8.7
zergling survivors with record failures; keeping the side without the straight step made pulse bot 9 of the showcase
reverse 413 times (a walk step across, then steering's step back); requiring a firing (anchored) friend before an
attack-move walks flipped battle 31 to the stalkers and left pulse bot 4 reversing 109 times; dropping the retry left
3 give-ups in the corridor reproduction. Those variants were not kept.

**Reproductions** (`--include-ignored`, still ignored with these figures): showcase 10 / 10 in range, 7 / 4
reversals, 0 give-ups; synthetic arc all in range by tick 98, 3 reversals, 0 give-ups; no room 3 of 6 in range, 5
reversals, 0 give-ups, rear units still on 73 to 85 % of their ticks after their first stop.

**Goldens** (debug; release and `wasm32-wasip1` in the hand-back): WP4 diverges where it did on the old base
(`golden_replay` checkpoint 1, tick 6; `golden_replay_orders` checkpoint 3, tick 21), WP2 likewise (checkpoints 2 and
6). The latent schema bump (24 → 25) changes every checkpoint and the sandbox checksum. WP3 moves `golden_replay`
from checkpoint 1. Amendment 4 moves `golden_replay` from checkpoint 1 (the paused walk's straight step; switched on
alone, the standing test first moves checkpoint 2, the kept side and the straight way in checkpoint 4, the scan bound
checkpoint 5) and `golden_replay_orders` from checkpoint 19 (the standing test).

## Amendment 5: the builder regression of WP2 (rule)

**What.** `player_view_tests::a_build_on_a_hidden_structure_is_accepted_and_abandoned_on_arrival` failed from WP2 on:
the builder started wedged between its own town hall and an idle worker, waited (D-1.3) and spent its task legs
(`Unreachable`). Gathering workers were not the blocker: a worker under `Harvest` or `ReturnCargo` already
mineral-walks (`is_mineral_walking` leaves it out of every other unit's obstacles). The blocker was an idle worker
that contact nudged by a few thousandths a tick: `SteeringObstacle::yields_to` required an idle friend to stand
exactly still, so it did not step aside and, standing within Amendment 4's drift, formed a wall with the hall.
`yields_to` now uses `STANDING_DRIFT` (1/64 a tick) for "standing", the test walls and the walk already use, which
completes D-4's row "idle, standing: pushed sideways".

**Matrix** (against Amendment 4): 1 battle moved, 35_12_vikings_vs_6_colossi (9 seeds; survivors 12.00 – 0.20 →
12.00 – 0.15, mean end tick 545.5 → 516.1); no overall winner changed, and none against the base. Blocking totals:
1,206,580 blocked attacker ticks, 53,025 reversals, 0 give-ups.

**Battle 33**, 68 seeds: identical to Amendment 4 (48 / 9 / 11; 5.34 – 0.57).

**Showcase:** 10 / 10 in range at tick 900, 11 / 8 reversals, 0 / 0 give-ups, ramp E turn 211°, player 0 wins at
tick 1,100.

**Goldens.** Re-recorded: `golden_replay` from checkpoint 11 and `golden_replay_orders` from checkpoint 19, both
the nudged idle friend now yielding; the sandbox checksum is unchanged. Identical in debug, release and on
`wasm32-wasip1`.

## WP5: push priorities (data, B-48)

**What.** Every unit's `push_priority` in `assets/catalog/units.ron` is StarCraft II's `PushPriority` 10 (the
`CUnit` default, `core` UnitData:59; no modelled unit overrides it, only `Critter` and `SlaynElemental` do).
`stag_behemoth`, the Ultralisk analogue, gets 11: `PushPriority` 10 plus the 5.0.14 `AlliedPushPriority` 1
(`voidmulti` UnitData:1943; 5.0.14 patch notes: "now pushes allied units out of the way when moving"), Q0073's
default while the question is open. Each value carries its source in a comment, and
`catalog_reference_values_tests::b48_push_priorities_follow_starcraft` pins every mobile analogue (the default, and
a table of exceptions holding the Ultralisk).

**Matrix** (against Amendment 5): **bit-identical**, all 1,000 rows. Priority is read only when a mover meets an
idle, standing friend (D-4); in the matrix no such meeting joins two archetypes whose old priorities differed in
the deciding direction. Blocking totals unchanged: 1,206,580 / 53,025 / 0.

**Battle 33**, 68 seeds: identical (48 / 9 / 11; 5.34 – 0.57).

**Showcase:** identical: 10 / 10 in range at tick 900, 11 / 8 reversals, 0 / 0 give-ups, ramp E turn 211°, player 0
wins at tick 1,100. (`route_trace` reads the catalog at run time; `matchup_trace` embeds it, and its binary was
checked to carry the new values.)

**Goldens.** Both re-recorded; the first differing tick of each was traced by replaying it with the old priorities
patched into the catalog, side by side with the new:
- `golden_replay` from checkpoint 1 (ticks 51 to 100): on tick 63, psi grey 21 (priority 25 → 10) walks into the
  idle railgun crawler 35 (40 → 10), which now steps aside for it.
- `golden_replay_orders` from checkpoint 20 (ticks 201 to 210): on tick 206, pulse bots 12 and 21 (20 → 10) meet
  rhino beetle 24 (30 → 10), idle on its arrival, which now steps aside.

The sandbox checksum is unchanged. Identical in debug, release and on `wasm32-wasip1`.

## WP6: burrowed bodies (rule, B-93)

**What.** `UnitModeSpec` gains `burrowed: bool` (default `false`), set on the Lurker analogue's `burrowed` mode
(`LurkerMPBurrowed` collides only with `Burrow`, `void` UnitData:775). `Entity::is_burrowed` reads it from the
active mode, or from a burrowing cloak toggle (`CloakToggle { is_burrow: true }`) that has hidden the unit; a unit
still channelling its burrow is not yet burrowed, as its other mode stats are not. Movement gives each body a class
(`MotionPolicy::body_class`: ground, air, burrowed), and steering, contact and displacement admission only pair
bodies of one class; terrain still follows the layer. So ground units walk over a burrowed body, it neither pushes
nor is pushed by them, and two burrowed bodies still keep apart. The plan said the rule "reads the existing burrowed
state"; there was none (a burrowed mode was only cloaked, immobile and unpushable, as a sieged tank is anchored), so
the mode now says it (Q0002). The plan's source `liberty` UnitData:775 is `void` UnitData:775, as in
`burrow_attack_tests`.

**Tests first** (`burrow_attack_tests.rs`): a ground unit, friendly or hostile, walks straight over a burrowed body
(sideways offset below 0.01); the same stance not marked burrowed is still walked round (control); a burrowed body
and the ground units standing half over it are never pushed apart; a burrowed mover still walks round a burrowed
anchor without overlapping it; the burrowing cloak toggle burrows, and a mode burrows only once entered. With
`body_class` ignoring `burrowed`, the first and third fail.

**Matrix** (against WP5): **1 battle moved**, 21_lurker_vs_6_marines, the only battle that burrows (17 of 20 seeds):
blue (the Lurker) wins 10 → 3, unresolved 10 → 17, red wins 0 → 0; marine survivors 0.65 → 1.25 (0 on 3 seeds, 1 on
9, 2 on 8), mean end tick 417.5 → 474.2; expectation (red 0 to 2 survivors) met on every seed. **No overall winner
flips**: the marines never win. The marines, unable to see the cloaked Lurker, walk to their attack-move point on
top of it and now stand over it instead of round it, so its volleys meet a different set of them; as before, the
survivors are marines that fled beyond its range of 8 when struck (the damage effects' `Flee` response) and stood
there (seed 22: two marines idle 11.5 from the Lurker from tick 300). The battle's comment in `matchup_matrix.ron`
is updated. Blocking totals: 1,206,580 blocked attacker ticks, 52,889 reversals, 0 give-ups.

**Battle 33**, 68 seeds: identical.

**Showcase:** identical (no unit burrows).

**Goldens.** Unchanged (no golden unit burrows), in debug, release and on `wasm32-wasip1`; the sandbox checksum is
unchanged. No checksum schema change: `burrowed` is read from the hashed `active_mode` through the catalog.

## Showcase storyboard (WP7)

**Render.** `showcase render --driver radv` of `00dc9ba` (release build, one `with-gpu-lock` per take), output under
`~/scratch/blocking/r2/show1/` (not committed). The take completed: 1,100 frames.

| Scene | Frames | Start tick |
|---|---|---|
| title | 125 | 0 |
| terrain | 150 | 2 |
| units | 150 | 301 |
| smog | 125 | 451 |
| advance | 50 (its minimum) | 579 |
| clash | 300 | 776 |
| closing (paused) | 200 | 1,075 |

Stills (frames 598, 700, 899, 1,000): at the end of `advance` the viewer's ten attackers already fire on the enemy
hall; `clash` shows them round that hall until tick 1,075, the hall still standing; `closing` shows the status card
with no result window. `route_trace` ends the battle at tick 1,100 with player 0 winning, 25 ticks after `closing`
pauses.

**Changed.** Only the storyboard's header comment in `assets/captures/showcase.ron`: it said the viewer's base falls
near tick 1,225; it now records this render's scene ticks and the win near tick 1,100. No scene length, camera or
caption changed: the story still fits, and the captions are generic. The margin before the result is 25 ticks; a
later change that ends the battle earlier needs `clash` shortened.

**Tests.** `showcase_tests` (the GPU smoke take through the harness on `radv`) and `showcase_storyboard_tests` pass.

## Amendment 7: a walk ends only at a full free step (2026-11-10)

Base: main `d914917` (simulation identical to `02f494b`). Tools: `route_trace` and `matchup_trace` release builds,
matrix with 16 extra seeds and battle 33 with 64 extra seeds under the gate lock; output under
`~/scratch/path-c/mb/` (not committed).

**Trace of the cause** (held mesh switch, throw-away probe printing the walk state on each reversal): player 0's
pulse bot 4 at (22.9 to 23.0, 34.5), heading for its route corner (25.575, 35.0) behind the firing line, alternates
every tick or two between `flank_active` with a clipped walk step of 0.05 and a straight step of 0.093 to 0.112
(its full step is 0.126): the half-step test ended the walk, the step forward met the line, the walk started again.

**Synthetic reproduction** (`walkers_at_the_crowds_edge_do_not_toggle`): 210 reversals (walkers 107, 69, 18, ...)
and 14 of 17 in range at tick 300 before; 31 (at most 7 per walker) and 17 of 17 after.

**Showcase on main:** every box unchanged (ramp W 1.064 / 1.112, 103° / 213°; plateau SE 1.044 / 1.093; ramp E
1.573 / 1.611, 211°); player 0 wins at tick 1,101 (1,100); reversals 10 and 8 (11 and 8); 10 of 10 in range per
side; 0 give-ups.

**On the held mesh switch:** player 0's reversals 253 to 56 and player 1's 24 to 2; ramp E turn 381° to 352°; 10 of
10 in range; player 1 still wins (the lane split, sc2 pathfinding Amendment 4).

**Matrix:** 18 battles move; reversals over all battles 52,889 to 28,879, blocked-attacker ticks 1,206,580 to
1,171,337, give-ups 0 and 0. One overall winner flips, battle 31 (blue wins 10 to 6, red 4 to 7, unresolved 6 to 7
of 20; its claim is that a small change of formation swings it). Battle 26 fails its expectation on 2 extra seeds
(0 of record). Battle 33 (68 seeds): blue 48 to 47, red 9 to 16, other 11 to 5; the overall winner stays blue.
