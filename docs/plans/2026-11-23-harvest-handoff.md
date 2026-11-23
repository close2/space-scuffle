# Blueprint: The Hand-off Between Two Gatherers (B-162)

This plan settles bug B-162 of the register against StarCraft II 5.0.14:

- **A resource is handed to the next worker in its line a whole engine step or two after the take, not on the same
  tick.** Today the next worker starts gathering on the tick the previous one takes (plan
  `2026-11-23-harvest-occupancy` D-1, D-2), so a saturated mineral field earns exactly one load per `HarvestTime`
  (150 a minute) and a saturated extractor 171.4. The game's saturated figures are about 5 % and 4 % lower (143 and
  163, Liquipedia).
- **The loss is not movement.** Read in the engine's own unit of time, the game loop (1/16 Normal second), the game's
  saturated period is a whole number of loops: 47 for a mineral field, 33 for a geyser, against a `HarvestTime` of
  44.576 and 31.696 loops that the engine ends on loops 45 and 32. So the hand-off is 2 loops on a field and 1 on a
  geyser. A worker with the data's acceleration needs 5 ticks or more to move even 0.05 world units from rest
  (§2.3), so these short gaps leave no time for the next worker to step into a new position: in the game it does not
  move. The gap is the ability's stage machine handing the resource over. The field is released when the gatherer
  enters `WaitToReturn`, one step after the take, and the next worker starts one step after the release. A geyser
  has no `ReturnDelay`, so it has no `WaitToReturn` stage and is released at the take.
- **The rule becomes data and one hashed fact.** Each resource kind's row in `assets/catalog/placement_rules.ron`
  gains `handoff_ticks` (mineral 2, gas 1). The world records, per node, the tick from which its line may start its
  next gather (`ResourceLines`, hashed). Occupancy, which the occupancy plan derives, also counts a node as occupied
  until that tick.

Nothing under `crates/sim_core/src/movement/` changes. The mechanism is inferred, not read from the data, so it is
marked **verify** throughout, and a draft owner question (§15) asks whether to adopt it. The default meanwhile is to
adopt it. It is data, so `handoff_ticks: 0` restores today's behaviour without a code change.

A full plan is required (charter §6): it changes the economy mechanic, adds hashed state (checksum schema), re-records
both goldens for the schema, and moves every economy world with a waiting worker.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`e8cc3f80` (checksum schema 37).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations | Closed. S-1 (RON data) covers authoring `handoff_ticks`; S-5 (25 ticks per Faster second, whole-tick rounding) converts loops to ticks and also explains the remaining gap to the game's figures (§6.3) | D-2, §6.3 |
| Q0008: where the specification is silent, search for what StarCraft II does | Followed: the data, Liquipedia and a web search (§2.1 S8) give no frame-level account of the hand-off | §2 |
| Q0002: scoping is the agent's call if written down | Standing rule | D-7, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed: §6 derives every figure before measuring; the game's figures are a sanity band, not a target | §6 |
| **Draft, not filed:** "Adopt the inferred engine hand-off (2 ticks a field, 1 a geyser) for B-162?" | For the orchestrator to file as Q0100 (§15); default (a), adopt | D-1, D-2 |

---

## 1. Summary

