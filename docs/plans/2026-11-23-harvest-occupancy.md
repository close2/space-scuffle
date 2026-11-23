# Blueprint: Resource Occupancy and the Uninterruptible Return Wait (B-61, B-160)

This plan gives harvesting StarCraft II 5.0.14's occupancy and its saturation:

- **One worker at a time on a resource (B-61).** A mineral field is gathered by one worker at a time, and one
  worker at a time is inside an extractor. A worker that reaches an occupied resource joins its line and waits its
  turn; the field is free again at the take, before `ReturnDelay`.
- **Workers spread themselves (B-61).** A worker that reaches an occupied mineral field moves on to a free field
  nearby when that field has at least two fewer workers assigned than its own; otherwise it waits. So workers sent
  together split over a mineral line, and a balanced line stays put. Gas workers never move on (the data acquires
  only minerals).
- **The search is the data's (B-67).** The radius in which a worker looks for another field, today an unsourced 12,
  becomes the harvest ability's `AcquireRadius 10`, authored on the worker's `harvester` row with the kinds it
  acquires (`ResourceAcquire Minerals`). An exhausted geyser therefore ends its workers' order, as in the game.
- **`WaitToReturn` is uninterruptible (B-160).** An order given during the `ReturnDelay` wait no longer cuts it
  short: the rest of the wait is carried into the unit's action lock, the mechanism that already makes a cast's
  backswing hold the next order, so the new order starts when the wait would have ended.

A full plan is required (charter §6): a new mechanic that moves every economy world (income per worker falls once a
field has more than its trip allows, and 3 and 4 workers on a geyser are capped), and it spans the catalog, the
economy and command admission. **No hashed state is added**: occupancy is derived from hashed state, the waiting
count is the existing `UnitTask::wait_ticks`, and the carried wait is the existing `Entity::cast_lock_ticks`. The
checksum schema stays 33 and the goldens are not re-recorded (§5).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`f399d403`. Every measurement of §7 is on that tree.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations | Closed. S-1 (RON data instead of editor data) covers authoring the acquire radius and kinds in RON | D-4 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0002: scoping is the agent's call if written down | Standing rule | §14 |
| The owner's guidance for this plan: no fitting to fixed seed sets; judge on principled properties | Followed | §6 (curves derived before measuring), D-8 |
| **None new** | The data and the sources decide every rule; the unsourced details (the balance margin of D-3, the hand-off gap, `HideHarvesters`) are recorded decisions | §4, §14 |

---

## 1. Summary

