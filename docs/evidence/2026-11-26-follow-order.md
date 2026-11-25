# Evidence: the follow order (B-163, B-166's remaining half, B-187)

Plan: [`docs/plans/2026-11-26-follow-order.md`](../plans/2026-11-26-follow-order.md). Measured on the package branch
`follow-order-plan` on top of `main` `b4a99fae` (plan commits `a502d94a`, `cc1d0ecc`; merge `581ba63d`), from
2026-11-25, on the shared VM. Scratch material lives under `~/scratch/follow-order/` (`scripts/`, `bin-<wp>/`,
`tree-<wp>/`, `world-<wp>/`, `red/`, `golden-<label>/`, `client-<label>/`, `perf-<label>/`); the scripts are adapted
from the rallies plan's (`~/scratch/rallies/scripts/`).

## 1. WP0: records, probe and baseline

### 1.1 Records

`DESIGN.md` §4 registers B-221 (`FollowAcquireRange` not modelled), B-222 (per-unit smart resolution) and B-223 (the
Battlecruiser's slop), with provisional numbers. `main`'s register ends at B-218, and the friend-crowd branch in
flight takes B-219 and B-220, so the plan's provisional numbers stand and nothing is renumbered. B-163, B-166 and
B-187 point to this plan.

### 1.2 Worlds (`world-wp0/`)

`scripts/wp.sh wp0 581ba63d` exports the commit (WP0 changes only documents, so its code is `581ba63d`'s) to
`tree-wp0/`, builds `matchup_trace` and `route_trace` in release in a separate target directory
(`/home/spacescuffle/cargo-target/follow-order-measure`) and runs `scripts/measure.sh` (the rallies plan's: the matrix
with every battle on its seeds of record and 16 seeds, battle 31 on 100 seeds, battle 33 on 64, the showcase and
showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks): 483 files, the reference for every
later comparison by `scripts/compare.sh` (every output file byte for byte; logs after removing the output path).

### 1.3 P-0: no measured world gives a follow-type input

`scripts/probe.sh 581ba63d` patches an archive of the commit (`scripts/patch_probe.py`, never committed) so that
`step_into` prints one line when a world starts, `issue_waypoint_order` one line per rally spawn on a `Unit` waypoint
(both rows of D-9), `context_order` one line per right click on a friend that falls through to `Move` (D-10), and
`submit_attack_on_unit` one line per A-click on a unit with its attacker and walker counts (D-10). It builds the trace
binaries from that tree and runs `measure.sh` into `world-probe/`:

| Lines | Count |
|---|---:|
| `P0PROBE start` (worlds played) | 1,410 |
| `P0PROBE rallyunit` (rally spawns on a unit) | **0** |
| `P0PROBE friendmove` (right clicks on a friend falling through to `Move`) | **0** |
| `P0PROBE attackclick` (A-clicks on a unit) | **0** |
| Follow orders in the scenarios, `sim_fixtures` and the sim and client tests (`\bFollow *[{(]`) | **0** |

The probe's output files are byte-identical to `world-wp0` (483 files); the 21 differing logs differ only by the
probe's own lines, checked by removing them. So no measured world gives an input that becomes a follow, and plan §7's
predictions stand.
