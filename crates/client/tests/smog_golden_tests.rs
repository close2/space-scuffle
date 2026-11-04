//! Bit-exact characterisation goldens for the smog fluid solver, the bouncing-bubble kinematics and
//! the two smog Bevy systems.
//!
//! # Responsibility
//! Pins the exact output of [`SmogGrid3d::step`], [`step_bouncing_kinematics`],
//! `setup_smog_volume_system` and `update_smog_field_system` so that the structural splits of
//! `crates/client/src/smog/{mod,fluid,kinematics}.rs` (Phase 9, WP7, step 9.2) are provably
//! behaviour-preserving. Every hashed `f32` is hashed by its raw bits (`to_bits`), so a reordered
//! float expression, a reordered solver stage or a reordered collision pass changes the hash and
//! fails the test.
//!
//! # Two classes of golden
//! The client may use the host's `libm` (charter §3.1 binds only the simulation crates), and the
//! last bit of `sinf`, `cosf`, `atan2f`, `expf` and `powf` differs between glibc versions, musl,
//! macOS and Windows. A golden that hashes a `libm` result pins the host as well as the code, so
//! every golden here belongs to one of two classes (plan
//! `docs/plans/2026-11-03-host-independent-smog-goldens.md`):
//!
//! - **Class H, host-independent.** No `libm` result reaches the hash. Values that `libm` computes
//!   are either supplied as recorded literals ([`smog_crowd_trig::CROWD_TRIG_BITS`] for the crowd
//!   generator's trig) or left out of the hash and checked by an exact relation that holds on every
//!   host: a heading equals this host's `atan2f` of the final velocity, bit for bit, and a showcase
//!   rotation equals this host's `Quat::from_rotation_y(-heading)`. The four kinematics goldens are
//!   class H.
//! - **Class L, `libm`-bound.** Production code calls `libm` on the hashed path, and a test cannot
//!   remove the call without changing smog output. Each class-L golden names its functions in its
//!   doc comment: the fluid goldens use `expf` (and `sinf` through `bubble_team_color` beyond the
//!   palette); the startup and system goldens use `expf` and `powf` (`Color::to_linear`), the
//!   system goldens with battle or preview bubbles also `sinf`; the noise golden uses `sinf`,
//!   `cosf` and `powf`. [`smog_golden_libm_canary`] pins the exact `libm` results these goldens
//!   consume, so a host with a different `libm` fails the canary with a message saying so rather
//!   than looking like a behaviour change. Making them host-independent is owner question
//!   `docs/questions/open/0014-host-independent-smog-math.md`.
//!
//! # Fit
//! The fluid and kinematics goldens run on the CPU only, through the public smog API: the fluid
//! fields are read back by sampling every voxel centre and by the packed RGBA8 upload buffers. The
//! system goldens run the production `SmogPlugin` in a headless `App` with a manual clock, with no
//! window and no GPU. They complement the semantic tests in `smog_bouncing_bubbles_tests.rs`, which
//! accept many fields; the goldens accept exactly one.
//!
//! # Coverage
//! - Fluid: the 10-bubble showcase (vorticity confinement on), a 24-source mix and a 48-source
//!   crowd (confinement off, crowd wake radius) with non-emitting, stationary and slow sources,
//!   nonzero buoyancy and zero dissipation.
//! - Fluid, C-18 characterisation (plan `docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md`,
//!   WP6): a wake that decays after its sources stop ([`fluid_wake_decay_golden`]), sources at 8
//!   to 12 world units per second at the 0.05 s clamp ([`fluid_fast_sources_golden`]), a
//!   quantized window shift followed by a clearing shift ([`fluid_bounds_shift_golden`]), and a
//!   cycle of `dt` against a source count that switches confinement on and off
//!   ([`fluid_variable_dt_golden`]). They pin the cases the brick-sparsity refactor can get wrong.
//! - Kinematics: the 10-bubble showcase, a non-planar set, the 400-bubble spatial-hash path and the
//!   600-bubble all-pairs fallback beyond [`client::smog::MAX_BOUNCING_BUBBLES`].
//! - Crowd inputs: [`crowd_lattice`] is the generator's crowd branch with its trig supplied by the
//!   caller; [`crowd_generator_matches_lattice`] pins it to [`generate_bouncing_bubbles`] bit for
//!   bit, and [`crowd_trig_table_matches_host`] ties the recorded table to the generator's trig.
//! - Systems: the Visual Lab startup and showcase frames (textures, material, bubbles, uniform),
//!   the noise texture, a Visual Lab preview bubble, the swarm tab and the live battle tab (window
//!   placement, velocity scale, trajectory pre-warm and cavity slots).
//!
//! # Invariants
//! - The fluid constants were recorded on `main` at `9dbf715`, before any WP7 edit, and did not
//!   change when this file moved to the two classes.
//! - Every other constant was recorded by running this file, unchanged, against `9dbf715` and then
//!   confirmed unchanged on `main` at `3f57fec` (glibc 2.41, `x86_64`), except those below.
//! - The four C-18 characterisation goldens and [`GOLDEN_LIBM_CANARY_C18`] were recorded on
//!   `main` at `db22ac2` (glibc 2.41, `x86_64`), whose `smog/fluid.rs` is that of `e85f79f`, the
//!   C-18 plan's base: the unmodified solver, before any C-18 refactor.
//! - The startup, Visual Lab, swarm and live battle system goldens hash each bubble record's
//!   `Debug` text, which names its fields. They were re-recorded when
//!   `BubbleRenderInstance::anim_progress: 0.0` became `attack_swing: None` (plan
//!   `2026-11-04-battle-flipbook-animation` WP1). With that one spelling substituted back, all
//!   four reproduced their previous values, so no smog output changed.
//! - The same four hash each bubble material's `BubbleUniformGpu` bytes. They were re-recorded
//!   when that uniform grew from 160 to 176 bytes and `DEFAULT_FEATURE_FLAGS` gained
//!   `FLAG_FRAME_BLEND` (same plan, WP4). Hashing the first 160 bytes with bit 10 of
//!   `feature_flags` cleared, the old image (the new row is all zero for fixed lanes), all four
//!   reproduced their previous values, so no smog output changed.
//! - A golden may only change together with a deliberate, documented change in smog output.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::unwrap_used,
    clippy::expect_used
)]

#[path = "support/smog_crowd_trig.rs"]
mod smog_crowd_trig;

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use client::smog::{
    bubble_team_color, generate_bouncing_bubbles, initial_bouncing_bubbles,
    step_bouncing_kinematics, BouncingBubbleState, BouncingSmogBubble, BubbleSmogSource,
    SmogArenaBounds, SmogConfig, SmogError, SmogGpuFrame, SmogGrid3d, SmogPlugin, SmogStats,
    SmogStepMetrics, SmogTextures, GRID_NX, GRID_NY, GRID_NZ, LAB_VOLUME_MAX, LAB_VOLUME_MIN,
};
use client::ResolvedAssetsRoot;
use smog_crowd_trig::CROWD_TRIG_BITS;

/// Fluid steps run per fluid scenario.
const FLUID_STEPS: usize = 120;

/// Kinematics steps run per kinematics scenario.
const KINEMATICS_STEPS: usize = 120;

/// Every how many fluid steps the whole field is hashed (metrics are hashed every step).
const FIELD_CHECKPOINT: usize = 30;

/// Fixed fluid and kinematics step, in seconds.
const STEP_DT: f32 = 1.0 / 60.0;

/// Horizontal speed, in world units per second, above which `sync_headings` re-derives a heading.
const HEADING_SYNC_MIN_SPEED: f32 = 1e-4;

/// 64-bit FNV-1a, written out so the golden does not depend on a hasher whose algorithm may
/// change between Rust or crate releases.
struct Fnv64(u64);

