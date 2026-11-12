# Independent review of WP13: mode and morph unit clips

Plan: [`2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), Amendment 10.
Owner approval: [Q0075](../../../questions/closed/0075-veo-morph-and-mode-clips.md). Implementer's write-up:
[`2026-11-12-mode-clips.md`](../2026-11-12-mode-clips.md). Branch `effects-wp13`, head `222d0bb`.

## How this review was done

I looked at the pictures before I read the implementer's write-up. That means the 11 download contact sheets, the
two chain sheets, the loop frames and seam sheets, and the three battle shots with their crops. I then cut both
mode atlases (`assets/textures/railgun_crawler-siege_mode.png` and `assets/textures/burrow_scorpion-burrowed.png`,
32 frames each, 512 px) and both default atlases into single frames with ImageMagick. I checked them:

- side by side, at full size and shrunk to 48 px (about a unit bubble at default battle zoom);
- by RMSE between frames (`compare -metric RMSE`, normalised 0–1) at every join and every inner step;
- with difference images;
- with a mound bounding box measured per frame, for the scorpion's zoom question.

Atlas frame numbers below count from 0. Idle is 0–7, Attack 8–15, Enter 16–23 and Leave 24–31. Default atlases
have 10 frames per state. Nothing was built, run or generated. The crops are in `~/scratch/wp13-review/`, which is
not committed.

For scale, the RMSE of one normal step inside a default Idle is 0.021–0.026 for the tank and 0.008–0.036 for the
scorpion. Inside the morphs, one step is 0.03–0.13.

**Verdict: all 8 accepted slices are ACCEPTED.** I agree with all 3 rejections. Five items are below the bar for a
reject and are listed under "Defects carried forward". The most important one is that the raised rails touch the
top of the source frame and are cut there (tank Attack frames 11–13, and slightly in the sieged pose itself).

| Atlas | Slice | Download | Verdict |
|---|---|---|---|
| `railgun_crawler-siege_mode` | Enter | `f589171c` | **ACCEPT** |
| `railgun_crawler-siege_mode` | Idle | `1868ff1e` | **ACCEPT** |
| `railgun_crawler-siege_mode` | Attack | `75b8dfad` | **ACCEPT**, with a framing defect noted |
| `railgun_crawler-siege_mode` | Leave | `8730b2d6` | **ACCEPT** |
| `burrow_scorpion-burrowed` | Enter | `7581379d` | **ACCEPT**, with a one-frame claw defect noted |
| `burrow_scorpion-burrowed` | Idle | `f4e82c3c` | **ACCEPT** |
| `burrow_scorpion-burrowed` | Attack | `6a021898` | **ACCEPT** |
| `burrow_scorpion-burrowed` | Leave | `69f7b85b` | **ACCEPT** |
| (rejected) | tank Enter | `5cc19947` | reject confirmed: continuous lightning out of the rails from frame 6 of the sheet |
| (rejected) | tank Attack | `4ce579c9` | reject confirmed: the rails drop to level and climb back each shot, a shell flies, and blue crackle shows in sheet frame 4 |
| (rejected) | scorpion Enter | `a40d56ac` | reject confirmed: both claws lie loose on the mound in sheet frames 6–7 |

## Identity against the default atlases

- **Tank.** The hull, tracks, turret, rail colour, cyan hull lights, camera angle and scale all match. Leave's last
  frame against the default Idle's frame 0 needs no scale correction; the best fit is a 4 px horizontal shift. Two
  things change in the mode, and both are acceptable.
  - The default has two smoke plumes; the sieged tank has one. The second plume fades during Enter (frames 21–23)
    and comes back during Leave (frames 28–31). This reads as an engine state, not an identity change.
  - The sieged hull carries four stabiliser feet, which is the point of the mode.
- **Scorpion.** The bronze carapace, amber glow bands, tail and claws match. Leave's last frame matches the default
  Idle's frame 0 at scale 0.98–1.00 with no shift. The burrowed mound is wider than the standing scorpion's
  footprint, but the body inside it is at the same scale.
- Both atlases keep the house letterbox: an opaque frame, a feathered bar above and below. The default atlases do
  the same, so the bubbles look alike.

## Railgun crawler (Siege-Tank analogue)

### Sieged versus mobile read at RTS zoom

At 48 px the sieged Idle reads as a different stance from the mobile Idle. The rails change from a horizontal cyan
pair to a steep cyan diagonal above the turret, and that is the dominant cue. The splayed feet widen the hull's
base, but they are only 1–2 px at that size and would not carry the read alone. The settled battle crop
(`battle/battle_railgun_crawler-siege_mode.png`, right panel) confirms the diagonal is the cue that survives. This
matches how the Siege Tank reads in StarCraft II: barrel and silhouette, not the feet. **The read is clear enough.**
A future polish could make the feet bigger or brighter, but that is not a defect of these clips.

### Enter (`f589171c`): ACCEPT

- **Join from default.** The default Idle's frame 0 against Enter frame 16 has an RMSE of 0.0077, which is
  effectively identical.
- **Join to mode.** Enter frame 23 against sieged Idle frame 0 is 0.0074. The mode picture is Enter's last frame,
  and the atlas sample matches it.
- **Motion.** The feet swing out over frames 17–19, and the rails rise from frame 21. Steps are 0.03–0.09, smooth for
  a 2.88 s channel. The camera is locked. There is no lightning, no sparks and no ray in the background.

### Idle (`1868ff1e`): ACCEPT

- **Loop seam.** Frame 7 to frame 0 is 0.027, which sits inside its own inner steps (0.022–0.048). Viewed as a crop
  of the stack, the plume continues across the wrap with no pop. The hull, feet and rails do not move.
- **Flagged smoke step.** The largest inner steps are frames 0→1 (0.044) and 1→2 (0.048). An enlarged crop of the
  stack across frames 0–7 shows a soft, continuously drifting plume, with no jump, cut or duplicate. The step is the
  normal speed of smoke at 8 frames over 3.9 s; the default Idle's smoke moves the same way. `loop-check` flagged it
  against a tight margin made for rigid bodies. **Not a defect.**

### Attack (`75b8dfad`): ACCEPT, with a framing defect

- **Read.** The rails stay raised, as the prompt demanded and as `4ce579c9` failed to do. The blast in frame 10 is a
  big white-blue bloom with short angular spark shards, followed by a recoil kick and smoke (frames 11–12). The tank
  then settles back to the pinned start pose: frame 15 against Idle frame 0 is 0.022, which is inside the Idle's own
  step range, so returning to Idle does not jump.
- **Artifacts.** There are no rays, stripes, sunburst or speed lines in the background. The shards are part of the
  muzzle blast and last one frame. The bloom lights the floor and the background blue for that frame. This is
  acceptable as a flash, and the default Attack's lightning-bolt frames (default frames 22–23) do the same.
- **Framing defect.** In frame 10, the flash is clipped flat along the top letterbox bar, giving a straight
  horizontal white edge. In frames 11–13, the recoil carries the rail muzzles above the source frame, so the rails
  end in a cut at the bar instead of at their muzzles. The cut is in the downloaded video itself (sheet row 2), so a
  re-bake cannot fix it. At bubble size the cut is a 1–2 px soft edge on 3 of 8 frames during a 4 s Attack, so it is
  not a reject.
- **Same problem in Idle.** Even in the sieged rest pose (Idle frame 0, which is Enter's last frame), the top
  muzzle grazes the bar and loses part of its rim. If the tank is ever regenerated, the prompt should place the
  tank lower or further away, so the raised rails stay inside the 16:9 frame.

### Leave (`8730b2d6`): ACCEPT

- **Join from mode.** Sieged Idle frame 0 against Leave frame 24 is 0.0073.
- **Join to default.** Leave frame 31 against default Idle frame 0 is 0.069. That is larger than a default Idle step
  (about 0.024), but no larger than Leave's own inner steps (0.049–0.080). The difference image shows a thin outline
  everywhere, plus the second smoke plume. That is a sub-pixel to 4 px placement drift and plume drift, not a pose or
  scale change. Leave holds its last frame until the channel ends, so the switch to the default atlas is one ordinary
  step of motion. **No visible jump.**
- **Motion.** The rails lower, the feet fold into the hull, and both plumes return. The camera is locked and there
  are no artifacts.

## Burrow scorpion (Lurker analogue)

### Burrowed read, and the faintness in battle

At 48 px the burrowed Idle reads at once: a dark earth disc with an amber-banded back and an upright tail. It is
clearly different from the standing scorpion, and the mound is the stance cue a player needs.

The "morphing" battle crop shows exactly that. In the "settled" crop the whole bubble, frame included, is faded to a
faint rim with a pink tint. That is the client's concealment shimmer for an own burrowed unit, applied to the whole
bubble. **It is the effect's fault (WP8, concealment and stance visuals), not the clip's.** The same clip is fully
legible mid-channel, before the concealment takes over.

I recommend recording the concealment as a follow-up against WP8. StarCraft II draws an own burrowed unit
translucent but still clearly legible, and here the own unit nearly disappears at default zoom. The low contrast of
dark-brown earth on the dark purple ground also does not help once the shimmer lowers alpha.

### Enter (`7581379d`): ACCEPT, with a one-frame claw defect

- **Join from default.** Default Idle frame 0 against Enter frame 16 is 0.030, which is inside the default Idle's
  own step range.
- **Motion.** The scorpion digs into a growing mound. Steps are 0.04–0.10, which is right for a 1.32 s channel. The
  camera is locked and there is no ray background.
- **Claw defect.** In frame 20 (about 2.3 s of the source), the right claw lies beyond the mound edge with no visible
  arm. It looks detached. In frame 19 the arm runs into the dirt, so the claw is plausibly buried rather than loose.
  This is the same class of fault that rejected `a40d56ac`, but much shorter: one atlas frame, 0.165 s of the
  channel, a claw about 4 px across at bubble size, against `a40d56ac`'s 1.2 s with both claws. I accept it as the
  implementer did. If the scorpion is ever regenerated, this slice should be redone.
- **Join to mode.** Enter frame 23 against burrowed Idle frame 0 is 0.054, larger than the tank's equivalent. Frame
  23 is a sample at about 3.5 s of the source, not the pinned 4.0 s frame. It shows the back slightly higher and
  brighter, with a claw tip out, and settles into Idle frame 0 one step later. That is within Enter's own step range,
  so it reads as the last bit of settling, **not a jump**.
- **Bar colour.** Enter's bar is a greyer `(69,68,87)` against the default's `(54,52,83)` and the burrowed Idle's
  `(60,58,87)`. The bubble's upper and lower interior therefore turns slightly grey for the 1.32 s channel. It is
  faint, but a tint step at both ends of the morph. Low severity.

### Idle (`f4e82c3c`): ACCEPT

- **Loop seam.** Frame 7 to frame 0 is 0.010, smoother than any inner step. The seam sheet and an enlarged crop of
  the stack show the claw sinking back into the mound by frame 7, with the same pose at frame 0. It loops without a
  jump.
- **Motion.** One claw lifts from the mound over frames 2–6 and settles back. The claw stays attached through a
  visible arm in every frame. It is a pleasant fidget for a burrowed unit, and the camera is locked.
- **Flagged: baked without zoom correction.** I measured the mound's bounding box per frame. Its bottom edge is at
  y = 386–387 px in all 8 frames, and its left edge is fixed at x = 31. The box grows only upward (from 208 to
  221 px tall) while the claw is raised. **There is no camera zoom.** The scale lock's +6 % was the raised claw
  enlarging the subject's bounds. Baking without the lock is the correct decision. Forcing the lock would have
  shrunk the whole picture while the claw was up, which would have made a real zoom pulse.

### Attack (`6a021898`): ACCEPT

- **Motion.** Spikes break through the back (frame 9). Then the mound heaves into a dark column with a crown of bone
  spikes (frame 10), the spikes scatter and sink (frame 11), and the claw rises and settles (frames 11–13). Frame 15
  against Idle frame 0 is 0.040, inside the Idle's step range, so the pinned end returns cleanly.
- **Read.** At 48 px the crown of pale spikes is the strongest burrowed cue in the atlas, and it says "Lurker attack"
  unambiguously. The spikes rise around the mound rather than in a line. As the implementer says, the line in the
  world belongs to the `spine_line` effect. That division is right.
- **Artifacts.** There are no ray backgrounds and no parts detaching; the spikes are projectiles and are meant to
  scatter. Frame 10's spike tips reach close to the top bar but are not cut.

### Leave (`69f7b85b`): ACCEPT

- **Join from mode.** Burrowed Idle frame 0 against Leave frame 24 is 0.0067.
- **Join to default.** Leave frame 31 against default Idle frame 0 is 0.062, inside Leave's own step range
  (0.048–0.128). The best alignment is scale 0.98 with no shift, and the residual is leg and tail pose. **No visible
  jump.**
- **Motion.** The mound bursts and the scorpion heaves out. The soil then thins from a ring of dirt (frame 27) to
  scattered crumbs (frames 28–29) and is gone by frame 30. At 4 s of source that would look like a dissolve, but the
  unburrow channel is 0.44 s, so it plays as a quick burst of earth, and that is right.
- **Artifacts.** A few dirt crumbs touch the top and bottom bars in frames 28–29. That is negligible. There are no
  rays, no detaching parts and no identity change.

## Checking the implementer's claims

- **"11 generations, 8 accepted, 3 rejected."** The download sheets confirm this, and every rejection reason
  matches what the sheet shows.
- **"Pictures chain the clips."** This holds. The tank's joins at the mode picture are effectively identical
  (0.007). The scorpion's Enter-to-Idle join is softer (0.054), because the 8-frame sampling does not land on the
  pinned last frame, but it is not a jump. Both Leave-to-default joins are ordinary motion steps.
- **"The tank is readable as deploying (legs out, rails raised)."** This is true of the rails. The legs are hard to
  see at default zoom.
- **"The burrowed scorpion's mound is faint because of WP8's concealment shimmer."** Confirmed: the clip is legible,
  and the shimmer fades the whole bubble.
- **"A claw rests apart from the body for a moment near 2.3 s; it is one frame."** Confirmed as one atlas frame
  (frame 20).
- **"The moving claw reads as a zoom of +6 %."** Confirmed. The measured mound does not change scale.
- **"Smoke inner step accepted as for the default Idle's smoke."** Confirmed. The plume is continuous.
- The write-up and the plan do not mention that the raised rails are cut by the top of the source frame (Attack
  frames 10–13; slightly in the rest pose). They should.

## Defects carried forward (none blocking)

1. The tank's raised rails and the Attack flash are cut by the top of the source frame. This shows in Attack frames
   10–13 and grazes the muzzle in the sieged rest pose. It can only be fixed by a regeneration with the subject
   framed lower or smaller.
2. In scorpion Enter frame 20, a claw appears detached for 0.165 s. Redo the slice if the scorpion is regenerated.
3. The scorpion Enter's letterbox bar is greyer than the bars of the slices on either side of it, so there is a faint
   tint step at both ends of the burrow morph. A bake-time bar normalisation per atlas family would remove it.
4. The feet are not legible at default zoom. The siege read rests on the rail angle alone.
5. Not this package's: WP8's concealment shimmer makes an own burrowed unit nearly invisible at default zoom.
   Record it against WP8.
