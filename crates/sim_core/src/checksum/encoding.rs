//! Private canonical encoders shared by partial and world checksums.
//!
//! Fixed-width little-endian primitives, length-prefixed collections and explicit
//! enum tags keep the byte stream independent of platform layouts and serde formats.
//! Exhaustive struct destructuring makes field additions require checksum review.

use crate::entity::{
    ActiveBuff, ActiveProjectile, BarrierState, Entity, MovementMemory, PendingLine,
    PendingStrikes, WeaponPhase,
};
use crate::entity::{Cargo, UnitTask};
use crate::match_state::MatchState;
use crate::production::construction::Construction;
use crate::production::{ProductionItem, ProductionJob};
use crate::world::{ActiveHazardZone, PlayerResources};
use sim_map::{
    CellFlags, CliffLevel, MapCell, PlacedStructure, RampDir, ResourceKind, ResourceNode,
    TerrainKind, Watchtower,
};
use sim_types::{
    AnimState, ArmorClassId, AttackOrigin, AttributeBonuses, AttributeFlags, EntityId, MatchResult,
    MoverType, PlayerId, PlayerUpgradeState, ProjectileTargetLossBehavior, ResourceNodeId,
    SimAngle, SimScalar, SimVec2, SplashLine, SplashLineAxis, SplashLineOrigin, SplashShape,
    SplashSpec, SplashZone, StructureStrike, Tick, TurretIdle, UnitOrder, UnitStatDeltas,
    UnitTypeId, VisibilityState, WeaponClassId, WeaponSpec, WeaponVisualKind,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use xxhash_rust::xxh64::Xxh64;

/// Bytes gathered before the hasher sees them.
const ENCODER_BUFFER_BYTES: usize = 4096;

// PERFORMANCE RATIONALE:
// Why: A world digest is millions of one- and two-byte fields, and each hasher update call costs
// far more than the bytes it carries. On a 256x256 map this dominated the whole tick.
// What: Gather fields in a stack buffer and hand the hasher whole blocks.
// Trade-off: A copy per field. xxHash is a stream, so the digest is byte-for-byte unchanged.
pub(super) struct Encoder {
    hasher: Xxh64,
    buffer: [u8; ENCODER_BUFFER_BYTES],
    filled: usize,
}

impl Encoder {
    pub(super) fn new(domain: &str) -> Self {
        let mut encoder = Self {
            hasher: Xxh64::new(0),
            buffer: [0; ENCODER_BUFFER_BYTES],
            filled: 0,
        };
        domain.encode(&mut encoder);
        super::CHECKSUM_SCHEMA_VERSION.encode(&mut encoder);
        encoder
    }

    pub(super) fn length(&mut self, length: usize) {
        (length as u64).encode(self);
    }

    fn write(&mut self, bytes: &[u8]) {
        if bytes.len() > ENCODER_BUFFER_BYTES - self.filled {
            self.hasher.update(&self.buffer[..self.filled]);
            self.filled = 0;
            if bytes.len() > ENCODER_BUFFER_BYTES {
                self.hasher.update(bytes);
                return;
            }
        }
        self.buffer[self.filled..self.filled + bytes.len()].copy_from_slice(bytes);
        self.filled += bytes.len();
    }

    pub(super) fn finish(mut self) -> u64 {
        self.hasher.update(&self.buffer[..self.filled]);
        self.hasher.digest()
    }
}

pub(super) trait Canonical {
    fn encode(&self, encoder: &mut Encoder);
}

macro_rules! integers {
    ($($ty:ty),+ $(,)?) => {$ (
        impl Canonical for $ty {
            fn encode(&self, encoder: &mut Encoder) {
                encoder.write(&self.to_le_bytes());
            }
        }
    )+};
}
integers!(u8, u16, u32, u64, u128, i8, i32, i64);

impl Canonical for bool {
    fn encode(&self, encoder: &mut Encoder) {
        u8::from(*self).encode(encoder);
    }
}

impl Canonical for str {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.length(self.len());
        encoder.write(self.as_bytes());
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
// Weapon and armor classes are catalog data (`assets/catalog/factions.ron`). Their authored IDs
// equal the explicit tags the former Rust enums had here, so they encode as the same single byte
// and every recorded checksum is unchanged. Changing an authored class ID is a schema change.
wrapper!(
    EntityId,
    UnitTypeId,
    PlayerId,
    Tick,
    CliffLevel,
    ResourceNodeId,
    WeaponClassId,
    ArmorClassId
);

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
// A field listed after `unhashed` is left out of the checksum on purpose; its declaration
// states why that is sound.
macro_rules! fields {
    ($ty:ident { $($field:ident),+ $(,)? }) => {
        impl Canonical for $ty {
            fn encode(&self, encoder: &mut Encoder) {
                let Self { $($field),+ } = self;
                $($field.encode(encoder);)+
            }
        }
    };
    ($ty:ident { $($field:ident),+ $(,)? } unhashed { $($skip:ident),+ $(,)? }) => {
        impl Canonical for $ty {
            fn encode(&self, encoder: &mut Encoder) {
                let Self { $($field,)+ $($skip: _,)+ } = self;
                $($field.encode(encoder);)+
            }
        }
    };
}

impl Canonical for AttributeBonuses {
    fn encode(&self, encoder: &mut Encoder) {
        // Every attribute's bonus in `Attribute::ALL` order, zeros included, so the encoding
        // has a fixed length and an attribute's place never depends on the others.
        for (_, bonus) in self.entries() {
            bonus.encode(encoder);
        }
    }
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
    flank_left,
    corner_round,
    flank_active,
    flank_reversed,
    flank_spent,
    flank_blocked_ticks,
    flank_walked,
    flank_target
});
fields!(ActiveBuff {
    buff_id,
    remaining_ticks,
    speed_multiplier,
    attack_rate_multiplier
});
fields!(BarrierState {
    raised_ticks,
    absorb_left,
    cooldown_ticks
});
fields!(ProductionJob {
    item,
    remaining_ticks,
    total_ticks,
    reserved_supply,
    paid_minerals,
    paid_gas
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
    weapon_delay_carry,
    pending_strikes,
    pending_line,
    active_buffs,
    barrier,
    attack_dash_cooldown,
    production_queue,
    captured_supply_used,
    captured_supply_provided,
    task,
    cargo,
    construction,
    is_mineral_walking,
    is_holding_position,
    current_anim,
    last_hit_angle,
    shield_hit_flash_ticks,
    hull_hit_flash_ticks,
    current_target,
    previous_target_position,
    engaged_target,
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
} unhashed {
    hull_turn_budget,
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
    strikes,
});
fields!(WeaponSpec {
    name,
    visual_kind,
    target_flags,
    min_range,
    max_range,
    firing_arc,
    acquire_arc,
    has_turret,
    turret_turn_rate,
    turret_idle,
    base_damage,
    attribute_bonuses,
    attacks_count,
    strike_interval_ticks,
    followup_max_distance,
    channeled,
    period_ticks,
    damage_point_ticks,
    backswing_ticks,
    random_delay_min,
    random_delay_max,
    splash,
    projectile_speed,
    weapon_class,
    upgrade_bonus_per_level,
    bonus_upgrade_per_level,
    target_loss_behavior,
    max_projectile_lifetime_ticks,
    suicide_on_attack,
    structure_strike,
});
fields!(PendingLine {
    points,
    ticks_until_next,
    plane,
    angle,
    primary,
    struck
});
fields!(PendingStrikes {
    target_id,
    plane,
    remaining,
    ticks_until_next,
    channeled
});
fields!(SplashSpec {
    zones,
    shape,
    friendly_fire,
    offset_by_unit_radius
});
fields!(SplashLine {
    origin,
    axis,
    offsets,
    period_ticks
});

impl Canonical for SplashShape {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            Self::Circle => 0_u8.encode(encoder),
            Self::Line(line) => {
                1_u8.encode(encoder);
                line.encode(encoder);
            }
        }
    }
}

