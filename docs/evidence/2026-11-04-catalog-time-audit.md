# Catalog time audit: every time and speed value against StarCraft II 5.0.14 (Q0039)

Work package 3 of [`docs/plans/2026-11-04-combat-sc2-alignment.md`](../plans/2026-11-04-combat-sc2-alignment.md)
(§D-7, WP3), answering the audit half of [Q0039](../questions/open/0039-normal-time-values.md) (owner: option 3).

**Status.** Written 2026-11-04 against `main` at `0e5565a`. This file holds the research: steps 1 to 4 and 6 of WP3.
Step 5, the whole-matrix measurement of a scratch catalog with every class B and C correction, runs after WP2 has
merged and will be added as §7. No catalog value changes until then (Q0039: "a whole-matrix measurement before any
value changes").

## 1. Summary

1. **759 values audited** across 42 units, 49 structures and 82 upgrades.

   | File | A | B | C | D | E | Total |
   |---|---|---|---|---|---|---|
   | `units.ron` | 182 | 29 | 188 | 42 | 113 | 554 |
   | `structures.ron` | 9 | 17 | 84 | 0 | 1 | 111 |
   | `upgrades.ron` | 9 | 0 | 85 | 0 | 0 | 94 |
   | **All** | 200 | 46 | 357 | 42 | 114 | 759 |

2. **The layer chain needed a correction.** The plan and the battle 33 investigation read the data as
   `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti`. For 5.0.14 the right chain is
   `core → liberty → swarm → void → voidmulti` (§2.2):
   - `libertymulti` and `swarmmulti` are the Wings of Liberty and Heart of the Swarm multiplayer mods.
   - `balancemulti` holds values older than 5.0.14.

   The evidence for both points is in §2.2. Among the battle 33 investigation's marine and zealot values, none
   changes. Among the audited values, the change decides four:
   - the Siege Mode period;
   - the Void Ray's build time;
   - the Hydralisk upgrade research times;
   - Anabolic Synthesis.
3. **Confirmations the plan asked for (WP3 step 4).**
   - **WP10 and WP11:** all 29 shielded archetypes (14 units, `flying_saucer` included, and 15 structures) map to
     game units with `ShieldRegenRate 2` and `ShieldRegenDelay 10` in Normal time. That is 0.112 per tick and 178.57
     ticks. The table is in §5, group WP10.
   - **WP8:** the marauder analogue `shock_golem` has **no** `overclock` in the catalog. The 20-life-cost Overclock
     sits on `rhino_beetle` (the Roach), which has no Stimpack in StarCraft II. WP8 therefore changes `pulse_bot`
     only. The Marauder's `StimpackMarauder` also lasts 15 Normal s (`liberty` BehaviorData:604). See §6, item 3.
4. **The six damage-point-0 identities of plan §2.2.**
   - Their periods match the reference for four of them:
     - `napalm_wheeler` 45;
     - `prism_weaver` 18;
     - `tripod_annihilator` 27;
     - `prismatic_saucer` 9.
   - `sky_bastion`'s Defender-mode period is 28 against 28.57, which rounds to 29 (class C).
   - `flying_saucer` has no clear analogue (class E). Its period of 20 would match the Phoenix's 19.64.

   **None of the six has a damage point of 0 in the reference.** The Hellion's is 0.25 s, the Sentry's, the
   Void Ray's and the Liberator's are the 0.167 s default or 0.125 s, and the Colossus's is 0.0832 s. Group W4
   would give each a wind-up of 1 to 4 ticks, so WP2's cadence fix matters for them only until W4 lands.
5. **The largest classes of difference.**
   - Damage points (W4) and backswings (W5) were never sourced: 88 values. The reference default is a damage point
     of 0.167 Normal s (3 ticks) and a backswing of 0.5 Normal s (9 ticks). Most catalog weapons have 1 to 2 and 2 to
     4.
   - Projectile speeds (W7) are 0.5 to 1.2 per tick. Most game missiles fly at 18.75 to 40 Normal units per s, which
     is 1.05 to 2.24 per tick.
   - Zerg life regeneration is 0.02 per tick against 0.0153 (R2).
   - Energy regeneration is 0.056 against 0.0315 (R3).
   - Production and research times (P2, P3) are mostly Liquipedia's whole-second roundings. 31 of them differ by more
     than that, for example Spawning Pool 525 against 1161 ticks.
6. **Values copied in Normal time (class B, 46).** Besides the four of Q0039, these were copied in Normal time:
   - worker speed 2.8125 (doc 07 §2.2 quotes it as "2.81 tiles/s");
   - Roach and Siege Tank speed;
   - three build times: zealot, Siege Tank and Planetary Fortress;
   - Siege Tank and Planetary Fortress periods;
   - the damage point 4 that plan 17 called SC2's "0.16 s" frontswing;
   - the Reaper's life regeneration;
   - the Stalker's projectile speed.
7. **Deliberate choices (class D, 42 values).** Doc 07 §2.3, plan 03 §2.2 and plan 16 §1.2 chose instant hull turning
   for every unit: `turn_rate_deg` of 180 or more is read as instant. `catalog.rs` reads the field in degrees per
   **tick**, while doc 07 §2.3 describes it in degrees per **second**. The reference turns at 56° per tick
   (999.84 Normal °/s) for most units, 40° per tick for those on the engine default of 720 Normal °/s (for
   example the Immortal, Colossus, Carrier and Brood Lord), and 20° per tick for the Siege Tank, Thor and
   Ultralisk. Plan 17 also chose `welder_drone`'s 6-tick backswing for a readable animation. All are collected in
   [Q0045](../questions/open/0045-catalog-time-audit-deliberate-values.md), default no change.
8. **Not found.** Four projectile speeds have no constant-speed mover in the data (§8):
   - High Templar;
   - Spine Crawler;
   - Brood Lord, whose attack spawns Broodlings;
   - the Carrier, whose weapon abstracts its interceptors.

   The worker mining time and the Siege Mode transformation time are not plain fields either. The mining time comes
   from Liquipedia (1.99 Faster s). The siege time is derived from the morph's section timings and is marked
   **verify**.

## 2. Method

### 2.1 What was audited

Every field of plan §D-7 that occurs in `assets/catalog/units.ron`, `structures.ron` and `upgrades.ron`:

| Kind | Fields in the shipped catalog |
|---|---|
| Vitals and motion | `max_speed`, `acceleration`, `deceleration`, `turn_rate_deg`, `hp_regen_per_tick`, `shield_regen_per_tick`, `energy_regen_per_tick` |
| Weapons (units, structures, the Siege Mode override) | `period_ticks`, `damage_point_ticks`, `backswing_ticks`, `turret_turn_rate_deg` (where non-zero or a turret), `projectile_speed` |
| Capabilities and abilities | `overclock.{duration_ticks, speed_multiplier, attack_rate_multiplier}`, `harvester.harvest_rate_ticks`, the one ability `cooldown_ticks`, the one mode `cast_ticks` |
| Production and upgrades | `build_time_ticks`, `research_ticks`, non-zero `move_speed_delta` and `attack_cooldown_delta` |

Some fields need no row:
- `shield_regen_delay_ticks` does not exist yet (WP9 adds it). Its reference is confirmed in §1 item 3.
- `max_projectile_lifetime_ticks` and `energy_cost_per_second` appear in no shipped catalog file.
- A structure without `hp_regen_per_tick` or `shield_regen_per_tick` defaults to 0, which matches the reference for
  every Terran and Protoss structure. Only the fields present are listed.

Each identity's StarCraft II counterpart comes from its `sc2_analogue` or `sc2_inspiration` field, from the doc 07
roster, and for upgrades from the display name. Three cases needed a decision:

- `flying_saucer`: the catalog names "Viking / Scout", doc 07 the Phoenix, and its cost and weapon match neither.
  It is class E, with the Phoenix's numbers noted where they help.
- `mothership_carrier` ("Carrier / Mothership") is audited against the **Carrier**, which its cost, supply and build
  time match. Its single 8-strike weapon abstracts the interceptors, so it is class E.
- `prismatic_saucer`: the Void Ray's 5.0.14 weapon is `VoidRaySwarm` (`void` UnitData:1423 replaces
  `PrismaticBeam`).

### 2.2 Source and layer chain

