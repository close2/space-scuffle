# Effect texture review: vertical slice (WP6a)

Plan: [`2026-11-09-effect-visualization`](../../../plans/2026-11-09-effect-visualization.md), §6.3 to
§6.5 and §7.2. Reviewed 2026-11-10 by the implementing agent, who looked at every contact sheet and
frame strip below and at a frame sheet of every source clip (Q0026 practice for Veo clips).
Recipes: [`../recipes/`](../recipes/). Raw sources are kept off the repository under
`~/scratch/effects/sources/`; every shipped element cites its ledger line in its set's
`assets/effects/<set>.provenance.ron`.

Checklist applied to every element: pure black (the baker's black check: border bands and the darkest
5 % at most 3/255), no quad edge or cut-off at a cell border, no lit floor, horizon or rays, no colour
fringes from keying, a shape that reads in greyscale, erosion that breaks up from the thin parts
first, a flipbook with onset, peak and dissipation, and no "AI-looking" clutter or regularity.

## Contact sheets

| Set | Sheet | Frame strips | Legend |
|---|---|---|---|
| `fx_noise` | [fx_noise_texture_sheet.png](fx_noise_texture_sheet.png) | (none: one cell per element) | [txt](fx_noise_texture_sheet.txt) |
| `fx_blast` | [fx_blast_texture_sheet.png](fx_blast_texture_sheet.png) | [fx_blast_frames.png](fx_blast_frames.png) | [txt](fx_blast_texture_sheet.txt) |
| `fx_acid` | [fx_acid_texture_sheet.png](fx_acid_texture_sheet.png) | [fx_acid_frames.png](fx_acid_frames.png) | [txt](fx_acid_texture_sheet.txt) |
| `fx_ground` | [fx_ground_texture_sheet.png](fx_ground_texture_sheet.png) | [fx_ground_frames.png](fx_ground_frames.png) | [txt](fx_ground_texture_sheet.txt) |

## Shipped elements

| Set / element | Layers | Source | Verdict | Reasons |
|---|---|---|---|---|
| `fx_noise/noise_tile` | 1 | authored (`effect-noise`) | Accept | R Perlin and G Worley tile without a seam in the 2 x 2 view; B/A curl components carry the expected directional streaks of a derivative field. |
| `fx_noise/ring_noise` | 1 | authored | Accept | Clean soft ring in R; G/B break it up. Not tileable by design. |
| `fx_noise/fire_noise` | 1 | authored | Accept | Fine Perlin and value noise tile; inverted Worley gives cell breakup. |
| `fx_noise/erosion_gradient` | 1 | authored | Accept | Three radial gradients from mild to strongly ragged rims; a threshold eats them from the rim inwards. |
| `fx_blast/fireball_flipbook` | 32 | Veo clip `318cc0e2…` (1st of 1), 0.7 s to 2.25 s | Accept, with a note | Black after the cut; peak, cooling and a dissipation into faint embers over the 32 frames; erosion breaks the thin smoke rims first. Cut: the first 0.7 s is a flash whose glow lifts the whole frame above 3/255 (the effect draws a flash as its own light layer), the tail after 2.25 s is smoke reaching the frame edge. Note: the fireball is a fairly regular six-lobed rosette; random rotation and the ramp hide it at gameplay zoom, but it is the first candidate to regenerate in the scale-out. |
| `fx_blast/smoke_puff` | 4 variants | 4 image-model stills on black, luminance key | Accept | Soft, round-on-every-side puffs with wispy edges, no base line; erosion leaves the dense cores last. Keyed by luminance, not by difference matte: see "Difference matte" below. |
| `fx_blast/debris_atlas` | 16 | 1 image-model still, 4 x 4 grid (3rd picture), sheet margin 16 px | Accept | 16 separate rock and dirt chunks, each centred in its own cell, none cut by a cell border; readable lit shading. |
| `fx_blast/scorch` | 2 variants | 2 image-model stills, top-down | Accept | One ragged splatter burn and one soft sooty burn; dense centres, broken rims, no floor texture. |
| `fx_acid/acid_crown` | 24 | Veo clip `874eca1f…` (4th of 4), 1.1 s to 2.15 s, letterboxed | Accept, with a note | A glowing splash crown opening to its peak, black around it, no surface or reflection. The flipbook ends at the peak: the clip's later frames spread droplets to the frame edge instead of dissipating, and its first second has the splash column on the frame's lower edge; the effect fades the crown in and erodes it away. Note: the crown fills about half of its cell because the late frames' droplets set the joint extent. |
| `fx_acid/acid_splat` | 3 variants | 3 image-model stills, top-down | Accept | A round splat with radial fingers, a thrown elongated splatter, and a flat paint-like stain; glossy grey reads well through both ramps. |
| `fx_acid/droplet` | 1 | 1 image-model still | Accept | Clean glossy teardrop; head towards +x, tail towards -x (orient along velocity with that axis). |
| `fx_ground/spine_erupt` | 24 | Veo clip `3ce386ff…` (3rd of 3), first 1.6 s, letterboxed | Accept, with a note | Seven crooked horn spikes slide up out of a floor slit, from black to full height; consistent centroid, no scaling. The clip's spikes never retract, so only the eruption is kept; the effect retracts them by playing the flipbook backwards or by erosion. The spikes' base is a hard horizontal cut (the slit), which the sprite's soft depth hides where it meets the ground. |
| `fx_ground/ground_crack` | 2 variants | 2 image-model stills, top-down | Accept | A wide branching crack and a narrow wavy fissure, each tapering to hairlines at both ends, so segments chain along a line with overlap rather than tiling seamlessly. |

