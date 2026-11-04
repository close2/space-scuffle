# Blueprint: Combat Rules and Catalog Times Aligned With StarCraft II (Q0035 to Q0041)

This plan carries out the owner's answers to the battle 33 follow-up questions. It changes two weapon rules in
`sim_core` (a backswing no longer delays the next attack; a damage point of 0 no longer adds a tick to the cycle),
moves the zealot (`psi_grey`) and the marine (`pulse_bot`) to StarCraft II's values, converts the values that were
copied in Normal time, moves the hard-coded shield-regeneration delay into the catalog, audits every time and speed
value of the catalog against the 5.0.14 game data, and records the follow-ups the owner asked for. A last,
low-priority package plans StarCraft II's scan-range rule.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-04 against `main` at `6a3815f`.
Line numbers drift; search for the quoted symbol. Claims that could not be confirmed while planning are marked
**verify**. Numbers in §5 come from a planning probe on scratch catalogs, not from the shipped game.

**Owner decisions** (answered 2026-11-04 in commit `9161cdb`; evidence in
[`docs/evidence/2026-11-03-battle-33-investigation.md`](../evidence/2026-11-03-battle-33-investigation.md)):

| Question | Answer | Where in this plan |
|---|---|---|
| [Q0035](../questions/open/0035-melee-contact-rules.md) contact rules | Option 1: no change now; record a follow-up for future investigation | WP13 step 1, §8 F-1 |
| [Q0036](../questions/open/0036-zealot-weapon-timing.md) zealot weapon | Option 2: `damage_point: 0`, `range: 0.1`, `backswing: 9`, `period: 21`; clean up the "damage point 0 fires every period + 1" quirk if appropriate | D-3, WP2, WP4 |
| [Q0037](../questions/open/0037-scan-ranges.md) scan ranges | Option 3, low priority: a plan for StarCraft II's rule across the catalog, mechanism and whole matrix first | WP14 (last) |
| [Q0038](../questions/open/0038-marine-backswing-gate.md) backswing | Option 2: change the rule first, then marine `damage_point: 1`, `backswing: 13` | D-2, WP1, WP6 |
| [Q0039](../questions/open/0039-normal-time-values.md) Normal-time values | Option 3: convert the four values and audit every time and speed value, with a whole-matrix measurement | D-4, D-5, WP3, WP7 to WP12 |
| [Q0040](../questions/open/0040-zealot-attributes.md) zealot attributes | Option 2: `Light, Biological` | WP5 |
| [Q0041](../questions/open/0041-battle-33-expectation-under-jitter.md) battle 33 under jitter | Option 2: document the ≈90 % marine win in the fixture, with a pointer to the evidence | WP0 step 5 |
| [Q0007](../questions/open/0007-combat-baselines.md) combat baselines | "Q0007 may now be closed" | WP13 step 1 |

New question filed by this plan: [Q0044](../questions/open/0044-battle-33-expectation-after-alignment.md) (how to
record battle 33's expectation once the corrections make it a close fight; default: re-record to the seeds of record
and state the distribution in the claim).

---

## 1. Summary

