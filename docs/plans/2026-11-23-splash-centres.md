# Blueprint: Where an Area Lands — the Baneling's Burst, the Siege Tank's Large Targets and the Planetary Fortress (B-83, B-84, B-169)

This plan settles two open splash rows of the bug register against StarCraft II 5.0.14 and one found while checking
them:

- **The Baneling's burst is centred on the Baneling (B-83).** Its damage effects name `ImpactLocation CasterUnit`,
  so the 2.2 area is searched around the Baneling itself, and the attacked unit is struck by that area like every
  other unit in it. Ours strikes the target directly and then searches around the target's near edge, which lies
  0.375 to 1.125 in front of the Baneling.
- **The Siege Tank's sieged shot has no area against a target of radius 1.25 or more (B-84).** The weapon's effect is
  a switch: a target with `Radius` below 1.25 gets the splashing `CrucioShockCannonBlast`, any other the
  `CrucioShockCannonDirected` with no `AreaArray`. Ours splashes around every target. No shipped ground unit is that
  large, so the change acts on structures: a Supply Depot and anything larger takes the shot alone.
- **The Planetary Fortress splashes (B-169, registered here).** `TwinIbiksCannon` has three zones (0.5, 0.8, 1.25 at
  100, 75 and 37.5 %). Our Planetary Fortress analogue has none.

