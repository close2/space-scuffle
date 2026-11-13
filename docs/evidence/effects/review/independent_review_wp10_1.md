# Independent Review of WP10 (Area and Link Looks), Round 1

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP10 row of
§9, §7.2 (checklist), Amendment 20. Packet: [`independent_review_packet.md`](independent_review_packet.md), section
"WP10 round 1". The bar: [`independent_review_wp9_3.md`](independent_review_wp9_3.md). This is round 1 of at most
five (Q0080). I did not build this work and did not read the implementer's self-checklist before forming these
verdicts.

## Method

I read the packet section, Amendment 20, the WP10 entries of `assets/catalog/effects.ron` (looks and ramps),
`crates/client/src/effects/areas.rs` in full, the measuring and fog parts of
`crates/client/tests/area_link_gpu_tests.rs` (`edge_errors`, the tether run, the fog counts, `find_fog_edge`), and
the WP10 diff of `sync_allocation_count_tests.rs`. I built and ran nothing. Pictures were cropped and zoomed with
ImageMagick into `~/scratch/wp10-review1/`, and pixel values were read from the committed JPEGs (so ±3 levels). I
describe radv unless I say otherwise; lavapipe matches it (the area sheet and the printed figures agree to within one
ray of 0.8 px, and the fog counts 14/17 against 16/22).

## Verdict table

| Look / item | Verdict | Main reason |
|---|---|---|
| `psionic_storm` | **reject** | Renders as a pale lilac, translucent glass disc that burns out to white (252, 253, 255) at its centre on the light floor; the floor's tile seams run crisply through it; no indigo body and no dark edge band are visible. It reads as a big bubble, not a storm (A1-1, A1-4) |
| `blinding_cloud` | **reject** | A flat, faintly grey translucent disc with one dark smudge in its far-left quarter that spills past the true edge; reads as a stain or frosted lens, not a low pall (A1-2, A1-4) |
| `abduct_tether` | **reject** | A straight salmon-pink rod of constant width with a lighter centre line: at 1:1 it reads as a laser beam; the grip is invisible (A1-3) |
| Root cause of the colour misses | finding | The ramps are authored as if their values were display (sRGB) colours, but they are linear: the tongue's top key (0.46, 0.22, 0.19) shows as (179, 130, 121), exactly its sRGB encoding; the storm cloud's top (0.48, 0.40, 0.95) as pale lilac (A1-4) |
| Fog: own area whole, enemy area wholly in fog hidden, tether only when both bubbles are `Shown`, no light | **accept** | Correct in code and in the fog shot; the catalog test refusing a `Light` layer is the right guard |
| Fog: the fail-closed cut | **accept with E1-1, E1-2** | Not provably fail-closed (half a sample step of margin) and stale clips survive a release; both cheap to fix |
| Straight-line cut instead of the fog texture | **accept for now** | Loses 61 to 70 % of the seen part in the test, but the case is rare in play (your own units see well beyond a 1.5 to 2 unit radius); recorded as P1-1 with a condition |
| Edge ±2 px test | **accept, strengthen (E1-5)** | Meaningful for position; says nothing about whether the edge can be seen, and the cloud's worst ray (−1.94 px) sits on the tolerance |
| Tether ends ±2 px test | **accept** | Discriminating: a centre-to-centre ribbon would fail by a bubble radius (about 10 px) |
| Steady-state allocation (valgrind) | **accept with E1-4** | Code is allocation-free by reading, but the window skips the enemy-area (`Part`) path and no count is recorded in the evidence |
| Slot lookup | **E1-3** | Linear `position` per published view is an O(N²) search in a sync system (charter §3.3) |
| Fungal Growth: no area look | **accept for WP10; F1 for a later round** | No `AreaView` exists for it, so no held area look is correct. "No look at all" is not an acceptable end state; a presentation-only route exists (F1) |
| **Overall** | **REJECT** | All three looks miss their brief; the engineering is sound in structure and needs only small fixes |

## 1. `psionic_storm`: reject

Evidence: `wp10r1_storm_sequence_*` (row 2 at 2x), `wp10r1_areas_*` (left: whole look; middle: `Critical` only),
`wp10r1_battle_sequence_*` frames 3 and 4.

1. **Reads at gameplay zoom: fail.** At 1:1 the storm is a lavender disc of about 86 px with a white blotch, next to
   bubbles that are themselves glass discs with light rims. Its read is "a large frosted bubble", not a danger zone.
   The true edge is a thin pale violet line, not the "dark violet band" the brief promises (the `storm_rim` ramp
   peaks at alpha 0.60 at its dark key and fades to 0.20 at the lip, so the lip is the weakest part of the band).
