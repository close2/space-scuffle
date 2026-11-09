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
