# Evidence: Missiles Where 5.0.14 Launches Them (B-227; B-52 in part)

Measurements of plan [`2026-11-27-weapon-delivery`](../plans/2026-11-27-weapon-delivery.md): the census, the probe,
the baseline and the close-battle list of WP0 (§7.1, §6.5), the latent proof and world checks of WP1 to WP3 (§5,
§6.4 to §6.6), and the held-out run, throughput and golden re-record of WP4 (§5, §7.3). Scratch material (trees,
binaries, worlds, the probe patch and its control, the census scripts, the red runs) is under
`~/scratch/weapon-delivery/`, never committed.

## WP0: census, probe, baseline and close battles

WP0's tree is the branch head `dff44e48` (`main` `c3c372c6` merged in: launch and spell visibility, checksum schema
53). Its commit adds only these records.

### Census (§7.1 (c))

`~/scratch/weapon-delivery/census/movers.py` resolves a launch effect across the eight layers of game data
`5444e12f13` (5.0.14.94342; last layer wins, parents followed, `MotionPhases` merged by index): its `Movers` array
(link, `IfRangeLTE`), `AmmoUnit`, flags and `Visibility`, the ammo unit's own `Mover` (`##id##` read as the unit's own
id), and each mover's phases (`Driver`, `Speed`, `Acceleration`, `MaxSpeed`, `Outro`). Output:
`~/scratch/weapon-delivery/census/movers.txt`.