| | Today (`e8cc3f80`) | After this plan |
|---|---|---|
| Next worker in a line | Granted the node on the tick of the take; its gather ends `harvest_time_ticks` later | Granted `handoff_ticks` after the take (the node counts as occupied until then); its gather is unchanged |
| A worker arriving at a free node | Starts at once | Unchanged (A-4 of the occupancy plan holds: one worker a node is byte-identical in behaviour) |
| Where the number lives | — | `placement_rules.ron`, per kind: `handoff_ticks` (mineral 2, gas 1) |
| Hashed state | — | `SimWorld::resource_lines`: per node, the tick from which its line may start; schema **next free at merge** (38 unless another branch takes it) |
| Saturated mineral field, per minute | 150.0 (5 × 1,500 / 50) | **144.2** (5 × 1,500 / 52); the game's 143 (theory 150.75) |
| Saturated extractor, per minute | 171.4 (4 × 1,500 / 35) | **166.7** (4 × 1,500 / 36); the game's 163 (theory 169.6) |
| Showcase mineral line, three a field | 1,200 | **1,153.8**; the game's "about 1,140" |
| Worlds that move | — | Economy worlds in which a worker waits in a line. Matrix, battles 31 and 33, the showcase on nine seeds byte-identical (nothing harvests); goldens by the schema only |
| Size | — | M: four packages, WP1 M, the rest S or XS |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`), read as data from `~/scratch/combat_wp3_data/<mod>/`: "core Abil:N"
  is line N of `core/AbilData.xml`, "liberty Unit:N" of `liberty/UnitData.xml`, and so on. Nothing there was
  executed.
- **[COMM]** Community documentation.
- **[INF]** An inference of this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | core Abil:353–380 (the `CAbilHarvest` default), inherited unchanged by `SCVHarvest`, `ProbeHarvest`, `DroneHarvest` (liberty Abil:1094–1099, 2241–2243) | No field of the ability names a delay between two gatherers. It has no `Range` either (only `MULEGather` sets one, `Range 0.5`, liberty Abil:706), so a worker gathers in contact [DATA] |
| S2 | voidmulti Abil:1890–1901; core Abil:379 | The ability's named stages: `ApproachResource`, `WaitAtResource` (cancelable), `WaitToReturn` (uninterruptible). The gather itself lies between `WaitAtResource` and `WaitToReturn` [DATA: the names; INF: the order] |
| S3 | liberty Behavior:743–756 (`MineralFieldMinerals`), :817–852 (`HarvestableVespeneGeyserGas`, `…Protoss`, `…Zerg`), core Behavior:81–85 (the `CBehaviorResource` default) | Mineral: `HarvestTime 2.786`, `ReturnDelay 0.5`. Extractor: `HarvestTime 1.981`, no `ReturnDelay`. No field names a hand-off, a queue delay or a waiting position. Every later layer changes only capacity, contents and amounts (plan `2026-11-22-harvest-timing` S3, S6) [DATA] |
| S4 | liberty Unit:4722–4806 (`SCV`), :3061–3157 (`Probe`), :7030–7122 (`Drone`) | `Speed 2.8125`, `Acceleration 2.5`, `TurningRate` and `StationaryTurningRate 999.8437`, `Radius 0.375` on all three (`LateralAcceleration` 46 or 46.0625). Our worker rows carry the same values per tick: `max_speed 0.1575`, `acceleration 0.00784`, `turn_rate_deg_per_tick 55.99` (`assets/catalog/units.ron`) [DATA] |
| S5 | Liquipedia, [Resources](https://liquipedia.net/starcraft2/Resources), wikitext through the MediaWiki API, saved on 2026-11-23 in `~/scratch/harvest-occupancy/web/Resources.json` | "Three workers on one mineral field gather 143 minerals per game minute (200 for gold minerals), regardless of distance"; "In theory … 1206.03 minerals per minute (60sec / 1.99sec * 5 minerals * 8 mineral fields)"; "about 1140" for a base of 8 fields with three each; "Workers gathering gas have to spend 1.415 seconds in a Vespene Geyser"; "A fully saturated geyser can therefore procure no more than 169.61 gas per minute"; "Three workers on a geyser gather 163 gas per minute"; "The next worker can already begin operations during the pause" [COMM] |
| S6 | Liquipedia, [Mining Minerals](https://liquipedia.net/starcraft2/Mineral), saved as `Mining_Minerals.json` beside S5 | "any patch with 3 workers will yield exactly the same rate regardless of distance. This is because the limiting factor becomes the time 'on' the mineral patch, while the difference in travel distances is nullified by having a worker that is always waiting to be 'on' the patch"; for gas, "once they get a 3rd, the rates become exactly the same, due to the waiting of a worker at all times" [COMM] |
| S7 | The engine's clock: 16 game loops per Normal second, 22.4 per Faster second (the 1.4 factor of plan `2026-11-22-harvest-timing` §2.3) | The unit in which the engine runs every timer [COMM, the standard figure; **verify** that harvest timers end on whole loops] |
| S8 | Web search on 2026-11-23 (Team Liquid threads "A treatise on the economy of SCII", "LotV economy: worker pairing", Blizzard forum "Mineral Saturation"; the Team Liquid pages refused the fetch, HTTP 403) | No source found that gives the hand-off in loops or describes the waiting worker's position. The search summaries describe a "check, wait, harvest" cycle: the waiter waits at the patch for its turn [COMM, indirect] |

### 2.2 The hand-off read in game loops

S5's saturated figures, turned into a period per load and read in loops (S7):

| Case | Figure (S5) | Period per load | In loops | Rounded figure's range (±0.5 a minute) | `HarvestTime` in loops (S3) | Ended on loop | Gap |
|---|---:|---:|---:|---|---:|---:|---:|
| Mineral field, 3 workers | 143 / min, 5 a load | 2.0979 s | **46.99** | 46.83 to 47.16 | 44.576 | 45 | **2** |
| Gold field, 3 workers | 200 / min, 7 a load | 2.1000 s | **47.04** | 46.92 to 47.16 | 44.576 (S3: the same `HarvestTime`) | 45 | **2** |
| Geyser, 3 workers | 163 / min, 4 a load | 1.4724 s | **32.98** | 32.88 to 33.08 | 31.696 | 32 | **1** |

- All three periods are whole loops within the figures' rounding, and the gold field, which shares the mineral
  `HarvestTime` but yields 7, gives the same 47 loops. So the loss is a time added to each load, not a change in
  amount [INF].
- If the engine did not end a timer on a whole loop, the gaps would be 2.42 and 1.30 loops. Either reading gives
  mineral about 2 and gas about 1 [INF].
- 47 loops gives 142.98 a minute and 33 loops 162.91: S5's figures. The data's own theory (`HarvestTime` unrounded,
  no gap) gives 150.75 and 169.61, as S5 states.

### 2.3 Why it is not movement

The waiter could lose time by stepping from where it waits to where it gathers, by turning, or by accelerating.

- **Moving.** A worker starts from rest at the data's acceleration (S4: 2.5 world units per Normal second², 0.00784
  per tick²). To move `d` from rest and stop takes at least `2 √(d / a)` ticks: 5.1 ticks for 0.05 world units,
  13.8 for 0.375 (one worker radius), 19.6 for 0.75. The measured gap is 1 to 2 loops, which is 1.1 to 2.7 ticks.
  So the game's waiter moves at most about 0.01 world units into position: it waits where it gathers [INF].
- **Turning.** At 999.84° a Normal second a worker turns 62.5° a loop. A turn could cost one or two loops, but it
  would depend on the angle at which the waiter arrived, and S5 and S6 give the same figure "regardless of
  distance". It would also not explain why gas, whose waiter arrives from the same depot, loses half as much [INF].
- **Consequence.** No movement model reproduces a constant 2-loop and 1-loop gap without an unsourced waiting
  geometry, and fitting that geometry to the figures would fit a test set. The movement code (`movement/`, owned by
  an active branch) is not touched.

### 2.4 The rules that follow

1. **The hand-off is the stage machine's.** A resource with a waiting worker is released to it in whole engine
   steps after the take: one step for the next worker to start, plus one if the gatherer passes through a
   `WaitToReturn` stage, because the field is released when that stage begins (S2, S5: "The next worker can already
   begin operations during the pause"). So a mineral field hands off in 2 steps and an extractor in 1 [INF from
   §2.2's whole-loop gaps and S2's stages; **verify**: the engine's code is not public].
2. **The waiter does not move.** It waits in contact and starts where it stands (§2.3) [INF].
3. **One step is one of our ticks.** S-5 converts the game's loops to whole ticks: 2 loops (2.23 ticks) are 2 ticks
   and 1 loop (1.12 ticks) is 1 tick [S-5].
4. **A worker that reaches a free node starts as today.** No figure measures a step for a lone worker's
   `ApproachResource` → gather (S5's one-worker figures are a 55 to 60 band), so this plan adds none (§14) [scoping].

### 2.5 Values

| Resource kind | `ReturnDelay` (S3) | Steps | `handoff_ticks` | Saturated period | Per minute |
|---|---|---:|---:|---:|---:|
| Mineral | 0.5 Normal s | 1 + 1 | **2** | 50 + 2 = 52 ticks | 7,500 / 52 = **144.23** |
| Gas | none | 1 | **1** | 35 + 1 = 36 ticks | 6,000 / 36 = **166.67** |

---

## 3. Diagnosis

| # | Today | File, symbol |
|---|---|---|
| T1 | A node is occupied while some alive unit with `Harvest` on it has `gather_ticks > 0`. The take sets `gather_ticks` to 0, so the node is free in the same tick's resolution, and the first claimant is granted with `gather_ticks = time_ticks` | `crates/sim_core/src/economy/harvest.rs` `tally_nodes`, `grant_claims`, `step_gather` |
| T2 | A waiter halts where its body first touches the node (`Approach::Arrived`) and is granted where it stands: no repositioning, as the game (§2.3) | `harvest.rs` `step_gather` |
| T3 | `ResourceHarvest { time_ticks, amount, return_delay_ticks }` per kind, read from `placement_rules.ron` | `crates/sim_core/src/catalog/placement_rules.rs` |
| T4 | Resource nodes live in `sim_map::ResourceNode` (`pos`, `kind`, `remaining`, `cell` hashed; `placed` unhashed). A field added there would touch about 15 struct literals in four crates, the client among them | `crates/sim_map/src/grid.rs`; `checksum/encoding.rs` `fields!(ResourceNode …)` |
| T5 | `current_tick` is advanced at the start of `SimWorld::tick`, before `update_unit_tasks`, which ends with `resolve_resource_claims` | `crates/sim_core/src/world.rs` |
| T6 | No shipped scenario, matrix battle, golden or bench row issues a `Harvest` order (occupancy T7, re-checked here by `grep` of `assets/scenarios`, `crates/sim_core/tests/fixtures`, `crates/sim_fixtures`, `crates/sim_core/benches`, **verify** in WP0). Lines form only in `harvest_occupancy_tests`, `harvest_edge_tests::saturation_follows_the_derivation`, client tests that send several workers to one node (`sync_steady_state_tests`), and the skirmish match's later workers | — |

**Finding F-1.** With a waiter present, the gap between one take and the next gather's start is 0 ticks on both
kinds (`the_next_worker_starts_on_the_tick_of_the_take`). So the saturated rates are exactly `amount × 1,500 /
time_ticks` (`saturation_follows_the_derivation`; occupancy evidence §2: 1,200.00 a line and 171.43 a geyser).

---

## 4. Design decisions

### D-1. The node is released `handoff_ticks` after the take

- At a take that yields cargo (`amount > 0`) on node `n` at tick `t`, the world records
  `release_tick(n) = t + handoff_ticks` of `n`'s kind.
- In D-1 of the occupancy plan, a node is occupied if a unit gathers there **or `current_tick < release_tick(n)`**.
  Nothing else changes: claims, the line's key, moving on (a node in its hand-off is not a free candidate) and the
  release at the take for a gatherer that never takes.
- So with a waiter present, the waiter is granted on tick `t + h` and its gather ends on `t + h + H`. The saturated
  period is `H + h`, and the gatherer's own trip is unchanged.
- **A worker arriving during the hand-off waits** until the release, like any claimant: the release is the node's,
  not the waiter's (§2.4 rule 1).
- **A worker arriving at a free node** (no take in the last `h` ticks) starts on that tick, as today (§2.4 rule 4).
  With one worker a node, its next arrival is a whole trip after its own take, far past `h`, so its trip is unchanged.
- **Occupancy is still shared by every player** (occupancy D-1, S10 there): the release tick is the node's, whoever
  took.
- `handoff_ticks: 0` gives `release_tick = t`, which is never above `current_tick` in the resolution of tick `t`:
  exactly today's rule. WP1 proves that.

**Rejected alternatives.**
- *Delay the waiter instead of the node* (grant on the take tick with `gather_ticks = H + h`). This needs no new
  state and gives the same saturated period. But a worker arriving one tick after a mineral take would start a tick
  early, and the "gathering" animation would cover the hand-off. The node's release is the mechanism of §2.4 rule 1.
- *Derive the release from the taker* (its `return_ticks` and `last_node`). This breaks for gas, where the taker has
  no countdown and leaves at once. It also breaks when the taker dies, or when an order cuts its wait into
  `cast_lock_ticks`, which clears `return_ticks`.
- *Model a waiting position and the step into contact.* §2.3 rules it out, and it would touch `movement/`.

### D-2. `handoff_ticks` is data on the resource row

```ron
(kind: Mineral, …, harvest_time_ticks: 50, harvest_amount: 5, return_delay_ticks: 9,
    // Hand-off to the next worker in line (plan 2026-11-23-harvest-handoff §2.2, §2.4, verify):
    // released on entering WaitToReturn, one engine step after the take (ReturnDelay present,
    // liberty BehaviorData:750), and the next worker starts one step later: 2 loops = 2 ticks (S-5).
    // Liquipedia Resources: three workers 143 / min = 47 loops a load against HarvestTime's 45.
    handoff_ticks: 2),
