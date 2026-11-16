# Independent Review of WP11 (the Weapons' Own Looks), Round 4

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP11 row of
§9, §7.2 (checklist), Amendments 24 to 27. Packet: [`independent_review_packet.md`](independent_review_packet.md),
section "WP11 round 4" and the shared sections. Earlier rounds:
[`independent_review_wp11_1.md`](independent_review_wp11_1.md), [`independent_review_wp11_2.md`](independent_review_wp11_2.md),
[`independent_review_wp11_3.md`](independent_review_wp11_3.md). Branch `effects-wp11`, head `f7ee0a2b`. This is round 4
of at most five (Q0080). I did not build this work and I did not write rounds 1 to 3.

## Method

I read round 3 in full, the packet's round-4 section, Amendment 27 and the corrected Amendment 26 text, and the diff
`d1232416..f7ee0a2b` of `crates/`, `assets/` and `DESIGN.md`. I read in full the new `effects/struck.rs` (grid,
`struck_at`, `onto_glass`, `onto_outline`, `at_struck_surface`, unit tests), the changed `spawn_effects_from_events`
in `systems.rs` (camera view, rebuild condition, the no-session path), `placement.rs` (`Across` following `Facing`),
the changed `effects.ron` entries (`bullet_hit`, `weld_sparks`, `psi_slash`, `claw_rake`, `impaler_strike`,
`spine_lift_struck`), the measure and planted negatives of `weapon_battle_gpu_tests.rs` (`seen_against_base`,
`judge`, `weapon_looks_fire_in_a_staged_battle`), and the valgrind stream of `sync_allocation_count_tests.rs`. To
judge the anchor I checked where the client draws a bubble and a dome (`scene_controller.rs`: a sphere at
`world_pos[2] + radius`, a dome at `world_pos[2]`, its base), the effect pipeline's depth state (`effects/render.rs`:
every non-decal layer is depth-tested against the scene, `GreaterEqual` in reverse Z, no depth write), what the
simulation publishes as the hit's height (`combat_resolution.rs`: `terrain_altitude_at(target) + 0.5` for every
target), the `WeaponSlot` meaning (`sim_types::presentation`) and how the shipped catalog uses the slots (30
`Primary` weapons, most of them `primary_weapon: Either`, e.g. `pulse_bot` with `target_flags: (bits: 255)`), and the
impaler's shipped carrier (`structures.ron`: a `MeleeSlash` weapon with `max_range: 7.0`). I ran no cargo and no GPU
command, as instructed.

All pictures are lavapipe. I cut each subject from the 2x rows of the twenty `wp11r4_battle_*` sheets, and enlarged
the welder-from-above and impaler frames a further 2x without smoothing. Crops are in `~/scratch/wp11-review4/`
(`weld_above_2x.png`, `set1.png`: welder from below, rifle from above and below; `set2.png`: rifle at a flyer,
impaler, claws; `imp_zoom.png`).

## Verdict table

| Item | Verdict | Main reason |
|---|---|---|
| C1 (A3-1) hits on flyers | **met for the staged case; partly overall (A4-2)** | The index keeps the drawn centre height and the hit is set on it: in `AtFlyer` the rifle's slug and `bullet_hit` end on the flyer's left rim at its altitude (282 px beside it). `Air`/`Ground` slots choose the right plane. But a `Primary` weapon that hits both planes (most of the catalog's, the rifle's included) falls back to "nearest in plan, ties by height from the published point", and the published height is always ground + 0.5, so the tie-break always favours the ground unit: a shot at a flyer hovering over a packed ground blob lands on a ground unit's glass about 50 px below the flyer. B-125 describes the mismatch as "a few hundredths of a unit"; for this case it is 2.5 units |
| C2 (A3-2) far-side attacks | **met for unit bubbles; regression on domes (A4-1)** | `onto_outline` puts the point on the silhouette as the camera sees it. In the pictures the rifle from above hits the target's top rim in front of the glass, from below its bottom rim; the welder's sparks show at the seam both ways. But a dome's drawn centre is its base, so for every attack with a component towards the camera the point lands below the floor (A4-1) |
| C3 (E3-1) a sound battle measure | **met, with a thin margin (E4-1)** | Drawn-centre masks on every presented bubble, the impact from its birth, off-line counting for travelling shots, and four planted negatives that fail (tracer 0, beam 10, blow across 4, blow from above 7; the blow from below, blanked by the same plant, 33). Each subject's own positive is 4 to 40 times its blank. The single bar of 40 is 4 px above the weakest positive (welder from above, 44; `needle_hit` also 44) and 7 px above the largest blank |
| C4 (E3-2) register | **met, one entry inexact (A4-2, E4-2)** | B-125 and B-126 registered, the row names Amendment 27, Amendment 26's crowd case corrected. B-125 understates the air/ground case; Amendment 27 and the test's comment attribute the 33-px blank to the blow from above, the evidence to the blow from below |
| C5 (E3-3) bounded query | **met** | Uniform grid, counting sort into retained `Vec`s, each bubble entered in every cell its reach covers (a dome included), a query reads one cell; cells capped at 512 a side by widening them. No O(N²); valgrind's effect stream now sends a hit and a blow each frame through the index and counts 0 blocks. See E4-3 |
| E3-4 per-end reach test | **met** | `a_shots_muzzle_reach_counts_only_near_its_muzzle` covers both ends and the single-reach contrast |
| E3-5 unfound bubble | **met** | With a camera, `Behind(UnitRadius × 1.15)` lands within about 0.05 units of the default bubble's centre; without one (45° glass) about 0.14 past it, harmless |
| P3-1 claws on the attacker | **met** | The three dark-red arcs now wrap the target's near rim and the seam, not the welder's body (`set2.png`, row 3) |
| Impaler, two spikes | **acceptable as recorded; P4-1** | Two small chitin spikes either side of the seam, readable at 2x, small at 1:1. The change is driven by the staged battle lending the look to a touching welder, while its real carrier strikes from 7 units |
| GPU tests not re-run (shots, Fungal, status) | **shots test's path is touched (E4-4)** | Without a session, events now pass through `at_struck_surface` (moved by the default radius, onto the camera's outline). `shipped_shots_draw_along_their_paths` sends instant shots through that path; the package gate must run it. Fungal (`ProjectileVisual`) and status deaths (`BubbleShatter`) are not touched |
| **Overall** | **ACCEPT WITH CONDITIONS** | C1 to C5 are met for unit bubbles, and the far-side and flyer pictures read well. Two items block merging: hits on structures from the camera's side are put under the floor (a round-4 regression), and the register misstates the air/ground mismatch. Both are small |

