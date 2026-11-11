//! Authoritative rendering snapshot consumed by the client visualization layer.

use crate::enums::{AnimState, ResourceKind, ShapeKind, SilhouetteClass, WeaponVisualKind};
use crate::ids::{EntityId, PlayerId, ResourceNodeId, Tick, UnitTypeId, MAX_PLAYERS};
use crate::presentation::{
    AreaView, Concealment, EffectSource, LineView, LinkView, SpellMissileView, StanceView,
    StatusFlags,
};
use serde::{Deserialize, Serialize};

/// Comprehensive visual parameters for a single bubble or building dome.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BubbleRenderInstance {
    /// Entity identifier for selection picking and visual persistence.
    pub entity_id: EntityId,
    /// Sphere (unit) or Hemisphere (building).
    pub shape: ShapeKind,
    /// Collar/bracket ornament on the rim.
    pub silhouette: SilhouetteClass,
    /// Unit archetype ID for flipbook video texture array indexing.
    pub unit_type: UnitTypeId,
    /// Actual owning player for tinting; player0 is valid and255 is neutral.
    pub owner: PlayerId,
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
    /// The attack cycle this entity is in, from the start of its current swing until its weapon
    /// is ready again; `None` while no swing is under way. Published whatever `anim_state` says,
    /// so a client can keep timing an attack clip through the cooldown part of the cycle.
    pub attack_swing: Option<AttackSwing>,
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
    /// Progress (0.0..=1.0) of a foundation's construction or, on a finished building, of the
    /// job at the front of its queue. `None` when neither is under way. A foundation is told
    /// from a producing building by `anim_state`, which is `AnimState::Constructing` on it.
    /// A client draws only the foundation case in the world. Production progress belongs in a
    /// panel for the owner, so that the world view cannot show an enemy's queue.
    pub build_or_queue_progress: Option<f32>,
    /// Selection highlight flag (activates double ground ring).
    pub selected: bool,
    /// How the bubble is hidden from players without detection. Whether a given viewer sees
    /// it is [`crate::PlayerView::revealed`]'s business, not this field's.
    ///
    /// Older records carry the boolean key `cloaked`, which reads as this field.
    #[serde(default, alias = "cloaked")]
    pub concealment: Concealment,
    /// Stim/speed boost flag (intensifies wake particle emission).
    pub speed_boosted: bool,
    /// The bubble's current mode and any mode switch under way.
    #[serde(default)]
    pub stance: StanceView,
    /// Status effects active on the bubble.
    #[serde(default)]
    pub status: StatusFlags,
    /// Fraction (0.0..=1.0) of the raised barrier's absorption left; 0.0 while it is down or
    /// for an archetype without one.
    #[serde(default)]
    pub barrier_ratio: f32,
}

/// Where an entity stands in one attack cycle, in whole simulation ticks (25 Hz).
///
/// A cycle runs from the tick a swing's wind-up starts to the earliest tick the next swing can
/// start. It is derived from the weapon state machine, not stored: the simulation keeps no
/// presentation state for it (plan `2026-11-04-battle-flipbook-animation` D-7).
///
/// # Invariants
/// - `elapsed_ticks < period_ticks`, and `period_ticks >= 1`.
/// - The shot is released on the tick where `elapsed_ticks == damage_point_ticks` (on the
///   swing's first tick for a damage point of 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackSwing {
    /// Ticks since the swing began: 0 on the tick its wind-up started.
    pub elapsed_ticks: u16,
    /// Ticks from the swing's start to the release of its shot (the resolved damage point).
    pub damage_point_ticks: u16,
    /// Ticks from one swing's start to the earliest next swing (the resolved attack period,
    /// after upgrades and attack-rate buffs).
    pub period_ticks: u16,
}

