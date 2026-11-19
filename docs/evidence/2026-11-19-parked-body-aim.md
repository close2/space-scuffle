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
