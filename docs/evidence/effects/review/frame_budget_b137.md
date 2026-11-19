# B-137: the Splash Stream's Main-World Cost, Profiled

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md)
Amendment 33 (B-137). Measured on 2026-11-19 by the implementing agent on branch `effects-polish-b137` (fork of
`main` at `a2bf8290`). It follows [`frame_budget_wp14.md`](frame_budget_wp14.md), whose method, arms (B, S, P)
and machine it uses.

## Result

**The cost has no single presentation-only cause.** It is the effect runtime's per-frame rebuild of every live
effect's instances (`EffectRuntime::build_frame`), and it grows with the number of live instances, which the
shipped splash looks' lasting ground marks keep high: at 200 splashes a second about 760 effects stay live and the
runtime writes about 1 800 cards and 3 300 decals every frame. The time is spread over the whole per-instance path
(placement, layer clocks and curves, line clip windows, level of detail, card and decal writing, the sort); no
function, allocation or O(N²) loop dominates it. Following the amendment's stop rule, nothing was changed in the
runtime: this file records the profile and a proposal, and B-137 stays open. No baseline was re-recorded, because no
fix was made.

## 1. The quiet B/S/P series (step 1)

**Not taken: the machine was never quiet.** WP14's runs of record ran at a one-minute load of 4 to 14. A script
(`~/scratch/effects-polish-b137/fb/series.sh`) waited to start each of B-11, S-11, P-11, B-12, S-12, P-12 until the
load fell below 10, then below 15, with the WP14 recipe (headless KWin, CPUs 32 to 47, one `with-gpu-lock` command
per run). From 09:39 to 11:10 UTC another user's builds kept the load between 30 and 620; it fell below 15 once,
for under a minute at 10:09, and that window was lost: B-11 ran the base binary (`39d6f544`) against this branch's
assets, whose bubble uniform had grown to 192 bytes, and the renderer refused the pipeline. The script then ran a
binary built at `3d72c7f0` and never found a window again. The series stays open for a quiet machine; the profiles
below do not depend on it, because they count CPU samples, not frame times.

To take it: build `frame_budget` at the branch head (`cargo build --release -p client_diagnostics --bin
frame_budget --locked`), set `FB` in the script to it, and run the script; it prints each run's whole-window
row and leaves the reports and logs in its directory.

## 2. Profiles (step 2)

### 2.1 `perf` of the S and B arms on radv

`perf record -F 997 -g` around the release `frame_budget` (`39d6f544`, the base binary), one `with-gpu-lock` command
per run, otherwise as in WP14. The machine was loaded by another user's builds (one-minute load 450 to 620), so the
runs' frame times are not figures of record; the profiles count CPU samples, which the load does not move between
functions. Compared over the last 60 s of each record (the measuring window):

| Samples in the window | B (`perfB-01`) | S (`perfS-01`) | S − B |
|---|---:|---:|---:|
| All | 65 448 | 73 280 | +7 832 |
| `client::effects` functions | 858 | 4 693 | **+3 835** |
| Kernel | 10 530 | 14 240 | +3 710 |
| libm trigonometry (`tanf`, `atan2f`, `roundf`, …) | 44 | 478 | +434 |

The effect functions' growth, by function (self samples, S − B): `EffectRuntime::write_layers` +1 175,
`EffectRuntime::spawn` +745, `placement::place` +475, `seen_clip_window` +381, `EffectRuntime::build_frame` +357,
`reserve_marks` +176, `push_card` +169; the trigonometry is level of detail (`CameraView::pixels`) and card
rotation. A DWARF call-graph run (`perfS-02`, `perfB-02`) puts `build_effect_frame` at 5.65 % of all samples in S
and 0.90 % in B, and `spawn_effects_from_events` at 1.50 % and 0.06 %.

The kernel's growth is a second, render-side cost: about 2 800 samples of command submission
(`amdgpu_cs_ioctl` → `ttm_bo_validate` → `amdgpu_vram_mgr_new` → `__alloc_range_bias`, plus TLB shoot-downs) on the
thread that submits command buffers, not on the main world's. The driver places buffers in VRAM again at submission,
which points to pressure on the CPU-visible VRAM window the per-frame uploads use. It does not count in the
main-world update this bug is about, and on a GPU shared with another user it is not attributable to the client
alone. The per-frame GPU buffer creations seen in the whole record (`amdgpu_gem_create_ioctl`, 10 % of S's samples)
are almost all at start-up and warm-up (media and texture uploads), not in the measuring window.

### 2.2 The splash path alone, offline

To separate the runtime from the renderer and the machine's load, a throw-away bench (not committed; kept at
`~/scratch/effects-polish-b137/b137_bench.rs`) streams the S arm's events (`stress_sources`, `stress_event`, the
production `spawn_request` and `LineAssembly`) at 200 a second into the production `EffectRuntime` with the shipped
vocabulary and builds a frame every 1/60 s for the frame-budget camera, timing each phase (3 000 frames after 600 of
warm-up, release build, one pinned CPU):

| Phase, per frame | Run 1 | Run 2 |
|---|---:|---:|
| `build_frame` mean (p50, p95) | 3.36 ms (3.16, 5.27) | 3.57 ms (3.27, 5.44) |
| Spawning the frame's events | 0.18 ms | 0.20 ms |
| Live effects, cards, decals at the end | 758, 1 783, 3 290 | the same |

So the runtime alone reproduces the update cost WP14 measured (+3.6 to +4.1 ms). Its profile (`perf`, DWARF call
graphs) inside `build_frame` (91.8 % of the bench's samples): `write_layers` 71.5 % inclusive (32.1 % self, the
inlined per-instance evaluation), `seen_clip_window` 12.7 %, `place` 11.5 %, `push_card` 5.6 %, `tanf` and `atan2f`
7.6 %, the card sort 6 %, `build_frame`'s own scans (ledger, bursts, particles) 9.4 %. Per written instance that is
about 0.6 µs, spent evenly along the path.

One candidate for "redundant work" was tested and set aside: skipping, before placing any instance, a layer whose
latest placed instance is already past its span (the fire, sparks and rings of an effect kept live by its marks).
It drew exactly the same frames and saved about 6 to 10 % of `build_frame` (3.15 and 3.54 ms), within the bench's
run-to-run spread under load: not the cause.

## 3. Proposal (step 3 not taken)

The cost is the design's: every live instance is re-evaluated on the CPU every frame, and the shipped looks keep
thousands of lasting marks live. A fix is a design change for its own plan, in this order:

1. **Retained lasting marks.** A decal whose layer has finished coming in changes only by its erosion and fade,
   which are functions of time. Write it once into a retained decal buffer with its birth and fade times and let the
   decal shader age it; the CPU then writes only new and retiring marks. This removes most of the per-frame decal
   work (about 3 300 of the 5 400 instances written a frame).
2. **Skip idle layers** (the tested change above) as a cheap, exact saving once (1) is in.
3. **Per-frame constants out of the instance loop:** the camera's `tan(fov/2)` in level of detail, and a line's
   clip window computed once per line and layer instead of once per instance (`seen_clip_window` scans the line's
   seen points for each instance).
4. Then re-record the frame-budget baseline and hold the plan's §10.3 stress to +5 % (p50), as Amendment 33 step 4
   says.

The render-side VRAM validation (§2.1) is a separate observation for the GPU-readback or frame-budget owners; it
needs a measurement on an unshared GPU before any change.

Raw logs, reports and `perf` data: `~/scratch/effects-polish-b137/fb/` on the development server (not committed).
