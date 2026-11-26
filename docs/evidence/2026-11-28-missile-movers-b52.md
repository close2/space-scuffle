# Evidence: Missile Movers for the Rest of B-52

Measurements of plan [`2026-11-28-missile-movers-b52`](../plans/2026-11-28-missile-movers-b52.md): WP0's census,
probe, positive control, close-battle list, four-build baseline and throughput (§7.1), and the latent proof, red runs,
world checks and confirmations of the later packages. Scratch material (trees, binaries, worlds, the probe patch and
its control, the census script, the red runs) is under `~/scratch/missile-movers-b52/`, never committed.

## WP0: census, probe, baseline and close battles

WP0's tree is the branch head `48ecc5ec` (the plan on `main` `4ff8402f`, checksum schema 57). Its commit adds only the
records of plan D-9 (B-52 restated, B-231 and B-232 registered) and this file; every measurement below is of
`48ecc5ec`'s code and assets.

### Census (§7.1 (c))

`~/scratch/missile-movers-b52/census/census.py` (the weapon-delivery `movers.py`, extended to print each defining
element's layer, file and line range, every field of each motion phase, child elements and attributes alike, and the
launching weapons' strike structure) resolves the five launches and the Brood Lord's escort across the eight layers of
SC2GameData `5444e12f13` (last layer wins, parents followed, `MotionPhases` merged by index). Output
`census/census.txt`:

| Plan source | Census | Agrees |
|---|---|---|
| S1 `LanzerTorpedoes` | `liberty` WeaponData:266–275 (`Range 9`, `Period 2`, `DisplayAttackCount 2`, `AllowedMovement Slowing`), `voidmulti` :133–135 (`DamagePoint 0.05`); persistent `liberty` EffectData:1698–1706 (`PeriodCount 2`, `LanzerTorpedoesLM` twice, `PeriodicPeriodArray 0, 0.21`) | yes |
| S2 `LanzerTorpedoesLM` | `liberty` EffectData:1707–1712: `AmmoUnit VikingFighterWeapon`, `ValidatorArray RangeCheckLE15`, no `Movers`, no flag, no `Visibility`; ammo unit `liberty` UnitData:6901–6905 `Mover VikingFighterMissile` | yes |
| S3 `VikingFighterMissile` | `liberty` MoverData:531–567 only: `Throw` 2/2 `Outro 0.125,0.25`; `Throw` 2/2 `Gravity 30` `Outro 0.625,0.75`; `Guidance` `Speed 2` `Acceleration 56` `MaxSpeed 28` `Outro 2,2.5`; `Guidance` `MaxSpeed 28` | yes |
| S4 `LiberatorMissileLaunchers` | `void` WeaponData:321–330 (`Period 1.8`, `DisplayAttackCount 2`, `Effect LiberatorMissileBurstPersistent`); persistent `void` EffectData:2942–2970 (`PeriodCount 2`, `LiberatorDamageMissileLM` twice, one `PeriodicPeriodArray 0.05`) | yes |
| S5 `LiberatorDamageMissileLM` | `void` EffectData:2979–2984 (the plan's 2979–2983 are its field lines; the element closes on 2984): `LaunchLocation CasterUnit`, `ImpactEffect LiberatorMissileDamage`, `AmmoUnit LiberatorDamageMissile`, no `Movers`; ammo unit `void` UnitData:3411–3415 `Mover LiberatorMissile` | yes |
| S6 `LiberatorMissile` | `void` MoverData:384–422: `Throw` `Speed 1` `Acceleration 3200` `MaxSpeed 8` `Outro 2.5,3.5` (`ThrowBandYaw 74.9926,-74.9926,64.995,-64.995`, `ThrowBandPitch 45,-45,4.9987,-4.9987`); `Guidance` `Acceleration 3200` `AccelerationRange 7` `MaxSpeed 16` `Outro -2.5,-1.5`; `Guidance` with no speed value | yes |
| S7 `BacklashRockets` | `liberty` WeaponData:548–562 (`Range 6`, `Period 1.25`, `DisplayAttackCount 2`); persistent `liberty` EffectData:2550–2557 (`PeriodCount 2`, `PeriodicPeriodArray 0.15`), `voidmulti` :36–39 (index 0 set to 0, then 0.15) | yes |
| S8 `BacklashRocketsLM` | `liberty` EffectData:2558–2563: `Movers BacklashRocketsLMWeapon` (no `IfRangeLTE`), no `AmmoUnit`; mover `liberty` MoverData:485–510: `Guidance` `Speed 10` `Acceleration 20` `MaxSpeed 50` `Outro 2.5`; `Outro -1`; empty | yes |
| S9 `LongboltMissile` | `liberty` WeaponData:300–309 (`Range 7`, `Period 0.8608`, `DisplayAttackCount 2`); persistent :1779–1788 (`Channeled`, `PeriodicPeriodArray 0, 0.1`); `LongboltMissileLM` :1789–1794 `AmmoUnit LongboltMissileWeapon`, no `Movers`; ammo unit `liberty` UnitData:11605–11608, no `Mover` (the default `##id##`) | yes |
| S10 `LongboltMissileWeapon` | `liberty` MoverData:589–600: `Guidance` `Speed 10` `MaxSpeed 10` `Outro -4`; a phase whose `Acceleration 200` and `MaxSpeed 20` are attributes of the `MotionPhases` element | yes |
| S12 `HighTemplarWeapon` | `voidmulti` WeaponData:284–294, `balancemulti` :257–267 (`Range 6`, `Period 1.754`, `Effect HighTemplarWeaponLM`); launch `voidmulti` EffectData:1532–1537, `balancemulti` :1404–1409 (`Movers HighTemplarWeaponMover`, no `IfRangeLTE`); mover `parent MissileDefault` (`voidmulti`, `balancemulti` MoverData:47); `MissileDefault` `core` MoverData:13–25: `Guidance` `Acceleration 3200` `MaxSpeed 18.75` `Outro -1`, then an empty phase | yes |
| S15 `BroodlingEscortLaunchA` | `liberty` EffectData:2664–2672 (`Movers` lines 2666–2667: `BroodLordWeaponRight`, `Left`, `IfRangeLTE 500`); escort missiles :2741–2752, :2774–2783 with the same movers; `BroodLordWeaponRight` MoverData:44–69, `Left` :620–644: `Throw` `Speed 5` `Acceleration 3` `MaxSpeed 8` `Outro 1.5,4.5`; `Guidance` `Acceleration 20` `MaxSpeed 40` | yes |

Every phase, band, ammo unit and strike structure of §2.1 is confirmed; stop rule 9 holds.

Where the five units can enter a world (read from the files at `48ecc5ec`): `matchup_matrix.ron` battles 12c and 19
(one `vector_interceptor` each) and 35 (twelve), and 13, 36a and 36b (one `cerebral_elder` each); the throughput
scenario "64x64, 67 casters" (`cerebral_elder`); the Starport analogue `aerospace_hangar_dome`'s `trains_units` names
`vector_interceptor`, `sky_bastion` and `cloak_gunship`; `upgrades.ron` names `flak_array_dome` and `cerebral_elder`
as upgrade targets. No golden, the sandbox, `start_loadouts.ron`, nor any `scenarios/*.ron` names any of the five.

### Probe and positive control (§7.1)

The scratch probe (`~/scratch/missile-movers-b52/probe/`; it names slugs, never committed) logs per world and seed, for
the five weapons, (r) releases, (t) the edge distance at each release with today's flight ticks (`ceil(path / speed)`)
and §2.4's (the planning script's integration, in the probe), (k) kills and (l) missiles lost in flight. Its worlds are
byte-identical to WP0's (`probe-wp0/compare.txt`: only `matrix.err`, which carries the probe's lines, differs).

