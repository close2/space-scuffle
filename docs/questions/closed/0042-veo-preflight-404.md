# Q0042: The free Veo check answered "not found". Spend one generation on the pilot anyway?

- **Asked:** 2026-11-04
- **Blocking:** yes (it decides whether to spend a paid generation)
- **Default until answered:** spend nothing. The pilot and batches 1 and 2 of the Veo plan wait. Work that spends
  nothing goes on.

## Question

Before the first paid Veo clip, the plan
[`docs/plans/2026-11-04-phase-8-5-veo-media.md`](../../plans/2026-11-04-phase-8-5-veo-media.md) runs a free check
(`asset_baker veo-preflight`). The check asks Google to describe the model `veo-3.1-fast-generate-001` using your key.
On 2026-11-04 it answered:

```text
Mode: Vertex AI Express
Model: veo-3.1-fast-generate-001
HTTP status: 404
Model visible: no
```

The 404 came with no Google error code. There are two possible reasons, and the free check cannot tell which applies
(details in [`docs/evidence/veo/2026-11-04-pilot.md`](../../evidence/veo/2026-11-04-pilot.md)):

1. Vertex AI Express keys may not be allowed to read model descriptions at all, even when they can generate videos.
   Google's documentation does not say either way. The earlier Veo clips in this repository (plan 06) were made in
   this same Express mode.
2. The key, its Google Cloud project, or the model name is not valid for Veo generation.

Options:

- **(a) Spend one generation on the pilot (recommended if you know the key worked for Veo recently).** The pilot is
  one 4-second clip (`plasma_siphon_dome` idle). If the key is bad, Google refuses the request, which should cost
  nothing. If it works, the clip is needed anyway.
- **(b) Check the key yourself first.** In the Google Cloud console, confirm that the key's project has Vertex AI
  enabled and that Veo 3.1 Fast is available, or replace the key. Then answer (a).
- **(c) Change the free check first.** An agent plans and writes another free check, for example the global model
  resource `v1/publishers/google/models/{model}` instead of the project-scoped one, and runs it before any spend. This
  costs no generation but delays the pilot by one planning and implementation round, and it may give the same
  inconclusive answer.

## Answer

Owner (2026-11-04): Option (a) — spend one generation on the pilot (`plasma_siphon_dome` idle).

## Resolution

2026-11-04: option (a) carried out. One generation was spent on the pilot (`plasma_siphon_dome` idle). Google
accepted the request with the same key, and the clip (4 s, 1280×720, 96 frames) came back inline 32 s later. So
Vertex AI Express keys can generate even though they cannot read the model description: the preflight's 404 was
reason 1, not a bad key. Ledger lines 1 to 4 of `docs/evidence/veo/ledger.jsonl`; details and cost ($0.40 at list
price) in [`docs/evidence/veo/2026-11-04-pilot.md`](../../evidence/veo/2026-11-04-pilot.md). The pilot's review
raised a new question about framing, [Q0043](0043-veo-square-crop.md), which now holds batch 1.
