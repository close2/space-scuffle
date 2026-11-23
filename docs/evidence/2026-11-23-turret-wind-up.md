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
