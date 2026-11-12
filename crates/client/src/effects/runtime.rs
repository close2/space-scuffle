//! The live effects of a session and the instances they draw this frame (plan §5.6).
//!
//! # Responsibility
//! Owns [`EffectRuntime`]: the [`EffectInstancePool`], the [`ParticlePool`], the spawn
//! sequence and the retained [`EffectFrame`] (cards, decals and lights of this frame, the cards
//! sorted back to front). [`EffectRuntime::spawn`] starts an effect; [`EffectRuntime::build_frame`]
//! ages everything and rebuilds the frame from the effects' state.
//!
//! # How it fits
//! Layers keep no per-instance state: every frame each live effect's layers are evaluated from
//! its birth time, bound values and seed, so the picture is a pure function of the event
//! stream and `Time<Virtual>`. Particles are the exception (they integrate), held in their `SoA`
//! pool. The renderer uploads the frame; the light pool applies its lights.
//!
//! # Budget and level of detail
//! `Critical` layers are written first, then `Detail` layers, each pool up to its budget. A
//! detail card that finds no room is skipped, and the oldest `Minor` or `Ambient` effect's
//! detail is retired (it erodes out over 0.1 s). A class cap counts an effect only while its
//! fire (its non-decal layers) is alive; an effect spawned over its class cap keeps its detail
//! and the oldest burning effect of its class gives its detail up instead (Amendment 7).
//! Decals are reserved whole at birth ([`super::ledger`]): above the pool's pressure mark the
//! oldest lasting marks erode out over 0.25 s to make room, an effect that still finds no room
//! draws no decal at all, and a reserved decal is drawn every frame until it ends or erodes. Layers smaller than `min_screen_px` are skipped; bursts
//! scale with projected size and halve in a crowd of `Major` effects (`budget` functions).
//!
//! # Invariants
//! - All vectors are allocated at construction from the budget and only cleared and refilled,
//!   so a steady frame allocates nothing.
//! - Cards are sorted back to front by view depth in place (`sort_unstable_by` on a retained
//!   index list), once per frame.

use super::binding::{resolve_value, BoundValues};
use super::budget::{
    burst_count, crowd_factor, layer_admitted, lod_skips, particle_scale, projected_px,
    retire_fade, retirement_candidate, RetireCandidate,
};
use super::catalog::schema::{
    Binding, CardShape, DecalYaw, EffectClass, Orient, ParticleOrient, Priority, RingMapping,
    RotationDef,
};
use super::catalog::{
    Block, Breakup, CardLook, CurveId, DecalLook, DistortionLook, EffectCatalog, EffectId,
    ElementRef, Layer, Pool, RibbonLook,
};
use super::instances::{
    DecalInstance, LightInstance, SpriteInstance, FLAG_ARC, FLAG_MIRROR, FLAG_POLAR, NO_NOISE,
    ORIENT_CAMERA, ORIENT_GROUND, ORIENT_RIBBON, ORIENT_RIBBON_GROUND, ORIENT_STRETCHED,
    ORIENT_UPRIGHT, ORIENT_VERTICAL, SHAPE_FRONT, SHAPE_RADIAL, SHAPE_TEXTURE,
};
use super::ledger::{mark_fade, BudgetLedger, MARK_RETIRE_SECS};
use super::particles::{Burst, LineClip, ParticlePool};
use super::placement::{hash3, instance_count, place, placed_seen, unit_float, WorldLine};
use super::pool::{EffectHandle, EffectInstance, EffectInstancePool};
use bevy::math::{Vec2, Vec3};
use bevy::prelude::Resource;

/// Soft-step width of the erosion threshold of an element without breakup.
const DEFAULT_SOFTNESS: f32 = 0.08;

/// The camera the frame is built for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraView {
    /// Position in world space.
    pub position: Vec3,
    /// Unit view direction.
    pub forward: Vec3,
    /// Vertical field of view, radians.
    pub fov_y: f32,
    /// Viewport height in pixels.
    pub viewport_px: f32,
    /// Viewport width over height.
    pub aspect: f32,
}

impl CameraView {
    /// Depth of `point` along the view direction.
    #[must_use]
    pub fn depth(&self, point: Vec3) -> f32 {
        (point - self.position).dot(self.forward)
    }

    /// Projected height in pixels of a world length `size` at `point`.
    #[must_use]
    pub fn pixels(&self, size: f32, point: Vec3) -> f32 {
        projected_px(size, self.depth(point), self.fov_y, self.viewport_px)
    }

    /// Whether `point` is inside the view cone.
    #[must_use]
    pub fn sees(&self, point: Vec3) -> bool {
        let depth = self.depth(point);
        if depth <= 0.0 {
            return false;
        }
        let lateral = (point - self.position - self.forward * depth).length();
        let half_height = depth * (self.fov_y * 0.5).tan();
        lateral <= half_height * self.aspect.max(1.0) * 1.05
    }
}

/// Counters of what the budget did, for diagnostics and tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EffectStats {
    /// Effects spawned.
    pub spawned: u64,
    /// Effects refused because the instance pool was full.
    pub refused: u64,
    /// Critical instances that found their pool full (a sizing defect).
    pub critical_overflow: u64,
    /// Detail instances skipped for want of room.
    pub detail_skipped: u64,
    /// Instances skipped as too small on screen.
    pub lod_skipped: u64,
    /// Effects whose detail was retired.
    pub retired: u64,
    /// Line requests merged into a live effect of the same line (a line whose searches land
    /// on several ticks).
    pub merged_lines: u64,
    /// Lasting marks retired (eroded out) to make room in the decal pool.
    pub marks_retired: u64,
    /// Effects that found no room for their decals and draw none.
    pub marks_refused: u64,
}

/// What one frame draws.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectFrame {
    /// Cards in build order.
    cards: Vec<SpriteInstance>,
    /// Depth of each card along the view.
    depth: Vec<f32>,
    /// Retained sort order of `cards`.
    order: Vec<u32>,
    /// Cards back to front: what the renderer uploads.
    pub sorted: Vec<SpriteInstance>,
    /// Decals.
    pub decals: Vec<DecalInstance>,
    /// The pool slot of the effect each decal of `decals` belongs to (diagnostics and tests).
    pub decal_owners: Vec<u32>,
    /// Point lights.
    pub lights: Vec<LightInstance>,
    /// Distortion discs (drawn in the transmissive phase, unsorted: each only bends the
    /// opaque scene, so their order does not matter).
    pub distortions: Vec<SpriteInstance>,
    /// Cards written this frame (not particles).
    card_count: u32,
}

impl EffectFrame {
    /// A frame with room for `cards` cards (cards and particles), `decals`, `lights` and
    /// `distortions`.
    fn with_capacity(cards: usize, decals: usize, lights: usize, distortions: usize) -> Self {
        Self {
            cards: Vec::with_capacity(cards),
            depth: Vec::with_capacity(cards),
            order: Vec::with_capacity(cards),
            sorted: Vec::with_capacity(cards),
            decals: Vec::with_capacity(decals),
            decal_owners: Vec::with_capacity(decals),
            lights: Vec::with_capacity(lights),
            distortions: Vec::with_capacity(distortions),
            card_count: 0,
        }
    }

    /// Address and capacity of every retained vector, which an allocation would change.
    #[must_use]
    pub fn buffers(&self) -> [(usize, usize); 8] {
        [
            (self.cards.as_ptr() as usize, self.cards.capacity()),
            (self.depth.as_ptr() as usize, self.depth.capacity()),
            (self.order.as_ptr() as usize, self.order.capacity()),
            (self.sorted.as_ptr() as usize, self.sorted.capacity()),
            (self.decals.as_ptr() as usize, self.decals.capacity()),
            (self.lights.as_ptr() as usize, self.lights.capacity()),
            (
                self.distortions.as_ptr() as usize,
                self.distortions.capacity(),
            ),
            (
                self.decal_owners.as_ptr() as usize,
                self.decal_owners.capacity(),
            ),
        ]
    }
}

/// The session's live effects.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct EffectRuntime {
    /// Live effect instances.
    pub pool: EffectInstancePool,
    /// Live particles.
    pub particles: ParticlePool,
    /// Effects spawned since the runtime was made; seeds the layers' hash.
    sequence: u32,
    /// The frame being drawn.
    pub frame: EffectFrame,
    /// Budget counters.
    pub stats: EffectStats,
    /// Burning effects per class and decal reservations, recounted every frame.
    pub ledger: BudgetLedger,
    /// Time of the last build, seconds.
    last_build: Option<f64>,
    /// When set, only layers of this priority are drawn and fired (diagnostics: the effect
    /// lab and the GPU tests isolate the `Critical` layers to measure their footprint).
    pub only: Option<Priority>,
}

/// Where and how an effect is spawned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnRequest {
    /// The effect.
    pub effect: EffectId,
    /// Anchor in world space.
    pub anchor: Vec3,
    /// Values bound from the event.
    pub bound: BoundValues,
    /// The event's line, from the anchor onward.
    pub line: Option<WorldLine>,
    /// Extra delay before the effect starts, seconds (a projectile's flight).
    pub delay: f32,
}

impl EffectRuntime {
    /// An empty runtime sized by `catalog`'s budget.
    #[must_use]
    pub fn new(catalog: &EffectCatalog) -> Self {
        let budget = &catalog.budget;
        let cards = (budget.sprites + budget.particles) as usize;
        Self {
            pool: EffectInstancePool::with_capacity(budget.effects),
            particles: ParticlePool::with_capacity(budget.particles),
            sequence: 0,
            frame: EffectFrame::with_capacity(
                cards,
                budget.decals as usize,
                budget.lights as usize,
                budget.distortions as usize,
            ),
            stats: EffectStats::default(),
            ledger: BudgetLedger::default(),
            last_build: None,
            only: None,
        }
    }