(kind: Gas, …, harvest_time_ticks: 35, harvest_amount: 4, return_delay_ticks: 0,
    // No ReturnDelay, so no WaitToReturn stage: released at the take; the next worker starts one
    // step later: 1 loop = 1 tick (S-5). Liquipedia: 163 / min = 33 loops against 32.
    handoff_ticks: 1),
```

- Read into `ResourceHarvest::handoff_ticks: u16` (doc: "ticks after a take before the node's line may start its
  next gather; the next worker in line starts on the tick the take's tick plus this"). The field is required: a
  catalog without it fails to load (`RonDeserializationError`). Any value is valid, 0 included (today's rule, the
  owner's option (b) of §15).
- **Why data and not derived in code from `return_delay_ticks > 0`.** The two-step reading is an inference
  (**verify**). Authored, it is one visible number per kind with its derivation beside it, it can be changed
  without recompiling (charter §3.3), and it can be set to 0 to switch the rule off. Code that derived it would
  hard-wire an unverified engine reading.
- **Charter §3.3, C5.** No kind or unit is named in code: the economy reads the row of the node's kind.

### D-3. Hashed state: `SimWorld::resource_lines`

- A new `economy::ResourceLines { release_tick: Vec<u32> }`, one entry per node of the map in node order, built
  with the world from the map's node count (the map keeps every node in its list, so identifiers hold). Reads past
  the end give 0, and a write past the end grows the list (it never does after construction; the growth path is
  for robustness and is unit-tested).
- Encoded after `world.map.resources()` under its own section tag (`"resource-lines"`): the length, then each
  `u32`. `CHECKSUM_SCHEMA_VERSION` takes the **next free number at merge** (38 on `e8cc3f80`), with its doc line:
  "Schema N adds each resource node's release tick, `ResourceLines::release_tick`, after the map's resource nodes
  (plan `2026-11-23-harvest-handoff` D-3)."
- It decides when a waiter starts, so it is hashed (the checksum's invariant).
- An absolute tick, not a countdown, so no per-tick pass over the nodes is needed. `current_tick` wraps after 2³²
  ticks (5.4 years of play). The comparison is `current_tick < release_tick`, and the wrap is documented on the
  field, not handled.
- **Why not on `sim_map::ResourceNode`** (T4): the release is economy state, not map geometry, and a field there
  would touch every struct literal of a node in four crates, the client among them.
- `checksum_state_tests::every_public_entity_field_changes_the_world_checksum` (or its world-level sibling,
  **verify** the right test while implementing) gains a mutation of one release tick.

### D-4. Acceptance is a derivation; the game's figures are a sanity band

See §6. The rule's effect follows from the data: a saturated node yields one load per `H + h`, and a worker cannot
trip faster than its legs allow. The measured income must equal that derivation within the measure's resolution.
The game's figures (S5) are a band stated in §6.3 before anything is measured. They check the result. They are not
a target, and no value is changed to move a result inside the band.

### D-5. Presentation

A waiter stands `Idle` for `h` more ticks before it gathers, and the node shows no gatherer during the hand-off.
No client code changes: the client reads `current_anim`.

### D-6. Records

- **B-162** fixed by WP2.
- **B-161** stays deferred, restated by WP0 (D-7).
- No new bug: the remaining gap to the game's figures (§6.3) is the whole-tick rounding of `HarvestTime` sanctioned
  as S-5, not a difference of mechanism. **B-178** stays free for the next plan.

### D-7. B-161 (`HideHarvesters`) stays out of this plan

- B-161 is about where the gas gatherer stands and whether it is seen, not about timing. Nothing in this plan's
  rule depends on it: the extractor's hand-off is 1 step whether the gatherer is hidden or not.
- It is probably **not presentation only**. In the game a hidden unit inside an extractor cannot be seen, selected
  or targeted (**verify** in the editor: `HideHarvesters` hides the unit, and a hidden unit is excluded by the
  `Hidden` filter that every weapon search sets, for example the `…Hidden…` exclusions of void EffectData:719 and
  :1028). That touches target acquisition, splash, vision, selection and the client's drawing. Those are combat and
  client subsystems that this economy plan does not own.
- So WP0 restates B-161: "The gas gatherer stands at the extractor's edge, seen and targetable. 5.0.14 hides it
  inside (`Flags HideHarvesters 1`, liberty BehaviorData:817–852), and a hidden unit is excluded from searches that
  filter `Hidden` (**verify**). Not presentation only: it needs a hidden state in the simulation." It stays open,
  for its own plan.

---

## 5. Determinism and checksum

- **Integer arithmetic only.** `handoff_ticks` is a whole number read once. The release tick is a `u32` addition
  (saturating) and a comparison. No float is added to the simulation crates, so C7's counts do not change.
- **Order.** The release tick is written in the gatherer's step and read only in `resolve_resource_claims`, after
  every step of the tick (occupancy D-2). So no step reads another's write, and iteration order decides nothing.
- **Hashed state:** D-3, schema next free at merge.
- **Goldens.** Neither fixture harvests (T6), so every release tick is 0 at every checkpoint. Only the schema number
  and the encoding change. WP1's proof, the procedure of plans `2026-11-22-harvest-timing` §5 and
  `2026-11-23-splash-cliff-levels` §5:
  1. With `resource_lines` left out of the encoding and the schema at its current number, WP1's tree reproduces
     every recorded checksum of `golden_replay.ron`, `golden_replay_orders.ron` and `sim_fixtures`'
     `DEFAULT_SANDBOX_CHECKSUM`. It does so on native debug and release and on `wasm32-wasip1` release and debug
     (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`).
  2. Then the field goes in, the schema becomes the next free number, and the checksums are re-recorded under the
     header "Re-recorded for plan 2026-11-23-harvest-handoff WP1 (checksum schema M -> N): the world also encodes each
     resource node's release tick, so every checkpoint changed. No behaviour did: with the field left out of the
     encoding at schema M, both fixtures reproduce as recorded for <the previous header>." The four builds reproduce
     the new values. The schema-byte tests of `checksum::encoding` move with it, as before.
  3. If `main` takes the number first, the merge repeats step 2 with the next one.
