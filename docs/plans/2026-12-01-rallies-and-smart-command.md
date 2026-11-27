# Blueprint: The Remaining Rally Rows and the Per-Unit Smart Command (B-163, B-167, B-214, B-215, B-217, B-222)

Six rows of `DESIGN.md` §4 remain from the production queue, rally and follow-order plans
([`2026-11-23-production-queue`](2026-11-23-production-queue.md), [`2026-11-25-rallies`](2026-11-25-rallies.md),
[`2026-11-26-follow-order`](2026-11-26-follow-order.md)). Read in full on `main` `80d01ebe`, they fall into four
groups:

- **Already fixed; only the record is wrong (B-163).** The follow order exists on `main`. A rally on a mobile unit
  that the new unit neither boards nor attacks gives `Follow { Move }` (`0a797bab`), and a right click on one's own
  mobile unit follows it (`27604c11`). The row still begins "open;". One doc table in `production/rally.rs` still
  describes the old snapshot `Move`. No behaviour is left to build (§3 T1 to T3).
- **Settled by the owner (B-214).** Q0115 was answered (a): the rally card entries keep positional keys. That is a
  sanctioned deviation, so it belongs in Q0052's addenda, not in the bug register.
- **Closed by the data (B-217).** The three `RallyHatchery` slots of 5.0.14 accept disjoint sets of units, so
  "first slot whose use rule accepts" and "first slot that accepts and holds a rally" give the same answer for
  every unit the game makes. The row's **verify** concerns a choice that the game's own data never exposes (§2.2
  rule 4). Two tests pin that, and the row closes.
- **Unsourced (B-167, B-215).** Neither the data nor any readable web source gives the engine's exit placement or a
  5.0.14 cap on rally waypoints. Both are drafted as owner questions (Q0140, Q0141 provisional, §16). Until they are
  answered nothing changes, and WP3 builds whichever answer comes, from data.
- **A real behaviour change (B-222).** A right click today picks one order for the whole selection: the first
  table row that *some* member can take. The members that cannot take it get nothing. 5.0.14 resolves the smart
  command **per unit**: each unit uses the first of its own abilities that accepts the target, and falls back to its
  move ability (a follow on a unit, a move on a point). This plan moves the right click to per-member resolution.
  The change is client-only (WP1). Each distinct order goes out as its own `PlayerCommand` in one tick, with
  disjoint recipients, as the A-click on a unit already does (`submit_attack_on_unit`).

One part of B-222 needs a frozen file and is **deferred until the Gated movement packages close** (§14). 5.0.14
lets a transport's right click on one's own ground unit pick that unit up (`CAbilTransport` `AllowSmartCmd`). This
project has no carrier-initiated load order. Adding one means a carrier walking to a moving passenger, which is a
new `walking_goal` arm in `movement/locomotion.rs`. It is registered as B-247 (provisional) and designed in outline
as WP-D1, with its schema step and golden proof.

A full plan is required (charter §6). This plan changes the client's command semantics across several packages
(WP1 to WP3). WP3 and WP-D1 may move battle outcomes or the checksum format.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-27 against `main` at
`80d01ebe` (checksum schema 61).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** `main` ends its register at B-245, and its last question is
  Q0138. The unmerged local branch `diagonal-cliffs-plan` (`61f2cd92`) already claims **B-246** (plan
  `2026-12-01-straight-diagonal-cliffs`) and **Q0139** (`open/0139-diagonal-cliff-overhang.md`). So this plan uses
  **B-247**, **Q0140** and **Q0141**. WP0 replaces them with the next free numbers at its commit and renumbers every
  mention here in the same commit.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations (S-1: RON data instead of editor data) | Closed; gains an addendum for Q0115 | D-5 |
| Q0115 (closed, answered (a)): rally card entries keep positional keys | Followed | D-5, B-214 |
| Q0119 (closed, answered (b)): A-click members that cannot attack a mobile unit follow it | Followed: the right click's per-member fallback is the same rule | D-1 |
| Q0104 (default): an A-click on one's own unit attacks it | Unchanged: a right click never attacks one's own unit (`SmartFilters "-;Player,Ally"`) | D-1 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0002: scoping is the agent's call if written down | Standing rule | §14 |
| The owner's notes of 2026-11-24 (movement over matchup claims) and 2026-11-23 (keep working while questions are open) | Followed | §7, WP3 |
| The Gated movement plan's freeze (`2026-11-27-gated-movement` §11 runtime rules) | Binding | §10, §14, WP-D1 |
| **Draft Q0140 (provisional, not filed):** exit placement (B-167) | Draft, §16 | D-6, WP3 |
| **Draft Q0141 (provisional, not filed):** the rally waypoint cap and a Shift-click past it (B-215) | Draft, §16 | D-7, WP3 |

---

## 1. Summary

