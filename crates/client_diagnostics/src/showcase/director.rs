//! The director: per video frame, the scene, the battle speed and the camera pose.
//!
//! # Responsibility
//! Plan `docs/plans/2026-11-05-ci-showcase-video.md`, decision D-5. One exclusive system in
//! `BattleSet::ExternalControl` ([`control`]) sets, for the current video frame:
//! 1. `LabState.battle_paused` and `simulation_speed` from the scene;
//! 2. the camera pose, written into the production `RtsCamera` (focus, distance, pitch) and kept in
//!    [`CameraYaw`] for the transform override of an orbit;
//! 3. cleared keyboard, mouse and wheel input, so no hotkey or edge of the window acts.
//!
//! [`SceneClock`] decides, after each recorded frame, whether the scene ends, including the
//! event-anchored `UntilCombat` scenes, which end on the first frame whose ticks contain hostile
//! damage and fail loudly at their maximum. [`CombatTracker`] observes that damage in the
//! authoritative world.
//!
//! # Invariants
//! - The director never changes the simulation: it issues no `PlayerCommand` and edits no entity.
//!   It reads the session's world and writes only presentation state (charter §3.2).
//! - Every decision is a function of the frame index and of the simulation state, never of wall
//!   time, so a take is reproducible.

use super::pose::{self, Key, Pose, Spring};
use super::storyboard::{CameraMove, Length, OrbitCenter, Scene, Speed};
use bevy::prelude::*;
use client::battle_pipeline::session::BattleSession;
use client::{LabState, RtsCamera};
use sim_core::SimWorld;
use sim_types::{EntityId, SimScalar};
use std::collections::{BTreeMap, VecDeque};

/// Ticks of game time over which [`CameraMove::FollowCombat`] averages the damage positions (one
/// second at 25 ticks per second).
pub const COMBAT_WINDOW_TICKS: u32 = sim_types::TICKS_PER_SECOND as u32;

/// Radius of one fight for [`CombatTracker::centroid_near`], world units.
pub const COMBAT_CLUSTER_RADIUS: f32 = 10.0;

/// Yaw of the showcase camera in radians, applied to the camera transform after the production
/// camera system, so an orbit can turn the view. 0 keeps the production view.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraYaw(pub f32);

/// Failure of the director.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DirectionError {
    /// An `UntilCombat` scene reached its maximum without hostile damage.
    #[error("the storyboard expects combat by frame {max} of scene `{scene}`")]
    NoCombat {
        /// Scene.
        scene: String,
        /// Its maximum length.
        max: u32,
    },
    /// A follow move found nothing to follow.
    #[error("scene `{scene}` has nothing to follow: {reason}")]
    NothingToFollow {
        /// Scene.
        scene: String,
        /// What was missing.
        reason: String,
    },
    /// The production camera or battle is missing.
    #[error("the production {0} is missing")]
    Missing(&'static str),
}

/// What the end of a recorded frame means for the take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameOutcome {
    /// The scene continues.
    Continue,
    /// The scene ended; the next frame starts the next scene.
    NextScene,
    /// The last scene ended; the take is complete.
    Finished,
}

/// Which scene and which frame of it the take is at.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SceneClock {
    /// Index of the current scene.
    pub scene: usize,
    /// Frame within the current scene.
    pub scene_frame: u32,
    /// Frame within the take.
    pub frame: u64,
}

impl SceneClock {
    /// Ends the current frame of `scenes[self.scene]`. `combat` says whether the frame's ticks
    /// contained hostile damage.
    ///
    /// # Errors
    /// [`DirectionError::NoCombat`] when an `UntilCombat` scene reaches `max` frames without combat.
    pub fn end_frame(
        &mut self,
        scenes: &[Scene],
        combat: bool,
    ) -> Result<FrameOutcome, DirectionError> {
        let scene = &scenes[self.scene];
        let played = self.scene_frame + 1;
        let ends = match scene.length {
            Length::Frames(frames) => played >= frames,
            Length::UntilCombat { min, max } => {
                if combat && played >= min {
                    true
                } else if played >= max {
                    return Err(DirectionError::NoCombat {
                        scene: scene.name.clone(),
                        max,
                    });
                } else {
                    false
                }
            }
        };
        self.frame += 1;
        if !ends {
            self.scene_frame = played;
            return Ok(FrameOutcome::Continue);
        }
        self.scene_frame = 0;
        if self.scene + 1 == scenes.len() {
            return Ok(FrameOutcome::Finished);
        }
        self.scene += 1;
        Ok(FrameOutcome::NextScene)
    }
}

