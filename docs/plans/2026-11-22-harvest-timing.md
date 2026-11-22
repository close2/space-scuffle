# Blueprint: Harvest Timing per Resource (B-150, B-151)

This plan gives harvesting StarCraft II 5.0.14's timings, taken from the resource rather than from the worker:

- **`ReturnDelay` (B-150).** A worker that has gathered from a mineral field stays at the field, holding its cargo,
  for the field's `ReturnDelay` (0.5 Normal seconds, 9 ticks) before it walks back. Today it walks back on the tick
  it gathers, so every mineral trip is 9 ticks short and income about 7 % high.
- **Harvest time and amount belong to the resource (B-151).** A mineral field yields 5 after 50 ticks; a geyser's
  extractor yields 4 after 35 ticks (`HarvestTime 1.981`, `HarvestAmount 4`). Today both come from the worker row
  (`harvest_rate_ticks` 50, `harvest_capacity` 5), so a gas trip yields 25 % too much and stays 15 ticks too long
  at the extractor.
- **The data moves.** Each resource kind's row in `assets/catalog/placement_rules.ron`, which already carries the
  game's `RemoveWhenEmpty` from the same `CBehaviorResource`, gains `harvest_time_ticks`, `harvest_amount` and
  `return_delay_ticks`. The worker rows keep only what is the worker's: that it can harvest. `HarvesterSpec` loses
  both fields.