/// Instantaneous transient visual combat event (tracers, beams, slashes, explosions).
///
/// Every weapon event carries its [`EffectSource`] and a shatter its unit type, so the client
/// can choose the effect the source unit's presentation block names. Records written before
/// those fields existed read their defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatEventVisual {
    /// Instant bullet/slug tracer line connecting emitter to target.
    HitscanTracer {
        /// Firing aperture origin.
        from: [f32; 3],
        /// Impact position.
        to: [f32; 3],
        /// Actual firing owner, retained after its source disappears.
        owner: PlayerId,
        /// Archetype and weapon that fired.
        #[serde(default)]
        source: EffectSource,
    },
    /// Traveling plasma or kinetic missile bolt.
    ProjectileVisual {
        /// Current world position.
        pos: [f32; 3],
        /// Directional velocity vector.
        vel: [f32; 3],
        /// Actual firing owner, retained after its source disappears.
        owner: PlayerId,
        /// Optional exact duration in seconds before reaching target surface.
        #[serde(default)]
        duration_secs: Option<f32>,
        /// Archetype and weapon that fired.
        #[serde(default)]
        source: EffectSource,
    },
    /// Continuous laser beam or jagged lightning arc.
    BeamVisual {
        /// Beam origin.
        from: [f32; 3],
        /// Beam endpoint.
        to: [f32; 3],
        /// If true, render jagged branching arc; if false, render straight focused laser.
        is_lightning: bool,
        /// Actual firing owner, retained after its source disappears.
        owner: PlayerId,
        /// Archetype and weapon that fired.
        #[serde(default)]
        source: EffectSource,
    },
    /// Close-quarters melee blade shockwave.
    MeleeSlashVisual {
        /// Center of slash arc [x, y, z].
        pos: [f32; 3],
        /// Direction of slash in radians.
        angle_rad: f32,
        /// Radius of slash sweep.
        radius: f32,
        /// Actual attacking owner, independent of color or alliance.
        owner: PlayerId,
        /// Archetype and weapon that fired.
        #[serde(default)]
        source: EffectSource,
    },
    /// Bubble glass fracture and explosive collapse upon entity death.
    BubbleShatter {
        /// Center of shattered bubble.
        pos: [f32; 3],
        /// Original bubble radius.
        radius: f32,
        /// True if building dome, false if unit sphere.
        is_dome: bool,
        /// Actual owner of the destroyed entity, retained for shard tinting.
        owner: PlayerId,
        /// Archetype of the destroyed entity, for its presentation block's death effect.
        #[serde(default)]
        unit_type: UnitTypeId,
    },
    /// One search of a weapon's area damage, emitted on the tick of the search: one event for a
    /// circular splash, one per search point for a line splash.
    SplashVisual {
        /// Centre of the search `[x, y, altitude]` in world units.
        center: [f32; 3],
        /// Radii in world units of the search's zones: innermost (full damage), middle and
        /// outermost (reach). With fewer than three zones the middle repeats the innermost.
        zones: [f32; 3],
        /// The whole line, when the search is one point of a line splash.
        line: Option<LineView>,
        /// Archetype and weapon that caused the splash.
        source: EffectSource,
        /// Actual attacking owner, retained after its source disappears.
        owner: PlayerId,
    },
}

impl CombatEventVisual {
    /// Returns the actual source owner captured when this event was produced.
    /// Player0 is valid;255 denotes neutral. This identity does not imply an alliance.
    #[must_use]
    pub const fn owner(&self) -> PlayerId {
        match self {
            Self::HitscanTracer { owner, .. }
            | Self::ProjectileVisual { owner, .. }
            | Self::BeamVisual { owner, .. }
            | Self::MeleeSlashVisual { owner, .. }
            | Self::BubbleShatter { owner, .. }
            | Self::SplashVisual { owner, .. } => *owner,
        }
    }

    /// The archetype and weapon a weapon event came from; `None` for a shatter, which is no
    /// weapon's.
    #[must_use]
    pub const fn source(&self) -> Option<EffectSource> {
        match self {
            Self::HitscanTracer { source, .. }
            | Self::ProjectileVisual { source, .. }
            | Self::BeamVisual { source, .. }
            | Self::MeleeSlashVisual { source, .. }
            | Self::SplashVisual { source, .. } => Some(*source),
            Self::BubbleShatter { .. } => None,
        }
    }