- **WP2 leaves the goldens and the sandbox unchanged** (nothing in them harvests). A move is traced to its first
  checkpoint and entity, and an unexplained one stops the package.
- **`sim_map` pins:** unchanged (no `sim_map` change).
- **wasm.** No new dependency; the simulation crates still build for `wasm32-unknown-unknown`.
- **Cost.** One vector read per node in `tally_nodes` (which runs only on a tick with claims) and one write per
  take. The vector is sized once, so nothing is allocated in steady state. The throughput bench does not harvest.

---

## 6. Expected income (derived before measuring)

### 6.1 The curve

The occupancy plan's closed cycle (§6 there) with the server's period `H` replaced by `H + h`. Each field `f` has its
one-worker trip `T_f` (legs + `H` + `D`), unchanged by this plan. With `k` workers on it:

`R_f(k) = min(k × amount × 1,500 / T_f, amount × 1,500 / (H + h))`

So a mineral field yields `min(k × 7,500 / T_f, 144.23)` and a geyser `min(k × 6,000 / T_f, 166.67)`. A field is
saturated when `T_f < k × (H + h)`: `k × 52` for minerals, `k × 36` for gas. Where `T_f ≥ k (H + h)`, the workers can
space themselves so that none waits. A waiter's start is only ever pushed later, so the spacing grows until no one
waits, and the result is today's (**verify** by A-2 on every unsaturated case).

