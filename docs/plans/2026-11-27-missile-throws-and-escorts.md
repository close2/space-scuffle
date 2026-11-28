# Blueprint: Missile Throws, Brood Lord Escorts and Spawn Placement — Throw Phases Fly Their Authored Direction (B-231), the Escorts Are Drawn (B-235), Placement Among Units Settled (B-236)

This plan covers three deferred rows of `DESIGN.md` §4:

- **B-231:** "Missile throw phases fly towards the target in our model; 5.0.14 throws them along `ThrowVector` with
  yaw and pitch bands and gravity" (registered by plan [`2026-11-28-missile-movers-b52`](2026-11-28-missile-movers-b52.md)
  D-7; the Brood Lord analogue's escorts joined it in plan [`2026-11-28-broodlings`](2026-11-28-broodlings.md) D-11 A-1).
- **B-235:** "The Brood Lord analogue's escorts are not drawn: 5.0.14 shows its two escorts orbiting it behind, at
  ±150° (`ExternalAngle`, `liberty` AbilData:3808–3824); here the magazine is a count with no entity" (broodlings plan
  D-1, D-11 A-3).
- **B-236:** "A missile's spawn is placed by terrain and structures only; whether other units block a 5.0.14
  `CEffectCreateUnit` place is not in the data (**verify**)" (broodlings plan D-11 A-6).

