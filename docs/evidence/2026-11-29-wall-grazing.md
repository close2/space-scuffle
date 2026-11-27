# Evidence: units grazing walls in the showcase (B-240 to B-242)

A diagnosis, not a fix. It answers the owner's observation of 2026-11-29 on the showcase video:
- one unit walks to the ramp, runs against the ramp's border, and has to correct its course to get down;
- some units run against the big block in the middle of the map.

StarCraft II units keep their radius clear of cliffs and obstacles and round corners smoothly. This file reproduces
both cases in the simulation, names the code path behind each, places each in the plans and the bug register, checks
which of it the `Gated` redesign covers, and recommends a fix.

**Tree.** `main` at `0f0608cd` (checksum schema 61, `contact.pipeline: Solver`), branch `wall-grazing`, measured on
2026-11-26 (session date) on the shared VM. No production code was changed.

**Instruments.**
- `route_trace` (`crates/sim_core/examples/route_trace.rs`) on `scenarios/showcase.ron`, default seed, 2,700 ticks.
  The match ends at tick 2,228 and player 1 wins. Its wall-overlap meter (`trace/route_overlap.rs`) measures depth at
  the terrain radius.
- A scratch probe, never committed, at `~/scratch/wall-grazing/probe/` (outputs in `~/scratch/wall-grazing/p0/` and
  `s1/` to `s8/`). It is a separate cargo package with path dependencies on this tree. It starts the battle through
  `sim_fixtures::trace::route::start_battle`, exactly as `route_trace` does. After every tick it writes one row per
  walking ground unit to `ticks.csv`:
  - position, the actual step (`position − pos_at_tick_start`), and the proposed step (`SimWorld::proposed_step`: the
    step before collision corrected it);
  - the first waypoint and the route length;
  - the nearest wall from `Pathfinder::wall_contact` (true terrain, every structure) and the clearance
    `distance − terrain_radius`;
  - the components of the proposed and actual steps along the wall normal (negative means into the wall);
  - the heading change.

  Each time a unit's route changes, the probe also writes the route to `routes.csv`, with the least clearance of the
  polyline "current position → waypoints", sampled every 0.02.
- `with-gpu-lock` was not needed. No frames were rendered (see §5).

