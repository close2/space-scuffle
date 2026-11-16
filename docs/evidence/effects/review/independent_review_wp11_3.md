# Independent Review of WP11 (the Weapons' Own Looks), Round 3

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP11 row of
§9, §7.2 (checklist), Amendments 24 to 26. Packet: [`independent_review_packet.md`](independent_review_packet.md),
section "WP11 round 3" and the shared sections. Earlier rounds:
[`independent_review_wp11_1.md`](independent_review_wp11_1.md), [`independent_review_wp11_2.md`](independent_review_wp11_2.md).
Branch `effects-wp11`, head `5de90a97`. This is round 3 of at most five (Q0080). I did not build this work and I did
not write rounds 1 or 2.

## Method

I read the packet section, Amendment 26, round 2 in full, and the diff `e471cede..5de90a97` of `crates/`, `assets/`
and the plan. I read these in full: the new `effects/struck.rs`; the changed `systems.rs` (`spawn_request_struck`,
`shot_clip`, `spawn_effects_from_events`); `placement.rs` (`Behind`, `Across`); `runtime.rs` (`card_roll`,
`facing_roll`); `catalog/resolve.rs` (`shot_reach`, `layer_reach`); the `psi_slash`, `claw_rake`, `impaler_strike` and
`needle_volley` entries of `effects.ron`; and the changed tests (`weapon_battle_gpu_tests.rs`, the beside-the-fog
and visibility parts of `effect_slice_gpu_tests.rs`, `weapon_look_tests.rs`). To judge the anchor I checked where the
simulation publishes an instant hit and its angle (`sim_core::combat_resolution::emit_instant_hit_visual`,
`hit_angle`), where a bubble stands (`sim_core::world::snapshot::bubble_instance`, `AIR_UNIT_ALTITUDE`), where the
client draws its sphere (`scene_controller.rs`, centre at `world_pos[2] + radius`), the camera pitch (58°), which
entities the snapshot lists as bubbles (all of them, structures included), and what the valgrind effect stream sends
(`sync_allocation_count_tests.rs`). I built and ran nothing, as instructed.

All pictures are lavapipe. I cut the 1:1 frames of the fifteen battle sheets round shooter and target, enlarged them
3 to 4 times without smoothing, diffed frames against each other, and cut the looks sheets' cells. Crops are in
`~/scratch/wp11-review3/` (`gauss_tgt.png`, `gauss_diff.png`, `tgtdiff.png`, `weld_all.png`, `psi_all.png`,
`claw_all.png`, `imp_all.png`, `prism_all.png`, `snip_all.png`, `nv_all.png`, `nv_lab.png`, `lab4.png`). Sizes are
screen pixels at the default zoom (about 34 px per world unit).

## Verdict table

