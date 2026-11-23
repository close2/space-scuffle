# Stance-checked orders: measurements per package

This file records the measurements of plan
[`2026-11-23-stance-checked-orders`](../plans/2026-11-23-stance-checked-orders.md) (§7; B-171, B-172, B-173): the
sources the plan's rules rest on, the world comparison of every package against the WP0 tree, and the red runs of
the new tests. WP0 creates it; WP1 to WP4 append.

## Sources

All from SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as data
(plan §2.1). The override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti →
balancemulti`.

| # | Fact | Source |
|---|---|---|
| S1 | Stop is its own ability (`CAbilStop`); Hold Position is the `HoldPos` command of `move` | `core` AbilData:1013–1027, :1362; :552–554 |
| S2 | Stop, Hold Position and Attack go to every selected unit that has them (`ToSelection`) | `core` AbilData:1015–1016, :127–129, :553 |
| S3 | The sieged Siege Tank carries `stop`, `attack` and `Unsiege`, and no `move` | `liberty` UnitData:5172–5174 |
| S4 | Its Attack button has face `AttackBuilding`; the unsieged tank's has `Attack` | `liberty` UnitData:5187; :5116 |
| S5 | The burrowed Lurker carries `attack`, both burrow abilities, `move` and `stop`; Attack face `AttackBuilding` | `liberty` UnitData:793–796, :800; `void` UnitData:767 |
| S6 | Armed structures carry `stop` and `attack` (Photon Cannon, Missile Turret, Planetary Fortress, Spine and Spore Crawler) | `liberty` UnitData:11083–11084, :6064–6065, :10696–10697, :8652–8653, :8784–8785 |
| S7 | Unarmed structures carry neither (Command Center, Orbital Command, Nexus) | `liberty` UnitData:10411–10418, :10557–10564, :4218–4223 |
| S8 | The Shield Battery's `liberty` `stop` (entry 2) is removed in 5.0.14 | `liberty` UnitData:1603–1605; `voidmulti` UnitData:1557; `balancemulti` UnitData:1463 |
| S9 | The Nexus's Stop and Attack require `PurifyNexusRequirements` | `swarm` AbilData:954–960, :968–970 |
| S10 | The Bunker's Stop and Attack redirect to its passengers | `liberty` UnitData:6192–6193; AbilData:3606–3611, :3633–3638 |
| S11 | The unburrowed Lurker carries `attack` without a weapon | `voidmulti` UnitData:974; `balancemulti` UnitData:909; `liberty` UnitData:735 |
| S12 | The attack ability skips own and allied units for acquisition and the smart click only | `core` AbilData:118–119 |
| S13 | Stop drops the unit's orders; nothing in `CAbilStop` treats a structure differently | `core` AbilData:1013–1027 |

## How to reproduce

The scripts are the anchored-commands plan's, copied to `~/scratch/stance-orders/scripts/` with the worktree and
output paths changed (`client.sh` also runs `battle_card_path_tests`). `matchup_trace` embeds the matrix fixture and
the catalog it was compiled with, so it is rebuilt for each tree; `route_trace` is given `--assets` explicitly.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/stance-orders-plan
R=~/scratch/stance-orders
$R/scripts/base.sh <wp>        # builds matchup_trace and route_trace in release, runs measure.sh into world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
$R/scripts/client.sh <wp>      # client targets in release, one at a time under ~/scratch/gate.lock
```

`measure.sh` plays the matchup matrix on its 4 seeds of record and 16 more (20 a battle), battle 31 on 100 seeds,
battle 33 on 64, and the showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000
ticks.

## WP0: baseline

The tree of the plan and Q0098 (`b78206d6`, behaviour-identical to `main` at `aa92b094`), measured into
`~/scratch/stance-orders/world-wp0`: 483 files; the matrix reports "1160 runs: expectation failures 0 on the seeds
of record, 4 over all seeds". The four battles whose orders WP1 restates read (20 seeds each):

| Battle | Blue wins | Red wins | Unresolved | Survivors blue – red | End tick | Red lead hull |
|---|---|---|---|---|---|---|
| 37a, Abduct on a sieged tank | 0 | 0 | 20 | 1.00 – 4.65 | 600.0 | — (dead on 20) |
| 37b, control | 0 | 0 | 20 | 1.00 – 6.00 | 600.0 | 141.45 (20) |
| 39a, Blinding Cloud on sieged tanks | 20 | 0 | 0 | 7.90 – 0.00 | 280.1 | — |
| 39b, control | 0 | 0 | 20 | 1.00 – 2.50 | 600.0 | 82.35 (17) |

