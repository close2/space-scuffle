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
//! # Host independence
//! Every golden here is host-independent: it pins the code, not the host's C math library. The
//! client may use the host's `libm` (charter §3.1 binds only the simulation crates), and the last
//! bit of `sinf`, `cosf`, `atan2f`, `expf` and `powf` differs between glibc versions, musl, macOS
//! and Windows. So every transcendental function on a hashed smog path is
//! `client::smog::math`, the pure-Rust `libm` crate, whose results are the same on every target
//! (owner question Q0014, option b; plan `docs/plans/2026-11-04-libm-smog-math.md`). Workspace
//! check C23 keeps smog production code off the host's functions, and the plan's interposer run
//! proved the hashes independent of them: with every host `libm` result moved by one ulp
//! (`LD_PRELOAD`), every golden here still passed (evidence
//! `docs/evidence/2026-11-05-libm-smog-math.md`). Headings, aims and rotations are hashed like
//! every other value. [`smog_libm_crate_canary`] pins the `libm` crate's results these goldens
//! consume, so a crate version that changes them fails there with a message saying so rather
//! than looking like a behaviour change.
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
//! - Crowd inputs: the production generator, [`generate_bouncing_bubbles`], for every crowd.
//! - Systems: the Visual Lab startup and showcase frames (textures, material, bubbles, uniform),
//!   the noise texture, a Visual Lab preview bubble, the swarm tab and the live battle tab (window
//!   placement, velocity scale, trajectory pre-warm and cavity slots).
//!
//! # Invariants
//! - The fluid constants were recorded on `main` at `9dbf715`, before any WP7 edit, and did not
//!   change when this file moved to the two classes of plan
//!   `docs/plans/2026-11-03-host-independent-smog-goldens.md` (since retired, see the last
//!   entries).
//! - Every other constant was recorded by running this file, unchanged, against `9dbf715` and then
//!   confirmed unchanged on `main` at `3f57fec` (glibc 2.41, `x86_64`), except those below.
//! - The four C-18 characterisation goldens and `GOLDEN_LIBM_CANARY_C18` (since retired) were recorded on
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
//! - C-18 WP7 and WP8 changed no constant (bit-exact refactors). C-18 WP9 re-recorded the seven
//!   fluid goldens and the four system goldens (glibc 2.41, `x86_64`) in one commit, for one
//!   deliberate change: velocities of at most `1e-3` world units per second become exactly zero
//!   (plan D-3, owner answer Q0020). The four kinematics goldens, the noise golden and both
//!   `libm` canaries are unchanged by it, and every golden keeps its class.
//! - Veo batch 1 (plan `2026-11-04-veo-structure-action-text` WP3) re-recorded the four system
//!   goldens (glibc 2.41, `x86_64`) because it ships five new atlases in `assets/textures`
//!   (`plasma_siphon_dome`, `tech_module_pod`, `machining_plant_dome`, `aerospace_hangar_dome`,
//!   `repair_shuttle`), and the production app these goldens run reads the real asset root, so
//!   bubbles of those identities now bind their atlases. With those five atlases moved out of
//!   `assets/textures`, all four reproduced their previous values, so no smog code or output
//!   changed. The fluid, kinematics and noise goldens and both `libm` canaries are unchanged.
//! - Veo batch 2 (plan `2026-11-04-phase-8-5-veo-media` WP5) re-recorded the four system goldens
//!   (glibc 2.41, `x86_64`) because it replaces four atlases in `assets/textures`
//!   (`welder_drone`, `capacitor_dome` and `assembly_bay_dome`, which were copies of the concept
//!   atlas, and `foundry_core_dome`). With the previous versions of those four atlases put back,
//!   all four reproduced their previous values, so no smog code or output changed. The fluid,
//!   kinematics and noise goldens and both `libm` canaries are unchanged.
//! - Veo batch 3 (plan `2026-11-04-phase-8-5-veo-media` WP6) re-recorded the four system goldens
//!   (glibc 2.41, `x86_64`) because it ships two new atlases in `assets/textures`
//!   (`jet_jumper`, `specter_android`). With those two atlases moved out, all four reproduced
//!   their previous values, so no smog code or output changed. The fluid, kinematics and noise
//!   goldens and both `libm` canaries are unchanged.
//! - Veo batch 4 (plan `2026-11-04-phase-8-5-veo-media` WP6) re-recorded the four system goldens
//!   (glibc 2.41, `x86_64`) because it ships twelve new atlases in `assets/textures`
//!   (`railgun_crawler`, `shock_golem` and the ten batch-4 structures). With those twelve atlases
//!   moved out, all four reproduced their previous values, so no smog code or output changed.
//!   The fluid, kinematics and noise goldens and both `libm` canaries are unchanged.
//! - Veo batch 5 (plan `2026-11-04-phase-8-5-veo-media` WP6) re-recorded the four system goldens
//!   (glibc 2.41, `x86_64`) because it ships four new atlases in `assets/textures`
//!   (`vector_interceptor`, `sky_bastion`, `cloak_gunship`, `omni_probe_drone`). With those four
//!   atlases moved out, all four reproduced their previous values, so no smog code or output
//!   changed. The fluid, kinematics and noise goldens and both `libm` canaries are unchanged.
//! - Veo batch 5b (plan `2026-11-04-phase-8-5-veo-media` WP6) re-recorded the four system
//!   goldens (glibc 2.41, `x86_64`) because it ships one new atlas in `assets/textures`
//!   (`napalm_wheeler`). With that atlas moved out, all seventeen tests passed on the previous
//!   values, so no smog code or output changed. The fluid, kinematics and noise goldens and both
//!   `libm` canaries are unchanged.
//! - Q0014 option (b) (plan `docs/plans/2026-11-04-libm-smog-math.md`, WP2) moved every host
//!   `libm` call on a smog path to the `libm` crate (at most about one ulp per call), hashed
//!   headings, aims and rotations, and made the crowd scenarios call the generator instead of the
//!   recorded trig table. The seven fluid goldens and the noise golden did not change: on their
//!   arguments the crate's results equal glibc 2.41's after rounding. The four kinematics and four
//!   system goldens were re-recorded in one commit, which lists every old and new value: headings,
//!   aims and rotations now enter their hashes, the 400- and 600-bubble crowds start from the
//!   crate's trig (28 of the 2,400 values differ from glibc's by one ulp), and four showcase
//!   colours' linear values differ by one ulp (`powf`). The two classes, the recorded crowd trig
//!   table and `smog_golden_libm_canary` were retired with it.
//! - A golden may only change together with a deliberate, documented change in smog output.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::unwrap_used,
    clippy::expect_used
)]

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