    /// Returns where this event lands on the map plane, `[x, y]` in world units.
    ///
    /// That is the impact of a tracer or beam, the expected arrival of a timed projectile, and the
    /// position of anything else (a splash's search centre). A client judges whether a viewer may see an event by this point.
    /// Every event position is `[x, y, altitude]`, so the first two components are the map plane.
    /// Derived from visual data only, so it takes no part in the simulation.
    #[must_use]
    pub fn anchor(&self) -> [f32; 2] {
        match self {
            Self::HitscanTracer { to, .. } | Self::BeamVisual { to, .. } => [to[0], to[1]],
            Self::ProjectileVisual {
                pos,
                vel,
                duration_secs,
                ..
            } => {
                let flight = duration_secs.unwrap_or(0.0);
                [pos[0] + vel[0] * flight, pos[1] + vel[1] * flight]
            }
            Self::MeleeSlashVisual { pos, .. }
            | Self::BubbleShatter { pos, .. }
            | Self::SplashVisual { center: pos, .. } => [pos[0], pos[1]],
        }
    }
}

/// Authoritative result of a match.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MatchResult {
    /// Two or more players are still undefeated, or the world has no opposing players.
    #[default]
    InProgress,
    /// One player outlasted every other.
    Victory {
        /// The last undefeated player.
        winner: PlayerId,
        /// Tick the match ended on.
        tick: Tick,
    },
    /// The last undefeated players all lost their last structure on one tick.
    Draw {
        /// Tick the match ended on.
        tick: Tick,
    },
}

impl MatchResult {
    /// Whether the match has ended, after which commands are refused.
    #[must_use]
    pub const fn is_over(self) -> bool {
        !matches!(self, Self::InProgress)
    }
}

/// One player slot's economy and standing, for HUDs and end-of-match screens.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStatusView {
    /// Mineral reserve.
    pub minerals: u32,
    /// Vespene gas reserve.
    pub gas: u32,
    /// Supply in use, including queued reservations.
    pub supply_used: u32,
    /// Effective supply capacity.
    pub supply_max: u32,
    /// Whether this player has lost every structure.
    pub defeated: bool,
}

/// One resource node as a player would pick or render it.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceNodeView {
    /// Identifier a `UnitOrder::Harvest` names.
    pub id: ResourceNodeId,
    /// Mineral or gas.
    pub kind: ResourceKind,
    /// World position of the node centre: `[x, y]`.
    pub position: [f32; 2],
    /// Units left to gather; zero marks an exhausted node.
    pub remaining: u32,
}

/// Complete, immutable snapshot of a single simulation frame passed to the renderer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RenderSnapshot {
    /// Current tick index.
    pub tick: Tick,
    /// State checksum of this frame (for HUD display and desync logging).
    pub checksum: u64,
    /// All active unit bubbles and building domes.
    pub bubbles: Vec<BubbleRenderInstance>,
    /// Visual combat events triggered during this tick.
    pub combat_events: Vec<CombatEventVisual>,
    /// Authoritative match result as of this frame.
    #[serde(default)]
    pub match_result: MatchResult,
    /// Economy and standing, indexed by [`crate::PlayerSlot::index`].
    #[serde(default)]
    pub players: [PlayerStatusView; MAX_PLAYERS],
    /// Every map resource node in identifier order, including exhausted ones.
    #[serde(default)]
    pub resource_nodes: Vec<ResourceNodeView>,
    /// Persistent areas on the ground (searching effect persistents), by area ID.
    #[serde(default)]
    pub areas: Vec<AreaView>,
    /// Tethers between entities; empty until a mechanic makes one.
    #[serde(default)]
    pub links: Vec<LinkView>,
    /// Spell missiles in flight presented as projectiles; empty when there are none, and then
    /// left out of the serialized frame so frames without spells keep their bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spell_missiles: Vec<SpellMissileView>,
}

