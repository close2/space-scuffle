# Stage 1 Architectural Blueprint: LiveBattle Low-Ground 3D Smog Alignment & Natural Downward Settling for Standing Mist

**Date:** 2026-10-30  
**Target Crates:** `crates/client` (`smog/mod.rs`, `smog/fluid.rs`, `tests/smog_bouncing_bubbles_tests.rs`), `assets/shaders/smog_volume.wgsl`  
**Status:** Stage 1 Complete — Ready for Stage 2 Implementation

---

## 1. Executive Summary & User-Reported Symptoms

The user reported two visual and physical defects in the 3D volumetric smog/mist subsystem:

1. **Displaced Smog on the Lower Level in `LiveBattle`:**
   > *"When I look at the battle simulation and the units are at the lower level there are camera positions, where the smog/mist is completely displaced. It's above or maybe even in front of the units."*
2. **Frozen Mist When Units Stand Still:**
   > *"another detail. When standing the mist no longer moves. This doesn't look natural. There should be a slight movement in the fog (probably downwards)"*

This blueprint documents the exact mathematical and code-level root causes for both defects and specifies a clean, zero-allocation, deterministic-boundary-compliant implementation plan.

---

## 2. Root-Cause Analysis

### 2.1 Defect 1: Why Lower-Level (`CliffLevel(0)`) Smog Is Displaced Above / In Front of Units in `LiveBattle`

In `sim_map` (`crates/sim_map/src/grid.rs` lines 25–43), each discrete cliff level step is `CLIFF_LEVEL_HEIGHT = 3` world units:
- **Lower Level (`CliffLevel(0)`):** `altitude = 0.0` (`ground_z = 0.0`)
- **Upper Level (`CliffLevel(1)`):** `altitude = 3.0` (`ground_z = 3.0`)
- **High Plateau (`CliffLevel(2)`):** `altitude = 6.0` (`ground_z = 6.0`)

Four interacting bugs in `crates/client/src/smog/mod.rs` and `assets/shaders/smog_volume.wgsl` cause lower-level (`ground_z = 0.0`) units to have their smog displaced `1.5m–3.0m` up in the air (which projects **above and in front of** the units along the 58° pitched RTS perspective camera ray):

1. **Bug 1A — `if cam_ground_y > 0.1` Rejects Lower-Level Altitude `0.0` (`crates/client/src/smog/mod.rs` lines 679–701):**
   ```rust
   let (cam_x, cam_z, cam_ground_y) = if let Some(cam) = camera_query.iter().next() {
       (cam.target.x, cam.target.y, (cam.look_at_y - 1.2).max(0.0))
   } else {
       (16.0, 32.0, 3.0)
   };
   let ground_y = if cam_ground_y > 0.1 {
       cam_ground_y
   } else {
       let mut found_y = 3.0_f32;
       for (_, instance, _) in &bubble_query {
           if instance.0.ground_z > 0.1 {
               found_y = instance.0.ground_z;
               break;
           }
       }
       found_y
   };
   ```
   When the camera pans over the lower level (`CliffLevel(0)`, `altitude = 0.0`), `camera_control_system` (`crates/client/src/camera.rs` line 233) sets `cam.look_at_y = 0.0 + 1.2 = 1.2`, so `cam_ground_y = 0.0`.
   Because line 684 checks `if cam_ground_y > 0.1`, it treats `0.0` as invalid and enters the `else` branch, which scans `&bubble_query` for `instance.0.ground_z > 0.1`, skips all lower-level units (`ground_z == 0.0`), grabs an upper-level building/unit (`ground_z == 3.0`), and sets `ground_y = 3.0`! Thus `ground_y` can **never** reach `0.0`!

