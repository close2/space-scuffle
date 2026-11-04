# Q0043: Veo clips are wide, but the game cuts a centred square. How should clips be framed?

- **Asked:** 2026-11-04
- **Blocking:** yes (it decides whether batch 1's 9 paid generations are worth starting)
- **Default until answered:** spend nothing more. Veo batch 1 (WP4) and batch 2 wait. The pilot clip stays in
  `~/scratch/veo/clips`, held but not rejected, so it can still be used.

## Question

The one-clip pilot of the Veo plan
([`docs/plans/2026-11-04-phase-8-5-veo-media.md`](../../plans/2026-11-04-phase-8-5-veo-media.md), WP3) worked. It
cost one generation (about $0.40), and the clip, `plasma_siphon_dome` idle, looks right: one dome, the prompt's
colours and details, no text, no morphing. See the contact sheet
[`docs/evidence/veo/review/plasma_siphon_dome_idle.png`](../../evidence/veo/review/plasma_siphon_dome_idle.png) and the
evidence [`docs/evidence/veo/2026-11-04-pilot.md`](../../evidence/veo/2026-11-04-pilot.md).

The problem is framing. Veo returns a wide 16:9 video (1280×720). The baker turns each clip into square 512 px game
frames by cutting out the **centre 720×720 square**. The dome is about 810 px wide, so the square cuts the base and
side ramps off at both edges, and the steam touches the top. The prompt already says "fully visible with empty space
around it", and Veo still fills the wide frame. Most structures in batch 1 are wide domes and would likely be cut the
same way.

Options:

- **(a) Keep the whole frame instead of cutting it (recommended).** Change the baker to shrink the full 16:9 frame
  into the square and fill the bars above and below with the backdrop colour (`#0B101B`, which the prompts already
  ask for). Costs no generation. The pilot clip then passes every review criterion and is used as is. Subjects come
  out smaller in the square (about 60 % of the width instead of more than 100 %). That needs one planning and
  implementation round and a check of how it looks in game. Already-baked units would stay as they are unless
  re-baked.
- **(b) Generate structures in portrait (9:16).** Veo 3.1 Fast supports 9:16. The centre square of a 720×1280 frame
  is its full width, so a subject framed for portrait fits across. Needs a small change (aspect ratio as catalog data)
  and costs one more generation to redo the pilot. The pilot clip is wasted.
- **(c) Change the prompt only** (for example "the building fills the middle half of the frame"). Costs one or more
  generations, with no guarantee Veo follows it.
- **(d) Accept the cut.** Use the clip as it is. The building's sides end in straight vertical cuts in game.

A smaller, related point: the request does not send `generateAudio: false`, so Google also generates sound, which the
game throws away. The published price is $0.10 per second with audio and $0.08 without (Veo 3.1 Fast, 720p). Adding
`generateAudio: false` could save about 20 % if Vertex accepts it. Testing it needs a real generation, which could be
the next clip of whichever option you choose. Say if you want that tried.

Spending so far: 1 generation, $0.40 at list price, against the $15 target. The 18 remaining required clips are
projected at about $7.20.

## Answer

Owner (2026-11-04): Up to the agent's judgement on framing. I have verified that I have 260€ credit expiring 2027-01-24. The agent is free to use up to 200€ and should not hesitate to request another video if a video doesn't fulfill requirements. I do think that having sound is not a bad idea as we might want to play it when clicking on a unit (adding it later would be more difficult) — so keep audio generation enabled (do NOT set `generateAudio: false`, overruling the provisional note below). I did notice that jumps in videos (when a video starts from the beginning) are distracting: the AI should try to generate videos where start and end position are at least similar.

*(Earlier provisional note by orchestrating agent: chose option (a) for framing — fit the whole 16:9 frame into the square with backdrop-coloured bars; the `generateAudio: false` part of that provisional note is overruled by the owner's answer above.)*
