# Q0024: How is the Veo key billed, and what weekly cap should agents keep?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** treat the key as a weekly allowance of unknown size. One run starts at most 12
  generations, all runs together start at most 25 in any 7 days, and every run stops at the first quota refusal.

## Question

Q0001 was answered "Option B. Veo should be part of my ultra plan. So use it. (I think I have weekly limits…)". The
plan `docs/plans/2026-11-04-phase-8-5-veo-media.md` generates 19 clips for the required set (gas, research,
transport, the mineral path) and about 49 more for the rest of the Automaton faction. Each clip is 4 seconds.

The pipeline calls the Veo API with the key in `.env` (Gemini API, or Vertex Express if the key starts with `AQ.`).
As far as the planner knows, the Google AI Ultra subscription's video allowance applies in Google's apps (Gemini,
Flow), while API calls with a key are usually billed per second of video to a Google Cloud or AI Studio account. The
planner could not check which applies to this key without reading it, which agents must not do.

The plan bounds spending either way: a required `--max-generations` per run, a rolling 7-day cap from
`assets/media/veo_budget.ron`, a stop at the first quota refusal (which costs nothing), a free pre-flight check of the
key, and a one-clip pilot before any batch.

Please say:

1. Does the key draw on the Ultra allowance, or is it billed per use? If billed, is there a spending limit you want?
2. What weekly cap should agents keep? Default: 25 generations per 7 days, 12 per run.
3. Should the expansion batches (the other 49 clips) run without asking again? Default: yes, one batch per week
   within the cap, in the plan's order.

## Answer

Owner (2026-11-04): Everything estimated in total below $15 is fine. (I think I have credit, if not, paying is fine for me. Target $15. Try really hard to stay below $50). If internet says that I should have free weekly quota, try to stay below it. Keep the 12/run and 25/week cap for the 19 required clips (~$11.40 estimated if billed at Fast API rates), and pause/check before starting the 49 expansion clips so we stay within the $15 target.
