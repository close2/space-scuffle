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

**Amendment 1 (2026-11-04, against `main` at `16a6352`).** Folds in the owner's answers to Q0044 and Q0045, given
after the audit of WP3 ([`docs/evidence/2026-11-04-catalog-time-audit.md`](../evidence/2026-11-04-catalog-time-audit.md),
commit `92838ba`). WP0 and WP1 were being implemented when this amendment was written; their scope is unchanged.
What changed:
- D-6 records Q0044 as decided (option 1, the former default); WP4 step 2 no longer waits for it.
- New §2.5 (hull turning today) and three design decisions: D-8 (hull turn rates, the unit of the field, and one
  hull rotation per tick), D-9 (the Overclock moves to the Marauder analogue), D-10 (`flying_saucer` becomes the
  Phoenix).
- Three new packages: WP15 (Overclock on `shock_golem`), WP16 (`flying_saucer` as the Phoenix) and WP17 (hull
  turning at StarCraft II rates, with its own whole-matrix measurement).
- WP3 step 5, WP8, WP12, WP13 and WP14 are amended. WP12 now applies every class B and C value and the class D
  backswing of `welder_drone`. The audit's turret group W8 moves from WP12 to WP17.
- The lane B chain, file ownership (§4), risks (§7) and follow-ups (§8) are updated.

**Owner decisions** (answered 2026-11-04 in commit `9161cdb`; evidence in
[`docs/evidence/2026-11-03-battle-33-investigation.md`](../evidence/2026-11-03-battle-33-investigation.md)):

| Question | Answer | Where in this plan |
|---|---|---|
| [Q0035](../questions/open/0035-melee-contact-rules.md) contact rules | Option 1: no change now; record a follow-up for future investigation | WP13 step 1, §8 F-1 |
| [Q0036](../questions/closed/0036-zealot-weapon-timing.md) zealot weapon | Option 2: `damage_point: 0`, `range: 0.1`, `backswing: 9`, `period: 21`; clean up the "damage point 0 fires every period + 1" quirk if appropriate | D-3, WP2, WP4 |
| [Q0037](../questions/closed/0037-scan-ranges.md) scan ranges | Option 3, low priority: a plan for StarCraft II's rule across the catalog, mechanism and whole matrix first | WP14 (last) |
| [Q0038](../questions/closed/0038-marine-backswing-gate.md) backswing | Option 2: change the rule first, then marine `damage_point: 1`, `backswing: 13` | D-2, WP1, WP6 |
| [Q0039](../questions/open/0039-normal-time-values.md) Normal-time values | Option 3: convert the four values and audit every time and speed value, with a whole-matrix measurement | D-4, D-5, WP3, WP7 to WP12 |
| [Q0040](../questions/closed/0040-zealot-attributes.md) zealot attributes | Option 2: `Light, Biological` | WP5 |
| [Q0041](../questions/open/0041-battle-33-expectation-under-jitter.md) battle 33 under jitter | Option 2: document the ≈90 % marine win in the fixture, with a pointer to the evidence | WP0 step 5 |
| [Q0007](../questions/open/0007-combat-baselines.md) combat baselines | "Q0007 may now be closed" | WP13 step 1 |
| [Q0044](../questions/closed/0044-battle-33-expectation-after-alignment.md) battle 33 after the corrections | Option 1: re-record to the seeds of record and document the distribution; "Having a close battle isn't a bad thing" | D-6, every WP that moves battle 33, WP13 step 3 |
| [Q0045](../questions/open/0045-catalog-time-audit-deliberate-values.md) the audit's deliberate values | "Try to close in on SC2 behavior": 1(b) reference hull turn rates; 2(b) worker backswing 9; 3(b) the 20-life Overclock moves from `rhino_beetle` to `shock_golem`, 268 ticks; 4(b) `flying_saucer` becomes the Phoenix and is audited | D-7, D-8, D-9, D-10, WP12, WP15, WP16, WP17 |

Questions filed by this plan: [Q0044](../questions/closed/0044-battle-33-expectation-after-alignment.md) (answered
above) and, through WP3, [Q0045](../questions/open/0045-catalog-time-audit-deliberate-values.md) (answered above).
Amendment 1 files no new question. WP17 may file one, under the conditions it states.

---

## 1. Summary

