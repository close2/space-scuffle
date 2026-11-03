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