2. **Bug 1B — Camera-Crosshair-Only Elevation Ignores Lower-Level Units in the Viewport (`crates/client/src/smog/mod.rs` lines 684–716):**
   When the camera crosshair `cam.target` is positioned over an upper-level cliff edge (`cam_ground_y = 3.0`) or partway up a ramp (`0.1 < cam_ground_y <= 3.0`) while looking at units fighting on the lower level (`ground_z = 0.0`), `ground_y` equals `cam_ground_y` (`1.5..3.0`), placing `target_min.y` (`base_y`) meters above the lower-level floor.

3. **Bug 1C — Artificial `pos.y` Override Teleports Lower-Level Smog to `target_min.y` (`crates/client/src/smog/mod.rs` line 800):**
   ```rust
   pos.y = target_min.y + (pos.y - instance.0.ground_z).clamp(0.35, 2.2);
   ```
   When a lower-level unit (`ground_z = 0.0`, `transform.translation.y = 0.45`) is processed while `target_min.y = 3.0` (from Bug 1A or 1B), line 800 overwrites `pos.y` to `3.0 + 0.45 = 3.45`!
   Both `BubbleSmogSource.position.y` and the shader sphere `bubble_pos_radius[b].y` are teleported `3.0` meters into mid-air above the unit. Viewed from the 58° pitched RTS camera (`RtsCamera`), a horizontal smog layer floating at `Y = 3.0..5.0` above a unit at `Y = 0.45` projects directly **above and in front of** the lower-level units.

4. **Bug 1D — Shader `march_max.y` Clamping, 2D XZ-Only Cylinder Clearance, and Unquantized Uniform Bounds (`assets/shaders/smog_volume.wgsl` lines 96–100 & 192–207, `crates/client/src/smog/mod.rs` lines 726–739 & 898–899):**
   - `smog_volume.wgsl` lines 96–100 hard-clamped `march_max.y = min(smog.volume_max.y, smog.volume_min.y + 2.05)`, slicing off smog whenever units span both lower (`0.0`) and upper (`3.0`) levels.
   - `smog_volume.wgsl` lines 192–207 computed only 2D horizontal distance `r_xz = sample_pos.xz - b_pos_rad.xz` instead of 3D spherical distance `r_3d = sample_pos - b_pos_rad.xyz`, applying bow-wave rim boosts and core clearance across an infinite vertical cylinder at all `Y` altitudes.
   - `update_smog_field_system` assigned `target_min`/`target_max` to `Transform` and `material.uniform.volume_min`/`volume_max` instead of reading back `grid.bounds_min()` and `grid.bounds_max()` after `grid.set_bounds_quantized(target_min, target_max)`.

---

### 2.2 Defect 2: Why Mist Freezes Completely When Units Stand Still

1. **Zero Velocity Injection for Stationary Sources (`crates/client/src/smog/fluid.rs` lines 619–684):**
   In `SmogGrid3d::step`, velocity impulses are gated by `if source.influence_strength > 0.0 && config.wake_influence > 0.0 && speed > 1e-3` and scaled by `speed`. When a bubble stands still (`speed == 0.0`), zero velocity is injected into surrounding voxels.
2. **Near-Zero Buoyancy & Sparse Advection Early-Out (`crates/client/src/smog/mod.rs` line 104 & `crates/client/src/smog/fluid.rs` lines 1074–1085):**
   - `SmogConfig::default()` sets `buoyancy: 0.008` (virtually zero).
   - In `SmogGrid3d::advect_density_and_tint` (lines 1074–1078):
     ```rust
     if self.density[idx] < 1e-6
         && self.glow[idx] < 1e-6
         && self.u_x[idx].abs() < 1e-4
         && self.u_z[idx].abs() < 1e-4
     {
         ...
         continue;
     }
     ```
     Notice that `self.u_y[idx]` and the neighboring vertical voxel `iy + 1` / `iy - 1` are **not** checked in the sparse early-out! Consequently, an empty voxel directly below a stationary bubble with `u_x == 0, u_z == 0` is skipped before backward semi-Lagrangian tracing can pull density downward into it.
