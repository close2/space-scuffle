# Retained Effect Instances: Base Figures and the CPU Work Packages (WP1 to WP5)

Plan: [`docs/plans/2026-11-19-retained-effect-instances.md`](../../../plans/2026-11-19-retained-effect-instances.md)
(B-137), WP1 to WP5 and Amendment 1. Measured on 2026-11-19 by the implementing agent on branch
`retained-decals-plan`, on the shared development server (one-minute load 20 to 255 during the day).

## Method

- **Instruction counts** (D-14 layer 2). `effect_frame` (`crates/client_diagnostics/benches/effect_frame.rs`),
  release build, pinned to CPU 41, under `perf stat -e instructions:u,task-clock`. The S stream is the frame budget's
  S arm (200 splashes a second); the P stream adds the effect mix's held set (20 areas, 4 tethers, 2 spell missiles,
  50 held unit looks) on 400 synthetic units. 600 warm-up frames at 1/60 s, then 1 200 or 2 400 measured frames.
  **Marginal instructions per frame** = (2 400-frame run − 1 200-frame run) / 1 200, without `--digest` (the digest
  hashes about 760 KB a frame and would add its own instructions). Three 2 400-frame runs per binary, alternating
  base and head: they agree to within 3 parts in 10⁷, so one comparison is decisive.
- **Frame digest.** `--digest`: FNV-1a over every measured frame's uploaded bytes (cards back to front, decals,
  lights, distortions, the darkening counters). Host-local (the host's `libm`), compared base against head on this
  host only; no digest is committed as a golden.
- **Pictures.** `decal_identity_gpu_tests` (the identity scene: every shipped look with a decal layer on a grid, two
  waves of respawns under a tightened decal budget and class caps, two lines with fogged first points, an area with a
  sweeping fog clip, a held look released at 4.5 s, a moving held look, a tether whose ends move apart, a moved
  lasting mark, a camera stepping outward), 16 captures over 9 s at 1280×720, rendered by two child processes and
  compared bit for bit. With `SPACE_SCUFFLE_DECAL_IDENTITY_SAVE` the first render's PNGs are kept; each WP's radv
  captures were compared with WP1's byte for byte (`cmp`). Kept under `~/scratch/retained-decals/`.

## Base (WP1, head `0b63d4b4`)

| Stream | Marginal instructions per frame | Digest (2 400 frames) | Live effects | Cards | Decals (last frame) |
|---|---:|---|---:|---:|---:|
| S | **10.10 M** | `071aaddfbf6f8981` | 760 | 1 781 | 3 295 |
| P | **11.89 M** | `936a7c2e7f8fcc06` | 1 028 | 2 798 | 3 353 |

The S figure matches the planner's 10.07 M (§3.2).

**Identity scene.** Both renders bit-identical on radv and on lavapipe (D-9). Counters (radv): marks retired 11,
details retired 20, decals and cards skipped by level of detail 2 875, marks refused 4, critical overflow 0.
Sheets: [radv](retained_decals_base_radv.jpg), [lavapipe](retained_decals_base_lavapipe.jpg).

### The series of record of the base (D-14 layer 3)

`frame_budget_series --arms B,S,P --pairs 6 --display-path kwin-virtual --cpus 32-47` (each child under its own
`with-gpu-lock`, RX 580, headless KWin, scenario 400), 18 runs, binaries of `0b63d4b4`:

| Run | Load (start → end) | p50 | p99 | Update p50 | CPU mean (ms/frame) |
|---|---|---:|---:|---:|---:|
| B-00 | 39.8 → 34.2 | 13.80 | 49.01 | 10.83 | 36.56 |
| S-00 | 34.2 → 39.7 | 24.62 | 86.19 | 21.47 | 57.16 |
| P-00 | 39.7 → 112.8 | 30.47 | 81.98 | 26.67 | 64.81 |
| P-01 | 112.8 → 176.2 | 36.47 | 102.39 | 32.56 | 69.72 |
| S-01 | 176.2 → 255.3 | 34.71 | 121.38 | 30.72 | 73.35 |
| B-01 | 255.3 → 235.7 | 25.25 | 142.20 | 21.89 | 53.40 |
| B-02 | 235.7 → 252.6 | 19.55 | 91.01 | 16.04 | 48.85 |
| S-02 | 252.6 → 198.2 | 24.82 | 47.37 | 21.56 | 55.56 |
| P-02 | 198.2 → 94.9 | 23.24 | 73.47 | 20.27 | 53.76 |
| P-03 | 94.9 → 45.7 | 17.56 | 94.86 | 14.94 | 45.97 |
| S-03 | 45.7 → 34.8 | 16.80 | 65.72 | 13.90 | 41.89 |
| B-03 | 34.8 → 46.8 | 16.93 | 34.53 | 13.86 | 41.36 |
| B-04 | 46.8 → 38.7 | 16.62 | 71.61 | 13.14 | 43.68 |
| S-04 | 38.7 → 30.7 | 17.06 | 47.38 | 14.46 | 41.81 |
| P-04 | 30.7 → 60.1 | 22.52 | 65.83 | 19.86 | 49.11 |
| P-05 | 60.1 → 40.5 | 20.59 | 124.22 | 18.34 | 51.42 |
| S-05 | 40.5 → 35.1 | 20.94 | 77.86 | 18.12 | 49.31 |
| B-05 | 35.1 → 152.3 | 21.07 | 53.58 | 17.30 | 50.11 |

No pair was quiet (load ≤ 16 at both runs' starts and ends), so the verdict took the CPU-time route. B's median
wall p50 was 18.24 ms, so the bar is 5 % of it, 0.91 ms:

| Arm | Pairs | Quiet pairs | Median CPU per frame over B | Bar | Verdict |
|---|---:|---:|---:|---:|---|
| S | 6 | 0 | +3.62 ms | 0.91 ms | missed |
| P | 6 | 0 | +5.17 ms | 0.91 ms | missed |

These are the first numbers of record for D-14's method, not a re-recorded baseline (that is WP7's). Note for WP7:
the process CPU per frame of B is 36 to 53 ms at a 14 to 25 ms frame interval, so it counts every thread of the
client (the render world, the task pools); its spread between B runs (±8 ms) is larger than the bar. The report is
`~/scratch/retained-decals/series-wp1/series-wp1.ron`.

## The CPU work packages

Every row compares the WP's head with the base on the same host, in the same alternating session.

| WP | Head | S instructions/frame | P instructions/frame | Digests | Identity captures (radv) |
|---|---|---:|---:|---|---|
| WP1 (base) | `0b63d4b4` | 10.10 M | 11.89 M | S `071aaddfbf6f8981`, P `936a7c2e7f8fcc06` | the base |
| WP2 exact CPU wins | `67560ea2` | 7.74 M | 9.31 M | equal to the base | 16 of 16 byte-identical |
| WP3 spawn-path indices | `67cf360a` | 7.30 M | 8.82 M | equal to the base | 16 of 16 byte-identical |
| WP4 decal table (CPU aging) | `dae18996` | 6.27 M | 7.82 M | equal to the base | 16 of 16 byte-identical |
| WP5 keyed card sort | `419ed33f` | **6.01 M** | **7.38 M** | re-recorded (ties): S `a4afd00c954b7095`, P `14a7fd038cc8d77e` | 13 of 16 differ in 66 to 359 pixels (see below) |

So far the CPU work packages remove **41 % (S) and 38 % (P)** of the build's instructions. Thread CPU per frame of
`build_frame` (secondary, load-dependent): base S 1.8 to 3.1 ms and P 2.2 to 4.1 ms; WP5 S 1.3 to 3.1 ms and
P 1.9 to 3.1 ms.

### WP5: equal card depths

The frame build's cards include pairs at exactly the same sort depth (cards of one effect at one anchor with the
same bias). The old indirect `sort_unstable_by` left their order to the sort; the keyed sort draws them in build
order. Per D-3 the `(key, index)` order is the definition from WP5 on:

