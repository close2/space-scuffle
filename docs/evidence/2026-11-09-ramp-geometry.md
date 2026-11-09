# Evidence: ramp geometry in the terrain mesher

Plan: [`2026-11-09-ramp-geometry.md`](../plans/2026-11-09-ramp-geometry.md). The owner reported on 2026-11-09 that
"the top floor continues over the ramp" in the showcase video. This file records the measurements before the fix
(on `main` at `5accec2`), the red runs of the new tests, the fix's effect on the goldens, and the renders before
and after. Images live under `~/scratch/rampplan/` on the development server; none is committed.

## 1. Mesh probe before the fix

A throwaway probe (`~/scratch/rampplan/zz_ramp_probe.rs`, run as a client integration test and not committed;
the planner's version missed samples on the quad diagonal, so the barycentric test was made inclusive) built every
chunk of both shipped maps with `build_chunk_mesh` and cast 64 vertical rays per ramp cell, at the points
`(i + 0.5) / 8` of the cell, comparing the highest hit with `GameMap::terrain_altitude_at`. Log:
`~/scratch/rampplan/probe_measured.log`.

| Map | Ramp cells | Cells with samples off | Samples off per cell | Highest hit above the slope |
|---|---|---|---|---|
| `showcase_canyon` (three runs of 3) | 36 | 28 | index 1: 32 to 36 of 64; index 2: 64 of 64; index 0: 0 to 8 | +1.44 (at x = 27.44; analytic +1.5 at the cell boundary 27.5) |
| `default_arena` (one run of 4) | 16 | 14 | index 1: 32 to 36; indices 2 and 3: 64; index 0: 0 to 8 | +1.83 (analytic +1.875) |

Every sample past the middle of a ramp hits the flat floor at altitude 6.0 (cliff level 2) first; this is the
slab of plan §2.3, 1.5 cells long on a run of 3 and 2.5 cells on a run of 4, across the ramp's full width. The few
samples off in index 0 and at the lateral samples of index 1 are the guard-rail tops (+0.30) 0.0625 cells from a
side edge, not a defect; the new test T1 keeps a rail-width margin.

## 2. Red runs

`crates/client/tests/ramp_geometry_tests.rs` was committed first (`6c3fe8a`) and run against the unchanged mesher:

| Test | showcase_canyon | default_arena | fixture (T0) |
|---|---|---|---|
| T1 top surface | 22 of 36 cells off | 12 of 16 | 74 cells |
| T2 footprint exclusivity | 33 of 36 cells | 16 of 16 | 108 cells |
| T3 closure (side walls only) | green | green | green |

After the WP1 fix (`3baab60`) all ten tests were green. WP2's test commit (`9eab60d`) added recessed, half-recessed, two-levels-up and side-by-side ramps to the fixture and let T3 check every
step: 470 open steps on 32 fixture cells (flank and ramp-to-ramp half-edges), all closed by `ef26242`.

## 3. Goldens

| Golden | Before | After WP1 (`3baab60`) | After WP2 (`ef26242`) |
|---|---|---|---|
| `default_arena` mesh | `0xd02323bb3431300e` | `0x0f0d39ef7f9758db` | unchanged |
| `showcase_canyon` mesh | `0x2b6a057b898da597` | `0x771ccfc198c0befd` | unchanged |
| synthetic mesh | `0x90266ce659f753a4` | `0xe2a178698ccc7174` | `0xce785e16375bbd4c` |
| all pick hashes and hit counts | as before | unchanged | unchanged |
| ramp-free chunks (new) | `0x1d88581ead3ff398` (21 chunks), `0xdbb59ede15b09c12` (21), `0x4b31db429fac1b7d` (1) | unchanged | unchanged |

The ramp-free hash covers every chunk none of whose dual cells has a ramp corner; it was recorded on `5accec2`
before any mesher edit.

## 4. Renders of the ramp close-up

`terrain_preview` with the planner's probe pose `probe_ramp_w` (`~/scratch/rampplan/terrain_preview_probe_poses.diff`)
before the fix, and with the committed pose `ramp` (the same camera) after it, on the RX 580 (`radv`, under
`with-gpu-lock`) and on `lavapipe`:

| Still | Driver | Tree |
|---|---|---|
| `~/scratch/rampplan/evidence/before_ramp_radv.png` | radv | `5accec2` |
| `~/scratch/rampplan/evidence/before_ramp_lavapipe.png` | lavapipe | `5accec2` |
| `~/scratch/rampplan/evidence/after_ramp_radv.png` | radv | WP3 (a) |
| `~/scratch/rampplan/evidence/after_ramp_lavapipe.png` | lavapipe | WP3 (a) |
| `~/scratch/rampplan/evidence/ramp_before_after_radv.png`, `..._lavapipe.png` | both | side by side, half size |

Before, a flat sand-textured slab leaves the metal plateau at the ramp head and ends with a vertical edge halfway
down the ramp; after, the slope runs unbroken from the plateau's edge to the canyon floor. Both drivers show the
same geometry, as expected for a CPU mesh defect.

The GPU test's pixel check projects the footprint of ramp cell (27, 9), shrunk by a fifth, from the known camera
(polygon of 4,114 pixels):

| Render | Metal-plate pixels | Lit-dirt pixels |
|---|---|---|
| before, radv and lavapipe | 0 | 2,127 (51.7 %) |
| after, radv | 0 | 4,113 (99.98 %) |
| after, lavapipe | 0 | 4,114 (100 %) |

The colours separate cleanly (metal plate: 92 % of plateau pixels bright and neutral; dirt: at most 1.3 %), but
the slab carried the ramp's own dirt splat, so the plan's metal check alone passes on the pre-fix render too. The
test therefore also requires more than 90 % lit dirt, which the pre-fix render fails.

## 5. Showcase re-render

`showcase render --driver radv` (release build of `1105091`'s mesher, one `with-gpu-lock` for the one take,
1,100 frames in 143 s) wrote `~/scratch/rampplan/showcase_radv/site/showcase.mp4`; the stills were cut at 7.5 s and
9 s as in plan §2.1. The "before" stills are those of the published video (CI, `lavapipe`), cut by the planner.

| Time | Before (published, lavapipe) | After (local, radv) |
|---|---|---|
| 7.5 s | `~/scratch/rampplan/evidence/before_showcase_7.5_published.png` | `~/scratch/rampplan/evidence/after_showcase_7.5_radv.png` |
| 9 s | `~/scratch/rampplan/evidence/before_showcase_9_published.png` | `~/scratch/rampplan/evidence/after_showcase_9_radv.png` |

Side by side: `~/scratch/rampplan/evidence/showcase_7.5_before_after.png`. Before, the slab juts out of the
plateau over the west ramp and a unit on the slope is drawn under it; after, the plateau ends at the ramp's head
and the units stand on the visible slope.

Not done locally: the `lavapipe` re-render of the take. The defect and the fix are in the CPU mesh, the ramp pose
shows identical geometry on both drivers (section 4), and the published video is re-rendered on `lavapipe` by
`.github/workflows/showcase.yml` after the next green CI on `main`; the orchestrator checks that video once it is
deployed (plan §5).
