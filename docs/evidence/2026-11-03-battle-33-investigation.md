# Battle 33 investigation: marines against zealots (Q0007)

This is the evidence file of [the battle 33 investigation plan](../plans/2026-11-04-zealot-marine-investigation.md),
which answers the owner's request in [Q0007](../questions/open/0007-combat-baselines.md): find out why published
StarCraft II sources say zealots win this fight, and why our battle 33 leaves 6 to 13 of 24 marines alive. The plan's
D-5 gives this file seven sections. **Sections 1 (WP1, reference research), 2 and 3 (WP2, comparison and analytic
model, with WP3's measured contact in §3.5), 4 (WP3, trace findings) and 5 (WP4, experiments) are written so far.**
WP5 adds sections 6 and 7. This file changes no catalog value, combat rule or expected result.

## 1. Reference: what StarCraft II does (WP1)

Written 2026-11-03 against `main` at `b4a223e`.

### 1.1 Summary

1. **The game data settles the unit numbers.** The values below come from the StarCraft II 5.0.14.94342 game data
   (the last 5.0.14 build), checked against Liquipedia. They agree everywhere except where Liquipedia rounds a value or
   still shows a pre-4.11 number (§1.11).
2. **The zealot strikes twice, 0.2 s apart.** The two 8-damage strikes are not on the same frame. The second lands
   0.2 Faster seconds (5 of our ticks) after the first, and only if the target is still within 2.1 of the zealot.
   The zealot's damage point is **0**, so there is no wind-up that could be cancelled.
3. **The data uses Normal time; Liquipedia uses Faster.** Editor values are in Normal-speed seconds. Liquipedia's
   Legacy of the Void pages are in Faster seconds. The factor is 1.4. Our catalog's marine speed (0.09 per tick =
   2.25 per second) and zealot shield regeneration (0.08 per tick = 2 per second) equal the **Normal-time** editor
   values with no conversion. The reference values at 25 ticks per Faster second are 0.126 and 0.112 per tick.
4. **Observed outcomes do not support "zealots win" at 24 against 12 without Charge.** At this size, with no upgrades,
   the published tests that come closest to battle 33 show marines winning. The best-matched test is 50 marines
   against 25 zealots with no upgrades and no micro. The marines won with 20 of 50 alive, 40 %. Our battle leaves 25 %
   to 54 % alive. Zealot wins are reported in three cases:
   - at small counts (1 against 2, 3 against 6);
   - with Charge;
   - from published opinion, with no test behind it.

   Every observed-outcome source is a single community test, and most are from 2010 to 2011 (§1.9). The evidence is
   weak, but it all points the same way.

### 1.2 Method and sources