    /// Spawns `request` at time `now` (seconds of `Time<Virtual>`). A request for a line
    /// that a live effect already draws (the same effect and the same far end) adds the
    /// search points it sees to that effect instead and returns `None`: a line whose searches
    /// land one after another on several ticks (the spine line, B-85) is one effect whose
    /// travel delays already place each search's instances at its time.
    pub fn spawn(
        &mut self,
        catalog: &EffectCatalog,
        request: &SpawnRequest,
        now: f64,
    ) -> Option<EffectHandle> {
        if let Some(line) = request.line {
            if self.merge_line(catalog, request.effect, line, now) {
                self.stats.merged_lines += 1;
                return None;
            }
        }
        let effect = catalog.effect(request.effect)?;
        let birth = now + f64::from(request.delay);
        let line_delay = max_line_delay(&effect.layers, request.line);
        let spans = effect_spans(effect, request);
        // A newborn effect is never the one stripped: over its class cap, the oldest burning
        // effect of its class gives its detail up (Amendment 7).
        let over_cap = self.ledger.burning(effect.class)
            >= catalog.budget.class_caps[effect.class.index()]
            && !self.retire_oldest_fire(effect.class, now);
        let burning = spans.fire > 0.0 && !over_cap;
        let decal_slots = self.reserve_marks(catalog, effect.class, burning, spans.decals, now);
        let instance = EffectInstance {
            effect: request.effect,
            class: effect.class,
            birth,
            end: birth + f64::from(effect.lifetime + line_delay),
            anchor: request.anchor,
            bound: request.bound,
            line: request.line,
            seed: instance_seed(self.sequence, effect.salt),
            detail_retired_at: over_cap.then_some(birth),
            has_detail: effect
                .layers
                .iter()
                .any(|layer| layer.priority == Priority::Detail),
            fire_end: birth + f64::from(spans.fire),
            decal_end: birth + f64::from(spans.decal),
            decal_slots,
            marks_retired_at: None,
            bursts_through: f32::NEG_INFINITY,
            held: false,
            released_at: None,
            held_exit: 0.0,
            oneshot_end: birth,
        };
        self.place(instance)
    }

    /// The held effect `handle`, if it is still live (held or eroding out after release).
    #[must_use]
    pub fn held(&self, handle: EffectHandle) -> Option<&EffectInstance> {
        self.pool.get(handle).filter(|instance| instance.held)
    }

    /// Places `instance` in the pool and advances the spawn sequence.
    fn place(&mut self, instance: EffectInstance) -> Option<EffectHandle> {
        self.sequence = self.sequence.wrapping_add(1);
        if let Ok(handle) = self.pool.spawn(instance) {
            self.stats.spawned += 1;
            Some(handle)
        } else {
            self.stats.refused += 1;
            None
        }
    }