| Launch effect | Movers (`IfRangeLTE`) | Phases (`Speed`, `Acceleration`, `MaxSpeed`, `Outro`) | Plan |
|---|---|---|---|
| `IonCannonsLMLeft`, `IonCannonsLMRight` (`liberty`) | `MissileDefault` ≤ 1; `IonCannonsWeaponLeft` / `…Right` ≤ 6 | `MissileDefault` (`core`): `Guidance`, —, 3200, 18.75, −1; then a phase with no value (the speed carried on). `IonCannonsWeapon{Left,Right}` (`liberty`): `Throw` 6, —, 6, 0.4,0.85; then `Outro` 1,1.5 only; then `Guidance` —, 60, 40 | S5, S7, S8 hold |
| (ammo unit `IonCannonsWeapon`'s own mover) | — | `IonCannonsWeapon`: the same three phases as Left and Right | §2.2 rule 5 holds |
| `ImpalerTentacleLM` (`liberty`) | `SpineCrawlerTentacleExtendShort` ≤ 3; `…ExtendLong` ≤ 7 | Short: `Throw` 1, 10, 20, 0.2,0.4; then `Guidance` —, 140, 200. Long: `Guidance` 12, 140, 200 | S10, S11 hold |
| (ammo unit `SpineCrawlerWeapon`'s own mover) | — | none defined in any layer, so no mover exists beyond 7 | §2.2 rule 5 holds |

Flags and visibility: no `Visibility` on any of the three launch effects (every one `Snapshot`, `core` EffectData:102);
`ImpalerTentacleLM` sets only `Return 1`; `IonCannonsLM*` set no flag. The `ValidatorArray` entry
`SpineCrawlerLMTargetFilters` is named only by `ImpalerTentacleLM` itself and defined in no layer's
`ValidatorData` (S10's inference holds). `MissileDefault`'s phase 0 ends 1 unit before the target (`Outro −1`) into a
phase that carries the speed on, so the mover is a constant 18.75 (1.05 per tick) as S8 states. `CarrierInterceptor`
is a `CEffectUseMagazine` and `InterceptorLaunchPersistent` a channeled persistent: no missile (S13). **No phase,
band or flag differs from §2.1; stop rule 9 holds.**

**B-52's remaining rows, recorded for the follow-up plan (§14):**

| Weapon | Mover (where named) | Phases (`Speed`, `Acceleration`, `MaxSpeed`, `Outro`) |
|---|---|---|
| Viking `LanzerTorpedoesLM` | ammo unit `VikingFighterWeapon`'s `VikingFighterMissile` (`liberty`) | `Throw` 2, —, 2, 0.125,0.25; `Throw` 2, —, 2, 0.625,0.75; `Guidance` 2, 56, 28, 2,2.5; `Guidance` —, —, 28 |
| Liberator `LiberatorDamageMissileLM` | ammo unit `LiberatorDamageMissile`'s `LiberatorMissile` (`void`) | `Throw` 1, 3200, 8, 2.5,3.5; `Guidance` —, 3200, 16, −2.5,−1.5; `Guidance` |
| Banshee `BacklashRocketsLM` | `BacklashRocketsLMWeapon`, any range (`liberty`) | `Guidance` 10, 20, 50, 2.5; `Outro` −1; an empty phase |
| Missile Turret `LongboltMissileLM` | ammo unit `LongboltMissileWeapon`'s own (`liberty`) | `Guidance` 10, —, 10, −4; then —, 200, 20 |
| High Templar `HighTemplarWeaponLM` | `HighTemplarWeaponMover`, any range (`balancemulti`; a `MissileDefault` child in `voidmulti` and `balancemulti`) | `MissileDefault`'s: constant 18.75 |

The other shipped missile launches (`AcidSalivaLM`, `ParticleDisruptors`, `AdeptLM`, `RavagerWeaponLM`,
`ParasiteSporeLaunchMissile`) fly `MissileDefault` or a child of it, `PunisherGrenadesLM` a 3200-to-20 guidance,
`NeedleSpinesLaunchMissile` and `AcidSpinesLM` constant 40, `BroodlingEscortLaunchA` a throw-then-guidance pair ≤ 500
(not in B-52); `GlaiveWurm`, `SporeCrawler`, `TalonsLM`, `PhotonCannonLM` name no mover of their own beyond their ammo
unit's (`movers.txt`).

### Probe (§7.1)

The scratch probe (`~/scratch/weapon-delivery/probe/probe.rs`, applied to a copy of the tree by `apply_probe.py`; it
names slugs, being scratch) counts at every release of the three weapons (`flying_saucer`'s air weapon,
`thorn_lasher_dome`'s ground weapon, `mothership_carrier`'s salvo) (r) the release, instant or missile, (t) the edge
distance at the release (centre distance less both collision radii) and the band D-2 would choose, and (k) every
strike of those weapons that takes a living target to zero life. It changes no state: its worlds are byte-identical
to WP0's (`compare.sh`: 490 files, none differing).

**Positive control.** A scratch test in the probe copy (`probe/probe_control.rs`) sets each weapon on an enemy for 300
ticks; the probe logs 17 `flying_saucer` releases (all in the `IonCannonsWeapon` band, edge ≤ 3), 2
`thorn_lasher_dome` releases (`ExtendShort`, edge ≤ 2.5), 2 `mothership_carrier` missile releases, and one kill by
each (`~/scratch/weapon-delivery/probe-wp0/control.log`). So a zero below is a measured zero.

| World | (r) releases | (t) bands | (k) kills |
|---|---:|---:|---:|
| Matrix, every battle (seeds of record and 1–16, 1,220 runs) | 0 | 0 | 0 |
| Battles 31 (100 seeds) and 33 (64 seeds) | 0 | 0 | 0 |
| Showcase, default seed and 1–8 (6,000 ticks) | 0 | 0 | 0 |
| Showcase effects, default seed and 1–8 (6,000 ticks) | 0 | 0 | 0 |
| Skirmish scenario (`scenarios/skirmish.ron`, the client's skirmish stage; 6,000 ticks) | 0 | 0 | 0 |
| `golden_replay`, `golden_replay_orders` | 0 | 0 | 0 |

**No measured world fires any of the three weapons**, as §7.1 predicted; `golden_replay_orders`' `blue_spotter`
(`flying_saucer`) releases nothing. So every first difference of class D (§6.4) is a stop in every measured world.
The skirmish scenario is added to the measure script (`scripts/measure.sh`) as the plan's "skirmish stage" world.

### Baseline on four builds (§7.1)

A `git archive` copy of `dff44e48`, every file touched, built in a fresh target directory
(`~/scratch/weapon-delivery/scripts/proofcopy.sh wp0 dff44e48`, `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime
--dir=<copy>"`): `golden_replay_tests` (every recorded checkpoint of both goldens) and `fixture_tests`
(`DEFAULT_SANDBOX_CHECKSUM`) pass on native debug and release and on `wasm32-wasip1` release and debug, 8 of 8 runs
green (schema 53).

### The close-battle list (§6.5), fixed before WP1

A run is won by the side with survivors while the other side has none; otherwise it is a draw. A battle is close when
the side that wins fewer of its baseline runs wins at least 2 of its 20 (battles 31 and 33: the same share of their
100 and 64 seeds). From WP0's worlds alone (`python3 -I ~/scratch/weapon-delivery/scripts/close.py
~/scratch/weapon-delivery/world-wp0`):

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
and 38b. **The list: 14, 26, 31, 33, 35, 38a, 38b**, the same as the visibility plan's WP0. It is not edited after
WP1 starts. The matrix plays 1,220 runs: 2 expectation misses on the seeds of record (battle 28, `known_bug` B-192)
and 26 over all seeds.

### Throughput (§5)

`cargo bench -p sim_core --bench throughput`, the release binary of `dff44e48` (`bench-bin-wp0/`), one run with
`perf stat -e instructions:u` (load average 9.8 to 7.8, another agent's harness running): 64x64 0 units 3139 ticks/s,
100 units 811, 400 units 110; 128x128 400 units 182; 256x256 0 units 196; `showcase_canyon` 400 units 82; 67 casters
202. User-space instructions of the whole run: 275,790,329,920. WP4 compares the final tree against this binary,
interleaved.

The trace binaries, the throughput binary and the worlds of this tree are kept under `~/scratch/weapon-delivery/`
(`bin-wp0/`, `bench-bin-wp0/`, `world-wp0/`) for the comparisons of WP1 to WP4.
