# Blueprint: Group-Consistent Routing (a group ordered together keeps one lane)

The owner answered [Q0079](../questions/closed/0079-group-consistent-routing.md) on 2026-11-10 with option 2. Units
that receive one order together take the lane of the group's centroid or leader whenever that route is within a
small configurable relative tolerance (about 5 to 10 %) of the unit's own shortest route. Units that are genuinely
separated still take their own shorter routes. The rule is an "allows more" deviation under Q0051.

This plan:

- defines a group, a lane and the decision exactly (§4);
- records what StarCraft II does and why this is a sanctioned deviation (§2);
- reproduces the split the question was about (§3);
- splits the work into latent packages with the tests written first (§6).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-10 against `main` at
`9bbf329`. Line numbers drift, so search for the quoted symbol. The planning probe of §3 was run on that commit and
was not committed. Claims not confirmed while planning are marked **verify**.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0079, group-consistent routing | Answered: option 2. Closed by WP6 | All |
| Q0051 (charter §8), mechanics follow StarCraft II; "allows more" deviations are recorded and preferably configurable | Standing rule | §2, D-7 |
| Q0052, the list of sanctioned deviations | Standing rule; this plan adds S-7 | D-7, WP6 |
| Q0061, implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §6 |
| Q0002, scoping is the agent's call if written down | Standing rule | D-1, §9 |
| None new | — | Every open choice has a default recorded as a decision |

---

## 1. Summary

| WP | Title | Changes results? | Size | Depends on |
|---|---|---|---|---|
| WP0 | This plan and its row | No | — | — |
| WP1 | `sim_map`: terrain lane cuts and lane words (homotopy signatures), latent | No | M | WP0 |
| WP2 | `sim_map`: TA* restricted to one lane, latent | No | M | WP1 |
| WP3 | `sim_core` and data: movement rules file (tolerance 0), route groups, order ordinals, checksum schema 26 | Checksums only (schema); no state changes | M | WP0 |
| WP4 | `sim_core`: the lane rule (leader, lane, member decision), route events, `route_trace` columns; shipped tolerance still 0 | No (tolerance 0) | M | WP2, WP3 |
| WP5 | Data: tolerance 75 ‰; measurements | Yes (terrain battles where a group straddles a fork) | S | WP4 |
| WP6 | Documents and bookkeeping: S-7, Q0079 closed, `DESIGN.md`, evidence, row | No | S | WP5 |

WP1 and WP2 (`sim_map`) and WP3 (`sim_core`) are independent and may run in parallel, in two worktrees. That is at
most two building agents (Q0055).

**Expected battle impact.**

- **Matrix and battle 33: bit-identical outcomes.** They are fought on flat `GameMap::new` arenas, which have no
  terrain obstacle and therefore no lane cuts. Every route has the empty lane word, so every member's own route is
  already in the group's lane and no route changes (D-5). The only differences are the extra lane search per group
  (`path_search_count`) and the checksum schema.
- **Showcase battle (`assets/scenarios/showcase.ron`, mirrored targets): expected unchanged in positions.** On
  `main` each army already takes one lane (§3). The units' per-tick positions are expected to be byte-identical in
  `route_trace`'s `units.csv`. Any change is a finding.
- **The goldens move by the schema bump of WP3.** WP5 may move them further, but only where a group straddles a fork
  on `default_arena`. Each change is traced to its cause.
- **The pre-Amendment-5 showcase (the Q0079 split, kept as a test fixture): the split disappears.** Player 1's two
  rhino beetles follow its eight pulse bots east (D-3: the leader is a pulse bot). The armies pass each other as they
  did on the grid, and the winner is expected to return to player 0. This is the fixture's purpose, not a shipped
  battle.

---

## 2. What StarCraft II does

Tags as in plan [`2026-11-09-sc2-pathfinding`](2026-11-09-sc2-pathfinding.md) §2:

- **[DOC]**: stated by Blizzard staff or in official material.
- **[2ND]**: a secondhand account.
- **[COMM]**: community observation.
- **[INF]**: inference made by this plan.

| # | Source | What it says |
|---|---|---|
| S5 of the pathfinding plan | Wicks Rivas & Souza, "Map Marker", SBGames 2021, citing an email from J. Anhalt (Blizzard), <https://www.sbgames.org/proceedings2021/ComputacaoFull/217610.pdf> | StarCraft II runs one A* per agent, so each path accounts for the agent's own size and climbing ability; the game "prioritized predictability" [DOC, private communication as cited] |
| S2, S3 of the pathfinding plan | Miyake, IGDA Japan GDC 2011 report; Czajecki 2024 | A navmesh, A* on triangles, a funnel per unit; moving units are not in the mesh, steering handles them [2ND] |
| S10 of the pathfinding plan | TeamLiquid, "Broodwar and Starcraft 2 Pathing", 2013, <https://tl.net/blogs/429573-broodwar-and-starcraft-2-pathing> | Groups clump through steering and flocking [COMM] |
| G1 | Liquipedia, "Magic Box", <https://liquipedia.net/starcraft2/Magic_Box> (**verify** the URL) | A group ordered to a point outside the selection's extent keeps its relative spacing: each unit is sent to the point plus its offset. An order to a point inside the extent makes the units converge [COMM]. The magic box spreads goals; it does not choose lanes [INF] |
| G2 | Bhattacharya, Kumar & Likhachev, "Search-Based Path Planning with Homotopy Class Constraints", AAAI 2010; Bhattacharya, Likhachev & Kumar, "Topological constraints in search-based robot path planning", Autonomous Robots 33(3), 2012 | A homotopy class of paths in the plane with obstacles is identified by a signature: the sequence of crossings of rays cast from one representative point in each obstacle. A graph search over states augmented with the signature finds the shortest path in a given class |
| G3 | Hershberger & Snoeyink, "Computing minimum length paths of a given homotopy class", Computational Geometry 4(2), 1994 | The shortest path in a homotopy class among polygonal obstacles is unique and taut. It is found by a funnel pass in the universal cover |
| G4 | Kamphuis & Overmars, "Finding paths for coherent groups using clearance", SCA 2004 | A group shares one backbone path and corridor, so that its members stay together instead of each taking its own shortest route; the technique is published for crowds, not RTS games |