    /// Holds `request`'s effect on a unit from time `now` until [`Self::release`]: its held
    /// layers come in and stay, its other layers play once (plan §5.4, WP8). With `settled`,
    /// the effect is shown as if it had been held long ago: its one-shot layers and bursts are
    /// over and its held layers are fully in (a unit first seen already in its stance).
    ///
    /// A held effect counts against no class cap and is never retired by the budget: it shows
    /// a unit's state, and there is at most one of each kind per unit. Its decals are reserved
    /// whole at once, as any effect's are.
    pub fn hold(
        &mut self,
        catalog: &EffectCatalog,
        request: &SpawnRequest,
        now: f64,
        settled: bool,
    ) -> Option<EffectHandle> {
        let effect = catalog.effect(request.effect)?;
        let birth = if settled {
            now - f64::from(effect.lifetime) - SETTLED_AGE
        } else {
            now
        };
        let spans = effect_spans(effect, request);
        let decal_slots = self.reserve_marks(catalog, effect.class, false, spans.decals, now);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "an effect's age in seconds fits f32"
        )]
        let bursts_through = if settled {
            (now - birth) as f32
        } else {
            f32::NEG_INFINITY
        };
        let instance = EffectInstance {
            effect: request.effect,
            class: effect.class,
            birth,
            end: f64::INFINITY,
            anchor: request.anchor,
            bound: request.bound,
            line: None,
            seed: instance_seed(self.sequence, effect.salt),
            detail_retired_at: None,
            has_detail: false,
            fire_end: birth + f64::from(spans.fire),
            decal_end: if spans.held_decals {
                f64::INFINITY
            } else {
                birth + f64::from(spans.decal)
            },
            decal_slots,
            marks_retired_at: None,
            bursts_through,
            held: true,
            released_at: None,
            held_exit: effect.held_exit,
            oneshot_end: birth + f64::from(effect.lifetime),
        };
        self.place(instance)
    }

    /// Moves the held effect `handle` to `anchor` (world space) with the values `bound`; a
    /// released or stale handle does nothing. Returns whether the effect is still held.
    pub fn update_held(&mut self, handle: EffectHandle, anchor: Vec3, bound: BoundValues) -> bool {
        match self.pool.get_mut(handle) {
            Some(instance) if instance.held && instance.released_at.is_none() => {
                instance.anchor = anchor;
                instance.bound = bound;
                true
            }
            _ => false,
        }
    }

    /// Releases the held effect `handle` at `now`: its held layers erode out over their exit
    /// and the effect ends once they and its one-shot layers are over. A stale or already
    /// released handle does nothing.
    pub fn release(&mut self, handle: EffectHandle, now: f64) {
        let Some(instance) = self.pool.get_mut(handle) else {
            return;
        };
        if !instance.held || instance.released_at.is_some() {
            return;
        }
        let exit_end = now + f64::from(instance.held_exit);
        instance.released_at = Some(now);
        instance.end = exit_end.max(instance.oneshot_end);
        if instance.decal_end.is_infinite() {
            instance.decal_end = exit_end.max(instance.oneshot_end);
        }
    }

    /// Adds `line`'s seen search points to the live effect `effect` drawing the same line, if
    /// there is one still laying the line out (younger than its longest travel delay plus
    /// [`LINE_MERGE_SLACK`]); returns whether there was. A later attack along the very same
    /// line (a unit firing again from where it stands) is a new effect.
    // PERFORMANCE RATIONALE:
    // Why: A line request must find the live effect of its line, if any; an index keyed by
    // line would be a second structure to keep in step with the pool's slots.
    // What: A linear scan over the pool's fixed slots, once per line request (one per line
    // per frame after `LineAssembly`), comparing two indices and a point.
    // Trade-off: O(capacity) per line request (2 048 slots, a few microseconds) in exchange
    // for no extra state and no allocation.
    fn merge_line(
        &mut self,
        catalog: &EffectCatalog,
        effect: EffectId,
        line: WorldLine,
        now: f64,
    ) -> bool {
        let Some(layers) = catalog.effect(effect).map(|effect| &effect.layers) else {
            return false;
        };
        for slot in 0..self.pool.capacity() as usize {
            let Some(instance) = self.pool.slot_mut(slot) else {
                continue;
            };
            let birth = instance.birth;
            let Some(live) = instance.line.as_mut() else {
                continue;
            };
            let window = f64::from(max_line_delay(layers, Some(*live)) + LINE_MERGE_SLACK);
            if instance.effect == effect
                && live.to.distance(line.to) <= SAME_LINE_END
                && now - birth <= window
            {
                live.seen |= line.seen;
                return true;
            }
        }
        false
    }

    /// Removes every effect and particle and restarts the spawn sequence, so the next effects
    /// draw the same random choices as the first ones did (the effect lab's takes).
    pub fn reset(&mut self) {
        self.clear();
        self.sequence = 0;
    }

    /// Removes every effect and particle.
    pub fn clear(&mut self) {
        self.pool.clear();
        self.particles.clear();
        self.frame.cards.clear();
        self.frame.sorted.clear();
        self.frame.decals.clear();
        self.frame.decal_owners.clear();
        self.frame.lights.clear();
        self.frame.distortions.clear();
        self.ledger.clear();
        self.last_build = None;
    }

    /// Ages every effect to `now` and rebuilds the frame for `camera` (`None` draws every
    /// layer at full detail and sorts nothing).
    pub fn build_frame(&mut self, catalog: &EffectCatalog, now: f64, camera: Option<&CameraView>) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a frame step in seconds fits f32"
        )]
        let dt = self
            .last_build
            .map_or(0.0, |last| (now - last).max(0.0) as f32);
        self.last_build = Some(now);
        self.pool.free_ended(now);
        self.settle_ledger(catalog, now);
        self.fire_bursts(catalog, now, camera);
        self.particles.step(dt, &catalog.emitters);
        self.spawn_landings(catalog, now);
        self.frame.cards.clear();
        self.frame.depth.clear();
        self.frame.decals.clear();
        self.frame.decal_owners.clear();
        self.frame.lights.clear();
        self.frame.distortions.clear();
        self.frame.card_count = 0;
        let overflowed_critical = self.write_layers(catalog, now, camera, Priority::Critical);
        let overflowed_detail = self.write_layers(catalog, now, camera, Priority::Detail);
        self.stats.critical_overflow += u64::from(overflowed_critical);
        self.stats.detail_skipped += u64::from(overflowed_detail);
        if overflowed_detail > 0 {
            self.retire_one(now);
        }
        self.write_particles(catalog, camera);
        self.sort_cards();
    }

    /// Spawns the `land` effect of every particle that landed in the last step, at its
    /// landing point.
    fn spawn_landings(&mut self, catalog: &EffectCatalog, now: f64) {
        for index in 0..self.particles.landings().len() {
            let landing = self.particles.landings()[index];
            let request = SpawnRequest {
                effect: landing.effect,
                anchor: landing.position,
                bound: BoundValues::default(),
                line: None,
                delay: 0.0,
            };
            self.spawn(catalog, &request, now);
        }
    }

    /// Gives back the decal slots of marks that ended or finished eroding out, and recounts
    /// the ledger from the live effects (once per frame). An effect holds the slots of its decal
    /// layers that have not ended yet: a short layer (embers) gives its slots back when it ends,
    /// a lasting one keeps them to its end.
    fn settle_ledger(&mut self, catalog: &EffectCatalog, now: f64) {
        self.ledger.clear();
        for slot in 0..self.pool.capacity() as usize {
            let Some(instance) = self.pool.slot_mut(slot) else {
                continue;
            };
            let eroded = instance
                .marks_retired_at
                .is_some_and(|retired| now - retired >= f64::from(MARK_RETIRE_SECS));
            if now > instance.decal_end || eroded {
                instance.decal_slots = 0;
            }
            let held = match catalog.effect(instance.effect) {
                Some(effect) if instance.decal_slots > 0 => held_marks(effect, instance, now),
                _ => 0,
            };
            let burning =
                !instance.held && now < instance.fire_end && instance.detail_retired_at.is_none();
            self.ledger.count(
                instance.class,
                burning,
                held,
                instance.marks_retired_at.is_some(),
            );
        }
    }

    /// Retires the detail of the oldest burning effect of `class`; returns whether there was
    /// one.
    // PERFORMANCE RATIONALE:
    // Why: Over a class cap the oldest burning effect must be found among the live ones; an
    // age-ordered index per class would be a second structure to keep in step with the pool.
    // What: A linear scan over the pool's fixed slots, only for a spawn over its class cap.
    // Trade-off: O(capacity) per such spawn (2 048 slots, a few microseconds) in exchange for
    // no extra state and no allocation.
    fn retire_oldest_fire(&mut self, class: EffectClass, now: f64) -> bool {
        let oldest = self
            .pool
            .iter()
            .filter(|(_, instance)| {
                instance.class == class
                    && !instance.held
                    && instance.has_detail
                    && instance.detail_retired_at.is_none()
                    && now < instance.fire_end
            })
            .min_by(|(_, a), (_, b)| a.birth.total_cmp(&b.birth))
            .map(|(index, _)| index);
        let Some(instance) = oldest.and_then(|index| self.pool.slot_mut(index)) else {
            return false;
        };
        instance.detail_retired_at = Some(now);
        self.ledger.quench(class);
        self.stats.retired += 1;
        true
    }

    /// Reserves `need` decal slots for a new effect of `class` (burning when `burning`):
    /// first retiring the oldest lasting marks while the pool is above its pressure mark, then
    /// granting all of them or none. Returns the slots granted.
    fn reserve_marks(
        &mut self,
        catalog: &EffectCatalog,
        class: EffectClass,
        burning: bool,
        need: u32,
        now: f64,
    ) -> u32 {
        let capacity = catalog.budget.decals;
        while need > 0 && self.ledger.needs_room(need, capacity) {
            if !self.retire_oldest_mark(now) {
                break;
            }
        }
        let granted = self.ledger.grant(class, burning, need, capacity);
        if need > 0 && granted == 0 {
            self.stats.marks_refused += 1;
        }
        granted
    }

    /// Retires the oldest lasting marks still holding decal slots: they erode out over
    /// [`MARK_RETIRE_SECS`] and then give their slots back. Returns whether there were any.
    // PERFORMANCE RATIONALE:
    // Why: Making room needs the oldest effect holding decal slots; an age-ordered index
    // would be a second structure to keep in step with the pool.
    // What: A linear scan over the pool's fixed slots, only while the decal pool is above its
    // pressure mark.
    // Trade-off: O(capacity) per retirement (2 048 slots, a few microseconds), rare below the
    // stress load, in exchange for no extra state and no allocation.
    fn retire_oldest_mark(&mut self, now: f64) -> bool {
        let oldest = self
            .pool
            .iter()
            .filter(|(_, instance)| {
                instance.decal_slots > 0 && instance.marks_retired_at.is_none() && !instance.held
            })
            .min_by(|(_, a), (_, b)| a.birth.total_cmp(&b.birth))
            .map(|(index, _)| index);
        let Some(instance) = oldest.and_then(|index| self.pool.slot_mut(index)) else {
            return false;
        };
        instance.marks_retired_at = Some(now);
        self.ledger.retire_marks(instance.decal_slots);
        self.stats.marks_retired += 1;
        true
    }

    /// Retires the detail of the oldest `Minor` or `Ambient` effect still drawing it.
    fn retire_one(&mut self, now: f64) {
        let candidate = retirement_candidate(self.pool.iter().map(|(index, instance)| {
            (
                index,
                RetireCandidate {
                    class: instance.class,
                    birth: instance.birth,
                    detail_retired: instance.detail_retired_at.is_some(),
                    has_detail: instance.has_detail && !instance.held,
                },
            )
        }));
        if let Some(instance) = candidate.and_then(|index| self.pool.slot_mut(index)) {
            instance.detail_retired_at = Some(now);
            self.stats.retired += 1;
        }
    }

    /// Spawns every particle burst that fell due since the previous build: each placed
    /// instance of a particle layer is due at the layer's start plus its placement delay.
    fn fire_bursts(&mut self, catalog: &EffectCatalog, now: f64, camera: Option<&CameraView>) {
        let crowd = camera.map_or(1.0, |camera| {
            let majors = self
                .pool
                .iter()
                .filter(|(_, instance)| {
                    instance.class == EffectClass::Major && camera.sees(instance.anchor)
                })
                .count();
            crowd_factor(
                u32::try_from(majors).unwrap_or(u32::MAX),
                catalog.budget.crowd_threshold,
            )
        });
        for slot in 0..self.pool.capacity() as usize {
            let Some(instance) = self.pool.slot_mut(slot) else {
                continue;
            };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "an effect's age in seconds fits f32"
            )]
            let age = (now - instance.birth) as f32;
            let through = instance.bursts_through;
            if age < 0.0 || age <= through {
                continue;
            }
            instance.bursts_through = age;
            let instance = *instance;
            let Some(effect) = catalog.effect(instance.effect) else {
                continue;
            };
            for (index, layer) in effect.layers.iter().enumerate() {
                let shown = self.only.is_none_or(|only| only == layer.priority);
                if matches!(layer.block, Block::Particles(_)) && layer.start <= age && shown {
                    let window = (through, age);
                    self.burst(catalog, &instance, layer, index, crowd, camera, window);
                }
            }
        }
    }

    /// Spawns the bursts of one particle layer's placed instances that fall due in
    /// `window` (exclusive start, inclusive end, seconds of the effect's age).
    #[expect(
        clippy::too_many_arguments,
        reason = "the burst's effect, layer, level-of-detail factors and time window"
    )]
    fn burst(
        &mut self,
        catalog: &EffectCatalog,
        instance: &EffectInstance,
        layer: &Layer,
        layer_index: usize,
        crowd: f32,
        camera: Option<&CameraView>,
        window: (f32, f32),
    ) {
        let Block::Particles(emitter_id) = layer.block else {
            return;
        };
        let Some(emitter) = catalog.emitters.get(usize::from(emitter_id.0)) else {
            return;
        };
        if layer.priority == Priority::Detail && instance.detail_retired_at.is_some() {
            return;
        }
        let radius = resolve_value(&layer.size, &instance.bound);
        let size_scale = camera.map_or(1.0, |camera| {
            particle_scale(
                camera.pixels(radius * 2.0, instance.anchor),
                catalog.budget.full_particle_px,
            )
        });
        let crowd = if layer.priority == Priority::Detail {
            crowd
        } else {
            1.0
        };
        let count = burst_count(&emitter.count, radius, size_scale * crowd);
        let layer_salt = u32::try_from(layer_index).unwrap_or(0);
        for (placement_index, placement) in layer.placements.iter().enumerate() {
            let placement_salt = u32::try_from(placement_index).unwrap_or(0);
            for index in 0..instance_count(placement, &instance.bound, instance.line) {
                if !placed_seen(placement, index, instance.line, &instance.bound) {
                    continue;
                }
                let seed = hash3(instance.seed, layer_salt * 64 + placement_salt, index);
                let placed = place(placement, index, seed, &instance.bound, instance.line);
                let due = layer.start + placed.delay;
                if due <= window.0 || due > window.1 {
                    continue;
                }
                let clip = clip_window(
                    instance.line,
                    layer.clip_to_line,
                    &instance.bound,
                    instance.anchor + placed.offset,
                );
                let burst = Burst {
                    emitter: emitter_id,
                    origin: instance.anchor + placed.offset,
                    radius,
                    count,
                    seed,
                    clip,
                };
                self.particles.spawn(&burst, emitter);
            }
        }
    }

    /// Writes every live effect's layers of `priority`; returns how many instances found
    /// their pool full.
    fn write_layers(
        &mut self,
        catalog: &EffectCatalog,
        now: f64,
        camera: Option<&CameraView>,
        priority: Priority,
    ) -> u32 {
        let mut overflow = 0;
        let budget = &catalog.budget;
        for slot in 0..self.pool.capacity() as usize {
            let Some(instance) = self.pool.slot_mut(slot).copied() else {
                continue;
            };
            let Some(effect) = catalog.effect(instance.effect) else {
                continue;
            };
            let fade = match (priority, instance.detail_retired_at) {
                (Priority::Detail, Some(retired)) => {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "seconds since retirement fit f32"
                    )]
                    let since = (now - retired) as f32;
                    retire_fade(since)
                }
                _ => 1.0,
            };
            if fade <= 0.0 {
                continue;
            }
            for (layer_index, layer) in effect.layers.iter().enumerate() {
                if layer.priority != priority || self.only.is_some_and(|only| only != priority) {
                    continue;
                }
                let free = match layer.block.pool() {
                    Pool::Sprites => budget.sprites.saturating_sub(self.frame.card_count),
                    Pool::Decals => budget.decals.saturating_sub(len32(self.frame.decals.len())),
                    Pool::Lights => budget.lights.saturating_sub(len32(self.frame.lights.len())),
                    Pool::Distortions => budget
                        .distortions
                        .saturating_sub(len32(self.frame.distortions.len())),
                    Pool::Particles | Pool::Bubble => continue,
                };
                let Some(fade) = marks_fade(&instance, layer, fade, now) else {
                    continue;
                };
                let context = LayerContext {
                    catalog,
                    instance: &instance,
                    slot: u32::try_from(slot).unwrap_or(u32::MAX),
                    layer,
                    layer_index,
                    now,
                    fade,
                    camera,
                    exit: layer_clock(layer, &instance, now, 0.0).map_or(0.0, |clock| clock.exit),
                };
                overflow += self.write_layer(&context, free);
            }
        }
        overflow
    }

    /// Writes one layer's placed instances while `free` lasts; returns those that found no
    /// room (critical ones are written past the soft cap up to the hard capacity).
    fn write_layer(&mut self, context: &LayerContext<'_>, mut free: u32) -> u32 {
        let mut overflow = 0;
        let layer = context.layer;
        let instance = context.instance;
        for (placement_index, placement) in layer.placements.iter().enumerate() {
            let layer_index = u32::try_from(context.layer_index).unwrap_or(0);
            let placement_index = u32::try_from(placement_index).unwrap_or(0);
            for index in 0..instance_count(placement, &instance.bound, instance.line) {
                if !placed_seen(placement, index, instance.line, &instance.bound) {
                    continue;
                }
                let seed = placed_seed(instance.seed, layer_index, placement_index, index);
                let placed = place(placement, index, seed, &instance.bound, instance.line);
                let Some(clock) = layer_clock(layer, instance, context.now, placed.delay) else {
                    continue;
                };
                if !layer_admitted(layer.priority, free) {
                    overflow += 1;
                    continue;
                }
                let position = instance.anchor + placed.offset;
                match self.write_instance(context, position, clock, seed) {
                    Written::Yes => free = free.saturating_sub(1),
                    Written::Full => overflow += 1,
                    Written::Skipped => {}
                }
            }
        }
        overflow
    }

    /// Writes one placed instance of a layer.
    fn write_instance(
        &mut self,
        context: &LayerContext<'_>,
        position: Vec3,
        clock: LayerClock,
        seed: u32,
    ) -> Written {
        let layer = context.layer;
        let catalog = context.catalog;
        let LayerClock { age, life, exit } = clock;
        // A released held layer erodes out (its erosion rises with `exit`, see `push_card` and
        // `push_decal`) and dims with it, so it never pops.
        let intensity = catalog.sample(layer.curve, life, 1.0) * context.fade * (1.0 - exit * exit);
        let jitter = 1.0 + layer.size_jitter * (2.0 * unit_float(hash3(seed, 0x51CE, 0)) - 1.0);
        let size = resolve_value(&layer.size, &context.instance.bound)
            * catalog.sample(layer.size_curve, life, 1.0)
            * jitter;
        let extent = match &layer.block {
            Block::Ring { .. } => layer.radius.map_or(size, |radius| {
                let from = resolve_value(&radius.from, &context.instance.bound);
                let to = resolve_value(&radius.to, &context.instance.bound);
                from + (to - from) * catalog.sample(radius.curve, life, life)
            }),
            _ => size,
        };
        // A ribbon is as large on screen as its line is long, however thin it is.
        let lod_extent = match (&layer.block, context.instance.line) {
            (Block::Ribbon(_), Some(line)) => extent.max(0.5 * line.from.distance(line.to)),
            _ => extent,
        };
        if let Some(camera) = context.camera {
            if lod_skips(
                camera.pixels(lod_extent * 2.0, position),
                context.catalog.budget.min_screen_px,
            ) {
                self.stats.lod_skipped += 1;
                return Written::Skipped;
            }
        }
        match &layer.block {
            Block::Sprite(look) => self.push_card(
                context,
                look,
                position,
                extent,
                age,
                life,
                intensity,
                seed,
                RingShape::NONE,
            ),
            Block::Ring {
                look,
                thickness,
                height,
                edge_noise,
                arc,
                waver,
            } => self.push_card(
                context,
                look,
                position + Vec3::Y * *height,
                extent,
                age,
                life,
                intensity,
                seed,
                RingShape {
                    inner: 1.0
                        - (thickness * catalog.sample(layer.size_curve, life, 1.0))
                            .clamp(0.01, 0.99),
                    edge_noise: *edge_noise,
                    arc: arc
                        .map(|arc| resolve_value(&arc, &context.instance.bound).clamp(0.0, 1.0)),
                    waver: *waver,
                },
            ),
            Block::Decal(look) => {
                self.push_decal(context, look, position, extent, life, intensity, seed)
            }
            Block::Ribbon(look) => self.push_ribbon(context, look, extent, age, life, intensity),
            Block::Distortion(look) => {
                self.push_distortion(look, position, extent, age, intensity, seed)
            }
            Block::Light {
                color,
                intensity: peak,
                height,
            } => {
                if self.frame.lights.len() == self.frame.lights.capacity() {
                    return Written::Full;
                }
                self.frame.lights.push(LightInstance {
                    position: (position + Vec3::Y * *height).to_array(),
                    color: color.to_array(),
                    intensity: peak * intensity,
                    range: extent,
                });
                Written::Yes
            }
            Block::Particles(_) | Block::Shell(_) => Written::Skipped,
        }
    }

    /// Pushes a sprite or ring card.
    #[expect(
        clippy::too_many_arguments,
        reason = "the evaluated values of one card, each a separate input of its record"
    )]
    fn push_card(
        &mut self,
        context: &LayerContext<'_>,
        look: &CardLook,
        position: Vec3,
        extent: f32,
        age: f32,
        life: f32,
        intensity: f32,
        seed: u32,
        ring: RingShape,
    ) -> Written {
        let ring_inner = ring.inner;
        if self.frame.cards.len() == self.frame.cards.capacity() {
            return Written::Full;
        }
        let catalog = context.catalog;
        let (layer_a, layer_b, frame_mix) = card_frame(catalog, look, age, life, seed);
        let (rotation, mut flags) = roll_and_mirror(look.rotation, seed);
        if ring_inner > 0.0 && look.mapping == RingMapping::Polar {
            flags |= FLAG_POLAR;
        }
        let line = context.instance.line;
        let line_axis = line.map_or(Vec3::ZERO, |line| (line.to - line.from).normalize_or_zero());
        let standing = matches!(look.orient, Orient::VerticalAlongLine | Orient::Upright);
        // A standing card clipped to its line stands whole inside it (review round 5).
        let (position, extent) = match line {
            Some(line) if standing && look.clip_to_line => fit_inside_line(line, position, extent),
            _ => (position, extent),
        };
        let lift = match look.lift_curve {
            Some(curve) => catalog.sample(Some(curve), life, 0.0),
            None => look.lift.unwrap_or(if standing { 1.0 } else { 0.0 }),
        };
        let lean = look.lean * (2.0 * unit_float(hash3(seed, 0x1EA4, 0)) - 1.0);
        let outward = spread_offset(look.spread, position - context.instance.anchor, life, seed);
        let center = position + Vec3::Y * extent * lift + outward + look.drift * age;
        let (orient, axis) = match look.orient {
            Orient::Ground => (ORIENT_GROUND, Vec3::ZERO),
            Orient::Upright => (ORIENT_UPRIGHT, Vec3::ZERO),
            Orient::VerticalAlongLine if line_axis != Vec3::ZERO => (ORIENT_VERTICAL, line_axis),
            Orient::Camera | Orient::VerticalAlongLine => (ORIENT_CAMERA, Vec3::ZERO),
        };
        let shape = match (look.shape, look.mapping) {
            (CardShape::Radial, _) => SHAPE_RADIAL,
            (_, RingMapping::Front) if ring_inner > 0.0 => SHAPE_FRONT,
            _ => SHAPE_TEXTURE,
        };
        // A standing card (a clump of spikes) is drawn only at a seen point and kept whole: cut
        // at the fog's edge it would read as half a clump (review round 4, fog capture). Only
        // the line's own ends cut it; ground-hugging cards end where the fog begins.
        let clip = match line {
            Some(line) if standing && look.clip_to_line => line_clip_window(line),
            _ => clip_window(line, look.clip_to_line, &context.instance.bound, position),
        };
        let surface = Surface::of(look.breakup.as_ref(), seed);
        let card = SpriteInstance {
            center: center.to_array(),
            half_size: extent,
            axis: axis.to_array(),
            stretch: if standing { lift } else { 1.0 },
            layer_a,
            layer_b,
            frame_mix,
            ramp_row: u32::from(look.ramp.0),
            intensity,
            erosion: catalog.sample(look.erosion, life, 0.0).max(context.exit),
            soft_depth: look.soft_depth,
            ring_inner,
            rotation: rotation + lean,
            orient,
            ramp_scale: catalog.sample(context.layer.ramp_curve, life, 1.0),
            flags: if ring.arc.is_some() {
                flags | FLAG_ARC
            } else {
                flags
            },
            arc: ring.arc.unwrap_or(0.0),
            warp: ring.waver,
            clip_axis: [clip[0], clip[1]],
            clip_min: clip[2],
            clip_max: clip[3],
            shape,
            front_edge: if shape == SHAPE_FRONT {
                ring.edge_noise
            } else {
                0.0
            },
            front_seed: seed,
            ..surface.card()
        };
        self.push_sorted_card(card, context.camera, look.sort_bias);
        self.frame.card_count += 1;
        Written::Yes
    }

    /// Pushes a distortion disc of radius `extent` at `position`.
    fn push_distortion(
        &mut self,
        look: &DistortionLook,
        position: Vec3,
        extent: f32,
        age: f32,
        intensity: f32,
        seed: u32,
    ) -> Written {
        if self.frame.distortions.len() == self.frame.distortions.capacity() {
            return Written::Full;
        }
        let phase = Vec2::new(
            unit_float(hash3(seed, 0xD157_0001, 0)),
            unit_float(hash3(seed, 0xD157_0002, 0)),
        ) + look.scroll * age;
        self.frame.distortions.push(SpriteInstance {
            center: (position + Vec3::Y * look.height).to_array(),
            half_size: extent,
            intensity,
            orient: if look.ground {
                ORIENT_GROUND
            } else {
                ORIENT_CAMERA
            },
            noise_offset: phase.to_array(),
            noise_scale: look.scale,
            noise_amount: look.strength,
            noise_layer: look.noise_layer,
            warp: look.lens,
            ..SpriteInstance::default()
        });
        Written::Yes
    }

    /// Pushes a ribbon of half width `extent` along the effect's line: the span its head and
    /// tail have reached at `age`, clipped to the run of search points the viewer sees around
    /// the first seen one (plan Amendment 2, R6).
    fn push_ribbon(
        &mut self,
        context: &LayerContext<'_>,
        look: &RibbonLook,
        extent: f32,
        age: f32,
        life: f32,
        intensity: f32,
    ) -> Written {
        let Some(line) = context.instance.line else {
            return Written::Skipped;
        };
        if self.frame.cards.len() == self.frame.cards.capacity() {
            return Written::Full;
        }
        let length = line.from.distance(line.to);
        let direction = (line.to - line.from).normalize_or_zero();
        let (tail, head) = ribbon_span(look, length, age);
        if head - tail <= RIBBON_MIN_LENGTH || direction == Vec3::ZERO || extent <= 0.0 {
            return Written::Skipped;
        }
        let spacing = context.instance.bound.get(Binding::LineSpacing);
        let Some(first_seen) = (0..64).find(|point| line.sees_point(*point)) else {
            return Written::Skipped;
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "a point index below 64 is exact in f32"
        )]
        let seen_at = line.from + direction * spacing * first_seen as f32;
        // The strip itself ends at the line's ends (its tapers round them); only a line the
        // viewer sees in part is cut, where the fog begins.
        let clip = if line.seen == u64::MAX {
            NO_CLIP
        } else {
            seen_clip_window(line, spacing, Vec2::new(seen_at.x, seen_at.z))
        };
        let catalog = context.catalog;
        let (layer_a, layer_b, frame_mix) = variant_frame(&look.element, context.instance.seed)
            .unwrap_or_else(|| flipbook_frame(&look.element, None, age, life, true));
        let drawn = head - tail;
        let center = line.from + direction * f32::midpoint(tail, head) + Vec3::Y * look.height;
        let card = SpriteInstance {
            center: center.to_array(),
            half_size: extent,
            axis: direction.to_array(),
            stretch: 0.5 * drawn / extent,
            layer_a,
            layer_b,
            frame_mix,
            ramp_row: u32::from(look.ramp.0),
            intensity,
            erosion: catalog.sample(look.erosion, life, 0.0),
            soft_depth: look.soft_depth,
            orient: if look.ground {
                ORIENT_RIBBON_GROUND
            } else {
                ORIENT_RIBBON
            },
            ramp_scale: catalog.sample(context.layer.ramp_curve, life, 1.0),
            clip_axis: [clip[0], clip[1]],
            clip_min: clip[2],
            clip_max: clip[3],
            shape: match look.shape {
                CardShape::Radial => SHAPE_RADIAL,
                CardShape::Texture => SHAPE_TEXTURE,
            },
            strip_repeats: drawn / look.repeat,
            strip_offset: tail / look.repeat - look.scroll * age,
            taper_tail: look.taper.0,
            taper_head: look.taper.1,
            ..Surface::of(look.breakup.as_ref(), context.instance.seed).card()
        };
        self.push_sorted_card(card, context.camera, look.sort_bias);
        self.frame.card_count += 1;
        Written::Yes
    }

    /// Appends a card and its depth, moved `bias` world units towards the camera in the sort.
    fn push_sorted_card(&mut self, card: SpriteInstance, camera: Option<&CameraView>, bias: f32) {
        let depth = camera.map_or(0.0, |camera| {
            camera.depth(Vec3::from_array(card.center)) - bias
        });
        self.frame.cards.push(card);
        self.frame.depth.push(depth);
    }

    /// Pushes a decal box.
    #[expect(
        clippy::too_many_arguments,
        reason = "the evaluated values of one decal, each a separate input of its record"
    )]
    fn push_decal(
        &mut self,
        context: &LayerContext<'_>,
        look: &DecalLook,
        position: Vec3,
        extent: f32,
        life: f32,
        intensity: f32,
        seed: u32,
    ) -> Written {
        if self.frame.decals.len() == self.frame.decals.capacity() {
            return Written::Full;
        }
        let catalog = context.catalog;
        let (layer_a, layer_b, frame_mix) = variant_frame(&look.element, seed)
            .unwrap_or_else(|| flipbook_frame(&look.element, None, 0.0, life, true));
        let random_yaw = std::f32::consts::TAU * unit_float(hash3(seed, 0x0DEC, 0));
        let line = context.instance.line;
        let clip = clip_window(line, look.clip_to_line, &context.instance.bound, position);
        let yaw = match (look.yaw, line) {
            (DecalYaw::AlongLine, Some(line)) => {
                let direction = line.to - line.from;
                if direction.x.abs() + direction.z.abs() > f32::EPSILON {
                    (-direction.z).atan2(direction.x)
                } else {
                    random_yaw
                }
            }
            _ => random_yaw,
        };
        self.frame.decal_owners.push(context.slot);
        self.frame.decals.push(DecalInstance {
            center: position.to_array(),
            yaw,
            half_extents: [extent, look.depth, extent * look.width],
            grazing: look.grazing,
            layer_a,
            layer_b,
            frame_mix,
            ramp_row: u32::from(look.ramp.0),
            intensity,
            erosion: catalog.sample(look.erosion, life, 0.0).max(context.exit),
            ramp_scale: catalog.sample(context.layer.ramp_curve, life, 1.0),
            clip_min: clip[2],
            clip_max: clip[3],
            clip_axis: [clip[0], clip[1]],
            clip_feather: look.clip_feather,
            relief: look.liquid.map_or(0.0, |liquid| liquid.relief),
            sheen: look.liquid.map_or(0.0, |liquid| liquid.sheen),
            gloss: look.liquid.map_or(1.0, |liquid| liquid.gloss),
            value_noise: look.liquid.map_or(0.0, |liquid| liquid.value_noise),
            dry_below: look.liquid.map_or(0.0, |liquid| liquid.dry_below),
            outline_noise: look.outline.map_or(0.0, |outline| outline.edge_noise),
            outline_fill: look.outline.map_or(0.0, |outline| outline.fill),
            ..Surface::of(look.breakup.as_ref(), seed).decal()
        });
        Written::Yes
    }

    /// Writes every live particle as a card.
    fn write_particles(&mut self, catalog: &EffectCatalog, camera: Option<&CameraView>) {
        let Self {
            particles, frame, ..
        } = self;
        for particle in particles.iter() {
            if frame.cards.len() == frame.cards.capacity() {
                break;
            }
            let Some(emitter) = catalog.emitters.get(usize::from(particle.emitter.0)) else {
                continue;
            };
            let life = particle.life_fraction;
            let (orient, axis, stretch) = match emitter.orient {
                ParticleOrient::Camera => (ORIENT_CAMERA, Vec3::ZERO, 1.0),
                ParticleOrient::Ground => (ORIENT_GROUND, Vec3::ZERO, 1.0),
                ParticleOrient::VelocityStretched(factor) => {
                    let speed = particle.velocity.length();
                    (
                        ORIENT_STRETCHED,
                        particle.velocity.normalize_or_zero(),
                        1.0 + factor * speed * 0.1,
                    )
                }
            };
            let (layer_a, layer_b, frame_mix) = variant_frame(&emitter.element, particle.seed)
                .unwrap_or_else(|| flipbook_frame(&emitter.element, None, 0.0, life, true));
            let (roll, flags) = roll_and_mirror(emitter.rotation, particle.seed);
            let card = SpriteInstance {
                center: particle.position.to_array(),
                half_size: particle.size * catalog.sample(emitter.size_curve, life, 1.0),
                axis: axis.to_array(),
                stretch,
                layer_a,
                layer_b,
                frame_mix,
                ramp_row: u32::from(emitter.ramp.0),
                intensity: catalog.sample(emitter.curve, life, 1.0),
                erosion: 0.0,
                soft_depth: 0.0,
                ring_inner: 0.0,
                rotation: emitter.roll + roll,
                orient,
                ramp_scale: 1.0 - life,
                flags,
                clip_axis: [particle.clip[0], particle.clip[1]],
                clip_min: particle.clip[2],
                clip_max: particle.clip[3],
                ..Surface::NONE.card()
            };
            let depth = camera.map_or(0.0, |camera| camera.depth(particle.position));
            frame.cards.push(card);
            frame.depth.push(depth);
        }
    }

    /// Sorts the cards back to front into `sorted`.
    fn sort_cards(&mut self) {
        let frame = &mut self.frame;
        frame.order.clear();
        frame.order.extend(0..len32(frame.cards.len()));
        let depth = &frame.depth;
        frame
            .order
            .sort_unstable_by(|a, b| depth[*b as usize].total_cmp(&depth[*a as usize]));
        frame.sorted.clear();
        let cards = &frame.cards;
        frame
            .sorted
            .extend(frame.order.iter().map(|index| cards[*index as usize]));
    }
}

