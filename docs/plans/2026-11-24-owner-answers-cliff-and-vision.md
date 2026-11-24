# Blueprint: The Owner's Answers on Cliff Levels and Vision — Ramps and Air in `SameCliff`, Detection as a Circle, Detectors Under Construction (Q0099, Q0101)

This plan carries out the owner's answers to two questions that two merged plans left open with defaults built:

- **Q0099** ([open](../questions/open/0099-cliff-level-of-ramps-and-air.md)), asked by plan
  [`2026-11-23-splash-cliff-levels`](2026-11-23-splash-cliff-levels.md) (B-170, `SameCliff`):
  - **1(c):** a unit on a ramp counts as **both** cliff levels, so a ramp never separates splash between high and
    low ground. *Built:* a ramp counts as its lower landing (D-2 of that plan). **Changes.**
  - **2(b):** air blasts **ignore** cliff levels. *Built:* an air unit has the level of the ground below it.
    **Changes.**
  - **3(a):** the level is read at the blast's centre point. *Built so.* **Already holds** (§3 item 3).
- **Q0101** ([open](../questions/open/0101-vision-and-detection-details.md)), asked by plan
  [`2026-11-23-vision-and-detection`](2026-11-23-vision-and-detection.md) (B-54 to B-57):
  - **1(a):** air units and Xel'Naga towers see into sight blockers. *Built so.* **Already holds** (§3 item 4).
  - **2(b):** detection is a plain circle: a detector detects cloaked or burrowed units within its detection radius,
    up cliffs and past sight blockers, wherever another friendly unit gives vision. *Built:* detection follows the
    detector's line of sight. **Changes.**
  - **3(b):** detectors under construction do not detect until construction completes. *Built:* they detect (B-184).
    **Changes**, and closes B-184.
  - **4(b):** the attack reveal (B-55) is visible to the struck player and its allies. *Built:* the struck player
    only. **No observable difference today**: the simulation has no teams, so a player's only ally is itself
    (`match_state.rs` module doc). The rule is recorded where the reveal is set and in the register (D-6).

Every change is a rule over existing data. No catalog row, no hashed field and no checksum encoding changes, so the
plan needs **no schema number** (§5). No code names a unit (charter §3.3, C5).

One consequence of 2(b) is settled here as well. Until now a detected cell was always a cell the detector itself
saw, so "detected" implied "seen". Under the circle the two part, and the weapon filter of plan
[`2026-11-24-weapon-validation`](2026-11-24-weapon-validation.md) must still ask for both. D-4 keeps a detected but
unseen unit untargetable, and lets the attack reveal count as "seen" for a concealed attacker, as it already does for
a visible one.