**What this means.**

- StarCraft II routes every unit on its own (S5, S2, S3). No source describes group-consistent routing in the
  game. Q0079 already recorded that a group spread over a near-tied fork can split there, and that players correct
  it with another order.
- The magic box (G1) is a different mechanism: it gives each unit its own goal, so it can make splits more likely,
  not fewer. This engine gives every unit of a command the same goal, so it has no magic box. The pathfinding plan
  (§9) placed the magic box out of scope, and this plan keeps it there (§9).
- The deviation is **sanctioned and configurable**. A tolerance of 0 switches the rule off. With it off, routing is
  StarCraft II's per-unit routing, bit-for-bit as before this plan (D-6, test G-3). It is recorded as S-7 (D-7).

---

## 3. Reproduction (planning probe)

`route_trace` built in release from `main` `9bbf329`
(`CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/groupplan`). The outputs are under
`~/scratch/groupplan/` and are not committed. A unit's lane is the x where it first crosses y = 32: the west lane
crosses at about x 25.6 and the east lane at about x 38.4.

| Scenario | Player 0's attackers | Player 1's attackers | Winner |
|---|---|---|---|
| `showcase.ron` as shipped (mirrored targets, plan `2026-11-09-sc2-pathfinding` Amendment 5) | all 10 west (x 22.5 to 26.0, ticks 323 to 392) | all 10 west: at tick 250 all ten head for (25.5 to 25.6, 35.2); they meet player 0 north of y = 32 | player 0 (Amendment 6: tick 1,693) |
| The same with player 1's order `AttackMoveToStart(player_slot: 1, target_player_slot: 0)` (the order before Amendment 5; target (5.5, 5.5)) | all 10 west (x 25.1 to 26.0) | **8 pulse bots east** (x 38.4, ticks 327 to 376), **2 rhino beetles west**: at tick 250 both head for (25.5, 35.3); they stop at (28.4, 38.6) and (27.5, 39.1) and fight there | **player 1, tick 1,266** |

The Q0079 split no longer occurs in the shipped showcase. The mirrored targets made both armies' lane choices
symmetric, and on the mesh each army finds the west lane shorter as a whole. The split remains reproducible with the
old target. That scenario becomes the test fixture of G-1, built in the test and never written under `assets/`.

---

## 4. Design decisions

### D-1. What a group is

- **A group is one admitted movement command with at least two ground recipients**: one `PlayerCommand` whose order
  is `Move` or `AttackMove`, queued or not, admitted by `issue_movement_prevalidated`
  (`crates/sim_core/src/movement/mod.rs`). Its members are the command's recipients after the existing sort and
  dedup, restricted to `MovementLayer::Ground`; air units do not path. A command with fewer than two ground
  recipients forms no group.
- **Scenario initial orders form groups the same way.** `attack_move_combatants` (`crates/sim_core/src/scenario.rs`)
  today calls `issue_order` per entity. It is changed to collect its recipients and admit them through the same
  group-forming path, so that "the army attack-moves" is one group, as one order is.
- **Move and AttackMove never share a group.** They are separate commands, so they form separate groups, each with
  its kind and target. A queued command forms its own group, distinct from the group of the order before it.
- **Reinforcements do not join.** A unit produced later, or sent by a later command, belongs only to the groups of
  the commands it received. Rally orders are single-unit orders and form no group. Recorded under Q0002 (§9).
- **Binding by order ordinal.** Each entity gets `route_orders_issued: u32`, a wrapping counter incremented each
  time a `Move` or `AttackMove` enters its `order_queue`. A group stores its members as `(EntityId, ordinal)`, where
  the ordinal is the counter value of the order that command enqueued.
  - The ordinal of the unit's *active* route order is `route_orders_issued` minus the number of `Move` and
    `AttackMove` orders still in the queue. Route orders leave the queue only from the front or by clearing, never
    from the middle, so this ordinal is exact.
  - Retargeting (`MotionProposal::commit` rewrites the front order's target in place) does not change the count.
  - An acquired `AttackTarget` pushed in front of an `AttackMove` does not count. When the chase ends, the
    attack-move resumes with its binding intact (the B-112 path).
- **One insertion point.** Every insertion of a `Move` or `AttackMove` into `order_queue` goes through one
  `Entity` method that increments the counter: commit, flee (`target_acquisition.rs`), the ability flee move
  (`world/abilities.rs`) and initial orders. `replace_task_order` (`economy/mod.rs`) only ever inserts task orders;
  a `debug_assert!` documents that. A unit test checks the counter's arithmetic on every such path.
- **Storage.** The groups live in `SimWorld::route_groups`: a `Vec<RouteGroup>` kept sorted by `RouteGroupId(u32)`.
  Ids come from a world counter `next_route_group`. Each group holds its members as a `Vec<(EntityId, u32)>` sorted
  by `EntityId`. There is no hashed collection. Ids depend only on command order, never on the selection's order,
  because recipients are sorted first.
- **Reconciliation, once per tick at the start of phase 5** (movement), before the lane pass of D-3. It drops a
  membership whose entity is dead or loaded, or whose ordinal is below the entity's active route ordinal (that
  order has been consumed). A membership above the active ordinal is a queued order and is kept. A group with no
  memberships left is removed.
