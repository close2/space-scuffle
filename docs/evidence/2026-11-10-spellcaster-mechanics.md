# Evidence: spellcaster mechanics (plan `2026-11-10-spellcaster-mechanics`)

Measurements of the plan's data commits (§12), one section per package. Builds in
`~/scratch/cargo-target/spells-storm`; the matrix with `matchup_trace --all-battles --seeds 16 --jobs 8`
(the four seeds of record plus 16 jittered ones, 20 per battle), battle 33 with `--seeds 64`.

## 1. WP2b part 1: hazard zones removed (checksum schema 29)

Base: branch `spells-d` at `5a79962` (WP1a to WP1e with `main` merged, schema 28).

- **Goldens.** Re-recorded for the schema only. A temporary test (not committed) wrote, after every tick of
  both golden scripts, the `Debug` form of every entity, projectile, player's resources, the effect state and
  the render snapshot. Base and new dumps (`golden_replay` 3,821,123 lines, `golden_replay_orders` 3,253,807)
  are byte-identical once the base's lines of the removed `last_storm_damage_tick` (19,051 and 17,293) are
  dropped, except each snapshot's `checksum` field, which is the world checksum of the new schema. Both
  goldens pass in debug, release and `wasm32-wasip1` (wasmtime). The battle sandbox's spawn checksum was
  re-recorded for the same cause.
- **Matrix.** Every battle bit-identical to the base (`--compare` reports only battle 13, which this commit
  takes out of the matrix until the storm data brings it back). Expectation failures: 0 on the seeds of
  record, 3 over all seeds, as on the base (all three predate this plan).
- **Battle 33**, 64 seeds plus the four of record: every per-seed trace file and summary byte-identical to the
  base.
- **Throughput.** Not measured: with no hazard zone open, the removed phase 2 step iterated an empty list and
  its two `Vec::new()` calls allocated nothing, so the removal changes no hot path of the benchmark scenarios.

## 2. WP2b part 2: Psionic Storm as data

The storm is data only: the `cerebral_elder` ability (`units.ron`), the tree in `ability_effects.ron`, the
`psionic_storm` status in `statuses.ron` and the `UnlockAbility` of the `psionic_storm` research
(`upgrades.ron`). Every value is cited in the RON from SC2GameData 5.0.14 (`5444e12f13`), checked against the
local copy (`~/scratch/combat_wp3_data/`):

