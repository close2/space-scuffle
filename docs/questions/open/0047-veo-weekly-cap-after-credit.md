# Q0047: Raise the Veo weekly cap now that the budget is 200 EUR of paid credit?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** keep `assets/media/veo_budget.ron` as it is (12 generations per run, 25 per rolling
  7 days). Batch 1 runs this week; batch 2 waits until batch 1's starts leave the 7-day window (about 2026-11-12).

## Question

The cap of 25 generations per rolling week comes from your first answer to Q0024, when the Veo allowance was thought
to be a free weekly quota. You then confirmed 260 EUR of paid credit and allowed up to 200 EUR, regenerations
included (Q0024 and Q0026, via Q0043).

Making loops seamless (plan [`2026-11-04-veo-seamless-loops.md`](../../plans/2026-11-04-veo-seamless-loops.md), §6)
adds about one "seed" clip per identity: its first frame becomes the picture each looping clip starts and ends on. The
required set then needs about 26 more generations, at most 32 with retries. That is about $10 to $13 at list price
($0.40 per 4-second clip with sound), $12 to $15 including the $2 already spent. Five of this week's 25 are used.

With 25 per week, batch 1 fits this week and batch 2 waits about a week. The expansion (about 49 clips plus seeds)
then takes three more weeks. The money is not the limit: 200 EUR is about 500 clips.

Options:

- **(a) Raise the weekly cap to 60 and keep 12 per run (recommended).** Batches 1 and 2 finish this week. Every run
  still has its per-run cap, the stop rules (first refusal, first unexpected result) and a ledger entry per request,
  and every batch is still reviewed before the next one starts.
- **(b) Keep 25 per week** (the default). Batch 2 starts about 2026-11-12.
- **(c) Another number.**

Either way, the agent pauses before the expansion batches, as you asked in Q0024.

## Answer

Owner (2026-11-04, in chat, copied by the orchestrating agent): "we have a lot of credits. rescuing clips is good,
but don't hesitate to use the credits (they will expire in 2 months anyway) if you think you should get a better
result." Read as: raise the weekly cap (option: 60 per rolling 7 days, 12 per run unchanged) and prefer quality over
saving generations, within the 200 EUR budget of Q0024.