- **Dissolution.** A group lives exactly as long as some member still has its order active or queued. It is
  not dissolved when its leader dies, and not when it shrinks to one member (D-3).

### D-2. What a lane is: a homotopy class relative to the terrain

Two routes are in the same lane when one can be deformed into the other without crossing terrain that no ground unit
crosses. That is their homotopy class in the plane with the terrain's obstacles removed (G2).

- **Obstacles.** These are the connected components of the mesh's `Wall` edges and `Blocked` triangles
  (`navmesh/mod.rs`) that do not touch the map border. The component that touches the border is the outside, not an
  obstacle. The showcase's central plateau is one obstacle: its cliff walls and its ramp's side walls form one C,
  open at the ramp, and topologically that is one hole in free space.
- **Terrain only.** Structures (`Seam` and `Structure` triangles) are not obstacles for lanes. Lanes describe the
  sides of terrain features. Going round a building on either side is local, and per-unit routing handles it, as in
  the game.
  - As a result, the lane data depends only on the terrain base mesh. It does not change when a structure is placed
    or removed, or with a player's knowledge.
  - Terrain does not change during a match, so a group's lane never needs recomputing (D-3).
- **Cuts (G2, adapted to the lattice).** Each obstacle *h* gets a representative vertex ζ_h: its lowest lattice
  vertex, ties broken by the smallest x. Its cut is the vertical segment from ζ_h straight down to the first obstacle
  edge or border edge below it.
  - **Symbolic perturbation.** The cut is taken to lie on the line x = ζ_h.x + ε. A point with x ≤ ζ_h.x is on its
    left. The cut therefore never overlaps a vertical wall or a vertical route leg, and it never passes through a
    lattice vertex.
  - **Termination.** The first obstacle below lies strictly lower, because ζ_h is the obstacle's lowest vertex, and
    so does its own cut. The cuts therefore form a forest rooted at the border. Obstacles, cuts and border form one
    connected set, so the rest of free space is simply connected. G2's argument then makes the reduced crossing word
    a complete invariant of the homotopy class (paths with the same endpoints).
- **Lane word.** A polyline's word is the sequence of its cut crossings in order: `+(h+1)` for left to right,
  `−(h+1)` for right to left. It is freely reduced: adjacent `+a −a` pairs cancel.
  - Crossings are decided exactly in `i128` on raw Q32.32 values. Cut coordinates are lattice integers; the products
    are at most about 2^82 on the largest supported map.
  - Half-open rules give every degenerate case one answer. A leg through ζ_h or along x = ζ_h.x is decided by the
    perturbation. The cut's span is half-open, (y_hit, ζ_h.y].
- **Types and API** (`crates/sim_map/src/navmesh/lanes.rs`, new):
  - `LaneCuts`: the cuts, built with the terrain base mesh and kept across structure rebuilds.
  - `LaneWord`: a `Vec<i32>` in reduced form.
  - `Pathfinder::lane_word(&self, polyline: &[SimVec2]) -> LaneWord`.
  - `Pathfinder::lane_cut_count(&self) -> usize`, a diagnostic.
- **A channel's word.** The channel of a TA* route is a sleeve of distinct triangles, which is simply connected. Its
  funnel route (shrunk, pinned or the portal-midpoint fallback) lies in the sleeve. Every path in the sleeve between
  the same endpoints therefore has the same word. The search uses the polyline through the portal midpoints, which
  is exact and cheap; the returned route's word equals it. WP1 tests that equality on a seeded corpus.

### D-3. The group's lane: the leader's route, computed once

- **The lane pass runs once per tick, at the start of phase 5, after reconciliation.** It visits every group in id
  order that has no lane yet and has at least one *active* member: alive, ground and not loaded, with its membership
  ordinal equal to its active route ordinal.
- **Leader.**
  - The centroid is the mean position of the active members, computed in checked `i128` and rounded toward zero.
  - The leader is the active member nearest that centroid by exact squared distance. Ties go to the smallest
    `EntityId`.
  - Positions are read before any actor moves this tick, so the choice does not depend on actor order.
- **Why a leader and not the centroid.** The centroid can lie inside a cliff, across a wall or off the ground layer.
  The leader is a real unit: its start is valid and its radius is real, and it stands where the bulk of the group
  stands. The owner's answer allows either.