impl Fnv64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.bytes(&v.to_bits().to_le_bytes());
    }

    fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }

    fn quat(&mut self, q: Quat) {
        self.f32(q.x);
        self.f32(q.y);
        self.f32(q.z);
        self.f32(q.w);
    }

    fn metrics(&mut self, m: &SmogStepMetrics) {
        self.f32(m.total_density);
        self.f32(m.peak_density);
        self.f32(m.max_speed);
        self.f32(m.max_vorticity);
        self.u64(m.active_sources as u64);
    }

    /// Hashes a kinematics state without its heading: the heading is `atan2f` of the velocity, a
    /// host `libm` result, and is checked by [`assert_heading_synced`] instead.
    fn state(&mut self, s: &BouncingBubbleState) {
        self.u64(s.index as u64);
        self.vec3(s.position);
        self.vec3(s.velocity);
        self.f32(s.radius);
        self.u64(s.wall_bounces);
        self.u64(s.peer_bounces);
    }

    /// Hashes every voxel's density, velocity, glow and vorticity, and both upload buffers.
    fn grid(&mut self, grid: &SmogGrid3d) {
        self.vec3(grid.bounds_min());
        self.vec3(grid.bounds_max());
        for iz in 0..GRID_NZ {
            for iy in 0..GRID_NY {
                for ix in 0..GRID_NX {
                    let p = grid.grid_to_world(ix, iy, iz);
                    self.f32(grid.sample_density_trilinear(p));
                    self.vec3(grid.sample_velocity_trilinear(p));
                    self.f32(grid.sample_glow_trilinear(p));
                    self.f32(grid.sample_vorticity_magnitude(p));
                }
            }
        }
        self.bytes(grid.density_bytes());
        self.bytes(grid.velocity_bytes());
    }

    /// Hashes an image's bytes, descriptor, sampler and usage.
    fn image(&mut self, image: &Image) {
        self.bytes(&image.data);
        self.bytes(format!("{:?}", image.texture_descriptor).as_bytes());
        self.bytes(format!("{:?}", image.sampler).as_bytes());
        self.bytes(format!("{:?}", image.asset_usage).as_bytes());
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// The heading `sync_headings` gives a bubble moving at `velocity` (world units per second) whose
/// heading was `previous` (radians): this host's `atan2f` of the horizontal velocity, or
/// `previous` when the bubble barely moves horizontally. The expression is the production one, so
/// on any host both sides call the same `atan2f` on the same bits.
fn expected_heading(velocity: Vec3, previous: f32) -> f32 {
    let speed_xz = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
    if speed_xz > HEADING_SYNC_MIN_SPEED {
        velocity.z.atan2(velocity.x)
    } else {
        previous
    }
}

/// Asserts the heading relation: `state.heading_angle_rad` is bit-equal to
/// [`expected_heading`] of its final velocity and `previous` heading.
fn assert_heading_synced(state: &BouncingBubbleState, previous: f32, context: &str) {
    let expected = expected_heading(state.velocity, previous);
    assert_eq!(
        state.heading_angle_rad.to_bits(),
        expected.to_bits(),
        "{context}: bubble {} heading {} is not synced to velocity {:?} (expected {expected})",
        state.index,
        state.heading_angle_rad,
        state.velocity,
    );
}

/// How a fluid scenario derives a source from a kinematics state.
#[derive(Clone, Copy)]
enum SourceMix {
    /// Every bubble emits and influences at full strength, as the showcase pre-warm does.
    Showcase,
    /// Every fourth bubble only displaces, every fifth stands still, every seventh crawls.
    Mixed,
}

fn source_for(index: usize, state: &BouncingBubbleState, mix: SourceMix) -> BubbleSmogSource {
    let mut source = BubbleSmogSource {
        position: state.position,
        velocity: state.velocity,
        radius: state.radius,
        team_color: bubble_team_color(index),
        emission_strength: 1.0,
        influence_strength: 1.0,
    };
    if let SourceMix::Mixed = mix {
        if index.is_multiple_of(4) {
            source.emission_strength = 0.0;
            source.influence_strength = 1.7;
        }
        if index.is_multiple_of(5) {
            source.velocity = Vec3::ZERO;
        } else if index.is_multiple_of(7) {
            source.velocity *= 0.04;
        }
    }
    source
}

/// The four trig values the crowd generator uses for bubble `i`, from the recorded table:
/// `sin(i * GOLDEN_RATIO)`, `sin(i * PI)`, `cos(i * golden angle)` and `sin(i * golden angle)`.
fn table_trig(i: usize) -> [f32; 4] {
    CROWD_TRIG_BITS[i].map(f32::from_bits)
}

/// The same four values as [`table_trig`], computed by this host's `libm` with the generator's
/// expressions.
fn host_trig(i: usize) -> [f32; 4] {
    let phi = (i as f32) * 2.399_963_1;
    [
        (i as f32 * std::f32::consts::GOLDEN_RATIO).sin(),
        (i as f32 * std::f32::consts::PI).sin(),
        phi.cos(),
        phi.sin(),
    ]
}

/// The crowd branch of [`generate_bouncing_bubbles`] (`count > 10`), expression for expression,
/// with its four trig calls replaced by `trig(i)` (ordered as [`table_trig`]).
///
/// With [`table_trig`] the result depends on no `libm` call except the unhashed initial heading,
/// so the crowd scenarios are host-independent. On the recording host the table equals the
/// host's trig, so this returns exactly what the generator returns.
fn crowd_lattice(
    count: usize,
    bounds: &SmogArenaBounds,
    trig: impl Fn(usize) -> [f32; 4],
) -> Vec<BouncingBubbleState> {
    assert!(
        count > client::BOUNCING_BUBBLE_COUNT && count <= CROWD_TRIG_BITS.len(),
        "the lattice replicates the crowd branch for 11..=600 bubbles, not {count}"
    );
    let radius = if count <= 20 {
        0.42
    } else if count <= 100 {
        0.30
    } else {
        0.24
    };

    let domain_x = (bounds.max.x - bounds.min.x - radius * 2.0).max(1.0);
    let domain_z = (bounds.max.z - bounds.min.z - radius * 2.0).max(1.0);

    let aspect = domain_x / domain_z;
    let nx = ((count as f32 * aspect).sqrt().round() as usize).max(1);
    let nz = count.div_ceil(nx);
    let cell_size_x = domain_x / nx as f32;
    let cell_size_z = domain_z / nz as f32;

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let ix = i % nx;
        let iz = i / nx;

        let fx = (ix as f32 + 0.5) / (nx as f32);
        let fz = (iz as f32 + 0.5) / (nz as f32).max(1.0);

        let [sin_golden_ratio, sin_pi, cos_phi, sin_phi] = trig(i);
        let jitter_x = (sin_golden_ratio * 0.25) * cell_size_x;
        let jitter_z = (sin_pi * 0.25) * cell_size_z;

        let r_i = radius + ((i % 5) as f32) * 0.01;
        let pos = Vec3::new(
            (bounds.min.x + r_i + fx * domain_x + jitter_x)
                .clamp(bounds.min.x + r_i, bounds.max.x - r_i),
            bounds.min.y + r_i,
            (bounds.min.z + r_i + fz * domain_z + jitter_z)
                .clamp(bounds.min.z + r_i, bounds.max.z - r_i),
        );

        let speed = 2.2 + ((i % 7) as f32) * 0.25;
        let vel = Vec3::new(cos_phi * speed, 0.0, sin_phi * speed);

        out.push(BouncingBubbleState {
            index: i,
            position: pos,
            velocity: vel,
            radius: r_i,
            heading_angle_rad: vel.z.atan2(vel.x),
            wall_bounces: 0,
            peer_bounces: 0,
        });
    }
    out
}

