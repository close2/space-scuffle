# Independent review of the WP13 reframe (sieged tank)

Plan: [`2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), Amendment 16.
Defect under repair: item 1 of the [independent review of WP13](independent_review_wp13.md). Implementer's
write-up: [`2026-11-12-mode-clips.md`](../2026-11-12-mode-clips.md), section "Reframe of the sieged tank". Branch
`wp13-reframe`, head `77b77f6`.

## How this review was done

I looked at the pictures before I read the write-up. That means the reframe sheets (`*_reframed*`,
`_old_vs_reframed`, `_reframe_pictures`), the five new download contact sheets, the battle shots and crop, and the
committed start picture `assets/ai_keyframes/railgun_crawler-siege_mode_reframed_idle.png`. I then cut the new
mode atlas `assets/textures/railgun_crawler-siege_mode.png` and the default atlas `railgun_crawler.png` into single
512 px frames with ImageMagick. I also decoded the four installed clips (`assets/videos/railgun_crawler-siege_mode_*.mp4`)
with FFmpeg. I checked:

- the topmost bright row of every atlas frame, and the brightness of the top rows of every frame of the Attack clip;
- RMSE (`compare -metric RMSE`, normalised 0–1) at the joins, and difference images;
- sharpness (standard deviation of a Laplacian over the hull) of default and reframed frames;
- the start picture with its contrast stretched, and with an edge filter, to look for seams.

Atlas frames count from 0: Idle 0–7, Attack 8–15, Enter 16–23, Leave 24–31. Nothing was built, run or generated.
The crops are in `~/scratch/reframe-review/`, which is not committed.

**Verdict: REJECT as it stands. The art is accepted.** The four new tank clips and the atlas fix the cut of the
raised rails, keep the tank's identity, scale and place, and join without jumps. Three things stop the merge, and
none needs a new generation:

1. The ledger now says the shipped scorpion Enter is rejected, and nothing in it says otherwise (item 3).
2. The write-up says the sampled Attack blast is whole with room above. It is not quite: a narrow flame tongue
   still runs off the top of the source frame in the sampled frame (item 1).
3. The write-up's count of `mode_clip_rejected` lines is wrong (12 claimed, 11 in the ledger).

## 1. Rails, blast, identity, scale and joins

**Rails: fixed.** The source frame starts at atlas row 50 (a 896×720 crop letterboxed to 512×411). In every Idle,
Enter end and Leave start frame, the first bright row is 72–73, so the muzzles sit about 22 atlas px (about 38 source
px) below the top of the source frame. In the old atlas the muzzles touched it. The settled battle crop shows the
rails whole.

**Blast: much better, not wholly inside.** In atlas frame 10, the blast frame, the main bloom is whole and round. In
the old atlas it was cut flat across its full width. But the Attack clip (`55e0ba27`) has bright pixels in its top
6 source rows in frames 23–31 (0.96–1.29 s), and full white there in frames 27–30. That is about 0.4 s, not "about
0.15 s" of "two thin spark tips". Atlas frame 10 matches clip frame 26 (RMSE 0.013; the neighbours are 0.039 and
0.045). In frame 26, the blast's upper flame tongue leaves the top of the frame: about 11 px wide in source row 0,
22 px wide at row 10. In the atlas, the 15 px bar feather fades that tongue into the bar colour (rows 50–62 brighten
from 0.31 to 0.86). So it reads as a soft tip rather than a hard cut. At bubble size it cannot be seen, and I do not
ask for a regeneration. The write-up's sentence "its blast frame (Attack frame 10) is whole, with room above" is
wrong, and must be corrected. A minor note: about 6 blast pixels in frame 10 also fall outside the inscribed circle
of the bubble.

**Identity, palette, scale and place: consistent.** Default Idle frame 0 and Enter frame 16 overlay with only a thin,
even outline in their difference image. A scale mismatch would make the outline grow with distance from the centre,
and it does not. Hull, tracks and turret land on the same pixels: hull x 65–445 and track bottom about row 362 in
both. The Leave end (frame 31) lands in the same place. Sharpness is unchanged (Laplacian standard deviation 0.211
default, 0.205–0.223 reframed). The 896 px crop still holds more than the 512 px it is scaled to, so the shrink-and-
crop round trip has cost no visible detail. The palette is the same grey hull, blue rails and amber lights. The bar
colours are (59–60, 66–67, 87–88), and the default's bars are the same.

**Joins and Idle loop: no visible jump.** My RMSEs agree with the write-up's: default Idle 0 → Enter 16 is 0.030,
and Leave 31 → default Idle 0 is 0.046, inside the Leave's own steps. The Idle wrap is clean on the seam sheet. The
one visible difference at the morph joins is in the band above the hull. The default draws a flat bar down to row
112, and the smoke is cut there. The mode atlas shows backdrop, with soft smoke haze rising to about row 60. In the
top 105 rows the RMSE is 0.021–0.023: visible only with the contrast stretched, and not at all in battle. This
meets the owner's no-jump rule.

## 2. The reframed start picture

**Seamless enough, with one artifact.** An edge filter finds no line or panel edge at the shrunk picture's border
(x 192 and 1088, y 108 and 612). With the contrast stretched hard, two things show:

- The margin fades out to the measured border colour, which is lighter than the vignetted backdrop next to the tank.
  This gives a soft, light frame around a darker centre. Veo copied it into all four clips. The bake removes the
  left and right margins, and the top and bottom read like the default bars. Acceptable.
- The cell-averaged backdrop runs the two smoke plumes, which the Idle picture cut at its top edge, upward into
  the margin as faint vertical haze columns. Enter frame 16 still shows a trace of this haze. It is harmless, but a
  backdrop average that ignores bright, low-saturation outliers (smoke) would remove it in a future reframe.

**Legitimate Veo input: yes.** The picture is a deterministic geometric transform of the unit's own Veo/AI Idle
picture: a Lanczos shrink plus a backdrop extension, with no drawn content. It is used only as Veo's start (and,
for Leave, end) image. Every frame in the atlas is decoded from a real Veo download. Each download has its operation
id, a ledger download line and a SHA-256 in the provenance record. The Enter's `keyframe_sha256` (`e95f80ac…`) is
the committed reframed picture. The Idle, Attack and Leave records name the new sieged picture (`6e06874a…`).
Nothing procedural reaches `assets/textures/`, and the picture is a mechanical input, not a stand-in for generated
media. This is within charter §1.2. A test proves that the committed picture is what the command writes.

## 3. Ledger integrity

The four tank supersessions are honest. Each `mode_clip_rejected` line says "Superseded after acceptance" and names
the review item, and none of those clips ships any more.

**The scorpion record is misleading.** Ledger line 1415 records `mode_clip_rejected` for
`08e60832…` (`7581379d`). Its reason, "Superseded after acceptance … near 2.3 s the right claw lies beyond the
mound …", reads as a final rejection. But the shipped `assets/textures/burrow_scorpion-burrowed.provenance.ron`
still names exactly that clip (`clip_sha256: "08e60832…"`, `ledger_line: 1387`). No later line withdraws the
rejection. The only hint is the free-text reason of a different clip (`46b20237…`): "no better than the shipped
7581379d, which stays installed". Consequences:

- A reader or tool that takes the ledger as the source of truth sees a shipped clip marked rejected.
  `LedgerHistory` puts `08e60832…` into `rejected_sha256`. From now on, `install-mode-clip` refuses to reinstall the
  clip that ships (`downloaded_sha256` in `mode_cli.rs` returns "was rejected in review"), so the shipped asset can
  no longer be reproduced through the tool's own path.
- The media audit in `crates/asset_baker/src/provenance.rs` (around line 1354) flags a shipped clip that has a
  `RejectedInReview` line, but it does not look at `ModeClipRejected`. That is why nothing turned red. The same
  state on a default atlas would be reported as `UnverifiedReason::RejectedInReview`.

The root cause is that the tool takes "rejected" as its only way to allow a regeneration. The supersession lines of
the tank used it honestly, because those clips were replaced. The scorpion's use was not, because the retries failed.

**Required fix (one of these, the first preferred):**

- (a) Add a ledger event that withdraws a rejection, for example `mode_clip_rejection_withdrawn { slug, stance,
  slot, sha256, reason }`, which `LedgerHistory` applies by removing the SHA-256 from `rejected_sha256`. Append one
  for `08e60832…` ("retries 21ebf9b8 and 4275320e were worse; 7581379d stays shipped"). Better still, make
  regeneration permission a separate event (`mode_clip_retry_opened`) so that "rejected" means only rejected.
- (b) In either case, extend the provenance audit to `ModeClipRejected` for mode atlases, and add a test, so a
  shipped clip with an unwithdrawn rejection is reported as it is for default atlases.

If (a) is judged too large for this branch, the minimum acceptable change is (b) plus an owner question recording
the inconsistency. The write-up's table row "kept, rejection line recorded" is honest. The ledger alone is not.

## 4. Code

`crates/asset_baker/src/framing.rs`, `mode_media.rs` and `mode_cli.rs` are sound.

- **Validation.** `Reframe::issue` enforces 500–999 per mille and the feather limit when the style loads.
  `FramingPolicy` refuses `ReframedLetterbox` as an identity framing. `BakeFraming::reframed` returns `None` for a
  centre crop and for an already reframed framing. `reframe_picture` validates again, and it refuses a source that
  would keep no pixel or no margin. `reframed_side` rounds down to an even size, and the picture and the FFmpeg crop
  (`trunc(iw*700/2000)*2`) both use it, so the inverse is exact.
- **Determinism.** `reframe_picture` is pure integer arithmetic over the `image` crate's resizes. The test
  `every_shipped_reframed_picture_is_what_reframe_mode_picture_writes` regenerates the shipped picture through the
  real CLI and compares it. It also checks that `--replace` is required. `framing_tests` proves the round trip with
  real FFmpeg: within 1 px of the default bake, and keeping the part above the default frame.
- **Refusal before spend.** `load()` in `mode_cli.rs` runs `check_framing` for every mode command, including
  `mode-veo`. `plan_mode_pictures` refuses a reframed morph whose reframed picture is missing. The test
  `a_reframe_of_a_centre_cropped_unit_is_refused_before_any_spend` exercises this through `mode-veo --dry-run`.
  Good.
- **Naming (should fix, not blocking).** `media_style.ron` has `reframe: (scale_permille: 700, feather_permille: 60)`,
  and the provenance has `ReframedLetterbox(feather_permille: 30, …)`. These are two different feathers: the
  picture's blend into the margin, and the bake's bar feather inherited from the unit's letterbox. They share one
  name, and the 60 is recorded nowhere in the provenance. Rename the reframe's field (for example
  `picture_feather_permille`), or say in the `media_style.ron` comment and in the `Reframe` docs that it is not the
  bake feather.

## Required changes

1. Fix the ledger's scorpion record as described in item 3: (a) a withdrawal event plus (b) audit coverage, or at
   minimum (b) plus an owner question.
2. Correct the `55e0ba27` row and the "Defects after this round" paragraph of the write-up. The sampled blast
   frame (clip frame 26) has a narrow flame tongue leaving the top edge, which the bar feather softens. The top
   rows are bright for frames 23–31, not about 0.15 s. Defect 1 is fixed for the rails and the bloom, and remains
   as a minor, invisible-in-battle residual at the blast tip.
3. Correct "12 `mode_clip_rejected` lines in all" to 11 (3 before, 4 tank supersessions, 1 scorpion, 3 new
   rejections).

Should fix, not blocking: the feather naming in item 4, and the smoke haze in the backdrop average (item 2).
