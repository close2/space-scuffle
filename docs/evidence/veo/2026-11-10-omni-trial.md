# Gemini Omni trial (2026-11-10): stopped at Gate 0, nothing spent

Evidence for Amendment 2 of [`2026-11-04-phase-8-5-veo-media.md`](../../plans/2026-11-04-phase-8-5-veo-media.md)
(D-13 to D-16), run on 2026-11-10 (UTC) from `main` at `0157437`. The owner asked (chat, 2026-11-09) whether to
switch away from Veo to Google's new video model, Gemini Omni 1.2 Flash (`gemini-omni-1.2-flash`, GA 2026-11-05).

**Result: the trial stopped at Gate 0, the free preflight.** Omni is served only through Vertex AI's Interactions
API, and that API refuses API keys, the only credential the pipeline has. Its error asks for an OAuth2 principal.
No generation was started, so nothing was spent and no ledger line was written. Gate 1 (the six clips) did not run.
The owner question is [Q0081](../../questions/closed/0081-gemini-omni-credential.md); until it is answered, media stays
on Veo 3.1 Fast.

## What was built (no spend)

- `crates/asset_baker/src/omni_client.rs`: the Interactions request (`input` text and inline image parts,
  `response_format`, `generation_config.video_config.task`, optional `generation_config.seed`,
  `background`/`store`), interaction polling, finding the video, a redacted record, and the free preflight. It runs
  on the Veo client's transport: the key only in a curl configuration on stdin, HTTPS to trusted hosts only (the
  global host `aiplatform.googleapis.com` added explicitly), no redirect followed with the key, and redacted text.
  Its unit tests cover the request shape and parsing without network access.
- `crates/asset_baker/src/video_service.rs`: the trait `veo_run` now uses, so `generate-veo --model
  gemini-omni-1.2-flash [--resolution 360p|720p|1080p|4k]` would use the same plan, ledger, budget, review and bake.
  The ledger records the text and parameters actually sent; a recorded operation resumes only on its own service.
- `assets/media/veo_budget.ron`: `gemini-omni-1.2-flash` at 45 cents a generation (D-15).

## Gate 0: the preflight

`asset_baker veo-preflight --model gemini-omni-1.2-flash --credentials-dir <checkout>` (full output
`~/scratch/omni/preflight.log`; the key does not appear in it). Every request is free: GETs, and a POST of an empty
object that names no model and so cannot start work.

| Probe | Result |
|---|---|
| `POST https://aiplatform.googleapis.com/v1beta1/projects/<project>/locations/global/interactions` with body `{}` | **HTTP 401** |
| `GET …/projects/<project>/locations/global/interactions/preflight-no-such-interaction` | **HTTP 401 `UNAUTHENTICATED`**, reason `CREDENTIALS_MISSING`, method `genai.vertex.v1beta1.InteractionsHttpService.GetInteractionHttp`: "API keys are not supported by this API. Expected OAuth2 access token or other authentication credentials that assert a principal." |
| `GET https://aiplatform.googleapis.com/v1beta1/publishers/google/models/gemini-omni-1.2-flash` (express mode's path, no project) | HTTP 401 `UNAUTHENTICATED`, the same message |
| `GET …/projects/<project>/locations/global/publishers/google/models/gemini-omni-1.2-flash` | HTTP 404 (Google's HTML "not found" page) |
| `GET https://aiplatform.googleapis.com/v1beta1/interactions/…` (no project) | HTTP 404 (no such route) |
| For contrast, `veo-preflight` on `veo-3.1-fast-generate-001` (us-central1) | HTTP 404 on the model GET, as on every day since the [API notes](2026-11-04-api-notes.md); its generations work with the same key |

The refusal is about the type of credential, not the key, the region or billing. The service rejects API keys by
design ("not supported by this API"). That matches the
[express-mode page](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/start/express-mode/overview), read
2026-11-10, which lists no video model and does not name the Interactions API. The documented call uses
`Authorization: Bearer` with a `gcloud` access token
([first and last frames](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/video/generate-videos-from-first-and-last-frames),
read 2026-11-10). As AGENTS.md §1.2 requires for a blocked tool, the trial stopped here and did not look for a
workaround.

## API and pricing, as verified (2026-11-10)

- **Request shape:** as D-13 of the amendment, from Google's Vertex and Gemini API pages. The Vertex seed is
  documented as `generation_config.seed`, but its exact shape is not shown. The client sends it only with
  `--seed`, as an integer.
- **Price, Gemini API:** $12.00 per 1M video output tokens. That is 5,792 tokens per second at 720p, "about $0.07/s"
  ($0.28 for a 4 s clip, against $0.40 for Veo 3.1 Fast with sound). 360p is $0.023/s and 1080p $0.104/s.
  [Pricing](https://aistudio.google.com/docs/pricing).
- **Price, Vertex AI:** not verified. The pricing page returned no tables, and the budget uses the higher known
  rate (D-15).
- **Express mode:** not supported for Omni (above).

## Spend

| | Generations | List price |
|---|---|---|
| This trial | 0 | $0.00 |
| Ledger on `main` (`docs/evidence/veo/ledger.jsonl`, 241 `request_started` lines × 40 cents) | 241 | $96.40 |

The orchestrator's running total of **$104.91** also counts the effect-image and effect-clip starts
(`image_request_started`, `effect_clip_request_started`) in the effects work's ledger, which is not on `main`. This
trial adds nothing to either figure. The cap stays $200.00.

## Per-clip verdicts

None: no clip was generated. The comparators remain the existing Veo clips: 10 rejected `colossus_mech` Walks
([batch 5b](2026-11-09-batch-5b.md)), and the accepted `capacitor_dome` Idle ([batch 2](2026-11-05-batch-2.md)) and
`jet_jumper` Idle ([batch 3](2026-11-05-batch-3.md)).

## Recommendation

**Stay on Veo for now.** Nothing forces a switch, because Veo 3.1 Fast stays GA on Vertex AI. Omni's only proven
advantage is price, about 30 % less on the Gemini API; the Vertex price is unknown. Its behaviour on our failure modes
(push-in, rays, walks turning toward the camera) is untested. To run the trial, the owner must supply an OAuth2
credential: a service account with "Vertex AI User" on the same project (Q0081 option 1, which keeps spending on the
Vertex credit), or a Gemini API key (option 2, billed separately). The client then needs only a bearer-token
transport, and the trial resumes at Gate 0 with the plan unchanged.