| | Today (`f399d403`) | After this plan |
|---|---|---|
| Workers on one field or extractor | Any number gather at once | One gathers; the others wait in line (earliest arrival first, then lowest `EntityId`) |
| Workers sent to an occupied field | Gather there anyway | Move on to a free field within 10 that has at least two fewer workers assigned; otherwise wait |
| Next field on exhaustion | Nearest live node of the same kind within 12 (unsourced, B-67), minerals and gas | Within `AcquireRadius` 10, minerals only (`ResourceAcquire`); an exhausted geyser ends the order |
| An order given during `ReturnDelay` | Replaces the wait at once (B-160) | Starts when the wait ends; the cargo stays on the unit |
| Where the numbers live | `NEXT_NODE_RADIUS` in `harvest.rs`; `harvester: Some(())` | `harvester: Some((acquire_radius: 10.0, acquires: [Mineral]))` in `units.ron` |
| Hashed state | — | Unchanged: schema 33. Occupancy is derived; the line uses `UnitTask::wait_ticks`; the carried wait `Entity::cast_lock_ticks` |
| Showcase mineral line (8 fields), per minute | `k` times one: 446 / 893 / 1,339 / 1,786 with 1 / 2 / 3 / 4 workers a field (counted 453 / 903 / 1,350 / 1,800) | **446 / 893 / 1,200 / 1,200**, derived and measured (§6, §7); the game's 925 with 2 and 1,140 with 3 (theory 1,206) |
| Near geyser, per minute | 61 / 121 / 182 / 242 with 1 to 4 workers | **61 / 121 / 171 / 171**; the game's 61 / 123 / 163, cap 169.6 |
| Worlds that move | — | Economy worlds with more than one worker on a resource. Matrix, battles 31 and 33, the showcase on nine seeds byte-identical (§7.3); goldens unchanged |
| Size | — | M: five packages, two of them M |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`), read as data from `~/scratch/combat_wp3_data/<mod>/`: "core Abil:N"
  is line N of `core/AbilData.xml`, "liberty Behavior:N" of `liberty/BehaviorData.xml`, and so on. Nothing there was
  executed.
- **[BLIZ]** Blizzard's own documentation.
- **[COMM]** Community documentation.
- **[INF]** An inference of this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | core Abil:353–380 (the `CAbilHarvest` default, which `SCVHarvest`, `ProbeHarvest` and `DroneHarvest` inherit unchanged, liberty Abil:1094–1099, 2241–2243) | `AcquireRadius 10` (:361); `ResourceAllowed` Minerals, Vespene, Terrazine, Custom 1 (:364–367); **`ResourceAcquire index="Minerals" value="1"`** (:368), no `ResourceAcquire` for Vespene; `UninterruptibleArray WaitToReturn 1` (:379) [DATA] |
| S2 | voidmulti Abil:1890–1901 | The three workers' harvest abilities: `CancelableArray ApproachResource 1`, `CancelableArray WaitAtResource 1`. So the ability has a stage of **waiting at the resource**, and it can be cancelled; `WaitToReturn` is not made cancelable by any layer [DATA, checked by `grep` of every layer] |
| S3 | liberty Abil:703–714 (`MULEGather`: `FlagArray BypassResourceQueue 1`); voidmulti Abil:679–683 and balancemulti Abil:578–582 (`BypassResourceQueue 0`, `ResourceQueueIndex 1`) | The engine keeps a **resource queue**: a harvest ability joins it unless it bypasses it, and the MULE uses a second queue (index 1), so it gathers beside an SCV but not beside another MULE. The workers use the default queue (index 0) [DATA; the reading of the flags is [INF], confirmed by S7's MULE sentence] |
| S4 | liberty Behavior:743–756 (`MineralFieldMinerals`), :817–852 (`HarvestableVespeneGeyserGas`, `…Protoss`, `…Zerg`) | Every mineral and extractor row: `IdealHarvesterCount 3` (presentation: the town hall's saturation display). The extractor rows: `Flags HideHarvesters 1` (the worker is hidden inside while it gathers) [DATA] |
| S5 | core Behavior:81–85 (`CBehaviorResource` default) | `EnabledSearchFilters Worker;Dead`, `EnabledSearchRadius 5`: not a harvest rule this plan uses (its meaning is not in the data, **verify**; §14) [DATA] |
| S6 | Blizzard, [Game Guide: Resources](https://news.blizzard.com/en-us/article/4488900/game-guide-resources), read on 2026-11-23 | "Workers will automatically arrange themselves to harvest from 'unoccupied' mineral clusters; if no free mineral clusters are available, workers will get in line behind one of the occupied clusters and wait their turn." "Each mineral cluster can only be mined by one worker at a time, but it's still possible to mine minerals faster if more workers are present." "Saturation is typically reached when three workers are harvesting each mineral cluster or vespene gas geyser (though this can change if a base is located further away from available resources)." Gas: "you won't need more than three workers collecting gas from that location. If the geyser is further away, you may need an additional worker or two"; "If you see more than one worker queued up waiting for the geyser to be free, you know you have too many mining it." On a geyser that has run dry: "select all of the workers that used to mine the now-collapsed geyser, and move them to a new geyser" [BLIZ] |
| S7 | Liquipedia, [Resources](https://liquipedia.net/starcraft2/Resources), wikitext through the MediaWiki API on 2026-11-23 | "A worker occupies a mineral field for 1.99 seconds. It then pauses for approximately 0.3571 seconds before returning at full speed. The next worker can already begin operations during the pause." "In theory, a fully saturated mineral line with 8 mineral fields can therefore mine no more than 1206.03 minerals per minute … However, due to workers commuting between mineral fields in search for one that is not occupied, these ideal numbers are never reached." Measurements: one worker on a field 55–60 a minute, two 110–120, "Three workers on one mineral field gather 143 minerals per game minute … regardless of distance"; a base with 8 fields "about 925" with two a field, "about 1140" with three. "MULEs can mine while other SCVs are mining, but only a single MULE can mine a mineral field at one time." Gas: "Workers gathering gas have to spend 1.415 seconds in a Vespene Geyser"; "A fully saturated geyser can therefore procure no more than 169.61 gas per minute"; one worker 61, two 123, three 163 [COMM] |
| S8 | Liquipedia, [Mining Minerals](https://liquipedia.net/starcraft2/Mining_Minerals), same date and method | "if a worker attempts to mine from a field that is occupied by another worker, that worker will instead select another field to mine from (unless the worker is almost done harvesting from the field)"; "a harvester will sometimes switch to another mineral field if their current field becomes occupied"; "workers will self-balance anyways"; "Having any more than two workers on a mineral field will force the third worker to move to mine another field instead, unless they are also over-saturated"; "any patch with 3 workers will yield exactly the same rate regardless of distance. This is because the limiting factor becomes the time 'on' the mineral patch"; gas: "once they get a 3rd, the rates become exactly the same, due to the waiting of a worker at all times"; "Very far gas geysers … may actually require 4 workers for full saturation" (that section is marked outdated for its absolute rates, not for these mechanisms) [COMM] |
| S9 | Liquipedia, [Worker split](https://liquipedia.net/starcraft2/Worker_split), same date and method | "Upon sending a worker to a patch that is being mined from, StarCraft II's AI will automatically assign it to go mine from a patch that is not being mined from. Therefore, sending your initial 6 workers to a single patch will trigger the AI to automatically send the 5 remaining workers to other patches." "workers will first touch upon the patch assigned by yourself before moving on to the next" [COMM] |
| S10 | Liquipedia, Mining Minerals (S8): "players can be found to 'harass' an opponent's mining workers by sending a worker out to mine their minerals. This action prevents the enemy's worker from mining as the field would then be occupied" | Occupancy is shared by all players on a mineral field [COMM] |
| S11 | Plan [`2026-11-10-spellcaster-mechanics`](2026-11-10-spellcaster-mechanics.md) D-7.4 | This project's rule for an uninterruptible ability stage (`CastOutroTime`): "A new order (not Shift) cancels nothing … and starts after the lock", through `Entity::cast_lock_ticks` and `is_action_locked` ("its orders wait") [project precedent] |

### 2.2 The rules that follow

1. **One gatherer per resource and queue.** A worker's harvest joins the resource's queue (S3); only the front of
   the queue gathers (S6 "one worker at a time", S7 for the extractor's cap). Workers bypass nothing (S1, S3).
2. **The line is first come, first served.** "Get in line … and wait their turn" (S6). Who arrived first is the
   order [INF from S6's words; the engine's tie rule between two arrivals on one tick is not sourced: this plan
   breaks it by the lower `EntityId`, D-2].
3. **The field is free at the take.** "The next worker can already begin operations during the pause" (S7). So
   the next gather starts on the tick the previous one ends.
4. **Mineral workers move on, gas workers do not.** The ability acquires another resource only for minerals
   (`ResourceAcquire Minerals 1`, S1). Mineral workers spread (S6, S8, S9); the gas worker queues ("more than one
   worker queued up waiting for the geyser", S6) and stays when its geyser runs dry (S6) [INF: that
   `ResourceAcquire` governs both moving on and the next resource on exhaustion, **verify**].
5. **Moving on balances; it does not churn.** The sources describe moving on to a field "not being mined from" (S9)
   and a third worker leaving a field with two "unless they are also over-saturated" (S8), and a saturated line
   that "self-balance[s]" (S8). They also say the ideal is "never reached" because of commuting (S7): moving on
   costs something. The exact test the engine applies is not sourced (**verify**); D-3 states this plan's rule and
   §7.1 measures why a weaker one fails.
6. **The search radius is `AcquireRadius` 10** (S1) [INF: that the ability's acquire radius bounds the search for
   another field, **verify**; it is the only radius on the ability and the data's only candidate].
7. **Occupancy is shared between players** on a mineral field (S10); only the extractor's owner can gather gas, so
   the question does not arise there.
8. **Saturation follows from 1 to 3** (S8: "the limiting factor becomes the time 'on' the mineral patch"): a field
   yields at most one load per `HarvestTime`, so `5 × 1,500 / 50 = 150` minerals a minute here (the game's theory
   150.75, S7), and an extractor `4 × 1,500 / 35 = 171.4` gas a minute (169.6, S7; the difference is the whole-tick
   rounding of `HarvestTime 1.981`, plan `2026-11-22-harvest-timing` §2.3).
9. **`WaitToReturn` is uninterruptible** (S1, no layer changes it, S2). An order given during it waits for the
   stage to end [INF from the flag's name and from this project's precedent for an uninterruptible stage, S11;
   whether the engine defers the order or refuses it is not in the data, **verify**].

### 2.3 Values

| Value | 5.0.14 | Here | Source |
|---|---|---|---|
| Acquire radius | `AcquireRadius 10` | 10.0 world units (one game unit is one cell, plan `2026-11-20-resource-footprints` D-1) | S1 :361 |
| Kinds a worker acquires | `ResourceAcquire Minerals 1` | `[Mineral]` | S1 :368 |
| Field cap | one gatherer, `HarvestTime` 50 ticks, 5 a load | 150 minerals a minute per field | §2.2 rule 8 |
| Extractor cap | one gatherer, 35 ticks, 4 a load | 171.4 gas a minute per extractor | §2.2 rule 8 |

---

## 3. Diagnosis

| # | Today | File, symbol |
|---|---|---|
| T1 | A worker that touches its node starts gathering at once (`gather_ticks = harvest.time_ticks`), whoever else gathers there | `crates/sim_core/src/economy/harvest.rs` `step_gather` (`Approach::Arrived`) |
| T2 | An exhausted node sends its worker to the nearest live node of the same kind within `NEXT_NODE_RADIUS` (12, unsourced, B-67), for minerals and gas alike | `harvest.rs` `NEXT_NODE_RADIUS`, `next_node`, `step_harvest` |
| T3 | A non-queued order clears the whole task leg, `return_ticks` included (`abandon_task_leg` → `fresh_leg`), so the wait ends at once (B-160) | `crates/sim_core/src/entity.rs` `commit_order`; `economy/mod.rs` `fresh_leg` |
| T4 | `HarvesterSpec` has no fields (plan `2026-11-22-harvest-timing` D-3); `harvester: Some(())` on the three worker rows | `crates/sim_types/src/commands.rs`; `crates/sim_core/src/catalog.rs` `HarvesterSpecDef`; `assets/catalog/units.ron` |
| T5 | Task iteration follows entity ID order and each task acts at once, so any rule that reads another unit's state during the steps would depend on who comes first | `economy/mod.rs` `update_unit_tasks` (module invariant "simultaneous workers resolve identically on every peer") |
| T6 | `UnitTask::wait_ticks` ("Ticks spent waiting for a site or for ground to clear") is hashed and used by construction and transport; harvesting never sets it | `entity.rs` `UnitTask`; `production/construction.rs`; `transport/orders.rs` |
| T7 | No shipped scenario, matrix battle, golden or bench row issues a `Harvest` order (as plan `2026-11-22-harvest-timing` T6, re-checked on `f399d403`). Harvesting is exercised by `harvest_income_tests`, `harvest_edge_tests`, `harvest_timing_tests`, `full_match_tests`, `player_view_tests`, client unit and integration tests, and the client's `skirmish_match_tests` (`TICK_LIMIT` 12,000, `crates/client/tests/support/skirmish_pilot.rs`; its pilot sends each worker to its own field) | — |

**Finding F-1 (no occupancy, measured).** On `f399d403` three workers on each showcase field earn 1,350 a minute
(§7.1 base, window count; `3 × 446.4` = 1,339), 12 % above the game's theoretical maximum of 1,206 and 18 % above its measured 1,140 (S7); three on
the near geyser earn 180 (window count), above the game's cap of 169.6.

**Finding F-2 (moving on to any free field churns, measured).** A prototype in which a worker that finds its field
occupied moves to the nearest unoccupied one, with or without a test that its own field frees sooner, never
settles: 446 to 496 moves in a 3,000-tick window with three workers a field, and the line earns 883 to 907 a
minute instead of 1,200 (§7.1). The cause is circular: a field is unoccupied about a quarter of the time while
its own workers walk, so a worker that moves there takes it from them, and they move on in turn. The game's
lines self-balance (S8), so its rule must look at more than the gatherer of the moment; D-3 adds the count of
workers assigned, and the same lines then earn exactly their derivation with no move after the warm-up.

---

## 4. Design decisions

### D-1. Occupancy is derived, not stored

- A node is **occupied** by the unit, alive, whose front order is `Harvest` on it and whose `task.gather_ticks > 0`.
  For a mineral field, units of every player count (S10); a geyser can be gathered only by the extractor's owner,
  so the rule is the same.
- No field records it. Every way a gatherer stops (the take sets `gather_ticks` to 0, a new order clears the leg,
  death, loading) frees the node by the same fact that ends the gather, so no stale occupant can exist. The rule
  is evaluated once per tick, in D-2's resolution, from the state the tick's task steps leave.
- **Released at the take.** `gather_ticks` reaches 0 on the take, before the `ReturnDelay` wait (S7, B-61's
  restatement), so the next worker in line gathers from that same tick.
- The **assigned** count of a node is the number of alive units whose front order is `Harvest` on it, wherever they
  are and whatever they carry (the game's harvester count, S4's `IdealHarvesterCount` display). It is derived in
  the same pass.

### D-2. The line: claims resolved after the task steps

- **Arrival.** An empty worker whose body touches its node (the existing contact test, `Approach::Arrived`) does
  not start gathering itself. It halts, sets `current_anim` `Idle`, adds 1 (saturating) to `task.wait_ticks` and
  records a **claim** `(node, id)` in a tick-local list (`EconomyWorkspace::claims`, cleared every tick, never
  hashed, retained buffer). It claims again on every tick it stays there.
- **Resolution** (new `SimWorld::resolve_resource_claims`, run at the end of `update_unit_tasks`, after every task
  step of the tick):
  1. Derive each node's occupant (D-1) and assigned count from the task units of the tick.
  2. Sort the claims by `(node, most wait_ticks first, lowest EntityId)`.
  3. For each node without an occupant, its first claimant starts gathering: `gather_ticks = time_ticks` of the
     node's kind, `wait_ticks = 0`, `current_anim = Cast`. The others keep waiting.
- **One worker alone is unchanged.** It touches the node, claims and is granted on the same tick, so its gather
  starts exactly when it does today: one worker per field gives byte-identical trips (§7.1, k = 1 rows), and its
  `wait_ticks` is 0 again before the tick's checksum.
- **Order-independence.** No step reads another unit's claim or occupancy while the steps run (T5): the take of a
  gatherer stepped late in ID order frees the node for a claimant stepped earlier, because the grant waits for the
  resolution. The winner is chosen by the stated key, never by iteration order. `wait_ticks` is hashed already
  (T6), so the line's order is part of the state every peer checks.
- **`wait_ticks` keeps its meaning.** "Ticks spent waiting for a site" is exactly a worker waiting at a resource;
  `fresh_leg` already clears it, so a new order, a move to another node or the end of the order leaves the line.
  Its doc gains "or for a resource to be free (plan `2026-11-23-harvest-occupancy` D-2)". It saturates at 65,535
  ticks (43 minutes); the lowest `EntityId` decides between two saturated waiters.

### D-3. Moving on: to a free field with at least two fewer workers

After the grants, each claimant that was not granted and whose node's kind its harvester **acquires** (D-4) may
move on:

- **Candidates:** nodes of the same kind, standing (`remaining > 0`) and gatherable by the player now
  (`gather_site`), within the harvester's `acquire_radius` of the worker's position, **unoccupied, not claimed this
  tick, and with `assigned(candidate) + 2 ≤ assigned(own node)`**.
- **Choice:** the nearest candidate by squared distance from the worker to the node's position, the lower node id
  on a tie.
- **Canonical order and bookkeeping.** The movers are decided one by one in the line's key (most `wait_ticks`,
  then lowest `EntityId`), and each decision moves one from the own node's assigned count to the target's and
  reserves the target for the rest of the pass. So six workers sent to one field go to six different fields, not
  all to the nearest. The key is explicit state, so the result does not depend on iteration order.
- **The move** is `replace_task_order(id, Harvest { target })`: a fresh leg, the worker walks to the new node and
  claims there. It keeps the new node as its own: after each delivery it returns there, as a game worker that has
  switched fields keeps mining its new one (S8).
- **Why "two fewer".** Moving one worker from a node with `a` assigned to one with `b` improves the balance only if
  `b + 1 < a`, that is `b + 2 ≤ a`; with `b + 1 = a` the move only swaps which field is short, which is F-2's churn.
  It reproduces the sourced cases: six on one field spread (S9: `6` against `0`); a third worker leaves for a field
  with one (S8: `3` against `1`) and stays among fields with two (S8, "unless they are also over-saturated"); a
  balanced line of two or three a field never moves (S8, "self-balance"). The exact engine test is not in the data
  (**verify**); §7.1 shows the rule reaching the derived income with no move in the window.
- **Not added: an "almost done" test.** S8's "(unless the worker is almost done harvesting from the field)" was
  prototyped as "move only if the candidate is reached, at full speed, before the own node frees". With D-3's
  balance test it changed no measured figure (§7.1, every line identical), so it is left out as unsourced
  complexity and recorded (§14).
- **Gas workers never move on** (D-4's `acquires` has no `Gas`); they wait at the extractor (S6).

### D-4. The acquire radius and kinds are the worker's data

- `HarvesterSpec` gains the harvest ability's two acquire fields (S1):

  ```ron
  // CAbilHarvest default (core AbilData:353-380), inherited unchanged by the three workers'
  // abilities: AcquireRadius 10 (game units = cells), ResourceAcquire Minerals 1 (no other kind).
  harvester: Some((acquire_radius: 10.0, acquires: [Mineral])),
  ```

  read into `HarvesterSpec { acquire_radius: SimScalar, acquires: ResourceKindSet }`. `ResourceKindSet` is a new
  `Copy` set over `sim_types::ResourceKind` (a bit per kind, `contains(kind)`), so `HarvesterSpec` stays `Copy`,
  `Eq` and `Hash`. `ResourceKind` is the engine's own two-valued enum, not a catalog identity, so charter §3.3's
  rule against named units is untouched.
- **Validation** (typed `CatalogInvalidValue`): `acquire_radius ≥ 0` and finite; `acquires` without duplicates. An
  empty list is valid (a worker that never moves on). Fields are required: a catalog without them fails to load.
- **Uses.** D-3's search, and the existing next node on exhaustion (`next_node`), which takes the radius and the
  kinds from the worker: a kind the worker does not acquire ends the order with `ResourceExhausted` when its node is
  empty, as an exhausted geyser's workers stop in the game (S6). `NEXT_NODE_RADIUS` is removed; **B-67** is fixed
  (its remaining part, the radius, is the data's).
- **Exhaustion prefers a free node.** `next_node` keeps its choice (the nearest live node), so an exhausted field's
  workers may arrive at an occupied field and then follow D-3 from there. This keeps one search rule.
- Where the radius is measured from (the worker, as today, or the exhausted node) is not in the data (**verify**);
  this plan keeps the worker's position.

### D-5. `WaitToReturn` holds the next order (B-160)

- In `Entity::commit_order`, before the leg is cleared, a **non-queued** order given to a unit whose front order is
  `Harvest` and whose `task.return_ticks > 0` sets `cast_lock_ticks = max(cast_lock_ticks, return_ticks)`. The leg
  is then cleared as today (so `return_ticks` is 0 and the cargo stays on the unit), and the new order runs as soon
  as the lock is down: `is_action_locked` already means "does not walk, attack, acquire or cast; its orders wait".
- The wait and the lock count down on the same ticks (the wait in phase 2b, the lock at the end of the tick), so
  the new order's first move is on the tick the delivery leg would have started (WP3 pins it by test against an
  uninterrupted twin).
- A queued (Shift) order is unchanged: it waits behind the `Harvest` order as today. `ReturnCargo` given during the
  wait is held like any other order and then delivers.
- `cast_lock_ticks`' doc is widened: "Ticks the unit stays locked by an uninterruptible stage: a cast's backswing
  (`CastOutroTime`, D-7.4) or the rest of a harvest's `WaitToReturn` that a new order cut into (plan
  `2026-11-23-harvest-occupancy` D-5)". The field keeps its name: renaming hashed state for a comment is churn.
- **Not changed:** task steps do not test `is_action_locked`. A held `Build` whose site is already in reach could
  place its foundation during the lock (at most 9 ticks, and a worker at a mineral field is not in reach of a
  site). Making every task respect the lock would change stunned workers too, which is a separate question (§14).

### D-6. Hashed state: none added; schema 33

- Occupancy and the assigned count are derived (D-1); the claims are tick-local (D-2); the line's order is
  `wait_ticks`; the carried wait is `cast_lock_ticks`. All hashed already.
- So `CHECKSUM_SCHEMA_VERSION` stays 33, and the goldens, which harvest nothing (T7), stay byte-identical with no
  re-record. `checksum_state_tests` needs no new mutation.
- **If an implementer finds a field necessary** (a stop of §8), it is an amendment: the schema takes the next free
  number at merge (34 on `f399d403`; a parallel contact-solver branch plans 34, so whichever merges second takes
  the next), both goldens are re-recorded under a "Re-recorded for plan 2026-11-23-harvest-occupancy WPn" header
  proven by the old encoding reproducing them, on native debug and release and `wasm32-wasip1` release and debug
  (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`), and `sim_fixtures`'
  `DEFAULT_SANDBOX_CHECKSUM` and the schema-byte tests of `checksum::encoding` move with it.

