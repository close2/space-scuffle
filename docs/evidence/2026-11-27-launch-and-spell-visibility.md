# Evidence: What a Missile's Launch and a Spell's Damage Show of Their Caster (B-205, B-206)

Measurements of plan [`2026-11-27-launch-and-spell-visibility`](../plans/2026-11-27-launch-and-spell-visibility.md):
the census, the probes, the baseline and the close-battle list of WP0 (§6.5, §7.1), the latent proof of WP1 (§5),
the world checks of WP1 and WP2 (§6.5, §6.6), the held-out run, throughput and the golden re-record of WP4 (§5,
§7.3). Scratch material (trees, binaries, worlds, the probe patch and its summary, the census scripts, the red runs)
is under `~/scratch/launch-spell-visibility/`, never committed.

## WP0: census, probes, baseline and close battles

WP0's tree is the branch head `7ce0de3d` (`main` `a0384b78`, schema 49; the two commits above `main` are the plan and
the filing of Q0128).

### The `Visibility` survey (§2.1 S4), re-run

Game data `5444e12f13` (5.0.14.94342) under `~/scratch/combat_wp3_data/<layer>/`. Every `<Visibility` line of every
layer's `EffectData.xml`, mapped to its enclosing top-level effect
(`~/scratch/launch-spell-visibility/census/visibility_survey.txt`):

| Class | `Snapshot` | `Visible` | `Hidden` |
|---|---:|---:|---:|
| `CEffectDamage` | 1 (the default, `core` :62) | 46 | 4 (`PsiStormDamage` `liberty` :2099, `KillRemove` `swarm` :1296, `90mmCannonsDummy` `voidmulti` :1195 and `balancemulti` :1070) |
| `CEffectLaunchMissile` | 1 (the default, `core` :102) | 5 (`CorruptionLaunchMissile` `liberty` :672, `FrenzyLaunchMissile` :2946, `ContaminateLaunchMissile` :2976, `EyeStalk` `swarm` :2632, `Doom` :2642) | 0 |

No other effect class carries the field. The plan counts 51 damage overrides (47 `Visible`); this run finds 50 (46
`Visible`). The difference is one `Visible` damage of a spell or an air-plane weapon, none of which the plan's
findings rest on: the four `Hidden` damages, the five launch overrides (all spells) and both defaults are as §2.1
states, so no weapon launch is overridden.

### Census (§7.1 (c))

`~/scratch/launch-spell-visibility/census/sc2.py` resolves a 5.0.14 weapon across the eight layers (last layer wins
per field; a weapon that names no `Effect` takes the `CWeaponLegacy` default `##id##`, `core` WeaponData:37) and
follows its effect through sets, persistents' initial and periodic effects and switches. Each shipped weapon is
matched to the weapon of its `sc2_analogue` (or `sc2_inspiration`) unit's `WeaponArray`.

**The 25 missile weapons** (`projectile_speed` set; 21 unit weapons, 4 structure weapons):

