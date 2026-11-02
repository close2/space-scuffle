//! Private canonical encoders shared by partial and world checksums.
//!
//! Fixed-width little-endian primitives, length-prefixed collections and explicit
//! enum tags keep the byte stream independent of platform layouts and serde formats.
//! Exhaustive struct destructuring makes field additions require checksum review.

use crate::entity::{ActiveBuff, ActiveProjectile, Entity, MovementMemory, WeaponPhase};
use crate::production::ProductionJob;
use crate::world::{ActiveHazardZone, PlayerResources};
use sim_map::{
    CellFlags, CliffLevel, MapCell, PlacedStructure, RampDir, ResourceKind, ResourceNode,
    TerrainKind, Watchtower,
};
use sim_types::{
    AnimState, ArmorClass, AttackOrigin, AttributeFlags, EntityId, MoverType, PlayerId,
    PlayerUpgradeState, ProjectileTargetLossBehavior, SimAngle, SimScalar, SimVec2, SplashSpec,
    Tick, UnitOrder, UnitStatDeltas, UnitTypeId, VisibilityState, WeaponClass, WeaponSpec,
    WeaponVisualKind,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use xxhash_rust::xxh64::Xxh64;

pub(super) struct Encoder(Xxh64);

impl Encoder {
    pub(super) fn new(domain: &str) -> Self {
        let mut encoder = Self(Xxh64::new(0));
        domain.encode(&mut encoder);
        super::CHECKSUM_SCHEMA_VERSION.encode(&mut encoder);
        encoder
    }

    pub(super) fn length(&mut self, length: usize) {
        (length as u64).encode(self);
    }

    pub(super) fn finish(self) -> u64 {
        self.0.digest()
    }
}

pub(super) trait Canonical {
    fn encode(&self, encoder: &mut Encoder);
}

macro_rules! integers {
    ($($ty:ty),+ $(,)?) => {$ (
        impl Canonical for $ty {
            fn encode(&self, encoder: &mut Encoder) {
                encoder.0.update(&self.to_le_bytes());
            }
        }
    )+};
}
integers!(u8, u16, u32, u64, u128, i32, i64);

impl Canonical for bool {
    fn encode(&self, encoder: &mut Encoder) {
        u8::from(*self).encode(encoder);
    }
}

impl Canonical for str {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.length(self.len());
        encoder.0.update(self.as_bytes());
    }
}

impl Canonical for String {
    fn encode(&self, encoder: &mut Encoder) {
        self.as_str().encode(encoder);
    }
}

impl<T: Canonical> Canonical for Option<T> {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            None => 0_u8.encode(encoder),
            Some(value) => {
                1_u8.encode(encoder);
                value.encode(encoder);
            }
        }
    }
}

fn sequence<'a, T: Canonical + 'a>(
    values: impl ExactSizeIterator<Item = &'a T>,
    encoder: &mut Encoder,
) {
    encoder.length(values.len());
    for value in values {
        value.encode(encoder);
    }
}

impl<T: Canonical> Canonical for [T] {
    fn encode(&self, encoder: &mut Encoder) {
        sequence(self.iter(), encoder);
    }
}

impl<T: Canonical, const N: usize> Canonical for [T; N] {
    fn encode(&self, encoder: &mut Encoder) {
        self.as_slice().encode(encoder);
    }
}

impl<T: Canonical> Canonical for Vec<T> {
    fn encode(&self, encoder: &mut Encoder) {
        self.as_slice().encode(encoder);
    }
}

impl<T: Canonical> Canonical for VecDeque<T> {
    fn encode(&self, encoder: &mut Encoder) {
        sequence(self.iter(), encoder);
    }
}

impl<T: Canonical> Canonical for BTreeSet<T> {
    fn encode(&self, encoder: &mut Encoder) {
        sequence(self.iter(), encoder);
    }
}

impl<K: Canonical, V: Canonical> Canonical for BTreeMap<K, V> {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.length(self.len());
        for (key, value) in self {
            key.encode(encoder);
            value.encode(encoder);
        }
    }
}

impl<A: Canonical, B: Canonical> Canonical for (A, B) {
    fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder);
        self.1.encode(encoder);
    }
}