/// The ring mask of a card: none for a sprite.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RingShape {
    /// Inner radius as a fraction of the card's half size; 0 for a plain card.
    inner: f32,
    /// Depth of a `Front` ring's leading-edge noise, as a fraction of its band's width.
    edge_noise: f32,
    /// Share of the turn an arc draws, in [0, 1]; `None` for the whole ring.
    arc: Option<f32>,
    /// How much the band wavers round the turn; 0 for an even band.
    waver: f32,
}

impl RingShape {
    /// A plain card.
    const NONE: Self = Self {
        inner: 0.0,
        edge_noise: 0.0,
        arc: None,
        waver: 0.0,
    };
}

/// Outcome of writing one instance.
enum Written {
    /// Written.
    Yes,
    /// Its pool was full.
    Full,
    /// Skipped by level of detail.
    Skipped,
}

/// What evaluating one layer of one effect needs.
struct LayerContext<'a> {
    /// The vocabulary.
    catalog: &'a EffectCatalog,
    /// The live effect.
    instance: &'a EffectInstance,
    /// Its pool slot.
    slot: u32,
    /// The layer.
    layer: &'a Layer,
    /// Its index in the effect.
    layer_index: usize,
    /// Time now, seconds.
    now: f64,
    /// Retirement fade.
    fade: f32,
    /// The camera.
    camera: Option<&'a CameraView>,
    /// Share of a released held layer's exit gone (0 otherwise): its erosion is at least this.
    exit: f32,
}

