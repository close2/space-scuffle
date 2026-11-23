# Design review: intent steering and slots above the contact solver

Date: 2026-11-23. Reviewer: an independent design-review agent. It changed no code and ran no harness.

Read:
- the movement survey (`docs/evidence/2026-11-20-movement-survey.md`);
- the plan `docs/plans/2026-11-20-contact-solver.md` (§3, §6, Amendments 1 to 6) on `contact-solver-plan` (`cf5086d7`);
- the evidence file §1 to §9 on `contact-solver-wp4-wip` (`c283994e`);
- `movement/intent.rs`, `slots.rs`, `concave.rs`, `solver.rs` and `locomotion.rs` (the pursuit stall path and
  `is_anchored_in_combat`) on `contact-solver-wp4-wip`;
- the per-scene CSVs under `~/scratch/contact-solver/wp3b/runs/{c3-1,s4-1}`: WP3 candidate and WP4 + Amendment 6,
  development seeds 1 to 1,000.

Tags: **[MEAS]** a figure recomputed here from the existing CSVs; **[CODE]** read in the source; **[SRC]** an
external source; **[INF]** an inference by this review, to be confirmed by the experiment it names.

---

## 0. Verdict in one paragraph

The contact solver is not the problem, and neither are the thresholds. The intent and slot layer brought back the
failure pattern the survey found in the legacy layer: **discrete decisions keyed on binary classes of neighbours**.

- The class **"anchored"** now has three definitions: Legacy, Amendment 5's attack cycle, and Amendment 6's "in range
  for contact".
- It has **four consumers:**
  - the contact shares (C-2);
  - the side rule's threat set (I-3);
  - the slot trigger (G-1);
  - **the pursuit stall exemption** (`friend_holds_ahead` → `note_queued_progress`), which no amendment named.

Each amendment moved the definition for one consumer and moved the failure to another.

On top of that:
- the side rule is a one-partner, full-tangent, on/off controller that is also applied to **standing** bodies. This is
  the static-obstacle and "collision you would never reach" failure that Sunshine-Hill describes for velocity-obstacle
  methods.
- the slot layer is a third state machine (claims, keep / free / re-choose, own-angle claims, contention sort).

The single largest failure in the ring families is not spreading at all. **Under the Solver, 85 % of corridor units and
about a quarter of arc units abandon their target through the stall rule** [MEAS]. The scenes' own known answer says
these units keep their order and stand.

---

## 1. Root cause (question 1)

### 1.1 What the data says

These figures are recomputed from the per-scene CSVs (development, original labelling) [MEAS].

| | Arc, WP3 | Arc, A6 | Corridor, WP3 | Corridor, A6 | Legacy (WP1) |
|---|---:|---:|---:|---:|---|
| units in corpus | 17,018 | 17,018 | 6,888 | 6,888 | same |
| units without room (units − min(k, C)) | 4,435 | 4,435 | 5,085 | 5,085 | same |
| give-ups (`PursuitAbandoned`) | **4,827** | **4,599** | **4,991** | **5,850** | arc 25, corridor 275 |
| scenes with any give-up | 789 | 787 | 795 | 917 | — |
| in range / expected | 6,861 / 12,583 | 7,713 / 12,583 | 1,681 / 1,803 | 1,719 / 1,803 | — |
| fill, scenes with **no** give-up | 0.715 (n 211) | **0.882** (n 213) | 0.866 | 0.931 | — |
| fill, all scenes | 0.609 | 0.669 | 0.908 | 0.914 | arc 0.980 |
| arc fill at weapon range R = 4 | **0.506** | 0.685 | — | — | — |

Three readings follow.

1. **In arc, give-ups ≈ shortfall.** Arc gives up about as many units (4,599) as it falls short of capacity (4,870).
   In the scenes with no give-up, fill is already 0.882 under Amendment 6. Most of T5's failure is units that quit,
   not units that cannot find room.
2. **R = 4 is not a packing problem.** At range 4 the ring holds far more than k, yet WP3's fill there is 0.506, the
   worst of any range. The rear ranks of a block do not fail to *pack*. They stall behind the front ranks and give up
   after `STALL_ABANDON_TICKS` = 10 s.
3. **Corridor: nearly every queued unit quits.** Its known answer is "the rest queue, keep the order and stand"
   (plan §6.2). Instead, give-ups exceed the number of units without room.