/// Fluid steps run per fluid scenario.
const FLUID_STEPS: usize = 120;

/// Kinematics steps run per kinematics scenario.
const KINEMATICS_STEPS: usize = 120;

/// Every how many fluid steps the whole field is hashed (metrics are hashed every step).
const FIELD_CHECKPOINT: usize = 30;

/// Fixed fluid and kinematics step, in seconds.
const STEP_DT: f32 = 1.0 / 60.0;

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

    /// Hashes a whole kinematics state, heading included.
    fn state(&mut self, s: &BouncingBubbleState) {
        self.u64(s.index as u64);
        self.vec3(s.position);
        self.vec3(s.velocity);
        self.f32(s.radius);
        self.f32(s.heading_angle_rad);
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

const GOLDEN_FLUID_SHOWCASE: u64 = 0x2b35_6d55_7c4a_1810;
const GOLDEN_FLUID_MIXED_24: u64 = 0x743d_d564_7e66_28b6;
const GOLDEN_FLUID_CROWD_48: u64 = 0xe040_edb2_fa5b_187e;

/// The showcase: ten emitting bubbles with confinement on, default configuration.
///
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
#[test]
fn fluid_mixed_sources_golden() -> Result<(), SmogError> {
    let config = crowd_config();
    let states = generate_bouncing_bubbles(24, &config.bounds)?;
    let hash = fluid_scenario_hash(states, &config, SourceMix::Mixed)?;
    assert_eq!(
        hash, GOLDEN_FLUID_MIXED_24,
        "fluid mixed golden: {hash:#018x}"
    );
    Ok(())
}

/// Forty-eight mixed sources: past the 32-source limit, so the crowd wake radius applies and
/// confinement is skipped.
#[test]
fn fluid_crowd_golden() -> Result<(), SmogError> {
    let config = crowd_config();
    let states = generate_bouncing_bubbles(48, &config.bounds)?;
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

const GOLDEN_FLUID_WAKE_DECAY: u64 = 0x2ff5_8b66_774b_7a2b;
const GOLDEN_FLUID_FAST_SOURCES: u64 = 0x5fb4_f8c6_25fd_54e2;
const GOLDEN_FLUID_BOUNDS_SHIFT: u64 = 0xea2a_91d9_b0cc_39c0;
const GOLDEN_FLUID_VARIABLE_DT: u64 = 0x55ae_4990_ed7a_4886;

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
/// exactly zero; the velocity field does not everywhere within these 4 s: the quiescence of
/// `docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md` D-3 zeroes it about 19 s after the
/// sources stop), so a rest flag that is set too early, or a stale `occupied` flag, changes this
/// hash.
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
#[test]
fn fluid_fast_sources_golden() -> Result<(), SmogError> {
    let config = SmogConfig::default();
    let mut states = generate_bouncing_bubbles(FAST_SOURCE_COUNT, &config.bounds)?;
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
#[test]
fn fluid_variable_dt_golden() -> Result<(), SmogError> {
    let config = SmogConfig::default();
    let max_count = VARIABLE_SOURCE_COUNTS[1];
    let mut states = generate_bouncing_bubbles(max_count, &config.bounds)?;
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

/// Steps kinematics for [`KINEMATICS_STEPS`] and hashes the event counts and every whole state
/// after every step.
fn kinematics_hash(
    mut states: Vec<BouncingBubbleState>,
    bounds: &SmogArenaBounds,
) -> Result<u64, SmogError> {
    let mut h = Fnv64::new();
    let mut total_walls = 0;
    for _ in 0..KINEMATICS_STEPS {
        let (walls, peers) = step_bouncing_kinematics(&mut states, bounds, STEP_DT)?;
        total_walls += walls;
        h.u64(walls);
        h.u64(peers);
        for state in &states {
            h.state(state);
        }
    }
    assert!(total_walls > 0, "the scenario must reach a wall");
    assert!(
        states.iter().any(|state| state.peer_bounces > 0),
        "the scenario must resolve a contact"
    );
    Ok(h.finish())
}

const GOLDEN_KINEMATICS_SHOWCASE: u64 = 0x1e86_3e23_56af_c847;
const GOLDEN_KINEMATICS_NON_PLANAR: u64 = 0x7144_4102_3899_d51b;
const GOLDEN_KINEMATICS_GRID_400: u64 = 0xe786_bf73_4bd4_c006;
/// Re-recorded for Phase 9 §9 gap 3 (WP1 of the 2026-11-04 open-findings plan): the all-pairs
/// fallback now returns its real contact count, so each step's hashed `peers` value changed. The
/// positions, velocities and per-bubble counters did not. Previous value: `0x8d1d_c59d_6fba_ec0f`.
/// Re-recorded again for Q0014 (b) with every other golden (see the module's invariants).
const GOLDEN_KINEMATICS_FALLBACK_600: u64 = 0x94ee_0403_20d7_35d8;

/// The ten showcase bubbles on the floor: the planar contact branch.
#[test]
fn kinematics_showcase_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let states = initial_bouncing_bubbles(&bounds)?.to_vec();
    let hash = kinematics_hash(states, &bounds)?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_SHOWCASE,
        "kinematics showcase golden: {hash:#018x}"
    );
    Ok(())
}

/// Bubbles lifted off the floor with vertical velocity: the 3D (not planar) contact branch.
#[test]
fn kinematics_non_planar_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let mut states = initial_bouncing_bubbles(&bounds)?.to_vec();
    for (i, state) in states.iter_mut().enumerate() {
        state.position.y = bounds.min.y + state.radius + 0.3 + (i % 3) as f32 * 0.5;
        state.velocity.y = if i % 2 == 0 { 1.7 } else { -2.3 };
    }
    let hash = kinematics_hash(states, &bounds)?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_NON_PLANAR,
        "kinematics non-planar golden: {hash:#018x}"
    );
    Ok(())
}

