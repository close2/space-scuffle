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

The goldens pass unchanged on WP1 in debug and release native and in `wasm32-wasip1` release under wasmtime.

## 2. WP2: collision and steering ignore structure circles (held on the showcase's flip, D-12, 4)

**Change.** `MotionEntry::foundation` (filled by `motion_entry` from the registration lookup it already made);
`resolve_collisions_and_push` pushes no `CollisionBody` for a foundation, so a structure is not in the bodies, the
grid or `largest` (B1), and `collision_body.fixed` loses its foundation term; `prepare_movement_snapshot` pushes no
`SteeringObstacle` for a foundation and leaves `largest_radius` at the largest unit radius (B2). Module and type
docs (`collision.rs` invariants, `MotionEntry::foundation`, `SteeringObstacle`) and the doc of
`movement_layers_tests::air_foundation_collision_preserves_registered_footprint_position` (D-7) updated.

**Tests, red on WP1 first** (WP1 tree with WP2's tests; the two unit tests red with only the two exclusions
reverted, since their files carry the change):

| Test | WP1 | WP2 |
|---|---|---|
| S-1 `a_unit_rests_at_its_terrain_radius_at_the_middle_of_a_structure_face` | Fails: rests 0.456 from the face (the circle at `R`, slid along it by the 45° push) | Passes: rests at `T` ± 1e-3, never below `T − 2⁻²⁰` |
| S-3 `a_structure_neither_moves_nor_pushes_a_body_outside_its_footprint_envelope` | Fails: pushed to x = 17.5 on the first tick | Passes: bit-identical for five ticks; dome and footprint unchanged |
| S-4 `a_unit_walks_straight_past_a_structure_face_inside_its_old_circle` | Fails: bent off its line (y 17.5983 at x 18.40) | Passes: within 1e-3 of y = 17.6 every tick; arrives |
| S-6 `two_units_beside_a_structure_still_separate_at_their_radii` (guard) | Passes | Passes |
| S-7 `an_immobile_unit_is_still_a_fixed_circle_body` (guard) | Passes | Passes |
| S-8 `a_body_wedged_against_a_footprint_is_not_thrown_out_and_walks_away` | Fails: thrown from x 17.90 to 17.77 on the first idle tick | Passes: stays five ticks; ordered west, its gap never falls and it walks away |
| S-10 `a_lone_body_beside_a_structure_is_never_corrected` (48 cases) | Fails: a body at (23.5, 20.5) beside a 5×5 corrected | Passes |
| `collision::tests::a_registered_structure_is_no_collision_body` | Fails: bodies hold the structure (3 ids) | Passes |
| `locomotion::tests::a_registered_structure_is_no_steering_obstacle` | Fails: obstacles hold the structure (3 ids) | Passes |

**Verified (S-8):** `spawn_unit` accepts a body 0.1 from the face (it checks no terrain), so the test spawns it
there directly; `validate_origin` admits it because its centre is on an open cell.

**Expected test edits: none** (**verified**): the whole `sim_core` suite (77 targets: lib, every integration target,
doc tests) passes on WP2 with no test changed beyond the new ones and the air-foundation doc, including
`movement_contact_tests`, `cluster_arrival_tests`, `checked_movement_tests`, `harvest_income_tests` (D-5),
`construction_lifecycle_tests`, `production_transactions_tests`, `load_orders_tests` and
`transport_ownership_tests`.

**Structure-free worlds** (D-12, 2), WP1 tools against WP2 tools:

| What | Result |
|---|---|
| Matrix (`--compare` WP1) | Bit-identical (1,160 runs; expectation failures 0 on the seeds of record, 4 over all seeds). **Verified:** the arenas spawn no structures, as the bit-identity requires |
| Battle 33 `seeds.csv` | Byte-identical |
| `golden_replay_orders` | Passes unchanged |

**`golden_replay` does not move** (D-12, 5, a finding): it passes unchanged in debug and release native, in
`wasm32-wasip1` release under wasmtime and once in `wasm32-wasip1` debug. No unit of its scenario comes within
reach of either hall's circle in a way that changes a checkpoint, so it is not re-recorded.

**Showcase** (`route_trace`, to the end of the match, at most 6,000 ticks; reversals and stall give-ups per owner,
P0 / P1; deeper unit-ticks from the overlap meter):

