//! Where an instant or melee hit shows: on the struck unit's own bubble, towards its attacker
//! (plan `2026-11-09-effect-visualization` WP11, reviews 2 to 4: A2-1, A3-1, A3-2, A4-1, E3-5;
//! plan `2026-11-17-effect-snapshot-identity` D-3 and Amendment 2, B-125).
//!
//! # Responsibility
//! The simulation publishes an instant hit (`HitscanTracer`, `BeamVisual`) and a melee strike
//! (`MeleeSlashVisual`) at the struck unit's map position, half a unit above the ground, and
//! names the struck entity in the event's [`StrikeEnds`]. That point lies inside a ground
//! unit's bubble, whose glass is drawn over anything there, and far below a flyer's. This
//! module moves it onto the struck bubble's glass, where the hit shows and where the shot's
//! slug ends:
//! - [`StruckBubbles::find`] finds the struck bubble **by the struck entity's id**, never by
//!   position: in the presented frame, else in the previous snapshot, else among the bubbles
//!   presented on the frame before this one, else nowhere. The third place serves a killing
//!   hit: its target is reaped on the hit's tick, and the hit is shown once the presentation
//!   reaches that tick, when usually neither snapshot it holds lists the target any more.
//! - [`at_struck_surface`] returns the event with its hit point on the struck bubble's surface.
//!   The horizontal position is the published point (the struck unit's centre on the hit's
//!   tick); the radius, drawn height and dome are the found bubble's. The point is moved
//!   towards the attacker, raised [`HIT_ELEVATION`] above the bubble's drawn centre (so a hit on
//!   the side of the bubble away from the camera is still in front of its glass: the camera
//!   looks down at about 58 degrees, and a point 45 degrees up the far side faces it), at the
//!   bubble's radius plus [`SURFACE_MARGIN`].
//!
//! # How it fits
//! `effects::systems::spawn_effects_from_events` passes every admitted event through
//! [`at_struck_surface`] before it asks for the event's travel and impact, so a slug, a beam,
//! its burn and a blow all meet the struck bubble's glass, a flyer's included, whatever stands
//! below or beside it. Presentation only: the simulation's events and state are read, never
//! changed (D-1). The fog is not consulted here: the event was admitted by its published point
//! (`BattleFog::reveals`), and the bubble's outline shows no more than that point shows.
//!
//! # Invariants
//! - An event without [`StrikeEnds`] (a synthetic event, a record older than the field), or
//!   whose struck bubble is found nowhere, is moved as if it struck a bubble of
//!   [`DEFAULT_STRUCK_RADIUS`] centred on its published point, the radius its impact binds, so a
//!   look placed `Behind` the hit still lands on that bubble's centre (review 3, E3-5).
//! - A shot's hit point never moves past [`MOST_PULL`] of the way back to its muzzle.
//! - Every lookup is a binary search over an id-ordered slice: the snapshot writes its bubbles
//!   in entity order and the presentation keeps that order. The one retained buffer, the last
//!   frame's bubbles, is refilled in place, so steady state allocates nothing.

use bevy::prelude::*;
use sim_types::snapshot::{BubbleRenderInstance, CombatEventVisual};
use sim_types::{EntityId, ShapeKind, StrikeEnds};

/// World units beyond the struck bubble's radius at which its hit shows, so it is drawn in
/// front of the glass rather than on it.
pub const SURFACE_MARGIN: f32 = 0.12;
/// Largest share of a shot's length its hit point is moved back towards its muzzle.
pub const MOST_PULL: f32 = 0.8;
/// The struck bubble's radius an impact binds (`UnitRadius`) when the bubble is not found.
pub const DEFAULT_STRUCK_RADIUS: f32 = 0.5;
/// Height above a dome's base at which a hit on its rim shows, so it is not sunk in the floor.
pub const DOME_RIM_LIFT: f32 = 0.1;
/// How far up the struck bubble, from its drawn centre, its hit shows: 45 degrees, radians.
pub const HIT_ELEVATION: f32 = std::f32::consts::FRAC_PI_4;

