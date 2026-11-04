//! 3D Eulerian Navier-Stokes fluid grid, bubble emission, wake displacement, and 3D texture packing.
//!
//! # Architectural Responsibilities
//! This module owns the `$96 \times 12 \times 72 = 82,944$` voxel Eulerian fluid solver for the 3D smog field.
//! Every mobile bubble acts simultaneously as:
//! 1. A **3D Smog Generator:** injecting density and chromatic tint into its trailing wake.
//! 2. A **3D Smog Influencer:** displacing pre-existing smog out of its core into a bow wave, injecting
//!    momentum drag along its velocity vector, and shedding wake vortex rings.
//!
//! # Invariants Maintained
//! - **Zero per-frame heap allocations:** all fluid slabs and GPU upload byte buffers are pre-allocated once.
//! - **Strict boundary containment:** velocity wall normal components vanish on all 6 faces (`$\mathbf{u} \cdot \hat{\mathbf{n}} = 0$`).
//! - **Non-negative density:** `$\rho(\mathbf{x}) \ge 0$` across all voxels at all times.
//! - **Finite arithmetic:** all fields remain finite without NaNs or infinities.

use super::{SmogConfig, SmogError};
use bevy::prelude::*;

/// Number of grid voxels along the world X axis.
pub const GRID_NX: usize = 96;
/// Number of grid voxels along the world Y axis (vertical elevation).
pub const GRID_NY: usize = 12;
/// Number of grid voxels along the world Z axis.
pub const GRID_NZ: usize = 72;
/// Total number of voxels in the 3D fluid volume (`$96 \times 12 \times 72 = 82,944$`).
pub const TOTAL_VOXELS: usize = GRID_NX * GRID_NY * GRID_NZ;

/// Peak clamp density for numerical stability and saturation control.
pub const SMOG_DENSITY_MAX: f32 = 4.0;
/// Velocity normalization ceiling for 8-bit texture packing (`$\pm 5.0$` world units/s).
pub const SMOG_PACK_VELOCITY_MAX: f32 = 5.0;
/// External bubble state presented to the 3D fluid solver for emission and obstacle influence.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BubbleSmogSource {
    /// World-space center of the bubble sphere.
    pub position: Vec3,
    /// 3D velocity vector in world units per second.
    pub velocity: Vec3,
    /// Collision radius of the bubble in world units.
    pub radius: f32,
    /// Linear RGB chromatic tint matching the bubble's player/team styling.
    pub team_color: Vec3,
    /// Emission strength multiplier (`$\ge 0.0$`).
    pub emission_strength: f32,
    /// Physical wake displacement and momentum drag strength (`$\ge 0.0$`).
    pub influence_strength: f32,
}

/// Telemetry metrics produced by a single 3D fluid step.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SmogStepMetrics {
    /// Sum of density values across all voxels.
    pub total_density: f32,
    /// Peak density found in any individual voxel.
    pub peak_density: f32,
    /// Maximum fluid speed `$\|\mathbf{u}\|$` across the grid.
    pub max_speed: f32,
    /// Maximum fluid vorticity magnitude `$\|\nabla \times \mathbf{u}\|$` across the grid.
    pub max_vorticity: f32,
    /// Number of active bubble sources that influenced the grid this step.
    pub active_sources: usize,
}

// PERFORMANCE RATIONALE:
// Why: 82,944 voxels per frame advected, confined, and packed into 3D textures.
// Re-allocating dynamic Vecs or hash maps per frame would cause 100+ allocations/frame and GC pressure.
// What: All fluid state (velocity u_x, u_y, u_z, density rho, glow, RGB chromatic tint r, g, b, vorticity)
// and ping-pong scratch buffers are pre-allocated once in flat Box<[f32]> slabs; the advection passes
// swap each field with its scratch slab instead of copying it back (see `advect_velocity`).
// Zero allocations occur during the per-frame `step()` call.
// Trade-off: ~7.3 MB of f32 slabs (22 x 331,776 bytes) plus 0.66 MB of upload bytes retained in the
// `SmogGrid3d` resource.
/// 3D Eulerian fluid solver with pre-allocated flat slabs.
#[derive(Resource)]
pub struct SmogGrid3d {
    bounds_min: Vec3,
    bounds_max: Vec3,
    cell_size: Vec3,
    inv_cell_size: Vec3,

    // Velocity field
    u_x: Box<[f32]>,
    u_y: Box<[f32]>,
    u_z: Box<[f32]>,
    u_x_scratch: Box<[f32]>,
    u_y_scratch: Box<[f32]>,
    u_z_scratch: Box<[f32]>,

    // Density, luminescent glow, and chromatic tint fields
    density: Box<[f32]>,
    density_scratch: Box<[f32]>,
    glow: Box<[f32]>,
    glow_scratch: Box<[f32]>,
    tint_r: Box<[f32]>,
    tint_g: Box<[f32]>,
    tint_b: Box<[f32]>,
    tint_r_scratch: Box<[f32]>,
    tint_g_scratch: Box<[f32]>,
    tint_b_scratch: Box<[f32]>,

    // Vorticity confinement fields
    vorticity_x: Box<[f32]>,
    vorticity_y: Box<[f32]>,
    vorticity_z: Box<[f32]>,
    vorticity_mag: Box<[f32]>,

    /// Pre-allocated RGBA8 byte buffer for uploading the 3D density + tint texture.
    pub rgba_density_bytes: Vec<u8>,
    /// Pre-allocated RGBA8 byte buffer for uploading the 3D velocity + vorticity texture.
    pub rgba_velocity_bytes: Vec<u8>,
}