A full plan is required (charter §6). It changes the checksum schema (a unit's return-delay countdown is hashed
state), both goldens are re-recorded, and it moves economy worlds: every harvest test and the client's skirmish
match.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-22 against `main` at
`dd17f4c5`. Every measurement of §7 is on that tree.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations | Closed. S-1 (RON data instead of editor data) covers authoring the timings in RON; S-5 (25 ticks per Faster second, whole-tick rounding) converts them | D-2, §2.3 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0002: scoping is the agent's call if written down | Standing rule | §14 |
| Q0091 (maps are the agents' to change) | Not used: no map changes | — |
| **None new** | The data decides every value. The interruptible wait (D-4), the worker's empty capability (D-3) and §14's items are recorded scoping decisions | §4, §14 |

---

## 1. Summary

| | Today (`dd17f4c5`) | After this plan |
|---|---|---|
| Mineral trip | Walk out, gather 50 ticks, walk back at once | Walk out, gather 50 ticks, **wait 9 ticks at the field with the cargo**, walk back |
| Gas trip | Gather 50 ticks, carry 5 | **Gather 35 ticks, carry 4** |
| Where the numbers live | `units.ron`, each worker's `harvester: (harvest_capacity: 5, harvest_rate_ticks: 50)` | `placement_rules.ron`, each resource kind's row: `harvest_time_ticks`, `harvest_amount`, `return_delay_ticks`; the worker's `harvester: Some(())` |
| Hashed state | `UnitTask { goal, legs, gather_ticks, wait_ticks, depot, last_node }` | Adds `return_ticks` after `gather_ticks`; schema 32 → 33 |
| Mineral income, one worker per field, showcase start (trip-period rate, §6) | 59.8 per worker-minute (window count 60.3) | **55.8** derived and measured to the tick (window count 55.6); the game's 53.6 to 61.2, base 57.8 |
| Gas income, one worker, the showcase's near geyser | 65.8 (114-tick trip, 5 a trip) | **60.6** (99-tick trip, 4 a trip); the game's 61 |
| Worlds that move | — | Harvest tests and the client's skirmish match. Matrix, battles 31 and 33, the showcase on nine seeds: byte-identical (nothing harvests, §7.3). Goldens: re-recorded for the schema only, proven by the old encoding reproducing them (§7.4) |
| Size | — | M: four packages, two of them M |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`), read as data from `~/scratch/combat_wp3_data/<mod>/`: "liberty
  Behavior:N" is line N of `liberty/BehaviorData.xml`, "core Abil:N" of `core/AbilData.xml`, and so on. Nothing
  there was executed.
- **[COMM]** Community documentation.
- **[INF]** An inference of this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | liberty Behavior:743–756 (`CBehaviorResource MineralFieldMinerals`) | `HarvestTime 2.786` (:747), `HarvestAmount 5` (:748), `Flags RemoveWhenEmpty 1` (:749), `ReturnDelay 0.5` (:750) [DATA] |
| S2 | liberty Behavior:757–771, 772–786 (`MineralFieldMineralsOpaque`, `…Opaque900`), 1492–1521 (`MineralFieldMinerals450`, `…750`) | The same `HarvestTime 2.786`, `HarvestAmount 5`, `ReturnDelay 0.5`. `HighYieldMineralFieldMinerals` (:787–800) the same with `HarvestAmount 7` [DATA] |
| S3 | libertymulti Behavior:6–17, swarmmulti :22–52, void :987–994, voidmulti :141–152, 263–272, 310–319, balancemulti :87–98, 207–216, 251–260 | Every later layer overrides only `Capacity`, `Contents`, `DepletionThreshold` and `IdealHarvesterCount` of the mineral rows: no layer changes `HarvestTime`, `HarvestAmount` or `ReturnDelay` [DATA, checked by `grep` of every layer] |
| S4 | liberty Behavior:801–808 (`RawVespeneGeyserGas`, the geyser itself) | `Contents 2500`, `HarvestAmount 4`, no `HarvestTime`, no `ReturnDelay`, no `RemoveWhenEmpty` [DATA] |
| S5 | liberty Behavior:817–828 (`HarvestableVespeneGeyserGas`), :829–840 (`…Protoss`), :841–852 (`…Zerg`) | The behaviour on the extractor of each race: `HarvestTime 1.981`, `HarvestAmount 4`, `Flags HideHarvesters 1`, no `ReturnDelay`. The three rows are identical in time and amount [DATA] |
| S6 | voidmulti Behavior:239–243, 382–385; balancemulti :185–189, :323–326 | Later layers change only the rich geyser's amount (8) and the geyser's contents (2,250): gas time and amount stay 1.981 and 4 [DATA] |
| S7 | core Behavior:81–85 (the `CBehaviorResource` default) | Sets `CarryResourceBehavior`, `EnabledSearchFilters`, `EnabledSearchRadius`; no `HarvestTime`, `HarvestAmount` or `ReturnDelay`. So a resource that names no `ReturnDelay` has the engine's default, taken to be 0 (**verify**: the engine default is not in the data; S10 supports it for gas) [DATA, INF] |
| S8 | core Abil:353–380 (the `CAbilHarvest` default) | Per resource: `ResourceAmountMultiplier 1` and `ResourceTimeMultiplier 1` for Minerals, Vespene, Terrazine, Custom (:371–378); `UninterruptibleArray WaitToReturn 1` (:379) [DATA] |
| S9 | liberty Abil:1094–1099, 2241–2243 (`SCVHarvest`, `ProbeHarvest`, `DroneHarvest`); voidmulti Abil:1890–1901; liberty Abil:703–714, balancemulti Abil:578–582 (`MULEGather`) | The three workers' harvest abilities set no multiplier, so a worker gathers the resource's time and amount unchanged; voidmulti makes `ApproachResource` and `WaitAtResource` cancelable. Only the MULE differs: `ResourceTimeMultiplier Minerals 2.05`, `ResourceAmountRequest Minerals 25`, `Range 0.5`, minerals only [DATA] |
| S10 | Liquipedia, [Resources](https://liquipedia.net/starcraft2/Resources), wikitext read through the MediaWiki API on 2026-11-22 (the same page plan `2026-11-20-resource-footprints` read as its S10 on 2026-11-20) | Minerals: "workers that mine 5 minerals per trip"; "A worker occupies a mineral field for 1.99 seconds. It then pauses for approximately 0.3571 seconds before returning at full speed. The next worker can already begin operations during the pause." "One worker on one mineral field gathers 55-60 minerals per minute". Gas: "Workers gathering gas have to spend 1.415 seconds in a Vespene Geyser before returning at full speed"; "Workers gather 4 gas per trip"; "One worker on a geyser gathers 61 gas per minute. Two workers … 123 … Three workers … 163"; "A fully saturated geyser can therefore procure no more than 169.61 gas per minute (60sec / 1.415sec * 4 gas)". MULE: "mines 25 minerals per trip" [COMM] |
| S11 | Liquipedia, [MULE (Legacy of the Void)](https://liquipedia.net/starcraft2/MULE_(Legacy_of_the_Void)) (as resource-footprints S11) | "SCVs average 61.22 minerals per minute on near blue mineral patches and 53.57 minerals per minute on far blue mineral patches (for two or less workers per patch)" [COMM] |

### 2.2 The rules that follow

1. **Time and amount are the resource's.** A worker gathering from a resource takes the resource's `HarvestTime`
   and yields its `HarvestAmount`, each multiplied by the worker ability's per-resource multiplier, which is 1 for
   every worker (S1, S5, S8, S9) [INF from the data's structure; S10's figures agree: 1.99 s and 5 for minerals,
   1.415 s and 4 for gas].
2. **Gas is gathered through the extractor.** The time comes from the extractor's `Harvestable…` behaviour (S5);
   the geyser's own row carries the same amount, 4 (S4). The three races' rows are identical, so one row per
   resource kind is exact [INF].
3. **`ReturnDelay` is a pause at the field after gathering.** The worker has finished `HarvestTime`, so it holds
   its cargo, and the field is already free for the next worker (S10, "The next worker can already begin
   operations during the pause"); then it returns at full speed. The ability's stage after gathering is
   `WaitToReturn` (S8) [INF: the stage's name and S10's description].
4. **Only minerals pause.** The gas rows name no `ReturnDelay` (S4, S5) and the default names none (S7); S10
   describes the gas worker "returning at full speed" after its 1.415 s. So gas pauses 0 [INF, **verify** the
   engine default].
5. **The pause is uninterruptible in the data** (`UninterruptibleArray WaitToReturn 1`, S8). What the engine does
   with an order given during it (defer it or refuse it) is not in the data (**verify**). This plan keeps the pause
   interruptible (D-4) and registers the difference.
6. **The MULE differs only by multipliers on the worker side** (S9). No catalog unit has a MULE's capability.

### 2.3 Values

At 25 ticks per Faster second with whole-tick rounding (S-5) and the game's 1.4 Faster factor, a time of `t`
Normal seconds is `round(t / 1.4 × 25)` ticks, halves up, as the catalog's other converted times (for example
`cast_backswing_ticks: 9` for `CastOutroTime 0.5`, `units.ron`).

| Resource kind | `HarvestTime` | Ticks | `HarvestAmount` | `ReturnDelay` | Ticks | Source |
|---|---|---|---|---|---|---|
| Mineral | 2.786 Normal s (1.990 s) | 49.75 → **50** | **5** | 0.5 Normal s (0.357 s) | 8.93 → **9** | S1, S3 |
| Gas | 1.981 Normal s (1.415 s) | 35.375 → **35** | **4** | none | **0** | S4 to S7 |

Rounding error per trip: mineral +0.25 + 0.07 ticks (+0.3 % of a 133-tick trip), gas −0.375 ticks (−0.4 % of a
99-tick trip). Both are below one tick and inside §6's tolerance.

---

## 3. Diagnosis

| # | Today | File, symbol |
|---|---|---|
| T1 | `HarvesterSpec { harvest_capacity: u32, harvest_rate_ticks: u16 }` is the worker's; all three harvester rows author 5 and 50 | `crates/sim_types/src/commands.rs` `HarvesterSpec`; `assets/catalog/units.ron` (three rows); `crates/sim_core/src/catalog.rs` `HarvesterSpecDef` |
| T2 | Gathering starts `gather_ticks = spec.harvest_rate_ticks.max(1)` on arrival and takes `spec.harvest_capacity.min(node.remaining)` when it reaches 0, for either kind | `crates/sim_core/src/economy/harvest.rs` `step_gather` |
| T3 | A worker holding cargo is sent to `step_delivery` on the next tick: no pause | `harvest.rs` `step_harvest` (`if actor.cargo.is_some()`) |
| T4 | The catalog refuses a harvester with `harvest_capacity` 0 ("would loop forever") | `crates/sim_core/src/catalog/validation.rs` `validate_harvester` |
| T5 | `placement_rules.ron`'s rows already carry one `CBehaviorResource` field, `remove_when_empty` (S1 :749), read into `sim_map::ResourceFootprint` | `assets/catalog/placement_rules.ron`; `crates/sim_core/src/catalog/placement_rules.rs` `ResourceFootprintDef` |
| T6 | No shipped scenario, matrix battle, golden or bench row issues a `Harvest` order (resource-footprints T7, re-checked on `dd17f4c5` by `grep` of `assets/scenarios`, `crates/sim_core/tests/fixtures`, `crates/sim_fixtures`, `crates/sim_core/benches`). Harvesting is exercised by `harvest_income_tests`, `harvest_edge_tests`, `full_match_tests`, `player_view_tests`, client unit and integration tests, and the client's `skirmish_match_tests` (its pilot sends every worker to a mineral and plays the match to a result within `TICK_LIMIT` 12,000) | — |

**Finding F-1 (the trip, measured).** On the showcase's slot-0 start a worker's trip is exactly its two walking
legs plus 50 ticks at the field (§7.1: 62 to 88 walking ticks a trip). The prototype adds the pause and moves
nothing else: every field's trip period grows by exactly 9 ticks and every gas trip shrinks by exactly 15. The
walking legs start and end at rest with or without the pause, so the trip geometry is the rule's invariant.

---

## 4. Design decisions

### D-1. `ReturnDelay` is a wait at the field, inside the `Harvest` order

- When a take yields cargo (`amount > 0`), the worker's `task.return_ticks` is set to its resource kind's
  `return_delay_ticks`. On each later tick of the `Harvest` order, a worker holding cargo with `return_ticks > 0`
  decrements it and does nothing else; at 0 it starts its delivery leg as today. So a delay of `d` ticks adds
  exactly `d` ticks to the trip (prototype: +9 on 27 of 29 mineral fields, +8.94 on the two whose window mean
  spans a drifting arrival, §7.1).
- The node is drawn down at the take, as today (S10: the field is free during the pause). A take that empties a
  field removes it at once (resource-footprints D-8); the worker then waits where the field stood.
- A take of 0 (a node emptied by another worker on the same tick) yields no cargo and no wait.
- The worker stands: no goal, `current_anim` `Idle` (set at the take, as today), and `is_mineral_walking` stays
  set, as for every harvest tick, so bodies pass it.
- `ReturnCargo` is unchanged: it is a different order, so it starts its delivery leg at once (D-4).
- The depot is chosen when the wait ends, at the first delivery tick, as today; a depot lost during the wait ends
  the order then (`NoDepot`), with the cargo kept (§7.5).

### D-2. Harvest time, amount and delay are data on the resource row

Each row of `placement_rules.ron` gains three required fields, with the game's lines in its comment:

```ron
(kind: Mineral, …, remove_when_empty: true,
    // MineralFieldMinerals (liberty BehaviorData:743-756): HarvestTime 2.786 Normal s = 49.75
    // ticks, HarvestAmount 5, ReturnDelay 0.5 Normal s = 8.93 ticks (S-5 whole-tick rounding).
    harvest_time_ticks: 50, harvest_amount: 5, return_delay_ticks: 9),
(kind: Gas, …, remove_when_empty: false,
    // HarvestableVespeneGeyserGas (liberty BehaviorData:817-852, the three races alike): HarvestTime
    // 1.981 Normal s = 35.375 ticks, HarvestAmount 4; no ReturnDelay.
    harvest_time_ticks: 35, harvest_amount: 4, return_delay_ticks: 0),
```

- **Where it is read.** `sim_core::catalog::placement_rules` reads them into a new public
  `ResourceHarvest { time_ticks: u16, amount: u32, return_delay_ticks: u16 }` per kind, held by `PlacementRules`
  beside its footprints and reached as `PlacementRules::harvest(kind) -> Option<&ResourceHarvest>`. `sim_map`
  does not need them, so `sim_map::ResourceFootprint` is unchanged.
- **Validation** (in `PlacementRulesFile::validate`, typed `CatalogInvalidValue` errors): `harvest_time_ticks ≥ 1`
  and `harvest_amount ≥ 1` (a kind that yields nothing would loop forever, T4's reason, now on the row that owns
  it). `return_delay_ticks` may be 0. Every kind already must have a row (resource-footprints D-1), so every kind
  has a harvest. Fields are required (no `serde(default)`): a catalog without them fails to load, loudly.
- **Why this file.** It holds one row per resource kind and already carries a field of the same game record
  (`remove_when_empty`, from the same `CBehaviorResource`). A separate file would split one game record over two.
  The file's head comment is extended to say its rows carry the resource's harvest data too; renaming the file
  is out of scope (§14).
- **The tick rule** is the catalog's: authored as whole ticks with the Normal-seconds value and the conversion in
  the comment (S-5). No conversion code is added.
- **Charter §3.3.** No kind or unit is named in code: the economy reads the row of the node's kind.

### D-3. The worker keeps only its capability

- `HarvesterSpec` becomes a struct with no fields (`pub struct HarvesterSpec {}`, derives kept), and every
  harvester row in `units.ron` becomes `harvester: Some(())`, with a comment pointing at the resource rows and S9
  (the workers' abilities set no multiplier). `HarvesterSpecDef` keeps `serde(deny_unknown_fields)`, so a catalog
  still naming `harvest_capacity` or `harvest_rate_ticks` is refused, not ignored (**verify** while implementing
  that `ron` reads `Some(())` into the empty struct; otherwise a unit struct `HarvesterSpecDef;`).
- `validate_harvester` is removed (its rule moves to D-2's row validation).
- Callers read `harvester.is_some()` already (client, `sim_fixtures`, scenario, published actions); only the
  economy and the harvest tests read the fields.
- **Not added:** the game's per-worker multipliers (S8, S9). They are 1 for every worker, and only a MULE
  analogue would need them. A plan that adds one adds `time_multiplier` and `amount_request` to `HarvesterSpec`
  then (§14).

### D-4. The wait is interruptible; the data's `WaitToReturn` is registered

Today a new order replaces a `Harvest` order on any tick, gathering included (`a_new_order_interrupts_gathering_cleanly`).
The wait is leg state like `gather_ticks`: `economy::fresh_leg` clears `return_ticks`, so any replacement drops it
and the cargo stays on the unit, as for an interrupted gather. The data marks `WaitToReturn` uninterruptible (S8);
what the engine does with an order given then is not sourced. Deferring orders by ability stage is a new mechanic
of command admission, so it is out of scope and registered as **B-160** (WP0): "An order given during the
`ReturnDelay` wait replaces it at once; 5.0.14 marks the stage uninterruptible (core AbilData:379); the engine's
handling is not sourced (**verify**)". It is a minor difference: at most 9 ticks per order.

### D-5. Hashed state: `UnitTask::return_ticks`, schema 33

- `UnitTask` gains `pub return_ticks: u16` ("ticks left waiting at the node with cargo before the return leg; zero
  when not waiting"), encoded after `gather_ticks` in `fields!(UnitTask { … })`.
- It decides what the unit does next, so it is hashed (the struct's invariant). Re-using `gather_ticks` for both
  stages would save the schema bump at the cost of one field meaning two things; this project pays the bump.
- `CHECKSUM_SCHEMA_VERSION` 32 → 33, with its doc line: "Schema 33 adds each unit's return-delay countdown,
  `UnitTask::return_ticks`, after `gather_ticks` (plan `2026-11-22-harvest-timing` D-5)."
- `checksum_state_tests::every_public_entity_field_changes_the_world_checksum` gains a `return_ticks` mutation.

### D-6. The income acceptance is a derivation, not a band fitted to a seed

See §6. The rule's effect on a trip is exact (D-1: +`return_delay_ticks`; D-2: gas time 50 → 35), and the trip
geometry (the two walking legs) is unchanged by it. So the expected income per field is derived from the base's
measured legs and the data, and the measured income must equal it within the measure's own resolution. The game's
published rates (S10, S11) are a second, coarser check: they bound the result, they are not its target.

### D-7. Economy worlds: reported, judged by trace to the rule

Nothing that decides a battle reads harvesting (T6), so the matrix, battles 31 and 33 and the showcase must be
byte-identical; any difference there is a defect. The worlds that move are those that harvest:

- **Every changed harvest test** is re-derived from a trace of the new rule: the tick a cargo appears, the tick
  the worker leaves the field, the tick of delivery. A changed value must be the old one plus the delay (minerals)
  or minus 15 ticks and with 4 for 5 (gas). No bound is loosened, and no constant is chosen to make a test pass.
- **The client's skirmish match** (`a_match_is_won_through_real_input_alone`) is reported: the tick of victory and
  the pilot's stage log before and after. Its result must stay `Victory` within `TICK_LIMIT` (the pilot's own
  limit, unchanged). If it misses, the package stops and the trace (bank over time, the stage the pilot waits in)
  goes to the orchestrator; the limit is not raised without an amendment.
- **Winners are reported, never targeted.** If a world that should be byte-identical differs, the first divergent
  tick is traced to a unit; if it is not traced to a harvest, it is a defect and the package stops.
- **Survivors.** If the matrix ever moved, crowd-slide D-2's placebo-aware rule would apply: a survivor miss on a
  seed of record is a stop unless one of the three placebo builds of the same base shows the same miss. Here the
  matrix must be byte-identical (§7.3), so the rule is a stated fallback that the prototype does not reach; the
  placebo builds are not rebuilt unless the matrix moves.

### D-8. Presentation

No client change is needed. The waiting worker publishes `Idle` (its `current_anim`) where the gathering worker
published `Cast`; the client's harvest icons and cards read `harvester.is_some()`. The showcase's timeline cannot
move (nothing in it harvests), so its storyboard is not re-checked unless §7.3's showcase rows differ (then WP3
re-checks it on radv under `with-gpu-lock`).

### D-9. Records

- B-150 and B-151 are fixed by WP2.
- **B-160** registered by WP0 (D-4).
- **B-61** (mineral-patch occupancy) is restated by WP0 to cover geysers: one worker at a time inside an
  extractor (`HideHarvesters`, S5), which caps a saturated geyser at 169.6 a minute (S10); ours has no cap (§7.2:
  three workers earn three times one). And when occupancy is built, a field is released at the take, before
  `ReturnDelay` (S10).

---

## 5. Determinism and checksum

- **Integer arithmetic only.** Times are whole ticks and amounts whole units, read once at load. No float is added
  to the simulation crates; the RON values are integers.
- **Hashed state:** D-5, schema 33. No other hashed field changes.
- **Re-records.** Both goldens (`golden_replay`, `golden_replay_orders`), once, in WP2: neither fixture harvests
  (T6), so `return_ticks` is 0 on every unit at every checkpoint and only the schema number and the encoding
  change. Proof header: "Re-recorded for plan 2026-11-22-harvest-timing WP2 (B-150, B-151): schema 33 hashes each
  unit's `UnitTask::return_ticks`. A build that omits the field and keeps schema 32 reproduces the previous
  checksums on all 20 + 60 checkpoints." Each re-record on native debug and release, `wasm32-wasip1` release and
  debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`). CI covers Windows and macOS
  ARM64 on the push.