impl Canonical for SplashLineOrigin {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            Self::Attacker => 0_u8,
            Self::Target => 1_u8,
        }
        .encode(encoder);
    }
}

impl Canonical for SplashLineAxis {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            Self::Along => 0_u8,
            Self::Across => 1_u8,
        }
        .encode(encoder);
    }
}
fields!(SplashZone { radius, fraction });
fields!(StructureStrike {
    damage,
    upgrade_per_level,
    ignores_armor
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
fields!(Cargo { kind, amount });
fields!(UnitTask {
    goal,
    legs,
    gather_ticks,
    wait_ticks,
    depot,
    last_node
});
fields!(Construction {
    remaining_ticks,
    total_ticks,
    hp_per_tick,
    final_hp_residue,
    paid_minerals,
    paid_gas,
    last_worked_tick
});
fields!(MatchState {
    result,
    participants,
    defeated
});

impl Canonical for MatchResult {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            Self::InProgress => 0_u8.encode(encoder),
            Self::Victory { winner, tick } => {
                1_u8.encode(encoder);
                winner.encode(encoder);
                tick.encode(encoder);
            }
            Self::Draw { tick } => {
                2_u8.encode(encoder);
                tick.encode(encoder);
            }
        }
    }
}

impl Canonical for ProductionItem {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            Self::Unit(product) => {
                0_u8.encode(encoder);
                product.encode(encoder);
            }
            Self::Research(upgrade) => {
                1_u8.encode(encoder);
                upgrade.encode(encoder);
            }
        }
    }
}
tags!(MoverType { Ground = 0, Air = 1, CliffJumper = 2, CliffWalker = 3 });

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