### D-7. Presentation

- A waiting worker publishes `Idle` (its `current_anim`) and stands at its contact point; the gatherer `Cast`, as
  today. No client code changes.
- **`HideHarvesters`** (S4): in the game the gas gatherer is hidden inside the extractor; here it stands at the
  extractor's edge. Presentation only, not hashed by this plan; registered (WP0, §14).

### D-8. Acceptance is a derivation, not a band fitted to a seed

See §6. The rule's effect follows from the data and the trip geometry: a field cannot yield more than one load per
`HarvestTime`, and a worker cannot trip faster than its legs allow. The measured income must equal that
derivation within the measure's resolution. The game's figures (S7, S8) check the shape: one and two workers a
field add linearly, three saturate a near field, a far geyser needs four. Seeds of battles are not used: nothing
in them harvests (T7), so they must be byte-identical.

### D-9. Records

- **B-61** fixed by WP2 (occupancy of fields and extractors, the line, moving on).
- **B-67** fixed by WP1 (the radius is `AcquireRadius`).
- **B-160** fixed by WP3.
- New, registered by WP0 with the next free numbers (B-161 and B-162 on `f399d403`):
  - "The gas gatherer stands at the extractor's edge; 5.0.14 hides it inside (`HideHarvesters`, liberty
    BehaviorData:817–852). Presentation only."
  - "The next worker in a resource's line starts on the tick the previous one takes, with no repositioning; the
    game loses about 5 % of a saturated field to it (three workers 143 a minute against the theoretical 150.75,
    Liquipedia Resources) and about 4 % of a saturated geyser (163 against 169.6). Here a saturated field earns 150
    and an extractor 171.4." Minor; the hand-off's length is not sourced.