/// The fade of `layer` of `instance` given its retirement fade `fade`: a decal layer draws
/// only while its effect holds its decal slots (`None` otherwise: refused room, or ended), and
/// erodes out with them once they are retired to make room.
fn marks_fade(instance: &EffectInstance, layer: &Layer, fade: f32, now: f64) -> Option<f32> {
    if layer.block.pool() != Pool::Decals {
        return Some(fade);
    }
    if instance.decal_slots == 0 {
        return None;
    }
    Some(instance.marks_retired_at.map_or(fade, |retired| {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "seconds since retirement fit f32"
        )]
        let since = (now - retired) as f32;
        fade * mark_fade(since)
    }))
}

/// The decal slots `instance` of `effect` holds at `now`: those of its decal layers that have
/// not ended yet.
fn held_marks(effect: &super::catalog::Effect, instance: &EffectInstance, now: f64) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an effect's age in seconds fits f32"
    )]
    let age = (now - instance.birth) as f32;
    effect
        .layers
        .iter()
        .filter(|layer| layer.block.pool() == Pool::Decals)
        .filter(|layer| {
            if layer.held && instance.held {
                return instance
                    .released_at
                    .is_none_or(|released| now - released <= f64::from(layer.exit));
            }
            age <= layer.start
                + layer.duration
                + max_line_delay(std::slice::from_ref(*layer), instance.line)
        })
        .map(|layer| {
            layer
                .placements
                .iter()
                .map(|placement| instance_count(placement, &instance.bound, instance.line))
                .sum::<u32>()
        })
        .sum()
}

