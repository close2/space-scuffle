# Evidence: Death Spawns and Hangar Commands — Part A, Broodlings from Dying Zerg Structures (B-234)

Measurements of plan [`2026-12-01-death-spawns-and-hangar-commands`](../plans/2026-12-01-death-spawns-and-hangar-commands.md),
packages WPA0 to WPA4: the census, probe, positive control, close-battle list, four-build baseline and throughput
(§7.1), and the red runs, proofs, world checks, held-out seeds and throughput of the later packages. Part B (B-230,
WPB0 to WPB3) is not built yet. Scratch material (trees, binaries, worlds, the probe patch and its control, the
census script, the red runs, the proof copies' status files) is under `~/scratch/death-spawns/`, never committed.

## WPA0: census, probe, baseline and close battles

WPA0's measured tree is `a7c03026` (the plan and Q0138 on `main` `0c16a3a9`, checksum schema 61); its code and
assets are `main`'s. `main` later merged the plan as `6a8fc821` (documents only), merged into this branch as
`07c9f1fc` with no change to any measured file.

### Census (§7.1 (c))

`python3 -I ~/scratch/death-spawns/census/dscensus.py` (the broodlings census helpers; output
`census/census.txt`) resolves every element of §2.1 across the eight layers of SC2GameData `5444e12f13`:

| Plan source | Census | Agrees |
|---|---|---|
| S1 | `ZergBuildingDies6` `liberty` BehaviorData:1270–1277, `ZergBuildingDies9` :1278–1285: `InfoFlags Hidden`, `BuffFlags DisableWhileUnderConstruction 1`, `DeathResponse Chance 1 Effect ZergBuildingSpawnBroodling6Delay` / `9Delay`; no other layer defines either | yes |
| S2 | `SDeathResponse`, `core` BehaviorData:19–21: `Type index Remove value 0` | yes |
| S3 | `ZergBuildingSpawnBroodling6Delay` `liberty` EffectData:1581–1586, `9Delay` :1587–1592: `ValidatorArray CasterIsNotHidden`, `ExpireDelay 0.5`, `FinalEffect` | yes |
| S4 | `CasterIsNotHidden` `core` ValidatorData:269–272: `WhichUnit Caster`, `Filters -;Hidden` | yes |
| S5 | `ZergBuildingSpawnBroodling6` EffectData:2791–2799, `9` :2800–2808: `SpawnUnit Broodling`, `SpawnCount 6` / `9`, `CreateFlags Placement 0`, `SpawnEffect BroodlingTimedLife`, `WhichLocation TargetPoint`, `SpawnOwner Caster` | yes |
| S6 | `BroodlingTimedLife` EffectData:772–775: `Behavior BroodlingFate`, no `UseDuration` | yes |
| S7 | `BroodlingFate` BehaviorData:650–655 (`Duration 8`, `KillCredit Unknown`, parent `TimedLife`); `TimedLife` `core` BehaviorData:105–117 (`DurationRandomMax 0.3`, `ExpireEffect TimedLifeFate`, `Summoned`, `Permanent`); `TimedLifeFate` `core` EffectData:207–211 (`Kill`, `NoKillCredit`) | yes |
| S8 | `ZergBuildingDies9` linked at `liberty` UnitData:8251 (Hatchery), :8333 (Lair), :8412 (Hive); `ZergBuildingDies6` at :669 (LurkerDenMP), :2070, :8478, :8920, :8983, :9046, :9117, :9249, :9936, :10000, :11799; Spine Crawler, Spore Crawler, Extractor, Creep Tumor and Nydus Canal link neither; every later-layer block of the 19 structures sets no `BehaviorArray` but the Extractor's `WorkerPeriodicRefineryCheck` (`voidmulti`) and the Nydus Canal's own; a grep of every layer finds no other mention of either behaviour or effect | yes |
| S11–S14 | Carrier `liberty` UnitData:3469–3543: `AbilArray CarrierHangar` :3512, `HangarQueue5` :3513, `LayoutButtons` :3521 (`CarrierHangar,Ammo1`, row 2 col 0), :3522 (`HangarQueue5,CancelLast`, row 2 col 4), :3523; `CarrierHangar` AbilData:2083–2098 (`Ammo1 Time 8 CountStart 4`, `Button Requirements ArmInterceptor`, `Flags AutoBuild`), `swarm` :467–469 (`MaxCount 8`), `void` :166–170 (a `Cooldown` on `Ammo1`, `TimeUse 0.01`, for part B), `voidmulti` :1296–1300 and `balancemulti` :1133–1137 (`Time 12`, `AutoBuildOn`); `HangarQueue5` AbilData:2072–2076 (`QueueSize 5`, `Passive`) | yes |

Every element is confirmed; **stop rule 9 holds**. Where a structure with a planned death spawn can enter a world (read
at `a7c03026`): no slug of the 14 appears outside `structures.ron` and `upgrades.ron` (`researched_at` and
`requires_structures`) in any crate or asset: not in `start_loadouts.ron`, any `scenarios/*.ron`, the matrix fixture,
battles 31 and 33, both goldens, the sandbox or `throughput_scenarios.ron`. No AI builds structures. The card raises
what a selected worker's faction offers, and no Swarm structure has media, so no client match fields one.

### Probe and positive control (§7.1 (p))

The scratch probe (`~/scratch/death-spawns/probe/`, `apply_probe.py`, naming slugs, never committed) prints one line
per structure the reap removes (tick, slug, faction, complete) and changes no state. Its worlds equal WPA0's byte for
byte but for the probe's lines in eight route logs (`probe-wp0/compare.txt`). **Positive control**
(`probe_control.rs`: a Hatchery, a Spawning Pool and a Spine Crawler analogue killed on a constructed map): three
lines, each `faction=sylvan_swarm complete=true`.

| World | Structure deaths | Swarm structure deaths |
|---|---:|---:|
| Matrix (20 runs per battle), battles 31 (100 seeds) and 33 (64 seeds) | 0 | 0 |
| Showcase, default seed and seeds 1 to 7 | 1 each (`foundry_core_dome`, complete) | 0 |
| Showcase seed 8, showcase effects (default and 1 to 8), skirmish (6,000 ticks) | 0 | 0 |
| `golden_replay.ron`, `golden_replay_orders.ron`, the sandbox | 0 | 0 |

The prediction of §7.1 holds: no measured world kills a Swarm structure. Hangar units present per world were not
probed here (part B's WPB0 re-takes its baseline and probes them).

### Baseline on four builds (§7.1)

`proofcopy.sh wp0 a7c03026` (a `git archive` copy, every file touched, a fresh target directory,
`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<copy>"`): `golden_replay_tests` (both goldens) and
`fixture_tests` (`DEFAULT_SANDBOX_CHECKSUM`) pass on native debug and release and `wasm32-wasip1` release and debug:
**8 of 8**, schema 61.

### The close-battle list (§6.6), frozen at this commit

`python3 -I ~/scratch/death-spawns/scripts/close.py world-wp0` (minority side at least 2 of 20 runs; battles 31 and
33 the same share of their seeds):

| Battle | Runs | Blue | Red | Draws | Minority share | Item |
|---|---:|---:|---:|---:|---:|---|
| 14_thor_vs_ultralisk | 20 | 5 | 15 | 0 | 0.25 | close |
| 26_100_lings_vs_6_colossi | 20 | 16 | 4 | 0 | 0.20 | close; named by Q0103, Q0044 |
| 28_100_lings_vs_10_immortals | 20 | 6 | 14 | 0 | 0.30 | `known_bug` B-192: not close |
| 31_15_marauders_vs_15_stalkers | 20 | 17 | 3 | 0 | 0.15 | close |
| 31, traced | 104 | 71 | 33 | 0 | 0.32 | close |
| 33_24_marines_vs_12_zealots, traced | 68 | 67 | 1 | 0 | 0.01 | named by its comment and claim |
| 32_30_marines_vs_15_banelings | 20 | 15 | 1 | 4 | 0.05 | not close |
| 35_12_vikings_vs_6_colossi | 20 | 0 | 0 | 20 | 0.00 | named by its claim |

**The list: 14, 26, 31, 33, 35, 38a, 38b** (the broodlings and Carrier plans' list). It is not edited after WPA0.

### Throughput (§5)

The release bench of `a7c03026` (`bench-bin-wp0`), one run under `perf stat -e instructions:u` (load 35 to 22):
64x64 0 units 2863 ticks/s, 100 units 727, 400 units 101; 128x128 400 units 174; 256x256 0 units 185;
`showcase_canyon` 400 units 77; 67 casters 191; 12 hangar units 315; 12 spawner units 168. User-space instructions of
the run: 324,436,095,403. WPA4 compares interleaved runs below.

## WPA1: death spawns, latent (`62e6dd5e`; C7 allowlist `88d286e6`)

**Red first** (`~/scratch/death-spawns/red/`): `death_spawn_tests.rs` on WPA0's tree does not compile
(`wpa1-red-on-wp0.log`: no `DeathSpawnSpec`, `ArchetypeCapabilities::death_spawn`, `SimWorld::pending_death_spawns`,
`DeathSpawnBlocked`). On WPA1's tree with the queue neutered (`queue_death_spawn` returns at once;
`wpa1-red-neutered.log`): DS-1, DS-2, DS-4 to DS-10 and DS-12 fail; DS-3 (a cancelled foundation creates nothing)
and DS-11 (validation) pass there too, as a negative test and a validation test must. On WPA1's tree all pass, with
the unit tests of the countdown and drain order, the field coverage and byte encoding of a pending entry, the
placement search and clock draw (`world::spawns`), and every D-1 rule (`catalog::strike_validation`).

**DS-7 holds, so stop rule 6 does not apply:** nine coincident units separate (edge distance at least −0.01) within
25 ticks with no movement invariant, by the solver's existing rule. No file under `movement/`, `steering.rs`,
`movement_rules.ron` or `target_acquisition.rs` is edited.

**A finding about DS-3.** `CancelConstruction` removes a foundation through the reap, as a death (it sets its life to
0, `production/construction.rs`), not by a separate removal, so a scratch spec with
`disabled_while_constructing: false` would leave units on a cancel. That matches 5.0.14's reading, where a cancel is a
death of type `Cancel`, not the `Remove` that `SDeathResponse` leaves out (S2). With the authored rule
(`DisableWhileUnderConstruction`, S1) a foundation never leaves units, cancelled or killed; DS-3 tests that.

**Worlds:** WPA1's worlds equal WPA0's byte for byte (`compare-wp1.txt`: 490 files, 0 differing).

**Latent proof (stop rule 1):** `proofcopy.sh wp1-latent 62e6dd5e unhash.py` (schema 61, the `death-spawns` section
left out, the base's goldens and sandbox value restored): **8 of 8** on native debug and release and `wasm32-wasip1`
release and debug. **Schema 62** (`main`'s next free; Gated WP2 had not merged), goldens and
`DEFAULT_SANDBOX_CHECKSUM` (`2_323_604_560_490_998_891`) re-recorded and proved: `proofcopy.sh wp1 62e6dd5e`,
**8 of 8**.

## WPA2: the Swarm structures' Broodlings (`6a311368`)

**Red first:** DS-13, DS-14 and DS-15 on WPA1's data fail (`red/wpa2-red-on-wpa1.log`: no row has a death spawn); with
the data they pass, with DS-6 over every shipped host. Two tests that loaded `structures.ron` with no units now load
the units first (a structure's death spawn names its unit by slug).

**Worlds:** WPA2's worlds equal WPA1's byte for byte (`compare-wp2.txt`: 490 files, 0 differing), as predicted: no
measured world kills a Swarm structure. Goldens and the sandbox unchanged (catalog data is not hashed, and neither
replay's world has a Swarm structure).

## WPA3: client (`827a5f81`)

**Red:** on WPA3's tree with `created_units` ignoring the death spawn and the death term forced to 0
(`red/wpa3-red-neutered.log`): DS-16, DS-17's count test, the shipped-bound test and the `created_units` unit test
fail; DS-17's zero test passes there too, as it must. On WPA3's tree all pass.

| Test | Result |
|---|---|
| DS-16 (`runtime_media_tests::a_structure_is_presentable_only_with_its_death_spawns_media`) | unpresentable (`Unavailable`, the created unit's slug) without the created unit's media, presentable with it |
| DS-17 (`render_pool` unit tests) | `death` = the start records' counts + players × the largest (9); 0 without death spawns or with untimed ones; the shipped two-player bound gains 18 |
| DS-18 steady state (`sync_steady_state_tests::razing_cycling_create_and_remove_nothing`) | passes: no asset added or removed, materials within their bounds, no entity spawned or despawned, no buffer moved, every arrival reused, `beyond_bound` 0 |
| DS-18 valgrind (`sync_allocation_count_tests::razing_cycle_frames_allocate_nothing`) | after 800 warm ticks, windows of 400 and 800 ticks: live 3664 and 7328 blocks, baseline 3664 and 7328: **0** beyond the drivers' |

**Worlds:** the final tree's worlds (`c82467b3`, WPA3's code with the bench row) equal WPA2's byte for byte
(`compare-final.txt`: 490 files, 0 differing).

## WPA4: confirmation, throughput; stopped at stop rule 5

### Held-out seeds (§7.3)

`heldout.sh` once on WPA0 (`a7c03026`) and once on the final tree (`c82467b3`): matrix seeds 7001 to 7016, battles 31
and 33 on 7001 to 7064, the showcase and showcase effects on seeds 9 to 16, the skirmish scenario on seeds 9 to 12.
**Byte-identical** (`compare-heldout.txt`: 409 files, 0 differing; `matrixdiff-heldout.txt` empty). Stop rule 3 holds.

### Throughput (§5)

Release bench binaries of WPA0 (`bench-bin-wp0`), the final tree without its razed row (`final-x`, for the
instructions of the existing scenarios) and the final tree, interleaved, two rounds, each run under the gate lock and
`perf stat -e instructions:u`. Ticks per second, medians of three repeats:

| Scenario | WPA0 r1 | WPA0 r2 | final-x r1 | final-x r2 | final r1 | final r2 |
|---|---:|---:|---:|---:|---:|---:|
| 64x64, 0 | 2467 | 3090 | 3090 | 3079 | 3073 | 3009 |
| 64x64, 100 | 727 | 792 | 797 | 789 | 795 | 777 |
| 64x64, 400 | 100 | 108 | 98 | 106 | 104 | 102 |
| 128x128, 400 | 173 | 175 | 169 | 175 | 169 | 170 |
| 256x256, 0 | 165 | 192 | 153 | 194 | 192 | 190 |
| showcase_canyon, 400 | 62 | 81 | 72 | 81 | 80 | 76 |
| 67 casters | 159 | 201 | 200 | 199 | 196 | 189 |
| 12 hangar units | 382 | 379 | 347 | 390 | 384 | 360 |
| 12 spawner units | 398 | 396 | 394 | 403 | 401 | 385 |
| **12 razed structures** (new) | — | — | — | — | **67** | **45** |

User-space instructions of the whole run: WPA0 324,436,219,286 and 324,436,086,813; final-x 324,468,444,785 and
324,468,503,233: **+0.010 %** on the existing scenarios (bar +0.5 %, holds). The final tree's runs (507,732,629,336 and
507,732,644,529) add the razed row, about 183 G instructions for its three repeats.

The razed row (`razed_structures: 12`: the 64x64 battle of 200 `pulse_bot` with 12 structures of the second player
carrying the largest timed death spawn, found by property: the Hatchery analogue, 9 Broodlings; on a 4 × 3 grid in the
centre, in the first army's path) runs 0.8 created units alive per measured tick. Its second round, **45 ticks per
second, is below the 50 of stop rule 5**; the package stopped here.

**Where the row's time goes** (`razed-variant.sh`, the final bench binary on a copy of the final assets whose death
spawns create 1 unit instead of 6 or 9, interleaved with the shipped assets, two rounds): shipped 61 and 65 ticks per
second (0.8 created units alive per tick), one unit per death 46 and 48 (0.0 alive per tick). So the cost is the
scenario's, not the created units': twelve 5 × 5 structures in the path of a 200-unit battle, which the army attacks
and walks around, cost as much with no Broodling alive. The mechanism's own per-tick work (one emptiness check in
upkeep, one capability lookup per reaped entity) shows as +0.010 % on the scenarios without death spawns. Across the
four runs of the shipped row: 67, 45, 61, 65.

Not done after the stop, for the orchestrator's decision: `DESIGN.md` §4 B-234 marked fixed, the §5 row and Combat
row, the pointer in plan `2026-11-28-broodlings` §14, the plan's `## As built` (part A).

## After merging `main` `b2ba4621` (`8b32ca78`, `27b9fe03`)

`main` gained the Baneling's whole cost (Q0135, `units.ron`) and plan documents; its schema is still 61, so **62 stays
the next free**. Re-taken on the merged tree `27b9fe03`: its worlds equal `main`'s byte for byte (`compare-merged.txt`:
490 files, 0 differing); the re-recorded goldens and sandbox value reproduce on the four builds (**8 of 8**,
`proof-merged/status`); and with the schema at 61, the section unhashed and `main`'s goldens and sandbox value
restored, every checkpoint reproduces `main`'s (**8 of 8**, `proof-merged-latent/status`).
