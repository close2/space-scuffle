# Turret wind-up: measurements per package

This file records the measurements of plan [`2026-11-23-turret-wind-up`](../plans/2026-11-23-turret-wind-up.md)
(§7; B-90, B-45): the survey's material, the world comparison of every package against the WP0 tree, the goldens
and their proof, the censuses §7 marks **verify**, and the red runs of the new tests. WP0 creates it; later packages
append.

## Survey material (§2.3)

Every modelled weapon was resolved from SC2GameData `5444e12f13` (5.0.14) through the override chain `core →
liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti`, with XML comments stripped, by
`~/scratch/turret-wind-up/weapons.py` (run with `python3 -I weapons.py <weapon id>…`, reading
`~/scratch/combat_wp3_data/<layer>/` as data). Its output for the surveyed weapons is
`~/scratch/turret-wind-up/weapons-out.txt` (and `pf.txt` for the Planetary Fortress' `TwinIbiksCannon`). It
reproduces §2.3's table:

| 5.0.14 weapon | `AllowedMovement` | `OnlyFireWhileInAttackOrder` |
|---|---|---|
| `IonCannons` (Phoenix) | Moving (`swarm`:181) | 0 (`swarm`:177) |
| `VoidRaySwarm` (Void Ray) | Moving (`void`:176) | 1 (`core`:20; the clearing options are commented out) |
| `PhaseDisruptors` (Immortal) | Slowing (`voidmulti`:207) | 0 (`balancemulti`:185) |
| `ThermalLances` (Colossus) | unset | 0 (`balancemulti`:229) |
| `VolatileBurst` (Baneling) | Moving (`liberty`:470) | 1 |
| `FusionCutter`, `ParticleBeam`, `Spines` (workers) | Slowing (`voidmulti`:104, :196, :219) | 1 |
| `LanzerTorpedoes`, `LiberatorMissileLaunchers`, `BacklashRockets` | Slowing (`liberty`:274, `void`:329, `liberty`:557) | 1 |
| `GlaiveWurm`, `ParasiteSpore`, `BroodlingStrike` | Slowing (`liberty`:406, :569, :418) | 1 |
| `90mmCannons`, `CrucioShockCannon`, `InfernalFlameThrower` | unset | 1 |

The same run confirms the registered rows' citations: `OnlyFireAtAttackOrderTarget 0` on `IonCannons`
(`swarm`:178), `PhaseDisruptors` (`balancemulti`:186), `ThermalLances` (`balancemulti`:230) and `TwinIbiksCannon`
(`liberty`:140) (B-180); `LockTurretWhileFiring 1` on `InfernalFlameThrower` (`liberty`:590) (B-179);
`NoDeceleration 1` on `FusionCutter` (`voidmulti`:103), `ParticleBeam` (:195), `Spines` (:218), `VolatileBurst`
(`liberty`:473) and `GlaiveWurm` (`liberty`:400) (B-181).

## How to reproduce

Binaries are built once per tree and run from a `git archive` copy of that tree's `assets/`, so each run reads its
own catalog (`matchup_trace` embeds the catalog it was compiled with; `route_trace` is given `--assets`
explicitly). The scripts are the strike-delays plan's, copied to `~/scratch/turret-wind-up/scripts/`
(`measure.sh`, `compare.sh`, `firstdiff.py`, `proof.sh`, `client.sh`, the last two pointed at this worktree).

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/turret-wind-up-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/turret-wind-up
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
and the showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks.

## WP0: baseline

The tree of the plan (`ebf45835`, behaviour-identical to `main` at `d71f7153`), measured into
`~/scratch/turret-wind-up/world-wp0`: 483 files; the matrix reports "1180 runs: expectation failures 0 on the seeds
of record, 4 over all seeds".

**Censuses (§7 verify).**

- *Showcase units.* Over the nine `showcase.ron` runs (default seed and seeds 1 to 8, 6,000 ticks) the unit samples
  are of three types only: `pulse_bot`, `welder_drone` and `rhino_beetle`. None has a turret, and none has
  `fires_outside_attack_order` or `Moving` after WP1 (`welder_drone` becomes `Slowing`, which no rule of this plan
  reads apart from `Stopped`).
- *Showcase effects orders.* `showcase_effects.ron` gives only initial orders (`EnterModes` for player 0,
  `AttackMoveToCell` for player 1); the scenario format has no later orders. Over its nine runs the unit samples are
  `napalm_wheeler`, `pulse_bot`, `railgun_crawler` and `rhino_beetle`: the turreted Hellions and tanks fight under
  attack-move, sieged, or idle where they stand.
- *What "blue_spotter" passes.* In `golden_replay_orders.ron` the one Phoenix analogue (`flying_saucer`, an air
  weapon only) moves from (2, 18) to (12, 32) from tick 10. Every other squad is ground (`pulse_bot`,
  `rhino_beetle`, `psi_grey`), so no enemy its weapon can attack is in reach of its pass. `golden_replay.ron` has no
  air unit; its turreted squad, "red_siege" (`railgun_crawler`), takes `AttackMove` at tick 120 and `Stop` at tick
  760.

## WP1: the fields and their data, latent

### Red runs

On the WP0 tree the new tests did not compile (`~/scratch/turret-wind-up/red/wp1-compile.log`): `E0432`/`E0433`/
`E0425` for the missing `sim_types::AllowedMovement` (in `weapon_catalog_rules_tests`, `checksum_state_tests` and
the validation unit tests) and `E0609 no field allowed_movement` / `fires_outside_attack_order` on `WeaponSpec` and
`WeaponSpecDef`.

With a field-only stub (the enum, both fields in `WeaponSpec` and `WeaponSpecDef`, their conversion, and the
encoding listing them `unhashed`; no data, no validation), `~/scratch/turret-wind-up/red/wp1-stub.log`:

- `weapon_catalog_rules_tests::b90_weapon_movement_follows_starcraft` failed on the first listed analogue: "welder_drone
  Drill Slash: AllowedMovement and OnlyFireWhileInAttackOrder as StarCraft II's weapon (FusionCutter AllowedMovement
  Slowing: voidmulti WeaponData:104)", left `(Stopped, false)`, right `(Slowing, false)`.
