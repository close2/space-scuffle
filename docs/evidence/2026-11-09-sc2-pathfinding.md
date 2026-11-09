# StarCraft II-style pathfinding: measurements per step

This file records the measurements of plan [`2026-11-09-sc2-pathfinding`](../plans/2026-11-09-sc2-pathfinding.md)
(D-11, D-12): the showcase route metrics of §3.1, the whole matchup matrix and battle 33 compared with the step
before, and the golden replays. WP1 creates it with the baseline; each later package appends its own section in the
commit that makes the change.

## How to reproduce

Every tool is a release build of an example of `sim_core`. The binaries are copied once per step, so a later edit
cannot change a running measurement.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/path-a
cargo build --release --locked -p sim_core --example route_trace --example matchup_trace
# Showcase battle: units.csv (every mobile unit per tick), passes.csv (each unit's first pass through
# each box) and summary.md (per box and owner, mean and maximum walked/chord and total turn; outcome).
$CARGO_TARGET_DIR/release/examples/route_trace --scenario scenarios/showcase.ron --ticks 1500 \
    --out ~/scratch/path-a/route/<step>
# Whole matrix: the seeds of record plus 16 more; outcomes only.
$CARGO_TARGET_DIR/release/examples/matchup_trace --all-battles --seeds 16 --jobs 8 \
    --out ~/scratch/path-a/<step>/matrix --compare ~/scratch/path-a/<previous step>/matrix/matrix.csv
# Battle 33: the four seeds of record plus 64 more, 68 seeds; seeds.csv is compared with diff.
$CARGO_TARGET_DIR/release/examples/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 \
    --no-unit-csv --out ~/scratch/path-a/<step>/b33
```

`route_trace` (`crates/sim_core/examples/route_trace.rs`, metrics in `crates/sim_fixtures/src/trace/route.rs`)
starts the scenario with `sim_core::scenario::load_scenario_battle`, which applies the scenario's initial orders
through `apply_initial_orders`, the function the client's `BattleDefinition::instantiate` now calls. It steps the
world with empty bundles until the match ends or the tick limit. Its default boxes are those of plan §3.1, in world
units, bounds inclusive: `ramp_w_foot` x 26–34, y 7–14; `plateau_se` x 35–41, y 21–29; `ramp_e_foot` x 31–39,
y 49–57. The heading is sampled every 3 ticks.

The stop rule of D-12 is applied to player 0's means, the rows of plan §3.6.1. Player 1's rows are recorded too.

## WP1: baseline (`68d6fc1`)

The harness reproduces the planning probe of §3.2, §3.3 and §3.6.1 to the rounding of the last digit.

| Box | Owner | Units | Ratio mean / max | Turn mean / max |
|---|---:|---:|---|---|
| ramp_w_foot | 0 | 10 | 1.196 / 1.383 | 324° / 840° |
| ramp_w_foot | 1 | 10 | 1.369 / 1.380 | 91° / 99° |
| plateau_se | 0 | 10 | 1.147 / 1.500 | 250° / 1070° |
| ramp_e_foot | 0 | 10 | 1.551 / 1.591 | 114° / 196° |
| ramp_e_foot | 1 | 10 | 1.283 / 1.473 | 235° / 446° |

Outcome: player 0 wins at tick 1,369, when player 1's base falls (plan D-11 gave about 1,225 from the video; the
trace measures the tick the match ends).

Per unit, the rows of §3.2 and §3.3 (`passes.csv`):

| Box | Unit | Walked | Chord | Ratio | Total turn | First-waypoint variants |
|---|---|---:|---:|---:|---:|---:|
| ramp_w_foot | 5, `pulse_bot` | 9.49 | 6.86 | 1.383 | 358° | 4 |
| ramp_w_foot | 8, `pulse_bot` | 9.96 | 7.52 | 1.323 | 840° | 4 |
| ramp_w_foot | 4, `pulse_bot` | 7.78 | 6.13 | 1.270 | 323° | 4 |
| ramp_w_foot | 13, `rhino_beetle` | 5.54 | 4.98 | 1.113 | 52° | 2 |
| plateau_se | 7, `pulse_bot` | 12.70 | 8.47 | 1.500 | 1,070° | 7 |
| plateau_se | 9, `pulse_bot` | 10.61 | 8.50 | 1.248 | 340° | 4 |
| plateau_se | 5, `pulse_bot` | 10.29 | 8.61 | 1.195 | 239° | 4 |
| plateau_se | 12, `rhino_beetle` | 8.69 | 8.40 | 1.035 | 31° | 2 |
| ramp_e_foot | 23, `pulse_bot` (player 1) | 11.06 | 7.51 | 1.473 | 282° | 5 |
| ramp_e_foot | 26, `rhino_beetle` (player 1) | 11.03 | 8.96 | 1.231 | 435° | 4 |

Matrix (1,000 runs: 50 battles on 20 seeds): 0 expectation failures on the seeds of record and over all seeds. This
run is the comparison base of WP2. Battle 33 on 68 seeds: `seeds.csv` kept as the comparison base.

WP1 changes no behaviour: `apply_initial_orders` is the code the client ran, moved, and the golden replays do not
start from a scenario file.

## WP2+WP3: route following leaves bodies to steering (B-99, B-100)

One commit (plan R-1). Showcase, `route_trace --ticks 1500`:

| Box | Owner | Units | Ratio mean / max | Turn mean / max |
|---|---:|---:|---|---|
| ramp_w_foot | 0 | 10 | 1.063 / 1.113 | 92° / 146° |
| ramp_w_foot | 1 | 10 | 1.399 / 1.416 | 156° / 207° |
| plateau_se | 0 | 10 | 1.045 / 1.078 | 67° / 148° |
| ramp_e_foot | 0 | 10 | 1.557 / 1.635 | 213° / 415° |
| ramp_e_foot | 1 | 10 | 1.176 / 1.216 | 107° / 222° |

Outcome: player 0 wins at tick 1,351. A1 and A2 alone give the same table: no body is parked on a showcase corner,
so A3 does not act there. These are §3.6.1's "A1 and A2 together" numbers.

The ramp E turn rose from 114° to 213°: the column reaches ramp E about 20 ticks sooner and flockmates jostle each
unit sideways by about ±15° per 3-tick sample while it heads for the corner (34.70, 52.30). Under D-12 the package
was held; the orchestrator accepted it and moved the criterion to plan `2026-11-09-friendly-blocking` (Amendment 1
of both plans).

Experiments not kept:
- A3 with D-9's literal occupant rule (any non-yielding body, moving flockmates included): ramp W 1.185 / 1.371,
  350° / 720°; plateau 1.129 / 1.236, 309° / 583°; ramp E 1.562 / 1.593, 179° / 258°; **player 1 wins** at tick
  1,464.
- Handoff only beyond the corner (no "near" alternative): ramp E 1.570 / 1.593, 189° / 230°.

Matrix (1,000 runs) and battle 33 (68 seeds): bit-identical to WP1. Goldens: checksum schema 21, re-recorded; first
behaviour change on tick 155 of `golden_replay` and tick 120 of `golden_replay_orders` (fixture headers).
