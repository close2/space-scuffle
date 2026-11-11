# Q0078: May agents use a Google image model for effect textures?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** yes. Agents may call a Gemini image model through the existing Google credential, for
  greyscale effect stills on black and Veo keyframes, within the owner's $200 total media limit and capped at the
  plan's **$25.25**, every call logged in `docs/evidence/veo/ledger.jsonl` with its model id. The default follows
  the owner's own proposal in chat on 2026-11-09: "maybe having generated images included in the effect could make
  this possible?" (orchestrator decision; the plan's earlier default was "no image spend").

## Question

Plan [`2026-11-09-effect-visualization`](../../plans/2026-11-09-effect-visualization.md) (§6) builds effects from authored greyscale textures (stains, scorches, cracks, smoke puffs, spikes) coloured
by the engine. A still-image model is the cheapest and best source for single-frame elements and decals. The model is
new to the project: the cheapest generally available Gemini image model the Vertex Express key may call on the day
(`gemini-nano-banana-2.1` at about $0.034 per 1K image, else `gemini-3.1-flash-image` at about $0.067 to $0.10),
for at most 60 images in the vertical slice and 250 in all (pairs on black and white for difference mattes, rejects
included), budgeted at $0.101 each = **$25.25** of the $200 total. The ledger stood at $96.40 on 2026-11-09.

Options:

1. Yes, within $25.25 (default, see above).
2. No: textures come only from Veo clips on black (already approved) and offline-generated noise.

Recommendation: yes, because decals and single-frame elements are where a still model is strongest and cheapest.

## Answer

**Option 1 — Yes: agents may use a Gemini still-image model for effect textures and Veo keyframes, targeting ~$25 (the planned $25.25) with an upper cap of < $50 within the $200 total media budget.**

**Technical insight:** Generating greyscale effect decals and sprite cards (scorch marks, cracks, smoke puffs, spikes, acid stains) on black/white backgrounds for difference matting via a Gemini still-image model is ~4–10× cheaper per asset than Veo video clips and yields cleaner alpha mattes and higher detail for ground decals and particle cards. Log every call in `docs/evidence/veo/ledger.jsonl`.

## Resolution

Recorded in plan [`2026-11-09-effect-visualization`](../../plans/2026-11-09-effect-visualization.md), Amendment 2
(commit `c5b5de2`): image-model target about $25, cap below $50, within the $200 total; every call goes
through `asset_baker effect-image`, which ledgers it. Review round 3 spent 8 paid images ($0.88); the image model
stands at 49 paid images, $5.39, and the ledger at $105.79 (2026-11-10).
