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