| WP | Title | Changes expected results? | Size | Depends on | Lane |
|---|---|---|---|---|---|
| WP0 | Measurement tool: whole-matrix mode, phase-independent release inference; baseline; Q0041 comment | no | M (1 day) | none | A |
| WP1 | Rule: a backswing locks orders but never delays the next attack | no (proved latent) | S-M (half a day to a day) | none (measure after WP0) | B |
| WP2 | Rule: a damage point of 0 releases on the tick the swing starts | yes: 6 identities with damage point 0 (battles 9a, 10, 12a, 12b, 17, 18, 26 by the probe); goldens possibly | M (1 day) | WP0, WP1 | B |
| WP3 | Audit research: every time and speed value against the 5.0.14 game data; all-corrections scratch measurement | no (documentation) | L (2 to 3 days); research done in `92838ba` | none; its measurement step after WP2 | C |
| WP4 | Data: zealot weapon (Q0036) | yes: battles 7, 8, 23a, 23b, 25a, 25b, 33; both golden replays | S (half a day) | WP2 | B |
| WP5 | Data: zealot attributes `Light, Biological` (Q0040) | no (inert, verified) | XS (1 to 2 h) | WP4 | B |
| WP6 | Data: marine damage point 1, backswing 13 (Q0038) | yes: battle 33 and marine battles slightly; both golden replays | S | WP1, WP5 | B |
| WP7 | Data: marine speed 0.126 per tick (Q0039) | yes: marine battles; both golden replays | S | WP6 | B |
| WP8 | Data: Stimpack duration 268 ticks on `pulse_bot` (Q0039) | yes: `golden_replay.ron` (its `pulse_bot` squad stims at tick 260) | S | WP7 | B |
| WP15 | Data: the 20-life Overclock moves from `rhino_beetle` to `shock_golem`, 268 ticks, unlocked by the same research (Q0045 3b) | expected no (no fixture stims either identity; verify) | S (half a day) | WP8 | B |
| WP9 | Code: shield-regeneration delay read from the catalog (value 250 kept) | no behaviour change (checksums only if the counter's start value changes) | M (1 day) | WP15 | B |
| WP10 | Data: shield regeneration 0.08 → 0.112 per tick, every shielded archetype (Q0039) | yes: Protoss battles that last past the delay; goldens | S | WP9, WP3 | B |
| WP11 | Data: shield-regeneration delay 250 → 179 ticks (Q0039) | yes: as WP10 | S | WP10 | B |
| WP16 | Research and data: `flying_saucer` becomes the Phoenix (Q0045 4b) | yes: `golden_replay_orders.ron` (its spotter); no matrix battle uses it | M (1 day) | WP11 | B (research step: C) |
| WP12 | Data: every class B and C correction of the audit and the worker's backswing (Q0045 2b), one field group and cause per commit | yes, per group | L (2 to 4 days, 20 commits) | WP16, WP3 step 5 | B |
| WP17 | Hull turning at StarCraft II rates (Q0045 1b): field unit, one hull rotation per tick, matrix spawn facing, firing arcs, turret rates (W8), then the 42 hull rates | rule and unit commits: no (proved latent); data commits: yes, potentially every battle and both goldens | L (3 to 4 days) | WP12 (research step: none) | B (research step: C) |
| WP13 | Bookkeeping: questions closed as each lands; `DESIGN.md`; docs 03 and 07; plan index | no | S, incremental | each WP | D |
| WP14 | Low priority: StarCraft II scan ranges (Q0037): mechanism, reference table, whole matrix, then data | yes, if applied | M-L (2 to 3 days) | WP17 | E (last) |

**Lanes and concurrency (at most three gates at once).** Lanes A, B and C start together: WP0, WP1 and the research
of WP3. Lane B is a chain, one gate at a time, because every package from WP4 on edits `assets/catalog/units.ron`
and most edit the golden fixtures: WP1 → WP2 → WP4 → WP5 → WP6 → WP7 → WP8 → WP15 → WP9 → WP10 → WP11 → WP16 →
WP12 → WP17. WP1's implementation may start before WP0 lands, but WP1 merges after WP0 so that its "no change"
proof uses the tool. WP3's research needs no gate; its measurement step (§D-5, step 5) waits for WP2. Lane C, after
WP3's research, takes the research steps of WP16 (step 1) and WP17 (step 1); both are documentation and need no
gate, and each must be merged before its package's first lane B commit. Lane D (WP13) is documentation and runs
beside whatever has just merged. Lane E (WP14) starts only after WP17, and only when the owner's low priority
allows. One agent can do WP4 to WP8 and WP15 in one session, one commit each.

**Why this order for the new packages.** WP15 follows WP8 so that the Overclock's duration is converted once, on the
identity that keeps it, and WP8 never edits a capability WP15 removes. WP16 follows WP11 so that the Phoenix's
shield values are already final, and precedes WP12 so that WP12's groups never meet a `flying_saucer` value in
flux. WP17 comes last in lane B because hull turning moves every battle: measuring it on the final catalog keeps its
effect separate from the audit's groups, and WP12's groups are measured with instant turning, as WP3 step 5 was.

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
- **Who stims in `golden_replay.ron`** (checked for Amendment 1): its only `UseStim` order (tick 260) goes to the
  squad `blue_infantry`, 16 `pulse_bot`. The 4 `rhino_beetle` of `blue_armor` are never ordered to stim. The matrix
  stims only in battle 25b (`pulse_bot`, research `overclock_stim`).

### 2.5 Hull turning today (read while writing Amendment 1)

- **The archetype's turn rate is dead data.** `UnitArchetype::turn_rate` (`catalog.rs`, from `turn_rate_deg` via
  `UnitArchetypeDef::turn_rate`, clamped to 0 to 180 degrees per tick, 180 meaning a half turn) is set by the loader
  and read by no simulation code. Raising or lowering it changes nothing today.
- **The hull heading snaps.** `Entity::heading` (binary angle, hashed in the checksum) is written in three places:
  - `movement/locomotion.rs`, the motion step (`proposal.heading = SimAngle::atan2(velocity…)`): a moving unit faces
    its velocity at once;
  - `locomotion.rs`, `Pursuit::aim_if_in_range` (`proposal.heading = angle`): a unit that stops to attack faces its
    goal at once (a turreted weapon turns `turret_angle` instead);
  - `combat.rs`, `slew_towards`: a weapon without a turret rotates the hull by the **weapon's** `turret_turn_rate`.
    For the 48 shipped weapons with `has_turret: false` the authored `turret_turn_rate_deg` is 0, which the loader
    turns into `SimAngle::PI`, so this rotation is a snap as well.

  The tick runs `step_movement_and_steering` before `step_weapons_and_projectiles` (`world.rs`, `step_into`), so the
  hull can be rotated twice in one tick, once by each writer.
