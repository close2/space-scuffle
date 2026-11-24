# Blueprint: Enemies Block, Never Push, under the Contact Solver (B-193), Then Retarget from a Full Ring (B-192)

This plan is the second attempt at the two defects that plan
[`2026-11-24-solver-retarget-and-shoving`](2026-11-24-solver-retarget-and-shoving.md) (on branch `solver-followup`,
head `6a96cd15`; hereafter **the first attempt**) set out to fix. The first attempt stopped under its own rules. Its
plan, evidence and parked code are the inputs of this one.

- **B-193, enemy shoving.** A crowd of melee attackers pushes the enemies it fights. The contact solver splits every
  enemy overlap evenly (C-8 of plan [`2026-11-20-contact-solver`](2026-11-20-contact-solver.md), hereafter **the
  solver plan**), so a crowd's press reaches the enemy through its front rank. In StarCraft II enemies never push
  enemies (patch 1.2.0; `AlliancePushable`, patch 1.3.0; high confidence; the first attempt's §2.2).
- **B-192, standing behind a full ring.** Zerglings whose automatic target is surrounded stand behind the front row
  instead of switching to an enemy with room. Battle 28 carries `known_bug: Some("B-192")`.

**What this plan does:**
1. **Root cause of the first attempt's failure, measured** (§2). Three causes, one of them a defect on `main` today.
2. **A survey** of how engines and crowd simulations keep enemies from pushing each other at zero penetration (§3).
3. **A different design for B-193, E-2 "enemies block"** (§4): an enemy is never moved by an enemy's correction or
   press; instead, every step a body takes in the tick, its own walk and every correction the solver gives it, is
   bounded so that it cannot close on an enemy by more than its share of the gap between them. Prototyped on the
   first attempt's tree: largest enemy overlap 0.00 % in every crowd battle (base 1.4 % to 2.8 %, E-1 10 % to 13 %),
   and the immortals', colossi's and stalkers' shove 0.000 (§2.5).
4. **B-193 alone first** (package P2), and B-192 afterwards in its own package (P3), on P2's merged tree (§5).
5. **Acceptance and stop rules written in advance** (§7), keeping the first attempt's checks B1 to B7, its close-battle
   list and its held-out seeds, with the definition problems its evidence found repaired.

**Status.** Stage 1 blueprint under charter §6, written on 2026-11-25 (session date 2026-11-24) on branch
`enemy-contact-plan`, cut from `main` `a59368a3` (checksum schema 46, contact solver the default pipeline).
- **Line numbers drift,** so search for the quoted symbol.
- **Claims not confirmed while planning** are marked **verify**.
- **Later decisions** are appended as `## Amendment N (role, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** It changes a mechanic (contact between enemies), moves battle outcomes and the golden
replays, and spans several packages. It also fixes a solver defect found while planning (B-208, §2.2).

**Numbers (provisional, next free at merge):**
- Bug rows from **B-208**: B-208 (the solver's stale pair cache, §2.2) and B-209 (the colossus collides with small
  ground units, which 5.0.14 does not, §2.6).
- Questions: **Q0110** (deviation D-ε, retarget from a full ring) and **Q0111** (cost, conditional) keep the first
  attempt's numbers for the same questions. No new question is needed; new numbers would start at **Q0113**.
- **Checksum schema:** P2 adds **no hashed state** (§8), so it takes no schema number. P3 takes the next free one at its
  merge (`main` is 46; weapon validation WP7 may take 47 first).

**Inputs read for this plan:**
- The first attempt's plan and evidence (`git show solver-followup:docs/plans/2026-11-24-solver-retarget-and-shoving.md`,
  `…:docs/evidence/2026-11-24-solver-retarget-and-shoving.md`), its WP0 diagnostics (`32491bac`) and E-1 (`940f6832`).
- The parked G-6′ on `solver-followup-wp2-wip` (`dc3b62ab`) and the original stopped G-6′ on `contact-solver-g6p-wip`.
- `crates/sim_core/src/movement/solver.rs` and `collision.rs` (`solve_contacts`) on `main`.
- `DESIGN.md` §4 rows B-188 to B-207; Q0052, Q0093, Q0095, Q0102, Q0103; `docs/questions/open/` (Q0112 only, unrelated).
- 5.0.14 game data under `~/scratch/combat_wp3_data/` (`liberty/UnitData.xml`), for §2.6.
- The owner's standing guidance: AGENTS.md §8 (natural movement outweighs a close matchup's winner; broken behaviour
  is a bug) and the pathfinding feedback (no fitting to the test set; simpler deviations allowed; survey first).

---

## 1. What the first attempt did and how it failed

**E-1** ("each body in an enemy pair takes the share of the gap it closed itself", `approach_shares` from each
body's displacement since the tick start, recomputed every iteration, with a held rule and a committed-step fallback):

| Battle (24 seeds) | Base max enemy overlap | E-1 | Red (pressed side) shove, base → E-1 (Legacy) | Wins blue/red, base → E-1 |
|---|---:|---:|---|---|
| 26, 100 zerglings vs 6 colossi | 2.59 % | 12.71 % | 4.89 → 1.56 (0.13) | 18/6 → 3/21 |
| 27, vs 8 ultralisks | 2.45 % | 10.37 % | 2.38 → 1.07 (0.18) | 0/24 → 0/24 |
| 28, vs 10 immortals | 1.42 % | 1.74 % | 2.87 → 0.075 (0.37) | 5/19 → 0/24 |
| 29, vs 12 stalkers | 2.77 % | 10.51 % | 3.84 → 0.66 (0.63) | 24/0 → 24/0 |

Its evidence read the overlap as "a body ringed by enemies that do not close takes every pair's whole correction,
from both sides" and recorded that turning off the held rule or the fallback did not help, while the even split gave
1.37 %. **G-6′** on top of E-1 then failed B1, B3 (up to 20.6 %), B4 and B7, flipped battle 33, failed battle 28 on
seed of record 33, and cost +4.0 % on 64 × 64 with Legacy +1.4 % to +3.5 % (its shared admission test).

---

## 2. Root cause (measured while planning)

### 2.1 Method

- A scratch worktree at `6a96cd15` (detached, never committed; removed after planning) instrumented
  `ContactSolver::solve`: when an enemy pair ends a solve above a threshold, the solve is re-run from its saved
  bodies with every correction, share, application and terrain step of the pair's two bodies printed. The re-run is
  bit-identical, since the solver is a function of its bodies. The instrumentation is kept as
  `~/scratch/enemy-contact-plan/diag-instrumentation.diff` (against `6a96cd15`).
- Environment switches of that scratch build (never shipped): `ZZ_FIXCACHE` (§2.2's fix), `ZZ_CAP` and `ZZ_REWIND`
  (two candidate repairs of E-1, §2.4), `ZZ_BLOCK` (a float prototype of §4's E-2), `ZZ_NOPROJ` (E-2 without its
  projection).
- Runs: the matchup trace on a sub-matrix of 14 battles (9a, 10, 23a, 26, 27, 28, 29, 31, 32, 33, 36a, 36b, 38a, 38b)
  and on the full matrix, each on the first attempt's 24 seeds, one at a time under `flock ~/scratch/gate.lock`.
  Outputs: `~/scratch/enemy-contact-plan/` (`m/<arm>/matrix.csv`, `full/<arm>/`, `s96.txt`, `s9834.txt`, `s29.txt`).
- **Calibration.** The scratch build with `enemies_push_each_other: true` reproduces the first attempt's base `b0`
  exactly (battle 26: 18/6 wins, shove 36.57 / 4.89, 2.59 %), and with `false` its E-1 `b1` exactly (3/21, 12.71 %).
  Battle 26 seed 19 reproduces the evidence's worst pair: tick 96, colossus 105 and zergling 89, 12.71 %.
- The figures below are on the first attempt's tree (`main` `708576ac` plus its diagnostics), not on today's `main`.
  WP0 re-derives every figure on the package base.

### 2.2 Cause 1 (a defect on `main`): the enemy pass reuses stale pair corrections (B-208)

`ContactSolver::accumulate` reuses a pair's cached correction when neither body `moved` "in the latest
application" (`BodyStep::moved`). But `apply` clears `moved` for **every** body at each application, including the
bodies it gives no correction. The sequence at the end of a solve is:

1. the last averaged iteration computes every pair's correction (the cache) and applies it;
2. `accumulate(Pass::HardFriends)` refreshes only the hard friend pairs, and its `apply` clears `moved` of every body;
3. `accumulate(Pass::EnemyAndFixed)` now sees `moved == false` for a body that **did** move in step 1, and reuses the
   enemy and fixed pairs' corrections computed **before** step 1's application.

**Evidence (solve 96 of battle 26 seed 19, `s96.txt`).** In the enemy pass, zerglings 77, 78, 87 and 99 contribute
corrections to colossus 105 of (0.0092, −0.0127), (−0.0058, −0.0255), (0.0046, −0.0227) and (−0.0256, −0.0338),
summing to (−0.0175, −0.0947). At that moment their overlaps with the colossus are −0.07 %, 0.15 %, 0.06 % and
1.7 %: three of the four do not overlap at all. The corrections are those of the last averaged iteration's start. The
colossus, which proposed no motion, is carried 0.095 south into zergling 89: 12.71 %.

**Effect of the fix alone** (a fresh correction in the summed passes; sub-matrix, 24 seeds, max enemy overlap):

| Battle | Even split (`main`'s rule) | Even + fix | E-1 | E-1 + fix |
|---|---:|---:|---:|---:|
| 26 | 2.59 % | 2.23 % | 12.71 % | 10.42 % |
| 27 | 2.45 % | 1.29 % | 10.37 % | 8.78 % |
| 28 | 1.42 % | 1.26 % | 1.74 % | 1.75 % |
| 29 | 2.77 % | 1.69 % | 10.51 % | 9.37 % |
| 38a / 38b | 0.94 / 1.19 % | 0.56 / 0.47 % | 0.94 / 1.19 % | 0.56 / 0.47 % |

It is a real defect of today's solver (about half of `main`'s own enemy overlap in battles 27 and 29), and a fifth
of E-1's. It is not the whole of E-1's failure.

### 2.3 Cause 2 (E-1's rule): a ratio of closings is discontinuous, so the shares flip every iteration

E-1's share is `closing_a / (closing_a + closing_b)`, recomputed each iteration from each body's displacement since
the tick start. A body displaced by 0.001 towards its partner, against a partner displaced by 0, takes **the whole
overlap**, however large. Displacements inside a crowd are themselves the previous iteration's corrections, so the
rule feeds back on itself.

**Evidence (solve 9834 of battle 26 under E-1 with the cache fixed, `s9834.txt`; pair 89–105, one line per
iteration):** shares (0.091, 0.909), (1, 0), (0.927, 0.073), (0, 1), (1, 0); the colossus moves +0.12, −0.09,
+0.12, −0.09 in y on alternate iterations. More iterations do not converge a bang-bang map. In the even split the
corrections are continuous in the positions, and the Jacobi average converges.

### 2.4 Cause 3 (the geometry): a press drives melee bodies into gaps narrower than themselves

The pairs that stay overlapped are wedges:
- zergling 89 between colossi 103 and 105, whose discs leave a gap of 0.29 for a zergling 0.75 wide (`s96.txt`);
- zergling 46 between stalkers 106 and 108, gap 0.59 (`s29.txt`, battle 29, 10 % over ticks 10,099 to 10,107).

What drives them in is the friendly press: the wedged zergling overlaps engaged friends by 25 % to 46 % of the radius
sum (pairs 56–89, 64–89, 34–46). Those friends are engaged melee units under D-δ, whose friend corrections keep only
their ring-tangential part, so the press is not relieved among friends.

In a wedge **no share rule can reach zero overlap without moving an enemy**: either a flanking enemy yields (the
even split opens the wedge, which is the shove of B-193) or the overlap stays. The two repairs of E-1 tried while
planning confirm it:

| Arm (all with the cache fix) | 26 | 27 | 29 | Red shove 26 / 27 / 28 / 29 |
|---|---:|---:|---:|---|
| **Cap**: each body yields at most what it closed; the rest, a carried overlap, split evenly (continuous) | 8.54 % | 5.25 % | 8.37 % | 1.70 / 1.18 / 0.075 / 0.76 |
| **Cap + rewind**: after the last pass, a body that closed on an enemy is walked back along its own path to first contact | 1.54 % | 5.66 % | 10.13 % | 0.32 / 0.73 / 0.033 / 0.43 |
| **E-1 + rewind** | 8.26 % | 7.88 % | 9.19 % | 0.52 / 0.73 / 0.018 / 0.22 |

The rewind fails where the wedged body's own tick start is already inside the wedge (a carried overlap from an
earlier tick), which is what a sustained press produces.

**Answer to the question asked of this diagnosis** ("when a mover is the only side that closed, is its correction
clipped by the Jacobi averaging, the iteration count or the hard-pass ordering?"):
- **Averaging, yes:** a wedged body's enemy corrections are divided by its contact count, which includes the friend
  pairs pressing it in. The averaged iterations settle at a balance of press and correction with the overlap
  nonzero.
- **Hard-pass ordering, yes, in two ways:** the enemy pass is a single summed step from one set of positions, so two
  opposite wedge walls cancel; and it reuses stale corrections (§2.2).
- **Iteration count, no:** under E-1 the shares flip (§2.3), so more iterations do not converge.

The underlying cause is that every variant **lets a body into an enemy's disc and then tries to push it out**. The
fix is to not let it in (§4).

### 2.5 The prototype of E-2 ("enemies block", §4)

Float prototype in the scratch build: every body's step (its committed walk, then each averaged and summed
correction) is bounded against every enemy so that the pair cannot close by more than the gap between them, shared by
the two bodies' committed closing (½ / ½ when neither walked towards the other). Enemy pairs keep `main`'s even split
for whatever overlap remains. The cache fix is on.

Sub-matrix, 24 seeds (shove: per-seed mean of the first attempt's D-SHOVE):

| Battle | Wins b/r, base → E-2 | Max enemy overlap, base → E-2 | Red shove, base → E-2 (Legacy) | Blue shove, base → E-2 | R10 free, base → E-2 | Shots/unit-s blue, base → E-2 |
|---|---|---|---|---|---|---|
| 9a | 0/24 → 0/24 | 0.00 → 0.00 % | 0.077 → 0.104 | 0.01 → 0.00 | 0 → 0 | 0.582 → 0.594 |
| 10 | 0/24 → 0/24 | 0.44 → 0.00 % | 1.034 → 0.927 | 0.07 → 0.00 | 401 → 368 | 0.607 → 0.601 |
| 26 | 18/6 → 1/23 | 2.59 → 0.00 % | 4.888 → 0.000 (0.130) | 36.57 → 12.30 | 69,561 → 51,716 | 0.599 → 0.515 |
| 27 | 0/24 → 0/24 | 2.45 → 0.00 % | 2.380 → 1.106 (0.178) | 29.30 → 18.00 | 22,579 → 24,614 | 0.391 → 0.386 |
| 28 | 5/19 → 1/23 | 1.42 → 0.00 % | 2.873 → 0.000 (0.374) | 17.99 → 13.90 | 318,843 → 319,585 | 0.420 → 0.399 |
| 29 | 24/0 → 24/0 | 2.77 → 0.00 % | 3.838 → 0.000 (0.628) | 20.11 → 15.85 | 292,449 → 291,958 | 0.268 → 0.264 |
| 33 | 24/0 → 24/0 | 0.00 → 0.00 % | 0.138 → 0.124 | 0.01 → 0.00 | 11,592 → 11,486 | 1.230 → 1.230 |
| 38a | 24/0 → 24/0 | 0.94 → 0.00 % | 0 → 0 | 1.87 → 1.74 | 9,788 → 9,787 | 0.339 → 0.338 |
| 38b | 0/7 → 0/6 | 1.19 → 0.00 % | 0 → 0 | 1.09 → 1.03 | 9,663 → 9,652 | 0.346 → 0.339 |

The remaining "shove" is the first attempt's metric counting friendly D-δ nudges (its evidence §2.3): the ultralisks
of 27 are melee units nudged by allied ultralisks; battle 10's red zerglings likewise. §7.1 repairs the metric.
Battles 31, 32, 36a and 36b are unchanged. The full matrix under E-2 is in §2.7.

Without the projection (the step only scaled, `ZZ_NOPROJ`), overlap is also 0.00 %, but blue's firing rate in 26
falls further (0.491) and 26 goes to 0/24: a body grazing an enemy stops dead instead of sliding. §4 keeps the
projection.

### 2.6 A confound found while planning: the colossus does not collide with zerglings in 5.0.14 (B-209)

5.0.14's `Colossus` carries `Collide` flags `Colossus`, `Structure` and `Flying` only (`liberty` UnitData:4066–4068),
while the Zergling, Ultralisk, Immortal and Stalker carry `Ground` and `Small` (`liberty` UnitData). Collision is by
matching flags, so small ground units walk beneath a colossus, and colossi block only colossi. Our analogue
(`tripod_annihilator`) collides with every ground body. Battle 26 therefore measures a ring of zerglings round a body
they would walk under. Under E-2 the zerglings lose battle 26 (1/23), because they can no longer prise the colossi
apart to wedge in. That is the correct consequence of a contact that should not exist.

This plan registers **B-209** (data-driven collision classes for bodies, so that a colossus analogue passes over small
ground units) and does not fix it: it is a new capability (a catalog field and the pair filter), independent of
B-193. Battle 26 is handled by a pre-registered rule (§7.3).

### 2.7 Full matrix under the prototype

1,464 runs per arm (`full/{base,fix,block}`). Base reproduces the first attempt's `b0`. Every battle whose
survivors moved in any arm (all others are identical in every run):

| Battle | Base: wins b/r, fails record/all, max enemy overlap | K-1 only | K-1 + E-2 |
|---|---|---|---|
| 9a, 23a | 0/24, 0/0, 0.00 % | same | same |
| 10 | 0/24, 0/0, 0.44 % | 0/24, 0/0, 0.00 % | 0/24, 0/0, 0.00 % |
| 26 (Class R) | 18/6, 0/0, 2.59 % | 17/7, 0/1, 2.23 % | **1/23, 1/8**, 0.00 % |
| 28 (Class R, B-192) | 5/19, 2/19, 1.42 % | 6/18, 2/18, 1.26 % | 1/23, 4/24, 0.00 % |
| 29 | 24/0, 0/0, 2.77 % | 24/0, 0/0, 1.69 % | 24/0, 0/0, 0.00 % |
| 33 (Class R) | 24/0, 0/0, 0.00 % | same | 24/0, 0/1, 0.00 % |
| 37a | 0/0, 0/0, 69.13 % | same | same |
| 38a / 38b (Class R) | 24/0 and 0/7, 0/0, 0.94 / 1.19 % | 0.56 / 0.47 % | 24/0 and 0/6, 0/0, 0.00 % |

Totals: failures on the seeds of record 2 → 2 → 5 (26 once, 28 four times); over all seeds 31 → 31 → 45. **No Class
D battle flips or fails a seed of record.** Battle 37a's 69 % is Abduct pulling a tank onto a zergling (a placement
by an ability, identical in every arm): an overlap E-2 does not create and C-8's even split still separates, which is
why B3 compares each battle with its own base.

---

## 3. Survey: keeping enemies from pushing each other at zero penetration

| # | Approach | Where | Pushing by enemies | Penetration | Fit for us |
|---|---|---|---|---|---|
| S-1 | **Inverse-mass weights; a static body has inverse mass 0** | Position-based dynamics: Müller, Heidelberger, Hennix, Ratcliff, "Position Based Dynamics", *JVRCI* 2007; Bender, Müller, Macklin, "A Survey on Position Based Dynamics", Eurographics tutorial 2017. Our C-2 already uses it for holding and fixed bodies | None against the static body | Zero against one static body; a body between two static ones (a wedge) cannot be resolved by summing | Correct only when we know which enemy is "static". In a fight both sides are engaged and either may move this tick |
| S-2 | **Per-pair mass ratios that depend on state** ("who closed", stacking mass scaling) | Macklin, Müller, Chentanez, Kim, "Unified Particle Physics for Real-Time Applications", *ACM TOG* 33(4), 2014 (mass scaling by height for stacks; Jacobi with constraint averaging) | Reduced | Ratios that change with state make the projection discontinuous; Macklin et al. fix the scaling per step for that reason | This is E-1, which flipped (§2.3). A ratio frozen per tick is safe; one recomputed per iteration is not |
| S-3 | **Kinematic movers: a body's own motion is swept against others and never penetrates; other movers are obstacles, never moved** | PhysX character controller ("collide and slide"; "precise sweep tests … enough to avoid all penetration"; two controllers are "effectively kinematic … neither the PhysX SDK nor the CCT module should be allowed to move them"), [PhysX 5.4 Character Controllers](https://nvidia-omniverse.github.io/PhysX/physx/5.4.0/docs/CharacterControllers.html); Fauerby, "Improved Collision Detection and Response", 2003 | None | Zero by construction for sequential movers | **The game's behaviour.** Its sequential sweeps depend on the order of movers, which C-1a forbids; §4 makes it order-free |
| S-4 | **Speculative contacts / conservative advancement:** a contact constraint lets two bodies approach up to touching within the step, never past it | Box2D and bepuphysics2 ([bepu CCD docs](https://github.com/bepu/bepuphysics2/blob/master/Documentation/ContinuousCollisionDetection.md)); [Unity speculative CCD](https://docs.unity3d.com/Manual/speculative-ccd.html); P. Firth, "Speculative contacts", 2011 | None if the allowance is split by responsibility | Zero to first order | **Chosen mechanism:** the allowance is the gap, shared by responsibility |
| S-5 | **Reciprocal responsibility** | van den Berg, Lin, Manocha, "Reciprocal Velocity Obstacles", ICRA 2008 ([pdf](https://www.cs.unc.edu/~geom/RVO/icra2008.pdf)); van den Berg, Guy, Lin, Manocha, "Reciprocal n-body Collision Avoidance" (ORCA), 2011 ([page](https://gamma.cs.unc.edu/ORCA/)): each agent takes half the responsibility; an agent that does not react is treated as an obstacle | None | Zero for velocities planned with the constraint | Gives E-2's split: ½ / ½ when neither walked towards the other; the walker takes all against a standing one |
| S-6 | **Position-based crowds** | Weiss, Litteneker, Jiang, Terzopoulos, "Position-Based Real-Time Simulation of Large Crowds", *Computers & Graphics* 78, 2019 ([pdf](https://web.cs.ucla.edu/~dt/papers/cag19/cag19.pdf)) | Mutual (everyone yields) | Low, by many iterations and long-range predictive constraints | Our solver's family. Does not separate "friend" from "enemy"; the even split is its rule |
| S-7 | **Order (Gauss–Seidel, unit by unit)** | StarCraft II and most RTS engines move units one after another (inferred; not documented) | None, if each mover is swept against the others' current positions | Zero | Order dependence breaks C-1a and the bit-identity across body order. Rejected |
| S-8 | **Push rights as data** | StarCraft II 5.0.14: `PushPriority` and `AlliedPushPriority` (allies only); `AlliancePushable` (patch 1.3.0, map scripting); patch 1.2.0 "units could push opposing units when they shouldn't"; collision by `Collide` flag sets (`liberty` UnitData) | None in melee | — | The facts E-2 implements; B-209 is the collision-flag half |

**Conclusions:**
1. Zero penetration and no pushing cannot both come from a share rule applied **after** bodies overlap: a wedge has
   no solution that leaves both enemies in place (§2.4). Every engine that achieves both does it by **not letting the
   mover in** (S-3, S-4).
2. Order independence comes from bounding all bodies at once against the positions at the start of each application,
   with the gap shared so that the two bodies' bounded closings never sum to more than the gap (S-4, S-5).
3. The responsibility must be **frozen for the tick** (S-2): E-2 reads it from the tick's committed steps, which do not
   change during the solve.

---

## 4. Design

### 4.1 K-1: the pair cache is keyed by each body's move count (B-208)

- `BodyStep` gains `moves: u32`, incremented whenever an application moves the body. It is reset with `steps` per
  solve.
- `Pair` records `cached_at: (u32, u32)`, the two counts when `cached` was computed.
- A cached correction is reused only when both counts are unchanged. `BodyStep::moved` keeps its other reader, the
  `held` skip.
- **Behaviour:** the summed passes read corrections at the positions they start from, as C-1b states. It changes
  outcomes (§2.2) and the goldens.
- **No switch.** It is a defect fix; the solver plan's text already describes the intended behaviour.

### 4.2 E-2: enemies block (B-193)

**Rule.** In the Solver pipeline, no step a body takes during the tick may close on an enemy by more than the body's
share of the gap between them. Enemies are never moved by an enemy's press; a body walking or pressed into an enemy is
stopped at its surface and keeps any part of its step that slides along it.

**Terms**, per tick, for an enemy pair `(a, b)` listed by the solver (same body class, neither a ghost, connected, the
C-2 rows for fixed and holding bodies unchanged):
- `s` tick-start position, `c` committed position (the proposal), `x` current position.
- `n₀ = (s_b − s_a) / |s_b − s_a|`, from `a` towards `b` at the tick start (coincident: `pair_normal`'s convention).
- **Responsibility, frozen for the tick:** `κ_a = max(0, (c_a − s_a)·n₀)`, `κ_b = max(0, −(c_b − s_b)·n₀)`;
  `w_a = κ_a / (κ_a + κ_b)`, `w_b = 1 − w_a`; when the sum is 0, `w_a = w_b = ½`. A partner that cannot move in the
  pair (fixed, or holding against a non-holding enemy: share 0 in C-2) gives the other the whole allowance:
  `w = 1` for the body that can move.
- **Allowance at an application**, from the positions at its start: `g = max(0, |x_b − x_a| − (r_a + r_b))`;
  `u = (x_b − x_a) / |x_b − x_a|`. Body `a` may close on `b` by at most `w_a · g` along `u`; `b` on `a` by `w_b · g`
  along `−u`.

**The bound of a step `v_a`** (in Q32.32):
1. **Projection (slide).** For each enemy pair of `a`: `e = max(0, v_a·u − w_a·g)`. `v′_a = v_a − Σ e·u`, every
   term computed from the unbounded `v_a` and summed by exact integer addition (order independent).
2. **Guard.** For each pair, if `v′_a·u > w_a·g`, the scale `w_a·g / (v′_a·u)`; `σ_a` is the minimum over pairs,
   clamped to [0, 1]. The bounded step is `σ_a · v′_a`. The guard catches the corners where one pair's projection
   adds closing on another (two enemies at an obtuse angle).
3. A pair is examined only when its bodies can reach each other in this application: `|x_b − x_a|² ≤
   (r_a + r_b + m_a + m_b)²` with `m = |v_x| + |v_y|` (no root). Only such pairs take one integer square root.

**Where the bound applies (every way a body moves in the solve):**
- **Application zero, the committed step.** The solver's bodies enter at their **tick-start** positions, and each
  body's step `c − s` is applied as the first (summed) application. A body whose step the bound leaves unchanged is
  placed at `c` exactly, without a terrain step (its proposal was admitted by locomotion), so a scene with no enemy
  in reach is bit-identical to today. A body whose step was bounded takes the terrain step (C-6) to its bounded target.
- **Every averaged application** (after the division by the contact count).
- **Both summed passes,** after O1's `bound_summed_steps` in the friends' pass. The fixed order per body is: O1 scale,
  then E-2's projection and guard.

**What stays from C-8.** Enemy pairs keep their even split for overlap that already exists: a pair overlapping at the
tick start (a spawn, an unload, a terrain residual, a bound's second-order rest) is separated ½ / ½, and the
correction away from an enemy is never bounded by that enemy. Under E-2 no new enemy overlap forms to first order, so
this rule acts on residuals only. D-SHOVE-E (§7.1) measures how much it moves anyone.

**What it is not:**
- No rule about orders or engagement: a chasing, kiting or walking enemy is not pushed either (the game's rule is
  about players, §3 S-8).
- No change to friend pairs, D-α, D-β, D-δ, the dead band, O1, acquisition or the ring rule.
- No change to the Legacy pipeline.
- No tunable value: the allowance is geometry, the responsibility is the committed steps.

**Switch.** `contact.enemies_block` in `movement_rules.ron`, absent = `true`, documented in the file's header; `false`
is `main`'s behaviour (with K-1). It exists for the bit-identity proof and for review, names no unit, and is not a
tuning knob.

**The pair list.** The list is built once per solve at the tick-start positions, with reach `r_a + r_b + PAIR_SLACK +
2·M`, where `M` is the largest committed step of the tick (an L∞ bound, no root); the Verlet staleness threshold
becomes `(PAIR_SLACK + 2·M) / 2` for the application zero and `PAIR_SLACK / 2` from the rebuild after it on. A pair
missed by the list could not have closed within the step. The implementer may instead build twice (at the tick start
for application zero, then as today); both give the same result, since unlisted pairs contribute nothing. The choice
is by measured cost (§9), proved by bit identity.

**Order independence (C-1a).** The responsibility reads `s` and `c`; the allowance reads the application's starting
positions; projections and scales are a sum and a minimum over pairs. The result is a function of the set of bodies.

**Expected effects** (from §2.5; WP2 measures them):
- Largest enemy overlap at the end of a tick ≈ 0 in every battle and scene family.
- D-SHOVE-E of every side at or below Legacy's.
- Friend overlaps rise where a crowd presses on an immovable enemy front (D-α bounds them; B6 checks).
- Battles 26 and 28 shift towards the ranged side: zerglings can no longer prise the enemies apart. Battle 28 stays
  a B-192 failure; battle 26 is B-209's (§7.3).

### 4.3 Alternatives rejected

| Alternative | Why not |
|---|---|
| E-1 as built | Discontinuous shares flip every iteration (§2.3); wedges (§2.4) |
| Capped closing (continuous E-1) | Still 5 % to 9 % in 26, 27, 29: wedges (§2.4) |
| Rewind to first contact after the last pass | Fails when the tick start is already wedged; 10 % in 29 (§2.4) |
| Enemies as static obstacles at their tick-start positions only | Two bodies walking into the same gap both clear the other's start disc and overlap each other; a second rule would be needed for them, which is E-2's shared allowance |
| Scaling the whole step instead of projecting | Zero overlap too, but a grazing body stops dead: blue's fire rate in 26 falls from 0.515 to 0.491 and the slide along an enemy, which the game has, is lost (§2.5) |
| Gauss–Seidel order | Breaks C-1a (§3 S-7) |
| Collision classes first (B-209) | Fixes the colossus only; immortals, stalkers and ultralisks collide with zerglings in 5.0.14 and are shoved today |

---

## 5. Packages and their order

| Package | Content | Merges | Behaviour | Schema |
|---|---|---|---|---|
| **P1** | WP0: diagnostics, baseline, close-battle list | on its own | none | — |
| **P2** | WP1 K-1 (B-208); WP2 E-2 (B-193); WP3 records, goldens | on its own, after P1 | Solver: summed passes; enemy contact | none |
| **P3** | WP4 churn diagnosis and cost-neutral acquisition; WP5 G-6′ (B-192), battle 28's marker, D-ε | after P2 is on `main` and measured | Solver: acquisition at a full ring | next free at merge |

**Why B-193 alone first:**
- E-2 is a correction to a high-confidence game fact; D-ε needs the owner (Q0110).
- The first attempt's G-6′ failures (B3 overlap, B7 firing, churn) were measured on a tree with E-1's flipping
  shares and the stale cache; they must be re-read on a sound base before G-6′ is judged.
- G-6′ fills rear rings, the press that wedges; under E-2 that press can no longer shove or wedge. Judging both in one
  package confounds them, as the first attempt found.
- P2 changes no hashed state, so it does not compete with weapon validation WP7 for a schema number.

---

## 6. Diagnostics and baseline (P1, WP0; no behaviour change)

**Port** the first attempt's WP0 (`32491bac`: `crates/sim_fixtures/src/trace/contact_checks.rs`, the matrix
columns, `harness/room.rs` and the mirror family's columns) onto `main`, with these repairs:

- **D-SHOVE-E (replaces D-SHOVE for B2).** For a unit that proposed no motion (the first attempt's definition) and is
  not holding, per tick: `max over enemies e touching it at the tick start of max(0, Δx · (s_u − s_e)/|s_u − s_e|)`,
  the outward radial part of its displacement relative to the enemies it touches. A D-δ nudge is tangential to the
  unit's own target ring, so it counts almost nothing; a shove counts fully. The first attempt's D-SHOVE stays as a
  reported column. **Test W0-T1′:** a standing unit displaced 0.1 straight away from a touching enemy counts 0.1; a
  unit nudged 0.1 along the tangent of its touching target counts at most 0.1 · (0.1 / (r + r_e)) (second order); a
  walking unit counts 0.
- **B7′, firing per engaged unit-second.** Shots per unit-second **while in band of its target** (the trace's
  `engaged` activity), beside the first attempt's shots per living unit-second. A smaller ring capacity (fewer
  attackers reach) lowers the latter without anything being broken; dithering, cancelled swings and units pushed
  out of band lower the former.
- **Enemy-overlap ticks.** Per battle: the count of unit-pair-ticks with an enemy overlap above T7's enemy bound
  (0.5 % of the radius sum) at the end of a tick, beside the maximum.
- **Friend overlap in the matrix.** The largest friend overlap at the end of a tick, and the count of pair-ticks above
  D-α's moving bound (`moving_overlap_permille`, 5 %).
- D-R10 and D-CHURN as on the first attempt's branch.

**Tests first** (red on `main` by compilation or value, green after): W0-T1′; the first attempt's W0-T2 (D-R10
columns), W0-T3 (churn); W0-T4 (CSV header and the old-header readers; new columns appended last); **W0-T5**
(B7′'s engaged-second count on a constructed two-unit trace).

**Baseline of record** (`~/scratch/enemy-contact/wp0/`, one release binary per arm, one run at a time under the gate
lock):
1. Matrix, Solver, the 24 seeds and the held-out seeds 21 to 44 (the tool's 21st to 44th non-record seeds).
2. Matrix, Legacy, the 24 seeds (all battles), for B2's bar.
3. Harness families, development seeds 1 to 1,000 and held-out 9,000,001 to 9,001,000.
4. Throughput: the three scenarios, `perf stat -e instructions:u`, three runs, both pipelines.
5. **The close-battle list:** the first attempt's list (its evidence §2.2) united with the same rule applied at this
   base, written into the evidence file before any WP1 run. A battle in either list is Class R.

**Acceptance:** no checksum, golden or harness figure moves except new columns; the list and the baseline are
committed to `docs/evidence/2026-11-25-enemy-contact-and-ring-retarget.md`.

---

## 7. Acceptance and stop rules (pre-registered)

### 7.1 Behaviour checks

Measured per battle over the 24 seeds against the **WP0 base** (and for B2 against Legacy), "beyond noise" being the
candidate's per-seed mean outside the base's 95 % bootstrap interval (2,000 resamples of the 24 seeds), on the worse
side. Harness checks use the solver plan's Amendment 8 step rule.

| Check | Broken when (after P2) | Change from the first attempt |
|---|---|---|
| **B1, stalled with room** | D-R10 `free` or blocked unit-seconds above the base beyond noise, in a battle without the B-192 marker | Split from B-192: P2 does not fix R10, so battle 28 is reported, not judged |
| **B2, shoving** | **D-SHOVE-E** of either side above Legacy's upper bound on the same seeds | D-SHOVE counted friendly D-δ nudges (first attempt §2.3); D-SHOVE-E counts only outward displacement from touching enemies |
| **B3, enemy overlap** | Largest enemy overlap above the base's (per battle, and per harness family), **or** enemy-overlap ticks above the base beyond noise | The task's bar: never above the base; the tick count catches a broad rise the maximum hides |
| **B4, retarget churn** | D-CHURN `unexplained` per seed above the base beyond noise | Judged against the base (first attempt §2.3: nonzero at the base by acquisition's nearest-first key) |
| **B5, give-ups** | Give-ups of automatic chases above the base beyond noise | Unchanged |
| **B6, friend overlap** | Harness: scenes with a friend pair above T7's friend bound above the base beyond noise. Matrix: friend pair-ticks above the moving bound above the base beyond noise in a battle where B3 did not improve | Matrix column added; a rise bought by removing enemy overlap is the expected trade (D-α), reported |
| **B7′, firing** | Shots per **engaged** unit-second of either side below the base beyond noise | First attempt's B7 per living unit-second confounded ring capacity with firing; it stays reported |

### 7.2 Battle classes (as the first attempt's §6.2)

- **Class R:** 26, 28, 33, 38a, 38b, and every battle in WP0's close-battle list. A Class R battle that leaves its
  range or flips is **restated** if every check of §7.1 passes for it: measured range over the 24 seeds, claim text
  rewritten where the winner changes, a fixture comment citing AGENTS.md §8, Q0103, this plan and the mechanism.
  A control pair (38a and 38b, 36a and 36b) is restated together.
- **Class D:** every other battle. A flip or a range left is a **stop for review** with traces and the §7.1 table.
  A claim sourced to StarCraft II that the new result contradicts goes to the owner as a question instead.
- **Any §7.1 failure** is a stop whatever the class: one amendment attempt within the package, then a register row and
  the package stops before merge.

### 7.3 Battles 26 and 28 (pre-registered)

- **Battle 28** keeps `known_bug: Some("B-192")` through P2 if it still fails a seed of record (the prototype: 4 of 4).
  If P2 makes it hold on every seed of record, the marker is removed in WP2's commit (`Battle::verdict` would
  otherwise give `StaleKnownBug`), and B-192's register row is rewritten to cite G-T1 and D-R10 `free`.
- **Battle 26.** Its claim describes zerglings packing round colossi they would walk under in 5.0.14 (§2.6). If it
  fails a seed of record under P2 **and** every §7.1 check passes for it, it takes `known_bug: Some("B-209")` in
  WP2's commit, with B-209 in the register in the same commit (`every_known_bug_marker_names_an_open_bug_of_the_register`),
  and its claim is **not** restated: the claim stays true of the game, and the deviation has a registered cause. If a
  check fails, the stop rule applies as for any battle.

### 7.4 Held-out check

Every restated claim is checked on the held-out matrix seeds 21 to 44: reported, never used to widen a range. A
restated claim that fails on more than a third of them was fitted too narrowly: stop and rewrite from both ranges in
an amendment. Harness: held-out range under Amendment 8's step rule.

### 7.5 Stop rules of P2 (in advance)

- **E2-T1 green on the base** (a pressing crowd does not move a standing enemy on `main`): the mechanism is not the
  one diagnosed. Stop before any code.
- **K1-T1 green on the base:** the cache defect is not as diagnosed. Stop.
- **Switch off not bit-identical** to the WP1 tree (every matrix and harness CSV, the per-tick digests of E2-T1's scene
  and battle 28's first seed of record): a defect. Stop and find it.
- **Enemy-free scene families** (gap, cross, corner) **not byte-identical** between WP1 and WP2: a defect (E-2 must not
  touch a scene without enemies). Stop.
- **Any §7.1 check fails,** a Class D battle moves, or a restated claim fails the held-out check: §7.2.
- **Cost:** §9's gates fail after the allowed cost step: stop, file Q0111 with the split.
- **Not allowed:** a constant, a per-unit or per-battle switch, a tolerance chosen after seeing figures, or a change to
  friend-pair rules to make a check pass.

---

## 8. Work packages

**Every package follows the charter's routine:** tests red first on the package's base; `cargo gate quick` and the
needed tests, then hand back; a verifier runs `cargo gate package` on the final tree (charter §4); builds in
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`; measurements one at a time under `flock
~/scratch/gate.lock`, outputs under `~/scratch/enemy-contact/`. **Base:** `main` at the start of each package.

### WP0 (P1): diagnostics and baseline
As §6. Files: `crates/sim_fixtures/src/trace/{contact_checks,matrix,mod}.rs`, `harness/{room,mirror,mod}.rs`,
`crates/sim_fixtures/tests/trace_tests.rs`; the evidence file. Size M. Gate: package.

### WP1 (P2): K-1, the pair cache by move count (B-208)

**Tests first:**
- **K1-T1** `solver::tests::the_enemy_pass_reads_corrections_at_its_own_positions`: three bodies; an enemy pair whose
  body moves in the last averaged iteration but has no hard friend pair; the enemy pass's correction equals
  `pair_correction` at the positions the pass starts from (red on `main`: it equals the earlier one).
- **K1-T2** (guard): `the_solution_does_not_depend_on_body_order` and `a_crowd_at_rest_within_the_dead_band_does_not_move`
  stay green.

**Code:** `movement/solver.rs` (`BodyStep::moves`, `Pair::cached_at`, `accumulate`); the module doc's active-set
rationale updated (the cache is valid while neither body has moved since it was computed).

**Measurement (attribution, not judged alone):** sub-matrix and full matrix on the 24 seeds; harness families. It is
recorded so that WP2's shifts can be attributed. A Class D battle that flips here is a stop for review.

### WP2 (P2): E-2, enemies block (B-193)

**Tests first** (red on WP1 unless marked guard; scratch archetypes, no catalog unit named):
- **E2-T1** `contact_solver_tests::a_pressing_crowd_does_not_move_a_standing_enemy`: the first attempt's E-T1 scene
  (a standing armed body attacked by a three-rank crowd of twice its ring capacity). Its D-SHOVE-E is 0 and its
  position equals its start over the rear rank's arrival time doubled. Mirrored and relabelled: the same.
- **E2-T2** `…::a_walker_stops_at_a_standing_enemy`: a body walking straight at a standing enemy ends touching it, the
  enemy unmoved, overlap 0 (within one raw unit of Q32.32 per application).
- **E2-T3** `…::a_grazing_walker_slides_along_an_enemy`: a walker whose path clips an enemy disc ends beyond it, its
  displacement's tangential part kept (the projection), the enemy unmoved.
- **E2-T4** `…::two_walkers_meet_at_contact`: two enemies walking head-on at equal speeds meet with overlap 0 and
  symmetric positions (bit-identical under mirror); at speeds 2:1 they cover the gap 2:1.
- **E2-T5** `…::a_body_pressed_by_friends_stops_at_an_enemy`: an idle body pushed by a moving friend into a standing
  enemy ends touching it; the friend overlap that remains is within D-α's moving bound.
- **E2-T6** `solver::tests::a_wedged_body_is_not_admitted`: two standing enemies with a gap narrower than a third body;
  that body, pressed towards the gap by friends over 50 ticks, never overlaps either enemy, and neither enemy moves.
  (The geometry of §2.4.)
- **E2-T7** `solver::tests::the_bound_does_not_depend_on_body_order`: the order-independence test with enemies,
  every body order of a random crowd giving the same positions.
- **E2-T8** `contact_solver_tests::the_block_switch_off_is_bit_identical`: with `enemies_block: false`, per-tick
  digests of E2-T1's scene and battle 28's first seed of record equal WP1's (FNV-1a, as the first attempt did).
- **E2-T9** `solver::tests::a_scene_without_enemies_is_unchanged`: a friend-only crowd gives bit-identical positions
  with the switch on and off (application zero places unbounded bodies at `c` exactly).
- **Guards:** `anchored_bodies_stay_and_enemies_separate` (a spawned overlap still separates), `enemy_pairs_take_no_dead_band`,
  `enemy_constraints_have_the_last_word`, `a_nudged_attacker_slides_along_its_ring_only`, `push_priority_tests.rs`
  "enemies never push", `terrain_is_never_entered_and_cliffs_separate`, `k_iterations_bring_injected_overlap_below_the_moving_bound`.

**Code:**
- `movement/solver.rs`: the responsibility weights in `add_pair` (stored on `Pair` for enemy pairs); the bound
  (`bound_enemy_steps`, applied by `apply` after averaging and after O1); application zero and the pair-list reach in
  `solve`; the module doc and `contact_shares`' table (C-8's row now "existing overlap only; steps bounded by E-2").