/// Runs kinematics and fluid together for [`FLUID_STEPS`] and hashes metrics and fields.
fn fluid_scenario_hash(
    mut states: Vec<BouncingBubbleState>,
    config: &SmogConfig,
    mix: SourceMix,
) -> Result<u64, SmogError> {
    let mut grid = SmogGrid3d::new(LAB_VOLUME_MIN, LAB_VOLUME_MAX)?;
    let mut sources = Vec::with_capacity(states.len());
    let mut h = Fnv64::new();
    for step in 1..=FLUID_STEPS {
        step_bouncing_kinematics(&mut states, &config.bounds, STEP_DT)?;
        sources.clear();
        sources.extend(
            states
                .iter()
                .enumerate()
                .map(|(index, state)| source_for(index, state, mix)),
        );
        let metrics = grid.step(&sources, config, STEP_DT)?;
        h.metrics(&metrics);
        if step % FIELD_CHECKPOINT == 0 {
            h.grid(&grid);
        }
    }
    Ok(h.finish())
}

fn crowd_config() -> SmogConfig {
    SmogConfig {
        emission_rate: 3.0,
        wake_influence: 2.2,
        dissipation_rate: 0.0,
        vorticity_confinement: 0.8,
        buoyancy: 0.3,
        velocity_damping: 0.25,
        ..SmogConfig::default()
    }
}

const GOLDEN_FLUID_SHOWCASE: u64 = 0xb42e_8fd5_0690_ef08;
const GOLDEN_FLUID_MIXED_24: u64 = 0x2e2d_8269_5e27_898d;
const GOLDEN_FLUID_CROWD_48: u64 = 0x6ab3_c16e_837d_e74b;

/// The showcase: ten emitting bubbles with confinement on, default configuration.
///
/// Class L: `expf` (the velocity, density and glow decay factors).
#[test]
fn fluid_showcase_golden() -> Result<(), SmogError> {
    let config = SmogConfig::default();
    let states = initial_bouncing_bubbles(&config.bounds)?.to_vec();
    let hash = fluid_scenario_hash(states, &config, SourceMix::Showcase)?;
    assert_eq!(
        hash, GOLDEN_FLUID_SHOWCASE,
        "fluid showcase golden: {hash:#018x}"
    );
    Ok(())
}

/// Twenty-four mixed sources: confinement still on, every source branch exercised.
///
/// Class L: `expf` (decay factors) and `sinf` (`bubble_team_color` for indices 10 and up). The
/// crowd comes from the recorded trig table, not from this host's trig.
#[test]
fn fluid_mixed_sources_golden() -> Result<(), SmogError> {
    let config = crowd_config();
    let states = crowd_lattice(24, &config.bounds, table_trig);
    let hash = fluid_scenario_hash(states, &config, SourceMix::Mixed)?;
    assert_eq!(
        hash, GOLDEN_FLUID_MIXED_24,
        "fluid mixed golden: {hash:#018x}"
    );
    Ok(())
}

/// Forty-eight mixed sources: past the 32-source limit, so the crowd wake radius applies and
/// confinement is skipped.
///
/// Class L: `expf` (decay factors) and `sinf` (`bubble_team_color` for indices 10 and up). The
/// crowd comes from the recorded trig table, not from this host's trig.
#[test]
fn fluid_crowd_golden() -> Result<(), SmogError> {
    let config = crowd_config();
    let states = crowd_lattice(48, &config.bounds, table_trig);
    let hash = fluid_scenario_hash(states, &config, SourceMix::Mixed)?;
    assert_eq!(
        hash, GOLDEN_FLUID_CROWD_48,
        "fluid crowd golden: {hash:#018x}"
    );
    Ok(())
}

// C-18 characterisation goldens (WP6 of `docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md`).
// They pin the cases where per-brick rest flags can go wrong (sources that stop, back-traces that
// cross bricks, a grid that shifts or clears, and `dt` and confinement that change from step to
// step), recorded on the unmodified solver before any C-18 refactor.

/// Steps with the showcase sources before they stop, in the wake decay golden.
const WAKE_SOURCE_STEPS: usize = 60;

/// Steps without any source after the showcase sources stop, in the wake decay golden.
const WAKE_DECAY_STEPS: usize = 240;

/// Sources in the fast sources golden: below the 32-source limit, so confinement runs.
const FAST_SOURCE_COUNT: usize = 20;

/// Slowest source speed of the fast sources golden, in world units per second; source `i` moves
/// at this speed plus `i % 5` world units per second, so 8 to 12.
const FAST_SOURCE_MIN_SPEED: f32 = 8.0;

/// Step of the fast sources golden, in seconds: the solver's own clamp, the longest back-trace
/// a step can take.
const FAST_DT: f32 = 0.05;

/// The bounds shift golden's phases: steps before the first shift, between the shifts, and after
/// the second.
const SHIFT_PHASE_STEPS: [usize; 3] = [40, 40, 20];

/// First window shift of the bounds shift golden, in voxels: an overlapping shift that moves
/// the field inside its slabs.
const SHIFT_SMALL_VOXELS: [i32; 3] = [3, 0, -2];

/// Second window shift of the bounds shift golden, in voxels along x: more than the grid's
/// width (96 voxels, checked below), so `set_bounds_quantized` takes its clearing path.
const SHIFT_LARGE_VOXELS_X: u16 = 100;

const _: () = assert!(
    SHIFT_LARGE_VOXELS_X as usize >= GRID_NX,
    "the large shift must be at least the grid width"
);

/// The variable `dt` golden's cycle of steps, in seconds: three display rates, the pre-warm
/// step, a step above the solver's 0.05 s clamp, and a paused frame (`dt = 0`, a no-op).
const VARIABLE_DTS: [f32; 6] = [1.0 / 240.0, 1.0 / 144.0, 1.0 / 60.0, 1.0 / 30.0, 0.07, 0.0];

/// The variable `dt` golden's source counts: 30 (confinement runs) and 40 (above the 32-source
/// limit, so confinement is skipped and the vorticity slabs keep their previous contents).
const VARIABLE_SOURCE_COUNTS: [usize; 2] = [30, 40];

/// Steps per source count in the variable `dt` golden. Five is coprime with the six-step `dt`
/// cycle, so every `dt` meets both source counts.
const VARIABLE_COUNT_RUN: usize = 5;

/// Steps of the variable `dt` golden.
const VARIABLE_STEPS: usize = 120;

const GOLDEN_FLUID_WAKE_DECAY: u64 = 0x6f8f_62ab_607f_d719;
const GOLDEN_FLUID_FAST_SOURCES: u64 = 0x7ffa_1917_b16b_ea08;
const GOLDEN_FLUID_BOUNDS_SHIFT: u64 = 0xb677_0ad8_9bf1_a12a;
const GOLDEN_FLUID_VARIABLE_DT: u64 = 0xf4fc_5e58_2628_330d;

/// Fills `sources` with one showcase-mix source per state, translated by `offset` world units.
fn fill_sources(sources: &mut Vec<BubbleSmogSource>, states: &[BouncingBubbleState], offset: Vec3) {
    sources.clear();
    sources.extend(states.iter().enumerate().map(|(index, state)| {
        let mut source = source_for(index, state, SourceMix::Showcase);
        source.position += offset;
        source
    }));
}

/// Steps `grid` once and hashes the metrics, and the whole field when `checkpoint` is set.
fn hashed_step(
    grid: &mut SmogGrid3d,
    sources: &[BubbleSmogSource],
    config: &SmogConfig,
    dt: f32,
    checkpoint: bool,
    h: &mut Fnv64,
) -> Result<(), SmogError> {
    let metrics = grid.step(sources, config, dt)?;
    h.metrics(&metrics);
    if checkpoint {
        h.grid(grid);
    }
    Ok(())
}

