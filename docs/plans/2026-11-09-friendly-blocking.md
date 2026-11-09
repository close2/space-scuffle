# Blueprint: Friendly Blocking: Second-Line Units That Vibrate Instead of Reaching Range

The owner reported this in chat on 2026-11-09: "in the showcase video some units only vibrate when reaching the
target but don't come close enough to actually fire (blocked by friendly units)". This plan reproduces the defect,
sets out what StarCraft II does when friendly units are in the way, and replaces our behaviour with rules that follow
the game:

- a blocked unit walks around the crowd on one side it keeps, or waits standing still;
- it never steps into a gap it cannot pass and then gets pushed back out on the next tick;
- moving units push idle friends exactly as the game's data says, and never push a unit that is firing or holding
  position.

**Status of this document.** This is a Stage 1 blueprint under charter §6, written on 2026-11-09 against `main` at
`33a8e82`. Line numbers drift, so search for the quoted symbol. Claims not confirmed while planning are marked
**verify**. A planning probe ran on this commit (§2.1). Its source is kept at
`~/scratch/blockplan/scripts/zz_block_probe.rs` and was not committed.

**Scope among the movement plans.** Plan `2026-11-05-sc2-deviation-fixes` WP16 is the movement research package.
This plan carries out the part of it about arrival, crowding, pushing and moving into range past friendly units:
- B-48 and B-93 in full;
- the vibration findings M-19 and M-21, measured;
- the showcase defect, with new IDs.

