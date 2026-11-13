# Independent Review of WP9 (Status and Death Looks), Round 2

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP9 row of
§9, §7 (quality bar), Amendments 16 and 17. Packet: [`independent_review_packet.md`](independent_review_packet.md),
section "WP9 round 2". Round 1: [`independent_review_wp9_1.md`](independent_review_wp9_1.md). This is round 2 of at
most five (Q0080). I did not build this work, did not write round 1, and did not read the implementer's
self-checklist before forming these verdicts.

## Method

I read round 1's review, the packet section, and Amendment 17. I read the round-2 code diff (`930ae11..daaa41f`):
`assets/catalog/effects.ron`, `assets/shaders/effects/decal.wgsl` (the exclusion discs),
`client::effects::{runtime, systems}` (`nearest_obstacles`, `collect_ground_obstacles`, `spawned_of`) and the
assertions of `crates/client/tests/status_gpu_tests.rs`. I read `sim_core::entity::apply_concussive_slow` and its
callers. I built and ran nothing. Every figure below is an ImageMagick crop of the committed `wp9r2_*` pictures
(nearest-neighbour, 2x to 8x), compared with the `wp9r1_*` pictures, and kept in `~/scratch/wp9-review2/`. "Cell
t = x" means the sequence cell with that label. Unless I say otherwise, I describe radv. The lavapipe pictures and
figures match radv: the same staging, the same counts, and ring figures within 2 %.

## Verdict table

| Look | Verdict | Main reason |
|---|---|---|
| `stim_surge` | **accept** | Warm body, red rim and rising fumes read at 1:1 next to an unstimmed twin. The onset flare plays. Advisory P2-1 |
| `concussive_daze` | **reject (narrow)** | The greyed, dulled body reads as different at 1:1 and is not a rim. The drag dust is still invisible at gameplay zoom and in the lab's default-zoom row (A2-1) |
| `charge_streak` | **reject** | The hot white outline works. The trail does not: its scuffs fade in over about 0.5 s, so the torn ground appears after the 0.4 s charge has ended. The tan puffs cannot be seen on the light floor. Overlapping scuffs merge into one continuous dark stroke (A2-2) |
| `barrier_shell` | **accept (art)**; E2-1 open | Reads as a full gold ring round a small unit at full, half and 0.05, at 1:1. In the battle take, though, a barrier held at full is invisible for its first 0.8 s (E2-1) |
| `barrier_absorb` | **accept** | Every absorbed hit brightens and thickens the whole ring, apart from the legacy slash at the contact edge. The slash no longer blocks judging. Advisory P2-2 |
| `bio_burst` | **reject** | The splat now reads as organic, but the stain's body is still a near-round disc. Its rim is a fringe of 1 px radial hairs: an aliased, comb-like tell. Its drops are round coins (A2-3) |
| `vehicle_wreck` | **reject (regression)** | Fireball and black smoke are good. The new "embers in the scorch" are flat, opaque orange blobs that read as paint, not glow (A2-4) |
| A1-6 obstacle mask | **reject** | The node keeps its look, but the stain gets a crisp circular hole about three times the crystal's size. The hole is a new drawn-circle tell (A2-5) |
| E1-1, E1-2, E1-4 | **accept** | The assertions now prove what they claim (§8) |
| E1-6 pink haze | **accept** | Present in both the haze and the bare frame: it is the smog's wake (`smog_volume.wgsl`, `bubble_vel_boost`), not an effect. Advisory P2-3 |
| Fog | **accept** | Status looks and deaths are unchanged since round 1. The mask is built only from drawn nodes and live bubbles, so it reveals nothing the viewer does not already see |
| Concussive slow in the simulation | **confirmed bug, already recorded** | No production code calls `apply_concussive_slow`, and `concussive_shells` has `effects: []`. This is B-44 and B-77, scheduled for WP13d (§9) |
| **Overall** | **REJECT** | A2-1 to A2-5, E2-1 |

## 1. `stim_surge`: accept