/// The bubble a hit struck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StruckBubble {
    /// Its glass radius, world units.
    pub radius: f32,
    /// Height of its drawn centre, world units (the event's height convention); a dome's base.
    pub height: f32,
    /// Whether it is a structure's dome: a half sphere standing on `height`, with nothing of
    /// it below (review 4, A4-1).
    pub dome: bool,
}

impl StruckBubble {
    /// The struck bubble a presented `bubble` is: its radius, its drawn centre's height (a unit
    /// sphere's centre is its radius above where it stands, a dome's is its base) and whether
    /// it is a dome.
    #[must_use]
    pub fn of(bubble: &BubbleRenderInstance) -> Self {
        let dome = matches!(bubble.shape, ShapeKind::BuildingDome { .. });
        Self {
            radius: bubble.radius,
            height: bubble.world_pos[2] + if dome { 0.0 } else { bubble.radius },
            dome,
        }
    }
}

/// The struck bubble of entity `id` among `bubbles`, which are in entity id order (a
/// snapshot's, or a presented frame's), by binary search.
#[must_use]
pub fn struck_by_id(bubbles: &[BubbleRenderInstance], id: EntityId) -> Option<StruckBubble> {
    bubbles
        .binary_search_by_key(&id, |bubble| bubble.entity_id)
        .ok()
        .and_then(|index| bubbles.get(index))
        .map(StruckBubble::of)
}

/// Where [`StruckBubbles::find`] found a struck bubble, in the order it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StruckFrom {
    /// The presented frame.
    Presented,
    /// The previous snapshot of the presentation (`BattleFrameState::previous`).
    Previous,
    /// The bubbles presented on the frame before this one.
    LastFrame,
    /// Nowhere: the default bubble is used.
    Nowhere,
}

impl StruckFrom {
    /// Every place, in the order [`StruckBubbles::find`] looks.
    pub const ALL: [Self; 4] = [
        Self::Presented,
        Self::Previous,
        Self::LastFrame,
        Self::Nowhere,
    ];

    /// Index of this place in [`Self::ALL`].
    const fn index(self) -> usize {
        match self {
            Self::Presented => 0,
            Self::Previous => 1,
            Self::LastFrame => 2,
            Self::Nowhere => 3,
        }
    }
}

/// The bubbles a hit's struck entity is looked up among beyond the presented frame (those
/// presented on the frame before this one, retained), and a count of where each lookup found
/// its bubble, for the steady-state tests and the evidence.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct StruckBubbles {
    /// The last presented frame's bubbles: entity id and struck bubble, in entity id order.
    last: Vec<(EntityId, StruckBubble)>,
    /// Lookups per [`StruckFrom`], in the order of [`StruckFrom::ALL`].
    found: [u64; 4],
}

impl StruckBubbles {
    /// The struck bubble of entity `id`: in `presented` (the presented frame), else in
    /// `previous` (the previous snapshot, when the presentation holds one), else among the
    /// bubbles last given to [`Self::remember`], else nowhere. Each slice is in entity id
    /// order. Counts where it was found.
    pub fn find(
        &mut self,
        presented: &[BubbleRenderInstance],
        previous: Option<&[BubbleRenderInstance]>,
        id: EntityId,
    ) -> Option<StruckBubble> {
        let (found, from) = if let Some(found) = struck_by_id(presented, id) {
            (Some(found), StruckFrom::Presented)
        } else if let Some(found) = previous.and_then(|previous| struck_by_id(previous, id)) {
            (Some(found), StruckFrom::Previous)
        } else if let Some((_, found)) = self
            .last
            .binary_search_by_key(&id, |(entity, _)| *entity)
            .ok()
            .and_then(|index| self.last.get(index))
        {
            (Some(*found), StruckFrom::LastFrame)
        } else {
            (None, StruckFrom::Nowhere)
        };
        self.found[from.index()] += 1;
        found
    }

    /// Keeps `presented` (this frame's presented bubbles, in entity id order) as the bubbles a
    /// later frame's hit may have struck; call once a frame, after this frame's lookups.
    pub fn remember(&mut self, presented: &[BubbleRenderInstance]) {
        self.last.clear();
        self.last.extend(
            presented
                .iter()
                .map(|bubble| (bubble.entity_id, StruckBubble::of(bubble))),
        );
    }

