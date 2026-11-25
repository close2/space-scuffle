# Evidence: Collision Classes as Data (B-209, B-212)

Measurements of plan [`2026-11-25-collision-classes`](../plans/2026-11-25-collision-classes.md). Branch
`collision-classes-plan`, whose head `0d3c6d0b` merges `main` `f390c4a2` (B-208, the solver pair cache, landed):
stop rule 1 is met. Scratch material (tree copies, binaries, worlds, probes, scripts, red runs) lives under
`~/scratch/collision-classes/` and is never committed.

## 1. WP0: base, close-battle list and census

### 1.1 The base

**WP0's tree** is `0d3c6d0b` (checksum schema 47, the contact solver the default pipeline). Its measurement copy is a
`git archive` of that commit; a second copy carries the scratch probes of plan §10.1 (a) and (c), which only print
lines to standard error and change no column of `matrix.csv`. Each arm builds in its own target directory
(`/home/spacescuffle/cargo-target/collision-classes-<arm>`); the scripts are under
`~/scratch/collision-classes/scripts/`:

```bash
TD=/home/spacescuffle/cargo-target/collision-classes-wp0 scripts/build.sh wp0 trees/wp0 trees/wp0-probe
scripts/assets.sh wp0 trees/wp0           # assets-wp0-solver, assets-wp0-legacy: the pipeline set in movement_rules.ron
scripts/measure.sh wp0 wp0                # §10.2's worlds, one run at a time under ~/scratch/gate.lock
python3 -I scripts/close_list.py trees/wp0/crates/sim_core/tests/fixtures/matchup_matrix.ron \
    worlds/wp0/matrix-solver/matrix.csv
```

`measure.sh` plays the matrix on 24 seeds (the four of record and the tool's first 20 others) under both pipelines,
battle 31 on 100 seeds and battle 33 on 64, the showcases (`showcase.ron`, `showcase_effects.ron`; default seed and
1 to 8, 1,500 ticks, `route_trace`), and the harness families (gap, arc, cross, corridor, corner, mirror; development
seeds 1 to 1,000, original and relabelled). The base matrix under the solver: 1,464 runs, expectation failures 2 on
the seeds of record (battle 28, B-192) and 32 over all seeds, as the K-1 arm of evidence
`2026-11-25-solver-pair-cache` §4.

### 1.2 The close-battle list (plan §7.3), fixed before WP1's first build

Derived from WP0's 24-seed base under the solver and nothing else:

1. **Minority side wins at least 3 of 24:** 14 (6 / 18), 26 (20 / 4), 28 (6 / 18), 31 (20 / 4).
2. **Named close or restated:** the plan's list from the fixture (10, 27, 33, 35, 36a, 36b, 38b, 39a, and 26), and
   the enemy-contact base's list (11a, 14, 21, 22a, 26, 28, 31, 32, 33, 35, 37a, 37b, 38b, 39b, 42, 38a). The
   script's grep of `close|[Rr]estated` also hits the claims of 8 ("a zealot closes") and 9b ("as the zerglings
   close around them"), where the word means approach; neither names the battle close, so neither is listed.
3. No other.

**Class R** (21 battles; blue / red wins of 24 on the base):

| Battle | Wins | Why |
|---|---|---|
| 10_2_hellions_vs_8_lings | 0 / 24 | plan §7.3 |
| 11a_2_banelings_vs_5_marines | 0 / 10 | enemy-contact list |
| 14_thor_vs_ultralisk | 6 / 18 | minority 6; enemy-contact list |
| 21_lurker_vs_6_marines | 11 / 0 | enemy-contact list |
| 22a_reaper_vs_2_workers | 0 / 0 | enemy-contact list |
| 26_100_lings_vs_6_colossi | 20 / 4 | minority 4; fixture (restated); plan §7.3; enemy-contact list |
| 27_100_lings_vs_8_ultralisks | 0 / 24 | plan §7.3 |
| 28_100_lings_vs_10_immortals | 6 / 18 | minority 6; enemy-contact list (`known_bug: Some("B-192")`; stop rule 5) |
| 31_15_marauders_vs_15_stalkers | 20 / 4 | minority 4; fixture ("close"); enemy-contact list |
| 32_30_marines_vs_15_banelings | 17 / 2 | enemy-contact list |
| 33_24_marines_vs_12_zealots | 24 / 0 | fixture; plan §7.3; enemy-contact list |
| 35_12_vikings_vs_6_colossi | 2 / 0 | fixture; plan §7.3; enemy-contact list |
| 36a_storm_turns_marines | 24 / 0 | plan §7.3 |
| 36b_marines_beat_zealots_without_storm | 0 / 24 | fixture; plan §7.3 |
| 37a_abduct_sieged_tank | 0 / 0 | enemy-contact list |
| 37b_sieged_tank_survives_without_abduct | 0 / 0 | enemy-contact list |
| 38a_fungal_turns_zerglings | 24 / 0 | enemy-contact list |
| 38b_marines_beat_zerglings_without_fungal | 0 / 7 | fixture; plan §7.3; enemy-contact list |
| 39a_cloud_on_tanks | 24 / 0 | fixture; plan §7.3 |
| 39b_tanks_beat_roaches_without_cloud | 0 / 0 | enemy-contact list |
| 42_sieged_tank_vs_fleeing_workers | 0 / 0 | enemy-contact list |

**Class D** is every other battle of the matrix (40). The list is not edited after WP1 starts. The colossus battles
outside it are **12a** (24 / 0), **12b** (0 / 24) and **12c** (24 / 0): an outcome move there is stop rule 4.

### 1.3 Census (plan §10.1 (c))

| World | Colossus analogue | Burrowed body | Producer |
|---|---|---|---|
| Matrix | 12a, 12b, 12c, 26, 35 (fixture) | 21 only (probe: the most burrowed units at once is above 0 in battle 21 alone, every seed) | none (no structure in the fixture) |
| Battles 31, 33 | no | no | no |
| Scenarios `showcase`, `showcase_effects`, `default_battle`, `skirmish` | no | no (no Lurker analogue) | structures of the start loadouts |
| `golden_replay.ron` | no | no | one structure (`foundry_core_dome`) |
| `golden_replay_orders.ron` | no | no | no |
| Harness families | no (scratch archetypes and the mirror's shipped picks by property; byte identity is checked) | — | no |

So WP2 (exits by class) can move no measured world: no producer stands beside a burrowed body or an authored class
(stop rule 3). WP3 can move only 12a, 12b, 12c, 26 and 35.
