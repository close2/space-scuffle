//! Held beams: one beam per attacker and weapon while it keeps striking, its ends following
//! both units (plan `2026-11-17-effect-snapshot-identity` D-4, B-126).
//!
//! # Responsibility
//! Owns [`HeldBeams`], the client-local table of the beams held now. The simulation publishes a
//! beam weapon's every strike as a `BeamVisual` that names its attacker and target
//! ([`StrikeEnds`]). A strike whose weapon's `travel` look is sustained (`sustain` in
//! `effects.ron`) does not spawn a shot of its own:
//! - [`HeldBeams::strike`] refreshes the beam held for the strike's attacker and weapon,
//!   moving its far end to the strike's target (a retarget, as the game's channelled beam may
//!   switch target without ending), or holds a new one. The beam's deadline becomes the
//!   strike's time plus the attacker's presented attack period (`attack_swing.period_ticks`, so
//!   attack-rate buffs count; the look's own duration without a swing) plus the look's grace;
//! - [`HeldBeams::keep`], once a frame after the frame's strikes, puts every held beam's line
//!   from its attacker's bubble to its target's outline, and recomputes its fog cut, because a
//!   held beam lives for as long as its attacker keeps firing. It releases a beam whose
//!   attacker or target is not presented (death, load, gone) or whose deadline has passed; the
//!   look's held layers then erode out over their exit (at most
//!   [`super::catalog::MAX_SUSTAIN_EXIT`]), and the beam is ended when its exit is over, so
//!   the one-shot pulses of its onset do not outlive it.
//!
//! The per-strike impact (the burn on the target) is still laid once per strike by the event
//! wiring, on the struck surface.
//!
//! # How it fits
//! `effects::systems::spawn_effects_from_events` hands every admitted strike to
//! [`HeldBeams::strike`] before it asks for a travel shot; [`keep_beams`] runs after it in the
//! battle's present stage. Presentation only: nothing is written back to the simulation.
//!
//! # Invariants
//! - At most one beam per `(attacker, weapon source)`; slots are sorted by that key and found by
//!   binary search. At most [`HELD_BEAMS`] are held: a strike that finds the table full is
//!   refused and counted ([`HeldBeams::refused`]), and draws no travel.
//! - The table's buffer is reserved once and never grows, so steady state allocates nothing.
//! - Entity ids restart in a new battle session, so every beam is ended when the presented
//!   session changes ([`HeldBeams::follow`]).

use super::binding::BoundValues;
use super::catalog::schema::Binding;
use super::catalog::{EffectCatalog, EffectId, ShotReach, UnitEffectTables};
use super::placement::WorldLine;
use super::pool::EffectHandle;
use super::runtime::{EffectRuntime, NO_CLIP};
use super::struck::{onto_struck_outline, StruckBubble, MOST_PULL};
use crate::battle_pipeline::commands::SessionGeneration;
use crate::battle_pipeline::fog::BattleFog;
use bevy::prelude::*;
use sim_types::snapshot::{BubbleRenderInstance, CombatEventVisual};
use sim_types::{EffectSource, EntityId, PlayerId, StrikeEnds};

/// Beams held at once: a battle of 400 units has a few dozen beam units at most.
pub const HELD_BEAMS: usize = 64;

/// Seconds of one simulation tick.
const TICK_SECONDS: f32 = 1.0 / sim_types::TICKS_PER_SECOND as f32;

/// One held beam.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeamSlot {
    /// The attacking entity.
    pub attacker: EntityId,
    /// The attacker's weapon.
    pub source: EffectSource,
    /// The entity it strikes now.
    pub target: EntityId,
    /// The attacker's owner, for the fog cut.
    pub owner: PlayerId,
    /// The look.
    pub effect: EffectId,
    /// Its live instance.
    pub handle: EffectHandle,
    /// Seconds of `Time<Virtual>` after which, with no further strike, it is released.
    pub deadline: f64,
}

/// The beams held now (see the module docs).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct HeldBeams {
    /// Held beams, sorted by `(attacker, source)`.
    slots: Vec<BeamSlot>,
    /// Strikes not held because [`HELD_BEAMS`] beams were.
    pub refused: u64,
    /// The battle session the beams were held in; `None` when none is presented.
    session: Option<SessionGeneration>,
    /// Released beams eroding out, with when their exit ends: each is then ended at once, with
    /// whatever one-shot layers it still plays (the pulses of its onset), so nothing of a beam
    /// outlives its exit.
    fading: Vec<(EffectHandle, f64)>,
}

