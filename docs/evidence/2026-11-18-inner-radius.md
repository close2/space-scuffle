# Evidence: Terrain Radius (B-116)

Measurements and test results of plan [`2026-11-18-inner-radius`](../plans/2026-11-18-inner-radius.md), one
section per work package.

## How to reproduce

Each tree builds its release tools into its own target directory and copies them aside, so later builds cannot
change an earlier step's binaries (charter §7: hot builds on the home disk, large output on `~/scratch`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} <bin>/
```

Then, from the repository root:

```bash
<bin>/matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv
<bin>/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route
for s in 1 2 3 4 5 6 7 8; do <bin>/route_trace --scenario scenarios/showcase.ron --seed $s --out <dir>/route-s$s; done
diff <base>/b33/33_24_marines_vs_12_zealots/seeds.csv <dir>/b33/33_24_marines_vs_12_zealots/seeds.csv
diff <base>/route/units.csv <dir>/route/units.csv
```

Goldens: `cargo test -p sim_core --test golden_replay_tests` in debug and with `--release`, and
`cargo test -p sim_core --test golden_replay_tests --target wasm32-wasip1 --release` under wasmtime (CI's
`determinism-wasm` command). Throughput: `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`,
base and candidate alternated, three rounds.

The base of WP1 is the plan's commit with WP0 (`79f1926b`, docs only on `345da93f`).

## 1. WP1: the field and the loader

**Change.** `UnitArchetypeDef` reads an optional `terrain_radius` (`#[serde(default)]`); `UnitArchetype` carries
`terrain_radius: SimScalar`, the authored value or else `collision_radius`; structures get their radius. The
loader refuses an authored value that is not finite, not strictly positive after Q32.32 conversion, not strictly
below `collision_radius`, or on an `Air` mover, each naming `terrain_radius` (D-1, D-2). Nothing reads the field
yet.

**Tests, red first.** In `catalog/validation.rs`, over the shipped catalog with one line inserted:

| Test | Before the loader rules | After |
|---|---|---|
| `a_terrain_radius_is_read_and_defaults_to_the_collision_radius` | Does not compile on the base (no field); passes once the field is read | Passes |
| `a_terrain_radius_must_be_positive_and_finite` (0, −0.1, NaN, inf, 1e-30) | Fails: the catalog loads | Passes |
| `a_terrain_radius_must_be_smaller_than_the_collision_radius` (equal, larger; 1/16 below accepted) | Fails: loads | Passes |
| `an_air_unit_may_not_have_a_terrain_radius` | Fails: loads | Passes |

`runtime_catalog_tests`, `catalog_reference_values_tests` and `golden_replay_tests` pass unchanged. With no data
and no reader, WP1 is a no-op on every state; its bit-identity is measured together with WP2's (§2).

## 2. WP2: every terrain path reads the terrain radius