The path search belongs to plan `2026-11-09-sc2-pathfinding` (branch `plan-pathfinding`, not yet written when this
plan was). That covers corridors, funnel, clearance field, waypoint handoff and route repair. The ramp plan
(`plan-ramps`) owns ramps. §4 draws the file boundary.

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II | Standing rule | D-1 to D-5 |
| [Q0035](../questions/closed/0035-melee-contact-rules.md) melee contact rules | Answered "no change … for now". B-50 stays recorded | §8 (out of scope) |
| [Q0073](../questions/open/0073-allied-push-priority.md) how `AlliedPushPriority` combines with `PushPriority` | Default: our one `push_priority` field takes the game's `PushPriority` (10 everywhere). The Ultralisk analogue's 5.0.14 "pushes allied units out of the way" is modelled as priority 11 | D-4, WP5 |
| [Q0074](../questions/open/0074-surround-walk.md) may a blocked second-line unit walk around a firing line it cannot pass, where the game would leave it standing (Blizzard's own guide tells the player to fix this by hand)? | Default: yes, as an "allows more" deviation (Q0051), configurable per archetype through `surround_walk: bool` (default `true`). The waiting rule R1 holds either way | D-2, WP3 |

---

## 1. Summary

1. **The defect, measured (§2).** In the showcase battle (scenario `assets/scenarios/showcase.ron`, seed
   1592639215), 6 of each side's 10 attackers form a firing line at weapon range from the enemy town hall. The other 4
   stand one body behind it:
   - **Player 0, the side the video follows.** Pulse bots 4, 8 and 10 have no target: their edge distance to the
     hall, 5.60 to 5.69, lies just beyond scan range 5.5. They flip direction every tick (period 2, ±0.02 to 0.03
     world units, 213 to 249 reversals). After 10 s without progress they give up the attack-move (ticks 909 to 970)
     and stand idle until the end, never firing.
   - **Player 1.** Pulse bots 22 and 24 have the hall as their target but stand 0.42 to 0.67 beyond weapon range.
     They flip every tick at ±0.08 to 0.11 (393 and 423 reversals in 600 ticks), drop the chase every 250 ticks and
     take it up again 51 ticks later. They never fire.
2. **Cause.** Each vibrating unit is wedged against two friends that do not yield: a firing unit, which counts as
   anchored, and another stalled unit. Their surfaces are closer than one body width apart.
   - Local steering picks the nearer blocker's side afresh every tick. The step either ends in contact that the
     collision phase undoes, or steers toward the other side.
   - The result is a two-tick limit cycle with zero net progress.
   - Only chasing units have the flanking walk (`flank_if_blocked`), and its trigger never fires here, because the
     sideways step is fast enough to pass its "blocked" test. Attack-moving units without a target have no way
     around at all.
   - The 10 s stall rule then turns the cycle into a permanent idle.
3. **StarCraft II (§3).**
   - Moving units push idle friends. Units that are firing, holding position, patrolling or moving are not pushed
     (community-sourced; Hold Position is documented in patch notes).
   - Enemies do not push each other (documented).
   - A blocked unit eventually gives up its move (community).
   - Units form a concave when there is room, and a unit that finds no room stands still. No source describes a unit
     that steps in and out every tick.
   - `PushPriority` is 10 for every unit in 5.0.14. The Ultralisk alone gained `AlliedPushPriority` 1.
4. **Rules (§5).**
   - **R1 waiting:** no step into a gap narrower than the unit; a blocked unit with no admissible step stands still.
   - **R2 surround walk:** a blocked attacker, chasing or attack-moving, walks along the crowd on a side it keeps
     until it has a clear approach.
   - **R3 push table:** who pushes whom, in one table, with Hold Position and firing units never pushed.
   - **R4 the stall rule** counts the surround walk as progress.
   - **R5 data:** the game's push priorities (B-48).
   - **R6 burrowed bodies** collide only with burrowed bodies (B-93).
5. **Work packages (§6).**
   - WP0: questions and IDs.
   - WP1: measurement tooling and a reproduction, test-support only.
   - WP2: R1, waiting.
   - WP3: R2 and R4, the surround walk.
   - WP4: R3, the push table pinned by tests, expected bit-identical.
   - WP5: R5 data.
   - WP6: R6.
   - WP7: bookkeeping.

   Each behaviour commit is measured on the matrix with the stop rule.

---

## 2. Diagnosis

### 2.1 The probe

The probe was a temporary integration test in `sim_core`, not committed. It builds the showcase world with
`sim_fixtures::scenario_world` (map `showcase_canyon`, loadout `battle_automaton` for both slots, seed 1592639215). It
gives every armed non-harvester an `AttackMove` to the other base's approach cell, exactly as the client's
`BattleDefinition::instantiate` does: (52.5, 52.5) for player 0 and (5.5, 5.5) for player 1. It then steps 1,300
ticks with empty bundles and writes one CSV row per unit per tick: position, achieved velocity, target, weapon phase,
order, `flank_left` and `stalled_ticks`. Analysis scripts are in `~/scratch/blockplan/scripts/` (`an.py`, `sum.py`,
`nb.py`, `steer.py`; run with `python3 -I`). The release build took about 5 minutes and the run 5.5 s.

**Video.** The video at <https://close2.github.io/space-scuffle/showcase.mp4> (44 s) was inspected with ffmpeg stills.
In its `clash` scene, frames 600 to 900, player 0's group of 8 pulse bots and 2 rhino beetles stands in a clump at
the upper-left of the enemy hall. At most two or three beams are visible in any frame. The probe shows the same
formation. The video renders the same scenario through the client, so the probe and the video agree in kind. Their
tick-exact agreement is **verify**: the storyboard says the base falls near tick 1,225, while in the probe it still
stands at 1,300.

**Bodies.**
- Pulse bot (Marine analogue): radius 0.375, speed 0.126, range 5.0, scan 5.5, push priority 20.
- Rhino beetle (Roach analogue): radius 0.5, range 4.0, priority 30.
- Town hall `foundry_core_dome`: radius 2.5, immobile. Player 0's hall is at (8.5, 8.5), player 1's at (55.5, 55.5).
- Range and scan are measured edge to edge.
- The welder drones are harvesters and take no order.

### 2.2 What every attacker does, ticks 600 to 1,300

**Snapshots of each side's 10 attackers:**

| Tick | Player 0 | Player 1 |
|---|---|---|
| 700 | 5 in range, 5 attack-moving without a target | 4 in range, 1 out of range with a target, 5 without a target |
| 800 to 900 | 6 in range, 4 without a target | 6 in range, 2 out of range, 2 without a target |
| 1,000 to 1,300 | 6 in range, 4 **idle** | 6 in range, 2 out of range, 2 idle |

**Units that never get into range** (reversal: consecutive achieved velocities with cosine below −0.3, both above
0.005 per tick):

| Unit | Side | State | Edge distance to hall | Reversals, ticks 600 to 1,300 | Step size | End |
|---|---|---|---|---|---|---|
| 4 | 0 | attack-move, no target | 5.606 (scan 5.5) | 249, from tick 676 | ±0.029 | idle at 925 (no progress) |
| 8 | 0 | attack-move, no target | 5.686 | 213, from 700 | ±0.027 | idle at 970 |
| 10 | 0 | attack-move, no target | 5.603 | 234, from 659 | ±0.021 | idle at 909 |
| 5 | 0 | attack-move, no target | 6.254 | 24 | small | idle at 925 |
| 22 | 1 | chasing the hall | 5.42 (range 5.0) | 393, from 698 | +0.11 / −0.08 | gives up at 947 and 1,248; takes it up again 51 ticks later |
| 24 | 1 | chasing the hall | 5.67 | 423, from 726 | ±0.08 to 0.11 | gives up at 976 and 1,277 |
| 21, 23 | 1 | attack-move, no target | 5.6 to 5.7 | 8 and 218 | ±0.03 | idle at 994 |

The firing line itself (6, 7, 9, 11, 12, 13 and 17 to 20) has zero or one reversal. Those units stand at edge
distances 4.92 to 4.99, inside range 5.0.

### 2.3 Mechanism

**Unit 4, ticks 750 to 757, period exactly 2.**
- Its goal direction is (1.0, −0.05).
- On its left, pulse bot 9 is firing, so it is anchored: `is_anchored_in_combat` covers Windup, Backswing, and zero
  velocity with a live target.
- On its right, pulse bot 10 is stalled; its velocity is not zero, so it does not yield.
- Unit 4 overlaps both, by −0.009 and +0.014.
- `compute_unified_local_steering` takes the nearest blocker on each side; the nearer one alternates between 9
  (0.741) and 10 (0.718 / 0.764).
- The achieved velocity alternates exactly between (−0.0192, 0.0215) and (0.0192, −0.0215).

**Unit 22.**
- Its left blocker is pulse bot 18 and its right blocker pulse bot 17. Both are firing, 0.76 apart: touching, so
  there is no gap to pass.
- Unit 22's achieved step alternates between about +0.11 toward one flank and −0.08 back. The contact phase undoes
  the step, through `slide_around_fixed` against the anchored body, capped at the unit's speed.
- `flank_left` stays 0 throughout. `flank_if_blocked` never triggers, because the steered velocity after
  `remove_blocked_velocity` stays above half walking pace.

Four defects combine:

| # | Defect | Where |
|---|---|---|
| D-a | Steering treats two non-yielding bodies whose surface gap is below the unit's diameter as two separate obstacles. It steers toward the gap between them, and the side decision is re-taken every tick from the nearest blocker on each side | `steering.rs` `compute_unified_local_steering` |
| D-b | A step that ends in contact with a non-yielding body is accepted, then undone by the contact phase. Achieved motion is a sawtooth, not a stop | `steering.rs` `remove_blocked_velocity` (removes only the approach along the line of centres per body, so two bodies at an angle leave a residual step into the corner); `movement/collision.rs` |
| D-c | Only chasing units may walk around a blocking crowd (`flank_if_blocked` runs only when `ignored.is_some()`). Its trigger ("velocity below half walking pace") misses the case where steering produces a fast sideways step into the corner | `movement/locomotion.rs` `step_toward`, `flank_if_blocked` |
| D-d | The 10 s stall rule turns a stuck attack-move into a permanent idle. The 250-tick pursuit stall drops a target that has not moved, then the unit takes it up again 51 ticks later | `locomotion.rs` `check_progress`, `check_pursuit_progress`; tuning `STALL_ABANDON_TICKS`, `ABANDONED_TARGET_TICKS` |

**Not causes.** Measured in the probe:
- **Target flips:** units 22 and 24 keep the hall as their target for 250 ticks each time.
- **Equal push priority:** every blocker is either anchored or not idle, so priority is never consulted.
- **Hostile contact:** none near the line.
- **Separation radius:** read by no code (B-49).

M-19 (marching crowd) and M-21 (melee ring) show the same signature: reversals with cosine near −1 between two
neighbours. They are expected to share D-a and D-b. **Verify** in WP1 by running their recorded reproductions with
the new metric.

### 2.4 Room that was there

The firing arc around a hall of radius 2.5 at pulse bot range is a circle of radius 2.5 + 5.0 + 0.375 = 7.875, with
a circumference of 49.5. That holds about 66 pulse bots shoulder to shoulder. The six firing units occupy about 4.5
of it. From where units 4, 8 and 10 stand, a free firing spot lies at most about 1.5 body widths sideways. Room was
never the limit; the walk to it was.

---

## 3. StarCraft II behaviour (sources)

**Data** comes from SC2Mapster `SC2GameData` at commit `5444e12f13` ("5.0.14.94342"), the chain explained in
`docs/evidence/2026-11-04-catalog-time-audit.md`. Files are `mods/<layer>.sc2mod/base.sc2data/GameData/UnitData.xml`.

| Unit | Radius | SeparationRadius | InnerRadius | Push |
|---|---|---|---|---|
| `CUnit` default | 0.5 (core:41) | 0.5 (core:42) | — | `PushPriority` 10 (core:59); mover Ground = `CMoverAvoid` (core MoverData:7) |
| Marine | 0.375 (liberty:4837) | 0.375 (:4838) | 0.375 (:4839) | default |
| SCV | 0.375 (liberty:4743) | 0.375 | 0.375 → 0.3125 (voidmulti:1528) | default |
| Zealot | 0.5 | 0.375 (liberty:3187) | 0.375 | default |
| Roach | 0.5 | 0.5 | 0.625 (liberty:7197) | default |
| Ultralisk | 1 (liberty:8022) | 1 | 0.75 | `AlliedPushPriority` 1 (voidmulti:1943), `PushPriority` 10 |

`PushPriority` is overridden only for `Critter` (5, liberty:156) and `SlaynElemental` (11, void:3569).
`AlliedPushPriority` appears nowhere else.

**Facts** (documented by Blizzard or in game data):
1. **The Ultralisk's 5.0.14 push change is allied push priority, 0 → 1** ([5.0.14 patch
   notes](https://news.blizzard.com/en-us/article/24162754/starcraft-ii-5-0-14-patch-notes)): "Increased allied push
   priority from 0 to 1 (now pushes allied units out of the way when moving)". The notes say the earlier size
   reduction was "to minimize cases where it got stuck behind other allied units".
   - So by default a unit does not push its allies out of its way when moving.
   - Doc 07's "push priority 50" has no support.
2. **Patch 5.0.15** "Increased allied push priority for Thors and Siege Tanks … to assist bulky units in pathfinding
   when surrounded by many small friendly units" (Liquipedia, Patch 5.0.15). This is after our patch of record and is
   recorded only.
3. **Enemies do not push each other.**
   - Patch 1.2.0: "Fixed an issue where units could push opposing units when they shouldn't have been able to."
   - Patch 1.3.0 added the trigger "AlliancePushable … whether a player's units will move through another player's
     units, pushing them out of the way" (Liquipedia patch pages, read through the API).
   - This settles battle-33 investigation §1.11's open item.
4. **Hold Position units are not pushed.** Patch 1.3.0: "Fixed an issue where you could push Hold Position units by
   Move/Hold Position spamming."
5. **Concave.** Blizzard's game guide [Unit Positioning](https://news.blizzard.com/en-us/article/6640646/game-guide-unit-positioning)
   says "The concave formation is a curved position which allows all your units to attack". For units left outside
   it, the player is told to "move them slightly forward before attack-moving once again".
   - So the game does not guarantee that every unit reaches range.
   - A unit left out stands. Nothing in any source has it oscillate.
6. **Hold Position** (Liquipedia "Micro (StarCraft)"): "It will attack enemy units if they're in range, but won't
   chase after them."

**Community** (secondary; SC2Mapster threads read only as search snippets because the site redirects):
7. Units "push their way through units that are idling (on stop command), but get blocked by units that are holding
   position, attacking, patroling and other moving units" (SC2Mapster data forum thread 158471).
8. Push priority "determines the behaviour of friendly ground units when they collide". Lower **or equal** priority
   units "step aside to make way" (SC2Mapster tutorial 179724). A second snippet says equal priorities cannot push.
   The two conflict, and the conflict is unresolved.
9. Units "will give up move commands if they make no progress towards their destination over some period of time as
   a result of unit on unit collisions" (SC2Mapster thread 187903). No duration is given.
10. The GDC 2011 AI Summit talk "AI Navigation: It's Not a Solved Problem – Yet" (James Anhalt of Blizzard, with
    Kring and Sturtevant) is summarised on gamedev.net threads 624135 and 563819 as:
    - navmesh plus A* plus a radius-aware funnel;
    - boids-style following, separation, avoidance and arrival;
    - other units left out of global pathing, with a unit that cannot slide past others repathing;
    - a stationary group moving out of the way of a moving one.

    No slides or recording were found. The plan's 2026-10-30 title "Crowds in a Polygon Soup" could not be traced to
    an SC2 talk.

**Inference** (ours, marked as such in the code comments that cite it):
- I-1. A firing unit is not pushed by a friendly mover (7). Ours already anchors it (`is_anchored_in_combat`).
- I-2. The game's avoidance mover (`CMoverAvoid`) slides a unit around stationary friends consistently. A rear unit
  in a free arc reaches range by walking around (5, 10). Where there is no room, it stands (5, 9).
- I-3. "Vibrating in place" is not game behaviour under any source. It is a defect of ours, not a reproduced quirk.
- I-4. Which radius spaces units: probably SeparationRadius for steering spacing, Radius for unit contact and
  InnerRadius against structures (fact 5 of Liquipedia's "Size" note). Not settled, so it stays B-49 (§8).

**Not found.** Whether a unit in weapon cooldown, standing with a target but not swinging, counts as "attacking" for
pushing. The give-up time. How `AlliedPushPriority` combines with `PushPriority`. Any group-arrival rule.

---

## 4. File ownership and the pathfinding boundary

| Area | This plan | Pathfinding plan (`2026-11-09-sc2-pathfinding`) |
|---|---|---|
| `steering.rs`: `compute_unified_local_steering`, `remove_blocked_velocity`, `checked_dynamic_push`, `SteeringObstacle::yields_to` | owns | reads |
| `movement/collision.rs` | owns | — |
| `movement/locomotion.rs`: `step_toward`, `flank_if_blocked`, `is_anchored_in_combat`, `obstacle_of`, `check_progress`, `check_pursuit_progress` | owns | — |
| `movement/locomotion.rs`: `search_route`, `route_is_usable`, `drop_passed_waypoints`, `sidestep_occupied_corner`, `reroute_pursuit`'s search | reads | owns |
| `movement/tuning.rs` | its contact, stall and flank constants | its route constants |
| `sim_map` (pathfinding, clearance) | — | owns |

**Rules of coexistence:**
- The surround walk is local steering. It never asks the path search to route around units: the game's global
  pathing ignores units (fact 10).
- If both plans' code packages are open at once, the one merged second rebases. Commits touching `locomotion.rs` land
  one at a time, each with its own matrix comparison.
- The orchestrator orders them. The default is pathfinding's rule commits first, because a better route changes
  where crowds form, which this plan then measures.

---

## 5. Design decisions

### D-1. R1, waiting: no step into a gap the unit cannot pass, and a blocked unit stands still

1. **Gap admissibility** (fixes D-a).
   - In `compute_unified_local_steering`, two non-yielding bodies, one on each side of the path, whose surface gap is
     below the unit's diameter plus `STEERING_CORRIDOR_MARGIN`, form one wall: the unit cannot pass between them.
   - The unit then deflects toward the outer side of the wall's nearer end. That side is the end whose outer edge
     lies nearer the unit's path line; on a tie, the left.
   - It never deflects toward the gap.
   - Bodies that yield (`yields_to`) are not walls.
2. **Contact as a constraint for all bodies at once** (fixes D-b).
   - `remove_blocked_velocity` repeats its per-body projection until no non-yielding body is approached, at most as
     many passes as there are such bodies within reach. The order is fixed: by entity ID.
   - Two bodies at an angle then remove both components, and the step into a corner becomes zero.
   - **PERFORMANCE RATIONALE** is needed only if the neighbour list grows past the current one. It does not: the same
     `neighbours` slice is reused.
3. **Waiting.**
   - A unit whose admissible step is below `WAIT_THRESHOLD` (one eighth of its speed: an exact rational in
     `tuning.rs`, named, not a sourced value; U-23) neither moves nor turns.
   - It keeps its order and plays `Idle`.
   - It retries every tick. Waiting is not stalling for R4's purpose, but it does count toward the move's give-up
     timer (D-3).
4. **Invariant, tested.** A unit's achieved velocity never reverses (cosine < −0.3) on two consecutive ticks while
   its order, target and goal are unchanged and no enemy touches it.

### D-2. R2, the surround walk: blocked attackers walk around the crowd on a kept side

Fixes D-c.

1. **Who.**
   - A unit whose front order is `AttackTarget`, or an acquired attack, while out of range.
   - A unit whose front order is `AttackMove` with no target.
   - In both cases the unit's archetype has `surround_walk: true`.
   - Plain `Move` keeps R1 only: a moved unit that cannot pass stands, as fact 9 says, and the move-arrival rules
     stay as they are.
2. **When.** The direct step toward its goal is blocked by R1, so the admissible forward component is below half its
   speed, and at least one blocker is a stationary friend that does not yield.
3. **Where.**
   - The unit steps along the crowd: perpendicular to the direction to its goal, on its kept side, at full speed,
     through `remove_blocked_velocity`.
   - For a chase, the goal is the target's centre, so the walk follows the arc around the target and a concave forms.
4. **Which side, kept** (hysteresis).
   - The side is chosen once, when the blockage starts: the side of the wall end nearer the unit, as in D-1.1; on a
     tie, by entity ID parity.
   - It is stored in `MovementMemory::flank_left`, which already exists, plus a new `flank_active: bool`.
   - It flips only when the walk on that side is itself blocked by a non-yielding body or by terrain for
     `FLANK_REVERSE_TICKS` (a named constant, 12 ticks, the repath cooldown; U-23).
   - It is cleared when the forward step is free again, or when the order or target changes.
5. **The walk ends** when the forward step is free (the unit turns in and closes), or the unit is in range (it
   stops; `aim_if_in_range`), or an attack-move acquires a target. The last case is what reaches units 4, 8 and 10:
   their walk brings the hall inside scan 5.5 within one body width.
6. **Hold Position units** never walk: they do not move by definition.
7. **Data.** `surround_walk: bool` is a new archetype field, defaulted to `true` through `#[serde(default)]`. The
   default is not the game's engine default, as deviation plan D-6 would require. It rests on the owner question in
   the header, and the field's doc comment cites the question and Q0051's "allows more". If the owner answers "no",
   the default becomes `false`, and the field stays so an archetype can opt in.

### D-3. R4, the stall rules count the walk

Fixes D-d.
- A unit on a surround walk makes progress by the walk's length, so `note_progress` does not count those ticks as
  stalled. A full circle around a stationary target has a bounded length (2π × the firing radius), so the walk cannot
  run forever: once it has walked one full circumference without a free forward step, the walk ends and R1's waiting
  applies.
- The 10 s give-up of `check_progress` (fact 9) and the 250-tick pursuit stall stay as they are, for genuinely stuck
  units. Their constants remain unsourced (U-23, B-13).
- A waiting unit (D-1.3) still counts as stalled, so a move into a blocked spot is given up as in the game (fact 9).

### D-4. R3, the push table, stated once and pinned by tests

Contact between two friendly ground bodies `a` (moving) and `b`:

| `b` is | `b` pushed? | Source |
|---|---|---|
| holding position, sieged, immobile, unpushable | never | fact 4 |
| firing: rotating to target, wind-up, backswing, or standing with a live target under an attack order | never | I-1 (community 7) |
| idle (no order, or `Idle`, or `Stop` done), standing | yes, sideways out of `a`'s lane, if `push(a) ≥ push(b)` | community 7, 8 (≥ per the tutorial; **verify** equal) |
| moving, or waiting under an order (D-1.3) | no; symmetric separation along the line of centres, relaxed | community 7 |

- Enemies separate evenly and never push (fact 3).
- This is today's `checked_dynamic_push` plus `is_anchored_in_combat`, with one change: a waiting unit (D-1.3) has
  zero velocity but an order. It must not count as idle, even though `yields_to` reads only velocity. The change
  makes `yields_to` read "idle" from the order (`obstacle_of` already knows it), not from the velocity alone.
- Without the change, R1 would let movers push waiting units forward through the firing line.
- In the probe, every pushed or pushing pair is already covered by the table, so WP4 is expected to be bit-identical
  on the matrix and the goldens.

### D-5. R5, push priorities from the game (B-48), and R6, burrowed bodies (B-93)

**R5.**
- `push_priority` becomes the game's: 10 for every shipped archetype.
- The Ultralisk analogue (`stag_behemoth`) gets 11, the smallest value that makes "pushes allied units out of the
  way" true under D-4. This is the question's default until the owner answers how `AlliedPushPriority` maps.
- A data commit, measured. It moves only pushes between idle and moving friends of different priorities.

**R6.**
- A burrowed body collides only with burrowed bodies (`liberty` UnitData:775: the burrowed Lurker collides with
  `Burrow` only).
- The rule reads the existing burrowed state, not an identity.
- Rule commit; latent where no matrix battle burrows; expected bit-identical on the matrix outside the Lurker
  battles. **Verify** which battles burrow.

### D-6. Determinism, checksums, catalog

- New `MovementMemory` field `flank_active`. Movement memory is hashed (`checksum/encoding.rs` encodes `flank_left` and
  `stalled_ticks`), so adding it bumps `CHECKSUM_SCHEMA_VERSION` in its own latent commit (deviation plan D-4).
- New archetype field `surround_walk`, in the catalog and its validation.
- New tuning constants `WAIT_THRESHOLD` and `FLANK_REVERSE_TICKS`, exact rationals.
- No floats and no collections beyond the existing retained buffers; nothing in the per-tick path allocates (charter
  §3.3).
- No identity in Rust: tests use scratch archetypes in RON strings (C5).

### D-7. Measurement and the stop rule

**After every rule or data commit** (deviation plan D-3):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name>
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --all-battles --seeds 16 --out ~/scratch/blocking/<step>/matrix \
    --compare ~/scratch/blocking/<prev>/matrix/matrix.csv
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/blocking/<step>/b33
```

**Goldens.** Run in debug, release and `wasm32-wasip1`.

**Stop rule.** A battle whose overall winner over its seeds changes stops the package. Report it before re-recording.
Single-seed flips are listed. Battle 33 is re-recorded to the smallest ranges that hold (Q0044 option 1).

**Evidence.** The new file `docs/evidence/<date WP1 starts>-friendly-blocking.md`, one section per commit, with:
- the matrix delta;
- battle 33 on 68 seeds;
- the first differing golden checkpoint;
- the **blocking metrics** of WP1 on the matrix and on the showcase reproduction.

**Gate.** Implementation agents hand back after `cargo gate quick`, and a cheap verifier runs `cargo gate package`
(Q0061).

---

## 6. Work packages (commit order)

### WP0: Questions, IDs (docs)

- File the two questions in the header table, with the next free numbers (0071 onward at the time of writing).
- Add the new register rows to `DESIGN.md` §4, with the next free IDs at merge time (B-94 to B-97 are proposed by
  plan `2026-11-09-client-mode-commands`):
  - **B-a:** a second-line unit oscillates between two friends that will not yield (D-a, D-b);
  - **B-b:** attack-moving units without a target have no way around a firing line, and give up after 10 s (D-c,
    D-d);
  - **B-c:** a chasing unit's flanking walk misses a fast sideways step into a corner (D-c).
- Gate: docs tier.

### WP1: Measurement tooling and reproductions (test-support only, bit-identical)

1. **Blocking metrics** in `sim_fixtures::trace` (and the matrix CSV), per battle and seed:
   - `blocked_attacker_ticks`: ticks a unit holds a target and is out of range while a friendly non-yielding body
     lies within its radius plus 0.1 toward the target;
   - `reversals`: the D-1.4 definition, ticks where neither unit nor target was touched by an enemy;
   - `stall_giveups`: the `NoProgress` and `PursuitAbandoned` outcomes.

   Then record the baseline matrix: all battles, 16 seeds, plus battle 33 on 68 seeds.
2. **Showcase reproduction test** `second_line_tests.rs::showcase_second_line_vibrates` (new file in
   `crates/sim_core/tests/`, registered in `crates/gate/suites.ron` for C22):
   - it builds the world as §2.1 does;
   - it asserts the current numbers as the defect's signature;
   - after WP2 and WP3, the assertions are inverted to the acceptance numbers of §7. The first commit records them
     as the "before" in the evidence, not as a passing expectation of wrong behaviour; the test lands red-first in
     WP2 per D-2 of the deviation plan.
3. **Synthetic reproduction** `second_line_tests.rs::ranged_block_reaches_the_arc`, with scratch RON archetypes:
   - a 64 × 64 flat map;
   - an immobile enemy "pillar" (radius 2.5, life high enough not to die);
   - 10 ranged units (radius 0.375, speed 0.126, range 5.0, scan 5.5, period 15, push priority 10) in two rows of 5
     at 12 world units from the pillar's edge, attack-moving to a point beyond it.

   **Today, expected (verify in WP1):** about 6 units reach range and the rest oscillate. **Acceptance:** all 10 in
   range by tick 300, zero D-1.4 reversals, zero stall give-ups.
4. **No-room reproduction** `second_line_tests.rs::no_room_means_waiting_still`:
   - the same pillar inside a corridor two bodies wide (terrain), with 6 units in single file;
   - **acceptance:** the units that cannot reach range keep zero velocity on at least 95 % of ticks after their first
     stop, and show zero reversals.
5. **M-19 and M-21 under the metric.** Run the M-19 window (evidence `2026-11-04-combat-sc2-alignment` §6.4) and
   `test_converging_units_respect_speed_cap_and_do_not_vibrate` with `reversals` counted. Confirm or refute that
   they share D-a and D-b.

Commit: test-support only. The new reproduction tests are `#[ignore]`d with a reason that names WP2 and WP3, or
written against the metrics so they pass as a baseline record (agent's choice, stated in the hand-back).

### WP2: R1, waiting (rule)

- **Tests first:**
  - unit tests in `steering.rs` for gap admissibility (two bodies 0.70 apart block a 0.375-radius unit; 0.80 apart
    do not) and for multi-body projection (a step into a 90° corner becomes zero);
  - `no_room_means_waiting_still` goes green;
  - the D-1.4 invariant as a property test in `push_priority_tests.rs` or `movement_contact_tests.rs`.
- Code: D-1.
- Restate first, test-only, any test that breaks for another reason (deviation plan D-2).
- Measure (D-7). R1 alone should already end the vibration, but not the blocking.
- Expected: the matrix moves. Goldens re-recorded in this commit if they move.

### WP3: R2 and R4, the surround walk (rule, then data flag)

- **Tests first:**
  - `ranged_block_reaches_the_arc` green;
  - a kept-side test: a unit that starts its walk left keeps left while the left is open, even when the nearest
    blocker changes side;
  - a Hold Position unit in the line is never displaced, and a held unit never walks (`push_priority_tests.rs`);
  - an attack-move unit acquires during its walk;
  - a full circle ends the walk (D-3).
- Code: D-2 and D-3; `flank_active` (its checksum schema bump goes first, in its own latent commit); the `surround_walk` field, defaulted `true`.
- The showcase reproduction's acceptance: at tick 900, all 10 attackers of each side in range, no reversals, no
  give-ups.
- Measure (D-7). This is the commit most likely to move winners: ranged armies get more shooters on static or slow
  targets. Apply the stop rule.

### WP4: R3, the push table (rule, expected bit-identical)

- Tests in `push_priority_tests.rs`, one per row of D-4's table, plus "enemies never push" and "a waiting unit is not
  pushed".
- Code: `yields_to` reads idleness from the order (D-4).
- Prove bit-identical on the matrix and goldens. A move is reported, not re-recorded silently.
- WP4 may land before WP2 if the orchestrator prefers a latent commit first. Its rule is independent.

### WP5: R5, push priorities (data, B-48)

- One data commit: `push_priority: 10` everywhere and 11 for the Ultralisk analogue, with each value's source in a
  comment.
- `catalog_reference_values_tests.rs` gains the values.
- Measure, with the stop rule.

### WP6: R6, burrowed bodies (rule, B-93)

- Tests in `burrow_attack_tests.rs` or `collision_and_immobility_tests.rs`: a burrowed unit is passed through by a
  ground unit, still collides with another burrowed unit, and never takes or gives a push from or to an unburrowed
  one.
- Code: the contact phase and `gather_neighbours` skip pairs whose burrowed states differ.
- Measure.

### WP7: Bookkeeping (docs)

- Evidence file complete.
- `DESIGN.md` Movement row and register: B-a to B-c, B-48 and B-93 closed by their commits.
- Remediation M-19 and M-21 closed or re-scoped by the WP1 finding.
- Deviation plan: an amendment saying WP16's B-48, B-93, M-19 and M-21 are carried by this plan, and that B-5, B-13,
  B-49, B-50, B-92, U-5 to U-7 and U-23 remain there.
- This plan's README row.
- Close the questions answered.

**Size.**
- WP1 is about a day.
- WP2 and WP3 are a day each, plus measurement.
- WP4 to WP6 are half a day each.

---

## 7. Acceptance (whole plan)

| Check | Before (probe) | After |
|---|---|---|
| Showcase: attackers in range at tick 900, per side | 6 of 10 | 10 of 10 |
| Showcase: D-1.4 reversals, ticks 600 to 1,300 | 4, 8, 10: 213 to 249; 22, 24: 393, 423 | 0 |
| Showcase: attack-moves given up (`NoProgress`) | 6 | 0 |
| Synthetic arc: in range by tick 300 | (WP1) | 10 of 10 |
| No room: still on ≥ 95 % of ticks after the first stop | (WP1) | yes, 0 reversals |
| Hold Position unit displaced by a friend | 0 | 0 |
| Matrix: winner flips | — | none unreported (stop rule) |

The showcase video changes when WP3 lands. Its storyboard's `clash` timing (`UntilCombat`, base falls near tick
1,225) may move. The CI showcase job renders whatever the scenario does, so the storyboard comment's numbers are
updated in WP7, not tuned.

---

## 8. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| The surround walk lets ranged armies fight much better than today in many battles | many matrix moves, possible winner flips | stop rule; R1 lands first and alone, so the shift R2 causes is measured by itself; `surround_walk` can be set to `false` per archetype if the owner prefers the game's "stand" |
| Gap admissibility changes marching through gaps between idle or moving friends | columns route differently | only non-yielding bodies form walls; idle friends still yield; M-19 measured |
| Multi-body projection stops units that used to slide through crowds | more waiting, slower crowds | D-1.4 property test; matrix end ticks compared |
| Overlap with the pathfinding plan in `locomotion.rs` | merge conflicts, mixed causes | §4 boundary; one merged at a time, each measured |
| Waiting counts as stalled, so a unit waiting for a gap gives up after 10 s | units idle where the game would also give up (fact 9) | matches the game; the duration stays U-23 |
| `AlliedPushPriority` mapping guessed | Ultralisk analogue pushes too much or too little | owner question; one value, one commit |

---

## 9. Out of scope

Nothing here is accepted as a deviation by being left out.

- **B-49** (which radius spaces units; inference I-4 only).
- **B-50** (melee packing; Q0035 deferred by the owner).
- **B-92** (lateral acceleration).
- **B-5** (damage point holding an automatic attacker).
- **B-13** (help, flee and leash radii).
- **U-5 to U-7.**
- **U-23** (steering tuning). This plan adds two named constants to it and replaces none.

These stay with the deviation plan's WP16. The path search, corridor and clearance stay with plan
`2026-11-09-sc2-pathfinding`. Patch 5.0.15's Thor and Siege Tank allied push priority is after our patch of record
and is recorded only.

---

## Amendment 1 (2026-11-09): showcase ramp E turning moves here from the pathfinding plan

Orchestrator decision recorded in plan [`2026-11-09-sc2-pathfinding`](2026-11-09-sc2-pathfinding.md), Amendment 1:
this plan's acceptance (§7) also requires that player 0's mean total turn at the showcase's ramp E foot be at most
115°, measured with `route_trace` (`crates/sim_core/examples/route_trace.rs`, default boxes) as in
[`docs/evidence/2026-11-09-sc2-pathfinding.md`](../evidence/2026-11-09-sc2-pathfinding.md). After that plan's
WP2+WP3 it is 213°, from flockmates jostling in a tighter column: local steering, which this plan owns.

## Amendment 2 (2026-11-09): register IDs and questions

WP0 filed the header's two questions as [Q0073](../questions/open/0073-allied-push-priority.md) (allied push
priority) and [Q0074](../questions/open/0074-surround-walk.md) (the surround walk). The orchestrator's numbering
replaces §6 WP0's placeholders in the bug register: **B-103** for B-a (oscillation between two friends that will not
yield), **B-104** for B-b (attack-moving units without a target have no way round a firing line) and **B-105** for
B-c (the chasing unit's flanking walk misses a fast sideways step into a corner).

## Amendment 3 (2026-11-09): WP4 first; WP2 and WP3 held

Implementation decisions, recorded under Q0002; figures in
[`docs/evidence/2026-11-09-friendly-blocking.md`](../evidence/2026-11-09-friendly-blocking.md).

- **WP4 landed before WP2**, as WP4 allows: R1 leaves units waiting under their orders, and without D-4 such a unit
  counts as idle and is pushed through the line. WP4 is **not bit-identical**, against §6's expectation: a squad
  standing still on its first tick after an order is already "waiting" today. 10 matrix battles moved, no overall
  winner changed; goldens re-recorded with the traced cause.
- **WP2 and WP3 are held** on branch `friendly-blocking-held` (stop rule, D-7): WP2 alone flips the showcase battle
  (player 1 wins at tick 1,254), and WP3 on top flips four matrix battles (9a, 28 and 31, and 32 to a tie) while the
  showcase stays won by player 1 (tick 1,171). Neither met §7: after WP3, 8 and 9 of 10 attackers are in range at
  tick 900, with 40 and 51 reversals. The held branch's evidence records how D-1.1 had to change (walls of standing
  bodies only, ends rounded only when known) and why WP3 needed more memory than `flank_active`.
