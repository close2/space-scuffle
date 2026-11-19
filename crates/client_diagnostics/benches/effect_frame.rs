//! Offline bench of the effect runtime's frame build under the frame-budget stress (plan
//! `2026-11-19-retained-effect-instances` D-14, instrument 1).
//!
//! # Responsibility
//! Drives the production [`EffectRuntime`] with no renderer, from the frame-budget stress sources
//! and the shipped vocabulary, at 1/60 s frames: [`WARM_FRAMES`] warm-up frames, then
//! `--frames` measured frames (default [`MEASURED_FRAMES`]). Two streams:
//! - `s`: the S arm, 200 splash events a second over every weapon whose block names a splash
//!   (`stress_sources`, `stress_event`), spawned through the production `spawn_request` and
//!   `LineAssembly`, as the planner's B-137 bench did;
//! - `p`: the plan §10.3 stress, the S stream plus the effect mix's held set
//!   (`EffectMix::step` with no shots or deaths: 20 areas, 4 tethers, 2 spell missiles and 50
//!   held unit looks) on 400 synthetic units that circle slowly, so the held looks' per-frame
//!   updates run as in a battle.
//!
//! It prints the thread CPU time per frame of the spawn phase and of `build_frame`
//! (`CLOCK_THREAD_CPUTIME_ID`), the live effects, cards, decals and lights of the last frame,
//! and with `--digest` a 64-bit FNV-1a of every frame's uploaded bytes (cards back to front,
//! decals, lights, distortions and the darkening counters), per 100 measured frames and in
//! total.
//!
//! # How it fits
//! Wall time on the shared host varies by a factor of two between identical runs, instruction
//! counts do not: the per-WP check runs this bench under `perf stat -e instructions:u` for the
//! base and the head on one host, and the digest proves that a CPU-only change left every byte
//! of every frame unchanged. The digest depends on the host's `libm` (`tanf`, `atan2f`), so no
//! digest is committed as a golden.
//!
//! # Invariants
//! - The stream is a pure function of the frame index: no clock, no randomness beyond the fixed
//!   `SplitMix64` hashes of the stress tool, so two runs on one host build the same frames.
//! - Nothing here names an effect, a unit or a weapon (charter C5): every source comes from the
//!   catalogs.
//!
//! Run with `cargo bench -p client_diagnostics --bench effect_frame -- --stream s|p [--digest]
//! [--frames N]`.

use bevy::math::Vec3;
use client::effects::areas::Keep;
use client::effects::catalog::{EffectCatalog, UnitEffectTables, EFFECTS_CATALOG_PATH};
use client::effects::runtime::{CameraView, EffectRuntime};
use client::effects::systems::{spawn_request, LineAssembly};
use client_diagnostics::cpu_clock::thread_cpu_time;
use client_diagnostics::frame_budget::effect_mix::{EffectMix, MixConfig};
use client_diagnostics::frame_budget::effect_stress::{
    mix, stress_event, stress_points, stress_sources, StressSource,
};
use sim_types::snapshot::{BubbleRenderInstance, CombatEventVisual};
use sim_types::{AnimState, ShapeKind, UnitTypeId, WeaponVisualKind};
use std::error::Error;
use std::path::Path;
use std::time::Duration;

/// Warm-up frames before the measured window.
const WARM_FRAMES: u32 = 600;

/// Measured frames when `--frames` is not given.
const MEASURED_FRAMES: u32 = 2400;

/// Frame step, seconds (60 frames a second).
const FRAME_STEP: f64 = 1.0 / 60.0;

/// Splash events a second (the S arm's `--effect-stress 200`).
const SPLASHES_PER_SECOND: f64 = 200.0;

/// Synthetic units of the `p` stream, a square grid (the 400-unit battle).
const UNITS_PER_SIDE: u32 = 20;

/// The stream's corner in the simulation's ground plane, and its side, world units: the frame
/// budget's stress area.
const AREA_CORNER: (f32, f32) = (44.5, 52.5);

/// Side of the stream's square, world units.
const AREA_SIDE: f32 = 40.0;

/// Digest lines are printed every this many measured frames.
const DIGEST_EVERY: u32 = 100;

/// FNV-1a 64-bit offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x0100_0000_01b3;

/// Which stream the bench drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    /// The S arm.
    Splashes,
    /// The §10.3 stress, P.
    Plan,
}

/// The command line.
#[derive(Debug, Clone, Copy)]
struct Options {
    stream: Stream,
    digest: bool,
    frames: u32,
    /// Print every measured frame's digest, and the digest with the darkening cards in a
    /// canonical order (they multiply the ground, so their order draws nothing different).
    frame_digests: bool,
}