**Change.** `MotionPolicy` gains `terrain` from `UnitArchetype::terrain_radius`, and `terrain_radius()` returns
it; `resolve` refuses a terrain radius that is not positive or above the collision radius (D-2). Then D-5's
paths: T1 contact (already through the accessor), T2 route requests (`route_request`), T3 group lanes
(`group_leader`), T4 the Charge path (`is_walk_shorter_than`), T5 the domain (`MotionPolicy::contains`,
`clamp_into_domain`, `flee_destination`'s bounds; messages now say "terrain-radius"), T6 placement against
terrain (`validate_placement_terrain`), T7 production exits (`ExitRing::terrain_radius`: ring margin, the
product's map checks and both `egress_connected` calls; occupancy keeps the collision radius), T8 construction
site occupancy (`stands_on`), T9 the mobile producer's map bounds, T10 the overlap meter. D-6's paths keep the
collision radius.

**Tests, red on WP1 first** (WP2's production code reverted, the tests kept):

| Test | WP1 | WP2 |
|---|---|---|
| T-1 `a_body_rests_at_its_terrain_radius_against_a_wall` | Fails: the goal 0.05 beyond `t` does not fit `r`, retargeted to the cell centre | Passes: walks there, a 45° hostile push rests it at `t` |
| T-2 `a_gap_that_fits_the_terrain_disc_is_routed_through` | Fails: routes round | Passes |
| T-3 `corner_waypoints_keep_the_terrain_radius_and_margin` (offset `t + 0.05` exactly, the funnel's `CORNER_SAFETY_MARGIN`) | Fails: `r + 0.05` | Passes |
| T-4 `two_bodies_still_separate_at_their_full_radii` | Passes (a guard: unit contact is unchanged) | Passes |
| T-5 `the_map_edge_admits_the_terrain_disc` | Fails: `t + ε` refused `OutsideDomain` | Passes; air still at `r` |
| T-6 `a_placement_beside_a_structure_uses_the_terrain_radius` | Fails: `t + ε` refused `Terrain` | Passes |
| T-7 `a_body_at_its_terrain_radius_from_a_site_does_not_block_it` | Fails: the site is obstructed | Passes |
| T-8 `exits::tests::an_exit_ring_keeps_the_terrain_radius_from_the_footprint_and_the_collision_radius_from_bodies` | Fails: exit two cells off | Passes |
| T-9 `a_charge_dash_path_is_tested_at_the_terrain_radius` | Fails: no dash | Passes |
| T-10 `a_group_lane_is_searched_at_its_leaders_terrain_radius` | Fails: lane at `r`, round the wall | Passes |
| T-11 `contact::tests::a_settled_move_never_deepens_at_the_terrain_radius` | Does not compile (no `terrain` field) | Passes |
| T-12 `movement::tests::a_terrain_radius_above_the_collision_radius_is_an_invariant` | Does not compile | Passes |
| T-13 `sim_map` `navmesh_tests::a_smaller_disc_routes_wherever_a_larger_one_does` | Passes (pins existing behaviour) | Passes |

T-13 searches 500 seeded pairs per map and structure variant, each at one of the four radius pairs in turn (125
per pair), which keeps `navmesh_tests` fast (about 9 s in debug; all four pairs on every start and goal took
34 s). No counterexample.

**Finding (T-6): structures are also bodies.** A registered structure is an entity whose collision radius is half
its footprint width. Placement occupancy (`validate_placement_unoccupied`), the collision phase and steering's
obstacles meet it at the unit's collision radius (D-6's paths), so at the middle of a footprint face that circle,
not the footprint, keeps a unit at `r`. Near a face's ends the footprint decides, at `t`. T-6 therefore tests
points beside the west face far enough off its middle that the circle does not reach them. Whether unit-structure
pairs in those paths should use `t` (R3: inner radius "governs interaction with buildings") is left to the
orchestrator; the plan's D-6 keeps them at `r`.

**Fixture edits.** 26 sites in eight `sim_core` integration test files set a shipped archetype's
`collision_radius` through `catalog.get_mut` (or on a clone) to build scratch bodies: `attack_dash_tests`,
`acceleration_tests`, `checked_movement_tests`, `cluster_arrival_tests`, `group_routing_tests`,
`movement_layers_tests`, `production_transactions_tests`, `transport_ownership_tests`. Each now sets
`terrain_radius` to the same value, which is what the field meant for them before (it followed the collision
radius); without it a radius set below the archetype's authored terrain radius is a `resolve` invariant, and a
larger one would route at the old size. No assertion changed.

**Bit-identity** (D-12, 1), WP0 tools against WP2 tools, same assets:

| What | Result |
|---|---|
| Matrix (`--compare`) | Bit-identical (1,160 runs; expectation failures 0 on the seeds of record, 4 over all seeds, as on the base) |
| Battle 33 `seeds.csv` (64 seeds plus 4) | Byte-identical |
| Showcase `units.csv`, 1,800 ticks and to the end; seeds 1 to 8 | Byte-identical, and every `summary.md` (player 1 wins at 2,218; seeds 1 to 8 as the plan's base) |
| Overlap meter (deeper unit-ticks) | 0 / 0 |
| Goldens: debug and release native, `wasm32-wasip1 --release` under wasmtime | Pass unchanged |
