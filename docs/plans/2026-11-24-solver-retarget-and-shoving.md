# Blueprint: Retargeting Round a Full Ring and No Shoving of Enemies under the Contact Solver (B-192, B-193)

This plan is the dedicated follow-up that the owner's answer to
[Q0103](../questions/open/0103-solver-matrix-claims.md) (battle 28, option (b)) asks for. It fixes two defects of the
contact-solver pipeline (plan [`2026-11-20-contact-solver`](2026-11-20-contact-solver.md), hereafter "the solver
plan"):

- **B-193, enemy shoving.** A crowd of melee attackers pushes the enemies it is fighting. The solver splits every
  enemy overlap evenly (C-2's "enemies ½ / ½", C-8), so a crowd's press reaches the enemy through its front rank.
- **B-192, standing behind a full ring.** An automatically acquired chase whose target's ring has no free angle
  stands (G-6). It goes on standing even while another enemy within its scan range has room (R10). Battle 28 of the
  matchup matrix flips because of it.

**Two changes, in this order:**
- **E-1, enemies never push each other (B-193).** In an enemy pair, each body takes the share of the overlap that its
  own displacement this tick closed. A body that has not moved towards the other takes none. This puts into the
  solver a StarCraft II fact the project already records (§2.2), which the owner also restated in Q0095: "never
  pushed by enemies".
- **G-6′, retarget from a full ring (B-192).** The stopped step of the solver plan's Amendment 17 §17.3, on branch
  `contact-solver-g6p-wip`, is ported onto E-1. Its acquisition refactor is made cost-neutral. It is judged under the
  owner's new standing guidance (AGENTS.md §8, Q0103): a close battle that moves is restated, not treated as a stop.

**Status of this document.** This is a Stage 1 blueprint under charter §6. It was written on 2026-11-24 on branch
`solver-followup-plan`, cut from `contact-solver-plan` at `a2b420c9`. That branch is the solver plan's head (Amendment
19, checksum schema 40 on that branch). It is not merged to `main` yet; it merges after fire-rules. `main` is at
`35793ef9`, checksum schema 44.
- **Line numbers drift,** so search for the quoted symbol.
- **Claims not confirmed while planning** are marked **verify**.
- **Later decisions** are appended as `## Amendment N (role, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** It changes a mechanic (contact shares and acquisition), moves battle outcomes and
the golden replays, changes the checksum format (one hashed memory field), and spans two packages.

**Numbers.** Bug rows start at **B-202** and question numbers at **Q0110**. Both are **provisional, next free at
merge**: the orchestrator renumbers them if `main` has taken them by then. The checksum schema is also "next free
at merge". `main` is 44, fire-rules takes 45 and the solver merge the next free one (46 if nothing intervenes), so
this plan's one bump is provisionally **47**.

**Inputs read for this plan:**
- The solver plan, its Amendments 17, 18 and 19, and §3.3 (C-1 to C-8) and §3.4 (G-1 to G-7).
- Evidence [`2026-11-20-contact-solver.md`](../evidence/2026-11-20-contact-solver.md) §13.12 and §13.13.
- The diff `contact-solver-plan...contact-solver-g6p-wip` (`2cfb880f`; 14 files, +1,473 / −110).
- `DESIGN.md` §4 rows B-188 to B-193.
- Q0052 (with the addenda for D-α to D-δ), Q0093, Q0095, Q0102 and Q0103.
- Plan [`2026-11-09-friendly-blocking`](2026-11-09-friendly-blocking.md) §1 (StarCraft II facts 1 to 10).
- The code at `a2b420c9`: `movement/solver.rs` (`contact_shares`, `approach_shares`, `ContactSolver::solve`,
  `accumulate`, `apply`), `movement/ring.rs`, `movement/locomotion.rs` (`prepare_pursuit`),
  `target_acquisition.rs` and `sim_fixtures::matchup` (`Battle::verdict`).

---

## 1. Problem statement and evidence

### 1.1 B-192: zerglings stand behind a saturated front column

**Scene.** Battle 28, `28_100_lings_vs_10_immortals`: 100 zerglings attack-move into 10 immortals that stand in two
touching columns.
- Under the solver the zerglings win 5 of 24 runs. The four seeds of record plus seeds 1 to 20 are the "24 seeds"
  throughout this plan.
- Under the legacy pipeline they win 24 of 24 (evidence §13.12).

**Mechanism (evidence §13.12, corrected by solver-plan Amendment 17 §17.1):**
1. Every zergling acquires the nearest immortal, which is in the western (near) column.
2. Each western ring saturates: zerglings on the west face, the touching neighbours to north and south, and the
   eastern twin behind.
3. The ring rule answers `RingAim::Stand`, so the zergling stands (G-6), keeps its order and accrues no stall.
4. A standing zergling never comes nearer to another immortal, so acquisition's nearest-first key never switches it
   (`target_acquisition.rs` invariant 3: a target out of reach is replaced by the best candidate by the whole key,
   and the saturated one is still the nearest).
5. The eastern immortals' rings keep 98° to 166° free each, on the far side, for the whole fight.

| Seed 1 of battle 28, whole battle | Legacy | Solver |
|---|---:|---:|
| zergling-seconds blocked | 105 | 678 |
| zergling-seconds flanking | 437 | 44 |
| target switches | 491 | 40 |

| Zerglings in contact / blocked (mean of 24 seeds), tick | 150 | 250 | 350 |
|---|---|---|---|
| Legacy | 14.4 / 10.8 | 24.5 / 5.7 | 22.4 / 3.2 |
| Solver | 11.9 / 58.1 | 12.2 / 46.0 | 12.4 / 28.6 |
| G-6′ (stopped step, `contact-solver-g6p-wip`) | 28.2 / 8.2 | 46.8 / 4.4 | 25.7 / 10.7 |

**What G-6′ did (evidence §13.13):**
- Battle 28 went back to zergling wins on 24 of 24 seeds.
- Every scene family of the harness stayed byte-identical. Those families have no automatic chase.
- The mirror's T9 held: 0.568 [0.478, 0.654] against 0.538 at the head.
- D-R10 fell from 7,761 unit-ticks in 215 scenes to 5,993 in 207.

**Why it was stopped.** Battles 33, 38a and 38b left their recorded ranges on seeds of record, which §17.3's stop
rule made a stop for review.

| Battle | Range | G-6′ on seeds of record | All 24 seeds |
|---|---|---|---|
| 33_24_marines_vs_12_zealots | blue 0..16, red 0..3 | seed 33: 5 zealots left | zealots win 21 (head: marines all 20) |
| 38a_fungal_turns_zerglings | blue 3..8 | 3203386110: 10 zerglings left | 7 of 24 within range |
| 38b_marines_beat_zerglings_without_fungal | blue 0..1, red 4..8 | 3203386110: 2 marines left | zerglings win 9 |

**Owner standing guidance** (AGENTS.md §8, commit `35793ef9`; Q0103):

> Good, natural pathing and movement take priority over preserving the exact winner of a close matchup … record it
> as a minor deviation and restate the claim … Treat a matchup shift as a bug only when the underlying unit behaviour
> itself is broken or unexpected (e.g. units stalling instead of engaging reachable targets).

Q0103's standing paragraph names battles 33, 38a and 38b under D-ε as examples of close battles to restate. It
names B-192's stalled zerglings and B-193's shoving as broken behaviour. This plan's matrix acceptance (§6) applies
that guidance with a pre-registered test of "broken".

### 1.2 B-193: a melee crowd shoves the enemies it fights

**The metric** (evidence §13.13): distance moved while in contact with an enemy, summed over the engaged ranged side.

| Measurement | Legacy | Solver (head) | G-6′ |
|---|---:|---:|---:|
| immortals, battle 28, seed 1 | 0.85 | 3.61 | 94.6 |
| colossi, battle 26, seed 3203386110 | 0.45 | 8.32 | 44.3 |
| red unit-ticks pushed in contact, battle 28 (§13.12) | 3 | 131 | — |
| red unit-ticks pushed in contact, battle 26 (§13.12) | 4 | 195 | — |

**Mechanism, read from the code at `a2b420c9`:**
- `contact_shares` gives every enemy pair `Weights(½, ½)` unless one side is holding or fixed.
- `ContactSolver::solve` then runs the averaged iterations and a summed hard pass. In that pass the enemy pairs
  (`Pass::EnemyAndFixed`) have the last word, unbounded.
- **Three ways in.** Overlap with an enemy arises from (a) a zergling's own committed pursuit step that overshoots
  into the enemy disc, (b) a non-engaged zergling pressed forward by moving friends (D-β), and (c) a D-δ nudge along
  one ring that runs into a neighbouring enemy.
- **Each way moves the enemy.** The even split moves the enemy by half of each overlap, every tick, from every side
  that presses. A one-sided crowd therefore walks its enemy backwards, which is the 3.61 under the head.
- **Why G-6′ made it much worse.** G-6′ fills the rear rings. The press then comes from all sides, but it is not
  symmetric: some rings are full and some still filling. The net push is large (94.6).
- **Legacy was not shove-free either.** Its "enemies separate evenly and uncapped" collision pass is the same rule,
  but its waiting rule kept most bodies from stepping into enemy discs. That gave 0.85.

**Why it matters beyond battle 28:**
- It is a StarCraft II difference (§2.2).
- It breaks the owner's Q0095 condition for D-δ: an engaged unit is "never pushed by enemies".
- It moves fights in ways no player ordered: a shoved immortal or colossus leaves its own allies' firing positions
  and opens or closes rings.
- It was a confound in the G-6′ stop. The surround that G-6′ creates is the press that shoves, so part of battles
  33, 38a and 38b moving may be the shoving and not the retarget. That is why E-1 comes first.

---

## 2. StarCraft II research (patch of record 5.0.14)

### 2.1 Does a melee unit re-acquire when its automatic target has no free contact room?

**What is documented:**
- **[Liquipedia, Automatic Targeting](https://liquipedia.net/starcraft2/Automatic_Targeting)**, the source of
  `target_acquisition.rs` invariant 3. A unit keeps a valid target in reach unless a higher-ranked candidate (threat,
  then attack target priority) appears. A target that is lost or out of reach is replaced by the best candidate
  under the whole key, including the closeness tie-break. The page says nothing about a target whose contact surface
  is full. It refused a fetch today (HTTP 429), as it did for Amendment 17, so the code's reading is relied on.
- **[Blizzard, Game Guide: Unit Positioning](https://news.blizzard.com/en-us/article/6640646/game-guide-unit-positioning)**
  (SC-10 of the solver plan):
  - "minimize the number of idle units behind the curve";
  - "if too many units are trapped outside of the concave, select the units and move them slightly forward before
    attack-moving once again";
  - zealots win by "preventing the zerglings from surrounding" them.

  So the game can leave units idle behind a full front, and its zerglings surround by default when there is room.
- **[Liquipedia, Micro](https://liquipedia.net/starcraft2/Micro_(StarCraft))**, on surrounding: players should not
  order melee units to attack the frontmost units, "in which case they will get in one another's way". They first
  move them through the enemy and then attack. This concerns explicit orders. G-6′ already leaves explicit `Attack`
  orders alone (test 2 of §17.3).

**Community accounts (secondary, not settling):**
- SC2Mapster threads describe zerglings going idle at a wall, and a trigger workaround that re-issues attack orders
  to idle units.
- Forum accounts from other games (Pillars of Eternity, Obsidian forums) describe melee units that "do not search
  for a new target until they get within a very short range" and so stand idle behind blockers. They show only that
  the defect is common to the genre's simplest rule.

**Finding.**
- No primary source states whether 5.0.14's automatic acquisition skips a target with no free contact room.
- The observable outcome the game's guides describe is that attack-moved zerglings surround clumps and that some
  units can idle behind a full curve. That is consistent with "acquisition by closeness, plus movement that slides
  units round the crowd edge". It is also what the legacy pipeline's crowd-edge walk did.
- G-6′ reaches the same outcome by a different mechanism (acquisition consults ring room).

**Confidence:**
- *Low* on the game's mechanism.
- *Medium* on the outcome: zerglings attack-moved into a two-column clump spread to the rear column.

G-6′ is therefore recorded as **deviation D-ε, a minor deviation that allows more, behind a data switch**
(`contact.retarget_from_full_ring`, on by default), and confirmed with the owner by draft Q0110 (§12).

### 2.2 Can units push enemy units?

**Documented by Blizzard** (already recorded as fact 3 of plan `2026-11-09-friendly-blocking` §1):
- **Patch 1.2.0** (January 2011): "Fixed an issue where units could push opposing units when they shouldn't have
  been able to." It is reproduced by, among others,
  [God is a Geek](https://godisageek.com/2011/01/starcraft-ii-patch-1-2-0-hits-live-servers/) and the
  [StarCraft Wiki patch 1.2 history](https://starcraft.fandom.com/wiki/StarCraft_II_version_history/Patch_1.2). No
  later patch note reverses it.
- **Patch 1.3.0** added the trigger aspect `AlliancePushable`, "whether a player's units will move through another
  player's units, pushing them out of the way" (Liquipedia patch page, read for the friendly-blocking plan). Pushing
  across players is opt-in map scripting, off in melee.
- **Patch 5.0.14:** the Ultralisk's change is to **allied** push priority, 0 → 1 ("now pushes allied units out of
  the way when moving", fact 1). The game's data has two fields, `PushPriority` and `AlliedPushPriority`. Neither
  concerns enemies.

**Community (secondary):**
- An SC2Mapster data tutorial says that opposing players "can not push each others' units at all".
- An SC2Mapster answer says there is "no way to use the default push priorities to push enemy units" and suggests a
  force effect instead.
- One thread claims the contrary for unequal priorities. It is not supported by the patch notes, and it predates or
  ignores 1.2.0. Sources:
  [SC2Mapster 187903](https://www.sc2mapster.com/forums/development/data/187903-unit-bumps-into-oncoming-unit-making-it-stop?comment=1),
  [SC2Mapster 174315](https://www.sc2mapster.com/forums/development/miscellaneous-development/174315-resolved-stop-units-from-bumping-eachother?comment=3).
- The surround as a tactic presupposes it. A unit ringed by zerglings cannot walk out
  ([Liquipedia, Micro](https://liquipedia.net/starcraft2/Micro_(StarCraft)); the Unit Positioning guide treats
  being surrounded as fatal). So neither side moves the other by pressing.

**Owner:** Q0095's answer, "never pushed out of attack range and never pushed by enemies".

**Finding.** In 5.0.14 melee play, no unit moves an enemy unit by contact. Enemy bodies block each other, and the
body that walks into an enemy is the one that is stopped. Confidence: **high** for "enemies are never pushed"
(Blizzard patch notes plus game data fields). It is **medium** for exactly how a contact between two enemies that
are both moving is shared; no source describes it.

**Consequence.** C-8's "enemies separate evenly" is a difference from the game that the owner never sanctioned, so
it is a bug (charter §8), as B-193 already says. E-1 removes it. E-1 needs no owner question. Its data switch
exists for the bit-identity proof and for review, as D-ε's does.

---

## 3. Algorithm survey (brief)

### 3.1 Retargeting of melee crowds

| Approach | Where seen | For | Against |
|---|---|---|---|
| **R-a. Greedy nearest-first acquisition, re-scanned** | StarCraft II (Liquipedia, Automatic Targeting), Brood War; our acquisition | Simple, local, deterministic | A saturated nearest target holds the unit for ever if nothing else moves it (R10) |
| **R-b. Movement carries the unit round the crowd edge; acquisition follows** | Our legacy pipeline (crowd-edge walk, plan `2026-11-19-crowd-edge-steering`); plausibly the game's outcome (§2.1) | Matches what players see | A walk-mode state machine of six fields; the solver plan replaced it deliberately (D-γ). The detours P2 and 2′ that would rebuild it were stopped by the step rule (evidence §13.3, §13.5) |
| **R-c. Capacity-aware acquisition: skip a target with no free slot** | G-6′ (solver plan §17.3); "attack slot" or "token" schemes of action-game AI are the same idea | One rule; reuses the ring rule (P8 cache) and acquisition's key; no constant | A new mechanic the game does not document (D-ε) |
| **R-d. Crowding penalty in the score** (attackers already on a target lower its rank) | Common in action and RTS AI | Spreads attackers early | A weight to tune (forbidden: "no parameter chosen after seeing figures"); changes who is attacked even when rings have room, which the game does not |
| **R-e. Global assignment** (Hungarian method; Bertsekas' auction algorithm, *Annals of Operations Research* 14, 1988) | Formation and slot assignment in academic multi-agent work | Optimal spread | Couples all units in one solve, so it costs more and is not the game's per-unit behaviour; ties and determinism are harder |
| **R-f. Flow fields towards enemies** (Emerson, "Crowd Pathfinding and Steering Using Flow Field Tiles", *Game AI Pro*, 2013; Supreme Commander 2) | Large-crowd RTS | Crowds flow round obstacles | Replaces the global layer; out of proportion to a targeting defect |

**Chosen: R-c (G-6′).**
- It is the smallest rule that removes R10.
- It reuses two tested pieces: the ring rule's admissibility and acquisition's key.
- It adds no constant.
- G-6′'s tests already passed (evidence §13.13).

R-b is ruled out by the solver plan's own history. R-d and R-e would choose targets differently from the game even
when every ring has room.

### 3.2 Enemy contact under a position-based solver

| Approach | Where seen | Behaviour |
|---|---|---|
| **P-a. Even split** | Our C-8; our legacy collision pass | The press is passed on: whoever presses harder walks the other backwards (B-193) |
| **P-b. Inverse-mass weights with infinite mass for "static" bodies** | Position-based dynamics (Müller, Heidelberger, Hennix, Ratcliff, "Position Based Dynamics", *JVRCI* 2007): a body of inverse mass 0 is never moved by a constraint; our C-2 uses this for holding and fixed bodies | Correct for bodies that never move; but which enemy is "static" changes tick by tick |
| **P-c. Kinematic mover carries the constraint**: the body that closed the gap this tick is corrected, the other not | Character controllers against other characters (kinematic against kinematic: each mover resolves its own penetration); our D-β-off rule `approach_shares` for friends (SC-8, "moving units block") | Nobody pushes: a walker into an enemy is stopped; a standing enemy stays; two walkers into each other share in proportion to their own approach |
| **P-d. Push priority across alliances** | SC2's `AlliancePushable` (opt-in map scripting only) | Not melee 5.0.14 |

**Chosen: P-c, as E-1.**
- It is exactly "enemies are never pushed" (§2.2) and is already a tested function (`approach_shares`).
- It reduces to P-b for a standing body.
- It gives a symmetric clash of two advancing lines the even split it has today, so head-on fights keep their
  symmetry (T9).

---

## 4. Design

### 4.1 E-1: enemies yield by their own closing (B-193)

**Rule.** For an enemy pair `(a, b)` of the Solver pipeline, the rows of C-2 for fixed bodies and for holding bodies
keep precedence, unchanged. The row "enemies, ½ / ½" becomes **"enemies, by closing"**:

```text
closing_a = max(0, −(x_a − s_a) · n)       n = (x_a − x_b) / |x_a − x_b|, from b to a
closing_b = max(0,  (x_b − s_b) · n)
(σ_a, σ_b) = (closing_a, closing_b) / (closing_a + closing_b)    if the sum > 0
           = (½, ½)                                               otherwise
```

- `x` is a body's position at the **start of the current iteration**, not its committed step. A body pressed into
  an enemy by its own friends during the solve has closed the gap as surely as one that stepped into it. It is the
  one that yields, and it ends the pass overlapping the friend behind it, within D-α, rather than shoving the enemy.
- `s` is the tick-start position.
- This is `approach_shares(n, x_a − s_a, x_b − s_b)` (`solver.rs`), already Q32.32 and already tested for D-β off.

**Held bodies.** A body whose terrain step was refused in the previous iteration (`BodyStep::held`) counts as fixed
in its enemy pairs for the current iteration: its partner takes 1. This is C-6's existing reading ("a body against a
wall acts as a fixed body after one iteration") made explicit for a pair in which the partner would otherwise take
0. Without it, a zergling pinned on a wall while overlapping an immortal would keep the overlap. That would break
D-α's enemy bound, which T7 checks. **Verify** with test E-T5.

**Neither closed (`sum = 0`).** This is an overlap carried from an earlier tick, which can only be a terrain-held
residual, a spawn or an unload. The even split stays here. It is the only case in which a body can be moved by an
enemy. It moves it by at most a residual that D-α bounds. WP0's D-SHOVE measures how much of the total it is
(§5.1).

**Passes.** The same shares apply in the averaged iterations and in the summed `EnemyAndFixed` pass. They are
recomputed from the pass's starting positions. The pair keeps no share field for enemies: `pair_correction` computes
it. The correction cache (`Pair::cached`) stays valid, because the shares depend only on the two bodies' positions
and tick-start positions, and the cache is reused only when neither body moved.

**Order independence (C-1a).** The shares are a function of the pair's two bodies at the iteration's start. The sum
over pairs is exact integer addition. The result is still a function of the set of bodies.

**Unchanged:**
- Hold Position, immobile and unpushable bodies (rows before).
- Friend rows, D-α, D-β, D-δ (friend pairs only).
- The O1 step bound and the dead band.
- The legacy pipeline.

**Switch.** `contact.enemies_push_each_other` in `movement_rules.ron`, documented in the file's header comment:
- `false` is E-1;
- `true` is today's even split;
- absent means `false`.

It is validated at load like the other contact fields, and it exists for the bit-identity proof (E-T4) and for
review. It is not a tuning knob, and it names no unit.

**What it is not.**
- It is no rule about "engaged" or "attacking" enemies. A chasing or kiting enemy is not pushed either. The game's
  rule is about players, not orders (§2.2).
- It does not touch acquisition or the ring rule.

**Expected effects** (stated before measurement; the measurements in WP1 decide):
- Battle 28's immortals' D-SHOVE falls to Legacy's level or below.
- Zerglings that press into an enemy stop at its disc and overlap their friends behind them transiently. D-α
  bounds this, and T7 measures it in the harness.
- A unit ringed by enemies on a `Move` order is trapped (the surround). This is the game's behaviour, and its
  give-up after 10 s (SC-8 fact 9) is the existing rule.
- The harness's scene families have no enemies except the arc, corridor and mirror families. Gap, cross and corner
  are expected to be byte-identical.

### 4.2 G-6′: an automatic chase at a full ring takes the next enemy with room (B-192)

The rule is the solver plan's Amendment 17 §17.3 unchanged in behaviour, ported from `contact-solver-g6p-wip`
(`2cfb880f`):

**Trigger and pick:**
- **Trigger:** `RingAim::Stand` in `prepare_pursuit` for an `AttackTarget` with `AttackOrigin::Acquired`, switch on.
- **Candidates:** every enemy that `scan_candidates` would admit (visibility, targetability, weapon against plane,
  flags, minimum range, `abandoned_target`, scan range), in `TargetPriorityKey` order, the current target left out.
  The test is evaluated at **tick-start positions** with the phase-4 spatial hash.
- **Pick:** the first candidate on which the ring rule does not stand. That is `Gap`, or `Straight` with the unit's
  bearing point free and admissible or the unit already in band. `RingAim::Unruled` (the bearing point is beyond the
  leash or inside terrain) never qualifies. If none qualifies, G-6 stands as before.

**State:**
- `MotionProposal::ring_retarget: Option<EntityId>`. On commit it rewrites the front acquired attack in place, drops
  the route, and restarts the memory while keeping `leash_origin`.
- `MovementMemory::ring_retarget: Option<EntityId>`, hashed (schema, §8). Acquisition keeps that target unless a
  candidate **outranks** it (`rank()`, not the tie-break), so phases 4 and 5 do not fight.

**Never retargeted:** explicit `Attack` orders, Hold Position, tracking weapons (`Engagement::Track`), defensive
acquire levels beyond what acquisition already allows, and the legacy pipeline.

**Switch:** `contact.retarget_from_full_ring`, on when absent (deviation **D-ε**, §2.1, Q0110).

**Two changes from the stopped branch:**
1. **The acquisition refactor is made cost-neutral.** On the branch, `candidate_key`, shared by `scan_candidates`
   and `ring_retarget`, raised acquisition from 25.1 % to 33.4 % of the Legacy build's instructions. That cost 1.17 G
   more on 64 × 64 for an unchanged behaviour (evidence §13.13, cost). The port keeps **one code path** for the
   admission test and key, as §17.3 requires, with these constraints:
   - the per-candidate predicate is split from the key computation;
   - the key is built only for admitted candidates;
   - the positions are passed by value;
   - `scan_candidates` keeps its early-out order: cheapest tests first, the scan range before the visibility and
     plane tests if the branch reordered them (**verify** against the plan head).

   Acceptance: Legacy and Solver instructions on every throughput scenario equal the WP1 tree's within noise
   (§7, cost gate C-1). This is a behaviour-preserving refactor, proved by the switch-off bit identity.
2. **It is built on E-1.** The surround that G-6′ creates no longer shoves (expected; WP2 measures it).

**Retarget churn guard.** A hand-over A → B may be followed by B → A only when A's ring has an admissible free angle
at that tick's start, or when A outranks B. This is the hysteresis §17.3 already specifies (the `ring_retarget`
memory plus "kept unless outranked"). WP2 adds a test of it (G-T7) and a trace count (D-CHURN, §5.1) that must be
zero for unexplained returns.

### 4.3 Order of the two changes

E-1 comes first, in its own step (WP1), because:
- it is a correction towards a documented game fact with high confidence, independent of D-ε;
- G-6′'s matrix stop is confounded by the shoving it amplified (§1.2), so G-6′ must be judged on a tree without
  shoving;
- WP1's matrix run gives the base against which G-6′'s shifts are read. Without it, a shift could not be attributed
  to the retarget or to the shove fix.

---

## 5. Diagnostics and figures

### 5.1 New committed diagnostics (WP0; `sim_fixtures`, no behaviour change)

**D-SHOVE (B-193).** For a unit, it sums `|x(t) − x(t−1)|` over the ticks `t` in which all three of these hold:
- the unit proposed no motion: it is in band of its target and in its attack phase, or standing under an order that
  does not walk;
- at tick `t − 1` an enemy body of its class lay within `r + r_e + 1/64` of it;
- it was not holding (holders are never moved by anyone).

Engaged units propose no motion, so every such displacement is contact-induced. It is reported per side and per
battle in the matchup trace (`crates/sim_fixtures/src/trace/`, a column of the whole-matrix CSV) and per scene in the
harness mirror family. It is the committed form of the evidence's ad hoc "distance moved while in contact" (§13.13),
and it must reproduce that figure: 3.61 on battle 28 seed 1 at the base (**verify**; the evidence's script is in
`~/scratch/contact-solver/wp4b/diag/py/`). If it differs, WP0 records why.

**D-R10 (B-192), ported from `contact-solver-g6p-wip` (`harness/room.rs`)** to the matrix trace as well as the
mirror. It counts unit-ticks standing out of band on an automatic chase while another enemy within scan range has
room on its ring. It is split into four columns:
- `leash`: the free slot lies beyond the leash;
- `terrain`: the slot or the bearing point is in terrain;
- `unruled`: the candidate's own bearing point is unruled;
- `free`: none of these, so G-6′ should take it.

Only `free` is a defect of R10's kind. The other three are the leash (game behaviour) and the known gaps of the
measure.

**D-CHURN.** It counts target returns A → B → A within 25 ticks (1 s), split into two:
- `explained`: at the return tick A's ring had room, or A outranked B;
- `unexplained`: neither.

**Existing figures kept:** contact and blocked counts, target switches, flanking seconds, give-ups and enemy
overlap maximum (matrix trace); T1 to T9, E1, D-S and the friend overlap maximum (harness).

### 5.2 Baseline of record (WP0)

All runs are on the package base: `main` after the solver merge (and WP7 if merged). They use one release binary per
arm and run one at a time under `flock ~/scratch/gate.lock`. Outputs go to `~/scratch/solver-followup/wp0/`.

1. **The matrix on the 24 seeds** and on the **held-out matrix seeds 21 to 44** (never looked at before), under the
   Solver pipeline. For battles 26, 28, 33, 38a and 38b, also run the Legacy pipeline: a scratch catalog with
   `pipeline: Legacy`, or the solver merge commit's binary if WP7 has deleted Legacy.
2. **The harness families** on the development range (seeds 1 to 1,000) and the held-out range of the solver plan's
   Amendment 16, with the new columns.
3. **Throughput:** the three scenarios (`cargo bench -p sim_core --bench throughput`), `perf stat -e instructions:u`
   under the gate lock, three runs each for the noise band, both pipelines.
4. **The close-battle list (pre-registered, §6.2)**, written into the evidence file before WP1 runs.

---

## 6. Matrix acceptance under the owner's guidance

### 6.1 What "broken behaviour" means here (pre-registered)

A matrix battle whose outcome moves under WP1 or WP2 is judged on **behaviour checks**, not on its winner. Each check
is measured per battle over the 24 seeds against the WP0 base (and, for B2, against Legacy).

| Check | Broken when | Source of the bar |
|---|---|---|
| **B1, stalled with room** | D-R10 `free` above 0 in more than noise (any battle, after WP2); or blocked unit-seconds above the WP0 base beyond noise | AGENTS.md §8 example "units stalling instead of engaging reachable targets"; R10 |
| **B2, shoving** | D-SHOVE of any side above the Legacy pipeline's on the same seeds (after WP1) | B-193; Q0095 "never pushed by enemies"; §2.2 |
| **B3, enemy overlap** | Largest enemy overlap above T7's enemy bound, or above the WP0 base beyond noise | D-α ("never enemies beyond the dead band") |
| **B4, retarget churn** | D-CHURN `unexplained` above 0 | §4.2 hysteresis |
| **B5, give-ups** | Give-ups of automatic chases above the WP0 base beyond noise | E1, T4 |
| **B6, friend overlap** | Friend overlap maximum above T7 in more scenes than the WP0 base beyond noise (harness) | T7, B-191 |
| **B7, firing** | Shots per unit-second of the ranged side lower than the WP0 base beyond noise in a battle where it is not explained by its units dying sooner | evidence §13.12's check for battle 26 |

"Noise" is the solver plan's rule: a bootstrap over seeds (harness) or over the 24 seeds (matrix), 95 %, plus
Amendment 17 §17.2's caveat on scene mix for held-out ranges.

### 6.2 Which battles may be restated, and how

**Class R (restatable as a minor deviation)** comprises:
- **33, 38a and 38b,** which the owner named;
- **26 and 28,** already under Q0103;
- every battle that WP0 classifies as **close**: its claim says "close", "narrowly", "trade evenly" or similar, **or**
  at the WP0 base neither side wins at least 22 of the 24 seeds.

The list is written into the evidence file in WP0, before any behaviour change.

A Class R battle that leaves its range, or flips its winner, is **restated** if every check of §6.1 passes for it.
The restatement:
- uses the measured range over the 24 seeds, as Amendment 18 and 19 did;
- rewrites the claim text where the winner changes;
- adds a fixture comment citing AGENTS.md §8, Q0103, this plan, and the mechanism (D-ε, E-1, or both).

A pair of battles that are each other's control (38a and 38b) is restated together, so that the pair still states
what the spell changes. For example, "Fungal Growth turns a close fight into a clear zergling win" is a claim about
the margin, if 38b becomes close.

**Class D (decisive)** is every other battle. If its winner flips or it leaves its range:
- **stop for review,** with traces and the §6.1 table;
- if every check passes and the review finds the mechanism sound, it is restated with the orchestrator's amendment;
- if the battle's claim is sourced to StarCraft II and the new result contradicts the source, the claim goes to the
  owner as a question instead.

**Any battle failing a §6.1 check** is a stop, whatever its class. The failing check is a defect of this package. It
is either fixed within the package (one attempt, as an amendment) or registered as a B-row with the package stopped
before WP2's merge.

**Battle 28.** After WP2 it must hold its StarCraft II claim ("Without splash, immortals are overrun") on every seed
of record. Its `known_bug: Some("B-192")` marker is removed in the same commit that makes it hold (§7, WP2), because
`Battle::verdict` turns a marked battle that holds on every seed into `StaleKnownBug`, which fails the test.
- **If WP1 alone makes battle 28 hold on every seed of record,** the marker is removed in WP1.
- B-192 then stays open: its register row is rewritten to cite test G-T1 (red until WP2) and D-R10 `free` instead of
  battle 28.
- WP2 proceeds.

### 6.3 Held-out check

**Claims:** every claim restated in WP1 or WP2 is checked on the held-out matrix seeds 21 to 44.
- They are reported, never used to widen a range.
- If a restated claim fails on more than a third of those seeds, the restatement was fitted too narrowly: stop for
  review, and rewrite the claim from both ranges in an amendment.
- Battle 28's claim must hold on at least 22 of the 24 held-out seeds (Legacy's level of the claim).

**Harness:** the harness families on the held-out range under Amendment 8's step rule against the WP0 base.

---

## 7. Work packages

**Every package follows the charter's routine:**
- failing tests first: red on the package's base, green after;
- `cargo gate quick` and the tests the change needs, then hand back;
- a verifier runs `cargo gate package` on the final tree (charter §4);
- builds use `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`;
- measurements run one at a time under `flock ~/scratch/gate.lock`, with outputs under `~/scratch/solver-followup/`.

**Base:** `main` after the solver plan's merge. The implementation branch is cut from there, not from this planning
branch.

| WP | Package | Content | Behaviour | Schema | Gate | Size |
|---|---|---|---|---|---|---|
| WP0 | P1 | Diagnostics D-SHOVE, D-R10 (matrix), D-CHURN; baseline of record; close-battle list | none | unchanged | package | M |
| WP1 | P2 | E-1 (B-193) | Solver: enemy pairs | unchanged | judged at WP2 | M |
| WP2 | P2 | G-6′ ported (B-192), cost-neutral acquisition, the allowed cost step, matrix restatements, battle 28's marker removed, goldens, register, `DESIGN.md` | Solver: acquisition at a full ring | +1, provisional 47 | package, then `full` on `main` | L |

P2's package gate runs once, on WP2's final tree. WP1's commit compiles and its own tests are green. The matrix and
golden tests are judged on WP2's tree only, and the golden replay is re-recorded once, in WP2.

### WP0: diagnostics and baseline (package P1, no behaviour change)

**Code** (sim_fixtures only):
- D-SHOVE in the matchup trace and the mirror family.
- D-R10 with its four columns, ported from `contact-solver-g6p-wip`'s `harness/room.rs` and `harness/mirror.rs`
  without G-6′, and added to the matchup trace.
- D-CHURN in the matchup trace.

**Tests first:**
- **W0-T1** `trace::tests::shove_counts_only_contact_displacement_of_units_that_propose_none`: plain values. A
  standing engaged unit displaced by 0.1 with an enemy in contact counts 0.1. A walking unit counts 0. A holder
  counts 0. Without enemy contact it counts 0.
- **W0-T2** `harness::room::tests::r10_splits_leash_terrain_unruled_and_free`: one constructed case per column.
- **W0-T3** `trace::tests::churn_separates_explained_returns`.
- **W0-T4:** the CSV header tests and the old-header readers (`LEGACY_CSV_HEADER`) keep reading the earlier tables.
  New columns are appended last.

**Acceptance:**
- No checksum, golden or harness figure moves except the new columns.
- D-SHOVE reproduces the evidence's 3.61 on battle 28 seed 1 within rounding, or WP0 records the difference.
- The baseline of §5.2 and the close-battle list are committed to the evidence file
  `docs/evidence/2026-11-24-solver-retarget-and-shoving.md`.

### WP1: E-1, enemies yield by their own closing (B-193)

**Code:**
- `movement/solver.rs`:
  - `contact_shares` returns a new variant, `PairShares::EnemyByClosing`, for non-holding, non-fixed enemies when
    the switch is off.
  - `pair_correction` computes the shares from `approach_shares(n, x_a − s_a, x_b − s_b)`, with the held rule of
    §4.1.
  - The table in `contact_shares`' rustdoc and the module doc are updated.
- `catalog/movement_rules.rs` and `assets/catalog/movement_rules.ron`: the switch `enemies_push_each_other` (absent
  = `false`), validated, and documented in the file's header comment.
- The plan text C-2 and C-8 of the solver plan are not edited. This plan's §4.1 supersedes them, and `DESIGN.md`'s
  Movement row says so.

**Tests first** (red on the base unless marked guard). Scenes use scratch archetypes and name no catalog unit.
- **E-T1** `contact_solver_tests::a_pressing_crowd_does_not_move_a_standing_enemy`.
  - The scene: one standing, armed, not-holding body of radius `R` attacks an unkillable target in reach. `N`
    melee attackers of radius `r` attack-move into it from one side in three ranks. `N` is twice the ring capacity
    from the solver plan's §4 capacity formula.
  - The assertion: over the time for the rear rank to arrive (the rank depth over the attackers' speed, times two),
    the standing body's D-SHOVE is 0 and its position equals its start.
  - Mirrored and relabelled: the same.
- **E-T2** `…::a_ringed_mover_is_trapped`: a body on `Move` through a closed ring of standing enemies does not pass;
  no ring body moves.
- **E-T3** `…::two_advancing_lines_share_by_their_approach`: two lines of equal speed meet head-on and their contact
  shares are ½ / ½ (bit-identical under mirror). A line that stops is not pushed by the one still walking.
- **E-T4** `…::the_enemy_push_switch_on_is_bit_identical`: with `enemies_push_each_other: true`, the tick-by-tick
  checksum digests of the E-T1 scene and of battle 28's first seed of record equal the base's (FNV-1a digests, as
  §13.13 did).
- **E-T5** `solver::tests::a_terrain_held_closer_hands_its_enemy_share_to_the_partner`: a zergling pinned on a wall,
  overlapping an enemy. The enemy pair ends within the dead band and the wall is never entered.
- **E-T6** `solver::tests::enemy_shares_follow_each_bodys_closing` (unit): the shares for one closing, both closing
  and neither closing, and order independence (the existing `the_solution_does_not_depend_on_body_order` extended
  with enemies).
- **Guards** (green before and after):
  - `anchored_bodies_stay_and_enemies_separate`, `enemy_pairs_take_no_dead_band`, `enemy_constraints_have_the_last_word`;
  - `a_nudged_attacker_slides_along_its_ring_only` (an enemy never nudges);
  - `push_priority_tests.rs` "enemies never push".

**Measurement** (one binary per arm):
1. Harness, development range, Amendment 8's step rule against the WP0 base.
   - Gap, cross and corner are expected byte-identical: they have no enemies.
   - Arc, corridor and mirror may move: T3, T7, T9 and D-S are judged, and D-SHOVE is reported.
2. Switch on: every harness CSV byte-identical to the base.
3. The matrix on the 24 seeds and the held-out 21 to 44, with the §6.1 checks.
4. Throughput, three scenarios.

**Stop rules:**
- E-T1 green on the base: the mechanism is not the even split. Stop before any code change.
- A family expected byte-identical is not: a defect, so stop and find it.
- Amendment 8's step rule fails on arc, corridor or mirror, or T7's enemy bound fails: stop for review.
- A §6.1 check fails: stop (§6.2).
- Throughput: a scenario below 50 ticks per second, or Solver instructions on any scenario above the WP0 base by more
  than 2 % (the shares are one dot product per enemy pair and iteration): stop and report the split.
- Not allowed: a constant, a per-unit or per-battle switch, or any parameter chosen after seeing figures.

**Matrix outcome:** Class R battles are restated per §6.2, Class D ones are stopped for review, and battle 28 is
handled per §6.2's WP1 clause.

### WP2: G-6′ (B-192), the marker, and the close of the follow-up

**Code** (ported from `2cfb880f`, rebased on WP1):
- `movement/ring.rs`: `SimWorld::ring_retarget` and `RingAim::Unruled`.
- `target_acquisition.rs`: the shared admission predicate and key (cost-neutral, §4.2 item 1), and
  `decide_acquisition` keeping a `ring_retarget` target unless outranked.
- `movement/locomotion.rs`: the stand/retarget helper in `prepare_pursuit` (line limit).
- `movement/mod.rs`: `MotionProposal::ring_retarget`.
- `entity.rs`: `MovementMemory::ring_retarget`.
- `checksum.rs` and `checksum/encoding.rs`: the field is hashed; schema next free at merge (provisional 47), proved
  as in the solver plan's Amendment 10. A scratch build with the field unhashed at the base schema reproduces the
  base's digests with the switch off.
- `catalog/movement_rules.rs` and `assets/catalog/movement_rules.ron`: `retarget_from_full_ring` (absent = on),
  with the header comment of the branch.
- `crates/sim_core/tests/fixtures/matchup_matrix.ron`:
  - battle 28's `known_bug` marker removed and its comment rewritten (B-192 fixed by this plan);
  - Class R restatements per §6.2.
- `crates/sim_core/tests/fixtures/golden_replay.ron` and the other pinned digests: re-recorded once, with a proof
  header on four builds (native debug and release; `wasm32-wasip1` debug and release, runner
  `wasmtime --dir=<real path>`), as the solver plan's §7 requires.

**Tests first:**
- **G-T1 to G-T6:** §17.3's tests 1 to 6 as on the branch (`crates/sim_core/tests/ring_retarget_tests.rs`;
  `ring::tests::a_bearing_point_beyond_the_leash_is_unruled`). Test 5's pinned digests are re-pinned at the new
  schema.
- **G-T7** `ring_retarget_tests::a_handed_over_chaser_returns_only_when_its_old_ring_has_room`: a static scene in
  which the near ring stays full, so there are no returns over 400 ticks. Then one near attacker is killed, and a
  return happens only after the near ring shows room.
- **G-T8** `ring_retarget_tests::a_surround_does_not_shove_the_block` (with E-1): the G-T1 scene with armed,
  non-holding blocks. The D-SHOVE of the block is 0.
- **G-T9** `target_acquisition::tests::the_shared_candidate_test_admits_what_scan_candidates_admitted`: property test
  over random scenes. The admitted set and order equal those of the base's `scan_candidates`, which is kept as a
  test oracle in the test module only.
- **Matrix:** battle 28 holds on every seed of record with the marker removed. `the_fixture_reads_a_known_bug_marker`
  is kept with a scratch fixture, since the shipped matrix may then carry no marker. **Verify** the test reads the
  shipped file, and if so, change it to a constructed fixture in the same commit.
- **Register:** `every_known_bug_marker_names_an_open_bug_of_the_register` stays green. With no marker it is
  vacuous, and that is correct.

**Measurement:**
1. Harness, development and held-out ranges, under Amendment 8's step rule against WP1. The scene families are
   expected byte-identical (§13.13). The mirror's T9 is judged, and D-R10 `free` is reported.
2. Switch off: every CSV byte-identical to WP1, and digests equal under the unhashed scratch build.
3. Battles 26 and 28 on the 24 seeds, with §13.12's contact/blocked table and ring occupancy.
4. The full matrix on the 24 seeds and the held-out seeds 21 to 44, with §6.1's checks.
5. Throughput (§9).

**Cost step (allowed once, behaviour-preserving).** This is §17.3's covered-ring early answer. P8's cache entry
records whether its intervals cover the whole ring, so a covered ring answers `Stand` at once for an asker with no
interval of its own on it. It is proved byte-identical with the cache switch off, as P8 was. It is taken only if
cost gate C-2 (§9) fails without it.

**Stop rules:**
- G-T1 green on WP1: E-1 fixed R10 by itself. Then follow §6.2's WP1 clause. G-6′ is not added unless D-R10 `free`
  is above 0.
- G-T1 stays red because retargeted attackers press into the near column and stall (R1 with an enemy front): stop.
  No detour is added.
- A family expected byte-identical is not: a defect.
- A §6.1 check fails, or a Class D battle moves: stop for review (§6.2).
- Battle 28 does not hold on every seed of record: stop, with the ring occupancy table.
- Cost: §9's gates fail after the one cost step. Stop, and file the conditional Q0111 (§12.2) with the split.
- Not allowed: a cap on candidates, a hysteresis margin, a per-unit or per-battle switch, or a parameter chosen after
  seeing figures.

**Documents (same WP):**
- **`DESIGN.md` §4:**
  - B-192 and B-193 are marked **fixed**, with the commits.
  - New rows only for what remains, provisional from B-202:
    - **B-202** if D-R10's `terrain` or `unruled` columns are nonzero after WP2. These are standing units whose
      other candidates are unruled; G-6′ does not cover them.
    - **B-203** if the neither-closed residual of E-1 shows in D-SHOVE above the dead band on any battle.
- **`DESIGN.md` Movement row:** E-1 replaces "enemies separate evenly", and D-ε is listed.
- **`DESIGN.md` §5:** the throughput table.
- **Q0052:** D-ε is added to the addendum only after Q0110's answer. Until then, the deviation is recorded in this
  plan and in `DESIGN.md` as "pending Q0110".
- **The evidence file:** every figure, trace and decision.
- **Q0110** is filed by the orchestrator (§12.1) when WP2's figures are in, with the restated battles listed.

**Gate:** `cargo gate package` on WP2's final tree. Then the orchestrator runs `cargo gate merge` and, before the
push, `cargo gate full` on `main` (charter §4).

---

## 8. Golden replay and checksum schema

- **WP0:** no change.
- **WP1:**
  - E-1 moves positions, so the golden replay's checkpoints and every pinned world digest move.
  - The checksum *format* is unchanged: no new state, since the shares are recomputed each tick.
  - The re-record is deferred to WP2 (one re-record per package).
- **WP2:**
  - `MovementMemory::ring_retarget` is hashed, giving schema **next free at merge**. That is provisionally 47:
    `main` is 44, fire-rules 45, and the solver merge takes the next free one.
  - The proof follows the solver plan's Amendment 10: a scratch build with the field unhashed at the old schema
    reproduces the WP1 tree's digests with the switch off, so the schema bump is the only cause of the digest
    change.
  - The golden replay is re-recorded on four builds with the proof header. `DEFAULT_SANDBOX_CHECKSUM` and the other
    pinned digests are re-pinned only if they move.
- **Determinism on every target:**
  - Every new input is a tick-start position, an iteration-start position, the phase-4 spatial hash, vision maps or
    the static map.
  - Every comparison is Q32.32 or `SimAngle`, and every order is total, ending in the entity id.
  - No float, `HashMap` or iteration-order dependence is introduced.
  - The CI determinism jobs (Linux, Windows, macOS ARM64, `wasm32-wasip1`, debug and release) check it.

---

## 9. Throughput budget

**The owner's bars:**
- **Q0093 (c):** at least 2× real time (50 ticks per second) on every throughput scenario, and instructions at most
  +60 % against Legacy.
- **Q0102 (a):** +66.8 % on 64 × 64 is accepted. The solver plan measured +7.6 % on 128 × 128 and +57.6 % on canyon.
  The follow-up must not make these worse without saying so.

**The figures before this plan** (evidence §13.13, plan head; to be re-measured on the WP0 base):

| Scenario | Legacy | Solver | Ratio | Ticks/s |
|---|---:|---:|---:|---:|
| 64 × 64 | 9.47 G | 15.79 G | +66.8 % | 111 |
| 128 × 128 | 10.06 G | 10.83 G | +7.6 % | 183 |
| canyon | 17.16 G | 27.04 G | +57.6 % | 80 |

The stopped G-6′ measured 17.18 G on 64 × 64. That is +61.4 % on its own Legacy, but +81.4 % on the plan head's
Legacy, because its acquisition refactor slowed both pipelines.

**Gates** (each scenario, three runs, noise = the spread of the three):
- **C-0, real time:** 50 ticks per second or more on every scenario after every WP. A failure is a hard stop.
- **C-1, refactor is cost-neutral (WP2):** the **Legacy** pipeline's instructions on the WP2 tree are equal to the
  WP1 tree's within noise on every scenario. If WP7 removed Legacy, the Solver tree with `retarget_from_full_ring:
  false` is used instead, against WP1.
- **C-2, the follow-up's own cost:** Solver instructions on WP2's tree are at most **+3 %** over the WP0 base's on
  each scenario. This is the solver plan's G-6′ cost per §17.3, one ring query per standing chaser, with E-1's one
  dot product per enemy pair. It keeps 64 × 64 within about +72 % of Legacy, and it must be reported as such.
- **Reporting:** the ratio against the same tree's Legacy and against the WP0 base's Legacy is reported for every
  scenario in `DESIGN.md` §5.
  - If 64 × 64 rises above +66.8 %, the change is **said so** in the register text and in Q0110's background.
  - If it rises above +70 %, or canyon above +60 %, the conditional **Q0111** (§12.2) is filed before the merge.
  - Q0111's default is to accept, provided C-0 holds, following the owner's reasoning in Q0102 (a).

---

## 10. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| E-1 lets a crowd pile overlap on its own friends (they yield to the enemy and close on the friend behind) | T7 friend overlap rises (B-191 worsens) | Measured in WP1 (§6.1 B6); D-α's moving bound and the next tick's averaged iterations resolve it; stop if beyond noise |
| A terrain-held closer and a standing enemy | The held rule hands the share back to the enemy, a small push | E-T5; D-SHOVE reports it; B-203 if it shows |
| Two fronts pressing into each other (both closing) still move each other proportionally | A stronger crowd still advances against a weaker one | That is two walkers meeting, not a push of a standing unit; the game's behaviour for this case is undocumented (§2.2, medium). The switch can restore the even split; E-T3 pins the symmetry |
| G-6′ shifts close battles further (33, 38a, 38b) | Matrix restatements | Allowed under AGENTS.md §8 if every §6.1 check passes; listed for the owner in Q0110 |
| A decisive battle moves | Stop for review | §6.2 Class D |
| The refactor's cost (as on the branch) | Legacy and Solver both slower | Cost gate C-1; G-T9 proves admission unchanged |
| Retarget ping-pong between two near-full rings | Units dither, routes are rebuilt each tick | Hysteresis (kept unless outranked); G-T7; D-CHURN `unexplained` = 0 |
| E-1 alone makes battle 28 hold | The marker goes stale in WP1 | §6.2's WP1 clause |
| Undocumented game mechanism (D-ε) | The owner may prefer standing | Q0110 with the switch; default on |
| Rebase conflicts: the solver branch merges after fire-rules, and WP7 may land first | Porting cost | The base is `main` after the merge; the port is from a diff of 14 files, mostly new code |

---

## 11. Out of scope and decisions recorded (charter §8: scoping is the agent's call)

- **Detours round an enemy block** (P2, 2′, or a walk to the far side of the same target) are not added. They are
  stopped by the solver plan's step rule, and G-6′ needs none if G-T1 passes.
- **Crowding penalties, global assignment and flow fields** (R-d to R-f) are not used (§3.1).
- **Explicit `Attack` orders are never retargeted.** This is acquisition invariant 1. The game's own advice on
  explicit attacks into a full front (Liquipedia, Micro) agrees.
- **Battle 26** stays under Q0103 (a). It is restated again only if its new range leaves the restated one, as a Class
  R battle.
- **B-188 to B-191** (oscillation, give-ups, E1, friend maximum) are not addressed. They are only guarded by §6.1.
- **The legacy pipeline** is not changed. Its own even enemy split stays until WP7 of the solver plan deletes it.
- **Patch 5.0.15's** allied push priorities for Thors and Siege Tanks are after the patch of record. They are not
  applied.
- **E-1 for air bodies.** Air bodies follow the same rule, since the game's rule is about players, not planes. There
  is no air-specific case.

---

## 12. Owner questions (drafts; not filed)

### 12.1 Draft Q0110 (provisional, next free at merge): filed by the orchestrator when WP2's figures are in

```markdown
# Q0110: Zerglings stuck behind a fully surrounded enemy now switch to an enemy with room. Keep this as a deviation?

- **Asked:** 2026-11-24 (filed when the follow-up's figures are in)
- **Blocking:** no
- **Default until answered:** (a), the switch on, as merged. Battles restated under your Q0103 guidance stay
  restated.

## Background

In a large melee fight, every attacking unit picks the nearest enemy. When the space round that enemy is full, the
new movement solver makes the unit stand and wait behind the front row (bug B-192). It waits even when another enemy
a little further away has free space all fight. Battle 28 (100 zerglings against 10 immortals in two columns) showed
it: the zerglings lost 19 of 24 runs, against 0 with the old movement code.

The fix: a unit whose automatically chosen target is fully surrounded picks, in its normal order of preference, the
next enemy within its scan range that still has room. It never rewrites an attack you ordered on a specific unit,
never moves a unit on Hold Position, and never extends how far a unit chases. With it, battle 28 goes back to
zergling wins on [N] of 24 runs.

We could not find how StarCraft II itself decides this. Blizzard's guides say zerglings surround clumps by default
and that some units can stand idle behind a full front. Neither says whether a unit switches to a free enemy, or
reaches it only by being carried round the crowd. So we record the rule as a deviation that allows more, behind a
data switch (`contact.retarget_from_full_ring` in `movement_rules.ron`), called D-ε.

A second fix in the same package makes the game's own rule exact: units never push enemy units (Blizzard patch 1.2.0;
your Q0095 answer). Before it, a crowd of zerglings walked the immortals it fought several world units backwards.
That fix needs no decision. It is mentioned because both fixes together moved these close battles, restated under
your AGENTS.md §8 guidance after checking that no unit stalls, shoves, overlaps or dithers:

| Battle | Before | After | Restated claim |
|---|---|---|---|
| [filled in from WP2's evidence] | | | |

The cost on the largest benchmark is [X] % more instructions than the old movement code (accepted level: +66.8 %,
Q0102), at [Y] ticks per second.

## Options

- **(a) Keep D-ε on, as a sanctioned deviation** (added to Q0052). Default.
- **(b) Keep it, but off by default:** units stand behind a full front, as the guide's "idle units behind the curve".
  Battle 28 then needs its expected-failure mark again, or a restated claim.
- **(c) Remove it,** and look for the game's mechanism (for example, being carried round the crowd's edge) in a later
  package.

## Answer
```

### 12.2 Draft Q0111 (provisional; filed only if §9's reporting threshold is crossed)

```markdown
# Q0111: The retarget fix costs [X] % more instructions on the 400-unit 64×64 benchmark. Accept?

- **Asked:** [date]
- **Blocking:** the merge of the solver follow-up (plan 2026-11-24-solver-retarget-and-shoving)
- **Default until answered:** (a), accept, if every scenario stays at or above 50 ticks per second (your Q0093 (c) bar).

## Background

You accepted +66.8 % on this benchmark for the movement solver (Q0102 (a)). The follow-up that fixes stuck zerglings
(B-192) and enemy shoving (B-193) adds [Z] %, now +[X] % in total, at [Y] ticks per second. One allowed
behaviour-preserving optimisation was already applied. [Split of the cost by phase.]

## Options

- **(a) Accept.** Default.
- **(b) Hold the merge** for a cost package (for example, Gauss–Seidel or sleeping bodies, as Q0093 mentioned).

## Answer
```

---

## 13. File ownership

| File | WP |
|---|---|
| `crates/sim_fixtures/src/trace/*.rs` (D-SHOVE, D-R10, D-CHURN columns), `crates/sim_fixtures/src/harness/room.rs`, `mirror.rs`, `mod.rs` | WP0 |
| `crates/sim_core/src/movement/solver.rs`, `crates/sim_core/src/catalog/movement_rules.rs`, `assets/catalog/movement_rules.ron` (`enemies_push_each_other`), `crates/sim_core/tests/contact_solver_tests.rs` | WP1 |
| `crates/sim_core/src/movement/{ring,locomotion,mod,collision}.rs`, `crates/sim_core/src/target_acquisition.rs`, `crates/sim_core/src/entity.rs`, `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs`, `movement_rules.*` (`retarget_from_full_ring`), `crates/sim_core/tests/ring_retarget_tests.rs`, `crates/sim_core/tests/fixtures/{matchup_matrix,golden_replay}.ron`, `crates/sim_fixtures/src/matchup.rs` (fixture test only, if needed) | WP2 |
| `DESIGN.md` §4, §5 and the Movement row; `docs/evidence/2026-11-24-solver-retarget-and-shoving.md` | WP0 (evidence), WP2 (all) |

`DESIGN.md` §2 (crate graph) is unchanged. No crate gains a dependency.

---

## As built (implementation agent, 2026-11-24)

Branch `solver-followup`, cut from `main` `e5f5c565` (the solver merge: contact solver default, schema 46, Legacy
still selectable because WP7 has not landed), merged with `main` `708576ac` (attacker snapshot, turret wind-up WP3)
at `485b27e5`, at the coordinator's request, after the first WP0 baseline. Every arm was rebuilt on the merge and
the baseline re-derived (`b0`); the close-battle list did not change. **The package stopped at WP2** (below). Figures are in
[`docs/evidence/2026-11-24-solver-retarget-and-shoving.md`](../evidence/2026-11-24-solver-retarget-and-shoving.md).

### Drift of this plan's references against `main`

- The plan was written on `contact-solver-plan` at `a2b420c9` (schema 40). On `main`, the solver merge took
  schema 46, so this plan's one bump is **47**. The stopped branch's schema 42 and its "41 reserved" note are void.
- `target_acquisition.rs` changed under the plan: fire-rules added the own-weapon-target scan (`ScanScope::
  own_target`, `OwnTargetScan`), and weapon validation replaced the vision test by `sees_enemy`/`sees_point`
  (attack reveal). The stopped branch's `candidate_key` did not apply; the shared test was rewritten on `main`'s
  scan (§4.2 item 1), keeping its early-out order (the scan range is still tested after the visibility test, as on
  `main`; nothing was reordered).
- `checksum.rs`'s schema history and `movement_rules.ron`'s header had moved; the port merged by hand. `ring.rs`,
  `locomotion.rs`, `mod.rs`, `entity.rs` and `encoding.rs` applied from `2cfb880f` unchanged.
- `the_fixture_reads_a_known_bug_marker` (`sim_fixtures::matchup`) reads the shipped file (**verify** answered:
  yes); it now inserts a marker into a copy of the shipped text.
- File and test names: W0-T1 and W0-T3 live in `trace::contact_checks::tests`, not `trace::tests`; the meter is
  one module, `trace/contact_checks.rs`.

### WP0

- D-SHOVE, D-R10 (four columns) and D-CHURN, as defined in the evidence's §1, in `matrix.csv`'s last 21 columns
  (with the counters of B1, B3 and B7) and in the mirror family (`shove`, `dr10_*`). The ring geometry is the
  stopped branch's (`harness/room.rs`, radius band − r/2), extended with the free arcs and the classifier.
- D-R10's precedence when several candidates have room: free > unruled > terrain > leash. A candidate whose band
  the unit is already in counts as free (G-6′ would take it, `RingAim::Straight`).
- D-SHOVE does not reproduce §13.13's 3.61: on this base it gives 1.13 for battle 28 seed 1. Both the measure and
  the tree differ (evidence §2.3, §3.1).

### WP1 (E-1)

- **E-T1's attacker speed.** At the scratch archetypes' 0.125 per tick the attackers stop at the band, 0.1 off
  the target's disc, and the even split never moves the target: E-T1 was green on the base. With a step longer
  than the melee gap (0.1875; the shipped zergling analogue steps 0.165), a committed step overshoots into the
  disc, B-193's first way in (§1.2 (a)), and the base moves the target: E-T1 red, then green under E-1. Treated as
  a test construction, not as §7's "E-T1 green on the base" stop, because the base never overlapped the target in
  the first scene at all; recorded for review.
- **E-T3 needs a corridor.** On open ground two lines of walkers slide past each other and never press. The test
  runs two single-file columns head-on in a corridor one cell wide. The scene is not point-symmetric in its last
  bits even before the columns meet (a few millionths in the walk from rest), so "½ / ½, bit-identical under
  mirror" is checked to 1/1024 world units.
- **Deviation from §4.1: the committed-step fallback.** In the corridor, the walls leave a residual overlap after
  the last pass each tick; the next tick an enemy correction carries the presser back past its tick-start position,
  so neither displacement closes and §4.1's even split hands the standing column half of the residual every tick
  (0.21 world units over 300 ticks, a real shove). When neither displacement closed, the shares now follow the
  committed steps (tick start to proposal) before the even split. A spawn, unload or two standing bodies still
  split evenly. Two standing walkers after a 10 s give-up still split a carried overlap evenly (one 0.05 push in the
  corridor); E-T3 runs inside the give-up time.
- **The held rule** reads "refused" as: the body's last attempted step went to a target other than where it stood
  and terrain left it in place; an application without corrections for the body (the friends' summed pass) leaves
  the flag. Without that persistence E-T5's closer lost its flag in the friends' pass and the pair ended 18 %
  overlapped. Both bodies refused: the closing shares apply.
- E-T4: with the switch on, the per-tick digests equal the base's (`0xa381a7c5b8982232`, `0x6b074574a51eff4b` on
  the merged base); the whole 24-seed matrix with the switch on is byte-identical to the base's.
- **Matrix (judged with WP2, recorded here): E-1 alone fails B3.** The largest enemy overlap rises from about 2.6 %
  to 12.7 % (battle 26), 10.4 % (27) and 10.5 % (29). A build with the committed-step fallback off, or the held
  rule off, gives the same 12.7 % on battle 26 seed 19; the even split gives 1.4 %. So it is §4.1's closing shares,
  not this package's additions. B-193's shove of battle 28's immortals is gone (0.075 per seed against Legacy's
  0.374); the colossi (1.56 against 0.13) and ultralisks (1.07 against 0.18) are still moved.

### WP2 (G-6′)

- G-T1 red on WP1 (`red/wp2-gt1-4-on-wp1.log`: attacker 11 stands at full ring 3 at tick 93 while far ring 2 has
  room), so §7's "G-T1 green on WP1" stop did not apply.
- Switch-off proof: this tree with `ring_retarget` unhashed (`fields!` `unhashed`) at schema 46 and the switch off
  gives WP1's digests (`0x1095d9cd774585ae`, `0x932545703fdf08ac`); G-T5 pins them at 47. E-T4 likewise: with both
  switches at the base's behaviour, the unhashed schema-46 build gives the base's digests; it is pinned at 47.
- G-T7's room check leaves out the asking unit, as the ring rule does (a first version counted the asker's own
  disc and failed on a legitimate return).
- G-T8 was added as specified (armed, non-holding block bodies with a reach of 6; no block body moves).
- **Stopped** (evidence §4, §6): battle 28 fails on seed of record 33 (6 immortals left; zerglings win 22 of 24 and
  21 of 24 held-out seeds hold the claim, 22 needed); §6.1's B1, B3, B4 and B7 fail on several battles; the Class R
  battles 26, 33, 38a and 38b leave their ranges and are not restated because checks fail; no Class D battle flips.
  Cost: Legacy rises 3.5 % on 64 × 64 between WP1 and WP2 (C-1 fails: the shared acquisition test is not
  cost-neutral), and Solver 4.0 % over the WP0 base (C-2's 3 % fails). The code is on `solver-followup-wp2-wip`
  (`dc3b62ab`); battle 28 keeps its marker, the goldens and the checksum encoding unit tests are not re-pinned, and
  the harness families were not run on the merged base.
