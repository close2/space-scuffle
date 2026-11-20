# Blueprint: One Contact Solver, Intent Steering and the Concave as a Goal (Q0092)

This plan carries out the rank-1 recommendation of the movement survey
([`docs/evidence/2026-11-20-movement-survey.md`](../evidence/2026-11-20-movement-survey.md), §4.1) for the **local**
movement layer: what a unit does between its route and its neighbours. The global layer (navigation mesh, TA*,
radius funnel, group lanes) stays as it is. Three parts replace about two thirds of today's local rules:

- **Intent.** A unit's preferred velocity is the direction to its next route point at its braked speed, bent by at
  most one **time-to-collision side rule** against the earliest oncoming or crossing body, with the side **kept per
  pair** until the two separate. Co-moving friends are ignored.
- **Contact.** One deterministic **position-based solver** after all units moved: Jacobi iterations over contact
  pairs, a fixed iteration count, shares (inverse-mass weights) from today's push table, and today's
  `settle_on_terrain` as its terrain step. It replaces the proposal-time velocity limit
  (`remove_blocked_velocity`), its receding allowance, the waiting rule and the separate soft collision pass.
- **The concave.** A **goal choice**: an attacker blocked by its own fighting line takes a free slot on its
  weapon-range arc round the target; a unit with no free slot stands. It replaces the surround-walk state machine.

**Status of this document.** This is a Stage 1 blueprint under charter §6, written on 2026-11-20 against `main` at
`53a0387f` (checksum schema 31) and brought up to `445ed008` (checksum schema 32: resource nodes as 2×1 and 3×3
obstacles, re-laid maps, Q0092 answered), on which the baseline was measured again (§5.1). Its evaluation harness and thresholds (§6) were committed first, before any
measurement (`7f4d26b7`); §6 is that text unchanged, followed by the corrections found while measuring (§6.6).
Line numbers drift, so search for the quoted symbol. Claims not confirmed while planning are marked **verify**.
Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** It is a new mechanic (the solver and the slot goal), it moves every battle outcome
and every golden replay, it changes the checksum schema twice, and it spans seven packages.