3. **Opposing Vertical Noise Drift Signs in `assets/shaders/smog_volume.wgsl` (lines 163–166):**
   In `fract(sample_pos * scale + offset(time))`, a feature at constant phase satisfies `sample_pos.y * scale + vy_phase * time = const`, so moving **downwards** (`d(sample_pos.y)/dt < 0`) requires **`vy_phase > 0` (positive time coefficient on Y)**.
   Lines 163–166 used `-time * 0.02` in `noise_uvw1.y` (drifting *upwards*) and `+time * 0.03` in `noise_uvw2.y` (drifting slowly *downwards*), causing the two octaves to cancel out into a static in-place shimmer when `vel_vec == 0`.

---

## 3. Detailed Architectural Design & Implementation Plan

### 3.1 Fix 1: True 3D World-Space Elevation & Multi-Tier Vertical Bounds in `LiveBattle` (`crates/client/src/smog/mod.rs`)

1. **Determine Ground Elevation from Visible Mobile Units (Supporting `0.0` Low Ground) + Camera Fallback:**
   In `update_smog_field_system` (`crates/client/src/smog/mod.rs`), when `current_tab == crate::LabTab::LiveBattle`:
   - Extract `(cam_x, cam_z, cam_ground_y)` from `camera_query` where `cam_ground_y = (cam.look_at_y - 1.2).max(0.0)`.
   - Scan `&bubble_query` for non-bouncing mobile units (`bouncing.is_none() && !matches!(instance.0.shape, ShapeKind::BuildingDome { .. })`) within the horizontal camera window `(transform.translation.x - cam_x).abs() <= 14.5 && (transform.translation.z - cam_z).abs() <= 11.5`.
   - Track `min_mobile_ground_y: Option<f32>` across those visible mobile units (treating `0.0` as a first-class altitude, never filtering with `> 0.1`).
   - Compute `ground_y = min_mobile_ground_y.unwrap_or(cam_ground_y).max(0.0)`.
   - Use a constant vertical span `LIVE_BATTLE_EXTENT_Y: f32 = 4.8` in `LiveBattle`:
     - `step_y = LIVE_BATTLE_EXTENT_Y / GRID_NY as f32` (`4.8 / 12.0 = 0.40m`).
     - `base_y = (ground_y / step_y).floor() * step_y`.
     - `target_min = Vec3::new(center_x - 14.0, base_y, center_z - 11.0)`.
     - `target_max = Vec3::new(center_x + 14.0, base_y + LIVE_BATTLE_EXTENT_Y, center_z + 11.0)`.
     - When any lower-level unit (`ground_z = 0.0`) is in view, `base_y = 0.0` and the volume spans `Y ∈ [0.0, 4.8]`, which simultaneously encompasses **both** `CliffLevel(0)` (`ground_z = 0.0`, units at `Y ≈ 0.45..1.8`) **and** `CliffLevel(1)` (`ground_z = 3.0`, units at `Y ≈ 3.45..4.5`) with zero vertical shifting!

2. **Use Authoritative `grid.bounds_min()` / `grid.bounds_max()` After `set_bounds_quantized`:**
   Immediately after `grid.set_bounds_quantized(target_min, target_max)` (or `grid.set_bounds(target_min, target_max)` in `VisualLab`):
   ```rust
   let actual_min = grid.bounds_min();
   let actual_max = grid.bounds_max();
   ```
   Use `actual_min` and `actual_max` for:
   - Updating the `SmogVolumeMarker` entity's `Transform` (`translation = (actual_min + actual_max) * 0.5`, `scale = (actual_max - actual_min) / Vec3::new(13.0, 2.8, 9.0)`).
   - Culling out-of-bounds unit sources (`actual_min.x - 1.5 .. actual_max.x + 1.5`, `actual_min.y - 0.5 .. actual_max.y + 0.5`, `actual_min.z - 1.5 .. actual_max.z + 1.5`).
   - Writing `material.uniform.volume_min` and `material.uniform.volume_max`.

