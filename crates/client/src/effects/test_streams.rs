//! Effect streams for the runtime's unit tests (plan `2026-11-19-retained-effect-instances`
//! WP2 to WP4): the shipped vocabulary driven through every runtime API a battle uses.
//!
//! # Responsibility
//! Owns [`TestStream`], a deterministic stream of spawns and changes at 1/60 s frames:
//! - [`StreamKind::Splashes`]: 200 events a second, round-robin over every spawned look of the
//!   vocabulary (splashes, points, deaths, lines and shots), at hashed places on a 40-unit
//!   square; a line is requested again on the next frame with more of its points seen, as a
//!   line whose searches land on several ticks is (`merge_line`);
//! - [`StreamKind::Plan`]: the same plus held areas whose fog clip moves every frame, tethers
//!   whose ends move (`update_held_line`), held unit looks that follow their units
//!   (`update_held`) and are released and held anew, and lasting marks moved by
//!   `move_anchor`, as the frame budget's §10.3 stress keeps them.
//!
//! # How it fits
//! `client` cannot use the diagnostics crate (the dependency runs the other way), so the
//! runtime's oracle tests drive this stream built from the shipped catalogs instead of the frame
//! budget's own S and P streams; the `effect_frame` bench drives those for the digests. Test
//! code only.
//!
//! # Invariants
//! - The stream is a pure function of the frame index.
//! - No effect is named: every look comes from the vocabulary by its event kind.

use super::binding::BoundValues;
use super::catalog::schema::{Binding, EventKind};
use super::catalog::{EffectCatalog, EffectId, EFFECTS_CATALOG_PATH};
use super::placement::{hash3, unit_float, WorldLine};
use super::pool::EffectHandle;
use super::runtime::{CameraView, EffectRuntime, SpawnRequest};
use bevy::math::Vec3;

/// Which stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    /// Spawned looks only.
    Splashes,
    /// Spawned looks and the held set.
    Plan,
}

/// Seconds per frame.
pub const FRAME_STEP: f64 = 1.0 / 60.0;

/// Events a second.
const EVENTS_PER_SECOND: f64 = 200.0;

/// Held areas, tethers and unit looks of the plan stream.
const HELD: (usize, usize, usize) = (20, 4, 50);

/// Frames between releases of one held unit look.
const RELEASE_EVERY: u32 = 90;

/// A deterministic stream of spawns and changes.
#[derive(Debug)]
pub struct TestStream {
    /// The shipped vocabulary.
    pub catalog: EffectCatalog,
    kind: StreamKind,
    frame: u32,
    owed: f64,
    sequence: u32,
    spawned: Vec<EffectId>,
    areas: Vec<EffectId>,
    links: Vec<EffectId>,
    looks: Vec<EffectId>,
    held_areas: Vec<Option<EffectHandle>>,
    held_links: Vec<Option<EffectHandle>>,
    held_looks: Vec<Option<EffectHandle>>,
    /// A line spawned last frame, requested again this frame with more points seen.
    again: Option<SpawnRequest>,
    /// The last spawned effect, moved every frame.
    moved: Option<EffectHandle>,
}

/// The shipped vocabulary from the repository's assets.
///
/// # Panics
/// When the shipped vocabulary does not load (a test failure).
#[must_use]
#[allow(clippy::expect_used, reason = "test support: a failure is a panic")]
pub fn shipped_catalog() -> EffectCatalog {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    EffectCatalog::load(&root.join(EFFECTS_CATALOG_PATH), &root)
        .expect("invariant: the repository's shipped vocabulary loads, which its own tests check")
}

/// The frame budget's camera: 75 units out at the battle camera's pitch over the stream.
#[must_use]
pub fn stream_camera() -> CameraView {
    let target = Vec3::new(20.0, 0.0, 20.0);
    let position = target + Vec3::new(0.0, 75.0 * 0.85, 75.0 * 0.53);
    CameraView::new(
        position,
        (target - position).normalize(),
        std::f32::consts::FRAC_PI_4,
        720.0,
        16.0 / 9.0,
    )
}

/// The generic values an event binds.
fn bound(seed: u32) -> BoundValues {
    BoundValues::default()
        .with(Binding::SplashFull, 0.4 + unit_float(hash3(seed, 1, 0)))
        .with(Binding::SplashMid, 0.8 + unit_float(hash3(seed, 2, 0)))
        .with(Binding::SplashReach, 1.2 + unit_float(hash3(seed, 3, 0)))
        .with(Binding::LineSpacing, 0.4)
        .with(Binding::AreaRadius, 1.5)
        .with(Binding::AreaRemaining, 50.0)
        .with(Binding::AreaTotal, 60.0)
        .with(Binding::UnitRadius, 0.5)
        .with(Binding::UnitHeight, 1.0)
        .with(Binding::BarrierRatio, unit_float(hash3(seed, 4, 0)))
        .with(
            Binding::Facing,
            std::f32::consts::TAU * unit_float(hash3(seed, 5, 0)),
        )
}

