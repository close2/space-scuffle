# Independent Review Packet: Effect Vertical Slice (Round 5) and Scale-Out (Round 1)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
This packet is for the second reviewer of the vertical slice. It lists the material to judge and the checklist to
judge it by, and nothing else: it holds no verdict of the implementer, on purpose. Please judge each of the three
effects against each of the seven checklist items, write **accept** or **reject** with a concrete reason (which
sheet, which cell or frame, what you see), and put your review in a new file next to this one
(round 5: `independent_review_round5.md`, see "Round 5" below).

## Scale-out round 1 (WP7)

The slice's review gate passed in round 5. The scale-out adds four splash effects and polishes two of the
accepted ones. Please write this review to a new file next to this one, `independent_review_scaleout1.md`, in
the same form as the slice's reviews: each effect below against each of the seven checklist items (at the end of
this packet), **accept** or **reject** with a concrete reason (sheet, cell or frame, what you see), and for the
two polished effects whether the change regressed anything. The images are lavapipe captures from a debug
`battle_capture --effect-lab` under Xvfb, as for the slice; the sheets' layout is the slice's (eight ages across;
rows: bright ground default zoom, the same with the 1-unit grid and the bound extent in red, bright close zoom
and its reference, then the same four rows on dark teal ground). For a line the grid row draws the line and the
full-damage radius at each search point.

### What the new effects are meant to be (StarCraft II analogues; looks stay original)