Evidence: `wp9r2_pair_stim_*`, `wp9r2_onset_stim_*` cropped at 3x, and `wp9r2_stim_sequence_*`.

1. **Reads at gameplay zoom: pass.** In the 1:1 pair, the stimmed unit is a salmon-red disc next to its navy twin
   and cannot be mistaken for it. The contents stay legible through the tint.
2. **Layered: pass.** There is a warm body, a red rim, and fumes. The fumes are salmon wisps of about half a radius
   that rise above the crown and trail behind a running unit (stim sequence, t = 2.24 to 3.44). They are now
   visible at 1:1.
3. **Timing: pass.** The onset (t = 1.08) is a red ring flare, then puffs rise from 1.16 on. The beat varies the warmth
   mildly (sequence t = 0.16 to 0.40 against 0.76) and never blinks.
4. **Cheapness: pass.** The puffs are soft and irregular. Nothing is line-like.
5. **Colour: pass, with an advisory.** P2-1: the body is now strongly salmon. On a red team, or next to the
   smog's pink wake (P2-3), "stimmed" may read as "burning" or "damaged". Round 1's P1-3 (check it next to a red
   team) remains open.
6. **Crowd, 7. fog:** no objection. The fumes are cheap at this size.

## 2. `concussive_daze`: reject (narrow)

Evidence: `wp9r2_pair_daze_*`, `wp9r2_onset_daze_*` at 3x, `wp9r2_daze_sequence_*` at 2.5x, and
`wp9r2_lab/lavapipe/daze_drag_repeat_*`.

1. **Reads: pass for the body.** In the pair, the slowed unit is a flat, matte, mid-grey disc next to a navy, glossy
   twin. The difference is obvious at 1:1. It reads as "drained" or "inert". It does not read as selected (it has no
   rim), as cloaked (it is opaque), or as dead (it keeps its contents and its shadow). In the battle the
   green-media unit is greyed green with dulled glass and stays distinct from its neighbours over the whole
   sequence.
2. **Onset: pass.** At t = 1.08 a pale rim flare, then the grey comes in over about two frames.
3. **Layered: fail.** `daze_drag` cannot be seen. In the battle cells of the slowed, walking unit (t = 0.04 to 2.12,
   2.5x) I find no grey-brown dust. The only thing behind the unit is the smog's pink wake. In the lab's
   bright-ground default-zoom row I find no pixel of it at any age. At dark close zoom it is a puff of 3 to 5 px.
   It has the same failure as in round 1 and does not meet A1-2's "visible in the lab at default zoom". The body alone
   now carries the read, so either fix is acceptable (A2-1).
4. to 7.: no cheapness tells. Colour is right. No fog issue.

## 3. `charge_streak`: reject

Evidence: `wp9r2_charge_sequence_*` (crop follows the charger) cropped at 4x, battle frame 0 at 4x, and
`wp9r2_lab/lavapipe/charge_dust_repeat_*`. The charge is now a real lunge on clear ground (V1-2 met). The status is
shown on 10 frames (0.4 s), and 6 dust effects are laid.

1. **Outline: pass.** In cells t = 0.04 to 0.52 the charger carries a pale, near-white outline that separates from
   its violet media. That is the fix A1-3 asked for.
2. **Trail timing: fail.** While the charger moves (t = 0.16 to 0.52, and battle frame 0 at 0.4 s) the ground behind
   it shows only the smog's pink wake. The dark scuffs first appear at t = 0.76 to 0.88, after the lunge has ended,
   and stay until about 1.6 s. The cause is in the data: the `scuff` ramp starts at alpha 0 and reaches 0.85 only at
   30 % of its 1.6 s life, about 0.48 s after each lay. Torn ground must be there the instant the foot leaves it.
3. **Dust puffs: fail in the battle.** `charge_kick` (light tan, up to 0.78 alpha) shows in the lab on the teal
   ground. On this map's pale grey floor it has almost no value contrast. I find no puff in any battle cell.