- **`sim_map` pins:** unchanged (no `sim_map` change).
- **wasm.** No new dependency; the simulation crates still build for `wasm32-unknown-unknown`.

---

## 6. Expected income (derived before measuring)

### 6.1 The trip

A worker's steady trip on a field, in ticks, is

`T = L_out + H + D + L_back`

where `H` and `D` are the resource row's `harvest_time_ticks` and `return_delay_ticks` and `L_out`, `L_back` the
walking legs from the depot's contact point to the field's and back. Each leg starts and ends at rest whatever
`H` and `D` are, so the legs are a property of the map's geometry and the worker's mover, not of this rule. Hence,
per field, with `T₀` the base trip (`H` 50, `D` 0 for both kinds):

- **Mineral:** `T = T₀ + 9`; income per worker-minute `5 × 1,500 / (T₀ + 9)`.
- **Gas:** `T = T₀ − 50 + 35 = T₀ − 15`; income per worker-minute `4 × 1,500 / (T₀ − 15)`.

`T₀` is measured on the base as the mean delivery-to-delivery period over a window (§7.1); `L_out + L_back =
T₀ − 50` is the trip geometry. Workers walk through each other while harvesting and a node has no worker limit
(B-61), so with `k` workers on a field each worker's trip is the same `T` and the per-worker rate does not depend
on `k`.

