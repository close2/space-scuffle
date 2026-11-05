# Blueprint: Turn Before Move for the Siege Tank, Thor and Ultralisk Analogues (Q0050 option b)

This plan carries out the owner's answer to Q0050. It adds a per-archetype catalog flag, `turn_before_move: bool`.
A unit with the flag does not translate on a tick when its hull cannot face that tick's step within the hull's
remaining turn budget: it turns in place instead. On the tick the remaining turn fits, it faces the step exactly
and takes the whole step. The flag is set on the three identities whose StarCraft II data set `TurnBeforeMove`:
`railgun_crawler` (Siege Tank), `colossus_mech` (Thor) and `stag_behemoth` (Ultralisk). It builds on the hull
turning rule of plan `2026-11-04-combat-sc2-alignment` WP17 (D-8 item 4: `Entity::turn_hull_towards`,
`hull_turn_budget`) and fills that plan's commit g, which WP17 skipped because no tolerance was known.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-05 against `main` at `a27ce2a`
(life-regeneration delay merged; combat alignment WP17 merged; WP13 and WP14 open). Line numbers drift; search for
the quoted symbol. Claims that could not be confirmed while planning are marked **verify**. Expectations that have
not been measured are marked as such; no planning probe was run, because the main checkout was in use by a gate.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| [Q0050](../questions/open/0050-turn-before-move.md) turn before move | Answered 2026-11-05: option (b), tolerance "within one tick's turn"; closed by WP4 | D-1 to D-4, WP4 |
| [Q0045](../questions/open/0045-catalog-time-audit-deliberate-values.md) item 1(b), hull turning | Open; closed by combat alignment WP13. Its turn-before-move part (WP17 commit g) is done here | §1 hand-off, WP4 |
| [Q0008](../questions/closed/) StarCraft II where the specification is silent (charter §8) | Standing rule | D-2, D-3, D-5 |