| Row | Today (`80d01ebe`) | After this plan |
|---|---|---|
| B-163 | Fixed in code by the follow order, but the row reads "open;" and `give_rally_order`'s doc table says "`Move` to where it stands now (B-163)" | Row **fixed** (WP0). The doc table states the `Follow` rows (WP2). A test asserts the doc's table row by row (P-3) |
| B-167 | Exit order clockwise from the ring's minimum corner, or nearest the rally; the outer-ring search is ours; unsourced | Q0140. Default: unchanged, row open. (a) sanctioned and closed; (c) the owner's observation built as data (WP3, D-6) |
| B-214 | Listed as a bug, though the owner chose positional keys (Q0115 (a)) | A Q0052 addendum "S-rally-keys"; the row is removed from the register (WP0, D-5) |
| B-215 | `max_waypoints: 4` on every producer (a community figure); a Shift-click on a full route is refused (`RallyWaypointsFull`) | Q0141. Default: unchanged, row open. (a) sanctioned; (c) the owner's count and full-route rule, as data (`max_waypoints`, and a new `when_full` only if the answer needs one; WP3, D-7) |
| B-217 | First slot whose use rule accepts, no fall-through, marked **verify** | Closed by the data's disjoint use rules (§2.2 rule 4). Shown by an oracle test over the shipped catalog and a no-fall-through test (WP2, D-4) |
| B-222 | One order for the selection, the first row some member takes; the rest get nothing | Per-member resolution, one `PlayerCommand` per distinct order in one tick (WP1, D-1 to D-3). The transport pickup half is B-247, deferred (WP-D1) |
| Hashed state | — | **None in WP0 to WP3.** WP-D1 (deferred) adds an order tag; schema next free at merge, with the 4-build proof (§5) |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`), read as data from `~/scratch/combat_wp3_data/<layer>/`. "core
  Abil:N" means line N of `core/AbilData.xml`, "liberty Unit:N" line N of `liberty/UnitData.xml`, and so on. The
  later layers (`libertymulti`, `swarm`, `swarmmulti`, `void`, `voidmulti`, `balancemulti`) were grepped for every
  record cited. An override is named where one exists. Nothing there was executed.
- **[BLIZ]** Blizzard's own pages.
- **[COMM]** Community sources.
- **[INF]** An inference of this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | core Abil:115–119 (`CAbilAttack` default) | `AcquireFilters "-;Player,Ally,Neutral,…"` and **`SmartFilters "-;Player,Ally"`**: the attack ability takes the smart command on enemy and neutral units only, never on one's own or an ally's. liberty Abil:1089–1093 (`attack`) does not override it [DATA] |
| S2 | core Abil:533–561 (`CAbilMove` default) | Commands `Move`, `Patrol`, `HoldPos`, `AcquireMove` and `Turn`, each `ToSelection`. No `SmartFilters`, no `Smart` flag, no `SmartPriority` field: the move ability accepts the smart command on anything. core Abil:1371 and later `move` records (voidmulti Abil:1757–1759, balancemulti Abil:1524–1526) add only `AbilSetId Move` and the follow fields (follow plan S2) [DATA] |
| S3 | core Abil:1192–1204 (`CAbilTransport` default) | `CmdButtonArray Load`, `TargetFilters "Visible;Ally,Neutral,Enemy,Buried,Dead,Hidden"` (a transport loads only one's own units), **`AllowPassengerSmartCmd 1`** (a would-be passenger's smart command on the transport boards it) and **`AllowSmartCmd 1`** (the transport's own smart command on a unit loads it). liberty Abil:1478–1498 (`CommandCenterTransport`) sets both to 0 [DATA] |
| S4 | liberty Abil:1272–1292 (`MedivacHeal`), liberty Abil:507–515 (`MULERepair`), 1136–1146 (`Repair`) | Effect abilities with `Flags Smart 1`: a Medivac's smart command on a biological friend heals it (`SmartValidatorArray healSmartTargetFilters`, liberty Valid:1318–1320, `Filters "-;Enemy"`). An SCV's on a mechanical friend repairs it. voidmulti Abil:692–694 and balancemulti Abil:587–589 change only `MedivacHeal`'s target filters. `SmartPriority` appears only on `BattlecruiserAttackEvaluator` (voidmulti and balancemulti Abil:56–66, value 20) [DATA] |
| S5 | liberty Unit:5431, 5475–5478 (`Medivac`); liberty Unit:6867–6870 (`VikingFighter`); liberty Unit:4851–4854 (`Marine`); liberty Unit:4760–4765 (`SCV`) | Ability arrays in order: Medivac `MedivacHeal, stop, move, MedivacTransport`; Viking `stop, attack, move, AssaultMode`; Marine `stop, attack, move, Stimpack`; SCV `stop, attack, move, Repair, SCVHarvest, TerranBuild`. The later layers add only `MedivacSpeedBoost` (swarmmulti Unit:906, void Unit:1485) and sprays [DATA] |
| S6 | Blizzard SC2 forum, ["Loading medivacs question"](https://us.forums.blizzard.com/en/sc2/t/loading-medivacs-question/20190), 2021-01-21, read 2026-11-27 | With marines and Medivacs selected, right-clicking each Medivac to load: "All medivacs move towards the first medivac that I right clicked on." The advice: "Select only the ground units and right click … to load the units." Another poster: when a Medivac fills, the game "remove[s] the move command for all other marines / medivacs currently selected". This is 5.0.x era (5.0.14 is from 2024) [COMM] |
| S7 | Blizzard, [Game Guide: Special Control](https://news.blizzard.com/en-us/article/4552955/game-guide-special-control), read 2026-11-27 | Grouped production structures in a selection with units: "right-clicking to attack will reset the buildings' rally point to the location of your current army." Shift queues commands, and "the number of commands you can queue is limited" (no number) [BLIZ] |
| S8 | core Abil:671–701 (`CAbilRally` default), liberty Abil:885–897 (`RallyHatchery`) | Three `CmdButtonArray`s, `Rally1` to `Rally3`, and no field naming a waypoint count. `RallyHatchery`: `InfoArray 0` `UseFilters "-;Worker"`, `UseValidators NotQueen`, `SetValidators NotResourcesOrEnemyTargetType`; `InfoArray 1` `SetOnGround 0`, `UseFilters "Worker;-"`, `UseValidators NotQueen`; `InfoArray 2` `SetOnGround 0`, `SetValidators Failure`, no use rule. swarmmulti Abil:404, void Abil:999, voidmulti Abil:452–467 and balancemulti Abil:388 hold the record only commented out. The voidmulti comment shows a dropped design in which slot 2 was an Overlord rally (`UseValidators IsOverlord`). So 5.0.14 runs the liberty record (rallies plan S3, re-read) [DATA] |
| S9 | liberty Valid:1598–1601 (`NotQueen`), liberty Valid:667–669, 694–703 | `NotQueen`: the unit using the rally is not a Queen. `NotResourcesOrEnemyTargetType` refuses a resource target (rallies plan S4, re-read) [DATA] |
| S10 | core Abil:1028–1191 (`CAbilTrain` default); swarm Abil:197–199 (`NexusTrainMothershipCore`), 533–549 (`LocustTrain`) | The train default sets flags, `Range 2` and per-slot `AllowMinimap 0`, and **no `Offset`**. Only two train records in any layer set `Offset` (`0,0`, spawning at the trainer's centre). A grep of every layer's `CAbilTrain` records finds no `Location` or placement field. So the exit point of an ordinary trained unit is the engine's placement search, which is not in the data [DATA] |
| S11 | SC2Mapster forum threads, seen only through web-search summaries on 2026-11-27 (`sc2mapster.com` now redirects to CurseForge, so the threads could not be read) | One poster set "the Offset parameter of the Ability used to create the units" to make units "spawn on the correct side of each barracks even with no rally point set". Another wrote that the Rally ability "only has 4 rally points available. You cannot add more then 4 even if you put a duplicate rally ability on the unit." That is ambiguous between slots (buttons) and waypoints, and S8 shows three buttons in 5.0.14 [COMM, **verify**] |
| S12 | TeamLiquid, "Shift-click Rally to Units tip", 2011 (rallies plan S10) | "Just shift-click on several units (up to four)". Wings of Liberty era [COMM, **verify** for 5.0.14] |
| S13 | Blizzard, [Game Guide: Buildings](https://news.blizzard.com/en-us/article/4488317/game-guide-buildings) (production plan S7) | "set rally points on the structures' unobstructed sides". A rally on a unit makes new units "group up around the designated unit" [BLIZ] |
| S14 | Blizzard, [Warcraft III basics: Buildings](https://classic.battle.net/war3/basics/buildings.shtml) | "Rally pointed units exit the building on the same side as the destination Rally Point is set to." Another game: an analogy only [BLIZ, analogy] |
| S15 | Liquipedia, *Micro (StarCraft)*, through a web-search summary on 2026-11-27 (a direct read returned HTTP 429 again) | Set Rally Point is Y plus a click, and Set Worker Rally Point is G plus a click on a resource. These are the keys Q0115 already decided [COMM, **verify** the wording] |

Web searches made on 2026-11-27 for an SC2 rally waypoint cap, for the side a unit leaves a building with no
rally, and for the smart command's per-unit rule found nothing beyond S6, S7, S11 and S15. The queries are
listed in §15.

### 2.2 The rules that follow

1. **The smart command is resolved per unit [INF from S1 to S6].** Each unit of the selection looks at its own
   abilities and uses the first that takes the smart command on the target:
   - attack, only on an enemy or neutral unit it can attack (S1);
   - its smart effect abilities, such as heal and repair (S4);
   - harvest, build and transport smart commands (S3);
   - and last its move ability, which takes anything (S2).

   The units of one selection may so carry out different orders on one click. S6 shows it directly: a selection of
   marines and Medivacs right-clicking a Medivac sends the Medivacs to it while the marines board.

   The precedence among a unit's own abilities follows `SmartPriority` (S4: only the Battlecruiser's evaluator sets
   it) and then the ability array's order (**verify**; this plan needs it only where one unit has two abilities that
   both take the same target, which no row of D-1 has).
2. **The move fallback.** On a unit target the move ability follows the unit (follow plan §2.2 rule 1). On a
   structure it is a move to the structure (follow plan §14). On the ground it is a move. So:
   - an unarmed unit right-clicking a mobile enemy follows it;
   - a ground-only unit right-clicking an air enemy follows it;
   - a Viking right-clicking a Medivac follows it (S5: `attack` refuses one's own unit by S1; `move` takes it);
   - a soldier in a mixed selection right-clicking a mineral field walks to it.
3. **Production structures in the selection** take the click as their rally (S7). This is unchanged here
   (production plan D-10, rallies plan D-8).
4. **Which rally slot a new unit uses (B-217).** The use rules of S8 are, for the three settable or never-set
   slots:
   - slot 0: non-workers except the Queen;
   - slot 1: workers except the Queen;
   - slot 2: every unit, but its `SetValidators Failure` means no rally is ever stored there.

   For every unit, at most one slot that can hold a rally accepts it. A Drone is refused by slot 0's `-;Worker`
   whatever slot 1 holds, so there is nothing for it to "fall through" to. A non-worker is refused by slot 1. A
   Queen is accepted only by slot 2, which is always empty. Consider two engine rules: "the first accepting slot,
   empty or not" and "the first accepting slot that holds a rally". They differ only when two settable slots
   accept the same unit, which 5.0.14's data never has. So the observable behaviour is fixed by the data alone:
   - a Drone with no worker rally gets no rally order;
   - a non-worker with no army rally gets none;
   - a Queen never gets one.

   That is this project's rule (rallies plan D-4) [DATA + INF; no **verify** remains].
5. **Exit placement (B-167).** The data leaves every ordinary trainer at the engine's default `Offset` and names no
   placement field (S10). S13's "unobstructed sides" and S14's analogy are the only hints that units leave on the
   rally's side, and nothing read gives the order without a rally. This stays unsourced (**verify**) and is the
   owner's call (Q0140).
6. **The waypoint cap (B-215).** No data field counts rally waypoints (S8). The only figures are S12's "up to four"
   (2011) and S11's ambiguous "4 rally points". What a Shift-click past the cap does is unsourced. This is the
   owner's call (Q0141).
7. **Keys (B-214).** Y and G (S15, Q0115). The owner chose positional keys (Q0115 (a)).

### 2.3 Values

| Value | 5.0.14 | Here | Source |
|---|---|---|---|
| Smart precedence per unit | own abilities, then `move` | the D-1 table per member | S1 to S6 |
| Smart on own unit by an attacker | never attacks (`SmartFilters "-;Player,Ally"`) | follow, board, build, return or harvest by D-1; never `AttackTarget` | S1 |
| Rally slots that accept one unit | at most one settable | at most one (shipped data, pinned by P-4) | S8 |
| Waypoints per rally | unsourced ("up to four", 2011) | `max_waypoints: 4`, data | S11, S12; Q0141 |
| Exit order | engine placement, unsourced | clockwise from the ring's minimum corner, or nearest the rally | S10, S13, S14; Q0140 |

---

## 3. Diagnosis (code survey on `80d01ebe`)

| # | Finding | Where |
|---|---|---|
| T1 | `issue_waypoint_order` gives `Follow { Move }` (via `issue_follow_prevalidated`) for a mobile `Unit` waypoint the child neither boards (`admit_load`) nor attacks (`issue_rally_attack`). A structure stays a point `Move` | `crates/sim_core/src/production/rally.rs` |
| T2 | `give_rally_order`'s doc table still reads "else `Move` to where it stands now (B-163)" and "else `Move` to where it stands now (Q0105's rule, B-187)" for the two rows that now follow | `rally.rs`, the doc comment above `give_rally_order` |
| T3 | Tests already pin B-163's fix: `a_unit_rallied_to_a_friend_follows_it`, `a_route_of_two_own_units_is_followed_in_turn`, `a_new_unit_following_its_rally_unit_stops_when_it_dies`, `a_rally_on_an_own_structure_stays_a_point_move`, `a_unit_that_cannot_attack_follows_the_enemy` | `crates/sim_core/tests/rally_tests.rs` |
| T4 | Slot rules: `RallyAbility::slot_used_by` scans slots in order, and the first whose `used_by` and `excludes` accept the product wins (`SlotRules::used_by`; a worker is an archetype with `capabilities.harvester`) | `crates/sim_core/src/production/rally_slots.rs` |
| T5 | The three Hatchery analogues author `[(NonWorkers, excludes [hive_matron], on_resources false), (Workers, excludes [hive_matron], smart_on_ground false)]`; every other producer has one slot, and every producer has `max_waypoints: 4` | `assets/catalog/structures.ron` |
| T6 | `the_queen_analogue_uses_no_rally` already asserts that a non-worker gets no order while only the worker slot is set. No test asserts the mirror case (a worker with only the army slot set), and none asserts over the shipped catalog that no product is accepted by two slots | `rally_tests.rs`; `catalog_reference_values_tests.rs::the_shipped_rallies_are_5_0_14s` |
| T7 | The exit search reads `rally_slot_for(producer, product)` and `rally_point(producer, slot)`, and orders ring cells "clockwise from the minimum corner, or nearest the producer's rally point first" | `crates/sim_core/src/production/exits.rs` (`production_exit`, `search_exit_ring`) |
| T8 | **Right click.** `issue_pointer_order` calls `context_order(world, owner, selection, target)`, which returns **one** `(UnitOrder, OrderMarkerKind, Vec3)`. `submit_selection` then sends it to the members that `can_execute` it. Structures take `SetRally { slot: None }` in a second submission | `crates/client/src/battle_input/mod.rs` (`issue_pointer_order`); `battle_input/context_order.rs` (`context_order`, `friendly_order`); `battle_pipeline/commands.rs` (`submit_selection`) |
| T9 | `context_order`'s rows: Enemy → `AttackTarget` for the selection; Friendly → `friendly_order`, the first of `Construct`, `Load`, `ReturnCargo`, `Harvest` (extractor) and `Follow { Move }` that **any** member takes (`any_recipient`); Node → `Harvest` when any member harvests; otherwise `Move` to the ground. Every member that cannot take the chosen order is dropped: a soldier in a selection with a worker right-clicking a mineral stays put, an unarmed unit right-clicking an enemy stays put, and a Viking with marines right-clicking a Medivac stays put | `context_order.rs` |
| T10 | The A-click on a unit already splits the selection per member into attackers, followers and walkers, with one `PlayerCommand` each in the same tick, every command validated before any is captured | `battle_pipeline/commands.rs` (`submit_attack_on_unit`) |
| T11 | `resolve_target` chooses what the pointer means (unit or node), once for the whole selection. 5.0.14's cursor also picks one target for everyone | `context_order.rs` |
| T12 | `capable` already has the member-level rule for every order D-1 uses: `AttackTarget` (settled Attack, plus `can_attack` in `can_execute`), `Follow` (mobile, not the target), `Load` (mobile ground, not the carrier), `Harvest`, `ReturnCargo`, `Construct`, `Move` (mobile) | `battle_pipeline/commands.rs` (`capable`, `can_execute`, `can_receive_order`) |
| T13 | No carrier-initiated load exists: `UnitOrder::Load { carrier }` is the passenger's order, and `pursue_carrier` walks the passenger. A carrier picking up a unit would need a new order whose walk is a `walking_goal` arm in `movement/locomotion.rs` | `crates/sim_types/src/commands.rs`; `crates/sim_core/src/transport/orders.rs`; `movement/locomotion.rs` |
| T14 | Heal and repair smart abilities (S4) do not exist here. The Medivac analogue `repair_shuttle` has no weapon and no heal ability. Missing abilities are B-69 | `assets/catalog/units.ron`; `DESIGN.md` B-69 |
| T15 | Tests that pin today's one-order rule: the `context_order` unit tests (every one calls `context_order`'s single-order signature; `a_node_is_harvested_by_a_harvester_and_walked_to_by_the_rest` asserts only the harvest, and the soldier gets nothing), `an_enemy_is_attacked_by_any_selection`, `a_mobile_friend_is_followed_by_the_rest_of_the_selection`, `a_carrier_is_boarded_by_a_ground_unit` and `boarding_beats_returning_cargo_at_a_carrier_that_is_also_a_dropoff`. `battle_input_tests::only_capable_units_of_a_mixed_selection_receive_an_order` (a structure takes no move) keeps its meaning | `context_order.rs` tests; `crates/client/tests/battle_input_tests.rs`, `battle_context_order_tests.rs` |
| T16 | Measured worlds: the matrix, battles 31 and 33 and both showcase scenarios run through `matchup_trace` and `route_trace`, with no client input. The showcase tool issues no `PlayerCommand` (`client_diagnostics/src/showcase/mod.rs` doc). Only the client skirmish match (its pilot right-clicks minerals, sites and enemies) can feel a client change. No fixture contains `Produce` (grep of `golden_replay*.ron` and `matchup_matrix.ron`: 0), so no measured world or golden trains a unit, and the exit order touches only the skirmish | fixtures; `crates/client/tests/support/skirmish_pilot.rs` |
| T17 | The Gated freeze: "No other package edits `movement/`, `steering.rs`, `target_acquisition.rs`' admission test or `movement_rules.ron` while a package of this plan is open." `gated-wp2` (`45a3a5ed`) is in flight. WP2 and WP3b of that plan also touch `entity.rs` and `checksum*`, so those files are a merge risk though not frozen | `docs/plans/2026-11-27-gated-movement.md` §11, file table |

---

## 4. Design decisions

### D-1. Per-member smart resolution (B-222)

`context_order` is replaced by **`smart_plan(world, owner, selection, target) -> SmartPlan`** in
`context_order.rs`. Each member of the selection gets the first row **it** can take. A row is taken when the
existing member rule (`can_receive_order` and `can_execute`, T12) accepts that member for the row's order. The
table, per target:

| Target | Rows, in order (first the member takes wins) | Source |
|---|---|---|
| Enemy unit `e` (ground `g` below it) | `AttackTarget { e, Explicit }`; else `Follow { e, Move }` when `e` is mobile; else `Move { g }` | S1, S2, rule 2 |
| Friendly foundation `f` | `Construct { f }`; else `Move { ground }` | S2; production plan |
| Friendly completed carrier `c` | `Load { c }`; else `Follow { c, Move }` (a carrier is mobile); else nothing for `c` itself | S3, S5, S6 |
| Friendly completed dropoff | `ReturnCargo` (holds cargo); else `Move { ground }` | S2 |
| Friendly completed extractor on node `n` | `Harvest { n }`; else `Move { ground }` | rallies plan D-8 |
| Friendly mobile unit `u` | `Follow { u, Move }`; nothing for `u` itself | follow plan D-10 |
| Any other friendly structure | `Move { ground }` | S2; follow plan §14 |
| Node `n` | `Harvest { n }`; else `Move { ground }` | S2, rule 2 |
| Ground | `Move { ground }` | S2 |

A friendly target that is several of these (a carrier that is also a dropoff) offers its rows in the order
`friendly_order` uses today: `Construct`, `Load`, `ReturnCargo`, `Harvest`, `Follow`, then the ground `Move`. The
row order is unchanged. Only the question changes: from "does any member take it" to "does **this** member take
it".

Properties of the table:
- **No new mechanic.** Every order exists and is admitted by the simulation as today, and every member test is the
  one the queue already applies (T12). `Follow` on an enemy is admitted for any owner (follow plan D-2). A follower of
  an enemy it cannot attack is the only kind of enemy follower the client creates, so B-221 is not reached (follow
  plan §2.2 rule 7).
- **Q0104 and S1 agree.** A right click on one's own unit never attacks it. The A-click keeps Q0104's default.
- **Q0119 agrees.** An armed member that cannot attack a mobile enemy follows it, as the A-click does since Q0119 (b),
  except that the right click's follow is `Move`, not `AcquireMove` (S2: the smart command uses the `Move` command,
  rule 2).
- **Structures** in the selection take none of these rows, since none is `capable` for them. They take the rally as
  today (D-3).

### D-2. The plan type

```rust
/// What one right click means for a selection, member by member (B-222; StarCraft II 5.0.14
/// resolves the smart command per unit, plan `2026-12-01-rallies-and-smart-command` D-1).
pub struct SmartPlan {
    /// One entry per distinct order, in the D-1 table's row order; recipients in selection order,
    /// disjoint across entries, never empty.
    pub orders: Vec<SmartOrder>,
    /// Where the click's marker shows, in Bevy world space: the ground under the target.
    pub marker_at: Vec3,
}

