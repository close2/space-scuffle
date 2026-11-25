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

## WP1: missile movers, latent (`16934087`)

Red first (`~/scratch/weapon-delivery/red/`): on WP0's tree with only WP1's tests, `sim_types`'s lib test fails to
compile (no `MissileMover`, `MissilePhase`, no field `missile_movers`; `wp1-red-sim_types.log`), and so do `sim_core`'s
lib test, `checksum_state_tests`, `hidden_harvester_tests` and the new `missile_mover_tests` (no field `mover`, `phase`,
`travelled` on `ActiveProjectile`, no `MissileMoverDef`; `wp1-red-sim_core.log`, built on a `git archive` copy of WP0
with the test files overlaid). Green after the change: W-1 to W-4, W-7 to W-9 (`missile_mover_tests.rs`: a constant
missile's position and landing tick equal the old step's on every tick; one phase lands on the first tick whose
`v₀k + ak²/2` reaches the path; a phase switches on the first tick whose path reaches its end, restarting or carrying
the speed; the band is chosen inclusively at the release and kept when the target closes; a mover missile fuels out on
the tick a constant one does; a lost target's missile flies its profile to the last point; the event's flight time is
the integration's), the `missile_flight` unit tests (W-1's `ceil(path / speed)` over every shipped speed and a table
of paths, W-2 to W-4), W-5 (`validation.rs`, every D-4 rule refused naming its field) and W-6 (`checksum_state_tests.rs`).

**Worlds.** WP1's worlds (binaries and assets of the commit) against WP0's: `compare.sh` finds 490 files, none
differing.

**Latent proof (§5, stop rule 1).** A `git archive` copy of `16934087` whose encoding lists `missile_movers` and the
projectile's `mover`, `phase` and `travelled` under `unhashed` and whose schema is 53
(`~/scratch/weapon-delivery/scripts/unhash.py`), each build in a fresh target directory (`proofcopy.sh wp1-latent
16934087 unhash.py`), reproduces every recorded checkpoint of both goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug
and release and on `wasm32-wasip1` release and debug: 8 of 8. The committed tree hashes the fields at schema 56
(provisional; 54 and 55 are reserved for the Gated plan), so its golden and sandbox tests fail by design until WP4.

## WP2: the data (`0cf9656b`)

