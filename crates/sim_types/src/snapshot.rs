//! Authoritative rendering snapshot consumed by the client visualization layer.

use crate::enums::{AnimState, ShapeKind, SilhouetteClass, WeaponVisualKind};
use crate::ids::{EntityId, TeamId, Tick, UnitTypeId};
use serde::{Deserialize, Serialize};

/// Comprehensive visual parameters for a single bubble or building dome.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BubbleRenderInstance {
    /// Entity identifier for selection picking and visual persistence.
    pub entity_id: EntityId,
    /// Sphere (unit) or Hemisphere (building).
    pub shape: ShapeKind,
    /// Collar/bracket ornament on the rim.
    pub silhouette: SilhouetteClass,
    /// Unit archetype ID for flipbook video texture array indexing.
    pub unit_type: UnitTypeId,
    /// Owning team for rim Fresnel and ground ring tinting.
    pub team_id: TeamId,
    /// Center coordinates in world space: [x, `ground_y`, `altitude_z`].
    pub world_pos: [f32; 3],
    /// Height of terrain directly below bubble (for air unit tether/shadow projection).
    pub ground_z: f32,
    /// Outer glass shell radius in world units.
    pub radius: f32,
    /// Current velocity [vx, vy] driving trailing mist wake particles.
    pub velocity: [f32; 2],
    /// Facing orientation angle in radians [-π, π], driving rim chevron and video flip.
    pub heading_angle_rad: f32,
    /// Weapon aim direction in radians, positioning the Attack Focal Lens on the rim.
    pub aim_angle_rad: f32,
    /// Whether weapon is currently in active firing/windup state.
    pub is_attacking: bool,
    /// Visual style of weapon attack.
    pub weapon_visual: WeaponVisualKind,
    /// World position of current attack target, if any.
    pub attack_target_pos: Option<[f32; 3]>,
    /// High-level animation cycle to display on the internal video core.
    pub anim_state: AnimState,
    /// Normalized animation progress [0.0, 1.0] within the current loop.
    pub anim_progress: f32,
    /// Health ratio (0.0..=1.0) driving spiderweb glass cracks and CRT glitch shader.
    pub hp_ratio: f32,
    /// Shield energy ratio (0.0..=1.0) driving outer energy sheen.
    pub shield_ratio: f32,
    /// Decaying ripple intensity (0.0..=1.0) at shield impact point.
    pub shield_hit_flash: f32,
    /// Direction the last hull hit originated from in radians.
    pub hull_hit_angle_rad: f32,
    /// Decaying impact flare intensity (0.0..=1.0) at hull impact point.
    pub hull_hit_flash: f32,
    /// Construction/production progress for building domes (radial ground ring fill).
    pub build_or_queue_progress: Option<f32>,
    /// Selection highlight flag (activates double ground ring).
    pub selected: bool,
    /// Stealth cloaking flag (translucent refractive shimmer).
    pub cloaked: bool,
    /// Stim/speed boost flag (intensifies wake particle emission).
    pub speed_boosted: bool,
}

/// Instantaneous transient visual combat event (tracers, beams, slashes, explosions).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CombatEventVisual {
    /// Instant bullet/slug tracer line connecting emitter to target.
    HitscanTracer {
        /// Firing aperture origin.
        from: [f32; 3],
        /// Impact position.
        to: [f32; 3],
        /// Firing team for tracer color tint.
        team_id: TeamId,
    },
    /// Traveling plasma or kinetic missile bolt.
    ProjectileVisual {
        /// Current world position.
        pos: [f32; 3],
        /// Directional velocity vector.
        vel: [f32; 3],
        /// Firing team.
        team_id: TeamId,
        /// Optional exact duration in seconds before reaching target surface.
        #[serde(default)]
        duration_secs: Option<f32>,
    },
    /// Continuous laser beam or jagged lightning arc.
    BeamVisual {
        /// Beam origin.
        from: [f32; 3],
        /// Beam endpoint.
        to: [f32; 3],
        /// If true, render jagged branching arc; if false, render straight focused laser.
        is_lightning: bool,
        /// Firing team.
        team_id: TeamId,
    },
    /// Close-quarters melee blade shockwave.
    MeleeSlashVisual {
        /// Center of slash arc [x, y, z].
        pos: [f32; 3],
        /// Direction of slash in radians.
        angle_rad: f32,
        /// Radius of slash sweep.
        radius: f32,
        /// Attacking team for visual color tinting.
        team_id: TeamId,
    },
    /// Bubble glass fracture and explosive collapse upon entity death.
    BubbleShatter {
        /// Center of shattered bubble.
        pos: [f32; 3],
        /// Original bubble radius.
        radius: f32,
        /// True if building dome, false if unit sphere.
        is_dome: bool,
        /// Owning team for shard tinting.
        team_id: TeamId,
    },
}

/// Complete, immutable snapshot of a single simulation frame passed to the renderer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderSnapshot {
    /// Current tick index.
    pub tick: Tick,
    /// State checksum of this frame (for HUD display and desync logging).
    pub checksum: u64,
    /// All active unit bubbles and building domes.
    pub bubbles: Vec<BubbleRenderInstance>,
    /// Visual combat events triggered during this tick.
    pub combat_events: Vec<CombatEventVisual>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_roundtrip() {
        let instance = BubbleRenderInstance {
            entity_id: EntityId(1),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::LightProngs,
            unit_type: UnitTypeId(1),
            team_id: TeamId(1),
            world_pos: [10.0, 20.0, 0.0],
            ground_z: 0.0,
            radius: 0.5,
            velocity: [1.0, 0.0],
            heading_angle_rad: 1.57,
            aim_angle_rad: 1.57,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: AnimState::Idle,
            anim_progress: 0.0,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: true,
            cloaked: false,
            speed_boosted: false,
        };

        let event = CombatEventVisual::HitscanTracer {
            from: [10.0, 20.0, 0.5],
            to: [15.0, 20.0, 0.5],
            team_id: TeamId(1),
        };

        let snapshot = RenderSnapshot {
            tick: Tick(42),
            checksum: 0x1234_5678_9ABC_DEF0,
            bubbles: vec![instance],
            combat_events: vec![event],
        };

        let ron_str = ron::to_string(&snapshot).expect("invariant: valid ron serialization");
        let decoded: RenderSnapshot =
            ron::from_str(&ron_str).expect("invariant: valid ron deserialization");
        assert_eq!(snapshot, decoded);
    }
}