/// The 400-bubble crowd: the spatial-hash contact path.
#[test]
fn kinematics_spatial_grid_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let states = generate_bouncing_bubbles(400, &bounds)?;
    let hash = kinematics_hash(states, &bounds)?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_GRID_400,
        "kinematics grid golden: {hash:#018x}"
    );
    Ok(())
}

/// The 600-bubble crowd: the all-pairs fallback beyond [`client::smog::MAX_BOUNCING_BUBBLES`].
#[test]
fn kinematics_all_pairs_fallback_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let states = generate_bouncing_bubbles(600, &bounds)?;
    let hash = kinematics_hash(states, &bounds)?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_FALLBACK_600,
        "kinematics fallback golden: {hash:#018x}"
    );
    Ok(())
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
        concealment: sim_types::Concealment::None,
        speed_boosted: false,
        stance: sim_types::StanceView::default(),
        status: sim_types::StatusFlags::default(),
        barrier_ratio: 0.0,
    })
}

/// Hashes the published uniform, stats, field, volume proxy and every showcase bubble: its
/// kinematics, transform (rotation included), render instance (heading and aim included),
/// visibility and material uniform.
fn hash_smog_app(app: &mut App, h: &mut Fnv64) {
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
        h.quat(transform.rotation);
        h.vec3(transform.scale);
        h.bytes(format!("{instance:?}").as_bytes());
        h.bytes(format!("{visibility:?}").as_bytes());
        let material = materials.get(material).expect("bubble material exists");
        h.bytes(bytemuck::bytes_of(&material.uniform));
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

const GOLDEN_SYSTEMS_STARTUP: u64 = 0xe79e_7028_2b67_1063;
const GOLDEN_SYSTEMS_NOISE: u64 = 0x1fe2_2cc0_e03e_f783;
const GOLDEN_SYSTEMS_VISUAL_LAB: u64 = 0xdef4_dd2e_c2ae_a6be;
const GOLDEN_SYSTEMS_SWARM: u64 = 0x410b_6916_d521_92c1;
const GOLDEN_SYSTEMS_LIVE_BATTLE: u64 = 0xf536_fd48_5dc1_a93f;

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
#[test]
fn smog_startup_assets_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = startup_app()?;
    let mut h = Fnv64::new();
    hash_smog_startup_assets(&app, &mut h);
    hash_smog_app(&mut app, &mut h);
    let hash = h.finish();
    assert_eq!(hash, GOLDEN_SYSTEMS_STARTUP, "startup golden: {hash:#018x}");
    Ok(())
}