Red first on WP1's tree (`red/wp2-red.log`): W-10 (`flying_saucer`'s Disruptor Beam instant where 5.0.14 launches a
missile), V-19 (the Carrier's salvo `Snapshot`), W-11 to W-16 (no shipped weapon with movers or a hidden launch), W-17
and W-18 fail. All pass after the data. The measured flight ticks equal §2.3's table at every distance: the Phoenix
analogue 2, 8 and 10 ticks at edge 0.5, 3 and 5 (release tick counted), both strikes on one missile, the reveal on the
landing tick and not before; the Spine Crawler analogue 6, 7, 7, 4, 4, 5 at edge 0.5, 2, 3, 3.5, 5, 7, the short band
the slower. The full `sim_types`, `sim_core` and `sim_fixtures` suites on WP2's data: every test binary green but the
goldens and the sandbox (schema bytes, re-recorded in WP4).

**World check (§6.4, §6.5).** WP2's worlds against WP1's: 490 files, none differing; `matrixdiff.py` lists nothing. No
first difference exists, so no world is classed; no outcome moved, and no close battle is restated (stop rules 2 and 4
hold). The prediction of §6.6 holds: no measured world fires the three weapons (WP0's probe).

## WP3: client looks (`c788b407`)

No `client/src` file changes (stop rule 7 holds). Red on WP2's tree (`red/wp3-red.log`):
`every_shipped_weapon_names_the_looks_of_the_event_it_publishes` (`flying_saucer Air None Projectile: prism_beam does
not fly the path`) and W-20 fail. Green after the data: W-19 (`effect_catalog_tests`, 41 tests, `impaler_tentacle`
validates), W-20 (`weapon_look_tests`). W-21: `sync_steady_state_tests` (13) and `sync_allocation_count_tests` (10
children under valgrind) pass unchanged on WP3's data.

**Deviation (W-21).** A Phoenix-analogue volley cannot be staged in the client harness: `flying_saucer` has no
flipbook media, and the client refuses to present a unit without it (the visibility plan met the same limit for its
spellcasters). Its travel, `plasma_bolt`, is the one seven shipped missile weapons already fly in those tests. Other
deviations: W-13's air target has no "loaded" case (an air unit boards no transport; the ground target covers all
five); W-14 clears the random delay on a scratch copy so the period is exact, and the victims hold position.

## WP4: re-record, final worlds (`3518b58d`)

**Merge.** `main` `39df7177` (docs only: Q0129 answered (a)) merged as `1943995d`. **Latent proof on the merged tree:**
a fresh-target copy of `1943995d` with the fields unhashed at schema 53 reproduces main's goldens and
`DEFAULT_SANDBOX_CHECKSUM` (`1_324_710_836_022_737_298`) on native debug and release and `wasm32-wasip1` release and
debug, 8 of 8: neither the mechanism nor the data moves any hashed state of the goldens' worlds.

**Re-record (§5).** `golden_replay.ron`, `golden_replay_orders.ron` (schema 53 → 56, with the plan's header) and
`DEFAULT_SANDBOX_CHECKSUM` (now `6_457_533_297_407_649_319`) re-recorded once on the final tree; a fresh-target
`git archive` copy of `3518b58d` passes both goldens and the sandbox on the four builds, 8 of 8 (`proofcopy.sh
final-recorded 3518b58d`).

**Final worlds against WP0's** (WP0's tree is `main` `c3c372c6` plus docs): 490 files, none differing; `matrixdiff.py`
lists nothing.

**Held-out (§7.3, stop rule 3).** Matrix seeds 7001–7016, battles 31 and 33 on 7001–7064, the showcase and showcase
effects on seeds 9–16, on WP0's binaries and assets (`main` `c3c372c6`'s worlds) and on the final tree's
(`~/scratch/weapon-delivery/scripts/wp4-all.sh 3518b58d`): 381 files, none differing. Stop rule 3 holds; nothing is
restated or tuned from them.

**Throughput (§5, stop rule 5).** `cargo bench -p sim_core --bench throughput` binaries of WP0 (`dff44e48`) and the
final tree, run interleaved, two rounds each, with `perf stat -e instructions:u`
(`~/scratch/weapon-delivery/bench-{wp0,final}-r{1,2}.{log,perf}`):

| Scenario | WP0 (r1, r2) ticks/s | final (r1, r2) ticks/s |
|---|---|---|
| 64x64, 0 units | 3066, 3021 | 3031, 3100 |
| 64x64, 100 | 784, 790 | 720, 796 |
| 64x64, 400 | 100, 72 | 101, 91 |
| 128x128, 400 | 162, 170 | 147, 151 |
| 256x256, 0 | 189, 181 | 188, 177 |
| showcase_canyon, 400 | 74, 65 | 72, 51 |
| 64x64, 67 casters, 400 | 182, 136 | 191, 192 |

The machine was heavily loaded during both rounds (load average 21 to 55 on 16 cores: another agent's measurement
harness, and this plan's `cargo gate quick` overlapping round 1), so the ticks per second scatter by up to 30 % between
rounds of the same binary (WP0's 64x64/400: 100 then 72). Every scenario still runs at 51 ticks/s or more (≥ 50). The
instruction count is the load-independent measure: WP0 275,790,339,203 and 275,790,351,766; final 270,019,223,948 and
270,019,233,867: **−2.09 %**, well inside +0.5 %. Stop rule 5 holds. No bench scenario fires a weapon with movers, so
the mover path costs nothing there; the decrease is not traced to a cause here (the two trees differ only by this
plan's code and data and a docs-only merge of `main`), and it is not a figure this plan relies on.