- `catalog::validation::weapon_movement_tests::a_suicide_weapon_may_not_fire_outside_an_attack_order` failed:
  "accepted". The two other unit tests passed on the stub, as they must: they pin what the stub already allows (the
  option and `Moving` are independent; omitted fields read the defaults).
- `checksum_state_tests::a_projectiles_movement_and_order_rules_change_the_world_checksum` failed: "checksum omitted
  Slowing" (both 11589841332170337729).

After the data, the validation and the encoding all of them pass.

### Golden proof

1. With both fields left out of the encoding (`unhashed`) at schema 38, and the D-7 data in place,
   `golden_replay_tests` (both fixtures, 4 tests) and `fixture_tests` (`DEFAULT_SANDBOX_CHECKSUM`
   5_609_708_170_486_988_781) passed on native debug, native release, `wasm32-wasip1` release and `wasm32-wasip1`
   debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<worktree>"`), logs in
   `~/scratch/turret-wind-up/proof-unhashed38/`.
2. With both fields hashed after `turret_idle` and schema 40, both fixtures were re-recorded under the plan's header
   and the sandbox checksum became 17_151_758_586_245_851_116. All four builds reproduce them
   (`~/scratch/turret-wind-up/proof-hashed40/`). Schema 40, not 39: `main` was still at 38 (`d71f7153`) when WP1
   was recorded, but the harvest-handoff branch, about to merge, holds 39.

### Worlds

`compare.sh world-wp0 world-wp1`: "files base=483 new=483 differing=0". Stop rule 1 holds.

## WP2: the order rule, tracking and the hull (B-90 first half, B-45 Phoenix)

### Red runs (on the WP1 tree)

`~/scratch/turret-wind-up/red/wp2.log`; every new test failed for the defect it pins:

- `turret_tracking_tests::a_turret_that_needs_an_attack_order_holds_fire_while_retreating`: "tick 1: no swing
  outside an attack order" (the tank fires at its stale target).
- `turret_tracking_tests::a_turret_that_fires_while_moving_tracks_and_fires_while_retreating`: "the turret turns
  independently of the hull" (the Phoenix analogue has no target under a move, so its turret never turns).
- `target_selection_tests::b90_a_weapon_fires_at_its_current_target_only_in_an_attack_order`: "turret true, Move: a
  swing only in an attack order", left true (the hull copy passed every order case already).
- `target_selection_tests::b90_a_weapon_outside_an_attack_order_fires_while_walking_only_when_moving`: "Stopped: a
  swing under an order that does not walk".
- `target_selection_tests::b45_a_unit_that_fires_while_moving_tracks_the_best_enemy_in_reach`: "the best enemy in
  reach, the order kept", current target `None`, expected the higher-priority enemy.
- `hull_turning_tests::a_hull_weapon_outside_an_attack_order_never_turns_the_hull`: "target at (15, 18): fires only
  at a target in its arc", left false.
- `runtime_catalog_tests::an_edited_movement_rule_makes_a_unit_hold_fire_while_moving_without_recompiling`: "the
  shipped unit fires while it moves past the enemy".

The `entity.rs` unit test `order_walks_tests::every_order_walks_as_the_movement_phase_classifies_it` was written with
the function and did not exist on the WP1 tree.

**Amendment 1.** After the rule, the Phoenix test first failed on "tick 5: the hull translates as it would with no
enemy": the movement phase holds the air-only Phoenix analogue for its wind-up (it reads the ground-first weapon).
The test was restated as Amendment 1 says; the hold is WP3's.