### 1.2 Why they give up [CODE]

`check_pursuit_progress` counts a stalled tick unless the unit is `queued`, and `queued = friend_holds_ahead(...)`
tests `SteeringObstacle::anchored` (`locomotion.rs`).

- **WP3's Solver run, legacy definition.** The definition was "standing under an attack order" with
  `actor.velocity == SimVec2::ZERO`. Under the legacy pipeline the waiting rule set the velocity to exactly zero, so
  the chain held: every unit stood behind a "standing" anchored friend.
- **Under the Solver,** velocity is the achieved displacement after the solve. A unit pressing into the one ahead ends
  each tick with a small, non-zero displacement inside the dead band, so it is not anchored. The unit behind it
  therefore is not queued, and it stalls out in 10 s. [INF, consistent with the counts; confirm with a counter of
  `queued` ticks per unit on corridor seeds 1 to 20.]
- **Amendment 5** narrowed "anchored" to the attack cycle for every consumer. That cut the chain further (corridor
  give-ups 4,991 → 5,915).
- **Amendment 6** split the definition per consumer but left the stall consumer on the narrow one.

**This is the same mechanism as legacy R-E and R-F** in the survey §1.3, item 3: a threshold class (exact zero
velocity) evaluated against the output of a soft solver.

### 1.3 Why the side rule trades failures between families [CODE, SRC]

`intent.rs` picks the earliest threat, commits to one partner and side, and turns the heading fully onto the tangent
of that partner's disc. It drops the threat once the bodies are within `R` (the bodies are then "in contact"). Its
threat set includes **standing, non-yielding bodies**: waiting, G-6 standers and firing friends.

- **Static bodies.** Against a standing body the "relative velocity" is the unit's own preferred velocity, so the rule
  is plain static-obstacle avoidance. In a corridor queue or behind a firing line, the body ahead is one the unit will
  never pass: its planned stop is behind it. Sunshine-Hill names this exact failure: agents "are attempting to avoid a
  collision at a point that they would never reach". The fix is to ignore collisions beyond the agent's planned stop
  [SRC: Game AI Pro 3, ch. 19, §19.7.3].
- **On/off control.** The rule has three switches. Tangent while closing, straight once in contact, tangent again once
  the solver has separated the pair. Against a static partner this is a limit cycle, and M1 counts it.
- **One partner.** With two equally urgent threats (two waiting or firing friends side by side), the earliest one
  alternates, and the committed side flips:
  - corridor seed 97: 693 reversals;
  - corridor seed 48: 1,642 reversals.

  Amendment 6's one-tick hysteresis keeps the partner but not the geometry. A partner kept while the other threat is
  nearer steers the unit into the other body, and contact pushes it back. Corridor M1 rose to 31.24.
- **The ID tie-break.** Equal τ goes to the smaller `EntityId`, and G-4's last sort key is also the ID. These are the
  only ID dependences in the layer. Corridor T8 failed under Amendment 6 (p = 0.003).
- **Where the rule works, the threats move.** In crossings (moving against moving) it works: cross M1 1.05 to 1.19,
  0 to 3 give-ups, no friend overlap above 5 %.

**Why fixes trade families.** Making standing friends non-threats (Amendment 5) fixed the corridor flips. But in gaps
the same rule had been doing useful queue-side spreading, and gap give-ups rose from 46 to 96. Restoring them
(Amendment 6) brought the flips back. The two families need different answers to the question "should I steer around
this standing body?". The correct answer depends on whether the unit's own planned stop lies before or beyond that
body. A class of the *other* body cannot answer that.

### 1.4 Why the slot layer did not converge

The slot layer is coupled in three places:
- G-1 asks whether an anchored friend is ahead (so it depends on the anchored definition);
- G-6 standers are "waiting under an order", so they become side-rule threats and get ½ / ½ shares in contact;
- an own-angle claim lives on in memory after its unit is pushed out of band.

Every fix to one coupling moved behaviour through the others. The claims machinery (keep, free, re-choose, own-angle,
contention sort, kept-claim blocking) is the surround walk's state machine moved into memory, which D-γ was meant to
remove.

### 1.5 Is it the evaluation?