/// The showcase sources for [`WAKE_SOURCE_STEPS`], then no source for [`WAKE_DECAY_STEPS`]:
/// the wake decays while nothing feeds it. Bricks return to rest here (density and glow reach
/// exactly zero; the velocity field does not, `docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md`
/// §5.4), so a rest flag that is set too early, or a stale `occupied` flag, changes this hash.
///
/// Class L: `expf` (the velocity, density and glow decay factors at `1/60` s).
#[test]
fn fluid_wake_decay_golden() -> Result<(), SmogError> {
    let config = SmogConfig::default();
    let mut states = initial_bouncing_bubbles(&config.bounds)?.to_vec();
    let mut grid = SmogGrid3d::new(LAB_VOLUME_MIN, LAB_VOLUME_MAX)?;
    let mut sources = Vec::with_capacity(states.len());
    let mut h = Fnv64::new();
    for step in 1..=WAKE_SOURCE_STEPS + WAKE_DECAY_STEPS {
        if step <= WAKE_SOURCE_STEPS {
            step_bouncing_kinematics(&mut states, &config.bounds, STEP_DT)?;
            fill_sources(&mut sources, &states, Vec3::ZERO);
        } else {
            sources.clear();
        }
        let checkpoint = step % FIELD_CHECKPOINT == 0;
        hashed_step(&mut grid, &sources, &config, STEP_DT, checkpoint, &mut h)?;
    }
    let hash = h.finish();
    assert_eq!(
        hash, GOLDEN_FLUID_WAKE_DECAY,
        "fluid wake decay golden: {hash:#018x}"
    );
    Ok(())
}

/// Twenty sources at 8 to 12 world units per second, stepped at the 0.05 s clamp, so a
/// semi-Lagrangian back-trace reaches more than two voxels on the finest axis (the fluid peaks at
/// about 6.3 world units per second, 2.5 voxels of z per step) and so crosses brick boundaries
/// (a brick is 8 × 4 × 8 voxels in the C-18 plan's D-2). A reach that is too short changes this
/// hash.
///
/// Class L: `expf` (decay factors at `0.05` s) and `sinf` (`bubble_team_color` for indices 10 and
/// up). The crowd comes from the recorded trig table; its speeds are rescaled with `sqrt`, which
/// is correctly rounded on every host.
#[test]
fn fluid_fast_sources_golden() -> Result<(), SmogError> {
    let config = SmogConfig::default();
    let mut states = crowd_lattice(FAST_SOURCE_COUNT, &config.bounds, table_trig);
    for (i, state) in states.iter_mut().enumerate() {
        let speed = FAST_SOURCE_MIN_SPEED + (i % 5) as f32;
        state.velocity *= speed / state.velocity.length();
    }
    let mut grid = SmogGrid3d::new(LAB_VOLUME_MIN, LAB_VOLUME_MAX)?;
    let mut sources = Vec::with_capacity(states.len());
    let mut h = Fnv64::new();
    let mut peak_speed = 0.0_f32;
    for step in 1..=FLUID_STEPS {
        step_bouncing_kinematics(&mut states, &config.bounds, FAST_DT)?;
        fill_sources(&mut sources, &states, Vec3::ZERO);
        let metrics = grid.step(&sources, &config, FAST_DT)?;
        peak_speed = peak_speed.max(metrics.max_speed);
        h.metrics(&metrics);
        if step % FIELD_CHECKPOINT == 0 {
            h.grid(&grid);
        }
    }
    // The scenario's purpose: the fluid moves fast enough that one back-trace spans more than
    // two voxels on the finest axis.
    let finest_cell = (LAB_VOLUME_MAX - LAB_VOLUME_MIN)
        / Vec3::new(GRID_NX as f32, GRID_NY as f32, GRID_NZ as f32);
    let reach_voxels = peak_speed * FAST_DT / finest_cell.min_element();
    println!("fast sources: peak fluid speed {peak_speed}, back-trace reach {reach_voxels} voxels");
    assert!(
        reach_voxels > 2.0,
        "fast sources must back-trace more than two voxels, reached {reach_voxels}"
    );
    let hash = h.finish();
    assert_eq!(
        hash, GOLDEN_FLUID_FAST_SOURCES,
        "fluid fast sources golden: {hash:#018x}"
    );
    Ok(())
}

/// The showcase sources for 40 steps; the window shifted by (+3, 0, −2) voxels with
/// `set_bounds_quantized` (the field moves inside its slabs and the exposed voxels are
/// re-initialised); 40 steps; a shift wider than the grid (the clearing path); 20 steps. The
/// sources move with the window. The field is hashed at the usual checkpoints, at the end of
/// each phase and right after each shift.
///
/// Class L: `expf` (decay factors at `1/60` s).
#[test]
fn fluid_bounds_shift_golden() -> Result<(), SmogError> {
    let config = SmogConfig::default();
    let mut states = initial_bouncing_bubbles(&config.bounds)?.to_vec();
    let mut grid = SmogGrid3d::new(LAB_VOLUME_MIN, LAB_VOLUME_MAX)?;
    let extent = LAB_VOLUME_MAX - LAB_VOLUME_MIN;
    let cell = extent / Vec3::new(GRID_NX as f32, GRID_NY as f32, GRID_NZ as f32);
    let shifts = [
        Vec3::new(
            SHIFT_SMALL_VOXELS[0] as f32 * cell.x,
            SHIFT_SMALL_VOXELS[1] as f32 * cell.y,
            SHIFT_SMALL_VOXELS[2] as f32 * cell.z,
        ),
        Vec3::new(f32::from(SHIFT_LARGE_VOXELS_X) * cell.x, 0.0, 0.0),
    ];
    let mut sources = Vec::with_capacity(states.len());
    let mut h = Fnv64::new();
    let mut step = 0;
    for (phase, &phase_steps) in SHIFT_PHASE_STEPS.iter().enumerate() {
        if phase > 0 {
            let before = grid.bounds_min();
            let new_min = before + shifts[phase - 1];
            grid.set_bounds_quantized(new_min, new_min + extent);
            assert_ne!(
                grid.bounds_min(),
                before,
                "shift {phase} must move the window"
            );
            h.grid(&grid);
        }
        let offset = grid.bounds_min() - LAB_VOLUME_MIN;
        for phase_step in 1..=phase_steps {
            step += 1;
            step_bouncing_kinematics(&mut states, &config.bounds, STEP_DT)?;
            fill_sources(&mut sources, &states, offset);
            let checkpoint = step % FIELD_CHECKPOINT == 0 || phase_step == phase_steps;
            hashed_step(&mut grid, &sources, &config, STEP_DT, checkpoint, &mut h)?;
        }
    }
    let hash = h.finish();
    assert_eq!(
        hash, GOLDEN_FLUID_BOUNDS_SHIFT,
        "fluid bounds shift golden: {hash:#018x}"
    );
    Ok(())
}

/// A fixed cycle of `dt` ([`VARIABLE_DTS`]: 1/240, 1/144, 1/60, 1/30, 0.07 and 0 s) against a
/// source count alternating 30 and 40 every [`VARIABLE_COUNT_RUN`] steps, so confinement switches
/// on and off and the stale vorticity of the last confined step is packed while it is off.
///
/// Class L: `expf` (decay factors at each nonzero `dt`, 0.07 clamped to 0.05) and `sinf`
/// (`bubble_team_color` for indices 10 and up). The crowd comes from the recorded trig table.
#[test]
fn fluid_variable_dt_golden() -> Result<(), SmogError> {
    let config = SmogConfig::default();
    let max_count = VARIABLE_SOURCE_COUNTS[1];
    let mut states = crowd_lattice(max_count, &config.bounds, table_trig);
    let mut grid = SmogGrid3d::new(LAB_VOLUME_MIN, LAB_VOLUME_MAX)?;
    let mut sources = Vec::with_capacity(max_count);
    let mut h = Fnv64::new();
    for step in 0..VARIABLE_STEPS {
        let dt = VARIABLE_DTS[step % VARIABLE_DTS.len()];
        let count =
            VARIABLE_SOURCE_COUNTS[(step / VARIABLE_COUNT_RUN) % VARIABLE_SOURCE_COUNTS.len()];
        step_bouncing_kinematics(&mut states, &config.bounds, dt)?;
        fill_sources(&mut sources, &states[..count], Vec3::ZERO);
        let checkpoint = (step + 1) % FIELD_CHECKPOINT == 0;
        hashed_step(&mut grid, &sources, &config, dt, checkpoint, &mut h)?;
    }
    let hash = h.finish();
    assert_eq!(
        hash, GOLDEN_FLUID_VARIABLE_DT,
        "fluid variable dt golden: {hash:#018x}"
    );
    Ok(())
}