| Effect | Unit (analogue) | Simulated extent | Meant to read like |
|---|---|---|---|
| `flame_line` | `napalm_wheeler` (Hellion) | a line of 25 searches of radius 0.15 from 0.25 to 6.5 units ahead of the attacker | a short jet of burning napalm racing from the nozzle along the whole line, burning for a moment and leaving a burnt streak |
| `lance_sweep` | `tripod_annihilator` (Colossus) | a line of 11 searches of radius 0.15, 2.5 units long, across the struck point | a thermal lance sweeping across the target, burning a glowing line into the ground that cools |
| `flak_burst` | `sky_bastion` air weapon (Liberator's anti-air rockets) | one zone of radius 1.5 in the air units' plane | an anti-air burst among air units (drawn on the ground in the lab; in battle it is at the air units' altitude) |
| `cleave_impact` | `stag_behemoth` (Ultralisk) | zones 0.5 / 1.0 / 1.5 at the struck unit's near edge | a heavy blow driven into the ground, dust and soil thrown out to the cleave's reach; frequent (every 0.6 s), so it must not stack |

Two new building blocks are used: a **ribbon** (a strip along the line, racing out and sweeping) and a
**distortion** (a heat haze that bends the ground behind it; subtle by design).

### Material

| Effect | Contact sheet | Sequence (40 ms steps) |
|---|---|---|
| `flame_line` | [flame_line_contact.png](flame_line_contact.png) ([legend](flame_line_contact.txt)) | [flame_line_sequence.png](flame_line_sequence.png) |
| `lance_sweep` | [lance_sweep_contact.png](lance_sweep_contact.png) ([legend](lance_sweep_contact.txt)) | [lance_sweep_sequence.png](lance_sweep_sequence.png) |
| `flak_burst` | [flak_burst_contact.png](flak_burst_contact.png) ([legend](flak_burst_contact.txt)) | [flak_burst_sequence.png](flak_burst_sequence.png) |
| `cleave_impact` | [cleave_impact_contact.png](cleave_impact_contact.png) ([legend](cleave_impact_contact.txt)) | [cleave_impact_sequence.png](cleave_impact_sequence.png) |

Measured on the GPU (`effect_slice_gpu_tests.rs`, lavapipe): the Critical marks of each line end within 1.3 px of
the line's projected ends; the fronts of the two circles end within ±2 px of the reach on at least 97 % of 72 rays;
coverage of the full-damage area within 80 ms is 65–99 %; the largest
frame-to-frame change after the impact is at most 1.63/255; nothing differs from the empty ground after the end.
With a line seen by the viewer only from its fifth search point on (fog), nothing is drawn more than 1 unit
before that point.

### Polish of the accepted effects (round 5's remaining improvements)

| Effect | Round-5 finding | What changed |
|---|---|---|
| `acid_burst` | Item 6 of the list: frame 0 shows a concentric inner ring and sparkle dots | The front and its dark edge start at 0.3 (was 0.55) of the full-damage radius, under the crown; the fine spray takes the globs' dark wet ramp. [acid_burst_contact.png](acid_burst_contact.png), [acid_burst_sequence.png](acid_burst_sequence.png) |
| `spine_line` | The scar's blotches repeat at about the point spacing | The scar's pieces are scattered 0.3 (was 0.1) along and across the line. [spine_line_contact.png](spine_line_contact.png), [spine_line_sequence.png](spine_line_sequence.png) |

The `acid_burst` and `spine_line` sheets of this directory are now the polished ones, and `shell_impact`'s is
re-taken too: the random choices of every effect (rotations, placements, variants, noise phases) are now drawn from
the effect's name rather than its position in the vocabulary, so each of the three shows a different, equally
random draw than in round 5 ([shell_impact_contact.png](shell_impact_contact.png),
[shell_impact_sequence.png](shell_impact_sequence.png)). Please check all three for regressions. Round 5's sheets
are in commit `58b8da2`. Not changed this round: `shell_impact`; the acid stain's meniscus, inner rims, glob shapes, crowd variation and
dried crust; the spine scar's torn clods.

## Round 5

Round 4 accepted `shell_impact` and `spine_line` and rejected `acid_burst` narrowly
([`independent_review_round4.md`](independent_review_round4.md), kept as written with rounds 1 to 3). This is
round 5, the last of Q0080's five rounds. Please write the round-5 review to a new file next to this one,
`independent_review_round5.md`, in the same form, and judge all three effects afresh against the checklist (the
two accepted ones too, so that a regression would show). The tables below map each round-4 finding to what
changed; they list changes and measurements, not verdicts. The images of this directory are the round-5 ones
(same path as rounds 3 and 4: lavapipe, debug `battle_capture`, Xvfb); round 4's are in commit `b72b693`. The
sheets' ages are unchanged.

### `acid_burst`

| Round-4 finding | What changed |
|---|---|
| Ranked change 1 / item 1: the stain's visible edge lies at 0.87 of the reach (its box at 0.98; the ragged, eroded edge about 13 % of the radius inside it) | The stain no longer uses its texture's outline as its edge. A decal may now have a **disc outline**: its visible edge is a disc whose edge wavers by a fine noise of ±2 % of the radius (about ±2 px at default zoom), and the texture (relief, pooling, rim) is read from its middle inside it. The stain is sized so that its mean edge lies on the reach, and it holds that edge **without erosion until about 1.8 s**; the deep bites come only with the drying after that. Measured on the round-5 frames along the centre row (bright ground; the outermost pixel more than 16 levels from the clean ground, against the middle of the reach circle's line): default zoom, 0.18, 0.48, 0.9, 1.8 and 3.0 s, 0 px left and right, diameter 1.000 of the reach; close zoom, 0.48–1.8 s, +2 px left, 0 to +1 px right. The GPU footprint test now measures the held stain's visible edge (72 rays, 1.39 s): 100 % of rays within ±2 px of the reach, mean −0.18 px, largest overshoot +1.27 px, on radv and lavapipe (it used to check only that the stain stayed inside). |
| Ranked change 2 / item 4a: a pale, translucent band between stain and lip in frames 2–8, about 0.1 of the radius wide all round | With the stain on the reach, the lip lies over the stain. The lip is also thickened (its coverage doubled) and its inner side is the stain's wet emerald at near-full opacity. Sampled (3×3, sequence frames, centre row from the reach inward, default zoom): frame 2 lip and inner side (41–50, 111–125, 36–50), body (47–57, 115–134, 47–57); frame 4 (40–48, 109–117, 36–48), body (52–57, 127–136, 52–58); frame 8 (41–47, 109–115, 38–47), body (54–58, 125–140, 52–59). Round 4's band sampled (98–109, 131–132, 88–98) at default zoom against a stain of (45, 103, 42). |
| Item 3: an 8 % contraction when the front leaves (frames 7–10) | The stain's edge is on the reach before the front fades, so the front fades onto stain. |
| Ranked change 3 / item 4b: the globs are one teardrop sprite, identical pale-lime commas | A new texture element of **three glob shapes** (round with a blunt trailing bulge, tearing in two along a thin neck, stretched with a ragged tail; image-model stills, [`texture_review.md`](texture_review.md) "Review round 5"), each instance one of them, of more varied size (0.22–0.46), oriented along its velocity, with its own ramp: a dark wet emerald body and lime only on its one small highlight. |
| Ranked change 4 / item 4c: dense near-white fumes | Thinner and fainter: smaller wisps broken harder by noise, in a yellow-green of lower value and about 60 % of the opacity. |
| Ranked change 5 (optional): a small "Y" crown | The crown card is 1.3 of the reach (was 1.0) and torn harder. Its texture fills about half its cell, so it still reads as a central splash, now larger. |

### `spine_line` (accepted in round 4; polish only)

| Round-4 note | What changed |
|---|---|
| Ranked 1 / 4c: the first clump is cut by the line's start into a single tall spike, an "L" or hook (frames 3–20) | A standing clump that a line end would cut is made smaller and moved inward so that it stands whole inside the line (it keeps its far edge); the first clump is now a smaller whole clump. The last clump is treated the same way. |
| Ranked 2 / 4c: on teal the scar is a grey-tan film of the ground's value ((119, 126, 116) on (95, 138, 133)) | The scar's ramp now only darkens: every colour lies below its opacity times the teal ground's darkest channel, so it is a darker shade of whatever ground it lies on. Sampled on the round-5 sheet (dark close, 1.622 s, 3-row averages of a 500 px run along the line): (79, 102, 97) against the clean ground's (92, 137, 133) on the same rows. The clods remain a step lighter than the furrows. |
| Ranked 3: a lone dark chip at the far end at 1.6 s | It was the last clump's tip sinking into the ground at the very end of its life; the clumps now fade over their last 15 % as they sink. |

The fog sheet and sequence are re-taken with the round-5 data ([`spine_line_fog_contact.png`](spine_line_fog_contact.png),
[`spine_line_fog_sequence.png`](spine_line_fog_sequence.png)); the fog clipping is unchanged.

### `shell_impact` (accepted; optional changes)

| Round-4 improvement | What changed |
|---|---|
| 1: let the mark under the ring appear only once the ring has passed (no grey veil inside the ring) | The scoured blast mark now starts at 0.12 s, after the shock ring has reached the reach (about 0.115 s), and ends as before (8.03 s). |
| 1: an irregular streak fringe (new blast-mark texture) | Not changed (scale-out). |
| 2: push the ring's width variation further | Not changed, to avoid regressing an accepted effect in the last round. |

### Crowd capture, round 5

[`stress_capture_400.png`](stress_capture_400.png) is re-taken with the round-5 effects the same way as rounds 3
and 4 (debug `frame_budget`, lavapipe under Xvfb, a scratch copy of `assets/` whose `units.ron` gives
`volatile_tick` and `burrow_scorpion` the lab blocks, tick 150, 200 splashes a second); figures in
[`stress_capture.txt`](stress_capture.txt), round-5 paragraph. Round 4's frame is in commit `b72b693`.

## Round 4

Round 3 accepted `shell_impact` again and rejected `acid_burst` and `spine_line` (narrowly)
([`independent_review_round3.md`](independent_review_round3.md), kept as written with rounds 1 and 2). This is
round 4 of up to five (Q0080). Please write the round-4 review to a new file next to this one,
`independent_review_round4.md`, in the same form, and judge all three effects afresh against the checklist
(`shell_impact` too, so that a regression would show). The tables below map each round-3 finding to what changed;
they list changes, not verdicts. The images of this directory are the round-4 ones; round 3's are in commit
`fd24c31`, round 2's in `d03ed19`, round 1's in `bb5a61c`.

**What else changed in the material.** The contact sheets' last column is now the effect's true end, the moment
the runtime frees it: for the line that is its lifetime plus the travel of its last point, so the spine sheet's
ages are 0, 0.099, 0.263, 0.493, 0.985, 1.642, 2.463 and 3.284 s (round 3 stopped at 2.53 s, before the far
points' marks had faded; see `spine_line` item 3). The splash sheets' ages are unchanged. A new **fog sheet** and
sequence show the line running out of the fog (section "Fog" below).

### `spine_line`

| Round-3 finding | What changed |
|---|---|
| 4a / ranked change 1: dark pen-stroke ruptures and leaning single spikes cross the line; at 1.27 s only "/ \| — \\" strokes remain | The ruptures are now short, wide, **ragged tears of torn earth running along the line** (the `earth_scar` texture turned along the line, 0.84 units long and about as wide as the scar, of their own size, scattered at most 0.1 off the line), soft-edged (erosion softness 0.25) and lower in contrast (their own ramp, opacity 0.55 to 0.66, was 0.85 to 0.92); they open a frame or two before the clump and are gone by 1.0 s, with the spines. The **single leaning spikes are removed**: each clump now carries three to five heavy spikes, so nothing slanted stands on its own at any age. |
| 4b / ranked change 2: clumps small and twiggy, flat silhouettes with almost no shading, about half a unit tall | New clump textures ([`texture_review.md`](texture_review.md), "Review round 4"): heavy conical spikes, about a third as wide at the base as tall, a bright warm-umber left flank and a shadowed right flank (the key light from the upper left), and a **collar of torn soil clods** at the root. The card is larger (0.9 of the spacing, was 0.62) and stands higher, so the spikes rise about 0.7 to 0.9 units over the ground. The newest clump's highlight is gentler (ramp input 1.8×, was 2.4×), so it stays shaded while it is the brightest thing in the line. |
| 4c / ranked change 3: the scar is a thin, pencil-like grey line; the torn earth does not show | The scar's box is three times as wide (a decal's `width` may now exceed 1), so the furrow of torn earth is about 0.3 to 0.5 across and of uneven width; its own ramp keeps the dark furrows and the upturned clods a step darker than the ground, with the faint parts of the texture transparent, and less noise breakup lets the texture's clods show. |
| 3 note / ranked change 3: a faint stretch of fissure still visible at 2.53 s | The sheet now runs to the true end (3.28 s): its last column shows the ground clean. The runtime always kept the effect until then and every mark's intensity reaches zero at its own end. |

### `acid_burst`

| Round-3 finding | What changed |
|---|---|
| 1 / ranked change 2: after the front leaves (about 0.36 s) the stain covers only 75–85 % of the reach, with deep bites; it visibly contracts between frames 8 and 10 | The stain's erosion **holds the full disc until about 1.8 s** (only a noise-broken edge, threshold 0.10 to 0.12), then dries from its edge and from holes. It is at its full size from about 0.12 s, before the front reaches the reach, so nothing contracts when the front hands over. Its size is 0.98 of the reach: a first round-4 take at 1.14 drew the stain up to 11 px (of 87) beyond the reach, which a `Critical` layer must not do (it would show damage where there is none, plan §10.2), so the ragged edge now lies just inside the reach at every age, wet or dried, and the front's dark lip marks the reach itself while it lasts. |
| 4a / 5 / ranked change 1: from 0.9 s a flat, mottled mid-green moss patch; no darker wet rim at any age | The liquid **keeps its relief as it dries** (the crust keeps 75 % of the relief light; only the wet sheen dries away). A **dark wet rim** is drawn by the liquid material along every edge of the liquid, its outline and the holes it dries into, at every age: a band about 0.1 to 0.2 wide, darker and more saturated than the body (colour ×0.24, 0.44, 0.24 at the edge). Sampled on the round-4 sheet (bright ground, close zoom, a ray from the centre to the right edge, sRGB): at 0.9 s the body is about (57, 113, 50) and the band inside the edge about (62, 96, 50); at 1.8 s the body is about (77, 88, 57) and the band about (70, 80, 53). |
| 3 note / ranked change 1: the colour is still emerald at 1.8 s; olive only at 4.5 s | The stain's ramp has a wide olive-brown range below the emerald, and the drying curve walks the body there faster: olive-green by 0.9 s, **olive-brown by about 1.8 s** (sample above; round 3 sampled (77, 140, 64) at 1.8 s). |
| 4b / ranked change 3: the front is a pale, translucent, even glass or soap-bubble rim with a lighter halo; a pale gap between stain and rim | The front's ramp is now a **dark, wet, nearly opaque lip** (opacity 0.80 to 0.96, darkest at its leading edge), with no pale or translucent part; its trailing edge is broken by stronger noise, so its width varies about twofold around the circle. The stain lies under it from the first frame, so no ground shows between them. |
| 4c: the contraction is a soft pop | See the first row. |
| 4d / ranked change 4: frame 0 is a smooth, glowing lime oval of half the reach with white sparkle dots | The radial ground glow is removed. At frame 0 the stain itself is there, thick, at 0.85 of its size (about 0.9 of the reach, ragged), with the splash crown above it and a small hot core for two frames. |
| 2 note / ranked change 4: no airborne part at default zoom; tiny crown; globs not visible; vapour lies on the stain | The crown has its own opaque dark-wet ramp (lime only on its highlights), is larger from frame 0 (1.0 of the reach, growing to 1.18) and thrown higher, and torn harder by noise. **Eleven globs** of 0.28 to 0.44 units fly on visible arcs from across the burst (the particle level of detail had cut them at default zoom, because it read the emitter's small size). The fumes are fainter and pale yellow-grey, start from five places over the stain and rise at 2.2 units per second. |
| 4e / ranked change 5: the relief is regular and blobby, uniform over the disc | The relief's strength varies with low-frequency pooling noise, from flat pools (0.2) to large swells (1.6), and is read three mips above the footprint, so only broad swells catch the light. |
| Crowd: flat, light, pastel-green fields | Follows from the rim, the kept relief and the early olive (see the crowd capture). |

### `shell_impact` (accepted in rounds 2 and 3; optional changes)

| Round-3 optional change | What changed |
|---|---|
| 1: vary the shock ring's width by about two around its circumference; drop the haze inside it | The front's trailing-edge noise is stretched in contrast, so the band's width varies about twofold around the circle (its outer edge, the reach, is unchanged). The dust kicked out along the ground starts at 0.12 s, after the front has left the reach (was 0.03 s), so no haze sits inside the ring while it races out. |
| 2: a less regular streak fringe | Not changed: it needs a new blast-mark texture; recorded for the scale-out. |
| New note: four soft lobes at the flash's edge on teal at age 0 | Not changed (not visible at gameplay zoom). |

### Fog (checklist item 7), round 4

The round-3 review asked for a picture of the line running out of the fog. [`spine_line_fog_contact.png`](spine_line_fog_contact.png)
(legend [`spine_line_fog_contact.txt`](spine_line_fog_contact.txt)) and [`spine_line_fog_sequence.png`](spine_line_fog_sequence.png)
are the same program as the other sheets with the burrowed attacker and the line's first four search points in
the viewer's fog. The fog is drawn as the battle terrain draws it: fogged ground keeps a quarter of its colour, with
a hard edge at a cell boundary (`terrain.wgsl`). The line's points sit at cell centres, as for units standing on
cells, so the fog edge lies half a spacing before the first seen point.

What it showed, and what changed for it. Before round 4 every layer was clipped only to the whole line, so the
scar and fissure pieces of the first seen point ran on up to about a unit into the fog. Now the ground-hugging
layers (scar, tears, heave, fissure, dust, chips) end where the line enters the fog, the cell edge half a spacing
before the first seen point, fading in over 0.3 units across it (the same feather as at the line's ends). A first
fog take also clipped the clumps there, and the first clump, whose splayed spikes reach about 0.9 to each side of
its point, was cut in half at the edge; so a standing clump is now drawn whole whenever its point is seen (only the
line's own ends cut it): it never pops or appears cut, and its outer spikes may lean up to about 0.4 over the
fogged ground beside the first seen point. Nothing else is drawn on the fogged side, and nothing at the attacker.

### Crowd capture, round 4

[`stress_capture_400.png`](stress_capture_400.png) is re-taken with the round-4 effects (final data, the stain at
0.98 of the reach), the same way as round 3: debug build of `frame_budget`, lavapipe under Xvfb (the driver and
build of the contact sheets), a scratch copy of `assets/` whose `units.ron` gives `volatile_tick` and
`burrow_scorpion` the lab blocks of `crates/client/tests/data/effect_lab_blocks.ron`, tick 150, 200 splashes a
second; the figures are in [`stress_capture.txt`](stress_capture.txt), round-4 paragraph. Round 3's frame is in
commit `fd24c31`. The baseline is unchanged (it draws no effect).

### Fog capture, round 4

New in this round, see "Fog (checklist item 7), round 4" above: [`spine_line_fog_contact.png`](spine_line_fog_contact.png)
with its legend [`spine_line_fog_contact.txt`](spine_line_fog_contact.txt), and
[`spine_line_fog_sequence.png`](spine_line_fog_sequence.png), the line running out of the viewer's fog.

## Round 3

Rounds 1 and 2 were rejected for `acid_burst` and `spine_line` ([`independent_review.md`](independent_review.md),
[`independent_review_round2.md`](independent_review_round2.md), both kept as written); `shell_impact` was accepted
in round 2. The owner chose to continue, for up to five rounds in all (plan Amendment 2, Q0080), working from
round 2's ranked changes. Please write the round-3 review to a new file next to this one,
`independent_review_round3.md`, in the same form, and judge all three effects afresh against the checklist
(`shell_impact` too, so that a regression would show). The table below maps each round-2 finding to what changed;
it is a list of changes, not of verdicts. The images of this directory are the round-3 ones; round 2's are in
commit `d03ed19`, round 1's in `bb5a61c`.

### `spine_line`

| Round-2 finding | What changed |
|---|---|
| 4a: curved spine variants read as handwritten glyphs; C-, J- and claw-shaped hooks | New textures (image-model stills, [`texture_review.md`](texture_review.md), "Review round 3"): the clump at each search point is one card showing three to five **straight, tapering conical chitin spikes splayed outward from a shared base** (three variants: a fan of four, an uneven five, an uneven three), wide at the base and needle-sharp at the tip, upright within a few degrees, mirrored at random. The hook, claw, thorn and S-curve variants are gone. |
| 4b: sparse, one or two spines per point | Each point has its clump (three to five spikes) plus two single straight spikes leaning out beside it (up to ±26°), a frame later: four to seven spikes per point. |
| Ranked change 1: taller, 0.6–0.9 units | The clump card is sized to the spacing (1 unit) with ±15 % jitter, and the spikes stand about 0.6 to 0.9 units tall at the default binding; the single spikes are smaller (0.6 of the clump). |
| 5 / ranked change 2: no focal point; newest clump no brighter; tips not visible | The newest clump is lit for its first two to three frames: its ramp input is raised to 2.4× (the dark umber body goes to warm bone and the tips and the lit left edges to bone-ivory), easing back to its own colours by about 0.2 s after it breaks the ground. A short, light dust burst (pale, about 0.3 s) rises at each eruption. The tips stay paler than the bodies on every spike. |
| 4c / ranked change 3: hard straight cut at both ends of the churned earth and fissure | Every ground mark of the line (scar, fissure, cracks) **feathers over 0.3 units at each end** across the end (60 % of the fade beyond it, so the mark is still about two thirds strong at the end), instead of the 0.015-unit cut. Nothing is drawn more than 0.18 before the first search point (which lies 1.0 from the attacker's centre), and that last 0.18 is the faint tip of the fade. |
| 4d / ranked change 3: leftover marks alternate stars and donut rings at exactly 1-unit spacing | The churned earth is a new texture, a ragged furrow of torn earth, stretched along the line (each piece about 1.9 units long, 1 unit apart, scattered and of its own size), so the pieces merge into **one continuous broken scar** of uneven width. The crack bursts are now two narrow, elongated ruptures (the donut-shaped torn hole and the radial star are dropped), scattered off the line by up to 0.25, randomly turned, ±40 % in size, and gone by 1.5 s, before the scar. Each eruption also heaves a noise-broken patch of soil over its full-damage disc, which erodes away within 0.7 s, so the point that is hit reads at once without leaving a row of patches. |
| 4e: dust puffs pale flat blobs on teal | The settling dust is fainter (opacity 0.22 to 0.38, was 0.35 to 0.55) and darker; the new light dust burst is short and small. |
| Crowd note (b): the churned earth reads as dirt over the whole battlefield | The scar is lighter (opacity 0.34 to 0.52, was 0.45 to 0.62) and a little shorter-lived (2.3 s, was 2.4 s); the fissure lasts 2.5 s (was 2.6 s), so the marks still state the line once the spines have sunk. |

### `acid_burst`

| Round-2 finding | What changed |
|---|---|
| 4a / 5 / ranked change 1: one flat, uniform mid-green fill, no sheen, depth or value variation; the rim lighter, not darker | The stain is drawn as a **liquid** (a new decal material): noise varies its value and thickness so it pools darker and thinner in places; its thickness is lit as a surface by the key light from the upper left (the units' studio light), so swells and the thick rim catch a wet specular sheen and the far sides fall darker; the sheen and the relief light dry away with the stain. The colour ramp follows the art direction: wet emerald in the middle, a **darker, more saturated wet rim**, olive-brown crust when dry. |
| 3 note / ranked change 1: drying starts late (still saturated green at 1.8 s) | Drying starts at about 0.4 s and is well under way by 1.5 s (ramp factor 0.64 at 1.5 s, was 0.85); the sheen is gone by about 1.5 s. |
| 4b: evenly spaced round darker dots | The flecks the landing globs left (they were small copies of the stain, which read as dots) are removed; the stain's erosion noise is coarser (scale 1.4, was 2.6), so its holes are irregular rather than round. |
| 4c / ranked change 2: the pop is a glossy lime ellipse with a bevelled rim, like a button | The glowing dome is replaced by a **ragged burst**: the splash-crown flipbook (a real liquid splash from a Veo clip) drawn as liquid matter (dark wet body, lime highlights) and torn by coarse noise, plus a small hot core broken by noise for two to three frames and a ragged ground flash. The front's leading edge is a darker wet green, no longer a glowing lime rim. |
| 2 note / ranked change 2: globs only tiny white specks; larger globs with dark wet bodies | Nine globs of 0.17 to 0.30 units (were sixteen of 0.10 to 0.18), dark green bodies with a lime highlight. |
| 2 note / ranked change 2: vapour a flat lichen-like patch to one side | The vapour is four soft puffs at the centre (within 0.15 of the reach), rising (0.7 units per second) and spreading, gone by 1 s. |
| 4e / 6 / ranked change 3: in the crowd only thin pale-green front rings, like selection circles | The stain is now **Critical**: when the crowd budget retires the Detail layers, the stain stays and marks the area as a liquid disc, and the front sits on it. The front ring and its dark edge are more broken by noise. |
| 4d / ranked change 3: a pale grey-white fringe at the stain's edge | The stain's edge is now its darker wet rim (the ramp's top) over an olive-brown low end, shaded by the liquid relief; the round-2 vapour patch that lay beside the stain, and the flecks along its edge, are gone. Please check the edge at 0.48 to 1.8 s (rows 3 and 4). |

### `shell_impact` (accepted in round 2; optional changes)

| Round-2 optional change | What changed |
|---|---|
| 3: darken the char core towards 0.01–0.03 linear | Done: the char ramp is now 0.007 to 0.024 linear (was 0.010 to 0.035). |
| 1: vary the shock ring's width by about two, drop the faint fill | Not changed: it alters the accepted Critical layer and its footprint test; recorded for the scale-out. |
| 2: a less regular streak fringe | Not changed: it needs a new blast-mark texture; recorded for the scale-out. |

### Fog (checklist item 7), round 3

The owner extended the fog rules (Q0076, plan Amendment 2): an undetected burrowed attacker stays hidden, but its
spine line is shown to the viewer **wherever the viewer sees it**, from its first search point on. In round 2 the
line appeared only if its first search point was in sight; now a line that runs out of the fog into sight draws
at exactly the search points in sight (tested in pixels). Judge item 7 on the sheets as before: nothing may be
drawn on or behind the attacker (before the first search point and its 0.15-unit feather).

### Crowd capture, round 3

[`stress_capture_400.png`](stress_capture_400.png) is re-taken with the round-3 effects (see
[`stress_capture.txt`](stress_capture.txt)); the baseline is unchanged.

## Round 2

Round 1 was rejected ([`independent_review.md`](independent_review.md), kept as it was written). This is the
second and last round: if it is rejected too, the effect work stops (plan, WP6c gate). Please write the round-2
review to a new file next to this one, `independent_review_round2.md`, in the same form, and judge the material
afresh against the checklist; the table below says what changed for each finding of round 1 so that you can check
each one, but it is a list of changes, not of verdicts. The images of this directory are the round-2 ones; round 1's
are in commit `bb5a61c`.

**One art direction** now governs every effect: painted realism to match the units, one focal point, a palette per
element, soft eroded edges, irregular shapes from noise with a phase of its own per instance, and an airborne part
for every impact. It is written as concrete rules in [`../reference.md`](../reference.md) ("Art direction").

| Round-1 finding | What changed |
|---|---|
| Shell 1: the ring is cut to two arcs after 120 ms; the scorch reaches only about 0.8; nothing marks the 0.47 core | The shock front is a whole ring drawn over the fireball's base (sorted in front of it), wavering in width and brightness, brightest at its leading edge, thinner as it expands, at the reach from about 0.1 s. The ground mark is two layers: a charred core sized to the full-damage zone and a new scoured blast mark (radial soot streaks) sized to the reach. |
| Shell 4a: cotton-ball flash | The flash is a soft radial glow computed in the shader (white core, warm halo), broken slightly by noise, two frames long. |
| Shell 4b: bright rims on fireball lobes | Self-shading darkens thin parts of the fireball (and of smoke and dust) instead of lighting them. |
| Shell 4c: a clean ring of constant width | See item 1: the front is computed from noise, uneven and asymmetric per burst. |
| Shell 4d: a uniform grey scorch with stair-stepped edges, the same silhouette each time | New blast-mark texture (dense core, radial streaks, feathered fringe) over a near-black char core; erosion by noise with its own phase per instance and a soft step; decals sample the mip their screen size needs. |
| Shell 3 note / ranked change 3: no outward throw; the fireball holds one silhouette | Three overlapping fireball cards of their own rotation, size and noise phase, growing by about 50 % over the first 0.2 s; larger, darker debris chunks on ballistic arcs; longer sparks; a dust ring kicked outward along the ground. |
| Acid 2: everything is a coplanar decal | A glowing pop dome over the centre, sixteen globs thrown up on arcs that each leave a small fleck where they land, a fine spray, and a low vapour puff that is gone by 0.6 s. |
| Acid 3: four symmetric wedges; no visible drying; uniform fade | The front's break-up is driven by noise with its own phase per burst; the stain dries from wet emerald to an olive crust and is eaten by erosion from its edge and from holes. |
| Acid 4: clip-art splat; two stacked stickers; notched ring | One new stain texture (an irregular viscous puddle with a thick wet rim, two variants, random turn and noise phase) over the whole disc; no stacking; the splat texture is gone. |
| Acid 5: no focal point; pastel lime; washes out on teal ground | A lime-yellow pop and wet centre over a deep emerald field with a darker wet rim and a thin dark rim under the front. |
| Acid 1 note: 55 % of the radius at age 0 | The stain starts at about 60 % of the reach and spreads with the front to the reach by 0.17 s. |
| Spine 1 and 5: pale spines on pale sand; a dashed line | New chitin spines (four curved variants, dark umber with bone tips), larger, in clumps of three per point; the newest clump is lit as the travelling front. |
| Spine 2 and 3: two layers; no anticipation, no overshoot, fading retraction; regular dashes | Six layers: churned earth, a crack burst per point that opens about two frames before its spines, the fissure, the spines (rising out of the ground, overshooting by 10 %, settling, sinking back into the ground), dirt puffs and chips. The regular oval dashes are gone. |
| Spine 4: a comb of identical flat wedges leaning one way | Hashed variant, mirror, lean (±14° about the base), height (±30 %), scatter and stagger per spine; cards stand upright and turn to the camera, so they are never seen edge-on. |
| Spine 7: streaks about 0.6 units behind the first search point | Every layer (cards, ground marks and particles) is clipped to the segment from the first search point to the last. The first search point lies 1.0 from the attacker's centre, just beyond its body (radius 0.9375), so nothing is drawn on or behind the burrowed attacker. |
| Cross-cutting: flatness, three styles, symmetric erosion, alpha-clip stair-stepping | The art direction above; airborne parts in every effect; per-instance noise in every erosion; soft erosion steps and mip-matched decal sampling. |

## What the effects are meant to be

The game is a real-time strategy game whose unit mechanics follow StarCraft II. Each effect must read like the
StarCraft II behaviour named here, in an original look:

| Effect | Fired by | Must read like | Gameplay size (world units; 1 unit is one grid cell) |
|---|---|---|---|
| `shell_impact` | a siege tank analogue in siege mode | a heavy shell exploding on impact, leaving a burn mark | full damage radius 0.47, middle 0.78, reach 1.25 |
| `acid_burst` | a Baneling analogue (a creature that bursts) | a burst of green acid covering its whole area at once, leaving a drying stain | one zone, radius 2.2 |
| `spine_line` | a burrowed Lurker analogue | a straight line of spines erupting one after another from the attacker towards the target and on to full range, then retracting | 8 units long, a search point every 1 unit, radius 0.5 at each |

## Material

All files are in this directory unless a path is given. Open every image at full size.

### Contact sheets (one per effect)

Each sheet is a grid of real captures of the production renderer (1280×720, the battle camera at its 58° pitch)
at eight ages of the effect (columns; the legend file gives the ages in seconds). Rows, top to bottom: bright
ground at the battle's default zoom; the same with a 1-unit grid and the gameplay radii drawn as reference
(red: full damage, yellow: reach; for the line, a red circle at each search point); bright ground at close
zoom; its reference; dark ground at default zoom; its reference; dark ground at close zoom; its reference. The
reference rows are drawn only to let you check sizes; they are not part of the effect.

| Effect | Sheet | Legend |
|---|---|---|
| `shell_impact` | [shell_impact_contact.png](shell_impact_contact.png) | [shell_impact_contact.txt](shell_impact_contact.txt) |
| `acid_burst` | [acid_burst_contact.png](acid_burst_contact.png) | [acid_burst_contact.txt](acid_burst_contact.txt) |
| `spine_line` | [spine_line_contact.png](spine_line_contact.png) | [spine_line_contact.txt](spine_line_contact.txt) |

### Sequences (timing)

The first 3 seconds on the bright ground at default zoom, one frame every 40 ms (the game's presentation
step), read left to right, top to bottom; frame 0 is the moment of the event.

- [shell_impact_sequence.png](shell_impact_sequence.png)
- [acid_burst_sequence.png](acid_burst_sequence.png)
- [spine_line_sequence.png](spine_line_sequence.png)

### Crowd capture (checklist item 6)

A frame of the real client during a measured battle of 400 units (eight armies on a flat map, seen from far
above), with 200 splash events per second streamed over the battle, round-robin over the three effects, each
at a random live unit; and the same moment of the same battle without the stream, for comparison. The
effects' footprints (cards and ground marks, including their transparent parts) cover the share of the screen
given in [`stress_capture.txt`](stress_capture.txt).

- [stress_capture_400.png](stress_capture_400.png): with the stream
- [stress_capture_400_baseline.png](stress_capture_400_baseline.png): without the stream
- [stress_capture.txt](stress_capture.txt): tick, live effects, coverage and overdraw

### Fog (checklist item 7)

The fog rules are enforced in code and tested, not visible on a contact sheet: an effect is shown when the
point where it lands is visible to the viewer, and no effect has a layer at its source. For the spine line that
means the spines show where they erupt and nothing is drawn at the burrowed attacker. Judge item 7 on the
sheets (is anything drawn at the source end of the line, or outside the area?) and on this description.

## Checklist (plan §7.2, verbatim)

1. **Reads at gameplay zoom:** the dangerous area or line is identifiable within 0.1 s; a Critical layer marks its
   true size (±2 px at default zoom); one focal point; secondary elements do not compete with it.
2. **Layered:** at least three layers for a Major effect (for example flash or light, core, smoke or debris, ground
   mark); no single flat shape.
3. **Timing:** peak within 2 frames of the event (≤ 80 ms at 25 Hz presentation), eased fall-off, a dissipation that
   erodes rather than fades, nothing lingers past its duration except authored decals.
4. **No tells of cheapness:** no visible quad edges, no hard line where a card meets the ground (soft depth), no
   visible tiling or repetition (random rotation, flip, frame offset, variants), no pops on spawn or despawn, no
   banding in gradients, no colour fringes from keying.
5. **Value and colour:** brightest at the focal point, HDR core where the element emits, ramp from hot to dark; team
   colour only in secondary elements; consistent with `media_style.ron`'s palette.
6. **Crowd behaviour:** 200 splashes per second over a 400-unit fight still shows the units (overdraw and coverage
   measured, §10.3); the budget's degradation is not visible as popping.
7. **Fog:** nothing shows that the viewer may not see (D-5); an undetected Lurker's spines show, the Lurker does not.

Note on item 5: the project decided against an HDR camera with bloom (no value above display white exists in the
picture), so "HDR core" is to be read as "the core is the brightest, display-white part of the element".
