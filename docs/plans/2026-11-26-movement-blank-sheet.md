# Design Round: Local Movement from a Blank Sheet (Q0118's Strategic Guidance)

This is a **design-round document**, not yet an implementation plan. It answers the owner's strategic guidance in
[Q0118](../questions/open/0118-crowd-overlap-solver-cost.md): before more patches go onto the Jacobi contact solver,
re-examine local movement from a blank sheet. It states the requirements, turns them into a scenario suite that
can judge any candidate, assesses the candidate architectures with sources, reports scratch prototypes of the
strongest candidates against today's solver, and recommends an architecture with a migration path. The owner's
decisions are collected as drafts at the end (§9). The implementation plan, with work packages and file ownership,
is written only after the owner has answered them.

**Status.** Written on 2026-11-26 (session date 2026-11-25) on branch `movement-blank-sheet`, cut from `main` at
`b4a99fae` (checksum schema 49, contact solver the default pipeline, Q0118 answered (a)).
- **Line numbers drift,** so search for the quoted symbol. Claims not confirmed while writing are marked **verify**.
- **Later decisions** are appended as `## Amendment N (role, YYYY-MM-DD): …`.
- **Numbers are provisional, next free at merge.** Question drafts start at **Q0120** (Q0119 is the follow-order
  plan's). This round registers **no new bug row**; it re-files the open ones (§7.4).
- **No production code.** The prototypes of §5 are a scratch float program under
  `~/scratch/movement-blank-sheet/` (`proto/`, `proto_frozen_main.rs`, `run_all.sh`, `results.txt`), never committed.

**Why a full plan (charter §6).** The work this document leads to replaces a subsystem, moves every battle outcome
and golden replay, changes hashed state and spans several packages. This document is its first half: the survey and
the design. The second half (the implementation plan) follows the owner's answers.

**Inputs read.**
- Q0118's whole Answer: option (a), the owner's strategic guidance and the seven-item research summary.
- Plan [`2026-11-20-contact-solver`](2026-11-20-contact-solver.md) (§2 the pipeline map, §3 the design, §6 the
  harness and thresholds, Amendments 11 to 21) and its evidence
  [`2026-11-20-contact-solver.md`](../evidence/2026-11-20-contact-solver.md).
- The survey [`2026-11-20-movement-survey.md`](../evidence/2026-11-20-movement-survey.md) (sources SC-1 to SC-11,
  FF-1 to FF-9, VO-1 to VO-9, ST-1 to ST-5, RT-1 to RT-7). This document cites those IDs and adds new sources as
  **N-1** onward (§4.1).
- The stopped or parked attempts and their evidence:
  - `solver-followup` (`6a96cd15`) and `solver-followup-wp2-wip` (`dc3b62ab`): E-1 (enemies take the share they
    closed) and G-6′ (retarget from a full ring), plan `2026-11-24-solver-retarget-and-shoving`;
  - `enemy-contact-plan` (`7f686d23`): E-2 (enemies block), F-1 (friend transfer), K-1 (merged as B-208), plan
    `2026-11-25-enemy-contact-and-ring-retarget` with Amendment 2;
  - `collision-classes-wp3-wip` (`42761cf2`, local only): the colossus walk-under (B-209);
  - `friend-crowd-plan` (`ba3e639b`): plan `2026-11-26-friend-crowd-overlap`, C-3′, being built as the interim fix.
- Q0052 with its addenda (D-α, D-β, D-γ, D-δ, S-snapshot-cap); Q0093 (c), Q0095 (b), Q0097, Q0102, Q0103 and its
  standing guidance (AGENTS.md §8); Q0113 (b, parked); Q0117 (open).
- The owner's standing pathfinding guidance (2026-11-20): no fitting to the test set; simpler deviations are allowed
  if recorded; survey first.
- Code at `b4a99fae`: `crates/sim_core/src/movement/` (`solver.rs`, `intent.rs`, `ring.rs`, `concave.rs`,
  `locomotion.rs`, `collision.rs`), `crates/sim_map/` (navmesh, funnel, lanes), the harness
  (`crates/sim_fixtures/src/harness/`), the contact meter B1 to B9 (`sim_fixtures::trace::contact_checks`), the
  matchup matrix (`crates/sim_core/tests/fixtures/matchup_matrix.ron`) and the throughput benchmark.

**Questions and standing rules.**

| Question | State | Where |
|---|---|---|
| Q0118 (a) and its strategic guidance | Answered; C-3′ is being built as the interim fix; this round answers the guidance | §0, §6 |
| Q0051 / AGENTS.md §8: unit mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing | §2, §6.4 |
| Q0052: D-α, D-β, D-γ, D-δ sanctioned | Standing; §7.3 proposes what becomes of each | §7.3, §9 |
| Q0093 (c), Q0102 (a): every throughput scenario ≥ 50 ticks/s; overlap tolerances are not traded for instruction ratio | Standing | §2 R-16, §7.6 |
| Q0103's guidance: natural movement outweighs a close matchup's winner; a broken behaviour is a bug | Standing | §7.7 |
| Q0113 (b): enemies-block parked | Closed; the owner now prefers "enemies never push" outright | §2 R-6 |
| Q0117: zerglings under the colossus | Open; collision classes are orthogonal to this round | §7.4 |
| Pathfinding guidance (2026-11-20): no test-set fitting, simpler deviations allowed, survey first | Followed: survey (§4) before prototypes (§5); acceptance on properties (§3, §7.5) | throughout |
| Q0002: scoping is the agent's call if written down | Standing | §8 |
| `docs/questions/open/` at `b4a99fae` | Q0117, Q0118 | — |

---

## 0. Summary

1. **What went wrong, in one sentence (§1).** Every failure of the last five movement packages traces to one of six
   root causes, and the first is the deepest: **intent proposes a full step into bodies that will not give way, and
   the contact solver is asked to undo it.** Injection plus Jacobi diffusion is B-219; injection plus unpushable
   fronts is an infeasible constraint set that no iteration count resolves (the prototype's `Jconv-O` arm: 250
   iterations, still above D-α); injection plus enemies that may not be pushed is why E-1, E-2 and F-1 compressed
   the crowd behind the front.
2. **Requirements (§2)** R-1 to R-18, each with its StarCraft II source or the owner's preference, and **the scenario
   suite (§3)** that judges a candidate: the six existing harness families, the matchup battles 26 to 29, 33, 38a/b
   with B1 to B9, the throughput benchmark, and nine new scenarios (Hold line, enemy press, deep queue, wedge,
   hallway, stutter step, moving surround, permutation, lanes).
3. **Candidates (§4).** Eight architectures assessed for fit, determinism and fixed point, cost at 400 units, the
   failure modes of our evidence each fixes or keeps, and migration risk. ORCA is rejected (§4.9).
4. **Prototypes (§5).** A float scratch program plays battle 26/27/28 stand-ins, a corridor, a surround, a gap, a
   crossing and two 400-unit clashes with eight arms. The figures are reported as measured, including the failures.
5. **Recommendation (§6): restructure, not replace.** Keep the global layer, the slot goal and the order-independent
   Jacobi solver with C-3′; move the decisive work **up** into intent and goal choice:
   - **G′, goal choice:** the slot goal with kept claims, an orbit approach to a far slot, and retarget from a full
     ring (D-ε) — the "unique arrival points" of SC2;
   - **I′, gated intent:** a unit never proposes a step into a body that will not give way (an enemy, a holding or
     attacking unit, a friend queued under an order, an oncoming mover); its direction is chosen among the bins
     that make progress, with a kept side (context steering restricted to a Lyapunov function);
   - **C′, contact:** the owner's rules (holding and attacking units are never pushed; enemies never push each other)
     with today's Jacobi plus C-3′ as the residual solver; shock propagation and Gauss–Seidel are not adopted as the
     default (order dependence, wedge throws), only kept as a measured optimisation candidate.
6. **Migration (§7):** a third value of `contact.pipeline` (`Gated`) beside `Solver`, as `Solver` was built beside
   `Legacy`; five packages; D-δ and the enemy half-share retired under `Gated`, D-β re-asked; B-192, B-193, B-219,
   B-220 targeted; acceptance and stop rules pre-registered in §7.5; cost budget §7.6; close battles §7.7.
7. **Owner questions (§9):** Q0120 (the architecture), Q0121 (D-δ), Q0122 (D-β: moving friends push or block),
   Q0123 (D-ε, retarget from a full ring), Q0124 (the cost budget), drafts with defaults.

---

## 1. What the evidence says: six root causes

The parked attempts were judged one at a time. Read together, their measured failures come from six causes.

| # | Root cause | Where it was measured | Which failures it explains |
|---|---|---|---|
| **RC-1** | **Injection.** Intent steps a full step into the crowd ahead: I-3 ignores co-moving friends and bodies already in contact (by design), and a share-0 partner (engaged, holding, enemy-held) does not stop the walker's step, only the solver's correction | Friend-crowd plan §2.5 point 1 (every walker steps 22 % of the radius sum into the crowd); enemy-contact Amendment 2 finding 1 (the walker keeps walking into the friend E-2 holds, to about 90 %); §5 of this document (in the stand-in of battle 28, 96 % of the friend pairs above 5 % under `J21` are mover–mover pairs) | B-219, B-193 (the press passes through the front), E-2's T5/B6 failure, F-1's chain limit, R5 / B-220 |
| **RC-2** | **Jacobi diffusion.** An averaged Jacobi correction travels one contact per iteration and halves at each body: iterations grow with the square of crowd depth | Friend-crowd plan §2.4 (dose–response 21 → 250 iterations), §2.5 point 2 | B-219, the cost of C-3′ (+50 % to +60 % on the 400-unit clashes) |
| **RC-3** | **Infeasibility.** With fronts that may not move (holding, attacking under the owner's rule, enemies), a pressing crowd defines a constraint set with no solution; no solver converges on it, and Gauss–Seidel throws wedged bodies back and forth | §5: `Jconv-O` (Jacobi to convergence under the owner's rules) still has 243 / 1,061 friend pair-ticks above 5 % per seed in the battle 26 / 28 stand-ins at twice the pair evaluations; `GS` throws a wedged body 1.2 to 1.5 world units per tick until a displacement cap is added | Why "enemies never push" (E-1, E-2) and "attackers never pushed" (D-δ off) each raised friend overlap |
| **RC-4** | **Binary per-pair decisions that flip.** A share, a side partner or a "fixed" flag chosen per pair per tick changes discontinuously as contacts come and go | Contact-solver Amendment 13 (step 1b: side partner at time-to-contact zero), enemy-contact §2.3 (E-1's shares flip between applications), Amendment 16 (S: removes oscillation but also the slide) | T1 failures, M1-hi, D-CHURN |
| **RC-5** | **Goal-level local minima.** The chase aims straight at its slot through the engaged front rank (R1); a unit whose target's ring is full stands although another enemy has room (B-192) | Contact-solver Amendment 11 R1, Amendment 12 (P2's limit cycle); B-192 (battle 28: 678 vs 105 zergling-seconds blocked) | B-190 (give-ups with room), B-192, arc fill, D-R10 |
| **RC-6** | **Order and identity.** Results that depend on processing order or entity IDs | Contact-solver T8/T9 (fixed by C-1a's exact sums), the live pursuit read (§5.4 there) | Relabelling bias; it constrains every candidate (R-13) |

**What follows.** RC-1 is upstream of RC-2 and RC-3: a solver only has to be fast and feasible because intent keeps
creating overlap it cannot place. An architecture that removes RC-1 at the intent level makes the solver's job small
and feasible under the owner's rules; one that does not must either diffuse faster (RC-2: Gauss–Seidel, shock
propagation, more iterations) or relax a constraint (D-δ, enemy half-shares), which is exactly the trade-off Q0095,
Q0113 and Q0118 put to the owner three times.

---

## 2. Requirements

Each requirement names its source: StarCraft II behaviour (survey IDs), an owner ruling or preference, or the
charter. "Shall" requirements are binding on the recommended architecture; the scenario of §3 that judges each is
named in its last column.

| ID | Requirement | Source | Judged by |
|---|---|---|---|
| **R-1 Surround** | Melee attackers fill the ring round a target up to its capacity; units outside the concave stand, keep their order and are not stalled | SC-10; Q0074; D-γ; T5 | arc (M6), surround-moving (new), battles 26 to 29 (B7′) |
| **R-2 Retarget at a full ring** | An automatically acquired chase whose target's ring is full takes another enemy within its scan range whose ring has room, instead of standing | B-192 (the game's units do not stand behind a full ring while another enemy has room: Legacy's behaviour, battle 28's claim) | battle 28, D-R10, D-CHURN |
| **R-3 Chokes** | A group passes a gap at a rate near the lane-capacity model, without deadlock, and nobody passes a gap narrower than its diameter | T4, T6 | gap (M5, M7), corner |
| **R-4 Corridors** | Attackers in a corridor plugged by a target fill its face and queue behind without jitter | T5 (corridor fill), C-e | corridor (fill, M3) |
| **R-5 Hold Position and attacking units are not pushable** | A unit on Hold Position, and a unit attacking (its target within its band), is never moved by a friend or an enemy | **Owner, Q0118** (reverting D-δ's basis); SC-8 ("blocked by units holding, attacking") | hold-line (new), B2/D-SHOVE, corridor |
| **R-6 Enemies never push each other** | No body is moved by an enemy's step or correction; enemy overlap stays within the rounding bound | **Owner, Q0118**; 5.0.14 `AlliancePushable` (enemy-contact plan §2.2, patch 1.2.0 / 1.3.0) | enemy-press (new), B2, B3 (≤ 0.5 %) |
| **R-7 Push priority** | A moving unit pushes an idle friend of no higher priority; a higher-priority mover pushes a lower one when both move (D-β) or both block (SC-8), as the owner decides (Q0122) | SC-5, SC-8, SC-9; D-β | gap (mixed priorities), cross |
| **R-8 Bounded overlap** | Friends overlap at most D-α (5 % of the radius sum while moving, 1 % at rest) on 99.9 % of near pair-ticks, never above 15 %; enemies and terrain never beyond rounding | D-α; T7 | every scenario (B6, M8) |
| **R-9 No stall with room** | A unit with room to make progress is not standing, and does not give up | T4, D-S, D-R10; B-190 | gap, arc, deep-queue (new), battles (D-R10) |
| **R-10 No hallway dance** | Two units or columns meeting head-on pass on a kept side, without reversals | SC-2 ("turn the same way"), ST-2, VO-3 | cross, hallway (new), M1 |
| **R-11 Stutter step** | A move order given to an attacking ranged unit moves it on the next tick, and an attack order resumes fire; contact adds no lag beyond one tick | SC2 micro (SC-6); the attack-order plans | stutter (new) |
| **R-12 Group movement and lanes** | A group move keeps its lanes (plan `2026-11-10-group-routing`), arrives as a clump, and its members do not fight over one point | Group routing; A-2 | lanes (new), gap (arrival) |
| **R-13 Determinism and order independence** | Bit-identical on the four platforms; the tick's result is a function of the set of bodies, not of entity IDs or list order (beyond exact-tie breaks named in the design) | Charter §3.1; C-1a; T8, T9 | golden replay on four builds, permutation (new), relabelled corpus, mirror |
| **R-14 Fixed point** | No float in authoritative state; every operation `SimScalar` with checked arithmetic and deterministic `sqrt`/`atan2` | Charter §3.1 | review, C7 |
| **R-15 No oscillation** | M1 ≤ 1.0 per 1,000 moving unit-ticks and M1-hi ≤ 0.25 in every family | T1 | every scenario |
| **R-16 Cost floor** | Every throughput scenario ≥ 50 ticks/s; instructions reported against today's `Solver` with C-3′ | Q0093 (c), Q0102 (a), Q0118 (a) | throughput benchmark (instructions retired) |
| **R-17 Natural movement over close winners** | A sound change that moves a close battle is restated with evidence; a battle whose claim is a StarCraft II fact (28: immortals overrun) must hold | Q0103 guidance (AGENTS.md §8) | matrix, close-battle list |
| **R-18 Simplicity** | Fewer rules and constants than today's local layer, each with a source; no per-family or per-seed switch | Owner pathfinding guidance; charter §1.1 | review (rule count, §6.5) |

---

## 3. The scenario suite

Any candidate is judged on the same suite, on the seed ranges and with the noise rule of the contact-solver plan
§6.1 and §6.4 (development 1 to 1,000; a fresh acceptance range registered before the code; held-out
9,000,001 to 9,001,000 confirms only). Thresholds are §6.5's T1 to T9 unless a row says otherwise.

### 3.1 Reused

| Scenario | What it judges | Metric and bound |
|---|---|---|
| Harness **gap** (with mixed-priority quarter) | R-3, R-7, R-8, R-9, R-15 | T1, T4, T6, T7; B-220's terrain-pinned shares reported |
| Harness **arc** | R-1, R-8, R-9 | T5 (fill ≥ 0.90, P5 ≥ 0.7476), T7, E1 (give-ups with room) |
| Harness **cross** | R-10, R-7, R-15 | T1, T4, T7 |
| Harness **corridor** | R-4, R-5 | fill (C-e), M3 ≤ 0.5, two-abreast figure reported (R8 of the solver plan) |
| Harness **corner** | R-3, R-9 | T4 read strictly (Amendment 11) |
| Harness **mirror** | R-13 | T9 (blue's share CI contains 0.5), D-SHOVE, D-R10 per scene |
| Matrix battles **26, 27, 28, 29** (100 zerglings against colossi, ultralisks, immortals, stalkers) | R-1, R-2, R-5, R-6, R-8, R-17 | B1 to B9 (`ContactMeter`), counted from the first stepped tick (friend-crowd WP0); battle 28 must hold its claim |
| Matrix battles **33, 38a, 38b** and the pre-registered close-battle list | R-17 | outcomes restated with traces, never a stop by themselves |
| **Throughput** benchmark (64 × 64, 128 × 128, canyon, casters) | R-16 | instructions retired per scenario, ticks/s ≥ 50 |
| **Golden replay** on four builds | R-13 | bit-identical checksums |

### 3.2 New scenarios (generated like the harness families: SplitMix64, 1/64 lattice, scratch archetypes)

| Scenario | Scene | Known answer |
|---|---|---|
| **hold-line** | A line of 4 to 10 friends on Hold Position across an open field; 6 to 30 friends of mixed priority given a `Move` through it | No holding unit moves (displacement 0); the movers route round the line's ends; no mover gives up |
| **enemy-press** | A line of 4 to 8 enemies standing (idle, attacking, or holding: three sub-families); 20 to 60 attackers attack-move into them from one side | No enemy moves except by its own step; enemy overlap ≤ 0.5 %; friend overlap within D-α (T7) |
| **deep-queue** | A corridor of width 3 to 6 cells plugged by an immobile enemy; 20 to 80 melee attackers, 6 to 12 ranks deep | The front fills (C-e); friend overlap within D-α at every depth; no reversals (T1); M3 ≤ 0.5 |
| **wedge** | A walker whose straight route runs between two unpushable bodies (holding friends, or enemies) with a gap 0.6 to 0.95 of its diameter | It never passes, never overlaps beyond D-α, never jitters (M1 = 0), and routes round |
| **hallway** | Two single files of 1 to 6 units meeting head-on in a corridor two diameters wide | Both files pass, each on its kept side; M1 ≤ 1.0, no give-up |
| **stutter** | 1 to 6 ranged attackers in contact with friends behind them, given `Move` back for 3 to 8 ticks then `Attack`, repeated | The first step lands on the tick after the order; displacement in the order's direction ≥ 0.9 × speed × ticks; the attack resumes when in range |
| **surround-moving** | 10 to 30 melee attackers chase a target moving at 0.5 to 0.9 of their speed in a circle | Fill reaches `min(k, C)` within 10 s of first contact; M1 ≤ 1.0 |
| **permutation** | Every scenario above, with the body list rotated by a seeded offset inside the tick (not only the spawn order relabelled) | Bit-identical positions every tick (exact-tie breaks excepted, and counted) |
| **lanes** | A group `Move` of 12 to 48 through a field with two parallel 3-cell gaps | Both lanes used (group routing); arrival as a clump; no member's order completed by a stranger's arrival (A-1) |

**The rule for adding to the suite.** A scenario is added only with its known answer stated before any candidate is
run on it, as the harness's families were (`7f4d26b7`). The prototypes of §5 ran only stand-ins of reused scenarios
and of corridor and surround, not the new ones; the new ones are first run by the implementation plan's WP0.

---

## 4. Candidate architectures

### 4.1 Sources added in this round

Earlier IDs are the survey's (`docs/evidence/2026-11-20-movement-survey.md` §2). New ones:

| ID | Source | Used for |
|---|---|---|
| N-1 | Weiss, Litteneker, Jiang, Terzopoulos, "Position-based real-time simulation of large crowds", *Computers & Graphics* 78 (2019) 12–22 (MIG 2017 version: "Position-Based Multi-Agent Dynamics for Real-Time Crowd Simulation"); <https://web.cs.ucla.edu/~dt/papers/cag19/cag19.pdf> | §4.3 (Gauss–Seidel/Jacobi projection with short-range contact and long-range anticipatory constraints, the same survey item ST-4) |
| N-2 | Macklin, Müller, Chentanez, Kim, "Unified particle physics for real-time applications", *ACM TOG* 33(4) 2014 (constraint averaging, over-relaxation); Macklin, Müller, Chentanez, "XPBD", MIG 2016 | §4.3 |
| N-3 | Guendelman, Bridson, Fedkiw, "Nonconvex rigid bodies with stacking", SIGGRAPH 2003 (shock propagation) | §4.3, already the solver plan's P3 and F-1's source |
| N-4 | Fratarcangeli, Tibaldo, Pellacini, "Vivace: a practical Gauss–Seidel method for stable soft body dynamics", *ACM TOG* 35(6) 2016; Fratarcangeli, Pellacini, "Scalable partitioning for parallel position based dynamics", *CGF* 2015 (graph-coloured Gauss–Seidel) | §4.3 |
| N-5 | Fray, "Context steering: behavior-driven steering at the macro scale", *Game AI Pro 2* ch. 18 (2015) (= ST-5) | §4.4 |
| N-6 | Karamouzas, Skinner, Guy, "Universal power law governing pedestrian interactions", *PRL* 113 (2014) (= VO-9); Karamouzas, Sohre, Narain, Guy, "Implicit crowds: optimization integrator for robust crowd simulation", *ACM TOG* 36(4) 2017, <https://people.computing.clemson.edu/~ioannis/implicit-crowds> | §4.5 |
| N-7 | Narain, Golas, Curtis, Lin, "Aggregate dynamics for dense crowd simulation", *ACM TOG* 28(5) 2009 (unilateral incompressibility), <https://gamma.cs.unc.edu/DenseCrowds>; Golas, Narain, Curtis, Lin, "Hybrid long-range collision avoidance for crowd simulation", *IEEE TVCG* 2014 | §4.6 |
| N-8 | Treuille, Cooper, Popović, "Continuum crowds", SIGGRAPH 2006 (= FF-6); Emerson, "Crowd pathfinding and steering using flow field tiles", *Game AI Pro* ch. 23 (2013) (= FF-1) | §4.7 |
| N-9 | van den Berg, Guy, Lin, Manocha, "Reciprocal n-body collision avoidance" (ORCA), ISRR 2009 / STAR 2011 (= VO-1); Sunshine-Hill (VO-4) | §4.9 |
| N-10 | Lumelsky, Stepanov, "Path-planning strategies for a point mobile automaton…", *Algorithmica* 1987 (Bug algorithms; the orbit approach of §6.2 is a Bug-2-style boundary following round a disc) | §6.2 |

The owner's StarCraft II statements (Anhalt, GDC 2011, SC-1, SC-2) have no public recording; they are cited as the
owner's and as the second-hand notes, as the solver plan did.

### 4.2 Candidate 1: layered SC2-style steering with soft separation, push priority and side-locking

**What.** Route following; a side-locked avoidance against the first non-yielding threat (time to collision, side
kept per pair); a soft separation that removes a fraction of each friendly overlap per tick, by push priority;
holding and attacking units and enemies as hard obstacles at proposal time (the step's component into them removed,
the tangential part kept). No iteration within the tick: overlap is closed over several ticks. Sources SC-1, SC-2,
SC-5, SC-8, ST-2, ST-3, RT-1.

**Fit.** R-5, R-6, R-7, R-10 by construction. R-8 only if separation outruns injection: a crowd that keeps pressing
settles where per-tick removal equals per-tick injection (the same balance as RC-2, one iteration instead of 21).
R-1 and R-2 need a goal layer it does not specify.

**Determinism, fixed point.** Straightforward: one summed pass is order-independent like C-1a; the side memory is
today's `SideMemory`. **Cost at 400 units:** one pass over ~1,600 pairs plus the steering scan: the cheapest
candidate (prototype: 6 % to 20 % of today's pair evaluations, §5).

**Our failure modes.** Fixes RC-2 (no diffusion problem, nothing to converge) and RC-3 (soft constraints are never
infeasible). Keeps RC-1 unless separation is strong, and **re-creates the legacy pipeline's history**: this is
essentially `Legacy` (proposal-time `remove_blocked_velocity`, `PUSH_RELAXATION` 0.35, the waiting rule) that the
solver replaced because of its 15.4 reversals per 1,000 at gaps and 337 give-ups (solver plan §5.2). **Migration risk:
high**, because it walks back to a measured failure. In the prototype (`SC2soft`) friend overlap is the worst of all
arms (§5).

### 4.3 Candidate 2: deterministic Gauss–Seidel or graph-coloured PBD with anticipatory constraints and shock propagation

**What.** Replace averaged Jacobi by sequential (or colour-parallel) projection: each pair's correction is applied
at once and seen by the next pair. Shock propagation orders the pairs outward from immovable anchors (walls,
enemies, holding and attacking units) and treats the inner body of each pair as fixed. Anticipatory constraints
(N-1) project predicted positions apart before contact. Sources N-1 to N-4.

**Fit.** Gauss–Seidel converges a chain in O(L) sweeps rather than O(L²) iterations (RC-2); shock propagation in one
sweep. It enforces R-5 and R-6 naturally (anchors are fixed). It does nothing for RC-1 (walkers still step in) and
RC-5 (goals), and under the owner's rules it meets RC-3 head-on: a wedged body is projected out of one fixed
partner into the next, so it is thrown back and forth (the prototype measured 1.2 to 1.5 world units per tick until
a per-body displacement cap was added, §5.3).

**Determinism, fixed point.** Exact in Q32.32 if the order is fixed, but **the order is the result**: plain
Gauss–Seidel depends on the pair list's order, which is a function of IDs or of a sort key. A geometric key (depth
from anchors, then position) removes ID dependence except on ties, but makes the result discontinuous in positions
(a body whose depth changes from 3 to 4 reorders its pairs). Graph colouring (N-4) is parallel within a colour but
still order-dependent across colours, and a deterministic colouring is itself a sort. **R-13 is weaker than C-1a**,
and T8 would have to be re-proved.

**Cost.** Fewest pair evaluations of the solver candidates: about a fifth to a quarter of today's per tick in the
prototype, at a BFS over the contact graph per tick. The cheapest way to converge.

**Our failure modes.** Fixes RC-2. Measured on the real code as the friend-crowd plan's FC-4 (−89 % overlap, −10 %
instructions) and Q0118 (c): it **moves battle 26 (9 of 12 → 2 of 12 zergling wins) and doubles reversals in one
family**, because the crowd stops pressing its front aside and queues; the owner chose (a) over (c). Keeps RC-1 and
RC-5. **Migration risk: medium** (a solver swap behind the existing switch), but its benefit depends on an intent
layer that stops injection; without it, it is a cheaper way to the queueing look.

### 4.4 Candidate 3: context steering (interest and danger maps)

**What.** Each unit scores a fixed set of directions (8 to 16 bins) by interest (towards its goal) and danger or free
distance (to bodies that will not give way), and moves along the best bin at a speed bounded by its free distance.
Source N-5 (ST-5).

**Fit.** It is the natural home for **removing RC-1**: a bin's free distance is exactly "how far can I step without
entering a body that will not move", so the step into an unpushable front is never proposed. Rear units flow round
a blocked front (R-1, R-9). It has two known weaknesses, both measured in §5: with sideways or backward bins allowed,
two blocked neighbours ping-pong (reversals); with only progress-making bins (the potential "distance to goal" must
fall), it has local minima behind a front unless the goal itself leads round (RC-5). The cure for both is to pair it
with a goal layer that has no local minimum (§6.2's orbit approach).

**Determinism, fixed point.** Bins are angles relative to the goal direction (rotation-invariant); the free distance
is one ray–disc root per neighbour per bin (a `sqrt` each, or the squared form). No float needed; no order
dependence (each unit reads the tick-start snapshot). Ties: the bin nearest the goal, then the kept side.

**Cost.** Bins × neighbours ray tests per walker: 16 × ~8 ≈ 130 root evaluations, more than today's one TTC test
per neighbour; the prototype's per-tick time under context steering was the highest of the arms (load-sensitive, so
quoted only qualitatively). It can be cut by testing only bins within ±67.5° (7 bins, §6.3) and by skipping
co-moving friends.

**Our failure modes.** Fixes RC-1 (by construction), part of RC-5 (flows round fronts), and RC-4 if the side is kept
as a hysteresis rather than per pair. Keeps RC-2 for what injection remains. **Migration risk: medium**: it replaces
I-3 inside the existing intent module.

### 4.5 Candidate 4: power-law time-to-collision forces and Implicit Crowds

**What.** An anticipatory interaction energy that falls as a power of the time to collision (N-6), integrated
either explicitly (the 2014 force model) or implicitly by minimising an energy per step with a few Newton/L-BFGS
iterations (Implicit Crowds, 2017), which is stable at large steps and dense crowds.

**Fit.** Smooth, memoryless anticipation (good for R-10 and RC-4); the energy has no hard notion of "this body may
not be pushed" (R-5, R-6) or of push priority (R-7), which would have to be encoded as asymmetric weights; the
implicit variant's own authors report that the power-law potential is discontinuous at the collision cone's
boundary and steep for grazing agents, which stalls numerical solvers (their slides), and that is exactly the dense
melee regime.

**Determinism, fixed point.** Explicit forces: feasible, with a power evaluated by a table or by repeated
multiplication (exponent 2). Implicit: a nonlinear minimisation with line search in Q32.32, convergence tests and an
iteration count that varies per tick: deterministic if fully specified, but hard to make bit-stable on four platforms
and hard to bound in cost. **Cost:** explicit ≈ today's intent; implicit several gradient and Hessian-vector
evaluations over all neighbour pairs per tick (several times the solver's).

**Our failure modes.** Rule S of the solver plan (Amendment 14 to 16) was a memoryless smooth correction in this
family; it removed oscillation everywhere and **lost the sideways slide** (arc give-ups 309 → 507). Keeps RC-1 (a
force does not forbid a step), RC-5. **Migration risk: high** (implicit) / medium (explicit); not prototyped again,
since rule S is its measurement on the real code.

### 4.6 Candidate 5: continuum hybrid with unilateral incompressibility

**What.** A coarse grid carries crowd density and velocity; a pressure-like solve enforces a maximum density
(unilateral incompressibility), and agents take the grid's velocity in dense regions and their own elsewhere
(N-7). Excess flow is diverted sideways round blocked fronts by the pressure gradient.

**Fit.** It addresses RC-1 and RC-2 at the aggregate level (a full cell cannot receive more flow), and gives organic
flow round fronts (R-1, R-9). It does not give exact radii, per-unit push priority, single-unit micro (R-7, R-11) or
the exact surround count of R-1; it needs the discrete layer anyway. Enemies, anchors and priorities would be grid
boundary conditions.

**Determinism, fixed point.** A pressure solve (conjugate gradient or projected Gauss–Seidel over cells) in fixed
point is feasible but numerically delicate (scaling, iteration counts, the unilateral projection); the coarse grid
adds a second spatial discretisation that must agree with the navmesh's terrain. **Cost:** cells rather than pairs; at
400 units on 64 × 64 a 1-cell grid has ~4,000 cells, a fixed number of relaxation sweeps: comparable to today's solver.

**Our failure modes.** Fixes RC-1 in aggregate (density cap) and RC-2; keeps RC-5; new risk: a second model to
keep consistent with the first. **Migration risk: high.** It is designed for 10⁴ to 10⁵ agents (N-7); this game's
battles are 10² to 4·10², where a discrete gate (§6.3) gives the same density cap per unit.

### 4.7 Candidate 6: flow fields with local separation

**What.** Per destination, an integration field over a grid and its gradient as the velocity, with congestion costs
routing rear units round clogged fronts; local separation keeps bodies apart (N-8, FF-1 to FF-9).

**Fit.** Scales to thousands of units with a shared goal, and has no local minima (the field is a true distance). It
is the right tool for many units to one goal (group moves) and for routing round a contested target (the solver plan's
step 2′ wavefront: one field per contested target). It does not, alone, do per-unit targets (each attacker's slot
is its own goal), push priority or Hold Position semantics, and it would replace or duplicate the navmesh, TA* and
funnel layer, which is sound and SC2-faithful (SC-2, SC-3).

**Determinism, fixed point.** Integer breadth-first or Dijkstra fields are naturally deterministic. **Cost:** one field
per goal; per contested target it was measured as part of the solver plan's step 2′ (cost reported then, built at
`75871a42`, not adopted).

**Our failure modes.** Fixes RC-5 where a field exists; keeps RC-1 to RC-3 (the separation is today's problem). As a
**local goal tool** it is the fallback of §6.2's orbit approach. **Migration risk: high** as a global replacement, low
as a local tool.

### 4.8 Candidate 7 (added): the restructured pipeline — goal choice, gated intent, convergent contact

**What.** Not a new algorithm but a re-division of responsibility, built from parts the project has measured:
- **Goal (G′):** the slot goal (D-γ) with claims kept until reached or taken (G-5 as written; the prototype
  re-claimed every tick, which churns), an orbit approach to a far slot (a Bug-2-style boundary walk round the
  target's occupied band, N-10), and retarget from a full ring (D-ε, G-6′).
- **Intent (I′):** the step is clipped to the free distance along its direction to bodies that will not give way
  (enemies, holding, attacking, friends standing under an order, oncoming movers): RC-1 removed by construction. The
  direction is chosen among bins that make progress toward the goal (a Lyapunov function, no ping-pong), with the
  side kept as a hysteresis (context steering restricted by N-5's own masking idea). The TTC side rule I-3 stays for
  oncoming movers, where it is what R-10 asks.
- **Contact (C′):** the owner's rules (R-5, R-6): holding and attacking units are never moved; enemies block (E-2:
  no step or correction closes on an enemy beyond its share of the gap), which is now feasible because no walker
  presses into a friend held at an enemy; today's Jacobi with exact sums (C-1a) and C-3′ as the residual solver.

**Fit.** Every requirement has an owner: R-1/R-2/R-9 the goal layer, R-5/R-6 the gate and the shares, R-8 the
gate (little injection) plus the solver (residuals), R-10 the kept side, R-13 the snapshot reads and exact sums.
**Determinism:** every part is from the snapshot or order-independent by C-1a. **Cost:** the gate replaces
injection the solver would otherwise remove; C-3′ then rarely extends (§7.6). **Failure modes:** removes RC-1 and
RC-3, makes RC-2 small, attacks RC-5 at the goal. It keeps the open risk of RC-4 in the bin choice (measured in §5:
reversals higher than today's in the battle stand-ins) and the claim churn. **Migration risk: medium**: every part
has a home in today's modules.

### 4.9 Candidate 8: ORCA (RVO2), rejected

**Rejected, for five reasons, each measured or sourced here before:**
1. Dense melee rings are infeasible linear programs; ORCA then falls back to a discontinuous relaxed LP (VO-4), which
   is where the battles live.
2. Full responsibility against static bodies (holding, attacking, walls) deadlocks in corners and chokes (VO-4,
   VO-5; Q0097 Update 2, VO-4 of the survey).
3. Its colliding case duplicates the contact solver, and its side keeping reads the current velocity, which contact
   jitters (Amendment 14's review).
4. Determinism: an LP in fixed point with a data-dependent active set is costly to make bit-stable (VO-7, VO-8).
5. Cost: +1 to +3 G instructions on the 64 × 64 clash (Amendment 14's estimate), for no requirement the
   restructured pipeline does not meet more cheaply.

### 4.10 Comparison

Ratings: ++ strong, + adequate, ○ neutral or needs another layer, − weak, −− contrary.

| Candidate | R-1/R-2 surround, retarget | R-5/R-6 owner rules | R-8 overlap | R-10/R-15 no dance | R-13/R-14 determinism, fixed point | Cost at 400 units | Fixes RC | Keeps RC | Migration risk |
|---|---|---|---|---|---|---|---|---|---|
| 1 SC2 layered soft | ○ | ++ | −− (balance of injection and one relaxed pass) | + (side lock) | ++ | ++ (cheapest) | 2, 3 | 1, 5 | high (walks back to `Legacy`) |
| 2 GS / coloured PBD + shock | ○ | ++ | + chains, − wedges | − (queueing; reversals ×2 measured) | − (order is the result) | ++ (¼ of today's pair evaluations) | 2 | 1, 3, 5 | medium |
| 3 Context steering | + | + | + (no injection) | − alone (ping-pong), + with a progress mask | ++ | − (bins × neighbours) | 1, part of 5 | 2 (small), 4 | medium |
| 4 Power-law TTC / Implicit Crowds | ○ | − (no hard notion) | ○ | ++ (smooth) | − implicit, + explicit | −− implicit | 4 | 1, 5 | high |
| 5 Continuum hybrid | + flow, − exact count | ○ (boundary conditions) | + aggregate | + | − (pressure solve in Q32.32) | + | 1, 2 aggregate | 5 | high |
| 6 Flow fields + separation | ++ for shared goals | ○ | ○ (separation is today's) | + | ++ (integer fields) | + per goal | 5 | 1, 2, 3 | high global, low local |
| **7 Restructured: G′ + I′ + C′** | ++ | ++ | + (prototype: §5) | ○ (measured reversals, §5) | ++ | + | 1, 3, 5; 2 small | 4 (bin choice) | medium |
| 8 ORCA | − | − | − | + (sides) / − (dense) | − | −− | — | 1, 3, 5 | rejected |

---

## 5. Prototype evidence

### 5.1 Method

- **Program.** One scratch Rust file, `f64` throughout, no dependencies (`~/scratch/movement-blank-sheet/proto/`;
  the frozen source of the reported run is `proto_frozen_main.rs`, the run script `run_all.sh`, the raw output
  `results.txt`), built in `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/movement-blank-sheet`, run as one
  single-threaded process at `nice 15` on a host at load average 50 to 170 (other users), so **wall-clock times are
  not reported**; cost is given as pair evaluations per tick in the contact solver.
- **World.** Discs on an open map with axis-aligned walls; a uniform grid for neighbours; tick-start snapshot;
  intent → proposal → terrain settle → contact solver → metrics → combat. Archetypes copy the catalog's radius,
  speed, hit points, armour, range, damage and period for the zergling, colossus, ultralisk, immortal and
  `pulse_bot` analogues; splash is a circle (the colossus' 0.9, the ultralisk's 1.0). **No navigation mesh**: goals
  are straight points (this matters for the gap and the crossing, §5.3).
- **Arms.** Built from three interchangeable parts:

| Arm | Intent | Solver | Contact rules | Retarget |
|---|---|---|---|---|
| `J21` (today's stand-in) | straight seek to the route point, or to a free slot on the target's ring (exact half-widths, claims in distance order); no slot: stand | averaged Jacobi, 21 iterations, early stop, then the summed hard pass (C-1, C-1b) | today's C-2 with D-δ's along-ring nudge, enemies ½ / ½ | no |
| `Jconv` (Q0118 (a)) | as `J21` | as `J21`, then C-3′: iterate while a friend pair is above 5 % and the excess falls, cap 250 | today's | no |
| `Jconv-O` | as `J21` | as `Jconv` | **owner's**: holding and attacking never moved by anyone; enemies by own approach; a walker's own step clamped off enemies and attackers | yes |
| `GS` (candidate 2) | as `J21` | Gauss–Seidel, 4 sweeps, pairs ordered by depth from anchors then position, shock propagation (lower depth fixed), per-body cap of one radius per tick, final summed hard pass | owner's | no |
| `Ctx+J21` | **context steering**: 16 bins relative to the goal, of which the 7 within ±67.5° are admitted (cos ≥ 0.3), score interest × free distance to bodies that do not give way, side kept (×0.6 on the other side), stand below 0.08; **orbit approach** to a far slot; step clipped to the free distance | `J21`'s | owner's | yes |
| `Ctx+JcO` (≈ candidate 7) | as `Ctx+J21` | `Jconv`'s (C-3′) | owner's | yes |
| `Ctx+GS` | as `Ctx+J21` | `GS`'s | owner's | yes |
| `SC2soft` (candidate 1) | seek, TTC side rule with kept partner, step projected off bodies that do not give way | three relaxed (½) summed passes for friends, then one hard pass | owner's | yes |

- **Scenes.** Stand-ins of matrix battles 26, 27, 28 (the matrix's squads, spacing, spawn jitter ±0.12 except seed 0,
  attack-move orders, ticks), 12 seeds; corridor (width 3, an immobile enemy pillar of radius 1, 6 to 11 attackers),
  surround (30 attackers on an immobile pillar of radius 1.5, capacity 16), gap (20 units, widths 1.6 to 3.0, mixed
  priority every fourth seed), crossing (8 against 8 head-on), 20 seeds each; two 400-unit clashes on 64 × 64 (200
  against 200 `pulse_bot`, and 200 against 200 zerglings), 300 ticks, one seed.
- **Metrics.** Friend pair-ticks above 5 % of the radius sum (per seed), the largest friend and enemy overlap, shove
  (displacement of units that proposed no motion while an enemy touched them: B2's D-SHOVE), engaged melee
  unit-ticks (B7′'s numerator, a damage proxy), stuck with room (a melee chaser standing while an enemy within 6 has
  a free ring angle: D-R10), reversals per 1,000 moving unit-ticks (M1), target switches, fill (corridor, surround),
  completion (gap, crossing), pair evaluations per tick.

### 5.2 Figures

**Battles** (12 seeds; per seed unless marked):

| Battle | Arm | friend > 5 % | friend max | enemy max | shove | engaged | stuck w/ room | M1 /1k | switches | pair evals /tick |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 26 | `J21` | 88.2 | 10.2 % | 0.1 % | 4.69 | 1,629 | 3,388 | 0.76 | 0 | 1,625 |
| 26 | `Jconv` | **0.1** | 10.6 % | 0.1 % | 4.72 | 1,694 | 3,578 | 1.38 | 0 | 1,627 |
| 26 | `Jconv-O` | 243.2 | 24.5 % | 4.3 % | 0.07 | 723 | 1,671 | 3.27 | 159 | 3,217 |
| 26 | `GS` | 216.8 | 21.4 % | 4.1 % | 0.00 | 1,059 | 3,160 | 3.44 | 0 | 343 |
| 26 | `Ctx+J21` | 122.4 | 20.9 % | 2.4 % | 0.01 | 994 | 2,723 | 5.29 | 166 | 1,679 |
| 26 | `Ctx+JcO` | 32.0 | 11.0 % | 2.5 % | 0.04 | 960 | 2,773 | 4.13 | 172 | 1,675 |
| 26 | `Ctx+GS` | 79.3 | 32.0 % | 2.2 % | 0.00 | 814 | 4,124 | 10.98 | 148 | 346 |
| 26 | `SC2soft` | 3,264.9 | 68.4 % | 15.2 % | 0.02 | 831 | 1,964 | 9.09 | 175 | 300 |
| 27 | `J21` | 277.8 | 12.3 % | 0.0 % | 1.46 | 435 | 870 | 0.31 | 0 | 3,367 |
| 27 | `Jconv` | **0.2** | 10.0 % | 0.0 % | 1.58 | 432 | 806 | 0.55 | 0 | 3,600 |
| 27 | `Jconv-O` | 26.9 | 15.5 % | 3.3 % | 0.00 | 369 | 83 | 0.50 | 177 | 4,144 |
| 27 | `GS` | 404.6 | 23.4 % | 2.8 % | 0.00 | 429 | 872 | 5.55 | 0 | 647 |
| 27 | `Ctx+J21` | 51.5 | 15.4 % | 0.8 % | 0.00 | 471 | 71 | 1.64 | 133 | 2,469 |
| 27 | `Ctx+JcO` | 5.4 | 10.0 % | 1.2 % | 0.00 | 514 | 89 | 1.62 | 129 | 2,430 |
| 27 | `Ctx+GS` | 60.9 | 27.5 % | 0.0 % | 0.00 | 523 | 229 | 2.63 | 134 | 552 |
| 27 | `SC2soft` | 3,500.5 | 51.8 % | 14.5 % | 0.00 | 356 | 54 | 2.67 | 152 | 518 |
| 28 | `J21` | 866.4 | 15.1 % | 0.3 % | 7.65 | 4,547 | 16,772 | 0.44 | 0 | 1,616 |
| 28 | `Jconv` | **10.1** | 10.9 % | 0.3 % | 10.48 | 4,648 | 16,761 | 0.86 | 0 | 1,782 |
| 28 | `Jconv-O` | 1,060.5 | 23.4 % | 9.1 % | 0.33 | 2,909 | 12,182 | 9.12 | 260 | 4,994 |
| 28 | `GS` | 886.8 | 76.8 % | 1.3 % | 0.02 | 2,784 | 16,290 | 10.07 | 0 | 393 |
| 28 | `Ctx+J21` | 404.3 | 34.0 % | 4.6 % | 0.17 | 5,257 | 13,531 | 4.72 | 298 | 1,818 |
| 28 | `Ctx+JcO` | 146.4 | 15.4 % | 3.2 % | 0.10 | 4,801 | 13,543 | 4.36 | 293 | 1,950 |
| 28 | `Ctx+GS` | 100.0 | 34.5 % | 0.8 % | 0.00 | 3,289 | 18,814 | 9.08 | 211 | 337 |
| 28 | `SC2soft` | 11,451.7 | 56.1 % | 17.7 % | 0.48 | 3,247 | 12,269 | 7.97 | 294 | 364 |

Winners: the colossi, ultralisks and immortals win all 12 seeds in every arm, except one zergling win in battle 28
under `Jconv` and under `Ctx+J21`. **The stand-in's combat is not calibrated** (straight-line acquisition, circle
splash, no random delays, no armour upgrades): `main`'s zerglings win 9 / 0 / 4 of 12 (friend-crowd plan §2.3).
Winners are therefore not read; engaged unit-ticks are the damage proxy.

**Harness stand-ins** (20 seeds):

| Scene | Arm | fill or completion | friend > 5 % /seed | friend max | M1 /1k | pair evals /tick |
|---|---|---|---:|---:|---:|---:|
| corridor | `J21`, `Jconv`, `GS`, `Ctx+*` | fill 1.00 (min 1.00) | 0.0 | 1.0–3.7 % | 0.00–0.05 | 15–18 |
| corridor | `SC2soft` | fill 1.15 (min 0.67) | 14.6 | 11.8 % | 0.09 | 25 |
| surround | `J21`, `Jconv` | fill 0.925 (min 0.875) | 0.0 | 2.8 % | 0.18 | 288 |
| surround | `Jconv-O` | 0.872 (0.688) | 0.0 | 1.9 % | 0.30 | 288 |
| surround | `GS` | 0.900 (0.750) | 0.8 | 10.2 % | 0.54 | 113 |
| surround | `Ctx+J21`, `Ctx+JcO` | 0.809 (0.688) | 0.0 | 1.4 % | 0.49 | 138 |
| surround | `Ctx+GS` | 0.750 (0.438) | 0.1 | 6.9 % | 0.32 | 81 |
| surround | `SC2soft` | 0.859 (0.688) | 423.6 | 28.2 % | 19.09 | 134 |
| gap | `J21`, `Jconv`, `Jconv-O`, `GS` | 20 / 20 complete, median 311–318 ticks | 131–140 | 62–64 % | 0.13–0.33 | 80–328 |
| gap | `Ctx+*` | **0 / 20** complete (362 of 400 units through) | 46–58 | 58–92 % | 1.2–1.3 | 46–113 |
| gap | `SC2soft` | 20 / 20, median 309 | 466 | 52.8 % | 6.66 | 69 |
| cross | `J21`, `Jconv`, `Jconv-O` | 20 / 20, median 207 | 0.0–0.2 | 4.4–12.5 % | 0.80 | 70 |
| cross | `GS` | 20 / 20, median 219 | 1.9 | 7.0 % | 0.31 | 31 |
| cross | `Ctx+*` | **13 / 20** | 0.2–0.3 | 5–21 % | 1.9–2.0 | 19–26 |
| cross | `SC2soft` | 20 / 20, median 196 | 26.6 | 24.8 % | 0.36 | 36 |

**400-unit clashes** (one seed, 300 ticks; totals):

| Clash | Arm | friend > 5 % | friend max | enemy max | M1 /1k | pair evals /tick |
|---|---|---:|---:|---:|---:|---:|
| ranged (`pulse_bot`) | `J21` | 54,559 | 94.3 % | 0.0 % | 1.07 | 14,233 |
| ranged | `Jconv` | 1,841 | 17.4 % | 0.0 % | 3.48 | 54,913 (×3.9) |
| ranged | `Jconv-O` | 1,608 | 13.6 % | 0.0 % | 2.03 | 53,367 |
| ranged | `GS` | 664 | 36.5 % | 0.0 % | 7.27 | 2,748 (×0.19) |
| ranged | `Ctx+J21` | 9,468 | 36.2 % | 0.0 % | 0.77 | 13,384 |
| ranged | `Ctx+JcO` | 788 | 13.9 % | 0.0 % | 1.51 | 21,674 (×1.5) |
| ranged | `Ctx+GS` | **159** | 29.6 % | 0.0 % | 4.52 | 2,753 |
| ranged | `SC2soft` | 67,108 | 89.7 % | 0.0 % | 2.93 | 2,812 |
| melee (zerglings) | `J21` | 43,402 | 40.4 % | 22.3 % | 0.58 | 15,271 |
| melee | `Jconv` | 1,243 | 19.8 % | 10.5 % | 23.96 | 39,923 |
| melee | `Jconv-O` | 2,584 | 91.6 % | **99.2 %** | 28.97 | 68,699 |
| melee | `GS` | 4,200 | 89.4 % | 11.6 % | 31.44 | 3,015 |
| melee | `Ctx+J21` | 16,656 | 47.5 % | 21.2 % | 1.69 | 14,847 |
| melee | `Ctx+JcO` | 864 | 21.1 % | 6.5 % | 2.22 | 26,700 |
| melee | `Ctx+GS` | **290** | 35.2 % | **0.0 %** | 5.99 | 2,765 |
| melee | `SC2soft` | 99,467 | 89.4 % | 57.8 % | 4.91 | 3,029 |

### 5.3 Reading

1. **Convergence under today's rules works, at a price, by breaking the owner's rules.** `Jconv` reaches D-α in all
   three battles (0.1 / 0.2 / 10.1 pair-ticks) as the friend-crowd plan found on the real code; it does so by
   moving engaged units (D-δ) and enemies (shove 4.7 / 1.6 / 10.5 per seed, the most of any arm). On the ranged
   clash it costs 3.9× today's pair evaluations.
2. **Under the owner's rules, a solver alone cannot keep overlap bounded (RC-3).** `Jconv-O` runs to its cap and
   still has 243 / 1,061 pair-ticks above 5 % in battles 26 / 28, at 2 to 3× the pair evaluations; in the melee
   clash bodies wedged between unpushable attackers of both sides end coincident with enemies (99 %). `GS` with shock
   propagation is the cheapest solver (a fifth of today's evaluations) but leaves 217 / 887 and a 77 % maximum in
   wedges, and doubles to quadruples reversals (3.4 to 31).
3. **The gate removes most of the load (RC-1).** With the same solver, context steering's free-distance gate cuts
   pair-ticks above 5 % from `Jconv-O`'s 243 / 27 / 1,061 to `Ctx+JcO`'s 32 / 5.4 / 146, from `J21`'s 54,559 to
   788 on the ranged clash and from 43,402 to 864 on the melee clash, with near-zero shove (enemy overlap still 1.2 % to 6.5 %,
   above B3's bound: the prototype's enemy rule is E-1's approach share, which the friend press can still load); and the gated solver
   needs only 1.0 to 1.5× today's pair evaluations, against `Jconv`'s 3.9×. With Gauss–Seidel underneath
   (`Ctx+GS`), the clashes have the fewest pairs of any arm (159, 290), no enemy overlap, at a fifth of today's cost.
4. **The gate does not yet reach D-α in the zergling battles,** and its largest overlaps (11 % to 35 %) are in the
   same wedges as `GS`'s. What remains is the residual of one tick's lag (a co-moving friend becomes a standing one a
   tick later) and the wedges.
5. **Reversals fail T1 in the gated arms** (1.6 to 11 per 1,000 against `J21`'s 0.3 to 0.8). Traces (§5.4) name two
   causes, both in the prototype's goal layer rather than the gate: slots re-claimed every tick, and the absence of
   a TTC rule for oncoming movers.
6. **Fill and the gap.** The surround's fill falls from 0.925 to 0.81 under the gated arms, and the gap and crossing
   fail to complete: the prototype has no route (a straight goal through a wall) and no TTC rule for movers, and
   progress-only bins cannot leave a wall's local minimum without one. This confirms the design's dependency (I′
   needs the funnel route's visible next point and I-3 for movers, §6.3) rather than measuring the design.
7. **Engagement.** The gated arms put more zerglings on their targets in battles 27 and 28 (514 vs 435, 4,801 vs
   4,547 engaged unit-ticks) and fewer in battle 26 (960 vs 1,629): without D-δ, zerglings can no longer slide
   engaged allies aside to reach a colossus, which is exactly the battle-26 sensitivity of Q0103 and Q0118 (c).
8. **Retarget (D-ε).** Stuck-with-room falls by 90 % in battle 27 and by 19 % in battle 28 under the gated arms; at
   the price of 130 to 300 target switches per seed (the prototype's claims are not kept, §5.4).
9. **SC2-style soft separation without iteration does not bound overlap** in any crowd (52 % to 90 % maxima): one
   relaxed pass per tick cannot keep up with injection, which is why the game's own feel needs the "blocked by
   moving units" half of SC-8, and why candidate 1 is rated as it is.

### 5.4 Traces (development, before the run of record)

- **The ping-pong.** With bins up to ±100° allowed, a zergling next to its slot alternated between +90° and −90°
  every tick for 25 ticks (surround seed 1, unit 13): each side's free distance closed after it stepped. Restricting
  bins to progress-making ones (cos ≥ 0.3) removed it (surround M1 4.71 → 0.20 on 6 seeds) and cost fill (0.88 →
  0.49) until the orbit approach was added (fill 0.82). Both changes were made on development seeds 0 to 5 of the
  surround and 0 to 3 of battles 26 and 28, before the run of record, and are disclosed as tuning of the prototype.
- **Claims re-sorted every tick** move a unit's slot between neighbouring angles from tick to tick (unit 13's slot
  moved between (31.55, 25.15), (30.99, 25.65) and (31.66, 24.97) in five ticks). G-5 keeps a claim; the prototype
  does not. This is the largest named reversal source near the ring.
- **The wedge throw.** Under Gauss–Seidel, a zergling between two unpushable bodies moved 1.48, 1.22, 1.20 and 1.17
  world units on four consecutive ticks (battle 28 seed 1, unit 26), projected out of one fixed partner into the other.
  A per-body cap of one radius per tick bounds it; it does not remove the wedge.
- **A teleport through an anchor.** The first version of the enemy clamp put the walker on the far side of a body
  it had passed the centre of; it was replaced by a bisection of the step plus its tangential rest before the run of
  record. The real code's E-2 bounds the step the same way.
- **Where the excess is.** In battle 28's seed 1, 1,504 of 1,570 friend pairs above 5 % under `J21` are pairs of two
  moving units; under `Ctx+JcO` 75 of 239 are, and 116 are a standing and a moving unit.

### 5.5 Limits of this evidence

Float arithmetic; no navigation mesh, funnel, lanes or hull turning; acceleration and braking simplified; combat not
calibrated (winners not read); one seed for the clashes; pair evaluations instead of instructions (the steering
gate's own cost is not counted in them, and the gate's ray tests are not counted). The figures rank
arms on identical scenes; they are not predictions of the real code's T-figures. No new scenario of §3.2 was run.

---

## 6. Recommendation

### 6.1 The recommendation

**Restructure, do not replace.** Adopt candidate 7 (§4.8): keep the global layer (navmesh, TA*, radius funnel,
lanes), the slot goal (D-γ), and the order-independent Jacobi solver with C-3′; move the decisive work up into goal
choice (G′) and a gated intent (I′), and put the owner's contact rules (C′) in the share table. Do **not** switch to
Gauss–Seidel with shock propagation as the default solver: it converges chains cheaply but gives up C-1a's order
independence, throws wedged bodies, and on the real code moved battle 26 and doubled reversals in one family
(Q0118 (c)); it stays a candidate for a later, behaviour-preserving cost step only if §7.6's budget is missed.

Why this and not a blank-sheet replacement:
- **The evidence points at the division of labour, not at the solver.** RC-1 (injection) and RC-5 (goal minima) are
  upstream of every solver failure; RC-3 says that under the owner's rules no solver fixes them. Each replacement
  candidate (1, 2, 4, 5, 6) still needs an intent or goal layer that does what G′ and I′ do.
- **The parts are measured.** The slot goal and its exact geometry (P1), C-1a, C-3′ (friend-crowd plan), E-2's
  enemy bound, G-6′'s retarget, the wavefront (step 2′) and the side rule exist as code or stopped branches with
  evidence. What failed each time was a part bolted on while injection continued.
- **It is the SC2 baseline, restated.** Anhalt's description (SC-1, SC-2) is exactly this layering: route, unique
  arrival points round a target, avoidance with a locked side, units blocked by holding, attacking and moving units
  (SC-8), soft pushing of idle friends. The solver's role shrinks to SC2's "separation".
- **It meets the owner's preferences without a trade.** R-5 and R-6 hold by the share table; the gate keeps the
  resulting constraint set feasible, so they no longer cost overlap (the Q0095/Q0113/Q0118 trade).

**What the prototype does and does not show.** In §5's stand-ins, the closest arm to this design (`Ctx+JcO`) cuts
friend pair-ticks above 5 % by 64 % / 98 % / 83 % (battles 26 / 27 / 28) and by 98 % in both 400-unit clashes
against today's solver stand-in, with near-zero shove of engaged units (0.04 to 0.10 per seed against 1.5 to 7.7) but enemy overlaps of 1.2 % to 6.5 %, above B3's 0.5 % bound, at 1.0 to 1.5× today's pair evaluations (C-3′ alone: up to
3.9×); it does **not** reach D-α (today's rules converged, `Jconv`, do, by pushing
enemies and nudging attackers), its reversals are 1.6 to 4.4 per 1,000 against today's 0.3 to 0.8 (T1 fails), and its
surround fill is 0.81 against 0.93. The prototype re-claims slots every tick and has no TTC rule for oncoming movers;
both are named causes in its traces (§5.4). These are the risks §7.5's stop rules are written for, and the reason the
owner is asked before an implementation plan is written (Q0120).

### 6.2 G′, goal choice

- **G′-1 Slots as today (D-γ), claims kept.** G-2 to G-5 as written in the solver plan, with P1's exact half-width.
  A claim is kept until reached, taken by a body at tick start, or invalidated by the target's move (G-5); it is not
  re-sorted every tick. Prototype finding: re-claiming every tick moves a slot between neighbours and is the largest
  single source of reversals near the ring (§5.4).
- **G′-2 Orbit approach.** A chaser whose slot lies more than a threshold angle round the target from its own
  bearing aims at a point on an outer orbit (the occupied band plus one diameter), stepped toward the slot by a
  bounded angle per tick, and dives to the slot when the angle is small. The potential "angular distance to the slot"
  falls every tick, so there is no local minimum behind the front (RC-5) and no limit cycle with the ring rule (P2's
  failure was a detour point recomputed from a ring rule that moved it back, Amendment 12). Orbit points inside
  terrain are refused; the fallback is the step 2′ wavefront (built, `75871a42`), once.
- **G′-3 Retarget from a full ring (D-ε, R-2).** An automatically acquired chase with no free slot on its target's
  ring takes the nearest enemy within its scan range whose ring has a free slot (claims counted), and keeps it until
  G-5 frees it; churn is bounded by the claim being kept. Source: G-6′ (`dc3b62ab`), Q0103's guidance. Q0123 asks
  whether it is a sanctioned deviation or the game's behaviour (the matrix's Legacy evidence: 491 target switches
  in battle 28's seed 1).

### 6.3 I′, gated intent

- **I′-1 Who does not give way.** For unit `i`, a body `j` does not give way if it is an enemy, holding, attacking
  (its target in band), immobile, a friend standing under an order (step below `STANDING_DRIFT`, not idle: a queued
  unit), or a friend moving against `i`'s direction (not co-moving, I-4's 60° test). An idle friend of no higher
  priority gives way (L6, `yields_to`); a co-moving friend gives way (it moves on).
- **I′-2 The gate.** The proposed step along a direction `d` is at most the free distance along `d` to the nearest
  body that does not give way (one ray–disc root per such neighbour). A step into such a body is therefore never
  proposed: RC-1 is removed for exactly the bodies the share table will not move.
- **I′-3 Direction.** The goal direction if its free distance allows a step of at least half the unit's speed;
  otherwise the best of the bins within ±67.5° of the goal direction (7 bins of 22.5°), scored by
  `interest × min(free, step) / step` with `interest = ½ (1 + cos)`, ties to the kept side. Only progress-making bins
  are allowed (the cosine is at least 0.38), so the distance to the goal (or the orbit point) never rises from the
  unit's own step: no ping-pong between two blocked bins (§5.4 measured the ping-pong with sideways bins allowed).
- **I′-4 Kept side.** A unit that leaves the goal direction to one side keeps that side while blocked (bins on the
  other side are scored down); the side is released when the goal direction is free again. This is the side lock of
  SC-2 and ST-2 as a hysteresis, not a per-pair partner (RC-4).
- **I′-5 Oncoming movers.** Against an oncoming or crossing mover the TTC side rule I-3 applies unchanged (its kept
  pair side and right-hand tie, I-3b): R-10 is its job, and it passed T8/T9.
- **I′-6 Stand.** No allowed bin with a step of at least an eighth of the speed: the unit stands, keeps its order and
  is not stalled while it is queued behind a body that does not give way (Q0089's rule, today's `queued`).

### 6.4 C′, contact rules (shares)

| Pair | Today (C-2 with D-δ) | C′ |
|---|---|---|
| a body holding, immobile or unpushable | 0 / 1 | unchanged |
| enemies, neither holding | ½ / ½ (B-193) | **E-2, enemies block** (enemy-contact plan §4.2, `7f686d23`): no body is moved by an enemy, and every step a body takes in the tick (its walk and each correction) is bounded so that it cannot close on an enemy by more than its share of their gap. E-2 alone measured 0.00 % enemy overlap on the real code; it stopped only because walkers kept pressing friends it held (RC-1), which the gate removes. E-1's approach shares, which the prototype used, left 1.2 % to 6.5 % (§5.3) |
| friend attacking (in band) against a friend | 0 / 1, but D-δ lets allies nudge a melee attacker along its ring | 0 / 1 always (R-5); D-δ retired under `Gated` (Q0121) |
| two friends attacking | ½ / ½ | unchanged |
| moving against idle standing | idle takes all by priority | unchanged |
| two movers | by priority, equal ½ / ½ (D-β on) | as the owner answers Q0122: D-β on (today) or "moving units block" (SC-8, `approach_shares`) |
| otherwise | ½ / ½ | unchanged |

The solver is today's (C-1, C-1a, C-1b, C-3′, C-6). B-220 (a share-1 body that terrain holds) is fixed in C′ by
C-6's own statement: a body whose correction terrain refused in an iteration counts as fixed for its pairs in the
next (P3 restricted to terrain), with the share passed to its partner; this is continuous in the iteration (no
per-tick flag) and order-independent (it reads the previous iteration's refusal).

### 6.5 The rule count

`Gated` against `Solver`: adds I′-1 to I′-4 and G′-2 (five rules, one constant each: the orbit margin, the bin
width, the progress cosine, the side weight, the stand threshold; each with its derivation in the implementation
plan) and G′-3 (no constant); removes D-δ's along-ring share, the enemy half-share and the per-pair side memory for
standing bodies (I-3 keeps it only for movers). Net: the local layer gains three rules and loses three special
cases. R-18 asks for no more.

---

## 7. Migration path

### 7.1 A third pipeline beside the current one

`contact.pipeline` gains a third value, **`Gated`**, as `Solver` was built beside `Legacy` (solver plan WP2 to WP6):
`Solver` stays the default and plays bit-identically until the switch-over package; every new rule is reached only
under `Gated`; a test per package proves `Solver` byte-identical on the development corpus and the matrix. `Legacy`
is deleted first if the solver plan's WP7 has not yet run (it removes one dead branch from every rule this round
touches); otherwise the switch carries two values.

### 7.2 Packages (outline; the implementation plan details them)

| WP | Content | Behaviour | Schema | Gate |
|---|---|---|---|---|
| **WP0** | Harness: the nine new scenarios of §3.2 with known answers, the permutation check, B1–B9 from the first stepped tick (if friend-crowd WP0 has not done it), a fresh acceptance range; baselines of `Solver` + C-3′ on all of §3 | none | — | package |
| **WP1** | C′ latent: owner shares (attackers never pushed), E-2 ported from `enemy-contact-plan` (enemies block), B-220's terrain-held transfer; data switches inside `Gated` | `Gated` only | none (no hashed field) | package |
| **WP2** | I′: who gives way, the gate, progress bins, the kept side (hashed: one side sign per unit), stand rule | `Gated` only | next free | package |
| **WP3** | G′: kept claims, orbit approach (wavefront fallback once), retarget from a full ring | `Gated` only | next free if a field is added | package |
| **WP4** | Switch-over: held-out run (confirms), goldens on four builds, matrix restated (§7.7), throughput by instructions, showcase storyboard on the physical GPU | yes | — | full |
| **WP5** | Deletion of `Solver`'s superseded rules (D-δ's along-ring share, the side memory for standing bodies) and of `Legacy` if still present | none | next free | full |

Package boundaries follow the layers so that each is judged alone: WP1 changes only who moves in a contact, WP2 only
what a unit proposes, WP3 only where it aims. Each behaviour package lands with tests that are red before it.

### 7.3 What happens to the deviations

| Deviation | Under `Gated` | Why |
|---|---|---|
| **D-α** (friends ≤ 5 % moving, ≤ 1 % at rest) | **Kept**, as the bound R-8 enforces | It is the tolerance of a soft solver; nothing here needs it relaxed |
| **D-β** (moving friends push by priority) | **Re-asked (Q0122)**; default kept | The friend-crowd diagnosis measured D-β off at −88 % overlap in battle 26, and SC-8 says moving units block; but SC-5/SC-9 support pushing. Under the gate, D-β off is closer to "a walker never pushes a friend that also wants to move", which matches I′-1 |
| **D-γ** (the concave as a slot goal) | **Kept**, restated as G′-1 with kept claims | Its mechanism is unchanged |
| **D-δ** (allies nudge an attacking melee unit along its ring) | **Retired under `Gated`** (Q0121, default retire) | The owner's preference (R-5). Its job (making room for the corridor's second rank and for the surround) moves to G′-2 and G′-3; the corridor two-abreast figure (the solver plan's R8) is reported, not a threshold, as before Q0095 |
| D-ε (retarget from a full ring; Q0110/Q0103 draft name) | **Proposed** (Q0123) | B-192's fix |
| S-1 to S-6, S-snapshot-cap | unaffected | — |

### 7.4 What happens to the open bugs

| Bug | Under `Gated` | Package |
|---|---|---|
| **B-192** (stand behind a full ring) | Fixed by G′-3; battle 28's `known_bug` marker removed when it holds | WP3, WP4 |
| **B-193** (enemy shoving) | Fixed by C′ (E-2, enemies block), made feasible by I′'s gate (E-2's own stop was RC-1) | WP1, WP2 |
| **B-209** (zerglings under the colossus) | Orthogonal: collision classes decide who touches; parked on Q0117. Its failure (overlap under the colossus) was RC-1 plus RC-2; the WP3 data change is re-measured on `Gated` after WP4 | after WP4 |
| **B-219** (crowd overlap; C-3′ the interim fix) | Fixed by C-3′ now; under `Gated` the gate removes most of the load, so C-3′ rarely extends (measured in WP4 against §7.6) | interim, WP4 |
| **B-220** (terrain-held 0/1 shares) | Fixed by C′'s terrain transfer (§6.4) | WP1 |
| **B-213** (cliff walkers and jumpers) | Out of scope (a mover feature, plan `45-round6-authored-movement-layers`); unaffected | — |
| B-190, B-191 (give-ups with room, overlap maxima in gap/corner) | Expected to fall (G′-2, B-220); judged by T4/T7 | WP3, WP4 |

### 7.5 Acceptance criteria and stop rules (pre-registered)

Binding on the implementation plan; written now, before any `Gated` code exists.
1. **Properties, not winners.** Each behaviour package passes on the §3 suite's thresholds (T1 to T9, the new
   scenarios' known answers, B1 to B9) on the development range under the solver plan's Amendment 8 step rule
   (nothing worse than its predecessor beyond the noise rule) and is confirmed once on a fresh acceptance range.
2. **The owner's rules are hard checks, not figures:** zero displacement of holding and attacking units by others
   (hold-line, D-SHOVE, B2), enemy overlap ≤ 0.5 % (B3), in every scenario.
3. **Overlap:** friend pair-ticks above 5 % in battles 26 to 29 and the two 400-unit clashes at most those of
   `Solver` + C-3′ beyond noise, and T7 in every harness family.
4. **Oscillation:** T1 in every family and in battles 26 to 29 (M1 per 1,000 moving unit-ticks ≤ 1.0). The
   prototype failed this (§5); it is therefore the first stop rule of WP2.
5. **Stop rules.** A package that breaks the step rule stops and reports; **one** amendment attempt per package,
   with a literature source; no tie-break, hysteresis or constant chosen after a trace (Amendment 14's bar). If WP2
   stops twice, the round ends with `Solver` + C-3′ as the movement of record and the evidence goes to the owner; no
   third design round is started without the owner (Q0097's precedent).
6. **Held-out** 9,000,001 to 9,001,000 is run once, at WP4, both pipelines from one binary; it confirms the
   development table and is never tuned on.

### 7.6 Cost budget

- **Floor (R-16):** every throughput scenario ≥ 50 ticks/s on the reference machine (Q0093 (c)), measured by
  instructions retired and projected as the solver plan did.
- **Budget against today:** `Gated` costs at most what `Solver` + C-3′ costs on each scenario (C-3′ is the
  owner-accepted cost of convergence: +62.5 % / +0.1 % / +51.7 % on 64 × 64 / 128 × 128 / canyon in its prototype).
  The gate's own cost (bins × non-yielding neighbours) is expected to be paid back by C-3′'s extension rarely running;
  the prototype's pair evaluations under `Ctx+JcO` were at or below `J21`'s in the battle stand-ins (§5).
- **If the budget is missed:** behaviour-preserving optimisation only (bin pruning, caching the non-yielding set per
  tick), proved byte-identical; then, only by the owner's leave, a solver cost step (graph-coloured Gauss–Seidel for
  the C-3′ extension only), which is a behaviour change and re-runs §7.5.

### 7.7 Close battles under §8

- **Battle 28** carries a StarCraft II claim (immortals are overrun); `Gated` must restore it (B-192), and its
  `known_bug` marker is removed only when it holds on the seeds of record and over the matrix's range.
- **Battles 26, 27, 29, 33, 38a, 38b and the close-battle list** fixed in WP0 are restated with traces under Q0103's
  guidance when they move: a moved winner is not a stop by itself; a moved winner whose trace shows a broken
  behaviour (standing with room, shoving, oscillation) is a bug and stops the package.
- **Battle 26 specifically:** its claim was already restated once (Q0103) and depends on how hard the crowd presses
  its front (Q0118 (c)'s 9 → 2 of 12); with D-δ retired, zerglings no longer slide engaged allies aside, so the
  outcome may move. The claim is re-examined with the colossus walk-under (Q0117), not tuned.

---

## 8. Out of scope (decisions recorded so they can be brought back)

- **The global layer** (navmesh, TA*, funnel, lanes): sound and SC2-faithful (SC-2, SC-3); unchanged.
- **Collision classes (B-209, B-211, Q0117):** who touches whom is orthogonal; re-measured after WP4.
- **Air units' separation:** the air layer keeps its own contact; the gate applies to it unchanged.
- **Flow fields as a global router (candidate 6), the continuum layer (candidate 5) and Implicit Crowds
  (candidate 4):** not pursued at this game's unit counts; revisit if a scenario of 1,000+ units is ever required.
- **Graph-coloured Gauss–Seidel:** kept only as a later cost step (§7.6), never as a default without re-proving R-13.
- **Prototype of the nine new scenarios:** not run in this round (§3.2); WP0 runs them first.
- **A fixed-point prototype:** the float prototype answers only comparative questions; R-13/R-14 are checked by the
  implementation's goldens on four builds.

---

## 9. Owner questions (filed as Q0120–Q0124 in `docs/questions/open/`)

```markdown
# Q0120: Rebuild movement as "aim well, never step into what won't move, then separate", keep today's solver, or swap in a different solver?

- **Asked:** (date of filing)
- **Blocking:** only the implementation plan of the movement redesign; the interim crowd fix (C-3′, Q0118 (a))
  proceeds either way.
- **Default until answered:** (a) is planned but not built; today's movement with C-3′ stays.

## Background

You asked for a blank-sheet look at movement. Every movement failure of the last weeks has one source: a unit walks
its full step into units that will not move out of its way (fighting allies, enemies, units queued in front), and
the solver that separates units afterwards has to undo it. With your rules (fighting units and Hold Position are
never pushed, enemies never push), there is often no way to undo it at all, so crowds overlap.

## Options

- **(a) Restructure (recommended).** Units choose free spots round a target and switch to an enemy with room when
  theirs is full; a unit never steps into a unit that will not move (it steps round it or waits); the solver stays
  as it is and only cleans up small overlaps. Close to how StarCraft II describes its own movement. Built beside
  today's movement and switched over only if it passes the tests written in advance. In a rough prototype it cut
  crowd overlap by two thirds or more while hardly moving fighting units, but units changed direction more often
  than today and still overlapped enemies slightly, which the build must fix first.
- **(b) A faster solver (Gauss–Seidel with shock propagation).** Cheaper and converges in one pass, but crowds queue
  behind their front line (battle 26: zerglings win 2 of 12 instead of 9), and results depend on the order units are
  processed in.
- **(c) Stop here.** Keep today's movement with the crowd fix you approved in Q0118.

## Answer

```

```markdown
# Q0121: Under the new movement, may allies still nudge a fighting melee unit sideways (D-δ)?

- **Asked:** (date of filing)
- **Blocking:** no
- **Default until answered:** (a), retire it under the new movement only.

## Background

In Q0095 you allowed allies to nudge an attacking melee unit along its attack ring (D-δ), so that a corridor's
second rank and a crowded surround find room. In Q0118 you said attacking units should not be pushable if a cleaner
design handles corridors and surrounds without it. The new design makes room by choosing free spots and switching
targets instead.

## Options

- **(a) Retire D-δ under the new movement (recommended):** fighting units are never moved by allies, as in
  StarCraft II. In a corridor only the front rank fights, as before Q0095.
- **(b) Keep it as a data switch,** default off.
- **(c) Keep it on.**

## Answer

```

```markdown
# Q0122: When two of your units both want to move and bump, should the higher-priority one push the other (today), or should they block each other as moving units do in StarCraft II?

- **Asked:** (date of filing)
- **Blocking:** no
- **Default until answered:** (a), today's rule.

## Background

You sanctioned D-β: moving friends push each other by push priority. StarCraft II sources disagree on this: one says
units push their friends, another that moving units block each other. Measured on today's movement, "block" removes
about 88 % of crowd overlap in the zergling battles; the new design already stops units from walking into friends
that are queued and standing.

## Options

- **(a) Keep pushing by priority (D-β on).**
- **(b) Moving units block each other;** each takes back its own step. Idle units are still pushed.
- **(c) Measure both in the new movement's first package and decide then** (recommended).

## Answer

```

```markdown
# Q0123: Should a zergling whose target is fully surrounded switch to the nearest enemy that still has room?

- **Asked:** (date of filing)
- **Blocking:** no
- **Default until answered:** (a).

## Background

Today a zergling whose automatically chosen target is surrounded stands behind the crowd while another enemy close
by has free space (bug B-192); in battle 28 the immortals win most runs because of it, though in StarCraft II
zerglings overrun immortals. The earlier fix (switch to the nearest enemy with room) worked for battle 28 but was
built on a solver that shoved enemies and was stopped. StarCraft II's exact rule for this is not documented.

## Options

- **(a) Switch to the nearest enemy with room within scan range, and keep it until reached (recommended);** recorded
  as a minor deviation (D-ε) unless a source shows the game does the same.
- **(b) Keep standing (today).**

## Answer

```

```markdown
# Q0124: May the new movement cost as much as today's movement with the crowd fix, if it stays above 50 ticks per second?

- **Asked:** (date of filing)
- **Blocking:** no
- **Default until answered:** (a).

## Background

Your bar is at least 50 ticks per second on every benchmark (Q0093). The crowd fix you chose in Q0118 costs about
+50 % to +60 % on the two 400-unit benchmarks. The new movement adds a check per unit but should let the crowd fix
run less often.

## Options

- **(a) Budget = today's movement with the crowd fix, and ≥ 50 ticks/s (recommended).**
- **(b) Budget = today's movement without the crowd fix** (stricter; may need a faster solver, with the trade-offs of
  Q0120 (b)).

## Answer

```

---

## 10. Risks

| Risk | Why | Mitigation |
|---|---|---|
| The gate makes crowds too polite (queues instead of pressing) | It is exactly Q0118 (c)'s look, reached by intent instead of solver | Idle friends still yield; D-β per Q0122; battle 26 restated under §8 with traces; R-9 checked by deep-queue and D-R10 |
| Bin choice oscillates (RC-4 again) | §5's arms had 4 to 11 reversals per 1,000 | Progress-only bins, kept side, kept claims (each a named cause in §5.4); T1 is WP2's first stop rule |
| Orbit approach meets terrain | An orbit point inside a wall | Refused points; the wavefront fallback once (built) |
| Retarget churn | Two targets alternating | Claims kept until reached (G-5); D-CHURN checked |
| Cost of the gate | 7 bins × non-yielding neighbours per walker | Measured per package; §7.6 |
| The prototype misleads | Float, simplified intent, crude combat | Used only for direction; every acceptance figure comes from the real code on the §3 suite |

