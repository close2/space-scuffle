# Anchored commands: measurements per package

This file records the measurements of plan [`2026-11-23-anchored-commands`](../plans/2026-11-23-anchored-commands.md)
(B-98; B-171 to B-173 registered): the world comparison of WP1 against the WP0 tree, the goldens, the client
skirmish stage log, the red runs of the new tests, and the sources of the plan's §2.1.

## How to reproduce

The scripts are the splash-centres plan's, copied to `~/scratch/anchored-commands/scripts/`. `base.sh <wp>` builds
`matchup_trace` and `route_trace` in release on the worktree's tree, copies them to `bin-<wp>/` and runs
`measure.sh`, which reads that tree's `assets/` (`route_trace` is given `--assets` explicitly; `matchup_trace` embeds
the catalog it was compiled with):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/anchored-commands-plan
R=~/scratch/anchored-commands
$R/scripts/base.sh <wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
$R/scripts/client.sh <wp>
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
the showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks. `client.sh` runs
the client targets this plan touches in release, one at a time under `~/scratch/gate.lock`.

## WP0: baseline

The tree of the plan (`fb162b42`, behaviour-identical to `main` at `2f317d0f`), measured into
`~/scratch/anchored-commands/world-wp0`: 483 files.

## WP1: the data and the shared rule

### Red run

On the WP0 code the new tests did not compile (`~/scratch/anchored-commands/red/wp1.log`): no
`sim_types::StanceOrders`, no field `orders` on `UnitModeSpec`, no `UnitArchetype::stance_orders`, no
`Entity::settled_stance_orders`. Targets: `stance_orders_tests` (10 errors), `catalog_reference_values_tests` (1),
`runtime_catalog_tests` (6), the `sim_core` lib tests (12).

### Worlds

`compare.sh world-wp0 world-wp1`: **files base=483 new=483 differing=0**. Every row of the plan's §7 is byte-identical.
The golden replays (`golden_replay_tests`, native debug) and the sandbox checksum
(`sim_fixtures` `fixture_tests`) pass unchanged; the checksum schema stays 36.

## WP2: the card and admission

### Red run

On WP1's tree (`~/scratch/anchored-commands/red/wp2*.log`):

| Target | Failed | Why |
|---|---|---|
| `client` lib, `battle_hud::card::tests` | `a_sieged_tank_shows_stop_and_no_hold`, `a_sieging_tank_answers_for_the_stance_it_settles_in`, `an_edited_stance_changes_the_fixed_slots` | the card showed Hold for a sieged tank, and Stop and Hold for any mobile type whatever its stance |
| `client` lib, `battle_pipeline::commands::tests` | `the_card_shows_stop_and_hold_exactly_when_admission_takes_them`, `an_anchored_unit_takes_the_plain_orders_of_its_stance`, `a_mixed_hold_goes_only_to_the_members_whose_stance_takes_it` | admission refused Stop and Hold to every anchored unit; the card disagreed with it (F-1) |
| `battle_mode_tests` | `stop_by_key_…`, `stop_by_card_…`, `stop_while_sieging_…`, `hold_keeps_a_burrowed_unit_…` ("Order rejected: select units that can perform this order"), `hold_on_a_sieged_tank_alone_…` (Hold drawn on the card) | B-98 as the player met it |
| `battle_card_tests` | `an_anchored_selection_shows_stop_and_an_empty_hold_slot` | Hold drawn for a sieged tank |

Tests that passed on WP1's tree, as expected: the card tests whose stance keeps both orders (burrowed, unsieging,
mixed with a worker), a structure, the warm card, the revalidation at the tick (Hold was refused there too), and
the mixed Hold through input (the sieged tank was refused before as well).

### Client targets (release, after merging `main`)

| Target | Result |
|---|---|
| `battle_input_tests` | 38 passed |
| `battle_context_order_tests` | 30 passed |
| `battle_card_tests` | 31 passed |
| `battle_mode_tests` | 14 passed |
| `battle_hud_tests` | 35 passed |
| `sync_steady_state_tests` | 8 passed |
| `skirmish_match_tests` | 7 passed, 1 ignored |
| `client` lib | 524 passed |

### Skirmish stage log

`skirmish_match_tests` on WP1's tree (`skirmish-wp1.log`) and on WP2's merged tree (`client-wp2/`): the match is won
in 1683 frames and 6494 ticks on both, and the 43 stage lines are identical (compared sorted, as the test binary
interleaves its threads' output; wall-clock seconds excluded).

## Sources (plan §2.1)

SC2GameData `5444e12f13` (5.0.14), `~/scratch/combat_wp3_data/<layer>/{AbilData,UnitData}.xml`, read as data: S3
`liberty` UnitData:5172–5174 (`SiegeTankSieged`: `stop`, `attack`, `Unsiege`, no `move`); S6 `liberty`
UnitData:793–796 and `void` UnitData:767–769 (`LurkerMPBurrowed`: `move`, `stop`); S1 `core` AbilData:552–554,
:1362, :1371 (Hold Position is `CAbilMove`'s `HoldPos`, Stop is `CAbilStop`). The other rows of §2.1 are cited in
the plan and were not re-read.