4. **Shape: borderline.** With a lay every 0.06 s on a 0.4 s lunge, the scuffs overlap into one continuous wavy
   dark band about three radii long (cells t = 0.88 to 1.48). It reads more like a skid or drag mark than like
   footfalls tearing ground. It is not a speed line, but it is one stroke. Separate gouges with gaps between them
   would read as kicked-up ground and keep clear of the "no lines" rule.
5. Colour, fog: no objection.

## 4. `barrier_shell`: accept (art); E2-1 open

Evidence: `wp9r2_barrier_pose_*` at 4x to 8x, `wp9r2_onset_barrier{ranged,melee}_*` at 3x to 5x, battle frames 0
to 3 at 4x, `wp9r2_barrier_sequence_*`, and the measurements.

1. **Reads round the whole body: pass.** In the pose at 0.5 and at 0.05, the shell is a soft gold-amber ring all
   round the bubble at about 1.3 radii, thicker and brighter along the lower rim and fainter at the top, but
   present there. In the onset take at 1:1 (cells t = 1.16 onward), the shielded unit is unmistakably different
   from its twin. The figures agree: 53 % of the ring changes by 24 levels or more at 0.5, and 41 % at 0.05.
2. **Snap-up: pass.** At t = 1.16 the ring comes in bright and thick, then settles by 1.24.
3. **Drawn circle: acceptable.** The ring is soft-edged with a broken, uneven density. It reads as a membrane seen
   edge-on, not as a stroked circle. Its colour on this floor is closer to khaki than gold (P2-2).
4. **Only while absorption is left: pass.** "Down" changes no ring pixel on either driver (E1-4).
5. **Full and 0.05 look nearly the same** (358 against 351 changed pixels). The floor of 0.75 makes the shell a
   presence cue rather than a gauge. The health and shield bar carries the amount, so I accept this.
6. **E2-1 (required).** In the battle take the barrier is staged at ratio 1.0, and the test reports the look held on
   88 of 88 frames. Yet in `wp9r2_barrier_sequence_*` cells t = 0.04 to 0.76, and in battle frame 0 (t = 0.4 s), I
   see no ring at all round the shielded unit (crop at 4x round (793, 220)). The ring appears from t = 0.88, about
   when the attacker arrives. Round 1's take showed the faint arc from t = 0.16. A barrier at full that is held
   but invisible until its first hit is either a defect or a staging artefact, and either would mislead. Explain it
   with a picture or a figure, and fix it if it is a defect.

## 5. `barrier_absorb`: accept

1. **Ranged take (V1-3).** The hitscan attacker fires once (tracer at t = 1.16) and then shatters (red shards
   below, t = 1.24 to 1.64), so the take holds exactly one absorbed hit. That hit coincides with the snap-up, so
   the absorb flare alone, after the barrier is already up, is not pictured on the ranged take.
2. **Melee take.** Each slash (t = 1.16, 1.32, 1.96, 2.28, 2.76, 3.08, by the mean brightness of an 85×80 crop:
   +3 to +5 levels on those cells) brightens and thickens the whole gold ring, top included. The legacy crescent is
   a red-orange arc at the lower-right contact edge. Both are visible separately, and the flare is plainly the
   barrier answering the hit. The slash no longer blocks judging. WP11 replaces it.
3. Timing, cadence (at most one flare per 0.16 s, asserted) and colour: pass.
4. Advisory **P2-2**: the flare is modest at 1:1. A slightly whiter, hotter peak in `barrier_flash` would
   separate "absorbing" from "standing" better in a crowd and would answer the khaki cast of §4.3.

## 6. `bio_burst`: reject

Evidence: `wp9r2_deaths_sequence_*` at 2.5x to 6x, `wp9r2_node_stain_*`, `wp9r2_lab/lavapipe/bio_burst_*`.

1. **Burst moment: improved, weak but acceptable.** At t = 1.12 to 1.24 the creature's place shows a dark-red,
   wet-looking splat of about 0.6 radii, with dark specks thrown out and a translucent grey splash disc. It now
   reads as organic, not as a ghost bubble. It is modest next to the two fireballs, which is right for a smaller
   death.