- **The lane search.** The pass runs the ordinary route search (`search_route` in `locomotion.rs`). The request is
  the leader's own: its position, its radius, the order's target and its owner's knowledge.
  - When the search returns `Found`, the group stores the lane: the route's waypoints, its `LaneWord`, the leader's
    id (as a diagnostic) and its radius.
  - A `Retargeted` or `Blocked` result stores `LaneState::None`. The members then route alone, and the pass does not
    retry.
  - The search is counted in `path_search_count`. The leader still runs its own search in its walk, which returns the
    same route from the same request, so its behaviour does not change.
- **Queued groups.** The lane is computed on the first tick at least one member has the queued order active. The
  leader is chosen among the members active on that tick.
- **Leader death and later changes.** The lane is stored data. After it is computed it does not depend on the
  leader, on the members' later positions or on structures (D-2). A dead leader changes nothing.
- **Stored per group** (hashed, D-6): id, player, kind, target, members, lane state, lane waypoints, lane word,
  leader id and radius.

### D-4. The member's lane at its position, and the decision

The decision is made at every route search of a member's active grouped route order: in `upkeep_route`, both the
first search and every re-plan. Pursuit re-routes (`reroute_pursuit`) chase an enemy and are not grouped. Neither
are task legs.

1. **Own route.** The member searches as today (`search_route`). If the result is not `Found` (retargeted or
   blocked), it is used unchanged, and the member is not grouped for this search.
2. **Join point.** The join point *q* is the point of the lane polyline nearest the member's position *p*, in
   Q32.32 (deterministic rounding). Ties go to the lowest leg index *k*.
   - If the segment p→q fails the mesh segment predicate at radius 0 under the member's own request (its owner's
     knowledge), the member is *separated*: a wall or a known structure lies between it and the lane. It keeps its
     own route.
3. **The member's target word.** T = reduce(word(p → q → w_{k+1}) · word(lane legs from w_{k+1})), using the lane's
   waypoints w.
   - The lane's per-leg crossings are stored with the lane, so the suffix costs one pass over the stored legs and no
     geometry.
4. **Same lane.** If `lane_word(own route) == T`, the own route is used, and no further search runs. This is the
   common case: on flat ground, and for every member once the group is past a fork.
5. **The lane route.** Otherwise the member runs one restricted search, `find_path_in_lane` (D-5), with its own
   request and T.
   - If it fails (`NoPathFound` or `InvalidGoal`: the lane does not admit the member's radius, or the lane is not
     reachable from it), the own route is used.
6. **Tolerance.** The lane route is adopted when `lane_len × 1000 ≤ own_len × (1000 + t)` in exact `i128`, where
   t is the tolerance in permille (D-7). Equality adopts the lane. Otherwise the own route is used.
7. **Re-plans stay with the group** by construction. The lane and T come from the group's stored data, not from the
   member's previous route.
   - A member already walking the lane that re-plans finds the lane route again.
   - Past the fork, its own route is in the lane's class anyway (step 4), so it costs nothing extra.
   - A member pushed across a fork is judged afresh by step 6.
8. **Route events (derived, unhashed).** A new `RouteOutcomeKind::GroupLane { group, own_length, lane_length,
   adopted }` is pushed whenever step 5 runs. `route_trace` counts these events (WP4).

**Cost.** At most one extra search per member per route search (step 5), plus one lane search per group (D-3).
Step 4 costs a polyline crossing test over the route's legs and the cuts. It is O(legs × cuts); WP1 measures the cut
count on the shipped maps, and a per-triangle cut index is added only if WP4's measurement asks for it.

### D-5. The restricted search (`find_path_in_lane`)

This is TA* exactly as in `navmesh/search.rs` (D-5 and D-6 of the pathfinding plan), with three differences.

- **Nodes carry their word.**
  - A node's representative point is the midpoint of its entry portal; the start node's point is the start.
  - Expanding a node adds the leg from its parent's point to its own point. That leg lies in the parent triangle,
    which is convex, so it stays inside the sleeve. The leg's crossings are pushed onto, or cancel the top of, a
    persistent word stack: an arena of `(letter, previous)` entries in a retained `Vec`, with each node keeping its
    top index.
  - A goal node adds the last leg to the goal. A goal is accepted only when its reduced word equals T.
- **Pruning.** Let *j* be the length of the longest common prefix of a node's word and T. The node is kept only
  while |word| − j ≤ K, where K is the largest number of cuts crossing any single mesh edge. K is computed with the
  mesh, and on the shipped maps it is expected to be 0 or 1 (WP1 measures it).
  - *Why this loses no route of class T.* G3 and the CAT(0) property of the universal cover: the shortest path in a
    class crosses each lift of a cut at most once. So its own crossing sequence is already reduced, and every prefix
    of it is a prefix of T.
  - The representative polyline differs from that path at the current portal by at most a sub-segment of one portal
    edge, which crosses at most K cuts. The rule therefore never prunes the channel of the shortest route in the
    class.
- **The visit cap is keyed by the state and the word.** A state is (triangle, entry edge, interned word id). Words
  are interned in a small trie, and only prefixes of T extended by at most K letters survive, so there are at most
  (|T| + 1) · (2·H)^K of them, where H is the cut count.
  - The first arrival of each word in each state is always expanded, so the search is complete within the class, as
    TA* is within the mesh. The cap `NAV_TA_STAR_TRIANGLE_VISITS` keeps its value.
  - Storage is a generation-stamped retained `Vec<u8>` of `3 · triangles · words` entries, with a `PERFORMANCE
    RATIONALE` block.

