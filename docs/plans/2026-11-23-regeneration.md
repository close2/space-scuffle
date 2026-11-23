# Blueprint: Regeneration Before Completion, Energy Without a Delay, and Structure Energy (B-35, B-36, B-168)

This plan settles the open regeneration rows of the bug register against StarCraft II 5.0.14:

- **Unfinished structures no longer regenerate (B-36).** A structure under construction gains life only from the
  work done on it. Life, shields and energy regenerate from the tick after it completes, under the same delays as
  any other entity.
- **Energy has no regeneration delay, as in the game (B-35).** No layer of the 5.0.14 data sets `EnergyRegenDelay`
  on any unit. Our rule (energy rises on every tick) is the game's. The row is closed as not a deviation, and tests
  pin the rule.
- **Structures with energy regenerate it (B-168, registered here).** The Orbital Command, Nexus and Shield Battery
  regenerate 0.5625 energy per Normal second in the data. Our structures cannot regenerate energy at all, because
  the structure loader hard-codes a rate of 0. The rate becomes a catalog field, authored on those three rows.

The neighbouring rules were checked and are recorded in §2.4: the life-regeneration delay (already correct), the
shield delay and rate (already correct), and the burrowed Roach's regeneration (B-38, deferred to the burrow work).

A full plan is required (charter §6): a new engine rule (regeneration waits for completion), a new catalog field,
and work across the catalog, `sim_core` and the records in three packages. No hashed state is added (§5).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`2bb84a5b` (checksum schema 34).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the specification is silent, search for what StarCraft II does | Standing rule; searched (§2.1), B-36's rule not found | D-1 |
| Q0002: scoping is the agent's call if written down | Standing rule | §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6 |
| **Draft, not filed:** "Do Zerg structures regenerate life while under construction?" | Proposed for the orchestrator to file (§15); default (no) followed meanwhile | D-1 |

---

## 1. Summary

