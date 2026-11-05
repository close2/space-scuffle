# Blueprint: Life Regeneration After a Per-Archetype Delay (Q0049 option b, audit group R1)

This plan gives every archetype a life-regeneration delay read from the catalog, sets StarCraft II's 179 ticks on
the Reaper analogue (`jet_jumper`), and then applies the catalog-time audit's deferred correction R1
(`hp_regen_per_tick` 0.08 → 0.112). It reuses the mechanism that plan `2026-11-04-combat-sc2-alignment` WP9 built
for shields (`shield_regen_delay_ticks`, D-4 there).

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-05 against `main` at `0f4e7fc`
(combat alignment WP12 merged; WP17 in progress). Line numbers drift; search for the quoted symbol. Claims that
could not be confirmed while planning are marked **verify**; expectations not yet measured are marked as such.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| [Q0049](../questions/closed/0049-reaper-life-regeneration-delay.md) life regeneration delay | Closed by WP4; this plan carried out option (b) under the standing rule Q0008 (D-1) | WP0 (default updated), WP4 (closed) |
| [Q0039](../questions/open/0039-normal-time-values.md) Normal-time values | Open; closed by combat alignment WP13 after WP12. Its group R1 was deferred there and is applied here | WP3; hand-off to WP13 in WP4 |
| [Q0008](../questions/closed/) StarCraft II where the specification is silent (charter §8) | Standing rule | D-1 |

---

## 1. Summary

| WP | Title | Changes expected results? | Size | Depends on |
|---|---|---|---|---|
| WP0 | Docs: Q0049's default becomes option (b), with a pointer to this plan | no | XS | none (first commit of the branch) |
| WP1 | Code: `hp_regen_delay_ticks` in the catalog, validation and upkeep; 0 everywhere | no (proved bit-identical) | M (half a day to a day) | WP0; combat alignment WP17 merged |
| WP2 | Data: `jet_jumper` `hp_regen_delay_ticks: 179` | yes: battles 22a and 22b only | S (2 to 3 h) | WP1 |
| WP3 | Data: audit R1, `jet_jumper` `hp_regen_per_tick` 0.08 → 0.112 | expected no in the matrix; visible in tests and per-tick traces | S (1 to 2 h) | WP2 |
| WP4 | Bookkeeping: doc 03, audit and alignment-evidence pointers, Q0049 closed, plan row | no | XS | WP3 |

**Lane and timing.** This is lane B work, one branch, one gate. It edits `catalog.rs`, `catalog/validation.rs` and
`assets/catalog/units.ron`, which combat alignment WP17 edits too (renamed turn-rate fields, range validation that
replaces `angles_preserve_finite_legacy_rates_but_reject_nonfinite_values`, 42 hull rates). WP17 also moves
potentially every matrix battle, battles 22a and 22b among them. So:

- **Start after WP17 has merged to `main`.** The baseline of D-8 must be the post-WP17 matrix, so that this plan's
  `--compare` shows only its own causes. Starting earlier would mean measuring against a baseline that changes
  under the branch and resolving conflicts in the same three files.
