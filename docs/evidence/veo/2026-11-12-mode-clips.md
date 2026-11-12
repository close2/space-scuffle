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