Everything else is unchanged: the anytime budget `NAV_TA_STAR_EXTRA_EXPANSIONS`, `two_pass`, `pinned_retry`, the
legality check and the fallback. With T equal to the unrestricted route's own word, the restricted search returns
exactly the unrestricted route (test H-4).

API: `Pathfinder::find_path_in_lane(&mut self, map, request: &PathRequestWithVision, lane: &LaneWord) ->
Result<PathResult>`, with the errors of `find_path_with_vision`.

### D-6. Determinism and checksum

- Integer and Q32.32 arithmetic only, with exact `i128` crossing and tolerance tests. `Vec` storage is iterated in
  index or id order. There are no hashed collections, clocks or floats (charter §3.1, C7, C9).
- Groups are created in command order. Members are sorted. The leader rule has a total tie-break. The lane pass reads
  tick-start positions. Member decisions read only the group's stored data, the map and the member's own request.
- **Checksum schema 25 → 26** (WP3) adds:
  - the world's `next_route_group` and its `route_groups`, in id order: id, player, kind, target, members and
    ordinals, lane state, and the lane's waypoints, word, leader and radius;
  - each entity's `route_orders_issued`.
- `LaneCuts`, the restricted search's buffers and the route events are derived and unhashed. `LaneCuts` is a pure
  function of the terrain.
- **Tolerance 0 is the off switch.** No group is formed, so `route_groups` stays empty, and no lane pass or extra
  search runs. The counters still advance, as they do for single-unit commands. A run at tolerance 0 is therefore
  bit-identical, checksums included, to the same run with every multi-unit command split into single-unit commands
  (test G-3). That is per-unit routing, StarCraft II's behaviour.
- Goldens are checked on Linux, Windows, macOS ARM64 and `wasm32-wasip1`, in debug and release.

### D-7. The tolerance: data, default, deviation record

- **File.** A new `assets/catalog/movement_rules.ron`:
  `(group_routing: (lane_tolerance_permille: 75))`, with a header comment that cites this plan, Q0079 and S-7.
  - It is loaded at run time by `RuntimeCatalogs::load_from_directory` as a fifth file, after `factions.ron`. It is
    parsed into a typed `MovementRules` and handed to `SimWorld`.
  - WP3 chooses between two carriers: a field of `UnitCatalog`, or a new argument of `SimWorld::with_catalogs`. It
    picks whichever changes fewer call sites, and records the choice.
  - The catalog is not hashed; catalogs must match separately, as for every other catalog file.
- **Validation.** `0 ≤ lane_tolerance_permille ≤ 1000`. A value above 1000, or a missing or unparsable file, is a
  typed `CatalogLoadError` naming the path. There is no default in code and no fallback (charter §1.1, §1.2).
- **Default 75 ‰ (7.5 %)**, the middle of the owner's 5 to 10 %. The showcase lanes differ by 0.35 %, and a member a
  few cells from its group adds a few percent at most over an 80- to 95-unit route. WP5 measures the
  own-to-lane ratio of every `GroupLane` event in the showcase, in the G-1 fixture and in the goldens, and records
  the distribution, so that the default can be tuned with evidence.
- **Fixtures.** `sim_fixtures` embeds the new file next to the others, and `sim_core::test_support` embeds it for
  `sim_core`'s unit tests, keeping `fixture_tests.rs`'s "embedded equals shipped" check. Tests that need another
  tolerance build a `MovementRules` in code.
- **Deviation record.** This is **S-7**, "group-consistent routing (Q0079)". It is added to the sanctioned list by
  an amendment of plan `2026-11-05-sc2-deviation-fixes` §4. `DESIGN.md`'s "S-1 to S-6" becomes "S-1 to S-7". It
  "allows more" and is configurable: 0 is StarCraft II.
- **Charter.** Charter §3.3 lists the catalog files. WP6 proposes to the orchestrator that
  `assets/catalog/movement_rules.ron` be added to that list. The charter is not edited here.

### D-8. Measurement

These are the tools of the pathfinding plan's evidence ("How to reproduce"): `route_trace`, `matchup_trace`, the
throughput bench, and the golden traces with per-tick state dumps.

- **Matrix**, the seeds of record plus 16 more, and **battle 33**, the four seeds of record plus 64 more, 68 seeds:
  compared with `main` at WP3, WP4 and WP5.
- **Goldens** (`golden_replay`, `golden_replay_orders`) and the sandbox checksum, re-recorded with traced causes in
  debug, release and `wasm32-wasip1`.
  - WP3's cause is the schema bump alone: per-tick `Debug` state dumps are identical except for the new fields.
  - WP5's cause, if any, is the first `GroupLane` adoption: its tick, entity and group.
- **Showcase** (`route_trace --ticks 1800`), shipped and G-1 fixture. Recorded per player:
  - the lane taken per unit (x at y = 32);
  - the per-box ratios and turns;
  - reversals, give-ups and the winner;
  - `GroupLane` counts and own-to-lane ratios.
- **Storyboard.** If the shipped showcase's `units.csv` changes, the storyboard (`assets/captures/showcase.ron`) is
  rendered and checked (`showcase_tests`, GPU through the harness of charter §7), and its scene ticks and captions
  are adjusted if needed.
