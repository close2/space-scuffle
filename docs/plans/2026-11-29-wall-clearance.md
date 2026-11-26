# Blueprint: Wall Clearance: No Corner Cut Through a Rounding "Wedge", the Held Leg Re-Checked, the Route Line Followed (B-240 to B-242)

This plan fixes the three route-following defects that the wall-grazing diagnosis found behind the owner's
2026-11-29 observation of the showcase video: units run against the ramp's border and against the central block
(evidence [`2026-11-29-wall-grazing`](../evidence/2026-11-29-wall-grazing.md), bugs **B-240**, **B-241** and
**B-242** in `DESIGN.md` §4). It follows owner question
[Q0134](../questions/closed/0134-wall-clearance.md) on its default, **(a): fix the defects only, no deviation from
StarCraft II**. It does not redraft Q0134.

**Status of this document.** A Stage 1 blueprint under charter §6, written on branch `wall-clearance-plan`, cut from
`wall-grazing` at `b64ccf0f` (`main` `0f0608cd` plus the findings; checksum schema 61). The file carries the
evidence's date; the session date was 2026-11-26. Line numbers drift, so search for the quoted symbol. Claims not
confirmed while planning are marked **verify**. Later decisions are appended as
`## Amendment N (orchestrator, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** Every package moves the showcase and probably golden replays and matrix battles.
The work spans four packages, and it must be sequenced around the open `Gated` movement redesign
([`2026-11-27-gated-movement`](2026-11-27-gated-movement.md)), which freezes the files it touches.

**Result in one line.** Two packages reuse predicates that already exist, so they add no constant and no hashed
state:
- WP1: a corner is handed off only along a leg the disc that fits where the body stands can walk, which is
  steering's own `probe_permits`;
- WP2: the held leg is re-checked by that predicate on the existing route-check cadence and on a refused step.

They land together in one window between `Gated` packages, and that window's measurement is also `Gated`'s one
re-base. WP3 adds route-line following (Reynolds; pure pursuit with a look-ahead of one step) and hands off an arc as
a whole. It waits until `Gated` has no package left to open. WP4, a larger margin, is built only if the owner answers
Q0134 (b).

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0134](../questions/closed/0134-wall-clearance.md): how much room from walls | Answered: (a), plus dedicated crowd tests; closed (Amendment 1) | All; WP4 not built under (a) |
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md): follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing | §3, §4 |
| [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md): sanctioned deviations | None touches route following. Under (a) none is added | §11 |
| [Q0103](../questions/closed/0103-solver-matrix-claims.md): natural movement outweighs a close battle's winner (charter §8) | Standing | §6.6 |
| [Q0124](../questions/closed/0124-new-movement-cost-budget.md) (a): ≥ 50 ticks/s on every benchmark | Standing | §6.7 |
| Q0061: hand back after `cargo gate quick`; a verifier runs `package` | Standing | §8 |
| Owner, 2026-11-20: no fitting to the test set; simpler deviations allowed; survey first | Standing | §3, §6.4 |
| `docs/questions/open/` at `b64ccf0f` | Q0131 (Carrier media; unrelated), Q0134 (this plan) | — |

---

## 1. Summary

1. **Three defects, one cause each** (evidence §2, §3).
   - **B-240.** `drop_passed_waypoints` treats a body tangent to a wall at fixed-point rounding depth as wedged. It
     then accepts a leg on its centre line, one that cuts the corner by up to 0.31. The result is a refused step
     and turns of 153° to 180° (case D).
   - **B-241.** The held leg, from the body to its first waypoint, is never checked for the disc. After a refused
     step it is checked on its centre line only. A body that crowd pushes moved off the leg's line aims through a
     corner (case A, the owner's ramp observation).
   - **B-242.** Route following aims at a waypoint up to 22 units ahead and has no term that pulls the body back to
     its line. Corners are handed off at zero clearance. A body pushed onto a wall rides it for up to 77 ticks
     (cases B and C, the owner's central-block observation).
2. **Fit with what exists.**
   - WP1 and WP2 change no rule of their own. They replace the hand-off's and the held leg's centre-line or
     zero-clearance test with the disc predicate steering already uses for every step (`probe_permits`, plan
     `2026-11-11-contact-and-missiles` D-3, R-3).
   - The hand-off and the step then agree on what the disc can walk, as plan `2026-11-19-parked-body-aim` D-1 made
     the hand-off and the parked-body aim agree.
   - WP3 closes the open lead of plan `2026-11-09-sc2-pathfinding` Amendment 3, item 2a: "a small extra corner
     margin for route following, handing an arc run off as a whole".
3. **Constants are derived, not chosen** (§4.6).
   - The tangent tolerance is `WALL_PROBE_SLACK` (2⁻¹⁴). It bounds the mesh predicate's Q16 rounding plus the
     contact rule's 2⁻²⁰.
   - The re-check cadence is `ROUTE_CHECK_PERIOD` and the re-plan rate is `REPATH_COOLDOWN_TICKS`.
   - WP3's look-ahead is the unit's step budget, the smallest pure-pursuit look-ahead that cannot overshoot the
     line. Its trigger is the route's own `CORNER_SAFETY_MARGIN`.
   - The hand-off's clearance is the funnel's leg guarantee `ARC_CHORD_DIP`.
4. **Coordination with `Gated`** (§5).
   - Every package edits `crates/sim_core/src/movement/locomotion.rs`, which `Gated` freezes while any of its
     packages is open.
   - WP0, the meter and records, touches no frozen file and runs now.
   - WP1 and WP2 run in one **window** after `Gated` WP1 merges (or ends) and before `Gated` WP2's branch is cut.
     The window is at most five working days. One measurement of the merged tree, made with `Gated`'s own WP0b
     scripts and arms, is both this plan's acceptance and `Gated`'s re-base: one re-measure, not two.
   - WP3 (and WP4) wait until `Gated` has no package left to open: after its WP4, or once the redesign ends under
     its stop rules 6 or 7. They therefore force no `Gated` re-base at all.
5. **Acceptance is pre-registered** (§6).
   - A committed meter measures touching unit-ticks, ticks within the margin, reversal events, ticks lost and
     cutting legs. It is the scratch probe, ported into `sim_fixtures` and tested.
   - It is read on the nine showcase seeds of record of both showcase scenarios.
   - Also read: the route boxes, the 13 harness families (T1 to T9, the known answers), the 24-seed matrix with
     `Gated` WP0b's frozen close-battle list, battles 31 and 33, the clashes, and throughput of at least 50
     ticks/s.
   - Fresh held-out seeds only confirm a direction.
   - One golden re-record per behaviour package. No schema change in WP1 or WP2. WP3 takes the next free schema
     number at its merge for one hashed field.

---

## 2. Starting point

### 2.1 The diagnosis (evidence, not repeated here)

Read evidence §1 to §4. In short, on `0f0608cd`, nine showcase seeds (the default and 1 to 8), 2,300 ticks:
- No disc enters a wall beyond rounding. The deepest is 2 × 10⁻⁶ (`route_trace`'s overlap meter).
- On the default seed, 276 of 8,992 moving ground unit-ticks end with a clearance below 0.005, and 1,102 inside
  the 0.05 margin.
- Over nine seeds, the ramp channels and the central block hold 3,026 touching moving unit-ticks: 58 % pushed
  there, 36 % riding the wall.
- Route updates whose first leg cuts a corner occur twice on each of the default seed and seeds 1 and 6.
- The owner's two observations are case A (rhino beetle 13, ramp W, ticks 113 to 128) and case C (the west face
  of the block, ticks 257 to 387).

### 2.2 The code paths (`crates/sim_core/src/movement/locomotion.rs` unless named)

| Symbol | What it does today | Defect |
|---|---|---|
| `drop_passed_waypoints` | Passes leading waypoints when the body is near or past them and `in_view` holds. `in_view` is the centre rule `permits_segment(p, s)` and, for a ground body, either the disc at the terrain radius fits along `p → s` (`segment_is_passable`), **or** the disc does not fit where it stands (`!segment_is_passable(p, p)`, the "wedged" escape of contact plan D-3) | B-240: a tangent body at rounding depth takes the escape. B-242: the hand-off is at zero clearance, point by point |
| `MotionProposal::pass_waypoint`, `next_waypoint` | An arc run is consecutive points within `2r + WAYPOINT_HANDOFF_MARGIN` (`same_arc`, the `reach` of `next_waypoint`) | Used by WP3 as the definition of a run |
| `route_is_usable` (called by `upkeep_route` when `route_check_due`: every `ROUTE_CHECK_PERIOD` = 8 ticks, staggered by entity ID, or on map news) | First leg: `permits_segment(position, first)`, the centre line only, on purpose ("a body pushed into a narrow gap has a first leg its disc would not fit"). Later legs: the disc, except legs leading a wedged waypoint out | B-241: a held leg that cuts a corner passes |
| `propose_step` | After a refused step, drops the route if `!permits_segment(position, next)`, the centre line | B-241: a leg through a corner keeps the route |
| `step_toward` | Heads for `next` (steering or the solver's intent), probes with `probe_permits`, slides with `slide_along_wall` | B-242: no lateral restoring term. `Gated` WP2 changes this function's proposal |
| `contact.rs` `probe_permits` | The centre rule, then the disc at the terrain radius, else at `clearance(p) − WALL_PROBE_SLACK`. A centre on the wall keeps the centre rule alone | The predicate WP1 and WP2 adopt |
| `locomotion/follow.rs` `follow`, `chase` | Call `drop_passed_waypoints`. Both re-search on any refused step (`search_follow_route`, `reroute_pursuit`) | WP1 reaches them, WP2 does not (§4.3) |
| `sim_map/src/pathfinding.rs` `CORNER_SAFETY_MARGIN` (0.05); `navmesh/funnel.rs` `ARC_CHORD_DIP` (0.025); `navmesh/search.rs` `fitted_margin` | Corner points at `r + m`; every leg keeps at least `r + m/2`; a narrow channel's fitted margin is smaller | WP4 only under Q0134 (b) |

### 2.3 The plans this builds on

- **`2026-11-09-sc2-pathfinding`.**
  - D-9 A2: a corner is passed by position plus terrain view, and bodies do not keep it.
  - A4: the corner offset is the own radius plus the margin.
  - Amendment 3, item 2a: the open lead "arc points at exactly `r + margin` whose legs graze the wall …, handing an
    arc run off as a whole". WP3 closes it.
  - Its rule "a unit pushed past its corner must not turn back to it" binds WP1 and WP3 (§4.2, §4.4).
- **`2026-11-11-contact-and-missiles`.** D-2 (non-deepening, `WALL_CONTACT_TOLERANCE`), D-3 (`probe_permits`, the
  wedged escape), R-3 (`WALL_PROBE_SLACK`).
- **`2026-11-19-parked-body-aim`.**
  - D-1: `drop_passed_waypoints` and `aim_past_parked_body` must agree on a held rounding. WP3's whole-arc hand-off
    keeps that agreement.
  - D-3: the showcase cannot be perturbed by one raw unit, so its noise is read across paired seeds (§6.4).
- **`2026-11-19-crowd-edge-steering` and `-2`.** Proposal-caused and contact-caused reversals
  (`SimWorld::proposed_step`, D-5), the showcase stop rules (D-8) and the rule-off control runs. Reused in §6.
- **`2026-11-27-gated-movement`.** The arms, the base of record, "beyond noise", the hard checks, the frozen
  close-battle list and the file freeze. §5 and §6 reuse them.

---

## 3. Survey and StarCraft II

The evidence's §6 surveys the field: StarCraft II (S1, S2, S5, S6 of the pathfinding plan), the radius funnel
(Demyen and Buro 2006, S11), Recast/Detour (S15; `DetourCrowd.cpp`) and Reynolds 1999. What follows from it for this
plan:

- **Grazing at exactly `r` is the funnel's nature**, in the game and in Detour. Under Q0134 (a) the margin stays.
  This plan does not try to keep a gap that StarCraft II does not keep.
- **Detour re-checks visibility from the agent's current position** every update (`dtPathCorridor::
  optimizePathVisibility`, `DT_CROWD_OPTIMIZE_VIS`), and repairs the corridor when it is invalid. WP2 is the same
  check the other way round: it drops a leg that has become invalid, rather than shortcutting one that has become
  free. A repair by a full re-search is simpler than Detour's corridor surgery, and the search already exists.
- **Reynolds' path following** predicts the agent's position, projects it onto the path spine, and steers to a
  point ahead on the spine once the prediction leaves the path's radius. **Pure pursuit** (Coulter, "Implementation
  of the Pure Pursuit Path Tracking Algorithm", CMU-RI-TR-92-01, 1992; **verify** the report number) steers to the
  spine point one look-ahead distance ahead. Its one parameter, the look-ahead, trades convergence against overshoot.
  WP3 uses this with the look-ahead derived in §4.6.
- **Not adopted.** Detour's wall segments in the velocity-obstacle query and a soft wall band in the contact
  solver are Q0134 (c): a new rule whose tuning constant no StarCraft II source gives. They are out of scope (§13).
- **Simplifications recorded (owner, 2026-11-20).**
  - Reynolds predicts the position from the velocity. WP3 projects the current position. Our step is decided anew
    each tick from the tick-start snapshot, and acceleration is bounded separately (`accelerated_speed`), so there
    is no inertia for the prediction to anticipate.
  - Detour's topology repair every 0.5 s is not adopted. A re-search under the 12-tick cooldown does its work.

No StarCraft II value is changed. B-240 to B-242 are defects of rules of our own (Q0051).

---

## 4. Design decisions

### D-1. The hand-off's view is the step's view (WP1, B-240)

In `drop_passed_waypoints`, `in_view` becomes:

```text
in_view = layer == Air ? permits_segment(p, s) : probe_permits(map, pathfinder, policy, p, s)
```

`probe_permits` (contact.rs) is the centre rule, then the disc at the terrain radius, and otherwise the disc of
`clearance(p) − WALL_PROBE_SLACK`. A centre on the wall keeps the centre rule. So:
- **A tangent body** (clearance `r − ε`, with ε at most the rounding of §4.6) is tested at about `r − 2⁻¹⁴`. Case D's
  leg, 0.31 inside the corner, fails, and the arc is kept.
- **A really wedged body** (a spawn, a placement, a push into a gap) is tested with the disc that fits where it
  stands. It may hand off along a leg that does not take it deeper. That is what its step may do anyway, so the
  hand-off never promises a leg the step would refuse.
- **A body clear of walls** is tested at `r`, as today. WP1 changes nothing for it.

The `!segment_is_passable(p, p)` escape is removed from `drop_passed_waypoints`. The escape of the same form in
`route_is_usable`'s loop over waypoints (legs from wedged waypoints) is not touched. Waypoints come from the
funnel, not from a body at rounding depth (§13).

A2's "a unit pushed past its corner must not turn back to it" is kept in its meaning:
- A pushed-past body that touches the wall is tested with its own disc less the slack. A leg along the wall it
  touches passes.
- Only a leg that would go deeper fails. That leg is one its step would refuse anyway.

The test `a_unit_pushed_past_its_corner_drops_it_whoever_stands_on_the_next_leg` (`route_following_tests.rs`) must
keep passing unchanged.

### D-2. The held leg is the step's too (WP2, B-241)

1. `route_is_usable`'s first-leg test `permits_segment(position, first)` becomes
   `probe_permits(map, pathfinder, policy, position, first)`.
   - The documented intent, that a body pushed into a narrow gap keeps the route it can follow, is preserved by the
     swept disc: such a body is tested with the disc that fits where it stands.
   - The rule reads: **a route is usable if the step rule lets the body walk its first leg**.
2. `propose_step`'s refusal check `!permits_segment(actor.position, next)` becomes `!probe_permits(…, next)`. A
   refused step whose leg the disc cannot walk drops the route. `upkeep_route` re-searches it next tick under
   `REPATH_COOLDOWN_TICKS`, as today.
3. **Cadence: the existing one.** The check runs where `route_is_usable` already runs: every `ROUTE_CHECK_PERIOD`
   (8) ticks, staggered by entity ID, on map news, and on every refused step. No drift trigger and no new state is
   added. The drift that matters builds up over tens of ticks (rhino 13: 1.3 units over 112 ticks, evidence §2
   case A), so an eight-tick cadence finds it long before the corner.
4. **Bound on a held cutting leg** (pre-registered, §6.3 M-5b):
   - detection waits at most `ROUTE_CHECK_PERIOD − 1` = 7 ticks;
   - a dropped route is re-searched at most `REPATH_COOLDOWN_TICKS` = 12 ticks later;
   - so no unit holds a cutting leg for more than **19 ticks**, unless the re-search itself returns one, and
     item 5 then applies.
5. **No re-search loop.**
   - A freshly searched route's first leg must pass the new test from its start: WP2's property test (§8 WP2)
     checks this.
   - Should a re-searched first leg fail anyway (a start the funnel leads out of a wedge), the cooldown already
     caps searches at one per 12 ticks per unit.
   - `path_searches` per run is reported against the base (§6.3).
6. **Walks only.** `chase` and `follow` re-search on every refused step already, and re-plan when their target
   drifts (`PURSUIT_ROUTE_DRIFT`). Their held legs are short and refreshed. They keep today's checks. WP0's meter
   reports their cutting-leg ticks separately, so a residual there is visible (§13).

### D-3. Route-line following near walls (WP3, B-242)

1. **The leg's line needs its start.** The route stores only the points ahead. `MovementMemory` gains
   `leg_from: Option<SimVec2>` (world units). It is set to the body's position when a route is adopted
   (`adopt_route`), and to the passed point when a waypoint is passed (`pass_waypoint`). It is cleared with the
   route (`drop_route`, `finish`). The field is hashed: one checksum schema step (§10).
2. **The aim.** Let `a = leg_from` and `b` the first waypoint, or the goal on the last leg. When:
   - neither an `aim_past_parked_body` aim nor a surround walk is active,
   - and the body's clearance at its terrain radius is below `CORNER_SAFETY_MARGIN`,

   the unit aims at `q + L · (b − a)/|b − a|`, where `q` is the projection of its position onto line `ab`, the
   look-ahead `L` is this tick's step budget (`context.speed`), and the result is clamped to `b`. Otherwise it aims
   at `b`, as today.
3. **Why gated by the margin.** The route keeps the disc at least `r + m` from the corner it rounds at its arc
   points, and at least `r + m/2` along the chords between them (`ARC_CHORD_DIP`). A body nearer than `m` to a wall
   has therefore left the route's line towards the wall, or is at a chord dip, where the aim lies on the line it is
   already on. In open ground the rule never fires. A crowd walking parallel lines is not drawn together onto
   converging route lines, a risk §12 R-2 records.
4. **Whole-arc hand-off.** In `drop_passed_waypoints` a run of arc points (consecutive points within
   `2r + WAYPOINT_HANDOFF_MARGIN` of each other, the existing `same_arc` definition) is passed only as a whole.
   - The view is tested from the body to the first point beyond the run, the `subsequent` that `next_waypoint`
     already computes.
   - The view is tested at the disc of radius `ρ = min(r + ARC_CHORD_DIP, c(p) − WALL_PROBE_SLACK)`, where `c(p)`
     is the body's distance to the nearest wall within `r + ARC_CHORD_DIP`.
   - Until the view holds, the run's first point is kept, and the unit walks the arc as the funnel drew it.
   - The rule reads: a hand-off never yields a leg closer to a wall than the funnel guarantees, or than the body
     already is. D-1 is its special case, with the cap at `r` instead of `r + m/2`.
   - A body parked on the run's first point keeps it, as today (parked-body-aim D-1).
   - A narrow channel whose fitted margin is below `m/2` cannot give `r + m/2`. There the unit walks each arc point,
     which only costs it the shortcut.
5. **What does not change.** `step_toward`, steering, the solver's intent, the slide, the margin, the funnel and the
   search.

### D-4. The margin (WP4, conditional on Q0134 (b))

Only if the owner answers (b):
- `CORNER_SAFETY_MARGIN` becomes a fraction of the radius, with the fraction the owner names (Q0134's example is
  `r/4`).
- The funnel's `ARC_CHORD_DIP`, `fitted_margin` and the search's `rho` follow from it as they follow from the
  constant today.
- It is recorded as a Q0052 addendum.

Under (a), WP4 is not built. Under (c), a new plan is written (§13).

### D-5. Gated coordination (§5)

The sequencing of §5 is part of the plan. Breaking it is a stop (§7, rule 9).

### D-6. Records

- B-240 is closed by WP1 and B-241 by WP2, each with its figures.
- B-242 is closed by WP3 under (a), restated as "units still touch walls briefly in a crowd" (Q0134 (a)'s wording),
  or closed by WP4 under (b).
- Each WP appends to a new evidence file, `docs/evidence/2026-11-29-wall-clearance.md`.

### 4.6 Derived constants

| Constant | Value | Derivation | Used by |
|---|---|---|---|
| Tangent tolerance | `WALL_PROBE_SLACK` = 2⁻¹⁴ ≈ 6.10 × 10⁻⁵ (existing) | Below this a body only touches the wall. The mesh predicate reduces a query point to the Q16 grid (`predicates.rs` `QPoint`, 2⁻¹⁶ per axis, at most √2 · 2⁻¹⁶ in distance) and rounds the radius test up to it (at most 2⁻¹⁶), together (1 + √2) · 2⁻¹⁶ ≈ 3.68 × 10⁻⁵. The contact rule lets a settled move end up to `WALL_CONTACT_TOLERANCE` = 2⁻²⁰ ≈ 9.5 × 10⁻⁷ deeper (contact D-2; the solver's terrain step uses the same settle). The sum, ≈ 3.78 × 10⁻⁵, is below 2⁻¹⁴, which is why contact plan R-3 chose four Q16 units. The measured deepest overlap, 2 × 10⁻⁶ over nine seeds, lies 30 times inside it. `DEPTH_TOLERANCE` (2⁻²⁰) is rejected: the measured depths exceed it, so it would leave case D a wedge. **Validity check** (§6.2 H-W1): no unit-tick's depth exceeds 2⁻¹⁴ − 3.68 × 10⁻⁵ ≈ 2.4 × 10⁻⁵. If depths accumulated beyond that, the tolerance would mislabel bodies and the check stops WP1 | D-1, D-2, D-3.4 |
| Held-leg cadence | `ROUTE_CHECK_PERIOD` = 8 ticks (existing) | The cadence of route checks. Its latency is far below the drift time that makes a leg cut (D-2.3) | D-2 |
| Re-plan rate | `REPATH_COOLDOWN_TICKS` = 12 (existing) | The cap on searches per unit | D-2 |
| Held cutting leg bound | 19 ticks | `ROUTE_CHECK_PERIOD − 1 + REPATH_COOLDOWN_TICKS` | §6.3 M-5b |
| Look-ahead `L` | the step budget `context.speed` (world units per tick) | Aiming at the line point `L` ahead of the projection from lateral error `d` moves the body `s·d/√(d² + L²)` towards the line in one step of length `s`. This is below `d` for every `d` exactly when `L ≥ s`, so `L = s` is the smallest look-ahead that never overshoots the line: no wobble across it. The error then falls by `s/√(d² + s²)` of itself per tick (about all of it for `d ≪ s`, a full step sideways for `d ≫ s`). A larger `L` only slows the return, the case-C failure (`L` = 9 units today) | D-3.2 |
| Trigger | clearance < `CORNER_SAFETY_MARGIN` (0.05, existing) | The route's designed clearance (D-3.3) | D-3.2 |
| Whole-arc hand-off clearance | `r + ARC_CHORD_DIP` (0.025, existing), capped at `c(p) − WALL_PROBE_SLACK` | The funnel's guarantee for every leg, and the non-deepening view of D-1 | D-3.4 |
| Arc run | `2r + WAYPOINT_HANDOFF_MARGIN` (existing) | The `same_arc` and `next_waypoint` reach already in the code | D-3.4 |

Each value is an existing named constant or follows from one. No package may add a constant, tolerance or threshold
chosen after a trace (§7, rule 6).

---

## 5. Coordination with `Gated`

### 5.1 What `Gated` freezes, and where each package writes

`Gated` §7: "No other package edits `movement/`, `steering.rs`, `target_acquisition.rs`' admission test or
`movement_rules.ron` while a package of this plan is open. A bug fix that must touch them lands on `main` first, and
this plan rebases." Its H-6 also requires `Solver` and `Legacy` worlds byte-identical to its base of record while
`Gated` is latent. Any merge here that moves a world therefore forces a re-base of `Gated` (`Gated` §6.1, "re-measured
only if `main` changes simulation code under the plan; the step is then recorded in an amendment").

| Package | Files | Frozen by `Gated`? | Overlap with a `Gated` package's own files (§14) |
|---|---|---|---|
| WP0 | `crates/sim_fixtures/src/trace/{route_clearance.rs (new), route.rs, mod.rs}`, `crates/sim_core/examples/route_trace.rs` (doc of the new outputs only), `crates/sim_fixtures/tests/trace_tests.rs`, `crates/sim_core/tests/route_following_tests.rs` (scene helpers only, no new assertion on `main`), evidence (new), `DESIGN.md` §4 (B-240 to B-242 links; B-243 if filed), this plan | **No** | None |
| WP1 | `crates/sim_core/src/movement/locomotion.rs` (`drop_passed_waypoints` and its doc), `crates/sim_core/tests/route_following_tests.rs`, `crates/sim_core/tests/fixtures/golden_replay*.ron`, `crates/sim_fixtures` `DEFAULT_SANDBOX_CHECKSUM` (if moved), evidence, `DESIGN.md` §4 | **Yes** (`movement/`) | `locomotion.rs` (`Gated` WP1, WP2, WP3a, WP3b); goldens (WP2, WP3b, WP4) |
| WP2 | `locomotion.rs` (`route_is_usable`, `propose_step` and their docs), `route_following_tests.rs`, goldens, sandbox checksum (if moved), evidence, `DESIGN.md` §4 | **Yes** | As WP1 |
| WP3 | `locomotion.rs` (`next_waypoint`, `drop_passed_waypoints`, `MotionProposal::{adopt_route, pass_waypoint, drop_route}`), `movement/route_line.rs` (new: the projection and look-ahead, with unit tests), `movement/mod.rs` (module line), `crates/sim_core/src/entity.rs` (`MovementMemory::leg_from`), `checksum.rs` and `checksum/encoding.rs` (schema), `crates/sim_core/tests/{route_following_tests,checksum_state_tests,golden_replay_tests}.rs`, goldens, sandbox checksum, evidence, `DESIGN.md` §4 and §5 | **Yes** | `locomotion.rs`, `entity.rs`, `checksum*`, goldens (WP2, WP3b, WP4) |
| WP4 (if Q0134 (b)) | `crates/sim_map/src/pathfinding.rs`, `navmesh/{search,funnel}.rs`, `sim_map` tests, goldens, evidence, Q0052 addendum, `DESIGN.md` | No (`sim_map`), but it moves every world | Goldens |

### 5.2 Sequencing

`Gated`'s order is WP0a → WP0b → WP1 → WP2 → WP3a → WP3b → WP4. At this writing WP0a and WP0b are done and WP1 is
open on `gated-wp1`. WP1 has stopped twice and has an Amendment 3b pending (`gated-wp1` `6f273f7c`; **verify** its
state with the orchestrator at the window's start).

| When | This plan | Why |
|---|---|---|
| **Now**, while `Gated` WP1 is open | **WP0** (meter, scenes, records). Then the base figures of the new meter on the base tree (§6.1). The red tests of WP1 and WP2 are written on this plan's branch, where they stay red until the window | WP0 touches no frozen file and moves no world (its outputs are new files; §8 WP0 proves identity) |
| **Window W** (opens when `Gated` WP1 merges to `main`, or is closed unmerged; closes when `Gated` WP2's branch is cut; at most **five working days**, agreed here) | **WP1 then WP2**, merged to `main` one after the other, then **one measurement** of the merged tree (§5.3) | WP1 (S) and WP2 (S to M) are small, their tests and scenes are ready from WP0, and they change no proposal: `Gated` WP2's gate and intent see the same `step_toward` |
| **If W expires** before WP2 merges | WP1 and WP2 that have not merged yield to `Gated` WP2, and move to window X with WP3. A merged WP1 alone is a valid re-base point: §5.3 is then run on WP1's tree | `Gated` is not held longer than agreed |
| **Not** between `Gated` WP2 and WP3a, WP3a and WP3b, or WP3b and WP4 | Nothing | Each would force a second `Gated` re-base and change the proposal that `Gated` WP3a, WP3b and WP4 are judged on |
| **Window X** (after `Gated` WP4 merges, or after `Gated` ends under its stop rule 6 or 7, or when the orchestrator closes it) | **WP3**, then **WP4** if Q0134 is answered (b) | WP3 changes the aim fed to `step_toward`, the function whose proposal `Gated` WP2 redesigns (evidence §7). After `Gated` there is nothing left to re-base |

The evidence recommended WP3 "after `Gated` WP2's decision". The freeze lasts while any `Gated` package is open,
and a world move between `Gated` packages costs a re-base. So WP3 waits for window X, not for WP2's decision alone.

### 5.3 One measurement, two uses: no double re-base

1. **This plan's base for WP1 and WP2 is `Gated`'s base of record.** `Gated` WP1 is latent, and its H-6 proves the
   `Solver` and `Legacy` worlds byte-identical to the base on its tree. Window W's start tree (`main` with `Gated`
   WP1) therefore has `Gated`'s base for arms S, R and L.
   - Only the new meter's figures, which `Gated` never measured, are measured on the base, by WP0.
   - If `main` moved simulation code since `Gated`'s last re-base (for example the Broodlings merge `0f0608cd`,
     which moved battle 20 after WP0b's final tree; **verify** whether `Gated` recorded it), WP0 measures the moved
     worlds once with `Gated`'s scripts. `Gated` adopts them as its base in the same amendment as item 3.
2. **The window's measurement** runs once, on WP2's merged tree (or WP1's, if W expires after WP1 alone), with
   `Gated`'s own WP0b instruments: `~/scratch/gated-movement/scripts/measure.sh`, the same binaries' build recipe,
   arms L, S and R from scratch catalog directories, the same seeds and the same "beyond noise" rule. It adds:
   - **arm G**, `Gated` latent at WP1's tree, run on its hard checks only (H-1, H-2, H-5), reported and not
     judged;
   - the new meter's columns.
3. **The same tables are `Gated`'s re-base.** The orchestrator records them in a `Gated` amendment ("base re-measured
   on `<tree>` after wall-clearance WP1 and WP2"), with the throughput table of `Gated` §9 (C-1's R and C-2's S and
   L references). `Gated` WP2 is then judged against them, and nothing is measured twice.
4. **WP3 and WP4 run after `Gated`**, so they never touch `Gated`'s base. Their base is `main` at window X's start.
   Its matrix, harness and throughput are `Gated` WP4's own acceptance tables if WP3 starts on that tree, and are
   measured once by WP3's WP-start step otherwise.

---

## 6. Acceptance (pre-registered; binding on every package)

### 6.1 The meter (WP0): `sim_fixtures::trace::route_clearance`

The scratch probe (`~/scratch/wall-grazing/probe/src/main.rs`), ported into `sim_fixtures` as a meter of
`run_route_trace` beside `OverlapMeter`. It follows `route_overlap.rs`'s pattern:
- it reads only positions, proposals and routes, and its queries use the pathfinder's own scratch buffers, which no
  simulation state reads;
- floats are used for reporting only;
- its outputs are new files (`clearance.csv`, `clearance_episodes.csv`, `clearance_routes.csv`,
  `clearance_summary.md`). Existing outputs stay byte-identical.

Definitions (world units, 25 Hz ticks):

| Name | Definition |
|---|---|
| Walking unit-tick | A living mobile ground unit, after a tick, whose proposal (`SimWorld::proposed_step`) is present and non-zero, or whose achieved step is longer than `REVERSAL_MIN_STEP` (0.005, the blocking meter's) |
| Clearance | `Pathfinder::wall_contact` distance at the unit's terrain radius plus 1.5 (the probe's reach), minus the terrain radius, under the physical request (true terrain, every structure, no vision) |
| **M-1 touching** | Walking unit-ticks with clearance ≤ `WALL_PROBE_SLACK` (2⁻¹⁴, §4.6). The evidence's 0.005 count is reported beside it for continuity |
| **M-2 within the margin** | Walking unit-ticks with clearance < `CORNER_SAFETY_MARGIN` |
| **Episode** | A maximal run of consecutive M-2 unit-ticks of one unit. Reported: its length, region, push/aim/ride class per tick (the probe's classes, evidence §1), and the longest run per region |
| **M-3 reversals at a wall** | `blocking::is_reversal` of two consecutive achieved steps (cosine < `REVERSAL_COSINE`, both longer than `REVERSAL_MIN_STEP`) where either tick is M-2. Split into proposal and contact reversals as crowd-edge D-5 does |
| **M-4 ticks lost** | Per episode: `max(0, length − |net displacement| ÷ top speed per tick)`, top speed the archetype's. Summed per region |
| **M-5a cutting route updates** | Route changes after which the first leg from the unit's position fails `segment_is_passable` at `r − WALL_PROBE_SLACK` while the unit's own clearance is at least `−WALL_PROBE_SLACK`. Exact predicate, no sampling; the sampled depth (step 0.02) is reported for reading |
| **M-5b held cutting legs** | Walking unit-ticks whose held first leg fails the same test, as episodes. Walks, chases and follows are reported separately |
| Regions | Default boxes, the evidence's: ramp W channel (24.5–30.5, 6.5–13), ramp E channel (33–39.5, 51–57.5), central block (24–39, 24–39), elsewhere. The boxes can be overridden from the command line, as `--box` is today. They are reporting regions: every figure is also given for the whole map |
| Depth check | The largest depth (`−clearance`) of any unit-tick, for H-W1 |

Tests (`crates/sim_fixtures/tests/trace_tests.rs`, scratch worlds in the file):
- a unit walking along a wall at a known clearance gives M-1 or M-2 exactly;
- a body pushed onto a wall and riding it gives one episode of known length and its M-4;
- a reversal at the wall is counted, and one in open ground is not;
- a route whose first leg cuts a corner is M-5a and M-5b, and one that grazes at exactly `r` is not;
- the meter leaves `units.csv`, `passes.csv`, `overlap.csv` and `summary.md` byte-identical with and without it.

### 6.2 Hard checks (never traded)

| ID | Check | Where | Since |
|---|---|---|---|
| **H-W1** | Depth validity: no unit-tick deeper than 2.4 × 10⁻⁵ (§4.6) | showcases, harness families (T7's meter) | WP1 |
| **H-W2** | No body deeper: `route_trace` overlap meter's deeper unit-ticks 0 on every showcase seed; harness H-5 (disc penetration ≤ 2⁻¹⁰) | showcases, harness | WP1 |
| **H-W3** | Determinism: goldens and pinned digests identical on native debug and release and `wasm32-wasip1` release and debug | goldens | every behaviour package |
| **H-W4** | Wiring: a scratch build of the package with its rule switched off (a scratch flag, never committed) reproduces the base byte for byte on the showcase's `units.csv` and the matrix | showcase, matrix | every behaviour package |
| **H-W5** | `Gated`'s hard checks under arm G (H-1, H-2, H-5) are no worse than at `Gated` WP1's merge. A failure this plan causes is its defect, and a stop | `Gated` §6.2 scenes | window W |
| **H-W6** | Give-ups (stall `NoProgress`) on every showcase seed at most the base's + 1, and matrix totals at most the base's (crowd-edge D-8 rule 2) | showcases, matrix | every behaviour package |

### 6.3 Figures, pre-registered per package

"Base" is §5.3's base for WP1 and WP2 and window X's start tree for WP3 and WP4. Showcase figures cover both
`scenarios/showcase.ron` and `scenarios/showcase_effects.ron`, on the default seed and seeds 1 to 8, played to the end
or tick 3,000.

| Package | Target (must hold) | Predicted | Not worse than base beyond noise |
|---|---|---|---|
| **WP1** | M-5a = 0 on all nine seeds of both scenarios (base: 2 on each of the default seed and seeds 1 and 6). The case-D unit test (§8 WP1) passes | First divergences at the default seed's tick 686 and seed 1's tick 717, seed 6's tick 630 (evidence case D). A first divergence elsewhere is traced and explained before acceptance. The matrix moves little or not at all | M-1 to M-4 on the nine seeds; route boxes; harness T1 to T9 |
| **WP2** | M-5b: no walk episode longer than 19 ticks (§4.6). Case A's unit test (§8 WP2) passes. On the default seed, no M-3 reversal for rhino beetle 13 in the ramp W channel over ticks 100 to 160 (base: 177.5° at tick 118) | M-4 in the ramp channels falls. `path_searches` rises by the re-plans of dropped held legs | M-1 to M-4 elsewhere; route boxes; harness; the matrix as §6.6 |
| **WP3** | In the central block and both ramp channels, M-2 per seed falls beyond noise (paired, §6.4), and so does the longest episode. M-1 does not rise. M-5a stays 0, and M-5b stays within 19 ticks | M-2 in the central block falls by most of its "push" and "ride" classes (96 % of the block's touching unit-ticks in the evidence). Brief touches remain in crowds (Q0134 (a)) | M-3 and M-4 everywhere; route boxes; harness, especially T1 (churn, M1 and M1-hi) and T7 |
| **WP4** (if (b)) | M-1 in the three regions falls beyond noise against WP3's tree | Routes slightly longer: route-box ratios may rise, reported | As WP3, route-box ratios reported |

**Route boxes** (`route_trace`'s `ramp_w_foot`, `plateau_se`, `ramp_e_foot`): walked/chord ratio and total turn per
owner. Reversals per owner on the default seed must not exceed the base's (crowd-edge D-8 rule 4). Seeds 1 to 8 are
reported, and a rise above the base + 5 for an owner is traced.

**Harness families** (`movement_harness --aggregate`, development seeds 1 to 1,000, all 13 families and the mirror,
arms S, R and L): `Gated` §6.3's step rule against the base's S. A threshold S meets must stay met, and a figure S
fails must not get worse beyond noise. The known answers of `Gated` §5.2 hold at their base rates.

**Throughput** (§6.7) and **the matrix** (§6.6) for every package.

### 6.4 Seeds and noise

- **Judging.** Showcases: the default seed and seeds 1 to 8, both scenarios. Harness: development seeds 1 to 1,000.
  Matrix: the 24 judging seeds of `Gated` §6.1. Battles 31 and 33 and the clashes as in `Gated` §6.1.
- **Directional claims on the showcase** (the falls in §6.3) are read on 33 paired seeds (the default and 1 to 32),
  as parked-body-aim D-3 established: one raw unit cannot perturb the showcase, so there is no placebo band. A fall
  holds when the mean paired difference lies below zero outside its 95 % bootstrap interval (2,000 resamples, seed
  7) and more seeds fall than rise. Hard targets (M-5a = 0, the 19-tick bound) are judged on the nine seeds.
- **Beyond noise** elsewhere: `Gated` §6.1's rule (the candidate's per-seed mean outside the base's 95 % bootstrap
  interval of the mean, on the worse side).
- **Held-out seeds, registered now and run once, only to confirm a direction.** They are fresh and do not overlap
  `Gated`'s acceptance (300,001 to 301,000) or held-out ranges (9,100,001 to 9,101,000; the matrix tool's 45th to
  68th non-record seeds), which this plan never runs:

  | Set | Used at | Showcases | Harness | Matrix |
  |---|---|---|---|---|
  | **HA** | window W's measurement (WP1 + WP2) | seeds 1,001 to 1,024 | 9,200,001 to 9,201,000 | the tool's 69th to 92nd non-record seeds (**verify** the selection flag `Gated` uses) |
  | **HB** | WP3's acceptance (WP4 reuses it only if WP3's run did not read it) | 1,025 to 1,048 | 9,201,001 to 9,202,000 | 93rd to 116th |

  A held-out run that contradicts the development direction of a judged figure stops the package (§7, rule 7).
  Nothing is tuned or widened from held-out seeds.
- Every measurement runs alone under `flock ~/scratch/gate.lock`. Scratch material lives under
  `~/scratch/wall-clearance/`.

### 6.5 Goldens and schema

- **One golden re-record per behaviour package** (WP1, WP2, WP3, and WP4 if built), on the package's final tree:
  `golden_replay` and `golden_replay_orders` in native debug and release, checked on `wasm32-wasip1` release and
  debug under `wasmtime --dir=<real path>`. `DEFAULT_SANDBOX_CHECKSUM` is re-recorded only if it moves.
- The header states the plan, the WP, the first changed checkpoint, and the entity and tick with what the rule did.
  It ends with "Proof: a scratch build of WP<n> with the rule off reproduces the previous checksums at every
  checkpoint" (H-W4).
- A golden that does not move is recorded as a finding.
- **Schema.** WP1 and WP2 add no hashed state, so the schema stays at `main`'s. WP3 adds `MovementMemory::leg_from`
  and takes **the next free schema number at its merge**: `main` is at 61, and `Gated` WP2 (`gate_side`) and WP3b
  (`ring_retarget`) take theirs at their own merges first. WP4 adds none.

### 6.6 Battles and claims (charter §8, Q0103)

- **The close-battle list is `Gated` WP0b's, frozen** (`docs/evidence/2026-11-27-gated-movement.md` §3.3): 8, 9b, 10,
  11a, 14, 21, 22a, 26, 27, 28, 31, 32, 33, 35, 36a, 36b, 37a, 37b, 38a, 38b, 39a, 39b, 42. Every other battle is
  Class D. This plan does not recompute it.
- **A moved battle** (any change to the matrix CSV) gets a first-difference trace of one moved seed: the first tick
  and entity, and the rule that acted (hand-off kept, leg dropped, line aim). Battles are expected to move
  (terrain battles and anything with a corner). A move is not a failure in itself.
- **A winner that flips** on a seed of record, or a battle's majority outcome that changes over the 24 seeds:
  1. the trace shows a broken behaviour (standing with room, oscillation, a wedge entered, a hard check, a
     corner cut): **a bug and a stop**;
  2. a clean trace in Class R: the claim is restated under §8, with the fixture comment citing AGENTS.md §8, Q0103,
     this plan and the mechanism, and survivor ranges restated from the 24 seeds;
  3. a clean trace in Class D, whose claim is a StarCraft II statement: the owner is asked (Q0135, §11.2), and the
     battle meanwhile takes `known_bug` for the package's bug;
  4. a clean trace in Class D otherwise: restated as in item 2.
- **Battle 28 keeps its `known_bug: Some("B-192")` marker** (`Gated` owns its removal). This plan must not make it
  hold or fail by accident. A change of its verdict is traced, and the trace goes to `Gated`'s evidence.
- **Held-out:** every restated claim holds on at least two thirds of the package's held-out matrix seeds (HA or HB).
  Otherwise the package stops, and the claim is rewritten from both ranges in an amendment.
- The showcase's default-seed winner is reported, not judged (Q0103). A flip there gets the same trace rule.

### 6.7 Throughput and cost (Q0124)

- **Floor:** every scenario of `cargo bench -p sim_core --bench throughput` (64 × 64, 128 × 128, the canyon, the
  casters, the hangar) runs at **≥ 50 ticks/s under every arm**, measured as `Gated` §7 does (`perf stat -e
  instructions:u`, three runs, load below 8 at both readings). Under R the canyon is the tightest: 55 ticks/s at
  `Gated` WP0b.
- **Cost is predicted before it is measured.** WP0 attributes the base's instructions with callgrind on the canyon
  scenario:
  - the share of `route_is_usable`, and its segment tests per call (mean legs per checked route);
  - the share of `drop_passed_waypoints`;
  - the share of the settle's `wall_contact`.

  Each behaviour package states its predicted change from that attribution before its measurement:
  - WP1: none beyond noise, since `probe_permits` asks the same query as today's first test and differs only after
    it fails;
  - WP2: one disc segment test more per route check;
  - WP3: one `wall_contact` per walking unit-tick at radius `r + m`, unless shared with the settle (§8 WP3).

  A measured cost above the prediction is explained in the evidence. The **binding rule is the floor**, which is the
  owner's.
- `Gated` C-2 (S and L within noise of the base while latent) is restated by §5.3's re-base, not broken: the re-base
  is its new reference.

---

## 7. Stop rules (in advance)

1. **A hard check (§6.2) fails:** stop. H-W4 failing is a wiring defect, which is found and fixed; it does not
   count as an amendment.
2. **A §6.3 target fails** after the rule is shown correct by its tests: stop with the figures.
3. **The step rule** (harness, route boxes, M-1 to M-4 where "not worse") fails: stop.
4. **A flipped winner with a broken trace** (§6.6, item 1): stop.
5. **Throughput below 50 ticks/s** in any scenario and arm: stop, and report the attribution.
6. **One amendment per package.** It must name a literature source (§3) or an earlier measured rule of this project.
   No constant, tolerance, hysteresis or tie-break may be chosen after a trace (`Gated` §6.6, rule 5; charter §1.1).
   A second failure ends the package. Its bug stays open with the figures, and the orchestrator decides with the
   evidence whether the next package proceeds. WP3 needs neither WP1 nor WP2 to have merged.
7. **Held-out contradicts development** (§6.4) or a restated claim fails two thirds of the held-out seeds: stop.
8. **WP3 fails twice under Q0134 (a):** the figures go to Q0134 as an addendum for the owner, who decides between (b)
   and (c). Nothing is built meanwhile.
9. **Sequencing (§5.2):** a frozen file edited outside windows W and X, or a merge between `Gated` packages other
   than in window W, is a process breach. The merge is reverted, and the orchestrator re-plans the window.
10. **Not allowed in any package:**
    - a change to the funnel, the search, the margin (except WP4 under (b)), the contact rule, the solver, steering,
      or the intent;
    - a per-unit, per-family, per-battle or per-map switch;
    - a rule fitted to the showcase: regions are reporting boxes, never rule inputs (owner, 2026-11-20).

---

## 8. Work packages

Every package follows the charter's routine:
- tests red first on the package's base, with each red run recorded in the evidence;
- `cargo gate quick` and the tests the change needs, then hand back;
- a verifier runs `cargo gate package` on the final tree (Q0061);
- the orchestrator runs `cargo gate merge`.

Builds use `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`. One building agent at a time, which also
means none beside a `Gated` package's. Sizes: S ≤ 1 day, M 2 to 3 days.

### WP0: the meter, the scenes, the base figures (M; no behaviour change; runs now)

- **Code.**
  - `crates/sim_fixtures/src/trace/route_clearance.rs` (new): the meter of §6.1, `ClearanceMeter` with `observe`,
    per-owner and per-region figures, episodes, and route events.
  - `route.rs`: wire it into `run_route_trace`. Write the four new files and add `--clearance-box` (default: the
    evidence's regions). Existing outputs are unchanged.
  - `mod.rs`: the module line.
  - `route_trace.rs` (example): the usage text names the new outputs.
- **Tests.** §6.1's five tests in `trace_tests.rs`.
- **Scenes for WP1 and WP2**, as helpers in `route_following_tests.rs` (a map with a convex corner, as
  `corner_world` has). They are committed without new assertions on `main`.
  - Case D: a body at rounding depth tangent beside a corner, whose route is the arc round it and then a far point.
  - Case A: a body displaced laterally off its leg, so that the leg from its position passes inside `r` of a corner
    ahead.
- **Red tests**, written on this plan's branch and not merged before window W. They are listed in WP1 and WP2.
- **Base figures.** On the base tree, the meter on both showcase scenarios, nine seeds and seeds 9 to 32 (the
  directional sample, §6.4), and the cost attribution of §6.7. If `main` moved worlds since `Gated`'s last re-base,
  the moved worlds are measured once (§5.3, item 1).
- **Records.**
  - The evidence file `docs/evidence/2026-11-29-wall-clearance.md` (new): method and base.
  - `DESIGN.md` §4: B-240 to B-242 point to this plan. B-243 is filed only if WP0's mesher check (§11.1) confirms
    it.
- **Acceptance.**
  - Every existing `route_trace` output is byte-identical with the meter on, on the default seed and seeds 1 to 8.
  - The meter reproduces the evidence's figures within its definitions. The probe's 0.005 and 0.01 counts are
    recomputed beside M-1 and M-2, and any difference is explained.
- **Merge.** WP0 may merge to `main` while `Gated` WP1 is open: no frozen file, no world moved.

### WP1: the hand-off's view is the step's view (S; B-240; window W)

- **Red first** (`route_following_tests.rs`):
  - `a_body_resting_on_a_wall_at_rounding_depth_keeps_the_arc_round_the_corner` (case D: the arc is not handed off;
    the unit's next aim is the arc point; over the following ticks no step reverses);
  - `a_wedged_body_hands_off_only_along_a_leg_its_fitting_disc_can_walk`;
  - the existing `a_unit_pushed_past_its_corner_drops_it_whoever_stands_on_the_next_leg` and
    `a_column_rounds_a_shared_corner_directly` stay green unchanged.
- **Code.** D-1: `in_view` in `drop_passed_waypoints` uses `probe_permits`. The escape and its comment are replaced
  by a doc that cites this plan, contact D-3 and §4.6's tolerance.
- **Measure** (light; the full run is window W's): the meter on both showcases, nine seeds; the matrix identity or
  moved battles traced; H-W4.
- **Records.** Golden re-record (§6.5). Evidence §WP1. B-240 closed with M-5a and the case-D trace.

### WP2: the held leg is the step's too (S to M; B-241; window W, after WP1)

- **Red first** (`route_following_tests.rs`):
  - `a_body_pushed_off_its_leg_replans_before_it_reaches_the_corner` (case A: the route is dropped within
    `ROUTE_CHECK_PERIOD` ticks of the leg starting to cut, a new route rounds the corner, and the body never touches
    the corner vertex);
  - `a_refused_step_whose_leg_the_disc_cannot_walk_drops_the_route`;
  - `a_body_pushed_into_a_narrow_gap_keeps_a_route_it_can_follow` (the documented intent of `route_is_usable`);
  - a property test, `a_fresh_route_passes_its_own_held_leg_test`: on the showcase and canyon maps, starts at
    tangent and wedged positions (from a seeded generator in the test), the first leg of every searched route
    passes `probe_permits` from its start (D-2.5).
- **Code.** D-2, items 1 and 2.
- **The window's measurement** (§5.3) on WP2's merged tree: arms L, S, R, plus G for its hard checks; HA run once;
  throughput.
- **Records.** Golden re-record. Evidence §WP2 with the full tables. B-241 closed. The tables are handed to the
  `Gated` orchestrator for the re-base amendment.

### WP3: route-line following near walls, the whole-arc hand-off (M; B-242; window X)

- **Base.** Window X's start tree (§5.3, item 4). The meter on it (nine seeds and 9 to 32), and the cost
  attribution re-run.
- **Red first:**
  - `route_line.rs` unit tests: projection onto a leg, the look-ahead point, clamping at `b`, a degenerate leg
    (`a = b`), and no overshoot (the lateral error after one step is in `[0, d)` for a grid of `d` and `s`);
  - `route_following_tests.rs`:
    - `a_body_pushed_onto_a_wall_returns_to_its_route_line` (case C: a body displaced onto the face regains
      clearance ≥ `CORNER_SAFETY_MARGIN − WALL_PROBE_SLACK` within `⌈d/s⌉ + 1` ticks when nothing pushes it);
    - `an_arc_is_handed_off_only_as_a_whole_and_never_at_less_than_the_funnels_clearance`;
    - `a_body_in_open_ground_aims_at_its_waypoint_as_before` (the trigger does not fire away from walls);
    - `a_rounding_held_by_a_parked_body_keeps_its_arc` (parked-body-aim D-1);
  - `checksum_state_tests.rs`: `leg_from` is encoded and decoded.
- **Code.** D-3: `leg_from`, `route_line.rs`, `next_waypoint`'s aim, `drop_passed_waypoints`' whole-run hand-off,
  the schema.
  - The trigger's `wall_contact` at `r + m` may be shared with the settle's query if §6.7's prediction says the
    cost needs it. Such sharing carries a `PERFORMANCE RATIONALE:` block (charter §2.3).
- **Measure.** Every §6 item against window X's base. HB run once.
- **Records.**
  - Golden re-record. Schema next free.
  - Evidence §WP3.
  - B-242 closed under (a) with Q0134 (a)'s wording "units still touch walls briefly in a crowd", or left open
    with the figures if WP3 stops (§7, rule 8).
  - An addendum to plan `2026-11-09-sc2-pathfinding` (Amendment 3, item 2a's lead closed).
  - The figures added to Q0134 as an addendum for the owner's (b) decision.

### WP4: the margin (S to M; only if Q0134 is answered (b); window X, after WP3)

- **Code.** D-4.
- **Tests.**
  - `sim_map`'s funnel and search tests at the new margin: every arc point at `r + m(r)`, every leg at
    `r + m(r)/2`, and the fitted margin in narrow channels.
  - A pair of narrow gaps where the disc fits at `r` but not at `r + m(r)`, which must still be routed (Q0134 (b):
    "narrow gaps keep their exact fit test").
- **Measure.** As WP3, against WP3's tree, with HB if WP3's run did not read it, else a fresh set registered in an
  amendment before the run.
- **Records.** The Q0052 addendum. Golden re-record. B-242 closed.

### WP5: bookkeeping (S)

- The plan's row in `docs/plans/README.md`, the plan's "As built" amendment, and `DESIGN.md` §5's throughput
  figures if moved.
- The showcase storyboard (`assets/captures/showcase.ron`) is re-checked only if a scene's frame moves. A render
  needs the GPU lock and follows `docs/guides/gpu-and-disk.md`.

---

## 9. File ownership

§5.1's table. Files outside it are not edited. `Gated` owns the files of its open package, and the two plans
exchange only through `main` at the windows.

## 10. Determinism, checksum, throughput

- Every new computation is integer fixed point on existing predicates: `probe_permits`, `segment_is_passable`,
  `wall_contact`, and the checked vector operations of `movement/math.rs`. No float enters the simulation crates.
  The meter's floats are in `sim_fixtures`, for reporting only.
- No `HashMap`, no clock, no new randomness. Iteration orders are unchanged.
- WP1 and WP2 change no state layout. WP3 adds one hashed `Option<SimVec2>` (schema next free).
- Throughput as §6.7.

## 11. Records

### 11.1 Bugs

| Row | Action |
|---|---|
| B-240 | Closed by WP1 |
| B-241 | Closed by WP2 |
| B-242 | Closed by WP3 under (a) (restated) or WP4 under (b); open with the figures if WP3 stops |
| **B-243** (draft, provisional, filed by WP0 only if confirmed) | "The cliff mesh's wall-row jitter (`WALL_JITTER_AMPLITUDE`, up to 0.06) can move a wall row towards the walkable side, so a body the simulation holds tangent to the wall is drawn dipping into the cliff" (evidence §2, "Not a cause"). WP0 checks it on the CPU mesher, with no render: the largest displacement of a wall row towards the walkable side over both showcase maps. If it is above zero, B-243 is filed as a client follow-up, deferred, out of this plan. Otherwise it is not filed |

New numbers start at B-243. Any further row takes the next free number at its merge.

### 11.2 Questions

- **Q0134** stays open on its default (a). It is referenced, not redrafted. WP3 adds the measured figures as an
  addendum.
- **Q0135 (draft, provisional, filed only on the event of §6.6, item 3):**

````markdown
# Q0135: Following routes more cleanly changes who wins a battle whose claim is a StarCraft II fact

- **Asked:** <date>
- **Blocking:** no
- **Default until answered:** the battle is marked with the package's bug as an expected failure; nothing else
  changes

## Question

Units now keep off walls as their routes intend (plan `2026-11-29-wall-clearance`, <WP>). In battle <name>, whose
claim "<claim>" is StarCraft II's, the winner changed on <k> of 24 seeds. The trace (<evidence link>) shows no broken
behaviour: <mechanism>.

- (a) Keep the claim and mark the battle as an expected failure, then look for what else differs from the game.
- (b) Restate the claim as a minor deviation (charter §8, Q0103's guidance), with the new range.

Recommendation: <one of them, with the reason>.

## Answer

````

### 11.3 Deviations

None under (a). WP4 under (b) adds one Q0052 addendum: "wider berth round corners: the route keeps `r + f·r`
instead of `r + 0.05`".

## 12. Risks

| ID | Risk | Mitigation |
|---|---|---|
| R-1 | WP2 re-plans too often: a held leg in a crowd keeps failing, and searches rise | The cooldown caps searches per unit (D-2.5); the property test; `path_searches` reported; the cost floor |
| R-2 | WP3 draws walking crowds near a wall onto converging route lines, so friend pressure near corners rises (T1 churn, F-figures) | The trigger confines the rule to bodies within the margin of a wall (D-3.3); T1 and the friend-overlap figures are judged; one amendment, then Q0134 (§7, rule 8) |
| R-3 | The tolerance mislabels a body that the settle let sink beyond rounding | H-W1 checks the bound on every run |
| R-4 | Window W slips because `Gated` WP1 stays open | WP0 runs now; W's start waits for `Gated`; on expiry WP1 and WP2 move to window X (§5.2) |
| R-5 | A narrow channel's fitted margin below `m/2` stops whole-arc hand-offs, so units walk every arc point | Correct by construction (the funnel's own points); route-box ratios and M-4 judged |
| R-6 | WP1 changes hand-offs for really wedged bodies (spawns, unloads) and slows their exit | Their hand-off then matches their step, which `probe_permits` already decides; harness families with spawns judged; H-W6 |
| R-7 | `Gated` WP2's later gate interacts with WP2's dropped routes | WP2 changes no proposal. `Gated` WP2 is judged on the re-based tables that include WP2 (§5.3) |

## 13. Out of scope (charter §8: scoping recorded)

- **Q0134 (c)**, steering that sees walls (Detour's wall segments, a soft band in the solver): a new plan if the
  owner chooses it.
- **The crowd push itself** (D-β, B-220): `Gated` (C′-3 terrain transfer, Q0122).
- **Held-leg checks for chases and follows** (D-2.6): reported by the meter (M-5b split). Filed as a bug only if
  measured above the 19-tick bound.
- **The `route_is_usable` wedged-waypoint escape**: waypoints come from the funnel and are not at rounding depth.
- **The presentation** (B-243 if filed): client.
- **The video** (evidence §5): a render for the owner after WP3 is a storyboard task (WP5), not acceptance.

## 14. Material (scratch, not committed)

`~/scratch/wall-clearance/`: `base/`, `wp1/`, `wp2/` (the window's measurement, all arms), `wp3/`, `wp4/`,
`heldout/`, `red/`, `cost/` (callgrind), `scripts/` (the paired-seed analysis; `Gated`'s scripts are reused by path,
not copied). The original probe stays at `~/scratch/wall-grazing/probe/`.

---

## Amendment 1 (implementation agent, from the owner's answer to Q0134, 2026-11-26): crowd tests judge with the showcase

**The answer.** The owner answered [Q0134](../questions/closed/0134-wall-clearance.md) (`main` `48ac2ff6`): option
**(a) now**, the three defects only; and the showcase is only one example, so it must not alone decide whether a wider
clearance (b) or wall-aware steering (c) is needed later. Dedicated tests are to be added in which large groups travel
through narrow sections and chokepoints and round corners, and wall grazing, corner bumping and any later clearance
change are judged against them. Q0134 is closed with a resolution that points here.

**What changes.**
1. **The crowd scenes** (built in WP0): `sim_fixtures::trace::crowd`, run by `crates/sim_core/examples/crowd_trace.rs`.
   - Geometries: chokepoints of 2, 3 and 5 cells (a wall four cells thick), a convex 90° corner, a hairpin round a
     thin wall's end, and a 45° wall of one-cell steps.
   - Groups of 12, 24 and 48 scratch units of radii 0.375, 0.5 and 0.625 (cycled), one owner, each unit moving to its
     own goal: its start translated across the obstacle, so the formation's spacing is kept.
   - Every scene is watched by this plan's route-clearance meter (§6.1), so M-1 to M-5b and the depth check are read
     on them as on the showcase.
   - Known answers: every move ends by the scene's tick bound with none given up (`NoProgress`), and no body lies
     deeper than 2.4 × 10⁻⁵ (H-W1). Arrival within 0.5 of the goal and units that stop short (against friends resting
     at or near their goals) are figures, not answers: the arrival rule ends a crowded move short by design.
   - Seeds: development 1 to 8 (the base of WP0 is measured on them under the arms S, R and L of `Gated` §6.1:
     shipped rules, `Solver` with crowd cap 151, `Legacy`). Held-out sets, registered now and run once per package
     like §6.4's: **HA-crowd** seeds 1,001 to 1,008 (window W) and **HB-crowd** 1,009 to 1,016 (WP3).
2. **Acceptance (§6.3) is read on the crowd scenes together with the showcase seeds, never on the showcase alone.**
   - WP1's target M-5a = 0 holds on the showcases' nine seeds; on the crowd scenes, cutting route updates caused by a
     rounding-depth hand-off must be gone (each remaining one traced and explained).
   - WP2's 19-tick bound on held cutting walk legs applies to the crowd scenes as well.
   - WP3's directional claims (M-2 and the longest episode fall, M-1 does not rise) must hold on the crowd scenes'
     development seeds, per geometry, by §6.4's paired rule (per scene, seeds paired), as well as on the showcase's
     33 paired seeds. A fall on the showcase with a rise on the crowd scenes beyond noise fails the claim.
   - "Not worse than base beyond noise" (§6.3, last column) includes the crowd scenes' M-1 to M-4, every-move-ends
     and the depth check, under S, R and L.
3. **Q0134 (b) and (c).** Any later proposal for a wider margin or wall-aware steering is judged first on the crowd
   scenes (and their held-out set), then on the showcase; the showcase alone never decides it (§7 rule 8's addendum
   to the owner carries both).
4. **Out of scope here.** The crowd scenes are reporting instruments; they add no rule to the simulation and no
   per-scene switch (§7 rule 10). New geometries may be added by a later amendment before a package's base is
   measured, never after a trace.

**Base (WP0).** Evidence [`2026-11-29-wall-clearance`](../evidence/2026-11-29-wall-clearance.md), WP0, "Crowd
scenes": both known answers hold in all 144 scenes under each arm; up to half the units of a 48-unit group stop short
(the hairpin); held cutting walk legs run up to 130 ticks; reported, not judged.