/// How long an effect's fire and its decals last, in seconds from its birth, and how many
/// decals its decal layers place in all.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Spans {
    /// The end of its last non-decal layer.
    fire: f32,
    /// The end of its last decal layer.
    decal: f32,
    /// Decals its decal layers place over its life.
    decals: u32,
    /// Whether a held layer is a decal (its marks last until the effect is released).
    held_decals: bool,
}

/// The spans of `effect` spawned by `request`. Held layers add their decals but no span: they
/// last until their effect is released.
fn effect_spans(effect: &super::catalog::Effect, request: &SpawnRequest) -> Spans {
    let mut spans = Spans {
        fire: 0.0,
        decal: 0.0,
        decals: 0,
        held_decals: false,
    };
    for layer in &effect.layers {
        let end = layer.start
            + layer.duration
            + max_line_delay(std::slice::from_ref(layer), request.line);
        match layer.block.pool() {
            Pool::Decals => {
                spans.decals += layer
                    .placements
                    .iter()
                    .map(|placement| instance_count(placement, &request.bound, request.line))
                    .sum::<u32>();
                if layer.held {
                    spans.held_decals = true;
                } else {
                    spans.decal = spans.decal.max(end);
                }
            }
            Pool::Bubble => {}
            _ if layer.held => {}
            _ => spans.fire = spans.fire.max(end),
        }
    }
    spans
}

/// How far a held effect is shown as born in the past when it is held `settled`, beyond its
/// one-shot lifetime, seconds: every one-shot layer and burst is over, every held layer in.
const SETTLED_AGE: f64 = 60.0;

/// When one layer of one live effect is drawn, for a placed instance delayed by `delay`
/// seconds: its age and life, and how far a released held layer has eroded out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerClock {
    /// Seconds since the layer started.
    pub age: f32,
    /// Share of the layer's duration gone, in [0, 1]; a held layer stays at 1 once in.
    pub life: f32,
    /// Share of a released held layer's exit gone, in [0, 1); 0 while held or not held.
    pub exit: f32,
}

/// The [`LayerClock`] of `layer` of `instance` at `now` for an instance placed `delay` seconds
/// late, or `None` when it is not drawn: not started, past its duration (a layer that is not
/// held), or eroded out (a released held layer).
#[must_use]
pub fn layer_clock(
    layer: &Layer,
    instance: &EffectInstance,
    now: f64,
    delay: f32,
) -> Option<LayerClock> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an effect's age in seconds fits f32"
    )]
    let age = (now - instance.birth) as f32 - layer.start - delay;
    if age < 0.0 {
        return None;
    }
    let held = layer.held && instance.held;
    if !held {
        if age > layer.duration {
            return None;
        }
        return Some(LayerClock {
            age,
            life: (age / layer.duration).clamp(0.0, 1.0),
            exit: 0.0,
        });
    }
    let exit = match instance.released_at {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "seconds since release fit f32"
        )]
        Some(released) => ((now - released) as f32 / layer.exit.max(1e-3)).max(0.0),
        None => 0.0,
    };
    if exit >= 1.0 {
        return None;
    }
    Some(LayerClock {
        age,
        life: (age / layer.duration).clamp(0.0, 1.0),
        exit,
    })
}

/// The seed of the effect instance spawned as number `sequence` of the runtime, for an effect
/// whose random choices are salted by `salt` ([`super::catalog::Effect::salt`]).
#[must_use]
pub const fn instance_seed(sequence: u32, salt: u32) -> u32 {
    hash3(sequence, 0x5EED_0F1E, salt)
}

/// The seed of placed instance `index` of placement `placement_index` of layer `layer_index` of
/// an effect instance seeded `instance_seed`: what a card's or decal's own random choices (and a
/// torn front's crests) are drawn from.
#[must_use]
pub const fn placed_seed(
    instance_seed: u32,
    layer_index: u32,
    placement_index: u32,
    index: u32,
) -> u32 {
    hash3(
        instance_seed,
        layer_index.wrapping_mul(4).wrapping_add(placement_index),
        index,
    )
}

/// A length as `u32`, saturating.
fn len32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// The longest extra delay the line placements of `layers` add on `line`.
pub fn max_line_delay(layers: &[Layer], line: Option<WorldLine>) -> f32 {
    let Some(line) = line else {
        return 0.0;
    };
    let length = line.from.distance(line.to);
    layers
        .iter()
        .flat_map(|layer| layer.placements.iter())
        .map(|placement| match placement {
            super::catalog::schema::PlacementDef::Line {
                travel,
                delay_jitter,
                ..
            } if *travel > f32::EPSILON => length / travel + delay_jitter,
            super::catalog::schema::PlacementDef::Line { delay_jitter, .. } => *delay_jitter,
            _ => 0.0,
        })
        .fold(0.0, f32::max)
}

/// No clip window.
const NO_CLIP: LineClip = [0.0; 4];

/// How far, in world units, the far ends of two requests for one line may lie apart: every
/// search point of a line publishes the same far end.
const SAME_LINE_END: f32 = 1e-4;

/// Seconds after a line effect's last travel delay during which a request for its line still
/// joins it: a search lands on the simulation's tick, up to a frame after the presentation
/// time its travel delay predicts.
const LINE_MERGE_SLACK: f32 = 0.1;

/// Shortest ribbon span drawn, world units: a span this short is the head just leaving the
/// tail (or the tail catching the head), not yet or no longer a strip.
const RIBBON_MIN_LENGTH: f32 = 1e-3;

/// The span of a ribbon along a line of `length` world units at `age` seconds of its layer,
/// as distances from the line's start: `(tail, head)`. The head races out at the look's
/// `travel` (the whole line at once without one); after `tail_delay` the tail follows at the
/// same speed, so the strip becomes a segment sweeping along the line and then shrinks into
/// its end. Without travel the tail stays at the start.
#[must_use]
pub fn ribbon_span(look: &RibbonLook, length: f32, age: f32) -> (f32, f32) {
    if look.travel <= 0.0 {
        return (0.0, length);
    }
    let head = (look.travel * age).clamp(0.0, length);
    let tail = look.tail_delay.map_or(0.0, |delay| {
        (look.travel * (age - delay)).clamp(0.0, length)
    });
    (tail.min(head), head)
}

/// The clip window of `line`: its XZ direction and the range of `dot(xz, direction)` from its
/// start to its end; no clip for a line without horizontal length.
#[must_use]
pub fn line_clip_window(line: WorldLine) -> LineClip {
    let along = Vec2::new(line.to.x - line.from.x, line.to.z - line.from.z);
    let length = along.length();
    if length <= f32::EPSILON {
        return NO_CLIP;
    }
    let axis = along / length;
    let start = axis.dot(Vec2::new(line.from.x, line.from.z));
    [axis.x, axis.y, start, start + length]
}

