# Evidence: resource node footprints (B-147)

Plan: [`docs/plans/2026-11-20-resource-footprints.md`](../plans/2026-11-20-resource-footprints.md), with
Amendments 1 and 2 (the showcase's geysers in the base corners). Measured on the package branch on top of `main`
`e58f1af1`, 2026-11-20, on the shared VM (AMD Radeon RX 580, Mesa 25.0.7). Scratch material lives under
`~/scratch/resource-footprints/wp/`.

## 1. What the data says (plan §2)

| Item | 5.0.14 source | As shipped |
|---|---|---|
| Mineral field cells and ring | liberty Footprint:1061–1096 (`FootprintMineralsRounded`), rows :1083–1089 | `x` 2×1 on the anchor and the cell east of it; ring 52 cells (`placement_rules.ron`) |
| Mineral shape | Footprint:1094, `Offsets -1,-0.5;-1,0.5;1,0.5;1,-0.5` | `Box(1.0, 0.5, 0.0)` |
| Geyser cells and ring | liberty Footprint:1535–1576, void Footprint:3–21 | `x` 3×3 centred on the anchor; void-rounded ring 69 cells |
| Geyser shape | void Footprint:18–19 (16 vertices) | `Outline([...])`, the 16 vertices |
| `RemoveWhenEmpty` | liberty BehaviorData:749 (minerals); none for gas (:801–807) | `remove_when_empty: true` / `false` |
| Worker harvest range | no `Range` on the workers' harvest abilities (liberty Abil:1094–1099, 2241–2243) | contact (D-7) |

## 2. Worlds without resources (plan §6.1)

The matrix (every battle, seeds of record and 16), battle 31 (100 seeds) and battle 33 (64 seeds) are byte-identical
on every package's tree to `e58f1af1` (`world-e58f`, `world-wp1` … `world-wp6`; only output paths differ in the logs).

## 3. Income (plan §6.2)

Scratch `harvest_measure` (`~/scratch/resource-footprints/wp/harvest_measure.rs`, not committed): per field within
12 cells of a start, a fresh world with the town hall and `k` workers ordered to it; income over ticks 750 to 3,750,
per worker per minute.

| Map, start | `k` | Mean | Fields |
|---|---:|---:|---|
| `showcase_canyon` 0 | 1 | 60.31 | 60.0, 67.5, 65.0, 60.0, 55.0, 60.0, 55.0, 60.0 |
| `showcase_canyon` 1 | 1 | 60.31 | the mirror of slot 0 |
| `showcase_canyon` 0 | 2 | 60.47 | 60.0, 67.5, 66.25, 60.0, 55.0, 60.0, 55.0, 60.0 |
| `showcase_canyon` 1 | 2 | 60.47 | the mirror of slot 0 |
| `default_arena` 1 (not banded) | 1 | 58.12 | 62.5, 65.0, 65.0, 57.5, 57.5, 60.0, 52.5, 45.0 |

The band of §6.2 (mean in [59.0, 65.2], every field in [54.3, 69.3]) holds on both showcase starts for `k` = 1 and 2.
The base (`TASK_REACH`, one-cell nodes) was 91.9 (plan §6.2). `harvest_edge_tests::income_lies_in_the_band` pins
the slot-0 band.

## 4. The showcase (plan §6.3, §6.6; Amendments 1 and 2)

Base: WP4's tree (worlds byte-identical to `e58f1af1`). Nine seeds, 6,000 ticks, winner@end tick.

| Run | Default | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | Reversals |
|---|---|---|---|---|---|---|---|---|---|---:|
| Base | 1@2622 | 1@2163 | 1@2153 | 1@1941 | 1@2601 | 1@1930 | 1@1925 | 1@2150 | 1@1926 | 132 |
| Placebos 1, 2, 3 on the base | as base (seed 8: 1@1937, 1@1926, 1@1937) | | | | | | | | | 133, 132, 133 |
| WP5 and WP6 (geysers in the corners) | 1@2622 | 1@2537 | 1@2544 | 1@2180 | 1@2601 | 1@1939 | 1@2192 | 1@2530 | 1@2154 | 175 |
| Control: geysers not placed | 1@2622 | 1@2537 | 1@2544 | 1@2189 | 1@2601 | 1@1939 | 1@2192 | 1@2530 | 1@2154 | 182 |

- Every winner holds. The geysers-not-placed control is equal on eight seeds and within 9 ticks on seed 3: in the
  corners the geysers stay out of the armies' routes (Amendment 2).
- Give-ups 0, deeper unit-ticks 0, and no unit reverses on more than 3 consecutive ticks (the base's own run).
- Reversals per seed (reported, Amendment 1): base 13 14 19 15 13 15 15 15 13; WP5 13 18 25 21 13 15 31 22 17
  (175, 34 of them within 1.5 of a node). B-152 records the rise.
- Earlier layouts, for the record: the D-10 geysers ((62, 53), (1, 10)) made seed 4 a draw and flipped seed 6;
  Amendment 1's ((59, 62), (4, 1)) flipped the default seed and made seeds 1, 3 and 8 draws. Both traces and
  controls are on `resource-footprints-wp5-wip` and `resource-footprints-wp5-wip-first`.

## 5. Goldens and pins (plan §6.4)

- WP3: both goldens re-recorded for the arena's re-laid nodes; the checksum ending after the route groups (scratch
  switch) equals `e58f1af1`'s on all 20 + 60 checkpoints. `VALIDATION_GOLDEN` and `DECODE_GOLDEN` re-recorded; the
  old arena reproduces the old values.
- WP5: schema 32; both goldens re-recorded; the same entity-only digest, with the schema number held at 31, equals
  WP4's on all 80 checkpoints.
- Native debug and release and `wasm32-wasip1` release and debug agree on every re-record.
- `SHOWCASE_RESOURCE_MESH_DIGEST` (new in WP4) `0xca1fad4bb3af339a` after Amendment 2; the earlier maps reproduce the
  earlier value.

## 6. Cost (plan §6.5, D-13)

Throughput bench (`throughput` binary, `perf stat -e instructions:u`, base `e58f1af1` and candidate alternated, two
rounds): base 200.951 G, candidate 205.466 G instructions retired (+2.25 %, both rounds equal to six digits). Every
scenario stays above 25 ticks per second (lowest: the showcase, 101 ticks per second, 97 on the base, wall clock on a
loaded host).

Mesh and clearance rebuild (scratch `rebuild_measure`, instructions per rebuild from 1 and 42 rebuilds):

| Map | Nodes removed | Nodes standing | Ratio |
|---|---:|---:|---:|
| `showcase_canyon` | 17.07 M | 46.70 M | 2.74 × |
| `default_arena` | 13.21 M | 40.16 M | 3.04 × |

The arena is above the plan's 3× bar. `perf record` of its rebuilds: 68.5 % in `ConstraintGraph::kind`, the sorted
lookup of footprint unit edges made for every candidate vertex and direction; the nodes add the lattice points of
their rectangles as candidates. An incremental or indexed rebuild is out of scope (plan §13, R-4); it needs its own
amendment.

## 7. Client and storyboard (plan §8, WP7)

- `battle_input::footprint_pick_tests::a_mineral_field_is_picked_anywhere_on_its_footprint`,
  `editor::tools::resource_footprint_tests::{mirrored_minerals_mirror_the_footprint, an_overlapping_field_is_refused}`.
- Re-derived: `battle_context_order_tests` (the build site moves off the corner geyser; a blocker in front of a field
  stands on the camera's side clear of the 2×1, the pointer on its bubble), `battle_card_path_tests` (gas cell and
  researcher corner), `battle_fog_tests` (the worker walks to the line's middle).
- GPU targets reading the shipped maps pass: `runtime_battle_tests`, `editor_app_tests`, `terrain_tests`,
  `mesher_golden_tests`; `showcase_storyboard_tests` and `showcase_tests` pass.
- **Storyboard:** the default seed's beats did not move (the viewer's army's last death at 662, its workers at 1,058
  and 1,101, the hall at 2,622). The `battle` take rendered on radv (one `with-gpu-lock` hold round `xvfb-run` with
  the environment `showcase render` sets, 1,227 frames in 164 s): `advance` from 251, `clash` from 305, `march` from
  683, `assault` from 973, as before; no scene changes.
- **Picture check** (frames 120 and 260 of that take): the mineral billboards stand on their 2×1 footprints, each
  crystal over a dark 2×1 pad, and read at field size; the geysers in the base corners are off the take's framing.
  Not captured: a worker mining a back field and a field before and after it empties. No capture path orders
  harvests (the showcase issues none, plan T7), and adding one is capture tooling outside this plan.

## 8. How to reproduce

1. `CARGO_TARGET_DIR=... cargo build --release -p sim_core --example matchup_trace --example route_trace`, then
   `~/scratch/resource-footprints/scripts/measure.sh <bin dir> <tree> <out>` and `summ.sh <out>`.
2. Controls: a scratch build with the `RF_SKIP=gas|min` switch in `place_resource_footprint` (not committed), run
   with `scripts/showcase.sh`.
3. Income: copy `harvest_measure.rs` into `crates/sim_core/examples/`, build in release, run
   `harvest_measure <map> <slot> <k> 750 3000`.
4. Goldens: `wp/goldens.sh <tree> <log>`; the entity-only switch is `wp/entity_only.py` (add) and
   `wp/entity_off.py` (remove).
5. Throughput and rebuilds: `wp/bench/run.sh`.
6. Take: `showcase take --take battle` under `with-gpu-lock` and `xvfb-run` with `WGPU_BACKEND=vulkan`,
   `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json`, `MESA_VK_WSI_DEBUG=sw`, `SPACE_SCUFFLE_VIRTUAL_GPU=1`.
