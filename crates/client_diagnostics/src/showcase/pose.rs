//! Camera poses of the showcase: scripted moves, a deterministic follow spring and scene blends.
//!
//! # Responsibility
//! Pure functions from a frame index (and, for follow moves, a target point) to a [`Pose`]. The
//! director ([`super::director`]) writes the result into the production `RtsCamera` and the camera
//! transform; nothing here touches Bevy's world.
//!
//! # Units
//! Positions are on the map plane in world units (one unit is one grid cell; `x` is world x, `y` is
//! world z). Distances are world units from the focus along the view direction. `pitch` and `yaw`
//! are radians: pitch above the horizontal, yaw about the vertical axis, 0 looking north as the
//! production camera does.
//!
//! # Invariants
//! - Every function is a fixed sequence of `f32` operations on its inputs, with a fixed 40 ms step
//!   for the spring, so the same inputs give bit-identical poses on one machine.
//! - The spring is critically damped: it never overshoots a still target.

use bevy::math::Vec2;

/// One simulated video frame: the 40 ms step of a 25 fps video, in seconds.
pub const FRAME_SECONDS: f32 = 0.04;

/// Time constant of the follow spring, in seconds.
pub const SPRING_SMOOTH_SECONDS: f32 = 0.6;

/// Frames after which the follow spring is within 1 % of a still target's initial distance.
pub const SPRING_SETTLE_FRAMES: u32 = 50;

/// Frames over which a scripted scene blends in from the previous scene's last pose.
pub const BLEND_FRAMES: u32 = 20;

/// A camera pose on the map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Ground focus on the map plane, world units.
    pub focus: Vec2,
    /// Distance from the focus, world units.
    pub distance: f32,
    /// Pitch above the horizontal, radians.
    pub pitch: f32,
    /// Yaw about the vertical axis, radians; 0 looks north like the production camera.
    pub yaw: f32,
}

/// `3t² − 2t³`, clamped to `0..=1`.
#[must_use]
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Shortest signed angular difference `to − from`, in `-π..=π`.
#[must_use]
pub fn angle_difference(from: f32, to: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let difference = (to - from).rem_euclid(tau);
    if difference > std::f32::consts::PI {
        difference - tau
    } else {
        difference
    }
}

/// Linear interpolation between two poses; yaw takes the short way round.
#[must_use]
pub fn mix(from: Pose, to: Pose, t: f32) -> Pose {
    Pose {
        focus: from.focus + (to.focus - from.focus) * t,
        distance: from.distance + (to.distance - from.distance) * t,
        pitch: from.pitch + (to.pitch - from.pitch) * t,
        yaw: from.yaw + angle_difference(from.yaw, to.yaw) * t,
    }
}

/// A key of a scripted path: the pose at a frame of the scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// Frame of the scene this key is reached at.
    pub frame: u32,
    /// The pose at that frame.
    pub pose: Pose,
}

/// The pose of a path at `frame`: smoothstep between the surrounding keys, held before the first
/// and after the last. `keys` must be non-empty and sorted by frame (the storyboard validates it).
#[must_use]
pub fn path_pose(keys: &[Key], frame: u32) -> Option<Pose> {
    let first = keys.first()?;
    if frame <= first.frame {
        return Some(first.pose);
    }
    for pair in keys.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if frame <= to.frame {
            let span = to.frame.saturating_sub(from.frame).max(1);
            #[expect(
                clippy::cast_precision_loss,
                reason = "scene frames are at most 3,000, exact in f32"
            )]
            let t = (frame - from.frame) as f32 / span as f32;
            return Some(mix(from.pose, to.pose, smoothstep(t)));
        }
    }
    keys.last().map(|key| key.pose)
}

/// An orbit about `center`: yaw from `start` sweeping by `sweep` radians, smoothstepped over the
/// scene's `length` frames.
#[must_use]
pub fn orbit_pose(
    center: Vec2,
    distance: f32,
    pitch: f32,
    start: f32,
    sweep: f32,
    frame: u32,
    length: u32,
) -> Pose {
    #[expect(
        clippy::cast_precision_loss,
        reason = "scene frames are at most 3,000, exact in f32"
    )]
    let t = frame as f32 / length.saturating_sub(1).max(1) as f32;
    Pose {
        focus: center,
        distance,
        pitch,
        yaw: start + sweep * smoothstep(t),
    }
}