| Ours | 5.0.14 weapon | Its effect | Launch |
|---|---|---|---|
| `rhino_beetle` Acid Saliva | `AcidSaliva` | `AcidSalivaLM` | `CEffectLaunchMissile` |
| `shock_golem` Concussive Shells | `PunisherGrenades` | `PunisherGrenadesLM` | `CEffectLaunchMissile` |
| `vector_interceptor` Sky Torpedo Volley | `LanzerTorpedoes` | persistent → `LanzerTorpedoesLM` | `CEffectLaunchMissile` |
| `sky_bastion` Lexington Rockets | `LiberatorMissileLaunchers` | persistent → `LiberatorDamageMissileLM` | `CEffectLaunchMissile` |
| `cloak_gunship` Backlash Thermite Rockets | `BacklashRockets` | persistent → `BacklashRocketsLM` | `CEffectLaunchMissile` |
| `warp_strider` Particle Disruptor (ground, air) | `ParticleDisruptors` | `ParticleDisruptors` | `CEffectLaunchMissile` |
| `astral_vagrant` Glaive Cannon | `Adept` | `AdeptLM` | `CEffectLaunchMissile` |
| `cerebral_elder` Psi-Bolt | `HighTemplarWeapon` | `HighTemplarWeaponLM` | `CEffectLaunchMissile` |
| `mothership_carrier` Interceptor Fleet Salvo | `InterceptorLaunch` (Carrier); `MothershipBeam` (Mothership) | persistent → `CarrierInterceptor`; set of beam persistents | **none** (`CEffectUseMagazine`; instant beam): B-227 |
| `mortar_stag` Corrosive Bile | `RavagerWeapon` | `RavagerWeaponLM` | `CEffectLaunchMissile` |
| `thorn_viper` Needle Spines (ground, air) | `NeedleSpines` | `NeedleSpinesLaunchMissile` | `CEffectLaunchMissile` |
| `sonic_bat` Glaive Stinger (ground, air) | `GlaiveWurm` | `GlaiveWurm` | `CEffectLaunchMissile` |
| `sky_leviathan` Parasitic Spores | `ParasiteSpore` | `ParasiteSporeLaunchMissile` | `CEffectLaunchMissile` |
| `spore_matriarch` Broodling Swarm (two rows) | `BroodlingEscort` | switch → set → `BroodlingEscortLaunchA` | `CEffectLaunchMissile` |
| `hive_matron` Matron Talons (`ground_weapon`, `weapon`) | `TalonsMissile` | persistent → `TalonsLM` | `CEffectLaunchMissile` |
| `hive_matron` Acid Spores | `AcidSpines` | `AcidSpinesLM` | `CEffectLaunchMissile` |
| `flak_array_dome` Longbolt AA Missiles | `LongboltMissile` | persistent → `LongboltMissileLM` | `CEffectLaunchMissile` |
| `tesla_sentinel_dome` Phase Discharge, Phase Discharge AA | `PhotonCannon` | `PhotonCannonLM` | `CEffectLaunchMissile` |
| `spore_spire_dome` Spore Volley | `AcidSpew` | `SporeCrawler` | `CEffectLaunchMissile` |

The Roach's and the Queen's close-range halves (`RoachMelee` 0–0.5, `Talons` 0–3, both instant) are folded into the
missile weapon from range 0, as plan `2026-11-05-sc2-deviation-fixes` recorded (class A). The game's instant strike
there leaves its snapshot on the same tick our release does, so the image differs in nothing.

**The instant weapons** match 5.0.14 except two: `welder_drone` (`FusionCutter`), `pulse_bot` ×2 (`GuassRifle`),
`psi_grey` (`PsiBlades`), `railgun_crawler` (`90mmCannons`; siege `CrucioShockCannon`, a switch to damages),
`jet_jumper` (`P38ScytheGuassPistol`), `specter_android` (`C10CanisterRifle`), `napalm_wheeler`
(`InfernalFlameThrower`), `colossus_mech` (`ThorsHammer` ×2, `LanceMissileLaunchers` →
`LanceMissileLaunchersDamage`), `grav_harvester` (`ParticleBeam`), `prism_weaver` (`DisruptionBeam`),
`shadow_visitor` (`WarpBlades`), `void_goliath` (`PhaseDisruptors`), `tripod_annihilator` (`ThermalLances`),
`prismatic_saucer` ×2 (`PrismaticBeam`), `nectar_worker` (`Spines`), `razor_mantis` (`Claws`), `volatile_tick`
(`VolatileBurst`), `burrow_scorpion` (`LurkerMP`, a persistent line search), `stag_behemoth` (`KaiserBlades`) and
`planetary_bulwark_dome` (`TwinIbiksCannon`) launch no missile in 5.0.14 either. **The two exceptions:**
`flying_saucer` Disruptor Beam is instant, but the Phoenix's `IonCannons` launches two missiles (`IonCannonsLMLeft`,
`IonCannonsLMRight`, `liberty` EffectData:1972, :1977); `thorn_lasher_dome` Impaler Spine is instant, but the Spine
Crawler's `ImpalerTentacle` launches `ImpalerTentacleLM` (`liberty` WeaponData:365, EffectData:2151).