Run it with
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/wall-grazing cargo build --release` in the probe directory, then
`wall_probe [--seed n] 2300 <out>`.

## 1. What the simulation shows

**No body enters a wall.** `route_trace`'s meter finds 4 + 5 unit-ticks with positive depth, all below 2⁻²⁰
(rounding). The deepest is 0.000002, unit 8 at tick 363. In the sense of B-113 and B-116, the disc at its terrain
radius never overlaps terrain.

**Bodies do touch walls, often and for long runs.** On the default seed, 276 of 8,992 moving ground unit-ticks (3.1 %)
end with a clearance under 0.005, and 1,102 (12 %) end inside the funnel's 0.05 margin. Over nine seeds (the default
and 1 to 8), touching unit-ticks were classed by region and by what put the body there:
- **aim:** the proposed step points into the wall (normal component < −0.001);
- **push:** the actual step goes further into the wall than the proposed step, that is, collision pushed the body;
- **ride:** neither; the body walks along the wall, already touching it.

| Region (world units) | aim | push | ride | longest run (unit, last tick) |
|---|---:|---:|---:|---|
| Ramp W channel (x 24.5 to 30.5, y 6.5 to 13) | 39 | 378 | 35 | 24 ticks (13, 155), identical on every seed |
| Ramp E channel (x 33 to 39.5, y 51 to 57.5) | 37 | 270 | 27 | 14 ticks (24, 210), identical on every seed |
| Central block (x 24 to 39, y 24 to 39) | 107 | 1,110 | 1,023 | 16 to 77 ticks; the longest is unit 23 up to tick 717, seed 3 |
| Elsewhere (spawn exits) | 42 | 9 | 58 | 3 |

The first 300 ticks are the same on every seed, because nothing random has been drawn yet.

**Map reference.** `y = 63 − row`.
- Ramp W is x 26 to 28, y 8 to 11, descending east from player 0's high ground. A side wall runs at y = 12 from x 26
  to 29, with convex corners at (26, 12) and (29, 12).
- Ramp E is the point image: x 35 to 37, y 52 to 55, with a side wall at y = 52.
- The central block spans x 26 to 37, y 26 to 34 at level 2, plus a tower at x 29 to 34, y 35 to 37 at level 3.
  Its west face is x = 26 from y 26 to 35, with convex corners at (26, 26) and (26, 35).

## 2. The cases

### Case A: the ramp. Rhino beetle 13 (player 0) meets the ramp's top corner at tick 113 and backs off until tick 128

This is the owner's first observation. Player 0 has two rhino beetles, entities 12 and 13. The `units` scene's
`FollowUnit` takes the lowest id (`director.rs`, `followed_unit`; `entities` is a `BTreeMap`), which is rhino 12.
Rhino 13 walks about 0.9 units from it.

1. **The route, at tick 0.** Rhino 13 (radius 0.5) starts at (11.63, 10.51). Its first waypoint is the corner point
   at the ramp's foot, (29.03, 11.45): the corner (29, 12) offset by r + `CORNER_SAFETY_MARGIN`. The leg is clear.
2. **Ticks 0 to 112: drift.** The proposed step stays on the leg. The actual step, however, carries an extra
   +0.007 to +0.023 in y on most ticks: friends to its south push it through the contact solver (moving friends push
   each other by priority, D-β). By tick 100 the body is at y = 11.86, 1.3 units north of the leg's line. It still
   aims at (29.03, 11.45). From there the straight leg passes 0.33 from the corner (26, 12), less than the radius 0.5.
   Nothing re-checks a leg until terrain refuses a step (`propose_step`: `permits_segment`, a centre-line test, only
   after a refusal).
3. **Ticks 113 to 115: the corner.** The disc touches (26, 12). Pulse bot 11, at (25.26, 11.06), touches it from
   the south (distance 0.876, radius sum 0.875).
   - The proposal collapses to (0.014, −0.047), because the step east is into the corner.
   - The actual step is (0.018, **+0.047**): the solver pushes the body north along the cliff face, against its own
     proposal.
   - Heading changes are 66.6° at tick 113 and 29.7° at tick 114.
4. **Ticks 116 to 127: the correction.** `slide_along_wall` removes the step's component into the corner (26, 12). An
   eastward wish at a corner to the east leaves only a small tangential part, pointing south. The body creeps south
   (step 0.024 rising to 0.088), reverses at tick 118 (177.5°), and curves round the corner.
5. **Tick 128.** The body is clear and goes east again at full speed, at y = 11.49 (clearance 0.0097 to 0.016).
6. **Ticks 132 to 155.** It then rides the ramp's side wall at exactly 0 clearance for 24 ticks, to the foot corner
   (29, 12).

About 15 ticks (0.6 s) are lost, with three sharp turns.

Rhino 12, the followed unit, does the same at the foot corner (29, 12) on tick 133, with clearance 0 and its proposal
into the corner. It is pushed north by the same crowd (+0.003 to +0.010 extra per tick over ticks 124 to 133). It then
turns 25.5° and 27.3° on ticks 135 and 137 as it takes the route's arc round the corner.

**Causes.**
- **(i) No check of the current leg after lateral drift (new, B-241).** The leg from the body's actual position to
  its first waypoint is never checked for the disc, so a crowd-displaced body aims through a corner.
- **(ii) Crowd pushes along the walk (the contact solver, D-β).** These pushes produce the drift, and at the corner
  they press the body into it.
- The wall slide then turns the step into the slow tangential creep.

The ramp's geometry is not the cause. The side wall at y = 12 lies where the navigation mesh has it, the route clears
it by the margin, and the rendered ramp follows the same cells (`mesher.rs` R1 and R2).

### Case B: the ramps' side walls. Every unit of both armies rides the y = 12 (W) and y = 52 (E) side walls

Player 1's units take the leg (56.50, 52.58) → (34.997, 52.425): 22 units long, 0.06 from the side wall at y = 52.
Pulse bot 23 walks exactly on it (clearance 0.060 to 0.065) until tick 186. Over ticks 186 to 190 its actual step
gains −0.012 to −0.016 in y over a proposal of ±0.001: friends push it. It then rides the wall at clearance 0 for 13
ticks, to the corner (35, 52), where its heading turns about 60° over 4 ticks. Pulse bots 22 and 24 do the same. In
the probe's classes, 81 % of the ramp-channel contact unit-ticks are **push**.

**Causes.**
- **(iii) The route's clearance is 0.05 (`CORNER_SAFETY_MARGIN`).** That is less than one tick of crowd push, which
  is 0.01 to 0.02 per tick here.
- **(iv) Route following has no lateral restoring term (new, B-242).** The unit aims at a waypoint up to 22 units
  away, so a lateral error of 0.05 is corrected at about `0.05 / distance` of a step per tick, effectively never.
- The wall holds the body, and the body rides the wall.

The tangent line between the ramp's two corners runs along the side wall, so any taut route through the ramp hugs it.
That part is geometry, and StarCraft II has the same.

### Case C: the block's west face. Player 0's column rides x = 25.625 for up to 75 ticks (ticks 257 to 387)

This is the owner's second observation, in the `advance` scene (ticks 250 to about 305, FollowArmy), and later in
`march`, when player 1's survivors walk the same face southward.

- Every route takes the west lane: (25.59, 25.89) → (25.575, 26.0) → (25.575, 35.0) for a pulse bot, at x = 25.575,
  which is 0.05 clear of the face x = 26.
- Pulse bot 10 reaches the corner (26, 26) from the south-east at tick 252. Its corner waypoints are handed off within
  `2r + 0.1` of the corner (`drop_passed_waypoints`), so it cuts the arc. At tick 258 it is at x = 25.62 with
  clearance 0.013, and from tick 259 it is at 25.623 to 25.625, clearance 0.
- It then aims at (25.575, 35.0), 9 units ahead. Its proposed lateral component is −0.0008 per tick, so it would need
  about 60 ticks to regain 0.05, and friends to its west keep pushing it east. It rides the face for 75 ticks
  (259 to 333), including a 25-tick stop in the queue (300 to 324).
- Units 4, 5, 7, 8 and 9 ride the same face, with runs of 7 to 68 ticks.

On the nine seeds the block holds 2,240 touching moving unit-ticks: 50 % push, 46 % ride, 5 % aim.

**Causes.** (iii) and (iv) as in case B, with the corner hand-off (v) as the entry.
- **(v) The corner hand-off discards the margin.** Corner points are passed as soon as the body is within
  `max(speed, 2r + WAYPOINT_HANDOFF_MARGIN)` of them and the disc fits along the leg to the next point with **zero**
  clearance (`segment_is_passable` at the terrain radius).
- This is plan `2026-11-09-sc2-pathfinding`'s own open lead from its switch section: "arc points at exactly `r +
  margin` whose legs graze the wall … a small extra corner margin for route following, handing an arc run off as a
  whole". It was never closed.

### Case D: the tower corner (26, 35). Pulse bot 21 (player 1) aims through the corner after a rounding-depth "wedge", ticks 686 to 697

- At tick 686, pulse bot 21 arrives at the corner (26, 35) with clearance 0. `route_overlap` lists it with a positive
  depth below 10⁻⁶ (`overlap.csv`: `686,21,1,25.9430,35.3706,0.000000`): fixed-point rounding.
- On tick 687, `drop_passed_waypoints` drops the whole arc (25.68, 35.28), (25.60, 35.15), (25.575, 35.0). The unit
  now aims at (25.575, 26.0), and the leg from its position to that point passes **0.31 inside** the corner.
- Why the hand-off allowed it: its view test, `in_view`, has an escape for a body "wedged where it stands (its disc
  overlaps a wall)". Such a body sees by terrain only, through `permits_segment`, which tests the centre line against
  cells. The rounding overlap makes `segment_is_passable(p, p)` false, so the leg is accepted on its centre line.
- The step into the corner is refused and slid, and speed collapses to 0.02 to 0.04.
- The unit turns 153° on tick 689, gets no route on tick 691, repaths, and reverses (180°) on tick 692.
- On ticks 694 to 695 it again stands at rounding depth (in `overlap.csv` too), and the same hand-off drops the new
  arc (leg 0.30 inside the corner). It works free by tick 698.
- Twelve ticks are lost, with two reversals.

The same event, a route update whose first leg runs 0.28 to 0.31 inside a corner, occurs twice on each of seeds 1 and
6 (ticks 717 and 727; 630 and 631). It does not occur on seeds 2 to 5, 7 or 8.

**Cause (new, B-240).** The wedged test has no tolerance. A body resting tangent to a wall, at rounding depth, is
treated as wedged, so the hand-off accepts a leg that cuts the corner.

### Not a cause

**Group lanes (S-7).** Both groups have no lane word and no `GroupLane` events (`groups.md`). The west lane is
each unit's own shortest route.

**The presentation.** The client draws the bubble at the collision radius, as a sphere resting on the ground
(`scene_controller.rs`: centre height = radius). For pulse bots and rhino beetles the collision radius equals the
terrain radius. For workers the bubble is 0.0625 larger than the terrain disc, but no worker case appears here. The
cliff mesh puts the contour on the cell edges (`mesher.rs`, dual-grid marching squares). Two of its features stand out
towards the unit:
- the wall rows' jitter, `WALL_JITTER_AMPLITUDE`, up to 0.06;
- the foot slope, `FOOT_WIDTH` 0.22 by `FOOT_RISE` 0.28. A sphere of radius 0.375 tangent to the wall line meets the
  slope by 0.021; one of radius 0.5 does not meet it.

A tangent body therefore looks as if it touches or barely dips into the cliff. The presentation adds at most about
0.06 to a contact the simulation really has; it does not create one.

## 3. Where these belong

| Cause | Code path | Plan or bug | Register |
|---|---|---|---|
| (v) corner hand-off at zero clearance | `locomotion.rs` `drop_passed_waypoints` (`handoff`, `in_view`) | Plan `2026-11-09-sc2-pathfinding`, switch section item a (lead not closed); A2 / D-9 | part of B-242 |
| Rounding "wedge" accepts a corner-cutting leg | `drop_passed_waypoints`, the `!segment_is_passable(p, p)` escape (plan D-3 of `2026-11-11-contact-and-missiles`) | none | **B-240 (new)** |
| (i) current leg not re-checked after drift | `next_waypoint` / `propose_step` (only a centre-line check after a refused step) | none (plan `2026-11-09-sc2-pathfinding` checks hand-offs, not the held leg) | **B-241 (new)** |
| (iii) + (iv) riding walls at 0 clearance | `CORNER_SAFETY_MARGIN` 0.05 (`sim_map/src/pathfinding.rs`); aim-at-waypoint following | Plan `2026-11-09-sc2-pathfinding` A4 (the margin) | **B-242 (new)** |
| (ii) crowd push toward the wall | contact solver, D-β | Plan `2026-11-20-contact-solver`; Q0122 measured in `Gated` WP2; B-220 (terrain-held body never passes its share) | B-220, existing |
| Wall slide creep at a corner | `contact.rs` `slide_along_wall` | Plan `2026-11-11-contact-and-missiles` | consequence, not a bug |

## 4. Does the `Gated` redesign address it?

Mostly not. Plan `2026-11-27-gated-movement` §12.2 puts "the global layer (navmesh, TA*, funnel, lanes)" out of
scope, and §3.2 I′-7 keeps `slide_along_wall` as a stage. None of its packages touch `drop_passed_waypoints`, the
held leg or the corner margin.

| Case | `Gated` effect |
|---|---|
| A (ramp corner, rhino 13) | **Partial.** C′-3, the terrain transfer (WP1, built latent): once rhino 13 is held by the corner, pulse bot 11 takes the whole correction, so the +0.047 shove north on tick 113 goes. WP2's gate (bodies only) proposes no step into a non-yielding friend, which should cut the drift injected over ticks 0 to 112, but D-β itself stays (Q0122's arm decides). Neither re-checks the leg, so a drifted body still aims through the corner. Unmeasured: `Gated` is latent and WP2 is stopped |
| B, C (riding walls) | **No.** The gate's rays are against bodies, not terrain. C′-3 stops a held body passing its share back, but leaves it on the wall. WP3a's orbit is for attack rings (`ring_aim`), not routes |
| D (rounding wedge) | **No.** It is route-following code, outside every package |

## 5. Cross-check against the video

No frames were rendered or inspected. The published video may be older than `0f0608cd`, and a render needs the GPU
lock. The scene clock is given so that a reviewer can check a render:
- `title` and `terrain` are paused, 275 frames (11.0 s).
- `units` plays ticks 0 to 149 at 1×, following rhino 12. Case A is ticks 113 to 128, at about 15.5 to 16.1 s, with
  rhino 13 in frame beside rhino 12.
- `smog` plays ticks 150 to 249 (17 to 21 s). It follows rhino 12 again: the ramp foot and its turn at tick 133 fall
  just before it.
- `advance` covers ticks 250 to about 305 (from 21 s). Case C, units 9 and 10 riding the west face, is in it.
- Case D (tick 686) is late in `clash` (FollowCombat, so it may be off-frame). Player 1's later rides of the west face
  are in `march`, at 4×.

The owner's two observations match cases A and C in scene and place.

## 6. Survey: how StarCraft II and RTS pathing keep clearance

Sources S1, S2, S11 and S15 are those of plan `2026-11-09-sc2-pathfinding` §2.

- **StarCraft II** routes on a constrained Delaunay navigation mesh, runs A* per unit at the unit's size, and
  smooths with a radius funnel (S1, Anhalt, GDC 2011; S2; S5). The route offsets corners by the radius. The public
  sources give no extra margin. Units do take tight lines round corners in the game.
  - Its pathing layer is authored separately from the cliff art, and cliff cells are unpathable in whole cells
    (S6, `pathing_grid`). How far the visible cliff face lies inside the unpathable band is **not sourced
    (verify)**. If it lies inside, a tangent body would look clear, which is consistent with the owner's
    observation.
- **The radius-aware funnel** (S11, Demyen & Buro 2006) places the path on circles of radius r round channel
  vertices. The path is tangent to obstacles by construction, so clearance beyond r must come from an explicit
  margin.
- **Recast/Detour** erodes the mesh by the agent radius (S15, `rcErodeWalkableArea`), so its paths also graze at
  exactly r. Two mechanisms keep agents off walls between replans
  (`DetourCrowd/Source/DetourCrowd.cpp`, <https://github.com/recastnavigation/recastnavigation>):
  - `dtCrowd::update` re-checks visibility from the agent's **current** position to the next corner every update
    (`optimizePathVisibility`, flag `DT_CROWD_OPTIMIZE_VIS`), and repairs the corridor (`optimizePathTopology` every
    0.5 s);
  - it adds the nearby **wall segments** to the velocity-obstacle query (`m_obstacleQuery->addSegment`, under
    `DT_CROWD_OBSTACLE_AVOIDANCE`), so steering itself avoids walls.
- **Reynolds' steering** ("Steering Behaviors For Autonomous Characters", GDC 1999,
  <https://www.red3d.com/cwr/steer/gdc99/>):
  - *path following*: the future position, predicted from the velocity, is projected onto the path spine, and the
    agent steers back once it leaves a path radius;
  - *wall following*: keep "a certain offset" from a wall;
  - *containment*: stay inside a region;
  - *obstacle avoidance*: a free cylinder ahead of the agent.

  Path following is the lateral restoring term this tree lacks (cause iv). Wall offset is the explicit clearance
  (cause iii).

**What follows.** Grazing at exactly r is the funnel's nature, in the game and in Detour. What keeps a crowd off the
walls in practice is three things:
1. re-checking the leg from where the unit actually is (B-241, B-240);
2. following the path line rather than a far point (B-242);
3. either a small clearance or steering that sees walls (B-242, Q0134).

## 7. Recommended fix

A full plan is needed (charter §6): every option moves the showcase battle and possibly golden replays. Suggested
title: `wall-clearance`. Three packages in `sim_core` route following and one constant in `sim_map`. It is
independent of `Gated` except where both touch `step_toward`.

| WP | Content | Bug | Size | Order |
|---|---|---|---|---|
| WP1 | The wedged escape in `drop_passed_waypoints` requires a real overlap, beyond rounding. The obvious line is the meter's `DEPTH_TOLERANCE`, 2⁻²⁰; it must be derived or justified, not guessed. Better still, a wedged body may pass a corner only when the leg is clear for the disc from the nearest free point. Test: case D's scene as a unit test (a body at rounding depth beside a corner keeps its arc) | B-240 | S | First; no conflict with `Gated` |
| WP2 | A held-leg check: when the body has moved off its leg's line by more than the margin (or on the hand-off cadence), test the leg from its current position with `segment_is_passable` at the terrain radius. If it fails, re-funnel from the current position, under the existing repath cooldown. This is Detour's `optimizePathVisibility` the other way round. Cost: one segment test per displaced unit-tick, measured against the throughput floor | B-241 | M | After WP1 |
| WP3 | Lateral restoring in route following: aim at a look-ahead point on the route polyline (Reynolds' path following) instead of the bare waypoint, so a displaced body regains the route's margin within a few ticks. Hand an arc run off as a whole (the switch section's lead). Then, per Q0134, the margin itself | B-242 | M | After `Gated` WP2's decision, because both change the proposal in `step_toward` |

**Acceptance.** Use this file's probe metrics on the showcase (default seed and 1 to 8):
- touching moving unit-ticks per region;
- the longest run;
- route updates whose first leg is inside a wall: 0, against 2 + 2 + 2 today;
- reversals and turns > 45° inside 1 unit of a wall;

plus the route_trace boxes and the matrix, without fitting to the showcase (the owner's 2026-11-20 guidance). Report
the presentation separately: a jitter that never moves a wall row towards the walkable side is a client-only
follow-up, not part of this fix.

## 8. Owner question (filed as [Q0134](../questions/closed/0134-wall-clearance.md))

````markdown
# Q0134: How much room should walking units keep from walls?

- **Asked:** 2026-11-29
- **Blocking:** no
- **Default until answered:** (a)

## Question

In the showcase, units graze cliffs and the central block. Their discs touch the wall at clearance 0 for up to 77
ticks (3 s), and twice a seed or so a unit aims through a corner and has to back off (evidence
`docs/evidence/2026-11-29-wall-grazing.md`). Routes keep 0.05 world units beyond the body's radius at corners; any
crowd push of more than that puts the body on the wall, and nothing brings it back. StarCraft II's published
pathing is a radius funnel with no stated extra margin, so a tangent line round a corner is the game's too. What
looks different is that our units stay on the wall and bump into corners.

- (a) Fix the defects only: no corner cut through a "wedge" of rounding depth (B-240), re-check the current leg
  after a push (B-241), and follow the route line so a pushed unit returns to the 0.05 margin (B-242). Units still
  touch walls briefly in a crowd. No deviation from StarCraft II.
- (b) (a), plus a larger route clearance, a fraction of the radius such as r/4 (0.09 for a pulse bot), so that a
  visible gap normally stays. It is recorded as a minor deviation (wider berth round corners, slightly longer
  routes); narrow gaps keep their exact fit test, so no path is lost.
- (c) (a), plus steering that sees walls (Detour-style wall segments in avoidance, or a soft band in the contact
  solver), so crowds are kept off walls actively. Most like a modern crowd system, but it is a new rule with a
  tuning constant that no StarCraft II source gives, and it costs throughput.

Recommendation: (a) first, then decide (b) on the measured result.

## Answer

````