/// Steps kinematics for [`KINEMATICS_STEPS`], hashes the event counts and every state without its
/// heading, and checks the heading relation for every state after every step.
fn kinematics_hash(
    mut states: Vec<BouncingBubbleState>,
    bounds: &SmogArenaBounds,
    context: &str,
) -> Result<u64, SmogError> {
    let mut previous_headings: Vec<f32> = states.iter().map(|s| s.heading_angle_rad).collect();
    let mut h = Fnv64::new();
    let mut total_walls = 0;
    for step in 1..=KINEMATICS_STEPS {
        let (walls, peers) = step_bouncing_kinematics(&mut states, bounds, STEP_DT)?;
        total_walls += walls;
        h.u64(walls);
        h.u64(peers);
        for (state, previous) in states.iter().zip(previous_headings.iter_mut()) {
            h.state(state);
            assert_heading_synced(state, *previous, &format!("{context}, step {step}"));
            *previous = state.heading_angle_rad;
        }
    }
    assert!(total_walls > 0, "the scenario must reach a wall");
    assert!(
        states.iter().any(|state| state.peer_bounces > 0),
        "the scenario must resolve a contact"
    );
    Ok(h.finish())
}

const GOLDEN_KINEMATICS_SHOWCASE: u64 = 0xb7a3_f586_377e_729a;
const GOLDEN_KINEMATICS_NON_PLANAR: u64 = 0x4e79_ecfc_c959_7e43;
const GOLDEN_KINEMATICS_GRID_400: u64 = 0x6aca_fe91_b0ed_7b8a;
/// Re-recorded for Phase 9 §9 gap 3 (WP1 of the 2026-11-04 open-findings plan): the all-pairs
/// fallback now returns its real contact count, so each step's hashed `peers` value changed. The
/// positions, velocities and per-bubble counters did not. Previous value: `0x8d1d_c59d_6fba_ec0f`.
const GOLDEN_KINEMATICS_FALLBACK_600: u64 = 0xcd78_5a1b_373f_3636;

/// Class H: literal positions and velocities; heading checked by relation.
#[test]
fn kinematics_showcase_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let states = initial_bouncing_bubbles(&bounds)?.to_vec();
    let hash = kinematics_hash(states, &bounds, "kinematics showcase")?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_SHOWCASE,
        "kinematics showcase golden: {hash:#018x}"
    );
    Ok(())
}

/// Bubbles lifted off the floor with vertical velocity: the 3D (not planar) contact branch.
///
/// Class H: literal positions and velocities; heading checked by relation.
#[test]
fn kinematics_non_planar_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let mut states = initial_bouncing_bubbles(&bounds)?.to_vec();
    for (i, state) in states.iter_mut().enumerate() {
        state.position.y = bounds.min.y + state.radius + 0.3 + (i % 3) as f32 * 0.5;
        state.velocity.y = if i % 2 == 0 { 1.7 } else { -2.3 };
    }
    let hash = kinematics_hash(states, &bounds, "kinematics non-planar")?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_NON_PLANAR,
        "kinematics non-planar golden: {hash:#018x}"
    );
    Ok(())
}

/// Class H: the 400-bubble crowd from the recorded trig table; heading checked by relation.
#[test]
fn kinematics_spatial_grid_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let states = crowd_lattice(400, &bounds, table_trig);
    let hash = kinematics_hash(states, &bounds, "kinematics grid")?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_GRID_400,
        "kinematics grid golden: {hash:#018x}"
    );
    Ok(())
}

/// Class H: the 600-bubble crowd from the recorded trig table; heading checked by relation.
#[test]
fn kinematics_all_pairs_fallback_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let states = crowd_lattice(600, &bounds, table_trig);
    let hash = kinematics_hash(states, &bounds, "kinematics fallback")?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_FALLBACK_600,
        "kinematics fallback golden: {hash:#018x}"
    );
    Ok(())
}

/// Pins [`crowd_lattice`] to the production generator: with this host's trig, both give
/// bit-identical index, position, velocity and radius for every crowd size class, and the
/// generator's initial heading satisfies the heading relation. Host-independent: both sides call
/// the same host `libm` on the same bits.
#[test]
fn crowd_generator_matches_lattice() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    for count in [11, 20, 21, 24, 48, 100, 101, 400, 600] {
        let generated = generate_bouncing_bubbles(count, &bounds)?;
        let lattice = crowd_lattice(count, &bounds, host_trig);
        assert_eq!(generated.len(), lattice.len(), "crowd {count}: length");
        for (g, l) in generated.iter().zip(&lattice) {
            let context = format!("crowd {count}, bubble {}", g.index);
            assert_eq!(g.index, l.index, "{context}: index");
            for (axis, gv, lv) in [
                ("position.x", g.position.x, l.position.x),
                ("position.y", g.position.y, l.position.y),
                ("position.z", g.position.z, l.position.z),
                ("velocity.x", g.velocity.x, l.velocity.x),
                ("velocity.y", g.velocity.y, l.velocity.y),
                ("velocity.z", g.velocity.z, l.velocity.z),
                ("radius", g.radius, l.radius),
            ] {
                assert_eq!(
                    gv.to_bits(),
                    lv.to_bits(),
                    "{context}: {axis} generator {gv} lattice {lv}"
                );
            }
            assert_eq!((g.wall_bounces, g.peer_bounces), (0, 0), "{context}");
            assert_eq!(
                g.heading_angle_rad.to_bits(),
                g.velocity.z.atan2(g.velocity.x).to_bits(),
                "{context}: initial heading is atan2 of the initial velocity"
            );
        }
    }
    Ok(())
}

/// Distance in units in the last place between two finite `f32` values.
fn ulp_distance(a: f32, b: f32) -> u64 {
    let ordered = |v: f32| {
        let bits = v.to_bits();
        if bits & 0x8000_0000 == 0 {
            i64::from(bits)
        } else {
            -i64::from(bits & 0x7fff_ffff)
        }
    };
    ordered(a).abs_diff(ordered(b))
}

/// Ties [`CROWD_TRIG_BITS`] to the generator's trig: every entry is within 2 ulps of this host's
/// value.
///
/// This checks where the table came from, not behaviour, so it has a tolerance. glibc, musl and
/// macOS `sinf`/`cosf` are faithfully rounded on these arguments, so two hosts differ by at most
/// one ulp, while a table of anything other than the generator's trig would be off by far more.
/// The bound leaves one ulp of headroom for a less accurate `libm`. The bit-exact pin of the
/// generator is [`crowd_generator_matches_lattice`].
#[test]
fn crowd_trig_table_matches_host() {
    const MAX_ULPS: u64 = 2;
    let names = [
        "sin(i * GOLDEN_RATIO)",
        "sin(i * PI)",
        "cos(i * golden angle)",
        "sin(i * golden angle)",
    ];
    let mut largest = (0, 0, 0);
    for i in 0..CROWD_TRIG_BITS.len() {
        let table = table_trig(i);
        let host = host_trig(i);
        for k in 0..4 {
            let gap = ulp_distance(table[k], host[k]);
            if gap > largest.0 {
                largest = (gap, i, k);
            }
        }
    }
    let (gap, i, k) = largest;
    println!(
        "largest crowd trig gap: {gap} ulp ({} at i = {i})",
        names[k]
    );
    assert!(
        gap <= MAX_ULPS,
        "crowd trig table entry {} at i = {i} is {gap} ulps from this host's value (table {}, \
         host {}); the table does not hold the generator's trig",
        names[k],
        table_trig(i)[k],
        host_trig(i)[k],
    );
}

