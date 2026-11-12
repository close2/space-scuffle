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

Owner (2026-11-03): Option a for now (eight players on a circle for 0, 200 and 400 units), option b as todo (allow the scenario builder to continue placing units on rings 4+ for two-army battles).

Owner update (2026-11-11): **Build Option (b) now (not as a deferred todo) and close Q0018.** Let the scenario builder (`sim_core::scenario::build_world_with_catalogs`) continue placing units on rings 4, 5, … around the town hall so two armies of 100 and 200 units can load (preserving existing placement on rings 1–3 so existing scenario checksums do not change), rerun the 200- and 400-unit frame-budget measurements with two armies, and move Q0018 to `closed/`.

## Resolution

2026-11-12. Option (b) is built and measured, as the owner's update of 2026-11-11 asked; plan
`docs/plans/2026-11-11-scenario-outer-rings.md`.

- **Scenario builder (WP1, `5085308`, merged in `c05b766`):** a player's starting units fill rings 1 to 3 around its
  town hall exactly as before, then continue on rings 4, 5, … inside the hall's ground region, outside other starts'
  inner zones and off resource and watchtower cells; a loadout the region cannot hold is a typed error. Every
  shipped scenario's checksum is unchanged (the table is in `5085308`'s message).
- **Frame-budget spec (WP2, `b0d373b`; Amendment 2, `0eb6fbf`):** two players of 0, 100 and 200 units on the same
  flat arena. Sent to each other's start, one army won and destroyed the other hall before the 1,500-tick window
  ended, so both armies now attack-move to the arena centre (`target_cell`).
- **Measurement (WP3):** fifteen accepted runs on the RX 580 through headless KWin at `0eb6fbf`; median frame
  interval p50/p95/p99 7.1/8.3/9.2 ms at 0 units, 9.2/13.4/15.2 ms at 200 and 11.8/19.1/22.5 ms at 400
  (`docs/evidence/2026-11-12-frame-budget-two-armies.md`, `DESIGN.md` §5).
