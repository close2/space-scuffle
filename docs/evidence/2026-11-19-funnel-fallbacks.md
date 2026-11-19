# Evidence: Funnel Fallbacks (B-141)

Measurements for plan [`2026-11-19-funnel-fallbacks`](../plans/2026-11-19-funnel-fallbacks.md). Each work package
appends its section. The base is `main` at `6b922732`; its D-5 measurement is the planner's
(`~/scratch/funnel-fallbacks/base`), taken with release tools built from that tree.

## How to reproduce

The battle measurement is crowd-edge-steering-2's (`~/scratch/funnel-fallbacks/scripts/measure.sh <bindir> <outdir>
[<basedir>]`, run from the repository root, the matrix and battle runs serialised under
`flock ~/scratch/funnel-fallbacks/measure.lock`), with release builds of the two trace tools of `sim_core`:

```sh
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
```

It runs the matrix on 16 seeds, battle 33 on 64 seeds, battle 31 on 96 further seeds (100 with the matrix's four of
record), and the showcase to tick 3,000 on the default seed and seeds 1 to 8. `summary.py <run> <base>` compares two
runs. The fallback corpora use a scratch copy of the planner's probe test (`~/scratch/funnel-fallbacks/impl/
zz_b141_probe.rs`, `B141_N` layouts, `B141_FLUSH` for the flush variant), never committed.

## WP1: regression cases, red on `main`

Five deterministic cases in `crates/sim_map/tests/funnel_tests.rs`, each `#[ignore]`d with the work package that
fixes it. Run with `--ignored` on the WP1 tree, every one fails on its first assertion, a portal-midpoint fallback:

| Test | Case | On `main` |
|---|---|---|
| `an_exact_border_pinch_is_rounded_on_its_tangent` | E1 | fallback, 8 waypoints |
| `a_goal_in_a_pinch_is_reached_along_the_pinch` | E2 | fallback |
| `a_one_wide_zigzag_is_walked_down_its_middle` | E3, layout 23 rebuilt from its 12 structures | fallback |
| `an_outside_corner_is_rounded` | M1 | fallback, 34.30 long |
| `a_start_touching_a_corner_does_not_circle_it` | flush layout 1138 rebuilt from its 11 contour structures | fallback |

**Deviation.** The plan's two unit tests of WP1 (`zero_room_legs_keep_the_disc_radius`,
`gate_runs_keep_the_feature_across`) test `ZERO_ROOM_SLACK`, `GATE_RUN(r)` and the circumscribed emission, which do
not exist before WP3, so they land with WP3. The per-class diagnostic stays a scratch probe (the plan's default): no
`fallback_class` in `MeshRoute`.