impl Default for HeldBeams {
    fn default() -> Self {
        Self {
            slots: Vec::with_capacity(HELD_BEAMS),
            refused: 0,
            session: None,
            fading: Vec::with_capacity(HELD_BEAMS),
        }
    }
}

/// What holding and keeping beams works with.
pub struct BeamContext<'a> {
    /// The vocabulary.
    pub catalog: &'a EffectCatalog,
    /// The units' choices.
    pub tables: &'a UnitEffectTables,
    /// Live effects.
    pub runtime: &'a mut EffectRuntime,
    /// Seconds of `Time<Virtual>`.
    pub now: f64,
}

/// The bubble of entity `id` among `bubbles`, which are in entity id order.
fn bubble_of(bubbles: &[BubbleRenderInstance], id: EntityId) -> Option<&BubbleRenderInstance> {
    bubbles
        .binary_search_by_key(&id, |bubble| bubble.entity_id)
        .ok()
        .and_then(|index| bubbles.get(index))
}

/// The line of a beam from `attacker`'s bubble to `target`'s, in Bevy world space: it ends on
/// the target's outline as the camera looking along `view` sees it (on its glass without a
/// camera), facing the attacker, and starts where it leaves the attacker's bubble, `muzzle`
/// world units from its drawn centre towards that end (at most 0.8 of the beam's length), as an
/// instant shot's travel does. The two bubbles' drawn centres give the heights, so a flyer's
/// beam leaves its own bubble, not the ground below it.
#[must_use]
pub fn beam_line(
    attacker: &BubbleRenderInstance,
    target: &BubbleRenderInstance,
    muzzle: f32,
    view: Option<Vec3>,
) -> WorldLine {
    let from = StruckBubble::of(attacker);
    let [ax, ay, _] = attacker.world_pos;
    let [tx, ty, _] = target.world_pos;
    let back = Vec2::new(ax - tx, ay - ty);
    let length = back.length();
    let mut end = [tx, ty, StruckBubble::of(target).height];
    if length > f32::EPSILON {
        onto_struck_outline(
            &mut end,
            StruckBubble::of(target),
            back / length,
            length * MOST_PULL,
            view,
        );
    }
    let start = Vec3::new(ax, from.height, ay);
    let end = Vec3::new(end[0], end[2], end[1]);
    let span = start.distance(end);
    let from = start + (end - start).normalize_or_zero() * muzzle.min(0.8 * span);
    WorldLine::whole(from, end)
}

/// The values a held beam along `line` binds: its length; it does not fly.
fn beam_bound(line: WorldLine) -> BoundValues {
    BoundValues::default()
        .with(Binding::LineLength, line.from.distance(line.to))
        .with(Binding::FlightTime, 0.0)
}

impl HeldBeams {
    /// Follows the presented battle session (`None`: none is presented): when it changes,
    /// every beam is ended at once (the old session's battle is no longer shown), because
    /// entity ids restart in a new session. Call once a frame, before that frame's strikes.
    pub fn follow(&mut self, session: Option<SessionGeneration>, runtime: &mut EffectRuntime) {
        if self.session != session {
            self.session = session;
            for slot in self.slots.drain(..) {
                runtime.retire(slot.handle);
            }
            for (handle, _) in self.fading.drain(..) {
                runtime.retire(handle);
            }
        }
    }

