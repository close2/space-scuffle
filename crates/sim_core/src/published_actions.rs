//! Which animation states the simulation can publish for each archetype.
//!
//! # Responsibility
//! Owns the one rule that answers "can an entity of this archetype ever appear in a
//! [`RenderSnapshot`](sim_types::snapshot::RenderSnapshot) with this
//! [`AnimState`]?". A presentation layer that must hold media for every state it may be asked
//! to draw asks this module, instead of demanding every state of every identity.
//!
//! # How it fits
//! `Entity::current_anim` is the only source of `BubbleRenderInstance::anim_state`. It is
//! written in exactly these places, and each rule below names the capability that gates it:
//!
//! | State | Writer | Gate |
//! |---|---|---|
//! | `Idle` | `Entity::from_archetype`, every task end, arrival | none: every entity starts idle |
//! | `Constructing` | `production::construction`, when a foundation is placed | a `Build` order, admitted only for a builder that [`UnitArchetype::can_raise`] the structure |
//! | `Walk` | `movement::locomotion::step_toward` | the actor is not `is_immobile` on that tick |
//! | `Attack` | `movement::locomotion`, in range during a weapon windup or backswing | the actor is not immobile and has a weapon |
//! | `Cast` | `economy::harvest` at a node, `production::construction` at a foundation | `capabilities.harvester` or `capabilities.can_construct` |
//! | `Die` | none | a dying entity leaves the snapshot; its shatter is a combat event |
//!
//! Locomotion returns early for an immobile actor, so neither `Walk` nor `Attack` can be written
//! for it. An entity is mobile when its archetype is not immobile by default, when a declared
//! mode clears `is_immobile`, or when a burrow toggle can lift it out of the ground.
//!
//! # Invariants
//! - The rule reads catalog data only: shapes, speeds, weapons, modes, abilities and
//!   capabilities. It never names an identity.
//! - It may over-approximate, never under-approximate: a state it leaves out can never be
//!   published. `published_actions_tests.rs` checks that against what real worlds emit.

use crate::catalog::{UnitArchetype, UnitCatalog};
use sim_types::{AbilityEffectSpec, AnimState};

/// A set of [`AnimState`]s, stored as one bit per state in declaration order.
///
/// `Copy` and allocation-free, so a presentation layer can keep one per identity and test it
/// every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActionSet(u8);

impl ActionSet {
    /// The empty set.
    pub const EMPTY: Self = Self(0);

    /// Bit of `action` in the set.
    const fn bit(action: AnimState) -> u8 {
        1 << action as u8
    }

    /// Returns this set with `action` added.
    #[must_use]
    pub const fn with(self, action: AnimState) -> Self {
        Self(self.0 | Self::bit(action))
    }

    /// Returns this set with `action` added when `present` is true.
    #[must_use]
    pub const fn with_if(self, action: AnimState, present: bool) -> Self {
        if present {
            self.with(action)
        } else {
            self
        }
    }

    /// Whether `action` is in the set.
    #[must_use]
    pub const fn contains(self, action: AnimState) -> bool {
        self.0 & Self::bit(action) != 0
    }

    /// The members, in [`AnimState::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = AnimState> {
        AnimState::ALL
            .into_iter()
            .filter(move |&action| self.contains(action))
    }
}

impl UnitArchetype {
    /// Whether an entity of this archetype can ever be mobile, so that locomotion runs for it.
    ///
    /// True when it is mobile by default, when one of its modes clears `is_immobile`, or when
    /// it can burrow, since unburrowing sets `is_immobile` to false whatever the default.
    #[must_use]
    pub fn can_ever_move(&self) -> bool {
        !self.is_immobile_by_default()
            || self.modes.iter().any(|mode| !mode.is_immobile)
            || self.abilities.iter().any(|ability| {
                matches!(
                    ability.effect,
                    AbilityEffectSpec::CloakToggle {
                        is_burrow: true,
                        ..
                    }
                )
            })
    }

    /// Whether an entity of this archetype can ever hold a weapon: its own, or one a mode or a
    /// siege toggle installs.
    #[must_use]
    pub fn can_ever_be_armed(&self) -> bool {
        self.weapon.is_some()
            || self.ground_weapon.is_some()
            || self.air_weapon.is_some()
            || self.modes.iter().any(|mode| {
                matches!(mode.ground_weapon_override, Some(Some(_)))
                    || matches!(mode.air_weapon_override, Some(Some(_)))
            })
            || self
                .abilities
                .iter()
                .any(|ability| matches!(ability.effect, AbilityEffectSpec::SiegeModeToggle { .. }))
    }
}