### 6.2 The game's figures, as a second check

- **Minerals.** S11 gives 61.22 near and 53.57 far; S10's base (925 a minute with 16 workers) 57.8. The band of
  plan `2026-11-20-resource-footprints` §6.2 was these less the unmodelled `ReturnDelay`; that plan said it is
  re-derived when `ReturnDelay` is modelled, with the references as its centre. So, ±5 % (S10's 55 to 60 is ±4.3 %
  round its middle): on each showcase start, with `k` = 1 and `k` = 2, the mean trip-period rate over its eight
  fields lies in **[54.9, 60.7]** (57.8 × 0.95, × 1.05) and every field in **[50.9, 64.3]** (53.57 × 0.95 to
  61.22 × 1.05).
- **Gas.** S10: one worker 61 a minute on a ladder geyser, a 98.4-tick trip, 35.4 of them in the geyser and 63.0
  walking. Our geysers are where the maps put them (resource-footprints Amendment 2: the showcase's in the base
  corners), so only a geyser whose walking legs are within 10 % of the ladder's 63 ticks is compared: its rate in
  **[57.95, 64.05]** (61 ± 5 %). Others are reported.

### 6.3 Acceptance (fixed before measuring)

- **A-1, the trip.** Per field and kind, the measured trip period equals the derived `T` within ±1 tick (one
  arrival may drift by a tick between trips; the period is a mean over the window).
