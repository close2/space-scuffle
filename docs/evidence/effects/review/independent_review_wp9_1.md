# Independent Review of WP9 (Status and Death Looks), Round 1

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP9 row of
§9, §7 (quality bar) and Amendment 16. Packet: [`independent_review_packet.md`](independent_review_packet.md), section
"WP9 round 1". Checklist: plan §7.2, quoted at the end of the packet. This is round 1 of at most five (Q0080). I did
not read the implementer's self-checklist before forming these verdicts.

## Method

I read the packet section, the plan's WP9 row, §7 and Amendment 16, and skimmed the WP8 reviews for the bar. I read
the WP9 diff against `main`: `assets/catalog/effects.ron` (the WP9 looks and ramps), `assets/shaders/effects/sprite.wgsl`
(`membrane_texel`), `assets/shaders/bubble_dome.wgsl` (the stim arc), `client::effects::stance` (`present_looks`,
`keep_statuses`, `status_shown`, `hit_due`, `emit_due`), `client::effects::systems` (`spawn_request`,
`replaces_shatter`), `vfx.rs`, `client::effects::runtime::layer_strength`, `battle_pipeline::{driver,fog}`
(`publish_combat_events`, `reveals`) and `crates/client/tests/status_gpu_tests.rs`. I built and ran nothing. Every
figure below is an ImageMagick crop of the committed pictures (nearest-neighbour, 2x to 6x), kept in
`~/scratch/wp9-review1/`. "Cell t = x" means the sequence cell with that label; window coordinates are 1280×720
pixels of the battle frames.

## Verdict table

| Look | Verdict | Main reason |
|---|---|---|
| `stim_surge` | **reject** | The red rim reads, and its beat does not distract. The fumes cannot be seen, and the stim-in flare and vapour are never pictured (A1-1, V1-1) |
| `concussive_daze` | **reject** | The cold rim is nearly the default lavender bubble rim, and the body is not visibly greyed. The drag dust is invisible in the lab, and an unexplained pink haze trails the unit in the battle (A1-2, E1-6) |
| `charge_streak` | **reject** | The violet rim does not show on a violet unit. The dust is a speck of about 6 px at close zoom and invisible at gameplay zoom. The staged charge never lunges (A1-3, V1-2). Nothing line-like: pass |
| `barrier_shell` | **reject** | Not readable on a small unit. It is a faint brown arc under the lower rim with no upper half, and it is almost gone once the barrier has absorbed (A1-4) |
| `barrier_absorb` | **reject (evidence)** | The flare's gold ring shows for one sampled frame per hit. The attacker's legacy melee crescent, red-white and far brighter, covers it every time, so the flare cannot be judged alone (V1-3) |
| `vehicle_wreck` | **accept** | Strong hot flash and fireball, then smoke and a charred scorch. It reads as a vehicle blowing up and is plainly not the glass shatter. Advisories P1-1, P1-2 |
| `bio_burst` | **reject** | It reads as a translucent mauve disc, then a hard-edged round brown puddle. No splash or globs can be seen, so it does not read as a creature bursting. Its stain paints over a resource node (A1-5, A1-6) |
| Default glass shatter | not regressed (code) | Only units whose block names a death skip it (`replaces_shatter`). No picture of the default shatter this round |
| WP8 looks, shipped splash effects | no regression found | Nothing in the diff changes their data or shaders apart from adding branches. `present_looks` now also releases status looks off `Shown` |
| Engineering | **reject** | E1-1 and E1-2: the battle test's death and emit assertions do not prove what its doc says |
| **Overall** | **REJECT** | |

## 1. `stim_surge`

Evidence: `wp9r1_stim_sequence_*` with the unit cropped at 3x, `wp9r1_lab/*/stim_fumes_repeat_*`, and battle frame 1
at (586, 383).

1. **Reads at gameplay zoom: partly.** At 1:1 the unit is a disc about 22 px across, and the red rim is a 1 to 2 px
   salmon ring. Next to the lavender rims of the units around it, it does read as "something is different about this
   one". It is the only element of the look I can see. I see no warmed picture: the dark contents with cyan dots look
   the same as on an ordinary unit of that kind.
2. **Layered: fail as pictured.** The look claims three parts: a beating rim, a flare with a breath of vapour, and
   fumes. In the battle only the rim shows. `stim_fumes` in the lab, close zoom on bright ground, is a salmon blot of
   3 to 4 px (ages 0.06 to 1.0 s). At the default zoom it is one or two tinted pixels, and in the battle crops I find
   no trace of it in any cell. For scale: the fumes are `UnitRadius × 0.3` (about 0.15 world units for a radius-0.5
   unit) with the "stim_vapour" ramp at alpha 0.16 to 0.34. That is too small and too faint to survive at gameplay
   zoom.
