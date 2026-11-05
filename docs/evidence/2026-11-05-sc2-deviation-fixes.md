# Evidence: StarCraft II Deviation Fixes

Measurements and research for plan [`2026-11-05-sc2-deviation-fixes`](../plans/2026-11-05-sc2-deviation-fixes.md),
one section per commit that can reach a unit a matrix battle spawns (plan D-3). The method is combat alignment D-5
([evidence](2026-11-04-combat-sc2-alignment.md) §14): the whole matrix
(`crates/sim_core/tests/fixtures/matchup_matrix.ron`, 50 battles) on 20 seeds each (the 4 of record plus 16
jittered), and battle 33 on 68 seeds (the 4 of record plus 64), both from release builds of
`crates/sim_core/examples/matchup_trace.rs`:

```bash
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --all-battles --seeds 16 --jobs 8 --out ~/scratch/sc2dev_wp1/<step>/matrix \
    --compare ~/scratch/sc2dev_wp1/<prev>/matrix/matrix.csv
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/sc2dev_wp1/<step>/b33
```

StarCraft II data are SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), layer chain
`core → liberty → swarm → void → voidmulti`, cited as `layer` File:line, as in the catalog time audit
([`2026-11-04-catalog-time-audit.md`](2026-11-04-catalog-time-audit.md) §2.2).

In the tables, "Wins blue/red" counts the seeds each side won over the 20; survivors and end tick are means over the
20 seeds; the lead hull is the first unit's life, averaged over the seeds it lived on (that count in brackets).

## 0. Baseline