## WP1: the four battles restated

### Red run (on the WP0 tree)

`cargo test -p sim_fixtures --lib matchup` with the new tests and the WP0 fixture (log
`~/scratch/stance-orders/red/wp1-red.log`): `no_matrix_battle_scripts_a_stop_or_hold_that_misses_a_unit_it_names`
(P-9) failed with "battles whose scripted Stop or Hold Position misses a unit: [37a_abduct_sieged_tank,
37b_sieged_tank_survives_without_abduct, 39a_cloud_on_tanks, 39b_tanks_beat_roaches_without_cloud]"; the rewritten
`mode_commands_precede_the_standing_order_which_survives_the_switch` passed. 13 passed, 1 failed.

### Worlds

After D-5's orders, `compare.sh world-wp0 world-wp1`: `files base=483 new=483 differing=0`. Battles 37a, 37b, 39a
and 39b are byte-identical on all 20 seeds (wins, survivors, end tick, lead fate and every other `matrix.csv`
column), as §3.3 predicted, so no tank target was traced and no expectation re-recorded.
`sim_core/sc2_who_wins_matchup_matrix_tests` (release) passed (3 tests); every `sim_fixtures` test passed.

## WP2: the simulation's rule; Stop for armed structures

### Red run (on the WP1 tree, with `stance_armed` and `settled_armed` in and the old rules)

Logs `~/scratch/stance-orders/red/wp2-*.log`:

| Test | Failure |
|---|---|
| `stance_orders_tests::the_simulation_gives_stop_and_hold_only_to_a_stance_that_takes_them` (P-1) | `railgun_crawler Entering(0) HoldPosition queue false: takes false`: the entering tank was given the Hold |
| `stance_orders_tests::one_command_reaches_each_named_unit_whose_stance_takes_it` (P-2) | "the sieged unit is left as it was" |
| `stance_orders_tests::the_default_stance_takes_stop_when_mobile_or_armed_and_hold_when_mobile` (P-4 rule) | `tesla_sentinel_dome`: `NONE` |
| `catalog_reference_values_tests::b171_structures_take_the_plain_orders_of_their_starcraft_unit_type` (P-4) | "Photon Cannon: Stop and Hold Position, StarCraft II liberty UnitData:11083-11084" |
| `runtime_catalog_tests::an_armed_structure_disarmed_in_its_file_loses_stop` (P-8) | "tesla_sentinel_dome: shipped with Stop" |
| `runtime_catalog_tests::an_anchoring_mode_given_hold_position_in_its_file_holds` (P-8) | the shipped sieged tank held (left `true`, right `false`) |
| `catalog::tests::the_default_stance_takes_stop_when_mobile_or_armed` | "slot 0": an armed structure had no Stop |
| client `battle_hud::card::tests::an_armed_structure_shows_stop_and_no_hold` | `(false, false)` |
| client `battle_pipeline::commands::tests::the_simulation_delivers_stop_and_hold_exactly_when_the_card_shows_them` (P-3) | "unarmed structure Stop": the simulation gave Stop to a structure the card shows none for |
| client `battle_card_tests::stop_on_an_armed_structure_drops_its_attack_and_keeps_production_and_rally` (P-5) | "Order rejected: select units that can perform this order" for `X` |

`armed_in_a_stance_means_the_live_unit_has_a_weapon` (P-6), `a_unit_armed_only_in_its_mode_is_armed_for_the_stance_it_settles_in`,
`a_mode_without_its_replacement_weapon_in_its_file_is_unarmed` (P-8) and the extended
`the_card_shows_stop_and_hold_exactly_when_admission_takes_them` passed on both trees: they pin the new queries, which
exist before the rules change.

### Tests rewritten by §8 rule 4

- `stance_orders_tests::the_default_stance_accepts_both_orders_exactly_when_the_archetype_is_mobile` asserted `NONE`
  for every immobile archetype; it is replaced by `the_default_stance_takes_stop_when_mobile_or_armed_and_hold_when_mobile`.