- **Finish before combat alignment WP13** (bookkeeping) and before WP14 (scan ranges, lane E). WP13 closes Q0039,
  whose resolution should list R1 as applied (by this plan's WP3), and at the end of lane B it rewrites the
  `DESIGN.md` Combat row, which should then describe both regeneration delays. WP14 edits `units.ron` and measures
  on the final catalog.
- If WP17 stalls for more than a few days, the orchestrator may run this plan first. In that case WP17 measures on
  top of it, and the stated order in §1 of the alignment plan gains this plan between WP12 and WP17. Record the
  swap in both plans' rows; nothing else changes.

---

## 2. What is known

### 2.1 Regeneration today (`crates/sim_core/src/world/upkeep.rs`, `upkeep_entity`)

Per living entity, per tick, in phase 2 of `step_into` (before weapons in phase 9):

1. channel, ability-cooldown and buff timers;
2. `entity.hp = min(hp + arch.hp_regen_per_tick, max_hp)`, on **every** tick, with no delay;
3. energy regeneration, then the active mode's energy drain;
4. `ticks_since_last_damage = ticks_since_last_damage.saturating_add(1)`;
5. shields regenerate when `arch.shield_regen_delay_ticks` is `Some(d)`, `ticks_since_last_damage >= d`, and the
   entity is completed;
6. hit-flash timers.

`Entity::apply_damage` (`entity.rs`) sets `ticks_since_last_damage = 0` for any hit, whether the damage went to
shields, to life or to both. Every damage path goes through it: `combat_resolution.rs` (direct hits and splash) and
`world/abilities.rs` (hazard zones). The Stimpack / Overclock life cost (`entity.rs`, `hp_cost`) does not go
through it and does not reset the counter. A new entity starts with the counter at `u16::MAX` ("never damaged",
WP9). The counter is hashed (`checksum/encoding.rs`); the catalog is not.

**Timing.** Damage on tick T (phase 9) sets the counter to 0. The upkeep of tick T+n leaves it at n, so a delay of
d regenerates first on tick T+d. `damage_and_shields_tests.rs` pins this for shields
(`assert_shields_regenerate_after_catalog_delay`).

### 2.2 Who regenerates life (shipped catalog after WP12)

- **33 unit archetypes in `units.ron` and 17 Zerg structures in `structures.ron`** have `hp_regen_per_tick > 0`:
  the Zerg at 0.01531 (`LifeRegenRate` 0.2734, audit R2), `sonic_bat` at 0.056 (the Mutalisk, `LifeRegenRate` 1),
  and `jet_jumper` at 0.08.
- No Protoss or other Terran archetype regenerates life. **No archetype has both shields and life regeneration.**
- `jet_jumper` appears in the matchup matrix only, in battles `22a_reaper_vs_2_workers` (expect `blue (1,1), red
  (0,0)`, no hull range) and `22b_reaper_vs_stalker` (expect `blue (0,0), red (1,1)`), each limited to 200 ticks,
  and in `combat_correctness_tests::s13_projectile_splash_spares_its_attacker`, which asserts at full life and
  so cannot see regeneration. No golden replay, scenario or client fixture spawns or trains it. The Barracks
  analogue lists it in `trains_units`, but no fixture trains one.
- `sim_fixtures::default_battle_sandbox` spawns every archetype at full life; its checksum is of the spawned state.

### 2.3 The reference (StarCraft II 5.0.14, chain `core → liberty → swarm → void → voidmulti`, audit §2.2)

| Value | Game data | Our units | Source |
|---|---|---|---|
| Reaper `LifeRegenRate` | 2 per Normal s | 2 × 1.4 ÷ 25 = **0.112 per tick** (exact) | `void` UnitData:1476 (also `swarmmulti` UnitData:894) |
| Reaper `LifeRegenDelay` | 10 Normal s | 10 ÷ 1.4 × 25 = 178.57 → **179 ticks** (nearest, halves up) | `void` UnitData:1475 (also `swarmmulti` UnitData:893) |
| Every other unit's `LifeRegenDelay` | not set in any layer of the chain (checked while planning: the only `LifeRegenDelay` elements in the five layers plus `libertymulti`, `swarmmulti` and `balancemulti` are the Reaper's two) | 0 (no delay) | engine default for an unset field |
| Zerg `LifeRegenRate` | 0.2734 | 0.01531 (shipped since R2) | `void` UnitData:1127 and others |

Liquipedia agrees: "Note: Although the 'Combat Drugs' upgrade states certain values, the regen is actually 2hp/s
with 10s delay"; infobox "Heals when out of combat" ([Reaper (Legacy of the Void)](https://liquipedia.net/starcraft2/Reaper_(Legacy_of_the_Void)),
raw wikitext read 2026-11-05).