**Three delivery mismatches, so B-227 is registered** (`DESIGN.md` §4), deferred under the plan's §14: a delivery
change moves flight times and outcomes. The rows keep the default; the launch image is missing for `flying_saucer`
and `thorn_lasher_dome` and shown where the game shows none for `mothership_carrier`.

**The spell nodes** (`ability_effects.ron`), against their 5.0.14 effects:

| Node | 5.0.14 effect | `Visibility` there | Ours after WP1 |
|---|---|---|---|
| `psionic_storm_damage_initial` (`Damage`) | `PsiStormDamageInitial`, `liberty` :2089–2096 | none: `Snapshot` | `Snapshot` (default) |
| `psionic_storm_damage` (`Damage`) | `PsiStormDamage`, `liberty` :2097–2101 | `Hidden` (:2099) | `Hidden` (authored) |
| `fungal_growth_search`'s inline `Damage` | `FungalGrowthSearchDummy`, `liberty` :736–741 (void :221, voidmulti :407, balancemulti :321) | none: `Snapshot` | `Snapshot` (default) |
| `fungal_growth_damage` (`Damage`) | `FungalGrowthDamage`, `liberty` :603–607 (and five later layers) | none: `Snapshot` | `Snapshot` (default) |
| `abduct_tongue` (`LaunchMissile`, unit) | `YoinkLaunchMissile`, `swarm` :1486–1494 | none: `Snapshot` | `Snapshot` (default) |
| `fungal_growth` (`LaunchMissile`, point) | `FungalGrowthLaunchMissile`, `void` :1141–1146, `swarmmulti` :47 | none: `Snapshot` | `Snapshot` (default; a point launch records nothing) |

None of these effects names a parent that sets `Visibility`.

### Probes (§7.1)

The scratch probe (`~/scratch/launch-spell-visibility/probe/probe.rs`, applied to a copy of the tree by
`apply_probe.py`, logged per world on standard error; it may name slugs, being scratch) evaluates the record rule of
`record_attack_snapshot` (another player's victim; caster alive and unhidden; not `shown_to` the viewer; a concealed
caster only where detected) at each weapon missile release, spell `Damage` execution and spell launch, with the 5.0.14
visibility of the census, and changes no state: its worlds are byte-identical to WP0's (`compare.sh`: 483 files, no
output differing; the 11 log differences are its own lines). Counts are record-time: a spell's test reads the
vision of the previous tick, and the same tick's vision update may prune the image before any view lists it (D-2).

| World | (l) launch images, new (refresh) | ticks gained by new ones | (a) air-plane | (s) spell images | (k) spell launch images | (v) reveals |
|---|---|---:|---:|---|---:|---:|
| Matrix, every battle but 13, 35, 38a (seeds of record and 1–16) | 0 | 0 | 0 | 0 | 0 | 0 |
| Battle 13 (storm on held zerglings) | 0 | 0 | 0 | 160 new, `psionic_storm_damage_initial` (20 seeds); the 480 `Hidden` pulses admit none | 0 | 0 |
| Battle 35 (vikings vs colossi) | 242 new (0) | 1,455 (6 per image) | 242 | 0 | 0 | 0 |
| Battle 38a (Fungal on zerglings) | 0 | 0 | 0 | 564 new, `fungal_growth_damage` (9 seeds); the impact dummy's 200 admit none | 0 (20 point launches) | 0 |
| Battle 37a (Abduct on a sieged tank) | 0 | 0 | 0 | 0 | 0 (20 unit launches, caster seen) | 0 |
| Battles 31, 33 | 0 | 0 | 0 | 0 | 0 | 0 |
| Showcase, default seed and 2–6 | 0 | 0 | 0 | 0 | 0 | 0 |
| Showcase seed 1, 7, 8 (viewer 0, `rhino_beetle`) | 4, 2, 3 new | 16, 8, 12 | 0 | 0 | 0 | 0 |
| Showcase effects, default and 1–8 | 0 (no release, spell damage or launch) | 0 | 0 | 0 | 0 | 0 |
| `golden_replay` | 0 (28 releases, all seen) | 0 | 0 | 0 | 0 | 0 |
| `golden_replay_orders` | 0 (no release) | 0 | 0 | 0 | 0 | 0 |

