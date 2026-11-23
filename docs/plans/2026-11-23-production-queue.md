# Blueprint: Supply at the Start of Production, Rally Points and Placed Exits (B-59, B-60, B-66)

This plan gives production StarCraft II 5.0.14's queue, rally and exit rules:

- **Only the job in production holds supply (B-59).** A job waiting in a queue has paid its minerals and gas but
  holds no supply. It takes its supply when it reaches the front and starts; if the supply is not there, it waits
  at the front with its countdown not started (the data's `WaitForFood`). A job admitted to an empty queue starts
  at once and so needs its supply at admission, as today.
- **Production structures have a rally (B-60).** A new `SetRally` order stores a point, a unit or a resource node
  on the structure. A unit that leaves it gets the order its rally means: a move to the point, a worker rallied to
  a resource harvests it, a unit rallied to a friendly carrier boards it, and a unit rallied to another unit moves
  to where that unit stands. A town hall rallies itself to a mineral field when it is created (the data's
  `CommandStructureAutoRally`). The client sets a rally by right-clicking with a production structure selected and
  draws it while the structure is selected.
- **A blocked exit no longer stalls production (B-66).** When no cell of the first exit ring is free, the unit is
  placed at the nearest free cell on the structure's level, searched ring by ring outward, bounded by the map. With
  a rally, exits are tried nearest the rally point first.

A full plan is required (charter §6): a new mechanic (rally), a change of the checksum format (two hashed fields),
golden re-records, and work across `sim_types`, `sim_core`, the catalog and the client.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`4af9f51f`.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## Amendment 1 (orchestrator, 2026-11-23): a structure alone takes a right click as its rally

`cargo gate package` on `e5f83766` failed `battle_input_tests::a_structure_alone_reports_why_it_cannot_move`: with
only the base selected, a right click on the ground used to be refused ("Order rejected") and now submits `SetRally`.
The new behaviour is the one D-10 states ("a structure-only selection submits only the rally") and S7 describes, so
the test is restated, not the code:
- `a_producer_alone_takes_a_right_click_as_its_rally`: a structure that trains units, selected alone, takes one
  `SetRally` to the clicked point, marked as a rally.
- `a_structure_alone_reports_why_it_cannot_move` keeps its refusal for a structure that trains nothing (every
  structure of the live battle's base trains something, so the test places the faction's smallest one that does
  not): D-3 gives a rally only to structures that train units, so the click is still a `Move` no recipient can
  take, refused with its reason.

The test was missed because `cargo gate quick` keeps only `fast` targets (`narrow_to_quick` in `crates/gate/src/select.rs`)
and `battle_input_tests` and `battle_context_order_tests` are `Slow`; no path rule can add them to `quick`, and
`package` already selects them through the `client` crate. WP5's own run of the client targets did not include
`battle_input_tests`. Reclassifying them is not this plan's to decide.

## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations. S-1 covers RON data instead of editor data (the auto-rally radius and kinds) | Closed | D-7 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0002: scoping is the agent's call if written down | Standing rule | §14 |
| The owner's guidance for this plan: no fitting to fixed seed sets; judge on principled properties | Followed | §6: every acceptance is a property of the rule, never a seed's figure |
| B-58 (no queue limit) is class (ii): the owner allows more than five slots | Standing (plan `2026-11-05-sc2-deviation-fixes`) | D-2 notes the interaction; out of scope |
| **None new** | The data and the sources decide every rule; the unsourced details are recorded decisions and registered rows (D-11) | §4, §14 |

---

## 1. Summary

| | Today (`4af9f51f`) | After this plan |
|---|---|---|
| Supply of a queued job | Every job reserves its full supply at admission (`ProductionJob::reserved_supply`), so a queue of five Marines holds 5 | Only the job in production holds supply; the jobs behind it hold 0 |
| Supply short at the front | Cannot happen: admission refused the job | The front job waits, its countdown not started, and the structure publishes `SupplyWait` |
| Admission and supply | Refused if the full reservation exceeds the cap | Refused only if the job would start at once (empty queue) and its supply exceeds the cap; a job behind others is not checked |
| Rally | None; a new unit stands at its exit | `UnitOrder::SetRally { target }`; the new unit moves, harvests, boards or follows by snapshot (D-4, D-5) |
| Town hall rally | — | Set on creation to the farthest mineral field within 6.5 (data, D-7) |
| Exit when the ring is full | The job waits at 0 ticks, holding its supply, until a ring cell frees (`ProductionBlock::ExitObstructed`) | The nearest free cell on the producer's level, ring by ring to the map's extent; `ExitObstructed` only when the whole map has none |
| Exit order | Clockwise from the ring's minimum corner | The same without a rally; nearest the rally point first with one |
| Hashed state | — | `ProductionJob::in_production`, `Entity::rally`: schema **next free at merge** (34 on `4af9f51f`; the contact-solver branch plans 34, so whichever merges second takes the next) |
| Worlds that move | — | The two goldens by their bytes only (WP1, proven); every package keeps the matrix, battles 31 and 33 and the showcase on nine seeds byte-identical; the skirmish match expected stage for stage (§7) |
| Size | — | L: seven packages, four of them M |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`), read as data from `~/scratch/combat_wp3_data/<mod>/`: "core Abil:N"
  is line N of `core/AbilData.xml`, "liberty Unit:N" of `liberty/UnitData.xml`, "voidmulti Effect:N" of
  `voidmulti/EffectData.xml`, and so on. Nothing there was executed.
- **[BLIZ]** Blizzard's own documentation.
- **[COMM]** Community documentation.
- **[INF]** An inference of this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | core Abil:1028–1036 (the `CAbilTrain` default, which every structure's train ability inherits: `BarracksTrain`, `FactoryTrain`, `StarportTrain`, `GatewayTrain`, `CommandCenterTrain`, `NexusTrain`, liberty Abil:1566–1670, 1990–2060) | **`Flags WaitForFood 1`** (:1035); also `BestUnit`, `IgnoreUnitBuildTime`, `Range 2` (:1036) [DATA] |
| S2 | liberty Abil:2472–2479 (`LarvaTrain`) | `Flags WaitForFood 0`: the larva's egg, which has no queue, does not wait for food. So the flag is a per-ability choice, and every queued train ability waits [DATA; the reading of `WaitForFood` is [INF], see rule 1] |
| S3 | core Abil:664–670 (`CAbilQueue` default), liberty Abil:1111–1134 (`que5` and its variants: `QueueSize 5`); liberty Unit:5868–5870 (`Barracks`: `que5`, `BarracksTrain`, `Rally`) | One queue of five slots beside the train ability (B-58's five, out of scope) [DATA] |
| S4 | core Abil:671–701 (the `CAbilRally` default); liberty Abil:859–897 (`Rally`, `ProgressRally`, `RallyCommand`, `RallyNexus`, `RallyHatchery`); no later layer redefines them (voidmulti and balancemulti only comment out a `RallyHatchery` edit, voidmulti Abil:452–466) | The default has one `InfoArray` entry with no filter (:700); `RallyCommand` and `RallyNexus` change only their colour. **`RallyHatchery`** has three: index 0 for non-workers (`UseFilters -;Worker`, `SetValidators NotResourcesOrEnemyTargetType`: it cannot be set on a resource), index 1 for workers (`UseFilters Worker;-`, `SetOnGround 0`) [DATA] |
| S5 | liberty Unit:4223, 4425, 5870, 6271, 6404, 8246, 9598, 9670, 10414 (`Nexus`, `Gateway`, `Barracks`, `Factory`, `Starport`, `Hatchery`, `RoboticsFacility`, `Stargate`, `CommandCenter`) | Every production structure carries a rally ability (`Rally`, `RallyNexus`, `RallyCommand` or `RallyHatchery`); so do `Bunker` (6189), the eggs and cocoons, and the Nydus structures [DATA] |
| S6 | voidmulti Effect:2507–2537 (`CommandStructureAutoRally`), run by `CCCreateSet`, `NexusCreateSet` and `HatcheryCreateSet` (voidmulti Effect:166–169, 696–699, 713–716) | When a town hall is created it searches `HarvestableResource` within **`Radius 6.5`**, sorts by **`TSFarthestDistance`**, takes `MaxCount 1`, and issues `Rally` (and for a Hatchery `RallyHatchery` command 1, its worker rally) on it, validated by **`Minerals`**; the first is refused for a Hatchery (`CasterIsNotHatchery`) [DATA] |
| S7 | Blizzard, [Game Guide: Buildings](https://news.blizzard.com/en-us/article/4488317/game-guide-buildings), read on 2026-11-23 | "All unit production structures have the ability to set Rally Points, which will dictate the location that the unit will travel to immediately after it is created." "Setting the Rally Point of your command center / nexus / hatchery on mineral patches or vespene geysers will automatically command new workers to begin gathering those resources." "Rally Points can also be set on units to ensure that any new troops emerging from a building will group up around the designated unit. If a unit is rallied to another unit, but said unit dies, the rallied unit will continue to move towards the last spot the rally target occupied (this only occurs when a unit is created while the rally target is still alive). If the rally target dies before the unit exits the production structure, however, the new unit will not go anywhere. It will stay near the building awaiting further orders, as if there'd never been a rally point." "Rally Points set on transports or terran bunkers will load the newly created units into them." "If you build your structures too close to other structures or objects, newly created units may become stuck when they exit said structures. To prevent this, plan before you build or set rally points on the structures' unobstructed sides." [BLIZ] |
| S8 | Blizzard, [Game Guide: Supply](https://news.blizzard.com/en-us/starcraft2/4488314/game-guide-supply), read on 2026-11-23 | The cap of 200, and "you cannot build combat units if you lose the supply buildings in your main base". Nothing on queued jobs [BLIZ] |
| S9 | Liquipedia, Resources §Supply (the `Supply` page redirects there), wikitext through the MediaWiki API on 2026-11-23 | "The production of almost every unit requires an additional resource: Supply"; "'Supply Block' … means that the player does not have any supply available to build additional units." Nothing on queued jobs. (Further Liquipedia reads were refused by its rate limit on the same day; §15) [COMM] |
| S10 | Blizzard, [Warcraft III basics: Buildings](https://classic.battle.net/war3/basics/buildings.shtml), surfaced by a search on 2026-11-23 | "Rally pointed units exit the building on the same side as the destination Rally Point is set to." Another game by the same studio: an analogy, not a source for 5.0.14 [BLIZ, analogy] |

### 2.2 The rules that follow

1. **A job takes its supply when it starts, and waits for it at the front.** `WaitForFood 1` on every queued train
   ability (S1), and its absence where nothing queues (S2), means a train order that lacks food waits instead of
   failing [INF from the flag's name and where it is set]. That only the job in production counts against supply,
   and that the jobs behind it do not, is the owner's statement of B-59 and the common account of the game; no
   source read here states it in words (**verify**).
2. **Admission to an empty queue needs supply.** A train order given to an idle structure while supply-blocked is
   refused with the game's "not enough supply" error [INF from common play, **verify**]. Whether an order queued
   behind a running job is checked at admission is not sourced (**verify**); this plan does not check it, which is
   what rule 1 makes consistent (the job waits at the front when it gets there).
3. **Every production structure has a rally** (S5). It is a point, a unit or a resource (S4, S7).
4. **What a rallied unit does** (S7): it travels to the rally point; a worker rallied to minerals or a geyser
   gathers; a unit rallied to a transport or a Bunker loads into it; a unit rallied to another unit moves to it,
   and keeps going to that unit's last spot if it dies after the unit was created; if the target died before, the
   new unit "will not go anywhere … as if there'd never been a rally point".
5. **A town hall rallies itself on creation** to the farthest harvestable resource within 6.5 that is a mineral
   field (S6). For a Hatchery only the worker rally is set (S6, `CasterIsNotHatchery`).
6. **The Hatchery's rallies are two** (S4): army and workers; the army rally cannot be set on a resource. The
   Command Center's and Nexus's `InfoArray` has one entry (S4); their default `CmdButtonArray` names `Rally1` to
   `Rally3`, whose use outside `RallyHatchery` is not in the data (**verify**).
7. **A blocked exit does not stop production.** S7 says new units "may become stuck when they exit" structures
   built too close to others: they are placed even where they cannot get out, not held in the queue. Where the
   engine places them when the first spot is taken is not in the data read here (**verify**); this plan places at
   the nearest free cell (D-9). Units exiting on the rally's side (S7's "set rally points on the structures'
   unobstructed sides" assumes it, and S10 states it for Warcraft III) is [INF, **verify**].

### 2.3 Values

| Value | 5.0.14 | Here | Source |
|---|---|---|---|
| Supply of a waiting job | 0 | 0 | rule 1 |
| Auto-rally radius | `Radius 6.5` | 6.5 world units (one game unit is one cell, plan `2026-11-20-resource-footprints` D-1), authored | S6 |
| Auto-rally kinds | `Minerals` validator | `[Mineral]`, authored | S6 |
| Auto-rally choice | `TSFarthestDistance`, `MaxCount 1` | the farthest by centre distance, then the lower node id | S6 |
| Queue size | `QueueSize 5` | unlimited (B-58, class (ii)) | S3 |

---

## 3. Diagnosis

| # | Today | File, symbol |
|---|---|---|
| T1 | Admission charges the full supply of every job and refuses one whose reservation exceeds the cap (`charge_job`); `reserved_supply` is the product's `supply_cost` for every queued job, and `validate_unit_job` checks exactly that | `crates/sim_core/src/production/mod.rs` `charge_job`, `prepare_admission`; `production/lifecycle.rs` `validate_unit_job` |
| T2 | The front job counts down from the first production phase after it reaches the front (`advance_production`); there is no state "waiting for supply" | `production/lifecycle.rs` `advance_production` |
| T3 | The ledger sums `captured_supply_used` and every queued job's `reserved_supply` (`validate_supply_ledger`); death releases both (`prepare_reap_with_resources`); cancelling returns the job's reservation (`cancel_production_job`) | `production/mod.rs`, `production/lifecycle.rs` |
| T4 | `UnitOrder` has no rally; `Entity` has no rally field; a new unit is spawned with no order (`prepare_spawn` → `commit_spawn`) | `crates/sim_types/src/commands.rs`; `crates/sim_core/src/entity.rs`; `production/mod.rs` |
| T5 | A ready job whose ring has no free cell stays at 0 ticks, keeps its supply and publishes `ProductionBlock::ExitObstructed`, retried every tick (B-66). The ring is the footprint expanded by `ceil(terrain_radius + 0.5)` cells; each cell must lie on the map at the product's terrain radius, connect to the footprint by `ground_footprint_connected` (ground products), and overlap no body of the product's layer at the collision radii | `production/exits.rs` `production_exit`, `exit_ring`, `free_exit_point` |
| T6 | A right click resolves to `context_order`; a structure-only selection gets `Move`, which `submit_selection` sends to no recipient (structures are not mobile, `capable`) | `crates/client/src/battle_input/context_order.rs`, `battle_input/mod.rs` `issue_pointer_order`, `battle_pipeline/commands.rs` `capable` |
| T7 | `PlayerView::queues` publishes `exit_blocked` per structure; the HUD shows it on the first slot | `crates/sim_core/src/player_view.rs` `record_entity`; `crates/client/src/battle_hud/queue.rs`, `panels.rs` |
| T8 | **No world outside dedicated tests produces.** Every caller of `try_queue_production`, `queue_production_prevalidated` and `UnitOrder::Produce` on `4af9f51f` is a test, the client's card (`battle_hud/actions.rs`) or the skirmish pilot; no scenario (`assets/scenarios/*.ron` has no order that produces), no `sim_fixtures` battle, no `matchup_trace` or `route_trace` run, no bench row and neither golden issues one (`grep` of `crates/*/src`, `crates/*/examples`, `crates/*/benches`, `assets/`, the fixtures) | — |
| T9 | The goldens spawn town halls: `golden_replay.ron` places `foundry_core_dome` at (6, 6) and (52, 52) of `default_arena`; scenarios place one per start (`scenario.rs` `place_town_hall` → `spawn_structure`) | `crates/sim_core/tests/fixtures/golden_replay.ron` `structures`; `crates/sim_core/src/scenario.rs` |
| T10 | **The skirmish pilot never meets the supply cap.** It starts with 6 workers and a town hall of 15 (`standard_automaton`), trains `ARMY_SIZE` 6 soldiers of the first enabled armed product with at most `QUEUE_DEPTH` 2 queued, and never queues more than the army still wanted, so the units alive plus the jobs queued never exceed 6 soldiers: today's peak is `6 + 6c` for a product of supply `c`, 12 ≤ 15 at `c` = 1 (`pulse_bot`, the first armed product of the shipped lists; the gate's stage log confirms the choice) (`crates/client/tests/support/skirmish_pilot.rs` `plan_train`; `units.ron` supply costs). It trains no worker, so a town hall's rally never acts in it | — |

**Finding F-1 (the queue is the supply).** On `4af9f51f` a structure with a queue of `n` holds `n × supply_cost`,
so a player who queues ahead is supply-blocked earlier than in the game, and the HUD's supply counter includes
units that will not exist for minutes. B-59 is the visible symptom.

**Finding F-2 (an obstructed ring waits forever).** A ring blocked by units standing around a structure, by a wall
of other structures, or by the map edge on one side and units on the others, holds a paid, ready job and its supply
indefinitely (`real_structure_ring_preserves_paid_ready_job_until_removal` pins exactly that today). In the game the
unit appears (S7, rule 7).

---

## 4. Design decisions

### D-1. A job is in production or waiting; only the job in production holds supply

- `ProductionJob` gains **`in_production: bool`**, hashed. It is true for at most the front job of a queue, from
  the update in which that job starts until it leaves the queue.
- `reserved_supply` keeps its name and its meaning, "supply this job holds now": the product's `supply_cost` while
  `in_production`, 0 otherwise; research jobs hold 0 always. The ledger (`validate_supply_ledger`) is unchanged: it
  already sums `reserved_supply`.
- **Why a flag, not a derived state.** "Started" cannot be read off the countdown: a zero-duration job (allowed by
  the catalog: no validation forbids `build_time_ticks` 0, and `fifo_advances_only_one_job_and_zero_jobs_do_not_drain_the_queue`
  exercises it) has `remaining == total == 0` both waiting and ready, and a free product (`supply_cost` 0) cannot be
  told apart by its reservation. An explicit hashed flag is the one representation with no ambiguous state.
- **Invariants**, checked where `validate_job_source` checks the front today, as typed `ProductionInvariant::EntityState`
  failures: a non-front job is not `in_production`; a unit job's `reserved_supply` is `supply_cost` if
  `in_production` and 0 otherwise; a job not `in_production` has `remaining_ticks == total_ticks`.

### D-2. Starting a job

- **At admission.** A unit job admitted to a completed producer whose queue is empty starts at once:
  `in_production = true`, its supply charged with its price (today's `charge_job` check: `supply_used +
  supply_cost ≤ supply_max` unless the cost is 0; refused with `ProductionAdmissionError::Supply` otherwise). So the
  first job behaves byte for byte as today, and two admissions in one tick compete in bundle order, as today.
- **Behind others.** A job admitted to a non-empty queue pays minerals and gas only, holds 0 supply and is **not
  checked against supply** (rule 2, **verify**).
- **At the front** (`advance_production`, phase 2, entities in `EntityId` order as today): a front job not
  `in_production` starts if its supply fits (`supply_used + supply_cost ≤ supply_max`, or cost 0): it takes the
  supply, becomes `in_production` and counts down in the same update. Otherwise it waits: no countdown, and the
  producer is listed as `ProductionBlock::SupplyWait { producer }` for this tick. Research jobs start
  unconditionally.
- **Timing is unchanged when supply suffices.** Today the next job's first countdown is the update after the
  previous one completed (the doc of `update_production_and_buffs`); it starts in that same update now, so every
  job completes on the same tick as today whenever supply never binds.
- **Competition for the last supply** between producers starting on the same tick is decided by `EntityId` order
  (the phase's iteration order, stated as the rule); the game's order is not sourced (**verify**, R-3).
- **Supply lost while a job runs** (a provider destroyed): the job keeps its supply and finishes, as today; supply
  may then exceed the cap (`captured_supply_survives_catalog_edits_and_provider_loss`).
- **Cancelling** a waiting job returns its minerals and gas (it holds no supply); cancelling the job in production
  returns its price and its supply, and the next job starts at the next update if its supply fits. Death releases
  `reserved_supply` as today.
- **B-58 interplay.** With supply no longer bounding a queue, only money does; the owner allows more than five
  slots (class (ii)). No limit is added here (§14).

### D-3. The rally: an order and a field

- `sim_types` gains:

  ```rust
  /// Where a production structure sends the units it makes (StarCraft II's rally, 5.0.14 `CAbilRally`).
  pub enum RallyTarget {
      /// A point on the map, world units.
      Point(SimVec2),
      /// One of the player's own units: new units go where it stands when they leave.
      Unit(EntityId),
      /// A resource node: workers harvest it, other units go to its centre.
      Node(ResourceNodeId),
  }
  ```

  and `UnitOrder::SetRally { target: RallyTarget }`, appended last to the enum (serde keeps older bundles readable).
- `Entity` gains **`rally: Option<RallyTarget>`**, hashed (tag 0 Point and the point, 1 Unit and the id, 2 Node and
  the id, inside the existing `Option` encoding).
- **Who can rally:** an alive, completed, owned, unloaded entity whose archetype's `trains_units` is not empty. That
  is the capability (S5: every production structure has a rally ability); no catalog field is added and no identity
  is named (charter §3.3).
- **Admission** goes through the economy dispatcher (`dispatch_game_command` → `admit_game_command` → new
  `admit_set_rally`), with typed `GameCommandError`s: `validate_selection` as for every game command; no capable
  recipient → `NoCapableProducer` (new); a `Point` outside the map extent → `RallyOutsideMap` (new); a `Unit` that
  does not exist, is dead or is not the player's → `InvalidRallyTarget` (new); a `Node` not on the map →
  `UnknownResourceNode` (existing). Every capable recipient's `rally` is set; `queue` (Shift) is ignored (§14).
- **Clearing.** `SetRally { target: Unit(id) }` where `id` is the recipient itself clears that recipient's rally,
  as targeting the building itself does in the game [COMM-common play, **verify**].
- **Enemy units.** A rally on another player's unit is refused by admission; the client sends a `Point` at the
  ground under it instead (D-10). The game's behaviour on an enemy target is not sourced (registered, D-11).

### D-4. What a new unit does with the rally

After `commit_spawn` in `complete_production`, the producer's rally gives the child its order, through the same
admission a player's order would take, so every rule of that order applies (single recipient, `queue: false`,
slices on the stack, no allocation):

| Rally | Child | Order |
|---|---|---|
| none | any | none (today) |
| `Point(p)` | any | `Move { p }` (`issue_movement_prevalidated`) |
| `Node(n)` | has `HarvesterSpec` and `admit_harvest` accepts (node not seen exhausted, gatherable: a geyser needs the player's extractor) | `Harvest { n }` (S7) |
| `Node(n)` | otherwise | `Move { node centre }` |
| `Unit(u)`, `u` alive and not carried | `u` is the player's carrier and `admit_load` accepts the child | `Load { u }` (S7) |
| `Unit(u)`, `u` alive and not carried | otherwise | `Move { u's position now }` (S7, by snapshot: D-11 registers the game's following) |
| `Unit(u)`, `u` alive but carried | any | none this time; the rally stays |
| `Unit(u)`, `u` dead or gone | any | none, and the rally is cleared ("as if there'd never been a rally point", S7) |

- An ordinary refusal of the child's order (an unreachable point is not one: S-4 walks to the nearest reachable
  point) leaves the child idle, exactly as a refused player order would; an invariant failure is reported as a
  `SimulationDiagnostic` as today. This rule is stated in the module doc, so it is not a silent path.
