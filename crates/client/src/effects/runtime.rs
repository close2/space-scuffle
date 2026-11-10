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
//! detail card or decal that finds no room is skipped, and the oldest `Minor` or `Ambient`
//! effect's detail is retired (it erodes out over 0.1 s). An effect spawned over its class cap
//! starts with its detail retired. Layers smaller than `min_screen_px` are skipped; bursts
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
    DecalYaw, EffectClass, Orient, ParticleOrient, Priority, RingMapping, RotationDef,
};
use super::catalog::{
    Block, CardLook, CurveId, DecalLook, EffectCatalog, EffectId, ElementRef, Layer, Pool,
};
use super::instances::{
    DecalInstance, LightInstance, SpriteInstance, FLAG_MIRROR, FLAG_POLAR, ORIENT_CAMERA,
    ORIENT_GROUND, ORIENT_STRETCHED, ORIENT_VERTICAL,
};
use super::particles::{Burst, ParticlePool};
use super::placement::{hash3, instance_count, place, unit_float, WorldLine};
use super::pool::{EffectHandle, EffectInstance, EffectInstancePool};
use bevy::math::Vec3;
use bevy::prelude::Resource;

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
    /// Point lights.
    pub lights: Vec<LightInstance>,
    /// Cards written this frame (not particles).
    card_count: u32,
}

impl EffectFrame {
    /// A frame with room for `cards` cards (cards and particles), `decals` and `lights`.
    fn with_capacity(cards: usize, decals: usize, lights: usize) -> Self {
        Self {
            cards: Vec::with_capacity(cards),
            depth: Vec::with_capacity(cards),
            order: Vec::with_capacity(cards),
            sorted: Vec::with_capacity(cards),
            decals: Vec::with_capacity(decals),
            lights: Vec::with_capacity(lights),
            card_count: 0,
        }
    }

