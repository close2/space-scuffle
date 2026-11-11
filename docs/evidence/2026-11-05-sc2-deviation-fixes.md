# Evidence: StarCraft II Deviation Fixes

Measurements and research for plan [`2026-11-05-sc2-deviation-fixes`](../plans/2026-11-05-sc2-deviation-fixes.md),
one section per commit that can reach a unit a matrix battle spawns (plan D-3). The method is combat alignment D-5
([evidence](2026-11-04-combat-sc2-alignment.md) §14): the whole matrix
(`crates/sim_core/tests/fixtures/matchup_matrix.ron`, 50 battles) on 20 seeds each (the 4 of record plus 16
jittered), and battle 33 on 68 seeds (the 4 of record plus 64), both from release builds of
`crates/sim_core/examples/matchup_trace.rs`:

```bash
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --all-battles --seeds 16 --jobs 8 --out ~/scratch/sc2dev_wp1/<step>/matrix \
    --compare ~/scratch/sc2dev_wp1/<prev>/matrix/matrix.csv
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/sc2dev_wp1/<step>/b33
```

StarCraft II data are SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), layer chain
`core → liberty → swarm → void → voidmulti`, cited as `layer` File:line, as in the catalog time audit
([`2026-11-04-catalog-time-audit.md`](2026-11-04-catalog-time-audit.md) §2.2).

In the tables, "Wins blue/red" counts the seeds each side won over the 20; survivors and end tick are means over the
20 seeds; the lead hull is the first unit's life, averaged over the seeds it lived on (that count in brackets).

## 0. Baseline