/// A point of the stream's square for hash `seed`.
fn place(seed: u32) -> Vec3 {
    Vec3::new(
        40.0 * unit_float(hash3(seed, 6, 0)),
        0.0,
        40.0 * unit_float(hash3(seed, 7, 0)),
    )
}

impl TestStream {
    /// A stream of `kind` over the shipped vocabulary.
    #[must_use]
    pub fn new(kind: StreamKind) -> Self {
        let catalog = shipped_catalog();
        let of = |kinds: &[EventKind]| -> Vec<EffectId> {
            (0..catalog.effects.len())
                .filter_map(|index| u16::try_from(index).ok().map(EffectId))
                .filter(|id| {
                    catalog
                        .effect(*id)
                        .is_some_and(|effect| kinds.contains(&effect.event))
                })
                .collect()
        };
        let spawned = of(&[
            EventKind::Splash,
            EventKind::Point,
            EventKind::Death,
            EventKind::Line,
            EventKind::Shot,
        ]);
        let areas = of(&[EventKind::Area]);
        let links = of(&[EventKind::Link]);
        let looks = of(&[EventKind::Status, EventKind::Mode]);
        Self {
            catalog,
            kind,
            frame: 0,
            owed: 0.0,
            sequence: 0,
            spawned,
            areas,
            links,
            looks,
            held_areas: vec![None; HELD.0],
            held_links: vec![None; HELD.1],
            held_looks: vec![None; HELD.2],
            again: None,
            moved: None,
        }
    }

    /// Time of the current frame, seconds.
    #[must_use]
    pub fn now(&self) -> f64 {
        f64::from(self.frame) * FRAME_STEP
    }

    /// Spawns and changes this frame's effects in `runtime`, then advances to the next frame;
    /// returns the time of the frame the caller then builds.
    pub fn step(&mut self, runtime: &mut EffectRuntime) -> f64 {
        let now = self.now();
        if let Some(again) = self.again.take() {
            runtime.spawn(&self.catalog, &again, now);
        }
        self.owed += FRAME_STEP * EVENTS_PER_SECOND;
        while self.owed >= 1.0 && !self.spawned.is_empty() {
            self.owed -= 1.0;
            self.spawn_one(runtime, now);
        }
        if self.kind == StreamKind::Plan {
            self.keep_held(runtime, now);
        }
        self.frame += 1;
        now
    }

    /// Spawns the next look of the round-robin.
    fn spawn_one(&mut self, runtime: &mut EffectRuntime, now: f64) {
        let seed = hash3(self.sequence, 0x57AE_A111, 0);
        let effect = self.spawned[self.sequence as usize % self.spawned.len()];
        self.sequence += 1;
        let anchor = place(seed);
        let kind = self.catalog.effect(effect).map(|effect| effect.event);
        let line = matches!(kind, Some(EventKind::Line | EventKind::Shot)).then(|| {
            let angle = std::f32::consts::TAU * unit_float(hash3(seed, 8, 0));
            let length = 1.0 + 7.0 * unit_float(hash3(seed, 9, 0));
            let to = anchor + Vec3::new(angle.cos(), 0.0, angle.sin()) * length;
            let seen = match hash3(seed, 10, 0) % 3 {
                0 => u64::MAX,
                1 => u64::MAX << (hash3(seed, 11, 0) % 8),
                _ => u64::from(hash3(seed, 12, 0)) | (u64::from(hash3(seed, 13, 0)) << 32),
            };
            WorldLine {
                from: anchor,
                to,
                seen,
            }
        });
        let bound = bound(seed).with(
            Binding::LineLength,
            line.map_or(0.0, |line| line.from.distance(line.to)),
        );
        let request = SpawnRequest {
            effect,
            anchor,
            bound,
            line,
            delay: 0.0,
        };
        let handle = runtime.spawn(&self.catalog, &request, now);
        if kind == Some(EventKind::Line) {
            if let Some(mut line) = line {
                line.seen |= u64::from(hash3(seed, 14, 0)) << 3;
                self.again = Some(SpawnRequest {
                    line: Some(line),
                    ..request
                });
            }
        } else if handle.is_some() {
            self.moved = handle;
        }
    }