/// Reads the command line (cargo bench adds `--bench`, which is ignored).
fn options() -> Result<Options, Box<dyn Error>> {
    let mut options = Options {
        stream: Stream::Splashes,
        digest: false,
        frames: MEASURED_FRAMES,
        frame_digests: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stream" => {
                options.stream = match args.next().as_deref() {
                    Some("s") => Stream::Splashes,
                    Some("p") => Stream::Plan,
                    other => return Err(format!("--stream takes s or p, not {other:?}").into()),
                };
            }
            "--digest" => options.digest = true,
            "--frame-digests" => options.frame_digests = true,
            "--frames" => {
                options.frames = args
                    .next()
                    .ok_or("--frames takes a count")?
                    .parse()
                    .map_err(|error| format!("--frames: {error}"))?;
            }
            "--bench" => {}
            other => return Err(format!("unknown argument {other:?}").into()),
        }
    }
    Ok(options)
}

/// FNV-1a over `bytes`, continuing from `hash`.
fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The digest of everything the renderer uploads from `runtime`'s frame.
fn frame_digest(hash: u64, runtime: &EffectRuntime) -> u64 {
    let frame = &runtime.frame;
    let mut hash = fnv(hash, bytemuck::cast_slice(&frame.sorted));
    hash = fnv(hash, bytemuck::cast_slice(&frame.decals));
    for light in &frame.lights {
        for value in light
            .position
            .iter()
            .chain(light.color.iter())
            .chain([light.intensity, light.range].iter())
        {
            hash = fnv(hash, &value.to_bits().to_le_bytes());
        }
    }
    hash = fnv(hash, bytemuck::cast_slice(&frame.distortions));
    hash = fnv(hash, &frame.darkening_cards.to_le_bytes());
    fnv(hash, &frame.darkening_decals.to_le_bytes())
}

/// [`frame_digest`] of one frame, with its darkening cards (the first `darkening_cards` of
/// `sorted`) taken in byte order instead of sort order.
fn canonical_digest(runtime: &EffectRuntime) -> u64 {
    let frame = &runtime.frame;
    let prefix = (frame.darkening_cards as usize).min(frame.sorted.len());
    let mut darkening: Vec<&[u8]> = frame.sorted[..prefix]
        .iter()
        .map(bytemuck::bytes_of)
        .collect();
    darkening.sort_unstable();
    let mut hash = FNV_OFFSET;
    for card in darkening {
        hash = fnv(hash, card);
    }
    hash = fnv(hash, bytemuck::cast_slice(&frame.sorted[prefix..]));
    fnv(hash, bytemuck::cast_slice(&frame.decals))
}

/// The synthetic units of the `p` stream at `now`: a grid, each circling its own cell slowly.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "unit indices below 400 and seconds of a short run are exact enough in f32"
)]
fn units_at(units: &mut Vec<BubbleRenderInstance>, now: f64) {
    units.clear();
    let spacing = AREA_SIDE / UNITS_PER_SIDE as f32;
    for index in 0..UNITS_PER_SIDE * UNITS_PER_SIDE {
        let (column, row) = (index % UNITS_PER_SIDE, index / UNITS_PER_SIDE);
        let phase = now as f32 * 0.5 + index as f32;
        let x = AREA_CORNER.0 + (column as f32 + 0.5) * spacing + 0.4 * phase.cos();
        let y = AREA_CORNER.1 + (row as f32 + 0.5) * spacing + 0.4 * phase.sin();
        units.push(BubbleRenderInstance {
            entity_id: sim_types::EntityId(index + 1),
            shape: ShapeKind::UnitSphere,
            silhouette: sim_types::SilhouetteClass::HeavyHex,
            unit_type: UnitTypeId::default(),
            owner: sim_types::PlayerId(u8::try_from(index % 2).unwrap_or(0) + 1),
            world_pos: [x, y, 0.5],
            ground_z: 0.0,
            radius: 0.5,
            velocity: [0.0; 2],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: AnimState::Idle,
            attack_swing: None,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            concealment: sim_types::Concealment::None,
            speed_boosted: false,
            stance: sim_types::StanceView::default(),
            status: sim_types::StatusFlags::default(),
            barrier_ratio: 0.0,
        });
    }
}

/// The S arm's splashes owed this frame, spawned as the production event wiring spawns them.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "16-bit hash fields are exact in f32; the source count is small"
)]
fn spawn_splashes(world: &mut World, owed: &mut f64, sequence: &mut u64, now: f64) {
    *owed += FRAME_STEP * SPLASHES_PER_SECOND;
    let mut lines = LineAssembly::default();
    while *owed >= 1.0 {
        *owed -= 1.0;
        let hash = mix(*sequence);
        let source = &world.sources[(*sequence % world.sources.len() as u64) as usize];
        *sequence += 1;
        let x = AREA_CORNER.0 + (hash & 0xFFFF) as f32 / 65536.0 * AREA_SIDE;
        let y = AREA_CORNER.1 + ((hash >> 16) & 0xFFFF) as f32 / 65536.0 * AREA_SIDE;
        let angle = ((hash >> 32) & 0xFFFF) as f32 / 65536.0 * std::f32::consts::TAU;
        for index in 0..stress_points(source) {
            let event = stress_event(source, [x, y, 0.0], angle, index);
            if let Some(ready) = spawn_request(&event, &world.tables, &world.catalog)
                .and_then(|request| lines.push(request))
            {
                world.runtime.spawn(&world.catalog, &ready, now);
            }
        }
    }
    if let Some(ready) = lines.finish() {
        world.runtime.spawn(&world.catalog, &ready, now);
    }
}