    /// How many lookups found their bubble in `from`.
    #[must_use]
    pub const fn found_in(&self, from: StruckFrom) -> u64 {
        self.found[from.index()]
    }

    /// Address and capacity of the retained buffer, which an allocation would move or grow.
    #[must_use]
    pub fn buffer(&self) -> (usize, usize) {
        (self.last.as_ptr() as usize, self.last.capacity())
    }
}

/// The attacker and the struck entity of an instant hit or a blow, when the event names them.
#[must_use]
pub const fn strike_ends(event: &CombatEventVisual) -> Option<StrikeEnds> {
    match event {
        CombatEventVisual::HitscanTracer { ends, .. }
        | CombatEventVisual::BeamVisual { ends, .. }
        | CombatEventVisual::MeleeSlashVisual { ends, .. } => *ends,
        CombatEventVisual::ProjectileVisual { .. }
        | CombatEventVisual::BubbleShatter { .. }
        | CombatEventVisual::SplashVisual { .. }
        | CombatEventVisual::SpellImpact { .. } => None,
    }
}

/// `point` (map `x, y`, height) moved onto the glass of a bubble of `struck` centred on it in
/// plan: towards `towards` (a unit map direction) and [`HIT_ELEVATION`] up from the bubble's
/// drawn centre, at its radius plus [`SURFACE_MARGIN`], the horizontal move at most `most`.
fn onto_glass(point: &mut [f32; 3], struck: StruckBubble, towards: Vec2, most: f32) {
    let reach = struck.radius + SURFACE_MARGIN;
    let across = (reach * HIT_ELEVATION.cos()).min(most);
    point[0] += towards.x * across;
    point[1] += towards.y * across;
    point[2] = struck.height + reach * HIT_ELEVATION.sin();
}

/// `point` (map `x, y`, height) moved onto the outline of a bubble of `struck` centred on it in
/// plan, as the camera looking along `view` (Bevy world, unit) sees it: the point of the
/// bubble's silhouette, at its radius plus [`SURFACE_MARGIN`], on the side of `towards` (a unit
/// map direction) on screen, so a shot's end, its hit and a blow are drawn at the edge of the
/// glass the attack comes from, never in front of the bubble's face or behind it (review 3,
/// A3-2). On a dome, which has nothing below its base, a point that would fall below the base
/// is put on the base rim instead (review 4, A4-1). The horizontal move is at most `most`.
/// Falls back to [`onto_glass`] when `towards` runs along the view.
fn onto_outline(point: &mut [f32; 3], struck: StruckBubble, towards: Vec2, most: f32, view: Vec3) {
    let across = Vec3::new(towards.x, 0.0, towards.y);
    let side = across - view * across.dot(view);
    if side.length_squared() < 1e-6 {
        onto_glass(point, struck, towards, most);
        return;
    }
    let reach = struck.radius + SURFACE_MARGIN;
    let mut offset = side.normalize() * reach;
    // A dome has nothing below its base: on the side towards the camera its visible outline is
    // its base rim, so the point stays there, a little above the floor (review 4, A4-1).
    if struck.dome && offset.y < DOME_RIM_LIFT {
        let flat = Vec2::new(offset.x, offset.z).normalize_or(towards) * reach;
        offset = Vec3::new(flat.x, DOME_RIM_LIFT, flat.y);
    }
    let flat = offset.x.hypot(offset.z);
    if flat > most {
        let shrink = most / flat;
        offset.x *= shrink;
        offset.z *= shrink;
    }
    point[0] += offset.x;
    point[1] += offset.z;
    point[2] = struck.height + offset.y;
}

