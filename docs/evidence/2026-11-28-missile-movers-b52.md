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

## WP1: target-relative phase ends, latent (`ecdaaea9`)

**Red first** (`~/scratch/missile-movers-b52/red/`): the WP1 tests on WP0's tree do not compile (`PhaseEnd` and
`MissilePhase::ends` do not exist; `wp1-red-on-wp0.log`). Behaviourally, on a scratch copy of WP1's tree with each new
rule switched off (a `Remaining` end never due, no release check, no D-4 end rule, unknown fields read, both variants
encoded with tag 0; `neuter_wp1.py`): M-2, M-3, M-4 and M-7 fail in `missile_mover_tests.rs` and `missile_flight.rs`,
M-5 in `validation.rs` (the end rules and the stale `ends_after`), M-6 in `checksum_state_tests.rs` and `encoding.rs`
(`wp1-red-neutered{,-2,-3}.log`), while every weapon-delivery test (M-1) passes. On WP1's tree all pass.

**Latent proof** (`proofcopy.sh wp1-latent ecdaaea9 unhash.py`): a `git archive` copy of WP1, every file touched, the
schema set back to 57 with the new encoding of a phase end kept, built in a fresh target directory: both goldens and
`DEFAULT_SANDBOX_CHECKSUM` reproduce `main`'s recorded values on native debug and release and `wasm32-wasip1` release
and debug, 8 of 8. No golden world encodes a missile phase (WP0's probe), so the schema number is the only difference.
Stop rule 1 holds. The schema then steps to 60 (provisional); WP1 to WP5 leave the goldens failing by design.

**Worlds** against WP0's: every output file byte-identical (490 files, 0 differing): every matrix battle, battles 31
and 33, the showcase, showcase effects and skirmish worlds.

## WP2: the High Templar analogue (`51c47c2a`)

**Red first** on WP1's tree (`red/wp2-red.log`): M-8 fails on the High Templar analogue (1.0, 5.0.14 1.05) and M-9 on
its speed. M-8's table (`catalog_reference_values_tests.rs`) cites, for every shipped missile weapon without movers,
the 5.0.14 mover its launch flies (`census/census_m8.txt`: `MissileDefault` and its children 1.05; Marauder 1.12;
Hydralisk and Queen 2.24; Photon Cannon 1.008), the Brood Lord analogue's two rows `known_bug: "B-232"`, and the four
rows of this plan's later packages `known_bug: "B-52"` until each lands. Green after the data.

**Worlds** against WP1's: every output file byte-identical, outcomes included. The per-unit traces of the three
probe-listed worlds (`scripts/trace.sh`, the 20 runs of each; `scripts/earlier.py`) give each first difference:

| Battle | Runs differing | First difference | Class |
|---|---:|---|---|
| 13_storm_vs_held_lings | 20 of 20 | tick 18 on every run: the bolt released on tick 13 (edge 5.85) strikes a zergling one tick earlier (6 ticks, not 7), its life 15.276 → 11.276 instead of on tick 19; the worlds agree again from tick 19 | D |
| 36a_storm_turns_marines | 8 of 20 | a red unit's life lower by the bolt's 4 one tick earlier, WP2's value at the tick equal to WP1's the next tick; equal again from the next tick | D |
| 36b_marines_beat_zealots_without_storm | 15 of 20 | the same on 14 runs; on seed 2 the bolt (21 → 17) lands a tick before a second hit WP1 lands with it (21 → 9); equal again from the next tick | D |

No outcome moved: the matrix summaries (end ticks, survivors, shots) are identical. Every other world is identical.

## WP3: the Viking analogue (`87800150`, restatement `06a6585d`)

**M-14 first** (green on WP2's tree): every analogue finder of `missile_mover_tests.rs` selects one archetype by a
property unique to it, through `one_archetype`, which fails unless exactly one shipped archetype matches; the template
finder `missile_archetype` takes a constant missile only. Red-proof (`red/m14-proof-{a,b}.log`): with the Viking
analogue given an unbounded mover, the finders stay unique (green); given a bounded first band like the Phoenix's,
`every_finder_matches_exactly_one_shipped_archetype` fails naming both. The finders of `weapon_catalog_rules_tests.rs`
pick "any air unit" and "any ground unit without an air weapon" for tests that need any such unit; no mover data can
change what they test, so they are left as they are.

**Red first** on WP2's tree with M-14 (`red/wp3-red.log`): M-11's Viking row, M-12, M-15, M-16, M-17 and the finder
test fail. With the data, both torpedoes of a volley land on §2.4's tick after their own release at every listed
distance, with no boundary tick: edge 0.5, 2, 4, 6, 8, 9 → 9, 12, 14, 15, 16, 17 (stop rule 10 holds).

**Worlds** against WP2's: only `matrix.csv` and `matrix.md` differ, in the three probe-listed battles; the showcase,
showcase effects, skirmish, battles 31, 33 and every other matrix battle are identical.

| Battle | Runs differing | Winner changes | Expectation changes | First difference (every run) | Class |
|---|---:|---|---|---|---|
| 12c_viking_vs_colossus | 20 of 20 | none | none | tick 6: the torpedo released on tick 2 struck the Colossus's shields after its old 5-tick flight; now on tick 16 | D |
| 19_viking_vs_brood_lord | 20 of 20 | none | none | tick 5: the torpedo released on tick 2 struck the Brood Lord after 4 ticks; now later | D |
| 35_12_vikings_vs_6_colossi | 20 of 20 | 2 (first seed of record, seed 4) | none | tick 6: the eight front vikings' torpedoes released on tick 2 no longer strike colossi 13, 15 and 17 | D |

Mean end ticks: 12c 480 → 490, 19 300.6 → 310.6, 35 840 → 900 (all runs of 35 last the 900 ticks).

**Battle 35, restated under charter §8** (on the frozen list; class D). WP2's 20 runs: 2 blue wins (all six colossi
killed: the first seed of record on tick 267, seed 4 on tick 333) and 18 draws; WP3's: 20 draws, the first seed of
record leaving 2 colossi and seed 4 one; the expectation (red 0 to 3) holds on every run. Traced on the first seed of
record: on tick 2 eight vikings release, the single unbounded band (`VikingFighterMissile`), each torpedo in its throw
phase at 0.112 per tick until its 7th step (tick 8) and then accelerating; in WP2 the constant 1.5 struck colossi 13, 15
and 17 on tick 6 (their shields 100 → 86, 72, 86); in WP3 the same strikes land on tick 16. The probe on battle 35
(`probe/probe35.sh`, WP2 against WP3): torpedoes lost on the last point when their colossus leaves the vikings' sight
rise from 140 to 619 over the 20 runs, and on the first seed of record from 4 to 31, its kills from 6 to 4. Source:
`liberty` MoverData:531–567 (S3). The claim and comment of battle 35 in `matchup_matrix.ron` carry the moved share and
this trace. The claim itself still holds and is kept (held-out seeds still fell all six on some runs, WP6).

## WP4: the Liberator and Banshee analogues (`53fb8665`, S-4 `4c94f740`)

**Red first** on WP3's tree (`red/wp4-red.log`): their M-11 rows, M-12 cases and finders fail. With the data, every
listed distance lands on §2.4's tick: Liberator edge 0.5, 1, 2, 3, 4, 5 → 3, 4, 7, 8, 9, 10; Banshee 0.5 to 6 → 3, 3,
5, 6, 7, 8, 9. A strike is counted as a drop of life plus shields: the Banshee's ground victim template regenerates.

S-4 (`combat_correctness_tests.rs`) picks the first shipped multi-strike missile `weapon`, the Banshee analogue's, and
states that both missiles fly the same distance; its victim cannot hit back and walked off between the releases, and
with the accelerating rockets the second landed a tick past one interval. The victim now holds, so its premise holds.

**Worlds** against WP3's: every output file byte-identical, as WP0's probe predicted (no release of either weapon).

## WP5: the Missile Turret analogue (`6524a75d`)

**Red first** on WP4's tree (`red/wp5-red.log`): its M-11 row, M-12, M-13, M-15, M-17 and finder fail. With the data,
every listed distance lands on §2.4's tick over the Turret's radius 1.125: edge 0.5 to 7 → 2, 3, 4, 5, 7, 8, 10, 12.
M-13: at a target holding 6 away, and at one flying at the Turret from beyond its range (released inside 7), the
missile is in its kill phase exactly on the ticks after whose step at most 4 is left, and the approaching target is
struck earlier than a still target at its release distance. M-15 removes the copy's detection: the Turret analogue
detects, as 5.0.14's does, so a cloaked target is otherwise not lost. M-17: a kill phase starting 1 from the target
instead of 4 lengthens the flight at edge 6 in the same binary.

**Worlds** against WP4's: every output file byte-identical, as predicted.

## WP6: confirmation, re-record, records

**Merge.** `main` had not moved from `4ff8402f` (checksum schema 57; Q0130 and Q0131 open, unrelated), so the final
tree is the branch head; the Gated plan has not taken 58 or 59, and 60 stays provisional, the next free at merge.

**Held-out seeds** (§7.3, `scripts/heldout.sh`, `main` `4ff8402f` against the final tree): matrix seeds 7001 to 7016,
battles 31 and 33 on 7001 to 7064, the showcase and showcase effects on seeds 9 to 16, the skirmish on 9 to 12. Only
`matrix.csv` and `matrix.md` differ, in 12c, 19 and 35 (Viking) and 36a (High Templar), each listed by WP0's probe for
that weapon; battles 31 and 33, the showcase, showcase effects and skirmish worlds are identical. First differences
(per-unit traces of the four battles on the 16 seeds): 12c, 19 and 35, every seed, the torpedo released on tick 2
striking on tick 6 (or 5) on `main` and later on the final tree; 36a, 6 seeds, a bolt's 4 landing one tick earlier,
the worlds equal again on the next tick. All class D. Outcomes: 12c and 19 unchanged; 36a unchanged (one run ends a
tick earlier); 35 changes winner on 4 of 16 seeds in both directions (`main` kills all six on 7008 and 7012, the final
tree on 7011 and 7014), a close battle restated in WP3, class D. No expectation changes. Stop rule 3 holds; nothing is
restated or tuned from these seeds.

**Throughput** (§5, `scripts/wp6-measure.sh`): WP0's binary and the final tree's, interleaved, two rounds each, under
the gate lock (load average 6 to 18). Every scenario at 80 ticks per second or more in both (final: 64x64 0 units 3103
and 3095, 100 units 810 and 796, 400 units 109 and 109; 128x128 180 and 178; 256x256 192 and 194; `showcase_canyon` 82
and 82; 67 casters 201 and 201; 12 hangar units 392 and 391). User-space instructions of the whole run: WP0
297,769,229,751 and 297,769,227,803; final 297,774,340,151 and 297,774,339,776, +0.0017 %. Stop rule 5 holds.

**Goldens re-recorded** once on the final tree (`bca70ca5`), schema 57 -> 60: both goldens' checkpoint lists and
`DEFAULT_SANDBOX_CHECKSUM` (`10_676_883_327_660_124_161` -> `14_163_679_094_600_060_237`). Proved on four builds, each
a `git archive` copy with every file touched, in a fresh target directory, `wasmtime --dir=<copy>`: the recorded tree
passes `golden_replay_tests` and `fixture_tests` on native debug and release and `wasm32-wasip1` release and debug, 8
of 8 (`proof-final`); and its latent form (`unhash_final.py`: the schema back at 57 with `main`'s goldens and sandbox
value) reproduces every checkpoint of `main` on the same four builds, 8 of 8 (`proof-final-latent`). No weapon of the
plan fires in these worlds.

**Client** (M-18): `effect_catalog_tests`, `sync_steady_state_tests` and `sync_allocation_count_tests` (valgrind) pass
unchanged. `weapon_look_tests` W-20 assumed every mover weapon names an impact look; the Liberator analogue's look
draws its landing with a splash (`flak_burst`), whose event the simulation publishes at the landing, so the test now
requires a splash and no impact spawn for such a look (`test(client)` commit, no `client/src` change). The steady-state
and allocation tests run no Viking volley: the default battle has no Viking analogue and the running children are
defined as firing nothing, so a Viking volley would need a new staged window; not added (see the plan's As built).
