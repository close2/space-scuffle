//! Categorical enums for geometry, silhouettes, animation, weapons, and classification.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// Canonical sci-fi factions in Space Scuffle.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Faction {
    /// Industrial war-machines, railguns, and overclock reactors (Terran mechanical archetype).
    AutomatonDirectorate,
    /// Classic Roswell Greys, flying saucers, psi-blades, and regenerating shields (Protoss archetype).
    CosmicEnclave,
    /// Bioluminescent predators, armored beetles, and corrosive bio-acid (Zerg archetype).
    SylvanSwarm,
}

impl Faction {
    /// Human-readable faction title.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::AutomatonDirectorate => "The Automaton Directorate",
            Self::CosmicEnclave => "The Cosmic Enclave",
            Self::SylvanSwarm => "The Sylvan Swarm",
        }
    }

    /// Primary tactical UI accent color (RGBA).
    #[must_use]
    pub const fn primary_color(self) -> [f32; 4] {
        match self {
            Self::AutomatonDirectorate => [0.18, 0.65, 1.0, 1.0], // Cyan / Cobalt
            Self::CosmicEnclave => [0.72, 0.25, 1.0, 1.0],        // Psionic Violet / Chrome
            Self::SylvanSwarm => [0.22, 0.95, 0.35, 1.0],         // Bioluminescent Emerald
        }
    }
}

/// Geometric shape category determining base model geometry and collision bounds.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShapeKind {
    /// Floating 3D holographic sphere for mobile units.
    UnitSphere,
    /// Grounded hemisphere mounted on a foundation grid plate for structures.
    BuildingDome {
        /// Foundation width in 1x1 map cells.
        footprint_w: u8,
        /// Foundation height in 1x1 map cells.
        footprint_h: u8,
    },
}

/// Distinct geometric collar/attachment on the bubble rim for swarm readability.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SilhouetteClass {
    /// Slender triangular prongs for light infantry (e.g., Marine).
    LightProngs,
    /// Reinforced heavy hexagonal bracket for armored assault (e.g., Marauder, Roach).
    HeavyHex,
    /// Floating glowing halo ring for spellcasters and support units (e.g., Sentry, Raven).
    CasterHalo,
    /// Radial curved spikes for melee assault units (e.g., Zealot, Zergling).
    MeleeSpikes,
    /// Foundation structural ring with industrial anchoring plates for buildings.
    StructureBase,
}

/// High-level unit action state driving inner video playback and procedural shaders.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AnimState {
    /// Blueprint assembly / foundation welding (buildings only).
    Constructing,
    /// Ambient idling loop.
    Idle,
    /// Locomotion / movement cycle.
    Walk,
    /// Attack windup, damage execution, and recoil cycle.
    Attack,
    /// Ability channel or casting cycle.
    Cast,
    /// Shatter collapse / explosion cycle before entity destruction.
    Die,
}

/// Visual category of an active weapon attack.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponVisualKind {
    /// No weapon visual active.
    None,
    /// Instant directional tracer line + muzzle flash (e.g., Marine Gauss Rifle).
    Hitscan,
    /// Physical traveling projectile bolt (e.g., Stalker Particle Disruptor).
    Projectile,
    /// Continuous sustained energy beam (e.g., Void Ray Prismatic Beam).
    LaserBeam,
    /// Jagged electric branch arc (e.g., High Templar Psionic Storm / Lightning Sentry).
    LightningArc,
    /// Crescent shockwave slash at close range (e.g., Zealot Psi-Blade).
    MeleeSlash,
}

/// Terrain traversal capabilities of a unit.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MoverType {
    /// Standard ground unit obeying `NavMesh` boundaries and cliff heights.
    Ground,
    /// Aerial unit floating above terrain, ignoring cliffs, ground pathing, and ramps.
    Air,
    /// Ground unit capable of hopping up/down cliff edges (e.g., Reaper).
    CliffJumper,
    /// Tall unit walking over cliffs while retaining ground footprint (e.g., Colossus).
    CliffWalker,
}