A full plan is required (charter §6): new rules in the simulation that may move battle outcomes, and work across
`sim_map` and `sim_core`. Two packages register a new row each in the bug register (D-8).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-24 against `main` at
`20322ca6` (checksum schema 43).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| **Q0099**: the cliff level of ramps and air units, and where it is read | **Answered** 1(c), 2(b), 3(a); closed by this plan's WP6 | D-1, D-2, §3 item 3 |
| **Q0101**: sight blockers and the air, detection through terrain, detectors under construction, allies and the reveal | **Answered** 1(a), 2(b), 3(b), 4(b); closed by this plan's WP6 | D-3 to D-6, §3 item 4 |
| Q0106: weapon targets leaving vision (a unit is seen by its centre cell, part 4's default) | Open; defaults built | D-4 ("seen" is unchanged) |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.4, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6, §7 |

---

## 1. Summary

| | Today (`20322ca6`) | After this plan |
|---|---|---|
| Area kept to its level (`SplashCliff::Same`), unit or centre on a ramp cell | The ramp counts as its lower landing, so a shell on the plateau spares a unit on the ramp's top cell | A ramp cell counts as **both** landings: it shares a level with each, and is separated only from a third level (Q0099 1(c)) |
| Area on the air plane (Liberator analogue's anti-air), or any unit in the air | The level of the ground below | **Every** level: an air blast and an air unit are never separated by a cliff (Q0099 2(b)) |
| Where a blast's level is read | At its search centre (the target's centre, its offset edge, the landing point, each line point) | Unchanged (Q0099 3(a), already built) |
| The Baneling analogue's other-level strike | When the target's cell level differs from the Baneling's | When their level spans do not overlap, by the same rule as the area, so a target is still struck exactly once |
| A detector's detection | Cells within its detection radius **that its line of sight reaches** (cliffs and blockers stop it) | Every cell within its detection radius (Q0101 2(b)); sight unchanged |
| A concealed enemy that is detected but whose cell no unit of the player sees | Cannot occur with shipped detectors (detection equals sight, along the same rays) | Not targetable (detection alone is not sight) |
| A concealed attacker, attack-revealed and detected, whose cell is fogged | Not targetable (the concealment test asks for a `Visible` cell) | Targetable: the reveal counts as seen for a concealed unit as for any other (D-4) |
| A detector under construction | Detects | Detects nothing until its construction completes; sees as before (Q0101 3(b), B-184) |
| Attack reveal | The struck player | The struck player and its allies; with no teams in the simulation, the struck player (Q0101 4(b), D-6) |
| Sight blockers and air units, Xel'Naga towers | Air, `CliffWalker` and tower sources see into blockers | Unchanged (Q0101 1(a), already built) |
| Hashed state, schema | — | No new field; **no schema change** (43 stays). `PlayerVisionMap::detector_counter` (hashed) changes value only where a detector's circle covers cells its rays did not reach |
| Worlds that move | — | **None predicted**: no measured world or golden fields a ground detector, a detector under construction or a cloaked attacker against one, and the splash-cliff probe found no area reaching a unit across levels (§7.2 confirms on today's tree) |
| Size | — | S to M: seven packages, each XS to S |

---

## 2. What the answers mean, and what StarCraft II does

Tags: [ANS] the owner's answer; [DATA] read in the game data; [INF] inferred; [EXT] an outside source (Q0008);
**verify** not confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | Q0099's answer (owner, 2026-11-24) | 1(c), 2(b), 3(a) [ANS] |
| S2 | Q0101's answer (owner, 2026-11-24), with "All four choices match StarCraft II's actual behaviour" | 1(a), 2(b), 3(b), 4(b) [ANS] |
| S3 | `CEffectDamage` default `SearchFlags SameCliff 1` (`core` EffectData:67); `CliffLevelNotEqual` (`voidmulti` ValidatorData:894–898); plan `2026-11-23-splash-cliff-levels` §2 S2 to S10 | The rule whose ramp and air readings Q0099 settles [DATA] |
| S4 | `CBehaviorBuff default id="Detector"` (`core` BehaviorData:58–61), `Detector11` (`liberty` BehaviorData:1175–1178); plan `2026-11-23-vision-and-detection` §2 S9, S10, S17 (Liquipedia *Distance*: a detector's range reveals cloaked units "if they are a Detector") | Detection is a radius of its own; the data have no line-of-sight field for it [DATA]; whether terrain stops it is what Q0101 2 asked |
| S5 | `LayoutButtons Face="Detector" … Requirements="NotUnderConstruction"` (`liberty` UnitData:6074, :8796, :11092) | Consistent with 3(b): the detector's button and its detection wait for completion [DATA + ANS] |
| S6 | `AttackReveal` (`core` BehaviorData:237–240); plan `2026-11-23-vision-and-detection` §2.2 rule 3: "A reveal grants visibility, not detection: a cloaked attacker stays cloaked" | What the reveal gives a concealed attacker [DATA, INF] |
| S7 | Liquipedia, *High Ground and Low Ground* (search abstract, vision plan S16): air units over high ground "will be revealed if they attack"; the common game case of a Missile Turret below a cliff firing at a cloaked Banshee that attacks from above | A revealed and detected cloaked attacker is a target [EXT, INF] (D-4) |
| S8 | `CWeapon` default `TargetFilters Visible;-` (`core` WeaponData:29); plan `2026-11-24-weapon-validation` D-3 | A weapon's target must be visible to its player, and detected if cloaked [DATA]; detection never stands in for vision |
| S9 | `crates/sim_core/src/match_state.rs` module doc: "The simulation has no teams, so every player is their own side" | 4(b) has no ally to reach today |

### 2.2 The rules that follow

1. **A ground position's cliff levels are a span** (S1 1(c)). A flat cell spans its one level; a ramp cell spans its
   lower landing and the level above it (the upper landing, which `sim_map` validation requires to be `low + 1`:
   `RampUpperLandingMismatch`). Two positions share a level when their spans overlap. A ramp therefore shares a level
   with both landings and with any other ramp of the same run, and is separated from a third level (a level-2
   cliff beside a 0-to-1 ramp). Overlap is not transitive, which is the answer's intent: the low ground and the
   plateau stay separated from each other.
2. **An air blast and an air unit span every level** (S1 2(b)). "Air blast" is read as an area searching the air
   plane, so the area's centre spans every level when the area's plane is `Air`. "Air-to-air splash does not miss
   across a ground cliff line" is read for each struck unit as well: a unit whose mover type is `Air` spans every
   level, so an area centred on a `CliffWalker` (Colossus) target by a weapon that hits air strikes air bystanders on
   any level [INF]. A `CliffWalker` struck unit stands on the ground and keeps its ground span [INF, §14 F-2].
3. **The level is read at the search centre** (S1 3(a)): built (§3 item 3).
4. **The Baneling's fallback is the complement of its area** (S3): the target is struck directly exactly when its
   span does not overlap the burst centre's. A Baneling on a ramp and its target at the top share a level, so the
   area strikes the target and the fallback does not.
5. **Detection is a circle** (S2 2(b), S4): a detector marks detection on every cell whose whole-cell offset from
   its own cell lies within the detection radius, whatever the terrain between. Sight keeps every cliff, ramp and
   blocker rule.
6. **Detection is not sight** (S2 2(b) "whenever another friendly unit grants vision there", S8). A concealed unit is
   a target only where its player both sees it and detects it. A detected cell that nobody sees gives nothing.
7. **The attack reveal is sight of the attacker** (S6, S7): a concealed attacker revealed by its attack to player p,
   and standing in a cell p detects, is seen and detected by p, so it is a target, as a non-concealed one already is
   (vision plan D-5) [INF].
8. **A detector detects from the tick its construction completes** (S2 3(b), S5). Its sight while under construction
   is unchanged.
9. **The reveal goes to the struck player and its allies** (S2 4(b)); with no teams (S9), that is the struck player.

### 2.3 Survey: what each answer touches

| Answer | Shipped data it reaches | Code it changes |
|---|---|---|
| Q0099 1(c) | The five `Same` circles (`railgun_crawler` siege, `planetary_bulwark_dome`, `stag_behemoth`, `sky_bastion`, `volatile_tick`) near a ramp | `sim_map` level query; `strike_area`, `strike_target_on_other_level` |
| Q0099 2(b) | `sky_bastion`'s anti-air area (the only area on the air plane); any `Same` area aimed at a `CliffWalker` by a weapon that hits air (none shipped strikes air bystanders that way, **verify** by the catalog: `splash` with `target_flags` containing an air attribute) | `strike_area` |
| Q0101 2(b) | Ground detectors: `flak_array_dome`, `tesla_sentinel_dome`, `spore_spire_dome`. Air detectors (`stealth_eye_orb`, `omni_probe_drone`) already ignore cliffs and blockers, so they change only if the rays leave gaps on open ground (**verify**, P-9) | `PlayerVisionMap::cast` |
| D-4 | Every cloaked air attacker (`cloak_gunship`) against a player with a detector | `SimWorld::is_cloak_revealed` and its five readers |
| Q0101 3(b) | The three detector structures while foundations | `collect_sights` |
| Q0101 4(b) | — | Docs only |

### 2.4 Neighbouring rules: in or out

| Rule | Verdict | Why |
|---|---|---|
| Q0099 1(c), 2(b); Q0101 2(b), 3(b) | **In** (D-1 to D-5) | |
| D-4, a revealed and detected cloaked attacker | **In** | Rule 7 is the composition of two built or answered rules; without it 2(b) leaves the common turret-against-Banshee case wrong |
| Q0101 4(b), allies | Recorded, no code (D-6) | No teams exist (S9) |
| Teams and shared vision | Out | A subsystem of its own; nothing in this plan depends on it |
| `CliffWalker` sources see into blockers and up cliffs (built by the vision plan) | Unchanged | 1(a) names air units and towers; the Colossus was not asked (§14 F-2) |
| The Lurker's `CliffLevelGE1` (B-177) | Out | Its own row |
| Snapshots of hidden ground attackers (B-183) | Out | Presentation |
| Scanner Sweep, Oracle Revelation, Overseer and Observer siege modes | Out | Not shipped; the circle rule applies to them as written when they are |

---

## 3. Diagnosis (on `20322ca6`, read from the code)

1. **Ramp reading.** `GameMap::cliff_level_at` (`crates/sim_map/src/grid.rs`) returns the cell's stored
   `cliff_level`, which for a ramp cell is its lower landing. Its only production readers are `strike_area` (the
   levels of the search points, into the retained buffer `CombatWorkspace::splash_levels: Vec<CliffLevel>`, and the
   level of each unit within reach) and `strike_target_on_other_level` (target against `source.origin`), both in
   `crates/sim_core/src/combat_resolution.rs`. `sim_types/src/weapon.rs` documents the defaults ("a ramp counts as
   its lower landing and an air unit has the level of the ground below it … Q0099's defaults").
2. **Air reading.** `strike_area` reads levels from positions only; nothing looks at the area's plane
   (`AreaStrike::plane`) or the struck unit's `mover_type` for the level test. `area_raw_damage` filters the plane
   with `shares_combat_plane` (`combat.rs`): an `Air` area strikes `Air` and `CliffWalker` units, a `CliffWalker`
   area strikes every plane.
3. **Q0099 3(a) holds.** `strike_area` computes `levels` from `points`, the search centres the caller built: the
   target's centre, or its edge facing the attacker under `offset_by_unit_radius`; the release position of an
   attacker-centred area; the landing point of a missile that lands on a point; each line point. The struck unit is
   compared with those points, never with the impact unit. No test pins the one case where the readings differ (an
   offset centre on another level than the target's centre); WP1 adds it, green from the start.
4. **Q0101 1(a) holds.** `PlayerVisionMap::cast` (`crates/sim_map/src/vision.rs`) sets
   `is_air = matches!(mover_type, Air | CliffWalker)`, and the blocker test `!is_air && !in_blocker &&
   target_is_blocker` stops only ground rays; `watchtower_source` gives towers `MoverType::Air`. Pinned by
   `vision_tests.rs` `air_and_towers_see_into_a_blocker` (air and tower sources) and `blocker_edge_tests.rs`
   `a_unit_in_a_blocker_edge_is_not_acquired_from_outside` (an air spotter of the attacker's player makes the unit in
   the edge acquirable). Only the comments cite "Q0101 part 1, default"; WP3 rewords them.
5. **Detection along the rays.** `cast` walks the rays to `max(sight, detection)` and marks `detector_counter` on
   each ray cell within the detection radius, after the cliff and blocker rules have stopped the ray. Tests
   `detection_follows_line_of_sight` and `equal_radii_detect_exactly_the_seen_cells` pin that reading.
6. **Detected implies seen today.** Every shipped detector has detection equal to sight, cast along the same rays,
   so every detected cell is a cell the detector sees. `SimWorld::is_cloak_revealed` (`target_acquisition.rs`)
   asks for a `Visible` cell and detection. Its readers: `is_entity_targetable` (so `passes_weapon_filters`, the
   weapon filter of plan `2026-11-24-weapon-validation`, and pursuit's gate), `StrikeContext::is_available`
   (`combat_resolution.rs`, on a `TargetSnapshot` that already carries `attack_revealed`), `is_attacker_visible_to`
   (retaliation), and `write_player_view`'s `revealed` list (`player_view.rs`). None of them looks at the attack
   reveal for the concealment half; each ANDs it with `sees_enemy` or `sees_point` where it needs sight.
7. **Construction.** `collect_sights` (`crates/sim_core/src/world/vision.rs`) builds each entity's
   `VisionSource` with `detection_radius` filtered only by `suppress_detection`. A foundation has
   `Entity::is_completed == false` until `production/construction.rs` completes it. `sees_the_same` compares
   `detection_radius`, so a change of the radius at completion triggers a recast.
8. **The reveal's audience.** `record_attack_reveal` (`combat_resolution.rs`) raises
   `attacker.attack_reveal_ticks[victim.owner]`. There is no alliance relation (`TeamId` in `sim_types/src/ids.rs`
   is read by nothing; S9).

---

## 4. Design decisions

### D-1. A position's cliff span (Q0099 1(c))

**Type** (`crates/sim_map/src/grid.rs`, exported from `lib.rs`):

```rust
/// The cliff levels a ground position belongs to for `StarCraft` II's `SameCliff`: an inclusive
/// range. A flat cell spans its one level; a ramp cell spans its lower landing and the level
/// above it, so a ramp shares a level with both of its landings (owner answer Q0099 1(c)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliffSpan {
    /// The lowest level the position belongs to.
    pub low: CliffLevel,
    /// The highest level the position belongs to; equal to `low` off a ramp.
    pub high: CliffLevel,
}
```

with `CliffSpan::single(level)`, `CliffSpan::ALL` (`MIN` to `MAX`) and `overlaps(self, other) -> bool`
(`self.low <= other.high && other.low <= self.high`).

**Query:** `GameMap::cliff_level_at` is **replaced** by `GameMap::cliff_span_at(pos) -> CliffSpan`: the same clamp to
the map and the same fallback for a short cell vector, then `single(cell.cliff_level)` for a flat cell and
`low = cell.cliff_level, high = low + 1` (saturating at `CliffLevel::MAX`) for a cell with `ramp: Some(_)`. The doc
cites Q0099 1(c) and `RampUpperLandingMismatch` (validation makes the upper landing `low + 1`; an unvalidated map
with a ramp at the top level spans only `MAX`). Diagonal legacy ramps span the same way. No other reader of
`cliff_level_at` exists (§3 item 1), so the replacement leaves no dead API; its unit tests are re-derived.

**Why the span lives in `sim_map`:** it is a fact about terrain. Mover types decide the air reading (D-2), which
belongs to combat.

### D-2. The level test in combat (Q0099 1(c), 2(b), 3(a))

One private helper in `combat_resolution.rs`:

```text
level_span(pos, mover) := CliffSpan::ALL                 if mover == MoverType::Air
                          map.cliff_span_at(pos)         otherwise
```

- **Search centres:** `level_span(point, area.plane)`. An area on the air plane spans every level (2(b)). The point
  is the centre the caller built (3(a), unchanged).
- **Struck units:** `level_span(other.position, other_arch.mover_type)`. A unit in the air spans every level (2(b));
  a `CliffWalker` and every ground unit use the ground span (§2.2 rule 2).
- **`strike_area`:** `CombatWorkspace::splash_levels` becomes `Vec<CliffSpan>` (the retained buffer's
  `PERFORMANCE RATIONALE` is unchanged: one value per search point). A unit within reach counts only the points
  whose span overlaps its own; the rest of the function is unchanged.
- **`strike_target_on_other_level`:** the early return becomes "the target's span overlaps the span of
  `source.origin` on `area.plane`", the exact complement of the area's test, so a target is struck by the area or
  the fallback, never both and never neither when in reach (rule 4).
- **Module doc**, invariant 6 of `combat_resolution.rs`: "An area whose `cliff` is `Same` strikes only units whose
  cliff span overlaps the span of a search point that reaches them: a ramp cell spans both its landings, and an
  area on the air plane, or a unit in the air, spans every level (owner answer Q0099 1(c), 2(b); plan
  `2026-11-24-owner-answers-cliff-and-vision` D-2). The level is read at the search point (Q0099 3(a))."
- **Docs:** `SplashCliff::Same` and the `SplashSpec` doc in `sim_types/src/weapon.rs` replace "a ramp counts as its
  lower landing and an air unit has the level of the ground below it … Q0099's defaults" with the answered rule.
- **Data:** none. Every shipped row keeps its `cliff` value; `SplashCliff` keeps its two cases.

**The direction of the change:** both answers only ever *add* units to an area's struck set (a span is a superset
of the stored level; `ALL` is a superset of every span), and move a Baneling's target from the fallback to the area
only where the spans now overlap. P-6 checks it at every world's first difference.

### D-3. Detection is a circle (Q0101 2(b))

`PlayerVisionMap::cast` splits its one walk in two, in this order:

1. **Sight** walks the rays to the **sight** radius (no longer to `max(sight, detection)`), with every cliff, ramp
   and blocker rule as today, and marks `sees` only.
2. **Detection**, when `detection_radius > 0`, marks `detects` on every cell `(cx + dx, cy + dy)` on the map with
   `dx² + dy² ≤ detection_radius²` in whole cells (the metric the rays already use, `(dist_sq << 32) <=
   detect_sq.raw()`), the source's own cell included. No terrain is read.

- `Reach` and `mark` stay as they are; each pass passes one of the two booleans. `Remove` mirrors `Add` and `Tally`
  exactly, since both passes depend only on the source's cell, radii and mover type, which `sees_the_same` compares.
  So the module invariant "incremental recasting leaves every counter and state exactly as a full recount" holds.
- The disc needs no generation stamp: each offset is visited once. It may iterate the square `[-r, r]²` directly;
  if the implementer precomputes the disc offsets per radius in the ray table instead, the existing
  `PERFORMANCE RATIONALE` block of `vision.rs` gains one `What:` line for it.
- `VisionSource::detection_radius`'s doc, the module doc ("Detection is a second circle cast along the same rays …")
  and the comment above `cast` say: "Detection is a plain circle of its own radius, which no terrain stops (owner
  answer Q0101 2(b)); a detected cell gives no sight, so a concealed unit there is a target only where its player
  also sees it."
- **Cost:** a detector's cast walks at most `π r²` disc cells (about 380 for 11) plus its sight rays; a non-detector
  is unchanged. No allocation.
- **The predicate side does not change in this package.** `is_cloak_revealed` still asks for a `Visible` cell and
  detection, so a detected but unseen unit is not targetable (rule 6). This is checked end to end (P-10), and it is
  what keeps the circle "about detection only".

### D-4. The attack reveal counts as seen for a concealed attacker

`SimWorld::is_cloak_revealed(map, vision_maps, viewer, position)` becomes
`is_cloak_revealed(map, vision_maps, viewer, position, attack_revealed: bool)`:

```text
is_cloak_revealed := vision_maps.get(viewer) is Some(vision)
                  && sees_point(map, vision_maps, viewer, position, attack_revealed)   # Visible cell, or revealed
                  && vision.is_detected(cell(position))
```

- Its readers pass the flag they already hold: `is_entity_targetable` and `is_attacker_visible_to`
  (`target.is_attack_revealed_to(viewer)`), `StrikeContext::is_available` (`snapshot.attack_revealed[slot]`) and
  `write_player_view`'s `revealed` list (`entity.is_attack_revealed_to(viewer)`).
- **Unchanged:** a viewer without a vision map detects nothing (`false`, as today); a concealed unit that is neither
  in a `Visible` cell nor attack-revealed is not a target, detected or not (rule 6); detection still never comes from
  the reveal (S6). `passes_weapon_filters` stays `is_entity_targetable && sees_enemy`; for a concealed target both
  halves now ask the same "seen", so the filter reads "seen, and detected if concealed".
- **What moves:** only a concealed attacker that is attack-revealed to p, in a cell p detects but does not see. With
  line-of-sight detection that was impossible for shipped detectors (§3 item 6), so D-4 changes nothing before D-3
  and is a separate package after it.
- Doc of `is_cloak_revealed`: "Whether `viewer` sees the concealed unit at `position` and detects it: its cell is
  `Visible` to `viewer` or it is attack-revealed to `viewer` (plan `2026-11-23-vision-and-detection` D-5), and its
  cell is detected. Detection alone is not sight (owner answer Q0101 2(b); plan
  `2026-11-24-owner-answers-cliff-and-vision` D-4)."
- The client needs nothing: `BattleFog::presence` already shows a unit listed in `attack_revealed` in a fogged cell
  and asks `revealed` for a concealed one; `revealed` now lists such an attacker during its reveal (P-12).

### D-5. Detectors under construction (Q0101 3(b), B-184)

In `collect_sights`, `detection_radius` is also filtered by `entity.is_completed`:

```text
detection_radius := arch.detection_radius
                    .filter(|_| entity.is_completed && !entity.status_modifiers().suppress_detection)
                    .unwrap_or(ZERO)
```

- Sight is unchanged (a foundation sees as today).
- At completion the source's `detection_radius` changes from `ZERO` to the authored radius, `sees_the_same` returns
  `false`, and `recast_changed_sights` removes and re-adds the source on the same tick's phase 3, so detection
  begins on the first vision update after the tick that completed it (**verify** the phase order in WP5: construction
  progress runs in an earlier phase of the same tick, or the one before; the test states which tick).
- A cancelled or destroyed foundation never detected, so its removal takes back nothing it did not add.
- The comment cites Q0101 3(b) and S5; `DESIGN.md` B-184 is fixed by it.

### D-6. The reveal and allies (Q0101 4(b))

No code changes. `record_attack_reveal`'s doc gains: "The reveal goes to the struck unit's owner and that player's
allies (owner answer Q0101 4(b)); the simulation has no teams (`match_state.rs`), so a player's only ally is
itself." The `DESIGN.md` B-55 row and the "Combat" row say the same. The test of P-10 in `attack_reveal_tests.rs`
that checks "third players are unaffected" gains the doc sentence "a third player is no ally: there are no teams".

**Why not build an alliance relation now:** it would decide shared vision, shared detection, friendly fire, victory
and command admission at once; nothing asks for it, and a one-off ally list read only by the reveal would be a
partial abstraction. When teams are planned, the reveal's audience is one of the rules that plan names (§14 F-1).

### D-7. Hashed state and checksum

- No field is added to `Entity`, `WeaponSpec`, `SplashSpec`, `PlayerVisionMap` or any hashed type, and no encoding
  changes. **The schema stays 43.** If an implementation finds it needs one, it stops and reports (stop rule 6); the
  number would be the next free at merge (42, 44 and 45 are reserved, so **46**).
- `detector_counter` is hashed (`checksum.rs`): D-3 and D-5 change its values wherever a detector's circle reaches
  cells its rays did not, or a foundation stands within a detector's own radius. The goldens and the sandbox field
  no detector (§7.3), so their checksums are predicted unchanged.

### D-8. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | Register **B-199**: "A `SameCliff` area counts a ramp as its lower landing and an air blast or air unit as the level of the ground below; the owner's answer to Q0099 is that a ramp counts as both levels (1(c)) and air blasts ignore levels (2(b))", open, plan `2026-11-24-owner-answers-cliff-and-vision`. Register **B-200**: "Detection follows the detector's line of sight (cliffs and blockers stop it); the owner's answer to Q0101 2(b) is a plain circle, with vision from any friendly unit", open, same plan. **B-184** restated: "Detectors under construction detect; the owner's answer to Q0101 3(b) is that they detect only once complete", open, same plan. Evidence file created | WP0 |
| `DESIGN.md` §4 | B-199, B-200 and B-184 fixed with commits and evidence. B-170's row gains "ramp and air readings per Q0099 (B-199)". B-55's row and the "Combat" row: the reveal's audience (D-6). The "Simulation vision and watchtowers" row: detection is a circle; foundations do not detect | WP6 |
| `docs/plans/2026-11-23-splash-cliff-levels.md` §14 F-2 | "Taken up by plan `2026-11-24-owner-answers-cliff-and-vision` (Q0099 1(c), 2(b))" | WP6 |
| `docs/plans/2026-11-23-vision-and-detection.md` §14 (B-184 line) and the As built "Open" paragraph | "Q0101 answered; taken up by plan `2026-11-24-owner-answers-cliff-and-vision`" | WP6 |
| New `docs/evidence/2026-11-24-owner-answers-cliff-and-vision.md` | §7's comparisons per package, the probes' counts, any traced first difference | WP0 creates; WP1 to WP6 append |
| `docs/questions/open/0099-…`, `0101-…` | `## Resolution` (what changed, which commits), then `git mv` to `closed/` | WP6 |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Done` (WP6) | plan, WP1, WP6 |

B-numbers B-199 and B-200 are the next free at this plan's writing (numbers below B-199 are held by other branches); if
`main` has taken them at WP0, WP0 takes the next free and says so in an amendment.

---

## 5. Determinism and checksum

- **Integer only.** `CliffSpan` compares `u8` levels; the disc test is the whole-cell square distance the rays
  already compare against fixed-point radii squared. No float enters the simulation; C7's allowances do not change.
- **No new randomness, clocks or collections.** The disc walk is a fixed double loop in offset order; it reads no
  container.
- **No schema change** (D-7). The packages move checksums only by behaviour:
  - WP1, WP2 (splash): only where §7.2's probe (a) counts a unit newly struck, or a fallback turned into an area
    strike. Predicted none in the goldens (no splash weapon fires in either replay: their tanks never siege).
  - WP3 (circle): only where a detector stands (probe b). Predicted none in the goldens (no detector; the orders
    replay's `flying_saucer` is not one) and the sandbox (its domes are a town hall, a supply and a production
    structure, chosen with `weapon.is_none()` for production; detectors are armed).
  - WP4, WP5: only where probes (c) and (d) count.
- **If a golden moves**, the vision plan's dump procedure (§5 there) applies: before and after dumps per checkpoint,
  the first difference must be a counted probe event (a detector counter at a counted cell, or a counted strike),
  and the re-record header names the package and the event. An unexplained move stops the package.
- **wasm.** No dependency changes. The package gate's determinism jobs run as usual.
- **Throughput.** The splash change adds one `Air` comparison per point and per unit in reach; the disc adds about
  `π r²` cell marks per detector recast (a recast happens about once in ten ticks per moving detector, and never
  for a structure). The bench is checked in the package gate.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` (a cliff edge, a ramp
run, a level-2 cliff beside a ramp, a 3×3 blocker), read every radius and range from the catalog, and find weapons,
detectors and cloakers by property (`splash.cliff`, `splash.impact`, `mover_type`, `detection_radius`,
`target_flags`, a cloak ability), never by slug.

**Q0099**
- **P-1 (span).** `cliff_span_at` returns `single(level)` on flat cells; `[low, low + 1]` on every cell of a ramp
  run, for each cardinal direction and run lengths 1 and 3; the clamped border cell's span off the map on each side
  and corner. `overlaps` is reflexive and symmetric; two adjacent single levels do not overlap; a ramp's span
  overlaps both landings and neither `low − 1` nor `low + 2`; `ALL` overlaps every span.
- **P-2 (ramp, 1(c)).** A `Same` area centred on the plateau at a ramp's top strikes a unit on the ramp's top cell
  as an `Any` area does; one centred on a ramp cell strikes units within reach on both landings and spares a unit on
  a third level (a level-2 cell beside the ramp); a centre on the low ground beside the cliff still spares a unit on
  the plateau (the cliff, not the ramp, separates them).
- **P-3 (air, 2(b)).** A `Same` area on the air plane strikes air units over both levels at the same distance alike,
  and a `CliffWalker` unit on another level too; an area centred on a `CliffWalker` target by a weapon that hits air
  and ground strikes an air bystander over another level and spares a ground bystander on another level.
- **P-4 (centre, 3(a)).** With `offset_by_unit_radius`, a target whose centre stands on the plateau but whose edge
  facing the attacker lies over the low ground is the centre of a search on the low ground: a low-ground bystander
  within reach is struck, a plateau bystander at the same distance is spared. *Green before WP1; it pins what is
  built.*
- **P-5 (fallback, rule 4).** A burst from a ramp cell at a target on the upper landing strikes the target once, by
  the area; a burst from the low ground at a target on the plateau strikes it once, by the fallback (unchanged); no
  target within the burst's reach is struck twice or not at all.
- **P-6 (direction).** At the first differing tick of any world that moves in WP1 or WP2, the struck set of the
  first differing search is a strict superset of the WP0 tree's for the same state, and the added units are exactly
  those whose spans overlap under the new rule and did not under the old.

**Q0101**
- **P-7 (circle).** A ground detector below a plateau detects every plateau cell within its radius and sees none of
  them; one outside a blocker detects the blocker's cells and the cells beyond within its radius and sees none of the
  blocker. Its states and visible counters equal those of the same source without detection, cell for cell, on
  every test map. Detection never exceeds the radius and covers the source's own cell.
- **P-8 (incremental).** Adding and removing detectors with unequal radii across cliffs, ramps and blockers leaves
  counters and states equal to a full recount.
- **P-9 (open ground).** On an open flat map, for every whole radius 1 to 32 and the shipped 11, the detection disc
  equals the cells the rays reach. **verify**: if the rays leave gaps, the test records the disc as authoritative and
  WP3 reports which cells, since air detectors then change too (probe b counts them).
- **P-10 (Q0101 2, end to end).** A detector structure at the foot of a cliff, a cloaked enemy air unit over the
  plateau within its detection radius, and an unarmed air spotter of the detector's player over the plateau: the
  detector's side strikes the cloaked unit. Without the spotter it does not (detected, unseen: rule 6), and the
  unit is absent from `PlayerView::revealed`. With the spotter and a scratch copy of the detector with no detection
  radius, it does not. The same with a blocker instead of a cliff, the spotter inside the blocker.
- **P-11 (reveal and detection, D-4).** A cloaked air attacker over a plateau striking a unit of the detector's
  player, within the detector's radius, with no spotter: the detector's side strikes it during the reveal, the
  attacker is listed in `PlayerView::revealed` during it, and both stop once the reveal ends. Outside the detection
  radius, the reveal alone never makes it a target (`a_cloaked_attacker_stays_hidden_without_detection`, unchanged).
- **P-12 (construction, 3(b)).** A detector foundation within its radius of a cloaked enemy that a spotter of the
  same player sees: the enemy is not detected and not struck while the foundation builds; it is detected from the
  first vision update after completion; the vision maps then equal a full recount; the foundation's sight is the
  same as before the change throughout.
- **P-13 (allies, 4(b)).** Unchanged behaviour: the reveal goes to the struck player; a third player is unaffected
  (no teams). The rule is in the docs (D-6).
- **P-14 (latent where nothing counts).** Every package changes only the worlds its probe counted (§7.2), and in each
  the first difference is a counted event.

---

## 7. Worlds that move

### 7.1 Commands

The weapon-validation scripts (`~/scratch/weapon-validation/scripts/measure.sh`, `compare.sh`, `client.sh`,
`proof.sh`), copied to `~/scratch/owner-answers/scripts/` with their paths changed. `measure.sh` runs `route_trace`
with `--assets <tree root>/assets`, so each tree is measured with its own data. Binaries are built once per tree:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/owner-answers
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64, and the
showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks. Run it in the
background, one at a time across worktrees (charter §7). The client skirmish (`skirmish_match_tests`) is compared by
its stage log (`client.sh`).

### 7.2 WP0's probes

On WP0's tree a scratch patch prints four kinds of event; it is never committed and is kept under
`~/scratch/owner-answers/probe/` with its diff. The probe plays every world of §7.1 and the two goldens (through a
scratch test). The evidence records each count per world and seed and the first event of each kind.

- **(a) Splash across a ramp or in the air.** In `strike_area`, every unit within reach and passing the area's
  filters that today's level test excludes, tagged `ramp` when the new spans overlap because the point or the unit
  is on a ramp cell, `air` when they overlap because the area's plane or the unit is `Air`, else `cliff` (still
  excluded). In the attacker-centred branch, every fallback strike whose target's span overlaps the burst centre's
  under D-1 (it becomes an area strike). *Expected:* zero `ramp`, `air` and fallback events everywhere. The
  splash-cliff probe found no area reaching a unit on another level in any measured world, battle 40 included
  (its Amendment 2); since then only weapon validation moved worlds (battles 31, 35, 42, showcase effects), none by
  adding splash near a ramp. The matrix's only air area, `sky_bastion`'s, is in no battle.
- **(b) Detection beyond the rays.** At each vision update, for every detector source, the cells within its
  detection disc that its rays do not reach, tagged ground or air source; and among them each cell holding a
  concealed enemy of the source's player that the player sees or that is attack-revealed to it. *Expected:* zero
  detector sources in every measured world and golden: no matrix battle, showcase loadout (`battle_automaton`,
  `showcase_siege_line`, `showcase_raiders`, `standard_automaton`) or golden names a detector (the client skirmish's
  pilot: **verify** from its stage log).
- **(c) Revealed, detected, unseen.** Every weapon-filter, retaliation and view check on a concealed enemy that is
  attack-revealed to the checking player, detected by it and in a cell it does not see. *Expected:* zero today by
  construction (§3 item 6), and zero after WP3 in every measured world (no detector).
- **(d) Detectors under construction.** Every tick on which a detector foundation exists, and each concealed enemy
  within its detection radius. *Expected:* zero everywhere (no measured world builds a detector).
- **Rule:** with every count zero, WP1 to WP5 are byte-identical in every measured world and golden, and the
  answers are proved by the constructed tests of §12 (P-1 to P-13). A nonzero count makes the counted worlds the
  ones that package may move (P-14). No new battle is added (§14 F-3).

### 7.3 Expected moves

| World | WP1 (1(c)) | WP2 (2(b)) | WP3 (circle) | WP4 (D-4) | WP5 (3(b)) |
|---|---|---|---|---|---|
| Matrix (seeds of record and 16) | byte-identical (probe a: battle 40 is the only arena with a ramp) | byte-identical (no `sky_bastion`) | byte-identical (no detector) | byte-identical | byte-identical |
| Battles 31, 33 | byte-identical (flat) | byte-identical | byte-identical | byte-identical | byte-identical |
| Showcase, default and 1–8 | byte-identical (no splash weapon) | byte-identical | byte-identical (no detector) | byte-identical | byte-identical |
| Showcase effects, default and 1–8 | **moves only on seeds probe (a) counts** (sieged tanks above the west ramp; predicted none) | byte-identical (no air area) | byte-identical (no detector; the burrowed Lurker analogues meet no detector) | byte-identical | byte-identical |
| Client skirmish stage log | identical (**verify**) | identical | identical unless the pilot builds a detector (**verify**) | identical | identical unless the pilot builds a detector |
| Goldens, sandbox | unchanged | unchanged | unchanged (no detector) | unchanged | unchanged |

**What a moving package records** for every world that moved (in the evidence, all seeds, before and after): the
first differing tick and the probe event behind it, each side's survivors and the tick of the last death.

**Direction (a prediction, not a threshold).** WP1 and WP2 only add struck units (P-6), so a side firing `Same`
areas near ramps or at air deals no less damage. WP3 and WP4 only add targets for a detector's side; WP5 only removes
detection while a detector builds. Which side remains should not change for a battle whose claim is about something
else; a change is traced (§8 rule 3).

---

## 8. Stop rules

1. **Every package moves only the worlds its probe counted** (P-14). A difference elsewhere, or on a seed with a zero
   count, is a defect of the package, not a re-record.
2. **A move whose first differing tick is not a counted event**, or that breaks P-6's superset property, stops the
   package until traced. If the trace shows a second cause, it is registered as a bug (from B-201) and the package
   lands only if its rule's change can be separated from it.
3. **A change of which side remains** is traced to its first counted event and reported. It is not by itself a stop,
   as each rule is the owner's answer. A matrix claim that breaks is reported to the orchestrator with its trace and
   not rewritten in the package.
4. **No tuning constant.** Every value is the map's, the catalog's or an existing engine rule's; the only new rule
   values are `CliffSpan::ALL` and "a ramp spans `low + 1`".
5. **Nothing under `crates/sim_core/src/movement/` changes.** A package that needs it stops and reports.
6. **No hashed field and no schema change.** A package that needs one stops and reports; the number would be the
   next free at merge (46).
7. **A golden move** follows §5's dump procedure or stops the package.
8. **P-9 failing** (rays leave gaps on open ground) is not a stop: WP3 records the cells, probe (b) counts air
   detectors, and the evidence states which worlds that adds. An amendment records it if any measured world moves
   by it.

---

## 9. Client consequences

- **Q0099:** none. The client draws a blast from its own effect blocks and the `SplashVisual` events, which are
  unchanged (splash-cliff D-6). Units on a ramp or in the air that are now struck show their damage as any struck
  unit does.
- **Q0101 2(b), D-4, 3(b):** no client code. `PlayerView::revealed` keeps its meaning ("concealed enemies the viewer
  sees and detects"); D-4 adds attack-revealed concealed units to it while their reveal lasts, which
  `BattleFog::presence` already draws (it accepts `attack_revealed` units in fogged cells and asks `revealed` for
  concealed ones). The editor's line-of-sight preview (`editor/los_preview.rs`) casts with detection `ZERO` and is
  unaffected. `battle_fog_tests.rs` and `sync_steady_state_tests.rs` run in the package gate on their catalog
  triggers; no capture or storyboard moves (no showcase take fields a detector).
- **Q0101 4(b):** none.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP6 | D-8 |
| `docs/plans/README.md` | plan, WP1, WP6 | Row |
| `docs/evidence/2026-11-24-owner-answers-cliff-and-vision.md` (new) | WP0 to WP6 | §7 |
| `crates/sim_map/src/grid.rs`, `crates/sim_map/src/lib.rs` | WP1 | `CliffSpan`, `cliff_span_at` replacing `cliff_level_at`, unit tests |
| `crates/sim_map/tests/ramp_profile_tests.rs` | WP1 | `cliff_level_at_reads_the_lower_landing_on_every_ramp_cell` re-derived as `cliff_span_at_covers_both_landings_on_every_ramp_cell` (P-1) |
| `crates/sim_core/src/combat_resolution.rs` | WP1, WP2, WP4, WP6 | D-2 (`level_span`, the buffer's type, both rules, invariant 6); D-4 (`StrikeContext::is_available`); D-6 doc |
| `crates/sim_types/src/weapon.rs` | WP1, WP2 | `SplashCliff::Same` and `SplashSpec` docs |
| `crates/sim_core/tests/splash_cliff_tests.rs` | WP1, WP2 | §12; two tests re-derived |
| `crates/sim_map/src/vision.rs` | WP3 | D-3; module and `VisionSource` docs; the 1(a) comment reworded |
| `crates/sim_map/tests/vision_tests.rs` | WP3 | P-7 to P-9; `detection_follows_line_of_sight` and `equal_radii_detect_exactly_the_seen_cells` re-derived; `air_and_towers_see_into_a_blocker`'s doc reworded |
| `crates/sim_core/tests/detection_circle_tests.rs` (new, `//!` doc) | WP3, WP5 | P-10, P-12 |
| `crates/sim_core/src/target_acquisition.rs` | WP4 | D-4 (`is_cloak_revealed` and its two readers there) |
| `crates/sim_core/src/player_view.rs` | WP4 | D-4 (the `revealed` list) |
| `crates/sim_core/tests/attack_reveal_tests.rs` | WP4, WP6 | P-11; P-13's doc sentence |
| `crates/sim_core/tests/player_view_tests.rs` | WP4 | `revealed` lists an attack-revealed, detected, concealed attacker (P-11) |
| `crates/sim_core/src/world/vision.rs` | WP5 | D-5 |
| `crates/gate/suites.ron` | WP3, only if needed | The new test target (**verify**: an uncovered target may fall to its crate's default class; C22) |
| `docs/plans/2026-11-23-splash-cliff-levels.md`, `docs/plans/2026-11-23-vision-and-detection.md` | WP6 | Pointers (D-8) |
| `docs/questions/open/0099-cliff-level-of-ramps-and-air.md`, `docs/questions/open/0101-vision-and-detection-details.md` | WP6 | `## Resolution`, moved to `closed/` |

No catalog file, fixture RON, golden or `crates/sim_core/src/movement/` file changes. **Overlap:** the contact solver
owns `movement/` and reserves schema 42; this plan touches neither. Weapon validation's WP7 (B-195) will edit pursuit
and acquisition's chase; D-4's change to `is_entity_targetable` is a parameter it passes through, so a merge conflict
there is mechanical.

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`
(check `quota -s` and `df -h ~/scratch` first). Write the failing tests first and paste their red run in the
hand-back. Run `cargo gate quick` and the tests the change needs; a verifier runs `cargo gate package` (Q0061). Run
§7's comparison in every package against WP0's tree. No `unwrap`; `expect` only with `invariant:`; no catalog
identity in production Rust (C5); tests find units by property. Check `docs/questions/open/` before the last commit.
WP0 to WP6 may run as one branch and one session, one commit each, with one `package` gate on the final tree
(charter §4.3).

### WP0: records, baseline and probes (XS, docs only)

- `DESIGN.md` §4: B-199 and B-200 registered, B-184 restated (D-8). Evidence file created, stating §3 items 3 and 4
  (Q0099 3(a) and Q0101 1(a) already hold, with the code and tests that show it).
- Measure WP0's tree (§7.1) for the baseline. Run §7.2's probes and record their counts and first events. State in
  the evidence which worlds each package may move.
- **Gate:** docs-only.

### WP1: a ramp spans both landings (S, `sim_map`, `sim_core`, Q0099 1(c), B-199)

Failing tests first (red on WP0's tree: they do not compile, or fail on the old reading):
- `grid.rs` unit tests for `CliffSpan::overlaps` and `cliff_span_at` (P-1); `ramp_profile_tests.rs`
  `cliff_span_at_covers_both_landings_on_every_ramp_cell` (P-1, replacing the lower-landing test).
- `splash_cliff_tests.rs`: `a_same_level_area_counts_a_ramp_as_both_landings` (re-derived from
  `…_counts_a_ramp_as_its_lower_landing`: the unit on the ramp's top cell is struck as the one beside it),
  `a_shot_centred_on_a_ramp_strikes_both_landings_and_spares_a_third_level` (P-2; a terrain variant with a level-2
  cell beside the ramp), `a_burst_on_a_ramp_strikes_its_target_at_the_top_by_its_area` (P-5).
- `splash_cliff_tests.rs` `the_level_is_read_at_the_blast_centre` (P-4): **green on WP0's tree**, pinning Q0099 3(a);
  its red run is not required, and the hand-back says so.

Then D-1 and D-2's span part (`level_span` with the ground span only; the `Air` case is WP2), the buffer type, the
fallback, invariant 6 (ramp half), the `weapon.rs` docs (ramp half). **Acceptance:** §8 rules 1, 2; P-6 on any moved
world. Plan row `Active`. Commit `fix(sim_core): a ramp counts as both cliff levels for area damage, owner answer
Q0099 1(c) (owner-answers-cliff-and-vision WP1, B-199)`.

### WP2: air blasts ignore cliff levels (XS, `sim_core`, Q0099 2(b), B-199)

Failing tests first (red on WP1's tree), in `splash_cliff_tests.rs`:
- `an_air_area_ignores_levels` (re-derived from `an_air_area_keeps_to_the_level_below`: both air bystanders struck
  alike), with a `CliffWalker` bystander on the other level struck too (P-3).
- `an_air_unit_in_a_ground_centred_area_ignores_levels` (P-3: a scratch weapon hitting air and ground, a
  `CliffWalker` target cloned from a shipped one found by `mover_type`, an air and a ground bystander on the other
  level).

Then D-2's `Air` case and the remaining docs. **Acceptance:** §8 rules 1, 2. Commit `fix(sim_core): air blasts and
air units ignore cliff levels, owner answer Q0099 2(b) (owner-answers-cliff-and-vision WP2, B-199)`.

### WP3: detection is a circle (S, `sim_map`, Q0101 2(b), B-200)

Failing tests first (red on WP2's tree):
- `vision_tests.rs`: `detection_is_a_plain_circle` (P-7, replacing `detection_follows_line_of_sight`: the plateau
  cell (16, 35) of the default arena is detected from (16, 24) with radius 12 and not `Visible`; a blocker's cells
  are detected from outside and not `Visible`), `equal_radii_detect_exactly_the_seen_cells` re-derived (on the open
  flat map detection equals sight; on the arena and the blocker map, visibility equals the plain source and
  detection equals the disc), `detection_and_incremental_casts_equal_a_recount_across_terrain` (P-8, or the existing
  incremental test extended), `on_open_ground_the_rays_cover_the_detection_disc` (P-9, **verify**; may be green).
- New `crates/sim_core/tests/detection_circle_tests.rs`: `a_detector_below_a_cliff_detects_what_a_spotter_sees`,
  `a_detected_unit_nobody_sees_is_no_target`, `a_detector_without_detection_cannot_strike_the_cloaked_unit`,
  `a_detector_outside_a_blocker_detects_inside_it` (P-10, through ticks with the detector structure found by
  `detection_radius` and an anti-air weapon, the cloaked unit found by a cloak ability or by its archetype's
  cloaking capability, a spotter cloned unarmed from a shipped air unit, as `blocker_edge_tests.rs` does).

Then D-3, the docs, and the 1(a) comment and test doc ("Q0101 part 1, answered (a)"). **Acceptance:** §8 rules 1, 2,
8. Commit `fix(sim_map): detection is a plain circle that no terrain stops, owner answer Q0101 2(b)
(owner-answers-cliff-and-vision WP3, B-200)`.

### WP4: a revealed, detected cloaked attacker is a target (S, `sim_core`, D-4, B-200)

Failing tests first (red on WP3's tree):
- `attack_reveal_tests.rs` `a_detected_cloaked_attacker_is_answered_during_its_reveal` (P-11: detector structure
  below a plateau, a cloaked air attacker found by property over the plateau within the detection radius striking a
  unit of the detector's player, no spotter; struck during the reveal, not after it ends).
- `player_view_tests.rs` `revealed_lists_an_attack_revealed_detected_attacker` (P-11).
- `a_cloaked_attacker_stays_hidden_without_detection` passes unchanged (the control).

Then D-4. **Acceptance:** §8 rules 1, 2. Commit `fix(sim_core): an attack reveal counts as sight of a detected
cloaked attacker (owner-answers-cliff-and-vision WP4, B-200)`.

### WP5: detectors under construction do not detect (S, `sim_core`, Q0101 3(b), B-184)

Failing tests first (red on WP4's tree), in `detection_circle_tests.rs`:
- `a_detector_foundation_detects_nothing` and `a_detector_detects_from_its_completion` (P-12: a foundation placed
  through the construction command path, as `construction_lifecycle_tests.rs` places one, next to a cloaked enemy
  that a spotter sees; the test states the first tick of detection relative to the completion tick).
- `a_completed_detector_recasts_to_a_full_recount` (P-12: the incremental maps after completion equal a fresh world's
  recount, through a forced full recount, e.g. a cell-revision bump, **verify** the means the tests already use).

Then D-5. **Acceptance:** §8 rules 1, 2. Commit `fix(sim_core): a detector detects only once its construction
completes, owner answer Q0101 3(b) (owner-answers-cliff-and-vision WP5, B-184)`.

### WP6: records and the questions closed (XS)

D-6's docs and test doc sentence; D-8's records; the evidence completed; this plan's `## As built`; README row
`Done`. The `## Resolution` of Q0099 and of Q0101 (each part: what changed and in which commit, or "already built",
with the commit that built it: Q0099 3(a) `e839bb4c`, Q0101 1(a) `e52c0c83`; 4(b): "no teams; recorded in the docs"),
then `git mv` of both files to `docs/questions/closed/`. Merge `main`; `cargo gate package` once on the final tree (by
the verifier).

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where |
|---|---|---|
| Map: spans | P-1 | WP1: `grid.rs` unit tests, `ramp_profile_tests.rs` |
| Map: detection | P-7, P-8, P-9 | WP3: `vision_tests.rs` (the `sim_map` line-of-sight subject) |
| Splash rules | P-2 to P-5 | WP1, WP2: `splash_cliff_tests.rs` |
| Detection end to end | P-10, P-12 | WP3, WP5: `detection_circle_tests.rs` |
| Reveal and detection | P-11, P-13 | WP4, WP6: `attack_reveal_tests.rs`, `player_view_tests.rs` |
| Worlds | P-6, P-14: §7.3 | every package (measure scripts); goldens and sandbox in the package gate (release and `wasm32-wasip1`) |
| Charter subjects | `sim_map`'s and `sim_core`'s subjects of the guide (`vision_tests.rs`, `ramp_vision_tests.rs`, `damage_and_shields_tests.rs`, `full_match_tests.rs`, `golden_replay_tests.rs`, …) and the client's `battle_fog_tests.rs` keep passing | package gate |

Re-derived (expected to change), each named in its commit: `ramp_profile_tests.rs`
`cliff_level_at_reads_the_lower_landing_on_every_ramp_cell` and the `cliff_level_at` unit tests of `grid.rs` (WP1);
`splash_cliff_tests.rs` `a_same_level_area_counts_a_ramp_as_its_lower_landing` (WP1) and
`an_air_area_keeps_to_the_level_below` (WP2); `vision_tests.rs` `detection_follows_line_of_sight` and
`equal_radii_detect_exactly_the_seen_cells` (WP3). Any other test that relies on a foundation detecting (search
`is_completed = false` beside a detector in `crates/sim_core/tests/`, WP5) or on a ramp or air unit being spared
(search `CliffLevel(1)` with `RampDir` or `MoverType::Air` in splash tests, WP1, WP2) is re-read against the answer.

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | "Air blast" read as an area on the air plane and "air unit" as `MoverType::Air` (§2.2 rule 2); a `CliffWalker` struck unit keeps the ground span | A Colossus analogue below a cliff is spared by a ground-centred `Same` blast that the owner might expect to hit it | It stands on the ground; the answer names air; one line in `level_span`; §14 F-2 |
| R-2 | D-4 goes beyond the literal answers | A reader takes it as unasked | It is the composition of two built or answered rules (§2.2 rule 7) and the game's well-known turret-against-Banshee case; its own package and commit, easy to revert |
| R-3 | The rays leave gaps on open ground (P-9) | Air detectors detect a few more cells than they see | P-9 states it; probe (b) counts it; detection as a circle is the answer's meaning |
| R-4 | A world moves that the probe did not predict | Unplanned re-records | Stop rules 1 and 2 |
| R-5 | Weapon validation's WP7 edits acquisition and pursuit near `is_entity_targetable` | Merge conflict | D-4 only adds an argument; the second branch to merge rebases it |
| R-6 | Nothing measured moves | The answers are proved only by constructed worlds | §7.2's rule; §14 F-3 names the battle that would show them and why it is not added |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: teams.** No alliance relation exists; when teams are planned, the attack reveal's audience (Q0101 4(b)),
  shared vision and shared detection are among the rules that plan sets. Nothing here waits for it.
- **F-2: the `CliffWalker` readings.** Kept as built: a Colossus analogue sees up cliffs and into blockers as an air
  source (vision plan), and stands on the ground for `SameCliff` (D-2). Q0101 1(a) and Q0099 2(b) name air units;
  neither asked about the Colossus. No question is filed; a change is one line in each place.
- **F-3: no new matrix battle.** The worlds of record field no ground detector, and every ground detector is a
  structure, which `sim_fixtures::matchup` spawns only as units (`spawn_unit_facing`), not as placed structures with
  footprints and construction. A battle with a Missile Turret
  analogue below a cliff would need structure squads in the fixture, its own extension; the answers are proved by
  constructed end-to-end tests through ticks (P-10 to P-12), as splash-cliff's Amendment 2 decided for its ramp. The
  orchestrator may ask for it.
- **Scanner Sweep, Revelation, siege-mode detectors:** not shipped; the circle rule applies to them when they are.
- **B-177** (the Lurker's `CliffLevelGE1`) and **B-183** (snapshots) stay their own rows.

---

## 15. Owner questions

None. Every reading this plan makes that the answers do not state outright is recorded with its reason (§2.2 rules 2
and 7, D-4, D-6, §14 F-2) and is one line to change.

---

## 16. Material (scratch, not committed)

- The owner's answers in `docs/questions/open/0099-cliff-level-of-ramps-and-air.md` and
  `docs/questions/open/0101-vision-and-detection-details.md` (commit `20322ca6`).
- The game data of the two source plans, `~/scratch/combat_wp3_data/` (`5444e12f13`), for S3 to S8.
- The catalog survey of §2.3 (`detection_radius: Some`, `splash` with `cliff` defaults, `mover_type`) and of the
  measured worlds' units (`matchup_matrix.ron`, `start_loadouts.ron`, both goldens), read with `grep`.
- No prototype: every package is predicted byte-identical in every measured world, and the probes of §7.2 confirm or
  name the worlds before any rule lands.

## As built