**Positive control** (`probe_control.rs`, each weapon set on an enemy for 300 ticks): the Viking analogue released 16
missiles, the Liberator 20, the Banshee 4 (1 kill), the Missile Turret 6 and the High Templar 7. The control also
confirms §2.4's **verify**: the Turret analogue's releases read its attacker radius as 1.125, the structure's own.
So a zero below is a measured zero.

| World | Weapon | (r) releases | (t) mean flight, today → §2.4 | (k) kills | (l) lost |
|---|---|---:|---|---:|---:|
| 12c_viking_vs_colossus (20 runs) | Viking | 560 | 5.0 → 15.0 ticks | 20 | 0 |
| 19_viking_vs_brood_lord (20) | Viking | 360 | 4.0 → 14.0 | 20 | 0 |
| 35_12_vikings_vs_6_colossi (20) | Viking | 3,099 | 3.72 → 12.38 | 88 | 140 (last point) |
| 13_storm_vs_held_lings (20) | High Templar | 20 | 7.0 → 6.0 (edge 5.85) | 0 | 0 |
| 36a_storm_turns_marines (20) | High Templar | 74 | 7.0 → 6.8 | 1 | 26 (last point) |
| 36b_marines_beat_zealots_without_storm (20) | High Templar | 140 | 6.79 → 6.54 | 5 | 13 (last point) |

