# Evidence: Crowd-Edge Steering 2 (B-103)

Measurements for plan [`2026-11-19-crowd-edge-steering-2`](../plans/2026-11-19-crowd-edge-steering-2.md), as
amended (Amendment 1: OQ-1 and OQ-2 decided yes). Each work package appends its section. Figures are counts over
the runs named; "of record" is the matrix's four seeds of record per battle (232 runs), "all seeds" its 20 seeds per
battle (1,160 runs).

## How to reproduce

All measurements use release builds of the two trace tools of `sim_core`:

```sh
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
```

The measurement (`~/scratch/crowd-edge-2/scripts/measure2.sh <bindir> <outdir> [<basedir>]`, run from the
repository root, the matrix and battle runs serialised under `flock ~/scratch/crowd-edge-2/measure.lock`) is:

```sh
matchup_trace --all-battles --seeds 16 --jobs 8 --out <out>/matrix --compare <base>/matrix/matrix.csv
matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <out>/b33
route_trace --scenario scenarios/showcase.ron --ticks 3000 --out <out>/route            # default seed
route_trace --scenario scenarios/showcase.ron --seed <s> --ticks 3000 --out <out>/route-s<s>   # s = 1 to 8
matchup_trace --battle 31_15_marauders_vs_15_stalkers --seeds 96 --no-unit-csv --out <out>/b31
```

`--seeds 16` adds 16 seeds to the four of record (20 runs per battle); `--seeds 96` gives battle 31 its 100 runs
(the four seeds of record and seeds 1 to 96). `~/scratch/crowd-edge-2/scripts/summary.py <out> <base>` prints the
totals, the battles' overall winners and flips, battle 33, the nine showcase runs and battle 31 on 100 runs. The
reproductions are `cargo test --release --locked -p sim_core --test second_line_tests --test crowd_edge_tests
--no-fail-fast -- --include-ignored --nocapture`. Reversal classes come from the scratch probe tree
`~/scratch/crowd-edge-2/probe` (`scripts/probe.sh <name> <candidate.diff>`, the candidate being the diff of
`crates/sim_core/src` against `76a7ac67`).

**Stop rule 1 for a battle decided by one seed (Amendment 1, OQ-1).** A matrix battle whose 20-run base split has
at least one decided run and differs between blue and red wins by at most one is judged on its 100 runs: the rule
holds when the 100-run winner is unchanged and the winner's share does not fall by more than the placebo band.

**The placebo band.** Three placebo builds each add one raw unit (2⁻³²) to one movement constant
(`~/scratch/crowd-edge-2/patches/patches-placebo{1,2,3}.diff`): `PUSH_RELAXATION`, `FIXED_BODY_TANGENT_WEIGHT`,
`STEERING_LOOKAHEAD`. The range of their matrix totals of record is the band; a reversal effect counts only below
its lower edge.

## WP1: the measurement method on the WP1 tree (bit-identical)

The WP1 tree (WP0 on `b0600041`) has `crates/`, `Cargo.lock`, `assets/` and `scenarios/` identical to `76a7ac67`.
Its tools were rebuilt and measured (`~/scratch/crowd-edge-2/wp1/`):

- the matrix is "bit-identical to `~/scratch/crowd-edge-2/base/matrix/matrix.csv`" (the plan's §2.1 base);
- battle 33's and battle 31's `seeds.csv` and the nine showcase runs' `units.csv` are byte-identical to base.

So the plan's §2.1 base holds: matrix of record 6,154 reversals (2,918 proposal, 3,236 contact), 2 give-ups; all
seeds 30,642 and 6; battle 33 blue 47 of 68; battle 31 6 / 7 / 7 (20 runs) and 46 / 22 / 32 (100 runs); showcase
player 1 on every seed, 235 reversals on nine seeds, default seed player 1 at tick 1,946 with 13 / 10.

**Battles judged on 100 runs.** Over the base matrix, the battles whose 20-run blue and red wins differ by at most
one are `31_15_marauders_vs_15_stalkers` (6 / 7 / 7) and five that end unresolved on all 20 runs (`22a`, `37a`,
`37b`, `38b`, `39b`, 0 / 0 / 20). Those five have no decided run, so their winner is a tie that any decided run
moves; they stay judged on 20 runs. Battle 31 is the only battle the 100-run reading covers.

**The placebo band, re-measured on the WP1 tree** (`~/scratch/crowd-edge-2/wp1-placebo{1,2,3}/`, each built from
the WP1 tree with its placebo patch; each matrix is byte-identical to the plan's §2.2 run on `76a7ac67`):

| Placebo | Battles moved | Reversals of record (proposal + contact) | All seeds | Give-ups (record / all) | Flips | Battle 31, 100 runs | Battle 33 | Showcase, nine seeds |
|---|---:|---|---:|---|---|---|---|---|
| P1 `PUSH_RELAXATION` | 9 | 6,412 (3,056 + 3,356) | 31,392 | 2 / 6 | none | 46 / 22 / 32 | 47 of 68 | 235, unchanged |
| P2 `FIXED_BODY_TANGENT_WEIGHT` | 4 | 6,017 (2,855 + 3,162) | 30,062 | 2 / 6 | none | 46 / 22 / 32 | 47 of 68 | 235, unchanged |
| P3 `STEERING_LOOKAHEAD` | 4 | 6,310 (2,999 + 3,311) | 31,088 | 2 / 6 | none | 46 / 22 / 32 | 47 of 68 | 235, unchanged |

**Band:** matrix reversals of record [6,017, 6,412] (−2.2 % to +4.2 % of base). **Battle 31 on 100 runs:** no
placebo moves it, so its band is empty; the rule of Amendment 1 then reads "blue stays the winner and its share
does not fall": blue 46 of 100 runs, and 46 of the 68 decided runs (67.6 %).