/// `event` with its hit point on the struck bubble's glass: a tracer's or a beam's `to` moved
/// back along the shot, a melee strike's `pos` along its published `angle_rad` (the direction
/// from the struck unit towards its attacker), and raised onto the bubble (see
/// [`onto_glass`]; onto the bubble's outline as the camera looking along `view` sees it, when
/// there is one, see [`onto_outline`]). `struck_by_id(id)` gives the bubble of the struck
/// entity `id` the event names in its [`StrikeEnds`] (see [`StruckBubbles::find`]); it is not
/// asked for an event that names none. Every other event is returned as it is. The struck
/// bubble's radius is returned with the event, when it was found, for the impact to bind
/// (`UnitRadius`); an unfound bubble counts as one of [`DEFAULT_STRUCK_RADIUS`] centred on the
/// published point.
#[must_use]
pub fn at_struck_surface(
    event: &CombatEventVisual,
    mut struck_by_id: impl FnMut(EntityId) -> Option<StruckBubble>,
    view: Option<Vec3>,
) -> (CombatEventVisual, Option<f32>) {
    let mut moved = event.clone();
    let assumed = |point: &[f32; 3]| StruckBubble {
        radius: DEFAULT_STRUCK_RADIUS,
        height: point[2],
        dome: false,
    };
    let found = strike_ends(event).and_then(|ends| struck_by_id(ends.target));
    match &mut moved {
        CombatEventVisual::HitscanTracer { from, to, .. }
        | CombatEventVisual::BeamVisual { from, to, .. } => {
            let back = Vec2::new(from[0] - to[0], from[1] - to[1]);
            let length = back.length();
            if length > f32::EPSILON {
                let bubble = found.unwrap_or_else(|| assumed(to));
                match view {
                    Some(view) => onto_outline(to, bubble, back / length, length * MOST_PULL, view),
                    None => onto_glass(to, bubble, back / length, length * MOST_PULL),
                }
            }
        }
        CombatEventVisual::MeleeSlashVisual { pos, angle_rad, .. } => {
            let bubble = found.unwrap_or_else(|| assumed(pos));
            let towards = Vec2::from_angle(*angle_rad);
            match view {
                Some(view) => onto_outline(pos, bubble, towards, f32::MAX, view),
                None => onto_glass(pos, bubble, towards, f32::MAX),
            }
        }
        CombatEventVisual::ProjectileVisual { .. }
        | CombatEventVisual::BubbleShatter { .. }
        | CombatEventVisual::SplashVisual { .. }
        | CombatEventVisual::SpellImpact { .. } => return (moved, None),
    }
    (moved, found.map(|bubble| bubble.radius))
}

