# Blueprint: Open Findings C-17, C-18 and the Phase 9 Gaps

This plan fixes the findings that the remediation plan (`docs/plans/2026-11-01-codebase-remediation.md`) and the
Phase 9 blueprint (`docs/plans/2026-11-03-phase-9-structure-and-docs.md` §9) left for a later plan:

1. **C-18 [R]**: the smog solver, `client::smog::fluid::SmogGrid3d::step`, costs about 8.2 ms of CPU per frame.
2. **C-17 [V]**: every combat effect creates its own `CombatFxMaterial`, and every live effect modifies it on
   every frame.
3. **Four Phase 9 §9 gaps**: `MapDocument::encode` panics on a malformed document; the terrain mesher skips the last
   dual column and row; the smog kinematics report 0 peer bounces above 512 bubbles; the golden replay does not pin
   two phase orders.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-03 (UTC; the file carries the
date its work starts, 2026-11-04) against `main` at `e85f79f`
(the code of `c918aa9`, where the frame budget of record was measured). The audit in §2 comes from reading the tree.
The measurements in §5 were taken on the development server while this plan was written, with throwaway probes that
are not committed: a standalone copy of `fluid.rs`, and a release build of `e85f79f` with timing lines added to the
smog systems. Line numbers drift; search for the quoted symbol. A claim about runtime behaviour that could not be
confirmed is marked **verify**, and the first test of its package confirms or refutes it.

**Out of scope:** step 8.5 (B-1, D7), now planned in `2026-11-04-phase-8-5-veo-media` (Q0001 answered); Q0014
(host-independent smog math), answered (b) and planned in `2026-11-04-libm-smog-math`, which runs after WP9 (§4);
everything listed in §8.

---

## 1. Summary