/// One order of a [`SmartPlan`] and the members that take it.
pub struct SmartOrder {
    /// The order, as the simulation admits it.
    pub order: UnitOrder,
    /// The members of the selection that carry it out.
    pub recipients: Vec<EntityId>,
}
```

- **Determinism.** Members are visited in selection order. Each order's entry is created the first time a member
  takes it, and entries are then sorted by row index (a small array index per row, no hash). Equal inputs give an
  equal plan.
- **Marker.** `SmartPlan::marker()` is `OrderMarkerKind::Attack` when the first entry is an `AttackTarget`, and
  `Move` otherwise, at `marker_at`. That is today's choice for a selection that attacks, and today's choice for every
  other click.
- **Cost.** One pass over the selection, at most six rows per member, each an O(1) member test. It runs on a right
  click, never per frame. Its `Vec`s are allocated by the user action, as `submit_attack_on_unit`'s are ("Allocation
  belongs to the user action", T10). No `// PERFORMANCE RATIONALE:` is needed.

### D-3. Submission

- `BattleCommandQueue::submit_smart(world, player, &SmartPlan, queued) -> Result<SubmissionSequence, SubmissionError>`
  follows `submit_attack_on_unit`'s shape (T10):
  - `accepts_orders` first;
  - **every** entry's `PlayerCommand` is `validate`d before any is captured, so a refusal leaves the queue as it was;
  - then each is captured in plan order;
  - it returns the last sequence.

  An empty plan is `SubmissionError::EmptySelection`.