- `effect_frame --frame-digests` with the old sort (a build of WP5 with the old `sort_cards`) against the new: S
  frames differ on 624 of 2 400, P on 2 400 of 2 400. Ordering the darkening cards canonically (they multiply, so
  their order draws nothing different) still leaves 624 and 2 397, so the ties are among ordinary cards.
- Identity scene: 13 of 16 captures differ from WP1's base in 66 to 359 of 921 600 pixels. The per-channel peak is
  at most 5 levels in ten of them, 8 in captures 10 and 13, and 65 in capture 5, where 34 pixels in one blob
  (x 621 to 631, y 246 to 254) show another of two equal-depth cards on top: a visible tie-order change, as D-3
  allows (independent review, round 1; the WP5 commit message's "at most 5/255" is wrong). [Sheet](retained_decals_wp5_ties_radv.jpg): capture 2 at the base, at WP5, and their difference × 16.

WP5's captures (`~/scratch/retained-decals/base-wp5/`) are the base WP6's D-10 check 3 is compared with.

## WP6: GPU aging

The decal rows are aged by `assets/shaders/effects/decal_age.wgsl` in the compute node `EffectDecalAgeNode`; the main
world writes only the rows of changed effects (plan Amendment 2).

**Instructions** (same method, WP5 and WP6 binaries alternating, load 60 to 190):

| Stream | Base (WP1) | WP5 | WP6 | WP6 against the base |
|---|---:|---:|---:|---:|
| S | 10.10 M | 6.01 M | **4.51 M** | −55 % |
| P | 11.89 M | 7.38 M | **5.88 M** | −51 % |

Runs agree to within 2 parts in 10⁷. The `--digest` of WP6 (the decals the Rust reference ages from the rows, hashed as
the frame list held them) equals WP5's on both streams, S `a4afd00c954b7095` and P `14a7fd038cc8d77e`: the D-8 time
split changes no decal of these streams.

**D-10 check 1, record parity** (`decal_age_gpu_tests`, identity scene; the GPU's aged buffer read back every frame
beside `decal_table::age_decal` of the same rows, time and camera):

| Driver | Child | Frames | Row-frames | Drawn rows | Bit-identical | Boundary flips | Outside tolerance |
|---|---|---:|---:|---:|---:|---:|---:|
| radv | gpu | 207 | 16 904 | 11 952 | 11 952 | 0 | 0 |
| radv | resync (2 + 42 full copies) | 210 | 17 021 | 12 012 | 12 012 | 0 | 0 |
| lavapipe | gpu | 203 | 16 900 | 11 948 | 11 948 | 0 | 0 |
| lavapipe | resync (41 full copies) | 203 | 16 900 | 11 948 | 11 948 | 0 | 0 |

The first port missed the tolerance on one frame. On 12 rows the intensity was 18 to 40 ulp off: radv's `/` left `life`
one ulp low, and a steep curve amplified it. The port's divisions are now correctly rounded (Amendment 2, decision 1),
and the parity is exact.

**D-10 check 2, same run.** All 16 captures of the GPU child equal the CPU-source child's (`EffectDecalSource::Cpu`)
pixel for pixel, on radv and lavapipe. All 16 captures of the resync child equal the GPU child's.

**D-10 check 3, against the base.** The head's 16 radv captures equal WP5's (`~/scratch/retained-decals/base-wp5/radv`)
byte for byte. [Sheet](retained_decals_wp6_radv.jpg): captures 2 and 9, base, head and the difference × 16 (black).

**Determinism (D-9).** `decal_identity_gpu_tests` renders the scene identically twice on radv and lavapipe. Its
level-of-detail counter now counts cards only (D-15): radv 2 171, lavapipe 2 157 (2 894 with decals at WP5).

**Lavapipe identity of WP2 to WP5 (Amendment 1's open item).** Lavapipe's head captures against its WP1 base: 13 of 16
differ, in 67 to 337 pixels, with peaks of 3 to 8 levels and 66 in capture 5. These are the same captures, of nearly the
same sizes, as radv's (66 to 359 pixels, peak 65): WP5's tie re-baseline, which is identical across drivers, with no
other change.

**Allocations.** `sync_allocation_count_tests` (valgrind): the effect stream's window allocates 0 blocks over 64 frames
(difference 0), as before. `sync_steady_state_tests` covers the frame's `aged_decals` and the table's patch list. The
render world's staged patches are a unit test (`staged_patches_never_outgrow_their_retained_vector`).

**Goldens.** The 21 `effect_slice_gpu_tests` stills pass unchanged on radv.

## Tests run per WP

- WP1: `frame_budget_tests` series statistics (8 tests), `cpu_clock` test, the identity test on radv and lavapipe,
  the lab program's crowd test.
- WP2: the seen-run lookup against `seen_clip_window` over every point of 10 000 random lines (more than 100 000
  points), the cached half tangent over 1 000 cameras × 20 points, the cached ledger equal to the recount on every
  frame of both test streams (a `cfg(test)` assertion in `cached_marks`), an ended layer never placed (counted
  `place` calls).
- WP3: every merge, fire, mark and detail choice equal to the replaced scan over both test streams at the shipped
  and a tightened budget (each kind compared at least once; 900 frames each), no index grows.
- WP4: the incremental table equal to one rebuilt from scratch after every frame of both streams at two budgets,
  rows never above an effect's granted slots, no failed row, no buffer growth; one dirty-coverage test per mutating
  API (spawn, hold, merge_line, update_held, update_held_line, move_anchor, set_fog_clip, release, retire, detail
  retirement, mark retirement, only), each mutation-checked (removing the API's mark fails its test);
  `effect_runtime_tests`, `sync_steady_state_tests` (with the table's and the indices' buffers), `weapon_look_tests`.
- WP5: the keyed sort over 2 000 random depth sets with and without ties.
- WP6: `decal_age_gpu_tests` on radv and lavapipe (checks 1 and 2, resync), with the base comparison on radv;
  `gpu_validation_tests` (`decal_age.wgsl` composes and validates, struct sizes equal their Rust twins);
  `decal_identity_gpu_tests` on both drivers; `effect_slice_gpu_tests`; the bare-picture GPU tests (status, stance,
  area and link, weapon battle); unit tests of the split time, the patch copy over missed frames, a clear, the grant
  clamp, the aligned aging, the GPU record and the staged patches; `effect_runtime_tests`, `weapon_look_tests`,
  `sync_steady_state_tests`, `sync_allocation_count_tests`; the `--decal-source` option.