The state after WP0 (`1fc72b8`): the turn-before-move package and combat alignment WP0 to WP12 and WP15 to WP17
merged. Whole matrix: 1000 runs, expectation failures 0 on the seeds of record and 2 over all seeds (both battle
33's). Battle 33 on 68 seeds: 39 / 20 / 9 (marine wins / zealot wins / unresolved), marine survivors 3.87 (SD 3.43),
zealot survivors 1.01 (SD 1.37); seeds of record 0-3, 2-0, 1-2, 9-0 (marines – zealots). These equal combat alignment
evidence §14.7: turn before move moved no matrix battle that this measurement sees.

## 1. WP1: a turn that aligns the weapon starts the swing on that tick (B-1, M-22)

- **Rule.** `combat.rs`: a `Ready` weapon that is not aligned enters `RotatingToTarget`, turns, and now checks its
  arc again in the same call, as a weapon that was already rotating does. Both go through one helper,
  `slew_and_try_swing`. Before, it turned and waited for the next tick even when this tick's turn had aligned it
  (remediation plan §4.2 M-22; combat alignment evidence §14.4 item 1).
- **StarCraft II.** A weapon may fire once its target is within `Arc` ÷ 2 + `ArcSlop` of its facing (`CWeapon`
  `Arc`, `ArcSlop`; audit §10.3). The turn and the attack are one motion: nothing in `CWeapon` adds a game loop
  between the facing that satisfies the arc and the start of the attack, so the attack starts on the loop the
  facing first satisfies it.
- **Tests first** (`crates/sim_core/tests/hull_turning_tests.rs`, scratch archetypes). On the old rule:
  `a_turn_that_aligns_the_weapon_starts_the_swing_on_that_tick` failed (swing on tick 2, expected 1),
  `a_turret_turn_that_aligns_the_weapon_starts_the_swing_on_that_tick` failed (2, expected 1), and the guard
  `a_turn_that_does_not_align_waits` passed (tick 3). All three pass with the rule.
- **Not latent** (plan WP1). Whole matrix against §0: 19 battles moved; no battle's winner over its seeds changed.
  Expectation failures 0 on the seeds of record, 1 over all seeds (battle 33's).

| Battle | Seeds moved | Wins blue/red | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|---|
| 6b_2_roaches_vs_hydra | 4 | 20/0 | 2.00 – 0.00, same | 80.2 → 80.0 | 112.58 → 113.12 / — |
| 9a_hellion_vs_4_lings | 7 | 20/0 | 1.00 – 0.00, same | 95.0, same | 25.40 → 23.80 / — |
| 10_2_hellions_vs_8_lings | 1 | 20/0 | 2.00 – 0.00, same | 61.2, same | 70.40 → 70.20 / — |
| 11a_2_banelings_vs_5_marines | 3 | 0/9 | 0.00 – 2.25, same | 8.2 → 8.1 | — / 10.00, same |
| 11b_2_banelings_vs_marauder | 9 | 0/20 | 0.00 – 1.00, same | 8.4 → 8.0 | — / 95.00, same |
| 12a_colossus_vs_6_marines | 1 | 20/0 | 1.00 – 0.00, same | 81.7, same | 238.15 → 237.90 / — |
| 21_lurker_vs_6_marines | 1 | 20/0 | 1.00 – 0.00, same | 103.6, same | 72.81 → 72.56 / — |
| 23a_zealot_vs_6_lings | 20 | 0/20 | 0.00 – 2.00, same | 275.0 → 271.3 | — |
| 24a_roach_vs_5_lings | 20 | 0/20 | 0.00 – 3.00, same | 289.8 → 288.5 | — |
| 24b_upgraded_roach_vs_5_lings | 6 | 20/0 | 1.00 – 0.00, same | 328.0, same | 26.62 → 25.42 / — |
| 27_100_lings_vs_8_ultralisks | 18 | 0/20 | 0.00 – 8.00, same | 216.8 → 217.6 | — / 421.72 → 418.14 |
| 28_100_lings_vs_10_immortals | 20 | 20/0 | 26.15 → 27.20 – 0.00 | 688.4 → 678.0 | 35.00 (1) → 35.00 (4) / — |
| 29_100_lings_vs_12_stalkers | 20 | 20/0 | 68.50 → 68.70 – 0.00 | 451.9 → 448.6 | 34.27 (15) → 35.00 (19) / — |
| 30_20_roaches_vs_10_immortals | 2 | 0/20 | 0.00 – 10.00, same | 239.7 → 239.1 | — / 146.95 → 149.95 |
| 31_15_marauders_vs_15_stalkers | 17 | 11/9, same | 1.85 – 1.45 → 1.80 – 1.70 | 669.4 → 672.4 | 57.00 (4) → 80.80 (5) / — |
| 32_30_marines_vs_15_banelings | 3 | 5/10 → 4/11 | 1.45 – 0.90 → 1.95 – 0.95 | 50.0 → 49.4 | 27.50 (4) → 36.25 (4) / — |
| 33_24_marines_vs_12_zealots | 17 | 11/6 → 10/6 | 3.55 – 1.20 → 3.55 – 1.05 | 442.2 → 447.7 | 38.60 (5) → 34.33 (6) / — |
| 34_20_hydras_vs_12_mutas | 8 | 20/0 | 19.30 → 19.25 – 0.00 | 184.4 → 184.6 | 90.00, same / — |
| 35_12_vikings_vs_6_colossi | 1 | 20/0 | 12.00 – 0.00, same | 280.4, same | 125.00, same / — |

  - The melee and zergling fights get back what the arcs had cost them (evidence §14.4): 23a and 24a end 3.7 and
    1.3 ticks sooner, back to their values before the arcs; 9a's and 24b's lead hulls return to 23.80 and 25.42.
    In 28 and 29 the zerglings strike sooner after each turn: 28 ends 10 ticks sooner with one more zergling alive.
  - **Single-seed flips** (none on a seed of record outside battle 33, no battle's winner over its seeds): battle
    31 seeds 9, 10, 14 and 15 (a close fight with both winners, 11/9 before and after); battle 32 seed 14 (1-0 →
    0-1; banelings win overall, 10 → 11 of 20); battle 33 seeds 3, 13 and 17 and the seeds of record below.
- **Battle 33, 68 seeds:** 37 / 20 / 11 (was 39 / 20 / 9), marine survivors 3.62 (SD 3.16; was 3.87, SD 3.43),
  zealot survivors 1.00 (SD 1.32), mean end tick 443.1 (was 438.9). Seeds of record 0-3 (same), 0-2 (was 2-0), 0-2
  (was 1-2, unresolved), 9-0 (same). The expectation `blue: (0, 9), red: (0, 3)` still holds on all four and is the
  smallest that does, so it is unchanged; the comment's distribution is rewritten (combat alignment D-6).
- **Golden replays.** `golden_replay.ron` re-recorded: checkpoints 1 to 3 unchanged; from checkpoint 4 (ticks 151
  to 200, the first fight) every checkpoint changed. `golden_replay_orders.ron` unchanged. Debug, release and
  `wasm32-wasip1` reproduce both.