After the rule every new test passes; P-9's guards (`weapon_cadence_tests`, `stutter_step_tests`, the existing
`target_selection_tests` and `hull_turning_tests`, `multi_strike_timing_tests`, `attack_move_simulation_tests`,
`b2_…` and `b81_…`) pass unchanged, and so do all 108 test targets of `sim_types`, `sim_core` and `sim_fixtures`
(`~/scratch/turret-wind-up/wp2-tests.log`). `stutter_step_tests` has no case that relied on firing under `Move`
(§12 verify): it passed unchanged.

### Worlds and goldens

`compare.sh world-wp0 world-wp2`: "files base=483 new=483 differing=0"; the matrix reports the WP0 line again. Both
goldens and `DEFAULT_SANDBOX_CHECKSUM` reproduce unchanged (no re-record). As §7 predicted, no measured world moved:
the matrix and scenarios give only attack-move, hold or no orders, "red_siege" fires inside attack orders, and the
golden Phoenix passes no enemy its air weapon reaches. P-12's WP2 direction holds trivially. Stop rule 2 holds: there
is no first difference to trace.

## Merge of `main` (`e212ca44`, harvest hand-off, schema 39)

`main` took schema 39 while WP1 held 40, so 40 stays. The goldens and the sandbox checksum were taken from `main` and
§5's proof repeated on the merged tree, which carries WP1 and WP2:

1. With both fields unhashed at schema 39, both fixtures and `DEFAULT_SANDBOX_CHECKSUM` (8_582_058_566_732_369_859)
   reproduce `main`'s values on native debug and release and `wasm32-wasip1` release and debug
   (`~/scratch/turret-wind-up/proof-merged-unhashed39/`). This also shows that WP2 moves neither golden on `main`.
