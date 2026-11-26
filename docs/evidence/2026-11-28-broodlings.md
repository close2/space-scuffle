# Evidence: Broodlings — the Brood Lord's Escort Magazine, Spawn on Impact and Timed Life

Measurements of plan [`2026-11-28-broodlings`](../plans/2026-11-28-broodlings.md): WP0's census, probe, positive
control, battle 20's margin, close-battle list, four-build baseline and throughput (§7.1), and the latent proof, red
runs, world checks and confirmations of the later packages. Scratch material (trees, binaries, worlds, the probe
patch and its control, the census script) is under `~/scratch/broodlings-plan/`, the red runs under
`~/scratch/broodlings/red/`; none of it is committed.

## WP0: census, probe, baseline and close battles

WP0's measured tree is the branch head `3e7934ed` (the plan merged with `main` `65326e39`, checksum schema 60). The
WP0 commit adds only the records of §13 (B-21 and B-232 restated, B-233 to B-236 registered) and this file; every
measurement below is of `3e7934ed`'s code and assets.

### Census (§7.1 (c))

`~/scratch/broodlings-plan/census/broodcensus.py` resolves every element of §2.1 across the eight layers of
SC2GameData `5444e12f13` (copy `~/scratch/combat_wp3_data/`), following parents, and prints each defining element's
layer, file, line range and every field it sets. Output `census/census.txt`:

| Plan source | Census | Agrees |
|---|---|---|
| S1 Brood Lord | `liberty` UnitData:11102–11165 (`WeaponArray BroodlingStrike`, `AbilArray BroodLordHangar`, `BroodLordQueue2`, `Radius 1`), `libertymulti` :132–142 and `swarm` :2414–2424 (life 225, armour 1), `voidmulti` :137–145 (`Speed 1.875`), `balancemulti` :113–119 | yes |
| S2 `BroodlingStrike` | `liberty` WeaponData:411–420 (`DisplayEffect BroodlingEscortDamage`, `TargetFilters Ground,Visible;Missile,Stasis,Dead,Hidden,Invulnerable`, `Range 9.5`, `Period 2.5`, `AllowedMovement Slowing`, `Effect ""`), `voidmulti` and `balancemulti` :74–77 (`Range 10`, `MinScanRange 10.5`) | yes |
| S3 `BroodLordHangar` | `liberty` AbilData:3808–3824 (`Leash 12`, `Retarget 1`, `Death InterceptorFate`, `Release KillsToCaster`, `Ammo1 Time 2.5 CountStart 2 Manage Recall Unit BroodlingEscort`, `AutoBuild`, `AutoBuildOn`, `External`, `ExternalAngle ±149.9853`), `voidmulti` and `balancemulti` :84–86 (`Leash 9`). **No `MaxCount` in any layer** | yes |
| S4 `BroodLordQueue2` | `liberty` AbilData:2077–2082 (`QueueSize 2`, `Hidden`, `Passive`) | yes |
| S5 `BroodlingEscort` unit | `liberty` UnitData:11194–11211 (the plan's :11214 is two lines past the element's close), parent `BroodlingDefault`: `Collide FlyingEscorts`, `Speed 6`, `Uncommandable`, `Unselectable`, `Untargetable`, `Invulnerable`, `StandardMissile`, `BroodlingAttackDelay`, `WeaponArray BroodlingEscort` | yes |
| S6 `BroodlingAttackDelay` | `liberty` BehaviorData:1361–1368 (`Duration 0.5`) | yes |
| S7 `BroodlingEscort` weapon | `liberty` WeaponData:421–429 (`TargetFilters Ground,Visible;…`, `Range 9`, `Period 10`); no `Effect` | yes |
| S8 the switch | `BroodlingEscort` `CEffectSwitch` `liberty` EffectData:2654–2658 (`IsStructure` → `BroodlingEscortStructure`, default `BroodlingEscortUnitSet`); `BroodlingEscortUnitSet` :2659–2663 (`BroodlingEscortRelease`, `BroodlingEscortLaunchA`); `BroodlingEscortRelease` `CEffectReleaseMagazine` :2738–2740 | yes |
| S9 `BroodlingEscortLaunchA` | `liberty` EffectData:2664–2672: `ImpactEffect BroodlingEscortDamageUnit`, `Movers BroodLordWeaponRight` and `Left` `IfRangeLTE 500`, `ValidateWeapon 0`, `FinishEffect BroodlingEscortImpactA`; no `AmmoUnit` | yes |
| S10 damage | `BroodlingEscortDamageUnit` :2678–2683 and `BroodlingEscortDamage` :2784–2790 (parent `DU_WEAP_MISSILE`, `ValidatorArray BroodlingEscortFilters`, `Amount 20`); `BroodlingEscortFilters` `liberty` ValidatorData:1723–1726 | yes |
| S11 `BroodlingEscortImpactA` | `CEffectCreateUnit` :2684–2692: `WhichLocation TargetUnitOrPoint`, `SpawnEffect BroodlingEscortLaunchB`, `SpawnUnit Broodling`, `SpawnRange 5`; **no `SpawnCount`, no validator** | yes |
| S12 launch B, order | `BroodlingEscortLaunchB` :2693–2699 (`BroodlingEscortMissileB`, `MakePrecursor`, `BroodlingTimedLife`, `…LaunchBTransfer`); `BroodlingEscortMissileB` :2705–2710 (`AmmoUnit BroodLordBWeapon`, `FinishEffect BroodlingEscortImpactB`); mover `BroodLordBWeapon` MoverData:36–43 (`Guidance`, `Acceleration 40`, `MaxSpeed 40`); `BroodlingEscortImpactB` :2711–2716; `BroodlingAttack` `CEffectIssueOrder` :2769–2773 (`Abil attack`, `Target Effect BroodlingEscort Value TargetUnitOrPoint`) | yes |
| S13 the Brood Lord's lifetime | `voidmulti` EffectData:163–165 (`BroodlingEscortLaunchB` index 2 → `BroodlingTimedLifeBroodLord`), :160–162 (`BroodlingEscortLaunch` index 3 the same); `BroodlingTimedLifeBroodLord` :2289–2294 (`Behavior BroodlingFate`, `UseDuration 1`, `Duration 5`) | yes |
| S14 timed life | `BroodlingFate` `liberty` BehaviorData:650–655 (parent `TimedLife`, `Duration 8`, `KillCredit Unknown`); `TimedLife` `core` BehaviorData:105–117 (`DurationRandomMax 0.3`, `ExpireEffect TimedLifeFate`); `TimedLifeFate` `core` EffectData:207–211 (`Kill`, `NoKillCredit`) | yes |
| S15 structure case | `BroodlingEscortStructure` :2717–2721; `BroodlingEscortCU` :2722–2730 (`NotHallucination`, `SpawnUnit Broodling`, `SpawnRange 5`, `SpawnEffect BroodlingEscortLaunch`); `BroodlingEscortMissile` :2741–2752 (same movers, `ImpactEffect BroodlingEscortDamage`, `FinishEffect BroodlingEscortImpact`); `BroodlingEscortImpact` :2757–2763; `BroodlingEscortFallbackMissile` :2774–2783 | yes |
| S16 Broodling | `liberty` UnitData:11166–11193 (parent `BroodlingDefault`: `LifeMax 30`, `LifeRegenRate 0.2734`, `Light`, `Biological`, `Sight 7`, `AttackTargetPriority 20`, `Radius 0.375`, `SeparationRadius 0.375`, `NoScore`; own: `Collide Ground, ForceField, Small`, `Speed 3.836`, `Acceleration 1000`, `TurningRate 999.8437`, `AbilArray stop, attack, move`, `WeaponArray NeedleClaws`), `swarmmulti` :65–67, `voidmulti` :150–156 (`LifeMax 20`, `Speed 2.9531`), `balancemulti` :123–126 | yes |
| S17 `NeedleClaws` | `liberty` WeaponData:109–117 (`MinScanRange 15`, `Range 0.1`, `Period 0.6455`), `voidmulti` :402–404 (`Period 0.8`); damage `liberty` EffectData:1413–1416 (`Amount 4`) | yes |
| S18 upgrades | `liberty` UpgradeData:502–503 (`NeedleClaws` +1), :599–600 (`Broodling` armour +1), :674–676 (escort damage +3), `libertymulti` :132–141 and `swarm` :427–436 (+2 per level) | yes |
| S19 movers | `BroodLordWeaponRight` `liberty` MoverData:44–69 and `Left` :620–644: `Throw` `Speed 5` `Acceleration 3` `MaxSpeed 8` `Gravity 5` `Outro 1.5,4.5` `ThrowVector ±0.8,−1,0.75`; `Guidance` `Acceleration 20` `MaxSpeed 40` | yes |