/// Observes hostile damage in the authoritative world, frame by frame.
///
/// Damage is hostile when a unit's hull or shields dropped since the previous observation while a
/// living unit of another owner targets it, the correlation the capture tool uses.
#[derive(Debug, Clone, Default)]
pub struct CombatTracker {
    prior: BTreeMap<EntityId, (SimScalar, SimScalar)>,
    recent: VecDeque<(u32, Vec2)>,
}

impl CombatTracker {
    /// Records `world`'s state; returns whether hostile damage happened since the last call.
    pub fn observe(&mut self, world: &SimWorld) -> bool {
        let tick = world.current_tick;
        let mut damaged = false;
        for (&id, victim) in &world.entities {
            let hurt = self
                .prior
                .get(&id)
                .is_some_and(|(hp, shields)| victim.hp < *hp || victim.shields < *shields);
            if hurt
                && world.entities.values().any(|source| {
                    source.current_target == Some(id)
                        && source.owner != victim.owner
                        && source.is_alive()
                })
            {
                damaged = true;
                self.recent.push_back((
                    tick,
                    Vec2::new(victim.position.x.to_f32(), victim.position.y.to_f32()),
                ));
            }
        }
        self.prior.clear();
        for (&id, entity) in &world.entities {
            self.prior.insert(id, (entity.hp, entity.shields));
        }
        while self
            .recent
            .front()
            .is_some_and(|(at, _)| at.saturating_add(COMBAT_WINDOW_TICKS) <= tick)
        {
            self.recent.pop_front();
        }
        damaged
    }

    /// The centroid of the damage positions of the last [`COMBAT_WINDOW_TICKS`] ticks.
    #[must_use]
    pub fn centroid(&self) -> Option<Vec2> {
        mean(self.recent.iter().map(|(_, position)| *position))
    }

    /// The centroid of one fight: the recent damage positions within [`COMBAT_CLUSTER_RADIUS`] of
    /// the damage position nearest `near`. Two fights far apart (each army raiding the other's
    /// base) would otherwise average to empty ground between them.
    #[must_use]
    pub fn centroid_near(&self, near: Vec2) -> Option<Vec2> {
        let anchor = self
            .recent
            .iter()
            .map(|(_, position)| *position)
            .min_by(|left, right| {
                left.distance_squared(near)
                    .total_cmp(&right.distance_squared(near))
            })?;
        mean(
            self.recent
                .iter()
                .map(|(_, position)| *position)
                .filter(|position| position.distance(anchor) <= COMBAT_CLUSTER_RADIUS),
        )
    }
}

fn mean(points: impl Iterator<Item = Vec2>) -> Option<Vec2> {
    let (sum, count) = points.fold((Vec2::ZERO, 0_u32), |(sum, count), point| {
        (sum + point, count + 1)
    });
    #[expect(clippy::cast_precision_loss, reason = "unit counts are far below 2^24")]
    (count > 0).then(|| sum / count as f32)
}

/// The centroid of `player`'s living mobile units.
#[must_use]
pub fn army_centroid(world: &SimWorld, player: u8) -> Option<Vec2> {
    mean(
        world
            .entities
            .values()
            .filter(|entity| entity.owner.0 == player && !entity.is_immobile && entity.is_alive())
            .map(|entity| Vec2::new(entity.position.x.to_f32(), entity.position.y.to_f32())),
    )
}

/// The living unit with the lowest identifier of the first type in `slugs` that has one.
fn followed_unit(world: &SimWorld, player: u8, slugs: &[String]) -> Option<Vec2> {
    slugs.iter().find_map(|slug| {
        let unit_type = world.catalog.id_by_slug(slug)?;
        world
            .entities
            .values()
            .find(|entity| {
                entity.owner.0 == player && entity.unit_type_id == unit_type && entity.is_alive()
            })
            .map(|entity| Vec2::new(entity.position.x.to_f32(), entity.position.y.to_f32()))
    })
}

/// Pose of a camera move given its focus, in the file's degrees.
fn aimed(focus: Vec2, distance: f32, pitch_deg: f32) -> Pose {
    Pose {
        focus,
        distance,
        pitch: pitch_deg.to_radians(),
        yaw: 0.0,
    }
}