- `catalog::tests::stance_orders_follow_mobility_for_the_default_stance_and_data_for_a_mode`: the speed-0 walker
  (armed) now keeps Stop, and the dome it checks is found unarmed.
- `multi_strike_timing_tests::q0058_an_explicit_order_ends_a_channeled_burst` sent Hold Position to a scratch shooter
  with speed 0, which the simulation now refuses; the bench's shooter is given a speed of 0.1 so every overriding
  order reaches it (it is never told to walk before the order under test). All 10 tests of the file pass.

No other test in `sim_core`, `sim_fixtures` or the client library failed. The grep of plan §11 found every other
Stop and Hold Position in `crates/sim_core/tests/` and `crates/client/tests/` either sent to mobile units, issued
straight through `Entity::issue_order` (which the rule does not gate), or submitted through client admission, which
applies the same `settled_stance_orders`.

### Worlds

`compare.sh world-wp0 world-wp2`: `files base=483 new=483 differing=0`. The golden replays (`golden_replay_tests`)
and `DEFAULT_SANDBOX_CHECKSUM` (`sim_fixtures`) pass unchanged on native debug.

## WP3: Attack by stance

### Red run (on the WP2 tree)

Log `~/scratch/stance-orders/red/wp3-red.log`:

| Test | Failure |
|---|---|
| `card::tests::an_armed_structure_shows_attack_and_an_unarmed_one_does_not` | no Attack first on the armed structure's card (`None`) |
| `card::tests::a_burrowed_unit_armed_only_when_burrowed_shows_attack_and_follows_its_settled_stance` | "burrowing: armed in the stance it settles in" |
| `card::tests::a_unit_armed_only_in_its_mode_shows_attack_only_in_its_mode` (rewritten, below) | `[Leave]` without Attack |
| `commands::tests::the_card_shows_attack_exactly_when_admission_takes_an_attack` (P-3) | the armed structure: card `false`, admission `true` |
| `commands::tests::an_anchored_unit_or_armed_structure_takes_an_attack_on_a_unit_and_no_attack_move` | the burrowed Lurker analogue refused `AttackTarget` |
| `command_mode::tests::attack_stays_for_anchored_units_and_ends_for_unarmed_ones` | a sieged tank alone ended the mode (`Normal`) |
| `battle_card_tests::an_armed_structure_attacks_an_enemy_clicked_in_attack_mode` (P-7) | "Attack is not on the card" |
| `battle_card_tests::a_walker_attacks_an_enemy_clicked_in_attack_mode` (P-7) | `AttackMove` to where the enemy stood |
| `battle_mode_tests::a_sieged_tank_attacks_a_clicked_enemy_and_refuses_the_ground` (P-7) | the Attack mode ended at once (`Normal`) |
| `battle_mode_tests::a_burrowed_unit_shows_attack_and_attacks_a_clicked_enemy` (P-7) | "no Attack entry" |

`a_sieged_tank_shows_attack`, the warm-card test with an armed structure and anchored units (P-11) and
`attack_on_the_ground_with_a_sieged_tank_and_a_walker_moves_the_walker_only` passed on both trees: the sieged tank
already showed Attack, the derivation allocated nothing, and the attack-move already named the walker only.

### Tests rewritten by §8 rule 4

- `card::tests::a_unit_armed_only_in_its_mode_shows_its_switch_first_and_no_attack` expected no Attack for the
  burrowed Lurker analogue; renamed `a_unit_armed_only_in_its_mode_shows_attack_only_in_its_mode`, it expects
  `[Attack, Leave]` once burrowed and `[Enter]` before.

### Client targets and the skirmish

`client.sh wp3` (release, one target at a time; `~/scratch/stance-orders/client-wp3/`): `battle_input_tests` 38,
`battle_context_order_tests` 30, `battle_card_tests` 34, `battle_card_path_tests` 4, `battle_mode_tests` 17,
`battle_hud_tests` 35, `sync_steady_state_tests` 8, `skirmish_match_tests` 7 (1 ignored) and the client library 533
passed, none failed. The skirmish stage log ("match won in 1683 frames, 6494 ticks" and its 16 stages) is identical
to the one of `main` before this plan (`~/scratch/anchored-commands/client-wp2/`), wall-clock time aside. The
simulation is untouched by WP3, so the measured worlds were not run again (plan §7).