Every element of §2.1 is confirmed; no `MaxCount`, no other rebuild time, no `SpawnCount`, no other lifetime and no
condition on the finish effect. **Stop rule 9 holds.** Other users (B-234's reach): `BroodlingTimedLife` is applied by
`ZergBuildingSpawnBroodling6`/`9` (`liberty` EffectData:2791–2808) and the campaign Queen's
`QueenMPSpawnBroodlingsCreate` (`void` EffectData:2198–2203); `TimedLife` is the parent of the timed lives of the
Auto-Turret, MULE, Point Defense Drone, Raven repair drone, Stasis Ward, Changeling, Force Field and others; the
`CEffectCreateUnit` users among shipped analogues' 5.0.14 units are those spawns, the hallucinations and the Adept's
shade, none modelled.

Where `spore_matriarch` can enter a world (read from the files at `3e7934ed`): `matchup_matrix.ron` battles 19 (one,
red) and 20 (one, blue); `structures.ron`'s `apex_spire_dome` trains it. No golden, the sandbox,
`start_loadouts.ron`, any `scenarios/*.ron` or any throughput scenario names it.

### Probe and positive control (§7.1)

The scratch probe (`~/scratch/broodlings-plan/probe/`, naming slugs, never committed) logs per world and seed, for the
Brood Lord analogue, its releases with the strikes each carries and the plane and kind of its target, the edge
distance at each release with today's flight ticks (`ceil(path / 0.9)`) and §2.4's (the planning script's
integration), each impact with whether the missile still had its target and the target passed the filters, and the
target's life before and after. Its worlds are byte-identical to WP0's (`probe-wp0/compare.txt`: only `matrix.err`,
which carries the probe's lines, differs).

**Positive control** (`probe_control.rs`: the analogue attacking a ground Ultralisk analogue for 300 ticks): 5
releases of 2 strikes, 5 impacts with the target held and available, flight ticks today 6 to 13, §2.4 14 to 21. So a
zero below is a measured zero.

| World | Releases | Strikes per release | Edge at release | Flight today → §2.4 | Impacts (target held, available) |
|---|---:|---|---|---|---:|
| 20_brood_lord_vs_roach (20 runs) | 80 | 2 (all) | 3.50 to 3.57 | 5–6 → 13 ticks | 80 |

The analogue releases in no other world: none in battle 19 (its target is the air Viking: the ground-only rule holds),
in any other matrix battle, battles 31 (100 seeds) and 33 (64 seeds), the showcase and showcase effects (default seed
and seeds 1 to 8), the skirmish scenario, or either golden (`golden_probe`, both replays). The prediction of §7.1
holds in full.

### Battle 20's margin (§6.7)

Measured per run: the first release on tick 4 (all 20 runs), edge 3.50 to 3.57, the roach (145 life, armour 1)
regenerating 0.01531 per tick (107.0000 → 107.7043 over the 46 ticks between impacts), 19 per strike; today it dies at
the fourth impact, ticks 142 to 147.

**WP3 alone** (with WP2's mover, 13 ticks of flight at this edge): strikes land on R + 13 (2), then one each on R + 70,
R + 115, R + 160, R + 205, R + 250 and R + 295 (§2.4's schedule; the weapon's random delay cannot move it: the
cooldown it charges, 42 + at most 2.2 ticks, ends before the next charge arms, 45 ticks after the last). The eighth
strike, on R + 295 = tick 299, brings 152 damage against 145 + 299 × 0.01531 = 149.6 life: the roach dies on about tick
299, inside the 400-tick limit with one strike's margin in time (the seventh leaves it 15.9 short) and 2.4 life to
spare at the eighth. **WP3 alone keeps battle 20's expectation, so the pre-decided amendment (WP3 and WP4 measured
together) is not taken.** WP3 with WP4's Broodlings is estimated from the mechanism once WP1 exists (a scratch catalog
of WP2 + WP3 + WP4, §WP3 below), not by a scratch patch at WP0: the mechanism did not exist yet.

### Baseline on four builds (§7.1)

A `git archive` copy of `3e7934ed`, every file touched, built in a fresh target directory
(`~/scratch/broodlings-plan/scripts/proofcopy.sh wp0 3e7934ed`, `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime
--dir=<copy>"`): `golden_replay_tests` (every recorded checkpoint of both goldens) and `fixture_tests`
(`DEFAULT_SANDBOX_CHECKSUM`) pass on native debug and release and on `wasm32-wasip1` release and debug, 8 of 8 runs
green (schema 60).

### The close-battle list (§6.6), frozen at this commit

From WP0's worlds alone (`python3 -I ~/scratch/broodlings-plan/scripts/close.py ~/scratch/broodlings-plan/world-wp0`;
a battle is close when the side that wins fewer of its baseline runs wins at least 2 of its 20; battles 31 and 33 the
same share of their 100 and 64 seeds):

