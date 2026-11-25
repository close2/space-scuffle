# Evidence: Carrier Interceptors — a Hangar of Launched Units (B-22, B-228)

Measurements of plan [`2026-11-28-carrier-interceptors`](../plans/2026-11-28-carrier-interceptors.md): WP0's census,
probe, positive control, close-battle list, four-build baseline and throughput (§7.1), and the world checks, red runs,
proofs and confirmations of the later packages. Scratch material (trees, binaries, worlds, the probe patch and its
control, the census script, the red runs) is under `~/scratch/carrier/`, never committed.

## WP0: census, probe, baseline and close battles

WP0's tree is the branch head `a9c0a248` (`main` `2afbad04` merged in: weapon delivery, checksum schema 56; Q0129
closed, Q0130 filed). Its commit adds only the records of plan §13 (B-22 and B-228 restated, B-229 and B-230
registered) and this file; every measurement below is of `a9c0a248`'s code and assets.

### Census (§7.1 (c))

`python3 -I ~/scratch/carrier/census/census.py <tree>` (output `~/scratch/carrier/census/census.txt`) reads every place
the Carrier analogue (`mothership_carrier`, `unit_type_id: 32`) can enter a world:

| Place | Mentions |
|---|---:|
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` (every battle's squads, battles 31 and 33) | 0 |
| `golden_replay.ron`, `golden_replay_orders.ron` (squads and structures) | 0 |
| `assets/catalog/start_loadouts.ron` | 0 |
| `crates/sim_core/benches/throughput_scenarios.ron` | 0 |
| `assets/scenarios/{default_battle,showcase,showcase_effects,skirmish}.ron` | 0 |
| `crates/sim_fixtures/src/**` (the sandbox, the trace and harness worlds) | 0 |
| Production code of every crate (C5 names no slug) | 0 |
| `assets/catalog/structures.ron`: `stellar_portal_dome` `trains_units` | the one trainer |

So the Carrier enters a world only by being trained at the Stargate analogue (by a player or an AI); no measured world
names it. **The census lists no measured world as carrying the Carrier**, so a first difference of class V or H
(§6.6) in any measured world is a stop (stop rule 2).

### Probe (§7.1)

The scratch probe (`~/scratch/carrier/probe/probe.rs`, applied to a `git archive` copy by `apply_probe.py`; it names the
slug, being scratch) counts (p) every Carrier entity committed to the world (spawned or trained, at
`SimWorld::commit_spawn`, the one insertion path), (r) every release of the Carrier's salvo, (k) every salvo strike that
takes a living target to zero life, and (d) every weapon hit on a Carrier (strikes, and life plus shields lost). It
changes no state: its worlds are byte-identical to WP0's (`compare.sh`: 490 files, none differing).

**Positive control.** A scratch test in the probe copy (`probe/probe_control.rs`) spawns a Carrier beside an enemy
`pulse_bot` and an enemy air-attacking saucer for 300 ticks: the probe logs (p) 1, (r) 2 missile releases, (k) 1, (d)
10 strikes and 88.0 life plus shields lost (`~/scratch/carrier/probe-wp0/control.log`). So a zero below is a measured
zero.

| World | (p) Carriers | (r) releases | (k) kills | (d) hits taken |
|---|---:|---:|---:|---:|
| Matrix, every battle (seeds of record and 1–16, 1,220 runs) | 0 | 0 | 0 | 0 |
| Battles 31 (100 seeds) and 33 (64 seeds) | 0 | 0 | 0 | 0 |
| Showcase, default seed and 1–8 (6,000 ticks) | 0 | 0 | 0 | 0 |
| Showcase effects, default seed and 1–8 (6,000 ticks) | 0 | 0 | 0 | 0 |
| Skirmish scenario (`scenarios/skirmish.ron`, 6,000 ticks) | 0 | 0 | 0 | 0 |
| `golden_replay`, `golden_replay_orders` | 0 | 0 | 0 | 0 |

**No measured world carries a Carrier**, as §3 item 10 predicted, including the showcase, showcase effects and skirmish
worlds whose AI could train one; stop rule 9 holds (no world the plan predicted empty carries it). The game data was
not re-read: §2.1's values are the plan's, read at `5444e12f13`.

### Baseline on four builds (§7.1)

A `git archive` copy of `a9c0a248`, every file touched, built in a fresh target directory
(`~/scratch/carrier/scripts/proofcopy.sh wp0 a9c0a248`, `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<copy>"`):
`golden_replay_tests` (every recorded checkpoint of both goldens) and `fixture_tests` (`DEFAULT_SANDBOX_CHECKSUM`) pass
on native debug and release and on `wasm32-wasip1` release and debug, 8 of 8 runs green (schema 56).

### The close-battle list (§6.7), fixed before WP1

A run is won by the side with survivors while the other has none; otherwise it is a draw. A battle is close when the
side that wins fewer of its baseline runs wins at least 2 of its 20 (battles 31 and 33: the same share of their 100 and
64 seeds). From WP0's worlds alone (`python3 -I ~/scratch/carrier/scripts/close.py ~/scratch/carrier/world-wp0`):

| Battle | Runs | Blue wins | Red wins | Draws | Minority share | Item |
|---|---:|---:|---:|---:|---:|---|
| 14_thor_vs_ultralisk | 20 | 5 | 15 | 0 | 0.25 | close |
| 26_100_lings_vs_6_colossi | 20 | 16 | 4 | 0 | 0.20 | close; named by Q0103, Q0044 |
| 28_100_lings_vs_10_immortals | 20 | 6 | 14 | 0 | 0.30 | `known_bug` B-192: not close |
| 31_15_marauders_vs_15_stalkers | 20 | 17 | 3 | 0 | 0.15 | close; its claim says so |
| 31_15_marauders_vs_15_stalkers, traced | 104 | 71 | 33 | 0 | 0.32 | close |
| 33_24_marines_vs_12_zealots, traced | 68 | 67 | 1 | 0 | 0.01 | named by its comment and claim, Q0041, Q0044 |
| 32_30_marines_vs_15_banelings | 20 | 15 | 1 | 4 | 0.05 | not close |

Every other matrix battle has one winner on all 20 runs, or one winner and draws, or draws only. `grep -n -i
"close\|restated" crates/sim_core/tests/fixtures/matchup_matrix.ron` finds battle 31's claim, battle 33's comment and
claim, battle 35's claim and restatement, and the restatements of 26, 38a and 38b; Q0103 and Q0044 name 26, 33, 38a
and 38b. **The list: 14, 26, 31, 33, 35, 38a, 38b**, the weapon-delivery plan's list. It is not edited after WP1
starts.

### Throughput (§5)

`cargo bench -p sim_core --bench throughput`, the release binary of `a9c0a248` (`~/scratch/carrier/bench-bin-wp0/`), one
run under `perf stat -e instructions:u` (load average 8.8 to 4.9, another agent's measurements running): 64x64 0 units
3140 ticks/s, 100 units 800, 400 units 108; 128x128 400 units 177; 256x256 0 units 195; `showcase_canyon` 400 units 81;
67 casters 199. Every scenario is above 50 ticks/s. User-space instructions of the whole run: 270,018,499,441. WP6
compares the final tree against this binary, interleaved.