/// The direction an instant or melee hit came from, in radians in the ground plane (Bevy
/// `atan2(z, x)`, which is the map's `atan2(y, x)`): from the hit point towards the attacker.
/// `None` for an event that is no such hit.
#[must_use]
pub fn hit_facing(event: &CombatEventVisual) -> Option<f32> {
    match event {
        CombatEventVisual::HitscanTracer { from, to, .. }
        | CombatEventVisual::BeamVisual { from, to, .. } => {
            Some((from[1] - to[1]).atan2(from[0] - to[0]))
        }
        CombatEventVisual::MeleeSlashVisual { angle_rad, .. } => Some(*angle_rad),
        CombatEventVisual::ProjectileVisual { vel, .. } => Some((-vel[1]).atan2(-vel[0])),
        CombatEventVisual::BubbleShatter { .. }
        | CombatEventVisual::SplashVisual { .. }
        | CombatEventVisual::SpellImpact { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle_pipeline::commands::SessionGeneration;
    use crate::battle_pipeline::timeline::BattleFrameState;
    use sim_types::{
        AnimState, Concealment, EffectSource, PlayerId, SilhouetteClass, StanceView, StatusFlags,
        Tick, UnitTypeId, WeaponKey, WeaponSlot, WeaponVisualKind,
    };

    /// The attacker of every test event.
    const ATTACKER: EntityId = EntityId(1);

    /// The hit point of an instant hit or a blow; NaN for anything else, which fails every
    /// comparison below.
    fn hit_point(event: &CombatEventVisual) -> [f32; 3] {
        match event {
            CombatEventVisual::HitscanTracer { to, .. } => *to,
            CombatEventVisual::MeleeSlashVisual { pos, .. } => *pos,
            _ => [f32::NAN; 3],
        }
    }

    fn source(slot: WeaponSlot) -> EffectSource {
        EffectSource {
            weapon: WeaponKey { slot, mode: None },
            ..EffectSource::default()
        }
    }

    /// A tracer from `from` to `to` that struck `target` (`None`: a synthetic event).
    fn tracer(
        from: [f32; 3],
        to: [f32; 3],
        slot: WeaponSlot,
        target: Option<EntityId>,
    ) -> CombatEventVisual {
        CombatEventVisual::HitscanTracer {
            from,
            to,
            owner: PlayerId(0),
            source: source(slot),
            ends: target.map(|target| StrikeEnds {
                attacker: ATTACKER,
                target,
            }),
        }
    }

    /// A presented unit bubble of entity `id` at map `at` on ground at 0, standing `altitude`
    /// up (a flyer's 2.5).
    fn bubble(id: u32, at: [f32; 2], altitude: f32, radius: f32) -> BubbleRenderInstance {
        BubbleRenderInstance {
            entity_id: EntityId(id),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::HeavyHex,
            unit_type: UnitTypeId::default(),
            owner: PlayerId(1),
            world_pos: [at[0], at[1], altitude],
            ground_z: 0.0,
            radius,
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
            concealment: Concealment::None,
            speed_boosted: false,
            stance: StanceView::default(),
            status: StatusFlags::default(),
            barrier_ratio: 0.0,
        }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// The hit point of `event` moved by the lookup of `struck` over `presented` and
    /// `previous`, without a camera.
    fn moved_by(
        event: &CombatEventVisual,
        struck: &mut StruckBubbles,
        presented: &[BubbleRenderInstance],
        previous: Option<&[BubbleRenderInstance]>,
    ) -> ([f32; 3], Option<f32>) {
        let (moved, radius) =
            at_struck_surface(event, |id| struck.find(presented, previous, id), None);
        (hit_point(&moved), radius)
    }

    #[test]
    fn a_tracer_ends_on_the_struck_bubbles_glass_towards_its_muzzle_and_up() {
        let bubble = StruckBubble {
            radius: 0.5,
            height: 0.5,
            dome: false,
        };
        let shot = tracer(
            [0.0, 0.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Primary,
            Some(EntityId(2)),
        );
        let (moved, radius) = at_struck_surface(&shot, |_| Some(bubble), None);
        assert_eq!(radius, Some(0.5));
        let to = hit_point(&moved);
        let reach = 0.5 + SURFACE_MARGIN;
        assert!(near(to[0], 4.0 - reach * HIT_ELEVATION.cos()), "{to:?}");
        assert!(near(to[1], 0.0) && near(to[2], 0.5 + reach * HIT_ELEVATION.sin()));
        assert_eq!(
            moved,
            tracer([0.0, 0.0, 0.5], to, WeaponSlot::Primary, Some(EntityId(2)))
        );
    }

    #[test]
    fn a_hit_takes_its_targets_bubble_by_id_not_its_neighbours() {
        // The target (id 4, radius 0.4) stood at (5, 0) on the hit's tick; it has since moved
        // 0.3 on, and a bigger neighbour (id 3, radius 0.9) now stands nearer the published
        // point. The hit takes the target's own radius and height.
        let presented = [
            bubble(3, [5.05, 0.0], 0.0, 0.9),
            bubble(4, [5.3, 0.0], 0.0, 0.4),
        ];
        let shot = tracer(
            [0.0, 0.0, 0.5],
            [5.0, 0.0, 0.5],
            WeaponSlot::Ground,
            Some(EntityId(4)),
        );
        let mut struck = StruckBubbles::default();
        let (to, radius) = moved_by(&shot, &mut struck, &presented, None);
        assert_eq!(radius, Some(0.4), "the target's own radius");
        let reach = 0.4 + SURFACE_MARGIN;
        assert!(near(to[0], 5.0 - reach * HIT_ELEVATION.cos()), "{to:?}");
        assert!(near(to[2], 0.4 + reach * HIT_ELEVATION.sin()), "{to:?}");
        assert_eq!(struck.found_in(StruckFrom::Presented), 1);
    }

    /// B-125: a `Primary` weapon's shot at a flyer hovering over a packed ground group. The
    /// published point is the flyer's centre at ground + 0.5, inside the ground unit under it;
    /// the hit is drawn on the flyer's glass, 2.5 units above the ground units' glass.
    #[test]
    fn a_primary_shot_at_a_flyer_over_a_crowd_is_drawn_on_the_flyer() {
        let presented = [
            bubble(10, [8.0, 8.0], 0.0, 0.5),
            bubble(11, [8.9, 8.0], 0.0, 0.5),
            bubble(12, [7.1, 8.0], 0.0, 0.5),
            bubble(13, [8.0, 8.0], 2.5, 0.5),
        ];
        let shot = tracer(
            [2.0, 8.0, 0.5],
            [8.0, 8.0, 0.5],
            WeaponSlot::Primary,
            Some(EntityId(13)),
        );
        let mut struck = StruckBubbles::default();
        let (to, radius) = moved_by(&shot, &mut struck, &presented, None);
        assert_eq!(radius, Some(0.5));
        let glass = (0.5 + SURFACE_MARGIN) * HIT_ELEVATION.sin();
        assert!(
            near(to[2], 2.5 + 0.5 + glass),
            "on the flyer's glass: {to:?}"
        );
        // The ground unit under it would have put the hit 2.5 units lower.
        let under = moved_by(
            &tracer(
                [2.0, 8.0, 0.5],
                [8.0, 8.0, 0.5],
                WeaponSlot::Primary,
                Some(EntityId(10)),
            ),
            &mut struck,
            &presented,
            None,
        )
        .0;
        assert!(near(to[2] - under[2], 2.5), "{to:?} over {under:?}");
    }

    /// A killing hit: its target was reaped on the hit's tick. When the presentation shows that
    /// tick exactly, the previous snapshot still lists the target.
    #[test]
    fn a_killing_hit_finds_its_target_in_the_previous_frame() {
        let previous = [
            bubble(20, [3.0, 3.0], 0.0, 0.6),
            bubble(21, [6.0, 3.0], 0.0, 0.3),
        ];
        let presented = [bubble(21, [6.0, 3.0], 0.0, 0.3)];
        let blow = CombatEventVisual::MeleeSlashVisual {
            pos: [3.0, 3.0, 0.5],
            angle_rad: 0.0,
            radius: 0.5,
            owner: PlayerId(0),
            source: EffectSource::default(),
            ends: Some(StrikeEnds {
                attacker: EntityId(21),
                target: EntityId(20),
            }),
        };
        let mut struck = StruckBubbles::default();
        let (pos, radius) = moved_by(&blow, &mut struck, &presented, Some(&previous));
        assert_eq!(radius, Some(0.6));
        let reach = 0.6 + SURFACE_MARGIN;
        assert!(near(pos[0], 3.0 + reach * HIT_ELEVATION.cos()), "{pos:?}");
        assert_eq!(struck.found_in(StruckFrom::Previous), 1);
    }

    /// Amendment 2: the common case of a killing hit. Its event is shown once the presentation
    /// reaches the hit's tick, which it does while interpolating from that tick towards the
    /// next: both snapshots it holds then lack the target. The bubbles presented on the frame
    /// before, interpolated towards the hit's tick, still hold it.
    #[test]
    fn a_killing_hit_shown_while_interpolating_finds_its_target_in_the_last_frame() {
        let mut frames = BattleFrameState::default();
        let target = bubble(30, [4.0, 4.0], 2.5, 0.7);
        let shooter = bubble(31, [1.0, 4.0], 0.0, 0.4);
        let initial = frames.initialize(SessionGeneration::FIRST).expect("fresh");
        initial.bubbles = vec![target.clone(), shooter.clone()];
        frames.finish_initialization().expect("initialized");
        let hit = tracer(
            [1.0, 4.0, 0.5],
            [4.0, 4.0, 0.5],
            WeaponSlot::Air,
            Some(EntityId(30)),
        );
        // Tick 1: the hit kills the target, which is reaped on that tick.
        let tick = frames.begin_tick().expect("ready");
        tick.tick = Tick(1);
        tick.bubbles = vec![shooter.clone()];
        tick.combat_events = vec![hit];
        frames.finish_tick().expect("published");
        let mut struck = StruckBubbles::default();
        // The frame that ran tick 1 shows tick 0 interpolated towards it, the target present.
        frames.present(0.5).expect("presented");
        assert!(frames.pop_presented_event().is_none(), "not yet shown");
        struck.remember(&frames.presented().bubbles);
        // The next frame runs tick 2 and shows tick 1 interpolated towards it: the hit pops.
        let tick = frames.begin_tick().expect("ready");
        tick.tick = Tick(2);
        tick.bubbles = vec![shooter];
        frames.finish_tick().expect("published");
        frames.present(0.25).expect("presented");
        let event = frames
            .pop_presented_event()
            .expect("the hit is shown")
            .event;
        let presented = &frames.presented().bubbles;
        let previous = frames
            .previous()
            .map(|snapshot| snapshot.bubbles.as_slice());
        assert!(struck_by_id(presented, EntityId(30)).is_none());
        assert!(previous.is_some_and(|previous| struck_by_id(previous, EntityId(30)).is_none()));
        let (to, radius) = moved_by(&event, &mut struck, presented, previous);
        assert_eq!(radius, Some(0.7), "the flyer's own bubble");
        assert!(to[2] > 2.5 + 0.7, "on the flyer's glass: {to:?}");
        assert_eq!(struck.found_in(StruckFrom::LastFrame), 1);
    }

    #[test]
    fn an_event_without_ends_counts_as_the_default_bubble() {
        let presented = [bubble(40, [2.0, 2.0], 2.5, 0.9)];
        let mut struck = StruckBubbles::default();
        let blow = CombatEventVisual::MeleeSlashVisual {
            pos: [2.0, 2.0, 0.5],
            angle_rad: std::f32::consts::FRAC_PI_2,
            radius: 0.5,
            owner: PlayerId(1),
            source: EffectSource::default(),
            ends: None,
        };
        let (pos, radius) = moved_by(&blow, &mut struck, &presented, None);
        assert_eq!(radius, None);
        let reach = DEFAULT_STRUCK_RADIUS + SURFACE_MARGIN;
        assert!(
            near(pos[0], 2.0) && near(pos[1], 2.0 + reach * HIT_ELEVATION.cos()),
            "the default bubble's surface: {pos:?}"
        );
        assert!(near(pos[2], 0.5 + reach * HIT_ELEVATION.sin()), "{pos:?}");
        assert_eq!(
            StruckFrom::ALL.map(|from| struck.found_in(from)),
            [0; 4],
            "no lookup is asked for"
        );
        // A struck entity found nowhere is counted and counts as the default too.
        let shot = tracer(
            [0.0, 2.0, 0.5],
            [2.0, 2.0, 0.5],
            WeaponSlot::Primary,
            Some(EntityId(99)),
        );
        let (_, radius) = moved_by(&shot, &mut struck, &presented, None);
        assert_eq!(radius, None);
        assert_eq!(struck.found_in(StruckFrom::Nowhere), 1);
        assert_eq!(hit_facing(&blow), Some(std::f32::consts::FRAC_PI_2));
    }

    #[test]
    fn a_hit_on_a_flyer_rises_to_its_glass() {
        let flyer = StruckBubble {
            radius: 0.6,
            height: 3.1,
            dome: false,
        };
        let shot = tracer(
            [0.0, 0.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Air,
            Some(EntityId(2)),
        );
        let to = hit_point(&at_struck_surface(&shot, |_| Some(flyer), None).0);
        assert!(
            near(to[2], 3.1 + (0.6 + SURFACE_MARGIN) * HIT_ELEVATION.sin()),
            "{to:?}"
        );
    }

    #[test]
    fn a_shot_ends_on_the_outline_the_camera_sees() {
        let bubble = StruckBubble {
            radius: 0.5,
            height: 0.5,
            dome: false,
        };
        let target = Some(EntityId(2));
        // A camera looking down at 58 degrees towards +z (the map's +y).
        let pitch = 58.0_f32.to_radians();
        let view = Vec3::new(0.0, -pitch.sin(), pitch.cos());
        // Shot from the camera's side (map -y): its end on the lower outline, towards the
        // camera and down; from the far side: on the upper outline, away and up.
        let below = tracer(
            [4.0, -4.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Primary,
            target,
        );
        let to = hit_point(&at_struck_surface(&below, |_| Some(bubble), Some(view)).0);
        let reach = 0.5 + SURFACE_MARGIN;
        assert!(
            near(to[1], -reach * pitch.sin()) && near(to[2], 0.5 - reach * pitch.cos()),
            "{to:?}"
        );
        let far = tracer(
            [4.0, 4.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Primary,
            target,
        );
        let to = hit_point(&at_struck_surface(&far, |_| Some(bubble), Some(view)).0);
        assert!(
            near(to[1], reach * pitch.sin()) && near(to[2], 0.5 + reach * pitch.cos()),
            "{to:?}"
        );
        // Across the view: the outline's side, level with the centre.
        let across = tracer(
            [0.0, 0.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Primary,
            target,
        );
        let to = hit_point(&at_struck_surface(&across, |_| Some(bubble), Some(view)).0);
        assert!(near(to[0], 4.0 - reach) && near(to[2], 0.5), "{to:?}");
    }

    #[test]
    fn a_dome_is_struck_on_its_visible_outline_never_below_its_base() {
        // A dome of radius 2 standing on the floor at 0 (review 4, A4-1).
        let dome = StruckBubble {
            radius: 2.0,
            height: 0.0,
            dome: true,
        };
        let target = Some(EntityId(2));
        let pitch = 58.0_f32.to_radians();
        let view = Vec3::new(0.0, -pitch.sin(), pitch.cos());
        // From below on screen (the camera's side), across, and from above.
        for (from, name) in [
            ([10.0, -10.0, 0.5], "from below"),
            ([0.0, 0.0, 0.5], "across"),
            ([10.0, 10.0, 0.5], "from above"),
        ] {
            let shot = tracer(from, [10.0, 0.0, 0.5], WeaponSlot::Primary, target);
            let to = hit_point(&at_struck_surface(&shot, |_| Some(dome), Some(view)).0);
            let flat = (to[0] - 10.0).hypot(to[1]);
            assert!(to[2] >= 0.0, "{name}: at or above the base: {to:?}");
            // On the outline: as far out as the dome's radius in plan, or up its far side.
            assert!(
                flat >= 2.0 - 1e-4 || to[2] > 2.0 * 0.5,
                "{name}: outside the dome: {to:?}"
            );
        }
        // From below: on the base rim, towards the camera, lifted off the floor.
        let shot = tracer(
            [10.0, -10.0, 0.5],
            [10.0, 0.0, 0.5],
            WeaponSlot::Primary,
            target,
        );
        let to = hit_point(&at_struck_surface(&shot, |_| Some(dome), Some(view)).0);
        assert!(
            near(to[2], DOME_RIM_LIFT) && near(to[1], -(2.0 + SURFACE_MARGIN)),
            "{to:?}"
        );
        // A presented dome's struck bubble stands on its base.
        let mut presented = bubble(50, [10.0, 0.0], 0.3, 2.0);
        presented.shape = ShapeKind::BuildingDome {
            footprint_w: 3,
            footprint_h: 3,
        };
        assert_eq!(
            StruckBubble::of(&presented),
            StruckBubble {
                radius: 2.0,
                height: 0.3,
                dome: true,
            }
        );
    }

    #[test]
    fn a_short_shot_is_never_pulled_past_its_muzzle() {
        let bubble = StruckBubble {
            radius: 2.0,
            height: 0.5,
            dome: false,
        };
        let shot = tracer(
            [0.0, 0.0, 0.5],
            [0.5, 0.0, 0.5],
            WeaponSlot::Primary,
            Some(EntityId(2)),
        );
        let to = hit_point(&at_struck_surface(&shot, |_| Some(bubble), None).0);
        assert!(near(to[0], 0.5 * (1.0 - MOST_PULL)), "{to:?}");
    }

    #[test]
    fn the_last_frame_is_kept_in_place() {
        let presented = [
            bubble(1, [0.0, 0.0], 0.0, 0.5),
            bubble(2, [1.0, 0.0], 0.0, 0.5),
        ];
        let mut struck = StruckBubbles::default();
        struck.remember(&presented);
        let kept = struck.buffer();
        for _ in 0..8 {
            struck.remember(&presented);
            struck.remember(&presented[..1]);
        }
        assert_eq!(struck.buffer(), kept, "refilled in place");
        let facing = hit_facing(&tracer(
            [0.0, 0.0, 0.5],
            [0.0, 3.0, 0.5],
            WeaponSlot::Primary,
            None,
        ))
        .unwrap_or(f32::NAN);
        assert!(
            near(facing, -std::f32::consts::FRAC_PI_2),
            "towards the muzzle"
        );
    }
}