impl<A: Canonical, B: Canonical, C: Canonical> Canonical for (A, B, C) {
    fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder);
        self.1.encode(encoder);
        self.2.encode(encoder);
    }
}

macro_rules! wrapper {
    ($($ty:ty),+ $(,)?) => {$ (
        impl Canonical for $ty {
            fn encode(&self, encoder: &mut Encoder) { self.0.encode(encoder); }
        }
    )+};
}
wrapper!(EntityId, UnitTypeId, PlayerId, Tick, CliffLevel);

impl Canonical for SimScalar {
    fn encode(&self, encoder: &mut Encoder) {
        self.raw().encode(encoder);
    }
}
impl Canonical for SimAngle {
    fn encode(&self, encoder: &mut Encoder) {
        self.raw().encode(encoder);
    }
}
impl Canonical for AttributeFlags {
    fn encode(&self, encoder: &mut Encoder) {
        self.bits().encode(encoder);
    }
}
impl Canonical for CellFlags {
    fn encode(&self, encoder: &mut Encoder) {
        self.bits().encode(encoder);
    }
}

// Field order below is part of the versioned schema. Destructuring intentionally
// has no rest pattern: adding fields must force an explicit coverage decision.
macro_rules! fields {
    ($ty:ident { $($field:ident),+ $(,)? }) => {
        impl Canonical for $ty {
            fn encode(&self, encoder: &mut Encoder) {
                let Self { $($field),+ } = self;
                $($field.encode(encoder);)+
            }
        }
    };
}

fields!(SimVec2 { x, y });
fields!(MovementMemory {
    arrived_at,
    progress_best,
    progress_legs,
    progress_tail,
    stalled_ticks,
    repath_cooldown,
    abandoned_target,
    abandoned_ticks,
    flank_left
});
fields!(ActiveBuff {
    buff_id,
    remaining_ticks,
    speed_multiplier,
    attack_rate_multiplier
});
fields!(ProductionJob {
    product,
    remaining_ticks,
    reserved_supply
});
fields!(Entity {
    id,
    unit_type_id,
    owner,
    position,
    pos_at_tick_start,
    velocity,
    heading,
    turret_angle,
    hp,
    shields,
    energy,
    ticks_since_last_damage,
    order_queue,
    current_funnel_waypoints,
    movement,
    weapon_phase,
    weapon_cooldown_remaining,
    active_buffs,
    production_queue,
    captured_supply_used,
    captured_supply_provided,
    is_mineral_walking,
    is_holding_position,
    current_anim,
    anim_progress,
    last_hit_angle,
    shield_hit_flash_ticks,
    hull_hit_flash_ticks,
    current_target,
    is_immobile,
    unpushable,
    is_sieged,
    siege_weapon,
    active_mode,
    channeling_remaining_ticks,
    pending_ability,
    ability_cooldowns,
    last_storm_damage_tick,
    is_cloaked,
    max_hp,
    is_completed,
    transport_carrier,
    is_invulnerable,
});
fields!(ActiveProjectile {
    id,
    owner,
    attacker_id,
    source_pos,
    current_pos,
    target_id,
    target_pos,
    target_mover,
    speed,
    weapon_spec,
    source_slug,
    remaining_lifetime_ticks,
});
fields!(WeaponSpec {
    name,
    visual_kind,
    target_flags,
    min_range,
    max_range,
    firing_arc,
    has_turret,
    turret_turn_rate,
    base_damage,
    bonus_armored,
    bonus_light,
    attacks_count,
    period_ticks,
    damage_point_ticks,
    backswing_ticks,
    splash,
    projectile_speed,
    weapon_class,
    upgrade_bonus_per_level,
    bonus_upgrade_per_level,
    target_loss_behavior,
    max_projectile_lifetime_ticks,
    suicide_on_attack,
});
fields!(SplashSpec {
    inner_radius,
    middle_radius,
    outer_radius,
    friendly_fire
});
fields!(PlayerResources {
    minerals,
    gas,
    supply_used,
    supply_max,
    supply_provided_total
});
fields!(PlayerUpgradeState {
    weapon_levels,
    armor_levels,
    shield_level,
    completed_upgrade_slugs,
    unit_stat_modifiers,
    unlocked_abilities,
});
fields!(UnitStatDeltas {
    max_hp_delta,
    move_speed_delta,
    sight_delta,
    weapon_range_delta,
    armor_delta,
    attack_cooldown_delta,
    bonus_damage_delta,
});
fields!(ActiveHazardZone {
    id,
    owner,
    center,
    radius,
    delay_ticks,
    remaining_ticks,
    interval_ticks,
    ticks_until_next_interval,
    damage_per_interval,
    targets_friendly,
});
fields!(MapCell {
    cliff_level,
    ramp,
    terrain_kind,
    flags,
    occupying_structure
});
fields!(PlacedStructure {
    id,
    top_left,
    width,
    height,
    prior_flags
});
fields!(ResourceNode {
    pos,
    kind,
    remaining,
    cell
});
fields!(Watchtower {
    id,
    pos,
    capture_radius,
    reveal_radius,
    controlling_player
});