3. **Timing: not evidenced.** The unit is spawned with the overclock buff already on, so it is first seen in the
   status and its look is held settled (`keep_statuses(..., settled)`). The flare and "breath of red vapour" are
   therefore never played in any picture.
4. **Cheapness: pass.** Nothing pops, and the rim is soft.
5. **Value and colour: pass, with one advisory.** The deep red is in the palette. A thin red ring is also the
   language of "enemy" or "damaged" in many RTS UIs. In this map's red-and-blue team scheme it should be checked
   next to a red team (P1-3).
6. **Crowd: not judged.** No stimmed squad is pictured. Twenty stimmed units laying fumes every 0.45 s is cheap at
   this size.
7. **Fog: pass** (see §8).

**Beat:** the measured rim runs from 0.252 to 0.620, at 0.6 s. In the 120 ms-sampled sequence I see the ring vary
mildly, never blink off, and never flash brighter than the bubble's highlight. It reads as a pulse, not a blinking
light. Accepted.

## 2. `concussive_daze`

Evidence: `wp9r1_daze_sequence_*` with the unit cropped at 3x, `wp9r1_lab/*/daze_drag_repeat_*`, and battle frame 1
at (551, 249).

1. **Reads: fail.** The daze rim, `(0.48, 0.52, 0.70, 0.32)`, is a pale cold lavender. Every bubble already has a
   pale lavender rim: compare the "standing, no barrier" cell of `wp9r1_barrier_pose_radv.jpg` with any daze cell.
   At gameplay zoom the slowed unit looks like an ordinary unit. The body is not visibly cooled or greyed either: its
   green contents are as saturated in cell t = 3.52 as in t = 0.04, and there is no unslowed control picture of the
   same unit to compare. It reads neither as "slowed" nor as "selected". It barely reads at all.
2. **Layered: fail.** `daze_drag` is invisible in the lab. In the close-zoom cells at ages 0.06 to 1.0 s on bright
   ground I find no pixel I can attribute to it. The "dust" ramp at alpha 0.22 to 0.38, at `UnitRadius × 0.32`, is
   too faint. The flare is not pictured, for the same settled-on-first-sight reason as the stim.
3. **The pink haze.** In the battle, from t = 0.40 to about 2.2 s, a soft lilac-pink haze about one unit radius wide
   lies on the ground behind the walking unit (left of it at (530, 250); clearest in cells t = 1.24 and 1.48, and in
   battle frame 1). It looks like a magic trail, not dust. Nothing in `concussive_daze` or `daze_drag` is pink. A
   similar pink haze shows by the dying creature before it dies (deaths cells t = 0.28 to 1.00, its lower left).
   Its source must be found (E1-6). If it is `daze_drag`, it is the wrong colour on this floor. If it is not, the
   daze has no drag at all.
4. to 7.: no tells of cheapness and no fog issue. Colour: the cool grey is right in intent but too weak to work.

## 3. `charge_streak`

Evidence: `wp9r1_charge_sequence_*` cropped at 2x, `wp9r1_lab/*/charge_dust_repeat_*`, and battle frames at
(691, 225).

1. **Reads: fail.** The charging unit's media is violet, and its rim is violet-white `(0.72, 0.64, 1.0, 0.58)`.
   On it, the rim adds almost nothing: cells t = 0.04 to 2.08 look like the ordinary unit. The ground trail is not
   visible at gameplay zoom. In the lab, close zoom on bright ground, the `charge_dust` scuff is a dark speck of
   about 6 px with hairline cracks. The warm dust puff is a faint tan smudge at most (ages 0.15 to 0.5). At the
   default zoom both are one to three pixels.
2. **The staging does not show a charge.** The enemy is 3.8 units away, but the viewer's worker line stands
   between them. In the sequence the unit drifts down into the workers and jostles there. The Dash status is
   published on 63 of 88 frames (2.5 s), which is no lunge. Whether the look works on a real lunge is not evidenced
   (V1-2).
3. **Art rules: pass.** Nothing is line-like. There are no streaks, rays or speed lines. The scuff's hairline cracks
   radiate from the speck, but at this size they make no star or sunburst. If the scuff is made larger (A1-3), check
   that again.
4. Timing, colour, fog: no objection. The scuff's 1.6 s linger is an authored decal.

## 4. `barrier_shell`