---

## 5. Determinism and checksum

- **Integer arithmetic only.** The radius is read once from RON into `SimScalar` (an authored value, as every
  catalog distance); distances compare in raw fixed point (`within_distance`, `squared_distance`); counts are
  integers. No float is added to the simulation crates.
- **Order.** D-2's resolution and D-3's decisions are keyed by hashed state (`wait_ticks`, `EntityId`, node id),
  never by iteration order; the claims list is cleared each tick.
- **Hashed state:** unchanged (D-6). Schema 33.
- **Goldens:** not re-recorded; every package checks both on native debug and release and `wasm32-wasip1` release
  and debug.
- **`sim_map` pins:** unchanged (no `sim_map` change).
- **wasm.** No new dependency; the simulation crates still build for `wasm32-unknown-unknown`.
- **Cost.** The resolution runs only on a tick with claims: one pass over the tick's task units (already collected
  in `EconomyWorkspace::ids`), a sort of the claims, and for each mover a scan of the map's nodes. All buffers are
  retained in `EconomyWorkspace` (`// PERFORMANCE RATIONALE:` on them). The throughput bench does not harvest.

---

## 6. Expected income (derived before measuring)

### 6.1 Minerals

Each field `f` has its one-worker trip `T_f` in ticks: the two walking legs plus `H` = 50 gathering plus
`D` = 9 waiting (plan `2026-11-22-harvest-timing` §6.1). The trip and the legs are not changed by this plan; they
are measured on the base as the field's take period with one worker. A field is busy `H` ticks per trip. With
`k` workers on it, in a closed cycle of `k` workers through one server of fixed service time `H` and a fixed walk,
the steady take rate is the smaller of what the walk allows and what the server allows:

