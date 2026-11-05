# Q0002: Are these player-mode choices acceptable?

- **Asked:** 2026-11-02
- **Blocking:** no
- **Default until answered:** build all three as listed

## Question

The Phase 5 blueprint assumes the following. Each can change later without replanning.

1. **Fog hides enemies but draws no darkness on terrain.** An enemy structure disappears when you lose sight of
   it, because remembered structures are out of scope.
2. **The game starts with `client --skirmish`.** Running `client` with no flags keeps opening the Visual Lab.
3. **Procedural atlases are allowed in temporary test fixtures only**, never at runtime paths. This is how gas,
   research and transport get tested without real art (remediation decision D6).

## Answer

Owner (2026-11-03): Agent is allowed to decide scoping decisions but must document them, so that we can easily bring them back into scope. (Note that I think that Q0002 is influenced by Q0001 — with Q0001 choosing Option B to generate media for gas, research and transport via Veo, item 3's temporary procedural test fixtures for those five assets can be replaced by real art once baked.)

## Resolution

2026-11-03. All three items stay as built; nothing in the game changes. The answer is recorded as a standing rule in
charter §8 (`AGENTS.md`, commit `ab53c32`): an agent may narrow or defer scope without asking, and records each such
decision in its plan's decisions or out-of-scope section (for example "Out of scope" in
`docs/plans/2026-11-02-phase-5-playable-client.md` §6, or "Decisions for the owner" in the newer plans), so it can be
brought back into scope. Item 3 follows Q0001: once real art for gas, research and transport is baked, it replaces
the temporary procedural fixtures for those five assets; that work belongs to the Q0001 media plan.

2026-11-05. Item 3 is done for those assets: Veo batch 1 replaced the temporary procedural fixtures of the gas,
research and transport tests with the shipped media (`fb1be5b`, merged `2c5ed5e`; `battle_card_path_tests.rs`), and
Q0001 is closed.
