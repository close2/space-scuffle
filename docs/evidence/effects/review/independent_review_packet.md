# Independent Review Packet: Effect Vertical Slice (Round 3)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
This packet is for the second reviewer of the vertical slice. It lists the material to judge and the checklist to
judge it by, and nothing else: it holds no verdict of the implementer, on purpose. Please judge each of the three
effects against each of the seven checklist items, write **accept** or **reject** with a concrete reason (which
sheet, which cell or frame, what you see), and put your review in a new file next to this one
(round 3: `independent_review_round3.md`, see "Round 3" below).

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
| 4c / ranked change 3: hard straight cut at both ends of the churned earth and fissure | Every ground mark of the line (scar, fissure, cracks) **feathers over 0.3 units at each end**, centred on the end (half the fade inside the line, half beyond), instead of the 0.015-unit cut. Nothing is drawn before the first search point's feather (the first point lies 1.0 from the attacker's centre). |
| 4d / ranked change 3: leftover marks alternate stars and donut rings at exactly 1-unit spacing | The churned earth is a new texture, a ragged furrow of torn earth, stretched along the line (each piece about 1.9 units long, 1 unit apart, scattered and of its own size), so the pieces merge into **one continuous broken scar** of uneven width. The crack bursts are now two narrow, elongated ruptures (the donut-shaped torn hole and the radial star are dropped), scattered off the line by up to 0.25, randomly turned, ±40 % in size, and gone by 1.5 s, before the scar. |
| 4e: dust puffs pale flat blobs on teal | The settling dust is fainter (opacity 0.22 to 0.38, was 0.35 to 0.55) and darker; the new light dust burst is short and small. |
| Crowd note (b): the churned earth reads as dirt over the whole battlefield | The scar is lighter (opacity 0.34 to 0.52, was 0.45 to 0.62) and shorter-lived (2.0 s, was 2.4 s); the fissure lasts 2.2 s (was 2.6 s). |

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