- The child is ordered in the tick it spawns (phase 2), so it moves in that tick's movement phase.
- A rally is never written by the spawn except the clearing of a dead unit target.

### D-5. The rally point, for the exit and the client

`SimWorld::rally_point(producer) -> Option<SimVec2>` resolves a rally to a point: the `Point`, the node's centre, or
the target unit's position (alive and not carried; `None` otherwise). D-9 orders exits by it and the player view
publishes it (D-10). One function, so the exit, the order and the display agree.

### D-6. The hashed state and the schema

- Two hashed fields: `ProductionJob::in_production` (after `reserved_supply`) and `Entity::rally` (after
  `captured_supply_provided`). The schema takes the **next free number at merge**: 34 on `4af9f51f`; the
  contact-solver branch plans 34, so whichever merges second takes the next. The schema doc in `checksum.rs` gains
  its line.
- Both fields land in **one** package (WP1), with no behaviour: `in_production` is maintained exactly as D-2 will
  use it (set at admission to an empty queue and when the front starts counting), supply is still reserved for every
  job, and `rally` is written only by the town hall's auto-rally (D-7), which no world acts on until WP3. So the
  goldens are re-recorded once, and later packages leave them byte-identical (§7).
- **Proof of the re-record.** With the two fields left out of the encoding and the schema number unchanged, WP1's
  tree reproduces every recorded checksum of both goldens on all four builds; then the fields go in, the schema
  moves, and the new checksums are recorded under a header "Re-recorded for plan 2026-11-23-production-queue WP1
  (checksum schema N−1 -> N): each job's `in_production` and each entity's `rally` are hashed; the town halls'
  auto-rally sets their rally. No behaviour changed: with both fields excluded from the encoding, every checkpoint
  stays as recorded before." Builds: native debug and release, `wasm32-wasip1` release and debug
  (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`, the path the tree really has, not a
  symlink). `sim_fixtures`' `DEFAULT_SANDBOX_CHECKSUM` and the schema-byte tests of `checksum::encoding` move with
  it; `checksum_state_tests` gains one mutation per field.

### D-7. The town hall's auto-rally is data

- The archetype gains an optional capability, read from `structures.ron`:

  ```ron
  // CommandStructureAutoRally (voidmulti EffectData:2507-2537), run by CCCreateSet,
  // NexusCreateSet and HatcheryCreateSet: on creation, rally to the farthest harvestable
  // resource within Radius 6.5 (cells) that passes the Minerals validator.
  auto_rally: Some((radius: 6.5, kinds: [Mineral])),
  ```

  as `AutoRallySpec { radius: SimScalar, kinds: ResourceKindSet }` (the set of plan `2026-11-23-harvest-occupancy`
  D-4). Validation (typed `CatalogInvalidValue`): radius ≥ 0 and finite, no duplicate kind, and only on an archetype
  that trains units. Absent means no auto-rally.
- Authored on the seven town hall rows (the rows with `is_resource_dropoff: true` that train a worker:
  `foundry_core_dome`, `orbital_relay_dome`, `planetary_bulwark_dome`, `citadel_nexus_dome`, `ancient_tree_dome`,
  `canopy_grove_dome`, `primal_world_tree_dome` on `4af9f51f`; the implementer re-reads the rows rather than this
  list).
- **When:** whenever an entity of such an archetype is created: `spawn_structure` (scenarios, fixtures) and the
  foundation laid by `Build` (`place_foundation`), as the game's `CreateSet` runs at creation (**verify** that a
  built hall's `Create` event fires at placement, not completion; a foundation cannot train, so the difference is
  only when the rally appears).
- **Which node:** the nodes of a kind in `kinds` with `remaining > 0` whose placed footprint box comes within
  `radius` of the hall's centre (the area search reaches a target's extent, [INF, **verify**]); among them the
  farthest by centre distance (`TSFarthestDistance`), then the lower node id. Squared distances in raw fixed point.
- **Derived check (showcase slot 0, by hand from the map file):** the hall at (6, 6) of 5×5 has its centre at
  (8.5, 8.5). The fields whose 2×1 box comes within 6.5 are those anchored at (6, 14) and (9, 14) (5.52 each), (7, 15)
  (6.5 exactly) and (13, 13) (6.36); the farthest centre among them is (13, 13)'s (14, 13.5), 7.43 away. WP1's test
  asserts that node from the map, not from this paragraph.

### D-8. Zero recompilation

Removing `auto_rally` from a row, or changing its radius, in a copy of the shipped catalog changes the rally in the
same binary (`runtime_catalog_tests`). Rally capability follows `trains_units`, already run-time data.

### D-9. The exit search (B-66)

`production_exit` keeps ring 0 and adds outer rings:

1. **Ring 0 is today's** (`exit_ring`: the footprint expanded by `ceil(terrain_radius + 0.5)`), with today's test
   (`free_exit_point`: on the map at the terrain radius, connected to the footprint on the ground, no body of the
   product's layer within the collision radii).
2. **Order within a ring.** Without a rally point (D-5): clockwise from the minimum corner, today's order, so every
   world without a rally exits on the same cell as today. With one: by squared distance from the cell centre to the
   rally point, then clockwise index (rule 7, [INF, **verify**]).
3. **Outer rings** `k = 1, 2, …`: ring 0's bounds expanded by `k` cells, tried only when ring `k − 1` had no free
   cell. A cell of an outer ring is free when it lies on the map at the product's terrain radius, a ground product's
   disc there is legal ground (the existing point test, `egress_connected(point, point, terrain, None)`) **on the
   producer's cliff level** (the level of its footprint's cells, or of the cell under a mobile producer), and no body
   of the product's layer lies within the collision radii. Air products need only the map and the air bodies.
4. **Bound:** the search stops at the first free cell, or when a ring lies wholly outside the map, so at most
   `max(width, height)` rings and every map cell once. Only then is `ExitObstructed` published, and the job waits as
   today.
5. **Why no connection test outside ring 0.** The game places units that then "may become stuck" (S7): it does not
   hold the queue for a path, so a ring of structures or bodies must not stall it. The level test keeps units from
   appearing on a plateau or in a pit beside the producer (the case `clear_but_disconnected_plateau_is_not_a_production_exit`
   pins, which keeps its meaning). A ramp cell has its own level and is accepted only on the producer's.
6. **No movement code.** The level test reads `sim_map` cells; the point test is the existing call. Nothing under
   `crates/sim_core/src/movement/` changes (R-5).
7. **Cost.** Outer rings run only on a tick a ready producer's ring 0 is full. Ordering a ring by distance sorts its
   perimeter in a retained `ProductionWorkspace` buffer (`// PERFORMANCE RATIONALE:`). The worst case, a map with no
   free cell, scans every cell once per tick per waiting producer; no shipped world approaches it.

### D-10. Client: setting and showing a rally

- **Setting (right click, as in the game).** `context_order` stays the rule for units. `issue_pointer_order` also
  submits, to the selection's rally-capable structures, `SetRally` with the target the click resolved to:
  `Ground(p)` → `Point(p)`; `Node(n)` → `Node(n)`; `Friendly(u)` → `Unit(u)` (the structure itself clears it);
  `Enemy(_, ground)` → `Point(ground)` (D-3). A structure-only selection submits only the rally; a mixed selection
  submits both, and the click is accepted when either is. `capable` admits `SetRally` for an archetype whose
  `trains_units` is not empty (the simulation's rule, D-3); `order_subject` has no subject for it.
- **The order marker** shows at the click with a new `OrderMarkerKind::Rally` tint.
- **Showing.** `PlayerView` gains `rallies: Vec<RallyView { entity, point: Vec2 }>` for the viewer's own structures
  with a rally that resolves (D-5); the point is `f32` presentation output (charter §3.2). While a structure with a
  rally is selected, the client draws a rally reticle at the point (the selection ring's material, rally tint) and a
  thin ground line from the structure's centre to it. Both are pooled entities, grown to the most rallies ever
  shown and hidden when unused, so the steady state allocates nothing (`sync_steady_state_tests`). A rally to a
  unit follows it, because the view resolves it every tick.
- **Supply wait.** `StructureQueueView` gains `supply_blocked`, true when the structure is in this tick's
  `SupplyWait` blocks; the queue panel marks the first slot with it as it marks `exit_blocked` today.
- **No card button.** The game's Rally button (and its targeting mode) is out of scope (§14): right click is the
  way the game's players set rallies.

### D-11. Records

- **B-59** fixed by WP2; **B-60** by WP3 (simulation) and WP5 (client); **B-66** by WP4.
- New rows, registered by WP0 with the next free numbers (B-163 to B-167 on `4af9f51f`):
  - "A rally on a unit sends the new unit to where the target stands when it leaves; in 5.0.14 the unit follows
    its target (Blizzard, Game Guide: Buildings)." Minor; a follow order is a movement change (§14).
  - "One rally per production structure. 5.0.14's Hatchery has a separate worker rally and its army rally cannot be
    set on a resource (`RallyHatchery`, liberty AbilData:885–897); here a resource rally sends other units to the
    node." Minor.
  - "No Shift-queued rally waypoints (**verify** the game's)." Minor.
  - "A rally on an enemy unit is a point rally at its position; the game's behaviour is not sourced." Minor.
  - "The exit order without a rally (clockwise from the ring's minimum corner) and the outer-ring search are this
    project's; the game's placement search is not in the data read (**verify**)." Minor.

---

## 5. Determinism and checksum

- **Integer arithmetic only.** Supply checks are integer; the auto-rally radius is read once from RON into
  `SimScalar`; distances compare squared in raw fixed point; the client's click point enters through `sim_point`,
  the checked input conversion every order uses.
- **Order.** Starts in `EntityId` order (D-2); admissions in bundle order; exit candidates by `(squared distance,
  clockwise index)` or clockwise index alone; the auto-rally node by `(centre distance descending, node id)`.
  Nothing depends on iteration order of an unordered container.
- **Hashed state:** D-6. Schema next free at merge.
- **Goldens:** re-recorded in WP1 only, proven on four builds (D-6); every later package passes them unchanged on
  native debug and release and `wasm32-wasip1` release and debug.
- **wasm.** No new dependency; the simulation crates still build for `wasm32-unknown-unknown`.
- **Cost.** One flag test per producer per tick; the rally resolution at a spawn; outer rings only when ring 0 is
  full (D-9). The throughput bench produces nothing.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance below is a figure of a seed. Each is a property of the rule, tested on constructed worlds that read
every price, time and radius from the catalog.

- **P-1 (supply).** At every tick of any world, `supply_used` equals the captured supply of live and unreaped
  entities plus the supply of the jobs `in_production`, and no job but a front one is `in_production`
  (`validate_supply_ledger` and D-1's invariants, run after every tick of every WP2 test).
- **P-2 (no regression in time).** In a world where supply never binds, every job completes on the same tick as on
  WP1's tree (a queue of five on two producers, compared tick for tick).
- **P-3 (waiting).** A front job that lacks supply does not count down, publishes `SupplyWait`, and starts on the
  first update in which its supply fits; jobs behind it neither count down nor hold supply.
- **P-4 (rally).** Each row of D-4's table holds on the tick of the spawn; a rally changes nothing in a world
  without one (every WP3 world without a rally byte-identical to WP2).
- **P-5 (exits).** With ring 0 free, the exit is today's cell (no rally) for every producer and product in the
  catalog tests; with ring 0 full, the unit appears on the nearest ring with a free cell on the producer's level,
  and on no nearer one; with a rally, on the free cell of that ring nearest the rally point.
- **P-6 (bounded).** With every cell of the map occupied, the search ends with `ExitObstructed` after visiting each
  cell at most once (a counting test on a small map).
- **P-7 (worlds).** §7's table.

---

## 7. Worlds that move, and why no prototype was built

**No scenario, battle or showcase uses production** (T8). The matrix, battles 31 and 33 and the showcase on its
default seed and seeds 1 to 8 (the commands of `~/scratch/resource-footprints/scripts/measure.sh`) produce nothing,
so B-59, B-66 and the spawn's rally order cannot act in them. The auto-rally writes `rally` on the showcase's and
skirmish's town halls, which the trace outputs do not print and nothing reads until a unit is produced there. A
prototype would measure a constant; the evidence is the code survey of T8, and each package proves the property by
running the same commands on its tree against WP0's.

| World | WP1 | WP2 | WP3 | WP4 | WP5 |
|---|---|---|---|---|---|
| Matrix (seeds of record and 16), battle 31 (100 seeds), battle 33 (64 seeds), showcase (9 seeds, 6,000 ticks) | byte-identical | byte-identical | byte-identical | byte-identical | byte-identical |
| `golden_replay`, `golden_replay_orders` | **re-recorded, bytes only** (D-6 proof) | unchanged | unchanged | unchanged | unchanged |
| `DEFAULT_SANDBOX_CHECKSUM` | moves (schema) | unchanged | unchanged | unchanged | unchanged |
| Client skirmish match (`skirmish_match_tests`) | stage log identical | identical (T10: supply never binds) | identical (no worker trained, no rally set) | identical (ring 0 free) | identical; the pilot's right clicks select units, never a structure (re-checked by the gate) |

- A difference in any byte-identical row is a defect of the package, not a re-record.
- The skirmish's stage ticks are reported against WP0's tree in every package that touches `sim_core` or the
  client; a change stops the package (§8).

---

## 8. Stop rules

1. **WP1 is behaviour-neutral.** Every world and test of §7 is unchanged except the checksums, and the D-6 proof
   holds on all four builds before the re-record. Any other difference stops the package.
2. **WP2 changes supply only.** P-1 to P-3 hold; every existing test that fails is one §12 lists, re-derived from
   the rule (not loosened); an unlisted failure is a finding and stops the package.
3. **WP3 changes only rallied worlds.** P-4; a world without a rally is byte-identical to WP2's (the full
   `sim_core` suite's worlds and §7's).
4. **WP4 changes only obstructed or rallied exits.** P-5 and P-6; every world whose ring 0 has a free cell and no
   rally exits on today's cell.
5. **No tuning constant** is introduced: the radius is the data's, the bound is the map, the orders are stated.
6. **The skirmish match** wins within its `TICK_LIMIT`, stage for stage as on WP0's tree; if not, the package
   stops with the trace (the bank, the supply, the stage the pilot waits in) and goes to the orchestrator.

---

## 9. Client consequences

- `battle_input/context_order.rs`: `rally_target(PointerTarget) -> RallyTarget` (pure, unit-tested).
- `battle_input/mod.rs` `issue_pointer_order`: the second submission (D-10).
- `battle_pipeline/commands.rs`: `capable` and `order_subject` arms for `SetRally`.
- `battle_input/order_marker.rs`: `OrderMarkerKind::Rally`.
- A new `battle_input/rally_marker.rs` (`//!` doc): the pooled reticle and line, fed from `PlayerView::rallies` and
  the selection.
- `battle_hud/queue.rs`, `panels.rs`: `supply_blocked`.
- Tests: `battle_context_order_tests` (structure-only, mixed, node, own unit, itself, enemy), `battle_hud_tests`
  (the supply-wait mark), `sync_steady_state_tests` (a selected structure with a rally keeps its pools on every
  tick), and one windowed check (`runtime_media_tests` style, GPU per child through `gpu_run.rs`) that the reticle is
  drawn at the projected rally point while selected and hidden otherwise.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP6 | Five rows registered (D-11); B-59, B-60, B-66 fixed; the Production row |
| `docs/plans/README.md` | plan, WP6 | Row `Proposed` with the plan; `Done` in WP6 |
| `crates/sim_types/src/commands.rs` (or a new `rally.rs`) | WP1 (`RallyTarget`), WP3 (`UnitOrder::SetRally`) | Types, docs, serde |
| `crates/sim_types/src/player_view.rs` | WP2 (`supply_blocked`), WP3 (`RallyView`, `rallies`) | View types |
| `crates/sim_core/src/production/mod.rs` | WP1, WP2 | `ProductionJob::in_production`; `charge_job` and admission; `ProductionBlock::SupplyWait` |
| `crates/sim_core/src/production/lifecycle.rs` | WP1, WP2, WP3 | Start at the front; invariants; cancel; the child's rally order after `commit_spawn` |
| `crates/sim_core/src/production/exits.rs` | WP4 | Outer rings, rally order, level test, retained buffer |
| `crates/sim_core/src/production/rally.rs` (new) | WP1 (auto-rally), WP3 | `admit_set_rally`, `rally_point`, the child's order, the auto-rally search |
| `crates/sim_core/src/entity.rs` | WP1 | `Entity::rally` |
| `crates/sim_core/src/world.rs`, `production/construction.rs` | WP1 | Auto-rally at `spawn_structure` and `place_foundation` |
| `crates/sim_core/src/economy/mod.rs`, `error.rs` | WP3 | `admit_game_command` arm; `GameCommandError` variants |
| `crates/sim_core/src/world/commands.rs` | WP3 | `SetRally` routed to the economy dispatcher |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs` | WP1 | `AutoRallySpec` and its validation |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | WP1 | Schema, fields, `RallyTarget` encoding |
| `crates/sim_core/src/player_view.rs` | WP2, WP3 | `supply_blocked`, `rallies` |
| `assets/catalog/structures.ron` | WP1 | `auto_rally` on the town hall rows |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures/tests/fixture_tests.rs` | WP1 | Re-record with header; sandbox checksum |
| `crates/sim_core/tests/production_supply_tests.rs` (new), `rally_tests.rs` (new), `production_transactions_tests.rs`, `research_and_cancel_tests.rs`, `production_timer_tests.rs`, `checksum_state_tests.rs`, `runtime_catalog_tests.rs`, `catalog_reference_values_tests.rs`, `player_view_tests.rs` | WP1 to WP4 | §12 |
| `crates/client/src/battle_input/*`, `battle_pipeline/commands.rs`, `battle_hud/queue.rs`, `panels.rs`; `crates/client/tests/battle_context_order_tests.rs`, `battle_hud_tests.rs`, `sync_steady_state_tests.rs` | WP5 | §9 |
| `docs/evidence/2026-11-23-production-queue.md` | WP1 to WP6 | The proofs and the world comparisons |

Nothing under `crates/sim_core/src/movement/` changes; a movement agent works there (R-5).

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`;
the implementation agent writes the failing tests first, runs `cargo gate quick` and the tests its change needs and
hands back; a verifier runs `cargo gate package` (Q0061). §7's world comparison is run in every package that
touches `sim_core` (the measure script against WP0's tree, outputs under `~/scratch/production-queue/<wp>/`).

### WP0: records (S, docs only)

- `DESIGN.md` §4: D-11's five rows with the next free numbers; B-59, B-60 and B-66 point to this plan.
- **Gate:** docs-only `package`.

### WP1: the new state, the auto-rally and the schema (M, `sim_types`, `sim_core`, catalog)

Failing tests first (red on `main`):
- `checksum_state_tests`: mutating a job's `in_production`, and an entity's `rally` (each variant), changes the
  checksum.
- `checksum::encoding` unit tests: `RallyTarget`'s three tags; the schema byte.
- `production_supply_tests::the_first_job_is_in_production_and_the_rest_wait_their_turn`: admissions to an idle
  producer: the first is `in_production`, the others not; after the first completes the next is `in_production`
  on the update it starts counting (supply still reserved for all in WP1, asserted as today).
- `rally_tests::a_town_hall_rallies_to_the_farthest_mineral_field_in_reach`: the showcase start's hall, the node
  computed from the map's nodes and the row's `auto_rally` (D-7); a hall with no field in reach has no rally; a
  geyser is never chosen.
- `runtime_catalog_tests`: a copy of the catalog without `auto_rally` on the hall row gives no rally, and one with a
  radius below every field's reach gives none (D-8).
- `catalog` unit tests: negative radius, duplicate kind, and `auto_rally` on an archetype that trains nothing are
  refused (`CatalogInvalidValue`, naming the field).
- `catalog_reference_values_tests::the_shipped_town_halls_auto_rally_as_5_0_14`: radius 6.5, `[Mineral]`, with S6's
  lines.

Then the change: D-1's field (maintained, supply unchanged), D-3's `RallyTarget` and `Entity::rally`, D-7, D-6.
**Acceptance:** §8 rule 1 with the D-6 proof on four builds, recorded in the evidence.

### WP2: supply at the start (M, `sim_types`, `sim_core`)

Failing tests first (red on WP1's tree), in `production_supply_tests.rs`, every price, time and supply from the
catalog:
- `queued_jobs_hold_no_supply`: three jobs: `supply_used` rises by one product's cost.
- `the_next_job_takes_its_supply_when_it_starts`.
- `a_front_job_waits_for_supply_and_starts_when_it_fits`: at the cap the next front job does not count down,
  `SupplyWait` is published, and on the update a provider completes it starts and counts (P-3).
- `admission_to_an_idle_producer_needs_supply_and_behind_a_job_does_not`.
- `cancelling_a_waiting_job_returns_only_its_price` and `cancelling_the_running_job_releases_its_supply_and_the_next_starts`.
- `two_producers_waiting_for_the_last_supply_start_in_entity_order`.
- `supply_lost_during_production_does_not_stop_the_running_job`.
- `jobs_complete_on_the_same_ticks_when_supply_never_binds` (P-2, against a recorded WP1 run of the same world).
- `corrupt_queue_states_are_invariant_failures`: a non-front job `in_production`, a waiting job holding supply.
- `player_view_tests`: `supply_blocked` set on the waiting structure only.

Re-derived (expected to change, from D-2): `production_transactions_tests::selection_order_and_reservations_are_exact`
(admissions behind a job are no longer refused for supply; the refusal is re-derived for an idle producer at the
cap), `research_and_cancel_tests::cancelling_training_refunds_payment_and_supply` (two admissions hold one product's
supply), and any test that asserts `supply_used` after queueing two or more. Expected unchanged:
`production_timer_tests` (P-2), `replay_keeps_transaction_and_supply_state_identical`.

Then the change: D-2. **Acceptance:** §8 rule 2.

### WP3: rally command and the new unit's order (M, `sim_types`, `sim_core`)

Failing tests first (red on WP2's tree), in `rally_tests.rs`:
- `set_rally_is_admitted_only_for_producers` and its refusals (no capable producer, outside the map, a foreign or
  dead unit, an unknown node); `a_rally_on_the_producer_itself_clears_it`.
- `a_unit_moves_to_a_point_rally`: the child's front order is `Move { p }` on its spawn tick and it arrives.
- `a_worker_rallied_to_a_mineral_field_harvests_it` (S7) and `a_soldier_rallied_to_a_field_walks_to_it`.
- `a_worker_rallied_to_a_geyser_without_an_extractor_walks_to_it`.
- `a_unit_rallied_to_a_friend_goes_where_the_friend_stood`: the friend moves on afterwards; the child's target is
  the snapshot (D-11's row).
- `a_dead_rally_target_sends_no_order_and_clears_the_rally` (S7).
- `a_unit_rallied_to_its_carrier_boards_it` (S7).
- `a_town_halls_auto_rally_sends_its_workers_to_harvest` (WP1's rally now acts).
- `a_world_without_a_rally_is_unchanged`: WP2's tree and WP3's agree byte for byte on a production world.
- `player_view_tests`: `rallies` lists the viewer's own resolved rallies only.

Then the change: D-3, D-4, D-5. **Acceptance:** §8 rule 3.

### WP4: placed exits (M, `sim_core`)

Failing tests first (red on WP3's tree), in `production_transactions_tests.rs` and `exits.rs` unit tests:
- `a_ringed_producer_places_its_unit_on_the_next_ring` (replaces `real_structure_ring_preserves_paid_ready_job_until_removal`:
  the same ring of 1×1 structures; the unit appears on ring 1, the job leaves the queue, the ledger holds).
- `units_crowding_the_ring_do_not_stall_production`: every ring-0 cell holds a body; the exit is the first free
  ring-1 cell in clockwise order.
- `the_unit_exits_nearest_the_rally_point` on ring 0, and on ring 1 when ring 0 is full (P-5).
- `an_exit_is_never_on_another_level` (`clear_but_disconnected_plateau_is_not_a_production_exit` kept unchanged:
  every other cell is a level up, so the job still waits).
- `a_full_map_publishes_exit_obstructed_after_one_visit_per_cell` (P-6, a counting test on a small map).
- `open_ground_exit_is_the_first_ring_cell_at_the_minimum_corner` and the other ring-0 unit tests stay as they are.
- Re-derived: `fractional_mobile_producer_keeps_paid_job_until_its_full_exit_is_clear`,
  `two_mobile_producers_contend_for_one_exit_in_stable_id_order`,
  `air_training_ignores_ground_walls_but_respects_air_occupants` where their premise was a stall; each is rewritten
  to the new rule's outcome from a trace, not loosened.

Then the change: D-9. **Acceptance:** §8 rule 4.

### WP5: client (M, `client`)

Failing tests first (red on WP4's tree): §9's tests. Then the change: D-10. **Acceptance:** the client package gate
(GPU per child), the skirmish match as §7, and a capture of a selected producer with a rally for the evidence
(`with-gpu-lock`, radv, under `~/scratch/production-queue/wp5/`).

### WP6: bookkeeping (S)

- `DESIGN.md`: B-59, B-60 and B-66 fixed with commits; the Production and research row ("production queues, cancel
  and refund, supply at the start of production, rally points, placed exits") with this plan's link and the new
  test files; the HUD row mentions the rally display.
- The evidence completed; this plan's `## As built`; README row `Done`.
- **Gate:** docs-only `package`.

---

## 12. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| State | Both fields hashed; schema; goldens re-recorded with proof | WP1: `checksum_state_tests`, `checksum::encoding`, goldens on four builds |
| Data | Auto-rally read at run time, validated, sourced | WP1: catalog unit tests, `runtime_catalog_tests`, `catalog_reference_values_tests` |
| Supply | Only the running job holds supply; waiting; admission; cancel; ties; no time regression | WP2: `production_supply_tests` |
| Rally | Admission; each row of D-4; auto-rally acting; no effect without a rally | WP3: `rally_tests`, `player_view_tests` |
| Exits | Outer rings; level; rally order; the bound | WP4: `production_transactions_tests`, `exits.rs` unit tests |
| Client | Right click sets the rally; display pooled and correct; supply-wait mark | WP5: §9 |
| Worlds | §7's table | Every package |

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The game checks supply at admission behind a running job (rule 2, **verify**) | Allows a queue the game would refuse | The job then waits at the front (D-2), so supply can never be exceeded; the admission rule is one condition in `charge_job`, an amendment if a source appears |
| R-2 | The schema number collides with the contact-solver branch | A merge conflict in `checksum.rs` and the goldens | "Next free at merge" (D-6): the second to merge rebases, takes the next number and re-records with its own proof |
| R-3 | Same-tick competition for the last supply differs from the game | Which of two producers starts first | Stated rule (`EntityId` order), registered as unsourced in D-2 |
| R-4 | `full_match_tests` or a client test trains a worker from a town hall and then counts idle workers | With WP3 the worker goes harvesting on its own (auto-rally) | Re-derived from S6/S7 in WP3 if it fails; listed here so it is not a surprise |
| R-5 | The movement agent is changing `sim_core/src/movement` | Conflicts | No file there changes; the exit search uses the existing point test and `sim_map` cells |
| R-6 | The outer-ring search costs a full-map scan per tick when nothing fits | Slow ticks in a pathological world | Bounded (P-6); runs only when ring 0 is full; no shipped world produces into a full map |
| R-7 | Units placed beyond a wall surprise a player who walled a structure in | Behaviour the game also shows ("may become stuck", S7) | Registered row on the exit order; the level test keeps cliffs |
| R-8 | The wasm runner's `--dir` is a symlinked path | Golden proof fails on `wasm32-wasip1` for a wrong reason | The real tree path, as D-6 says |
| R-9 | The client's two submissions per click double a rejection message | Noise in the HUD | The click is accepted when either submission is; only both failing reports |

---

## 14. Out of scope and decisions recorded (Q0002)

- **B-58, the queue limit of five** (S3): class (ii), the owner allows more; a configurable limit is a separate
  change.
- **A follow order** for rallies to units (D-11): a movement change, after the movement work.
- **The Hatchery's second (worker) rally** and the `Rally2`/`Rally3` buttons (S4, D-11).
- **Shift-queued rally waypoints** (D-11).
- **The Rally button on the command card** and its targeting mode (D-10).
- **Rally on Bunkers, eggs and Nydus structures** (S5): no shipped unit is one of these with `trains_units`.
- **Rally while a structure is under construction** (the game's `ProgressRally`): rallies are set on completed
  structures here; the auto-rally of a laid foundation is the one exception (D-7).
- **`CAbilTrain` `Range 2`** (S1): its meaning is not in the data.
- **Larva, eggs and warp-in** (B-63): no queue, no rally here.
- **A rally to a foundation** making a worker resume construction (the game's smart order): the worker moves there.
- **No radv capture for WP5's acceptance** (orchestrator, 2026-11-23): the windowed test child replaces the capture; no capture goal is added to `client_diagnostics`. `crates/client/tests/rally_gpu_tests.rs` shows the rally by pixels on radv locally and lavapipe in CI.

---

## 15. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,voidmulti,balancemulti}/{AbilData,UnitData,EffectData}.xml`
  (`5444e12f13`), read as data.
- Liquipedia: `Supply` (→ `Resources`) and `Mechanics` wikitext through the MediaWiki API on 2026-11-23, saved in
  `~/scratch/production-plan/web/`. Further requests (search, other titles) were refused by Liquipedia's rate limit
  that day; no source here depends on them. The Blizzard guides (S7, S8) and S10 were read through the web the same
  day.
- No prototype (§7).

## As built (implementation agent, 2026-11-23)

| WP | Commit | Notes |
|---|---|---|
| WP0 | `8807620e` | B-163 to B-167 registered (D-11); B-59, B-60 and B-66 point here |
| WP1 | `0ddc0b4f` | Schema 34 (`main` at 33). D-6 proof on four builds; the sandbox checksum moves with it. `AutoRallySpec` and `RallyTarget` in `sim_types::commands`; the auto-rally in `production/rally.rs`. C7's count for `catalog.rs` 71 → 72 |
| WP2 | `19edaebe` | `start_front_job` in `lifecycle.rs`; P-2 checked against WP1's tree |
| WP3 | `e9268383` | `RallyView::point` is `[f32; 2]` (the plan's `Vec2` is a Bevy type), so `PlayerView` loses `Eq`; C7 gains `sim_types/src/player_view.rs`. The client's `capable`, `order_subject` and refusal-message arms land here because the new variants break the client's exhaustive matches. `a_world_without_a_rally_is_unchanged` asserts that a child of a structure without a rally takes no order; the byte comparison of WP2's and WP3's trees is §7's worlds |
| WP4 | `d4486d19` | `RingTest { Zero, Outer(level) }` names the cell test; `exit_cells_visited` counts the bound. The plan's `an_exit_is_never_on_another_level` is the kept `clear_but_disconnected_plateau_is_not_a_production_exit` |
| WP5 | `365f7568`, WP5B | `battle_input::rally_marker`. The windowed check is `rally_gpu_tests` (WP5B: one child through `gpu_run.rs`, three shots: unselected, selected, deselected, compared at the reticle and along the line); the radv capture was dropped from the acceptance (§14) |
| WP6 | this commit | Register, Production and HUD rows, evidence, README |

Figures are in [`docs/evidence/2026-11-23-production-queue.md`](../evidence/2026-11-23-production-queue.md): the
matrix, battles 31 and 33 and the showcase on nine seeds byte-identical in every package; goldens re-recorded once
(WP1) and unchanged after on four builds; the skirmish match won at tick 6,486 in every package, every stage
unchanged.