## Findings

### A4-1 (condition): hits on a dome from the camera's side are put below the floor

`StruckBubbles::rebuild` stores a dome's height as its base (`world_pos[2]`), which is where the dome is drawn.
`onto_outline` then offsets the point by the silhouette direction of a full sphere: for an attack with any
component towards the camera the vertical offset is negative, down to `-(R + 0.12) × cos 58° ≈ -0.53 (R + 0.12)`
for an attack from straight below on screen. For a dome of radius 2 that is about 1.1 units under the ground at the
dome's base. Effect layers are depth-tested against the scene (`render.rs`, `GreaterEqual`), so the hit, a blow's
point and the last stretch of the slug are hidden by the floor; attacks across the screen land at the base, half
sunk. In round 3 the same hit was at ground + 0.5 just outside the dome and visible. Half of all attack directions on
every structure are affected, and nothing tests a dome target (the unit tests use spheres only; the staged battle has
no structure).

Required: on a dome, keep the point on the dome's visible outline: for directions towards the camera the base rim
(height at least the base plus a small lift, horizontal reach `R + SURFACE_MARGIN`), for directions away from it the
upper silhouette as now. Add a unit test with a dome (`BuildingDome`) struck from the camera's side, from across and
from the far side, asserting the point's height is at or above the base and its horizontal distance is at least the
radius.

### A4-2 (condition): the air/ground mismatch for weapons that hit both planes is misregistered

`struck_plane(Primary)` is `None`, so the choice is the nearest bubble in plan with a penalty of `0.05 × |height − z|`.
The simulation publishes `z = terrain + 0.5` for every target, so the penalty always favours a ground unit by about
0.15 units: a `Primary` shot at a flyer whose plan position lies within roughly its lag plus 0.15 of a ground unit's
centre is drawn on that ground unit, 2.5 units (about 50 px) below the flyer, the A3-1 failure. Flyers over a packed
ground army are an ordinary battle sight, and most anti-air in the catalog is a `Primary` weapon that hits both
planes. The client cannot settle this for such weapons without the target's id or altitude in the event, which is a
simulation change, so registering it is acceptable. B-125 says a mismatch puts the hit "a few hundredths of a unit
off its glass", which is true only within one plane.

