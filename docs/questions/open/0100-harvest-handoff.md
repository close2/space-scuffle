# Q0100: Adopt the inferred hand-off between two gatherers (B-162)?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** (a): the next worker in a resource's line starts 2 ticks after the previous take on a
  mineral field and 1 tick on a geyser (plan `2026-11-23-harvest-handoff` D-1, D-2).

## Question

In StarCraft II, three workers on one mineral field earn about 143 minerals a minute, not the 150.75 the data's
harvest time allows, and three on a geyser about 163 gas, not 169.6 (Liquipedia, Resources). Space Scuffle earns
150 and 171.4, because the next worker starts on the very tick the previous one finishes (bug B-162).

The game data contain no delay for this. Read in the engine's game loops, the game's figures are whole numbers:
47 loops a load on a field against a harvest time that ends on loop 45, and 33 on a geyser against 32. A worker
cannot move into position in 1 or 2 loops at its acceleration, so the waiting worker does not move. The plan reads the
gap as the harvest ability's stages: the field is released when the gatherer starts its half-second pause, one step
after it finishes, and the next worker starts one step later. A geyser has no pause, so it takes one step. That
reading fits all three published figures (gold fields too). It is an inference, not something the data states.

Options:
- (a) **Adopt it** (the default): 2 ticks on a field, 1 on a geyser, authored in the catalog. A saturated field then
  earns 144.2 a minute and a geyser 166.7.
- (b) **Keep today's behaviour** and sanction the difference as a minor deviation: `handoff_ticks: 0`, a data
  change; B-162 is closed as sanctioned.
- (c) Something else, for example after checking in the editor: with three SCVs on one mineral field, count the game
  loops between one SCV's mineral appearing in its hands and the next SCV's mining animation starting (the plan
  predicts 2), and the same at a refinery (predicts 1).

## Answer

- **Option (a): Adopt the inferred hand-off delay** (`2` ticks on a mineral field, `1` tick on a geyser, authored in the catalog as `handoff_ticks`).

**Reasoning:** Closely matches StarCraft II's 3-worker saturation economy (~144.2 vs ~143 minerals/min; ~166.7 vs ~163 gas/min, and fits gold fields) while keeping the value cleanly data-authored if ever refined.