/// A critically damped spring following a moving target with a fixed [`FRAME_SECONDS`] step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// The current pose.
    pub pose: Pose,
    velocity: [f32; 5],
}

impl Spring {
    /// A spring at rest at `pose`.
    #[must_use]
    pub fn at_rest(pose: Pose) -> Self {
        Self {
            pose,
            velocity: [0.0; 5],
        }
    }

    /// Advances one frame towards `target` and returns the new pose.
    pub fn step(&mut self, target: Pose) -> Pose {
        let yaw_target = self.pose.yaw + angle_difference(self.pose.yaw, target.yaw);
        let current = [
            self.pose.focus.x,
            self.pose.focus.y,
            self.pose.distance,
            self.pose.pitch,
            self.pose.yaw,
        ];
        let goal = [
            target.focus.x,
            target.focus.y,
            target.distance,
            target.pitch,
            yaw_target,
        ];
        let mut next = [0.0; 5];
        for index in 0..5 {
            next[index] = damp(current[index], goal[index], &mut self.velocity[index]);
        }
        self.pose = Pose {
            focus: Vec2::new(next[0], next[1]),
            distance: next[2],
            pitch: next[3],
            yaw: next[4],
        };
        self.pose
    }
}

/// One step of a critically damped spring (the closed form used by game engines' "smooth damp").
fn damp(current: f32, target: f32, velocity: &mut f32) -> f32 {
    let omega = 2.0 / SPRING_SMOOTH_SECONDS;
    let x = omega * FRAME_SECONDS;
    let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let change = current - target;
    let temp = (*velocity + omega * change) * FRAME_SECONDS;
    *velocity = (*velocity - omega * temp) * decay;
    target + (change + temp) * decay
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(x: f32, distance: f32) -> Pose {
        Pose {
            focus: Vec2::new(x, 2.0 * x),
            distance,
            pitch: 1.0,
            yaw: 0.0,
        }
    }

    #[test]
    fn follow_spring_is_deterministic_and_converges() {
        let start = pose(0.0, 10.0);
        let target = Pose {
            focus: Vec2::new(30.0, 20.0),
            distance: 25.0,
            pitch: 0.5,
            yaw: 0.8,
        };
        let run = || {
            let mut spring = Spring::at_rest(start);
            (0..SPRING_SETTLE_FRAMES)
                .map(|_| spring.step(target))
                .collect::<Vec<_>>()
        };
        let first = run();
        let second = run();
        let bits = |poses: &[Pose]| {
            poses
                .iter()
                .flat_map(|p| [p.focus.x, p.focus.y, p.distance, p.pitch, p.yaw].map(f32::to_bits))
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(&first), bits(&second));
        let last = first.last().copied().unwrap_or(start);
        let initial = (target.focus - start.focus).length();
        assert!((target.focus - last.focus).length() <= 0.01 * initial);
        assert!((target.distance - last.distance).abs() <= 0.01 * 15.0);
        assert!((target.yaw - last.yaw).abs() <= 0.01 * 0.8);
        // Critically damped: never past the target on the way.
        assert!(first.iter().all(|p| p.focus.x <= target.focus.x + 1e-4));
    }

    #[test]
    fn path_holds_ends_and_smoothsteps_between_keys() {
        let keys = [
            Key {
                frame: 0,
                pose: pose(0.0, 10.0),
            },
            Key {
                frame: 10,
                pose: pose(10.0, 20.0),
            },
        ];
        assert_eq!(path_pose(&keys, 0), Some(keys[0].pose));
        assert_eq!(path_pose(&keys, 25), Some(keys[1].pose));
        let middle = path_pose(&keys, 5).map(|p| p.distance);
        assert_eq!(middle, Some(15.0));
        assert_eq!(path_pose(&[], 3), None);
    }

    #[test]
    fn angle_difference_takes_the_short_way() {
        let pi = std::f32::consts::PI;
        assert!((angle_difference(0.1, 2.0 * pi - 0.1) + 0.2).abs() < 1e-5);
        assert!((angle_difference(-0.1, 0.1) - 0.2).abs() < 1e-5);
    }

    #[test]
    fn orbit_sweeps_from_start_to_end() {
        let center = Vec2::new(1.0, 2.0);
        assert_eq!(orbit_pose(center, 5.0, 1.0, 0.5, 1.0, 0, 11).yaw, 0.5);
        assert_eq!(orbit_pose(center, 5.0, 1.0, 0.5, 1.0, 10, 11).yaw, 1.5);
    }
}
