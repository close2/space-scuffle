# Independent Review of WP9 (Status and Death Looks), Round 3

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP9 row of
§9, §7 (quality bar), Amendments 16 to 18. Packet: [`independent_review_packet.md`](independent_review_packet.md),
section "WP9 round 3". Earlier rounds: [`independent_review_wp9_1.md`](independent_review_wp9_1.md),
[`independent_review_wp9_2.md`](independent_review_wp9_2.md). This is round 3 of at most five (Q0080). I did not
build this work, did not write rounds 1 or 2, and did not read the implementer's self-checklist before forming these
verdicts.

## Method

I read round 2's review, the packet section and Amendment 18. I read the round-3 code diff (`8a6143d..1fca057`):
`assets/catalog/effects.ron`, `assets/shaders/effects/{common,decal}.wgsl` (the exclusion loop removed),
`client::effects::render::queue_effects` (the E2-1 fix) and `queue_distortions`, the removal of the obstacle code in
`runtime.rs`, `systems.rs`, `instances.rs` and `pool.rs`, the new runtime test in `effect_runtime_tests.rs`, and the
changed assertions in `status_gpu_tests.rs`. I built and ran nothing. Every figure below is an ImageMagick crop of the
committed `wp9r3_*` pictures (nearest neighbour, 2x to 6x), compared with `wp9r2_*`, kept in `~/scratch/wp9-review3/`.
"Cell t = x" means the sequence cell with that label. I describe radv unless I say otherwise. The lavapipe pictures
and printed figures match radv: the same staging and counts, ring figures within 2 %, and the same scuffs, coals and
stain in the crops I compared (battle frames 1 and 2).

## Verdict table

| Look / item | Verdict | Main reason |
|---|---|---|
| `concussive_daze` | **accept** | Without the dust the greyed, matte body still reads as a different, drained unit, and the return to full colour when the status ends (cells t = 2.96 against 3.20 to 3.44) shows the read is the look, not lighting |
| `charge_streak` | **accept** | The trail is there while the charger moves: brown dust behind it from t = 0.20, dark scuffs from t = 0.36 to 0.40, at full strength from the frame they show, as separate short marks. Thin and faint at 1:1 (P3-1) |
| `bio_burst` | **accept** | The stain is a soft, irregular three-lobed splash with no hair fringe. The drops vary in size and shape. Lobes read slightly as separate round blobs at close zoom (P3-3) |
| `vehicle_wreck` | **accept** | The round-2 painted blobs are gone. Small orange points glow and go out in the smoke over about a second; nothing is a flat filled shape |
| A2-5 node and stain | **accept** | No hole: the stain runs whole under the node, and the node reads as standing over it. The 22 tinted node pixels are the floor seen through an additive billboard; acceptable as recorded |
| E2-1 barrier shell | **accept** | The gold ring is round the shielded unit in every cell from t = 0.04 (round 2: from 0.88). The fix is correct and cheap. Advisories E3-1, E3-2 |
| `stim_surge`, `barrier_shell`, `barrier_absorb` (accepted in round 2) | **no regression** | Catalog diff touches none of the stim's or shell's entries; the shell is unchanged in the battle cells; the whiter flash changes the lab peak by 1 to 3 levels only (P3-5) |
| Charge-dust attribution in the test | **accept** | Still discriminating: no lay more than one frame outside the charge, at least 3 lays within it |
| **Overall** | **ACCEPT** | Every required change of round 2 (A2-1 to A2-5, E2-1) is met. No required change remains; polish below is non-blocking |

## 1. `concussive_daze`: accept

Evidence: `wp9r3_daze_sequence_{radv,lavapipe}.jpg` at 3x (cells t = 0.04, 2.96, 3.20, 3.44), round 2's pair and
onset pictures (unchanged, still valid since only the drag layer was removed).