The state after WP0 (`1fc72b8`): the turn-before-move package and combat alignment WP0 to WP12 and WP15 to WP17
merged. Whole matrix: 1000 runs, expectation failures 0 on the seeds of record and 2 over all seeds (both battle
33's). Battle 33 on 68 seeds: 39 / 20 / 9 (marine wins / zealot wins / unresolved), marine survivors 3.87 (SD 3.43),
zealot survivors 1.01 (SD 1.37); seeds of record 0-3, 2-0, 1-2, 9-0 (marines – zealots). These equal combat alignment
evidence §14.7: turn before move moved no matrix battle that this measurement sees.

## 1. WP1: a turn that aligns the weapon starts the swing on that tick (B-1, M-22)

- **Rule.** `combat.rs`: a `Ready` weapon that is not aligned enters `RotatingToTarget`, turns, and now checks its
  arc again in the same call, as a weapon that was already rotating does. Both go through one helper,
  `slew_and_try_swing`. Before, it turned and waited for the next tick even when this tick's turn had aligned it
  (remediation plan §4.2 M-22; combat alignment evidence §14.4 item 1).
- **StarCraft II.** A weapon may fire once its target is within `Arc` ÷ 2 + `ArcSlop` of its facing (`CWeapon`
  `Arc`, `ArcSlop`; audit §10.3). The turn and the attack are one motion: nothing in `CWeapon` adds a game loop
  between the facing that satisfies the arc and the start of the attack, so the attack starts on the loop the
  facing first satisfies it.
- **Tests first** (`crates/sim_core/tests/hull_turning_tests.rs`, scratch archetypes). On the old rule:
  `a_turn_that_aligns_the_weapon_starts_the_swing_on_that_tick` failed (swing on tick 2, expected 1),
  `a_turret_turn_that_aligns_the_weapon_starts_the_swing_on_that_tick` failed (2, expected 1), and the guard
  `a_turn_that_does_not_align_waits` passed (tick 3). All three pass with the rule.
- **Not latent** (plan WP1). Whole matrix against §0: 19 battles moved; no battle's winner over its seeds changed.
  Expectation failures 0 on the seeds of record, 1 over all seeds (battle 33's).

| Battle | Seeds moved | Wins blue/red | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|---|
| 6b_2_roaches_vs_hydra | 4 | 20/0 | 2.00 – 0.00, same | 80.2 → 80.0 | 112.58 → 113.12 / — |
| 9a_hellion_vs_4_lings | 7 | 20/0 | 1.00 – 0.00, same | 95.0, same | 25.40 → 23.80 / — |
| 10_2_hellions_vs_8_lings | 1 | 20/0 | 2.00 – 0.00, same | 61.2, same | 70.40 → 70.20 / — |
| 11a_2_banelings_vs_5_marines | 3 | 0/9 | 0.00 – 2.25, same | 8.2 → 8.1 | — / 10.00, same |
| 11b_2_banelings_vs_marauder | 9 | 0/20 | 0.00 – 1.00, same | 8.4 → 8.0 | — / 95.00, same |
| 12a_colossus_vs_6_marines | 1 | 20/0 | 1.00 – 0.00, same | 81.7, same | 238.15 → 237.90 / — |
| 21_lurker_vs_6_marines | 1 | 20/0 | 1.00 – 0.00, same | 103.6, same | 72.81 → 72.56 / — |
| 23a_zealot_vs_6_lings | 20 | 0/20 | 0.00 – 2.00, same | 275.0 → 271.3 | — |
| 24a_roach_vs_5_lings | 20 | 0/20 | 0.00 – 3.00, same | 289.8 → 288.5 | — |
| 24b_upgraded_roach_vs_5_lings | 6 | 20/0 | 1.00 – 0.00, same | 328.0, same | 26.62 → 25.42 / — |
| 27_100_lings_vs_8_ultralisks | 18 | 0/20 | 0.00 – 8.00, same | 216.8 → 217.6 | — / 421.72 → 418.14 |
| 28_100_lings_vs_10_immortals | 20 | 20/0 | 26.15 → 27.20 – 0.00 | 688.4 → 678.0 | 35.00 (1) → 35.00 (4) / — |
| 29_100_lings_vs_12_stalkers | 20 | 20/0 | 68.50 → 68.70 – 0.00 | 451.9 → 448.6 | 34.27 (15) → 35.00 (19) / — |
| 30_20_roaches_vs_10_immortals | 2 | 0/20 | 0.00 – 10.00, same | 239.7 → 239.1 | — / 146.95 → 149.95 |
| 31_15_marauders_vs_15_stalkers | 17 | 11/9, same | 1.85 – 1.45 → 1.80 – 1.70 | 669.4 → 672.4 | 57.00 (4) → 80.80 (5) / — |
| 32_30_marines_vs_15_banelings | 3 | 5/10 → 4/11 | 1.45 – 0.90 → 1.95 – 0.95 | 50.0 → 49.4 | 27.50 (4) → 36.25 (4) / — |
| 33_24_marines_vs_12_zealots | 17 | 11/6 → 10/6 | 3.55 – 1.20 → 3.55 – 1.05 | 442.2 → 447.7 | 38.60 (5) → 34.33 (6) / — |
| 34_20_hydras_vs_12_mutas | 8 | 20/0 | 19.30 → 19.25 – 0.00 | 184.4 → 184.6 | 90.00, same / — |
| 35_12_vikings_vs_6_colossi | 1 | 20/0 | 12.00 – 0.00, same | 280.4, same | 125.00, same / — |

  - The melee and zergling fights get back what the arcs had cost them (evidence §14.4): 23a and 24a end 3.7 and
    1.3 ticks sooner, back to their values before the arcs; 9a's and 24b's lead hulls return to 23.80 and 25.42.
    In 28 and 29 the zerglings strike sooner after each turn: 28 ends 10 ticks sooner with one more zergling alive.
  - **Single-seed flips** (none on a seed of record outside battle 33, no battle's winner over its seeds): battle
    31 seeds 9, 10, 14 and 15 (a close fight with both winners, 11/9 before and after); battle 32 seed 14 (1-0 →
    0-1; banelings win overall, 10 → 11 of 20); battle 33 seeds 3, 13 and 17 and the seeds of record below.
- **Battle 33, 68 seeds:** 37 / 20 / 11 (was 39 / 20 / 9), marine survivors 3.62 (SD 3.16; was 3.87, SD 3.43),
  zealot survivors 1.00 (SD 1.32), mean end tick 443.1 (was 438.9). Seeds of record 0-3 (same), 0-2 (was 2-0), 0-2
  (was 1-2, unresolved), 9-0 (same). The expectation `blue: (0, 9), red: (0, 3)` still holds on all four and is the
  smallest that does, so it is unchanged; the comment's distribution is rewritten (combat alignment D-6).
- **Golden replays.** `golden_replay.ron` re-recorded: checkpoints 1 to 3 unchanged; from checkpoint 4 (ticks 151
  to 200, the first fight) every checkpoint changed. `golden_replay_orders.ron` unchanged. Debug, release and
  `wasm32-wasip1` reproduce both.

## 2. WP2: air-only weapons do not fire at the ground (B-25, B-26, B-27 part)

### 2.1 Research

| Unit (analogue) | Its weapon in the modelled mode | `TargetFilters` | Where |
|---|---|---|---|
| Viking (`vector_interceptor`), fighter mode | `LanzerTorpedoes` | `Air,Visible;Missile,Stasis,Dead,Hidden,Invulnerable` | `liberty` WeaponData:271; the unit's `WeaponArray` `liberty` UnitData:6885 |
| Corruptor (`sky_leviathan`) | `ParasiteSpore` | `Air,Visible;…` | `liberty` WeaponData:567; UnitData:11274 |
| Liberator (`sky_bastion`), fighter mode | `LiberatorMissileLaunchers` | `Air,Visible;…` | `void` WeaponData:326; UnitData:3317 |
| Liberator in Defender Mode (`LiberatorAG`, a separate unit the morph `LiberatorMorphtoAG` turns it into) | `LiberatorAGWeapon` | `Ground,Visible;Structure,…` | `void` WeaponData:335; UnitData:3387 |

`voidmulti` overrides only `DamagePoint` of `LanzerTorpedoes` and `ParasiteSpore` and the range of
`LiberatorAGWeapon` (`voidmulti` WeaponData:133, :191, :136); no later layer changes a filter. The Viking reaches the
ground only as `VikingAssault`, whose weapon is `TwinGatlingCannon` (`liberty` UnitData:6814): Assault Mode, bug B-46.

**Catalog.** A unit's ground weapon is `ground_weapon`, or else the legacy `weapon` slot
(`UnitArchetypeDef::into_archetype`); `air_weapon` is its air weapon. The three archetypes carried their air weapon
in `weapon` as well, so it fired at ground units; `sky_bastion` also had a `ground_weapon` (Defender Mode's,
75 damage, range 9, period 29 ticks, damage point 2, backswing 9, projectile speed 2.24, `LaserBeam`) active in
every mode. A cliff walker (the Colossus analogue) is in both planes, so an air weapon still reaches it
(`weapon_against`, `CliffWalker`).

### 2.2 B-79, found here: an air-only weapon froze while it had no target

- **Finding.** The data change alone moved battle 35 on one seed (seed 13: end tick 296 → 297). The per-unit trace
  showed why: a Viking whose target died during its backswing kept `remaining_ticks` 4 for every following tick.
  `advance_weapon_cycles` (`combat_resolution.rs`) resolves the weapon of a unit with no target against the ground
  plane. A unit with no ground weapon found none and skipped its weapon step, so its cooldown and backswing stood
  still until it next had a target. In StarCraft II a cooldown runs from the start of the attack whatever the unit
  does (combat alignment D-2), so this is a rule bug, not a consequence of the data. The Phoenix analogue
  (`flying_saucer`, air weapon only) and the air-only defence structures already had it.
- **Rule commit, before the data.** A unit with no target resolves its weapon against the ground plane if it has a
  ground weapon, else against the air (`idle_weapon_plane`).
- **Test first:** `combat_correctness_tests::b79_an_air_only_weapon_keeps_cycling_after_its_target_is_gone` (an
  archetype found by property: air mover, an air weapon and no ground weapon). On the old rule the cooldown stood
  at 17 two ticks after the target died (expected 15); it passes with the rule.
- **Measurement, on the catalog before WP2.** Whole matrix bit-identical to §1; battle 33 bit-identical
  (37 / 20 / 11). No matrix battle had an air-only unit lose its target mid-cycle before the data change.

### 2.3 Data: no ground weapon on the three air-only units

- **Change** (`assets/catalog/units.ron`): `weapon: None` for `vector_interceptor`, `sky_leviathan` and
  `sky_bastion`; `sky_bastion`'s `ground_weapon` removed. Its values are listed in §2.1 for WP18, which adds Defender
  Mode (B-27's mode part stays open in the bug register).
- **Tests first** (`crates/sim_core/tests/weapon_catalog_rules_tests.rs`): the archetypes are found through their
  `sc2_analogue` (the StarCraft II unit name), never by slug, and checked against the filters of §2.1.
  `b25_to_b27_units_with_an_air_only_starcraft_weapon_have_no_ground_weapon` failed on the old data
  (`vector_interceptor` fires at ground units) and `b25_to_b27_air_only_units_leave_a_ground_unit_unharmed` failed
  (`vector_interceptor` killed a `welder_drone` standing 3 away). Both pass on the new data.
- **Measurement** against §2.2: whole matrix bit-identical (battles 12c, 18, 19 and 35 have these units, against air
  units and the cliff-walking Colossus analogue, which an air weapon still reaches); battle 33 bit-identical. Neither
  golden replay has these units; both unchanged.
- **Not in the plan.** Without §2.2 the data change moved battle 35 on one seed (end tick 296 → 297, same outcome)
  through B-79, a second cause. The rule was therefore committed first, and this commit moves nothing the matrix
  measures. Its effect is in skirmish play: the three units no longer shoot ground units.

## 3. WP3: Stimpack only from the catalog, its life rule, start energy (B-32, B-33, B-34, B-72)

### 3.1 Commit a: no hard-coded Stimpack (B-32)

- **Change.** `Entity::apply_stimpack` (10 life, 250 ticks, ×1.5 speed and attack rate, written in Rust) is deleted.
  `Entity::issue_order` no longer routes an immediate `UseStim` there: it returns
  `EntityAdmissionError::StimOutsideWorld` and changes nothing. A stim is admitted only by `SimWorld`'s command path
  (`world/commands.rs` `apply_stim`), which reads the archetype's catalog `overclock` spec (the Marine analogue's
  10 life and 268 ticks, `liberty` AbilData:1229, BehaviorData:680; the Marauder analogue's 20, BehaviorData:604)
  and its research unlock.
- **Test restated first** (test-only commit): `checked_movement_tests` reached the overclock admission through the
  removed path; it now calls `apply_overclock` with its own scratch spec and asserts the same typed error.
- **Test first:** `an_immediate_stim_issued_to_an_entity_outside_the_world_is_rejected` failed on the old rule
  (`Ok(true)`: the entity stimmed with the hard-coded values) and passes now.
- **Latent, proved.** Matrix and battle 33 bit-identical to §2.3 (37 / 20 / 11); both golden replays unchanged
  (their `UseStim` goes through `PlayerCommand`). Nothing in the workspace called the removed path outside that test.

### 3.2 U-8: the life rule (B-33), not sourced

- **Data.** `Stimpack` costs `<Vital index="Life" value="10"/>` (`liberty` AbilData:1229) and `StimpackMarauder` 20
  (AbilData:983). What the engine does when the unit's life is at or below the cost is not in the data.
- **Liquipedia** ([Stimpack](https://liquipedia.net/starcraft2/Stimpack), wikitext, read 2026-11-05): "You can never
  kill the unit itself by using Stimpack". It does not say whether the stim is refused or the unit is left with some
  life. The [Marine](https://liquipedia.net/starcraft2/Marine_(Legacy_of_the_Void)) page gives only the cost.
- **Decision** (plan D-5 item 2): not specifiable from the sources, so no rule commit. Owner question
  [Q0053](../questions/closed/0053-stimpack-at-low-life.md) proposes an in-game measurement, default "no change, bug
  B-33 stays recorded". Today's rule (refused at 1 life or less, the unit keeps at least 1) fits the Liquipedia
  sentence; whether it allows more than the game is what the question asks.
- **Answered** (owner, chat, 2026-11-05): "stim can never kill a unit it therefore can't be activated with too
  little hp". Rule (a) in `ad0dcde`: the stim is refused, changing nothing, unless life is strictly greater than the
  cost. Matrix (1000 runs, 16 seeds) bit-identical to the commit before, both golden replays unchanged. B-33 fixed.

### 3.3 Commit c: the start energy is catalog data (B-34, latent)

- **Change.** A required `start_energy` on every unit and structure definition (`catalog.rs`), validated finite,
  nonnegative and at most `max_energy` (`validation.rs` `start_energy`), loaded as `UnitArchetype::start_energy`;
  `Entity` spawns with it instead of `max_energy`. Data: `start_energy` equal to `max_energy` on all 42 units and 49
  structures, so behaviour is unchanged. A catalog without the field fails to load (plan D-6).
- **Test restated first** (test-only commit): two `transport_ownership_tests` cast an energy ability at once on a
  fixture unit and relied on spawning at the maximum; the fixture now fills the pool itself.
- **Test:** `weapon_catalog_rules_tests::b34_start_energy_must_lie_between_zero_and_the_maximum` (a start above the
  maximum and a negative start rejected, for units and structures).
- **Latent, proved.** Matrix and battle 33 bit-identical to §3.1; both golden replays unchanged (a unit's energy is
  hashed, and every unit still starts at its maximum).

### 3.4 Commit d: the reference start energies (B-34, B-72)

- **Research** (`EnergyStart`, `UnitData`; `EnergyMax` is 200 for each unless stated, `liberty` unless stated):

| Analogue (archetype) | `EnergyStart` | Where |
|---|---|---|
| Phoenix (`flying_saucer`) | 50 | UnitData:3727 |
| Ghost (`specter_android`) | 75 | `voidmulti` UnitData:600 |
| Medivac (`repair_shuttle`) | 50 | UnitData:5439 |
| Banshee (`cloak_gunship`) | 50 | UnitData:5372 |
| Raven (`omni_probe_drone`) | 50 | UnitData:5633 |
| Sentry (`prism_weaver`) | 50 | UnitData:11296 |
| High Templar (`cerebral_elder`) | 50 | UnitData:3250 |
| Queen (`hive_matron`) | 25 | UnitData:11414 |
| Infestor (`mycelium_shaman`) | 75 | `voidmulti` UnitData:831 |
| Viper (`tongue_chameleon`) | 50 | `swarm` UnitData:2334 |
| Orbital Command (`orbital_relay_dome`) | 50 | UnitData:10513 |
| Nexus (`citadel_nexus_dome`) | 50 | `voidmulti` UnitData:1096 |
| Shield Battery (`harmonic_recharge_dome`), `EnergyMax` 100 | 50 | `voidmulti` UnitData:1563 |

  Each value was read along the layer chain; the last layer that sets it wins (the Ghost's and the Infestor's 75 are
  `voidmulti` overrides). These are all 13 archetypes with energy. The plan named the units; the three structures
  are included because commit c gave structures the same field, and leaving them at their maximum would have kept
  B-34 for them.
- **Test first:** `weapon_catalog_rules_tests::b34_everything_with_energy_starts_with_its_starcraft_energy` (each
  archetype found through its analogue, spawned, its energy compared; and the table must cover every archetype
  with energy). It failed on commit c's data (`flying_saucer` started with 200; with the unit values alone,
  `orbital_relay_dome` with 200) and passes now. The rejection case of §3.3 was restated in a test-only commit
  before (its edit produced 150 from a start of 50, which loads).
- **Measurement** against §3.3: whole matrix bit-identical (no matrix battle spends energy); battle 33
  bit-identical.
- **`golden_replay_orders.ron` re-recorded:** its `blue_spotter` (`flying_saucer`) starts with 50 energy, which is
  hashed, so every checkpoint changed from checkpoint 1. No behaviour changed: with `energy` moved to the unhashed
  fields of the encoding in a throwaway build, the 60 checkpoints on this commit's data equal those on commit c's.
  `golden_replay.ron` unchanged (none of its units has energy). Debug, release and `wasm32-wasip1` reproduce both.
- **Doc 03** §1.1 states the per-unit start and names `start_energy` (B-72).

## 4. WP4: target selection (B-7 to B-11, B-76)

Measurements under `~/scratch/sc2dev_wp4/<step>`. The base is §3.4 (WP1 to WP3 merged with `main`): whole matrix
bit-identical to §3.4, battle 33 37 / 20 / 11.

### 4.1 Research

**Rule** (Liquipedia [Automatic Targeting](https://liquipedia.net/starcraft2/Automatic_Targeting), wikitext, read
2026-11-05). A unit considers the valid targets within its scan range and ranks them by: (1) threat, (2) attack target
priority, (3) primary weapon, (4) only if it lost its previous target, its tie-break. "Once a target is chosen, it
will remain the target until it ceases being a valid target …, leaves the weapon range or a higher-priority target
enters the weapon range"; the section on criterion 4 says the unit keeps its target until "a more important target
(according to the first 3 criteria) enters Scan Range". A threat has "a weapon that can return fire, i.e., the
attacker is a legal target for the weapon", or the type's flag "Always a Threat to Attackers". Workers have the
*defensive* acquire level: they acquire automatically only while attack-moving, and "are not classified as threats
by other units".

**Data** (`UnitData`, `WeaponData`, layer chain as above; resolved with the audit's layered resolver,
`~/scratch/sc2dev_wp4/scripts/targeting.py`, not committed):

| Field (`StarCraft` II) | Catalog field | Values for the modelled analogues |
|---|---|---|
| `CUnit` `AttackTargetPriority` | `attack_target_priority` | every unit 20 (each set in its own `CUnit`, for example Marine `liberty` UnitData:4833, SCV :4738, Medivac :5460, Overlord :7372, Viper `swarm` UnitData:2350, Adept `void` UnitData:3210); Planetary Fortress 20 (:10660), Photon Cannon 20 (:11051), Spine Crawler 20 (:8614), Missile Turret 19 (:6032), Spore Crawler 19 (:8748), Bunker 19 (:6153); every other modelled structure 11 (for example Command Center :10376, Nexus :4183, Hatchery :8207) |
| `CUnit` flag `AlwaysThreatens` | `always_a_threat` | High Templar (`liberty` UnitData:3276), Raven (:5584), Infestor (:10080), Viper (`swarm` UnitData:2319), Lurker (`voidmulti` UnitData:971). The Medivac's flag (`liberty` :5452) is cleared in `swarm` UnitData:1704. No modelled structure |
| `CUnit` `DefaultAcquireLevel` | `acquire_level` | `Defensive`: SCV (`liberty` UnitData:4726), Probe (:3065), Drone (:7034); every other unit the `core` default `Offensive` (`core` UnitData:31) |
| first `CUnit` `WeaponArray` entry | `primary_weapon` | Thor: `JavelinMissileLaunchers` (UnitData:5270; `Air` filter, `liberty` WeaponData:216) before `ThorsHammer`; Queen: `AcidSpines` (UnitData:11478; `Air`, WeaponData:104) before `Talons`. Both `Air`, as Liquipedia says. Every other modelled unit has a single `StarCraft` II weapon per plane (or a second entry that is a melee or building variant on the same plane: Roach, Hydralisk, Ultralisk, Baneling, Reaper), so `Either` |
| `CWeapon` `AcquirePrioritization` | `acquire_tie_break` | `ByAngle`: `JavelinMissileLaunchers`, `ThorsHammer` (`liberty` WeaponData:217, :230), `KaiserBlades`, `Ram` (:435, :445), and the unmodelled Battlecruiser, Hellbat and Thor high-impact weapons (:281, :293; `swarm` :137; `void` :80). `ByDistanceFromTarget`: `ThermalLances` (`liberty` :498) and `ThermalLanceAir` (`void` :128), the Colossus. No other weapon sets it; `core` WeaponData sets no default, so the engine default is the closest target |

- **Liquipedia and the data disagree on two units.** The page lists the Immortal and the unsieged Siege Tank with
  the Colossus as choosing the candidate closest to the previous target; at 5.0.14 neither `PhaseDisruptors` nor
  `90mmCannons` sets `AcquirePrioritization`. The data win (plan, "What StarCraft II means here"): only the
  Colossus analogue is `ClosestToPrevious`.
- **Not reachable here.** The filled Bunker's 20 (the catalog's Bunker carries no passengers), an uprooted Spine
  Crawler's 19 (it cannot uproot), the Widow Mine's and Interceptor's 19 and the Larva's and cocoons' 10 (not
  modelled), and an SCV that repairs a threat inheriting its threat and priority (no repair order exists; B-69).
- **Found: B-80.** A *defensive* unit (a worker) acquires targets automatically while idle; in `StarCraft` II it
  does so only while attack-moving. WP4 models the acquire level's effect on threat only. Recorded in the register.

### 4.2 Commit a: the targeting fields, latent

- **Change.** `attack_target_priority` (required on units and structures), `always_a_threat` and `acquire_level`
  (optional, engine defaults), `primary_weapon` and `acquire_tie_break` (required on units; structures take the
  engine defaults, which no modelled structure overrides) in `catalog.rs`, the enums in `catalog/targeting.rs`;
  validation requires both a ground and an air weapon for a named primary slot. Data reproduce the old categories:
  armed 20, unarmed units 10, unarmed structures 11; `compute_threat_priority` read the unarmed category from the
  field (no unarmed detector structure exists, so the old 15 was unreachable).
- **Latent, proved.** Matrix (1000 runs) and battle 33 bit-identical to the base; both golden replays unchanged.

### 4.3 Commit b: a threat is an enemy that can hit me (B-7, B-76)

- **Rule.** `TargetPriorityKey` is (threat, attack target priority, distance, id); `combat::is_threat_to` replaces
  `compute_threat_priority` (armed and attacking 20, armed 19). The retention rule is unchanged here.
- **Tests first** (`target_selection_tests.rs`, scratch archetypes): `b7_an_enemy_whose_weapon_cannot_hit_me_is_no_threat`,
  `b7_an_enemy_flagged_always_a_threat_is_a_threat_unarmed`, `b7_an_attacking_enemy_is_no_greater_threat_than_an_idle_one`
  and `b7_a_defensive_enemy_threatens_only_while_attack_moving` all failed on the old rule (each chose the other
  candidate) and pass.
- **Measured** against §4.2: one battle moved; no battle's winner over its seeds changed; battle 33 bit-identical
  (37 / 20 / 11); both golden replays unchanged.

| Battle | Seeds moved | Wins blue/red | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|---|
| 35_12_vikings_vs_6_colossi | 20 | 20/0 → 13/0 | 12.00 – 0.00 → 12.00 – 0.35 | 280.4 → 504.6 | 125.00, same / — |

  The colossi cannot hit the Vikings, so none is a threat to them; the old rule had ranked a colossus holding an
  attack-move order (20) above one that had stopped (19). On 7 seeds, 3 of them of record, one colossus now flees
  out of the Vikings' scan range and lives to the time limit (tick 900); the mechanism is inferred from the rule,
  not traced per unit. The expectation `red: (0, 1)` and the fixture's claim
  ("a helpless colossus may flee out of their scan range") hold; the Vikings still lose nothing.

### 4.4 B-7 data: always a threat, defensive workers

- **Data** per §4.1. **Test first:** `weapon_catalog_rules_tests::b7_threat_flags_and_acquire_levels_follow_starcraft`
  failed on the old data (`welder_drone` `Offensive`) and passes.
- **Measured** against §4.3: matrix and battle 33 bit-identical; goldens unchanged.

### 4.5 Commit c: the reference attack target priorities (B-8, B-76)

- **Data** per §4.1: the seven unarmed units 10 → 20; Missile Turret and Spore Crawler analogues 20 → 19; Bunker
  11 → 19. **Test first:** `b8_every_archetype_has_its_starcraft_attack_target_priority` failed on the old data
  (`repair_shuttle` 10) and passes.
- **Measured** against §4.4: matrix and battle 33 bit-identical; goldens unchanged.

### 4.6 Commit d: keeping the target (B-9)

- **Rule tried.** A unit keeps a valid target inside weapon reach unless a candidate in scan range outranks it;
  once the target is out of reach or lost, it takes the best candidate by the whole key, the closest among equals.
  `RETARGET_DISTANCE_MARGIN` (1.0) deleted. Against the old rule the only difference is a target out of reach while
  no equal candidate is in reach: the old rule kept chasing it unless a rival was more than 1.0 closer, the new one
  takes the closest. Liquipedia's two sentences differ on whether a challenger must enter weapon or scan range; the
  rule used scan range, where the candidates are.
- **Tests:** `b9_a_target_out_of_range_gives_way_to_the_closest_candidate` failed on the old rule (kept the target 9
  away against one 8 away); the guards `b9_a_target_in_range_is_kept_against_a_closer_equal_candidate` and
  `b9_a_target_in_range_gives_way_to_a_higher_ranked_candidate` passed before and after.
- **Measured** against §4.5: 12 battles moved. **Battle 33's winner over its seeds flipped**: on 20 seeds 10/6 → 7/11
  (marine / zealot wins); on 68 seeds 37 / 20 / 11 → 20 / 35 / 13, marine survivors 3.62 (SD 3.16) → 1.85 (SD 2.46),
  zealot survivors 1.00 (SD 1.32) → 1.76 (SD 1.52), mean end tick 443.1 → 446.8. Seeds of record 0-3, 0-2, 0-2, 9-0 →
  0-2, 3-0, 0-2, 2-1 (the expectation `blue: (0, 9), red: (0, 3)` still holds on all four; 3 failures over all 20
  seeds, was 1). `golden_replay.ron` diverged from checkpoint 5 (ticks 201 to 250). Other battles, no winner flip:
  31_15_marauders_vs_15_stalkers 11/9 → 16/4 (11 seed winners changed); 32_30_marines_vs_15_banelings 4/11 → 4/8;
  28_100_lings_vs_10_immortals survivors 27.20 → 35.10, end 678.0 → 597.1; 29 68.70 → 70.80, end 448.6 → 423.2;
  27, 30, 34, 26, 15, 11a, 10 by under 5 ticks.
- **First stopped, then applied.** The first run stopped under the hand-off's rule ("a battle's overall winner over its
  seeds flipping stops the step"). The orchestrator then decided to apply it (plan Amendment 5): it is StarCraft II's
  keep-target rule with a cited source, Q0051 asks for identical unit mechanics, and plan D-4 re-records battle 33
  rather than stopping on it. It is committed after the B-10 commits, which were bit-identical, so the figures above
  are its effect. No battle other than 33 changed its winner over its seeds.
- **Mechanism, traced on the seeds of record** (`matchup_trace` unit CSVs before and after the rule, compared by
  `~/scratch/sc2dev_wp4/scripts/trace_diff.py`). On seed 11 the two runs first differ at tick 68: zealot 34, chasing
  marine 16 5.775 away and out of reach, takes marine 20, 5.774 away, where the old rule kept chasing marine 16.
  On seed 3203386110 the first difference is the same kind at tick 57 (zealot 34: marine 20 at 8.503 gives way to
  marine 24 at 8.495). Over the whole fight the zealots change target while their old one still lives 22 times on
  seed 11 and 29 times on seed 3203386110 (2 and 2 before); the marines 1 and 8 times (1 and 0 before). The
  marines rarely switch because they fire from range and keep their in-reach target, as before. So the change is
  the keep-target rule acting on chasing zealots, not a side effect.
- **Battle 33 re-recorded** (orchestrator: the smallest ranges that hold on all 20 matrix seeds): `blue: (0, 8),
  red: (0, 5)` (was `(0, 9)`, `(0, 3)`); 0 failures over all 1000 runs. The fixture comment carries the 68-seed
  distribution.
- **`golden_replay.ron` re-recorded.** Checkpoints 1 to 4 unchanged; from checkpoint 5 (ticks 201 to 250) every one
  changed. Cause, from a per-tick dump of each unit's target in the replay before and after: on tick 213 the
  "red_psi" zealot (entity 24), chasing "blue_infantry" marine 15 2.291 away, out of reach, takes marine 8, 2.284
  away; the old rule kept marine 15. `golden_replay_orders.ron` unchanged. Debug, release and `wasm32-wasip1`
  reproduce both.

### 4.7 Commit e: the primary weapon (B-10)

- **Rule** (latent on the data of §4.5). `TargetPriorityKey` gains `primary` after the attack target priority
  (`UnitArchetype::primary_weapon_reaches`: an air primary reaches air units and cliff walkers, a ground primary every
  mover but air units, `Either` everything), and `rank()` includes it, as the source's "first 3 criteria".
- **Tests first:** `b10_a_unit_prefers_a_target_for_its_primary_weapon` failed on the old rule (the closer ground
  threat over the air threat) and passes; the guard `b10_threat_outranks_the_primary_weapon` passed before and
  after. Matrix and battle 33 bit-identical to §4.5; goldens unchanged.
- **Data:** `Air` on the Thor and Queen analogues (§4.1). **Test first:**
  `b10_primary_weapons_follow_starcraft` failed on the old data (`colossus_mech` `Either`) and passes. Matrix and
  battle 33 bit-identical (battle 14's Thor meets no air candidate); goldens unchanged.

### 4.8 After Amendment 5

B-9 (§4.6) was committed on top of the B-10 commits (§4.7), then B-11 in three commits (§4.9, §4.10) and B-80
(§4.11). From §4.10 on, each matrix measurement was taken from the commit itself (extracted by an archive of the
commit into a fixed directory, under the shared heavy-run lock), so the worktree could move on while the lock was
busy.

### 4.9 Commit f: checksum schema 11, latent (B-11)

- **Change.** `Entity::previous_target_position: Option<SimVec2>`, `None` at spawn and hashed;
  `CHECKSUM_SCHEMA_VERSION` 10 → 11. The tie-break "closest to the previous target" needs the position once the
  target is lost (a dead entity leaves the world).
- **Latent.** Nothing writes the field in this commit, so it is `None` on every entity on every tick and only its
  empty option tag enters the digest. Re-recorded for the schema alone: both golden replays (every checkpoint), the
  default sandbox checksum (`sim_fixtures` `fixture_tests`) and the schema bytes of four encoding unit tests. Debug,
  release and `wasm32-wasip1` reproduce both goldens. No matrix measurement: the matrix does not read checksums
  and no behaviour changed.

### 4.10 Commit g: the per-archetype tie-break (B-11)

- **Rule** (latent on all-`Closest` data). `TargetPriorityKey::tie_break` replaces the distance: the squared
  distance (`Closest`), the turn of the firing facing towards the candidate in binary angle units
  (`LeastTurning`; the turret's facing for a turreted weapon, the hull's otherwise), or the squared distance from the
  previous target's last position (`ClosestToPrevious`; the unit's own distance while it has none). It decides only
  when a unit chooses anew, because a kept target (§4.6) is never compared on it. Acquisition records the current
  target's position each tick on `ClosestToPrevious` units only.
- **Tests first:** `b11_least_turning_takes_the_candidate_ahead` and
  `b11_closest_to_previous_takes_the_candidate_nearest_the_lost_target` failed on the old rule (each took the closer
  candidate) and pass. Matrix and battle 33 bit-identical to §4.6; goldens unchanged on every target.
- **Data:** `LeastTurning` on the Thor and Ultralisk analogues, `ClosestToPrevious` on the Colossus analogue
  (§4.1). **Test first:** `b11_acquire_tie_breaks_follow_starcraft` failed on the old data (`colossus_mech`
  `Closest`) and passes. Goldens unchanged on every target (none of their units changes).

  Measured against the rule commit: four battles moved, no battle's winner over its seeds changed; battle 33
  bit-identical (20 / 35 / 13); 0 expectation failures over 1000 runs.

| Battle | Seeds moved | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|
| 12a_colossus_vs_6_marines | 6 | 1.00 – 0.00, same | 81.7 → 76.2 | 237.90 → 240.75 / — |
| 15_plated_ultralisk_vs_8_marines | 20 | 1.00 – 0.00, same | 116.8 → 108.0 | 412.96 → 420.92 / — |
| 26_100_lings_vs_6_colossi | 14 | 0.00 – 6.00, same | 116.3 → 107.5 | — / 250.00, same |
| 27_100_lings_vs_8_ultralisks | 20 | 0.00 – 8.00, same | 213.2 → 273.8 | — / 417.91 → 393.99 |

  The Colossi keep sweeping the cluster their last target stood in, and the lone Ultralisk turns least and kills the
  marines sooner (12a, 15, 26). Eight Ultralisks against 100 zerglings take 61 ticks longer: each takes the zergling
  ahead of it rather than the closest one (the mechanisms of this paragraph are inferred from the rule, not traced
  per unit).

### 4.11 Commit h: a worker acquires only while attack-moving (B-80)

- **Rule.** An archetype with `acquire_level: Defensive` (the SCV, Probe and Drone analogues, §4.1) acquires no
  target while idle or holding; attack-moving, or carrying out an attack it already has, it goes on (Liquipedia
  "Automatic Targeting", Notes: units with a defensive acquire level "do not automatically acquire targets for their
  weapons" unless they attack-move). Retaliation when struck and the alert of idle allies are unchanged; they are
  B-13's (WP16).
- **Test first:** `b80_a_defensive_unit_acquires_only_while_attack_moving` failed on the old rule (the idle scratch
  worker took the enemy) and passes. Goldens unchanged on every target.

  Measured against §4.10: matrix and battle 33 bit-identical (no matrix battle has an idle worker beside an enemy).

### 4.12 What WP4 leaves open

Nothing of B-7 to B-11, B-76 or B-80. The rank of a challenger follows the source's "first 3 criteria" (Amendment
4); Liquipedia's weapon-range versus scan-range wording for a challenger is resolved to scan range (§4.6). The C7
float counts did not move (the new fields are integers, enums and a fixed-point vector).

## 5. WP5: multi-strike timing (B-2)

Measurements under `~/scratch/sc2dev_wp5/<step>`, each from its commit extracted into a fixed directory under the
shared heavy-run lock (§4.8). The base is `main` at `9df767e` (WP4 complete and StarCraft II's scan ranges applied,
Q0057; combat alignment evidence §16): whole matrix 0 expectation failures, battle 33 21 / 37 / 10 on 68 seeds, as
recorded there. (WP5 and WP6 were first built on `d1876fa`, before the scan ranges; every number below is from the
commits rebased onto `9df767e`, each measured against its predecessor.)

### 5.1 Research (U-2)

**Rule.** A multi-strike weapon of `StarCraft` II fires a `CEffectCreatePersistent`: `PeriodCount` strikes, the
`k`-th after the delay `PeriodicPeriodArray[k]` from the one before (the first after entry 0), each applying its
`PeriodicEffectArray` effect, which may carry its own validators. The data show that entry 0 delays the first strike:
`voidmulti` EffectData:36 sets `BacklashRockets`' entry 0 to 0 and appends the old 0.15 (`liberty` EffectData:2550 had
only 0.15), which is how a patch removes a first-strike delay. A layer that adds an array entry without an `index`
appends it (the same Banshee edit). Times are Normal seconds; ticks = Normal s ÷ 1.4 × 25, rounded half up (audit
§2.3).

| Analogue (catalog slug) | `StarCraft` II weapon → effect | Periods (Normal s) | Interval | Later-strike condition | Source |
|---|---|---|---|---|---|
| Zealot (`psi_grey`) | `PsiBlades` → `PsiBladesBurst` | 0, 0.28 | **5** | `ZealotAttackDamageMaxRange` ≤ 2.1; `MultipleHitGroundOnlyAttackTargetFilter` | `liberty` WeaponData:150, EffectData:1919; `voidmulti` EffectData:361, ValidatorData:855, :867 |
| Reaper (`jet_jumper`) | `P38ScytheGuassPistol` → `P38ScytheGuassPistolBurst` | 0, 0.122 | **2** (2.18) | `ReaperAttackDamageMaxRange` ≤ 7 (8 from a Bunker with its range upgrade; no Bunker carries units here); ground filter | `liberty` WeaponData:317, EffectData:1803; `voidmulti` EffectData:346, ValidatorData:843, :877 |
| Thor ground (`colossus_mech`) | `ThorsHammer` → `ThorsHammer` | 0, 0.375 | **7** (6.70) | ground filter | `liberty` WeaponData:224, EffectData:2242; `voidmulti` EffectData:876 |
| Viking (`vector_interceptor`) | `LanzerTorpedoes` → `LanzerTorpedoes` | 0, 0.21 | **4** (3.75) | each missile `RangeCheckLE15` ≤ 15 | `liberty` WeaponData:266, EffectData:1698, :1707, ValidatorData:1956 |
| Banshee (`cloak_gunship`) | `BacklashRockets` → `BacklashRockets` | 0, 0.15 | **3** (2.68) | each missile ≤ 15 | `liberty` WeaponData:548, EffectData:2550, :2558; `voidmulti` EffectData:36 |
| Liberator AA (`sky_bastion`) | `LiberatorMissileLaunchers` → `LiberatorMissileBurstPersistent` | 0.05 (one entry, repeated) | **1** (0.89) | none | `void` WeaponData:321, EffectData:2942 |
| Missile Turret (`flak_array_dome`) | `LongboltMissile` → `LongboltMissile` | 0, 0.1 | **2** (1.79) | each missile ≤ 15 | `liberty` WeaponData:300, EffectData:1779, :1789 |
| Queen ground (`hive_matron`) | `TalonsMissile` → `TalonsMissileBurst` (and the hidden melee `Talons` → `TalonsBurst`) | 0, 0.3 | **5** (5.36) | `QueenTalonsAGAttackDamageMaxRange` ≤ 5 (on `Talons`, inherited by `TalonsMissileDamage`); ground filter | `swarm` WeaponData:230, EffectData:3038, :3053; `liberty` EffectData:2320; `voidmulti` EffectData:649, ValidatorData:872 |
| Phoenix (`flying_saucer`) | `IonCannons` → `IonCannons` | 0, 0 | 0 (both missiles on one loop) | none | `liberty` EffectData:1959 |
| Colossus (`tripod_annihilator`) | `ThermalLances` → `ThermalLancesForward` + `ThermalLancesReverse` | both strikes on the target expire together | 0 | `ColossusAttackDamageMaxRange` ≤ 2 from the attack's target point | `liberty` EffectData:2357–2410; `voidmulti` EffectData:964, ValidatorData:882 |
| Carrier (`mothership_carrier`) | interceptors (B-22) | — | not applicable | — | WP18 |
| Brood Lord (`spore_matriarch`) | `BroodlingStrike` (B-21) | — | not applicable | — | WP18 |

- **Measuring rule of the distance validators.** They are `CValidatorLocationCompareRange`: they compare two
  *locations* (`WhichLocation TargetUnit` is the target unit's position point; the Colossus' compares two points
  explicitly, `Value Effect="ThermalLances" Value="TargetPoint"`), and the class has no radius field. The catalog
  measures `followup_max_distance` from centre to centre accordingly. This is read from the validator's type, not
  stated in a source; for the Zealot the two readings differ only once the target is more than 2.1 − 0.875 = 1.225
  further than the strike's 0.975 centre distance after 5 ticks, faster than any unit here moves.
- **Not modelled, recorded.** The first strike of the Liberator's burst waits 0.05 Normal s (1 tick) and both of the
  Colossus' strikes on its target 0.375 Normal s (7 ticks, the persistents' `ExpireDelay`) after the weapon fires;
  ours land on the release tick. Recorded as **B-81** (class (i)); the Colossus part belongs with its sweep (B-18,
  WP8b). Whether an order given between the strikes ends a `Channeled` burst is not stated in the data or on
  Liquipedia: filed as [Q0058](../questions/closed/0058-channeled-burst-interruption.md), default no change (only the
  attacker's death or boarding ends a burst).

### 5.2 Commit a: strikes on separate ticks, latent

- **Rule** (`combat_resolution.rs` `begin_strikes`, `advance_pending_strikes`). A weapon with
  `strike_interval_ticks` > 0 lands its first strike on the release tick and keeps the rest in
  `Entity::pending_strikes`, one per interval. A later strike lands only if its target is still available to the
  attacker (alive, damageable, revealed if cloaked), on the plane of the weapon that released it (the data's
  `MultipleHitGroundOnlyAttackTargetFilter`), and within `followup_max_distance` from centre to centre if the
  weapon names one; a strike that fails is lost and the next still tries, as each period's effect is validated on
  its own. The attacker's death or boarding ends the burst. A projectile weapon launches one missile per strike,
  each carrying one strike (`ActiveProjectile::strikes`); a strike with splash splashes. Catalog validation rejects
  an interval on a single-strike weapon, a burst whose last strike would land after the attack period, and a
  distance without an interval.
- **Tests first** (`crates/sim_core/tests/multi_strike_timing_tests.rs`, scratch archetypes). On the old code
  (`d1876fa`, built from a fresh extraction): `the_second_strike_lands_one_interval_after_the_first`,
  `a_followup_beyond_its_distance_is_lost`, `a_dead_attacker_lands_no_further_strike`,
  `a_spaced_projectile_weapon_launches_one_missile_per_strike` and `catalog_rejects_strike_spacing_it_cannot_honour`
  failed (both strikes on the release tick: 20 damage on the first tick; one missile with both strikes; the
  catalog accepted every spacing); the guards `strikes_with_no_interval_land_on_the_release_tick` and
  `catalog_accepts_a_burst_that_ends_within_its_period` passed. All seven pass with the rule.
- **Latent.** Every shipped interval is 0. Checksum schema 11 → 12 (the two weapon fields, `pending_strikes`, the
  projectile's `strikes`): both golden replays, the default sandbox checksum and the schema bytes of four encoding
  unit tests re-recorded for the schema alone. With the four fields left out of the encoding at schema 11, both
  goldens reproduce as recorded for the scan ranges (release). Debug, release and `wasm32-wasip1` (debug and
  release) reproduce the new lists. Matrix and battle 33 bit-identical to §5's base (21 / 37 / 10).
- **Trace tool.** `sim_fixtures::trace` replays only the strikes a release lands (one for a spaced weapon) when it
  counts wasted shots; later strikes are not releases, so in battles with spaced weapons the count is a lower
  bound.

### 5.3 Commit b1: the Zealot's second blade (B-2)

- **Data.** `psi_grey`: `strike_interval_ticks: 5`, `followup_max_distance: Some(2.1)` (§5.1). **Test first:**
  `b2_strike_spacing_follows_starcraft` (`weapon_catalog_rules_tests.rs`, a table of the cited spacings found by
  `sc2_analogue`; every other weapon must land its strikes together) failed on the old data (`psi_grey` `(0, None)`)
  and passes. Tests restated for the cause: the 1-v-1 cadence test (`sc2_battle_benchmarks_tests.rs`) and the trace
  tool's melee test now expect each swing's second blade 5 ticks after its first, so the marine dies on the third
  swing's second blade, 5 ticks later than before.
- **Measured against §5.2.** Six battles moved; only battle 33's winner over its seeds changed. Each field alone:
  the interval alone gives the same matrix and battle 33 bit for bit, so the 2.1 condition decides no blade in the
  matrix.

| Battle | Seeds moved | Wins blue/red | Survivors blue – red | End tick | Lead hull blue / red |
|---|---|---|---|---|---|
| 7_zealot_vs_roach | 20 | 20/0, same | 1.00 – 0.00, same | 240.9 → 245.9 | 42.00, same / — |
| 8_zealot_vs_stalker | 20 | 20/0, same | 1.00 – 0.00, same | 241.5 → 246.5 | 51.00, same / — |
| 23b_upgraded_zealot_vs_6_lings | 20 | 20/0, same | 1.00 – 0.00, same | 245.6 → 250.6 | 62.80 → 57.80 / — |
| 25a_2_marines_vs_zealot | 20 | 0/20, same | 0.00 – 1.00, same | 145.7 → 153.7 | — / 62.00 → 57.50 |
| 25b_upgraded_marines_vs_zealot | 20 | 0/20, same | 0.00 – 1.00, same | 141.4 → 146.4 | — / 32.00 → 22.00 |
| 33_24_marines_vs_12_zealots | 20 | 7/13 → 17/1 | 2.25 – 1.75 → 5.80 – 0.25 | 442.9 → 431.7 | 37.00 (4) → 39.29 (7) / — |

  Every zealot fight ends 5 ticks later (the killing blow is now a second blade), or 8 in 25a where the zealot
  kills two marines; living 5 ticks longer, the last victims deal the zealot 4.5 to 10 more damage (lead hulls of
  23b, 25a, 25b).
- **Battle 33, 68 seeds:** 54 / 7 / 7 (was 21 / 37 / 10), marine survivors 5.29 (SD 3.27; was 2.06), zealot
  survivors 0.37 (SD 0.83; was 1.74), mean end tick 434.9 (was 448.6). Seeds of record 7-0, 7-0, 6-0, 7-0 (were
  0-2, 0-2, 3-0, 6-0). **Mechanism** (a probe build counting each second blade as it falls due, 68 seeds, not
  committed; taken on the base before the scan ranges, where the same change moved 20 / 35 / 13 to 57 / 5 / 6):
  3966 second blades fell due; 3650 landed (every one within 2.1 and on an available target) and 316
  found their marine already dead (killed by its first blade or by another zealot in the 5 ticks). So the zealots
  deal the same blades, half of them 5 ticks later: each marine kill takes 5 ticks longer, the marines fire through
  those ticks, and in this even fight that turns most seeds. Re-recorded per combat alignment D-6:
  `blue: (0, 13), red: (0, 3)` (was `(0, 10)` and `(0, 5)`), the smallest ranges that hold on all 20 matrix seeds;
  the fixture comment states the distribution. The fixture's claim ("either side may win narrowly") is kept, as the
  claim of the battle's design, and is no longer what the matrix shows.
- **Golden replay** re-recorded: from checkpoint 5 (ticks 201 to 250), the first in which a "red_psi" (`psi_grey`)
  zealot swings, every checkpoint changed; `golden_replay_orders` is unchanged (its zealots release no swing). Debug,
  release and `wasm32-wasip1` reproduce.

### 5.4 Commits b2 to b5: the other spaced weapons (B-2)

Each commit sets the cited spacing (§5.1) and first adds its row to `b2_strike_spacing_follows_starcraft`, which
failed on the old data (the archetype's weapon `(0, None)`) and passes. Each is measured against the one before.

- **b2, Thor ground** (`colossus_mech` "Thor's Hammer", both of its ground slots): 7 ticks. One battle moved, no
  winner: `14_thor_vs_ultralisk` (20 seeds), the Thor still wins every seed but ends 7 ticks later (200.0 → 207.0)
  with 4.00 life left instead of 37.00, its killing blow now a second shot. Battle 33 bit-identical.
- **b3, Viking** (`vector_interceptor`): 4 ticks, each torpedo within 15. Two battles moved, no winner over its
  seeds: `19_viking_vs_brood_lord` ends 4 ticks later (293.0 → 297.0); `35_12_vikings_vs_6_colossi` moved on 16
  seeds, blue wins 16 → 12 of 20 (seeds of record 0 and 22, and seeds 7 and 9, go from a Viking win to a fight
  unresolved at the tick limit; no seed goes to the Colossi), Colossus survivors 0.20 → 0.40, mean end tick
  403.8 → 546.5. Battle 33 bit-identical.
- **b4, Reaper** (`jet_jumper`): 2 ticks, within 7. Matrix and battle 33 bit-identical: in `22a` the Reaper kills
  each 45-life worker with its fifth pistol shot, a first shot (9 a shot against Light), and in `22b` it dies to the
  Stalker either way; neither battle records the Stalker's shields.
- **b5, units no matrix battle spawns**: Banshee (`cloak_gunship`) 3 ticks within 15, Liberator (`sky_bastion`)
  1 tick, Missile Turret (`flak_array_dome`) 2 ticks within 15, Queen (`hive_matron`, both ground slots) 5 ticks
  within 5. Matrix and battle 33 bit-identical; goldens unchanged. Test restated for the cause: S-4
  (`combat_correctness_tests.rs`) takes the first shipped multi-strike missile weapon, now the Banshee's, and
  expects its second missile one interval after the first.
- **Goldens.** Only b1 moves them (no other spaced weapon is in either fixture); debug, release and
  `wasm32-wasip1` reproduce after b1 and after b5.

## 6. WP6: bonus damage against any attribute (B-14)

### 6.1 Research

**Rule.** A `StarCraft` II `CEffectDamage` carries an `AttributeBonus` array indexed by attribute (Light,
Armored, Biological, Mechanical, Psionic, Massive, Structure, Heroic); a strike adds the entry of every attribute
its target has. Upgrades raise an entry on its own: `EffectArray Reference="Effect,<effect>,AttributeBonus[<attribute>]"`
(`liberty` UpgradeData:509, the Baneling's Light +2 a level; :679, the Corruptor's Massive +1; `voidmulti`
UpgradeData:314, the Thor's high-impact Massive +1). The catalog's single `bonus_upgrade_per_level` therefore applies
to each entry that applies; no shipped weapon has two entries, so per entry and once (the old rule) agree on all
shipped data.

**Data.** Every damage effect of the modelled weapons, resolved over the layer chain (`core` → … → `voidmulti`):

| Analogue | Damage effect | `AttributeBonus` at 5.0.14 | Catalog before | Source |
|---|---|---|---|---|
| Corruptor (`sky_leviathan`) | `ParasiteSporeDamage` | Massive 6 | Armored 6 | `swarm` EffectData:2062 (`liberty` :2590 had Massive 10) |
| Thor, high-impact mode (`colossus_mech` air weapon) | `LanceMissileLaunchersDamage` | Massive 10 | Armored 10 | `voidmulti` EffectData:749 |
| Reaper (`jet_jumper`) | `P38ScytheGuassPistol` | none (Light 0) | Light 5 | `void` EffectData:1499 (`liberty` :1817 had Light 5) |
| Spore Crawler (`spore_spire_dome`) | `SporeCrawlerU` | Biological 10 | none | `voidmulti` EffectData:870 (`void` :1432 had 30); Liquipedia Spore Crawler, patch 5.0.14: "Damage increased from 15 +15 biological to 20 +10 biological" |
| Siege Tank, Marauder, Phoenix, Ghost, Hellion, Viking, Stalker, Adept, Immortal, Colossus, Void Ray, Baneling, Lurker, Spine Crawler | as cited in the test `b14_attribute_bonuses_follow_starcraft` | Armored or Light, as the catalog has | unchanged | `liberty` EffectData:1666, :1651, :1986, :1634, :2607, :1718, :2439, :2050, :2166; `voidmulti` :583, :722, :966, :1026, :817 |
| every other armed analogue | — | none | none | no `AttributeBonus` in the chain (the Ultralisk's is set to 0 in `void` EffectData:1168) |

- **Not an attribute bonus, recorded: B-82.** The Baneling's damage to structures is a second effect,
  `VolatileBurstU2`: 80 to `Ground,Structure` targets, +5 a level, and `ArmorReduction 0`, so building armour does
  not reduce it (`liberty` EffectData:1052, UpgradeData:505); its unit effect `VolatileBurstU` excludes structures
  (`liberty` EffectData:1756). Ours has the one unit effect for every target, so a structure takes 16 less its armour. A Structure bonus of 64
  would match only at level 0 and not the armour rule, so it is not used; class (i), WP9 with the other non-time
  values.
- **Found in passing, not attribute bonuses (B-47, WP9).** The Spore Crawler's base damage is 20 at 5.0.14
  (`voidmulti` EffectData:871), the catalog's 15; the High Templar analogue has a weapon, the High Templar none.

### 6.2 Commit a: a bonus against any attribute, latent

- **Rule.** `WeaponSpec::attribute_bonuses` (`sim_types::AttributeBonuses`, one value per `Attribute`, Light to
  Heroic) replaces `bonus_armored` and `bonus_light`; a strike adds the bonus of every attribute its target has,
  each with `bonus_upgrade_per_level` a level (§6.1). The catalog field `attribute_bonuses: [(Attribute, bonus)]` is
  required, so a catalog written with the old fields fails to load instead of losing its bonuses; validation
  rejects a negative bonus and an attribute listed twice. The Siege Mode ability's weapon is boxed
  (`AbilityEffectSpec::SiegeModeToggle`), because the larger `WeaponSpec` made that variant far larger than the
  others (`clippy::large_enum_variant`).
- **Tests first.** `attribute_bonus_tests.rs` (scratch archetypes; a Massive bonus, two bonuses on a target with
  both, no bonus, and the validation including a stale weapon with the old fields) failed on the old code (the
  catalog required `bonus_armored`), and `b14_each_applying_attribute_bonus_gains_its_upgrade`
  (`damage_and_shields_tests.rs`, Light 5 and Armored 3 at level 2 against a Light and Armored target) failed with
  20 for 22: the old rule added the per-level bonus once. All pass.
- **Latent.** Every weapon's data is rewritten to the same bonuses. Checksum schema 12 → 13 (eight encoded values
  for two): both golden replays, the default sandbox checksum and four encodings' schema bytes re-recorded; with the
  bonuses encoded as the two former fields at schema 12, both goldens reproduce as recorded for WP5 (release).
  Matrix and battle 33 bit-identical to §5.4.

### 6.3 Commits b1, b2 and b4: the reference bonuses; b3 stopped

Each data commit first adds its rows to `b14_attribute_bonuses_follow_starcraft` (`weapon_catalog_rules_tests.rs`,
every armed analogue's cited bonuses, found by `sc2_analogue`), which failed on the old data and passes. Measured
against the one before, from the commits rebased onto `9df767e`; commit a was bit-identical to §5.4.

- **b1, Corruptor** (`sky_leviathan`): Massive 6 for Armored 6. One battle moved, no winner:
  `18_void_ray_vs_corruptor`, where the Void Ray (Armored, not Massive) no longer takes the +6; it still wins every
  seed, with 150.00 life left instead of 110.00. Battle 33 bit-identical.
- **b2, Thor high-impact payload** (`colossus_mech` air weapon): Massive 10 for Armored 10. Matrix and battle 33
  bit-identical (no Massive air unit in the matrix; the Thor's battle 14 is against the ground).
- **b3, Reaper** (`jet_jumper`): no Light bonus (it was 5). **Stopped first** (applied later, §6.4). Measured on a scratch commit
  on top of b2: `22a_reaper_vs_2_workers` changes its winner over its seeds, the Reaper winning 20 of 20 → 0 of 20
  (all 20 unresolved at the battle's 200-tick limit: the Reaper alive with 9.00 life, one worker alive in 11 seeds
  with 13.00); expectation failures 4 of record and 20 over all seeds; no other battle moved, battle 33
  bit-identical. The battle's claim is "The bonus vs light kills workers", a bonus the 5.0.14 data no longer has
  (`void` EffectData:1499 sets it to 0; `liberty` EffectData:1817 had Light 5). The stop rule
  applies: the data value is reported for the orchestrator's decision, B-14 keeps the Reaper's bonus as its open
  part, and the test table has no Reaper row yet.
- **b4, Spore Crawler** (`spore_spire_dome`): Biological 10. No matrix battle spawns it: matrix and battle 33
  bit-identical; goldens unchanged.
- **Goldens.** No commit of WP6 b moves them (none of these units is in either fixture).

### 6.4 Commit b3: the Reaper's Light bonus is 0 (B-14, plan Amendment 10)

- **Resolved over the whole chain** (`core → liberty → swarm → void → voidmulti`, audit §2.2): `liberty`
  EffectData:1817 gives `P38ScytheGuassPistol` `AttributeBonus Light 5`; `void` EffectData:1498–1499 sets it to 0;
  `voidmulti` EffectData:346 overrides the same effect but only adds validators, so the 5.0.14 value is **0**
  (final layer `void`, line 1499). The layers outside the audit's chain agree: `swarmmulti` EffectData:389 also
  sets 0, and neither `libertymulti` nor `balancemulti` sets it. Applied per the orchestrator's decision
  (Amendment 10).
- **Test first:** the Reaper row of `b14_attribute_bonuses_follow_starcraft` failed on the old data (Light 5) and
  passes; the table now requires a row for every armed analogue.
- **Measured against §6.3's head:** one battle moved. `22a_reaper_vs_2_workers` changed its winner, as the decision
  sanctions: the Reaper won 20 of 20 seeds by tick 101.0 with 40.00 life left; now all 20 end at the 200-tick limit
  with the Reaper alive (9.00 life) and one worker alive. Cause, traced to the units: the Reaper (`jet_jumper`,
  4 × 2 a 20-tick attack, 60 life) against two Light workers (`welder_drone`, 45 life, no armour) needed five 9-damage
  shots a worker with the bonus and needs twelve 4-damage shots, six attacks, without it. Two workers take twelve
  attacks, 240 ticks of attack periods, more than the battle's 200 ticks; on all 20 seeds exactly one worker is
  left (the first of the two in 11 seeds, the second in 9). Battle 33
  bit-identical (54 / 7 / 7); 0 expectation failures over 1000 runs after the re-record. Re-recorded:
  `expect: (blue: (1, 1), red: (1, 1))` (was `red: (0, 0)`), and the claim, which said "The bonus vs light kills
  workers", now reads "Without a bonus vs light, the Reaper kills only one of two workers in 200 ticks". Goldens
  unchanged (no Reaper in them).

## 7. WP7: shield spillover (B-15, B-74)

### 7.1 Research (U-3)

**Where the rule lives.** How a strike is divided between shields and life is engine code: no 5.0.14 data file
states it. `CEffectDamage` carries the amounts, `ArmorReduction` and flags, not the order (`core` EffectData,
`liberty` EffectData, searched for minimum, spill and shield fields over the chain `core` → … → `voidmulti`); the
`DamageResponse` fields of `BehaviorData` (`ModifyMinimumDamage`, `Minimum`) belong to behaviours such as
Hardened Shield, not to the default. Liquipedia is the source, read as wikitext on 2026-11-06:

- [Damage Calculation](https://liquipedia.net/starcraft2/Damage_Calculation): "Defense Available: If the defender
  is a Protoss unit with Shield Points, this equals Shield Defense. Otherwise, this equals Armor Defense"; "Damage
  Inflicted … whichever is greater, Damage Capped − Guardian×Spell or 0.5"; "Total Damage: If the defender is a
  Protoss unit with Shield Points, and the Damage Inflicted is greater than Shield Points + Armor Defense, then the
  damage spills over to the HP and this equals Damage Inflicted − Armor Defense"; then "apply the Total Damage
  first to the Shield Points and then to the HP". Its worked example: a +2 Marauder (10 + 10 Armored + 4) against
  an Immortal with +1 shields, 4 armour and 2 shields; without Corruption, Hardened Shield and Guardian Shield that
  is 24 − 1 = 23 inflicted, 2 to the shields and 23 − 4 − 2 = 17 to life.
- [Plasma Shield](https://liquipedia.net/starcraft2/Plasma_Shield): shields "apply their own armor stat … independent
  of the unit or building's standard armor which reduces damage to hit points"; shield armour "reduces incoming
  damage to the shield total the same way Armor reduces attack damage to hit points".
- [Armor](https://liquipedia.net/starcraft2/Armor): "Armor cannot reduce damage below 0.5, every attack deals at
  least 0.5 damage."

**Decided.**
1. **Which remainder: sourced.** Life takes the shield-mitigated damage less the shields, then less life armour
   (Damage Calculation). Ours took the raw damage scaled by the share the shields did not absorb,
   `raw × (1 − shields ÷ mitigated)`, then life armour. The two agree when shield armour is 0 (exactly, in real
   numbers) and differ once it is positive: 30 against shield armour 1, 20 shields and life armour 2 gives 7, where
   ours gave 7.31. Fixed (§7.2).
2. **The 0.5 minimum on the life part: not sourced.** Armor's "every attack deals at least 0.5" speaks of the strike,
   which has already damaged the shields; Damage Calculation, read literally, sends a remainder no larger than
   armour to life unarmoured, a jump (life damage falls from the armour value to nearly 0 as the strike grows past
   shields plus armour) that reads as an editing slip. TeamLiquid, SC2Mapster and Hive Workshop searches found
   nothing more. Filed as [Q0062](../questions/closed/0062-shield-break-life-minimum.md) with an editor measurement
   and the default "no change": the life part stays at least 0.5, and B-15 stays recorded for that part only.

### 7.2 Commit: the shield-mitigated remainder reaches life (B-15, B-74 part)

- **Rule.** `combat.rs` `mitigate_strike`: when the shield-mitigated damage exceeds the shields, life takes
  `max(mitigated − shields − life armour, 0.5)` (§7.1 item 1); the 0.5 stays pending Q0062 (§7.1 item 2). The life
  armour sum is one helper, `effective_life_armor`, for both branches. The module doc's "exact shield spillover
  damage formulas" is replaced by the rule it now implements (B-74's `combat.rs` part; the `SplashSpec` part stays
  with WP8a).
- **Tests first** (`damage_and_shields_tests.rs`): `b15_shield_break_sends_the_mitigated_remainder_to_life`
  (Liquipedia's worked example, 24 against shield armour 1, 2 shields, life armour 4) failed with 17.9130 for 17,
  and `b15_shield_break_remainder_is_not_raw_proportional` (the old test's numbers) with 7.3103 for 7; both pass.
  `b15_shield_break_remainder_below_life_armour_keeps_the_minimum` pins the Q0062 default (green before and after).
  The old `test_sc2_shield_spillover_exact_formula`, which asserted the raw-proportional rule, is replaced by them.
- **Measured** against `61e88d9` (the same commands, `~/scratch/sc2dev_wp7/{base,rule}`): no shipped unit has
  shield armour and no matrix battle researches a shield level, so the two rules agree in real numbers on every
  matrix strike; the old quotient lost up to 1.9e-9 of life to fixed-point rounding at each break. Four battles
  "moved" by that alone, in the lead hull only (`8_zealot_vs_stalker`, `12a_colossus_vs_6_marines`,
  `25a_2_marines_vs_zealot`, `25b_upgraded_marines_vs_zealot`: e.g. 50.9999999981 → 51); no end tick, survivor
  count or expectation changed on any of the 1000 runs; 0 expectation failures. Battle 33 bit-identical on 68 seeds
  (54 / 7 / 7). The rule moves battles only once shield armour is positive: the skirmish and future
  shield-upgrade battles.
- **Goldens re-recorded** for the same rounding: `golden_replay` checkpoints 5, 6 and 8 (ticks 201 to 300 and 351
  to 400), `golden_replay_orders` checkpoints 7 to 12 and 16 to 34, while a damaged `psi_grey` zealot carried it.
  Proof in a throwaway run: asserting shield armour 0 at every shield break and that old and new life parts differ
  by at most 1e-5, and substituting the old value, both fixtures reproduce as recorded for WP6. The new lists
  reproduce in debug, release and `wasm32-wasip1` release (`wasmtime`). Checksum schema unchanged (13).

## 8. WP8a: splash zones and the splash distance (B-16, B-17, B-74)

### 8.1 Research (B-16, U-4)

**Zones and fractions: sourced.** A weapon's splash is its damage effect's `AreaArray`: one entry per zone, each
a `Radius` and the `Fraction` of the strike it deals. Read through the chain `core` → … → `voidmulti` for every
circular splash this catalog models:

| Analogue (catalog weapon) | Effect | `AreaArray` (radius, fraction) | `OffsetByUnitRadius` | Catalog before |
|---|---|---|---|---|
| Siege Tank, sieged (`railgun_crawler` Crucio Shock Cannon) | `CrucioShockCannonBlast` | 0.4687 1, 0.7812 0.5, 1.25 0.25 (`liberty` EffectData:1676-1678; later layers set only amounts) | 0 (`liberty` EffectData:1680) | equal |
| Baneling (`volatile_tick`) | `VolatileBurstU` | 2.2 1 (`liberty` EffectData:1757; `voidmulti` EffectData:1025 sets only amounts) | 1, inherited | equal (three equal radii) |
| Liberator, fighter (`sky_bastion` Lexington Rockets) | `LiberatorMissileDamage` | 1.5 1 (`void` EffectData:2938; `voidmulti` EffectData:752 sets only the amount, 5) | 1, inherited | 0.5 1, 1.0 0.5, 1.5 0.25 |

`OffsetByUnitRadius` is 1 on every weapon damage effect by inheritance from `DU_WEAP` (`core` EffectData:128). The
Hellion, Lurker, Colossus and Ultralisk areas are a line or an arc in the game (`InfernalFlameThrowerE`,
`LurkerMPSearch`, `ThermalLancesE`, `KaiserBladesSearch`); their zones go with their shapes (B-18, B-19; WP8b,
WP8c). The Thor's anti-air splash (`JavelinMissileLaunchersDamage`, 0.5 1, `liberty` EffectData:2280) belongs to
its Explosive Payload mode, which the catalog does not model: `colossus_mech` carries the high-impact mode (§6.1).

**Splash distance (U-4): not sourced.** Two things are open, and no source states either precisely enough to
specify:
1. **Where the area is centred.** The data name `OffsetByUnitRadius`; the Siege Tank's blast and the Thor's
   anti-air splash clear it. Patch 1.1 (2010, [Blizzplanet's copy](https://www.blizzplanet.com/blog/comments/starcraft_ii_patch_11_build_013015250))
   changed the splash of exactly the four weapons whose effects clear it: those two, the Archon's
   (`PsionicShockwaveDamage`, `liberty` EffectData:1359) and the Planetary Fortress' (`TwinIbiksCannon`, `liberty`
   EffectData:2545): "splash damage now originates
   from the center of the target, rather than the impact location near the unit". So the flag most likely puts the
   area's centre on the target's edge facing the attacker. Hive Workshop's page on the flag returned 502 throughout;
   a search engine's extract of it reads "offsets the origin of the search radius … by the unit radius of the damage
   effect's primary target". Neither states the direction, nor what the flag does for the Baneling, whose area is
   centred on itself (`ImpactLocation` `CasterUnit`, `liberty` EffectData:1759).
2. **Whether a zone reaches a unit's edge.** Liquipedia's Siege Tank page says only "within .4687 matrices of the
   target"; TeamLiquid (403 to tools), SC2Mapster and Hive Workshop gave nothing more.

Filed as [Q0063](../questions/closed/0063-splash-distance-rule.md) with two editor measurements; the default is no
change (centre of the target, centre of each struck unit), and B-17 stays recorded. The Siege Tank, whose flag is
0, already splashes from the target's centre as the game does.

**Found, recorded (class (i), not fixed here).**
- **B-83:** the Baneling's burst is centred on the Baneling in the game (`ImpactLocation` `CasterUnit`, above) and
  damages its target through the area, not by a direct strike; ours strikes the target and splashes around it.
- **B-84:** the Siege Tank does not splash a target of radius 1.25 or more: `CrucioShockCannonSwitch` sends a
  `TargetRadiusLarge` target (radius ≥ 1.25, `liberty` ValidatorData:824) to `CrucioShockCannonDirected`, which has
  no `AreaArray` (`swarm` EffectData:2183, `voidmulti` EffectData:739, which adds only a lowered Supply Depot to the
  splashing case). Ours splashes around every target.

### 8.2 Commit a: zones with their own fractions, latent (B-16, B-74)

- **Rule.** `SplashSpec` holds `zones: Vec<SplashZone>`, each a radius and a fraction; a unit takes the fraction of
  the innermost zone that reaches it (`SplashSpec::fraction_at`). The catalog field is required, so a catalog with
  the former three radii fails to load; load rejects an empty list, radii that do not increase, and a fraction
  outside `0 < f <= 1`. `SplashSpec`'s doc no longer claims StarCraft II parity: it states what is modelled and
  names B-17 to B-19 (B-74's `SplashSpec` part).
- **Latent.** Every splash rewritten to the same zones (a triple of equal radii is one zone). Matrix (1000 runs)
  and battle 33 (68 seeds, 54 / 7 / 7) bit-identical to WP7 (`~/scratch/sc2dev_wp8/a`).
- **Checksum schema 13 → 14** in the same commit, for the new encoding. Before the bump, with the zones encoded at
  schema 13, both goldens reproduced as recorded for WP7: no splash is in either replay's hashed state, so only the
  schema byte moved them. Both re-recorded, with the sandbox checksum and four encodings' schema bytes; debug,
  release and `wasm32-wasip1` release (`wasmtime`) reproduce.
- **Tests first.** `b16_each_splash_zone_deals_its_own_fraction` and `b16_a_single_zone_deals_full_damage_to_its_edge`
  (`damage_and_shields_tests.rs`) failed to compile on the old type; `b16_splash_zones_must_be_usable`
  (`weapon_catalog_rules_tests.rs`) failed with "fixture text "(radius: 0.4687, fraction: 1.0)" not in catalog".
  All pass.
- **C7.** `catalog.rs` 51 → 50 float tokens, in its own commit.

### 8.3 Commit b: the Liberator's anti-air splash (B-16)

- **Data.** `sky_bastion` Lexington Rockets: one zone, 1.5 at fraction 1 (§8.1), for 0.5 / 1.0 / 1.5 at
  100 / 50 / 25 %. The Siege Tank's and the Baneling's zones already matched.
- **Test first.** `b16_splash_zones_follow_starcraft` checks every circular splash against the table of §8.1 and
  fails on an armed analogue with a splash and no row; red on `sky_bastion` before the change.
- **Measured** against commit a (`~/scratch/sc2dev_wp8/b`): no matrix battle spawns the Liberator analogue, so the
  matrix (1000 runs, 0 expectation failures) and battle 33 (68 seeds, 54 / 7 / 7) are bit-identical; goldens
  unchanged. The change reaches skirmish play only.
- **B-16 is fixed** for every circular splash; the four non-circular weapons take their zones with their shapes
  (B-18, B-19).

## 9. WP8b: line splash (B-18), stopped at the data

Measurements under `~/scratch/sc2dev_wp8b/<step>`, base `main` at `e99abae` (WP8a): matrix 0 expectation failures,
battle 33 54 / 7 / 7 on 68 seeds. Winner tallies below are blue / red / unresolved wins over a battle's 20 seeds.

### 9.1 Research (B-18)

**Shape: sourced.** Each of the three weapons runs a `CEffectCreatePersistent` whose `PeriodicOffsetArray` places one
`CEffectEnumArea` per period. The offsets are in the persistent's frame, oriented from the attacker to the target
(`OffsetVectorStartLocation SourceUnit`, `OffsetVectorEndLocation TargetPoint`, `core` EffectData:37-38); a
`0,-d,0` entry lies along the attack and a `d,0,0` entry across it, which matches Liquipedia's "perpendicular to the
Colossus's direction of fire" ([Colossus (Legacy of the Void)](https://liquipedia.net/starcraft2/Colossus_(Legacy_of_the_Void))).

| Analogue (catalog weapon) | Persistent → search | Origin | Offsets | Search radius | Catalog now |
|---|---|---|---|---|---|
| Hellion (`napalm_wheeler`) | `InfernalFlameThrowerCP` → `InfernalFlameThrowerE` | `SourcePoint` (the Hellion) | along: 0.25 to 6.0 in steps of 0.25 (`liberty` EffectData:2623-2646), then 6.5 (`swarm` EffectData:2244-2246 raises `PeriodCount` to 25 and appends it; 6.25 is not an entry); all in one loop (`PeriodicPeriodArray` 0, `liberty` EffectData:2647) | 0.15 (`liberty` EffectData:2613) | circle 2.2 at 100 % |
| Lurker (`burrow_scorpion`) | `LurkerMP` → `LurkerMPCU` → `LurkerMPSearch` | `SourcePoint` (the Lurker) | along: 1 to 9 (`swarm` EffectData:3010-3019; `voidmulti` EffectData:809 sets `PeriodCount` 9), each spine 0.125 Normal s after the one before (`void` EffectData:298-307) and only while in weapon range (`voidmulti` EffectData:822) | 0.5 (`voidmulti` EffectData:821; was 0.625) | circle 1.5 / 2.0 / 2.5 at 100 / 50 / 25 % |
| Colossus (`tripod_annihilator`) | `ThermalLancesForward` and `ThermalLancesReverse` → `ThermalLancesE`, `ThermalLancesEReverse` | `TargetPoint` (`swarm` EffectData:2323, :2328) | across: −1.25 to 1.25 in steps of 0.25, one sweep each way (`liberty` EffectData:2373-2383, :2398-2408), 0.0227 Normal s apart (`voidmulti` EffectData:954, :995) | 0.15 (`liberty` EffectData:789, :2415) | circle 1.8 / 2.4 / 3.0 at 100 / 50 / 25 %, two strikes, one splash |

- **Fraction.** No search sets a `Fraction`; a damage effect's default is 1 (`core` EffectData:69). Each line is one
  zone at 100 %.
- **Once per attack, per sweep.** Every damage effect of the three carries the `noMarkers` validator
  (`InfernalFlameThrower`, `liberty` EffectData:2605; `LurkerMPDamage`, `swarm` EffectData:3035; `ThermalLancesMU`,
  `voidmulti` EffectData:968), a `CValidatorUnitCompareMarkerCount` that fails with `AlreadyTargeted` (`liberty`
  ValidatorData:1251-1253), and the weapon or persistent carries a marker (`InfernalFlameThrower` `voidmulti`
  WeaponData:592-594, `LurkerMP` `swarm` WeaponData:222-224, each Colossus sweep `liberty` EffectData:2366-2368,
  :2391-2393). A unit found by several searches is struck once per marker: once per Hellion or Lurker attack, once
  per Colossus sweep, so twice per Colossus attack (Liquipedia: "10 (+1) (x2)").
- **Enemies only, ground only.** Every search filters `Ground;Player,Ally,…` (`liberty` EffectData:2611, :788,
  :2414; `swarm` EffectData:3025).
- **Cross-checks.** Liquipedia's Colossus area "2.8 width, 0.3 depth" is 2 × 1.25 + 2 × 0.15 by 2 × 0.15. Its
  Hellion "stream is about 0.15 wide" and "AoE damage up to a range of 5.15" and its Lurker "10 spines … radius of
  0.5 … reach out 10 cells" with range 9 describe earlier patches; the data win (audit §2.2). The Lurker's last
  spine lies at range + 1 in both, so the range gate is read as letting it fire.
- **Timing, not modelled (B-85, new).** The Lurker's spines are 0.125 Normal s apart (2 to 3 ticks), so the outer
  ones can be dodged (Liquipedia), and each Colossus search hands a unit to `ThermalLancesDamageDelay`, 0.25 Normal s
  later (`liberty` EffectData:780-785). Our line lands whole on the shot's tick. A delayed area is a mechanism of its
  own; recorded as **B-85**, class (i), a WP8 follow-up.
- **B-81, the Colossus part: not sourced.** WP5 (§5.1) read the delay of the Colossus' strikes on its target as
  `liberty`'s `ExpireDelay` 0.375 Normal s; `swarm` EffectData:2325, :2330 set 0.73, and the persistent sits at
  `TargetPoint`. Whether `ExpireDelay` counts from creation or after the last period, and whether a point persistent
  still applies its `ExpireEffect` to the target unit, are not stated (an SC2Mapster forum tutorial says "after the
  conditions of the persistent have been completed"). Filed as [Q0064](../questions/closed/0064-colossus-damage-timing.md)
  with an editor measurement; default no change.

### 9.2 Commit a: the line shape, latent (B-18)

- **Rule.** `SplashSpec::shape` is `Circle` (one search at the impact point, the engine default of a damage effect's
  `AreaArray`, so the catalog field defaults to it) or `Line { origin, axis, offsets }`. Each offset is a search
  point, the centre of the splash's zones; a unit takes the fraction of its nearest search point, once per strike.
  With the attacker on its target, the direction falls back to the attacker's facing (`OffsetFacingFallback`
  `SourceUnit`, `core` EffectData:39). Load rejects a line with no offset or a non-finite one.
- **Latent.** No catalog weapon has a line. Matrix (1000 runs) bit-identical to WP8a (`~/scratch/sc2dev_wp8/b`);
  battle 33 54 / 7 / 7, identical on every seed.
- **Checksum schema 14 → 15** in the same commit. With the shape encoded at schema 14, both goldens reproduced as
  recorded for WP8a; re-recorded for the schema byte with the sandbox checksum and four encodings' schema bytes.
  Debug, release and `wasm32-wasip1` release (`wasmtime`) reproduce.
- **Tests first.** `b18_a_line_along_the_attack_strikes_the_line_once` and
  `b18_a_line_across_the_attack_strikes_beside_the_target` (`damage_and_shields_tests.rs`) red against the
  circle-only resolution ("on the line in front of the target", left 0, right 40; "beside the target, one side",
  left 0, right 40); green after.
- **C7.** `catalog.rs` 50 → 51 and `catalog/validation.rs` 30 → 31 float tokens (the authored offsets and their
  range check), in their own commit.

### 9.3 Measured, not committed: one search per strike

The Colossus' two strikes land together, and `apply_splash` searched once per shot, so its splash victims took one
strike where the game's two sweeps strike them twice (§9.1). The rule "each strike searches its area"
(`b18_each_strike_searches_its_area`, red: left 40, right 80) reaches only the Colossus (no other splash weapon has
two strikes on one tick). On the Colossus' current circle (`~/scratch/sc2dev_wp8b/b`, against commit a):

| Battle | Before | After | |
|---|---|---|---|
| `12a_colossus_vs_6_marines` | 20 / 0 / 0, end tick 83 | 20 / 0 / 0, end tick 29 | faster |
| `12b_colossus_vs_3_marauders` ("Three armor-killers fell a colossus") | 0 / 20 / 0 | 16 / 4 / 0 | **winner flips** |
| `26_100_lings_vs_6_colossi` | 0 / 20 / 0 | 0 / 20 / 0 | faster |

Battle 33 54 / 7 / 7. **Stopped** (plan D-4): not committed.

### 9.4 Measured, not committed: the three lines

Each against commit a, with Q0063's default (centre to centre):

| Probe | Battle | Before | After |
|---|---|---|---|
| Hellion line (`c`) | `9a_hellion_vs_4_lings` ("Flame splash clears four clumped zerglings") | 20 / 0 / 0 | 0 / 20 / 0, **flips** |
| | `10_2_hellions_vs_8_lings` ("Two hellions roast eight zerglings") | 20 / 0 / 0 | 0 / 20 / 0, **flips** |
| Lurker and Colossus lines (`probe_lc`) | `21_lurker_vs_6_marines` | 20 / 0 / 0 | 0 / 20 / 0, **flips** |
| | `26_100_lings_vs_6_colossi` | 0 / 20 / 0 | 20 / 0 / 0, **flips** |
| | `12a`, `12b` | moved | winners kept |
| Lurker and Colossus lines and one search per strike (`probe_lc_strike`) | `21`, `26` | as above | **flip** |
| | `12b` | 0 / 20 / 0 | 14 / 6 / 0, **flips** |

Battle 33 is untouched by all (54 / 7 / 7). **Why.** A line search of radius 0.15 measured to a unit's centre reaches
only units whose centre lies within 0.15 of the line, a strip far narrower than a Zergling (radius 0.375) or a Marine:
under Q0063's default the lines hit almost nothing besides the target. **The edge rule decides it.** With Q0063
option 3 substituted for a probe (a unit is reached when any part of it is in a zone; `probe_hellion_edge`,
`probe_lc_strike_edge`, applied to every splash):

- the Hellion still loses 9a and 10 (0 / 20 / 0 each): a lone Hellion's two shots before four Zerglings kill it do
  not kill them, whatever the area;
- the Lurker keeps 21 (16 / 4 / 0) and the Colossi keep 26 (0 / 20 / 0);
- `12b` still flips (16 / 4 / 0): its three Marauders stand in a column 1 apart, exactly across the Colossus' line,
  so both sweeps strike all three;
- `32_30_marines_vs_15_banelings` flips (4 / 8 / 8 → 2 / 16 / 2) from the circles' edge rule, not from a line.

**Held** (plan D-4, a non-33 winner flip stops): the three data commits and the per-strike rule, until the
orchestrator decides. B-18 stays in the register; the rule of §9.2 is ready for the data.

## 10. WP9: the non-time catalog audit (B-47, B-51, B-82)

### 10.1 Research (B-47)

**What was audited.** Every non-time field of `units.ron`, `structures.ron` and `upgrades.ron` for every identity
with an analogue (the identity map of the time audit, [audit](2026-11-04-catalog-time-audit.md) §2.1): life,
shields, energy, life and shield armour, attributes, sight, collision radius, mineral and gas cost, supply cost and
supply provided; per weapon the range, minimum range, damage of each damage effect its effect chain reaches, damage
per upgrade level, target filters and `RangeSlop`; per upgrade its research cost, amount and targets. Push priority
and `InnerRadius` (B-48, B-49, WP16) and cargo size (B-64, WP17) were read and listed, not classified. Fields that
earlier packages audited are not repeated: times and speeds (time audit), attack target priority, threat flags and
tie-breaks (§4.1), strikes and their spacing (§5.1), attribute bonuses (§6.1), splash zones (§8.1), start energy
(§3.4), scan ranges (time audit §11).

**Method.** As the time audit (§2.2 there): SC2GameData at `5444e12f13`, layer chain `core → liberty → swarm →
void → voidmulti`, a later layer overriding an earlier one, an unset value taken from the `parent` element and then
the class default. Weapon damage follows each weapon's effect chain (`CEffectSet`, `CEffectLaunchMissile`,
`CEffectCreatePersistent`, `CEffectSwitch`, `CEffectEnumArea`) to every `CEffectDamage` it reaches. Damage per
upgrade level is read from the merged `EffectArray` of each `…Level1` upgrade, with indexed overrides applied in
layer order: `voidmulti` rewrites several slots by index, for example `ThermalLancesMU` +2 → +1
(`voidmulti` UpgradeData:481), `KaiserBladesDamage` +2 → +3 (`void` UpgradeData:624) and
`LanceMissileLaunchersDamage` +3 (`voidmulti` UpgradeData:309). Research costs are the research ability's
`InfoArray` `Resource` entries. Scripts: `~/scratch/sc2dev_wp9/scripts/` (`audit_nt.py`, `upg.py`, `classify.py`;
not committed, as the time audit's); rows in `~/scratch/sc2dev_wp9/final_base.tsv`.

**Reading rules.**
- A Zerg structure's `CostResource` includes the Drone (50): the Spawning Pool's 250 (`liberty` UnitData:8930)
  is a build cost of 200. Ours is the build cost; class A.
- Morphs (Orbital Command, Planetary Fortress, Lair, Hive, Greater Spire, Baneling, Ravager, Lurker, Brood Lord)
  carry the morph's increment as their cost and supply, and the Zergling analogue is one Zergling at the pair's
  price. That is right only under a morph and pair model, which we lack (B-63): a Baneling trained from its nest for
  25 minerals and no supply is cheaper than the game's 75 and half a supply. Recorded as **B-86** (class (i), WP17,
  with the morph model); no value changes here.
- The Queen's and the Roach's ground attack is two weapons in the game (`Talons` 0–3 and `TalonsMissile` 3–5,
  `liberty` WeaponData:480, `swarm` WeaponData:235; `RoachMelee` 0–0.5 and `AcidSaliva` 0.6–4, `liberty`
  WeaponData:93, :346) with the same damage and period: one weapon from 0 here, class A.
- Target filters: every catalog weapon targets every attribute (`bits: 255`); the game's filters on these weapons
  name only planes and states (`Ground`/`Air`, `Visible`; excluded `Missile`, `Stasis`, `Dead`, `Hidden`,
  `Invulnerable`), which WP2 audited. Class A.

**Counts** (one row per value; listed only, not classified: `RangeSlop` 50 (B-6), push priority and inner radius
82 (WP16), cargo 41 (WP17)):

| File | A | C | D (B-86) | E | B-20 / B-82 | Classified |
|---|---|---|---|---|---|---|
| `units.ron` | 670 | 53 | 13 | 8 | 8 / 12 | 764 |
| `structures.ron` | 501 | 63 | 7 | 0 | 0 | 571 |
| `upgrades.ron` | 134 | 30 | 0 | 0 | 0 | 164 |

Class E: `flying_saucer` (no single analogue), the Carrier / Mothership composite's vitals (350 / 350 are the
Mothership's, the Carrier's are 300 / 150, `liberty` UnitData:3474, :3478; its weapon abstracts the interceptors,
B-22), and the Brood Lord's damage, which the game deals through Broodlings (B-21).

**Mismatches (class C), by field kind.** Every reference below is the 5.0.14 value; the change groups of §10.2
onward follow this order (no matrix identity first).

| Group | Field kind | Values | Matrix identities |
|---|---|---|---|
| g1 | Structure life, shields, armour: Ghost Academy 850 → 1250 (`liberty` UnitData:9763), Starport 1000 → 1300 (:6361), Fusion Core 850 → 750 (:10907), Assimilator 450 / 450 → 300 / 300 (`voidmulti` UnitData:50, :52), Forge 500 / 500 → 400 / 400 (`swarm` UnitData:1613, :1615), Cybernetics Core 500 / 500 → 550 / 550 (`liberty` :10807, :10811), Stargate 500 / 500 → 600 / 600 (:9630, :9634), Fleet Beacon 750 / 750 → 500 / 500 (:9698, :9702), Pylon armour 0 → 1 (:4325), Photon Cannon armour 0 → 1 (:11045) | 15 | none |
| g2 | Structure sight: Sensor Tower 22 → 12 (`liberty` UnitData:9480; its radar is not modelled, B-69), Factory 10 → 9, Starport 10 → 9, Tech Lab 7 → 9, Reactor 7 → 9, Pylon 9 → 10 (`voidmulti` UnitData:1300), Stargate 10 → 9, Fleet Beacon 10 → 9, Hatchery 11 → 12 (`voidmulti` :649), Lair 11 → 12 (`voidmulti` :932), Spire 10 → 9, Greater Spire 10 → 9 | 12 | none |
| g3 | Costs: Armory gas 100 → 50 (`voidmulti` UnitData:2100), Dark Shrine 100 / 100 → 150 / 150 (`void` UnitData:1914), Liberator gas 150 → 125 (`voidmulti` UnitData:468), Ultralisk 300 → 275 (`voidmulti` UnitData:1940); research costs of 15 upgrades (Vehicle and Ship Plating 100 / 175 / 250, Protoss Air Weapons and Air Armor levels 2 and 3 175 / 250, Zerg Ground Carapace levels 2 and 3 200 / 250, Flyer Attacks levels 2 and 3 175 / 250, Flyer Carapace 100 / 175 / 250, Grooved Spines 75 / 75; `voidmulti` AbilData and `liberty` AbilData, cited per row in the test) | 36 | costs decide no battle |
| g4 | Spore Crawler damage 15 → 20 (`SporeCrawlerU`, `voidmulti` EffectData:871) | 1 | none |
| g5 | Upgrade targets: Neosteel Armor gives +2 armour to every Terran structure (`TerranBuildingArmor`, `liberty` UpgradeData:842-898; ours: the Bunker only, B-51); Advanced Ballistics adds range only to the Defender Mode weapon (`LiberatorAGRangeUpgrade`, `voidmulti` UpgradeData:154-155; ours lengthened the anti-air weapon, recorded as **B-87**) | 2 upgrades | none |
| g6 | Observer shields 20 → 30 (`voidmulti` UnitData:1137) | 1 | none |
| g7 | Attributes: SCV + Biological (`liberty` UnitData:4799), Sentry − Light (`voidmulti` UnitData:1553), Infestor + Armored (`liberty` UnitData:10113) | 3 | SCV (22a) |
| g8 | Ravager damage 13 → 16 (`RavagerWeaponDamage`, `void` EffectData:1603) and +1 → +2 a level (`void` UpgradeData:517) | 2 | none |
| g9 | Unit life and armour: Viking 125 → 135 (`voidmulti` UnitData:2000), Hellion armour 1 → 0 (no layer sets it; doc 07 says 0), Thor armour 2 → 1 (`liberty` UnitData:5208) | 3 | Viking, Hellion, Thor |
| g10 | Unit sight: Dark Templar 9 → 8 (`liberty` UnitData:3376), Roach 10 → 9 (:7210), Brood Lord 11 → 12 (:11132) | 3 | Roach, Brood Lord |
| g11 | Weapon range: Dark Templar and Zergling 0.2 → 0.1 (`liberty` WeaponData:165, :361), Thor anti-air 10 → 11 (`voidmulti` WeaponData:130) | 3 | Zergling, Thor |
| g13 | Bonus damage per upgrade level: Siege Mode Armored +5 → +1 (`voidmulti` UpgradeData:305); Viking and Void Ray Armored +1 → 0 (no `AttributeBonus` entry in any layer's weapon upgrades) | 4 | none (no matrix upgrades) |
| g12 | Collision radius (`CUnit` `Radius`) of 26 units, for example Siege Tank 0.75 → 0.875 (`liberty` UnitData:5094), Stalker 0.5 → 0.625 (:3973), Roach 0.5625 → 0.5 (class default, `core` UnitData:41) | 26 | 14 of them |
| rule | Structure attributes are derived in `catalog.rs` (Psionic for a shielded structure, Mechanical otherwise); the game gives Protoss structures Armored and Structure only and Zerg structures Armored, Biological and Structure (for example Nexus `liberty` UnitData:4166, Hatchery :8193). Recorded as **B-88** | 33 | none |

**Not deviations.**
- **The High Templar has a weapon at 5.0.14** (`voidmulti` UnitData:683 adds `HighTemplarWeapon`, `voidmulti`
  WeaponData:284: range 6, period 1.754); §6.1's note that it has none read the `liberty` layer only. The analogue's
  weapon stands; its values are class A.
- Damage per upgrade level, after the indexed merge: every catalog value matches, except the Ravager's (g8) and the
  Baneling's structure effect (B-82).

**B-6, read and not applied.** `RangeSlop` is 1 by default (`core` WeaponData:13) and 0, 1.25 or 2 on nine weapons
(for example 0 on the Hydralisk's and the Lurker's, `liberty` WeaponData:372, :505; 2 on the Siege Mode cannon,
:402). Ours is one constant, 0.5. Applying it is a rule (a per-weapon field) with its own latent commit and a
matrix-wide measurement; it is not part of this package's data groups and stays in the register.

**Indexed upgrade overrides.** The resolver merges a `…Level1` upgrade's `EffectArray` parent first, then each
layer's entries, an `index` updating that slot. It reproduces every per-level value Liquipedia gives where the
override names its reference or only sets a value, but one case shows its numbering is not certain: `voidmulti`
UpgradeData:911 (index 13, a Zergling icon) lands on the slot of the Baneling's Light +2 (`liberty` UpgradeData:509)
in the merge. That value was read from its own line (WP6, §6.1) and is kept.

### 10.2 Data groups and measurements

Order as in the table of §10.1, groups with no matrix identity first; each group's test in
`crates/sim_core/tests/catalog_reference_values_tests.rs` covers every identity of its field kind and failed
before its data. Matrix: 20 seeds a battle (1000 runs); battle 33: 68 seeds. Directories under
`~/scratch/sc2dev_wp9/<group>/`.

| Group | Commit | Matrix (against the previous group) | Battle 33 | Goldens |
|---|---|---|---|---|
| g1 structure vitals | `9dcaa0e` | no structure in the matrix | — | unchanged |
| g2 structure sight | `2d5eb17` | no structure | — | unchanged |
| g3 costs | `b1e894f` | costs decide no battle | — | unchanged |
| g4 Spore Crawler | `77d26e8` | no structure | — | unchanged |
| g5 upgrade targets | `aab7041` | no upgrades researched | — | unchanged |
| g7 attributes | `a5d7989` | bit-identical (the SCV of 22a; no matrix weapon has a Biological bonus) | 54 / 7 / 7 | unchanged |
| g8 Ravager | `7d9fb44` | not spawned | — | unchanged |
| g13 bonus per level | `e73667e` | no upgrades researched | — | unchanged |
| g6 unit vitals | `eacdf18` | 9a, 10, 12c, 19, 35 moved, no winner changed; 12c and 19 re-recorded (the Viking's untouched hull 125 → 135) | bit-identical | unchanged |
| g10 unit sight | `69cad65` | no result moved | bit-identical | re-recorded (Roach sight, §10.3) |
| g11 weapon range | `aafc247` | 9a, 10, 23a, 23b, 24a, 24b, 26–29 moved (Zergling melee 0.1), no winner changed | bit-identical | unchanged |
| g12 collision radii | `8626edf` | 24 battles moved, no winner changed; 35: Viking wins 12 → 20 of 20 (the rest were unresolved); 31: two seeds changed winner (19 / 1 overall either way) | bit-identical | re-recorded (§10.3) |

Expectation failures after every group: 0 on the seeds of record and 0 over all seeds. Battle 33 stays
54 / 7 / 7 (marine survivors 5.29, zealot survivors 0.37): no group touches the Marine or the Zealot, so its
expectation `blue (0, 13), red (0, 3)` is not re-recorded.

**Held: the Thor's armour (g6).** 2 → 1 (`liberty` UnitData:5208) flips battle `14_thor_vs_ultralisk` from the
Thor on all 20 seeds (survivors 1.00 – 0.00, Thor hull 4.00) to the Ultralisk on all 20 (0.00 – 1.00, Ultralisk hull
26.87); 12 expectation failures on the seeds of record. Per the stop rule it is not committed: the patch is
`~/scratch/sc2dev_wp9/held/g6_unit_vitals_full.patch` (its `colossus_mech` hunk), the Thor is exempt from the
vitals table, and B-47 keeps it as its open part until the orchestrator decides.

### 10.3 Goldens

- **g10:** the Roach squad (`blue_armor`) sees 9; what it sees is world state from tick 1, so every checkpoint of
  both fixtures changed. With only the Roach's sight restored to 10 both reproduce as recorded for WP8b.
- **g12:** the Roach (0.5625 → 0.5) and the Siege Tank (0.75 → 0.875) radii are hashed body state: every checkpoint
  of `golden_replay` and checkpoints 1–60 of `golden_replay_orders` changed. With only those two radii restored
  both reproduce as recorded for g10.
- **B-82 rule (§10.4):** checksum schema 15 → 16; both fixtures and the sandbox checksum re-recorded for the
  encoding alone.

Each re-recording reproduces in debug, release, and `wasm32-wasip1` debug and release (`wasmtime`).

### 10.4 B-82: the Baneling's structure strike

- **Rule** (`3bbc750`, latent): `WeaponSpec::structure_strike` (damage, per-level growth, whether armour applies;
  serde default `None`, the engine default). A structure struck by such a weapon takes it, with no attribute bonus,
  instead of the weapon's own strike; without armour neither life nor shield armour reduces it. Checksum schema
  16. Test `b82_a_structure_strike_replaces_the_strike_against_structures` (did not compile before). Matrix and
  battle 33 bit-identical to g12.
- **Data** (`2f8af7f`): the Baneling analogue, 80, +5 a level, ignoring armour (`VolatileBurstU2`, `liberty`
  EffectData:1058, :1063; UpgradeData:505). No matrix battle has a structure: bit-identical. Its centre on the
  Baneling stays B-83.

### 10.5 What WP9 leaves open

- **B-47:** fixed; the Thor's armour was applied by plan Amendment 15 (battle 14 re-recorded: the Ultralisk now
  wins all 20 seeds with 26.8 life left, the Thor taking 34 a strike instead of 33). Open: the structure attributes
  (B-88, a rule: a required `attributes`
  field on structures; nothing in the catalog reads Psionic, Mechanical or Biological against a structure today).
- **B-6:** the per-weapon `RangeSlop` (§10.1), a rule with its own measurement.
- **B-86:** morph and Zergling pair costs, with the morph model (WP17).
- **B-87** is fixed by g5; **B-51** and **B-82** are fixed.


## 11. WP10: turning follow-ups (B-28, B-29, B-30)

Sources: SC2GameData `5444e12f13`, chain `core → liberty → swarm → void → voidmulti` (catalog-time audit §2.2), and
the SC2Mapster editor reference for the field semantics ([Data/Weapons/Legacy](https://sc2mapster.wiki.gg/wiki/Data/Weapons/Legacy)
as in audit §10.3; [Data/Turrets](https://sc2mapster.wiki.gg/wiki/Data/Turrets), read 2026-11-09). Matrix: 20 seeds a
battle (1000 runs); battle 33: 68 seeds. Directories `~/scratch/sc2dev_wp10/<step>/`; baseline `base` reproduces
WP9's last matrix bit for bit.

### 11.1 Results

| Step | Commit | Matrix (against the previous step) | Battle 33 | Goldens |
|---|---|---|---|---|
| B-29 rule, schema 17 | `91ecbef` | bit-identical | 54 / 7 / 7 | re-recorded for the encoding (§11.5) |
| B-29 data | `26e3bf6` | 15, 27, 28, 29, 32, 33, 34, 35 moved, no overall winner changed; single-seed winner changes: 32 (2 seeds; 4 / 8 → 4 / 10 wins), 33 (2; 17 / 1 → 18 / 0) | 55 / 6 / 7 (marine survivors 5.29 → 5.53) | re-recorded (§11.5) |
| Hellion mount | `95a3ce4` | 10 moved (end ticks), winner unchanged | bit-identical | unchanged |
| Immortal mount | `12dc0ac` | 28, 30 moved (28: survivors 36.75 → 36.95), winners unchanged | bit-identical | unchanged |
| Colossus mount | `ca6e1e4` | bit-identical | bit-identical | unchanged |
| Lurker mount | `1abc38b` | bit-identical (battle 21 included) | bit-identical | unchanged |
| Phoenix mount | `379bf4f` | bit-identical (not spawned) | bit-identical | unchanged |
| Thor anti-air mount | `b81f8c4` | bit-identical (no air target) | bit-identical | unchanged |
| B-30 rule, schema 18 | `9b9cc48` | bit-identical | bit-identical | re-recorded for the encoding (§11.5) |
| B-30 data | `0b91676` | 28 (survivors 36.95 → 37.80), 30 (end ticks) moved, winners unchanged | bit-identical | unchanged |

Expectation failures after every step: 0 on the seeds of record and 0 over all seeds. No battle other than 33 changed
its overall winner, so nothing was held. Battle 33's expectation `blue (0, 13), red (0, 3)` holds (its 20 matrix seeds
now span blue 1 to 13, red 0 to 2) and is not re-recorded; its comment's distribution is rewritten in `26e3bf6`.

### 11.2 B-29: two arcs

**Rule.** `Arc` is the angle within which a weapon acquires a target without turning; `ArcSlop` how far outside it
the target may go before the weapon loses it (audit §10.3). `WeaponSpec` gains `acquire_arc` (catalog
`acquire_arc_deg`, a required half angle, `0 ≤ acquire ≤ firing_arc_deg`) beside `firing_arc` (the engaged arc,
`Arc` ÷ 2 + `ArcSlop`), and `Entity` gains `engaged_target`: set when a swing starts, cleared when the weapon works
against another target or none, or when the target's bearing leaves the engaged arc. A swing against the engaged
target starts within the engaged arc, against any other within the acquisition arc. Tests in
`weapon_cadence_tests.rs`: a new target behind a turret is swung at on the call it is faced exactly (16), not on the
call it enters the engaged 30° (14); a target knocked 45° off is re-acquired exactly (call 24), not at 22.5° (22).
Both failed on the old rule.

**Data** (`26e3bf6`): `Arc` ÷ 2 for every weapon whose mount matches: 0 where `Arc` is unset (no `CWeapon` default
sets it, `core` WeaponData:3-31), 180 for the Siege Tank's tank cannon (`voidmulti` WeaponData:5) and the Carrier
analogue (`liberty` WeaponData:606), 45 for the Planetary Fortress (`liberty` WeaponData:138). Tables
`SC2_UNIT_ACQUIRE_ARC` and `SC2_STRUCTURE_ACQUIRE_ARC` in `catalog_reference_values_tests.rs`. With an acquisition
arc of 0 a unit faces a new target exactly before its first swing; the marines of battle 33 gain slightly (one more
win in 68).

### 11.3 B-28: the six mounts

| Analogue | Block | Now | Turret rate (per tick) | Arcs engaged / acquisition | Source |
|---|---|---|---|---|---|
| Hellion | `weapon` | turret | 55.99 | 11.25 / 0 | `liberty` UnitData:11603, TurretData:97 |
| Immortal | `weapon` | turret | 40.32 | 11.25 / 0 | `liberty` UnitData:10791, TurretData:26, :73 |
| Colossus | `weapon` | turret | 55.99 | 56.25 / 45 | `liberty` UnitData:4099, TurretData:51, WeaponData:500 |
| Lurker | `weapon` | turret | 156.79 | 11.25 / 0 | `void` UnitData:775, TurretData:18; `LurkerMP` sets no `Arc` (`swarm` WeaponData:215, `void` :14, `voidmulti` :144) |
| Phoenix | `air_weapon` | turret | 30.20 | 2.4994 / 2.4994 | `swarm` UnitData:1805, TurretData:33, WeaponData:179-180 |
| Thor | `air_weapon` | hull | — | 14.0625 / 2.8125 | `liberty` UnitData:5270-5271 (no turret), `void` WeaponData:82 |

**Correction to audit §10.5.** In 5.0.14 the attacking Lurker (`LurkerMPBurrowed`) mounts its weapon on the `Lurker`
turret (`void` UnitData:775, `YawRate` 2799.8437), not on `FreeRotate` (`liberty` UnitData:797, which `void`
overrides). Table `SC2_WEAPON_MOUNTS`, test `b28_weapon_mounts_follow_starcraft`, one row added per commit, each red
before its data.

**Found: B-89.** The Liberator's unit lists a second weapon entry with the `Liberator` turret and no weapon
(`void` UnitData:3317-3318) besides `LiberatorMissileLaunchers` with none. Whether the anti-air weapon turns on that
turret cannot be read from the data; it is recorded as B-89 (research), not changed.

### 11.4 B-30: turret idle behaviour

**Rule** (`9b9cc48`). `TurretIdle` from `CTurret` `Idle` and `YawIdleRate`, with the reference's definitions: `Hold`
keeps the angle; `Reset` turns towards the hull's facing; `ResetIfMoving` and `FollowMovement` do so on a tick the unit
was displaced; `Spin` turns by its signed rate every tick (positive counter-clockwise, as the game's angles). It acts
only on a tick the weapon has no target. Hull weapons author `Hold`. Tests: following, resetting and spinning turrets
failed with the rule's call removed.

**Data** (`0b91676`): Immortal `FollowMovement` 20.16 (`liberty` TurretData:74-75), Colossus `FollowMovement` 5.04
(:50-52), Missile Turret `Spin` 2.52 (:85-86); every other turret `Hold` (`FreeRotate`, `liberty` TurretData:24;
the Lurker's own, `void` TurretData:16). Test `b30_turret_idle_follows_starcraft`.

**Not sourced: Q0065.** The `SiegeTank`, `Hellion` and `Phoenix` turrets use `ResetIfMoving` and set no `YawIdleRate`;
no layer's default sets one (`core` TurretData:3-10) and the reference states none. They hold until the owner answers
[Q0065](../questions/closed/0065-turret-idle-rate.md); B-30 stays open for that part only.

### 11.5 Goldens

- **Schema 17** (`91ecbef`) and **schema 18** (`9b9cc48`): both fixtures and the sandbox checksum re-recorded for the
  encoding alone; with the new fields left unhashed at the previous schema both fixtures reproduce as recorded.
- **B-29 data** (`26e3bf6`): `golden_replay` differs from checkpoint index 3 (ticks 151 to 200), `golden_replay_orders`
  from index 4 (ticks 41 to 50), the first fights; with the acquisition arcs back at the engaged arcs both reproduce
  as recorded for schema 17.

Each re-recording reproduces in debug, release, and `wasm32-wasip1` debug and release (`wasmtime`).

### 11.6 What WP10 leaves open

- **B-30**, narrowed to the three `ResetIfMoving` return rates (Q0065).
- **B-89**, the Liberator's turret entry (research).
- `has_turret` also decides, in our movement code, that a wind-up does not hold the unit and that a turret tracks
  `current_target` under any order. Turreting the Hellion, Immortal, Colossus, Lurker and Phoenix gives them that
  behaviour, which the game ties to `AllowedMovement` instead. Recorded as **B-90** for WP13e, beside B-45; no matrix
  battle moved for it beyond the rows above.

## 12. WP11: random delays (B-3, B-4, U-1, U-22)

Sources: SC2GameData `5444e12f13`, chain `core → liberty → swarm → void → voidmulti`; the SC2Mapster editor reference
([Data/Weapons/Legacy](https://sc2mapster.wiki.gg/wiki/Data/Weapons/Legacy) and
[Data/Abilities/Morph](https://sc2mapster.wiki.gg/wiki/Data/Abilities/Morph), read 2026-11-09); Blizzard's
[StarCraft II Balance Revamp 2018](https://starcraft2.com/news/22372713) (quoted on
[devtrackers.gg](https://devtrackers.gg/starcraft2/p/41c0dca3-starcraft-ii-balance-revamp-2018), read 2026-11-09).
Liquipedia's Cooldown, Damage Point and Siege Tank pages (wikitext, read 2026-11-09) state neither delay. Matrix: 20
seeds a battle (1000 runs); battle 33: 68 seeds. Directories `~/scratch/sc2dev_wp11/<step>/`; baseline `base`
reproduces WP10's last matrix bit for bit.

### 12.1 Research

**U-1, the weapon delay.** `CWeaponLegacy` sets `RandomDelayMin` −0.0625 and `RandomDelayMax` 0.125 Normal s (`core`
WeaponData:35-36). The reference: "A random value between the maximum and minimum is selected and used as a delay
before firing the weapon". Blizzard, 2018: "While virtually all units in StarCraft II have a random delay, the
Battlecruiser's high random delay ratio to the auto-attack cooldown could cause moments where the Battlecruiser would
do less damage than expected" (patch 4.7.1 removed it), so the delay adds to the cycle: the mean period is `Period` +
0.03125 Normal s. The negative minimum places it between attacks, where it can shorten one. Drawn per attack; the
reference names no distribution, so "a random value between" is read as uniform. Not sourced: whether an
attack-rate buff scales it, and whether the first attack draws one ([Q0066](../questions/closed/0066-random-delay-scaling.md),
B-91, default: neither).

Overrides at 5.0.14: `PointDefenseLaser` 0/0 (`liberty` WeaponData:76-77), the Battlecruiser's `ATSLaserBattery` and
`ATALaserBattery` (:285, :297, then 0/0 in `voidmulti` :22-40), `D8Charge` 0.1/0.5 (:336-337),
`BattlecruiserWeaponSwitch`, `MothershipCoreWeapon`, `Oracle` and `BypassArmorDroneWeapon` 0/0 (`voidmulti` :68-69,
:151-152, :165-166, :281-282). None is the weapon of a shipped analogue, and every other weapon is a `CWeaponLegacy`
except the Interceptor beams (`CWeaponStrafe`); the Carrier / Mothership composite's catalog weapon follows its
`InterceptorLaunch` (Legacy, no override), as its acquisition arc does (§11.2), and the saucer without an analogue
takes the engine default.

**U-22, the morph.** The reference: Section Delay "is how long after the morph begins before [the section] start[s]
to become that of the final unit", Section Duration "the time over which [it] transition[s]", and the morph's total
time is "the single section with the greatest sum of time": delay and duration add. `SiegeMode` (`liberty`
AbilData:1301-1326): `Actor` and `Stats` 0.5 + 3.5417 = 4.0417 Normal s; `Unsiege` (:1328-1341): 3.5417 s. Both set
`RandomDelayMax="0.5"` (:1308, :1334; no `RandomDelayMin`, so 0), which "will be added just before the morph starts".
No later layer changes a time (`swarm` AbilData:351, :1353; `void` :805; `voidmulti` :915, :1112). This corrects
combat alignment evidence §13.7, which read Unsiege as having no random part.

### 12.2 Results

| Step | Commit | Matrix (against the previous step) | Battle 33 (68 seeds) | Goldens |
|---|---|---|---|---|
| B-3 rule, schema 19 | `006d804` | bit-identical | 55 / 6 / 7 | re-recorded for the encoding (§12.5) |
| Published swing at a delayed release | `46465f9` | (presentation only) | — | unchanged |
| B-3 data | `ad5f6eb` | 47 battles moved, no overall winner changed; single-seed winner changes: 14 (5 seeds; Ultralisk 20 / 20 → 15 / 20), 31 (2; 19 / 1 → 19 / 0), 32 (2; 4 / 10 → 4 / 8), 33 (7; 18 / 0 → 12 / 1), 35 (3; 20 / 0 → 17 / 0, 3 unresolved) | 41 / 10 / 17 (marine survivors 5.53 → 4.32, SD 3.02; zealot survivors 0.74, SD 1.13) | re-recorded (§12.5) |
| B-4 rule | `1e3dddd` | bit-identical | bit-identical | unchanged |
| B-4 data | `a8b961a` | bit-identical (no battle sieges) | bit-identical | unchanged (no fixture sieges) |

**Battle 14** (Thor against Ultralisk) is a near-tie since the Thor's armour (Amendment 15): the Ultralisk won all 20
seeds keeping 26.8 of 500 life. With the delay the Thor wins 5 (two of them seeds of record), keeping 26 of 400; the
Ultralisk wins 15, keeping 26.9 to 55. The overall winner is unchanged, so the stop rule does not hold the commit;
the expectation `blue (0, 0), red (1, 1)` failed on those 5 seeds and is re-recorded in `ad5f6eb` to `blue (0, 1),
red (0, 1)` with the claim "An unupgraded ultralisk and a thor are a near-tie at 5.0.14 armour; the ultralisk wins
most". After it: expectation failures 0 on the seeds of record and 0 over all seeds.

**Battle 33** keeps its expectation `blue (0, 13), red (0, 3)`: on its 20 matrix seeds the marines win 12 and the
zealots 1 (blue 0 to 9, red 0 to 3); the seeds of record are now 5-0, 9-0, 2-1 (unresolved) and 8-0. The comment's
distribution is rewritten (combat plan D-6). On the 68 seeds the range is blue 0 to 10, red 0 to 5: one seed outside
the expectation, none of the 20 the expectation covers.

### 12.3 B-3: the rule

`WeaponSpec` gains `random_delay_min` and `random_delay_max` (ticks; catalog `random_delay_ticks: (min, max)`,
required), `Entity` gains `weapon_delay_carry`. When a release discharges, `combat::charge_random_delay` draws the
delay uniformly over the fixed-point values of the range from `SimWorld::rng` (the combat phase steps entities in id
order, once per release), adds it to the carry, and moves the carry's whole ticks into the cooldown, so the shot times
are the exact sums of periods and delays quantised to ticks. Equal bounds add a fixed delay without a draw; (0, 0)
leaves the generator untouched, which is what made the rule commit bit-identical. Validation: `min <= max`, and `min`
above minus the charged cooldown. Tests (`weapon_cadence_tests.rs`, four red before the rule): a 10-tick period with
a delay in [−1, 2] ticks releases every 9 to 12 ticks with a mean of 10.5 over 6000 ticks; a fixed half-tick
alternates 10 and 11 without a draw; the same seed replays the same shots and another seed differs; the range rules
reject (1, −1) and (−9.5, 0).

`RenderSnapshot`'s swing counted `period − cooldown left`, which stepped back into the wind-up for up to two ticks at a
delayed release; it now holds at the release (`46465f9`, unit test red before). Five cadence tests that pin a shipped
weapon's exact period (`attack_swing_snapshot_tests` ×2, `sc2_battle_benchmarks_tests`, `weapon_period_tests`,
`trace_tests`) remove the delay through `sim_fixtures::without_random_delays`.

**Data** (`ad5f6eb`): every weapon of `units.ron` and `structures.ron`, the Siege Mode cannon included, (−1.1161,
2.2321) ticks = (−0.0625, 0.125) × 25 ÷ 1.4. Test `b3_every_weapon_has_the_legacy_random_delay`.

### 12.4 B-4: the rule and the data

A mode has `cast_ticks` (entering), `leave_cast_ticks`, and for each direction `*_random_delay_max_ticks`: when the
toggle is committed, an extra drawn uniformly from `0..=max` ticks with `SimWorld::rng` and rounded half up is added
to the channel. A maximum of 0 draws nothing (`1e3dddd`, bit-identical with 71 / 71 / 0 / 0). Tests
(`spells_and_abilities_tests.rs`, red: the fields did not exist): 10 ticks in and 4 out with no extra and no draw;
with up to 3 extra ticks, 40 seeds give 10 to 13 in and 4 to 7 out, at least three distinct lengths, the same per
seed on a replay. Data (`a8b961a`): 72 in (4.0417 s), 63 out (3.5417 s), each plus up to 8.9286 (0.5 s), so sieging
takes 72 to 81 ticks and unsieging 63 to 72. Test `b4_siege_mode_timings_follow_starcraft`. The legacy
`AbilityEffectSpec::SiegeModeToggle` effect, which no shipped catalog uses, keeps its one `cast_ticks`.

### 12.5 Goldens

- **Schema 19** (`006d804`): both fixtures and the sandbox checksum re-recorded for the encoding alone; with the new
  fields unhashed at schema 18 both fixtures reproduce as recorded.
- **B-3 data** (`ad5f6eb`): `golden_replay` differs from checkpoint index 3 (ticks 151 to 200), `golden_replay_orders`
  from index 4 (ticks 41 to 50), the first fights, as for B-29 (§11.5); with the delays at (0, 0) both reproduce as
  recorded for schema 19.

Each re-recording reproduces in debug, release, and `wasm32-wasip1` debug and release (`wasmtime`).

### 12.6 What WP11 leaves open

- **B-91** ([Q0066](../questions/closed/0066-random-delay-scaling.md)): whether an attack-rate buff scales the delay,
  and whether the first attack draws one.
- The morph random delays of other modes (burrow, the Viking's) belong to the packages that add those modes (WP13).

## 13. WP12: acceleration and deceleration (B-39)

Sources: SC2GameData `5444e12f13`, chain `core → liberty → swarm → void → voidmulti` (the catalog time audit's
references, §4, each value re-read at its line). The mover rule itself (how the engine applies `Acceleration` and
`Deceleration` each game loop) is engine code with no published source: neither the SC2Mapster editor references nor
Liquipedia describe it, so the rule below is the plain reading of the field names, stated as such. Matrix: 20 seeds a
battle (1000 runs); battle 33: 68 seeds. Directories `~/scratch/sc2dev_wp12/<step>/`; baseline WP11's last matrix
(`~/scratch/sc2dev_wp11/b4d`).

### 13.1 Research

**Units and conversion.** `Speed`, `Acceleration` and `Deceleration` are game-time values (Normal seconds), as
every speed in the audit: `Speed` in units per Normal second, the two rates in units per Normal second squared. One
tick is 1.4 ÷ 25 = 0.056 Normal s, so a rate converts as Normal × 0.056² = Normal × 1.4² ÷ 25² = Normal × 0.003136
world units per tick per tick, the plan's formula. Check: the Mutalisk (`Speed` 4, `Acceleration` 3.5) reaches full
speed in 4 ÷ 3.5 = 1.14 Normal s = 20.4 ticks; in the catalog 0.224 ÷ 0.010976 = 20.4 ticks.

**Acceleration.** Every shipped analogue's value is set in the data (the audit's class E rows): 1000 for every ground
combat unit (instant: 3.136 per tick², above any speed); finite for the workers (SCV `liberty` UnitData:4732, Probe
:3075, Drone :7041: 2.5) and the air units: Phoenix 3.25 (:3686), Viking 2.625 (:6839), Medivac 2.25 (`swarm`
UnitData:1706), Liberator 3.5 (`void` UnitData:3297), Banshee 3.25 (`liberty` :5377), Raven 2 (:5579), Observer 2.125
(:3423), Warp Prism 2.625 (`voidmulti` UnitData:2043), Void Ray 2 (`liberty` :3758), Carrier 1.0625 (:3483), Mutalisk
3.5 (`void` :3092), Corruptor 3 (`voidmulti` :269), Brood Lord 1.0625 (`liberty` :11113), Viper 3 (`void` :1417),
Overlord 1.0625 (`liberty` :7366). The plan's B-39 row named the Hellion as finite; its value is 1000 (`liberty`
:11552), instant.

**Deceleration.** No `CUnit` default sets it, and only the High Templar (1000, `liberty` UnitData:3259) and the
Overlord (1.625, `voidmulti` UnitData:1199) among the analogues do. What the engine does with an unset (zero)
`Deceleration` is not sourced: [Q0067](../questions/closed/0067-unset-deceleration.md), default no change (unset stays
instant; the catalog keeps its copy of `max_speed`).

**Not modelled.** `LateralAcceleration` (46.0625 for most ground units, e.g. `liberty` UnitData:4823; 1000 for the
Medivac, `void` :1489) bounds how sharply a moving unit can curve; our units turn their velocity freely within the hull
rate. Recorded as **B-92**.

### 13.2 The rule

`MotionEntry` gains `step`, this tick's own-motion budget: the effective top speed (upgrades and buffs included),
capped at the length of the velocity the unit achieved on the tick before plus its archetype's `acceleration`. A
walking unit whose `deceleration` is finite lowers it further to `braking_speed(d)`, the speed from which a run
losing `deceleration` per tick covers exactly the `d` world units left along its route: the root `v` of
`v² ÷ (2 dec) + v ÷ 2 = d`, never below `min(d, dec)`, so it always snaps onto its goal. A rate at or above the
archetype's `max_speed` is instant (the game's 1000); comparing with the archetype's speed, not the effective one,
keeps a stimmed unit instant too. The budget replaces the top speed in the step and in the "within one step"
arrival checks; collision pushes, neighbour margins and the progress checks keep the top speed. No field is added
to `Entity`: the speed carried between ticks is the achieved velocity, already hashed, so the checksum schema stays
19.

Scope, recorded here as the plan's D-2 allows: a unit stops at once when an order ends for any other reason than
arriving (Stop, a target in range, a blocked step), and pursuit does not brake. With the data of Q0067's default only
the Overlord analogue has a finite deceleration, and it has no weapon; these cases return with Q0067 if it is
answered "use `Acceleration`".

Tests (`acceleration_tests.rs`; three of five red before the rule): from rest a unit's speed is 0.02, 0.04, ... 0.1
for acceleration 0.02 and top speed 0.1, ground and air alike; acceleration equal to the top speed moves the whole
speed on the first tick; instant deceleration arrives at full speed; deceleration 0.01 slows the unit by at most
0.01 a tick and it stops exactly on its goal.

### 13.3 Results

| Step | Commit | Matrix (against the previous step) | Battle 33 (68 seeds) | Goldens |
|---|---|---|---|---|
| Rule | `f71fa94` | bit-identical | bit-identical (41 / 10 / 17) | unchanged |
| Acceleration data | `e11a118` | 3 battles moved, no overall winner changed (below) | bit-identical | `golden_replay_orders` re-recorded (§13.4) |
| Deceleration data | `e9cd87b` | bit-identical (no battle spawns an Overlord analogue) | bit-identical | unchanged |

**Acceleration data, the moved battles** (20 seeds each; outcome blue / red / unresolved):
- **22a** (Reaper against two workers): 0 / 0 / 20 unchanged; the workers start their flight from rest, so the
  per-tick rows differ, survivors and end ticks do not.
- **34** (20 Hydralisks against 12 Mutalisks): 20 / 0 / 0 unchanged; Hydralisk survivors 18.90 → 19.15, mean end
  tick 223 → 238. The Mutalisks reach the fight later and arrive at lower speed.
- **35** (12 Vikings against 6 Colossi): 17 / 0 / 3 → 13 / 0 / 7; 8 seeds changed outcome (6 blue → unresolved, 2
  unresolved → blue), mean end tick 416 → 571. The Vikings keep all 12; a Viking that stops to fire and then
  follows its target restarts from rest each time, so in 7 seeds one Colossus survives to the 900-tick limit. Two
  of the four seeds of record became unresolved (22, 33) and one resolved (11); the expectation holds on all 20.
  The overall winner is unchanged (blue), so the stop rule does not hold the commit.

Expectation failures: 0 on the seeds of record, 0 over all seeds, after each step; no expectation was re-recorded.

**Tests adjusted by the acceleration data** (not weakened: each pins what it was written for on an instant unit):
`checked_movement_tests` (overflow on a 60 000-cell flight, first-tick position) and `movement_contact_tests`
(`buffed_unit_keeps_its_speed_through_friendly_contact`, first-tick distance) set their unit's acceleration to its
speed; `cluster_arrival_tests` gives the air unit 48 ticks instead of 16 to walk its one cell (it needs about 20
from rest). Client lib, `headless_tests` and `skirmish_match_tests` pass unchanged (the skirmish's workers
accelerate).

### 13.4 Goldens

`golden_replay_orders` is re-recorded in the acceleration data commit: its Phoenix spotter (`blue_spotter`) starts its
first move from rest, so checkpoint 1 (ticks 11 to 20) and every later one differ. `golden_replay` spawns no worker
or air unit that moves and is unchanged. The rule commit left both as recorded (the latent catalog makes every rate
instant). The re-recording reproduces in debug, release, and `wasm32-wasip1` debug and release (`wasmtime`).

### 13.5 What WP12 leaves open

- **B-39, narrowed** ([Q0067](../questions/closed/0067-unset-deceleration.md)): how an unset `Deceleration` brakes.
  Until answered it is instant, which also leaves out braking for a unit whose order ends without arriving, and for
  pursuit.
- **B-92**: `LateralAcceleration` (turning arcs of moving units) is not modelled.

## 14. WP13a: the Immortal's Barrier (B-41)

Sources: SC2GameData `5444e12f13`, chain `core → liberty → swarm → void → voidmulti`, each line re-read. Matrix: 20
seeds a battle (1000 runs); battle 33: 68 seeds. Directories `~/scratch/sc2dev_wp13a/<step>/`; baseline WP12's last
matrix (`~/scratch/sc2dev_wp12/decel`).

### 14.1 Research

The 5.0.14 Barrier is a chain of behaviours, not a cast ability:

- The Immortal carries `BarrierDamageResponse` and `ImmortalOverload` (`voidmulti` UnitData:775-776).
- `BarrierDamageResponse` (`voidmulti` BehaviorData:577-590) responds to any damage (`Chance` 1) with a cooldown of
  45 Normal s, running `ImmortalBarrierAfterfirstdamageeffect` (EffectData:2395-2399): it applies `TakenDamage`
  (BehaviorData:599-609, `Duration` 3, through `ImmortalOverloadAB`, EffectData:2408-2410) and
  `ImmortalBarrierSupressed` (BehaviorData:592-598, `Duration` 45, through EffectData:1314-1318).
- `ImmortalOverload` (`void` BehaviorData:97-108, overridden by `voidmulti` :273-279) has no duration, absorbs every
  point (`ModifyFraction` 0) up to `ModifyLimit` 100 (the `void` 200 is overridden), and is enabled only while the
  response's cooldown runs (`BarrierNoGravitonBeamOrBarrierOnCooldown`, ValidatorData:905-908 and :698-703).
  Exhausting it removes `TakenDamage` (EffectData:2400-2403); `TakenDamage` expiring removes it (:2404-2407);
  `ImmortalBarrierSupressed` expiring restores it (:2391-2394).
- So: the first damage while ready raises an absorb of 100 for 3 Normal s; it is spent at 100 or at the end of the
  window; it can be raised again 45 Normal s after it was raised. In ticks: 3 × 25 ÷ 1.4 = 53.6 → 54 and
  45 × 25 ÷ 1.4 = 803.6 → 804, rounded as the catalog's other times.
- The `ImmortalOverload` ability (`voidmulti` AbilData:585-591, autocast, cooldown 45) the plan's row cited is not in
  the Immortal's command card at 5.0.14 (the card's button shows `BarrierDamageResponse`, UnitData:778); its
  cooldown agrees.

Not stated by the data, and read as follows (owner question
[Q0068](../questions/closed/0068-barrier-damage-order.md), default this reading): the raising hit is itself absorbed;
the barrier acts on the damage before armour (Liquipedia "Damage Calculation" caps Hardened Shield, also a damage
response, before defence; §7.1 quotes it); a hit absorbed whole deals nothing, no 0.5 minimum, and does not restart
the regeneration delays.

### 14.2 The rule

`ArchetypeCapabilities::barrier: Option<BarrierSpec>` (`absorb_limit`, `duration_ticks`, `cooldown_ticks`), validated
(limit positive, duration at least 1, cooldown at least the duration). Per unit, `Entity::barrier: BarrierState`
(ticks raised left, absorption left, cooldown left), hashed: checksum schema 20. `BarrierState::absorb` takes the
raw damage of each strike (direct, every strike of a multi-strike attack, splash after its falloff, storm pulses),
raises a ready barrier, absorbs up to what is left and returns the rest, which is then mitigated by shields and
armour as before; upkeep (phase 2) counts the window and the cooldown down. The window counts the raising tick.

Tests (`barrier_tests.rs`, 6 of 7 rule tests red with the rule disabled): limit then life; no capability; the
window's end; re-raising after the cooldown; before armour (life armour 2, limit 50: 8 reaches life, where after
armour would be 6); splash; storm. Unit tests: `BarrierState` and catalog validation. The shipped-values test was
red before the data commit.

### 14.3 Results

| Step | Commit | Matrix (against the previous step) | Battle 33 (68 seeds) | Goldens |
|---|---|---|---|---|
| Rule | `e79972a` | bit-identical | bit-identical (41 / 10 / 17) | re-recorded for schema 20 alone (§14.4) |
| Data | `34d578c` | 4 battles moved, no overall winner changed (below) | bit-identical | unchanged |

**The moved battles** (20 seeds; outcome blue / red / unresolved unchanged in each):

| Battle | Before | After |
|---|---|---|
| 2b (Immortal against two Roaches) | lead hull 197.25 (20 seeds) | 200.00 |
| 3 (Immortal against a Siege Tank) | lead hull 176 on every seed | 200 on every seed |
| 28 (100 Zerglings against 10 Immortals) | Zergling survivors 36.50, end tick 610 | 28.00, 692 |
| 30 (20 Roaches against 10 Immortals) | Immortal lead hull 165.05 | 190.70 |

Battle 3's expectation (`blue_lead_hp: (176, 176)`) failed on all 20 seeds and is re-recorded to `(200, 200)`, its
claim now saying the Barrier absorbs the first shots; this closes remediation finding M-20 (combat alignment
§13.13). Battle 2a (one Roach) is bit-identical: its Immortal already lost no hull.
Expectation failures after re-recording: 0 on the seeds of record, 0 over all seeds.

### 14.4 Goldens

The rule commit adds `Entity::barrier` to the hash (schema 20). With the field unhashed at schema 19 both golden
replays and the sandbox checksum reproduce as recorded (debug), so the re-recording is the encoding alone; the
re-recorded lists reproduce in debug, release, and `wasm32-wasip1` debug and release (`wasmtime`). The data commit
leaves both as recorded: neither spawns the Immortal analogue.

### 14.5 What WP13a leaves open

- **B-41, narrowed** ([Q0068](../questions/closed/0068-barrier-damage-order.md)): where in a hit the Barrier acts.
- Graviton Beam and Scrambler Missile, which disable the Barrier in the data, are not modelled (no shipped
  analogue casts them); the Barrier has no visual in the client.


## 15. WP13b: the Lurker attacks only burrowed (B-42)

Sources: SC2GameData `5444e12f13`, chain `core → liberty → swarm → void → voidmulti`, each line re-read; Liquipedia
[Lurker (Legacy of the Void)](https://liquipedia.net/starcraft2/Lurker_(Legacy_of_the_Void)) (wikitext,
2026-11-09). Matrix: 20 seeds a battle (1000 runs); battle 33: 68 seeds. Directories `~/scratch/sc2dev_wp13b/`;
baseline WP13a's data matrix (`~/scratch/sc2dev_wp13a/data`).

### 15.1 Research

- **No weapon unburrowed.** `LurkerMP` has no `WeaponArray` in any layer (`liberty` UnitData:690-758; `void` :752-765;
  `voidmulti` :967-985). Its `EquipmentArray` is display only, and `voidmulti` replaces it with `Spinesdisabled`
  (:982), a copy of `LurkerMP` with `Options Disabled` (WeaponData:362-364). Liquipedia: the Lurker "may attack only
  when burrowed".
- **The burrowed form.** `LurkerMPBurrowed` carries `LurkerMP` (`liberty` UnitData:797, turret `Lurker` from `void`
  :775), is `Cloaked` and `Buried` (`liberty` :767-768), collides only with `Burrow` (:775) and moves with the
  `Burrowed` mover (:783), which has no speed. Life, armour and sight equal the unburrowed form's (`voidmulti`
  :987-995).
- **Burrowing.** `BurrowLurkerMPDown` (`swarm` AbilData:1654-1674): `Actor` duration 2 (raised to 2.5 by `void`
  :149-155), `Collide` and `Stats` delay 1.8332, `RandomDelayMax` 0.25, `SuppressMovement`. By WP11's reading of the
  morph (§12.1: the morph's time is its longest section), 2.5 Normal s = 44.64 → **45 ticks**, plus up to 0.25 s =
  **4.4643 ticks**. Cross-check: Adaptive Talons subtracts 1.0 from `Actor` and 0.66 from both delays (`voidmulti`
  UpgradeData:780-783), and Liquipedia's patch 5.0.9 entry "Reduced the effectiveness of adaptive talon from 0.71
  seconds to 1.07 seconds" is (2.5 − 1.0) ÷ 1.4 = 1.07 Faster s, the `Actor` time.
- **Unburrowing.** `BurrowLurkerMPUp`: `Actor` 0.625 Normal s, `RandomDelayMax` 0.25 (`voidmulti` AbilData:181-189,
  overriding `swarm` :1675-1685's 1 and 0.5) = 11.16 → **11 ticks**, plus up to **4.4643**.
- **No automatic burrow for players.** The `TacticalBurrowLurkerMP` validators (`void` ValidatorData:1021-1049) are
  referenced by no ability: they serve the computer AI. A player burrows the Lurker.

Not stated by the data, and read as follows (owner question
[Q0069](../questions/closed/0069-lurker-burrow-sections.md), default this reading): every property of the burrowed form
(weapon, cloak) arrives when the morph ends, not at the `Stats` section's end 12 ticks earlier.

### 15.2 The mechanism needed no rule

The existing mode capability (`UnitArchetype::modes`, WP11's timed channel) already expresses the unit: an archetype
with no weapon of its own whose mode's `ground_weapon_override` arms it. `burrow_attack_tests.rs` pins it on scratch
archetypes (five tests): no attack unburrowed, idle, under an attack order or attack-moving past an enemy (it walks
on); the first hit on the tick the channel ends (cast 10 ticks: hit on tick 10 to 13); after unburrowing no hit and
the unit walks again. All five passed on the engine as it was (`e7f002a`); the only red during writing was the test's
own base archetype, a worker with the defensive acquire level. So WP13b has no rule commit, and nothing in the
simulation or its hash changed.

The same commit restates the reference-value tests, test-only and green before and after: the weapon a unit carries
only in a mode is checked as its `weapon` block (`catalog_reference_values_tests`, `weapon_catalog_rules_tests`), so
the Lurker's sourced rows (damage, Armored bonus, range 8, mount, idle, random delay) keep reading it; Siege Mode's
replacing weapon is checked as before. The matrix harness gains `Side::mode`: on the first tick, after the standing
order, each unit casts the ability that switches it into the named mode (two unit tests).

### 15.3 Results

| Step | Commit | Matrix (against the previous step) | Battle 33 (68 seeds) | Goldens |
|---|---|---|---|---|
| Tests, harness | `e7f002a` | (no simulation change; no battle sets a mode) | — | unchanged |
| Data | `7e5e524` | battle 21 moved, winner unchanged (below) | bit-identical (41 / 10 / 17) | unchanged (no Lurker in them) |

**Data** (`7e5e524`): `burrow_scorpion` `weapon: None`; mode `burrowed` (cast 45 ticks, leave 11, each plus up to
4.4643; immobile, unpushable, cloaked) holding the unchanged spines; ability `Burrow` (`ToggleMode`,
`SwitchMode("burrowed")`). Test `b42_the_lurker_analogue_attacks_only_burrowed`, red before.

**Battle 21** (one Lurker against six Marines, both attack-moving) now burrows its Lurker on the first tick
(`mode: Some("burrowed")`). Without that line the marines win all 20 seeds, keeping all 270 life: the Lurker never
attacks. With it, blue wins all 20 as before:

| | Before | After |
|---|---|---|
| Lurker lead hull (20 seeds) | 72.92 (42.2 to 96.1) | 119.72 (106.8 to 131.8) |
| End tick | 94.95 | 130.70 |

On the seed of record the Marines fire 17 shots, about 85 damage, all while it burrows (it is in contact from tick
49); then it is cloaked and they have no detection, and three spine volleys kill all six. Battle 3 shows as moved
only in its expectation column, which WP13a re-recorded after its data matrix. Expectation failures: 0 on the seeds
of record, 0 over all seeds. The sim_core and sim_fixtures suites pass in debug.

### 15.4 What WP13b leaves open

- **B-42, narrowed** ([Q0069](../questions/closed/0069-lurker-burrow-sections.md)): whether the weapon and the cloak
  arrive at the `Stats` section's end, 12 ticks before the morph ends.
- **B-93 (new):** a burrowed unit still blocks ground units and is pushed by nothing; the game's burrowed Lurker
  collides only with `Burrow` (`liberty` UnitData:775). A mode has no collision layer (WP16, with B-49).
- **The skirmish client cannot burrow** ([Q0070](../questions/closed/0070-lurker-without-burrow-button.md)): it refuses
  `CastAbility` (no ability UI in Phase 5), so a skirmish Lurker can move but not attack. Default: keep the fix; a
  client package adds the command.
- The `Interruptible` flag of the burrow (a unit can cancel it) and Adaptive Talons (B-68) are not modelled; Seismic
  Spines (+2 range by slug) reaches the burrowed weapon, since upgrades find a weapon by its unit, not its slot.
## 16. WP13c: Charge (B-43)

Numbered after WP13b's §15, which lands on another branch. Sources: SC2GameData `5444e12f13`, chain
`core → liberty → swarm → void → voidmulti` (and `balancemulti`, read only to confirm it agrees), each line re-read;
Liquipedia [Charge](https://liquipedia.net/starcraft2/Charge) (wikitext, 2026-11-09); battle 33 investigation §1.6
and §1.11. Matrix: 20 seeds a battle (1000 runs); battle 33: 68 seeds. Directories `~/scratch/sc2dev_wp13c/`;
baseline `a227e7d` (`base`). The gate lock's queue was long (another worktree's package gate held it), so every
build, test and measurement ran unlocked at `-j 4` under `nice`, with the user slice's `anon` memory at about 5 GiB when checked (limit 8).

### 16.1 Research

- **The ability.** `Charge` is a `CAbilAugment` of the `attack` command (`liberty` AbilData:3825-3834): `AutoCast`
  and `AutoCastOn` (on by default), cost a 10 Normal s cooldown on the unit, the autocast validator
  `CasterNotHoldingPosition` (ValidatorData:166-170: no Hold Position order in the queue), and the requirement
  `UseCharge` (the Charge research). The augment's default effect is its own id (`core` AbilData:145) and its default
  autocast filters `Visible;Player,Ally` (:146): visible enemies only. `swarm` AbilData:473-475 changes only its
  alignment.
- **The trigger.** Effect `Charge` (`liberty` EffectData:2062-2068) applies `Charging` to the caster if
  `ChargeMinTriggerDistance` (ValidatorData:183-190, order target range `GT 0.6`) and `ChargeMaxDistance`
  (:1697-1704, source to target location `LT 4`, `Pathing 1`) pass.
- **The dash.** `Charging` (`liberty` BehaviorData:967-975): `Duration` 3.5, `MoveSpeedMultiplier` 2.2, flag
  `Channeling`, removed when `ChargeTargetAttacked` fails (ValidatorData:3007-3011: `PsiBlades` not firing), that is
  when the Zealot's weapon fires. `voidmulti` BehaviorData:94-97 adds only the impact-damage chain, whose damage
  effects need `HaveSunderingImpact` (`voidmulti` EffectData:1254-1259, ValidatorData:94-98), an upgrade no player
  researches at 5.0.14 (`TechTreeCheat`; patch 4.11.0 removed the impact damage, investigation §1.6). No later layer
  changes the duration, multiplier, cooldown or window.
- **The research.** `voidmulti` UpgradeData:68-71 (Zealot speed +1.125 Normal = +0.063 a tick, the shipped
  `move_speed_delta`).
- **In ticks** (25 a Faster second, 1.4 Normal s a Faster second, halves up as the audit rounds): 3.5 Normal s =
  62.5 → **63 ticks**; 10 Normal s = 178.6 → **179 ticks**. A dashing Zealot with Charge: 0.189 × 2.2 = 0.4158 a
  tick (10.40 Faster; Liquipedia's 9.1 is the pre-4.11 base speed, investigation §1.11).
- **Liquipedia** agrees on the window (range 4), the autocast and the end on attack, and adds that "switching to a
  more distant target while Charge is active" lets the Zealot cover more ground, so the dash survives a retarget.

Not stated by the data, and read as follows (owner question
[Q0071](../questions/closed/0071-charge-trigger-details.md), default this reading): the 0.6 is measured edge to edge
(as weapon and scan ranges are); the 4 is a straight centre-to-centre distance, not a walk; the autocast looks on every
tick; the dash ends at the start of a swing, and a new order does not end it.

### 16.2 The rule

`ArchetypeCapabilities::attack_dash: Option<AttackDashSpec>` (`speed_multiplier`, `duration_ticks`,
`cooldown_ticks`, `min_gap`, `max_distance`), validated (multiplier and distance positive, gap nonnegative,
duration at least 1). A new phase, `world/attack_dash.rs`, runs after target acquisition and before movement: a
living, completed unit in the world, not channelling and not holding position, whose front order is an attack on a
visible, attackable enemy (explicit or acquired, so an attack-move dashes too), with its dash off cooldown and
unlocked for its owner, starts the dash when the gap exceeds `min_gap` and the centre distance is below
`max_distance`. The dash is the speed buff 3 (`ATTACK_DASH_BUFF_ID`), so movement reads it as it reads Stimpack;
upkeep runs it and the new `Entity::attack_dash_cooldown` down; `start_swing` ends it. Research gates it through
`UnlockAbility` with the slug `attack_dash`, as `overclock` gates Stimpack. The cooldown is hashed: checksum
schema 21.

Tests (`attack_dash_tests.rs`): with the phase disabled 5 of 8 rule tests are red (the dash and its speed until the
swing; the maximum distance; the duration and cooldown; attack-move; the unlock); the other three assert that no
dash starts (no capability; within the minimum gap; holding position) and pass either way. Unit tests: catalog
validation, and an `UnlockAbility` naming `attack_dash` on an archetype without the capability is a load error. The
shipped-values test was red before the data commit (`None`).

### 16.3 Results

| Step | Commit | Matrix (against the previous step) | Battle 33 (68 seeds) | Goldens |
|---|---|---|---|---|
| Rule | `b36ec13` | bit-identical | bit-identical (41 / 10 / 17) | re-recorded for schema 21 alone (§16.4) |
| Data | `1ac40ac` | bit-identical | bit-identical (41 / 10 / 17) | unchanged |

**No matrix battle reaches Charge.** The plan's §3 row and WP13 list battle 23b, but 23b researches only
`ground_weapons_1`, and no battle researches `charge`; the data commit therefore moves nothing the matrix measures.
No commit was held, and no expectation changed.

**What the dash does (scratch probe, not committed).** The matrix fixture with the Zealot side researching `charge`
in battles 23b and 33 (`~/scratch/sc2dev_wp13c/probe/`), 68 seeds each, once on the shipped catalog and once on
`a227e7d`'s (the speed bonus without the dash):

| Battle | Charge speed only | Charge with the dash |
|---|---|---|
| 23b (one Zealot against six Zerglings) | Zealot wins 68; life left 58.0; end tick 253.2 | wins 68; 57.3; 251.0 |
| 33 (24 Marines against 12 Zealots) | Zealots win 67, Marines 1; Zealot survivors 3.62; end tick 408.5 | Zealots win 68; survivors 5.78; end tick 341.6 |

The Zerglings run at the Zealot, so the dash saves only two ticks; against standing Marines it closes the gap at
10.40 a Faster second and two more Zealots survive.

### 16.4 Goldens

The rule commit adds `Entity::attack_dash_cooldown` to the hash (schema 21). With the field unhashed at schema 20
both golden replays (debug and release) and the sandbox checksum (debug) reproduce as recorded, so the
re-recording is the encoding alone; the re-recorded lists reproduce in debug, release, and `wasm32-wasip1` debug and release
(`wasmtime`). The data commit leaves both as recorded: neither researches Charge.

### 16.5 What WP13c leaves open

- **B-43, narrowed** ([Q0071](../questions/closed/0071-charge-trigger-details.md)): how the engine measures the 0.6
  and the 4 (edges or centres; a straight line or the walk), when the autocast looks, and whether a new order ends a
  dash.
- **B-94 (new):** Charge's autocast cannot be switched off; the game's is a player toggle (`AutoCast`, on by
  default). Ours always charges (WP18, with the abilities plan's autocast commands).
- The dash has no visual of its own in the client, and the render snapshot's speed-boost flag still reads only
  Stimpack.

## 17. Owner answers of 2026-11-09 (plan Amendment 22)

Measurements under `~/scratch/answers/<step>`, base `main` at `bc3af54` (`base`: matrix 0 expectation failures,
battle 33 41 / 10 / 17 on 68 seeds). Each step compares against the one before; tallies are blue / red / unresolved
wins over a battle's 20 seeds. Goldens are checked in debug, release and `wasm32-wasip1` release (`wasmtime`).

### 17.1 Package A: channeled bursts (Q0058, B-2)

- **Data.** The 5.0.14 burst effects that set `Flags` `Channeled` 1, read through the chain: Zealot `PsiBladesBurst`
  (`liberty` EffectData:1924), Thor `ThorsHammer` (:2247), Reaper `P38ScytheGuassPistolBurst` (:1808), Missile Turret
  `LongboltMissile` (:1784), Queen `TalonsMissileBurst` (`swarm` EffectData:3043) and `TalonsBurst` (`liberty`
  :2325), Liberator `LiberatorMissileBurstPersistent` (`void` EffectData:2945), Colossus `ThermalLancesForward` and
  `ThermalLancesReverse` (`liberty` :2371, :2396). Not set: the Viking's `LanzerTorpedoes` (`liberty` :1698), the
  Banshee's `BacklashRockets` (`liberty` :2550, `voidmulti` :36), the Phoenix's `IonCannons` (`liberty` :1959). No
  `CEffectCreatePersistent` default sets it (`core` EffectData:34-40). The owner's answer names the Zealot and the
  Thor and says to follow the flag; the flag also covers the Reaper, Queen, Liberator and Missile Turret.
- **Rule** `a9a24bf` (latent, checksum schema 21 → 22 on the branch, 22 → 23 after the merge with pathfinding Part A): `WeaponSpec::channeled` (catalog default `false`) is copied
  onto the pending burst; a non-queued order that replaces the attacker's orders drops a channeled burst, except an
  attack on the burst's own target. Red: `q0058_an_explicit_order_ends_a_channeled_burst` (Move between the strikes:
  damage 20, expected 10). Matrix and battle 33 bit-identical; goldens and the sandbox re-recorded for the schema.
- **Data** `0c0cb1e`: red `q0058_channeled_bursts_follow_starcraft` (`psi_grey`: false). Matrix and battle 33
  bit-identical (no matrix battle orders a unit between its strikes). `golden_replay` changed from checkpoint 5, the
  first in which its zealot swings; with the drop disabled the checksums are the same, so the encoded flag alone
  moved them. `golden_replay_orders` unchanged.

### 17.2 Package B: splash distance and lines (Q0063, B-17, B-18)

- **Rule** `2f65bc9` (checksum schema 22 → 23 on the branch, 23 → 24 after the merge): a zone reaches a unit whose edge is within it (centre distance less
  the unit's collision radius, never below 0); a splash with `offset_by_unit_radius` is centred on the struck
  target's edge facing the attacker. Red: `q0063_a_zone_reaches_a_units_edge` (0, expected 40) and
  `q0063_offset_by_unit_radius_centres_the_area_on_the_near_edge` ([0, 0], expected [40, 0]). Measured
  (`b_rule`): 8 battles moved, every expectation met; battle 32 4 / 8 / 8 → 0 / 18 / 2 (a Baneling's 2.2 now reaches
  a marine 2.575 away between centres; its claim "Banelings trade far above their cost" holds more clearly); 10,
  12a, 15, 21, 26, 27 end sooner. Battle 33 41 / 10 / 17. Goldens re-recorded for the schema only (no unit of either
  replay splashes). The encoding unit tests' schema byte, left at 21 by `a9a24bf`, is set here.
- **Offset data** `089d6f5`: the flag is inherited from `DU_WEAP` (`core` EffectData:128) by the Liberator's
  `LiberatorMissileDamage` (`void` :2932), the Baneling's `VolatileBurstU` (`liberty` :1753) and the Ultralisk's
  `KaiserBladesDamage` (`liberty` :2296); the Siege Tank's `CrucioShockCannonBlast` clears it (`liberty` :1680); the
  line searches are `CEffectEnumArea` effects, whose default sets no flag (`core` :71-77). Red
  `q0063_splash_offset_follows_starcraft`. Measured (`b_offset`): 15, 27 and 32 moved (32: 0 / 18 / 2 → 2 / 11 / 7),
  no winner changed; battle 33 unchanged; goldens unchanged.
- **Lines** (WP8b's held data, §9.4, redone from the recorded values):
  - Colossus `1b16eac`: landed. Measured with the Lurker's line in the same run (`b_lc`; each matrix battle is its
    own world and only battle 21 spawns a Lurker): 12a, 12b, 26 moved, no winner changed; battle 26's expectation
    re-recorded red (5, 999) → (4, 999) (the colossi win every seed, the fight lasts 313 ticks instead of 89, two
    seeds lose two colossi).
  - Hellion: **held** (landed later by the orchestrator's decision, §17.8) on branch `answers-held-b18-hellion-line` (`06eb273`; `b_hellion`): 9a and 10 flip from
    20 / 0 / 0 to 0 / 20 / 0, as §9.4 found under the edge rule.
  - Lurker: **held** (landed later, §17.8) on branch `answers-held-b18-lurker-line`: battle 21 20 / 0 / 0 → 4 / 0 / 16 (16 seeds reach the
    250-tick limit). §9.4's probe kept the winner (16 / 4 / 0); it predates WP10 to WP13b, which changed the
    Lurker (its turret, and attacking only burrowed). Measured again after package E in §17.5.
  - One search per strike (§9.3): not redone; §9.4's edge probe already flipped battle 12b with it.

### 17.3 Package C: turrets with no idle rate (Q0065, B-30)

- **Data** `9c2e8e4`: the three `ResetIfMoving` turrets with no `YawIdleRate` in any layer (`core` TurretData:3-10
  sets none) return at their `YawRate`: the Siege Tank's tank-mode `SiegeTank` (`liberty` TurretData:77-79, 720 from
  `FreeRotate`) 40.32° per tick, the `Hellion` (:95-98, 999.8437) 55.99°, the `Phoenix` (`swarm` TurretData:31-39,
  539.2968) 30.20°. The question listed the same three rates in another order; each turret takes its own.
- Red `b30_turret_idle_follows_starcraft` (`railgun_crawler` weapon: idle `Hold`). Matrix and battle 33
  bit-identical (`c_data`). Both goldens re-recorded: their tank and Phoenix turn their turrets back to the front
  while moving without a target, and the turret angle is hashed.

### 17.4 Package D: unset deceleration (Q0067, B-39)

- **Data** `4c1e83a`: 40 unit archetypes take their converted `Acceleration` as `deceleration`; the High Templar
  (`liberty` UnitData:3259) and the Overlord (`voidmulti` UnitData:1199) keep the values their data set. Ground units
  (1000, 3.136 per tick²) still stop within a tick. Red `b39_deceleration_follows_starcraft` (`welder_drone`:
  0.1575, expected 0.00784).
- Measured (`d_data`): only battle 35 moved (end tick 570.8 → 570.9), no winner or expectation changed; battle 33
  41 / 10 / 17. `golden_replay_orders` re-recorded from checkpoint 7: its Phoenix analogue brakes into its move
  destination; `golden_replay` unchanged.
- **Not done:** the rule half of option 2, braking for orders that end without arriving (Stop, Hold Position, a
  replaced move) and for pursuit. Today a unit brakes only into a move order's destination; B-39 stays open for it.

### 17.5 Package E: the Lurker's burrow sections (Q0069, B-42)

- **Data** `e59a23f`: `burrow_scorpion`'s entering channel is the `Stats` section's end, 1.8332 Normal s (`swarm`
  AbilData:1654-1674, `void` :149-155) = 32.74 → 33 ticks, plus the morph's random extra (up to 4.4643) as before;
  it was the `Actor` section's 45. `UnitModeSpec::cast_ticks` now names the `Stats` section; Siege Mode's sections
  end together, so its times stand. No rule was needed: the channel's end already arms the mode's weapon and sets
  its cloak. The `Actor` section's 45 ticks are the burrow animation, which the client does not present yet.
- Red `b42_the_lurker_analogue_attacks_only_burrowed` (`cast_ticks` 45, expected 33). Measured (`e_data`): only
  battle 21 moved (20 / 0 / 0 as before, end tick 125.2 → 112.6); battle 33 41 / 10 / 17; goldens unchanged.
- **The Lurker's line, measured again on top of this commit** (`b_lurker`): battle 21 still flips, 20 / 0 / 0 →
  2 / 0 / 18 (18 seeds reach the 250-tick limit, 18 expectation failures). The line stays held on branch
  `answers-held-b18-lurker-line`.

### 17.6 Package F: when the Colossus' and the Lurker's damage lands (Q0064, B-81, B-85)

- **Colossus:** no change, by the owner's answer: both strikes and the sweep land on the release tick. B-81's
  Colossus part and B-85's Colossus search delay are closed; B-81 keeps the Liberator's 1-tick first-missile delay.
- **Lurker spines:** not done in this round. The data space the 9 searches by `PeriodicPeriodArray` 0, then 0.125
  Normal s each (`void` EffectData:297-307, appended by `voidmulti` :797-813 with `PeriodCount` 9), so spine k lands
  k × 0.125 s after the first; the owner allows 2 ticks apart. The searches stop when the Lurker dies or unburrows
  (`PeriodicValidator` `CasterIsAliveandBurrowedLurker`, `voidmulti` :813). A spaced line needs per-unit pending
  state (the next search, its origin and direction, and the units already struck for `noMarkers`), a hashed field
  and a schema bump; B-85 stays open for it.

### 17.7 Package G: answers that change nothing

Q0062 (B-15's life minimum: option 1, the landed WP7 rule), Q0066 (B-91: option 1, the unscaled delay kept) and Q0068
(B-41's order within a hit: option 1, WP13a as landed) are closed with resolutions. Q0070 (option 1) keeps the
simulation as it is; its resolution is drafted and the question stays open until plan
`2026-11-09-client-mode-commands` delivers the skirmish client's mode buttons.

### 17.8 Addendum: why the Hellion's and the Lurker's lines flip their battles

Traces with the held commits' `matchup_trace` builds (`~/scratch/answers/diag_*`, per-unit CSVs; seed 11 unless
named). Positions are world units, the Hellion or Lurker at about (9.9, 32.0).

**Hellion (battles 9a and 10): the SC2-correct consequence, not a defect.**
- 9a, seed 11. The Hellion fires three times before it dies (ticks 5, 50, about 95; its period is 2.5 Normal s, 45
  ticks). Shot 1 at the leading zergling (15.15, 31.68) strikes it and the one behind it on the line (15.78, 31.61);
  the two beside it, 0.7 off the line, are outside 0.15 of every search even measured to their edge (0.7 − 0.375 =
  0.33). Shot 2, with the zerglings around the Hellion, strikes only its target: the others stand 0.6 to 0.8 to the
  side of a line that starts at the Hellion. With the stand-in circle (2.2 around the target) every shot struck all
  four. In battle 10 (seed 11) the two Hellions' volleys strike 4, then 2 and 2 zerglings, where the circle struck all
  eight on the first volley.
- The geometry is the data's (§9.1: 25 searches of 0.15 along the attack from `SourcePoint`, `liberty`
  EffectData:2609-2646), the reach to a unit's edge is Q0063's answer, and the numbers are the game's: 14 per flame
  against a 35-life zergling (`InfernalFlameThrower`: 8, +6 Light, `liberty` EffectData:2600-2607), so three flames kill only the zerglings struck by
  all three, while four zerglings (5 per 0.696 Normal s each) kill a 90-life Hellion in about 3 Normal s. A Hellion
  that stands still is not expected to clear four zerglings in the game; the battles' claims rest on the circle.
- **Proposal:** land the line and re-record 9a and 10 with claims the game supports (for example a Hellion that
  kites, or zerglings that arrive in a column along the flame); no code change.

**Lurker (battle 21): the line is right; the battle's 250 ticks are too short for it, and the marines flee.**
- The line is anchored on the Lurker and points at the target: on the first volley (tick 35) the spines aimed at the
  marine at (16.63, 32.52) strike it and the two within reach of the line (16.02, 32.03) and (15.91, 32.77), not the
  three below it, 1.0 to 1.2 off the line. Volleys strike 3, 4, 1, 2, 1, 1 marines (ticks 35, 72, 108, 143, 178, 250),
  where the circle struck all six on the first two and won by tick 113.
- The marines cannot target the burrowed, cloaked Lurker: after tick 35 its life stays at about 136 to the end. They
  walk onto its position and stand around it, so a line from the Lurker meets one or two of them; struck, they move
  away (the damage effects' `ResponseFlags` `Flee`, `core` EffectData:124-125). With the limit raised to 500 ticks
  (`diag_21_500`), the Lurker kills all six on 8 of 20 seeds (ticks 215 to 363); on the others one marine survives
  at 5 life, out of range: seed 3203386110's last marine stands at (21.3, 28.3), 11.9 from the Lurker.
- **Not a defect of the line search.** What differs from the game is the marines standing beside the Lurker rather
  than over it (B-93: a burrowed Lurker should not block ground units) and the battle's time limit. **Proposal:** land
  the line with battle 21 re-recorded to what the game supports (the Lurker unharmed after burrowing, at most one
  marine left: `blue (1, 1), red (0, 1)`, with 500 ticks), and fix B-93 in WP16; no change to the line rule.

**Decision and result.** The orchestrator accepted both lines (plan Amendment 22). Measured against the merged tree
(`~/scratch/answers/merged`): the Hellion's line (`585c2ab`, `hellion2`) moved only 9a and 10 (20 / 0 / 0 → 0 / 20 / 0
each), re-recorded red (2, 4) and red (4, 7); the Lurker's line (`678a0db`, `lurker3`) moved only battle 21, at 500
ticks 8 / 0 / 12 with the Lurker alive on every seed (marines left: 0 on 8 seeds, 1 on 11, 2 on seed 8), re-recorded
`blue (1, 1), red (0, 2)`. Battle 33 41 / 10 / 17 on 68 seeds; 0 expectation failures; goldens and the sandbox
unchanged in debug, release and `wasm32-wasip1`.


## 18. Owner answers Q0071 to Q0074 and the open rule halves (plan Amendment 24)

Method as §17: every new test was run red on its commit's parent tree with only the new tests added
(and, for B-85, the latent `period_ticks` field without its mechanism), then green on the head. Each
commit's tree was exported with `git archive` and measured by release builds of `matchup_trace`
(whole matrix, the seeds of record plus 16, `--compare` against the step before) and battle 33 (68
seeds), and by `route_trace` on the showcase battle (1,800 ticks). Outputs under
`~/scratch/followups/` (`out/<step>/`, `logs/`; not committed). Steps: base (main `3ae6409`), a
(`93dffe7`), b (`cd02e88`), c (`adc4b08`), d (`1d58e67`), e (`993ee14`, the glide fix before its
animation amendment), final (`c207048`).

| Step | Matrix against the step before | Battle 33 (68 seeds) | Showcase |
|---|---|---|---|
| a, Charge | bit-identical to main | bit-identical: blue 47, red 16, unresolved 5 | identical to main |
| b, braking | moved 22a, 34, 35; no winner changed; expectations met | bit-identical | diverged: no winner by tick 1,800 (see 18.2) |
| c, spaced line (latent) | bit-identical | bit-identical | as b |
| d, the Lurker's spines | moved 21 only; winner unchanged | bit-identical | as b |
| e, glide fix | bit-identical | bit-identical | identical to main |
| final | bit-identical | bit-identical | identical to main |

Expectation failures: 0 on the seeds of record and 3 over all seeds at every step, as on main.

### 18.1 Charge (Q0071, B-43), `93dffe7`

- Red on `e04f46e`: `q0071_an_explicit_non_attack_order_ends_a_running_dash` ("ends the dash at
  once" fails for `Move`) and `q0071_the_maximum_is_the_walk_not_the_straight_line` (the dasher
  dashes on its first tick, through the wall's straight line). `q0071_an_attack_retarget_or_attack_move_keeps_a_running_dash`
  passes on both trees: it guards the half of the answer the rule already met. Two expectations of
  the new tests were wrong and are corrected in `0a9af39` (the cooldown counts from the dash's
  start; the wall test reads the position on the dash's first tick), with no rule change.
- No matrix battle researches Charge (Amendment 21), so the matrix and battle 33 do not move.

### 18.2 Braking (Q0067, B-39), `cd02e88` and `af70813`

- Red on `93dffe7`: the glide after Stop and Hold Position (speeds 0.09, 0.08, … 0.01, 0.45 past the
  order's point) and the pursuit braking into its band. The instant-deceleration twins pass on both
  trees, as they should.
- Matrix: battles 22a (Reaper against workers: blocked attacker ticks rise, the Reaper's lead hull 10 to
  15 on some seeds), 34 (Hydralisks against Mutalisks: end ticks and survivors move by one or two)
  and 35 (Vikings against Colossi: end ticks move) moved, each with finite-acceleration units; every
  winner and expectation held.
- Showcase: on `cd02e88` the battle diverged on tick 3, where welder drone 16 (a worker, finite
  deceleration) stood beside pulse bot 17. Collision writes a body's net displacement into its
  velocity, so a push gave the standing worker a velocity, and the glide carried it on. The battle
  then took another shape: no winner by tick 1,800 (main: player 0 at 1,693), one stall give-up,
  player 1's reversals 7 to 29. Fix `af70813`: only a unit that walked on the tick before (animation
  `Walk`) glides; red on `1d58e67`, `q0067_a_pushed_unit_at_rest_does_not_glide`. The showcase is then
  identical to main (ramp W foot P0 1.301 / 1.473, 157° / 301°; ramp E foot P0 1.198 / 1.300,
  190° / 454°, P1 1.149 / 1.168, 81° / 152°; reversals 17 / 7; give-ups 0 / 0; player 0 wins at
  tick 1,693), and the matrix is bit-identical to step d. The amendment of `af70813` keeps the
  animation of a unit that rests to its caller, as before (`checked_movement_tests` pins it); the
  final tree measures as step e.
- Not modelled: a unit whose target comes into range while it moves fast stops within a tick (the
  braking aims at the band's edge, which a closing target can move), and an order given up by the
  stall check ends at rest (the unit had stalled).

### 18.3 The Lurker's spines (Q0064 note, B-85), `adc4b08` and `1d58e67`

- Red on `cd02e88` with the latent field: `b85_a_spaced_line_lands_its_searches_period_ticks_apart`
  and `b85_a_spaced_line_ends_when_its_attacker_dies` (every search on the release tick). Red on
  `adc4b08`: `b18_line_splash_follows_starcraft` (the Lurker row's period 2).
- The rule is latent at `adc4b08` (matrix bit-identical). With the data, battle 21 moved alone: the
  Lurker survives every seed as before; the marines left change on seeds 1 (0 to 1, the run now ends
  at 500), 7 and 13 (2 to 1); the expectation `blue (1, 1), red (0, 2)` holds.
- The target itself is still struck on the release tick, as before: in the game the Lurker's target
  takes its damage from a spine like any other unit, so a fast target could outrun it; recorded as a
  detail difference, not a new ID, since the target is where the line points.

### 18.4 Goldens and the sandbox

Both replays pass unchanged (schema 25) on `93dffe7` and on `cd02e88` with the glide fix: no rule of
this round changes their battles, and the Lurker analogue is in neither. Schema 26 (`adc4b08`)
changes every checkpoint from the first and the default sandbox checksum; re-recorded in `c207048`.
The re-recorded lists pass in debug and release and on `wasm32-wasip1` (wasmtime); the sandbox
checksum passes in debug, release and on `wasm32-wasip1`.

### 18.5 B-113: variant D, rejected

Variant D: a wedged body (its disc does not fit where it stands) keeps a route corner's points until
it is past them (the half-plane test of `drop_passed_waypoints`), instead of passing them by terrain
view when near. Showcase on the head with D: ramp W foot P0 1.343 / 1.473, 227° / 527°; player 1
now reaches ramp W (1.656 / 1.869, 250° / 712°); give-ups 1 / 0; player 1 wins at tick 1,605, a
winner flip. Rejected. With variants A to C (plan `2026-11-09-sc2-pathfinding` evidence, Amendment 5)
four route-following variants have failed; a fix likely needs the body to slide along the wall it is
wedged against (a contact rule), not a route rule. B-113 stays open.

### 18.6 B-113: contact variants E to H, rejected (2026-11-11)

On the spellcaster head `fc4c811` (schema 29), four contact rules were tried in the collision phase
instead of a route rule, with a new exact mesh query (the nearest point of the blocking edges a disc
overlaps, found by the segment predicate's flood, and whether it is a convex corner the centre lies
beyond). Each was measured on the whole showcase (`route_trace --ticks 1800`, release; outputs under
`~/scratch/bugfix-113/`, not committed). Base: ramp W foot P0 1.301 / 1.473, 157° / 301°; ramp E foot
P0 1.198 / 1.300, 190° / 454° (6 units), P1 1.149 / 1.168, 81° / 152°; reversals 17 / 7; give-ups
0 / 0; player 0 wins at tick 1,693.

| Variant | Ramp W P0 | Ramp E P0 | Ramp E P1 | Reversals, give-ups | Outcome |
|---|---|---|---|---|---|
| E: a pushed ground disc is moved off every wall, to its radius from the nearest wall point (two passes; kept only where the disc fits) | 1.334 / 1.473, 173° / 296° | not reached by tick 1,800; player 1 reaches ramp W (2 units) | 1.152 / 1.167, 69° / 89° | 14 / 8, 2 / 0 | not over at tick 5,000 |
| F: as E for a convex corner only (radially off the vertex) | 1.348 / 1.473, 224° / 415° | 1.161 / 1.184, 65° / 71° (4 units) | 1.150 / 1.172, 83° / 171° | 10 / 5, 0 / 0 | player 0 at tick 2,157 |
| G: every ground body, pushed or not, off every wall each tick | 1.334 / 1.473, 173° / 296° | not reached by tick 1,800 | 1.153 / 1.167, 73° / 88° | 10 / 9, 0 / 0 | not over at tick 5,000 |
| H: as F, but a push only stops a body going deeper onto the corner (back to the smaller of its radius and its distance before the push) | 1.304 / 1.473, 163° / 322° | 1.176 / 1.231, 186° / 518° (5 units) | 1.149 / 1.168, 81° / 152° | 18 / 7, 0 / 0 | not over at tick 1,800 |

F fixes the corner of B-113 (pulse bot 7 no longer wedges at (35, 52); ramp E's maximum turn falls
from 454° to 71°, below the grid's 287°) but makes ramp W worse for six of its ten units. Its first
divergence is the cause: on tick 167 pulse bot 7 leaves ramp W along the ramp's north wall (y = 12),
its disc 0.355 into it, as the centre-only physics allows along a straight wall; when its centre
passes the wall's end (29, 12) by 0.003, the corner rule moves it 0.35 south, against its walk, and
the column behind it follows. A corner rule is discontinuous wherever a straight wall's overlap is
allowed and the corner's is not; E and G remove that by applying the rule to every wall, and change
the battle's shape (player 0 no longer reaches ramp E, the match does not end within 5,000 ticks).
H removes the jump but leaves the wedge (the crowd still carries the body inside its radius over
several ticks). None meets the stop rule (no box worse, no winner change), so none is kept and the
matrix and battle 33 were not run for them; B-113 stays open. A fix that keeps the showcase needs disc
contact everywhere with the battle re-tuned around it, or a route rule; both are larger than a
single-cause fix.
