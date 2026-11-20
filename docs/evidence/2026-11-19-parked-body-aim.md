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
tick-2 reversal on base in many placements with the bodies apart; this one was taken because the friend walks
away from both the corner and the unit's goal. T3 orders the friend one tick earlier, so its tick-start velocity is
not zero when the unit first meets the corner.

**Deviation.** The plan's unit tests of `parked_on_corner` call it with the `rounding` input, which does not exist
before WP2, so they land with WP2.

## WP2: D-1, a rounding once begun is held

**Change.** `locomotion.rs`: `parked_on_corner` takes `rounding`; with it set, the standing test is dropped (the
yielding and touching tests stay). `aim_past_parked_body` passes `*sense != 0`, `drop_passed_waypoints` passes
`proposal.memory.corner_round != 0`. Docs of the three functions and of `MovementMemory::corner_round`. No constant,
no hashed field: schema 31. T1 and T2 un-ignored; three unit tests of `parked_on_corner` in `locomotion.rs`
(a walking non-yielding body on the corner is found only with `rounding`; an idle standing friend never; a body
beyond both radii never, one just within them with `rounding`).

**Control.** A scratch build with `rounding` forced false in `parked_on_corner` (never committed; tools in
`~/scratch/aim-flip/bin-implctrl`, results `~/scratch/aim-flip/implctrl`, `xs/implctrl`) reproduces `bf4252cf`
byte for byte: `matrix.csv` on 16 seeds, battle 33's and battle 31's `seeds.csv`, the nine showcase `units.csv`,
and showcase seeds 9 to 32.

**Measured** (`~/scratch/aim-flip/impl`, `xs/impl`; base in brackets). The build reproduces the planner's H prototype
(`AF=hold`) byte for byte on every one of these runs, so §4.2's figures are this code's:

| Measure | WP2 |
|---|---|
| Matrix, record / all seeds | bit-identical: 4,659 / 22,666, give-ups 2 / 5, 0 battles moved, no flip |
| Battle 33 / battle 31 (100) | identical: 39 of 68 / 49, 17, 34 |
| Showcase, nine seeds | player 1 on all nine; seed 6 ends (1,925); default seed 2,622 (1,935); 132 reversals (174), every seed at or below base; 0 give-ups; 0 deeper |
| Showcase seeds 9 to 32 | 387 reversals (421); 13 seeds lower, 9 higher; player 1 wins 21, not over 3 (17, 28, 31; base 22, 26); mean length 2,278 (2,201) |
| 33 seeds, paired (D-3) | 519 against 595; per seed −2.30 ± 4.57, t = −2.90; 21 lower, 9 higher |
| Reproductions (`second_line_tests`, `crowd_edge_tests`, `--include-ignored`) | arc, walkers, T1, CE-1 to CE-14 pass; `showcase_crowd_edges_do_not_reverse` fails as expected at 8 / 5 (bounds 0 / 3; base 8 / 9), its ignore reason restated; the corridor's two ignored tests fail with base's figures exactly (3 reversals; entities 5 and 7 standing on 87.3 % and 94.9 %) |