2. **Layered: pass in structure** (rim, floor glow, six standing billows, flares), but on screen the floor glow and
   the cloud merge into one flat tint.
3. **Timing: not judgeable.** The storm is absent at t = 0.72 and whole at 0.96; at 0.24 s sampling a 0.12 s
   come-in and a 0.3 s exit cannot be seen, and the exit is not in the sequence at all (V1-1).
4. **Cheapness: fail.** The floor's straight tile seams show undiminished through the "cloud" in every frame
   (left frame of the area sheet), so it reads as a tinted sheet of glass. The flares do not draw strokes or rays;
   I found no line, ray or sunburst element. Good.
5. **Value and colour: fail.** Additive `storm_floor` (up to 0.34, 0.20, 0.62) plus additive flares on a floor of
   about (200, 203, 202) clip to display white at the centre. The body never gets darker than the floor, so there
   is no indigo anywhere: every pixel I sampled inside it is lighter than the bare floor. Two overlapping storms
   will be a white disc.
6. **Crowd: not shown.** No unit stands in the storm in any picture (V1-2).
7. **Fog: pass** (section 4).

## 2. `blinding_cloud`: reject

Evidence: `wp10r1_cloud_sequence_*`, `wp10r1_areas_*`, battle frame 3 at 3x.

1. **Reads: fail.** The disc is a faint grey film: centre (126, 122, 136) against bare floor (170, 176, 176), and
   the near half of the disc (262, 180 in the sheet) at (162, 163, 167), i.e. almost untouched floor. The only
   "murk" is one dark mass in the far-left quarter of the disc, which in every frame of the sequence stays in the
   same place and spills over the true edge at the top left (and past the map's edge). A player cannot tell from
   the murk where the cloud is; only the thin outline tells. It reads as a grey stain or a frosted lens.
2. **Layered: pass in structure** (front ring, shade decal, ten billows, emitted billows).
3. **Timing:** as for the storm (V1-1).
4. **Cheapness: fail.** Tile seams crisp through the shade; a hard, glassy circular outline; murk bunched on one
   side. The bunching is consistent with lifted cards projecting up-screen (`lift: 0.35` to `0.4` at this camera
   pitch) plus random placement of only ten cards, so the visible mass sits on the far half and beyond the edge.
5. **Value: fail.** Darkening of about 25 % at the centre and under 10 % near the edge is far short of a blinding
   pall. Cold cast: present but too weak to register.
6. **Crowd: not shown** (V1-2). Whatever the fix, units inside must stay readable; StarCraft II keeps them visible.
7. **Fog: pass.**

## 3. `abduct_tether`: reject

Evidence: `wp10r1_tether_radv.jpg` (3x: whole, `Critical`, bare), `wp10r1_tether_sequence_*` cells 7 to 9.

1. **Reads: fail.** At the default zoom (sequence cells 7 to 9) it is a thin, straight salmon line between two
   bubbles: the plan's own forbidden "beam". At 3x it is a rod of constant width with straight parallel edges and a
   lighter centre (centre (182, 129, 125), edges (137, 99, 98) and darker), which is the textbook laser look the
   two earlier drafts were replaced for. No ripple is visible in any cell. The grip at the victim is a faint pink
   haze barely larger than the tongue.
2. **Layered:** two layers; acceptable for a `Minor` link if each reads.
3. **Timing: pass.** It appears with the cast, follows both bubbles as the victim is dragged in (cells 7 to 9), and
   is gone when they meet (cell 10).
4. **Cheapness: fail** (uniform width, perfectly straight, translucent at its edges).
5. **Colour: fail.** "Dark red-brown, opaque, no bright core" is authored, but the linear ramp shows as light
   dusty pink, and `shape: Radial` puts the ramp's lightest key on the centre line: a bright core.
6. **Crowd:** not shown; low risk for a line this thin.
7. **Fog: pass.** The enemy's tether with both ends in fog is not drawn (fog shot, held `(3, 1)` with one own
   tether).

## 4. Fog and engineering

**Rules (accept).** `keep_views` draws an own area whole, samples an enemy's on a 17 × 17 grid, holds nothing when
no sample is seen, and cuts with a half plane otherwise; `keep_link` requires `Presence::Shown` at both ends. No
`Light` layer remains, and a catalog test refuses one. The fog shot changes 0 of 277 unseen ground samples with the
`Critical` layers and 0 with the whole look, on both drivers.

