# Blueprint: The Follow Order (B-163), Rallies on Friendly Units (B-166's Remaining Half) and Scan Move on a Unit (B-187)

StarCraft II 5.0.14 has a command this project lacks: a `Move` (or Scan Move, `AcquireMove`) whose target is a
unit. The unit walks to the target and then stays with it while it moves: it **follows**. Three open rows need it:

- **B-163.** A rally on a friendly unit sends each new unit to where the target stood when the unit left. In
  5.0.14 the new unit follows its target.
- **B-166, the remaining half.** Plan [`2026-11-25-rallies`](2026-11-25-rallies.md) fixed rallies on enemy units.
  A new unit that cannot attack the enemy still walks to a snapshot of its position (its D-5 table and §14).
- **B-187.** A Scan Move aimed at a unit walks to the point under it. In 5.0.14, `AcquireMove` on a unit follows
  it. B-187 also names an attack by a unit that cannot attack the clicked unit. The owner answered that case in
  Q0105 (attack-move to where the target stands), so this plan keeps it and drafts a question (§16).

This plan adds one order, `UnitOrder::Follow { target, kind }`, with these parts:

- an admission rule;
- a pursuit goal (the target's tick-start position);
- a rest rule (stand once touching the target, or touching a follower already resting on it);
- a resume rule (walk again once the target has moved more than 5.0.14's `FollowRangeSlop`, 1, authored in data);
- end rules (the target dies, is carried or housed, or is lost to fog; or an order is queued behind a follow that
  has reached its target).

The client sends it for a right click on one's own mobile unit and for Scan Move members of an A-click on a unit.
Rallies on units give it too. The contact solver (`movement/solver*`) is not touched. A resting follower only
reports itself to it as an idle body (D-7).

A full plan is required (charter §6). This is a new mechanic. It changes the checksum format (a new order tag and a
movement-memory field), and it spans `sim_types`, `sim_core`, the catalog and the client.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-25 against `main` at
`8574cf4d` (checksum schema 49).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- B-221 to B-223 and Q0119 are provisional numbers. WP0 replaces them with the next free numbers at its commit and
  renumbers every mention here in the same commit.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations. S-1 covers RON data; S-4 covers a move to an unreachable point walking to the nearest reachable one | Closed | D-3, D-4 |
| Q0105 (closed, answered (a)): a unit A-clicked onto a target it cannot attack attack-moves to where the target stands | Followed, unchanged | D-10; draft Q0119 asks whether it should follow instead |
| Q0118 (answered (a), with the owner's blank-sheet movement note) | Respected: no solver change; follow only produces goals and an idle flag | D-7, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0002: scoping is the agent's call if written down | Standing rule | §14 |
| The owner's guidance (2026-11-20): no fitting to fixed seed sets; survey first; simpler deviations allowed | Followed | §2.3, §6 |
| **Draft Q0119 (provisional number, not filed):** should units that cannot attack an A-clicked unit follow it, as Scan Move does? | Draft, §16 | D-10 |

---

## 1. Summary

| Topic | Today (`8574cf4d`) | After this plan |
|---|---|---|
| Order | No unit-target move | `UnitOrder::Follow { target: EntityId, kind: FollowKind }`, where `kind` is `Move` or `AcquireMove` (D-1) |
| Admission | — | `issue_follow_prevalidated`: the recipients are checked as for a move; the target must be a live, uncarried, unhoused, mobile unit of any owner; the target itself and immobile members are skipped; `AcquireMove` goes only to members with an A command (D-2) |
| Goal | — | The target's tick-start position, clamped into the follower's domain (D-3) |
| Rest | — | The follower stands, keeping its order, when it touches the target, touches a follower resting on it, reaches the nearest reachable point, or stalls (D-4) |
| Resume | — | When the target has moved more than `follow_range_slop` (1, 5.0.14 `FollowRangeSlop`, `movement_rules.ron`) from where it stood when the follower came to rest (D-4) |
| End | — | The target dies, is carried or housed; or the follower reaches it with an order queued behind; an enemy target lost to fog is walked to where it was last seen, and the order ends there (D-5) |
| Acquisition | — | `AcquireMove` acquires like an attack-move and resumes the follow after the fight; `Move` acquires nothing, like a move (D-6) |
| Contact | — | A resting follower counts as idle for pushes, so friends, its target included, push it aside. No solver code changes (D-7) |
| Groups | Route groups for point moves | None for follows: each follower routes alone and the group bunches round the target (D-8) |
| Rally on a unit | Snapshot `Move` (friend, or an enemy the new unit cannot attack) | `Follow { Move }`; structures and carriers unchanged (D-9) |
| Client | A right click on one's own mobile unit moves to the ground; an A-click with Scan Move members walks them to the point | `Follow { Move }` (Shift queues); Scan Move members get `Follow { AcquireMove }` (D-10) |
| Hashed state | — | The order's tag 19 and `MovementMemory::follow_rest`. Schema **next free at merge** (50 on `8574cf4d`) (§5) |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`), read as data from `~/scratch/combat_wp3_data/<layer>/`. "core
  Abil:N" means line N of `core/AbilData.xml`. Nothing there was executed.
- **[BLIZ]** Blizzard's own pages (patch notes, game guides).
- **[COMM]** Community sources (forums, Liquipedia).
- **[INF]** An inference of this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | core Abil:533–561 (`CAbilMove` default) | The move ability has five commands, `Move`, `Patrol`, `HoldPos`, `AcquireMove` and `Turn`, each `ToSelection`. `OrderArray` 0 is the move's green waypoint (`255,0,255,0`, `WayPointMove`). An unnamed second entry is blue (`255,35,125,255`, `WayPointConfirm`) [DATA] |
| S2 | core Abil:1371–1377 (`CAbilMove id="move"`, the move of every multiplayer unit) | `FleeRange 5`, `FleeTime 5`, **`FollowRangeSlop 1`**, **`FollowAcquireRange 6`**, `MinPatrolDistance 1`. No layer overrides them: balancemulti Abil:1524–1526 adds only `AbilSetId Move`. A grep of all eight layers finds no other `Follow*` field on `CAbilMove`, and in particular no `MinimumFollowRange` [DATA] |
| S3 | balancemulti Abil:70–73, voidmulti Abil:70–73 (`BattlecruiserMove`) | A separate move ability for the Battlecruiser with no `Follow*` fields, so it has the class default, which is not in the data (**verify**) [DATA] |
| S4 | core Abil:351, void Abil:444 | `FollowRange` exists only on `CAbilEffectTarget` (5 by default, 7 on `LockOn`): a caster's approach to its spell target, not the move ability's follow [DATA] |
| S5 | Blizzard, [5.0.11 PTR patch notes](https://news.blizzard.com/en-us/article/23891308/starcraft-ii-5-0-11-ptr-patch-notes), "Misc Bug fixes / QOL Changes, All Units" | "Follow acquire range reduced from 5 to 2 (Units will switch to attacking from moving when within this range of the follow target)." [BLIZ] |
| S6 | Blizzard, [5.0.11 Hotfix patch notes](https://news.blizzard.com/en-us/article/23897177/starcraft-ii-5-0-11-hotfix-patch-notes), "Bug fixes" | "Fixed an issue where units following enemy units may only attack them within range 2 (e.g. Adept after Shading) instead of 6." With S2's 6, the 5.0.14 value is 6, and it governs a follower's switch to attacking the **enemy it follows** [BLIZ] |
| S7 | Blizzard SC2 forum, ["The Follow Command and Healer Units"](https://us.forums.blizzard.com/en/sc2/t/the-follow-command-and-healer-units/26896), June 2023 | For healers: "press A(Attack), and click on any unit you want it to follow"; "The attack command will stick unlike the move command"; it applies to healers with an auto-cast heal "AND no weapon". The poster also says a move-follow "will still 'return fire', therefore triggering the stop command"; another says that if "the followed unit moved a little bit away, they would stop everything and start to follow again". Co-op era, same engine [COMM] |
| S8 | Blizzard SC2 forum, ["MedVac Behavior"](https://us.forums.blizzard.com/en/sc2/t/medvac-behavior/4213), October 2019 | A Medivac given a Move on a marine "will follow them until they die" [COMM] |
| S9 | Liquipedia, *Micro (StarCraft)*, seen only through web-search summaries on 2026-11-25. Direct reads returned HTTP 429 and then a rate-limit page, so it was not retried | The unit "will move to and then closely Follow the designated friendly unit or move to the friendly building". "Should the target unit die, the following unit will Stop." The follower does not copy its target's Hold Position or other orders. A summary also claimed it "switches to Attack-move" near the target. That reads like S5's follow-acquire rule, unconfirmed [COMM, **verify** the wording] |
| S10 | Blizzard, [Game Guide: Buildings](https://news.blizzard.com/en-us/article/4488317/game-guide-buildings) (rallies plan S9) | A rally on a unit makes new units "group up around the designated unit"; a rally on a unit that died behaves "as if there'd never been a rally point" [BLIZ] |
| S11 | TeamLiquid, ["Shift-click Rally to Units tip"](https://tl.net/forum/sc2-strategy/206981-shift-click-rally-to-units-tip?view=all), 2011 (rallies plan S10) | Rally waypoints on several units: "If e.g. first immortal in 'rally queue' dies, units will be rallied straight from production building(s) to 2nd -> 3rd -> 4th." So a new unit passes from one unit waypoint to the next: **a follow with an order queued behind it ends once the follower reaches its target** [COMM, Wings of Liberty] |
| S12 | Blizzard, [Game Guide: Special Control](https://news.blizzard.com/en-us/article/4552955/game-guide-special-control) | "Most commands (such as left-clicking, right-clicking, Stop, Attack, Hold Position, Move, Patrol, and Load/Unload) can be queued", and the queue is limited [BLIZ] |
| S13 | Rallies plan S11 (Liquipedia through a summary) | "Friendly or enemy units/buildings are also valid targets for Rally, in which case a Follow or Attack command respectively is issued" [COMM, **verify**] |
| S14 | Attack-orders plan S5, S6 | Every unarmed mobile unit's A slot is Scan Move (`move,AcquireMove`). Its §2.2 rule 4 says that, aimed at a unit, it follows, marked **verify**. S7 is the first direct evidence: an A-click on a friendly unit by a weaponless healer follows it and sticks [DATA, COMM] |

### 2.2 The rules that follow

1. **A move on a unit follows it.** The follower walks to the target, then stays with it as it moves (S8, S9, S10).
   It keeps following while the target stands: the order does not end at arrival (S8: "until they die").
2. **Stop distance.** The follower closes until it touches the target, its collision radius against the target's
   (S9 "closely", S10 "group up around") [INF, **verify** the exact gap]. Several followers bunch round the target:
   those behind stop against those in front, as arrivals at a point already do here.
3. **Resume distance.** `FollowRangeSlop 1` (S2) is the target's drift that a resting follower tolerates before it
   walks again [INF from the field's name, **verify**]. S7's "moved a little bit away … start to follow again" is
   consistent with it. It prevents a step on every tick that the target shuffles.
4. **End.**
   - The target dies: the follower stops (S9, S8).
   - The target boards a transport or goes into a structure: the follower stops [INF, **verify**; the target leaves
     the map, as with a dead rally waypoint, S10].
   - An enemy target becomes unseen: the follower walks to where it was last seen and stops. This is the B-195 rule
     for an attack order [INF, **verify**].
   - A new order without Shift replaces the follow (S12).
   - With an order queued behind it, the follow ends once its target is reached, and the queue goes on (S11).
5. **Shift.** A follow can be queued behind other orders and runs when it reaches the front (S12).
6. **`AcquireMove` on a unit follows it and acquires on the way** (S1, S7, S14). It is to a follow what Scan Move on
   a point is to a move. After a fight the follow resumes ("will stick", S7).
7. **Follow versus attack on an enemy.** The smart command on an enemy attacks it. A follow of an enemy arises only
   from an explicit move command (`M` and a click) or from units that cannot attack it: a rally, Scan Move, or a
   right click by units without a weapon for it (**verify** the last). A follower that **can** attack the enemy it
   follows switches to attacking it within `FollowAcquireRange` 6 (S5, S6). Here no gesture issues that case: the
   client has no `M` key. Every follow of an enemy this plan creates is by a unit that cannot attack that enemy,
   so the switch never applies. It is registered as B-221 (§14).
8. **Formation.** A command on a unit has no group formation: each unit makes its own way to the target and they
   bunch round it (S10) [INF, **verify**]. That is 5.0.14's per-unit routing, which group routing (S-7) keeps only
   for point moves.
9. **Allies.** 5.0.14 follows an ally's unit like one's own. This project has no teams (DESIGN §4, Combat), so
   "allied" means one's own.

### 2.3 Survey: how a moving goal is followed elsewhere

The owner asked for a survey before movement rules (2026-11-20). A follow is a moving-goal problem, and the
standard answers are:

- **Re-target per tick.** The unit replans to the target's current position, and the route is reused while the
  target drifts within a tolerance. That is the "seek" or "pursuit" behaviour of steering literature (Reynolds,
  *Steering Behaviors For Autonomous Characters*, GDC 1999), and the repath cooldown of every RTS.
- **Arrive with a dead band.** The unit stops within a radius and restarts only past a larger one: hysteresis,
  which `FollowRangeSlop` names.
- **Leader-following with offsets** (Reynolds 1999, "leader following"; formation slots in many RTS titles). The
  formation is kept by giving each follower an offset from the leader.
- **Flow fields to a moving goal** rebuild a field per goal move. That is too costly for one target per follower
  here.

This project's attack pursuit is already the first answer: `chase_target` routes to the target's tick-start
position and repaths after `PURSUIT_ROUTE_DRIFT`. 5.0.14's data names the second (S2). The third is the formation
5.0.14 does not keep (§2.2 rule 8). So the design reuses pursuit with a dead band. It adds no new local-movement
algorithm, and it is not tuned against any seed set (§6).

### 2.4 Values

| Value | 5.0.14 | Here | Source |
|---|---|---|---|
| Resume drift | `FollowRangeSlop 1` | `movement_rules.ron` `pursuit.follow_range_slop: 1` | S2 |
| Rest gap | touch (**verify**) | centre distance ≤ radius sum + `ARRIVAL_TOUCH_MARGIN` (0.15, the existing arrival margin) | §2.2 rule 2 |
| Follow-acquire range on a followed enemy | `FollowAcquireRange 6` | not modelled: no follower here can attack the enemy it follows (B-221) | S2, S5, S6 |
| Battlecruiser's slop | class default (not in data) | 1, the world rule | S3; B-223 |

---

## 3. Diagnosis (code survey on `8574cf4d`)

| # | Finding | Where |
|---|---|---|
| T1 | `UnitOrder` has 19 variants and no unit-target move. Encoding tags 0 to 18 (`SetRally` is 18) | `crates/sim_types/src/commands.rs`; `crates/sim_core/src/checksum/encoding.rs` (`impl Canonical for UnitOrder`) |
| T2 | `route_command` sends `Move` and `AttackMove` to `dispatch_movement` → `issue_movement_prevalidated`, whose validation refuses other orders (`UnsupportedOrder`). Unknown orders fall to `issue_plain_order` (`takes_order`, then `Entity::issue_order`) | `crates/sim_core/src/world/commands.rs`; `crates/sim_core/src/movement/mod.rs` |
| T3 | `walking_goal` reads the front order: `Move` and `AttackMove` give a point; task orders give `task.goal`; `AttackTarget` gives a pursuit (`MotionAction::Pursue`) or, when its target is lost to fog, `last_seen_goal`; anything else coasts | `crates/sim_core/src/movement/locomotion.rs` |
| T4 | Pursuit: `start_pursuit` (target available, `pursuit_band` from the weapon and both collision radii, `aim_if_in_range`), then under the solver `ring_aim` (D-γ), then `chase_target`. That builds a `MotionDraft` with the target's tick-start position (`pursued_motion`) as its goal | `locomotion.rs`; `movement/ring.rs` |
| T5 | `Load` already walks to a moving unit: `step_load` → `pursue_carrier`, which re-aims `task.goal` when the carrier drifts more than `BOARDING_REACH` | `crates/sim_core/src/transport/orders.rs` |
| T6 | Arrival: a unit is done at its destination or when it touches a body with `settled_on == Some(destination)` (`check_cluster_arrival_with`, `ARRIVAL_TOUCH_MARGIN` 0.15). `obstacle_of` fills `settled_on` from `MovementMemory::arrived_at` for idle bodies only | `crates/sim_core/src/steering.rs`; `locomotion.rs` (`obstacle_of`); `movement/tuning.rs` |
| T7 | Contact classification reads `Entity::is_idle()` (no order, `Idle`, `Stop`) in three body builders: `collision_body` and the solver's `ContactParty` in `movement/collision.rs`, and `obstacle_of` in `locomotion.rs`. The solver's share table pushes an idle standing friend aside (C-2 row 6, `ContactParty::idle_standing`) | `movement/collision.rs`; `movement/solver.rs` (read only) |
| T8 | Acquisition: `engagement_of` maps `AttackMove` and idle to `Seek`; `engage` pushes the acquired `AttackTarget` in front of an `AttackMove`; any other order is `Track` or `Busy`. `holds_fire` lets a defensive-level unit acquire only under `AttackMove` | `crates/sim_core/src/target_acquisition.rs` |
| T9 | `MovementMemory::sight` and `lost_target_point` keep the last-seen point of an explicit attack target (B-195, schema 47); `sighting` writes them for `Engagement::Explicit` only | `crates/sim_core/src/entity.rs`; `target_acquisition.rs` |
| T10 | Rally: `issue_waypoint_order` gives `Unit(u)` of the child's player `Load` when admitted, else `Move { u's position }`. For another player's unit it gives `AttackTarget` when `takes_order` accepts it, else the same `Move` | `crates/sim_core/src/production/rally.rs` |
| T11 | Client right click: `friendly_order` offers `Construct`, `Load`, `ReturnCargo` or `Harvest` (extractor), first taken wins, else `context_order` falls back to `Move` to the ground under the pointer. `resolve_target` prefers a node over a mobile friend for harvesters | `crates/client/src/battle_input/context_order.rs` |
| T12 | Client A-click on a unit: `submit_attack_on_unit` splits the selection into attackers (`AttackTarget`) and walkers (`AttackMove` to the point under the unit). The walkers are the Scan Move members plus the Q0105 members | `crates/client/src/battle_pipeline/commands.rs` |
| T13 | No client `M` command: a move on a unit can come only from the right click | `crates/client/src/battle_input/command_mode.rs` |
| T14 | No alliances: owner equality is friendship throughout | DESIGN §4, Combat ("with no teams") |
| T15 | Route groups form only for `Move` and `AttackMove` (`form_route_group`) | `crates/sim_core/src/movement/groups.rs` |
| T16 | `movement_rules.ron` has a `pursuit` section with `acquire_leash_radius: 21` (whole world units, validated) | `assets/catalog/movement_rules.ron`; `crates/sim_core/src/catalog/movement_rules.rs` |
| T17 | One client test pins today's right-click fallback on a friend: `a_soldier_right_clicking_a_mineral_behind_a_friend_still_moves` expects `Move` where the click resolves to the friend. After this plan it is a follow (D-10). It is the one expected test change | `crates/client/tests/battle_context_order_tests.rs` |

---

## 4. Design decisions

### D-1. The order

```rust
/// Walk to a unit and stay with it (StarCraft II 5.0.14 `move` with a unit target; plan
/// `2026-11-26-follow-order`).
Follow {
    /// The unit followed, of any owner.
    target: EntityId,
    /// `Move` acquires nothing; `AcquireMove` (Scan Move) acquires like an attack-move.
    #[serde(default)]
    kind: FollowKind,
},

/// Which command of the move ability a follow is (`core` AbilData:555–557).
pub enum FollowKind { #[default] Move, AcquireMove }
```

The order is a variant, not a target kind on `Move`. `Move { target: SimVec2 }` is matched in over thirty places
whose point semantics would all need a guard. A new variant makes every exhaustive match name it, and the compiler
lists the sites (R-1).

Encoding: tag 19, then the target, then the kind (0 `Move`, 1 `AcquireMove`).

### D-2. Admission

The order is routed with `Move` and `AttackMove` to `dispatch_movement`, so it reports a `MovementOutcome`. It is
admitted by a new `SimWorld::issue_follow_prevalidated(player, recipients, target, kind, queue)` in
`movement/mod.rs`:

1. **The selection**, exactly as for a move: the player has a slot, the selection is not empty, and every recipient
   is the player's, alive and not carried. Otherwise the whole command is refused with the existing
   `MovementAdmissionError` variants.
2. **The target.** New typed variants of `MovementAdmissionError` refuse the whole command when the target:
   - is missing: `FollowTargetMissing(id)`;
   - is dead: `FollowTargetDead(id)`;
   - is carried or housed: `FollowTargetAway(id)`;
   - is a structure (immobile by its archetype, `is_immobile_by_default`): `FollowTargetImmobile(id)`. A
     structure is no body here (B-131), so the radius rest rule does not fit it. A move on a structure keeps its
     current meaning, a point move (§14). A unit anchored by its stance (a sieged tank) is a body and may be
     followed; the follower simply rests against it.
3. **Takers**, filtered silently as `AttackMove` filters its takers. These are recipients that:
   - are mobile;
   - are not the target itself. The clicked unit keeps its orders [INF, **verify**];
   - for `AcquireMove`, have an A command (`takes_attack_move`).
4. **Commit** through `Entity::commit_follow_order(target, kind, queue)`, which mirrors `commit_movement_order`:
   - without Shift, it replaces the queue, clears the route and restarts the memory;
   - it does not count as a route order (no `route_orders_issued`, no route group, D-8).

   The receipt counts the takers, which may be zero, as for `AttackMove`.

Any owner's unit is a valid target (§2.2 rule 7). Visibility at admission is not checked, as for `AttackTarget`
(rallies plan T13). The client offers only what the viewer can pick.

### D-3. The goal

`walking_goal` gains a `Follow` arm, which calls a new `follow_step(entry, actor, target, kind)` in a new submodule
`movement/follow.rs`. That file is part of the pursuit and goal code, not the solver. Each tick it decides one of:

- **end** (D-5);
- **walk to last seen** (D-5);
- **rest** (D-4);
- **chase**: a `MotionDraft` toward the target's **tick-start position** (`pursued_motion`, the rule B-157 fixed for
  pursuit), clamped into the follower's domain (`clamp_into_domain`).

The chase reuses `chase_target`'s draft, its route upkeep and `PURSUIT_ROUTE_DRIFT` repaths, with **no ring**
(`gap: None`). The ring is the weapon arc of D-γ. A follower without room on a ring must walk up to the crowd and
rest against it (D-4), not stand far away as G-6 has an attacker do. A ground follower of an air target, or a
target over unpathable ground, walks to the nearest reachable point (S-4, Q0006) and rests there.

### D-4. Rest and resume

`MovementMemory` gains `follow_rest: Option<SimVec2>`: the target's tick-start position when the follower came to
rest, in world units. It is `None` while the follower walks. A new order, a pop and `restart()` clear it.

**The follower comes to rest** (velocity zero after any glide, animation `Idle`, order kept) when, on the tick-start
snapshot, any of these holds:

1. its centre is within the radius sum (collision radii, as `pursuit_band` measures) plus `ARRIVAL_TOUCH_MARGIN` of
   the target. This is the touch rule of arrival, with no new number;
2. it touches, within the same margin, a body whose `resting_on` (below) is this target. This is cluster arrival
   (T6) by identity instead of by destination point;
3. its walk reaches the nearest reachable point that replaced an unreachable goal;
4. its walk reaches the stall limit (`STALL_ABANDON_TICKS`). A move gives up there. A follow rests instead, because
   its target may come back. This is a deliberate difference from the move rule: a follow has no point to give
   up on.

`follow_rest` is set to the target's tick-start position.

**A resting follower walks again** when the target's tick-start position lies more than `follow_range_slop` from
`follow_rest` (§2.2 rule 3; strict `>`). Otherwise it stays at rest. A follower pushed away while resting stays
where the push left it until the target moves (**verify**; the same as an idle unit pushed aside).

`SteeringObstacle` gains `resting_on: Option<EntityId>`. `obstacle_of` sets it to the front order's target when the
order is a `Follow` with `follow_rest` set. That is the snapshot's only new field. Witnesses come from the snapshot,
so the answer does not depend on processing order (the locomotion module's invariant).

`follow_range_slop` is authored in `movement_rules.ron` under `pursuit` and documented in the file's header like
`acquire_leash_radius`. It is a whole number of world units, at least 0, and a typed `CatalogError` refuses any
other value. Tests read it from the catalog.

### D-5. How a follow ends

Checked first each tick, in this order:

| Condition (tick-start) | Effect |
|---|---|
| The target is gone or dead | Pop the follow; the follower coasts (B-39 glide) and the next queued order, if any, starts next tick |
| The target is carried (`is_loaded_in_transport`) or housed (`UnitTask::housed_in` set, hidden harvesters) | Same as dead (**verify**) |
| The target is another player's, and the follower's player does not see it or sees it concealed and undetected | Walk to the last-seen point (the B-195 memory, D-6). The order ends there through the move arrival, as `last_seen_goal` does for an attack. If no sighting exists under the order, pop it |
| The follower is resting, or comes to rest this tick, and an order is queued behind the follow | Pop the follow: the queue goes on (S11). A rally route of several units then visits each in turn |

So a follow with nothing behind it lasts until its target is gone or the player gives a new order (S8).

### D-6. Acquisition and sighting

- `engagement_of`: `Follow { AcquireMove }` is `Seek`, like `AttackMove`. `Follow { Move }` stays in the generic
  arm (`Track` or `Busy`), like `Move`.
- `engage`: an acquired target is pushed **in front of** a `Follow { AcquireMove }`, as in front of an
  `AttackMove`. When the attack ends, the follow is the front order again and resumes (S7 "will stick").
- `holds_fire`: a defensive-level unit acquires under `Follow { AcquireMove }` as under `AttackMove`.
- `sighting`: a follow of another player's unit records its sighting through `MovementMemory::sight`, exactly as an
  explicit attack does. The memory is the existing `last_seen` (schema 47), so no new field is needed.
- The acquired chase's leash, D-γ ring and D-δ nudge are those of any acquired attack. They follow from the front
  order, which is an ordinary `AttackTarget { Acquired }`.

### D-7. Contact: a resting follower counts as idle

The solver is not touched. A new `Entity::stands_idle()` returns `is_idle()`, or true when the front order is a
`Follow` with `follow_rest` set. It replaces `is_idle()` in the three body builders of T7:
- `collision_body` and the `ContactParty` in `movement/collision.rs`;
- `obstacle_of` in `locomotion.rs`.

Moving friends then push a resting follower aside by C-2 row 6 (in SC2, standing units are pushed by moving ones,
Anhalt GDC 2011 as summarised in Q0118) [INF, **verify** for a unit under a follow]. This includes the target
walking back through its followers. `is_idle()` itself keeps its meaning for acquisition, the HUD and the tasks.

- **Friend overlap.** The rest band is outside contact (the radius sum plus 0.15). So followers do not press into
  their target, and D-α's tolerances play no part in a rest. A follower of an enemy meets the hard enemy contact
  first and rests by D-4 rule 1 at contact.
- **D-δ.** A resting follower is not engaged, so D-δ never anchors or nudges it. An `AcquireMove` follower in a
  melee fight is an engaged attacker like any other, and D-δ applies as today. Nothing here depends on D-δ, so the
  owner's possible revert (Q0118's note) leaves follows unchanged.
- **Moving goal.** The chase reads the target's tick-start position. A follower directly behind a walking target
  therefore stays about one step back and does not push it. The solver separates any overlap by the ordinary
  friend rows.

### D-8. Group follows

Each follower is independent. There is no route group, lane or formation (§2.2 rule 8; S-7's lane sharing stays a
point-move rule). Several followers bunch round the target by D-4 rules 1 and 2: the first ring touches the target,
and later followers rest against resting followers. When the target moves past the slop, every follower resumes on
the same tick from its own position. This is 5.0.14's "group up around" (S10) without a formation, which 5.0.14 does
not keep either.

### D-9. Rallies on units (B-163, B-166's half)

`issue_waypoint_order` (T10) changes two rows:

| Waypoint | Child | Order (was) | Order (now) |
|---|---|---|---|
| `Unit(u)` own, `admit_load` refuses, `u` mobile | any | `Move { u's position }` | `Follow { u, Move }` through `issue_follow_prevalidated` |
| `Unit(u)` another player's, `takes_order` refuses the attack, `u` mobile | any | `Move { u's position }` | `Follow { u, Move }` |
| `Unit(u)` immobile (a structure) | any | `Move { u's position }` | unchanged |

A rally is a move, not a scan move (rallies plan S11), so the kind is `Move`. With D-5's queue rule, a route of
several unit waypoints is visited in turn. Admission's invariants are reported as the waypoint's other orders
report them.

### D-10. Client

- **Right click on one's own mobile unit.** `friendly_order` gains a last row,
  `Follow { target: friend, kind: Move }`, offered when the friend is mobile and some member can follow it
  (`any_recipient`). It comes after `Construct`, `Load`, `ReturnCargo` and `Harvest`, so a carrier still loads and
  an extractor still harvests. The marker is `OrderMarkerKind::Move` at the ground under the friend. Shift queues,
  as every right click does. A right click on one's own structure keeps the `Move` to the ground (D-2). T17's test
  changes to expect the follow.
- **The context-order table** in `context_order`'s doc gains the row.
- **A-click on a unit** (`submit_attack_on_unit`). The walkers split in two:
  - **Scan Move members**, whose settled stance's A command is `ScanMove`, get `Follow { target, AcquireMove }`
    (B-187);
  - **the Q0105 members**, who have Attack but cannot attack the target, keep `AttackMove` to the point. That is the
    owner's answer to Q0105 (draft Q0119 asks whether they should follow too).

  The three commands go into the same tick with disjoint recipients, and their outcomes reach the status line as
  today's two do. If the target is immobile, every walker keeps `AttackMove` to the point (D-2).
- **Exhaustive matches.** `can_receive_order`, `capable`, `can_execute` and status text handle `Follow`.
  `capable(Follow)` is "mobile, and for `AcquireMove` has an A command". This mirrors D-2, so the client never
  offers what admission refuses.
- **Rally view.** Unchanged. A unit waypoint is already drawn at the unit's current position.
- **Allies.** None exist (T14). The `Friendly` target is one's own unit.

### D-11. Records

WP0 registers these rows with provisional numbers:

- **B-221 (open, minor, deferred):** "A follower that can attack the enemy it follows never switches to attacking it
  within 5.0.14's `FollowAcquireRange` 6 (S5, S6). Unreachable through the client (no `M` command). Only a
  `PlayerCommand` can create it."
- **B-222 (open, deferred):** "A right click gives its order to the members that take it and leaves the rest. 5.0.14
  resolves the smart command per unit: in a selection right-clicking a Medivac, a Viking follows it while marines
  board, and an unarmed unit right-clicking an enemy moves to it (**verify**)."
- **B-223 (open, minor, verify):** "The Battlecruiser's move ability (`BattlecruiserMove`) has no `FollowRangeSlop`,
  so its slop is the class default, not 1 (S3). Here every unit uses the world's 1."

When this plan is done:
- **B-163** is fixed by WP3 (simulation) and WP4 (client).
- **B-166**'s remaining half is fixed by WP3.
- **B-187**'s Scan Move half is fixed by WP4. Its Q0105 half is re-worded as "by the owner's answer to Q0105, not a
  difference while Q0119 is open".

---

## 5. Determinism and checksum

- **No float.** Distances are checked `SimScalar` arithmetic through `movement::math` (`within_distance`,
  `checked_distance`). The slop is a whole number read into `SimScalar`. Nothing reads a clock or a hash map.
- **Order independence.** Every decision reads the tick-start snapshot: the target's position, the bodies'
  `resting_on`, and the follower's own memory. Commits go through the existing proposal.
- **Hashed state.**
  - The `UnitOrder` encoding gains tag 19, which is reachable in any world with a follow.
  - `MovementMemory` gains `follow_rest` (an `Option<SimVec2>`), encoded after `last_seen`. That adds bytes to
    every unit.
  - The schema takes the **next free number at merge** (50 on `8574cf4d`; friend-crowd-plan changes no schema,
    per the orchestrator's note).
- **Goldens are re-recorded once, in WP1, by bytes only.** The proof follows rallies WP1:
  1. With the new field present but not encoded, and the schema at its old number, both goldens and
     `DEFAULT_SANDBOX_CHECKSUM` reproduce on four builds (Linux debug and release, `wasm32-wasip1` debug and
     release).
  2. Then the encoding and schema go in, and the checksums are recorded under a header "Re-recorded for plan
     2026-11-26-follow-order WP1 (checksum schema N−1 -> N): each unit's movement memory gains `follow_rest` (None
     in these worlds). No behaviour change."

  No later package may move a golden. If one does, stop rule 2 applies.
- **The schema doc line** in `checksum.rs`: "Schema N adds the `Follow` order (tag 19, then its target and kind, 0
  `Move` and 1 `AcquireMove`) and each unit's follow rest, `MovementMemory::follow_rest`, after `last_seen` (plan
  `2026-11-26-follow-order` D-1, D-4). The number is the next free at merge."
- **Cost.** A follower costs a chase per tick when it walks and a few distance checks when it rests. The extra
  snapshot field is one `Option<EntityId>` per body. The throughput bench has no follow, so its cost is the field
  and the match arms. It must stay ≥ 50 ticks/s in every scenario, with user-space instructions within 3 % of WP0's
  tree (rallies plan P-8's method, since wall-clock time on the shared server is noisy).

---

## 6. Acceptance (principled properties, fixed before implementation)

Every property is a property of the rule, tested on constructed worlds. The worlds read radii, speeds, the slop
and the margin from the catalog and the tuning constants, never from a seed's outcome. Each sim test is written
first and shown red on the tree before its package's change. A red run that fails to compile (no `Follow` yet)
counts as red only for WP1. From WP2 on, the variant exists and the tests fail on behaviour.

- **P-0 (probe, WP0, docs only).** A temporary scratch counter, not committed, is run on WP0's tree over:
  - the commands of `~/scratch/resource-footprints/scripts/measure.sh` (the matrix with its seeds of record and 16,
    battle 31 on 100 seeds, battle 33 on 64, the showcase on nine seeds for 6,000 ticks, and `showcase_effects`);
  - the client skirmish match.

  It counts the inputs that would become a follow:
  - rally spawns with a `Unit` waypoint (rows of D-9);
  - client right clicks that resolve to `PointerTarget::Friendly` and fall through to `Move`;
  - A-clicks on a unit with Scan Move walkers;
  - scenario or fixture orders that name a unit as a move target (none can exist today; checked by a grep of the
    fixtures for `Follow`, which must return 0).

  Expected: zero in every measured world, so their state outputs are byte-identical through every package. The
  skirmish pilot right-clicks minerals and enemy domes only (T11 survey), so its count is expected to be zero too.
  A non-zero count stops the plan before WP1 (stop rule 1).
- **P-1 (state).**
  - `checksum::encoding` unit tests cover the tag and kind bytes and `follow_rest` present or absent.
  - `checksum_state_tests`: the checksum changes when a follow's target, kind or `follow_rest` changes.
  - §5's bytes-only proof is recorded.
- **P-2 (admission)** (`follow_order_tests.rs`):
  - each typed refusal (missing, dead, carried, housed or immobile target; a foreign, dead or carried recipient;
    an empty selection);
  - the target is not among the takers;
  - immobile members are skipped;
  - `AcquireMove` skips members without an A command;
  - Shift appends behind a `Move` and the follow starts when the move ends;
  - the receipt counts the takers.
- **P-3 (follow a walking friend).** On open ground, one follower walks after a target that moves along a polyline
  at a speed at most the follower's. From the first tick the follower comes within the rest band, until the target
  stops, its edge gap stays at most `follow_range_slop` + the target's top speed per tick +
  `ARRIVAL_TOUCH_MARGIN`. The bound is derived from D-3 and D-4, not measured. Its order is the follow on every
  tick.
- **P-4 (rest and slop).**
  - The target stops, and the follower rests within the band: zero velocity after its glide, `Idle`, order kept.
  - The target is then displaced by `follow_range_slop` exactly: the follower stays at rest on every tick.
  - Displaced by the slop plus one raw unit: the follower walks on the next tick and rests again touching it.
- **P-5 (ends).**
  - The target dies: the follow is popped on the next movement phase and the follower glides to rest.
  - The target boards a carrier: the same.
  - `[Follow A, Move p]`: the follower reaches A and then walks to p.
  - The same with A dead before it is reached: it walks to p.
  - `[Follow A]` alone: still following 500 ticks after A stopped.
- **P-6 (group).** Twelve followers of one standing target on open ground, started on a ring of radius 10:
  - within the ticks needed to walk 10 at their speed plus `STALL_ABANDON_TICKS`, every follower rests;
  - the graph "rests touching" (D-4 rules 1 and 2) is connected to the target;
  - no follower gives its order up;
  - no follower's push moves the target more than D-α's resting bound.

  Then the target walks 20 and stops: all twelve rest again round its new position under the same bound.
- **P-7 (acquire and enemy).**
  - A `Follow { AcquireMove }` follower meets an enemy in scan range: it acquires it, fights, and when the enemy is
    dead its front order is the follow again, and it rests by the target.
  - A `Follow { Move }` follower under the same scene acquires nothing (its weapon tracks only, as under `Move`).
  - A follow of an enemy that walks into fog: the follower walks to the last-seen point and its order ends there.
  - A follow of an enemy the follower never saw under the order is popped.
- **P-8 (contact).**
  - `stands_idle` is true for a resting follower and false for a walking one.
  - A target walking back through two resting followers reaches its goal in at most the ticks of an unobstructed
    walk plus `STALL_REPATH_TICKS`. The followers are pushed aside as idle units are.
  - No `movement/solver*.rs` file appears in the diff (`git diff --stat`).
- **P-9 (rally).**
  - Spawns with a rally on:
    - one's own mobile non-carrier: `Follow { Move }`;
    - one's own carrier: `Load`, unchanged;
    - one's own structure: `Move`, unchanged;
    - an enemy the child can attack: `AttackTarget`, unchanged;
    - an enemy it cannot attack: `Follow { Move }`.
  - A route of two own units is visited in turn.
  - A child following a unit that later dies stops.
- **P-10 (integration through `PlayerCommand`).** One test in `crates/sim_core/tests/follow_order_tests.rs` drives
  everything through `TickBundle`s only:
  - a group of three soldiers and a Medivac analogue are given `Follow { Move }` on a walking soldier;
  - a Raven analogue is given `Follow { AcquireMove }` on the same soldier;
  - a structure's rally is set (`SetRally`) on that soldier, and a unit is trained.

  The soldier then walks a route and stops. The test asserts:
  - every follower rests round it at the end;
  - the trained unit's order is the follow from its spawn tick;
  - after the soldier is killed by a scripted enemy, every follower's queue is empty.
- **P-11 (client)** (`battle_context_order_tests.rs`, `battle_input_tests.rs`):
  - a right click on one's own mobile unit queues `Follow { Move }` for the selection, and with Shift, `queue:
    true`;
  - on one's own carrier, it loads;
  - on one's own structure, `Move` as before;
  - T17's test expects the follow;
  - an A-click on a unit with a Medivac-class member, an armed member and a ground-only member against an air
    target gives `Follow { AcquireMove }`, `AttackTarget` and `AttackMove` respectively.
- **P-12 (worlds).** §7.
- **P-13 (throughput).** Every bench scenario ≥ 50 ticks/s in release, and user-space instructions within 3 % of
  WP0's tree, measured after WP2 and at WP5.

---

## 7. Worlds that move

| World | WP1 | WP2 | WP3 | WP4 |
|---|---|---|---|---|
| Matrix (seeds of record and 16), battle 31 (100), battle 33 (64), showcase and `showcase_effects` (9 seeds, 6,000 ticks) | byte-identical (state outputs) | byte-identical | byte-identical | byte-identical |
| `golden_replay`, `golden_replay_orders` | **re-recorded, bytes only** (§5 proof) | unchanged | unchanged | unchanged |
| `DEFAULT_SANDBOX_CHECKSUM` | moves (schema, encoding) | unchanged | unchanged | unchanged |
| Client skirmish match | stage log identical | identical | identical (P-0: no rally on a unit) | identical (P-0: no right click on a friend) |

"Byte-identical" means the trace output of the measure script, compared with WP0's tree in every package that
touches `sim_core`. Any difference is a defect of the package, never a re-record.

---

## 8. Stop rules

1. **P-0 finds a follow-type input in a measured world or the skirmish.** Stop before WP1 and rewrite §7 from the
   trace.
2. **A world marked byte-identical, or a golden after WP1, differs.** Stop, trace the first differing tick, and
   report. Do not re-record.
3. **The design needs a change inside `movement/solver*.rs`** (for example, a share row for followers). Stop and
   report. That code belongs to the friend-crowd work and to Q0118's blank-sheet round.
4. **An existing test other than T17's fails** in a way that is not a compile error from the new variant. Stop and
   report the test and why.
5. **A rule needs a constant not derived in §2.4 or D-4** (a margin, a timer, a distance) to pass a test. Stop. No
   tuning constant may be introduced; the plan is amended instead.
6. **Throughput below 50 ticks/s** in any scenario, or instructions more than 3 % above WP0's. Stop with the profile.
7. **A source read during implementation contradicts a §2.2 rule** (for example, Liquipedia readable again and
   saying a follow ends when the target stops). Stop and amend before building on the rule.
8. **The schema number collides at merge.** Take the next free number, re-record by bytes only with the same proof,
   and note it in `## As built`.

---

## 9. Client consequences

- `context_order.rs`: the follow row and the table in the doc (D-10).
- `battle_pipeline/commands.rs`: `submit_attack_on_unit`'s three-way split; `capable` and `can_execute` for
  `Follow`.
- Status text for the new `MovementAdmissionError` variants.
- No HUD, card, marker or view change. The move marker shows at the ground under the friend.
- No GPU test is needed. The orders are asserted in the existing headless input harness.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP5 | B-221 to B-223 registered; B-163, B-166, B-187 point here; fixed rows; Movement row sentence |
| `crates/sim_types/src/commands.rs` | WP1 | `UnitOrder::Follow`, `FollowKind` |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | WP1 | Tag 19, `follow_rest`, schema line |
| `crates/sim_core/src/entity.rs` | WP1, WP2 | `follow_rest`; `commit_follow_order`; `stands_idle`; `order_walks` arm |
| `crates/sim_core/src/world/commands.rs` | WP1 | Routing to `dispatch_movement` |
| `crates/sim_core/src/movement/mod.rs` | WP1 | `issue_follow_prevalidated`, error variants |
| `crates/sim_core/src/movement/follow.rs` (new) | WP2, WP3 | `follow_step`: end, rest, resume, chase |
| `crates/sim_core/src/movement/locomotion.rs` | WP2 | `walking_goal` arm; `obstacle_of` (`resting_on`, `stands_idle`); the draft's arrival and stall outcomes for a follow |
| `crates/sim_core/src/steering.rs` | WP2 | `SteeringObstacle::resting_on` |
| `crates/sim_core/src/movement/collision.rs` | WP2 | `stands_idle` in the two body builders (merge risk with friend-crowd, R-3) |
| `crates/sim_core/src/catalog/movement_rules.rs`, `assets/catalog/movement_rules.ron` | WP2 | `pursuit.follow_range_slop` and its validation |
| `crates/sim_core/src/target_acquisition.rs` | WP3 | `engagement_of`, `engage`, `holds_fire`, `sighting` |
| `crates/sim_core/src/production/rally.rs` | WP3 | D-9 rows |
| `crates/sim_core/tests/follow_order_tests.rs` (new) | WP1 to WP3 | P-2 to P-10 |
| `crates/sim_core/tests/rally_tests.rs` | WP3 | P-9 |
| `crates/gate/suites.ron` | WP1 | The new test target in the tiers [`test-gates.md`](../guides/test-gates.md) names for `sim_core` |
| `crates/client/src/battle_input/context_order.rs`, `battle_pipeline/commands.rs`, status text | WP4 | D-10 |
| `crates/client/tests/battle_context_order_tests.rs`, `battle_input_tests.rs` | WP4 | P-11 |
| `crates/sim_fixtures` | WP1 | Exhaustive matches only, if any |
| `docs/evidence/2026-11-26-follow-order.md` | WP0 to WP5 | Probe, world comparisons, proof, throughput |

Not touched: `movement/solver*.rs`, `movement/ring.rs`, `movement/concave.rs`, `movement/groups.rs`.

---

## 11. Work packages (commit order)

Every package is implemented on branch `follow-order`:
- each test it adds is written and shown red first;
- the implementer runs `cargo gate quick` and the tests the change needs, then hands back; a verifier runs
  `cargo gate package` on the final tree (Q0061);
- §7's comparison runs against WP0's tree, with outputs under `~/scratch/follow-order/<wp>/`.

### WP0: records and probe (S, docs only)

- B-221 to B-223 registered with the next free numbers, and this plan renumbered in the same commit if they differ.
- B-163, B-166 and B-187 point to this plan.
- P-0 on WP0's tree.
- The baselines of §7's worlds and the bench (wall clock and instructions) go into the evidence.

### WP1: the order, its state and admission (M, `sim_types`, `sim_core`)

- `UnitOrder::Follow`, `FollowKind`, the encoding, `follow_rest` (written by nobody yet), the schema and §5's
  bytes-only re-record.
- D-2 admission and routing, and `commit_follow_order`.
- Until WP2, the movement phase leaves a follower standing: `walking_goal`'s generic arm coasts it. That is the
  package boundary. The branch is merged only after WP4.
- **Acceptance:** P-1, P-2, §7's WP1 column.

### WP2: following (M, `sim_core`, catalog)

- `movement/follow.rs`: D-3, D-4, and D-5's own-player rows.
- `resting_on`, `stands_idle`, `order_walks`.
- `follow_range_slop` in data.
- **Acceptance:** P-3, P-4, P-5, P-6, P-8; the throughput bench once (P-13).

### WP3: enemies, acquisition and rallies (S–M, `sim_core`)

- D-5's last-seen row and D-6.
- D-9's rally rows.
- **Acceptance:** P-7, P-9, P-10. B-163's and B-166's simulation halves are fixed.

### WP4: client (S, `client`)

- D-10. T17's test is updated in the same commit, with the reason in the message.
- **Acceptance:** P-11, and the skirmish stage log identical (§7).

### WP5: bookkeeping (S)

- `DESIGN.md`:
  - B-163 and B-166 fixed with commits;
  - B-187 re-worded (D-11);
  - B-221 to B-223 open;
  - the Movement row gains one sentence on follows.
- The rallies and attack-orders plans' pointers.
- The evidence completed: P-0, §7 per package, the WP1 proof, P-13.
- This plan's `## As built`, and the README row `Done`.
- Draft Q0119 filed if the orchestrator agrees.

---

## 12. Test plan (summary)

| Concern | Property | Where |
|---|---|---|
| State | Tag, kind and rest hashed; schema; goldens by bytes only | WP1: `checksum::encoding`, `checksum_state_tests`, goldens on four builds |
| Admission | Typed refusals, takers, Shift | WP1: `follow_order_tests` |
| Following | Gap bound, rest, slop, resume | WP2: `follow_order_tests` |
| Ends | Death, carried, queue behind, persistence | WP2: `follow_order_tests` |
| Groups | Connected rest graph, no give-ups, re-rest after a move | WP2: `follow_order_tests` |
| Contact | Resting followers yield; no solver diff | WP2: `follow_order_tests`, diff check |
| Acquisition and fog | `AcquireMove` resumes after a fight; `Move` does not acquire; last-seen walk | WP3: `follow_order_tests` |
| Rally | D-9 table, routes of units | WP3: `rally_tests` |
| Integration | Through `TickBundle` only | WP3: `follow_order_tests` (P-10) |
| Client | Right click, Shift, A-click split | WP4: client input tests |
| Worlds | §7 | WP1 to WP4 |
| Throughput | ≥ 50 ticks/s, within 3 % | WP2, WP5 |

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | Exhaustive `UnitOrder` matches across `sim_core`, `sim_fixtures` and `client` | Build failures in WP1 | The compiler lists them. Each gets the arm its meaning needs (a follow walks, is not a task order, is not a route order) |
| R-2 | The draft's arrival and stall outcomes are shared with moves | A follow pops like a move, or a move rests like a follow | The follow outcome is chosen by the front order's variant in one place. P-5 and the existing route-following tests pin both |
| R-3 | `movement/collision.rs` is also edited by the friend-crowd branch | Merge conflict | The change is the one-word `stands_idle` swap at two sites. Whichever merges second rebases |
| R-4 | A resting follower being pushable changes a battle | A measured world moves | No measured world follows (P-0), and stop rule 2 applies |
| R-5 | The §2.2 rules marked **verify** are wrong (stop gap, carried target, pushability) | A minor difference | Each is one rule in `follow.rs` or one predicate. The evidence lists them for a later in-game check |
| R-6 | A ground follower of an air target over a cliff stalls every few ticks | Animation flicker | D-4 rule 3 rests it at the nearest reachable point. P-4's resume rule keeps it at rest until the target moves past the slop |
| R-7 | The schema number collides with another branch | Conflict in `checksum.rs` and the goldens | Next free at merge, stop rule 8 |

---

## 14. Out of scope and decisions recorded (Q0002)

- **`FollowAcquireRange` (B-221).** No client gesture makes a follower of an enemy it can attack.
- **Per-unit smart resolution (B-222).** A right click still gives one order to the members that take it.
  Built by plan [`2026-12-01-rallies-and-smart-command`](2026-12-01-rallies-and-smart-command.md) WP1 (each member takes the first row it can carry out); its transport
  pickup half is B-247 there (WP-D1, deferred).
- **The Battlecruiser's slop (B-223).**
- **Move on a structure** stays a point move to the ground under the pointer (D-2). 5.0.14 "move[s] to the
  friendly building" (S9), which matches.
- **The Q0105 members** keep the owner's answer (point attack-move). Draft Q0119.
- **A client `M` (Move) command and Patrol:** no such command exists here (T13).
- **Order-path display** (5.0.14's waypoint lines, S1's `OrderArray`): this project draws no order lines.
- **Allies:** no teams exist (T14).
- **The contact solver**, the D-γ ring, D-δ and group routing are not changed (stop rule 3; Q0118's blank-sheet
  round may revisit pushability, and a follow depends only on `stands_idle`).
- **Return fire under a move-follow** (S7's report that it breaks the follow): a `Move` follow here behaves as a
  `Move` does (its weapon tracks or is busy, T8), which is this project's current rule for moves; the report is
  co-op era and unconfirmed for 5.0.14 multiplayer (**verify**).

---

## 15. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,void,voidmulti,balancemulti}/AbilData.xml` (`5444e12f13`),
  read as data.
- Web, read on 2026-11-25 through the web fetcher; notes in `~/scratch/follow-order-plan/web/sources.txt`.
  Liquipedia returned HTTP 429 and then a rate-limit page. It was not retried, and its claims carry **verify**.

---

## 16. Owner question (filed as [Q0119](../questions/closed/0119-follow-when-cannot-attack.md); answered (b), see As built item 8)

```markdown
# Q0119: When you A-click a unit, should the units that cannot attack it follow it, as Scan Move units do?

- **Asked:** 2026-11-26
- **Blocking:** no
- **Default until answered:** (a), they attack-move to where the target stood when you clicked (your answer to
  Q0105).

## Question

With the follow order (plan `2026-11-26-follow-order`), an A-click on a unit now makes units without any weapon
(Medivac, Raven, Observer: Scan Move) follow that unit and fight what they can on the way, as StarCraft II does.
Units that do have a weapon, just not one for this target (a Marauder A-clicked onto a Viking, an unburrowed
Lurker), still attack-move to the point where the target stood when you clicked, as you chose in Q0105. If the
target flies away, they stop at that old point.

- **(a) Keep your Q0105 answer (default).** They go to the point. It is simple and predictable.
- **(b) Let them follow too.** They follow the target and fight on the way, like the Scan Move units next to them,
  so a mixed army stays together. The game data does not say which the game does for these units.

Recommendation: (b) if you want mixed selections to move as one; (a) otherwise. Either is a one-line change in the
client.

## Answer

```

---

## As built

Built on branch `follow-order-plan` (on `main` `b4a99fae`); evidence in
[`docs/evidence/2026-11-26-follow-order.md`](../evidence/2026-11-26-follow-order.md).

| WP | Commit | What |
|---|---|---|
| WP0 | `af52e314` | B-221 to B-223 registered (B-219 and B-220 are the friend-crowd branch's, so no renumbering); B-163, B-166, B-187 point here. P-0: no follow-type input in 1,410 measured worlds or the skirmish |
| WP1 | `b913f97e`, `4e77c0b5` | `UnitOrder::Follow`, `FollowKind`, tag 19, `MovementMemory::follow_rest`, schema **50 (provisional)**, admission with four typed refusals; the goldens re-recorded by bytes only after §5's proof on four builds |
| WP2 | `b5e9f218` | `movement/locomotion/follow.rs`, `stands_idle`, `SteeringObstacle::resting_on`, `pursuit.follow_range_slop` |
| WP3 | `0a797bab` | Acquisition and sighting for follows, the lost-to-fog walk, D-9's rally rows |
| WP4 | `27604c11`, `75253b34` | The right-click row and the A-click's three-way split; two more tests that pinned the old walk |
| WP5 | this commit | Records |

Differences from the text above, each needed to meet the plan's own acceptance:

1. **Where the code lives.** `follow.rs` is `movement/locomotion/follow.rs`, a child of the locomotion module, so
   it reuses the pursuit's private draft, stages and helpers without widening their visibility. Admission is in
   `movement/mod.rs` as planned; `try_issue_movement` routes a `Follow` too (`admit_movement_order`), so a refusal
   of a unit dead this tick is reachable through the public API.
2. **A rest is an arrival (D-4).** A follower that comes to rest stands at once, as a move that arrives does
   (`MotionProposal::finish` sets no glide); only the end of a follow glides (B-39). With a glide, twelve followers
   converging on P-6's target pushed it about twice D-α's resting bound.
3. **Rule 2 inherits its witness's anchor (D-4, D-8).** A follower that rests because it touches a resting
   follower takes that follower's `follow_rest`, the lowest id's when it touches several, and `resting_on` carries
   the target and the anchor (`steering::FollowRest`) and is set only while the target stands within the slop of
   the anchor. As written (the anchor is the target's position whenever a follower comes to rest), a group behind a
   walking target re-anchored in waves against each other and stood still; with the inherited anchor every rest of
   a group traces back to a follower that touched the target, and the group resumes on one tick (D-8).
4. **Braking (D-3).** The walk brakes to stop at the rest band's edge (radius sum plus `ARRIVAL_TOUCH_MARGIN`),
   and also at the band of a resting follower of the same target ahead of it; on its last leg it steps toward the
   point `touch` short of the target's centre, never onto it. Without the second brake, followers trailing a target
   that stopped walked into the resting group and pushed the target about 0.16 on. No constant was added.
5. **D-4 rule 3** rests the follower when a route search finds the target unreachable and the nearest reachable
   point lies within one step (or nothing nearer is reachable at all). A follow pushes no `RouteOutcome`.
6. **Existing tests that changed.** Besides T17, D-9 and D-10 change tests that pinned the snapshot `Move`:
   `rally_tests::a_unit_rallied_to_a_friend_goes_where_the_friend_stood` (now `..._follows_it`),
   `a_dead_waypoint_is_skipped_and_an_all_dead_route_is_cancelled` (the live waypoint is followed),
   `a_unit_that_cannot_attack_moves_to_the_enemy` (now `..._follows_the_enemy`, its enemy in the producer's sight:
   a follow of an enemy never seen ends at once), the queue unit test
   `an_attack_on_a_unit_splits_the_selection_into_attackers_and_walkers` (the Medivac-class member follows),
   `context_order::tests::open_ground_and_an_idle_friend_mean_move` (an idle friend is followed) and
   `battle_mode_tests::an_a_click_on_ones_own_unit_attacks_it` (the Medivac-class member follows the own unit); the
   last two were found by the release client run and updated in `75253b34`. Each asserted the behaviour this plan
   fixes; none failed for another reason (stop rule 4).
7. **`gate/suites.ron`.** Unchanged: unlisted targets are `fast`, which is where `follow_order_tests` belongs.
8. **Q0119** was filed before WP0 (`cc1d0ecc`), and WP4 built its default (a). The owner answered (b) (`705d350d`):
   in `94445b39` the A-click's split gives `Follow { AcquireMove }` to every member that cannot attack a mobile target,
   Attack or Scan Move alike, and the attack-move to the point stays only for a structure target. Q0105's default is
   superseded for unit targets; B-187 is fully fixed. D-10's "Q0105 members keep `AttackMove`" and §14's matching
   entry no longer hold.

Measured results (P-0, §7's worlds per package, §5's proof, P-13) are in the evidence.
