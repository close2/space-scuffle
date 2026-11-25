# Q0118: Crowds overlap far more than the 5 % you allowed. Pay for the solver to finish its work, keep it as it is, or change how crowds push?

- **Asked:** 2026-11-26
- **Blocking:** only the switch-over (WP2) of plan `2026-11-26-friend-crowd-overlap`, and through it the resumed
  "enemies block" (Q0113) and collision-classes (Q0117) packages. Measurement and latent code continue.
- **Default until answered:** (b), today's movement stays; the fix is built switched off.

## Background

You allowed friendly units to overlap by up to 5 % of their size while moving (deviation D-α). In big crowds they
overlap much more: a hundred zerglings attacking six colossi reach 39 % (812 pairs above 5 % per run), and the
400-marine benchmark reaches 47 %. The cause is a defect, not a rule: the movement solver stops after 21 rounds per
tick, and a crowd ten units deep needs 50 to 170 rounds for the push from the front to reach the back. Two parked fixes
(enemies that block, zerglings under the colossus) stopped on exactly this.

## Options

- **(a) Let the solver finish (recommended).** It keeps working only while some friendly pair is above 5 % and it is
  still making progress. Battle 26: 812 → 11 pairs above 5 % per run, worst 39 % → 7.6 %; nothing else about
  movement changes, and every situation that is fine today stays bit-for-bit the same. Cost: about +50 % to +60 %
  instructions on the two 400-unit battle benchmarks (64 × 64 and canyon), none elsewhere; projected about 70 and
  53 ticks per second, above your 50 floor, canyon with little margin.
- **(b) Keep it as it is (default).** Crowds keep overlapping; the defect is registered (B-219); enemies-block and
  collision classes stay parked.
- **(c) Change how crowds push (cheap).** A unit standing behind a friend that cannot give way may no longer push
  that friend; it takes the whole correction itself ("shock propagation", standard in physics engines). Overlap
  falls by about 90 % and the benchmark gets 10 % cheaper, but crowds stop pressing their front line aside, which
  looks more like StarCraft II units queueing behind a full surround, and moves results: battle 26's zerglings go
  from winning 9 of 12 runs to 2, and units in one test family turn back and forth more than twice as often.

Recommendation: (a), since the cost stays above 50 ticks per second and nothing but the broken crowds changes; (c)
if you prefer the queueing look and a cheaper solver over the battle claims.

## Answer

- **Option (a): Let the solver finish** (keep iterating only while a friendly pair exceeds 5 % and progress is being made), keeping non-crowded cases bit-for-bit unchanged and watching canyon's `~53` TPS headroom against the `50` TPS floor.

### Owner Strategic Guidance: Consider a Blank-Sheet Movement Design Round

The owner notes that it may be a good idea to **use a dedicated design/research round to take a step back and re-examine the movement algorithm from a blank sheet**, rather than continuing to layer incremental patches onto the current Jacobi overlap solver.

**Owner preferences for any clean-sheet design:**
- **Hold Position and Attacking units should NOT be pushable** (reverting the assumption behind `D-δ` if a cleaner surround/steering design handles corridors and surrounds without pushing engaged units; enemies also do not push each other).
- **Open to slight deviations from StarCraft II** where another algorithm is cleaner, faster, or produces more natural/expected movement (StarCraft II solved this very well, so its architecture is a strong baseline, while academic/game crowd algorithms may offer an even better fit).

### Research Summary: StarCraft II & Alternative Crowd/RTS Movement Algorithms

Why the current solver hits `50–170` rounds on 10-deep crowds: a **Jacobi** position-based overlap solver diffuses pushes at $O(k^2)$ iterations for a crowd $k$ units deep. Trying to enforce `<5 %` hard overlap in one tick with Jacobi turns dense melee fights and immovable boundaries (`Q0113` enemies-block) into a stiff boundary-value problem. Below are the main alternatives from shipped RTS games and crowd-simulation literature to weigh in a blank-sheet round:

1. **StarCraft II's Layered Steering + Soft Separation + Push Priority** (*James Anhalt, GDC 2011, "AI Navigation: It's Not a Solved Problem—Yet"*):
   - **Global:** Constrained Delaunay Triangulation (CDT) NavMesh + A* + funnel/string-pulling with radius-aware clearance tunnels (`InnerRadius` for terrain/structures, `Radius` for unit separation).
   - **Local steering & avoidance (not a 100-round hard contact solver):** Targeted side-avoidance with **side-locking** (reading oncoming unit intent and locking left/right bias while avoiding to prevent "hallway dance" oscillation), plus **soft multi-tick separation forces** rather than forcing `<5 %` overlap in a single tick.
   - **Surround & unique arrival points:** Units never share one exact target coordinate; approaching units steer tangentially around occupied front ranks to find open space on the target's perimeter (preventing `B-192`-style stalled rear ranks naturally).
   - **Push hierarchy:** Moving allies displace idle/lower-priority allies (`PushPriority`); **Hold Position and Attacking units are immovable**; **enemies never push each other** (they block and induce tangential sliding/steering).
   - *Trade-off:* $O(1)$ per tick per unit, battle-tested feel for SC2 micro; requires careful tuning of separation + tangential sliding so crowds don't compress excessively or stall at chokes.