**Sources on StarCraft II.** The survey's SC-1 to SC-11, and **the owner's statement** (Q0092's answer): StarCraft II
itself uses iterative soft separation and pushing, with slight temporary overlap between friends (per James Anhalt's
GDC 2011 navigation talk, SC-1), so that crowds flow through ramps and chokes instead of locking up. No public
recording or slides of that talk exist (SC-1); the statement is cited as the owner's, next to the second-hand notes
SC-2 ("lots of rules around who pushes who") and the community accounts SC-5 to SC-7.

**Owner decision.** The owner answered [Q0092](../questions/closed/0092-movement-contact-redesign.md) with option
(a) on 2026-11-20: the redesign is planned and built, and the deviations **D-α, D-β and D-γ are sanctioned**
(addendum of [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md), `445ed008`). Every work package of §7,
up to the switch-over and the deletion of the superseded rules, may proceed in order. Acceptance stays on the
harness's stated properties over held-out seeds, with the thresholds committed before any measurement (§6).

**Inputs.**
- The survey above (§1 the pipeline, §2 sources SC-1 to RT-7, §4.1 the recommendation, §5 the evaluation).
- Plans [`2026-11-09-friendly-blocking`](2026-11-09-friendly-blocking.md) (the push table D-4, the waiting rule,
  the surround walk D-2), [`2026-11-19-crowd-edge-steering`](2026-11-19-crowd-edge-steering.md) and its sequel `-2`
  (the reversal classes R-A to R-F, D-4 queued is not stalled), [`2026-11-19-crowd-slide`](2026-11-19-crowd-slide.md)
  (nine settle candidates, the placebo method), [`2026-11-19-parked-body-aim`](2026-11-19-parked-body-aim.md) (B-144's
  measured rule, B-145's held rounding), [`2026-11-11-contact-and-missiles`](2026-11-11-contact-and-missiles.md)
  (the wall contact rule, B-113), [`2026-11-10-group-routing`](2026-11-10-group-routing.md) (lanes).
- A read of `crates/sim_core/src/movement/` and `crates/sim_core/src/steering.rs` at `53a0387f` (§2).

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0092](../questions/closed/0092-movement-contact-redesign.md) the redesign and deviations D-α, D-β, D-γ | Answered (a), 2026-11-20 | §4, §7 |
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §3, §4 |
| [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) sanctioned deviations | D-α to D-γ added by its 2026-11-20 addendum | §4 |
| [Q0074](../questions/closed/0074-surround-walk.md) the surround walk stays; second-row units slide along the front line | Answered; D-γ changes the mechanism, not the visible result | §3.4, §4 |
| [Q0089](../questions/closed/0089-queued-attacker-keeps-order.md) a queued attacker keeps its order | Kept (G-6) | §3.4 |
| Q0061 hand back after `cargo gate quick`; a verifier runs `package` | Standing rule | §7 |
| Q0002 scoping is the agent's call if written down | Standing rule | §11 |
| `docs/questions/open/` at the start of this session; again after merging `445ed008` | Q0091 (map resource layout, unrelated) and Q0092; then Q0092 closed | — |

---

## 1. Summary

1. **The map (§2).** The local layer has 31 rules (finer than the survey's 25) and 29 constants. The plan keeps 10
   rules, restates 8, replaces 5 and deletes 8; it deletes **12 of the 29 constants** and the surround walk's six
   hashed memory fields, and adds four values to `movement_rules.ron`, each with a derivation (iterations, dead band,
   moving overlap bound, side horizon) and the D-β switch.
2. **The design (§3).**
   - **Intent:** route direction, today's braking, one time-to-collision side rule against the earliest oncoming or
     crossing body that does not yield, the side kept per pair until the pair separates; an exact head-on tie passes
     on the right for both bodies (no ID, mirror-consistent); co-moving friends ignored.
   - **Contact:** Jacobi over contact pairs with averaged corrections, exact integer sums (order-independent by
     construction), shares 0 / ½ / 1 from today's push table, K = 14 iterations derived from the catalog's worst
     closing speed and D-α's 5 % bound, 1 % dead band, a final unaveraged pass over enemy and anchored pairs,
     `settle_on_terrain` after every iteration, early stop.
   - **The concave:** a blocked attacker claims the free angle on its weapon arc nearest its bearing (ties clockwise),
     claims resolved per target by a sort that does not depend on processing order, kept until reached, taken or the
     target moves by a radius; a unit with no free slot stands and keeps its order.
   - **Schema:** 32 → 33 (WP2, new fields) → 34 (WP7, the walk's fields deleted).
3. **Deviations (§4).** D-α, D-β, D-γ are **sanctioned** (Q0092 (a), Q0052 addendum); D-α's tolerances and D-β's switch
   are data. The owner's statement that StarCraft II itself separates softly and iteratively with slight temporary
   friendly overlap (Anhalt, GDC 2011) is cited next to the survey's sources.
4. **The harness (§6)** was committed with its thresholds before any measurement (`7f4d26b7`): four generated
   families with known answers plus a mirror family, relabelled twins, development / acceptance / held-out seed
   ranges of 1,000 each, placebo builds, nine pre-registered thresholds T1 to T9.
5. **Baseline (§5.2),** identical on `53a0387f` and `445ed008`: today's pipeline fails T1 (15.4 visible reversals per
   1,000 moving unit-ticks at gaps), T3, T4 (337 units give up with room at gaps, 73 in open crossings), T6, T7, T8
   (ID bias in crossings and corridors) and T9 (blue wins 42.3 % [38.7, 46.0] of mirror matches).
6. **Prototype (§5.3)** of intent, solver and a slot sketch: reversals at gaps fall 13× (1.18), crossings 2.6×,
   throughput through gaps rises by a third, overlap above D-α's bound falls from 11.6 % to 0.8 %, every relabelling
   bias disappears, crossings complete in 99.7 % of scenarios; it still fails T1 on three families, T4 (mixed-priority
   give-ups at the destination), T5's tail (0.68), T7 and T9. D-β off is worse everywhere. Cost +38 % instructions in
   the unoptimised prototype.
7. **The mirror bias is not contact (§5.4):** the pursuit reads its target's live position; reading the tick-start
   position gives 0.531 [0.481, 0.581] today and 0.544 [0.427, 0.657] with the prototype. WP5 fixes it.
8. **Work packages (§7):** WP1 harness; WP2 data, latent state and the three parts as tested library code; WP3 the Solver
   pipeline (contact and intent) behind `contact.pipeline`; WP4 the slot goal; WP5 the pursuit read; WP6 switch-over
   with the held-out run, goldens on four builds, matrix ranges, throughput by instructions and the showcase
   storyboard on radv; WP7 deletion. Each is accepted on §6.5's properties, never on battle winners.

---

## 2. The current pipeline, mapped

### 2.1 Tick order today

`SimWorld::step` (`world.rs`): commands → production → tasks → vision → `acquire_targets` → attack dashes →
**`step_movement_and_steering`** (hull budgets, then `advance_checked_movement`) → **`resolve_collisions_and_push`**
→ weapons → cast locks → reap → checksum.

`advance_checked_movement` (`locomotion.rs`):
1. `reconcile_route_groups`, `compute_group_lanes`.
2. `prepare_movement_snapshot`: per body class, a tick-start `SteeringObstacle` for every body (position, last tick's
   achieved displacement, owner, priority, `anchored`, `idle`, `settled_on`) and a grid.
3. **Walkers** in `EntityId` order. Each reads the tick-start snapshot and commits at once
   (`commit_motion`: `settle_on_terrain`, then `commit`). Stages of `walk`: `brake` → `upkeep_route` →
   `gather_neighbours` → `decide_arrival` → `next_waypoint` (parked-body aim) → `queued` (`friend_holds_ahead`) →
   `check_progress` → `propose_step` → `step_toward`.
4. Casts, then **pursuits** (`chase`) in `EntityId` order. `prepare_pursuit` reads the target's **live** position,
   which a walker or an earlier pursuit may already have moved this tick (§5.4 measures what this does).
5. `step_toward`: snap onto the goal → `steer_body` → `keep_terrain_view` → `remove_blocked_velocity` →
   `surround_walk` → `is_waiting` → `probe_permits`, else `slide_along_wall` + `after_wall_slide` → `slide_target` →
   `gate_step_by_facing`.

`resolve_collisions_and_push` (`collision.rs`): one pass over overlapping same-class pairs from one snapshot,
`resolve_pair` → `compute_push_displacement_with_mobility` → `checked_dynamic_push` (the push table), friendly sums
capped by `soft_position`, hostile sums uncapped, each correction through `slide_target`, `permits_segment` and
`settle_on_terrain`; finally `velocity = position − pos_at_tick_start`.

### 2.2 Rules of the local layer and what becomes of them

Verdicts: **Kept** (unchanged), **Restated** (kept in meaning, rewritten for the new pipeline), **Replaced** (by the
named part of §3), **Deleted** (nothing replaces it, because the new pipeline cannot produce the case it handled).
The survey counts about 25 local rules; this table is finer, one row per function or decision.

| # | Rule (where) | What it does today | Verdict |
|---|---|---|---|
| L1 | `side_blockers`, `side_weight`, `steer` net weight (`steering.rs`) | Bends the goal direction by the nearest non-yielding body on each side, with radial and lateral weights (touching 0.25 / overlapping 0.35 for co-moving bodies, 0.85 for others, full below 0.85 of touching) | **Replaced** by I-3 (one TTC side rule) |
| L2 | `is_co_moving` (`COMOVING_MIN_COSINE`) | A body moving within 60° of the unit's heading is a flockmate, not an obstacle | **Restated** as I-4: co-moving friends are ignored by intent (SC-2) |
| L3 | `forms_wall`, `wall_end`, `round_wall`, `is_wall_body` | Two standing non-yielding bodies closer than a diameter plus margin are a wall; the unit heads for the nearer end, or straight in if the end is farther than the lookahead | **Replaced**: a standing non-yielding body is a TTC threat like any other (I-3); a line of them is slid along by the solver (C-1); a blocked attacker takes a slot (G) |
| L4 | `surround_side` | Side of a surround walk: nearer wall end, else ID parity | **Deleted** with the walk (G) |
| L5 | `compute_concave_arc_steering` (public) | A tangent left or right of the target; no caller in the tick, tests only | **Deleted** (dead API) |
| L6 | `yields_to` (`STANDING_DRIFT`) | An idle, standing, non-anchored friend of no higher priority steps aside | **Kept**: it is C-2's "idle friend takes the whole push" and I-3's "not a threat" |
| L7 | `is_standing` | Anchored or moved at most 1/64 last tick | **Deleted** (walls and the walk only) |
| L8 | `remove_blocked_velocity`, `remove_blocked_once` | Iterated projection removing approach into every non-yielding body, allowing the room that body receded last tick (the receding allowance) | **Replaced** by C-1: contact is resolved after the move, in one model |
| L9 | `is_waiting` (`WAIT_THRESHOLD` 1/8) | A step cut below 1/8 by blocking bodies waits (velocity 0, `Idle`) | **Deleted**: no proposal-time cut exists any more; C-4's dead band keeps a pressed unit from creeping |
| L10 | `after_wall_slide` `Wait` branch (`crowded`) | A sliver of wall slide in a crowd waits | **Deleted** (needs L8's `crowded`); the `Step` / `refused` part is **Kept** (route refusal signal) |
| L11 | `surround_walk`, `walk_bound`, `across_on_side`, `admits_whole_step`, `closed_side_reopens`, `add_retreat`, `end_surround_walk`, `friend_blocks_ahead` (`FLANK_REVERSE_TICKS`) | The walk mode: start when blocked by a standing non-yielding friend, walk across on a kept side, reverse after 12 blocked ticks, spent after a circumference, end on room beyond the retreat, re-arm on a closed side reopening, on ground gained or on a new target | **Replaced** by the slot goal (G) |
| L12 | `note_queued_progress` | Re-arms a spent walk on ground gained; restores the stall count on a queued tick | **Restated**: the re-arm half is deleted with the walk; the queued half is G-6 |
| L13 | `friend_holds_ahead` (D-4, Q0089) | A chase queued behind an anchored friend is not stalled | **Kept**; it is also G-1's trigger |
| L14 | `check_cluster_arrival_with` (`ARRIVAL_TOUCH_MARGIN`, `ARRIVAL_WITNESS_REACH`) | Arrival on touching a body settled on the same goal | **Kept** |
| L15 | `checked_dynamic_push` (the push table), `motion_flags`, `lane_exit` | Holding/holding zero; holding vs enemy takes all; enemies ½ each unrelaxed; anchored friend: `slide_around_fixed`; else ×0.35; mover vs idle: idle aside along the lane exit (or the mover back if lower priority); else ½ each | **Restated** as C-2's share table (same classes, same who-moves answers; the lane exit and the 0.35 go) |
| L16 | `slide_around_fixed` (`FIXED_BODY_TANGENT_WEIGHT` 0.85) | A body against an anchored friend is moved half along the normal and 0.85 along the lane exit | **Deleted**: the projection keeps the tangential part of a step by construction (C-1) |
| L17 | `PUSH_RELAXATION` 0.35 | Closes a friendly overlap over about three ticks | **Deleted**: K iterations within the tick (C-3) |
| L18 | `soft_position` (`SOFT_PUSH_SPEED_SHARE` 0.65, `SOFT_PUSH_FLOOR` 0.015) | Caps friendly correction per tick and total travel at speed | **Deleted**: the solver only redistributes overlap the steps created (C-5) |
| L19 | `resolve_pair` anchoring | A friend is anchored if holding, or engaged and (priority ≥ the other's, or still against a moving one) | **Restated** in C-2 (anchored: holding or engaged; the priority and stillness tie-breaks go, see C-2 note) |
| L20 | Friendly and hostile accumulators, hostile uncapped | Two sums, applied in two passes | **Replaced**: one sum per iteration (C-1) |
| L21 | `apply_correction` (`slide_target`, `permits_segment`, `settle_on_terrain`) | A correction is walkable from the tick start and never deeper in a wall | **Kept** as C-6, the solver's terrain step |
| L22 | Body classes, ghosts, cliff connectivity, foundations excluded | Who can touch whom | **Kept** (C-7) |
| L23 | `compute_push_displacement_with_mobility`, `resolve_push_interaction`, `PushEntityParams`, `CollisionMobility` (public API) | Single-pair push as an API | **Restated** as the public `contact_shares` (C-2); the old API is deleted in WP7 |
| L24 | `aim_past_parked_body`, `parked_on_corner`, held rounding (`corner_round`, B-145) | Aim past a body parked on a route corner, sense kept while occupied | **Kept** (route following; the survey keeps corner offsets). Reconsidering it is out of scope (§11) |
| L25 | `keep_terrain_view` (`TERRAIN_GUARD_HORIZON`) | A steered step that would lose sight of the leg is replaced by the straight one | **Kept** |
| L26 | `probe_permits`, `slide_along_wall`, `settle_on_terrain`, `project_out` (`WALL_*`) | Disc against terrain | **Kept** |
| L27 | `hull_gate`, `gate_step_by_facing`, `accelerated_speed`, `brake`, `braking_speed` | Turning and speed | **Kept**; `brake` is I-2 |
| L28 | `check_progress`, `check_pursuit_progress` (`STALL_*`) | Repath after 1 s, give up after 10 s; `flank_active` resets the evidence | **Restated**: the `flank_active` reset becomes "walking to a slot" (G-6) |
| L29 | `beyond_leash`, `reroute_pursuit`, `upkeep_route`, `next_waypoint`, `drop_passed_waypoints` | Route following and the leash | **Kept** |
| L30 | `gather_neighbours` reach (`STEERING_LOOKAHEAD` + 2 · speed) | Who steering sees | **Restated**: reach from I-3's horizon |
| L31 | Pursuit reads the target's live position | Order-dependent read inside the walker loop | **Restated** in WP5 if §5.4's hypothesis holds: the target's tick-start position |

**Count.** Of the 31 rows, 10 are kept (L6, L13, L14, L21, L22, L24 to L27, L29), 8 restated (L2, L12, L15, L19,
L23, L28, L30, L31), 5 replaced (L1, L3, L8, L11, L20) and 8 deleted (L4, L5, L7, L9, L10's wait, L16 to L18); L10's
`Step`/`refused` part is kept. Of the survey's 25 local rules, the new pipeline keeps 8 (co-moving exemption, yielding
friends, the push table's classes, parked-body aim, held rounding, terrain slide, queued-not-stalled, arrival) and adds
4 (the TTC side rule with its per-pair commitment, the solver with its hard pass, the dead band, the slot goal).

### 2.3 Constants (`movement/tuning.rs`, 29) and what becomes of them

| Constant | Value | Verdict |
|---|---|---|
| `STEERING_LOOKAHEAD` | 0.35 | **Deleted**: I-3's horizon is derived per pair |
| `STEERING_CORRIDOR_MARGIN` | 0.05 | **Kept** as I-3's clearance margin |
| `FLOCK_SHELL` | 0.08 | **Deleted** |
| `COMOVING_MIN_COSINE` | 0.5 | **Kept** (I-4) |
| `FLOCK_WEIGHT_TOUCHING` | 0.25 | **Deleted** |
| `FLOCK_WEIGHT_OVERLAPPING` | 0.35 | **Deleted** |
| `OBSTACLE_WEIGHT` | 0.85 | **Deleted** |
| `EMERGENCY_SEPARATION` | 0.85 | **Deleted** |
| `ARRIVAL_TOUCH_MARGIN` | 0.15 | **Kept** |
| `ARRIVAL_WITNESS_REACH` | 1.5 | **Kept** |
| `WAIT_THRESHOLD` | 1/8 | **Deleted** |
| `STANDING_DRIFT` | 1/64 | **Kept** (L6; C-2's "moving") |
| `FLANK_REVERSE_TICKS` | 12 | **Deleted** |
| `STALL_PROGRESS`, `STALL_REPATH_TICKS`, `STALL_ABANDON_TICKS`, `STALL_ARRIVAL_REACH` | 0.1, 25, 250, 3 | **Kept** |
| `REPATH_COOLDOWN_TICKS`, `ROUTE_CHECK_PERIOD`, `PURSUIT_ROUTE_DRIFT`, `ABANDONED_TARGET_TICKS` | 12, 8, 2, 50 | **Kept** |
| `WAYPOINT_HANDOFF_MARGIN`, `ROUTE_START_TOLERANCE`, `TERRAIN_GUARD_HORIZON` | 0.1, 0.2, 2 | **Kept** |
| `PUSH_RELAXATION` (pub) | 0.35 | **Deleted** |
| `FIXED_BODY_TANGENT_WEIGHT` | 0.85 | **Deleted** |
| `SOFT_PUSH_SPEED_SHARE`, `SOFT_PUSH_FLOOR` | 0.65, 0.015 | **Deleted** |
| `WALL_CONTACT_TOLERANCE`, `WALL_CONTACT_PASSES`, `WALL_PROBE_SLACK` | 2⁻²⁰, 3, 2⁻¹⁴ | **Kept** |

**12 of 29 are deleted**, 17 kept. The literal thresholds of the walk (half step in `surround_walk`, 2π · distance in
`walk_bound`) go with it; the half step of `after_wall_slide` stays. New values, all in
`assets/catalog/movement_rules.ron` with their derivation (§3.6): `contact.iterations`, `contact.dead_band_permille`,
`contact.moving_overlap_permille`, `contact.side_horizon_factor`, the D-β switch `contact.moving_friends_push`, and
the migration switch `contact.pipeline` (removed in WP7). The per-archetype catalog field `surround_walk` (default
true, set by no archetype) is deleted in WP7.

### 2.4 Authoritative state

`MovementMemory` (all hashed, `checksum/encoding.rs`): `arrived_at`, `progress_best`, `progress_legs`,
`progress_tail`, `stalled_ticks`, `repath_cooldown`, `abandoned_target`, `abandoned_ticks`, `flank_left`,
`corner_round`, `flank_active`, `flank_reversed`, `flank_spent`, `flank_blocked_ticks`, `flank_walked`,
`flank_retreat`, `flank_target`, `leash_origin`. The walk's six (`flank_active` to `flank_retreat`) are **deleted**
(WP7); `flank_left` and `flank_target` are **restated** as `side_left` and `side_partner` (I-3's commitment);
`slot: Option<SlotClaim>` is **added** (G-3). See §3.7 for the schema.

---

## 3. The design

### 3.1 The new tick

Unchanged outside movement. Inside it:

1. Groups and lanes, then the tick-start snapshot (as today).
2. **Slot choice** (G): a pass over the attackers that need a slot, from the snapshot, with a conflict resolution
   that does not depend on processing order (G-4).
3. **Walkers, casts, pursuits** in `EntityId` order, each from the snapshot: route upkeep, arrival, goal (the slot
   point if the unit holds a slot), **intent** (I-1 to I-4), hull gate, terrain probe and wall slide, commit of the
   proposed position `x*`.
4. **Contact** (C): the solver over all bodies, K Jacobi iterations, each followed by the terrain step; then
   `velocity = position − pos_at_tick_start`.

The proposal no longer looks at friends' last displacement to predict room; it only states where the unit wants to
be. Everything about who yields happens in one place.

### 3.2 Intent

**I-1. Preferred velocity.** `v_pref = speed_t · normalize(next − x)`, where `next` is the route point (or the slot
point, G), and `speed_t` today's accelerated speed. The snap onto a goal within one step is kept.

**I-2. Arrival braking.** Today's `brake` (`braking_speed`: the speed from which the unit stops on the goal, or on the
band's edge for a chase) is kept unchanged.

**I-3. One time-to-collision side rule per pair.** Threats are the same-class bodies in the neighbour set that are
not ignored (the chase target), are not co-moving friends (I-4), and do not yield (`yields_to`, L6): oncoming and
crossing movers of either side, and standing bodies that hold (anchored, holding, waiting under an order).
- For each threat `b`, with `p = x_b − x`, `v_rel = v_pref − v_b` (`v_b` its achieved displacement last tick, the
  snapshot's velocity) and `R = r + r_b + m` (`m` = `STEERING_CORRIDOR_MARGIN` 0.05), the time to collision is the
  smaller root of `|p − v_rel t| = R` when `p · v_rel > 0` and the discriminant is non-negative; bodies already
  within `R` are not threats (contact is the solver's job).
- The threat is the one with the smallest `t ≤ H_b`, `H_b = h · (r + r_b) / speed_t` ticks (`h` =
  `contact.side_horizon_factor`, I-3a). Equal `t`: the smaller `EntityId` (this tie needs exact equality of two
  Q32.32 roots and changes only which of two equally urgent threats is avoided first).
- **Side.** If the threat is the committed partner (`side_partner`), keep `side_left`. Otherwise choose
  `left = cross(v_rel, p) < 0` (the threat lies to the right of the relative motion, so pass it on the left) and
  commit: `side_partner = b`, `side_left = left`. **Dead ahead (`cross = 0`): pass on the right** (I-3b).
- **Heading.** The relative velocity is turned onto the tangent of the disc `(p, R)` on the chosen side,
  `t̂ = û cos α ± û⊥ sin α` with `sin α = R / |p|`, and the heading is `normalize(v_b + |v_rel| · t̂)`. One `sqrt` per
  threat (the root) and one for `cos α`; no trigonometric table.
- **Commitment ends** when the committed partner no longer closes (`p · v_rel ≤ 0`) or leaves the neighbour set and
  no threat is found; a new, earlier threat replaces the partner (one pair at a time, I-3c).

**I-3a. The horizon, derived.** A unit that turns its relative velocity by α moves sideways by `|v_rel| sin α` per tick.
Dead ahead, it needs `r + r_b` of lateral clearance. Reacting at `t = h · (r + r_b) / v` and moving sideways at
`v / h`, it clears in time with `sin α = 1 / h`; `h = 2` caps the deflection at 30°, the largest turn that keeps
most of the forward speed (cos 30° = 0.87) and is well inside every archetype's per-tick hull rate (**verify** the
slowest `turn_rate_deg_per_tick` of a ground unit against 30° over `H` ticks in WP3). A shorter horizon swerves
harder and later; a longer one reacts to bodies that would have moved away. `h` is data, not tuned on outcomes.

**I-3b. The symmetric head-on tie, without IDs.** Two bodies exactly head-on both see `cross = 0` and both pass on
their right, which is the same handedness for both, so they separate (each moves to its own right; their relative
velocities turn the same way). The rule needs no ID and is mirror-consistent: under a point reflection both bodies
still pass right. It is SC-2's "on meeting oncoming units they read the other's heading and turn the same way" and
the traffic convention. Off the exact tie, the geometric side is antisymmetric under swapping the pair
(`p → −p`, `v_rel → −v_rel`, so `cross` keeps its sign), so both bodies pick the same handedness there too. A mirror
match therefore sees no left/right bias from this rule; T9 checks it.

**I-3c. Why one pair.** Two fields (`side_partner`, `side_left`) are the whole memory: bounded state in the
checksum and no per-pair table. The kept side stops the flicker of ORCA-style per-tick side choices (VO-3, VO-4) for
the pair that matters; a second, later threat is handled when it becomes the earliest. The cost is a possible side
change when a new earliest threat appears; the harness's M1 counts it if it shows.

**I-4. Co-moving friends are ignored.** A friend whose last displacement points within 60° of `v_pref`
(`COMOVING_MIN_COSINE`) is neither a threat nor steered round; contact handles it (SC-2, ST-2, SC-5).

### 3.3 Contact

**C-1. The solver.** After every proposal is committed, let `x_i` be the bodies' positions and `s_i` their tick-start
positions. Build the pair list once: same body class, not ghosts, within `r_i + r_j + 1/4` of each other, on
connected ground (as `collision.rs` does). Each pair carries shares `(σ_i, σ_j)` from C-2. Then, K times:

```text
for each pair (i, j) with overlap o = r_i + r_j − |x_i − x_j| > δ_rest · (r_i + r_j):
    n = (x_i − x_j) / |x_i − x_j|        (coincident centres: n = (1, 0) towards the larger id, C-1a)
    Δ_i += σ_i · o · n,  c_i += [σ_i > 0]
    Δ_j −= σ_j · o · n,  c_j += [σ_j > 0]
for each body i with c_i > 0 and not fixed:
    x_i ← terrain_step(s_i, x_i, x_i + Δ_i / c_i)       (C-6)
stop early when no body moved in an iteration
```

**C-1b. The hard pass.** After the K averaged iterations, one more pass over the **hard pairs** only (enemies, and
pairs in which one share is 0: a fixed, holding or anchored partner), with each body's corrections summed and **not
averaged**, then the terrain step. Averaging lets a body pressed by a queue of friends from behind keep part of its
overlap with the enemy or the anchored body in front (the corridor's case); the hard pass gives the constraints that
D-α says are never relaxed the last word. It is order-independent for the same reason as C-1a. The prototype
measured it together with G-3's terrain check (§5.3: the corridor's largest enemy overlap 7.6 % → 2.1 %, the arc's 2.2 % → 1.0 %; still above T7's 0.5 %, see K-12).

Velocity is then `x_i − s_i`, the achieved displacement, as today.

**C-1a. Order independence.** Within an iteration every correction is computed from the positions at the
iteration's start; corrections are **sums** of Q32.32 values, which are exact integer additions, so the order of the
pair list cannot change a bit of the result (checked arithmetic: an overflow is a typed `MovementInvariant`, as
today). The division by `c_i` and the terrain step depend on body `i` alone. The early stop is a global test. Hence
the solver's output is a function of the set of bodies, not of their order, and is identical on every platform
(integer arithmetic, deterministic `sqrt`). The only ID use is the measure-zero coincident-centre direction, as in
today's `compute_push_displacement_with_mobility`.

**C-2. Shares from the push table (inverse-mass weights).** For a pair `(i, j)`, in order, first match wins:

| Case | σ_i | σ_j | Push-table source |
|---|---|---|---|
| both fixed (immobile or unpushable) | — (no pair) | | `Fixed`/`Fixed` |
| `i` fixed | 0 | 1 | one fixed |
| enemies, both holding | — (no pair) | | rule 1 |
| enemies, `i` holding | 0 | 1 | rule 2 |
| enemies | ½ | ½ | rule 3 (unrelaxed) |
| friends, both anchored (holding or engaged) | ½ | ½ | rule 5 |
| friends, `i` anchored | 0 | 1 | rule 4 (`slide_around_fixed` becomes the projection) |
| friends, `i` moving, `j` idle and standing (`yields_to`) | 0 | 1 if `p_i ≥ p_j`, else σ = (1, 0) | rule 6 |
| friends, both moving, **D-β on** | 0 / 1 by priority: the lower priority takes all; equal ½ / ½ | | rule 7, priority-weighted (SC-9) |
| friends, both moving, **D-β off** | in proportion to each body's own approach along `n` this tick (a body that did not close takes none) | | "moving units block" (SC-8) |
| friends, otherwise (waiting under orders) | ½ | ½ | rule 7 |

"Moving" is a committed step longer than `STANDING_DRIFT`; "anchored" is `cannot_be_pushed()` or
`is_anchored_in_combat`. Today's tie-breaks inside `resolve_pair` (an engaged friend is anchored only if its priority
is at least the other's, or if it is still and the other moves) are **dropped**: an engaged body is anchored against
every friend. They existed so that `slide_around_fixed`'s lane exit could not throw a higher-priority unit; with a
projection there is no throw (**verify** in WP3 that `test_higher_priority_melee_slides_around_engaged_ranged_ally`
holds; it is the case they were written for). σ is 0, ½ or 1 except under D-β off, so the shares are exact.

**C-3. The iteration count, derived.** A tick injects at most one step of new overlap per pair. Relative to the
radius sum, the worst injection is `ρ = 2 · s_max / (2 r_min)`, two bodies of the smallest radius closing at the
largest step. With averaging over at most `c = 6` contacts (the kissing number of equal discs in the plane), one
iteration removes at least the fraction `1/c` of the deepest overlap of a body whose other contacts are resolved
(**verify** as a property test, WP2). The iterations needed to bring `ρ` down to the moving bound `δ_move` are

`K = ⌈ ln(δ_move / ρ) / ln(1 − 1/c) ⌉`.

With `δ_move` = 5 % (D-α) and `ρ` taken over every pair of ground archetypes of the shipped catalog as
`(s_a + s_b) / (r_a + r_b)`, with `s` the top speed times the archetype's largest speed multiplier, the worst pair
today is two `jet_jumper` bodies (0.21 per tick, radius 0.375: ρ = 0.56; `psi_grey` with its 2.2 multiplier against
`jet_jumper` gives 0.557), so `K = ⌈ln(0.05 / 0.56) / ln(5/6)⌉ = 14`. The worst case is two bodies at full speed
head-on, which I-3 steers apart before contact; the bound is a guarantee, not the typical load.
`contact.iterations` holds K; a test of WP2 recomputes the bound from the shipped catalog and fails if the data is
smaller, so a faster unit or a smaller radius in the catalog forces the data to follow. The early stop makes the
typical cost smaller; how many iterations a tick uses is not yet measured (**verify** in WP3: a histogram over the
development corpus; §5.5's cost includes the early stop).

**C-4. Tolerances (D-α), as data.** `contact.dead_band_permille` = 10: an overlap of at most 1 % of the radius sum
is not corrected. It is the at-rest tolerance of D-α and the dead band of the survey's §4.1 item 5: a crowd standing
in contact stops moving, so no jitter of 10⁻³ per tick survives, and nothing below 1 % is ever pushed. Derivation:
1 % of the smallest radius sum (0.75) is 0.0075 world units, below a pixel at the default camera (**verify** the
pixel size in WP6's storyboard re-check). `contact.moving_overlap_permille` = 50 is D-α's moving bound, used only by
C-3's test and the harness's T7.

**C-5. No speed caps.** Today friendly correction is capped at 0.65 of speed because a relaxed spring could fling a
body. The solver only moves bodies apart by the overlap that exists, so a body moves at most by the overlaps it has,
which this tick's steps created (bounded by `ρ`) plus what earlier ticks left (bounded by `δ_move` after K
iterations). The exception is a spawn or an unload into a crowd, whose overlap may be large; WP3 adds a test that
such a body leaves within a bounded number of ticks and never through a wall (C-6), and records the jump if it is
visible.

**C-6. Terrain and structures.** Each iteration's correction goes through today's `apply_correction` sequence:
`slide_target` from the current position, `permits_segment` from the tick start, then `settle_on_terrain(s_i, x_i,
target)`. A correction that terrain refuses leaves the body where it is for that iteration; its partner keeps its
share and, in later iterations, takes the rest because the pair's overlap is still there (a body against a wall
acts as a fixed body after one iteration; geometric decay ½ per iteration for a pair). Structures stay out of the
body set: they are terrain to bodies (plan `2026-11-18-structure-contact`). Air bodies skip the terrain step and keep
the map-edge domain as today.

**C-7. Who touches whom.** Body classes, ghosts (mineral walking, suppressed collision), burrowed bodies and cliff
connectivity are today's rules, unchanged.

**C-8. Anchored, holding and enemy bodies.** Never moved by a friend (σ = 0): holding, immobile, unpushable,
engaged. Never moved by an enemy: holding, immobile, unpushable. Enemies separate evenly and are never allowed a
tolerance beyond the dead band (D-α: never with enemies): the enemy pair is corrected to the dead band within the
tick, and T7 bounds what remains. A unit pressing into an anchored line keeps the tangential part of its step,
which is the slide along the line that Q0074 asks for.

### 3.4 The concave as a goal (D-γ)

**G-1. Trigger.** A unit chasing a target (an `Attack` order, or an attack-move's acquired target) that is out of its
band and **queued** — `friend_holds_ahead` is true: an anchored friend is within one step of its edge towards its
next point — asks for a slot. A unit already holding a slot keeps it (G-5). An unblocked chase is today's chase.

**G-2. The free arc.** Let `T` be the target's tick-start position and `ρ = band_max − r/2` (inside the band by half
the unit's radius). Every snapshot body `o` (friend or enemy, same class, not the unit, not the target) whose centre
lies within `r + r_o` of the circle `|x − T| = ρ` blocks the angular interval of centre `atan2(o − T)` and half-width
`asin((r + r_o) / ρ)` (computed as `atan2(s, √(1 − s²))`, binary angles). So does every other unit's current claim
(G-3), as a body of the claimant's radius at the claimed point. A free angle is one outside every interval.

**G-3. Choice and claim.** The candidates are the unit's own bearing from `T` and the two outer ends of every
blocked interval. The chosen slot is the free candidate with the smallest angular distance from the bearing; equal
distance goes to the unit's right (clockwise), the right-hand rule of I-3b, so no ID decides. The claim is
`SlotClaim { target, point }` in `MovementMemory`, `point = T + ρ (cos θ, sin θ)`. A slot point beyond the leash
(from `leash_origin`) or outside the unit's terrain domain is not a candidate.

**G-4. Determinism under contention.** Claims are made in the slot pass before the walkers, from the tick-start
snapshot and last tick's claims. Two claimants may pick overlapping points on the same tick. Conflicts are resolved
per target by sorting the new claims by (angular distance from the claimant's bearing, distance to `T`, `EntityId`)
and admitting them in that order, each admitted claim blocking its interval for the rest; a refused claimant tries
again next tick. The result depends on the set of claimants and the snapshot, not on iteration order; the ID is the
last key and decides only exact ties of two Q32.32 distances.

**G-5. Kept, freed and re-chosen.** A claim is kept until: the unit is in band (then it is a body on the arc and
needs no claim: freed); the claimed interval is blocked by a body at tick start (taken: re-chosen at once); the
target's tick-start position is more than `r` from the claim's circle (`| |point − T| − ρ | > r`: re-chosen); the
order or target changes, or the unit dies (freed with the memory, `restart()`).

**G-6. No free slot: stand.** A unit with no free slot proposes no motion (velocity 0, `Idle`), keeps its order and is
not stalled (Q0089's rule, today's `queued`), and asks again every tick at no extra cost beyond G-2's scan. It stands
outside the concave, as SC-10 says the game's units do. A unit walking to its slot does not accumulate stall ticks
(the role `flank_active` has in `check_pursuit_progress` today).

**G-7. Routes, lanes and the leash.** The slot point is the chase's goal: the route to it is a pursuit route
(`reroute_pursuit`, the same TA* and funnel); group lanes apply to group Move and AttackMove destinations only, not to
pursuit routes, so they are unaffected (verified: lanes are applied in `upkeep_route` through `GroupedSearch`, and
`reroute_pursuit` does not use it). The leash is measured from `leash_origin` as today; G-3 never chooses a point
beyond it.

### 3.5 Stall rules

Unchanged except: `flank_active` in `check_progress` and `check_pursuit_progress` becomes "holds a slot"; the walk's
re-arm in `note_queued_progress` goes. A plain `Move` that makes no progress for 10 s is still given up (SC-8 fact 9,
`a_blocked_move_is_still_given_up`).

### 3.6 Data

`assets/catalog/movement_rules.ron` gains a `contact` section, read by `MovementRules` and validated at load:

```ron
contact: (
    // Migration switch (WP2 to WP6): Legacy is today's pipeline, Solver this plan's. Removed in WP7.
    pipeline: Legacy,
    // C-3: Jacobi iterations per tick; at least ceil(ln(moving / rho) / ln(5/6)) for the catalog's rho.
    iterations: 14,
    // C-4, D-α at rest: overlaps of at most this many thousandths of the radius sum are not corrected.
    dead_band_permille: 10,
    // D-α while moving: the bound `iterations` is derived for (thousandths of the radius sum).
    moving_overlap_permille: 50,
    // I-3a: a threat counts from `factor · (r + r_b) / speed` ticks before contact (2: at most a 30° turn).
    side_horizon_factor: 2,
    // D-β: moving friends push each other by priority (true) or block, each yielding its own approach (false).
    moving_friends_push: true,
),
```

Validation: `iterations` ≥ 1, `0 < dead_band_permille < moving_overlap_permille < 1000`, `side_horizon_factor` ≥ 1.
The catalog-derived lower bound on `iterations` is a test (WP2), not a load-time refusal, so a catalog edit fails CI
rather than the game at start-up.

### 3.7 Checksum schema

- **WP2: 32 → 33.** (`main` is at schema 32 since `669b100c`, resource footprints.) `MovementMemory` gains `side_partner: Option<EntityId>`, `side_left: bool` (written only by the
  Solver pipeline; under Legacy they stay `None`/`false`) and `slot: Option<SlotClaim>` (`target: EntityId`,
  `point: SimVec2`). Encoded after `leash_origin`. Every golden checkpoint changes; no behaviour does (render
  snapshots byte-identical, as for flipbook WP2's schema change).
- **WP7: 33 → 34.** The walk's six fields and `flank_left`/`flank_target` are deleted (the Solver pipeline never
  wrote them). Every checkpoint changes again; no behaviour does.
- The `contact` section of `movement_rules.ron` is catalog data. Catalog data is not hashed (at `53a0387f`) (neither unit
  values nor `group_routing` and `pursuit` are; verified: no checksum encoder reads `MovementRules`), because both
  peers load the same catalog; the switch has the same status as every unit statistic. No schema change for it.

### 3.8 Platforms

All arithmetic is Q32.32 with checked operations and `SimScalar::sqrt`; angles in G-2 are `SimAngle` binary angles
from `atan2`. No float, no hash map, no clock. The solver's buffers are retained in `MovementWorkspace` (no per-tick
allocation in steady state, as `pair_scratch` is today). Determinism is checked by the golden replay on the four
builds (native debug and release, `wasm32-wasip1` release and debug under `wasmtime --dir=<real path>`), and by the
harness's relabelling property (T8).

### 3.9 Cost

Pairs per tick ≈ 400 units × 8 contacts / 2 = 1,600; × K = 14 worst case ≈ 21k pair projections of a few dozen
integer operations each, plus per-iteration terrain steps for moved bodies only. The early stop ends an iteration loop in which nothing moved (its typical length is a WP3 measurement). The TTC rule replaces `side_blockers` and `remove_blocked_velocity`, both linear in
neighbours, by one linear pass. The slot pass scans the bodies near each claimant's target circle, only for
claimants. §5.5 gives the prototype's instructions retired; WP6 judges throughput by instructions retired on the
release benchmark, per scenario.

---

## 4. Deviations from StarCraft II

All three are **sanctioned**: the owner answered Q0092 (a) on 2026-11-20, and they are listed in the addendum of
[Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) (`445ed008`). Under the standing rule Q0051 they are
therefore not bugs; their bounds are data and are checked by T7 (D-α) and by the switch's tests (D-β).

| ID | Deviation | Data | Why it is simpler | Evidence for or against the game |
|---|---|---|---|---|
| **D-α** | Friends may overlap transiently: at most 5 % of the radius sum while moving and 1 % at rest. Never enemies beyond the dead band, never terrain | `contact.dead_band_permille` 10, `contact.moving_overlap_permille` 50 (and `iterations` derived from it) | One solver with a residual instead of a hard constraint at proposal time plus a soft one after it; no waiting rule, no sliver waits, no standing-drift classes against bodies | The owner's statement (Anhalt, GDC 2011): iterative soft separation with slight temporary friendly overlap. SC-6, SC-7 (clumping): the game's friends overlap separation discs; no source gives a bound. Today's pipeline is not stricter in practice: §5.2 measures 11.6 % of moving near pair-ticks above 5 % and maxima of 72 % on the corpus |
| **D-β** | Moving friends push each other by priority; equal priority shares | `contact.moving_friends_push` (true: push; false: each body yields its own approach) | No prediction of who recedes (the receding allowance) | SC-5 says friends are pushed; SC-8 says moving units block. The switch keeps both; the default follows SC-5 and SC-9 (5.0.14's Ultralisk priority change "pushes allied units out of the way when moving") |
| **D-γ** | The concave is reached by choosing a free slot on the weapon arc, not by a walk mode | none (a migration switch only until WP7) | One goal choice instead of a six-field state machine with start, side, reversal, circumference, retreat and re-arm rules | Q0074 described the visible result (second-row units slide along the front line and stand outside the concave); SC-10 says units outside the concave stand. The mechanism is not sourced either way; the owner's statement (soft separation and pushing) is consistent with the slide along the front line falling out of contact |

---

## 5. Measurements: baseline and prototype

All figures: development range (seeds 1 to 1,000 per family, each family also relabelled; mirror seeds 1 to 1,000),
scratch harness and scratch builds as described below, measured one run at a time on the shared server. Nothing of
the acceptance or held-out ranges has been run. Scripts, binaries' patch and CSVs:
`~/scratch/contact-solver/` (`run.sh`, `scripts/analyse.py`, `scripts/table.py`, `proto.diff`, `runs/*`); none is
committed.

### 5.1 Base and method

- **Harness.** A scratch implementation of §6 (`crates/sim_core/examples/movement_harness.rs` in a scratch worktree,
  never committed). Its output for today's pipeline is **byte-identical on `53a0387f` and on `445ed008`** for every
  family, relabelled corpus and the mirror (the corpus uses flat generated maps without resource nodes, and nothing
  else in the merged change touches the local layer). The baseline below is therefore the baseline of `445ed008`.
- **Placebos** (§6.4): P1 adds one raw unit to `PUSH_RELAXATION`; P2 to both `PUSH_RELAXATION` and
  `STEERING_LOOKAHEAD` (§6.6 C-b); P1 rebuilt on `445ed008` gives P1's figures again.
- **The prototype** is the patch `proto.diff` behind an environment switch: with the switch off it reproduces
  today's CSVs byte for byte (checked on the gap family and its relabelled corpus on both bases).

### 5.2 Baseline: today's pipeline against the pre-registered thresholds

**gap**

| Metric | Today | P1 | P2 |
|---|---:|---:|---:|
| M1 per 1,000 moving unit-ticks | 15.38 | 15.32 | 15.33 |
| M1 95 % CI | [14.58, 16.18] | [14.49, 16.14] | [14.49, 16.17] |
| M1-hi | 3.31 | 3.34 | 3.29 |
| M2 wasted share | 0.0576 | 0.0577 | 0.0577 |
| M3 standing waste per unit | 0.017 | 0.018 | 0.019 |
| M4 progress violations % | 0.43 | 0.42 | 0.42 |
| Deadlocked units (passable) | 337 | 349 | 361 |
| Give-ups (passable) | 334 | 344 | 358 |
| Scenarios complete % | 83.1 | 82.9 | 82.7 |
| Units complete % | 97.60 | 97.53 | 97.43 |
| Impassable: wrong passes | 0 | 0 | 0 |
| η median | 0.485 | 0.483 | 0.483 |
| η 5th percentile | 0.240 | 0.240 | 0.240 |
| Monotonicity drops | 0 | 0 | 0 |
| Friend overlap >5 % moving, % of near pair-ticks | 11.61 | 11.62 | 11.60 |
| Friend overlap >1 % standing, % | 5.06 | 5.07 | 5.04 |
| Friend overlap max | 0.720 | 0.720 | 0.720 |
| Enemy overlap max | 0.0000 | 0.0000 | 0.0000 |
| Terrain penetration max | 0.00005 | 0.00005 | 0.00005 |
| Relabel sign test p (M1) | 0.290 | 0.113 | 0.208 |
| Relabel sign test p (completion tick) | 0.594 | 0.617 | 0.242 |

**cross**

| Metric | Today | P1 | P2 |
|---|---:|---:|---:|
| M1 | 5.89 | 5.90 | 5.92 |
| M1 95 % CI | [5.45, 6.35] | [5.48, 6.36] | [5.49, 6.37] |
| M1-hi | 0.98 | 0.98 | 0.98 |
| M2 | 0.0433 | 0.0433 | 0.0433 |
| M3 | 0.001 | 0.001 | 0.001 |
| M4 % | 0.00 | 0.00 | 0.00 |
| Deadlocked units | 73 | 75 | 75 |
| Give-ups | 73 | 75 | 75 |
| Scenarios complete % | 96.6 | 96.5 | 96.5 |
| Units complete % | 99.60 | 99.59 | 99.59 |
| Friend >5 % moving % | 5.67 | 5.68 | 5.68 |
| Friend >1 % standing % | 1.49 | 1.49 | 1.49 |
| Friend max | 0.599 | 0.599 | 0.599 |
| Relabel p (M1) | 0.522 | 0.711 | 0.662 |
| Relabel p (completion tick) | 0.000 | 0.000 | 0.000 |

**arc**

| Metric | Today | P1 | P2 |
|---|---:|---:|---:|
| M1 | 3.52 | 3.51 | 3.52 |
| M1 95 % CI | [3.39, 3.66] | [3.38, 3.65] | [3.39, 3.65] |
| M1-hi | 2.19 | 2.18 | 2.18 |
| M2 | 0.1031 | 0.1031 | 0.1032 |
| M3 | 2.221 | 2.228 | 2.227 |
| M4 % | 4.80 | 4.80 | 4.81 |
| Fill mean | 0.982 | 0.982 | 0.982 |
| Fill 5th percentile | 0.900 | 0.900 | 0.900 |
| Give-ups | 11 | 15 | 13 |
| Friend >5 % moving % | 1.36 | 1.36 | 1.37 |
| Friend >1 % standing % | 0.22 | 0.22 | 0.22 |
| Friend max | 0.674 | 0.674 | 0.674 |
| Enemy overlap max | 0.0190 | 0.0190 | 0.0249 |
| Relabel p (M1) | 0.675 | 0.518 | 0.648 |
| Relabel p (completion tick) | 0.060 | 0.051 | 0.051 |

**corridor**

| Metric | Today | P1 | P2 |
|---|---:|---:|---:|
| M1 | 8.76 | 9.43 | 9.06 |
| M1 95 % CI | [6.36, 11.46] | [6.84, 12.34] | [6.47, 12.02] |
| M1-hi | 3.13 | 3.28 | 3.07 |
| M2 | 0.0528 | 0.0532 | 0.0527 |
| M3 | 0.503 | 0.512 | 0.495 |
| M4 % | 0.64 | 0.63 | 0.63 |
| Fill mean | 1.020 | 1.015 | 1.015 |
| Fill 5th percentile | 0.500 | 0.500 | 0.500 |
| Give-ups | 564 | 561 | 560 |
| Friend >5 % moving % | 5.88 | 6.00 | 5.93 |
| Friend >1 % standing % | 3.82 | 3.73 | 3.73 |
| Friend max | 0.963 | 0.963 | 0.963 |
| Enemy overlap max | 0.0471 | 0.0471 | 0.0471 |
| Relabel p (M1) | 0.000 | 0.001 | 0.000 |
| Relabel p (completion tick) | 1.000 | 0.464 | 0.550 |
| Terrain penetration max | 0.00009 | 0.00009 | 0.00009 |

**mirror** (blue wins / red wins / draws; blue share of decisive [95 % Wilson])

| Run | Mirror |
|---|---|
| Today | 295 / 403 / 302; 0.423 [0.387, 0.460] |
| P1 | 295 / 403 / 302; 0.423 [0.387, 0.460] |
| P2 | 294 / 404 / 302; 0.421 [0.385, 0.458] |

**Against the thresholds** (§6.5; development range, informative only: thresholds bind on the acceptance and
held-out ranges). "Today" is `445ed008`; "Prototype" is the final prototype of §5.3 (`final`).

| ID | Threshold | Today | Prototype |
|---|---|---|---|
| T1 | M1 ≤ 1.0 in every family; M1-hi ≤ 0.25 | **Fails**: M1 15.38 / 5.89 / 3.52 / 8.76 (gap / cross / arc / corridor); M1-hi 3.31 / 0.98 / 2.19 / 3.13 | **Fails**: M1 1.18 / 2.26 / 3.39 / 5.95; M1-hi 0.21 / 0.98 / 0.12 / 1.05 (gap and arc meet M1-hi). Not above today in any family |
| T2 | M2 not above today; M3 ≤ 0.5 | M3 fails on arc (2.22) and corridor (0.503) | M2 below today on gap, cross, arc; **above on corridor** (0.0637 vs 0.0528); M3 holds everywhere (≤ 0.389) |
| T3 | M4 ≤ 1 % | **Fails** on arc (4.80 %) | Holds (≤ 0.54 %) |
| T4 | 0 deadlocks and give-ups with room; ≥ 99.5 % complete | **Fails**: gap 337 units (83.1 % of scenarios complete), cross 73 (96.6 %) | **Fails**: gap 258 (85.9 %), cross 4 (99.7 %) |
| T5 | Arc fill mean ≥ 0.90, 5th percentile ≥ 0.7476 | Holds (0.982, 0.900) | Mean holds (0.902); **5th percentile fails** (0.682) |
| T6 | η median ≥ 0.5, 5th ≥ 0.25, monotone | **Fails** (0.485, 0.240); monotone | Holds (0.658, 0.323); monotone |
| T7 | Enemy ≤ 0.5 %; terrain ≤ 2⁻¹⁰; friends > 5 % moving ≤ 0.1 % of near pair-ticks, > 1 % standing ≤ 0.1 %, max ≤ 15 % | **Fails**: enemy 4.7 %; friends 11.6 % / 5.1 % (gap), max 96 % | **Fails**: enemy 2.1 %; friends 0.83 % / 0.15 % (gap), 6.1 % / 34 % (corridor), max 100 % (squeezed pairs, K-12) |
| T8 | Sign tests p ≥ 0.01 | **Fails**: cross completion tick p < 0.001; corridor M1 p < 0.001 | Holds (smallest p 0.080) |
| T9 | Mirror Wilson interval contains 0.5 | **Fails**: 0.423 [0.387, 0.460] | **Fails**: 0.443 [0.404, 0.481]; holds with the pursuit read of §5.4 |

**What today's pipeline shows that the plans' battle matrix did not:**
- **Visible reversals** are 15.4 per 1,000 moving unit-ticks at gaps, 8.8 in the corridor (95 % intervals well clear of
  each other and of the placebos); the legacy meter (cosine −0.3, any amplitude) counts 291,637 on the gap corpus.
- **Give-ups with room:** 73 units in 34 crossing scenarios on open ground give their `Move` up; in the gap family 315
  of 337 come from mixed-priority scenarios (147 of 230 such scenarios), all after passing the gap, at the shared
  destination (K-10).
- **ID bias:** the crossing family's completion ticks differ between a scene and its relabelled twin more often in one
  direction (413 against 538, p < 0.001), and blue, spawned first, wins 42.3 % of decisive mirror matches.
- **Overlap:** today's friends overlap more than D-α allows: 11.6 % of near moving pair-ticks at gaps exceed 5 %.
- **Placebos** move no aggregate beyond its bootstrap interval; one or two raw units change counts by up to 8 % (corridor M1, gap give-ups), the noise any single count carries.

### 5.3 The prototype

A scratch patch (`~/scratch/contact-solver/proto.diff`, 300 lines over `collision.rs`, `locomotion.rs`,
`steering.rs`, a new `movement/proto.rs`) behind `SS_PROTO=1`:
- **Intent:** I-1 to I-4 as designed (one committed pair, right-hand tie), stored in the walk's `flank_target` and
  `flank_left` fields; `remove_blocked_velocity`, the walk, the waiting rule and the crowded wall-slide wait skipped.
- **Contact:** C-1 to C-2 and C-6 as designed (Jacobi with averaging, early stop, today's terrain step), pair list
  built once with a 1/4 slack; K = 10, dead band 1 %, D-β on unless stated.
- **Slot sketch:** G-1 trigger and G-2/G-3 free-interval choice as designed, kept while free, stored in other walk
  fields; **no claim resolution (G-4) and no slot route** (the unit walks straight at its slot). The `final` variant adds
  G-3's terrain check of the slot point and C-1b's hard pass, with K = 14.

Variants (columns): `noslot` intent and solver without the slot sketch; `full` with it; `K14` with K = 14; `final` K = 14
with the hard pass and the slot terrain check; `betaoff` `full` with D-β off.

**gap**

| Metric | Today | noslot | full | K14 | final | betaoff |
|---|---:|---:|---:|---:|---:|---:|
| M1 per 1,000 moving unit-ticks | 15.38 | 1.24 | 1.24 | 1.16 | 1.18 | 6.52 |
| M1 95 % CI | [14.58, 16.18] | [0.99, 1.54] | [0.99, 1.54] | [0.94, 1.42] | [0.97, 1.43] | [5.28, 7.97] |
| M1-hi | 3.31 | 0.21 | 0.21 | 0.18 | 0.21 | 0.36 |
| M2 wasted share | 0.0576 | 0.0264 | 0.0264 | 0.0269 | 0.0270 | 0.0414 |
| M3 standing waste per unit | 0.017 | 0.002 | 0.002 | 0.002 | 0.003 | 0.022 |
| M4 progress violations % | 0.43 | 0.53 | 0.53 | 0.53 | 0.54 | 0.49 |
| Deadlocked units (passable) | 337 | 241 | 241 | 259 | 258 | 420 |
| Give-ups (passable) | 334 | 241 | 241 | 257 | 260 | 428 |
| Scenarios complete % | 83.1 | 86.6 | 86.6 | 86.2 | 85.9 | 80.3 |
| Units complete % | 97.60 | 98.33 | 98.33 | 98.20 | 98.19 | 96.92 |
| Impassable: wrong passes | 0 | 0 | 0 | 0 | 0 | 0 |
| η median | 0.485 | 0.667 | 0.667 | 0.659 | 0.658 | 0.466 |
| η 5th percentile | 0.240 | 0.325 | 0.325 | 0.323 | 0.323 | 0.230 |
| Monotonicity drops | 0 | 0 | 0 | 0 | 0 | 0 |
| Friend overlap >5 % moving, % of near pair-ticks | 11.61 | 1.26 | 1.26 | 0.80 | 0.83 | 0.26 |
| Friend overlap >1 % standing, % | 5.06 | 0.16 | 0.16 | 0.14 | 0.15 | 0.05 |
| Friend overlap max | 0.720 | 0.709 | 0.709 | 0.823 | 0.995 | 0.782 |
| Enemy overlap max | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 0.0000 | 0.0000 |
| Terrain penetration max | 0.00005 | 0.00005 | 0.00005 | 0.00005 | 0.00005 | 0.00005 |
| Relabel sign test p (M1) | 0.290 | 0.707 | 0.707 | 0.747 | 0.295 | 0.238 |
| Relabel sign test p (completion tick) | 0.594 | 0.776 | 0.776 | 1.000 | 1.000 | 1.000 |

**cross**

| Metric | Today | noslot | full | K14 | final | betaoff |
|---|---:|---:|---:|---:|---:|---:|
| M1 | 5.89 | 2.24 | 2.24 | 2.26 | 2.26 | 10.03 |
| M1 95 % CI | [5.45, 6.35] | [2.12, 2.36] | [2.12, 2.36] | [2.14, 2.37] | [2.14, 2.37] | [9.49, 10.55] |
| M1-hi | 0.98 | 0.97 | 0.97 | 0.98 | 0.98 | 2.20 |
| M2 | 0.0433 | 0.0311 | 0.0311 | 0.0315 | 0.0315 | 0.0484 |
| M3 | 0.001 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 |
| M4 % | 0.00 | 0.40 | 0.40 | 0.39 | 0.39 | 0.19 |
| Deadlocked units | 73 | 0 | 0 | 5 | 4 | 0 |
| Give-ups | 73 | 0 | 0 | 5 | 4 | 0 |
| Scenarios complete % | 96.6 | 100.0 | 100.0 | 99.6 | 99.7 | 100.0 |
| Units complete % | 99.60 | 100.00 | 100.00 | 99.97 | 99.98 | 100.00 |
| Friend >5 % moving % | 5.67 | 0.20 | 0.20 | 0.04 | 0.05 | 0.05 |
| Friend >1 % standing % | 1.49 | 0.01 | 0.01 | 0.01 | 0.01 | 0.00 |
| Friend max | 0.599 | 0.374 | 0.374 | 0.289 | 0.432 | 0.332 |
| Relabel p (M1) | 0.522 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| Relabel p (completion tick) | 0.000 | 0.508 | 0.508 | 0.454 | 0.607 | 1.000 |

**arc**

| Metric | Today | noslot | full | K14 | final | betaoff |
|---|---:|---:|---:|---:|---:|---:|
| M1 | 3.52 | 13.67 | 4.93 | 3.10 | 3.39 | 11.58 |
| M1 95 % CI | [3.39, 3.66] | [12.39, 14.95] | [3.54, 6.56] | [2.36, 4.03] | [2.51, 4.45] | [9.54, 13.72] |
| M1-hi | 2.19 | 0.37 | 0.21 | 0.13 | 0.12 | 0.36 |
| M2 | 0.1031 | 0.2107 | 0.0626 | 0.0625 | 0.0610 | 0.0992 |
| M3 | 2.221 | 1.284 | 0.107 | 0.114 | 0.104 | 0.270 |
| M4 % | 4.80 | 0.03 | 0.01 | 0.01 | 0.01 | 0.02 |
| Fill mean | 0.982 | 0.613 | 0.903 | 0.903 | 0.902 | 0.879 |
| Fill 5th percentile | 0.900 | 0.391 | 0.667 | 0.692 | 0.682 | 0.625 |
| Give-ups | 11 | 5111 | 259 | 257 | 289 | 560 |
| Friend >5 % moving % | 1.36 | 2.99 | 0.85 | 0.24 | 0.41 | 0.21 |
| Friend >1 % standing % | 0.22 | 18.73 | 1.02 | 0.57 | 0.34 | 0.09 |
| Friend max | 0.674 | 0.619 | 0.498 | 0.508 | 0.460 | 0.582 |
| Enemy overlap max | 0.0190 | 0.0178 | 0.0390 | 0.0215 | 0.0099 | 0.0158 |
| Relabel p (M1) | 0.675 | 0.112 | 0.500 | 0.302 | 0.422 | 1.000 |
| Relabel p (completion tick) | 0.060 | 0.647 | 0.310 | 0.122 | 0.080 | 1.000 |

**corridor**

| Metric | Today | noslot | full | K14 | final | betaoff |
|---|---:|---:|---:|---:|---:|---:|
| M1 | 8.76 | 11.14 | 13.47 | 12.48 | 5.95 | 37.44 |
| M1 95 % CI | [6.36, 11.46] | [8.21, 14.79] | [8.60, 19.01] | [7.73, 17.83] | [3.47, 9.03] | [27.21, 48.38] |
| M1-hi | 3.13 | 1.20 | 0.96 | 0.31 | 1.05 | 0.58 |
| M2 | 0.0528 | 0.0843 | 0.0734 | 0.0697 | 0.0637 | 0.0684 |
| M3 | 0.503 | 0.898 | 0.497 | 0.433 | 0.389 | 0.416 |
| M4 % | 0.64 | 0.49 | 0.16 | 0.16 | 0.15 | 0.14 |
| Fill mean | 1.020 | 1.001 | 1.001 | 0.982 | 0.916 | 0.965 |
| Fill 5th percentile | 0.500 | 0.667 | 0.667 | 0.500 | 0.500 | 0.500 |
| Give-ups | 564 | 2814 | 815 | 645 | 619 | 446 |
| Friend >5 % moving % | 5.88 | 8.39 | 9.31 | 5.81 | 6.10 | 0.53 |
| Friend >1 % standing % | 3.82 | 50.11 | 53.94 | 46.91 | 34.06 | 1.93 |
| Friend max | 0.963 | 0.999 | 1.000 | 0.986 | 1.000 | 0.999 |
| Enemy overlap max | 0.0471 | 0.4768 | 0.1167 | 0.0760 | 0.0208 | 0.0078 |
| Relabel p (M1) | 0.000 | 0.447 | 0.915 | 0.188 | 0.488 | 0.130 |
| Relabel p (completion tick) | 1.000 | 0.375 | 1.000 | 1.000 | 1.000 | 1.000 |
| Terrain penetration max | 0.00009 | 0.00018 | 0.00019 | 0.00020 | 0.00037 | 0.00014 |

**mirror** (blue wins / red wins / draws; blue share of decisive [95 % Wilson])

| Run | Mirror |
|---|---|
| Today | 295 / 403 / 302; 0.423 [0.387, 0.460] |
| noslot | 279 / 351 / 370; 0.443 [0.405, 0.482] |
| full | 281 / 357 / 362; 0.440 [0.402, 0.479] |
| K14 | 280 / 353 / 367; 0.442 [0.404, 0.481] |
| final | 281 / 354 / 365; 0.443 [0.404, 0.481] |
| betaoff | 286 / 356 / 358; 0.445 [0.407, 0.484] |

**Readings.**
- **The solver and intent do what the survey predicted where they apply.** Against today, visible reversals fall
  13× at gaps (15.38 → 1.18) and 2.6× in crossings; wasted path halves at gaps; overlap above D-α's moving bound
  falls from 11.6 % to 0.8 % of near pair-ticks; chokepoint throughput rises by a third (η 0.485 → 0.658) and stays
  monotone; crossings complete in 99.7 % of scenarios (96.6 %); the relabelling bias disappears in every family; the
  arc's standing waste falls 20× (2.22 → 0.10) and its progress violations vanish.
- **The concave needs the whole of G.** Without slots the arc collapses (5,111 give-ups, fill 0.61); the sketch
  restores the mean fill (0.90) but not the tail (0.68), and still gives up 289 chases (K-13).
- **The corridor is the hardest family for the solver** (standing friend overlap above 1 % on 34 % of near pair-ticks,
  M2 above today): attackers queued in a one-unit-wide corridor push into a fighting front that cannot move, and the
  dead band does not hold a queue still under pressure from behind. WP3 must classify it before WP4 (K-12).
- **K = 14 against K = 10** changes little except overlap, as C-3 predicts (moving friend overlap above 5 %: 1.26 % →
  0.80 % at gaps); the early stop keeps the cost down.
- **D-β off is worse on every family** (M1 6.52 / 10.03 / 11.58 / 37.44): letting each body yield only its own approach
  makes moving friends block each other and reverse. The default stays on, as Q0092 sanctioned.
- **The arrival chain was rejected.** Letting a cluster-arrival witness be anywhere, not within 1.5 of the goal
  (an attempt at K-10), cuts give-ups at gaps (241 → 21) but ends 486 units' orders before the gap, where the clump's
  tail reaches back (scenarios complete 86.6 % → 74.6 %); on today's pipeline it is worse still (60.0 %).
- **Not good enough yet:** T1 (cross, arc, corridor), T4 (mixed-priority give-ups), T5's tail, T7 and T9 fail on the
  prototype. Each failure has a named cause and a work package that owns it (WP3 K-12 and K-10, WP4 K-13, WP5 §5.4); the
  thresholds stay as committed.

### 5.4 Mirror asymmetry: the pursuit's live read

`prepare_pursuit` reads the target's **live** position: a pursuer processed after its target this tick (a higher
`EntityId`) sees where the target has already moved. In the mirror family blue's units are spawned first, so red's
pursuers always see blue's new positions. A scratch switch that reads the target's tick-start position from the
movement snapshot instead:

| Pipeline | Live read (today) | Tick-start read |
|---|---|---|
| Today's | 295 / 403 / 302; 0.423 [0.387, 0.460] | 204 / 180 / 616; 0.531 [0.481, 0.581] |
| Prototype (`full`) | 281 / 357 / 362; 0.440 [0.402, 0.479] | 37 / 31 / 932; 0.544 [0.427, 0.657] |

(blue wins / red wins / draws; blue's share of decisive games [95 % Wilson].) With the tick-start read both pipelines
meet T9, and the mirror fights become symmetric enough that most end in a mutual kill (a draw). **The hypothesis holds:
the ID bias of the mirror is the pursuit's order-dependent read, not contact.** WP5 makes the fix; T9's power then
rests on the decisive games, so WP5's acceptance also reports the draw rate. (This is a separate defect from the
contact redesign and is registered as a new row, §8.2.)

### 5.5 Cost

Release benchmark, the 64 × 64 and showcase-canyon scenarios with 400 units, one repeat each, `perf stat -e
instructions:u` under `flock ~/scratch/gate.lock` (scratch bench table, on `445ed008` plus the prototype):

| | Today | Prototype (`final` settings) |
|---|---:|---:|
| Instructions, both scenarios | 28.58 G | 39.58 G (+38 %) |
| Ticks per second, 64 × 64 / canyon | 161 / 123 | 114 / 91 |

Both stay well above real time (25). The prototype allocates its pair list and accumulators every tick and reads
environment switches in hot paths (the arrival and pursuit switches per call); WP3's retained buffers and catalog
values remove that. WP6's stop at +10 % per scenario is therefore a real constraint (K-8), measured, not assumed.

---

## 6. The evaluation harness (WP1, no behaviour change)

### 6.1 Seed ranges

| Range | Seeds per family | Use |
|---|---|---|
| Development | 1 to 1,000 | Every measurement during development, the baseline of this plan, every prototype |
| Acceptance | 100,001 to 101,000 | Once per work package that changes behaviour, at its acceptance; never for tuning |
| Held-out | 9,000,001 to 9,001,000 | Once, at the switch-over work package, for both pipelines side by side; nobody runs it before |

Mirror matches use the same three ranges (1,000 seeds each, at least the 500 the survey asks for).

### 6.2 Scenario families with known answers

Every scenario is generated from `(family, seed)` by a SplitMix64 stream; every coordinate and size lies on a 1/64
lattice, so the conversion to `SimScalar` is exact and the corpus is identical on every platform. Units are scratch
archetypes generated per scenario (charter §3.3: no shipped identity is named); radius `r` is drawn from
{0.375, 0.5, 0.5625, 0.625, 0.75, 0.875, 1.0}, speed `v` from {0.1, 0.125, 0.15625, 0.1875} world units per tick.

| Family | Scene | Known answer |
|---|---|---|
| **gap** | A one-cell wall across a 48 × 48 map with a gap of `w` ∈ {1, …, 6} cells, `w / 2r` in [0.7, 4.5] and never within 6 % of 1 (the predicate's rounding band); 6 to 24 units in a block, a quarter of the scenarios mixed with a second archetype of 0.75 `r` and push priority 20; one `Move` to a point beyond the wall | Passable iff `w > 2r`. Passable: every unit is past the wall with its order complete by the bound `2 · (50 / v + n · 2r / (lanes · v)) + 100` ticks, `lanes = ⌊(w − 2r) / 2r⌋ + 1`. Impassable: nobody passes |
| **arc** | An immobile enemy pillar of radius `P` ∈ {0.5, 1, 1.5, 2, 2.5} at the centre; 4 to 30 attackers with range `R` ∈ {0.1, 0.5, 1, 2, 4}, 10 to 16 units away; one `Attack` on the pillar | Ring capacity at the band's outer edge, `C = ⌊π / asin(r / (P + r + R))⌋`, is a lower bound on how many fit in range; `min(k, C)` attackers reach range by the bound `2 · d / v + 500` ticks; the others stand |
| **cross** | Two friendly groups of 4 to 14, each unit with its own `Move` through the centre, crossing at 90°, 135° or 180° | Every unit completes its order by the bound `2 · (24 + 2n · spacing) / v + 100` ticks |
| **corridor** | A corridor of `w` ∈ {2, 3, 4} cells, no way round, plugged by an enemy pillar; 3 to 11 attackers with range 0.25 to 1 attack it | `min(k, ⌊w / (2r + 1/16)⌋)` attackers reach range; the rest queue, keep the order and stand |
| **mirror** | Random compositions of one or two shipped ground armies (supply 1 to 4 each), spawned on a point mirror of a 64 × 64 map; both attack-move, and every 100 ticks re-attack-move to the other side's centroid; random weapon delays removed; 3,000-tick limit | Blue (spawned first, lower entity IDs) wins half the decisive games |

Every scenario family except mirror is also run **relabelled**: the same scene with the spawn order shuffled by a
seeded permutation, which permutes the entity IDs and nothing else.

### 6.3 Metrics

All metrics are measured outside the simulation, from positions after each tick, in `f64`. A unit's step is its
achieved displacement over one tick.

| ID | Metric | Definition |
|---|---|---|
| M1 | Visible reversals | Two consecutive steps both at least `v / 4` long with cosine below −0.5; per 1,000 moving unit-ticks (a moving tick: step above `v / 8`). M1-hi counts those with both steps at least `v / 2` |
| M2 | Wasted path | Over each one-second window: path length minus net displacement; summed and divided by total path length |
| M3 | Standing waste | Over the last 250 ticks of each scenario: path length minus net displacement per unit, world units. Units that should stand (no room, or arrived) should have none |
| M4 | Progress | Potential `Φ`: geodesic distance to the goal (gap: through the gap; cross: straight), or distance to the weapon band (arc, corridor). A violation is a one-second window, under an unchanged order, in which `Φ` rises by more than the unit's radius; share of such windows |
| M5 | Completion and deadlock | Units that complete (passable gap, cross) or reach range (arc, corridor) by the bound, against the known answer; a **deadlock** is a unit with room that has not completed by the bound and has not moved for the last 50 ticks; give-ups (`NoProgress`, `PursuitAbandoned`) |
| M6 | Arc fill | In range at the end over `min(k, C)` |
| M7 | Chokepoint throughput | Passable gaps: units per second through the gap between the first and the last unit through, divided by the capacity model `lanes · 25v / 2r` (η); and its dependence on `w` at fixed `r` |
| M8 | Overlap | Friend overlap `(r_a + r_b − d) / (r_a + r_b)`: maximum, and the share of near pair-ticks above 5 % while either moves and above 1 % while both stand (step below `v / 8`); enemy overlap with the pillar; terrain penetration of the disc into blocked cells and the map edge |
| M9 | Relabelling invariance | Per family, the original and the relabelled corpus: difference of every aggregate in M1 to M8, and a two-sided sign test on the paired per-scenario values of M1 and the mean completion tick |
| M10 | Mirror symmetry | Blue's share of decisive games, with its 95 % Wilson interval; draws reported |

### 6.4 Noise: placebo builds

For the baseline, two placebo builds of today's pipeline each add one raw unit (2⁻³²) to one constant
(`PUSH_RELAXATION`, `STEERING_LOOKAHEAD`), as plan `2026-11-19-crowd-slide` §2.2 did. For the new pipeline, the
placebos add one raw unit to the solver's overlap tolerance and to the side rule's time horizon. **An effect counts
only when it lies outside the range spanned by the placebo builds and outside the 95 % bootstrap interval of the
aggregate** (scenarios resampled with replacement, 2,000 resamples).

### 6.5 Thresholds (pre-registered, binding on the switch-over)

Absolute thresholds hold for the new pipeline on the acceptance range at every behaviour work package and on the
held-out range at the switch-over. Relative thresholds compare it with today's pipeline on the same range.

| ID | Property | Threshold | Why this value |
|---|---|---|---|
| T1 | No oscillation | M1 ≤ 1.0 per 1,000 moving unit-ticks in every family, M1-hi ≤ 0.25; and M1 not above today's in any family (beyond the noise rule) | One visible reversal per 40 unit-seconds of motion is rarer than a viewer can attribute to a unit; half-speed reversals are the ones a viewer sees at once |
| T2 | No wasted motion | M2 not above today's in any family; M3 ≤ 0.5 world units per unit over 10 s | 0.5 over 250 ticks is a jitter of 0.002 per tick, below a pixel at the default camera |
| T3 | Progress | M4 ≤ 1 % of active windows in every family | A one-radius retreat sustained over a second is a visible step back; one window in a hundred allows a push by a higher-priority body |
| T4 | No deadlock | M5: 0 deadlocks and 0 give-ups in the passable gap and cross families; every unit with room completes by the bound in ≥ 99.5 % of scenarios; nobody passes an impassable gap | The bounds are twice a free-flow estimate; with room, a unit that has not arrived by then is stuck |
| T5 | Concave | M6 mean ≥ 0.90 and its 5th percentile ≥ 0.7476 | 0.7476 is Rényi's parking constant, the saturation density of random sequential placement on a line: a ring filled by arrival order alone reaches it, so a slot choice must beat it |
| T6 | Throughput | M7: η median ≥ 0.5 and 5th percentile ≥ 0.25; for each radius, median throughput non-decreasing in `w` (each step within its bootstrap interval) | Half the capacity model is a queue that keeps moving; a collapse at high load would put the 5th percentile near 0 |
| T7 | Overlap (D-α) | Enemy overlap ≤ 0.5 % of the radius sum (rounding); terrain penetration ≤ 2⁻¹⁰ world units; friends: share of moving near pair-ticks above 5 % ≤ 0.1 %, share of standing near pair-ticks above 1 % ≤ 0.1 %, maximum ≤ 15 % | D-α's figures; the maximum caps a transient of three bodies pressed by a heavier one |
| T8 | Relabelling | M9: sign tests p ≥ 0.01 in every family (Bonferroni over the families still gives a family-wise rate below 5 %); aggregate differences within the noise rule | An ID bias shows as a one-sided shift between paired runs |
| T9 | Mirror | M10: the 95 % Wilson interval of blue's share contains 0.5 | The survey's symmetry criterion |

A work package passes when every threshold its scope touches holds; the switch-over passes when all of T1 to T9
hold on the held-out range. Battle winners of named seeds are not a criterion.

### 6.6 Corrections found while measuring (none changes a threshold)

- **C-0, what ran before the pre-registration commit.** Only a smoke test of the scratch harness on today's
  pipeline, on seeds 500,001 to 500,016 outside every range of §6.1, to check that it ran; it is disclosed here
  because it showed the gap family's give-ups and the mirror's draws before the thresholds were fixed.

- **C-a, the mixed gap's known answer.** In a quarter of the gap scenarios half the units are a second archetype of
  0.75 `r` (§6.2). When `w ≤ 2r` but `w > 1.5r`, those smaller units can pass, and do in both pipelines; the generator's
  answer "nobody passes" was wrong for them. The analysis counts a pass in an impassable scenario as wrong only beyond
  the smaller archetype's units. With the correction, no pipeline lets a unit through a gap narrower than itself.
- **C-b, the second placebo.** Placebo P2 was built on top of P1's file by mistake, so it adds one raw unit to both
  `PUSH_RELAXATION` and `STEERING_LOOKAHEAD`. It is still a placebo (two raw units); it is reported as such.
- **C-c, the new pipeline's placebos are silent.** One raw unit added to the prototype's dead band or to its side
  horizon gives bit-identical CSVs on the whole development corpus: the prototype is not chaotic under the smallest
  perturbation of its own constants. Its noise is therefore read from the bootstrap intervals alone, and from the
  relabelled corpus, which does move it.
- **C-d, give-ups count as deadlocks.** M5's deadlock is "not complete by the bound and not moving for 50 ticks"; a
  unit that gave its `Move` up (`NoProgress`) satisfies it. In the passable gap family nearly all of them gave up at the
  shared destination, after passing the gap, in mixed-priority scenarios (§5.2). They are reported as measured.
- **C-e, the corridor's fill.** Its capacity counts one rank; an attacker of the second rank whose range exceeds a
  diameter also reaches, so the corridor's fill can exceed 1. T5 is judged on the arc family only, as M6 defines it.

---

## 7. Work packages

Every package: failing tests first (red on the package's base, green after), `cargo gate quick` and the tests the
change needs, then hand back; a verifier runs `cargo gate package` on the final tree (Q0061, charter §4). Builds in
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`; measurements one at a time under
`flock ~/scratch/gate.lock`. Behaviour packages are accepted on the harness's properties (§6.5) over the
**acceptance** range, never on named battle winners. Every package that moves behaviour re-records `golden_replay`
with a proof header on four builds (native debug and release; `wasm32-wasip1` release and debug, runner
`wasmtime --dir=<real path>`), and re-pins `DEFAULT_SANDBOX_CHECKSUM` and the other pinned digests only if they move.

| WP | Content | Behaviour | Schema | Gate | Size |
|---|---|---|---|---|---|
| WP1 | Evaluation harness | none | 32 | package | M |
| WP2 | Data, latent state, the solver and intent as tested library code | none (Legacy) | 33 | package | L |
| WP3 | Solver pipeline: contact and intent | under `pipeline: Solver` only | 33 | package | L |
| WP4 | Slot goal | under `Solver` only | 33 | package | M |
| WP5 | Pursuit reads the target's tick-start position (if §5.4 confirms) | both pipelines | 33 | package | S |
| WP6 | Switch-over: `Solver` by default; held-out acceptance; goldens; matrix; throughput; storyboard | yes | 33 | full | M |
| WP7 | Deletion of the legacy pipeline, its constants, memory fields and tests | none (Solver) | 34 | full | M |

### WP1: evaluation harness (no behaviour change)

- **Code.** `crates/sim_fixtures/src/harness/` (new): `corpus.rs` (the generator of §6.2, SplitMix64, 1/64 lattice,
  scratch archetypes in RON), `metrics.rs` (M1 to M8 as an observer like `BlockingMeter`), `mirror.rs`,
  `report.rs` (CSV rows and the aggregation, bootstrap and sign test of §6.4 in integer-seeded `f64` code, outside the
  simulation). The binary is `crates/sim_core/examples/movement_harness.rs` (as `matchup_trace` is): `--family`,
  `--from`, `--count`, `--relabel`, `--jobs`, `--out`; an `--aggregate <dirs>` mode prints §5's tables and the
  threshold verdicts. The known answer of a mixed gap scenario counts the smaller archetype's passes (§6.6 C-a).
- **Families added beyond §6.2:** none required; a `corner` family (a group passing one rounded obstacle, B-152's
  case, see §8.2) is added as a fifth family under the same thresholds.
- **Failing tests first** (`crates/sim_fixtures/tests/harness_tests.rs`): the generator is a pure function of
  `(family, seed)` and every coordinate lies on the 1/64 lattice; the relabelled scene differs only in spawn order;
  M1 counts a synthetic track's reversal only above `v/4` and below cosine −0.5; M2 and M3 on synthetic tracks;
  the gap potential decreases along the straight path through the gap; the sign test and Wilson interval against
  known values; a 5-seed run per family plays without a simulation diagnostic and twice gives identical CSV.
- **Acceptance.** The baseline of §5 reproduced to the last digit from the committed code on the development
  range (`base-1` figures), and recorded in `docs/evidence/2026-11-20-contact-solver.md`, with the placebo builds
  (§6.4) as patch files under `~/scratch`, never committed. Runtime of the development corpus under 30 minutes with
  `--jobs 4`.
- **Also measured in WP1 (scratch only):** §5.4's hypothesis (a build in which `prepare_pursuit` reads the target's
  tick-start position) on the mirror family's development range; recorded in the evidence file.

### WP2: data, latent state and the parts as library code (no behaviour change)

- `assets/catalog/movement_rules.ron`: the `contact` section of §3.6 with `pipeline: Legacy`;
  `catalog/movement_rules.rs` parses and validates it (typed `thiserror` errors).
- `MovementMemory`: `side_partner`, `side_left`, `slot` (§3.7); `restart()` clears `side_*` and `slot`; schema 33.
- `movement/solver.rs` (new): the pair build, C-2's `contact_shares` (public, replacing the old push API's role), the
  K-iteration loop with C-6's terrain step, retained buffers in `MovementWorkspace`; not called by the tick yet.
- `movement/intent.rs` (new): I-1 to I-4 as a function of the snapshot; not called yet.
- `movement/concave.rs` (new): G-2 to G-5 as functions; not called yet.
- **Failing tests first** (unit tests in the three modules and `crates/sim_core/tests/contact_solver_tests.rs`):
  - solver: order independence (the same bodies in shuffled pair and body order give bit-identical positions,
    a property test over generated crowds); a fixed, holding or engaged body never moves under a friend; enemies
    end within the dead band; a body is never moved deeper into a wall nor across a cliff; after K iterations
    no friendly pair of a generated hexagonal crowd with injected overlap `ρ` exceeds `δ_move` (the C-3 property);
    a crowd at rest within the dead band does not move at all;
  - `contact_shares` reproduces every row of C-2;
  - the iteration bound: `iterations` ≥ the bound computed from the shipped catalog (C-3);
  - intent: head-on pair passes right-to-right (I-3b) and is mirror-consistent; a crossing at 90° commits a side and
    keeps it while closing; a co-moving friend and a yielding idle friend are not threats; the deflection never
    exceeds `asin(1/h)` at the horizon;
  - concave: free-interval computation against hand-built rings; ties go clockwise; claims resolved in sort order
    independent of input order; a claim is freed in band, re-chosen when blocked or when the target moved by `r`.
- **Acceptance.** Every existing test green; render snapshots of the golden replay byte-identical to schema 32;
  `golden_replay` re-recorded for the schema (proof header: "checksum schema 32 → 33, three hashed `MovementMemory`
  fields added and unused under Legacy; render snapshots byte-identical").

### WP3: the Solver pipeline, contact and intent

- Under `pipeline: Solver`: `step_toward` uses `intent` instead of `steer_body`, skips `remove_blocked_velocity`,
  `surround_walk` and `is_waiting` and the `crowded` wait; `resolve_collisions_and_push` runs `solver` instead of the
  pair pass. Legacy is untouched (a test runs the golden replay under Legacy and requires WP2's checksums).
- `gather_neighbours`' reach under Solver: `r + largest + max(H_b) · speed`, capped at today's reach plus the horizon
  (I-3a), **verify** the neighbour count stays within the retained buffer's high-water mark on the benchmark.
- **Failing tests first:** the harness's gap and cross families on 20 development seeds each, run in a test under
  `pipeline: Solver` through a scratch catalog directory, assert T4 (no deadlock, no give-up where there is room) and
  T7 (bounds) — red today (§5.2: 337 units give up with room at gaps, 73 in crossings); the tests of §8.1 marked
  *restated* in their new form.
- **Acceptance (acceptance range, Solver against Legacy):** T1, T2, T3, T4, T6, T7, T8 on gap and cross; T7 on all.
  Arc and corridor are not judged until WP4.

### WP4: the slot goal

- Under Solver: the slot pass (§3.1 step 2), `chase` takes the claim's point as its goal (G-7), G-6's stand.
- **Failing tests first:** `a_walk_out_of_a_pocket_still_reaches_range` and `ranged_block_reaches_the_arc` restated
  under Solver; arc and corridor families on 20 development seeds each assert T5 and G-6 (units without a slot stand:
  M3 ≤ 0.5).
- **Acceptance (acceptance range):** T1 to T8 on every family.

### WP5: the pursuit's target position (conditional)

- Only if §5.4's hypothesis is confirmed in WP1. `prepare_pursuit` reads the target's position and motion from the
  tick-start snapshot, not the live entity. Both pipelines (it is a determinism-of-order fix, not a contact rule).
- **Failing test first:** a mirror pair test: two identical units placed point-symmetrically, attacking each other,
  must reach range on the same tick and fire on the same tick (red today if the hypothesis holds).
- **Acceptance:** T9 on the mirror family's acceptance range, under both pipelines.

### WP6: switch-over

- `pipeline: Solver` becomes the default in `movement_rules.ron`.
- **Held-out run, once:** both pipelines on the held-out range, every family and the mirror; T1 to T9 must hold for
  Solver. The figures go to the evidence file and §5 of this plan as an amendment.
- **Goldens:** `golden_replay`, `DEFAULT_SANDBOX_CHECKSUM` and every pinned digest that moves, re-recorded on the four
  builds with proof headers naming this plan, WP6 and the first changed checkpoint.
- **Matchup matrix** (`sc2_who_wins_matchup_matrix_tests`, `sc2_battle_benchmarks_tests`): battle winners are not an
  acceptance criterion. Ranges that the switch moves are restated from the new pipeline's results over the
  matrix's 20 seeds, each restatement listed with its old and new range in an amendment; a battle whose claim
  (who wins) flips is reported to the orchestrator as a finding about the battle, not fixed by tuning.
- **Throughput:** the release benchmark, each scenario alone (`BENCH_ONLY`), `perf stat -e instructions:u`, under
  `flock ~/scratch/gate.lock`; every scenario at least real time (charter §3.1); an increase above 10 % on any
  scenario is a stop for review. `DESIGN.md` §5 updated if the ticks-per-second figures move.
- **Showcase storyboard** re-checked with a real render on the RX 580 under radv, `with-gpu-lock` per take
  (charter §7), `WGPU_BACKEND=vulkan` and the radv `VK_ICD_FILENAMES`; `assets/captures/showcase.ron` re-timed if
  the default seed's timeline moved; `showcase_storyboard_tests` and `showcase_tests` pass.
- `DESIGN.md`: the Movement row, §4 rows of §8.2, §5 if moved.
- **Gate:** `full` on the merged tree.

### WP7: deletion

- Delete the Legacy pipeline: L3, L4, L5, L7 to L11, L15 to L18, L20, L23's old API; the 12 constants of §2.3; the
  walk's memory fields and `flank_left`/`flank_target` (schema 34); `contact.pipeline`; the archetype field
  `surround_walk` (and its `catalog_serialization_compatibility` entry: an old catalog with the field still loads,
  the field is ignored with a load-time note, **verify** the compatibility test's policy).
- Delete or restate the tests of §8.1 marked *superseded*.
- Charter: every remaining constant carries its derivation; `docs/guides/charter-checks.md` unchanged (no new
  check).
- **Acceptance:** render snapshots of the golden replay byte-identical to WP6's; `golden_replay` re-recorded for
  schema 34 on the four builds; harness development range bit-identical to WP6's.
- **Gate:** `full`.

---

## 8. Tests and register rows

### 8.1 Existing tests

*Kept*: must pass unchanged under Solver from WP3/WP4 on. *Restated*: the property stays, the assertion is rewritten
for the new mechanism in the WP named. *Superseded*: deleted in WP7, with the property that replaces it named.

| File | Test | Verdict |
|---|---|---|
| `crowd_edge_tests.rs` | `a_queued_chaser_keeps_its_target`, `a_blocked_move_is_still_given_up` | Kept (G-6, §3.5) |
| | `a_walk_out_of_a_pocket_still_reaches_range` | Restated (WP4): the unit reaches range through a slot |
| | `a_spent_walk_resumes_when_the_way_opens`, `a_spent_walk_re_arms_when_its_closed_side_opens`, `contact_creep_does_not_re_arm_a_walk`, `a_walk_does_not_end_on_the_room_its_retreat_made`, `a_walk_without_retreat_ends_as_before` | Superseded (no walk); replaced by G-5's claim tests and T1/T5 |
| `push_priority_tests.rs` | `a_holding_body_is_never_pushed`, `a_firing_body_is_never_pushed_by_a_friend`, `test_hold_position_friendly_hard_blocks`, `an_idle_standing_body_steps_aside_for_equal_or_higher_priority`, `enemies_never_push`, `test_enemy_unit_hard_collision_reciprocal`, `a_unit_turning_in_place_is_pushed_like_a_standing_unit`, `a_push_is_not_gated_by_facing` | Kept (C-2 rows) |
| | `moving_friends_separate_evenly` | Kept under D-β on; a D-β-off twin added (WP3) |
| | `test_idle_friendly_displaced_laterally` | Restated (WP3): displaced along the contact normal, not the lane exit |
| | `a_waiting_body_is_not_pushed_aside` | Restated (WP3): a waiting body takes at most half the overlap (C-2), and never leaves its lane by more than that |
| | `test_pure_displacement_math` | Restated against `contact_shares` (WP3) |
| `collision_and_immobility_tests.rs` | `test_hold_position_friendly_acts_as_immovable_wall`, `test_immobile_unit_unpushable_during_siege_and_spell`, `test_higher_priority_melee_slides_around_engaged_ranged_ally`, ramp traversals, `battlefield_bubble_trajectories_are_smooth_without_micro_jitter`, `test_moving_friendly_pushes_idle_friendly_laterally` | Kept (the last restated like `test_idle_friendly_displaced_laterally` if it asserts the lane exit, **verify**) |
| | `test_converging_units_respect_speed_cap_and_do_not_vibrate` | Restated (WP3): no speed cap (C-5); no vibration by M1's amplitude rule |
| | `test_sc2_moving_units_never_push_each_other_laterally` | Kept (C-2 pushes along the normal only) |
| `movement_contact_tests.rs` | all but the two below | Kept |
| | `exact_head_on_tie_separates_to_opposite_world_sides` | Restated (WP3): I-3b, both pass on their right |
| | `units_blocked_by_a_holding_line_never_reverse` | Restated (WP3): reversals by M1's amplitude rule |
| `second_line_tests.rs` | `ranged_block_reaches_the_arc` | Restated (WP4) under slots, same bound |
| | `walkers_at_the_crowds_edge_do_not_toggle` | Restated (WP4) with M1 |
| | `showcase_parked_corner_aim_does_not_flip` | Kept (L24) |
| `cluster_arrival_tests.rs`, `wall_contact_tests.rs`, `route_following_tests.rs`, `stutter_step_tests.rs`, `hull_turning_tests.rs`, `structure_contact_tests.rs`, `contour_footprint_tests.rs`, `movement_layers_tests.rs`, `status_modifier_tests.rs` (collision-suppressed body), `attack_move_simulation_tests.rs` (`test_move_command_units_fan_out_and_surround_target_point`) | all | Kept |
| `fixed_body_transactions_tests.rs` | all seven | Restated against `contact_shares` in WP3; the old-API versions deleted in WP7 |
| `checked_movement_tests.rs`, `world_api_surface_tests.rs` | `compute_concave_arc_steering`, `compute_unified_local_steering` uses | Superseded with the dead API (WP7) |
| `steering.rs` in-source | `wall_tests` (3), `weighted_tests` (2) | Superseded (WP7) / restated against `contact_shares` |
| `locomotion.rs` in-source | the walk's tests (`a_walk_ends_only_on_room_beyond_its_retreat` … `a_closed_side_opens_only_for_a_whole_step`) and `a_sliver_of_slide_waits_instead_of_creeping` | Superseded (WP7) |
| | the wall-slide, parked-corner and hull-budget tests | Kept |
| `collision.rs` in-source | `soft_correction_never_exceeds_the_units_own_speed` | Superseded by the solver's C-3 and C-5 tests |
| | `a_registered_structure_is_no_collision_body`, `failed_pair_keeps_both_accumulators_and_prior_contributions` | Kept / restated for the solver's accumulators |
| `sc2_who_wins_matchup_matrix_tests.rs`, `sc2_battle_benchmarks_tests.rs` | survivor ranges | Restated at WP6 (ranges, not claims; §7 WP6) |
| `golden_replay_tests.rs`, `fixture_tests.rs` | pinned checksums | Re-recorded at WP2, WP6, WP7 |

### 8.2 Register rows (`DESIGN.md` §4)

| Row | Verdict |
|---|---|
| **B-103** (reversals at a crowd's edge; corridor standing) | Superseded by the design. Closed at WP6 if T1, T2 and T5 hold on the held-out range; its R-A to R-F figures are retired with the meter they were counted by (the legacy `REVERSAL_COSINE` −0.3 meter stays in `sim_fixtures` for comparison) |
| **B-144** (a crossing friend's side taken per tick) | Addressed by I-3 (a side with a derivation and a source, SC-2, kept per pair). Closed at WP6 if the cross family meets T1 |
| **B-145** (aim flip; Viking residual open as a walk-end event) | The fixed part stays (L24). The residual, a surround walk ending when its target dies, cannot occur without the walk: closed at WP7 |
| **B-152** (a unit beside rounded obstacles at a narrow gap reverses its step, up to 12 consecutive ticks; 345 reversals for one unit in a prototype; on `main` since `669b100c`) | Measured by WP1's `corner` family (a group passing a narrow gap between rounded obstacles: two resource-node-sized discs or 2×1 footprints) under both pipelines; closed at WP6 if the family meets T1, else restated with its figure |
| **U-23** (unsourced tuning values) | 12 of the 29 constants are deleted; the solver's and intent's values carry derivations (C-3, C-4, I-3a); the remaining unsourced ones are restated in the row |
| **New, WP1** (number assigned at merge; B-156 is the highest on `445ed008`) | Mirror asymmetry: blue (lower IDs) wins 42.3 % [38.7, 46.0] of decisive mirror matches on the development range (§5.4); number assigned at merge |
| **New, WP1** | Give-ups with room: in 147 of 230 mixed-priority gap scenarios a lower-priority unit gives its `Move` up at the shared destination after passing the gap, and 73 units give up in open crossings (§5.2); both pipelines (K-10) |

---

## 9. File ownership

| Path | WP | Change |
|---|---|---|
| `crates/sim_fixtures/src/harness/{mod,corpus,metrics,mirror,report}.rs` (new), `crates/sim_fixtures/src/lib.rs` | WP1 | The harness |
| `crates/sim_core/examples/movement_harness.rs` (new) | WP1 | Thin `main` |
| `crates/sim_fixtures/tests/harness_tests.rs` (new) | WP1 | Harness tests |
| `docs/evidence/2026-11-20-contact-solver.md` (new) | WP1, WP3, WP4, WP6 | Figures |
| `assets/catalog/movement_rules.ron`, `crates/sim_core/src/catalog/movement_rules.rs` | WP2, WP6, WP7 | `contact` section |
| `crates/sim_core/src/entity.rs`, `crates/sim_core/src/checksum.rs`, `crates/sim_core/src/checksum/encoding.rs` | WP2, WP7 | Memory fields, schema 32 and 33 |
| `crates/sim_core/src/movement/{solver,intent,concave}.rs` (new), `movement/mod.rs` | WP2 to WP4 | The three parts |
| `crates/sim_core/src/movement/locomotion.rs` | WP3 to WP5, WP7 | Pipeline switch in `step_toward` and `chase`; slot pass; deletion |
| `crates/sim_core/src/movement/collision.rs` | WP3, WP7 | Solver call; deletion of the pair pass |
| `crates/sim_core/src/steering.rs` | WP7 | Deletion of L1, L3 to L5, L7, L8, L15, L16, L23 |
| `crates/sim_core/src/movement/tuning.rs` | WP7 | 12 constants deleted |
| `crates/sim_core/src/catalog.rs` | WP7 | `surround_walk` field |
| `crates/sim_core/tests/contact_solver_tests.rs` (new) and the files of §8.1 | WP2 to WP7 | Tests |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `crates/sim_fixtures/tests/fixture_tests.rs`, `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP2, WP6, WP7 | Re-recorded pins and restated ranges |
| `assets/captures/showcase.ron` | WP6 | Only if the timeline moved |
| `DESIGN.md`, `docs/plans/README.md`, `docs/questions/closed/0052-sanctioned-sc2-deviations.md` | WP1, WP6, WP7 | Bookkeeping |

No other package may edit `movement/`, `steering.rs` or `movement_rules.ron` while WP2 to WP7 are open; a package that
must (a bug fix) lands on `main` first and this plan's next package rebases on it.

---

## 10. Risks

| # | Risk | Mitigation |
|---|---|---|
| K-1 | Every battle and golden replay changes; a matrix battle's claim may flip | Accepted by the owner (Q0092 (a)). WP6 restates ranges, reports flipped claims as findings, never tunes for them |
| K-2 | The solver's transient overlap is visible (D-α) | Bounds are data and checked by T7 on the held-out range; the storyboard re-check looks at the showcase |
| K-3 | Large initial overlaps (spawn, unload, burrow) resolve in one visible jump (C-5) | WP3 test bounds the ticks to leave and forbids wall crossing; a per-tick cap is added only if the jump is visible, as a recorded amendment |
| K-4 | The TTC rule's one-pair memory changes side when a new earliest threat appears | M1 counts it; if cross-family T1 fails on that cause, rank 2 of the survey (§4.2, an HRVO layer for moving non-co-moving neighbours) is the next plan, not more rules here |
| K-5 | Slot contention: several claimants for one gap on one tick | G-4's sort; a refused claimant retries next tick; T5 measures fill |
| K-6 | Slot goals over a non-circular target (structure footprints) | The band is today's centre-distance band (`pursuit_band`); a footprint target uses its collision radius like the band does; a structure-target family is out of scope (§11) |
| K-7 | The harness's known answers are approximate (ring capacity is a lower bound; the corridor's capacity counts one rank) | T5 compares with Rényi's constant, not with perfect packing; known-answer corrections are recorded in §6.6 like C-a, never by changing thresholds |
| K-8 | Throughput: K = 14 iterations in the worst case | Early stop; WP6's instructions-retired check with a 10 % stop; §5.5 gives the prototype's figure |
| K-9 | Pursuit order dependence (§5.4) is not a contact problem and may not be the whole mirror bias | WP5 is conditional on WP1's diagnostic; T9 is judged under both pipelines so the bias is not blamed on the solver |
| K-10 | Mixed-priority give-ups at a shared destination (§5.2) persist in both pipelines: a lower-priority mover cannot push an idle, arrived higher-priority friend (C-2) and the cluster-arrival witness must lie within 1.5 of the goal (L14) | Not a contact rule: WP3 records the figure; an unbounded arrival chain was measured and rejected (§5.3: it ends orders before the gap). A later note on arrival (L14) owns it |
| K-11 | A concurrent movement package conflicts | §9's ownership rule |
| K-12 | Residual overlap where constraints cannot all hold: the prototype leaves enemy overlaps up to 2.1 % (corridor) and 1.0 % (arc) against T7's 0.5 %, and friend pairs squeezed between a fixed pillar and a wall reach full overlap in both pipelines (largest friend overlap 0.96 today, 1.0 prototype) | WP3 classifies the T7 violations by cause with the harness (infeasible wedge, queue pressure, spawn); the hard pass (C-1b) may iterate up to K times over hard pairs only; infeasible scenes (a disc that fits nowhere) are reported by the harness as such, never hidden by a threshold change |
| K-13 | The prototype's slot sketch gives up 260 to 290 chases on the arc family and its 5th-percentile fill (0.68) is below Rényi's constant | Two causes are expected from what the sketch left out: no claim resolution (G-4), so two claimants keep taking each other's gap, and no slot routing (G-7), so a unit walks straight at its slot through the fighting line; WP4 builds both and is accepted only on T5 and T4 |

---

## 11. Out of scope and decisions recorded (Q0002)

- **The global layer** (navigation mesh, TA*, funnel, lanes) is out of scope; B-149 and B-154 stay with their plans.
- **Rank 2 of the survey** (fixed-point HRVO/ORCA for moving neighbours) is out of scope; it is the named next step if
  K-4 materialises.
- **Parked-body aim and held rounding (L24)** are kept; whether the solver makes them unnecessary is a later
  measurement on the `corner` family, not part of this plan.
- **Air units** use the same solver and intent (they are bodies of their own class); no air-specific family is in the
  corpus. Recorded so it can be brought back if an air-only behaviour is reported.
- **Structure targets for slots** use the collision-radius disc (K-6); a footprint-shaped arc is out of scope.
- **Congestion costs in TA*** (FF-7) are out of scope.
- **The prototype** (§5.3) used the walk's memory fields as scratch storage for the slot and the side, had no claim
  resolution (G-4) and no slot routing (it walked straight to the slot point), and allocated per tick; none of it is
  committed. Its patch is kept at `~/scratch/contact-solver/proto.diff` for WP3's implementer to read, not to apply.
- **Arrival (L14) and the mixed-priority give-ups (K-10)** are out of scope beyond recording them: the one change
  measured (an unbounded arrival chain) was rejected (§5.3).
- **Decisions made here without the owner (Q0002):** the right-hand tie rule (I-3b) instead of an ID tie-break; one
  committed pair per unit (I-3c); dropping the engaged-friend priority tie-breaks (C-2 note); the slot trigger is the
  existing `friend_holds_ahead` (G-1); the mirror family removes random weapon delays (§6.2); the pursuit fix is a
  separate, conditional package (WP5).

## Amendment 1 (orchestrator, 2026-11-20): WP1's baseline of record; the pair list and the iteration count of WP2; WP5 under Solver only

1. **The WP1 baseline supersedes §5.** The planner's scratch harness source is gone, so §5's figures cannot be
   reproduced to the last digit. The committed harness was built from §6's text, with its open choices fixed in
   evidence §1.3 before any solver measurement. Its baseline on the development range is the baseline of record.
   No threshold changes.
2. **WP2's pair list and K.** The C-3 property test fails as written, for two traced reasons (evidence §2):
   - pairs built once miss contacts that the solve itself creates;
   - convergence is slower than the 1 − 1/c derivation.

   **Decision:**
   - **(a) Pair list.** The solver keeps a Verlet pair list: it rebuilds the list when any body's accumulated
     correction since the last build exceeds half the list's slack. This is the standard rule, so no new tuning
     constant: the slack and the rebuild rule are stated with their derivation.
   - **(b) K.** K becomes the smallest iteration count that passes C-3 on its generated crowds. It is recorded with
     that test as its derivation, then confirmed on a fresh, disjoint set of generated crowds that was never used
     to choose it. If the confirmation fails, stop.
   - **(c) Bounds.** D-α's 5 % moving bound is not restated.
   - **(d) Cost.** Report the Solver pipeline's instructions retired with the chosen K as soon as WP3 can measure
     it. Flag at once if the 10 % stop of WP6 looks unreachable.
3. **WP5 under Solver only until WP6.** The tick-start read of a pursued target's position applies only to the
   Solver pipeline until WP6, so Legacy stays bit-identical. The Legacy effect is measured with the scratch patch:
   the mirror goes from 0.438 to 0.488, as evidence §1 shows.
4. **Register rows.** The plan's two new rows, the mirror asymmetry and give-ups with room, take the next free
   numbers on main, with WP1's development-range figures.
5. **Gate tier.** `harness_tests` (about 65 s) is reclassed from fast to slow in `crates/gate/suites.ron`.