// Stable explicit tags do not depend on declaration order or Rust enum layout.
macro_rules! tags {
    ($ty:ident { $($variant:ident = $tag:literal),+ $(,)? }) => {
        impl Canonical for $ty {
            fn encode(&self, encoder: &mut Encoder) {
                let tag: u8 = match self { $(Self::$variant => $tag),+ };
                tag.encode(encoder);
            }
        }
    };
}
tags!(AnimState { Constructing = 0, Idle = 1, Walk = 2, Attack = 3, Cast = 4, Die = 5 });
tags!(WeaponVisualKind { None = 0, Hitscan = 1, Projectile = 2, LaserBeam = 3, LightningArc = 4, MeleeSlash = 5 });
tags!(VisibilityState { Unexplored = 0, Fogged = 1, Visible = 2 });
tags!(RampDir { North = 0, South = 1, East = 2, West = 3, NorthEast = 4, NorthWest = 5, SouthEast = 6, SouthWest = 7 });
tags!(TerrainKind { Grass = 0, MetalPlate = 1, Dirt = 2, Abyss = 3 });
tags!(ResourceKind { Mineral = 0, Gas = 1 });
tags!(AttackOrigin { Explicit = 0, Acquired = 1 });
tags!(MoverType { Ground = 0, Air = 1, CliffJumper = 2, CliffWalker = 3 });
tags!(WeaponClass {
    DirectorateInfantry = 0, DirectorateVehicle = 1, DirectorateShip = 2,
    EnclaveGround = 3, EnclaveAir = 4, SwarmMelee = 5, SwarmMissile = 6, SwarmFlyer = 7,
});
tags!(ArmorClass {
    DirectorateInfantry = 0, DirectoratePlating = 1, EnclaveGround = 2,
    EnclaveAir = 3, SwarmGroundCarapace = 4, SwarmFlyerCarapace = 5,
});

impl Canonical for WeaponPhase {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            Self::Ready => 0_u8.encode(encoder),
            Self::RotatingToTarget { target_id } => {
                1_u8.encode(encoder);
                target_id.encode(encoder);
            }
            Self::Windup {
                target_id,
                remaining_ticks,
            } => {
                2_u8.encode(encoder);
                target_id.encode(encoder);
                remaining_ticks.encode(encoder);
            }
            Self::Backswing {
                target_id,
                remaining_ticks,
            } => {
                3_u8.encode(encoder);
                target_id.encode(encoder);
                remaining_ticks.encode(encoder);
            }
            Self::Cooldown => 4_u8.encode(encoder),
        }
    }
}

impl Canonical for ProjectileTargetLossBehavior {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            Self::Fizzle => 0_u8.encode(encoder),
            Self::ImpactLastKnownPosition => 1_u8.encode(encoder),
            Self::RetargetNearest { search_radius } => {
                2_u8.encode(encoder);
                search_radius.encode(encoder);
            }
        }
    }
}

