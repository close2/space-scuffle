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
