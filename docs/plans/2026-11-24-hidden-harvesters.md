# Blueprint: Gas Gatherers Hidden Inside the Extractor (B-161)

This plan closes B-161 of the register against StarCraft II 5.0.14:

- **The gas gatherer is hidden inside its extractor while it gathers.** Every 5.0.14 extractor carries a resource
  behaviour with `Flags HideHarvesters 1` (S1, S2). Here the gatherer stands at the extractor's edge for its whole
  `HarvestTime`, where it is seen, picked, acquired, shot and splashed.
- **Hidden is simulation state, not presentation.** The engine's unit filter `Hidden` is excluded by 144 of the
  146 weapon target filters, by the attack ability's acquisition, by 199 of the 247 effect searches (every modelled
  splash among them), by the transport's load filter and by the "caster is not hidden" validator (S3 to S8). This
  project already reads one kind of hidden unit that way: transport cargo, which `effects/filter.rs` documents as
  "the editor's default `Dead` and `Hidden` exclusions". A gatherer inside an extractor becomes the second kind.
- **The rule is capability data on the structure.** In 5.0.14 `HideHarvesters` sits on the resource behaviour that
  the Refinery, Assimilator and Extractor carry (S2). Here each `structures.ron` row authors
  `hides_harvesters`, true for the three extractor analogues and false elsewhere. No code names a structure.
- **Nothing about timing changes.** The worker is inside for exactly its gather, the kind's `harvest_time_ticks`
  (35 for gas). It comes out where it went in, at the take, and the line, the hand-off and income are unchanged.

What 5.0.14's data does not state (the extractor dying with a worker inside, the worker's sight, where it comes out,
whether it can be selected) is marked **verify** and put to the owner in two draft questions (§15). The plan
follows a stated default for each meanwhile.

A full plan is needed (charter §6): it is a new mechanic, adds hashed state (the checksum schema), re-records both
goldens for the schema, and spans `sim_types`, `sim_core`, `assets/catalog` and `client`.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-24 against `main` at
`e313268f` (checksum schema 41).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- The checksum schema is "next free at merge": 42 and 43 are reserved by other branches, so this plan proposes
  **44**, and the merge takes whatever number is next then.
- B-196 is provisional. The next free number at WP0's commit replaces it, and every mention in this plan is
  renumbered in that commit.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations (S-1: RON data instead of editor data) | Closed | D-1 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0002: scoping is the agent's call if written down | Standing rule | §14 |