**Amendment 1 (orchestrator, 2026-11-05, against this plan's commit on `main` at `a27ce2a`).** The owner gave a
standing rule on 2026-11-05, relayed by the orchestrator and overriding this plan where they differ: "always use the
SC2 mechanics unless I specifically tell to deviate and mention the deviation. When you see a deviation which has not
been explicitly mentioned as deviation, consider it a mistake or bug." Q0050's answer names `turn_rate_deg` for the
turn in place but does not call it a deviation, so StarCraft II's rate wins. What changes:

1. **The stationary rate is applied (D-6 and R-4 reversed; F-1 brought into scope).** New D-10 below and two new
   packages, WP5 (code) and WP6 (data), run after WP3 and before WP4: WP1, WP2, WP3, WP5, WP6, WP4. One cause per
   commit, each with its own measurement.
2. **D-1's comparison reads the moving rate.** A tick that translates turns at the moving rate (`turn_rate`), so a
   step is taken only when the remaining turn fits in one moving tick: `remaining <= turn_rate`. A tick that does
   not translate turns at the stationary rate. The gate therefore carries `turn_rate`, not the tick-start budget
   (`HullGate { heading, rate }`). Before WP6 both rates are equal and WP1's behaviour is exactly D-1's. After WP6
   a reversal of the three flagged units takes 4 ticks in place (4 × 7 340 = 29 360; the remaining 3 408 fits in
   3 670) and a step on the 5th tick, instead of 8 and the 9th.
3. **D-3's validation bound becomes** `ceil(max(0, 32 768 − turn_rate) ÷ stationary_turn_rate) + 1 ≤
   STALL_REPATH_TICKS` (the ticks to the first step of a reversal), in binary units. With equal rates this is D-3's
   `ceil(32 768 ÷ turn_rate) ≤ 25`, so WP1's tests hold unchanged; WP5 generalises the rule.
4. **A translation the plan's §2.2 missed.** `MotionDraft::upkeep_route` puts a unit that has no route and stands
   within one step of its goal on the goal and finishes the order, outside `step_toward`. WP1 gates it the same way
   for flagged units (turn in place, no finish), so invariant 1 holds on every path. This corrects the plan; it is
   not a choice between behaviours.
5. **Differences from StarCraft II that no source settles** are listed in the evidence file as open deviations,
   each with what this code does: the tolerance itself (taken from the owner's answer), the facing of a short step
   that lands on a waypoint, whether a unit turning in place counts as still for pushing and anchoring, its
   animation, and whether pushes are gated. None is changed without a source.

**D-10. The stationary turn rate (Amendment 1).** `UnitArchetypeDef` gains `stationary_turn_rate_deg_per_tick:
Option<f32>` (`#[serde(default)]`; omitted means equal to `turn_rate_deg_per_tick`, as `CUnit` has the two fields
default to related values and 38 of the 42 identities have them equal, audit §10.1), validated like the hull rate
(`0 < rate ≤ 180`); `UnitArchetype` gains `stationary_turn_rate: SimAngle` (binary units per tick, through the
loader's `binary_angle`); structures get `SimAngle::ZERO`. **Rule:** `reset_hull_turn_budgets` sets every entity's
budget to its stationary rate; `MotionProposal::commit` sets the budget to the moving rate `turn_rate` before it
turns the hull when the unit's own motion translates it this tick (`proposal.position != actor.position`). So the
hull turns at the stationary rate on every tick the unit does not translate by its own motion (turning in place
before a move, aiming a hull weapon while standing, Hold Position, idle), and at the moving rate on a tick it does;
a collision push is not the unit's motion (D-4). Nothing spends the budget before the actor's single motion commit,
so setting rather than capping it is exact. No new entity state; `hull_turn_budget` stays unhashed; schema stays
10. **Values** (audit §10.1, §10.2, from the `StationaryTurningRate` field, Normal °/s × 1.4 ÷ 25):
`railgun_crawler`, `colossus_mech`, `stag_behemoth`: 40.32 (720, `core` UnitData:39, the `CUnit` default none of
the three overrides); `napalm_wheeler` (Hellion): 84.00 (1499.9414, `liberty` UnitData:11554). Every other identity
omits the field.

**WP5 (code, Amendment 1).** Tests first: catalog validation (stationary rate out of range rejected naming the
field; omitted reads the hull rate; the generalised bound accepts a flagged unit whose stationary rate brings a
slow hull within 25 ticks and rejects one that does not), and in `hull_turning_tests.rs` scratch archetypes whose
stationary rate differs: a standing unit aiming its hull weapon turns at the stationary rate; a moving unit turns at
the moving rate; a flagged unit on a reversal turns in place at the stationary rate and steps once the remainder
fits in the moving rate. Implement D-10; every shipped value omitted, so the matrix, battle 33, both goldens and the
sandbox are bit-identical (stop if not). Commit `feat(sim_core): hulls turn at a per-archetype stationary rate while
the unit does not translate, equal to the moving rate everywhere (turn before move WP5, Amendment 1)`.

**WP6 (data, Amendment 1).** Failing test first: the WP3 shipped-units test restated to the generalised count of
ticks in place (`ceil(max(0, 32 768 − turn_rate) ÷ stationary_turn_rate)`), and a shipped-data test that every
archetype whose stationary rate differs from its moving rate turns faster standing than moving (by property). Edit
the four values above. Measure as WP3 against WP3's step: only battles containing the four identities may move
(3, 9a, 9b, 10, 14, 15, 27); battle 33 bit-identical; `golden_replay.ron` from checkpoint 3 at the earliest (the
tanks first turn their hulls after tick 120), the orders golden and the sandbox unchanged; same stop rules as WP3.
Commit `data(catalog): the Siege Tank, Thor, Ultralisk and Hellion analogues turn at their StarCraft II stationary
rates (turn before move WP6, Amendment 1)`, citing the sources above. WP4's documents and Q0050's resolution then
describe both rates; F-8 of the alignment plan is closed by a pointer to this plan instead of being kept.

---

## 1. Summary

| WP | Title | Changes expected results? | Size | Depends on |
|---|---|---|---|---|
| WP1 | Code: `turn_before_move` in the catalog, its validation, and the step gate in locomotion; `false` everywhere | no (proved bit-identical) | M (1 day) | none on the branch |
| WP2 | Tests: restate tests that use a shipped flagged identity and would break for a reason that is not their subject (test-only; only if the probe finds any) | no | XS-S | WP1 |
| WP3 | Data: `turn_before_move: true` on `railgun_crawler`, `colossus_mech`, `stag_behemoth` | yes: matrix battles 14, 15 and 27 at most (3 expected not to move); `golden_replay.ron` from checkpoint 3 | S-M (half a day) | WP1, WP2 |
| WP4 | Bookkeeping: doc 03, doc 07, `DESIGN.md` Movement row, audit and alignment pointers, Q0050 closed, plan row | no | XS | WP3 |

**Lane and timing.** Lane B of `2026-11-04-combat-sc2-alignment`, one branch, one gate (`cargo gate package` once,
§5). It comes after the life-regeneration delay (merged, `a27ce2a`) and before combat alignment WP13 (bookkeeping)
and WP14 (scan ranges, lane E), because:

- it edits `catalog.rs`, `catalog/validation.rs`, `units.ron`, `golden_replay.ron` and the movement code, which
  WP14 measures on top of; the scan ranges should be measured on the final movement rule, as WP14 itself says of
  hull turning;
- WP13 closes Q0045, whose item 1(b) resolution must now name this plan's commits in place of WP17's commit g.

**How combat alignment WP13's hand-off changes** (stated again in WP4's hand-back and in this plan's README row):

1. WP13 step 3, "close Q0045, … 1(b) WP17 commits a to f (and g if any)": no commit g exists. The 1(b) line becomes
   "WP17 commits a to f; turn-before-move by plan `2026-11-05-turn-before-move` WP1 and WP3 (Q0050)".
2. Q0050 is closed by this plan (WP4), not by WP13.
3. Follow-up F-8 of the alignment plan loses `TurnBeforeMove`; WP4 edits that bullet to a pointer. F-8 keeps
   `StationaryTurningRate`, which now matters more (D-6, §8 F-1).
4. The `DESIGN.md` Movement row is updated here (WP4), not by WP13. WP13's end-of-lane rewrite of the Combat row is
   unaffected but must keep the Movement row's new sentence.

---

## 2. What is known

### 2.1 Hull turning today (after combat alignment WP17)

- `UnitArchetype::turn_rate: SimAngle` (`catalog.rs`) is the hull rate in binary angle units per tick (65 536 per
  turn), from the authored `turn_rate_deg_per_tick` (validated `0 < rate ≤ 180`, rounded to the nearest unit,
  halves up). The three identities have 20.16° = **3 670** units: a half turn takes 9 ticks.
- `SimWorld::step_movement_and_steering` (`world.rs`) first calls `reset_hull_turn_budgets`, setting every entity's
  `hull_turn_budget: u16` to its `turn_rate`, then `advance_checked_movement`. `Entity::turn_hull_towards`
  (`entity.rs`) rotates `heading` by at most the remaining budget, the short way round, and spends what it used. The
  weapon phase (`combat.rs`, `slew_towards`) gets what the movement phase left. The budget is unhashed
  (`checksum/encoding.rs`, `unhashed { hull_turn_budget }`) because it is reset before its first read every tick.
- **Translation is never gated by facing** (alignment D-8 item 4; `hull_turning_tests.rs` module doc and
  `a_moving_unit_keeps_its_path_while_its_heading_follows`).

### 2.2 Where a unit translates (`movement/locomotion.rs`)

Each actor prepares one `MotionProposal` from the tick-start snapshot; `MotionProposal::commit` writes position,
velocity, `turn_hull_towards(proposal.heading)`, animation, route and order edits together. A unit's own
translation comes from exactly one function, `step_toward`, called by `MotionDraft::propose_step` (walking:
`Move`, `AttackMove`, task legs, flee goals, which are `Move` orders) and `MotionDraft::chase` (pursuing an
`AttackTarget`). It has two branches:

1. **Short step** (`gap <= speed` and the segment is permitted): `position = next`,
   `velocity = next − origin`, `anim = Walk`. **`proposal.heading` is not changed**: the hull does not turn on a
   step that lands on a waypoint or the goal.
2. **Steered step**: `compute_unified_local_steering` (obstacle deflection up to about 40°, `OBSTACLE_WEIGHT`),
   `keep_terrain_view`, `remove_blocked_velocity`, `flank_if_blocked` (pursuit only: a perpendicular sidestep along
   a crowd), then `slide_target` along terrain. If a position results: `position = slide target`,
   `velocity = the steered velocity`, `heading = atan2(velocity)` when the velocity is not zero, `anim = Walk`.

After `step_toward`, `propose_step` consumes the waypoint the step landed on (`pass_reached_waypoint`) and finishes
the order when the step reached the goal; `chase` does the former. Every other path writes no translation:
`walking_goal` holds still for Hold Position, immobile, channeling, and a hull weapon in `Windup`;
`Pursuit::aim_if_in_range` stops and faces the goal (hull weapon) or leaves the turret to the weapon phase;
cluster-arrival and stall finishes stop. The collision phase (`collision.rs`, phase 6) displaces bodies afterwards
and then sets `velocity` to the displacement achieved; it never writes `heading`.

### 2.3 Contact rules that read motion

- `SimWorld::is_anchored_in_combat` (`locomotion.rs`): a unit is anchored if its weapon is turning, winding up or in
  backswing, **or** `velocity == 0` with an attack order at the front and a living target.
- `CollisionBody` and `resolve_pair` (`collision.rs`) and `checked_dynamic_push` (`steering.rs`): Hold Position
  bodies are never pushed by friends; between two dynamic friendly bodies "still" means `velocity == 0`, and a
  moving body with a lower push priority yields to a still one, otherwise it pushes the still one out of its lane.
- Steering (`steering.rs`) treats a neighbour as a co-moving flockmate by the cosine between the unit's heading and
  the neighbour's velocity; a still neighbour is an obstacle.

### 2.4 Stall detection

`note_progress` counts ticks without gaining `STALL_PROGRESS` (0.1) along the remaining route. At
`STALL_REPATH_TICKS` (25, one second) and its multiples a cooled-down ground unit drops its route to repath; at
`STALL_ABANDON_TICKS` (250) it gives the order up; on the last leg within 3 units, 25 stalled ticks finish it
(`check_progress`; `check_pursuit_progress` abandons at 250). A tick spent turning in place gains no ground.

### 2.5 The three identities (verified in `assets/catalog/units.ron` at `a27ce2a`)

| Slug | `display_name` | `sc2_analogue` | Speed per tick | Turn rate | Push priority | Radius | `TurnBeforeMove` source (audit §10.1) |
|---|---|---|---|---|---|---|---|
| `railgun_crawler` | Railgun Crawler | Siege Tank | 0.126 | 20.16 | 40 | 0.75 | `liberty` UnitData:5084 |
| `colossus_mech` | Titan AA-Walker | Thor | 0.1048 | 20.16 | 50 | 0.875 | `liberty` UnitData:5217 |
| `stag_behemoth` | Armored Stag Behemoth | Ultralisk | 0.1652 | 20.16 | 50 | 0.875 | `liberty` UnitData:8033 |

The slugs the owner named exist and are these analogues. Do not confuse `colossus_mech` (the Thor) with
`tripod_annihilator` (the Colossus). No other analogue sets the flag (audit §10.2).

**Where they appear.**
- Matrix (`crates/sim_core/tests/fixtures/matchup_matrix.ron`): `3_immortal_vs_tank` (one `railgun_crawler`, red),
  `14_thor_vs_ultralisk` (one `colossus_mech` vs one `stag_behemoth`), `15_plated_ultralisk_vs_8_marines` (one
  `stag_behemoth`, blue), `27_100_lings_vs_8_ultralisks` (eight `stag_behemoth`, red). Battle 33 has none.
- `golden_replay.ron`: squad `red_siege`, three `railgun_crawler` spawned at heading 0 (+x) at (36, 44), ordered
  `AttackMove(32, 28)` at tick 120 (a facing about 104° from +x, so about 6 ticks of turning, **verify** from the
  trace). `golden_replay_orders.ron`: none. `sim_fixtures::default_battle_sandbox`: spawned state only.
- Tests that spawn a shipped flagged identity: `combat_correctness_tests.rs` (tanks at 10,10; several),
  `collision_and_immobility_tests.rs`, `spells_and_abilities_tests.rs`, `turret_tracking_tests.rs`,
  `determinism_lockstep_tests.rs` (lockstep equality only), `upgrades_and_tech_tree_tests.rs`
  (`stag_behemoth`), `sc2_battle_benchmarks_tests.rs` (siege modes). Most do not move the unit; WP2 finds the ones
  that do.

### 2.6 The reference

- **The flag.** `CUnit` `FlagArray index="TurnBeforeMove"`, set for exactly the three analogues (audit §10.2, the
  lines above). The data hold no tolerance.
- **What it does, informally.** An SC2Mapster forum thread on making units turn before moving ("[Solved] how to make
  units turn slowly when moving", thread 163898, read 2026-11-05 through a search result summary only: the page now
  redirects to CurseForge and could not be read in full) describes the flag as making the unit rotate to face the
  direction it wants to move before it starts moving, and notes such units still turn quickly from a standstill.
  The second point is the `StationaryTurningRate` of these three (40.32° per tick against 20.16, audit §10.2).
- **The tolerance.** Not found in the data or any source within the effort bound (audit §10.2, Q0050). The owner's
  answer fixes it: one tick's turn.

---

## 3. Design decisions

### D-1. The rule (the owner's answer, made exact)

For an archetype with `turn_before_move: true`, on every tick on which locomotion proposes a translation:

```text
direction := SimAngle::atan2(step.y, step.x)          step = the proposal's velocity (D-2)
remaining := |heading.diff(direction)|                binary units, 0 ..= 32 768
if remaining <= hull_turn_budget:                     the turn fits in this tick
    the hull turns to `direction` exactly (turn_hull_towards reaches it) and the unit takes the whole step
else:                                                 it does not fit
    the unit does not translate: position unchanged, velocity 0, animation Walk
    the hull turns towards `direction` by the whole budget
    every step-derived edit of this tick is void (no waypoint consumed by the step, no finish on arrival)
```

- **Units.** Angles are `SimAngle` binary angles (65 536 per turn, counter-clockwise from +x, world frame); the
  budget is binary units per tick; "one tick's `turn_rate_deg_per_tick`" is the archetype's `turn_rate` as loaded.
- **Why the budget, not `turn_rate`.** At the time of the decision they are equal: the budget is reset to
  `turn_rate` at the start of the movement phase, and nothing spends it before the actor's own (single) motion
  commit. Writing the rule against the budget keeps it true by construction if a later rule ever spends hull
  rotation earlier in the phase, and it is the same quantity `turn_hull_towards` spends.
- **The boundary.** `remaining == budget` translates (the owner: "if the angle … exceeds one tick's turn rate,
  rotate in place"; equal does not exceed). A half turn with a budget of 32 768 (180° per tick) always translates,
  so a flag on a unit that turns instantly changes nothing.
- **What the owner's two cases become.** A reversal (180°) at 3 670 per tick: 8 ticks in place
  (8 × 3 670 = 29 360), then on the 9th the remaining 3 408 fits, the hull snaps and the unit steps. A gentle
  curve, where each tick's step direction differs from the heading by at most 3 670 units (20.16°), never stops.
- **"Snap" is not a second write.** `turn_hull_towards` already reaches any facing within the budget exactly
  (`SimAngle::rotate_towards`). The rule only chooses between translating and not; the hull write stays in
  `MotionProposal::commit`, so `heading` keeps exactly one writer in the movement phase (alignment D-8 item 4).

**Invariants** (added to `locomotion.rs`'s `//!` "Invariants" and to `hull_turning_tests.rs`'s module doc):
1. A unit with `turn_before_move` translates by its own motion on a tick only if, after that tick's hull turn, its
   heading equals the direction of the step. (Collision pushes are not its own motion, D-4.)
2. A unit without the flag moves exactly as before this plan (WP1 proves bit-identical results).
3. A turn in place spends the whole remaining hull budget, so the weapon phase gets none that tick (as for any
   unit that turned the full rate while moving).
4. A turn in place never finishes, shortens or abandons the order by itself; at most it adds ticks to the stall
   counter, bounded by D-3.

### D-2. "Step direction" is the step's velocity

The step direction is the direction of `MotionProposal::velocity` as `step_toward` leaves it: the velocity after
obstacle steering, terrain view, blocked-velocity removal and flanking. In the short-step branch that velocity is
`next − origin`, the step itself. Considered:

| Candidate | Problem |
|---|---|
| Direction to the next waypoint (or the goal) | Ignores local steering: a tank deflected 40° around a friend would translate sideways while facing the waypoint, which is exactly the crab-walk the flag exists to forbid. |
| Displacement after terrain slide | Differs from the velocity only when sliding along a wall. The hull of every other unit already faces the velocity (D-8 item 4), so using the displacement would give flagged units a second facing rule for one rare case. |
| **The proposal's velocity (chosen)** | It is already the hull's requested facing for every moving unit (`step_toward`: `heading = atan2(velocity)`), so the gate adds a condition to the existing request instead of a new facing. It covers walking, attack-move, chasing and task legs alike, because all of them translate only through `step_toward`. |

**Consequence for the short step.** For flagged units only, the short-step branch also asks the hull to face the
step (`heading = atan2(velocity)`), where today the hull does not turn on such a step. Otherwise a flagged unit could
translate on a short step while facing elsewhere, breaking invariant 1. Unflagged units keep today's behaviour, so
WP1 stays latent.

**Why not apply it in `MotionProposal::commit`.** The step-derived edits (`pass_reached_waypoint`'s
`remove_prefix += 1`, `propose_step`'s finish on reaching the goal, and the snap `position = goal`) are made
*after* `step_toward` from `proposal.position == next`. If the gate ran in `commit`, a flagged unit within one step
of its goal would pop its order without arriving. Gating inside `step_toward`, before those edits read the
position, makes them see an unmoved unit and do nothing, with no new bookkeeping.

### D-3. Every order, every layer; one validation bound

- **Which motion.** The flag belongs to the unit (`CUnit`), not to an order, so it gates every translation the unit
  makes by itself: `Move`, `AttackMove`, chasing an `AttackTarget` (explicit or acquired), task legs and flee goals.
  No source distinguishes orders (§2.6); a per-order rule would need one. Air units are covered by the same code;
  none of the three flies.
- **Turning in place and the stall counter.** A turn in place gains no ground, so `note_progress` counts it, as it
  counts any tick without progress. The longest turn in place is `ceil(32 768 ÷ turn_rate) − 1` ticks (8 for the
  shipped three). If that reached `STALL_REPATH_TICKS` (25), a reversal alone would drop the unit's route
  mid-turn, and at very low rates (`STALL_ABANDON_TICKS`, 250) abandon the order: the tick loop would run, but a
  flagged unit could never complete a reversal. **Validation rule:** a flagged archetype must turn half a turn
  within `STALL_REPATH_TICKS` ticks, i.e. `ceil(32 768 ÷ turn_rate.raw()) ≤ STALL_REPATH_TICKS`, computed on the
  converted binary rate (exact: 7.2° → 1 311 units → 25 ticks is accepted; 7.19° → 1 309 → 26 is rejected). This is
  safety validation in the sense of `validation.rs`'s module doc (a value the tick loop cannot honour), not a
  balance limit. `movement::tuning` is `pub(crate)`, so the validation reads `STALL_REPATH_TICKS` by name.
- **Rejected alternative: pausing the stall counter while turning.** It would need the gate's outcome before
  `check_progress`, which runs before the step, or an undo after it; both add state or ordering coupling for a case
  the bound rules out.
- **A flag on a unit that cannot move** (`max_speed == 0`) is a value nothing reads; rejected, as
  `hp_regen_delay` rejects a delay on a unit that does not regenerate.

### D-4. Interactions

| Situation | Behaviour | Why |
|---|---|---|
| **Hold Position** | Unchanged: `walking_goal` returns before any step; the weapon phase turns the hull to fire as today. | No translation, nothing to gate. |
| **Hull weapon in `Windup`**, **channeling**, **immobile** (Siege Mode) | Unchanged: no step. | As above. |
| **In range of its target** (`aim_if_in_range`) | Unchanged: stops and turns towards the goal within the budget. | Already a turn without translation. |
| **Attack-move** | Gated like a move. If an enemy comes into range during a turn in place, acquisition and `aim_if_in_range` take over on the next tick as for any unit. | D-3. |
| **Chasing** (including `flank_if_blocked`) | Gated. A perpendicular flank step makes the unit turn towards the flank before it sidesteps. | Invariant 1. Risk R-2 (oscillation) is measured. |
| **Friendly push and separation** | Not gated. The collision phase displaces a flagged unit like any other; its heading is not changed by a push. | A push is passive displacement, not the unit's motion (SC2 pushes idle and turning units alike: doc 03 §1.4 push priority). Gating it would let a turning unit refuse to give way. |
| **Contact rules while turning in place** | The unit's velocity is 0, so it is "still" for `checked_dynamic_push` and an obstacle for neighbours' steering; with an attack order at the front and a living target it counts as anchored for one tick (`is_anchored_in_combat`). | No new state: the rules already read velocity, and a turning unit is in fact not moving. Pinned by a test (WP1) so a later change is visible. |
| **Hostile contact** | Unchanged (hard, reciprocal). | Not motion. |
| **Hull turn budget** | The movement phase spends all of it on a turn in place; the weapon phase gets none that tick. | Invariant 3; alignment D-8 item 4's order. |
| **Animation** | `Walk` while turning in place. | The unit is executing its move (treads, legs); `Idle` would flicker between steps of a curve. Presentation only. |

### D-5. Data model and validation

**Field.** `turn_before_move: bool` in `UnitArchetypeDef` with `#[serde(default)]` (omitted reads `false`, the
game's default for an unset flag), and in `UnitArchetype` (`#[serde(default)]` as well, because `UnitArchetype`
derives `Deserialize`). Next to `turn_rate`. `///` doc on `UnitArchetype::turn_before_move`:

> Whether the hull must face a step before the unit takes it. When `true`, on a tick whose step direction lies
> further from the heading than the hull can turn this tick (its remaining `hull_turn_budget`, which starts at
> `turn_rate`), the unit turns in place and does not translate; once the remaining turn fits, it faces the step
> exactly and takes it. Collision pushes are not gated. `StarCraft` II stores this as the `CUnit` flag
> `TurnBeforeMove`.

`StructureArchetypeDef` does **not** get the field: structures have `turn_rate` 0 and the game data set the flag on
no structure. The structure constructor writes `false`. (A walking structure such as an uprooted crawler is out of
scope, §8 F-4.)

**Charter §3.3.** This is a property of the archetype read by mechanism code, like `push_priority`; no identity is
named in Rust (C5), and the three values live in `units.ron`.

**Validation** (`catalog/validation.rs`, a new private `fn turn_before_move(definition, flagged, max_speed,
turn_rate_deg_per_tick)` called from `UnitArchetypeDef::validate` after `rate_per_tick` for the hull rate):

| Authored | Result |
|---|---|
| `turn_before_move` omitted or `false` | accepted |
| `true`, `max_speed > 0`, half turn within 25 ticks | accepted |
| `true`, `max_speed == 0` | `SimCoreError::CatalogInvalidValue { field: "turn_before_move", reason: "must be false when max_speed is zero" }` |
| `true`, `ceil(32 768 ÷ binary_angle(rate)) > STALL_REPATH_TICKS` | `CatalogInvalidValue { field: "turn_before_move", reason: "requires a hull rate that turns half a turn within the stall repath window (25 ticks)" }` |
| not a bool (`turn_before_move: 1`) | the RON parse error the loader already returns (typed) |

The binary rate is computed with the loader's own `binary_angle`, so validation and simulation cannot disagree.

### D-6. What this plan does not change: the stationary rate (superseded by Amendment 1 and D-10)

The owner's rule turns in place at `turn_rate` (20.16° per tick). The game data give these three a
`StationaryTurningRate` of 40.32° per tick (audit §10.2), and the forum description (§2.6) suggests a unit turning
from a standstill uses it. With turn-before-move, a turn in place *is* stationary, so the game may turn these
units about twice as fast in place as this plan does (5 ticks for a reversal instead of 9). This is not applied:
the owner's answer names `turn_rate_deg_per_tick`, and the stationary rate is alignment follow-up F-8 with its own
rule question (which rate applies on which tick). Recorded as §8 F-1 and risk R-4; WP4 adds one sentence to F-8
saying that turn-before-move makes it observable.

### D-7. Determinism and checksums

- Integer binary-angle arithmetic only: `SimAngle::atan2` (deterministic table), `diff`, `unsigned_abs`, a `u16`
  comparison. No float enters state (C7).
- **No new entity state.** The flag is catalog data (not hashed); the gate reads `heading` and
  `hull_turn_budget`, both existing. `CHECKSUM_SCHEMA_VERSION` stays **10**; `checksum/encoding.rs` unchanged.
- **WP1** (every flag `false`): bit-identical by construction (the gate and the short-step heading request are
  behind the flag). Matrix, battle 33, both goldens and the sandbox checksum must be unchanged; anything else stops
  the package.
- **WP3** (data): `golden_replay.ron` is expected to change from **checkpoint 3** (ticks 101 to 150), the first
  checkpoint containing tick 120, when `red_siege` receives its first order. No flagged unit translates before
  then. `golden_replay_orders.ron` and the sandbox checksum must not change.
- **Re-record rule.** A golden that moves in WP1: stop. In WP3: `golden_replay.ron` may be re-recorded on Linux in
  the same commit only after the first differing checkpoint is shown to be 3 or later and traced to a `red_siege`
  turn in place (per-tick heading and position of the three tanks around tick 120). A difference before checkpoint
  3, or in the orders golden, stops the package. `cargo gate package` runs the golden replays in release and on
  `wasm32-wasip1`; CI's determinism jobs re-verify Linux, Windows, macOS ARM64 and `wasm32-wasip1`; a red job stops
  lane B until explained.

### D-8. Measurement

**Tool** (combat alignment D-5; always release, in the background, logs and outputs under
`~/scratch/turn_before_move/<step>/`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name>
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --all-battles --seeds 16 --jobs 8 --out ~/scratch/turn_before_move/<step>/matrix \
    --compare ~/scratch/turn_before_move/<previous step>/matrix/matrix.csv
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/turn_before_move/<step>/b33
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 27_100_lings_vs_8_ultralisks --seeds 16 --out ~/scratch/turn_before_move/<step>/b27   # likewise 3, 14, 15
```

`--seeds 16` adds 16 seeds to the four of record (20 in all); battle 33's 64 plus the four of record is the 68 of
the investigation. `<step>` is `base` (this plan's branch point, `main` after the life-regeneration merge; compare
against the life-regeneration plan's last matrix to confirm the branch reproduces it), `wp1`, `wp3`.

**What the per-battle traces show.** The per-unit CSV has position and velocity but no heading. A flagged unit
turning in place while chasing is classed `blocked` (`trace/activity.rs`: a target outside its band and less than
10 % of its speed moved). Report, for battles 14, 15 and 27, the flagged units' `blocked` tick count before and
after (means over 20 seeds) as the trace-level evidence of turning in place. No tool change is planned; if the
counts cannot separate turning from crowding, say so rather than adding a column in this plan (§8 F-5).

**Expectations and stop rules** (expectations, not measurements):

| Step | Whole matrix, 20 seeds | Battles 3, 14, 15, 27 | Battle 33, 68 seeds | Goldens | Stop if |
|---|---|---|---|---|---|
| WP1 | bit-identical (all rows) | identical | bit-identical | unchanged | anything moves |
| WP3 | only 3, 14, 15, 27 may move | **3:** expected bit-identical (the tank stands in range from the first tick and never translates). **14:** both sides spawn facing each other (alignment D-8 item 5) and walk straight at each other; expected identical or a few ticks later; thor win on every seed. **15:** after each kill the ultralisk turns to the next marine; end tick later by a few ticks, ultralisk lead life somewhat lower, still inside `(350, 1000)`, ultralisk win on every seed. **27:** the largest effect: ultralisks turning to chase zerglings around them; end tick later, ultralisk lead life lower, `red: (6, 999)` expected to hold. | bit-identical (no flagged unit) | `golden_replay.ron` from checkpoint 3 only; orders golden unchanged | any other battle moves; a battle's winner over its seeds flips (Amendment 4's reading); battle 33 moves; a golden differs before checkpoint 3; a flagged unit stops translating for more than 25 consecutive ticks on any seed (R-2) |

A stop means report, not widen. An expectation that fails on a seed of record is re-recorded only with the
measured reason in the commit (alignment D-1 item 6), to the smallest ranges that hold on the seeds of record. If
battle 14 moves, also measure it with each of its two flags alone on a scratch catalog (alignment D-5 item 5), so
the evidence says whose turning moved it.

**Battle 33.** Contains no flagged identity; its comment in `matchup_matrix.ron` does not change (alignment D-6
applies only to commits that move it).

**Evidence.** New file `docs/evidence/<date WP1 starts>-turn-before-move.md`, created by WP1 with "How to
reproduce" (pointing to [the alignment evidence's section](2026-11-04-combat-sc2-alignment.md#how-to-reproduce)),
the baseline and WP1's section; WP3 appends its own. Each section: commit and change; matrix against the step
before ("bit-identical", or the moved battles with survivors, end tick and lead life, means over 20 seeds, and
failures of record and over all seeds); battle 33 (wins / losses / unresolved, survivors mean and SD, seeds of
record); the flagged units' `blocked` counts in 14, 15, 27; goldens (first differing checkpoint and its cause).

### D-9. Documents

| Document | Change | WP |
|---|---|---|
| `docs/questions/open/0050-turn-before-move.md` | `## Resolution` (WP4 step 3), then `git mv` to `closed/`. Nothing under `## Answer` is edited. | WP4 |
| `docs/03-sc2-mechanics-reference.md` §1.4 | New row after "Turn Rate & Stationary Turn Rate": **Turn Before Move** — "A flagged unit (Siege Tank, Thor, Ultralisk) turns in place until its next step lies within one tick's turn, then faces it and moves" — why: "Heavy units pivot before reversing instead of sliding backwards" | WP4 |
| `docs/07-sc2-patch-5-0-14-roster-and-faction-mapping.md` §2.3 | After the "Siege Tank, Thor, Ultralisk (`360`)" bullet: "These three also set `TurnBeforeMove` (`turn_before_move: true`): a step further from the heading than one tick's turn is preceded by turning in place, so a reversal takes 9 ticks before the first step." | WP3 |
| `DESIGN.md` §4, Movement and pathfinding row | Status text: "…; hulls turn at StarCraft II's per-tick rates, at most once per tick, and units flagged `turn_before_move` turn in place before a step their hull does not face (Q0050)". Tests column unchanged (`hull_turning_tests.rs`, `push_priority_tests.rs` already listed). | WP4 |
| `docs/evidence/2026-11-04-catalog-time-audit.md` §10.2 `TurnBeforeMove` bullet and §10.6 | One line each: "Implemented by plan `2026-11-05-turn-before-move` (owner tolerance: one tick's turn, Q0050), WP1 `<hash>`, WP3 `<hash>`." | WP4 |
| `docs/plans/2026-11-04-combat-sc2-alignment.md` §8 F-8 | Replace "`TurnBeforeMove` (Q0050)" with a pointer to this plan; add: "turn-before-move makes the stationary rate observable: these units turn in place at `TurningRate`". | WP4 |
| `docs/evidence/2026-11-04-combat-sc2-alignment.md` §14.8 | One line under "Not applied": "turn-before-move later applied by plan `2026-11-05-turn-before-move` (Q0050), [evidence](2026-11-05-turn-before-move.md)" (link to this plan's evidence file name as created). | WP4 |
| `docs/plans/README.md` | This plan's row: `Proposed` → `Active` (WP1) → `Done` with commits and the WP13 hand-off (WP4) | WP1, WP4 |

---

## 4. File ownership

| File | WP |
|---|---|
| `crates/sim_core/src/catalog.rs` (field in `UnitArchetypeDef` and `UnitArchetype`, both constructors, docs) | WP1 |
| `crates/sim_core/src/catalog/validation.rs` (rule and unit tests) | WP1 |
| `crates/sim_core/src/movement/locomotion.rs` (`MotionEntry::turn_before_move`, `StepContext` hull gate, `step_toward`, pure helper and its unit tests, module invariants) | WP1 |
| `crates/sim_core/src/entity.rs` (doc of `heading` and `hull_turn_budget` only) | WP1 |
| `crates/sim_core/tests/hull_turning_tests.rs` | WP1, WP3 |
| `crates/sim_core/tests/push_priority_tests.rs` | WP1 |
| `crates/sim_core/tests/damage_and_shields_tests.rs`, `crates/sim_core/tests/attack_move_simulation_tests.rs` (their `UnitArchetype` literals gain `turn_before_move: false`) | WP1 |
| tests restated by WP2 (as found) | WP2 |
| `assets/catalog/units.ron` (the three identities only) | WP3 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`; `matchup_matrix.ron` only if an expectation moves | WP3 |
| new `docs/evidence/<date>-turn-before-move.md` | WP1 creates; WP3 appends |
| documents of D-9 | as listed |

RON fixtures that spell out archetypes (`hull_turning_tests.rs`'s `unit_ron`, `sim_fixtures/tests/trace_tests.rs`,
`weapon_cadence_tests.rs`) need no edit: the field defaults to `false`. The compiler finds any missed struct
literal (`client` uses `..base` or `clone()`; **verify** with the build).

---

## 5. Work packages

Work in a worktree with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name>`; check
`df -h / ~/scratch` first; outputs under `~/scratch/turn_before_move/`; remove the target directory after merge
(charter §7). Write each failing test first and paste its red run into the hand-back. Every commit names its WP and
Q0050. No `unwrap`; `expect` only with `invariant:`; no catalog identity in production Rust (C5); functions under
100 lines (C8; `step_toward` is near the limit, so the gate goes in a helper). Mechanism tests use scratch
archetypes; the shipped-data test finds its archetypes by property. Never run a long command in the foreground.
Check `docs/questions/open/` before the last commit.

**Gate (charter §4.3).** `cargo gate quick` is optional while working. **`cargo gate package` once**, after merging
`main` into the branch and before the final commit; its stamp goes in the hand-back. It runs the golden replays in
release and on `wasm32-wasip1` for this simulation change. The orchestrator runs `cargo gate merge <branch>`.

### WP1: Code: the flag, its validation, and the step gate (all `false`)

1. **Baseline.** Run D-8's three commands as `base`; create the evidence file with "How to reproduce" and the
   baseline. Set this plan's README row to `Active`.
2. **Failing tests first.**
   - `catalog/validation.rs` unit tests (beside the hull-rate range tests, using `rejected_field`):
     - `turn_before_move_is_rejected_on_a_unit_that_cannot_move`: a shipped unit with the flag set and
       `max_speed = 0.0` gives the typed error naming `turn_before_move`; with its own speed it validates.
     - `turn_before_move_needs_a_half_turn_within_the_stall_repath_window`: flagged, rate 7.2 validates, 7.19 is
       rejected naming `turn_before_move`; 20.16 and 180 validate.
     - `an_omitted_turn_before_move_reads_as_false`: every shipped unit parses with `false` (before WP3).
     The first two do not compile before the field exists and fail with the field and no rule.
   - `locomotion.rs` unit tests of the pure helper (D-1), e.g. `fn hull_allows_step(heading: SimAngle, direction:
     SimAngle, budget: u16) -> bool`:
     - `a_step_within_the_budget_is_allowed` (0, budget − 1, budget exactly: allowed);
     - `a_step_beyond_the_budget_is_refused` (budget + 1; a half turn with budget 3 670: refused);
     - `a_half_turn_budget_allows_any_step` (32 768 against a half turn, both signs of `diff`).
   - `hull_turning_tests.rs` (scratch archetypes; `unit_ron` gains a `turn_before_move` parameter; shooter rate
     20.16, speed 0.15). Update the module doc ("translation is gated by facing only for archetypes flagged
     `turn_before_move`"):
     - `a_turn_before_move_unit_turns_in_place_on_a_reversal`: facing +x, `Move` due west. Ticks 1 to 8: position
       unchanged, heading turned exactly `rate` per tick; tick 9: heading exactly west and the unit has moved west
       by its speed. Red before the gate (moves on tick 1).
     - `a_turn_before_move_unit_does_not_stop_for_a_gentle_turn`: `Move` 15° off its heading: it moves on tick 1
       and its heading equals the step direction. Green before and after (guard for the owner's "without stalling
       on gentle path curves").
     - `a_turn_before_move_unit_moves_on_the_tick_its_remaining_turn_fits`: `Move` 30° off: tick 1 in place, tick
       2 moves facing the step exactly. Red before.
     - `a_turn_before_move_unit_turns_before_it_chases_a_target_behind_it`: an `AttackTarget` on a target behind
       and out of range: no translation until aligned, then it closes. Red before.
     - `turning_in_place_neither_finishes_nor_shortens_the_order`: `Move` to a point 0.1 behind (within one step):
       the order is still at the front after tick 1 and the unit has not moved; it arrives and the order ends only
       after it faces the point. Red before (today it arrives on tick 1 without turning).
     - `a_turn_before_move_unit_on_hold_position_turns_to_fire_and_never_moves`: guard, green before and after.
     - Extend `turning_is_deterministic_and_its_budget_is_not_hashed` with a flagged shooter on a reversal.
     - Keep `a_moving_unit_keeps_its_path_while_its_heading_follows` unchanged (the unflagged case, invariant 2).
   - `push_priority_tests.rs` (charter §4 subject "push priority versus Hold Position"; add a small scratch-catalog
     helper modelled on `hull_turning_tests.rs`, and update the file's `//!` list):
     - `a_unit_turning_in_place_is_pushed_like_a_standing_unit`: a flagged unit of lower push priority, ordered to
       reverse, is displaced by a moving friend of higher priority walking into it on the same tick, and its
       heading still turned exactly its rate (pushes never change heading, D-4). Green or red before depending on
       whether it moved anyway; the assertion that matters is after.
     - `a_push_is_not_gated_by_facing`: a flagged idle unit facing +x is pushed along y by a moving friend: it is
       displaced although it does not face y. Green before and after (guard).
3. **Implement.** D-5's field, default, docs and validation; `MotionEntry` gains `turn_before_move: bool` (read in
   `motion_entry` from the archetype, beside `push_priority`); `StepContext` gains `hull: Option<HullGate>` with
   `HullGate { heading: SimAngle, budget: u16 }`, `Some` only for flagged archetypes (built in
   `MotionDraft::step_context` from the tick-start actor); `step_toward` calls a private helper after either branch
   that, for `Some(gate)` and a non-zero velocity, sets `proposal.heading = atan2(velocity)` and, when
   `hull_allows_step` is false, sets `position = origin`, `velocity = 0`, `anim = Walk` (D-1, D-2). The `refused`
   result is unchanged (it reports terrain, not facing). `MotionProposal::commit` is unchanged. Add the
   `turn_before_move: false` field to the two test literals. Update the docs of `Entity::heading` (the motion
   phase may hold a flagged unit in place while it turns) and `locomotion.rs`'s module invariants (D-1). No
   `// PERFORMANCE RATIONALE` block is needed: the gate is one angle difference and one comparison per translating
   flagged actor, with no allocation.
4. **Proof (D-8, row WP1).** Matrix, battle 33, both goldens and the sandbox checksum unchanged. Evidence section.
5. Commit `feat(sim_core): units flagged turn_before_move turn in place before a step their hull does not face,
   false everywhere (turn before move WP1, Q0050)`, stating that no result or checksum moved and why (D-7).

### WP2: Tests that would break for another reason (test-only)

1. **Probe.** On a scratch copy of `units.ron` with the three flags set (WP3's edit, uncommitted), run
   `cargo gate quick` (or the `sim_core` and `sim_fixtures` test targets) and list every failing test other than
   the goldens and the matrix expectations.
2. For each, decide: if it asserts a movement time, path or position of a shipped flagged unit that is not its
   subject (for example a siege or turret test that walks a tank first), restate it so it does not depend on
   instant translation (spawn facing the move, or a scratch archetype), green before and after, one commit:
   `test(sim_core): <tests> no longer depend on the tank, thor and ultralisk translating while turned away (turn
   before move WP2, Q0050)`. If a test's subject *is* the movement of one of these units, it changes in WP3 with the
   data, with its reason.
3. If the probe finds nothing, WP2 makes no commit; say so in the evidence.

### WP3: Data: the three flags

1. **Failing test first**, in `hull_turning_tests.rs`: `shipped_turn_before_move_units_turn_in_place_on_a_reversal`.
   Load the shipped catalog, collect the archetypes with `turn_before_move` (by property), assert there is at least
   one, and for each: spawn facing +x on a flat map, order a `Move` due west, and assert no translation for
   `ceil(32 768 ÷ turn_rate) − 1` ticks and a step on the next. Red before the data edit (none flagged).
2. Edit `units.ron`: `turn_before_move: true,` after `turn_rate_deg_per_tick` on `railgun_crawler`,
   `colossus_mech` and `stag_behemoth` only.
3. Measure (D-8, row WP3). Trace `golden_replay.ron`'s first differing checkpoint to `red_siege` (D-7), re-record
   it, and reproduce in debug, release and `wasm32-wasip1`. Re-record a matrix expectation only under D-8's rule.
   Write the evidence section.
4. Doc 07 §2.3 sentence (D-9).
5. Commit `data(catalog): the Siege Tank, Thor and Ultralisk analogues turn before they move (turn before move
   WP3, Q0050)`, citing `liberty` UnitData:5084, :5217, :8033, the owner's tolerance, every moved battle with its
   measured reason, and the golden's first differing checkpoint.

### WP4: Bookkeeping

1. Doc 03 §1.4, `DESIGN.md` Movement row, the audit's §10.2 and §10.6 lines, the alignment plan's F-8 and the
   alignment evidence §14.8 line (D-9).
2. Check that `DESIGN.md`'s links and evidence still resolve (C17) and that no path named in the charter changed
   (C20).
3. Q0050: add `## Resolution`: "Carried out as option (b) with the owner's tolerance (turn in place while the step
   lies further than one tick's turn; then face it and move): `turn_before_move` and the step gate (WP1
   `<hash>`), the three flags (WP3 `<hash>`); measurement in `<evidence link>`. Not applied: the game's
   `StationaryTurningRate` for turning in place (alignment plan F-8). Setting the three flags to `false` restores
   the old behaviour." Then `git mv` it to `closed/`.
4. Hand-off to combat alignment WP13 (§1), in the hand-back and in this plan's README row.
5. This plan's row: `Done` with the WP commits.
6. Merge `main`, run `cargo gate package` once, and commit `docs: turn before move done; Q0050 closed (turn before
   move WP4)`.

---

## 6. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| R-1: the gate changes an unflagged unit | WP1 not latent | the gate and the short-step heading request are behind `Some(gate)`; D-8 stop rule; bit-identical proof on matrix, battle 33, goldens, sandbox |
| R-2: oscillating step directions (a flank side that toggles, steering deflection jittering around 20°) keep a flagged unit turning without stepping | a tank or ultralisk stalls in a crowd; repath or abandonment | measured: the `blocked` counts and the stop rule "more than 25 consecutive ticks without translation" (D-8); if it happens, stop and report with the trace; a fix (for example a minimum step-direction hold) is a new rule needing its own decision, not a widening |
| R-3: residual short steps at a waypoint or the goal point far from the heading | a flagged unit pivots to cover a few hundredths of a unit | the waypoint hand-off (`drop_passed_waypoints`) drops corners before the unit reaches them; the measurement shows how often; recorded as an accepted consequence of the owner's rule (no exemption is specified) |
| R-4: StarCraft II turns these units in place at the stationary rate (40.32°), twice ours | reversals here take up to 4 ticks longer than in the game | superseded by Amendment 1: applied by WP5 and WP6 (D-10); formerly D-6, follow-up F-1 and alignment F-8 |
| R-5: a turning unit counts as still and, with an attack order, as anchored for that tick | friendly traffic flows around it instead of being pushed | consistent with the existing rules (D-4); pinned by a test; visible in battle 27's measurement |
| R-6: tests spawning shipped tanks, thors or ultralisks assumed instant translation | WP3 fails tests for a reason that is not its cause | WP2's probe and test-only restatements first |
| R-7: a client test or the skirmish AI trains a flagged unit and pins timing | a downstream test moves | `cargo gate package` selects downstream contracts; such a test is restated in WP2 or explained in WP3 |
| R-8: determinism on other targets | red CI job | integer arithmetic only; CI re-verifies; red stops lane B |

---

## 7. Determinism summary

No float, no new entity state, no schema bump (stays 10). WP1: nothing moves. WP3: `golden_replay.ron` re-recorded
from checkpoint 3 with a traced cause; the orders golden and the sandbox unchanged. Checks: `cargo gate package`
(golden replays in release and `wasm32-wasip1`), then CI's determinism jobs on Linux, Windows, macOS ARM64 and
`wasm32-wasip1`.

---

## 8. Out of scope and follow-ups

Recorded here so they can be brought back (charter §8, Q0002):

- **F-1: `StationaryTurningRate`** (brought into scope by Amendment 1: WP5 and WP6) for turning in place (D-6; alignment F-8). Would need a second rate field and a
  rule for which rate a tick uses; this plan's turn in place is the natural place it would apply.
- **F-2: a step-direction hold or hysteresis** if R-2 is observed. Not specified by the owner or found in a source.
- **F-3: turning while standing to fire.** The weapon phase's hull turns are unchanged; only translation is gated.
- **F-4: structures that walk** (an uprooted crawler): `turn_rate` 0 for structures, no flag; unchanged.
- **F-5: a heading column in the trace's per-unit CSV**, if the `blocked` counts cannot separate turning in place
  from crowding.
- **F-6: client presentation of turning in place** (for example a dedicated pivot animation). `Walk` is used.
