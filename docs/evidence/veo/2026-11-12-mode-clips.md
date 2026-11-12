# Veo: mode and morph clips of the sieged tank and the burrowed scorpion (2026-11-12)

Evidence for WP13 of [`2026-11-09-effect-visualization.md`](../../plans/2026-11-09-effect-visualization.md)
(Amendment 10), approved by the owner in [Q0075](../../questions/closed/0075-veo-morph-and-mode-clips.md): up to
$20.00, target under $10.00. Run on 2026-11-12 (UTC) from `main` at `1d258fd`, branch `effects-wp13`.

**Result: both catalog modes that the shipped units have now show their own clips.** The Siege-Tank analogue
`railgun_crawler` in `siege_mode` and the Lurker analogue `burrow_scorpion` in `burrowed` each have a mode atlas
(`<slug>-<mode>`, 512 px, 8 frames per slice) with an Idle and an Attack in the mode and the two morphs, Enter and
Leave. **11 paid generations, $4.40; the ledger total went from $106.45 to $110.85** of the $200.00 limit. 8 clips
were accepted, 3 rejected (each rejection is a `mode_clip_rejected` line in the ledger, with its reason).

## Method

- **Prompts from data.** The `modes` section of [`media_style.ron`](../../../assets/catalog/media_style.ron) holds
  one text per slice and the loop treatment of the mode's states; a clip's prompt is the unit's look (its Idle
  `veo_prompts` entry, or the mode's own `look`), the slice's text, then the house style. Every prompt passed the
  prompt rules (deny list, placeholders, 1024 characters) before it was paid for; `mode-veo --dry-run` prints it.
- **Pictures chain the clips.** The enter morph starts on the unit's Idle picture
  (`assets/ai_keyframes/<slug>_idle.png`); its last frame (3.95 s) became the mode's picture
  (`assets/ai_keyframes/<slug>-<mode>.png`); every state in the mode starts on that picture and, pinned, ends on it
  (`lastFrame`); the leave morph starts on it and ends on the Idle picture. A regenerated enter morph is pinned to the
  existing mode picture, so the clips made from it stay valid. The chain sheets below show the joins.
- **Review** of every download on a 12-frame contact sheet (3 fps, below) against the written criteria of the Veo
  plan (camera locked, no rays or stripes behind the subject, no change of identity, loops that do not jump, the
  action readable), then `install-mode-clip`, which accepts only bytes the ledger downloaded for exactly that slice.
- **Bake** with `bake-mode-videos`, which takes every slice the mode needs from `assets/videos/<slug>-<mode>_<slice>.mp4`
  and writes the atlas, its manifest and a provenance record whose origins point at the `mode_clip_downloaded` lines.

## Accept / reject log