- **Throughput** (`cargo bench -p sim_core --bench throughput`), under the gate lock, alternating head and `main`.
  - Every scenario must stay at least at 25 ticks/s.
  - `showcase_canyon` (400 units) must stay within 15 % of `main` (the criterion of the pathfinding plan's
    Amendment 5, 2).
  - The path-search count per scenario is recorded too.

### D-9. Stop rules

- **The matrix or battle 33 moves** in any outcome. Their arenas have no cuts, so any movement is a defect. Stop and
  explain before committing further.
- **A winner flips without a traced cause** in the matrix, battle 33 or the shipped showcase. Stop and report with
  the trace. The G-1 fixture's flip back to player 0 is expected and is not a stop.
- **The shipped showcase moves** (any `units.csv` byte) without a traced `GroupLane` adoption. Stop and diagnose.
- **A golden moves at WP4** (shipped tolerance 0). Stop: tolerance 0 must be bit-identical.
- **Throughput** below 25 ticks/s in any scenario, or `showcase_canyon` more than 15 % below `main`. Stop WP5 and
  profile: restricted-search expansions, word-trie size, and the step-4 crossing test.

---

## 5. File ownership

| WP | Files |
|---|---|
| WP1 | new `crates/sim_map/src/navmesh/lanes.rs`, `crates/sim_map/src/navmesh/mod.rs`, `crates/sim_map/src/pathfinding.rs` (the `lane_word` and `lane_cut_count` wrappers; cuts built with the terrain base), new `crates/sim_map/tests/lane_tests.rs`, `crates/gate/suites.ron` |
| WP2 | `crates/sim_map/src/navmesh/search.rs`, `crates/sim_map/src/navmesh/lanes.rs`, `crates/sim_map/src/pathfinding.rs`, `crates/sim_map/tests/lane_tests.rs`, `crates/sim_map/tests/pathfinding_properties.rs` |
| WP3 | new `assets/catalog/movement_rules.ron`, `crates/sim_core/src/catalog/runtime.rs` (and `catalog.rs` or `world.rs` per D-7), `crates/sim_core/src/error.rs`, `crates/sim_core/src/entity.rs`, new `crates/sim_core/src/movement/groups.rs`, `crates/sim_core/src/movement/mod.rs`, `crates/sim_core/src/scenario.rs`, `crates/sim_core/src/target_acquisition.rs`, `crates/sim_core/src/world/abilities.rs`, `crates/sim_core/src/economy/mod.rs`, `crates/sim_core/src/checksum.rs`, `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/lib.rs` (`test_support`), `crates/sim_fixtures/src/*` (embedded catalog), `crates/sim_core/tests/fixtures/golden_replay*.ron`, new `crates/sim_core/tests/group_routing_tests.rs` |
| WP4 | `crates/sim_core/src/movement/groups.rs`, `crates/sim_core/src/movement/locomotion.rs` (the `upkeep_route` hook only), `crates/sim_core/src/movement/mod.rs` (`RouteOutcomeKind`), `crates/sim_fixtures/src/trace/route.rs`, `crates/sim_core/examples/route_trace.rs`, `crates/sim_core/tests/group_routing_tests.rs` |
| WP5 | `assets/catalog/movement_rules.ron`, goldens, new `docs/evidence/2026-11-10-group-routing.md` |
| WP6 | `DESIGN.md`, `docs/plans/2026-11-05-sc2-deviation-fixes.md` (amendment: S-7), `docs/questions/closed/0079-group-consistent-routing.md` → `docs/questions/closed/` with its resolution, `docs/plans/README.md` |

**Boundary with the friendly-blocking plan.** That plan's §4 split of `locomotion.rs` stays as it is. This plan
adds one call in `upkeep_route`, which replaces the direct `search_route` call for grouped route orders. All group
logic lives in `movement/groups.rs`. Steering, the surround walk and the push table are not touched.

---

## 6. Work packages (commit order)

