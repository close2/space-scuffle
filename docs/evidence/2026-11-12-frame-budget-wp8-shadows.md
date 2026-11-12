# Frame budget at 400 units: WP8 round 3 against `main` (conditional shadow batching)

This is the measurement plan `2026-11-09-effect-visualization` Amendment 13 (item 2) asks for. WP8 review 2
([`independent_review_wp8_2.md`](effects/review/independent_review_wp8_2.md), S-2) found that round 2 put
`NoAutomaticBatching` on every bubble, which could add up to about a thousand shadow draws at 400 units. Round 3 puts
it only on bubbles whose effect shell moves their shadow caster (`client::effects::stance::casts_own_shadow`). The
orchestrator asked for one 400-unit measurement on the RX 580 at reduced scale: two runs of this build interleaved
with two of `main`'s, by the protocol of [`2026-11-12-frame-budget-two-armies.md`](2026-11-12-frame-budget-two-armies.md).

No pass or fail line applies; the figures are recorded for the review.

## Builds, host and protocol

- **This build (`wp8`):** branch `effects-wp8`, the round-3 tree (the commit that adds this file; parent `1547da7`).
- **Baseline (`main`):** `main` at `573a88c`, the branch's merge base, exported with `git archive` and built in its own
  target directory, so each tool reads its own tree's `assets/`.
- Both: `cargo build --release -p client_diagnostics --bin frame_budget --locked`. No build ran during the runs.
- **Host, GPU, display path, window, scenario:** as in the protocol of record: the shared VM's slice of CPUs 32 to 47
  (every run pinned with `taskset -c 32-47`), AMD Radeon RX 580 (RADV POLARIS10, Mesa 25.0.7), headless KWin
  (`kwin-virtual`), 1280×720 with vsync off, the `400` scenario of `crates/client_diagnostics/data/frame_budget.ron`
  (two armies of 200, 1,500 measured ticks after 180 warm-up frames).
- **Each run:** one `with-gpu-lock` command holding KWin and the tool, after 30 s of CPUs 32 to 47 under 15 % busy,
  then a 10 s gap; the order was `main`, `wp8`, `main`, `wp8`. The repeat rule of the protocol (a frame over 1 s, or
  the slice over 20 % busy during the run) applied to none. Scripts and raw reports: `~/scratch/wp8-fb/` (scratch,
  so the table below is the record).
- **Load:** the machine's one-minute load average was 54 to 141 (the VM's other user builds on CPUs 0 to 31); this
  slice was 12.5 to 18.4 % busy during the runs, the run itself included.

## Figures (ms, nearest rank over the whole 60 s window)

| Run | Build | Load (1 min) | CPUs 32–47 busy | Frames | p50 | p95 | p99 | max | Update p50 | Update p95 | Simulation p50 | Simulation p95 |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 01 | `main` | 65.23 | 18.4 % | 4,465 | 12.3 | 20.8 | 27.1 | 282.6 | 9.4 | 17.9 | 0.16 | 7.72 |
| 02 | `wp8` | 66.83 | 12.5 % | 4,503 | 12.5 | 19.7 | 22.7 | 30.7 | 9.6 | 17.4 | 0.15 | 7.38 |
| 03 | `main` | 53.93 | 12.7 % | 4,646 | 11.9 | 19.8 | 23.6 | 33.0 | 9.3 | 17.5 | 0.16 | 7.46 |
| 04 | `wp8` | 141.31 | 13.5 % | 4,572 | 12.4 | 18.9 | 22.4 | 363.4 | 9.3 | 16.5 | 0.15 | 7.40 |

Means of each build's two runs:

| Build | p50 | p95 | p99 | Update p50 | Update p95 |
|---|---:|---:|---:|---:|---:|
| `main` | 12.10 | 20.30 | 25.35 | 9.35 | 17.70 |
| `wp8` | 12.45 | 19.30 | 22.55 | 9.45 | 16.95 |

For scale: the five `400` runs of record on the same host (2026-11-12) ranged from 9.7 to 15.1 ms at p50 and from
16.9 to 29.6 ms at p95, with medians of 11.8 and 19.1 ms. Every run here reported 100 % of the live units in view, and
the first combat event at tick 97, as in the runs of record. The single long frames of runs 01 (282.6 ms) and 04
(363.4 ms) are under the protocol's 1 s repeat threshold.
