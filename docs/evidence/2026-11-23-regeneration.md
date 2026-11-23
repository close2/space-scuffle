# Regeneration: measurements per package

This file records the measurements of plan [`2026-11-23-regeneration`](../plans/2026-11-23-regeneration.md) (§7):
the world comparison of every code package against the WP0 tree, the goldens, and the constructed worlds of the new
`crates/sim_core/tests/regeneration_tests.rs`. WP1 creates it; WP2 and WP3 append their sections.

## How to reproduce

Binaries are built once per tree and run from a copy of that tree's `assets/`, so each run reads its own catalog.
The scripts are those of the production-queue plan, kept under `~/scratch/regeneration/scripts/`: `measure.sh`
(the resource-footprints measure script with `--assets` passed to `route_trace` explicitly) and `compare.sh` (every
output file byte for byte, logs after removing the output path).

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/regeneration-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/regeneration
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
cp -r assets $R/tree-<wp>/
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>     # expected: differing=0
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
and the showcase scenario on its default seed and seeds 1 to 8 for 6,000 ticks.

## WP0: baseline

The tree of commit `e878c849` (`main` at `2bb84a5b` plus the plan, Q0094 and the register entries), measured into
`~/scratch/regeneration/world-wp0`: the matrix reports "1160 runs: expectation failures 0 on the seeds of record,
4 over all seeds".

## WP1: no vital regenerates before completion

### Red run (WP0 tree)

`cargo test -p sim_core --test regeneration_tests` before the change: 3 passed, 5 failed. Failing:
`a_foundation_does_not_regenerate_life`, `a_foundation_completes_on_the_construction_curve`,
`damage_to_a_foundation_stays_taken` (each stopped by the P-1 check on the first worked tick of the first
regenerating structure: life 150.7712 where the curve gives 150.7559, the difference 0.01531 being one tick of Zerg
regeneration), `a_foundation_holds_its_start_energy` (energy 51 where 50 holds), and
`an_uncompleted_entity_cannot_cast` (an uncompleted caster was admitted). Passing, as the plan expects before and
after: `a_completed_structure_regenerates_from_the_next_tick`, `energy_regenerates_on_the_tick_after_a_cast`,
`energy_regenerates_on_the_tick_after_damage`.

### Life at completion per regenerating structure

`a_foundation_completes_on_the_construction_curve -- --nocapture`, undamaged and worked without a pause. "Full
early" counts the ticks before the completion tick on which the foundation already had `max_hp`. Before is the WP0
rule, measured with the change disabled by a temporary local edit and the curve assertion removed; after is WP1.

| Structure (id) | Laid life | Build ticks | Life at completion, before / after | Full early, before / after |
|---|---|---|---|---|
| `ancient_tree_dome` (129) | 150 | 1,786 | 1,500 / 1,500 | 35 / 0 |
| `canopy_grove_dome` (130) | 200 | 1,429 | 2,000 / 2,000 | 17 / 0 |
| `primal_world_tree_dome` (131) | 250 | 1,786 | 2,500 / 2,500 | 21 / 0 |
| `spore_vent_dome` (132) | 50 | 536 | 500 / 500 | 9 / 0 |
| `spawning_hollow_dome` (133) | 100 | 1,161 | 1,000 / 1,000 | 22 / 0 |
| `metamorphic_chamber_dome` (134) | 75 | 625 | 750 / 750 | 8 / 0 |
| `thorn_lasher_dome` (135) | 30 | 893 | 300 / 300 | 43 / 0 |
| `spore_spire_dome` (136) | 30 | 536 | 300 / 300 | 15 / 0 |
| `carapace_mound_dome` (137) | 85 | 982 | 850 / 850 | 18 / 0 |
| `caustic_nursery_dome` (138) | 85 | 1,071 | 850 / 850 | 22 / 0 |
| `spine_den_dome` (139) | 85 | 714 | 850 / 850 | 10 / 0 |
| `burrow_spire_dome` (140) | 85 | 1,429 | 850 / 850 | 39 / 0 |
| `chitin_spire_dome` (141) | 85 | 1,786 | 850 / 850 | 61 / 0 |
| `apex_spire_dome` (142) | 100 | 1,786 | 1,000 / 1,000 | 52 / 0 |
| `infestation_nursery_dome` (143) | 85 | 893 | 850 / 850 | 15 / 0 |
| `subterranean_network_dome` (144) | 85 | 893 | 850 / 850 | 15 / 0 |
| `behemoth_den_dome` (145) | 85 | 1,161 | 850 / 850 | 26 / 0 |