The rules become two capability fields on `SplashSpec`, read from the catalog: where the area is centred
(`impact`: on the target, the engine default, or on the attacker) and below which target radius the area opens at
all (`target_radius_below`). No code names a unit (charter §3.3, C5). The neighbouring area rules were checked
(§2.4): one more difference is registered and deferred (B-170: area damage keeps to the impact's cliff level).

A full plan is required (charter §6): a new rule in the simulation, a change of hashed encoding (checksum schema),
and data that moves battle outcomes (matrix battles 11a, 11b and 32 contain Banelings).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`e7153b8b` (checksum schema 35).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0063: a zone reaches a unit's edge; `OffsetByUnitRadius` centres the area on the target's edge facing the attacker | Answered (options 3 and 4); kept for target-centred areas | D-1, D-2 |
| Q0008: where the specification is silent, search for what StarCraft II does | Standing rule; the Baneling's offset is not sourced | D-2 |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.4, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6, §7 |
| **Draft, not filed:** "Is the Baneling's 2.2 burst measured from its centre?" | Proposed for the orchestrator to file (§15); default (its centre) followed meanwhile | D-2 |

---

## 1. Summary

| | Today (`e7153b8b`) | After this plan |
|---|---|---|
| Baneling (`volatile_tick`) | Direct strike on the target, then the 2.2 area around the target's edge facing the Baneling, sparing the target | One 2.2 area around the Baneling's centre; the target is struck by it, once, like any unit in it; no direct strike |
| Sieged Siege Tank (`railgun_crawler`, `siege_mode`) | Area around every target | Area only around a target whose `collision_radius` < 1.25; the target's own damage unchanged |
| Planetary Fortress (`planetary_bulwark_dome`) | No splash | Three zones, 0.5 / 0.8 / 1.25 at 1 / 0.75 / 0.375, centred on the target, enemies only |
| `SplashSpec` | zones, shape, friendly fire, offset | + `impact` (`Target` default, `Attacker`) and `target_radius_below` (`None` default) |
| Hashed state | — | The two fields in the splash encoding (hashed inside a projectile's `WeaponSpec`); schema **next free at merge** (36 unless another branch takes it) |
| Worlds that move | — | WP2: matrix battles 11a, 11b, 32 (the only ones with Banelings); WP3, WP4: none measured (§7) |
| Size | — | M: five packages, WP1 M, the rest S or XS |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; **verify** not confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at `~/scratch/combat_wp3_data/`, files `EffectData.xml`, `WeaponData.xml`, `ValidatorData.xml`, `UnitData.xml`. Chain `core → liberty → swarm → void → voidmulti` (catalog-time audit §2.2); `libertymulti`, `swarmmulti` and `balancemulti` are not in it. `liberty EffectData:N` is line N of that layer's file | Every value below [DATA] |
| S2 | `CEffectDamage` defaults, `core EffectData:60–69`: `ImpactLocation TargetUnitOrPoint` (:66), `SearchFlags SameCliff 1` (:67), `LaunchLocation SourceUnit`; `DU_WEAP`, `core EffectData:120–129`: `SearchFlags OffsetByUnitRadius 1` (:128) | An area is centred on the impact unit or point unless the effect says otherwise |
| S3 | The Baneling's weapon `VolatileBurst`, `liberty WeaponData:463–475`: `Range 0.25`, `Options Melee`, effect = its id (the `CWeaponLegacy` default; `swarm WeaponData:197`, `void :198` and `voidmulti :365` change only `Disabled` and its marker); the set `VolatileBurst`, `liberty EffectData:1739–1747`: `BanelingDontExplode`, `SuicideTargetFriendlySwitch`, `VolatileBurstU2`, `VolatileBurstU`, `Suicide`; `voidmulti EffectData:2333–2336` puts `BanelingVolatileBurstDirectFallbackSet` at index 4, before `Suicide` | The burst is two damage effects and the suicide |
| S4 | `VolatileBurstU`, `liberty EffectData:1753–1765`: `SearchFilters Ground;Player,Ally,Structure,…` (:1756), `AreaArray Radius 2.2 Fraction 1` (:1757), `ExcludeArray Outer` (:1758), `ImpactLocation CasterUnit` (:1759); amounts `voidmulti EffectData:1025–1028` (16, +19 Light) | The unit burst: centred on the Baneling, enemy ground non-structures within 2.2 |
| S5 | `VolatileBurstU2`, `liberty EffectData:1052–1064`: `ImpactLocation CasterUnit` (:1055), `AreaArray 2.2 1`, `ExcludeArray Outer`, `SearchFilters Ground,Structure;Self,Player,Ally,…`, `Amount 80`, `ArmorReduction 0` | The structure burst (B-82), also centred on the Baneling |
| S6 | `BanelingVolatileBurstDirectFallbackSet`, `voidmulti EffectData:2327–2358`: validator `CliffLevelNotEqual` (`voidmulti ValidatorData:894–898`, target vs. origin unit), effects that copy `VolatileBurstU` and `VolatileBurstU2` with the area removed, `ImpactLocation TargetUnit`, `SameCliff 0` and `noMarkers` | The target is struck directly only when it stands on another cliff level than the Baneling. On the same level it is struck by the area alone, and `noMarkers` keeps it from being struck twice [DATA]; so a target on the same level is not struck directly [INF] |
| S7 | `ExcludeArray Outer`, read across its uses in `liberty` (EffectData:1061, 1356, 1758, 2198, 2217, 2543): the Archon's and Planetary Fortress' blasts (target-centred, target struck directly), the Glaive bounce searches (exclude the unit just hit), the Baneling's bursts | "Outer" is the effect's impact unit: the target for a target-centred area, the Baneling for its bursts [INF, **verify**]. Either way the Baneling is not struck by its own burst |
| S8 | The Siege Tank: `liberty WeaponData:255–265` (`CrucioShockCannon`, `Range 13`, `MinimumRange 2`); `swarm WeaponData:169–171` sets its `Effect` to `CrucioShockCannonSwitch` | The 5.0.14 weapon effect is the switch, not `liberty`'s set |
| S9 | `CrucioShockCannonSwitch`, `swarm EffectData:2183–2187`: case 0 `TargetRadiusSmall` → `CrucioShockCannonBlast`, case 1 `TargetRadiusLarge` → `CrucioShockCannonDirected`; `voidmulti EffectData:739–742` replaces case 1 with `IsSupplyDepotLowered` → `Blast` and appends `TargetRadiusLarge` → `Directed` | First matching case wins: small → area; lowered Supply Depot → area; large → no area |
| S10 | `TargetRadiusLarge` / `TargetRadiusSmall`, `liberty ValidatorData:824–833`: `CValidatorUnitCompareField`, `Field Radius`, `GE` / `LT` `1.25` | The threshold is 1.25, "or more" goes to `Directed` |
| S11 | `CrucioShockCannonBlast`, `liberty EffectData:1668–1681`: zones 0.4687 1, 0.7812 0.5, 1.25 0.25; `ExcludeArray Target`; `OffsetByUnitRadius 0`; `SearchFilters Ground;Self,Missile,Stasis,Dead,Hidden,Invulnerable` (allies included). `CrucioShockCannonDirected`, `:1682–1690`: no `AreaArray`. `swarm EffectData:2197–2201`, `:2235–2239` clear both effects' radius validators (the switch decides); `voidmulti EffectData:669–676` gives both 40, +30 Armored | The target takes the same strike either way; only the area differs |
| S12 | `TwinIbiksCannon`, `liberty EffectData:2533–2546`: zones 0.5 1, 0.8 0.75, 1.25 0.375 (:2540–2542); `ExcludeArray Outer` and `Target` (:2543–2544); `OffsetByUnitRadius 0` (:2545); `SearchFilters Ground;Self,Player,Ally,…`; `swarm EffectData:309–311` changes only `Kind`. Weapon `liberty WeaponData:133–141` (effect = its id), on the Planetary Fortress `liberty UnitData:10718` | The Planetary Fortress' splash, enemies only, centred on the target |
| S13 | Search filters of the other modelled areas: Liberator `void EffectData:2940`, Hellion `liberty :2611`, Colossus `liberty :2414`, Lurker `swarm :3025`, Ultralisk `liberty :2287`, `swarm :2279` | All exclude `Player,Ally`; none excludes `Massive` or `Structure`; the Ultralisk's also `Neutral` |
| S14 | `SearchFlags SameCliff`: 1 on the `CEffectDamage` default (S2) and on `KaiserBladesSearch` (`liberty EffectData:2292`); 0 on the friendly-target and fallback variants of the Baneling (`liberty :1076`, `:1777`; `voidmulti :2341`, `:2352`) | Damage-effect areas keep to a cliff level (meaning **verify**), B-170 |
| S15 | Plan [`2026-11-05-sc2-deviation-fixes`](2026-11-05-sc2-deviation-fixes.md) Amendment 12 and evidence §8.1 (where B-83 and B-84 were found); [Q0063](../questions/closed/0063-splash-distance-rule.md) | The rules this plan keeps |

### 2.2 The rules that follow

1. **An area is centred where its damage effect impacts.** By default that is the target unit (S2), so the target's
   centre, moved to its edge facing the attacker when the effect keeps `OffsetByUnitRadius` (Q0063). An effect with
   `ImpactLocation CasterUnit` is centred on the attacker (S4, S5) [DATA].
2. **The impact unit takes the effect directly and is excluded from the search; every other unit in reach takes it
   through the area.** The Siege Tank's and Planetary Fortress' targets are struck directly (`ExcludeArray Target`,
   S11, S12). The Baneling is its own impact unit; its attacked unit is reached through the area, and a direct strike
   happens only across a cliff (S6) [INF from S6, S7].
3. **The Baneling's target is always inside the burst.** The weapon releases at an edge distance of at most
   0.25 + 0.5 (our `RELEASE_RANGE_SLOP`, B-6), so the target's edge is at most 0.375 + 0.75 = 1.125 from the
   Baneling's centre, inside 2.2. So the target's damage is unchanged (fraction 1); what changes is who else is
   reached [INF].
4. **The sieged shot opens its area only for a target whose radius is below 1.25** (S9, S10); a lowered Supply Depot
   is the one exception (S9). The struck target's damage is the same in both cases (S11) [DATA].
5. **The Planetary Fortress' shot splashes** at the zones of S12, from the target's centre, enemies only [DATA].

### 2.3 Values

| Value | Game data | Our units | Shipped | Source |
|---|---|---|---|---|
| Baneling area centre | the Baneling (`CasterUnit`) | the Baneling's position at release | the target's near edge (`offset_by_unit_radius: true`) | S4, S5 |
| Baneling area | 2.2, fraction 1, enemies | 2.2, 1, `friendly_fire: false` | the same | S4 |
| Siege Tank area condition | `Radius` < 1.25 | `collision_radius` < 1.25 | none | S9, S10 |
| Planetary Fortress zones | 0.5 1, 0.8 0.75, 1.25 0.375; offset 0; enemies | the same | `splash: None` | S12 |

Which radius the condition reads: the validator reads the unit's `Radius` field. Our `collision_radius` is that
value for every unit, and for a structure its engine radius `max(Radius, shape radius)` (plan
`2026-11-18-structure-radius`). For the 49 shipped structures both readings fall on the same side of 1.25 (counted
from the `structures.ron` comments on `e7153b8b`): the 11 rows of engine radius 1.125 or less (Sensor Tower, Spore
Crawler, Pylon, Missile Turret, Photon Cannon, Spine Crawler, Shield Battery, Tech Lab, Reactor, Spire, Greater Spire;
`Radius` 0.75 to 1.0) are below; the Supply Depot (1.25, `Radius` 1.25) and the 37 rows of 1.5 or more (`Radius`
1.25 to 2.5) are not. No shipped ground unit reaches 1.25 (largest: 1.0, the Thor, Ultralisk and Colossus
analogues). So the choice changes nothing today; the
plan reads `collision_radius`, the one radius every rule uses, and records the reading as **verify** (D-3).

### 2.4 Neighbouring rules: in or out

| Rule | Verdict | Why |
|---|---|---|
| B-83 Baneling centre | **In** (D-1, D-2, D-4) | |
| B-84 Siege Tank's large targets | **In** (D-3, D-4) | |
| Planetary Fortress splash (B-169, new) | **In** (D-5) | A clear [DATA] gap found on the patch 1.1 list of Q0063; data only, using the existing rule |
| The lowered Supply Depot's exception (S9) | Out (§14 F-1) | No lowered depot exists (no `lower` mode in `structures.ron`); the hook is recorded |
| Area damage keeps to a cliff level (`SameCliff`, S14), and the Baneling's cross-cliff fallback (S6) — **B-170, registered here** | Deferred (§14 F-2) | A rule for every damage area, whose meaning for ramps and air targets is not in the data (**verify**); without it the fallback is moot, because our burst reaches its target on any level (rule 3) |
| The Baneling ordered onto a friendly unit (`SuicideTargetFriendlySwitch`, `liberty EffectData:1390–1395`) | Out (§14 F-3) | Whether our orders let a Baneling attack a friend is **verify**; the switch is its own small rule |
| The Baneling's manual Explode and its structure-attack toggle (`BanelingExplode`, `liberty BehaviorData:1141`; `VolatileBurstBuilding`, `swarm WeaponData:200–213`) | Out (§14 F-4) | Abilities not modelled (deviation-fixes WP18) |
| Friendly-fire flags of every modelled area | **Conforming** | Siege Tank allies included (S11, ours `true`); every other area excludes `Player,Ally` (S4, S12, S13, ours `false`) |
| `Massive` / `Structure` exclusions | **Conforming** | No modelled area excludes `Massive` (S13). Only the Baneling's unit burst excludes structures, and B-82's `structure_strike` gives them `VolatileBurstU2`'s 80 instead (S5) |
| B-19 (Ultralisk arc), B-18 (one search per strike) | Out, open | Their own rows; untouched |
| Thor's Explosive Payload splash, Archon | Out | Mode and unit not modelled |

---

## 3. Diagnosis

All in `crates/sim_core/src/combat_resolution.rs` (tick phase 7), `crates/sim_types/src/weapon.rs` and the catalog.

1. **B-83.** `resolve_instant_shot` always calls `apply_weapon_hit(source, shot.target)` (a direct strike, the
   target's retaliation and weapon hit effects) and then `apply_splash(source, splash, hit, Some(shot.target))`.
   `apply_splash` centres the search on `hit.position`, moved by `direction * hit.radius` towards the attacker because
   the Baneling row sets `offset_by_unit_radius: true` (`assets/catalog/units.ron`, `volatile_tick`), and
   `strike_area` spares `area.primary`. `dispatch_shot` then zeroes the attacker (`suicide_on_attack`). Nothing in
   `SplashSpec` can say "centre on the attacker"; the only attacker-anchored form is a line's
   `SplashLineOrigin::Attacker`. So the fix is **code and data**: a field and its rule, then the Baneling's row.
2. **B-84.** `apply_splash` searches for every hit; `SplashSpec` has no condition on the target. The sieged
   `ground_weapon_override` of `railgun_crawler` splashes around anything it hits, structures included. **Code and
   data**: a field and its test in `apply_splash`, then the tank's row.
3. **B-169.** `planetary_bulwark_dome`'s `ground_weapon` authors `splash: None` (`assets/catalog/structures.ron`).
   **Data only**, plus its presentation block.
4. **B-170.** `strike_area` reaches every damageable unit within the zones, whatever its cliff level. Recorded only.

---

## 4. Design decisions

### D-1. Where an area is centred: `SplashSpec::impact` (B-83 rule)

**Type** (`sim_types/src/weapon.rs`, exported from `lib.rs`):

```rust
/// Where a weapon's area is centred: `StarCraft` II's `ImpactLocation` of its damage effect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplashImpact {
    /// On the struck target (the engine default, `TargetUnitOrPoint`, `core` EffectData:66): the
    /// target is struck directly and the area spares it.
    #[default]
    Target,
    /// On the attacker at release (`CasterUnit`, the Baneling's bursts): no direct strike; the
    /// target is struck by the area like any other unit in it.
    Attacker,
}
```

`SplashSpec` gains `#[serde(default)] pub impact: SplashImpact`, and its doc replaces "searched around one point" with
the two centres and drops B-83 from what is not modelled.

**Rule** (`combat_resolution.rs`):

```text
resolve_instant_shot(shot):
    if shot.weapon.splash is Some(splash) and splash.impact == Attacker:
        hit := area_at_attacker(shot)          # position = shot.origin, radius = 0,
                                               # plane = target's mover type, else the attacker's,
                                               # angle = hit_angle(origin, target position), else the attacker's heading
        apply_splash(source, splash, hit, primary = None)
        emit_instant_hit_visual(shot, hit) only if visual_kind is a delivery (the Baneling's is None: nothing)
        return
    as today
```

- **No direct strike, so no weapon hit effects.** `run_weapon_hit_effects` already runs on direct hits only ("the
  game's impact set runs on the missile's target alone"); an attacker-centred weapon makes none. No shipped
  `WeaponHitEffect` unlock names such a weapon (the one shipped unlock is the Marauder's Concussive Shells); WP1's
  test pins it, and validation refuses it: where the upgrade catalog's unlocks are resolved against the unit catalog
  (**verify** the site; `UnknownWeaponHitEffect` is the runtime diagnostic), an unlock whose archetype's every weapon
  has an attacker-centred area is a `SimCoreError::CatalogInvalidValue`. If no load-time site resolves unlocks
  against archetypes, the implementer adds the check beside the existing slug resolution and names the file in the
  hand-back.
- **Retaliation is unchanged in kind:** `strike_area` already runs each struck unit's retaliation against the
  attacker, so the target retaliates as before (a dead Baneling cannot be answered either way).
- **The area lands whether or not the target still stands.** Today a target removed before dispatch
  (`entities.get` is `None`) cancels the whole shot; for an attacker-centred area the burst does not depend on it, as
  in the game (the weapon fired). A target that died earlier this phase is skipped by `is_damageable` as now.
- **Validation** (`catalog/validation.rs`, `SplashSpecDef::validate` and the weapon's validate):
  - `impact: Attacker` with `shape: Line` is refused ("a line's searches are placed by its origin; `impact` applies
    to a circle").
  - `impact: Attacker` with `offset_by_unit_radius: true` is refused (D-2).
  - `impact: Attacker` on a weapon with `projectile_speed` is refused ("an area at the attacker needs an instant
    weapon"): no data needs it, and a missile's impact at its launcher has no meaning here.
- **Invariant 1 of the module** (every area through `apply_splash`) holds; the module doc's invariant list gains
  "an attacker-centred area is the only damage of its shot".

### D-2. The Baneling's area is centred on its centre (the offset, **verify**)

`VolatileBurstU` inherits `OffsetByUnitRadius 1` from `DU_WEAP` (S2). Q0063's rule moves a target-centred area to the
impact unit's edge facing the attacker; for the Baneling the impact unit **is** the attacker, so that direction does
not exist. Three readings are possible: (a) no offset, the Baneling's centre (the register's text of B-83, and the
only symmetric one); (b) the edge facing its target, 0.375 forward; (c) the search radius measured from the
Baneling's edge, an effective 2.575 all round. None is sourced.

**Decision: (a).** The Baneling row authors `offset_by_unit_radius: false` with a comment giving the inherited flag,
why it is cleared, and this plan; validation refuses the combination (D-1), so no row can author an offset the
engine does not apply. The draft owner question (§15) asks for an editor check; switching to (b) or (c) is a data or
one-line change.

### D-3. When an area opens: `SplashSpec::target_radius_below` (B-84 rule)

**Field:** `#[serde(default)] pub target_radius_below: Option<SimScalar>`: "The area opens only when the struck
target's collision radius, in world units, is below this; a target of this radius or more takes the strike alone.
`StarCraft` II's `CEffectSwitch` between a splashing and a non-splashing damage effect on
`TargetRadiusSmall`/`TargetRadiusLarge` (`liberty` ValidatorData:824–833). `None`: the area always opens."

**Rule:** first statement of `apply_splash`:

```text
if let Some(limit) = splash.target_radius_below:
    if hit.radius >= limit: return            # no search, no SplashVisual, no pending line
```

- `hit.radius` is `ResolvedHit::radius`: the struck target's `collision_radius` (a unit's `Radius`, a structure's
  engine radius, §2.3), and zero for a projectile that lands on a point, which therefore splashes, the only reading
  that keeps `ImpactLastKnownPosition` meaningful (no shipped weapon has both).
- The direct strike is untouched: the target's damage is the same with and without the area (S11).
- **Validation:** positive and finite when present (`positive`, the existing helper), any shape, either impact.
- The game's lowered-Supply-Depot case (S9) has no counterpart (§14 F-1).

### D-4. Data (B-83, B-84)

- `volatile_tick` weapon `splash`: `impact: Attacker`, `offset_by_unit_radius: false`, the comment of D-2, citing
  S4, S5 and `ExcludeArray Outer`. Zones, friendly fire, `structure_strike` and `suicide_on_attack` unchanged.
- `railgun_crawler` mode `siege_mode` `ground_weapon_override` `splash`: `target_radius_below: Some(1.25)`, citing S8
  to S10. Tank mode has no splash.
- **Zero recompilation** (charter §3.3): a catalog copy with the condition removed, or with the Baneling's impact set
  back to `Target`, changes the outcome in the same binary (`runtime_catalog_tests`, WP2 and WP3).

### D-5. The Planetary Fortress splashes (B-169)

`planetary_bulwark_dome` `ground_weapon` `splash: Some((zones: [(radius: 0.5, fraction: 1.0), (radius: 0.8,
fraction: 0.75), (radius: 1.25, fraction: 0.375)], friendly_fire: false))`, offset and impact at their defaults,
citing S12. Its effects block names a splash look (D-8). The rest of the weapon row is not re-audited here.

### D-6. Hashed state and checksum

- `fields!(SplashSpec { …, offset_by_unit_radius, impact, target_radius_below })` in `checksum/encoding.rs`, with a
  `Canonical` for `SplashImpact` (0 `Target`, 1 `Attacker`); `Option<SimScalar>` uses the existing encoding. A
  `WeaponSpec` is hashed inside every in-flight `ActiveProjectile`, so both fields are decision state.
- **Schema: next free at merge** (36 on `e7153b8b`; 37 if another branch takes 36). The doc comment of
  `CHECKSUM_SCHEMA_VERSION`: "Schema N adds each splash's impact (0 target, 1 attacker) and its target-radius
  condition (an `Option` of a scalar), after `offset_by_unit_radius` (plan `2026-11-23-splash-centres` D-6)."
- WP1 claims the number in its own commit; the merge takes the next free one and re-records then (§5).

### D-7. What the simulation publishes

- An attacker-centred area publishes its `SplashVisual` at the attacker's position (the search point), with the
  zones as today, and no delivery event (the Baneling's `visual_kind` is `None`, B-123).
- An area that does not open publishes no `SplashVisual`; the shot's own delivery event (`HitscanTracer` for the
  tank) is unchanged.
- No event type or field changes, so the client's event contract is untouched; D-8 lists what the client shows.

### D-8. Client consequences

1. **The Baneling's `acid_burst`** is laid at the `SplashVisual` centre, now the Baneling's position instead of a
   point 0.375 to 1.125 in front of it. That is the correction itself; the look sizes from the published zones and
   needs no change. The Baneling analogue has no battle media, so it is seen in the lab, the tests and the frame
   budget only (`DESIGN.md` Effect presentation row). `effect_lab_blocks.ron` and `effect_catalog_tests.rs` name the
   look, not its position (**verify** in WP2 that no client test asserts the burst's position against the target).
2. **The sieged shot at a large target publishes no `SplashVisual`,** so `shell_impact` no longer plays on a Supply
   Depot or larger. The siege block names only `travel` and `splash`, so such a shot would show the tracer and no
   landing. **Decision:** the siege-mode entry of `railgun_crawler`'s effects block gains `impact: Some("cannon_hit")`
   (the tank mode's landing), and `effect_catalog_tests::every_shipped_weapon_names_the_looks_of_the_event_it_publishes`
   gains the rule "a weapon whose area is conditional (`target_radius_below`) names an impact, so a shot whose area
   does not open still shows a landing". A small target then shows both looks, as the game's sieged shot shows its
   impact model and its blast. The alternative, an impact only when the area does not open, needs a new event and is
   recorded in §14 F-5.
3. **The Planetary Fortress' block** gains `splash: Some("shell_impact")` (the existing ground blast whose
   `Critical` layers bind the published zones); `every_shipped_splash_weapon_with_a_block_names_a_splash_effect_bound_to_its_extent`
   and "a splash weapon, and only one, names its splash" then hold (**verify** that the test walks structure
   archetypes; it builds its tables from `sim_fixtures::shipped_catalogs().units`).
4. **Pictures.** `weapon_battle_gpu_tests` pictures each weapon family; the siege family gains an impact look
   (**verify** whether it compares against a stored image or only checks spawned looks; if stored, the re-baseline is
   reviewed in WP3 as a visual change).

### D-9. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | Register **B-169** ("The Planetary Fortress analogue has no splash; 5.0.14's `TwinIbiksCannon` splashes 0.5 / 0.8 / 1.25 at 100 / 75 / 37.5 % (`liberty` EffectData:2540–2542)") and **B-170** ("Area damage reaches units on every cliff level; 5.0.14's damage-effect searches set `SameCliff` (`core` EffectData:67), and the Baneling strikes a target on another level directly (`voidmulti` EffectData:2327–2358); the flag's meaning on ramps and for air targets is not in the data"); B-83, B-84 point to this plan | WP0 |
| `DESIGN.md` §4 | B-83, B-84, B-169 fixed with commits and evidence; B-170 deferred with its plan pointer; the Combat row: "splash centred on the target or, by data, on the attacker, and opened by data only below a target radius" | WP5 |
| `docs/03-sc2-mechanics-reference.md` | The effect-inventory row 10 (Baneling) and any splash text: centre on the Baneling | WP5 |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md` | Amendment 2's rows B-83, B-84: "fixed by plan `2026-11-23-splash-centres`" | WP5 |
| `docs/plans/2026-11-09-effect-visualization.md` §12 | B-83: "taken up by plan `2026-11-23-splash-centres`" | WP5 |
| New `docs/evidence/<date>-splash-centres.md` | §7's comparisons per package, the traced bursts | WP0 creates; WP1 to WP5 append |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Done` (WP5) | plan, WP1, WP5 |

---

## 5. Determinism and checksum

- **Fixed point only.** The rule compares `SimScalar`s and moves a search centre to a `SimVec2` the shot already
  holds. The two authored values are `f32` in the catalog definitions and converted with `SimScalar::from_f32_lossy`
  at load, like every splash value (C7; `catalog.rs`'s and `validation.rs`'s float allowances rise by the new tokens,
  in `crates/workspace_checks/tests/charter.rs`, with the reason "catalog decimals converted at load").
- **No new entity state.** The two fields live in `WeaponSpec`, hashed in projectiles (D-6).
- **Goldens.** Neither replay contains a Baneling, a sieged tank or a Planetary Fortress (`golden_replay.ron`: marines,
  Roach analogues, zealots, three tanks that never siege, two Command Center analogues; `golden_replay_orders.ron`:
  marines, an Observer analogue, Roach analogues, zealots). So behaviour cannot move; only WP1's schema byte moves
  every checkpoint of both, and the sandbox checksum.
- **WP1's proof** (the procedure of the regeneration and production-queue plans):
  1. With the two fields left out of the encoding and the schema at 35, the tree reproduces every recorded checksum
     of both goldens, and `DEFAULT_SANDBOX_CHECKSUM`, on native debug and release and on `wasm32-wasip1` release and
     debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`).
  2. Then the fields go in, the schema becomes the next free number, and the checksums are re-recorded under the
     header "Re-recorded for plan 2026-11-23-splash-centres WP1 (checksum schema 35 -> N): each splash also encodes
     its impact and its target-radius condition, so every checkpoint changed. No behaviour did: with the two fields
     left out of the encoding at schema 35, both fixtures reproduce as recorded for <the previous header>." The four
     builds reproduce the new values.
  3. If `main` takes the number first, the merge repeats step 2 with the next one.
- **WP2 to WP4 leave both goldens and the sandbox unchanged.** A move is traced to its first checkpoint and entity;
  an unexplained one stops the package.
- **wasm.** No dependency changes. **Cost:** one comparison per splash and one branch per instant shot; the
  throughput bench is unaffected (its scenarios are checked in WP2's gate as usual).

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure of a seed. Constructed tests read every radius, zone and range from the catalog and find
weapons by property (`impact == Attacker`, `target_radius_below.is_some()`, a structure with a ground weapon and
splash), never by slug.

- **P-1 (attacker-centred area).** For every shot of a weapon with `impact: Attacker`: the units struck are exactly
  the damageable units of the shot's plane, not the attacker, not excluded by friendly fire, whose edge lies within
  the zones measured from the attacker's position at release; each is struck once at its zone's fraction; the target
  takes no other strike; units outside are untouched.
- **P-2 (the Baneling's target is always struck).** For every Baneling burst in every test world, the target, if
  damageable, takes exactly the area's full strike (`raw × 1`, or the structure strike), equal to today's direct
  strike against the same target.
- **P-3 (the condition).** For a weapon with `target_radius_below = L`: a shot at a target of radius `r < L` strikes
  as today (target directly, area around it); at `r >= L` (boundary included) it strikes the target alone, with the
  same damage, and publishes no `SplashVisual`. Checked at `r = L − 2⁻¹⁶`, `L`, `L + 2⁻¹⁶`.
- **P-4 (the shipped classification).** Every shipped structure with `collision_radius < 1.25` is splashed around by
  the sieged tank; every other is not; no shipped ground unit has a radius of 1.25 or more (so no unit loses the
  area).
- **P-5 (the Planetary Fortress).** Its splash has the zones of S12, centred on the target, enemies only, the target
  struck once.
- **P-6 (confinement).** WP1 changes no world (all of §7 byte-identical; goldens by the schema byte only). WP2 changes
  only worlds that contain a weapon with `impact: Attacker` (the matrix battles 11a, 11b, 32); WP3 and WP4 change no
  measured world (§7).
- **P-7 (zero recompilation).** Changing `impact` or `target_radius_below` in a catalog copy changes the outcome in the
  same binary.
- **P-8 (direction of the Baneling's move, a prediction recorded before measuring, not a threshold).** The area moves
  back by 0.375 to 1.125 along the attack, so it loses reach beyond the target and gains it behind the Baneling,
  where its own side stands. In battle 32 (Banelings rolling into a 5 × 6 marine block) the number of marines struck
  per burst should fall, marine survivors should rise and the Banelings' win count should not rise. In 11b the
  Marauder is the only enemy and always inside the burst (§2.2 rule 3), so 11b is expected byte-identical. In 11a the
  five marines spawn in a column 0.8 long, so most bursts should reach the same marines under both centres; 11a is
  expected identical or nearly so, any difference traced to a marine beyond the target. A result against this prediction is not "accepted as measured":
  it is traced (§8 rule 3).

---

## 7. Worlds that move

**Baseline** (`~/scratch/structure-attributes/world-wp1/matrix/matrix.md`, tree `5e3ac1ea`, behaviour-identical to
`e7153b8b`; 20 seeds each, 4 of record): 11a blue (Banelings) 0 wins, red (marines) 9, both gone 11, survivors
0.00 – 2.25; 11b red 20, survivors 0.00 – 1.00; 32 blue (marines) 4, red (Banelings) 15, both gone 1, survivors
0.75 – 0.90, end tick 53.8. No battle of the matrix places a structure; 3, 37a/b and 39a/b have tanks, sieged only
against units.

**Commands** (the regeneration plan's; binaries built once per tree, run from that tree so its assets are read;
outputs under `~/scratch/splash-centres/`; scripts copied from `~/scratch/structure-attributes/scripts/`, with one
addition to `measure.sh`: `route_trace --scenario scenarios/showcase_effects.ron` on its default seed and seeds 1 to
8 for 6,000 ticks, the one shipped scenario with sieged tanks and town halls):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
mkdir -p ~/scratch/splash-centres/bin-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} ~/scratch/splash-centres/bin-<wp>/
~/scratch/splash-centres/scripts/measure.sh ~/scratch/splash-centres/bin-<wp> <tree> ~/scratch/splash-centres/world-<wp>
~/scratch/splash-centres/scripts/compare.sh ~/scratch/splash-centres/world-wp0 ~/scratch/splash-centres/world-<wp>
```

Run in the background, one at a time across worktrees (charter §7). The client skirmish (`skirmish_match_tests`) is
compared by its stage log, as in the production-queue plan (`client.sh`).

| World | WP1 | WP2 (Baneling) | WP3 (Siege Tank) | WP4 (Planetary Fortress) |
|---|---|---|---|---|
| Matrix (seeds of record and 16) | byte-identical | **11a, 11b, 32 may move** (P-8); every other battle byte-identical | byte-identical: no structure in any battle, every sieged target a unit of radius ≤ 1.0 | byte-identical |
| Battles 31 (100 seeds), 33 (64 seeds) | byte-identical | byte-identical (no Baneling) | byte-identical | byte-identical |
| Showcase, default seed and 1–8 | byte-identical | byte-identical (`battle_automaton`: no Baneling) | byte-identical (no tank) | byte-identical (no Planetary Fortress) |
| Showcase effects, default seed and 1–8 | byte-identical | byte-identical | byte-identical **unless** a sieged tank fires at a Command Center analogue (radius 2.75); then the change begins on that shot and is traced to it | byte-identical |
| Goldens, sandbox | schema byte only (§5) | unchanged | unchanged | unchanged |
| Client skirmish stage log | identical | identical (`standard_automaton` builds no Baneling) | identical (**verify**: the pilot builds no tank) | identical (**verify**: no Planetary Fortress) |
| Constructed worlds (§12) | P-1, P-3, P-7 on scratch weapons | P-1, P-2, P-7 | P-3, P-4, P-7 | P-5 |

**What WP2 records for 11a, 11b and 32** (in the evidence, all 20 seeds, before and after): wins, survivors, end tick,
and, for the seed of record of each, one burst traced with its centre, the set of units struck before and after, and
their distances. A matrix expectation that fails after WP2 is re-recorded in the same package with that trace as its
evidence (the cause is B-83's sourced mechanism, the deviation-fixes stop rule's exception), and its claim is
re-read: if "Banelings trade far above their cost" (32) no longer describes the result, the claim text says what the
battle now shows and the evidence says why.

---

## 8. Stop rules

1. **WP1 is latent.** Every world of §7 byte-identical; goldens moved by the schema byte only, proved (§5).
2. **Each data package moves only its worlds** (P-6). A difference in any other row of §7 is a defect of the package,
   not a re-record.
3. **A move against P-8, or a move in 11a, 11b or 32 whose first differing tick is not a Baneling burst**, stops WP2
   until traced. If the trace shows a second cause (for example a Baneling's approach), it is registered as a bug and
   WP2 lands only if the Baneling's change is separable from it.
4. **No tuning constant.** Every number is the data's (S4, S10, S12) or an existing engine rule's.
5. **A golden move in WP2 to WP4** follows §5's tracing or stops the package.

---

## 9. Client consequences

D-8. Code: none planned. Data: `units.ron` (siege entry's `impact`) and `structures.ron` (the Planetary Fortress'
`splash`) effects blocks. Tests: `effect_catalog_tests.rs` gains D-8 item 2's rule; `effect_snapshot_tests.rs`
(`sim_core`) `every_shipped_weapon_publishes_its_delivery_or_splash` and `each_shipped_splash_weapon_publishes_its_zones`
fire each shipped weapon at a unit target (**verify**: if either fires the sieged tank at a structure, it is
re-derived from D-3 to pick a small target, or to expect no `SplashVisual` for a large one). The package gate runs
the client's GPU and lab tests that its path rules select.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP5 | D-9 |
| `docs/plans/README.md` | plan, WP1, WP5 | Row |
| `crates/sim_types/src/weapon.rs`, `crates/sim_types/src/lib.rs` | WP1 | `SplashImpact`, the two fields, docs, unit tests of the literal constructors in the module |
| `crates/sim_core/src/catalog.rs` | WP1 | `SplashSpecDef` fields and conversion |
| `crates/sim_core/src/catalog/validation.rs` | WP1 | D-1, D-3 validation and unit tests |
| The upgrade-unlock resolution site (**verify**: `crates/sim_core/src/catalog/` or `upgrades.rs`) | WP1 | D-1's refusal |
| `crates/sim_core/src/combat_resolution.rs` | WP1 | D-1 and D-3 rules, module doc, `area_at_attacker` |
| `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/checksum.rs` | WP1 | D-6 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `sim_fixtures`' `DEFAULT_SANDBOX_CHECKSUM`; the encoding unit tests' schema byte | WP1 | §5 |
| Test files with `SplashSpec { … }` literals (`barrier_tests`, `checksum_state_tests`, `combat_correctness_tests`, `combat_retaliation_and_projectile_tests`, `damage_and_shields_tests`, `spells_and_abilities_tests`, `transport_ownership_tests`, `weapon_catalog_rules_tests`) | WP1 | The two fields at their defaults (mechanical) |
| `crates/sim_core/tests/splash_centre_tests.rs` (new, `//!` doc) | WP1 to WP4 | §12 |
| `crates/workspace_checks/tests/charter.rs` | WP1 | C7 allowances (§5) |
| `crates/gate/suites.ron` | WP1, only if needed | The new target (**verify**: an uncovered target may fall to the crate's default class) |
| `assets/catalog/units.ron` | WP2, WP3 | D-4; the siege entry's `impact` look (D-8) |
| `crates/sim_core/tests/weapon_catalog_rules_tests.rs` | WP2 to WP4 | `SC2_SPLASH_OFFSET` (Baneling row → `false`, D-2; Planetary Fortress row), `b16_splash_zones_follow_starcraft` (Planetary Fortress row), the new `splash_impact_and_condition_follow_starcraft` |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP2, only by §7's rule | Re-recorded expectations with evidence |
| `assets/catalog/structures.ron` | WP4 | D-5 and its `splash` look |
| `crates/sim_core/tests/runtime_catalog_tests.rs` | WP2, WP3 | P-7 |
| `crates/client/tests/effect_catalog_tests.rs` | WP3 | D-8 item 2's rule |
| `docs/03-sc2-mechanics-reference.md`, `docs/plans/2026-11-05-sc2-deviation-fixes.md`, `docs/plans/2026-11-09-effect-visualization.md` | WP5 | D-9 |
| `docs/evidence/<date>-splash-centres.md` (new) | WP0 to WP5 | §7 |

Nothing under `crates/sim_core/src/movement/` changes; a contact-solver branch works there.

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`
(check `quota -s` and `df -h ~/scratch` first); failing tests first with their red run pasted in the hand-back;
`cargo gate quick` and the tests the change needs; a verifier runs `cargo gate package` (Q0061). §7's comparison in
every package against WP0's tree. No `unwrap`; `expect` only with `invariant:`; no catalog identity in production
Rust (C5); tests find weapons by property. Check `docs/questions/open/` before the last commit. All six packages may
run as one branch and one session, one commit each, with one `package` gate on the final tree (charter §4.3).

### WP0: records and baseline (XS, docs only)

- `DESIGN.md` §4: B-169, B-170 registered; B-83, B-84 point to this plan (D-9). Evidence file created.
- Measure WP0's tree (§7) for the baseline, `showcase_effects` included.
- **Gate:** docs-only.

### WP1: the two rules, latent (M, `sim_types`, `sim_core`)

Failing tests first (red on WP0's tree: they do not compile), in `splash_centre_tests.rs` with scratch weapons built
from a shipped circular splash weapon found by property:
- `an_attacker_centred_area_strikes_around_the_attacker` (P-1): a scratch weapon with `impact: Attacker`, enemies at
  the zone's edge on both sides of the attacker ± 2⁻¹⁶; the target struck once; the attacker untouched.
- `an_attacker_centred_area_makes_no_direct_strike`: the target's damage equals one area strike, the target's
  retaliation runs once, and no weapon hit effect runs.
- `an_attacker_centred_area_lands_without_its_target`: the target removed before dispatch; the area still lands.
- `a_conditional_area_opens_only_below_its_target_radius` (P-3): three targets at `L − 2⁻¹⁶`, `L`, `L + 2⁻¹⁶`
  with a bystander in the first zone; the bystander struck only in the first case; the target's damage equal in all
  three; `SplashVisual` published only in the first.
- `a_point_impact_opens_a_conditional_area`: a projectile with `ImpactLastKnownPosition` lands on a point and
  splashes.
- `validation.rs` unit tests: Attacker with a line, with the offset, on a projectile weapon, a non-positive
  `target_radius_below`, each refused naming the field; omitted fields read `Target` and `None`.
- The unlock refusal (D-1).
- `checksum_state_tests::embedded_weapon_fields_and_splash_are_sensitive` extended to both fields.

Then the change: D-1, D-3, D-6 and the docs; the schema proof of §5. **Acceptance:** §8 rule 1. Plan row `Active`.

Commit `feat(sim_core): a splash may centre on its attacker and open only below a target radius (splash-centres WP1, schema N)`.

### WP2: the Baneling's burst (S, data, B-83)

Failing tests first (red on WP1's tree):
- `weapon_catalog_rules_tests::splash_impact_and_condition_follow_starcraft`: a table of every modelled splash with
  its 5.0.14 `ImpactLocation` and condition, each row citing S4, S5, S9 to S12; red on the Baneling. In WP2 the
  Siege Tank's condition reads `None` (the data before WP3), and WP3 changes that one cell to 1.25 as its own red
  test.
- `SC2_SPLASH_OFFSET`'s Baneling row becomes `false` with D-2's reason (re-derived, named in the commit).
- `splash_centre_tests::the_banelings_burst_is_centred_on_it` (P-1, P-2) on the shipped row: an enemy marine
  analogue 2.2 + its radius − 2⁻¹⁶ behind the Baneling is struck, one the same distance beyond the target's near
  edge but farther than reach from the Baneling is not (red today on both).
- `runtime_catalog_tests` (P-7): the impact set back to `Target` in a catalog copy restores the old struck set.

Then the data (D-4). **Acceptance:** §8 rules 2 and 3; §7 column WP2 with the traced bursts; expectations of 11a,
11b, 32 re-recorded only by §7's rule. Commit `fix(sim_core): the Baneling's burst is centred on it (splash-centres WP2, B-83)`,
stating which battles moved, by how much, and why.

### WP3: the Siege Tank's large targets (S, data and client data, B-84)

Failing tests first (red on WP2's tree):
- The table's Siege Tank condition becomes `Some(1.25)` (red on WP2's data).
- `splash_centre_tests::a_sieged_shot_at_a_large_structure_has_no_area` and `…_at_a_small_structure_splashes` (P-3,
  P-4): every shipped structure found by property, a bystander enemy unit in the first zone of each; the area opens
  exactly for those below 1.25; plus "no shipped ground unit has a radius of 1.25 or more".
- `runtime_catalog_tests` (P-7): the condition removed restores the area on a large structure.
- `effect_catalog_tests`: D-8 item 2's rule, red until the siege entry names its impact.

Then the data (D-4, D-8 item 2). **Acceptance:** §8 rule 2; §7 column WP3. Commit
`fix(sim_core): the sieged Siege Tank does not splash targets of radius 1.25 or more (splash-centres WP3, B-84)`.

### WP4: the Planetary Fortress splashes (S, data, B-169)

Failing tests first: the `b16_splash_zones_follow_starcraft` and `SC2_SPLASH_OFFSET` rows for the Planetary Fortress
(S12); `splash_centre_tests::the_planetary_fortress_splashes` (P-5: zones, fractions, target struck once, a friendly
unit untouched); the client's splash-look tests red until the block names `shell_impact`.

Then the data (D-5, D-8 item 3). **Acceptance:** §8 rule 2; §7 column WP4. Commit
`fix(sim_core): the Planetary Fortress analogue splashes (splash-centres WP4, B-169)`.

### WP5: bookkeeping (XS)

D-9's records; the evidence completed; this plan's `## As built`; README row `Done`. Merge `main`; `cargo gate
package` once on the final tree (by the verifier); the schema re-recorded if `main` took the number (§5 step 3).

---

## 12. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Types and validation | Defaults, refused combinations, positive condition | WP1: `sim_types` unit tests, `catalog/validation.rs` unit tests |
| Rule | P-1, P-3; no direct strike; landing without a target; point impacts | WP1: `splash_centre_tests.rs` |
| Encoding | Both fields change the checksum | WP1: `checksum_state_tests.rs` |
| Data | Every splash's impact, condition, offset and zones against 5.0.14 | WP2 to WP4: `weapon_catalog_rules_tests.rs` |
| Shipped behaviour | P-2, P-4, P-5 | WP2 to WP4: `splash_centre_tests.rs` |
| Zero recompilation | P-7 | WP2, WP3: `runtime_catalog_tests.rs` |
| Presentation data | Conditional areas name an impact; the Planetary Fortress names a splash look | WP3, WP4: `effect_catalog_tests.rs` |
| Worlds | P-6, P-8: §7's table | every package (measure scripts); goldens and sandbox in the package gate |
| Charter subjects | `sim_core`'s subjects of `docs/guides/test-gates.md` (`damage_and_shields_tests.rs`, `full_match_tests.rs`, `golden_replay_tests.rs`, …) keep passing; `client`'s skirmish | package gate |

Re-derived (expected to change, from D-1 to D-5): the `SC2_SPLASH_OFFSET` Baneling row; `b16`'s table; any test
asserting the Baneling's direct strike or the burst's position at the target (none found by `grep -rn
"volatile_tick"` in `combat_correctness_tests.rs`, `damage_and_shields_tests.rs`; **verify** in WP2); any test firing
the sieged tank at a structure (`spells_and_abilities_tests::test_siege_mode_toggle_immobility_and_long_range_splash`,
`sc2_battle_benchmarks_tests::test_railgun_crawler_declarative_modes_and_siege_splash`, `structure_radius_tests`'s
`splash_weapon`, which picks the first circular splash weapon of range ≥ 1 and clears its offset; **verify** each
fires at units).

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The Baneling's offset reading (D-2) is wrong | Up to 0.375 of reach, forward or all round | One data value or line; the draft question (§15) |
| R-2 | Battle 32 flips to the marines | A matrix claim no longer holds | §7's rule: traced, re-recorded with evidence, claim re-read; not a reason to keep a known deviation |
| R-3 | A test fires the sieged tank at a structure or the Baneling at a lone target and asserts the old numbers | Package red for a correct change | §12's re-derived list; §8 rule 2 |
| R-4 | `showcase_effects` moves in WP3 | A capture's take differs | Expected only if a sieged tank fires at a town hall; traced; the capture is re-taken by its own procedure if its storyboard checks fail (**verify** the capture test's tolerance) |
| R-5 | Another branch takes schema 36 | Conflict at merge | "Next free at merge"; §5 step 3 |
| R-6 | The `Radius` vs. engine-radius reading of the condition differs for a future structure | A structure on the wrong side of 1.25 | P-4's test reads every shipped structure; the comment on each `radius` row names both values, so a new row whose two fall on different sides is visible; recorded as **verify** |
| R-7 | The visual double look (impact and blast) on small targets is judged noisy | Presentation | §14 F-5's alternative; the look is reviewed with WP3's picture |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: the lowered Supply Depot's exception** (S9). When depot lowering is modelled (a mode of the Supply Depot
  analogue), the sieged shot must splash around a lowered depot. Hook: a mode field that sets the structure's
  radius as the condition reads it, or a per-mode `splash_exempt` flag read by `apply_splash`; decided with that
  mode.
- **F-2: B-170, area damage keeps to a cliff level.** `SameCliff` on every `CEffectDamage` area (S14) and the
  Baneling's fallback for a target on another level (S6). Needs the flag's meaning for ramps and air targets
  (**verify**), a cliff-level query on the search centre, and the fallback as a data rule (a direct strike when the
  target is on another level than the attacker). Its own plan; it moves every splash near a cliff.
- **F-3: the Baneling ordered onto a friendly unit** (`SuicideTargetFriendlySwitch`): a direct strike, no area.
  **verify** whether our orders allow it; if they do, register it.
- **F-4: the Baneling's manual Explode and structure-attack toggle** (deviation-fixes WP18).
- **F-5: an impact look only when a conditional area does not open.** Needs the simulation to publish which branch a
  shot took (a flag on the delivery event); taken up if the review of D-8 item 2 finds the double look wrong.
- **The rest of the Planetary Fortress weapon row** (its upgrade growth, Hi-Sec Auto Tracking) is not re-audited here.

---

## 15. Draft owner question (for the orchestrator to file; not filed by this plan)

```markdown
# Q0096: Is the Baneling's 2.2 burst measured from its centre?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** from the Baneling's centre (plan `2026-11-23-splash-centres` D-2): a unit is hit when
  its edge is within 2.2 of the Baneling's centre.

## Question

The StarCraft II 5.0.14 data centre the Baneling's burst on the Baneling itself (`ImpactLocation CasterUnit`), and
Space Scuffle now does too (bug B-83). The same data keep a flag, `OffsetByUnitRadius`, that for other weapons moves
the centre to the struck unit's edge facing the attacker (Q0063). For the Baneling the "struck unit" is the Baneling,
so the data do not say what the flag does. Three readings:

- (a) **The Baneling's centre** (the default, recommended: the only reading that needs no direction).
- (b) The Baneling's front edge, towards its target (0.375 forward).
- (c) The Baneling's edge all round (an effective radius of 2.575).

If you can, check in the editor: a Baneling attacking a Zergling, with a Marine (radius 0.375) placed 2.8 behind the
Baneling's centre (reached only under (c)) and another 2.8 in front of it, beside the Zergling (reached under (b) and
(c), not under (a)).

## Answer

```

The question number is the next free at filing (0096 on `e7153b8b`).

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`), read as data with `grep` and `sed`; the chain excludes `libertymulti`, `swarmmulti` and
  `balancemulti` (S1), whose older Siege Tank switch (`libertymulti EffectData:59–63`) is not used.
- Baseline figures: `~/scratch/structure-attributes/world-wp1/matrix/matrix.md`.
- No prototype: WP1 is latent by construction, WP3 and WP4 act on no measured world, and WP2's direction is
  predicted (P-8) and measured in the package itself.