**Probe (v) counts zero in every world** (stop rule 9 holds). The showcase ticks that will gain images, all launch
images for player 0: seed 1 ticks 568, 605, 640, 678; seed 7 ticks 578, 613; seed 8 ticks 581, 617, 653. The default
seed, which the `battle` take renders, gains none.

### Baseline on four builds (§7.1)

A `git archive` copy of `7ce0de3d`, built in a fresh target directory per run
(`~/scratch/launch-spell-visibility/scripts/proofcopy.sh wp0 7ce0de3d`, `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime
--dir=<copy>"`): `golden_replay_tests` (every recorded checkpoint of `golden_replay` and `golden_replay_orders`) and
`fixture_tests` (`DEFAULT_SANDBOX_CHECKSUM`) pass on native debug and release and on `wasm32-wasip1` release and debug,
8 of 8 runs green (schema 49).

### The close-battle list (§6.5), fixed before WP1

A run is won by the side with survivors while the other side has none; otherwise it is a draw. A battle is close when
the side that wins fewer of its baseline runs wins at least 2 of its 20 (battles 31 and 33: the same share of their
100 and 64 seeds). Derived from WP0's worlds alone by `python3 -I ~/scratch/launch-spell-visibility/scripts/close.py
~/scratch/launch-spell-visibility/world-wp0`:

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
and 38b. **The list: 14, 26, 31, 33, 35, 38a, 38b.** It is not edited after WP1 starts. Any outcome move, on the
list or not, is stop rule 4 (§6.5).

The matrix plays 1,220 runs: 2 expectation misses on the seeds of record (battle 28, `known_bug` B-192) and 26 over
all seeds, as on `main`. The trace binaries and the throughput binary of this tree are kept under
`~/scratch/launch-spell-visibility/bin-wp0/` and `bench-bin-wp0/` for the comparisons of WP1, WP2 and WP4.

## WP1: the data, latent (`d52957e3`)

Red first: on WP0's tree with only WP1's tests applied, `sim_types` fails to compile (7 errors: no field
`launch_visibility` on `WeaponSpec`, no field `visibility` on `DamageDef` or `LaunchMissileDef`) and so does every
`sim_core` target that names the fields (`lib test`, `catalog_reference_values_tests`, `checksum_state_tests`,
`weapon_catalog_rules_tests` and the eight test files whose `WeaponSpec` literals list every field;
`~/scratch/launch-spell-visibility/red/wp1-red-sim_types.log`, `wp1-red-sim_core.log`). Green after the change:
V-17 (`spell_visibility_resolves_and_a_zero_reveal_is_refused`,
`a_launch_visibility_needs_a_missile_and_a_visible_one_a_tick`,
`an_instant_weapon_or_a_zero_reveal_authoring_launch_visibility_is_refused`), V-18
(`a_projectiles_launch_visibility_is_hashed_and_a_spell_nodes_is_not`, and `launch_visibility` in
`embedded_weapon_fields_and_splash_are_sensitive`), V-19 (`launch_visibility_follows_starcraft`,
`shipped_spell_visibility_follows_starcraft` and the six shipped nodes' citations in the storm, Abduct and Fungal
reference tests), and the `sim_types` RON forms.

**Worlds.** WP1's worlds (binaries built from the commit, assets from its `git archive`) against WP0's: `compare.sh`
finds 483 files and none differing; `matrixdiff.py` lists nothing.

**Latent proof (§5, stop rule 1).** A `git archive` copy of `d52957e3` whose encoding lists `launch_visibility`
under `unhashed` and whose schema is 49 (`~/scratch/launch-spell-visibility/scripts/unhash.py`), each build in a
fresh target directory (`proofcopy.sh wp1-latent d52957e3 unhash.py`), reproduces every recorded checkpoint of both
goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release and on `wasm32-wasip1` release and debug: 8 of 8
runs green. Stop rule 1 holds. The committed tree hashes the field at schema 53, so its golden and sandbox tests fail
by design until WP4 re-records them.

## WP2: the rule (`99bf0be7`)

Red first on WP1's tree (`~/scratch/launch-spell-visibility/red/wp2-red-integration.log`,
`wp2-red-unit-runtime.log`): of V-1 to V-16 in `launch_and_spell_visibility_tests.rs`, 11 fail (every new image or
reveal is missing: V-1, V-4, V-6, V-7, V-8, V-9, V-10, V-11, V-12, V-14, V-16); V-2, V-3, V-5, V-13 and V-15 pass
before and after, as the guards of what the rule must not record. V-20 (`runtime_catalog_tests.rs`) fails (`[0, 0]`
for `[1, 0]`), V-22 (`world/snapshot.rs`) fails its premise (no image of the Fungal caster). V-21 was written
against the weapon-hit path and passed there (`wp2-v21-old-path.log`): its table is the old behaviour; after the
change the same table holds for every cause (strike, launch, spell). All pass after the change, with the effect
engine's valgrind count (`effect_tree_allocation_tests`) still zero.

**World check (§6.5).** WP2's worlds against WP1's: 483 files, none differing; `matrixdiff.py` lists nothing. No first
difference exists, so no world is classed R or other; no outcome moved (stop rules 2 and 4 hold).

**Class P (§6.6), measured.** A scratch view probe (`~/scratch/launch-spell-visibility/viewprobe/`) logs every
player's listed images whenever they change, on the showcase and the showcase effects scenario (default seed and
1–8, 6,000 ticks), on WP1's and WP2's trees. Their route outputs are byte-identical; the image lists differ only by
launch images for player 0, exactly at the ticks probe (l) predicted in WP0: seed 1 ticks 568, 605, 640, 678; seed 7
ticks 578, 613; seed 8 ticks 581, 617, 653 (each a `rhino_beetle` release, its impact refreshing the image 4 ticks
later as on `main`). The default seed, which the `battle` take renders, and every showcase effects world list the
same images as before.

**Deviations.** V-22 extends P-9 as a unit test in `world/snapshot.rs`, since clearing images needs the crate's
test hook; V-15's reveal half (a dead caster raises no reveal) is V-21's dead-attacker row for every cause, since a
dead caster is reaped at the end of its tick; a loaded caster is likewise covered by V-21's hidden-attacker row
(`transport_carrier` is crate-private). `crates/gate/suites.ron` needs no entry: the new file is a `fast` target
(unlisted targets are fast), selected by every `sim_core` change and by the catalog rule.

## WP3: client verification

No `client/src` file changes (stop rule 7 holds). Red: with WP2's three new calls removed from a scratch state of
the tree, V-23 and V-24 (`battle_fog_tests.rs`: `a_launch_image_is_drawn_by_the_pool_before_its_missile_lands`,
`a_spell_casters_image_is_drawn_by_the_pool`, `a_launch_image_is_never_picked_or_targeted`) and V-25's spell case
fail (`~/scratch/launch-spell-visibility/red/wp3-red.log`); V-25's launch case was then made to prove its image is the
release's (listed while the missile flies). Green on WP2's tree: the five new client tests, the existing snapshot
tests (P-13 to P-16), and `sync_allocation_count_tests` under valgrind (10 of 10), whose running children now hold
two images, a strike's and a spell caster's, and count no allocation in the live battle sync.

**Deviations.** No shipped spellcaster has flipbook media yet, and the client refuses to present a unit without it
(`cannot read media .../cerebral_elder.flipbook.ron`), so the stage's storm is cast with a soldier that has media as
the walk's caster: the image shows the caster whatever made it (D-3), so the pool is tested the same. V-25's launch
case warms up 400 ticks after the attacker is ordered to hold: with the default 30, one bubble material was modified
once early in the window (a live unit settling after its target became invulnerable; the listed image was unchanged
throughout, so it is not the pool's).

## Merge of `main` 2f922a50 (follow-order, schema 50)

Merged as `89bfb0e8`; the conflicts were the schema line and constant (53 kept after main's 50), the encoding tests'
schema bytes beside the follow order's new tests, and the bug rows. **Latent proof re-run against main's schema-50
values:** a `git archive` copy of `89bfb0e8` with `launch_visibility` unhashed and schema 50 reproduces main's
re-recorded checkpoints of both goldens and its `DEFAULT_SANDBOX_CHECKSUM` (`13_849_945_286_125_516_044`) on native
debug and release and `wasm32-wasip1` release and debug, 8 of 8 (`proofcopy.sh merged-latent 89bfb0e8 unhash.py`).
That tree holds the rule of WP2 as well, so neither the data nor the rule moves any hashed state of main's goldens.

## WP4: re-record, held-out, throughput

**Re-record (§5), `a04d71ce`.** `golden_replay.ron`, `golden_replay_orders.ron` (schema 50 → 53, with the header
the plan prescribes) and `DEFAULT_SANDBOX_CHECKSUM` (now `1_324_710_836_022_737_298`) re-recorded once on the final
tree; a fresh-target `git archive` copy of `a04d71ce` passes both goldens and the sandbox on native debug and
release and `wasm32-wasip1` release and debug, 8 of 8 (`proofcopy.sh final-recorded a04d71ce`).

**Worlds after the merge.** The final tree's worlds against `main` 2f922a50's (both built from fresh-mtime archive
copies): 483 files, none differing; `matrixdiff.py` lists nothing.

**Held-out (§7.3, stop rule 3).** Matrix seeds 7001–7016, battles 31 and 33 on 7001–7064, the showcase and showcase
effects on seeds 9–16, on `main` 2f922a50 and on the final tree: 381 files, none differing. The comparison base is
`main` rather than WP0's tree because the merge brings the follow order's own changes; this plan's changes are the
only difference between the two trees.

**Throughput (§5, stop rule 5).** `cargo bench -p sim_core --bench throughput` binaries of `main` and the final tree,
run interleaved, two rounds each, with `perf stat -e instructions:u` (`~/scratch/launch-spell-visibility/bench-*`):

| Scenario | `main` (r1, r2) ticks/s | final (r1, r2) ticks/s |
|---|---|---|
| 64x64, 0 units | 3140, 3130 | 3122, 3130 |
| 64x64, 100 | 807, 806 | 802, 796 |
| 64x64, 400 | 108, 107 | 107, 107 |
| 128x128, 400 | 181, 180 | 177, 177 |
| 256x256, 0 | 197, 196 | 196, 196 |
| showcase_canyon, 400 | 82, 82 | 81, 81 |
| 64x64, 67 casters, 400 | 200, 201 | 200, 200 |

User-space instructions of the whole benchmark: `main` 275,777,076,792 and 275,777,196,618; final 275,790,208,649
and 275,790,206,577: **+0.005 %**. Every scenario runs at 81 ticks/s or more (≥ 50). The instruction count is the
whole run's, as the method of `DESIGN.md` §5 takes it; a whole-run +0.005 % bounds every scenario's share far below
+0.5 %. Stop rule 5 holds.