2. **Position-Based Crowds with Gauss–Seidel + Anticipatory Constraints** (*Weiss et al., Computers & Graphics 2019, "Position-Based Real-Time Simulation of Large Crowds"*; *Macklin et al. 2014/2016 XPBD*; *Shock Propagation*):
   - Replacing Jacobi with deterministic **Gauss–Seidel** (sequential in-place updates in a fixed deterministic order or graph-coloured passes) propagates crowd pushes in $O(k)$ instead of $O(k^2)$ (typically 3–5× fewer iterations).
   - Pairing PBD with **long-range predictive/TTC avoidance** prevents units from slamming 10-deep into a front line in the first place.
   - **Shock propagation** (one-way ordering outward from immovable anchors: walls, enemies, Hold Position, Attacking units) resolves deep queues in 1–2 passes, *provided* rear units also have tangential surround steering so they flow around blocked allies instead of queuing passively (avoiding `Q0118 (c)`'s Battle 26 regression).

3. **Context Steering (Interest / Danger Maps)** (*Andrew Fray, Game AI Pro*):
   - Each unit evaluates 8–16 directional bins with **Interest** (toward goal, open target flank, path tangent) minus **Danger** (walls, enemies, immovable attacking/holding allies, oncoming collisions) and picks the best unblocked direction.
   - *Trade-off:* Very cheap fixed $O(1)$ CPU cost, deterministic, zero vector-cancellation deadlocks, and naturally makes rear melee units flow around immovable front-rank allies to surround an enemy; still pairs with a lightweight clamp/separation pass to keep overlap bounded.

4. **Anticipatory Power-Law Steering & Implicit Crowds** (*Karamouzas, Skinner & Guy 2014, "Universal Power Law Governing Pedestrian Interactions"*; *Karamouzas et al. 2017, "Implicit Crowds"*):
   - Uses Time-to-Collision (TTC) inverse-power-law potentials for smooth, memoryless anticipation (eliminating sticky side-choice bugs).
   - *Implicit Crowds (2017)* integrates those nonlinear anticipatory forces via implicit optimization (a few Newton/L-BFGS steps), staying unconditionally stable and jitter-free at large time steps (`40 ms` / 25 TPS) and high density.
   - *Trade-off:* Higher per-step math complexity than steering/Context Steering, though fixed-iteration cost; needs fixed-point adaptation for lockstep determinism.

5. **Aggregate / Continuum Hybrid — Unilateral Incompressibility** (*Narain, Golas, Curtis & Lin 2009, "Aggregate Dynamics for Dense Crowd Simulation"*; *Golas et al. 2014*):
   - Combines discrete agents with a coarse grid enforcing **unilateral incompressibility** (a pressure solve capping maximum unit density so excess flow automatically diverts sideways around chokepoints and immovable front lines).
   - *Trade-off:* Cost scales with coarse grid cells rather than $O(k^2)$ pair iterations, preventing deep overlap spikes in 400-unit clumps and producing organic flowing surrounds; needs discrete local steering/clamping for tight 1v1 micro and exact radii.

6. **Flow Field Tiles + Local Separation** (*Treuille et al. 2006, "Continuum Crowds"*; *Elijah Emerson, Game AI Pro 2013* — *Supreme Commander 2*, *Beyond All Reason*):
   - Cached tiled flow fields with dynamic congestion costs route rear units automatically around clogged fronts in $O(1)$ per unit, paired with lightweight separation.
   - *Trade-off:* Scales effortlessly to thousands of units; still needs targeted per-unit micro steering for SC2-style focus-fire, stutter-stepping, and tight melee surrounds.

7. **Velocity Obstacles (RVO2 / ORCA)** (*van den Berg et al. 2008/2011*):
   - Already known from `VO-4` and `Q0097` Update 2 to deadlock in corners, narrow chokes, and against immovable obstacles/attackers; generally inferior to hybrids above for SC2-style combat.

## Update (implementation agent, 2026-11-25): (a) measured, stopped for review

The switch-over to option (a) was measured against its pre-registered checks (plan
`2026-11-26-friend-crowd-overlap` Amendment 1, evidence `2026-11-26-friend-crowd-overlap` §5). It does what it was
for: friendly pairs above 5 % fall by 99 % in the four big zergling battles (battle 26: 1,008 → 10 per run, worst
43.6 % → 7.9 %), and nothing changes where crowds stay within the bound. Cost: +47 % on the 64 × 64 benchmark, +43 %
on canyon (about 56 ticks per second projected; it could not be measured on a quiet host). But the plan's stop rules
were hit: battle 27's worst overlap stays at 11 % (above the 10 % target), zerglings in battle 27 stand with room
more often, and battles not on the close list (29, 30, 34) change results within their noise. So the change is
built but switched off, and is waiting for review; no answer is needed from you unless you want to waive those
checks. Your blank-sheet guidance above is noted for the next movement round.

## Resolution

Answered (a). Plan `2026-11-26-friend-crowd-overlap` built it: WP0 (the measurement repair) and WP1 (C-3′, with its
cap `crowd_iterations` 151 derived by P-1) are merged, but **latent**: the shipped `movement_rules.ron` keeps the
extension off, so no world changes. The switch-over (WP2) stopped on its pre-registered checks (update above, evidence
§5): it removes 99 % of the friendly overlap, but battle 27 stays at 11 % against the 10 % target, a few behaviour
checks miss narrowly, and three non-close battles move within their noise. The orchestrator did not waive the checks.
Following the owner's strategic guidance and Q0120 (a), the switch is decided inside the movement redesign (plan
`2026-11-27-gated-movement`, being written), which takes C-3′ switched on as its cost baseline (Q0124 (a)) and judges
it together with the new contact rules. B-219 stays open until then.