| Item | Verdict | Main reason |
|---|---|---|
| A2-1 instant and melee impacts inside the bubble | **fixed for ground targets attacked across the screen; partly overall** | In the battle at 1:1 every impact now shows on the struck bubble's side facing the attacker: `bullet_hit` (rifle and sniper) a 6 to 8 px hot point on frames 5 to 7 after the slug; `cannon_hit` a fireball left of the target; `beam_burn` and `lance_burn` a bloom where the beam meets the glass; `weld_sparks` a hot point in the seam; the spike between the units. Not covered: **air targets**, whose hits stay at ground height 2.5 units below the bubble (A3-1), and **attacks from the far side of the screen**, where the point lands at the top rim behind the glass (A3-2) |
| A2-2 melee faces the attack | **fixed** | `psi_slash` is an ice-blue crescent wrapping the target's near side, its middle towards the attacker, swept open over two frames (frames 2 to 4 and 8 to 10). `claw_rake` is three dark-red arcs across the line of the blow. `facing_roll` is derived and the sim's `angle_rad` is from the target towards the attacker, as `struck.rs` assumes. See P3-1 |
| A2-3 two needles | **fixed** | Two distinct dark spindles about 12 × 3 px each, side by side ("="), in the lab and in flight in the battle |
| E2-1 fog reach per end, positive control | **fixed, with E3-4** | `shot_reach` separates muzzle and path; the far take (1.3 × the largest reach) must draw at least 30 px with none over unseen ground, and does (357 to 6875 px). The improvement itself (a unit near unseen ground no longer cut by the muzzle's reach) has no test |
| E2-2 visibility measured where the game draws | **partly (E3-1)** | The lab binds `Facing` and `UnitRadius`; the battle now measures the impact beside the struck bubble. But the battle measure is weaker than it claims (E3-1) |
| E2-3 battle pass test discriminating | **partly (E3-1)** | A units-only baseline and a window round path and target are in. The bubble masks are centred on the ground point while the spheres are drawn a radius higher, neighbouring subjects' units fall in the window, and an instant shot's slug counts as its impact |
| E2-4 display colours | **fixed** | Medians of each look's own changed pixels; the A1-4 hues show (`psi_slash` (148, 194, 253), `prism_beam` (175, 120, 238), `plasma_bolt` (111, 143, 214), `spore_bolt` (160, 194, 102)) |
| E2-5 independent impact-delay test | **fixed** | The slug's head is found from built frames for every shipped instant travel, and the impact must draw nothing before it reaches the end (one frame's advance of tolerance). Independent of the delay formula |
| P2-1 `cannon_hit` rosette | **fixed** | A hot core under the lobed fireball at every age |
| P2-2 held beam | **recorded in Amendment 26, not in `DESIGN.md` (E3-2)** | |
| P2-3 `glass_shatter` shards | **partly** | Three to five pale flakes visible at the pictured ages, not the 12 to 18 claimed (P3-2) |
| P2-4 `acid_hit` crown | **partly** | The disc's edge is a little broken; at 1:1 it still reads as a green button with a glyph (P3-3) |
| Struck bubble by position | **sound for ground crowds; see A3-1, E3-3** | The published point is the target's centre, so its own bubble is nearest (distance = interpolation lag) unless another bubble's centre is nearer, which collision prevents on the ground; only the radius is taken from the match, never the centre. The 0.8 cap keeps the slug from inverting (the muzzle trim uses the moved length). An unfound event is unchanged |
| Steady state, O(N²) (charter §3.3) | **allocation-free by inspection; search bound not tight (E3-3)** | Retained buffer, in-place unstable sort, no heap in `CombatEventVisual`. The valgrind stream sends only splashes and has no `StruckBubbles`, so the new path is not counted. The x-band query is O(N) per event in the worst case, widened by the largest structure's radius |
| **Overall** | **ACCEPT WITH CONDITIONS** | The round-2 blocker is fixed where it was found: every instant and melee impact now reads at 1:1 on the struck ground bubble's near side, and the melee looks face the blow. Conditions C1 to C5 below: air targets, far-side attacks, a sound battle measure, the register, and a bounded search |

## Findings

### A3-1 (condition): hits on air units are drawn on the ground, 2.5 units below the bubble

`emit_instant_hit_visual` publishes `to` at `terrain_altitude_at(target) + 0.5` for every target, while an air unit's
bubble stands at `ground + AIR_UNIT_ALTITUDE (2.5)` and is drawn a radius higher still. `at_struck_surface` finds the
air bubble by `x, y` and moves the point back by its radius, but keeps `to[2]`. So a tracer or beam at a flyer runs to
the ground under it and the burn or hit shows there, about 50 screen pixels below the flyer at the default pitch.
Every anti-air instant weapon (the rifle analogue's included) is affected. This predates round 3 (the legacy cards did
the same), but A2-1 asked for the hit on the struck bubble, and the client now has that bubble.

