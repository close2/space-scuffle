# Q0026: Who decides whether a generated Veo clip looks right?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** the agent accepts or rejects each clip against written criteria and commits a contact
  sheet of every clip under `docs/evidence/veo/review/`. The owner can reject any clip later; it is then regenerated in
  the next batch.

## Question

The plan `docs/plans/2026-11-04-phase-8-5-veo-media.md` (decision D-7) reviews every clip before it is baked into the
game. The written criteria: one subject; matches its prompt; no text or watermark; no cut or morph into another
object; the subject stays inside the square crop; the action reads at 512 px; the identity's states look like the
same object (the plan reuses a frame of the Idle clip as the starting image of the other states to help).

Taste is the owner's. The first clip (the pilot, `plasma_siphon_dome` idle) and batch 1's sheets will be linked from
`docs/evidence/veo/`. A rejected clip costs one more generation from the weekly allowance.

Options: (a) the default above; (b) the owner approves the pilot's look before batch 1 starts (batch 1 then waits
for the answer); (c) the owner approves every batch before it is baked.

The style sentence that every prompt ends with moves from Rust into data (`assets/catalog/media_style.ron`) and loses
its reference to a commercial game's name. Say if you want the look described differently.

## Answer

Owner (2026-11-04): Option (a) — the agent accepts or rejects each clip against the written criteria and commits a contact sheet under `docs/evidence/veo/review/`; the owner can reject any clip later to be regenerated in the next batch.

Owner update (2026-11-04, via Q0043): The agent is free to use up to 200€ of the 260€ credit (expiring 2027-01-24) and should not hesitate to request another video if a video doesn't fulfill requirements. Keep sound enabled (we may play it when clicking on a unit). Also, jumps when a looping video restarts from the beginning are distracting — try to generate videos where the start and end position are at least similar (and check loop continuity during review).

## Resolution

2026-11-05. Option (a) was followed for every clip of the required set. The agent reviewed each clip against the
plan's D-7 criteria as amended (the framed square, the plain backdrop, and loop continuity measured by `asset_baker
loop-check`). It wrote a reason for every rejection into the ledger (`rejected_in_review` lines in
`docs/evidence/veo/ledger.jsonl`) and committed the contact sheets under
[`docs/evidence/veo/review/`](../../evidence/veo/review/). Per-clip verdicts:
[`2026-11-04-pilot.md`](../../evidence/veo/2026-11-04-pilot.md),
[`2026-11-04-batch-1.md`](../../evidence/veo/2026-11-04-batch-1.md) (merged `2c5ed5e`),
[`2026-11-05-batch-2.md`](../../evidence/veo/2026-11-05-batch-2.md) (merged `473b977`). Sound is kept, and
regenerations were preferred to near misses, as the owner's update asked.

The rule stays in force for later batches. To reject a shipped clip, name it (identity and state) in this file and
move the file back to `open/`; the clip is then regenerated in the next batch.