1. **Reads at gameplay zoom: pass.** The slowed unit is a dull grey-green, matte disc. Its glass highlight is
   gone and its green media are desaturated. When the status ends (74 of 88 frames shown, so it ends near t = 3.0 s)
   the same unit turns saturated, glossy green within a cell (t = 3.20 to 3.44). That contrast within one unit is the
   best evidence that the greyed body alone carries "affected and drained". "Slowed" specifically is learned from
   the unit's own slower walk next to that look, which is how StarCraft II's own concussive tint works too.
2. **Not mistaken for selected, cloaked or dead: pass** (no rim, opaque, keeps its contents and shadow).
3. **Layered:** now a single held body look plus the onset flare. With the dust dropped there is no extra layer;
   I accept this as round 2 allowed. Nothing behind the unit is a daze effect; the faint pink behind it is the
   smog's wake (P2-3, out of scope).
4. to 7.: no cheapness tell, colour right, no fog issue.

The slow is still never applied in the shipped simulation (B-44, B-77, WP13d), so players will not see this look
until that lands. That is recorded and not this WP's defect.

## 2. `charge_streak`: accept

Evidence: `wp9r3_charge_sequence_*` cells t = 0.20 to 1.20 at 3x, battle frames 0 (t = 0.4 s) and 1 (1.2 s) at 4x
against the same crops of round 2, and `wp9r3_lab/*/charge_dust_repeat_*` at 4x.

1. **Outline:** unchanged and still good (pale outline on the violet charger, cells t = 0.04 to 0.36).
2. **Trail timing: pass.** While the charger moves, a warm brown haze trails it from t = 0.20, distinct from the pink
   wake round 2 showed alone (battle frame 0, round 2 against round 3: the round-3 trail is browner and carries
   dark streaks). The first dark scuffs show at t = 0.36 to 0.40, still within the 0.4 s lunge, and are at full
   strength in the cell they first appear. A2-2's "within a frame or two of their lay" is met.
3. **Dust value contrast: pass, modest.** The dust is a mid-brown translucent haze on the pale floor, not the
   near-black the packet suggests: it is legible against the floor, but it mixes with the smog's pink wake. In the
   lab's bright row (close zoom) each puff is a soft brown cloud with a lighter rim, plausible kicked-up dirt.
4. **Separate marks: pass.** In battle frame 1 and cells t = 0.48 to 1.20 the scuffs are two or three separate dark
   strokes in a row behind the charger, with clear floor between them. They read as scrape or claw marks, not as one
   skid band. They are not speed lines: they are fixed to the ground, dark, irregular in edge, and fade in place.
5. **Weakness (non-blocking, P3-1):** each scuff is about 13 × 2 to 3 px at 1:1. They read as thin scratches rather
   than torn ground, and being collinear they form a short dashed row. Acceptable; see polish.
6. Colour and fog: no objection.

Lab note (P3-2): the repeat take lays six scuffs on one spot at random orientations, so from age 0.5 on the sheet
shows a six-armed dark asterisk. In a battle the lays never share a spot (they are laid only while moving, 0.1 s
apart), so this is a lab artefact, but the sheet now shows exactly the sunburst shape the art rules forbid. It
should not be the evidence of record for this effect.

## 3. `bio_burst`: accept

Evidence: `wp9r3_node_stain_*` (the "stain, node hidden" panel at 6x), `wp9r3_deaths_sequence_*` cells t = 1.64 to
3.44 at 4x, `wp9r3_lab/*/bio_burst_contact.jpg` close bright row at 5x.

1. **Stain silhouette: pass.** The body is now a cluster of three soft, dark maroon lobes about a smaller core, an
   irregular low-frequency shape. It is not a disc. The edge is soft and filtered: I find no 1 px radial hairs,
   comb or sunburst fringe anywhere, at 6x.
2. **Drops: pass.** Four round drops of clearly different sizes thrown to different distances, and three elongated
   drips. In the lab and the battle they no longer sit on one ring like dots round a flower.