/// The director's camera state across frames.
#[derive(Debug, Clone, Default)]
pub struct CameraDirector {
    /// The pose of the previous frame.
    pub last: Option<Pose>,
    /// The pose at the end of the previous scene.
    pub scene_start: Option<Pose>,
    spring: Option<Spring>,
}

impl CameraDirector {
    /// Marks the start of a new scene: its blend or spring starts from the last pose.
    pub fn start_scene(&mut self) {
        self.scene_start = self.last;
        self.spring = None;
    }

    /// The pose of `scene` at `scene_frame`, given the battle `world` and the combat history.
    ///
    /// # Errors
    /// [`DirectionError::NothingToFollow`] when a follow move has no subject and no previous pose.
    pub fn pose(
        &mut self,
        scene: &Scene,
        scene_frame: u32,
        world: &SimWorld,
        combat: &CombatTracker,
    ) -> Result<Pose, DirectionError> {
        let target = self.target(scene, scene_frame, world, combat)?;
        let follows = matches!(
            scene.camera,
            CameraMove::FollowArmy { .. }
                | CameraMove::FollowCombat { .. }
                | CameraMove::FollowUnit { .. }
        );
        let start = if scene.cut { None } else { self.scene_start };
        let pose = if follows {
            let spring = self
                .spring
                .get_or_insert_with(|| Spring::at_rest(start.unwrap_or(target)));
            spring.step(target)
        } else {
            match start {
                Some(from) if scene_frame < pose::BLEND_FRAMES => {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "blend frames are a small constant"
                    )]
                    let t = scene_frame as f32 / pose::BLEND_FRAMES as f32;
                    pose::mix(from, target, pose::smoothstep(t))
                }
                _ => target,
            }
        };
        self.last = Some(pose);
        Ok(pose)
    }

    /// The fight nearest the previous focus, or the midpoint of the first two armies without one.
    fn combat_focus(&self, world: &SimWorld, combat: &CombatTracker) -> Option<Vec2> {
        let fallback = army_centroid(world, 0)
            .zip(army_centroid(world, 1))
            .map(|(first, second)| (first + second) * 0.5);
        self.last
            .map_or_else(
                || combat.centroid(),
                |pose| combat.centroid_near(pose.focus),
            )
            .or(fallback)
    }

    /// Where the scene's camera move aims at `scene_frame`, before blending and smoothing.
    fn target(
        &self,
        scene: &Scene,
        scene_frame: u32,
        world: &SimWorld,
        combat: &CombatTracker,
    ) -> Result<Pose, DirectionError> {
        let follow = |focus: Option<Vec2>, what: &str| {
            focus.or(self.last.map(|pose| pose.focus)).ok_or_else(|| {
                DirectionError::NothingToFollow {
                    scene: scene.name.clone(),
                    reason: what.to_owned(),
                }
            })
        };
        Ok(match &scene.camera {
            CameraMove::Fixed {
                focus,
                distance,
                pitch_deg,
            } => aimed(Vec2::new(focus.0, focus.1), *distance, *pitch_deg),
            CameraMove::Path { keys } => {
                let keys: Vec<Key> = keys
                    .iter()
                    .map(|key| Key {
                        frame: key.frame,
                        pose: Pose {
                            yaw: key.yaw_deg.to_radians(),
                            ..aimed(
                                Vec2::new(key.focus.0, key.focus.1),
                                key.distance,
                                key.pitch_deg,
                            )
                        },
                    })
                    .collect();
                pose::path_pose(&keys, scene_frame).ok_or(DirectionError::NothingToFollow {
                    scene: scene.name.clone(),
                    reason: "the path has no key".to_owned(),
                })?
            }
            CameraMove::Orbit {
                center,
                distance,
                pitch_deg,
                start_deg,
                sweep_deg,
            } => {
                let center = match center {
                    OrbitCenter::At(point) => Vec2::new(point.0, point.1),
                    OrbitCenter::Current => {
                        follow(self.scene_start.map(|pose| pose.focus), "no previous pose")?
                    }
                };
                pose::orbit_pose(
                    center,
                    *distance,
                    pitch_deg.to_radians(),
                    start_deg.to_radians(),
                    sweep_deg.to_radians(),
                    scene_frame,
                    scene.length.max_frames(),
                )
            }
            CameraMove::FollowArmy {
                player,
                distance,
                pitch_deg,
            } => aimed(
                follow(army_centroid(world, *player), "the army is gone")?,
                *distance,
                *pitch_deg,
            ),
            CameraMove::FollowCombat {
                distance,
                pitch_deg,
            } => aimed(
                follow(self.combat_focus(world, combat), "no combat and no armies")?,
                *distance,
                *pitch_deg,
            ),
            CameraMove::FollowUnit {
                player,
                unit_types,
                distance,
                pitch_deg,
            } => {
                let focus = followed_unit(world, *player, unit_types).ok_or_else(|| {
                    DirectionError::NothingToFollow {
                        scene: scene.name.clone(),
                        reason: format!("no living unit of {unit_types:?}"),
                    }
                })?;
                aimed(focus, *distance, *pitch_deg)
            }
        })
    }
}

