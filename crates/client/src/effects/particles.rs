//! CPU-simulated particles: the D-10 fallback for the `Particles` block.
//!
//! # Responsibility
//! Owns [`ParticlePool`], a fixed structure-of-arrays pool: bursts spawn into it, every frame
//! integrates gravity and drag and ages each particle, and dead particles are removed by
//! swapping the last live one into their place. The runtime draws each live particle as a
//! card through the sprite instancer.
//!
//! # How it fits
//! The `WP4a` spike rejected `bevy_hanabi` (`docs/evidence/effects/hanabi-spike.md`: its
//! per-effect sort key cannot order a cloud against a bubble it crosses, and it allocates in
//! the main world every frame). Drawn as sprite instances, particles are sorted with every
//! other card, one key per particle.
//!
//! # Invariants
//! - The arrays are allocated once at the budget's capacity; spawning, stepping and removal
//!   never allocate. A burst beyond the free capacity is truncated.
//! - A particle's motion depends only on its spawn hash and the frame steps, never on a clock.

use super::catalog::schema::EmitterShape;
use super::catalog::{Emitter, EmitterId};
use super::placement::{hash3, unit_float};
use bevy::math::Vec3;

/// The live particles, one array per attribute.
// PERFORMANCE RATIONALE:
// Why: Up to 30 000 particles are integrated every frame (§5.6); an entity or a struct per
// particle with its emitter's data copied in would be both slow and allocation-heavy.
// What: Parallel arrays (structure of arrays) allocated once at capacity, compacted by
// swap-removal so the live particles are always the first `len` entries and the integration
// loop runs over contiguous memory.
// Trade-off: Particles change index when others die, so nothing may hold a particle index
// across frames; per-particle constants are looked up through the emitter id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParticlePool {
    /// Positions, world space.
    position: Vec<Vec3>,
    /// Velocities, world units per second.
    velocity: Vec<Vec3>,
    /// Age in seconds.
    age: Vec<f32>,
    /// Life in seconds.
    life: Vec<f32>,
    /// Half-size at birth, world units.
    size: Vec<f32>,
    /// Emitter of each particle.
    emitter: Vec<EmitterId>,
    /// Spawn hash of each particle.
    seed: Vec<u32>,
    /// Capacity, fixed.
    capacity: usize,
}

/// One live particle, as the runtime reads it to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleView {
    /// Position, world space.
    pub position: Vec3,
    /// Velocity.
    pub velocity: Vec3,
    /// Life fraction 0..1.
    pub life_fraction: f32,
    /// Half-size at birth.
    pub size: f32,
    /// Emitter.
    pub emitter: EmitterId,
    /// Spawn hash.
    pub seed: u32,
}

/// Where and how a burst spawns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Burst {
    /// Emitter id.
    pub emitter: EmitterId,
    /// Centre of the burst, world space.
    pub origin: Vec3,
    /// Radius of the emitter shape, world units.
    pub radius: f32,
    /// Particles to spawn.
    pub count: u32,
    /// Seed of the burst.
    pub seed: u32,
}

impl ParticlePool {
    /// An empty pool of `capacity` particles.
    #[must_use]
    pub fn with_capacity(capacity: u32) -> Self {
        let capacity = capacity as usize;
        Self {
            position: Vec::with_capacity(capacity),
            velocity: Vec::with_capacity(capacity),
            age: Vec::with_capacity(capacity),
            life: Vec::with_capacity(capacity),
            size: Vec::with_capacity(capacity),
            emitter: Vec::with_capacity(capacity),
            seed: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Live particles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.age.len()
    }

    /// Whether no particle is live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.age.is_empty()
    }

    /// Free capacity.
    #[must_use]
    pub fn free(&self) -> usize {
        self.capacity - self.len()
    }