3. **Organic read: pass.** At 1:1 it reads as a dark wet splash that darkens the floor.
4. **Weaknesses (non-blocking, P3-3):** at close zoom the three lobes can still be read as three overlapping round
   blobs (a trefoil or paw-print), and the left lobe of the battle stain has a darker inner spot that reads as a
   disc inside a disc. The drips are oriented at random rather than pointing away from the centre (the lower-left
   drip in the node shot runs across its throw direction).
5. Burst moment, timing, colour: unchanged from round 2, which passed them.

## 4. `vehicle_wreck`: accept

Evidence: `wp9r3_deaths_sequence_*` cells t = 1.36 to 3.44 at 4x, `wp9r3_lab/*/vehicle_wreck_contact.jpg` close bright
row at 4x (ages 0.48, 0.91, 1.81, 3.02, 4.53 s).

1. **Coals read as glow, not paint: pass.** In the battle, from t = 1.64 to about 2.24, small orange-yellow points
   (1 to 2 px at 1:1) glow inside and below the black smoke, thin out and are gone by t = 2.96. In the lab at
   0.9 s there are eight or so bright orange points scattered through the dark wreck, at 1.8 s a couple of dull
   ones, at 3 s none. They are points with a bright core, additive, irregularly placed; there is no flat filled
   shape. The round-2 tell is gone.
2. **Fireball, flash, black smoke:** unchanged and still pass (t = 1.12 to 2.60).
3. **Weakness (non-blocking, P3-4):** at gameplay zoom the coals are very small and few; they register as a
   sprinkle of sparks rather than as a bed of coals. Acceptable.
4. The scorch's spiky, dark edge at late ages is the scorch round 1 accepted; it is not radial hair.

## 5. A2-5: node and stain, accept

Evidence: `wp9r3_node_stain_{radv,lavapipe}.jpg` at 2x and 6x, the measurement line, the shader diff.

- The exclusion loop and the 64-byte `exclude` array are gone (`DecalInstance` back to 144 bytes, shader struct
  matching). No stain has a hole. In "stain, as shipped" the stain runs continuously under and round the node.
- **Draw order: pass.** The node's green crystal and orange sparkles are drawn over the stain and keep their full
  brightness ("its own light is dimmed on 0 of them"). It reads as standing on the splash.
- **The 22 tinted pixels: acceptable as recorded.** They are 12 % of the node's 183 drawn pixels, all in the faint,
  translucent lower part of the billboard where the floor shows through by design. At 1:1 I cannot see them as a
  defect: a node standing in a splash shows splash through its translucent base, which is physically plausible.
  Covering them needs an alpha-blended node material, rightly outside this plan.
- **The test now measures the right thing:** the node's own light (scene with against without its billboard) over
  stained and bare floor, and 946 stained floor pixels round it. It is no longer circular.

## 6. E2-1: the barrier shell, accept

Evidence: `wp9r3_barrier_sequence_*` (every frame of the first 1.2 s) at 2x against `wp9r2_barrier_sequence_radv.jpg`,
the measurement line, `render.rs` lines 680 to 730.

- **The picture:** the soft gold ring stands round the shielded unit in every cell from t = 0.04 on both drivers.
  In round 2 the same cells (t = 0.04 to 0.76) show the bare unit. The defect is fixed.
- **The code: correct.** `queue_effects` now calls `SpecializedRenderPipelines::specialize` for the four
  `(Cards | Decals) × darken` keys every frame before any early return on empty card or decal counts. `specialize`
  is a lookup in the specialization cache that queues the pipeline for asynchronous compilation the first time and
  returns the cached id after, so the pipelines compile before the first effect needs them. The later calls for the
  actual items use the same keys and hit the cache.
- **Cost: negligible.** Four hash lookups of a 4-field key per frame; no allocation after the first frame, no GPU
  work. The `PERFORMANCE RATIONALE` block is correct in Why, What and Trade-off.
- **Risk: low.** If MSAA or HDR changes, new keys compile once, as they would anyway. The residual risks are the
  advisories E3-1 and E3-2 below; neither is a WP9 defect.