- **Not started:** WP5 (push priorities), WP6 (burrowed bodies), WP7 (bookkeeping), and Amendment 1's ramp E turn
  (206° on the base, 202° after WP4).

## Amendment 4 (2026-11-09): WP2 and WP3 released on the integration base; the surround walk's artefacts fixed

Orchestrator decision: the held direction is accepted in principle, each winner flip must be justified, and the
acceptance of §7 pursued. Implementation decisions, recorded under Q0002; figures in
[`docs/evidence/2026-11-09-friendly-blocking.md`](../evidence/2026-11-09-friendly-blocking.md), section "Round 2".

- **Base.** The work was carried onto the integration tree `b7ac748` (checksum schema 24); the latent WP3 schema
  bump is therefore **24 → 25**. Every figure was measured again on that base.
- **Flips of WP3 as held, traced.** On this base WP3 flipped battles 28 (100 zerglings now lost to 10 immortals) and
  31, and kept WP2's showcase flip (player 1 won). Battle 9a no longer flips: its expectation already says the
  zerglings win. Both matrix flips were artefacts of the walk, not StarCraft II consequences:
  - **28:** zerglings stood for good behind squadmates that waited under their orders. Contact nudges such a
    squadmate by a few thousandths a tick, so it was neither "exactly still" for the walk's start nor for walls;
    walks that did start chose their side afresh on every restart (a third of 2,419 restarts swapped side in one
    seed), and a spent walk never looked again. StarCraft II zerglings flow round such a crowd: Liquipedia's
    Immortal page lists Zerglings among its counters, and the battle's claim is "Without splash, immortals are
    overrun".
  - **31:** an attack-move without a target walked up to one circle round its far attack-move point (about 100 world
    units), so the second and third ranks stretched the line 15 units to one side and the armies passed each other.