Required: amend B-125 (and Amendment 27) to state the cross-plane case and its size, with the same simulation fix
(the target's id, or its drawn altitude in `to[2]`). Recommended, not required: for a `Primary` weapon whose
`target_flags` admit only one plane, pass that plane to `struck_at` (the client has the catalog), and drop the height
tie-break, which carries no information while the published height is constant.

### E4-1 (minor, strongly advised): the impact bar has little headroom

The planted negatives show the measure is discriminating per subject: every planted blank is far below its own
positive (tracer 0 vs 59, beam 10 vs 88, blow across 4 vs 175, blow from above 7 vs 44, blow from below 33 vs 135).
But one bar of 40 must sit above every blank and below every positive, and the evidence leaves 4 px above the weakest
positives (the welder from above and `needle_hit`, both 44) and 7 px above the largest blank. On another driver (radv,
the default for GPU tests on this server when it has a render node) small shifts may fail either way. The weld from
above is low not because it is hard to see (it is a bright spark at the seam, `weld_above_2x.png`) but because it is
drawn over the attacker's lower glass, which the attacker's 0.8 mask removes from the count.

The sounder measure is a per-subject difference: the planted run already renders the same wave with the impact
blank, so count the pixels beside the struck bubble where the real run differs from the blank run (the units'
animations, the attacker's rim included, cancel), and require that against a bar well clear of zero. If the frames are
not deterministic run to run, compare each subject's impact count with its own blank (for example at least twice the
blank plus 20). Either is a test-only change. If the package gate stays green on the current bar, this need not block.

### E4-2 (minor): which blank leaves 33 px

Amendment 27 and the comment on `IMPACT_PIXELS` say the 33-px blank was "measured on the blow struck from above";
`wp11r4_measurements.txt` gives 7 px for the blow from above and 33 px for the blow from below (blanked by the same
plant, as both use `weld_sparks`). The packet says it correctly. Fix the two texts.

### E4-3 (minor): the grid

Sound: the grid spans the bubbles' reaches, a bubble is entered in every cell of its reach's bounding square, so the
cell holding a query point lists every bubble whose reach can contain it; the distance filter does the rest. Rebuild
is O(cells + entries) once per frame with events, at most 512² cells (a 1 MB `u32` table) and a few dozen entries per
dome, no allocation once the buffers have grown. Two small points: (a) when `span / cell` rounds up past 512 the size
is clamped, so a sliver at the grid's high edge returns `None` (harmless: only points at the edge of a reach); (b) the
valgrind stream asserts that one `hit` spawns per frame but not that the struck bubble was found, so a broken lookup
falling back to the default bubble would still pass. An assertion that the returned radius equals the presented
bubble's (or a unit test through `rebuild` from `BubbleRenderInstance`, which today only `rebuild_from` covers) would
close it.

### E4-4 (note for the package gate): the shots test's path changed

Before round 4 an event without a session was passed unchanged; now it is moved onto a default bubble's outline (E3-5).
The lab and `effect_slice_gpu_tests::shipped_shots_draw_along_their_paths` (both beside-the-fog takes and the
positive control) send instant shots through it, and that test was not re-run this round. The visibility test that
was re-run passes. The package gate must run the shots test; Fungal and the status deaths do not touch the changed
code.

### P4-1 (minor): the impaler's two spikes are shaped by the staging

`impaler_strike`'s shipped carrier is the spine crawler analogue, which strikes from up to 7 units, so a single
spike before the target would never be hidden by it; the pair exists because the staged battle lends the look to a
welder touching its target. Amendment 27 records it as a deviation, which is acceptable. A better course for the
polish pass is to lend the impaler to a ranged shooter in the staged battle and return to one spike, as StarCraft II's
Spine Crawler shows. The spikes also float about 0.3 units above the ground when struck from above and partly sink
from below (recorded in Amendment 27); with one spike anchored at ground height under the outline point, neither would
happen. The spikes are not decoration (each is a thrust), so the owner's art rule holds.

### P4-2 (note): the welder's sparks from above sit over the attacker

From above, the struck outline point lies where the attacker's bubble touches the target, so the spark is drawn over
the attacker's lower glass. It reads as a weld at the seam, which is right for a blow; no change asked.

### Note: perspective

`onto_outline` uses the camera's forward direction for every bubble. With a perspective camera a bubble near the top
or bottom of the screen is seen along a slightly different line, so its silhouette point moves by a fraction of the
0.12 margin. I saw no hit inside the glass in the sheets; no change asked.

## Notes on what is now good

- The far-side problem of round 3 is solved for units without special cases: the silhouette point is derived from
  the camera, and the pictures show hits on the top and bottom rims in front of the glass.
- A shot at a flyer now ends on its glass.
- The battle measure now has planted negatives for each family, and the off-line counting closed the beam's false
  positive (118 px blank before, 10 now).
- The grid is a textbook counting-sort bucket grid with a performance rationale, and its path is in the valgrind
  stream.

## Conditions

| ID | Condition |
|---|---|
| C1 (A4-1) | Hits on a dome from the camera's side kept on the dome's visible outline at or above its base; a unit test with a dome struck from below, across and above on screen |
| C2 (A4-2) | B-125 and Amendment 27 state the cross-plane mismatch for weapons hitting both planes (size about 2.5 units) with its simulation fix |

Both are small and checkable from the diff and the unit tests; I do not think they need a fifth full review round
if the orchestrator verifies them and the package gate (with the shots test, E4-4) is green. E4-1 is strongly
advised in the same change; E4-2, E4-3 and P4-1 may follow or be recorded.