**E1-1: the cut is not provably fail-closed.** `area_sight` places the cut half a sample step beyond the farthest
unseen *sample*. The cell boundary between an unseen sample and the next, seen, sample can lie anywhere in that
step, so up to half a step of unseen ground (0.09 units for the storm, about 2.7 px at the default zoom; more on
the diagonal axes, and proportionally more for larger radii: the step is `R/8`) can be drawn. None of the test's 277 unseen
ground samples happened to fall in that margin, so the test cannot see it. Fog cells are 1-unit squares,
so the exact answer is cheap: project the four corners of every unseen cell that intersects the disc (a few cells
for these radii) and put the cut at the farthest corner. That is exact, fail-closed by construction, and keeps at
least as much seen ground as the sample method. Add a unit test with a fog edge placed between two samples, and
assert no unseen corner lies on the drawn side.

**E1-2: stale clips survive a release.** When an enemy area goes from seen to `Hidden`, `keep_area` returns early,
the slot is not kept, and `release` lets the look erode for its `exit` (0.3 to 0.45 s) with the *last* clip, or
with `NO_CLIP` if it was wholly seen: up to 0.45 s of drawing over ground that is now unseen. Flares and billows
laid by `emit` keep their lay-time clip for their whole life (0.8 s and 1.4 s) while the sight may shrink under
them. Fix: when the reason for dropping an enemy area is sight (not unpublication), retire it at once (or shrink its
clip to nothing); and keep each held area's emitted children on its current clip (re-set the clip on the area's
live children each frame, or give children a reference to the parent's clip). Assert both in the fog test: one
frame after the viewer loses sight, the enemy area changes no sample.

**E1-3: O(N²) search.** `keep_area` and `keep_link` call `iter().position` over the held slots for every published
view: quadratic in the number of live areas, which the charter forbids in sync systems even when bounded by
`HELD_SLOTS = 64`. Published areas and links come in a stable order; keep the slots sorted by `id` (and by
`(from, to, source)` for links) and walk both sorted lists together, or binary-search. Cheap.

**E1-4: valgrind evidence.** The window holds an area *of the viewer* with `fog: None`, so `area_sight`, the `Part`
clip and `set_fog_clip` on emitted children are not exercised. Add an enemy area seen in part under a real
`BattleFog`, and write the valgrind block counts (baseline and window) into `wp10r1_measurements.txt` (or its
round-2 successor). By reading, `area_sight` uses a stack array and the slot vectors are retained, so I expect 0.