impl Canonical for UnitOrder {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            Self::Idle => 0_u8.encode(encoder),
            Self::Move { target } => {
                1_u8.encode(encoder);
                target.encode(encoder);
            }
            Self::AttackMove { target } => {
                2_u8.encode(encoder);
                target.encode(encoder);
            }
            Self::AttackTarget { target, origin } => {
                3_u8.encode(encoder);
                target.encode(encoder);
                origin.encode(encoder);
            }
            Self::HoldPosition => 4_u8.encode(encoder),
            Self::Stop => 5_u8.encode(encoder),
            Self::UseStim => 6_u8.encode(encoder),
            Self::Produce { unit_type } => {
                7_u8.encode(encoder);
                unit_type.encode(encoder);
            }
            Self::Harvest { target } => {
                8_u8.encode(encoder);
                target.encode(encoder);
            }
            Self::ReturnCargo => 9_u8.encode(encoder),
            Self::Build {
                structure_type,
                cell_x,
                cell_y,
            } => {
                10_u8.encode(encoder);
                structure_type.encode(encoder);
                cell_x.encode(encoder);
                cell_y.encode(encoder);
            }
            Self::CastAbility {
                ability_index,
                target_pos,
                target_entity,
            } => {
                11_u8.encode(encoder);
                ability_index.encode(encoder);
                target_pos.encode(encoder);
                target_entity.encode(encoder);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Canonical, Encoder};
    use xxhash_rust::xxh64::xxh64;

    #[test]
    fn canonical_primitives_match_explicit_little_endian_bytes() {
        let mut encoder = Encoder::new("test");
        0x1234_u16.encode(&mut encoder);
        (-2_i32).encode(&mut encoder);
        Some(0_u8).encode(&mut encoder);
        None::<u8>.encode(&mut encoder);
        "é".encode(&mut encoder);
        // Domain length, UTF-8 domain, schema u32, u16, i32, option tags/payload,
        // then UTF-8 string byte length and its two encoded bytes.
        let bytes = [
            4, 0, 0, 0, 0, 0, 0, 0, b't', b'e', b's', b't', 8, 0, 0, 0, 0x34, 0x12, 0xfe, 0xff,
            0xff, 0xff, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0xc3, 0xa9,
        ];
        assert_eq!(encoder.finish(), xxh64(&bytes, 0));
    }

    fn digest<T: Canonical + ?Sized>(value: &T) -> u64 {
        let mut encoder = Encoder::new("test");
        value.encode(&mut encoder);
        encoder.finish()
    }

    #[test]
    fn production_queue_encodes_all_four_remaining_tick_bytes() {
        let queue: std::collections::VecDeque<crate::production::ProductionJob> =
            [crate::production::ProductionJob {
                product: sim_types::UnitTypeId(0x1234),
                remaining_ticks: 65_537,
                reserved_supply: 0x1234_5678,
            }]
            .into();
        // Domain length and bytes, schema 5, queue length, archetype u16,
        // then the full remaining duration u32 (0x0001_0001).
        let bytes = [
            4, 0, 0, 0, 0, 0, 0, 0, b't', b'e', b's', b't', 8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0,
            0x34, 0x12, 1, 0, 1, 0, 0x78, 0x56, 0x34, 0x12,
        ];
        assert_eq!(digest(&queue), xxh64(&bytes, 0));
    }

    #[test]
    fn carrier_identity_has_explicit_option_tag_and_all_id_bytes() {
        let mut bytes = vec![4, 0, 0, 0, 0, 0, 0, 0, b't', b'e', b's', b't', 8, 0, 0, 0];
        bytes.push(0);
        assert_eq!(digest(&None::<sim_types::EntityId>), xxh64(&bytes, 0));
        bytes.pop();
        bytes.extend([1, 0x78, 0x56, 0x34, 0x12]);
        assert_eq!(
            digest(&Some(sim_types::EntityId(0x1234_5678))),
            xxh64(&bytes, 0)
        );
    }

    #[test]
    fn options_and_collection_boundaries_are_unambiguous() {
        assert_ne!(digest(&None::<u8>), digest(&Some(0_u8)));
        assert_ne!(
            digest(&vec!["ab".to_owned(), "c".to_owned()]),
            digest(&vec!["a".to_owned(), "bc".to_owned()])
        );
        assert_ne!(
            digest(&vec![vec![1_u8], vec![2_u8, 3]]),
            digest(&vec![vec![1_u8, 2], vec![3_u8]])
        );
    }
}