/// The tileable 3D noise texture created at startup: bytes, descriptor, sampler and usage.
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

/// Visual Lab showcase with a preview bubble, through both systems for 40 frames: the showcase
/// bubbles turn with their headings, and the preview takes `bubble_team_color(77)`.
#[test]
fn smog_visual_lab_systems_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        smog_bouncing_active: true,
        smog_emission_rate: 4.1,
        smog_wake_influence: 2.6,
        ..default()
    })?;
    app.update();
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
        if frame % 10 == 0 {
            hash_smog_app(&mut app, &mut h);
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
            hash_smog_app(&mut app, &mut h);
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
            hash_smog_app(&mut app, &mut h);
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

/// The canary's recorded hash of the `libm` crate (0.2.16) results the goldens consume.
const GOLDEN_LIBM_CRATE_CANARY: u64 = 0x7ed6_18b3_f95d_ef41;

/// Arguments of the canary's `atan2` and `sin_cos` sweep.
const CANARY_SWEEP: usize = 64;

/// Pins the `libm` crate's results for the arguments the goldens consume, so a crate version
/// that changes them fails here, with a message naming the cause, rather than as a behaviour
/// change in every golden.
///
/// It hashes the bits of: the fluid's three decay factors, written as in `fluid.rs`, for the
/// default and crowd configurations at every step the goldens use (1/60 s, the manual clock's
/// step, the pre-warm's 1/30 s, the variable-`dt` golden's 1/240 s and 1/144 s, and the 0.05 s
/// clamp); `bubble_team_color` for every index a golden uses beyond the palette; the linear
/// colour of the ten showcase palette colours; and `atan2` and `sin_cos` on a fixed sweep around
/// the circle (headings and rotations). The noise texel's calls are too many to list;
/// [`smog_noise_texture_golden`] covers them.
#[test]
fn smog_libm_crate_canary() {
    use client::smog::math;
    let mut h = Fnv64::new();
    let clock_dt = Duration::from_secs_f32(STEP_DT).as_secs_f32();
    let dts = [
        STEP_DT,
        clock_dt,
        1.0 / 30.0,
        VARIABLE_DTS[0],
        VARIABLE_DTS[1],
        FAST_DT,
    ];
    for c in [SmogConfig::default(), crowd_config()] {
        for dt in dts {
            h.f32(math::exp(-c.velocity_damping * dt));
            h.f32(math::exp(-c.dissipation_rate * dt));
            h.f32(math::exp(-(1.8 * c.dissipation_rate).max(1.65) * dt));
        }
    }
    let team_indices = (10..600)
        .chain(5000..=5030)
        .chain([6000])
        .chain(9000..9022)
        .chain([9100]);
    for index in team_indices {
        h.vec3(bubble_team_color(index));
    }
    for c in client::smog::kinematics::SHOWCASE_BUBBLE_PALETTE {
        let linear = math::srgb_to_linear(c);
        h.f32(linear.red);
        h.f32(linear.green);
        h.f32(linear.blue);
        h.f32(linear.alpha);
    }
    for i in 0..CANARY_SWEEP {
        let angle = (i as f32 / CANARY_SWEEP as f32 - 0.5) * std::f32::consts::TAU;
        h.f32(math::atan2(3.0 * angle, 2.0 - angle));
        h.quat(math::quat_from_rotation_y(angle));
    }
    let hash = h.finish();
    assert_eq!(
        hash, GOLDEN_LIBM_CRATE_CANARY,
        "libm crate canary {hash:#018x}: the `libm` crate's results changed (crate version \
         bump?); re-record the smog goldens in a commit that names the version change"
    );
}