/// Applies the scene's speed and clears the window's input; the battle is driven only by the
/// scenario's own orders.
pub fn apply_speed(world: &mut World, speed: Speed) {
    let mut lab = world.resource_mut::<LabState>();
    match speed {
        Speed::Paused => lab.battle_paused = true,
        Speed::Speed(ticks) => {
            lab.battle_paused = false;
            lab.simulation_speed = ticks;
        }
    }
}

/// Clears keyboard, mouse and wheel input, as the capture tool does.
pub fn clear_input(world: &mut World) {
    world.resource_mut::<ButtonInput<KeyCode>>().reset_all();
    world.resource_mut::<ButtonInput<MouseButton>>().reset_all();
    world
        .resource_mut::<Events<bevy::input::mouse::MouseMotion>>()
        .clear();
    world
        .resource_mut::<Events<bevy::input::mouse::MouseWheel>>()
        .clear();
}

/// Writes `pose` into the production camera and [`CameraYaw`].
///
/// # Errors
/// [`DirectionError::Missing`] without exactly one `RtsCamera`.
pub fn apply_pose(world: &mut World, pose: Pose) -> Result<(), DirectionError> {
    let mut query = world.query::<&mut RtsCamera>();
    let mut camera = query
        .get_single_mut(world)
        .map_err(|_| DirectionError::Missing("RTS camera"))?;
    camera.target = pose.focus;
    camera.distance = pose.distance;
    camera.pitch = pose.pitch;
    world.insert_resource(CameraYaw(pose.yaw));
    Ok(())
}

/// Directs one frame of `scene`: speed, input and camera.
///
/// # Errors
/// [`DirectionError`] when the battle or camera is missing or a follow move has no subject.
pub fn direct_frame(
    world: &mut World,
    scene: &Scene,
    scene_frame: u32,
    camera: &mut CameraDirector,
    combat: &CombatTracker,
) -> Result<(), DirectionError> {
    apply_speed(world, scene.speed);
    clear_input(world);
    let pose = {
        let session = world.resource::<BattleSession>();
        let simulation = session.world().ok_or(DirectionError::Missing("battle"))?;
        camera.pose(scene, scene_frame, simulation, combat)?
    };
    apply_pose(world, pose)
}