- **Changes to D-1, D-2 and D-3** (each measured alone; together they are the fix commit):
  1. A body "stands" for walls (D-1.1) and for the walk's start (D-2.2) when it moved by at most
     `STANDING_DRIFT` (1/64 world unit) last tick, not only when exactly still.
  2. The walk's side is chosen once per target, when the unit first walks for it, and kept across pauses (D-2.4).
  3. A walk in progress ends only when the straight way in is free; while a walk pauses, the unit steps straight in,
     not along steering's step, which led back across the walk (a reversal every tick).
  4. A walk without a target is bounded by the unit's scan range instead of a circle round its attack-move point
     (D-3).
  5. A spent walk waits `FLANK_RETRY_TICKS` (24) and then walks afresh: the crowd round a target changes as units
     die. Without it, a corridor unit gave up its order (3 give-ups in `no_room_means_waiting_still`).
- **Result.** No overall winner of the matrix or the showcase differs from the base. Battle 10's upper bound was
  re-recorded with a cited reason (zerglings now surround both Hellions, and in StarCraft II all eight often live).
  No archetype sets `surround_walk: false`.
- **Acceptance (§7), not all reached; tuning stopped here.**
  - Showcase: 10 of 10 attackers of each side in range at tick 900 (met); 0 give-ups (met); 7 and 4 reversals
    (not 0).
  - Synthetic arc: all 10 in range by tick 98 (met), 3 reversals.
  - No room: 5 reversals; the rear units stand on 73 to 85 % of their ticks after their first stop, not 95 %: a
    spent walk retries (change 5), and dropping the retry left them at 84 % with 3 give-ups.
  - Amendment 1's ramp E turn stays at 211° (115° asked): 206° on the base, 202° after WP4, 211° after WP2 and at
    every step since. No change to the blocked-attacker rules moved it, so it comes from the marching column's
    steering (R1's walls among flockmates), not from the walk; it is not tuned here and stays open under
    Amendment 1.
  The three reproductions stay ignored with these figures. The remaining reversals are single steps at the edge of a
  crowd; zeroing them would need a further rule beyond StarCraft II's data, so tuning stops (Q0051).
- **Not started:** WP5 (push priorities), WP6 (burrowed bodies), WP7 (bookkeeping); the showcase storyboard
  (`assets/captures/showcase.ron`) was not changed: its captions are generic and its winner (player 0) is the base's,
  but the battle now ends at tick 1,098 in `route_trace` (1,355 on the base), so the header's tick figures need a
  render to confirm.

## Amendment 5 (2026-11-09): the builder regression of WP2

`player_view_tests::a_build_on_a_hidden_structure_is_accepted_and_abandoned_on_arrival` failed from WP2 on: the
builder started wedged between its own town hall and another worker, waited (D-1.3) and spent its task legs
(`Unreachable`). The orchestrator asked first for the StarCraft II rule that gathering workers do not collide with
other units ("mineral walk"). Our harvest code already models that: a worker under a `Harvest` or `ReturnCargo` order
sets `is_mineral_walking` and is left out of every other unit's obstacles (`prepare_movement_snapshot`), so it could
not be the blocker. The blocker was an **idle** worker that contact had nudged by a few thousandths a tick: D-4's
`yields_to` required an idle friend to stand *exactly* still, so the nudged one did not step aside and, standing
within Amendment 4's drift, formed a wall with the hall. `yields_to` now uses Amendment 4's `STANDING_DRIFT` (1/64 a tick) for "standing",
the same test as walls and the walk use, which completes D-4's row "idle, standing: pushed sideways".

Measured against Amendment 4: 1 matrix battle moved (35), no overall winner changed (none against the base either),
battle 33 identical on 68 seeds, showcase 10 / 10 in range at tick 900, 11 / 8 reversals, 0 give-ups, player 0 wins
at tick 1,100. Goldens re-recorded (`golden_replay` from checkpoint 11, `golden_replay_orders` from checkpoint 19; the
only change).