fn resolve_root() -> Result<ResolvedAssetsRoot, Box<dyn std::error::Error>> {
    Ok(ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .canonicalize()?,
    )?)
}

/// Headless application with the production smog plugin, real catalog, palette and media, and a
/// manual clock advancing [`STEP_DT`] per update.
fn production_smog_app(lab: client::LabState) -> Result<App, Box<dyn std::error::Error>> {
    let root = resolve_root()?;
    let definition = client::runtime_battle::BattleDefinition::load(
        root.as_path(),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        STEP_DT,
    )));
    app.add_event::<AppExit>();
    app.init_resource::<client::RenderEntityMap>();
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<client::bubble_shader::BubbleDomeMaterial>>();
    app.init_resource::<Assets<client::smog::SmogVolumeMaterial>>();
    app.insert_resource(definition.palette.clone());
    app.init_resource::<client::player_presentation::PresentationStatus>();
    app.insert_resource(root);
    app.insert_resource(client::CatalogResource(definition.catalogs.units.clone()));
    app.insert_resource(lab);
    app.add_systems(Startup, client::bubble_shader::setup_bubble_assets_system);
    app.add_plugins(SmogPlugin);
    app.edit_schedule(Update, |schedule| {
        schedule.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
    });
    Ok(app)
}

/// A mobile battle unit sphere moving at `velocity` world units per tick along world X and Z.
fn battle_unit(entity_id: u32, pos: Vec3, velocity: [f32; 2]) -> client::BubbleInstance {
    client::BubbleInstance(sim_types::snapshot::BubbleRenderInstance {
        entity_id: sim_types::EntityId(entity_id),
        shape: sim_types::enums::ShapeKind::UnitSphere,
        silhouette: sim_types::enums::SilhouetteClass::LightProngs,
        unit_type: sim_types::ids::UnitTypeId(1),
        owner: sim_types::PlayerId(1),
        world_pos: [pos.x, pos.z, pos.y],
        ground_z: 0.0,
        radius: 0.45,
        velocity,
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: sim_types::enums::WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state: sim_types::enums::AnimState::Walk,
        attack_swing: None,
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: Some(1.0),
        selected: false,
        cloaked: false,
        speed_boosted: false,
    })
}

/// Whether the showcase bubbles have been stepped by `sync_bouncing_smog_bubbles_system` since
/// they were spawned, which decides which heading relations hold.
#[derive(Clone, Copy)]
enum ShowcasePhase<'a> {
    /// Never stepped (the startup frame, or a tab that hides the showcase): heading and aim are
    /// still the spawn heading and the rotation is the identity.
    Unstepped,
    /// Stepped: the heading follows the velocity, the rotation follows the heading, and the aim
    /// keeps the spawn value recorded in `spawn_aims` (bits, by bubble index).
    Stepped { spawn_aims: &'a [u32] },
}

/// Checks the showcase heading relations and returns each showcase bubble's aim bits by index.
///
/// The showcase spawns from the startup pre-warm, which ends with `sync_headings`, so the spawn
/// heading of every bubble is this host's `atan2f` of its spawn velocity, and the aim equals it
/// (the aim is never updated). In both phases the instance velocity, the material uniform's
/// heading and aim (written once at spawn) and the relations below are compared bit for bit:
///
/// - [`ShowcasePhase::Unstepped`]: heading bits equal aim bits equal [`expected_heading`] of the
///   bubble's velocity, and `transform.rotation` is `Quat::IDENTITY` bit for bit.
/// - [`ShowcasePhase::Stepped`]: aim bits equal the spawn aim, heading bits equal
///   [`expected_heading`] of the velocity, and `transform.rotation` equals this host's
///   `Quat::from_rotation_y(-heading)` bit for bit.
///
/// Every showcase bubble moves faster than [`HEADING_SYNC_MIN_SPEED`] (elastic collisions keep the
/// speed), which is asserted, so the previous-heading branch of the relation is never needed.
fn check_showcase_headings(app: &mut App, phase: ShowcasePhase<'_>) -> Vec<u32> {
    let world = app.world_mut();
    let mut bubbles: Vec<_> = world
        .query::<(
            &BouncingSmogBubble,
            &Transform,
            &client::BubbleInstance,
            &MeshMaterial3d<client::bubble_shader::BubbleDomeMaterial>,
        )>()
        .iter(world)
        .map(|(bubble, transform, instance, material)| {
            (
                bubble.clone(),
                *transform,
                instance.0.clone(),
                material.0.clone(),
            )
        })
        .collect();
    bubbles.sort_by_key(|(bubble, ..)| bubble.index);
    assert_eq!(
        bubbles.len(),
        client::BOUNCING_BUBBLE_COUNT,
        "every showcase bubble exists"
    );
    let materials = world.resource::<Assets<client::bubble_shader::BubbleDomeMaterial>>();
    let mut aims = Vec::with_capacity(bubbles.len());
    for (bubble, transform, instance, material) in &bubbles {
        let index = bubble.index;
        let v = bubble.velocity;
        let speed_xz = (v.x * v.x + v.z * v.z).sqrt();
        assert!(
            speed_xz > HEADING_SYNC_MIN_SPEED,
            "showcase bubble {index} must move horizontally (speed {speed_xz})"
        );
        assert_eq!(
            instance.velocity.map(f32::to_bits),
            [v.x.to_bits(), v.z.to_bits()],
            "showcase bubble {index}: instance velocity follows the bubble"
        );
        let heading = instance.heading_angle_rad;
        let aim = instance.aim_angle_rad;
        let synced = expected_heading(v, heading);
        assert_eq!(
            heading.to_bits(),
            synced.to_bits(),
            "showcase bubble {index}: heading {heading} is not atan2 of velocity {v:?}"
        );
        let uniform = materials
            .get(material)
            .expect("bubble material exists")
            .uniform;
        match phase {
            ShowcasePhase::Unstepped => {
                assert_eq!(
                    aim.to_bits(),
                    heading.to_bits(),
                    "showcase bubble {index}: aim starts at the spawn heading"
                );
                assert_eq!(
                    transform.rotation.to_array().map(f32::to_bits),
                    Quat::IDENTITY.to_array().map(f32::to_bits),
                    "showcase bubble {index}: an unstepped bubble is not rotated"
                );
            }
            ShowcasePhase::Stepped { spawn_aims } => {
                assert_eq!(
                    aim.to_bits(),
                    spawn_aims[index],
                    "showcase bubble {index}: aim is never updated"
                );
                assert_eq!(
                    transform.rotation.to_array().map(f32::to_bits),
                    Quat::from_rotation_y(-heading).to_array().map(f32::to_bits),
                    "showcase bubble {index}: rotation {:?} does not follow heading {heading}",
                    transform.rotation
                );
            }
        }
        assert_eq!(
            [
                uniform.heading_angle_rad.to_bits(),
                uniform.aim_angle_rad.to_bits()
            ],
            [aim.to_bits(), aim.to_bits()],
            "showcase bubble {index}: the material keeps the spawn heading as heading and aim"
        );
        aims.push(aim.to_bits());
    }
    aims
}