| Q0100: the hand-off between two gatherers (open, default (a), adopted on `main`) | Unaffected | D-2: the hand-off is the node's, inside or not |
| The owner's guidance: no fitting to fixed seed sets; acceptance by principled properties | Followed | §6 |
| **QA = Q0107: what happens to a worker inside an extractor (the extractor's death, its sight, where it comes out)?** | Filed (§15) | D-5, D-6, D-7; default: it survives and comes out where it went in, and it sees nothing while inside |
| **QB = Q0108: can a worker inside an extractor be selected or ordered?** | Filed (§15) | D-8, D-10; default: not selectable, it leaves the selection, control groups keep it; an order that reaches it brings it out at once |

---

## 1. Summary

1. **Data (D-1).** `ArchetypeCapabilities` gains `hides_harvesters: bool`, authored as a required field on every
   `structures.ron` row next to `is_gas_extractor`. The Refinery, Assimilator and Extractor analogues author `true`
   with their S2 citations. Catalog validation refuses `hides_harvesters` on a structure that is not a gas
   extractor (typed `CatalogError`).
2. **State (D-3).** `UnitTask` gains `housed_in: Option<EntityId>`, the structure a gatherer is inside. It is set
   when the line grants a worker a node whose site is a structure that hides harvesters, and cleared at the take,
   by every new leg (`fresh_leg`) and when the structure dies (the reap phase). Hashed; checksum schema 44 (next
   free at merge).
3. **One predicate for the engine's `Hidden` (D-4).** `Entity::is_hidden()` is `is_loaded_in_transport() ||
   is_housed()`. Every reader that stands for a search filtering `Hidden` takes it: damage, acquisition,
   call-for-help, weapons, effect searches, sight, casts, cloak and attack reveals, production exits and
   construction sites. Readers about carrier topology, ownership counts and order admission keep
   `is_loaded_in_transport()` (§3.3's table names each one).
4. **The worker's life inside (D-5 to D-8).** Inside for the gather only; it comes out where it went in; it keeps no
   sight; it survives its extractor's death and comes out on that tick; an order that reaches it ends the gather
   and brings it out at once. The last four are QA and QB defaults.
5. **Presentation and the client (D-9, D-10).** The snapshot publishes the unit with `housed: true`. The client's
   single presence gate (`battle_pipeline/fog.rs`) makes it `Hidden` for every viewer: not drawn, not picked, not
   box-selected, no health bar. It leaves the current selection, control groups keep it, and the client sends it
   no order. The render entity stays, so a gas cycle spawns and despawns nothing (charter §3.3 steady state).
6. **After the contact solver (D-11, WP5).** The movement phase's readers under `crates/sim_core/src/movement/`, and
   `steering.rs`, take `is_hidden()`. Collision already ignores the gatherer (it is a ghost while it harvests), so
   nothing waits on WP5 for correctness.
7. **Records (D-12).** B-161 fixed. B-196 (provisional) registered: in 5.0.14 a worker sent to an extractor under
   construction waits beside it and starts when it completes; here the order is refused.
8. **Worlds (§7).** No shipped scenario, matrix battle, golden or bench row harvests (checked on `e313268f`, §3.4),
   so every measured world is byte-identical, and the goldens and the sandbox move by the schema only. Gas income is
   identical to the tick; the only harvesting worlds that change are vision near an extractor and who can be shot.

---

## 2. What StarCraft II does

### 2.1 Sources

All from SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as data.
The override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti`. Every
count below is over all eight layers.

| # | Fact | Data | Source |
|---|---|---|---|
| S1 | **Every extractor resource hides its harvesters.** `HarvestableVespeneGeyserGas`, `…Protoss` and `…Zerg` each carry `Flags HideHarvesters 1`, `HarvestTime 1.981`, `HarvestAmount 4`, `RequiredAlliance Control`. The rich and terrazine variants carry it too. No later layer clears it | `CBehaviorResource` | `liberty` BehaviorData:817–852 (:824, :836, :848; `HarvestTime` :820); rich :90–121 (:97, :109, :121); terrazine `swarm` BehaviorData:850–858 (:857, :868, :879) |
| S2 | **The flag belongs to the structure.** The Refinery, Assimilator and Extractor list the behaviour in their `BehaviorArray`. Each also has `Sight 9` | `CUnit` | Refinery `liberty` UnitData:5758, :5806 (behaviour), :5804 (sight); Assimilator :4103, :4151, :4149; Extractor :8133, :8179, :8177 |
| S3 | **Weapons exclude `Hidden` by name.** The weapon default's `TargetFilters` is `Visible;-`, so a weapon must exclude it itself. 144 of the 146 `TargetFilters` lines exclude `Hidden`. The other two are the default and one weapon that excludes every alliance | `CWeapon` | `core` WeaponData:29; the exception `liberty` WeaponData:619 |
| S4 | **Acquisition excludes `Hidden`.** The attack ability's `AcquireFilters` exclude `Player, Ally, Neutral, Dead, Hidden, Invulnerable, Benign` | `CAbilAttack default` | `core` AbilData:118 |
| S5 | **Splash and effect searches exclude `Hidden`.** 199 of the 247 `SearchFilters` lines exclude it, among them the sieged tank's blast, the Baneling's burst and Psionic Storm, and the `void` searches the handoff plan named | `SearchFilters` | `liberty` EffectData:1675 (`CrucioShockCannonBlast`), :1756 (`VolatileBurstU`), :2079 (`PsiStormSearch`); `void` EffectData:719 (`PurificationNovaNotificationSearch`), :1028 (`CorruptionBombChannelSearch`) |
| S6 | Loading into a transport excludes `Hidden` targets | `CAbilTransport default` `TargetFilters` | `core` AbilData:1202 |
| S7 | **Effects check that their caster is not hidden.** `CasterIsNotHidden` (`Filters -;Hidden`, `WhichUnit Caster`) guards broodling spawns, the Cyclone's Lock On autocast, and the removal of several buffs | `CValidatorUnitFilters` | `core` ValidatorData:269–272; used `liberty` EffectData:1582, :1588; `voidmulti` AbilData:656; `voidmulti` BehaviorData:14, :496 |
| S8 | Missile redirection and the death reveal exclude `Hidden` | `ImpactFilters`, `DeathRevealFilters` | `core` EffectData:140; `core` UnitData:26 |
| S9 | **The harvest stages.** The ability's only uninterruptible stage is `WaitToReturn`; the workers' abilities make `ApproachResource` and `WaitAtResource` cancelable. The gather itself is neither, so an order interrupts it | `CAbilHarvest` | `core` AbilData:353–380 (:379); `voidmulti` AbilData:1890–1901 |
| S10 | **A worker can be sent to an extractor under construction (LotV).** `WorkerPeriodicRefineryCheck` (only while the extractor is under construction) finds workers within 0.3 whose harvest order targets it, holds them with a channelled idle order and suppressed collision, and issues the gather when construction ends | `CBehaviorBuff`, `CEffectEnumArea`, validators | `voidmulti` UnitData:53, :60, :552, :1463; BehaviorData:1121–1146; EffectData:2554–2655; ValidatorData:951–970 |
| S11 | The time inside, as the community measures it: "Workers gathering gas have to spend 1.415 seconds in a Vespene Geyser" (1.981 Normal seconds ÷ 1.4) | Liquipedia, Resources | plan [`2026-11-23-harvest-occupancy`](2026-11-23-harvest-occupancy.md) S7 |
| S12 | This project's own reading of `Hidden`: transport cargo is "hidden in a transport, the editor's default `Dead` and `Hidden` exclusions" | Module doc | `crates/sim_core/src/effects/filter.rs` |

**What the data does not say.** No field states:
- that `HideHarvesters` puts the unit in the filter state `Hidden` (the engine does it; S1 names the flag, S3 to S8
  the filter, S12 is this project's precedent);
- where the worker comes out;
- what happens to it when its extractor dies;
- whether it keeps its sight;
- whether it can be selected, or ordered while inside.

A web search found no StarCraft II source for any of these. A TL.net "SC2 Mythbusters" list appeared in the
results with items on a gas worker under attack and on the extractor's death, but the page refused the fetch (403)
and its verdicts were not read (§16). So each of these is **verify**, and the ones with a visible effect go to the
owner (§15).

### 2.2 The rules that follow

1. **Inside is the gather (S1, S11).** The worker is hidden from the tick its gather starts to the tick it takes.
   A worker waiting in the line stands outside and is seen (the extractor holds one worker, occupancy plan S7).
2. **Hidden means the engine's `Hidden` (S3 to S8, S12; the state itself **verify**).** A hidden unit is not
   acquired, attacked, splashed, found by an effect search or loaded, and it casts nothing. An attack aimed at it
   loses its target; a missile in flight lands on the point, as for cargo (plan `2026-11-23-vision-and-detection`).
3. **It is the structure's flag (S2).** A structure that carries a harvestable resource with `HideHarvesters`
   hides; a mineral field has no structure and hides nothing.
4. **An order interrupts the gather (S9).** If an order reaches a hidden worker, it leaves at once and the gather
   is lost. Whether a player can give it one is QB (D-8).
5. **The rest is unsourced** (§2.1): QA and QB, with defaults in D-5 to D-8.

---

## 3. Diagnosis (on `e313268f`, read from the code)

### 3.1 The gather today

- `economy/harvest.rs::grant_claims` starts a gather by setting `task.gather_ticks` to the kind's
  `harvest_time_ticks` and `current_anim` to `Cast`. The worker stands at its contact point on the extractor's
  outline. `step_gather` counts down and takes on the last tick. Nothing marks the worker as inside.
- While its front order is `Harvest`, `step_harvest` sets `is_mineral_walking`, so collision treats it as a ghost
  (`movement/collision.rs` `collision_body`, `ghost`; `movement/solver.rs` skips every pair with a ghost). Other
  units already pass through it.
- When the extractor dies, the worker's next step finds no `gather_site` and ends the order with
  `TaskEventKind::ExtractorLost`. It stands where it was.

### 3.2 The engine's `Hidden` here is cargo only

`Entity::is_loaded_in_transport()` is the only hidden state. 53 production call sites read it (§3.3). The effect
engine's filter doc (S12) already calls it the editor's `Hidden`.

### 3.3 Every reader, classified

A reader takes `is_hidden()` when it stands for a search or validator that filters `Hidden` (§2.2 rule 2). It keeps
`is_loaded_in_transport()` when it asks about a carrier, about what a player owns, or about order admission (QB).

| Reader (file, symbol) | What it decides | Takes | Why |
|---|---|---|---|
| `target_acquisition.rs` `is_damageable` | Damage at all; also `is_entity_targetable`, `is_target_available_for_combat` (the client's enemy pick), area damage | `is_hidden` | S3, S5 |
| `target_acquisition.rs` `apply_combat_retaliation_prevalidated`, the damage-response victim filter | Retaliation and flee of a victim | `is_hidden` | A hidden unit takes no damage, so it never reaches here; kept consistent |
| `target_acquisition.rs` `is_attacker_visible_to` | Shooting back at an attacker | `is_hidden` | S3 |
| `target_acquisition.rs` `alert_idle_allies` | Call-for-help responders | `is_hidden` | `EA_WEAP` `CallForHelp` searches (S5); a gatherer is not idle anyway |
| `target_acquisition.rs` `acquire_targets` (spatial hash; acquirer skip) | Candidates; who acquires | `is_hidden` (both) | S4; a hidden unit acquires nothing |
| `combat_resolution.rs` `pending_line_weapon`, the weapons loop, pending strikes | Weapon steps and strikes | `is_hidden` | As cargo: no weapon participation |
| `combat.rs` `step_entity_weapon` | Freezes the weapon's phase and cooldown | `is_hidden` | As cargo; whether a hidden unit's cooldown runs is **verify** (§14) |
| `effects/filter.rs` `unit_target`, `passes_filter` | Every effect search and validator | `is_hidden` | S5, S7, S12 |
| `world/vision.rs` `collect_sights`, `update_watchtower_control` | Sight sources; tower control | `is_hidden` | D-6 (QA default) |
| `world/abilities.rs` (Overclock actor; caster), `world/casts.rs` `would_accept_cast_order`, `world/attack_dash.rs` `attack_dash_ready` | Casting, dashing | `is_hidden` | S7 |
| `player_view.rs` `revealed`, `attack_revealed` | Lists of revealed enemies | `is_hidden` | A hidden unit is never listed |
| `production/exits.rs` `rebuild_production_occupancy` | Bodies that block an exit | `is_hidden` | No body inside |
| `production/construction.rs` `stands_on` | Bodies on a site | `is_hidden` | No body inside |
| `entity.rs` `issue_order` | Order admission | stays | D-8 (QB default): an order reaches it and interrupts (S9) |
| `economy/mod.rs` `validate_selection`, `update_unit_tasks` | Command recipients; who steps a task | stays | The gather must keep stepping; QB |
| `player_view.rs` own entities | The owner's units | stays | Still the player's; its bubble says `housed` (D-9) |
| `tech_tree.rs` (two), `production/mod.rs` (two), `production/lifecycle.rs` `validate_job_source` | Prerequisites, producers | stays | Structures and ownership |
| `production/rally.rs` `can_rally`, the rally point of a unit, the rally target | Rallies | stays | A rally on a hidden unit follows its position (§14) |
| `transport/*`, `transport/orders.rs` carrier checks | Carrier topology | stays | Carriers only |
| `client` `battle_presentation.rs` roster | Counts of a player's units | stays | Still the player's |
| `sim_fixtures` `trace/blocking.rs`, `trace/route_overlap.rs` | Diagnostic traces | stays | Nothing traced harvests (§3.4) |
| `entity.rs` `takes_part_in_movement`; `movement/mod.rs` displacement admission bodies; `steering.rs` `resolve_push_interaction` | Movement and push | `is_hidden` in **WP5, after the contact solver merges** | Already a ghost (§3.1) |
| `movement/mod.rs` movement admission, `movement/groups.rs` route groups, `movement/flee.rs` | Orders, route groups, flee | stays | Orders (QB); a gatherer is in no route group and does not flee |

The client's readers (D-10):

| Reader | Today | After |
|---|---|---|
| `battle_pipeline/fog.rs` `presence_of`, `BattleFog::presence` | Fog and detection | `Hidden` first for a housed bubble, for every viewer and with no fog |
| `battle_hud/fog.rs` `apply_battle_fog` | Hides bubbles; stops inspecting | Also drops a housed own unit from the selection (`BattleSelection::leave`) |
| `battle_input/mod.rs` `pick_unit` (presence), `units_in_box` (`Visibility`) | — | Unchanged code; covered by the presence gate |
| `effects/areas.rs`, `effects/stance.rs` | Presence | Unchanged code |
| `battle_pipeline/commands.rs` `can_receive_order` | Alive, not cargo | Also not housed (QB) |
| `battle_selection.rs` | Forgets a unit that leaves the snapshot | `leave(id)`: out of the selection, kept in groups |

### 3.4 Worlds that harvest

On `e313268f`:
- No scenario in `assets/scenarios/` gives a `Harvest` order (`InitialOrder` has `AttackMoveToStart`,
  `AttackMoveToCell` and `EnterModes` only), and none trains a worker.
- `golden_replay.ron` and `golden_replay_orders.ron` harvest nothing; their own re-record headers say so ("Nothing in
  this replay harvests").
- `matchup_matrix.ron`, the bench and `sim_fixtures`' sandbox issue no `Harvest`.
- The client's skirmish pilot (`crates/client/tests/support/skirmish_pilot.rs`) mines minerals only
  (`node.kind == ResourceKind::Mineral`).

So gas mining is exercised only by the harvest tests (`harvest_*_tests`, `full_match_tests`, `player_view_tests`),
the client's `battle_card_path_tests::gas_through_the_card_extractor_snap_harvest_and_income`, and the occupancy
plans' scratch income tool. WP0 re-checks this on its tree (P-0) before predicting §7.

---

## 4. Design decisions

### D-1. `hides_harvesters` is the structure's capability

- `sim_types::ArchetypeCapabilities` gains:

  ```rust
  /// Whether a harvester gathering from this structure is inside it, hidden, for its gather
  /// (5.0.14 `Flags HideHarvesters` of the structure's resource behaviour; plan
  /// `2026-11-24-hidden-harvesters` D-1). Only a gas extractor may carry it.
  #[serde(default)]
  pub hides_harvesters: bool,
  ```

- `StructureArchetypeDef` gains `hides_harvesters: bool`, **required in RON** as `is_gas_extractor` is. All 49
  structure rows author it. The three extractor analogues author `true` with a comment citing S1 and S2:
  `plasma_siphon_dome` (Refinery), `rift_siphon_dome` (Assimilator), `spore_vent_dome` (Extractor). Units always set
  `false`.
- **Validation.** `CatalogError` (`catalog/spells.rs`) gains
  `HidesHarvestersWithoutExtractor { definition }`: a structure with `hides_harvesters: true` and
  `is_gas_extractor: false` fails to load. In 5.0.14 the flag is a field of a resource behaviour, so a structure
  that holds no resource cannot carry it.
- **Not hashed.** The catalog is not part of the checksum.
- **Rejected:** a field on the resource kind's row in `placement_rules.ron`. The flag is the structure's in the
  data (S2), and a kind row would hide minerals' gatherers if a map ever put a mineral-bearing structure in play.

### D-2. When a worker is inside

- **Enters** in `grant_claims`, on the tick the line grants it the node, when the node's gather site is a
  structure whose archetype has `hides_harvesters`. That structure's id is recorded (D-3).
- **Leaves** on the first of:
  1. the take (`step_gather`, the tick `gather_ticks` reaches 0);
  2. a new leg: every new order, `end_task`, `replace_task_order` and mode switch go through `fresh_leg`;
  3. the structure's death (D-5).
- So with `H = harvest_time_ticks` and a grant on tick `g`, the worker is inside at the end of ticks `g` to
  `g + H − 1` (35 checksums for gas), and seen again at the end of `g + H`, the take. A worker waiting in the line
  is never inside. The hand-off (Q0100) is the node's, so it is unchanged.
- **Mineral fields** have no structure, so their gatherers are never inside.

### D-3. Hashed state: `UnitTask::housed_in`; schema 44

- `UnitTask` gains `pub housed_in: Option<EntityId>`: "The structure this gatherer is inside, hidden, while it
  gathers (plan `2026-11-24-hidden-harvesters` D-2); `None` otherwise." `fresh_leg` sets it to `None`.
- `Entity` gains `pub const fn housing(&self) -> Option<EntityId>`, `pub const fn is_housed(&self) -> bool`, and
  `pub const fn is_hidden(&self) -> bool { self.is_loaded_in_transport() || self.is_housed() }`, whose doc names
  the editor's `Hidden` filter (S3 to S8) and lists both kinds.
- **Invariant**, documented on the field and pinned by P-9: `housed_in == Some(s)` implies that the front order is
  `Harvest`, `task.gather_ticks > 0`, and `s` is alive with `hides_harvesters`.
- **Encoded** after `last_node` in `checksum/encoding.rs`'s `UnitTask` destructuring. `CHECKSUM_SCHEMA_VERSION`
  takes the next free number at merge (**44** proposed: 42 and 43 are reserved), with the doc line "Schema 44 adds
  each unit task's housing structure, `UnitTask::housed_in`, after `last_node` (plan
  `2026-11-24-hidden-harvesters` D-3)." It decides who can be shot, so it is hashed.
- **Why stored and not derived.** Derived, the state is "front order `Harvest` on a node whose site's structure
  hides harvesters, with `gather_ticks > 0`". That is three lookups (map cell, structure entity, catalog) for each of
  the ~30 readers of §3.3, most of which hold only an `&Entity`, and it would stay true between the extractor's
  death and the worker's next step. Stored, it is one `Option` like `transport_carrier`, and D-5 can release it in
  the phase that removes the structure.
- **Why on `UnitTask`.** `fresh_leg` already clears the leg on every way an order ends, so no exit path can leave a
  stale value. On `Entity` each path would need its own clear.
- **Rejected:** reusing `transport_carrier` with the extractor as a carrier. A carrier's cargo is validated as
  topology (`validate_transport_links`), dies with it in a death group, counts slots and unloads by order. None of
  that is the extractor's.

### D-4. One predicate for `Hidden`

Every reader in §3.3 marked `is_hidden` changes from `is_loaded_in_transport()` to `is_hidden()`, and nothing else
in it changes. Readers marked "stays" are left alone, and their docs that say "loaded" stay true. The effect filter's
module doc is widened to name both kinds.

### D-5. The extractor dies with a worker inside (QA, default: it survives)

- In `reap_dead_and_emit_shatter`, before `reap_transport_groups` removes the dead, a new
  `SimWorld::release_housed_harvesters` (in `economy/harvest.rs`) clears `housed_in` of every unit whose housing
  structure is dead or gone, in entity-ID order. The worker is seen and damageable at the end of that tick.
- Its next task step finds no gather site and ends the order with `TaskEventKind::ExtractorLost`, as today. Its
  gather is lost, and it carries nothing.
- **QA (b), if answered so:** the worker dies with the extractor. `release_housed_harvesters` would then mark it
  dead, and the transport death groups would reap it in the same pass (its supply and cargo as for any death). That
  is one branch of one function.
- In 5.0.14 the data has no field for this (§2.1). **verify** in the editor.

### D-6. Sight while inside (QA, default: none)

- A housed unit is no sight source (`collect_sights` reads `is_hidden`). Cargo already gives none.
- The extractor sees 9 from its centre (S2). A worker at the edge, about 2 world units from the centre with sight
  8, reaches at most about 1 unit further on its own side, which is the side of its depot. So the loss is a sliver
  that the player's own base usually covers.
- **verify**: whether the engine keeps a hidden unit's sight.

### D-7. Where it comes out (QA, default: where it went in)

- The worker's position does not change while it is inside. It comes out at its contact point, on the outline side
  it reached from its depot, and walks its return leg from there as today.
- **verify**: whether the engine places it on the side toward its return target. If so, the difference is at most
  the extractor's diameter (3 cells) on a trip that heads that way anyway.
- No position is written, so nothing under `movement/` changes.

### D-8. Selection and orders (QB, default: not selectable; an order that reaches it interrupts)

- **The simulation admits orders to a housed unit as today** (`issue_order` and `validate_selection` stay on
  `is_loaded_in_transport`). An immediate order ends the leg, `fresh_leg` clears `housed_in`, and the worker is out
  on that tick at its contact point with the gather lost (S9: the gather stage is interruptible). A queued order
  waits behind the `Harvest` order.
- **The client cannot select it** (D-10): it is not drawn or picked, it leaves the current selection, and the
  client sends it no order. Control groups keep it, so it is back in the group when it comes out.
- So in play only a script or a remote command reaches it. **verify** whether the engine lets a control group
  recall select a hidden unit, and whether it keeps hidden units in control groups at all (QB).

### D-9. The snapshot publishes the housed unit

- `sim_types::BubbleRenderInstance` gains `#[serde(default)] pub housed: bool`, with the doc "Inside a structure,
  hidden from every player (plan `2026-11-24-hidden-harvesters` D-9): a client draws and picks nothing of it". Old
  records read it as `false`.
- `world/snapshot.rs` publishes a housed unit with `housed: true` rather than skipping it as it skips cargo. A unit
  that left the snapshot every gas trip would despawn and respawn its render entity every 1.4 s, which charter §3.3's
  steady-state rule forbids. Cargo stays skipped.
- `PlayerView` keeps the owner's housed unit in its own-entity records (it is still the player's), and never lists
  it in `revealed` or `attack_revealed` (D-4).

### D-10. The client

- **One gate.** `presence_of` and `BattleFog::presence` return `Presence::Hidden` for a bubble with `housed`,
  before any fog test, for every viewer, and `presence_of` does so also with no fog. Every drawer and picker that
  reads presence (§3.3's client table) follows: no bubble, no shimmer, no pick, no inspection, no health bar.
- **Selection.** `apply_battle_fog` calls a new `BattleSelection::leave(id)` for a housed own unit: out of
  `selected`, kept in `groups`. The module's invariants gain: "A housed unit is in no selection; control groups keep
  it." A recall copies the group, and the next fog pass drops its housed members.
- **Orders.** `can_receive_order` (`battle_pipeline/commands.rs`) also requires `!entity.is_housed()`, so an order
  in the same frame as a recall skips a housed member.
- **No allocation.** The render entity stays; only its `Visibility` toggles. P-10 extends the steady-state tests
  with a gas cycle.

### D-11. After the contact solver merges: the movement phase reads `is_hidden` (WP5)

- `Entity::takes_part_in_movement`, the displacement admission's body list in `movement/mod.rs`, and
  `steering.rs::resolve_push_interaction` take `is_hidden()`.
- Today the gatherer is a ghost through `is_mineral_walking` (§3.1), so no pair and no push involves it. WP5 makes
  the rule explicit, so it no longer rests on the harvest's walking flag.
- Expected byte-identical everywhere (§7). If a world moves, that is a finding traced before anything changes
  (§8 rule 5).

### D-12. Records

- **B-161** fixed by WP2 and WP3 (WP5 completes the movement half).
- **B-196 (new, provisional number):** "A worker can be sent to an extractor under construction in 5.0.14 (LotV): it
  waits beside it, collision suppressed, and gathers when construction ends (`WorkerPeriodicRefineryCheck`,
  `voidmulti` BehaviorData:1121–1146, EffectData:2554–2655, ValidatorData:951–970). Here `Harvest` on an unfinished
  extractor is refused (`ExtractorRequired`)." Open, deferred: an admission and economy change, not part of hiding.
  The reading of S10's effect chain is **verify**.

---

## 5. Determinism and checksum

- **Integer state only.** `housed_in` is an `Option<EntityId>`; the predicates are boolean. No float enters the
  simulation crates (C7 unchanged).
- **Order.** `housed_in` is written in three places: the line's resolution (after every task step, in the line's
  key), the gatherer's own step (the take), and the reap phase (entity-ID order). A reader in a later phase reads a
  value no other step of the same phase writes, so iteration order decides nothing.
- **Hashed state:** D-3, schema next free at merge (44 proposed).
- **Goldens and sandbox.** Nothing in them harvests (§3.4), so every `housed_in` is `None` at every checkpoint and
  only the encoding changes. WP2's proof, the procedure of plan `2026-11-23-harvest-handoff` §5:
  1. With `housed_in` left out of the encoding and the schema at its current number, WP2's tree reproduces every
     recorded checksum of `golden_replay.ron`, `golden_replay_orders.ron` and `sim_fixtures`'
     `DEFAULT_SANDBOX_CHECKSUM`, on native debug and release and on `wasm32-wasip1` release and debug
     (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`).
  2. Then the field goes in, the schema becomes the next free number, and both goldens are re-recorded under the
     header "Re-recorded for plan 2026-11-24-hidden-harvesters WP2 (checksum schema M -> N): each unit task also
     encodes its housing structure, so every checkpoint changed. No behaviour did: with the field left out of the
     encoding at schema M, both fixtures reproduce as recorded for <the previous header>." The four builds reproduce
     the new values. The schema-byte tests of `checksum::encoding` and the sandbox constant move with it.
  3. If `main` takes the number first, the merge repeats step 2 with the next one.
- **WP1, WP3 and WP5 leave the goldens and the sandbox unchanged.**
- **`checksum_state_tests`** gains a mutation of `housed_in`.
- **wasm.** No new dependency.
- **Cost.** One `Option` per entity in the encoding; `release_housed_harvesters` is one pass over the entities in the
  reap phase that returns at once when no structure died this tick (a flag the reap already knows, **verify** the
  cheapest hook while implementing). The throughput bench does not harvest.

---

## 6. Acceptance (principled properties, fixed before implementation)

Tests find units by property (a harvester; a structure with `hides_harvesters`; an armed ground unit with splash),
never by slug (C5).

- **P-0 (no world harvests, the premise of §7).** WP0 records in the evidence: a grep of `Harvest` and
  `InitialOrder` variants in `assets/scenarios/`, both goldens, `matchup_matrix.ron`, `sim_fixtures` and the bench;
  and a scratch probe that counts, every tick, the units whose front order is `Harvest` in each world of `measure.sh`
  (expected 0 throughout). If either finds harvesting, §7's predictions for that world are redone before WP2.
- **P-1 (data).** `catalog_reference_values_tests` pins `hides_harvesters` for every structure, keyed by
  `sc2_inspiration`: true for the Refinery, Assimilator and Extractor analogues (S1, S2, each row cited), false for
  the other 46; every unit false. The validation refuses `hides_harvesters` without `is_gas_extractor` with the typed
  error. A run-time catalog edit (C6) that sets the Refinery analogue's flag to false leaves its gatherer seen and
  targetable for its whole gather, with no rebuild.
- **P-2 (the time inside).** For a gas gatherer granted on tick `g`: `housed_in` names its extractor at the end of
  ticks `g` to `g + H − 1` and is `None` at `g + H`, the take; a second worker waiting in the line is never housed;
  a mineral gatherer is never housed. `H` is read from the kind's row.
- **P-3 (hidden from searches).** While a gatherer is housed, with an armed enemy ground unit in range, an enemy
  splash unit and an enemy caster with an area spell placed around it, and an identical visible worker of the same
  owner beside it as the control:
  - the enemy acquires the control, never the housed worker;
  - an `AttackTarget` on the housed worker given before it entered is dropped on the grant tick (as for cargo);
  - splash and the area spell centred between them damage the control and not the housed worker;
  - a missile launched at the worker before it entered lands on the point (plan `2026-11-23-vision-and-detection`);
  - the housed worker acquires nothing and calls no help.
  When it comes out it is acquirable again on the take's tick.
- **P-4 (sight).** With QA's default, a housed worker contributes no sight source: the owner's vision equals that
  of the same world with the worker removed, on the tick after the grant.
- **P-5 (orders, simulation).** An immediate `Move` given to a housed worker: on that tick `housed_in` is `None`, the
  gather is lost (no cargo), and it walks from its contact point. A queued `Move` waits for the delivery.
- **P-6 (the extractor dies).** Killing the extractor of a housed worker: at the end of that tick the worker is
  alive, not housed, damageable, at its contact point and empty; on the next tick its order ends with
  `ExtractorLost`, the event and tick of today's path.
- **P-7 (where it comes out).** The worker's position at the take equals its position at the grant.
- **P-8 (checksum).** `housed_in` changes the checksum (`checksum_state_tests`); the schema byte is the new number;
  the goldens and the sandbox reproduce per §5.
- **P-9 (invariant).** In a 6,000-tick run with three workers on each of two geysers and an enemy that kills one
  extractor half-way, every tick satisfies D-3's invariant for every unit.
- **P-10 (client).** On a live battle with a gas worker:
  - while housed its bubble is `Visibility::Hidden`, `pick_unit` through it finds the extractor or nothing,
    `units_in_box` over it does not list it, and it has no health bar;
  - a selected worker that enters leaves the selection and stays in its control group; recalling the group while
    it is inside selects the others, and an order in that frame does not name it;
  - the render entity count is constant over a full gas cycle, and `sync_steady_state_tests` and
    `sync_allocation_count_tests` pass with a gas worker added to their warm battle.
- **P-11 (income unchanged).** With the scratch income tool (§7), every take tick of every gas and mineral line of
  the handoff plan's §6.1 is identical on WP1's and WP2's trees.
- **P-12 (no identity).** No slug or archetype name in the production changes (C5).
- **P-13 (worlds).** §7's table holds byte for byte.

---

## 7. Worlds that move

**Commands.** The attack-orders plan's scripts, with outputs under `~/scratch/hidden-harvesters/`. Copy `base.sh`,
`measure.sh`, `compare.sh` and `client.sh` from `~/scratch/attack-orders/scripts/` and change the worktree and target
paths in each. `measure.sh` passes `--assets "$R/assets"` to `route_trace`; keep that, because `route_trace`'s default
reads the assets of the tree it was compiled in. `matchup_trace` embeds the matrix fixture and the catalog, so it is
rebuilt for each tree.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
R=~/scratch/hidden-harvesters
$R/scripts/base.sh <wp>        # release matchup_trace and route_trace, then measure.sh into world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
$R/scripts/client.sh <wp>      # client targets in release, one at a time under ~/scratch/gate.lock
```

**Gas mining.** It is in no measured world (§3.4, P-0), so it is measured with the handoff plan's scratch income
tool, `~/scratch/harvest-handoff/tool/harvest_occ.rs`, copied into `crates/sim_core/examples/` of a scratch tree and
never committed. It runs in `line` and `node` modes on the cases of that plan's §6.1 (showcase slot 0 and
`default_arena` slot 0 and 1; `k` = 1 to 4 on each geyser and field). Two probes are added to the copy:
- the take ticks of every node, written one per line, compared between trees with `cmp`;
- per gas trip, the number of tick ends with `housed_in` set, and per mineral trip the same (expected `H` = 35 and
  0).

Run everything in the background, one at a time across worktrees (charter §7). Check `quota -s` and `df -h ~/scratch`
first.

| World | WP1 (data) | WP2 (state, readers, schema) | WP3 (client) | WP5 (movement readers) |
|---|---|---|---|---|
| Matrix, seeds of record and 16 (`matrix.csv`, `matrix.md`) | byte-identical | byte-identical (nothing harvests) | not run | byte-identical |
| Battles 31 (100 seeds), 33 (64 seeds) | byte-identical | byte-identical | not run | byte-identical |
| Showcase and showcase effects, default seed and 1–8 | byte-identical | byte-identical (nothing harvests) | not run | byte-identical |
| `golden_replay`, `golden_replay_orders`, `DEFAULT_SANDBOX_CHECKSUM` | unchanged | **schema re-record only**, proven by §5 | unchanged | unchanged |
| Gas and mineral income, take ticks (scratch tool) | identical | **identical** (P-11): nothing in the line, the gather or the walk reads the new state | identical | identical (the gatherer was a ghost already) |
| Ticks inside per trip (probe) | 0 everywhere | **35 per gas trip, 0 per mineral trip** | 35 / 0 | 35 / 0 |
| Harvest tests (`harvest_*_tests`, `full_match_tests`, `player_view_tests`) | pass unchanged | pass unchanged, unless a test shoots or sights a gas gatherer (§8 rule 4) | pass | pass |
| Client skirmish stage log (`skirmish_match_tests`) | identical | identical (the pilot mines minerals only) | identical | identical |
| Client `battle_card_path_tests` gas test | pass | pass | pass (its worker leaves the selection when it enters; the test reads income, **verify** it does not reselect the worker by click) | pass |

**Direction, recorded before measuring.** The change only removes a gatherer from searches, sight and the client's
reach for `H` ticks a trip. So no income moves, no take tick moves, and no world without a gas gatherer changes. A
harvesting world with enemies near an extractor changes only in who is shot, and with QA's default, in a sliver of
vision.

---

## 8. Stop rules

1. **WP1 is latent.** Nothing reads the flag; every world of §7 is byte-identical and the goldens unchanged. Any
   difference is a defect and stops the package.
2. **WP2 moves the goldens and the sandbox by the schema only,** proven by §5 step 1 on all four builds. A checkpoint
   that the old encoding does not reproduce stops the package and is traced to its first tick and entity.
3. **No measured world moves in any package,** and no take tick moves in the income tool (P-11). A difference is
   traced to the first differing tick and unit. If P-0 found a harvesting world, its difference is checked against
   D-2 to D-7 and reported; otherwise it stops the package.
4. **No existing test is loosened.** A test that shoots, splashes or sights a gas gatherer during its gather, or
   clicks one, fails by design; it is rewritten from P-2 to P-10 and listed in the As built. Any other failure is a
   finding and stops the package.
5. **WP5 is byte-identical** (D-11). A move is a finding about the ghost rule, traced and taken to the orchestrator.
   It is not fitted.
6. **No unit or structure identity in code** and no tuning constant. Every rule is the data's (§2.1), an existing
   engine rule, or a recorded default of QA or QB.
7. **QA or QB answered otherwise** before a package merges: the affected D is amended (§13 R-3, R-4) and its test
   rewritten first. No package guesses an answer.

---

## 9. Client consequences

- `crates/client/src/battle_pipeline/fog.rs`: `presence_of` and `BattleFog::presence` (D-10).
- `crates/client/src/battle_hud/fog.rs`: `apply_battle_fog` drops a housed own unit from the selection (D-10).
- `crates/client/src/battle_selection.rs`: `leave(id)` and the invariant line (D-10).
- `crates/client/src/battle_pipeline/commands.rs`: `can_receive_order` (D-10).
- No code change in `battle_input/mod.rs`, `effects/areas.rs` or `effects/stance.rs`: they read presence or
  `Visibility`. P-10 covers them.
- No GPU work: hiding is `Visibility`. The client tests of P-10 run through the GPU harness where they already do.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP4, WP5 | §4: B-161 "open; plan 2026-11-24-hidden-harvesters" (WP0), fixed (WP4), movement half (WP5); B-196 registered (WP0); the Economy row (WP4) |
| `docs/plans/README.md` | plan, WP4 | Row |
| `docs/evidence/2026-11-24-hidden-harvesters.md` (new) | WP0 to WP5 | §2.1's sources and counts, P-0, §7, red runs, the golden proof |
| `crates/sim_types/src/commands.rs` | WP1 | `ArchetypeCapabilities::hides_harvesters` |
| `crates/sim_core/src/catalog.rs` | WP1 | `StructureArchetypeDef::hides_harvesters`, `capabilities()`, unit `false`, validation call; unit tests |
| `crates/sim_core/src/catalog/spells.rs` | WP1 | `CatalogError::HidesHarvestersWithoutExtractor` |
| `assets/catalog/structures.ron` | WP1 | `hides_harvesters` on all 49 rows; `true` with citations on three |
| `crates/sim_core/tests/catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs` | WP1 | P-1 |
| `crates/sim_core/src/entity.rs` | WP2 | `UnitTask::housed_in`, `housing`, `is_housed`, `is_hidden`, docs |
| `crates/sim_core/src/economy/mod.rs` | WP2 | `fresh_leg` |
| `crates/sim_core/src/economy/harvest.rs` | WP2 | Grant, take, `release_housed_harvesters`, module docs |
| `crates/sim_core/src/world.rs` | WP2 | `reap_dead_and_emit_shatter` calls the release |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | WP2 | Schema, encoding |
| `crates/sim_core/src/target_acquisition.rs`, `combat_resolution.rs`, `combat.rs`, `effects/filter.rs`, `world/vision.rs`, `world/abilities.rs`, `world/casts.rs`, `world/attack_dash.rs`, `player_view.rs`, `production/exits.rs`, `production/construction.rs` | WP2 | §3.3's `is_hidden` readers |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`, `crates/sim_fixtures/src/` (sandbox constant) | WP2 | Schema re-record (§5) |
| `crates/sim_core/tests/hidden_harvester_tests.rs` (new), `checksum_state_tests.rs` | WP2 | P-2 to P-9 |
| `crates/sim_types/src/snapshot.rs`, `crates/sim_core/src/world/snapshot.rs` | WP3 | `BubbleRenderInstance::housed` (D-9) |
| `crates/client/src/battle_pipeline/fog.rs`, `battle_hud/fog.rs`, `battle_selection.rs`, `battle_pipeline/commands.rs` | WP3 | D-10 |
| `crates/client/tests/battle_fog_tests.rs`, `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` | WP3 | P-10 |
| `crates/sim_core/src/movement/mod.rs`, `crates/sim_core/src/steering.rs`, `entity.rs` `takes_part_in_movement` | **WP5 only, after the contact solver merges** | D-11 |

Nothing under `crates/sim_core/src/movement/` changes before WP5.

---

## 11. Work packages (commit order)

Each package is one branch from `main`, in its own worktree, with
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`.
- Write the failing tests first and paste their red run into the hand-back.
- Run `cargo gate quick` and the tests the change needs, including the `Slow` targets named in the package. A
  verifier then runs `cargo gate package` (Q0061).
- No `unwrap`. `expect` only with `invariant:`. No catalog identity in production Rust (C5). Tests find units by
  property.
- Check `docs/questions/open/` before the last commit, and whether QA and QB were filed and answered.

WP0 to WP4 may run as one branch and one session, one commit each, with one `package` gate on the final tree
(charter §4.3). Each package's §7 measurement is still taken against the tree before it.

### WP0: records and baseline (XS, docs only)

- `DESIGN.md` §4: B-161 reads "open; plan 2026-11-24-hidden-harvesters". B-196 is registered under the next free
  number (D-12), and this plan is renumbered in the same commit if it differs.
- Evidence file: §2.1's sources with the counts (the `grep` commands that give 144/146 and 199/247 included), P-0's
  grep and probe.
- Measure WP0's tree into `world-wp0`, and the income tool's take ticks into `income-wp0` (§7).
- **Gate:** docs-only `package`.

### WP1: the capability as data (S, `sim_types`, `sim_core`, catalog)

Failing tests first: P-1's table and validation (red until the field exists; the table compiles against a field
defaulting to `false`, so add the field to `ArchetypeCapabilities` first with every row `false`, then the table is
red). Then D-1: the RON field on 49 rows, the three `true` rows with citations, the validation variant.

**Acceptance:** P-1; §7's WP1 column against `world-wp0`; the client builds (struct literals of
`ArchetypeCapabilities` in its tests gain the field).

### WP2: the hidden state in the simulation (M, `sim_core`, goldens; B-161 simulation half)

Failing tests first (red on WP1's tree), in `hidden_harvester_tests.rs`: P-2, P-3, P-4, P-5, P-6, P-7, P-9; and
P-8's mutation in `checksum_state_tests`. P-5's queued half, P-6's `ExtractorLost` half and P-7 are expected green on
WP1's tree (they pin today's behaviour that D-5, D-7 and D-8 keep); they stay as guards.

Then the change: D-2 to D-8, and §3.3's readers in one commit. Then §5's proof and re-record.

**Acceptance:** P-2 to P-9, P-11 (income tool against `income-wp0`); §7's WP2 column; the goldens on the four builds.
Before hand-back, run the `Slow` targets this package reaches: `sim_core/sc2_who_wins_matchup_matrix_tests`,
`sim_core/full_match_tests`, `client/battle_card_path_tests` (selected by the `sim_core` rule).

### WP3: the snapshot and the client (M, `sim_types`, `sim_core`, `client`; B-161 client half)

Failing tests first (red on WP2's tree): P-10, and `presence_of` units (a housed bubble is `Hidden` with fog, without
fog, and for its owner).

Then D-9, D-10.

**Acceptance:** P-10; the skirmish stage log (§7). Before hand-back, run the `Slow` client targets this package's
code reaches, which are outside `quick`:
- `client/battle_card_path_tests`
- `client/battle_input_tests`
- `client/battle_hud_tests`
- `client/battle_fog_tests`
- `client/sync_steady_state_tests`
- `client/sync_allocation_count_tests` (valgrind)

### WP4: bookkeeping (XS)

- `DESIGN.md`: B-161 fixed with its commits and the evidence, noting WP5 open; the Economy row says a gas gatherer is
  hidden inside its extractor for its gather.
- Evidence: §7's comparisons, the golden proof, the red runs, the rewritten tests.
- This plan's `## As built`; README row `Partly done` (WP5 waits for the contact solver).
- **Gate:** docs-only `package`.

### WP5: the movement phase reads `is_hidden` (S, `sim_core/src/movement/`, `steering.rs`; **after the contact solver merges**)

Not started until the contact-solver branch is on `main`. Then a new branch is made from that `main`, and D-11 is
re-read against the merged `movement/`.
- Failing tests first: a housed worker whose `is_mineral_walking` a test clears by hand (the only way to separate the
  two rules) is still no collision pair, no push partner and no displacement obstacle.
- Then D-11.
- **Acceptance:** that test; §7's WP5 column byte-identical; the goldens unchanged; P-11.
- Then `DESIGN.md` B-161's movement half fixed, the As built and README row `Done`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where | Gate selection |
|---|---|---|---|
| Data | The flag per structure against S1 and S2; validation; run-time edit | WP1: `catalog.rs` units, `catalog_reference_values_tests`, runtime-catalog test | `catalog` rule; `sim_core` crate |
| State | When a worker is inside; the invariant; release on death | WP2: `hidden_harvester_tests` (new) | `sim_core` crate |
| Readers | Not acquired, attacked, splashed, searched; no sight; no casts | WP2: `hidden_harvester_tests` | `sim_core` crate |
| Checksum | The field is hashed; schema; goldens | WP2: `checksum_state_tests`, golden tests on four builds | `sim_core` crate; determinism jobs |
| Economy | Income and take ticks unchanged | WP2: scratch income tool (§7); the existing harvest tests | `sim_core` crate |
| Client | Not drawn, picked, box-selected; leaves the selection; no order; no allocation | WP3: presence units, P-10 targets | `battle_pipeline/**`, `battle_hud/**`, `client` crate; `Slow` run explicitly |
| Movement | Hidden is no body | WP5 | `movement/**` |
| Worlds | §7 | WP1, WP2, WP5 (measure), WP3 (skirmish log) | — |

The gate catalog (`crates/gate/suites.ron`) needs no new rule if it lists integration targets by crate rule.
**Verify** for `hidden_harvester_tests.rs`; if targets are listed by name, WP2 adds its row (C22).

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | A reader of §3.3 is misclassified (for example a "stays" reader that is really a search) | A hidden worker is found by one search | P-3 covers acquisition, attack, splash, effect search and missiles; a grep in WP2 lists every remaining `is_loaded_in_transport` call with its reason in the evidence |
| R-2 | A client test clicks or reselects a gas worker during its gather | Red after WP3 | §8 rule 4: rewritten from P-10, listed |
| R-3 | QA answered (b): the worker dies with the extractor | D-5's release becomes a death | One branch in `release_housed_harvesters`; P-6 rewritten; the transport reap handles supply and removal |
| R-4 | QB answered: hidden units can be selected, or control groups drop them | D-10 changes | (selectable) the client keeps them in `selected` and only hides the bubble; (dropped) `leave` becomes `forget`; the simulation does not change |
| R-5 | QA answered: the worker keeps its sight | D-6 changes | `collect_sights` keeps `is_loaded_in_transport`; P-4 rewritten |
| R-6 | The reap-phase pass costs a scan every tick | Throughput | It returns at once on a tick with no death of a structure (§5); the bench does not harvest |
| R-7 | The contact-solver branch touches `entity.rs` or `steering.rs` | Conflict | WP2 touches `entity.rs` only to add fields and predicates; `steering.rs` waits for WP5 |
| R-8 | 42 and 43 are not taken when WP2 merges, or more are | Schema number | "Next free at merge" (§5 step 3) |
| R-9 | The client's `BubbleRenderInstance` is compared field by field in a capture verifier (`client_diagnostics`) | A capture test fails on the new field | `#[serde(default)]`; a capture with no gas gatherer writes `false` everywhere; check `runtime_capture/driver/verify.rs` in WP3 |

---

## 14. Out of scope and decisions recorded (Q0002)

- **An extractor under construction** (S10): B-196, deferred. It is an admission change (`ExtractorRequired`), not
  hiding.
- **A hidden unit's weapon cooldown** freezes as cargo's does (§3.3). Whether 5.0.14 runs it is **verify**; a worker's
  weapon rarely matters.
- **Statuses on a hidden unit** keep counting down where they live on the unit; a periodic effect that targets it
  finds it hidden (S7) and does nothing. Not checked against the engine (**verify**).
- **A rally on a unit that goes inside** keeps following its position (the rally readers stay). Whether 5.0.14 drops
  such a rally is **verify**; a rally on one's own gas worker is rare.
- **The extractor's own selection card** showing its worker count: presentation of `IdealHarvesterCount`, not
  modelled anywhere yet.
- **Detection** of a hidden unit: it is not concealed, it is absent; detection does not apply.
- **Allied players**: no shipped mode has allies; `RequiredAlliance Control` (S1) is the owner rule `gather_site`
  already applies.
- **Cargo in control groups**: cargo still leaves the snapshot and is forgotten from groups. Whether 5.0.14 keeps
  passengers in control groups is not this plan's question; it is noted in QB's text for the owner's reference only.

---

## 15. Draft owner questions (filed as Q0107 and Q0108)

Numbers Q0107 and Q0108 are as assigned; the orchestrator renumbers if they are taken when filing.

```markdown
# Q0107: What happens to a worker while it is inside a gas extractor?

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** (1a), (2a), (3a): it survives its extractor's destruction and comes out;
  it sees nothing while inside; it comes out where it went in.

## Question

In StarCraft II a worker collecting gas disappears into the Refinery, Assimilator or Extractor for about
1.4 seconds, and while it is inside it cannot be attacked or hit by splash. Plan
`2026-11-24-hidden-harvesters` builds that. The game data say the worker is hidden but not the details
below, and we found no page that does. Each choice is small and easy to change.

1. **The extractor is destroyed while a worker is inside.**
   - (a) The worker survives and appears where it went in (default).
   - (b) The worker dies with the extractor.
2. **Sight while inside.**
   - (a) The worker sees nothing; the extractor's own sight (9) covers almost the same area (default).
   - (b) The worker keeps its own sight (8) from where it stands.
3. **Where it comes out.**
   - (a) Where it went in, which is normally the side facing its base (default).
   - (b) On the side facing the base it returns to, wherever it went in.

If you can check in the game or editor: with an SCV inside a Refinery, kill the Refinery, and watch
whether the SCV survives and where it appears. Also: send a single SCV to a Refinery in fog, and see
whether the fog stays lifted beyond the Refinery's own sight while the SCV is inside.

## Answer

```

```markdown
# Q0108: Can you select or order a worker while it is inside a gas extractor?

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** (a): you cannot select it; it leaves your current selection; control
  groups keep it, so it is back in the group when it comes out; you cannot order it while it is inside.

## Question

While a worker is inside a Refinery, Assimilator or Extractor (about 1.4 seconds a trip), it is not
drawn. Plan `2026-11-24-hidden-harvesters` has to decide what the player can do with it meanwhile.

- **(a) Not selectable, kept in control groups (default).** It cannot be clicked or box-selected. If it
  was selected, it drops out of the selection. A control group keeps it: pressing the group's key while
  it is inside selects the others, and once it is out the key selects it again. You cannot order it
  while it is inside.
- **(b) It stays selected and takes orders.** It stays in the selection while inside; an order makes it
  leave the extractor at once, losing that load (the game data allow this: collecting gas is an
  interruptible step).
- **(c) As (a), but control groups also drop it,** as they drop a unit loaded into a transport here
  today.

Recommendation: (a), unless you know the game does otherwise. If you can check: put a gas SCV in a
control group and press the key while it is inside.

## Answer

```

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/
  {AbilData,BehaviorData,EffectData,UnitData,ValidatorData,WeaponData}.xml` (`5444e12f13`), read with `grep`, `sed`
  and `awk`. The counts of S3 and S5:
  `cat */WeaponData.xml | grep '<TargetFilters' | grep -c Hidden` (144 of 146) and
  `cat */EffectData.xml | grep 'SearchFilters' | grep -c Hidden` (199 of 247, three of the 247 lines are inside
  XML comments).
- Web: a search for the extractor's death with a worker inside, and for attacks on a gas worker, found no
  StarCraft II source. A TL.net thread, "SC2 Mythbusters season 3" (`tl.net/forum/starcraft-2/303033`), lists items
  on both, but the fetch was refused (403) and its verdicts were not read. WP0 may try it again and records what it
  says as [COMM]; it does not change a default without the owner.
- No prototype. §3's claims are read from the code on `e313268f`; §7's predictions follow from §3.4 and D-2's
  timeline. P-0 measures the premise first.

---

## As built
