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
  Liquipedia: filed as [Q0058](../questions/open/0058-channeled-burst-interruption.md), default no change (only the
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
  each 40-life worker with its fifth pistol shot, a first shot (9 a shot against Light), and in `22b` it dies to the
  Stalker either way; neither battle records the Stalker's shields.
