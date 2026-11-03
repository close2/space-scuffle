# Q0008: Are the Phase 5 game rules acceptable?

- **Asked:** 2026-11-02
- **Blocking:** no
- **Default until answered:** keep them as committed

## Question

One agent chose these rules while building the simulation. They shape how the game plays.

- Admitting an order charges nothing. Payment lands when the builder arrives.
- Cancelling a foundation refunds 75%. Cancelling a queued job refunds 100%.
- Defeat means no structures left. There are no teams.
- Builders are not consumed, so every faction builds alike.
- Only `standard_automaton` has a starting bank, so the skirmish is Automaton against Automaton.
- Add-on adjacency is not implemented, and bunkers are not carriers.
- A zero-supply unit can be queued while over the supply cap. This was not requested.

## Answer

Owner (2026-11-03): Keep as committed. Whenever the spec is silent make an internet search how sc2 does it.

## Resolution

2026-11-03, commit `ab53c32`. The rules stay as committed. The standing rule is now in charter §8 (`AGENTS.md`): where
the specification is silent on a game rule, search how StarCraft II does it, follow that, and cite the source in
the plan. `DESIGN.md` §4 (match result) no longer lists the rules as awaiting the owner.
