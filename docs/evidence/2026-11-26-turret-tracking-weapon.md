# Evidence: The Turret That Tracks Under Any Order (B-198)

Measurements of plan [`2026-11-26-turret-tracking-weapon`](../plans/2026-11-26-turret-tracking-weapon.md): the
survey of §2.2, the baseline, the census, the close-battle list and the probes of WP0 (§6.4, §7.1), the comparisons
of WP1 and WP2 against their predecessors (§6.3, §7.3), the held-out run, throughput and the golden proof (§5, §7.4).
Scratch material (trees, binaries, worlds, the probe patch and its summariser, the red runs) is under
`~/scratch/turret-tracking/`, never committed.

## WP0: survey, baseline, census, close battles and probes

### The survey of §2.2

Game data `5444e12f13` (5.0.14.94342) under `~/scratch/combat_wp3_data/<layer>/`:

```bash
cd ~/scratch/combat_wp3_data
grep -n 'CWeapon[A-Za-z]* id="[^"]*\(Fake\|Dummy\)' */WeaponData.xml
grep -rn "90mmCannonsFake" --include=*.xml .
```

The first command lists 17 weapons in eight layers: `90mmCannonsFake` (`voidmulti`, `balancemulti` :7), the display
dummies (`DisruptionBeamDisplayDummy`, `OracleDisplayDummy`, `VoidRaySwarmDisplayDummy`, `VoidRaySwarmDummy`), the
`liberty` and `swarm` dummies with real effects (`VolatileBurstDummy`, `InterceptorsDummy`, `WidowMineDummy`,
`TalonsDummy`), the Cyclone's `CycloneLockOnDummy` and `CycloneFakeWeapon` (`void`) and `PurifierBeamDummyTargetFire`
(`voidmulti` :375), as §2.2 classes them. The second finds the fake on `Turret="SiegeTank"` (`balancemulti`
UnitData:1473, `voidmulti` :1575) and in the two morph behaviours' `WeaponDisableArray` (`balancemulti`
BehaviorData:411, :419; `voidmulti` :470, :478). The tank's fake is the only tracking-only weapon on a 5.0.14
multiplayer unit, as the plan states.

### The baseline (§7.2)

WP0's tree is the branch head `37ba5ed8` (`main` `91350af1`, schema 48; the two commits above `main` are documents).
Its trace binaries were built with `cargo build --release --locked -p sim_core --example matchup_trace --example
route_trace` and the worlds measured from a `git archive` copy of the tree with the hold-facing plan's `measure.sh`
(the matrix on the seeds of record and 16 development seeds, battle 31 on 100 seeds, battle 33 on 64, the showcase
and the showcase effects scenario on the default seed and seeds 1 to 8, 6,000 ticks each):

```bash
~/scratch/turret-tracking/scripts/measure.sh ~/scratch/turret-tracking/bin-wp0 ~/scratch/turret-tracking/tree-wp0 \
    ~/scratch/turret-tracking/world-wp0
```

The matrix plays 1,220 runs: 2 expectation misses on the seeds of record (battle 28, `known_bug` B-192, seeds 11
and 22) and 26 over all seeds (28: 14; 39a: 4 (seeds 4, 5, 9, 14); 36a: 3 (4, 12, 14); 41: 2 (4, 7); 26: 1 (13);
32: 1 (13); 33: 1 (10)). The throughput binary of this tree (`cargo bench --locked -p sim_core --bench throughput
--no-run`) is kept as `~/scratch/turret-tracking/bench-bin-wp0/throughput` for the interleaved comparison of WP3.

### Census (§7.1 (c))

The tank analogue appears in matrix battles 3 (tank mode, one tank, attack-moving), 37a, 37b, 39a, 39b, 40 and 42 and
in the showcase effects scenario; the showcase, battles 31 and 33 and `golden_replay_orders` have none, and
`golden_replay` has three ("red_siege"). Every tank starts in tank mode (probe (c), `c-tank stance=tank`): battles
37a and 37b two per run, 39a, 39b and 40 three, 3 and 42 one, the showcase effects five (the siege line's three and
the raiders' two), `golden_replay` three. Siege channels start in 37a, 37b (two per run), 39a, 39b, 40 (three) and
42 (one), all from the battle's own script (front order none, so not a player's order), and in the showcase effects
scenario three per seed (the siege line). Battle 3's tank and the raiders' tanks never siege.

No trace output records a turret angle: the unit CSV of `sim_fixtures::trace` has the columns `tick, id, team, x, y,
vx, vy, hp, shields, phase, phase_target, phase_remaining, target, activity` (`crates/sim_fixtures/src/trace/
report.rs`), and `matrix.csv` and the route traces carry no angle and no checksum. A class-Y world therefore moves
only the goldens' checksums, never a measured output.

### Probes (§7.1), stored

