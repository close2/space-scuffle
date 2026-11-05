# Q0046: Veo clips always zoom in slowly. How should looping clips be made seamless?

- **Asked:** 2026-11-04
- **Blocking:** no (only Veo batch 1 and later batches wait; nothing else does)
- **Default until answered:** spend no more Veo generations on looping states. The orchestrator plans option (a)
  below as an amendment to the Veo plan, under charter §6, and batch 1 resumes with it once the amendment is
  approved.

## Question

You asked for looping clips whose start and end look alike (Q0026). Veo batch 1 (WP4) regenerated the
`plasma_siphon_dome` Idle clip four times. Every clip from `veo-3.1-fast-generate-001`, the pilot included, has a
slow camera push-in: the building grows by about 7 to 10 % over the 4 seconds. The game loops the clip, so the
building jumps back to its smaller size every 2 seconds or so. The push-in stayed with every approach tried:
"locked-off camera", "one static tripod shot … never moves, zooms or pans", a negative prompt naming zoom, dolly and
push-in, and starting from a still image. Evidence and measurements:
[`docs/evidence/veo/2026-11-04-batch-1.md`](../../evidence/veo/2026-11-04-batch-1.md).

The radiating stripes you pointed out are fixed. The prompt no longer asks for a "radial" backdrop, and a negative
prompt excludes rays. The last two clips have a plain backdrop.

Options:

- **(a) Pin the last frame to the first (recommended).** Veo 3.1 Fast accepts a `lastFrame` image as well as the
  starting image (Google's first-and-last-frame mode lists `veo-3.1-fast-generate-001`). Give a looping state the
  same picture as start and end, so the clip must return to where it began. The start picture would be a frame of
  a clip that is already good apart from the zoom (for example the plain-backdrop clip `0dbd7948`). That costs no
  extra generation, and the states chained from it keep the same design. It needs a small pipeline change (one
  request field and a data flag per state) and a plan amendment first. It costs about $0.40 per clip, as now. Not
  yet tried live: the documentation shows the end image as a Cloud Storage URI, and inline bytes (which this
  pipeline sends for the start image) are **to be verified**. If they are refused, the refusal costs nothing.
- **(b) Play loops forward then backward ("ping-pong").** A baker or client change only, with no generation, and
  it works for every clip, the pilot included. The zoom then breathes in and out instead of jumping. Rising vapour
  would also run backwards half the time.
- **(c) Compensate the zoom when baking.** Measure the push-in and crop each frame a little less as the clip goes
  on, so the building keeps its size. No generation, but it changes the framing frame by frame, and a wrong
  estimate makes the subject wobble.
- **(d) Accept the zoom jump** (about 7 %, every 2 seconds).

Spending so far: 5 generations, $2.00 at list price (1 pilot and 4 WP4 regenerations), all rejected, against your
200 EUR allowance.

## Answer

Owner (2026-11-04): Ratify Option (a) as primary (pin `lastFrame` to the start frame in Veo 3.1 Fast, seeded from frame 0 of `0dbd7948`). For any residual drift at the loop boundary or as fallback if `lastFrame` is unsupported, prefer combining scale normalization (Option c) and/or a short cross-fade of the last few frames back into frame 0 in `asset_baker` over ping-pong playback (Option b) for clips with directional motion such as rising steam/vapour (so steam does not run backwards half the time).

## Resolution

2026-11-05. Carried out by
[`docs/plans/2026-11-04-veo-seamless-loops.md`](../../plans/2026-11-04-veo-seamless-loops.md): option (a) as primary,
with the owner's fallback and no ping-pong.

- `lastFrame` is pinned to the start picture (WP1 `4a20fd3`, merged `b7becda`). Vertex accepted it inline, with
  sound, in the pilot (WP4 `16c456a`, merged `b35d793`).
- The bake repairs the seam with a scale lock and a short cross-fade into frame 0 (WP2 `464bd75`, `3668510`, merged
  `79f53f5`), and `asset_baker loop-check` measures loop continuity (WP3 `03ecf7b`, merged `71d80a0`).
- The push-in on buildings was then removed by per-kind action texts
  ([`2026-11-04-veo-structure-action-text.md`](../../plans/2026-11-04-veo-structure-action-text.md), `2155890`,
  `0ac74a4`).
- Every looping clip of batch 1 (merged `2c5ed5e`) and batch 2 (merged `473b977`) passes `loop-check`, raw or
  repaired. The numbers are in [`2026-11-04-batch-1.md`](../../evidence/veo/2026-11-04-batch-1.md) and
  [`2026-11-05-batch-2.md`](../../evidence/veo/2026-11-05-batch-2.md).
