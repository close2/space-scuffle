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
R + 115, R + 160, R + 205, R + 250 and R + 295 (§2.4's schedule, which leaves out the weapon's random delay; WP3
measured that the delay may hold a release up to 3 ticks past the arming, see below). The eighth
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

## WP1: the mechanisms, latent (`2aa72f59`)

**Red first** (`~/scratch/broodlings/red/`): `broodling_tests.rs` on WP0's tree does not compile (13 errors: no
`StrikeMagazineSpec`, `ImpactSpawn`, `TimedLifeSpec`, `Entity::strike_magazine`, `Entity::timed_life`,
`ImpactSpawnBlocked`; `wp1-red-on-wp0.log`). Behaviourally, on a copy of WP1's tree with each rule switched off
(`neuter_wp1.py`: the swing gate ignores the magazine, a release spends nothing, the magazine never rebuilds, no spawn
is collected, the clock never kills; `wp1-red-neutered*.log`): B-1 to B-8 fail and both magazine unit tests fail; B-9
(nothing created by a fizzled missile) passes there too, as a negative test must. On WP1's tree all pass, with the
validation (B-10, `catalog/strike_validation.rs`), encoding (`checksum/encoding.rs`) and placement unit tests.

**B-1 is exact**: with no random delay, the releases land on §2.4's ticks R, R + 57, R + 102, … over 20 strikes.
The magazine's upkeep counts the arming, then starts a build, then counts the build: a build started on the upkeep
after a release ends 45 ticks after it (the plan's prose order would end it one tick later and give R + 58); stop rule
10 holds on that order.

**Latent proof** (`proofcopy.sh wp1-latent 2aa72f59 unhash.py`): a `git archive` copy of WP1, every file touched,
the schema set back to 60 and the three new fields unhashed, built in a fresh target directory: both goldens and
`DEFAULT_SANDBOX_CHECKSUM` reproduce `main`'s recorded values on native debug and release and `wasm32-wasip1` release
and debug, **8 of 8**. Stop rule 1 holds; the schema steps to 61 (provisional).

**Worlds** against WP0's: every output file byte-identical (490 files, 0 differing). Stop rule 6 holds: no file under
`movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs` was edited in any package.

## WP2: the escort missile's mover (`d942ba2d`)

**Red first** on WP1's tree (`red/wp2-red.log`): B-11 (`the_brood_lord_escorts_land_after_their_flight`), the finder
census and the movers reference table fail. **B-11 lands on §2.4's tick at all eleven edge distances (0.5 to 10: 5,
7, 10, 12, 14, 16, 17, 18, 19, 20, 21 ticks), with no fixed-point exception**; stop rule 10 holds.

**Worlds** against WP1's: only `matrix.csv`/`matrix.md` differ, in battle 20 alone (20 of 20 runs; no winner or
expectation change; the roach dies on tick 149 to 155, was 142 to 147). Battle 19 is identical on all 20 runs. Per-unit
traces (`scripts/firstdiff.py trace-wp1 trace-wp2`): every run's first difference is the roach's life on tick 8 or 9,
the tick the constant-speed missile landed; it lands 13 ticks after its release at edge 3.5 instead. **Class D.**

## WP3: the escort magazine (`7b815da4`)

Q0132 was unanswered on `main` when WP3 landed (checked by fetching `origin/main`), so default (a) stands and stop rule
11 holds. **Red first** on WP2's tree (`red/wp3-red.log`): B-12 fails (no shipped unit has a strike magazine).

**The shipped random delay.** With the shipped weapon's random delay (−1.1161 to 2.2321 ticks, B-3) the charge arms
on §2.4's tick but the swing may still wait for the cooldown that release charged (42 ticks plus the delay's whole
ticks, carried): after the first two releases (R, R + 57, the magazine's alone), each follows the last by 43 to 48
ticks, and over 20 strikes the releases fall up to 3 ticks after §2.4's (B-12 asserts exactly that). §2.4 left the
delay out; B-1 without it is exact, so this is the plan's derivation, not the mechanism (recorded, no stop).

**Worlds** against WP2's: battle 20 alone (20 of 20; outcome and expectation unchanged; the roach dies on tick 298 to
304 of 400, as WP0's margin predicted, ±3). First difference (`firstdiff-wp3.txt`): the Brood Lord analogue's weapon
phase on tick 45 to 47, where WP2's swing started and WP3's, with no charge armed, does not. **Class M.** Battle 19 and
every other world identical.

## WP4: the Broodling (`3f51a790`)

**Red first** on WP3's tree (`red/wp4-red-on-wp3.log`): B-13 and B-15 fail (no shipped spawned unit), B-14
(`runtime_catalog_tests`) fails, and ten reference tables fail on the missing "Broodling" rows.

**Worlds** against WP3's: battle 20 alone (20 of 20; outcome and expectation unchanged; the roach dies on tick 163 to
167). First difference (`firstdiff-wp4.txt`): on tick 17 the held roach turns toward unit 3 (`rotating`, target 3), a
Broodling created at the end of tick 16, where WP3's first missile landed; the squads' rows are equal before it (the
trace lists squads, not spawned units; the spawn and its clock draw are on tick 16). **Class N.** Battle 19 and every
other world identical. No world WP0's probe did not list moved in any package; stop rules 2 and 4 hold; no close
battle moved, so nothing is restated.