| Line (one-worker `T_f`, occupancy plan §6.1) | `k` = 1 | 2 | 3 | 4 |
|---|---:|---:|---:|---:|
| Showcase slot 0, 8 fields, `T` 121 to 147 | 446.4 (unchanged) | 892.9 (unchanged: `2 × 52 = 104 ≤ 121`) | **1,153.8** (was 1,200.0; every `T` < 156) | **1,153.8** |
| Showcase slot 1 (`T` as built: 893.22 at `k` = 2, occupancy As built) | unchanged | 893.2 (unchanged) | **1,153.8** | **1,153.8** |
| `default_arena` slot 1, `T` 129, 121, 123, 143, 139, 135, 153, 176 | 434.7 (unchanged) | 869.4 (unchanged) | **1,137.5** (was 1,174.9: the 153-tick field now saturates, `153 < 156`, at 144.2; the 176-tick field stays at 127.8) | **1,153.8** (was 1,200.0) |

| Geyser (one-worker `T`) | `k` = 1 | 2 | 3 | 4 |
|---|---:|---:|---:|---:|
| Showcase near (10, 1), arena slot 1 (50, 56), `T` 99 | 60.61 | 121.21 | **166.67** (was 171.43) | **166.67** |
| `default_arena` slot 0 (13, 12), `T` 127 | 47.24 | 94.49 | 141.73 (unchanged: `108 ≤ 127`) | **166.67** |
| Showcase corner (1, 1), `T` 135 | 44.44 | 88.89 | 133.33 (unchanged) | **166.67** |

### 6.2 What does not move

- One worker per node: every trip and income identical to WP1's tree, to the tick (D-1).
- Every unsaturated line (all `k` = 1 and 2 rows above, the unchanged `k` = 3 gas rows): identical rates within
  A-1's tolerance, and no moves in the window.
- Spreading (occupancy D-3): the "two fewer" rule is unchanged. A node in its hand-off is occupied, so it is not a
  free candidate, exactly as a node being gathered.

### 6.3 Acceptance (fixed before measuring)

- **A-1, the curve.** Every line and `k` of §6.1, and every geyser and `k`, equals the derived rate within one load
  per node over the window: ±2.5 minerals a minute per field (±20 for a line of eight) and ±2.0 gas a minute per
  geyser, for a 3,000-tick window after 1,500 ticks of warm-up (minerals) or 750 (gas), measured by take period as
  in the occupancy plan. A saturated node is exact: one take every `H + h` ticks.
- **A-2, stability.** A balanced line (`k` = 1 to 4 on every field) makes no move in the window.
- **A-3, the hand-off.** On every hand-off in a window, the waiter's `gather_ticks` becomes `H` on exactly the tick
  `t + h` of the previous take `t` (P-1, §12).
- **A-4, one worker is unchanged.** `k` = 1 trips identical to WP1's tree on every field and geyser of §6.1.
- **A-5, the game's band (a sanity check, stated now).** The saturated rates must lie within ±3 % of the game's
  measured figures (S5): a field in **[138.7, 147.3]** (143 ± 3 %), a geyser in **[158.1, 167.9]** (163 ± 3 %), the
  showcase line with three a field in **[1,105.8, 1,174.2]** (1,140 ± 3 %). The derivation gives 144.2, 166.7 and
  1,153.8, inside the band. The remaining +0.9 %, +2.2 % and +1.2 % are the whole-tick `HarvestTime` of S-5: the
  engine ends 2.786 Normal seconds on loop 45 (50.22 ticks) and 1.981 on loop 32 (35.71 ticks), where we round to 50
  and 35. With the engine's loops the same rule gives 142.98 and 162.91 (§2.2). If the result falls outside the band,
  that is a finding to trace, not a reason to change `handoff_ticks`.

The derivation and A-1 to A-5 were written before any measurement. No prototype was run for this plan: WP1 is
latent by construction, and WP2's figures follow from the occupancy plan's measured `T_f` and the data.

---

## 7. Worlds that move

### 7.1 Commands

The splash-cliff scripts (`~/scratch/splash-cliff/scripts/measure.sh`, `compare.sh`), copied to
`~/scratch/harvest-handoff/scripts/`. The binaries are built once per tree and run from that tree's `assets/`:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/harvest-handoff
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64, and the
showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks. Run them in the
background, one at a time across worktrees (charter §7). Income is measured with the occupancy plan's scratch tool
(`~/scratch/harvest-occupancy/proto/harvest_occ.rs`, `income.sh`, `minerals.sh`), copied into
`crates/sim_core/examples/` of the scratch tree and never committed. It runs in `line` and `node` modes on the cases
of §6.1. The client skirmish (`skirmish_match_tests::a_match_is_won_through_real_input_alone`) is compared by its
stage log, on radv under the test's own GPU lock.

### 7.2 Expected moves

| World | WP1 (data 0, state hashed) | WP2 (data 2 / 1) |
|---|---|---|
| Matrix (seeds of record and 16), battles 31 (100), 33 (64) | byte-identical | byte-identical (nothing harvests, T6) |
| Showcase and showcase effects, default seed and 1 to 8 | byte-identical | byte-identical (nothing harvests) |
| Goldens, sandbox | schema re-record only, proven (§5) | unchanged |
| Income, `k` = 1 and unsaturated `k` | identical | identical (A-4, §6.2) |
| Income, saturated lines and geysers | identical | **down**, to §6.1's bold figures (field 150 → 144.2, geyser 171.4 → 166.7, showcase line 1,200 → 1,153.8, arena slot 1 `k` = 3 1,174.9 → 1,137.5) |
| `harvest_occupancy_tests`, `harvest_edge_tests::saturation_follows_the_derivation` | pass unchanged | the three tests of §12 that read the hand-off move by `h` exactly; nothing else |
| Client skirmish stage log | identical | **identical expected**: the pilot sends each starting worker to its own field, and on the occupancy plan's tree no stage moved when later workers shared fields (occupancy §7.5). Its later workers may now wait `h` more ticks a load where they queue, so a later stage may move by a few ticks at most, and only later (less income, never more) |
| Client `sync_steady_state_tests`, `battle_card_path_tests` gas test | pass | pass (one worker per node, or a warm-up read from the simulation, occupancy Amendment 1) |

**Direction, recorded before measuring.** The rule only delays a waiter's start. So no income rises anywhere, no
worker's take comes earlier than on WP1's tree, and a world with no waiter is unchanged.

### 7.3 Stop rules

1. **WP1 is latent.** With `handoff_ticks: 0` on both rows every world of §7.2 is byte-identical, and every harvest
   test passes unchanged. The goldens move by the schema only, proven by §5. Any other difference is a defect and
   stops the package.