Evidence: `wp9r1_barrier_pose_*` cropped at 4x, `wp9r1_barrier_sequence_*` cropped at 4x, and the measurements.

1. **Reads on a small unit: fail.** In the pose at 0.5 left (cell 2), the shell is a thin brownish-amber arc along
   the lower and lower-right rim of the bubble, about 1 px wide at 1:1. The upper half of the ring shows nothing.
   That fits the measurement: 104 of the 198 ring pixels change, 52.5 %, and at a change threshold of 8 levels in
   255. At 0.05 left (cell 4) only a trace of the arc remains. It does not read as "amber glass and a pale gold
   light towards its edge" round the body. At gameplay zoom it reads, at most, as a slightly dirty lower outline.
2. **In the battle**, at full charge (cells t = 0.16 to 0.88), the arc is a little stronger and goldish, still mostly
   on the lower half. After the first absorbed hits (t ≥ 1.48) the floor of 0.45 leaves it practically invisible,
   although the barrier is still up on all 88 frames. A player cannot tell this unit has a barrier.
3. **Drawn circle?** No. What there is fades softly (`MEMBRANE_EDGE` works). The problem is that there is too
   little of it, not that it is too hard.
4. **"Only while `barrier_ratio > 0`": pass.** "Down" changes 2 pixels on radv and 0 on lavapipe, and "without its
   layers" changes 0. The rule holds. Note that radv's 2 is exactly `TOLERATED_PIXELS` (E1-4).
5. The snap-up pass is not pictured, because the barrier is staged raised.

## 5. `barrier_absorb`

