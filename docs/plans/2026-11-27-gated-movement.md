# Blueprint: The `Gated` Movement Pipeline (aim well, never step into what won't move, then separate)

This is the **implementation plan** of the movement redesign. The design round is
[`2026-11-26-movement-blank-sheet`](2026-11-26-movement-blank-sheet.md) (cited below as **the design**, with its section
numbers). The owner approved it in Q0120 to Q0124. This plan turns the design's recommended architecture (candidate 7:
G′ goal choice, I′ gated intent, C′ contact rules) into work packages. It builds the new pipeline beside today's as a
third value of `contact.pipeline`, `Gated`, and switches over only once the acceptance written here, before any
`Gated` code exists, has passed.

**Status.** Written on 2026-11-27 (session date 2026-11-25) on branch `gated-movement-plan`, cut from `main` at
`6bd0fc4b` (checksum schema 49; `Solver` is the shipped pipeline; Q0117 and Q0120 to Q0124 closed).
- **Line numbers drift,** so search for the quoted symbol. Claims not confirmed while writing are marked **verify**.
- **Later decisions** are appended as `## Amendment N (role, YYYY-MM-DD): …`.
- **Numbers are provisional, next free at merge.** Bug rows start at **B-224**, after B-219 and B-220 (the
  friend-crowd branch) and B-221 to B-223 (the follow-order branch). Questions start at **Q0125**. The schema follows
  the follow-order plan's 50. Every number is a fenced draft in §13 and is not filed by this plan's commit.
- **No production code here.** The only code this plan names is the code its packages write.

**Why a full plan (charter §6).** It adds a mechanic and replaces a subsystem's default. It moves every battle
outcome and golden replay, adds hashed fields (checksum format), and spans five packages and two crates.

**Inputs read.**
- AGENTS.md (§3.1 determinism and the throughput floor, §6, §8), `docs/questions/README.md`, and the owner's
  pathfinding guidance of 2026-11-20: no fitting to the test set, simpler deviations allowed if recorded, survey
  first.
- The design, all of it: RC-1 to RC-6 (§1), R-1 to R-18 (§2), the suite (§3), the candidates (§4), the prototype
  figures (§5), the recommendation (§6), migration, acceptance, cost and close battles (§7), and risks (§10).
- The owner's answers in `docs/questions/closed/`: Q0120 (a), Q0121 (a), Q0122 (c), Q0123 (a), Q0124 (a), Q0117 (b),
  Q0113 (b), and Q0118 (a) with its strategic guidance. Also Q0052 and its addenda (D-α to D-δ, S-snapshot-cap),
  Q0093 (c), Q0102 (a), and Q0103's guidance.
- The contact-solver plan [`2026-11-20-contact-solver`](2026-11-20-contact-solver.md): C-1 to C-8, I-1 to I-4, G-1 to
  G-7, the harness §6 (T1 to T9), Amendment 8 (the step rule), and Amendment 17.3 (G-6′). Its evidence §13 is also
  used.
- Friend-crowd: plan `2026-11-26-friend-crowd-overlap` and evidence §2 to §7 on branch `friend-crowd-plan`
  (`dd53aa2f`). WP0 and WP1 are built (C-3′ latent). WP2's switch-over was **measured and stopped** (evidence §5).
- Enemy-contact: plan `2026-11-25-enemy-contact-and-ring-retarget` on `enemy-contact-plan` (`7f686d23`). Used: E-2
  §4.2, the tests E2-T1 to E2-T9, §7 acceptance, Amendment 2 (F-1, the repaired B2) and A2.4.
- Solver-followup: plan `2026-11-24-solver-retarget-and-shoving` on `solver-followup` (`6a96cd15`), its As built, and
  `solver-followup-wp2-wip` (`dc3b62ab`). Used: G-6′, the tests G-T1 to G-T9, and why the package stopped.
- Collision classes: plan [`2026-11-25-collision-classes`](2026-11-25-collision-classes.md) §7.2 (B1 to B9, B8 and B9
  probes) and the parked `collision-classes-wp3-wip` (`42761cf2`, B-209).
- Follow order: plan `2026-11-26-follow-order` on `follow-order-plan`. It touches `locomotion.rs`, `mod.rs`,
  `entity.rs` and `movement_rules.ron`, and takes schema 50.
- Code at `6bd0fc4b`: `crates/sim_core/src/movement/` (`solver.rs`, `intent.rs`, `ring.rs`, `concave.rs`,
  `locomotion.rs`, `collision.rs`, `mod.rs`), `catalog/movement_rules.rs`, `entity.rs` (`MovementMemory`),
  `crates/sim_fixtures/src/harness/` and `trace/contact_checks.rs`, `sim_fixtures::matchup` (`Battle::verdict`), the
  matrix fixture, and `DESIGN.md` §5.

**Questions and standing rules.**

| Question | State | Where |
|---|---|---|
| Q0120 (a): restructure as G′ + I′ + C′ beside today's pipeline; switch over only once the pre-registered acceptance passes, including the prototype's direction churn and residual enemy overlap | Followed | §2, §6, §7 |
| Q0121 (a): D-δ retired under the new pipeline; fighting and Hold Position units are never pushed or nudged | Followed: C′-1, hard check H-1 | §3.1, §6.2 |
| Q0122 (c): measure D-β against moving-allies-block in the first package that has the gate, then decide | Followed: the measurement in WP2, its decision rule pre-registered | §8 |
| Q0123 (a): D-ε, retarget to the nearest enemy with room within scan range, kept until reached | Followed: G′-3, WP3b | §3.3 |
| Q0124 (a): budget = `Solver` + C-3′, and ≥ 50 ticks/s on every benchmark | Followed: C-3′ switched on (cap 151) as the reference arm | §9 |
| Q0118 (a), closed: C-3′ merges latent, and its switch-over is decided inside this redesign | Followed: C-3′ is part of `Gated`'s contact layer; `Solver` never ships with it on unless the redesign stops (§6.6, Q0126 draft) | §3.1, §6.6 |
| Q0117 (b), Q0113 (b): colossus walk-under (B-209) and enemies-block stay parked until this lands | E-2 is ported into C′ (WP1); B-209 is re-measured after WP4 (records only) | §3.1, §10 |
| Q0051 / AGENTS.md §8: unit mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing: deviations recorded in Q0052 (§11) | §11 |
| Q0093 (c), Q0102 (a): every throughput scenario ≥ 50 ticks/s; overlap tolerances are not traded for instruction ratio | Standing | §9 |
| Q0103's guidance: natural movement outweighs a close matchup's winner; a broken behaviour is a bug | Standing | §6.5 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing | §7 |
| Q0002: scoping is the agent's call if written down | Followed | §12 |
| `docs/questions/open/` at `6bd0fc4b` | Q0118 (being closed by the orchestrator, per its message), Q0119 (follow order, unrelated) | — |

---

## 1. Summary

1. **What is built.** `MovementPipeline::Gated`, a third pipeline beside `Legacy` and `Solver`, reached only through
   `contact.pipeline: Gated` until the switch-over. It is `Solver` plus three layers, each in its own package:
   - **C′ (WP1), contact.** Attacking and holding units are never moved by anyone (D-δ off). Enemies block (E-2,
     ported). A body that terrain holds passes its share to its partner (B-220). C-3′ is on, capped at 151.
   - **I′ (WP2), gated intent.** A unit never proposes a step into a body that will not give way. Its direction is
     chosen among the bins that make progress, with a kept side.
   - **G′ (WP3a, WP3b), goal choice.** An orbit approach to a far slot, and D-ε, retarget from a full ring (G-6′
     ported).
2. **Seven packages:** WP0a (nine new scenarios and the permutation probe), WP0b (the base of record), WP1, WP2,
   WP3a, WP3b, and WP4 (switch-over). **WP5, deleting `Legacy` and `Solver`'s superseded rules, is a separate
   decision** taken after WP4 has proved itself. Until then both pipelines stay as measurement references (§7, WP5).
3. **Q0122 (c) is measured in WP2,** the first package in which a unit's own step is gated. The two arms differ
   only in `moving_friends_push`. A lexicographic rule, registered now, decides between them, or files Q0125 with
   the figures if the arms tie or split (§8).
4. **Acceptance (§6)** judges properties, not winners:
   - **Hard checks:** no attacking or holding unit moved by another body (H-1), enemies never push (H-2), order
     independence (H-3), four-build determinism (H-4), terrain (H-5).
   - **Figures:** the harness T1 to T9 under the step rule, the nine new scenes' known answers, B1 to B9 with every
     repair, F-1 to F-4 friend overlap, and the friend-crowd switch-over's failures, which `Gated` must beat (§6.4).
   - **Stops:** **T1 (direction churn) is WP2's first stop.** One amendment attempt per package. **Two WP2 stops end
     the redesign** (§6.6).
5. **Battles.** Battle 28 must hold its claim under `Gated` (WP3b), and its B-192 marker goes in the WP4 commit that
   makes `Gated` the shipped pipeline. Battle 26 follows §6.5's pre-registered rule.
6. **Determinism.** Q32.32 only. Every new read comes from the tick-start snapshot, every sum is exact
   (order-independent), and the new permutation scene rotates the processing order inside the tick. Goldens are proved
   on four builds. Two hashed fields: `MovementMemory::gate_side` (WP2) and `MovementMemory::ring_retarget` (WP3b),
   each taking the next free schema at its merge (51 and 52 provisional).
7. **Cost (§9).** On every throughput scenario, `Gated` ≤ `Solver` with C-3′ switched on (cap 151) in instructions,
   and ≥ 50 ticks/s. While the packages are latent, `Solver` and `Legacy` cost what WP0b's base costs, within noise.

---

## 2. What the plan inherits, and what it changes from the design

### 2.1 Kept from the design as written

- RC-1 to RC-6 (design §1) are the diagnosis. R-1 to R-18 (§2) are the requirements. §3 is the suite.
- The architecture of design §6.1. The global layer (navmesh, TA*, radius funnel, lanes) is unchanged.
- The rules: C′ as design §6.4, I′-1 to I′-6 as §6.3, and G′-1 to G′-3 as §6.2, with the simplifications of §2.2.
- Migration (design §7.1, §7.2), deviations (§7.3), bugs (§7.4), acceptance (§7.5), cost (§7.6), close battles
  (§7.7).

### 2.2 Changed, each a simplification (the owner's guidance: simpler deviations allowed if recorded; R-18)