- **Firing arcs.** `firing_arc_deg` is read as a half angle (`firing_half_angle`): 30 unit weapons have 15, 11 have
  180, 10 have 360, and the 6 structure turrets 360. While the hull snaps, a hull weapon is always aligned, so its
  arc has never mattered. It matters for the 3 turreted unit weapons (`railgun_crawler`'s weapon and its Siege Mode
  override, `colossus_mech`'s air weapon), whose turrets turn at 21.6° per tick, and for the structure turrets. The audit did not cover arcs (they are not time values).
- **Two unit conventions for the same quantity.** The hull field is read in degrees per tick. The turret field is
  read in degrees per tick up to 360 and in degrees per **second** above 360 (`convert_weapon`, the
  `turret_turn_rate_deg > 360.0` branch). Doc 07 §2.3 describes both in degrees per second ("Siege Tank Turret
  (`540.0` deg/s): 21.6°/tick"; "Colossus Turn Rate (`280.0` deg/s)"), and the catalog's 280 for the Colossus
  analogue was written to that description, but the loader reads it as 280° per tick. This is the inconsistency the
  audit and Q0045 name.
- **Spawn heading.** `Entity::new` sets `heading: SimAngle::ZERO` (facing +x) for every unit, and
  `sim_fixtures::matchup` spawns through `SimWorld::spawn_unit` without a facing. In a matrix battle one side
  therefore starts facing its enemy and the other faces away. With instant turning that costs nothing.

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

### D-6. Battle 33's expectation (Q0041 and Q0044)

- **Q0041, now (WP0).** A RON comment above battle 33 in `matchup_matrix.ron`: on 68 seeds with the matrix's jitter
  the battle is a marine win on about 90 % (61 of 68; 6 zealot wins, 1 unresolved), the seeds of record are among the
  winners, and the evidence is the investigation's §4.2. No expectation, seed or claim text changes in that commit.
- **After the corrections (WP4 on).** The probe (§5) says battle 33 becomes about a 70 % marine win, and one or two
  of its seeds of record stop meeting `blue: (4, 16), red: (0, 0)`. **Decided (Q0044, option 1; "Having a close
  battle isn't a bad thing"):** each commit that moves battle 33 off its expectation re-records it to the smallest
  ranges that hold on the four seeds of record, updates the comment's distribution to the step's 68-seed measurement
  (marine wins / zealot wins / unresolved, marine survivors mean and SD), and rewrites the claim to describe a close
  fight, with a pointer to the step's section of the evidence file. Seeds and setup do not change (option 4 was not
  chosen), and no distribution test is added (option 2 was not chosen).
- **Every commit that moves battle 33 updates the comment, not only those that break the expectation.** A step that
  keeps the expectation but shifts the distribution (for example WP7) still rewrites the comment's figures, so the
  comment always describes the catalog it sits next to. That is a documentation change inside a commit that already
  has its cause; it adds no second cause.
- **WP17 and WP14 apply the same rule.** Hull turning (WP17) and the scan ranges (WP14) are expected to move battle 33
  again; each re-records it this way.

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
| D | Differs, and a project document chose it deliberately (doc 07, a plan) | listed; one owner question collects them, default no change. **Amendment 1:** the owner answered that question (Q0045) with "close in on SC2 behavior", so every class D value is now corrected too: the hull turn rates in WP17, the worker's backswing in WP12 |
| E | No reference (no analogue, or not modelled) | listed |

**Amendment 1: what WP12 applies.** Q0045's general direction ("try to close in on SC2 behavior … for now SC2
behavior is much easier to test and debug") applies to the whole audit. Every class B and C value is corrected. A
value may be deferred only for a reason outside the value itself, and the deferral names the reason in the evidence:
- no single reference value exists (the audit's §8: thrown or accelerating missiles, the Brood Lord's broodlings);
- the reference is marked **verify** and the verification fails within the effort bound (for example the Siege Mode
  transformation time, audit §8);
- the reference needs a mechanism we do not model (for example a Siege Mode without cooldown, if validation requires
  one; WP12 then records it and asks before changing validation).

A value is never deferred because it moves an expectation; that is what the measurement and the re-recording are
for. Class E values stay as they are, except `flying_saucer`'s, which WP16 turns into class A to D values by giving
it the Phoenix as its counterpart.

**Amendment 1: the layer chain.** The audit corrected the chain to `core → liberty → swarm → void → voidmulti`
(audit §2.2). Every later package cites values from that chain, not from the eight-layer chain above.

Already visible while planning (to be confirmed by WP3): `psi_grey.build_time_ticks` is 950 = 38 s × 25, the
Normal-time build time (reference 27.14 Faster s = 679 ticks; doc 07 says 675). Every shielded archetype has
`shield_regen_per_tick: 0.08` (class B).

**Output.** A new evidence file `docs/evidence/<date>-catalog-time-audit.md`: method, one table per catalog file
(identity, field, ours, editor value and source line, reference in our units, class), the change groups for WP12 (one
field kind and one cause each), and the whole-matrix measurement of a scratch catalog with every class B and C
correction applied (D-5), against the state after WP2. Q0039 asks for that measurement "before any value changes":
the audit's own corrections (WP12) start only after it.

### D-8. Hull turning at StarCraft II rates (Q0045 1b)

Adopting the reference rates is not a data change alone: the field is read by nothing (§2.5), so the rates need a
rule first. This decision fixes the field's unit, the rule, and the order of commits.

**1. Unit: degrees per tick, in the field's name.**
- Both angular rates are degrees per tick, as every other rate in the catalog is per tick (`max_speed`,
  `hp_regen_per_tick`, `shield_regen_per_tick`) and as the loader already reads the hull field.
- The fields are renamed `turn_rate_deg_per_tick` and `turret_turn_rate_deg_per_tick`, with no `#[serde(default)]`.
  A catalog that still uses an old name then fails to load with a "missing field" error naming the new one, instead of
  being misread.
- The turret branch that reads values above 360 as degrees per second is removed. No shipped value is above 360.
- Doc 07 §2.3 is rewritten to the per-tick unit, with the conversion "degrees per tick = Normal °/s × 1.4 ÷ 25"
  (audit §2.3). Its "999 deg/s overflows to instant" and "Colossus 280 deg/s" lines go.

*Why not degrees per second?* It would match how the game and Liquipedia state the rates. But the loader would then
divide by 25 with rounding for this one field, and per-second is the reading that caused the inconsistency: doc 07
described per-second while the loader read per-tick. *Why rename instead of documenting?* The misreading happened
with a doc comment in place ("degrees per tick" on `UnitArchetypeDef::turn_rate`). A unit in the name cannot be
missed.

**2. Range, and what "instant" means.**
- Hull: `0 < rate ≤ 180`. Turret: `0 < rate ≤ 180` when `has_turret`, and exactly 0 when not (a hull-mounted weapon
  turns with the hull, item 4).
- 180 means a half turn per tick. Because no two facings are more than 180° apart and `SimAngle::rotate_towards`
  reaches any target within its step, 180 reaches any facing in one tick. "Instant" is therefore a consequence of
  the geometry, available to any value that asks for it (Q0045 option b), not a special case in code.
- Values outside the range are rejected by `catalog/validation.rs` with a typed error naming the field and the
  range. Today they are clamped silently, which charter §1.1 forbids.

**3. Precision.** Hull and turret rates are converted with the same function: binary angle units =
`round(deg × 65536 ÷ 360)`, halves up. Today the hull value is truncated to whole degrees (`deg as i32`). Reference
rates are authored with two decimals:

| Normal °/s (game data) | Degrees per tick | Binary units per tick | Ticks for a half turn | Identities (audit §4) |
|---|---|---|---|---|
| 1499.9414 | 84.0 | 15292 | 3 | `sky_bastion`, `cloak_gunship`, `sonic_bat` |
| 999.8437 | 55.99 | 10193 | 4 | the other 28 units |
| 720 (`CUnit` default) | 40.32 | 7340 | 5 | `napalm_wheeler`, `void_goliath`, `tripod_annihilator`, `stealth_eye_orb`, `phase_prism_saucer`, `mothership_carrier`, `spore_matriarch` |
| 360 | 20.16 | 3670 | 9 | `railgun_crawler`, `colossus_mech`, `stag_behemoth` |

`flying_saucer`'s rate is the Phoenix's, found by WP16. The implementer checks each identity's row against the
audit's §4 table, which is the source; this table only groups them.

**4. Rule: the hull turns at most its rate per tick, whoever asks.**
- New `Entity` method `turn_hull_towards(&mut self, desired: SimAngle) -> SimAngle`, and a per-tick budget
  `hull_turn_budget: u16` (binary angle units). At the start of `step_movement_and_steering` every entity's budget is
  set to its archetype's `turn_rate` (one pass over the entities, no allocation). Each call rotates the heading by at
  most the remaining budget and spends what it used.
- The three hull writers of §2.5 call it instead of assigning: the motion step (facing the velocity), `aim_if_in_range`
  (facing the goal), and `slew_towards` for a weapon without a turret. So the movement phase spends the budget first
  and the weapon phase gets the rest, in today's tick order, and the hull can no longer turn twice in one tick.
- A hull-mounted weapon therefore turns at the hull's rate, never at a turret rate. A weapon with a turret keeps
  turning `turret_angle` at its own rate, as today.
- **Translation is not gated by facing.** A moving unit moves along its velocity as today, and its heading follows at
  the hull rate. This is the smallest rule. If WP17's research finds that the game makes these units turn before they
  move (a unit flag in `UnitData` or a mover setting; **verify**), that becomes a separate, data-driven rule commit
  (an archetype field such as `turn_before_move: bool`, default false) and a separate data commit, both inside WP17.
  If the research cannot state the game's rule precisely enough to specify and test it, WP17 files an owner question
  (next free number, default: no gating) and goes on without it.
- **Determinism and the checksum.** Integer binary-angle arithmetic only. The budget is reset before its first read
  in every tick, so it carries no state from one tick to the next. It is left out of the checksum, and the field's doc
  states that invariant. `heading` is already hashed. The checksum schema does not change.

**5. Matrix battles spawn facing their enemy.**
- `sim_fixtures::matchup` spawns every unit facing the centre of the opposing groups. This uses a new public
  `SimWorld::spawn_unit_facing(unit_type, owner, pos, heading)`; `spawn_unit` keeps heading 0 and calls it.
- Reason: the matrix asks who wins a fight. With finite turning, a side that spawns facing away (§2.5) starts 4 to 9
  ticks late, which is an artefact of the fixture, not of either unit.
- The golden replays keep heading 0: their spawn is part of the scenario they pin, and they are re-recorded by the
  data commit anyway.
- This commit changes no outcome while turning is instant. It is proved bit-identical by the tool, before any rate
  changes.

**6. One cause per commit.** WP17 lands in this order. Items a to c change no result on shipped data and must be
proved bit-identical; d to g each have one cause and are measured.

| Commit | Content | Expected results |
|---|---|---|
| a | Unit and range: the renames, the validation range, the shared rounding, the removed per-second branch. Data rewritten to the same behaviour: hull 999, 360 and 280 → 180; structure turrets 360 → 180 | none. Golden checksums move only if a hashed `WeaponSpec` holds a structure turret's rate (360° gave raw 65535 through the saturating cast, 180° gives 32768). In that case the proof is as WP9's: outcomes unchanged, first differing checkpoint explained |
| b | The rule of item 4 | none: every shipped rate is 180 |
| c | Matrix spawn facing (item 5) | none in outcomes |
| d | Firing arcs to the reference, if the research found differences: hull-weapon arcs first (latent while turning is instant; proved), then turreted-weapon arcs (measured) | turreted-weapon arcs: battles with `railgun_crawler` or `colossus_mech` air |
| e | Turret rates to the reference (audit group W8, moved here from WP12) | battles with `railgun_crawler` |
| f | The 42 hull rates of item 3 | potentially every battle and both goldens |
| g | Only if the research found turn-before-move: its rule (latent) and then its data, two commits | measured |

**7. Decisions taken by WP17 (2026-11-05, from its research, audit §10).** Charter §8 lets the implementer narrow
scope and fill gaps the way StarCraft II does, if written down. These are written here:
- **Turn-before-move: not implemented; [Q0050](../questions/closed/0050-turn-before-move.md) filed.** The data have
  a `TurnBeforeMove` unit flag (Siege Tank, Thor, Ultralisk) but no tolerance, so the rule cannot be specified and
  tested (item 4's condition). Commit g is not made; the default is no gating.
- **`StationaryTurningRate` not applied.** The game has a second hull rate for a unit that is not moving; it differs
  from `TurningRate` for four identities (audit §10.2). We model one rate, `TurningRate`, as item 3 says; the
  second is follow-up F-8.
- **Firing arcs: one tolerance, `Arc` ÷ 2 + `ArcSlop`.** `Arc` is a full angle (SC2Mapster's editor reference;
  audit §10.3) and `ArcSlop` the extra angle an engaged weapon tolerates. Our `firing_arc_deg` is one half angle
  for both starting and continuing; it takes the engaged tolerance, because the acquisition angle alone (0° for
  most weapons) would make every unit stop and re-aim before each shot at a moving target, which `ArcSlop` exists
  to prevent. The acquisition angle is follow-up F-8. Arcs are converted with item 3's rounding (11.25° = 2048
  binary units), not truncated to whole degrees.
- **Mounts are not changed.** Six weapons sit on a turret in the game and on the hull here, or the reverse (audit
  §10.5). Their arcs and mounts stay as they are, so an arc is never applied to the wrong part; follow-up F-8.
- **The turret is turned in one place.** §2.5 lists `aim_if_in_range` as a hull writer, but it also snaps
  `turret_angle` to the goal for a turreted weapon, so "a weapon with a turret keeps turning at its own rate, as
  today" (item 4) was not true. Commit b removes that write: the weapon phase alone turns a turret, at most its rate
  per tick, as it already did for every other turret move. On shipped data this is latent in outcomes while every
  turreted weapon has an unrestricted arc; `turret_angle` is hashed, so checksums may move, and commit b explains
  any that do.

### D-9. The Overclock moves to the Marauder analogue (Q0045 3b)

- **Today.** `rhino_beetle` (the Roach since plan 21a §2.1) carries an `overclock` with `hp_cost: 20.0`,
  `duration_ticks: 250` and both multipliers 1.5. `shock_golem` (the Marauder) has none. The research
  `overclock_stim` unlocks the ability on `pulse_bot` only (`upgrades.ron`, `UnlockAbility`). Doc 08 line 175 already
  says the research unlocks Overclock "on `pulse_bot` and `shock_golem`".
- **Decision.** One data commit (WP15), one cause: the capability follows its StarCraft II counterpart.
  - Remove `overclock` from `rhino_beetle` (`None`).
  - Give `shock_golem` an `overclock` with `hp_cost: 20.0`, `duration_ticks: 268` and multipliers 1.5. Each value is
    checked against `StimpackMarauder` (`liberty` BehaviorData:604, audit §1 item 3) and its life cost against the
    ability's data; a value that differs takes the reference.
  - Add `UnlockAbility(target_slug: "shock_golem", ability_slug: "overclock")` to `overclock_stim`, because the game's
    single Stimpack research unlocks both (**verify** in `UpgradeData`/`AbilData`; if the game uses a separate
    research for the Marauder, mirror that instead and say so in the commit).
- **The duration is 268 at once.** WP8 converts only `pulse_bot`'s Overclock, and WP15 creates `shock_golem`'s
  already converted, so no value is converted on an identity that loses it.
- **Expected results.** No behaviour change: no fixture stims either identity (§2.4). **Amendment 3 (2026-11-04,
  orchestrator, after WP15's probe):** the checksum does hash each player's `unlocked_abilities` map
  (`checksum/encoding.rs`, `PlayerUpgradeState`), and `golden_replay.ron` researches `overclock_stim` before tick 1, so
  the new `UnlockAbility(shock_golem, overclock)` line changes every checkpoint of `golden_replay.ron` (the `units.ron`
  part alone changes nothing). WP15 therefore re-records `golden_replay.ron` in its data commit with that reason;
  `golden_replay_orders.ron` and the whole matrix must stay bit-identical, which WP15 proves with the tool. Tests that assert
  which archetypes are overclock-ready (`player_view_tests.rs`, `full_match_tests.rs`, the client's battle HUD tests)
  are checked; any that names `rhino_beetle` as a stimmer is restated first, test-only (D-1 item 5).

### D-10. `flying_saucer` becomes the Phoenix (Q0045 4b)

- **Today** (audit §6 item 4). The catalog says "Viking / Scout", doc 07 says Phoenix. Its cost (125/75), build time
  (850), weapons (two identical "Disruptor Beam" blocks, 10 +4 light, range 6, both ground and air, damage point 0),
  speed (0.15) and attributes (`bits: 17`, Light and Psionic) match neither. Only its shield regeneration was audited.
  The research `anion_pulse_crystals` (+2 range) already matches the Phoenix's Anion Pulse-Crystals.
- **Decision.** The Phoenix is its counterpart, for every value doc 07's roster gives for other units and for every
  field of D-7: `sc2_analogue`; cost, supply and build time; life, shields, armour and attributes; speed (and the
  copied `acceleration`/`deceleration`); sight; weapons (targets, damage and bonus, attacks per strike, range,
  period, damage point, backswing, projectile speed, upgrade bonuses); hull turn rate (applied in WP17, D-8). Radii,
  scan range and push priority stay out, as for every other unit (WP14 covers scan ranges).
- **Expected reference, to be confirmed with citations by WP16's research** (Liquipedia's Phoenix page, Legacy of the
  Void; every figure **verify**): 150/100, 2 supply, 25 Faster s build (625 ticks, doc 07's figure), 120 life,
  60 shields, armour 0, Light and Mechanical, speed 5.95 Faster (0.238 per tick), Ion Cannons 5 (+5 light) ×2,
  range 5, air targets only, cooldown 1.1 Normal s (19.64 ticks, audit §1 item 4). If the data and Liquipedia
  disagree, the data at `5444e12f13` wins, as in the audit.
- **What follows for the catalog.** The Phoenix has one weapon, against air. So the ground `weapon` becomes `None`,
  and `air_weapon` targets air only. Its two `Disruptor Beam` blocks become one `air_weapon`. Name and `visual_kind`
  are presentation and may stay.
- **Not modelled (follow-ups, §8):** attacking while moving, and Graviton Beam.
- **One commit, one cause.** The identity decision is the cause. No matrix battle uses `flying_saucer`;
  `golden_replay_orders.ron` spawns it as `blue_spotter`, and `high_ground_combat_tests.rs` uses it as an air spotter.
  The spotter loses its ground attack, so the orders golden changes if the spotter ever fired at a ground unit
  (**verify** with the trace). The high-ground test needs its sight only; it is checked, and restated first if it
  depends on a changed value.

---

## 4. File ownership and order

| File | Owner, in order |
|---|---|
| `crates/sim_fixtures/src/trace/{cli.rs, swing.rs, mod.rs, report.rs}`, new `trace/matrix.rs`, `crates/sim_fixtures/src/matchup.rs`, `crates/sim_fixtures/tests/trace_tests.rs` | WP0; then WP4 (one test restated); `matchup.rs` again in WP17 (spawn facing, D-8 item 5) |
| `crates/sim_core/examples/matchup_trace.rs` (usage text) | WP0 |
| `crates/sim_core/tests/sc2_who_wins_matchup_matrix_tests.rs` (uses the shared expectation check) | WP0 |
| `crates/sim_core/src/combat.rs` | WP1, WP2, then WP9 (module doc invariant 4 only), then WP17 (`slew_towards`) |
| `crates/sim_core/src/entity.rs` | WP1 (`WeaponPhase::Backswing` doc), WP9 (spawn counter), WP17 (`turn_hull_towards`, `hull_turn_budget`) |
| `crates/sim_core/src/movement/locomotion.rs`, `crates/sim_core/src/world.rs` (`spawn_unit_facing`, the budget reset), new `crates/sim_core/tests/hull_turning_tests.rs` | WP17 |
| new `crates/sim_core/tests/weapon_cadence_tests.rs`; `crates/sim_core/tests/stutter_step_tests.rs` | WP1, WP2 |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs`, `world/upkeep.rs`, `crates/sim_core/tests/damage_and_shields_tests.rs` | WP9; `catalog.rs` and `catalog/validation.rs` again in WP17 (renamed fields, range, rounding) |
| `assets/catalog/units.ron`, `structures.ron`, `upgrades.ron` | WP4 → WP5 → WP6 → WP7 → WP8 → WP15 → WP9 → WP10 → WP11 → WP16 → WP12 → WP17 → WP14 |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP0 (comment), then every data WP that moves a battle |
| `crates/sim_core/tests/fixtures/golden_replay*.ron` | WP2 (if moved), WP4, WP6, WP7, WP8, WP15 (expected unchanged), WP9 (if the counter moves), WP10, WP11, WP16 (orders fixture), WP12, WP17 |
| `docs/07-sc2-patch-5-0-14-roster-and-faction-mapping.md` | the WP whose value a cell describes; WP1 and WP2 for the timing diagram text; WP17 for §2.3 (turn rates); WP16 for the Phoenix row |
| `docs/08-sc2-structures-tech-tree-and-upgrades.md` | WP8 (Stimpack duration text), WP15 (Overclock on `shock_golem`), WP12 (production times, P groups) |
| `docs/03-sc2-mechanics-reference.md` §1.3 | WP1, WP2 |
| new `docs/evidence/<date>-combat-sc2-alignment.md` | WP0 creates it; every later WP appends its section |
| new `docs/evidence/<date>-catalog-time-audit.md` | WP3; WP16 adds the Phoenix rows; WP17 adds the turning section (§10); WP14 adds the scan-range table |
| `docs/questions/open/00{07,35..41,44,45}*.md`, `DESIGN.md`, `docs/plans/README.md` (this row), `docs/evidence/2026-11-03-battle-33-investigation.md` §1.2 (layer chain note) | WP13 |

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

**Not in these probes (Amendment 1).** None of the probes includes the audit's groups, the worker's backswing, the
Overclock move, the Phoenix or hull turning. The first four are measured by WP3 step 5 and their own packages. Hull
turning cannot be probed on a scratch catalog at all, because the rule does not exist yet (§2.5): its first
measurement is WP17 commit f. Every marine and zealot turns at the same 55.99° per tick, so its direct effect on
battle 33 should be small once both sides spawn facing each other (D-8 item 5); larger effects are expected where
slow turners (20.16° per tick) fight fast targets, for example battles with `railgun_crawler`, `colossus_mech` or
`stag_behemoth`. These are expectations, not measurements.

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

Documentation-only commits (WP3, WP13, and the research steps of WP16 and WP17) use `cargo fmt --all -- --check` and `cargo test --locked -p workspace_checks`.

Rules for every package: failing test first, with the red run pasted into the agent's report. Every commit names its
WP and question. Every new Rust file begins with `//!` (responsibility, fit, invariants). No `unwrap`; `expect` only
with an `invariant:` message. No catalog identity in production Rust (C5): tests may name identities, and new
mechanism tests use scratch archetypes defined in the test (`ron` strings), so a later rebalance cannot break them. No
function over 100 lines outside tests (C8). Never run a long command in the foreground: background it and read its
log. Check `docs/questions/open/` before each push (Q0044 and Q0045 are answered; look for later answers that change
them).

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
   **Amendment 1:** the scratch catalog also carries the worker's backswing 9 (Q0045 2b) and the Overclock move of
   D-9, so the measurement covers everything WP15 and WP12 will apply. It does **not** carry the turn rates (no rule
   reads them before WP17) or the Phoenix (no matrix battle uses `flying_saucer`). It also leaves out group W8, which moved to
   WP17. Record that scope in the audit's §7.
6. File one owner question for all class D items (next free number at that time), default no change. If the
   measurement shows a failure that is not explained by a group's own effect, file it as a question too.
   **Done:** Q0045, answered 2026-11-04 (Amendment 1).

**Gate.** Documentation gate.

---

### WP4: Data: zealot weapon (Q0036)

**Files (owned):** `units.ron` (`psi_grey` weapon), `trace_tests.rs` (one test restated), goldens, matrix, doc 07
(zealot row), evidence.

**Steps**

1. **Test-only:** restate `a_swing_winding_up_when_another_attacker_kills_the_target_is_cancelled` on a scratch
   melee archetype with a wind-up (it needs one; the zealot will have none). Green before and after. Check
   `sc2_battle_benchmarks_tests.rs` and any other test that assumes the zealot winds up; restate the same way.
2. **Q0044 is decided (option 1, D-6).** Nothing to implement first; battle 33 is re-recorded in step 5 if it
   moves. Check `docs/questions/open/` for any later answer that changes this before starting.
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

1. Edit `pulse_bot`'s `overclock.duration_ticks`: 250 → 268 (`liberty` BehaviorData:680). **Amendment 1:** WP3
   found that `shock_golem` has no Overclock and that the 20-life one sits on `rhino_beetle` (audit §1 item 3).
   WP8 does not touch `rhino_beetle`'s: WP15 removes it and gives `shock_golem` its own at 268 (D-9).
2. Measure; re-record. Expected: no matrix battle lasts past tick 250 of a stim (25b ends near tick 142), so the
   matrix should be bit-identical; `golden_replay.ron` changes after its `pulse_bot` stim at tick 260 plus 250
   (**verify** that the replay runs past tick 510; if it does not, it is unchanged too and the commit says so).
3. Doc 08's `overclock_stim` row says "for 178 ticks (7.1s)"; correct it to 268 ticks (10.7 s).

**Gate.** Common gate.

---

### WP15: Data: the Overclock moves to the Marauder analogue (Q0045 3b; Amendment 1)

**Files (owned):** `units.ron` (`rhino_beetle`, `shock_golem`), `upgrades.ron` (`overclock_stim`), doc 07 (both
rows), doc 08 line 175 (check only; it already names `shock_golem`), evidence.

**Steps**

1. **Check the tests first.** Search `crates/` for tests that stim, or assert the overclock-readiness of,
   `rhino_beetle` or `shock_golem` (`player_view_tests.rs`, `full_match_tests.rs`, `research_and_cancel_tests.rs`,
   the client's `battle_hud_tests.rs` and `battle_card_tests.rs`). Restate any that names `rhino_beetle` as a stimmer
   on a scratch archetype, test-only, green before and after.
2. **Failing test first** (`crates/sim_core/tests/full_match_tests.rs` or `player_view_tests.rs`, whichever already
   holds the research-unlock tests): after `overclock_stim`, a `shock_golem` can stim (life −20, the Overclock
   active for 268 ticks), and a `rhino_beetle` cannot. Red today.
3. Edit as D-9: `rhino_beetle.capabilities.overclock: None`; `shock_golem.capabilities.overclock` with the
   reference values; `overclock_stim` unlocks it on `shock_golem` too. Each value cites its source line.
4. Measure (D-5). Expected: matrix bit-identical; both goldens unchanged (§2.4: nobody stims either identity). If
   anything moves, stop and report: the capability then has an effect the plan did not see.
5. Doc 07: the `rhino_beetle` row loses "Overclock", the `shock_golem` row gains it, if they mention it.

**Size.** S (half a day). **Gate.** Common gate. **Changes no expected result** (to be proved).

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

### WP16: `flying_saucer` becomes the Phoenix (Q0045 4b; Amendment 1)

**Files (owned):** `units.ron` (`flying_saucer`), the audit file (Phoenix rows), doc 07 (row 12), goldens
(`golden_replay_orders.ron`), `high_ground_combat_tests.rs` (only if restated), evidence.

**Steps**

1. **Research (lane C, documentation gate; may run any time after WP3's research).** Audit `flying_saucer` against
   the Phoenix exactly as WP3 audited the others (same chain, citations, conversion, classes), for every field of
   D-10, and add the rows to the audit file's `units.ron` table and a short section "Phoenix (Q0045 4b)". Record the
   Phoenix's `TurningRate` for WP17. Mark **not found** where the effort bound runs out.
2. **Check the users first.** Trace `golden_replay_orders.ron` with the trace tool and record whether its spotter
   ever fires, and at what. Check `high_ground_combat_tests.rs` and any client test or fixture that names
   `flying_saucer` (`crates/client/tests/data/` included) for a dependence on a value that changes; restate such a
   test first, test-only, green before and after.
3. Edit `flying_saucer` per D-10: `sc2_analogue: "Phoenix"`, the values of step 1, ground `weapon: None`, one
   air-only `air_weapon`. Its turn rate stays 999 here; WP17 sets it with the others. Its shield values are WP10's
   and WP11's and stay.
4. Measure (D-5): the matrix must be bit-identical (no battle uses it); re-record `golden_replay_orders.ron` if it
   moved, and say why (for example "the spotter no longer fires at ground units").
5. Doc 07 row 12: the Phoenix's values. Commit message: Q0045 4(b), every changed field with old and new value and
   source, and the golden change.

**Size.** M (1 day, half of it research). **Gate.** Documentation gate for step 1; common gate for the data commit.

---

### WP12: Data: the audit's corrections and the worker's backswing

**Steps.**

1. **Groups and order.** One commit per group of the audit's §5, in its order, then the class D backswing:

   | Order | Group | Values | Touches a matrix identity |
   |---|---|---|---|
   | 1 to 8 | P1, P2, P3, U1, U2, M1, M2, R3 | 3, 127, 31, 8, 1, 3, 2, 9 | no (nothing is trained, researched, harvested or sieged in the matrix; R3 has no matrix identity) |
   | 9, 10 | S1, S2 (`max_speed`) | 5, 5 | yes |
   | 11, 12 | R1, R2 (`hp_regen_per_tick`) | 1, 32 | yes |
   | 13 to 15 | W1, W2, W3 (period; damage point, class B) | 2, 14, 3 | yes |
   | 16 | W4 (damage points never sourced) | 40 | yes |
   | 17 | W5 (backswings never sourced) | 48 | yes |
   | 18 | **D2: `welder_drone` backswing 6 → 9 (Q0045 2b, class D; `liberty` default 0.5 Normal s)** | 1 | yes (`welder_drone` is in battles through W2, W3 and S1) |
   | 19, 20 | W6, W7 (`projectile_speed`) | 2, 21 | yes |
   | — | W8 (turret rates) | 8 | moved to WP17 (commit e): it is the same field kind as the hull rates and needs D-8's unit decision first |

   D2 follows W5 because it is the same field kind with a different cause (an owner decision, not "never sourced").
2. **Each commit:** edit, measure (D-5), re-record, cite, and correct the doc 07 or doc 08 cell that restates the
   value (audit §5 notes). The commit message names the group, its cause, its value count, and every moved
   expectation.
3. **What may be deferred** (D-7, Amendment 1): only values without a single reference, values whose **verify**
   fails, or values that need a mechanism we do not model. Each deferral is listed with its reason in the evidence;
   no value is deferred because it moves an expectation.
4. **W4 and W5 may be split** into one commit per faction if their measurement moves too many battles to explain one
   by one. The cause stays the same in each part.
5. The package ends when every class B and C value and D2 is corrected or deferred with a reason.

**Size.** L (2 to 4 days; 20 commits, most of them small). **Gate.** Common gate per commit.

---

### WP17: Hull turning at StarCraft II rates (Q0045 1b; Amendment 1)

**Files (owned):** `catalog.rs`, `catalog/validation.rs`, `entity.rs`, `world.rs`, `movement/locomotion.rs`,
`combat.rs` (`slew_towards`), `sim_fixtures/src/matchup.rs`, new `crates/sim_core/tests/hull_turning_tests.rs`,
`runtime_catalog_tests.rs` (field names), `units.ron`, `structures.ron`, matrix, goldens, doc 07 §2.3 and its turn
rate cells, `DESIGN.md` (Movement and Combat rows), the audit file (§10), evidence.

**Steps** (D-8; commit letters as in D-8 item 6)

1. **Research (lane C, documentation gate; may run any time after WP3's research).** Add §10 "Turning" to the audit
   file, citing the chain of D-7:
   - each analogue's `TurningRate` (confirm the audit's §4 rows and the 4 classes of D-8 item 3);
   - whether the game makes a unit turn before it moves, and how (a unit flag, a mover field, or engine behaviour;
     **verify** the name in `UnitData` and `MoverData`), for each matrix identity;
   - each weapon's arc and arc slop (**verify** the field names in `WeaponData`) against our `firing_arc_deg`,
     including whether ours is the half angle the loader reads (§2.5), with classes A to E;
   - each turret's rate (confirm W8) and whether a turret's idle return matters (not modelled; list only).
2. **Test-only decoupling.** Find the tests that would break once shipped units turn at finite rates because they
   assert exact ticks with shipped identities (candidates: `weapon_period_tests.rs`, `combat_correctness_tests.rs`,
   `stutter_step_tests.rs`, `attack_move_simulation_tests.rs`, `high_ground_combat_tests.rs`, the
   `sim_fixtures` trace tests). Run them on a scratch catalog with the reference rates **after commit b** to see
   which fail; restate each on a scratch archetype or make it read the archetype, green before and after (D-1
   item 5). This step's commits land before commit f.
3. **Commit a (unit and range).** Failing tests first: validation rejects a hull rate of 0 and of 180.5, a turret
   rate on a weapon without a turret, and a turreted weapon with rate 0 (typed errors naming the field); the loader
   gives 55.99 → 10193 binary units and 20.16 → 3670; a catalog with the old field name fails to load naming the new
   one. The unit test `angles_preserve_finite_legacy_rates_but_reject_nonfinite_values` (`catalog/validation.rs`),
   which accepts 999 on purpose, is replaced by the range tests. Then rename, convert and rewrite the data to 180
   (D-8 items 1 to 3). Prove bit-identical outcomes; explain
   any checksum change (D-8 item 6, row a).
4. **Commit b (one hull rotation per tick).** Failing tests first, in `hull_turning_tests.rs` with scratch archetypes
   defined in the test:
   - `a_unit_facing_away_starts_its_swing_after_turning`: rate 20.16, target behind in range, arc 15 → the swing
     starts on the 9th tick, not the 1st;
   - `a_hull_never_turns_more_than_its_rate_in_one_tick`: a unit that moves and then aims in the same tick turns at
     most its rate in total (today it can turn twice);
   - `a_hull_weapon_turns_at_the_hull_rate_not_a_turret_rate`;
   - `a_turret_turns_at_its_own_rate_while_the_hull_turns_at_its_rate`;
   - `a_moving_unit_keeps_its_path_while_its_heading_follows` (translation not gated, D-8 item 4);
   - `a_rate_of_180_turns_any_facing_in_one_tick` (guard; green before and after);
   - determinism: two worlds stepped alike give equal checksums, and the budget is not hashed.

   Implement D-8 item 4 (functions under 100 lines, C8). Prove bit-identical on shipped data (all 180).
5. **Commit c (matrix spawn facing).** Failing test first in `sim_fixtures` (a two-group battle spawns each group
   facing the other's centre); add `SimWorld::spawn_unit_facing` with `///` docs (binary angle, counter-clockwise from
   +x, world frame). Prove bit-identical outcomes.
6. **Commit d (firing arcs),** only if step 1 found class B or C arcs: hull-weapon arcs in one commit (latent, proved
   bit-identical), turreted-weapon arcs in another (measured).
7. **Commit e (turret rates, audit group W8).** Measure; re-record. Expected: battles with `railgun_crawler`;
   structures appear in no matrix battle.
8. **Commit f (hull rates).** The 42 values of D-8 item 3 (41 units and the Phoenix). Before committing, measure
   each rate class alone on a scratch catalog (84, 55.99, 40.32, 20.16) as D-5 item 5 does for multi-field steps,
   and record the per-class effect. Measure the whole matrix on 20 seeds and battle 33 on 68; re-record every moved
   expectation, battle 33 per D-6, and both goldens. Expected: end ticks move in most battles by a few ticks; outcome
   counts may move in close battles and where slow turners fight. No survivor expectation is widened without the
   measured reason (D-1 item 6). If a battle flips its winner, stop and report it before re-recording. (Amendment 4, orchestrator 2026-11-05: "a battle flips its winner" means the battle's winner over its seeds, as WP17 read it. WP17's single-seed flips on seeds of record in battles 32, 33 and 35 (evidence §14.7) were reported and accepted: no battle's overall winner changed, battles 32 and 35 accept either side, and battle 33 is re-recorded under D-6.)
9. **Commit g,** only if step 1 found turn-before-move and could specify it: rule (latent, `turn_before_move: bool`
   in the archetype, default false, tests first), then data (measured). Otherwise, if step 1 found it but could not
   specify it, file the owner question of D-8 item 4.
10. Docs: doc 07 §2.3 rewritten (D-8 item 1) and the roster's turn-rate cells; `DESIGN.md`'s Movement and Combat rows
    say that the hull turns at a per-tick rate and that hull weapons wait for it.

**Size.** L (3 to 4 days; research half a day). **Gate.** Documentation gate for step 1; common gate for every
other commit.

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
   **After WP4 (Amendment 1):** close Q0044. Its resolution: option 1 as D-6 states it; the first re-recording is
   WP4's commit; every later commit that moves battle 33 (WP6, WP7, WP11, WP12, WP17, WP14) applies the same rule
   and updates the comment's distribution. If WP4 leaves battle 33 inside its expectation, close Q0044 with the
   first commit that does re-record it.
   **After WP17 (Amendment 1):** close Q0045, with one line per item: 1(b) WP17 commits a to f (and g if any);
   2(b) WP12 group D2; 3(b) WP15; 4(b) WP16; and the general direction applied through WP12's deferral rule (D-7).
4. **After WP3 step 5 (Amendment 1):** correct the battle 33 investigation's §1.2 layer chain to the audit's
   (`core → liberty → swarm → void → voidmulti`, audit §2.2), with a note that none of its figures change.
5. **At the end of lane B:** `DESIGN.md` Combat row (remove "Changed outcomes await the owner (Q0007)"; describe the
   weapon timing rule and hull turning) and Catalog data row; this plan's row in `docs/plans/README.md` (`Done` with
   commits, or `Partly done` naming WP14).

**Amendment (plan `2026-11-05-sc2-deviation-fixes` WP0, 2026-11-05; owner's standing rule, Q0051).** When steps 1
and 3 close Q0035 and Q0045, what they leave open is called a recorded bug, never "accepted" or "kept":
- Q0035's resolution and the Combat row line of step 1 read "the contact rules of `d4109e2` stay for now; how
  StarCraft II spaces melee attackers is recorded bug B-50 in plan `2026-11-05-sc2-deviation-fixes` (WP16)".
- Q0045's 1(b) line adds "the remaining turning differences are recorded bugs B-28 (six mounts) and B-29
  (acquisition arc) in plan `2026-11-05-sc2-deviation-fixes` (WP10); B-31 (`StationaryTurningRate`) is fixed by plan
  `2026-11-05-turn-before-move` WP5 and WP6".
- Step 5 is absorbed into that plan's WP19. The `DESIGN.md` Combat and Movement rows already point at its bug
  register, and a rewrite of either row must keep that pointer.

**Gate.** Documentation gate.

---

### WP14 (low priority, last): StarCraft II scan ranges (Q0037)

Starts only after WP17 (Amendment 1: hull turning changes how fast a unit engages what it acquires, so the scan
ranges are measured on top of it), when the owner's priority allows. It may be split into its own plan if the mechanism study
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
3. **Whole matrix** on a scratch catalog with every reference scan range (D-5), against the state after WP17.
4. **Report** in the evidence file. If every moved battle is explained by the mechanism, apply the values in one
   data commit ("scan ranges to StarCraft II", cause stated), re-recording per D-6; otherwise file the unexplained
   part as an owner question and stop.

**Gate.** Common gate for code and data; documentation gate for the report.

**Outcome (2026-11-05, after sc2-deviation-fixes WP4).** Steps 1 to 4 done
([evidence](../evidence/2026-11-04-combat-sc2-alignment.md) §15, reference table in the
[audit](../evidence/2026-11-04-catalog-time-audit.md) §11); the trace tool gained targeting measures (`b1d28e6`).
The mechanism in battle 33 is explained (timing, not focus), but the game's measuring rule (centre or edge) is not
sourced and decides what the values do, so no value is applied: [Q0057](../questions/open/0057-scan-range-measuring-rule.md),
default no change; bug B-12 of plan [`2026-11-05-sc2-deviation-fixes`](2026-11-05-sc2-deviation-fixes.md) stays open
(its Amendment 6).

---

## 7. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| WP1 is not latent after all (a weapon under some upgrade's `attack_cooldown_delta` has backswing ≥ cooldown) | the rule commit moves results | D-1 item 2: WP1 stops, reports the case, and the plan is amended (the move gets its own explained commit) |
| WP2 moves battles beyond the probe's list (the probe used period − 1, not the real fix) | more re-recording | measured, not guessed; each named in the commit |
| Battle 33's expectation churns across WP4 to WP17 | several fixture edits | D-6 as decided by Q0044 (option 1): smallest ranges on the seeds of record, distribution in the comment |
| The trace inference rule needs period > damage point after modifiers | miscounted releases in the evidence | WP1's validation rule for base values; WP0 documents the bound; no shipped modifier breaks it (verify in WP0) |
| The audit finds many class C values | WP12 grows | groups, one commit each; class D decided by the owner (Q0045: align); WP12 may be split across sessions |
| A client test or golden depends on a marine or zealot value (presentation, battle card) | a data commit breaks a client test | the common gate runs the whole workspace; such a test is decoupled first (D-1 item 5) |
| Determinism on other targets | a re-recorded golden fails on Windows, macOS or wasm | no float enters state; CI's determinism jobs re-verify; a red job stops the chain |
| Hull turning moves many battles at once (WP17 commit f) | a large re-recording that is hard to review | spawn facing first (commit c) removes the fixture artefact; per-rate-class scratch measurements explain the parts; a battle that flips its winner stops the step |
| Hull turning breaks tests that assumed instant facing with shipped units | the data commit fails tests for a reason that is not its cause | WP17 step 2 finds them with the reference rates after commit b and restates them first, test-only |
| The game gates movement on facing and the data does not state the rule | a rule we cannot specify from the source | D-8 item 4: no gating by default, an owner question with that default, no guessing |
| Two systems turning the hull in one tick double the rate | units turn faster than the reference | the per-tick budget of D-8 item 4 and its test `a_hull_never_turns_more_than_its_rate_in_one_tick` |
| The Phoenix's air-only weapon changes the orders golden or a spotter test | re-recording outside the matrix | WP16 step 2 traces the spotter first and restates tests before the data commit |
| A test or HUD names `rhino_beetle` as a stimmer | WP15's data commit breaks it | WP15 step 1 restates it first |
| WP12 grows (20 commits, W4 and W5 move many weapons) | long lane B | groups are small apart from P2, W4, W5; W4 and W5 may be split per faction; groups touching no matrix battle go first |

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
- **F-5 (Amendment 1): Phoenix abilities and moving attacks.** The Phoenix attacks while moving and casts Graviton
  Beam. Neither is modelled; WP16 aligns its values only. Each would need its own rule, plan and measurement.
- **F-6 (Amendment 1): turret idle behaviour and acceleration of turning.** Whether turrets return to a rest angle,
  and whether the game accelerates turning (D-8 assumes a constant rate per tick), are listed by WP17's research and
  not modelled.
- **F-8 (WP17): turning the game models and we do not.** From audit §10: the acquisition arc `Arc` ÷ 2 beside the
  engaged tolerance, and the six weapons whose mount differs (Hellion, Immortal, Colossus, Lurker and Phoenix
  turrets modelled as hull weapons; the Thor's air weapon modelled as a turret). Each needs its own rule or data
  commit and measurement. `StationaryTurningRate` and `TurnBeforeMove` (Q0050), formerly listed here, are done by
  plan [`2026-11-05-turn-before-move`](2026-11-05-turn-before-move.md) (WP1, WP3, WP5, WP6); turn-before-move made
  the stationary rate observable, since these units turn in place, and the owner's standing rule of 2026-11-05
  (follow StarCraft II unless a deviation is named) brought it in.
- **F-7 (Amendment 1): our own units.** The owner plans original units later (Q0045). Until then every identity with
  a counterpart follows it; an original unit later will be audited as class E and needs no change to these rules.
