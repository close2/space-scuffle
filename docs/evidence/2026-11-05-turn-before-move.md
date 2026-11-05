# Turn before move: measurements per step

This file records the measurement of every step of plan
[`2026-11-05-turn-before-move`](../plans/2026-11-05-turn-before-move.md) (D-8, and Amendment 1 for the stationary
turn rate): the whole matchup matrix on 20 seeds compared with the step before, the four battles that hold a flagged
identity (3, 14, 15, 27) traced tick by tick on the same 20 seeds, battle 33 on 68 seeds, and the golden replays.
WP1 creates it with the baseline (section 1) and its own section; later packages append theirs in the commit that
makes the change.

## How to reproduce

The tool and its outputs are those of the combat alignment evidence,
[How to reproduce](2026-11-04-combat-sc2-alignment.md#how-to-reproduce): `crates/sim_core/examples/matchup_trace.rs`,
always a release build. Per step (the binary is copied once per step, so a later edit cannot change a running
measurement):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/turn-before-move
cargo build --release --locked -p sim_core --example matchup_trace
TOOL=$CARGO_TARGET_DIR/release/examples/matchup_trace
# Whole matrix: the four seeds of record plus 16 more, 20 seeds per battle; outcomes only.
$TOOL --all-battles --seeds 16 --jobs 8 --out ~/scratch/turn_before_move/<step>/matrix \
    [--compare ~/scratch/turn_before_move/<previous step>/matrix/matrix.csv]
# Battle 33: the four seeds of record plus 64 more, 68 seeds.
$TOOL --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/turn_before_move/<step>/b33
# The battles with a flagged identity, traced tick by tick on the matrix's 20 seeds.
for b in 3_immortal_vs_tank 14_thor_vs_ultralisk 15_plated_ultralisk_vs_8_marines 27_100_lings_vs_8_ultralisks; do
    $TOOL --battle $b --seeds 16 --out ~/scratch/turn_before_move/<step>/b${b%%_*}
done
```

`--seeds <n>` plays the fixture's seeds of record and then the first `n` positive integers that are not seeds of
record, so `--seeds 16` gives 20 seeds per battle and `--seeds 64` gives battle 33's 68.

- **Flagged sides.** Battle 3: red (the tank); 14: blue (the thor) and red (the ultralisk); 15: blue (the
  ultralisk); 27: red (eight ultralisks).
- **Turning in place, as the trace sees it** (plan D-8). The per-unit CSV has no heading. A flagged unit that turns
  in place while it has a target outside its band moves less than 10 % of its speed and is classed `blocked`
  (`crates/sim_fixtures/src/trace/activity.rs`). For each flagged side: the mean number of `blocked` ticks per
  flagged unit, and the longest run of consecutive `blocked` ticks of one flagged unit on any seed. The run is the
  proxy for the stop rule "a flagged unit stops translating for more than 25 consecutive ticks" (risk R-2); it also
  counts crowding, so it is an upper bound on turning in place.
- **Battle 33's figures** come from its `seeds.csv` (wins, survivors and their mean and SD, the seeds of record).
- The script that computes these (`summarize.py`) is a throwaway kept beside the outputs on the development server;
  it only counts and averages those columns.

## 1. Baseline

**Commit.** `main` at `a27ce2a` (life-regeneration delay merged), measured on this plan's branch at `f7b4096` (the
plan and its Amendment 1, documents only). Outputs under `~/scratch/turn_before_move/base/` on the development
server.

- **Whole matrix, 20 seeds:** 50 battles, 1000 runs; expectation failures 0 on the seeds of record and 2 over all
  seeds. `matrix.csv` is **byte-identical** to the last measurement of the life-regeneration delay plan (its WP3
  step, `~/scratch/life_regen/wp3/matrix/matrix.csv`), so the branch reproduces `main`.
- **Battle 33, 68 seeds:** byte-identical to the life-regeneration delay's last step. Marine wins 39, zealot wins
  20, other 9; marine survivors 3.87 (SD 3.43), zealot survivors 1.01 (SD 1.37). Seeds of record (marines –
  zealots): 3203386110: 0–3, 11: 2–0, 22: 1–2, 33: 9–0.
- **Battles with a flagged identity** (means over 20 seeds):

| Battle | Wins blue – red | End tick | Survivors | Side life at the end | Flagged side: `blocked` ticks per unit | Longest `blocked` run |
|---|---|---|---|---|---|---|
| 3 immortal vs tank | 20 – 0 | 91.0 | 1.00 – 0.00 | 176.0 – 0.0 | red 0.00 | 0 |
| 14 thor vs ultralisk | 20 – 0 | 200.0 | 1.00 – 0.00 | 37.0 – 0.0 | blue 0.00, red 0.00 | 0, 0 |
| 15 plated ultralisk vs 8 marines | 20 – 0 | 116.0 | 1.00 – 0.00 | 413.2 – 0.0 | blue 0.00 | 0 |
| 27 100 lings vs 8 ultralisks | 0 – 20 | 221.5 | 0.00 – 8.00 | 0.0 – 3709.6 | red 0.51 | 4 |

## 2. WP1: the flag, its validation and the step gate, `false` everywhere

**Change.** `UnitArchetype::turn_before_move` (catalog field, omitted reads `false`), its validation (a flagged
unit must move and face a reversal within `STALL_REPATH_TICKS`), and the gate in `movement/locomotion.rs`: for a
flagged unit, a step whose direction lies further from the tick-start heading than one tick's turn is voided and the
unit turns in place (`step_toward`, and the snap onto a goal within one step in `upkeep_route`, Amendment 1 item 4).
No shipped archetype sets the flag. Outputs under `~/scratch/turn_before_move/wp1/`.

- **Whole matrix, 20 seeds: bit-identical** to §1 (all 1000 rows of `matrix.csv`).
- **Battle 33, 68 seeds: bit-identical** to §1 (every output file).
- **Battles 3, 14, 15, 27:** every output file identical to §1.
- **Goldens:** `golden_replay.ron` and `golden_replay_orders.ron` pass unchanged; the sandbox checksum
  (`sim_fixtures`) unchanged.

As plan D-7 says: the gate and the short-step heading request are reached only through `Some(HullGate)`, which
exists only for flagged archetypes, and no state or checksum field was added.