- `issue_pointer_order` builds the plan, then submits the rally to the structures as today. `orders` becomes
  `!rallies || !plan.orders.is_empty()`. The match on `(ordered, rallied)` is unchanged, and the marker comes from
  the plan.
- **Shift** applies the click's `queue` to every entry, as today.
- **The simulation is unchanged.** The bundle carries up to three commands for the units, plus the rally, with
  disjoint recipients. Route groups form per `Move` command (`form_route_group`), so the `Move` entry's members
  share a group as a one-order click's members did.
- **Docs.** The module doc of `context_order.rs` states the new invariant: "each member takes the first row it can
  carry out; a click gives one command per distinct order". The table moves to `smart_plan`'s doc.

### D-4. Rally slot choice (B-217): proved from the data, no code change

- **No code change.** `slot_used_by` (T4) stays. §2.2 rule 4 shows that for 5.0.14's data the engine's possible
  rules cannot be told apart, and the shipped data mirrors it (T5).
- **Pinned by two tests (WP2):**
  - `catalog_reference_values_tests::no_shipped_product_is_taken_by_two_rally_slots`. For every shipped producer and
    every product in its `trains_units`, count the slots whose `used_by` and `excludes` accept the product, a worker
    being an archetype with `capabilities.harvester`. The count is at most 1. It is exactly 0 only for the product
    every slot excludes (the Queen analogue), and that product is found by `sc2_analogue`, not by slug. The test is
    an oracle written from S8 against public catalog fields, independent of `rally_slots.rs`.
  - `rally_tests::a_worker_gets_no_order_while_only_the_army_rally_is_set`. On the Hatchery analogue, clear the worker
    slot (a `SetRally` on itself with `slot: Some(1)`), set the army slot on a point, and train a worker. It gets no
    order. This is the mirror of T6's existing non-worker test.