2. **WP2 moves only worlds with a waiter.** A difference in the matrix, battles 31 and 33, either showcase or a
   golden is a defect. An income figure outside A-1, or a move in a balanced line (A-2), is traced (take ticks,
   claims, release ticks) before anything changes, and an unexplained miss stops the package.
3. **A-5 outside its band** stops WP2 for a trace. The fix is never a change of `handoff_ticks` to fit: it is a
   finding for the orchestrator (an amendment, or the owner's question).
4. **The skirmish match** must still end in `Victory` within `TICK_LIMIT` (unchanged). A stage that moves earlier is a
   defect (§7.2's direction). A later stage is reported with its trace and is not a stop. A miss of the limit stops
   the package, and the limit is not raised without an amendment.
5. **A failing test not listed in §12** is a finding and stops the package. No bound is loosened, and no constant is
   chosen to make a test pass.

---

## 8. Client consequences

- **Code: none** (D-5). The waiter's extra `Idle` ticks are read from `current_anim`.
- **Tests:** the client tests that harvest (`battle_card_path_tests::gas_through_the_card_extractor_snap_harvest_and_income`,
  `battle_context_order_tests`, `battle_fog_tests`, `battle_hud_tests`, `battle_pipeline` unit tests,
  `skirmish_match_tests`, `sync_steady_state_tests`, `battle_flipbook_tests`) run in WP2's package gate. A test that
  waits for income with a tick bound is re-derived from §6 if it fails, never loosened.
- **`HideHarvesters`** (B-161) stays a client and simulation matter for its own plan (D-7).

---

## 9. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP3 | B-162 points here; B-161 restated (D-7); B-162 fixed; the Economy row |
| `docs/plans/README.md` | plan, WP3 | Row `Proposed` with the plan; `Done` in WP3 |
| `assets/catalog/placement_rules.ron` | WP1 (0, 0), WP2 (2, 1) | `handoff_ticks` per row with D-2's comment |
| `crates/sim_core/src/catalog/placement_rules.rs` | WP1 | `ResourceHarvest::handoff_ticks`, its parse and unit tests |
| `crates/sim_core/src/economy/lines.rs` (new) | WP1 | `ResourceLines` with its `//!` doc and unit tests |
| `crates/sim_core/src/economy/harvest.rs`, `economy/mod.rs` | WP1 | Release tick written at the take; read in `tally_nodes`; module doc (the line) |
| `crates/sim_core/src/world.rs` | WP1 | `SimWorld::resource_lines`, built from the map |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | WP1 | Schema next free; the section; schema-byte tests |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures` (`DEFAULT_SANDBOX_CHECKSUM`) | WP1 | Re-recorded for the schema, proven (§5) |
| `crates/sim_core/tests/harvest_handoff_tests.rs` (new), `harvest_occupancy_tests.rs`, `harvest_edge_tests.rs`, `checksum_state_tests.rs`, `runtime_catalog_tests.rs`, `catalog_reference_values_tests.rs` | WP1, WP2 | §12 |
| `docs/evidence/2026-11-23-harvest-handoff.md` | WP0 to WP3 | Baseline, WP1's latency proof, WP2's figures |

Nothing under `crates/sim_core/src/movement/` or `crates/sim_map/` changes, and no client source changes.

---

## 10. Owner question and defaults

§15 drafts Q0100. Until it is answered, this plan follows option (a): the hand-off of D-1 with D-2's values. Every
package is easy to undo: (b) is `handoff_ticks: 0` on both rows, a data change, after which the state stays
latent. If the owner sanctions the difference instead, it goes into Q0052's list and B-162 is closed as sanctioned.

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR`; the implementation agent runs
`cargo gate quick` and the tests its change needs and hands back; a verifier runs `cargo gate package` (Q0061).

### WP0: records and baseline (XS, docs only)

- `DESIGN.md` §4: B-162 points to this plan; B-161 restated (D-7), still open and deferred.
- `docs/evidence/2026-11-23-harvest-handoff.md`: the baseline on WP0's tree. §7.1's worlds into `world-wp0`; the
  income cases of §6.1 with the scratch tool (each node's `T_f` at `k` = 1, the lines at `k` = 1 to 4, the geysers at
  `k` = 1 to 4), which must reproduce the occupancy evidence. If they differ, the derivation of §6.1 is redone from
  the measured `T_f` before WP2, and the plan is amended. The T6 `grep` re-checked.
- **Gate:** docs-only `package`.

### WP1: the release tick, latent (M, `sim_core`, catalog; schema next free)

Failing tests first (red on WP0's tree):
- `catalog/placement_rules.rs` `mod tests`: a row without `handoff_ticks` is refused (`RonDeserializationError`);
  `handoff_ticks: 0` is accepted; `harvest(kind).handoff_ticks` returns the row's value.
- `economy/lines.rs` `mod tests`: a new list of `n` nodes reads 0 everywhere; a write and read back; a read past the
  end gives 0; a write past the end grows the list.
- `checksum_state_tests`: a mutated release tick changes the world checksum.
- `harvest_handoff_tests::a_zero_handoff_is_todays_rule` (a scratch catalog with 0): the waiter starts on the tick
  of the take, as `the_next_worker_starts_on_the_tick_of_the_take` (which stays as it is in WP1).

Then the change: D-1 to D-3 with `handoff_ticks: 0` on both rows. Then §5's proof and re-record.

**Acceptance:** §7.3 rule 1. The four builds of §5. World comparison `world-wp0` against `world-wp1`.

### WP2: the hand-off (S, data, B-162)

Failing tests first (red on WP1's tree), every time and amount read from the catalog, never written as literals:
- **`harvest_handoff_tests::the_next_worker_starts_handoff_ticks_after_the_take`**, for each kind (a lone field, a
  lone extractor, three workers): on every hand-off in 1,500 ticks, the waiter's `gather_ticks` is 0 on ticks
  `t … t + h − 1` and becomes `H` on `t + h` (A-3). Replaces occupancy's
  `the_next_worker_starts_on_the_tick_of_the_take`, which moves here renamed. Its WP1 form becomes the zero-hand-off
  test above.
- **`a_worker_arriving_during_the_handoff_waits_for_it`:** a second worker placed to touch a mineral field on the tick
  after the first one's take is granted on `t + h`, not before.
- **`a_worker_arriving_at_a_free_node_starts_at_once`:** one worker alone: its gather starts on the tick it touches
  the node, and its trip equals WP1's (A-4).
- **`the_handoff_is_shared_by_every_player`:** an enemy worker waiting at a field gathered by player 1 starts `h`
  after player 1's take.
- **`a_saturated_node_takes_every_time_plus_handoff_ticks`:** three workers on a near geyser and on a showcase field: the
  take period is exactly `time_ticks + handoff_ticks`.
- **`harvest_edge_tests::saturation_follows_the_derivation`** re-derived with `H + h` (§6.1), its doc citing this
  plan; the near geyser at 3 equals `amount × 1,500 / (time + handoff)` exactly.
- **`harvest_occupancy_tests::one_worker_at_a_time_in_an_extractor_and_gas_workers_wait`:** its saturated take period
  becomes `time_ticks + handoff_ticks`, read from the row.
- **`a_balanced_line_stays_put`** and **`workers_sent_to_one_field_spread_over_the_line`** stay unchanged and must
  pass (A-2, §6.2).
- **`runtime_catalog_tests`:** a copy of the shipped catalog with `handoff_ticks: 0` on both rows loads in the same
  binary, and three workers on the near geyser then earn `amount × 1,500 / time` (charter §3.3, zero recompilation).
- **`catalog_reference_values_tests::the_shipped_handoffs_follow_the_stage_reading`:** mineral 2, gas 1, with
  §2.2's derivation in the doc.

Then the change: D-2's values and comments.

**Acceptance:** §7.3 rules 2 to 5. Measure §6.3 (A-1 to A-5) on WP2's tree with `T_f` from WP1's, compare §7.1's
worlds `world-wp1` against `world-wp2`, and run the skirmish match on WP1 and WP2 with both stage logs in the
evidence.

### WP3: bookkeeping (XS)

- `DESIGN.md`: B-162 fixed (commit, §6.3's figures as built). The Economy row says the next worker in line starts
  `handoff_ticks` after the take (2 a field, 1 a geyser), with this plan's link. The evidence column adds
  `harvest_handoff_tests.rs`.
- Evidence completed; this plan's `## As built`; README row `Done`.
- **Gate:** docs-only `package`.

---

## 12. Test plan (summary)

Per `docs/guides/test-gates.md`: unit tests beside the code, integration tests in `crates/sim_core/tests/`. The new
file must be selected by `crates/gate/suites.ron`'s `sim_core` rules (**verify**; if a rule names files, WP1 adds
it).

| Layer | What it proves | Where |
|---|---|---|
| Data | `handoff_ticks` read at run time, required, sourced; zero recompilation | WP1: `placement_rules` unit tests; WP2: `runtime_catalog_tests`, `catalog_reference_values_tests` |
| State | The release list's reads, writes and bounds; hashed | WP1: `economy/lines.rs` unit tests, `checksum_state_tests` |
| Rule (P-1) | The waiter starts exactly `h` after the take, for each kind; 0 is today's rule | WP1, WP2: `harvest_handoff_tests` |
| Rule (P-2) | A worker arriving during the hand-off waits; one arriving at a free node starts at once | WP2: `harvest_handoff_tests` |
| Rule (P-3) | Shared by every player; order-independent (occupancy's `the_grant_does_not_depend_on_entity_order` still passes) | WP2 |
| Income (P-4) | The saturated period `H + h`; the curve of §6.1 | WP2: `harvest_handoff_tests`, `harvest_edge_tests::saturation_follows_the_derivation` |
| Spreading | Balanced lines stay; splits unchanged | WP2: existing `harvest_occupancy_tests` |
| Determinism | WP1 latent; goldens re-recorded for the schema with proof on four builds; non-harvest worlds byte-identical | §5, §7 |
| Client | The skirmish match still won within its limit | WP2's package gate |

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The two-step reading (§2.4 rule 1) is wrong: the game's gap has another cause that gives the same whole loops | The right figures for the wrong reason; a later rule (for example a lone worker's approach step) might double count | Marked **verify**; the value is data and the question (§15) asks for an editor check; §14 lists what the reading does not cover |
| R-2 | Another branch takes schema 38 (or 39) first | A conflicting number and re-record | §5 step 3: the merge takes the next number and repeats the proof |
| R-3 | A line near the saturation boundary (`default_arena`'s 153-tick field at `k` = 3, 3 ticks under `3 × 52`) settles slowly | A-1 misses on that field | §6.1's convergence note; a miss is traced (§7.3 rule 2), the warm-up is not lengthened to pass it without an amendment |
| R-4 | A client test bounds a wait for income in ticks with several workers on one node | A red test in WP2's gate | §8: re-derived from §6, never loosened |
| R-5 | `current_tick` wraps | A release tick in the past reads as future | 2³² ticks is 5.4 years; documented on the field |
| R-6 | The skirmish pilot's later workers queue and a late stage moves | Reported, not a stop (§7.3 rule 4) | Direction is fixed: later or equal only |

---

## 14. Out of scope and decisions recorded (Q0002)

- **B-161, `HideHarvesters`** (D-7): stays deferred, restated as not presentation only.
- **A step for a lone worker's start** (§2.4 rule 4): the stage reading would also put a step between
  `ApproachResource` and the gather, which would lengthen every trip by a tick. No figure measures it (S5's
  one-worker band is 55 to 60), so it is not added. A sourced measurement would bring it back as an amendment.
- **A release step after an interrupted gather** (a gatherer given an order, killed or loaded before its take): the
  node is freed at once, as today. The engine's step there is not measured.
- **Ending `HarvestTime` on whole game loops** (§6.3's residual): S-5 rounds to whole ticks, sanctioned. Modelling
  the engine's loop ceiling would be a change of the tick model, not of harvesting.
- **The waiter's position and facing** (§2.3): unchanged, in contact where it first touched; no `movement/` change.
- **The MULE's second queue** (`ResourceQueueIndex 1`): no catalog unit has it (occupancy §14).

---

## 15. Draft owner question (for the orchestrator to file as Q0100; not filed by this plan)

```markdown
# Q0100: Adopt the inferred hand-off between two gatherers (B-162)?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** (a): the next worker in a resource's line starts 2 ticks after the previous take on a
  mineral field and 1 tick on a geyser (plan `2026-11-23-harvest-handoff` D-1, D-2).

## Question

In StarCraft II, three workers on one mineral field earn about 143 minerals a minute, not the 150.75 the data's
harvest time allows, and three on a geyser about 163 gas, not 169.6 (Liquipedia, Resources). Space Scuffle earns
150 and 171.4, because the next worker starts on the very tick the previous one finishes (bug B-162).

The game data contain no delay for this. Read in the engine's game loops, the game's figures are whole numbers:
47 loops a load on a field against a harvest time that ends on loop 45, and 33 on a geyser against 32. A worker
cannot move into position in 1 or 2 loops at its acceleration, so the waiting worker does not move. The plan reads the
gap as the harvest ability's stages: the field is released when the gatherer starts its half-second pause, one step
after it finishes, and the next worker starts one step later. A geyser has no pause, so it takes one step. That
reading fits all three published figures (gold fields too). It is an inference, not something the data states.

Options:
- (a) **Adopt it** (the default): 2 ticks on a field, 1 on a geyser, authored in the catalog. A saturated field then
  earns 144.2 a minute and a geyser 166.7.
- (b) **Keep today's behaviour** and sanction the difference as a minor deviation: `handoff_ticks: 0`, a data
  change; B-162 is closed as sanctioned.
- (c) Something else, for example after checking in the editor: with three SCVs on one mineral field, count the game
  loops between one SCV's mineral appearing in its hands and the next SCV's mining animation starting (the plan
  predicts 2), and the same at a refinery (predicts 1).

## Answer

```

The question number is the next free at filing (0100 on `e8cc3f80`).

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,swarm,void,voidmulti,balancemulti}/{AbilData,BehaviorData,UnitData}.xml`
  (`5444e12f13`), read with `grep` and `sed`.
- Liquipedia wikitext saved by the occupancy plan in `~/scratch/harvest-occupancy/web/` (`Resources.json`,
  `Mining_Minerals.json`); the web search of S8 on 2026-11-23.
- The arithmetic of §2.2, §2.3 and §6 was done by hand and checked with a one-line Python calculation; no prototype
  (§6.3).
- The occupancy plan's measuring tool and figures: `~/scratch/harvest-occupancy/proto/`,
  `docs/evidence/2026-11-23-harvest-occupancy.md`.

---

## As built (implementation agent, 2026-11-23)

One branch (`harvest-handoff-plan`), one commit per package: WP0 `f93705a6`, WP1 `efaef9d7`, WP2 `6288d3d1`, the
merge of `main` `e028fc0b` (schema 39), WP3 the commit that adds this section. Evidence: [`docs/evidence/2026-11-23-harvest-handoff.md`](../evidence/2026-11-23-harvest-handoff.md).
No decision changed, so there is no amendment.

### WP1

1. **Schema 39.** WP1 took 38, the next free number on `origin/main` (`e8cc3f80`, schema 37) when it was built, with
   §5's proof on four builds (evidence §2.2). `main` then took 38 for plan `2026-11-23-strike-delays` (`d71f7153`),
   so the merge of `main` repeated §5 step 2 with 39 (evidence §4). `CHECKSUM_SCHEMA_VERSION`'s doc line is D-3's. The release ticks are encoded right
   after `world.map.resources()`, under the tag `"resource-lines"`, before the watchtowers.
2. **`ResourceLines`** is public (`sim_core::economy::ResourceLines`, re-exported from `economy`), and
   `SimWorld::resource_lines` is a public field like the world's other hashed state, so the integration test
   `checksum_state_tests` can mutate a release tick. Besides D-3's reads and writes it has `in_handoff(node, tick)`,
   the comparison `tally_nodes` uses, unit-tested with the rest.
3. **`tally_nodes`** marks a node occupied from its hand-off before it counts the units, one read per node on a tick
   with claims, as §5 states.
4. **The required field outside §9.** `handoff_ticks` is required, so the inline rows of two tests outside §9's list
   gain `handoff_ticks: 0`: `catalog/validation.rs` (`A_HARVEST`, its tests) and `footprint_flag_tests.rs`. No
   assertion changed.
5. **`harvest_handoff_tests`** builds a catalog with another hand-off by replacing every `handoff_ticks:` value of the
   shipped `placement_rules.ron` text (`sim_fixtures::PLACEMENT_RULES_RON`) and parsing it with
   `PlacementRules::from_ron_str`; the rest of the catalog is the shipped one. Its flat map puts the field at (12, 4)
   and the geyser at (11, 5), so that three workers keep a line at both (a geyser at (13, 5) gave only two hand-offs in
   1,500 ticks at `h` = 0: its trip was too close to three gathers).
6. The new test file is selected by `crates/gate/suites.ron`'s crate rules; no rule names files, so none was added.

### WP2

1. **The tests that read the hand-off** assert first that the shipped row hands off after the take (`h > 0`, and
   `h ≥ 2` for the field in `a_worker_arriving_during_the_handoff_waits_for_it`), so on WP1's tree they fail instead
   of passing trivially with `h` = 0 (evidence §3.1). Every time, amount and hand-off is read from the catalog.
2. **`a_saturated_node_takes_every_time_plus_handoff_ticks`** uses the showcase's near geyser and, for minerals, the
   flat map's lone field rather than a showcase field: three workers sent to one showcase field spread over the line
   (occupancy D-3), and the line at three a field is `harvest_edge_tests::saturation_follows_the_derivation`.
3. **`the_next_worker_starts_on_the_tick_of_the_take`** left `harvest_occupancy_tests`; its rule is
   `harvest_handoff_tests::a_zero_handoff_is_todays_rule`, and its hand-off form is
   `the_next_worker_starts_handoff_ticks_after_the_take`. The occupancy file's module doc and the wording of
   `the_grant_does_not_depend_on_entity_order` (the waiter starts after the take, not on it) changed with it; the test's
   assertion did not.
4. **The mineral row's comment** says "(ReturnDelay present: liberty BehaviorData:750);" where D-2 has "(ReturnDelay
   present, liberty BehaviorData:750), and". `runtime_catalog_tests::an_edited_placement_rule_changes_the_loaded_reach`
   cuts the mineral row at its first `),`, which the comment's punctuation would have been. It failed on that alone
   (a RON parse error of the cut text, not a behaviour) and passes with the comment's punctuation changed; the test is
   unchanged.
5. **Income** met §6.1 to the measure's resolution, every A inside its bound and A-5 inside its band (evidence §3.2).
   The skirmish match: every stage on its WP0 tick, Victory at 6,486 (evidence §3.4).