/// A standing card of half width `extent` (world units) at `position` on `line`, made smaller
/// and moved inward when the line's nearer end would cut it, so that it stands whole between
/// the line's ends (review round 5: the first clump, cut at the line's start, read as a lone
/// spike). Such a card keeps its far edge and spans from the end to that edge, so it stays
/// over its own point; it is never wider than the line. Returns the card's position and half
/// width; unchanged when the card already fits or the line has no horizontal length.
#[must_use]
pub fn fit_inside_line(line: WorldLine, position: Vec3, extent: f32) -> (Vec3, f32) {
    let window = line_clip_window(line);
    if window == NO_CLIP {
        return (position, extent);
    }
    let [x, z, start, end] = window;
    let along = x * position.x + z * position.z;
    let (behind, ahead) = ((along - start).max(0.0), (end - along).max(0.0));
    if behind >= extent && ahead >= extent {
        return (position, extent);
    }
    let room = behind.min(ahead);
    let fitted = f32::midpoint(extent, room).min(0.5 * (end - start));
    let centre = if behind <= ahead {
        start + fitted
    } else {
        end - fitted
    };
    (position + Vec3::new(x, 0.0, z) * (centre - along), fitted)
}

/// The clip window of an instance at `position` of a layer that `clips` to the event's
/// `line`: [`seen_clip_window`] with the line's bound spacing, or no clip.
fn clip_window(
    line: Option<WorldLine>,
    clips: bool,
    bound: &BoundValues,
    position: Vec3,
) -> LineClip {
    match line {
        Some(line) if clips => seen_clip_window(
            line,
            bound.get(Binding::LineSpacing),
            Vec2::new(position.x, position.z),
        ),
        _ => NO_CLIP,
    }
}

/// The clip window of an instance at `position` (XZ) on `line`, whose search points lie
/// `spacing` apart from its start: the whole line when the viewer sees every point, otherwise
/// the run of seen points around the instance's own point, ending half a spacing beyond its
/// outermost seen points (where the fog's cell edge lies for points at cell centres), so the
/// ground-hugging layers of the line (marks, dust, chips) end at the same place where the line
/// runs into the fog (plan Amendment 2, R6; review round 4). No clip for a line without
/// horizontal length.
#[must_use]
pub fn seen_clip_window(line: WorldLine, spacing: f32, position: Vec2) -> LineClip {
    let whole = line_clip_window(line);
    if whole == NO_CLIP || line.seen == u64::MAX || spacing <= f32::EPSILON {
        return whole;
    }
    let [x, z, start, end] = whole;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a point index of a line of at most a few hundred spacings, clamped at 0"
    )]
    let (point, last) = (
        ((position.dot(Vec2::new(x, z)) - start) / spacing)
            .round()
            .max(0.0) as u32,
        ((end - start) / spacing + 1e-3).floor().max(0.0) as u32,
    );
    let point = point.min(last);
    let mut low = point;
    while low > 0 && line.sees_point(low - 1) {
        low -= 1;
    }
    let mut high = point;
    while high < last && line.sees_point(high + 1) {
        high += 1;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a small point index is exact in f32"
    )]
    let (low_at, high_at) = (
        if low == 0 {
            start
        } else {
            start + (low as f32 - 0.5) * spacing
        },
        if high == last {
            end
        } else {
            start + (high as f32 + 0.5) * spacing
        },
    );
    [x, z, low_at, high_at]
}

/// Offset of a card that moves `spread` world units away from its anchor over its life,
/// easing out; `from_anchor` is its placed offset (a hashed direction when it is at the anchor).
fn spread_offset(spread: f32, from_anchor: Vec3, life: f32, seed: u32) -> Vec3 {
    if spread <= 0.0 {
        return Vec3::ZERO;
    }
    let flat = Vec3::new(from_anchor.x, 0.0, from_anchor.z);
    let direction = flat.try_normalize().unwrap_or_else(|| {
        let angle = std::f32::consts::TAU * unit_float(hash3(seed, 0x5B2E, 0));
        Vec3::new(angle.cos(), 0.0, angle.sin())
    });
    let eased = 1.0 - (1.0 - life.clamp(0.0, 1.0)).powi(2);
    direction * spread * eased
}

/// The breakup fields of one card or decal: its noise layer and parameters and its own noise
/// phase drawn from its hash.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Surface {
    /// Noise phase in tiles.
    offset: [f32; 2],
    /// Noise repeats.
    scale: f32,
    /// Weight of the noise in the erosion order.
    amount: f32,
    /// Soft-step width of the erosion threshold.
    softness: f32,
    /// Self-shading.
    shade: f32,
    /// Noise layer.
    layer: u32,
}

impl Surface {
    /// No breakup: the baked erosion order with the default soft step.
    const NONE: Self = Self {
        offset: [0.0; 2],
        scale: 1.0,
        amount: 0.0,
        softness: DEFAULT_SOFTNESS,
        shade: 0.0,
        layer: NO_NOISE,
    };

    /// The fields of `breakup` with a phase drawn from `seed`.
    fn of(breakup: Option<&Breakup>, seed: u32) -> Self {
        breakup.map_or(Self::NONE, |breakup| Self {
            offset: [
                unit_float(hash3(seed, 0x4015_E001, 0)),
                unit_float(hash3(seed, 0x4015_E002, 0)),
            ],
            scale: breakup.scale,
            amount: breakup.amount,
            softness: breakup.softness,
            shade: breakup.shade,
            layer: breakup.noise_layer,
        })
    }

    /// A card with these fields and every other field zero.
    fn card(self) -> SpriteInstance {
        SpriteInstance {
            noise_offset: self.offset,
            noise_scale: self.scale,
            noise_amount: self.amount,
            softness: self.softness,
            shade: self.shade,
            noise_layer: self.layer,
            ..SpriteInstance::default()
        }
    }

    /// A decal with these fields and every other field zero.
    fn decal(self) -> DecalInstance {
        DecalInstance {
            noise_offset: self.offset,
            noise_scale: self.scale,
            noise_amount: self.amount,
            softness: self.softness,
            shade: self.shade,
            noise_layer: self.layer,
            ..DecalInstance::default()
        }
    }
}

/// The roll in radians and the `FLAG_*` mirror bit of a card under `rotation`, drawn from
/// the card's hash `seed`.
#[must_use]
pub fn roll_and_mirror(rotation: RotationDef, seed: u32) -> (f32, u32) {
    let mirror = if hash3(seed, 0x0303, 0) & 1 == 1 {
        FLAG_MIRROR
    } else {
        0
    };
    match rotation {
        RotationDef::Fixed => (0.0, 0),
        RotationDef::Random => (
            std::f32::consts::TAU * unit_float(hash3(seed, 0x0207, 0)),
            mirror,
        ),
        RotationDef::Mirror => (0.0, mirror),
    }
}

/// The frame of a variant set (see [`ElementRef::is_variant_set`]) shown by the instance
/// hashed `seed`, as `(layer, layer, 0.0)`; `None` for a flipbook or a still.
#[must_use]
pub fn variant_frame(element: &ElementRef, seed: u32) -> Option<(u32, u32, f32)> {
    if !element.is_variant_set() {
        return None;
    }
    let layer = element.first_layer + hash3(seed, 0x7A41, 0) % element.frames;
    Some((layer, layer, 0.0))
}

/// The frame of a card: its variant, its flipbook position from the layer's frame curve, or
/// its flipbook frame by `fps` or fitted to the layer.
fn card_frame(
    catalog: &EffectCatalog,
    look: &CardLook,
    age: f32,
    life: f32,
    seed: u32,
) -> (u32, u32, f32) {
    if let Some(frame) = variant_frame(&look.element, seed) {
        return frame;
    }
    match look.frame_curve {
        Some(curve) => curve_frame(catalog, &look.element, curve, life, look.frame_blend),
        None => flipbook_frame(&look.element, look.fps, age, life, look.frame_blend),
    }
}

/// The frame of `element` at the flipbook position the curve `curve` gives at `life` (0 the
/// first frame, 1 the last).
fn curve_frame(
    catalog: &EffectCatalog,
    element: &ElementRef,
    curve: CurveId,
    life: f32,
    blend: bool,
) -> (u32, u32, f32) {
    let frames = element.frames.max(1);
    #[expect(
        clippy::cast_precision_loss,
        reason = "frame counts are far below 2^24"
    )]
    let last = (frames - 1) as f32;
    let position = catalog.sample(Some(curve), life, 0.0).clamp(0.0, 1.0) * last;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "position is clamped to 0..frames-1"
    )]
    let whole = position.floor() as u32;
    let next = (whole + 1).min(frames - 1);
    let mix = if blend {
        position - position.floor()
    } else {
        0.0
    };
    (element.first_layer + whole, element.first_layer + next, mix)
}