2. **Stain body: fail.** In the battle (t = 2.96, 6x) and in the node-stain "mask off" panel, the stain is a
   near-round dark maroon disc. The "lobed outline" is barely lobed. Its rim is a fringe of short, 1 px wide radial
   hairs, evenly spaced round the circumference: a comb or sunburst fringe from radial edge noise. At 1:1 it reads
   as a hairy coin. It is visibly procedural and close to the "no rays/sunburst" rule.
3. **Satellite drops: fail.** They are round discs of similar size. In the lab (age 0.4 to 3.8 s) they sit at
   nearly equal distance round the centre, like dots round a flower. In the battle three of them are round coins.
4. Timing (the stain arrives with the burst and dries) and colour (dark red-brown that darkens the floor): pass.

## 7. `vehicle_wreck`: reject (regression of an accepted look)

1. **Black smoke: pass.** The smoke is now a dark charcoal cloud spreading over about three radii (t = 1.64 to
   2.60), clearly darker than a shell's. That answers P1-1.
2. **Fireball, flash, timing: still pass** (t = 1.12 to 1.36).
3. **Embers: fail.** The new embers glowing in the scorch (t = 1.64 to 2.84, 5x) are two flat, uniformly filled,
   opaque orange blobs of about half a radius. They have no hot core, no falloff, no breakup into coals, and no
   flicker, and they shrink as flat shapes. They read as orange paint or cut-out paper on the smoke, not as glow.
   This is a new cheapness tell on a look round 1 accepted.
4. The heavier dark wreckage is not separable from the black smoke at 1:1. That is acceptable. P1-2's aim, to
   read apart from a shell, is met by the smoke and by the size of the scorch.

## 8. A1-6 obstacle mask: reject

Evidence: `wp9r2_node_stain_{radv,lavapipe}.jpg`, the `decal.wgsl` and `collect_ground_obstacles` diff, and the
measurement "469 of 1137 pixels with the mask off, 0 with it on".

- The mask does keep the crystal and its base clean. But the cut is a disc of half the billboard's width × 1.05
  round the node's base point. The billboard is far wider than the visible crystal, so the stain loses a crisp
  circular bite about three times the crystal's footprint, including plain floor to the left of and below the
  crystal. The 0.15 world-unit feather is a few pixels at this zoom, so the bite's edge is a clean arc. At 1:1 it
  reads as "something round was stamped out of the splash": a new drawn-circle tell, more noticeable than the tint
  it removes.
- A ground disc is also the wrong model for a billboard. Seen at 58°, an upright billboard covers floor *behind*
  it on screen, not a disc round its foot. A disc either cuts floor the node does not cover or misses floor it does.
- Required (A2-5): spare the node by its silhouette, not by a disc. The clean way is the one `collect_ground_obstacles`'
  own doc names for units: units are not obstacles because their glass is drawn after the decals. Draw node
  billboards (and structure domes) after the decal pass as well, or let decals paint only fragments whose depth
  is the terrain's (for example a terrain-only depth or stencil). Then no hole is visible at all. If a disc mask
  stays, size it to the node's visible footprint, not its billboard, and break its edge with the stain's own
  noise.
- The A1-6 test is close to circular: it measures change inside the same disc the mask excludes, so 0 is
  guaranteed by construction. Measure the node's drawn pixels (its billboard alpha) instead, and the bare floor
  round it, which should stay painted.

## 9. Engineering

**E1-1: accept.** Each named death is counted by effect id with `spawned_of`, an exact equality per name between
the kill frame and the end (two `vehicle_wreck`, one `bio_burst`). Another unit dying with the same effect would
break the equality, so it is strict. The control with no named death peaks at 16 shards in the first 0.3 s, so
the shard query does match, and the named deaths' 0 is meaningful. Shards go to the nearest dying unit within 2.5
units, and the control stands a spacing off, so that is sound.