- **Row.** B-217 is closed as "matches 5.0.14 by the data (`RallyHatchery`'s use rules are disjoint, S8)".

### D-5. Keys (B-214): owner-sanctioned

- **Q0052 gains an addendum.** "(2026-12-01): rally card entries keep positional keys, from Q0115. Sanctioned by
  the owner's answer to Q0115, option (a), for plan `2026-11-25-rallies`: **S-rally-keys:** the rally card entries
  take the key of the card position they land on, like every other entry. StarCraft II 5.0.14's standard keys are Y
  (rally) and G (worker rally)."
- B-214 is removed from §4's table. The register's preamble says accepted differences are not listed; its
  history stays in the rallies plan and Q0115.
- No code change.

### D-6. Exit placement (B-167): the owner's call

- **Default (Q0140 (b)): unchanged.** The row stays open, re-worded with S10: "the data leaves every ordinary
  trainer at the engine's default `Offset` (`core` AbilData:1028–1191; only `NexusTrainMothershipCore` and
  `LocustTrain` set it)".
- **If (a):** a Q0052 addendum "S-exit-order", and the row is removed (docs only, WP3).
- **If (c), the owner reports the side and the order observed in game:** WP3 makes the start of the ring scan data.
  `movement_rules.ron` is frozen and is the wrong home anyway, so the field goes into `structures.ron` as an optional
  per-archetype `exit: Some((first: <Side>, turn: Clockwise | CounterClockwise))`. A world default sits next to the
  production rules already in `structures.ron`'s header, or a new `production_rules` section if none exists
  (**verify** at WP3, on the tree of the day). `Side` is `MinCorner` (today), `South`, `North`, `East` or `West` in
  map axes, documented with the coordinate convention.
  - Absent means today's rule, so every unchanged row behaves the same.
  - The rally-nearest ordering stays first whenever a rally resolves (S13, S14).
  - Validation is a typed `CatalogInvalidValue`.
  - `runtime_catalog_tests` shows a catalog copy turning the start in the same binary.

  The outer-ring search is not changed in any option. Nothing sources the game's search, and B-66's "may become
  stuck" rule stays.

### D-7. The waypoint cap (B-215): the owner's call

- **Default (Q0141 (b)): unchanged.** `max_waypoints: 4`, a full route refuses a further Shift-click
  (`RallyWaypointsFull`), and the row stays open, re-worded with S8 and S11.
- **If (a):** a Q0052 addendum "S-rally-cap", and the row is removed (docs only).
- **If (c), the owner reports a count N and what a Shift-click past it does:**
  - `max_waypoints: N` on every producer row, and the reference test reads N from the answer's citation.
  - When the answer is "refused", nothing else changes.
  - When it is "the newest replaces the last" or "the oldest is dropped", `RallySpec` gains `#[serde(default)]
    when_full: RallyWhenFull { Refuse (default), ReplaceLast, DropFirst }`, read by `rally_write`. It is catalog
    data, not hashed. Routes are already hashed (schema 48 of the rallies plan), so no schema change follows.
  - `RallyWaypointsFull` then occurs only under `Refuse`.

### D-8. Records

WP0 registers B-247 with the next free number:

- **B-247 (open, deferred until the Gated packages close):** "A transport's right click on one of its owner's
  ground units does not pick it up. 5.0.14's `CAbilTransport` has `AllowSmartCmd 1` (`core` AbilData:1192–1204), so
  a Medivac right-clicking a marine loads it, and the Medivac flies to it. Here a carrier member follows the unit
  (D-1's `Follow` row), since no carrier-initiated load order exists, and adding one is a movement change
  (`walking_goal`)." Plan WP-D1.

The heal and repair smart abilities (S4) are not modelled at all (T14). They belong to B-69, and this plan adds
nothing for them (§14).

When this plan is done:
- B-163 is **fixed** (records, WP0; doc, WP2).
- B-214 is removed as sanctioned (WP0).
- B-217 is **fixed** as matching the data (WP2).
- B-222 is **fixed** except its pickup half, which is B-247 (WP1).
- B-167 and B-215 follow Q0140 and Q0141 (WP3).

---

## 5. Determinism and checksum

- **WP0 to WP3 change no hashed state and no checksum format.**
  - WP1 is client-only. Its commands are ordinary `PlayerCommand`s, and their order within a bundle is
    deterministic (D-2).
  - WP2 changes a doc comment and adds tests.
  - WP3's options are catalog data (D-6, D-7), not hashed. Routes and positions are already hashed.

  So no golden is re-recorded in WP0 to WP3, and a golden that moves is a stop (§8 rule 2). Neither golden trains a
  unit or sets a rally (T16), so WP3's exit and cap data cannot reach them.
- **Exit order (WP3 (c) only).** The scan start is a fixed index into the ring, and the turn direction is a fixed
  iteration order. Both are integer. Distances to the rally stay raw squared fixed point.
- **WP-D1 (deferred) is the one package with a schema step.** Its new order variant `UnitOrder::LoadUnit { passenger }`
  takes the next `UnitOrder` tag, and the carrier's walk may need a `MovementMemory` field. The schema takes the
  **next free number at merge** (61 on `80d01ebe`; other packages may take 62 and 63 first). The proof follows the
  rallies and follow plans:
  1. With the new variant and any new field present but **not encoded**, and `CHECKSUM_SCHEMA_VERSION` at its old
     number, both goldens (`golden_replay`, `golden_replay_orders`) and `DEFAULT_SANDBOX_CHECKSUM` reproduce every
     recorded checksum on four builds:
     - native debug and release;
     - `wasm32-wasip1` release and debug, with `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`
       naming the tree's real path, not a symlink.
  2. Then the encoding and the schema go in, and the checksums are re-recorded under a header "Re-recorded for plan
     2026-12-01-rallies-and-smart-command WP-D1 (checksum schema N−1 -> N): the `LoadUnit` order's tag (unreachable
     in these worlds) [and each unit's …]. No behaviour changed: with the new state unencoded at schema N−1, every
     checkpoint stays as recorded."
  3. The schema doc line in `checksum.rs` and the schema-byte tests move with it.
- **wasm.** No package adds a dependency. The simulation crates still build for `wasm32-unknown-unknown`.
- **Cost.** WP1 runs only on a click. WP3 (c) changes a constant start index. Per tick nothing new runs, so the
  throughput bench is unaffected. WP3 runs it once if it touches `sim_core` code, comparing user-space instructions
  within 3 % of WP0's tree (rallies plan P-8's method).

---

## 6. Acceptance (principled properties, fixed before implementation)

Each property is a property of the rule, checked on constructed worlds or on the shipped catalog. None is a
seed's figure. Each test that changes behaviour is written first and shown red on the tree before its package's
change. Tests that pin behaviour already right (WP2) are shown red against a scratch mutation of the rule (named
per test, not committed).

- **P-0 (probe, WP0, scratch only).** A temporary counter in a scratch copy of `issue_pointer_order` (never
  committed). It runs the client skirmish match on WP0's tree with today's code and counts the right clicks whose
  per-member plan (D-1) would differ from today's single order: a member that would get an order today's rule drops,
  or a different one. Expected: zero. The pilot's selections are its workers alone or its army alone (skirmish pilot
  `plan_*` stages). A non-zero count rewrites §7's skirmish row from the trace before WP1 (stop rule 1). The probe's
  result and the measured-world baselines go into the evidence.