/// Replaces the camera transform with the yawed pose of [`CameraYaw`], after the production camera
/// system has eased its look-at height. Runs in `PostUpdate` before transform propagation.
pub fn apply_camera_yaw(
    yaw: Option<Res<'_, CameraYaw>>,
    mut cameras: Query<'_, '_, (&RtsCamera, &mut Transform)>,
) {
    let Some(yaw) = yaw else {
        return;
    };
    for (camera, mut transform) in &mut cameras {
        let rotation = Quat::from_rotation_y(yaw.0) * Quat::from_rotation_x(-camera.pitch);
        let center = Vec3::new(camera.target.x, camera.look_at_y, camera.target.y);
        let eye = center + rotation * Vec3::new(0.0, 0.0, camera.distance);
        *transform = Transform::from_translation(eye).looking_at(center, Vec3::Y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::showcase::storyboard::Overlay;

    fn scene(name: &str, speed: Speed, length: Length) -> Scene {
        Scene {
            name: name.to_owned(),
            speed,
            length,
            camera: CameraMove::FollowArmy {
                player: 0,
                distance: 20.0,
                pitch_deg: 50.0,
            },
            overlay: Overlay::None,
            badge: None,
            cut: false,
        }
    }

    #[test]
    fn combat_centroid_follows_the_fight_nearest_the_camera() {
        let mut tracker = CombatTracker::default();
        for (tick, position) in [
            (1, Vec2::new(8.0, 8.0)),
            (2, Vec2::new(10.0, 8.0)),
            (2, Vec2::new(55.0, 55.0)),
        ] {
            tracker.recent.push_back((tick, position));
        }
        assert_eq!(
            tracker.centroid_near(Vec2::new(12.0, 12.0)),
            Some(Vec2::new(9.0, 8.0))
        );
        assert_eq!(
            tracker.centroid_near(Vec2::new(50.0, 50.0)),
            Some(Vec2::new(55.0, 55.0))
        );
        assert_eq!(CombatTracker::default().centroid_near(Vec2::ZERO), None);
    }

    #[test]
    fn until_combat_fails_loudly_at_max() {
        let scenes = [scene(
            "advance",
            Speed::Speed(4),
            Length::UntilCombat { min: 2, max: 4 },
        )];
        let mut clock = SceneClock::default();
        for _ in 0..3 {
            assert_eq!(clock.end_frame(&scenes, false), Ok(FrameOutcome::Continue));
        }
        assert_eq!(
            clock.end_frame(&scenes, false),
            Err(DirectionError::NoCombat {
                scene: "advance".into(),
                max: 4
            })
        );
        let message = DirectionError::NoCombat {
            scene: "advance".into(),
            max: 4,
        }
        .to_string();
        assert!(message.contains("frame 4 of scene `advance`"), "{message}");
    }

    #[test]
    fn until_combat_waits_for_min_then_ends_on_combat() {
        let scenes = [
            scene(
                "advance",
                Speed::Speed(4),
                Length::UntilCombat { min: 3, max: 9 },
            ),
            scene("clash", Speed::Speed(1), Length::Frames(2)),
        ];
        let mut clock = SceneClock::default();
        assert_eq!(clock.end_frame(&scenes, true), Ok(FrameOutcome::Continue));
        assert_eq!(clock.end_frame(&scenes, false), Ok(FrameOutcome::Continue));
        assert_eq!(clock.end_frame(&scenes, true), Ok(FrameOutcome::NextScene));
        assert_eq!((clock.scene, clock.scene_frame, clock.frame), (1, 0, 3));
        assert_eq!(clock.end_frame(&scenes, false), Ok(FrameOutcome::Continue));
        assert_eq!(clock.end_frame(&scenes, false), Ok(FrameOutcome::Finished));
        assert_eq!(clock.frame, 5);
    }

    #[test]
    fn director_pauses_and_sets_speed_per_scene() -> Result<(), Box<dyn std::error::Error>> {
        let simulation = sim_fixtures::default_battle_sandbox()?;
        let checksum = simulation.compute_checksum();
        let mut world = World::new();
        world.insert_resource(LabState::default());
        world.insert_resource(BattleSession::default().with_world(simulation));
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<ButtonInput<MouseButton>>();
        world.init_resource::<Events<bevy::input::mouse::MouseMotion>>();
        world.init_resource::<Events<bevy::input::mouse::MouseWheel>>();
        world.spawn(RtsCamera::default());
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        let mut camera = CameraDirector::default();
        let combat = CombatTracker::default();
        let paused = scene("title", Speed::Paused, Length::Frames(3));
        direct_frame(&mut world, &paused, 0, &mut camera, &combat)?;
        assert!(world.resource::<LabState>().battle_paused);
        assert!(!world
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        let fast = scene("advance", Speed::Speed(4), Length::Frames(3));
        camera.start_scene();
        direct_frame(&mut world, &fast, 0, &mut camera, &combat)?;
        let lab = world.resource::<LabState>();
        assert!(!lab.battle_paused);
        assert_eq!(lab.simulation_speed, 4);
        let session = world.resource::<BattleSession>();
        let centroid = session
            .world()
            .and_then(|w| army_centroid(w, 0))
            .ok_or("army")?;
        let mut query = world.query::<&RtsCamera>();
        let rts = query.single(&world);
        assert!(
            (rts.target - centroid).length() < 1e-3,
            "{:?} {centroid:?}",
            rts.target
        );
        assert!((rts.pitch - 50.0_f32.to_radians()).abs() < 1e-6);
        // The director changed nothing in the simulation.
        assert_eq!(
            world
                .resource::<BattleSession>()
                .world()
                .map(SimWorld::compute_checksum),
            Some(checksum)
        );
        Ok(())
    }
}