**E1-2: accept.** Dust is attributed by the frame before its count shows: 6 laid while the charger moves under its
status, 0 otherwise, against at least 5 charging frames. The fumes, 10 over 10.1 beats, are within ±1. It is not
asserted that the fumes stop without the status, because the stim is held on all frames. That is acceptable.

**E1-4: accept.** "Down" changes `[]` on both drivers.

**E1-6: accept.** The haze is in both `wp9r2_battle_haze_*` and `wp9r2_battle_bare_*`, in the same place, and
`smog_volume.wgsl` takes per-bubble velocity and a wake factor (`bubble_vel_boost`).

**Fog: accept.** `stance.rs` is unchanged since round 1, which accepted it. The new obstacle list reads only
`MapObjectSnapshot::visible` nodes and live battle bubbles, the objects the viewer is drawn. A mask hole therefore
marks only what is already on screen.

**radv `gpu_validation_tests`:** the clean run of 32 passed is attached. Round 1's question is closed.

## 10. The concussive slow in the shipped simulation

Confirmed from the code. `sim_core::entity::Entity::apply_concussive_slow` (`crates/sim_core/src/entity.rs:1027`,
buff id 2, 50 %, 75 ticks) has no production caller. Its only callers are `sim_core/tests/checked_movement_tests.rs`
and `effect_snapshot_tests.rs`, and now this WP's GPU test. The `concussive_shells` upgrade in
`assets/catalog/upgrades.ron` has `effects: []`. So no hit by the Marauder analogue ever slows, with or without the
research. This is a StarCraft II parity bug, but it is already recorded: B-44 (the research does nothing; the slow
is never applied) and B-77 (the unsourced, hard-coded 75 ticks) in `docs/plans/2026-11-05-sc2-deviation-fixes.md`,
scheduled for WP13d. No new bug entry is needed. The WP9 test is right to set the slow through its own function. I
changed nothing.

## Required changes

| ID | Look | Change |
|---|---|---|
| A2-1 | `concussive_daze` | Make `daze_drag` visible at gameplay zoom on this map's light floor (darker and larger, so it reads in the lab's bright default row), or drop the layer and record it. The body read stands as it is |
| A2-2 | `charge_streak` | Scuffs present at full strength within a frame or two of their lay (no 0.5 s fade-in). Dust with value contrast on light floors (a darker or shadowed dust, or a dark base under the light puff). Space the scuffs as separate gouges rather than one continuous band. Show it again on the same clear-ground take |
| A2-3 | `bio_burst` | Give the stain body a low-frequency irregular silhouette (2 to 4 large lobes or elongated drips in the throw direction), not a disc. Replace the 1 px radial hair fringe with a filtered, irregular edge. Vary the drops in size, elongation and distance |
| A2-4 | `vehicle_wreck` | Make the embers read as glow: small hot points or broken coals with a bright yellow-orange core falling off to dark red, additive or emissive, under or through the smoke, flickering as they cool. No flat filled shapes |
| A2-5 | decals | Spare nodes and structures by their drawn silhouette (draw them after decals, or paint only terrain depth) so the stain shows no circular hole. Re-take the node-stain picture, and measure the node's drawn pixels and the floor round it |
| E2-1 | `barrier_shell` | Explain why the shell held at ratio 1.0 is invisible in the battle take's first 0.8 s (sequence cells t = 0.04 to 0.76, battle frame 0), and fix it if it is a defect |

## Advisories

- P2-1: check the stim's salmon body next to a red team and next to the smog's pink wake (continues P1-3).
- P2-2: a whiter, hotter peak in `barrier_flash`. The steady shell reads khaki on the light floor.
- P2-3 (owner, outside this plan, D-9): the smog's pink wake behind every moving bubble reads as a magic trail. It
  sits exactly where the charge's and the daze's ground effects should read, and it competes with the stim's
  salmon fumes.
- The ranged absorb take holds one hit only, because the attacker shatters right after it fires. A take in which the
  ranged attacker keeps firing would picture the absorb flare apart from the snap-up.