- **A-2, the income.** Per field, the window income per worker-minute equals the derived rate within one
  delivery's worth: ±2.5 minerals or ±2.0 gas a minute for a 3,000-tick window (one trip's amount over two
  minutes).
- **A-3, the game.** The trip-period rates lie in §6.2's bands. The window count is not used for the band: at
  2.5 a minute per delivery its quantization alone can cross a band edge (§7.1, field (4, 15): trip rate 51.0,
  window count 50.0).
- Scope of A-1 and A-2: every mineral field within 12 cells of both showcase starts and 15 of `default_arena` slot 1,
  the five of `default_arena` slot 0 within 18 (its authored line is 14 to 16 cells away), and every geyser within
  12 (showcase) or 15 (arena) of a start; `k` = 1, plus `k` = 2 on showcase slot 0 for minerals and `k` = 2 and 3
  for gas. A-3 applies to the showcase starts.

---

## 7. Prototype and measurements

Prototype: a scratch worktree of `dd17f4c5` (`.claude/worktrees/harvest-timing-proto`,
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/harvest-timing-proto`, removed at hand-back). Its diff is kept
at `~/scratch/harvest-timing/proto/proto.diff`. It:

- added `UnitTask::return_ticks`, cleared by `fresh_leg`, and D-1's wait in `step_harvest` and `step_gather`;
- took the mineral delay from `PROTO_RD=<ticks>` (default 0) and, with `PROTO_GAS=1`, gas time 35 and amount 4
  (default the worker's 50 and 5), so one binary is both the base and the candidate;
- with `PROTO_HASH=1`, hashed `return_ticks` after `gather_ticks` under schema 33; without it, the encoding and
  schema are `dd17f4c5`'s.

It did not move the data to the resource rows (D-2, D-3): those change where the numbers are read, not what they
are, and WP1 proves that move bit-identical on its own.

Tool: `~/scratch/harvest-timing/proto/harvest_measure.rs` (copied into `crates/sim_core/examples/` of the scratch
tree, not committed): per node of a kind within a radius of a start, a fresh world with the hall, for gas a
completed extractor over the geyser, and `k` workers beside the hall; 750 ticks of warm-up, then over 3,000
ticks the income, worker 0's deliveries and its ticks split into out, gather, delay and back.
`harvest_measure <map> <slot> <mineral|gas> <k> 750 3000 <radius>`.

### 7.1 Minerals

`showcase_canyon` slot 0, `k` = 1. `T₀` base trip, `T` candidate trip, both measured; derived `T₀ + 9`; rates per
worker-minute (trip-period rate `7,500 / T`, and the window count). Legs are the walking ticks per trip (worker 0's
phase split puts them about evenly out and back, for example 39 + 39 on (3, 14)).

| Field (cell) | `T₀` | Derived | `T` | Legs `T − 59` | Derived rate | Window count (base → cand) |
|---|---:|---:|---:|---|---:|---|
| (3, 14) | 128 | 137 | 137 | 78 | 54.74 | 60.0 → 55.0 |
| (6, 14) | 112 | 121 | 121 | 62 | 61.98 | 67.5 → 62.5 |
| (9, 14) | 114 | 123 | 123 | 64 | 60.98 | 65.0 → 60.0 |
| (12, 14) | 128 | 137 | 137 | 78 | 54.74 | 60.0 → 55.0 |
| (4, 15) | 138 | 147 | 147 | 88 | 51.02 | 55.0 → 50.0 |
| (7, 15) | 126 | 135 | 135 | 76 | 55.56 | 60.0 → 55.0 |
| (10, 15) | 138 | 147 | 147 | 88 | 51.02 | 55.0 → 50.0 |
| (13, 13) | 124 | 133 | 133 | 74 | 56.39 | 60.0 → 57.5 |
| **Mean** | | | | | **55.80** | 60.31 → **55.63** |

- **A-1:** every measured `T` equals the derived trip exactly. **A-2:** every window count within 1.25 of the
  derived rate (bound 2.5).
- **A-3:** mean 55.80 in [54.9, 60.7]; fields 51.02 to 61.98 in [50.9, 64.3].
- **`k` = 2** (slot 0): the same trip periods on every field; window counts 55.0, 62.5, 61.25, 55.0, 50.0, 55.0,
  50.0, 56.25, mean 55.63 (base 60.47). **Slot 1** is slot 0's mirror field for field (identical figures).
- **`default_arena` slot 1** (radius 15): `T₀` 120, 114, 114, 134, 130, 126, 144, 167; `T` 129, 123, 123, 143,
  139, 135, 153, 176 (+9 each); derived mean 54.21, window mean 58.12 → 54.38; A-2 within 1.86.
- **`default_arena` slot 0** (radius 18, the authored far line): `T₀` 232, 228.33, 228.33, 232, 254; `T` 241,
  237.27, 237.27, 241, 263 (+9, +8.94, +8.94, +9, +9: within A-1's tick); window 32.0 → 30.0; A-2 within 1.5.

The base row reproduces resource-footprints' evidence (60.31, 60.47, 58.12): the prototype without switches is
the base.

### 7.2 Gas

`k` = 1. Derived `T₀ − 15`; rate `6,000 / T`.

| Map, slot, geyser | `T₀` | Derived | `T` | Legs `T − 35` | Derived rate | Window count (base → cand) |
|---|---:|---:|---:|---|---:|---|
| Showcase 0, (10, 1) | 114 | 99 | 99 | 64 | **60.61** | 65.0 → 60.0 |
| Showcase 0, (1, 1) (corner) | 150 | 135 | 135 | 100 | 44.44 | 50.0 → 44.0 |
| Showcase 1, (53, 62) | 114 | 99 | 99 | 64 | 60.61 | 65.0 → 60.0 |
| Showcase 1, (62, 62) (corner) | 150 | 135 | 135 | 100 | 44.44 | 50.0 → 44.0 |
| Arena 0, (13, 12) | 142 | 127 | 127 | 92 | 47.24 | 52.5 → 48.0 |
| Arena 1, (50, 56) | 114 | 99 | 99 | 64 | 60.61 | 65.0 → 60.0 |

- **A-1:** exact on every geyser. **A-2:** every window count within 0.8 of the derived rate (bound 2.0).
- **A-3:** the near geysers' legs are 64 ticks against the ladder's 63 (within 10 %), and their rate 60.61 lies in
  [57.95, 64.05]: the game's 61 within 0.7 %. The corner geysers' legs (100) are the map's layout; reported.
- **`k` = 2 and 3:** per-worker rates identical to `k` = 1 (no extractor occupancy, B-61 as restated by D-9; the
  game's three workers earn 163, ours 182).

### 7.3 Worlds that do not harvest

Base (no switches) against the candidate (`PROTO_GAS=1 PROTO_RD=9`), the same binary; commands of
resource-footprints D-13 (`~/scratch/resource-footprints/scripts/measure.sh`), output under
`~/scratch/harvest-timing/proto/world/`.

| Run | Result |
|---|---|
| Matrix, every battle, seeds of record and 16 | Byte-identical (all output files; the logs differ only in their output path) |
| Battle 31, 100 seeds | Byte-identical |
| Battle 33, 64 seeds | Byte-identical |
| Showcase, default seed and seeds 1 to 8, 6,000 ticks | Byte-identical `units.csv` and summaries: player 1 wins at 2622, 2537, 2544, 2180, 2601, 1939, 2192, 2530, 2154 (as resource-footprints' evidence §4); reversals 175; give-ups 0; deeper unit-ticks 0 |

405 output files compared, none differs. Nothing in these worlds harvests (T6), so nothing could move: they are the
control that the rule adds nothing where no worker gathers. The placebo builds were not rebuilt (D-7: the
survivor rule is not reached).

### 7.4 Goldens

`cargo test --release -p sim_core --test golden_replay_tests` on the prototype, native (logs in
`~/scratch/harvest-timing/proto/golden/`):

| Build | Result |
|---|---|
| No switches (the base) | 4 of 4 pass: the prototype is a clean control |
| Candidate rule (`PROTO_RD=9 PROTO_GAS=1`), old encoding and schema 32 | 4 of 4 pass: the rule changes no golden state (nothing harvests) |
| Candidate rule with `PROTO_HASH=1` (schema 33, `return_ticks` hashed) | `golden_replay` and `golden_replay_orders` diverge at checkpoint 0 (the schema number), the two property tests pass |

So the re-record of WP2 is the schema alone, and the second row is its proof. WP2 repeats both rows on native
debug and release and `wasm32-wasip1` release and debug.

### 7.5 Tests on the prototype

`cargo test --release -p sim_core --no-fail-fast` with the candidate rule (`PROTO_RD=9 PROTO_GAS=1`, old encoding):
1,038 passed, 2 failed (`~/scratch/harvest-timing/proto/sim_core_tests.log`). Both are this plan's:

| Test | Why | WP |
|---|---|---|
| `harvest_edge_tests::income_lies_in_the_band` | Its band is resource-footprints' (the references less `ReturnDelay`), and it reads the window count: field (4, 15) counts 50.0 against the old floor 54.3. §6.2 re-derives the band and A-3 moves the test to the trip-period rate (51.02 on that field) | WP2 |
| `harvest_income_tests::losing_the_depot_ends_the_order_and_keeps_the_cargo` | It kills the depot on the tick the cargo appears and expects the order ended two ticks later; the worker now looks for its depot only after the wait (D-1), so the order ends `return_delay_ticks` + 1 ticks after the take. The test is re-derived to step that many ticks, read from the mineral row; its assertions stay | WP2 |

`full_match_tests`, `player_view_tests`, `harvest_income_tests`' gas test and every golden pass. The suite was not
run with `PROTO_HASH=1`; WP2's gate covers the tests that pin checksums (`checksum_state_tests` and the goldens).
Client tests were not run (§9, R-1, R-5).

---

## 8. Stop rules and acceptance

1. **WP1 (data move) is bit-identical everywhere.** The same values read from the new place: matrix, battles 31
   and 33, the showcase's nine `units.csv`, both goldens on all four builds, and every harvest test unchanged
   (`harvest_income_tests`, `harvest_edge_tests`, `full_match_tests`, `player_view_tests`). WP1 authors the
   resource rows with the worker's **current** values for both kinds (mineral 50 / 5 / 0, gas 50 / 5 / 0), so no
   behaviour changes; a difference is a defect and stops the package.
2. **WP2 (the rule) moves the expected worlds and nothing else.**
   - Matrix, battles 31 and 33, the showcase on nine seeds: byte-identical to WP1 (§7.3). A difference is a defect.
   - Goldens: re-recorded once, with §5's header and proof on four builds. If the old encoding with schema 32 does
     not reproduce the previous checksums, the package stops.
   - Income: A-1, A-2 and A-3 of §6.3 on WP2's tree against WP1's `T₀`. A miss is traced (the field's legs out and
     back, the arrival points) before anything changes; an unexplained miss stops the package.
   - Every changed harvest test re-derived from a trace (D-7); a failure not listed in §7.5 is a finding and stops
     the package.
   - The skirmish match: `Victory` within `TICK_LIMIT`, its tick reported against WP1's (D-7).
3. **No tuning constant** is introduced. Every number comes from §2.3 with its line.

---

## 9. Client consequences

None in code (D-8). The client tests that harvest (`battle_card_path_tests::gas_through_the_card_extractor_snap_harvest_and_income`,
`battle_context_order_tests`, `battle_fog_tests`, `battle_hud_tests`, `battle_pipeline` unit tests,
`skirmish_match_tests`, `sync_steady_state_tests`, `battle_flipbook_tests`) run in WP2's package gate. A test that
waits for income with a tick bound is re-derived from the trip of §6 if it fails; one that names 5 gas per trip
reads the gas row's amount instead (charter §3.3: no literal from the catalog in a test where the row can be read).

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP3 | B-160 added; B-61 restated (D-9); B-150 and B-151 fixed; the Economy row |
| `docs/plans/README.md` | plan, WP3 | Row `Proposed` with the plan; `Done` in WP3 |
| `assets/catalog/placement_rules.ron` | WP1, WP2 | Three fields per row (WP1 today's values, WP2 the game's) |
| `assets/catalog/units.ron` | WP1 | `harvester: Some(())` on the three harvester rows |
| `crates/sim_types/src/commands.rs` | WP1 | `HarvesterSpec` with no fields |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs`, `catalog/placement_rules.rs` | WP1 | `HarvesterSpecDef`; `validate_harvester` removed; `ResourceHarvest`, `PlacementRules::harvest`, the row validation |
| `crates/sim_core/src/economy/harvest.rs`, `economy/mod.rs`, `entity.rs` | WP1 (reads), WP2 (the wait) | `step_gather` reads the row; `return_ticks`; `fresh_leg` |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | WP2 | Schema 33 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron` | WP2 | `checksums` re-recorded under a "Re-recorded for …" header (§5) |
| `crates/sim_core/tests/harvest_income_tests.rs`, `harvest_edge_tests.rs`, `harvest_timing_tests.rs` (new), `checksum_state_tests.rs`, `catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs` | WP1, WP2 | §12 |
| `crates/sim_core/src/catalog/placement_rules.rs` `mod tests` | WP1 | Parse and validation unit tests |
| `docs/evidence/2026-11-22-harvest-timing.md` | WP2, WP3 | Figures of §6.3 on the as-built trees |

Nothing in `crates/sim_core/src/movement/` changes: the parallel movement work there does not overlap (R-3).

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR`; the implementation agent runs
`cargo gate quick` and the tests its change needs and hands back; a verifier runs `cargo gate package` (Q0061).

### WP0: records (S, docs only)

- `DESIGN.md` §4: B-160 (D-4) added; B-61 restated (D-9); B-150 and B-151 point to this plan.
- **Gate:** docs-only `package`.

### WP1: the data moves, values unchanged (M, `sim_types`, `sim_core`, catalog)

Failing tests first (red on `dd17f4c5`, they do not compile or fail on load):
- `catalog/placement_rules.rs` `mod tests`: a row without `harvest_time_ticks` is refused
  (`RonDeserializationError`); `harvest_time_ticks: 0` and `harvest_amount: 0` are refused
  (`CatalogInvalidValue`, naming the field); `return_delay_ticks: 0` is accepted; `harvest(kind)` returns the
  row's values.
- `runtime_catalog_tests`: a copy of the shipped catalog with the mineral row's `harvest_time_ticks` and
  `harvest_amount` changed loads in the same binary, and a worker in a world built from it gathers for the new
  time and carries the new amount (charter §3.3, zero recompilation).
- `harvest_income_tests::gathering_takes_the_authored_time` and the gas test read time and amount from
  `placement_rules().harvest(kind)`, not from the worker.
- A units catalog whose harvester names `harvest_capacity` is refused (`deny_unknown_fields`).

Then the change: D-2 and D-3 with **today's values on both rows** (50 / 5 / 0). `step_gather` reads the row of the
node's kind. `return_ticks` is **not** added yet.

**Acceptance:** §8 rule 1, bit-identical everywhere (matrix, battles 31 and 33, showcase nine seeds, both goldens
on native release and `wasm32-wasip1` release, every harvest test unchanged in value).

### WP2: the rule (M, `sim_core`, catalog; schema 33)

Failing tests first (red on WP1's tree), in a new `crates/sim_core/tests/harvest_timing_tests.rs`, values read
from the catalog, never written as literals:
- **`a_worker_waits_the_return_delay_at_the_field`:** from the tick its cargo appears, a mineral worker's position
  is unchanged for exactly `return_delay_ticks` ticks and its `task.return_ticks` counts down to 0; on the next
  tick it has a goal toward the depot.
- **`a_gas_worker_leaves_at_once`:** with the gas row's delay 0, the worker has its depot goal on the tick after its
  cargo appears; its cargo is the gas row's `harvest_amount` and its gather lasted the gas row's
  `harvest_time_ticks`.
- **`the_field_is_drawn_down_before_the_wait`:** the node's `remaining` falls on the tick of the take, not after
  the wait; a take that empties a field removes it before the wait (D-1).
- **`a_new_order_during_the_wait_drops_it_and_keeps_the_cargo`:** a `Move` during the wait: `return_ticks` is 0,
  the cargo stays (D-4, the B-160 behaviour), and a later `Harvest` delivers first.
- **`the_trip_is_the_legs_plus_the_resource_times`:** on the showcase's slot-0 start, field (6, 14): the trip
  period over the window equals the legs measured in the same run plus `harvest_time_ticks` plus
  `return_delay_ticks` (an identity that pins D-1's accounting without a fitted number).
- **`income_lies_in_the_band`** (`harvest_edge_tests`): re-derived by §6.2: trip-period rates (deliveries over the
  window), mean in [54.9, 60.7], every field in [50.9, 64.3]; the doc comment cites §6.2.
- **`a_near_geyser_earns_the_games_rate`:** showcase slot 0, geyser (10, 1): trip-period rate in [57.95, 64.05]
  (§6.2).
- `checksum_state_tests`: the `return_ticks` mutation (D-5).
- `harvest_income_tests::losing_the_depot_ends_the_order_and_keeps_the_cargo` re-derived (§7.5): the order ends
  `return_delay_ticks` + 1 ticks after the take.
- `catalog_reference_values_tests::the_shipped_resource_harvests_are_5_0_14`: both rows against §2.3 with their
  lines.

Then the change: D-1, D-4, D-5; the rows of `placement_rules.ron` set to §2.3's values with D-2's comments.

**Acceptance:** §8 rule 2. Measure §6.3 on WP1's tree (`T₀`) and WP2's (`T`) with the scratch tool of §7, and
record both tables in the evidence. Re-record the goldens on four builds with the proof.

### WP3: bookkeeping (S)

- `DESIGN.md`: B-150 and B-151 fixed (commit, the figures of §6.3 as built); the Economy row says harvest timings
  are the resource's, with this plan's link; evidence column adds `harvest_timing_tests.rs`.
- `docs/evidence/2026-11-22-harvest-timing.md` completed; this plan's `## As built`; README row `Done`.
- **Gate:** docs-only `package`.

---

## 12. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Data | Harvest fields read at run time, validated, sourced; the worker keeps no numbers | WP1: `placement_rules` unit tests, `runtime_catalog_tests`, `catalog_reference_values_tests` |
| Rule | The wait's length and place, the gas row's time and amount, the take before the wait, interruption | WP2: `harvest_timing_tests` |
| Income | The trip identity; the game's bands on the trip-period rate | WP2: `harvest_timing_tests`, `harvest_edge_tests::income_lies_in_the_band` |
| Hashing | `return_ticks` changes the checksum | WP2: `checksum_state_tests` |
| Determinism | WP1 bit-identical; WP2's golden re-record proven by the old encoding on four builds | §8 |
| Client | The skirmish match still won within its limit | WP2's package gate |

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The skirmish pilot's match runs past `TICK_LIMIT` with 7 % less mineral income and 8 % less gas (not run on the prototype: it is a GPU-class client target) | WP2 stops | D-7: reported with its trace; the limit is not raised without an amendment. The pilot's play is stage-driven, so a slower bank delays stages by about the income ratio |
| R-2 | `ron` does not read `Some(())` into an empty struct with `deny_unknown_fields` | A different authored form | D-3's fallback (a unit struct); either way a stale field is refused |
| R-3 | The parallel contact-solver work (`contact-solver-*` branches, B-157, B-158) bumps the checksum schema or re-records the goldens first | A conflicting schema number and re-record | Whoever merges second takes the next schema number and re-records on the merged tree, with this plan's proof (the old encoding reproduces the merged tree's goldens); this plan touches no file under `movement/` |
| R-4 | The engine's `ReturnDelay` or default differs (S7, rule 5, **verify**) | Income off by up to 9 ticks a trip | S10's description and figures agree with the data as read; the gas comparison of §7.2 is within 0.7 % |
| R-5 | A client test bounds a wait for income in ticks | A red test in WP2's gate | §9: re-derived from §6's trip, never loosened without the derivation |

---

## 14. Out of scope and decisions recorded (Q0002)

- **The uninterruptible `WaitToReturn`** (S8): registered as B-160 (D-4).
- **Occupancy** of mineral fields and extractors (B-61, restated by D-9): a third mineral worker, and a geyser
  saturated by three, are not banded.
- **The MULE** and per-worker multipliers (`ResourceTimeMultiplier`, `ResourceAmountRequest`, S9): no catalog unit
  has them (D-3).
- **Rich and high-yield nodes** (`HarvestAmount` 7 and 6 or 8): no such kinds exist here.
- **Mineral and gas contents** (1,800 / 900 per field, 2,250 per geyser on ladder maps): map balance, as
  resource-footprints §13 recorded.
- **Renaming `placement_rules.ron`** now that its rows carry harvest data too: a rename touches the loader, the
  baker's catalog list and the charter's file list for no behaviour; D-2 extends the file's head comment instead.
- **`IdealHarvesterCount`, `DepletionThreshold`, `ExhaustedAlert`**: presentation and alerts, not harvesting.

---

## 15. Prototype material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/{BehaviorData,AbilData}.xml`
  (`5444e12f13`), read as data. Liquipedia `Resources` wikitext through the MediaWiki API on 2026-11-22.
- Prototype diff: `~/scratch/harvest-timing/proto/proto.diff`; the measure `harvest_measure.rs` and
  `world.sh` beside it; binaries in `~/scratch/harvest-timing/bin-proto/`.
- Results: `~/scratch/harvest-timing/proto/income/` (`base-*` and `cand-*` CSVs per map, slot, kind and `k`),
  `world/` (base and candidate), `golden/`, `sim_core_tests.log`.
- The scratch worktree `harvest-timing-proto` and its target directory were removed at hand-back; the diff
  rebuilds the prototype on `dd17f4c5`.