    /// Keeps the plan stream's held set.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "seconds of a short stream and indices below 64 fit f32"
    )]
    fn keep_held(&mut self, runtime: &mut EffectRuntime, now: f64) {
        let t = now as f32;
        for (index, slot) in self.held_areas.iter_mut().enumerate() {
            let Some(effect) = pick(&self.areas, index) else {
                break;
            };
            let seed = hash3(u32::try_from(index).unwrap_or(0), 0xA4EA, 0);
            let anchor = place(seed);
            let handle = *slot.get_or_insert_with(|| {
                runtime
                    .hold(
                        &self.catalog,
                        &request(effect, anchor, None, bound(seed)),
                        now,
                        false,
                    )
                    .unwrap_or(EffectHandle {
                        index: u32::MAX,
                        generation: 0,
                    })
            });
            let edge = anchor.x - 2.0 + (t * 0.7 + index as f32).sin() * 2.0;
            let clip = if index % 3 == 0 {
                super::runtime::NO_CLIP
            } else {
                [1.0, 0.0, edge, anchor.x + 3.0]
            };
            runtime.set_fog_clip(handle, clip);
        }
        for (index, slot) in self.held_links.iter_mut().enumerate() {
            let Some(effect) = pick(&self.links, index) else {
                break;
            };
            let seed = hash3(u32::try_from(index).unwrap_or(0), 0x714C, 0);
            let from = place(seed) + Vec3::new((t * 0.5).sin(), 0.0, 0.0);
            let to = from + Vec3::new(3.0, 0.0, 1.0 + (t * 0.3).cos());
            let line = WorldLine::whole(from, to);
            let bound = bound(seed).with(Binding::LinkLength, from.distance(to));
            match *slot {
                Some(handle) if runtime.update_held_line(handle, line, bound) => {}
                _ => {
                    *slot = runtime.hold(
                        &self.catalog,
                        &request(effect, from, Some(line), bound),
                        now,
                        false,
                    );
                }
            }
        }
        for (index, slot) in self.held_looks.iter_mut().enumerate() {
            let Some(effect) = pick(&self.looks, index) else {
                break;
            };
            let seed = hash3(u32::try_from(index).unwrap_or(0), 0x100C, 0);
            let anchor = place(seed) + Vec3::new(0.0, 0.0, (t * 0.4 + index as f32).sin());
            let due =
                (self.frame + u32::try_from(index).unwrap_or(0) * 7).is_multiple_of(RELEASE_EVERY);
            if let (true, Some(handle)) = (due, *slot) {
                runtime.release(handle, now);
                *slot = None;
            }
            match *slot {
                Some(handle) if runtime.update_held(handle, anchor, bound(seed)) => {}
                _ => {
                    *slot = runtime.hold(
                        &self.catalog,
                        &request(effect, anchor, None, bound(seed)),
                        now,
                        index % 5 == 0,
                    );
                }
            }
        }
        if let Some(handle) = self.moved {
            runtime.move_anchor(handle, place(self.frame) + Vec3::new(0.01 * t, 0.0, 0.0));
        }
    }
}

/// Entry `index` of `ids`, round-robin.
fn pick(ids: &[EffectId], index: usize) -> Option<EffectId> {
    (!ids.is_empty()).then(|| ids[index % ids.len()])
}

/// A spawn request.
const fn request(
    effect: EffectId,
    anchor: Vec3,
    line: Option<WorldLine>,
    bound: BoundValues,
) -> SpawnRequest {
    SpawnRequest {
        effect,
        anchor,
        bound,
        line,
        delay: 0.0,
    }
}

/// Runs `frames` frames of `kind` through a fresh runtime, calling `check` after every build.
pub fn run_stream(
    kind: StreamKind,
    frames: u32,
    check: impl FnMut(&EffectCatalog, &EffectRuntime, f64),
) {
    run_stream_with(kind, frames, |_| {}, check);
}

/// [`run_stream`] over the shipped vocabulary changed by `edit` first (a tighter budget).
pub fn run_stream_with(
    kind: StreamKind,
    frames: u32,
    edit: impl FnOnce(&mut EffectCatalog),
    mut check: impl FnMut(&EffectCatalog, &EffectRuntime, f64),
) {
    let mut stream = TestStream::new(kind);
    edit(&mut stream.catalog);
    let mut runtime = EffectRuntime::new(&stream.catalog);
    let camera = stream_camera();
    for _ in 0..frames {
        let now = stream.step(&mut runtime);
        runtime.build_frame(&stream.catalog, now, Some(&camera));
        check(&stream.catalog, &runtime, now);
    }
}