**Primary source: the game data.** SC2Mapster's [SC2GameData](https://github.com/SC2Mapster/SC2GameData) repository
mirrors the game's data files. We used commit
[`5444e12f13`](https://github.com/SC2Mapster/SC2GameData/tree/5444e12f13), titled "SC2 5.0.14.94342". It is the last
5.0.14 build; 5.0.15 follows. The catalog targets patch 5.0.14
([doc 07](../07-sc2-patch-5-0-14-roster-and-faction-mapping.md)).

The data is layered. Each layer overrides the one before it:

`core` → `liberty` → `libertymulti` → `swarm` → `swarmmulti` → `void` → `voidmulti` → `balancemulti`.

The table cells cite the file under `mods/<layer>.sc2mod/base.sc2data/GameData/` and the line of the element. A
value that no layer sets comes from the `default="1"` element in `core`, which the cell says. Where `voidmulti` and
`balancemulti` both override a value, they agree for every value cited here.

**Secondary sources, in order of weight:**

1. Blizzard patch notes.
2. Liquipedia's wikitext, read with `action=raw`. The rendered pages refuse automated fetches.
3. The SC2Mapster wiki, for what editor fields mean.
4. Community tests and forum posts.

A value from one secondary source only is marked *(single secondary source)*.

**Conversions.** Every time value in the editor is in **Normal**-speed seconds
([Liquipedia, Game Speed](https://liquipedia.net/starcraft2/Game_Speed)). Faster seconds are Normal seconds ÷ 1.4;
speeds and rates per second are multiplied by 1.4. Our tick values are Faster seconds × 25
([doc 07](../07-sc2-patch-5-0-14-roster-and-faction-mapping.md)). The "ticks" columns give the exact quotient. We do
not round, so the difference from the catalog stays visible.

### 1.3 Time base

| Item | Value | Source |
|---|---|---|
| Base of editor and data values | Normal speed | [Liquipedia, Game Speed](https://liquipedia.net/starcraft2/Game_Speed): "Internally, as well as in the editor, all time-related values are based on Normal speed." |
| Base of Liquipedia Legacy of the Void infoboxes | Faster speed (real time) | Same page: "On Liquipedia, all Legacy of the Void pages use real-time time and speed values corresponding to Faster game speed." |
| Faster ÷ Normal | 1.4 (exactly 5734/4096 = 1.39990234375) | Same page, speed table |
| Game loops per Faster second | 22.4 | [s2client-proto `docs/protocol.md`](https://github.com/Blizzard/s2client-proto/blob/master/docs/protocol.md) ("faster" game speed, 22.4 gameloops per second) |
| Game loops per Normal second | 16 (= 22.4 ÷ 1.4; derived) | Derived from the two rows above |
| Our ticks per Faster second | 25 | [doc 07](../07-sc2-patch-5-0-14-roster-and-faction-mapping.md) §2.1 |
| Base of in-game tooltips since Legacy of the Void | Real time (Faster in ladder) | [Liquipedia, Game Speed](https://liquipedia.net/starcraft2/Game_Speed), "Legacy of the Void" section |

### 1.4 Marine

Patch 5.0.14.94342 unless stated. Liquipedia's [Marine page](https://liquipedia.net/starcraft2/Marine_(Legacy_of_the_Void))
records "No patch changes so far in LotV". Every marine value below is therefore the same in Wings of Liberty, which
matters for the 2011 tests in §1.9.

| Field | Editor (Normal) | Faster | Our ticks (×25) | Source |
|---|---|---|---|---|
| Life | 45 | — | — | `liberty` UnitData:4814 (`LifeMax`); Liquipedia infobox `hp=45 (+10)` |
| Armour | 0 (none set) | — | — | `liberty` UnitData:4814 sets no `LifeArmor`; Liquipedia `armor=0 (+1)` |
| Attributes | Light, Biological | — | — | `liberty` UnitData:4814; Liquipedia `attributes=Biological, Light` |
| Movement speed | 2.25 | **3.15** | **0.126 per tick** | `liberty` UnitData:4814 (`Speed`); Liquipedia `speed=3.15 (+1.57)` |
| Acceleration | 1000 (effectively instant) | — | — | `liberty` UnitData:4814; [Liquipedia, Speed](https://liquipedia.net/starcraft2/Speed) §Acceleration: ground non-worker units "have effectively instantaneous acceleration (1000)" |
| Deceleration | not set in any layer | — | — | Not found as a published number; treated as instant by Liquipedia's statement above |
| Lateral acceleration | 46.0625 | 64.49 | — | `liberty` UnitData:4814 |
| Turn rate (moving and stationary) | 999.8437 °/s | 1399.8 °/s | 56 ° per tick (instant) | `liberty` UnitData:4814 (`TurningRate`, `StationaryTurningRate`) |
| Radius | 0.375 | — | — | `liberty` UnitData:4814 (`Radius`) |
| Separation radius | 0.375 | — | — | `liberty` UnitData:4814 (`SeparationRadius`) |
| Inner radius | 0.375 | — | — | `liberty` UnitData:4814 (`InnerRadius`) |
| Push priority | 10 (default) | — | — | `core` UnitData:59 (`CUnit default`, `PushPriority`); no layer overrides it for the marine |
| Mass | 0.4 (default) | — | — | `core` UnitData default |
| Sight | 9 | — | — | `liberty` UnitData:4814; Liquipedia `sight=9` |
| Attack target priority | 20 | — | — | `liberty` UnitData:4814 (`AttackTargetPriority`) |
| Weapon | C-14 Gauss Rifle (`GuassRifle`, the data's spelling) | — | — | `liberty` UnitData:4814 (`WeaponArray`) |
| Damage | 6, kind Ranged | — | — | `liberty` EffectData:1625 (`CEffectDamage id="GuassRifle"`); Liquipedia `6 (+1)` |
| Delivery | instant: the weapon applies a damage effect directly, with no missile | — | — | `liberty` WeaponData:203 (no launch or missile effect); `liberty` EffectData:1625 |
| Range | 5 (default) | — | — | `core` WeaponData:12; Liquipedia `range=5` |
| Range slop (how far a target may move past range before it is lost) | 1 (default) | — | — | `core` WeaponData:13; meaning per [SC2Mapster wiki, Legacy weapons](https://sc2mapster.wiki.gg/wiki/Data/Weapons/Legacy) |
| Minimum scan (acquisition) range | 5.5 | — | — | `voidmulti` WeaponData:107 and `balancemulti` WeaponData:102; [Liquipedia, Automatic Targeting](https://liquipedia.net/starcraft2/Automatic_Targeting): Legacy of the Void scan range is weapon range + 0.5 |
| Cooldown (period) | 0.8608 s | **0.6149 s** | **15.37** | `liberty` WeaponData:203 (`Period`); Liquipedia `attack1_cooldown=0.61 (-0.203)` |
| Damage point | 0.05 s | 0.0357 s | **0.89** | `liberty` WeaponData:203 (`DamagePoint`); [Liquipedia, Damage Point](https://liquipedia.net/starcraft2/Damage_Point) gives 0.0357 |
| Backswing | 0.75 s | 0.5357 s | **13.39** | `liberty` WeaponData:203 (`Backswing`) *(data only; Liquipedia publishes no backswing)* |
| Random delay before firing | −0.0625 to +0.125 s (default) | −0.0446 to +0.0893 s | −1.12 to +2.23 | `core` WeaponData:35–36 (`CWeaponLegacy default`); meaning per [SC2Mapster wiki](https://sc2mapster.wiki.gg/wiki/Data/Weapons/Legacy). Whether it lengthens the period on average is **verify** |
| Arc slop | 11.25° (default) | — | — | `core` WeaponData:15; the weapon `Arc` is set in no layer, so the engine default is **not found** |
| DPS | — | 9.76 (6 ÷ 0.6149) | — | Derived; Liquipedia `dps=9.8 (+1.6)` |

### 1.5 Zealot

Patch 5.0.14.94342 unless stated. The data gives the zealot no `Radius`, so the core default of 0.5 applies. Its
inner and separation radii are 0.375.

| Field | Editor (Normal) | Faster | Our ticks (×25) | Source |
|---|---|---|---|---|
| Life | 100 | — | — | `liberty` UnitData:3166; Liquipedia `hp=100` |
| Shields | 50 | — | — | `liberty` UnitData:3166 (`ShieldsMax`); Liquipedia `shield=50` |
| Life armour | 1 | — | — | `liberty` UnitData:3166 (`LifeArmor`); Liquipedia `armor=1 (+1)` |
| Shield armour | 0 (not set) | — | — | `liberty` UnitData:3166 sets none; [Liquipedia, Plasma Shield](https://liquipedia.net/starcraft2/Plasma_Shield): shield armour "starts at 0" |
| Attributes | **Light, Biological** (not Psionic) | — | — | `liberty` UnitData:3166 (`Attributes`); Liquipedia `attributes=Light, Biological`. The catalog tags `psi_grey` Light and Psionic. Neither tag changes damage in battle 33, because the marine weapon has no attribute bonus |
| Movement speed | 2.25 | **3.15** | **0.126 per tick** | `liberty` UnitData:3166; Liquipedia `speed=3.15` |
| Acceleration | 1000 (instant) | — | — | `liberty` UnitData:3166 |
| Turn rate | 999.8437 °/s | 1399.8 °/s | instant | `liberty` UnitData:3166 |
| Radius | 0.5 (core default) | — | — | `core` UnitData:41 (`CUnit default`, `Radius`); the zealot element sets no `Radius` |
| Separation radius | 0.375 | — | — | `liberty` UnitData:3166 |
| Inner radius | 0.375 | — | — | `liberty` UnitData:3166 |
| Push priority | 10 (default) | — | — | `core` UnitData:59 |
| Sight | 9 | — | — | `liberty` UnitData:3166; Liquipedia `sight=9` |
| Weapon | Psi Blades (`PsiBlades`), option Melee | — | — | `liberty` WeaponData:150 |
| Range | **0.1** | — | — | `liberty` WeaponData:150; Liquipedia `range=0.1` and [Liquipedia, Range](https://liquipedia.net/starcraft2/Range) §Melee Range |
| How melee range is measured | From the attacker's edge: the editor adds the attacker's radius to the weapon range unless "Use Range To Center" is set, and neither weapon sets it | — | — | [SC2Mapster wiki, Legacy weapons](https://sc2mapster.wiki.gg/wiki/Data/Weapons/Legacy) ("The units radius field won't be added to the weapon range"); `liberty` WeaponData:150 and :203 set no such option. Whether the **target's** radius is also added (true edge-to-edge) is **verify** *(single secondary source; the wiki names only the attacker's radius)* |
| Melee firing condition | Can fire only if the attacker can path directly to the target | — | — | [SC2Mapster wiki, Legacy weapons](https://sc2mapster.wiki.gg/wiki/Data/Weapons/Legacy), "Melee (Option)" |
| Cooldown (period) | 1.2 s | **0.8571 s** | **21.43** | `liberty` WeaponData:150 (`Period`); Liquipedia `attack1_cooldown=0.86` and [Liquipedia, Cooldown](https://liquipedia.net/starcraft2/Cooldown) |
| Damage point | **0** | 0 | **0** | `liberty` WeaponData:150 (`DamagePoint value="0"`); [Liquipedia, Damage Point](https://liquipedia.net/starcraft2/Damage_Point) gives 0 |
| Backswing | 0.5 s (default) | 0.3571 s | **8.93** | `core` WeaponData:18 (the zealot sets none) *(data only)* |
| Random delay before firing | −0.0625 to +0.125 s (default) | as marine | as marine | `core` WeaponData:35–36 |
| Strikes per attack | 2 × 8 damage, as a persistent effect with periods at **0 and 0.28 s** | second strike **0.2 s** after the first | **5 ticks apart** | `liberty` EffectData:1919 (`PsiBladesBurst`: `PeriodCount 2`, `PeriodicPeriodArray 0, 0.28`, flag `Channeled`, `TimeScaleSource Caster`) and :1929 (`PsiBlades`, `Amount 8`) |
| Condition on each strike | The target must be **within 2.1** of the zealot, a ground unit, and not in stasis, hidden or invulnerable | — | — | `voidmulti` EffectData:361 adds validators `MultipleHitGroundOnlyAttackTargetFilter` and `ZealotAttackDamageMaxRange`. `voidmulti` ValidatorData:867 defines the second (`Range 2.1`, `Compare LE`) and :855 the first |
| Armour per strike | Armour applies to each of the two strikes | — | — | [Liquipedia, Armor](https://liquipedia.net/starcraft2/Armor) |
| Shield regeneration | 2 per s | **2.8 per s** | **0.112 per tick** | `liberty` UnitData:3166 (`ShieldRegenRate 2`); [Liquipedia, Plasma Shield](https://liquipedia.net/starcraft2/Plasma_Shield): "2 points per game second" |
| Shield regeneration delay | 10 s | **7.14 s** | **178.6** | `liberty` UnitData:3166 (`ShieldRegenDelay 10`); Plasma Shield page: "after 10 game seconds … just after 7 real time seconds" |
| Minimum scan range | 5 (default) | — | — | `core` WeaponData:11; Automatic Targeting page: "remaining at 5 for low-range units" |
| DPS against 0 armour | — | 18.67 (16 ÷ 0.8571) | — | Derived; Liquipedia `dps=18.6 (+2.33)` |

### 1.6 Stimpack, Combat Shield and Charge

None of these is researched in battle 33. Battle 25b uses Combat Shield and Stimpack.

| Item | Editor (Normal) | Faster | Our ticks | Source |
|---|---|---|---|---|
| Stimpack: life cost (marine) | 10 | — | — | `liberty` AbilData:1229 (`Cost/Vital Life 10`); [Liquipedia, Stimpack](https://liquipedia.net/starcraft2/Stimpack) |
| Stimpack: duration | 15 s | 10.71 s | 267.9 | `liberty` BehaviorData:680 (`Duration 15`); Liquipedia says "11 seconds" (rounded) |
| Stimpack: movement | ×1.5 | 4.725 | 0.189 per tick | `liberty` BehaviorData:680 (`MoveSpeedMultiplier 1.5`); Liquipedia marine `speed=3.15 (+1.57)` |
| Stimpack: attack speed | ×1.5 (cooldown ÷ 1.5) | cooldown 0.41 s | 10.25 | `liberty` BehaviorData:680 (`AttackSpeedMultiplier 1.5`); Liquipedia Cooldown table "0.61 (0.41)" |
| Combat Shield (`ShieldWall`) | +10 maximum and starting life (45 → 55) | — | — | `liberty` UpgradeData:1940; [Liquipedia, Combat Shield](https://liquipedia.net/starcraft2/Combat_Shield) |
| Charge upgrade: base speed | +1.125 (2.25 → 3.375) | 3.15 → **4.725** | 0.189 per tick | `balancemulti` UpgradeData:67 (`Unit,Zealot,Speed 1.125`); 4.11.0 patch notes: "increases Zealot movement speed from 3.15 to 4.72, up from 4.13" |
| Charge ability: autocast | on by default; not while holding position | — | — | `liberty` AbilData:3825 (`AutoCast`, `AutoCastOn`, validator `CasterNotHoldingPosition`); Liquipedia Zealot: "set to Autocast by default" |
| Charge ability: trigger window | attack-order target farther than 0.6 and within pathing distance 4 | — | — | `liberty` ValidatorData:183 (`ChargeMinTriggerDistance`, `GT 0.6`) and :1697 (`ChargeMaxDistance`, `LT 4`, `Pathing 1`); [Liquipedia, Charge](https://liquipedia.net/starcraft2/Charge) gives range 4 |
| Charge ability: speed while charging | ×2.2 | 4.725 × 2.2 = **10.40** (derived) | 0.416 per tick | `liberty` BehaviorData:967 (`Charging`, `MoveSpeedMultiplier 2.2`); Liquipedia says 9.1 (§1.11) |
| Charge ability: duration | 3.5 s, ends when the zealot's weapon fires | 2.5 s | 62.5 | `liberty` BehaviorData:967 (`Duration 3.5`, remove validator `ChargeTargetAttacked`); Liquipedia `duration=2.5` |
| Charge ability: cooldown | 10 s | **7.14 s** | 178.6 | `liberty` AbilData:3825 (`Cooldown TimeUse 10`); Liquipedia Cooldown table 7.14; Charge infobox `cooldown=7` |
| Charge: damage on impact | 8, but only if the player has `HaveSunderingImpact` | — | — | `balancemulti` EffectData:1127 (`ChargingDamage`) and ValidatorData `ZealotSunderingImpactUpgraded`. [4.11.0 patch notes](https://news.blizzard.com/en-us/starcraft2/23227811/starcraft-ii-4-11-0-patch-notes): "The Charge upgrade no longer provides Zealots with +8 damage on impact". Ladder 5.0.14 therefore has no impact damage |
| Charge history | 3.8.0: speed 3.15 → 4.13. 4.11.0: impact damage removed, speed 4.13 → 4.72 | — | — | [Liquipedia, Zealot](https://liquipedia.net/starcraft2/Zealot_(Legacy_of_the_Void)), patch changes; 4.11.0 patch notes. [5.0.14 patch notes](https://news.blizzard.com/en-us/article/24162754/starcraft-ii-5-0-14-patch-notes) change neither unit (only a Changeling zealot animation fix) |

### 1.7 Upgrades

Battle 33 has no upgrades: level 0 weapons and armour, and no Stimpack, Combat Shield or Charge
([plan](../plans/2026-11-04-zealot-marine-investigation.md) §2.1).

| Upgrade | Per level | Source |
|---|---|---|
| Terran Infantry Weapons (marine) | +1 damage | Liquipedia Marine `6 (+1)`; `liberty` UpgradeData:1086 (`TerranInfantryWeaponsLevel1`) |
| Terran Infantry Armor (marine) | +1 armour | Liquipedia Marine `armor=0 (+1)`; `liberty` UpgradeData:1122 |
| Protoss Ground Weapons (zealot) | +1 damage on each of the two strikes | Liquipedia Zealot `8 (+1) (x2)`; `liberty` UpgradeData:1338 |
| Protoss Ground Armor (zealot) | +1 life armour | Liquipedia Zealot `armor=1 (+1)`; `liberty` UpgradeData:1293 |
| Protoss Shields | +1 shield armour | [Liquipedia, Plasma Shield](https://liquipedia.net/starcraft2/Plasma_Shield) |

### 1.8 Mechanics that shape the fight

| Mechanic | What StarCraft II does | Source and weight |
|---|---|---|
| Target choice under attack-move | Candidates within scan range are ranked: (1) threats (can shoot back), (2) highest attack target priority (20 for both units), (3) can use the primary weapon, (4) closest — but step 4 applies **only if the unit lost its previous target**. A target is kept until it dies or becomes invalid, leaves weapon range, or a higher-priority target enters scan range | [Liquipedia, Automatic Targeting](https://liquipedia.net/starcraft2/Automatic_Targeting) |
| Scan range | Marine 5.5; zealot 5 | §1.4 and §1.5 |
| Overkill and smart fire | The targeting rules contain no step that avoids targets already receiving lethal damage, so marines have no smart fire. Instant-damage units that fire in the same game loop all fire, even if fewer shots would kill (tank example) | Automatic Targeting page (absence of a rule; derived). [TL blog "Overkill is anti deathball"](https://tl.net/blogs/371077-overkill-is-anti-deathball), 2012 *(single secondary source)*. Whether marine shots in one game loop are wasted on a target killed earlier in that loop is **not found** |
| Zealot's second strike on a dead target | If the first strike kills, the second is wasted: no new target is chosen inside the burst | Derived from `liberty` EffectData:1919 (`WhichLocation TargetUnit`) |
| Cancelled swings | The zealot has no wind-up (damage point 0), so its first strike lands the moment it attacks and cannot be cancelled. The second strike lands 0.2 s later if the target is within 2.1 of the zealot, which is much more than melee range. The marine's wind-up is 0.0357 s (under one of our ticks). Damage point is "the time between when an attacking unit attacks, and when it can receive new commands" | §1.5; [Liquipedia, Damage Point](https://liquipedia.net/starcraft2/Damage_Point); [JaKaTaK, "The Effects of Damage Point on Unit Interaction"](https://tl.net/forum/starcraft-2/481523-the-effects-of-damage-point-on-unit-interaction), TL.net, 2015. Legacy weapons default to `UninterruptibleDuration 32767.9997` (`core` WeaponData:38), so a started attack is not interrupted. Whether a melee attack started on a target that then moves away spends the cooldown is **not found** |
| Melee surround | Melee units attack-moved at the enemy front "get in one another's way". A surround needs them ordered through the enemy first. Zealots trapped in their own clump is a recognised problem. Clumped bio "deal[s] with Zealots more easily" | [Liquipedia, Micro](https://liquipedia.net/starcraft2/Micro_(StarCraft)); [TL "Unit Clumping in SC2"](https://tl.net/forum/starcraft-2/298813-unit-clumping-in-sc2-good-or-bad?page=4), 2011; [Liquipedia, Bio (vs. Protoss)](https://liquipedia.net/starcraft2/Bio_(vs._Protoss)) |
| How many zealots fit around one marine | Derived, not published. The ring of attack positions has radius d = 0.375 (marine) + 0.5 (zealot) + 0.1 (range) = 0.975, assuming the target's radius also counts (see §1.5, **verify**). The most zealots on it is ⌊π ÷ asin(s ÷ 2d)⌋, where s is the closest centre spacing two zealots allow: 5 if zealots keep their 0.5 radii apart (s = 1.0), 7 if they pack to the 0.375 inner or separation radius (s = 0.75). Which radius governs unit-to-unit spacing is **not found** | Derived from §1.4 and §1.5 |
| Enemies pushing each other | Push priority is 10 for both units. Community sources conflict on whether enemies can push each other at all (§1.11). No authoritative statement **found** | `core` UnitData:59; [SC2Mapster thread 187903](https://www.sc2mapster.com/forums/development/data/187903-unit-bumps-into-oncoming-unit-making-it-stop) (now unreachable, quoted in search results only) |
| Armour floor | Each hit deals at least 0.5 | [Liquipedia, Armor](https://liquipedia.net/starcraft2/Armor) |
| Shield-to-life spillover | Shields have their own armour, applied first. Damage that breaks the shields reaching life minus life armour is common knowledge but **not found** stated in a source | [Liquipedia, Plasma Shield](https://liquipedia.net/starcraft2/Plasma_Shield) (separate armour only) |
| Group spread while moving | Units keep their separation radius apart. Both units here use 0.375 | §1.4, §1.5. No published description of the flocking rule was **found** |

### 1.9 Observed outcomes

No source matches battle 33 exactly. For reference, battle 33 has:
- 24 marines in a 4 × 6 block at spacing 0.8, against 12 zealots in a 3 × 4 block at spacing 1.2;
- the two fronts about 22.6 apart, both sides attack-moving;
- no upgrades and no micro, on a flat open map.

| # | Source | Game, date | Setup | Result | Weight |
|---|---|---|---|---|---|
| O1 | ["Marine strength tests. Are they really OP?"](https://us.battle.net/forums/en/sc2/topic/1965994848), Battle.net forums, read from the [archived copy of 2019-08-21](https://web.archive.org/web/20190821141738/https://us.battle.net/forums/en/sc2/topic/1965994848) (posts dated 2011-01-31) | Wings of Liberty (marine and zealot base stats as in 5.0.14; §1.4) | **50 marines against 25 zealots (equal supply), no upgrades, no micro.** "Positioning was left up to the AI, they were left in clumps". Open area, "perfect for letting zerglings/zealots get a nice surround". "Tested multiple times"; the number of runs and the start distance are not stated | **Marines win, 30 marines lost (20 of 50 alive, 40 %)** | Highest of the set: same supply ratio, no upgrades, no micro, clumped. Still one anecdotal tester, Wings of Liberty engine, about twice battle 33's size |
| O2 | Same thread | Wings of Liberty | 1 zealot against 2 marines, no upgrades: (a) no micro; (b) "basic micro". Also 1 zealot against 4 marines | (a) zealot wins; (b) marines win, 1 marine lost; 4 marines win, 1 lost | Medium, small count (compare battle 25a and the benchmark battles) |
| O3 | ["Zealots vs Marines test"](https://us.forums.blizzard.com/en/sc2/t/zealots-vs-marines-test/9265), Blizzard SC2 forums, 2020-03-15 | Legacy of the Void (around 4.11) | **Charge researched; one Stimpack used**, "when more or less all zealots were within gun range". "Units were in small blobs facing each other and no other micro" | 5 Z against 10 M: zealots win with 2–3 left. 10 against 20: zealots win. 15 against 30: zealots win "but barely". **30 against 60: marines win with 20–30 alive** | Medium: current engine, but Charge and Stim both differ from battle 33. Shows the crossover with size |
| O4 | ["Which basic unit is best/easiest"](https://staredit.net/235735/), StarEdit Network, 2010-06-11 | Wings of Liberty beta | 6 marines against 3 zealots, 10 trials each, upgrades not stated: (a) "straight-forward battle, no micro"; (b) hit-and-run micro; (c) bottlenecks | (a) zealots won 9 of 10; (b) marines won 9 of 10; (c) marines won 7 of 10 | Low-medium: beta, small count, upgrades unstated |
| O5 | ["Zealots are op as all heck"](https://us.forums.blizzard.com/en/sc2/t/zealots-are-op-as-all-heck/7401), Blizzard SC2 forums, 2020-01-13 (Ody) | Legacy of the Void | Unit tester, 0/0. 16 marines, 8 marauders and 4 medivacs with Stim, Combat Shield and Concussive Shells, against 27 zealots with Charge. Open field, with variants | Zealots win in every variant (17, 13 or 24 left) | Low for battle 33: mixed composition, Charge |
| O6 | ["Marines versus zealots"](https://www.starcraftforum.com/threads/marines-versus-zealots.59123/), StarcraftForum, 2011-04-13 | StarCraft II | Claim, not a test: "20 marines will quite easily defeat 20 zealots" (unequal supply) | Marines | Low: untested, unequal supply |
| O7 | Videos: ["400 Zealots vs 400 Marines!"](https://www.youtube.com/watch?v=jpJWyUXChBE) (BATTLEX!, 2025-02-18, LotV unit tester); ["Marines vs Zealots vs Zerglings"](https://www.youtube.com/shorts/yJpoUQC3wZQ) (noobSC, 2026-09-30, 2 marines against 1 zealot); ["Zealot vs. Marine - Armor or Attack Upgrades?"](https://www.youtube.com/watch?v=Qd7xQAP7yNI) (D.Shlango, 2024-12-20, +2 upgrades); ["Marine vs Zealot"](https://www.youtube.com/watch?v=Eg8zvgK7Exk) (2016) and ["STARCRAFT 2 MARINE VS ZEALOT"](https://www.youtube.com/watch?v=bCfAZSGsebY) (2015) | Various | Not stated in text | Results appear only in the video and were **not extracted** | Not used |

**What the outcomes say.** Zealots win at small counts (O2a, O4a) and with Charge (O3 up to 15 against 30, O5).
Marines win when either side has micro (O2b, O4b, O4c) and at large equal-supply counts (O1, O3 at 30 against 60).
The crossover lies between 15 against 30 and 30 against 60, and Charge pushes it towards larger counts. Battle 33,
at 12 against 24 with no Charge, is near the crossover. The no-Charge test closest to it (O1) is a clear marine win
with 40 % alive.

### 1.10 The "zealots should win" claim and its assumptions

| Claim | Where | What it assumes |
|---|---|---|
| "Without kiting, Zealots close in and defeat Marines in a close, brutal fight" (suite 33) | [Plan 21a](../plans/21a-plan-comprehensive-sc2-who-wins-matchup-benchmarks.md), suite 33 | No source, test or setup given. This is the expectation the battle was first written against |
| "Before Stimpack, Marines are less than effective against Zerglings and Zealots; at least one Bunker and Supply Depots to wall-off is recommended for the early game" | [Liquipedia, Marine](https://liquipedia.net/starcraft2/Marine_(Legacy_of_the_Void)), Description | Early game: few marines, no Stim. Counts not stated |
| "Marines will not last long in even numbers against Zealots and Stalkers; but will increase in effectiveness as numbers increase quickly" | [Liquipedia, Bio (vs. Protoss)](https://liquipedia.net/starcraft2/Bio_(vs._Protoss)) | "Even numbers" (counts, or supply?) without a buffer such as Bunkers or Marauders. It says marines **gain** as numbers grow, which agrees with O1 and O3 |
| "In small numbers, they can be kited by ranged units … as Charge is researched … the movement boost brings the Zealot's health and damage to bear" | [Liquipedia, Zealot](https://liquipedia.net/starcraft2/Zealot_(Legacy_of_the_Void)) | Charge is what makes zealots strong against ranged units. Without it, kiting decides |
| Zealots win at 5 against 10 up to 15 against 30 | O3 | Charge, plus one Stim for the marines |
| 1 zealot beats 2 marines; 3 zealots beat 6 marines | O2a, O4a | Small counts, no micro |
| Zealots are highly cost-effective "only if you can utilize them. Can they surround?" | [TL "Starcraft 2 units: a cost-effectiveness analysis"](https://tl.net/forum/starcraft-2/138141-starcraft-2-units-a-cost-effectiveness-analysis?view=all), 2010 | A surround, and Charge ("legs") |

**Summary of the assumptions.** The claims hold under one or more of four conditions:
- small numbers, where nearly every zealot reaches a marine at once;
- Charge;
- marines without a buffer;
- a zealot surround.

None of them assumes zealot or marine weapon or armour upgrades. None states battle 33's setup: 12 against 24, no
Charge, attack-move from blocks, clumped marines. The tests closest to that setup favour marines (§1.9).

### 1.11 Conflicts between sources

| Item | Values | Which is used and why |
|---|---|---|
| Speed during Charge | Liquipedia Charge page: 9.1. Data: 4.725 × 2.2 = 10.40 | **10.40**. 9.1 = 4.13 × 2.2, so Liquipedia still uses the base speed from before 4.11.0 |
| Charge cooldown | Liquipedia Charge infobox: 7. Liquipedia Cooldown table: 7.14. Data: 10 Normal = 7.14 Faster | **7.14**; the infobox rounds |
| Stimpack duration | Liquipedia: 11 s. Data: 15 Normal = 10.71 Faster | **10.71**; Liquipedia rounds |
| Charge speed after 4.11 | Patch notes: 4.72. Liquipedia Zealot: 4.725. Data: 3.375 Normal = 4.725 Faster | **4.725**; the notes round |
| Zealot attributes | Catalog: Light, Psionic. Data and Liquipedia: Light, Biological | **Light, Biological** (reference). The catalog's choice is for WP2 to explain; it does not change battle 33 |
| Can enemies push each other? | SC2Mapster replies, as quoted in search results: "opposing players can not push each others' units at all" against "if the push priority of one unit is lower than the other, it gets pushed, even if it is an enemy". The plan's §2.3 assumes they cannot | Unresolved. **Not found** in an authoritative source; WP4 measures our rule by trace instead |
| Do numbers favour zealots or marines? | O2a and O4a (small counts) against O1 and O3 at 30 against 60 (large counts); Liquipedia Bio (vs. Protoss) says marines gain with numbers | Not a contradiction: the sources describe different sizes. Battle 33 sits between them |
| Weapon range measured from edges or centres | SC2Mapster wiki: the attacker's radius is added. A forum claim (search results only) says range runs from the unit's "origin" | Attacker's edge (editor field semantics). Whether the target's radius counts is **verify** |

### 1.12 Not found within the effort bound

The plan allows "not found" with the best secondary value when an item cannot be sourced in reasonable time. These
items were not found:

- Marine deceleration. The data sets none. Liquipedia treats ground-unit movement changes as instant (§1.4).
- The engine's default weapon arc. Turn rates are effectively instant, so it barely matters.
- An authoritative statement on four mechanics:
  - whether enemies can push each other;
  - which radius governs unit-to-unit spacing;
  - whether the target's radius counts towards range;
  - whether random delay lengthens the average period.
- Whether a melee attack started on a target that moves away spends its cooldown. The zealot's damage point is 0,
  so this barely arises.
- Whether marine shots in one game loop are wasted on a target already killed in that loop.
- A test of exactly 24 marines against 12 zealots, with or without upgrades.

### 1.13 Differences already visible, for WP2

These are pointers, not conclusions. WP2 makes the full comparison, with the time base of every value. The "ours"
values come from `assets/catalog/units.ron` and the plan's §2.2.

| Field | Ours | Reference (Faster, 25 ticks per s) | Note |
|---|---|---|---|
| Marine speed | 0.09 per tick (2.25 per s) | 0.126 per tick (3.15 per s) | 2.25 is the **Normal**-time editor value, unconverted |
| Zealot shield regeneration | 0.08 per tick (2 per s) | 0.112 per tick (2.8 per s) after 178.6 ticks | 2 per s is the Normal-time editor value, unconverted; the delay is for WP2 to check |
| Zealot period | 22 ticks | 21.43 ticks | Doc 07 itself says 21 |
| Zealot damage point | 4 ticks | 0 | Ours adds a wind-up that can be cancelled; the reference has none |
| Zealot strikes | both in one tick | 5 ticks apart; the second needs the target within 2.1 | |
| Zealot range | 0.3 | 0.1 | Already noted in plan §2.4 |
| Marine damage point / backswing | 2 / 4 ticks | 0.89 / 13.39 ticks | Backswing matters only for orders given during an attack; check how `sim_core` uses it |
| Zealot backswing | 6 ticks | 8.93 ticks | |
| Separation radius (marine / zealot) | 0.45 / 0.6 | 0.375 / 0.375 | Ours is larger than the collision radius; the reference's is equal to or smaller than it |
| Push priority (marine / zealot) | 20 / 25 | 10 / 10 | Friendly pushing only |
| Scan range (both) | 9 | 5.5 (marine) / 5 (zealot) | Ours acquires from much farther away |

### 1.14 Sources

Game data (primary):

- SC2Mapster, SC2GameData, commit `5444e12f13` ("SC2 5.0.14.94342"):
  <https://github.com/SC2Mapster/SC2GameData/tree/5444e12f13>. Files: `mods/{core,liberty,voidmulti,balancemulti}.sc2mod/base.sc2data/GameData/{UnitData,WeaponData,EffectData,AbilData,BehaviorData,UpgradeData,ValidatorData}.xml`.
- Blizzard, s2client-proto, `docs/protocol.md`: <https://github.com/Blizzard/s2client-proto/blob/master/docs/protocol.md>.

Patch notes:

- StarCraft II 4.11.0 patch notes: <https://news.blizzard.com/en-us/starcraft2/23227811/starcraft-ii-4-11-0-patch-notes>.
- StarCraft II 5.0.14 patch notes: <https://news.blizzard.com/en-us/article/24162754/starcraft-ii-5-0-14-patch-notes>.

Liquipedia (wikitext read 2026-11-03):

- Units: [Marine](https://liquipedia.net/starcraft2/Marine_(Legacy_of_the_Void)),
  [Zealot](https://liquipedia.net/starcraft2/Zealot_(Legacy_of_the_Void)).
- Abilities and upgrades: [Charge](https://liquipedia.net/starcraft2/Charge),
  [Stimpack](https://liquipedia.net/starcraft2/Stimpack), [Combat Shield](https://liquipedia.net/starcraft2/Combat_Shield).
- Timing and stats: [Game Speed](https://liquipedia.net/starcraft2/Game_Speed),
  [Damage Point](https://liquipedia.net/starcraft2/Damage_Point), [Cooldown](https://liquipedia.net/starcraft2/Cooldown),
  [Range](https://liquipedia.net/starcraft2/Range), [Speed](https://liquipedia.net/starcraft2/Speed).
- Combat rules: [Plasma Shield](https://liquipedia.net/starcraft2/Plasma_Shield),
  [Armor](https://liquipedia.net/starcraft2/Armor),
  [Automatic Targeting](https://liquipedia.net/starcraft2/Automatic_Targeting).
- Play: [Micro](https://liquipedia.net/starcraft2/Micro_(StarCraft)),
  [Bio (vs. Protoss)](https://liquipedia.net/starcraft2/Bio_(vs._Protoss)).

Editor field semantics:

- SC2Mapster wiki, Legacy weapons: <https://sc2mapster.wiki.gg/wiki/Data/Weapons/Legacy>.

Community tests and discussion:

- The forum, video and TL.net sources are linked in the rows of §1.8 to §1.11, each with its date.

## 2. Comparison: our data and rules against the reference (WP2)

Written 2026-11-03 against `main` at `eef92c3`. "Ours" is read from `assets/catalog/units.ron` (`pulse_bot`, `psi_grey`)
and from `crates/sim_core/src`; the reference is §1.

### 2.1 Summary

1. **Two values are unconverted Normal-time numbers.** The marine's speed (0.09 per tick) and the zealot's shield
   regeneration (0.08 per tick) are the editor's Normal values put straight into our Faster-based ticks. Both should be
   28.6 % higher. The zealot's speed has the same editor value (2.25) and **was** converted (0.126), and doc 07's own
   roster table gives the marine 0.1260. The shield-regeneration delay, hard-coded as 250 ticks in `sim_core`, is the
   same mistake (10 Normal s; the reference is 178.6 ticks).
2. **None of the data differences decides battle 33 in the model.** Together they move the contact fraction the
   zealots need from 0.577 (reference values) to 0.607 (ours), about 0.4 of a zealot (§3.3). The ones that favour the
   marines are the zealot period (+2.7 %), the marine period (−2.4 %) and the zealot's 4-tick wind-up, which can be
   cancelled. The ones that favour the zealots are melee range 0.3 (7 zealots fit around a marine instead of 5) and
   both strikes in one tick.
3. **Four flagged differences have no effect in battle 33.** Separation radius is read by no system in `sim_core`.
   Push priority applies between friends only, and each side has one unit type. Backswing neither stops movement nor
   delays the next attack. Attributes do not matter, because the marine's weapon has no attribute bonus.
4. **What does decide the battle is how many zealots fight at once (§3).** The data puts battle 33 close to the
   crossover. The survivors we see (6 to 13 of 24) fit 4 to 6 zealots in contact at a time, which is what the
   marine block's front face holds. Depending on how pre-contact damage is counted, 5 to 8 zealots in contact
   throughout would give the zealots the win.

### 2.2 Time base

`sim_types::TICKS_PER_SECOND` is 25 (`crates/sim_types/src/lib.rs`), and doc 07 §2 defines a tick as 1/25 of a
Faster second. Every reference time below is converted explicitly:

- Faster seconds = editor (Normal) seconds ÷ 1.4.
- Our ticks = Faster seconds × 25.
- Speeds and rates per second are multiplied by 1.4, then divided by 25 for "per tick".

The catalog's per-tick fields and tick counts are converted back the same way for the "ours" column, so 0.09 per tick
reads as 2.25 per Faster second. Values with no time base (life, damage, radii, priorities) are compared directly.

### 2.3 Marine (`pulse_bot`)

"Favours" says which side the difference helps **in battle 33**, compared with the reference value. "Documented"
says whether a project document chose the value.

| Field | Ours (catalog or rule) | Reference, editor (Normal) | Reference in our ticks | Delta (ours − ref) | Documented? | Favours in battle 33 |
|---|---|---|---|---|---|---|
| Life / armour | 45 / 0 | 45 / 0 | — | 0 | — | neither |
| Attributes | Light, Biological (`bits: 5`) | Light, Biological | — | none | — | neither |
| Movement speed | **0.09 per tick = 2.25 per Faster s** | 2.25 per Normal s | **0.126 per tick** (3.15 per Faster s) | **−0.036 per tick (−28.6 %)** | No. In the first commit `3d9a9c4`; doc 07's roster table says 0.1260 | Almost neither: no kiting in battle 33. It lengthens the walk to engagement (77.4 against 66.4 ticks, §3.2) and how fast rear marines step into range (3.7 against 3.2 ticks per 0.8 of depth). Pre-contact shots are the same (66). Zealots slightly, if at all |
| Acceleration / deceleration | 0.09 per tick (parsed, unused: speed changes are instant, plan §2.3) | 1000 (instant) | instant | none in effect | — | neither |
| Turn rate | 999 ° per tick, clamped to 180 ° (instant) | 999.84 ° per Normal s | 56.0 ° per tick (1399.8 ° per Faster s); effectively instant | both instant | — | neither |
| Collision radius | 0.375 | 0.375 | — | 0 | — | neither |
| Separation radius | 0.45 | 0.375 | — | +0.075 | No | **neither: no `sim_core` system reads `separation_radius`.** Movement and contact use `collision_radius` (`movement/mod.rs`, `MotionPolicy::resolve`) |
| Push priority | 20 | 10 | — | +10 | Doc 07 roster | **neither:** priority only orders friendly contact (`steering.rs`, `checked_dynamic_push`), and all 24 blue units share it |
| Sight | 9 | 9 | — | 0 | — | neither |
| Scan (acquisition) range | **9**, centre to centre (`target_acquisition.rs`) | 5.5 | — | **+3.5** | No | Unclear. Marines turn from the attack-move line to the closest zealot at 9 instead of 5.5. Both sides are attack-moving at each other, so both acquire well before contact anyway. WP4 can measure it |
| Damage | 6 | 6 | — | 0 | — | neither |
| Range | 5, from edge to edge (both radii: `combat.rs`, `in_engagement_band`) | 5, from the attacker's edge; the target's radius is **verify** (§1.5) | — | 0 if the target's radius counts | — | neither (engagement at 5.875 between centres either way) |
| Period (cooldown) | **15 ticks** = 0.600 Faster s | 0.8608 s | **15.37** (0.6149 Faster s) | −0.37 ticks (−2.4 %) | Doc 07 §2 rounds 0.61 × 25 = 15 | **marines:** DPS 10.0 against 9.76 per Faster s (+2.5 %) |
| Damage point | 2 ticks | 0.05 s | 0.89 | +1.1 ticks | Plan 17 §2.2 (0.08 s) | neither: the first shot is about 1 tick later, and approaching zealots never leave the band during it |
| Backswing | 4 ticks | 0.75 s | 13.39 | −9.39 ticks | No | **neither.** In `sim_core` the backswing does not hold the unit in place (only the wind-up does: `locomotion.rs`). It ends before the cooldown (13 ticks) does, so it never delays the next shot. It matters only for stutter-step orders, which battle 33 does not give |
| Random delay before firing | none (deterministic) | −0.0625 to +0.125 s | −1.12 to +2.23 ticks | not modelled | — | marines slightly, **if** the delay lengthens the mean period (§1.4, verify): +0.56 ticks on average would make the reference period 15.93 |
| Release slop | 0.5 at the end of the wind-up only (`RELEASE_RANGE_SLOP`) | `RangeSlop` 1 | — | −0.5 | Comment in `combat.rs` calls it "the same half-cell `RangeSlop`" | neither for the marine (its targets walk towards it) |

### 2.4 Zealot (`psi_grey`)

| Field | Ours (catalog or rule) | Reference, editor (Normal) | Reference in our ticks | Delta (ours − ref) | Documented? | Favours in battle 33 |
|---|---|---|---|---|---|---|
| Life / shields / life armour / shield armour | 100 / 50 / 1 / 0 | 100 / 50 / 1 / 0 | — | 0 | — | neither |
| Attributes | Light, **Psionic** (`bits: 17`) | Light, **Biological** | — | Psionic instead of Biological | No document chooses it; it matches the unit's lore text ("psionic exo-carapace") | **neither:** the marine's weapon has no attribute bonus, and no rule in battle 33 reads either flag |
| Movement speed | 0.126 per tick = 3.15 per Faster s | 2.25 per Normal s | 0.126 per tick | 0 | Doc 07 roster | neither (converted correctly, unlike the marine's identical editor value) |
| Acceleration / turn rate | 0.126 per tick (unused) / 999 ° per tick (instant) | 1000 / 999.84 ° per Normal s | instant | none in effect | — | neither |
| Collision radius | 0.5 | 0.5 (core default) | — | 0 | — | neither |
| Separation radius | 0.6 | 0.375 | — | +0.225 | No | **neither: not read by `sim_core`** (as for the marine) |
| Push priority | 25 | 10 | — | +15 | Doc 07 roster | **neither:** friendly contact only, and all 12 red units share it |
| Sight | 9 | 9 | — | 0 | — | neither |
| Scan range | **9**, centre to centre | 5 | — | **+4** | No | Unclear: rear zealots pick a marine from farther away. WP4 |
| Weapon range | **0.3**, edge to edge: in band at 1.175 between centres, released up to 1.675 | **0.1** | — | **+0.2** | Plan 17 §2.2 says 0.3; doc 07's roster says 0.2 | **zealots:** up to 7 zealots fit on the ring around one marine instead of 5, and about 6 instead of 5 along the marine block's front (§3.4). The walk under fire is 1.6 ticks shorter, too little to cost a marine shot |
| Period (cooldown) | **22 ticks** = 0.88 Faster s | 1.2 s | **21.43** (0.8571 Faster s) | +0.57 ticks (+2.7 %) | Plan 17 §2.2 says 22 (0.88 s); doc 07 says 21 | **marines:** DPS 18.18 against 18.67 per Faster s (−2.6 %) |
| Damage point | **4 ticks** | **0** | **0** | +4 ticks | Plan 17 Bug 3 chose 4 as a "realistic SC2 frontswing"; the reference has none | **marines.** (a) The first strike lands 4 ticks after the zealot reaches its band, and it stands still during the wind-up (`locomotion.rs`). (b) A wind-up whose target dies or leaves the band (1.675 between centres) is cancelled with no damage and no cooldown (`combat.rs`, `step_windup`). With 24 marines focusing fire and other zealots killing the same marine, (b) can waste swings. WP3 counts them |
| Backswing | 6 ticks | 0.5 s (default) | 8.93 | −2.93 ticks | Plan 17 Bug 3 | **neither:** it ends before the cooldown (18 ticks) does, and it does not hold the unit |
| Strikes per attack | 2 × 8 **in the same tick** (`combat_resolution.rs`, `apply_weapon_hit` loops `attacks_count`) | 2 × 8, the second 0.28 s later, only if the target is within 2.1 | **5 ticks apart** | second strike 5 ticks earlier | No | **zealots, slightly:** each killing swing lands 5 ticks sooner. Both sides waste the second strike when the first kills |
| Shield regeneration | **0.08 per tick = 2 per Faster s** | 2 per Normal s | **0.112 per tick** (2.8 per Faster s) | **−0.032 per tick (−28.6 %)** | No. Doc 03 §1.1 itself says 2.8 per second | **neither** in battle 33 (see the delay below) |
| Shield regeneration delay | **250 ticks**, hard-coded in `sim_core` (`world/upkeep.rs`; module doc of `combat.rs`: "10-second (250-tick)") | 10 s | **178.6** (7.14 Faster s) | +71.4 ticks (+40 %) | Doc 03 §1.1 says "10 seconds" (the Normal value) | **neither** in battle 33: a zealot under fire is hit far more often than every 179 ticks, and unhit zealots are already full |
| Damage rules | shield armour first, spillover minus life armour, minimum 0.5 (`combat.rs`, `mitigate_strike`) | same (§1.8) | — | none | — | neither: 29 marine shots kill a zealot under both, and 6 strikes (3 swings) kill a marine |

### 2.5 Rules that are not catalog data

| Rule | Ours | Reference (§1.8) | Favours in battle 33 |
|---|---|---|---|
| Contact between enemies | Enemies separate evenly along the line of centres, whatever their push priority, unless one holds position (`steering.rs`, `checked_dynamic_push`, rule 3). Hostile correction is uncapped (`movement/collision.rs`) | Unresolved: sources disagree whether enemies can push each other at all | Unclear. A zealot walking into the marine block shoves the marine half the overlap back. That keeps the marine inside the zealot's band, but it reshapes the front. WP3 and WP4 |
| Wind-up cancellation | A wind-up is bound to its target and cancelled with no cooldown if the target dies or leaves the band | The zealot has no wind-up; the marine's (0.89 ticks) is shorter than one of ours | marines (through the zealot's 4 ticks, §2.4) |
| Effective period with a damage point of 0 | By reading `step_entity_weapon`: a weapon with `damage_point_ticks: 0` enters its wind-up on the tick the cooldown ends and releases on the next tick, so it fires every `period + 1` ticks. With a damage point of 1 or more it fires every `period` ticks | Every 21.43 ticks | none today (neither unit has a damage point of 0). **WP4 note:** the counterfactual "zealot damage point 0" must also set `period_ticks: 21` to fire every 22 ticks, or it changes two things at once. WP3's trace can confirm the reading |
| Overkill | Shots prepared and applied in entity-id order; a target killed earlier in the phase wastes the later shots | No smart fire; same-loop waste **not found** | unknown; WP3 counts wasted shots |
| Target choice | threat priority, then distance, then id; switching margin 1.0; call for help to idle allies within 5 | Keep the target until it dies or leaves range; closest only after losing it | unclear |

### 2.6 Found outside battle 33

Two more differences of the same kind, which battle 33 does not exercise:

- **Stimpack (`overclock`) duration:** 250 ticks (10 Faster s) against 15 Normal s = 10.71 Faster s = **267.9 ticks**.
  The speed and attack-rate multipliers (1.5) and the life cost (10) match. Battle 25b uses Stimpack.
- **Doc 07 disagrees with the catalog** on three values in battle 33: the marine's speed (0.1260 against 0.09), the
  zealot's range (0.2 against 0.3) and the zealot's period (21 against 22 ticks).

## 3. Analytic model (WP2)

A Lanchester square-law model with the number of zealots in contact as its parameter. It answers one question: how
many zealots must be fighting at once, on average, for the zealots to win battle 33? The script that produced the
numbers is a throwaway (`~/scratch/b33/zm_wp2_model.py`, not committed); every formula is given here, so the tables
can be checked by hand.

### 3.1 Per-pair numbers

| Quantity | Ours | Reference | How |
|---|---|---|---|
| Marine shots to kill a zealot | 29 | 29 | 8 shots take 48 shields; the 9th takes the last 2 and spills 6 × (1 − 2/6) − 1 = 3 into life; then 20 shots of 5 take the remaining 97 |
| Zealot strikes to kill a marine | 6 (3 swings) | 6 (3 swings) | ⌈45 ÷ 8⌉; the 6th strike overkills by 3 |
| One marine kills one zealot in (T_M) | 29 × 15 = **435 ticks** (17.4 Faster s) | 29 × 15.37 = **445.8 ticks** (17.8 s) | sustained fire |
| One zealot kills one marine in (T_Z) | 3 × 22 = **66 ticks** (2.64 s) | 3 × 21.43 = **64.3 ticks** (2.57 s) | sustained swings |
| First marine dead after the first zealot reaches its band | 4 + 2 × 22 = **48 ticks** | 0 + 2 × 21.43 + 5 = **47.9 ticks** | our 4-tick wind-up cancels out the reference's 5-tick strike gap |

The plan's back-of-envelope figures (§2.4) hold: about 17 marine-seconds per zealot and 2.5 to 2.6 zealot-seconds per
marine.

### 3.2 The approach

Battle 33 starts the fronts 22.6 apart (marine front at x = 22.4, zealot front at x = 45). The marines engage at
5 + 0.375 + 0.5 = 5.875 between centres; the zealots strike at 0.3 + 0.875 = 1.175 (ours) or 0.1 + 0.875 = 0.975
(reference, counting the target's radius).

| Phase | Ours | Reference |
|---|---|---|
| Walk until the marine front is in range: (22.6 − 5.875) ÷ (v_marine + v_zealot) | 16.725 ÷ 0.216 = **77.4 ticks** | 16.725 ÷ 0.252 = **66.4 ticks** |
| Zealots walk through marine fire: (5.875 − contact) ÷ 0.126 | 4.7 ÷ 0.126 = 37.3 ticks | 4.9 ÷ 0.126 = 38.9 ticks |
| Plus the zealot's wind-up | + 4 = **41.3 ticks of fire** before the first strike | + 0 = **38.9 ticks** |
| First zealot strike | tick ≈ 119 | tick ≈ 105 |
| Rear marine columns enter range later by 0.8 k ÷ (v_marine + v_zealot) | 3.7 ticks per column | 3.2 ticks per column |
| Marine shots before contact (first shot after the damage point, then every period) | 3, 3, 3 and 2 per marine by column: **66 shots** | 3, 3, 3, 2: **66 shots** |
| Zealot-equivalents of that fire (66 ÷ 29) | **2.28** | **2.28** |
| Contact time left in the 500-tick battle | 381 ticks | 395 ticks |

The marine's slower speed only delays the meeting; the fire window depends on the zealot's speed, which is the same.

### 3.3 Square law with a contact fraction

**Model.** With M marines and Z zealots alive, and a fraction f of the zealots in contact:

- dM/dt = −f Z ÷ T_Z
- dZ/dt = −M ÷ T_M

The zealots win when f Z₀² ÷ T_Z > M₀² ÷ T_M, so the **critical contact fraction** is
**f\* = (M₀ ÷ Z₀)² × T_Z ÷ T_M**. If the marines win, their survivors are S = √(M₀² − f (T_M ÷ T_Z) Z₀²), so a
survivor count implies f = (M₀² − S²) T_Z ÷ (T_M Z₀²).

**Assumptions.**

1. From contact on, all 24 marines fire. Range 5.875 between centres covers the marine block (2.4 deep) and the
   front zealots.
2. Damage counts as a fraction of a kill (a health pool). Real zealots keep their full damage until they die, so the
   two pre-contact variants bracket the truth:
   - **A**: the 66 pre-contact shots are spread and kill nobody, so Z₀ = 12 at contact.
   - **B**: the pre-contact damage counts as 2.28 dead zealots, so Z₀ = 9.72.
3. Sustained rates only. No overkill or wasted shots, no cancelled swings, no shield regeneration, no random delay.
4. f is constant through the fight.

**Results.**

| | Ours, A | Ours, B | Reference, A | Reference, B |
|---|---|---|---|---|
| Critical fraction f\* | **0.607** | **0.924** | **0.577** | **0.878** |
| Zealots in contact at the start, f\* × Z₀ | 7.3 | 9.0 | 6.9 | 8.5 |
| f implied by 16 marine survivors (top of the expected range) | 0.337 | 0.513 | 0.320 | 0.488 |
| f implied by 13 survivors | 0.429 | 0.653 | 0.408 | 0.621 |
| f implied by 6 survivors | 0.569 | 0.866 | 0.541 | 0.824 |
| f implied by 4 survivors (bottom of the expected range) | 0.590 | 0.899 | 0.561 | 0.854 |

With every zealot in contact (f = 1, variant A) the zealots win clearly in both data sets: 7.5 (ours) or 7.8
(reference) of 12 survive. At f = 0.5 the marines win with 10.1 (ours) or 8.8 (reference) alive. The data
differences of §2 move f\* by 0.03 (5 %), about 0.4 of a zealot.

### 3.4 Contact capped by the front

A constant fraction is not the geometry of battle 33. Zealots in contact are limited by the room along the marine
block, not by how many zealots are alive. So a second form caps them at K:
dM/dt = −min(Z, K) ÷ T_Z, with dZ/dt as before.

**How many zealots fit.**

- **Around one marine.** At most ⌊π ÷ asin(0.5 ÷ r)⌋ zealots fit on a ring of radius r, because zealots are 1.0
  apart (two radii of 0.5). For our band, r = 1.175 gives **7**. For the reference, r = 0.975 gives **5**, and so does
  touching distance (r = 0.875).
- **Along the marine block's front face.** The six front marines span 4.0 between centres. A zealot standing 1.0
  ahead of the face reaches a front marine across a span of 4.0 + 2√(1.175² − 1.0²) = 5.2, which holds **6** zealots.
  The reference zealot touching the face reaches across 4.0 + 2√(0.975² − 0.875²) = 4.9, which holds **5**.
- **All round the block.** The contour of the 2.4 × 4.0 block holds about 20 (ours) or 19 (reference) zealots. So all
  12 zealots fit only if they wrap around the flanks.

**Results** (survivors when the fight ends; † marks a fight that lasts longer than the contact time left in 500
ticks, so the battle would end on time with both sides alive).

| K (zealots in contact at most) | Ours, A | Ours, B | Reference, A | Reference, B |
|---|---|---|---|---|
| 3 | marines, 12.7 | marines, 15.8 | marines, 11.8 | marines, 15.3 |
| 4 | marines, 7.0 † | marines, 13.0 | marines, 4.6 † | marines, 12.1 |
| 5 | zealots, 2.8 | marines, 10.0 | zealots, 3.5 | marines, 8.7 |
| 6 | zealots, 4.5 | marines, 6.6 | zealots, 5.0 | marines, 4.1 † |
| 7 | zealots, 5.6 | marines, 1.3 † | zealots, 6.0 | zealots, 2.0 |
| 8 | zealots, 6.4 | zealots, 2.0 | zealots, 6.7 | zealots, 2.9 |

**Reading.** The zealots need **5 or more** in contact for the whole fight (variant A) or **8 or more** (variant B).
Battle 33's 6 to 13 survivors match about **K = 4 to 6**: the marine block's front face, with little wrap-around onto
its flanks. The reference data gives the same picture, with the crossover at most one zealot lower. So the catalog
differences cannot by themselves turn a zealot win into a marine win. What decides battle 33 is how many zealots
reach marines at once. That depends on our rules for solid enemy bodies, flanking (`flank_if_blocked`), target
switching and hostile contact (§2.5), which the plan's history notes tie to commit `d4109e2`.

**A consistency check, not evidence.** If the front grows with the square root of the army, then at test O1's size
(50 marines against 25 zealots, §1.9) K scales by √(25 ÷ 12) ≈ 1.44: from about 5 to about 7. With the reference
data, variant A at K = 7 leaves **20.3 of 50 marines** alive; O1 reported 20. This agrees with the sources' view that
marines gain as numbers grow (§1.10). O1 is a single anecdotal test.

### 3.5 Measured contact fraction

Measured by WP3 with the trace tool (§4.1) on 68 seeds of battle 33: the four seeds of record and 64 jittered seeds.
"In contact" means a zealot with a living marine inside its weapon band (1.175 between centres, without the release
slop); K is the number of zealots in contact on a tick.

| Measure (mean over 68 seeds, SD in brackets) | Measured | Model (§3.3, §3.4) |
|---|---|---|
| K over the first 5 s after the first zealot reaches its band (125 ticks) | **4.25** (0.40); range 3.23 to 4.97 | K = 4 to 6 implied by 6 to 13 survivors |
| K over the whole contact window (first contact to the end) | 2.96 (0.25) | — |
| Largest K on any tick | 6 (median), 4 to 7 | 6 along the front face, 7 around one marine |
| Share of contact-window ticks with K = 0, 1, 2, 3, 4, 5, 6, 7 (all seeds pooled) | 11.7, 11.2, 15.3, 19.4, 20.8, 15.7, 5.8, 0.1 % | — |
| Fraction of the living zealots in contact, f, mean per tick over the window | 0.648 (0.050) | f\* = 0.607 (A), 0.924 (B) |
| Zealots alive, mean over those first 5 s | 8.47 | 12 (A), 9.72 (B) |

**Reading.**

1. **K is about 4, and the capped model predicts the result.** With K = 4 in variant A, §3.4 predicts the marines win
   with 7.0 alive; the 68 seeds give a mean of **7.41** (SD 3.34, median 8). The per-seed K in the first 5 s explains
   most of the spread between seeds: its correlation with marine survivors is **−0.79**. Binned, seeds with K from 3.5
   to 4 leave 10.2 marines on average (17 seeds), 4 to 4.5 leave 7.4 (29 seeds), 4.5 to 5 leave 4.4 (20 seeds).
2. **The front never holds more than 7, and it is rarely full.** K reaches 6 on 5.8 % of contact ticks and 7 on 0.1 %.
   This matches §3.4's geometry (6 along the front face, 7 around one marine). All 12 zealots never fight at once.
3. **The constant-fraction model of §3.3 does not fit, because f is not the limit.** The measured f (0.648) is above
   variant A's f\* (0.607), yet the marines win 61 of 68 seeds. f is high because zealots die fast: the living count
   falls to 8.5 on average within 5 s of contact, while those in contact stay near 4 to 5. The cap on K (the room at
   the front), not the share of the living, decides the battle, so §3.4's form is the one to use.
4. **The zealots lose numbers early, to focused fire.** 0.5 zealots are dead when the first zealot reaches its band,
   3.3 after 2 s and 5.8 after 5 s. The marines' fire, about 24 × 10 = 240 damage per second, concentrates on the
   front zealots (closest-target acquisition) and kills one about every 0.7 s. That is closer to variant B (pre-contact
   damage counted as kills) than to A, while K stays at about 4.

## 4. Trace findings (WP3)

Written 2026-11-03 against `main` at `2296113`. Tool, inferences and runs are described in §4.1; the outputs are on
the development server under `~/scratch/zm_wp3/baseline/<battle>/` (not committed; any reviewer can regenerate them
with the command in §4.1, bit for bit, because the simulation is deterministic).

### 4.1 The tool

`sim_fixtures::trace` (in the test-support crate) and the runner `crates/sim_core/examples/matchup_trace.rs`:

```text
cargo run --release -p sim_core --example matchup_trace -- \
    --battle 33_24_marines_vs_12_zealots --seeds 64 --out ~/scratch/zm_wp3/baseline \
    [--seed <n>]... [--catalog-dir <dir>] [--fixture <file>] [--no-unit-csv]
```

- **Battles and seeds.** The matrix setup (fixture schema, spawn jitter, first-tick commands, end condition) moved from
  the matrix test into `sim_fixtures::matchup`, which both the matrix test and the tool now use, so the tool plays
  exactly the battles the test checks. `--seeds 64` plays the four seeds of record plus the first 64 positive integers
  that are not seeds of record (1 to 67 without 11, 22 and 33): **68 distinct seeds**. Plan D-3's "seeds 1 to 64 plus
  the four of record" would repeat 11, 22 and 33. The first seed of record plays the authored positions; every other
  seed jitters each spawn by up to ±0.12, as in the matrix test.
- **Outputs**, in `<out>/<battle>/`: per seed `seed_<n>_ticks.csv` (per tick and side: alive, health, health lost and
  regained, swings started, shots released, cancellations by cause, wasted shots, target switches, units per activity,
  pushes in contact), `seed_<n>_units.csv` (per tick and unit: position, velocity, life, shields, weapon phase, target,
  activity) and `seed_<n>.md` (summary and one-second table); for all seeds `seeds.csv` and `summary.md` (outcomes,
  survivor histograms, the spread of every measure, the pooled K histogram, contact over time, a per-seed table).
- **What is inferred, not published by the simulation** (each unit-tested on hand-made phases and on scripted two- and
  three-unit battles, `crates/sim_fixtures/tests/trace_tests.rs`):
  - *Releases and cancellations* from consecutive weapon phases (`trace::swing`): a wind-up at `remaining <= 1`
    releases if the next phase is its backswing or cooldown, and was cancelled otherwise. A unit killed in the tick
    it fired still fired (all weapons advance before any shot lands), so its release is inferred from its last
    wind-up and its target's reach.
  - *Cancellation cause*: the bound target is dead or gone ("target gone"), or alive but beyond the band plus the 0.5
    release slop ("out of band"), or neither ("other").
  - *Wasted shots*: shots at a target that died in the tick are replayed in attacker-id order (the order
    `combat_resolution` applies them) with the simulation's own `calculate_hit_damage_upgraded`, from the target's
    health at the previous tick; every shot after the killing one is wasted. Instant weapons only.
  - *Activity*: engaged (an enemy in band); otherwise, with a target, blocked (moved less than 10 % of its maximum
    speed), flanking (moved more than 60° away from the direction to its target) or approaching; without a target,
    walking or idle. Movement is `Entity::velocity`, which the collision phase sets to the movement achieved, pushes
    included. "Pushed in contact" counts engaged units that moved at least 10 % of their speed.
- **Checks on the tool itself.** Tracing changes nothing (same checksum and tick count with and without the trace).
  The seeds of record reproduce the matrix result: 8, 6, 9 and 13 marines alive on seeds 3203386110, 11, 22 and 33,
  the "6 to 13" of commit `d4109e2`. A scripted zealot against a held marine makes contact on the tick the catalog
  predicts (speed 0.126, band 1.175) and swings on contact + 4, + 26 and + 48.
- **Not exercised.** WP2's reading that a weapon with damage point 0 fires every `period + 1` ticks (§2.5) cannot be
  confirmed here: no unit in these battles has a damage point of 0. The swing inference handles that case
  (`Windup { remaining: 0 }`), so WP4's counterfactual can measure it with the same tool.

### 4.2 Battle 33 on 68 seeds: outcomes

| | Result |
|---|---|
| Marines win | **61** of 68 seeds |
| Zealots win | **6** (seeds 1, 6, 28, 50, 58, 63), with 2 or 3 zealots left |
| Unresolved at 500 ticks | 1 (seed 19: 3 marines against 1 zealot) |
| Marine survivors | mean **7.41**, SD 3.34, median 8, range 0 to 14 |
| Marine survivors, histogram (survivors: seeds) | 0: 6, 3: 1, 4: 4, 5: 7, 6: 7, 7: 3, 8: 10, 9: 13, 10: 5, 11: 7, 12: 3, 13: 1, 14: 1 |
| Zealot survivors, histogram | 0: 61, 1: 1, 2: 2, 3: 4 |
| End tick | mean 420.4, SD 36.6, range 351 to 500 |

**The recorded outcome does not hold on every seed.** The matrix expects `blue: (4, 16), red: (0, 0)` on its four
seeds, and those four pass. On the 64 further seeds it fails 7 times: 6 zealot wins and 1 timeout (seed 19, 3 marines
against 1 zealot). With the same ±0.12 jitter, battle 33 is about a 90 % marine win, not a certain one. This plan does not change
the expectation (Q0007 keeps the baselines); WP5 reports it to the owner.

### 4.3 Battle 33: what happens, and what does not

Means over the 68 seeds, per battle (SD in brackets).

| Measure | Marines (blue) | Zealots (red) |
|---|---|---|
| First tick in contact | 77.6 (0.5); model 77.4 (§3.2) | 114.6 (0.6); model about 115 (§3.2, without the wind-up) |
| Units in contact, max | 24 | 6 (median), 4 to 7 |
| Unit-seconds engaged | 205.3 (5.0) | 40.9 (6.9) |
| Unit-seconds flanking | 3.6 | **11.5** (1.7) |
| Unit-seconds blocked | 0.1 | **0.3** (0.3) |
| Unit-seconds approaching | 11.5 | 33.0 (1.0) |
| Shots or swings released | 349.8 (8.7) | 53.3 (9.2) |
| Swings cancelled, target gone | 7.76 (3.60) | **1.44** (1.21) |
| Swings cancelled, out of band | 0 | **0** |
| Swings cancelled, other | 0 | 0 |
| Shots wasted on a target already dead | 4.01 (2.72) | 0.28 (0.57) |
| Target switches between living enemies | 0.90 | 4.04 (1.67) |
| Unit-ticks pushed while in contact | 29.6 | 40.8 (13.6) |

**Contact over time** (mean over seeds; n = seeds still fighting in that second):

| s | n | Marines alive | Zealots alive | Marines in contact | Zealots in contact (K) | Zealots flanking | Zealots approaching |
|---|---|---|---|---|---|---|---|
| 4 | 68 | 24.0 | 12.0 | 14.09 | 0.00 | 0.00 | 11.98 |
| 5 | 68 | 24.0 | 10.8 | 24.00 | 1.09 | 0.94 | 9.46 |
| 6 | 68 | 23.4 | 9.4 | 23.83 | 3.86 | 3.32 | 2.67 |
| 7 | 68 | 20.9 | 8.3 | 22.22 | 4.61 | 2.34 | 1.54 |
| 8 | 68 | 18.6 | 7.5 | 19.57 | 4.53 | 1.75 | 1.33 |
| 9 | 68 | 17.1 | 6.7 | 17.74 | 4.65 | 1.30 | 0.93 |
| 10 | 68 | 15.3 | 5.6 | 16.25 | 4.48 | 0.85 | 0.67 |
| 11 | 68 | 13.2 | 4.6 | 14.07 | 3.86 | 0.50 | 0.58 |
| 12 | 68 | 11.9 | 4.0 | 12.44 | 3.55 | 0.26 | 0.36 |
| 13 | 68 | 10.7 | 3.1 | 11.29 | 3.16 | 0.11 | 0.24 |
| 14 | 68 | 9.5 | 2.4 | 10.02 | 2.52 | 0.05 | 0.15 |
| 15 | 68 | 8.7 | 1.8 | 8.76 | 1.87 | 0.01 | 0.11 |
| 16 | 62 | 7.7 | 1.1 | 7.67 | 1.50 | 0.01 | 0.07 |
| 17 | 41 | 6.2 | 1.1 | 5.86 | 1.37 | 0.02 | 0.08 |

Seconds 1 to 3 are the walk (nobody in contact); seconds 18 to 20 hold the few long seeds.

**Findings.**

1. **The zealots that are in contact fight at full rate.** 40.9 zealot-seconds in contact allow 40.9 ÷ 0.88 = 46.5
   swings; 53.3 were released (a zealot also swings in the slop band while a target steps away). The marines released
   349.8 shots in 205.3 marine-seconds in contact, 1.70 per second, the full rate of 1 ÷ 0.6 = 1.67. Neither side
   loses meaningful damage to its weapon cycle.
2. **Cancelled zealot swings are rare: 1.44 per battle, 2.7 % of 53.3.** Every one is "target gone": another zealot
   killed the marine during the 4-tick wind-up. None is out of band, so marines pushed by contact never escape a swing.
   0.28 zealot swings per battle are wasted on a marine already killed in the same tick. Together they cost the
   zealots about 1.7 swings, about half a marine kill. **The cancellable wind-up (§2.4) is not a material cause.**
3. **Marine overkill is small too:** 4.0 shots per battle wasted on a zealot killed earlier in the same tick, and 7.8
   marine wind-ups (2 ticks) cancelled because the target died, together 3.4 % of the shots. Neither side smart-fires.
4. **The rear zealots are not standing blocked; they are walking around.** 0.3 zealot-seconds blocked against 11.5
   flanking and 33.0 approaching. In seconds 6 to 9, of 7.5 to 9.4 living zealots about 4.5 are in contact, 1.3 to 3.3
   are flanking along the crowd and 0.9 to 2.7 still approaching. `flank_if_blocked` keeps them moving, but the walk
   around the marine block is long enough that the fight at the front is decided before they arrive.
5. **Pushes in contact are small.** Zealots in contact moved on 40.8 of about 1 020 zealot-ticks in contact (4 %),
   marines on 29.6 of about 5 130 (0.6 %). The hostile-separation rule reshapes the front little once contact forms.
6. **Target switching is low:** 4.0 zealot switches per battle between two living marines, about one per three
   zealots. It does not keep zealots out of contact in measurable amounts.
7. **What varies between seeds is K.** Per seed, marine survivors correlate with K in the first 5 s (−0.79), with the
   zealots' total swings (−0.99, the same thing seen from the other side), with zealot pushes in contact (−0.70), and
   positively with zealot flanking time (+0.54). They hardly correlate with cancelled zealot swings (−0.14), wasted
   marine shots (+0.19) or target switches (−0.01).

### 4.4 The fights that still favour the zealots

Same tool, 68 seeds each (the four seeds of record plus 64). Battles 25a and 25b are matrix battles. The two
benchmarks of `sc2_battle_benchmarks_tests.rs` (`test_equal_cost_1_zealot_vs_2_marines_vs_3_marines`, parts A and B)
were restated as a scratch fixture, `~/scratch/zm_wp3/benchmarks.ron`, with world seed 0x4242 first. It differs from
the test in three ways: marines are blue and the zealot red (so the columns read as in battle 33), so the marines
spawn first; orders go out as commands on tick 0 instead of being pushed into the order queues; and the test's
assertions are not applied, only recorded.

| Battle | Outcome on 68 seeds | Zealot health left | Zealot first contact (tick) | Zealot swings | Cancelled / wasted (zealot) | Marine shots wasted |
|---|---|---|---|---|---|---|
| 25a: 2 marines vs 1 zealot | zealot wins 68 | 61.8 (57 to 62) | 39.1 | 6.0 | 0 / 0 | 0 |
| 25b: 2 upgraded, stimmed marines vs 1 zealot | zealot wins 68 | 23.2 (22 to 27) | 35.0 | 6.0 | 0 / 0 | 0 |
| Benchmark A: 1 zealot vs 2 marines | zealot wins 68 | 62.3 (62 to 67) | 30.5 | 6.0 | 0 / 0 | 0 |
| Benchmark B: 4 marines vs 1 zealot | marines win 68, 3 alive | 0 | 29.2 | 4.0 | 0 / 0 | 0.37 |

These outcomes are fully stable under jitter. With a single zealot the contact question does not arise: it is in
contact for all of its fight (3.8 to 4.5 zealot-seconds) and never flanks or waits. Its six swings kill two marines in
the minimum possible number. So the small fights and battle 33 do not disagree about the units; they differ in how many
zealots reach the marines at once, which only the large battle tests.

### 4.5 What WP3 says about the model and the plan's hypotheses

- **Confirmed:** the outcome of battle 33 is set by the number of zealots in contact, K ≈ 4 (4.25 in the first 5 s,
  at most 6 to 7), against the 5 or more the zealots need (§3.4). The capped model with K = 4 predicts 7.0 marine
  survivors; the 68-seed mean is 7.41.
- **Ruled out as material:** cancelled zealot swings (1.44 per battle, all from a target killed by another zealot),
  zealot overkill (0.28), marines escaping swings by being pushed (0 out-of-band cancellations), target switching
  (4.0 per battle), and zealots standing blocked behind each other (0.3 zealot-seconds).
- **What keeps K low:** the room along the marine block (6 at its front face, 7 around one marine, §3.4); the
  marines' focused fire, which removes front zealots at about one per 0.7 s; and the time rear zealots spend walking
  around the crowd (11.5 zealot-seconds flanking, 33.0 approaching) instead of arriving together. WP4 measures which
  of these the history (`d4109e2`) and the reference data change.
- **New for WP5:** battle 33's recorded outcome fails on 7 of 64 jittered seeds (§4.2).

## 5. Experiments (WP4)

Written 2026-11-04 against `main` at `c68d5e8`. `main` moved to `51946c5` during the work without touching
`sim_core`, `sim_types`, `sim_map`, `assets/catalog` or the matrix fixture, so every number below holds there too.
Nothing here changed a catalog value, a rule or an expected result. Every experiment used a scratch catalog or a
scratch fixture under `~/scratch/zm_wp4/` on the development server, or an exported tree of an old commit.

### 5.1 Method

- **Runs.** Each experiment changes one thing and plays battle 33 on WP3's 68 seeds (the four of record and 64
  jittered ones, §4.1) with the trace tool, `--seeds 64 --no-unit-csv`, plus `--catalog-dir <scratch catalog>` or
  `--fixture <scratch fixture>`. A scratch catalog is a full copy of `assets/catalog` with only the named fields of
  `pulse_bot` or `psi_grey` edited in `units.ron`. The baseline is WP3's run, which this work reproduced bit for bit.
- **Measures.**
  - Wins, and unresolved battles (both sides alive at the 500-tick limit).
  - Marine survivors: mean and SD over the 68 seeds.
  - The **paired difference** Δ against the baseline: each seed compared with itself, given as mean and standard
    error. It is far more sensitive than comparing two means.
  - K over the first 5 s after the first zealot reaches its band (same definition as §3.5).
  - Zealot swings released.
- **The 500-tick limit leaves some fights unresolved** when a variant slows the marines' win. §5.6 replays the main
  variants with 1500 ticks, so every fight ends.
- **Scripts.** The catalog builder, job runner, summaries and the history runner of §5.2 are throwaways in
  `~/scratch/zm_wp4/scripts/`. They are not committed, as the plan allows for WP2's model. Every catalog edit is
  listed in the tables, so any run can be repeated with the committed tool.

### 5.2 History: `d4109e2`'s parent against `d4109e2`

The trace tool did not exist at these commits. So each commit was exported with `git archive` to
`~/scratch/zm_wp4/trees/<commit>/` and built with its own target directory, with one scratch integration test added
(never committed). That test copies the commit's own matrix runner (`Jitter`, `spawn_side`, `side_commands`, `play`)
and plays battle 33 of the commit's own fixture on the same 68 seeds. Per tick it records, from public state:

- units alive;
- zealots in contact (K): a living marine within `max_range` plus both collision radii, centre to centre;
- hostile body overlap: zealot–marine pairs closer than the sum of their collision radii.

The battle setup is the same at both commits and at `main`; only the expectation and the claim differ.

**Check of the runner.** At `d4109e2` it reproduces `main`'s trace exactly: the same marine survivors on all 68 seeds,
the same mean K over the first 5 s (4.25), and 8, 6, 9 and 13 marines on the seeds of record. So battle 33 has not
changed since `d4109e2`, and the runner measures K the way the trace tool does.

| | Parent `b10e9ca` | `d4109e2` (= `main`) |
|---|---|---|
| Outcome on 68 seeds | **zealots win 67**, 1 unresolved | marines win 61, zealots 6, 1 unresolved |
| Marine survivors, mean (SD) | 0.04 (0.36) | 7.41 (3.34) |
| Zealot survivors, mean (SD) | 4.31 (1.15); histogram 1: 1, 2: 4, 3: 6, 4: 29, 5: 21, 6: 4, 7: 3 | 0.25 (0.78) |
| Δ in marine survivors | −7.37 (SE 0.41), against `d4109e2` | — |
| Seeds of record (marines – zealots) | 0–5, 0–5, 0–4, 0–5 | 8–0, 6–0, 9–0, 13–0 |
| First zealot contact (tick) | 115.4 | 114.6 |
| **K, first 5 s, mean (SD)** | **5.40 (0.30)** | **4.25 (0.40)** |
| Largest K on any tick (median over seeds) | **8** | 6 |
| Hostile pairs overlapping, mean per tick in the first 5 s | 0.53 | 0 |
| Mean / deepest overlap of such a pair | 0.037 / 0.130 | 0 / 0 |

Contact over time (means over the seeds still fighting):

| s | Parent: marines | zealots | K | overlapping pairs | `d4109e2`: marines | zealots | K | overlapping pairs |
|---|---|---|---|---|---|---|---|---|
| 5 | 23.4 | 10.0 | 4.06 | 0.28 | 23.9 | 10.1 | 3.79 | 0 |
| 6 | 21.1 | 8.9 | 5.73 | 0.69 | 22.3 | 8.8 | 4.62 | 0 |
| 7 | 18.6 | 8.1 | **6.32** | 1.03 | 19.7 | 7.9 | 4.54 | 0 |
| 8 | 15.9 | 7.5 | **6.37** | 0.56 | 17.8 | 7.1 | 4.63 | 0 |
| 9 | 13.2 | 6.7 | 6.02 | 0.11 | 16.3 | 6.2 | 4.51 | 0 |
| 10 | 10.9 | 6.0 | 5.45 | 0.01 | 14.2 | 5.1 | 3.88 | 0 |
| 11 | 8.7 | 5.6 | 4.93 | 0.01 | 12.5 | 4.3 | 3.55 | 0 |
| 12 | 6.5 | 5.2 | 4.33 | 0 | 11.3 | 3.6 | 3.18 | 0 |
| 13 | 4.5 | 4.8 | 3.75 | 0 | 10.1 | 2.8 | 2.55 | 0 |
| 14 | 3.0 | 4.5 | 3.16 | 0 | 9.1 | 2.1 | 1.90 | 0 |

**Reading.**

1. **`d4109e2` flipped the battle by lowering K by about one zealot**: from 5.4 to 4.25 in the first 5 s, and the
   peak from 8 to 6. The zealots died at the same pace in both (8.9 against 8.8 alive at 6 s, 6.7 against 6.2 at 9 s).
   What changed is how many of the living zealots were striking. That is the threshold of §3.4: with K = 5 or more the
   zealots win, with K = 4 they lose.
2. **"Overlapping onto the marine line" was shallow.** Before `d4109e2`, 0.5 to 1 zealot–marine pairs overlapped per
   tick on average in the first seconds of contact. The overlap was 0.04 on average and 0.13 at most, against radii
   that sum to 0.875. The zealots did not stand inside the marines. The overlap let one or two more zealots squeeze
   onto the front: 8 at the peak against 6, and 6.3 against 4.6 at 7 to 8 s. That was enough to cross the threshold.
   After `d4109e2`, no hostile pair ever overlaps.
3. **The rule changes of `d4109e2` were not separated.** The plan allows a throwaway branch that reverts one rule at a
   time. But `d4109e2` changes 14 files of `sim_core/src` (+2 047, −1 082 lines), and its message says "each rule
   depends on the others to hold". Impenetrable hostile bodies, the step that "loses whatever would drive it into a
   body that will not yield", flanking and the arrival rules are written against each other. Before `d4109e2`, hostile
   pushes were already uncapped (`world.rs`, the `hard_pushes` accumulator), so "uncapped hostile correction" alone is
   not the difference. Reverting a single rule would test a combination that never existed, so this was not done.

### 5.3 Reference data, one value at a time

Battle 33, 500 ticks, 68 seeds. Δ is the paired difference in marine survivors against the baseline, mean (SE).
Reference values are §1's, converted to our ticks as in §2.2 and rounded to a whole tick.

| Variable | Shipped → scratch value | Marine wins / zealot wins / unresolved | Marine survivors mean (SD) | Δ (SE) | K first 5 s (SD) | Zealot swings |
|---|---|---|---|---|---|---|
| Baseline (WP3) | — | 61 / 6 / 1 | 7.41 (3.34) | — | 4.25 (0.40) | 53.3 |
| Marine speed | 0.09 → **0.126** (acceleration and deceleration likewise; both unused) | 58 / 4 / 6 | 6.72 (3.17) | −0.69 (0.44) | 4.27 (0.37) | 54.9 |
| Zealot range | 0.3 → **0.1** | 66 / 0 / 2 | 8.24 (2.47) | **+0.82** (0.42) | 4.16 (0.38) | 51.0 |
| Zealot period | 22 → **21** (damage point 4 kept: fires every 21) | 49 / 9 / 10 | 5.62 (3.52) | −1.79 (0.25) | 4.24 (0.38) | 57.9 |
| Zealot damage point | 4 → **0**, with period 22 → **21** so it still fires every 22 (see below) | 42 / 10 / 16 | 4.59 (3.06) | **−2.82** (0.33) | 4.28 (0.35) | 60.6 |
| Zealot damage point, period kept | 4 → 0, period 22 (fires every 23) | 55 / 2 / 11 | 6.22 (2.84) | −1.19 (0.32) | 4.31 (0.35) | 56.4 |
| Zealot damage point and faster cadence | 4 → 0, period 20 (fires every 21) | 25 / 26 / 17 | 2.46 (2.66) | −4.96 (0.37) | 4.25 (0.32) | 66.3 |
| Zealot backswing | 6 → 9 | 62 / 5 / 1 | 7.29 (3.26) | −0.12 (0.12) | 4.26 (0.40) | 53.3 |
| Marine damage point and backswing | 2 / 4 → 1 / 13 (ground and air weapon) | 63 / 1 / 4 | 7.50 (2.66) | +0.09 (0.39) | 4.25 (0.38) | 52.6 |
| Separation radius, both | 0.45 / 0.6 → 0.375 / 0.375; also tried equal to the collision radius (0.375 / 0.5) | identical to the baseline on all 68 seeds | 7.41 (3.34) | 0 | 4.25 (0.40) | 53.3 |
| Zealot attributes | Light, Psionic → Light, Biological (`bits` 17 → 5) | identical to the baseline on all 68 seeds | 7.41 (3.34) | 0 | 4.25 (0.40) | 53.3 |
| Zealot shield regeneration | 0.08 → 0.112 per tick | identical to the baseline on all 68 seeds | 7.41 (3.34) | 0 | 4.25 (0.40) | 53.3 |
| Push priority, both | 20 / 25 → 10 / 10 | identical to the baseline on all 68 seeds | 7.41 (3.34) | 0 | 4.25 (0.40) | 53.3 |
| Scan range, both | 9 / 9 → **5.5 / 5** | 37 / 11 / 20 | 4.29 (3.23) | **−3.12** (0.59) | 4.45 (0.29) | 62.0 |
| Scan range, marine only | 9 → 5.5 | 50 / 5 / 13 | 5.78 (3.24) | −1.63 (0.52) | 4.24 (0.30) | 57.8 |
| Scan range, zealot only | 9 → 5 | 58 / 1 / 9 | 6.35 (2.82) | −1.06 (0.55) | 4.36 (0.36) | 56.2 |
| **All reference values** | every row above at its reference value (zealot damage point 0, period 21) | **45 / 17 / 6** | **4.71 (3.34)** | **−2.71** (0.61) | 4.28 (0.33) | 60.2 |
| All reference, zealot every 21 ticks | as above, but zealot period 20 | 28 / 28 / 12 | 2.65 (2.76) | −4.76 (0.56) | 4.27 (0.33) | 65.9 |
| All reference but the marine speed | marine speed stays 0.09 | 42 / 13 / 13 | 4.81 (3.38) | −2.60 (0.58) | 4.31 (0.32) | 60.3 |
| All reference but the zealot range | zealot range stays 0.3 | 32 / 24 / 12 | 3.24 (3.20) | −4.18 (0.61) | 4.39 (0.28) | 64.8 |
| All reference but the scan ranges | scan ranges stay 9 / 9 | 57 / 8 / 3 | 6.41 (3.64) | −1.00 (0.48) | 4.17 (0.39) | 55.9 |

**A damage point of 0 fires every `period + 1` ticks: WP2's reading (§2.5) is confirmed.** On the one-zealot
benchmark (§4.4, seed 0x4242), the marines took damage every:

- 22 ticks with the shipped weapon;
- 23 ticks with damage point 0 and period 22;
- 22 ticks with damage point 0 and period 21;
- 21 ticks with damage point 0 and period 20.

So the experiment "damage point 0" runs with period 21. It then fires every 22 ticks, as today, and changes only the
wind-up. The reference cadence (21.43 ticks) lies between periods 20 and 21 at damage point 0, so both are shown.

**Not expressible in data, so not run:**

- **The 5-tick gap between the zealot's two strikes**, and the 2.1 reach of the second (§1.5). `attacks_count`
  applies every strike in the same tick (`combat_resolution.rs`), and no catalog field spaces them.
- **The shield-regeneration delay** (250 ticks against 178.6), which is hard-coded in `sim_core` (§2.4). The
  regeneration rate alone changes nothing (table), because no zealot goes 250 ticks unhit while it matters.
- Acceleration and turn rate are parsed but have no effect in this battle (§2.3), so they were not varied.

**Reading.**

1. **No single data difference makes the zealots the favourites, and all of them together only partly close the
   gap.** The all-reference catalog moves the mean from 7.41 to 4.71 marines and the zealot wins from 6 to 17 of 68.
   The marines still win 45; on 1500 ticks (§5.6) the split is 49 to 19. K over the first 5 s stays at 4.28: the
   reference data does not change how many zealots reach the marines, only what each contact is worth.
2. **The zealot's wind-up and cadence are the largest data effects.**
   - The 4-tick wind-up costs the marines 2.82 survivors (paired), although WP3 found only 1.4 swings cancelled per
     battle (§4.3). The wind-up also delays the first strike of every new engagement by 4 ticks (3 more than damage
     point 0, which still waits one tick), and it holds the zealot still.
   - Each tick of period moves the result by about 1.6 to 2.1 marines. With damage point 4, period 22 → 21 gives
     −1.79. With damage point 0, cadence 23 → 22 → 21 gives −1.19, −2.82 and −4.96.
   - Battle 33 sits on a steep slope: a 5 % change in the zealot's rate of fire moves the mean by about a quarter.
3. **The scan range is the third, and its mechanism is not isolated.** The reference scan ranges (5.5 and 5) cost the
   marines 3.12 survivors: 1.63 from the marine's alone and 1.06 from the zealot's alone. K rises only from 4.25 to
   4.45. Meanwhile zealot swings rise from 53.3 to 62.0, marine target switches fall from 0.90 to 0 per battle, and
   zealot pushes in contact rise from 40.8 to 50.8 unit-ticks. The likely reading is a change in which zealots the
   marines focus, but the trace does not show it directly. WP5 should treat it as a measured effect with an open
   mechanism.
4. **Marine speed hardly matters** (−0.69, SE 0.44), as WP2 predicted. Without kiting it only moves the meeting 11
   ticks earlier. **Zealot range 0.1 favours the marines** (+0.82), also as predicted: fewer zealots fit on the ring
   (§3.4). In the all-reference set, keeping range 0.3 instead costs the marines 1.5 more survivors.
5. **Four flagged differences are exactly inert in battle 33.** Separation radius, zealot attributes, shield
   regeneration rate and push priority give results identical to the baseline on all 68 seeds, as §2 predicted from the
   code. The marine's damage point and backswing (1 / 13) change single seeds but not the mean (+0.09, SE 0.39).

### 5.4 Formation and setup

The scratch fixture `~/scratch/zm_wp4/zm_wp4_formations.ron` holds battle 33 with one setup change each, on the
shipped catalog unless stated. Its control battle, a verbatim copy of battle 33, reproduces the baseline on every
seed.

| Setup change | Marine wins / zealot wins / unresolved | Marine survivors mean (SD) | Δ (SE) | K first 5 s (SD) | Largest K (median) | Zealot swings |
|---|---|---|---|---|---|---|
| Baseline: zealots in a 3 × 4 block, marines 4 × 6 at 0.8, fronts 22.6 apart | 61 / 6 / 1 | 7.41 (3.34) | — | 4.25 (0.40) | 6 | 53.3 |
| **Zealots in one line of 12** facing the marines (spacing 1.2, front at x 45, centred like the block) | **4 / 51 / 13** | **0.63 (1.29)** | **−6.78** (0.44) | **5.37** (0.36) | 7 | 71.0 |
| The same line, all-reference catalog | 0 / 63 / 5 | 0.12 (0.47) | −7.29 (0.40) | 5.32 (0.35) | 7 | 72.2 |
| Marines spaced 1.2 instead of 0.8 (front kept at x 22.4, centre at y 46) | 54 / 8 / 6 | 6.44 (3.53) | −0.97 (0.48) | 4.31 (0.39) | 6 | 55.4 |
| Start distance halved: fronts 11.3 apart, each side moved 5.65 closer | 58 / 5 / 5 | 5.85 (3.16) | −1.56 (0.49) | 4.46 (0.37) | 6 | 57.3 |
| **O1's setup** (§1.9): 50 marines (5 × 10 at 0.8) against 25 zealots (5 × 5 at 1.2), 1000 ticks | 68 / 0 / 0 | **29.34 of 50** (1.96), 59 % | — | 5.49 (0.29) | 8 | 68.3 |
| O1's setup, all-reference catalog | 68 / 0 / 0 | 27.69 of 50 (1.99), 55 % | — | 5.37 (0.30) | 8 | 73.3 |

**Reading.**

1. **Formation decides battle 33 more than any data value.** Putting the same 12 zealots in a line instead of a
   3-deep block raises K from 4.25 to 5.37. That turns a 90 % marine win into a 75 % zealot win (−6.78 marines), more
   than twice the all-reference effect. It is the same mechanism as the history: about one more zealot in contact.
2. **Marine spread and start distance move the result by one to one and a half marines**, through a slightly higher K
   (4.31 and 4.46). Neither changes the winner.
3. **At O1's size our marines do better than the one observed test.** O1 reports 20 of 50 marines alive (40 %). We get
   29.3 (59 %), or 27.7 (55 %) with the reference data. K here is 5.5, below the 7 that §3.4's square-root scaling
   assumed, so our front grows more slowly with army size than that estimate. O1 is a single anecdotal test (§1.9), so
   this is a hint, not a measured error.

### 5.5 Rules measured by trace only (plan item 4)

In every 24-against-12 variant of §5.3 and §5.4:

- **No zealot swing was cancelled because its target left the band.** All cancellations are "target gone".
- **Zealots spend 0.2 to 0.4 zealot-seconds blocked** per battle, against 10.4 to 12.6 flanking.

So, as in WP3, no variant has zealots robbed of swings by pushed targets or stuck behind each other. The limit is
where the zealots stand when the front is full, and the formation sets that.

### 5.6 Fought to the end: 1500 ticks

The same battle with only the tick limit raised to 1500 (scratch fixture), so no fight is unresolved.

| Catalog | Marine wins / zealot wins | Marine survivors mean (SD) |
|---|---|---|
| Shipped | 62 / 6 (seed 19 becomes a marine win) | 7.41 (3.34) |
| Marine speed 0.126 | 59 / 9 | 6.62 (3.35) |
| Zealot period 21 | 56 / 12 | 5.51 (3.65) |
| Zealot damage point 0, period 21 | 49 / 19 | 4.35 (3.30) |
| Zealot damage point 0, period 22 | 59 / 9 | 6.00 (3.21) |
| Scan ranges 5.5 / 5 | 44 / 24 | 3.94 (3.55) |
| All reference | 49 / 19 | 4.65 (3.39) |
| All reference, zealot every 21 ticks | 35 / 33 | 2.50 (2.84) |

The unresolved battles of §5.3 split between the sides much as the resolved ones do, and the ranking of the variants
is unchanged. Even with every reference value and the faster of the two reference cadences, the zealots win only
about half the seeds.

### 5.7 Control: the whole matrix on the all-reference catalog

All 50 battles of the shipped matrix were played on the four seeds of record plus 16 jittered seeds, once with the
shipped catalog and once with the all-reference catalog of §5.3. Survivor ranges were checked against each battle's
`expect`. Lead-unit hull was checked by hand for the battles whose health moved.

- **47 battles keep their survivor counts** (identical means over the 20 seeds). Most contain neither unit. Those
  that do (5a to 5c, 7, 8, 11a, 12a, 15, 16a, 16b, 21, 23b, 25a) change at most the remaining health, or the end tick
  by a few ticks, and keep their expectation.
- **Three battles change their survivor counts:**

| Battle | Expectation | Shipped, seeds of record (blue – red) | All reference, seeds of record | Seeds failing the expectation, of 4 / of 20 |
|---|---|---|---|---|
| 23a zealot vs 6 lings | blue 0, red 1 or more | red 2 on all four | red 2 on all four (mean over 20 seeds 2.10 → 2.00) | 0 → 0 / 0 → 0 |
| 32 30 marines vs 15 banelings | blue 0 to 15 | 0–1, 2–0, 1–0, 0–0 | 3–0, 2–0, 0–0, 0–0 | 0 → 0 / 0 → 0 |
| **33 24 marines vs 12 zealots** | blue 4 to 16, red 0 | 8–0, 6–0, 9–0, 13–0 | **3–1, 1–1, 3–1, 9–0** | **0 → 3 / 2 → 6** |

- **One more battle fails on its lead-unit hull.** Battle **25b** (2 upgraded, stimmed marines against 1 zealot)
  expects the zealot to end with 5 to 45 hull. With the all-reference catalog it ends with **62** on all four seeds,
  instead of 22.
  - **The cause is the marine's backswing of 13.** On the first seed of record, marine damage point and backswing
    1 / 13 alone give 62. Marine speed, zealot range, zealot backswing, shield regeneration and push priority leave 22.
    Zealot damage point 0 or the reference scan ranges give 32, still inside the range.
  - **The marines fire 16 shots instead of 24.** In `step_entity_weapon` (`combat.rs`) a weapon leaves its
    `Backswing` phase only when the backswing ends. So a backswing longer than the cooldown lengthens the period.
    Unstimmed (period 15, cooldown 14 after a damage point of 1), a 13-tick backswing fits. Stimmed (period 10), it
    does not.
  - **StarCraft II does not work this way.** There the backswing is a lock on new orders, not a gate on the next
    attack (§1.8, Damage Point), and stimmed marines fire every 0.41 s. Whether the game also scales the backswing by
    the attack-speed multiplier is **not found**.
  - So this failure is a rule interaction exposed by the reference value, not evidence about 25b's outcome.

**Reading.** The all-reference catalog costs little in the rest of the matrix. Of 50 battles it breaks only two:
battle 33 (3 of 4 seeds of record) and battle 25b (through the backswing gate above). Any proposal that adopts the
marine's reference backswing needs that rule looked at first.

### 5.8 What the experiments say about the cause

| Experiment | Effect on marine survivors (paired Δ) | K first 5 s | What it says |
|---|---|---|---|
| History: before `d4109e2` | −7.37 (zealots win 67 of 68) | 5.40 against 4.25 | The flip came from contact rules that let about one more zealot reach the front, not from data |
| Zealots in a line | −6.78 (zealots win 51) | 5.37 | The same lever as the history, set by the formation instead of the rules |
| All reference data | −2.71 (marines still win 45) | 4.28 | The data differences are real but secondary: they change what a contact is worth, not how many contacts there are |
| Zealot damage point 0 (same cadence) | −2.82 | 4.28 | The largest single data effect: the 4-tick wind-up delays every new engagement |
| Zealot period 21 | −1.79 | 4.24 | Battle 33 is very sensitive to the zealot's rate of fire |
| Scan ranges 5.5 / 5 | −3.12 | 4.45 | Large, with the mechanism not isolated |
| Zealot range 0.1 | +0.82 | 4.16 | Favours the marines, as predicted |
| Marine speed 0.126 | −0.69 | 4.27 | Small, as predicted without kiting |
| Separation, attributes, shield regeneration, push priority | 0 | 4.25 | Inert, as predicted from the code |
| O1's setup (50 against 25) | 59 % alive, against O1's 40 % | 5.49 | Our rules favour the marines somewhat more than the one observed large test |

**In short:** K, the number of zealots striking at once, decides battle 33.

- The rules of `d4109e2` hold K near 4, below the 5 the zealots need.
- The battle's 3-deep zealot block keeps K low too. A line formation lifts it past the threshold.
- The reference data makes each contact worth more to the zealots: no wind-up, a faster cadence, shorter scan
  ranges. It moves the battle from a 90 % to about a 70 % marine win, but on its own it does not make the zealots
  the favourites.

WP5 ranks these causes and writes the questions.

### 5.9 Deviations from the plan

- **History runner.** The trace tool postdates `d4109e2`. So the history used a scratch integration test that copies
  each commit's own matrix runner and measures K and overlap from public state (§5.2), not the full trace. Its K
  matches the tool's exactly at `d4109e2`.
- **History trees.** They were exported with `git archive` under `~/scratch/zm_wp4/trees/`, not created with
  `git worktree add`. `1349c67` was not built: `d4109e2`'s parent already shows the zealot win, and `d4109e2` is the
  commit that flipped it.
- **No per-rule revert of `d4109e2`** (§5.2, reading 3).
- **Rounding of reference cadences.** Reference times are rounded to whole ticks: zealot period 21 (21.43), zealot
  backswing 9 (8.93), marine damage point 1 (0.89) and marine backswing 13 (13.39). The zealot's damage point 0 runs
  with period 21 so that its cadence stays 22 (§5.3), and both neighbouring cadences are reported.
- **Extra variants** not in the plan's list:
  - scan range, which WP2 asked WP4 to measure;
  - push priority;
  - the damage-point cadence pair;
  - three "all reference but one" catalogs;
  - a 1500-tick replay;
  - the O1 setup on both catalogs.
- **Matrix control on 20 seeds**, the four of record plus 16, not 68, for all 50 battles. Lead-unit hull was checked
  by hand only where health moved.