Mostly not (see §4 for details). No threshold contradicts its scene's known answer. Every threshold the Solver fails
has been met by some build, or is geometrically feasible:
- arc T5: met by Legacy (0.980 / 0.889);
- corridor T1: 0.03 under the pressed-front build;
- T7 for enemies: feasible, see §4.

Two process issues did contribute to the impression of "trading":
- Amendment 6's stop rule ("any development T-figure worse than WP3") was applied to counts and maxima without §6.4's
  noise rule. Cross give-ups 2 → 3 of 17,976 units, and the cross friend maximum, are noise-level. Corridor M1
  31.24 [27.60, 35.24] against 25.76 [22.99, 28.47] is borderline. The arc enemy overlap (25 scenes above 0.05,
  against none) and corridor T8 are real.
- The scene property that failed most, "units without room keep the order", has no threshold of its own (§4, E1).
  So the dominant regression was seen only through its side effects on M1, M3, T5 and T7.

**Verdict.**
- **Primary cause:** coupling through binary neighbour classes, the anchored class above all, including into the
  stall rule.
- **Secondary cause:** a side rule applied to static bodies with one-partner, on/off control.
- **The slot claims** amplify both.
- **Not the cause:** the contact solver, or the thresholds.

---

## 2. The simplest sound design (question 2)

Design principle: **nothing above the solver reads a class of another body that the solver's output can flip.**
Decisions read either geometry from the tick-start snapshot (positions, radii, velocities) or the unit's own order
and goal. "Anchored" gets exactly one consumer: contact.

### 2.1 (a) Oncoming and crossing bodies

Candidates weighed:

| Candidate | For | Against | Verdict |
|---|---|---|---|
| No side rule; contact + preferred velocity only | Simplest; co-moving friends already need none (SC-2) | An exact head-on pair has normal along the line of motion and shoves; the cross family's 135° and 180° scenes need anticipation (WP3's cross results came *with* the side rule). The survey ranks it below anticipation (ST-3, SC-2 "turn the same way") | Not alone |
| HRVO for moving against moving [SRC: Snape et al. 2011] | Fixes the reciprocal dance; deterministic in principle (Photon Quantum ships HRVO in fixed point, VO-6) | No public fixed-point port; candidate enumeration and an LP-like selection per agent; known failures with cornering, static obstacles and squeezed agents [SRC: Sunshine-Hill §19.6–19.7]; it would fix the family that already works (cross) | Not now; stays rank 2 |
| Social force (distance-based repulsion) [SRC: Helbing & Molnár 1995] | Simple | Distance-based, not anticipatory; force models show "oscillations and overlappings" and need extra collision handling [SRC: Chraibi et al. 2010]. With the solver it would be a second contact model, the survey's original diagnosis | No |
| Weiss PBD long-range constraint inside the solver [SRC: Weiss et al. 2019 §3.4–3.5] | Pairwise, reciprocal, continuous in τ (stiffness `k·exp(−τ̂²/τ₀)`), keeps only the **tangential** part of the predicted contact correction, so agents "divert sideways" and are not "pushed back in a dense flow"; no memory, no partner | Inside K = 21 iterations it multiplies avoidance cost by K and couples contact convergence to avoidance; Weiss runs 6 iterations on a GPU and notes tuning; symmetric head-on may not resolve | Adopt its **term**, evaluated **once per tick in intent**, not inside the iterations |

**Recommendation (a): a summed, continuous, memoryless tangential TTC term, in intent.**

For each neighbour `b` that the current rules count as a candidate (not the chase target, not a co-moving friend
(I-4), not a yielding idle friend (L6)):

1. **Time to collision.** Compute τ from the tick-start snapshot, as `threat_of` does today. Keep the horizon
   `H = h·(r + r_b)/speed` (I-3a) and its derivation.
2. **Planned-stop cut-off [SRC: Sunshine-Hill §19.7.3].** If the collision point lies beyond the unit's planned stop,
   skip `b`. The planned stop is `dist(goal)/speed` ticks: the goal, the gap point of §2.2, or the band for a chase. A
   unit queued behind a body it will stop short of no longer steers round it.
3. **Weight.** `w = clamp(1 − τ/H, 0, 1)`: linear and division-free given `1/H`, a fixed-point stand-in for Weiss's
   exponential and Karamouzas's power law [SRC: Karamouzas, Skinner & Guy 2014]. It rises smoothly to 1 at contact
   and does **not** switch off at contact.
