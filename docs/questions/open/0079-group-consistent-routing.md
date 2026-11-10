# Q0079: Should a group ordered together keep one lane?

- **Asked:** 2026-11-10
- **Blocking:** no
- **Default until answered:** no. Each unit plans its own shortest route at its own radius, as StarCraft II
  reportedly does, so a group can split between two nearly equal lanes (plan
  [`2026-11-09-sc2-pathfinding`](../../plans/2026-11-09-sc2-pathfinding.md), Amendment 4).

## Question

**Background.** With routes taken from the navigation mesh (plan WP9), the showcase map has two lanes round its
central plateau, west and east, whose lengths differ by about 0.35 %. Units of one army spawned a few cells apart
can find different lanes shortest: in the showcase, player 1's two rhino beetles go west while its eight pulse bots
go east, so the armies split, and the match's winner changes (evidence
[`2026-11-09-sc2-pathfinding`](../../evidence/2026-11-09-sc2-pathfinding.md), "Amendment 3, diagnosis").

In StarCraft II each unit paths on its own; a box-selected group spread over a fork can split the same way, and
players correct it with a move command or rally. The game has no documented group-consistent routing.

**Options.**

1. *(default)* Per-unit shortest routes, as StarCraft II. A group can split between near-tied lanes.
2. Group-consistent routing, an "allows more" deviation (Q0051): units that received one order together take the
   lane of the group's leader (or of the group's centroid) when their own best lane is at most a small fraction
   longer, configurable, so an army stays together.

**Recommendation.** Option 1, because it is the game's behaviour and Option 2 can be added later as a configurable
order-level rule without changing the mesh or the route search.

## Answer

**Option 2 (Group-consistent routing as an "allows more" deviation under `Q0051` when a unit's shortest route is within a small configurable relative tolerance, e.g. ~5–10%, of the group centroid/leader's lane).**

**Engine & gameplay insight:** While StarCraft II's low-level constrained Delaunay triangulation (CDT) funnel query is evaluated per unit at its own collision radius, having a cohesive army at a single spawn location fracture 2-vs-8 around opposite sides of a central plateau over a `0.35%` distance tie separates frontline units from ranged support on a single `AttackMove` order and looks like a pathfinding defect in the showcase. Routing units that receive a single order together along the group centroid/leader's corridor whenever that route is within a small configurable fraction (e.g., `5%–10%`) of the unit's individual shortest path keeps cohesive armies together across near-symmetric forks while still allowing genuinely separated units to take their own shorter routes.