| Atlas | Slice | Download | Verdict | Reason |
|---|---|---|---|---|
| `railgun_crawler-siege_mode` | enter | `5cc19947` ([sheet](modes/railgun_crawler-siege_mode_enter_5cc19947.jpg)) | **reject** | The legs deploy and the rails tilt up, but from 1.7 s the rails discharge continuous lightning out of frame, so the last frame (the mode's picture) would show a weapon firing. Prompt: "glowing steadily blue. Nothing fires: no sparks, no lightning, no discharge". |
| `railgun_crawler-siege_mode` | enter | `f589171c` ([sheet](modes/railgun_crawler-siege_mode_enter_f589171c.jpg)) | **accept** | Four stabilizer legs swing out and brace, the rail barrels rise to a steep angle; camera locked; ends still. |
| `railgun_crawler-siege_mode` | idle | `1868ff1e` ([sheet](modes/railgun_crawler-siege_mode_idle_1868ff1e.jpg)) | **accept** | Anchored, still; only the rail glow and the stack smoke move. Loop: start fidelity 0.995, wrap 0.951 against a mean step of 0.928, so the seam does not jump ([frames](modes/railgun_crawler-siege_mode_idle_loop_frames.png), [seam](modes/railgun_crawler-siege_mode_idle_loop_seam.png)). `loop-check` flags its worst inner step (0.877, smoke) against its 0.03 margin; accepted as for the default Idle's smoke. |
| `railgun_crawler-siege_mode` | attack | `4ce579c9` ([sheet](modes/railgun_crawler-siege_mode_attack_4ce579c9.jpg)) | **reject** | A strong blast, but the raised rails drop to level to fire and climb back, so a sieged cannon changes pose every shot; a shell flies before the flash. Prompt: "the twin rail barrels staying raised at their steep angle throughout … up into the sky". |
| `railgun_crawler-siege_mode` | attack | `75b8dfad` ([sheet](modes/railgun_crawler-siege_mode_attack_75b8dfad.jpg)) | **accept** | A heavy blue muzzle blast with the barrels raised, a kick and smoke, then the barrels settle back to the start pose (pinned). |
| `railgun_crawler-siege_mode` | leave | `8730b2d6` ([sheet](modes/railgun_crawler-siege_mode_leave_8730b2d6.jpg)) | **accept** | The barrels lower level, the legs fold into the hull; ends on the Idle picture. |
| `burrow_scorpion-burrowed` | enter | `a40d56ac` ([sheet](modes/burrow_scorpion-burrowed_enter_a40d56ac.jpg)) | **reject** | Reads as digging in and ends on a good burrowed picture (kept as the mode's picture), but from 1.5 s to 2.7 s both claws detach and lie loose on the mound. Prompt: "staying one whole, intact body with every leg and claw attached". |
| `burrow_scorpion-burrowed` | enter | `7581379d` ([sheet](modes/burrow_scorpion-burrowed_enter_7581379d.jpg)) | **accept** | Digs down into a growing mound of dark earth, ending on the mode's picture (pinned). A claw rests apart from the body for a moment near 2.3 s; at 8 frames over the 1.32 s channel it is one frame, accepted. |
| `burrow_scorpion-burrowed` | idle | `f4e82c3c` ([sheet](modes/burrow_scorpion-burrowed_idle_f4e82c3c.jpg)) | **accept** | The mound with the glowing back and tail; one claw lifts out and settles back. Loop: start fidelity 0.990, wrap 0.991, so the seam does not jump ([frames](modes/burrow_scorpion-burrowed_idle_loop_frames.png), [seam](modes/burrow_scorpion-burrowed_idle_loop_seam.png)). The moving claw reads as a zoom of +6 % to the scale lock, which refuses it, so the mode's Idle is baked without a scale lock (`media_style.ron`). |
| `burrow_scorpion-burrowed` | attack | `6a021898` ([sheet](modes/burrow_scorpion-burrowed_attack_6a021898.jpg)) | **accept** | The mound heaves and a crown of bone spikes bursts up and sinks away; the spikes rise round the mound rather than in a line to the right (the line itself is the `spine_line` effect in the world). Ends on the mode's picture (pinned). |
| `burrow_scorpion-burrowed` | leave | `69f7b85b` ([sheet](modes/burrow_scorpion-burrowed_leave_69f7b85b.jpg)) | **accept** | The mound bursts open, the scorpion heaves out and the soil vanishes; ends on the Idle picture. |

Chain sheets (enter's last frame, Idle's first and last, Attack's first and last, leave's first and last, the Idle
picture): [`railgun_crawler-siege_mode_chain.jpg`](modes/railgun_crawler-siege_mode_chain.jpg),
[`burrow_scorpion-burrowed_chain.jpg`](modes/burrow_scorpion-burrowed_chain.jpg).

The loop measurements were taken with `loop-check` on the default atlas's budget of 10 frames (the mode atlas keeps 8)
and, for the scorpion, with the default Idle's scale lock; they judge the seam, which the frame count does not change.

## Spend

| | Generations | Cost |
|---|---|---|
| Ledger before (Veo unit and effect clips, paid image requests) | 251 Veo, 55 paid images | $106.45 |
| WP13 mode clips (`mode_clip_request_started`, `veo-3.1-fast-generate-001`, $0.40) | 11 | $4.40 |
| **Ledger after** | | **$110.85 of $200.00** |

The stop rule of Q0075 (target under $10.00) was not reached; no further retry was judged worth its cost.

## In battle

Written by `crates/client/tests/mode_atlas_gpu_tests.rs` with `SPACE_SCUFFLE_MODE_EVIDENCE_DIR` on the physical
GPU (radv, 2026-11-12): the shipped skirmish with one unit of the viewer for each mode atlas, ordered into its mode
through the production command queue. Shots: [default](modes/battle/battle_default.jpg),
[morphing](modes/battle/battle_morphing.jpg) (half way through the first channel),
[settled](modes/battle/battle_settled.jpg); enlarged crops, default | morphing | settled:
[sieged tank](modes/battle/battle_railgun_crawler-siege_mode.png),
[burrowed scorpion](modes/battle/battle_burrow_scorpion-burrowed.png).

The test checks that each bubble binds its mode atlas's texture while it morphs and once it settles, and that the
settled stance changed the unit's patch (tank 955 of 5041 pixels, scorpion 2596 of 5041). The tank is readable as
deploying (legs out, rails raised). The settled burrowed scorpion is drawn under the client's existing concealment
shimmer for an own burrowed unit, so its mound is faint at the default zoom; how a burrowed own unit is drawn is
WP8's (concealment and stance visuals), not this package's.

## Reframe of the sieged tank (2026-11-12, after the independent review)

The [independent review of WP13](modes/independent_review_wp13.md) accepted all eight clips. As defect 1 it
recorded that the raised rails and the Attack blast were cut by the top edge of the source frame: Attack frames
10–13, and a muzzle rim in the sieged rest pose. Defects 2 and 3 were the scorpion Enter's detached claw (one atlas
frame) and its greyer bar. Plan amendment: [Amendment 16](../../plans/2026-11-09-effect-visualization.md). Run from
branch `wp13-reframe`.

**Why a prompt alone could not fix it.** The enter morph must start on the unit's Idle picture, so that the default
atlas flows into it without a jump. Veo keeps the first frame's framing, and the camera may not move. So the tank
could not be "placed lower" by asking. **What changed instead:**

- **A reframed start picture.** `asset_baker reframe-mode-picture` shrinks the Idle picture to 70 % about its centre
  and continues its own backdrop into the margin. Next to the picture, the margin takes the colour of the picture's
  nearest edge, averaged over 40 px cells. It fades to the measured border colour at the canvas edge. No panel and no
  line are left for Veo to keep. The picture is
  [`railgun_crawler-siege_mode_reframed_idle.png`](../../../assets/ai_keyframes/railgun_crawler-siege_mode_reframed_idle.png),
  and the comparison sheet is [here](modes/railgun_crawler-siege_mode_reframe_pictures.jpg): Idle picture, reframed
  picture, new sieged picture. A test proves that the committed picture is what the command writes.
- **The bake undoes the shrink.** The mode's `reframe` in `media_style.ron` bakes its clips as
  `ReframedLetterbox(feather_permille: 30, scale_permille: 700)`. That means: crop the centre 70 % of the width over
  the whole height, then letterbox as before. The tank is back at the default atlas's scale and place. The margin
  above and below stays in the square, where a default atlas has its bars, so the raised rails and the blast have
  room. The provenance record names this framing. `framing_tests` proves the round trip with real FFmpeg: a subject
  baked through the reframe lands within 1 px of its default bake, and a part above the default frame is kept.
- **All four tank clips regenerated** from the new chain: reframed Idle picture → Enter → new sieged picture →
  Idle, Attack, Leave → reframed Idle picture. The old installed clips were each recorded as `mode_clip_rejected`,
  "superseded after acceptance", because the ledger refuses to regenerate a slice whose download was never
  rejected. The prompts changed in two places. Enter drops "extend longer" and adds "the muzzles inside the frame".
  Attack drops the shell "streaking out of the top of the frame" and asks for barrels "locked at their steep upward
  angle the whole time" and a "compact" flash "that stays inside the frame".

| Atlas | Slice | Download | Verdict | Reason |
|---|---|---|---|---|
| `railgun_crawler-siege_mode` | enter | `f589171c` | **superseded** | Accepted before; its last frame (the sieged picture) has the rails touching the top edge (review defect 1). |
| `railgun_crawler-siege_mode` | idle | `1868ff1e` | **superseded** | Starts and ends on the old sieged picture. |
| `railgun_crawler-siege_mode` | attack | `75b8dfad` | **superseded** | The blast is clipped flat by the top edge, and the rails run out of the frame for three frames (review defect 1). |
| `railgun_crawler-siege_mode` | leave | `8730b2d6` | **superseded** | Starts on the old sieged picture. |
| `railgun_crawler-siege_mode` | enter | `a0d4247d` ([sheet](modes/railgun_crawler-siege_mode_enter_a0d4247d.jpg)) | **accept** | From the reframed picture, the four stabilizer legs swing out and brace on broad feet, and the rails rise to a steep angle with their muzzles about 40 px below the top of the source frame. The camera is locked and nothing fires. The hull rises slightly on the legs, and both smoke plumes fade out (an engine state; the review accepted one plume fading before). The feet are larger than in the previous clip, which helps review defect 4. |
| `railgun_crawler-siege_mode` | idle | `d554ddc2` ([sheet](modes/railgun_crawler-siege_mode_idle_d554ddc2.jpg)) | **accept** | Still on its legs; a smoke puff rises and fades from the stacks. Loop: start fidelity 0.995, wrap 0.980 against a mean step of 0.955, so the seam does not jump ([frames](modes/railgun_crawler-siege_mode_idle_loop_frames_reframed.png), [seam](modes/railgun_crawler-siege_mode_idle_loop_seam_reframed.png)). As before, `loop-check` flags its worst inner step (0.877, smoke) against its 0.03 margin, accepted on the same grounds as `1868ff1e`. |
| `railgun_crawler-siege_mode` | attack | `057e9373` ([sheet](modes/railgun_crawler-siege_mode_attack_057e9373.jpg)) | **reject** | From 1.0 s to 2.3 s the rails drop to level and climb back (the fault of `4ce579c9`). The first blast is orange-white and touches the top edge. |
| `railgun_crawler-siege_mode` | attack | `55e0ba27` ([sheet](modes/railgun_crawler-siege_mode_attack_55e0ba27.jpg)) | **accept** | The rails stay raised throughout. A blue-white blast blooms at the muzzles, then a smoke puff, then it settles on the pinned start pose. The round bloom stays inside the source frame, but the blast's upper flame tongue does not. In clip frames 23–31 (0.96–1.29 s, about 0.4 s) the top source rows are bright, and full white in frames 27–30. The sampled blast frame (atlas Attack frame 10, clip frame 26) has the tongue leaving the top edge, about 11 px wide at source row 0 and 22 px wide at row 10. The bake's 15 px bar feather fades it into the bar colour, so it reads as a soft tip rather than a flat cut. It cannot be seen at battle size, and no regeneration is asked for (independent review of the reframe, item 1). |
| `railgun_crawler-siege_mode` | leave | `7e984d36` ([sheet](modes/railgun_crawler-siege_mode_leave_7e984d36.jpg)) | **accept** | The rails lower to level, the legs fold into the hull, both plumes return, and it ends on the reframed Idle picture. The camera is locked. |
| `burrow_scorpion-burrowed` | enter | `7581379d` | **kept; rejection withdrawn** | A `mode_clip_rejected` line was recorded to allow a regeneration (review defects 2 and 3). Neither retry was better, so it **stays installed and shipped**. A later `mode_clip_rejection_withdrawn` line (written by `mode-withdraw-rejection`, free) withdraws that rejection, so the ledger says it ships. The media audit now fails a shipped mode clip whose rejection stands. Its atlas re-bakes byte for byte. |
| `burrow_scorpion-burrowed` | enter | `21ebf9b8` ([sheet](modes/burrow_scorpion-burrowed_enter_21ebf9b8.jpg)) | **reject** | From 2.0 s to 2.7 s both claws lie loose on the mound (the fault of `a40d56ac`), and at 1.0 s a dark blob of dirt hides half the body. |
| `burrow_scorpion-burrowed` | enter | `4275320e` ([sheet](modes/burrow_scorpion-burrowed_enter_4275320e.jpg)) | **reject** | Near 1.3 s a claw lies on the mound below the body with no arm, and the other sits apart at the mound's edge. This is no better than `7581379d`, so retries stop here. |

**The atlas.** [Whole atlas](modes/railgun_crawler-siege_mode_atlas_reframed.jpg) (rows: Idle, Attack, Enter,
Leave); [old against new](modes/railgun_crawler-siege_mode_old_vs_reframed.jpg) (top: the old atlas's sieged rest
pose and Attack frames 9–13; bottom: the new ones). [Chain](modes/railgun_crawler-siege_mode_chain_reframed.jpg):
default Idle frame 0, Enter 16 and 23, Idle 0 and 7, Attack 8 and 15, Leave 24 and 31, then the default Idle frame
0 again.

Join RMSE (normalised 0–1, `compare -metric RMSE`, atlas frames counted from 0):

| Join | RMSE | Compared with |
|---|---|---|
| default Idle 0 → Enter 16 | 0.030 | a default Idle step, 0.024. The margin above and below now shows backdrop where the default draws bars, which is the same colour and scale. |
| Enter 23 → Idle 0 | 0.0075 | |
| Idle 7 → Idle 0 (wrap) | 0.017 | Idle inner steps 0.003–0.080 |
| Idle 0 → Attack 8, Attack 15 → Idle 0 | 0.0063, 0.042 | |
| Idle 0 → Leave 24 | 0.0065 | |
| Leave 31 → default Idle 0 | 0.046 | Leave inner steps 0.005–0.098; the old Leave had 0.069 |

**Spend.** 7 generations (`mode_clip_request_started`, `veo-3.1-fast-generate-001`, $0.40 each): tank 5 (Enter,
Idle, Attack twice, Leave), scorpion Enter 2. **$2.80; the ledger went from $110.85 to $113.65** of the $200.00
limit. WP13's mode clips total $7.20 of Q0075's $20.00. 11 `mode_clip_rejected` lines in all: 3 from before, 4
tank supersessions, 1 scorpion supersession, and 3 new rejections. The new rejections are counted in the 7
generations above. One `mode_clip_rejection_withdrawn` line withdraws the scorpion supersession; it is not a
request and costs nothing.

**Defects after this round.** Defect 1 is fixed for the raised rails and the bloom: in every frame the muzzles sit
about 38 source px below the top edge, and the bloom is whole. A minor residual remains at the blast tip. For
about 0.4 s (clip frames 23–31) a narrow flame tongue, about 11 px wide at the edge, still leaves the top of the
source frame, and the sampled blast frame shows it. The bake's edge fade softens it to a soft tip, and it is
invisible at battle size. Defects 2 and 3 (the scorpion Enter) remain as the review
recorded them, after two failed paid retries. Defect 4 (feet legibility) is improved by the larger feet of
`a0d4247d`. Defect 5 belongs to WP8.

**Records after the review of the reframe.** The
[independent review of the reframe](modes/independent_review_reframe.md) accepted the art and asked for record
fixes only. These were made, with no paid generation (plan Amendment 16, "Records"):

- The ledger withdraws the scorpion `7581379d` rejection (`mode_clip_rejection_withdrawn`).
- The media audit checks every shipped mode atlas and fails one whose clip has a standing rejection.
- The reframe's picture blend is renamed `picture_feather_permille`, and the tank atlas's provenance records it
  (`bake.reframe`).
- The text above is corrected about the blast tongue and the count of rejection lines.

The backdrop average that runs cut smoke into faint haze columns in the margin is recorded as polish. Fixing it
would change the shipped start picture.

**In battle (reframed tank).** Written by `crates/client/tests/mode_atlas_gpu_tests.rs` with
`SPACE_SCUFFLE_MODE_EVIDENCE_DIR`, through the test harness's GPU run (2026-11-12). The test passes: each bubble
binds its mode atlas while morphing and settled, and the settled stance changes the patch. Shots:
[default](modes/battle/battle_default_reframed.jpg), [morphing](modes/battle/battle_morphing_reframed.jpg),
[settled](modes/battle/battle_settled_reframed.jpg). Enlarged crop of the sieged tank, default | morphing |
settled: [here](modes/battle/battle_railgun_crawler-siege_mode_reframed.png). The settled bubble shows the raised
rails whole, with no cut at the top. The earlier shots stay beside these for comparison.
