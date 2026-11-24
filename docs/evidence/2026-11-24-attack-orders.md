# Attack orders by stance: measurements per package

This file records the measurements of plan [`2026-11-24-attack-orders`](../plans/2026-11-24-attack-orders.md)
(§7; B-174, B-175, B-186, B-187): the sources the plan's rules rest on, the S8 join, the world comparison of every
package against the WP0 tree, and the red runs of the new tests. WP0 creates it; WP1 to WP4 append.

B-186 and B-187 are provisional numbers, the next free at merge (`main` ends at B-181 and another branch holds
B-182 to B-185).

## Sources

All from SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as data
(plan §2.1). The override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti →
balancemulti`.

| # | Fact | Source |
|---|---|---|
| S1 | The attack ability's only alliance filters are for acquisition (`AcquireFilters` excludes `Player, Ally, Neutral, Dead, Hidden, Invulnerable, Benign`) and the smart click (`SmartFilters` excludes `Player, Ally`); `CAbilAttack` has no `TargetFilters` of its own | `core` AbilData:115–139 (:117–119) |
| S2 | Its `Execute`, `Towards` and `Barrage` commands go to every selected unit that has them (`ToSelection`) | `core` AbilData:127–135 |
| S3 | Weapons accept own units unless they say otherwise: default `TargetFilters` `Visible;-`; ordinary weapons exclude `Missile, Stasis, Dead, Hidden, Invulnerable`; `PointDefenseLaser`, `BypassArmorDroneWeapon`, `WidowMineDummy`, `TwinIbiksCannon` and `BuildingShield` exclude their own side or themselves by name | `core` WeaponData:3–31; `liberty` WeaponData:9, :19, :35, :71, :136; `swarm` WeaponData:8, :76; `voidmulti` WeaponData:267; `balancemulti` WeaponData:240 |
| S4 | `AttackWarpPrism` excludes `Self, Player, Ally, Neutral, Enemy` in its acquire and smart filters and turns off `ToSelection` | `liberty` AbilData:1100–1110 |
| S5 | The move ability carries `Move`, `Patrol`, `HoldPos`, `AcquireMove` (Scan Move) and `Turn`, each `ToSelection` | `core` AbilData:533–561 (:555–557) |
| S6 | Every unarmed mobile unit's card puts `attack,Execute` and then `move,AcquireMove` in the Attack slot (row 0, column 4); these units carry `move` and no `attack`, so the slot is Scan Move | Medivac `liberty` UnitData:5475–5478, :5485–5486; Observer :3443–3445, :3452–3453; Overlord :7391–7395, :7400–7401; Overseer :7520–7521; Raven :5621–5622; Infestor :10108–10109; Warp Prism :3859–3860; Viper `swarm` UnitData:2363–2364; Swarm Host `swarm` :1214–1215 |
| S7 | The unburrowed Lurker has the same pair, and 5.0.14 adds `attack` to it, so its slot is Attack although it has no weapon | `liberty` UnitData:727–729, :735–736; `voidmulti` UnitData:974; `balancemulti` UnitData:909 |
| S8 | Every unit with a weapon carries an attack ability; the units with an attack ability and no weapon are listed below | the join below |
| S9 | The burrowed Roach's slot changes from Attack to Scan Move in LotV | `liberty` UnitData:7284; `voidmulti` :1472; `balancemulti` :1383 |
| S10 | The sieged Siege Tank and the burrowed Lurker carry `attack` | `liberty` UnitData:5172–5174, :793–796 |
| S11 | The Lurker's burrow and unburrow change the ability set at the start of the switch | `swarm` AbilData:1663–1673 |

Web sources (plan §2.1): Liquipedia's *Micro (StarCraft)*; the Blizzard forum answers
`us.forums.blizzard.com/en/starcraft/t/how-can-i-remove-building/2150` (StarCraft: Remastered, lineage only) and
`us.forums.blizzard.com/en/sc2/t/which-units-deal-friendly-fire/12103` (splash only). None states the 5.0.14 A-click
on an own unit, so B-175 stays **verify** (Q0104).

## The S8 join

A short Python script (stdlib `xml.etree`, run with `python3 -I`; kept in `~/scratch/attack-orders/s8/`) collects
every `CAbilAttack` id from the eight layers' `AbilData.xml`, then applies each layer's `AbilArray` and `WeaponArray`
entries per `CUnit` (`index` and `removed` honoured, `parent` inheritance not followed, plan R-6). Its output:

```text
attack abilities: AttackWarpPrism, BattlecruiserAttack, attack, attackProtossBuilding

# units with an attack ability and no weapon
AdeptPhaseShift	attack
FlyoverUnit	attack
LurkerMP	attack,attack
Nexus	attackProtossBuilding
PylonOvercharged	attackProtossBuilding,attackProtossBuilding
QueenMP	attack
Replicant	attack
WarpPrismPhasing	AttackWarpPrism

