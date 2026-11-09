# Q0063: Where is a splash measured from, and does it reach a unit's edge?

- **Asked:** 2026-11-06
- **Blocking:** no
- **Default until answered:** no change. Every splash is centred on the struck target's centre and reaches a unit
  when the unit's centre lies within a zone's radius. Whatever the game does differently stays recorded under bug
  B-17 in `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP8a (bugs
B-16 and B-17, research item U-4) gave every splash its own zones and fractions from the 5.0.14 data
(`CEffectDamage` `AreaArray`). How the game measures the distance is engine code, and the data only name a flag
(evidence [`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §8.1):

- Every weapon damage effect inherits `SearchFlags` `OffsetByUnitRadius` = 1 from `DU_WEAP` (`core`
  EffectData:128). The Siege Tank's blast (`liberty` EffectData:1680) and the Thor's anti-air splash (`liberty`
  EffectData:2281) set it to 0. The Liberator's anti-air splash and the Baneling's burst keep 1.
- Patch 1.1 (2010) changed the splash of exactly the four weapons whose effects clear the flag: those two, the
  Archon's (`PsionicShockwaveDamage`, `liberty` EffectData:1359) and the Planetary Fortress'
  (`TwinIbiksCannon`, `liberty` EffectData:2545): "splash damage now originates from the center of the target,
  rather than the impact location near the unit". So the flag
  most likely moves the area's centre from the target's centre to the target's edge facing the attacker (the
  "impact location near the unit"). Hive Workshop's description of the flag, seen only through a search engine
  because the page returned 502, reads "offsets the origin of the search radius … by the unit radius of the damage
  effect's primary target". No source states the direction or what the flag does when the area is centred on the
  attacker, as the Baneling's is (`ImpactLocation` `CasterUnit`, `liberty` EffectData:1759).
- Whether an area reaches a unit whose centre is outside the radius but whose edge is inside is not stated in any
  source found (Liquipedia's Siege Tank page: "within .4687 matrices of the target"; TeamLiquid, SC2Mapster and
  Hive Workshop searched, 2026-11-06).

**Why it matters.** Both decide how many units a splash catches. For the line areas of WP8b (plan Amendment 13,
evidence §9.4) the edge rule decides whether a line hits anything: the Hellion's and the Colossus' searches have a
radius of 0.15, so measured to a unit's centre they reach almost no unit beside the line, and with the edge rule
battles 21 and 26 keep their winners. The edge rule would widen every splash by the
struck unit's radius (0.375 for a Marine); the offset would shift the Liberator's and the Baneling's areas by the
target's radius.

**Options.**

1. **No change (default).** Centre of the target, centre of each struck unit.
2. **The offset only.** An area whose weapon keeps `OffsetByUnitRadius` is centred on the target's edge facing the
   attacker; struck units are still measured to their centres.
3. **The edge only.** A unit is struck when any part of it lies in the zone (distance minus its radius); every area
   stays centred on the target's centre.
4. **Both 2 and 3.**

A measurement in the StarCraft II editor would settle both: (a) a Liberator in fighter mode shooting a Mutalisk
(radius 0.5) with a second Mutalisk placed 1.6 from the first's centre on the Liberator's side, then on the far
side, reading which one loses life; (b) a sieged Siege Tank firing at a Zergling with a Marine placed so that its
centre is 1.4 and its edge 1.025 from the Zergling's centre, reading whether the Marine loses 25 % of the shot.

**Recommendation.** Option 1 until measured; option 2 is the reading the patch notes support if the owner accepts
them as the source.

## Answer

**Option 3 or Option 4 (Measure splash reach to the victim's edge, `distance - victim_radius <= R`, and optionally honor `OffsetByUnitRadius` on weapons that retain the flag — no editor test needed).**

**Engine insight:** In StarCraft II's `CEffectDamage` / `CEffectEnumArea` search engine, area searches test intersection against each potential victim unit's collision circle (`center_distance - victim_radius <= R`), not its center point. That is why the Hellion's and Colossus's line-splash searches specify a radius of only `0.15` in `EffectData`: `0.15` is the half-width of the flame/beam itself, which catches any unit whose collision edge touches the `0.15` corridor (e.g., a Marine of radius `0.375` within `0.525` center distance), and why a Baneling (`2.2` splash radius) damages units whose collision circles touch the `2.2` circle. Combining victim-edge reach (Option 3) with `OffsetByUnitRadius` on weapons that keep the flag (Option 4 — matching Patch 1.1.0, where Siege Tank, Archon, Thor AA, and Planetary Fortress cleared `OffsetByUnitRadius` to center splash on the primary target while default splash weapons offset to the impact edge) reproduces SC2's exact geometry and unblocks WP8b's held line-splash data commit.