`R_f(k) = min(k × 5 × 1,500 / T_f, 5 × 1,500 / H) = min(k × 7,500 / T_f, 150)` minerals a minute.

- No worker waits in steady state while `T_f ≥ k × H`; above that the field is busy every tick.
- `T_f` is 121 to 147 ticks on the showcase's starts and 121 to 176 on `default_arena` slot 1, so with `k` = 2 no
  field is saturated (`2 × 50 = 100 ≤ 121`) and with `k` = 3 every showcase field is (`150 > 147`), while
  `default_arena`'s fields of 153 and 176 ticks are not.
- **A balanced line does not move** (D-3: equal counts), so its income is the sum of `R_f(k)`.
- A **split**: `n ≤ 8` workers sent to one field of a line end one on each of `n` fields, each earning its
  one-worker rate (the bank earns the sum of those fields' one-worker rates).

| Line | `k` = 1 | `k` = 2 | `k` = 3 | `k` = 4 |
|---|---:|---:|---:|---:|
| Showcase slot 0, 8 fields, `T` 137, 121, 123, 137, 147, 135, 147, 133 | 446.4 | 892.9 | **1,200.0** | 1,200.0 |
| Per worker | 55.80 | 55.80 | 50.00 | 37.50 |
| `default_arena` slot 1, 8 fields, `T` 129, 121, 123, 143, 139, 135, 153, 176 | 434.7 | 869.4 | **1,174.9** (fields of 153 and 176: 147.1 and 127.8) | 1,200.0 |
| Per worker | 54.34 | 54.34 | 48.95 | 37.50 |

Showcase slot 1 is slot 0's mirror (the same `T`).

### 6.2 Gas

`R(k) = min(k × 4 × 1,500 / T, 4 × 1,500 / 35) = min(k × 6,000 / T, 171.43)`; no geyser's worker moves on.

| Geyser (trip `T`) | `k` = 1 | 2 | 3 | 4 |
|---|---:|---:|---:|---:|
| Showcase near (10, 1) and arena slot 1 (50, 56), `T` 99 | 60.61 | 121.21 | **171.43** | 171.43 |
| `default_arena` slot 0 (13, 12), `T` 127 | 47.24 | 94.49 | 141.73 | **171.43** |
| Showcase corner (1, 1), `T` 135 | 44.44 | 88.89 | 133.33 | **171.43** |

The game's shape (S6, S8): three workers saturate a geyser at the usual distance, a far one needs four.

### 6.3 Acceptance (fixed before measuring)

- **A-1, the curve.** For every line and `k` of §6.1 and every geyser and `k` of §6.2, the measured take-period
  rate (loads taken over the span from a node's first take in the window to its last, summed over the line's
  fields) equals the derived rate within one load per field over the window: ±2.5 minerals a minute per field
  (±20 for a line of eight), ±2.0 gas a minute per geyser, for a 3,000-tick window after a 1,500-tick warm-up
  (minerals) or 750 (gas). Saturated cases are exact: a field busy every tick takes every 50 ticks.
- **A-2, stability.** A balanced line (`k` = 1 to 4 on every field) makes **no** move in the window.
- **A-3, the split.** Six workers sent to one showcase field end on six different fields, each at its one-worker
  rate within A-1's tolerance.
- **A-4, one worker is unchanged.** `k` = 1 on every field and geyser gives the base's take periods to the tick.
- **A-5, the game's shape.** Per patch, two workers earn twice one (within A-1) and three earn the field cap on
  every showcase field; the near geyser saturates with 3, the corner geyser only with 4 (S6, S8). The game's
  absolute figures (S7) are reported beside the result, not banded: the saturated figures differ from them by the
  hand-off gap this plan does not model (D-9, second row), which is a registered difference, not a tolerance.

The derivation (§6.1, §6.2) and A-1 to A-5 were written before the candidates were measured. D-3's balance test
was adopted after F-2's measurement showed the first rule failing A-1 and A-2; the derivation and the acceptance
were not changed for it.

---

## 7. Prototype and measurements

Prototype: a scratch worktree of `f399d403` (`.claude/worktrees/harvest-occupancy-proto`,
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/harvest-occupancy-proto`, removed at hand-back). Its diff is kept
at `~/scratch/harvest-occupancy/proto/proto.diff`. Switches, so one binary is base and candidates:

- `PROTO_OCC=1`: D-1 and D-2 (claims, resolution), and gas never moves on or acquires on exhaustion; radius 10
  (`PROTO_R`). Without it the code is `f399d403`'s.
- `PROTO_BAL=1`: D-3's balance test and its canonical, bookkept order. Without it a claimant moves to the nearest
  unoccupied, unclaimed node (F-2's rule).
- `PROTO_ETA=1`: the "almost done" test (D-3, not adopted).
- `PROTO_B160=1`: D-5.

It did not move the radius and kinds into the catalog (D-4): that changes where two numbers are read, not what they
are; WP1 proves it bit-identical on its own.

Tool: `~/scratch/harvest-occupancy/proto/harvest_occ.rs` (copied into `crates/sim_core/examples/` of the scratch
tree, not committed). `harvest_occ <map> <slot> <mineral|gas> <line|node> <k> <warm> <window> <radius>`:
- `line`: one world with the start's hall (and an extractor on every geyser for gas) and `k` workers ordered to each
  node within the radius; per node its take-period rate from the node's own drawdown, so a moved worker is counted
  where it gathers; the bank's income; the moves in the window.
- `node`: a fresh world per node with `k` workers on that node; income, worker 0's trip, the take-period rate.
Scripts: `income.sh`, `minerals.sh` beside it; results in `~/scratch/harvest-occupancy/proto/income/`.

### 7.1 Minerals

Take-period rate of the line, minerals a minute; moves in the 3,000-tick window in brackets. The base column is the
window count (the bank's drawdown), because without occupancy several workers take on one tick and the take period
of a field is then not defined.

| Line, `k` | Derived (§6.1) | Base (`f399d403`) | F-2's rule (`OCC`) | F-2 with "almost done" (`OCC ETA`) | **D-3 (`OCC BAL`)** | D-3 with "almost done" |
|---|---:|---:|---:|---:|---:|---:|
| Showcase 0, 1 | 446.4 | 446.44 (452.5 counted) | 446.44 (0) | 446.44 (0) | **446.44 (0)** | 446.44 (0) |
| Showcase 0, 2 | 892.9 | 902.5 | 843.13 (63) | 848.74 (50) | **892.62 (0)** | 892.62 (0) |
| Showcase 0, 3 | 1,200.0 | 1,350.0 (no cap) | 883.43 (496) | 907.31 (446) | **1,200.00 (0)** | 1,200.00 (0) |
| Showcase 0, 4 | 1,200.0 | 1,800.0 (no cap) | 903.73 (1,014) | 999.16 (895) | **1,200.00 (0)** | 1,200.00 (0) |
| Showcase 1, 2 | 892.9 | — | — | — | **890.78 (0)** | 890.78 (0) |
| Showcase 1, 3 | 1,200.0 | — | — | — | **1,200.00 (0)** | 1,200.00 (0) |
| Arena 1, 1 | 434.7 | 434.69 | 434.69 (0) | 434.69 (0) | **434.69 (0)** | 434.69 (0) |
| Arena 1, 2 | 869.4 | 870.0 | 844.34 (33) | 808.15 (60) | **871.76 (0)** | 871.76 (0) |
| Arena 1, 3 | 1,174.9 | 1,302.5 (no cap) | 832.65 (488) | 949.87 (362) | **1,174.80 (0)** | 1,174.80 (0) |
| Arena 1, 4 | 1,200.0 | 1,732.5 (no cap) | 938.29 (897) | 994.37 (780) | **1,200.00 (0)** | 1,200.00 (0) |

- **A-1:** every D-3 line within 2.4 of its derivation (bound 20); the saturated lines exact. **A-2:** no move in
  any window. **A-4:** `k` = 1 identical to the base on both maps.
- **Per field**, showcase 0, `k` = 2 (D-3): 108.81, 124.41, 121.95, 108.81, 102.04, 111.79, 102.04, 112.78 against
  derived 109.49, 123.97, 121.95, 109.49, 102.04, 111.11, 102.04, 112.78 (each within one load).
- **A-3, the split** (showcase 0, `node` mode, six workers sent to each field in turn): 6 to 8 moves, and the
  per-worker income 54.6 to 57.1, the one-worker rates of the fields they settle on (mean 55.80). Two workers sent
  to one field: one moves (1 move), and the bank earns the two fields' one-worker rates (for example (3, 14):
  105.0 counted against 54.74 + 51.02 for the neighbour (4, 15) it moved to).
- **A-5:** per field one 55.8, two 111.6, three 150 on every showcase field (the game's 55–60, 110–120, 143).
  Reported against the game: the line with two a field 892.6 against 925 (−3.5 %); with three 1,200 against the
  measured 1,140 (+5.3 %, the hand-off of D-9's second row) and the theoretical 1,206 (−0.5 %).
- **F-2** is the base/`OCC` columns: moving on to any unoccupied field churns; the "almost done" test damps it but
  does not stop it, and adds nothing once D-3 holds (identical last two columns).

### 7.2 Gas

`node` mode, take-period rate per geyser, gas a minute. The base (no cap) earns `k` times one: the near geyser
60, 120, 180, 240 counted with 1 to 4 workers, the corner 44, 88, 132, 176.

| Geyser, `T` | `k` = 1 derived / measured | 2 | 3 | 4 |
|---|---|---|---|---|
| Showcase (10, 1), 99 | 60.61 / 60.61 | 121.21 / 121.21 | 171.43 / **171.43** | 171.43 / 171.43 |
| Showcase (1, 1), 135 | 44.44 / 44.44 | 88.89 / 88.89 | 133.33 / 133.33 | 171.43 / **171.43** |
| Arena 0 (13, 12), 127 | 47.24 / 47.24 | 94.49 / 95.44 | 141.73 / 141.73 | 171.43 / **171.43** |
| Arena 1 (50, 56), 99 | 60.61 / 60.61 | 121.21 / 121.21 | 171.43 / **171.43** | 171.43 / 171.43 |

- **A-1:** exact but one (arena 0, `k` = 2: +0.95, one load's phase; bound 2.0). Identical with every switch set
  (`OCC`, `OCC ETA`, `OCC BAL`): gas never moves on.
- **A-5:** the near geysers saturate with 3, the corner geyser and arena 0 only with 4 (S6, S8). Reported: 3
  workers on a near geyser 171.4 against the game's measured 163 (+5.2 %, D-9's hand-off row) and its cap 169.6
  (+1.1 %, the whole-tick `HarvestTime`).
- Worker 0's trip with three on a near geyser: 105 ticks (`3 × 35`), the extractor busy every tick.

### 7.3 Worlds that do not harvest

Base (no switches) against the candidate (`PROTO_OCC=1 PROTO_BAL=1 PROTO_B160=1`), the same binary; the commands
of resource-footprints D-13 (`~/scratch/resource-footprints/scripts/measure.sh`, run by `world.sh`), output under
`~/scratch/harvest-occupancy/proto/world/`.

| Run | Result |
|---|---|
| Matrix, every battle, seeds of record and 16 | Byte-identical (every output file; the logs differ only in their output path) |
| Battle 31, 100 seeds | Byte-identical |
| Battle 33, 64 seeds | Byte-identical |
| Showcase, default seed and seeds 1 to 8, 6,000 ticks | Byte-identical `units.csv` and summaries: player 1 wins at 2622, 2537, 2544, 2180, 2601, 1939, 2192, 2530, 2154 (as plan `2026-11-22-harvest-timing` §7.3) |

408 output files compared, none differs; 12 logs differ only in the path they name. Nothing in these worlds
harvests (T7), and D-5 acts only on a worker waiting with cargo, so nothing could move: they are the control that
the rule adds nothing where no worker gathers.

### 7.4 Goldens and tests

Native release, the prototype tree (`tests.sh`, logs in `~/scratch/harvest-occupancy/proto/golden/` and
`sim_core_tests.log`):

| Build | Result |
|---|---|
| `golden_replay_tests`, no switches (the base) | 4 of 4 pass: the prototype is a clean control |
| `golden_replay_tests`, candidate (`OCC BAL B160`), schema 33 unchanged | 4 of 4 pass: no re-record (D-6) |
| `cargo test --release -p sim_core --no-fail-fast`, candidate | **1,052 passed, 0 failed** |

No existing test fails: one worker per node is unchanged (A-4), and the tests with several workers
(`workers_walk_through_each_other_while_harvesting`: five on a field with a second field 2 away) still reach their
income. So every test of WP2 is new and red first. `a_new_order_during_the_wait_drops_it_and_keeps_the_cargo`
passes under D-5 because it asserts only the cleared countdown and the kept cargo; WP3 replaces it with the held
order's test. WP2 and WP3 repeat the goldens on native debug and release and `wasm32-wasip1` release and debug.

### 7.5 The client's skirmish match

`cargo test --release -p client --test skirmish_match_tests -- --nocapture` on the prototype, radv
(`WGPU_BACKEND=vulkan`, `VK_ICD_FILENAMES=…/radeon_icd.json`; the test locks the GPU per child), base then
candidate (`OCC BAL B160`); logs in `~/scratch/harvest-occupancy/proto/skirmish/`.

| Test | Base | Candidate |
|---|---|---|
| `a_match_is_won_through_real_input_alone` | **Victory at tick 6,486** | **Victory at tick 6,486**; every stage on the same tick (Mine 0, Raise 22, Site 550, Foundation 586, Train 642, Group 4,502, March 4,526, the three assaults 4,834, 5,098, 5,362) |
| The other 6 (1 ignored by design) | pass | pass |

- The match is far inside `TICK_LIMIT` 12,000, and its stage log is identical tick for tick: the pilot sends each
  starting worker to its own field (T7), so whatever sharing its later workers meet does not move any stage.
- `the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window` logged its `Done` at tick 116 on the first
  candidate run against 115 on the base; reruns (`base-hud-reruns.log`, `cand-hud-reruns.log`): base 115 and 115,
  the full candidate 115, `OCC BAL` alone 115, `B160` alone 116. It is a windowed child whose stage ticks follow
  its frames, so the one tick is run-to-run variance of that test, not the rule (nothing in it waits with cargo
  or shares a field). WP2's and WP3's package gates run it as usual.

---

## 8. Stop rules and acceptance

1. **WP1 (data) is bit-identical everywhere.** With `acquire_radius: 12.0` and `acquires: [Mineral, Gas]` authored
   first (today's behaviour), every world and test is unchanged: matrix, battles 31 and 33, the showcase's nine
   `units.csv`, both goldens on native release and `wasm32-wasip1` release, every harvest test. Then the row takes
   the data's values (10, `[Mineral]`) in the same package, and only the tests of exhaustion that name a geyser or a
   field between 10 and 12 away may move (none on `f399d403`: the prototype's suite, which has the radius 10 and
   minerals only, passed whole, §7.4); any other difference stops the package.
2. **WP2 (occupancy) moves the expected worlds and nothing else.**
   - Matrix, battles 31 and 33, the showcase on nine seeds: byte-identical to WP1 (§7.3). A difference is a defect.
   - Goldens: pass unchanged on four builds (D-6). If one moves, the package stops: nothing in them harvests.
   - Income: A-1 to A-5 of §6.3 on WP2's tree, with `T_f` measured on WP1's tree (`k` = 1). A miss is traced (the
     field's take times, the line's claims) before anything changes; an unexplained miss stops the package.
   - Every changed harvest test is re-derived from a trace of the new rule; a failure not listed in §7.4 is a
     finding and stops the package. No bound is loosened, and no constant is chosen to make a test pass.
   - The skirmish match: `Victory` within `TICK_LIMIT` (the pilot's own limit, unchanged), its tick reported
     against WP1's. If it misses, the package stops and the trace (bank over time, the stage the pilot waits in)
     goes to the orchestrator; the limit is not raised without an amendment.
3. **WP3 (B-160)** changes no world that does not interrupt a waiting worker: §7.3's set byte-identical, goldens
   unchanged.
4. **No tuning constant** is introduced. The radius and kinds are the data's (§2.3); the balance margin of D-3 is
   the smallest that improves a balance (`b + 1 < a`), not a fitted number.

---

## 9. Client consequences

None in code (D-7). The client tests that harvest (`battle_card_path_tests::gas_through_the_card_extractor_snap_harvest_and_income`,
`battle_context_order_tests`, `battle_fog_tests`, `battle_hud_tests`, `battle_pipeline` unit tests,
`skirmish_match_tests`, `sync_steady_state_tests`, `battle_flipbook_tests`) run in WP2's package gate. A test that
waits for income with a tick bound is re-derived from §6 if it fails (one worker per node is unchanged, A-4). The
skirmish pilot sends each starting worker to its own field (T7); §7.5 found its match unchanged on the
prototype, stage for stage.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP4 | Two rows registered (D-9); B-61, B-67, B-160 fixed; the Economy row |
| `docs/plans/README.md` | plan, WP4 | Row `Proposed` with the plan; `Done` in WP4 |
| `assets/catalog/units.ron` | WP1 | `harvester: Some((acquire_radius: …, acquires: […]))` on the three worker rows, with D-4's comment |
| `crates/sim_types/src/commands.rs` (or a new `resource_kind_set.rs` in `sim_types`) | WP1 | `HarvesterSpec { acquire_radius, acquires }`; `ResourceKindSet` with unit tests |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs` | WP1 | `HarvesterSpecDef`, its validation |
| `crates/sim_core/src/economy/harvest.rs` | WP1 (radius, kinds), WP2 (claims, resolution, moving on) | `next_node` from the spec; `NEXT_NODE_RADIUS` removed; `resolve_resource_claims` |
| `crates/sim_core/src/economy/mod.rs` | WP2 | `EconomyWorkspace` buffers; the resolution called at the end of `update_unit_tasks`; module doc |
| `crates/sim_core/src/entity.rs` | WP2 (`wait_ticks` doc), WP3 | `commit_order`'s carried wait; `cast_lock_ticks` doc |
| `crates/sim_core/tests/harvest_occupancy_tests.rs` (new), `harvest_income_tests.rs`, `harvest_edge_tests.rs`, `harvest_timing_tests.rs`, `runtime_catalog_tests.rs`, `catalog_reference_values_tests.rs` | WP1 to WP3 | §12 |
| `docs/evidence/2026-11-23-harvest-occupancy.md` | WP2, WP4 | Figures of §6.3 on the as-built trees |

Nothing in `crates/sim_core/src/movement/` changes: the parallel movement work there does not overlap (R-4).

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR`; the implementation agent runs
`cargo gate quick` and the tests its change needs and hands back; a verifier runs `cargo gate package` (Q0061).

### WP0: records (S, docs only)

- `DESIGN.md` §4: the two rows of D-9 registered with the next free numbers; B-61, B-67 and B-160 point to this
  plan.
- **Gate:** docs-only `package`.

### WP1: acquire radius and kinds as data (M, `sim_types`, `sim_core`, catalog)

Failing tests first (red on `main`):
- `sim_types`: `ResourceKindSet` unit tests (empty, insert, `contains`, from a list, duplicates refused by the
  catalog, not by the set).
- `catalog` unit tests: a harvester row without `acquire_radius` is refused (`RonDeserializationError`); a
  negative radius and a duplicate kind are refused (`CatalogInvalidValue`, naming the field); an empty `acquires`
  is accepted.
- `runtime_catalog_tests`: a copy of the shipped catalog with the worker's `acquire_radius` set below the distance
  to the second field loads in the same binary, and an exhausted field's worker then ends its order instead of
  moving on (charter §3.3, zero recompilation).
- `harvest_income_tests::an_exhausted_geyser_ends_the_order` (new): two geysers with extractors 6 apart; the first
  runs dry; its worker's order ends with `ResourceExhausted` and it does not walk to the second (S6).
- `catalog_reference_values_tests::the_shipped_harvesters_acquire_as_5_0_14`: radius 10, `[Mineral]`, with S1's
  lines.

Then the change: D-4. First commit with today's values (`12.0`, `[Mineral, Gas]`), proven bit-identical (§8 rule
1); second commit with the data's values.

### WP2: occupancy, the line and moving on (M, `sim_core`)

Failing tests first (red on WP1's tree), in a new `crates/sim_core/tests/harvest_occupancy_tests.rs`, every time,
amount and radius read from the catalog, never written as literals:
- **`one_worker_gathers_a_field_at_a_time`:** two workers on one field of a lone-field map; while one has
  `gather_ticks > 0` the other's is 0 and it stands touching the field with `wait_ticks` counting.
- **`the_next_worker_starts_on_the_tick_of_the_take`:** the waiting worker's `gather_ticks` becomes the row's
  `time_ticks` on the tick the gatherer's cargo appears (S7, D-1).
- **`the_line_is_first_come_first_served`:** three workers arrive on three different ticks; they gather in arrival
  order; two arriving on one tick gather lower `EntityId` first.
- **`the_grant_does_not_depend_on_entity_order`:** the same two-worker scene built twice with the roles' ids
  swapped (the gatherer spawned first, then second): the waiter starts on the same tick in both (D-2, T5).
- **`an_enemy_worker_occupies_a_field`:** a worker of player 2 gathering blocks player 1's worker (S10).
- **`one_worker_at_a_time_in_an_extractor_and_gas_workers_wait`:** three workers on a near geyser with a second
  extractor free nearby: they never move to it, and the geyser's take period is `time_ticks` once saturated.
- **`workers_sent_to_one_field_spread_over_the_line`:** six workers to one showcase field: they end on six distinct
  fields (A-3).
- **`a_balanced_line_stays_put`:** two, then three, a field on the showcase line: no `Harvest` target changes after
  the warm-up (A-2).
- **`a_worker_moves_only_to_a_field_two_fewer`:** fields with assigned counts (3, 2) do not exchange; (3, 1) does
  (D-3).
- **`moving_on_respects_the_acquire_radius`:** a free field beyond `acquire_radius` is never chosen.
- **`saturation_follows_the_derivation`** (`harvest_edge_tests`, beside `income_lies_in_the_band`): the showcase
  slot-0 line at `k` = 2 and 3 against §6.1's `R_f(k)` from the one-worker take periods measured in the same test
  (A-1); the near geyser at 3 equals `amount × 1,500 / time_ticks` exactly.
- Existing tests re-derived where §7.4 lists them.

Then the change: D-1, D-2, D-3.

**Acceptance:** §8 rule 2. Measure §6.3 on WP1's tree (`T_f`) and WP2's with the scratch tool of §7, record both
in the evidence.

### WP3: the held order (S, `sim_core`)

Failing tests first (red on WP2's tree), in `harvest_timing_tests.rs`:
- **`a_new_order_during_the_wait_starts_after_it`** (replaces `a_new_order_during_the_wait_drops_it_and_keeps_the_cargo`):
  a `Move` given during the wait: the worker's position is unchanged until the tick an uninterrupted twin starts
  its delivery leg, and moves from that tick; the cargo stays; a later `Harvest` delivers first.
- **`a_queued_order_during_the_wait_is_unchanged`:** a Shift `Move` lands behind the `Harvest` order and nothing
  else changes (the twin's trip to the tick).
- **`return_cargo_during_the_wait_is_held_then_delivers`.**

Then the change: D-5. **Acceptance:** §8 rule 3.

### WP4: bookkeeping (S)

- `DESIGN.md`: B-61, B-67 and B-160 fixed (commits, the figures of §6.3 as built); the Economy row says one worker
  at a time per resource, with this plan's link; the evidence column adds `harvest_occupancy_tests.rs`.
- `docs/evidence/2026-11-23-harvest-occupancy.md` completed; this plan's `## As built`; README row `Done`.
- **Gate:** docs-only `package`.

---

## 12. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Data | Radius and kinds read at run time, validated, sourced | WP1: catalog unit tests, `runtime_catalog_tests`, `catalog_reference_values_tests` |
| Rule | One gatherer, the line's order, release at the take, order-independence, shared occupancy, gas waits | WP2: `harvest_occupancy_tests` |
| Spreading | The split, a balanced line stays, the two-fewer test, the radius | WP2: `harvest_occupancy_tests` |
| Income | The derived saturation curve, the extractor cap | WP2: `harvest_edge_tests::saturation_follows_the_derivation` |
| Held order | The wait holds a new order to the tick; Shift unchanged | WP3: `harvest_timing_tests` |
| Determinism | No hashed state added; goldens unchanged on four builds; matrix byte-identical | §8 |
| Client | The skirmish match still won within its limit | WP2's package gate |

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The skirmish pilot's later workers crowd fields and its income falls below what its stages expect | WP2 stops | §7.5: on the prototype the match is won on the same tick with an identical stage log; D-8: a change is reported with its trace, the limit not raised without an amendment |
| R-2 | The engine's moving-on test differs from D-3 (S8 is community wording) | Spreading differs in detail (which field, when) | The rule reproduces every sourced case and the sourced shape (§7.1); recorded **verify**; a later source is an amendment |
| R-3 | `ResourceAcquire` does not govern moving on or the next resource on exhaustion | Gas workers' behaviour on a dry geyser | S6 (Blizzard) states the dry-geyser behaviour directly; gas never moving on matches S6's "queued up waiting" |
| R-4 | The parallel contact-solver work (`contact-solver-*` branches) bumps the schema or re-records the goldens | None for this plan (no schema change, no re-record) | If an amendment adds hashed state, D-6's next-free rule applies; this plan touches no file under `movement/` |
| R-5 | Waiting workers stand in each other's way at a field's contact points | Longer legs on crowded fields | Harvesting workers already walk through each other (`is_mineral_walking`), and waiting is part of the order, so the flag stays set; §7.1's lines meet their derivations |
| R-6 | A test bounds a wait for income in ticks with several workers on one node | A red test in WP2's gate | Re-derived from §6 (D-8), never loosened without the derivation |
| R-7 | `wait_ticks` saturates in a very long wait | Two saturated waiters ordered by id | 43 minutes of waiting at one node; stated in D-2 |

---

## 14. Out of scope and decisions recorded (Q0002)

- **The "almost done" test** of S8: prototyped, no effect once D-3 holds (§7.1); left out.
- **The hand-off gap** between two gatherers (registered, D-9): its length is not sourced.
- **`HideHarvesters`** (registered, D-9): presentation.
- **The MULE's second queue** (`ResourceQueueIndex 1`, `BypassResourceQueue`, S3): no catalog unit has it. A plan
  that adds one adds a queue index to `HarvesterSpec`.
- **`EnabledSearchRadius` and `EnabledSearchFilters`** (S5): their meaning is not in the data.
- **`IdealHarvesterCount`** (S4): the town hall's saturation display, a client feature.
- **Task steps respecting the action lock** (D-5): it would change stunned workers; a separate question.
- **Where the acquire radius is measured from** (D-4, **verify**): the worker, as today.
- **Choosing a nearer field on exhaustion when the nearest is occupied** (D-4): one search rule, D-3 applies on
  arrival.

---

## 15. Prototype material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,voidmulti,balancemulti}/{AbilData,BehaviorData}.xml`
  (`5444e12f13`), read as data. Liquipedia `Resources`, `Mining Minerals` and `Worker split` wikitext through the
  MediaWiki API on 2026-11-23, saved in `~/scratch/harvest-occupancy/web/`; the Blizzard guide read on the same
  day.
- Prototype diff: `~/scratch/harvest-occupancy/proto/proto.diff`; the measure `harvest_occ.rs`, `income.sh`,
  `minerals.sh`, `world.sh`, `tests.sh` beside it; binaries in `~/scratch/harvest-occupancy/bin-proto/`.
- Results: `~/scratch/harvest-occupancy/proto/income/` (`base-`, `occ-`, `eta-`, `bal-`, `baleta-` CSVs per map,
  slot, kind, mode and `k`), `world/`, `golden/`, `sim_core_tests.log`, `skirmish/`.
- The scratch worktree `harvest-occupancy-proto` and its target directory were removed at hand-back; the diff
  rebuilds the prototype on `f399d403`.
