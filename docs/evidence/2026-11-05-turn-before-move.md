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

## 3. WP2: tests that would break for another reason

**Probe.** `assets/catalog/units.ron` with WP3's three flags (uncommitted), then `cargo test -p sim_core -p
sim_fixtures -p asset_baker --no-fail-fast` (log `~/scratch/logs/turn_before_move_wp2_probe.log`). The only failure
was `golden_replay_tests::golden_replay_reproduces_every_checkpoint`, which diverged at the third checkpoint (ticks
101 to 150) as plan D-7 expects; every other test, the matchup-matrix expectations included, passed. No test
spawns a shipped flagged identity and depends on instant translation for a reason that is not its subject, so WP2
makes no commit.

## 4. WP3: the three flags

**Change.** `turn_before_move: true` on `railgun_crawler` (Siege Tank, `liberty` UnitData:5084), `colossus_mech`
(Thor, :5217) and `stag_behemoth` (Ultralisk, :8033); hull rate 20.16° per tick, so a reversal is 8 ticks in place
and a step on the 9th. Outputs under `~/scratch/turn_before_move/wp3/`.

- **Whole matrix, 20 seeds:** against §2, exactly one battle moved, `27_100_lings_vs_8_ultralisks`, on all 20
  seeds: end tick 221.5 → 220.0, red lead life 421.19 → 416.07, red side life at the end 3709.6 → 3708.0; red
  (ultralisks) wins every seed before and after, 8 survivors; `red: (6, 999)` holds. Expectation failures stay 0 on
  the seeds of record and 2 over all seeds (battle 33's). No battle's winner changed on any seed.
- **Battle 33, 68 seeds: bit-identical** to §2 (no flagged identity).
- **Battle 3:** every output file identical: the tank never translates (it stands in range from the first tick).
- **Battle 14:** every output file identical: the thor and the ultralisk spawn facing each other and walk straight
  at each other, then fight in place; no step ever lies beyond one tick's turn.
- **Battle 15:** matrix row identical (end tick 116.0, ultralisk life 413.2, win on every seed); the trace moved:
  the ultralisk turns in place between targets, 4.60 `blocked` ticks per seed (0 before), longest run 2, and its
  mean fraction of ticks in contact falls from 0.786 to 0.747. Its end tick, survivors and life are unchanged on
  every seed.
- **Battle 27:** `blocked` ticks per ultralisk 0.51 → 32.08 (exactly-still `blocked` ticks, position unchanged from
  the tick before: 0.33 → 27.76); longest `blocked` run 4 → 14 (seed 15, unit 102). Pure turning in place lasts at
  most 8 ticks at this rate, so the longer runs combine a turn with crowding or a change of step direction while
  chasing zerglings (risk R-2); none reaches the stop rule's 25 ticks.
- **Golden replays.** `golden_replay.ron` first differs at checkpoint 3 (ticks 101 to 150); checkpoints 1 and 2 are
  unchanged and every later one changed. Traced tick by tick (positions, headings and velocities of the three
  `red_siege` tanks, ticks 115 to 150, with and without the flags): ordered at tick 120 to an attack-move about 104°
  from their spawn facing, the tanks stand with zero velocity on ticks 121 to 125, turning 3 670 units per tick
  (heading 0 → 61 866 → 58 196 → 54 526 → 50 856 → 47 186), take their first step on tick 126 facing it exactly,
  and from then walk the positions they walked before, five ticks later. Re-recorded on Linux; reproduced in debug,
  release and `wasm32-wasip1`. `golden_replay_orders.ron` and the sandbox checksum unchanged.

## 5. WP5: the stationary turn rate, equal to the moving rate everywhere (Amendment 1)

**Change.** `UnitArchetype::stationary_turn_rate` (authored `stationary_turn_rate_deg_per_tick`, optional, the
hull rate when omitted, validated `0 < rate <= 180`). Every tick's hull budget now starts at the stationary rate and
is set to the moving `turn_rate` when the unit's own motion translates it (`MotionProposal::commit`), so a hull
turns at the stationary rate on every tick its unit does not move (D-10). The `turn_before_move` gate still
compares against the moving rate, and its validation counts the turn in place at the stationary rate
(`ceil(max(0, 32 768 − turn_rate) ÷ stationary_turn_rate) + 1 ≤ 25`). No shipped archetype authors the field.
Outputs under `~/scratch/turn_before_move/wp5/`.

- **Whole matrix, 20 seeds: bit-identical** to §4 (all 1000 rows of `matrix.csv`).
- **Battle 33, 68 seeds, and battles 3, 14, 15, 27:** every output file identical to §4.
- **Goldens:** both pass unchanged; the sandbox checksum unchanged.

## 6. WP6: the StarCraft II stationary rates (Amendment 1)

**Change.** `stationary_turn_rate_deg_per_tick` on four identities, from `StationaryTurningRate` (Normal °/s × 1.4
÷ 25; audit §10.1, §10.2): `railgun_crawler` (Siege Tank), `colossus_mech` (Thor) and `stag_behemoth` (Ultralisk)
40.32 (720, `core` UnitData:39, the `CUnit` default, which none of the three overrides; 7 340 binary units against
their moving 3 670); `napalm_wheeler` (Hellion) 84.00 (1499.9414, `liberty` UnitData:11554; 15 292 against
7 340). A reversal of the three flagged units is now 4 ticks in place and a step on the 5th (was 8 and the 9th).
Outputs under `~/scratch/turn_before_move/wp6/`.

- **Whole matrix, 20 seeds:** against §5, exactly one battle moved, `27_100_lings_vs_8_ultralisks`, on all 20
  seeds: end tick 220.0 → 216.8, red lead life 416.07 → 421.72 (§2: 221.5 and 421.19), red side life at the end
  3708.0 → 3719.7; ultralisks win every seed with 8 survivors; `red: (6, 999)` holds. Expectation failures stay 0 on
  the seeds of record and 2 over all seeds (battle 33's). No battle's winner changed on any seed. The Hellion
  battles (9a, 9b, 10) did not move: their rows of `matrix.csv` are identical.
- **Battle 33, 68 seeds: bit-identical** to §5.
- **Battles 3 and 14:** every output file identical to §5.
- **Battle 15:** matrix row identical; turning in place falls to 3.05 `blocked` ticks per seed (4.60 in §4),
  longest run 1, and the ultralisk's fraction of ticks in contact rises from 0.747 to 0.760 (0.786 in §2).
- **Battle 27:** `blocked` ticks per ultralisk 32.08 → 27.14 (exactly still: 27.76 → 22.90); longest `blocked`
  run 14 → 9 (seed 1, unit 104). Pure turning in place now lasts at most 4 ticks, so the longer runs again combine
  a turn with crowding or a change of step direction; none reaches the stop rule's 25 ticks.
- **Golden replays.** `golden_replay.ron` first differs at checkpoint 3 (ticks 101 to 150); checkpoints 1 and 2 are
  unchanged and every later one changed. Traced as in §4: the `red_siege` tanks turn in place 7 340 units per tick
  on ticks 121 and 122 (heading 0 → 58 196 → 50 856) and reach their facing on tick 123 still in place (46 671,
  45 503 and 45 265 for the three tanks: about 4 185 units remained, more than the moving 3 670, so the step was
  refused and the stationary turn finished the facing; open deviation 2 below), step on tick 124, and from then walk the positions of §4 two ticks earlier. Re-recorded on Linux;
  reproduced in debug, release and `wasm32-wasip1`. `golden_replay_orders.ron` and the sandbox checksum unchanged.

## 7. Differences from StarCraft II that no source settles (Amendment 1 item 5)

Under the owner's standing rule (2026-11-05), any difference from the game not named as a deviation is a defect.
These could not be determined from a source within this plan's effort (the game data hold the flag and the two
rates; the behaviour around them is engine code, and the one forum description found, plan §2.6, says only that
the unit "rotates to face the direction it wants to move before it starts moving"). Each is recorded as an open
deviation with what this code does, for a measurement in the editor or a unit tester:

1. **The tolerance.** How far from its step a `TurnBeforeMove` unit may face and still move. Here: one moving
   tick's turn (`turn_rate`), the owner's answer to Q0050.
2. **Which rate the snapping tick uses.** Here the tick that translates turns at the moving rate (so the tolerance
   is the moving rate), and every tick in place at the stationary rate. The game may instead finish the turn at
   the stationary rate and start moving on the next tick.
3. **A short step that lands on a waypoint or the goal** (plan R-3). Here it is gated like any step: a flagged unit
   turns to face even a few hundredths of a unit of travel.
4. **Contact while turning in place.** Here a turning unit has zero velocity, so friendly units treat it as still
   (pushing and steering), and with an attack order and a living target it counts as anchored for that tick.
5. **Animation while turning in place.** Here `Walk` (presentation only).
6. **Pushes.** Here a collision push displaces a flagged unit whatever its facing and never turns its hull.
7. **"Not moving" for the stationary rate.** Here a tick on which the unit's own motion does not change its
   position; a tick on which it is only pushed counts as stationary.