/// The two array layers and their blend for element `element` at `age` seconds into a layer
/// at life fraction `life`: at `fps`, or fitted to the layer's life when `fps` is `None`.
#[must_use]
pub fn flipbook_frame(
    element: &ElementRef,
    fps: Option<f32>,
    age: f32,
    life: f32,
    blend: bool,
) -> (u32, u32, f32) {
    let frames = element.frames.max(1);
    #[expect(
        clippy::cast_precision_loss,
        reason = "frame counts are far below 2^24"
    )]
    let frame_count = frames as f32;
    let position = fps.map_or(life * frame_count, |fps| age * fps);
    let last = frame_count - 1.0;
    let position = position.clamp(0.0, last);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "position is clamped to 0..frames-1"
    )]
    let whole = position.floor() as u32;
    let next = (whole + 1).min(frames - 1);
    let mix = if blend {
        position - position.floor()
    } else {
        0.0
    };
    (element.first_layer + whole, element.first_layer + next, mix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::catalog::ElementRef;

    fn ribbon(travel: f32, tail_delay: Option<f32>) -> RibbonLook {
        RibbonLook {
            element: ElementRef {
                first_layer: 0,
                frames: 1,
                duration_secs: None,
            },
            ramp: super::super::catalog::RampId(0),
            ground: false,
            repeat: 1.0,
            scroll: 0.0,
            travel,
            tail_delay,
            taper: (0.1, 0.1),
            height: 0.0,
            soft_depth: 0.0,
            erosion: None,
            sort_bias: 0.0,
            shape: CardShape::Radial,
            breakup: None,
        }
    }

    #[test]
    fn a_ribbon_races_out_then_sweeps_and_shrinks_into_its_end() {
        assert_eq!(
            ribbon_span(&ribbon(0.0, None), 6.0, 0.3),
            (0.0, 6.0),
            "no travel"
        );
        assert_eq!(
            ribbon_span(&ribbon(0.0, Some(0.1)), 6.0, 0.3),
            (0.0, 6.0),
            "without travel the tail stays"
        );
        let racing = ribbon(10.0, Some(0.2));
        assert_eq!(
            ribbon_span(&racing, 6.0, 0.0),
            (0.0, 0.0),
            "nothing at once"
        );
        let (tail, head) = ribbon_span(&racing, 6.0, 0.1);
        assert!(
            tail.abs() < 1e-6 && (head - 1.0).abs() < 1e-5,
            "head out 1 unit"
        );
        let (tail, head) = ribbon_span(&racing, 6.0, 0.5);
        assert!(
            (tail - 3.0).abs() < 1e-5 && (head - 5.0).abs() < 1e-5,
            "a segment sweeping: {tail}..{head}"
        );
        let (tail, head) = ribbon_span(&racing, 6.0, 1.0);
        assert!(
            (tail - 6.0).abs() < 1e-5 && (head - 6.0).abs() < 1e-5,
            "shrunk into the end"
        );
        let (tail, head) = ribbon_span(&ribbon(10.0, None), 6.0, 5.0);
        assert!(tail.abs() < 1e-6 && (head - 6.0).abs() < 1e-5, "held whole");
    }

    #[test]
    fn a_line_clip_window_spans_the_line_from_its_start_to_its_end() {
        let line = WorldLine::whole(Vec3::new(2.0, 1.0, 3.0), Vec3::new(2.0, 1.0, 11.0));
        let [x, z, min, max] = line_clip_window(line);
        assert!(x.abs() < 1e-6 && (z - 1.0).abs() < 1e-6, "along +z");
        assert!((min - 3.0).abs() < 1e-5 && (max - 11.0).abs() < 1e-5);
        let point = Vec3::new(2.0, 0.0, 3.0);
        assert!(((Vec2::new(point.x, point.z).dot(Vec2::new(x, z))) - min).abs() < 1e-5);
        let vertical = WorldLine::whole(Vec3::ZERO, Vec3::new(0.0, 4.0, 0.0));
        assert_eq!(
            line_clip_window(vertical),
            NO_CLIP,
            "no horizontal length, no clip"
        );
    }

    #[test]
    fn a_standing_card_at_a_line_end_is_fitted_whole_inside_the_line() {
        let line = WorldLine::whole(Vec3::new(2.0, 0.0, 1.0), Vec3::new(2.0, 0.0, 9.0));
        let middle = Vec3::new(2.0, 0.0, 5.0);
        assert_eq!(
            fit_inside_line(line, middle, 0.9),
            (middle, 0.9),
            "a card that fits is unchanged"
        );
        // At the first point: half as wide, spanning from the start to its old far edge.
        let (position, extent) = fit_inside_line(line, Vec3::new(2.0, 0.0, 1.0), 0.9);
        assert!((extent - 0.45).abs() < 1e-6, "{extent}");
        assert!(
            position.distance(Vec3::new(2.0, 0.0, 1.45)) < 1e-5,
            "{position}"
        );
        // Near the last point, 0.3 short of it: it keeps its far edge 0.9 behind its point.
        let (position, extent) = fit_inside_line(line, Vec3::new(2.0, 0.0, 8.7), 0.9);
        assert!((extent - 0.6).abs() < 1e-6, "{extent}");
        assert!(
            (position.z - 8.4).abs() < 1e-5 && (position.z - extent - 7.8).abs() < 1e-5,
            "{position}"
        );
        // Never wider than a short line.
        let short = WorldLine::whole(Vec3::ZERO, Vec3::new(0.6, 0.0, 0.0));
        let (position, extent) = fit_inside_line(short, Vec3::ZERO, 0.9);
        assert!((extent - 0.3).abs() < 1e-6 && (position.x - 0.3).abs() < 1e-6);
        // A line without horizontal length leaves the card alone.
        let vertical = WorldLine::whole(Vec3::ZERO, Vec3::Y);
        assert_eq!(
            fit_inside_line(vertical, Vec3::ZERO, 0.9),
            (Vec3::ZERO, 0.9)
        );
    }

    #[test]
    fn a_partly_seen_line_is_clipped_to_the_seen_run_around_each_instance() {
        let mut line = WorldLine::whole(Vec3::new(0.0, 0.0, 0.0), Vec3::new(8.0, 0.0, 0.0));
        assert_eq!(
            seen_clip_window(line, 1.0, Vec2::new(3.0, 0.0)),
            line_clip_window(line),
            "a wholly seen line keeps its whole window"
        );
        // Points 0..=3 fogged, 4..=6 seen, 7 fogged, 8 seen.
        line.seen = 0b1_0111_0000;
        let [_, _, min, max] = seen_clip_window(line, 1.0, Vec2::new(5.2, 0.3));
        assert!(
            (min - 3.5).abs() < 1e-5 && (max - 6.5).abs() < 1e-5,
            "{min} {max}"
        );
        let [_, _, min, max] = seen_clip_window(line, 1.0, Vec2::new(8.0, 0.0));
        assert!(
            (min - 7.5).abs() < 1e-5 && (max - 8.0).abs() < 1e-5,
            "the end point keeps the line's end: {min} {max}"
        );
        line.seen = 0b0_0000_0111;
        let [_, _, min, max] = seen_clip_window(line, 1.0, Vec2::new(-0.4, 0.0));
        assert!(
            min.abs() < 1e-5 && (max - 2.5).abs() < 1e-5,
            "the first point keeps the line's start"
        );
        assert_eq!(
            seen_clip_window(line, 0.0, Vec2::ZERO),
            line_clip_window(line),
            "no spacing bound: the whole window"
        );
    }

    #[test]
    fn spreading_cards_move_outward_and_ease_out() {
        assert_eq!(spread_offset(0.0, Vec3::X, 0.5, 1), Vec3::ZERO);
        let half = spread_offset(1.0, Vec3::new(0.0, 0.0, 2.0), 0.5, 1);
        let end = spread_offset(1.0, Vec3::new(0.0, 0.0, 2.0), 1.0, 1);
        assert!(
            (end - Vec3::Z).length() < 1e-6,
            "the whole spread along the placed offset"
        );
        assert!(
            (half.z - 0.75).abs() < 1e-6,
            "eased out: three quarters at half life"
        );
        let from_anchor = spread_offset(1.0, Vec3::ZERO, 1.0, 9);
        assert!((from_anchor.length() - 1.0).abs() < 1e-5 && from_anchor.y.abs() < 1e-6);
    }

    #[test]
    fn breakup_gives_every_instance_its_own_noise_phase() {
        let breakup = Breakup {
            noise_layer: 5,
            scale: 2.0,
            amount: 0.4,
            softness: 0.1,
            shade: 0.3,
        };
        let a = Surface::of(Some(&breakup), 1);
        let b = Surface::of(Some(&breakup), 2);
        assert_ne!(a.offset, b.offset, "phases differ per instance");
        assert!(a.offset.iter().all(|value| (0.0..1.0).contains(value)));
        assert_eq!(a.layer, 5);
        let card = a.card();
        assert_eq!(card.noise_layer, 5);
        assert!((card.noise_amount - 0.4).abs() < f32::EPSILON);
        let none = Surface::of(None, 1).decal();
        assert_eq!(none.noise_layer, NO_NOISE);
        assert!(none.noise_amount.abs() < f32::EPSILON);
        assert!((none.softness - DEFAULT_SOFTNESS).abs() < f32::EPSILON);
    }

    #[test]
    fn variant_sets_pick_one_whole_variant_and_flipbooks_none() {
        let variants = ElementRef {
            first_layer: 10,
            frames: 4,
            duration_secs: None,
        };
        let picks: std::collections::BTreeSet<u32> = (0..64)
            .filter_map(|seed| variant_frame(&variants, seed))
            .map(|(a, b, mix)| {
                assert_eq!(a, b);
                assert!(mix.abs() < f32::EPSILON);
                a
            })
            .collect();
        assert_eq!(picks, (10..14).collect(), "every variant is used");
        let flipbook = ElementRef {
            duration_secs: Some(1.0),
            ..variants
        };
        assert!(variant_frame(&flipbook, 3).is_none());
        let still = ElementRef {
            frames: 1,
            ..variants
        };
        assert!(variant_frame(&still, 3).is_none());
    }

    #[test]
    fn mirror_never_rolls_and_fixed_never_mirrors() {
        let mut mirrored = 0;
        for seed in 0..64 {
            let (roll, flags) = roll_and_mirror(RotationDef::Mirror, seed);
            assert!(roll.abs() < f32::EPSILON);
            mirrored += flags & FLAG_MIRROR;
            assert_eq!(roll_and_mirror(RotationDef::Fixed, seed), (0.0, 0));
            let (roll, _) = roll_and_mirror(RotationDef::Random, seed);
            assert!((0.0..std::f32::consts::TAU).contains(&roll));
        }
        assert!(
            mirrored > 16 && mirrored < 48,
            "about half are mirrored: {mirrored}"
        );
    }
}