The scratch probe (`~/scratch/turret-tracking/scripts/probe.rs`, applied to a copy of the tree by `apply_probe.py`,
logged per world on standard error) counts (t) every tick on which a tank-mode tank's turret is free by D-4's rule
(no target, or a `Ready` or `RotatingToTarget` weapon whose target lies outside its engagement band) while an enemy
its weapon may strike lies in its reach on acquisition's positions, tagged with its front order; (p) every siege
channel start, or swing start of a weapon whose acquisition or firing arc is below 180°, while a (t) episode is
open (until the turret next lies along the hull); (c) the census above. Its worlds are byte-identical to WP0's (only
the logs differ: `compare.sh` finds 10 log differences and no output difference).

| Family | (t) tracking opportunities | (p) pre-aim exposures |
|---|---:|---:|
| Matrix (seeds of record and 1 to 16) | 0 | 0 |
| Battles 31, 33 | 0 (no tank) | 0 |
| Showcase (default, 1–8) | 0 (no tank) | 0 |
| Showcase effects (default, 1–8) | 0 | 0 |
| `golden_replay` | 0 | 0 |
| `golden_replay_orders` | 0 (no tank) | 0 |

Every tank in these worlds attack-moves, holds a siege line or sieges by script. Acquisition engages an enemy on
the tick it enters reach (`Engagement::Seek`, or an automatic attack whose target is out of reach, which takes the
in-reach candidate), so its weapon owns the turret whenever an enemy is in reach. A positive control (a tank under a
Move past an idle worker, `probe_control.rs` in the probe copy) counts 114 (t) ticks, so the probe sees tracking
where it exists.

**What WP2 may move, fixed here:** no measured world; `golden_replay` moves by the schema bytes only (probe (t)
counts zero there); `golden_replay_orders` and the sandbox by the schema bytes only (no tank). Class Y and class P
are not expected in any measured world; a difference in one is stop rule 2.

### The close-battle list (§6.4), fixed before WP1

A run is won by the side with survivors while the other side has none; otherwise it is a draw. A battle is close
(items 1 and 2) when the side that wins fewer of its baseline runs wins at least 10 % of them, at least one. Derived
from WP0's worlds alone by `python3 -I ~/scratch/turret-tracking/scripts/close.py ~/scratch/turret-tracking/world-wp0`:

| Battle | Runs | Blue wins | Red wins | Draws | Minority share | Item |
|---|---:|---:|---:|---:|---:|---|
| 14_thor_vs_ultralisk | 20 | 5 | 15 | 0 | 0.25 | 1 |
| 26_100_lings_vs_6_colossi | 20 | 16 | 4 | 0 | 0.20 | 1, 3 |
| 28_100_lings_vs_10_immortals | 20 | 6 | 14 | 0 | 0.30 | `known_bug` B-192: not close (item 4) |
| 31_15_marauders_vs_15_stalkers | 20 | 17 | 3 | 0 | 0.15 | 1, 3 |
| 31_15_marauders_vs_15_stalkers, traced | 104 | 71 | 33 | 0 | 0.32 | 2 |
| 33_24_marines_vs_12_zealots, traced | 68 | 67 | 1 | 0 | 0.01 | 3 (item 2 not met) |
| 32_30_marines_vs_15_banelings | 20 | 15 | 1 | 4 | 0.05 | not close |

Every other matrix battle has one winner on all 20 runs, or one winner and draws, or draws only; none is close by
items 1 and 2. Item 3: Q0103 and Q0044 name battles 26, 33, 38a and 38b; `grep -n "close\|restated"
crates/sim_core/tests/fixtures/matchup_matrix.ron` finds battle 31's claim ("Equal supply is close"), battle 33's
comment and claim, and battle 35's claim and its restatement under charter §8; its other hits are the verb.

**The list: 14, 26, 31, 33, 35, 38a, 38b.** Battle 28 carries `known_bug` (B-192) and is reviewed under stop rule 5,
not restated. The list is not edited after WP1 starts. No battle on it contains a tank, so by the census no close
battle can move through this plan.

## WP1: the fields, latent (`48023594`)

Red first: on WP0's tree with only the new tests of `weapon_catalog_rules_tests.rs` and `checksum_state_tests.rs`
applied, both targets fail to compile (`no field turret_tracks_any_enemy`, `no field tracked_target`;
`~/scratch/turret-tracking/red/wp1-red.log`).

**L-1.** WP1's worlds (binaries built from the commit, assets from its `git archive`) against WP0's: `compare.sh`
finds 483 files and none differing. **Latent proof (§5):** a `git archive` copy of `48023594` whose encoding lists
`tracked_target` and `turret_tracks_any_enemy` under `unhashed` and whose schema is 48, each build in a fresh target
directory (`~/scratch/turret-tracking/scripts/proof.sh`, `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<copy>"`),
reproduces every checkpoint of both goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release and on
`wasm32-wasip1` release and debug (8 of 8 runs green). `crates/gate/suites.ron` needed no change: every test is in
an existing target.

