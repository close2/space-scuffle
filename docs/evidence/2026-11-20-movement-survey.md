# Unit movement and pathfinding: literature survey and recommendation

Date: 2026-11-20. Base: `main` at `914684b1` (read-only survey; no code changed).
Scope: the owner's question whether our movement stack should move to a different, simpler algorithm, and
whether allowing units to push, overlap slightly or intercept would make it much simpler.

Tags: **[DOC]** primary or official source; **[2ND]** second-hand account; **[COMM]** community observation;
**[INF]** this survey's inference. Every external claim carries a URL. Where a page could not be fetched
(403, Anubis) the claim is marked as read from a search snippet.

---

## 0. Summary

1. **The global layer is sound and should stay.** CDT + TA* + Demyen width + radius funnel is the published
   design closest to every account of StarCraft II (S2–S5 of plan `2026-11-09-sc2-pathfinding`), it is exact in
   integers, and its defects (B-141, B-146, B-149) are measured against an oracle on thousands of random layouts.
   That is property-based work, not seed fitting. Keep it.
2. **The local layer is the problem, and the problem is structural, not a missing rule.** Contact between bodies is
   enforced in three places with three different models: a velocity constraint at proposal time
   (`remove_blocked_velocity`, with a "receding allowance" read from last tick), a mode machine on top of it
   (waiting, surround walk with side, retreat accounting, closed-side re-arm), and a separate soft positional
   pass after the move (`collision.rs`, relaxation 0.35, speed caps, anchored bodies, `slide_around_fixed`).
   Bodies are sorted into binary classes (yields / does not yield, standing / moving, anchored, co-moving
   cosine ≥ 0.5) that switch from tick to tick. Every reversal class R-A to R-F is a disagreement between two of
   these models, or a class switch. Each new rule has fixed one class and moved others (crowd-slide: nine
   candidates, none holds; parked-body-aim X: B-144's rule made R-E worse).
3. **The literature has one consistent answer to this structure:** separate *intent* (a preferred velocity from
   the route) from *non-penetration* (one projection pass that all bodies share), and let mass/priority, not
   modes, decide who moves. Position-based dynamics (Weiss et al. 2017/2019), Continuum Crowds' "enforce the
   minimum distance" pass (Treuille 2006), Supreme Commander 2's physics pushing (Emerson), 0 A.D.'s pushing
   (A25+), Factorio's non-colliding biters, and — per community accounts — StarCraft II itself ("units move while
   pushing their own friendly units"; "lots of rules around who pushes who") all resolve contact softly and
   positionally. None of them keeps a hard velocity constraint between friends plus a walk mode.
4. **Recommendation (rank 1): a single deterministic position-based contact solver** (Jacobi, fixed iteration
   count, inverse-mass weights from the existing push table, terrain via the existing `settle_on_terrain`), with
   steering reduced to "preferred velocity from the route + arrival + one anticipatory side rule for oncoming
   bodies", and the surround walk replaced by a *goal choice* (a free slot on the weapon-range arc), not a
   locomotion mode. **Rank 2:** add a fixed-point ORCA/HRVO-style anticipatory layer for moving-vs-moving
   avoidance only if rank 1's crossing-friend behaviour (B-144) is not good enough. **Not recommended:** flow
   fields or continuum crowds as a replacement for the navmesh.
5. **Deviations rank 1 needs:** bounded transient overlap between friends (a few percent of the radius sum, never
   with enemies or terrain), and moving friends pushing each other by priority (community accounts say SC2 does
   this already, so it may be closer to SC2, not further). Both are configurable in data and should be put to the
   owner as one question.
6. **Evaluation must change with it:** pre-registered properties (no oscillation by amplitude, monotone progress
   potential, deadlock-freedom on scenarios with known capacity, chokepoint throughput, permutation invariance
   under ID relabelling), measured on thousands of *generated* scenarios with a held-out seed set nobody looks at
   during development. Battle winners are not an acceptance criterion except for mirror-match symmetry.

---

## 1. The current pipeline (as of `914684b1`)

Sources read: `DESIGN.md` §2 tick order and the Movement row and register rows B-103, B-141, B-144–B-149 (B-150–B-153
read from branch `resource-footprints-plan`; B-154 does not exist yet on any local branch); plans
`2026-11-09-sc2-pathfinding`, `2026-11-09-friendly-blocking`, `2026-11-19-crowd-edge-steering`, `-2`,
`2026-11-19-crowd-slide`, `2026-11-19-parked-body-aim`; module docs and entry points of
`crates/sim_core/src/steering.rs` (1,591 lines), `crates/sim_core/src/movement/` (6,270 lines;
`locomotion.rs` alone 3,110), `crates/sim_map/src/navmesh/` (6,307 lines).

### 1.1 Stages

| Stage | What it does | Where |
|---|---|---|
| **Global path** | Exact-integer CDT on a quarter-cell lattice, rebuilt on structure change; TA* over triangles with Demyen widths at the unit's own radius (anytime, `NAV_TA_STAR_EXTRA_EXPANSIONS`, visit cap); radius funnel (SSF with circles of `r + margin`); fitted retry; zero-room retry with circumscribed arcs and gate lines; free (unknown) vertices pulled as points and clamped; portal-midpoint fallback; nearest reachable point | `sim_map/src/navmesh/{cdt,search,funnel,segment}.rs` |
| **Group lanes** | Units ordered together take the leader's homotopy class if ≤ 75 ‰ longer (S-7) | `movement/groups.rs`, `navmesh/lanes.rs` |
| **Route following** | Route checked on schedule (8 ticks), on knowledge change, on refused step; repath cooldown 12; waypoint handoff by half-plane + predicate; own-radius corner offsets; aim past a body parked on a corner, rounding held while occupied | `locomotion.rs` (`upkeep_route`, `next_waypoint`, `aim_past_parked_body`, `parked_on_corner`) |
| **Local steering** | Goal direction bent by the nearest non-yielding body on each side (side weights, touching/overlapping flock weights, co-moving exemption at cosine ≥ 0.5); two standing non-yielding bodies closer than a diameter + margin are one wall, rounded at its nearer end | `steering.rs` `steer`, `side_blockers`, `forms_wall`, `round_wall` |
| **Velocity constraint** | `remove_blocked_velocity`: iterated projection removing approach into every non-yielding body, with an allowance equal to how fast that body receded *last tick* | `steering.rs` |
| **Waiting** | Admissible step < 1/8 speed: stand still, keep order | `locomotion.rs` `is_waiting`, `WAIT_THRESHOLD` |
| **Surround walk** | Blocked attackers (forward part < half step, stationary non-yielding friend) walk across the goal direction on a kept side; side from wall end or ID parity; reverse after 12 blocked ticks; spent after a full circumference; ends only on room beyond its own retreat; spent walk re-arms when the closed side opens for a whole step, on ground gained, or on a new target | `locomotion.rs` `surround_walk`, `walk_bound`, `across_on_side`, `admits_whole_step`, `closed_side_reopens`, `add_retreat`, `end_surround_walk`, `friend_*_ahead` |
| **Terrain contact** | Disc never ends deeper in a wall; slide along it; a sliver of slide waits | `movement/contact.rs` `settle_on_terrain`, `after_wall_slide` |
| **Hull** | Per-tick turn rates, `turn_before_move` gating | `locomotion.rs` `hull_gate` |
| **Collision and push** | After all moves: pairwise soft correction from one snapshot (order independent), relaxation 0.35, friendly correction capped at 0.65 of speed with a floor, anchored (firing/holding) bodies immovable, tangent weight 0.85 against fixed bodies, push table and push priorities, enemies separate evenly and uncapped, body classes | `movement/collision.rs`, `steering.rs` `slide_around_fixed`, `checked_dynamic_push` |
| **Arrival** | Done on reaching the goal or touching a body settled on the same goal | `steering.rs` `check_cluster_arrival*` |
| **Stall and give-up** | Progress < 0.1 → repath after 1 s, give up after 10 s; queued behind a fighting friend is not stalled (D-4); pursuit stall; abandoned-target cooldown; acquire leash 21 | `locomotion.rs` `check_progress`, `check_pursuit_progress`, `beyond_leash` |

### 1.2 How many special cases

- `movement/tuning.rs` holds **29 named tuning constants**, most unsourced (U-23). 30 distinct plan-decision
  citations appear in `steering.rs` and `movement/*.rs`.
- Counting rules in the table above: about **12 for the global layer** (each justified against an oracle) and
  **about 25 for the local layer**: side weights, co-moving exemption, wall formation, wall rounding, receding
  allowance, iterated projection, wait threshold, standing drift, walk trigger, walk side, walk reversal timer,
  walk circumference limit, walk end on room beyond retreat, re-arm on closed side, re-arm on ground gained,
  parked-body aim, held rounding, terrain slide, sliver wait, anchored classification, push table, push
  priorities, soft-push cap and floor, fixed-body tangent weight, queued-not-stalled.
- Open after all of this: B-103 (R-B 22, R-D 20, R-C 10, walk start retreat 51 + 14 over nine showcase seeds;
  4,659 reversals on the matrix of record), B-144 (crossing friend), B-145 residual, B-152 (oscillation beside
  rounded obstacles, 345 reversals for one unit), and the corridor's standing residual.

### 1.3 Diagnosis of the local layer [INF, from the plans' own traces]

1. **Two contact models that disagree.** The proposal treats a non-yielding body as a hard velocity constraint,
   relaxed by a prediction (receding allowance) taken from the tick-start snapshot. The collision pass then
   treats the same pair as a soft spring with different weights. R-B (step into a friend that stopped this tick),
   R-C (wall slide into a body: the body constraint is not re-applied after the terrain slide), and R-D (residual
   overlap eased apart over several ticks, sometimes back and forth) are all this disagreement. Crowd-slide's nine
   candidates tried to make the proposal predict the contact pass better; each fixed some events and created others,
   because the two models remain two.
2. **Simultaneous (Jacobi) decisions with hard constraints.** Every unit decides from the same snapshot, so a
   prediction about a neighbour's motion is always one tick stale. Hard constraints plus stale predictions give
   sawtooth motion. Soft, iterated positional solvers are designed exactly for this (§2.4).
3. **Binary classes that flip.** Yields / does not yield, standing / moving (`STANDING_DRIFT`), anchored, co-moving
   (cosine 0.5), parked on corner, first tick of an order. A body crossing a threshold changes which rule applies,
   so the step changes discontinuously (R-E, R-F, walk-start events traced to friends that "stand only because
   their order is one tick old").
4. **Modes with hysteresis.** The surround walk is a state machine (start, side, reverse, spent, re-arm, end).
   Each transition is a place where the step can reverse (R-A and the walk start/end retreats). Every fix added a
   condition to a transition.
5. **The metric rewards rules.** The reversal meter counts cosine < −0.3 between consecutive achieved steps,
   regardless of amplitude, so contact jitter of 10⁻³ world units counts the same as a visible step back.
   Crowd-slide shows the matrix's placebo band ([4,633, 4,698]) is wider than several candidates' effects, and one
   raw unit in `PUSH_RELAXATION` flips a winner. Rules are being accepted and rejected on noise-level evidence over
   fixed seed sets.

---

## 2. Literature and industry survey

### 2.1 StarCraft II itself

| # | Claim | Tag | Source |
|---|---|---|---|
| SC-1 | GDC 2011 AI Summit, "AI Navigation: It's Not a Solved Problem – Yet", James Anhalt (Blizzard) with Kring and Sturtevant; no public slides or recording | DOC (title) | <https://www.gdcvault.com/play/1014514/AI-Navigation-It-s-Not>; <https://hanecci.hatenadiary.org/entry/20101219/1292762130> |
| SC-2 | Notes on the talk: triangle navmesh by Delaunay triangulation, A* on it; "Collision. Lots of rules around who pushes who."; steering behaviours following, flocking, grouping, separation, avoidance, arrival; "Avoidance was hard to get right"; avoidance lets units walk around static groups; **units ignore units moving in their general direction; idle units can be pushed out of the way**; units look ahead and aim for the gap closest to the centre of their path; on meeting oncoming units they read the other's heading and turn the same way; the pathing engine uses no dynamic memory | 2ND | <https://howtorts.github.io/2014/01/06/pathing-literature-review.html> |
| SC-3 | CDT navmesh, A* with a funnel that accounts for unit radii, then local steering and collision layers; a blog about the talk mentions Boids steering and "horizon analysis" | 2ND (forum; page 403, read as snippet) | <https://gamedev.net/forums/topic/624135-does-starcraft2-use-navmesh-a-or-grid-a-for-pathfinding/>; <https://gamedev.net/forums/topic/648438-how-to-do-starcraft-2-pathfinding/> |
| SC-4 | Anhalt (private email, as cited): one A* per agent accounting for size; the game "prioritized predictability in the behavior of its agents" | DOC as cited | <https://www.sbgames.org/proceedings2021/ComputacaoFull/217610.pdf> |
| SC-5 | "in SCBW and W3, units move while avoiding friendly units, but in SC2, units move while **pushing their own friendly units** (when on hold position, they avoid each other)" | COMM (snippet) | <https://tl.net/forum/starcraft-2/223889-dynamic-unit-movements-your-thoughts> |
| SC-6 | The engine pushes units apart when separation radii overlap; why ground units cannot have a true moving shot | COMM (snippet; 403) | <https://tl.net/forum/starcraft-2/433944-depth-of-micro> |
| SC-7 | Units clump into tight balls while moving; players asked for a spread option | COMM | <https://tl.net/forum/starcraft-2-hots/377527-no-fix-to-clumpy-unit-movement?page=18>; <https://tl.net/blogs/373708-hots-perspectives-4-unit-clumping> |
| SC-8 | Units push through idling units but are blocked by units holding, attacking, patrolling and other moving units | COMM | SC2Mapster thread 158471 (friendly-blocking §3, fact 7); <https://sc2mapster.com/forums/data/158471-hero-limit-requirements/previous> |
| SC-9 | 5.0.14: Ultralisk allied push priority 0 → 1, "now pushes allied units out of the way when moving"; earlier shrunk to avoid getting stuck behind allies | DOC | <https://news.blizzard.com/en-us/article/24162754/starcraft-ii-5-0-14-patch-notes> |
| SC-10 | Units outside the concave stand; the player moves them | DOC | <https://news.blizzard.com/en-us/article/6640646/game-guide-unit-positioning> |
| SC-11 | `CMoverAvoid` "Ground" has no data parameters; `CGame` has no crowd field | DATA | SC2GameData `5444e12f13` (crowd-edge-steering §3.1) |

**What follows [INF].** SC2's local layer is *pushy and permissive* among friends: co-moving friends are ignored
by avoidance (SC-2) and are pushed (SC-5, SC-7); idle friends are pushed (SC-2, SC-8); units that hold or fight
are hard (SC-8, SC-10); "who pushes who" is a rule table (SC-2, SC-9). Two sources (SC-5, SC-8) disagree on
whether *moving* friends block or are pushed; SC-8's "blocked by other moving units" may mean that a moving unit
is not shoved sideways out of the lane, while SC-5 says units push friends along. Nothing describes a hard
non-penetration constraint between moving friends, and the visible clumping (SC-7) suggests friends overlap
their separation discs while moving. Our model, in which a moving friend is a hard constraint with a receding
allowance, is **stricter than any account of the game**.

### 2.2 Flow fields and continuum crowds

| # | Claim | Source |
|---|---|---|
| FF-1 | Supreme Commander 2 (Emerson): sectors of 10×10 one-metre cells, portal graph, A* over portals, integration and flow field tiles cached; per movement type its own cost field; large units by "wall cushioning" (walls moved outward per type). Motivation: in SupCom 1 colliding units **stopped and waited for the conflict to resolve**, because repathing on collision compounds; "all movement was engineered to prefer to stay on the same path". In SupCom 2, "agents could push each other around as well as slide along walls using physics" | <http://www.gameaipro.com/GameAIPro/GameAIPro_Chapter23_Crowd_Pathfinding_and_Steering_Using_Flow_Field_Tiles.pdf> |
| FF-2 | Tiling to reduce memory; SupCom 2 uses LOS to the destination near the goal | <https://howtorts.github.io/2014/01/04/basic-flow-fields.html> |
| FF-3 | A reverse-engineering account: SupCom 2 uses a static vector field plus local steering | <https://wildfiregames.com/forum/topic/16018-supreme-commander-2-pathfinding/> |
| FF-4 | Planetary Annihilation: flow fields (Uber video); density adds "time" in congested areas (community); units stuck on cracks (community) | <https://pcgamer.com/planetary-annihilation-devs-show-planet-creation-tech-clever-unit-pathfinding>; <https://forums.planetaryannihilation.com/posts/696344> |
| FF-5 | Age of Empires IV (GDC 2022, Frank Cheng): flow fields, hierarchical A*, and steering behaviours; buildings and walls change the world | <https://www.gdcvault.com/play/1027659/Pathing-in-Age-of-Empires> |
| FF-6 | Continuum Crowds (Treuille, Cooper, Popović 2006): per step, density field; per *group* a cost field and a potential by an eikonal solve; move; then **"enforce the minimum distance between people"**. Designed "for large groups with common goals, not for scenarios where each person's intention is distinctly different"; 2–5 fps for thousands of agents on 2006 hardware | <https://grail.cs.washington.edu/projects/crowd-flows/78-treuille.pdf>; <https://grail.cs.washington.edu/projects/crowd-flows/> |
| FF-7 | Congestion maps (Pentheny): aggregate density and velocity on a grid as an extra path cost; agents otherwise "pile up at popular, shared path waypoints"; hysteresis needed to stop path oscillation | <http://www.gameaipro.com/GameAIPro2/GameAIPro2_Chapter17_Advanced_Techniques_for_Robust_Efficient_Crowds.pdf> |
| FF-8 | Photon Quantum's docs advise flow fields or tile pathfinders when agent counts exceed ~100 | <https://doc.photonengine.com/quantum/v2/manual/navigation/avoidance> |
| FF-9 | A design survey of RTS engines: "no shipped RTS engine uses pure flowfields (except SupCom2/PA by the same team)"; Spring tried flow maps and abandoned them | <https://iron-curtain-engine.github.io/iron-curtain-design-docs/decisions/09d/D045-pathfinding-presets.html> |

### 2.3 Velocity obstacles (RVO, ORCA, HRVO)

| # | Claim | Source |
|---|---|---|
| VO-1 | ORCA: each agent takes half the responsibility; action selection is a low-dimensional LP; sufficient conditions for collision-free motion; smooth | <https://gamma.cs.unc.edu/ORCA/>; van den Berg et al. 2011, <https://ics-websites.science.uu.nl/docs/vakken/mcrws/papers_new/van%20den%20Berg%20et%20al%20-%202011%20-%20Reciprocal%20n-body%20collision%20avoidance.pdf> |
| VO-2 | RVO2 library: C++ ORCA, OpenMP parallel, floats | <https://github.com/snape/RVO2>; <https://gamma.cs.unc.edu/RVO2/> |
| VO-3 | HRVO fixes RVO's "reciprocal dance" (agents cannot agree on a side) by enlarging the obstacle on the side not to pass | <https://gamma.cs.unc.edu/HRVO/>; <https://github.com/snape/HRVO> |
| VO-4 | Sunshine-Hill, "RVO and ORCA: How They Really Work": ORCA keeps the passing sides of first contact and so eliminates side flicker, **but its solver often fails** — a half-plane per neighbour lets three agents rule out every velocity, resolved by linear relaxation; **cornering**: an agent following a winding path violates the constant-preferred-velocity assumption, so ORCA agents "travel far, far from their desired trajectories to maintain passing sides, or simply become stuck at the corner", leading "to frequent deadlocks"; static obstacles need ad hoc time horizons that "can lead to collisions when agents end up squeezed between static obstacles and other agents"; "there is no perfect collision avoidance system"; build a scenario library and an automatic test rig | <http://www.gameaipro.com/GameAIPro3/GameAIPro3_Chapter19_RVO_and_ORCA_How_They_Really_Work.pdf> |
| VO-5 | ORCA deadlocks in symmetric configurations; deadlock with polygonal agents | <https://www.cambridge.org/core/journals/robotica/article/addressing-the-deadlock-problem-of-the-optimal-reciprocal-collision-avoidance-algorithm-in-distributed-multirobot-navigation/70CEA27CFAE13D30DD3ABCD189F9019D>; <https://oaktrust.library.tamu.edu/bitstream/1969.1/152671/1/GIESE-THESIS-2014.pdf> |
| VO-6 | Photon Quantum (a deterministic fixed-point engine) ships HRVO-based avoidance; priorities split responsibility 0.25/0.75 | <https://doc.photonengine.com/quantum/v3/manual/navigation/avoidance> |
| VO-7 | A design for a deterministic RTS engine specifies "fixed-point deterministic collision avoidance based on RVO2/ORCA" with commitment locking against back-and-forth in narrow passages | <https://iron-curtain-engine.github.io/iron-curtain-design-docs/decisions/09d/D045-pathfinding-presets.html> |
| VO-8 | Float RVO in a tick-based game desynced; advice not to rely on determinism | <https://forum.arongranberg.com/t/deterministic-rvocontroller-results/13605> |
| VO-9 | Anticipatory (time-to-collision) forces: interaction energy depends on time to collision, a power law (Karamouzas, Skinner, Guy 2014); practical guide with time horizon as the key parameter, and a trick for overlapping agents (shrink radii) | <https://link.aps.org/doi/10.1103/PhysRevLett.113.238701>; <http://www.gameaipro.com/GameAIPro2/GameAIPro2_Chapter19_Guide_to_Anticipatory_Collision_Avoidance.pdf> |

### 2.4 Boids, steering behaviours and position-based dynamics

| # | Claim | Source |
|---|---|---|
| ST-1 | Reynolds' steering behaviours: seek, flee, arrival, obstacle avoidance, separation, cohesion, alignment, path following, combined by weights | <https://www.red3d.com/cwr/steer/gdc99/> |
| ST-2 | jdxdev "Boids for RTS": separation "more or less cancelled out the seek force" and caused stalls in concave groups; replaced by directional avoidance with a **side locked** to stop flicker; **friendly units generally push each other**, only holding units and enemies are avoided; synchronised arrival (touching neighbours stop together) | <https://www.jdxdev.com/blog/2021/03/19/boids-for-rts/> |
| ST-3 | howtorts avoidance: ray-cast avoidance works for one-on-one, fails for large groups and steep angles; suggests instead making physics push sideways "so they slide past each other", cheaper than RVO | <https://howtorts.github.io/2014/01/14/avoidance-behaviours.html> |
| ST-4 | Position-based crowds (Weiss, Litteneker, Jiang, Terzopoulos, MIG 2017; C&G 2019): agents as particles; predict positions from planned velocity; project short-range contact constraints (stiffness 1), long-range TTC constraints (soft), walls; 1 stability iteration + 6 solver iterations, Jacobi with averaging; ~10⁵ agents interactive; velocity = (x* − x)/Δt. Limitations: tuning; head-on symmetric cases might not resolve deterministically (a stochastic term proposed) | <https://web.cs.ucla.edu/~dt/papers/cag19/cag19.pdf>; <https://diglib.eg.org/handle/10.1145/3099564-3108160> |
| ST-5 | Context steering (Fray): a behaviour layer that combines interest and danger maps instead of summed forces, avoiding the cancel-out problem | <http://www.gameaipro.com/GameAIPro2/GameAIPro2_Chapter18_Context_Steering_Behavior-Driven_Steering_at_the_Macro_Scale.pdf> |

### 2.5 Navmesh, funnel and clearance

| # | Claim | Source |
|---|---|---|
| NM-1 | TA*/TRA*: A* over CDT triangles, per-triangle width test necessary and sufficient for a disc, modified funnel, anytime search | <https://cdn.aaai.org/AAAI/2006/AAAI06-148.pdf> |
| NM-2 | Kallmann LCT: a CDT refinement under which the triangle-local clearance test is exact for any disc; locally shortest paths first, extended search only when global optimality is needed; dynamic repair | <https://diglib.eg.org/handle/10.2312/SCA.SCA10.159-168>; <https://history.siggraph.org/?p=107908> |
| NM-3 | jdxdev: dynamic CDT navmesh with Q48.16 fixed point, variable agent size via triangle widths and expanded vertices | <https://www.jdxdev.com/blog/2021/07/06/rts-pathfinding-2-dynamic-navmesh-with-constrained-delaunay-triangles/>; <https://www.jdxdev.com/blog/2021/12/07/rts-pathfinding-3-variable-agent-size-smoke-tests-navmesh-fixes/> |
| NM-4 | Polyanya: optimal any-angle search on a navmesh for a point agent | <https://ijcai.org/proceedings/2017/70.md> |

### 2.6 Other RTS games and open-source engines

| # | Claim | Source |
|---|---|---|
| RT-1 | **0 A.D. (deterministic, fixed-point `CFixed` simulation)**: since Alpha 25 units no longer rely on unit-unit collision avoidance but on **pushing**; "makes movement of groups of units much smoother overall"; moving units push moving units, idle units push idle units, not across; average position over the turn; a perpendicular nudge when units appear to have crossed paths; pushing pressure slows crowded units; max push multiplier for light-vs-heavy stability; per-template `DisablePushing`. Developer: overlap "is not the intended result", but preventing it "while keeping pushing working is much harder" | <https://sources.debian.org/src/0ad/0.0.26-3/source/simulation2/components/CCmpUnitMotion_System.cpp>; <https://wildfiregames.com/forum/topic/56436-for-a-better-unit-movement>; <https://wildfiregames.com/forum/topic/64466-why-can-units-overlap> |
| RT-2 | 0 A.D. positions are fixed point (16-bit fraction); replacing `CFixed` with float causes out-of-sync errors | <https://svn.science.uu.nl/repos/edu.3803627.INFOMOV/0AD/docs/pathfinder.pdf>; <https://wildfiregames.com/forum/topic/17274-discuss-performance-improvements/page/2/> |
| RT-3 | Factorio FFF #316: biters jammed because "the biters find a path, but then there is another biter in the way"; fix: "we simply don't make biters collide with other biters", separation logic prevents infinite stacking | <https://factorio.com/blog/post/fff-316> |
| RT-4 | OpenRA: a cooldown so blocked units stop repeatedly failing path searches (cell-reservation grid movement) | <https://github.com/OpenRA/OpenRA/pull/21391> |
| RT-5 | Warcraft III: pathing by footprint and collision size on a grid; blocked units wait/route; no confirmed push (modder accounts) | <https://www.hiveworkshop.com/threads/collision-size.309631>; <https://us.forums.blizzard.com/en/warcraft3/t/pathing-algorithm/3786> |
| RT-6 | They Are Billions: up to 20,000 units, each with its own AI; no public description of the movement algorithm was found | <https://pcgamer.com/lords-of-xulima-dev-reveals-steampunk-strategy-game-they-are-billions> |
| RT-7 | Brood War: tile grid, regions, and a large unstick state machine ("the path-finding hack") — the design SC2 replaced | <https://www.codeofhonor.com/blog/the-starcraft-path-finding-hack> |

### 2.7 Lockstep determinism

| # | Claim | Source |
|---|---|---|
| DT-1 | Float determinism needs one compiler and one instruction set; cross-compiler/architecture identity is "naive" to expect | <https://gafferongames.com/post/floating_point_determinism/>; <https://gafferongames.com/post/deterministic_lockstep/> |
| DT-2 | Frontier (GDC 2024): floats possible with extreme care, fixed point the safer choice | <https://media.gdcvault.com/gdc2024/Slides/GDC+slide+presentations/Pollard_Bradley_CrossPlatformDeterminism+2024-03-26+09.34.19.pdf> |
| DT-3 | Spring RTS keeps floats in sync with `streflop`; small differences accumulate into a different game state | <https://springrts.com/wiki/Syncing_System>; <https://springrts.com/phpbb/viewtopic.php?p=565927> |
| DT-4 | Age of Empires IV runs a deterministic simulation and multithreads it safely | <https://gdcvault.com/play/1027610/The-MAW-Safely-Multithreading-the> |
| DT-5 | 0 A.D. and Photon Quantum: fixed point throughout (RT-2, VO-6) | as cited |

**Lesson [INF].** Every algorithm in this survey is deterministic once three things are fixed: arithmetic (we have
Q32.32 with deterministic `sqrt` and LUT trig), iteration order (we already iterate by `EntityId`), and the
neighbour set (our grid query returns sorted candidates). None needs randomness; where a paper adds noise to break
symmetric deadlocks (ST-4), a tie-break by ID or by the seeded `ChaCha8Rng` replaces it. Order-independent
(Jacobi) solvers are preferable to Gauss–Seidel because the result does not depend on the order at all, which our
collision phase already guarantees.

---

## 3. Comparison

Scale assumption: 400 units, 25 ticks/s, so a 40 ms budget; today 400 units run at 173 ticks/s on 64×64
(≈ 5.8 ms per tick, `DESIGN.md` §5), so there is roughly 7× headroom. Typical contacts: 6–10 neighbours per unit.

| Approach | Replaces of our local rules | Determinism / fixed point | Cost at 400 units | SC2 feel | Deviations needed |
|---|---|---|---|---|---|
| **A. Unified position-based contact solver (PBD, Jacobi, K fixed)** + simple steering | Velocity constraint, receding allowance, wait threshold and drift, settle-against-bodies attempts, `slide_around_fixed`, soft-push caps and floor, tangent weight, wall formation and rounding (mostly), walk retreat and re-arm rules if the walk becomes a goal choice. About 15–20 of ~25 | Easy: one `sqrt` per contact (have it), sums of corrections (order independent), fixed K. No LP, no division by near-zero beyond guarded normalisation | 400 × 8 × K=4 ≈ 13k pair projections, each tens of fixed-point ops: well under 1 ms | Good: pushy friends, hard holders and enemies, clumping while moving, standing outside the concave. Matches SC-2, SC-5, SC-7, SC-8 | Bounded transient overlap between friends (solver residual after K iterations); moving friends push each other by priority |
| **B. Fixed-point ORCA/HRVO** for movers + contact pass | Steering side rules (B-144, R-E), part of the wall rule. Does **not** remove the contact pass, waiting, or the walk | Feasible (Quantum, VO-6/7) but the 2-D LP needs line intersections and divisions; infeasible cases need the relaxed 3-D LP; per-agent results depend on the LP's constraint order unless fixed | ~400 LPs of ≤ 10 constraints: cheap | Unlike SC2: SC2 ignores co-moving friends and pushes; ORCA gives polite, spread, anticipatory avoidance | Not a deviation in itself, but a different feel; known cornering deadlocks (VO-4) on our route-following agents |
| **C. Flow fields / continuum crowds** | Replaces global routing and some steering for big groups | Grid integer Dijkstra/eikonal is deterministic | Field per goal × per radius class; attack targets give many goals; Continuum Crowds needs per-group eikonal solves per tick | Not SC2's design (SC2 is navmesh + A* per unit) | Radius classes or wall cushioning per size (FF-1); loses the exact navmesh work |
| **D. Keep the current architecture, more rules** | — | as today | as today | as today | — ; evidence: nine candidates in crowd-slide, X in parked-body-aim, all fail |
| **E. Non-colliding friends (Factorio)** + separation | Almost all friendly contact rules | trivial | trivial | Too soft for SC2: no blocking by holders, no concave | Large overlap; not acceptable for SC2 combat geometry |

**Would allowing push, slight overlap or interception make it much simpler?** Yes, and the evidence is in our own
plans. R-B, R-C and R-D exist *only* because a standing friend is a hard constraint at proposal time while contact
is soft afterwards. If friends are bodies in one soft solver with masses (anchored = infinite mass), a step "into a
friend that stopped this tick" is resolved in the same tick by the solver; there is no stale allowance to
predict, and there is no second model to disagree with. Pushing among co-moving friends removes the need to
predict who recedes. A permitted residual overlap (the solver's tolerance) removes the need for the wait
threshold, sliver waits and standing drift classes against bodies. What it does **not** remove by itself: the
choice of side around an obstacle group (needs one rule, see §4.2) and the concave (best done as a goal choice).
"Intercept slightly" (cutting a corner through a friend's disc) is subsumed by soft overlap and needs no separate
rule.

---

## 4. Recommendation

### 4.1 Rank 1: one contact solver, simple intent, concave as a goal

**Keep:**
- The navmesh, TA*, radius funnel, segment predicate and nearest reachable point (`sim_map/navmesh/*`), and their
  oracle-based acceptance. B-149's budget problem should be solved there (a budget by path cost rather than
  expansions, or an LCT-style refinement, NM-2), independently of this change.
- Group lanes (S-7), route checks, route following and the corner offsets.
- `settle_on_terrain` (B-113): it already is a positional, non-deepening projection; it becomes the terrain
  constraint of the solver.
- Push table and priorities as data (SC-9), hull turning, acceleration and braking, leash, the move give-up (SC-8
  fact 9 of friendly-blocking), body classes, and the order-independent snapshot discipline of `collision.rs`.

**Replace:**
1. **Contact (one place).** After each unit proposes `x* = x + v_pref·Δt` (clipped by hull and speed), run K (e.g. 4)
   Jacobi iterations over contact pairs: for overlap `d = r_a + r_b − |x_a − x_b| > 0`, move each body along the
   normal by `d · w_a / (w_a + w_b)`, where `w` is an inverse mass from the push table (0 for anchored, holding,
   immobile; equal for enemies; priority-weighted for friends; mover vs. idle friend: the idle one takes all). Sum
   corrections per body, average by count (ST-4), apply, then `settle_on_terrain`. Velocity is the achieved
   displacement. This replaces `remove_blocked_velocity`, the receding allowance, `slide_around_fixed`, the soft
   push caps, and every "settle against standing friends" candidate. Sliding along a holding line falls out of the
   projection: the normal component is removed, the tangential one kept.
2. **Steering (intent only).** Preferred velocity = direction to the next route point, braking on arrival. Add one
   anticipatory term for bodies that are *not* co-moving (oncoming or crossing): a time-to-collision side choice
   with the side **committed per pair** until the pair separates (SC-2's "turn the same way", ST-2's side lock,
   VO-4's lesson that kept passing sides kill flicker). Co-moving friends are ignored, as SC-2 says. This replaces
   side weights, the wall formation and rounding, and gives B-144 a rule with a source.
3. **Concave (a goal, not a mode).** An attacker out of range whose straight approach is blocked by anchored friends
   picks a goal point: the nearest free slot on its weapon-range arc around the target, from the tick-start
   occupancy of that arc, kept until it is reached, taken, or the target moves by more than a slot. The route and
   the contact solver then carry it there; the slide along the back of the front line (Q0074's account) is what the
   solver produces. A unit with no free slot stands (SC-10). This replaces the surround walk's start, side, reverse
   timer, circumference limit, retreat accounting and re-arm rules.
4. **Stall rules.** One progress potential per order (route length to goal, or distance to the range arc for an
   attack); give up a plain move after the existing timeout without progress (SC-8); an attack order with no free
   slot waits (D-4's decision, kept).
5. **Anti-oscillation.** Inertia from the existing acceleration limits, and a dead band: a correction below a
   small fraction of the step is not applied. This is a single, named, explained constant instead of the current
   class thresholds.

**Deviations to put to the owner (one question):**
- D-α: friends may overlap transiently by at most the solver's residual (target: ≤ 5 % of the radius sum while
  moving, ≤ 1 % at rest); enemies and terrain never. Configurable (`movement_rules.ron`).
- D-β: moving friends push each other by priority; equal priority shares the push. SC-5 and SC-7 suggest this
  is the game's behaviour, SC-8 suggests moving units block. Ask the owner to rule, with a data switch so both
  behaviours remain possible.
- D-γ: the concave is reached by slot choice, not by a walk mode. Q0074 described the visible result, not the
  mechanism, so this is a minor difference in mechanism.

**Risks.** Every battle and golden replay changes (a full plan, checksum schema bump if memory fields change).
Symmetric head-on deadlocks need an ID tie-break. Tuning K and the dead band is still tuning, but of two
constants with a physical meaning instead of ~25 class rules. Performance: within budget (§3).

### 4.2 Rank 2: anticipatory VO layer, only if needed

If rank 1's crossing and oncoming behaviour is visibly poor (B-144-type events remain), add a fixed-point HRVO or
ORCA layer for **moving, non-co-moving** neighbours only, feeding a preferred velocity to the same contact solver
(as Quantum does, VO-6, and as the Guy–Karamouzas guide suggests for TTC + ORCA, VO-9). Do not apply it to
co-moving friends or to static bodies (VO-4's cornering and static-obstacle failures). Its LP must iterate
constraints in `EntityId` order and use the relaxed LP on infeasibility.

### 4.3 Not recommended

- Flow fields or continuum crowds as a replacement (C): StarCraft II is a per-unit navmesh A* design (SC-3,
  SC-4), our attack goals are many and individual (FF-6 is for "large groups with common goals"), sizes need per
  class fields (FF-1), and the navmesh investment is sound. A congestion cost (FF-7) could later be added to TA* as a
  deviation if chokepoint throughput demands it.
- More special rules on the current architecture (D): the last three plans are the evidence.
- Factorio-style non-collision (E): too soft for SC2's blocking and concave.

### 4.4 Migration path

1. **WP0 — evaluation harness, no behaviour change.** A scenario generator (`sim_fixtures`): random maps with
   random obstacles and chokepoints of known width, random army compositions and radii, random orders (move,
   attack-move, attack, hold), and families with *known answers*: N units through a gap of width w (capacity
   bound), two groups crossing, a ring around a target with a known arc length, a corridor with no way round.
   Metrics in §5. Baseline the current code on ≥ 2,000 generated scenarios; freeze a held-out set of seeds that
   no developer runs until acceptance.
2. **WP1 — the contact solver, behind a data switch** (`movement_rules.ron`), latent; unit tests of its
   invariants (no enemy overlap, anchored bodies never move, order independence, terrain never deepened,
   bounded friend overlap).
3. **WP2 — switch contact; delete `remove_blocked_velocity`'s allowance and `slide_around_fixed`.** Measure.
4. **WP3 — steering to intent + committed TTC side; delete side weights and walls.** Measure.
5. **WP4 — concave as slot choice; delete the surround walk.** Measure.
6. **WP5 — stall rules on the progress potential; delete superseded constants; goldens; throughput.**
7. Only then consider rank 2.

Each WP is accepted on the properties of §5 over the generated corpus (development seeds), and the whole is
accepted once on the held-out seeds.

---

## 5. Evaluation without fitting to seed sets

1. **Oscillation by amplitude, not by cosine.** Count a reversal only when two consecutive achieved steps both
   exceed a visible fraction of the unit's speed (for example 1/4) and their cosine is below −0.5; report the
   total "wasted path" = path length − net displacement over windows of 1 s. Contact jitter of 10⁻³ is not a
   behaviour.
2. **Progress (a Lyapunov-style check).** Per unit and order, the potential (route length to goal, or distance to
   the free arc) must not rise by more than ε over any 1 s window unless the order, target or knowledge changed.
3. **No deadlock.** On scenario families whose capacity is known (gap width vs. radius, arc length vs. attackers),
   every unit that has room reaches range or goal within a bound derived from distance/speed × a slack factor;
   units without room stand still (velocity below the dead band) — that is the SC2 behaviour (SC-10).
4. **Throughput.** Units per second through a gap of width w, compared across w and against a simple capacity
   model; monotone in w; no collapse at high load.
5. **Penetration bounds.** Enemy overlap = 0, terrain clearance never deepened, friend overlap ≤ D-α's bound.
6. **Permutation invariance.** Relabel entity IDs (a seeded permutation): aggregate metrics must be within
   statistical noise. This catches order and parity bias.
7. **Symmetry.** Mirror matches: win rate 50 % within its confidence interval over ≥ 500 seeds. Specific winners
   of specific seeds are not a criterion.
8. **Statistics, pre-registered.** Thresholds written in the plan before measurement; development on one seed
   range, acceptance once on a held-out range; distributions with confidence intervals, not single counts;
   placebo builds (as crowd-slide already does) to estimate noise, now over the generated corpus.
9. **Scenario library and rig** (VO-4's practical advice): keep every reproduced defect as a scenario, run every
   algorithm variant against every scenario automatically, and look at the replays as well.

---

## 6. Where the current approach is sound

- **Global routing** (CDT, TA*, radius funnel, segment predicate) is the best-supported reconstruction of SC2's
  design, exact and deterministic. Its plans measured against an oracle on up to 2,000 random layouts; that is the
  evaluation style §5 asks the local layer to adopt.
- **The snapshot discipline** (decide from tick-start state, order-independent pair sums) is the right foundation
  for a Jacobi solver.
- **Terrain contact** (`settle_on_terrain`) is already a positional projection and carries over unchanged.
- **The push table and priorities as data** match SC-2, SC-8 and SC-9 and become the solver's masses.
- **Diagnostic tooling** (class probe, placebo builds, paired seeds) is good method; what needs to change is the
  scenario population (generated, with held-out seeds) and the metric (amplitude-based).
- The plans' **refusal to land rules that fail stop rules** was right; it is the evidence that the architecture,
  not the rule set, is the limit.

---

## 7. Gaps and caveats

- No primary source describes SC2's local avoidance algorithm; SC-2 and SC-5 to SC-7 are second-hand or
  community, and SC-5 and SC-8 disagree on whether moving friends block. The recommendation is chosen because it
  can express both (data switch D-β), not because SC2 is known to be PBD.
- GameDev.net, TeamLiquid and Wildfire Games' Gitea returned 403 or an Anubis page; their claims are from search
  snippets.
- No published fixed-point ORCA/RVO2 port was found; Quantum's implementation is closed source.
- They Are Billions' and Warcraft III's movement algorithms are not publicly documented.
- B-154 was named in the task but does not exist on any local branch at the time of writing.