- `movement/collision.rs` (`solve_contacts`): bodies enter at `start` with their committed position carried
  (`SolverBody` gains `committed: SimVec2`); `SolverSettings` gains `enemies_block`.
- `catalog/movement_rules.rs`, `assets/catalog/movement_rules.ron`: the switch, validated, documented.
- Every new comparison in Q32.32; one `integer_sqrt` per pair within reach; no float, no `HashMap`.

**Measurement:** harness (development, held-out; switch on and off), sub-matrix and full matrix (24 seeds, held-out),
Legacy on the 24 seeds (unchanged), throughput. Each judged by §7.

### WP3 (P2): records and the single golden re-record

- `crates/sim_core/tests/fixtures/matchup_matrix.ron`: Class R restatements (§7.2); battle 26 per §7.3; battle 28 per
  §7.3.
- `golden_replay.ron` and the other pinned digests: re-recorded **once, on this final tree**, with a proof header on
  four builds (native debug and release; `wasm32-wasip1` debug and release), as the solver plan's §7 requires. No
  schema change: the proof is that the checksum format is unchanged (no field added) and the switch-off build
  reproduces WP1's digests.
- `DESIGN.md` §4: B-193 fixed (WP2's commit), B-208 fixed (WP1's), **B-209** registered (open); the Movement row (E-2
  replaces C-8's even split for new contact); §5 the throughput table.
- Q0052: no addendum (E-2 is the game's rule, not a deviation).
- Evidence: every figure, trace and decision.
- **Gate:** `cargo gate package` on WP3's tree; the orchestrator's `cargo gate merge`, then `cargo gate full` on
  `main` before the push.

### WP4 (P3): re-reading G-6′ on P2's tree, and acquisition without a cost to Legacy

**Entry:** P2 on `main`; G-T1 (`ring_retarget_tests`, from `dc3b62ab`) red on that `main`; D-R10 `free` above 0 in
battle 28. If G-T1 is green, B-192 was the press: rewrite its row and stop P3.

**Diagnosis first (no behaviour change):** port G-6′ behind its switch (off) and measure it on, on the sub-matrix:
- which of the first attempt's G-6′ failures (B1, B3, B4, B7, battles 33, 38a, 38b) remain on the E-2 base;
- for every unexplained return of D-CHURN, the trace of the two rings at the return tick (a full ring that opened and
  closed within the 25-tick window, or acquisition's nearest-first key outranking the retarget memory).

**Cost-neutral acquisition.** The first attempt's shared admission test cost Legacy 1.4 % to 3.5 %. P3 does not
touch `scan_candidates`' code path: the retarget's candidate scan is its own function that calls the same admission
predicates (the per-candidate tests are already separate functions on `main`, **verify**), proved equal by the
first attempt's property test G-T9 (oracle: `scan_candidates`). Acceptance: Legacy instructions equal WP3's within
noise (C-1).

**Stop:** if the diagnosis shows B4 or B7 failing for a reason inside G-6′'s rule (not the base), WP5 does not start;
an amendment proposes the change, before code.

### WP5 (P3): G-6′ (B-192), battle 28, D-ε

As the first attempt's WP2 (`solver-followup-wp2-wip`, `dc3b62ab`), on WP4's port: `ring.rs` (`ring_retarget`,
`RingAim::Unruled`), `locomotion.rs` (`prepare_pursuit`), `mod.rs` (`MotionProposal::ring_retarget`), `entity.rs`
(`MovementMemory::ring_retarget`, hashed), `checksum.rs` (schema next free at merge, proved by an unhashed scratch
build), `movement_rules.*` (`retarget_from_full_ring`, absent = on), the tests G-T1 to G-T9, battle 28's marker removed
in the commit that makes it hold, goldens re-recorded once on WP5's final tree, `DESIGN.md` (B-192 fixed, D-ε
"pending Q0110"), Q0110 filed with the restated battles. The checks of §7.1 apply with B1 judged in every battle,
against WP3's tree as the base.

---

## 9. Throughput budget

**Owner's bars:** Q0093 (c), every scenario at least 50 ticks per second; Q0102 (a), +66.8 % over Legacy on 64 × 64
accepted. First attempt's base: Legacy 10.205 G, Solver 16.681 G (+63.5 %) on 64 × 64; 128 × 128 +7.3 %; canyon
+55.2 % (re-measured in WP0).

**Gates** (three runs per scenario; noise = their spread):
- **C-0:** ≥ 50 ticks per second on every scenario after every WP. A hard stop.
- **C-1:** Legacy instructions on every scenario equal the WP0 base's within noise after P2 (P2 never runs in Legacy),
  and equal WP3's after P3.
- **C-2:** Solver instructions on every scenario at most **+3 %** over the WP0 base after P2, and at most +3 % over
  WP3's after P3.

**Expected cost of P2** (estimated, not measured: the prototype is float and allocates): K-1, one recomputation of the
enemy and fixed pairs in the last pass, under 0.3 %; E-2, one scan of the enemy pairs per application (23 per tick)
with a root only for pairs within reach, plus application zero, about 1 % to 2 % on 64 × 64; together within the
+3 % gate. **Allowed cost step (once, behaviour-preserving, proved by bit identity):** an index of the enemy pairs
built with the list, so the bound scans only them; or skipping the bound for bodies with a zero step.

**Reporting:** the ratio to the same tree's Legacy and to the WP0 base's Legacy in `DESIGN.md` §5. Q0111 is filed if
64 × 64 rises above +70 % over Legacy or canyon above +60 %.

---

## 10. Determinism, golden replay and checksum

- Every new input is a tick-start, committed or application-start position and the static map. Every comparison is in
  Q32.32; sums are exact integer additions; minima are order-free.
- **P2 hashes nothing new:** the weights and move counts live only inside one solve. The checksum format and schema are
  unchanged; positions move, so the golden replay and pinned digests are re-recorded once (WP3).
- **P3** hashes `MovementMemory::ring_retarget` (schema next free at its merge; weapon validation WP7 may take 47
  first).
- The CI determinism jobs (Linux, Windows, macOS ARM64, `wasm32-wasip1`, debug and release) check both.

---

## 11. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| Friend overlaps rise behind an immovable enemy front | B6, T7 friend bound | Measured (B6); D-α's moving bound; the projection keeps the slide; a stop if beyond noise where enemy overlap did not improve |
| Application zero changes scenes without enemies | Byte-identity of gap, cross, corner | Unbounded bodies placed at `c` exactly (E2-T9); stop rule |
| Terrain slide after a bounded step closes on an enemy | Small residual overlap | Measured by B3's tick count; residuals separated by C-8's even split |
| A walker reaching a standing enemy now stops at contact one tick later in rare geometries (the allowance is the gap at the application's start) | Melee arrival time | E2-T2; B7′ |
| Battles 26 and 28 move towards the ranged side | Class R handling; 26 by §7.3 | Pre-registered; B-209 registered |
| The cost estimate is wrong | C-2 | One allowed cost step; Q0111 |

---

## 12. Out of scope (decisions, recorded so they can be brought back)

- **B-209** (colossus collision classes): registered, not fixed. A follow-up package: a `collides_with` body-class set
  in the catalog, read by the pair filter in both pipelines.
- **D-δ's large friend overlaps among engaged melee units** (25 % to 46 % in §2.4): B-191's territory; E-2 stops them
  from wedging into enemies, not from overlapping each other.
- **A movement-side remedy for B-192** (being carried round the crowd's edge): the first attempt's §3.1 R-b stays
  rejected.

---

## 13. Owner questions (drafts; not filed)

### 13.1 Q0110 (draft, filed by the orchestrator at WP5 with its figures)

The first attempt's draft (its §12.1), unchanged in its question and options, with two edits to its background:
the shove fix is now E-2 ("a unit walking or pressed into an enemy is stopped at its surface; enemies are never
moved"), and its restated-battle table and cost are WP5's. Default (a): D-ε on, as merged.

### 13.2 Q0111 (draft, conditional on §9's reporting threshold)

The first attempt's draft (its §12.2), with the cost figures of whichever package crosses the threshold. Default (a):
accept if every scenario stays at or above 50 ticks per second.

No other question: E-2 is the game's rule (no deviation), K-1 and B-209 are defects.

---

## 14. File ownership

| File | WP |
|---|---|
| `crates/sim_fixtures/src/trace/*.rs`, `crates/sim_fixtures/src/harness/{room,mirror,mod}.rs`, `crates/sim_fixtures/tests/trace_tests.rs` | WP0 |
| `crates/sim_core/src/movement/solver.rs` | WP1, WP2 |
| `crates/sim_core/src/movement/collision.rs`, `crates/sim_core/src/catalog/movement_rules.rs`, `assets/catalog/movement_rules.ron` (`enemies_block`), `crates/sim_core/tests/contact_solver_tests.rs` | WP2 |
| `crates/sim_core/tests/fixtures/{matchup_matrix,golden_replay}.ron` and pinned digests; `DESIGN.md` §4, §5, Movement row | WP3 (P2), WP5 (P3) |
| `crates/sim_core/src/movement/{ring,locomotion,mod}.rs`, `target_acquisition.rs`, `entity.rs`, `checksum.rs`, `checksum/encoding.rs`, `movement_rules.*` (`retarget_from_full_ring`), `crates/sim_core/tests/ring_retarget_tests.rs` | WP4, WP5 |
| `docs/evidence/2026-11-25-enemy-contact-and-ring-retarget.md` | every WP |

Weapon validation WP7 (pursuit to the last-seen point) owns `locomotion.rs`'s chase of a lost target and may take
schema 47; P2 touches neither. P3 rebases on it. `DESIGN.md` §2 is unchanged; no crate gains a dependency.

---

## Amendment 1 (implementer, 2026-11-24): P1 and P2 as built

Decisions taken while implementing WP0 to WP2, each recorded here so that it can be reviewed or brought back. None
chose a constant, tolerance or switch after seeing a matrix or harness figure.

1. **Base.** `main` moved to `802df056` (weapon validation WP7, schema 47) during WP2. The branch merged it, and every
   arm of the evidence (WP0, WP1, WP2, switch off) was rebuilt on the merged base: WP0's and WP1's commits each merged
   with `802df056` in scratch trees, WP2 the branch head. The pre-merge WP0 runs (`matrix/wp0-*`) are kept but not used.
   The close-battle list is the same on both bases.
2. **The overlap counters** (§6) count bodies only: living units that do not pass through units (hidden, mineral
   walking, collision suppressed) and whose archetype is not immobile by default (structures meet units through their
   footprints). B7′'s engaged unit-ticks are the ticks ending with the unit's attack target in its band (the solver's
   "engaged"; the trace's `Engaged` class, any enemy in band, is wider).
3. **Placed bodies (E-2).** A body whose position this tick was not set by its own committed walk is not bounded in
   application zero: the movement phase records no proposed step for it (a cast in the movement phase, such as Blink's
   teleport, sets `pos_at_tick_start` to the origin and the position to the destination). Such a body enters at its
   committed position (`SolverBody::walk_from = committed`). Bounding a teleport as a walk would cut it short at the first
   enemy on the straight line from its origin. Test `a_placed_body_is_not_bounded`.
4. **Terrain slides (E-2).** §11's risk "terrain slide after a bounded step closes on an enemy" appeared in E2-T2: a
   walker sliding along an enemy into the corner of a wall had its bounded step slid back along the wall into the enemy,
   and C-8's even split then moved the enemy by 0.0006. A terrain step whose result differs from its bounded target and
   closes on an enemy of the application beyond its allowance is refused, the body staying (`closes_too_far`); in
   application zero it stays where its walk began.
5. **Projection rounding.** The projection removes `PROJECTION_SLACK` (8 raw units of Q32.32) beyond each excess, the
   bound of its own arithmetic's rounding (truncated unit vector, scaled step, dot product). Without it a projected step
   could still close by a raw unit or two, and the guard would then scale a body touching an enemy to zero, stopping a
   slide dead.
6. **Pair list.** Built twice (the plan's alternative): an enemy-only list at the walks' starts with reach `2M` for
   application zero (skipped with one owner or no walk), then the contact list as before. The index of enemy pairs (§9's
   first allowed cost step) is kept from the start, behaviour-identical by construction; the allowed cost step was not
   otherwise used.
7. **Tests.** K1-T1's first scene let a Verlet rebuild empty the cache between the passes and passed on the base; the
   scene now keeps every correction below half the pair slack, and is red on the base (−0.1000 reused, −0.0437 fresh).
   E2-T3's walker starts touching the post at 30° (on open ground steering side-steps a clipping walker before contact,
   so the scene did not test the projection). E2-T4's 2:1 split is pinned at the solver
   (`two_walkers_share_the_gap_by_their_walks`); in the world, steering's side rule slows the slower walker first, so the
   distances covered are not 2:1 (1.92 and 0.75), while both scenes meet at contact.
8. **E2-T5 fails as specified and is `#[ignore]`d.** The pressed friend stops at the enemy's surface (overlap 0, the post
   unmoved), but its walking friend keeps walking into it: C-2 gives a moving body no share against an idle standing
   friend, and that friend can no longer yield, so their overlap grows to about 90 % until the walker is thrown back, every
   25 ticks (bodies as wide as a two-cell corridor; in a wider corridor the walker squeezes past at about 68 %). The
   plan's expectation (friend overlap within D-α's moving bound) needs a friend-pair rule that a body blocked by an enemy
   counts as unable to move, which §7.5 forbids changing within P2. Reported for review with the matrix's B6 figures.

## Amendment 2 (planning agent, 2026-11-25, at the orchestrator's direction): the friend rule F-1, the repaired B2 reading, C-1's codegen control, and the resumed WP2's acceptance

**Why.** P2 stopped at WP2 under §7.5 (evidence `docs/evidence/2026-11-25-enemy-contact-and-ring-retarget.md` §4 and
§5; Amendment 1 item 8). E-2 does what it was designed to do against enemies: the largest enemy overlap is 0.00 %
everywhere, the pressed side's D-SHOVE-E falls to 0 for colossi, immortals and stalkers, no Class D battle flips, and
the Solver costs +1.3 % to +2.0 %. Three findings stopped it:
1. **E2-T5 fails, and B6 rises, for one mechanism.** E-2 holds a friend at an enemy's surface, but C-2 still gives a
   walker share 0 against that idle standing friend, so the walker keeps walking into it (to about 90 %). The same
   mechanism raises friend pair-ticks above 5 % in battles 26 to 29, and battle 27's largest friend overlap goes from
   37.0 % to 59.9 %. This is a defect of the plan: §4.2 left the friend rules unchanged and did not foresee that a
   body E-2 holds can no longer take its friend share.
2. **Battle 27 fails B2 and B4.** B2: the ultralisks' D-SHOVE-E is 0.146 per seed against Legacy's upper bound
   0.099, and 0.059 with D-δ off. B4: unexplained returns are 12.62 per seed against the base's upper bound 12.42,
   and 12.33 with D-δ off.
3. **C-1, strictly, fails.** Legacy retires 0.05 % to 0.08 % more instructions than the base, above the three runs'
   0.015 % spread, while its matrix is byte-identical to the base's.

**This amendment is P2's one amendment attempt** (§7.2: "one amendment attempt within the package"). A §7.1 failure
of the resumed WP2 stops the package for review. No further rule is added.

### A2.1 F-1: a body E-2 holds against an enemy passes the part of its friend share it cannot take to the friend pressing it

**Source.** This is shock propagation (Guendelman, Bridson, Fedkiw, "Nonconvex Rigid Bodies with Stacking", SIGGRAPH
2003): a body that cannot move counts as immovable for the bodies pressing it. The solver plan proposed it for
terrain as its P3 (Amendment 11, step 4, R5: "the partner with share 0 takes share 1"), but it was not built.
F-1 is its continuous form, limited to enemy walls. It is the rule the orchestrator named, "as bounded": the held
friend is not made fixed (a binary state would flip between applications as E-1's shares did, §2.3). It is bounded
exactly as E-2 bounds it, and the part E-2 would remove goes to its partner.

**Walls.** At the start of each application (averaged, and the friends' summed pass), every listed enemy pair `(x, e)`
gives body `x` a wall: the unit vector `u_xe` from `x` towards `e`, and `x`'s allowance `allow_xe = w_x · g`. These
are exactly E-2's terms (§4.2: `w` frozen from the tick's committed walks, `g` the gap at the application's start).
A pair is a wall only if it is in the contact list (gap ≤ `PAIR_SLACK`). A body with no listed enemy has no wall.

**Rule.** For a friend pair `(a, b)` with corrections `δ_a` and `δ_b`, after C-2's shares, D-δ's `along_ring` and
the dead band:

```text
β_a = Σ over a's walls of  max(0, δ_a · u_ae − allow_ae) · u_ae      (the part E-2 would project off)
δ_a′ = δ_a − β_a
δ_b′ = δ_b − (β_a · n) · n                                           (n: the pair's unit normal from b to a)
```

The rule for `b` is symmetric. Each term is computed from the pair's own correction and the walls at the
application's start, and summed exactly. The part of `a`'s yield that points into an enemy beyond its allowance is
removed. The same separation along the line of centres is given to `b` instead, in the opposite sense. The
tangential part, the slide along the enemy, stays with `a`. In `add_to`, a transferred correction counts towards
`b`'s contact count, as any correction does.

**Properties:**
- **Continuous.** `β` is a positive part of a function that is continuous in positions, so no state can flip
  (contrast §2.3).
- **Order independent (C-1a).** Each pair's terms read only the pair and the walls at the application's start, and
  the sums are exact.
- **No enemy moves.** `b`'s extra correction is bounded by E-2's step bound against `b`'s own enemies, like any
  correction. If both bodies are walled, the pair keeps its overlap, a legitimate jam within D-α.
- **Bit-identical without enemies.** With no wall, `β = 0` and every friend correction is today's. Scenes without
  enemies (the gap, cross, corner and corridor families, every friend-only test) are unchanged byte for byte. The
  same holds for any friend pair in a battle with no body that has a listed enemy.
- **Under the existing switch.** F-1 is part of `enemies_block`: with the switch off, WP1's behaviour is unchanged
  bit for bit. No new switch.
- **Cache.** A friend pair's cached correction is reused only when neither body has a wall. A pair with a walled body
  is recomputed in every application, because its walls move with the enemies. The move-count key of K-1 is
  unchanged for every other pair.
- **The enemy pass** has no friend pairs; application zero has none either. F-1 acts in the averaged applications
  and in the friends' summed pass (`HardFriends`), which is where E2-T5's walker meets the idle friend (the pair
  "moving against idle standing", one share 0).

**Its limit, stated in advance.** F-1 relieves the friend in direct contact with a walled body. In a chain (walker →
friend → friend → enemy), the middle friend has no wall. It takes the transfer from the walled friend and the push
from the walker, and settles between them, while the walker still takes nothing. The chain compresses as it does on
the base when a crowd presses a friend against terrain. This is the solver plan's R5, unchanged here. The general
rule (P3 for terrain and for chains) changes friend rules where no enemy is present, which this package may not do.
If B6's maximum check (A2.4) fails on a chain, the package stops and the chain rule goes to a later package with its
own plan.

**Tests (red first on the WP2 tree unless marked guard):**
- **F-T1 = E2-T5, un-ignored, as specified in §8.** An idle friend pushed by a moving friend into a standing enemy ends
  touching the enemy, which does not move. Their friend overlap stays within D-α's moving bound (5 % of the radius sum)
  on every tick, in the two-cell corridor and in an open field.
- **F-T2** `solver::tests::a_held_friend_keeps_its_slide`: the idle friend touches the enemy at 45° to the walker's
  line. It slides along the enemy (its tangential displacement equals the unbounded pair's tangential part within one
  raw unit); the walker takes the normal part; the enemy does not move.
- **F-T3** `solver::tests::a_nudged_attacker_between_a_presser_and_an_enemy`: an engaged melee unit under D-δ, its ring
  tangent running into a neighbouring enemy, is pressed by a friend. It never closes on that enemy beyond its
  allowance. The presser takes the blocked tangential part. The radial part removed by D-δ stays removed: no new
  radial correction reaches the presser.
- **F-T4** (guard, extended) `the_bound_does_not_depend_on_body_order` with walled friend pairs: every body order of a
  random mixed crowd gives the same positions.
- **F-T5** (guard) `a_scene_without_enemies_is_unchanged` (E2-T9): a random friend-only crowd is bit-identical with
  `enemies_block` on and off.
- **F-T6** `solver::tests::a_two_friend_chain_against_an_enemy_holds_the_enemy`: walker → friend → friend → standing
  enemy. It asserts only the enemy invariants (the enemy unmoved; no enemy overlap). The friend overlaps are recorded
  in the evidence, as the stated limit.
- **Guards:** every test listed green in the evidence §4.1, including `a_pressed_column_keeps_its_order_and_settles`
  and `an_overlapped_idle_pair_separates_monotonically` (friend-only, so bit-identical).

**Expected effect on B6** (stated before measuring F-1):
- Friend pair-ticks above 5 % fall in battles 26 to 29 towards the base, wherever the compressed pair is a walker
  against a front-rank friend held by an enemy. This is E2-T5's case, and D-δ's front rank whose tangential nudge E-2
  bounds.
- They do not fall where the compression is in a chain behind the front rank.
- Battle 27's largest friend overlap falls from 59.9 %. Whether it reaches the base's 37.0 % is not predicted; A2.4's
  maximum check decides.

**Cost (estimate).** The walls need the gap of each listed enemy pair at each application. That is one root per listed
enemy pair, shared with E-2's bound, which computes the same gap for the pairs within its reach: compute the walls
once per application and let the bound read them (behaviour-identical, proved by bit identity). The friend pairs with
a walled body lose their cache. Estimate: +0.3 % to +1.0 % on 64 × 64 over WP2's +1.99 %, so close to C-2's +3 %.

**Code.** `movement/solver.rs` only: walls per application (`Self::walls`, retained), the transfer in `accumulate`
for friend pairs, the cache rule, and the docs (module doc, C-2's table note: "F-1: a friend held by an enemy passes
on what it cannot take"). No catalog field. No hashed state, so still no schema change.

### A2.2 B2 and B4: what is repaired, what stays

**B2: definition repaired, after seeing figures; both readings judged.**
- **Why it is a definition defect, not a loosening.** §6 stated the intent before any figure: "A D-δ nudge is
  tangential to the unit's own target ring, so it counts almost nothing; a shove counts fully." Its test W0-T1′ checked
  only a nudge along the ring of the touching enemy itself. The definition takes the largest outward part over **all**
  touching enemies. A nudge along an ultralisk's own target ring is radial to another zergling touching it, so it is
  counted in full. The D-δ-off arm shows the residue is the nudge: 0.146 with D-δ, 0.059 without, against Legacy's
  [0.073, 0.099].
- **This repair is made after seeing battle 27's figures,** and is said so here and in the evidence. To keep it from
  becoming a way to pass, B2 is judged by **two readings, both of which must pass**, and the original is kept:
  1. **B2-a (no confound): the original D-SHOVE-E, on the D-δ-off arm** (`nudge_engaged_along_ring: false`, a
     diagnostic build of the same tree). Without D-δ no friend nudges an engaged unit, so the original definition
     measures exactly what B2 was written to measure: displacement outward from touching enemies of a unit that
     proposed no motion. Every side of every battle must be at or below Legacy's upper bound on the same seeds.
  2. **B2-b (shipped behaviour): D-SHOVE-E′ on the shipped arm.**
     - Same as D-SHOVE-E, except that for a unit eligible for D-δ at the tick's start (the solver's
       `ContactParty::nudge` is `Some`: an engaged melee unit in band of its target, not holding), the component of
       its displacement along its own target ring's tangent is removed before the outward parts are taken.
     - D-δ can move such a unit only along that tangent, so the repair removes every D-δ contribution and nothing an
       enemy could cause radially.
     - Its blind spot is an enemy push exactly along the unit's own ring tangent. B2-a covers it, since that reading
       has no D-δ to confound it.
     - The rule: every side of every battle at or below Legacy's upper bound.
  3. **Reported, not judged:** the original D-SHOVE-E on the shipped arm (the column of §6) and the first attempt's
     D-SHOVE.
- **Code (sim_fixtures only):** `trace::contact_checks::shove_e_prime_of`, taking the unit's target centre and its
  D-δ eligibility, appended last as `blue_shove_e_prime` and `red_shove_e_prime`. The eligibility is read the way the
  solver reads it (`solver_body`'s `nudge`), through a `pub` predicate if one exists. Otherwise it is a test-pinned
  replica: **W0-T6** constructs an engaged melee unit and a ranged one and checks the replica against the solver
  party. **W0-T7**: a nudge along the own ring that is radial to a second touching enemy counts 0 in D-SHOVE-E′ and in
  full in D-SHOVE-E; a push radial to the own target counts in both.
- **Held-out.** B2-a and B2-b are run on the held-out seeds 21 to 44 as well (shipped arm and D-δ-off arm), against
  Legacy on the same held-out seeds (a Legacy held-out run is added). A failure there is a §7.1 failure.

**B4: stays as pre-registered; no repair.**
- **Reasons.** Nothing shows a defect in D-CHURN's definition. A return the acquisition key makes because a nudge moved
  a unit is a real return, and D-δ is shipped behaviour, so the D-δ-off figure (12.33) does not excuse the shipped one.
  The excess is small (12.62 against 12.42), but "small" is not a rule this plan registered. Introducing a
  multiplicity correction now, after seeing a near miss, would be the loosening the orchestrator forbids.
- **What changes is the tree.** F-1 moves positions in battle 27, so B4 is judged afresh on the resumed WP2's tree, as
  written: the per-seed mean of unexplained returns against the base's 95 % interval.
- **Added, reported only:** B4 on the held-out seeds against the base's held-out interval. If B4 fails on the 24 seeds,
  the evidence classifies battle 27's unexplained returns, at the return tick, into nearest-first flips after a D-δ
  nudge, after an E-2 bound, or other. That is for the review; it does not change the stop.

### A2.3 C-1: behaviour identity plus a codegen control

**Reasons.** C-1 exists to catch cost that leaks into Legacy from shared code. The first attempt's shared admission test
made Legacy 1.4 % to 3.5 % slower. A Legacy that runs no line of E-2 can still retire a few more instructions when the
crate's code layout and inlining change. That is a property of the binary, not of anything the package does. The
three-run spread (0.015 %) measures run-to-run noise, not that.

**C-1′ (replaces C-1 for P2), all three parts required:**
1. **Behaviour.** The Legacy matrix on the 24 seeds and the Legacy harness families (development seeds) are
   byte-identical to the base's.
2. **Codegen band, measured before the resumed WP2's Legacy figure is read.**
   - Two control builds change only code Legacy never executes:
     - **(C-a)** WP1's tree (K-1 changes only `ContactSolver`, which Legacy never calls);
     - **(C-b)** the base tree with only `ContactRules`' `enemies_block` field and its parsing added (the catalog
       shape change E-2 makes, read by nothing in Legacy).
   - Each is run three times per scenario, like the arms.
   - The band is the largest absolute Legacy instruction delta of the two controls against the base, per scenario.
3. **The rule.** The resumed WP2's Legacy delta per scenario lies within the band. If it does not, `perf record -e
   instructions:u` of Legacy on the base and on WP2 attributes the excess by symbol:
   - C-1′ passes if the excess lies only in functions whose source is unchanged (layout and inlining), recorded in
     the evidence;
   - otherwise it fails (a stop, §7.5).
   - No numeric allowance is chosen.

### A2.4 The resumed WP2: acceptance and stop rules (pre-registered again)

**Tree.** The WP2 branch head plus F-1 (A2.1) and the sim_fixtures column of A2.2, with `main` merged if it has moved.
Every arm is rebuilt on that tree:
- **WP0m** (the base);
- **WP1m**;
- **WP2′** (shipped);
- **WP2′-off** (`enemies_block: false`);
- **WP2′-nonudge** (`nudge_engaged_along_ring: false`, for B2-a);
- Legacy;
- the two C-1′ controls.

**Runs** (one at a time under the gate lock):
- the matrix on the 24 seeds and on the held-out seeds 21 to 44, for WP2′, WP2′-nonudge and Legacy;
- the harness on the development seeds and on the held-out range 9,000,001 to 9,001,000, which the stopped WP2 did not
  run, for WP2′, WP2′-off and WP1m;
- throughput, three runs per arm.

**Tests.** E2-T1 to E2-T9, F-T1 to F-T6, W0-T6, W0-T7, K1-T1, `two_walkers_share_the_gap_by_their_walks`,
`a_placed_body_is_not_bounded`, and every guard of §8 and the evidence §4.1, all green. No `#[ignore]`.

**Checks** (§7.1 as written, with these readings):

| Check | Judged |
|---|---|
| B1 | As §7.1 (battle 28 reported, not judged) |
| B2 | **B2-a and B2-b** (A2.2), every side of every battle, on the 24 seeds and the held-out seeds; the original D-SHOVE-E reported |
| B3 | As §7.1: largest enemy overlap not above the base's, overlap ticks not above the base beyond noise, every battle and harness family |
| B4 | As §7.1, unchanged (A2.2) |
| B5 | As §7.1 |
| B6 | As §7.1 (pair-ticks above 5 % beyond noise reported where B3 improved; harness scenes above T7's friend bound), **plus B6-max: in every battle, the largest friend overlap at the end of a tick is not above the base's largest**. This is new and strictly tighter: F-1 exists to remove the friend-overlap rise. A battle that fails it stops the package, with a trace saying whether the pair is a direct contact (F-1's case, a defect) or a chain (A2.1's stated limit) |
| B7′ | As §7.1; B7 reported |

**Battles** (§7.2 and §7.3 unchanged):
- Battle 26 takes `known_bug: Some("B-209")` in the commit that makes the matrix test hold, if it fails a seed of record
  and every check passes for it.
- Battle 28 keeps its B-192 marker while it fails a seed of record.
- No Class D battle may flip or leave its range.
- Every restated Class R claim is checked on the held-out seeds (§7.4).

**Throughput:**
- C-0: at least 50 ticks per second on every scenario.
- C-1′ as in A2.3.
- C-2: Solver at most +3 % over the base on every scenario, F-1 included. The one allowed cost step of §9 is the shared
  wall computation of A2.1, proved by bit identity, if C-2 fails without it.
- Q0111 is filed if 64 × 64 rises above +70 % over Legacy (WP2: +68.8 %) or canyon above +60 % (WP2: +59.1 %).

**Stop rules (in advance; no further amendment attempt within P2):**
- F-T1 or F-T2 green on the WP2 tree (before F-1): the mechanism is not as diagnosed. Stop before code.
- The switch off is not bit-identical to WP1 (matrix CSVs, harness CSVs, E2-T8's digests). Stop.
- The gap, cross, corner or corridor family is not byte-identical to the base. Stop: F-1 must not touch a scene
  without enemies.
- Any judged check fails in any battle (B1, B2-a, B2-b, B3, B4, B5, B6, B6-max, B7′), on the 24 seeds or, for B2, the
  held-out seeds. Stop for review, with the traces and the A2.2 classification.
- A Class D battle moves, or a restated claim fails §7.4. Stop for review.
- C-0, C-1′ or C-2 fails after the allowed cost step. Stop and report the split.
- **Not allowed:** another friend rule (the chain rule included), a constant or tolerance chosen after seeing figures,
  a per-unit or per-battle switch, or a further change to any check's definition.

**If it passes:** WP3 proceeds as §8 (records, battle 26's marker, the single golden re-record on WP3's final tree,
`DESIGN.md`, evidence). B-193 and B-208 are marked fixed and B-209 is registered, as planned.
