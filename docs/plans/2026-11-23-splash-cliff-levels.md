# Blueprint: Area Damage Keeps to Its Cliff Level — `SameCliff` and the Baneling's Fallback (B-170)

This plan settles bug B-170 of the register against StarCraft II 5.0.14:

- **A damage effect's area keeps to the cliff level of its search centre.** Every `CEffectDamage` sets
  `SearchFlags SameCliff 1` by default (`core` EffectData:67). The searches that a `CEffectEnumArea` runs (the lines
  of the Hellion, the Colossus and the Lurker) do not. Ours reaches every level for every area.
- **The Baneling strikes a target on another level directly.** Its burst is centred on the Baneling, so under
  `SameCliff` it would miss a target standing on another level. 5.0.14 adds a fallback set that strikes that target
  directly with the burst's own damage (`voidmulti` EffectData:2327–2358, validator `CliffLevelNotEqual`,
  `voidmulti` ValidatorData:894–898).

Both rules become capability data on `SplashSpec`: `cliff` (`Same`, the engine default of a damage area, or `Any`)
and `strike_target_on_other_level` (the Baneling's fallback). A position's cliff level is a new query on the map,
`GameMap::cliff_level_at`. No code names a unit (charter §3.3, C5). One neighbouring difference is registered and
deferred (B-174: the Lurker's spine searches run only over cliff level 1 or more).

A full plan is required (charter §6): a new rule in the simulation, a change of hashed encoding (checksum schema),
and data that moves battles near cliffs (the showcase effects scenario's siege line stands above a ramp).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`d0fd365b` (checksum schema 36).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the specification is silent, search for what StarCraft II does | Standing rule; the level of a ramp and of an air unit are not in the data | D-2 |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.4, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0096: the Baneling's burst is measured from its centre | Answered (a) | D-4 (the burst's centre is the Baneling's position, so its level is the Baneling's) |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6, §7 |
| **Draft, not filed:** "Which cliff level does a unit on a ramp, and an air unit, stand on?" | Proposed for the orchestrator to file as Q0099 (§15); defaults followed meanwhile | D-2 |

---

## 1. Summary

| | Today (`d0fd365b`) | After this plan |
|---|---|---|
| Target-centred circle (Siege Tank sieged, Planetary Fortress, Ultralisk, Liberator's anti-air) | Strikes every unit in reach on any level | Strikes only units on the cliff level of the search centre; the target is still struck directly on any level |
| Attacker-centred circle (Baneling) | Burst reaches every level; the target is always inside it | Burst keeps to the Baneling's level; a target on another level is struck directly, once, with the same damage |
| Lines (Hellion, Colossus, Lurker) | Every level | Every level (unchanged; the data clear or never set the flag) |
| A position's level | Read ad hoc from the cell (placement, exits, spawns) | `GameMap::cliff_level_at(pos)`: the level of the cell under the position, clamped to the map (D-2) |
| `SplashSpec` | zones, shape, friendly fire, offset, impact, target-radius condition | + `cliff` (`Same` default, `Any`) and `strike_target_on_other_level` (`false` default) |
| Hashed state | — | The two fields in the splash encoding; schema **next free at merge** (37 unless another branch takes it) |
| Worlds that move | — | WP4 only, and only worlds where an area reaches a unit on another level: the showcase effects scenario is expected to; every flat world (matrix, battles 31 and 33) and the showcase (no splash weapon) are byte-identical |
| Size | — | M: six packages, WP2 M, the rest S or XS; WP3 runs only on WP0's finding (§7.2) |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; **verify** not confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at `~/scratch/combat_wp3_data/`. Chain `core → liberty → swarm → void → voidmulti` (as in plan `2026-11-23-splash-centres` S1); `libertymulti`, `swarmmulti` and `balancemulti` are not in it. `liberty EffectData:N` is line N of that layer's file | Every value below [DATA] |
| S2 | `CEffectDamage` default, `core EffectData:61–69`: `ImpactLocation TargetUnitOrPoint` (:66), `SearchFlags SameCliff 1` (:67) | Every damage effect's own area keeps to a level unless it clears the flag |
| S3 | `CEffectEnumArea` default, `core EffectData:71–77`, and `EA_WEAP`, `:81–83`: no `SameCliff` | An enum-area search reaches every level unless it sets the flag [DATA: absent; INF: the engine's unset flag is 0] |
| S4 | The siege shot: `CrucioShockCannonBlast`, `liberty EffectData:1668–1681` (`AreaArray` 0.4687 / 0.7812 / 1.25, parent `DU_WEAP`, no `SameCliff`); `swarm EffectData:2197–2201`, `voidmulti EffectData:669–676` change only validators and amounts | Keeps to its level (inherits S2) |
| S5 | The Planetary Fortress: `TwinIbiksCannon`, `liberty EffectData:2533–2546`; `swarm EffectData:309–311` changes only `Kind` | Keeps to its level (S2) |
| S6 | The Ultralisk: weapon `KaiserBlades`, `swarm WeaponData:183–184`, effect `KaiserBladesDamage`; `swarm EffectData:2272–2281` gives that damage effect its `AreaArray` (arcs 45 and 180, radius 2); `void EffectData:1167–1170` changes amounts only. `liberty`'s former search `KaiserBladesSearch` (`liberty EffectData:2285–2295`) also set `SameCliff 1` (:2292) | Keeps to its level (S2) |
| S7 | The Liberator's anti-air: `LiberatorMissileDamage`, `void EffectData:2932–2940` (parent `DU_WEAP_MISSILE`, `AreaArray Radius 1.5`, `SearchFilters Air;…`); `voidmulti EffectData:752–754` changes `Amount` only | Keeps to its level (S2); the area searches air units |
| S8 | The Baneling: `VolatileBurstU` (`liberty EffectData:1753–1765`) and `VolatileBurstU2` (`:1052–1064`), both `ImpactLocation CasterUnit`, no `SameCliff` | The burst keeps to the level of its centre, the Baneling (S2) |
| S9 | `BanelingVolatileBurstDirectFallbackSet`, `voidmulti EffectData:2327–2332`: validator `CliffLevelNotEqual`; effects `VolatileBurstDirectFallbackEnemyNeutralBuilding` (`:2337–2347`, parent `VolatileBurstU2`) and `…EnemyNeutralUnit` (`:2348–2358`, parent `VolatileBurstU`), each with its `AreaArray` and `ExcludeArray` removed, `ImpactLocation TargetUnit`, `SameCliff 0`, `OffsetByUnitRadius 0`, `noMarkers` and a unit filter (`voidmulti ValidatorData:899–904`: enemy ground non-structure; enemy ground structure). `VolatileBurst`, `voidmulti EffectData:2333–2336`, runs the set at index 4, after `VolatileBurstU2` and `VolatileBurstU`, before `Suicide` | When the target's level differs from the Baneling's, the target takes the burst's unit or structure damage directly, once (`noMarkers`) |
| S10 | `CliffLevelNotEqual`, `voidmulti ValidatorData:894–898`: `WhichLocation TargetUnit`, `OtherLocation OriginUnit`, `Compare NE`; `CValidatorLocationCompareCliffLevel` default, `core ValidatorData:37–39` | The comparison is between the target unit's location and the origin (the Baneling's) location |
| S11 | The Hellion: weapon `InfernalFlameThrower`, `liberty WeaponData:582–595`, effect `InfernalFlameThrowerCP` (searches `InfernalFlameThrowerE`, `liberty EffectData:2609–2616`, a `CEffectEnumArea`, no `SameCliff`); `swarm EffectData:2244–2247` changes the count and offsets only | The line reaches every level (S3) |
| S12 | The Colossus: `ThermalLancesE` / `ThermalLancesEReverse`, `liberty EffectData:2412–2418`, `:786–792` (`CEffectEnumArea`, no `SameCliff`), each unit found struck through `ThermalLancesDamageDelay` → `ThermalLancesMU`. `liberty`'s `ThermalLancesMU` carries the validator `ThermalLancesCliffLevel` (`:2425`), which no layer of S1 defines; `voidmulti EffectData:964–970` replaces that validator (index 0) with `MultipleHitGroundOnlyAttackTargetFilter` | The 5.0.14 line reaches every level; the former cliff validator is gone |
| S13 | The Lurker: `LurkerMPSearch`, `swarm EffectData:3022–3030` (a `CEffectEnumArea`, no `SameCliff`; validator `CliffLevelGE1`); `LurkerMPDamage`, `:3031–3037`, sets `SameCliff 0` (:3036); `voidmulti EffectData:815–823` changes the radius, validators and the bonus only | The line reaches every level |
| S14 | Damage effects without an area that clear the flag: `VolatileBurstFriendlyBuildingDamage`, `VolatileBurstFriendlyUnitDamage` (`liberty EffectData:1076`, `:1777`), the two fallbacks (S9), `LurkerMPDamage` (S13), `Feedback` (`liberty :601`); and every direct weapon damage (e.g. `90mmCannons`, `liberty EffectData:1661–1667`) keeps the default 1 and strikes its target on any level | The flag acts only on a damage effect's own `AreaArray` search, never on its impact unit [INF] |
| S15 | `CliffLevelGE1`, `core ValidatorData:671–675` (`Compare GE`, `Value 1`, location the target point by the `CValidatorLocation` default, `core ValidatorData:34–36`) | B-174 (§2.4) |
| S16 | Plan [`2026-11-23-splash-centres`](2026-11-23-splash-centres.md) §2 S14, §14 F-2 (where B-170 was found) | The register text this plan replaces |

### 2.2 The rules that follow

1. **A damage effect's area strikes only units on the cliff level of its search centre** (S2, S4 to S8). The
   reference is the search centre, not the attacker: a Siege Tank above a cliff splashes the low ground around its
   target, which is the classic high-ground siege position; with the attacker as reference no shell on the low ground
   could splash at all [INF]. The impact unit itself is struck directly whatever its level (S14) [INF].
2. **Enum-area searches reach every level** unless they set the flag (S3, S11 to S13). Every modelled line is one
   [DATA].
3. **The Baneling's burst keeps to the Baneling's level; a target on another level is struck directly** with the
   burst's own unit or structure damage, once, and only if it passes the burst's own filters (enemy, ground) (S8 to
   S10) [DATA]. A target on the Baneling's level is struck by the area alone (`noMarkers`) [DATA].
4. **What "cliff level" means for a position is not in the data.** The validators compare `CliffLevel` of locations
   (S10, S15); a unit's location is its position. How a ramp counts, and whether an air unit has the level of the
   ground below it, the data cannot say (**verify**, D-2, §15).

### 2.3 Values: every shipped splash

| Ours (slug, weapon) | 5.0.14 effect | Kind | `cliff` | Fallback | Source |
|---|---|---|---|---|---|
| `railgun_crawler`, `siege_mode` ground override | `CrucioShockCannonBlast` | damage area | `Same` | — | S4 |
| `planetary_bulwark_dome`, ground | `TwinIbiksCannon` | damage area | `Same` | — | S5 |
| `stag_behemoth`, primary | `KaiserBladesDamage` | damage area | `Same` | — | S6 |
| `sky_bastion`, air | `LiberatorMissileDamage` | damage area (air) | `Same` | — | S7 |
| `volatile_tick`, primary | `VolatileBurstU`, `VolatileBurstU2` | damage area at the caster | `Same` | `true` | S8 to S10 |
| `napalm_wheeler`, primary (line) | `InfernalFlameThrowerE` | enum area | `Any` | — | S11 |
| `tripod_annihilator`, primary (line) | `ThermalLancesE` | enum area | `Any` | — | S12 |
| `burrow_scorpion`, `burrowed` (line) | `LurkerMPSearch` | enum area | `Any` | — | S13 |

WP4's test finds these rows by property (shape and impact), not by slug; the table is its oracle.

### 2.4 Neighbouring rules: in or out

| Rule | Verdict | Why |
|---|---|---|
| B-170 `SameCliff` and the Baneling's fallback | **In** (D-1 to D-5) | |
| The Lurker's searches run only over cliff level ≥ 1 (`CliffLevelGE1`, S13, S15) — **B-174, registered here** | Deferred (§14 F-1) | Every flat test map of ours is level 0, where the Lurker would then never hit; how the game's level numbers map onto ours (our level 0 is ordinary ground in the matrix, abyss on the showcase map) needs its own decision (**verify**) |
| The Colossus' former `ThermalLancesCliffLevel` validator (S12) | **Conforming** | Removed in `voidmulti`; the 5.0.14 line reaches every level, as ours |
| The Baneling ordered onto a friendly unit (`VolatileBurstFriendly…Damage`, S14) | Out (splash-centres §14 F-3, unchanged) | Its own switch |
| B-19 (the Ultralisk's arcs), B-18 (one search per strike), B-85 | Out, their own rows | The level filter composes with whatever search shape lands |
| Vision, targeting and pathing across cliffs | Out | Unchanged; this plan touches only who an area strikes |

---

## 3. Diagnosis

1. **`strike_area`** (`crates/sim_core/src/combat_resolution.rs`, tick phase 7) strikes every damageable unit whose
   edge lies within the zones of the nearest search point, on the area's plane, sparing the primary target and the
   attacker. Nothing reads terrain. `SplashSpec` has no field for a level rule; its doc names B-170 as not modelled
   (`crates/sim_types/src/weapon.rs`, the `SplashSpec` doc: "an area reaches every cliff level (bug B-170)").
2. **The Baneling** (`resolve_instant_shot`, the `SplashImpact::Attacker` branch) runs `apply_splash` around
   `shot.origin` with no primary; its target is struck only by the area (splash-centres D-1). Under a level filter alone
   a target on another level would be missed; the fallback is needed in the same package as the Baneling's `Same`.
3. **No position-level query exists.** `sim_map::GameMap` stores `MapCell::cliff_level` per cell; a ramp cell stores
   its lower landing's level (`GameMap::ramp_profile` reads it as `low_level`; validation's `RampLowerLandingMismatch`
   and `RampUpperLandingMismatch` enforce it). Callers read cells directly (`production/exits.rs`
   `producer_level`, `scenario.rs` spawn rings, `scenario/placement.rs`). Terrain height has a query
   (`terrain_altitude_scalar`), level has none.
4. **So the fix is code and data:** a map query, two `SplashSpec` fields with their rule, and the shipped rows.

---

## 4. Design decisions

### D-1. `SplashSpec::cliff`: the level rule of an area

**Type** (`sim_types/src/weapon.rs`, exported from `lib.rs`):

```rust
/// Which cliff levels a weapon's area reaches: `StarCraft` II's `SearchFlags` `SameCliff`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplashCliff {
    /// Only units on the cliff level of the search point that reaches them (`SameCliff 1`, the
    /// default of every `CEffectDamage`, `core` EffectData:67).
    #[default]
    Same,
    /// Units on every level (`SameCliff 0`; a `CEffectEnumArea` search, which does not set it).
    Any,
}
```

`SplashSpec` and `SplashSpecDef` gain `#[serde(default)] pub cliff: SplashCliff`. The serde default is the
engine's default for a damage area, as the charter's "default as in the game" asks; a line must author `Any`, which
every shipped line does with its citation (§2.3). Validation accepts both values for both shapes (a line that keeps
to a level is expressible data, not an error).

**Rule** (in `strike_area`, after the existing distance filter):

```text
if splash.cliff == Same:
    levels[i] := map.cliff_level_at(points[i])         # once per strike_area call, retained buffer
    level_u   := map.cliff_level_at(other.position)    # only for a unit already within reach of some point
    distance  := min over i with levels[i] == level_u of edge_distance(points[i], other)
    skip other if no such i is within reach
```

- **Per search point.** A circle has one point, so the rule reads "the unit is on the level of the centre". A line
  (none shipped with `Same`) compares each of its searches against its own point, and a unit's fraction comes from
  the nearest search point on its level: the game runs one search per point (S3).
- **The search centre is the reference** (§2.2 rule 1): the target's centre or, with `offset_by_unit_radius`, its edge
  facing the attacker; the attacker's release position for an attacker-centred area; the landing point for a
  projectile that lands on a point; each line point. The two readings, centre point or impact unit, differ only for
  an offset centre that falls in another cell of another level than the target's centre (a target touching a cliff
  edge, struck from beyond it); the plan takes the centre point, the only location a search has (**verify**, §15
  part 3).
- **The primary target is unaffected**: it is struck directly by `apply_weapon_hit` on any level (S14), and the area
  spares it as now.
- **Pending lines** (B-85, `advance_pending_lines`) go through the same `strike_area`, so they follow the rule
  unchanged.
- The module doc's invariants gain: "6. An area whose `cliff` is `Same` strikes only units on the cliff level of a
  search point that reaches them (plan `2026-11-23-splash-cliff-levels` D-1)."

### D-2. What a position's cliff level is: `GameMap::cliff_level_at`

**Function** (`crates/sim_map/src/grid.rs`):

```rust
/// The cliff level a position stands on: the level stored in the cell under `pos`, a ramp cell
/// counting as its lower landing (as the map stores it). A position off the map takes the level
/// of the nearest cell on the map, so every position has a level. The level depends on the
/// position alone, not on what stands there: an air unit has the level of the ground below it.
#[must_use]
pub fn cliff_level_at(&self, pos: SimVec2) -> CliffLevel
```

Implementation: clamp `pos` to `[0, width) × [0, height)` cell coordinates (a negative coordinate to 0, one at or
beyond the edge to the last cell), read `cells[y * width + x].cliff_level`. No `Result`: the clamp makes the function
total, and it is documented, not a silent fallback (the terrain off the map is defined as its border's). Unit
positions are always on the map; only a line point can lie off it.

**Ramps: the stored level (the lower landing).** The data do not say how a ramp counts (§2.2 rule 4). Three readings:
(a) the lower landing, as each ramp cell stores it; (b) the nearer landing by surface height, the upper one from the
ramp's midline (the half-cliff that ramp sight already uses, `sim_map/src/vision.rs`); (c) both landings (a ramp
separates from neither). **Decision: (a)**, for three reasons: it is the one level the map already stores, which
placement, exits and spawns read, so the simulation keeps one meaning of "a position's level"; it needs no geometry;
and the data hint at it: the Baneling's fallback (S9), added late to `voidmulti`, matters for a melee unit only where a
Baneling and its target stand at melee distance on two levels, which is chiefly a Baneling on a ramp striking a wall at
its top; under (b) such a Baneling would already share its target's level [INF, weak]. The choice lives in this one
function; (b) or (c) is a change of its body and its tests. The draft question (§15 part 1) asks for an editor check.

**Air units: the ground below.** `CliffLevel` is a property of a location (S10, S15), and a unit's location is its
position, so an air unit over a cliff edge belongs to the level beneath it (literal reading, [INF]). The alternative,
that an air search ignores levels, would need an engine rule the data do not show. Only the Liberator's anti-air area
searches air units (§2.3); the draft question (§15 part 2) asks.

### D-3. `SplashSpec::strike_target_on_other_level`: the Baneling's fallback

**Field:** `#[serde(default)] pub strike_target_on_other_level: bool`: "When the target stands on another cliff level
than the attacker, the target takes this area's strike directly, at the first zone's fraction, once. `StarCraft` II's
`BanelingVolatileBurstDirectFallbackSet` (`voidmulti` EffectData:2327–2358) on `CliffLevelNotEqual` (ValidatorData:
894–898). Meaningful only for an attacker-centred area that keeps to its level."

**Rule** (`resolve_instant_shot`, the attacker-centred branch, after `apply_splash`):

```text
if splash.strike_target_on_other_level
   and target exists
   and map.cliff_level_at(target.position) != map.cliff_level_at(shot.origin):
    strike_area_target(source, splash, area, target)
        # strike_area's filters without its distance and level tests;
        # damage = splash_raw_damage at distance 0 (the first zone's fraction)
```

- **Same damage as the area.** `splash_raw_damage` at distance zero is exactly what the area would have dealt the target
  at its centre zone: the unit strike or the structure strike (B-82's `structure_strike`), with upgrades. So P-3's
  damage equals today's (splash-centres rule 3: the target is always inside the 2.2 burst).
- **The same filters as the area** (S9: the fallbacks' unit filters are the bursts' search filters): damageable,
  friendly fire, plane, `target_flags`; the attacker never strikes itself. Retaliation runs as for any struck unit.
  No weapon hit effect (an attacker-centred weapon makes none, splash-centres D-1).
- **Once.** A target on the attacker's level is reached by the area and never by the fallback; one on another level is
  never reached by the area (D-1). So `noMarkers` holds by construction; P-3 checks it.
- **Order** follows the game's set (S9): the area, then the fallback, then the suicide (`dispatch_shot`, unchanged).
- **Independent of the area's condition:** if `target_radius_below` closed the area, the fallback still runs (the game's
  set does not depend on the area). No shipped row combines them.
- **Validation** (`catalog/validation.rs`): `strike_target_on_other_level: true` requires `impact: Attacker` and
  `cliff: Same` ("a target-centred area strikes its target directly already; an area reaching every level always
  reaches its target"), each refusal naming the field.
- A small private helper (`strike_area_target`) shares `strike_area`'s per-unit body, so the filters and the strike are
  one code path (invariant 1 of the module: all area damage goes through `apply_splash`'s machinery; the module doc
  names the fallback as part of an attacker-centred area).

### D-4. Data

- WP2 (latent): every shipped splash authors `cliff:` explicitly. The three lines `Any` with S11 to S13 (their final
  value). The five circles `Any` with the comment "B-170: reaches every level until plan
  `2026-11-23-splash-cliff-levels` WP4 applies `SameCliff`", the value they have today, so WP2 moves no world.
- WP4: the five circles drop their `cliff: Any` (they take the default `Same`), each with its source from §2.3; the
  Baneling gains `strike_target_on_other_level: true` with S9 and S10. The Baneling's change and its fallback land in
  one commit (§3 item 2).
- **Zero recompilation** (charter §3.3): a catalog copy with the Siege Tank's `cliff: Any`, or the Baneling's fallback
  removed, changes the outcome in the same binary (`runtime_catalog_tests`, WP4).

### D-5. Hashed state and checksum

- `fields!(SplashSpec { …, target_radius_below, cliff, strike_target_on_other_level })` in `checksum/encoding.rs`,
  with a `Canonical` for `SplashCliff` (0 `Same`, 1 `Any`); `bool` uses the existing encoding. A `WeaponSpec` is hashed
  inside every in-flight `ActiveProjectile` (the Liberator's missiles), so both fields are decision state.
- **Schema: next free at merge** (37 on `d0fd365b`; 38 if another branch takes 37). The doc comment of
  `CHECKSUM_SCHEMA_VERSION`: "Schema N adds each splash's cliff rule (0 same level, 1 any level) and its
  other-level target strike (a `bool`), after `target_radius_below` (plan `2026-11-23-splash-cliff-levels` D-5)."
- WP2 claims the number in its own commit; the merge takes the next free one and re-records then (§5).

### D-6. What the simulation publishes

Nothing changes in kind. `SplashVisual` is published per search point as today, with its zones, at the terrain height
of the point; a blast at a cliff edge still draws over both levels, as the game's blast model does, while it strikes
one. The fallback publishes no event of its own (the burst's `SplashVisual` at the Baneling is the look). No event type
or field changes.

### D-7. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | B-170 points to this plan (status "open; plan `2026-11-23-splash-cliff-levels`"); register **B-174** ("The Lurker analogue's spine searches run over every cliff level; 5.0.14's `LurkerMPSearch` runs only where the cliff level is 1 or more (`CliffLevelGE1`, `swarm` EffectData:3023, `core` ValidatorData:671–675); how the game's level numbers map onto ours is **verify**") | WP0 |
| `DESIGN.md` §4 | B-170 fixed with commits and evidence; the Combat row gains "an area keeps to its search point's cliff level unless its data say otherwise; the Baneling strikes a target on another level directly"; the Terrain or Map row (whichever lists levels) names `cliff_level_at` | WP5 |
| `crates/sim_types/src/weapon.rs` `SplashSpec` doc | Drop "an area reaches every cliff level (bug B-170)"; name the rule | WP2 |
| `docs/plans/2026-11-23-splash-centres.md` §14 F-2 | "Taken up by plan `2026-11-23-splash-cliff-levels`" | WP5 |
| New `docs/evidence/2026-11-23-splash-cliff-levels.md` | §7's comparisons per package, the probe's counts (§7.2), the traced first differences | WP0 creates; WP1 to WP5 append |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Done` (WP5) | plan, WP1, WP5 |

---

## 5. Determinism and checksum

- **Integer and fixed point only.** `cliff_level_at` floors fixed-point coordinates to cells (the existing
  `saturating_to_u32` path of `world_to_cell`, after a clamp) and compares `u8` levels. No float is added to the
  simulation; the catalog's `cliff` is an enum and the fallback a `bool`, so C7's float allowances do not change.
- **No new entity state.** The fields live in `WeaponSpec` (D-5); the level is derived from the map and positions each
  time.
- **Goldens.** Neither replay contains a splash weapon (`golden_replay.ron` and `golden_replay_orders.ron`: marines,
  Roach analogues, zealots, an Observer analogue, tanks that never siege; splash-centres §5), so behaviour cannot move,
  although the default arena has levels 1 and 2 and ramps. Only WP2's schema byte moves every checkpoint of both, and
  `DEFAULT_SANDBOX_CHECKSUM`.
- **WP2's proof** (the procedure of the splash-centres and production-queue plans):
  1. With the two fields left out of the encoding and the schema at its current number, the tree reproduces every
     recorded checksum of both goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release and on
     `wasm32-wasip1` release and debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`).
  2. Then the fields go in, the schema becomes the next free number, and the checksums are re-recorded under the
     header "Re-recorded for plan 2026-11-23-splash-cliff-levels WP2 (checksum schema M -> N): each splash also
     encodes its cliff rule and its other-level target strike, so every checkpoint changed. No behaviour did: with the
     two fields left out of the encoding at schema M, both fixtures reproduce as recorded for <the previous header>."
     The four builds reproduce the new values.
  3. If `main` takes the number first, the merge repeats step 2 with the next one.
- **WP3 to WP5 leave both goldens and the sandbox unchanged.** A move is traced to its first checkpoint and entity; an
  unexplained one stops the package.
- **wasm.** No dependency changes; `sim_map` gains a pure function.
- **Cost.** One `cliff_level_at` per search point (one for a circle) and one per unit already within reach, both O(1);
  the per-point levels go in a retained buffer of the combat workspace beside `splash_points` (a
  `// PERFORMANCE RATIONALE:` block: Why, a strike must not allocate; What, levels kept beside the points; Trade-off,
  one more buffer at its high-water mark). The throughput bench is checked in the package gate as usual.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` (a plateau, a cliff edge, a
ramp run), read every radius, zone and range from the catalog, and find weapons by property (`cliff`, shape,
`impact`, the fallback flag, plane), never by slug.

- **P-1 (same level).** For every strike of an area with `cliff: Same`: the units struck are exactly those struck
  today whose `cliff_level_at(position)` equals the level of a search point reaching them, each at the fraction of the
  nearest such point; units on other levels are untouched; the primary target is struck directly on any level.
  Checked with a bystander at equal distance on each side of a cliff edge, at the edge distance ± 2⁻¹⁶, and with
  bystanders on a ramp's cells and its upper landing.
- **P-2 (any level).** An area with `cliff: Any` strikes exactly the set and fractions it strikes today, on a map with
  levels.
- **P-3 (the fallback).** For an attacker-centred area with the fallback: a target on the attacker's level is struck
  once, by the area; a target on another level is struck once, directly, with damage equal to the area's centre-zone
  strike (the unit strike, or the structure strike for a structure); bystanders on the target's level are untouched; a
  friendly or air target is not struck by the fallback (the area's own filters). With the fallback off (a scratch
  copy), the target on another level is untouched.
- **P-4 (the level of a position).** `cliff_level_at` returns the cell's stored level: on flat cells; on every cell of
  a ramp run, the lower landing, for each cardinal direction; on cell boundaries, the cell the floor gives; off the map
  on each side and corner, the nearest border cell's level. It depends on the position only (an air unit's level is the
  ground's).
- **P-5 (the shipped classification).** §2.3's table: every shipped circle keeps to its level, every shipped line
  reaches every level, exactly the attacker-centred area has the fallback.
- **P-6 (confinement).** WP1 and WP2 change no world (all of §7 byte-identical; goldens by the schema byte only). WP3,
  when it runs, adds a battle and changes no existing world. WP4 changes only worlds in which, on the WP0 tree, an
  area reached a unit on another level than its search point, or a Baneling burst had its target on another level
  (§7.2's probe); every other world is byte-identical.
- **P-7 (zero recompilation).** Changing `cliff` or the fallback in a catalog copy changes the outcome in the same
  binary.
- **P-8 (direction of WP4's move, a prediction recorded before measuring, not a threshold).** At the first differing
  tick of every world that moves, the struck set of one area search is a strict subset of the WP0 tree's for the same
  state (the rule only removes units), the removed units are exactly those on another level, and no fallback strike
  occurs (no measured world places a Baneling near a cliff). In the showcase effects scenario the siege line stands on
  player 0's base plateau and the raiders climb the ramp below it, so the shells on raiders at the ramp's top spare
  raiders still on the ramp (a ramp counts as the lower landing, D-2) and, with the tank's friendly fire, spare the
  defenders above when the target is on the ramp. So the raiders should take less splash damage in total and the
  defenders no more; which side's units remain at the end should not change on a seed. (The route trace logs only
  moving units, so WP0 records each side's survivors at tick 6,000 from the scenario's final snapshot, **verify** the
  example's output, before WP4 compares.) A result against this is traced (§8 rule 3), not accepted as measured.

---

## 7. Worlds that move

### 7.1 Commands

The splash-centres scripts (`~/scratch/splash-centres/scripts/measure.sh`, `compare.sh`), copied to
`~/scratch/splash-cliff/scripts/`; binaries built once per tree and run from that tree's `assets/`:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/splash-cliff
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64, the
showcase and the showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks. Run in the
background, one at a time across worktrees (charter §7). The client skirmish (`skirmish_match_tests`) is compared by its
stage log (`client.sh`, as in the production-queue plan).

### 7.2 WP0's probe: does a measured world splash across a cliff?

On WP0's tree, a scratch patch (never committed; kept under `~/scratch/splash-cliff/probe/` with its diff) makes
`strike_area` and the attacker-centred branch print, for every search, each unit struck whose cell level differs from
its search point's, and every Baneling burst whose target's cell level differs from the Baneling's. The probe binary
plays every world of §7.1; the evidence records the count per world and seed and the first such event of each (tick,
attacker, target, units, levels).

- **Expected** (from the map and the trace of splash-centres' WP4 tree, `world-wp4/fx-default/units.csv`): the matrix,
  battles 31 and 33 are flat (`sim_fixtures` builds them at level 0) and the showcase fields no splash weapon
  (`battle_automaton`: SCV, Marine and Roach analogues), so their counts are zero. In the showcase effects scenario the
  raiders reach player 0's plateau (level 2) by the west ramp (cells x 26 to 28, y 8 to 11, level 1) around ticks 540
  to 740 while the three sieged tanks fire from the plateau, so nonzero counts are expected there.
- **Rule:** if any measured world has a nonzero count, WP3 does not run, and P-8 is judged on those worlds. If every
  count is zero, WP3 adds one principled world (§7.4) before the rule's data lands, so WP4's move is measured on it.

### 7.3 Expected moves

| World | WP1 (map query) | WP2 (rules, latent) | WP3 (only if §7.2 finds nothing) | WP4 (data) |
|---|---|---|---|---|
| Matrix (seeds of record and 16) | byte-identical | byte-identical | byte-identical except the new battle | byte-identical: every arena is flat |
| Battles 31 (100 seeds), 33 (64 seeds) | byte-identical | byte-identical | byte-identical | byte-identical (flat) |
| Showcase, default seed and 1–8 | byte-identical | byte-identical | byte-identical | byte-identical (no splash weapon) |
| Showcase effects, default seed and 1–8 | byte-identical | byte-identical | byte-identical | **moves on the seeds §7.2 counts**, from the first counted event (P-8); byte-identical on seeds with a zero count |
| Battle 40 (WP3 only) | — | — | recorded on WP3's tree | **moves** (P-8): the shell on a unit that crested the ramp spares the units still on it |
| Goldens, sandbox | unchanged | schema byte only (§5) | unchanged | unchanged |
| Client skirmish stage log | identical | identical | identical | identical (`standard_automaton` and the pilot build no splash unit, **verify** from the stage log) |
| Constructed worlds (§12) | P-4 | P-1, P-2, P-3, P-7 on scratch weapons | — | P-3, P-5, P-7 on shipped rows |

**What WP4 records** for every world that moved (in the evidence, all seeds, before and after): the first differing
tick and the event behind it (from the probe), each side's survivors at the end and the tick of the last death, and,
for the default seed, the struck sets of the first three cross-level searches before and after.

### 7.4 Battle 40 (only if §7.2 finds no measured world)

Why a battle and not a shipped scenario: the matrix is the measured world of record for combat rules, it plays 20 seeds,
and its expectations are checked on every gate; a scenario would add a world the measure script does not compare by
seed. Why this battle: it is battle 39b's tank side and Roach side with the caster removed and a plateau added, so the
only new element is terrain, placed by a rule, not to an outcome:

- `sim_fixtures::matchup::Battle` gains `#[serde(default)] terrain: Vec<ArenaRegion>`, each a cell rectangle with a
  level and an optional ramp direction, applied in order to the flat arena and checked with `sim_map::validate_map`
  (an invalid arena is a `MatchupError`).
- `40_sieged_tanks_above_a_ramp`, map 96, 600 ticks. Red: 39b's three sieged `railgun_crawler`s on Hold Position at
  (56.0, 44.0), spacing 1.8. Blue: 39b's eight `rhino_beetle`s at (38.0, 44.5), attack-moving to (60, 46). Terrain:
  cells with x ≥ 47 (midway between the sides' centres) at level 1; a ramp of width 4 centred on the sides' line
  (y 43 to 46), run 3 (x 44 to 46), ascending East, lower landing level 0.
- Claim: "Sieged tanks above a ramp: a shell on a unit that crested the ramp spares the units still on it (5.0.14
  `SameCliff`)". Expectation recorded on WP3's tree (under today's rule), re-recorded in WP4 with the probe's trace as
  its evidence (§8 rule 3).

---

## 8. Stop rules

1. **WP1 and WP2 are latent.** Every world of §7 byte-identical; goldens moved by the schema byte only, proved (§5).
2. **WP4 moves only the worlds §7.2 counted** (P-6). A difference in any other world, or on a seed with a zero count,
   is a defect of the package, not a re-record.
3. **A move whose first differing tick is not a counted event, or that breaks P-8's subset property**, stops WP4 until
   traced. If the trace shows a second cause, it is registered as a bug and WP4 lands only if the level rule's change is
   separable from it. A change of which side remains in the showcase effects scenario on any seed is traced to its
   first counted event and reported with the trace; it is not by itself a stop, as the rule is sourced (the
   deviation-fixes stop rule's exception).
4. **No tuning constant.** Every number is the data's or an existing engine rule's; the only new rule values are the
   enum's two cases and a `bool`.
5. **A golden move in WP3 to WP5** follows §5's tracing or stops the package.

---

## 9. Client consequences

- **Code: none.** The client reads splash looks from its own effect blocks (`crates/client/src/effects/catalog/`), not
  `SplashSpec`; no client file constructs a `SplashSpec` (`grep -rln "SplashSpec {" crates/client*` is empty on
  `d0fd365b`). Events are unchanged (D-6).
- **What a player sees:** a blast drawn at a cliff edge spreads over both levels while it damages one, as in the game.
  Units on the other level no longer flash damage numbers or health loss from it. No look needs a change.
- **Captures.** The showcase effects take (`assets/captures/showcase.ron`, take `effects`) plays the showcase effects
  scenario. If WP4 moves it, its storyboard checks (`crates/client_diagnostics/tests/showcase_storyboard_tests.rs`, **verify**
  its tolerance) run in the package gate; a failing storyboard is re-taken by its own procedure and the
  change is reviewed as a visual change in WP4 (R-3).
- **Tests.** `effect_catalog_tests.rs` and `effect_snapshot_tests.rs` fire weapons on flat maps and are unaffected.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP5 | D-7 |
| `docs/plans/README.md` | plan, WP1, WP5 | Row |
| `docs/evidence/2026-11-23-splash-cliff-levels.md` (new) | WP0 to WP5 | §7 |
| `crates/sim_map/src/grid.rs` | WP1 | `cliff_level_at` and its unit tests |
| `crates/sim_map/tests/ramp_profile_tests.rs` | WP1 | P-4 on ramp runs of every cardinal direction (the `sim_map` ramp subject of `docs/guides/test-gates.md`) |
| `crates/sim_types/src/weapon.rs`, `crates/sim_types/src/lib.rs` | WP2 | `SplashCliff`, the two fields, the `SplashSpec` doc (D-7) |
| `crates/sim_core/src/catalog.rs` | WP2 | `SplashSpecDef` fields and conversion |
| `crates/sim_core/src/catalog/validation.rs` | WP2 | D-3 validation and its unit tests |
| `crates/sim_core/src/combat_resolution.rs` | WP2 | D-1 and D-3 rules, the retained level buffer, module doc |
| `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/checksum.rs` | WP2 | D-5 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `sim_fixtures`' `DEFAULT_SANDBOX_CHECKSUM`; the encoding unit tests' schema byte | WP2 | §5 |
| Test files with `SplashSpec { … }` literals (`barrier_tests`, `checksum_state_tests`, `combat_correctness_tests`, `combat_retaliation_and_projectile_tests`, `damage_and_shields_tests`, `spells_and_abilities_tests`, `splash_centre_tests`, `transport_ownership_tests`, `weapon_catalog_rules_tests`) | WP2 | The two fields at their defaults (mechanical; these tests run on flat maps) |
| `crates/sim_core/tests/splash_cliff_tests.rs` (new, `//!` doc) | WP2, WP4 | §12 |
| `crates/gate/suites.ron` | WP2, only if needed | The new target (**verify**: an uncovered target may fall to the crate's default class) |
| `assets/catalog/units.ron`, `assets/catalog/structures.ron` | WP2, WP4 | D-4 |
| `crates/sim_core/tests/weapon_catalog_rules_tests.rs` | WP4 | `splash_cliff_follows_starcraft` (§2.3's table, P-5) |
| `crates/sim_core/tests/runtime_catalog_tests.rs` | WP4 | P-7 |
| `crates/sim_fixtures/src/matchup.rs`, `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP3 only | §7.4 |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP4, only if WP3 ran | Battle 40's re-recorded expectation |
| `docs/plans/2026-11-23-splash-centres.md` | WP5 | F-2 pointer |

Nothing under `crates/sim_core/src/movement/` changes; a contact-solver branch works there.

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`
(check `quota -s` and `df -h ~/scratch` first); failing tests first with their red run pasted in the hand-back;
`cargo gate quick` and the tests the change needs; a verifier runs `cargo gate package` (Q0061). §7's comparison in
every package against WP0's tree. No `unwrap`; `expect` only with `invariant:`; no catalog identity in production Rust
(C5); tests find weapons by property. Check `docs/questions/open/` before the last commit. All packages may run as
one branch and one session, one commit each, with one `package` gate on the final tree (charter §4.3).

### WP0: records, baseline and probe (XS, docs only)

- `DESIGN.md` §4: B-170 points here; B-174 registered (D-7). Evidence file created.
- Measure WP0's tree (§7.1) for the baseline; run §7.2's probe and record its counts and first events. State in the
  evidence whether WP3 runs.
- **Gate:** docs-only.

### WP1: a position's cliff level (S, `sim_map`)

Failing tests first (red on WP0's tree: they do not compile): `grid.rs` unit tests and `ramp_profile_tests.rs`
`cliff_level_at_reads_the_lower_landing_on_every_ramp_cell` (P-4: flat cells, each cardinal ramp run of length 1 and 3,
cell boundaries, the four sides and four corners off the map). Then `GameMap::cliff_level_at` (D-2). No caller yet;
latent by construction. **Acceptance:** §8 rule 1. Plan row `Active`. Commit
`feat(sim_map): the cliff level a position stands on (splash-cliff-levels WP1)`.

### WP2: the two rules, latent (M, `sim_types`, `sim_core`, data)

Failing tests first (red on WP1's tree: they do not compile), in `splash_cliff_tests.rs`, with scratch weapons copied
from a shipped circular splash weapon found by property and bodies cloned from a shipped mobile ground unit (the
pattern of `splash_centre_tests.rs`), on maps with a level-1 plateau and a ramp:
- `a_same_level_area_spares_units_on_another_level` (P-1): bystanders at equal distance on each side of a cliff edge;
  only the target's level struck; reach boundaries ± 2⁻¹⁶ on each level.
- `a_same_level_area_counts_a_ramp_as_its_lower_landing` (P-1 with D-2): target on the plateau at the ramp's top,
  bystanders on the ramp's top cell and on the plateau beside it.
- `the_target_is_struck_on_any_level` (P-1): an attacker below the plateau, the target above, the target's damage as on
  a flat map.
- `an_any_level_area_strikes_as_before` (P-2).
- `a_same_level_line_keeps_each_search_to_its_point` (D-1 per point, a scratch line with `Same` crossing an edge).
- `an_air_area_keeps_to_the_level_below` (D-2's default): a scratch air area, air bystanders over each level.
- `an_attacker_centred_area_strikes_a_target_on_another_level_directly` and `…_on_its_own_level_once` (P-3), with a
  structure target (the structure strike), a friendly target (not struck) and the fallback off (target untouched).
- `validation.rs` unit tests: the fallback with `impact: Target`, with `cliff: Any`, each refused naming the field;
  omitted fields read `Same` and `false`.
- `checksum_state_tests::embedded_weapon_fields_and_splash_are_sensitive` extended to both fields.

Then the change: D-1, D-3, D-5, the docs, and D-4's WP2 data (every shipped row authors `cliff: Any`). The schema proof
of §5. **Acceptance:** §8 rule 1. Commit
`feat(sim_core): a splash may keep to its cliff level and strike its target across a cliff (splash-cliff-levels WP2, schema N)`.

### WP3: battle 40 (S, only if §7.2 counted nothing)

`sim_fixtures` `terrain` with its unit tests (a region applied, an invalid arena refused); battle 40 (§7.4) with its
expectation recorded on this tree; the matrix test passes. **Acceptance:** §8 rule 1 for every other world. Commit
`test(sim_fixtures): matrix arenas with terrain; battle 40, sieged tanks above a ramp (splash-cliff-levels WP3)`.

### WP4: area damage keeps to its level (S, data, B-170)

Failing tests first (red on WP2's or WP3's tree):
- `weapon_catalog_rules_tests::splash_cliff_follows_starcraft` (P-5): §2.3's table, each row citing its source.
- `splash_cliff_tests::the_shipped_siege_shot_spares_the_other_level` and
  `the_shipped_burst_strikes_its_target_across_a_cliff` (P-1, P-3 on shipped rows found by property).
- `runtime_catalog_tests` (P-7): the Siege Tank's `cliff: Any` restores the struck set; the Baneling's fallback removed
  leaves a target on another level untouched.

Then the data (D-4). **Acceptance:** §8 rules 2 and 3; §7.3 column WP4 with the traces. Battle 40's expectation, if WP3
ran, re-recorded with its trace. Commit `fix(sim_core): area damage keeps to its cliff level; the Baneling strikes across one (splash-cliff-levels WP4, B-170)`,
stating which worlds moved, on which seeds, and why.

### WP5: bookkeeping (XS)

D-7's records; the evidence completed; this plan's `## As built`; README row `Done`. Merge `main`; `cargo gate package`
once on the final tree (by the verifier); the schema re-recorded if `main` took the number (§5 step 3).

---

## 12. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Map | P-4 | WP1: `grid.rs` unit tests, `ramp_profile_tests.rs` |
| Types and validation | Defaults, refused combinations | WP2: `sim_types` unit tests, `catalog/validation.rs` unit tests |
| Rule | P-1, P-2, P-3 on scratch weapons; per-point lines; air | WP2: `splash_cliff_tests.rs` |
| Encoding | Both fields change the checksum | WP2: `checksum_state_tests.rs` |
| Data | Every shipped splash's cliff rule and fallback against 5.0.14 | WP4: `weapon_catalog_rules_tests.rs` |
| Shipped behaviour | P-1, P-3 on shipped rows | WP4: `splash_cliff_tests.rs` |
| Zero recompilation | P-7 | WP4: `runtime_catalog_tests.rs` |
| Worlds | P-6, P-8: §7.3's table | every package (measure scripts); goldens and sandbox in the package gate |
| Charter subjects | `sim_map`'s and `sim_core`'s subjects of `docs/guides/test-gates.md` (`ramp_vision_tests.rs`, `damage_and_shields_tests.rs`, `full_match_tests.rs`, `golden_replay_tests.rs`, …) keep passing; `client`'s skirmish | package gate |

Re-derived (expected to change): none on flat maps. A test that fires a splash weapon on a map with levels (search
`CliffLevel(1)` and `cliff_level =` in `crates/sim_core/tests/`, **verify** in WP4) is re-read against D-1; its change
is named in WP4's commit.

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The ramp reading (D-2) is wrong | Up to a ramp's run of splash reach at every ramp | One function; the draft question (§15 part 1); the tests name the reading |
| R-2 | The air reading (D-2) is wrong | The Liberator's anti-air splash spares air units over another level | Same; §15 part 2; no measured world has the Liberator analogue near a cliff (probe) |
| R-3 | The showcase effects take moves | A capture's storyboard check may fail | Expected; re-taken by its procedure and reviewed as a visual change in WP4; which side remains is predicted unchanged (P-8) |
| R-4 | WP2's latent data leave the circles at `Any` for one commit | A reader takes the interim value as sourced | Each interim row's comment names B-170 and WP4; WP4's table test fails on any row left at `Any` |
| R-5 | Another branch takes schema 37 | Conflict at merge | "Next free at merge"; §5 step 3 |
| R-6 | The probe finds no measured world | WP4 unmeasured on any battle | WP3's battle 40 (§7.4) |
| R-7 | A line point off the map | Its level read by clamping | D-2 defines it; P-4 tests it; no shipped line keeps to a level |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: B-174, the Lurker's searches over cliff level 1 or more** (`CliffLevelGE1`, S13, S15). Needs a mapping of the
  game's cliff level numbers to ours (the game's lowest playable ground is not level 0 on every map; ours is level 0 on
  every flat test map and abyss on the showcase map). Hook: a `search_level_at_least: Option<CliffLevel>` on a line
  search, read with `cliff_level_at`. Its own plan.
- **F-2: ramp and air readings other than D-2's defaults**, if the owner's answer to the draft question differs: a
  change of `cliff_level_at` (ramp) or of `strike_area`'s plane test (air), with this plan's tests re-derived.
- **F-3: a published "level" for the client** (e.g. dimming a blast's other level): not in the game; not planned.
- **The Baneling ordered onto a friendly unit** stays splash-centres' F-3.

---

## 15. Draft owner question (for the orchestrator to file as Q0099; not filed by this plan)

```markdown
# Q0099: Which cliff level does a unit on a ramp, and an air unit, stand on?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** a ramp counts as its lower level; an air unit has the level of the ground below it;
  the level is read at the blast's centre (plan `2026-11-23-splash-cliff-levels` D-1, D-2).

## Question

In StarCraft II 5.0.14 a blast (Siege Tank, Planetary Fortress, Ultralisk, Liberator anti-air, Baneling) damages only
units on the same cliff level as its centre (`SameCliff`), and Space Scuffle is adopting that (bug B-170). The game
data do not say how three cases count:

1. **A unit on a ramp.** (a) The ramp counts as its lower level (the default: it is how our maps store ramps, and the
   Baneling's 5.0.14 fix for targets "on another level" fits a Baneling on a ramp attacking a wall at its top);
   (b) it counts as the nearer level, switching at the ramp's middle; (c) it counts as both, so a ramp never separates
   anything.
2. **An air unit.** (a) It has the level of the ground below it (the default, the literal reading); (b) air blasts
   ignore levels.
3. **Where the level is read.** (a) At the blast's centre point (the default); (b) at the struck unit the blast is
   centred on. These differ only for a target standing right at a cliff edge.

If you can, check in the editor: a sieged Siege Tank on high ground firing at a Marine standing on the top cell of a
ramp, with a second Marine one cell lower on the ramp and a third on the high ground beside the ramp's top; and a
Liberator's anti-air shot at a Viking over high ground with a second Viking over the low ground 1 apart.

## Answer

```

The question number is the next free at filing (0099 on `d0fd365b`).

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`), read with `grep` and `sed`; the chain excludes `libertymulti`, `swarmmulti` and `balancemulti`
  (S1).
- The showcase effects positions behind §7.2's expectation: `~/scratch/splash-centres/world-wp4/fx-default/units.csv`
  (splash-centres WP4's tree), read against `assets/maps/showcase_canyon.scmap.ron`'s levels and ramps (rows stored
  north first: row = 63 − y).
- No prototype: WP1 and WP2 are latent by construction, and WP4's direction is predicted (P-8) and measured in the
  package itself after WP0's probe.