## Rejected sources

| Source | Element | Reason |
|---|---|---|
| Image `fx_blast.smoke_puff.…002821Z…` (pair) | smoke_puff | The puff sits on a flat cut base (the side-on view's base line): a visible cut-off. |
| Images `…003033Z…`, `…004650Z…`, `…004732Z…` (on black only) | smoke_puff | Unpaired: the service refused their on-white edits for quota; not needed once the stills were luminance-keyed. |
| Pairs `…003418Z…`, `…003520Z…`+edit, `…004854Z…`+edit | smoke_puff | Difference matte rejected: residual above 4/255 on 1.2 %, 22.8 % and 5.5 % of pixels (limit 0.5 %). Their pictures on black ship luminance-keyed. |
| Image `fx_blast.debris_atlas.…003321Z…` and `…010725Z…` | debris_atlas | Generator grain on the sheet's outer edge crossed 3/255 on a cell border band (1.3 % and 1.1 %, limit 1 %). |
| Image `fx_acid.acid_splat.…004206Z…` | acid_splat | Puffy, inflated 3D blob, not a flat splat. |
| Veo clip `11a3cbab…` | acid_crown | Milk crown on a reflective table (lit floor and reflection); droplets leave the frame; no dissipation. |
| Veo clip `0df8a5c4…` | acid_crown | Reflective surface again; the splash collapses into a puddle that fills the frame width. |
| Veo clip `86e047c4…` | acid_crown | A static ball is lit from the first frame for 1.6 s (no onset); the splash spreads to the frame edges. |
| Veo clip `28a0f7aa…` | spine_erupt | Anatomical finger bones that grow by scaling (a zoom), never retract; literal and "AI-looking". |
| Veo clip `7b9dcd6b…` | spine_erupt | A regular comb of identical cones that grows by scaling and never retracts. |
| Veo clip `6648f2a0…` | acid_bubbles | Green-tinted background (not black), an underwater bubble stream with a water surface line. |
| Veo clip `5cfd4f1d…` | acid_bubbles | A faint lit floor line across the whole frame; one bubble that collapses rather than pops. |

## Not shipped

- `fx_acid/acid_bubbles`: both clips rejected and the WP's cap of 10 Veo clips reached. The slice's acid
  burst can use `droplet` particles and `noise_tile` breakup instead; a third attempt belongs to the
  scale-out (WP14).
- `fx_ground/dust_burst` and `fx_ground/dirt_mound`: out of the slice (WP6a brief).

## Difference matte

Five black/white pairs were requested for `smoke_puff`. The model's on-white edit redraws the smoke
rather than recolouring its background: only two pairs passed the residual rule (one of them the
rejected flat-based puff). Four variants from one keying method needed stills, so `smoke_puff` is
luminance-keyed. Recorded for the scale-out: budget pairs only for elements where the edit is likely
to hold (opaque, hard-edged shapes), and prefer luminance keys for smoke.

## Spend (recounted from `docs/evidence/veo/ledger.jsonl`)

| Item | Count | Cost |
|---|---|---|
| Ledger before WP6a | 241 `request_started` x $0.40 | $96.40 |
| Image model `gemini-nano-banana-2.1` (`global`) | 42 `image_request_started`, 17 refused for quota (free) = 25 paid x $0.11 | $2.75 |
| Veo `veo-3.1-fast-generate-001`, `effect_clip_request_started` | 10 x $0.40 | $4.00 |
| **Ledger after WP6a** | | **$103.15** |