- **P-1 (per-member table, WP1)** (`context_order` unit tests, rewritten onto `smart_plan`):
  - Enemy, with a selection of an armed ground unit, an unarmed air unit and a ground-only unit against an air
    target: `AttackTarget` to the first; `Follow { Move }` to the second and third.
  - Enemy structure, with an unarmed member: `Move` to the ground below it.
  - Own carrier, with a ground unit, an air unit and the carrier itself: `Load`; `Follow { Move }`; nothing for the
    carrier.
  - Node, with a worker and a soldier: `Harvest` to the worker; `Move` to the ground to the soldier. This is
    `a_node_is_harvested_by_a_harvester_and_walked_to_by_the_rest`, now asserting its title.
  - Foundation, with a builder and a soldier: `Construct`; `Move`.
  - Dropoff, with a worker holding cargo, a worker without and a soldier: `ReturnCargo`; `Move`; `Move` (one entry).
  - Own extractor, with a worker and a soldier: `Harvest` of the node; `Move`.
  - Own mobile unit, with that unit in the selection: `Follow` for the others, nothing for it.
  - Ground: one `Move` entry with every mobile member.
  - **Exclusivity:** the entries' recipient sets are disjoint, their union is exactly the members some row
    accepts, and entries come in table-row order.
  - **Agreement with today:** every member test is today's `can_execute`. A selection whose members all take the
    same row gives a one-entry plan equal to today's order.