| Value | Game data | Ours |
|---|---|---|
| Energy | `liberty` AbilData:2067 (75; `swarm` :1110 the same) | 75 |
| Cooldown | `swarm` AbilData:1111, 2 Normal s (over `liberty`'s 2.5) | 36 ticks |
| Range | `liberty` AbilData:2061, 9 from the caster's edge | 9 |
| Backswing | `liberty` AbilData:2062, `CastOutroTime` 0.5 | 9 ticks |
| Persistent | `liberty` EffectData:2069-2076: initial and 6 periodic searches every 0.5712 | period 10.2, count 6 |
| Search | :2077-2084: radius 1.5; filters exclude structures, hidden, invulnerable | radius 1.5, any owner, ground and air, structures excluded |
| Status | `liberty` BehaviorData:709-716: 0.6665 s, period 0.571, initial and periodic damage | 11.901786 / 10.196429 ticks |
| Damage | EffectData:2089-2101: 10 initial with `Acquire` and `Flee`, 10 periodic without | the same |
| Research | `PsiStormTech` (`liberty` UpgradeData:1808) meets `UsePsiStorm` (AbilData:2070) | `UnlockAbility` |

No later layer (`void`, `voidmulti`) overrides any storm entry. No engine change was needed.

- **Tests.** `crates/sim_core/tests/psionic_storm_tests.rs` (14 tests): 80 on ticks 0, 10, 20, 31, 41, 51, 61, 71
  with armour ignored; overlapping storms still 80; a unit that leaves after the first search takes 20; shields
  first; own and air units hit, structures not; cloaked and burrowed units hit without detection; carried units
  untouched, their carrier hit; a barrier absorbs the first 60; held units stay; an idle unit that cannot answer
  flees the centre; an armed idle unit answers the caster; refused without research, under 75 energy and for 36
  ticks of cooldown; range 9 from the caster's edge; the 9-tick lock; the area published from the cast until its
  last search (61 ticks), with its source and radius 1.5. `catalog_reference_values_tests.rs` checks every value
  above as authored, `runtime_catalog_tests.rs` shows an edited `ability_effects.ron` changing a storm without
  recompiling.
- **Battle 13** (rewritten, B-24): the High Templar analogue (start energy 75, researched) storms eight held
  zerglings' centroid on the first tick. All die to the fourth pulse; the battle ends on tick 32 on all 20 matrix
  seeds, blue 1 – red 0.
- **Battles 36a and 36b** (new; the plan's armies, untuned): one High Templar analogue and six zealots, idle,
  against sixteen marines attack-moving into them, in contact from tick 109. With a storm on the marines'
  centroid on tick 150 (36a) the zealots win all 20 seeds, 4 or 5 of blue's 7 left (11 and 9 seeds), ending on
  ticks 248 to 325. Without it (36b) the marines win all 20, 9 to 11 left (1, 8 and 11 seeds), ending on ticks 345
  to 368.
- **Matrix.** Against the base (`5a79962`), `--compare` reports only 13, 36a and 36b; every other battle is
  bit-identical. Expectation failures: 0 on the seeds of record, 3 over all seeds (26 on seeds 10 and 13, 33 on
  seed 9), the same three as on the base.
- **Battle 33**, 64 seeds plus the four of record: every trace file byte-identical to the base.
- **Goldens** unchanged by the data (no caster in either script): they pass in debug, release and
  `wasm32-wasip1` with the schema 29 lists of part 1.

## 3. WP3: Abduct

Abduct is data: the `tongue_chameleon` ability (`units.ron`), four named nodes in `ability_effects.ron` and the
`abducted` status in `statuses.ron`. One generic engine fix came first, in its own commit (plan Amendment 5):
origin placement and landing ignore units, as `PlacementIgnoreBlockers` does. Every value is cited in the RON
from SC2GameData 5.0.14 (`5444e12f13`), checked against the local copy:

| Value | Game data | Ours |
|---|---|---|
| Energy, range | `swarm` AbilData:1230, :1233 (75; 9) | 75; 9 edge to edge |
| Cooldown | none in `swarm` AbilData:1222-1239 | 0 |
| Backswing | `voidmulti` AbilData:1754, `CastOutroTime` 0.8 | 14 ticks |
| Targets | `voidmulti` AbilData:1755; `NoYoink`, `swarm` EffectData:1468 | any but self, ground and air, no structures, not already abducted |
| Landing point | `YoinkStartCreatePlaceholder`, `swarm` EffectData:1447-1471 (offset 0,0.1 :1460; range 2 :1459; ignores units :1453) | origin 0.1 ahead, ring within 2, terrain and structures only |
| Tongue | `YoinkLaunchMissile` :1486-1494; `YoinkMissile` MaxSpeed 18.75, MoverData:147 | homing tether, 1.05 per tick |
| Contact | `YoinkSet` :1498-1509: cancel orders, instant unburrow, `Yoink` buff, delay persistent | `CancelOrders(force_unburrow)`, `ApplyStatus(abducted)`, `abduct_delay` |
| Delay | `YoinkDelayPersistent` :1539-1547, period 0.1 | one pulse of 1.785714 ticks: 2 |
| Flight | `YoinkLaunchTarget` :1554-1565; `YoinkMover` FlightTime 1, MoverData:167 | `Displace` 18 ticks, tether, `on_land` removes the status |
| Status | `Yoink`, `swarm` BehaviorData:1074-1088: stun, suppress cloak, collision, passenger; 2 s | `abducted`: 35.714286 ticks, stun, reveal, no collision, no boarding |

- **Tests.** `crates/sim_core/tests/abduct_tests.rs` (18 tests): contact on cast + ceil(gap / 1.05), the 2-tick
  delay stunned in place, 18 ticks of straight flight, landing on contact + 20 within 0.1 of the caster's cast
  position with the status removed; the point fixed at the cast though the caster flies off or dies; a ground
  unit pulled up a cliff lands on the caster's level; an air unit and a massive unit pulled; own units allowed,
  the caster itself and a structure refused; contact clears orders, hold and target, stuns, lifts collision,
  forbids boarding, and the target does not attack during the pull; orders given meanwhile start after landing
  (SU-3); a sieged tank lands sieged and a siege in progress completes; a detected burrowed unit is unburrowed at
  contact and revealed, an undetected one cannot be targeted; an undetected cloaked enemy refused with no payment,
  allowed once detected; a tongue at a target that boards a transport is dropped and the energy stays spent; no
  landing point (caster over unpathable ground) refuses the cast with no payment (SU-4); the landing found on the
  ring 2 away; a unit under the caster does not move the landing point; the 14-tick lock; range 9 edge to edge; a
  second Abduct on an abducted unit refused; the tether in `RenderSnapshot::links` on every tick from the cast to
  the landing. `effect_tree_tests.rs` gains the engine test of the fix. `catalog_reference_values_tests.rs` checks
  every value as authored; `runtime_catalog_tests.rs` shortens the flight in a copied `ability_effects.ron` (18 to
  6 ticks) without recompiling.
- **Red runs.** The tests' first run with the data failed 9 of 18 on the tests' own setup (a cast before the
  first tick computes vision is refused; a siege takes 72 ticks plus its random part, beyond 80 steps; regeneration
  moves energy); fixed in the tests. Without the engine fix the air-target and blocked-landing tests fail, and so
  does the engine test.
- **Matrix.** Against the WP2b measurement of `4b47bef` (`matrix.csv` of its §2 run), `--compare` reports the
  whole matrix bit-identical; expectation failures 0 on the seeds of record, 3 over all seeds (the same three).
- **Battle 33**, 64 seeds plus the four of record: every trace file both runs wrote is byte-identical to the base.
- **Goldens** unchanged (no caster in either script), passing in debug, release and `wasm32-wasip1`. The DHAT
  allocation test passes.
- **Battles 37a/37b not landed.** Arrangements measured on the 20 matrix seeds, with the fixture extensions of
  Amendment 5: (1) the plan's armies, zerglings and Viper attack-moving together: the Viper sees the tank only
  from about 9.5 away, inside the tanks' range 13 of the zerglings beside it, so the first volley kills four or
  five zerglings before the cast, and the pulled tank lands behind zerglings that ran on; (2) the Viper waiting on
  a flank, the zerglings timed to meet the landing: the pulled tank dies on most seeds, but the second sieged
  tank, within 13 of the landing point, and the marines kill the rest; (3) the tanks 6 apart so the landing lies
  out of the second tank's range: with 8 zerglings red keeps 5 of 6 on 19 seeds (4 on one) with Abduct and all 6
  on every seed without it; with 12 to 20 zerglings red keeps 1 to 5 with Abduct and 2 to 6 without. The claim
  (opposite winners) needs a further arrangement or army size; Abduct's effect, the pulled tank killed, is clear
  in (3).
- **Battles 37a/37b, landed under plan Amendment 6** (the spell's own effect). Arrangement (3) with eight
  zerglings, red's tanks spawned first so the pulled tank is red's lead unit: the Viper analogue waits at
  (50, 37.3), the zerglings attack-move from (39, 20.5) to (60, 56), and Abduct is cast on tick 90 at the first
  tank (56, 44); the second tank stands 6 away. The fixture's `Expected` gained `blue_lead_alive` and
  `red_lead_alive` (generic, unit-tested in `sim_fixtures`). 37a: red's lead dead on all 20 matrix seeds, red 5
  left (18 seeds) or 4 (2), every zergling dead, the Viper alive at the 600-tick limit. 37b: red's lead alive on
  all 20 with 94 to 175 life, red 6, blue 1. On 52 seeds the same split held (lead dead on all 52 with Abduct,
  alive on all 52 without). Against the base (`57f2747`, `--compare`) only 37a and 37b are new; every other
  battle is bit-identical. Expectation failures 0 on the seeds of record, 3 over all seeds (the same three).

## 4. WP4: Fungal Growth

Fungal Growth is data: the `mycelium_shaman` ability (`units.ron`), three named nodes in `ability_effects.ron` and
the `fungal_growth` and `fungal_growth_slow` statuses in `statuses.ron`. No engine change was needed. Every value
is cited in the RON from SC2GameData 5.0.14 (`5444e12f13`), checked against the local copy:

| Value | Game data | Ours |
|---|---|---|
| Energy | `liberty` AbilData:486 (75) | 75 |
| Cooldown | `liberty` AbilData:487, the `Leech` link with no time | 0 |
| Range | `void` AbilData:674 (10; `liberty` :488 had 9) | 10 from the caster's edge |
| Backswing | no `CastIntroTime` or `CastOutroTime` in the chain or the `CAbilEffect` default (`core` AbilData:34-88) | 0 (D-7.5 verified) |
| Missile | `FungalGrowthLaunchMissile`, `void` EffectData:1141-1146; `FungalGrowthWeapon` MaxSpeed 12, `void` MoverData:18 | to the point, 0.672 per tick |
| Impact | `FungalGrowthInitialSet`, `liberty` EffectData:416-420, `SurfaceForSpellcast` removed at `voidmulti` :693-695 | search, then the set |
| Search | `liberty` EffectData:728-735 (filters :731); radius `voidmulti` :1559 | 2.25, enemies, ground and air, no structures |
| Set | `FungalGrowthSet`, `liberty` :714-719; slot 1 the slow, `voidmulti` :1562 | `fungal_growth`, `fungal_growth_slow` |
| Responses | `FungalGrowthSearchDummy`, `liberty` :736-741 (radius 2.25 `voidmulti` :407-409, filters `void` :221-223) | `Damage(0)`, acquire and flee |
| Damage status | `FungalGrowth` buff, `liberty` BehaviorData:328-338 (SuppressCloak :332, PeriodicEffect :337); Duration 4 and Period 0.25, `swarm` :838-841 | 71.428571 ticks, period 4.464286, reveal, no Blink |
| Pulse | `FungalGrowthDamage` 1.5625, `voidmulti` EffectData:2389; armoured bonus 0, `void` :1139 | 1.5625 × 16 = 25 |
| Slow | `FungalGrowthSlowMovement`, `voidmulti` BehaviorData:691-705 (Duration :696, MoveSpeedMultiplier 0.25 :697, SuppressPassenger :698) | 71.428571 ticks, × 0.25, no boarding |
| No Blink | `Blink` validator `CasterNotFungalGrowthed`, `swarm` EffectData:2180-2182, ValidatorData:537-541 | on `fungal_growth` (plan Amendment 7) |

- **Tests.** `crates/sim_core/tests/fungal_growth_tests.rs` (14 tests): the missile lands 13 ticks after its
  cast tick for 8.5 world units (`ceil(8.5 / 0.672)`) and is published in `spell_missiles` with its source while
  it flies; 25 in 16 pulses on ticks 4, 9, 13, 18, 22, 27, 31, 36, 40, 45, 49, 54, 58, 63, 67 and 71 after the
  impact, armour ignored; the search takes an enemy 2.2 away and air units, and misses an enemy 2.3 away, an own
  unit inside and a structure inside; a carried unit untouched while its carrier is hit; walking speed × 0.25
  (measured ratio within 0.01) with the `Slowed`, `DamageOverTime` and `Revealed` flags published; Blink and
  boarding refused while affected and allowed once both statuses end, together, on the 72nd tick; a cloaked
  enemy revealed (a blue marine without detection shoots it) and hidden again on the 72nd tick; a burrowed enemy
  revealed while it stays burrowed (SU-7); a recast refreshes both statuses with the first phase kept, one
  pulse per due tick and no damage at the second impact; an armed idle victim answers the caster (the dummy's
  `Acquire`); the missile lands after its caster died; 75 energy, no cooldown, no lock; range 10.625 from the
  caster's centre; a cast order out of range walks into range and casts there.
  `catalog_reference_values_tests.rs` checks every value above as authored; `runtime_catalog_tests.rs` changes
  the slow from 0.25 to 0.5 in a copied `statuses.ron` without recompiling.
- **Red runs.** Before the data every Fungal test failed at its first step (no ability named Fungal Growth), as
  did the reference and runtime tests. With the data, 3 of 14 failed on the tests' own setup, fixed in the
  tests: the impact counted from the step after a between-ticks cast (Amendment 7), the expiry stepped one tick
  short, and armed red and blue units placed side by side fought each other.
- **Battles 38a/38b** (the plan's armies, untuned but for placement): ten marines hold in a clump at (50, 42.5);
  sixteen zerglings attack-move from (30, 42.5); the Infestor analogue waits at (41, 45), 9 from the marines and
  out of their range (`Squad::order`), and casts at the marines' centroid on tick 70 as the zerglings arrive.
  38a: the zerglings win all 20 matrix seeds with 2 to 7 left (blue 3 to 8 with the caster; 6 left on 8 seeds),
  ending on ticks 222 to 286. 38b (no cast): the marines win all 20 with 6 to 8 left (10, 9 and 1 seeds); every
  zergling dies and the idle caster outlives the 400-tick limit. On 52 seeds the split held on every seed (38b
  once with 5 marines left, inside its range), and with the cast on tick 65 or 75 as well.
- **Matrix.** Against the base (`57f2747`, `--compare`) only 37a, 37b, 38a and 38b are new; every other battle
  is bit-identical, and 37a/37b are identical to their run before the Fungal data. Expectation failures 0 on the
  seeds of record, 3 over all seeds (the same three).
- **Battle 33**, 64 seeds plus the four of record: all 206 trace files byte-identical to the WP3 run of
  `57f2747`.
- **Goldens** unchanged (no caster in either script), passing in debug, release and `wasm32-wasip1`
  (wasmtime). The DHAT allocation test (`effect_tree_allocation_tests.rs`) passes.

## 5. WP5: melee weapons and Blinding Cloud

Builds in `~/scratch/cargo-target/spells-cloud`. Base: branch `spells-fungal` at `0beb4e4` (WP4 done, schema 29);
its matrix and battle 33 (68 seeds, 206 trace files, byte-identical to the WP4 run) are the base of every
comparison below.

### 5.1 WP5a: `WeaponSpec::melee` from the game data (`4f62d8c`)

Every `CWeapon` of the local 5.0.14 chain (`~/scratch/combat_wp3_data/`, layers `core` to `voidmulti`) was read
for `Options index="Melee"`: no `CWeapon` default sets it (`core` WeaponData:3-31) and no later layer clears it
(no `value="0"` anywhere). Eight catalog weapons have a melee counterpart and now carry `melee: true`, each cited
in `units.ron`:

| Analogue | Catalog weapon | Game weapon |
|---|---|---|
| SCV | Drill Slash | `FusionCutter`, `liberty` WeaponData:195-201 |
| Probe | Particle Beam | `ParticleBeam`, :142-148 |
| Zealot | Psi-Blade Double Slash | `PsiBlades`, :150-158 |
| Dark Templar | Warp Blade | `WarpBlades`, :161-169 |
| Drone | Mandible Bite | `Spines`, :349-355 |
| Zergling | Scythe Slash | `Claws`, :357-363 |
| Baneling | Volatile Burst (and its structure strike) | `VolatileBurst`, :463-472; `VolatileBurstBuilding`, `swarm` :200-203 |
| Ultralisk | Kaiser Blades Cleave | `KaiserBlades`, :430-439 |

The other melee weapons of the data have no catalog counterpart: the Roach's and Hydralisk's melee weapons
(`RoachMelee` :86-89, `HydraliskMelee` :46-49; the analogues carry only their ranged weapon), the Ultralisk's
`Ram`, the Broodling's `NeedleClaws`, the Locust's, the Warhound's, the Sheep's and two Baneling dummies.
`d14_melee_weapons_follow_starcraft` (`catalog_reference_values_tests.rs`) checks every weapon block, siege-mode
weapons included; it failed on the first unflagged weapon before the data.

**Hashed fields.** `WeaponSpec::melee` is hashed only inside the `WeaponSpec` copies that projectiles
(`ActiveProjectile::weapon_spec`) and siege weapons (`Entity::siege_weapon`) hold. None of the eight weapons has a
projectile speed or belongs to a mode, so no hashed value changes; no rule reads the flag but the ranged-range
status delta. Measured: the matrix (20 seeds) is bit-identical to the base (`--compare`), battle 33's 206 trace
files are byte-identical, and both goldens pass unchanged in debug, release and `wasm32-wasip1` (wasmtime).

### 5.2 WP5b: Blinding Cloud as data (`370225a` tests, `83c74de` data)

Blinding Cloud is data: the second ability of `tongue_chameleon` (`units.ron`), two named nodes in
`ability_effects.ron` and the statuses `blinding_cloud` and `blinding_cloud_structure` in `statuses.ron`. No
engine change was needed. Every value is cited in the RON from SC2GameData 5.0.14 (`5444e12f13`), checked against
the local copy:

| Value | Game data | Ours |
|---|---|---|
| Energy, range | `swarm` AbilData:1202, :1204 (100; 10) | 100; 10 from the caster's edge |
| Cooldown, backswing | none in `swarm` AbilData:1197-1209, no entry in `void` or `voidmulti`, none in the `CAbilEffect` default (`core` AbilData:34-88) | 0, 0 |
| Cloud | `BlindingCloudCP`, `swarm` EffectData:236-242 (period 0.5 :241); `PeriodCount` 19 (:239), 27 (`void` :1012-1014), 15 (`voidmulti` :157-159) | persistent at the point, initial search and 15 more every 8.928572 ticks |
| Search | `BlindingCloudSearch`, :243-248 (radius 2 :247; `SearchFilters "Ground;Self,Neutral,Missile,Dead"` :246) | radius 2, any owner but the caster, ground only, structures included |
| Branches | `BlindingCloudSwitch` :254-258 (`Case IsStructure`, `CaseDefault BlindingCloudAB`) | a set of two `ApplyStatus` nodes filtered `structures: Exclude` / `Only` |
| Unit status | `BlindingCloud`, `swarm` BehaviorData:994-1000 (0.625 s :997); `voidmulti` :1067-1069 (`WeaponRange 0, RangedWeaponRange -20`) | 11.160714 ticks, ranged weapons −20, floor 0.1 (SU-2) |
| Structure status | `BlindingCloudStructure`, `swarm` BehaviorData:19-26 (`WeaponRange -20 DetectArc 0 RadarArc 0` :25) | 11.160714 ticks, every weapon −20, no detection |

**The period.** The exact period is 0.5 × 25 ÷ 1.4 = 8.9285714 ticks, and the eighth search's exact time,
7 × 8.9285714 = 62.5, rounds half up to 63. The six-decimal 8.928571 of plan §5.2 gives 62.499997 and the 62nd
tick; the catalog authors 8.928572, rounded up as Fungal's 4.464286 is, so the 16 searches fall on ticks 0, 9, 18,
27, 36, 45, 54, 63, 71, 80, 89, 98, 107, 116, 125 and 134, the exact products rounded half up.

- **Tests.** `crates/sim_core/tests/blinding_cloud_tests.rs` (10 tests): the 16 searches on those ticks, each
  refreshing one status in place, which ends 12 ticks after the last search (11.16 elapsed); own, enemy and
  cloaked ground units within 2 of the centre hit, one 2.4 away, an air unit and the caster not, with the
  `RangeReduced` flag published; a marine's range 5 down to the floor 0.1 while a zergling's melee weapon keeps
  its own; a blinded marine never hits an unarmed unit 1 away but hits a zergling in contact, and the zergling
  hits it; a marine outside the cloud keeps range 5 and hits a unit inside; a marine walking out keeps the status
  and the floor until 12 ticks after the only search that found it, then range 5; a sieged tank in the cloud
  (range 0.1 under its minimum range 2) fires at nothing, near or far, until the status ends, then at the far
  target; a Photon Cannon analogue gets the structure status only, every weapon (melee included) at the floor,
  stops revealing a cloaked Dark Templar analogue and shooting it, and detects it again once the status ends;
  100 energy, range 11 from the caster's centre, no cooldown (two casts at once); the area published from the
  cast to its last search, 134 ticks, with its source and radius 2. `catalog_reference_values_tests.rs` checks
  every value above as authored; `runtime_catalog_tests.rs` changes the reduction from −20 to −2 in a copied
  `statuses.ron` and a range-5 weapon keeps 3, without recompiling.
- **Red runs.** Before the data all 10 tests failed at their first step (no ability named Blinding Cloud); the
  structure test also failed earlier on its own setup (a Dark Templar analogue is not cloaked by itself), fixed by
  cloaking it. With the data, 6 of 10 failed on the tests' own setup, fixed in the tests: three casters stood
  12.8 from the point, out of range 11; the floor compared against `from_num(0.1)` instead of the catalog's
  `1 / 10`; and a marine shot its target during two ticks stepped before the cast.
- **Battles 39a/39b** (the plan's armies; placement and cast tick tuned): three sieged tanks hold in a column
  1.8 apart, each within 2 of their centroid; eight roaches attack-move into them from 18 away; the Viper
  analogue waits 9 from the tanks (they cannot shoot air) and clouds their centroid on tick 40. 39a: the roaches
  win all 20 matrix seeds, blue keeping 7 to 9 (9, 6 and 5 seeds) with the caster, ending on ticks 260 to 298.
  39b (no cast): the tanks win all 20 with 2 or 3 left (11 and 9 seeds); every roach dies and the Viper analogue
  outlives the 600-tick limit. On 52 seeds the split held on every seed (39a blue 5 to 9, 39b red 2 or 3), and
  with the cast on tick 35 or 45 as well. Measured first: with the roaches 2 nearer and the cast on tick 10, one
  of 52 control seeds was a roach win; with seven roaches, 39a kept as few as two.
- **Matrix.** Against the base (`--compare`), only 39a and 39b are new; every other battle is bit-identical.
  Expectation failures 0 on the seeds of record, 3 over all seeds (26 on seeds 10 and 13, 33 on seed 9), the same
  three as on the base.
- **Battle 33**, 64 seeds plus the four of record: all 206 trace files byte-identical to the base.
- **Goldens** unchanged (no caster in either script), passing in debug, release and `wasm32-wasip1` (wasmtime).
- **Presentation (WP5c)** not started: the cloud is published in `RenderSnapshot::areas` and the statuses as the
  `RangeReduced` flag; its drawing waits on Q0080 with the other spells' (D-11).

## 6. WP6: throughput and steady-state allocation

**Throughput** (`cb55683`, `3aaec66`). `cargo bench -p sim_core --bench throughput`, release, under the gate lock
(`flock -o ~/scratch/gate.lock`), on 2026-11-11 at load average 12 to 13 (another user's builds); median of three
repeats, 300 measured ticks each. The new row is the 64x64 battle of 400 marines with 29 Viper, 19 High Templar and
19 Infestor analogues of the first player casting Blinding Cloud (every 6 ticks), Psionic Storm (every 12) and
Fungal Growth (every 10) at the second army's centroid, after 250 warm ticks:

| Map | Units | Ticks/s | `DESIGN.md` §5 (2026-11-10) |
|---|---:|---:|---:|
| 64x64 | 0 | 3367 | 3392 |
| 64x64 | 100 | 1002 | 1031 |
| 64x64 | 400 | 170 | 173 |
| 128x128 | 400 | 200 | 203 |
| 256x256 | 0 | 210 | 208 |
| showcase_canyon | 400 | 123 | 127 |
| 64x64, 67 casters | 400 | 243 | — |

The spell row held 13.7 live areas and 101.4 statuses per measured tick on average. Every row is above the floor
of 25 ticks per second, and the existing rows are within 3 % of the figures of record (no simulation code changed
in WP5; the spell catalogs differ only in data no existing row uses). The spell row runs faster than the plain
400-unit row because its measured ticks start after 250 ticks of battle, when fewer units live. Two earlier
runs of the same tree: one under a load average of 22 to 180 (2833 ticks/s on the empty map, spell row 148, with
150 warm ticks: 8.9 areas and 64 statuses, the casts before the armies see each other being refused for want of
target vision), and one with clouds every 8 ticks and Fungal every 12 (spell row 220 ticks/s, 15.2 areas, 90.0
statuses).

**Allocation** (`cb55683`). `effect_tree_allocation_tests.rs` gained a child that recasts the four shipped spells
(storm every 60 ticks, cloud every 130, Fungal Growth every 70, Abduct every 40) on a held clump of 20 units whose
life is restored each tick, so each spell is live throughout the 200 measured ticks. Under DHAT its engine blocks
are 8 (30,432 bytes, the world's construction) with 0 and with 200 measured ticks: the shipped spells allocate
nothing in steady state. The existing children are unchanged (steady 8 and 8, planted outside 8 and 8 with 200
more window blocks, planted engine 8 and 14).

**Finding (B-114).** The child's first schedule recast Abduct every 20 ticks, so a second tongue reached the unit on
the tick its first pull landed: the contact refreshed `abducted`, the landing then removed it, and the second pull
flew without the status (no stun, collision rule or boarding ban). The game's behaviour is not yet sourced;
recorded as B-114 in `DESIGN.md` for a WP3 follow-up. The test now recasts only after the landing.

**Client checks** (asked by the orchestrator): `cargo test -p client --lib` failed on a hard-coded count of shipped
casts (`battle_hud::catalog`, 4 against 3 since Psionic Storm shipped); the test now counts the shipped casts first
(`84902a6`). `client --lib` (380), `battle_card_tests` (28) and `battle_cast_tests` (6) pass.

## 7. B-114 fixed: Abduct's mark (plan Amendment 9)

Base: `spells-cloud` head `fc4c811` (schema 29). Outputs under `~/scratch/bugfix-113/` (not committed).

**What the game does** (SC2GameData 5.0.14, `swarm` layer; no later layer touches these entries).
The ability's effect `YoinkStartSwitch` runs `YoinkStartCreatePlaceholder` (EffectData:1447-1471),
whose validators include `NoYoink` (:1468): a `CValidatorUnitCompareBehaviorCount` on the buff
`YoinkMarker`, failing with "Can't target multiple Abduct" (ValidatorData:812-815). Its spawn effect
`YoinkStartSet` (:1472-1478) applies `YoinkMarker` to the target (`YoinkMarkerBehavior`, :1482-1485)
before it launches the tongue. `YoinkMarker` (BehaviorData:1089-1096) is hidden, has no
modification and lasts 2 Normal s (35.71 ticks). The tongue's flight (at most about 12 ticks at cast
range), the delay (2) and the flight (18) all end within it, so the game never lets a second tongue
reach a unit whose pull is under way. The `Yoink` buff itself refreshes (one stack) and
`YoinkRemoveBehavior` removes it at landing whoever applied it, as our engine already does.

**Cause.** Our D-8 put `without_status: "abducted"` on the ability: `abducted` (the `Yoink` buff)
starts only at contact, so a second cast was admitted before the first contact (and a direct walk
at any time). A second tongue reaching the unit late in the first flight refreshed `abducted`; the
first landing removed it; the second delay then launched a flight without the status.

**Fix (data only).** Status `abduct_mark` (`YoinkMarker`); root `abduct` a `Set` with `require:
without_status abduct_mark` (`NoYoink`) whose children apply the mark and launch the tongue
(`abduct_tongue`); the ability's target filter asks the same mark.

**Tests** (`abduct_tests.rs`). `a_second_tongue_never_meets_a_unit_whose_pull_is_under_way` walks
the tree a second time 0 to 35 ticks after a first walk that lands the unit where it stands (the
allocation child's case), with the tongue from the far side. Red on the base: offset 19, the unit
flies from tick 28 without `abducted` (the second contact refreshed it before the first landing on
tick 27 removed it); green after. `a_second_abduct_is_refused_while_the_first_casts_mark_lasts`
(red on the base: no mark status) checks a second Viper refused on the cast tick and through the
pull, then admitted once the mark expires, its pull keeping the status. The catalog reference test
checks the new nodes and the mark's values. All `sim_core` tests pass, the allocation test under
valgrind included.

**Measurements** against the base build. Matrix (`--seeds 16`, 1,160 runs): bit-identical, 0
expectation failures on the seeds of record, 3 over all seeds as before. Battle 33 (`--seeds 64`):
every file identical. Showcase (`route_trace --ticks 1800`): `units.csv` and `summary.md`
identical. Golden replays pass unchanged (no Abduct in them); no re-recording.

**Found while tracing: B-115.** A homing missile steps `min(speed, gap)`, which puts it exactly at
contact, and impacts only when the next tick's check finds the gap at most 2^-20. A target that moved
away in between is never reached: in the trace the second tongue trailed the flying unit at contact
distance for 20 ticks and landed two ticks after it stopped. By the same step rule a unit walking
away from a tongue escapes it until it stops (not traced). Registered in `DESIGN.md`, not fixed here
(it changes every tongue's timing).
