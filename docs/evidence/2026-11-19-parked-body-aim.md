# Evidence: Parked-Body Aim (B-145)

Measurements for plan [`2026-11-19-parked-body-aim`](../plans/2026-11-19-parked-body-aim.md). Each work package
appends its section. The base is `main` at `bf4252cf`; its measurement is the planner's (`~/scratch/aim-flip/base`),
taken with release tools built from that tree (`~/scratch/aim-flip/bin-base`).

## How to reproduce

The battle measurement is plan `2026-11-19-crowd-slide`'s (`~/scratch/aim-flip/scripts/measure.sh <bindir> <outdir>
[<basedir>]`, run from the repository root, serialised under `flock ~/scratch/aim-flip/measure.lock`), with release
builds of the two trace tools of `sim_core`:

```sh
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
```

It runs the matrix on 16 seeds, battle 33 on 64 seeds, battle 31 on 96 further seeds (100 with the matrix's four of
record), and the showcase to tick 3,000 on the default seed and seeds 1 to 8; `scripts/xseeds.sh` runs showcase
seeds 9 to 32 (D-3). `summary.py <run> <base>` compares two runs. The class probe is `scripts/probe.sh` on a scratch
tree with `patches/probe-tree.diff`, never committed.

## WP0: records

- `docs/plans/README.md`: the plan's row is `Active` (the plan says "In progress", which is not one of the index's
  statuses; `Active` is the index's word for it).
- `DESIGN.md`: B-145 restated per D-4 (the aim flip, 9 showcase reversals; 0 aim flips in the matrix; the walk events
  to B-103; the six Viking events of battle 35 an untraced residual); B-144 gains D-5's figures and pointer; B-103's
  walk's start retreat gains the R-F walk events (5 showcase, 7 matrix of record) with §2.5's trace.
- Plan `2026-11-09-sc2-pathfinding`: a pointer under its deviation "A3 acts only on parked bodies" (D-1.4).

## WP1: failing tests, red on `bf4252cf`

| Test | File | On `bf4252cf` |
|---|---|---|
| T1 `showcase_parked_corner_aim_does_not_flip` | `second_line_tests.rs` | fails: achieved steps of ticks 1 to 3 `(0.126, 0)`, `(−0.0656, 0.1076)`, `(−0.0656, 0.1076)`; `corner_round` set on tick 1 |
| T2 `a_rounding_begun_is_kept_when_the_body_walks` | `movement_contact_tests.rs` | fails: proposed steps `(−0.0060, −0.0050)` on tick 1 (rounding on the outside, `corner_round` set), `(0.0150, −0.0044)` on tick 2 (corner handed off by distance), a reversal |
| T3 `a_body_walking_at_first_sight_is_not_rounded` (control) | `movement_contact_tests.rs` | passes |
| M-5 `route_continues_past_a_blocker_on_its_corner_waypoint` (control) | `movement_contact_tests.rs` | passes |

T1 and T2 are committed `#[ignore]`d with the work package that fixes them, and are un-ignored by WP2.

**T2's placement.** M-5's map (`wall_with_gap(32, 16, 12..16)`, seed 7) and unit (the smallest armed ground unit,
radius 0.375). A probe from (8.5, 24.5) to (24.5, 24.5) gives the first corner (15.72, 15.77), the rounded end of
the wall above the gap, reached along the unit direction (0.637, −0.771). The unit is placed 0.3 before the corner
along that line (within one radius), the friend 0.55 beyond it (the bodies 0.85 apart; the friend 0.55 from the
corner, within both radii, 0.75). Both are ordered on the same tick, the unit to (24.5, 24.5), the friend to
(4.5, 4.5), so the friend stands in the tick-1 snapshot and walks from tick 2. A scratch sweep (unit 0.1 to 0.375
before the corner, friend 0.45 to 0.75 beyond and up to 0.3 to either side, eight friend goals) reproduced the
tick-2 reversal on base in most placements with the bodies apart; this one was taken because the friend walks
away from both the corner and the unit's goal. T3 orders the friend one tick earlier, so its tick-start velocity is
not zero when the unit first meets the corner.

**Deviation.** The plan's unit tests of `parked_on_corner` call it with the `rounding` input, which does not exist
before WP2, so they land with WP2.