**Primary source.** SC2Mapster SC2GameData at commit
[`5444e12f13`](https://github.com/SC2Mapster/SC2GameData/tree/5444e12f13) ("SC2 5.0.14.94342"), files
`mods/<layer>.sc2mod/base.sc2data/GameData/{Unit,Weapon,Effect,Abil,Behavior,Upgrade,Mover,Turret}Data.xml`.

**Citations.** A citation such as `liberty` WeaponData:207 gives the layer, the file and the line of the element that
holds the value, for example the `<Period value="0.8608"/>` line. The investigation cited the line of the enclosing
unit or weapon instead.

**Resolution.**
- A value no layer sets on the element comes from its `parent` element, then from the class's `default="1"`
  element. Such a citation says "(default `CWeapon`)" or "(parent …)".
- `##id##` stands for the element's own id.

**The chain is `core → liberty → swarm → void → voidmulti`.** This deviates from the plan §D-7 and the
investigation §1.2, which list eight layers ending in `balancemulti`. Two kinds of layer are left out.

**1. `libertymulti` and `swarmmulti`.** These are the Wings of Liberty and Heart of the Swarm multiplayer mods.
Legacy of the Void's multiplayer builds on `void`, not on them. Among the audited values, only one depends on them:

| Value | Without the two layers | With them | Liquipedia |
|---|---|---|---|
| Siege Mode period (`CrucioShockCannon`) | 3 Normal s (`liberty` WeaponData:261), 2.14 Faster s | 2.8 Normal s (`swarmmulti` WeaponData:4), 2.0 Faster s | 2.14 ([Siege Tank (LotV)](https://liquipedia.net/starcraft2/Siege_Tank_(Legacy_of_the_Void)), infobox) |

**2. `balancemulti`.** This mod carries values from before 5.0.14. Where it and `voidmulti` disagree, the
patch-dated value is `voidmulti`'s:

| Value | `voidmulti` | `balancemulti` | Liquipedia (5.0.14) |
|---|---|---|---|
| Void Ray build time | 60.2 Normal s = 43 Faster s (`voidmulti` AbilData:959) | 52 = 37 s | 43; "5.0.9: Void Ray build time increased from 37 to 43 seconds" ([Void Ray (LotV)](https://liquipedia.net/starcraft2/Void_Ray_(Legacy_of_the_Void))) |
| Anabolic Synthesis speed bonus | +0.421871 Normal = +0.59 Faster (`voidmulti` UpgradeData:47) | +0.5898 = +0.83 | "+0.59"; "5.0.14: … reduced from 0.82 to 0.57 … The new value is 0.59" ([Ultralisk (LotV)](https://liquipedia.net/starcraft2/Ultralisk_(Legacy_of_the_Void))) |
| Grooved Spines / Muscular Augments research | 70 / 90 Normal s = 50 / 64 Faster s (`voidmulti` AbilData:528, :535) | 100 / 100 = 71 / 71 s | 50 / 64, changed in 5.0.12 ([Hydralisk (LotV)](https://liquipedia.net/starcraft2/Hydralisk_(Legacy_of_the_Void))) |
| Hydralisk "Frenzy" research (Nanomuscular Swell) | present, 90 Normal s = 64 Faster s (`voidmulti` AbilData:542) | absent | "5.0.14: New research: Nanomuscular Swell … 64 seconds" (same page) |

**Effect on earlier work.** The battle 33 investigation's marine and zealot values are identical under both chains.
A full diff of the per-unit dumps shows no marine or zealot value that changes. Its `balancemulti` citations, for
example the Charge speed at `balancemulti` UpgradeData:67, have equal `voidmulti` counterparts (UpgradeData:69). Its
figures therefore stand, but its §1.2 description of the chain should be corrected by WP13.

**Secondary source.** Liquipedia wikitext (`action=raw`), read 2026-11-04. It is used:
- for the mining time, which the data does not hold;
- for the chain checks above;
- as a cross-check for the speed and period roundings.

### 2.3 Conversion and classification

**Conversion** (as in the investigation §2.2; `sim_types::TICKS_PER_SECOND` is 25):
- Faster seconds = Normal seconds ÷ 1.4.
- Ticks = Faster seconds × 25.
- Per-tick speeds and rates = Normal value × 1.4 ÷ 25.
- Turn rates in degrees per tick = Normal °/s × 1.4 ÷ 25.

**Matching rules.**
- **Times:** a value matches (A) when it equals the reference rounded to the nearest whole tick, halves up (plan
  §2.3).
- **Speeds and rates:** a value matches when it is within 0.0002 per tick. That is Liquipedia's two-decimal Faster
  value divided by 25, so 0.1888 for 4.725 is a match.
- **Class B:** the value equals the Normal value used unconverted. For times that is Normal s × 25, rounded; for
  speeds and rates it is the Normal value ÷ 25.

**Classes** (plan §D-7):

| Class | Rule used here |
|---|---|
| A | Matches the reference after rounding |
| B | Equals the Normal-time value, unconverted |
| C | Differs otherwise, and no project document chose it. This includes values that a document **quotes as StarCraft II's** and gets wrong: that is a transcription error. The document is named in the note so the correcting commit can fix it too |
| D | Differs, and a project document chose it **for a reason of its own** (determinism, readability, a mechanic modelled differently) |
| E | No reference. Either the identity has no clear counterpart, the mechanic is not modelled (for example `acceleration` and `deceleration`, which are parsed but unused because speed changes are instant; the catalog copies `max_speed` into both), or the game's value is not a single number (thrown or accelerating missiles) |

**Field-specific rules.**
- **Projectile speeds.** Weapons whose effect chain has no launch-missile effect deal damage instantly. Our `None`
  matches them. For a missile, the reference is the mover's `MaxSpeed`, but only when the mover is a single
  guidance phase that is effectively constant (`Acceleration` ≥ 1000, or `Speed` equal to `MaxSpeed`). Multi-phase
  or accelerating missiles are class E, with the phases listed.
- **Upgrade speed deltas.** These were resolved by hand from UpgradeData, because array merging across layers is
  not modelled by the script. Each was checked against Liquipedia's "(+x)" figure.

### 2.4 Reproduction

The scripts live in `~/scratch/combat_wp3_scripts/` (not committed; plan WP3 step 1). The data comes from SC2GameData
at `5444e12f13`, layers as in §2.2, fetched into `~/scratch/combat_wp3_data/<layer>/`.

```bash
# 1. our values: one row per time or speed field
python3 -I extract_catalog.py assets/catalog > fields.tsv
# 2. references and classes (chain: SC2_LAYERS, default "core liberty swarm void voidmulti")
python3 -I audit.py ~/scratch/combat_wp3_data fields.tsv > audit_rows.tsv
# 3. annotations (scheduled WPs, class D), WP12 groups and the tables of §4 and §5
python3 -I render.py audit_rows.tsv > tables.md
```

## 3. Counts

| File | A | B | C | D | E | Total |
|---|---|---|---|---|---|---|
| `units.ron` | 182 | 29 | 188 | 42 | 113 | 554 |
| `structures.ron` | 9 | 17 | 84 | 0 | 1 | 111 |
| `upgrades.ron` | 9 | 0 | 85 | 0 | 0 | 94 |
| **All** | 200 | 46 | 357 | 42 | 114 | 759 |

"Scheduled" values belong to a work package that already changes them, so they are not part of WP12:

| Work package | Values |
|---|---|
| WP4 | zealot period, damage point and backswing |
| WP6 | marine damage point and backswing, ground and air |
| WP7 | marine speed |
| WP8 | Stimpack duration |
| WP10 | shield regeneration, 29 values |

The WP10 values are still listed as group WP10 in §5, as the plan's confirmation.

## 4. Tables

Columns:
- **Editor (Normal)** is the game-data value in Normal seconds or units per Normal second.
- **Reference (ours)** is that value in our units, unrounded.
- **Group** is the WP12 group of §5.


### `units.ron`

| Identity | SC2 | Field | Ours | Editor (Normal) | Source | Reference (ours) | Class | Group | Note |
|---|---|---|---|---|---|---|---|---|---|
| `welder_drone` | SCV | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `welder_drone` | SCV | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `welder_drone` | SCV | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `welder_drone` | SCV | `max_speed` | 0.1124 | 2.8125 | `liberty` UnitData:4731 | 0.1575 | B | S1 |  |
| `welder_drone` | SCV | `acceleration` | 0.1124 | 2.5 | `liberty` UnitData:4732 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `welder_drone` | SCV | `deceleration` | 0.1124 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `welder_drone` | SCV | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:4806 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `welder_drone` | SCV | `build_time_ticks` | 300 | 17 | `liberty` AbilData:1569 (`CommandCenterTrain` Train1) | 303.6 | C | P2 |  |
| `welder_drone` | SCV `FusionCutter` | `weapon/period_ticks` | 19 | 1.5 | `liberty` WeaponData:200 | 26.79 | C | W2 |  |
| `welder_drone` | SCV `FusionCutter` | `weapon/damage_point_ticks` | 4 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | B | W3 | plan 17 §1 Bug 3 chose 4 as SC2's "0.16 s" frontswing: the Normal default 0.167 s × 25 |
| `welder_drone` | SCV `FusionCutter` | `weapon/backswing_ticks` | 6 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | D |  | chosen by plan 17 §1 Bug 3 ("readable windup animation") |
| `welder_drone` | SCV `FusionCutter` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `FusionCutter` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `welder_drone` | SCV | `capabilities/harvester/harvest_rate_ticks` | 60 | 1.99 Faster s (2.786 Normal) | Liquipedia, Resources ("occupies a mineral field for 1.99 seconds"); not found in the game data (engine default) | 49.8 | C | M1 |  |
| `pulse_bot` | Marine | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `pulse_bot` | Marine | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `pulse_bot` | Marine | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `pulse_bot` | Marine | `max_speed` | 0.09 | 2.25 | `liberty` UnitData:4821 | 0.126 | B |  | changed by WP7 (Q0039) |
| `pulse_bot` | Marine | `acceleration` | 0.09 | 1000 | `liberty` UnitData:4822 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `pulse_bot` | Marine | `deceleration` | 0.09 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `pulse_bot` | Marine | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:4825 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `pulse_bot` | Marine | `build_time_ticks` | 450 | 25 | `liberty` AbilData:1630 (`BarracksTrain` Train1) | 446.4 | C | P2 |  |
| `pulse_bot` | Marine `GuassRifle` | `weapon/period_ticks` | 15 | 0.8608 | `liberty` WeaponData:207 | 15.37 | A |  |  |
| `pulse_bot` | Marine `GuassRifle` | `weapon/damage_point_ticks` | 2 | 0.05 | `liberty` WeaponData:208 | 0.89 | C |  | changed by WP6 (Q0038) |
| `pulse_bot` | Marine `GuassRifle` | `weapon/backswing_ticks` | 4 | 0.75 | `liberty` WeaponData:209 | 13.39 | C |  | changed by WP6 (Q0038) |
| `pulse_bot` | Marine `GuassRifle` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `GuassRifle` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `pulse_bot` | Marine `GuassRifle` | `air_weapon/period_ticks` | 15 | 0.8608 | `liberty` WeaponData:207 | 15.37 | A |  |  |
| `pulse_bot` | Marine `GuassRifle` | `air_weapon/damage_point_ticks` | 2 | 0.05 | `liberty` WeaponData:208 | 0.89 | C |  | changed by WP6 (Q0038) |
| `pulse_bot` | Marine `GuassRifle` | `air_weapon/backswing_ticks` | 4 | 0.75 | `liberty` WeaponData:209 | 13.39 | C |  | changed by WP6 (Q0038) |
| `pulse_bot` | Marine `GuassRifle` | `air_weapon/projectile_speed` | None | instant | no launch-missile effect under `GuassRifle` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `pulse_bot` | Marine | `capabilities/overclock/duration_ticks` | 250 | 15 | `liberty` BehaviorData:684 (`Stimpack`) | 267.9 | C |  | changed by WP8 (Q0039) |
| `pulse_bot` | Marine | `capabilities/overclock/speed_multiplier` | 1.5 | 1.5 | `liberty` BehaviorData:685 (`Stimpack`) | 1.5 | A |  |  |
| `pulse_bot` | Marine | `capabilities/overclock/attack_rate_multiplier` | 1.5 | 1.5 | `liberty` BehaviorData:685 (`Stimpack`) | 1.5 | A |  |  |
| `rhino_beetle` | Roach | `hp_regen_per_tick` | 0.00 | 0.2734 | `liberty` UnitData:7184 | 0.01531 | C | R2 |  |
| `rhino_beetle` | Roach | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `rhino_beetle` | Roach | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `rhino_beetle` | Roach | `max_speed` | 0.09 | 2.25 | `liberty` UnitData:7186 | 0.126 | B | S1 |  |
| `rhino_beetle` | Roach | `acceleration` | 0.09 | 1000 | `liberty` UnitData:7188 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `rhino_beetle` | Roach | `deceleration` | 0.09 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `rhino_beetle` | Roach | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:7191 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `rhino_beetle` | Roach | `build_time_ticks` | 750 | 27 | `liberty` AbilData:2510 (`LarvaTrain` Train10) | 482.1 | C | P3 |  |
| `rhino_beetle` | Roach `AcidSaliva` | `weapon/period_ticks` | 28 | 2 | `liberty` WeaponData:345 | 35.71 | C | W2 |  |
| `rhino_beetle` | Roach `AcidSaliva` | `weapon/damage_point_ticks` | 2 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `rhino_beetle` | Roach `AcidSaliva` | `weapon/backswing_ticks` | 4 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `rhino_beetle` | Roach `AcidSaliva` | `weapon/projectile_speed` | Some(0.64) | 18.75 | `liberty` MoverData:674 (mover `AcidSalivaWeapon`, `MaxSpeed`) | 1.050 | C | W7 |  |
| `rhino_beetle` | Roach | `capabilities/overclock/duration_ticks` | 250 | — | the reference unit has no Stimpack | — | E |  | no counterpart: the Roach has no Stimpack (the Marauder's lasts 15 Normal s, `liberty` BehaviorData:604) |
| `rhino_beetle` | Roach | `capabilities/overclock/speed_multiplier` | 1.5 | — | the reference unit has no Stimpack | — | E |  | no counterpart (see duration) |
| `rhino_beetle` | Roach | `capabilities/overclock/attack_rate_multiplier` | 1.5 | — | the reference unit has no Stimpack | — | E |  | no counterpart (see duration) |
| `psi_grey` | Zealot | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `psi_grey` | Zealot | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:3176 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `psi_grey` | Zealot | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `psi_grey` | Zealot | `max_speed` | 0.126 | 2.25 | `liberty` UnitData:3178 | 0.126 | A |  |  |
| `psi_grey` | Zealot | `acceleration` | 0.126 | 1000 | `liberty` UnitData:3179 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `psi_grey` | Zealot | `deceleration` | 0.126 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `psi_grey` | Zealot | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:3224 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `psi_grey` | Zealot | `build_time_ticks` | 950 | 38 | `swarm` AbilData:1068 (`GatewayTrain` Train1) | 678.6 | B | P1 |  |
| `psi_grey` | Zealot `PsiBlades` | `weapon/period_ticks` | 22 | 1.2 | `liberty` WeaponData:155 | 21.43 | C |  | changed by WP4 (Q0036) |
| `psi_grey` | Zealot `PsiBlades` | `weapon/damage_point_ticks` | 4 | 0 | `liberty` WeaponData:156 | 0.00 | C |  | changed by WP4 (Q0036) |
| `psi_grey` | Zealot `PsiBlades` | `weapon/backswing_ticks` | 6 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C |  | changed by WP4 (Q0036) |
| `psi_grey` | Zealot `PsiBlades` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `PsiBladesBurst` (`liberty` WeaponData:157) | None | A |  |  |
| `railgun_crawler` | SiegeTank | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `railgun_crawler` | SiegeTank | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `railgun_crawler` | SiegeTank | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `railgun_crawler` | SiegeTank | `max_speed` | 0.09 | 2.25 | `liberty` UnitData:5075 | 0.126 | B | S1 |  |
| `railgun_crawler` | SiegeTank | `acceleration` | 0.09 | 1000 | `liberty` UnitData:5076 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `railgun_crawler` | SiegeTank | `deceleration` | 0.09 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `railgun_crawler` | SiegeTank | `turn_rate_deg` | 360.0 | 360 | `liberty` UnitData:5079 | 20.2 deg/tick | D |  | ours is read as degrees per tick, so 360 = instant (catalog.rs `turn_rate`) |
| `railgun_crawler` | SiegeTank | `build_time_ticks` | 1125 | 45 | `liberty` AbilData:1649 (`FactoryTrain` Train2) | 803.6 | B | P1 |  |
| `railgun_crawler` | SiegeTank turret `SiegeTank` | `weapon/turret_turn_rate_deg` | 21.6 | 720 | `liberty` TurretData:26 (parent `FreeRotate`) | 40.32 deg/tick | C | W8 | ours = 21.6 deg/tick (values above 360 are read as deg/s) |
| `railgun_crawler` | SiegeTank `90mmCannons` | `weapon/period_ticks` | 26 | 1.04 | `liberty` WeaponData:242 | 18.57 | B | W1 |  |
| `railgun_crawler` | SiegeTank `90mmCannons` | `weapon/damage_point_ticks` | 4 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | B | W3 |  |
| `railgun_crawler` | SiegeTank `90mmCannons` | `weapon/backswing_ticks` | 4 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `railgun_crawler` | SiegeTank `90mmCannons` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `90mmCannons` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `railgun_crawler` | SiegeTank | `modes/cast_ticks` | 71 | 0.5 + 3.5417 | `liberty` AbilData:1323-1324 (`SiegeMode`, section Stats delay and duration) | 72.2 | C | M2 | **verify** that the section delay and duration add; Liquipedia gives no figure |
| `railgun_crawler` | SiegeTank turret `SiegeTankSieged` | `modes/turret_turn_rate_deg` | 21.6 | 360 | `liberty` TurretData:82 | 20.16 deg/tick | C | W8 | ours = 21.6 deg/tick (values above 360 are read as deg/s) |
| `railgun_crawler` | SiegeTank `CrucioShockCannon` | `modes/period_ticks` | 54 | 3 | `liberty` WeaponData:261 | 53.57 | A |  |  |
| `railgun_crawler` | SiegeTank `CrucioShockCannon` | `modes/damage_point_ticks` | 4 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | B | W3 |  |
| `railgun_crawler` | SiegeTank `CrucioShockCannon` | `modes/backswing_ticks` | 4 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `railgun_crawler` | SiegeTank `CrucioShockCannon` | `modes/projectile_speed` | None | instant | no launch-missile effect under `CrucioShockCannonSwitch` (`swarm` WeaponData:170) | None | A |  |  |
| `railgun_crawler` | SiegeTank | `abilities/cooldown_ticks` | 75 | none | `liberty` AbilData:1308 (`SiegeMode`): no `Cost/Cooldown` | 0 | C | M2 | the reference has no cooldown on Siege Mode; the morph itself locks the tank |
| `flying_saucer` | — | `hp_regen_per_tick` | 0.00 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | Phoenix? | `shield_regen_per_tick` | 0.08 | 2 | every shielded unit in the data sets `ShieldRegenRate 2` | 0.112 | B | WP10 | changed by WP10 (Q0039); analogue unclear (catalog: Viking / Scout; doc 07: Phoenix); shield rate is 2 for every candidate |
| `flying_saucer` | — | `energy_regen_per_tick` | 0.000 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `max_speed` | 0.15 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `acceleration` | 0.15 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `deceleration` | 0.15 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `turn_rate_deg` | 999.0 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `build_time_ticks` | 850 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `weapon/period_ticks` | 20 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `weapon/damage_point_ticks` | 0 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `weapon/backswing_ticks` | 2 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `weapon/projectile_speed` | None | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `air_weapon/period_ticks` | 20 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `air_weapon/damage_point_ticks` | 0 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `air_weapon/backswing_ticks` | 2 | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `flying_saucer` | — | `air_weapon/projectile_speed` | None | — | — | — | E |  | analogue unclear: catalog says Viking / Scout, doc 07 says Phoenix, cost and weapon match neither |
| `shock_golem` | Marauder | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `shock_golem` | Marauder | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `shock_golem` | Marauder | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `shock_golem` | Marauder | `max_speed` | 0.126 | 2.25 | `liberty` UnitData:10968 | 0.126 | A |  |  |
| `shock_golem` | Marauder | `acceleration` | 0.126 | 1000 | `liberty` UnitData:10969 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `shock_golem` | Marauder | `deceleration` | 0.126 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `shock_golem` | Marauder | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:11021 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `shock_golem` | Marauder | `build_time_ticks` | 525 | 30 | `liberty` AbilData:1642 (`BarracksTrain` Train4) | 535.7 | C | P2 |  |
| `shock_golem` | Marauder `PunisherGrenades` | `weapon/period_ticks` | 26 | 1.5 | `liberty` WeaponData:249 | 26.79 | C | W2 |  |
| `shock_golem` | Marauder `PunisherGrenades` | `weapon/damage_point_ticks` | 1 | 0 | `liberty` WeaponData:250 | 0.00 | C | W4 |  |
| `shock_golem` | Marauder `PunisherGrenades` | `weapon/backswing_ticks` | 4 | 0 | `liberty` WeaponData:251 | 0.00 | C | W5 |  |
| `shock_golem` | Marauder `PunisherGrenades` | `weapon/projectile_speed` | Some(0.64) | 20 | `liberty` MoverData:666 (mover `PunisherGrenadesWeapon`, `MaxSpeed`) | 1.120 | C | W7 |  |
| `jet_jumper` | Reaper | `hp_regen_per_tick` | 0.08 | 2 | `void` UnitData:1476 | 0.112 | B | R1 | the reference regenerates only after 10 Normal s without damage (`void` UnitData:1475); not modelled |
| `jet_jumper` | Reaper | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `jet_jumper` | Reaper | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `jet_jumper` | Reaper | `max_speed` | 0.21 | 3.75 | `void` UnitData:1480 | 0.21 | A |  |  |
| `jet_jumper` | Reaper | `acceleration` | 0.21 | 1000 | `liberty` UnitData:4892 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `jet_jumper` | Reaper | `deceleration` | 0.21 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `jet_jumper` | Reaper | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:4895 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `jet_jumper` | Reaper | `build_time_ticks` | 800 | 45 | `void` AbilData:908 (`BarracksTrain` Train2) | 803.6 | C | P2 |  |
| `jet_jumper` | Reaper `P38ScytheGuassPistol` | `weapon/period_ticks` | 20 | 1.1 | `liberty` WeaponData:323 | 19.64 | A |  |  |
| `jet_jumper` | Reaper `P38ScytheGuassPistol` | `weapon/damage_point_ticks` | 1 | 0 | `liberty` WeaponData:324 | 0.00 | C | W4 |  |
| `jet_jumper` | Reaper `P38ScytheGuassPistol` | `weapon/backswing_ticks` | 3 | 0.75 | `liberty` WeaponData:325 | 13.39 | C | W5 |  |
| `jet_jumper` | Reaper `P38ScytheGuassPistol` | `weapon/projectile_speed` | Some(1.0) | instant | no launch-missile effect under `P38ScytheGuassPistolBurst` (`liberty` WeaponData:326) | None | C | W7 | the reference applies its damage with no missile |
| `specter_android` | Ghost | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `specter_android` | Ghost | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `specter_android` | Ghost | `energy_regen_per_tick` | 0.056 | 0.5625 | `liberty` UnitData:4989 | 0.0315 | C | R3 |  |
| `specter_android` | Ghost | `max_speed` | 0.1576 | 2.8125 | `voidmulti` UnitData:587 | 0.1575 | A |  |  |
| `specter_android` | Ghost | `acceleration` | 0.1576 | 1000 | `liberty` UnitData:4991 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `specter_android` | Ghost | `deceleration` | 0.1576 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `specter_android` | Ghost | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:5052 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `specter_android` | Ghost | `build_time_ticks` | 725 | 40 | `liberty` AbilData:1638 (`BarracksTrain` Train3) | 714.3 | C | P2 |  |
| `specter_android` | Ghost `C10CanisterRifle` | `weapon/period_ticks` | 27 | 1.5 | `liberty` WeaponData:578 | 26.79 | A |  |  |
| `specter_android` | Ghost `C10CanisterRifle` | `weapon/damage_point_ticks` | 1 | 0.083 | `liberty` WeaponData:579 | 1.48 | A |  |  |
| `specter_android` | Ghost `C10CanisterRifle` | `weapon/backswing_ticks` | 3 | 1.167 | `liberty` WeaponData:580 | 20.84 | C | W5 |  |
| `specter_android` | Ghost `C10CanisterRifle` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `C10CanisterRifle` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `napalm_wheeler` | Hellion | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `napalm_wheeler` | Hellion | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `napalm_wheeler` | Hellion | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `napalm_wheeler` | Hellion | `max_speed` | 0.238 | 4.25 | `liberty` UnitData:11551 | 0.238 | A |  |  |
| `napalm_wheeler` | Hellion | `acceleration` | 0.238 | 1000 | `liberty` UnitData:11552 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `napalm_wheeler` | Hellion | `deceleration` | 0.238 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `napalm_wheeler` | Hellion | `turn_rate_deg` | 999.0 | 720 | `core` UnitData:40 (default `CUnit`) | 40.3 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `napalm_wheeler` | Hellion | `build_time_ticks` | 525 | 30 | `liberty` AbilData:1657 (`FactoryTrain` Train6) | 535.7 | C | P2 |  |
| `napalm_wheeler` | Hellion `InfernalFlameThrower` | `weapon/period_ticks` | 45 | 2.5 | `liberty` WeaponData:587 | 44.64 | A |  |  |
| `napalm_wheeler` | Hellion `InfernalFlameThrower` | `weapon/damage_point_ticks` | 0 | 0.25 | `liberty` WeaponData:588 | 4.46 | C | W4 |  |
| `napalm_wheeler` | Hellion `InfernalFlameThrower` | `weapon/backswing_ticks` | 2 | 0.75 | `liberty` WeaponData:589 | 13.39 | C | W5 |  |
| `napalm_wheeler` | Hellion `InfernalFlameThrower` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `InfernalFlameThrowerCP` (`liberty` WeaponData:591) | None | A |  |  |
| `colossus_mech` | Thor | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `colossus_mech` | Thor | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `colossus_mech` | Thor | `energy_regen_per_tick` | 0.000 | 0 | `void` UnitData:1403 | 0 | A |  |  |
| `colossus_mech` | Thor | `max_speed` | 0.1048 | 1.875 | `liberty` UnitData:5210 | 0.105 | A |  |  |
| `colossus_mech` | Thor | `acceleration` | 0.1048 | 1000 | `liberty` UnitData:5211 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `colossus_mech` | Thor | `deceleration` | 0.1048 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `colossus_mech` | Thor | `turn_rate_deg` | 360.0 | 360 | `liberty` UnitData:5234 | 20.2 deg/tick | D |  | ours is read as degrees per tick, so 360 = instant (catalog.rs `turn_rate`) |
| `colossus_mech` | Thor | `build_time_ticks` | 1075 | 60 | `liberty` AbilData:1653 (`FactoryTrain` Train5) | 1071.4 | C | P2 |  |
| `colossus_mech` | Thor `ThorsHammer` | `ground_weapon/period_ticks` | 23 | 1.28 | `swarm` WeaponData:195 | 22.86 | A |  |  |
| `colossus_mech` | Thor `ThorsHammer` | `ground_weapon/damage_point_ticks` | 2 | 0.831 | `liberty` WeaponData:233 | 14.84 | C | W4 |  |
| `colossus_mech` | Thor `ThorsHammer` | `ground_weapon/backswing_ticks` | 4 | 0.25 | `liberty` WeaponData:234 | 4.46 | A |  |  |
| `colossus_mech` | Thor `ThorsHammer` | `ground_weapon/projectile_speed` | None | instant | no launch-missile effect under `ThorsHammer` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `colossus_mech` | Thor `LanceMissileLaunchers` | `air_weapon/turret_turn_rate_deg` | 21.6 | — | the reference weapon has no turret | — | E |  | no counterpart turret |
| `colossus_mech` | Thor `LanceMissileLaunchers` | `air_weapon/period_ticks` | 36 | 1.28 | `voidmulti` WeaponData:129 | 22.86 | C | W2 |  |
| `colossus_mech` | Thor `LanceMissileLaunchers` | `air_weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `colossus_mech` | Thor `LanceMissileLaunchers` | `air_weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `colossus_mech` | Thor `LanceMissileLaunchers` | `air_weapon/projectile_speed` | Some(1.2) | instant | no launch-missile effect under `LanceMissileLaunchersDamage` (`void` WeaponData:85) | None | C | W7 | the reference applies its damage with no missile |
| `colossus_mech` | Thor `ThorsHammer` | `weapon/period_ticks` | 23 | 1.28 | `swarm` WeaponData:195 | 22.86 | A |  |  |
| `colossus_mech` | Thor `ThorsHammer` | `weapon/damage_point_ticks` | 2 | 0.831 | `liberty` WeaponData:233 | 14.84 | C | W4 |  |
| `colossus_mech` | Thor `ThorsHammer` | `weapon/backswing_ticks` | 4 | 0.25 | `liberty` WeaponData:234 | 4.46 | A |  |  |
| `colossus_mech` | Thor `ThorsHammer` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `ThorsHammer` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `vector_interceptor` | VikingFighter | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `vector_interceptor` | VikingFighter | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `vector_interceptor` | VikingFighter | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `vector_interceptor` | VikingFighter | `max_speed` | 0.154 | 2.75 | `liberty` UnitData:6838 | 0.154 | A |  |  |
| `vector_interceptor` | VikingFighter | `acceleration` | 0.154 | 2.625 | `liberty` UnitData:6839 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `vector_interceptor` | VikingFighter | `deceleration` | 0.154 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `vector_interceptor` | VikingFighter | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:6841 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `vector_interceptor` | VikingFighter | `build_time_ticks` | 750 | 42 | `liberty` AbilData:1681 (`StarportTrain` Train5) | 750.0 | A |  |  |
| `vector_interceptor` | VikingFighter `LanzerTorpedoes` | `air_weapon/period_ticks` | 36 | 2 | `liberty` WeaponData:273 | 35.71 | A |  |  |
| `vector_interceptor` | VikingFighter `LanzerTorpedoes` | `air_weapon/damage_point_ticks` | 1 | 0.05 | `voidmulti` WeaponData:134 | 0.89 | A |  |  |
| `vector_interceptor` | VikingFighter `LanzerTorpedoes` | `air_weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `vector_interceptor` | VikingFighter `LanzerTorpedoes` | `air_weapon/projectile_speed` | Some(1.5) | — | mover `VikingFighterMissile` has 4 motion phase(s) [(('Acceleration', '56'), ('Driver', 'Guidance'), ('MaxSpeed', '28'), ('Speed', '2')), (('Driver', 'Guidance'), ('MaxSpeed', '28')), (('Driver', 'Throw'), ('MaxSpeed', '2'), ('Speed', '2'))] | — | E |  | no single speed (thrown or accelerating missile) |
| `vector_interceptor` | VikingFighter `LanzerTorpedoes` | `weapon/period_ticks` | 36 | 2 | `liberty` WeaponData:273 | 35.71 | A |  |  |
| `vector_interceptor` | VikingFighter `LanzerTorpedoes` | `weapon/damage_point_ticks` | 1 | 0.05 | `voidmulti` WeaponData:134 | 0.89 | A |  |  |
| `vector_interceptor` | VikingFighter `LanzerTorpedoes` | `weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `vector_interceptor` | VikingFighter `LanzerTorpedoes` | `weapon/projectile_speed` | Some(1.5) | — | mover `VikingFighterMissile` has 4 motion phase(s) [(('Acceleration', '56'), ('Driver', 'Guidance'), ('MaxSpeed', '28'), ('Speed', '2')), (('Driver', 'Guidance'), ('MaxSpeed', '28')), (('Driver', 'Throw'), ('MaxSpeed', '2'), ('Speed', '2'))] | — | E |  | no single speed (thrown or accelerating missile) |
| `repair_shuttle` | Medivac | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `repair_shuttle` | Medivac | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `repair_shuttle` | Medivac | `energy_regen_per_tick` | 0.056 | 0.5625 | `liberty` UnitData:5441 | 0.0315 | C | R3 |  |
| `repair_shuttle` | Medivac | `max_speed` | 0.14 | 2.5 | `swarm` UnitData:1705 | 0.14 | A |  |  |
| `repair_shuttle` | Medivac | `acceleration` | 0.14 | 2.25 | `swarm` UnitData:1706 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `repair_shuttle` | Medivac | `deceleration` | 0.14 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `repair_shuttle` | Medivac | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:5446 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `repair_shuttle` | Medivac | `build_time_ticks` | 750 | 42 | `liberty` AbilData:1665 (`StarportTrain` Train1) | 750.0 | A |  |  |
| `sky_bastion` | Liberator | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `sky_bastion` | Liberator | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `sky_bastion` | Liberator | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `sky_bastion` | Liberator | `max_speed` | 0.1888 | 3.375 | `void` UnitData:3296 | 0.189 | A |  |  |
| `sky_bastion` | Liberator | `acceleration` | 0.1888 | 3.5 | `void` UnitData:3297 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `sky_bastion` | Liberator | `deceleration` | 0.1888 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `sky_bastion` | Liberator | `turn_rate_deg` | 999.0 | 1499.9414 | `void` UnitData:3299 | 84.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `sky_bastion` | Liberator | `build_time_ticks` | 850 | 60 | `void` AbilData:307 (`StarportTrain` Train7) | 1071.4 | C | P3 |  |
| `sky_bastion` | Liberator `LiberatorAGWeapon` | `ground_weapon/period_ticks` | 28 | 1.6 | `void` WeaponData:339 | 28.57 | C | W2 |  |
| `sky_bastion` | Liberator `LiberatorAGWeapon` | `ground_weapon/damage_point_ticks` | 0 | 0.125 | `void` WeaponData:338 | 2.23 | C | W4 |  |
| `sky_bastion` | Liberator `LiberatorAGWeapon` | `ground_weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `sky_bastion` | Liberator `LiberatorAGWeapon` | `ground_weapon/projectile_speed` | None | 40 | `void` MoverData:427 (mover `LiberatorAGMissile`, `MaxSpeed`) | 2.240 | C | W7 | ours is instant; the reference fires a missile |
| `sky_bastion` | Liberator `LiberatorMissileLaunchers` | `air_weapon/period_ticks` | 25 | 1.8 | `void` WeaponData:327 | 32.14 | C | W2 |  |
| `sky_bastion` | Liberator `LiberatorMissileLaunchers` | `air_weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `sky_bastion` | Liberator `LiberatorMissileLaunchers` | `air_weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `sky_bastion` | Liberator `LiberatorMissileLaunchers` | `air_weapon/projectile_speed` | Some(1.2) | — | mover `LiberatorMissile` has 2 motion phase(s) [(('Acceleration', '3200'), ('Driver', 'Guidance'), ('MaxSpeed', '16')), (('Acceleration', '3200'), ('Driver', 'Throw'), ('MaxSpeed', '8'), ('Speed', '1'))] | — | E |  | no single speed (thrown or accelerating missile) |
| `sky_bastion` | Liberator `LiberatorMissileLaunchers` | `weapon/period_ticks` | 25 | 1.8 | `void` WeaponData:327 | 32.14 | C | W2 |  |
| `sky_bastion` | Liberator `LiberatorMissileLaunchers` | `weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `sky_bastion` | Liberator `LiberatorMissileLaunchers` | `weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `sky_bastion` | Liberator `LiberatorMissileLaunchers` | `weapon/projectile_speed` | Some(1.2) | — | mover `LiberatorMissile` has 2 motion phase(s) [(('Acceleration', '3200'), ('Driver', 'Guidance'), ('MaxSpeed', '16')), (('Acceleration', '3200'), ('Driver', 'Throw'), ('MaxSpeed', '8'), ('Speed', '1'))] | — | E |  | no single speed (thrown or accelerating missile) |
| `cloak_gunship` | Banshee | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `cloak_gunship` | Banshee | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `cloak_gunship` | Banshee | `energy_regen_per_tick` | 0.056 | 0.5625 | `liberty` UnitData:5374 | 0.0315 | C | R3 |  |
| `cloak_gunship` | Banshee | `max_speed` | 0.154 | 2.75 | `liberty` UnitData:5376 | 0.154 | A |  |  |
| `cloak_gunship` | Banshee | `acceleration` | 0.154 | 3.25 | `liberty` UnitData:5377 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `cloak_gunship` | Banshee | `deceleration` | 0.154 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `cloak_gunship` | Banshee | `turn_rate_deg` | 999.0 | 1499.9414 | `liberty` UnitData:5415 | 84.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `cloak_gunship` | Banshee | `build_time_ticks` | 1075 | 60 | `liberty` AbilData:1669 (`StarportTrain` Train2) | 1071.4 | C | P2 |  |
| `cloak_gunship` | Banshee `BacklashRockets` | `weapon/period_ticks` | 22 | 1.25 | `liberty` WeaponData:556 | 22.32 | A |  |  |
| `cloak_gunship` | Banshee `BacklashRockets` | `weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `cloak_gunship` | Banshee `BacklashRockets` | `weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `cloak_gunship` | Banshee `BacklashRockets` | `weapon/projectile_speed` | Some(1.2) | — | mover `BacklashRocketsLMWeapon` has 1 motion phase(s) [(('Acceleration', '20'), ('Driver', 'Guidance'), ('MaxSpeed', '50'), ('Speed', '10'))] | — | E |  | no single speed (thrown or accelerating missile) |
| `omni_probe_drone` | Raven | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `omni_probe_drone` | Raven | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `omni_probe_drone` | Raven | `energy_regen_per_tick` | 0.056 | 0.5625 | `liberty` UnitData:5635 | 0.0315 | C | R3 |  |
| `omni_probe_drone` | Raven | `max_speed` | 0.154 | 2.9492 | `voidmulti` UnitData:2660 | 0.1652 | C | S2 |  |
| `omni_probe_drone` | Raven | `acceleration` | 0.154 | 2 | `liberty` UnitData:5579 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `omni_probe_drone` | Raven | `deceleration` | 0.154 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `omni_probe_drone` | Raven | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:5637 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `omni_probe_drone` | Raven | `build_time_ticks` | 850 | 48 | `voidmulti` AbilData:1026 (`StarportTrain` Train3) | 857.1 | C | P2 |  |
| `grav_harvester` | Probe | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `grav_harvester` | Probe | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:3072 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `grav_harvester` | Probe | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `grav_harvester` | Probe | `max_speed` | 0.1124 | 2.8125 | `liberty` UnitData:3074 | 0.1575 | B | S1 |  |
| `grav_harvester` | Probe | `acceleration` | 0.1124 | 2.5 | `liberty` UnitData:3075 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `grav_harvester` | Probe | `deceleration` | 0.1124 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `grav_harvester` | Probe | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:3157 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `grav_harvester` | Probe | `build_time_ticks` | 300 | 17 | `liberty` AbilData:2053 (`NexusTrain` Train1) | 303.6 | C | P2 |  |
| `grav_harvester` | Probe `ParticleBeam` | `weapon/period_ticks` | 15 | 1.5 | `liberty` WeaponData:147 | 26.79 | C | W2 |  |
| `grav_harvester` | Probe `ParticleBeam` | `weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `grav_harvester` | Probe `ParticleBeam` | `weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `grav_harvester` | Probe `ParticleBeam` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `ParticleBeam` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `grav_harvester` | Probe | `capabilities/harvester/harvest_rate_ticks` | 60 | 1.99 Faster s (2.786 Normal) | Liquipedia, Resources ("occupies a mineral field for 1.99 seconds"); not found in the game data (engine default) | 49.8 | C | M1 |  |
| `warp_strider` | Stalker | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `warp_strider` | Stalker | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:3961 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `warp_strider` | Stalker | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `warp_strider` | Stalker | `max_speed` | 0.1652 | 2.9531 | `liberty` UnitData:3963 | 0.1654 | A |  |  |
| `warp_strider` | Stalker | `acceleration` | 0.1652 | 1000 | `liberty` UnitData:3964 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `warp_strider` | Stalker | `deceleration` | 0.1652 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `warp_strider` | Stalker | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:3967 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `warp_strider` | Stalker | `build_time_ticks` | 675 | 38 | `voidmulti` AbilData:436 (`GatewayTrain` Train2) | 678.6 | C | P2 |  |
| `warp_strider` | Stalker `ParticleDisruptors` | `weapon/period_ticks` | 34 | 1.87 | `voidmulti` WeaponData:201 | 33.39 | C | W2 |  |
| `warp_strider` | Stalker `ParticleDisruptors` | `weapon/damage_point_ticks` | 3 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | A |  |  |
| `warp_strider` | Stalker `ParticleDisruptors` | `weapon/backswing_ticks` | 4 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `warp_strider` | Stalker `ParticleDisruptors` | `weapon/projectile_speed` | Some(0.75) | 18.75 | `core` MoverData:17 (mover `StalkerWeapon (parent `MissileDefault`)`, `MaxSpeed`) | 1.050 | B | W6 |  |
| `warp_strider` | Stalker `ParticleDisruptors` | `air_weapon/period_ticks` | 34 | 1.87 | `voidmulti` WeaponData:201 | 33.39 | C | W2 |  |
| `warp_strider` | Stalker `ParticleDisruptors` | `air_weapon/damage_point_ticks` | 3 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | A |  |  |
| `warp_strider` | Stalker `ParticleDisruptors` | `air_weapon/backswing_ticks` | 4 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `warp_strider` | Stalker `ParticleDisruptors` | `air_weapon/projectile_speed` | Some(0.75) | 18.75 | `core` MoverData:17 (mover `StalkerWeapon (parent `MissileDefault`)`, `MaxSpeed`) | 1.050 | B | W6 |  |
| `prism_weaver` | Sentry | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `prism_weaver` | Sentry | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:11301 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `prism_weaver` | Sentry | `energy_regen_per_tick` | 0.056 | 0.5625 | `liberty` UnitData:11298 | 0.0315 | C | R3 |  |
| `prism_weaver` | Sentry | `max_speed` | 0.126 | 2.5 | `voidmulti` UnitData:1552 | 0.14 | C | S2 |  |
| `prism_weaver` | Sentry | `acceleration` | 0.126 | 1000 | `liberty` UnitData:11304 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `prism_weaver` | Sentry | `deceleration` | 0.126 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `prism_weaver` | Sentry | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:11386 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `prism_weaver` | Sentry | `build_time_ticks` | 650 | 32 | `voidmulti` AbilData:437 (`GatewayTrain` Train6) | 571.4 | C | P3 |  |
| `prism_weaver` | Sentry `DisruptionBeam` | `weapon/period_ticks` | 18 | 1 | `liberty` WeaponData:544 | 17.86 | A |  |  |
| `prism_weaver` | Sentry `DisruptionBeam` | `weapon/damage_point_ticks` | 0 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `prism_weaver` | Sentry `DisruptionBeam` | `weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `prism_weaver` | Sentry `DisruptionBeam` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `DisruptionBeam` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `astral_vagrant` | Adept | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `astral_vagrant` | Adept | `shield_regen_per_tick` | 0.08 | 2 | `void` UnitData:3198 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `astral_vagrant` | Adept | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `astral_vagrant` | Adept | `max_speed` | 0.14 | 2.5 | `void` UnitData:3200 | 0.14 | A |  |  |
| `astral_vagrant` | Adept | `acceleration` | 0.14 | 1000 | `void` UnitData:3201 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `astral_vagrant` | Adept | `deceleration` | 0.14 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `astral_vagrant` | Adept | `turn_rate_deg` | 999.0 | 999.8437 | `void` UnitData:3204 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `astral_vagrant` | Adept | `build_time_ticks` | 675 | 42 | `voidmulti` AbilData:438 (`GatewayTrain` Train7) | 750.0 | C | P3 |  |
| `astral_vagrant` | Adept `Adept` | `weapon/period_ticks` | 40 | 2.25 | `void` WeaponData:318 | 40.18 | A |  |  |
| `astral_vagrant` | Adept `Adept` | `weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `astral_vagrant` | Adept `Adept` | `weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `astral_vagrant` | Adept `Adept` | `weapon/projectile_speed` | Some(1.0) | 18.75 | `core` MoverData:17 (mover `AdeptWeapon (parent `MissileDefault`)`, `MaxSpeed`) | 1.050 | C | W7 |  |
| `cerebral_elder` | HighTemplar | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `cerebral_elder` | HighTemplar | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:3255 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `cerebral_elder` | HighTemplar | `energy_regen_per_tick` | 0.056 | 0.5625 | `liberty` UnitData:3252 | 0.0315 | C | R3 |  |
| `cerebral_elder` | HighTemplar | `max_speed` | 0.1048 | 2.0156 | `voidmulti` UnitData:685 | 0.1129 | C | S2 |  |
| `cerebral_elder` | HighTemplar | `acceleration` | 0.1048 | 1000 | `liberty` UnitData:3258 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `cerebral_elder` | HighTemplar | `deceleration` | 0.1048 | 1000 | `liberty` UnitData:3259 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `cerebral_elder` | HighTemplar | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:3320 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `cerebral_elder` | HighTemplar | `build_time_ticks` | 975 | 55 | `liberty` AbilData:2000 (`GatewayTrain` Train4) | 982.1 | C | P2 |  |
| `cerebral_elder` | HighTemplar `HighTemplarWeapon` | `weapon/period_ticks` | 31 | 1.754 | `voidmulti` WeaponData:292 | 31.32 | A |  |  |
| `cerebral_elder` | HighTemplar `HighTemplarWeapon` | `weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `cerebral_elder` | HighTemplar `HighTemplarWeapon` | `weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `cerebral_elder` | HighTemplar `HighTemplarWeapon` | `weapon/projectile_speed` | Some(1.0) | — | mover `HighTemplarWeaponMissile` of `HighTemplarWeaponLM` not in the data | — | E |  | **not found** |
| `shadow_visitor` | DarkTemplar | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `shadow_visitor` | DarkTemplar | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:3351 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `shadow_visitor` | DarkTemplar | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `shadow_visitor` | DarkTemplar | `max_speed` | 0.1576 | 2.8125 | `liberty` UnitData:3353 | 0.1575 | A |  |  |
| `shadow_visitor` | DarkTemplar | `acceleration` | 0.1576 | 1000 | `liberty` UnitData:3354 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `shadow_visitor` | DarkTemplar | `deceleration` | 0.1576 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `shadow_visitor` | DarkTemplar | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:3401 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `shadow_visitor` | DarkTemplar | `build_time_ticks` | 975 | 55 | `liberty` AbilData:2004 (`GatewayTrain` Train5) | 982.1 | C | P2 |  |
| `shadow_visitor` | DarkTemplar `WarpBlades` | `weapon/period_ticks` | 30 | 1.694 | `liberty` WeaponData:166 | 30.25 | A |  |  |
| `shadow_visitor` | DarkTemplar `WarpBlades` | `weapon/damage_point_ticks` | 1 | 0.361 | `liberty` WeaponData:167 | 6.45 | C | W4 |  |
| `shadow_visitor` | DarkTemplar `WarpBlades` | `weapon/backswing_ticks` | 4 | 1.333 | `liberty` WeaponData:168 | 23.80 | C | W5 |  |
| `shadow_visitor` | DarkTemplar `WarpBlades` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `WarpBlades` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `void_goliath` | Immortal | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `void_goliath` | Immortal | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:10735 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `void_goliath` | Immortal | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `void_goliath` | Immortal | `max_speed` | 0.126 | 2.25 | `liberty` UnitData:10737 | 0.126 | A |  |  |
| `void_goliath` | Immortal | `acceleration` | 0.126 | 1000 | `liberty` UnitData:10738 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `void_goliath` | Immortal | `deceleration` | 0.126 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `void_goliath` | Immortal | `turn_rate_deg` | 999.0 | 720 | `core` UnitData:40 (default `CUnit`) | 40.3 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `void_goliath` | Immortal | `build_time_ticks` | 975 | 55 | `swarm` AbilData:1131 (`RoboticsFacilityTrain` Train4) | 982.1 | C | P2 |  |
| `void_goliath` | Immortal `PhaseDisruptors` | `weapon/period_ticks` | 28 | 1.6 | `voidmulti` WeaponData:208 | 28.57 | C | W2 |  |
| `void_goliath` | Immortal `PhaseDisruptors` | `weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `void_goliath` | Immortal `PhaseDisruptors` | `weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `void_goliath` | Immortal `PhaseDisruptors` | `weapon/projectile_speed` | Some(1.2) | instant | no launch-missile effect under `PhaseDisruptors` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | C | W7 | the reference applies its damage with no missile |
| `tripod_annihilator` | Colossus | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `tripod_annihilator` | Colossus | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:4039 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `tripod_annihilator` | Colossus | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `tripod_annihilator` | Colossus | `max_speed` | 0.126 | 2.25 | `liberty` UnitData:4042 | 0.126 | A |  |  |
| `tripod_annihilator` | Colossus | `acceleration` | 0.126 | 1000 | `liberty` UnitData:4043 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `tripod_annihilator` | Colossus | `deceleration` | 0.126 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `tripod_annihilator` | Colossus | `turn_rate_deg` | 280.0 | 720 | `core` UnitData:40 (default `CUnit`) | 40.3 deg/tick | D |  | ours is read as degrees per tick, so 280 = instant (catalog.rs `turn_rate`) |
| `tripod_annihilator` | Colossus | `build_time_ticks` | 1350 | 75 | `liberty` AbilData:2040 (`RoboticsFacilityTrain` Train3) | 1339.3 | C | P2 |  |
| `tripod_annihilator` | Colossus `ThermalLances` | `weapon/period_ticks` | 27 | 1.5 | `voidmulti` WeaponData:254 | 26.79 | A |  |  |
| `tripod_annihilator` | Colossus `ThermalLances` | `weapon/damage_point_ticks` | 0 | 0.0832 | `liberty` WeaponData:502 | 1.49 | C | W4 |  |
| `tripod_annihilator` | Colossus `ThermalLances` | `weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `tripod_annihilator` | Colossus `ThermalLances` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `ThermalLances` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `stealth_eye_orb` | Observer | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `stealth_eye_orb` | Observer | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:3419 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `stealth_eye_orb` | Observer | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `stealth_eye_orb` | Observer | `max_speed` | 0.1052 | 2.0156 | `voidmulti` UnitData:1134 | 0.1129 | C | S2 |  |
| `stealth_eye_orb` | Observer | `acceleration` | 0.1052 | 2.125 | `liberty` UnitData:3423 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `stealth_eye_orb` | Observer | `deceleration` | 0.1052 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `stealth_eye_orb` | Observer | `turn_rate_deg` | 999.0 | 720 | `core` UnitData:40 (default `CUnit`) | 40.3 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `stealth_eye_orb` | Observer | `build_time_ticks` | 525 | 25 | `voidmulti` AbilData:870 (`RoboticsFacilityTrain` Train2) | 446.4 | C | P3 |  |
| `phase_prism_saucer` | WarpPrism | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `phase_prism_saucer` | WarpPrism | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:3821 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `phase_prism_saucer` | WarpPrism | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `phase_prism_saucer` | WarpPrism | `max_speed` | 0.1652 | 2.9531 | `voidmulti` UnitData:2042 | 0.1654 | A |  |  |
| `phase_prism_saucer` | WarpPrism | `acceleration` | 0.1652 | 2.625 | `voidmulti` UnitData:2043 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `phase_prism_saucer` | WarpPrism | `deceleration` | 0.1652 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `phase_prism_saucer` | WarpPrism | `turn_rate_deg` | 999.0 | 720 | `core` UnitData:40 (default `CUnit`) | 40.3 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `phase_prism_saucer` | WarpPrism | `build_time_ticks` | 900 | 50 | `liberty` AbilData:2032 (`RoboticsFacilityTrain` Train1) | 892.9 | C | P2 |  |
| `prismatic_saucer` | VoidRay | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `prismatic_saucer` | VoidRay | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:3754 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `prismatic_saucer` | VoidRay | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `prismatic_saucer` | VoidRay | `max_speed` | 0.154 | 2.75 | `voidmulti` UnitData:2025 | 0.154 | A |  |  |
| `prismatic_saucer` | VoidRay | `acceleration` | 0.154 | 2 | `liberty` UnitData:3758 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `prismatic_saucer` | VoidRay | `deceleration` | 0.154 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `prismatic_saucer` | VoidRay | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:3760 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `prismatic_saucer` | VoidRay | `build_time_ticks` | 1075 | 60.2 | `voidmulti` AbilData:959 (`StargateTrain` Train5) | 1075.0 | A |  |  |
| `prismatic_saucer` | VoidRay `VoidRaySwarm` | `weapon/period_ticks` | 9 | 0.5 | `void` WeaponData:169 | 8.93 | A |  |  |
| `prismatic_saucer` | VoidRay `VoidRaySwarm` | `weapon/damage_point_ticks` | 0 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `prismatic_saucer` | VoidRay `VoidRaySwarm` | `weapon/backswing_ticks` | 1 | 0.75 | `void` WeaponData:170 | 13.39 | C | W5 |  |
| `prismatic_saucer` | VoidRay `VoidRaySwarm` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `VoidRaySwarm` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `prismatic_saucer` | VoidRay `VoidRaySwarm` | `air_weapon/period_ticks` | 9 | 0.5 | `void` WeaponData:169 | 8.93 | A |  |  |
| `prismatic_saucer` | VoidRay `VoidRaySwarm` | `air_weapon/damage_point_ticks` | 0 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `prismatic_saucer` | VoidRay `VoidRaySwarm` | `air_weapon/backswing_ticks` | 1 | 0.75 | `void` WeaponData:170 | 13.39 | C | W5 |  |
| `prismatic_saucer` | VoidRay `VoidRaySwarm` | `air_weapon/projectile_speed` | None | instant | no launch-missile effect under `VoidRaySwarm` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `mothership_carrier` | Carrier | `hp_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `mothership_carrier` | Carrier | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:3479 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `mothership_carrier` | Carrier | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `mothership_carrier` | Carrier | `max_speed` | 0.1048 | 1.875 | `liberty` UnitData:3482 | 0.105 | A |  |  |
| `mothership_carrier` | Carrier | `acceleration` | 0.1048 | 1.0625 | `liberty` UnitData:3483 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `mothership_carrier` | Carrier | `deceleration` | 0.1048 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `mothership_carrier` | Carrier | `turn_rate_deg` | 360.0 | 720 | `core` UnitData:40 (default `CUnit`) | 40.3 deg/tick | D |  | ours is read as degrees per tick, so 360 = instant (catalog.rs `turn_rate`) |
| `mothership_carrier` | Carrier | `build_time_ticks` | 1600 | 90 | `voidmulti` AbilData:958 (`StargateTrain` Train3) | 1607.1 | C | P2 |  |
| `mothership_carrier` | Carrier | `weapon/period_ticks` | 40 | — | Interceptors (`InterceptorBeam`, period 3 Normal s, `liberty` WeaponData:632) | — | E |  | not modelled: our single 8-strike weapon abstracts the interceptors |
| `mothership_carrier` | Carrier | `weapon/damage_point_ticks` | 1 | — | Interceptors (`InterceptorBeam`, period 3 Normal s, `liberty` WeaponData:632) | — | E |  | not modelled: our single 8-strike weapon abstracts the interceptors |
| `mothership_carrier` | Carrier | `weapon/backswing_ticks` | 4 | — | Interceptors (`InterceptorBeam`, period 3 Normal s, `liberty` WeaponData:632) | — | E |  | not modelled: our single 8-strike weapon abstracts the interceptors |
| `mothership_carrier` | Carrier | `weapon/projectile_speed` | Some(1.2) | — | Interceptors (`InterceptorBeam`, period 3 Normal s, `liberty` WeaponData:632) | — | E |  | not modelled: our single 8-strike weapon abstracts the interceptors |
| `nectar_worker` | Drone | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:7038 | 0.01531 | C | R2 |  |
| `nectar_worker` | Drone | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `nectar_worker` | Drone | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `nectar_worker` | Drone | `max_speed` | 0.1124 | 2.8125 | `liberty` UnitData:7040 | 0.1575 | B | S1 |  |
| `nectar_worker` | Drone | `acceleration` | 0.1124 | 2.5 | `liberty` UnitData:7041 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `nectar_worker` | Drone | `deceleration` | 0.1124 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `nectar_worker` | Drone | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:7122 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `nectar_worker` | Drone | `build_time_ticks` | 300 | 17 | `liberty` AbilData:2481 (`LarvaTrain` Train1) | 303.6 | C | P2 |  |
| `nectar_worker` | Drone `Spines` | `weapon/period_ticks` | 15 | 1.5 | `liberty` WeaponData:354 | 26.79 | C | W2 |  |
| `nectar_worker` | Drone `Spines` | `weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `nectar_worker` | Drone `Spines` | `weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `nectar_worker` | Drone `Spines` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `Spines` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `nectar_worker` | Drone | `capabilities/harvester/harvest_rate_ticks` | 60 | 1.99 Faster s (2.786 Normal) | Liquipedia, Resources ("occupies a mineral field for 1.99 seconds"); not found in the game data (engine default) | 49.8 | C | M1 |  |
| `razor_mantis` | Zergling | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:7550 | 0.01531 | C | R2 |  |
| `razor_mantis` | Zergling | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `razor_mantis` | Zergling | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `razor_mantis` | Zergling | `max_speed` | 0.1652 | 2.9531 | `liberty` UnitData:7552 | 0.1654 | A |  |  |
| `razor_mantis` | Zergling | `acceleration` | 0.1652 | 1000 | `liberty` UnitData:7554 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `razor_mantis` | Zergling | `deceleration` | 0.1652 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `razor_mantis` | Zergling | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:7557 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `razor_mantis` | Zergling | `build_time_ticks` | 425 | 24 | `liberty` AbilData:2485 (`LarvaTrain` Train2) | 428.6 | C | P2 |  |
| `razor_mantis` | Zergling `Claws` | `weapon/period_ticks` | 12 | 0.696 | `liberty` WeaponData:362 | 12.43 | A |  |  |
| `razor_mantis` | Zergling `Claws` | `weapon/damage_point_ticks` | 2 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `razor_mantis` | Zergling `Claws` | `weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `razor_mantis` | Zergling `Claws` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `Claws` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `volatile_tick` | Baneling | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:9307 | 0.01531 | C | R2 |  |
| `volatile_tick` | Baneling | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `volatile_tick` | Baneling | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `volatile_tick` | Baneling | `max_speed` | 0.14 | 2.5 | `liberty` UnitData:9309 | 0.14 | A |  |  |
| `volatile_tick` | Baneling | `acceleration` | 0.14 | 1000 | `liberty` UnitData:9311 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `volatile_tick` | Baneling | `deceleration` | 0.14 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `volatile_tick` | Baneling | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:9314 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `volatile_tick` | Baneling | `build_time_ticks` | 350 | 20 | `voidmulti` AbilData:1720 (`MorphToBaneling`, section Stats delay) | 357.1 | C | P2 |  |
| `volatile_tick` | Baneling `VolatileBurst` | `weapon/period_ticks` | 10 | 0.833 | `liberty` WeaponData:468 | 14.88 | C | W2 |  |
| `volatile_tick` | Baneling `VolatileBurst` | `weapon/damage_point_ticks` | 1 | 0 | `liberty` WeaponData:469 | 0.00 | C | W4 |  |
| `volatile_tick` | Baneling `VolatileBurst` | `weapon/backswing_ticks` | 1 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `volatile_tick` | Baneling `VolatileBurst` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `VolatileBurst` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `mortar_stag` | Ravager | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:991 | 0.01531 | C | R2 |  |
| `mortar_stag` | Ravager | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `mortar_stag` | Ravager | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `mortar_stag` | Ravager | `max_speed` | 0.154 | 2.75 | `liberty` UnitData:993 | 0.154 | A |  |  |
| `mortar_stag` | Ravager | `acceleration` | 0.154 | 1000 | `liberty` UnitData:995 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `mortar_stag` | Ravager | `deceleration` | 0.154 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `mortar_stag` | Ravager | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:998 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `mortar_stag` | Ravager | `build_time_ticks` | 225 | 12 | `void` AbilData:239 (`MorphToRavager`, section Stats delay) | 214.3 | C | P2 |  |
| `mortar_stag` | Ravager `RavagerWeapon` | `weapon/period_ticks` | 28 | 1.6 | `voidmulti` WeaponData:214 | 28.57 | C | W2 |  |
| `mortar_stag` | Ravager `RavagerWeapon` | `weapon/damage_point_ticks` | 1 | 0.2 | `void` WeaponData:145 | 3.57 | C | W4 |  |
| `mortar_stag` | Ravager `RavagerWeapon` | `weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `mortar_stag` | Ravager `RavagerWeapon` | `weapon/projectile_speed` | Some(1.0) | 18.75 | `core` MoverData:17 (mover `MissileDefault`, `MaxSpeed`) | 1.050 | C | W7 |  |
| `thorn_viper` | Hydralisk | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:7671 | 0.01531 | C | R2 |  |
| `thorn_viper` | Hydralisk | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `thorn_viper` | Hydralisk | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `thorn_viper` | Hydralisk | `max_speed` | 0.126 | 2.25 | `liberty` UnitData:7673 | 0.126 | A |  |  |
| `thorn_viper` | Hydralisk | `acceleration` | 0.126 | 1000 | `liberty` UnitData:7675 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `thorn_viper` | Hydralisk | `deceleration` | 0.126 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `thorn_viper` | Hydralisk | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:7679 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `thorn_viper` | Hydralisk | `build_time_ticks` | 600 | 33 | `liberty` AbilData:2494 (`LarvaTrain` Train4) | 589.3 | C | P2 |  |
| `thorn_viper` | Hydralisk `NeedleSpines` | `weapon/period_ticks` | 15 | 0.825 | `voidmulti` WeaponData:155 | 14.73 | A |  |  |
| `thorn_viper` | Hydralisk `NeedleSpines` | `weapon/damage_point_ticks` | 1 | 0.14 | `voidmulti` WeaponData:157 | 2.50 | C | W4 |  |
| `thorn_viper` | Hydralisk `NeedleSpines` | `weapon/backswing_ticks` | 2 | 0.5607 | `voidmulti` WeaponData:158 | 10.01 | C | W5 |  |
| `thorn_viper` | Hydralisk `NeedleSpines` | `weapon/projectile_speed` | Some(0.90) | 40 | `liberty` MoverData:606 (mover `NeedleSpinesWeapon`, `MaxSpeed`) | 2.240 | C | W7 |  |
| `thorn_viper` | Hydralisk `NeedleSpines` | `air_weapon/period_ticks` | 15 | 0.825 | `voidmulti` WeaponData:155 | 14.73 | A |  |  |
| `thorn_viper` | Hydralisk `NeedleSpines` | `air_weapon/damage_point_ticks` | 1 | 0.14 | `voidmulti` WeaponData:157 | 2.50 | C | W4 |  |
| `thorn_viper` | Hydralisk `NeedleSpines` | `air_weapon/backswing_ticks` | 2 | 0.5607 | `voidmulti` WeaponData:158 | 10.01 | C | W5 |  |
| `thorn_viper` | Hydralisk `NeedleSpines` | `air_weapon/projectile_speed` | Some(0.90) | 40 | `liberty` MoverData:606 (mover `NeedleSpinesWeapon`, `MaxSpeed`) | 2.240 | C | W7 |  |
| `burrow_scorpion` | LurkerMP | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:710 | 0.01531 | C | R2 |  |
| `burrow_scorpion` | LurkerMP | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `burrow_scorpion` | LurkerMP | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `burrow_scorpion` | LurkerMP | `max_speed` | 0.1652 | 2.9531 | `void` UnitData:753 | 0.1654 | A |  |  |
| `burrow_scorpion` | LurkerMP | `acceleration` | 0.1652 | 1000 | `liberty` UnitData:714 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `burrow_scorpion` | LurkerMP | `deceleration` | 0.1652 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `burrow_scorpion` | LurkerMP | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:717 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `burrow_scorpion` | LurkerMP | `build_time_ticks` | 450 | 25 | `void` AbilData:269 (`MorphToLurker`, section Stats delay) | 446.4 | C | P2 |  |
| `burrow_scorpion` | LurkerMP `LurkerMP` | `weapon/period_ticks` | 36 | 2 | `swarm` WeaponData:221 | 35.71 | A |  |  |
| `burrow_scorpion` | LurkerMP `LurkerMP` | `weapon/damage_point_ticks` | 1 | 0 | `void` WeaponData:15 | 0.00 | C | W4 |  |
| `burrow_scorpion` | LurkerMP `LurkerMP` | `weapon/backswing_ticks` | 4 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `burrow_scorpion` | LurkerMP `LurkerMP` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `LurkerMP` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `sonic_bat` | Mutalisk | `hp_regen_per_tick` | 0.02 | 1 | `void` UnitData:3094 | 0.056 | C | R2 |  |
| `sonic_bat` | Mutalisk | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `sonic_bat` | Mutalisk | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `sonic_bat` | Mutalisk | `max_speed` | 0.224 | 4 | `void` UnitData:3093 | 0.224 | A |  |  |
| `sonic_bat` | Mutalisk | `acceleration` | 0.224 | 3.5 | `void` UnitData:3092 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `sonic_bat` | Mutalisk | `deceleration` | 0.224 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `sonic_bat` | Mutalisk | `turn_rate_deg` | 999.0 | 1499.9414 | `liberty` UnitData:7821 | 84.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `sonic_bat` | Mutalisk | `build_time_ticks` | 600 | 33 | `liberty` AbilData:2498 (`LarvaTrain` Train5) | 589.3 | C | P2 |  |
| `sonic_bat` | Mutalisk `GlaiveWurm` | `weapon/period_ticks` | 27 | 1.5246 | `liberty` WeaponData:404 | 27.22 | A |  |  |
| `sonic_bat` | Mutalisk `GlaiveWurm` | `weapon/damage_point_ticks` | 1 | 0 | `liberty` WeaponData:405 | 0.00 | C | W4 |  |
| `sonic_bat` | Mutalisk `GlaiveWurm` | `weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `sonic_bat` | Mutalisk `GlaiveWurm` | `weapon/projectile_speed` | Some(0.50) | 18.75 | `core` MoverData:17 (mover `MissileDefault`, `MaxSpeed`) | 1.050 | C | W7 |  |
| `sonic_bat` | Mutalisk `GlaiveWurm` | `air_weapon/period_ticks` | 27 | 1.5246 | `liberty` WeaponData:404 | 27.22 | A |  |  |
| `sonic_bat` | Mutalisk `GlaiveWurm` | `air_weapon/damage_point_ticks` | 1 | 0 | `liberty` WeaponData:405 | 0.00 | C | W4 |  |
| `sonic_bat` | Mutalisk `GlaiveWurm` | `air_weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `sonic_bat` | Mutalisk `GlaiveWurm` | `air_weapon/projectile_speed` | Some(0.50) | 18.75 | `core` MoverData:17 (mover `MissileDefault`, `MaxSpeed`) | 1.050 | C | W7 |  |
| `sky_leviathan` | Corruptor | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:11218 | 0.01531 | C | R2 |  |
| `sky_leviathan` | Corruptor | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `sky_leviathan` | Corruptor | `energy_regen_per_tick` | 0.000 | 0 | `swarm` UnitData:1569 | 0 | A |  |  |
| `sky_leviathan` | Corruptor | `max_speed` | 0.1888 | 3.375 | `voidmulti` UnitData:268 | 0.189 | A |  |  |
| `sky_leviathan` | Corruptor | `acceleration` | 0.1888 | 3 | `voidmulti` UnitData:269 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `sky_leviathan` | Corruptor | `deceleration` | 0.1888 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `sky_leviathan` | Corruptor | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:11224 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `sky_leviathan` | Corruptor | `build_time_ticks` | 725 | 40 | `liberty` AbilData:2518 (`LarvaTrain` Train12) | 714.3 | C | P2 |  |
| `sky_leviathan` | Corruptor `ParasiteSpore` | `air_weapon/period_ticks` | 34 | 1.9 | `liberty` WeaponData:568 | 33.93 | A |  |  |
| `sky_leviathan` | Corruptor `ParasiteSpore` | `air_weapon/damage_point_ticks` | 1 | 0.0625 | `voidmulti` WeaponData:192 | 1.12 | A |  |  |
| `sky_leviathan` | Corruptor `ParasiteSpore` | `air_weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `sky_leviathan` | Corruptor `ParasiteSpore` | `air_weapon/projectile_speed` | Some(1.2) | 18.75 | `core` MoverData:17 (mover `MissileDefault`, `MaxSpeed`) | 1.050 | C | W7 |  |
| `sky_leviathan` | Corruptor `ParasiteSpore` | `weapon/period_ticks` | 34 | 1.9 | `liberty` WeaponData:568 | 33.93 | A |  |  |
| `sky_leviathan` | Corruptor `ParasiteSpore` | `weapon/damage_point_ticks` | 1 | 0.0625 | `voidmulti` WeaponData:192 | 1.12 | A |  |  |
| `sky_leviathan` | Corruptor `ParasiteSpore` | `weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `sky_leviathan` | Corruptor `ParasiteSpore` | `weapon/projectile_speed` | Some(1.2) | 18.75 | `core` MoverData:17 (mover `MissileDefault`, `MaxSpeed`) | 1.050 | C | W7 |  |
| `spore_matriarch` | BroodLord | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:11109 | 0.01531 | C | R2 |  |
| `spore_matriarch` | BroodLord | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `spore_matriarch` | BroodLord | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `spore_matriarch` | BroodLord | `max_speed` | 0.0788 | 1.875 | `voidmulti` UnitData:143 | 0.105 | C | S2 |  |
| `spore_matriarch` | BroodLord | `acceleration` | 0.0788 | 1.0625 | `liberty` UnitData:11113 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `spore_matriarch` | BroodLord | `deceleration` | 0.0788 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `spore_matriarch` | BroodLord | `turn_rate_deg` | 360.0 | 720 | `core` UnitData:40 (default `CUnit`) | 40.3 deg/tick | D |  | ours is read as degrees per tick, so 360 = instant (catalog.rs `turn_rate`) |
| `spore_matriarch` | BroodLord | `build_time_ticks` | 600 | 33.8332 | `liberty` AbilData:2548 (`MorphToBroodLord`, section Stats delay) | 604.2 | C | P2 |  |
| `spore_matriarch` | BroodLord `BroodlingStrike` | `ground_weapon/period_ticks` | 45 | 2.5 | `liberty` WeaponData:417 | 44.64 | A |  |  |
| `spore_matriarch` | BroodLord `BroodlingStrike` | `ground_weapon/damage_point_ticks` | 2 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `spore_matriarch` | BroodLord `BroodlingStrike` | `ground_weapon/backswing_ticks` | 4 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `spore_matriarch` | BroodLord `BroodlingStrike` | `ground_weapon/projectile_speed` | Some(0.9) | — | weapon effect empty (`liberty` WeaponData:419) | — | E |  | not modelled: the attack spawns Broodlings, not a missile |
| `spore_matriarch` | BroodLord `BroodlingStrike` | `weapon/period_ticks` | 45 | 2.5 | `liberty` WeaponData:417 | 44.64 | A |  |  |
| `spore_matriarch` | BroodLord `BroodlingStrike` | `weapon/damage_point_ticks` | 2 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `spore_matriarch` | BroodLord `BroodlingStrike` | `weapon/backswing_ticks` | 4 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `spore_matriarch` | BroodLord `BroodlingStrike` | `weapon/projectile_speed` | Some(0.9) | — | weapon effect empty (`liberty` WeaponData:419) | — | E |  | not modelled: the attack spawns Broodlings, not a missile |
| `stag_behemoth` | Ultralisk | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8011 | 0.01531 | C | R2 |  |
| `stag_behemoth` | Ultralisk | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `stag_behemoth` | Ultralisk | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `stag_behemoth` | Ultralisk | `max_speed` | 0.1652 | 2.9531 | `swarm` UnitData:1994 | 0.1654 | A |  |  |
| `stag_behemoth` | Ultralisk | `acceleration` | 0.1652 | 1000 | `liberty` UnitData:8015 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `stag_behemoth` | Ultralisk | `deceleration` | 0.1652 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `stag_behemoth` | Ultralisk | `turn_rate_deg` | 999.0 | 360 | `liberty` UnitData:8037 | 20.2 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `stag_behemoth` | Ultralisk | `build_time_ticks` | 975 | 55 | `swarm` AbilData:1307 (`LarvaTrain` Train7) | 982.1 | C | P2 |  |
| `stag_behemoth` | Ultralisk `KaiserBlades` | `weapon/period_ticks` | 15 | 0.86 | `void` WeaponData:208 | 15.36 | A |  |  |
| `stag_behemoth` | Ultralisk `KaiserBlades` | `weapon/damage_point_ticks` | 1 | 0.3332 | `liberty` WeaponData:438 | 5.95 | C | W4 |  |
| `stag_behemoth` | Ultralisk `KaiserBlades` | `weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `stag_behemoth` | Ultralisk `KaiserBlades` | `weapon/projectile_speed` | None | instant | no launch-missile effect under `KaiserBladesDamage` (`swarm` WeaponData:184) | None | A |  |  |
| `hive_matron` | Queen | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:11412 | 0.01531 | C | R2 |  |
| `hive_matron` | Queen | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `hive_matron` | Queen | `energy_regen_per_tick` | 0.056 | 0.5625 | `liberty` UnitData:11416 | 0.0315 | C | R3 |  |
| `hive_matron` | Queen | `max_speed` | 0.0524 | 0.9375 | `swarm` UnitData:2283 | 0.0525 | A |  |  |
| `hive_matron` | Queen | `acceleration` | 0.0524 | 1000 | `liberty` UnitData:11420 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `hive_matron` | Queen | `deceleration` | 0.0524 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `hive_matron` | Queen | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:11423 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `hive_matron` | Queen | `build_time_ticks` | 900 | 50 | `liberty` AbilData:3496 (`TrainQueen` Train1) | 892.9 | C | P2 |  |
| `hive_matron` | Queen `TalonsMissile` | `ground_weapon/period_ticks` | 18 | 1 | `swarm` WeaponData:236 | 17.86 | A |  |  |
| `hive_matron` | Queen `TalonsMissile` | `ground_weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `hive_matron` | Queen `TalonsMissile` | `ground_weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `hive_matron` | Queen `TalonsMissile` | `ground_weapon/projectile_speed` | None | 40 | `swarm` MoverData:339 (mover `TalonsMissileWeapon`, `MaxSpeed`) | 2.240 | C | W7 | ours is instant; the reference fires a missile |
| `hive_matron` | Queen `AcidSpines` | `air_weapon/period_ticks` | 18 | 1 | `liberty` WeaponData:106 | 17.86 | A |  |  |
| `hive_matron` | Queen `AcidSpines` | `air_weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `hive_matron` | Queen `AcidSpines` | `air_weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `hive_matron` | Queen `AcidSpines` | `air_weapon/projectile_speed` | Some(1.2) | 40 | `liberty` MoverData:614 (mover `AcidSpinesWeapon`, `MaxSpeed`) | 2.240 | C | W7 |  |
| `hive_matron` | Queen `TalonsMissile` | `weapon/period_ticks` | 18 | 1 | `swarm` WeaponData:236 | 17.86 | A |  |  |
| `hive_matron` | Queen `TalonsMissile` | `weapon/damage_point_ticks` | 1 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `hive_matron` | Queen `TalonsMissile` | `weapon/backswing_ticks` | 2 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `hive_matron` | Queen `TalonsMissile` | `weapon/projectile_speed` | None | 40 | `swarm` MoverData:339 (mover `TalonsMissileWeapon`, `MaxSpeed`) | 2.240 | C | W7 | ours is instant; the reference fires a missile |
| `mycelium_shaman` | Infestor | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:10055 | 0.01531 | C | R2 |  |
| `mycelium_shaman` | Infestor | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `mycelium_shaman` | Infestor | `energy_regen_per_tick` | 0.056 | 0.5625 | `liberty` UnitData:10059 | 0.0315 | C | R3 |  |
| `mycelium_shaman` | Infestor | `max_speed` | 0.126 | 2.25 | `swarm` UnitData:1680 | 0.126 | A |  |  |
| `mycelium_shaman` | Infestor | `acceleration` | 0.126 | 1000 | `liberty` UnitData:10062 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `mycelium_shaman` | Infestor | `deceleration` | 0.126 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `mycelium_shaman` | Infestor | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:10065 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `mycelium_shaman` | Infestor | `build_time_ticks` | 900 | 50 | `liberty` AbilData:2514 (`LarvaTrain` Train11) | 892.9 | C | P2 |  |
| `tongue_chameleon` | Viper | `hp_regen_per_tick` | 0.02 | 0.2734 | `swarm` UnitData:2332 | 0.01531 | C | R2 |  |
| `tongue_chameleon` | Viper | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `tongue_chameleon` | Viper | `energy_regen_per_tick` | 0.056 | 0.5625 | `swarm` UnitData:2336 | 0.0315 | C | R3 |  |
| `tongue_chameleon` | Viper | `max_speed` | 0.1652 | 2.9531 | `void` UnitData:1416 | 0.1654 | A |  |  |
| `tongue_chameleon` | Viper | `acceleration` | 0.1652 | 3 | `void` UnitData:1417 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `tongue_chameleon` | Viper | `deceleration` | 0.1652 | not set | not set | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `tongue_chameleon` | Viper | `turn_rate_deg` | 999.0 | 999.8437 | `swarm` UnitData:2342 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `tongue_chameleon` | Viper | `build_time_ticks` | 725 | 40 | `swarm` AbilData:1312 (`LarvaTrain` Train13) | 714.3 | C | P2 |  |
| `amber_pollen_dome` | Overlord | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:7362 | 0.01531 | C | R2 |  |
| `amber_pollen_dome` | Overlord | `shield_regen_per_tick` | 0.00 | 0 | not set in any layer (0) | 0 | A |  |  |
| `amber_pollen_dome` | Overlord | `energy_regen_per_tick` | 0.000 | 0 | not set in any layer (0) | 0 | A |  |  |
| `amber_pollen_dome` | Overlord | `max_speed` | 0.036 | 0.6445 | `voidmulti` UnitData:1194 | 0.0361 | A |  |  |
| `amber_pollen_dome` | Overlord | `acceleration` | 0.036 | 1.0625 | `liberty` UnitData:7366 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `amber_pollen_dome` | Overlord | `deceleration` | 0.036 | 1.625 | `voidmulti` UnitData:1199 | — | E |  | parsed, unused by the simulation (speed changes are instant); the catalog copies max_speed |
| `amber_pollen_dome` | Overlord | `turn_rate_deg` | 999.0 | 999.8437 | `liberty` UnitData:7369 | 56.0 deg/tick | D |  | ours is read as degrees per tick, so 999 = instant (catalog.rs `turn_rate`) |
| `amber_pollen_dome` | Overlord | `build_time_ticks` | 450 | 25 | `liberty` AbilData:2490 (`LarvaTrain` Train3) | 446.4 | C | P2 |  |

### `structures.ron`

| Identity | SC2 | Field | Ours | Editor (Normal) | Source | Reference (ours) | Class | Group | Note |
|---|---|---|---|---|---|---|---|---|---|
| `foundry_core_dome` | CommandCenter | `build_time_ticks` | 1775 | 100 | `liberty` AbilData:1172 (`TerranBuild` Build1) | 1785.7 | C | P2 |  |
| `orbital_relay_dome` | OrbitalCommand | `build_time_ticks` | 625 | 35 | `liberty` AbilData:3250 (`UpgradeToOrbital`, section Stats delay) | 625.0 | A |  |  |
| `planetary_bulwark_dome` | PlanetaryFortress | `build_time_ticks` | 1250 | 50 | `liberty` AbilData:3163 (`UpgradeToPlanetaryFortress`, section Stats delay) | 892.9 | B | P1 |  |
| `planetary_bulwark_dome` | PlanetaryFortress turret `PlanetaryFortress` | `ground_weapon/turret_turn_rate_deg` | 360.0 | 90 | `liberty` TurretData:89 | 5.04 deg/tick | C | W8 | ours = 360 deg/tick (values above 360 are read as deg/s) |
| `planetary_bulwark_dome` | PlanetaryFortress `TwinIbiksCannon` | `ground_weapon/period_ticks` | 50 | 2 | `liberty` WeaponData:139 | 35.71 | B | W1 |  |
| `planetary_bulwark_dome` | PlanetaryFortress `TwinIbiksCannon` | `ground_weapon/damage_point_ticks` | 3 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | A |  |  |
| `planetary_bulwark_dome` | PlanetaryFortress `TwinIbiksCannon` | `ground_weapon/backswing_ticks` | 5 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `planetary_bulwark_dome` | PlanetaryFortress `TwinIbiksCannon` | `ground_weapon/projectile_speed` | None | instant | no launch-missile effect under `TwinIbiksCannon` (`core` WeaponData:37 (default `CWeaponLegacy`)) | None | A |  |  |
| `capacitor_dome` | SupplyDepot | `build_time_ticks` | 525 | 30 | `liberty` AbilData:1175 (`TerranBuild` Build2) | 535.7 | C | P2 |  |
| `plasma_siphon_dome` | Refinery | `build_time_ticks` | 525 | 30 | `liberty` AbilData:1178 (`TerranBuild` Build3) | 535.7 | C | P2 |  |
| `assembly_bay_dome` | Barracks | `build_time_ticks` | 1150 | 65 | `swarm` AbilData:1179 (`TerranBuild` Build4) | 1160.7 | C | P2 |  |
| `mechanics_lab_dome` | EngineeringBay | `build_time_ticks` | 625 | 35 | `liberty` AbilData:1185 (`TerranBuild` Build5) | 625.0 | A |  |  |
| `fortress_bunker_dome` | Bunker | `build_time_ticks` | 725 | 40 | `swarm` AbilData:1182 (`TerranBuild` Build7) | 714.3 | C | P2 |  |
| `radar_beacon_dome` | SensorTower | `build_time_ticks` | 450 | 25 | `liberty` AbilData:1198 (`TerranBuild` Build9) | 446.4 | C | P2 |  |
| `flak_array_dome` | MissileTurret | `build_time_ticks` | 450 | 25 | `liberty` AbilData:1188 (`TerranBuild` Build6) | 446.4 | C | P2 |  |
| `flak_array_dome` | MissileTurret turret `MissileTurret` | `air_weapon/turret_turn_rate_deg` | 360.0 | 720 | `liberty` TurretData:26 (parent `FreeRotate`) | 40.32 deg/tick | C | W8 | ours = 360 deg/tick (values above 360 are read as deg/s) |
| `flak_array_dome` | MissileTurret `LongboltMissile` | `air_weapon/period_ticks` | 15 | 0.8608 | `liberty` WeaponData:307 | 15.37 | A |  |  |
| `flak_array_dome` | MissileTurret `LongboltMissile` | `air_weapon/damage_point_ticks` | 2 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `flak_array_dome` | MissileTurret `LongboltMissile` | `air_weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `flak_array_dome` | MissileTurret `LongboltMissile` | `air_weapon/projectile_speed` | Some(0.80) | 10 | `liberty` MoverData:593 (mover `LongboltMissileWeapon`, `MaxSpeed`) | 0.560 | C | W7 |  |
| `machining_plant_dome` | Factory | `build_time_ticks` | 1075 | 60 | `liberty` AbilData:1204 (`TerranBuild` Build11) | 1071.4 | C | P2 |  |
| `covert_ops_nexus_dome` | GhostAcademy | `build_time_ticks` | 725 | 40 | `liberty` AbilData:1201 (`TerranBuild` Build10) | 714.3 | C | P2 |  |
| `aerospace_hangar_dome` | Starport | `build_time_ticks` | 900 | 50 | `liberty` AbilData:1207 (`TerranBuild` Build12) | 892.9 | C | P2 |  |
| `weapons_arsenal_dome` | Armory | `build_time_ticks` | 1150 | 65 | `liberty` AbilData:1213 (`TerranBuild` Build14) | 1160.7 | C | P2 |  |
| `fusion_reactor_dome` | FusionCore | `build_time_ticks` | 1150 | 65 | `swarm` AbilData:1186 (`TerranBuild` Build16) | 1160.7 | C | P2 |  |
| `tech_module_pod` | BarracksTechLab | `build_time_ticks` | 450 | 25 | `liberty` AbilData:1520 (`BarracksAddOns` Build1) | 446.4 | C | P2 |  |
| `duplicator_module_pod` | BarracksReactor | `build_time_ticks` | 450 | 50 | `liberty` AbilData:1523 (`BarracksAddOns` Build2) | 892.9 | C | P3 |  |
| `citadel_nexus_dome` | Nexus | `build_time_ticks` | 1775 | 100 | `liberty` AbilData:1925 (`ProtossBuild` Build1) | 1785.7 | C | P2 |  |
| `citadel_nexus_dome` | Nexus | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:4178 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `psi_pylon_dome` | Pylon | `build_time_ticks` | 450 | 25 | `liberty` AbilData:1928 (`ProtossBuild` Build2) | 446.4 | C | P2 |  |
| `psi_pylon_dome` | Pylon | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:4329 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `rift_siphon_dome` | Assimilator | `build_time_ticks` | 450 | 30 | `liberty` AbilData:1931 (`ProtossBuild` Build3) | 535.7 | C | P3 |  |
| `rift_siphon_dome` | Assimilator | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:4117 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `warp_gate_dome` | Gateway | `build_time_ticks` | 1150 | 65 | `liberty` AbilData:1935 (`ProtossBuild` Build4) | 1160.7 | C | P2 |  |
| `warp_gate_dome` | Gateway | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:4389 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `crystalline_forge_dome` | Forge | `build_time_ticks` | 800 | 45 | `swarm` AbilData:1106 (`ProtossBuild` Build5) | 803.6 | C | P2 |  |
| `crystalline_forge_dome` | Forge | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:4531 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `tesla_sentinel_dome` | PhotonCannon | `build_time_ticks` | 725 | 40 | `liberty` AbilData:1947 (`ProtossBuild` Build8) | 714.3 | C | P2 |  |
| `tesla_sentinel_dome` | PhotonCannon | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:11049 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `tesla_sentinel_dome` | PhotonCannon turret `PhotonCannon` | `ground_weapon/turret_turn_rate_deg` | 360.0 | 720 | `liberty` TurretData:26 (parent `FreeRotate`) | 40.32 deg/tick | C | W8 | ours = 360 deg/tick (values above 360 are read as deg/s) |
| `tesla_sentinel_dome` | PhotonCannon `PhotonCannon` | `ground_weapon/period_ticks` | 22 | 1.25 | `liberty` WeaponData:535 | 22.32 | A |  |  |
| `tesla_sentinel_dome` | PhotonCannon `PhotonCannon` | `ground_weapon/damage_point_ticks` | 2 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `tesla_sentinel_dome` | PhotonCannon `PhotonCannon` | `ground_weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `tesla_sentinel_dome` | PhotonCannon `PhotonCannon` | `ground_weapon/projectile_speed` | None | 18 | `liberty` MoverData:411 (mover `PhotonCannonWeapon`, `MaxSpeed`) | 1.008 | C | W7 | ours is instant; the reference fires a missile |
| `tesla_sentinel_dome` | PhotonCannon turret `PhotonCannon` | `air_weapon/turret_turn_rate_deg` | 360.0 | 720 | `liberty` TurretData:26 (parent `FreeRotate`) | 40.32 deg/tick | C | W8 | ours = 360 deg/tick (values above 360 are read as deg/s) |
| `tesla_sentinel_dome` | PhotonCannon `PhotonCannon` | `air_weapon/period_ticks` | 22 | 1.25 | `liberty` WeaponData:535 | 22.32 | A |  |  |
| `tesla_sentinel_dome` | PhotonCannon `PhotonCannon` | `air_weapon/damage_point_ticks` | 2 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `tesla_sentinel_dome` | PhotonCannon `PhotonCannon` | `air_weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `tesla_sentinel_dome` | PhotonCannon `PhotonCannon` | `air_weapon/projectile_speed` | None | 18 | `liberty` MoverData:411 (mover `PhotonCannonWeapon`, `MaxSpeed`) | 1.008 | C | W7 | ours is instant; the reference fires a missile |
| `harmonic_recharge_dome` | ShieldBattery | `build_time_ticks` | 725 | 40 | `voidmulti` AbilData:848 (`ProtossBuild` Build16) | 714.3 | C | P2 |  |
| `harmonic_recharge_dome` | ShieldBattery | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:1597 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `cybernetics_conduit_dome` | CyberneticsCore | `build_time_ticks` | 800 | 50 | `liberty` AbilData:1966 (`ProtossBuild` Build15) | 892.9 | C | P3 |  |
| `cybernetics_conduit_dome` | CyberneticsCore | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:10812 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `templar_conclave_dome` | TwilightCouncil | `build_time_ticks` | 1150 | 50 | `liberty` AbilData:1944 (`ProtossBuild` Build7) | 892.9 | C | P3 |  |
| `templar_conclave_dome` | TwilightCouncil | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:4599 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `stellar_portal_dome` | Stargate | `build_time_ticks` | 1075 | 60 | `liberty` AbilData:1951 (`ProtossBuild` Build10) | 1071.4 | C | P2 |  |
| `stellar_portal_dome` | Stargate | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:9635 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `robotic_foundry_dome` | RoboticsFacility | `build_time_ticks` | 1150 | 65 | `liberty` AbilData:1963 (`ProtossBuild` Build14) | 1160.7 | C | P2 |  |
| `robotic_foundry_dome` | RoboticsFacility | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:9564 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `psionic_archives_dome` | TemplarArchive | `build_time_ticks` | 900 | 50 | `liberty` AbilData:1954 (`ProtossBuild` Build11) | 892.9 | C | P2 |  |
| `psionic_archives_dome` | TemplarArchive | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:4662 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `void_sanctuary_dome` | DarkShrine | `build_time_ticks` | 1775 | 100 | `liberty` AbilData:1957 (`ProtossBuild` Build12) | 1785.7 | C | P2 |  |
| `void_sanctuary_dome` | DarkShrine | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:9505 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `robotic_assembly_bay_dome` | RoboticsBay | `build_time_ticks` | 1150 | 65 | `liberty` AbilData:1960 (`ProtossBuild` Build13) | 1160.7 | C | P2 |  |
| `robotic_assembly_bay_dome` | RoboticsBay | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:9832 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `celestial_beacon_dome` | FleetBeacon | `build_time_ticks` | 1075 | 60 | `liberty` AbilData:1941 (`ProtossBuild` Build6) | 1071.4 | C | P2 |  |
| `celestial_beacon_dome` | FleetBeacon | `shield_regen_per_tick` | 0.08 | 2 | `liberty` UnitData:9703 | 0.112 | B | WP10 | changed by WP10 (Q0039) |
| `ancient_tree_dome` | Hatchery | `build_time_ticks` | 1775 | 100 | `liberty` AbilData:2196 (`ZergBuild` Build1) | 1785.7 | C | P2 |  |
| `ancient_tree_dome` | Hatchery | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8202 | 0.01531 | C | R2 |  |
| `canopy_grove_dome` | Lair | `build_time_ticks` | 1425 | 80 | `liberty` AbilData:2314 (`UpgradeToLair`, section Stats delay) | 1428.6 | C | P2 |  |
| `canopy_grove_dome` | Lair | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8284 | 0.01531 | C | R2 |  |
| `primal_world_tree_dome` | Hive | `build_time_ticks` | 1775 | 100 | `liberty` AbilData:2340 (`UpgradeToHive`, section Stats delay) | 1785.7 | C | P2 |  |
| `primal_world_tree_dome` | Hive | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8364 | 0.01531 | C | R2 |  |
| `spore_vent_dome` | Extractor | `build_time_ticks` | 525 | 30 | `liberty` AbilData:2202 (`ZergBuild` Build3) | 535.7 | C | P2 |  |
| `spore_vent_dome` | Extractor | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8144 | 0.01531 | C | R2 |  |
| `spawning_hollow_dome` | SpawningPool | `build_time_ticks` | 525 | 65 | `liberty` AbilData:2206 (`ZergBuild` Build4) | 1160.7 | C | P3 |  |
| `spawning_hollow_dome` | SpawningPool | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8881 | 0.01531 | C | R2 |  |
| `metamorphic_chamber_dome` | EvolutionChamber | `build_time_ticks` | 450 | 35 | `liberty` AbilData:2209 (`ZergBuild` Build5) | 625.0 | C | P3 |  |
| `metamorphic_chamber_dome` | EvolutionChamber | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8439 | 0.01531 | C | R2 |  |
| `thorn_lasher_dome` | SpineCrawler | `build_time_ticks` | 900 | 50 | `liberty` AbilData:2234 (`ZergBuild` Build15) | 892.9 | C | P2 |  |
| `thorn_lasher_dome` | SpineCrawler | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8610 | 0.01531 | C | R2 |  |
| `thorn_lasher_dome` | SpineCrawler turret `SpineCrawler` | `ground_weapon/turret_turn_rate_deg` | 360.0 | 360 | `liberty` TurretData:62 | 20.16 deg/tick | C | W8 | ours = 360 deg/tick (values above 360 are read as deg/s) |
| `thorn_lasher_dome` | SpineCrawler `ImpalerTentacle` | `ground_weapon/period_ticks` | 33 | 1.85 | `swarm` WeaponData:173 | 33.04 | A |  |  |
| `thorn_lasher_dome` | SpineCrawler `ImpalerTentacle` | `ground_weapon/damage_point_ticks` | 3 | 0.3332 | `liberty` WeaponData:371 | 5.95 | C | W4 |  |
| `thorn_lasher_dome` | SpineCrawler `ImpalerTentacle` | `ground_weapon/backswing_ticks` | 5 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `thorn_lasher_dome` | SpineCrawler `ImpalerTentacle` | `ground_weapon/projectile_speed` | None | — | mover `SpineCrawlerWeapon` of `ImpalerTentacleLM` not in the data | — | E |  | **not found** |
| `spore_spire_dome` | SporeCrawler | `build_time_ticks` | 450 | 30 | `liberty` AbilData:2237 (`ZergBuild` Build16) | 535.7 | C | P3 |  |
| `spore_spire_dome` | SporeCrawler | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8744 | 0.01531 | C | R2 |  |
| `spore_spire_dome` | SporeCrawler turret `SporeCrawler` | `air_weapon/turret_turn_rate_deg` | 360.0 | 360 | `liberty` TurretData:70 | 20.16 deg/tick | C | W8 | ours = 360 deg/tick (values above 360 are read as deg/s) |
| `spore_spire_dome` | SporeCrawler `AcidSpew` | `air_weapon/period_ticks` | 15 | 0.8608 | `liberty` WeaponData:382 | 15.37 | A |  |  |
| `spore_spire_dome` | SporeCrawler `AcidSpew` | `air_weapon/damage_point_ticks` | 2 | 0.167 | `core` WeaponData:17 (default `CWeapon`) | 2.98 | C | W4 |  |
| `spore_spire_dome` | SporeCrawler `AcidSpew` | `air_weapon/backswing_ticks` | 3 | 0.5 | `core` WeaponData:18 (default `CWeapon`) | 8.93 | C | W5 |  |
| `spore_spire_dome` | SporeCrawler `AcidSpew` | `air_weapon/projectile_speed` | Some(0.80) | 18.75 | `core` MoverData:17 (mover `MissileDefault`, `MaxSpeed`) | 1.050 | C | W7 |  |
| `carapace_mound_dome` | RoachWarren | `build_time_ticks` | 525 | 55 | `liberty` AbilData:2231 (`ZergBuild` Build14) | 982.1 | C | P3 |  |
| `carapace_mound_dome` | RoachWarren | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:11760 | 0.01531 | C | R2 |  |
| `caustic_nursery_dome` | BanelingNest | `build_time_ticks` | 525 | 60 | `liberty` AbilData:2227 (`ZergBuild` Build11) | 1071.4 | C | P3 |  |
| `caustic_nursery_dome` | BanelingNest | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:9961 | 0.01531 | C | R2 |  |
| `spine_den_dome` | HydraliskDen | `build_time_ticks` | 700 | 40 | `liberty` AbilData:2212 (`ZergBuild` Build6) | 714.3 | C | P3 |  |
| `spine_den_dome` | HydraliskDen | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:8944 | 0.01531 | C | R2 |  |
| `burrow_spire_dome` | LurkerDenMP | `build_time_ticks` | 1425 | 80 | `voidmulti` AbilData:1341 (`ZergBuild` Build12) | 1428.6 | C | P2 |  |
| `burrow_spire_dome` | LurkerDenMP | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:656 | 0.01531 | C | R2 |  |
| `chitin_spire_dome` | Spire | `build_time_ticks` | 1075 | 100 | `liberty` AbilData:2215 (`ZergBuild` Build7) | 1785.7 | C | P3 |  |
| `chitin_spire_dome` | Spire | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:9006 | 0.01531 | C | R2 |  |
| `apex_spire_dome` | GreaterSpire | `build_time_ticks` | 900 | 100 | `liberty` AbilData:2367 (`UpgradeToGreaterSpire`, section Stats delay) | 1785.7 | C | P3 |  |
| `apex_spire_dome` | GreaterSpire | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:9077 | 0.01531 | C | R2 |  |
| `infestation_nursery_dome` | InfestationPit | `build_time_ticks` | 900 | 50 | `liberty` AbilData:2221 (`ZergBuild` Build9) | 892.9 | C | P2 |  |
| `infestation_nursery_dome` | InfestationPit | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:2059 | 0.01531 | C | R2 |  |
| `subterranean_network_dome` | NydusNetwork | `build_time_ticks` | 900 | 50 | `liberty` AbilData:2224 (`ZergBuild` Build10) | 892.9 | C | P2 |  |
| `subterranean_network_dome` | NydusNetwork | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:9894 | 0.01531 | C | R2 |  |
| `behemoth_den_dome` | UltraliskCavern | `build_time_ticks` | 1150 | 65 | `liberty` AbilData:2218 (`ZergBuild` Build8) | 1160.7 | C | P2 |  |
| `behemoth_den_dome` | UltraliskCavern | `hp_regen_per_tick` | 0.02 | 0.2734 | `liberty` UnitData:9210 | 0.01531 | C | R2 |  |

### `upgrades.ron`

| Identity | SC2 | Field | Ours | Editor (Normal) | Source | Reference (ours) | Class | Group | Note |
|---|---|---|---|---|---|---|---|---|---|
| `infantry_weapons_1` | TerranInfantryWeaponsLevel1 | `research_ticks` | 2850 | 160 | `liberty` AbilData:1699 (`EngineeringBayResearch`) | 2857.1 | C | P2 |  |
| `infantry_weapons_2` | TerranInfantryWeaponsLevel2 | `research_ticks` | 3400 | 190 | `liberty` AbilData:1704 (`EngineeringBayResearch`) | 3392.9 | C | P2 |  |
| `infantry_weapons_3` | TerranInfantryWeaponsLevel3 | `research_ticks` | 3925 | 220 | `liberty` AbilData:1709 (`EngineeringBayResearch`) | 3928.6 | C | P2 |  |
| `infantry_armor_1` | TerranInfantryArmorsLevel1 | `research_ticks` | 2850 | 160 | `liberty` AbilData:1721 (`EngineeringBayResearch`) | 2857.1 | C | P2 |  |
| `infantry_armor_2` | TerranInfantryArmorsLevel2 | `research_ticks` | 3400 | 190 | `liberty` AbilData:1726 (`EngineeringBayResearch`) | 3392.9 | C | P2 |  |
| `infantry_armor_3` | TerranInfantryArmorsLevel3 | `research_ticks` | 3925 | 220 | `liberty` AbilData:1731 (`EngineeringBayResearch`) | 3928.6 | C | P2 |  |
| `vehicle_weapons_1` | TerranVehicleWeaponsLevel1 | `research_ticks` | 2850 | 160 | `liberty` AbilData:1874 (`ArmoryResearch`) | 2857.1 | C | P2 |  |
| `vehicle_weapons_2` | TerranVehicleWeaponsLevel2 | `research_ticks` | 3400 | 190 | `liberty` AbilData:1879 (`ArmoryResearch`) | 3392.9 | C | P2 |  |
| `vehicle_weapons_3` | TerranVehicleWeaponsLevel3 | `research_ticks` | 3925 | 220 | `liberty` AbilData:1884 (`ArmoryResearch`) | 3928.6 | C | P2 |  |
| `ship_weapons_1` | TerranShipWeaponsLevel1 | `research_ticks` | 2850 | 160 | `liberty` AbilData:1904 (`ArmoryResearch`) | 2857.1 | C | P2 |  |
| `ship_weapons_2` | TerranShipWeaponsLevel2 | `research_ticks` | 3400 | 190 | `liberty` AbilData:1909 (`ArmoryResearch`) | 3392.9 | C | P2 |  |
| `ship_weapons_3` | TerranShipWeaponsLevel3 | `research_ticks` | 3925 | 220 | `liberty` AbilData:1914 (`ArmoryResearch`) | 3928.6 | C | P2 |  |
| `vehicle_ship_plating_1` | TerranVehicleAndShipArmorsLevel1 | `research_ticks` | 2850 | 160 | `voidmulti` AbilData:25 (`ArmoryResearch`) | 2857.1 | C | P2 |  |
| `vehicle_ship_plating_2` | TerranVehicleAndShipArmorsLevel2 | `research_ticks` | 3400 | 190 | `voidmulti` AbilData:30 (`ArmoryResearch`) | 3392.9 | C | P2 |  |
| `vehicle_ship_plating_3` | TerranVehicleAndShipArmorsLevel3 | `research_ticks` | 3925 | 220 | `voidmulti` AbilData:35 (`ArmoryResearch`) | 3928.6 | C | P2 |  |
| `combat_shield` | ShieldWall | `research_ticks` | 1975 | 110 | `liberty` AbilData:1770 (`BarracksTechLabResearch`) | 1964.3 | C | P2 |  |
| `overclock_stim` | Stimpack | `research_ticks` | 3025 | 140 | `voidmulti` AbilData:48 (`BarracksTechLabResearch`) | 2500.0 | C | P3 |  |
| `concussive_shells` | PunisherGrenades | `research_ticks` | 1075 | 60 | `swarm` AbilData:987 (`BarracksTechLabResearch`) | 1071.4 | C | P2 |  |
| `infernal_preigniter` | HighCapacityBarrels | `research_ticks` | 1975 | 110 | `liberty` AbilData:1794 (`FactoryTechLabResearch`) | 1964.3 | C | P2 |  |
| `drilling_claws` | DrillClaws | `research_ticks` | 1975 | 110 | `void` AbilData:638 (`FactoryTechLabResearch`) | 1964.3 | C | P2 |  |
| `smart_servos` | SmartServos | `research_ticks` | 1975 | 110 | `voidmulti` AbilData:326 (`FactoryTechLabResearch`) | 1964.3 | C | P2 |  |
| `personal_cloaking` | PersonalCloaking | `research_ticks` | 2150 | 120 | `liberty` AbilData:1842 (`GhostAcademyResearch`) | 2142.9 | C | P2 |  |
| `cloaking_field` | BansheeCloak | `research_ticks` | 1975 | 110 | `liberty` AbilData:1804 (`StarportTechLabResearch`) | 1964.3 | C | P2 |  |
| `neosteel_armor` | TerranBuildingArmor | `research_ticks` | 2500 | 140 | `liberty` AbilData:1694 (`EngineeringBayResearch`) | 2500.0 | A |  |  |
| `hi_sec_auto_tracking` | HiSecAutoTracking | `research_ticks` | 1425 | 80 | `liberty` AbilData:1689 (`EngineeringBayResearch`) | 1428.6 | C | P2 |  |
| `advanced_ballistics` | LiberatorAGRangeUpgrade | `research_ticks` | 1975 | 110 | `voidmulti` AbilData:419 (`FusionCoreResearch`) | 1964.3 | C | P2 |  |
| `ground_weapons_1` | ProtossGroundWeaponsLevel1 | `research_ticks` | 3025 | 170 | `voidmulti` AbilData:396 (`ForgeResearch`) | 3035.7 | C | P2 |  |
| `ground_weapons_2` | ProtossGroundWeaponsLevel2 | `research_ticks` | 3625 | 202.5 | `voidmulti` AbilData:397 (`ForgeResearch`) | 3616.1 | C | P2 |  |
| `ground_weapons_3` | ProtossGroundWeaponsLevel3 | `research_ticks` | 4200 | 235 | `voidmulti` AbilData:398 (`ForgeResearch`) | 4196.4 | C | P2 |  |
| `ground_armor_1` | ProtossGroundArmorsLevel1 | `research_ticks` | 3025 | 170 | `voidmulti` AbilData:399 (`ForgeResearch`) | 3035.7 | C | P2 |  |
| `ground_armor_2` | ProtossGroundArmorsLevel2 | `research_ticks` | 3625 | 202.5 | `voidmulti` AbilData:400 (`ForgeResearch`) | 3616.1 | C | P2 |  |
| `ground_armor_3` | ProtossGroundArmorsLevel3 | `research_ticks` | 4200 | 235 | `voidmulti` AbilData:401 (`ForgeResearch`) | 4196.4 | C | P2 |  |
| `plasma_shields_1` | ProtossShieldsLevel1 | `research_ticks` | 3025 | 170 | `voidmulti` AbilData:402 (`ForgeResearch`) | 3035.7 | C | P2 |  |
| `plasma_shields_2` | ProtossShieldsLevel2 | `research_ticks` | 3625 | 202.5 | `voidmulti` AbilData:403 (`ForgeResearch`) | 3616.1 | C | P2 |  |
| `plasma_shields_3` | ProtossShieldsLevel3 | `research_ticks` | 4200 | 235 | `voidmulti` AbilData:407 (`ForgeResearch`) | 4196.4 | C | P2 |  |
| `air_weapons_1` | ProtossAirWeaponsLevel1 | `research_ticks` | 3025 | 180 | `voidmulti` AbilData:297 (`CyberneticsCoreResearch`) | 3214.3 | C | P3 |  |
| `air_weapons_2` | ProtossAirWeaponsLevel2 | `research_ticks` | 3625 | 215 | `voidmulti` AbilData:298 (`CyberneticsCoreResearch`) | 3839.3 | C | P3 |  |
| `air_weapons_3` | ProtossAirWeaponsLevel3 | `research_ticks` | 4200 | 250 | `voidmulti` AbilData:299 (`CyberneticsCoreResearch`) | 4464.3 | C | P3 |  |
| `air_armor_1` | ProtossAirArmorsLevel1 | `research_ticks` | 3025 | 180 | `voidmulti` AbilData:300 (`CyberneticsCoreResearch`) | 3214.3 | C | P3 |  |
| `air_armor_2` | ProtossAirArmorsLevel2 | `research_ticks` | 3625 | 215 | `voidmulti` AbilData:304 (`CyberneticsCoreResearch`) | 3839.3 | C | P3 |  |
| `air_armor_3` | ProtossAirArmorsLevel3 | `research_ticks` | 4200 | 250 | `voidmulti` AbilData:308 (`CyberneticsCoreResearch`) | 4464.3 | C | P3 |  |
| `warp_gate_tech` | WarpGateResearch | `research_ticks` | 2500 | 140 | `voidmulti` AbilData:312 (`CyberneticsCoreResearch`) | 2500.0 | A |  |  |
| `charge` | Charge | `research_ticks` | 2500 | 140 | `liberty` AbilData:3426 (`TwilightCouncilResearch`) | 2500.0 | A |  |  |
| `charge` | Charge | `effects/move_speed_delta` | 0.038 | 1.125 | `voidmulti` UpgradeData:69 (`Unit,Zealot,Speed +1.125`) | 0.0630 | C | U1 |  |
| `phase_blink` | BlinkTech | `research_ticks` | 3025 | 170 | `void` AbilData:832 (`TwilightCouncilResearch`) | 3035.7 | C | P2 |  |
| `resonating_glaives` | AdeptPiercingAttack | `research_ticks` | 2500 | 140 | `void` AbilData:833 (`TwilightCouncilResearch`) | 2500.0 | A |  |  |
| `resonating_glaives` | AdeptPiercingAttack | `effects/attack_cooldown_delta` | -12 | rate +0.45 | `void` WeaponData:318 (period 2.25) and `void` UpgradeData:880 (`RateMultiplier +0.45`) | -12 (40 → 28; exact 40.18 → 27.71) | A |  |  |
| `shadow_stride` | DarkTemplarBlinkUpgrade | `research_ticks` | 2500 | 140 | `voidmulti` AbilData:1318 (`DarkShrineResearch`) | 2500.0 | A |  |  |
| `psionic_storm` | PsiStormTech | `research_ticks` | 1975 | 110 | `liberty` AbilData:2183 (`TemplarArchivesResearch`) | 1964.3 | C | P2 |  |
| `extended_thermal_lance` | ExtendedThermalLance | `research_ticks` | 2500 | 140 | `liberty` AbilData:2166 (`RoboticsBayResearch`) | 2500.0 | A |  |  |
| `gravitic_boosters` | ObserverGraviticBooster | `research_ticks` | 1425 | 80 | `liberty` AbilData:2150 (`RoboticsBayResearch`) | 1428.6 | C | P2 |  |
| `gravitic_boosters` | ObserverGraviticBooster | `effects/move_speed_delta` | 0.0526 | 0.9375 | `liberty` UpgradeData:1802 (`Unit,Observer,Speed +0.9375`) | 0.0525 | A |  |  |
| `gravitic_drive` | GraviticDrive | `research_ticks` | 1425 | 80 | `liberty` AbilData:2157 (`RoboticsBayResearch`) | 1428.6 | C | P2 |  |
| `gravitic_drive` | GraviticDrive | `effects/move_speed_delta` | 0.0496 | 0.875 | `liberty` UpgradeData:1823 (`Unit,WarpPrism,Speed +0.875`) | 0.0490 | C | U1 |  |
| `anion_pulse_crystals` | PhoenixRangeUpgrade | `research_ticks` | 1600 | 90 | `swarm` AbilData:1049 (`FleetBeaconResearch`) | 1607.1 | C | P2 |  |
| `flux_vanes` | VoidRaySpeedUpgrade | `research_ticks` | 1425 | 80 | `voidmulti` AbilData:380 (`FleetBeaconResearch`) | 1428.6 | C | P2 |  |
| `flux_vanes` | VoidRaySpeedUpgrade | `effects/move_speed_delta` | 0.0308 | 0.5703 | `voidmulti` UpgradeData:348 (set 3.3203) and `voidmulti` UnitData:2025 (base 2.75) | 0.0319 | C | U1 |  |
| `melee_attacks_1` | ZergMeleeWeaponsLevel1 | `research_ticks` | 2850 | 160 | `liberty` AbilData:2246 (`evolutionchamberresearch`) | 2857.1 | C | P2 |  |
| `melee_attacks_2` | ZergMeleeWeaponsLevel2 | `research_ticks` | 3400 | 190 | `liberty` AbilData:2251 (`evolutionchamberresearch`) | 3392.9 | C | P2 |  |
| `melee_attacks_3` | ZergMeleeWeaponsLevel3 | `research_ticks` | 3925 | 220 | `liberty` AbilData:2256 (`evolutionchamberresearch`) | 3928.6 | C | P2 |  |
| `missile_attacks_1` | ZergMissileWeaponsLevel1 | `research_ticks` | 2850 | 160 | `liberty` AbilData:2276 (`evolutionchamberresearch`) | 2857.1 | C | P2 |  |
| `missile_attacks_2` | ZergMissileWeaponsLevel2 | `research_ticks` | 3400 | 190 | `liberty` AbilData:2281 (`evolutionchamberresearch`) | 3392.9 | C | P2 |  |
| `missile_attacks_3` | ZergMissileWeaponsLevel3 | `research_ticks` | 3925 | 220 | `liberty` AbilData:2286 (`evolutionchamberresearch`) | 3928.6 | C | P2 |  |
| `ground_carapace_1` | ZergGroundArmorsLevel1 | `research_ticks` | 2850 | 160 | `liberty` AbilData:2261 (`evolutionchamberresearch`) | 2857.1 | C | P2 |  |
| `ground_carapace_2` | ZergGroundArmorsLevel2 | `research_ticks` | 3400 | 190 | `liberty` AbilData:2266 (`evolutionchamberresearch`) | 3392.9 | C | P2 |  |
| `ground_carapace_3` | ZergGroundArmorsLevel3 | `research_ticks` | 3925 | 220 | `liberty` AbilData:2271 (`evolutionchamberresearch`) | 3928.6 | C | P2 |  |
| `flyer_attacks_1` | ZergFlyerWeaponsLevel1 | `research_ticks` | 2850 | 160 | `liberty` AbilData:2429 (`SpireResearch`) | 2857.1 | C | P2 |  |
| `flyer_attacks_2` | ZergFlyerWeaponsLevel2 | `research_ticks` | 3400 | 190 | `liberty` AbilData:2436 (`SpireResearch`) | 3392.9 | C | P2 |  |
| `flyer_attacks_3` | ZergFlyerWeaponsLevel3 | `research_ticks` | 3925 | 220 | `liberty` AbilData:2443 (`SpireResearch`) | 3928.6 | C | P2 |  |
| `flyer_carapace_1` | ZergFlyerArmorsLevel1 | `research_ticks` | 2850 | 160 | `liberty` AbilData:2450 (`SpireResearch`) | 2857.1 | C | P2 |  |
| `flyer_carapace_2` | ZergFlyerArmorsLevel2 | `research_ticks` | 3400 | 190 | `liberty` AbilData:2457 (`SpireResearch`) | 3392.9 | C | P2 |  |
| `flyer_carapace_3` | ZergFlyerArmorsLevel3 | `research_ticks` | 3925 | 220 | `liberty` AbilData:2464 (`SpireResearch`) | 3928.6 | C | P2 |  |
| `metabolic_boost` | zerglingmovementspeed | `research_ticks` | 1500 | 110 | `liberty` AbilData:2409 (`SpawningPoolResearch`) | 1964.3 | C | P3 |  |
| `metabolic_boost` | zerglingmovementspeed | `effects/move_speed_delta` | 0.0826 | 1.746 | `liberty` UpgradeData:1856 (`Unit,Zergling,Speed +1.746`) | 0.0978 | C | U1 |  |
| `adrenal_glands` | zerglingattackspeed | `research_ticks` | 2150 | 130 | `liberty` AbilData:2402 (`SpawningPoolResearch`) | 2321.4 | C | P3 |  |
| `adrenal_glands` | zerglingattackspeed | `effects/attack_cooldown_delta` | -5 | rate +0.4 | `liberty` WeaponData:362 (period 0.696), `liberty` UpgradeData:1846 and `void` UpgradeData:886 (`RateMultiplier +0.2` each) | -3 (12 → 9; exact 12.43 → 8.88) | C | U2 |  |
| `centrifugal_hooks` | CentrificalHooks | `research_ticks` | 1775 | 100 | `voidmulti` AbilData:1771 (`BanelingNestResearch`) | 1785.7 | C | P2 |  |
| `centrifugal_hooks` | CentrificalHooks | `effects/move_speed_delta` | 0.0252 | 0.4531 | `liberty` UpgradeData:1041 (`Unit,Baneling,Speed +0.4531`) | 0.0254 | A |  |  |
| `glial_reconstitution` | GlialReconstitution | `research_ticks` | 1775 | 110 | `liberty` AbilData:900 (`RoachWarrenResearch`) | 1964.3 | C | P3 |  |
| `glial_reconstitution` | GlialReconstitution | `effects/move_speed_delta` | 0.0400 | 0.75 | `liberty` UpgradeData:1925 (`Unit,Roach,Speed +0.75`) | 0.0420 | C | U1 |  |
| `tunneling_claws` | TunnelingClaws | `research_ticks` | 1975 | 110 | `liberty` AbilData:907 (`RoachWarrenResearch`) | 1964.3 | C | P2 |  |
| `muscular_augments` | EvolveMuscularAugments | `research_ticks` | 1775 | 90 | `voidmulti` AbilData:535 (`HydraliskDenResearch`) | 1607.1 | C | P3 |  |
| `muscular_augments` | EvolveMuscularAugments | `effects/move_speed_delta` | 0.0126 | 0.7 | `voidmulti` UpgradeData:743 (`Unit,Hydralisk,Speed +0.7`, off creep) | 0.0392 | C | U1 |  |
| `grooved_spines` | EvolveGroovedSpines | `research_ticks` | 1775 | 70 | `voidmulti` AbilData:528 (`HydraliskDenResearch`) | 1250.0 | C | P3 |  |
| `nanomuscular_swell` | Frenzy | `research_ticks` | 1600 | 90 | `voidmulti` AbilData:542 (`HydraliskDenResearch`) | 1607.1 | C | P2 |  |
| `adaptive_talons` | DiggingClaws | `research_ticks` | 1975 | 80 | `voidmulti` AbilData:1347 (`LurkerDenResearch`) | 1428.6 | C | P3 |  |
| `seismic_spines` | LurkerRange | `research_ticks` | 1975 | 80 | `voidmulti` AbilData:1354 (`LurkerDenResearch`) | 1428.6 | C | P3 |  |
| `pneumatized_carapace` | overlordspeed | `research_ticks` | 1075 | 60 | `void` AbilData:905 (`LairResearch`) | 1071.4 | C | P2 |  |
| `pneumatized_carapace` | overlordspeed | `effects/move_speed_delta` | 0.0576 | 1.2345 | `void` UpgradeData:621 (set 1.879) and `voidmulti` UnitData:1194 (base 0.6445) | 0.0691 | C | U1 |  |
| `chitinous_plating` | ChitinousPlating | `research_ticks` | 1975 | 110 | `liberty` AbilData:1064 (`UltraliskCavernResearch`) | 1964.3 | C | P2 |  |
| `anabolic_synthesis` | AnabolicSynthesis | `research_ticks` | 1075 | 60 | `voidmulti` AbilData:1104 (`UltraliskCavernResearch`) | 1071.4 | C | P2 |  |
| `anabolic_synthesis` | AnabolicSynthesis | `effects/move_speed_delta` | 0.0228 | 0.4219 | `voidmulti` UpgradeData:47 (`Unit,Ultralisk,Speed +0.421871`, off creep) | 0.0236 | C | U1 |  |
| `microbial_shroud` | MicrobialShroud | `research_ticks` | 1975 | 110 | `voidmulti` AbilData:598 (`InfestationPitResearch`) | 1964.3 | C | P2 |  |
| `neural_parasite` | NeuralParasite | `research_ticks` | 1975 | 110 | `swarm` AbilData:1072 (`InfestationPitResearch`) | 1964.3 | C | P2 |  |

## 5. Change groups for WP12

Each group covers one field kind and one cause (WP3 step 4). The first eight groups touch no matrix battle. Each group's
identity list shows whether it touches a matrix identity; whether a matrix battle actually moves is for the
measurement (§7). WP10 is listed last as the confirmation for that work package and is not part of WP12.

| Order | Group | Field kind | Cause | Values |
|---|---|---|---|---|
| 1 | P1 | `build_time_ticks` | Normal time, unconverted (class B): zealot, Siege Tank, Planetary Fortress | 3 |
| 2 | P2 | `build_time_ticks`, `research_ticks` | Liquipedia's whole-second rounding (at most 12.5 ticks off) | 127 |
| 3 | P3 | `build_time_ticks`, `research_ticks` | other differences (old patch values, unsourced numbers) | 31 |
| 4 | U1 | `move_speed_delta` | upgrade speed bonuses that differ from the data | 8 |
| 5 | U2 | `attack_cooldown_delta` | Adrenal Glands: −3 ticks (12 → 9), not −5 | 1 |
| 6 | M1 | `harvest_rate_ticks` | mining time 1.99 Faster s = 50 ticks, not 60 | 3 |
| 7 | M2 | Siege Mode `cast_ticks` and `cooldown_ticks` | transformation 72 ticks (**verify**); the game has no Siege Mode cooldown | 2 |
| 8 | R3 | `energy_regen_per_tick` | 0.5625 Normal per s = 0.0315 per tick, not 0.056 | 9 |
| 9 | S1 | `max_speed` | Normal time, unconverted (class B): workers, Roach, Siege Tank | 5 |
| 10 | S2 | `max_speed` | other differences (High Templar, Raven, Sentry, Brood Lord, Observer) | 5 |
| 11 | R1 | `hp_regen_per_tick` | Normal time, unconverted (class B): the Reaper | 1 |
| 12 | R2 | `hp_regen_per_tick` | Zerg 0.2734 Normal per s = 0.0153 per tick, not 0.02; Roach 0 → 0.0153; Mutalisk 1 per s = 0.056 | 32 |
| 13 | W1 | `period_ticks` | Normal time, unconverted (class B): Siege Tank, Planetary Fortress | 2 |
| 14 | W2 | `period_ticks` | differs by more than rounding | 14 |
| 15 | W3 | `damage_point_ticks` | Normal time, unconverted (class B): the "0.16 s" frontswing of plan 17 | 3 |
| 16 | W4 | `damage_point_ticks` | never sourced | 40 |
| 17 | W5 | `backswing_ticks` | never sourced | 48 |
| 18 | W6 | `projectile_speed` | Normal time, unconverted (class B): the Stalker | 2 |
| 19 | W7 | `projectile_speed` | never sourced, or instant against a missile (and back) | 21 |
| 20 | W8 | `turret_turn_rate_deg` | never sourced | 8 |
| — | WP10 | `shield_regen_per_tick` | Normal time, unconverted (class B); WP10 changes it | 29 |

The groups need the following notes for WP12.

- **W4 and W5 are the largest change in behaviour.** They give almost every weapon a longer wind-up (about 3 ticks
  instead of 1 to 2) and a longer backswing (about 9 instead of 2 to 4). After WP1 the backswing no longer delays the
  next attack. It still anchors a unit and marks it as attacking, and a wind-up still holds a unit in place and can be
  cancelled.
- **W7 makes most missiles roughly twice as fast**, which shortens every ranged unit's time to damage.
- **R2 and R3 lower** Zerg life regeneration and energy regeneration.
- **Documents to correct with the data.** Doc 07's roster restates several of these values (for example Siege Tank
  period "18 ticks", Stalker "26 ticks", Roach "0.1200 / 475 / 36 ticks", Colossus turn rate "280"), and doc 08 the
  production times. The commit that changes a value corrects its doc 07 or doc 08 cell (plan D-1, item 4).


### M1 (3 values, 3 identities)

Matrix battles: not exercised (battles spawn their units and apply research at once; nothing is trained, built, researched or harvested)

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `welder_drone` | `capabilities/harvester/harvest_rate_ticks` | 60 | 49.8 | C |
| `grav_harvester` | `capabilities/harvester/harvest_rate_ticks` | 60 | 49.8 | C |
| `nectar_worker` | `capabilities/harvester/harvest_rate_ticks` | 60 | 49.8 | C |

### M2 (2 values, 1 identity)

Matrix battles: not exercised (no battle orders Siege Mode)

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `railgun_crawler` | `modes/cast_ticks` | 71 | 72.2 | C |
| `railgun_crawler` | `abilities/cooldown_ticks` | 75 | 0 | C |

### P1 (3 values, 3 identities)

Matrix battles: not exercised (battles spawn their units and apply research at once; nothing is trained, built, researched or harvested)

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `psi_grey` | `build_time_ticks` | 950 | 678.6 | B |
| `railgun_crawler` | `build_time_ticks` | 1125 | 803.6 | B |
| `planetary_bulwark_dome` | `build_time_ticks` | 1250 | 892.9 | B |

### P2 (127 values, 127 identities)

Matrix battles: not exercised (battles spawn their units and apply research at once; nothing is trained, built, researched or harvested)

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `welder_drone` | `build_time_ticks` | 300 | 303.6 | C |
| `pulse_bot` | `build_time_ticks` | 450 | 446.4 | C |
| `shock_golem` | `build_time_ticks` | 525 | 535.7 | C |
| `jet_jumper` | `build_time_ticks` | 800 | 803.6 | C |
| `specter_android` | `build_time_ticks` | 725 | 714.3 | C |
| `napalm_wheeler` | `build_time_ticks` | 525 | 535.7 | C |
| `colossus_mech` | `build_time_ticks` | 1075 | 1071.4 | C |
| `cloak_gunship` | `build_time_ticks` | 1075 | 1071.4 | C |
| `omni_probe_drone` | `build_time_ticks` | 850 | 857.1 | C |
| `grav_harvester` | `build_time_ticks` | 300 | 303.6 | C |
| `warp_strider` | `build_time_ticks` | 675 | 678.6 | C |
| `cerebral_elder` | `build_time_ticks` | 975 | 982.1 | C |
| `shadow_visitor` | `build_time_ticks` | 975 | 982.1 | C |
| `void_goliath` | `build_time_ticks` | 975 | 982.1 | C |
| `tripod_annihilator` | `build_time_ticks` | 1350 | 1339.3 | C |
| `phase_prism_saucer` | `build_time_ticks` | 900 | 892.9 | C |
| `mothership_carrier` | `build_time_ticks` | 1600 | 1607.1 | C |
| `nectar_worker` | `build_time_ticks` | 300 | 303.6 | C |
| `razor_mantis` | `build_time_ticks` | 425 | 428.6 | C |
| `volatile_tick` | `build_time_ticks` | 350 | 357.1 | C |
| `mortar_stag` | `build_time_ticks` | 225 | 214.3 | C |
| `thorn_viper` | `build_time_ticks` | 600 | 589.3 | C |
| `burrow_scorpion` | `build_time_ticks` | 450 | 446.4 | C |
| `sonic_bat` | `build_time_ticks` | 600 | 589.3 | C |
| `sky_leviathan` | `build_time_ticks` | 725 | 714.3 | C |
| `spore_matriarch` | `build_time_ticks` | 600 | 604.2 | C |
| `stag_behemoth` | `build_time_ticks` | 975 | 982.1 | C |
| `hive_matron` | `build_time_ticks` | 900 | 892.9 | C |
| `mycelium_shaman` | `build_time_ticks` | 900 | 892.9 | C |
| `tongue_chameleon` | `build_time_ticks` | 725 | 714.3 | C |
| `amber_pollen_dome` | `build_time_ticks` | 450 | 446.4 | C |
| `foundry_core_dome` | `build_time_ticks` | 1775 | 1785.7 | C |
| `capacitor_dome` | `build_time_ticks` | 525 | 535.7 | C |
| `plasma_siphon_dome` | `build_time_ticks` | 525 | 535.7 | C |
| `assembly_bay_dome` | `build_time_ticks` | 1150 | 1160.7 | C |
| `fortress_bunker_dome` | `build_time_ticks` | 725 | 714.3 | C |
| `radar_beacon_dome` | `build_time_ticks` | 450 | 446.4 | C |
| `flak_array_dome` | `build_time_ticks` | 450 | 446.4 | C |
| `machining_plant_dome` | `build_time_ticks` | 1075 | 1071.4 | C |
| `covert_ops_nexus_dome` | `build_time_ticks` | 725 | 714.3 | C |
| `aerospace_hangar_dome` | `build_time_ticks` | 900 | 892.9 | C |
| `weapons_arsenal_dome` | `build_time_ticks` | 1150 | 1160.7 | C |
| `fusion_reactor_dome` | `build_time_ticks` | 1150 | 1160.7 | C |
| `tech_module_pod` | `build_time_ticks` | 450 | 446.4 | C |
| `citadel_nexus_dome` | `build_time_ticks` | 1775 | 1785.7 | C |
| `psi_pylon_dome` | `build_time_ticks` | 450 | 446.4 | C |
| `warp_gate_dome` | `build_time_ticks` | 1150 | 1160.7 | C |
| `crystalline_forge_dome` | `build_time_ticks` | 800 | 803.6 | C |
| `tesla_sentinel_dome` | `build_time_ticks` | 725 | 714.3 | C |
| `harmonic_recharge_dome` | `build_time_ticks` | 725 | 714.3 | C |
| `stellar_portal_dome` | `build_time_ticks` | 1075 | 1071.4 | C |
| `robotic_foundry_dome` | `build_time_ticks` | 1150 | 1160.7 | C |
| `psionic_archives_dome` | `build_time_ticks` | 900 | 892.9 | C |
| `void_sanctuary_dome` | `build_time_ticks` | 1775 | 1785.7 | C |
| `robotic_assembly_bay_dome` | `build_time_ticks` | 1150 | 1160.7 | C |
| `celestial_beacon_dome` | `build_time_ticks` | 1075 | 1071.4 | C |
| `ancient_tree_dome` | `build_time_ticks` | 1775 | 1785.7 | C |
| `canopy_grove_dome` | `build_time_ticks` | 1425 | 1428.6 | C |
| `primal_world_tree_dome` | `build_time_ticks` | 1775 | 1785.7 | C |
| `spore_vent_dome` | `build_time_ticks` | 525 | 535.7 | C |
| `thorn_lasher_dome` | `build_time_ticks` | 900 | 892.9 | C |
| `burrow_spire_dome` | `build_time_ticks` | 1425 | 1428.6 | C |
| `infestation_nursery_dome` | `build_time_ticks` | 900 | 892.9 | C |
| `subterranean_network_dome` | `build_time_ticks` | 900 | 892.9 | C |
| `behemoth_den_dome` | `build_time_ticks` | 1150 | 1160.7 | C |
| `infantry_weapons_1` | `research_ticks` | 2850 | 2857.1 | C |
| `infantry_weapons_2` | `research_ticks` | 3400 | 3392.9 | C |
| `infantry_weapons_3` | `research_ticks` | 3925 | 3928.6 | C |
| `infantry_armor_1` | `research_ticks` | 2850 | 2857.1 | C |
| `infantry_armor_2` | `research_ticks` | 3400 | 3392.9 | C |
| `infantry_armor_3` | `research_ticks` | 3925 | 3928.6 | C |
| `vehicle_weapons_1` | `research_ticks` | 2850 | 2857.1 | C |
| `vehicle_weapons_2` | `research_ticks` | 3400 | 3392.9 | C |
| `vehicle_weapons_3` | `research_ticks` | 3925 | 3928.6 | C |
| `ship_weapons_1` | `research_ticks` | 2850 | 2857.1 | C |
| `ship_weapons_2` | `research_ticks` | 3400 | 3392.9 | C |
| `ship_weapons_3` | `research_ticks` | 3925 | 3928.6 | C |
| `vehicle_ship_plating_1` | `research_ticks` | 2850 | 2857.1 | C |
| `vehicle_ship_plating_2` | `research_ticks` | 3400 | 3392.9 | C |
| `vehicle_ship_plating_3` | `research_ticks` | 3925 | 3928.6 | C |
| `combat_shield` | `research_ticks` | 1975 | 1964.3 | C |
| `concussive_shells` | `research_ticks` | 1075 | 1071.4 | C |
| `infernal_preigniter` | `research_ticks` | 1975 | 1964.3 | C |
| `drilling_claws` | `research_ticks` | 1975 | 1964.3 | C |
| `smart_servos` | `research_ticks` | 1975 | 1964.3 | C |
| `personal_cloaking` | `research_ticks` | 2150 | 2142.9 | C |
| `cloaking_field` | `research_ticks` | 1975 | 1964.3 | C |
| `hi_sec_auto_tracking` | `research_ticks` | 1425 | 1428.6 | C |
| `advanced_ballistics` | `research_ticks` | 1975 | 1964.3 | C |
| `ground_weapons_1` | `research_ticks` | 3025 | 3035.7 | C |
| `ground_weapons_2` | `research_ticks` | 3625 | 3616.1 | C |
| `ground_weapons_3` | `research_ticks` | 4200 | 4196.4 | C |
| `ground_armor_1` | `research_ticks` | 3025 | 3035.7 | C |
| `ground_armor_2` | `research_ticks` | 3625 | 3616.1 | C |
| `ground_armor_3` | `research_ticks` | 4200 | 4196.4 | C |
| `plasma_shields_1` | `research_ticks` | 3025 | 3035.7 | C |
| `plasma_shields_2` | `research_ticks` | 3625 | 3616.1 | C |
| `plasma_shields_3` | `research_ticks` | 4200 | 4196.4 | C |
| `phase_blink` | `research_ticks` | 3025 | 3035.7 | C |
| `psionic_storm` | `research_ticks` | 1975 | 1964.3 | C |
| `gravitic_boosters` | `research_ticks` | 1425 | 1428.6 | C |
| `gravitic_drive` | `research_ticks` | 1425 | 1428.6 | C |
| `anion_pulse_crystals` | `research_ticks` | 1600 | 1607.1 | C |
| `flux_vanes` | `research_ticks` | 1425 | 1428.6 | C |
| `melee_attacks_1` | `research_ticks` | 2850 | 2857.1 | C |
| `melee_attacks_2` | `research_ticks` | 3400 | 3392.9 | C |
| `melee_attacks_3` | `research_ticks` | 3925 | 3928.6 | C |
| `missile_attacks_1` | `research_ticks` | 2850 | 2857.1 | C |
| `missile_attacks_2` | `research_ticks` | 3400 | 3392.9 | C |
| `missile_attacks_3` | `research_ticks` | 3925 | 3928.6 | C |
| `ground_carapace_1` | `research_ticks` | 2850 | 2857.1 | C |
| `ground_carapace_2` | `research_ticks` | 3400 | 3392.9 | C |
| `ground_carapace_3` | `research_ticks` | 3925 | 3928.6 | C |
| `flyer_attacks_1` | `research_ticks` | 2850 | 2857.1 | C |
| `flyer_attacks_2` | `research_ticks` | 3400 | 3392.9 | C |
| `flyer_attacks_3` | `research_ticks` | 3925 | 3928.6 | C |
| `flyer_carapace_1` | `research_ticks` | 2850 | 2857.1 | C |
| `flyer_carapace_2` | `research_ticks` | 3400 | 3392.9 | C |
| `flyer_carapace_3` | `research_ticks` | 3925 | 3928.6 | C |
| `centrifugal_hooks` | `research_ticks` | 1775 | 1785.7 | C |
| `tunneling_claws` | `research_ticks` | 1975 | 1964.3 | C |
| `nanomuscular_swell` | `research_ticks` | 1600 | 1607.1 | C |
| `pneumatized_carapace` | `research_ticks` | 1075 | 1071.4 | C |
| `chitinous_plating` | `research_ticks` | 1975 | 1964.3 | C |
| `anabolic_synthesis` | `research_ticks` | 1075 | 1071.4 | C |
| `microbial_shroud` | `research_ticks` | 1975 | 1964.3 | C |
| `neural_parasite` | `research_ticks` | 1975 | 1964.3 | C |

### P3 (31 values, 31 identities)

Matrix battles: not exercised (battles spawn their units and apply research at once; nothing is trained, built, researched or harvested)

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `rhino_beetle` | `build_time_ticks` | 750 | 482.1 | C |
| `sky_bastion` | `build_time_ticks` | 850 | 1071.4 | C |
| `prism_weaver` | `build_time_ticks` | 650 | 571.4 | C |
| `astral_vagrant` | `build_time_ticks` | 675 | 750.0 | C |
| `stealth_eye_orb` | `build_time_ticks` | 525 | 446.4 | C |
| `duplicator_module_pod` | `build_time_ticks` | 450 | 892.9 | C |
| `rift_siphon_dome` | `build_time_ticks` | 450 | 535.7 | C |
| `cybernetics_conduit_dome` | `build_time_ticks` | 800 | 892.9 | C |
| `templar_conclave_dome` | `build_time_ticks` | 1150 | 892.9 | C |
| `spawning_hollow_dome` | `build_time_ticks` | 525 | 1160.7 | C |
| `metamorphic_chamber_dome` | `build_time_ticks` | 450 | 625.0 | C |
| `spore_spire_dome` | `build_time_ticks` | 450 | 535.7 | C |
| `carapace_mound_dome` | `build_time_ticks` | 525 | 982.1 | C |
| `caustic_nursery_dome` | `build_time_ticks` | 525 | 1071.4 | C |
| `spine_den_dome` | `build_time_ticks` | 700 | 714.3 | C |
| `chitin_spire_dome` | `build_time_ticks` | 1075 | 1785.7 | C |
| `apex_spire_dome` | `build_time_ticks` | 900 | 1785.7 | C |
| `overclock_stim` | `research_ticks` | 3025 | 2500.0 | C |
| `air_weapons_1` | `research_ticks` | 3025 | 3214.3 | C |
| `air_weapons_2` | `research_ticks` | 3625 | 3839.3 | C |
| `air_weapons_3` | `research_ticks` | 4200 | 4464.3 | C |
| `air_armor_1` | `research_ticks` | 3025 | 3214.3 | C |
| `air_armor_2` | `research_ticks` | 3625 | 3839.3 | C |
| `air_armor_3` | `research_ticks` | 4200 | 4464.3 | C |
| `metabolic_boost` | `research_ticks` | 1500 | 1964.3 | C |
| `adrenal_glands` | `research_ticks` | 2150 | 2321.4 | C |
| `glial_reconstitution` | `research_ticks` | 1775 | 1964.3 | C |
| `muscular_augments` | `research_ticks` | 1775 | 1607.1 | C |
| `grooved_spines` | `research_ticks` | 1775 | 1250.0 | C |
| `adaptive_talons` | `research_ticks` | 1975 | 1428.6 | C |
| `seismic_spines` | `research_ticks` | 1975 | 1428.6 | C |

### R1 (1 value, 1 identity)

Matrix identities: `jet_jumper`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `jet_jumper` | `hp_regen_per_tick` | 0.08 | 0.112 | B |

### R2 (32 values, 32 identities)

Matrix identities: `burrow_scorpion`, `razor_mantis`, `rhino_beetle`, `sky_leviathan`, `sonic_bat`, `spore_matriarch`, `stag_behemoth`, `thorn_viper`, `volatile_tick`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `rhino_beetle` | `hp_regen_per_tick` | 0.00 | 0.01531 | C |
| `nectar_worker` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `razor_mantis` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `volatile_tick` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `mortar_stag` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `thorn_viper` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `burrow_scorpion` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `sonic_bat` | `hp_regen_per_tick` | 0.02 | 0.056 | C |
| `sky_leviathan` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `spore_matriarch` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `stag_behemoth` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `hive_matron` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `mycelium_shaman` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `tongue_chameleon` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `amber_pollen_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `ancient_tree_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `canopy_grove_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `primal_world_tree_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `spore_vent_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `spawning_hollow_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `metamorphic_chamber_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `thorn_lasher_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `spore_spire_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `carapace_mound_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `caustic_nursery_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `spine_den_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `burrow_spire_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `chitin_spire_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `apex_spire_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `infestation_nursery_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `subterranean_network_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |
| `behemoth_den_dome` | `hp_regen_per_tick` | 0.02 | 0.01531 | C |

### R3 (9 values, 9 identities)

Matrix identities: none

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `specter_android` | `energy_regen_per_tick` | 0.056 | 0.0315 | C |
| `repair_shuttle` | `energy_regen_per_tick` | 0.056 | 0.0315 | C |
| `cloak_gunship` | `energy_regen_per_tick` | 0.056 | 0.0315 | C |
| `omni_probe_drone` | `energy_regen_per_tick` | 0.056 | 0.0315 | C |
| `prism_weaver` | `energy_regen_per_tick` | 0.056 | 0.0315 | C |
| `cerebral_elder` | `energy_regen_per_tick` | 0.056 | 0.0315 | C |
| `hive_matron` | `energy_regen_per_tick` | 0.056 | 0.0315 | C |
| `mycelium_shaman` | `energy_regen_per_tick` | 0.056 | 0.0315 | C |
| `tongue_chameleon` | `energy_regen_per_tick` | 0.056 | 0.0315 | C |

### S1 (5 values, 5 identities)

Matrix identities: `railgun_crawler`, `rhino_beetle`, `welder_drone`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `welder_drone` | `max_speed` | 0.1124 | 0.1575 | B |
| `rhino_beetle` | `max_speed` | 0.09 | 0.126 | B |
| `railgun_crawler` | `max_speed` | 0.09 | 0.126 | B |
| `grav_harvester` | `max_speed` | 0.1124 | 0.1575 | B |
| `nectar_worker` | `max_speed` | 0.1124 | 0.1575 | B |

### S2 (5 values, 5 identities)

Matrix identities: `spore_matriarch`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `omni_probe_drone` | `max_speed` | 0.154 | 0.1652 | C |
| `prism_weaver` | `max_speed` | 0.126 | 0.14 | C |
| `cerebral_elder` | `max_speed` | 0.1048 | 0.1129 | C |
| `stealth_eye_orb` | `max_speed` | 0.1052 | 0.1129 | C |
| `spore_matriarch` | `max_speed` | 0.0788 | 0.105 | C |

### U1 (8 values, 8 identities)

Matrix battles: not exercised (battles spawn their units and apply research at once; nothing is trained, built, researched or harvested)

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `charge` | `effects/move_speed_delta` | 0.038 | 0.0630 | C |
| `gravitic_drive` | `effects/move_speed_delta` | 0.0496 | 0.0490 | C |
| `flux_vanes` | `effects/move_speed_delta` | 0.0308 | 0.0319 | C |
| `metabolic_boost` | `effects/move_speed_delta` | 0.0826 | 0.0978 | C |
| `glial_reconstitution` | `effects/move_speed_delta` | 0.0400 | 0.0420 | C |
| `muscular_augments` | `effects/move_speed_delta` | 0.0126 | 0.0392 | C |
| `pneumatized_carapace` | `effects/move_speed_delta` | 0.0576 | 0.0691 | C |
| `anabolic_synthesis` | `effects/move_speed_delta` | 0.0228 | 0.0236 | C |

### U2 (1 value, 1 identity)

Matrix battles: not exercised (battles spawn their units and apply research at once; nothing is trained, built, researched or harvested)

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `adrenal_glands` | `effects/attack_cooldown_delta` | -5 | -3 (12 → 9; exact 12.43 → 8.88) | C |

### W1 (2 values, 2 identities)

Matrix identities: `railgun_crawler`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `railgun_crawler` | `weapon/period_ticks` | 26 | 18.57 | B |
| `planetary_bulwark_dome` | `ground_weapon/period_ticks` | 50 | 35.71 | B |

### W2 (14 values, 11 identities)

Matrix identities: `colossus_mech`, `rhino_beetle`, `shock_golem`, `void_goliath`, `volatile_tick`, `warp_strider`, `welder_drone`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `welder_drone` | `weapon/period_ticks` | 19 | 26.79 | C |
| `rhino_beetle` | `weapon/period_ticks` | 28 | 35.71 | C |
| `shock_golem` | `weapon/period_ticks` | 26 | 26.79 | C |
| `colossus_mech` | `air_weapon/period_ticks` | 36 | 22.86 | C |
| `sky_bastion` | `ground_weapon/period_ticks` | 28 | 28.57 | C |
| `sky_bastion` | `air_weapon/period_ticks` | 25 | 32.14 | C |
| `sky_bastion` | `weapon/period_ticks` | 25 | 32.14 | C |
| `grav_harvester` | `weapon/period_ticks` | 15 | 26.79 | C |
| `warp_strider` | `weapon/period_ticks` | 34 | 33.39 | C |
| `warp_strider` | `air_weapon/period_ticks` | 34 | 33.39 | C |
| `void_goliath` | `weapon/period_ticks` | 28 | 28.57 | C |
| `nectar_worker` | `weapon/period_ticks` | 15 | 26.79 | C |
| `volatile_tick` | `weapon/period_ticks` | 10 | 14.88 | C |
| `mortar_stag` | `weapon/period_ticks` | 28 | 28.57 | C |

### W3 (3 values, 2 identities)

Matrix identities: `railgun_crawler`, `welder_drone`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `welder_drone` | `weapon/damage_point_ticks` | 4 | 2.98 | B |
| `railgun_crawler` | `weapon/damage_point_ticks` | 4 | 2.98 | B |
| `railgun_crawler` | `modes/damage_point_ticks` | 4 | 2.98 | B |

### W4 (40 values, 29 identities)

Matrix identities: `burrow_scorpion`, `colossus_mech`, `jet_jumper`, `napalm_wheeler`, `prismatic_saucer`, `razor_mantis`, `rhino_beetle`, `shock_golem`, `sonic_bat`, `spore_matriarch`, `stag_behemoth`, `thorn_viper`, `tripod_annihilator`, `void_goliath`, `volatile_tick`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `rhino_beetle` | `weapon/damage_point_ticks` | 2 | 2.98 | C |
| `shock_golem` | `weapon/damage_point_ticks` | 1 | 0.00 | C |
| `jet_jumper` | `weapon/damage_point_ticks` | 1 | 0.00 | C |
| `napalm_wheeler` | `weapon/damage_point_ticks` | 0 | 4.46 | C |
| `colossus_mech` | `ground_weapon/damage_point_ticks` | 2 | 14.84 | C |
| `colossus_mech` | `air_weapon/damage_point_ticks` | 1 | 2.98 | C |
| `colossus_mech` | `weapon/damage_point_ticks` | 2 | 14.84 | C |
| `sky_bastion` | `ground_weapon/damage_point_ticks` | 0 | 2.23 | C |
| `sky_bastion` | `air_weapon/damage_point_ticks` | 1 | 2.98 | C |
| `sky_bastion` | `weapon/damage_point_ticks` | 1 | 2.98 | C |
| `cloak_gunship` | `weapon/damage_point_ticks` | 1 | 2.98 | C |
| `grav_harvester` | `weapon/damage_point_ticks` | 1 | 2.98 | C |
| `prism_weaver` | `weapon/damage_point_ticks` | 0 | 2.98 | C |
| `astral_vagrant` | `weapon/damage_point_ticks` | 1 | 2.98 | C |
| `cerebral_elder` | `weapon/damage_point_ticks` | 1 | 2.98 | C |
| `shadow_visitor` | `weapon/damage_point_ticks` | 1 | 6.45 | C |
| `void_goliath` | `weapon/damage_point_ticks` | 1 | 2.98 | C |
| `tripod_annihilator` | `weapon/damage_point_ticks` | 0 | 1.49 | C |
| `prismatic_saucer` | `weapon/damage_point_ticks` | 0 | 2.98 | C |
| `prismatic_saucer` | `air_weapon/damage_point_ticks` | 0 | 2.98 | C |
| `nectar_worker` | `weapon/damage_point_ticks` | 1 | 2.98 | C |
| `razor_mantis` | `weapon/damage_point_ticks` | 2 | 2.98 | C |
| `volatile_tick` | `weapon/damage_point_ticks` | 1 | 0.00 | C |
| `mortar_stag` | `weapon/damage_point_ticks` | 1 | 3.57 | C |
| `thorn_viper` | `weapon/damage_point_ticks` | 1 | 2.50 | C |
| `thorn_viper` | `air_weapon/damage_point_ticks` | 1 | 2.50 | C |
| `burrow_scorpion` | `weapon/damage_point_ticks` | 1 | 0.00 | C |
| `sonic_bat` | `weapon/damage_point_ticks` | 1 | 0.00 | C |
| `sonic_bat` | `air_weapon/damage_point_ticks` | 1 | 0.00 | C |
| `spore_matriarch` | `ground_weapon/damage_point_ticks` | 2 | 2.98 | C |
| `spore_matriarch` | `weapon/damage_point_ticks` | 2 | 2.98 | C |
| `stag_behemoth` | `weapon/damage_point_ticks` | 1 | 5.95 | C |
| `hive_matron` | `ground_weapon/damage_point_ticks` | 1 | 2.98 | C |
| `hive_matron` | `air_weapon/damage_point_ticks` | 1 | 2.98 | C |
| `hive_matron` | `weapon/damage_point_ticks` | 1 | 2.98 | C |
| `flak_array_dome` | `air_weapon/damage_point_ticks` | 2 | 2.98 | C |
| `tesla_sentinel_dome` | `ground_weapon/damage_point_ticks` | 2 | 2.98 | C |
| `tesla_sentinel_dome` | `air_weapon/damage_point_ticks` | 2 | 2.98 | C |
| `thorn_lasher_dome` | `ground_weapon/damage_point_ticks` | 3 | 5.95 | C |
| `spore_spire_dome` | `air_weapon/damage_point_ticks` | 2 | 2.98 | C |

### W5 (48 values, 35 identities)

Matrix identities: `burrow_scorpion`, `colossus_mech`, `jet_jumper`, `napalm_wheeler`, `prismatic_saucer`, `railgun_crawler`, `razor_mantis`, `rhino_beetle`, `shock_golem`, `sky_leviathan`, `sonic_bat`, `spore_matriarch`, `stag_behemoth`, `thorn_viper`, `tripod_annihilator`, `vector_interceptor`, `void_goliath`, `volatile_tick`, `warp_strider`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `rhino_beetle` | `weapon/backswing_ticks` | 4 | 8.93 | C |
| `railgun_crawler` | `weapon/backswing_ticks` | 4 | 8.93 | C |
| `railgun_crawler` | `modes/backswing_ticks` | 4 | 8.93 | C |
| `shock_golem` | `weapon/backswing_ticks` | 4 | 0.00 | C |
| `jet_jumper` | `weapon/backswing_ticks` | 3 | 13.39 | C |
| `specter_android` | `weapon/backswing_ticks` | 3 | 20.84 | C |
| `napalm_wheeler` | `weapon/backswing_ticks` | 2 | 13.39 | C |
| `colossus_mech` | `air_weapon/backswing_ticks` | 3 | 8.93 | C |
| `vector_interceptor` | `air_weapon/backswing_ticks` | 3 | 8.93 | C |
| `vector_interceptor` | `weapon/backswing_ticks` | 3 | 8.93 | C |
| `sky_bastion` | `ground_weapon/backswing_ticks` | 3 | 8.93 | C |
| `sky_bastion` | `air_weapon/backswing_ticks` | 2 | 8.93 | C |
| `sky_bastion` | `weapon/backswing_ticks` | 2 | 8.93 | C |
| `cloak_gunship` | `weapon/backswing_ticks` | 3 | 8.93 | C |
| `grav_harvester` | `weapon/backswing_ticks` | 2 | 8.93 | C |
| `warp_strider` | `weapon/backswing_ticks` | 4 | 8.93 | C |
| `warp_strider` | `air_weapon/backswing_ticks` | 4 | 8.93 | C |
| `prism_weaver` | `weapon/backswing_ticks` | 2 | 8.93 | C |
| `astral_vagrant` | `weapon/backswing_ticks` | 3 | 8.93 | C |
| `cerebral_elder` | `weapon/backswing_ticks` | 3 | 8.93 | C |
| `shadow_visitor` | `weapon/backswing_ticks` | 4 | 23.80 | C |
| `void_goliath` | `weapon/backswing_ticks` | 3 | 8.93 | C |
| `tripod_annihilator` | `weapon/backswing_ticks` | 3 | 8.93 | C |
| `prismatic_saucer` | `weapon/backswing_ticks` | 1 | 13.39 | C |
| `prismatic_saucer` | `air_weapon/backswing_ticks` | 1 | 13.39 | C |
| `nectar_worker` | `weapon/backswing_ticks` | 2 | 8.93 | C |
| `razor_mantis` | `weapon/backswing_ticks` | 2 | 8.93 | C |
| `volatile_tick` | `weapon/backswing_ticks` | 1 | 8.93 | C |
| `mortar_stag` | `weapon/backswing_ticks` | 3 | 8.93 | C |
| `thorn_viper` | `weapon/backswing_ticks` | 2 | 10.01 | C |
| `thorn_viper` | `air_weapon/backswing_ticks` | 2 | 10.01 | C |
| `burrow_scorpion` | `weapon/backswing_ticks` | 4 | 8.93 | C |
| `sonic_bat` | `weapon/backswing_ticks` | 3 | 8.93 | C |
| `sonic_bat` | `air_weapon/backswing_ticks` | 3 | 8.93 | C |
| `sky_leviathan` | `air_weapon/backswing_ticks` | 3 | 8.93 | C |
| `sky_leviathan` | `weapon/backswing_ticks` | 3 | 8.93 | C |
| `spore_matriarch` | `ground_weapon/backswing_ticks` | 4 | 8.93 | C |
| `spore_matriarch` | `weapon/backswing_ticks` | 4 | 8.93 | C |
| `stag_behemoth` | `weapon/backswing_ticks` | 2 | 8.93 | C |
| `hive_matron` | `ground_weapon/backswing_ticks` | 2 | 8.93 | C |
| `hive_matron` | `air_weapon/backswing_ticks` | 2 | 8.93 | C |
| `hive_matron` | `weapon/backswing_ticks` | 2 | 8.93 | C |
| `planetary_bulwark_dome` | `ground_weapon/backswing_ticks` | 5 | 8.93 | C |
| `flak_array_dome` | `air_weapon/backswing_ticks` | 3 | 8.93 | C |
| `tesla_sentinel_dome` | `ground_weapon/backswing_ticks` | 3 | 8.93 | C |
| `tesla_sentinel_dome` | `air_weapon/backswing_ticks` | 3 | 8.93 | C |
| `thorn_lasher_dome` | `ground_weapon/backswing_ticks` | 5 | 8.93 | C |
| `spore_spire_dome` | `air_weapon/backswing_ticks` | 3 | 8.93 | C |

### W6 (2 values, 1 identity)

Matrix identities: `warp_strider`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `warp_strider` | `weapon/projectile_speed` | Some(0.75) | 1.050 | B |
| `warp_strider` | `air_weapon/projectile_speed` | Some(0.75) | 1.050 | B |

### W7 (21 values, 15 identities)

Matrix identities: `colossus_mech`, `jet_jumper`, `rhino_beetle`, `shock_golem`, `sky_leviathan`, `sonic_bat`, `thorn_viper`, `void_goliath`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `rhino_beetle` | `weapon/projectile_speed` | Some(0.64) | 1.050 | C |
| `shock_golem` | `weapon/projectile_speed` | Some(0.64) | 1.120 | C |
| `jet_jumper` | `weapon/projectile_speed` | Some(1.0) | None | C |
| `colossus_mech` | `air_weapon/projectile_speed` | Some(1.2) | None | C |
| `sky_bastion` | `ground_weapon/projectile_speed` | None | 2.240 | C |
| `astral_vagrant` | `weapon/projectile_speed` | Some(1.0) | 1.050 | C |
| `void_goliath` | `weapon/projectile_speed` | Some(1.2) | None | C |
| `mortar_stag` | `weapon/projectile_speed` | Some(1.0) | 1.050 | C |
| `thorn_viper` | `weapon/projectile_speed` | Some(0.90) | 2.240 | C |
| `thorn_viper` | `air_weapon/projectile_speed` | Some(0.90) | 2.240 | C |
| `sonic_bat` | `weapon/projectile_speed` | Some(0.50) | 1.050 | C |
| `sonic_bat` | `air_weapon/projectile_speed` | Some(0.50) | 1.050 | C |
| `sky_leviathan` | `air_weapon/projectile_speed` | Some(1.2) | 1.050 | C |
| `sky_leviathan` | `weapon/projectile_speed` | Some(1.2) | 1.050 | C |
| `hive_matron` | `ground_weapon/projectile_speed` | None | 2.240 | C |
| `hive_matron` | `air_weapon/projectile_speed` | Some(1.2) | 2.240 | C |
| `hive_matron` | `weapon/projectile_speed` | None | 2.240 | C |
| `flak_array_dome` | `air_weapon/projectile_speed` | Some(0.80) | 0.560 | C |
| `tesla_sentinel_dome` | `ground_weapon/projectile_speed` | None | 1.008 | C |
| `tesla_sentinel_dome` | `air_weapon/projectile_speed` | None | 1.008 | C |
| `spore_spire_dome` | `air_weapon/projectile_speed` | Some(0.80) | 1.050 | C |

### W8 (8 values, 6 identities)

Matrix identities: `railgun_crawler`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `railgun_crawler` | `weapon/turret_turn_rate_deg` | 21.6 | 40.32 deg/tick | C |
| `railgun_crawler` | `modes/turret_turn_rate_deg` | 21.6 | 20.16 deg/tick | C |
| `planetary_bulwark_dome` | `ground_weapon/turret_turn_rate_deg` | 360.0 | 5.04 deg/tick | C |
| `flak_array_dome` | `air_weapon/turret_turn_rate_deg` | 360.0 | 40.32 deg/tick | C |
| `tesla_sentinel_dome` | `ground_weapon/turret_turn_rate_deg` | 360.0 | 40.32 deg/tick | C |
| `tesla_sentinel_dome` | `air_weapon/turret_turn_rate_deg` | 360.0 | 40.32 deg/tick | C |
| `thorn_lasher_dome` | `ground_weapon/turret_turn_rate_deg` | 360.0 | 20.16 deg/tick | C |
| `spore_spire_dome` | `air_weapon/turret_turn_rate_deg` | 360.0 | 20.16 deg/tick | C |

### WP10 (29 values, 29 identities)

Matrix identities: `prismatic_saucer`, `psi_grey`, `tripod_annihilator`, `void_goliath`, `warp_strider`

| Identity | Field | Ours | Reference | Class |
|---|---|---|---|---|
| `psi_grey` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `flying_saucer` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `grav_harvester` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `warp_strider` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `prism_weaver` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `astral_vagrant` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `cerebral_elder` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `shadow_visitor` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `void_goliath` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `tripod_annihilator` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `stealth_eye_orb` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `phase_prism_saucer` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `prismatic_saucer` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `mothership_carrier` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `citadel_nexus_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `psi_pylon_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `rift_siphon_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `warp_gate_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `crystalline_forge_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `tesla_sentinel_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `harmonic_recharge_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `cybernetics_conduit_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `templar_conclave_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `stellar_portal_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `robotic_foundry_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `psionic_archives_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `void_sanctuary_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `robotic_assembly_bay_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |
| `celestial_beacon_dome` | `shield_regen_per_tick` | 0.08 | 0.112 | B |

## 6. Deliberate values (class D) and other findings for the owner

All of these are in [Q0045](../questions/open/0045-catalog-time-audit-deliberate-values.md). Its default is no change.

1. **Hull turn rates (41 values, every unit except `flying_saucer`).**
   - Doc 07 §2.3 lines 62 to 64 call 999 "near-instant" and say it "overflows to full 360° instant alignment per
     tick".
   - Plan 03 §2.2 gives the marine "999.0°/s … Instantaneous".
   - Plan 16 §1.2 made `catalog.rs` clamp the field to 0 to 180 **degrees per tick**, with 180 or more meaning instant.

   So every catalog value, including the 360 of the Siege Tank, Thor, Colossus and Brood Lord analogues, turns
   instantly. The reference is 56° per tick for most units, 40° per tick on the 720 Normal °/s engine default, and
   20° per tick for the Siege Tank, Thor and Ultralisk. Turning 180° would then take 4, 5 or 9 ticks. The unit of the
   field is also inconsistent: doc 07 §2.3 reads it as degrees per second (280 °/s for the Colossus = 11.2° per
   tick), while `catalog.rs` reads it as degrees per tick.
2. **`welder_drone` backswing 6 (1 value).** Plan 17 §1 Bug 3 chose a damage point of 4 and a backswing of 6 for the
   melee workers and the zealot, "guaranteeing a distinct, readable windup animation". The damage point of 4 is
   class B: plan 17 gives it as SC2's "0.16 s", which is the Normal default 0.167 s × 25. The backswing has no such
   claim. The reference is 0.5 Normal s, 9 ticks. The same plan's zealot values are already decided by Q0036, and
   `razor_mantis` no longer carries them.
3. **`rhino_beetle` has an Overclock (class E, 3 values); `shock_golem` has none.** Doc 08 line 175 says Overclock
   unlocks "on `pulse_bot` and `shock_golem`: sacrifices 10/20 HP". Plan 21a §2.1 moved `rhino_beetle` from
   "Marauder / Roach" to the Roach but left the 20-life Overclock on it. The Roach has no Stimpack in StarCraft II.
   `golden_replay.ron` orders `UseStim` for its infantry squad at tick 260. Moving the capability is an identity
   change, not a time value, so the question asks it separately.
4. **`flying_saucer` has no clear counterpart (class E, 15 values).**
   - The catalog says "Viking / Scout".
   - Doc 07 says Phoenix, at 150/100 and 625 ticks.
   - The catalog has 125/75, 850 ticks, a 10 (+4 light) range-6 weapon, and speed 0.15.

   Its weapon period of 20 and its shield rate would match the Phoenix. Only its shield regeneration is audited
   (class B, WP10), because every candidate counterpart has the same rate.

## 7. Whole-matrix measurement of the corrections (WP3 step 5)

Not yet run. Plan WP3 step 5 runs it after WP2 has merged. It will use a scratch catalog with every class B and C
value of §5, groups P1 to W8 together with WP10, and the scheduled values of WP4 to WP8 at their post-WP2 state. It
will measure the whole matrix on 20 seeds and battle 33 on 68 seeds against the state after WP2 (plan D-5). For any
failure it reruns with groups removed one at a time.

## 8. Not found within the effort bound

- **Projectile speed: High Templar.** The High Templar's `HighTemplarWeaponMissile` has no mover element in any
  layer (`voidmulti` EffectData:1535 names the ammo unit).
- **Projectile speed: Spine Crawler.** The Spine Crawler's `SpineCrawlerWeapon` mover is likewise absent.
- **Projectile speed: Brood Lord.** The Brood Lord's attack spawns Broodlings (`BroodlingStrike` has an empty
  effect, `liberty` WeaponData:419); this is not modelled.
- **Mining time.** The data sets no mining time on `SCVHarvest`, `ProbeHarvest` or `DroneHarvest` (`liberty`
  AbilData:1094, :1097, :2241), so it is engine data. Liquipedia gives 1.99 s, Faster by its LotV convention
  ([Resources](https://liquipedia.net/starcraft2/Resources): "A worker occupies a mineral field for 1.99 seconds").
- **Siege Mode transformation time.** Siege Mode's `Stats` section has a delay of 0.5 and a duration of 3.5417
  Normal s (`liberty` AbilData:1323–1324). That they add to 4.0417 Normal s (72.2 ticks) is **verify**; Liquipedia's
  Siege Tank page gives no figure.
- **Thrown or accelerating missiles** (class E, no single speed): `LanzerTorpedoes` (Viking), `LiberatorMissileLaunchers`
  (Liberator), `BacklashRockets` (Banshee), `LongboltMissile` (Missile Turret). Their motion phases are in the table
  notes.

## 9. Sources

Game data (primary): SC2Mapster, SC2GameData, commit `5444e12f13` ("SC2 5.0.14.94342"),
<https://github.com/SC2Mapster/SC2GameData/tree/5444e12f13>. Layers `core`, `liberty`, `swarm`, `void`, `voidmulti`
(§2.2); `libertymulti`, `swarmmulti` and `balancemulti` were read only for the chain check.

Liquipedia wikitext, read 2026-11-04 with `action=raw`:
- [Game Speed](https://liquipedia.net/starcraft2/Game_Speed), for the time base, as in the investigation §1.3.
- [Resources](https://liquipedia.net/starcraft2/Resources), for the mining time.
- [Siege Tank (LotV)](https://liquipedia.net/starcraft2/Siege_Tank_(Legacy_of_the_Void)), for the Siege Mode
  cooldown 2.14.
- [Void Ray (LotV)](https://liquipedia.net/starcraft2/Void_Ray_(Legacy_of_the_Void)), for the build time 43 and
  patch 5.0.9.
- [Ultralisk (LotV)](https://liquipedia.net/starcraft2/Ultralisk_(Legacy_of_the_Void)), for Anabolic Synthesis +0.59
  in 5.0.14.
- [Hydralisk (LotV)](https://liquipedia.net/starcraft2/Hydralisk_(Legacy_of_the_Void)), for patch 5.0.12 research
  times and Nanomuscular Swell in 5.0.14.
- [Zergling (LotV)](https://liquipedia.net/starcraft2/Zergling_(Legacy_of_the_Void)), for the cooldown 0.497
  (−0.143) with Adrenal Glands, which confirms the two stacking `RateMultiplier +0.2` entries.

Project documents: doc 07 §2.2 and §2.3, doc 08 line 175, plan 03 §2.2, plan 16 §1.2, plan 17 §1 Bug 2 and Bug 3,
plan 21a §2.1, and the battle 33 investigation §1 and §2.