/// Hashes the published uniform, stats, field, volume proxy and every showcase bubble.
///
/// Showcase heading and aim are host `atan2f` results and the stepped rotation is host
/// `sinf`/`cosf` of the heading, so they are checked by [`check_showcase_headings`] (called here)
/// instead of hashed: the render instance and the material uniform are hashed with heading and
/// aim set to `0.0`, and the rotation is hashed only in [`ShowcasePhase::Unstepped`], where the
/// relation makes it `Quat::IDENTITY`.
fn hash_smog_app(app: &mut App, h: &mut Fnv64, phase: ShowcasePhase<'_>) {
    check_showcase_headings(app, phase);
    let world = app.world_mut();
    let frame = world.resource::<SmogGpuFrame>();
    h.bytes(bytemuck::bytes_of(&frame.uniform));
    h.u64(frame.generation);
    let stats = *world.resource::<SmogStats>();
    h.f32(stats.total_density);
    h.f32(stats.peak_density);
    h.f32(stats.max_speed);
    h.f32(stats.max_vorticity);
    h.u64(stats.wall_bounces);
    h.u64(stats.peer_bounces);
    h.u64(stats.active_bubbles as u64);
    h.u64(stats.step_count);
    h.u64(world.resource::<client::LabState>().smog_bubble_count as u64);
    h.grid(world.resource::<SmogGrid3d>());

    let mut volumes =
        world.query_filtered::<(&Transform, &Visibility), With<client::SmogVolumeMarker>>();
    for (transform, visibility) in volumes.iter(world) {
        h.vec3(transform.translation);
        h.vec3(transform.scale);
        h.bytes(format!("{visibility:?}").as_bytes());
    }

    let mut bubbles: Vec<_> = world
        .query::<(
            &BouncingSmogBubble,
            &Transform,
            &client::BubbleInstance,
            &Visibility,
            &MeshMaterial3d<client::bubble_shader::BubbleDomeMaterial>,
        )>()
        .iter(world)
        .map(|(bubble, transform, instance, visibility, material)| {
            (
                bubble.clone(),
                *transform,
                instance.0.clone(),
                *visibility,
                material.0.clone(),
            )
        })
        .collect();
    bubbles.sort_by_key(|(bubble, ..)| bubble.index);
    let materials = world.resource::<Assets<client::bubble_shader::BubbleDomeMaterial>>();
    for (bubble, transform, instance, visibility, material) in &bubbles {
        h.u64(bubble.index as u64);
        h.vec3(bubble.velocity);
        h.f32(bubble.radius);
        h.u64(bubble.wall_bounces);
        h.u64(bubble.peer_bounces);
        h.vec3(transform.translation);
        if let ShowcasePhase::Unstepped = phase {
            h.quat(transform.rotation);
        }
        h.vec3(transform.scale);
        let mut instance = instance.clone();
        instance.heading_angle_rad = 0.0;
        instance.aim_angle_rad = 0.0;
        h.bytes(format!("{instance:?}").as_bytes());
        h.bytes(format!("{visibility:?}").as_bytes());
        let material = materials.get(material).expect("bubble material exists");
        let mut uniform = material.uniform;
        uniform.heading_angle_rad = 0.0;
        uniform.aim_angle_rad = 0.0;
        h.bytes(bytemuck::bytes_of(&uniform));
        h.bytes(format!("{:?}", material.alpha_mode).as_bytes());
        h.bytes(format!("{:?}", material.video_texture.is_some()).as_bytes());
    }
}

/// Hashes the density and velocity 3D textures and the volume material created at startup. The
/// noise texture is hashed by [`smog_noise_texture_golden`] alone.
fn hash_smog_startup_assets(app: &App, h: &mut Fnv64) {
    let world = app.world();
    let textures = world.resource::<SmogTextures>();
    let images = world.resource::<Assets<Image>>();
    for handle in [&textures.density_image, &textures.velocity_image] {
        h.image(images.get(handle).expect("smog image exists"));
    }
    let materials = world.resource::<Assets<client::smog::SmogVolumeMaterial>>();
    let material = materials
        .get(&textures.material_handle)
        .expect("smog material exists");
    h.bytes(bytemuck::bytes_of(&material.uniform));
}

/// Guards a system golden against hashing a scenario that stopped stepping smog: returns the
/// number of sources the last step saw.
fn require_smog_activity(app: &App) -> usize {
    let stats = app.world().resource::<SmogStats>();
    assert!(
        stats.step_count > 0,
        "the scenario must step the smog field"
    );
    assert!(
        stats.total_density > 0.0,
        "the scenario must generate smog (total {})",
        stats.total_density
    );
    stats.active_bubbles
}

const GOLDEN_SYSTEMS_STARTUP: u64 = 0x5690_15a8_6873_e107;
const GOLDEN_SYSTEMS_NOISE: u64 = 0x1fe2_2cc0_e03e_f783;
const GOLDEN_SYSTEMS_VISUAL_LAB: u64 = 0xe585_490b_a96b_dced;
const GOLDEN_SYSTEMS_SWARM: u64 = 0x8d70_5277_8a52_19e3;
const GOLDEN_SYSTEMS_LIVE_BATTLE: u64 = 0x5e7e_6426_b5ee_e540;

/// The headless Visual Lab app after its startup frame (the clock's first update has `dt = 0`).
fn startup_app() -> Result<App, Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        smog_bouncing_active: true,
        ..default()
    })?;
    app.update();
    Ok(app)
}

/// Startup assets, as `setup_smog_volume_system` creates them before any frame steps them.
///
/// Class L: `expf` (the pre-warm's decay factors) and `powf` (the showcase colours'
/// `Color::to_linear`).
#[test]
fn smog_startup_assets_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = startup_app()?;
    let mut h = Fnv64::new();
    hash_smog_startup_assets(&app, &mut h);
    hash_smog_app(&mut app, &mut h, ShowcasePhase::Unstepped);
    let hash = h.finish();
    assert_eq!(hash, GOLDEN_SYSTEMS_STARTUP, "startup golden: {hash:#018x}");
    Ok(())
}

/// The tileable 3D noise texture created at startup: bytes, descriptor, sampler and usage.
///
/// Class L: `sinf`, `cosf` and `powf` (the noise texel), each quantised to `u8`.
#[test]
fn smog_noise_texture_golden() -> Result<(), Box<dyn std::error::Error>> {
    let app = startup_app()?;
    let world = app.world();
    let textures = world.resource::<SmogTextures>();
    let images = world.resource::<Assets<Image>>();
    let mut h = Fnv64::new();
    h.image(
        images
            .get(&textures.noise_image)
            .expect("noise image exists"),
    );
    let hash = h.finish();
    assert_eq!(hash, GOLDEN_SYSTEMS_NOISE, "noise golden: {hash:#018x}");
    Ok(())
}

/// Visual Lab showcase with a preview bubble, through both systems for 40 frames. The showcase
/// heading relations are checked every frame.
///
/// Class L: `expf` (decay factors), `powf` (showcase colours) and `sinf` (the preview's
/// `bubble_team_color(77)`).
#[test]
fn smog_visual_lab_systems_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        smog_bouncing_active: true,
        smog_emission_rate: 4.1,
        smog_wake_influence: 2.6,
        ..default()
    })?;
    app.update();
    let spawn_aims = check_showcase_headings(&mut app, ShowcasePhase::Unstepped);
    let stepped = ShowcasePhase::Stepped {
        spawn_aims: &spawn_aims,
    };
    let preview = Vec3::new(1.2, 0.6, -0.4);
    app.world_mut().spawn((
        client::scene_controller::LabPreviewBubble,
        battle_unit(77, preview, [0.0, 0.0]),
        Transform::from_translation(preview),
        Visibility::Inherited,
    ));
    let mut h = Fnv64::new();
    for frame in 1..=40 {
        app.update();
        check_showcase_headings(&mut app, stepped);
        if frame % 10 == 0 {
            hash_smog_app(&mut app, &mut h, stepped);
        }
    }
    assert_eq!(
        require_smog_activity(&app),
        client::BOUNCING_BUBBLE_COUNT + 1
    );
    let hash = h.finish();
    assert_eq!(
        hash, GOLDEN_SYSTEMS_VISUAL_LAB,
        "visual lab golden: {hash:#018x}"
    );
    Ok(())
}