3. **Eliminate the Artificial `pos.y = target_min.y + ...` Override (`crates/client/src/smog/mod.rs` line 800):**
   Replace:
   ```rust
   pos.y = target_min.y + (pos.y - instance.0.ground_z).clamp(0.35, 2.2);
   ```
   with true world-space vertical positioning anchored to the unit's own `instance.0.ground_z`:
   ```rust
   pos.y = instance.0.ground_z + (pos.y - instance.0.ground_z).clamp(0.35, 1.8);
   ```
   This guarantees that every unit on `CliffLevel(0)` (`ground_z = 0.0`), ramps (`ground_z ∈ (0.0, 3.0)`), or `CliffLevel(1)` (`ground_z = 3.0`) emits smog and carves its bubble cavity at its exact 3D world-space position.

---

### 3.2 Fix 2: Natural Downward Settling & Basal Mist Dispersion for Standing Units (`crates/client/src/smog/fluid.rs` & `crates/client/src/smog/mod.rs`)

1. **Gentle Downward Gravitational Settling in `SmogConfig::default()` (`crates/client/src/smog/mod.rs`):**
   - Update `buoyancy` in `SmogConfig::default()` from `0.008` to `-0.18` (world units/sec in `-Y`), modeling cool, dense vapor that slowly settles downward toward the floor.

2. **Stationary & Low-Speed Downward Mist Cascade in `SmogGrid3d::step` (`crates/client/src/smog/fluid.rs`):**
   - Inside the per-voxel loop of `SmogGrid3d::step` (after the moving wake impulse block at lines 619–684), add a continuous **stationary / slow-unit downward settling & basal outward drift impulse** active whenever `source.influence_strength > 0.0 && config.wake_influence > 0.0`:
     ```rust
     let stationary_weight = (1.0 - (speed / 0.45)).clamp(0.0, 1.0);
     if stationary_weight > 0.0 && dist < 1.35 * r_k && dist >= 0.62 * r_b {
         let shell_prox = (1.0 - (dist / (1.35 * r_k)).min(1.0)).max(0.0);
         let r_horiz = Vec3::new(r_vec.x, 0.0, r_vec.z);
         let r_hat_radial = if r_horiz.length_squared() > 1e-6 {
             r_horiz.normalize()
         } else {
             Vec3::X
         };
         // Gentle downward cascade (-Y) along the bubble sheath + soft outward spread near the base
         let vy_sink = -0.42 * shell_prox;
         let basal_t = ((0.15 - (r_vec.y / r_k)) / 0.85).clamp(0.0, 1.0);
         let v_radial = r_hat_radial * (0.20 * shell_prox * basal_t);
         let settle_impulse = (v_radial + Vec3::new(0.0, vy_sink, 0.0))
             * (stationary_weight * dt * config.wake_influence * source.influence_strength);
         self.u_x[idx] += settle_impulse.x;
         self.u_y[idx] += settle_impulse.y;
         self.u_z[idx] += settle_impulse.z;
     }
     ```
   - This ensures that even when a unit is completely stationary (`speed == 0.0`), the mist emitted around its sphere continuously glides downwards along the bubble's curvature (`u_y < 0`) and fans softly outward at its base.

3. **Vertical-Aware Sparse Fast-Path in `SmogGrid3d::advect_density_and_tint` (`crates/client/src/smog/fluid.rs` lines 1074–1085):**
   - Update the sparse early-out condition in `advect_density_and_tint` so it checks `self.u_y[idx].abs() < 1e-4` as well as vertical neighbor density `self.density[Self::index(ix, (iy + 1).min(GRID_NY - 1), iz)] < 1e-6` and `self.density[Self::index(ix, iy.saturating_sub(1), iz)] < 1e-6`.
   - This ensures that downward settling (`u_y < 0` or `buoyancy < 0`) always advects density into empty voxels directly beneath a stationary bubble.

