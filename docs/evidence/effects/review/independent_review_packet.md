# Independent Review Packet: Effect Vertical Slice (Round 2)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
This packet is for the second reviewer of the vertical slice. It lists the material to judge and the checklist to
judge it by, and nothing else: it holds no verdict of the implementer, on purpose. Please judge each of the three
effects against each of the seven checklist items, write **accept** or **reject** with a concrete reason (which
sheet, which cell or frame, what you see), and put your review in a new file next to this one
(round 2: `independent_review_round2.md`, see "Round 2" below).

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