impl UnitCatalog {
    /// Every animation state the simulation can publish for an entity of `archetype`.
    ///
    /// `archetype` should belong to this catalog: `Constructing` depends on whether any
    /// archetype here can raise it. See the module table for each rule.
    #[must_use]
    pub fn publishable_actions(&self, archetype: &UnitArchetype) -> ActionSet {
        let raised = self
            .archetypes()
            .any(|builder| builder.can_raise(archetype));
        let mobile = archetype.can_ever_move();
        let casts =
            archetype.capabilities.harvester.is_some() || archetype.capabilities.can_construct;
        ActionSet::EMPTY
            .with(AnimState::Idle)
            .with_if(AnimState::Constructing, raised)
            .with_if(AnimState::Walk, mobile)
            .with_if(AnimState::Attack, mobile && archetype.can_ever_be_armed())
            .with_if(AnimState::Cast, casts)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use sim_types::ShapeKind;

    fn shipped() -> UnitCatalog {
        UnitCatalog::from_ron_str_bundle(
            &crate::test_support::shipped_factions(),
            include_str!("../../../assets/catalog/units.ron"),
            include_str!("../../../assets/catalog/structures.ron"),
        )
        .expect("the shipped catalog loads")
    }

    #[test]
    fn a_set_holds_exactly_what_was_added() {
        let set = ActionSet::EMPTY
            .with(AnimState::Walk)
            .with_if(AnimState::Die, false)
            .with_if(AnimState::Cast, true);
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            [AnimState::Walk, AnimState::Cast]
        );
        for action in AnimState::ALL {
            assert_eq!(
                set.contains(action),
                matches!(action, AnimState::Walk | AnimState::Cast)
            );
        }
        assert_eq!(ActionSet::EMPTY.iter().count(), 0);
    }

    #[test]
    fn no_identity_is_ever_asked_to_die_and_every_one_idles() {
        let catalog = shipped();
        for archetype in catalog.archetypes() {
            let set = catalog.publishable_actions(archetype);
            assert!(set.contains(AnimState::Idle), "{}", archetype.slug);
            assert!(!set.contains(AnimState::Die), "{}", archetype.slug);
        }
    }

    #[test]
    fn a_shipped_dome_never_walks_or_attacks_and_is_always_raised() {
        let catalog = shipped();
        let domes = catalog
            .archetypes()
            .filter(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }));
        let mut seen = 0;
        for dome in domes {
            seen += 1;
            let set = catalog.publishable_actions(dome);
            assert!(!set.contains(AnimState::Walk), "{}", dome.slug);
            assert!(!set.contains(AnimState::Attack), "{}", dome.slug);
            assert!(set.contains(AnimState::Constructing), "{}", dome.slug);
        }
        assert!(seen > 0, "the shipped catalog has structures");
    }

    #[test]
    fn a_mode_or_a_burrow_can_free_an_immobile_archetype() {
        let catalog = shipped();
        let mut anchored = catalog
            .archetypes()
            .find(|a| a.is_immobile_by_default() && a.modes.is_empty() && a.abilities.is_empty())
            .expect("the shipped catalog has an immobile archetype")
            .clone();
        assert!(!anchored.can_ever_move());

        let mut moded = anchored.clone();
        moded.modes.push(sim_types::UnitModeSpec {
            mode_name: "uprooted".to_owned(),
            cast_ticks: 0,
            leave_cast_ticks: 0,
            cast_random_delay_max_ticks: sim_types::SimScalar::ZERO,
            leave_random_delay_max_ticks: sim_types::SimScalar::ZERO,
            is_immobile: false,
            unpushable: false,
            is_cloaked: false,
            is_burrowed: false,
            mover_type_override: None,
            sight_radius_override: None,
            ground_weapon_override: None,
            air_weapon_override: None,
            energy_cost_per_second: sim_types::SimScalar::ZERO,
        });
        assert!(moded.can_ever_move());

        anchored.abilities.push(sim_types::AbilitySpec {
            name: "burrow".to_owned(),
            target_kind: sim_types::AbilityTargetKind::ToggleMode,
            energy_cost: sim_types::SimScalar::ZERO,
            cooldown_ticks: 0,
            cast_range: sim_types::SimScalar::ZERO,
            effect: AbilityEffectSpec::CloakToggle {
                energy_cost_per_second: sim_types::SimScalar::ZERO,
                is_burrow: true,
            },
            leave_name: None,
        });
        assert!(anchored.can_ever_move());
    }
}