| # | Design | This plan | Why |
|---|---|---|---|
| S-1 | I′-3 scores a bin by `½(1 + cos) · min(free, step) / step`, and keeps the goal direction when its free distance is at least half the speed (one constant) | Score = **progress along the preferred direction**, `cos θ_k · min(free_k, step)`. No "half the speed" constant | Progress toward the goal is the Lyapunov function the design names: the best bin is the one that lowers it most. This drops a constant, and the goal direction wins whenever it is free for at least `cos 22.5° ≈ 0.924` of a step. |
| S-2 | I′-4 scores bins on the other side ×0.6 (one constant) | The **kept side is lexicographic**: the goal bin and kept-side bins are tried first, and the other side only if none of them reaches the stand line. The side is released when the preferred direction is free for a whole step | It drops a constant and still gives the hysteresis that SC-2 and ST-2 describe. A weighted side can flip when two scores cross. |
| S-3 | I′-3: "7 bins within ±67.5°" and "the cosine is at least 0.38" (two figures) | **Every bin whose cosine is positive** among `contact.gated.bins` equal bins. With 16 bins that is the same seven | One constant, `bins`, derived in §4.2; the rest follows from it |
| S-4 | G′-2's orbit radius is "the occupied band plus one diameter". It dives "when the angle is small" (a threshold) | Orbit radius = `ρ + max r_o + r`, so it clears the largest body on the ring (geometry). Dive when the **ring arc between the unit's bearing and its slot holds no blocked interval** (G-2's intervals, already computed) | Parameter-free |
| S-5 | G′-2's fallback is "the step 2′ wavefront (built, `75871a42`), once" | Fallback is **today's straight chase to the gap**. The wavefront is WP3a's pre-registered amendment candidate, used only if the orbit's terrain refusals show in the corner or corridor families | It imports no unmerged code until a measurement asks for it |
| S-6 | G′-1 "claims kept until reached" (the prototype re-claimed every tick, which churned) | G′-1 is **today's kept gap** (contact-solver Amendment 7 item 2: the gap is kept while it stays free; it is not re-sorted). Claims visible to other units (G-3, G-4) are WP3a's pre-registered amendment candidate, used only if T5 or the surround-moving fill fails | Today's ring rule already keeps its gap, and the base meets T5. The churn the prototype measured came from its own re-sort, which `main` does not do (design §5.4). |
| S-7 | C′'s E-2 alone. F-1 (enemy-contact Amendment 2) not named | E-2 alone. Extending the terrain transfer to bodies that E-2 holds (F-1 restated as one rule) is WP1's pre-registered amendment candidate | Design §6.4: the gate removes what F-1 was for. A measurement, not an assumption, must ask for it |
| S-8 | WP3 is one package | WP3a (orbit) and WP3b (D-ε, the G-6′ port: 1,742 changed lines on `dc3b62ab`) | Each must fit one agent package, and each is judged alone |
| S-9 | WP0 is one package | WP0a (the scenes and the probe, code) and WP0b (the base of record, measurement) | The same |
| S-10 | Held-out: 9,000,001 to 9,001,000 (harness) and the matrix's 21st to 44th non-record seeds | **Fresh ranges:** harness 9,100,001 to 9,101,000; matrix the tool's 45th to 68th non-record seeds | The earlier ranges have been run and read by the solver, friend-crowd and enemy-contact packages; this plan has never seen the fresh ones |

None of these adds a rule. S-1 to S-4 drop four constants, and S-5 to S-7 defer three mechanisms until a pre-registered
measurement asks for them.

---

## 3. The design, restated for implementation

Units, as everywhere in the movement code: world units, Q32.32 `SimScalar`; per-tick speeds; `SimAngle` binary angles
(65,536 per turn). Every read is from the movement snapshot taken at the tick start (`prepare_movement_snapshot`) or
from the body's own proposal. The tick order is the contact-solver plan's §3.1: snapshot → walkers in `EntityId`
order → contact solver.

### 3.1 C′, contact (WP1)

`contact_shares` gains the pipeline as an argument (`ShareRules { moving_friends_push, gated: bool }` replaces the
`bool`), so one table answers for all three pipelines. Under `Gated`:

| Row | `Solver` today (C-2 with D-δ) | `Gated` (C′) |
|---|---|---|
| a body fixed, holding or unpushable | 0 / 1 | unchanged |
| **C′-1:** a friend attacking (its target in its band) against a friend that is not attacking | 0 / 1, with D-δ's tangential nudge (`ContactParty::nudge`) | **0 / 1, never nudged**: `nudge` is not set under `Gated` (`nudge_radial` is not called) whatever `nudge_engaged_along_ring` says (R-5, Q0121) |
| two friends attacking | ½ / ½ | unchanged |
| **C′-2:** enemies | ½ / ½ (C-8; B-193) | **E-2** (§3.1.1) |
| moving against an idle standing friend | idle takes all, by priority | unchanged |
| two moving friends | D-β on: by priority; off: approach shares | unchanged, under the arm Q0122 selects (§8) |
| otherwise | ½ / ½ | unchanged |

**3.1.1 E-2, enemies block.** As the enemy-contact plan §4.2 specifies it on `7f686d23`, ported without change of
rule. The pieces:
- the responsibility weights frozen per tick from the committed steps;
- the allowance `w · g`;
- the projection and the guard;
- **application zero**: bodies enter the solve at their tick-start positions, each committed step is the first summed
  application, and an unbounded body is placed at its committed position exactly;
- the bound on every averaged application and on both summed passes;
- existing overlap still split ½ / ½;
- the pair list's reach `r_a + r_b + PAIR_SLACK + 2M`.

K-1 (B-208) is already on `main`, so it is not ported. E-2 has **no switch under `Gated`**: it is the owner's rule
there, not an option. `enemies_block` is not added to the data. The bit-identity proofs compare `Gated` with `Solver`
on enemy-free scenes instead (C1-T1).

**3.1.2 C′-3, the terrain transfer (B-220).** C-6 says a body that terrain holds "acts as a fixed body after one
iteration". The code never passes the share on (friend-crowd §10, solver plan Amendment 11 R5). Rule: in iteration
`k + 1`, a body whose own correction terrain refused in iteration `k` counts as fixed for every friend pair in which
its share is positive. The partner's share becomes 1, through C-2's "one fixed" row.
- **Refused** means the terrain step returned a target other than the averaged one and the body moved less than half
  its averaged correction. This is the solver's existing terrain-step result, read as a flag `held_by_terrain` on
  `SolverBody`, written by `apply` and read by `accumulate` in the next iteration.
- The flag is cleared at the start of each tick.
- It is continuous in the iteration (no per-tick decision), and order-independent because each body reads its own
  flag from the previous iteration.
- Both bodies held: C-2's "both fixed" row, so no correction, as a wedge against two walls should be.

**3.1.3 C-3′ under `Gated`.** Friend-crowd's crowd extension (`ContactSolver::solve` past K while the friends'
summed excess over D-α's moving bound is positive and still falling) runs under `Gated` with the cap
`contact.gated.crowd_iterations` (151, P-1's derived value on the shipped catalog). Under `Solver` it stays at
`contact.crowd_iterations` (21 = off).

**Why C-3′ is part of `Gated`.** It is the design's residual solver (§6.1, R-8). With the gate it rarely extends,
because the gate removes the injection it was paid to undo (design §5.3 item 3: 1.0 to 1.5× today's pair evaluations
against 3.9× for C-3′ under today's intent). It costs nothing on a tick whose crowd is within the bound (F-4 of
friend-crowd). Leaving it out would bring back B-219 wherever the gate leaves a residual (the wedges, design §5.3
item 4).

**Whether `Solver` ever ships with C-3′ on.** No, under this plan. `Gated` replaces that need. The only way `Solver`
+ C-3′ becomes the shipped movement is the fallback of §6.6 (WP2 stops twice). Even then it goes to the owner (Q0126
draft) with friend-crowd §5's failed checks, because those failures stand.

### 3.2 I′, gated intent (WP2)

A new module `crates/sim_core/src/movement/gate.rs`. It is pure functions of plain values, like `intent.rs` and
`solver.rs`, so every rule is tested without a world. `step_toward` calls it under `Gated` only.

**I′-1, who does not give way.** `fn gives_way(unit: &GateBody, other: &SteeringObstacle, rules: ShareRules) -> bool`.
A snapshot body `j` gives way to unit `i` when the share C′ would give `j` against `i` is 1. That is, when the
contact solver would move `j` fully out of `i`'s way. Concretely, `j` **does not** give way if any of these holds:
- it is an enemy (C′-2), or holding, immobile or unpushable;
- it is attacking: its attack target is in its band in the snapshot (the solver's `engaged`; C′-1);
- it is a friend standing under an order (step below `STANDING_DRIFT`, not idle: the queued body of Q0089; C-2
  "otherwise ½ / ½");
- it is a moving friend that is not co-moving (`co_moving`, I-4's 60° test) and either
  - under `moving_friends_push: true` (D-β), its priority is at least `i`'s, or
  - under `false` (moving allies block), whatever its priority.

An idle standing friend of no higher priority (L6, `yields_to`) gives way, and so does a co-moving friend.
`SteeringObstacle` gains the two snapshot facts this needs and does not yet carry: `engaged: bool` and
`queued: bool` (**verify** which of holding, immobile and priority it already has; `obstacle_of` fills them).

**I′-2, the gate.** `fn free_distance(origin, radius, dir: SimVec2, blockers: &[SteeringObstacle]) -> SimScalar`.
- For each non-yielding `j` within `r + r_j + step` of the origin, it takes the smallest `t ≥ 0` with
  `|o + t·d − x_j| = r + r_j`.
- If `|o − x_j| < r + r_j` already (residual overlap) and `d · (x_j − o) > 0`, the free distance is 0. Moving away
  is unbounded by `j`.
- One `SimScalar::sqrt` per body that the ray meets. The discriminant is tested first in squared form, so a miss
  costs no root.
- The result is the minimum over bodies, capped at `step`.

**I′-3, the direction.** The preferred direction `d₀` is I-3's heading (`intent_heading`). That is the direction to
the goal (route point, gap or orbit point) bent by the side rule against the earliest oncoming threat. So I-3 keeps
the job R-10 gives it (I′-5).
- Bins are `d_k = rotate(d₀, k · 65,536 / bins)` for every `k` with `cos(k · 360° / bins) > 0`. With 16 bins:
  `k = −3 … 3`, ±67.5°.
- Rotations use the exact constants `cos`, `sin` of `k · 22.5°`, obtained from `SimAngle::cos`/`sin` (exists in
  `sim_types::math`). No float.
- Score `s_k = c_k · min(free_k, step)`, with `c_k` the cosine (S-1). The chosen bin has the largest score.
- Ties go to the smaller `|k|`, then the kept side (I′-4), then the right (clockwise, I-3b's hand), so no id decides.

**I′-4, the kept side** (S-2). The memory field `MovementMemory::gate_side: i8` is `−1` (right), `0` (none) or `+1`
(left). It is hashed.
- If it is non-zero, the candidates are first `k = 0` and the bins on that side. The other side is tried only if no
  candidate reaches the stand line (I′-6).
- When the chosen bin is off-centre, `gate_side` becomes its side.
- It returns to 0 when `free_0 ≥ step`, on `restart()`, and on a new order.

**I′-5, oncoming movers.** I-3 unchanged (its one-pair `SideMemory`, I-3b's tie, its horizon). The gate clips I-3's
heading like any other preferred direction.

**I′-6, stand.** If the best score is below `step · stand_permille / 1000` (`contact.gated.stand_permille`, 125, the
harness's "moving" line v/8, §4.2), the unit proposes no motion and `Idle`.
- A chase keeps its order and accrues no stall: today's `queued` (Q0089).
- A plain `Move` accrues stall ticks as today, so a blocked `Move` is still given up after 10 s
  (`a_blocked_move_is_still_given_up` holds).

**I′-7, the gate is the last word on the step.** Several stages can turn a step:
- the snap onto a goal within one step (`gap <= speed`);
- `keep_terrain_view`;
- the wall slide (`slide_along_wall`);
- the hull gate (`gate_step_by_facing`).

Each final step is clipped once more along its own direction by I′-2: one ray per non-yielding body, the same
function. The snap is taken only if `free(d) ≥ gap`. The design names no later stage, so this is a defect guard,
not a rule: no stage after the gate may re-inject.

**What I′ removes under `Gated`:** nothing of `Solver`'s code. Under `Solver` the intent path is unchanged.

### 3.3 G′, goal choice (WP3a, WP3b)

**G′-1, the kept gap** (S-6). Today's ring rule (`ring.rs`, `RingAim`, `kept`) unchanged. A guard test pins that a
free kept gap is never re-chosen (G3-T1).

**G′-2, the orbit approach** (WP3a; S-4, S-5). A new module, `movement/orbit.rs`. When `ring_aim` returns
`RingAim::Gap(angle, point)` under `Gated`:
- **Orbit or dive.** Let `β` be the unit's bearing angle from the target's tick-start position `T`, and `σ` the gap's
  angle. The way round is the one `choose_slot` took: the shorter one, ties clockwise. If the ring arc from `β` to
  `σ` that way intersects no blocked interval (`concave::is_free` over the arc), the unit **dives**: its goal is
  `point`, as today. Otherwise it **orbits**.
- **The orbit.** Radius `ρ_o = ρ + r_max + r`. Here `ρ` is the ring's radius (the band's outer edge, as `ring_aim`
  uses it), `r_max` the largest radius among the bodies whose intervals were counted, and `r` the unit's. A disc on
  the orbit clears every body on the ring.
  - The goal is the orbit point at `β + s · Δ`: `s` the way round, `Δ = step · UNITS_PER_RADIAN / ρ_o` in binary
    units (one step's arc), capped at the remaining angle to `σ`.
  - It is computed with `slot_point(T, ρ_o, angle)`.
- **No local minimum.** The goal lies on a circle and advances monotonically toward `σ`, so the angular distance to
  the slot does not grow from the unit's own step (the design's Bug-2 argument, N-10). The gate (I′) may make the
  unit stand, but never turn back.
- **Terrain.** If the orbit point is not on ground connected to the unit (the ring walk's terrain test,
  `RingWalk::reaches`, and `on_connected_ground`), the unit falls back to today's straight chase to `point` (S-5).
  The probe counter `orbit_refusals` counts it (unhashed, WP0's column).
- **No new state.** The orbit is recomputed each tick from the snapshot, the kept gap and the bearing.

**G′-3, D-ε, retarget from a full ring** (WP3b; Q0123 (a)). G-6′ of the contact-solver plan, Amendment 17.3, ported
from `dc3b62ab` under `Gated` only:
- `SimWorld::ring_retarget` and `RingAim::Unruled` in `ring.rs`;
- the per-candidate test shared with `scan_candidates` in `target_acquisition.rs`;
- `prepare_pursuit`'s stand branch as a helper;
- `MotionProposal::ring_retarget`;
- `MovementMemory::ring_retarget: Option<EntityId>` (hashed).

Acquisition keeps the chosen target unless a candidate **outranks** it. The order follows Q0123's "nearest":

1. The candidates are those of acquisition's own admission test.
2. They are taken in acquisition's rank classes (threat, attack-target priority, primary weapon).
3. Within a class, the nearest comes first.

So a zergling never leaves a threat for a structure, and among equal ranks (every battle of 26 to 29) the nearest with
room wins. G-6′ already orders this way (its key ends in distance and the entity id, **verify**).

**Kept until reached:** the retarget lapses only when the unit reaches its band, the target dies or becomes invalid,
the order changes, or the new ring itself has no slot (then G′-3 runs again from there). The original target is never
handed back merely because it gained room. That is G-6′'s "kept unless outranked" together with Q0123's wording.

**Cost lesson from G-6′:** sharing the admission test cost `Legacy` 3.5 % and `Solver` 4.0 % on 64 × 64
(`solver-followup` As built). Here the shared function must cost `Solver` and `Legacy` nothing beyond noise (§9 C-2).
If it does, the duplicate form is allowed only with G-T9's equivalence property test proving the two admit the same
candidates.

### 3.4 What `Gated` reads from the data

`assets/catalog/movement_rules.ron`, `contact` section. `ContactRules` gains `gated: Option<GatedRules>`, required
when `pipeline: Gated` (a typed `CatalogInvalidValue` naming `gated` otherwise) and validated whenever present:

```ron
contact: (
    pipeline: Solver,              // WP1 adds the value Gated; WP4 ships Gated
    iterations: 21,
    dead_band_permille: 10,
    moving_overlap_permille: 50,
    side_horizon_factor: 2,
    moving_friends_push: true,     // D-β; under Gated set by Q0122's decision (§8)
    nudge_engaged_along_ring: true,// D-δ: Solver only; never read under Gated (Q0121)
    crowd_iterations: 21,          // C-3′ under Solver: off (friend-crowd, latent)
    crowd_depth: 12,
    gated: (
        crowd_iterations: 151,     // WP1: C-3′ under Gated (P-1's derived cap)
        bins: 16,                  // WP2: I′-3, the direction bins (§4.2)
        stand_permille: 125,       // WP2: I′-6, the stand line (§4.2)
    ),
),
```

Validation (typed errors, `movement_rules.rs` unit tests):
- `iterations ≤ gated.crowd_iterations ≤ 1000`;
- `bins` a power of two in 8 ..= 64, so that `65,536 / bins` is an exact binary angle and at least one off-centre bin
  makes progress;
- `0 < stand_permille < 1000`.

The header comment of the file documents each field with its derivation (§4.2), as the C-3 and C-3′ fields are.
`deny_unknown_fields` stays, so WP2's fields arrive with WP2's commit.

### 3.5 Rule count (R-18)

| Pipeline | Rules added | Rules removed or not applied | Constants |
|---|---|---|---|
| `Gated` against `Solver` | I′-1 to I′-4 (the gate, the bins, the kept side, who gives way), I′-6's stand line, G′-2 (the orbit), G′-3 (D-ε), C′-3 (the terrain transfer) | D-δ's tangential nudge, the enemy half-share for new overlap (E-2), and the standing-body half of the side memory (I-3 is kept for movers only, **verify** in WP2 that `side_partner` is never written for a standing partner under `Gated`) | `bins` (16) and `stand_permille` (125), both derived; C-3′'s cap is P-1's existing derivation. The design had five constants for the same rules (§2.2). |

---

## 4. Derivations

### 4.1 The cap of C-3′ under `Gated`

`gated.crowd_iterations = 151` is P-1's derived value (friend-crowd evidence §4: a channel crowd 12 deep and 12 wide
of the smallest ground radius, every walker stepped by ρ = 0.56). The friend-crowd test
`the_catalogs_crowd_cap_covers_the_catalogs_crowd` (P-5) is extended to read the `gated` value and fail if it is below
P-1's derivation (C1-T10). Nothing is re-derived for the gate. Its effect is to make the cap rarely reached, which
WP2 measures (`solver_extended_ticks`).

### 4.2 `bins` and `stand_permille`

- **`bins = 16`.** The gate must be able to turn a step at least as finely as the side rule turns one. I-3a caps the
  side rule's turn at `asin(1/h) = 30°` (`h = 2`). The bin width must therefore be at most 30°, and for exact binary
  angles it must be `65,536 / 2ⁿ`. The coarsest such width is 22.5° (`n = 4`): 45° is too coarse and 11.25° doubles
  the rays for no requirement. The test `gated_bins_are_the_coarsest_below_the_side_rule_turn` (I2-T13) recomputes it
  from `side_horizon_factor` and fails if the data differs. With 16 bins the progress bins are the seven within
  ±67.5° (S-3).
- **`stand_permille = 125`.** The harness counts a tick as moving when the step exceeds `v/8` (M1, §6.3 of the solver
  plan). A step below that line is what the metrics call standing, so the gate must not produce one that still moves
  the unit. Such a creep is M3's standing waste: `v/8` over 250 ticks is far above T2's 0.5 world units for every
  speed of the corpus. The test `gated_stand_line_is_the_harness_moving_line` ties the two constants
  (`sim_fixtures`'s `MOVING_STEP_FRACTION` and the data), so neither changes alone.

No constant is chosen from a measurement (the design §7.5 item 5, Amendment 14's bar).

---

## 5. The scenario suite (WP0a builds what is new)

### 5.1 Reused, unchanged

The design §3.1:
- the harness families gap (mixed-priority quarter), arc, cross, corridor, corner, mirror;
- the matrix (61 battles; 26 to 29, 33, 38a/b named);
- the throughput benchmark (64 × 64, 128 × 128, canyon, casters);
- the golden replay on four builds.

### 5.2 New families (WP0a)

Generated like the harness families (SplitMix64 per `(family, seed)`, the 1/64 lattice, scratch archetypes, radius and
speed from the corpus sets). Each family's known answer is stated here, before any `Gated` code, as the design's rule
for adding to the suite requires (§3.2).

| Family | Scene (exactly as the design §3.2) | Known answer, as a meter | Judged as |
|---|---|---|---|
| `hold-line` | 4 to 10 friends on Hold Position in a line across an open 48 × 48 field; 6 to 30 friends of mixed priority (a quarter with priority 20) given one `Move` to a point beyond it | `held_moved` = 0 (no holder displaced by more than 2⁻²⁰ in any tick); every mover completes by the gap family's bound with the line's length added to the distance; give-ups 0 | H-1 (hard); T4 |
| `enemy-press` | 4 to 8 enemies in a line, standing in three sub-families (idle, attacking a dummy behind them, holding); 20 to 60 attackers attack-move into them from one side | no enemy displaced except by its own step (`enemy_moved_by_others` = 0); enemy overlap ≤ 0.5 %; friend overlap within T7 | H-2 (hard); T7 |
| `deep-queue` | A corridor 3 to 6 cells wide plugged by an immobile enemy pillar; 20 to 80 melee attackers, 6 to 12 ranks deep | the front fills (corridor's C-e fill); friend overlap within T7 at every rank depth (reported per rank); M1 ≤ 1.0; M3 ≤ 0.5 | T1, T2, T5-like fill, T7 |
| `wedge` | A walker whose straight route passes between two unpushable bodies (holding friends in half the seeds, standing enemies in the other half) with a gap 0.6 to 0.95 of its diameter, an open way round | the walker never passes the gap; never overlaps either body beyond D-α (friend) or 0.5 % (enemy); M1 = 0; completes round | T4, T7, T1 |
| `hallway` | Two single files of 1 to 6 units meeting head-on in a corridor two diameters (of the larger radius) wide | both files complete; M1 ≤ 1.0; give-ups 0; each pair passes on one side (side changes per unit counted) | T1, T4 |
| `stutter` | 1 to 6 ranged attackers (range 4 to 6) firing at a dummy, friends packed behind them; a `Move` back for 3 to 8 ticks, then `Attack`, repeated 4 times | the first step lands on the tick after the order (lag 1); displacement along the order ≥ 0.9 · speed · ticks after the first tick of acceleration (**verify** the acceleration model's first-tick step and state the bound with it before WP1); fire resumes within one weapon period of being in range | R-11 (hard per scene) |
| `surround-moving` | 10 to 30 melee attackers chase a target walking a circle of radius 6 to 12 at 0.5 to 0.9 of their speed | fill reaches `min(k, C)` within 250 ticks of first contact (C as the arc family's capacity at the band edge); M1 ≤ 1.0 | T5-like (fill), T1 |
| `lanes` | A group `Move` of 12 to 48 through a field with two parallel 3-cell gaps | both gaps used (each passes at least a quarter of the group); arrival as a clump (final convex hull area within 2× the packed disc area, **verify** the bound on `main` before WP1); no order completed by A-1's forbidden witness | R-12 |
| `permutation` (a mode, not a family) | Every scene of every family above and of the six old ones, played twice: as generated, and with the **processing order rotated** inside every tick by a seeded offset (`movement_order_rotation`) | bit-identical positions in every tick. Differences are classified as exact-tie breaks (named in the code: coincident centres, I-3's equal roots) or **unexplained**. Unexplained = 0 under `Gated` | H-3 (hard) |

**The permutation probe (sim_core, WP0a).** `SimWorld::set_movement_order_rotation(&mut self, offset: u32)` stores
the offset in `MovementWorkspace`, which is unhashed (not serialised, not in the checksum, reset by nothing). It is
documented as a determinism probe for R-13. Each tick it rotates:
- the order in which `advance_checked_movement` visits its `MotionEntry` list;
- the order in which bodies are pushed into the `ContactSolver`.

Offset 0 is today's order. No production path sets it.
- **Why a world method and not a test-only hook:** the harness, a separate crate, must drive it, and `sim_core` has
  no features (**verify**; adding a feature for this would split the test build from the shipped one).
- **Why the relabelling of T8 is not enough:** it permutes entity ids, which change tie-breaks. The rotation keeps
  the ids and changes only the order, so any difference is order dependence.

**New meter columns (WP0a, `sim_fixtures`):**
- `ContactMeter` gains `held_moved` (per side: unit-ticks and the largest displacement of a unit that was holding,
  or whose attack target was in its band, at the tick start and that proposed no step) and M1 / M1-hi per 1,000
  moving unit-ticks with the harness's definition, so battles 26 to 29 are judged on T1.
- Two cost probes are read from the world (unhashed, `MovementWorkspace` counters as C-3′'s are): `gate_rays`
  (ray–disc roots taken) and `orbit_refusals`. Both read 0 on any tree without `Gated`.
- The two 400-unit throughput clashes (64 × 64 `pulse_bot` and a zergling analogue) are added as a `clash` mode of
  `matchup_trace`, played with the meter (300 ticks, one seed of record plus 4 seeds), so F-1 and F-2 can be read on
  them.

---

## 6. Acceptance and stop rules (pre-registered; binding on every package)

### 6.1 Arms, base, seeds, noise

- **Arms.** All are from one binary per tree, selected by a scratch catalog directory as the harness and matrix tools
  already allow:
  - **L**: `Legacy`;
  - **S**: `Solver` as shipped (C-3′ off);
  - **R**: `Solver` with `crowd_iterations: 151` (C-3′ on: the Q0124 reference);
  - **G**: `Gated` at the package's tree;
  - **G-β+** and **G-β−**: `Gated` with `moving_friends_push` true and false (WP2 on).
- **Base of record.** WP0b's tree (`main` after friend-crowd's latent merge and follow-order, plus WP0a), arms L, S
  and R on the whole suite. Every later package is judged against **S** and **R** of this base. The base is
  re-measured only if `main` changes simulation code under the plan; the step is then recorded in an amendment.
- **Seeds.**
  - Harness: development 1 to 1,000 for judging every package; **acceptance 300,001 to 301,000**, fresh and
    registered now, run once per behaviour package at its acceptance; **held-out 9,100,001 to 9,101,000**, fresh, run
    once at WP4 (S-10).
  - Matrix: the 24 seeds (four of record and the tool's first 20 others) for judging; **held-out: the tool's 45th to
    68th non-record seeds**, at WP4 only.
  - Battles 31 (100 seeds) and 33 (64 seeds) and the showcases as in the collision-classes evidence §1.1.
  - The 400-unit clashes on their five seeds.
- **Beyond noise.** The candidate's per-seed (or per-scene) mean lies outside the base arm's 95 % bootstrap interval
  of the mean (2,000 resamples, seed 7), on the worse side, as every recent movement plan reads it. Harness verdicts
  use `movement_harness --aggregate` (T1 to T9) with the solver plan's Amendment 8 step rule.
- **Every measurement** runs one at a time under `flock ~/scratch/gate.lock`. Scratch material lives under
  `~/scratch/gated-movement/`, never committed.

### 6.2 Hard checks: not figures, never traded

| ID | Check | Where | Since |
|---|---|---|---|
| **H-1** | No holding or attacking unit is moved by another body: `held_moved` = 0. Overlap present at a pair's first contact beyond the dead band (spawn, unload) is excluded and reported separately | every harness family, hold-line, enemy-press, the matrix, the clashes | WP1 |
| **H-2** | Enemies never push: `enemy_moved_by_others` and D-SHOVE-E = 0 for both sides (the same exclusion as H-1); the largest one-class enemy overlap ≤ 0.5 % of the radius sum at the end of every tick (T7's enemy bound; B3) | as H-1 | WP1 |
| **H-3** | Order independence: the permutation mode, unexplained differences = 0 | every family | WP2 (WP1 reported) |
| **H-4** | Determinism: `golden_replay` and every pinned digest identical on native debug and release and `wasm32-wasip1` release and debug (`wasmtime --dir=<real path>`) | goldens | every package that re-records |
| **H-5** | Terrain: disc penetration ≤ 2⁻¹⁰ world units (T7) | every family | WP1 |
| **H-6** | Identity: `Solver` and `Legacy` worlds byte-identical to WP0b's base (every matrix and harness CSV, the goldens under both pipelines) while `Gated` is not shipped | all | WP1 to WP3b |
| **H-7** | Stutter: R-11's lag of one tick and the resumed fire, in every `stutter` scene | stutter | WP2 |

The base arms are expected to fail H-1 and H-2. S fails them through D-δ's nudges and B-193's enemy split, which is
why the owner asked for them. They are reported for S and L, and not judged.

### 6.3 Figures, judged against S of the base (and R where named)

**Friend overlap (F-1 to F-4)**, from the first stepped tick, over pairs the solver corrects (friend-crowd WP0's
columns):

| ID | Check |
|---|---|
| **F-1** | Battles 26 to 29 and both clashes: friend pair-ticks above 5 % per seed at most **10 % of S's** (a fall of at least 90 %) and **not above R's** beyond noise. Every other battle: not above S's beyond noise |
| **F-2** | Battles 26 to 29 and the clashes: the largest friend overlap ≤ **10 %** of the radius sum (twice D-α's moving bound). Friend-crowd failed this in battle 27 (11.0 %). Every battle: not above S's |
| **F-3** | Harness: scenes with `fo_move_max` above 5 % not more than R's beyond noise in every family; in arc and corner, fewer than S's beyond noise; the T7 maxima of arc and corner not above S's (49.7 %, 52.0 % on friend-crowd's base) |
| **F-4** | Identity: H-6. Under `Gated`, a world in which no C′ rule and no gate clip fires (all four probe counters 0) is byte-identical to R (proved per scene by the counters) |

**B1 to B9** (the collision-classes §7.2 table with every repair carried over: D-SHOVE-E for B2, B7′ per engaged
second, B3 and B6 of one body class, B6 from the first stepped tick over corrected pairs, B8 standing while able, B9
cross-class corrections):

| Check | Broken when |
|---|---|
| B1 standing with room | D-R10 `free` or `Blocked` unit-seconds above S's beyond noise. From WP3b on, judged in **every** battle, battle 28 included; before WP3b, battle 28 is reported (B-192) |
| B2 shoving | = H-2 |
| B3 enemy overlap | = H-2 |
| B4 retarget churn | D-CHURN `unexplained` above S's beyond noise. From WP3b, D-ε's retargets are `explained` only when the old ring was full (the meter's rule); target switches per seed are reported against the design's prototype (130 to 300 per seed under re-claiming) |
| B5 give-ups | Give-ups of automatic chases above S's beyond noise |
| B6 friend overlap | = F-1 to F-3 |
| B7′ firing | Shots per engaged unit-second of either side below S's beyond noise |
| B8 standing while able to fight | Above S's beyond noise |
| B9 cross-class corrections | Any |

**Harness T1 to T9** (the solver plan §6.5), under the Amendment 8 step rule against S:
- a threshold S meets must be met;
- a figure S fails must not get worse beyond noise.

S fails T1 (arc 2.46, cross 1.20, gap 1.10; M1-hi cross 0.475, gap 0.257), T4 (every family) and T7 (arc, corner,
gap). T5 (fill ≥ 0.90, P5 ≥ 0.7476) and T9 are met and must stay met.

**New families:** each known answer of §5.2 holds in at least 99.5 % of scenes (the T4 standard), and R's rate is
reported beside it. The hard parts (H-1, H-2, H-3, H-7) admit no exception.

**T1 for `Gated`, the owner's churn condition (Q0120).** In **every** family (old and new) and in battles 26 to 29
and the clashes:
- M1 and M1-hi are not above S's beyond noise;
- they are ≤ 1.0 and ≤ 0.25 wherever S meets those bounds.

The prototype failed this in every gated arm: 1.6 to 11 per 1,000 against 0.3 to 0.8 (design §5.3 item 5).

### 6.4 The known failure modes `Gated` must beat

Every earlier attempt's measured failure is a named check here. A package that reproduces one has failed that check.

| # | Earlier failure (source) | Figure then | Caught by |
|---|---|---|---|
| K-1 | Friend-crowd WP2 (C-3′ on): battle 27's largest friend overlap (evidence §5.1) | 11.0 % > 10 % | F-2 |
| K-2 | Friend-crowd WP2: B1 in battles 27 (`free` 602 → 981, `Blocked` 1,000 → 1,403 blue) and 31 (red `Blocked` 39 → 97) | above the base's interval | B1 |
| K-3 | Friend-crowd WP2: B4 in battle 28 (3.67 → 4.79) | above | B4 |
| K-4 | Friend-crowd WP2: B7′ in battles 31 and 34 (by 0.0013 and 0.0003 shots per engaged second) | below | B7′ |
| K-5 | Friend-crowd WP2: Class D battles 29, 30, 34 moved | moved | §6.5 (a moved battle needs a clean trace and a restatement at WP4) |
| K-6 | Friend-crowd WP2: corner deadlocks and give-ups 7 → 9; T7 maxima arc 49.7 → 51.8 %, corner 52.0 → 69.1 % | worse | T4, F-3 |
| K-7 | Friend-crowd WP2: cost +47.2 % (64 × 64), +42.9 % (canyon), canyon about 56 ticks/s projected | — | §9 (`Gated` ≤ R; ≥ 50) |
| K-8 | Enemy-contact P2 (E-2 alone): battle 26 B6-max 37.06 → 40.44 %; battle 27 friend maximum 59.9 % (walkers pressing friends held at enemies, RC-1) | worse | F-2 (judged at WP2, reported at WP1) |
| K-9 | Solver-followup WP1 (E-1, closing shares): largest enemy overlap 2.6 → 12.7 % (battle 26) | 12.7 % | H-2 |
| K-10 | Solver-followup WP2 (G-6′): battle 28 failed seed of record 33; 21 of 24 held-out (22 needed); B1, B3, B4, B7 failed; cost `Legacy` +3.5 %, `Solver` +4.0 % from the shared acquisition test | — | §6.5 battle 28; B-checks; §9 C-2 |
| K-11 | Collision classes WP3 (B-209): zergling pairs above 5 % per 1,000 zergling-ticks 54.8 → 124.1 in battle 26; B4, B7′ | worse | re-measured after WP4 only (§10) |
| K-12 | Design prototype, gated arms: M1 1.6 to 11 per 1,000; surround fill 0.81 (against 0.93); enemy overlap 1.2 to 6.5 %; gap 0 / 20 and cross 13 / 20 complete (no route, no TTC rule); retarget switches 130 to 300 per seed | — | T1 (WP2's first stop), T5 and surround-moving, H-2, T4, B4 |
| K-13 | Solver plan Amendment 13 (side partner at time-to-contact zero), Amendment 16 (rule S lost the slide: arc give-ups 309 → 507) | — | T1, T4, B5 |

### 6.5 Battles and claims

- **The close-battle list** (Class R) is fixed in WP0b from S's 24-seed base by the collision-classes rule:
  - the minority side wins at least 3 of 24;
  - or the battle is named close or restated in the fixture;
  - or it is on friend-crowd's list (8, 9b, 10, 11a, 14, 21, 22a, 26, 27, 28, 31, 32, 33, 35, 36a, 36b, 37a, 37b, 38a,
    38b, 39a, 39b, 42).

  It is committed to the evidence file before WP1's first build.
- **WP1 to WP3b** do not change the shipped matrix (H-6). Their `Gated` matrices, played through a scratch catalog,
  are judged on §6.3 and reported battle by battle.
- **At WP4, battles move by design.** A pipeline change moves every melee battle, which the solver plan's WP6 also
  did. So:
  1. **Survivor ranges** are restated from G's 24 seeds in WP4's commit, each listed with its old and new range in the
     evidence (as the solver plan's WP6).
  2. **A winner that flips** in any battle needs a first-difference trace of one flipped seed. A trace that shows a
     broken behaviour (standing with room, shoving, oscillation, a wedge entered, a hard check) is a **bug and a
     stop**. A clean trace:
     - restates a Class R battle under §8 (claim rewritten, fixture comment citing AGENTS.md §8, Q0103, this plan and
       the mechanism);
     - sends a Class D battle whose claim is a StarCraft II statement to the owner (Q0127 draft);
     - restates a Class D battle whose claim is not one.
  3. **Held-out:** every restated claim holds on at least two thirds of the held-out matrix seeds. Otherwise stop and
     rewrite it from both ranges in an amendment. Nothing is widened or tuned from the held-out seeds.
- **Battle 28 must hold** (R-17, the claim "Without splash, immortals are overrun" is StarCraft II's):
  - WP3b's `Gated` matrix holds it on all four seeds of record and on at least 22 of the 24 judging seeds, the figure
    G-6′ missed. Otherwise WP3b stops.
  - At WP4 it holds on the seeds of record and on at least two thirds of the held-out seeds.
  - The `known_bug: Some("B-192")` marker is removed **in the WP4 commit that ships `Gated`**, the commit that makes
    the shipped matrix hold. `Battle::verdict` would report `StaleKnownBug` otherwise.
  - B-192's register row is closed in the same commit.
- **Battle 26** (claim: zerglings that pack round each colossus mostly overwhelm six colossi; S: 20 of 24 zergling
  wins). With D-δ retired, zerglings no longer slide engaged allies aside. The outcome may move toward the colossi
  (design §5.3 item 7, Q0118 (c)'s 9 → 2 of 12).
  - Registered now, before any measurement: if at WP4 it fails a seed of record **and** every §6.2 and §6.3 check
    passes for it with a clean trace, it takes `known_bug: Some("B-209")` in WP4's commit, as the enemy-contact plan
    §7.3 pre-registered.
  - The claim is **not** rewritten. It is true of the game, where zerglings walk under colossi (5.0.14, B-209), and
    the deviation has a registered cause, re-measured after WP4 (§10).
  - If a check fails for it, the stop applies as for any battle.
  - If it holds, nothing changes but its range.
  - This is the "restated under §8 if needed" of the design §7.7: the evidence restates its range and outcome, and
    the claim stays.
- **Battles 27, 29, 33, 38a, 38b** follow item 2.

### 6.6 Stop rules (in advance)

1. **Identity (H-6, F-4) fails:** a defect. Stop and find it. It does not count as an amendment.
2. **A hard check (H-1 to H-7) fails under `Gated`:** stop.
3. **The step rule** (§6.3) fails on any harness figure or any B-check: stop.
4. **WP2's first stop is T1.** Before any other WP2 figure is read, §6.3's T1 for `Gated` is evaluated on the
   development range (every family) and the battles 26 to 29. A failure stops WP2 at once.
5. **One amendment attempt per package.** It must name a literature source or an earlier measured rule of this
   project (S-5, S-6, S-7 are pre-registered). No tie-break, hysteresis or constant may be chosen after a trace
   (Amendment 14's bar). A second failure ends the package.
6. **If WP2 stops twice** (its first attempt and its amendment), the redesign ends:
   - `Gated` is not shipped and `Solver` stays the movement of record;
   - the evidence goes to the owner with the question of what to do with C-3′ (Q0126 draft), since friend-crowd §5's
     failures stand: `Solver` does **not** ship with C-3′ on without that answer;
   - **no third design round** starts without the owner (Q0097's precedent).

   WP1's latent code stays on `main` (H-6 holds), or is reverted if the owner prefers.
7. **WP3a or WP3b stops twice:** that package ends, and the orchestrator decides with the evidence whether WP4 can
   proceed without it. WP4 cannot proceed without WP3b, because battle 28 must hold. Two WP3b stops therefore end the
   redesign as rule 6 does, with Q0126 and the B-192 evidence.
8. **Cost (§9):** C-0, C-1 or C-2 fails after the allowed cost steps: stop, and re-ask Q0124 with the split.
9. **Held-out (WP4)** contradicts the development direction of any judged figure, or a restated claim fails item 3
   of §6.5: stop.
10. **Not allowed** in any package:
    - a constant, a per-unit, per-family or per-battle switch, or a tolerance chosen after seeing figures;
    - any change to D-α, the dead band, K, C-3′'s P-1 derivation or the global layer;
    - a change to `Solver` or `Legacy` behaviour (H-6).

---

## 7. Work packages

Every package follows the charter's routine:
- tests red first on the package's base (each red run recorded in the evidence, or by mutation when the tests need the
  package's new API, as friend-crowd §7 did);
- `cargo gate quick` and the tests the change needs, then hand back;
- a verifier runs `cargo gate package` on the final tree (Q0061).

The runtime rules:
- **One building agent at a time.** Builds use `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`.
- **Order:** WP0a → WP0b → WP1 → WP2 (and the Q0122 decision) → WP3a → WP3b → WP4. WP5 is a separate decision.
- **Merging:** every package from WP0a to WP3b merges to `main` latent (H-6), one at a time.
- **No other package edits `movement/`, `steering.rs`, `target_acquisition.rs`' admission test or
  `movement_rules.ron` while a package of this plan is open.** A bug fix that must touch them lands on `main` first,
  and this plan rebases.
- **Prerequisites:** friend-crowd's latent C-3′ (`dd53aa2f`) and the follow-order plan (schema 50) are on `main`
  before WP0b.

### WP0a: the nine scenes, the meter columns, the permutation probe (M; `sim_fixtures`, one `sim_core` probe; no behaviour change)

- **Code.**
  - **The new families.** `crates/sim_fixtures/src/harness/corpus.rs` gains the eight families of §5.2, and `Family`
    gains the eight variants. The known answers go in a new `harness/answers.rs`, the meters (`held_moved`,
    `enemy_moved_by_others`, per-rank overlap, stutter lag, fill time, gaps used) in `harness/metrics.rs`, and the
    verdict lines in `report.rs`.
  - **The CLI.** The `--permute <offset>` mode goes into `movement_harness`.
  - **The meter.** `trace/contact_checks.rs` gains `held_moved`, M1 and M1-hi, and the probe columns `gate_rays`,
    `orbit_refusals` and `terrain_transfers`, appended last in `matrix.csv` (an older header still parses).
  - **The clashes.** `trace/matrix.rs` or `trace/cli.rs` gains the `clash` mode.
  - **The probe.** `sim_core`: `SimWorld::set_movement_order_rotation`, the `MovementWorkspace` field, and the
    rotation in `advance_checked_movement` and in the solver's body fill (`collision.rs`). The three probe counters
    are read through `SimWorld` and read 0 until their rules exist.
- **Tests (red first; `crates/sim_fixtures/tests/harness_tests.rs` unless named):**
  - **W0-T1** `new_families_are_pure_functions_of_family_and_seed`: lattice and repeatability for each new family.
  - **W0-T2** `held_moved_counts_a_displaced_holder_and_not_a_turning_one`, on synthetic tracks.
  - **W0-T3** `enemy_moved_by_others_ignores_the_enemys_own_step`.
  - **W0-T4** `the_wedge_answer_fails_a_pass_through_the_narrow_gap`.
  - **W0-T5** `stutter_lag_is_read_from_the_order_tick`.
  - **W0-T6** `surround_fill_time_starts_at_first_contact`.
  - **W0-T7** `lanes_answer_requires_both_gaps`.
  - **W0-T8** `trace_tests::m1_of_the_meter_matches_the_harness_definition` (the same synthetic track through both).
  - **W0-T9** `contact_solver_tests::order_rotation_zero_is_the_identity`: battle 28's first 150 ticks with offset 0
    give the shipped per-tick checksums.
  - **W0-T10** `contact_solver_tests::order_rotation_never_enters_the_checksum`: the same world with two offsets
    gives equal checksums whenever its positions are equal (the field is unhashed).
  - **W0-T11** `five_seeds_of_each_new_family_play_without_a_diagnostic_and_repeat`.
- **Acceptance:** every world, checksum and harness CSV byte-identical to `main`; only new columns and families.

### WP0b: the base of record (S; measurement and records only)

- On WP0a's tree, arms L, S and R:
  - the whole suite of §5 on the development range (old and new families, permutation mode);
  - the matrix on the 24 seeds; battles 31 and 33; the showcases; the clashes;
  - throughput (§9) by `perf stat -e instructions:u`, three runs per scenario and arm.
- **The close-battle list** (§6.5) is computed and committed to `docs/evidence/2026-11-27-gated-movement.md` before
  WP1's first build. The acceptance and held-out ranges of §6.1 are recorded there as registered and unrun.
- **The suite's first run on the new families.** Before any `Gated` code exists, the known answers are run on S, R
  and L. A known answer that **every** arm fails in a way the family's text did not foresee (a generator defect, an
  impossible bound) is corrected in an amendment with the trace, as the solver plan's §6.6 C-a. This is the last
  point at which a known answer may change.
- **Conditional bug rows:** B-224 to B-226 (§13.1) are filed only if WP0b measures them under S.
- **Acceptance:** none beyond reproducibility (two runs of one family give identical CSVs).

### WP1: C′, the contact rules, latent under `Gated` (M; `sim_core`, catalog)

- **Code.**
  - `catalog/movement_rules.rs`: `MovementPipeline::Gated` with its doc; `GatedRules` (`crowd_iterations` in WP1);
    the `gated` section, required under `Gated`; validation with typed errors; `MovementPipeline::runs_solver(self)`
    (true for `Solver` and `Gated`). Every `== MovementPipeline::Solver` in `movement/` and `target_acquisition.rs`
    becomes `runs_solver()` (**verify** the list: `collision.rs:324`, `locomotion.rs:789, 823, 1320, 1477, 1655` at
    `6bd0fc4b`, and any added by follow-order).
  - `movement/solver.rs`:
    - `ShareRules` and C′'s rows in `contact_shares`;
    - E-2 ported from `7f686d23`: the responsibility weights on `Pair`, `bound_enemy_steps`, application zero,
      `SolverBody::committed`, and the pair-list reach;
    - C′-3 (`SolverBody::held_by_terrain`, written in `apply`, read in `accumulate`);
    - the cap read from `gated.crowd_iterations` under `Gated`;
    - the module doc's C-2 table and invariants restated.
  - `movement/collision.rs` (`solve_contacts`): the pipeline in `SolverSettings`; bodies entered at `start` with the
    committed position; no `nudge` under `Gated`.
  - `assets/catalog/movement_rules.ron`: the `gated` section with `crowd_iterations: 151` and its header comment.
    `pipeline` stays `Solver`.
- **Tests (red first; `crates/sim_core/tests/contact_solver_tests.rs` unless named; scratch archetypes):**
  - **C1-T1** `gated_is_the_reference_where_no_c_prime_rule_applies`: an enemy-free, attack-free crowd with no wall
    contact plays bit-identically under `Gated` and R (per-tick checksums). It is red until `Gated` parses.
  - **C1-T2** `an_attacking_unit_is_never_nudged_under_gated`: a melee attacker in band, pressed along its ring by a
    friend. Its displacement is 0. Red: `Gated` falls back to `Solver`'s D-δ until C′-1.
  - **C1-T3** `a_holding_line_is_not_moved_by_a_pressing_crowd_under_gated`: H-1's scene in miniature.
  - **C1-T4 to C1-T10**, E-2's tests ported under `Gated`, with names and scenes as the enemy-contact plan §8 WP2
    gives them: E2-T1 `a_pressing_crowd_does_not_move_a_standing_enemy`, E2-T2 `a_walker_stops_at_a_standing_enemy`,
    E2-T3 `a_grazing_walker_slides_along_an_enemy`, E2-T4 `two_walkers_meet_at_contact`, E2-T5
    `a_body_pressed_by_friends_stops_at_an_enemy`, E2-T6 `solver::tests::a_wedged_body_is_not_admitted`, E2-T7
    `solver::tests::the_bound_does_not_depend_on_body_order`.
  - **C1-T11** `solver::tests::a_terrain_held_body_passes_its_share_to_its_partner` (B-220): a 0 / 1 friend pair
    whose share-1 body stands against a wall, pressed. The overlap ends within D-α's moving bound within K iterations
    under `Gated`, and does not under `Solver`. Red.
  - **C1-T12** `solver::tests::the_terrain_transfer_does_not_depend_on_body_order`: every permutation of a mixed
    crowd against a wall gives the same positions.
  - **C1-T13** `solver::tests::a_body_held_by_two_walls_is_fixed_for_both_partners`: no correction, no oscillation
    over 20 iterations.
  - **C1-T14** `contact_solver_tests::the_gated_crowd_cap_covers_the_catalogs_crowd`: P-5 for the `gated` value.
  - **C1-T15** `movement_rules::tests::the_gated_section_is_required_under_gated_and_validated`: absent under
    `Gated`, a cap below `iterations` or above 1,000, and an unknown field are each refused with a typed error.
  - **C1-T16** `golden_replay_tests` under `Solver` and `Legacy` unchanged (H-6). These are guards, not new.
  - **Guards** that must stay green: `push_priority_tests.rs`, `collision_and_immobility_tests.rs`,
    `fixed_body_transactions_tests.rs`, and C-3′'s P-1 to P-7.
- **Measurement and acceptance.** G at WP1 against S and R, on the development range and the 24-seed matrix:
  - **Judged:** H-1, H-2, H-5, H-6, and F-4's second half on gap, cross and corner. Those families have no enemy and
    no attacker, so a scene whose `terrain_transfers` is 0 must be byte-identical to R's.
  - **Reported, not judged:** F-1 to F-3, B1 to B9, T1 to T9, and H-3. The reason is registered now: under the owner's
    rules without the gate, a pressing crowd defines an infeasible constraint set (RC-3; design §5.3 item 2, `Jconv-O`;
    K-8). WP1's figures are the C′-only arm against which WP2's gate is read. If WP1 measures F-2 above K-8's figures,
    that is the expected RC-1 residual, not a stop.
  - **The amendment candidate** (S-7): if WP2 later fails F-1 or F-2 with traces showing chains behind E-2-held
    bodies, C′-3's held set is extended to bodies whose step E-2 bounded to zero in the previous application (F-1 of
    enemy-contact Amendment 2, restated as one rule; source N-3, shock propagation restricted to held bodies). It is
    used as WP2's one amendment.
- **Records:**
  - `DESIGN.md` §4: B-220 noted as fixed under `Gated` (latent), closed at WP4.
  - Q0052: the D-δ addendum (§11) written as "retired under `Gated`", in WP1's commit.
  - The schema is unchanged (no hashed field; catalog data is not hashed).

### WP2: I′, the gated intent, and the Q0122 measurement (L; `sim_core`, catalog)

- **Code.**
  - `movement/gate.rs` (new): `GateBody`, `gives_way`, `free_distance`, `choose_bin` (I′-3 and I′-4) and
    `GateChoice { velocity, side, stood }`, with a `//!` doc of responsibility, place and invariants. Every function
    is pure and checked Q32.32.
    - A `// PERFORMANCE RATIONALE:` block (Why, What, Trade-off) covers the squared-form miss test before the root and
      the reach pre-filter.
  - `movement/locomotion.rs`: `step_toward` under `Gated` calls the gate after `keep_terrain_view` and clips the final
    step after the wall slide and the hull gate (I′-7), and gates the snap. `proposed_heading` is unchanged and gives
    `d₀`. The stand branch is I′-6. `gather_neighbours` under `Gated` reaches `r + r_max + step`, within the existing
    reach (**verify**).
  - `steering.rs` (`SteeringObstacle`): `engaged`, `queued`; `obstacle_of` fills them.
  - `entity.rs`: `MovementMemory::gate_side: i8`, cleared by `restart()`. `checksum.rs` and `checksum/encoding.rs`:
    encode it after `slot`, **schema next free at merge (51 provisional)**. Goldens are re-recorded by bytes only,
    with a proof header: "checksum schema 50 → 51, one hashed field added and never written under `Solver` or
    `Legacy`; render snapshots byte-identical". It is proved by an unhashed scratch build giving WP1's digests, as
    G-6′'s Amendment 10 practice.
  - `catalog/movement_rules.rs` and `movement_rules.ron`: `bins`, `stand_permille` in `gated`, with their header
    derivations.
  - The probe counter `gate_rays`.
- **Tests (red first; `crates/sim_core/tests/gated_pipeline_tests.rs` (new) or `gate::tests`):**
  - **I2-T1** `gate::tests::the_free_distance_is_the_first_contact_along_the_ray`, against hand-computed roots
    (exact in Q32.32 to one raw unit).
  - **I2-T2** `gate::tests::a_step_never_enters_a_body_that_will_not_give_way`: a property over 2,000 generated scenes.
    The overlap with every non-yielding body after the step is at most its overlap before it.
  - **I2-T3** `gate::tests::the_own_step_never_increases_the_distance_to_the_preferred_point`: progress bins only.
  - **I2-T4** `gate::tests::the_kept_side_holds_while_blocked_and_releases_when_free`.
  - **I2-T5** `gate::tests::the_choice_is_rotation_and_mirror_consistent`: a scene rotated by 90° rotates the chosen
    step exactly; a mirrored scene chooses the mirrored bin, or the right-hand tie on exact symmetry.
  - **I2-T6** `gate::tests::who_gives_way_follows_the_share_table`: every row of §3.1 under both D-β arms.
  - **I2-T7** `gated_pipeline_tests::two_blocked_neighbours_do_not_ping_pong`: the design's §5.4 scene (a zergling
    beside its slot between two blocked neighbours), M1 = 0 over 50 ticks. Red on WP1: the solver path presses and
    reverses (**verify** red, else record it as a guard).
  - **I2-T8** `gated_pipeline_tests::a_queue_behind_an_attacking_front_stays_within_the_moving_bound`: deep-queue in
    miniature, 6 ranks. The largest friend overlap stays within 5 % from tick 25. Red on WP1 (RC-1).
  - **I2-T9** `gated_pipeline_tests::the_wedge_is_never_entered`: both wedge sub-scenes; the walker routes round.
  - **I2-T10** `gated_pipeline_tests::a_head_on_pair_still_passes_right_to_right`: I-3b under `Gated`, a guard.
  - **I2-T11** `stutter_step_tests::a_move_order_steps_on_the_next_tick_under_gated` (H-7) and
    `…::an_attack_order_resumes_fire_under_gated`.
  - **I2-T12** `gated_pipeline_tests::the_tick_does_not_depend_on_processing_order`: three rotations of a mixed
    100-body battle over 200 ticks, identical checksums (H-3).
  - **I2-T13** `gated_bins_are_the_coarsest_below_the_side_rule_turn` and **I2-T14**
    `gated_stand_line_is_the_harness_moving_line` (§4.2).
  - **I2-T15** `gated_pipeline_tests::a_wall_slide_does_not_re_inject`: a walker sliding along a wall toward a holding
    body ends touching it, not overlapping it (I′-7).
  - **I2-T16** `crowd_edge_tests::a_blocked_move_is_still_given_up` under `Gated`, a guard.
- **Acceptance, in this order:**
  1. **T1 first** (§6.6 rule 4): G-β+ and G-β− on the development range and battles 26 to 29. If **both** arms fail
     T1, WP2 stops. If one passes, that arm continues as the only WP2 candidate. That is item 1 of §8's rule.
  2. H-1 to H-7 for the continuing arm(s).
  3. F-1 to F-4, B1 to B9, T2 to T9 and the known answers, against S and R, on the development range.
  4. The Q0122 decision (§8).
  5. The chosen arm on the acceptance range 300,001 to 301,000, once.
  6. Cost (§9): C-1 interim and C-2.
- **Records:**
  - the evidence tables, both arms in full;
  - `moving_friends_push` in the shipped `movement_rules.ron` is **not** changed (it governs `Solver` too). The
    decided value goes into `gated` as `moving_friends_push` only if β− wins (a field added then, its header naming
    Q0122);
  - Q0052's D-β line per §11;
  - the gate's figures for the design's prediction (RC-1 removed): friend pair-ticks above 5 % against WP1's and R's.

### WP3a: G′-2, the orbit approach (M; `sim_core`)

- **Code.**
  - `movement/orbit.rs` (new): `OrbitAim { Dive(point), Orbit(point) }`, `orbit_aim(T, ρ, r_max, r, β, σ, way,
    intervals, step)`, and the terrain fallback. `//!` doc.
  - `ring.rs`: `ring_aim_with` returns `r_max` with the intervals (no new scan).
  - `locomotion.rs::prepare_pursuit` under `Gated` puts `Orbit`'s point in `pursuit.gap`, so `chase_target` walks to
    it. The progress measure is to the slot, not the orbit point, so a unit on the orbit does not accrue stall
    (`check_pursuit_progress` reads `slot`).
  - The probe counter `orbit_refusals`.
  - No new state, no schema change.
- **Tests (red first):**
  - **G3-T1** `ring::tests::a_free_kept_gap_is_never_rechosen` (G′-1, a guard).
  - **G3-T2** `orbit::tests::the_orbit_clears_every_body_on_the_ring`.
  - **G3-T3** `orbit::tests::the_angular_distance_to_the_slot_falls_with_every_orbit_point`.
  - **G3-T4** `orbit::tests::an_unobstructed_arc_dives`.
  - **G3-T5** `orbit::tests::an_orbit_point_in_terrain_falls_back_to_the_gap`.
  - **G3-T6** `gated_pipeline_tests::a_chaser_behind_the_front_reaches_a_far_slot`: a chaser whose bearing is behind
    an attacking front rank and whose gap lies 120° round. It reaches range within twice the orbit's arc length over
    speed. Red on WP2: the gate stands it behind the front, the local minimum of RC-5.
  - **G3-T7** `gated_pipeline_tests::the_orbit_does_not_depend_on_processing_order`.
- **Acceptance:** §6 against S and R, and against WP2's G as the step rule's predecessor. Arc T5, surround-moving
  fill and corridor fill must not fall below WP2's beyond noise; the prototype's 0.81 against 0.93 is K-12. Then the
  acceptance range, once.
- **Amendment candidates (S-5, S-6):** the wavefront fallback (`75871a42`, contact-solver Amendment 12 step 2′) if
  `orbit_refusals` traces explain a corner or corridor failure; visible claims with an age key (G-3, G-4 of the solver
  plan) if T5 or the surround-moving fill fails. Only one of the two may be tried, as the package's one amendment.

### WP3b: G′-3, D-ε, retarget from a full ring (L; `sim_core`, catalog none)

- **Code.** The G-6′ port from `dc3b62ab` (§3.3), under `Gated` only. It covers `ring.rs`, `target_acquisition.rs`
  (the shared admission test), `locomotion.rs`, `mod.rs` (`MotionProposal::ring_retarget`), `entity.rs`
  (`MovementMemory::ring_retarget`, cleared by `restart()` and on any change of target) and `checksum*`.
  - **Schema next free at merge (52 provisional),** proved by an unhashed scratch build giving WP3a's digests;
    goldens by bytes only.
  - No data switch: D-ε is sanctioned and is `Gated`'s rule. The port's `retarget_from_full_ring` field is dropped.
- **Tests (red first; `crates/sim_core/tests/ring_retarget_tests.rs` from the branch, re-pointed at `Gated`):**
  - **G-T1 to G-T9** as on `dc3b62ab`;
  - **G3b-T10** `a_retargeted_chaser_keeps_its_new_target_until_reached` (Q0123's "until reached": the original
    target gains room while the chaser walks; the chaser does not return);
  - **G3b-T11** `the_retarget_is_the_nearest_of_the_highest_rank`;
  - **G3b-T12** `solver_and_legacy_never_retarget` (H-6 in miniature).
- **Acceptance:**
  - §6 against S and R; B1 judged in every battle including 28; B4 per §6.3;
  - battle 28 per §6.5 (four seeds of record, at least 22 of 24 judging seeds) under `Gated`;
  - the acceptance range, once;
  - cost C-2 (`Solver` and `Legacy` unchanged by the shared admission test: K-10).
- **Records:**
  - Q0052's D-ε addendum (§11);
  - `DESIGN.md` §4: B-192 "fixed under `Gated`, closed at WP4", with G-T1 and D-R10 `free` as evidence.

### WP4: the switch-over (M; data, records; `full` gate)

- **The data.** `pipeline: Gated` in `movement_rules.ron`, in one commit with:
  - battle 28's marker removed and B-192 closed;
  - battle 26 as §6.5 decides;
  - the restated survivor ranges and claims (§6.5);
  - `golden_replay`, `DEFAULT_SANDBOX_CHECKSUM` and every pinned digest that moves, re-recorded once on four builds
    with proof headers naming this plan, WP4 and the first changed checkpoint (H-4).
- **The held-out run, once:** harness 9,100,001 to 9,101,000 and the matrix's 45th to 68th non-record seeds, arms L,
  S, R and G from one binary. They confirm the development direction of every judged figure (§6.6 rule 9).
- **Throughput of record** (§9): C-0 on a quiet host, C-1 final. The `DESIGN.md` §5 table gains the `Gated` column and
  the ratio to R and to L.
- **The showcase storyboard** is re-checked with a real render on the RX 580 (radv), `with-gpu-lock` per take, with
  `WGPU_BACKEND=vulkan` and the radv `VK_ICD_FILENAMES`. `assets/captures/showcase.ron` is re-timed if the default
  seed's timeline moved; `showcase_storyboard_tests` and `showcase_tests` pass.
- **Tests:** `contact_solver_tests::the_shipped_pipeline_is_gated`; every test of §8.1 of the solver plan that asserts
  `Solver`'s behaviour by name is re-pointed at the shipped pipeline or pinned to `Solver` through a test catalog
  (**verify** the list in WP4; it is mechanical).
- **Records:**
  - `DESIGN.md`: the Movement row; §4 B-192, B-193, B-219 and B-220 closed with the figures; B-190 and B-191 restated
    or closed on T4 and T7.
  - The contact-solver and friend-crowd plans each gain an amendment pointing here. Q0113's and Q0118's files gain a
    `## Resolution` addendum.
  - This plan's row becomes "Partly done" (WP5 open) or "Done".
- **Gate:** `cargo gate full` on the merged tree before push.

### WP5: deleting the superseded pipelines (M; a separate decision, not part of this plan's acceptance)

**Not started by this plan.** `Legacy` and `Solver` are the measurement references of every §6 check (arms L, S and
R). Deleting them before `Gated` has proved itself on `main` would remove the comparison any regression needs.

**When it may start.** After WP4 has been on `main` through at least two later movement-touching packages with no
movement regression reported, the orchestrator decides with a short note. The note states whether a frozen binary of
the WP4 tree under `~/scratch/gated-movement/reference/` is enough to keep the measurement references.

**Its content when it runs:**
- `Legacy`'s code, constants, memory fields (`flank_*`, the walk) and tests (the solver plan's WP7 list);
- `Solver`'s superseded rules: D-δ (`nudge_engaged_along_ring`, `nudge_radial`, `ContactParty::nudge`), the enemy
  ½ / ½ split for new overlap, `contact.crowd_iterations` under `Solver`;
- the value `Solver` of `contact.pipeline`, and then the switch itself;
- the archetype field `surround_walk` (its compatibility test's policy, **verify**);
- the schema next free at merge (53 provisional);
- goldens by bytes only with render snapshots byte-identical;
- the harness development range bit-identical to WP4's;
- `cargo gate full`.

---

## 8. Q0122 (c): pushing by priority (D-β) against moving allies block

**What is compared.** Two arms on WP2's tree, from one binary, differing only in `contact.moving_friends_push`:
- **β+** (true, today's D-β): two moving friends share by priority (the lower takes all, equal ½ / ½). The gate
  treats a crossing or oncoming friend of no lower priority as not giving way (I′-1).
- **β−** (false, SC-8's "moving units block"): each moving friend takes back its own approach (`approach_shares`).
  The gate treats every moving friend that is not co-moving as not giving way.

Idle allies are still pushed in both arms (Q0122's wording). Co-moving friends give way in both.

**Why WP2.** The owner asked for the measurement "inside the new architecture". Before the gate (WP1), β− and β+
differ mostly in how the solver undoes injected overlap, which the gate removes. The comparison would then measure
RC-1, not the rule.

**Measured:** every figure of §6 on the development range for both arms, plus five comparison figures chosen now
because each is where the two rules differ in kind:
- **Q-a:** friend pair-ticks above 5 % in battles 26 to 29 and the clashes (F-1's meter);
- **Q-b:** M1 in gap, cross, hallway and lanes;
- **Q-c:** gap completion time and η (T6), on the mixed-priority quarter alone and on all scenes;
- **Q-d:** cross and hallway completion time and give-ups;
- **Q-e:** lanes' clump area and A-1 violations.

**The decision rule (pre-registered).** Applied in order; the first item that separates the arms decides:
1. **Hard checks and T1.** An arm that fails a hard check (H-1 to H-7) or T1 is out. If both are out, WP2 stops
   (§6.6).
2. **Thresholds.** An arm that fails a T-threshold, a known answer, F-1 to F-3 or a B-check that the other arm passes
   loses.
3. **Comparison figures.** Count, for each arm, the figures Q-a to Q-e on which it is better than the other beyond
   noise (the other arm's bootstrap interval). An arm better on at least two more figures than the other wins.
4. **Otherwise** (a tie, or a split of one figure), the figures go to the owner as **Q0125** (§13.2). WP2 continues
   on the default **β+** (Q0122's default (a), today's sanctioned D-β). WP3a and WP3b are built on β+, and the
   orchestrator re-runs their acceptance on β− if the owner picks it. WP4's switch-over waits for the answer.

When rules 1 to 3 decide, Q0125 is not filed. The figures and the rule that decided go into Q0122's closed file as
an addendum, and the outcome goes into Q0052 (§11).

**What either outcome means for the deviation.**
- **β− wins:** `Gated` uses StarCraft II's SC-8 rule. D-β is not used under `Gated` (recorded as retired under
  `Gated`), and the shipped data's `gated.moving_friends_push: false` names Q0122.
- **β+ wins:** D-β stays sanctioned under `Gated` as under `Solver`.

---

## 9. Cost (Q0124 (a), Q0093 (c), R-16)

**Measurement.** Release benchmark binaries with one scenario each (64 × 64 armies meet, 128 × 128 armies apart,
showcase canyon, the 67-caster battle; 400 `pulse_bot`, 300 ticks). Each is measured by `perf stat -e
instructions:u`, three runs per arm and scenario, under the gate lock. Noise is the spread of the three runs.

**Reference (WP0b re-measures).** R is `Solver` with `crowd_iterations: 151`, from a test catalog. Friend-crowd
evidence §5.4 measured it on its tree at 24.091 G (64 × 64), 10.976 G (128 × 128) and 39.047 G (canyon), against S
at 16.367 G, 10.971 G and 27.320 G. Projected quiet-host rates are about 75, 183 and 56 ticks/s.

**Gates:**

| ID | Gate | When |
|---|---|---|
| **C-0 (hard)** | Every scenario at least **50 ticks/s** under the shipped pipeline. Measured on a host at load average below 8; the measurement is repeated until the host is quiet, and the evidence records the load. On a loaded host the projection from instructions (as friend-crowd §5.4) is reported, never recorded as C-0 | WP4 (of record); projected at WP2, WP3a, WP3b |
| **C-1 (budget)** | `Gated` instructions **at most R's** on every scenario, beyond noise | WP4 (final). Interim at WP2 to WP3b: at most **1.25 × R** on any scenario (a cost step is owed before WP4 if it is above R) |
| **C-2 (latent)** | `Solver` and `Legacy` instructions within noise of WP0b's on every scenario | WP1 to WP3b (K-10's lesson) |

**Allowed cost steps** (behaviour-preserving, proved by bit identity of every harness CSV and matrix world, any number,
any package):
- a per-tick cache of each unit's non-yielding neighbour list, filled once in `gather_neighbours`;
- bins pruned when a bin's ray cannot score above the best found (its `cos · step` bound);
- the enemy-pair index of E-2 (enemy-contact §9's allowed step);
- skipping C-3′'s excess scan on ticks whose solve ends within the bound.

**If C-1 fails** after them: stop (§6.6 rule 8), and Q0124 is re-asked with the per-phase split (intent, gate, solver
iterations, C-3′ extension). Only by the owner's leave may a behaviour-changing cost step follow, for example
graph-coloured Gauss–Seidel for the C-3′ extension alone (design §7.6). It would re-run §6.

**Reported** in `DESIGN.md` §5 at WP4, for every scenario:
- the ratios `Gated` / R, `Gated` / S and `Gated` / L;
- the mean iterations per tick and `solver_extended_ticks`, which show how rarely C-3′ extends under the gate;
- `gate_rays` per tick.

---

## 10. Records: bugs, deviations, re-measures

| Row | What this plan does | Package |
|---|---|---|
| **B-192** (stand behind a full ring; battle 28) | Fixed by G′-3 under `Gated`; marker removed and row closed in WP4's commit | WP3b, WP4 |
| **B-193** (enemy shoving) | Fixed by E-2 (C′-2), made feasible by the gate | WP1 (latent), WP4 |
| **B-219** (crowd overlap beyond D-α; friend-crowd) | Fixed under `Gated` by the gate plus C-3′ (F-1, F-2); closed at WP4. Under `Solver` it stays open until WP5 deletes `Solver` | WP2, WP4 |
| **B-220** (terrain-held 0/1 shares) | Fixed by C′-3 | WP1, WP4 |
| **B-190, B-191** (give-ups with room; overlap maxima in gap and corner) | Expected to fall (G′-2, C′-3); judged by T4 and T7; closed or restated with figures | WP4 |
| **B-208** (K-1) | Already fixed on `main`; E-2's port relies on it | — |
| **B-209** (zerglings under the colossus) | Orthogonal (collision classes). **Re-measured after WP4:** a scratch merge of `collision-classes-wp3-wip` (`42761cf2`) onto WP4's `main`, judged on that plan's §7.2 B1 to B9 against a base re-derived on the same `main`. Records only: one evidence section here, and a `## Resolution` addendum to Q0117 saying which of its stopping checks are gone. The branch resumes under its own plan | after WP4 |
| **B-213** (cliff movers) | Out of scope; unaffected | — |
| **Q0113** (enemies block, parked) | E-2 lands as part of C′; Q0113's file gains a resolution addendum at WP4 | WP4 |
| **Q0118** (C-3′) | C-3′ is on under `Gated` (§3.1.3); `Solver` keeps it off | WP1, WP4 |
| **B-224 to B-226** | Conditional rows (§13.1), filed by WP0b only if measured | WP0b |

---

## 11. Deviation records (Q0052)

Each addendum goes into `docs/questions/closed/0052-sanctioned-sc2-deviations.md` in the commit named, in its
existing style:

```markdown
## Addendum (WP1 date): attacking units are never nudged under the Gated pipeline, from Q0121

Sanctioned by the owner's answer to [Q0121](0121-nudge-under-new-movement.md), option (a), for plan
`docs/plans/2026-11-27-gated-movement.md`:

- **D-δ retired under `Gated`:** under the `Gated` pipeline no ally moves an attacking or holding unit, as in
  StarCraft II. D-δ stays in force under `Solver` until that pipeline is deleted (the plan's WP5).
```

```markdown
## Addendum (WP3b date): retarget from a full ring, from Q0123

Sanctioned by the owner's answer to [Q0123](0123-retarget-from-full-ring.md), option (a), for plan
`docs/plans/2026-11-27-gated-movement.md`:

- **D-ε:** under the `Gated` pipeline, a unit chasing an automatically acquired target whose attack ring has no free
  place switches to the nearest enemy of the same acquisition rank within its scan range whose ring has one, and
  keeps it until it reaches it. StarCraft II's exact rule is not documented; this is a minor deviation unless a
  source shows the game does the same.
```

```markdown
## Addendum (WP2 date): moving friends under the Gated pipeline, from Q0122

Measured as the owner asked in [Q0122](0122-moving-friends-push-or-block.md), option (c), for plan
`docs/plans/2026-11-27-gated-movement.md` §8: <β+ | β−> was chosen by <rule item>.

- <β+:> **D-β** stays sanctioned under `Gated`.
- <β−:> **D-β is not used under `Gated`:** moving friends block each other, as SC-8 says StarCraft II's do.
```

D-α and D-γ are unchanged (the design §7.3). S-1 to S-10 (§2.2) are simplifications of this project's own design,
not differences from StarCraft II, so they need no Q0052 entry. They are recorded here.

---

## 12. Risks and scope

### 12.1 Risks

| Risk | Mitigation |
|---|---|
| The gate makes crowds too polite (queues instead of pressing), moving battle 26 toward the colossi | Idle friends still yield; Q0122 measured; battle 26's rule pre-registered (§6.5); R-9 judged by deep-queue, D-R10 and B1 |
| Bin choice oscillates (RC-4) | Progress-only bins, the lexicographic kept side, the kept gap; T1 is WP2's first stop; I2-T7 is the design's own trace scene |
| The orbit meets terrain | The fallback to today's chase; `orbit_refusals` counted; the wavefront as WP3a's amendment candidate |
| D-ε's retargets churn | Kept until reached; B4 judged; switches per seed reported against the prototype's 130 to 300 |
| E-2's port interacts with follow-order's resting followers (D-7: an idle body to the solver) | Follow-order lands first; WP1's guards include `follow_order_tests` (**verify** the file name after its merge) |
| The gate's cost (7 bins × non-yielding neighbours) | Allowed cost steps (§9); interim C-1 at 1.25 × R; final at R |
| The permutation probe finds order dependence under `Solver` (live reads in locomotion) | Reported as B-224 (conditional, §13.1); judged only under `Gated` (H-3), where every new read is the snapshot's |
| The fresh held-out ranges disagree with development | The stop of §6.6 rule 9; nothing tuned on them |
| WP3b's port is large (G-6′'s 1,742 lines on a moved base) | Its own package; G-T1 to G-T9 travel with it; H-6 and C-2 prove `Solver` untouched |

### 12.2 Out of scope (Q0002)

- The global layer (navmesh, TA*, funnel, lanes), collision classes (B-209, B-211, Q0117), air separation (the gate
  applies to air bodies unchanged), and cliff movers (B-213).
- Gauss–Seidel or shock propagation as the default solver (design §6.1). It is only a later cost step, by the owner's
  leave (§9).
- Flow fields, the continuum layer, Implicit Crowds, ORCA (design §8).
- Visible slot claims, the wavefront fallback, and F-1's extension. Each is pre-registered as one package's
  amendment candidate (S-5 to S-7), and not built otherwise.
- Shipping `Solver` with C-3′ on (§3.1.3).
- WP5 (§7, WP5).

---

## 13. Drafts (provisional numbers; not filed by this plan's commit)

### 13.1 Bug rows (filed by WP0b only if measured under `Solver`)

```markdown
| B-224 | Under the contact solver the movement tick's result depends on the order in which units are processed: the
permutation mode of the movement harness (plan `2026-11-27-gated-movement` §5.2) finds <n> scenes in <families> whose
positions differ when the processing order is rotated, with no exact tie involved (first difference: <trace>) |
plan `2026-11-27-gated-movement`; open under `Solver`, judged absent under `Gated` (H-3) |
```

```markdown
| B-225 | Under the contact solver a unit attacking in its band is moved by allies beyond D-δ's tangential nudge, or a
holding unit is moved at all: `held_moved` reads <figure> in <scenes/battles> (plan `2026-11-27-gated-movement` H-1)
| plan `2026-11-27-gated-movement`; fixed by `Gated` at WP4 |
```

```markdown
| B-226 | A ranged unit in contact with friends behind it takes more than one tick to step back on a Move order (stutter
step): the `stutter` family measures a lag of <n> ticks in <share> of scenes under `Solver` (R-11) | plan
`2026-11-27-gated-movement`; judged under `Gated` (H-7) |
```

### 13.2 Questions

```markdown
# Q0125: Moving units of yours that bump: push by priority or block each other? The measurement you asked for

- **Asked:** (date of filing; only if plan `2026-11-27-gated-movement` §8's rule ties or splits)
- **Blocking:** only the switch-over of the new movement (WP4)
- **Default until answered:** (a), pushing by priority (today's D-β); the remaining packages are built on it.

## Background

You asked (Q0122 (c)) to measure both rules inside the new movement. Measured on <seeds>:

| Figure | Push by priority | Block |
|---|---|---|
| Crowd overlap in the zergling battles (pairs above 5 % per run) | | |
| Direction changes per 1,000 moving unit-ticks (gaps, crossings, hallways, lanes) | | |
| Time for a group to pass a gap; throughput | | |
| Crossings and hallways: completion time, give-ups | | |
| Group moves through two gaps: clumping | | |

Neither rule is better on enough of these to decide by the rule written in advance.

## Options

- **(a) Push by priority (today, D-β).**
- **(b) Block each other, as StarCraft II's moving units do (SC-8);** idle units are still pushed.

## Answer

```

```markdown
# Q0126: The new movement did not pass its tests twice. Keep today's movement as it is, or switch the crowd fix on despite its failed checks?

- **Asked:** (date of filing; only if plan `2026-11-27-gated-movement` §6.6 rule 6 or 7 ends the redesign)
- **Blocking:** no
- **Default until answered:** (a); today's movement stays, the crowd fix stays off.

## Background

The redesign you approved (Q0120 (a)) stopped twice at <package> on <check> (<figures>). As agreed, no third round
starts without you. The crowd fix you approved in Q0118 was built but did not pass its own checks when switched on:
crowd overlap fell 99 %, but one battle's worst overlap stayed at 11 % (target 10 %), units stood with room in two
battles, three battles moved without a reason in their traces, and it costs +47 % on the 400-unit benchmark (about 56
ticks per second on the canyon map, above your 50).

## Options

- **(a) Keep today's movement** (crowds overlap up to about 40 %; B-219 open).
- **(b) Switch the crowd fix on** despite its failed checks.
- **(c) Something else** you would like tried; say what.

## Answer

```

```markdown
# Q0127: The new movement changes who wins a battle whose claim is a StarCraft II fact

- **Asked:** (date of filing; only if plan `2026-11-27-gated-movement` §6.5 item 2 meets a Class D battle with a
  StarCraft II claim that flips with a clean trace)
- **Blocking:** only the switch-over (WP4)
- **Default until answered:** the battle is marked with a known-bug row and the switch-over proceeds.

## Background

Battle <n> (<claim>) flips from <old> to <new> on <k> of 24 runs under the new movement. The trace shows no broken
behaviour: <one sentence of mechanism>.

## Options

- **(a) Keep the claim and mark the battle with a known bug** until the mechanism is understood (default).
- **(b) Restate the claim** from the new results.

## Answer

```

The design's Q0124 is re-asked (filed as a new number then) only under §6.6 rule 8, with the measured split.

---

## 14. File ownership

| Package | Files |
|---|---|
| WP0a | `crates/sim_fixtures/src/harness/{mod,corpus,metrics,report}.rs`, `harness/answers.rs` (new), `crates/sim_fixtures/src/trace/{contact_checks,matrix,cli}.rs`, `crates/sim_core/examples/movement_harness.rs` and `matchup_trace.rs` (flags), `crates/sim_core/src/movement/{mod,locomotion,collision}.rs` and `world` accessor (the rotation probe only), `crates/sim_fixtures/tests/{harness_tests,trace_tests}.rs`, `crates/sim_core/tests/contact_solver_tests.rs` |
| WP0b | `docs/evidence/2026-11-27-gated-movement.md` (new); `DESIGN.md` §4 only for the conditional rows |
| WP1 | `crates/sim_core/src/catalog/movement_rules.rs`, `crates/sim_core/src/catalog.rs` (re-exports), `movement/{solver,collision,locomotion}.rs` (the `runs_solver` switches), `target_acquisition.rs` (the same), `assets/catalog/movement_rules.ron`, `crates/sim_core/tests/contact_solver_tests.rs`, evidence, `DESIGN.md` §4, Q0052 |
| WP2 | `movement/gate.rs` (new), `movement/{mod,locomotion,intent}.rs`, `steering.rs` (`SteeringObstacle`), `entity.rs`, `checksum.rs`, `checksum/encoding.rs`, `catalog/movement_rules.rs`, `movement_rules.ron`, `crates/sim_core/tests/{gated_pipeline_tests (new),stutter_step_tests,crowd_edge_tests,checksum_state_tests,golden_replay_tests}.rs`, `fixtures/golden_replay*.ron`, `crates/sim_fixtures/tests/fixture_tests.rs`, evidence, Q0052, Q0122's addendum |
| WP3a | `movement/orbit.rs` (new), `movement/{ring,locomotion,mod}.rs`, `crates/sim_core/tests/gated_pipeline_tests.rs`, evidence |
| WP3b | `movement/{ring,locomotion,mod}.rs`, `target_acquisition.rs`, `entity.rs`, `checksum*`, `crates/sim_core/tests/ring_retarget_tests.rs` (new, from `dc3b62ab`), goldens, evidence, Q0052, `DESIGN.md` §4 |
| WP4 | `assets/catalog/movement_rules.ron` (value), `crates/sim_core/tests/fixtures/matchup_matrix.ron`, goldens and pinned digests, `assets/captures/showcase.ron` (only if moved), `DESIGN.md` (§4, §5, the Movement row), the contact-solver and friend-crowd plans (amendments), Q0113 and Q0118 (resolution addenda), `docs/plans/README.md`, evidence |
| WP5 | as §7 WP5, when decided |

## 15. Material (scratch, not committed)

`~/scratch/gated-movement/`: `scripts/` (arms, checks, the Q0122 rule as code, perf), `trees/<tag>`, `worlds/<arm>`,
`harness/`, `red/` (red runs), `trace/`, `reference/` (the frozen WP4 binary for WP5's decision).

## Amendment 1 (implementation, 2026-11-25): `WP0a` as built

`WP0a` is built on branch `gated-movement-plan` (merged with `main` at `1ea9cf44`, friend-crowd's latent C-3′). No
world changes: the older families' harness CSVs, the mirror, and the matrix's columns up to friend-crowd's are
byte-identical to `main` (§A1.3). The decisions below are the implementation's, recorded under Q0002; none adds a
rule to any pipeline.

### A1.1 Where the code went (§14 extended)

- The eight generators are a child module of `corpus.rs`, `harness/corpus/families.rs`; the answer meters a child of
  `metrics.rs`, `harness/metrics/known.rs`; the permutation mode `harness/permutation.rs`; the clash mode
  `trace/clash.rs` (not `matrix.rs` or `cli.rs`). Each file stays one responsibility.
- Scenes gain `Scenario::script` (orders at set ticks, the stutter family's) and `Scenario::known` (the
  `KnownAnswer`); `Role` gains `Hold`, `Stand`, `AttackMove`, `AttackAt` (named by spawn point, so relabelling keeps
  it) and `Circle`. The older families' commands are issued first and unchanged.
- The probe: `SimWorld::set_movement_order_rotation` and `movement_order_rotation`; one helper, `movement::rotated`,
  used by the movement phase's visit and the contact solver's fill. The three cost probes are one struct,
  `movement::MovementProbes`, read through `SimWorld::movement_probes`, counted per latest phase (cleared when the
  movement phase starts) and summed by the meters; no rule of this tree writes them, so they read 0.

### A1.2 How each known answer is measured (the plan's text, made exact)

- **H-1 / H-2's "proposed no step":** the unit's tick-start state (holding, or attack target in band; for H-2 also
  no order, `Idle` or `Stop`) and the world's `proposed_step` absent or zero; displaced = more than 2⁻²⁰.
  `moved_by_others` and `standing_of` are shared by the harness and the battle meter. **Deferred to `WP1`** (where
  H-1 and H-2 are first judged): the exclusion of overlap present at a pair's first contact.
- **hold-line:** a quarter of priority 20 = every fourth walker in spawn order, same radius; bound = the gap
  family's with one lane and the line's length (holders × pitch) added to the distance.
- **enemy-press:** the idle and holding lines are unarmed (an armed idle enemy acquires and chases, which is its own
  step); the attacking line has range 1, each enemy with an immobile friendly dummy of the attackers' side half a
  unit behind its edge. Attackers' ranges are the corridor set (0.25 to 1). Enemy overlap is the largest over all
  pairs of different owners.
- **deep-queue:** ranks × files on a grid of pitch `2r + 1/4`, front rank three units before the pillar's edge;
  capacity the corridor family's; per-rank overlap attributed to the pair's rear rank, T7 per scene as shares of
  near pair-ticks above D-α's bounds ≤ 0.1 % and maximum ≤ 15 %.
- **wedge:** the gap is the edge distance between the bodies, rounded down to an even number of sixty-fourths;
  "passed" = the walker's centre crossing the segment between the bodies' centres; standing enemies are idle and
  unarmed; bound `2 · 50 / v + 100`.
- **hallway:** width `w` ∈ {2, 3, 4} cells with the larger radius `w / 4`, so two diameters is whole cells; a side
  change = a change of sign of the lateral offset of an east/west pair while their centres are within the radius
  sum along the corridor.
- **stutter:** range ∈ {4, 5, 6}, two friends behind each attacker; cues at `40 + 50c`, the `Attack` `m` ticks
  later. **The verify of §5.2 done:** the scratch archetypes' acceleration (3.136) is above their top speed, so
  `accelerated_speed` makes it instant: the first tick's step is a whole `v`; the bound keeps the plan's
  `0.9 · v · (ticks − 1)`. Lag = the observation of the first step along the order above `v / 8` minus the cue's
  tick (lag 1 = the step of the tick that carried the order). Fire = the first release within one weapon period
  (15) of the first tick in band.
- **surround-moving:** the target's speed is the attackers' times 4 to 7 eighths (0.5 to 0.875, exact decimals for
  the plan's 0.5 to 0.9); the circle is the sixteen-sided polygon of `HEADINGS`; melee ranges 0.125, 0.25, 0.5.
- **lanes:** gaps symmetric about the group's route at ±3 to 6 cells. Clump = area of the convex hull of the discs
  over `n · 2√3 · r²` ≤ 2 (the plan's "verify on main": reported). **A-1's clause is vacuous** in this scene (one
  group, no stranger) and is not metered.
- **Permutation:** compares every entity's life and position each tick; a first difference after a tick that began
  with two coincident living centres is a tie, anything else unexplained. I-3's equal roots are not visible from
  outside the world, so they count as unexplained until `WP2`, which judges H-3, adds what it needs.

### A1.3 Evidence (scratch, `~/scratch/gated/wp0a/`)

- **Red first** (`red/`): the new tests against `bf58581d`'s sources fail to compile (28 errors in
  `harness_tests`, 1 in `trace_tests`, 3 in `contact_solver_tests`: the missing families, roles, meters and probe);
  then by mutation, each pinned rule broken in turn fails its test (W0-T2 to W0-T8, W0-T10, the per-rank, side-change
  and old-header tests, the solver-fill wiring). W0-T9 is red only for a setter that writes world state: both shipped
  pipelines play identically under any rotation (below), so offset 0 cannot be told from another by outcome, and the
  probe's wiring is pinned directly (`the_order_rotation_rotates_the_solver_fill`).
- **Identity** (`identity/`, release, exported trees): against `main` `1ea9cf44`, the harness families gap, arc,
  cross, corridor, corner (200 seeds, 50 relabelled) and mirror (20) under `Solver` and `Legacy` give 22 CSVs
  byte-identical, and the whole matrix on its seeds of record plus 4 (488 runs) is byte-identical in its first 51
  columns (all but `WP0a`'s 13). Rerun after merging `main` `8dde83cd` (`identity2/`: 50 seeds, 20 relabelled, 10
  mirrors, the matrix's seeds of record, 244 runs): again 22 CSVs and the matrix's 51 columns byte-identical.
- **Permutation, offset 7, `Solver`** (`figures/permute/`): 50 scenes of every family and 10 mirrors, all
  `identical`: no order dependence is visible in the shipped pipeline (B-224's condition not met on this sample).
  `Legacy` gave the same on the 4-seed spot check.
- **The new families, seeds 1 to 200, reported, not judged** (share of scenes whose answer held; hard-part
  failures; failing parts):

| Family | S (`Solver`) | L (`Legacy`) |
|---|---|---|
| hold-line | 3.0 %; hard 0 (no holder moved); give-ups and incomplete in 194 | 2.0 %; hard 0; 196 |
| enemy-press | 13.0 %; hard 64 (enemies moved 64, enemy overlap 30); friend overlap 173 | 0 %; hard 72 |
| deep-queue | 53.5 %; fill 67, M1 31, rank overlap 4, M3 2 | 0 %; rank overlap 200, M1 199, M3 177, fill 44 |
| wedge | 0 %: friends: the walker gives up (101); enemies: it passes between them, pushing them apart (99) | 0 %; passed 139, overlap 135, incomplete 61 |
| hallway | 100 % | 100 % |
| stutter | 100 % (lag 1, displacement and fire everywhere) | 0 %; hard 200 (displacement short) |
| surround-moving | 0 %; fill 184, M1 194 (5.9 per 1,000) | 0 %; fill 157, M1 198 |
| lanes | 2.0 %; one gap only in 187, clump 21 | 2.0 %; 195, 6 |

  M1 per 1,000 moving unit-ticks under S: hold-line 28.2, enemy-press 10.9, deep-queue 93.9, stutter 49.4 (its
  ordered reversals), surround 5.9, lanes 1.3, wedge and hallway 0. H-1's held-moved unit-ticks under S:
  enemy-press 108,057, surround-moving 179,749 (D-δ's nudges, as §6.2 expects of S), deep-queue 3, stutter 74.
  The R arm (C-3′ on) is not run here: the harness selects a pipeline, not a crowd cap; that is `WP0b`'s arm.
- **The clashes** (`figures/clash/`, 400 units, 300 ticks, seeds 7 and 1 to 4, `Solver`): the friend overlap of
  `pulse_bot` reaches 43.5 to 49.1 % (corrected pairs the same), 58,042 to 60,811 pair-ticks above D-α; of the
  zergling analogue 13.4 to 15.7 %, 2,624 to 3,824. M1 6.5 to 8.8 (`pulse_bot`) and 0.5 to 1.3; H-1 held-moved
  unit-ticks 267 to 525 and 2,502 to 2,917 per side. Probe counters 0 everywhere.
- Every new-family scene played without a simulation diagnostic.