| WP | Title | Changes expected results? | Size | Depends on | Lane |
|---|---|---|---|---|---|
| WP0 | Measurement tool: whole-matrix mode, phase-independent release inference; baseline; Q0041 comment | no | M (1 day) | none | A |
| WP1 | Rule: a backswing locks orders but never delays the next attack | no (proved latent) | S-M (half a day to a day) | none (measure after WP0) | B |
| WP2 | Rule: a damage point of 0 releases on the tick the swing starts | yes: 6 identities with damage point 0 (battles 9a, 10, 12a, 12b, 17, 18, 26 by the probe); goldens possibly | M (1 day) | WP0, WP1 | B |
| WP3 | Audit research: every time and speed value against the 5.0.14 game data; all-corrections scratch measurement | no (documentation) | L (2 to 3 days) | none; its measurement step after WP2 | C |
| WP4 | Data: zealot weapon (Q0036) | yes: battles 7, 8, 23a, 23b, 25a, 25b, 33; both golden replays | S (half a day) | WP2 | B |
| WP5 | Data: zealot attributes `Light, Biological` (Q0040) | no (inert, verified) | XS (1 to 2 h) | WP4 | B |
| WP6 | Data: marine damage point 1, backswing 13 (Q0038) | yes: battle 33 and marine battles slightly; both golden replays | S | WP1, WP5 | B |
| WP7 | Data: marine speed 0.126 per tick (Q0039) | yes: marine battles; both golden replays | S | WP6 | B |
| WP8 | Data: Stimpack duration 268 ticks (Q0039) | yes: `golden_replay.ron` (stim at tick 260) | S | WP7, WP3 (marauder source) | B |
| WP9 | Code: shield-regeneration delay read from the catalog (value 250 kept) | no behaviour change (checksums only if the counter's start value changes) | M (1 day) | WP8 | B |
| WP10 | Data: shield regeneration 0.08 → 0.112 per tick, every shielded archetype (Q0039) | yes: Protoss battles that last past the delay; goldens | S | WP9, WP3 | B |
| WP11 | Data: shield-regeneration delay 250 → 179 ticks (Q0039) | yes: as WP10 | S | WP10 | B |
| WP12 | Data: the audit's remaining corrections, one field group and cause per commit | yes, per group | M-L (depends on WP3) | WP11, WP3 | B |
| WP13 | Bookkeeping: questions closed as each lands; `DESIGN.md`; docs 03 and 07; plan index | no | S, incremental | each WP | D |
| WP14 | Low priority: StarCraft II scan ranges (Q0037): mechanism, reference table, whole matrix, then data | yes, if applied | M-L (2 to 3 days) | WP12 | E (last) |

**Lanes and concurrency (at most three gates at once).** Lanes A, B and C start together: WP0, WP1 and the research
of WP3. Lane B is a chain, one gate at a time, because every package from WP4 on edits `assets/catalog/units.ron`
and most edit the golden fixtures: WP1 → WP2 → WP4 → WP5 → WP6 → WP7 → WP8 → WP9 → WP10 → WP11 → WP12. WP1's
implementation may start before WP0 lands, but WP1 merges after WP0 so that its "no change" proof uses the tool.
WP3's research needs no gate; its measurement step (§D-5, step 5) waits for WP2. Lane D (WP13) is documentation
and runs beside whatever has just merged. Lane E (WP14) starts only after WP12, and only when the owner's low
priority allows. One agent can do WP4 to WP8 in one session, one commit each.

---

## 2. What is known

### 2.1 The weapon state machine (`crates/sim_core/src/combat.rs`)

`step_entity_weapon` runs once per unit per tick, from `combat_resolution.rs`, with the weapon's period already
resolved for upgrades and Stimpack (`resolve_attack_period` in `weapon_timing.rs`). Per tick:

1. `weapon_cooldown_remaining` drops by one. A `Cooldown` phase whose cooldown reached 0 becomes `Ready` at once.
2. `Ready`: if a target is in band and inside the firing arc and the cooldown is 0, the phase becomes
   `Windup { remaining_ticks: damage_point_ticks }` (`step_ready_weapon`). **No shot this tick.**
3. `Windup`: `remaining` drops by one (saturating). At 0 the swing releases if the target is still in band plus
   `RELEASE_RANGE_SLOP`: cooldown = `period − damage_point`, phase `Backswing { backswing_ticks }` or `Cooldown`
   (`step_windup`).
4. `Backswing`: `remaining` drops by one; at 0 the phase becomes `Ready` (cooldown 0) or `Cooldown`. **The next swing
   cannot start until the backswing has ended, and not on the tick it ends.**

Read off these rules (and confirmed by the investigation, §5.3 of the evidence):

- A swing that starts on tick T releases on T + max(damage point, 1). The cooldown it charges runs out on
  T + max(dp, 1) + (period − dp). So the cycle is `period` for damage point ≥ 1 and **`period + 1` for damage
  point 0**: the cadence quirk of Q0036.
- If backswing ≥ cooldown (period − dp), the next swing starts one tick after the backswing ends, so the cycle is
  dp + backswing + 1. That is the **backswing gate** of Q0038: stimmed marines with backswing 13 (period 10, dp 1)
  fire every 15 ticks instead of 10, which broke battle 25b.

What the backswing does besides gating (unchanged by this plan): it marks the unit as attacking (`snapshot.rs`,
`locomotion.rs` animation); it anchors the unit against being pushed (`is_anchored_in_combat` in `locomotion.rs`);
a player `Move` or `Stop` cancels it into `Cooldown` with the cooldown still running (`Entity::handle_interrupting_move`
and `commit_order` in `entity.rs`; this is stutter-step, `stutter_step_tests.rs`). It does **not** hold the unit in
place: only a wind-up does (`locomotion.rs`, the `Windup` check before the order dispatch).

### 2.2 Who is affected (planning probe of the shipped catalog)

A scan of every weapon in `units.ron` and `structures.ron` (57 weapon blocks) found:

- **No weapon has a backswing reaching its cooldown** (backswing ≥ period − damage point), nor two thirds of it (the
  Stimpack case). The backswing rule of WP1 is therefore latent on shipped data, as Q0038 says; WP1 proves it.
- **Six identities already have damage point 0**, and so fire every `period + 1` today: `flying_saucer` (both
  weapons), `napalm_wheeler`, `sky_bastion`, `prism_weaver`, `tripod_annihilator`, `prismatic_saucer` (both
  weapons). The cadence fix of WP2 is **not** latent: it makes each of them one tick faster per cycle, which for the
  void-ray analogue (`prismatic_saucer`, period 9) is 11 % more damage per second. Their authored periods are their
  StarCraft II cooldowns in ticks (for example the colossus 1.07 Faster s ≈ 27 ticks, the hellion 1.79 s ≈ 45; WP3
  verifies each), so the fix brings them to the reference, it does not move them away from it.

### 2.3 Values the owner named (reference from evidence §1, in our ticks at 25 per Faster second)

| Value | Today | Reference | Rounded for the catalog | Source |
|---|---|---|---|---|
| Zealot damage point | 4 | 0 | 0 | evidence §1.5 (`liberty` WeaponData:150) |
| Zealot period | 22 | 21.43 | 21 (D-3) | evidence §1.5 |
| Zealot range | 0.3 | 0.1 | 0.1 | evidence §1.5 |
| Zealot backswing | 6 | 8.93 | 9 | evidence §1.5 (`core` default) |
| Zealot attributes | Light, Psionic (`bits: 17`) | Light, Biological | `bits: 5` | evidence §1.5 |
| Marine damage point | 2 | 0.89 | 1 | evidence §1.4 |
| Marine backswing | 4 | 13.39 | 13 | evidence §1.4 |
| Marine speed | 0.09 per tick | 0.126 | 0.126 (acceleration and deceleration likewise; both parsed, unused) | evidence §1.4 |
| Shield regeneration | 0.08 per tick (all 29 shielded archetypes) | 0.112 | 0.112 | evidence §1.5; the other 28 confirmed by WP3 |
| Shield-regeneration delay | 250, hard-coded in `world/upkeep.rs` | 178.57 | 179 | evidence §1.5 (`ShieldRegenDelay 10` Normal) |
| Stimpack duration | 250 | 267.86 | 268 | evidence §1.6 (`liberty` BehaviorData:680) |

Rounding rule (used throughout, as in the investigation): nearest whole tick, halves up. Fractional periods are out
of scope (§8 F-3).

### 2.4 Tests and fixtures that see these values

- **Golden replays** (`crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`) spawn
  `pulse_bot`, `psi_grey`, `rhino_beetle`, `railgun_crawler` and (orders fixture) `flying_saucer`, and the first uses
  `UseStim` at tick 260. Every marine, zealot, Stimpack or shield change re-records them; WP2 may re-record the
  orders fixture (its spotter has a damage-point-0 weapon; **verify** whether it fires).
- **Matrix** (`crates/sim_core/tests/fixtures/matchup_matrix.ron`, 50 battles, 4 seeds of record, jitter 0.12).
- **Tests that restate catalog values:** `stutter_step_tests.rs` asserts the marine's wind-up as the literal
  `remaining_ticks: 2`; `damage_and_shields_tests.rs` asserts the delay as the literal 250;
  `crates/sim_fixtures/tests/trace_tests.rs` reads timings from the catalog, but
  `a_swing_winding_up_when_another_attacker_kills_the_target_is_cancelled` needs a melee unit **with** a wind-up,
  which the zealot no longer has after WP4; `sc2_battle_benchmarks_tests.rs` asserts zealot-against-marine outcomes.
- **The trace tool's swing inference** (`crates/sim_fixtures/src/trace/swing.rs`) mirrors the phase pattern of §2.1.
  After WP1 and WP2 that pattern changes (a damage-point-0 swing goes `Ready` → `Backswing` with no `Windup` in
  between; a backswing can be cut short by the next swing), so the inference must not depend on it (WP0).

---

## 3. Design decisions

### D-1. Order: rules first, proved; then one cause per commit

1. **Rule changes come before the data that needs them**, each in its own package with focused tests written first
   and seen red.
2. **WP1 (backswing) changes no result on shipped data.** Its commit must leave both golden replays and the whole
   matrix bit-identical (proved with WP0's `--compare`). If anything moves, the rule is not latent and WP1 stops and
   reports, because then the rule commit would carry two causes.
3. **WP2 (cadence) moves only the six damage-point-0 identities.** Its re-recorded expectations name them.
4. **Data steps change one identity and field group at a time**, in the order of the table in §1, so every
   checksum and expectation change has exactly one cause. Each data commit contains: the catalog edit, the
   re-recorded expectations it causes (matrix entries, golden checksums, any other golden), the doc 07 roster cell
   it corrects, and the measurement it was checked against (§D-5). Its message names the question, the old and new
   values, the source line in the game data, and every expectation it moved and why.
5. **Tests that restate a catalog value are decoupled first**, in a test-only commit that passes before and after
   (they read the archetype instead of a literal). So the data commit changes only outcomes, never the arithmetic of
   a test.
6. **No expectation is ever widened to make a step pass without a measured reason.** The matrix and golden updates
   are re-recordings of measured outcomes, explained per battle.

### D-2. The backswing rule (Q0038)

**Rule.** A backswing is a lock on orders, not a gate on the weapon. When the weapon's cooldown has run out while
the backswing is still running, the weapon steps exactly as a `Ready` weapon would on that tick: if that starts a
swing (or turns towards a target), the backswing ends there; if nothing starts (no target, out of band), the
backswing runs on to its end as before.

**Source.** StarCraft II: the backswing is the follow-through after the damage point that a new order cancels, while
the cooldown runs in parallel from the start of the attack (evidence §1.8, Liquipedia
[Damage Point](https://liquipedia.net/starcraft2/Damage_Point); doc 03 §1.3 already draws the cooldown "in parallel
with Backswing"). Stimmed marines fire every 0.41 s (Liquipedia, Cooldown) although their backswing (0.5357 Faster s)
is longer: the backswing cannot be gating the next attack. Whether the game scales the backswing by the attack-speed
multiplier was **not found** (evidence §5.7); with this rule it does not matter for the rate of fire.

**Specification** (`step_entity_weapon`, the `Backswing` arm):

```text
remaining' = remaining − 1 (saturating)
if cooldown == 0:
    phase := Ready; step it as Ready (step_ready_weapon) with this tick's target
    if the phase is no longer Ready (a swing started, released, or the weapon turns): return that tick's result
    phase := if remaining' == 0 { Ready } else { Backswing { remaining' } }    // the lock continues
else:
    phase := if remaining' == 0 { Cooldown } else { Backswing { remaining' } }
```

The `Ready` step on the backswing's last tick is new also when backswing == cooldown, which today costs a tick (§2.1);
no shipped weapon is in that case (§2.2).

**Unchanged on purpose.** The anchoring, the attack presentation, and the cancellation by `Move` or `Stop` stay as
they are. The backswing still does **not** hold the unit in place. Whether StarCraft II holds an automatically
attacking unit through its backswing (for example an attack-moving unit whose target died) was not researched; it
would move every unit's movement in every battle and needs its own research and plan (§8 F-2). `combat_resolution.rs`
binds the target only during a wind-up; a swing that starts out of a backswing binds as any other.

**Tests first** (WP1).

### D-3. The damage-point-0 cadence (Q0036): fixed

**Decision: fix it**, so that `period_ticks` is the cycle for every damage point.

**Why it is appropriate.**

1. It is an off-by-one in the state machine, not a modelling choice. `step_windup` charges `period − damage point`
   as cooldown so that the cycle equals the period; that bookkeeping assumes the swing releases `damage point` ticks
   after it starts, which is false only for 0 (the release waits for the next `step_windup`).
2. StarCraft II's damage point 0 means the damage lands when the attack starts (evidence §1.8: the zealot's first
   strike "lands the moment it attacks"). Our wind-up of 0 holds the unit still for one tick and can be cancelled,
   which the reference has no counterpart for.
3. Six shipped identities (§2.2) are one tick slower per cycle than their authored period, without any document
   choosing that.
4. With the fix, the period the owner named, 21, gives a cycle of 21 ticks (0.84 Faster s; reference 0.857 s =
   21.43 ticks, error −0.43 tick, −2.0 %). Keeping the quirk, period 21 would give 22 (+0.57 tick, +2.7 %).

**Re-derived zealot period.** Reference 1.2 Normal s ÷ 1.4 = 0.857 Faster s × 25 = 21.43 ticks; the nearest whole
tick is **21**, which is both the owner's value and doc 07's. So `period: 21` stands, and after the fix it is also the
cycle. (The investigation's "damage point 0, period 20" run is the closest measured proxy for this final state: it
fires every 21 ticks with today's code.)

**Specification.** One release routine (factor it out of `step_windup`: band check with `RELEASE_RANGE_SLOP`,
cooldown `period − damage point`, phase `Backswing` or `Cooldown`, return `true`) is called:

- by `step_windup` when `remaining` reaches 0 (as today);
- by the start paths (`step_ready_weapon`, `step_rotating_weapon`, and D-2's backswing path) **when
  `damage_point_ticks == 0`**: the swing starts and releases in the same call, so the function returns `true` on the
  tick it starts. `step_ready_weapon` and `step_rotating_weapon` then return `bool`.

No `Windup { remaining: 0 }` phase is ever created after the fix; a defensive `Windup { remaining: 0 }` arriving from
older state (none exists: the phase is not persisted outside the world) still releases through `step_windup`.

**Determinism.** Integer tick arithmetic only; no new state, no new float. The checksum schema does not change.

### D-4. The shield-regeneration delay moves into the catalog

**Decision.** A per-archetype field `shield_regen_delay_ticks: Option<u16>` in both `UnitArchetypeDef` and the
structure definition, carried into `UnitArchetype` (`catalog.rs`), read by `upkeep_entity` (`world/upkeep.rs`) in
place of the literal 250.

**Why per archetype.** StarCraft II stores `ShieldRegenDelay` per unit (`UnitData`, evidence §1.5); the charter keeps
unit values in data (§3.3) and forbids magic numbers (§1.1). A single constant would be smaller but would repeat the
literal's problem for any unit whose reference differs.

**Validation** (`catalog/validation.rs`, typed errors): an archetype with `max_shields > 0` must set the field; an
archetype without shields must not. `ron` with `#[serde(default)]` keeps older catalogs loading until validation
names the missing field.

**The spawn counter.** `Entity::ticks_since_last_damage` starts at the literal 250 ("starts ready"). With the delay
in data that literal stops meaning anything. Use `u16::MAX` ("never damaged"), which is ready for any authored delay
and equals the value the saturating counter reaches anyway. It changes the hashed value of the counter for every
entity, so it changes golden checksums without changing behaviour; WP9 proves that (matrix bit-identical in
outcomes, every behavioural test unchanged) and says so in the commit message. If the implementer finds a way to keep
the checksums (for example spawning with the archetype's own delay, **verify** that unshielded spawns are not
affected), prefer it.

**Two commits.** WP9 introduces the field with the value 250 everywhere (no behaviour change); WP11 changes the value
to 179 (one cause: the Normal-time conversion).

### D-5. Measurement after every step

**Tool.** `crates/sim_core/examples/matchup_trace.rs` (`sim_fixtures::trace`), extended by WP0 with a whole-matrix
mode. Always a release build:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<wp>
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --all-battles --seeds 16 --out ~/scratch/combat_align/<step>/matrix [--compare ~/scratch/combat_align/<prev>/matrix/matrix.csv]
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/combat_align/<step>/b33
```

Cost on this server (planning probe, release): the whole matrix on 20 seeds takes about 2 to 3 minutes; battle 33 on
68 seeds about 26 s. Run them in the background and read the log.

**What is recorded**, in a new evidence file `docs/evidence/<date WP0 starts>-combat-sc2-alignment.md`, one section per
step, written in the step's commit:

1. The commit, the change, and the question it answers.
2. The matrix against the previous step: the battles whose per-seed outcome moved, with survivors per side, end tick
   and lead-unit hull (means over the 20 seeds) before and after, and expectation failures on the 4 seeds of record
   and on all 20. "Bit-identical" when nothing moved.
3. Battle 33 on 68 seeds: marine wins / zealot wins / unresolved, marine survivors mean and SD, K over the first 5 s
   (the investigation's measure), and the four seeds of record.
4. Golden replays: which fixture changed and the first checkpoint that differs.
5. For data steps with several fields (WP4, WP6), each field alone on a scratch catalog as well, so the evidence keeps
   the per-field effect the investigation reported.

Seeds: the four of record plus 16 (`--seeds 16`) for the matrix, as the investigation's control (§5.7); the 68 seeds
of the investigation for battle 33, so the numbers stay comparable with its §5.3.

### D-6. Battle 33's expectation (Q0041 now, Q0044 later)

- **Q0041, now (WP0).** A RON comment above battle 33 in `matchup_matrix.ron`: on 68 seeds with the matrix's jitter
  the battle is a marine win on about 90 % (61 of 68; 6 zealot wins, 1 unresolved), the seeds of record are among the
  winners, and the evidence is the investigation's §4.2. No expectation, seed or claim text changes in that commit.
- **After the corrections (WP4 on).** The probe (§5) says battle 33 becomes about a 70 % marine win, and one or two
  of its seeds of record stop meeting `blue: (4, 16), red: (0, 0)`. Q0044 asks how to record that. **Default
  (option 1):** each commit that moves battle 33 off its expectation re-records it to the smallest ranges that hold on
  the four seeds of record, updates the comment's distribution to the step's 68-seed measurement, and rewrites the
  claim to describe a close fight. If the owner chooses option 2 or 3 before WP4 starts, WP4 implements that choice
  first, in its own commit.

### D-7. The audit (Q0039 option 3)

**Scope.** Every time and speed value in `assets/catalog/units.ron`, `structures.ron` and `upgrades.ron`, for every
identity with a StarCraft II analogue (`sc2_analogue`; units, structures and upgrades). Fields, by kind:

- Unit vitals and motion: `max_speed`, `acceleration`, `deceleration`, `turn_rate_deg`, `hp_regen_per_tick`,
  `shield_regen_per_tick`, `energy_regen_per_tick`, and the new `shield_regen_delay_ticks`.
- Weapons (units and structures): `period_ticks`, `damage_point_ticks`, `backswing_ticks`, `turret_turn_rate_deg`,
  `projectile_speed`.
- Capabilities and abilities: `overclock.duration_ticks`, `harvester.harvest_rate_ticks`, ability cooldowns, durations
  and cast times, mode `cast_ticks`, `energy_cost_per_second`.
- Production: `build_time_ticks` (units, structures), `research_ticks` and time or speed modifiers in `upgrades.ron`
  (for example `attack_cooldown_delta`, speed bonuses).

`max_projectile_lifetime_ticks` and other values with no StarCraft II counterpart are listed as "no reference".

**Source.** The same as the investigation's WP1: SC2Mapster SC2GameData at commit
[`5444e12f13`](https://github.com/SC2Mapster/SC2GameData/tree/5444e12f13) ("SC2 5.0.14.94342"), layers
`core` → … → `balancemulti`, each value cited by file and line; Liquipedia as the secondary source. Conversion as in
evidence §2.2: Faster = Normal ÷ 1.4; ticks = Faster s × 25; per-tick speeds and rates = Normal value × 1.4 ÷ 25.

**Classification of each value.**

| Class | Meaning | What happens |
|---|---|---|
| A | Matches the reference after rounding | nothing |
| B | Equals the Normal-time value, unconverted (off by the factor 1.4) | corrected: the same cause the owner accepted in Q0039 |
| C | Differs otherwise, and no project document chose it | corrected under the standing rule of Q0008 |
| D | Differs, and a project document chose it deliberately (doc 07, a plan) | listed; one owner question collects them, default no change |
| E | No reference (no analogue, or not modelled) | listed |

Already visible while planning (to be confirmed by WP3): `psi_grey.build_time_ticks` is 950 = 38 s × 25, the
Normal-time build time (reference 27.14 Faster s = 679 ticks; doc 07 says 675). Every shielded archetype has
`shield_regen_per_tick: 0.08` (class B).

**Output.** A new evidence file `docs/evidence/<date>-catalog-time-audit.md`: method, one table per catalog file
(identity, field, ours, editor value and source line, reference in our units, class), the change groups for WP12 (one
field kind and one cause each), and the whole-matrix measurement of a scratch catalog with every class B and C
correction applied (D-5), against the state after WP2. Q0039 asks for that measurement "before any value changes":
the audit's own corrections (WP12) start only after it.

---

## 4. File ownership and order

| File | Owner, in order |
|---|---|
| `crates/sim_fixtures/src/trace/{cli.rs, swing.rs, mod.rs, report.rs}`, new `trace/matrix.rs`, `crates/sim_fixtures/src/matchup.rs`, `crates/sim_fixtures/tests/trace_tests.rs` | WP0; then WP4 (one test restated) |
| `crates/sim_core/examples/matchup_trace.rs` (usage text) | WP0 |
| `crates/sim_core/tests/sc2_who_wins_matchup_matrix_tests.rs` (uses the shared expectation check) | WP0 |
| `crates/sim_core/src/combat.rs` | WP1, WP2, then WP9 (module doc invariant 4 only) |
| `crates/sim_core/src/entity.rs` | WP1 (`WeaponPhase::Backswing` doc), WP9 (spawn counter) |
| new `crates/sim_core/tests/weapon_cadence_tests.rs`; `crates/sim_core/tests/stutter_step_tests.rs` | WP1, WP2 |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs`, `world/upkeep.rs`, `crates/sim_core/tests/damage_and_shields_tests.rs` | WP9 |
| `assets/catalog/units.ron`, `structures.ron`, `upgrades.ron` | WP4 → WP5 → WP6 → WP7 → WP8 → WP9 → WP10 → WP11 → WP12 → WP14 |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP0 (comment), then every data WP that moves a battle |
| `crates/sim_core/tests/fixtures/golden_replay*.ron` | WP2 (if moved), WP4, WP6, WP7, WP8, WP9 (if the counter moves), WP10, WP11, WP12 |
| `docs/07-sc2-patch-5-0-14-roster-and-faction-mapping.md` | the WP whose value a cell describes; WP1 and WP2 for the timing diagram text |
| `docs/03-sc2-mechanics-reference.md` §1.3 | WP1, WP2 |
| new `docs/evidence/<date>-combat-sc2-alignment.md` | WP0 creates it; every later WP appends its section |
| new `docs/evidence/<date>-catalog-time-audit.md` | WP3; WP14 adds the scan-range table |
| `docs/questions/open/00{07,35..41,44}*.md`, `DESIGN.md`, `docs/plans/README.md` (this row) | WP13 |

---

## 5. Measurements taken while planning

Probe catalogs and scripts are throwaways under `~/scratch/plancombat_probe/` (not committed). Every number below can
be regenerated with the committed tool (`matchup_trace --catalog-dir`) and the edits listed. The zealot's new cadence
was approximated with today's code as damage point 0 and **period 20** (fires every 21 ticks, D-3), and the cadence
fix of the other six identities as their period − 1; both proxies differ from the real fixes by the one-tick
wind-up, so WP2 and WP4 re-measure.

### 5.1 Whole matrix, 20 seeds (seeds of record + 16)

Battles not listed were bit-identical to the shipped catalog in every probe. "Mean" is survivors blue / red over 20
seeds, "t" the mean end tick. No probe broke a survivor expectation except battle 33 (failures on the 4 seeds of
record / on all 20); lead-unit hull was not checked by the probe (WP0 adds it), and battle 25b is known to fail its
hull range with marine backswing 13 until WP1 lands (evidence §5.7).

| Battle | Shipped | Cadence fix (6 identities) | Zealot weapon + attributes + regeneration rate | Marine weapon, speed, Stimpack | All of these |
|---|---|---|---|---|---|
| 5c hydra vs 3 marines | 0/2, t78 | same | same | 0/2, t77 | 0/2, t77 |
| 7 zealot vs roach | 1/0, t253 | same | 1/0, t242 | same | 1/0, t242 |
| 8 zealot vs stalker | 1/0, t254 | same | 1/0, t242 | same | 1/0, t242 |
| 9a hellion vs 4 lings | 1/0, t94 | 1/0, t92 | same | same | 1/0, t92 |
| 10 2 hellions vs 8 lings | 2/0, t57 | 2/0, t52 | same | same | 2/0, t52 |
| 12a colossus vs 6 marines | 1/0, t82 | 1/0, t79 | same | 1/0, t85 | 1/0, t82 |
| 12b colossus vs 3 marauders | 0/3, t164 | **0/2**, t164 | same | same | 0/2, t164 |
| 16b muta vs 2 marines | 0/1, t183 | same | same | 0/1, t182 | 0/1, t182 |
| 17 void ray vs muta | 1/0, t202 | 1/0, t182 | same | same | 1/0, t182 |
| 18 void ray vs corruptor | 1/0, t252 | 1/0, t227 | same | same | 1/0, t227 |
| 21 lurker vs 6 marines | 1/0, t99 | same | same | 1/0, t106 | 1/0, t106 |
| 23a zealot vs 6 lings | 0/2.10, t266 | same | 0/2.00 | same | 0/2.00 |
| 23b upgraded zealot vs 6 lings | 1/0, t260 | same | 1/0, t247 | same | 1/0, t247 |
| 25a 2 marines vs zealot | 0/1, t153 | same | 0/1, t147 | same | 0/1, t146 |
| 25b upgraded marines vs zealot | 0/1, t149 | same | 0/1, t142 | same | 0/1, t142 |
| 26 100 lings vs 6 colossi | 0/6, t120 | 0/6, t116 | same | same | 0/6, t116 |
| 32 30 marines vs 15 banelings | 0.85/0.80 | same | same | 0.90/0.90 | 0.90/0.90 |
| 33 24 marines vs 12 zealots | 7.65/0.20, fails 0/2 | same | 4.50/0.65, fails **2**/9 | 6.85/0.35, fails **1**/4 | 4.50/0.70, fails **1**/8 |

### 5.2 Battle 33, 68 seeds

| Probe | Marine wins / zealot wins / unresolved | Marine survivors mean (SD) | Seeds of record (marines – zealots) |
|---|---|---|---|
| Shipped (investigation §4.2) | 61 / 6 / 1 | 7.41 (3.34) | 8–0, 6–0, 9–0, 13–0 |
| Zealot weapon, attributes, regeneration rate | 38 / 13 / 17 | 4.07 (3.15) | 3–0, 6–0, 3–1, 7–0 |
| All of §5.1 | 47 / 15 / 6 | 4.75 (3.47) | 7–0, 0–1, 9–0, 5–0 |

**Expected effect on battle 33.** The zealot's weapon is the large step (about −3.3 marine survivors here; the
investigation measured −2.82 for damage point 0 at the old cadence and −4.96 with the 21-tick cadence, both with
range 0.3). The marine's changes partly offset it. The end state without the regeneration delay, the audit and the
scan ranges is about a 70 % marine win (47 of 68 seeds) with 4.75 marines alive on average. The zealots still need
more of their number in contact than the front holds most of the time (zealots in contact over the whole contact
window: 2.86 on average, against 2.96 today; evidence §3.4 and §3.5), so the corrections narrow the fight without
reversing it. The seeds of record stop meeting the expectation from WP4 on
(Q0044). The reference scan ranges (WP14, low priority) would move it again by
about −3 marines (investigation §5.3).

---

## 6. Work packages

Each package works in its own worktree with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/align-wpN`,
checks `df -h / ~/scratch` first, keeps logs and measurement output under `~/scratch/combat_align/`, merges `main`
before its last commit, and removes its target directory after merge (charter §7). Common gate before each commit:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/align-wpN
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked    # plain: the GPU harness takes the lock per child; never wrap it
```

Documentation-only commits (WP3, WP13) use `cargo fmt --all -- --check` and `cargo test --locked -p workspace_checks`.

Rules for every package: failing test first, with the red run pasted into the agent's report. Every commit names its
WP and question. Every new Rust file begins with `//!` (responsibility, fit, invariants). No `unwrap`; `expect` only
with an `invariant:` message. No catalog identity in production Rust (C5): tests may name identities, and new
mechanism tests use scratch archetypes defined in the test (`ron` strings), so a later rebalance cannot break them. No
function over 100 lines outside tests (C8). Never run a long command in the foreground: background it and read its
log. Check `docs/questions/open/` before each push (Q0044 in particular before WP4).

**Determinism.** Every change here is integer tick arithmetic or catalog data parsed by the existing loaders; no float
enters simulation state, and the checksum schema does not change. Golden checksums are re-recorded on Linux in the
commit that changes them; CI's determinism jobs (Linux, Windows, macOS ARM64, `wasm32-wasip1`, debug and release)
re-verify them. A red determinism job after any step stops the chain until it is explained.

---

### WP0: Measurement tool, baseline, Q0041 comment

**Files (owned):** see §4.

**Steps**

1. **Shared expectation check.** Move `holds`, `within` and the lead-hull reading of
   `sc2_who_wins_matchup_matrix_tests.rs` into `sim_fixtures::matchup` (for example `Expected::holds(&Outcome)` and
   `Outcome::of(&BattleRun)`), so the tool and the matrix test judge a battle with the same code. The matrix test
   calls it. Pure move: the matrix test passes unchanged.
2. **Phase-independent release inference.** Failing tests first in `trace/swing.rs`:
   - `a_swing_that_starts_and_releases_in_one_tick_is_counted` (phases `Ready` → `Cooldown` with the cooldown going
     0 → 21; and `Ready` → `Backswing` likewise): red today (reported as `Idle`).
   - `a_backswing_cut_short_by_the_next_swing_is_a_start` (`Backswing`, cooldown 1 → `Windup`, cooldown 0) and
     `... by an immediate release` (`Backswing`, cooldown 1 → `Backswing`, cooldown 10): the second is red today.
   - The existing cases stay green.

   Then add the weapon cooldown to `UnitSample` and make `infer` take both samples. The rule: a shot was released
   in the tick **iff** `after.cooldown > before.cooldown.saturating_sub(1)` (the cooldown only rises at a release;
   this needs the cooldown charged at release to be at least 1, i.e. a resolved period greater than the damage point,
   which holds for every shipped weapon and Stimpack; WP1 adds the catalog check `period_ticks > damage_point_ticks`).
   Starts and cancellations keep their phase rules, with "released" taken from the cooldown. Update the module doc.
3. **Whole-matrix mode.** Failing test first in `trace_tests.rs`: a scratch fixture with two battles, run with
   `--all-battles --seeds 1`, writes `matrix.csv` with one row per battle and seed and `matrix.md` with the per-battle
   summary; a second run with `--compare <first matrix.csv>` on the same inputs reports "bit-identical", and on a
   scratch catalog with one changed period reports that battle as moved.
   - `matrix.csv` columns: battle, seed, of record (bool), end tick, survivors blue and red, lead hull blue and red
     (empty when dead), expectation met (bool). Values are printed exactly (`SimScalar` hull via its exact decimal
     form), so the comparison is a text comparison.
   - `matrix.md`: per battle the seed count, wins and unresolved, survivor and end-tick means, lead-hull means,
     expectation failures of record and over all seeds; then, with `--compare`, a "moved" table (before and after
     per battle) or the line "bit-identical to <file>".
   - Matrix mode writes no per-tick files (too large for 50 battles); `--battle` keeps today's behaviour. Update
     `USAGE` and the example's doc.
4. **Baseline.** Create the evidence file (§D-5) with section 1: the shipped matrix on 20 seeds and battle 33 on 68
   seeds, at the commit of this WP. Battle 33 must reproduce the investigation's 61 / 6 / 1 and 7.41.
5. **Q0041.** Add the comment of D-6 above battle 33 in `matchup_matrix.ron`, in its own commit ("docs: battle 33 is
   a marine win on about 90 % of jittered seeds (Q0041)").

**Gate.** Common gate. **Changes no expected result.**

---

### WP1: Rule: a backswing never delays the next attack (Q0038, step 1)

**Files (owned):** `combat.rs`, `entity.rs` (doc of `WeaponPhase::Backswing`), `catalog/validation.rs` (one rule),
new `tests/weapon_cadence_tests.rs`, `stutter_step_tests.rs`, docs 03 §1.3 and 07 (timing diagram text).

**Steps**

1. **Test-only decoupling.** `stutter_step_tests.rs` reads the marine's damage point and backswing from the
   archetype instead of the literals (`remaining_ticks: 2`). Green before and after.
2. **Failing tests first**, in a new `weapon_cadence_tests.rs` with a scratch catalog defined in the test (a
   stationary shooter and a stationary target, no shipped identity), recording the ticks on which the shooter
   releases:
   - `a_backswing_longer_than_the_cooldown_does_not_delay_the_next_attack`: period 10, damage point 1, backswing 13
     → releases every 10 ticks. Red today (every 15).
   - `a_backswing_equal_to_the_cooldown_does_not_delay_the_next_attack`: period 15, damage point 1, backswing 14 →
     every 15. Red today (16).
   - `a_stimmed_weapon_fires_at_the_stimmed_period_whatever_its_backswing`: period 15, damage point 1, backswing 13,
     an `overclock` with attack-rate multiplier 1.5, stim applied → every 10 ticks. Red today. (This is battle 25b's
     mechanism, evidence §5.7.)
   - `a_short_backswing_is_unchanged`: period 15, damage point 2, backswing 4 → every 15, and the phase sequence
     tick by tick equals today's. Green before and after (guard).
   - `a_backswing_with_nothing_to_attack_runs_to_its_end`: the target dies on release; the phase stays `Backswing`
     for the full backswing, then `Ready`/`Cooldown` as today. Green before and after (guard of the lock).
   - In `stutter_step_tests.rs`, keep the cancellation test, and add
     `a_move_during_a_long_backswing_still_cancels_it_and_keeps_the_cooldown` with the scratch weapon above.
   - Catalog validation: `a_period_not_longer_than_the_damage_point_is_rejected` (typed error), for WP0's inference
     and because such a weapon has no cooldown to speak of. **Verify** that no shipped weapon violates it (the
     probe of §2.2 found none).
3. Implement D-2. Keep `step_entity_weapon` under 100 lines (C8) by giving the backswing arm its own function.
4. **Proof of no change.** Both golden replays pass unchanged. WP0's tool on the shipped catalog with `--compare`
   against WP0's baseline reports "bit-identical". Record that in the evidence file (section "WP1"). If anything
   moved, stop and report (D-1, item 2).
5. Update doc 03 §1.3 and doc 07's timing text: the next attack starts when the cooldown ends, during the backswing
   if need be; a new order cancels the backswing. Update `DESIGN.md`'s Combat row text if it describes the gate
   (it does not today).

**Gate.** Common gate. **Changes no expected result.**

---

### WP2: Rule: a damage point of 0 releases on the tick the swing starts (Q0036, cadence)

**Files (owned):** `combat.rs`, `weapon_cadence_tests.rs`, golden fixtures and matrix entries if moved, docs 03 and 07.

**Steps**

1. **Failing tests first** (`weapon_cadence_tests.rs`, scratch catalog):
   - `damage_point_zero_fires_every_period`: period 21, damage point 0 → releases on ticks t, t + 21, t + 42. Red
     today (22).
   - `damage_point_zero_releases_on_the_tick_its_swing_starts`: `step_entity_weapon` returns `true` on the first
     tick the target is in band and aligned, and the phase never shows `Windup`. Red today.
   - `damage_point_zero_after_turning_releases_on_the_tick_it_is_aligned` (a firing arc narrower than the start
     angle). Red today.
   - `damage_point_zero_out_of_a_backswing_releases_at_once` (D-2's path; backswing 30, period 21). Red today.
   - `damage_point_one_is_unchanged` (guard, green before and after).
2. Implement D-3. `step_ready_weapon` and `step_rotating_weapon` return `bool`; the release routine is shared.
3. **Measure** (D-5) and re-record what moved. Expected from the probe (§5.1): battles 9a, 10, 12a, 12b (red
   survivors 3 → 2, still passing), 17, 18 and 26 change their end tick or counts; no survivor expectation fails.
   Check lead-unit hull with the tool. `golden_replay_orders.ron` changes only if its damage-point-0 spotter fires
   (**verify**). Each re-recorded value is explained in the commit message as "damage-point-0 weapons now fire
   every `period` ticks (Q0036, D-3)", naming the identity.
4. Docs: doc 03 §1.3 and doc 07 say a damage point of 0 means the shot lands on the tick the attack starts.

**Gate.** Common gate.

---

### WP3: Audit research and the all-corrections measurement (Q0039, option 3)

**Files (owned):** new `docs/evidence/<date>-catalog-time-audit.md`.

**Steps**

1. List every field of D-7 for every identity with an analogue, with our value (script over the RON files; the
   script stays in `~/scratch`, its command line goes in the evidence).
2. For each, find the editor value in SC2GameData `5444e12f13` (cite layer, file and line; the `default` element of
   `core` when no layer sets it) and Liquipedia where it helps. Convert as in D-7. Mark **not found** where the
   effort bound runs out, as the investigation did.
3. Classify A to E (D-7). For class D, name the document and the line that chose the value.
4. Group the class B and C corrections for WP12, one field kind and one cause per group (for example "build times:
   Normal → Faster", "zerg life regeneration: Normal → Faster", "weapon periods that differ from the reference by
   more than rounding"). Mark which groups touch battles of the matrix. Confirm explicitly: every shielded
   archetype's `ShieldRegenRate` (2) and `ShieldRegenDelay` (10) in Normal time (for WP10 and WP11), and the marauder
   analogue's Stimpack duration (for WP8).
5. **After WP2 has merged:** build a scratch catalog with every class B and C correction, measure the whole matrix
   on 20 seeds and battle 33 on 68 (D-5) against the post-WP2 state, and write the result into the audit file:
   which battles move, which expectations would fail, and which group causes each (rerun with groups removed one at
   a time for any failure). This is the "measurement before any value changes" of Q0039.
6. File one owner question for all class D items (next free number at that time), default no change. If the
   measurement shows a failure that is not explained by a group's own effect, file it as a question too.

**Gate.** Documentation gate.

---

### WP4: Data: zealot weapon (Q0036)

**Files (owned):** `units.ron` (`psi_grey` weapon), `trace_tests.rs` (one test restated), goldens, matrix, doc 07
(zealot row), evidence.

**Steps**

1. **Test-only:** restate `a_swing_winding_up_when_another_attacker_kills_the_target_is_cancelled` on a scratch
   melee archetype with a wind-up (it needs one; the zealot will have none). Green before and after. Check
   `sc2_battle_benchmarks_tests.rs` and any other test that assumes the zealot winds up; restate the same way.
2. **Read Q0044.** If answered with option 2 or 3, implement that first in its own commit.
3. Per-field scratch measurements (D-5 item 5): damage point 0; period 21; range 0.1; backswing 9 — each alone,
   then together.
4. Edit `psi_grey`: `damage_point_ticks: 0`, `period_ticks: 21`, `max_range: 0.1`, `backswing_ticks: 9`.
5. Measure; re-record. Expected (probe): battles 7, 8, 23a (zealots alive 2.10 → 2.00 on 20 seeds), 23b, 25a, 25b
   move; battle 33 fails on two seeds of record → re-record per D-6 / Q0044. Both golden replays change. The
   benchmarks may change their margins; record them. Doc 07's zealot row: range 0.1 (it says 0.2), period 21.
6. Commit message: Q0036, the four values with sources, the cadence (21 ticks, D-3), every moved expectation.

**Gate.** Common gate.

---

### WP5: Data: zealot attributes (Q0040)

**Steps**

1. **Verify inertness first:** search the catalog for any weapon `target_flags`, ability filter or bonus that reads
   Biological (bit 2) or Psionic (bit 4), and `sim_core` for any read of those flags. Today none in `sim_core` (only
   `catalog.rs` sets them and `client/src/lab_ui.rs` displays them). If a filter exists, the change is not inert:
   measure and record it like any data step.
2. Edit `psi_grey`: `attributes: (bits: 5)`. Measure: the matrix and goldens should be bit-identical (attributes are
   catalog data, not hashed state). Record "bit-identical" in the evidence.

**Gate.** Common gate.

---

### WP6: Data: marine damage point and backswing (Q0038, step 2)

**Steps**

1. Per-field scratch measurements: damage point 1; backswing 13; both.
2. Edit `pulse_bot` ground and air weapons: `damage_point_ticks: 1`, `backswing_ticks: 13`.
3. Measure; re-record. Expected: battle 25b keeps its zealot inside 5 to 45 hull (the gate is gone; before WP1 it
   was 62); battle 33 within noise (investigation +0.09); marine battles may shift by a tick. Both golden replays
   change. `stutter_step_tests.rs` passes untouched (decoupled in WP1).

**Gate.** Common gate.

---

### WP7: Data: marine speed (Q0039)

**Steps**

1. Edit `pulse_bot`: `max_speed: 0.126`, and `acceleration` and `deceleration` to the same value (the catalog's
   convention; both are parsed and unused, evidence §2.3).
2. Measure; re-record. Expected (probe, combined with WP6): battles 5c, 12a, 16b, 21, 32 and 33 move; battle 33's
   seeds of record move again (re-record per D-6). Both golden replays change. Doc 07 already says 0.1260.

**Gate.** Common gate.

---

### WP8: Data: Stimpack duration (Q0039)

**Steps**

1. Edit every `overclock.duration_ticks` whose analogue's Stimpack behaviour lasts 15 Normal s: the marine's
   (`liberty` BehaviorData:680) and, if WP3 confirms it, the marauder analogue's: 250 → 268.
2. Measure; re-record. Expected: no matrix battle lasts past tick 250 of a stim (25b ends near tick 142), so the
   matrix should be bit-identical; `golden_replay.ron` changes after its stim at tick 260 plus 250.

**Gate.** Common gate.

---

### WP9: Code: the shield-regeneration delay comes from the catalog (D-4)

**Steps**

1. **Failing tests first:**
   - `catalog/validation.rs` unit tests: a shielded archetype without `shield_regen_delay_ticks` is rejected; an
     unshielded archetype with it is rejected (typed errors naming the field).
   - `world/upkeep.rs` unit test (or `damage_and_shields_tests.rs`): a scratch shielded archetype with delay 10
     regenerates from the 10th tick after damage, not before. Red today (250).
   - `damage_and_shields_tests.rs` reads the delay from the archetype instead of the literal 250.
2. Add the field (D-4) to the definitions, `UnitArchetype` (with `///` doc: ticks at 25 per Faster second, measured
   from the last damage), validation and `upkeep_entity`. Set it to 250 on every shielded archetype in `units.ron`
   and `structures.ron`. Replace the spawn literal (D-4). Update the docs that say "250-tick" (`combat.rs` module
   invariant 4, `entity.rs`, `catalog.rs`).
3. **Proof:** the matrix is bit-identical in outcomes; if the golden checksums moved, it is only through the spawn
   counter (show it: the first differing checkpoint is the first one, and every behavioural test is unchanged).

**Gate.** Common gate.

---

### WP10: Data: shield regeneration rate (Q0039)

**Steps**

1. Edit `shield_regen_per_tick: 0.08 → 0.112` on every shielded archetype WP3 confirmed (`ShieldRegenRate 2`).
   Any shielded archetype WP3 found different is left to WP12 with its own group.
2. Measure; re-record. Expected: only fights in which a shielded unit goes 250 ticks unhit move (the investigation
   found battle 33 identical with the zealot's rate alone). Goldens change if a shielded unit regenerates in them.

**Gate.** Common gate.

---

### WP11: Data: shield-regeneration delay (Q0039)

**Steps**

1. Edit `shield_regen_delay_ticks: 250 → 179` on every archetype WP3 confirmed (`ShieldRegenDelay 10`).
2. Measure; re-record. Expected: long Protoss fights (for example battles 28 and 29, 100 zerglings against immortals
   or stalkers) and battle 33 may move slightly; goldens change. Not measured before (evidence §5.3).

**Gate.** Common gate.

---

### WP12: Data: the audit's remaining corrections

**Steps.** For each group of WP3 step 4, in the order WP3 lists (groups touching no matrix battle first, then the
others), one commit each: edit, measure (D-5), re-record, cite. Class D items wait for the owner's answer to WP3's
question. The package ends when every class B and C value is corrected or recorded as deferred with a reason.

**Gate.** Common gate per commit.

---

### WP13: Bookkeeping

Each step is its own documentation commit, made when its trigger lands. A closed question gets `## Resolution`
(what changed, which commits) and moves with `git mv` to `closed/` (questions README, rule 2).

1. **At once (no trigger):**
   - **Q0035:** record the follow-up: `DESIGN.md` §4, Combat row, "Known gaps": "How StarCraft II spaces melee
     attackers around a solid enemy is not researched; the contact rules of `d4109e2` stay (Q0035); follow-up in plan
     `2026-11-04-combat-sc2-alignment` §8 F-1." Close Q0035.
   - **Q0007:** close, as the owner allowed: the investigation is done, its follow-ups Q0035 to Q0041 are answered,
     and this plan carries them out.
   - **Q0037:** close with "planned as WP14 of this plan, low priority" (the answer asked for a plan).
2. **After WP0:** close Q0041 (the comment's commit; later steps update its figures, D-6).
3. **After WP4** (with WP2): close Q0036. **After WP5:** close Q0040. **After WP6** (with WP1): close Q0038.
   **After WP12:** close Q0039 (the four values in WP7, WP8, WP10, WP11; the audit in WP3 and WP12).
4. **At the end of lane B:** `DESIGN.md` Combat row (remove "Changed outcomes await the owner (Q0007)"; describe the
   weapon timing rule) and Catalog data row; this plan's row in `docs/plans/README.md` (`Done` with commits, or
   `Partly done` naming WP14). Q0044 is closed by whichever WP implements its answer.

**Gate.** Documentation gate.

---

### WP14 (low priority, last): StarCraft II scan ranges (Q0037)

Starts only after WP12, when the owner's priority allows. It may be split into its own plan if the mechanism study
(step 1) finds a rule problem.

1. **Mechanism in battle 33 first.** With the trace tool's per-unit CSV on 68 seeds, at 9 / 9 and at 5.5 / 5 on a
   scratch catalog: for each marine, the time to its first target, the number of distinct targets and switches, and
   per tick the number of marines firing at the most-targeted zealot. The investigation saw marine switches fall to
   0 and zealot swings rise (§5.3, reading 3); find which zealots the marines focus and why. Add any measure the tool
   lacks to `sim_fixtures::trace`, test first.
2. **Reference table** in the audit file: for each identity with a weapon, StarCraft II's acquisition range (the
   weapon's `MinScanRange` in the game data, and Liquipedia's Legacy of the Void rule "weapon range + 0.5, at least
   5"), and how our `scan_range` is measured (centre to centre, `target_acquisition.rs`) against how the game
   measures it (**verify**: edge or centre). Values stay per-unit data (charter §3.3); the rule is how the data is
   derived, not code.
3. **Whole matrix** on a scratch catalog with every reference scan range (D-5), against the state after WP12.
4. **Report** in the evidence file. If every moved battle is explained by the mechanism, apply the values in one
   data commit ("scan ranges to StarCraft II", cause stated), re-recording per D-6; otherwise file the unexplained
   part as an owner question and stop.

**Gate.** Common gate for code and data; documentation gate for the report.

---

## 7. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| WP1 is not latent after all (a weapon under some upgrade's `attack_cooldown_delta` has backswing ≥ cooldown) | the rule commit moves results | D-1 item 2: WP1 stops, reports the case, and the plan is amended (the move gets its own explained commit) |
| WP2 moves battles beyond the probe's list (the probe used period − 1, not the real fix) | more re-recording | measured, not guessed; each named in the commit |
| Battle 33's expectation churns across WP4 to WP11 | several fixture edits | D-6 default; Q0044 may replace it with a distribution test |
| The trace inference rule needs period > damage point after modifiers | miscounted releases in the evidence | WP1's validation rule for base values; WP0 documents the bound; no shipped modifier breaks it (verify in WP0) |
| The audit finds many class C values | WP12 grows | groups, one commit each; class D to the owner; WP12 may be split across sessions |
| A client test or golden depends on a marine or zealot value (presentation, battle card) | a data commit breaks a client test | the common gate runs the whole workspace; such a test is decoupled first (D-1 item 5) |
| Determinism on other targets | a re-recorded golden fails on Windows, macOS or wasm | no float enters state; CI's determinism jobs re-verify; a red job stops the chain |

---

## 8. Out of scope and follow-ups

- **F-1 (Q0035): melee contact rules.** How StarCraft II spaces melee attackers around a solid enemy (which radius
  governs, whether enemies push each other) was not found in an authoritative source (evidence §1.12, §3.4, §5.2).
  The rules of `d4109e2` stay. A future investigation would: measure spacing in the StarCraft II editor or a unit
  tester (zealots attacking a held marine block), read the engine's separation and push rules from the game data
  where they are data, and only then propose a rule with a whole-matrix measurement. Recorded in `DESIGN.md` §4 by
  WP13.
- **F-2: does a backswing hold an automatically attacking unit in place?** D-2 keeps today's behaviour (it does
  not). Researching and changing it would move every unit's movement; it needs its own plan.
- **F-3: fractional cadences and StarCraft II's random firing delay** (evidence §1.4: −0.0625 to +0.125 Normal s).
  Periods stay whole ticks, rounded to the nearest.
- **F-4: the zealot's second strike 5 ticks after the first, only within 2.1** (Q0036 option 3, not chosen).
- **Separation radius and push priority** (evidence §7.1, "found but not proposed"): not changed.