    /// Takes the admitted strike `event` (its points already on the struck surface) if its
    /// weapon's travel look is sustained: refreshes the beam its attacker and weapon hold, or
    /// holds a new one along `line` (the strike's travel line). `presented` (in entity id
    /// order) gives the attacker's attack period. Returns whether the strike was taken, so the
    /// caller spawns no travel shot for it; a `BeamVisual` without [`StrikeEnds`] or with a look
    /// that is not sustained is not taken, and a strike refused for a full table is taken
    /// (counted, drawn without a travel).
    pub fn strike(
        &mut self,
        event: &CombatEventVisual,
        line: Option<WorldLine>,
        presented: &[BubbleRenderInstance],
        context: &mut BeamContext<'_>,
    ) -> bool {
        let CombatEventVisual::BeamVisual {
            source,
            owner,
            ends: Some(StrikeEnds { attacker, target }),
            ..
        } = *event
        else {
            return false;
        };
        let Some(effect) = context.tables.travel(source) else {
            return false;
        };
        let Some((sustain, look)) = context
            .catalog
            .effect(effect)
            .and_then(|look| look.sustain.map(|sustain| (sustain, look)))
        else {
            return false;
        };
        let period = bubble_of(presented, attacker)
            .and_then(|bubble| bubble.attack_swing)
            .map_or_else(
                || {
                    look.layers
                        .iter()
                        .map(|layer| layer.start + layer.duration)
                        .fold(0.0_f32, f32::max)
                },
                |swing| f32::from(swing.period_ticks) * TICK_SECONDS,
            );
        let deadline = context.now + f64::from(period + sustain.grace);
        let key = (attacker, source);
        match self
            .slots
            .binary_search_by_key(&key, |slot| (slot.attacker, slot.source))
        {
            Ok(index) if self.slots[index].effect == effect => {
                let slot = &mut self.slots[index];
                slot.target = target;
                slot.deadline = deadline;
                true
            }
            found => {
                let index = match found {
                    Ok(index) => {
                        // The weapon's look changed (a mode switch): the old beam goes.
                        let old = self.slots.remove(index);
                        fade(
                            &mut self.fading,
                            old,
                            context.catalog,
                            context.runtime,
                            context.now,
                        );
                        index
                    }
                    Err(index) => index,
                };
                if self.slots.len() == HELD_BEAMS {
                    self.refused += 1;
                    return true;
                }
                let line = line.unwrap_or(WorldLine::whole(Vec3::ZERO, Vec3::ZERO));
                let request = super::runtime::SpawnRequest {
                    effect,
                    anchor: line.from,
                    bound: beam_bound(line),
                    line: Some(line),
                    delay: 0.0,
                };
                if let Some(handle) =
                    context
                        .runtime
                        .hold(context.catalog, &request, context.now, false)
                {
                    self.slots.insert(
                        index,
                        BeamSlot {
                            attacker,
                            source,
                            target,
                            owner,
                            effect,
                            handle,
                            deadline,
                        },
                    );
                }
                true
            }
        }
    }

    /// Keeps every held beam on its two bubbles in `presented` (the presented frame, in entity
    /// id order), seen by the camera looking along `view`, and cut by `fog` where the viewer
    /// does not see another player's beam; releases the beams whose attacker or target is not
    /// presented or whose deadline has passed.
    pub fn keep(
        &mut self,
        presented: &[BubbleRenderInstance],
        fog: Option<&BattleFog>,
        view: Option<Vec3>,
        context: &mut BeamContext<'_>,
    ) {
        let (catalog, tables, now) = (context.catalog, context.tables, context.now);
        let runtime = &mut *context.runtime;
        self.fading.retain(|(handle, until)| {
            let over = *until <= now;
            if over {
                runtime.retire(*handle);
            }
            !over
        });
        let fading = &mut self.fading;
        self.slots.retain(|slot| {
            let ends = bubble_of(presented, slot.attacker).zip(bubble_of(presented, slot.target));
            let Some((attacker, target)) = ends.filter(|_| now <= slot.deadline) else {
                fade(fading, *slot, catalog, runtime, now);
                return false;
            };
            let line = beam_line(attacker, target, tables.muzzle(attacker.unit_type), view);
            if !runtime.update_held_line(slot.handle, line, beam_bound(line)) {
                return false;
            }
            let clip = match fog {
                Some(fog) if fog.viewer() != Some(slot.owner) => super::systems::shot_clip(
                    line,
                    catalog
                        .effect(slot.effect)
                        .map_or(ShotReach::default(), |effect| effect.shot_reach),
                    |x, y| fog.sees(x, y),
                ),
                _ => NO_CLIP,
            };
            runtime.set_fog_clip(slot.handle, clip);
            true
        });
    }

    /// The beams held now, sorted by attacker and weapon.
    #[must_use]
    pub fn slots(&self) -> &[BeamSlot] {
        &self.slots
    }