impl SmogGrid3d {
    /// Creates a new 3D smog fluid grid spanning `[bounds_min, bounds_max]`.
    ///
    /// # Errors
    /// Returns [`SmogError::InvalidBounds`] if `bounds_min >= bounds_max` or bounds are non-finite.
    #[expect(
        clippy::cast_precision_loss,
        reason = "fluid grid indices and counts stay far below the range of f32 and i32"
    )]
    pub fn new(bounds_min: Vec3, bounds_max: Vec3) -> Result<Self, SmogError> {
        if !bounds_min.is_finite()
            || !bounds_max.is_finite()
            || bounds_min.x >= bounds_max.x - 1.0
            || bounds_min.y >= bounds_max.y - 1.0
            || bounds_min.z >= bounds_max.z - 1.0
        {
            return Err(SmogError::InvalidBounds {
                min: bounds_min,
                max: bounds_max,
            });
        }

        let extent = bounds_max - bounds_min;
        let cell_size = Vec3::new(
            extent.x / GRID_NX as f32,
            extent.y / GRID_NY as f32,
            extent.z / GRID_NZ as f32,
        );
        let inv_cell_size = Vec3::new(1.0 / cell_size.x, 1.0 / cell_size.y, 1.0 / cell_size.z);

        let make_slab = || vec![0.0_f32; TOTAL_VOXELS].into_boxed_slice();

        // Default initial tint: pale neutral cyan-gray (0.6, 0.7, 0.8)
        let mut tint_r = make_slab();
        let mut tint_g = make_slab();
        let mut tint_b = make_slab();
        for i in 0..TOTAL_VOXELS {
            tint_r[i] = 0.60;
            tint_g[i] = 0.70;
            tint_b[i] = 0.80;
        }

        let rgba_density_bytes = vec![0u8; TOTAL_VOXELS * 4];
        let rgba_velocity_bytes = vec![128u8; TOTAL_VOXELS * 4];

        Ok(Self {
            bounds_min,
            bounds_max,
            cell_size,
            inv_cell_size,
            u_x: make_slab(),
            u_y: make_slab(),
            u_z: make_slab(),
            u_x_scratch: make_slab(),
            u_y_scratch: make_slab(),
            u_z_scratch: make_slab(),
            density: make_slab(),
            density_scratch: make_slab(),
            glow: make_slab(),
            glow_scratch: make_slab(),
            tint_r,
            tint_g,
            tint_b,
            tint_r_scratch: make_slab(),
            tint_g_scratch: make_slab(),
            tint_b_scratch: make_slab(),
            vorticity_x: make_slab(),
            vorticity_y: make_slab(),
            vorticity_z: make_slab(),
            vorticity_mag: make_slab(),
            rgba_density_bytes,
            rgba_velocity_bytes,
        })
    }

    /// Converts a continuous world-space coordinate into floating-point grid index coordinates `$[0, N-1]^3$`.
    #[inline]
    #[must_use]
    pub fn world_to_grid(&self, world_pos: Vec3) -> Vec3 {
        Vec3::new(
            (world_pos.x - self.bounds_min.x) * self.inv_cell_size.x - 0.5,
            (world_pos.y - self.bounds_min.y) * self.inv_cell_size.y - 0.5,
            (world_pos.z - self.bounds_min.z) * self.inv_cell_size.z - 0.5,
        )
    }

    /// Converts discrete voxel indices into the world-space center coordinate of the voxel.
    #[inline]
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "fluid grid indices and counts stay far below the range of f32 and i32"
    )]
    pub fn grid_to_world(&self, ix: usize, iy: usize, iz: usize) -> Vec3 {
        Vec3::new(
            self.bounds_min.x + (ix as f32 + 0.5) * self.cell_size.x,
            self.bounds_min.y + (iy as f32 + 0.5) * self.cell_size.y,
            self.bounds_min.z + (iz as f32 + 0.5) * self.cell_size.z,
        )
    }

    /// Returns the minimum bounding corner of the fluid grid.
    #[inline]
    #[must_use]
    pub fn bounds_min(&self) -> Vec3 {
        self.bounds_min
    }

    /// Returns the maximum bounding corner of the fluid grid.
    #[inline]
    #[must_use]
    pub fn bounds_max(&self) -> Vec3 {
        self.bounds_max
    }

    /// Dynamically updates the bounding box of the fluid simulation.
    ///
    /// If bounds shift by more than 0.5m, the fluid fields are cleared to avoid stretching artifacts.
    #[expect(
        clippy::cast_precision_loss,
        reason = "fluid grid indices and counts stay far below the range of f32 and i32"
    )]
    pub fn set_bounds(&mut self, bounds_min: Vec3, bounds_max: Vec3) {
        if !bounds_min.is_finite()
            || !bounds_max.is_finite()
            || bounds_min.x >= bounds_max.x - 1.0
            || bounds_min.y >= bounds_max.y - 1.0
            || bounds_min.z >= bounds_max.z - 1.0
        {
            return;
        }
        let shift = (bounds_min - self.bounds_min)
            .length()
            .max((bounds_max - self.bounds_max).length());
        self.bounds_min = bounds_min;
        self.bounds_max = bounds_max;
        let extent = bounds_max - bounds_min;
        self.cell_size = Vec3::new(
            extent.x / GRID_NX as f32,
            extent.y / GRID_NY as f32,
            extent.z / GRID_NZ as f32,
        );
        self.inv_cell_size = Vec3::new(
            1.0 / self.cell_size.x,
            1.0 / self.cell_size.y,
            1.0 / self.cell_size.z,
        );
        if shift > 0.5 {
            self.clear();
        }
    }

    /// Quantizes dynamic bounding box updates to integer voxel cell multiples and preserves
    /// existing smoke fields by shifting slabs in-place instead of clearing or dragging.
    ///
    /// When the camera or active region shifts by `$(\Delta i_x, \Delta i_y, \Delta i_z)$` voxels,
    /// the valid overlapping region of the simulation fields is shifted inside the buffers without
    /// allocating any memory or clearing the grid. Newly exposed outer boundary voxels are filled
    /// with zero density and velocity.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "fluid grid indices and counts stay far below the range of f32 and i32"
    )]
    pub fn set_bounds_quantized(&mut self, new_min: Vec3, new_max: Vec3) {
        if !new_min.is_finite()
            || !new_max.is_finite()
            || new_min.x >= new_max.x - 1.0
            || new_min.y >= new_max.y - 1.0
            || new_min.z >= new_max.z - 1.0
        {
            return;
        }

        let new_extent = new_max - new_min;
        let old_extent = self.bounds_max - self.bounds_min;
        // If extent changed significantly (more than 1mm), re-initialize cell sizes and clear.
        if (new_extent - old_extent).length_squared() > 1e-6 {
            self.set_bounds(new_min, new_max);
            return;
        }

        let dx_cells = ((new_min.x - self.bounds_min.x) * self.inv_cell_size.x).round() as i32;
        let dy_cells = ((new_min.y - self.bounds_min.y) * self.inv_cell_size.y).round() as i32;
        let dz_cells = ((new_min.z - self.bounds_min.z) * self.inv_cell_size.z).round() as i32;

        if dx_cells == 0 && dy_cells == 0 && dz_cells == 0 {
            return;
        }

        // If the shift is larger than the entire grid dimensions, clear everything.
        if dx_cells.abs() >= GRID_NX as i32
            || dy_cells.abs() >= GRID_NY as i32
            || dz_cells.abs() >= GRID_NZ as i32
        {
            self.bounds_min = new_min;
            self.bounds_max = new_max;
            self.clear();
            return;
        }

        // Snap bounds precisely by the integer voxel shift to eliminate floating-point drift
        self.bounds_min += Vec3::new(
            dx_cells as f32 * self.cell_size.x,
            dy_cells as f32 * self.cell_size.y,
            dz_cells as f32 * self.cell_size.z,
        );
        self.bounds_max = self.bounds_min + old_extent;

        // Copy source fields to scratch buffers, then read from scratch with offset into source fields.
        self.density_scratch.copy_from_slice(&self.density);
        self.glow_scratch.copy_from_slice(&self.glow);
        self.tint_r_scratch.copy_from_slice(&self.tint_r);
        self.tint_g_scratch.copy_from_slice(&self.tint_g);
        self.tint_b_scratch.copy_from_slice(&self.tint_b);
        self.u_x_scratch.copy_from_slice(&self.u_x);
        self.u_y_scratch.copy_from_slice(&self.u_y);
        self.u_z_scratch.copy_from_slice(&self.u_z);

        for iz in 0..GRID_NZ {
            let src_z = iz as i32 + dz_cells;
            let valid_z = src_z >= 0 && src_z < GRID_NZ as i32;
            for iy in 0..GRID_NY {
                let src_y = iy as i32 + dy_cells;
                let valid_y = valid_z && src_y >= 0 && src_y < GRID_NY as i32;
                for ix in 0..GRID_NX {
                    let dst_idx = Self::index(ix, iy, iz);
                    let src_x = ix as i32 + dx_cells;
                    if valid_y && src_x >= 0 && src_x < GRID_NX as i32 {
                        let src_idx = Self::index(src_x as usize, src_y as usize, src_z as usize);
                        self.density[dst_idx] = self.density_scratch[src_idx];
                        self.glow[dst_idx] = self.glow_scratch[src_idx];
                        self.tint_r[dst_idx] = self.tint_r_scratch[src_idx];
                        self.tint_g[dst_idx] = self.tint_g_scratch[src_idx];
                        self.tint_b[dst_idx] = self.tint_b_scratch[src_idx];
                        self.u_x[dst_idx] = self.u_x_scratch[src_idx];
                        self.u_y[dst_idx] = self.u_y_scratch[src_idx];
                        self.u_z[dst_idx] = self.u_z_scratch[src_idx];
                    } else {
                        self.density[dst_idx] = 0.0;
                        self.glow[dst_idx] = 0.0;
                        self.tint_r[dst_idx] = 0.60;
                        self.tint_g[dst_idx] = 0.70;
                        self.tint_b[dst_idx] = 0.80;
                        self.u_x[dst_idx] = 0.0;
                        self.u_y[dst_idx] = 0.0;
                        self.u_z[dst_idx] = 0.0;
                    }
                }
            }
        }
    }

    /// Computes the flat 1D buffer index for a discrete voxel coordinate `(ix, iy, iz)`.
    #[inline]
    #[must_use]
    pub fn index(ix: usize, iy: usize, iz: usize) -> usize {
        ix + iy * GRID_NX + iz * (GRID_NX * GRID_NY)
    }

    /// Trilinearly samples scalar density at arbitrary world coordinates.
    #[must_use]
    pub fn sample_density_trilinear(&self, world_pos: Vec3) -> f32 {
        let gp = self.world_to_grid(world_pos);
        Self::sample_slab_trilinear(&self.density, gp)
    }

    /// Trilinearly samples the 3D fluid velocity vector at arbitrary world coordinates.
    #[must_use]
    pub fn sample_velocity_trilinear(&self, world_pos: Vec3) -> Vec3 {
        let gp = self.world_to_grid(world_pos);
        Vec3::new(
            Self::sample_slab_trilinear(&self.u_x, gp),
            Self::sample_slab_trilinear(&self.u_y, gp),
            Self::sample_slab_trilinear(&self.u_z, gp),
        )
    }

    /// Computes the magnitude of fluid vorticity `$\|\nabla \times \mathbf{u}\|$` at arbitrary world coordinates.
    #[must_use]
    pub fn sample_vorticity_magnitude(&self, world_pos: Vec3) -> f32 {
        let gp = self.world_to_grid(world_pos);
        let wx = Self::sample_slab_trilinear(&self.vorticity_x, gp);
        let wy = Self::sample_slab_trilinear(&self.vorticity_y, gp);
        let wz = Self::sample_slab_trilinear(&self.vorticity_z, gp);
        (wx * wx + wy * wy + wz * wz).sqrt()
    }

    /// Trilinearly samples the luminescent glow scalar at arbitrary world coordinates.
    #[must_use]
    pub fn sample_glow_trilinear(&self, world_pos: Vec3) -> f32 {
        let gp = self.world_to_grid(world_pos);
        Self::sample_slab_trilinear(&self.glow, gp)
    }

    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "fluid grid indices and counts stay far below the range of f32 and i32"
    )]
    fn trilinear_indices_and_weights(gp: Vec3) -> ([usize; 8], [f32; 8]) {
        let gx = gp.x.clamp(0.0, (GRID_NX - 1) as f32);
        let gy = gp.y.clamp(0.0, (GRID_NY - 1) as f32);
        let gz = gp.z.clamp(0.0, (GRID_NZ - 1) as f32);

        // PERFORMANCE RATIONALE:
        // Why: `f32::floor` is a call to `libm`'s `floorf` on the `x86_64` baseline (no SSE4.1
        // `roundss`), made three times per back-traced voxel; the C-18 `perf` run attributed
        // 0.45 ms per frame to it.
        // What: a truncating `as usize` cast replaces `floor() as usize`. The operands are clamped
        // to `[0, N-1]` just above, and for a non-negative `f32` (including `-0.0`) or a NaN,
        // truncation and floor yield the same `usize` (a NaN casts to 0 either way).
        // Trade-off: the equivalence rests on the clamp above; the debug assertions guard it, and
        // a negative operand would truncate toward zero where `floor` rounds down.
        debug_assert!(
            gx >= 0.0 || gx.is_nan(),
            "invariant: gx clamped to [0, NX-1]"
        );
        debug_assert!(
            gy >= 0.0 || gy.is_nan(),
            "invariant: gy clamped to [0, NY-1]"
        );
        debug_assert!(
            gz >= 0.0 || gz.is_nan(),
            "invariant: gz clamped to [0, NZ-1]"
        );
        let x0 = gx as usize;
        let y0 = gy as usize;
        let z0 = gz as usize;

        let x1 = (x0 + 1).min(GRID_NX - 1);
        let y1 = (y0 + 1).min(GRID_NY - 1);
        let z1 = (z0 + 1).min(GRID_NZ - 1);

        let fx = gx - x0 as f32;
        let fy = gy - y0 as f32;
        let fz = gz - z0 as f32;

        let w000 = (1.0 - fx) * (1.0 - fy) * (1.0 - fz);
        let w100 = fx * (1.0 - fy) * (1.0 - fz);
        let w010 = (1.0 - fx) * fy * (1.0 - fz);
        let w110 = fx * fy * (1.0 - fz);
        let w001 = (1.0 - fx) * (1.0 - fy) * fz;
        let w101 = fx * (1.0 - fy) * fz;
        let w011 = (1.0 - fx) * fy * fz;
        let w111 = fx * fy * fz;

        let indices = [
            Self::index(x0, y0, z0),
            Self::index(x1, y0, z0),
            Self::index(x0, y1, z0),
            Self::index(x1, y1, z0),
            Self::index(x0, y0, z1),
            Self::index(x1, y0, z1),
            Self::index(x0, y1, z1),
            Self::index(x1, y1, z1),
        ];
        let weights = [w000, w100, w010, w110, w001, w101, w011, w111];
        (indices, weights)
    }

    #[inline]
    fn sample_indexed(slab: &[f32], indices: &[usize; 8], weights: &[f32; 8]) -> f32 {
        let mut val = 0.0;
        for k in 0..8 {
            val += slab[indices[k]] * weights[k];
        }
        val
    }

    #[inline]
    fn sample_slab_trilinear(slab: &[f32], gp: Vec3) -> f32 {
        let (indices, weights) = Self::trilinear_indices_and_weights(gp);
        Self::sample_indexed(slab, &indices, &weights)
    }

    /// Resets all density, glow, tint, and velocity fields to initial quiescence.
    pub fn clear(&mut self) {
        self.u_x.fill(0.0);
        self.u_y.fill(0.0);
        self.u_z.fill(0.0);
        self.density.fill(0.0);
        self.glow.fill(0.0);
        self.vorticity_x.fill(0.0);
        self.vorticity_y.fill(0.0);
        self.vorticity_z.fill(0.0);
        self.vorticity_mag.fill(0.0);
        for i in 0..TOTAL_VOXELS {
            self.tint_r[i] = 0.28;
            self.tint_g[i] = 0.46;
            self.tint_b[i] = 0.68;
        }
        self.rgba_density_bytes.fill(0);
        self.rgba_velocity_bytes.fill(128);
    }

    /// Executes one full 3D Navier-Stokes step with bubble smog generation and bubble-through-smog influence.
    ///
    /// # Errors
    /// Returns [`SmogError::InvalidDeltaTime`] if `dt < 0.0` or non-finite.
    pub fn step(
        &mut self,
        sources: &[BubbleSmogSource],
        config: &SmogConfig,
        dt: f32,
    ) -> Result<SmogStepMetrics, SmogError> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(SmogError::InvalidDeltaTime(dt));
        }

        if dt == 0.0 {
            return Ok(SmogStepMetrics::default());
        }

        // Clamp dt for numerical stability
        let dt = dt.min(0.05);

        // 1. Bubble Smog Generation & Bubble-Through-Smog Influence
        let active_sources = self.inject_sources(sources, config, dt);

        // 2. Vorticity Confinement (Section 10.4 & 11.4: scaled by 0.25 to prevent turbulent plume breakup)
        if active_sources <= 32 {
            self.apply_vorticity_confinement(config.vorticity_confinement * 0.25, dt);
        }

        // 3. Velocity Self-Advection & Boundary Conditions
        self.advect_velocity(config.velocity_damping, dt);

        // 4. Incompressible Pressure Projection (Section 11.4: disabled to preserve compressible slipstream convergence)
        // PERFORMANCE & PHYSICS RATIONALE:
        // Why: Analytical slipstream convergence (u_converge) intentionally has negative divergence
        // (grad . u_converge < 0) to pull top and bottom boundary layers inward into a teardrop wake.
        // Projecting pressure solves grad^2 p = grad . u < 0 and subtracts grad p, which mathematically
        // cancels the inward aerodynamic convergence and puffs the wake into a rounded oval/mushroom.
        // Trade-off: Leaving slipstream velocity compressible preserves crisp teardrop wakes and saves
        // 4 full-grid Jacobi iterations per tick.

        // 5. Density & Chromatic Advection + Dissipation
        self.advect_density_and_tint(config.buoyancy, config.dissipation_rate, dt);

        // 6. Zero-Allocation RGBA8 3D Texture Packing & Metrics Accumulation
        let metrics = self.pack_textures_and_compute_metrics(active_sources);

        Ok(metrics)
    }

    /// Stage 1 of [`Self::step`]: every source evacuates its core, pushes the fluid around it and
    /// emits smog into its wake. Sources are applied in slice order, each seeing the field the
    /// previous ones left. Returns the number of sources applied.
    fn inject_sources(
        &mut self,
        sources: &[BubbleSmogSource],
        config: &SmogConfig,
        dt: f32,
    ) -> usize {
        let mut active_sources = 0usize;
        for source in sources {
            let frame = SourceFrame::new(source, sources.len());
            active_sources += 1;
            self.inject_source(&frame, config, dt);
        }
        active_sources
    }

    /// Inclusive voxel index ranges `[min, max]` per axis of the box a source can touch.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "fluid grid indices and counts stay far below the range of f32 and i32"
    )]
    fn source_voxel_box(&self, frame: &SourceFrame<'_>) -> [(usize, usize); 3] {
        let source = frame.source;
        let horiz_radius = frame.horiz_radius;
        let r_k = frame.r_k;
        // Tight bounding box in grid index space (Section 10.2 & 10.3: 8x fewer voxels visited)
        let p_min = self.world_to_grid(Vec3::new(
            source.position.x - horiz_radius,
            source.position.y - 0.85 * r_k,
            source.position.z - horiz_radius,
        ));
        let p_max = self.world_to_grid(Vec3::new(
            source.position.x + horiz_radius,
            source.position.y + 0.95 * r_k,
            source.position.z + horiz_radius,
        ));

        let ix_min = (p_min.x.floor() as isize).clamp(0, (GRID_NX - 1) as isize) as usize;
        let ix_max = (p_max.x.ceil() as isize).clamp(0, (GRID_NX - 1) as isize) as usize;
        let iy_min = (p_min.y.floor() as isize).clamp(0, (GRID_NY - 1) as isize) as usize;
        let iy_max = (p_max.y.ceil() as isize).clamp(0, (GRID_NY - 1) as isize) as usize;
        let iz_min = (p_min.z.floor() as isize).clamp(0, (GRID_NZ - 1) as isize) as usize;
        let iz_max = (p_max.z.ceil() as isize).clamp(0, (GRID_NZ - 1) as isize) as usize;
        [(ix_min, ix_max), (iy_min, iy_max), (iz_min, iz_max)]
    }

    /// Applies one source to every voxel of its box inside its spherical reach, in z, y, x order.
    /// Per voxel the four effects run in a fixed order: core evacuation, aerodynamic impulse,
    /// stationary settling, emission.
    fn inject_source(&mut self, frame: &SourceFrame<'_>, config: &SmogConfig, dt: f32) {
        let [(ix_min, ix_max), (iy_min, iy_max), (iz_min, iz_max)] = self.source_voxel_box(frame);
        for iz in iz_min..=iz_max {
            for iy in iy_min..=iy_max {
                for ix in ix_min..=ix_max {
                    let Some(voxel) = self.voxel_offset(frame, ix, iy, iz) else {
                        continue;
                    };
                    self.evacuate_core(frame, &voxel, dt);
                    self.apply_wake_impulse(frame, &voxel, config, dt);
                    self.apply_stationary_settling(frame, &voxel, config, dt);
                    self.emit_smog(frame, &voxel, config, dt);
                }
            }
        }
    }

    /// The voxel's offset from the source, or `None` outside the source's spherical reach.
    fn voxel_offset(
        &self,
        frame: &SourceFrame<'_>,
        ix: usize,
        iy: usize,
        iz: usize,
    ) -> Option<VoxelOffset> {
        let idx = Self::index(ix, iy, iz);
        let voxel_pos = self.grid_to_world(ix, iy, iz);
        let r_vec = voxel_pos - frame.source.position;
        let dist_sq = r_vec.length_squared();

        // Spherical Early-Out (Section 10.3: skip outside sphere before sqrt and dot products)
        if dist_sq > frame.max_r_sq {
            return None;
        }

        let r_k = frame.r_k;
        let dist = dist_sq.sqrt();
        let q = dist / r_k;
        let x_parallel = r_vec.dot(frame.h_hat);
        let q_par = x_parallel / r_k;
        let s_lat = r_vec.dot(frame.s_hat);
        let q_lat = s_lat.abs() / r_k;
        let q_y = r_vec.y.abs() / r_k;
        let r_hat_lat = if s_lat >= 0.0 {
            frame.s_hat
        } else {
            -frame.s_hat
        };
        Some(VoxelOffset {
            idx,
            r_vec,
            dist,
            q,
            q_par,
            q_lat,
            q_y,
            r_hat_lat,
        })
    }

    /// Interior Sphere Core Evacuation (Section 11.2: keeps interior flipbook clean without carving
    /// a split wake trench behind the creature).
    fn evacuate_core(&mut self, frame: &SourceFrame<'_>, voxel: &VoxelOffset, dt: f32) {
        let r_b = frame.r_b;
        let dist = voxel.dist;
        let idx = voxel.idx;
        if dist < 0.72 * r_b {
            let evac = (1.0 - (dist / (0.72 * r_b)).powi(2)).clamp(0.0, 1.0);
            let atten = (1.0 - 0.90 * evac * (dt * 36.0).min(1.0)).max(0.0);
            self.density[idx] = (self.density[idx] * atten).max(0.0);
        }
    }

    /// Aerodynamic Velocity Impulse (Section 11.4: front bow parting vs rear slipstream inward
    /// taper + trailing draft), for a moving source.
    fn apply_wake_impulse(
        &mut self,
        frame: &SourceFrame<'_>,
        voxel: &VoxelOffset,
        config: &SmogConfig,
        dt: f32,
    ) {
        let source = frame.source;
        if source.influence_strength > 0.0 && config.wake_influence > 0.0 && frame.speed > 1e-3 {
            let apply_impulse = source.emission_strength == 0.0 || voxel.dist >= frame.r_b * 0.70;
            if apply_impulse {
                let u_bow = bow_wave_velocity(frame, voxel);
                let (u_converge, u_draft, u_swirl) = slipstream_velocity(frame, voxel);

                let mut total_impulse = (u_bow + u_converge + u_draft + u_swirl)
                    * (dt * config.wake_influence * source.influence_strength);
                total_impulse.y *= 0.15;
                let idx = voxel.idx;
                self.u_x[idx] += total_impulse.x;
                self.u_y[idx] += total_impulse.y;
                self.u_z[idx] += total_impulse.z;
            }
        }
    }

    /// Stationary & Low-Speed Downward Mist Cascade + Basal Outward Dispersion.
    fn apply_stationary_settling(
        &mut self,
        frame: &SourceFrame<'_>,
        voxel: &VoxelOffset,
        config: &SmogConfig,
        dt: f32,
    ) {
        let source = frame.source;
        let r_k = frame.r_k;
        let dist = voxel.dist;
        let r_vec = voxel.r_vec;
        if source.influence_strength > 0.0 && config.wake_influence > 0.0 {
            let stationary_weight = (1.0 - (frame.speed / 0.45)).clamp(0.0, 1.0);
            if stationary_weight > 0.0 && dist < 1.35 * r_k && dist >= 0.62 * frame.r_b {
                let shell_prox = (1.0 - (dist / (1.35 * r_k)).min(1.0)).max(0.0);
                let r_horiz = Vec3::new(r_vec.x, 0.0, r_vec.z);
                let r_hat_radial = if r_horiz.length_squared() > 1e-6 {
                    r_horiz.normalize()
                } else {
                    Vec3::X
                };
                let rel_y = (r_vec.y / r_k).clamp(-1.0, 1.0);
                let vy_sink = -0.55 * (0.45 + 0.55 * shell_prox) * (1.0 - 0.3 * rel_y);
                let basal_t = ((0.15 - rel_y) / 0.85).clamp(0.0, 1.0);
                let v_radial = r_hat_radial * (0.22 * shell_prox * basal_t);
                let settle_impulse = (v_radial + Vec3::new(0.0, vy_sink, 0.0))
                    * (stationary_weight * dt * config.wake_influence * source.influence_strength);
                let idx = voxel.idx;
                self.u_x[idx] += settle_impulse.x;
                self.u_y[idx] += settle_impulse.y;
                self.u_z[idx] += settle_impulse.z;
            }
        }
    }

    /// Full-Diameter Rear-Hemisphere Rim & Core Emission + Stationary Basal Halo (Section 11.4):
    /// deposits density, blends the injected tint, and adds glow.
    fn emit_smog(
        &mut self,
        frame: &SourceFrame<'_>,
        voxel: &VoxelOffset,
        config: &SmogConfig,
        dt: f32,
    ) {
        let source = frame.source;
        if source.emission_strength > 0.0 && config.emission_rate > 0.0 {
            let w_emit = emission_weight(frame, voxel);
            if w_emit > 1e-4 {
                let idx = voxel.idx;
                let q = voxel.q;
                let q_lat = voxel.q_lat;
                let delta_rho = dt
                    * config.emission_rate
                    * source.emission_strength
                    * frame.speed_factor
                    * w_emit;

                let old_rho = self.density[idx];
                let new_rho = (old_rho + delta_rho).min(SMOG_DENSITY_MAX);
                self.density[idx] = new_rho;

                let sheath_w = (0.55 * q_lat * q_lat).clamp(0.0, 1.0);
                let c_inject = frame.c_b * (1.0 - sheath_w) + frame.c_sheath * sheath_w;

                let blend_alpha = (delta_rho / (new_rho + 1e-5)).clamp(0.0, 1.0);
                let color_alpha = (blend_alpha * (2.0 - blend_alpha)).clamp(0.0, 1.0);
                self.tint_r[idx] =
                    self.tint_r[idx] * (1.0 - color_alpha) + c_inject.x * color_alpha;
                self.tint_g[idx] =
                    self.tint_g[idx] * (1.0 - color_alpha) + c_inject.y * color_alpha;
                self.tint_b[idx] =
                    self.tint_b[idx] * (1.0 - color_alpha) + c_inject.z * color_alpha;

                let w_glow = (1.0 - (q / 1.12).powi(2)).max(0.0);
                let delta_glow = dt
                    * config.emission_rate
                    * source.emission_strength
                    * frame.speed_factor
                    * 1.85
                    * w_glow;
                self.glow[idx] = (self.glow[idx] + delta_glow).min(1.0);
            }
        }
    }

    fn apply_vorticity_confinement(&mut self, confinement_strength: f32, dt: f32) {
        if confinement_strength <= 0.0 {
            return;
        }

        let inv_2hx = 0.5 * self.inv_cell_size.x;
        let inv_2hy = 0.5 * self.inv_cell_size.y;
        let inv_2hz = 0.5 * self.inv_cell_size.z;

        // Compute curl omega = grad x u
        for iz in 1..(GRID_NZ - 1) {
            for iy in 1..(GRID_NY - 1) {
                for ix in 1..(GRID_NX - 1) {
                    let idx = Self::index(ix, iy, iz);

                    let duz_dy = (self.u_z[Self::index(ix, iy + 1, iz)]
                        - self.u_z[Self::index(ix, iy - 1, iz)])
                        * inv_2hy;
                    let duy_dz = (self.u_y[Self::index(ix, iy, iz + 1)]
                        - self.u_y[Self::index(ix, iy, iz - 1)])
                        * inv_2hz;

                    let dux_dz = (self.u_x[Self::index(ix, iy, iz + 1)]
                        - self.u_x[Self::index(ix, iy, iz - 1)])
                        * inv_2hz;
                    let duz_dx = (self.u_z[Self::index(ix + 1, iy, iz)]
                        - self.u_z[Self::index(ix - 1, iy, iz)])
                        * inv_2hx;

                    let duy_dx = (self.u_y[Self::index(ix + 1, iy, iz)]
                        - self.u_y[Self::index(ix - 1, iy, iz)])
                        * inv_2hx;
                    let dux_dy = (self.u_x[Self::index(ix, iy + 1, iz)]
                        - self.u_x[Self::index(ix, iy - 1, iz)])
                        * inv_2hy;

                    let wx = duz_dy - duy_dz;
                    let wy = dux_dz - duz_dx;
                    let wz = duy_dx - dux_dy;

                    self.vorticity_x[idx] = wx;
                    self.vorticity_y[idx] = wy;
                    self.vorticity_z[idx] = wz;
                    let w_sq = wx * wx + wy * wy + wz * wz;
                    self.vorticity_mag[idx] = if w_sq > 1e-8 { w_sq.sqrt() } else { 0.0 };
                }
            }
        }

        // Apply confinement force: f = eps * h * (N x omega)
        let avg_h = (self.cell_size.x + self.cell_size.y + self.cell_size.z) / 3.0;
        let coeff = dt * confinement_strength * avg_h;

        // PERFORMANCE RATIONALE:
        // Why: Evaluating (wx*wx + wy*wy + wz*wz).sqrt() 6 times per voxel inside the gradient loop
        // would perform ~500,000 redundant square root operations per frame at 82,944 voxels.
        // What: We precompute vorticity magnitude into self.vorticity_mag during the curl pass
        // and read it directly here with simple index lookups.
        // Trade-off: Uses one pre-allocated flat f32 slab (332 KB) to save CPU cycles in the physics tick.
        for iz in 2..(GRID_NZ - 2) {
            for iy in 2..(GRID_NY - 2) {
                for ix in 2..(GRID_NX - 2) {
                    let idx = Self::index(ix, iy, iz);

                    // Sparse Fast-Path (Section 10.3: skip near-zero vorticity cells before gradient math)
                    if self.vorticity_mag[idx] < 1e-4 {
                        continue;
                    }

                    let dmag_dx = (self.vorticity_mag[Self::index(ix + 1, iy, iz)]
                        - self.vorticity_mag[Self::index(ix - 1, iy, iz)])
                        * inv_2hx;
                    let dmag_dy = (self.vorticity_mag[Self::index(ix, iy + 1, iz)]
                        - self.vorticity_mag[Self::index(ix, iy - 1, iz)])
                        * inv_2hy;
                    let dmag_dz = (self.vorticity_mag[Self::index(ix, iy, iz + 1)]
                        - self.vorticity_mag[Self::index(ix, iy, iz - 1)])
                        * inv_2hz;

                    let grad_mag =
                        (dmag_dx * dmag_dx + dmag_dy * dmag_dy + dmag_dz * dmag_dz).sqrt() + 1e-5;
                    let nx = dmag_dx / grad_mag;
                    let ny = dmag_dy / grad_mag;
                    let nz = dmag_dz / grad_mag;

                    let wx = self.vorticity_x[idx];
                    let wy = self.vorticity_y[idx];
                    let wz = self.vorticity_z[idx];

                    // N x omega
                    let fx = ny * wz - nz * wy;
                    let fy = nz * wx - nx * wz;
                    let fz = nx * wy - ny * wx;

                    self.u_x[idx] += coeff * fx;
                    self.u_y[idx] += coeff * fy;
                    self.u_z[idx] += coeff * fz;
                }
            }
        }
    }

    fn advect_velocity(&mut self, damping: f32, dt: f32) {
        let decay = (-damping * dt).exp();

        // PERFORMANCE RATIONALE:
        // Why: u_x, u_y, and u_z share the exact same advective back-trace trajectory x_back = x - u(x)*dt.
        // What: We compute the back-trace grid coordinate and 8-corner trilinear weights once per voxel,
        // reusing them across all 3 velocity components instead of 3 independent transforms and weightings.
        // Trade-off: None; strictly eliminates 2/3 of the trilinear math in velocity advection.
        for iz in 0..GRID_NZ {
            let is_boundary_z = iz == 0 || iz == GRID_NZ - 1;
            for iy in 0..GRID_NY {
                let is_boundary_y = iy == 0 || iy == GRID_NY - 1;
                for ix in 0..GRID_NX {
                    let is_boundary_x = ix == 0 || ix == GRID_NX - 1;
                    let idx = Self::index(ix, iy, iz);

                    if is_boundary_x && is_boundary_y && is_boundary_z {
                        self.u_x_scratch[idx] = 0.0;
                        self.u_y_scratch[idx] = 0.0;
                        self.u_z_scratch[idx] = 0.0;
                        continue;
                    }

                    // Sparse Fast-Path (Section 10.3: zero velocity means back_pos == cur_pos, sampling zero yields zero)
                    let ux = self.u_x[idx];
                    let uy = self.u_y[idx];
                    let uz = self.u_z[idx];
                    if ux == 0.0 && uy == 0.0 && uz == 0.0 {
                        self.u_x_scratch[idx] = 0.0;
                        self.u_y_scratch[idx] = 0.0;
                        self.u_z_scratch[idx] = 0.0;
                        continue;
                    }

                    let cur_pos = self.grid_to_world(ix, iy, iz);
                    let back_pos = cur_pos - Vec3::new(ux, uy, uz) * dt;
                    let gp = self.world_to_grid(back_pos);
                    let (indices, weights) = Self::trilinear_indices_and_weights(gp);

                    self.u_x_scratch[idx] = if is_boundary_x {
                        0.0
                    } else {
                        Self::sample_indexed(&self.u_x, &indices, &weights) * decay
                    };

                    self.u_y_scratch[idx] = if is_boundary_y {
                        0.0
                    } else {
                        Self::sample_indexed(&self.u_y, &indices, &weights) * decay
                    };

                    self.u_z_scratch[idx] = if is_boundary_z {
                        0.0
                    } else {
                        Self::sample_indexed(&self.u_z, &indices, &weights) * decay
                    };
                }
            }
        }

        // PERFORMANCE RATIONALE:
        // Why: copying the three scratch slabs back into the fields moves 1 MB per step (3 x
        // 331,776 bytes read and written), and the solver is bound by memory traffic when the
        // machine is busy (C-18; `memmove` took 1.9 ms per frame in the `perf` run).
        // What: the fields and their scratch slabs exchange their boxes (`std::mem::swap`, a
        // pointer swap); after it the scratch slabs hold the previous step's field, which no pass
        // reads before overwriting it.
        // Trade-off: correct only because the loop above writes every voxel of all three scratch
        // slabs (it covers `0..N` on every axis, and each branch writes all three components);
        // `advection_passes_write_every_scratch_voxel` pins that coverage.
        std::mem::swap(&mut self.u_x, &mut self.u_x_scratch);
        std::mem::swap(&mut self.u_y, &mut self.u_y_scratch);
        std::mem::swap(&mut self.u_z, &mut self.u_z_scratch);
    }

    fn advect_density_and_tint(&mut self, buoyancy: f32, dissipation: f32, dt: f32) {
        let decay = (-dissipation * dt).exp();
        let lambda_glow = (1.8 * dissipation).max(1.65);
        let glow_decay = (-lambda_glow * dt).exp();
        let c_tail = Vec3::new(0.28, 0.46, 0.68);
        let buoyancy_vel = Vec3::new(0.0, buoyancy, 0.0);

        // PERFORMANCE RATIONALE:
        // Why: Density, luminescent glow, and RGB chromatic tint (5 scalar slabs) are advected along the same trajectory.
        // What: We compute the 8-corner indices and trilinear weights once per voxel and evaluate all 5 slabs in a fused pass.
        // Additionally, we early-out when all 8 corners have zero density and zero glow, skipping 5 trilinear inner products.
        // Trade-off: None; reduces per-voxel arithmetic by ~75% across the 82,944 grid voxels.
        for iz in 0..GRID_NZ {
            for iy in 0..GRID_NY {
                let iy_up = (iy + 1).min(GRID_NY - 1);
                let iy_dn = iy.saturating_sub(1);
                for ix in 0..GRID_NX {
                    let idx = Self::index(ix, iy, iz);

                    // Sparse Fast-Path (Section 10.3: skip dormant, non-moving cells before world-to-grid math)
                    if self.density[idx] < 1e-6
                        && self.glow[idx] < 1e-6
                        && self.density[Self::index(ix, iy_up, iz)] < 1e-6
                        && self.density[Self::index(ix, iy_dn, iz)] < 1e-6
                        && self.u_x[idx].abs() < 1e-4
                        && self.u_y[idx].abs() < 1e-4
                        && self.u_z[idx].abs() < 1e-4
                    {
                        self.density_scratch[idx] = 0.0;
                        self.glow_scratch[idx] = 0.0;
                        self.tint_r_scratch[idx] = c_tail.x;
                        self.tint_g_scratch[idx] = c_tail.y;
                        self.tint_b_scratch[idx] = c_tail.z;
                        continue;
                    }

                    let cur_pos = self.grid_to_world(ix, iy, iz);
                    let eff_vel =
                        Vec3::new(self.u_x[idx], self.u_y[idx], self.u_z[idx]) + buoyancy_vel;
                    let back_pos = cur_pos - eff_vel * dt;
                    let gp = self.world_to_grid(back_pos);

                    let (indices, weights) = Self::trilinear_indices_and_weights(gp);

                    let max_corner = indices.iter().fold(0.0_f32, |acc, &k| {
                        acc.max(self.density[k]).max(self.glow[k])
                    });

                    if max_corner < 1e-6 {
                        self.density_scratch[idx] = 0.0;
                        self.glow_scratch[idx] = 0.0;
                        self.tint_r_scratch[idx] = c_tail.x;
                        self.tint_g_scratch[idx] = c_tail.y;
                        self.tint_b_scratch[idx] = c_tail.z;
                        continue;
                    }

                    let sampled_density = Self::sample_indexed(&self.density, &indices, &weights);
                    // Clean Haze Floor Subtraction (Section 10.5 & 11.5: evaporates background haze cleanly to 0.0 while preserving longer slipstream tails)
                    let floor_decay = if dissipation > 0.0 { 0.018 * dt } else { 0.0 };
                    self.density_scratch[idx] =
                        (sampled_density * decay - floor_decay).clamp(0.0, SMOG_DENSITY_MAX);

                    let sampled_glow = Self::sample_indexed(&self.glow, &indices, &weights);
                    self.glow_scratch[idx] = (sampled_glow * glow_decay).clamp(0.0, 1.0);

                    // Density-weighted trilinear color advection: avoids unweighted blending with empty ambient voxels
                    let mut color_w_sum = 0.0_f32;
                    let mut adv_r = 0.0_f32;
                    let mut adv_g = 0.0_f32;
                    let mut adv_b = 0.0_f32;
                    for k in 0..8 {
                        let cw = weights[k] * (self.density[indices[k]] + 1e-5);
                        color_w_sum += cw;
                        adv_r += self.tint_r[indices[k]] * cw;
                        adv_g += self.tint_g[indices[k]] * cw;
                        adv_b += self.tint_b[indices[k]] * cw;
                    }
                    let inv_cw = 1.0 / color_w_sum.max(1e-8);
                    adv_r *= inv_cw;
                    adv_g *= inv_cw;
                    adv_b *= inv_cw;

                    let luma = (adv_r + adv_g + adv_b) * (1.0 / 3.0);
                    self.tint_r_scratch[idx] = (luma + (adv_r - luma) * 1.12).clamp(0.0, 1.0);
                    self.tint_g_scratch[idx] = (luma + (adv_g - luma) * 1.12).clamp(0.0, 1.0);
                    self.tint_b_scratch[idx] = (luma + (adv_b - luma) * 1.12).clamp(0.0, 1.0);
                }
            }
        }

        // PERFORMANCE RATIONALE:
        // Why: copying the five scratch slabs back moves 1.66 MB per step (5 x 331,776 bytes read
        // and written) through a solver that is bound by memory traffic under load (C-18).
        // What: each field exchanges its box with its scratch slab (`std::mem::swap`); the scratch
        // slabs then hold the previous step's values, which no pass reads before overwriting them
        // (`set_bounds_quantized` copies the fields into them first).
        // Trade-off: correct only because the loop above writes every voxel of all five scratch
        // slabs (it covers `0..N` on every axis, and each of its three exits writes all five);
        // `advection_passes_write_every_scratch_voxel` pins that coverage.
        std::mem::swap(&mut self.density, &mut self.density_scratch);
        std::mem::swap(&mut self.glow, &mut self.glow_scratch);
        std::mem::swap(&mut self.tint_r, &mut self.tint_r_scratch);
        std::mem::swap(&mut self.tint_g, &mut self.tint_g_scratch);
        std::mem::swap(&mut self.tint_b, &mut self.tint_b_scratch);
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "fluid grid indices and counts stay far below the range of f32 and i32"
    )]
    fn pack_textures_and_compute_metrics(&mut self, active_sources: usize) -> SmogStepMetrics {
        let mut total_density = 0.0_f32;
        let mut peak_density = 0.0_f32;
        let mut max_speed_sq = 0.0_f32;
        let mut max_vorticity_sq = 0.0_f32;

        let inv_v_max_2 = 1.0 / (2.0 * SMOG_PACK_VELOCITY_MAX);

        for i in 0..TOTAL_VOXELS {
            let rho = self.density[i];
            total_density += rho;
            if rho > peak_density {
                peak_density = rho;
            }

            let byte_idx = i * 4;
            if rho <= 1e-5 {
                self.rgba_density_bytes[byte_idx + 3] = 0;
            } else {
                let r_byte = (self.tint_r[i] * 255.0).clamp(0.0, 255.0) as u8;
                let g_byte = (self.tint_g[i] * 255.0).clamp(0.0, 255.0) as u8;
                let b_byte = (self.tint_b[i] * 255.0).clamp(0.0, 255.0) as u8;
                let a_byte = ((rho / SMOG_DENSITY_MAX) * 255.0).clamp(0.0, 255.0) as u8;

                self.rgba_density_bytes[byte_idx] = r_byte;
                self.rgba_density_bytes[byte_idx + 1] = g_byte;
                self.rgba_density_bytes[byte_idx + 2] = b_byte;
                self.rgba_density_bytes[byte_idx + 3] = a_byte;
            }

            // Pack Velocity + Vorticity Magnitude / Glow -> rgba_velocity_bytes
            let vx = self.u_x[i];
            let vy = self.u_y[i];
            let vz = self.u_z[i];
            let speed_sq = vx * vx + vy * vy + vz * vz;
            if speed_sq > max_speed_sq {
                max_speed_sq = speed_sq;
            }

            let vort_mag = self.vorticity_mag[i];
            let vort_sq = vort_mag * vort_mag;
            if vort_sq > max_vorticity_sq {
                max_vorticity_sq = vort_sq;
            }

            if speed_sq <= 1e-6 && vort_sq <= 1e-6 && self.glow[i] <= 1e-5 {
                self.rgba_velocity_bytes[byte_idx] = 128;
                self.rgba_velocity_bytes[byte_idx + 1] = 128;
                self.rgba_velocity_bytes[byte_idx + 2] = 128;
                self.rgba_velocity_bytes[byte_idx + 3] = 0;
            } else {
                let vx_byte = ((vx * inv_v_max_2 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
                let vy_byte = ((vy * inv_v_max_2 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
                let vz_byte = ((vz * inv_v_max_2 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
                let combined_glow = (self.glow[i] + 0.035 * vort_mag).clamp(0.0, 1.0);
                let glow_byte = (combined_glow * 255.0).clamp(0.0, 255.0) as u8;

                self.rgba_velocity_bytes[byte_idx] = vx_byte;
                self.rgba_velocity_bytes[byte_idx + 1] = vy_byte;
                self.rgba_velocity_bytes[byte_idx + 2] = vz_byte;
                self.rgba_velocity_bytes[byte_idx + 3] = glow_byte;
            }
        }

        SmogStepMetrics {
            total_density,
            peak_density,
            max_speed: max_speed_sq.sqrt(),
            max_vorticity: max_vorticity_sq.sqrt(),
            active_sources,
        }
    }
}

/// Per-source quantities shared by every voxel a bubble touches during one injection pass.
///
/// All lengths are world units; `speed` is world units per second; `h_hat` and `s_hat` are the
/// horizontal heading and its horizontal left-hand side, both unit length with `y = 0`.
struct SourceFrame<'a> {
    source: &'a BubbleSmogSource,
    speed: f32,
    h_hat: Vec3,
    s_hat: Vec3,
    /// Bubble radius.
    r_b: f32,
    /// Smog kernel radius: the bubble radius widened, with a floor.
    r_k: f32,
    /// Horizontal reach of the injection box around the source centre.
    horiz_radius: f32,
    max_r_sq: f32,
    /// Core emission tint (the team colour).
    c_b: Vec3,
    /// Rim emission tint: the team colour's channels rotated.
    c_sheath: Vec3,
    /// Emission multiplier from speed, in `[0.42, 5.2]`.
    speed_factor: f32,
}

impl<'a> SourceFrame<'a> {
    /// Precomputes the frame of `source`. `source_count` is the number of sources this step; a
    /// crowd of more than 32 uses a tighter wake radius.
    fn new(source: &'a BubbleSmogSource, source_count: usize) -> Self {
        let speed = source.velocity.length();
        let heading = if speed > 1e-4 {
            source.velocity / speed
        } else {
            Vec3::X
        };

        let h_horiz = Vec3::new(heading.x, 0.0, heading.z);
        let h_hat = if h_horiz.length_squared() > 1e-6 {
            h_horiz.normalize()
        } else {
            Vec3::X
        };
        let s_hat = Vec3::new(-h_hat.z, 0.0, h_hat.x);

        let r_b = source.radius;
        let r_k = (source.radius * 1.15).max(0.56);
        let horiz_radius = if source_count > 32 {
            (r_k * 1.15).max(0.40)
        } else {
            (r_k * 1.65).max(0.55)
        };
        let max_r_sq = horiz_radius * horiz_radius;

        // Precompute multi-tone spectrum colors for this source
        let c_b = source.team_color;
        let c_sheath = Vec3::new(c_b.z, c_b.x, c_b.y);

        let speed_factor = (0.42 + speed.min(6.0) * 1.15).clamp(0.42, 5.2);

        Self {
            source,
            speed,
            h_hat,
            s_hat,
            r_b,
            r_k,
            horiz_radius,
            max_r_sq,
            c_b,
            c_sheath,
            speed_factor,
        }
    }
}

/// Geometry of one voxel relative to a source, in the source's heading frame.
///
/// `q*` values are distances divided by the kernel radius `r_k`: `q` radial, `q_par` signed
/// along the heading (positive ahead), `q_lat` unsigned to the side, `q_y` unsigned vertical.
struct VoxelOffset {
    idx: usize,
    r_vec: Vec3,
    dist: f32,
    q: f32,
    q_par: f32,
    q_lat: f32,
    q_y: f32,
    /// Horizontal unit vector from the wake axis toward this voxel's side.
    r_hat_lat: Vec3,
}

/// Front Bow-Wave Parting (`q_par > 0.02`, `dist < 1.45 r_k`): velocity, in world units per
/// second, that pushes fluid ahead of the bubble out to its sides.
fn bow_wave_velocity(frame: &SourceFrame<'_>, voxel: &VoxelOffset) -> Vec3 {
    let r_k = frame.r_k;
    let dist = voxel.dist;
    if voxel.q_par > 0.02 && dist < 1.45 * r_k {
        let falloff = (1.0 - (dist / (1.45 * r_k)).min(1.0)).powi(2);
        let perp_factor = (voxel.q_lat / 0.35).min(1.0);
        (voxel.r_hat_lat * (1.85 * perp_factor) + frame.h_hat * 0.25) * (frame.speed * falloff)
    } else {
        Vec3::ZERO
    }
}

/// Rear Slipstream Inward Taper & Trailing Draft (`q_par` in `-1.65..=0.10`, `dist < 1.65 r_k`):
/// the `(convergence, draft, swirl)` velocities, in world units per second, behind the bubble.
fn slipstream_velocity(frame: &SourceFrame<'_>, voxel: &VoxelOffset) -> (Vec3, Vec3, Vec3) {
    let source = frame.source;
    let speed = frame.speed;
    let r_k = frame.r_k;
    let dist = voxel.dist;
    let q = voxel.q;
    let q_par = voxel.q_par;
    let q_lat = voxel.q_lat;
    if (-1.65..=0.10).contains(&q_par) && dist < 1.65 * r_k {
        let drag_t = (1.0 - (dist / (1.65 * r_k)).min(1.0)).max(0.0);
        let t_rear = ((0.10 - q_par) / (0.10 - (-0.18))).clamp(0.0, 1.0);
        let rear_step = t_rear * t_rear * (3.0 - 2.0 * t_rear);

        // Strong inward convergence pulling top & bottom boundary layers toward wake axis (q_lat -> 0)
        let rim_inward = (q_lat / 0.72).clamp(0.0, 1.15) * rear_step;
        let u_conv = -voxel.r_hat_lat * (1.95 * speed * drag_t * rim_inward);

        // Gentle forward draft near the emitting rear shell (q_par > -0.65) so mist trails out cleanly
        // into a long tapered cone instead of bunching on the bubble, and strong forward draft in the
        // trailing wake (q_par <= -0.65) or outer shear (q > 0.98) so crossing bubbles pull mist strongly!
        let core_focus = (1.0 - (q_lat / 0.90).powi(2)).max(0.0);
        let wake_dist_t = ((-q_par - 0.35) / 0.55).clamp(0.0, 1.0);
        let shear_t = ((q - 0.92) / 0.25).clamp(0.0, 1.0);
        let draft_scale = if source.emission_strength == 0.0 {
            0.65
        } else {
            0.14 + 0.48 * wake_dist_t.max(shear_t)
        };
        let u_dr =
            source.velocity * (draft_scale * drag_t * (0.35 + 0.65 * core_focus) * rear_step);

        let u_sw = Vec3::new(0.0, (0.18 * speed * drag_t * (0.5 - q_lat)).max(0.0), 0.0);
        (u_conv, u_dr, u_sw)
    } else {
        (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO)
    }
}

/// Emission weight of a voxel: the rear-hemisphere rim shell for a moving source, blended toward
/// the basal halo as the source slows to a stop. Dimensionless, `>= 0`.
fn emission_weight(frame: &SourceFrame<'_>, voxel: &VoxelOffset) -> f32 {
    let r_k = frame.r_k;
    let dist = voxel.dist;
    let q = voxel.q;
    let q_lat = voxel.q_lat;
    let shell_half_w = 0.16_f32;
    let shell_diff = (q - 0.84) / shell_half_w;
    let shell_profile = (1.0 - shell_diff * shell_diff).max(0.0);

    // cos_theta: -1.0 at rear pole, 0.0 at equatorial rim, +1.0 at front nose
    let cos_theta = (voxel.q_par / q.max(0.1)).clamp(-1.0, 1.0);
    let t_arc = ((0.14 - cos_theta) / (0.14 - (-0.30))).clamp(0.0, 1.0);
    let rear_arc = t_arc * t_arc * (3.0 - 2.0 * t_arc);
    let vert_profile = (1.0 - (voxel.q_y / 0.82).powi(2)).max(0.0);
    let lat_env = (1.0 - (q_lat / 0.96).powi(4)).max(0.0);
    // Rim boundary-layer emphasis so top and bottom edges visibly feed into the slipstream
    // while keeping the rear core (>= 0.82) completely unified with zero split:
    let rim_sheath = 0.82 + 0.38 * (q_lat / 0.85).clamp(0.0, 1.0);

    let w_moving = if dist <= 1.02 * r_k && q >= 0.66 {
        shell_profile * rear_arc * vert_profile * lat_env * rim_sheath
    } else {
        0.0
    };

    let basal_diff = (q - 0.86) / 0.22;
    let w_basal = if dist <= 1.08 * r_k {
        0.62 * (1.0 - basal_diff * basal_diff).max(0.0).powi(2) * vert_profile
    } else {
        0.0
    };

    let move_weight = ((frame.speed - 0.05) / 0.10).clamp(0.0, 1.0);
    move_weight * w_moving + (1.0 - move_weight) * w_basal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smog_grid_initialization_invariants() -> Result<(), SmogError> {
        let grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 4.0, 4.5))?;
        assert_eq!(grid.density.len(), TOTAL_VOXELS);
        assert_eq!(grid.rgba_density_bytes.len(), TOTAL_VOXELS * 4);
        assert_eq!(grid.rgba_velocity_bytes.len(), TOTAL_VOXELS * 4);

        // Check trilinear sampling of empty grid returns 0
        assert_eq!(grid.sample_density_trilinear(Vec3::new(0.0, 2.0, 0.0)), 0.0);
        assert_eq!(
            grid.sample_velocity_trilinear(Vec3::new(0.0, 2.0, 0.0)),
            Vec3::ZERO
        );

        Ok(())
    }

    #[test]
    fn test_smog_generation_from_bubble() -> Result<(), SmogError> {
        let mut grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 4.0, 4.5))?;
        let config = SmogConfig {
            enabled: true,
            bouncing_active: true,
            bounds: super::super::SmogArenaBounds::default(),
            emission_rate: 2.5,
            wake_influence: 1.0,
            dissipation_rate: 0.01,
            vorticity_confinement: 0.2,
            buoyancy: 0.0,
            velocity_damping: 0.05,
        };

        let bubble_pos = Vec3::new(0.0, 0.55, 0.0);
        let sources = [BubbleSmogSource {
            position: bubble_pos,
            velocity: Vec3::new(2.5, 0.0, 0.0),
            radius: 0.55,
            team_color: Vec3::new(0.2, 0.8, 0.4),
            emission_strength: 2.0,
            influence_strength: 1.0,
        }];

        let shoulder_pos = bubble_pos + Vec3::new(0.0, 0.0, 0.55 * 0.88);

        // Initially density is zero
        assert_eq!(grid.sample_density_trilinear(shoulder_pos), 0.0);

        // Step for 30 frames
        let mut prev_density = 0.0;
        for frame in 0..30 {
            let metrics = grid.step(&sources, &config, 1.0 / 60.0)?;
            assert!(metrics.total_density > 0.0);
            let cur_density = grid.sample_density_trilinear(shoulder_pos);
            if frame < 20 {
                assert!(
                    cur_density >= prev_density,
                    "Smog density must strictly increase around active emitting bubble shoulder (cur={cur_density}, prev={prev_density})"
                );
            }
            prev_density = cur_density;
        }

        assert!(
            prev_density > 0.02,
            "After 30 frames of emission, shoulder density should be significant (got {prev_density})"
        );

        // Verify core clearance protects creature flipbook inside bubble
        let core_density = grid.sample_density_trilinear(bubble_pos);
        assert!(
            core_density < 0.01,
            "Bubble core must remain clear for flipbook visibility (got {core_density})"
        );

        Ok(())
    }

    /// Bit pattern no solver pass produces: a NaN with a recognisable payload.
    const SCRATCH_SENTINEL: u32 = 0x7fc0_dead;

    fn poison(slab: &mut [f32]) {
        slab.fill(f32::from_bits(SCRATCH_SENTINEL));
    }

    fn assert_unpoisoned(name: &str, slab: &[f32]) {
        let stale = slab
            .iter()
            .filter(|v| v.to_bits() == SCRATCH_SENTINEL)
            .count();
        assert_eq!(
            stale, 0,
            "{name}: {stale} voxels were not written by the pass"
        );
    }

    /// The precondition of the ping-pong swaps in `advect_velocity` and
    /// `advect_density_and_tint`: each pass writes every voxel of every scratch slab it swaps
    /// in, so no stale value from an earlier step can surface in a field. The scratch slabs are
    /// filled with a sentinel before each pass; after the swap the fields must hold none of it.
    #[test]
    fn advection_passes_write_every_scratch_voxel() -> Result<(), SmogError> {
        let mut grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 4.0, 4.5))?;
        let config = SmogConfig::default();
        // A moving and a resting source, so the field holds moving smog, still smog, moving
        // clear fluid and fluid at exact rest: every branch of both passes is taken.
        let sources = [
            BubbleSmogSource {
                position: Vec3::new(-2.0, 0.6, 0.0),
                velocity: Vec3::new(2.5, 0.0, 1.0),
                radius: 0.55,
                team_color: Vec3::new(0.2, 0.8, 0.4),
                emission_strength: 2.0,
                influence_strength: 1.0,
            },
            BubbleSmogSource {
                position: Vec3::new(3.0, 0.6, 2.0),
                velocity: Vec3::ZERO,
                radius: 0.6,
                team_color: Vec3::new(0.9, 0.3, 0.2),
                emission_strength: 1.5,
                influence_strength: 1.0,
            },
        ];
        for _ in 0..20 {
            grid.step(&sources, &config, 1.0 / 60.0)?;
        }
        // Velocity at a grid corner and on a boundary face exercises the boundary branches.
        let corner = SmogGrid3d::index(0, 0, 0);
        let face = SmogGrid3d::index(GRID_NX - 1, 5, 30);
        grid.u_x[corner] = 0.5;
        grid.u_z[face] = -0.5;
        let moving = grid.u_x.iter().filter(|v| **v != 0.0).count();
        assert!(
            moving > 0 && moving < TOTAL_VOXELS,
            "both velocity branches taken"
        );
        let smoky = grid.density.iter().filter(|v| **v > 1e-6).count();
        assert!(
            smoky > 0 && smoky < TOTAL_VOXELS,
            "both density branches taken"
        );

        for slab in [
            &mut grid.u_x_scratch,
            &mut grid.u_y_scratch,
            &mut grid.u_z_scratch,
        ] {
            poison(slab);
        }
        grid.advect_velocity(config.velocity_damping, 1.0 / 60.0);
        assert_unpoisoned("u_x", &grid.u_x);
        assert_unpoisoned("u_y", &grid.u_y);
        assert_unpoisoned("u_z", &grid.u_z);

        for slab in [
            &mut grid.density_scratch,
            &mut grid.glow_scratch,
            &mut grid.tint_r_scratch,
            &mut grid.tint_g_scratch,
            &mut grid.tint_b_scratch,
        ] {
            poison(slab);
        }
        grid.advect_density_and_tint(config.buoyancy, config.dissipation_rate, 1.0 / 60.0);
        assert_unpoisoned("density", &grid.density);
        assert_unpoisoned("glow", &grid.glow);
        assert_unpoisoned("tint_r", &grid.tint_r);
        assert_unpoisoned("tint_g", &grid.tint_g);
        assert_unpoisoned("tint_b", &grid.tint_b);
        Ok(())
    }

    /// Reference for the truncating cast: the original `floor()` form.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "mirrors trilinear_indices_and_weights"
    )]
    fn floor_index(g: f32, n: usize) -> usize {
        g.clamp(0.0, (n - 1) as f32).floor() as usize
    }

    /// The truncating cast in `trilinear_indices_and_weights` selects the same corner as the
    /// `floor()` it replaced for every operand class: negative, `-0.0`, NaN, infinite, just
    /// below and above an integer, and beyond the far edge.
    #[test]
    fn truncating_cast_matches_floor_on_clamped_operands() {
        let probes = [
            -1.0e9,
            -3.5,
            -f32::MIN_POSITIVE,
            -0.0,
            0.0,
            f32::MIN_POSITIVE,
            0.999_999_94,
            1.0,
            5.5,
            10.999_999,
            11.0,
            70.5,
            71.0,
            94.999_99,
            95.0,
            1.0e9,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ];
        for &px in &probes {
            for &(py, pz) in &[(px, px), (0.25, 70.75), (10.5, -0.0)] {
                let gp = Vec3::new(px, py, pz);
                let (indices, weights) = SmogGrid3d::trilinear_indices_and_weights(gp);
                let expected = SmogGrid3d::index(
                    floor_index(px, GRID_NX),
                    floor_index(py, GRID_NY),
                    floor_index(pz, GRID_NZ),
                );
                assert_eq!(indices[0], expected, "corner of {gp:?}");
                if !px.is_nan() && !py.is_nan() && !pz.is_nan() {
                    assert!(weights.iter().all(|w| w.is_finite()), "weights of {gp:?}");
                }
            }
        }
    }

    #[test]
    fn test_bubble_moving_through_existing_smog_influence() -> Result<(), SmogError> {
        let mut grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 4.0, 4.5))?;
        let config = SmogConfig {
            enabled: true,
            bouncing_active: true,
            bounds: super::super::SmogArenaBounds::default(),
            emission_rate: 0.0, // Emission disabled to strictly isolate obstacle influence!
            wake_influence: 3.0,
            dissipation_rate: 0.0,
            vorticity_confinement: 0.25,
            buoyancy: 0.0,
            velocity_damping: 0.02,
        };

        // Populate a uniform ambient smog slab of density 1.5 in the center
        let center = Vec3::new(0.0, 2.0, 0.0);
        let r_bubble = 0.6;

        for iz in 0..GRID_NZ {
            for iy in 0..GRID_NY {
                for ix in 0..GRID_NX {
                    let p = grid.grid_to_world(ix, iy, iz);
                    if (p - center).length() < 2.5 {
                        let idx = SmogGrid3d::index(ix, iy, iz);
                        grid.density[idx] = 1.5;
                    }
                }
            }
        }

        let pre_core_density = grid.sample_density_trilinear(center);
        assert!(
            (pre_core_density - 1.5).abs() < 0.1,
            "Initial ambient density at center should be ~1.5 (got {pre_core_density})"
        );

        // Advance a bubble directly through center with high velocity along +X
        let vel = Vec3::new(3.5, 0.0, 0.0);
        let sources = [BubbleSmogSource {
            position: center,
            velocity: vel,
            radius: r_bubble,
            team_color: Vec3::ONE,
            emission_strength: 0.0, // Strict zero emission
            influence_strength: 2.5,
        }];

        // Step grid once with bubble traversing the smog
        grid.step(&sources, &config, 1.0 / 30.0)?;

        // Invariant 1: Physical Cavity Evacuation (Core density drops)
        let post_core_density = grid.sample_density_trilinear(center);
        assert!(
            post_core_density < pre_core_density,
            "Bubble core density must be evacuated (before: {pre_core_density}, after: {post_core_density})"
        );

        // Invariant 2: Momentum Drag Injection (Fluid velocity along bubble path acquires +X component)
        let fluid_vel_at_center = grid.sample_velocity_trilinear(center);
        assert!(
            fluid_vel_at_center.x > 0.05,
            "Fluid along bubble path must acquire positive forward velocity along v_b (got {fluid_vel_at_center:?})"
        );

        // Invariant 3: Bow-Wave Compression (Ahead of the bubble, density is compressed higher than ambient)
        let bow_pos = center + Vec3::new(r_bubble * 1.25, 0.0, 0.0);
        let bow_density = grid.sample_density_trilinear(bow_pos);
        assert!(
            bow_density >= 1.5 - 1e-4,
            "Bow wave ahead of advancing bubble must be compressed (got {bow_density})"
        );

        // Invariant 4: Wake Vortex Shedding (Behind the bubble, vorticity is strictly positive)
        let wake_pos = center - Vec3::new(r_bubble * 1.1, 0.2, 0.0);
        let wake_vort = grid.sample_vorticity_magnitude(wake_pos);
        assert!(
            wake_vort > 0.01,
            "Vorticity behind moving bubble wake must be strictly non-zero (got {wake_vort})"
        );

        Ok(())
    }
}