The flare is pictured in cells t = 1.12, 2.20 and 3.28 as a full pale-gold ring round the body, one sampled frame
each. That is round, warm, and visible at 4x. In every case, though, the attacker's legacy melee slash
(`combat_fx.wgsl` mode 3, a red-white crescent) lands on the same unit in the same and the next sampled frame
(t = 1.24, 2.32, 3.40). It sits under the lower rim and is far brighter than the flare. The test picked "an attacker
whose shots draw no splash of their own", which for this unit is a melee unit with the legacy slash. So the
evidence shows the slash, not the absorb. In the lab the flare is a half dome cut flat at the ground (the lab anchors
it at the ground, not at a body's centre), so the lab cannot show it either. I cannot accept or reject the art on
this evidence (V1-3). In passing: the absorb flash competing with a melee crescent is what will happen in real
games against the Zealot analogue. Once the absorb flash is isolated, judge whether it holds up next to that
crescent.

## 6. `vehicle_wreck`: accept

Evidence: `wp9r1_deaths_sequence_*`, battle frames 1 to 3 cropped at 4x round (892, 231) and (899, 270),
`wp9r1_lab/*/vehicle_wreck_*`.

- **Reads: pass.** At t = 1.12, one presented step after the kill at 1.0 s, both vehicles are hot yellow-white
  fireballs of about 1.3 radii, with pale glass specks and dark chips thrown out. It is unmistakably a vehicle
  blowing up, and nothing like the glass shatter.
- **Layered: pass.** Light, flash, fireball, glass shards, sparks, chunks, smoke, ember scorch and char scorch.
- **Timing: pass.** The peak is in the first sampled frame after the event. The fireball cools through orange
  (t = 1.36 to 1.60) into grey smoke (1.72 to 2.56) that erodes. The char scorch lingers as an authored decal. The
  lab's last column (age 1) is clean.
- **Cheapness: pass.** No quad edges and no pops. The two vehicles' scorches are of different sizes and do not repeat
  visibly.
- **Colour: pass.** Brightest at the core, ramping hot to dark.
- Advisory **P1-1**: the smoke is grey, not the "black smoke for two seconds" the packet promises. Darker smoke
  would also separate a wreck from a shell impact.
- Advisory **P1-2**: the stack is nearly the siege shell's detonation, with the same fireball and the same
  `scorch` ember and char decals. A death next to a siege bombardment will read as one more shell. Wreck-specific
  debris, such as larger dark chunks or a lingering burning hull ember, would give it its own identity.
- The glass shards keep the bubble's identity only weakly: they are pale-blue specks, visible at 4x and barely at
  1:1. That is acceptable, because the fireball carries the read.

## 7. `bio_burst`

Evidence: the same sequence and battle frames round (913, 303), and `wp9r1_lab/*/bio_burst_*`.

1. **Reads: fail.** At t = 1.12 to 1.24 the creature's place becomes a translucent mauve-maroon disc about 1.1
   radii across, with a darker outline: the `acid_crown` card with the "ichor" ramp, seen against the light floor.
   It reads as a ghost of the bubble, not as a body bursting. No ichor globs on arcs are visible, at 1:1 or at 4x.
   From t = 1.36 the stain takes over. It is a round, hard-edged, flat brown ellipse with a darker rim, and it stays
   the whole sequence. In the lab, close zoom, the stain is a uniform brown coin with a crisp round edge, plus two or
   three glass specks and one dark dot (age 0.06). It reads as a puddle or a drawn disc, not as a splash.
2. **Layered: fail as seen.** Five layers are authored, but the picture shows two: the disc, then the coin.
3. **Cheapness: fail.** The stain's round boundary is a visible procedural shape. A splash needs an irregular,
   lobed outline with satellite drops, the way the accepted acid stains break their edges.
4. **Distinct from the glass shatter and the vehicle: yes**, but for the wrong reason: it reads as a hole or puddle.
5. **Stain on a resource.** The creature stood next to a green crystal node. From t = 1.36 to 3.52, and in battle
   frames 2 and 3, the brown stain covers the node's orange-grid base, which shows through tinted inside the
   ellipse. The crystal's green stays bright, which suggests the decal paints the node's ground-level parts. A ground
   decal must not paint resource nodes, structures or units, only the terrain (A1-6). Decals need not avoid spawning
   near resources.

## 8. Engineering

**Fog in `stance.rs`: accept.** `present_looks` releases the stance, morph and every status slot, and clears
`hit_seen`, whenever `presence != Presence::Shown`. `keep_statuses` runs only after that early return. An undetected
cloaked enemy (`Shimmer`) or a burrowed one (`Hidden`) therefore holds no status look, lays no fumes, dust or scuffs,
and spawns no absorb flash. A released look erodes out over its `exit` (0.3 to 0.4 s) where it was last seen, as
WP8's looks do. The viewer saw it there a frame earlier, so this is no leak. Emitted effects are laid only while the
look is held, so only where the unit is seen.

**Deaths and fog: accept, with one owner point.** The death effect is spawned from the same `BubbleShatter` event as
the legacy shatter. That event is admitted by `BattleFog::reveals` (the event's cell is visible, or the dead unit is
the viewer's) in `publish_combat_events`, and `battle_fog_tests.rs` already proves a foe's shatter in fog is dropped.
A death in fog therefore draws nothing. The same rule shows the death of an *undetected* cloaked or burrowed enemy in
a visible cell. That was true of the glass shatter too, and StarCraft II shows such deaths. A fireball or a stain
5 s long, however, now marks the spot far more clearly. I accept this as SC2-faithful, but it belongs in the plan's
decisions as a stated D-5 exception.

**Cadence: accept.** `emit_due` beats on `floor((t + phase·every) / every)` with a per-unit phase from the entity id.
That gives exactly one lay per beat per unit, out of step across a group. `hit_due` spawns on the first frame of a
hit and then at most once per `every`. The test's three hits, three flashes and 0.16 s spacing agree.
`MOVING_SPEED = 1e-3` is a threshold on published velocity. A unit pushed by a neighbour while standing would count
as moving and lay charge dust. That is harmless.

**`Membrane` card: accept as code.** `membrane_texel` is a sound sphere-outline shading (face 0.12, rim rising with
obliquity squared, an 8 % soft outline). Ribbons map it to radial and are refused at load. Its poor read (§4) is a
matter of data and placement: intensity, ramp alpha, and where the card stands relative to the bubble. The missing
upper half needs explaining (A1-4).

**Legacy stim arc: accept.** `bubble_dome.wgsl` §7 now requires `shell.driven.x < 0.5`, so on effect-driven battle
bubbles the warm arc is gone, and the Visual Lab checkbox keeps it. In the stim crops I see no trailing warm arc.

**`status_gpu_tests.rs`, how meaningful.**
- *Held exactly while shown*: meaningful for the holding logic (a status that stops, Dash at 63 of 88 frames, is
  released in step), though it compares two values the client derives from the same snapshot.
- *Stim rim beats*: meaningful (range 0.25 to 0.62), but it checks no period. That is acceptable.
- *Flashes keep their beat*: meaningful.
- **E1-1 (required)**: *"effects were spawned"* (`spawned.1 > spawned.0`) does not show that any death effect
  spawned. The stimmed unit's fumes (every 0.45 s, moving or not) and the charge dust alone raise the count after the
  deaths. Count spawns per effect id (two `vehicle_wreck`, one `bio_burst`). Also add a control: a dying unit whose
  block names no death, which must produce shards. Otherwise "no shards after the deaths" passes just as well if the
  shard query never matches. The window starting at DEATH + 0.2 s does sit inside the shards' 0.75 s life, so the
  check is not vacuous on timing.
- **E1-2 (required)**: the module doc says the parent checks "that the charge lays its dust". Nothing asserts that.
  Assert that `charge_dust` spawns while the charging unit moves and stops when it stands, and that
  `stim_fumes` spawns at its beat.
- **E1-3 (advisory)**: the test changes the shared catalog archetype of each dying type (`weapon = None`) and of the
  barrier stand-in. That is fine in a test world, but it is invisible to a reader of the pictures: an enemy of the
  same type elsewhere would also be unarmed.
- **E1-4 (evidence)**: radv's "down" changes exactly the tolerated 2 pixels. Say which two, so the margin is not luck.
- **E1-5 (evidence)**: no test or picture exercises a status *onset* on a unit already seen: the stim going in, the
  concussive blow landing, or the barrier snapping up. Every subject is staged in its status, so the look starts
  settled, and the flare layers of three looks are untested and unpictured.
- **E1-6 (required)**: identify the pink ground haze (§2.3), with a picture without effect layers.

**gpu_validation_tests on radv.** Nothing in these pictures suggests a real radv defect. The radv and lavapipe
measurements agree line for line: the same staging coordinates, the same frame counts, rims 0.252 to 0.620 on both,
104 ring pixels on both, and 62 against 64 at 0.05. The two drivers' sequences match by eye. I have not seen the
radv failure log, however. Please attach its lock-timeout lines from `/run/lock/gpu-lock.log` or the harness, or a
clean radv rerun of `gpu_validation_tests`, so that "lock starvation only" is evidenced rather than stated.

## Required changes

| ID | Look | Change |
|---|---|---|
| A1-1 | `stim_surge` | Make the fumes perceptible at gameplay zoom (larger: at least about 0.6 radii; denser; lifted over the crown, where they separate from the floor), or drop them and record it. Make the warmed body visible against an unstimmed unit of the same type |
| A1-2 | `concussive_daze` | Give the daze a read that differs from the default rim. Desaturate and darken the body clearly (show a slowed and an unslowed unit of the same type side by side). Make the drag dust visible in the lab at default zoom, in a dust colour |
| A1-3 | `charge_streak` | Make the trail visible at gameplay zoom: scuffs of at least about 0.8 radii with a broken, irregular outline, and dust puffs that read for 0.3 to 0.5 s. Give the rim a value or colour that separates from the unit's own media, for example a hotter, whiter outline. Keep it free of streaks |
| A1-4 | `barrier_shell` | Make the shell read round the whole body on a small unit at full, half and 0.05, including after absorbed hits. Raise the floor and the membrane alpha, and find out why the upper half is missing. Report the ring change at a perceptual threshold (for example ≥ 24 levels), not 8 |
| A1-5 | `bio_burst` | Make the burst moment read as an organic splash: dark, wet, irregular, with globs visible at gameplay zoom. Give the stain a lobed, broken outline with satellite drops instead of a round coin |
| A1-6 | decals | Ground decals (stain, scorch, scuff) must not paint resource nodes or structures. Show a death stain next to a crystal node without and with the fix |
| E1-1 | test | Per-effect death spawn counts, plus a default-death control that produces shards |
| E1-2 | test | Assert the emitted `charge_dust` and `stim_fumes` |
| E1-6 | evidence | Identify the pink haze trailing the slowed unit and beside the dying creature |

## Evidence asks

| ID | Ask |
|---|---|
| V1-1 | Status onsets on units already seen: stim going in, a real concussive hit landing, the barrier raised by its first hit (snap-up pass), at 40 ms steps |
| V1-2 | A real charge on clear ground: no units between the charger and its target, crop following the charger |
| V1-3 | The absorb flash alone: a ranged attacker without a legacy impact (or the legacy slash suppressed for the shot), plus one take with a melee attacker for the competition question |
| V1-4 | Per look, a gameplay-zoom pair, "with status" next to "same unit without", at 1:1 and 3x |
| E1-4, radv | The two radv "down" pixels; the radv lock-timeout log or a clean `gpu_validation_tests` rerun |

## Advisories

- P1-1: the wreck's smoke is grey, not black.
- P1-2: the wreck is close to the siege shell's detonation; give it wreck-specific debris.
- P1-3: check the stim's red rim next to a red-team unit.
- E1-3: the test unarms the dying types' archetypes for the whole test world.
- D-5: record in the plan that a death of an undetected enemy in a visible cell is shown, as in StarCraft II.