bitflags! {
    /// Bitflags representing unit combat and armor classification attributes.
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct AttributeFlags: u16 {
        /// Light units susceptible to anti-light weapon bonuses.
        const LIGHT       = 1 << 0;
        /// Armored units susceptible to anti-armor weapon bonuses.
        const ARMORED     = 1 << 1;
        /// Living organic units susceptible to biological abilities.
        const BIOLOGICAL  = 1 << 2;
        /// Robotic/mechanical units repairable by workers.
        const MECHANICAL  = 1 << 3;
        /// Psionic entities vulnerable to feedback/EMP.
        const PSIONIC     = 1 << 4;
        /// Massive tier-3 units immune to concussive slow and certain crowd controls.
        const MASSIVE     = 1 << 5;
        /// Fixed building domes and defensive structures.
        const STRUCTURE   = 1 << 6;
        /// Special campaign or commander heroes.
        const HEROIC      = 1 << 7;
    }
}

/// Fog-of-war visibility state of a map tile or entity for a specific player.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VisibilityState {
    /// Black shroud: never seen by player, no terrain or units visible.
    Unexplored,
    /// Fogged: terrain and static structures previously explored; dynamic units hidden.
    Fogged,
    /// Fully revealed by allied line-of-sight.
    Visible,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_bitflags() {
        let flags = AttributeFlags::LIGHT | AttributeFlags::BIOLOGICAL;
        assert!(flags.contains(AttributeFlags::LIGHT));
        assert!(flags.contains(AttributeFlags::BIOLOGICAL));
        assert!(!flags.contains(AttributeFlags::ARMORED));

        let ron_str = ron::to_string(&flags).expect("invariant: valid ron serialization");
        let decoded: AttributeFlags =
            ron::from_str(&ron_str).expect("invariant: valid ron deserialization");
        assert_eq!(flags, decoded);
    }

    #[test]
    fn test_enums_serde() {
        let shape = ShapeKind::BuildingDome {
            footprint_w: 3,
            footprint_h: 3,
        };
        let shape_ron = ron::to_string(&shape).expect("invariant: valid ron serialization");
        let shape_dec: ShapeKind =
            ron::from_str(&shape_ron).expect("invariant: valid ron deserialization");
        assert_eq!(shape, shape_dec);

        let silhouette = SilhouetteClass::CasterHalo;
        let sil_ron = ron::to_string(&silhouette).expect("invariant: valid ron serialization");
        let sil_dec: SilhouetteClass =
            ron::from_str(&sil_ron).expect("invariant: valid ron deserialization");
        assert_eq!(silhouette, sil_dec);

        let anim = AnimState::Attack;
        let anim_ron = ron::to_string(&anim).expect("invariant: valid ron serialization");
        let anim_dec: AnimState =
            ron::from_str(&anim_ron).expect("invariant: valid ron deserialization");
        assert_eq!(anim, anim_dec);

        let weapon = WeaponVisualKind::LightningArc;
        let weapon_ron = ron::to_string(&weapon).expect("invariant: valid ron serialization");
        let weapon_dec: WeaponVisualKind =
            ron::from_str(&weapon_ron).expect("invariant: valid ron deserialization");
        assert_eq!(weapon, weapon_dec);

        let mover = MoverType::CliffWalker;
        let mover_ron = ron::to_string(&mover).expect("invariant: valid ron serialization");
        let mover_dec: MoverType =
            ron::from_str(&mover_ron).expect("invariant: valid ron deserialization");
        assert_eq!(mover, mover_dec);

        let vis = VisibilityState::Visible;
        let vis_ron = ron::to_string(&vis).expect("invariant: valid ron serialization");
        let vis_dec: VisibilityState =
            ron::from_str(&vis_ron).expect("invariant: valid ron deserialization");
        assert_eq!(vis, vis_dec);

        let faction = Faction::AutomatonDirectorate;
        let fac_ron = ron::to_string(&faction).expect("invariant: valid ron serialization");
        let fac_dec: Faction =
            ron::from_str(&fac_ron).expect("invariant: valid ron deserialization");
        assert_eq!(faction, fac_dec);
    }

    #[test]
    fn test_faction_display_and_color() {
        assert_eq!(
            Faction::AutomatonDirectorate.display_name(),
            "The Automaton Directorate"
        );
        assert_eq!(Faction::CosmicEnclave.display_name(), "The Cosmic Enclave");
        assert_eq!(Faction::SylvanSwarm.display_name(), "The Sylvan Swarm");

        let col_auto = Faction::AutomatonDirectorate.primary_color();
        assert_eq!(col_auto[3], 1.0);
        let col_cosm = Faction::CosmicEnclave.primary_color();
        assert_eq!(col_cosm[3], 1.0);
        let col_sylv = Faction::SylvanSwarm.primary_color();
        assert_eq!(col_sylv[3], 1.0);
    }
}
