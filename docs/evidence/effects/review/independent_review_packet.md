# Independent Review Packet: Effect Vertical Slice

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
This packet is for the second reviewer of the vertical slice. It lists the material to judge and the checklist to
judge it by, and nothing else: it holds no verdict of the implementer, on purpose. Please judge each of the three
effects against each of the seven checklist items, write **accept** or **reject** with a concrete reason (which
sheet, which cell or frame, what you see), and put your review in a new file next to this one,
`independent_review.md`.

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
