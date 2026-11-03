# Q0018: How should the frame-budget scenarios field 200 and 400 units?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** (a). Every frame-budget scenario has the same eight players on a circle. Each player
  starts with 0, 25 or 50 units (0, 200 or 400 in all) and attack-moves to the start opposite its own, so the armies
  meet in the middle. Only the measurement's data file changes if the answer differs.

## Question

Remediation Phase 6 asks for the client's frame time "measured at 200 and 400 units". The plan
`docs/plans/2026-11-03-close-phase-6-7-8-exits.md` (decision D-4) designs the tool `frame_budget` in
`crates/client_diagnostics`. Its scenarios were to be two players with 100 or 200 units each.

That cannot be loaded. The production scenario path (`sim_core::scenario::build_world_with_catalogs`) places a
player's units only on rings 1 to 3 around its town hall. The town hall is 5×5 cells, so those rings hold 96 cells,
and a player can start with at most 96 units. A map may give each player slot only one start location, and the
simulation has no teams, so two players cannot share a side. The plan expected the problem to be solved by moving
start cells in the data, but the limit does not depend on where the start is.

Options:

1. **(a) Eight players, measured as they are (default).** The tool's data file
   (`crates/client_diagnostics/data/frame_budget.ron`) puts eight players on a circle of radius 22 to 23 cells
   around the middle of a flat 128×128 map. Each attack-moves to the opposite start. The 0, 200 and 400 scenarios
   differ only in units per player: 0, 25 and 50. Every scenario has the same eight town halls, so the 0 scenario is
   a like-for-like baseline. The fight is an eight-way melee rather than two lines, which is denser than a two-army
   battle of the same size. The figures are 200 and 400 units on screen, as the criterion asks.
2. **(b) Two armies, after a simulation change.** A separate plan lets the scenario builder continue placing units
   on rings 4, 5, … until the loadout is placed (or until a stated limit). Scenarios that load today place the same
   units on the same cells, so their checksums do not change. That changes `sim_core`, needs its own tests, and the
   frame-budget scenarios would then be two players of 100 and 200 units.
3. **(c) Two armies of 96.** Measure at 192 and keep 400 out of reach. This does not meet the criterion.

Recommendation: (a) now. If the owner wants two-army battles specifically, choose (b), and the measurement is redone
after that plan lands.

## Answer