## WP5: client (`41ae62a2`)

B-17 (`runtime_media_tests::a_spawning_unit_is_presentable_only_with_its_spawns_media`) and B-18
(`battle_input::hangar_pick_tests::a_spawned_unit_is_picked_as_any_unit`,
`battle_presentation_tests::a_spawned_units_expiry_draws_its_death`) pass. Red on WP4's tree
(`red/wp5-red-on-wp4.log`): B-17 fails (the spawner was presentable without its spawn's media); B-18's expiry test
already passes there, as D-12 item 2 predicts (spawned units are ordinary units and need no client code). B-19 (steady state with Broodlings cycling) waits for WP6's media, as the Carrier's H-20 does.

## WP6: media (`330c106a`), stopped

[`veo/2026-11-28-batch-broodlings.md`](veo/2026-11-28-batch-broodlings.md): 8 generations, $3.20 (ledger $120.05 of
$230); the Brood Lord analogue's four seeds gave no usable pin (three drew light shafts), the Broodling's pin is good
but its three pinned Idles nodded. Nothing installed; Q0133 filed; the Brood Lord analogue stays locked in the client.

## WP7: confirmation, re-record, records

**Goldens and sandbox** (`1e79a52b`; `main` `65326e39` was already merged, nothing landed since): both goldens and
`DEFAULT_SANDBOX_CHECKSUM` re-recorded once at schema 61 (provisional). `proofcopy.sh final 1e79a52b`: **8 of 8**
recorded (native debug and release, `wasm32-wasip1` release and debug, each a fresh target directory on a touched
`git archive` copy, `wasmtime --dir=<copy>`). `proofcopy.sh final-latent 1e79a52b unhash_final.py` (schema 60, the
three fields unhashed, `main`'s goldens and sandbox value): **8 of 8**, so nothing but the encoding moved these worlds.

**Held-out seeds** (`heldout.sh`, WP0's binary for `main` against the final tree's; matrix seeds 7001 to 7016, battles
31 and 33 on 7001 to 7064, showcase and showcase effects on 9 to 16, skirmish on 9 to 12): 409 files, only
`matrix.csv`/`matrix.md` differ, in battle 20 alone (16 of 16 seeds; no winner or expectation change; mean end 144.6 →
164.6). Battle 20 is the one world WP0's probe listed; its classes are D, M and N. Stop rule 3 holds.

**Throughput** (two interleaved rounds of WP0's binary, the final binary built with WP0's scenario table, `final-x`,
and the final binary with the `brood` row; `perf stat -e instructions:u`; load average 11 to 59 from other agents):

| Scenario | WP0 r2 | final-x r1 / r2 | final r1 / r2 |
|---|---:|---:|---:|
| 64x64, 0 units | 3097 | 3055 / 3104 | 3105 / 2719 |
| 64x64, 100 | 795 | 791 / 793 | 796 / 542 |
| 64x64, 400 | 106 | 108 / 104 | 108 / 105 |
| 128x128, 400 | 169 | 176 / 171 | 178 / 159 |
| 256x256, 0 | 189 | 194 / 190 | 194 / 189 |
| showcase_canyon, 400 | 77 | 80 / 78 | 81 / 75 |
| 64x64, 67 casters | 197 | 198 / 193 | 199 / 189 |
| 64x64, 12 hangar units | 384 | 385 / 376 | 385 / 374 |
| **64x64, 12 spawner units (brood)** | — | — | **402 / 403** |

Every scenario runs at 75 ticks/s or more (the plan's floor is 50); the brood row averages 1.6 spawned units alive per
measured tick. **Instructions: WP0 297,774,234,285 and 297,774,230,189; final-x 304,650,007,003 and 304,650,009,741:
+2.31 %, above the plan's +0.5 % (stop rule 5).** Bisection (one run each, `bisect-*.perf`, `var-*.perf`): WP1's binary
already gives 304,648,592,933 and WP4's 304,648,860,528; WP1 with the three fields unhashed 304,562,428,527 and WP1
with the new per-tick paths removed (the swing gate, the upkeep, the spawn collection) 305,052,424,268, so neither the
encoding nor the new code is the cost. WP0's tree with an unused 24-byte field added to `WeaponSpec` gives
304,284,195,045 and with an 8-byte boxed one 304,554,255,120: **any growth of `WeaponSpec` (376 → 384 bytes or more)
costs about 2.3 % of the run's instructions**, through the weapon phase's per-entity, per-tick `WeaponSpec` clone
(`ResolvedWeapon::to_spec` in `advance_weapon_cycles`) and the projectile list's copies; boxing `impact_spawn` would
not help. Rates are unchanged within noise. Fixing it means either removing the per-tick clone (a combat-phase change
outside this plan) or moving `impact_spawn` off `WeaponSpec` (an amendment of D-4); both are left to the orchestrator.

## After the weapon-spec-borrow merge (`main` `301d20d1`)

`main` now borrows each weapon in the combat phase instead of cloning `WeaponSpec` per tick and boxes
`Entity::siege_weapon`. Merged into the branch (`50462a75`; conflicts in `combat.rs`, where the swing gate keeps
`may_start_swing` on the borrowed `ResolvedWeapon`, and `combat_resolution.rs`, where the magazine spend follows
`begin_strikes(entity, weapon.spec, …)`); goldens and the sandbox still reproduce at schema 61.

**Throughput, re-measured** (`scripts/remeasure.sh 301d20d1 6a7d3f86`; two interleaved rounds of `main`'s binary, the
merged tree with `main`'s scenario table, and the merged tree with the `brood` row; load average 27 to 96):
instructions `main` 296,588,891,466 and 296,588,910,131, merged with `main`'s table 296,709,229,108 and 296,709,251,042:
**+0.041 %**, within +0.5 %. Stop rule 5 is cleared. Every scenario of the merged tree ran at 65 ticks/s or more under
that load (`main` itself dropped to 49 on the canyon in the same rounds); the `brood` row ran at 392 and 355.

**Worlds against `main`'s new baseline** (`wp.sh main 301d20d1`, `wp.sh merged 6a7d3f86 main`): 490 files, only
`matrix.csv`/`matrix.md` differ, in battle 20 alone (20 of 20 seeds, no winner or expectation change; mean end 144.3 →
164.4), as before the merge.

**Golden proof on the merged tree** (`proofcopy.sh merged 6a7d3f86`): both goldens and `DEFAULT_SANDBOX_CHECKSUM` at
schema 61 reproduce on native debug and release and `wasm32-wasip1` release and debug, **8 of 8**.

## WP6 continued and B-19

The media were generated after the owner's answer to Q0133
([`veo/2026-11-28-batch-broodlings.md`](veo/2026-11-28-batch-broodlings.md), continuation): six clips installed and
baked, strict audit 32 of 32, the Brood Lord analogue unlocked in the client.

**B-19 fails: stop rule 7.** The steady-state test (`~/scratch/broodlings/b19-test.patch`, not committed:
`broodlings_cycling_create_and_remove_no_bubble_material` in `sync_steady_state_tests.rs`) removes every other mobile
unit of the default battle, has the shipped Brood Lord analogue attack a held, durable, unarmed enemy, warms 400
ticks and observes 300: Broodlings come and go in the window (created and expired by their clocks), and the bubble
materials show **6 added and 6 removed**. `sync_live_battle_system` (`scene_controller.rs`) spawns a render entity with
its own `BubbleDomeMaterial` for every unit that enters the snapshot and despawns it (dropping the material) when the
unit leaves, and `RenderEntityMap` (a `BTreeMap`) inserts and removes a node each time. D-12 item 3's pool (departed
bubbles hidden and kept with their materials, reused by the next arrival; the map's storage retained at its
high-water mark, with a PERFORMANCE RATIONALE) is the fix; the Carrier's H-20 needs the same. It touches the live
sync path the allocation test measures, so it is left for the orchestrator to plan or assign.
