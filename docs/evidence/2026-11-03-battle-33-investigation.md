# Battle 33 investigation: marines against zealots (Q0007)

This is the evidence file of [the battle 33 investigation plan](../plans/2026-11-04-zealot-marine-investigation.md),
which answers the owner's request in [Q0007](../questions/open/0007-combat-baselines.md): find out why published
StarCraft II sources say zealots win this fight, and why our battle 33 leaves 6 to 13 of 24 marines alive. The plan's
D-5 gives this file seven sections. **Only section 1 (WP1, reference research) is written so far.** WP2 to WP5 add
sections 2 to 7 below it. This file changes no code, catalog value or expected result.

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