Each implementation agent writes the tests first, red where the WP changes behaviour, and hands back after `cargo
gate quick` and the tests its change needs. A verifier runs `cargo gate package` and reports the stamp or the
failing targets (Q0061). Simulation WPs also run the golden replays in release and on `wasm32-wasip1`, which
`package` selects. Agents build with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree>`
(charter §7).

### WP0: this plan and its row

`docs/plans/2026-11-10-group-routing.md` and its row in `docs/plans/README.md`, status `Proposed`.

### WP1: terrain lane cuts and lane words (`sim_map`, latent)

- Obstacle components, representatives, cuts and K (D-2, D-5), built with the terrain base mesh. `lane_word` and
  `lane_cut_count`.
- Tests, in a new `crates/sim_map/tests/lane_tests.rs` plus unit tests in `lanes.rs`:
  - **H-1.** A synthetic map with one walled island between start and goal. A route west of it and a route east of
    it have different words. Two different routes on the same side have equal words. A route round the island and
    back has the empty word after reduction.
  - **H-2.** On both shipped maps and a synthetic map with nested obstacles: every cut ends on an obstacle strictly
    lower or on the border, and the cut forest reaches the border. Records each map's cut count and K.
  - **H-3.** Splitting any leg at an interior point leaves the word unchanged, on a seeded corpus. Degenerate legs (a
    leg through a representative vertex, a leg along x = ζ.x, a leg ending exactly on a cut) are decided as D-2
    says.
  - **H-3b.** For every route of a seeded corpus on both shipped maps, the word of the route's waypoints equals the
    word of its channel's portal-midpoint polyline.
  - The showcase canyon's west and east routes, from player 1's spawn to (5.5, 5.5), have different words.
- No behaviour change. Every existing `sim_map` digest passes unchanged.

### WP2: TA* restricted to one lane (`sim_map`, latent)

- `find_path_in_lane` (D-5).
- Tests:
  - **H-4.** For every pair of a seeded corpus on both shipped maps, at radii 0.375 and 0.5, the restricted search
    with T equal to the unrestricted route's own word returns the identical `PathResult`.
  - **H-5.** On the H-1 island map, the restricted search with the other side's word returns a route of that word,
    legal under the segment predicate, and no shorter than the unrestricted route. A word no route can have (a lane
    too narrow for the radius, or an obstacle the route cannot reach) gives `NoPathFound`.
  - **H-6.** A recorded digest of restricted routes over a seeded corpus (both maps, both sides of the canyon
    plateau), as `validation_golden_tests` does.
  - In `pathfinding_properties.rs`: restricted routes are legal and carry their target word.
- No behaviour change.

### WP3: movement rules, groups, ordinals, schema 26 (`sim_core` and data)

- `movement_rules.ron` shipped with **tolerance 0**. Its loader, validation and typed errors, and the fixtures
  (D-7).
- `route_orders_issued` and the single insertion method (D-1). `RouteGroup`, group formation in
  `issue_movement_prevalidated` and in `apply_initial_orders`, and reconciliation (D-1). Lane state is stored but
  never computed while the tolerance is 0.
- Checksum schema 26 (D-6).
- Goldens and the sandbox checksum are re-recorded. Their only cause is the schema bump, which the per-tick state
  dumps prove (D-8).
- Tests, in a new `crates/sim_core/tests/group_routing_tests.rs` plus unit tests in `groups.rs`:
  - **G-6a.** The ordinal arithmetic is right through `Move` A then queued `Move` B, then a chase pushed in front
    and ended, a retarget, a flee and a non-queued replacement. Memberships are dropped exactly when their order is
    consumed.
  - **G-4a.** Group ids and memberships do not depend on the order of `PlayerCommand::entities`.
  - **G-11.** A tolerance above 1000, a missing file and an unparsable file each fail loudly with the path. The
    shipped file loads.
- Matrix and battle 33 bit-identical in outcome.

### WP4: the lane rule (`sim_core`)

- The lane pass (D-3), the member decision (D-4), the `upkeep_route` hook and `RouteOutcomeKind::GroupLane`.
- `route_trace` reports per group the members, `GroupLane` events, adoptions and own-to-lane ratios. Per unit, it
  reports the x where it first crosses a `--lane-line y` (default none).
- The shipped tolerance stays 0, so the WP is bit-identical to WP3 everywhere. Tests set the tolerance in code.
- Tests (red before the rule, at tolerance 75 ‰):
  - **G-1.** On the canyon map with the old player 1 order (§3), built in the test from `showcase.ron`, player 1's
    ten attackers all cross y = 32 on one side. Each unit's tick-0 route has the leader's lane word.
  - **G-2.** A group of two at tolerance 75 ‰: one member beside the leader, one placed on the far side of the
    plateau, whose lane route is more than 7.5 % longer than its own. The far member takes its own route, and its
    `GroupLane` event says `adopted: false`. A member behind a wall from the lane is separated (step 2) and runs no
    restricted search.
  - **G-3.** Tolerance 0 equals per-unit routing bit-for-bit. In the G-1 fixture and in the `golden_replay_orders`
    commands, the run at tolerance 0 and the same run with every multi-unit command split into single-unit commands
    have equal checksums on every tick and equal routes. A run at 75 ‰ on a flat arena equals the 0 run in every
    position (no cuts).
  - **G-4b.** The G-1 battle run twice gives identical checksum sequences.
  - **G-5.** Leader death. In G-1, the leader is killed on the tick after the lane is computed. The lane, the
    members' routes and their lane words are unchanged, and nothing is diagnosed.
  - **G-6b.** Queued orders. Move A, then queued Move B, round the island. B's lane is computed when the first
    member activates B, and it is that tick's leader's. A later non-queued order drops both memberships.
  - **G-7.** Re-plans stay with the group. A lane member whose route is invalidated (a structure placed on its lane,
    or the route dropped as stale) re-plans into the same lane word when within tolerance.
  - **G-8.** A member too large for the lane's narrowest passage gets `NoPathFound` from the restricted search and
    keeps its own route.
  - **G-9.** Air members form no membership. An attack-move interrupted by a chase resumes in its group's lane.
  - **G-10.** A group whose target is unreachable for the leader has `LaneState::None`, and its members route alone.
- Matrix, battle 33 and goldens bit-identical to WP3.

### WP5: tolerance 75 ‰ and measurements (data)

- `lane_tolerance_permille: 75` in `assets/catalog/movement_rules.ron`.
- The measurements of D-8 go into a new `docs/evidence/2026-11-10-group-routing.md`. Goldens are re-recorded with
  their traced causes if they move. The storyboard is re-checked if the showcase moves.
- The stop rules of D-9 apply. If one fires, the data change is not committed. The code of WP1 to WP4 stays on the
  main line at tolerance 0, and the numbers are reported to the orchestrator.

### WP6: documents and bookkeeping

- `DESIGN.md`:
  - the Movement and pathfinding row: group routing, `lane_tests.rs` and `group_routing_tests.rs`;
  - "S-1 to S-7";
  - §5 throughput, if it changed.
- Plan `2026-11-05-sc2-deviation-fixes`: an amendment adds S-7.
- Q0079 moves to `docs/questions/closed/` with its resolution: implemented by this plan, with commits.
- This plan's row is set to `Done` with commits and evidence. The orchestrator gets the proposal of D-7 for charter
  §3.3, and optionally adds "lane words" to `sim_map`'s §4 subject.

---

## 7. Risks

- **R-1. One lane means one column.** A whole army in one lane crowds more than a split army, and that crowding
  feeds the open friendly-blocking items B-103 and B-113 (reversals, corner wedging). WP5 records reversals and
  turning per box. A rise that is not caused by a defect of this plan is the friendly-blocking plan's to fix, and is
  reported to it.
- **R-2. Homotopy machinery is subtle.** Degenerate crossings, the cut forest and the K bound could each be wrong in
  a way that only shows on some map. Mitigations:
  - exact `i128` tests and one perturbation rule;
  - H-2 and H-3 on both shipped maps and synthetic nests;
  - H-4's identity check, which catches any pruning that loses the unrestricted route.
- **R-3. Pruning or the visit cap starves the restricted search.** The argument of D-5 covers the shortest route.
  If H-5 or G-1 still finds `NoPathFound` where a route exists, K is raised to the cut count for that query, and the
  case is recorded.
- **R-4. Throughput.** A grouped re-plan can cost two searches, and the word trie multiplies the visit array.
  Members in the lane already cost nothing extra (step 4). If D-9's throughput rule fires, profile before tuning:
  a per-triangle cut index, and caching T per member per lane leg.
- **R-5. The leader is unrepresentative.** A group spread over both sides of an obstacle at order time has a
  centroid between them, and the leader is whichever side is nearer. The tolerance still sends far members their own
  way (G-2). Recorded as accepted behaviour.
- **R-6. Order-ordinal bookkeeping.** An insertion path that forgets the counter would bind later orders to the
  wrong group. Mitigations: the single insertion method, the `debug_assert!` in `replace_task_order` and G-6a's
  coverage of every insertion path.
- **R-7. Goldens.** WP3 must change nothing but the schema, which the per-tick dumps prove. WP4 must change nothing
  at all.

---

## 8. Determinism summary

- Integer and Q32.32 arithmetic only. Crossings and the tolerance test are exact in `i128`.
- `Vec` storage, iterated in index or id order. There are no hashed collections, clocks or floats.
- Groups are formed from sorted recipients in command order. The leader has a total tie-break. Lanes are computed
  from tick-start positions and are fixed thereafter.
- `LaneCuts` is a pure function of the terrain.
- Checksum schema 26 (WP3) is the only schema change.
- Goldens are checked on Linux, Windows, macOS ARM64 and `wasm32-wasip1`, in debug and release.

---

## 9. Out of scope and follow-ups

Each item is recorded under Q0002 and can be brought back.

- **The magic box (G1)**, which spreads a group's goals by its formation, and formations and speed matching.
  StarCraft II has the magic box and this engine does not. That is a deviation in its own right, and WP6 proposes
  registering it in the bug register with its source.
- **Reinforcements joining a moving group**, rallies included.
- **Lanes round structures.** Lanes are terrain-only (D-2).
- **Pursuit and task legs.** Chases and harvest or build walks route per unit.
- **Re-choosing a group's lane** after it is computed: when terrain changes (it does not during a match), when the
  leader dies, or when the group's shape changes.
- **Air units.** They do not path.
- **Per-player or per-match tolerance**, for example a lobby option. The tolerance is one catalog value.

---

## Amendment 1 (WP3 to WP6 implementation, 2026-11-10): what landed differently

Recorded under Q0002; each item can be brought back.

1. **Checksum schema 27, not 26.** The branch was built on the deviation follow-ups, whose B-85 already took schema 26.
2. **Carrier of the rules (D-7):** a field of `UnitCatalog` (`movement_rules`, `set_movement_rules`), which changes
   none of the twenty `SimWorld::with_catalogs` call sites. A catalog built from unit definitions alone carries
   `MovementRules::PER_UNIT` (0); `RuntimeCatalogs::load_from_directory` always reads the file, with no fallback.
3. **The lane polyline** is the leader's route waypoints, which already start at its position.
4. **A member retargeted** to a reachable point (its goal no longer the group's target) routes alone.
5. **Goldens at WP5** moved by the hashed group state alone; the per-tick state is unchanged (`default_arena` has no
   lane cut). §1 expected them to move only where a group straddles a fork.
6. **G-3 on the goldens** is a test in `golden_replay_tests.rs` (orders fixture, tolerance 0, squads against
   single-unit commands, every tick); the canyon script and the flat arena at 75 ‰ are in `group_routing_tests.rs`.
7. **`route_trace`** gains `--order` and `--lane-tolerance`, so the G-1 fixture and pre-data-change measurements need
   no file under `assets/`; group measures go to `groups.md` and `lanes.csv`.
