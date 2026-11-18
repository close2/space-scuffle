# Evidence: Structures Meet Units by Their Footprint (B-131)

Measurements and test results of plan [`2026-11-18-structure-contact`](../plans/2026-11-18-structure-contact.md),
one section per work package.

## How to reproduce

Each tree builds its release tools into its own target directory and copies them aside, so later builds cannot
change an earlier step's binaries (charter §7: hot builds on the home disk, large output on `~/scratch`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/structure-contact
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} <bin>/
```

Then, from the repository root (`<dir>` under `~/scratch/structure-contact/`):

```bash
<bin>/matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv
<bin>/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 6000 --out <dir>/route-end
for s in 1 2 3 4 5 6 7 8; do
  <bin>/route_trace --scenario scenarios/showcase.ron --seed $s --ticks 6000 --out <dir>/route-s$s
done
diff <base>/b33/33_24_marines_vs_12_zealots/seeds.csv <dir>/b33/33_24_marines_vs_12_zealots/seeds.csv
for d in route route-end route-s1 ... route-s8; do cmp <base>/$d/units.csv <dir>/$d/units.csv; done
```

Goldens: `cargo test -p sim_core --test golden_replay_tests` in debug and with `--release`, and
`cargo test -p sim_core --test golden_replay_tests --target wasm32-wasip1 --release` under wasmtime (CI's
`determinism-wasm` command). Throughput: `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`,
the trees' bench binaries alternated, three rounds.

The base of WP1 is the plan's tree with WP0 (`f63be82f`, docs only on `a445de59`, which files the plan on `main`
`b71e4e50`).

## 0. WP0: records

B-131 restated with the fix and production exit occupancy as a fourth path; B-134 (contour footprints) and B-135
(structure `Radius`) registered in `DESIGN.md` §4. **Verified for B-135** against each analogue's `sc2_inspiration`
in `assets/catalog/structures.ron` and its `UnitData.xml` block (liberty layer; no later layer of the extraction
under `~/scratch/combat_wp3_data/` sets a `Radius` or `Footprint` on any of them, except swarmmulti's Spine Crawler
footprint): of the plan's five examples, the Barracks (1.75), Gateway (1.75), Nexus (2), Hatchery (2) and Supply
Depot (1.25) analogues all differ from half their footprint width; the Command Center analogue matches (2.5 on
5×5). The row lists every confirmed mismatch of the 49 structures. The Hatchery analogue's footprint is 5×5 against
the game's 6×5 (liberty:8210), confirmed; so are the Sensor Tower analogue (2×2 against 1×1) and the Spire and
Greater Spire analogues (3×3 against 2×2), which the row adds.

## 1. WP1: placement and exit occupancy ignore structure circles

**Change.** `validate_placement_unoccupied` skips registered structures (D-2 B3); `rebuild_production_occupancy`
leaves them out of the exit occupancy and its `max_radius` (B4). The `Occupied` doc says it never names a
structure; the `exits.rs` module doc now says the producer's footprint is waived only on the leg from the footprint
to the exit, and that structures are judged by their footprints at the terrain radius.

**Tests, red on WP0 first** (WP1's production code absent, the tests present):

| Test | WP0 | WP1 |
|---|---|---|
| S-2 `terrain_radius_tests::a_placement_beside_a_structure_uses_the_terrain_radius`, at the west face's middle | Fails: `left − T − ε` refused `Occupied(EntityId(1))`, the structure | Passes |
| S-5 `exits::tests::a_producers_circle_does_not_block_its_face_middle_exit` | Fails: `None` for the west face's middle ring cell, expected `(9.5, 11.5)` | Passes; a unit 0.9 from the cell still refuses it |
| S-9 `structure_contact_tests::no_placement_is_refused_as_occupied_by_a_structure` (64 cases × 200 points) | Fails: `(17.68, 19)` beside a 2×2 at its west face's middle refused `Occupied` by the structure (`r` 0.375, `t` 0.3125) | Passes |

**Verified (S-9's convention).** `ground_footprint_connected` with `start = end` refuses a point when it lies
strictly inside some occupied cell widened by `t` on every side (`segment_overlaps_open_box`, an open box); the union
of the widened cells of a square footprint is the footprint widened by `t`, open. So a point exactly `t` from a face
is admitted. The test states and asserts this, in raw Q32.32, both ways: every point inside is `Terrain`, every
point outside is admitted.

**Other targets** (expected unchanged, **verified**): `production_transactions_tests` (19),
`load_orders_tests` (6), `transport_ownership_tests` (15), `movement_layers_tests` (16), `checked_movement_tests`
(18), `spells_and_abilities_tests` (14), `fungal_growth_tests` (15), `status_modifier_tests` (11),
`checksum_state_tests` (9), `construction_lifecycle_tests` (15), `harvest_income_tests` (11) and the `exits` unit
tests pass unchanged. `production_transactions_tests`' pinned exits are first corner cells, which no producer's
circle reached (D-6).

**Bit-identity** (D-12, 1), WP0 tools against WP1 tools:

| What | Result |
|---|---|
| Matrix (`--compare`) | Bit-identical (1,160 runs; expectation failures 0 on the seeds of record, 4 over all seeds, as on the base) |
| Battle 33 `seeds.csv` | Byte-identical |
| Showcase `units.csv`, `summary.md` and `overlap.csv`, 1,800 ticks, to the end, seeds 1 to 8 | Byte-identical. **Verified:** the showcase has no Blink, unload or exit at a face middle (it has no production or transport orders) |
| Overlap meter (deeper unit-ticks, default seed to the end) | 0 / 0 |

The base reproduces the plan's expected values (inner-radius evidence §3): default seed player 0 wins at 2,599
(reversals 24 / 16, stall give-ups 0 / 0); seeds 2 to 5, 7 and 8 player 0 (at 3,417, 1,615, 1,727, 2,156, 1,886 and
2,559); seed 6 player 1 at 2,617; seed 1 no end within 6,000 ticks.