The design in one paragraph. **B-231 (simulation):** a missile phase may carry a **throw**
(`MissilePhase::throw: Option<ThrowSpec>`): during it the missile flies a straight line fixed at the release, in the
frame whose forward is the release's bearing to the target, at the authored yaw (`ThrowVector`'s sideways share, plus a
yaw drawn from `ThrowBandYaw` when one is set), covering the horizontal share of its path (`ThrowVector`'s and the drawn
`ThrowBandPitch`'s elevation). A guidance phase may carry a **turn rate** (`YawPitchRoll`, degrees per second, below the
instant threshold): the missile's heading then turns towards the target by at most that much per tick. Everything
vertical (height, `Gravity`, the pitch's own height) stays out of the 2D simulation and goes to presentation. Five
rows author throws: the Phoenix, Viking, Liberator, Spine Crawler (short band) and Brood Lord analogues; the Liberator's
also authors its turn rate and a band draw from the world's `ChaCha8Rng`. Missiles without a throw or a turn rate fly
byte for byte as today. **B-235 (presentation):** `BubbleRenderInstance` gains the number of escorts a unit holds
(built charges, armed or arming), and the client draws each at its `ExternalAngle` behind the unit, with a real
flipbook (owner question Q0144). **B-236 (records):** the data's placement rule is read and recorded; the simulation
keeps terrain-and-structure placement until the owner answers Q0145. No package edits `movement/`, `steering.rs`,
`movement_rules.ron` or `target_acquisition.rs`' admission (Q0143 freeze).

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `2a90a9b4` (checksum
schema 62).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** `main` is at schema 62; 63 is reserved for the hangar-commands
  package (plan [`2026-12-01-death-spawns-and-hangar-commands`](2026-12-01-death-spawns-and-hangar-commands.md) WPB1),
  so WP1 takes **64** provisional. The next free bug number is **B-251** (`main` has B-250; branches `rallies` and
  `minimap` hold B-247 to B-249); this plan registers none unless WP0's census finds one. Questions **Q0144** and
  **Q0145** are filed with this plan (free on `main` and every local branch at `2a90a9b4`).

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S10, S11, S20 |
| Q0045 (align times to SC2), Q0052 S-5 (25 ticks/s, half up) | Closed | §2 conversions |
| Q0020: visual questions are the agent's call | Standing rule | D-8 (escort distance, slot order) |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0103 / owner 2026-11-24: natural movement outweighs close matchup flips | Standing guidance | §6.6 (missiles are not movement; applied by analogy only to restatements) |
| Q0132 (a), Q0133 | Closed | The Brood Lord analogue's magazine and media as built |
| **Q0143** (movement redesign ended): `movement/`, `steering.rs`, `movement_rules.ron` and acquisition admission are frozen | Open, binding on this plan | D-10, stop rule 6 |
| **Q0144** (new): the escorts' look | Open, default (a) followed | D-8, WP4 |
| **Q0145** (new): placement of created units among other units | Open, default (b) followed | D-9, WP5 |

---

## 1. Summary

| | Today (`2a90a9b4`) | After this plan |
|---|---|---|
| A throw phase (`Throw` driver) | Flown as progress straight to the target | Flown along a line fixed at the release: release bearing plus the authored yaw (and a drawn band yaw), horizontal share of the path only |
| A guidance phase after a throw | Turns instantly (re-aims every tick) | Instantly where 5.0.14's turn rate is `MAX` or ≥ 180° per tick; otherwise at most `turn_rate` per tick (the Liberator analogue's 20.1°) |
| Liberator analogue (`sky_bastion`) flight, edge 0.5 / 3 / 5 | 3 / 8 / 10 ticks | about 10–21 / 16–17 / 16 ticks (§2.4, a planning estimate) |
| Brood Lord analogue (`spore_matriarch`) flight, edge 0.5 / 5 / 10 | 5 / 16 / 21 ticks | about 14 / 17 / 22 ticks |
| Phoenix, Viking, Spine Crawler analogues | §2.4 | Phoenix +1 tick at some edges; Viking and Spine Crawler equal against a still target (their throws point at it), different only against a moving one |
| Missiles without throws or turn rates (Banshee, Missile Turret, High Templar analogues, every constant missile) | — | Byte-identical |
| Randomness | — | One draw per Liberator-analogue missile from `SimWorld`'s `ChaCha8Rng` (yaw band, sign, pitch band) |
| Renderer event | Straight travel, muzzle to impact | Plus a presentation `bend` point (the throw's end, with a height from the vertical components); the client draws the travel through it (WP3) |
| Brood Lord analogue's escorts | Not drawn | Drawn behind it at ±150° while built (armed or arming) and vanish when thrown (WP4, Q0144) |
| Created units among other units (B-236) | Terrain and structures block, units do not | Unchanged; the data reading is recorded, B-236 stays open on Q0145's default (b) |
| Size | — | WP0 records, census, probe, baseline (XS); WP1 throw and turn mechanism, latent, schema (M); WP2a deterministic rows (S); WP2b the Liberator analogue (S); WP3 client throw travel (M); WP4 escorts (M); WP5 B-236 records (XS); WP6 confirmation (S) |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed. `L Kind:N` is line N of layer L's file in SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy
at `~/scratch/combat_wp3_data/<layer>/`; layers apply in the order core, liberty, libertymulti, swarm, swarmmulti, void,
voidmulti, balancemulti, a later layer overriding a field. The mover census of plan `2026-11-27-weapon-delivery`
(`~/scratch/weapon-delivery/census/movers.txt`) lists which launch flies which mover. Conversions (Q0045, Q0052 S-5):
a Normal speed `v` is `v × 0.056` world units per tick; an acceleration `a` is `a × 0.003136` per tick squared; a turn
rate `r` degrees per Normal second is `r × 0.056` degrees per tick, and a degree is `65536 / 360` `SimAngle` units.

### 2.1 Sources

Every line below was read while planning; "no override" means a grep of all eight layers' `MoverData.xml` for the id
found no other definition.

**B-231**

| # | Source | What it gives |
|---|---|---|
| S1 | `IonCannonsWeaponLeft` / `Right`, `liberty MoverData:446–484` (parent `IonCannonsWeapon`): phase 0 `Throw`, `Speed 6`, `MaxSpeed 6`, `Outro 0.4,0.85`, `ThrowRotationType ThrowForward`, **`ThrowVector ±1.75,−1.25,−0.7`**; phase 1 `Outro 1,1.5`, **`ThrowVector ∓0.75,−1.25,0.2`**; phase 2 `Guidance`, `Acceleration 60`, `MaxSpeed 40`, `YawPitchRoll MAX`; no override. Launched by `IonCannonsLMLeft`/`Right` `IfRangeLTE 6` (`liberty EffectData:1973–1979`) | The Phoenix's missiles are thrown out to one side and down, then back across and up, then guided [DATA] |
| S2 | `VikingFighterMissile`, `liberty MoverData:531–567`: phases 0 and 1 `Throw`, `Speed 2`, **`ThrowVector 0,−1,0`**, phase 1 **`Gravity 30`**, `Outro 0.125,0.25` and `0.625,0.75`, no `ThrowRotationType`; phase 2 `Guidance`, `YawPitchRoll MAX`; phase 3 `YawPitchRoll 5568.75`; no override | The torpedoes are dropped straight forward, falling, then guided [DATA] |
| S3 | `LiberatorMissile`, `void MoverData:384–422`: phase 0 `Throw`, `Speed 1`, `Acceleration 3200`, `MaxSpeed 8`, `Outro 2.5,3.5`, `ThrowRotationType ThrowForward`, `ThrowVector 0,−1,0`, **`ThrowBandYaw 74.9926,−74.9926,64.995,−64.995`**, **`ThrowBandPitch 45,−45,4.9987,−4.9987`**; phase 1 `Guidance`, `Acceleration 3200`, `MaxSpeed 16`, `Outro −2.5,−1.5`, **`YawPitchRoll 359.2968`**; phase 2 `Guidance`, bands `−360`, **`YawPitchRoll 5759.6484`**; a `Revolver` overlay; no override | Each missile is thrown forward deflected 65° to 75° to a side and 5° to 45° up or down, then guided with a limited turn, then a near-instant turn once 2.5 to 1.5 from the target [DATA; the band's four values read as outer and inner bound on each side, INF, as plan `2026-11-28-missile-movers-b52` S6 read them] |
| S4 | `SpineCrawlerTentacleExtendShort`, `liberty MoverData:243–265`: phase 0 `Throw`, `Speed 1`, `Acceleration 10`, `MaxSpeed 20`, `Outro 0.2,0.4`, `ThrowRotationType LookAtTarget`, **`ThrowVector 0,−1,0.6`**; phase 1 `Guidance`, `Acceleration 140`, `MaxSpeed 200`, **`YawPitchRoll 180`**, `YawPitchRollAccel 1440`; `IfRangeLTE 3` (`ImpalerTentacleLM`); the long band `…ExtendLong` (:266–278) has no throw | Short shots are lifted forward and up, then strike [DATA] |
| S5 | `BroodLordWeaponRight` / `Left`, `liberty MoverData:44–69`, :620–644: phase 0 `Throw`, `Speed 5`, `Acceleration 3`, `MaxSpeed 8`, **`Gravity 5`**, `Outro 1.5,4.5`, `ThrowRotationType LookAtTarget2D`, **`ThrowVector ±0.8,−1,0.75`**; phase 1 `Guidance`, `Acceleration 20`, `MaxSpeed 40`, `YawPitchRoll MAX`, `Overlays Scale 0.75`; `MotionOverlays` `Wave`, `Polarity Random`, `Axis 1,0,0`, `Wavelength 6`; no override | The escorts are thrown forward, to a side and up, falling, then guided with a sideways wave [DATA] |
| S6 | `MissileDefault`, `core MoverData:13–25`: `Guidance`, `YawPitchRoll 719.2968`, then a kill phase `YawPitchRoll MAX` | The default turn rate of most missiles; every shipped `MissileDefault` weapon is a constant missile here and keeps that model (D-5) [DATA] |
| S7 | `CMover` defaults, `core MoverData:3–7`: no default `ThrowRotationType`, `ThrowVector` or band | A phase without these fields has none [DATA] |
| S8 | Census of every shipped missile launch (`movers.txt`): `Throw` appears only in S1–S5's movers | No other shipped row throws [DATA] |
| S9 | `voidmulti`, `balancemulti` `MoverData` (16 `Throw` entries each): none of S1–S5's ids (grep) | No later layer changes a shipped throw [DATA] |
| S10 | SC2 editor guides, *Missile Movers* (<https://s2editor-guides.readthedocs.io/Classic_Tutorials/04_Misc/missile-movers/>, read 2026-11-27): `Throw` "travels along an arbitrary linear path in a specified direction"; Throw Vector: "The local coordinates of the throw. They do not need to be normalized."; Throw Rotation Type `None` "travels in the direction of the throw if it is the first phase", `ThrowForward` "face[s] in the direction of the throw", `LookAtTarget` "face[s] directly at its impact point", `LookAtTarget2D` the same "parallel to the ground"; Throw Band Yaw "throw variance along the yaw plane, as a deflection with respect to the core throw axis", Pitch "like Throw Band Yaw, but for pitch"; Gravity "the gravity experienced by the missile"; local axes "0,−1,0 (forward) or 0,1,0 (backwards)" (said of an overlay's `Axis`); turn limits are "degrees per second"; overlays "do not actually alter the path of the motion driver underlying them" and do not count toward outros | `ThrowVector`'s −y is forward; the rotation type sets only the missile's facing; a wave changes no flight [EXT, editor documentation; that the throw's local frame is the overlay's frame is INF] |
| S11 | Same guide: Rotation Launch Actor Type `None` "the missile actor's launch rotation matches the missile's game rotation"; the guide does not define the frame of a missile's game rotation at launch | What "forward" is at the release is **not documented** (plan b52 D-7 found the same, Q0008 searched 2026-11-26); D-2 takes the release's bearing to the target [INF, **verify**] |

**B-235**

| # | Source | What it gives |
|---|---|---|
| S12 | `BroodLordHangar` (`CAbilArmMagazine`), `liberty AbilData:3808–3824`: `InfoArray Ammo1 … Unit BroodlingEscort`, **`Flags External 1`**, **`ExternalAngle −149.9853`**, **`149.9853`**; `voidmulti`, `balancemulti AbilData:84–86` change `Leash` only | Two escort slots held outside the unit at ±150° from its facing [DATA; "from its facing" INF] |
| S13 | `BroodlingEscort`, `liberty UnitData:11194–11214` (parent `BroodlingDefault` :79–109, `Radius 0.375`): `PlaneArray Air`, **`Height 4.25`**, `Unselectable`, `Untargetable`, `Invulnerable`; Brood Lord `liberty UnitData:11102–11165`: **`Height 3.75`**, `Radius 1` | Escorts hover half a unit above the Brood Lord; their size is a Broodling's [DATA] |
| S14 | No field gives the slot's distance from the unit (`CAbilArmMagazine` defaults, `core AbilData:108–114`, have none) | The orbit radius is not in the data [DATA, a negative] |
| S15 | A web search on 2026-11-27 quoted an SC2Mapster forum thread ("separating cluttered units", the page itself redirects and could not be read): units of an external magazine are placed taking "the producing unit's radius into account" | The slot lies at the owner's radius, or just outside it [EXT, low: a search excerpt] |
| S16 | The broodlings plan S5, S6 (`BroodlingAttackDelay` 9 ticks from creation) and its D-3 as built (`StrikeMagazineState::{armed, arming_left, build_left}`) | An escort exists from the end of its build: armed, or arming for 9 ticks; a build in progress has no escort yet [DATA, as built] |

**B-236**

| # | Source | What it gives |
|---|---|---|
| S17 | `CEffectCreateUnit` defaults, `core EffectData:41–55`: **`CreateFlags Placement 1`**, `OffsetByRadius 1`, `SpawnRange 500` | A created unit is placed by a placement check unless an effect clears the flag [DATA] |
| S18 | `BroodlingEscortImpactA`, `liberty EffectData:2684–2692`: no `Placement` override (so 1), `SpawnRange 5`; `ZergBuildingSpawnBroodling6`/`9` (:2791–2808): **`Placement 0`** | The Brood Lord's Broodlings are placed by the check; the death spawns are not [DATA] |
| S19 | `CMoverAvoid Ground`, `core MoverData:8–12`: `PlacementArray Prevent` **`GroundNoSpawn`**; mobile units carry no footprint field in `UnitData` (the Marine's block: `Radius`, `SeparationRadius`, `InnerRadius`, `Collide`; footprints live in `FootprintData`, which our copy lacks) | The placement check reads footprint layers; whether ground units take part in it is not in the data [DATA for the fields; that units do not block is INF, **verify**] |
| S20 | Web search, 2026-11-27 ("CEffectCreateUnit Placement … overlap"): no source describes the check against units | Nothing found [EXT: nothing] |

### 2.2 The rules that follow

**B-231**

1. **The frame** (S10, S11). A throw's `ThrowVector` `(x, y, z)` is in the missile's local frame, −y forward and
   +z up. Forward is the horizontal bearing from the launch point to the target at the release [INF, **verify**; the
   alternative, the launcher's facing, is the same for every launcher that must face its target to fire and is
   recorded in D-11 A-1].
2. **The throw's direction** (S1–S5). Its yaw from forward is `atan2(x, −y)`; its elevation `atan2(z, √(x²+y²))`.
   With a yaw band, the yaw is drawn uniformly within the band's magnitude (65° to 75° for the Liberator) on a side
   drawn at random; with a pitch band, the elevation is drawn within the band's magnitude (5° to 45°), up or down.
3. **The throw's path** (S10). A throw flies a straight line at the phase's speed profile; its `Outro` counts the
   path along that line (S10: "distance along the core motion driver"). Its horizontal displacement is the path
   times the cosine of the elevation. The rotation type sets the missile's facing only.
4. **Gravity** (S2, S5) bends the throw downward. In a 2D simulation it changes no horizontal displacement [INF:
   gravity is vertical]; it is presentation (D-6).
5. **Guidance** (S1–S6). A guidance phase turns towards the target at most its `YawPitchRoll` per second; `MAX` is
   instant. 5759.6 (the Liberator's phase 2) and 5568.75 (the Viking's phase 3) are above 180° per tick, so they are
   instant too.
6. **The turn's acceleration** (S4's `YawPitchRollAccel 1440`) ramps the Spine Crawler tentacle's turn up from 0; its
   throw points at the target, so its turn only follows a moving target (D-11 A-5).
7. **Overlays** (S3's revolver, S5's wave) move what is drawn, not the flight, and count toward no `Outro` (S10).
8. **The Phoenix's and Brood Lord's two missiles** are mirror images (S1, S5): against a still target they take equal
   flight times.

**B-235**

9. **The escorts** (S12, S13, S16). While a Brood Lord holds escorts, each is drawn at ±150° from its facing, about
   its radius out (S14, S15) and half a unit higher (S13). An escort appears when its build finishes and leaves its
   slot when thrown.

**B-236**

10. **The check** (S17–S19). The Brood Lord's Broodlings are placed by 5.0.14's placement check, whose data names
    footprint layers only. Whether that check also avoids other ground units is not in the data and no outside
    source says (S20).

### 2.3 Findings and confidence

| Finding | Confidence | Basis |
|---|---|---|
| Five shipped movers throw; their vectors, bands, gravity and turn rates | High | S1–S5, S8, S9 [DATA] |
| −y is forward, +z up in `ThrowVector` | Medium-high | S10 [EXT]; agrees with every shipped vector (all have −y, the Viking's drop and the Liberator's band are around forward) |
| Forward is the release's bearing to the target | Medium, **verify** | S11 [INF]; equal to the launcher's facing for a launcher facing its target |
| Which side `+x` is (left or right) | Low | Not documented; changes no flight time (rule 8), only which side the drawn path bends to (D-6) |
| Band values are outer and inner bounds per side | Medium-high | S3 [DATA]; b52 S6 [INF] |
| Gravity and pitch change no horizontal flight | High for the geometry, medium for `Outro` (counted along the driver in 3D) | S10 [EXT], rule 3 |
| A wave or revolver overlay changes no flight | High | S10 [EXT] |
| Escort slots at ±150°, 0.5 above the Brood Lord | High for the angles and heights | S12, S13 [DATA] |
| Escort slot distance | Low | S14 (absent), S15 [EXT, low] |
| Created units avoid other ground units | Unknown | S17–S20 |

### 2.4 Flight ticks (planning script `~/scratch/missile-throw-plan/flight.py`, §16; floats, still target)

Path model as `advance_projectile` (impact when the centre distance is at most the tick's step plus the target's
radius); throws as rules 1 to 5; the Liberator's band at three corners (yaw/pitch); the Spine Crawler's turn without its
ramp. Edge distance as the range check reads it; target radius 0.5 (0.75 for the Viking's, 0.375 for ground).

| Edge distance | 0.5 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Liberator today | 3 | 4 | 7 | 8 | 9 | 10 | — | — | — | — | — |
| Liberator 65°/5° | 21 | 20 | 18 | 17 | 16 | 16 | — | — | — | — | — |
| Liberator 70°/25° | 21 | 20 | 18 | 17 | 16 | 16 | — | — | — | — | — |
| Liberator 75°/45° | 10 | 20 | 18 | 16 | 16 | 16 | — | — | — | — | — |
| Phoenix today / thrown | 4 / 5 | 5 / 6 | 7 / 7 | 8 / 8 | 9 / 9 | 10 / 10 | — | — | — | — | — |
| Viking today / thrown | 9 / 9 | 10 / 10 | 12 / 12 | 13 / 13 | 14 / 14 | 14 / 14 | 15 / 15 | 16 / 16 | 16 / 16 | 17 / 17 | — |
| Brood Lord today / thrown | 5 / 14 | 7 / 13 | 10 / 14 | 12 / 15 | 14 / 16 | 16 / 17 | 17 / 19 | 18 / 20 | 19 / 21 | 20 / 22 | 21 / 22 |
| Spine Crawler short today / thrown | 4 / 4 | 5 / 5 | 6 / 6 | 7 / 7 | — | — | — | — | — | — | — |

Over a grid of the Liberator's band (yaw 65° to 75°, pitch 5° to 45°) and edges 0 to 5 in steps of 0.1, every
missile lands, at most 21 ticks after the release; none circles its target (its turning radius, 0.896 ÷ 0.351 = 2.55,
is larger than phase 2's 2.0 instant-turn window only near the launcher; acceptance T-9 pins this in fixed point).
B-231's own bound ("up to about 7 ticks for the Liberator") was too small: the throw's sideways leg and the turn back
cost up to 18 ticks at point-blank range. The Brood Lord's short-range flights lengthen most (its 3-unit throw passes
beside a close target).

---

## 3. Diagnosis (on `2a90a9b4`, read from the code)

1. **Types.** `MissileMover { up_to_range, phases }` and `MissilePhase { speed, acceleration, max_speed, ends }`
   (`crates/sim_types/src/weapon.rs`); the phase's doc says "the path's shape (`Throw`, `Guidance`) is not modelled".
   `PhaseEnd::{Flown, Remaining}`.
2. **Flight** (`combat_resolution/missile_flight.rs`): `choose_mover`, `launch_speed`, `MoverTick::of`,
   `enter_due_phases`, `current_phase`, `flight_ticks` (presentation: the renderer event's duration). Integers only,
   invariant 1 (no division by a variable, no square root).
3. **Stepping** (`combat_resolution.rs`, `advance_projectile`): each tick refreshes the aim point
   (`track_projectile_target`), tests impact (`delta.length() <= step + target_radius`), then moves
   `delta.normalize_or_zero() * step` towards the aim point; a mover missile adds `step` to `travelled` and enters due
   phases. **The missile always flies at its target.**
4. **Launch** (`launch_projectile`): chooses the mover by edge distance, computes `flight_ticks`, emits
   `CombatEventVisual::ProjectileVisual { pos, vel, duration_secs, … }` along the straight line, pushes
   `ActiveProjectile` with `mover`, `phase`, `travelled` (no heading).
5. **Hashing.** `ActiveProjectile` is encoded field by field (`checksum/encoding.rs`, `fields!(ActiveProjectile …)`),
   its `weapon_spec` included, so any new `MissilePhase` field reaches the hash through projectiles in flight.
6. **Randomness.** `SimWorld`'s `ChaCha8Rng` (hashed); no projectile draws from it today.
7. **Rows** (`assets/catalog/units.ron`, `structures.ron`): `flying_saucer` (Phoenix: `≤ 1` constant 1.05, then a
   merged throw to `Flown(1.25)` and guidance), `vector_interceptor` (Viking: merged drop to `Flown(0.6875)`),
   `sky_bastion` (Liberator: 0.448 to `Flown(3.0)`, then 0.896; phases 1 and 2 merged), `spore_matriarch` (Brood
   Lord: two weapons, 0.28 accelerating to `Flown(3.0)`, then guidance to 2.24), `thorn_lasher_dome` (Spine Crawler:
   `≤ 3` short band with a 0.3 lift). Plan b52's merges relied on "the path's shape is not modelled"; with throws, the
   Phoenix's two throw phases (different vectors) and the Liberator's two guidance phases (different turn rates)
   separate again.
8. **Client travel** (`crates/client/src/effects/systems.rs`, `travel_request`): a projectile's travel effect runs
   along a `WorldLine` from `pos` to `pos + vel × duration`, with a fog clip computed along that line
   (`shot_clip`). No other projectile state reaches the client.
9. **Escorts** (broodlings plan D-1): `Entity::strike_magazine: Option<StrikeMagazineState { armed, arming_left,
   build_left }>` (hashed); nothing about it reaches `RenderSnapshot` (`BubbleRenderInstance` has no such field).
   `spore_matriarch` and `brood_mite` have installed media (broodlings plan as built, Q0133).
10. **Placement** (`combat_resolution/impact_spawn.rs`, `spawn_place`, and `world/spawns.rs` from the death-spawns
    plan): a ring search through `MotionPolicy` over terrain and structures; other units never block (plan
    `2026-11-10` D-8). Collisions separate overlaps on the next tick's movement.
11. **The tick**: commands → upkeep (magazines) → unit tasks → vision → hangars → lost swings → acquisition → attack
    dashes → movement → collisions → weapons and projectiles → cast locks → reap. Projectiles read nothing that
    movement writes after them.

---

## 4. Design decisions

### D-1. The throw and the turn rate are phase data (`sim_types::weapon`)

```rust
pub struct MissilePhase {
    // … speed, acceleration, max_speed, ends as today …
    /// The straight line this phase flies instead of homing (`StarCraft` II's `Throw` driver;
    /// plan `2026-11-27-missile-throws-and-escorts` D-1). `None`, the default, is guidance.
    #[serde(default)]
    pub throw: Option<ThrowSpec>,
    /// Greatest turn of a guidance phase's heading per tick (`YawPitchRoll`, converted); `None`,
    /// the default, re-aims at the target every tick. Refused on a throw phase.
    #[serde(default)]
    pub turn_rate: Option<SimAngle>,
}

/// A throw's direction in the release's frame (forward is the bearing to the target), resolved
/// from `ThrowVector` and its bands at catalog load in fixed point.
pub struct ThrowSpec {
    /// Yaw from forward, counter-clockwise positive (`atan2(x, −y)` of `ThrowVector`).
    pub yaw: SimAngle,
    /// Horizontal share of the path flown (`cos` of the vector's elevation), in (0, 1].
    pub horizontal: SimScalar,
    /// A yaw drawn per missile in place of `yaw`: magnitude between the bounds, side at random
    /// (`ThrowBandYaw`). `None` without a band.
    pub yaw_band: Option<AngleBand>,
    /// An elevation drawn per missile in place of the vector's (`ThrowBandPitch`); `None` without.
    pub pitch_band: Option<AngleBand>,
    /// The vertical part, for presentation only: the vector's up share and the phase's gravity
    /// (world units per tick squared). Read by the renderer event (D-6), never by the flight.
    pub rise: SimScalar,
    pub gravity: SimScalar,
}

/// An angle band: magnitudes from `inner` to `outer`, either side.
pub struct AngleBand { pub inner: SimAngle, pub outer: SimAngle }
```

- **The RON form** (`MissilePhaseDef` in the catalog): `throw: Some((vector: (0.8, -1.0, 0.75), yaw_band: None,
  pitch_band: None, gravity: 5.0))`, angles in degrees, `turn_rate: Some(20.12)` in degrees per tick. Resolution in
  fixed point only: `SimScalar::sqrt` and `SimAngle::atan2`, `cos` (the LUT), never `f64` trigonometry (whose results
  differ between platforms); the authored floats are converted once by `SimScalar::from_num`, as every catalog value.
  `gravity` is converted per tick squared (× 0.003136).
- **Validation** (`catalog/validation.rs`, typed `SimCoreError` naming the slug and
  `missile_movers[i].phases[j].<field>`): a vector is not zero and its horizontal part is not zero (a vertical throw
  has no 2D direction: refused rather than guessed); `0 < inner ≤ outer ≤ 90°` for a band; a throw phase has a
  `Flown` end (a throw never ends by nearing the target) and is not the last phase (a missile must end guided);
  `turn_rate` only on a phase without a throw, positive and below 180° (≥ 180° is authored `None`, rule 5); at most one
  banded phase per mover (one draw per missile, D-3).
- **Rejected: a 3D missile** (height as state, `Gravity` integrated, `Outro` measured in 3D). The simulation has no
  launch or impact heights for weapons; a height state would be precision the rest of the world lacks. Recorded with
  its bound in D-11 A-2.
- **Rejected: keep the speed-only model and correct flight time by a table.** It would give the time but not the
  path, so a moving target, the turn and the drawn line would all disagree with the game.

### D-2. The frame and the state (`entity.rs`, `combat_resolution.rs`)

`ActiveProjectile` gains, after `travelled`:

```rust
/// The release's forward: the bearing from the launch point to the aim point at the release
/// (plan `2026-11-27-missile-throws-and-escorts` D-2). Zero for a missile without a throw.
pub frame: SimAngle,
/// The direction the missile flies now: set on entering a throw phase (frame + yaw), turned by
/// a turn-limited guidance phase; zero for a missile that has neither.
pub heading: SimAngle,
/// The yaw and elevation share drawn at the release for its banded phase, if any (D-3).
pub throw_draw: Option<ThrowDraw>,
```

`ThrowDraw { yaw: SimAngle, horizontal: SimScalar }`. Hashed (§5).

### D-3. The release (`launch_projectile`)

1. Choose the mover as today.
2. If the mover has any throw or turn-limited phase: `frame = SimAngle::atan2(delta.y, delta.x)` of the release's
   delta (launch to aim point). If `delta` is zero (a target on the launcher), the frame is the launcher's `heading`.
3. If the mover has a banded throw phase: **one** draw from `SimWorld`'s rng, `next_u32()`, split into fields: the
   yaw magnitude `inner + (bits % (outer − inner + 1))` in `SimAngle` units, the side from one bit, the pitch magnitude
   likewise from the remaining bits (or the vector's own elevation without a pitch band); the horizontal share is the
   LUT `cos` of the pitch. Draws happen in launch order, which is deterministic (the weapon phase's order). Only the
   Liberator analogue draws.
4. `enter_due_phases` as today; entering a throw phase (at the release or later) sets `heading = frame + yaw` (drawn or
   authored).
5. `flight_ticks` (presentation) steps the same geometry against the aim point standing still (D-4), with the draw.

### D-4. The step (`advance_projectile`, `missile_flight.rs`)

Per tick, after `track_projectile_target` and before the impact test, the current phase is read:

- **Throw phase:** the impact test is unchanged (a missile passing within its step and the target's radius lands).
  Otherwise `current_pos += dir(heading) × step × horizontal`, where `dir` is `(cos, sin)` from the LUT and
  `horizontal` the phase's (or the draw's) share; `travelled += step` (the path along the line, rule 3).
- **Guidance phase with `turn_rate`:** `heading = heading.rotate_towards(bearing to the aim point, turn_rate)`;
  `current_pos += dir(heading) × step`.
- **Guidance phase without `turn_rate`:** exactly today's step, `delta.normalize_or_zero() × step`, so every mover
  without throws or turn rates (Banshee, Missile Turret, High Templar analogues) and every constant missile flies byte
  for byte as before (invariant kept by test T-1). After a throw, the first guided tick re-aims at once (rule 5).
- Phase ends are entered after the move as today; `Remaining` reads the 2D remaining path as today.

`missile_flight.rs` gains `ThrowGeometry` helpers (`dir`, the per-phase heading on entry) and `flight_ticks` takes the
frame and draw. Its invariant 1 is restated: integers only, `SimScalar::sqrt` (already used by `length`) and LUT
trigonometry, no float. A turn-limited missile that circles its target is bounded by `max_projectile_lifetime_ticks`
(fuel-out, `Disjointed`); T-9 proves no shipped row does over its range and band.

### D-5. Which rows change (WP2a, WP2b)

| Row (5.0.14) | Phases after | Source |
|---|---|---|
| `flying_saucer` (Phoenix), band `None` | throw `(1.75, −1.25, −0.7)` 0.336 to `Flown(0.625)`; throw `(−0.75, −1.25, 0.2)` carrying 0.336 to `Flown(1.25)`; guidance 0.18816 to 2.24 (instant); the `≤ 1` band unchanged | S1 (Right is the mirror; one projectile carries both strikes, rule 8) |
| `vector_interceptor` (Viking) | throw `(0, −1, 0)`, gravity 30, 0.112 to `Flown(0.6875)`; guidance as today (instant: phase 3's 311.9°/tick) | S2 (phases 0 and 1 stay merged: same vector) |
| `sky_bastion` (Liberator) | throw `(0, −1, 0)`, yaw band 65°–75°, pitch band 5°–45°, 0.448 to `Flown(3.0)`; guidance 0.896, `turn_rate 20.12°`, to `Remaining(2.0)`; guidance carrying 0.896, instant | S3 (phases 1 and 2 separate again: their turn rates differ) |
| `thorn_lasher_dome` (Spine Crawler) `≤ 3` | throw `(0, −1, 0.6)` 0.056 accelerating to `Flown(0.3)`; guidance with `turn_rate 10.08°` | S4 (the ramp not modelled, D-11 A-5); the long band unchanged |
| `spore_matriarch` (Brood Lord) both weapons | throw `(0.8, −1, 0.75)`, gravity 5, 0.28 accelerating to `Flown(3.0)`; guidance as today (instant) | S5 (Right; Left mirrors, rule 8) |

Each row's comment cites its S-source, as the b52 rows do. Turn rates: 359.2968 × 0.056 = 20.12°; 180 × 0.056 =
10.08°. WP2a carries the four rows that draw nothing (Phoenix, Viking, Spine Crawler, Brood Lord); WP2b the Liberator
(randomness and the turn), so a world move in either is traced to one cause.

### D-6. Presentation of the throw (`sim_types::snapshot`, client, WP3)

`CombatEventVisual::ProjectileVisual` gains `#[serde(default, skip_serializing_if = "Option::is_none")] bend:
Option<[f32; 3]>`: the point where the release's predicted path (D-3 step 5, still target) leaves its last throw phase,
its height the throw's rise less its fall under gravity over that path (presentation arithmetic in `f32` on the way out,
charter §3.2). `None` for every missile without a throw, so their frames keep their bytes. The client's
`travel_request` builds the travel along a quadratic path from the muzzle through the bend to the impact
(`WorldLine` gains an optional control point, `WorldLine::bent`), and `shot_clip` samples along that path (fail closed
as today). The Brood Lord's wave and the Liberator's revolver (overlays, rule 7) are not drawn (§14). No shader
changes: the travel layers already place their particles by line parameter (**verify** in WP3; stop rule 7).

### D-7. The escort count in the snapshot (WP4)

`BubbleRenderInstance` gains `#[serde(default, skip_serializing_if = "is_zero")] escorts: u8`: for a unit with a strike
magazine, `armed + (arming_left > 0)` (rule 9, S16); 0 otherwise, so frames without a Brood Lord keep their bytes.
Written where bubbles are filled (`SimWorld::write_render_snapshot` path), read-only on `StrikeMagazineState` through
`Entity::strike_magazine()`. Not hashed (the snapshot is presentation).

### D-8. Drawing the escorts (client, WP4)

- **Data.** The unit's presentation block in `units.ron` (`effects`) gains `escorts: Some((look: "<slug>", angles:
  [-149.9853, 149.9853], lift: 0.5))` (S12, S13), parsed by the client's catalog reader. Slots fill in the order
  listed and empty from the last (agent's call, Q0020). The slot's distance from the centre is the host's drawn
  radius plus the look's drawn radius (S14, S15; D-11 A-6).
- **Look.** Q0144 default (a): the escort is drawn as a small bubble of the `look` archetype's own flipbook and
  radius, the `brood_mite` (5.0.14's `BroodlingEscort` is a `Broodling` in flight: parent `BroodlingDefault`, S13).
  Its media is real and installed; no clip is generated (charter §1.2). Answer (b) would add a paid Veo batch as an
  amendment; (c) removes the drawing and keeps the count in the snapshot.
- **Retained.** Each host's escort bubbles are children created once with the host's render entity (capacity = the
  number of `angles`), shown or hidden per frame by the count; they follow the host's pose (heading, position, flight
  height + lift) and are parked with it by the render pool (`render_pool.rs`), so the steady state allocates nothing
  (charter §3.3; T-15).
- **Presentability.** A unit whose presentation names an escort look is presentable only if the look is
  (`media.rs`), as `created_units` is today.

### D-9. B-236: placement among units (records; Q0145)

The data names footprint layers for the placement check (S19) and nothing about units; no outside source says (S20).
Making units block would change every impact spawn's place by an inference, so the plan does not. WP5 records S17–S20
in B-236, restates it as "kept; Q0145", and files Q0145 with three options: (a) close B-236 as matching the data's
reading (terrain and structures), (b) keep it open as a minor difference until someone can check the game (**default**),
(c) the owner describes what the game shows. If the answer is (c) and says units block, an amendment adds a unit-aware
ring search to `world/spawns.rs`' shared placement (reading the spatial hash; no `movement/` file), with its own tests
and a world check; nothing else in this plan depends on it.

### D-10. The freeze (Q0143)

No package edits `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`.
Projectiles are stepped in `combat_resolution.rs`; the escorts are not entities; placement is in `world/spawns.rs` and
`impact_spawn.rs`. Anything that would need a frozen file stops the package (stop rule 6) and goes to §14.

### D-11. Approximations and deviations, each recorded

| # | What | Ours | Game | Record |
|---|---|---|---|---|
| A-1 | The throw's frame | Forward = release bearing to the target | Undocumented (S11) | **verify**; equal to the launcher's facing whenever it faces its target; B-231's fixed text names it |
| A-2 | Height | Throws fly their horizontal share; no height state; guidance closes the 2D gap only | 3D flight from a launch height to an impact height | Detail: the Liberator's worst rise, 3 × sin 45° = 2.1, adds at most about 0.5 to a 4-unit 3D path (under one tick at 0.896) |
| A-3 | `Outro` blend | The midpoint (weapon-delivery D-3) | A blend from the first value to the second | Detail, as today |
| A-4 | Two mirrored missiles in one projectile (Phoenix, Brood Lord) | One path (Right) carrying both strikes | Two paths, mirror images | None for still targets (rule 8); against a mover the two differ by the throw's side, detail |
| A-5 | The Spine Crawler's turn ramp (`YawPitchRollAccel`) | Full rate from the start | Ramps from 0 | Detail: its throw points at the target |
| A-6 | Escort slot distance | Host drawn radius + escort drawn radius | Not in the data | Presentation, Q0020 |
| A-7 | Escort launch point | The Brood Lord's centre (as today) | The escort's slot | Detail (broodlings plan A-3), within the slot's radius |
| A-8 | Overlays | Not drawn | Wave (Brood Lord), revolver (Liberator) | §14 |

---

## 5. Determinism, checksum and coordination

- **Integers only.** `SimAngle` (u16) and `SimScalar`; LUT `sin`/`cos`/`atan2`, `SimScalar::sqrt` at catalog load;
  the draw is one `u32` from the world rng split by integer arithmetic. No float enters state; `bend` is presentation.
- **Order.** Draws happen in launch order (the weapon phase's existing order); a world that launches no
  Liberator-analogue missile draws nothing new, so its rng stream is untouched.
- **No new collection, clock or global.**
- **Hashed (WP1, schema 64 provisional, next free at merge):** `ActiveProjectile::{frame, heading, throw_draw}` after
  `travelled`; `MissilePhase::{throw, turn_rate}` after `ends` (inside `weapon_spec`). `checksum.rs` gains: "Schema N
  adds each missile phase's throw and turn rate, `MissilePhase::{throw, turn_rate}` (option tags, then the throw's yaw,
  horizontal share, bands, rise and gravity, and the turn), after `ends`, and each projectile's frame, heading and
  throw draw, `ActiveProjectile::{frame, heading, throw_draw}`, after `travelled` (plan
  `2026-11-27-missile-throws-and-escorts` D-1, D-2). The number is the next free at merge."
- **The four-build golden proof (WP1)**, as the death-spawns plan §5: with the schema at `main`'s number and the new
  fields left out of the encoding in a proof copy (`~/scratch/death-spawns-plan/scripts/proofcopy.sh`, `unhash.py`,
  copied to `~/scratch/missile-throw-plan/scripts/`), the tree reproduces every checkpoint of `golden_replay.ron`,
  `golden_replay_orders.ron` and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release and `wasm32-wasip1` debug and
  release (8 of 8). Then the schema steps and **WP1 re-records** the goldens and sandbox under "Re-recorded for plan
  2026-11-27-missile-throws-and-escorts WP1 (checksum schema M -> N): missile throws and turn rates encoded; with the
  schema at M and the fields unhashed every checkpoint reproduced. Behaviour: none (no shipped throw yet)", proved on
  the four builds. If the hangar-commands package has not taken 63 by WP1's merge, the number is still the next free
  after 63's reservation (64); if `main` moves past it, the merge repeats the proof with the next free number.
- **WP2a, WP2b** change data only: if WP0's probe finds a golden or the sandbox launching a throw missile, that
  package re-records the moved golden with a header naming the rows and the class (T); otherwise goldens are untouched.
- **Replays.** No command changes; replays recorded under schema 62 are refused by schema, as for every step.
- **Throughput.** `cargo bench -p sim_core --bench throughput`, release, base and final trees interleaved, two rounds
  each: every scenario at **50 ticks per second or more**, instructions of the existing scenarios within **+0.5 %**
  (`perf stat -e instructions:u`, `DESIGN.md` §5's method). Per tick the new work is one `Option` check per mover
  missile; a throw or turn-limited missile adds two LUT reads. `ActiveProjectile` grows by about 12 bytes;
  `MissilePhase` grows, but lives behind `WeaponSpec`'s `Vec`, so `WeaponSpec` keeps its size (the broodlings plan's
  +2.31 % came from growing `WeaponSpec` itself; **verify** with `size_of` in WP1). WP6 adds a **`salvo`** row: 40
  `sky_bastion`, 40 `flying_saucer` and 20 `spore_matriarch` against 60 `vector_interceptor` and ground units
  (`throughput_scenarios.ron` names slugs, being data), so throws, draws and turns are measured; its figure enters
  `DESIGN.md` §5.
- **Client steady state** (WP3, WP4): no per-frame allocation; escort children retained; the bent travel reuses the
  travel's spawn path.

---

## 6. Acceptance (fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` and use shipped archetypes
found by property (a weapon with a banded throw; one with an unbanded throw; one with a turn rate; one with movers but
neither; a constant missile; a unit with a strike magazine), never by slug, except the shipped-values tables. Every
test is written red first against the previous package's tree.

### 6.1 Mechanism (WP1; unit tests in `missile_flight.rs`, `catalog` validation, `encoding.rs`; integration in `missile_mover_tests.rs`)

- **T-1 (no change without throws).** For every shipped weapon whose movers have no throw or turn rate, and for every
  constant missile, a fight's projectile positions, phases, `travelled` and impact ticks equal `main`'s reference
  recorded in the test (a table from WP0's tree) over a grid of release distances and a target walking across.
- **T-2 (throw line).** A scratch mover with an unbanded throw (yaw 90°, horizontal 1) against a still target: each
  throw tick moves the missile perpendicular to the release bearing by exactly `step` (fixed point), `travelled` grows
  by `step`; the phase ends at its `Flown`; the first guided tick re-aims.
- **T-3 (horizontal share).** The same with elevation 60°: displacement per tick `step × cos 60°` (LUT value),
  `travelled` the full step.
- **T-4 (turn limit).** A guidance phase with `turn_rate` r after a sideways throw: the heading changes by exactly r per
  tick until aligned, then follows the bearing; the instant phase after `Remaining` re-aims at once.
- **T-5 (frame).** Rotating the whole scene (launcher, target) by 90°, 180°, 270° rotates every recorded position
  (within LUT rounding stated in the test, 1 raw unit per step); a target on the launcher uses the launcher's heading.
- **T-6 (draw).** With a banded phase, exactly one rng draw per missile (the world rng's word position advances by one);
  yaw magnitude within `[inner, outer]` and both sides occur over 256 seeded draws; a mover without a band draws nothing
  (rng position unchanged).
- **T-7 (flight_ticks agrees).** For every shipped mover and a grid of distances (and band corners), `flight_ticks`
  equals the tick the stepped missile lands on against a still target.
- **T-8 (validation).** Each D-1 refusal with its field path: zero or vertical vector, empty or inverted band, throw on
  the last phase, throw with a `Remaining` end, `turn_rate` on a throw, `turn_rate ≥ 180°`, two banded phases.
- **T-9 (no orbit).** For every shipped turn-limited mover, every release edge distance 0 to its weapon's range in
  steps of 0.05, the band's corners and centre, and a still target: the missile lands before its lifetime. (Runs
  after WP2b's data; WP1 runs it on a scratch row with the Liberator's values.)
- **T-10 (encoding).** The new fields change the checksum when changed and only then; RON round trip of
  `ThrowSpec`/`turn_rate` defaults (absent fields read as `None`).

### 6.2 Rows (WP2a, WP2b; `catalog_reference_values_tests.rs`, `missile_mover_tests.rs`)

- **T-11.** The shipped values of D-5 per row, with S-citations in the table (the reference test's mover table gains
  the throw and turn columns).
- **T-12.** Each row's flight ticks at §2.4's edges against a still target equal the fixed-point script's values
  (`flight.py`, ported to the test as an `f64` oracle, as b52's W-1), within one tick where the script names the
  boundary.
- **T-13 (moving target, Viking).** A target walking perpendicular at its top speed: the Viking analogue's drop flies
  the release bearing for its 0.6875, then homes; it lands.

### 6.3 Client (WP3, WP4)

- **T-14 (bent travel).** `travel_request` of an event with a `bend` builds a path through it: the start, the bend and
  the end lie on the path at parameters 0, the bend's, and 1; an event without `bend` builds today's line byte for byte;
  `shot_clip` along a bent path cuts at the first unseen sample (fail closed) — unit tests in `effects/systems.rs`.
- **T-15 (escorts drawn).** A snapshot with a Brood Lord analogue holding 2, then 1, then 0 escorts shows 2, 1, 0 escort
  children at ±150° behind its heading at the D-8 distance; a unit without escorts has none (client test through the
  live sync, found by property).
- **T-16 (steady state).** `sync_steady_state_tests.rs` and `sync_allocation_count_tests.rs` gain a `BroodSiege` case
  (a Brood Lord analogue attacking continuously, escorts cycling, Broodlings coming and going): zero allocations and no
  new entities after warm-up.
- **T-17 (presentability).** A unit whose escort look lacks media is not presentable.
- **T-18 (GPU, one child through `gpu_run.rs`).** A capture of the battle window with a Brood Lord analogue holding two
  escorts renders the real flipbooks (non-empty escort pixels inside their bubbles); reviewed capture in the evidence.

### 6.4 Snapshot (WP4; `sim_core` tests)

- **T-19.** `escorts` equals `armed + (arming_left > 0)` through a release, a rebuild and an arming; 0 for every other
  unit; frames without a magazine unit serialize byte-identical to WP3's.

### 6.5 Records (WP5)

- **T-20.** None in code. B-236's row carries S17–S20 and Q0145; the death-spawns plan's §14 bullet and the broodlings
  plan's §14 bullet gain the cross-reference.

### 6.6 World check and close battles (pre-registered)

Every world that differs from the previous package's is explained by its **first difference** (`firstdiff.py` and a
per-entity trace) and classed:
- **S (schema bytes):** goldens and sandbox differ by the encoding only (WP1's proof).
- **T (throw):** the first difference is a projectile of a WP2a row (position, phase, impact tick), allowed only in a
  world WP0's probe lists as launching that row.
- **L (Liberator):** the first difference is a `sky_bastion` projectile or the rng word after its draw (WP2b), allowed
  only in a world WP0 lists as launching it.
- **P (presentation):** a `RenderSnapshot` field (`bend`, `escorts`) only.
- **Anything else:** a defect, stop rule 2.

The close-battle list is fixed by WP0 from its baseline alone, by the rule of the death-spawns plan's §6.6 (every
matrix battle whose minority side wins at least 2 of its 20 baseline runs; battles 31 and 33 at the same share over 100
and 64 seeds; every battle a closed owner question or fixture comment names close or restated; no other; a `known_bug`
battle is not close), and frozen when WP0 commits. **Predicted to move:** battles whose squads hold a throw row —
by the fixture, 12c, 19 and 35 (Viking: equal still-target times, so moves only through moving targets), 20 (Brood
Lord: longer flights, Broodlings later) — and any others WP0's probe finds. **An outcome move** in a listed world is not
a stop: per charter §8 a matchup moved by a faithful mechanic is restated with the moved share, one traced seed and the
S-source, by the orchestrator, before the package continues (stop rule 4 covers moves of another class or an unlisted
world). A claim restated is recorded in the evidence and the fixture comment.

### 6.7 Predictions (recorded before measuring, not thresholds)

| World | WP1 | WP2a | WP2b | WP3 | WP4 |
|---|---|---|---|---|---|
| Matrix (seeds of record, 16 development seeds) | identical | T in 12c, 19, 20, 35 (20 certain: longer escort flights) | L only where WP0 finds `sky_bastion` (none expected) | identical | identical |
| Battles 31, 33 | identical | identical unless WP0 finds a throw row | identical | identical | identical |
| Showcase, showcase effects (default, 1–8), skirmish (6,000 ticks) | identical | T where WP0's probe finds a throw row | L where found | identical | identical |
| Goldens, sandbox | S, proved, re-recorded | identical unless WP0 finds a throw row | likewise | identical | identical |
| Snapshot frames | identical | P (`bend`) where throws fly | P | identical | P (`escorts`) |

---

## 7. Measurement

### 7.1 WP0 census and probe (scratch, not committed)

- **(c) Census.** `~/scratch/missile-throw-plan/census/`: the weapon-delivery census script extended to print, per
  layer with file and line, every field of S1–S5's movers (`Driver`, speeds, `Outro`, `ThrowVector`,
  `ThrowRotationType`, bands, `Gravity`, `YawPitchRoll`, `YawPitchRollAccel`, overlays), the launches that fly them,
  `MissileDefault`, S12–S13's blocks and S17–S19's defaults; and every shipped launch's mover with any `Throw` or
  turn-limited guidance phase. Any difference from §2.1 is stop rule 9.
- **(p) Probe** (`~/scratch/missile-throw-plan/probe/`, from the death-spawns probe; it may name slugs, being
  scratch): per world and seed, changing no state, every release of a weapon of D-5's five rows (tick, weapon, edge
  distance, target moving or not) and every Brood Lord analogue present. A positive control (a scratch fight with each
  row) proves a zero is a measured zero. Its listing fixes which worlds may move (classes T and L).
- **Close-battle list** (§6.6, frozen), **baseline** on four builds (both goldens and `DEFAULT_SANDBOX_CHECKSUM`),
  **throughput and instructions**.
- **Fixed-point preview.** `flight.py` ported to fixed point (`flightfx.py`, the LUT reproduced from `math.rs`) prints
  §2.4 again; a difference of more than one tick from §2.4 is reported in the evidence, not a stop.

### 7.2 Commands

The death-spawns plan's scripts, copied to `~/scratch/missile-throw-plan/scripts/` with their paths changed:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/missile-throw-plan
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-<prev> $R/world-<wp>
python3 -I $R/scripts/matrixdiff.py $R/world-<prev> $R/world-<wp>
```

One at a time across worktrees, in the background, never beside a `cargo gate` or a workspace `cargo test`
(charter §7). **How each world is measured:** a world comparison against the previous package (every matrix battle at
its seeds of record and the 16 development seeds, battles 31 and 33 at their seeds, the showcase and showcase-effects
worlds at the default seed and seeds 1 to 8, the skirmish scenario for 6,000 ticks, both goldens and the sandbox on four
builds, the client skirmish stage log), byte for byte on the per-tick checksums and the outcome records; a
first-difference trace for any world that moves, classed by §6.6.

### 7.3 Held-out seeds (WP6, confirmation only)

Once, on `main` and the final tree: matrix seeds 7001 to 7016, battles 31 and 33 on 7001 to 7064, the showcase and
showcase effects on seeds 9 to 16, the skirmish scenario on seeds 9 to 12. Byte-identical expected outside the worlds
WP0 lists; in those, classes T and L only; anything else is stop rule 3. Nothing is restated or tuned from them.

---

## 8. Stop rules

1. WP1's latent proof fails (a checkpoint differs with the schema at `main`'s number and the new fields unhashed):
   stop; something other than the encoding moved.
2. A measured world's first difference is not of class S, T, L or P, or T/L in a world the probe did not list: stop,
   trace, report.
3. A held-out seed differs outside §7.3's allowance: stop and report.
4. An outcome moves in a world or by a class §6.6 does not allow: stop for review with the trace.
5. Throughput below 50 ticks/s in any scenario (the `salvo` row included), or instructions above +0.5 % on the
   existing scenarios: stop and report.
6. Anything under `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`
   would need an edit: stop and report to the orchestrator (D-10).
7. The bent travel needs a shader or renderer change beyond `effects/` (D-6), or a steady-state or allocation test
   fails (T-16): stop; never fix by excluding the case from the test. WP3 may then land as `bend` in the event only,
   the drawing recorded as deferred.
8. T-9 finds a shipped row circling its target, or T-1 finds a non-throw missile moved: stop.
9. WP0's census contradicts §2.1 (another vector, band, turn rate, gravity, or a later layer overriding a mover):
   stop WP0; amend the plan first.
10. Q0144 is answered (b) or (c) before WP4 lands, or Q0145 (a) or (c) before WP5: stop that package and amend D-8 or
    D-9 first.

---

## 9. Client consequences

WP3: `ProjectileVisual::bend`, the bent `WorldLine`, `shot_clip` along it, T-14. WP4: `escorts` in the snapshot, the
presentation field, retained escort children, presentability, T-15 to T-18. No new asset (Q0144 (a)); no shader. The
diagnostics and GPU suites run in each client package's gate through their catalogue triggers.

---

## 10. File ownership

| File | Change | WP |
|---|---|---|
| `DESIGN.md` | §4 B-231 restated (WP0) and fixed (WP6); B-235 restated (WP0) and fixed (WP4/WP6); B-236 restated with S17–S20 and Q0145 (WP5); §5 `salvo` row (WP6) | WP0, WP4, WP5, WP6 |
| `docs/evidence/2026-11-27-missile-throws-and-escorts.md` | New: census, probe, close list, baselines, proofs, worlds, held-out, bench, captures | all |
| `crates/sim_types/src/weapon.rs` | `ThrowSpec`, `AngleBand`, `MissilePhase::{throw, turn_rate}`; the phase doc no longer says the shape is not modelled | WP1 |
| `crates/sim_types/src/snapshot.rs` | `ProjectileVisual::bend` (WP3), `BubbleRenderInstance::escorts` (WP4) | WP3, WP4 |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs` | Defs, fixed-point resolution, D-1 rules | WP1 |
| `crates/sim_core/src/entity.rs` | `ActiveProjectile::{frame, heading, throw_draw}`, `ThrowDraw` | WP1 |
| `crates/sim_core/src/combat_resolution.rs`, `combat_resolution/missile_flight.rs` | D-3, D-4; `bend` (WP3) | WP1, WP3 |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | Schema line; fields | WP1 |
| Snapshot writer in `sim_core` (`world` render path) | `escorts` | WP4 |
| `crates/sim_core/tests/{missile_mover_tests,catalog_reference_values_tests,runtime_catalog_tests,checksum_state_tests}.rs`, literals listing every `MissilePhase`/`ActiveProjectile` field | T-1 to T-13, T-19 | WP1, WP2a, WP2b, WP4 |
| `assets/catalog/units.ron`, `structures.ron` | D-5 rows (WP2a, WP2b); `escorts` presentation block on `spore_matriarch` (WP4) | WP2a, WP2b, WP4 |
| `crates/client/src/effects/{systems,binding}.rs` and the `WorldLine` owner | Bent travel, clip | WP3 |
| `crates/client/src/` live sync, `render_pool.rs`, `media.rs`, the presentation catalog reader | Escort children, presentability | WP4 |
| `crates/client/tests/{sync_steady_state_tests,sync_allocation_count_tests,weapon_look_tests}.rs`, a battle-window GPU test | T-14 to T-18 | WP3, WP4 |
| `crates/sim_core/benches/throughput_scenarios.ron` | `salvo` row | WP6 |
| Goldens, `crates/sim_fixtures/tests/fixture_tests.rs` | Re-recorded in WP1 (and WP2a/2b if WP0 finds a throw in them) | WP1 |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | Comments of restated claims (§6.6), if any | WP2a, WP2b |
| `docs/plans/README.md` | Row: Proposed (plan), Active (WP1), Done (WP6) | plan, WP1, WP6 |

**Not touched:** `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron`, `target_acquisition.rs`,
`effects/` of `sim_core`, every shader, every asset under `assets/textures/` and `assets/videos/`.

---

## 11. Work packages (commit order)

Each package is red first, then the change, then `cargo gate quick` and the tests its change needs; a verifier runs
`cargo gate package` on the final tree after merging `main` (Q0061, `docs/guides/test-gates.md`). **Each package
leaves `main` green and can merge alone.** WP3 and WP4 need only WP1 (WP3) or nothing (WP4) and may run in parallel with
WP2a/WP2b if the orchestrator allows two building rounds. Builds go to
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`; logs, probes and proofs under
`~/scratch/missile-throw-plan/`. GPU tests lock the GPU themselves; `cargo test` is never wrapped in `with-gpu-lock`.

### WP0: records, census, probe, baseline (XS, docs and scratch)

§7.1. `DESIGN.md` §4 B-231 restated with S1–S11 and §2.4's bound (the "about 7 ticks" replaced), B-235 with S12–S16.
**Acceptance:** stop rule 9 holds; the evidence lists the census, the probe's worlds with the control, the frozen close
list, the four-build baseline, throughput and instructions, the fixed-point preview. Commit `docs(design): restate
B-231 and B-235 (plan 2026-11-27-missile-throws-and-escorts WP0)`.

### WP1: throws and turn rates, latent, schema (M, `sim_types`, `sim_core`)

D-1 to D-4. Red first: T-1 to T-10 on scratch rows. **Acceptance:** they pass; every measured world byte-identical to
WP0's; latent proof 8 of 8; schema stepped (64 provisional); goldens and sandbox re-recorded and proved on four builds;
stop rules 1, 6 and 8 hold. Commit `feat(sim_core): missile throw phases and turn-limited guidance (B-231; latent)`.

### WP2a: the four unbanded throws (S, data and tests)

D-5's Phoenix, Viking, Spine Crawler and Brood Lord rows, each with its S-citation comment. T-11 to T-13. World check:
classes T only, in worlds WP0 lists; restatements per §6.6. Commit `feat(catalog): Phoenix, Viking, Spine Crawler and
Brood Lord analogues throw their 5.0.14 missiles (B-231)`.

### WP2b: the Liberator analogue (S, data and tests)

D-5's `sky_bastion` row; T-9 on the shipped row, T-11, T-12. World check: class L only. Commit `feat(catalog):
Liberator analogue missiles thrown in their 5.0.14 band and turned back (B-231)`.

### WP3: the throw drawn (M, `sim_types`, `sim_core` event, `client`)

D-6; T-14 and a steady-state run with throws. A reviewed capture of a Phoenix and a Brood Lord analogue volley goes to
the evidence. Stop rule 7's fallback recorded if taken. Commit `feat(client): bent missile travel through the throw's
end (B-231)`.

### WP4: the escorts drawn (M, `sim_types`, `sim_core` snapshot, `client`, data)

D-7, D-8 under Q0144's answer or default (a). T-15 to T-19. Reviewed capture in the evidence. B-235 **fixed** in
`DESIGN.md`. Commit `feat(client): Brood Lord analogue escorts drawn at their ExternalAngle slots (B-235)`.

### WP5: B-236 records (XS, docs)

D-9: B-236 restated with S17–S20 and Q0145; if Q0145 was answered by then, the answer is carried out under stop rule
10. Commit `docs(design): restate B-236 with the placement data (plan 2026-11-27-missile-throws-and-escorts WP5)`.

### WP6: confirmation and records (S)

§7.3 held-out seeds; the `salvo` row and the bench (§5); B-231 **fixed** in `DESIGN.md` with commits and figures;
cross-references (§13); README row `Done` (or `Partly done` naming WP3's fallback or B-236 if open). Commit `docs:
close B-231 (plan 2026-11-27-missile-throws-and-escorts WP6)`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Level | File | Covers |
|---|---|---|
| Unit | `sim_types` `weapon.rs`, `snapshot.rs` | RON forms and defaults of `ThrowSpec`, `turn_rate`, `bend`, `escorts` |
| Unit | `sim_core` `missile_flight.rs`, validation, `encoding.rs` | T-2 to T-8, T-10 |
| Integration | `missile_mover_tests.rs`, `catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs`, `checksum_state_tests.rs` | T-1, T-9, T-11 to T-13, T-19 |
| Determinism | goldens, sandbox, four builds | §5 proofs, WP1's re-record |
| Client | `effects/systems.rs` units, live-sync tests | T-14, T-15, T-17 |
| GPU | battle-window capture child through `gpu_run.rs` | T-18 |
| Steady state | `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` (valgrind) | T-16 (`BroodSiege`) |
| Bench | `cargo bench -p sim_core --bench throughput`; `perf stat` | ≥ 50 ticks/s (`salvo` included), ≤ +0.5 % instructions |

---

## 13. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 B-231 | Restated (S1–S11, §2.4's bound); **fixed** with commits and figures | WP0, WP6 |
| `DESIGN.md` §4 B-235 | Restated (S12–S16); **fixed** (or kept, under Q0144 (c)) | WP0, WP4 |
| `DESIGN.md` §4 B-236 | Restated (S17–S20, Q0145) | WP5 |
| `DESIGN.md` §5, Combat row | `salvo` row; throws named | WP6 |
| `docs/plans/2026-11-28-missile-movers-b52.md` D-7, `2026-11-27-weapon-delivery.md` R-1, `2026-11-28-broodlings.md` §14, D-11 A-1/A-3/A-6, `2026-12-01-death-spawns-and-hangar-commands.md` §14 | "*Taken up by plan `2026-11-27-missile-throws-and-escorts`.*" | WP6 |
| New evidence file | §7 | all |
| `docs/questions/open/0144-…`, `0145-…` | Filed with this plan | plan |
| `docs/plans/README.md` | Row `Proposed`, `Active`, `Done` | plan, WP1, WP6 |

---

## 14. Out of scope and decisions recorded (Q0002)

- **Overlays** (the Brood Lord's wave, the Liberator's revolver): presentation only (S10), not drawn; a later client
  plan may draw them from the same data.
- **Missile height** (launch and impact heights, gravity's arc as state, 3D `Outro`): D-1's rejected option; the bend's
  height is presentation.
- **`MissileDefault`'s 719°/s turn** on constant missiles: those rows stay constant (b52 D-1); only movers carry turns.
- **Two drawn missiles for one two-strike projectile** (Phoenix, Brood Lord): one travel drawn, as today.
- **The escort as the launched missile** (its slot as the launch point, A-7): the simulation launches from the
  centre; the drawn escort vanishes from its slot when thrown.
- **Escort `Leash`, `Recall`** (broodlings plan A-7): escorts are not entities.
- **Placement among units** (B-236): kept until Q0145; D-9's amendment path.
- **Anything in `movement/`, `steering.rs`, `movement_rules.ron` or acquisition admission** (Q0143): none needed.

---

## 15. Owner questions (filed as [Q0144](../questions/open/0144-brood-lord-escort-look.md) and [Q0145](../questions/open/0145-spawn-placement-among-units.md))

Both are summarized in §4 D-8 and D-9; the files carry the full text and defaults.

---

## 16. Material (scratch, not committed)

- `~/scratch/missile-throw-plan/flight.py`: §2.4's planning estimate (floats; `--grid` runs the Liberator band grid).
- `~/scratch/combat_wp3_data/<layer>/`: the 5.0.14 data copy.
- `~/scratch/weapon-delivery/census/movers.txt`: which launch flies which mover.
- `~/scratch/death-spawns-plan/scripts/`: the measurement and proof scripts WP0 copies.

---

## Amendment 1 (orchestrator, 2026-11-27): owner answers Q0144 (b) and Q0145 (c)

Stop rule 10 fired before any package started, so D-8 and D-9 are amended here; nothing else in the plan changes.

**Q0144 (b), the escorts' look (D-8, WP4).** The escort is drawn with its own real Veo clip of the escort creature in
flight, generated through `asset_baker` under the project's media rules (Q0020 and the broodlings plan's D-12 review
rules: loops without jumps, audio as the style guide says, no rays or radiating stripes). Budget: at most 4
generations and at most $4.00; take the best result inside that cap rather than spend more. The media ledger's total
stays ≤ $230; WP4 reads the ledger's current total first and records every generation, its cost and each rejection
reason in the evidence. If Veo is blocked (key, service, quota or budget), WP4 stops and reports; it never falls
back to a drawn stand-in or to `brood_mite`'s flipbook in its place (charter §1.2). The escort look becomes its own
presentation entry (a `look` slug with its own flipbook and radius); D-8's data shape, slots, retained children and
presentability rule are unchanged. §9's "no new asset" no longer holds: WP4 adds one baked flipbook, covered by
`runtime_media_tests.rs` and `bake_integrity_tests.rs` like every installed clip.

**Q0145 (c), created units among other units (D-9, WP5).** The owner's reading: a created unit with placement enabled
(battlefield spawns such as the Broodlings an escort impact or a death spawn creates) skips spots occupied by other
units and lands on the nearest free, non-colliding spot beside them, never inside them, as StarCraft II's Broodlings
surround their target. WP5 changes from records to a simulation package:
- `world/spawns.rs`' shared placement ring search also rejects a candidate whose disc overlaps any unit's collision
  disc that the new unit collides with (same collision classes as movement uses; read through the spatial hash, no
  per-candidate scan of all units, no `movement/` file edited). Air and ground keep their own layers as the
  collision classes say. Order of candidates and tie-breaks are unchanged, so the result stays deterministic.
- Red-first tests: a Broodling impact onto a packed group lands beside it with zero overlap; a death spawn among units
  places all its Broodlings without overlap; a spawn with no free ring spot within the existing search bound behaves
  as the search bound already defines (recorded, not new behaviour); placement with no units nearby is byte-identical
  to `main`.
- World check: classify every first difference by the spawn that moved; battle 20 (Brood Lords) and any death-spawn
  world are expected to move; restate any close-battle claim under AGENTS.md §8 with traces.
- Throughput by callgrind instructions against §5's bar (stop rule 5); the hashed state and checksum format are
  unchanged.
- B-236 is closed by WP5 and Q0145 moves to `closed/` with its resolution.