Required: store the bubble's drawn centre height in `StruckBubbles` and set the moved point's height to it (the slug
then ends on the flyer's glass); where an air and a ground bubble overlap in plan view, prefer the one whose plane
matches the weapon slot (`EffectSource::weapon.slot` `Air`/`Ground`) and otherwise the nearest in three dimensions
using the published point. Add one air target to the staged battle. If the owner prefers to wait for the target's id
in the event, register it as a bug in `DESIGN.md` with that fix instead.

### A3-2 (condition): an attacker on the far side of the screen puts the hit behind the glass

The staged battle fires only along screen `+x`. With the camera at 58°, a point on the struck bubble's equator on the
side away from the camera (attacker "above" the target on screen) lies on a sight line that passes `0.56 × sin 58° ≈
0.47` units from the centre of a 0.5 bubble: inside its silhouette, at the top rim, behind the glass. That is the
round-2 failure for one quarter of attack directions. Required: stage one attack from screen-up and one from
screen-down; if the far-side hit is hidden or tinted, place it where it is seen (for example raise it towards the
bubble's top, or step it towards the camera by the silhouette margin).

### E3-1 (condition): the battle measure does not mask what it says it masks

- `project` places each mask on the unit's ground point, but `scene_controller` draws the sphere `radius` higher,
  which at 58° moves its disc about `0.53 × radius` up the screen: about 8 to 9 px for the target (I measured the drawn
  centre 9 px above the subject's ground line in the gauss crop). With a margin of 2 px the top slice of both bubbles
  is "outside both bubbles", and the units' flipbooks animate there (`gauss_diff.png`: the shooters' interiors change
  every frame). In the melee subjects the shooter touches the target, so the shooter's own swing in that slice lies in
  the target's attacker-side impact zone. The repeated 39 px (weld, gauss, cannon) may be partly this.
- The window round the target (radius `target_r + 0.7` units, about 41 px) takes in the neighbouring subject's
  bubble, which touches the target in waves 0, 1 and 3 (the green Rhino beside the gauss target): its animation
  counts towards "round path and target".
- The impact frames start at the spawn request, but an instant shot's impact is delayed until its slug arrives (110
  to 150 ms), so the four frames hold the slug's approach, which counts as impact pixels. For tracers and beams the
  check passes with an empty impact look.

Required: project the drawn centre (ground + radius, + altitude for air), mask every presented bubble in the crop,
not only the subject's two, and start the impact window at the impact's birth (`delay` after the request). Show that
the check fails with the impact look removed for one tracer, one beam and one melee family (a planted negative).

### E3-2 (condition): the register

`DESIGN.md` is not changed in round 3. Register as bugs: the struck bubble found by position (exact match needs the
target's id in the event, a simulation change), P2-2 (a held beam is a series of fixed lines), and A3-1 if it is not
fixed. Refresh the "Effect presentation" row to Amendment 26. Note also that Amendment 26's description of the crowd
case is inexact: a wrong match moves the point by the neighbour's radius from the target's own centre; it does not put
the hit on the neighbour's surface.

### E3-3 (condition): the struck-bubble query is not bounded

`radius_at` scans every bubble whose `x` lies within `widest + 0.35` of the hit, where `widest` is the largest radius
in the snapshot. Structures are bubbles, so one large dome widens the band for every query, and a formation spread in
`y` (a wall at a choke, a column on a ramp) puts most of the army in the band: O(events × bubbles) per frame, which
charter §3.3 forbids. Use a uniform grid over the map (cell about two unit radii, structures entered in every cell
they cover, or kept in a separate short list), refilled into retained buffers. Add the new path to the valgrind effect
stream (hitscan and melee events, a session, `StruckBubbles`), which today sends only splashes and so does not
measure it.

### E3-4 (minor): the per-end reach has no direct test

Every `shot_clip` unit test uses `ShotReach::uniform`. Add one where unseen ground lies between the path reach and the
muzzle reach beside the landing half (the shot draws there) and beside the muzzle (the shot is cut behind it). That is
the improvement E2-1 asked for.

### E3-5 (minor): `Behind` with no bubble found

When no bubble is found the anchor stays at the target's centre but `UnitRadius` binds 0.5, so `Behind(UnitRadius)`
moves the blade's centre 0.5 *past* the target and the crescent's middle lands inside the bubble. Only synthetic or
bubble-less events are affected; bind 0 for `Behind` in that case, or move the anchor by the default radius too.

### P3-1: the claw marks lie on the attacker

`claw_rake`'s arcs at 1.3, 1.5 and 1.7 struck radii lie, for a melee attacker touching its target, across the
attacker's own bubble (`claw_all.png`, frames 3 to 5: the three arcs cover the welder's right half). The blade at 1.4
overlaps the attacker's rim less. Bringing the rakes to about 1.05 to 1.3 radii would put them on the struck glass. The
arcs read as claw marks, which is the look's meaning, not decoration.

### P3-2: `glass_shatter`

The lab shows three to five flakes at the pictured ages; the bubble's pale ball dominates. If 12 to 18 shards are
thrown, most are too small or too faint to be seen at the default zoom.

### P3-3: `acid_hit`

Still a green disc with a fountain glyph at 1:1 (`lab4.png`). A splash broken into droplets at its edge would read
better.

## Notes on what is now good

- The anchor fix is small, data-independent and presentation-only: the simulation already publishes the angle from
  target to attacker, and the radius alone is taken from the matched bubble, so a mismatch errs by tenths of a unit.
- In the battle the slug ends where the hit shows, and the beams' burns sit on the glass.
- The blade is now a blow across the target, and the needles are two spines.
- E2-4 and E2-5 are done to the bar asked; the far fog take is a real positive control.

## Conditions

| ID | Condition |
|---|---|
| C1 (A3-1) | Instant hits on air units on the flyer's glass (or registered as a bug with the simulation fix); an air target in the staged battle |
| C2 (A3-2) | Attacks from screen-up and screen-down in the staged battle; far-side hits visible |
| C3 (E3-1) | Battle masks on the drawn bubbles (all of them), impact window from the impact's birth, a planted negative per tracer, beam and melee family |
| C4 (E3-2) | `DESIGN.md`: register the by-position match and P2-2 (and A3-1 if not fixed); row refreshed to Amendment 26 |
| C5 (E3-3) | A bounded spatial query for the struck bubble, and the path in the valgrind effect stream |

C1 to C3 change pictures or tests and need evidence; I suggest a short round 4 limited to them. E3-4, E3-5 and P3-1
to P3-3 may follow in that round or be recorded.