    /// The table's retained buffers, as `(name, address, capacity in elements)`, which an
    /// allocation would move or grow: the held beams and the released ones fading out.
    #[must_use]
    pub fn buffers(&self) -> [(&'static str, usize, usize); 2] {
        [
            (
                "beams.slots",
                self.slots.as_ptr().addr(),
                self.slots.capacity(),
            ),
            (
                "beams.fading",
                self.fading.as_ptr().addr(),
                self.fading.capacity(),
            ),
        ]
    }
}

/// Releases the beam held in `slot` at `now`: its held layers erode out over the look's exit,
/// and the beam is ended once that exit is over ([`HeldBeams::keep`]), or at once when
/// [`HELD_BEAMS`] beams are already fading.
fn fade(
    fading: &mut Vec<(EffectHandle, f64)>,
    slot: BeamSlot,
    catalog: &EffectCatalog,
    runtime: &mut EffectRuntime,
    now: f64,
) {
    if fading.len() >= HELD_BEAMS {
        runtime.retire(slot.handle);
        return;
    }
    runtime.release(slot.handle, now);
    let exit = catalog
        .effect(slot.effect)
        .map_or(0.0, |effect| effect.held_exit);
    fading.push((slot.handle, now + f64::from(exit)));
}

/// What [`keep_beams`] reads to put the held beams on their bubbles: the battle, the fog and
/// the battle camera (its view direction).
type BeamWorld<'w, 's> = (
    Option<Res<'w, crate::battle_pipeline::session::BattleSession>>,
    Option<Res<'w, BattleFog>>,
    Query<'w, 's, &'static GlobalTransform, With<crate::RtsCamera>>,
);

/// Keeps the held beams on their bubbles once a frame, after the frame's strikes.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn keep_beams(
    beams: Option<ResMut<'_, HeldBeams>>,
    catalog: Option<Res<'_, EffectCatalog>>,
    tables: Option<Res<'_, UnitEffectTables>>,
    runtime: Option<ResMut<'_, EffectRuntime>>,
    time: Res<'_, Time<Virtual>>,
    (session, fog, cameras): BeamWorld<'_, '_>,
) {
    let (Some(mut beams), Some(catalog), Some(tables), Some(mut runtime)) =
        (beams, catalog, tables, runtime)
    else {
        return;
    };
    let view = cameras
        .get_single()
        .ok()
        .map(|transform| transform.forward().as_vec3());
    let presented = session.as_deref().map_or(&[][..], |session| {
        session.frames.presented().bubbles.as_slice()
    });
    let mut context = BeamContext {
        catalog: &catalog,
        tables: &tables,
        runtime: &mut runtime,
        now: time.elapsed_secs_f64(),
    };
    beams.keep(presented, fog.as_deref(), view, &mut context);
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;
    use sim_types::{
        AnimState, Concealment, ShapeKind, SilhouetteClass, StanceView, StatusFlags, UnitTypeId,
        WeaponVisualKind,
    };

    /// A unit bubble of entity `id` at map `(x, y)` standing at altitude `altitude`.
    fn bubble(id: u32, x: f32, y: f32, altitude: f32, radius: f32) -> BubbleRenderInstance {
        BubbleRenderInstance {
            entity_id: EntityId(id),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::HeavyHex,
            unit_type: UnitTypeId::default(),
            owner: PlayerId(0),
            world_pos: [x, y, altitude],
            ground_z: 0.0,
            radius,
            velocity: [0.0; 2],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: true,
            weapon_visual: WeaponVisualKind::LaserBeam,
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
            barrier_ratio: 1.0,
        }
    }

    #[test]
    fn a_flyers_beam_leaves_its_own_bubble_and_ends_on_the_targets_glass() {
        // A flyer hovering 2.5 above the ground beams at a ground unit 6 units away.
        let flyer = bubble(1, 0.0, 0.0, 2.5, 0.6);
        let ground = bubble(2, 6.0, 0.0, 0.0, 0.5);
        let line = beam_line(&flyer, &ground, 0.6, None);
        let centre = Vec3::new(0.0, 3.1, 0.0);
        assert!(
            (line.from.distance(centre) - 0.6).abs() < 1e-4,
            "the muzzle is on the flyer's own glass, not below it: {:?}",
            line.from
        );
        let target = Vec3::new(6.0, 0.5, 0.0);
        let reach = 0.5 + super::super::struck::SURFACE_MARGIN;
        assert!(
            (line.to.distance(target) - reach).abs() < 1e-4,
            "{:?}",
            line.to
        );
        assert!(line.to.x < 6.0, "the end faces the attacker");
        assert_eq!(line.seen, u64::MAX, "a beam is drawn whole");
    }

    #[test]
    fn the_muzzle_never_passes_four_fifths_of_a_short_beam() {
        let attacker = bubble(1, 0.0, 0.0, 0.0, 0.5);
        let target = bubble(2, 0.9, 0.0, 0.0, 0.5);
        let line = beam_line(&attacker, &target, 5.0, None);
        let start = Vec3::new(0.0, 0.5, 0.0);
        let span = start.distance(line.to);
        assert!((line.from.distance(start) - 0.8 * span).abs() < 1e-4);
    }

    #[test]
    fn units_standing_on_one_point_still_give_a_line() {
        let attacker = bubble(1, 2.0, 2.0, 0.0, 0.5);
        let target = bubble(2, 2.0, 2.0, 0.0, 0.5);
        let line = beam_line(&attacker, &target, 0.5, None);
        assert!(line.from.is_finite() && line.to.is_finite());
    }
}