**What resets the delay.** Liquipedia's shield page says "If a unit or building is not damaged for a period of time,
its shields will regenerate until they are fully replenished or more damage is taken"
([Plasma Shield](https://liquipedia.net/starcraft2/Plasma_Shield), read 2026-11-05). It speaks of the unit being
damaged, not of its shields being damaged. The game data has separate `LifeRegenDelay`, `ShieldRegenDelay` and
`EnergyRegenDelay` fields, but neither the data nor any source found says whether damage to one vital restarts the
delay of another. **Not found** within the effort bound (an SC2Mapster search and the two Liquipedia pages). It
does not matter for any shipped value: the only archetype with a life delay has no shields (§2.2). D-3 records the
choice.

### 2.4 The audit's measurement (why R1 waited)

With 0.112 applied **without** a delay, `22b_reaper_vs_stalker` ends unresolved on all 20 seeds at its 200-tick
limit (reaper 3.62 life, stalker 80), where it was a stalker win: the reaper out-heals the stalker in combat
(audit [§7](../evidence/2026-11-04-catalog-time-audit.md), alignment evidence §13.11). Left out one group at a time,
only R1 caused it. In the game the reaper never heals while it is being shot.

---

## 3. Design decisions

### D-1. Option (b), chosen without waiting for the owner

Q0049 asks the owner and recommends (b), with option (a) as the default until answered. Charter §8 (Q0008) is a
standing owner rule: "Where the specification is silent on a game rule, do what StarCraft II does", citing the
source. Our specification is silent on a life-regeneration delay (doc 03 §1.1 describes only the shield delay). The
game has one (§2.3). So the rule settles the question, as it settled the audit's class C values. Option (c) is
excluded by the same rule (§2.4: it moves away from the game). Option (a) is what the code does before this plan.

The decision is easy to reverse: a delay of 0 on `jet_jumper` restores every-tick regeneration, and the rate is one
value. WP0 updates Q0049's default to (b) with a pointer here. WP4 closes it with a `## Resolution` saying it was
carried out under Q0008, naming the commits, and saying the owner may reopen it (questions README, "change your
mind").

### D-2. Data model: `hp_regen_delay_ticks: u16`, default 0

**Field.** `hp_regen_delay_ticks: u16` in `UnitArchetypeDef`, `StructureArchetypeDef` and `UnitArchetype`
(`catalog.rs`), next to `hp_regen_per_tick`, with `#[serde(default)]` in both definitions. Units: ticks at 25 per
Faster second, counted from the tick of the last damage. `///` doc on `UnitArchetype::hp_regen_delay_ticks`:

> Ticks (25 per Faster second) without taking damage before life starts regenerating, counted from the tick of the
> last damage (`Entity::ticks_since_last_damage`). 0 means life regenerates on every tick, as for StarCraft II's
> Zerg. StarCraft II stores this per unit as `LifeRegenDelay`; only the Reaper sets it.

**Why `u16` with 0, not `Option<u16>` like the shield field.**
1. *0 is the game's own value for "no delay".* Every unit except the Reaper leaves `LifeRegenDelay` unset (§2.3).
   An omitted field reading as 0 copies the game data exactly, and the 50 regenerating archetypes need no edit.
2. *No `None` with two meanings.* On the shield field `None` means "never regenerates shields" and is tied to
   `max_shields`. For life, `None` would have to mean "regenerates without delay". Two `None`s next to each other
   with opposite meanings is the kind of trap charter §1.1 ("no leaky abstractions") warns against.
3. *No special case in code.* `ticks_since_last_damage >= 0` is always true, so the delay-0 path is the general
   rule, not a branch. That also makes WP1's "no change" proof a property of the comparison, not of a test.
4. *Same type as the counter.* `u16` compares directly with `ticks_since_last_damage` (saturating at `u16::MAX`,
   655 s), and the `ron` loader rejects values above 65 535 at parse time.

The shield field stays `Option<u16>`; changing it is out of scope (§8).

**Which archetypes get a value.** Only `jet_jumper`: `hp_regen_delay_ticks: 179` (WP2). Every other archetype
omits the field and reads 0. Structures get the field for symmetry with `shield_regen_delay_ticks` and because the
game's field belongs to every `CUnit`; no structure sets it.

**Validation** (`catalog/validation.rs`, a new private `fn hp_regen_delay(definition, regenerates: bool, delay: u16)`
beside `shield_regen_delay`, called from `UnitArchetypeDef::validate` and `StructureArchetypeDef::validate`):

| Authored | Result |
|---|---|
| `hp_regen_per_tick > 0`, any delay | accepted |
| `hp_regen_per_tick == 0`, delay 0 (or omitted) | accepted |
| `hp_regen_per_tick == 0`, delay > 0 | `SimCoreError::CatalogInvalidValue { field: "hp_regen_delay_ticks", reason: "must be zero when hp_regen_per_tick is zero" }` (a value nothing reads, the rule of `shield_regen_delay`'s `(false, Some(_))` arm) |

"Regenerates" is the authored `hp_regen_per_tick > 0.0`. It runs after the existing `nonnegative` check of the rate
(units: in the list of nonnegative fields; structures: the first line of `validate`), so a negative rate is
reported first under its own field. No upper bound beyond `u16`. A delay is legal on any regenerating archetype,
shielded or not (D-3).

### D-3. One counter for every vital, reset by any damage

`Entity::ticks_since_last_damage` stays the only counter. Any call of `apply_damage` resets it, whether the damage
went to shields, to life or to both, and both delays read it.

- **Source:** the game describes regeneration as waiting until the *unit* is not damaged (§2.3, Liquipedia Plasma
  Shield; Reaper "heals when out of combat"). Whether shield-only damage restarts a life delay was not found (§2.3).
- **Why not a counter per vital:** it would add hashed entity state, a checksum schema bump and re-recorded goldens
  for a difference no shipped archetype can show (§2.2). If a future archetype with shields and a life delay needs
  per-vital timers, that is a separate rule with its own source (§8 F-1).
- **Pinned by a test** (WP1: `shield_damage_alone_restarts_the_life_regeneration_delay`), so the choice is explicit
  and a later change is visible.
- **The Stimpack life cost does not reset the counter**, as today. Whether it does in the game was not researched.
  No archetype that stims regenerates life, so it cannot show (§8 F-2).

### D-4. The upkeep rule and its invariants

**Change** (`upkeep_entity`, the "Passive Vitals & Shield delay" block): the counter increment moves from step 4
to the **top** of the block, and the life line is gated:

```text
ticks_since_last_damage := saturating_add(ticks_since_last_damage, 1)
if ticks_since_last_damage >= arch.hp_regen_delay_ticks:
    hp := min(hp + arch.hp_regen_per_tick, max_hp)
energy regeneration, then mode drain              (unchanged, same order)
shields: as today, reading the same counter       (unchanged)
```

**Why moving the increment changes nothing.** Between its old and new position the block reads only `hp`,
`energy`, `active_mode` and the archetype; nothing reads or writes the counter there. So the shield rule sees the
same value, and with delay 0 the life rule is always true: the result is bit-identical for every entity. WP1 proves
it on the whole matrix and the goldens.

**Invariants** (added to the module's `//!` "Invariants" list and to `combat.rs` invariant 4):
1. The counter rises by exactly one per tick per living entity, before any regeneration reads it, and is reset to 0
   only by `Entity::apply_damage`, for damage to shields or life.
2. Life regenerates on a tick if and only if the counter is at least `hp_regen_delay_ticks`. A delay of d
   regenerates first on the d-th tick after the damage; 0 means every tick.
3. Shields regenerate on a tick if and only if the counter is at least `shield_regen_delay_ticks` and the entity is
   completed (unchanged).
4. Regeneration never exceeds `max_hp` or `max_shields` and never touches a dead entity (unchanged).
5. A new entity's counter is `u16::MAX` and so satisfies every delay (unchanged from WP9).

`upkeep_entity` stays under 100 lines (C8; it gains two lines). The implementer may extract the vitals block into a
private `fn regenerate_vitals(entity, arch)` for readability if it keeps the order above.

**Docs to update in code:** `UnitArchetype::hp_regen_per_tick` ("active once `hp_regen_delay_ticks` ticks have
passed without damage"), `Entity::ticks_since_last_damage` (both delays read it), `combat.rs` module invariant 4
("Shield and life regeneration wait for the archetype's catalog delays …"), and the `upkeep.rs` module doc.

### D-5. Values and rounding

As the audit (§2.3 there): Faster s = Normal s ÷ 1.4; ticks = Faster s × 25, nearest, halves up; per-tick rates =
Normal × 1.4 ÷ 25. So the delay is **179** (178.57) and the rate **0.112**, the same pair WP10 and WP11 gave the
shields. The rate is authored as `0.112` and converted with `SimScalar::from_f32_lossy`, like every other rate.

### D-6. One cause per commit, rules before data

Following combat alignment D-1:
1. **WP1 (mechanism) changes no result**: every archetype reads 0. Proved bit-identical (D-8). If anything moves,
   stop: the rule is not latent.
2. **WP2 (the delay) moves only `jet_jumper`.** Cause: the game's `LifeRegenDelay`.
3. **WP3 (the rate) after WP2**, never before. Cause: Normal-time value copied unconverted (class B, Q0039). Order
   matters: the audit showed that the rate without the delay moves the catalog away from the game (§2.4).
4. Each data commit contains the catalog edit, any expectation it causes, the doc cell it corrects, its evidence
   section and its measurement. Its message names Q0049 or Q0039 (R1), old and new value, the source line, and
   every moved expectation.
5. No expectation is widened without a measured reason.

### D-7. Determinism and checksums

- Integer `u16` comparison and existing fixed-point arithmetic only; no float enters state (C7).
- **No new entity state.** The counter already exists and is hashed. The catalog is not hashed. So
  `CHECKSUM_SCHEMA_VERSION` stays **10** (`checksum.rs`), and no `checksum/encoding.rs` change is needed.
- **No golden should move in any commit.** WP1: delay 0 everywhere, so the result is identical (D-4). WP2 and WP3:
  no golden replay (`golden_replay.ron`, `golden_replay_orders.ron`) spawns or trains `jet_jumper` (§2.2), and the
  sandbox checksum (`sim_fixtures` `DEFAULT_SANDBOX_CHECKSUM`) is of the spawned state at full life.
- **Re-record rule.** If a golden or the sandbox checksum moves in WP1: stop, the proof failed. If one moves in WP2
  or WP3: trace it to a reaper (the trace tool or a checkpoint diff), name it in the commit, and only then
  re-record on Linux in the same commit, reproducing it in debug, release and `wasm32-wasip1` (`cargo gate package`
  runs the golden replays in release and on `wasm32-wasip1` for simulation changes). An unexplained move stops the
  package. CI's determinism jobs re-verify every target; a red job stops lane B until it is explained.

### D-8. Measurement

**Tool** (combat alignment D-5; always release, in the background, logs under `~/scratch/life_regen/`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name>
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --all-battles --seeds 16 --jobs 8 --out ~/scratch/life_regen/<step>/matrix \
    --compare ~/scratch/life_regen/<previous step>/matrix/matrix.csv
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/life_regen/<step>/b33
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 22b_reaper_vs_stalker --seeds 16 --out ~/scratch/life_regen/<step>/b22b    # likewise 22a
```

`<step>` is `base` (post-WP17 `main`, before WP1; `--compare` omitted, or against WP17's last measurement to
confirm the branch reproduces it), then `wp1`, `wp2`, `wp3`. For 22a and 22b, the per-tick
`blue_health_regained` column (as alignment evidence §10 used for shields) shows when the reaper regains life.

**Expectations and stop rules.**

| Step | Whole matrix, 20 seeds | Battles 22a and 22b | Battle 33, 68 seeds | Goldens | Stop if |
|---|---|---|---|---|---|
| WP1 | bit-identical (all 1000 rows) | identical | bit-identical | unchanged | anything moves |
| WP2 | only 22a and 22b may move | **22b:** stalker win on all 20 seeds, end tick at most the baseline's (the reaper no longer heals between the stalker's shots). **22a:** reaper win, both workers dead on all 20 seeds; the reaper's lead life falls by the life it regains in combat today (*expected* a few points: 0.08 per tick from its first hit to the end). Per-tick `blue_health_regained` 0 after the first damage on every seed | bit-identical | unchanged | any other battle moves; 22a or 22b changes winner or fails its expectation on any of the 20 seeds |
| WP3 | *expected* bit-identical: both battles end at a kill, well inside 179 ticks of the reaper's last hit, so the faster rate never applies | as WP2 | bit-identical | unchanged | 22b stops being a stalker win on any seed (the delay is not doing its job); any battle other than 22a and 22b moves |

These are expectations, not measurements: no planning probe could be run. A stop means report, not widen.

**Battle 33.** It has no `jet_jumper`, so its comment in `matchup_matrix.ron` does not change (combat alignment
D-6 applies only to commits that move it).

**Evidence.** New file `docs/evidence/<date WP1 starts>-life-regeneration-delay.md`, created by WP1 with "How to
reproduce" (pointing to the alignment evidence's section of that name), the baseline, and WP1's section; WP2 and
WP3 append theirs in their own commits. Each section gives: the commit and change; the matrix against the step
before ("bit-identical", or the moved battles with survivors, end tick and lead life, means over 20 seeds, and
failures of record and over all seeds); battle 33 (wins / losses / unresolved, survivors mean and SD, the seeds of
record); 22a and 22b per-tick regained life; the goldens.

### D-9. Documents

| Document | Change | WP |
|---|---|---|
| `docs/questions/open/0049-…md` | default updated to (b) (D-1); then `## Resolution`, `git mv` to `closed/` | WP0; WP4 |
| `docs/07-sc2-patch-5-0-14-roster-and-faction-mapping.md` §3.1 row 4 (`jet_jumper`) | "Combat regenerative fuel cell." → "Regenerates 2.8 life per second after 179 ticks (7.16 s) without damage." | WP2 (delay); WP3 adjusts the rate if WP2 wrote 2.0 |
| `docs/03-sc2-mechanics-reference.md` §1.1, Life row | add "the Reaper regenerates `2.8 HP/sec` **only after `10 seconds` without taking damage** (Zerg: no delay)"; Engine column adds `hp_regen_delay_ticks`, `ticks_since_last_damage` | WP4 |
| `docs/08-sc2-structures-tech-tree-and-upgrades.md` | none (no row states the Reaper's regeneration; checked) | — |
| `docs/evidence/2026-11-04-catalog-time-audit.md` | the R1 row's note and §5 R1: "applied by plan `2026-11-05-life-regeneration-delay` WP3 (`<hash>`) after its delay (WP2)" | WP4 |
| `docs/evidence/2026-11-04-combat-sc2-alignment.md` §13.11 and §13.21 | one line each: R1 applied later by this plan, with the commit and a link to its evidence | WP4 |
| `DESIGN.md` | **not edited here.** The Combat row is rewritten by combat alignment WP13 step 5 at the end of lane B; WP4 hands it the sentence "Life and shields regenerate after per-archetype catalog delays (`hp_regen_delay_ticks`, `shield_regen_delay_ticks`)". No subsystem, phase or tick-rate statement of `DESIGN.md` changes (§3 already says phase 3 covers "regeneration"). | — |
| `docs/plans/README.md` | this plan's row: `Proposed` → `Active` (WP1) → `Done` with commits (WP4) | WP1, WP4 |

---

## 4. File ownership

| File | WP |
|---|---|
| `crates/sim_core/src/catalog.rs` (field in two definitions, `UnitArchetype`, both conversions, docs) | WP1 |
| `crates/sim_core/src/catalog/validation.rs` (rule and unit tests) | WP1 |
| `crates/sim_core/src/world/upkeep.rs`, `crates/sim_core/src/entity.rs` (doc only), `crates/sim_core/src/combat.rs` (module invariant 4 only) | WP1 |
| `crates/sim_core/tests/damage_and_shields_tests.rs` | WP1, WP2 |
| `crates/sim_core/tests/attack_move_simulation_tests.rs` (its `UnitArchetype` literal gains `hp_regen_delay_ticks: 0`) | WP1 |
| `assets/catalog/units.ron` (`jet_jumper` only) | WP2, WP3 |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | only if WP2 or WP3 moves an expectation (not expected) |
| new `docs/evidence/<date>-life-regeneration-delay.md` | WP1 creates; WP2, WP3 append |
| docs of D-9 | as listed |

RON fixtures that spell out archetypes (`sim_fixtures/tests/trace_tests.rs`, `sim_core/tests/weapon_cadence_tests.rs`)
need no edit: the field defaults to 0. Rust struct literals of `UnitArchetype`: `catalog.rs` (two constructors),
`damage_and_shields_tests.rs` (`dummy_target_archetype`), `attack_move_simulation_tests.rs`; the others use `..base`
or `clone()` (`world/snapshot.rs`, `client/src/battle_input/command_mode.rs`). The compiler finds any missed.

---

## 5. Work packages

Work in a worktree with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name>`; check
`df -h / ~/scratch` first; outputs under `~/scratch/life_regen/`; remove the target directory after merge
(charter §7). Write each failing test first and paste its red run into the hand-back. Every commit names its WP and
question. No `unwrap`; `expect` only with `invariant:`; no catalog identity in production Rust (C5). Tests may name
identities, but new mechanism tests use scratch archetypes, and the shipped-data test finds its archetype by
property. Never run a long command in the foreground. Check `docs/questions/open/` before the last commit.

**Gate (charter §4.3).** `cargo gate quick` is optional while working. **`cargo gate package` once**, after
merging `main` into the branch and before the final commit; its stamp goes in the hand-back. Documentation-only
commits need nothing beyond it. The orchestrator runs `cargo gate merge <branch>`.

### WP0: Q0049's default

1. Edit Q0049's `Default until answered` to: "option (b), under the standing rule Q0008 (charter §8): plan
   [`2026-11-05-life-regeneration-delay`](../../plans/2026-11-05-life-regeneration-delay.md) adds the delay and
   then applies R1." Do not write under `## Answer` (it is the owner's).
2. Commit `docs(questions): Q0049 follows option (b) under Q0008 (life regeneration delay WP0)`.

### WP1: Code: the life-regeneration delay comes from the catalog

1. **Failing tests first.**
   - `catalog/validation.rs` unit tests (beside the shield-delay tests, reusing a variant of
     `assert_rejects_delay` that takes the field name):
     - `a_unit_without_life_regeneration_rejects_a_life_regeneration_delay`: shipped `units.ron`, every unit
       validates; pick a unit with `hp_regen_per_tick == 0.0`, set `hp_regen_delay_ticks = 10`, and get the typed
       error naming `hp_regen_delay_ticks`. Pick a unit with a positive rate, set 10, and it validates.
     - `a_structure_without_life_regeneration_rejects_a_life_regeneration_delay`: the same on `structures.ron`.
     - `an_omitted_life_regeneration_delay_reads_as_zero`: every shipped unit and structure parses with 0 (before
       WP2).
     These do not compile before the field exists. With the field and no rule, the first two fail.
   - `damage_and_shields_tests.rs` (charter §4 subject "shield and armour spillover"; regeneration has lived here
     since WP9). Add a helper `assert_life_regenerates_after_catalog_delay(world, unit_type, life_damage)`
     mirroring the shield helper: it reads `hp_regen_delay_ticks` from the archetype, applies life damage through
     `apply_damage`, asserts life holds for `delay - 1` empty ticks and rises on tick `delay`. Tests on scratch
     archetypes (`dummy_target_archetype` with `max_shields` and the shield fields as the case needs, a life rate
     of 1):
     - `a_scratch_archetype_with_a_10_tick_life_delay_regenerates_from_the_10th_tick_after_damage`: red before the
       upkeep change (life rises on tick 1).
     - `life_regeneration_without_a_delay_runs_on_the_tick_after_damage`: delay 0, life rises on tick 1. A guard
       for the Zerg: green before and after.
     - `shield_damage_alone_restarts_the_life_regeneration_delay` (D-3): shields 50, life delay 10. Damage life,
       step 5 ticks, damage shields only, then life holds for 9 more ticks and rises on the 10th after the shield
       hit. Red before the change.
     - `a_unit_never_damaged_regenerates_life_at_once`: delay 10, spawn, lower `hp` directly (no `apply_damage`),
       step one tick, and life rises (counter `u16::MAX`, invariant 5). Green before and after.
     - Update the module doc's list (item 6 becomes "Shield and life regeneration wait for the archetype's catalog
       delays …").
2. **Implement** D-2 (field, defaults, conversions in both constructors, `///` docs), the validation of D-2, the
   upkeep change of D-4, and the code docs of D-4. Add `hp_regen_delay_ticks: 0` to the two test literals.
3. **Proof (D-8, row WP1).** Base and WP1 matrices bit-identical; battle 33 bit-identical; both goldens and the
   sandbox checksum pass unchanged. Create the evidence file with the baseline and this section. Set this plan's
   row to `Active`.
4. Commit `feat(sim_core): life regeneration waits for a per-archetype catalog delay, 0 everywhere (life
   regeneration delay WP1, Q0049)`. The message states that no result or checksum moved and why (D-4, D-7).

### WP2: Data: the Reaper analogue's delay

1. **Failing test first**, in `damage_and_shields_tests.rs`:
   `test_life_regeneration_waits_for_the_catalog_delay`. It finds the shipped archetype with the largest
   `hp_regen_delay_ticks` (by property, not by slug, so a later rename cannot break it), asserts that delay is
   positive, and runs `assert_life_regenerates_after_catalog_delay` on it with 20 life damage. Red before the data
   edit: no shipped archetype has a delay.
2. Edit `jet_jumper` in `units.ron`: add `hp_regen_delay_ticks: 179,` after `hp_regen_per_tick`.
3. Measure (D-8, row WP2). Re-record nothing unless a stop rule allows it; write the section (matrix, 22a and 22b
   per-tick regained life before and after, battle 33, goldens).
4. Doc 07 row 4 (D-9).
5. Commit `data(catalog): the Reaper analogue regenerates life only after 179 ticks without damage (life
   regeneration delay WP2, Q0049)`, citing `void` UnitData:1475, 10 Normal s → 178.57 → 179, and the moved
   battles.

### WP3: Data: audit R1

1. No new test: the WP2 test reads the rate from the archetype and keeps passing. This is a data value with an
   audited reference, measured (D-6 item 4).
2. Edit `jet_jumper` `hp_regen_per_tick: 0.08 → 0.112`.
3. Measure (D-8, row WP3). The per-tick 22a and 22b traces should show no regained life inside the fights. To show
   the rate itself, record the WP2 test's per-tick gain (0.112 per tick after the 179th tick) in the section.
4. Doc 07 row 4: the rate, if WP2 wrote the old one.
5. Commit `data(catalog): R1, the Reaper analogue's life regeneration 0.08 -> 0.112 per tick (life regeneration
   delay WP3, Q0039)`, citing `void` UnitData:1476, 2 × 1.4 ÷ 25, audit group R1, and "applied after its delay
   (WP2), which the audit's §7 measurement required".

### WP4: Bookkeeping

1. Doc 03 §1.1, the audit's R1 notes, and the alignment evidence §13.11 and §13.21 pointers (D-9).
2. Q0049: add `## Resolution`: "Carried out as option (b) under the standing rule Q0008 (charter §8), without a
   separate answer: per-archetype `hp_regen_delay_ticks` (WP1 `<hash>`), `jet_jumper` 179 ticks (WP2 `<hash>`),
   R1 0.112 (WP3 `<hash>`); measurement in `<evidence link>`. The owner may reopen this to choose (a) or (c): a
   delay of 0 and a rate of 0.08 restore the old behaviour." Then `git mv` it to `closed/`.
3. Hand-off to combat alignment WP13, in the hand-back and as a line in this plan's README row: Q0039's resolution
   lists R1 as applied by this plan's WP3; the `DESIGN.md` Combat row gets the sentence of D-9.
4. This plan's row: `Done` with the WP commits.
5. Merge `main`, run `cargo gate package` once, and commit `docs: life regeneration delay done; Q0049 closed (life
   regeneration delay WP4)`.

---

## 6. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| WP17 not merged when this starts | conflicts in `catalog.rs`, `validation.rs`, `units.ron`; baseline moves | start after WP17 (§1); the swap rule if WP17 stalls |
| Moving the counter increment changes a result | WP1 not latent | D-4's argument; D-8 stop rule; bit-identical proof on matrix, battle 33, goldens, sandbox |
| The shared counter is not the game's rule | a future shielded archetype with a life delay behaves differently from the game | latent on shipped data (§2.2); pinned by a test; F-1 |
| R1 shows nowhere in the matrix | evidence of R1 rests on tests | expected (D-8); per-tick traces and the WP2 test record the rate |
| Battle 22a's reaper ends with less life | fixture claim weaker | its expectation has no hull range; measured; a winner change stops the package |
| A golden or client fixture contains a reaper not found by the scan | an unexpected checksum move | D-7 re-record rule: trace, explain, then re-record; else stop |
| Determinism on other targets | red CI job | integer comparison only; CI re-verifies; red stops lane B |

---

## 7. Determinism summary

No float, no new state, no schema bump (stays 10), no golden expected to move in any commit. Checks: `cargo gate
package` (golden replays in release and `wasm32-wasip1`), then CI's determinism jobs on Linux, Windows, macOS ARM64
and `wasm32-wasip1`.

---

## 8. Out of scope and follow-ups

Recorded here so they can be brought back (charter §8, Q0002):

- **F-1: a counter per vital.** Separate life, shield and energy timers, if a source shows the game keeps them
  apart and an archetype needs it (D-3).
- **F-2: does the Stimpack life cost restart regeneration delays?** Not researched; latent (D-3).
- **F-3: `EnergyRegenDelay`.** The game has one; no shipped value sets it (not checked per unit; the audit lists
  energy rates only).
- **F-4: life regeneration of unfinished structures.** Today life regenerates whether or not `is_completed`
  (shields wait for completion). The game's rule for Zerg structures under construction was not researched; unchanged.
- **F-5: the Roach's faster burrowed regeneration** (audit R2 note) and Tunneling Claws (doc 08): not modelled.
- **F-6: the shield field's `Option`.** Making `shield_regen_delay_ticks` a `u16` like this field would unify the
  two, but it couples to `max_shields` validation and WP9's semantics. Not changed.
- Healing (Medivac analogue, repair) and the Reaper's other abilities (KD8 Charge) are unrelated and not touched.