4. **Contribution.** The tangential component of the predicted contact correction, as Weiss eq. (10)–(12): the
   lateral offset needed to clear `R` by τ, divided by τ. Antisymmetric in the pair, so both bodies of a moving pair
   pass on consistent sides with no memory (HRVO's purpose, by geometry). Exact head-on (`cross = 0`): a fixed
   right-hand tangent (I-3b, kept).
5. **Combination.** Sum `w · tangential` over all candidates and add it to `v_pref`. Clamp the deflection at
   `asin(1/h)` (I-3a's 30°) and the magnitude at the unit's speed.

**What this fixes.**
- **Partner flips.** Two equally urgent threats sum. If they are symmetric they cancel: the unit goes straight, and
  contact queues it, which is the right answer in a corridor.
- **The on/off cycle.** The weight is continuous.
- **The ID tie-break.** The sum of Q32.32 values is exact and does not depend on order, so T8 loses its last ID
  dependence in intent.
- **Static queues.** The planned-stop cut-off removes them without any class of the other body.
- **What it keeps.** Against a standing body the unit *will* pass (a queue member beside a gap mouth, the case that
  served gaps under WP3), it still deflects, because the collision lies before its stop.

**Delete from `intent.rs` and memory:**
- `SideMemory` and its two hashed `MovementMemory` fields (`side_partner`, `side_left`): one checksum-schema step;
- the earliest-threat selection and its ID tie-break;
- the commitment start and end, and Amendment 6's one-tick hysteresis;
- the full-tangent `tangent_heading`;
- the rule that bodies within `R` are not threats.

### 2.2 (b) Attackers distributing round a target

Candidates weighed:

- **"Attack-move into range, then stop; pushing does the spreading" (StarCraft II).** This is the base, and it is
  already today's chase: the unit stops at the band's outer edge.
  - The game does not promise a full surround. Blizzard's guide tells players to move units that are "trapped outside
    of the concave" forward themselves [SRC: Blizzard, Game Guide: Unit Positioning; SC-10].
  - Liquipedia describes melee units attacking the front-most unit as getting "in one another's way", with the
    surround done by player micro [SRC: Liquipedia, Micro (StarCraft); read from a search snippet, the page returned
    403].
  - But the second-hand notes on Blizzard's 2011 navigation talk say units "aim for the gap closest to the centre of
    their path" (SC-2).
  - With intent aimed at the target's centre, the tangential part of a push against a firing line is about zero, so
    pushing alone parks units directly behind the front. Pushing alone will not meet T5's 0.90 for short ranges
    [INF; WP3 without slots, in its no-give-up scenes: fill 0.715].
- **Slots with claims, contention sort and keep / free / re-choose (today's G).** Typical of action games, where a few
  enemies face one player (battle-circle and token designs [SRC: Tuts+ "Battle Circle AI"; the Unreal slot
  write-ups]). There is no evidence of claims in an RTS. Here they are the coupling source of §1.4.
- **The nearest free in-range point by deterministic assignment.** This is the right core, without claims.

**Recommendation (b): range-then-stop, plus a memoryless "aim for the nearest free gap" goal.**

1. **Occupancy is geometry.** From the tick-start snapshot, the ring is the band's outer edge (as Amendment 5, item 2
   already placed it). The bodies that block it are those whose discs reach the ring (G-2's `blocking_interval`),
   **whatever their class or order**. The free set excludes:
   - ring points outside the unit's terrain domain;
   - ring points beyond the leash;
   - ring points with no ground connection (G-3's checks, kept).
2. **Goal.**
   - If the unit's own bearing point on the ring is free, its goal is today's chase: walk in and stop in range.
   - Otherwise its goal is the free angle nearest its bearing, ties clockwise (G-3, kept).
   - Hysteresis: keep last tick's angle while it is still free. This is one remembered point, no claim. The existing
     `slot` field can hold it as a goal cache, with no `target` semantics beyond "this chase".
3. **No free angle:** the unit stands. Intent is zero, the order is kept and no stall accrues (G-6, Q0089).
   - The test is "the ring has no free reachable angle". It is geometric and reads no neighbour's class, so it cannot
     cascade the way the pressed-front rule and G-1 did.
   - A stander injects no overlap, which is what K-12 needed.
4. **No contention resolution.** Two units may aim at the same gap. Each choice is a function of the snapshot, so the
   result does not depend on processing order. The first to reach the band occupies the gap, and the other re-aims
   next tick. Contact absorbs the brief overlap (D-α).
5. **Stall rule.**
   - For a unit walking to a gap, the potential is the distance to the gap point.
   - For a stander (no free angle), no stall.
   - `friend_holds_ahead` is no longer read by the pursuit stall rule under the Solver.
   - A pursuit is still abandoned when no route reaches range (`reroute_pursuit`) or the leash is exceeded, as today.
6. **Anchored: one definition, one consumer.** C-2 keeps Amendment 6, item 3's contact definition: hold, immobile,
   unpushable, or **in range of a live target under an attack order**. That matches SC-8 ("blocked by units holding,
   attacking"). The side rule, G-1 and the stall rule no longer read it. Amendment 5's attack-cycle definition is
   deleted.

**Delete from `slots.rs` / `concave.rs`:**
- `resolve_claims` and the `SlotRequest` contention sort;
- `KeptClaim` and blocking by kept claims;
- `claim_fate` (Keep / Free / Rechoose) with its "taken", "away", "left the band" tests;
- own-angle claims (Amendment 5, item 3; Amendment 6, item 2);
- the G-1 trigger `queued_toward`;
- the per-tick `slot_wanting` / `slot_requests` / `slot_kept` buffers.

**Keep:** `blocking_interval`, `is_free`, `choose_slot` (nearest free angle, clockwise tie) with its terrain and arc
checks, `slot_point`, and the G-6 stand list.

### 2.3 Also in the solver (outside the intent / slot question, but behind T7)

C-6 leaves a body in place when terrain refuses its correction [CODE: `solver.rs`: "`None` when terrain refuses the
correction: the body stays"]. Where an immobile enemy plug meets a wall (the corridor's cusp), the pillar's normal
correction has a component into the wall and is refused whole. Two changes follow:

- **Slide instead of refusing.** Project a refused correction onto the wall's tangent, as `slide_target` already does
  for steps.
- **Make C-1b's hard pass prioritised.** Run it after the friend pairs, with fixed and enemy constraints applied
  last.

Geometrically, every corridor scene has room (`2r ≤ w − 1/8`), so enemy overlap 0 is feasible [INF; confirm on the 55
corridor and 50 arc scenes of A6 with enemy overlap above 0.005].

---

## 3. Determinism, fixed point and cost (question 3)

**Determinism.**
- Every recommended part is a function of the tick-start snapshot and the unit's own memory:
  - the summed TTC term is exact Q32.32 sums (order-independent, like C-1a);
  - the gap goal reads the snapshot and one own field.
- No `EntityId` enters any decision except C-1a's coincident-centre normal. Two ID tie-breaks disappear (the threat
  tie, and G-4's last key), which helps T8.
- No float, no exponential (a linear ramp), one `sqrt` per candidate for τ, as today. Nothing changes for wasm.

**Checksum.**
- `side_partner` and `side_left` are deleted.
- `slot` keeps its shape (a point and its target) or shrinks to a point. Schema +1 at WP7 as planned, or now.

**Cost.** The recommendation **does not by itself bring +51 to +55 % down to +10 %**. The split (evidence §6.1,
64 × 64, after step 1) is:

| Part | Share of the +5.4 G excess | Effect of this recommendation |
|---|---|---|
| Contact solver | +3.4 G | **Partial.** Standers inject no overlap, so pressed fronts stop holding the solve at K. But the pressed-front build barely moved battle iterations (15.75 → 15.42), and evidence §6.2 attributes the K-bound ticks to dense *moving* crowds. The tolerance-based early stop also failed (15.02). The remedy is in the solver: over-relaxed averaging (Weiss: averaging coefficient 1.2; Macklin et al.'s unified-particles SOR) or Gauss–Seidel with a fixed colouring (Amendment 3, item 3), re-deriving K under Amendment 2's protocol |
| Target acquisition | +1.6 G | **None.** A behaviour effect of denser packing; a separate optimisation (for example a throttled acquire scan), outside this layer |
| Movement phase (intent, wider reach) | +0.6 G | **Small saving.** The summed term costs one pass, as today. The planned-stop cut-off lets most queued units skip the far candidates, and standers skip intent entirely |
| Slots | not yet measured | **Saving.** No claim fates, no contention sort, no kept-claim intervals; one free-interval scan per chasing unit whose bearing point is blocked |

Expect the 64 × 64 figure to fall by a few points from intent and slots, and by more only with solver work.
Whether +10 % is the right bar is Q0093's open question to the owner. This review does not change it.

---

## 4. Evaluation (question 4)

None of the pre-registered thresholds is unattainable in principle:

| Threshold | Attainable? | Evidence |
|---|---|---|
| T1 corridor and arc | Yes | Corridor 0.03 and arc 1.86 under the pressed-front build (§6.2); the corridor's answer (the rest stand) implies near-zero reversals |
| T4 gap and cross, 0 give-ups | Yes in principle | The bounded arrival chain took gap 336 → 46; the residual is mixed-priority arrival (B-158), an arrival-rule question, not a local-avoidance limit |
| T5 arc fill | Yes | Legacy 0.980 / 0.889; A6's no-give-up scenes already 0.882 |
| T7 enemy ≤ 0.5 % (a maximum) | Yes, with a prioritised and sliding hard pass (§2.3) | Feasible geometry in every scene; legacy and the Solver both exceed it today because of solver details, not physics |
| T8 | Yes | Fails only where ID tie-breaks enter (§1.3) |

**`is_anchored` and corridor give-ups.** `is_anchored` semantics do force give-ups in the corridor family. But that
is the design's stall rule contradicting the scene's known answer ("keep the order and stand"), not a threshold
contradicting it. **No change to a pre-registered threshold is warranted.**

Issues found:

- **E1. The ring families' "keep the order" has no threshold.**
  - T4's give-up clause covers the passable gap and cross families only, so 5,850 corridor give-ups (85 % of units)
    count against no threshold directly.
  - **Proposal:** add, by amendment and **before** the acceptance run, the property "0 `PursuitAbandoned` in arc and
    corridor while the target lives and a route reaches range", or report give-ups beside T5 in the verdict table.
  - This adds a check derived from the scene's own known answer; it loosens nothing.
- **E2. The noise rule was not applied to amendment stops.** Amendments 5 and 6 stopped on "any development T-figure
  worse than WP3", including:
  - counts of 2 → 3;
  - maxima, which are extreme-value statistics that move with any change;
  - an M1 with overlapping intervals.

  §6.4's rule (outside the placebo range and the bootstrap interval) should govern development comparisons too. The
  real regressions (arc enemy overlap, corridor give-ups, corridor T8) would still have stopped A6.
- **E3. T8's stated family-wise rate is wrong.** T8 says p ≥ 0.01 per test "still gives a family-wise rate below 5 %".
  With five scene families and two sign tests each (M1 and completion tick), that is ten tests. Bonferroni bounds the
  false-failure rate at 10 %, not 5 %.
  - The threshold as written is the stricter one; this review does **not** propose changing it.
  - But a single acceptance p in [0.005, 0.01) has a non-negligible chance of being a false alarm under no bias. The
    plan should say so in advance, so that such a result is read against its traces rather than re-run.
- **E4. M1 in ring families probably counts abandoned, idle units being shoved** [INF]. Once a unit abandons, it
  yields (L6) and the pushers move it. Splitting M1 by "under order / idle" would show how much of arc's 25 to 30
  reversals per 1,000 moving unit-ticks is give-up churn. This is a diagnostic column, not a threshold change.

---

## 5. Recommended next step

Build from WP3 on `contact-solver-plan`, not on top of `contact-solver-wp4-wip`. Run two arms on development seeds 1
to 1,000, so that the effects can be told apart:

1. **Arm A: stall and occupancy only.**
   - WP3, plus the stall change of §2.2, item 5;
   - plus the single anchored definition (§2.2, item 6);
   - plus the geometric "no free angle → stand" (§2.2, item 3);
   - with no gap aim.

   Expected [INF]:
   - arc and corridor give-ups near 0;
   - corridor M1 toward the pressed-front build's 0.03;
   - arc fill toward the no-give-up figure of about 0.72 to 0.88.
2. **Arm B: A plus the gap goal** (§2.2, items 1, 2, 4). Expected [INF]: arc fill toward Legacy's 0.98; T5 is to be
   shown, not assumed.
3. **Then the summed TTC term** (§2.1) in place of the one-partner rule, judged on all families. Expected [INF]:
   - gap and cross unchanged within noise (the term behaves like WP3's rule against single threats);
   - corridor flips gone;
   - corridor T8 restored.
4. **§2.3's sliding, prioritised hard pass** for T7 enemies.
5. **Cost split** after these, per Amendment 4, item 5. Solver acceleration is decided only on that split and on
   Q0093.

This review counts as the "design review against the survey's alternatives" that Amendment 6 named. It recommends
**against** HRVO (rank 2) at this point: the family HRVO would serve (crossings) already works, and HRVO's documented
failures are the static and queued cases that fail here.

---

## Sources

- Sunshine-Hill, B. "RVO and ORCA: How They Really Work." *Game AI Pro 3*, ch. 19 (§19.5 sidedness, §19.6 cornering,
  §19.7.3 time horizons, §19.9 test rig).
  <http://www.gameaipro.com/GameAIPro3/GameAIPro3_Chapter19_RVO_and_ORCA_How_They_Really_Work.pdf>
- Weiss, T., Litteneker, A., Jiang, C., Terzopoulos, D. "Position-based real-time simulation of large crowds."
  *Computers & Graphics* 2019 (MIG 2017): §3.4 long-range constraint, §3.5 tangential-only avoidance model, §4 Jacobi
  with averaging coefficient 1.2, 6 iterations, §5 limitations (tuning).
  <https://web.cs.ucla.edu/~dt/papers/cag19/cag19.pdf>
- Snape, J., van den Berg, J., Guy, S. J., Manocha, D. "The Hybrid Reciprocal Velocity Obstacle." *IEEE Transactions
  on Robotics* 27(4), 2011, 696–706. <https://gamma.cs.unc.edu/HRVO/HRVO-T-RO.pdf>; <https://gamma.cs.unc.edu/HRVO/>
- van den Berg, J., Guy, S. J., Lin, M., Manocha, D. "Reciprocal n-body collision avoidance" (ORCA), 2011.
  <https://gamma.cs.unc.edu/ORCA/>
- Karamouzas, I., Skinner, B., Guy, S. J. "Universal power law governing pedestrian interactions." *Physical Review
  Letters* 113, 238701 (2014). <https://link.aps.org/doi/10.1103/PhysRevLett.113.238701>
- Helbing, D., Molnár, P. "Social force model for pedestrian dynamics." *Physical Review E* 51, 4282 (1995).
  <https://doi.org/10.1103/PhysRevE.51.4282>; <https://arxiv.org/abs/cond-mat/9805244>
- Chraibi, M., Seyfried, A., Schadschneider, A. "Generalized centrifugal force model for pedestrian dynamics" (on
  force-model oscillations and overlaps, and the extra collision handling they need). <https://arxiv.org/abs/0910.2343>
- Blizzard Entertainment. "StarCraft II Game Guide: Unit Positioning" (concave; move units trapped outside it).
  <https://news.blizzard.com/en-us/article/6640646/game-guide-unit-positioning>
- Liquipedia. "Micro (StarCraft)": surround by moving through, not attacking the front unit. Read from a search
  snippet; the page returned 403. <https://liquipedia.net/starcraft2/Micro_(StarCraft)>
- howtorts. "Pathing literature review" (notes on Blizzard's GDC 2011 talk: gap closest to the path's centre; turn the
  same way; co-moving ignored). <https://howtorts.github.io/2014/01/06/pathing-literature-review.html>
- Slot and battle-circle designs in action games: <https://code.tutsplus.com/battle-circle-ai-let-your-player-feel-like-theyre-fighting-lots-of-enemies--gamedev-13535t>;
  Unreal slot write-up <https://pavankrao-qlp.github.io/portfolio/blog/unreal-rpg-devlog-part-1---state-tree-based-ai--unreal>
- Survey sources SC-2, SC-8, SC-10, RT-1 (0 A.D. pushing pressure), VO-6 (Photon Quantum HRVO), as cited in
  `docs/evidence/2026-11-20-movement-survey.md`.