| Item | Outcome | Smog goldens | Simulation checksums | Work packages |
|---|---|---|---|---|
| C-18, smog solver cost | Three bit-exact refactors (no copies, no `floorf` call, brick sparsity), then one deliberate, documented change (velocities below the packing threshold become exactly zero) | Unchanged by WP7 and WP8; seven (and any of WP6's four new ones that change) re-recorded by WP9 in one commit that says why | None | WP6, WP7, WP8, WP9 |
| C-17, per-effect combat materials | Pooled material slots; per-effect shader data in an ECS component, written to the slot's GPU buffer in the render world. Pixels unchanged | None | None | WP3 |
| `MapDocument::encode` panics | `encode` returns `Result<MapFile, MapFormatError>`, rejecting exactly what `from_game_map` rejects | None | None | WP2 |
| Last dual column and row unmeshed | `(width + 1).div_ceil(16)` chunk columns and rows | None (mesher goldens already cover those chunks) | None | WP4 |
| `peer_bounces` is 0 above 512 bubbles | The all-pairs fallback counts its contacts | `GOLDEN_KINEMATICS_FALLBACK_600` re-recorded by WP1 | None | WP1 |
| Golden replay misses two phase swaps | A second replay fixture that fails under either swap | None | None (no simulation change; the existing fixture's checksums stay) | WP5 |
| Remediation §4 rows of Phases 0 to 4 carry no status | Audited and marked, or recorded as open | None | None | WP10 |

**Expected effect** (estimated from the measurements of §5; WP9's A/B runs measure it): at 0 units, the solver falls
from a median of 5.7 ms per frame to well under 1 ms, because an empty field is skipped brick by brick. At 400 units
it falls from 7.3 ms to an estimated 2 to 3 ms during the fight, and to the empty-field cost about 20 s after the
fight ends, instead of staying at the fight's cost for the rest of the session. C-17 removes every per-frame material
re-preparation: today each live effect gets a new uniform buffer and bind group on every frame.

---

## 2. Current state

### 2.1 C-18: what the solver does, and where the time goes

`SmogGrid3d` (`crates/client/src/smog/fluid.rs`) is a 96 × 12 × 72 = 82,944-voxel Eulerian grid. Its state is 24
flat `f32` slabs of 331,776 bytes each (7.96 MB): velocity and its scratch (6), density, glow and their scratch (4),
tint and its scratch (6), `pressure` and `divergence` (2, allocated and cleared but never read: the projection is
disabled), and vorticity x, y, z and magnitude (4). Two RGBA8 upload buffers (331,776 bytes each) go to the GPU
through `gpu_upload.rs`.

`update_smog_field_system` (`smog/mod.rs`) calls `step` once per frame with the frame's `dt` (clamped to 0.05 s).
`step` runs five stages over the whole grid:

1. `inject_sources`: per source, a box of voxels around it (cheap; scales with sources).
2. `apply_vorticity_confinement`, only when there are 32 sources or fewer: a curl pass over every interior voxel,
   then a force pass that skips voxels with vorticity magnitude below `1e-4`.
3. `advect_velocity`: semi-Lagrangian back-trace per voxel, skipping voxels whose velocity is exactly zero; then
   three `copy_from_slice` of the scratch slabs back into the fields.
4. `advect_density_and_tint`: a fast path for dormant voxels, a trilinear back-trace for the rest, and an early
   out when all eight corners are empty; then five `copy_from_slice`.
5. `pack_textures_and_compute_metrics`: one pass over all voxels writing both byte buffers and summing metrics.

Measured while planning (§5.3), in the real client on the frame-budget path, with timing lines added:

| Units | `step` p50 | p95 | inject | confinement | advect velocity | advect density | pack | Voxels with nonzero velocity | with nonzero density |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | 5.66 | 8.48 | 0.00 | 0.59 | 1.15 | 2.12 | 1.49 | 0 | 0 |
| 400 | 7.26 | 11.50 | 0.20 | 0.59 | 2.20 | 2.74 | 1.43 | 23,352 (28 %) | 1,027 (1.2 %) |

Times in ms; stage figures are means over the run. The 400 run averaged 29 smog sources (at most 74): only units
inside the 28 × 22 world-unit window around the camera feed the field. The trajectory pre-warm never ran.

Three facts follow, and the design rests on them:

- **The cost does not depend on what is in the field.** An empty grid with no source costs 5.7 ms, because every
  stage visits all 82,944 voxels and the two advection stages copy eight full slabs (2.65 MB) every frame.
- **The solver is bound by memory traffic under load, not by arithmetic.** The same code, compiled alone, takes
  2.1 ms per step on an empty grid on one idle core. With seven copies running on neighbouring cores it takes
  4.0 to 5.1 ms, close to the in-game 5.7 ms (§5.2). In the game, Bevy's other threads, the render thread and KWin share
  the memory system with it. Reducing the bytes touched per step is therefore the lever; reducing arithmetic is not.
- **The velocity field does not return to exactly zero.** Damping multiplies velocity by `exp(-0.4 dt)` each step,
  which shrinks a value but does not make it zero: decay alone takes about 220 s to reach the subnormal range, and
  can then stall at the smallest subnormal, because rounding maps `1.4e-45 × 0.993` back to `1.4e-45`. A standalone
  probe (§5.4) shows 39,089 of 82,944 voxels (30 sources) and 50,444 (240 sources) with nonzero velocity 30 s after
  the last source stopped, every one of them below `1e-4` world units per second, and none subnormal yet. The solver's sparse fast paths test for exact
  zero, so once a wake has crossed a region they stop firing there for the rest of the session. This explains the
  evidence's open observation that the 400-unit main update stays 3.4 to 3.7 ms above the 0-unit baseline after the
  fight, with 13 to 21 units left (`docs/evidence/2026-11-03-frame-budget.md`, table 1).

**What pins the output.** `crates/client/tests/smog_golden_tests.rs` hashes the solver bit for bit (class L,
`libm`-bound, Q0014): three fluid goldens that call `step` directly at `dt = 1/60`, and four system goldens (startup
with its 26 pre-warm steps, Visual Lab, swarm, live battle) that run the production `SmogPlugin` with a manual clock.
The grid is hashed by sampling density, velocity, glow and vorticity magnitude at every voxel centre, plus both byte
buffers. Two consequences for this plan:

- At a voxel centre the trilinear weights are exactly 1 and 0, so the hash reads raw field values. The **tint** slabs
  are not hashed directly: they reach the hash only through the density bytes, where density exceeds `1e-5`. A
  refactor that changed tint in empty voxels would pass the goldens and change the output later. WP8's
  differential test closes that hole.
- Vorticity enters the hash only as a magnitude (squares), so the sign of a zero in `vorticity_x/y/z` cannot be
  observed.

The client is not bound by charter §3.1 (only the simulation crates are). Smog is already frame-rate dependent: `step`
takes the frame's `dt`. Bit-exactness here is a test discipline (a refactor proves itself by unchanged goldens), not
a lockstep requirement.

### 2.2 C-17: what a combat effect costs

`vfx.rs` spawns one entity per combat event (`hitscan_tracer`, `projectile`, `beam`, `melee_slash`) and 17 per death
(`spawn_death_shatter`: a starburst and 16 shards). Each calls `Assets<CombatFxMaterial>::add` with a 32-byte
`CombatFxUniformGpu`. Then, **on every frame of its life** (0.12 to 0.75 s), the four update systems
(`update_projectiles_system`, `update_beams_system`, `update_melee_slashes_system`, `update_shatter_system`) call
`fx_materials.get_mut(&handle)` to write `uniform.age`. In Bevy 0.15 `get_mut` raises `AssetEvent::Modified`, and
`prepare_assets::<PreparedMaterial<CombatFxMaterial>>` answers it by calling `as_bind_group` again: a new uniform
buffer and a new bind group, per live effect, per frame. This is the per-frame churn C-2 and C-5 removed elsewhere.
The asset is removed when the entity despawns and its last handle drops.

So C-17's cost is not only "hundreds of creations a second" as its row says: it is one buffer and one bind group per
**live** effect per frame. The evidence measured at least 0.44 ms per frame on the main threads and frame intervals of
20.4 ms against 15.1 ms in frames with 10 to 49 combat events (most of the difference outside the main world, where
render-asset preparation runs).

Who reads the material's uniform today:

- the four update systems above (write);
- `client_diagnostics::runtime_capture::ownership` (`freeze.rs`, `timing.rs`, `geometry.rs`, `mod.rs`, `tests.rs`,
  `control.rs`): the capture tool freezes effects and checks `material.uniform.age.to_bits() ==
  timer.fraction().to_bits()`;
- tests: `combat_light_shadow_tests.rs`, `player_presentation_tests.rs`, `round2_visual_tests.rs`,
  `battle_fog_tests.rs`, `sync_steady_state_tests.rs` (the last counts `AssetEvent<CombatFxMaterial>`);
- `editor/playtest.rs` despawns effects by `With<MeshMaterial3d<CombatFxMaterial>>`, and `runtime_battle/scene.rs` by
  the effect components.

### 2.3 The Phase 9 gaps

1. **`MapDocument::encode`** (`crates/sim_map/src/map_format.rs`) is `fn encode(&self) -> MapFile`. It indexes
   `self.cells[(y * width + x)]`, so a document with `cells.len() != width * height` panics. It also hides three
   silent conversions against charter §1.2: a cliff level above 9 overflows `b'0' + level` (a level of 4 to 9 writes
   a digit that `decode` rejects); a diagonal ramp becomes `'.'` (`None | Some(_) => '.'`); flags outside the four
   authorable bits are masked away. `MapDocument::from_game_map` already rejects every one of these cases with typed
   errors (`DimensionsOutOfRange`, `LevelOutOfRange`, `UnsupportedDiagonalRamp`, `RuntimeStateInCell`,
   `InconsistentRampFlag`, `InconsistentAbyssFlag`). Callers: `editor/playtest.rs`, `editor/startup.rs`,
   `editor/ui.rs`, `editor/ui/dialogs.rs`, `client_diagnostics/src/palette_fixture.rs`,
   `client_diagnostics/src/frame_budget/fixture.rs`, and tests in `sim_map` and `client`.
2. **Terrain chunks** (`crates/client/src/terrain/mod.rs`, the spawn of step 5): `cols = width.div_ceil(16)`, `rows =
   height.div_ceil(16)`. The dual grid has `width + 1` columns (`build_chunk_mesh` clips to `width + 1`), so on a side
   that is a multiple of 16 (every shipped 64 × 64 map, the 128 × 128 frame-budget arena) the last dual column or row
   is never meshed. `crates/client/tests/terrain_tests.rs` (around line 186) computes the same wrong count and clamps
   with `.min(chunk_cols - 1)`, so it encodes the bug. The editor's dirty-chunk mapping (`max_dual_x = (… + 3).min(w)`)
   already names dual index `w`, so it becomes correct once that chunk exists.
3. **Peer bounces** (`crates/client/src/smog/kinematics.rs`, `step_bouncing_kinematics`): above
   `MAX_BOUNCING_BUBBLES` (512) it calls `resolve_contacts_all_pairs`, which discards `resolve_contact`'s `bool`, and
   returns 0. Each bubble's own counter is right. `kinematics_all_pairs_fallback_golden` hashes the step count, so the
   fix changes that one golden.
4. **Golden replay** (`crates/sim_core/tests/golden_replay_tests.rs`, `fixtures/golden_replay.ron`): swapping
   `self.update_vision(); self.acquire_targets();` in `world.rs` (`step_into`) leaves it passing, and so does swapping
   `upkeep_route` and `decide_arrival` in `MotionDraft::walk` (`movement/locomotion.rs`). Three behaviour tests catch
   each swap. CI runs the whole test file on Linux, Windows, macOS ARM64 and `wasm32-wasip1`, debug and release
   (`.github/workflows/ci.yml`: `cargo test -p sim_core --test golden_replay_tests`), so a new test in that file is
   checked on every target with no CI change.

### 2.4 Questions, and other open findings

**Owner questions.** `docs/questions/open/` holds Q0001 to Q0011, Q0013 to Q0015, Q0018 and Q0019. None has an
answer (checked 2026-11-03, 21:30 UTC). Relevant here: Q0014 (smog `libm`; default (a), so the class-L goldens stay
`libm`-bound and WP9's re-recorded hashes are class L too), Q0015 (no frame-time target; this plan measures and
publishes, it does not pass or fail a frame time), Q0018 and Q0019 (the frame-budget scenario and display path; their
defaults are used). This plan files Q0020 (§7).

**Other findings in the remediation plan's §4.** The only rows marked open are C-17, C-18 and B-1. B-1 is blocked on
the owner (Q0001). But the rows of §4.1 (S-1 to S-19), §4.2 (M-1 to M-18), §4.3 (F-1 to F-12) and §4.7 (Q-1 to Q-8,
Q-11) carry no `Fixed` mark, unlike §4.4 to §4.6, and Phases 0 to 4 have no "Exit status" paragraph. Their phases
were carried out (the plan index cites `71c4df0`, `56437da`, `3052128`, `94ca5c7`, `e469b8b`, `9111d8e`), so most are
probably fixed, but the table does not say so, and a reader cannot tell a fixed row from an open one. WP10 audits
them; any row found really open is recorded, not fixed, and goes to a later plan.

**Not included, and why:**

- The remediation plan's §6 (shell ordering 53B, presentation discontinuities 55, more smog art, networking, replay
  and bot API) was out of scope "until Phase 5 exits". Phase 5 has exited, but these are features, not findings.
- The frame-budget evidence's single slow frame per battle (43 to 72 ms, the frame of the first tick): one frame per
  battle, not a recurring cost. It is not a finding under the evidence's own threshold. WP9's runs will show whether
  it moves.
- C-7 (smog behind every unit) is proved on the swarm scene, and the live-battle screenshot shows no smog at that
  camera distance. That is a verification gap of Phase 6, not a defect, and needs art judgement. Not here.

---

## 3. Design decisions

### D-1. C-18: bit-exact refactors first, then one deliberate change

The options, with what each does to the output and the tests:

| Option | Output | Goldens | Effect (§5) | Decision |
|---|---|---|---|---|
| A. Ping-pong the scratch slabs with `std::mem::swap` instead of eight `copy_from_slice` | Bit-exact: every voxel of the scratch slab is written before the swap | Unchanged | Removes 2.65 MB of copies per step | **WP7** |
| B. Replace `floor()` by a truncating cast where the operand is clamped to `>= 0` (`trilinear_indices_and_weights`) | Bit-exact: for `x >= 0` and NaN, `x as usize == x.floor() as usize` | Unchanged | `floorf` is a `libm` call on the `x86_64` baseline: 0.4 ms per frame in the `perf` run | **WP7** |
| C. Remove the dead `pressure` and `divergence` slabs | Bit-exact | Unchanged | 0.66 MB less memory, two fewer `fill` in `clear` | **WP7** |
| D. Brick sparsity: per-brick rest flags; skip bricks whose result is provably the rest value | Bit-exact, by the rules of D-2 | Unchanged | An empty field costs almost nothing; at 400 units only bricks near wakes are visited | **WP8** |
| E. Quiescence: a voxel whose new speed is at most `1e-3` world units per second gets exactly zero velocity | Deliberate change, below the 8-bit packing threshold the solver already uses (D-3) | Seven re-recorded in one commit | The field returns to rest about 20 s after the last source, so D's skipping keeps working | **WP9** |
| F. Step at a fixed lower rate (for example 30 Hz with an accumulator) | Deliberate change: the stepping cadence and the uniform's animation time change | The four system goldens change; the fluid goldens do not (they call `step` directly) | Halves the cost at 60 frames per second, more at higher rates | Not now (trigger below) |
| G. Step on a worker thread, joined at the start of the next frame | Same values one frame later; the system goldens change because they hash after `update` | System goldens change | Takes the step off the main schedule's critical path, but keeps its memory traffic, which slows the threads it overlaps with (§5.2) | Not now (trigger below) |
| H. SIMD layout | Per-lane op order can be kept, so bit-exact is possible | Unchanged | Small: the trilinear back-trace is a gather, and the step is bound by memory, not arithmetic | Rejected |
| I. Brick-major memory layout (voxels of a brick contiguous) | Bit-exact if expressions are kept | Unchanged | Better locality, but changes `index()` and every caller and test that reads a voxel | Rejected for now; D gets most of the gain with the row-major layout |

**Order.** A, B and C are small and independent of D, so they land first (WP7) and shrink the baseline D is measured
against. D (WP8) is the large one. E (WP9) comes last because it is the only change to the output: when its goldens
are re-recorded, every earlier step has already been proved bit-exact against the original hashes.

**Characterisation before refactoring.** The seven goldens were recorded for Phase 9's structural split, and none
stops its sources, shifts the grid, or varies `dt`, which are exactly the cases where brick rest flags can go wrong.
WP6 adds four goldens recorded on the unmodified solver before any WP7 edit (D-2, "Proofs").

**Trigger for F or G.** If WP9's A/B runs show the solver above 2.0 ms mean per frame at 400 units on the
frame-budget path, WP9 records it in the evidence and the remediation plan as a remaining part of C-18, and a later
plan chooses between F and G. This plan does not implement either: both change what the system goldens see, and
both cost more design (a cadence and its interpolation, or ownership of the grid across frames) than the evidence
justifies before D and E are measured.

### D-2. Brick sparsity (WP8): exact rules

**Bricks.** 8 × 4 × 8 voxels (x, y, z), so the grid is 12 × 3 × 9 = 324 bricks, and each brick row along x is
8 contiguous voxels of one grid row. The brick of voxel `(ix, iy, iz)` is `(ix / 8, iy / 4, iz / 8)`. A new module
`crates/client/src/smog/bricks.rs` owns the brick index math and the flag sets (fixed arrays of 324 entries, no
allocation), with unit tests.

**Rest values.** Each field group has a rest value, the value the dense code writes when a voxel is dormant:

| Group | Rest value |
|---|---|
| velocity `u_x`, `u_y`, `u_z` | `+0.0` in all three (bit pattern zero) |
| scalars `density`, `glow` | `+0.0` |
| tint `tint_r`, `tint_g`, `tint_b` | `c_tail` = (0.28, 0.46, 0.68) exactly, the constant `advect_density_and_tint` writes |
| vorticity `vorticity_x/y/z`, `vorticity_mag` | `+0.0` |
| density bytes | alpha byte 0 (RGB bytes untouched, as the dense code leaves them) |
| velocity bytes | `(128, 128, 128, 0)` |

**Flags.** A flag is a promise: `rest[b] == true` means every voxel of brick `b` holds its group's rest value,
bit for bit. A flag may be `false` when the brick is in fact at rest (conservative); it may never be `true` when it is
not. Every write path either keeps the promise or clears the flag. Flags live with their slab: the velocity and
scalar groups have one set for the field and one for the scratch slab, and `std::mem::swap` swaps them together.

Additional per-brick data, filled while a pass writes:

- `max_abs_u[b]`: per axis, the largest `|u|` written into brick `b` by `advect_velocity` (0 for a skipped brick).
- `occupied[b]`: some voxel of `b` may have `!(density < 1e-6)` or `!(glow < 1e-6)` (the negated form treats a NaN
  as occupied). Here the conservative direction is `true`. Set by `advect_density_and_tint` from the values it writes
  and by injection; like the rest flags it belongs to a slab, so there is one set for the field and one for the
  scratch slab, swapped together.
- `max_vort_mag[b]`: the largest `vorticity_mag` written into `b` by the curl pass.

**Pass rules.** Inside a brick that is not skipped, the per-voxel code stays byte for byte what it is today.

1. **Injection** (`inject_source`): unchanged. After a source's box is applied, clear the velocity and scalar rest
   flags of every brick the box touches and set their `occupied` flag. (Conservative: a box that wrote nothing still
   clears flags.)
2. **Curl** (first loop of `apply_vorticity_confinement`, interior voxels `1..N-1`): a brick's curl is the rest value
   when the brick and its six face neighbours (bricks, clipped to the grid) are velocity-rest, because every
   difference is then `+0.0 - +0.0 = +0.0`. Then: if the vorticity flag is already rest, skip; else write `+0.0` to
   the brick's **interior** voxels only (boundary voxels are never written by the dense loop either) and set the flag.
   Otherwise compute the brick densely and set `max_vort_mag[b]`, and the flag to whether every written value is
   `+0.0`.
3. **Confinement force** (second loop, voxels `2..N-2`): skip a brick whose `max_vort_mag` is below `1e-4`, because
   the dense loop skips each of its voxels. A brick skipped in step 2 has `max_vort_mag = 0`. The force adds to `u`,
   so clear the velocity rest flag of every brick in which this loop runs: a brick can be velocity-rest and still
   carry vorticity computed from its neighbours' velocity.
   When there are more than 32 sources, steps 2 and 3 do not run at all, exactly as today, and the vorticity slabs
   and their flags keep their previous contents.
4. **Advect velocity**: if the source brick is velocity-rest, every voxel takes the dense fast path (`u == 0` writes
   `0.0`, corner voxels write `0.0`), so the destination brick's result is rest: skip if the scratch flag is already
   rest, else fill the brick with `+0.0` and set it. Otherwise compute densely, record `max_abs_u`, and set the scratch
   flag to whether all written values are `+0.0`. Swap slabs and flags.
5. **Advect density and tint**: a brick's result is the rest value (density `0`, glow `0`, tint `c_tail`) when no
   brick within its reach is `occupied` in the current (source) slab. Reach per axis, in voxels:
   `r_a = ceil((max_abs_u[b][a] + |buoyancy_a|) * dt * inv_cell_size[a]) + 2`, where `buoyancy` only has a y
   component. One voxel is the trilinear upper corner, one is a margin for rounding in `world_to_grid`, and
   clamping to the grid only moves a sample inward, inside the same clipped reach. In that case the dense code takes
   either the dormant fast path or the `max_corner < 1e-6` exit for every voxel of the brick, and both write the rest
   value. Skip if the destination's scalar flag is rest, else fill and set it. Otherwise compute densely and set
   `occupied` and the scalar flag from the written values. Swap.
6. **Pack**: must keep the linear voxel order, because `total_density` is a sequential `f32` sum. Iterate `iz`,
   `iy`, then the 12 x-segments of 8 voxels (a segment is the part of one grid row inside one brick). The byte flags
   are kept **per segment**, not per brick: 12 × 12 × 72 = 10,368 booleans for each buffer, a fixed array. Skip a
   segment when its brick is scalar-rest, velocity-rest and vorticity-rest and both of the segment's byte flags are
   rest. A skipped voxel would have added `+0.0` to `total_density` (no change to the bits of a sum that starts at
   `+0.0`), would not raise `peak_density`, `max_speed_sq` or `max_vorticity_sq` (strict `>` against `0`), and would
   have written the bytes it already holds. Otherwise run the dense per-voxel code for the segment and set the
   segment's two byte flags from what it wrote.
7. **`new`, `clear`, `set_bounds`, `set_bounds_quantized`**: recompute every flag from the slabs after the change
   (a full scan, rare). Note the starting tints differ: `new` and the cells exposed by `set_bounds_quantized` use
   (0.60, 0.70, 0.80), `clear` uses `c_tail`. Only the scan knows which bricks are at rest. The byte buffers start as
   density 0 (rest) and velocity `128` in all four channels, which is **not** rest (alpha is 128), so the first pack
   writes every voxel. `clear` refills them the same way, so it sets the density byte flags to rest and the velocity
   byte flags to not rest; the bounds functions touch the bytes only through `clear`, so otherwise they keep the byte
   flags.

**Byte buffers become private.** Rule 6 relies on no one else writing `rgba_density_bytes` and
`rgba_velocity_bytes`, which are `pub` today. WP8 makes them private and adds `pub fn density_bytes(&self) -> &[u8]`
and `pub fn velocity_bytes(&self) -> &[u8]`. Readers to update: `smog/mod.rs` (startup clone), `gpu_upload.rs`,
`smog_golden_tests.rs`, and `sync_steady_state_tests.rs`, which records the two `Vec`s' address and capacity. For the
last, expose `#[doc(hidden)] pub fn retained_byte_buffers(&self) -> [(&'static str, usize, usize); 2]` (name,
pointer as `usize`, capacity), the pattern of the Phase 6 blueprint's WP2. That file belongs to WP3 until WP3 merges
(D-7).

**Proofs.**

- **Original hashes.** WP7 and WP8 change no golden constant. The seven existing goldens plus WP6's four new ones
  pass unchanged after each commit.
- **Differential test** (unit test in `fluid.rs`, `#[cfg(test)]`): a `force_dense` switch, settable only under
  `cfg(test)`, makes every flag query answer "not at rest" and every reach infinite, which is the dense algorithm
  through the same per-voxel code. The test runs two grids, sparse and dense, through 600 steps of a scripted
  sequence (sources appear, move fast, stop, more than 32 then fewer, buoyancy up and down, dissipation zero and
  positive, `dt` from `1/240` to `0.07`, a quantized bounds shift and a full clear), and after every step compares
  **every slab, both byte buffers and the metrics** bit for bit (`to_bits`), tint included. This closes the hole of
  §2.1: tint is compared directly.
- **Flag invariant test**: after every step of the same script, every brick whose flag says rest really holds the
  rest value bit for bit.

### D-3. Quiescence (WP9): the one deliberate change

After computing a voxel's three new velocity components in `advect_velocity`, if
`ux * ux + uy * uy + uz * uz <= SMOG_QUIESCENT_SPEED_SQ` (a new named constant, `1e-6`, so speed at most `1e-3`
world units per second), write `+0.0` to all three.

Why this threshold, and why it is invisible:

- It is the threshold the solver already uses: `pack_textures_and_compute_metrics` writes the rest velocity bytes
  when `speed_sq <= 1e-6` (and vorticity and glow are small). The new constant replaces that literal too, so the two
  uses cannot drift apart.
- A packed velocity byte is `((v / 10 + 0.5) * 255) as u8`. Every `|v| <= 1e-3` gives byte 127, the same byte as
  `v = 0` when the voxel is not at rest, so no velocity byte changes because of the flush itself.
- A flushed velocity of at most `1e-3` moves a density back-trace by at most `1e-3 × 0.05 = 5e-5` world units, under
  0.02 % of a voxel. Density, glow and vorticity therefore change by amounts that can flip an 8-bit value at a
  truncation boundary in rare voxels, and nowhere by more than one step.
- After the last source leaves, velocity decays from about 2 to `1e-3` world units per second in
  `ln(2000) / 0.4 ≈ 19 s`, and the field is exactly zero after that. D-2's skipping then makes the empty field cheap
  again.

Goldens re-recorded by WP9, all in one commit: `GOLDEN_FLUID_SHOWCASE`, `GOLDEN_FLUID_MIXED_24`,
`GOLDEN_FLUID_CROWD_48`, `GOLDEN_SYSTEMS_STARTUP` (its 26 pre-warm steps), `GOLDEN_SYSTEMS_VISUAL_LAB`,
`GOLDEN_SYSTEMS_SWARM`, `GOLDEN_SYSTEMS_LIVE_BATTLE`, and those of WP6's four new goldens that change. Unchanged and
checked: the four kinematics goldens, the noise golden and the `libm` canary (the change adds no `libm` call, so every
class stays as it is). The commit message names C-18 and D-3, states the threshold and the argument above, and lists
every constant with its old and new value. The semantic tests in `smog_bouncing_bubbles_tests.rs` and the unit tests
in `fluid.rs` must pass unchanged; if one does not, stop and report (the change would be visible after all).

This touches the look of a visual system, so the owner is told and can object: Q0020 (§7). Its default is to make
the change.

### D-4. C-17: pooled material slots with per-effect data in the ECS

The constraint is Bevy 0.15. A `Material` instance gets its data from its own bind group. Bevy 0.15 has no
per-instance user data for materials (`MeshTag` arrived in 0.16, bindless materials too), so "one shared material
with per-instance data" is not available through the `Material` API. The options:

| Option | Per-frame cost | Visual change | Ripple | Decision |
|---|---|---|---|---|
| a. Compute `age` in the shader from `globals.time`, spawn time and lifetime; never touch the material again | No per-frame preparation; one creation and removal per effect remain | None intended, but `age` would no longer equal `Timer::fraction()` bit for bit, and the time wraps hourly | The capture tool's freeze check (`age.to_bits() == fraction().to_bits()`) loses its meaning | Rejected |
| b. Custom instanced pipeline (Bevy's `shader_instancing` example): one draw for all effects, a per-frame instance buffer | Lowest | Effects stop being sorted one by one against transparent bubbles; additive cards behind a bubble would draw over it | Large: a render command, a phase item, specialisation; the capture tool rewritten | Rejected for now |
| c. **Pooled slots**: a material asset is a reusable slot; the effect's data lives in a component and is written into the slot's existing GPU buffer in the render world | One 32-byte `write_buffer` per live effect per frame; no buffer or bind group creation in steady combat | None: the same uniform reaches the same shader at the same time | Moderate: the update systems, the capture tool's reads, and the tests | **Chosen** |

The same mechanism is already in the tree: `smog/gpu_upload.rs` writes binding 0 of a prepared material with
`RenderQueue::write_buffer` and never modifies the asset (C-2). Design:

- **`CombatFxInstance` component** (new, in `vfx`): `pub uniform: CombatFxUniformGpu`. It is the authoritative
  per-effect shader data. The spawner fills it as it fills the material today; the update systems write
  `uniform.age = timer.fraction()` through `Mut::set_if_neq` (or compare first), so a paused frame changes nothing.
  It has a component hook `on_remove` that returns the entity's slot to the pool (below).
- **`CombatFxMaterialPool` resource** (new): `free: Vec<Handle<CombatFxMaterial>>` and counters (`created`,
  `in_use`, `peak_in_use`). `acquire(&mut self, materials: &mut Assets<CombatFxMaterial>, initial:
  CombatFxUniformGpu) -> Handle<CombatFxMaterial>` pops a free slot, or adds a new asset **initialised with
  `initial`** so the first prepared buffer is already right. `release(handle)` pushes it back. The pool never removes
  assets, so steady combat raises no `Added`, `Modified` or `Removed` event once the pool has grown to the peak number
  of live effects. Its `Vec` grows only with that peak.
- **Returning slots.** The `on_remove` hook of `CombatFxInstance` reads the entity's
  `MeshMaterial3d<CombatFxMaterial>` (still present while the hook runs) and calls `release` through the
  `DeferredWorld`. Every way an effect disappears is covered: lifetime end in the update systems, the battle scene
  teardown (`runtime_battle/scene.rs`), the editor's playtest exit (`editor/playtest.rs`), app exit. A slot is never
  held by two live entities.
- **Render-world write** (new `upload_combat_fx_uniforms_system` in `ExtractSchedule`, next to smog's upload):
  `Extract<Query<(&CombatFxInstance, &MeshMaterial3d<CombatFxMaterial>), Changed<CombatFxInstance>>>`; for each, find
  the `PreparedMaterial` in `RenderAssets` and write `bytemuck::bytes_of(&instance.uniform)` into its binding-0 buffer
  (reuse smog's `uniform_buffer_of` pattern; move it to a shared helper if both need it). A slot whose prepared
  material does not exist yet is skipped: it was just created with the right initial data, and the next change is
  written on the next frame. No allocation per frame (the query is iterated in place).
- **The material asset's own `uniform`** is then only the slot's initial contents. Its doc comment says so, and no
  code may read it for the live state; readers use `CombatFxInstance`.
- **Capture tool** (`client_diagnostics::runtime_capture::ownership`): read `age` from `CombatFxInstance` instead of
  `Assets<CombatFxMaterial>`. The freeze check keeps its meaning (`instance.uniform.age.to_bits() ==
  timer.fraction().to_bits()`).

`// PERFORMANCE RATIONALE:` blocks are required on the pool (Why: per-effect creation plus per-frame re-preparation;
What: reusable slots and render-world writes; Trade-off: slots are kept at the peak count, and the asset's CPU copy
is not the live state) and on the upload system.

### D-5. The Phase 9 gaps

- **`encode`** becomes `pub fn encode(&self) -> Result<MapFile, MapFormatError>`. It rejects, before writing anything:
  `cells.len() != width * height` with a new variant `CellCountMismatch { width, height, found }`; dimensions out of
  range (`DimensionsOutOfRange`); and per cell exactly what `from_game_map` rejects, with the same variants, in the
  same order (level above 3, diagonal ramp, flag bits outside the four authorable ones, inconsistent ramp and abyss
  flags). For the flag bits, reuse `RuntimeStateInCell` if `from_game_map` uses it for the same bits (**verify**),
  else add `NonAuthorableFlags { x: u32, y: u32, bits: u32 }`. Share one private
  `fn check_cell(x, y, spec) -> Result<(), MapFormatError>` between the two, so they cannot drift. A document that
  `from_game_map` or `decode` produced always encodes, and so must every document the editor's tools produce
  (WP2 step 3 tests it, because a save that used to succeed must not start failing). Callers propagate the error: the editor shows it on its
  existing save-failure path; the diagnostics fixtures return it.
- **Chunks**: `cols = (width + 1).div_ceil(CHUNK_DUAL_CELLS)`, `rows = (height + 1).div_ceil(CHUNK_DUAL_CELLS)`.
  A chunk that holds only the last dual column has no cell range (`build_chunk_mesh` already clips both). The visible
  effect is a strip of terrain at the map's +x and +y edges that is meshed for the first time.
- **Peer bounces**: `resolve_contacts_all_pairs` returns the number of `true` results of `resolve_contact`, and
  `step_bouncing_kinematics` returns it. `GOLDEN_KINEMATICS_FALLBACK_600` is re-recorded in the fix commit, which
  says why. The positions, velocities and per-bubble counters are not changed, so only the step counts in that hash
  move.
- **Replay order**: a second fixture, played by a new test in the same file, built from the situations the six
  behaviour tests use, and proved by mutation: it must fail under each swap. No simulation change, so no checksum of
  the existing fixture changes.

### D-6. Measuring before and after

Every timing comparison in this plan is an **A/B interleaved** comparison: two release binaries, built outside the
lock, run alternately in the same session (A, B, A, B, …), so drift in the shared server's load affects both arms
alike. Arm A is always the plan's base, `e85f79f`, built in a separate worktree
(`git worktree add ~/scratch/wt-planbase e85f79f`, target `~/scratch/cargo-target/planbase`). Each run is one locked
command with KWin and the tool inside it, exactly as in `docs/evidence/2026-11-03-frame-budget.md`:

```bash
# FB = the arm's release frame_budget; W = ~/scratch/<wp>-ab/<arm>; one work dir per arm, prepared once (step 3 of
# the Phase 6 blueprint's WP6, on lavapipe, without the lock). NN = run number, S = 0 or 400.
WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json \
with-gpu-lock taskset -c 32-47 kwin_wayland --virtual --xwayland --no-lockscreen --no-global-shortcuts \
  --width 1280 --height 720 --socket ab-NN \
  --exit-with-session "$FB --display-path kwin-virtual --work-dir $W/work --scenario S --out $W/run-NN.ron"
```

KWin exits 0 even when the session fails: a run counts only if `run-NN.ron` exists and the log has no
`frame_budget failed`. Before each run record `date -u` and `/proc/loadavg`; a run started above a one-minute load
of 8 is marked loaded and repeated at the end. Nothing else builds on CPUs 32 to 47 during the runs. About 70 s of
lock per run, released between runs.

The `perf` attribution run puts `perf record -F 997 -o $W/perf-NN.data --` in front of `$FB` inside the session
string, and converts a symbol's samples to ms per frame as samples ÷ 997 ÷ rendered frames (measured plus warm-up).

The solver micro-benchmark (WP6) is the fast, GPU-free measure used inside the C-18 chain; the A/B frame-budget runs
are the measure of record.

### D-7. File ownership and order

| File | Owner, in order |
|---|---|
| `crates/client/src/smog/kinematics.rs` | WP1 |
| `crates/client/tests/smog_golden_tests.rs` | WP1, then WP6, WP8 (accessor rename only), WP9; then plan `2026-11-04-libm-smog-math` |
| `crates/client/src/smog/fluid.rs`, new `smog/bricks.rs` | WP7, then WP8, then WP9; then plan `2026-11-04-libm-smog-math` (call sites only) |
| `crates/client/src/smog/mod.rs`, `smog/gpu_upload.rs` | WP8 (byte accessors only) |
| new `crates/client/benches/smog_solver.rs`, `crates/client/Cargo.toml` (`[[bench]]` entry) | WP6 |
| `crates/client/src/vfx.rs` (may become `vfx/mod.rs` plus submodules), `crates/client/src/lib.rs`, `crates/client/src/editor/app.rs` (registration only) | WP3 |
| `crates/client_diagnostics/src/runtime_capture/ownership/*` | WP3 |
| `crates/client/tests/sync_steady_state_tests.rs` | WP3, then WP8 (two lines, after WP3 merged) |
| `combat_light_shadow_tests.rs`, `player_presentation_tests.rs`, `round2_visual_tests.rs`, `battle_fog_tests.rs` | WP3 |
| `crates/sim_map/src/map_format.rs`, `crates/sim_map/tests/map_format_tests.rs`, `validation_golden_tests.rs` (call site) | WP2 |
| `crates/client/src/editor/{playtest.rs, startup.rs, ui.rs, ui/dialogs.rs}`, `crates/client/tests/editor_app_tests.rs` (call sites), `crates/client/tests/editor_core_tests.rs` (one test) | WP2 |
| `crates/client_diagnostics/src/{palette_fixture.rs, frame_budget/fixture.rs}` | WP2 |
| `crates/client/src/terrain/mod.rs`, `crates/client/tests/terrain_tests.rs` | WP4 |
| `crates/sim_core/tests/golden_replay_tests.rs`, new `crates/sim_core/tests/fixtures/golden_replay_orders.ron` | WP5 |
| `docs/plans/2026-11-01-codebase-remediation.md` §4.1, §4.2, §4.3, §4.7 and Phases 0 to 4 | WP10 |
| `docs/plans/2026-11-01-codebase-remediation.md` §4.4 (C-17, C-18), Phase 6 and 9 notes; `DESIGN.md`; Phase 9 blueprint §9; `docs/plans/README.md` (this plan's row) | WP11 |
| new evidence files `docs/evidence/2026-11-*-c17-*.md`, `…-c18-*.md` | WP3 (C-17), WP6 and WP9 (C-18) |
| `docs/questions/open/0020-smog-velocity-quiescence.md` | this plan (filed); WP9 reads it before its re-record commit |

---

## 4. Work packages

| WP | Title | Closes | Size | Depends on | Group |
|---|---|---|---|---|---|
| WP1 | Smog kinematics: count all-pairs contacts | Phase 9 gap 3 | S (2 to 3 h) | none | A1 |
| WP2 | `MapDocument::encode` returns typed errors | Phase 9 gap 1 | S-M (half a day to a day) | none | A1 |
| WP3 | Pooled combat-effect material slots | C-17 | M-L (2 to 3 days) | none | A1 |
| WP4 | Terrain chunks cover the last dual column and row | Phase 9 gap 2 | S (2 to 4 h) | none | A2 |
| WP5 | Golden replay pins vision-before-acquisition and upkeep-before-arrival | Phase 9 gap 4 | M (1 to 2 days) | none | A2 |
| WP6 | C-18 baseline: characterisation goldens, solver benchmark, attribution | C-18 (proof base) | M (1 day) | WP1 | B |
| WP7 | C-18 bit-exact I: ping-pong slabs, truncating cast, dead slabs | C-18 (part) | S (half a day) | WP6 | B |
| WP8 | C-18 bit-exact II: brick sparsity | C-18 (part) | L (3 to 4 days) | WP7; WP3 merged before its last commit | B |
| WP9 | C-18 deliberate: quiescence; goldens re-recorded; A/B measurement | C-18 | M (1 day, plus about 20 min of locked GPU) | WP8, WP3 | B |

**Followed by** (amended 2026-11-03 after Q0014's answer): plan
[`2026-11-04-libm-smog-math`](2026-11-04-libm-smog-math.md) (Q0014 option b) starts only after WP9 is merged (or
WP8, if Q0020 is answered "no"), and never runs beside WP6 to WP9, because it owns the same two files. It re-records
the smog goldens a second time, in its own commit, as class H. Nothing in WP6 to WP9 changes for it: their goldens
stay class L, and their commits are proved against the server's glibc as planned.
| WP10 | Audit of the remediation rows of Phases 0 to 4 | §2.4 | M (1 day, documentation) | none | A2 |
| WP11 | Records: `DESIGN.md`, remediation plan, Phase 9 §9, plan index | all | S (2 to 3 h) | all others, CI green on `main` | C |

**Parallelism (at most three gates at once; 16 cores, CPUs 32 to 47).** Group A1 starts together: WP1, WP2, WP3. As a
slot frees, start in this order: WP6 (needs WP1 merged), WP4, WP5, WP10. Group B is a chain, one gate at a time:
WP6 → WP7 → WP8 → WP9. WP3 is the longest of group A and must be merged before WP8's last commit (the two lines of
`sync_steady_state_tests.rs`) and before WP9's measurement (which measures both fixes together against the base).
Group C (WP11) runs last, when CI on `main` is green with everything merged. WP10 and WP11 change documentation only.

Each package works in its own worktree with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/open-wpN`,
checks `df -h / ~/scratch` first, keeps logs under `~/scratch/logs/open_wpN_*`, and removes its target directory after
merge (charter §7). Common gate before each commit:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/open-wpN
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked    # plain: the GPU harness takes the lock per child; never wrap it
```

Documentation-only packages (WP10, WP11) use `cargo fmt --all -- --check` and
`cargo test --locked -p workspace_checks` (C17 to C20 check their links, evidence and index).

Rules for every package: failing test first, with the red run pasted into the agent's report (for a pure refactor,
a mutation red run instead, as each WP says). Each commit names its finding (C-17, C-18, or "Phase 9 §9 gap N") and
its WP. Every new Rust file begins with `//!` (responsibility, fit, invariants). No `unwrap`; `expect` only with an
`invariant:` message. No catalog identity in Rust. No function over 100 lines outside tests (C8). Every divergence
from idiomatic code for speed carries a `// PERFORMANCE RATIONALE:` block with Why, What and Trade-off (C11). Never
run a long command in the foreground: background it and read its log every few minutes. Check
`docs/questions/open/` before each push.

---

### WP1: Smog kinematics: count all-pairs contacts

**Files (owned):** `crates/client/src/smog/kinematics.rs`; `crates/client/tests/smog_golden_tests.rs` (one constant
and its doc comment).

**Steps**

1. **Failing test first** (unit test in `kinematics.rs`): `all_pairs_fallback_reports_its_contacts`. Build
   `MAX_BOUNCING_BUBBLES + 2` bubbles on a sparse lattice with no contact, then place two of them overlapping and
   approaching (as `test_sphere_sphere_elastic_collision` does). Step once. Assert the returned peer count equals the
   sum of the per-bubble `peer_bounces` increases divided by 2, and is 1. Red on `main`: it returns 0.
2. Make `resolve_contacts_all_pairs` return `u64`: the count of `resolve_contact` calls that returned `true`.
   Return it from `step_bouncing_kinematics`. Delete the comment that explains the old behaviour.
3. Run `kinematics_all_pairs_fallback_golden`: it fails. Record the new value of `GOLDEN_KINEMATICS_FALLBACK_600`.
   Check that the other three kinematics goldens pass unchanged. Commit the fix and the new constant together; the
   message says the hash changed only because the step's peer count is now hashed as the real count (Phase 9 §9 gap
   3), and gives the old and new values.

**Gate.** Common gate.

---

### WP2: `MapDocument::encode` returns typed errors

**Files (owned):** see D-7.

**Steps**

1. **Failing test first** (`crates/sim_map/tests/map_format_tests.rs`):
   `encode_of_a_document_with_too_few_cells_does_not_panic`: a valid document with one cell popped, encoded inside
   `std::panic::catch_unwind`; assert no panic. Red on `main` (index out of bounds). Also add, red on `main`:
   `encode_rejects_a_diagonal_ramp` and `encode_rejects_a_level_above_three` (today they silently write `'.'` and a
   digit `decode` rejects); write them against today's API by checking that `encode` followed by `decode` returns the
   document unchanged, which fails.
2. Add `MapFormatError::CellCountMismatch { width: u32, height: u32, found: usize }` with a `///` doc and an `#[error]`
   message. Extract `fn check_cell(x, y, spec: &MapCellSpec) -> Result<(), MapFormatError>` from `from_game_map`'s
   per-cell checks, adapted to `MapCellSpec` (level, ramp, flags); use it in both. Change `encode` to return
   `Result<MapFile, MapFormatError>`; it checks dimensions, the cell count, then every cell, before building rows.
   Remove the `None | Some(_)` arm (diagonals are rejected before) and the `.unwrap_or('0')` (bits are checked).
3. Convert the step-1 tests to assert the exact error variants. Add `encode_accepts_every_document_decode_produces`:
   for every map in `assets/maps/` and the decode corpus of `validation_golden_tests.rs`, `decode` then `encode`
   succeeds and round-trips. In the client, add `every_editor_tool_leaves_an_encodable_document` (with the editor
   core tests): apply each paint tool, the eraser, ramp placement and symmetric painting to a fixture document, and
   `encode` must succeed after each. If a tool can produce a cell `check_cell` rejects, that is a finding: stop and
   report it rather than weakening `check_cell`.
4. Update every caller (D-7). The editor maps the error onto its existing save-failure path (**verify** where
   `save_atomic` errors are shown; the same place shows this one); `editor_app_tests.rs` keeps passing. The
   diagnostics fixtures propagate with `?`.

**Checksums and goldens.** None: the map format is not simulation state.

**Gate.** Common gate.

---

### WP3: Pooled combat-effect material slots (C-17)

**Files (owned):** see D-7.

**Steps**

1. **Failing tests first.**
   - In `sync_steady_state_tests.rs`, add `combat_frames_reuse_effect_materials`: the default battle (`Harness::battle()`),
     resumed into the fight. Warm until combat events have flowed for 120 frames, then reset the tallies and run 240
     frames. Assert: at least 100 combat events in the window (the window is real; **verify** the default battle's
     event rate and lower the bound to what it sustains, never below 30); `CombatFxMaterial` `Modified` is 0;
     `Removed` is 0; `Added` is at most the growth of the pool's `created` counter over the window, and strictly less
     than the number of effect entities spawned in the window (proof of reuse). Red on `main` on `Modified` (every
     live effect, every frame) and on `Removed`. Count spawned effects with a census of `CombatLightEffect` entities
     added (an `Added<CombatLightEffect>` counting system in the test's observation plugin).
   - Unit tests in `vfx` (pool): `acquire_reuses_a_released_slot`; `a_slot_is_never_held_by_two_live_effects` (a
     scripted sequence of 500 spawns and despawns in a headless `App`, asserting the live handles are distinct after
     every frame); `despawning_by_any_path_returns_the_slot` (lifetime end, `despawn_recursive` from the scene teardown
     query, and app teardown).
   - `paused_battle_frames_create_modify_and_remove_nothing` must keep passing with live effects on screen when paused
     (`set_if_neq`).
2. Implement D-4: `CombatFxInstance` with its `on_remove` hook, `CombatFxMaterialPool`, the spawner and update systems
   writing the component, and `upload_combat_fx_uniforms_system` in the render app's `ExtractSchedule`. Register them
   in `lib.rs` and in the editor's playtest app (`editor/app.rs`), wherever `MaterialPlugin::<CombatFxMaterial>` is
   added today.
3. **GPU test** (through `crates/client/tests/support/gpu_run.rs`, as `smog_bouncing_bubbles_tests.rs` does):
   `a_reused_slot_draws_its_new_effect`. A camera, a black background and one additive effect card of the first owner
   style; render and read back the frame; the card's pixels are dominated by that style's colour. Despawn it, spawn an
   effect of a second owner whose style has a different dominant channel, assert it got the **same** slot handle,
   render again: the pixels now follow the second style. Then advance its age to near 1 and check the card fades (the
   per-frame write reaches the GPU). Red on `main`: there is no pool, so the handle assertion fails.
4. Switch `client_diagnostics::runtime_capture::ownership` to `CombatFxInstance`, keeping the freeze check's bit
   equality. Its tests (`ownership/tests.rs`) and the four client test files of D-7 read the component instead of the
   asset.
5. **Measure** (D-6): A/B interleaved, arm B = this branch, 400 units only, three pairs, plus one `perf` run per arm.
   Report: the whole-run and combat-window medians; the table "frames grouped by combat events, one tick that frame"
   of the evidence file (interval p50 and p95 for 10 to 49 events); and the samples of functions specialised for
   `CombatFxMaterial` per arm. Write `docs/evidence/<date>-c17-combat-fx-pool.md`.

**Mutation red runs** (make, see the named test fail, revert; paste each): call `get_mut` on the slot in
`update_beams_system` → `combat_frames_reuse_effect_materials` fails on `Modified`; skip `release` in the hook →
`acquire_reuses_a_released_slot` fails; drop the render-world write → the GPU test's second colour check fails.

**Gate.** Common gate.

---

### WP4: Terrain chunks cover the last dual column and row

**Files (owned):** `crates/client/src/terrain/mod.rs`, `crates/client/tests/terrain_tests.rs`.

**Steps**

1. **Failing test first** (`terrain_tests.rs`): `spawned_chunks_cover_every_dual_cell`. For maps of 64 × 64, 48 × 32,
   17 × 17 and 16 × 33 cells, spawn the terrain through the production path and assert that the union of every
   chunk's dual range is exactly `0..=width` × `0..=height`, with no overlap. Red on `main` for every side that is a
   multiple of 16.
2. Fix the existing test near line 186: compute `(w + 1).div_ceil(CHUNK_DUAL_CELLS)` and remove the
   `.min(chunk_cols - 1)` clamp that hid the gap.
3. Change `cols` and `rows` in the spawn (D-5). Grep the client for every other chunk count
   (`div_ceil(CHUNK_DUAL_CELLS)`, `/ CHUNK_DUAL_CELLS`) and check each; the editor's dirty mapping needs no change
   (§2.3). Check that `mesher_golden_tests.rs` passes unchanged.

**Goldens.** None change. Spawned chunk entities increase (a 64 × 64 map goes from 16 to 25 chunks, most of the new
ones a single dual column or row). Entity counts recorded by tests are counted after startup, so no steady-state
test changes (**verify**).

**Gate.** Common gate.

---

### WP5: Golden replay pins vision-before-acquisition and upkeep-before-arrival

**Files (owned):** `crates/sim_core/tests/golden_replay_tests.rs`; new
`crates/sim_core/tests/fixtures/golden_replay_orders.ron`.

**Steps**

1. Make the two mutations one at a time (swap `update_vision` and `acquire_targets` in `step_into`; swap
   `upkeep_route` and `decide_arrival` in `MotionDraft::walk`) and run `cargo test -p sim_core`. Record the three
   behaviour tests that fail for each, and read their setups: they show the situations the replay lacks (a target that
   becomes visible on the tick it becomes acquirable; a unit that arrives within one step of its goal mid-route).
   Revert.
2. Write `golden_replay_orders.ron` in the existing schema (squads, orders, checkpoints), on a shipped map embedded
   with `include_str!` (no filesystem, for wasm), orders through `PlayerCommand` only. It must contain both situations.
   Use a short run (at most 600 ticks) with a checkpoint every 10 ticks, so a one-tick divergence is caught near where
   it starts. If the default arena lacks high ground for a reveal, embed another shipped map and generalise the loader
   to take the map per fixture.
3. Generalise the test file to play a list of fixtures (one `#[test]` per fixture, sharing the player); keep every
   existing invariant of its `//!` (embedded data, real fight required, schema equals `CHECKSUM_SCHEMA_VERSION`). Add
   for the new fixture: "must contain a same-tick reveal and acquisition" and "must contain a mid-route arrival",
   checked from the replay itself if the public API exposes enough (**verify**; otherwise state the scenario's intent in
   the fixture's header comment and rely on the mutation proof below).
4. Record the checksums on unmodified `main` code.
5. **Red runs (the proof):** with each mutation applied, the new test fails at a named checkpoint; paste both. The old
   fixture still passes under both mutations (paste), which is the gap this closes.

**Checksums.** No simulation change. The existing fixture's checksums do not change. The new fixture's are recorded
once, in the commit that adds it.

**Gate.** Common gate. Also run `cargo test -p sim_core --test golden_replay_tests --release --locked` (CI's
determinism job).

---

### WP6: C-18 baseline: characterisation goldens, solver benchmark, attribution

**Files (owned):** `crates/client/tests/smog_golden_tests.rs`; new `crates/client/benches/smog_solver.rs` and its
`[[bench]]` entry in `crates/client/Cargo.toml`; new `docs/evidence/<date>-c18-smog-solver.md`.

**Steps**

1. **Four new characterisation goldens**, recorded on the unmodified solver (no edit to `fluid.rs` in this WP), each
   class L with its `libm` functions named in its doc comment, in the style of the file:
   - `fluid_wake_decay_golden`: the showcase sources for 60 steps, then no source for 240 steps (bricks return to
     rest; under WP9 the field reaches exact zero);
   - `fluid_fast_sources_golden`: 20 sources at 8 to 12 world units per second, `dt = 0.05` (the clamp), so a
     back-trace spans more than one brick;
   - `fluid_bounds_shift_golden`: 40 steps, then `set_bounds_quantized` by (+3, 0, -2) voxels, 40 steps, then a shift
     larger than the grid (the clearing path), 20 steps;
   - `fluid_variable_dt_golden`: a fixed cycle of `dt` in {1/240, 1/144, 1/60, 1/30, 0.07, 0} and a source count
     alternating 30 and 40, so confinement switches on and off and stale vorticity is kept.
   Hash with the existing `Fnv64::grid` and `metrics` at the existing checkpoint rhythm. The file's `//!` Coverage and
   Invariants sections name them and the commit they were recorded at.
2. **Benchmark** `crates/client/benches/smog_solver.rs` (`harness = false`, as `sim_core`'s `throughput`):
   scenarios built from the public API only: `empty` (no source), `showcase` (10), `crowd_48`, `battle_240` (the
   live-battle window, 240 sources marching), `after_fight` (240 sources for 30 s, then none for 30 s; reports the
   second half separately). Arguments: `--steps N`, `--contend K` (run K-1 extra grids on their own threads on the
   same scenario, to reproduce the memory contention of §5.2; report only the first). Output: one Markdown row per
   scenario with mean, p50 and p95 ms per step. Run with `cargo bench -p client --bench smog_solver`. Its `//!` says
   that it is a relative measure for the C-18 chain and that the frame-budget A/B runs are the figures of record.
3. **Baseline record** in the evidence file: the benchmark at `e85f79f` with `--contend 1` and `--contend 8`, pinned
   with `taskset -c 32-47`; and one `perf` run of the 400 scenario on the frame-budget path (D-6), with
   `SmogGrid3d::step`'s samples per frame, `floorf`, and `__memmove_avx_unaligned_erms`. Copy §5 of this plan's
   in-game stage table into the evidence as the planning measurement, marked as such.

**Red run.** Not applicable: these goldens pin current behaviour. Mutation check instead: change one constant in
`advect_density_and_tint` (`0.018` floor decay to `0.019`), see `fluid_wake_decay_golden` fail, revert.

**Gate.** Common gate (`clippy --all-targets` lints the bench).

---

### WP7: C-18 bit-exact I: ping-pong slabs, truncating cast, dead slabs

**Files (owned):** `crates/client/src/smog/fluid.rs`.

**Steps**

1. Replace the three `copy_from_slice` at the end of `advect_velocity` and the five at the end of
   `advect_density_and_tint` with `std::mem::swap` of the field and scratch slabs. Precondition, stated in a comment
   and a unit test: each loop writes every voxel of every scratch slab it swaps (both loops cover `0..N` on all axes
   and every branch writes all its outputs). `set_bounds_quantized` copies into the scratch slabs before reading them,
   so it is unaffected.
2. In `trilinear_indices_and_weights`, replace `gx.floor() as usize` (and y, z) with `gx as usize`, with a
   `debug_assert!(!(gx < 0.0))` and a comment: the operand is clamped to `[0, N-1]` on the line above, and for a
   non-negative or NaN `f32`, truncation and floor give the same `usize`.
3. Delete `pressure` and `divergence` (allocation, `clear`). Correct the stale voxel counts in the touched
   `PERFORMANCE RATIONALE` blocks (they say 49,152; the grid has 82,944 voxels).
4. Add `PERFORMANCE RATIONALE` blocks for 1 and 2 (Why: 2.65 MB of copies per step on a memory-bound solver;
   `floorf` is a `libm` call on the `x86_64` baseline, 0.4 ms per frame in `perf`. What. Trade-off: the swap relies on
   full coverage of each pass; the cast relies on the clamp).
5. All eleven fluid and system goldens pass unchanged. Run the benchmark (`--contend 1` and `8`) and append the result
   to the C-18 evidence file.

**Mutation red run:** replace the cast by `gx.round() as usize`; `fluid_showcase_golden` fails; revert.

**Gate.** Common gate.

---

### WP8: C-18 bit-exact II: brick sparsity

**Files (owned):** `crates/client/src/smog/fluid.rs`, new `crates/client/src/smog/bricks.rs`,
`crates/client/src/smog/mod.rs` and `gpu_upload.rs` (byte accessors), `smog_golden_tests.rs` (accessor rename),
`sync_steady_state_tests.rs` (two lines, after WP3 merged).

**Steps**

1. **Tests first.**
   - `bricks.rs` unit tests: index mapping round trip for every voxel; face-neighbour sets at grid edges; the reach
     query returns every brick that intersects a voxel box, clipped.
   - In `fluid.rs`, the differential test and the flag invariant test of D-2, with the `cfg(test)` `force_dense`
     switch. Write them against the dense code first: with no sparsity yet, both grids are dense and the test passes
     trivially; it becomes meaningful as each pass gains its skip.
2. Implement D-2 pass by pass, in this order, running the differential test and the goldens after each: velocity
   advection, density advection, curl and confinement, pack, then injection's flag updates, then `new`, `clear` and
   the bounds functions. Keep each pass function under 100 lines by moving the per-brick loop and the dense per-voxel
   body into separate functions; the per-voxel body's expressions stay byte for byte.
3. Make the byte buffers private with the accessors of D-2 and update their readers.
4. `PERFORMANCE RATIONALE` on the brick flags (Why: an empty or mostly empty field still cost 5.7 ms per frame
   because every pass visited every voxel; What: rest flags per 8 × 4 × 8 brick, skipping provably unchanged bricks;
   Trade-off: flag bookkeeping on every write path, a promise that must hold for correctness, guarded by the
   differential test).
5. All eleven fluid and system goldens pass unchanged. Run the benchmark and append it to the evidence file. **Stop
   rule:** if `empty` is not at least five times faster than WP7's figure, or `battle_240` not at least 1.3 times
   faster, stop and report before WP9: the assumptions of §2.1 would be wrong.

**Mutation red runs** (paste each): reduce the reach margin from `+ 2` to `+ 0` → the differential test fails with
`fluid_fast_sources_golden`; set a velocity flag to rest after injection instead of clearing it → the flag invariant
test fails; iterate pack brick by brick instead of row by row → `total_density` differs and the differential test
fails.

**Gate.** Common gate.

---

### WP9: C-18 deliberate: quiescence; goldens re-recorded; A/B measurement

**Files (owned):** `crates/client/src/smog/fluid.rs`, `crates/client/tests/smog_golden_tests.rs`, the C-18 evidence
file.

**Steps**

1. Read Q0020. If the owner rejected the change, skip steps 2 to 4, record that in the evidence and the remediation
   plan (C-18 stays partly open, D-1's trigger applies), and go to step 5.
2. **Failing test first** (unit test in `fluid.rs`): `velocity_returns_to_exact_rest_after_sources_stop`: the
   showcase sources for 60 steps at `1/60`, then none for 1,800 steps (30 s); assert every velocity component of every
   voxel is exactly `0.0` and every brick flag is at rest. Red at WP8 (§5.4: tens of thousands of voxels stay
   nonzero).
3. Implement D-3 with the constant `SMOG_QUIESCENT_SPEED_SQ`, used in `advect_velocity` and in place of the `1e-6`
   speed literal in `pack_textures_and_compute_metrics`. `PERFORMANCE RATIONALE` (Why: without it the velocity field
   never reaches exact zero and no sparse path fires after the first wake; What; Trade-off: a deliberate output change
   below the packing threshold, D-3).
4. Run the smog goldens; record every changed constant; confirm the unchanged ones of D-3 still pass and that
   `smog_bouncing_bubbles_tests.rs` and the `fluid.rs` unit tests pass unchanged. One commit, with the message D-3
   requires.
5. **Measure** (D-6), with `main` at WP3 and WP8 merged plus this branch as arm B: interleave 0 and 400 units, three
   pairs each (12 runs), plus one `perf` run per arm at 400. Report per arm: the medians of frame interval p50, p95,
   p99 and main update p50; the combat and after-combat windows; `SmogGrid3d::step` ms per frame from `perf`; and
   the benchmark at the final commit. State D-1's trigger result (solver above or below 2.0 ms per frame at 400).
   Finish the C-18 evidence file.

**Gate.** Common gate.

---

### WP10: Audit of the remediation rows of Phases 0 to 4

**Files (owned):** `docs/plans/2026-11-01-codebase-remediation.md`, §4.1, §4.2, §4.3, §4.7 rows only.

**Steps**

1. For each row without a status (S-1 to S-19, M-1 to M-18, F-1 to F-12, Q-1 to Q-8, Q-11), find the step of
   Phases 0 to 4 that names it, then the commit (`git log --oneline -- <file>` and `git log --grep '<ID>'`) and the
   test that covers it. Append to the row: "**Fixed** in `<sha>` (<step>); test `<file>::<name>`", or "**Refuted**:
   <reason>", or "**Open**: <what remains>".
2. Do not fix anything. Each row marked Open is listed at the end of the agent's report as a candidate for a later
   plan, and recorded in this plan's §8 by WP11.
3. Do not write exit paragraphs for Phases 0 to 4: that needs a CI run per phase, which is a separate decision
   (report it).

**Gate.** `cargo fmt --all -- --check`, `cargo test --locked -p workspace_checks`.

---

### WP11: Records

**Files (owned):** see D-7.

**Steps**

1. Remediation plan §4.4: mark C-17 and C-18 **Fixed** with their commits and evidence files, or **Partly fixed**
   with what remains (D-1 trigger). Add to the Phase 6 section a one-line note naming this plan.
2. `DESIGN.md` §4: remove the four gaps from the "Terrain, cliffs and ramps", "Cross-platform determinism" and "Smog
   fog presentation" rows, naming the tests that close them. §5: replace "about 8.2 ms of every frame" with WP9's
   figures and link the C-18 and C-17 evidence files. Keep the frame-budget table of record as it is unless WP9
   re-measured the full five-run protocol (it does not by default; its A/B runs are attribution).
3. Phase 9 blueprint §9: append "Resolved by `2026-11-04-open-findings-c17-c18-p9-gaps` WPn (`<sha>`)" to each of
   the four bullets.
4. `docs/plans/README.md`: set this plan's row to `Done` (or `Partly done` with what is missing), with the commits.

**Gate.** `cargo fmt --all -- --check`, `cargo test --locked -p workspace_checks`.

---

## 5. Measurements taken while planning

All on the development server on 2026-11-03 between 21:00 and 21:30 UTC, AMD EPYC 7301 under KVM, pinned to CPUs 32
to 47. The probe code
is not kept; its logs are under `~/scratch/logs/planopen_*` and `~/scratch/planopen_fb/` and may be cleaned at any
time, so the figures here are the record.

### 5.1 Standalone solver, one idle core

A copy of `fluid.rs` at `e85f79f` with stubbed `SmogConfig` and `SmogError`, depending only on `glam 0.29.3` (the
version Bevy uses), release profile, on CPU 40. The live-battle window (28 × 4.8 × 22 world units), default
configuration, `dt = 1/60`, 600 steps; `n` sources march in rows across the window. Arm "A+B" applies D-1's options
A (swap) and B (truncating cast). Mean ms per step:

| Sources | Original | inject | confinement | advect velocity | advect density | pack | A+B | Saving |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | 2.12 | 0.00 | 0.25 | 0.35 | 1.05 | 0.46 | 1.78 | 16 % |
| 10 | 2.80 | 0.06 | 0.28 | 0.70 | 1.26 | 0.50 | 2.45 | 13 % |
| 30 | 3.32 | 0.17 | 0.31 | 0.87 | 1.44 | 0.53 | 2.89 | 13 % |
| 48 | 2.49 | 0.13 | 0 | 0.58 | 1.27 | 0.51 | 2.23 | 10 % |
| 120 | 3.23 | 0.33 | 0 | 0.78 | 1.57 | 0.56 | 2.86 | 11 % |
| 240 | 4.51 | 0.65 | 0 | 1.11 | 2.10 | 0.64 | 3.95 | 12 % |

**Bit-exactness of A+B:** 120 steps, hashing metrics every step and every voxel's density, velocity, glow and
vorticity magnitude plus both byte buffers every 30 steps (the golden file's method), for 0, 10, 30, 48, 120 and 240
sources under the default configuration and under `dissipation 0, buoyancy 0.3`: all twelve hashes equal the
original's.

### 5.2 The same, under memory contention

Seven more copies of the probe on CPUs 33 to 39 while CPU 40 measures. Each copy runs the whole scenario sequence,
and they finish at different times, so contention is heaviest in the first rows. Both arms measured in the same run:

| Sources | Original | A+B | Saving |
|---:|---:|---:|---:|
| 0 | 3.97 | 2.71 | 32 % |
| 10 | 4.00 | 3.10 | 23 % |
| 30 | 4.29 | 3.25 | 24 % |
| 48 | 2.86 | 2.48 | 13 % |
| 120 | 3.39 | 2.90 | 14 % |
| 240 | 4.58 | 3.96 | 14 % |

An earlier contended run (before A+B existed) measured the original at 5.14, 5.14 and 3.57 ms for 0, 10 and 30
sources. The empty grid's step thus went from 2.1 ms alone to 4.0 to 5.1 ms contended, close to the 5.7 ms the
client shows in game (§5.3). The solver is bound by memory traffic when the machine is busy, and A's saving grows
under contention.

### 5.3 In game, with timing lines (throwaway build of `e85f79f`)

`update_smog_field_system` and `step` printed the duration of `step` and of each stage, the source count, whether
the trajectory pre-warm ran, and the count of voxels with nonzero velocity and nonzero density. Release build,
headless KWin on the RX 580, one locked run per scenario (D-6's command), fixture root prepared on `lavapipe`. These
are not figures of record: one run each, with printing in the frame.

| Units | Load (1 min) | Frames | Frame interval p50 | p95 | Update p50 | `step` p50 | `step` p95 |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | 8.64 (after the build) | 5,121 | 11.70 | 15.01 | 10.44 | 5.66 | 8.48 |
| 400 | 3.85 | 3,794 | 15.53 | 19.78 | 13.25 | 7.26 | 11.50 |

Stage means and field activity are in §2.1's table. At 400 units the smog sources averaged 29.0 (at most 74), and
the pre-warm never ran (0 of 3,978 steps).

### 5.4 Does the field ever come to rest?

The standalone probe (with A+B, bit-exact), live-battle window, default configuration, 30 s with sources then 30 s
without, counting voxels with any nonzero velocity component:

| Sources | after 10 s | 20 s | 30 s (sources stop) | 40 s | 50 s | 60 s | of which below `1e-4` at 60 s | subnormal | nonzero density at 40 s and later |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 30 | 17,155 | 27,081 | 37,984 | 39,087 | 39,089 | 39,089 | 39,089 | 0 | 0 |
| 240 | 20,828 | 24,898 | 26,516 | 50,329 | 50,444 | 50,444 | 50,441 | 0 | 0 |

Density and glow return to exactly zero (the haze floor and the `1e-6` early outs). Velocity never does: §2.1 explains
why, and D-3 is the fix.

---

## 6. Risks

| Risk | Mitigation |
|---|---|
| A brick flag claims rest when a voxel is not at rest, and the field silently diverges | The flag is a one-way promise (D-2); the differential test compares every slab bit for bit, tint included, over a script built to hit every write path; WP6's goldens pin the edge cases on the original code |
| Pack's skipped segments change the `total_density` sum | Pack keeps row-major order and skips only exact zeros (D-2 rule 6); the mutation red run of WP8 shows the test catches a reordering |
| The quiescence change is visible after all | D-3's argument; the semantic smog tests must pass unchanged, or WP9 stops; Q0020 lets the owner object |
| A pooled slot shows the previous effect for one frame | The slot's buffer is written in `ExtractSchedule` of the frame the new effect spawns, before the render schedule draws it; a newly created slot carries the effect's initial data; WP3's GPU test checks the reused colour |
| An effect despawned by a path the pool does not know leaks its slot | The `on_remove` hook runs on every removal path; the unit test covers lifetime end, scene teardown and app exit |
| `write_buffer` per live effect costs more than expected at 300 live effects | Measured by WP3's A/B runs; the write is skipped for unchanged instances (`Changed`) |
| The new replay fixture is slow under wasmtime | At most 600 ticks; the CI determinism jobs show its time |
| Three gates at once on 16 cores slow each other | At most three (§4); documentation WPs use the light gate |
| Re-recorded smog goldens conflict with Q0014 (b) | Q0014 was answered (b); plan `2026-11-04-libm-smog-math` runs strictly after WP9 and re-records in its own commit as class H, so each golden change has one cause |

---

## 7. Decisions for the owner

- **Q0020 (new, not blocking):** may the smog solver set velocities of at most `1e-3` world units per second to
  exactly zero (D-3)? Default: yes. WP9 makes the change and re-records the smog goldens. If the owner says no, WP9
  keeps WP8's bit-exact solver, and C-18 stays partly open with D-1's trigger.
- **Q0015 (open):** this plan publishes before-and-after figures and passes or fails no frame time.
- **Q0014 (answered 2026-11-03, option b):** carried out by plan `2026-11-04-libm-smog-math` after WP9. Within this
  plan, every re-recorded smog golden remains class L.

---

## 8. Out of scope

- Options F and G of D-1 (lower fixed smog rate, worker thread), unless WP9's trigger fires; then a later plan.
- A custom instanced pipeline for combat effects (D-4 option b).
- The frame-budget figures of record in `DESIGN.md` §5 (five interleaved runs per scenario): WP9's A/B runs are
  attribution. A full re-measurement of record is a separate task.
- Exit paragraphs for remediation Phases 0 to 4 (WP10 reports whether they are needed).
- Step 8.5 (Q0001; plan `2026-11-04-phase-8-5-veo-media`), Q0014 (b) (plan `2026-11-04-libm-smog-math`, after WP9), the remediation plan's §6 items, the single slow first-tick frame, and C-7 in a live
  battle (§2.4).
- Any change to charter §6.