- **P-2 (submission, WP1)** (`battle_input_tests`, `battle_context_order_tests`, through the real input path):
  - A mixed selection (two marines and a Medivac-class transport and a Viking-class air unit, chosen from the
    catalog by capability) right-clicking the transport queues `Load` for the marines and `Follow { Move }` for the
    air unit, in **one tick's bundle**, and the marker shows. With Shift, every entry has `queue: true`.
  - A worker and a soldier right-clicking a mineral: `Harvest` and `Move`, one tick. 200 ticks later the worker
    harvests and the soldier stands within arrival of the field.
  - An unarmed unit and an armed one right-clicking an enemy: `AttackTarget` and `Follow { Move }`. The follower's
    front order is the follow on the next tick.
  - **No partial queue:** a plan with one entry the simulation's `validate` refuses queues nothing (a constructed
    stale entry, e.g. a member that died after the plan was built).
  - **Mixed with structures:** a producer in the selection still takes the rally, and the units still take their
    entries (today's `a_mixed_selection_orders_its_units_and_rallies_its_structures` keeps passing).
- **P-3 (B-163 record, WP2).** `rally_tests::the_rally_order_table_is_as_documented`. One constructed base covers
  the doc table's rows: point, node harvested, node walked, own carrier boarded, own mobile followed, own structure
  walked, enemy attacked, enemy followed, carried unit (none). It asserts each child's front order on its spawn
  tick. The doc table of `give_rally_order` lists the same rows in the same order; the test's doc names them.
- **P-4 (B-217, WP2).** D-4's two tests. Red by mutation: `slot_used_by` changed to skip an empty slot
  (fall-through) makes `a_worker_gets_no_order_while_only_the_army_rally_is_set` fail. An extra `All` slot in a
  catalog copy makes the oracle fail.
- **P-5 (WP3, by answer).**
  - (c) for Q0140: `exit_tests` give a constructed producer with no rally; the first exit cell is the authored side's
    first cell, then the turn. A rally still wins. `runtime_catalog_tests` turns the start in a catalog copy.
  - (c) for Q0141: `shift_appends_waypoints_up_to_the_cap` reads N; the full-route rule's test per `when_full`
    value; `runtime_catalog_tests` changes `when_full` in a copy.
  - (a) or (b): no test change.
- **P-6 (client steady state and allocation, WP1).**
  - `sync_steady_state_tests::frames_after_a_mixed_right_click_create_modify_and_remove_nothing`. The skirmish
    harness gives a mixed selection a right click that yields three entries. After the marker has shown, 60 running
    frames add no asset, entity or retained buffer, through `retained_buffers` and the asset tallies, and the order
    marker pool keeps its size.
  - `sync_allocation_count_tests` gains a variant `smart_mixed` (valgrind). The click lands before the counted
    window, so followers, boarders and walkers move during it. The live battle sync must count zero allocations in
    the window, as the other variants do.
- **P-7 (GPU).**
  - WP1 draws nothing new. The marker is the existing pooled one, and its kind follows D-2. No new GPU child is
    needed. The marker's drawing is proved by the existing pool tests, and the order by P-2.
  - WP3 (c) for Q0141 with `ReplaceLast` or `DropFirst` changes the drawn route. Then `rally_gpu_tests` gains a shot
    after a Shift-click past the cap, asserting the reticles and lines of the authored rule. That is one GPU child,
    which locks itself through `crates/client/tests/support/gpu_run.rs`: radv locally, lavapipe in CI.
- **P-8 (worlds).** §7.

---

## 7. Measurement plan: worlds that move

Every package that changes code runs the comparison against `main` at its base. The commands are those of
`~/scratch/resource-footprints/scripts/measure.sh`:
- the matrix with its seeds of record and 16;
- battle 31 on 100 seeds;
- battle 33 on 64 seeds;
- `showcase.ron` on the default seed and seeds 1 to 8, for 6,000 ticks;
- `showcase_effects`, as the follow plan's probe did.

The client skirmish match is run through `skirmish_match_tests` with its stage log. Outputs go under
`~/scratch/rallies-smart/<wp>/`.

| World | WP0 | WP1 | WP2 | WP3 (a)/(b) | WP3 (c) |
|---|---|---|---|---|---|
| Matrix, battles 31 and 33, showcase and `showcase_effects` (state outputs) | baseline | byte-identical (no `sim_*` file in the diff; `git diff --stat` proves it, and the run confirms it) | byte-identical | — (docs) | byte-identical (no world trains a unit, T16) |
| `golden_replay`, `golden_replay_orders`, `DEFAULT_SANDBOX_CHECKSUM` | — | unchanged | unchanged | — | unchanged |
| Client skirmish match | baseline stage log, P-0 count | stage log identical if P-0 is zero | identical | — | **may move**: the skirmish trains 7 units at the Barracks analogue without a rally (rallies plan As built, WP0), so a new exit start moves their first cells |

- **A world that moves where the table says byte-identical** is a defect of the package, never a re-record (stop
  rule 2). A first-difference trace finds the first tick and entity that differ (`matchup_trace` / `route_trace`
  with `--from-tick` dumps, as in the follow plan's evidence §1), and the trace goes to the orchestrator.
- **The skirmish under WP3 (c).** A first-difference trace gives the first tick, unit and position that differ. The
  match must still be won within `TICK_LIMIT`, stage for stage. Its result is a property of the pilot, not a claim
  about StarCraft II. If the time to win shifts, the evidence restates it with the new figure and the reason (the
  exit side). This follows the owner's 2026-11-24 note: an exit change that moves a close outcome is recorded and
  restated, not reverted, unless a unit's behaviour is broken (units stuck at the exit, for example).
- **Claims restated per §8.** WP1 changes no battle claim: no measured battle takes client input. A claim of
  `DESIGN.md` §5 that WP3 (c) would move (none known: the skirmish is not a matchup claim) is restated in the same
  package with the trace.

---

## 8. Stop rules

1. **P-0 finds a right click whose per-member plan differs** from today's order in the skirmish. Stop before WP1 and
   rewrite §7's skirmish row from the trace (orchestrator).
2. **A world marked byte-identical, or a golden, differs.** Stop with the first differing tick and entity. Do not
   re-record.
3. **A package needs a file the Gated freeze names** (`movement/`, `steering.rs`, `target_acquisition.rs`' admission
   test, `movement_rules.ron`). Stop and report. The change waits for Gated or lands on `main` first by the
   orchestrator's note.
4. **An existing test other than those T15 lists fails** in a way that is not the signature change of
   `context_order` → `smart_plan`. Stop and report the test and why.
5. **A member rule the client and the simulation disagree on** (a plan entry the simulation refuses on a live,
   capable member). Stop. `capable` must mirror admission (follow plan D-10). The fix is in `capable`, with a test.
6. **WP3 without an answer.** WP3 is built only for an answered option. Under the defaults it is docs only, and an
   implementer never picks (c)'s values.
7. **A source read during implementation contradicts a §2.2 rule**, for example Liquipedia readable again with a
   different smart rule, or a waypoint cap in 5.0.14. Stop and amend.
8. **WP-D1 before the Gated packages close.** Not started. When it starts, its schema number collides: take the next
   free one and re-record by bytes only with the §5 proof.

---

## 9. Client consequences (WP1)

- `battle_input/context_order.rs`:
  - `context_order` → `smart_plan` (D-1, D-2), with `SmartPlan` and `SmartOrder`;
  - `friendly_order` → `friendly_rows`, which returns the rows instead of choosing among them;
  - a member-level `takes(world, owner, member, order, subject)`, extracted from `any_recipient`'s closure into
    `battle_pipeline/commands.rs` and made `pub(crate)`, so both use one rule;
  - the module doc and the table.
- `battle_input/mod.rs`: `issue_pointer_order` submits the plan (D-3).
- `battle_pipeline/commands.rs`: `submit_smart` (D-3); `takes`.
- `input_bindings.rs`: `BATTLE_CONTROLS_HELP` gains "Right click: each unit does what it can with the target" if
  the help lists the right click (**verify** the text at WP1).
- Exhaustive matches: none new (no new `UnitOrder`).
- Tests: P-1, P-2, P-6. The T15 tests move to the new signature in the same commit. The commit message names each
  test whose assertion changes and why.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP2, WP3, WP4 | B-163 fixed; B-214 removed; B-247 registered; B-167 and B-215 re-worded; B-217, B-222 fixed; the HUD/input row mentions per-unit right clicks |
| `docs/questions/closed/0052-sanctioned-sc2-deviations.md` | WP0 (S-rally-keys), WP3 (S-exit-order, S-rally-cap if (a)) | Addenda |
| `docs/questions/open/0140-*.md`, `0141-*.md` | WP0, if the orchestrator files them | §16's drafts |
| `crates/client/src/battle_input/context_order.rs`, `battle_input/mod.rs`, `battle_pipeline/commands.rs`, `input_bindings.rs` | WP1 | D-1 to D-3 |
| `crates/client/tests/battle_context_order_tests.rs`, `battle_input_tests.rs`, `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` | WP1 | P-1, P-2, P-6 |
| `crates/sim_core/src/production/rally.rs` | WP2 | The doc table only |
| `crates/sim_core/tests/rally_tests.rs`, `catalog_reference_values_tests.rs` | WP2 | P-3, P-4 |
| `assets/catalog/structures.ron`, `crates/sim_types` (`RallySpec::when_full`, the exit spec), `crates/sim_core/src/catalog*`, `production/exits.rs`, `production/rally.rs`, `crates/sim_core/tests/{rally_tests,runtime_catalog_tests,catalog_reference_values_tests}.rs`, `crates/client/tests/rally_gpu_tests.rs` | WP3, only under an answer (c) | D-6, D-7 |
| `docs/evidence/2026-12-01-rallies-and-smart-command.md` | WP0 to WP4 | Probe, baselines, comparisons, traces |
| `docs/plans/README.md` | this commit, WP4 | Row |

**Not touched by WP0 to WP4:** `crates/sim_core/src/movement/**`, `steering.rs`, `target_acquisition.rs`,
`assets/catalog/movement_rules.ron`, `entity.rs`, `checksum*`. WP-D1 is the only package that would touch any of
them, and it waits (§14).

---

## 11. Work packages (commit order, by value)

Each package is one branch from `main` in its own worktree, built with
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`:
- each behaviour test is written and shown red first;
- the implementer runs `cargo gate quick` and the tests its change needs, then hands back;
- a verifier runs `cargo gate package` on the final tree after merging `main` (Q0061);
- workspace `cargo test` outside the gate runs under `flock ~/scratch/gate.lock`;
- GPU children lock themselves (`gpu_run.rs`), so no test run is wrapped in `with-gpu-lock`.

Every package is independently mergeable. WP1 needs nothing from WP0 except P-0's count, and WP2 and WP3 need
nothing from WP1.

### WP0: records, probe and baselines (S, docs only; scratch probe)

- Renumber B-247, Q0140 and Q0141 to the next free numbers, here and everywhere.
- `DESIGN.md` §4:
  - B-163's status becomes "**fixed** by plan `2026-11-26-follow-order` WP3 (`0a797bab`) and WP4 (`27604c11`) …";
    "open;" and "stays open under …" are struck, and the row points here;
  - B-214 is removed (D-5);
  - B-247 is registered (D-8);
  - B-167, B-215, B-217 and B-222 point here.
- The Q0052 addendum S-rally-keys (D-5).
- §16's drafts filed as Q0140 and Q0141 if the orchestrator agrees (they are not blocking).
- P-0 and the §7 baselines into the evidence.
- **Gate:** docs-only `package`.

### WP1: per-unit smart command (M, `client`) — B-222

- Red first: P-1 (unit tests against `smart_plan`, which do not compile before it exists, so the red run is the old
  tests' assertions rewritten as the new expectations against a stub returning today's single order), P-2 and P-6.
- Then D-1 to D-3 and §9.
- **Acceptance:** P-1, P-2, P-6, §7's WP1 column, and the skirmish stage log identical. The client package gate runs
  the valgrind and steady-state tests, since the change touches their triggers. B-222 is fixed except B-247.

### WP2: rally records proved (S, `sim_core` tests and one doc comment) — B-163, B-217

- P-3 and P-4, each shown red by its named mutation.
- The `give_rally_order` doc table rewritten to T1's rows.
- **Acceptance:** P-3, P-4, §7's WP2 column (the measure script, since a `sim_core` source file changes). B-217
  is fixed, and B-163's code record is complete.

### WP3: exit order and waypoint cap by the owner's answers (S; docs, or M under (c)) — B-167, B-215

- Under (a): the Q0052 addenda, and the rows removed.
- Under (b), the default: the rows re-worded with S8, S10 and S11.
- Under (c): D-6 and/or D-7, P-5, and P-7's GPU shot if `when_full` changes the drawn route. §7's skirmish
  first-difference trace and restatement.
- **Gate:** docs-only `package`, or the full `package` under (c).

### WP4: bookkeeping (S, docs)

- `DESIGN.md` rows with commits; the HUD/input row's sentence on the per-unit right click.
- The evidence completed. This plan's `## As built`. README row `Done`, or `Partly done` naming WP-D1.
- Pointers from the follow plan's §14 (B-222) and the rallies plan's D-9 (B-167) to this plan.

### WP-D1 (deferred until the Gated packages close): a transport picks up a unit (M–L; `sim_types`, `sim_core`, `client`) — B-247

An outline only. It is re-planned as an amendment when it starts, against the movement code of the day.
- `UnitOrder::LoadUnit { passenger }` (5.0.14 `CAbilTransport` `Load` with a unit target, S3).
- Admission:
  - a completed carrier of the player with free capacity;
  - a live, unloaded, uncarried ground passenger of the player that `admit_load` would accept for this carrier.
- The walk: the carrier pursues the passenger as `pursue_carrier` pursues a carrier, re-aiming beyond
  `BOARDING_REACH`, through a `walking_goal` arm. The passenger keeps its orders until boarded [INF, **verify**].
- D-1's carrier row gains `LoadUnit` before `Follow` for a carrier member right-clicking an own ground unit (S3, S5:
  `MedivacTransport` comes after `MedivacHeal`, and heal does not exist here).
- §5's schema step and 4-build proof.
- Tests:
  - admission refusals;
  - pickup of a standing and of a walking passenger;
  - capacity full, which ends the order (S6's "remove the move command");
  - the right click;
  - a P-6-style steady-state variant.

---

## 12. Test plan (summary)

| Concern | Property | Where |
|---|---|---|
| Per-member table | Rows per target, exclusivity, row order, agreement with today | WP1: `context_order` unit tests (P-1) |
| Submission | One bundle, disjoint recipients, Shift, no partial queue, rally alongside | WP1: `battle_input_tests`, `battle_context_order_tests` (P-2) |
| Client steady state | No asset, entity or buffer churn after a mixed click | WP1: `sync_steady_state_tests` (P-6) |
| Client allocation | Zero allocations in the live battle sync with mixed orders running | WP1: `sync_allocation_count_tests` `smart_mixed` (valgrind, P-6) |
| GPU | Nothing new drawn in WP1; WP3 (c) with a changed full rule: `rally_gpu_tests` shot | P-7 |
| Rally table | The doc table's rows on spawn ticks | WP2: `rally_tests` (P-3) |
| Slot choice | Shipped slots never overlap; no fall-through for a worker | WP2: `catalog_reference_values_tests`, `rally_tests` (P-4) |
| Exit, cap (by answer) | Authored start and turn; cap N and the full rule; run-time data | WP3: `exit_tests`/`rally_tests`, `runtime_catalog_tests` (P-5) |
| Worlds | §7 | WP0 to WP3 |
| Schema (deferred) | 4-build bytes-only proof | WP-D1 |

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The per-unit precedence within one unit's abilities (§2.2 rule 1) differs in some case | A member takes a different row than in game | No D-1 row gives one member two accepting rows except the carrier-and-dropoff target, whose order is today's. The table is one function |
| R-2 | Splitting one click into several commands changes route grouping | Walkers bunch differently | Members that took the same order share one `Move` command as before. New `Move` members (soldiers at a node) were idle before, so no world that measured them exists (P-0) |
| R-3 | `capable` drifts from the simulation's admission | A queued entry is refused for a live member | Stop rule 5; P-2's integration asserts the orders land |
| R-4 | Following an enemy into fog with an unarmed unit | It walks to the last-seen point and stops (follow plan D-5); a player may expect it to keep going | That is 5.0.14's rule as the follow plan read it (**verify** there) |
| R-5 | WP3 (c) moves the skirmish | Stage timing changes | §7's trace and restatement; stop if the match is lost or units stick |
| R-6 | Q0140 and Q0141 stay unanswered | Rows stay open | They are minor and recorded; nothing waits on them (owner's 2026-11-23 note) |
| R-7 | Number collisions with `diagonal-cliffs-plan` and other branches | Renumbering | Provisional numbers, WP0 renumbers |
| R-8 | `client` exhaustive or signature changes ripple into `client_diagnostics` | Build failure | `context_order` is used only in `battle_input` (grep at WP1; the showcase issues no commands) |

---

## 14. Out of scope and decisions recorded (Q0002)

- **B-247, a transport picking up a unit (WP-D1).** It needs a new walk (`movement/locomotion.rs`), which the Gated
  freeze forbids. Until then a carrier member right-clicking an own ground unit follows it (D-1).
- **Heal and repair smart commands** (S4). The abilities do not exist (T14, B-69).
- **B-221** (`FollowAcquireRange`). Unchanged: the right click creates followers only of enemies they cannot attack
  (D-1).
- **The target choice under the pointer** (`resolve_target`, T11) stays one choice for the whole selection, as the
  game's cursor makes one.
- **The status line for a mixed click** reports the first refusal, as `submit_attack_on_unit` does. A per-entry
  report is presentation and is not needed for 5.0.14 parity.
- **The outer-ring exit search** (B-66) is unchanged under every Q0140 option (D-6).
- **The Hatchery's third slot** stays unauthored (rallies plan §14). §2.2 rule 4 shows that it never changes an
  outcome.
- **Order-path lines** (5.0.14's waypoint display) are still not drawn (follow plan §14).

---

## 15. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/{AbilData,UnitData,ValidatorData}.xml`
  (`5444e12f13`), read as data. Every line cited in §2.1 was read on 2026-11-27, and the later layers were grepped
  for overrides of each record.
- Web, 2026-11-27:
  - read: S6, S7, the sc2tips 2011 shift-rally post (no cap named) and Blizzard's rally-point-control thread (no
    cap named);
  - refused: Liquipedia (HTTP 429), and SC2Mapster threads, which redirect to CurseForge;
  - searched without a further source: "SmartPriority" smart resolution; a shift-click rally waypoint cap; the exit
    side without a rally; Y and G keys.
- No prototype. P-0 is the only scratch code, under `~/scratch/rallies-smart/`.

---

## 16. Owner question drafts (provisional numbers, not filed)

```markdown
# Q0140: Where should a newly trained unit leave its building when no rally is set?

- **Asked:** 2026-12-01
- **Blocking:** no
- **Default until answered:** (b), keep today's rule and keep B-167 open

## Question

When a building finishes a unit, the unit appears on a free spot just outside it. With a rally set, Space Scuffle
picks the free spot nearest the rally, which StarCraft II also seems to do. Without a rally, it tries the spots
clockwise from the building's corner with the smallest map coordinates, and if all are blocked it searches wider rings.
Both rules are this project's own: StarCraft II's game data does not contain the placement search (every
ordinary training ability leaves its `Offset` at the engine default), and no guide or forum we could read
describes it. Today no recorded battle trains units, so only the skirmish match notices.

- (a) Accept today's rule as a recorded deviation, like the other sanctioned ones (Q0052), and close B-167.
- (b) Keep it as an open minor difference until someone can check the game (default).
- (c) Tell us what you see in StarCraft II (for example: "a Barracks with no rally drops units at its bottom
  edge, left to right"), and we make the start side and direction data in `structures.ron`, per building if needed.

Recommendation: (a), unless you can check in game, then (c).

## Answer

```

```markdown
# Q0141: How many rally waypoints may a building hold, and what does one more Shift-click do?

- **Asked:** 2026-12-01
- **Blocking:** no
- **Default until answered:** (b), keep 4 waypoints and refuse a further Shift-click; keep B-215 open

## Question

Shift-clicking with a production building selected adds rally waypoints; a new unit walks them in order. Space
Scuffle allows 4 per rally, read from data (`max_waypoints` in `structures.ron`), and refuses a fifth. The 4 comes
from a 2011 community tip ("shift-click on several units (up to four)"); StarCraft II 5.0.14's data has no such
field, and no current source gives the number or says what a Shift-click past it does.

- (a) Accept 4 and the refusal as a recorded deviation (Q0052), and close B-215.
- (b) Keep them as an open minor difference (default).
- (c) Tell us what you see in StarCraft II: how many Shift-click rally targets a building keeps, and whether one
  more is ignored, replaces the last, or pushes out the first. We change the data (and, if it is not "ignored", add
  a data switch for the rule).

Recommendation: (c) if you can check in game; otherwise (a).

## Answer

```