impl Canonical for TurretIdle {
    fn encode(&self, encoder: &mut Encoder) {
        let (tag, rate) = match *self {
            Self::Hold => (0_u8, SimAngle::ZERO),
            Self::Reset { rate } => (1, rate),
            Self::ResetIfMoving { rate } => (2, rate),
            Self::FollowMovement { rate } => (3, rate),
            Self::Spin { step } => (4, step),
        };
        tag.encode(encoder);
        rate.encode(encoder);
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
            Self::Construct { foundation } => {
                12_u8.encode(encoder);
                foundation.encode(encoder);
            }
            Self::CancelConstruction => 13_u8.encode(encoder),
            Self::Research { upgrade } => {
                14_u8.encode(encoder);
                upgrade.encode(encoder);
            }
            Self::CancelProduction { job_index } => {
                15_u8.encode(encoder);
                job_index.encode(encoder);
            }
            Self::Load { carrier } => {
                16_u8.encode(encoder);
                carrier.encode(encoder);
            }
            Self::UnloadAll => 17_u8.encode(encoder),
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
            4, 0, 0, 0, 0, 0, 0, 0, b't', b'e', b's', b't', 25, 0, 0, 0, 0x34, 0x12, 0xfe, 0xff,
            0xff, 0xff, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0xc3, 0xa9,
        ];
        assert_eq!(encoder.finish(), xxh64(&bytes, 0));
    }

    fn digest<T: Canonical + ?Sized>(value: &T) -> u64 {
        let mut encoder = Encoder::new("test");
        value.encode(&mut encoder);
        encoder.finish()
    }

    /// Phase 9 WP12 (D-6): weapon and armor classes moved from Rust enums with explicit checksum
    /// tags into `factions.ron`. Each shipped class ID equals its former tag and encodes as the
    /// same single byte, alone, in an `Option` and as a level-map key, so no recorded checksum
    /// or golden replay changes.
    #[test]
    fn shipped_class_ids_encode_exactly_as_the_former_enum_tags() -> crate::error::SimResult<()> {
        use sim_types::{ArmorClassId, WeaponClassId};
        use std::collections::BTreeMap;
        // (slug, former tag) in the former declaration order of `WeaponClass` and `ArmorClass`.
        const WEAPON_TAGS: [(&str, u8); 8] = [
            ("directorate_infantry_weapons", 0),
            ("directorate_vehicle_weapons", 1),
            ("directorate_ship_weapons", 2),
            ("enclave_ground_weapons", 3),
            ("enclave_air_weapons", 4),
            ("swarm_melee_attacks", 5),
            ("swarm_missile_attacks", 6),
            ("swarm_flyer_attacks", 7),
        ];
        const ARMOR_TAGS: [(&str, u8); 6] = [
            ("directorate_infantry_armor", 0),
            ("directorate_vehicle_plating", 1),
            ("enclave_ground_armor", 2),
            ("enclave_air_armor", 3),
            ("swarm_ground_carapace", 4),
            ("swarm_flyer_carapace", 5),
        ];
        let factions = crate::test_support::shipped_factions();
        assert_eq!(factions.weapon_classes().count(), WEAPON_TAGS.len());
        assert_eq!(factions.armor_classes().count(), ARMOR_TAGS.len());
        let mut weapon_levels = (BTreeMap::new(), BTreeMap::new());
        for (slug, tag) in WEAPON_TAGS {
            let id = factions.resolve_weapon_class("test", slug)?;
            assert_eq!(id, WeaponClassId(tag), "{slug}");
            assert_eq!(digest(&Some(id)), digest(&Some(tag)), "{slug}");
            weapon_levels.0.insert(id, tag + 1);
            weapon_levels.1.insert(tag, tag + 1);
        }
        assert_eq!(digest(&weapon_levels.0), digest(&weapon_levels.1));
        let mut armor_levels = (BTreeMap::new(), BTreeMap::new());
        for (slug, tag) in ARMOR_TAGS {
            let id = factions.resolve_armor_class("test", slug)?;
            assert_eq!(id, ArmorClassId(tag), "{slug}");
            assert_eq!(digest(&id), digest(&tag), "{slug}");
            armor_levels.0.insert(id, tag + 1);
            armor_levels.1.insert(tag, tag + 1);
        }
        assert_eq!(digest(&armor_levels.0), digest(&armor_levels.1));
        Ok(())
    }

    #[test]
    fn digest_does_not_depend_on_how_fields_meet_the_buffer() {
        // Straddles the buffer boundary and then exceeds the whole buffer in one field.
        for length in [
            1,
            super::ENCODER_BUFFER_BYTES - 1,
            super::ENCODER_BUFFER_BYTES,
            3 * super::ENCODER_BUFFER_BYTES,
        ] {
            let text = "x".repeat(length);
            let mut encoder = Encoder::new("test");
            7_u32.encode(&mut encoder);
            text.as_str().encode(&mut encoder);
            let mut bytes = vec![4, 0, 0, 0, 0, 0, 0, 0, b't', b'e', b's', b't', 25, 0, 0, 0];
            bytes.extend([7, 0, 0, 0]);
            bytes.extend((length as u64).to_le_bytes());
            bytes.extend(text.as_bytes());
            assert_eq!(encoder.finish(), xxh64(&bytes, 0));
        }
    }

    #[test]
    fn production_queue_encodes_all_four_remaining_tick_bytes() {
        let queue: std::collections::VecDeque<crate::production::ProductionJob> =
            [crate::production::ProductionJob {
                item: crate::production::ProductionItem::Unit(sim_types::UnitTypeId(0x1234)),
                remaining_ticks: 65_537,
                total_ticks: 2,
                reserved_supply: 0x1234_5678,
                paid_minerals: 3,
                paid_gas: 4,
            }]
            .into();
        // Domain length and bytes, schema version, queue length, item tag, archetype u16,
        // then the full remaining duration u32 (0x0001_0001), total, supply and payments.
        let bytes = [
            4, 0, 0, 0, 0, 0, 0, 0, b't', b'e', b's', b't', 25, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0,
            0x34, 0x12, 1, 0, 1, 0, 2, 0, 0, 0, 0x78, 0x56, 0x34, 0x12, 3, 0, 0, 0, 4, 0, 0, 0,
        ];
        assert_eq!(digest(&queue), xxh64(&bytes, 0));
    }

    #[test]
    fn carrier_identity_has_explicit_option_tag_and_all_id_bytes() {
        let mut bytes = vec![4, 0, 0, 0, 0, 0, 0, 0, b't', b'e', b's', b't', 25, 0, 0, 0];
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