- **The new runtime test** (`a_status_look_held_settled_draws_as_one_that_came_in`) is sound: it proves the
  settled hold draws at the same intensity as the one that came in, which rules out the other suspected cause.

## 7. Accepted looks: no regression

- `stim_surge`: no line of the stim's look, ramp or fumes changed in the catalog diff, and the measurements match
  round 2 (rim 0.252 to 0.620, 10 fumes over 10.1 beats). Not re-pictured, which is right.
- `barrier_shell`: the ring figures match round 2 within 1 % (358 / 351 changed pixels at 0.5 / 0.05, 202 at 24
  levels), "down" changes no pixel. The battle cells show the same ring as round 2's pose.
- `barrier_absorb`: the `barrier_flash` ramp's top is whiter (`(1.00, 1.00, 0.94)` at 0.75). It is no regression:
  the flash stays gold-white and soft. But its effect is tiny: the brightest pixel of the lab's flare cell rises from
  (222, 206, 181) to (224, 207, 182) on lavapipe (P3-5).

## 8. Engineering: the charge-dust attribution

The rule changed from "the frame before the count shows charges" to "either frame of the pair charges". This widens
the window by one 40 ms frame at each end of the lunge. It is still meaningful:

- `laid_otherwise == 0` still fails on any dust laid more than one frame before the charge starts or after it ends,
  and with lays 0.1 s apart a leaking emitter (one that kept laying after the status cleared, or laid without moving)
  would put a lay outside that window.
- `laid_charging >= 3` against the expected 4 (0.4 s at 0.1 s) still proves the emitter fires during the charge.
- The justification (the sample and the emission are not ordered against each other in the schedule) is plausible
  and matches the lavapipe observation the packet records.

What it can no longer catch is a single stray lay in the one frame right after the status clears. That is an
acceptable loss.

## Required changes

None. Round 4 is not needed for WP9 unless the owner wants the polish below taken.

## Non-blocking polish and advisories

| ID | Look / area | Suggestion |
|---|---|---|
| P3-1 | `charge_streak` | Scuffs are thin (about 2 to 3 px at 1:1) and collinear. Widen them (width about 0.6), add a small lateral and angular jitter per lay so they read as torn ground rather than a dashed row, and optionally a few dark clods at each lay |
| P3-2 | lab, `charge_dust` | The repeat take stacks six randomly rotated scuffs on one spot into an asterisk. Offset the repeat lays along a line (as a moving charger lays them) so the sheet represents the battle and shows no star |
| P3-3 | `bio_burst` | Overlap the three lobes more and vary their sizes so the cluster does not read as three circles; remove the darker inner spot of a lobe; orient drips along their throw direction |
| P3-4 | `vehicle_wreck` | Coals could be a little larger and more numerous near the wreck's centre so they read as a bed of coals at gameplay zoom |
| P3-5 | `barrier_absorb` | P2-2's whiter peak changes the lab peak by 1 to 3 levels. If the aim is to separate "absorbing" from "standing" in a crowd, raise the flash's opacity or size, not only its top colour |
| E3-1 | `render.rs` | `queue_distortions` still asks for the distortion pipeline only when distortions exist, the same first-use gap E2-1 fixed for cards and decals (heat haze of explosions and the WP8 shimmer can go undrawn in a session's first second). Warm it in the same loop. Also note that the warming starts only once a 3D view with a depth prepass exists, so the very first frames of the first battle view still compile; a warm-up during scene load would close that |
| E3-2 | `status_gpu_tests.rs` | The printed shell-card samples are 0 at t = 0.04 and 0.08, yet the ring is visible in those cells. The sample and picture timing evidently differ by up to two frames; say which in the test's doc. The assertion `first_shell <= 4` is card-side only; a pixel check of the ring in the first few frames (the ring metric already exists) would assert what E2-1 was about |
| P2-1, P2-3 | carried | The stim next to a red team, and the smog's pink wake behind moving bubbles (owner, D-9), remain open as recorded in Amendment 18 |