impl RenderSnapshot {
    /// Copies the frame-wide match, economy and resource state of `source` into this frame.
    ///
    /// Presentation frames assemble their records themselves and use this for the rest. The
    /// node list reuses this frame's storage, so a warmed frame allocates nothing.
    pub fn copy_match_status_from(&mut self, source: &Self) {
        self.match_result = source.match_result;
        self.players = source.players;
        self.resource_nodes.clone_from(&source.resource_nodes);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::presentation::{WeaponKey, WeaponSlot};

    const OWNER: PlayerId = PlayerId(1);
    const SOURCE: EffectSource = EffectSource {
        unit_type: UnitTypeId(7),
        weapon: WeaponKey {
            slot: WeaponSlot::Ground,
            mode: Some(1),
        },
    };

    #[test]
    fn a_tracer_and_a_beam_are_anchored_at_their_impact() {
        let tracer = CombatEventVisual::HitscanTracer {
            from: [1.0, 2.0, 3.0],
            to: [4.0, 5.0, 6.0],
            owner: OWNER,
            source: SOURCE,
        };
        let beam = CombatEventVisual::BeamVisual {
            from: [1.0, 2.0, 3.0],
            to: [7.0, 8.0, 9.0],
            is_lightning: true,
            owner: OWNER,
            source: SOURCE,
        };
        assert_eq!(tracer.anchor(), [4.0, 5.0]);
        assert_eq!(beam.anchor(), [7.0, 8.0]);
    }

    #[test]
    fn a_melee_slash_and_a_shatter_are_anchored_where_they_happen() {
        let slash = CombatEventVisual::MeleeSlashVisual {
            pos: [4.0, 5.0, 6.0],
            angle_rad: 1.0,
            radius: 2.0,
            owner: OWNER,
            source: SOURCE,
        };
        let shatter = CombatEventVisual::BubbleShatter {
            pos: [7.0, 8.0, 9.0],
            radius: 1.0,
            is_dome: true,
            owner: OWNER,
            unit_type: UnitTypeId(3),
        };
        assert_eq!(slash.anchor(), [4.0, 5.0]);
        assert_eq!(shatter.anchor(), [7.0, 8.0]);
        assert_eq!(slash.source(), Some(SOURCE));
        assert_eq!(shatter.source(), None, "a shatter is no weapon's");
    }

    #[test]
    fn a_splash_is_anchored_at_its_centre() {
        let splash = CombatEventVisual::SplashVisual {
            center: [4.5, 6.5, 1.0],
            zones: [0.5, 0.75, 1.25],
            line: Some(LineView {
                from: [0.0, 0.0, 0.0],
                to: [9.0, 0.0, 0.0],
                spacing: 1.0,
                index: 4,
                count: 9,
            }),
            source: SOURCE,
            owner: OWNER,
        };
        assert_eq!(splash.anchor(), [4.5, 6.5]);
        assert_eq!(splash.owner(), OWNER);
        assert_eq!(splash.source(), Some(SOURCE));
    }

    #[test]
    fn a_timed_projectile_is_anchored_where_it_arrives() {
        let shot = |duration_secs| CombatEventVisual::ProjectileVisual {
            pos: [1.0, 2.0, 3.0],
            vel: [4.0, -2.0, 8.0],
            owner: OWNER,
            duration_secs,
            source: SOURCE,
        };
        assert_eq!(shot(Some(0.5)).anchor(), [3.0, 1.0]);
        assert_eq!(
            shot(None).anchor(),
            [1.0, 2.0],
            "with no flight time, only its current position is known"
        );
    }

    #[test]
    fn test_snapshot_roundtrip() {
        let instance = BubbleRenderInstance {
            entity_id: EntityId(1),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::LightProngs,
            unit_type: UnitTypeId(1),
            owner: PlayerId(1),
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
            attack_swing: Some(AttackSwing {
                elapsed_ticks: 4,
                damage_point_ticks: 2,
                period_ticks: 15,
            }),
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: true,
            concealment: Concealment::Burrowed,
            speed_boosted: false,
            stance: StanceView::default(),
            status: StatusFlags::default(),
            barrier_ratio: 0.0,
        };

        let event = CombatEventVisual::HitscanTracer {
            from: [10.0, 20.0, 0.5],
            to: [15.0, 20.0, 0.5],
            owner: PlayerId(1),
            source: SOURCE,
        };

        let snapshot = RenderSnapshot {
            tick: Tick(42),
            checksum: 0x1234_5678_9ABC_DEF0,
            bubbles: vec![instance],
            combat_events: vec![event],
            match_result: MatchResult::Victory {
                winner: PlayerId(1),
                tick: Tick(40),
            },
            players: [PlayerStatusView::default(); MAX_PLAYERS],
            resource_nodes: vec![ResourceNodeView {
                id: ResourceNodeId(3),
                kind: ResourceKind::Gas,
                position: [4.5, 6.5],
                remaining: 25,
            }],
            ..RenderSnapshot::default()
        };

        let ron_str = ron::to_string(&snapshot).expect("invariant: valid ron serialization");
        let decoded: RenderSnapshot =
            ron::from_str(&ron_str).expect("invariant: valid ron deserialization");
        assert_eq!(snapshot, decoded);
    }
}