    /// Address and capacity of every retained vector, which an allocation would change.
    #[must_use]
    pub fn buffers(&self) -> [(usize, usize); 6] {
        [
            (self.cards.as_ptr() as usize, self.cards.capacity()),
            (self.depth.as_ptr() as usize, self.depth.capacity()),
            (self.order.as_ptr() as usize, self.order.capacity()),
            (self.sorted.as_ptr() as usize, self.sorted.capacity()),
            (self.decals.as_ptr() as usize, self.decals.capacity()),
            (self.lights.as_ptr() as usize, self.lights.capacity()),
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
            ),
            stats: EffectStats::default(),
            last_build: None,
            only: None,
        }
    }

    /// Spawns `request` at time `now` (seconds of `Time<Virtual>`).
    pub fn spawn(
        &mut self,
        catalog: &EffectCatalog,
        request: &SpawnRequest,
        now: f64,
    ) -> Option<EffectHandle> {
        let effect = catalog.effect(request.effect)?;
        let birth = now + f64::from(request.delay);
        let line_delay = max_line_delay(&effect.layers, request.line);
        let over_cap =
            self.pool.live(effect.class) >= catalog.budget.class_caps[effect.class.index()];
        let instance = EffectInstance {
            effect: request.effect,
            class: effect.class,
            birth,
            end: birth + f64::from(effect.lifetime + line_delay),
            anchor: request.anchor,
            bound: request.bound,
            line: request.line,
            seed: hash3(self.sequence, 0x5EED_0F1E, u32::from(request.effect.0)),
            detail_retired_at: over_cap.then_some(birth),
            has_detail: effect
                .layers
                .iter()
                .any(|layer| layer.priority == Priority::Detail),
            bursts_through: f32::NEG_INFINITY,
        };
        self.sequence = self.sequence.wrapping_add(1);
        if let Ok(handle) = self.pool.spawn(instance) {
            self.stats.spawned += 1;
            Some(handle)
        } else {
            self.stats.refused += 1;
            None
        }
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
        self.frame.lights.clear();
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
        self.fire_bursts(catalog, now, camera);
        self.particles.step(dt, &catalog.emitters);
        self.frame.cards.clear();
        self.frame.depth.clear();
        self.frame.decals.clear();
        self.frame.lights.clear();
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

    /// Retires the detail of the oldest `Minor` or `Ambient` effect still drawing it.
    fn retire_one(&mut self, now: f64) {
        let candidate = retirement_candidate(self.pool.iter().map(|(index, instance)| {
            (
                index,
                RetireCandidate {
                    class: instance.class,
                    birth: instance.birth,
                    detail_retired: instance.detail_retired_at.is_some(),
                    has_detail: instance.has_detail,
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
                let seed = hash3(instance.seed, layer_salt * 64 + placement_salt, index);
                let placed = place(placement, index, seed, &instance.bound, instance.line);
                let due = layer.start + placed.delay;
                if due <= window.0 || due > window.1 {
                    continue;
                }
                let burst = Burst {
                    emitter: emitter_id,
                    origin: instance.anchor + placed.offset,
                    radius,
                    count,
                    seed,
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
                    Pool::Particles => continue,
                };
                let context = LayerContext {
                    catalog,
                    instance: &instance,
                    layer,
                    layer_index,
                    now,
                    fade,
                    camera,
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
            let salt = u32::try_from(context.layer_index * 4 + placement_index).unwrap_or(0);
            for index in 0..instance_count(placement, &instance.bound, instance.line) {
                let seed = hash3(instance.seed, salt, index);
                let placed = place(placement, index, seed, &instance.bound, instance.line);
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "an effect's age in seconds fits f32"
                )]
                let age = (context.now - instance.birth) as f32 - layer.start - placed.delay;
                if age < 0.0 || age > layer.duration {
                    continue;
                }
                if !layer_admitted(layer.priority, free) {
                    overflow += 1;
                    continue;
                }
                let position = instance.anchor + placed.offset;
                match self.write_instance(context, position, age, seed) {
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
        age: f32,
        seed: u32,
    ) -> Written {
        let layer = context.layer;
        let catalog = context.catalog;
        let life = (age / layer.duration).clamp(0.0, 1.0);
        let intensity = catalog.sample(layer.curve, life, 1.0) * context.fade;
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
        if let Some(camera) = context.camera {
            if lod_skips(
                camera.pixels(extent * 2.0, position),
                context.catalog.budget.min_screen_px,
            ) {
                self.stats.lod_skipped += 1;
                return Written::Skipped;
            }
        }
        match &layer.block {
            Block::Sprite(look) => self.push_card(
                context, look, position, extent, age, life, intensity, seed, 0.0,
            ),
            Block::Ring {
                look,
                thickness,
                height,
            } => self.push_card(
                context,
                look,
                position + Vec3::Y * *height,
                extent,
                age,
                life,
                intensity,
                seed,
                1.0 - thickness,
            ),
            Block::Decal(look) => {
                self.push_decal(context, look, position, extent, life, intensity, seed)
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
            Block::Particles(_) => Written::Skipped,
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
        ring_inner: f32,
    ) -> Written {
        if self.frame.cards.len() == self.frame.cards.capacity() {
            return Written::Full;
        }
        let catalog = context.catalog;
        let (layer_a, layer_b, frame_mix) = card_frame(catalog, look, age, life, seed);
        let (rotation, mut flags) = roll_and_mirror(look.rotation, seed);
        if ring_inner > 0.0 && look.mapping == RingMapping::Polar {
            flags |= FLAG_POLAR;
        }
        let line_axis = context
            .instance
            .line
            .map_or(Vec3::ZERO, |line| (line.to - line.from).normalize_or_zero());
        let vertical = look.orient == Orient::VerticalAlongLine;
        let lift = look.lift.unwrap_or(if vertical { 1.0 } else { 0.0 });
        let center = position + Vec3::Y * extent * lift;
        let (orient, axis) = match look.orient {
            Orient::Ground => (ORIENT_GROUND, Vec3::ZERO),
            Orient::VerticalAlongLine if line_axis != Vec3::ZERO => (ORIENT_VERTICAL, line_axis),
            Orient::Camera | Orient::VerticalAlongLine => (ORIENT_CAMERA, Vec3::ZERO),
        };
        self.push_sorted_card(
            SpriteInstance {
                center: (center + look.drift * age).to_array(),
                half_size: extent,
                axis: axis.to_array(),
                stretch: 1.0,
                layer_a,
                layer_b,
                frame_mix,
                ramp_row: u32::from(look.ramp.0),
                intensity,
                erosion: catalog.sample(look.erosion, life, 0.0),
                soft_depth: look.soft_depth,
                ring_inner,
                rotation,
                orient,
                ramp_scale: catalog.sample(context.layer.ramp_curve, life, 1.0),
                flags,
            },
            context.camera,
        );
        self.frame.card_count += 1;
        Written::Yes
    }

    /// Appends a card and its depth.
    fn push_sorted_card(&mut self, card: SpriteInstance, camera: Option<&CameraView>) {
        let depth = camera.map_or(0.0, |camera| camera.depth(Vec3::from_array(card.center)));
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
        let mut clip = (-2.0, 2.0);
        let yaw = match (look.yaw, context.instance.line) {
            (DecalYaw::AlongLine, Some(line)) => {
                let direction = line.to - line.from;
                if direction.x.abs() + direction.z.abs() > f32::EPSILON {
                    if look.clip_to_line && extent > f32::EPSILON {
                        clip = line_clip(line, position, extent);
                    }
                    (-direction.z).atan2(direction.x)
                } else {
                    random_yaw
                }
            }
            _ => random_yaw,
        };
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
            erosion: catalog.sample(look.erosion, life, 0.0),
            ramp_scale: catalog.sample(context.layer.ramp_curve, life, 1.0),
            clip_min: clip.0,
            clip_max: clip.1,
            padding: [0.0; 3],
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
}

/// A length as `u32`, saturating.
fn len32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// The longest extra delay the line placements of `layers` add on `line`.
fn max_line_delay(layers: &[Layer], line: Option<WorldLine>) -> f32 {
    let Some(line) = line else {
        return 0.0;
    };
    let length = line.from.distance(line.to);
    layers
        .iter()
        .flat_map(|layer| layer.placements.iter())
        .map(|placement| match placement {
            super::catalog::schema::PlacementDef::Line { travel, .. } if *travel > f32::EPSILON => {
                length / travel
            }
            _ => 0.0,
        })
        .fold(0.0, f32::max)
}

/// The local-x range (in half extents `extent`) of a mark centred at `position` and turned
/// along `line` that lies between the line's ends.
#[must_use]
pub fn line_clip(line: WorldLine, position: Vec3, extent: f32) -> (f32, f32) {
    let axis = line.to - line.from;
    let length = axis.length();
    let direction = axis / length.max(f32::EPSILON);
    let along = (position - line.from).dot(direction);
    ((0.0 - along) / extent, (length - along) / extent)
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

    #[test]
    fn a_mark_on_a_line_is_clipped_at_both_ends() {
        let line = WorldLine {
            from: Vec3::ZERO,
            to: Vec3::new(8.0, 0.0, 0.0),
        };
        let (min, max) = line_clip(line, Vec3::new(0.5, 0.0, 0.0), 0.8);
        assert!((min + 0.625).abs() < 1e-6, "cut 0.5 behind the centre");
        assert!(max > 1.0, "the far end is beyond this mark");
        let (min, max) = line_clip(line, Vec3::new(7.5, 0.0, 0.0), 0.8);
        assert!(min < -1.0);
        assert!((max - 0.625).abs() < 1e-6, "cut 0.5 ahead of the centre");
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