/// Everything the bench drives.
struct World {
    catalog: EffectCatalog,
    tables: UnitEffectTables,
    sources: Vec<StressSource>,
    runtime: EffectRuntime,
    mix: EffectMix,
    units: Vec<BubbleRenderInstance>,
}

/// CPU time of the calling thread, or an error naming the clock.
fn thread_now() -> Result<Duration, Box<dyn Error>> {
    Ok(thread_cpu_time()?)
}

/// The catalogs, the stream's sources and a fresh runtime.
fn load_world() -> Result<World, Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()?;
    let definition = client::runtime_battle::BattleDefinition::load(
        &root,
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;
    let catalog = EffectCatalog::load(&root.join(EFFECTS_CATALOG_PATH), &root)?;
    let units = &definition.catalogs.units;
    let tables = UnitEffectTables::build(units, &catalog)?;
    let sources = stress_sources(units, &tables, &catalog);
    if sources.is_empty() {
        return Err("no weapon of the shipped catalog names a splash look".into());
    }
    let mut mix = EffectMix::new(MixConfig {
        shots_per_second: 0,
        deaths_per_second: 0,
        ..MixConfig::at_rate(0)
    });
    mix.load(units, &tables, &catalog);
    let world = World {
        runtime: EffectRuntime::new(&catalog),
        catalog,
        tables,
        sources,
        mix,
        units: Vec::with_capacity((UNITS_PER_SIDE * UNITS_PER_SIDE) as usize),
    };
    Ok(world)
}

/// Spawns frame `now`'s effects of `stream` into the world.
fn spawn_frame(
    world: &mut World,
    stream: Stream,
    (owed, sequence): (&mut f64, &mut u64),
    now: f64,
) {
    spawn_splashes(world, owed, sequence, now);
    if stream == Stream::Plan {
        units_at(&mut world.units, now);
        let mut keep = Keep {
            catalog: &world.catalog,
            tables: &world.tables,
            runtime: &mut world.runtime,
            now,
            delta: FRAME_STEP,
        };
        world
            .mix
            .step(&world.units, &mut keep, &mut |_: CombatEventVisual| {});
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = options()?;
    let mut world = load_world()?;
    // The frame budget's camera pose: 75 units out at the battle camera's pitch.
    let target = Vec3::new(64.5, 0.0, 72.5);
    let position = target + Vec3::new(0.0, 75.0 * 0.85, 75.0 * 0.53);
    let camera = CameraView::new(
        position,
        (target - position).normalize(),
        std::f32::consts::FRAC_PI_4,
        720.0,
        16.0 / 9.0,
    );
    let (mut spawn_cpu, mut build_cpu) = (Duration::ZERO, Duration::ZERO);
    let (mut owed, mut sequence) = (0.0, 0_u64);
    let (mut digest, mut window_digest) = (FNV_OFFSET, FNV_OFFSET);
    for frame in 0..WARM_FRAMES + options.frames {
        let now = f64::from(frame) * FRAME_STEP;
        let started = thread_now()?;
        spawn_frame(&mut world, options.stream, (&mut owed, &mut sequence), now);
        let spawned = thread_now()?;
        world
            .runtime
            .build_frame(&world.catalog, now, Some(&camera));
        let built = thread_now()?;
        if frame < WARM_FRAMES {
            continue;
        }
        spawn_cpu += spawned.saturating_sub(started);
        build_cpu += built.saturating_sub(spawned);
        if options.frame_digests {
            println!(
                "frame {frame:>5}: {:016x} {:016x}",
                frame_digest(FNV_OFFSET, &world.runtime),
                canonical_digest(&world.runtime)
            );
        }
        if options.digest {
            digest = frame_digest(digest, &world.runtime);
            window_digest = frame_digest(window_digest, &world.runtime);
            let measured = frame - WARM_FRAMES + 1;
            if measured.is_multiple_of(DIGEST_EVERY) {
                println!("digest frames {measured:>5}: {window_digest:016x}");
                window_digest = FNV_OFFSET;
            }
        }
    }
    let per_frame = |total: Duration| total.as_secs_f64() * 1e3 / f64::from(options.frames.max(1));
    let runtime = &world.runtime;
    println!(
        "stream {:?}, {} measured frames: spawn {:.3} ms/frame, build_frame {:.3} ms/frame \
         (thread CPU); live {}, cards {}, decals {}, lights {}; stats {:?}",
        options.stream,
        options.frames,
        per_frame(spawn_cpu),
        per_frame(build_cpu),
        runtime.pool().live_total(),
        runtime.frame.sorted.len(),
        runtime.frame.decals.len(),
        runtime.frame.lights.len(),
        runtime.stats,
    );
    if options.digest {
        println!("digest total: {digest:016x}");
    }
    Ok(())
}