# units with a weapon and no attack ability
```

- The second list is empty: every unit with a weapon carries an attack ability (S8).
- `LurkerMP` (the unburrowed Lurker, S7), `AdeptPhaseShift` (the shade, plan §14), `Nexus` and `PylonOvercharged`
  (under `PurifyNexusRequirements`) are the multiplayer cases the plan names. `FlyoverUnit`, `QueenMP` and
  `Replicant` are the three campaign or flyover units.
- `WarpPrismPhasing` carries `AttackWarpPrism`, S4's ability that excludes every alliance and turns off
  `ToSelection`, so it gives no A command in practice.
- The Bunker does not appear: its Attack is a redirect (`CAbilRedirect`, stance-checked-orders S10), not a
  `CAbilAttack`.
- `LurkerMP` and `PylonOvercharged` list their ability twice because a later layer appends it without `index`; the
  join does not merge duplicates.

## How to reproduce

The scripts are the stance-checked-orders plan's, copied to `~/scratch/attack-orders/scripts/` with the worktree and
output paths changed. `matchup_trace` embeds the matrix fixture and the catalog it was compiled with, so it is
rebuilt for each tree; `route_trace` is given `--assets` explicitly.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/attack-orders-plan
R=~/scratch/attack-orders
$R/scripts/base.sh <wp>        # builds matchup_trace and route_trace in release, runs measure.sh into world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
$R/scripts/client.sh <wp>      # client targets in release, one at a time under ~/scratch/gate.lock
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
and the showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks.

## WP0: baseline

The tree of the plan and its questions (`c72c6a4f`, exported with `git archive` to
`~/scratch/attack-orders/tree-wp0` so that later edits in the worktree cannot reach a measurement), measured into
`~/scratch/attack-orders/world-wp0`: 483 files; the matrix reports "1180 runs: expectation failures 0 on the seeds of
record, 4 over all seeds". `base.sh` now takes the commit to measure as its second argument for that reason: a first
WP0 run measured the worktree while WP1's `units.ron` edit landed in it, and its showcase-effects runs read the new
field (`route_trace: invalid catalog …`); that run was discarded.

## WP1: the A command as data

### Red run (the field and the enum in place, the derivation still `attack: None` everywhere, no validation)

Logs `~/scratch/attack-orders/red/wp1-red-a.log` and `wp1-red-b.log`:

| Test | Failure |
|---|---|
| `catalog::tests::the_default_a_command_follows_weapons_and_mobility_unless_authored` | "mobile true, weapon slot 0": `None`, expected `Attack` |
| `catalog::tests::the_default_stance_takes_stop_when_mobile_or_armed` (rewritten, below) | "armed false": `attack: None`, expected `ScanMove` |
| `catalog::tests::stance_orders_follow_mobility_for_the_default_stance_and_data_for_a_mode` (rewritten, below) | the mobile armed walker: `attack: None`, expected `Attack` |
| `catalog_reference_values_tests::b186_stances_have_the_a_command_of_their_starcraft_unit_type` (P-1) | "Medivac None: the A command, StarCraft II liberty UnitData:5475-5478, :5485-5486": `None`, expected `ScanMove` |
| `catalog_reference_values_tests::the_a_command_is_derived_for_every_archetype_but_the_lurker_analogue` (P-1) | `welder_drone`: `None`, expected `Attack` |
| `runtime_catalog_tests::removing_an_authored_default_stance_derives_scan_move` (P-8) | `burrow_scorpion`: shipped `None`, expected `Attack` |
| `runtime_catalog_tests::an_unarmed_walker_authored_without_an_a_command_loses_scan_move` (P-8) | `repair_shuttle`: `None`, expected `ScanMove` |
| `runtime_catalog_tests::a_stance_with_scan_move_and_no_hold_position_is_refused` (P-8, validation) | the edited catalog loaded (`Ok(UnitCatalog { … })`) |
| `entity::can_attack_tests` (both) | do not compile: no `Entity::can_attack`, `Entity::settled_attack_command` or `UnitArchetype::stance_weapon_against` (E0599) |

### Tests rewritten by §8 rule 4

- `catalog::tests::the_default_stance_takes_stop_when_mobile_or_armed`: an unarmed walker's default set is now
  `ALL` with `attack: ScanMove`, not `ALL`.
- `catalog::tests::stance_orders_follow_mobility_for_the_default_stance_and_data_for_a_mode`: the speed-0 walker's
  set gains `attack` (Attack when armed, none otherwise), and the mode loop covers the three A commands.
- `stance_orders_tests::the_default_stance_takes_stop_when_mobile_or_armed_and_hold_when_mobile` compares Stop and
  Hold Position only; the A command has its own derivation and P-1 table.
- `runtime_catalog_tests::edited_stance_orders_change_the_loaded_rule` and
  `an_anchoring_mode_given_hold_position_in_its_file_holds` write the new required field in their RON edits.
- `damage_and_shields_tests` and `attack_move_simulation_tests` build a `UnitArchetype` literal, which gains
  `default_stance_orders: None`; the client's `card.rs` test that rewrites a mode's orders keeps its `attack`.

### Green

`sim_types`, `sim_fixtures` and `sim_core` (every target, the matrix included): 109 test binaries, none failed
(`~/scratch/attack-orders/wp1-sim-tests.log`). Clippy on the three crates with `-D warnings` is clean.

## WP2: the simulation

### Red run (on the WP1 tree)

Logs `~/scratch/attack-orders/red/wp2-red.log` and `wp2-red-p8.log`:

| Test | Failure |
|---|---|
| `stance_orders_tests::the_simulation_gives_an_attack_only_to_a_unit_that_can_attack_its_target` (P-2) | "welder_drone Default EnemyAir queue false: takes false": the ground-only worker was given the attack on an air unit |
| `stance_orders_tests::one_attack_command_reaches_each_named_unit_that_can_attack_the_target` (P-3) | "EnemyAir: the unarmed unit is left as it was": the unarmed unit took the attack |
| `scenario::tests::an_attack_move_reaches_every_mobile_unit_with_an_a_command` (P-10) | the scan-mover had no order (`[Some(AttackMove …), None]`) |
| `runtime_catalog_tests::an_anchoring_mode_authored_without_attack_refuses_an_attack` (P-8, simulation half; run against WP1's `world/commands.rs`) | "edited true": the sieged tank authored without Attack took the attack |

Green on both trees, as the plan expected: `friendly_attack_tests` (P-4, both the in-reach and the closing-in
case: an attack ordered on an own unit is carried out today, and the chase in `movement/` closes in, so R-1 did not
occur and nothing moves to WP5), `an_unarmed_attack_mover_walks_past_enemies_and_acquires_nothing` (P-5) and
`sim_fixtures` `no_matrix_battle_scripts_an_attack_that_misses_a_unit_it_names` (P-9: every scripted first-tick
`AttackTarget` reaches its units and every `AttackMove` names units with an A command).

### Tests rewritten by §8 rule 4

- `scenario_tests::enter_modes_queues_behind_earlier_orders` expected the burrowing units (unarmed by default) to
  take no attack-move and switch at once. They now have Attack and walk along (D-6). To keep the test's "switch at
  once" branch, the test authors the default stance of the mode units unarmed by default without an A command in
  the world's catalog before the orders, a run-time edit the catalog allows; both branches are still counted.

### The grep of plan §11

Every `AttackTarget` given straight to a world in `crates/sim_core/tests/`, `crates/sim_fixtures/` and the client's
tests was checked by running them: the sim crates' tests issue it through `Entity::issue_order` (not gated) or to
armed units that reach the target's plane; the crowd harness's `Attack` role units are armed (the crowd tests pass);
the client's world tests submit through client admission. None needed a rewrite.

### Green

`sim_fixtures` and `sim_core`, every target with `--no-fail-fast` (the matrix and the goldens included): 100 test
binaries, none failed (`~/scratch/attack-orders/wp2-sim-tests.log`).

### Worlds

`compare.sh world-wp0 world-wp1` and `compare.sh world-wp0 world-wp2` (each tree exported from its commit, `1d353ed4`
and `5e5b4fe8`): `files base=483 new=483 differing=0` both. The matrix (seeds of record and 16 more), battles 31 and
33, and the showcase and showcase effects on the default seed and seeds 1 to 8 are byte-identical, as plan §7
predicted: no fixture or shipped scenario orders a unit that D-3 refuses or D-6 now adds.

## WP3: the client

### Red run (on the WP2 tree; `CardLabel::ScanMove` declared but not emitted, the partition test set aside because
`submit_attack_on_unit` did not exist yet)

Log `~/scratch/attack-orders/red/wp3-red.log`:

| Test | Failure |
|---|---|
| `card::tests::an_unarmed_mobile_unit_shows_scan_move_first` | no A slot |
| `card::tests::every_unarmed_mobile_unit_has_its_a_slot_first` (R-2) | `UnitTypeId(17)`: `None`, expected `ScanMove` |
| `card::tests::a_selection_with_any_attack_member_shows_attack_and_no_scan_move` | the Medivac-class unit and the Lurker analogue: first entry the Lurker's switch, not Attack |
| `card::tests::a_unit_armed_only_in_its_mode_shows_attack_in_both_stances` (rewritten, below) | `[Enter]` without Attack |
| `card::tests::a_burrowed_unit_armed_only_when_burrowed_shows_attack_in_every_stance` (rewritten, below) | "unburrowed: Attack without a weapon" |
| `commands::tests::the_card_shows_the_a_slot_exactly_when_admission_takes_an_a_command` (P-6) | "132 Attack, 0 Scan Move, 78 none": no selection showed Scan Move |
| `commands::tests::client_admission_of_an_attack_on_a_unit_equals_the_simulations_delivery` (P-6) | the worker on the own target: the simulation delivers, admission refused |
| `battle_mode_tests::a_scan_mover_shows_scan_move_and_attack_moves_on_ground_and_on_an_enemy` (P-7) | no A slot |
| `battle_mode_tests::the_unburrowed_lurker_analogue_shows_attack_and_attack_moves_on_the_ground` (P-7) | no A slot |
| `battle_mode_tests::a_mixed_army_attack_moves_together_and_splits_on_an_enemy` (P-7) | the test's own selection step (units spawned too close to click apart; fixed by spacing them before the green run) |
| `battle_mode_tests::an_a_click_on_ones_own_unit_attacks_it` (P-7) | the same selection step |
| `commands::tests::an_attack_on_a_unit_splits_the_selection_into_attackers_and_walkers` (D-5) | did not compile: no `submit_attack_on_unit` (E0599) |

### Tests rewritten by §8 rule 4

- `card::tests::a_unit_armed_only_in_its_mode_shows_attack_only_in_its_mode` expected no Attack for the unburrowed
  Lurker analogue; renamed `a_unit_armed_only_in_its_mode_shows_attack_in_both_stances`, it expects
  `[Attack, Enter]` before burrowing and `[Attack, Leave]` after.
- `card::tests::a_burrowed_unit_armed_only_when_burrowed_shows_attack_and_follows_its_settled_stance` expected no
  Attack unburrowed and unburrowing; renamed `…_shows_attack_in_every_stance`.
- `commands::tests::the_card_shows_attack_exactly_when_admission_takes_an_attack` (the previous plan's P-3 Attack
  half) is replaced by P-6's `the_card_shows_the_a_slot_exactly_when_admission_takes_an_a_command`: the unburrowing
  Lurker analogue now shows Attack with no target it can attack, so "Attack exactly when some attack is admitted" no
  longer holds; the A slot follows `any_attacker` or an admitted attack-move.
- `commands::tests::stationary_armed_actor_can_target_hostile_but_cannot_attack_move` expected an attack on the unit
  itself to be refused as `InvalidTarget` (the owner filter); an own unit is now a valid target and the self-attack
  is refused by the member's capability (`Capability`).
- P-6's delivery test reads the simulation's command phase (`SimWorld::process_commands`) rather than a whole tick:
  an anchored armed structure out of reach takes the attack and drops it later in the same tick, which is combat's
  rule, not admission's.

`default_armed`, `mode_armed` and `settled_armed` in the HUD lost their last reader and are removed.

### Client targets and the skirmish

`client.sh wp3` (release, one target at a time under `~/scratch/gate.lock`; `~/scratch/attack-orders/client-wp3/`):
`battle_input_tests` 38, `battle_context_order_tests` 30, `battle_card_tests` 34, `battle_card_path_tests` 4,
`battle_mode_tests` 21, `battle_hud_tests` 35, `sync_steady_state_tests` 8, `skirmish_match_tests` 7 (1 ignored) and
the client library 539 passed, none failed. The skirmish stage log ("match won in 1683 frames, 6494 ticks" and its 16
stages) is identical to the stance-checked-orders plan's (`~/scratch/stance-orders/client-wp3/`), wall-clock time
aside: the pilot trains armed units only, so §8 rule 3's case did not arise. The simulation is untouched by WP3, so
the measured worlds were not run again.

## After merging `main` (`22daa14e`, vision and detection, schema 41)

- `compare.sh world-main world-merged` (`main` at `22daa14e` against the merge `3e29c8d1`, both exported, rebuilt and
  measured the same way): `files base=483 new=483 differing=0`. The plan's changes move no measured world on top of
  the vision branch either. (`base.sh` now touches the exported sources before building: an export's commit-time
  mtimes let a first `main` build reuse stale release binaries; that run was discarded.)
- `cargo gate quick` on the final tree: green (fmt, clippy on `client`, `sim_core`, `sim_fixtures`, `sim_types`, and
  their fast tests).
- `client.sh merged` (release, one target at a time under `~/scratch/gate.lock`): `battle_input_tests` 38,
  `battle_context_order_tests` 30, `battle_card_tests` 34, `battle_card_path_tests` 4, `battle_mode_tests` 21,
  `battle_hud_tests` 35, `sync_steady_state_tests` 8, `skirmish_match_tests` 7 (1 ignored), client library 540, all
  passed; the skirmish stage log is unchanged. `sim_core/sc2_who_wins_matchup_matrix_tests` (release): 3 passed.