Before WP1 every regenerating foundation reached `max_hp` early (8 to 61 ticks before completion) and was clamped
there, as the plan's §3 expected. After WP1 every one reaches `max_hp` exactly on its completion tick. The build
ticks column is the catalog's `build_time_ticks`; the test counts the ticks before the completion tick, one fewer.
Logs: `~/scratch/regeneration/wp1-life-at-completion-before.log`, `~/scratch/regeneration/wp1-green.log`.

### Worlds

| World | Result |
|---|---|
| Matrix, battles 31 and 33, showcase on 9 seeds (`compare.sh world-wp0 world-wp1`) | byte-identical: "files base=420 new=420 differing=0" |
| `golden_replay_tests` (native debug) | 4 passed, no checksum moved |
| `sim_fixtures` `fixture_tests` (`DEFAULT_SANDBOX_CHECKSUM`) | 3 passed; the checksum is of the spawned state before any tick (verified, plan §5) |
| Constructed worlds (`regeneration_tests`) | 8 passed: P-1 checked after every tick of the foundation tests, P-2, P-3, P-4, and the refusal of an uncompleted caster |

The caster refusal: `world/abilities.rs` `admit_cast` and `world/casts.rs` `would_accept_cast_order` read no
completion before WP1 (plan D-1 invariant 3, **verify**), so WP1 adds it to both. No shipped structure has an
ability, so no world moves.

## WP2: structure energy regeneration

### Red run (WP1 tree)

`cargo test --no-fail-fast -p sim_core --test regeneration_tests --test catalog_reference_values_tests --test
runtime_catalog_tests` before the change: `structures_with_energy_regenerate_as_5_0_14` failed (the three
structures with energy read a rate of 0), `a_structure_with_energy_regenerates_it` failed (no gain), and
`an_edited_structure_energy_rate_changes_the_regeneration` failed (`structures.ron` had no `energy_regen_per_tick`
to edit). The three `catalog/validation.rs` unit tests (`a_structure_without_energy_rejects_an_energy_rate`,
`a_unit_without_energy_rejects_an_energy_rate`, `an_omitted_structure_energy_rate_reads_as_zero`) did not compile,
as `StructureArchetypeDef` had no `energy_regen_per_tick` field. Logs: `~/scratch/regeneration/wp2-red.log`,
`~/scratch/regeneration/wp2-red-lib.log`.

### Constructed worlds and data

- P-5: each shipped structure with energy (the Orbital Command, Nexus and Shield Battery analogues), spawned
  complete two and a half ticks below its maximum, gains 0.0315 per tick and stops at `max_energy`
  (`regeneration_tests::a_structure_with_energy_regenerates_it`).
- Every structure with energy authors 0.0315, every other structure reads 0, and the set with energy is exactly
  those three analogues (`catalog_reference_values_tests::structures_with_energy_regenerate_as_5_0_14`).
- P-7: a catalog copy with the first authored rate set to 0.063 doubles that structure's gain per tick in the same
  binary (`runtime_catalog_tests::an_edited_structure_energy_rate_changes_the_regeneration`).
- Validation refuses a positive rate without energy and a negative rate, for structures and units alike, naming
  `energy_regen_per_tick`; every shipped unit with a rate has energy.

### Worlds

| World | Result |
|---|---|
| Matrix, battles 31 and 33, showcase on 9 seeds (`compare.sh world-wp0 world-wp2`, run from the WP2 tree's assets) | byte-identical: "files base=420 new=420 differing=0" |
| `golden_replay_tests` (native debug) | 4 passed, no checksum moved |
| `sim_fixtures` tests (`DEFAULT_SANDBOX_CHECKSUM`) | passed, unchanged |

No measured world holds a structure with energy, so none moves.

## WP3: the final tree

- `golden_replay_tests` in release (native): 4 passed, no checksum moved. The package gate runs the
  `wasm32-wasip1` goldens.
- Client skirmish match (`cargo test --release -p client --test skirmish_match_tests -- --nocapture`, the
  physical GPU through the harness's own lock): 7 passed, 1 ignored. Its stage log equals the one recorded for the
  production-queue plan's last package on `main`'s tree `2bb84a5b`
  (`~/scratch/production-queue/impl/client-wp5/status`): "match won in 1683 frames, 6494 ticks", and every stage
  on the same tick (Raise 22, Site 550, Foundation 586, Train 642, Group 4502, March 4526, the three Command and
  Assault pairs at 4822/4834, 5086/5098 and 5350/5362, Done 6486). The pilot raises a Barracks analogue, which has
  neither life regeneration nor energy, so neither rule can act there (plan §7 **verify**, confirmed).
  Log: `~/scratch/regeneration/skirmish.log`.