**E1-5: the edge test proves position, not visibility.** `CHANGE_LEVEL = 8` lets an invisible edge pass, and the
look's visible body can contradict the edge (the cloud's murk spills past it). Add: (a) the median luminance change
in a 3 px band just inside the projected radius is at least 32 levels against the bare floor, on the shipped floor;
(b) the whole look changes no pixel more than 4 px outside the projected radius at ground level (cards standing
inside may rise over it; measure along the near half's rays, where nothing rises). The cloud's worst ray of
−1.94 px is on the tolerance; after the art changes keep the margin, not the bare pass.

**Tether ends test (accept).** It walks the line between the two centres and compares the first and last changed
pixels with the projected surface points; 108 of 127 pixels change, the rest are the two bubble interiors. A
ribbon drawn from centre to centre would fail by about 10 px. Keep it.

**Straight-line cut versus binding the fog texture (accept for now, P1-1).** In the test the whole look covers 22
of 57 seen samples (radv) and 17 of 57 (lavapipe); the visible remainder is a thin violet crescent that reads as a
blade, not a storm. That would be a defect if it were common, but an enemy storm or cloud is cast on the viewer's
units, which see far beyond a 1.5 to 2 unit radius; partly seen enemy areas arise mostly at cliffs and at sight
edges. I accept the cut for WP10 on the condition that E1-1 makes it exact, and that the per-pixel mask (the
terrain's visibility texture bound to the decal and card pipelines) is scheduled before any large-radius area
ships (the `R/8` step and the crescent loss grow with radius).

## 5. Fungal Growth

"No area look" is the correct reading of the simulation: it publishes no `AreaView`, so WP10 has nothing to hold.
But as an end state it is not acceptable. The only thing a player sees today is `concussive_daze`, which is also
Concussive Shells' slow, so a 2.25-radius, 30-damage area spell is indistinguishable from a single marauder hit, and
its footprint (who was caught) is never shown. A presentation-only route needs no simulation change:

**F1 (recommended, a later round or WP10b; not blocking WP10):**

1. **Impact from the missile's end, client-side.** `RenderSnapshot::spell_missiles` already publishes each spell
   missile with a stable `id`, `position` and `source`. When a missile whose source's presentation block names an
   `impact` look leaves the published list, lay that look at its last published position (Fungal flies to its
   target point and flies on if its caster dies, so it only disappears at impact; the last position is at most one
   tick, 0.67 units, short, which a `Critical` impact ring centred there absorbs, or extrapolate one tick along its
   last motion). Bind `AreaRadius` from the ability's search radius read from the catalog, not a constant. Apply
   the same fog rule as a fresh area (draw only if the impact point is seen, with the E1-1 cut).
2. **The look.** A one-shot `Major` splash: a sickly green-ochre spore burst rising and settling (`smoke_puff`
   cards), a ground mark of fungal growth (`Decal` with a lobed outline, darkening plus a green-ochre tint) that
   fades over the status duration (2.85 s), and a `Critical` ring at the true radius for its first 0.3 to 0.5 s.
3. **The status.** Give the victims' presentation blocks a `DamageOverTime` status look (WP9's per-kind status
   table): spore growth clinging to the lower half of the bubble with a slow pulse. A unit under Fungal carries both
   `Slowed` and `DamageOverTime`, so it will read differently from a concussive slow without any new kind. If a
   per-ability status look is wanted later, that is a `StatusView` source field and belongs to a simulation plan.
4. **The missile.** Present `spell_missiles` with a projectile look named in the caster's block, so the impact has
   a cause on screen.

## Required changes

| ID | Change |
|---|---|
| A1-1 | `psionic_storm`: a dark, saturated indigo body that is darker than a light floor and hides most of it (alpha-blended cloud cards, display colour around sRGB (45, 25, 95) in their folds, coverage at least 0.6 across the disc), lit from within by flares that stay below display white even where two storms overlap (smaller, fewer at once, peak about sRGB (200, 180, 255)). Replace the additive floor glow with a darkening violet tint, so the floor reads darker inside, never whiter. A clearly visible edge band whose outer lip is its strongest part (E1-5 (a)). It must not read as a glass bubble: no thin bright circular rim alone |
| A1-2 | `blinding_cloud`: a dense, low murk that covers the whole disc evenly (stratified or ring placement, at least 14 cards, lift and placement chosen so the projected mass sits over the disc and not beyond its far edge), darkening the ground under it to about half its brightness out to a darker front at the edge. Units inside stay readable (V1-2) |
| A1-3 | `abduct_tether`: an opaque dark red-brown tongue at display colour sRGB about (95, 30, 28) with darker edges about (45, 12, 12); any wet highlight thin and off the centre line, never a light core; its width varies by about ±25 % in bulges that travel back towards the caster (the ripple must be visible at 1:1 in the sequence); a clear grip at the victim at least 1.8 times the tongue's width, and a swelling at the caster's end. No parallel straight silhouette |
| A1-4 | Ramp colours: author every WP10 ramp as linear values of the intended display colour (convert from sRGB), and check every look's sampled pixels against its brief in the measurements (centre, mid, edge of each area; centre and edge of the tongue) |
| E1-1 | Make the cut exact and fail-closed from the unseen cells' corners, with the unit test described in section 4 |
| E1-2 | Drop an enemy area at once when sight is lost; keep emitted children on the parent's current clip; assert it in the fog test |
| E1-3 | Replace the per-view linear slot search with a sorted merge or binary search |
| E1-4 | Exercise the enemy `Part` path in the valgrind window; record the block counts in the measurements |
| E1-5 | Add the edge-contrast and outside-spill assertions of section 4 |
| V1-1 | Every 40 ms frame from the area's publication to 0.4 s, and from 0.2 s before its end to 0.5 s after, for both areas; the tether every frame from the cast to the victim's arrival |
| V1-2 | Each area over units of both sides (at least four in each), and two overlapping storms, at 1:1 and 2x |
| V1-3 | The fog shot with the viewer's unseen cells shown (an evidence-only darkening overlay of the cells, or the terrain's own fog), so a reader can see where the sight edge lies; and one partly seen area across a cliff edge |

## Non-blocking polish and advisories

- **P1-1** The per-pixel fog mask (section 4), before any area larger than radius 2 ships.
- **P1-2** A gentle sag on the tongue (the plan's catenary, §5.5); with A1-3's bulges it can wait, since Abduct's
  tether lives under a second.
- **P1-3** Stage the evidence away from the map's corner: half of every battle frame is starfield, and the cloud
  sits on the floor's edge.
- **P1-4** The storm and the cloud are distinct from each other by hue; after A1-1 and A1-2 check them again
  against `barrier_shell` and the glass bubbles, which share the "translucent disc with a rim" silhouette today.
- **F1** Fungal Growth's impact, status and missile looks (section 5).