| Seed | WP1 | WP2 | Reversals WP1 → WP2 | Stall give-ups WP1 → WP2 | Deeper |
|---|---|---|---|---|---|
| Default | P0 at 2,599 | **P1 at 2,193** | 24 / 16 → 25 / 10 | 0 / 0 → 0 / 0 | 0 / 0 |
| 1 | no end within 6,000 | **P1 at 2,203** | 21 / 15 → 23 / 10 | 0 / 0 → 0 / 0 | 0 / 0 |
| 2 | P0 at 3,417 | **P1 at 2,187** | 15 / 35 → 22 / 12 | 0 / 0 → 0 / 0 | 0 / 0 |
| 3 | P0 at 1,615 | **P1 at 1,880** | 24 / 22 → 22 / 15 | 0 / 0 → 1 / 0 | 0 / 0 |
| 4 | P0 at 1,727 | **P1 at 5,688** | 17 / 12 → 11 / 11 | 0 / 0 → 0 / 0 | 0 / 0 |
| 5 | P0 at 2,156 | **P1 at 1,593** | 15 / 23 → 22 / 20 | 0 / 0 → 1 / 0 | 0 / 0 |
| 6 | P1 at 2,617 | P1 at 3,419 | 23 / 10 → 19 / **23** | 0 / 0 → 0 / 0 | 0 / 0 |
| 7 | P0 at 1,886 | P0 at 2,554 | 13 / 11 → 11 / 10 | 0 / 0 → 0 / 0 | 0 / 0 |
| 8 | P0 at 2,559 | **P1 at 1,716** | 16 / 24 → **40** / 14 | 0 / 0 → 0 / 0 | 0 / 0 |

The base (WP1, bit-identical to WP0) is the plan's expected reference. **The default-seed winner flips**, so the
stop rule of D-12, 4 applies and WP2 is held: no work beyond WP2 (no WP3 storyboard, no WP4 bookkeeping) is done.
On the default seed the other acceptance items hold: 0 deeper unit-ticks, stall give-ups 0 / 0 (bound base + 1),
reversals +1 / −6 (bound + 10), and the match ends. Over the other seeds, seed 6's player 1 reversals (+13) and seed
8's player 0 reversals (+24) exceed base + 10, and seeds 3 and 5 have one stall give-up each for player 0 (within
base + 1). Seed 1 now ends.

**Control runs** (D-12, 3; scratch builds of the WP2 tree with one exclusion reverted, not committed):

| Build | Default seed | `units.csv` |
|---|---|---|
| B2 reverted (structures are steering obstacles again), B1 kept | P0 at 2,599 | **Byte-identical to WP1** |
| B1 reverted (structures are collision bodies again), B2 kept | P1 at 2,193 | Identical to WP2 until tick 979 |

So **B2, the steering obstacle, moves the run**; B1 alone changes nothing on the default seed. B1 first acts at tick
979, where player 0's welder drone 3, walking east along the south face (y = 6) of its own hall (cells 6 to 10) at
its terrain radius (y = 5.6866, 0.3134 below the face), reaches x = 7.92, 0.58 off the face's middle and inside the
hall's circle (2.87 from its centre, below 2.5 + 0.375); with B1 reverted the circle pushes it 0.0034 further out
(the S1 rule at a face middle). The outcome is the same.

**Trace from the spawn** (WP1 against WP2 `units.csv`, default seed): positions are identical to tick 20. On tick 21
player 1's rhino beetle 26 (collision and terrain radius 0.5, speed 0.126), which spawned east of player 1's hall
(footprint cells 53 to 57, centre (55.5, 55.5), circle 2.5) and walks round its north side, is at (56.55, 58.53),
0.53 above the north face y = 58, heading for its corner waypoint (53.00, 58.55) (the funnel's `t + 0.05`). It is
3.21 from the hall's centre: outside the circle's contact (3.0) but within steering reach
(0.5 + 2.5 + 0.35 + 2 × 0.126 = 3.60), so on WP1 the circle bends it north (y 58.5319 → 58.5391 over ticks 21 to
29); on WP2 it keeps nearer its route (58.5317 → 58.5369). Player 1's pulse bot 23 follows from tick 28 at
(56.57, 52.59), south of the same hall, and the lane fight diverges from there.

**Classification: rule, not defect.** The cause is D-3's accepted consequence: a unit no longer steers round a
building's circle and walks its route, which the funnel keeps `t + 0.05` off the footprint's corners (S1, S6). No
body is wedged (0 deeper unit-ticks on every run), nothing stalls on the default seed, and no slide is refused at the
divergence: the rhino is in open ground 0.53 from the face. **Why the shift favours player 1** (seven of nine runs now
end for player 1, against one of nine before): the showcase spawns both armies on the south and east rings of their
own hall (a translation, not a rotation, of each other). Player 0 attacks north-east, away from its hall; player 1
attacks south-west, so its east-ring units (pulse bots 22 to 24, rhinos 25 and 26) walk round their own hall's north
and south faces, where its circle used to bend them, while player 0's march does not pass its hall's faces. The
change therefore reaches player 1's lane first (the default seed's first divergence is player 1's).

This is the third flip of the showcase's near-tied lane fight for a contact rule (contact Amendment 1, inner-radius
Amendment 1). The orchestrator decides by amendment; a proposed amendment is in the plan.
