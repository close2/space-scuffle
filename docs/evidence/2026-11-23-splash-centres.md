# Splash centres: measurements per package

This file records the measurements of plan [`2026-11-23-splash-centres`](../plans/2026-11-23-splash-centres.md)
(§7; B-83, B-84, B-169): the world comparison of every package against the WP0 tree, the goldens and their proof,
the red runs of the new tests, and the traced Baneling bursts. WP0 creates it; WP1 to WP5 append.

## How to reproduce

Binaries are built once per tree and run from a copy of that tree's `assets/`, so each run reads its own catalog
(`matchup_trace` embeds the catalog it was compiled with; `route_trace` is given `--assets` explicitly). The scripts
are the structure-attributes plan's, copied to `~/scratch/splash-centres/scripts/`, with one addition to
`measure.sh`: `route_trace --scenario scenarios/showcase_effects.ron` on its default seed and seeds 1 to 8 for 6,000
ticks (`fx-*`), the one shipped scenario with sieged tanks and town halls.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/splash-centres-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/splash-centres
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
the showcase scenario and the showcase effects scenario on their default seed and seeds 1 to 8 for 6,000 ticks.

## WP0: baseline

The tree of the plan and Q0096 (`e97d826a`, behaviour-identical to `main` at `e7153b8b`), measured into
`~/scratch/splash-centres/world-wp0`.
483 files; the matrix reports "1160 runs: expectation failures 0 on the seeds of record, 4 over all seeds", and its
three Baneling battles (20 seeds each) read as the plan's baseline (§7):

| Battle | Blue wins | Red wins | Both gone | Survivors | End tick (s) |
|---|---|---|---|---|---|
| 11a, 2 Banelings vs 5 marines | 0 | 9 | 11 | 0.00 – 2.25 | 7.8 |
| 11b, 2 Banelings vs a Marauder | 0 | 20 | 0 | 0.00 – 1.00 | 7.3 |
| 32, 30 marines vs 15 Banelings | 4 | 15 | 1 | 0.75 – 0.90 | 53.8 |