    /// Spawns `burst` with `emitter`'s rules; returns how many were spawned (truncated to the
    /// free capacity).
    pub fn spawn(&mut self, burst: &Burst, emitter: &Emitter) -> u32 {
        let count = (burst.count as usize).min(self.free());
        for index in 0..count {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            let draw = |salt: u32| unit_float(hash3(burst.seed, index, salt));
            let offset = match emitter.shape {
                EmitterShape::Point => Vec3::ZERO,
                EmitterShape::Disc | EmitterShape::Ring => {
                    let angle = std::f32::consts::TAU * draw(1);
                    let distance = if emitter.shape == EmitterShape::Disc {
                        burst.radius * draw(2).sqrt()
                    } else {
                        burst.radius
                    };
                    Vec3::new(angle.cos() * distance, 0.0, angle.sin() * distance)
                }
            };
            let azimuth = std::f32::consts::TAU * draw(3);
            let polar = emitter.cone * draw(4).sqrt();
            let direction = Vec3::new(
                polar.sin() * azimuth.cos(),
                polar.cos(),
                polar.sin() * azimuth.sin(),
            );
            let lerp = |range: (f32, f32), t: f32| range.0 + (range.1 - range.0) * t;
            self.position.push(burst.origin + offset);
            self.velocity.push(direction * lerp(emitter.speed, draw(5)));
            self.age.push(0.0);
            self.life.push(lerp(emitter.lifetime, draw(6)).max(1e-3));
            self.size.push(lerp(emitter.size, draw(7)));
            self.emitter.push(burst.emitter);
            self.seed.push(hash3(burst.seed, index, 8));
        }
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    /// Advances every particle by `dt` seconds under its emitter's gravity and drag, and
    /// removes those past their life.
    pub fn step(&mut self, dt: f32, emitters: &[Emitter]) {
        let mut index = 0;
        while index < self.age.len() {
            self.age[index] += dt;
            if self.age[index] >= self.life[index] {
                self.swap_remove(index);
                continue;
            }
            if let Some(emitter) = emitters.get(usize::from(self.emitter[index].0)) {
                let velocity = &mut self.velocity[index];
                velocity.y -= emitter.gravity * dt;
                *velocity *= (1.0 - emitter.drag * dt).max(0.0);
            }
            self.position[index] += self.velocity[index] * dt;
            index += 1;
        }
    }

    /// Removes every particle.
    pub fn clear(&mut self) {
        self.position.clear();
        self.velocity.clear();
        self.age.clear();
        self.life.clear();
        self.size.clear();
        self.emitter.clear();
        self.seed.clear();
    }

    /// Removes particle `index` by moving the last one into its place.
    fn swap_remove(&mut self, index: usize) {
        self.position.swap_remove(index);
        self.velocity.swap_remove(index);
        self.age.swap_remove(index);
        self.life.swap_remove(index);
        self.size.swap_remove(index);
        self.emitter.swap_remove(index);
        self.seed.swap_remove(index);
    }

    /// Every live particle.
    pub fn iter(&self) -> impl Iterator<Item = ParticleView> + '_ {
        (0..self.age.len()).map(|index| ParticleView {
            position: self.position[index],
            velocity: self.velocity[index],
            life_fraction: self.age[index] / self.life[index],
            size: self.size[index],
            emitter: self.emitter[index],
            seed: self.seed[index],
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "a failed test is a panic")]
mod tests {
    use super::*;
    use crate::effects::catalog::schema::{BurstCount, ParticleOrient};
    use crate::effects::catalog::{ElementRef, RampId};

    fn emitter(gravity: f32, drag: f32) -> Emitter {
        Emitter {
            count: BurstCount {
                base: 4,
                per_unit_radius: 0.0,
                max: 4,
            },
            shape: EmitterShape::Disc,
            speed: (2.0, 2.0),
            cone: 0.0,
            gravity,
            drag,
            lifetime: (1.0, 1.0),
            size: (0.1, 0.1),
            element: ElementRef {
                first_layer: 0,
                frames: 1,
                duration_secs: None,
            },
            orient: ParticleOrient::Camera,
            ramp: RampId(0),
            size_curve: None,
            curve: None,
        }
    }

    fn burst(count: u32) -> Burst {
        Burst {
            emitter: EmitterId(0),
            origin: Vec3::ZERO,
            radius: 1.0,
            count,
            seed: 7,
        }
    }

    #[test]
    fn a_burst_is_truncated_to_the_free_capacity_and_never_reallocates() {
        let mut pool = ParticlePool::with_capacity(5);
        let before = pool.position.capacity();
        assert_eq!(pool.spawn(&burst(4), &emitter(0.0, 0.0)), 4);
        assert_eq!(pool.spawn(&burst(4), &emitter(0.0, 0.0)), 1);
        assert_eq!(pool.len(), 5);
        assert_eq!(pool.position.capacity(), before);
        for view in pool.iter() {
            assert!(view.position.length() <= 1.0 + 1e-5, "inside the disc");
        }
    }

    #[test]
    fn gravity_bends_the_path_and_particles_die_at_their_life() {
        let mut pool = ParticlePool::with_capacity(4);
        let emitters = [emitter(10.0, 0.0)];
        pool.spawn(&burst(1), &emitters[0]);
        let start = pool.iter().next().expect("one particle");
        assert!(
            (start.velocity.y - 2.0).abs() < 1e-5,
            "a zero cone launches straight up"
        );
        for _ in 0..5 {
            pool.step(0.1, &emitters);
        }
        let mid = pool.iter().next().expect("still alive");
        assert!(
            mid.velocity.y < 2.0 - 4.9,
            "gravity slowed the climb: {}",
            mid.velocity.y
        );
        for _ in 0..6 {
            pool.step(0.1, &emitters);
        }
        assert!(pool.is_empty(), "dead after its one-second life");
    }

    #[test]
    fn drag_slows_and_the_same_burst_spawns_the_same_particles() {
        let emitters = [emitter(0.0, 2.0)];
        let mut a = ParticlePool::with_capacity(4);
        let mut b = ParticlePool::with_capacity(4);
        a.spawn(&burst(4), &emitters[0]);
        b.spawn(&burst(4), &emitters[0]);
        a.step(0.1, &emitters);
        b.step(0.1, &emitters);
        assert_eq!(a, b);
        let speed = a.iter().next().expect("alive").velocity.length();
        assert!(
            (speed - 2.0 * 0.8).abs() < 1e-4,
            "drag 2/s over 0.1 s: {speed}"
        );
    }
}