**Class probe** (`~/scratch/crowd-slide/probe-main.patch` applied to this tree in a scratch worktree,
`~/scratch/aim-flip/probe-out/impl.classes`, identical line for line to the prototype's `hold.classes`):

| Run | R-A | R-B | R-C | R-D | R-E | R-F |
|---|---:|---:|---:|---:|---:|---:|
| Showcase, nine seeds | 65 (42) | 22 (31) | 10 (37) | 20 (24) | 14 (26) | **1 (14)** |
| Matrix of record, battles 26 to 31 and 35 | 1,355 | 1,614 | 0 | 1,035 | 166 | 13 (as base) |
| Walkers / corridor / arc | 6 / 2 / 0 | 2 / 0 / 0 | 0 | 7 / 1 / 0 | 1 / 0 / 0 | 0 |

The showcase's one R-F event left is unit 9's walk event on seed 6 (B-103). The placebo probes' R-F figures of plan
§2.2 (14 showcase, 13 matrix under all three placebos) stand: `main` has not moved from `bf4252cf`.

**Stop rules (§4.1).** 1 winner: player 1 on the default seed, every matrix battle unchanged, battle 31 unchanged.
2 survivor ranges: bit-identical matrix, 0 misses of record. 3 give-ups: 2 / 5, 0 on every showcase seed. 4
reversals: matrix bit-identical, so read on 33 paired showcase seeds (D-3): t = −2.90 < −2, 21 lower against 9
higher. 5 guards: arc 10 / 10, walkers pass, corridor equal to base, every showcase seed reported, none of the nine
newly unfinished or flipped (seed 6 now ends), deeper 0. 6 no new constant. **All hold.**

**Goldens.** `golden_replay` re-recorded with the proof header (fixture comment): checkpoint 6 (ticks 251 to 300)
first changed, not checkpoint 5 as the plan expected. Entity 34 ("red_siege", railgun crawler) rounds entity 35,
parked on its corner; at tick 273 entity 35 starts to walk and entity 34 keeps rounding it (its hashed state differs
on that tick, its pose from tick 282). The two runs are identical again from tick 274 to 281 and from about tick 300
to 351 (checkpoint 7 unchanged) and part for good at tick 352, where entity 34 holds a rounding of entity 33. Over
the 1,000 ticks a rounding is held by a walking body on 70 unit-ticks (entity 34 on 4, entity 35 on 66; scratch
instrumentation of `parked_on_corner`, never committed). Proof: the build with `rounding` forced false reproduces
the previous 20 checksums. `golden_replay_orders` and `DEFAULT_SANDBOX_CHECKSUM` do not move. Passing builds: native
release and debug, `wasm32-wasip1` release and debug (wasmtime, `--dir` the worktree's real path).

**Throughput** (instructions retired, `perf stat -e instructions:u`, each scenario alone through a scratch
`BENCH_ONLY` switch of the bench, never committed; base built from the same tree with the source of `bf4252cf`):

| Scenario | Base (G instructions) | WP2 | Change | WP2 ticks/s (loaded host) |
|---|---:|---:|---:|---:|
| 64×64, 0 units | 1.8604 | 1.8604 | −0.00005 % | 2,359 |
| 64×64, 100 units | 6.2023 | 6.2025 | +0.004 % | 914 |
| 64×64, 400 units | 30.9056 | 30.9065 | +0.003 % | 135 |
| 128×128, 400 units | 31.2287 | 31.2298 | +0.003 % | 134 |
| 256×256, 0 units | 29.1412 | 29.1412 | +0.000003 % | 146 |
| Showcase canyon, 400 units | 53.3773 | 53.6046 | **+0.43 %** | 78 |
| 64×64, 67 casters, 400 units | 48.0654 | 48.0668 | +0.003 % | 202 |

Every scenario is within 1 % of base and above 25 ticks per second. The showcase canyon's +0.43 % is behaviour (units
keep rounding, battles take other courses), not cost: the rule adds one boolean per call. A wall-clock run on a quiet
host is an open confirmation item.

**Viking R-F event traced (D-4)**, battle 35 seed 11, tick 230, Viking entity 1 (scratch test, never committed): to
tick 227 it chases colossus 14 under an acquired `AttackTarget`, on an active surround walk (left, walked 1.28,
retreat 0.33). Colossus 14 dies at tick 228; on tick 229 the order falls back to the attack-move to (40, 48) and the
movement memory is reset, and the step `(−0.0475, 0.0546)` continues the walk's heading; on tick 230 the attack-move
steps forward `(0.0806, 0.0018)`. The probe labels it R-F because the tick-229 step carries no route tag (it comes
from neither the walk nor the chase branch), so the route's length differs between the two ticks. It is a walk that ends when its target dies, not an aim change; it
stays under B-145 as recorded, its cause now named.

## WP3: bookkeeping and the storyboard

- `DESIGN.md`: B-145 fixed for the aim flip with D-1's figures and the Viking trace; the Movement and pathfinding
  row gains "and keeps rounding it while the corner stays occupied" and `movement_contact_tests.rs` in its test
  files; B-103's showcase figures restated from WP2's probe (before D-1 in brackets): R-B 22 (31), R-D 20 (24),
  R-C 10 (37), the walk's start retreat 51 (32), walk then forward 14 (10), R-E 14 (26); default seed 8 / 5
  reversals, 132 over nine seeds.
- `docs/plans/README.md`: the plan's row `Done` with its commits.

**Storyboard** (the default seed's timeline moved: player 1 wins at 2,622, not 1,935; from `impl/route/units.csv`
the viewer's last unit dies at tick 663 (622 before), its workers at 1,059 and 1,102 (1,007 and 1,033), and three
of player 1's pulse bots survive (five before)). The `battle` take rendered on radv (RX 580, Mesa 25.0.7, one
`with-gpu-lock` hold round `xvfb-run` with the environment `showcase render` sets, 182 s, revision `1f129416`;
manifest `~/scratch/aim-flip/wp3/takes/battle.manifest.ron`): `advance` 251 to 305 (first shot), `clash` 305 to
680, past the lane fight's last death (663), `march` 683 to 973 at 4x, ending on combat at the viewer's base, and
`assault` from 973 at 2x (to about 1,373) over both workers' deaths. Every scene still covers its beat; no scene
length changed, and the storyboard's comment gains the re-check. The hall's fall (2,622) is after the take, as the
fall at 1,935 was. `showcase_storyboard_tests` and `showcase_tests` pass.

## Merge of `main` at `fb66d85e` (footprint flags)

`main`'s merge of plan `2026-11-19-footprint-flags` moved five geysers and re-recorded `golden_replay` (node cells are
hashed); the two re-records conflicted. Resolved by re-recording `golden_replay` on the merged tree, its header
covering both changes:

- **Proof.** A scratch build of the merged tree with `rounding` forced false reproduces footprint-flags WP4's 20
  checksums; `golden_replay_orders` is identical with the rule on or off. With the rule on, checkpoint 6 first
  changes again (hashed state at tick 273, entity 34's pose from tick 282; checkpoint 7 unchanged; 8 to 20 changed).
  The per-tick entity states (position, velocity, `corner_round`, route length) of both builds equal those of the
  same builds before the merge on all 1,000 ticks, so the held-rounding count (70 unit-ticks) stands.
- **Builds.** Native release and debug, `wasm32-wasip1` release and debug pass.
- **Control on the merged tree** (`~/scratch/aim-flip/main-fb66`, `mctrl`, `mimpl`): `main`'s own tools reproduce
  `bf4252cf` byte for byte (matrix, battles 31 and 33, the nine showcase seeds; the geyser move is battle-identical);
  the merged tree with `rounding` forced false reproduces `main`'s run byte for byte, and with the rule on reproduces
  WP2's run byte for byte. Every WP2 figure holds.