| Battle | Runs | Blue wins | Red wins | Draws | Minority share | Item |
|---|---:|---:|---:|---:|---:|---|
| 14_thor_vs_ultralisk | 20 | 5 | 15 | 0 | 0.25 | close |
| 26_100_lings_vs_6_colossi | 20 | 16 | 4 | 0 | 0.20 | close; named by Q0103, Q0044 |
| 28_100_lings_vs_10_immortals | 20 | 6 | 14 | 0 | 0.30 | `known_bug` B-192: not close |
| 31_15_marauders_vs_15_stalkers | 20 | 17 | 3 | 0 | 0.15 | close; its claim says so |
| 31_15_marauders_vs_15_stalkers, traced | 104 | 71 | 33 | 0 | 0.32 | close |
| 33_24_marines_vs_12_zealots, traced | 68 | 67 | 1 | 0 | 0.01 | named by its comment and claim, Q0041, Q0044 |
| 32_30_marines_vs_15_banelings | 20 | 15 | 1 | 4 | 0.05 | not close |
| 35_12_vikings_vs_6_colossi | 20 | 0 | 0 | 20 | 0.00 | named by its claim and restatement |

Every other matrix battle has one winner on all 20 runs, or one winner and draws, or draws only. The claims, comments
and closed questions name 26, 31, 33, 35, 38a and 38b. **The list: 14, 26, 31, 33, 35, 38a, 38b**, the list of the
B-52 plan. It is not edited after this commit. Battles 19 and 20 are not on it: an outcome move in either is stop
rule 4.

### Throughput (§5)

`cargo bench -p sim_core --bench throughput`, the release binary of `3e7934ed` (`~/scratch/broodlings-plan/
bench-bin-wp0/`), one run under `perf stat -e instructions:u` (load average 27 to 35, another agent's measurements
running): 64x64 0 units 3047 ticks/s, 100 units 612, 400 units 63; 128x128 400 units 115; 256x256 0 units 172;
`showcase_canyon` 400 units 65; 67 casters 159; 12 hangar units 280. Every scenario is above 50 ticks/s. User-space
instructions of the whole run: 297,774,251,463. WP7 compares the final tree against this binary, interleaved.