/// Swarm tab: the swarm window, the swarm velocity scale and filter, and the trajectory pre-warm.
///
/// Class L: `expf` (decay factors), `powf` (showcase colours) and `sinf` (`bubble_team_color`
/// of the swarm units' ids).
#[test]
fn smog_swarm_systems_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        current_tab: client::LabTab::SwarmStressTest,
        ..default()
    })?;
    for index in 0..30u32 {
        let column = index % 6;
        let row = index / 6;
        let pos = Vec3::new(
            client::scene_controller::SWARM_MIN_X + 1.0 + column as f32 * 2.3,
            0.45,
            client::scene_controller::SWARM_MIN_Z + 1.0 + row as f32 * 1.9,
        );
        let velocity = [0.9 + 0.1 * column as f32, -0.6 + 0.2 * row as f32];
        app.world_mut().spawn((
            battle_unit(5000 + index, pos, velocity),
            Transform::from_translation(pos),
            Visibility::Inherited,
        ));
    }
    let far = Vec3::new(client::scene_controller::SWARM_MAX_X + 40.0, 0.45, 0.0);
    app.world_mut().spawn((
        battle_unit(6000, far, [1.0, 0.0]),
        Transform::from_translation(far),
        Visibility::Inherited,
    ));
    let mut h = Fnv64::new();
    for frame in 1..=12 {
        app.update();
        if frame % 4 == 0 {
            hash_smog_app(&mut app, &mut h, ShowcasePhase::Unstepped);
        }
    }
    assert_eq!(
        require_smog_activity(&app),
        30,
        "the far unit is filtered out"
    );
    let hash = h.finish();
    assert_eq!(hash, GOLDEN_SYSTEMS_SWARM, "swarm golden: {hash:#018x}");
    Ok(())
}

/// Live battle tab on a 64x48 map: window placement from camera and units, tick-rate velocity
/// scale, vertical anchoring, the camera-volume filter, pre-warm and nearest-first cavity slots.
///
/// Class L: `expf` (decay factors), `powf` (showcase colours) and `sinf` (`bubble_team_color`
/// of the battle units' ids).
#[test]
fn smog_live_battle_systems_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        current_tab: client::LabTab::LiveBattle,
        ..default()
    })?;
    let map = sim_map::GameMap::new(
        64,
        48,
        sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
    )?;
    app.insert_resource(client::terrain::TerrainState::new(map));
    app.world_mut().spawn(client::camera::RtsCamera {
        target: Vec2::new(30.0, 21.0),
        look_at_y: 1.9,
        ..default()
    });
    for index in 0..22u32 {
        let pos = Vec3::new(
            22.0 + (index % 11) as f32 * 1.4,
            0.3 + (index % 3) as f32 * 0.9,
            17.0 + (index / 11) as f32 * 3.1,
        );
        let velocity = [
            0.04 + 0.01 * (index % 5) as f32,
            -0.03 + 0.02 * (index % 4) as f32,
        ];
        app.world_mut().spawn((
            battle_unit(9000 + index, pos, velocity),
            Transform::from_translation(pos),
            Visibility::Inherited,
        ));
    }
    let outside = Vec3::new(60.0, 0.45, 45.0);
    app.world_mut().spawn((
        battle_unit(9100, outside, [0.1, 0.1]),
        Transform::from_translation(outside),
        Visibility::Inherited,
    ));
    let mut h = Fnv64::new();
    for frame in 1..=12 {
        app.update();
        if frame % 4 == 0 {
            hash_smog_app(&mut app, &mut h, ShowcasePhase::Unstepped);
        }
    }
    assert_eq!(
        require_smog_activity(&app),
        22,
        "the unit off the window is filtered out"
    );
    let uniform = app.world().resource::<SmogGpuFrame>().uniform;
    assert_eq!(
        uniform.control_params.x,
        client::MAX_SMOG_SHADER_BUBBLES as f32,
        "more units than cavity slots"
    );
    let hash = h.finish();
    assert_eq!(
        hash, GOLDEN_SYSTEMS_LIVE_BATTLE,
        "live battle golden: {hash:#018x}"
    );
    Ok(())
}

/// The canary's recorded hash of every `libm` result the class-L goldens consume (glibc 2.41,
/// `x86_64`).
const GOLDEN_LIBM_CANARY: u64 = 0xd3dc_9154_746f_1272;

/// The canary's recorded hash of the `expf` decay factors the C-18 characterisation goldens add
/// (glibc 2.41, `x86_64`): the steps of [`VARIABLE_DTS`] and [`FAST_DT`] that
/// [`GOLDEN_LIBM_CANARY`] does not already cover. Kept separate so that constant stays as it was
/// recorded.
const GOLDEN_LIBM_CANARY_C18: u64 = 0x98d9_3224_8b8e_11df;

/// Pins the exact production `libm` results the class-L goldens consume, so a host whose `libm`
/// differs from the recording host fails here with a message that says so.
///
/// It hashes the bits of: the fluid's three `expf` decay factors, written exactly as in
/// `fluid.rs`, for the default and crowd configurations (the system goldens' effective
/// configuration changes only emission and wake, not the decay rates) at the golden step, the
/// manual clock's step and the pre-warm step (and, in the separate [`GOLDEN_LIBM_CANARY_C18`],
/// at the C-18 goldens' other steps: 1/240, 1/144 and 0.05 s); `bubble_team_color` (`sinf`) for every index a
/// class-L golden uses beyond the palette; and `Color::to_linear` (`powf`) of the ten showcase
/// palette colours. The noise texel's calls are too many to list; [`smog_noise_texture_golden`]
/// covers them.
#[test]
fn smog_golden_libm_canary() {
    let mut h = Fnv64::new();
    let clock_dt = Duration::from_secs_f32(STEP_DT).as_secs_f32();
    let prewarm_dt = 1.0_f32 / 30.0;
    for c in [SmogConfig::default(), crowd_config()] {
        for dt in [STEP_DT, clock_dt, prewarm_dt] {
            h.f32((-c.velocity_damping * dt).exp());
            h.f32((-c.dissipation_rate * dt).exp());
            h.f32((-(1.8 * c.dissipation_rate).max(1.65) * dt).exp());
        }
    }
    let team_indices = (10..48)
        .chain([77])
        .chain(5000..=5030)
        .chain([6000])
        .chain(9000..9022)
        .chain([9100]);
    for index in team_indices {
        h.vec3(bubble_team_color(index));
    }
    for c in client::smog::kinematics::SHOWCASE_BUBBLE_PALETTE {
        let linear = Color::srgb(c.x, c.y, c.z).to_linear();
        h.f32(linear.red);
        h.f32(linear.green);
        h.f32(linear.blue);
        h.f32(linear.alpha);
    }
    let hash = h.finish();
    let mut c18 = Fnv64::new();
    for c in [SmogConfig::default(), crowd_config()] {
        for dt in [VARIABLE_DTS[0], VARIABLE_DTS[1], FAST_DT] {
            c18.f32((-c.velocity_damping * dt).exp());
            c18.f32((-c.dissipation_rate * dt).exp());
            c18.f32((-(1.8 * c.dissipation_rate).max(1.65) * dt).exp());
        }
    }
    let c18_hash = c18.finish();
    assert_eq!(
        c18_hash, GOLDEN_LIBM_CANARY_C18,
        "libm canary (C-18 steps) {c18_hash:#018x}: this host's expf differs from the recording \
         host (glibc 2.41, x86_64), so the C-18 characterisation goldens are not comparable on \
         this host. The fix is owner question \
         docs/questions/open/0014-host-independent-smog-math.md, not re-recording the goldens."
    );
    assert_eq!(
        hash, GOLDEN_LIBM_CANARY,
        "libm canary {hash:#018x}: this host's libm (expf, sinf or powf) differs from the \
         recording host (glibc 2.41, x86_64), so the class-L smog goldens (fluid, startup, noise \
         and system goldens) are not comparable on this host. The fix is owner question \
         docs/questions/open/0014-host-independent-smog-math.md (host-independent smog math), \
         not re-recording the goldens."
    );
}