2. Hashed at 40, both fixtures were re-recorded (header "Re-recorded for plan 2026-11-23-turret-wind-up WP1
   (checksum schema 39 -> 40)") and the sandbox checksum became 5_438_070_395_987_993_470; all four builds reproduce
   them (`~/scratch/turret-wind-up/proof-merged-hashed40/`).

## WP3: the wind-up hold follows `AllowedMovement` (B-90 second half, B-45 Void Ray and Phoenix hold)

Branch `turret-wind-up-wp3`, from `main` at `e5f5c565` (checksum schema 46, the contact solver the default
pipeline). WP0's baseline predates the solver, so WP3 was measured against `e5f5c565` itself with the same scripts,
copied to `~/scratch/turret-wp3/scripts/` (`world-base` against `world-wp3`; binaries in `bin-base`, `bin-wp3`;
assets in `tree-base`, `tree-wp3`). Plan Amendments 2 and 3.

### Red runs (on `e5f5c565`)

`~/scratch/turret-wp3/red/red-final.log`:

```text
test a_wind_up_holds_its_unit_unless_its_weapon_fires_while_moving ... FAILED
  Solver, turret false: a `Moving` weapon's wind-up does not hold its unit
test a_turret_that_fires_while_moving_tracks_and_fires_while_retreating ... FAILED
  tick 5: the hull moves on every tick, its wind-ups included
test a_hull_weapon_that_fires_while_moving_chases_through_its_wind_up ... FAILED
  the unit chases through its wind-up (3 wind-up ticks, none walked)
test a_turret_that_must_stand_to_fire_stands_for_its_wind_up_in_a_chase ... FAILED
  tick 1 of the chase: the wind-up holds the turreted unit
```

The test that pins B-207 (`a_wind_up_whose_target_is_gone_holds_as_its_idle_weapon_says`, written after Amendment 3)
fails on `e5f5c565` too ("Solver, turret false: a stale `Moving` swing does not hold",
`~/scratch/turret-wp3/red/red-pin-on-base.log`). After the change every test of `stutter_step_tests`,
`turret_tracking_tests` and `golden_replay_tests` passes.

### Worlds

`compare.sh world-base world-wp3`: 47 of 483 files differ. Battles 31 and 33 and the showcase (default seed and 1 to
8) are byte-identical. The matrix line is unchanged ("1220 runs: expectation failures 2 on the seeds of record, 25
over all seeds"); three battles moved, every one with a turreted unit:

| Battle | Seeds moved (of 20) | Winner changes | Turreted shots (20 seeds) | Shots per 1000 alive turreted unit-ticks | Mean end tick |
|---|---:|---|---|---|---|
| 26, 100 lings vs 6 colossi | 9 | seeds 4, 12 (not of record): zerglings → colossi, 2 left each | 1309 → 1353 | 36.83 → 36.79 | 421.7 → 439.1 |
| 28, 100 lings vs 10 immortals | 4 | none | 3819 → 3813 | 31.32 → 31.31 | 803.5 → 805.7 |
| 30, 20 roaches vs 10 immortals | 9 | none | 1216 → 1215 | 25.49 → 25.46 | 237.6 → 237.6 |

No swing is cancelled out of band in either tree in any of the three, and stall give-ups stay 0. First differences,
read from per-unit traces (`matchup_trace --battle … --seed …`):

- **Battle 28, seed 10, tick 62:** immortal 102, in its wind-up against zergling 20 (alive), stands (0.066 per tick
  before); immortal 104 beside it is no longer pushed. The one-shot deficit appears at tick 98, after the battles
  have diverged (seeds 2 and 4, also one to three shots fewer, were not traced per unit).
- **Battle 30, seed 4:** one shot fewer, on the battle's last tick (235 in both).
- **Battle 26, seeds 4 and 12, tick 16:** colossus 104 (seed 4) and 106 (seed 12) were in a 1-tick wind-up on tick
  15 against a zergling (50, 80) that died on tick 15. On tick 16 the movement phase holds each for that stale
  wind-up (Amendment 3, B-207) where it used to walk on. On the comment's 24 seeds (of record and 1 to 20) the
  zerglings now win 18 with 4 to 31 left (20 with 10 to 31 before) and the colossi 6 with 1 to 3 left (seed 33 of
  record unchanged). The expectation `blue (0, 31), red (0, 3)` and the claim hold; the fixture comment is restated.

**Showcase effects** (default seed, 1 to 8) moved on every seed. The first difference is the same on all nine: tick
330, Hellion analogue 14, wind-up against the `foundry_core_dome` (entity 1) begun on tick 329, its target out of
the pursuit band. Before, it walked 0.243; now it is held and pushed 0.116 by the two Hellions walking behind it
(contact may displace a held unit, D-3). Its order is unchanged. Per seed, owner 0's blocked attacker ticks fall
(76 → 69 on the default seed, 63 to 79 against 78 to 101 on seeds 1 to 8). Owner 1's reversals vary both ways (26 →
38 default; 57 → 19 seed 6). Stall give-ups stay 0 on every seed.

**P-12.** The prediction "turreted ground units release at least as many shots" holds as a rate: shots per alive
turreted unit-tick are equal to within 0.1 % in all three battles. Battles 28 and 30 release 6 and 1 fewer shots in
total, traced above to divergence and the final tick, not to a held swing. Two winners changed, both traced to the
stale hold (stop rule 3: explained by the hold; restated under AGENTS.md §8). The Void Ray, Hellion, Tank, Lurker and
the other Colossus battles give identical matrix rows on all 20 seeds; no matrix battle has the Phoenix analogue.

### Amendment 3's alternative, measured

A build in which a wind-up whose bound target is gone or dead holds nothing (`bin-wp3b`, `world-wp3b`) moves 255 of
483 files: battles 31 and 33 on most seeds, the showcase on seeds 1 to 8, and 17 matrix battles, with two more
expectation misses not of record (26 seed 13, 36a seed 2; "2 on the seeds of record, 27 over all seeds"). It is
registered as B-207, not built here.

### Goldens

Per-tick dumps of every entity (a temporary observer, not committed) on `e5f5c565` and on WP3 are equal up to tick
253 of `golden_replay`. The first difference is tick 254, entity 34 of "red_siege" (`railgun_crawler`, standing):
the target of its wind-up, entity 13 of "blue_infantry", died on tick 253; the movement phase of tick 254 holds the
tank for that stale wind-up, so its acquired attack order ends on tick 255 instead of 254 (Amendment 3). Checkpoints
1 to 5 are unchanged and every checkpoint from 6 on changed; the list was re-recorded once with that header.
`golden_replay_orders` and `DEFAULT_SANDBOX_CHECKSUM` are unchanged. All four builds pass
(`~/scratch/turret-wp3/proof-rerecorded/status`): native debug and release, `wasm32-wasip1` release and debug
(`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<worktree>"`).

### Throughput

`cargo bench -p sim_core --bench throughput`, both binaries run in three interleaved rounds on a shared machine
(load average 12 to 20 throughout), ticks per second, median of the rounds (each round itself a median of 3):

| Map | Units | `e5f5c565` | WP3 |
|---|---:|---:|---:|
| 64x64 | 0 | 3096 | 3014 |
| 64x64 | 100 | 794 | 789 |
| 64x64 | 400 | 108 | 106 |
| 128x128 | 400 | 178 | 175 |
| 256x256 | 0 | 194 | 191 |
| showcase_canyon | 400 | 80 | 80 |
| 64x64, 67 casters | 400 | 199 | 199 |

Every scenario stays above 50 ticks per second (Q0093). The differences are within the rounds' spread (64x64 with 0
units: 3087 to 3128 against 2872 to 3118).