| | Today (`2bb84a5b`) | After this plan |
|---|---|---|
| Life of a foundation | Rises by the work done **and** by `hp_regen_per_tick` on every tick, worked or not (the 17 Zerg structure rows, 0.01531 per tick) | Rises by the work done only; regeneration starts on the tick after completion |
| Shields of a foundation | 0 until completion, then full; regeneration already waits for `is_completed` | Unchanged |
| Energy of a foundation | Holds its `start_energy` (no structure regenerates energy) | Holds its `start_energy` until completion, then regenerates |
| Energy regeneration delay | None: energy rises on every tick, a cast or damage notwithstanding | Unchanged, now sourced (no `EnergyRegenDelay` in any 5.0.14 layer) and pinned by tests |
| Structure energy rate | Hard-coded `SimScalar::ZERO` in `StructureArchetypeDef`'s conversion | `energy_regen_per_tick` read from `structures.ron`; 0.0315 on the three rows with energy |
| Hashed state | — | None added; schema unchanged (34, or whatever `main` has at merge) |
| Worlds that move | — | None measured: no measured world builds a regenerating structure or reads structure energy (§7) |
| Size | — | S–M: four packages, one of them M |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [WEB] read on a web source; [INF] inferred from the data; **verify** not
confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), layers `core`, `liberty`, `libertymulti`, `swarm`, `swarmmulti`, `void`, `voidmulti`, `balancemulti`, files `UnitData.xml`, `BehaviorData.xml`, `EffectData.xml` (the copy of the catalog-time audit, `~/scratch/combat_wp3_data/`). Chain `core → liberty → swarm → void → voidmulti` (audit §2.2). Citation `liberty UnitData:N` is line N of that layer's file | Every value below [DATA] |
| S2 | `grep -n EnergyRegenDelay` over all eight layers' `UnitData`, `BehaviorData`, `EffectData`, `AbilData` | **No match.** No unit, default element (`core UnitData:23`, `<CUnit default="1">`) or behaviour sets an energy-regeneration delay [DATA] |
| S3 | `grep -n LifeRegenDelay` over the same | Two matches, both the Reaper: `void UnitData:1475`, `swarmmulti UnitData:893` (value 10) [DATA]; already applied by plan [`2026-11-05-life-regeneration-delay`](2026-11-05-life-regeneration-delay.md) |
| S4 | `ShieldRegenDelay` / `ShieldRegenRate` | Every shielded unit and structure: 10 and 2 (e.g. `liberty UnitData:404–405`, Nexus `:4178`, `:4238`, Shield Battery `:1596–1597`); no `void` or `voidmulti` override; `swarmmulti UnitData:517–518` commented out [DATA] |
| S5 | Structure energy: Orbital Command `liberty UnitData:10513–10515` (`EnergyStart 50`, `EnergyMax 200`, `EnergyRegenRate 0.5625`); Nexus `liberty UnitData:4239–4240` (`EnergyMax 100`, `EnergyRegenRate 0.5625`), `void UnitData:1459` (`EnergyMax 0`), `voidmulti UnitData:1094–1096` (`EnergyMax 200`, `EnergyStart 50`); Shield Battery `liberty UnitData:1591–1593` (`EnergyStart 100`, `EnergyMax 100`, `EnergyRegenRate 0.5625`), `voidmulti UnitData:1563` (`EnergyStart 50`) | All three regenerate 0.5625 energy per Normal second [DATA] |
| S6 | `liberty BehaviorData:458–461`, `UnderConstruction` (`CBehaviorBuff`, `InfoFlags Hidden`, `RemoveValidatorArray IsConstructing`; `core ValidatorData:266–267`, filter `UnderConstruction`) | The buff a unit under construction carries. It sets **no** `SuppressLifeRegen`, `SuppressShieldsRegen` or `SuppressEnergyRegen` flag. Those flags exist (e.g. `liberty BehaviorData:1218–1220`, Vortex; `void BehaviorData:622–624`, `:759–761`, `:785–787`, the Stasis Ward and Oracle states), so the data would say so if a behaviour did it. Whether the engine suppresses regeneration under construction by itself is not in the data [DATA; the engine rule **verify**] |
| S7 | RoachBurrowed `liberty UnitData:7250–7262` (`LifeRegenRate 5`, no `LifeRegenDelay`); later layers change only its `Food` and card (`libertymulti :461`, `swarm :1859`, `voidmulti :1468`, `balancemulti :1379`) | The burrowed Roach regenerates 5 life per Normal second, 7 per Faster second, without a delay [DATA] |
| S8 | Liquipedia, Zerg Regeneration (<https://liquipedia.net/starcraft2/Zerg_Regeneration>), read through a web search's excerpt on 2026-11-23; the page itself and `Health` were refused by Liquipedia's rate limit that day | "regeneration starts the moment damage is taken", 0.38 life per game second for Zerg units and buildings, and Zerg structures off creep decay [WEB]. Nothing about structures under construction |
| S9 | Two further searches on 2026-11-23 (Zerg buildings under construction and regeneration; the SC2 editor's construction and regeneration fields) | Nothing on regeneration under construction [WEB: not found] |
| S10 | Energy behaviours: `GhostCloak` and `BansheeCloak` (`liberty BehaviorData:687–706`, `VitalRegenArray Energy −1.4765`), `DisableEnergyRegen` (`liberty BehaviorData:468–473`, applied by Graviton Beam, `liberty EffectData:1258–1290`), `OracleWeapon` (`void BehaviorData:459–466`, −2), `BatteryEnergy` / `BatteryAddEnergy` (`voidmulti BehaviorData:31–36`, `EffectData:40–46`, +50 near a Nexus) | Energy changes that belong to abilities not modelled here (§14) [DATA] |

### 2.2 The rules that follow

1. **Energy regenerates on every tick, with no delay** (S2). An unset `EnergyRegenDelay` takes the engine default.
   The default is 0 by the same reading the life-regeneration plan made for `LifeRegenDelay` (its §2.3), and S8
   confirms that reading for life: Zerg units leave `LifeRegenDelay` unset and "regeneration starts the moment
   damage is taken". So **B-35 is not a deviation**: we regenerate energy on every tick, as the game does. The
   default for energy is [INF] by the same argument (**verify** the engine default, which no data file states).
2. **Structures with energy regenerate it at 0.5625 per Normal second** (S5): 0.5625 × 1.4 ÷ 25 = **0.0315 per
   tick**, the units' shipped rate (audit conversion, life-regeneration plan D-5).
3. **Shields regenerate 2 per Normal second after 10 Normal seconds without damage** (S4): 0.112 per tick after
   179 ticks. All 14 shielded unit rows and 15 shielded structure rows of the shipped catalog carry exactly these
   values (counted on `2bb84a5b`). Conforming; nothing to do.
4. **Life regenerates without a delay for every unit but the Reaper** (S3, S8). Shipped since the life-regeneration
   plan. Conforming; nothing to do.
5. **Under construction** (S6, S9). The data has no flag that stops regeneration under construction, and no source
   found says whether the engine stops it. This plan follows the bug register's reading: **no vital regenerates
   until the structure is complete** (D-1). The reading is **verify**. It agrees with how construction is shown in
   the game, where a building's life bar follows its progress (S8 describes regeneration of finished units and
   buildings only), and with the rule our shields have followed since combat alignment WP9.
6. **The burrowed Roach regenerates 7 life per Faster second** (S7): 5 × 1.4 ÷ 25 = **0.28 per tick**, no delay.
   It needs a burrowed Roach, which does not exist here (§2.4).

### 2.3 Values

| Value | Game data | Our units | Shipped | Source |
|---|---|---|---|---|
| Energy regeneration, units | 0.5625 per Normal s | 0.0315 per tick | 0.0315 on the 10 caster rows | `liberty UnitData:1593` and others |
| Energy regeneration delay | not set in any layer | 0 (none) | none (no field) | S2 |
| Energy regeneration, Orbital Command, Nexus, Shield Battery | 0.5625 per Normal s | **0.0315 per tick** | **0** (hard-coded) | S5 |
| Shield regeneration rate and delay | 2 per Normal s, 10 Normal s | 0.112 per tick, 179 ticks | the same on all 29 rows | S4 |
| Zerg life regeneration | 0.2734 per Normal s | 0.01531 per tick | the same on 14 unit and 17 structure rows | audit R2 |
| Burrowed Roach life regeneration | 5 per Normal s | 0.28 per tick | no burrowed Roach | S7 |

### 2.4 Neighbouring rules: in or out

| Rule | Verdict | Why |
|---|---|---|
| Life-regeneration delay (Zerg: none; Reaper: 179) | **Conforming**, no work | §2.2 rule 4; plan `2026-11-05-life-regeneration-delay` |
| Shield regeneration delay and rate | **Conforming**, no work | §2.2 rule 3 |
| One damage counter for every vital (life-regeneration plan D-3) | Unchanged | No source found since; no shipped archetype has shields and a life delay |
| B-38: the burrowed Roach regenerates 0.28 per tick | **Deferred** to the burrow work of the abilities plan (deviation-fixes WP18), which owns B-38's other half (Tunneling Claws) | The Roach analogue (`rhino_beetle`) has no burrowed mode, and burrowing it needs the Burrow research, which `upgrades.ron` lacks. A rate override without a mode that uses it would be dead data. The value and the design hook are recorded in §14 F-1 so WP18 inherits them |
| Energy of a structure under construction | **In** (D-1): it holds `start_energy` until completion | The same gate as life; the value at completion is `start_energy` either way, so the only visible effect is that a foundation's energy does not move (**verify** when the game grants `EnergyStart` to a building under construction) |
| Structure energy regeneration (B-168) | **In** (D-2) | A clear [DATA] difference found while checking B-35; one field and three rows |
| Cloak energy drain, Graviton Beam's `DisableEnergyRegen`, the Oracle's beam, `BatteryAddEnergy` | Out (§14) | Abilities not modelled (B-69, B-70); no shipped mode drains energy (`energy_cost_per_second` appears in no catalog file) |
| `Suppress*Regen` state flags (Stasis Ward, Vortex) | Out (§14) | No shipped ability applies one |
| Zerg structures decaying off creep | Out (§14) | No creep (B-63) |
| Shields of a Protoss structure under construction | Out (§14 F-3, **verify**) | Ours are 0 until completion and then full; whether the game's grow with progress is not sourced |

---

## 3. Diagnosis

All in `crates/sim_core/src/world/upkeep.rs`, `upkeep_entity`, the "Passive Vitals & regeneration delays" block
(phase 2 of `step_into`, before unit tasks), and in `crates/sim_core/src/catalog.rs`.

1. **B-36: life regenerates on foundations.** The block applies

   ```text
   if ticks_since_last_damage >= arch.hp_regen_delay_ticks:
       hp := min(hp + arch.hp_regen_per_tick, max_hp)
   ```

   to every living entity, without looking at `entity.is_completed`. The shield line below it does look
   (`if regenerates && entity.is_completed`). A foundation is a living entity with `is_completed == false`
   (`production/construction.rs`, `prepare_foundation`), so a Zerg foundation gains 0.01531 life on every tick on
   top of the work's gain (`work_on_foundation`: `hp_per_tick`, and `final_hp_residue` on the last tick).
   - A Hatchery analogue (`ancient_tree_dome`, 1,786 ticks, 1,500 life) built without a pause ends 27.3 life above
     the construction curve; while undamaged it reaches `max_hp` before its last tick and is clamped there. A Spine
     Crawler analogue (`thorn_lasher_dome`, 893 ticks, 300 life) gains 13.7.
   - **A paused foundation heals itself.** With its builder gone, the construction curve stops, but regeneration
     goes on, up to `max_hp`. Any foundation of a regenerating structure left long enough is full before it is
     finished, and a damaged one heals while nobody builds it.
2. **B-35: no energy delay.** The line `entity.energy = (entity.energy + arch.energy_regen_per_tick).min(arch.max_energy)`
   runs on every tick. That is the game's rule (§2.2 rule 1). Nothing is wrong; the row records an unresearched
   question (deviation-fixes U-21), now answered.
3. **B-168: structures never regenerate energy.** `StructureArchetypeDef` has no `energy_regen_per_tick` field,
   and its conversion to `UnitArchetype` writes `energy_regen_per_tick: SimScalar::ZERO` (`catalog.rs`, the
   structure arm, beside `max_speed: SimScalar::ZERO`). The three rows with energy (`orbital_relay_dome` 200/50,
   `citadel_nexus_dome` 200/50, `harmonic_recharge_dome` 100/50 on `2bb84a5b`) hold their start energy forever.
   No shipped structure has an ability, so nothing spends it today, but the value is wrong in the hashed state
   and becomes a gameplay defect when any structure ability (Calldown: MULE, Chrono Boost, Recharge) arrives.
4. **The energy line also ignores `is_completed`.** With B-168 fixed, a Nexus foundation would regenerate energy
   during construction. D-1 gates it with life.

---

## 4. Design decisions

### D-1. No vital regenerates before completion (B-36)

**Rule.** In `upkeep_entity` the vitals block becomes:

```text
ticks_since_last_damage := saturating_add(ticks_since_last_damage, 1)      (unchanged, first)
if entity.is_completed:
    if ticks_since_last_damage >= arch.hp_regen_delay_ticks:
        hp := min(hp + arch.hp_regen_per_tick, max_hp)
    energy := min(energy + arch.energy_regen_per_tick, max_energy)
mode drain                                                                (unchanged, every tick)
shields: as today (delay and is_completed)                                (unchanged)
```

- **The counter still counts during construction.** Damage to a foundation resets it, so a structure finished
  under fire still waits its delays after its last hit, and a structure that was last hit long ago regenerates
  from the tick after completion. This keeps one meaning for the counter (life-regeneration plan D-3).
- **The mode drain stays outside the gate.** A foundation has no active mode (modes are entered by abilities, and
  `record_cast_states` already publishes none for an uncompleted entity), so this is the same result either way;
  leaving the drain where it is changes nothing for units.
- **The first regeneration is on the tick after completion.** Completion happens in the unit-task phase
  (`work_on_foundation` → `finish_structure`), after phase 2 of the same tick. Units are created completed
  (`Entity::new`, `is_completed: true`), so nothing changes for them.
- **Why code, not data.** It is an engine rule for every entity, like the shields' completion gate. No archetype is
  named, and no catalog value expresses it (§3.3 of the charter forbids identity, not engine rules).
- **The source is missing** (§2.2 rule 5), so the rule is the register's reading with **verify**, and the draft
  owner question (§15) asks the owner to confirm it in the game. Reversing it is one condition.

**Invariants** (the `upkeep.rs` module doc's list, and `combat.rs` invariant 4 where it speaks of regeneration):

1. Life and energy regenerate on a tick only if the entity is completed; shields as before.
2. A foundation's life equals its start life plus the gains of the ticks worked on it minus the damage it has taken,
   clamped to `[0, max_hp]` (the construction module's "damage taken stays taken", now exact).
3. A foundation's energy equals its archetype's `start_energy` until completion, less anything spent (nothing can
   spend it: an uncompleted entity publishes no cast state and is refused as a caster, **verify** the admission
   path in `world/casts.rs` reads `is_completed` and add the refusal if it does not).
4. The rest of the module's invariants are unchanged.

`upkeep_entity` grows by two lines and stays under 100 (C8). The implementer may extract the vitals block into a
private `fn regenerate_vitals(entity: &mut Entity, arch: &UnitArchetype)` if it keeps the order above.

### D-2. Structure energy regeneration is data (B-168)

- **Field.** `energy_regen_per_tick: f64` in `StructureArchetypeDef` (`catalog.rs`), `#[serde(default)]`, next to
  `start_energy`, with the `///` doc "Energy regenerated per tick (25 per Faster second) once the structure is
  complete (`EnergyRegenRate` × 1.4 ÷ 25); omitted reads as 0, the value of every structure without energy." The
  conversion reads it with `SimScalar::from_num`, as its neighbours do, instead of `SimScalar::ZERO`.
- **Why a default of 0, not a required field.** Of 49 structure rows, 46 have no energy; 0 is the game's value for
  them (no `EnergyRegenRate` on the `CUnit` default, S1). Requiring the field would add 46 lines that state the
  default. `catalog_reference_values_tests` pins the three rows that matter (WP2), so an omission there is caught.
- **Validation** (`catalog/validation.rs`): the field joins the structure's list of nonnegative, finite fields
  (beside `max_energy`, `start_energy`). A new private `fn energy_regen(definition, max_energy, rate)` refuses a
  positive rate when `max_energy` is 0 (`SimCoreError::CatalogInvalidValue { field: "energy_regen_per_tick",
  reason: "must be zero when max_energy is zero" }`), the rule of `hp_regen_delay` for a value nothing reads. It is
  called from both `UnitArchetypeDef::validate` and `StructureArchetypeDef::validate`; every shipped unit already
  satisfies it (the 10 rows with a rate have `max_energy` 200), which WP2's test proves.
- **Data.** `energy_regen_per_tick: 0.0315,` on the three rows, each with a comment citing S5's lines.
- **Zero recompilation** (charter §3.3): changing the field in a copy of the catalog changes the regeneration in
  the same binary (`runtime_catalog_tests`, WP2).

### D-3. B-35 closes as conforming; no delay field

No `energy_regen_delay_ticks` field is added. No layer of the game sets one (S2), so every row would read 0, and
the rule would be a branch no data uses. If an archetype ever needs a delay, the field is added then, as the life
delay was (§14 F-2). B-35's register row is marked "not a deviation" with S2 as its evidence, and two tests pin the
rule (WP1): energy rises on the tick after a cast and on the tick after damage.

### D-4. Hashed state: none added; schema unchanged

`Entity::hp`, `energy`, `ticks_since_last_damage` and `is_completed` are hashed already
(`checksum/encoding.rs`). The catalog is not hashed. So `CHECKSUM_SCHEMA_VERSION` does not move, no encoding
changes, and no package claims a schema number. If the contact-solver branch merges first, this plan simply
follows whatever schema `main` has. Checksums of a world move only where its behaviour moves (§5).

### D-5. Presentation

- **No client code changes.** The health bar reads `hp / max_hp` from `RenderSnapshot` (`world/snapshot.rs`). A
  damaged Zerg foundation's bar now follows its construction and stays there while the work is paused, which is
  the corrected behaviour itself.
- **No energy display exists** in the HUD (`battle_hud/card.rs` reads energy only as `short_of_energy` of a cast
  state, and structures have no abilities). Structure energy rising is visible nowhere until a structure ability
  or an energy bar is added. Not added here (§14).

### D-6. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | Register **B-168**: "Structures never regenerate energy: the structure loader writes a rate of 0; the Orbital Command, Nexus and Shield Battery regenerate 0.5625 per Normal second (`liberty UnitData:10515`, `:4240`, `:1593`)". B-35, B-36 and B-38 point to this plan (B-38: deferred to WP18 with §14 F-1) | WP0 |
| `DESIGN.md` §4 | B-35 "not a deviation" (S2), B-36 and B-168 fixed, each with its commit and evidence link; the Combat row gains "life and energy regenerate only on completed entities" | WP3 |
| `docs/03-sc2-mechanics-reference.md` §1.1 | Energy row: "no regeneration delay (no `EnergyRegenDelay` in 5.0.14)"; structures with energy regenerate it; a new sentence under the table: "No vital regenerates while a structure is under construction" with `is_completed` in the Engine column | WP3 |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md` | WP15's row: "B-35 (not a deviation), B-36 done by plan `2026-11-23-regeneration`; B-38 to WP18; U-10 and U-21 answered" | WP3 |
| `docs/plans/2026-11-05-life-regeneration-delay.md` §8 | F-3 and F-4: "taken up by plan `2026-11-23-regeneration`" | WP3 |
| New `docs/evidence/<date>-regeneration.md` | §7's comparisons per package | WP1 creates; WP2, WP3 append |
| `docs/plans/README.md` | This plan's row: `Proposed` (with the plan), `Active` (WP1), `Done` (WP3) | plan, WP1, WP3 |

---

## 5. Determinism and checksum

- **Integer and fixed point only.** The change is a boolean test and the existing `SimScalar` additions. The new
  rate is authored as `f64` and converted with `SimScalar::from_num` at load, like every structure value (C7).
- **No new hashed state, no schema change** (D-4).
- **Goldens are expected unchanged in every package.** `golden_replay.ron` places two `foundry_core_dome` (the
  Command Center analogue: `max_energy` 0, no life regeneration) and builds nothing; `golden_replay_orders.ron`
  places no structure. Neither contains a foundation or a structure with energy. `sim_fixtures`'
  `DEFAULT_SANDBOX_CHECKSUM` is of the spawned state before any tick (`fixture_tests.rs`, **verify** that it steps
  no tick; if it does, its command, supply and production roles must be checked for energy and regeneration).
- **If a golden moves anyway**, the package stops and the move is proved, not just re-recorded:
  1. Trace the first moved checkpoint to an entity (the checkpoint diff or `matchup_trace`'s state dump) and name
     the cause (a foundation's life, a structure's energy).
  2. With the package's change disabled by a temporary local edit (the `is_completed` test removed, or the three
     rows' rate set back to 0), the tree reproduces every recorded checksum of both goldens on native debug and
     release and on `wasm32-wasip1` release and debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real
     tree path>"`, the path the tree really has, not a symlink).
  3. Then the change goes back in, the checksums are re-recorded under a header "Re-recorded for plan
     2026-11-23-regeneration WP<n>: <cause>. No other behaviour changed: with <the change> disabled, every
     checkpoint stays as recorded before", and the four builds reproduce the new values.
  No such move is expected; an unexplained one is a defect.
- **wasm.** No dependency changes; the simulation crates still build for `wasm32-unknown-unknown`.
- **Cost.** One boolean per entity per tick; three structure rows gain an addition per tick. The throughput bench
  is unaffected (its scenarios contain no foundation).

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure of a seed. Each property holds for every tick of the worlds that test it, and the tests
read every rate, delay, life and build time from the catalog; regenerating structures are found by property
(`hp_regen_per_tick > 0`, `max_energy > 0`), never by slug.

- **P-1 (no regeneration before completion).** For every entity with `is_completed == false`, on every tick: `hp`
  changes only by construction gains and damage, `energy` does not change, `shields` stay 0.
- **P-2 (the construction curve is exact).** For every structure archetype in the shipped catalog with
  `hp_regen_per_tick > 0`: a foundation built without damage completes with exactly `max_hp` on its completion
  tick and not before; one damaged by `d` life at any point completes with `max_hp − d` (exact fixed point:
  start + Σ `hp_per_tick` + `final_hp_residue` − d); and a foundation paused for any number of ticks has the same
  life when work resumes as when it stopped.
- **P-3 (regeneration after completion).** On the tick after completion, a completed regenerating structure gains
  `min(hp_regen_per_tick, max_hp − hp)` if its counter meets `hp_regen_delay_ticks`, and its energy gains
  `min(energy_regen_per_tick, max_energy − energy)`.
- **P-4 (energy has no delay, B-35).** For every completed entity with `energy_regen_per_tick > 0` and energy below
  its maximum, energy on tick t+1 is `min(e_t + rate, max)` minus that tick's spending, whatever happened on tick t:
  a cast, damage, both or neither.
- **P-5 (structure energy, B-168).** Each shipped structure with `max_energy > 0` regenerates at its authored rate
  of 0.0315 per tick, and no structure without energy authors a positive rate (validation).
- **P-6 (units are untouched).** Every world without a foundation and without a structure that has energy is
  byte-identical before and after each package (§7).
- **P-7 (zero recompilation).** A catalog copy with a different structure rate changes the regeneration in the same
  binary.

---

## 7. Worlds that move, and why no prototype was built

**No measured world contains a foundation of a regenerating structure or reads structure energy.** The matrix,
battles 31 and 33 and the showcase on nine seeds spawn finished units and structures (the showcase's loadouts are
`battle_automaton` and the showcase sets, the Terran analogue, `assets/catalog/start_loadouts.ron`), and their
outputs print no energy. The client's skirmish match plays `standard_automaton` on both sides: Terran structures
have no life regeneration, so B-36 cannot act there, and B-168 acts only if the pilot builds the Orbital analogue,
whose energy no stage reads (**verify** in WP2 from the stage log). A prototype would measure a constant; each
package proves P-6 by running the same commands on its tree against WP0's.

**Commands** (the production-queue plan's; binaries built once per tree, run from that tree so its assets are
read; outputs under `~/scratch/regeneration/<wp>/`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} ~/scratch/regeneration/bin-<wp>/
~/scratch/resource-footprints/scripts/measure.sh ~/scratch/regeneration/bin-<wp> <tree> ~/scratch/regeneration/world-<wp>
diff -r ~/scratch/regeneration/world-wp0 ~/scratch/regeneration/world-<wp>     # expected: no output
```

(`measure.sh`: the matrix with seeds of record and 16, battle 31 on 100 seeds, battle 33 on 64, the showcase on its
default seed and seeds 1 to 8 for 6,000 ticks.) Run in the background, one at a time across worktrees (charter §7).

| World | WP1 | WP2 |
|---|---|---|
| Matrix, battles 31 and 33, showcase (9 seeds) | byte-identical | byte-identical |
| `golden_replay`, `golden_replay_orders`, `DEFAULT_SANDBOX_CHECKSUM` | unchanged | unchanged |
| Client skirmish match (`skirmish_match_tests`) | stage log identical | stage log identical |
| Constructed worlds (§12) | P-1 to P-4 | P-3, P-5, P-7 |

A difference in a byte-identical row is a defect of the package, not a re-record (§8).

**Where the change does show** is in constructed worlds only: a Zerg foundation built, damaged and paused (P-2); a
Nexus, Orbital and Shield Battery analogue spawned and stepped (P-5). The evidence records, for each regenerating
structure row, the life at completion before and after WP1 (computed by the WP1 test, printed with
`--nocapture`), so the size of the old error is on record (expected: `max_hp` today for every undamaged foundation,
reached early; `max_hp` exactly on the completion tick after).

---

## 8. Stop rules

1. **WP1 changes only foundations.** P-1, P-2 hold; §7's worlds are byte-identical; every failing existing test is
   one §12 lists as re-derived, re-derived from D-1 (not loosened). An unlisted failure stops the package.
2. **WP2 changes only structures with energy.** P-5, P-7; §7's worlds byte-identical; goldens unchanged. A test that
   asserted a structure's energy constant is re-derived from S5 and named in the commit.
3. **No tuning constant** is introduced: the rate is the data's, the gate is a state the world already has.
4. **A golden move** follows §5's proof or stops the package.

---

## 9. Client consequences

None in code (D-5). The client's tests that build structures (`battle_card_path_tests`, `skirmish_match_tests`,
`full_match`-style client tests) run in the package gate as usual; none is expected to change, because none builds
a regenerating structure (Terran pilot) and none reads structure energy (**verify** with `grep -rn
"energy" crates/client/tests`).

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP3 | Register B-168; pointers; then fixed / not a deviation; Combat row |
| `docs/plans/README.md` | plan, WP1, WP3 | Row `Proposed` → `Active` → `Done` |
| `crates/sim_core/src/world/upkeep.rs` | WP1 | D-1's gate, module doc and invariants |
| `crates/sim_core/src/combat.rs` | WP1 | Module invariant 4's wording only |
| `crates/sim_core/src/production/construction.rs` | WP1 | Module doc, "The life of a building" item 4: "nothing regenerates until completion"; no code |
| `crates/sim_core/src/world/casts.rs` | WP1, only if D-1 invariant 3's **verify** finds no completion check | The refusal of an uncompleted caster |
| `crates/sim_core/src/catalog.rs` | WP2 | `StructureArchetypeDef::energy_regen_per_tick`, its conversion, `///` docs (`UnitArchetype::energy_regen_per_tick`: "only on completed entities") |
| `crates/sim_core/src/catalog/validation.rs` | WP2 | `energy_regen` rule, nonnegative list, unit tests |
| `assets/catalog/structures.ron` | WP2 | Three rows, with S5's lines |
| `crates/sim_core/tests/regeneration_tests.rs` (new, `//!` doc) | WP1, WP2 | §12 |
| `crates/sim_core/tests/construction_lifecycle_tests.rs` | WP1 | Re-derived assertions on foundation life, if any read a regenerating structure (§12) |
| `crates/sim_core/tests/catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs` | WP2 | §12 |
| `crates/gate/suites.ron` | WP1, only if needed | The new target, if the catalog requires an entry for it (**verify**: an uncovered integration test may fall to the crate's default class) |
| `docs/03-sc2-mechanics-reference.md`, `docs/plans/2026-11-05-sc2-deviation-fixes.md`, `docs/plans/2026-11-05-life-regeneration-delay.md` | WP3 | D-6 |
| `docs/evidence/<date>-regeneration.md` (new) | WP1 to WP3 | §7 |

Nothing under `crates/sim_core/src/movement/` changes; a contact-solver branch works there.

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`
(check `quota -s` and `df -h ~/scratch` first); the implementation agent writes the failing tests first and pastes
their red run in the hand-back, runs `cargo gate quick` and the tests its change needs, and hands back; a verifier
runs `cargo gate package` (Q0061). §7's comparison runs in WP1 and WP2 against WP0's tree. No `unwrap`; `expect`
only with `invariant:`; no catalog identity in production Rust (C5); tests find archetypes by property. Check
`docs/questions/open/` before the last commit. All four packages may run as one branch and one agent session, one
commit each, with one `package` gate on the final tree (charter §4.3: test a tree once).

### WP0: records (XS, docs only)

- `DESIGN.md` §4: B-168 registered (D-6); B-35, B-36 and B-38 point to this plan.
- Measure WP0's tree (§7) for the baseline.
- **Gate:** docs-only.

### WP1: no regeneration before completion (S, `sim_core`)

Failing tests first (red on WP0's tree), in the new `regeneration_tests.rs`:
- `a_foundation_does_not_regenerate_life`: for every shipped structure with `hp_regen_per_tick > 0`, lay its
  foundation (the `Build` path the construction tests use), let a builder work 10 ticks, remove the builder, step
  100 ticks: life unchanged (red today: it rises by 100 × rate).
- `a_foundation_completes_on_the_construction_curve`: undamaged, life is below `max_hp` on every tick before the
  completion tick and equals `max_hp` on it (red today for every regenerating structure: full early).
- `damage_to_a_foundation_stays_taken`: damage `d` through the public damage path mid-build; at completion life is
  `max_hp − d` exactly (red today).
- `a_completed_structure_regenerates_from_the_next_tick` (P-3): green before and after for structures; asserts
  the first gain on the tick after `StructureCompleted`.
- `a_foundation_holds_its_start_energy`: a scratch structure archetype with `max_energy` 100, `start_energy` 50 and,
  through a struct literal of `UnitArchetype` (WP2 does not exist yet), `energy_regen_per_tick` 1: the foundation
  holds 50 until completion and gains 1 on the tick after (red today: it rises during construction).
- B-35 guards (green before and after, P-4): `energy_regenerates_on_the_tick_after_a_cast` (a shipped caster found
  by an ability with `energy_cost > 0`, cast once, energy on the next tick is the cost-reduced value plus the rate)
  and `energy_regenerates_on_the_tick_after_damage`.
- An invariant helper `assert_no_regeneration_before_completion(world_before, world_after)` (P-1), run after every
  tick of the foundation tests.

Re-derived (expected to change, from D-1): any assertion in `construction_lifecycle_tests.rs` or
`production_transactions_tests.rs` on the life of a Zerg foundation (none found while planning by
`grep -n "hp" crates/sim_core/tests/construction_lifecycle_tests.rs`, which builds Terran analogues; **verify**).

Then the change: D-1 and its docs. **Acceptance:** §8 rule 1; §7 worlds byte-identical; evidence created with the
per-row life at completion (§7). Plan row `Active`.

Commit `fix(sim_core): no vital regenerates before a structure completes (regeneration WP1, B-36)`; the message
states that no world of §7 and no golden moved, and that B-35's rule is pinned.

### WP2: structure energy regeneration (S–M, `sim_core`, catalog)

Failing tests first (red on WP1's tree):
- `catalog/validation.rs` unit tests: `a_structure_without_energy_rejects_an_energy_rate` and the same for a unit
  (`CatalogInvalidValue` naming `energy_regen_per_tick`); a negative rate is refused by the nonnegative check; an
  omitted field reads 0 on every structure row without energy.
- `catalog_reference_values_tests::structures_with_energy_regenerate_as_5_0_14`: every shipped structure with
  `max_energy > 0` has `energy_regen_per_tick` 0.0315 (0.5625 × 1.4 ÷ 25), with S5's lines in the test's doc.
- `regeneration_tests::a_structure_with_energy_regenerates_it` (P-5): each such structure spawned completed below
  its maximum gains its rate per tick and stops at `max_energy`.
- `runtime_catalog_tests`: a catalog copy with the rate doubled on one structure doubles its gain in the same
  binary (P-7).

Then the change: D-2 and the three rows. **Acceptance:** §8 rule 2; §7.

Commit `feat(sim_core): structures regenerate energy from the catalog (regeneration WP2, B-168)`, citing S5 and the
conversion.

### WP3: bookkeeping (XS)

- D-6's records: B-35 "not a deviation" (S2), B-36 and B-168 fixed with commits; the Combat row; doc 03; the
  deviation-fixes and life-regeneration pointers; the evidence completed; this plan's `## As built`; README row
  `Done`.
- Merge `main`; `cargo gate package` once on the final tree (by the verifier).

---

## 12. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Rule | P-1 to P-3: no regeneration before completion; the exact construction curve; regeneration from the next tick | WP1: `regeneration_tests.rs` |
| B-35 | P-4: energy rises on the tick after a cast and after damage | WP1: `regeneration_tests.rs` |
| Data | P-5, P-7: field validated, read at run time, sourced | WP2: `catalog/validation.rs` unit tests, `catalog_reference_values_tests`, `runtime_catalog_tests`, `regeneration_tests.rs` |
| Worlds | P-6: §7's table | WP1, WP2 (measure script); goldens and sandbox in the package gate |
| Charter subjects | `sim_core`'s subjects of `docs/guides/test-gates.md` (shield and armour spillover, a whole match, golden replay) keep passing unchanged | package gate |

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The game does regenerate structures under construction (D-1 is **verify**) | A small difference reversed: up to 27 life on a Hatchery | One condition to remove; the draft question (§15) asks the owner to look; the register row records the reading |
| R-2 | A test elsewhere asserts a Zerg foundation's life or a structure's constant energy | Package red for a correct change | §8 rules 1 and 2: re-derive from D-1 or S5, name it in the commit |
| R-3 | An uncompleted structure can cast (D-1 invariant 3) | Energy spent on a foundation | The **verify** step in WP1; a refusal added if missing (no structure has abilities today, so latent) |
| R-4 | The skirmish pilot builds the Orbital analogue and a later stage reads energy | Stage log differs | §7 says identical; a difference is traced and stops WP2 |
| R-5 | A golden or the sandbox moves | Checksum churn | §5's proof procedure; not expected |
| R-6 | The contact-solver branch merges first and changes `upkeep.rs` or the schema | Conflict | It works under `movement/`; `upkeep.rs` is not its file; this plan claims no schema |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: B-38, the burrowed Roach's regeneration** (deferred to deviation-fixes WP18, the burrow work). Recorded for
  that plan: the value is `LifeRegenRate 5` on `RoachBurrowed` (`liberty UnitData:7262`), **0.28 per tick**, no
  delay (S7). The hook is a mode override beside the Lurker analogue's `burrowed` mode fields: an optional
  `hp_regen_per_tick` on the mode (`Option<f64>`, `None` keeps the archetype's), read by `upkeep_entity` through the
  settled mode, as weapons are overridden through `ground_weapon_override`. The Roach analogue then gets a
  `burrowed` mode gated by the Burrow research (absent from `upgrades.ron`) and Tunneling Claws its movement.
- **F-2: an energy-regeneration delay field.** Added when an archetype needs one (D-3).
- **F-3: shields of a Protoss structure under construction.** Ours are 0 until completion, then full
  (`finish_structure`). Whether the game's grow with progress like life is not sourced (**verify**); not
  registered until it is.
- **F-4: when a building under construction gets its `EnergyStart`** (at placement or at completion). The value at
  completion is the same either way (§2.4).
- **Energy behaviours of abilities** (S10): cloak drain as a regeneration modifier (the data's `VitalRegenArray
  Energy −1.4765` adds to the rate, so a cloaked Ghost's net change is (0.5625 − 1.4765) × 1.4 = −1.28 per Faster
  second; when cloak is modelled through `energy_cost_per_second`, which upkeep subtracts after adding the full
  rate, the authored drain is the data's 1.4765 × 1.4 = 2.067 per second), Graviton Beam's `DisableEnergyRegen`
  (B-70), the Oracle's beam, the Shield Battery's `BatteryAddEnergy` near a Nexus, and the `Suppress*Regen` flags of
  Stasis Ward and Vortex. Each belongs to its ability's plan.
- **Zerg structures decaying off creep** (S8): needs creep (B-63).
- **An energy bar** in the HUD (D-5).

---

## 15. Draft owner question (for the orchestrator to file; not filed by this plan)

```markdown
# Q0094: Do Zerg structures regenerate life while under construction?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** no: a structure under construction gains life only from the building work; it
  regenerates life (and energy) from the tick after it completes (plan `2026-11-23-regeneration` D-1).

## Question

Space Scuffle's Zerg structures regenerate 0.38 life per second when finished. Until now they also regenerated
while being built, so a damaged Hatchery foundation healed while it was built, and even while its builder was away.
The StarCraft II 5.0.14 data has no flag for this either way, and no web source we found says what the game does.

If you can, check in the game: start a Hatchery (or Spine Crawler), have a Zergling hit it a few times, and watch
whether its life rises faster than the build progress between hits (or simply compare its life at completion with
its maximum).

- (a) **No regeneration under construction** (the default, recommended: it is what the bug register assumed and how
  our shields already behave).
- (b) Regeneration under construction, as before.

The choice is one condition in the code; either is easy to switch.

## Answer

```

The question number is the next free at filing (0094 on `2bb84a5b`).

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`), read as data with `grep` and `sed`.
- Web: three searches on 2026-11-23 (S8, S9); Liquipedia's own pages were refused by its rate limit that day, so S8
  rests on the search excerpt. No source in this plan depends on a page that could not be read, except the
  **verify** items.
- No prototype (§7).