No release of any of the five in any other matrix battle, battles 31 (100 seeds) and 33 (64 seeds), the showcase and
showcase effects (default seed and seeds 1 to 8), the skirmish scenario, either golden (`golden_probe`, both replays)
or the sandbox. The Liberator, Banshee and Missile Turret analogues fire in no measured world, as predicted; so WP4
does not split (D-6), and WP4 and WP5 must leave every measured world identical. The prediction of §7.1 holds in full.

### Baseline on four builds (§7.1)

A `git archive` copy of `48ecc5ec`, every file touched, built in a fresh target directory
(`~/scratch/missile-movers-b52/scripts/proofcopy.sh wp0 48ecc5ec`, `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime
--dir=<copy>"`): `golden_replay_tests` (every recorded checkpoint of both goldens) and `fixture_tests`
(`DEFAULT_SANDBOX_CHECKSUM`) pass on native debug and release and on `wasm32-wasip1` release and debug, 8 of 8 runs
green (schema 57).

### The close-battle list (§6.6), frozen at this commit

A run is won by the side with survivors while the other has none; otherwise it is a draw. A battle is close when the
side that wins fewer of its baseline runs wins at least 2 of its 20 (battles 31 and 33: the same share of their 100 and
64 seeds). From WP0's worlds alone (`python3 -I ~/scratch/missile-movers-b52/scripts/close.py
~/scratch/missile-movers-b52/world-wp0`):

| Battle | Runs | Blue wins | Red wins | Draws | Minority share | Item |
|---|---:|---:|---:|---:|---:|---|
| 14_thor_vs_ultralisk | 20 | 5 | 15 | 0 | 0.25 | close |
| 26_100_lings_vs_6_colossi | 20 | 16 | 4 | 0 | 0.20 | close; named by Q0103, Q0044 |
| 28_100_lings_vs_10_immortals | 20 | 6 | 14 | 0 | 0.30 | `known_bug` B-192: not close |
| 31_15_marauders_vs_15_stalkers | 20 | 17 | 3 | 0 | 0.15 | close; its claim says so |
| 31_15_marauders_vs_15_stalkers, traced | 104 | 71 | 33 | 0 | 0.32 | close |
| 33_24_marines_vs_12_zealots, traced | 68 | 67 | 1 | 0 | 0.01 | named by its comment and claim, Q0041, Q0044 |
| 32_30_marines_vs_15_banelings | 20 | 15 | 1 | 4 | 0.05 | not close |
| 35_12_vikings_vs_6_colossi | 20 | 2 | 0 | 18 | 0.00 | named by its claim and restatement |

Every other matrix battle has one winner on all 20 runs, or one winner and draws, or draws only. `grep -n -i
"close\|restated" crates/sim_core/tests/fixtures/matchup_matrix.ron` finds battle 31's claim, battle 33's comment and
claim, battle 35's claim and restatement, and the restatements of 26, 38a and 38b; Q0103 and Q0044 name 26, 33, 38a
and 38b. **The list: 14, 26, 31, 33, 35, 38a, 38b**, the list of the two previous plans. It is not edited after this
commit. Battles 12c, 19, 13, 36a and 36b are not on it: an outcome move in any of them is stop rule 4.

### Throughput (§5)

`cargo bench -p sim_core --bench throughput`, the release binary of `48ecc5ec` (`~/scratch/missile-movers-b52/
bench-bin-wp0/`), one run under `perf stat -e instructions:u` (load average 21 to 14, this package's own builds
running): 64x64 0 units 3075 ticks/s, 100 units 802, 400 units 107; 128x128 400 units 177; 256x256 0 units 191;
`showcase_canyon` 400 units 80; 67 casters 197; 12 hangar units 381. Every scenario is above 50 ticks/s. User-space
instructions of the whole run: 297,769,229,666. WP6 compares the final tree against this binary, interleaved.