## WP2: the turret tracks (`9a8e2ce2`)

Red first on WP1's tree (`~/scratch/turret-tracking/red/wp2-red.log`): T-1, T-2/T-3, T-4, T-6, T-7, T-9, T-14 (the
threat), T-15 in `turret_tracking_tests.rs`, T-11 in `runtime_catalog_tests.rs` and T-16 in
`attack_swing_snapshot_tests.rs` fail (10 tests). T-5 (the real weapon owns the turret) and T-8 (channels and the
sieged stance track nothing) pass before and after: they guard what tracking must not do.

**World check (§6.3).** WP2's worlds against WP1's: 483 files, none differing (the matrix on the seeds of record and
1 to 16, battles 31 and 33, the showcase and the showcase effects on the default seed and 1 to 8). No first
difference exists, so no world is classed Y or P; no outcome moved, no close battle is restated, and battle 28's
state is unchanged. This is what WP0's probes predicted: no tank-mode turret in these worlds is ever free with an
enemy in reach. The constructed tests carry the behaviour (each against a flag-off twin whose entities, generator
and projectiles equal it on every tick but for the turret's angle and the tracked enemy).

## WP3: confirmation and records

**Held-out seeds (§7.4)**, run once on WP0's tree and on the final tree (`heldout.sh`: matrix seeds 7001 to 7016,
battles 31 and 33 on 7001 to 7064, the showcase and the showcase effects on seeds 9 to 16): 381 files, none
differing. They confirm the development seeds; nothing was tuned or restated from them.

**Goldens and sandbox (§5).** Re-recorded once, on the final tree (no `main` commit landed since `91350af1`):
`golden_replay.ron` (20 checkpoints) and `golden_replay_orders.ron` (60) at schema 49, `DEFAULT_SANDBOX_CHECKSUM`
`10_709_269_369_881_941_416` (was `601_552_017_353_564_848`). Proofs, each a `git archive` copy in fresh target
directories, all green on native debug and release and `wasm32-wasip1` release and debug: (1) the final code with
both fields unhashed at schema 48 reproduces `main`'s lists and sandbox checksum, so no checkpoint moved by
behaviour (no class Y, no class P); (2) the re-recorded tree reproduces the new lists.

**Throughput (§5), stop rule 7 fired.** `cargo bench -p sim_core --bench throughput`, WP0's binary (`37ba5ed8`) and
the final binary (`9a8e2ce2`; the WP3 commit changes no code) interleaved, two rounds, each with its tree's catalog,
`perf stat -e instructions:u` over the whole binary (the benchmark has no per-scenario selector, so the budget is
read on the whole run, as the hold-facing plan did). Ticks per second, medians of 3 (round 1 / round 2):

| Scenario | WP0 | Final |
|---|---:|---:|
| 64x64, 0 units | 3091 / 3126 | 3119 / 3132 |
| 64x64, 100 | 789 / 801 | 808 / 804 |
| 64x64, 400 | 109 / 110 | 108 / 106 |
| 128x128, 400 | 175 / 177 | 178 / 178 |
| 256x256, 0 | 194 / 196 | 196 / 197 |
| showcase_canyon, 400 | 81 / 81 | 80 / 81 |
| 64x64, 67 casters, 400 | 201 / 202 | 202 / 201 |

Every scenario runs at 80 ticks per second or more: the 50 ticks/s floor holds. **Instructions do not:** WP0
268,618,534,581 and 268,618,533,467; final 274,805,621,132 and 274,785,444,032, **+2.30 %** against the +0.5 %
budget. Profiled (stop rule 7):

- `perf record -e instructions:u` puts the whole difference in `SimWorld::scan_candidates` (13.34 % of WP0's
  instructions, 15.24 % of the final's: +6.05 G).
- The number of scans is unchanged: a scratch counter in `scan_candidates` counts 1,530,396 calls on both trees (the
  benchmark's units carry no tracking weapon, so `decide_tracked_target` never scans).
- The cost is WP1's, not the rule's: WP1's binary (`48023594`, latent) measures 274,687,313,105 (+2.26 %). WP1 without
  `Entity::tracked_target` (the weapon's flag kept) measures 268,651,304,076 (+0.01 %); WP1 without the weapon's flag
  (the entity field kept) 274,655,800,871. The eight bytes `Entity` gains change the code generated for the scan's
  candidate loop (register allocation and stack spills in `scan_candidates`, 45 more instructions in the function),
  not what it does.
- Folding the scan's three licences into one enum (tried on scratch) leaves the count at 274,779,465,686, so the
  added licence test is not the cause; it was not kept.

The package stops here for review under stop rule 7; the plan names no remedy, and D-2 fixes the field on `Entity`.
Options for the reviewer, none taken: accept the cost (every scenario keeps a wide margin over 50 ticks/s); keep the
tracked enemy outside `Entity` (a world-level map, hashed, a change of D-2); or tune the loop's code generation.
