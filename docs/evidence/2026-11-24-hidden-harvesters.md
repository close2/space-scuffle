# Evidence: gas gatherers hidden inside the extractor (B-161)

Plan: [`docs/plans/2026-11-24-hidden-harvesters.md`](../plans/2026-11-24-hidden-harvesters.md). Measured on the package
branch `hidden-harvesters-plan` on top of `main` `e313268f` (plan commits `ee029d65`, `c0d679d8`), 2026-11-24, on the
shared VM. Scratch material lives under `~/scratch/hidden-harvesters/` (`scripts/`, `tool/`, `bin-<wp>/`,
`tree-<wp>/`, `world-<wp>/`, `income/`, `red/`, `golden-<label>/`, `client-<label>/`).

## 1. WP0: sources and baseline

### 1.1 The counts of plan §2.1

Read from `~/scratch/combat_wp3_data/<layer>/` (SC2GameData `5444e12f13`, all eight layers):

```bash
cat */WeaponData.xml | grep '<TargetFilters' | grep -c Hidden     # 144
cat */WeaponData.xml | grep -c '<TargetFilters'                   # 146
cat */EffectData.xml | grep 'SearchFilters' | grep -c Hidden      # 199
cat */EffectData.xml | grep -c 'SearchFilters'                    # 247
grep -n HideHarvesters */BehaviorData.xml
```

The last command lists `liberty` BehaviorData:97, :109, :121 (rich geysers), :824, :836, :848
(`HarvestableVespeneGeyserGas`, `…Protoss`, `…Zerg`) and `swarm` BehaviorData:857, :868, :879 (terrazine), each
`<Flags index="HideHarvesters" value="1"/>`; no later layer clears it. The Refinery, Assimilator and Extractor list
the three behaviours in their `BehaviorArray` at `liberty` UnitData:5806, :4151 and :8179. All match S1 to S5.

The TL.net "SC2 Mythbusters" page (§16 of the plan) was not fetched again; no default depends on it.

### 1.2 P-0: no measured world harvests

`grep` over the tree at `c0d679d8`:

| Where | `Harvest` or `ReturnCargo` |
|---|---|
| `assets/scenarios/` | none; `InitialOrder` variants used: `AttackMoveToStart` (3), `AttackMoveToCell` (3), `EnterModes` (2) |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron` | 0 lines each; their headers say "Nothing in this replay harvests" |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | 0 |
| `crates/sim_fixtures/src/` | 0 |
| `crates/sim_core/benches/` | 0 |

The scratch probe (`scripts/probe.sh`) patches an archive of `c0d679d8` so that `SimWorld::step_into` prints one line
when a world starts and one line on every tick on which any unit holds cargo or has a front order `Harvest` or
`ReturnCargo`, builds `matchup_trace` and `route_trace` from it and runs `measure.sh` (every world of plan §7):

| Lines | Count |
|---|---:|
| `P0PROBE start` (worlds played) | 1,390 |
| `P0PROBE tick … harvesting …` | **0** |

So nothing harvests in any measured world, and plan §7's predictions stand. The probe's world files are
byte-identical to `world-wp0` (`compare.sh`: 483 files, the 21 differing ones are the logs that carry the probe's own
lines).

### 1.3 Worlds (`world-wp0/`)

`scripts/wp.sh <wp> <commit> <housed-expr>` exports the commit to `tree-<wp>/`, adds the scratch income tool to
`crates/sim_core/examples/` of that export only, builds `matchup_trace`, `route_trace` and `harvest_occ` in release in
a separate target directory, runs `measure.sh` (the attack-orders script; `route_trace` gets `--assets <tree>/assets`)
into `world-<wp>/`, compares it with `world-wp0/` and runs `income.sh`. The matrix (every battle, seeds of record and
16: 1,200 runs, 0 expectation failures on the seeds of record, 4 over all seeds), battle 31 on 100 seeds, battle 33 on
64, the showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks: 483 files, the
reference for every later comparison.

### 1.4 Income (`income/wp0-*`)

The handoff plan's tool (`tool/harvest_occ.rs`) with two probes added for this plan: every take of every node is
printed as `take,<node>,<tick>,<amount>`, and every trip of a worker (from one take to the next) as
`housed_trip,<kind>,<tick ends housed>,<trips>`. `HOUSED_PROBE` is replaced per tree: `false` before WP2 (no state
exists), `e.is_housed()` from WP2 on. The cases are the handoff plan's §6.1 (`income.sh`): mineral lines `k` = 1 to 4
on showcase slots 0 and 1 and `default_arena` slot 1, mineral nodes `k` = 1, 2 on showcase slot 0, and gas nodes
`k` = 1 to 4 on showcase slot 0 and `default_arena` slots 0 and 1. WP0: 26 files, 5,661 takes; every trip housed 0
tick ends.
