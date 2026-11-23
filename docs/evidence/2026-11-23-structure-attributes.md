# Structure attributes: measurements per package

This file records the measurements of plan
[`2026-11-23-structure-attributes`](../plans/2026-11-23-structure-attributes.md) (§7, B-88): the world comparison
of the code package against the WP0 tree, the goldens, the red run of the new tests, and the attribute bits per
faction before and after. WP1 creates it; WP2 completes it.

## How to reproduce

Binaries are built once per tree and run from a copy of that tree's `assets/`, so each run reads its own catalog
(`matchup_trace` embeds the catalog it was compiled with; `route_trace` is given `--assets` explicitly). The scripts
are the regeneration plan's, copied to `~/scratch/structure-attributes/scripts/`: `measure.sh` (the
resource-footprints measure script with `--assets` passed to `route_trace`) and `compare.sh` (every output file byte
for byte, logs after removing the output path).

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/structure-attributes-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/structure-attributes
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
cp -r assets $R/tree-<wp>/            # WP0: git archive HEAD assets | tar -x -C $R/tree-wp0
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>     # expected: differing=0
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
and the showcase scenario on its default seed and seeds 1 to 8 for 6,000 ticks.

## WP0: baseline

The tree of commit `84c2767d` (the plan `d1d36bbe` on `main` at `d2671045`, plus the B-88 pointer), measured into
`~/scratch/structure-attributes/world-wp0`: 420 files; the matrix reports "1160 runs: expectation failures 0 on the
seeds of record, 4 over all seeds".

## WP1: attributes as data

### Red run

Integration tests, with the 49 `attributes:` lines already in `structures.ron` but the WP0 code (which has no
`attributes` field on `StructureArchetypeDef` and does not deny unknown fields, so it ignored them silently, §3.1):

| Test | Result | First failure |
|---|---|---|
| `catalog_reference_values_tests::b88_structure_attributes_follow_starcraft` (P-1) | failed | `tesla_sentinel_dome` (Photon Cannon): 82, StarCraft II 66 |
| `catalog_reference_values_tests::the_structure_attribute_marks_exactly_the_structures` (P-2) | passed (a guard, green before and after) | — |
| `attribute_bonus_tests::a_bonus_reaches_exactly_the_structures_that_author_its_attribute` (P-5) | failed | `ancient_tree_dome` (Hatchery) authors Biological; the bonus added 0, not 7 |
| `attribute_bonus_tests::a_structure_bonus_reaches_every_structure_and_no_unit` (P-5) | passed (every structure had `Structure` before) | — |
| `attribute_bonus_tests::the_structure_strike_reaches_every_structure_and_no_unit` (P-5) | passed (likewise) | — |
| `runtime_catalog_tests::an_edited_structure_attribute_list_is_read_as_written` (P-3) | failed | the shielded structure loaded `ARMORED \| PSIONIC \| STRUCTURE` |

The nine `catalog::validation` unit tests of P-4 (`attribute_tests`) did not compile on the WP0 code: no
`attributes` field on `StructureArchetypeDef` (7 errors) and no `AttributeFlagsDef::bits` (2 errors). Logs:
`~/scratch/structure-attributes/red/`.

### Bits per faction

| Faction (structures) | Before (derived from shields) | After (authored) |
|---|---|---|
| Terran analogue (17) | Armored, Mechanical, Structure (74) | Armored, Mechanical, Structure (74) |
| Protoss analogue (15) | Armored, Psionic, Structure (82) | Armored, Structure (66) |
| Zerg analogue (17) | Armored, Mechanical, Structure (74) | Armored, Biological, Structure (70) |

### Worlds

| World | Result |
|---|---|
| Matrix (seeds of record and 16), battle 31 (100 seeds), battle 33 (64 seeds), showcase default seed and seeds 1–8 | `compare.sh world-wp0 world-wp1`: `files base=420 new=420 differing=0` |
| `golden_replay`, `golden_replay_orders` (native debug, `golden_replay_tests`) | every checkpoint reproduced; not re-recorded |
| `DEFAULT_SANDBOX_CHECKSUM` (`sim_fixtures` `fixture_tests`) | unchanged |

Every test of `sim_types`, `sim_core` and `sim_fixtures` passed on the WP1 tree (105 test executables and doc
tests). No world moved, as §7 predicts: measured worlds contain Terran-analogue structures only, whose set did not
change, and no shipped rule reads `Biological`, `Mechanical` or `Psionic` on a ground structure (§3.2).
