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

Owner update (2026-11-04, via Q0043): I have verified that I have 260€ credit expiring 2027-01-24. The agent is free to use up to 200€ (including expansion batches and re-generations) and should not hesitate to request another video if a video doesn't fulfill requirements. Keep audio enabled.

Owner update (chat, 2026-11-05): "the only bro limit you need to take into account are the 200$. no weekly, round or batch limit (unless it's a limit dictated by Google)". Carried out by the Veo plan's Amendment 1 / WP8 (total spend ≤ $200.00 from the ledger; no per-run or weekly caps).

## Status

2026-11-05 (no answer needed now). The required set is done: 41 generations since the pilot, **$16.40** at list
price (Veo 3.1 Fast, 720p, with sound, $0.40 a clip), about 8 % of the 200 EUR allowance. Batch 1 spent 24 including
the pilot and the loop experiments ([`2026-11-04-batch-1.md`](../../evidence/veo/2026-11-04-batch-1.md)), batch 2
spent 17 ([`2026-11-05-batch-2.md`](../../evidence/veo/2026-11-05-batch-2.md)). The ledger
(`docs/evidence/veo/ledger.jsonl`) holds every request. Caps in `assets/media/veo_budget.ron`: 12 per run and 60 per
rolling 7 days (Q0047); no run hit a quota refusal. The expansion batches (Veo plan WP6, about 49 clips plus seeds
and regenerations, about $20 to $35 at list price, depending on seeds and regenerations) have not started; they wait for the orchestrator, which checks
in here first as the first answer asked. This question stayed open as the record of the spending rule until the
owner's 2026-11-05 answer replaced it with the single total-spend limit below.

## Resolution

Carried out on 2026-11-05 by the Veo plan's Amendment 1 (D-12, WP8,
[`2026-11-04-phase-8-5-veo-media.md`](../../plans/2026-11-04-phase-8-5-veo-media.md) §8) in commit `938a455`.
The only limit is now the total list-price spend of every generation the service accepted, summed from the ledger:
`assets/media/veo_budget.ron` holds `total_budget_usd_cents: 20000` ($200.00, below the 200 EUR allowance) and a
price per generation by model id (`veo-3.1-fast-generate-001`: 40 cents). `generate-veo` refuses a run whose
`--max-generations` would take the total above it (`BudgetError::TotalExceeded`), refuses a ledger or run model with
no price (`BudgetError::UnpricedModel`), and prints spent and remaining dollars before and after every run. The
per-run cap (12), the rolling 7-day cap (25, then 60 by Q0047) and `--weekly-budget` are removed; Google's quota
refusals still stop a run (exit code 3), and the message says it is Google's limit. The committed ledger
(`docs/evidence/veo/ledger.jsonl`) totals 41 generations, **$16.40 spent, $183.60 remaining**.
