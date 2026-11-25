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