---

### 3.3 Fix 3: 3D Spherical Bubble Clearance, Full Volume Height Raymarching, and Coherent Downward Wispy Noise Drift (`assets/shaders/smog_volume.wgsl`)

1. **Remove Artificial `2.05m` Vertical Clipping (`assets/shaders/smog_volume.wgsl` lines 96–100):**
   Replace:
   ```wgsl
   let march_max = vec3<f32>(
       smog.volume_max.x,
       min(smog.volume_max.y, smog.volume_min.y + 2.05),
       smog.volume_max.z,
   );
   ```
   with:
   ```wgsl
   let march_max = smog.volume_max.xyz;
   ```
   so multi-tier battles spanning `CliffLevel(0)` (`0.0m`) and `CliffLevel(1)` (`3.0m`) raymarch across the full `[volume_min.y, volume_max.y]` bounds.

2. **Coherent Downward Wispy Noise Advection (`assets/shaders/smog_volume.wgsl` lines 163–166):**
   Update both 3D noise octaves so their `Y` phase offsets have a coherent positive `time` rate (which translates noise features **downwards** in `-Y` as `time` increases):
   ```wgsl
   let noise_uvw1 = fract(sample_pos * 0.28 - vel_vec * 0.08 + vec3<f32>(time * 0.03, time * 0.11, -time * 0.02));
   let noise_uvw2 = fract(sample_pos * 0.58 - vel_vec * 0.14 + vec3<f32>(-time * 0.04, time * 0.19, time * 0.03));
   ```

3. **True 3D Spherical Distance for Sub-Voxel Bubble Clearance & Bow-Wave Rim (`assets/shaders/smog_volume.wgsl` lines 192–207):**
   Replace 2D `let r_xz = sample_pos.xz - b_pos_rad.xz; let dist_sq = dot(r_xz, r_xz);` with true 3D distance around `b_pos_rad.xyz`:
   ```wgsl
   let r_3d = sample_pos - b_pos_rad.xyz;
   let r_xz = r_3d.xz;
   let dist_sq = dot(r_3d, r_3d);
   ```
   so a bubble at `b_pos_rad.xyz` only clears its actual 3D spherical interior and never affects smog layers meters above or below it.

---

## 4. Verification & Test Plan (`crates/client/tests/smog_bouncing_bubbles_tests.rs`)

Add comprehensive automated tests in `crates/client/tests/smog_bouncing_bubbles_tests.rs`:

1. **`stationary_bubble_generates_downward_settling_mist_motion`:**
   - Spawn a stationary `BubbleSmogSource` (`velocity = Vec3::ZERO`) in `SmogGrid3d` and step for 25 frames with `SmogConfig::default()`.
   - Assert that `metrics.total_density > 0.0`, `metrics.max_speed > 0.05` (mist is actively moving, not frozen), and that the mean vertical velocity `u_y` in the bubble's shell voxels is strictly negative (`mean_uy < -0.02`, confirming downward movement).
2. **`live_battle_low_ground_and_high_ground_units_preserve_true_world_y`:**
   - Simulate `LiveBattle` smog volume bounds with lower-level units (`ground_z = 0.0`, `y = 0.45`) and upper-level units (`ground_z = 3.0`, `y = 3.45`) across camera positions (camera over lower level `cam_ground_y = 0.0` and camera over upper level `cam_ground_y = 3.0` while lower-level units are in view).
   - Verify that lower-level units inject smog at `Y ≈ 0.45` (never displaced to `3.45`), upper-level units inject smog at `Y ≈ 3.45`, and `grid.bounds_min().y <= 0.0` and `grid.bounds_max().y >= 4.2` so both tiers are faithfully represented in 3D world space.
3. **Workspace Quality Gates:**
   - `cargo test --workspace`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo fmt --all -- --check`
