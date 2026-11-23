//! Safe authored-data validation before lossy catalog DTO conversion.
//!
//! Preserves existing valid f32/f64 quantization while rejecting nonfinite and
//! out-of-range fixed-point input, narrowing identifier/footprint conversions and
//! blank identities. It also rejects every value the tick loop cannot handle: a
//! zero divisor, an empty attack cadence, an inverted range or a body with no size
//! fails here, once, instead of on every peer during a match. This is safety
//! validation, not a balance audit: odd but runnable authored values remain legal.

use super::placement_rules::{
    PlacementRulesFile, ResourceFootprintDef, MAX_REACH_SIDE, MAX_RESOURCE_SIDE,
};
use super::{
    PrimaryWeapon, SplashShapeDef, SplashSpecDef, StructureArchetypeDef, TurretIdleDef,
    UnitArchetypeDef, UnitModeSpecDef, WeaponSpecDef,
};
use crate::error::{SimCoreError, SimResult};
use crate::movement::tuning::STALL_REPATH_TICKS;
use crate::weapon_timing::AttackRate;
use sim_map::MAX_SIGHT_RADIUS_CELLS;
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, Attribute, AttributeFlags, MoverType,
    PlacementFlag, ProjectileTargetLossBehavior, ResourceKind, ShapeKind, SimAngle, SimScalar,
    WeaponVisualKind,
};

fn invalid(definition: &str, field: &str, reason: &'static str) -> SimCoreError {
    SimCoreError::CatalogInvalidValue {
        definition: definition.to_owned(),
        field: field.to_owned(),
        reason,
    }
}

pub(crate) fn validate_slug(slug: &str) -> SimResult<()> {
    if slug.trim().is_empty() {
        return Err(invalid(slug, "slug", "must not be blank"));
    }
    Ok(())
}

fn scalar(definition: &str, field: &str, value: f64) -> SimResult<()> {
    if !value.is_finite() || value < f64::from(i32::MIN) || value >= f64::from(i32::MAX) + 1.0 {
        return Err(invalid(
            definition,
            field,
            "must be finite and representable in Q32.32 simulation units",
        ));
    }
    Ok(())
}

/// Rejects a start energy above the maximum: a new entity starts with at most a full pool.
/// Both values are authored decimals, already checked finite and nonnegative.
fn start_energy(definition: &str, start: f64, max: f64) -> SimResult<()> {
    if start > max {
        return Err(invalid(
            definition,
            "start_energy",
            "must not exceed max_energy",
        ));
    }
    Ok(())
}

fn nonnegative(definition: &str, field: &str, value: f64) -> SimResult<()> {
    scalar(definition, field, value)?;
    if value < 0.0 {
        return Err(invalid(definition, field, "must be nonnegative"));
    }
    Ok(())
}

/// Rejects a value that would not remain strictly positive once quantized to Q32.32.
fn positive(definition: &str, field: &str, value: f32) -> SimResult<()> {
    scalar(definition, field, f64::from(value))?;
    if SimScalar::from_f32_lossy(value) <= SimScalar::ZERO {
        return Err(invalid(
            definition,
            field,
            "must remain strictly positive after Q32.32 conversion",
        ));
    }
    Ok(())
}

/// Rejects a sight radius that vision would have to shorten.
fn sight(definition: &str, field: &str, value: f64) -> SimResult<()> {
    nonnegative(definition, field, value)?;
    if value > f64::from(MAX_SIGHT_RADIUS_CELLS) {
        return Err(invalid(
            definition,
            field,
            "must not exceed the maximum sight radius supported by vision",
        ));
    }
    Ok(())
}

fn angle(definition: &str, field: &str, value: f32) -> SimResult<()> {
    if !value.is_finite() {
        return Err(invalid(
            definition,
            field,
            "angle in degrees must be finite",
        ));
    }
    Ok(())
}

/// Checks an angular rate in degrees per tick: finite, above 0 and at most 180.
///
/// 180 degrees per tick is half a turn, which reaches any facing in one tick (no two facings
/// are further apart), so a larger rate would mean nothing more. A rate of 0 would never
/// turn at all. Values outside the range are rejected rather than clamped.
fn rate_per_tick(definition: &str, field: &str, value: f32) -> SimResult<()> {
    angle(definition, field, value)?;
    if value <= 0.0 || value > 180.0 {
        return Err(invalid(
            definition,
            field,
            "must lie in 0 < degrees per tick <= 180",
        ));
    }
    Ok(())
}

/// Requires `shield_regen_delay_ticks` exactly when the archetype has shields: a shielded
/// archetype without it would never know when to regenerate, and an unshielded one with it
/// carries a value nothing reads. `has_shields` is `max_shields > 0` as authored.
fn shield_regen_delay(definition: &str, has_shields: bool, delay: Option<u16>) -> SimResult<()> {
    match (has_shields, delay) {
        (true, None) => Err(invalid(
            definition,
            "shield_regen_delay_ticks",
            "must be set when max_shields is positive",
        )),
        (false, Some(_)) => Err(invalid(
            definition,
            "shield_regen_delay_ticks",
            "must be absent when max_shields is zero",
        )),
        _ => Ok(()),
    }
}

/// Requires a structure's `scan_range` exactly when it is `armed` (has a ground or an air
/// weapon), as a finite, nonnegative world-unit value: an unarmed structure acquires nothing, so
/// a scan range on it is a value nothing reads.
fn structure_scan_range(definition: &str, armed: bool, scan_range: Option<f64>) -> SimResult<()> {
    match (armed, scan_range) {
        (true, None) => Err(invalid(
            definition,
            "scan_range",
            "must be set on a structure with a weapon",
        )),
        (false, Some(_)) => Err(invalid(
            definition,
            "scan_range",
            "must be absent on a structure without a weapon",
        )),
        (true, Some(value)) => {
            scalar(definition, "scan_range", value)?;
            nonnegative(definition, "scan_range", value)
        }
        (false, None) => Ok(()),
    }
}

/// Requires `energy_regen_per_tick` to be 0 when the archetype has no energy (`max_energy` 0):
/// a rate nothing can show is a value nothing reads (plan `2026-11-23-regeneration` D-2). Both
/// values are authored decimals, already checked finite and nonnegative.
fn energy_regen(definition: &str, max_energy: f64, rate: f64) -> SimResult<()> {
    if max_energy == 0.0 && rate > 0.0 {
        return Err(invalid(
            definition,
            "energy_regen_per_tick",
            "must be zero when max_energy is zero",
        ));
    }
    Ok(())
}

/// Requires `hp_regen_delay_ticks` to be 0 when the archetype does not regenerate life: a delay
/// on an archetype with no life regeneration is a value nothing reads. `regenerates` is
/// `hp_regen_per_tick > 0` as authored; any delay, 0 included, is legal when it regenerates.
fn hp_regen_delay(definition: &str, regenerates: bool, delay: u16) -> SimResult<()> {
    if !regenerates && delay > 0 {
        return Err(invalid(
            definition,
            "hp_regen_delay_ticks",
            "must be zero when hp_regen_per_tick is zero",
        ));
    }
    Ok(())
}

/// Checks `turn_before_move` against the unit's speed and hull rates. Runs after both rates are
/// validated, so each lies in `0 < degrees per tick <= 180`.
///
/// A flag on a unit that cannot move is a value nothing reads. A flagged unit gains no ground
/// while it turns in place, so the stall counter of an order counts those ticks: a hull that
/// needed more than `STALL_REPATH_TICKS` ticks to take the first step of a reversal would drop
/// its route mid-turn (and at very low rates abandon the order). It turns in place at the
/// stationary rate `s` until the rest of the half turn fits in one moving tick `r`, and steps on
/// the next tick: `ceil(max(0, 32 768 − r) ÷ s) + 1` ticks, in binary units per tick computed
/// with the loader's own conversion.
fn turn_before_move(
    definition: &str,
    flagged: bool,
    max_speed: f32,
    turn_rate_deg_per_tick: f32,
    stationary_turn_rate_deg_per_tick: f32,
) -> SimResult<()> {
    if !flagged {
        return Ok(());
    }
    if max_speed <= 0.0 {
        return Err(invalid(
            definition,
            "turn_before_move",
            "must be false when max_speed is zero",
        ));
    }
    let moving = u32::from(super::binary_angle(turn_rate_deg_per_tick).raw());
    let stationary = u32::from(super::binary_angle(stationary_turn_rate_deg_per_tick).raw());
    let half_turn = u32::from(SimAngle::DEG_180.raw());
    let rest = half_turn.saturating_sub(moving);
    let in_place = (stationary > 0).then(|| rest.div_ceil(stationary));
    if in_place.is_none_or(|ticks| ticks + 1 > u32::from(STALL_REPATH_TICKS)) {
        return Err(invalid(
            definition,
            "turn_before_move",
            "requires hull rates that take the first step of a reversal within the stall repath window (25 ticks)",
        ));
    }
    Ok(())
}

fn positive_rate(definition: &str, field: &str, rate: SimScalar) -> SimResult<()> {
    AttackRate::new(rate).map_err(|_| {
        invalid(
            definition,
            field,
            "attack rate must remain strictly positive after Q32.32 conversion",
        )
    })?;
    Ok(())
}

/// Checks a structure row's attributes (plan `2026-11-23-structure-attributes` D-2): at least one,
/// none twice, and `Structure` among them. Any other attribute is allowed, because the game puts
/// `Light` on a structure (the Creep Tumor); `Structure` is the one every structure has.
fn structure_attributes(definition: &str, attributes: &[Attribute]) -> SimResult<()> {
    if attributes.is_empty() {
        return Err(invalid(
            definition,
            "attributes",
            "must name at least one attribute",
        ));
    }
    for (index, attribute) in attributes.iter().enumerate() {
        if attributes[..index].contains(attribute) {
            return Err(invalid(
                definition,
                &format!("attributes.{attribute:?}"),
                "listed twice",
            ));
        }
    }
    if !attributes.contains(&Attribute::Structure) {
        return Err(invalid(
            definition,
            "attributes",
            "a structure has the Structure attribute",
        ));
    }
    Ok(())
}

/// Checks a unit row's authored attribute `bits` (plan `2026-11-23-structure-attributes` D-3):
/// every bit names an attribute, at least one is set (an archetype with none matches no weapon's
/// target flags, so nothing could attack it), and `Structure` is not among them, which only a
/// structure row has.
fn unit_attributes(definition: &str, bits: u16) -> SimResult<()> {
    let Some(flags) = AttributeFlags::from_bits(bits) else {
        return Err(invalid(
            definition,
            "attributes",
            "names a bit no attribute has",
        ));
    };
    if flags.is_empty() {
        return Err(invalid(
            definition,
            "attributes",
            "must name at least one attribute; an archetype without one matches no weapon's \
             target flags",
        ));
    }
    if flags.contains(AttributeFlags::STRUCTURE) {
        return Err(invalid(
            definition,
            "attributes",
            "only a structure row has the Structure attribute",
        ));
    }
    Ok(())
}

fn ability_rates(definition: &str, prefix: &str, abilities: &[AbilitySpec]) -> SimResult<()> {
    for (index, ability) in abilities.iter().enumerate() {
        for (field, value) in [
            ("energy_cost", ability.energy_cost),
            ("cast_range", ability.cast_range),
        ] {
            if value < SimScalar::ZERO {
                return Err(invalid(
                    definition,
                    &format!("{prefix}[{index}].{field}"),
                    "must be nonnegative",
                ));
            }
        }
        if let AbilityEffectSpec::BlinkTeleport { max_range, .. } = &ability.effect {
            if *max_range < SimScalar::ZERO {
                return Err(invalid(
                    definition,
                    &format!("{prefix}[{index}].effect.max_range"),
                    "must be nonnegative",
                ));
            }
        }
        if let AbilityEffectSpec::Overclock(spec) = &ability.effect {
            for (field, value) in [
                ("speed_multiplier", spec.speed_multiplier),
                ("hp_cost", spec.hp_cost),
            ] {
                if value < SimScalar::ZERO {
                    return Err(invalid(
                        definition,
                        &format!("{prefix}[{index}].effect.{field}"),
                        "must be nonnegative",
                    ));
                }
            }
            positive_rate(
                definition,
                &format!("{prefix}[{index}].effect.attack_rate_multiplier"),
                spec.attack_rate_multiplier,
            )?;
        }
    }
    Ok(())
}

impl UnitArchetypeDef {
    /// Checks the Overclock capability's rates, when authored: each finite, the HP cost and speed
    /// multiplier nonnegative, and the attack-rate multiplier a positive rate.
    fn validate_overclock(&self) -> SimResult<()> {
        if let Some(overclock) = &self.capabilities.overclock {
            for (field, value) in [
                ("capabilities.overclock.hp_cost", overclock.hp_cost),
                (
                    "capabilities.overclock.speed_multiplier",
                    overclock.speed_multiplier,
                ),
                (
                    "capabilities.overclock.attack_rate_multiplier",
                    overclock.attack_rate_multiplier,
                ),
            ] {
                scalar(&self.slug, field, f64::from(value))?;
            }
            nonnegative(
                &self.slug,
                "capabilities.overclock.speed_multiplier",
                f64::from(overclock.speed_multiplier),
            )?;
            nonnegative(
                &self.slug,
                "capabilities.overclock.hp_cost",
                f64::from(overclock.hp_cost),
            )?;
            positive_rate(
                &self.slug,
                "capabilities.overclock.attack_rate_multiplier",
                SimScalar::from_f32_lossy(overclock.attack_rate_multiplier),
            )?;
        }
        Ok(())
    }

    /// Checks the attributes ([`unit_attributes`]) and that the shape is not a building's: the
    /// structure conversion alone builds that shape, so with [`structure_attributes`] an
    /// archetype has `Structure` exactly when it has a building shape (plan
    /// `2026-11-23-structure-attributes` D-3).
    fn validate_attributes(&self) -> SimResult<()> {
        unit_attributes(&self.slug, self.attributes.bits())?;
        if matches!(self.shape, ShapeKind::BuildingDome { .. }) {
            return Err(invalid(
                &self.slug,
                "shape",
                "only a structure row has a building shape",
            ));
        }
        Ok(())
    }

    /// A barrier absorbs a positive amount, stays raised at least one tick, and cannot be
    /// raised again while it is still up.
    fn validate_barrier(&self) -> SimResult<()> {
        let Some(barrier) = &self.capabilities.barrier else {
            return Ok(());
        };
        positive(
            &self.slug,
            "capabilities.barrier.absorb_limit",
            barrier.absorb_limit,
        )?;
        if barrier.duration_ticks == 0 {
            return Err(invalid(
                &self.slug,
                "capabilities.barrier.duration_ticks",
                "must be positive",
            ));
        }
        if barrier.cooldown_ticks < barrier.duration_ticks {
            return Err(invalid(
                &self.slug,
                "capabilities.barrier.cooldown_ticks",
                "must be at least duration_ticks",
            ));
        }
        Ok(())
    }

    /// A harvester's acquire radius is finite and nonnegative, and its acquired kinds name no
    /// kind twice; an empty list is a worker that never looks for another node (plan
    /// `2026-11-23-harvest-occupancy` D-4).
    fn validate_harvester(&self) -> SimResult<()> {
        let Some(harvester) = &self.capabilities.harvester else {
            return Ok(());
        };
        nonnegative(
            &self.slug,
            "capabilities.harvester.acquire_radius",
            f64::from(harvester.acquire_radius),
        )?;
        for (index, kind) in harvester.acquires.iter().enumerate() {
            if harvester.acquires[..index].contains(kind) {
                return Err(invalid(
                    &self.slug,
                    "capabilities.harvester.acquires",
                    "must not name a resource kind twice",
                ));
            }
        }
        Ok(())
    }

    /// An attack dash speeds its unit up, lasts at least a tick, and reaches some distance;
    /// its minimum gap may be 0 (every target beyond contact).
    fn validate_attack_dash(&self) -> SimResult<()> {
        let Some(dash) = &self.capabilities.attack_dash else {
            return Ok(());
        };
        positive(
            &self.slug,
            "capabilities.attack_dash.speed_multiplier",
            dash.speed_multiplier,
        )?;
        positive(
            &self.slug,
            "capabilities.attack_dash.max_distance",
            dash.max_distance,
        )?;
        nonnegative(
            &self.slug,
            "capabilities.attack_dash.min_gap",
            f64::from(dash.min_gap),
        )?;
        if dash.duration_ticks == 0 {
            return Err(invalid(
                &self.slug,
                "capabilities.attack_dash.duration_ticks",
                "must be positive",
            ));
        }
        Ok(())
    }

    /// Checks an authored terrain radius (`StarCraft` II `InnerRadius`, plan
    /// `2026-11-18-inner-radius` D-2): finite and strictly positive after Q32.32 conversion,
    /// strictly below the collision radius (the game uses `min(InnerRadius, Radius)`, so an equal
    /// or larger value has no effect and is omitted instead), and on a ground mover only (terrain
    /// does not touch an air body). Runs after `collision_radius` is checked positive.
    fn validate_terrain_radius(&self) -> SimResult<()> {
        let Some(terrain_radius) = self.terrain_radius else {
            return Ok(());
        };
        positive(&self.slug, "terrain_radius", terrain_radius)?;
        if SimScalar::from_f32_lossy(terrain_radius)
            >= SimScalar::from_f32_lossy(self.collision_radius)
        {
            return Err(invalid(
                &self.slug,
                "terrain_radius",
                "must be smaller than collision_radius; omit it when equal",
            ));
        }
        if self.mover_type == MoverType::Air {
            return Err(invalid(
                &self.slug,
                "terrain_radius",
                "terrain does not touch air bodies; omit it on an air unit",
            ));
        }
        Ok(())
    }

    /// Checks the hull rate, the stationary hull rate when authored, and `turn_before_move`
    /// against both. Runs after `max_speed` is checked nonnegative.
    fn validate_hull_turning(&self) -> SimResult<()> {
        let hull_rate = self.turn_rate_deg_per_tick;
        rate_per_tick(&self.slug, "turn_rate_deg_per_tick", hull_rate)?;
        let stationary_rate = self.stationary_turn_rate_deg_per_tick.unwrap_or(hull_rate);
        if self.stationary_turn_rate_deg_per_tick.is_some() {
            rate_per_tick(
                &self.slug,
                "stationary_turn_rate_deg_per_tick",
                stationary_rate,
            )?;
        }
        turn_before_move(
            &self.slug,
            self.turn_before_move,
            self.max_speed,
            hull_rate,
            stationary_rate,
        )?;
        Ok(())
    }

    /// Checks each authored weapon slot, then the primary weapon.
    fn validate_weapons(&self) -> SimResult<()> {
        for (name, weapon) in [
            ("weapon", &self.weapon),
            ("ground_weapon", &self.ground_weapon),
            ("air_weapon", &self.air_weapon),
        ] {
            if let Some(weapon) = weapon {
                weapon.validate(&self.slug, name)?;
            }
        }
        self.validate_primary_weapon()
    }

    /// Checks that a named primary weapon is a distinct slot beside another armed one: `Ground`
    /// and `Air` need both a ground weapon (`ground_weapon` or `weapon`) and an `air_weapon`.
    fn validate_primary_weapon(&self) -> SimResult<()> {
        let has_ground = self.ground_weapon.is_some() || self.weapon.is_some();
        let has_air = self.air_weapon.is_some();
        match self.primary_weapon {
            PrimaryWeapon::Either => Ok(()),
            PrimaryWeapon::Ground | PrimaryWeapon::Air if has_ground && has_air => Ok(()),
            PrimaryWeapon::Ground | PrimaryWeapon::Air => Err(invalid(
                &self.slug,
                "primary_weapon",
                "a primary weapon slot needs both a ground and an air weapon; use Either",
            )),
        }
    }

    /// Checks the life and shield regeneration delays against the authored rate and shields.
    /// Runs after the nonnegative checks, so a negative rate is reported under its own field.
    fn validate_regeneration_delays(&self) -> SimResult<()> {
        hp_regen_delay(
            &self.slug,
            self.hp_regen_per_tick > 0.0,
            self.hp_regen_delay_ticks,
        )?;
        shield_regen_delay(
            &self.slug,
            self.max_shields > 0.0,
            self.shield_regen_delay_ticks,
        )
    }

    pub(super) fn validate(&self) -> SimResult<()> {
        validate_slug(&self.slug)?;
        self.validate_attributes()?;
        for (field, value) in [
            ("max_hp", self.max_hp),
            ("max_shields", self.max_shields),
            ("max_energy", self.max_energy),
            ("start_energy", self.start_energy),
            ("life_armor", self.life_armor),
            ("shield_armor", self.shield_armor),
            ("hp_regen_per_tick", self.hp_regen_per_tick),
            ("shield_regen_per_tick", self.shield_regen_per_tick),
            ("energy_regen_per_tick", self.energy_regen_per_tick),
            ("max_speed", self.max_speed),
            ("acceleration", self.acceleration),
            ("deceleration", self.deceleration),
            ("collision_radius", self.collision_radius),
            ("separation_radius", self.separation_radius),
            ("sight_radius", self.sight_radius),
            ("scan_range", self.scan_range),
        ] {
            scalar(&self.slug, field, f64::from(value))?;
        }
        for (field, value) in [
            ("max_hp", self.max_hp),
            ("collision_radius", self.collision_radius),
        ] {
            positive(&self.slug, field, value)?;
        }
        for (field, value) in [
            ("max_shields", self.max_shields),
            ("max_energy", self.max_energy),
            ("start_energy", self.start_energy),
            ("hp_regen_per_tick", self.hp_regen_per_tick),
            ("shield_regen_per_tick", self.shield_regen_per_tick),
            ("energy_regen_per_tick", self.energy_regen_per_tick),
            ("max_speed", self.max_speed),
            ("acceleration", self.acceleration),
            ("deceleration", self.deceleration),
            ("separation_radius", self.separation_radius),
            ("scan_range", self.scan_range),
        ] {
            nonnegative(&self.slug, field, f64::from(value))?;
        }
        sight(&self.slug, "sight_radius", f64::from(self.sight_radius))?;
        start_energy(
            &self.slug,
            f64::from(self.start_energy),
            f64::from(self.max_energy),
        )?;
        energy_regen(
            &self.slug,
            f64::from(self.max_energy),
            f64::from(self.energy_regen_per_tick),
        )?;
        self.validate_terrain_radius()?;
        self.validate_hull_turning()?;
        self.validate_regeneration_delays()?;
        self.validate_weapons()?;
        self.validate_barrier()?;
        self.validate_harvester()?;
        self.validate_attack_dash()?;
        self.validate_overclock()?;
        ability_rates(&self.slug, "abilities", &self.abilities)?;
        ability_rates(
            &self.slug,
            "capabilities.abilities",
            &self.capabilities.abilities,
        )?;
        for mode in self.modes.iter().chain(&self.capabilities.modes) {
            mode.validate(&self.slug)?;
        }
        self.validate_mode_abilities()
    }

    /// A mode toggle switches one of the archetype's own modes and names the command that
    /// leaves it; no other ability names one (plan `2026-11-09-client-mode-commands` D-2).
    ///
    /// The client builds its command card from these abilities, so a toggle it could not
    /// label or switch would be a button that does nothing. Each ability list is checked
    /// against the modes the archetype will hold: `capabilities.modes` when authored,
    /// otherwise `modes`, as `UnitArchetypeDef::capabilities` resolves them.
    fn validate_mode_abilities(&self) -> SimResult<()> {
        let modes = if self.capabilities.modes.is_empty() {
            &self.modes
        } else {
            &self.capabilities.modes
        };
        for (prefix, abilities) in [
            ("abilities", &self.abilities),
            ("capabilities.abilities", &self.capabilities.abilities),
        ] {
            for (index, ability) in abilities.iter().enumerate() {
                mode_ability(&self.slug, &format!("{prefix}[{index}]"), ability, modes)?;
            }
        }
        Ok(())
    }
}

/// Checks the mode-toggle rule of [`UnitArchetypeDef::validate_mode_abilities`] for one ability
/// at `field` of `definition`, whose archetype holds `modes`.
fn mode_ability(
    definition: &str,
    field: &str,
    ability: &AbilitySpec,
    modes: &[UnitModeSpecDef],
) -> SimResult<()> {
    if !matches!(ability.target_kind, AbilityTargetKind::ToggleMode) {
        return if ability.leave_name.is_some() {
            Err(invalid(
                definition,
                &format!("{field}.leave_name"),
                "only a ToggleMode ability names a leave command",
            ))
        } else {
            Ok(())
        };
    }
    let AbilityEffectSpec::SwitchMode { mode_name } = &ability.effect else {
        return Err(invalid(
            definition,
            &format!("{field}.effect"),
            "a ToggleMode ability must have a SwitchMode effect",
        ));
    };
    if !modes.iter().any(|mode| mode.mode_name == *mode_name) {
        return Err(invalid(
            definition,
            &format!("{field}.effect.mode_name"),
            "must name a mode of the same archetype",
        ));
    }
    if ability
        .leave_name
        .as_deref()
        .is_none_or(|name| name.trim().is_empty())
    {
        return Err(invalid(
            definition,
            &format!("{field}.leave_name"),
            "a ToggleMode ability must name its leave command",
        ));
    }
    Ok(())
}

impl WeaponSpecDef {
    /// Checks the attack cycle: at least one strike, at least one tick, and a period longer
    /// than the damage point.
    fn validate_cadence(&self, definition: &str, prefix: &str) -> SimResult<()> {
        if self.attacks_count == 0 {
            return Err(invalid(
                definition,
                &format!("{prefix}.attacks_count"),
                "a weapon must strike at least once per attack cycle",
            ));
        }
        if self.period_ticks == 0 {
            return Err(invalid(
                definition,
                &format!("{prefix}.period_ticks"),
                "an attack cycle must last at least one tick",
            ));
        }
        // The cooldown a release charges is `period - damage point`. Without at least one tick
        // of it the weapon would have no cooldown to speak of, and a release could not be told
        // from the weapon's state (the cooldown rises only at a release).
        if self.period_ticks <= self.damage_point_ticks {
            return Err(invalid(
                definition,
                &format!("{prefix}.period_ticks"),
                "must be longer than damage_point_ticks",
            ));
        }
        Ok(())
    }

    /// Checks the random delay `(min, max)` in ticks: both finite fixed-point values, `min <=
    /// max`, and `min` greater than minus the cooldown a release charges (`period − damage
    /// point`), so no draw can take the whole cooldown away (B-3).
    fn validate_random_delay(&self, definition: &str, prefix: &str) -> SimResult<()> {
        let field = &format!("{prefix}.random_delay_ticks");
        let (min, max) = self.random_delay_ticks;
        scalar(definition, field, f64::from(min))?;
        scalar(definition, field, f64::from(max))?;
        if min > max {
            return Err(invalid(definition, field, "min must not exceed max"));
        }
        let cooldown = f64::from(self.period_ticks) - f64::from(self.damage_point_ticks);
        if f64::from(min) <= -cooldown {
            return Err(invalid(
                definition,
                field,
                "min must leave part of the period − damage point cooldown",
            ));
        }
        Ok(())
    }

    /// Checks the spacing of the strikes of one attack: an interval needs a second strike, the
    /// whole burst ends before the next attack can start, and a follow-up distance belongs to a
    /// spaced burst and is positive.
    fn validate_strikes(&self, definition: &str, prefix: &str) -> SimResult<()> {
        let interval = &format!("{prefix}.strike_interval_ticks");
        if self.strike_interval_ticks > 0 && self.attacks_count < 2 {
            return Err(invalid(
                definition,
                interval,
                "a single-strike weapon has no interval between strikes; author 0",
            ));
        }
        // The last strike lands (attacks_count - 1) intervals after the release; the next release
        // comes a whole period later at the earliest, so a burst never overlaps the next one.
        let burst =
            u32::from(self.attacks_count.saturating_sub(1)) * u32::from(self.strike_interval_ticks);
        if burst >= u32::from(self.period_ticks) {
            return Err(invalid(
                definition,
                interval,
                "the last strike must land before the attack period ends",
            ));
        }
        if let Some(distance) = self.followup_max_distance {
            let field = &format!("{prefix}.followup_max_distance");
            if self.strike_interval_ticks == 0 {
                return Err(invalid(
                    definition,
                    field,
                    "only a strike after the first, on a later tick, has a distance condition",
                ));
            }
            positive(definition, field, distance)?;
        }
        Ok(())
    }

    /// Checks the attribute bonuses: each finite and nonnegative, at most one per attribute.
    fn validate_attribute_bonuses(&self, definition: &str, prefix: &str) -> SimResult<()> {
        let field = &format!("{prefix}.attribute_bonuses");
        for (index, &(attribute, bonus)) in self.attribute_bonuses.iter().enumerate() {
            nonnegative(definition, field, f64::from(bonus))?;
            if self.attribute_bonuses[..index]
                .iter()
                .any(|&(earlier, _)| earlier == attribute)
            {
                return Err(invalid(
                    definition,
                    &format!("{field}.{attribute:?}"),
                    "listed twice; a weapon has one bonus per attribute",
                ));
            }
        }
        Ok(())
    }

    /// Checks the two firing arcs, half angles in degrees: the engaged arc lies in `0..=360`, and
    /// the acquisition arc (`Arc` ÷ 2) in `0..=firing_arc_deg`, because `ArcSlop` only widens it.
    fn validate_arcs(&self, definition: &str, prefix: &str) -> SimResult<()> {
        let engaged = format!("{prefix}.firing_arc_deg");
        angle(definition, &engaged, self.firing_arc_deg)?;
        if !(0.0..=360.0).contains(&self.firing_arc_deg) {
            return Err(invalid(definition, &engaged, "must lie in 0..=360 degrees"));
        }
        let acquisition = format!("{prefix}.acquire_arc_deg");
        angle(definition, &acquisition, self.acquire_arc_deg)?;
        if !(0.0..=self.firing_arc_deg).contains(&self.acquire_arc_deg) {
            return Err(invalid(
                definition,
                &acquisition,
                "must lie in 0..=firing_arc_deg degrees: the engaged arc only widens it",
            ));
        }
        Ok(())
    }

    /// Checks the turret's idle behaviour: a hull weapon has no turret to move and authors
    /// `Hold`; a turret's idle rate lies in `0 < |rate| <= 180` degrees per tick, and only `Spin`
    /// may turn clockwise (a negative rate).
    fn validate_turret_idle(&self, definition: &str, prefix: &str) -> SimResult<()> {
        let field = format!("{prefix}.turret_idle");
        if !self.has_turret && self.turret_idle != TurretIdleDef::Hold {
            return Err(invalid(
                definition,
                &field,
                "must be Hold on a weapon without a turret",
            ));
        }
        match self.turret_idle {
            TurretIdleDef::Spin(rate) => rate_per_tick(definition, &field, rate.abs()),
            idle => idle
                .rate()
                .map_or(Ok(()), |rate| rate_per_tick(definition, &field, rate)),
        }
    }

    /// Checks the turret rate: a turreted weapon turns at a rate in `0 < rate <= 180` degrees
    /// per tick; a hull-mounted weapon turns with the hull and authors exactly 0.
    fn validate_turret(&self, definition: &str, prefix: &str) -> SimResult<()> {
        self.validate_turret_idle(definition, prefix)?;
        let field = format!("{prefix}.turret_turn_rate_deg_per_tick");
        if self.has_turret {
            return rate_per_tick(definition, &field, self.turret_turn_rate_deg_per_tick);
        }
        angle(definition, &field, self.turret_turn_rate_deg_per_tick)?;
        if self.turret_turn_rate_deg_per_tick != 0.0 {
            return Err(invalid(
                definition,
                &field,
                "must be 0 on a weapon without a turret, which turns with the hull",
            ));
        }
        Ok(())
    }

    fn validate(&self, definition: &str, prefix: &str) -> SimResult<()> {
        for (field, value) in [
            ("min_range", self.min_range),
            ("max_range", self.max_range),
            ("base_damage", self.base_damage),
            ("upgrade_bonus_per_level", self.upgrade_bonus_per_level),
            ("bonus_upgrade_per_level", self.bonus_upgrade_per_level),
        ] {
            scalar(definition, &format!("{prefix}.{field}"), f64::from(value))?;
        }
        for (field, value) in [
            ("min_range", self.min_range),
            ("max_range", self.max_range),
            ("base_damage", self.base_damage),
        ] {
            nonnegative(definition, &format!("{prefix}.{field}"), f64::from(value))?;
        }
        if self.min_range > self.max_range {
            return Err(invalid(
                definition,
                &format!("{prefix}.min_range"),
                "must not exceed max_range",
            ));
        }
        self.validate_cadence(definition, prefix)?;
        self.validate_random_delay(definition, prefix)?;
        self.validate_strikes(definition, prefix)?;
        self.validate_attribute_bonuses(definition, prefix)?;
        if let Some(strike) = &self.structure_strike {
            if strike.damage <= SimScalar::ZERO || strike.upgrade_per_level < SimScalar::ZERO {
                return Err(invalid(
                    definition,
                    &format!("{prefix}.structure_strike"),
                    "damage must be positive and upgrade_per_level not negative",
                ));
            }
        }
        self.validate_arcs(definition, prefix)?;
        self.validate_turret(definition, prefix)?;
        // B-123 (plan `2026-11-17-effect-snapshot-identity` D-7): the delivery path is chosen by
        // the speed, and an instant hit publishes its event by `visual_kind`, which has no event
        // for `Projectile`: such a weapon would fire with no look at all.
        if self.projectile_speed.is_none() && self.visual_kind == WeaponVisualKind::Projectile {
            return Err(invalid(
                definition,
                &format!("{prefix}.visual_kind"),
                "an instant weapon publishes no projectile",
            ));
        }
        if let Some(speed) = self.projectile_speed {
            positive(definition, &format!("{prefix}.projectile_speed"), speed)?;
            if self.max_projectile_lifetime_ticks == 0 {
                return Err(invalid(
                    definition,
                    &format!("{prefix}.max_projectile_lifetime_ticks"),
                    "a projectile must live for at least one tick",
                ));
            }
        }
        if let ProjectileTargetLossBehavior::RetargetNearest { search_radius } =
            self.target_loss_behavior
        {
            if search_radius < SimScalar::ZERO {
                return Err(invalid(
                    definition,
                    &format!("{prefix}.target_loss_behavior.search_radius"),
                    "must be nonnegative",
                ));
            }
        }
        if let Some(splash) = &self.splash {
            splash.validate(definition, &format!("{prefix}.splash"))?;
        }
        Ok(())
    }
}

impl SplashSpecDef {
    /// Checks the zones: at least one; each radius finite and nonnegative and larger than the
    /// one before; each fraction in `0 < fraction <= 1`, the share of a strike a zone can deal.
    /// A line needs at least one search point, each offset a finite fixed-point value.
    fn validate(&self, definition: &str, prefix: &str) -> SimResult<()> {
        if self.zones.is_empty() {
            return Err(invalid(
                definition,
                &format!("{prefix}.zones"),
                "a splash needs at least one zone",
            ));
        }
        for (index, zone) in self.zones.iter().enumerate() {
            let field = format!("{prefix}.zones[{index}]");
            nonnegative(
                definition,
                &format!("{field}.radius"),
                f64::from(zone.radius),
            )?;
            if index > 0 && zone.radius <= self.zones[index - 1].radius {
                return Err(invalid(
                    definition,
                    &format!("{field}.radius"),
                    "splash zone radii must increase from one zone to the next",
                ));
            }
            positive(definition, &format!("{field}.fraction"), zone.fraction)?;
            if zone.fraction > 1.0 {
                return Err(invalid(
                    definition,
                    &format!("{field}.fraction"),
                    "a splash zone deals at most the whole strike",
                ));
            }
        }
        if let SplashShapeDef::Line { offsets, .. } = &self.shape {
            if offsets.is_empty() {
                return Err(invalid(
                    definition,
                    &format!("{prefix}.shape.offsets"),
                    "a line splash needs at least one search point",
                ));
            }
            for (index, &offset) in offsets.iter().enumerate() {
                scalar(
                    definition,
                    &format!("{prefix}.shape.offsets[{index}]"),
                    f64::from(offset),
                )?;
            }
        }
        Ok(())
    }
}

impl UnitModeSpecDef {
    fn validate(&self, definition: &str) -> SimResult<()> {
        let prefix = format!("modes.{}", self.mode_name);
        for (field, value) in [
            (
                "cast_random_delay_max_ticks",
                self.cast_random_delay_max_ticks,
            ),
            (
                "leave_random_delay_max_ticks",
                self.leave_random_delay_max_ticks,
            ),
        ] {
            let field = format!("{prefix}.{field}");
            nonnegative(definition, &field, f64::from(value))?;
            if f64::from(value) > f64::from(u16::MAX) {
                return Err(invalid(definition, &field, "must not exceed 65535 ticks"));
            }
        }
        scalar(
            definition,
            &format!("{prefix}.energy_cost_per_second"),
            f64::from(self.energy_cost_per_second),
        )?;
        if let Some(radius) = self.sight_radius_override {
            sight(
                definition,
                &format!("{prefix}.sight_radius_override"),
                f64::from(radius),
            )?;
        }
        for (name, weapon) in [
            ("ground_weapon_override", &self.ground_weapon_override),
            ("air_weapon_override", &self.air_weapon_override),
        ] {
            if let Some(Some(weapon)) = weapon {
                weapon.validate(definition, &format!("{prefix}.{name}"))?;
            }
        }
        // The simulation applies a mode's stance flags and a replacement ground weapon. Data
        // asking for anything else must fail here rather than silently do nothing in a match.
        let unsupported = [
            ("mover_type_override", self.mover_type_override.is_some()),
            (
                "sight_radius_override",
                self.sight_radius_override.is_some(),
            ),
            ("air_weapon_override", self.air_weapon_override.is_some()),
            (
                "ground_weapon_override",
                matches!(self.ground_weapon_override, Some(None)),
            ),
        ];
        for (name, authored) in unsupported {
            if authored {
                return Err(invalid(
                    definition,
                    &format!("{prefix}.{name}"),
                    "this mode override has no simulation effect",
                ));
            }
        }
        Ok(())
    }
}

impl StructureArchetypeDef {
    /// Checks the authored footprint shape (plan `2026-11-18-contour-footprints` D-2): every value
    /// finite and a multiple of 1/4 world unit, so it is exact in quarter cells and every vertex
    /// lies on the navigation mesh's lattice; `0 < half_width ≤ width / 2` and
    /// `0 < half_height ≤ height / 2`, so the shape lies in its cells; and
    /// `0 ≤ chamfer < min(half_width, half_height)`, so every face keeps a positive length. Runs
    /// after the footprint is checked.
    fn validate_footprint_shape(&self) -> SimResult<()> {
        let shape = self.footprint_shape;
        let refuse = |reason: &'static str| Err(invalid(&self.slug, "footprint_shape", reason));
        for value in [shape.half_width, shape.half_height, shape.chamfer] {
            if !value.is_finite() || (value * 4.0).fract() != 0.0 {
                return refuse("every value must be a finite multiple of 1/4 world unit");
            }
        }
        let (width, height) = (f64::from(self.footprint.0), f64::from(self.footprint.1));
        if shape.half_width <= 0.0
            || shape.half_height <= 0.0
            || shape.half_width > width * 0.5
            || shape.half_height > height * 0.5
        {
            return refuse("the shape must be a nonempty box inside the footprint's cells");
        }
        if shape.chamfer < 0.0 || shape.chamfer >= shape.half_width.min(shape.half_height) {
            return refuse("the chamfer must be shorter than the smaller half-extent");
        }
        Ok(())
    }

    /// Checks the authored structure radius (plan `2026-11-18-structure-radius` D-2): finite and
    /// strictly positive; a multiple of 1/16 world unit, as every 5.0.14 engine radius is, so it
    /// converts exactly; and covering the footprint shape, `radius² ≥ max(hw² + (hh − c)²,
    /// (hw − c)² + hh²)`, so a unit touching any point of the shape is within reach of a melee
    /// weapon (plan `2026-11-18-contour-footprints` D-10, replacing that plan's floor at half the
    /// footprint). Runs after the footprint and its shape are checked.
    fn validate_radius(&self) -> SimResult<()> {
        scalar(&self.slug, "radius", self.radius)?;
        if self.radius <= 0.0 {
            return Err(invalid(&self.slug, "radius", "must be strictly positive"));
        }
        if (self.radius * 16.0).fract() != 0.0 {
            return Err(invalid(
                &self.slug,
                "radius",
                "must be a multiple of 1/16 world unit",
            ));
        }
        // The farthest vertex of the footprint shape, squared. Every value is a multiple of 1/4
        // and the radius of 1/16 (checked above and in `validate_footprint_shape`), so these
        // squares are exact in `f64`.
        let shape = self.footprint_shape;
        let (hw, hh, c) = (shape.half_width, shape.half_height, shape.chamfer);
        let farthest = (hw * hw + (hh - c) * (hh - c)).max((hw - c) * (hw - c) + hh * hh);
        if self.radius * self.radius < farthest {
            return Err(invalid(
                &self.slug,
                "radius",
                "must cover the footprint shape, so a unit touching any point of it is within \
                 reach",
            ));
        }
        Ok(())
    }

    /// Checks the authored auto-rally (plan `2026-11-23-production-queue` D-7): a finite,
    /// nonnegative radius, no resource kind twice, and only on a structure that trains units,
    /// since only those have a rally (D-3).
    fn validate_auto_rally(&self) -> SimResult<()> {
        let Some(rally) = &self.auto_rally else {
            return Ok(());
        };
        nonnegative(&self.slug, "auto_rally.radius", rally.radius)?;
        if rally
            .kinds
            .iter()
            .enumerate()
            .any(|(index, kind)| rally.kinds[..index].contains(kind))
        {
            return Err(invalid(
                &self.slug,
                "auto_rally.kinds",
                "must not name a resource kind twice",
            ));
        }
        if self.trains_units.is_empty() {
            return Err(invalid(
                &self.slug,
                "auto_rally",
                "only a structure that trains units has a rally",
            ));
        }
        Ok(())
    }

    pub(super) fn validate(&self) -> SimResult<()> {
        validate_slug(&self.slug)?;
        structure_attributes(&self.slug, &self.attributes)?;
        self.validate_auto_rally()?;
        nonnegative(&self.slug, "hp_regen_per_tick", self.hp_regen_per_tick)?;
        hp_regen_delay(
            &self.slug,
            self.hp_regen_per_tick > 0.0,
            self.hp_regen_delay_ticks,
        )?;
        nonnegative(
            &self.slug,
            "shield_regen_per_tick",
            self.shield_regen_per_tick,
        )?;
        ability_rates(&self.slug, "abilities", &self.abilities)?;
        if u16::try_from(self.structure_type_id).is_err() {
            return Err(invalid(
                &self.slug,
                "structure_type_id",
                "must fit the unsigned 16-bit UnitTypeId",
            ));
        }
        if self.footprint.0 == 0
            || self.footprint.1 == 0
            || u8::try_from(self.footprint.0).is_err()
            || u8::try_from(self.footprint.1).is_err()
        {
            return Err(invalid(
                &self.slug,
                "footprint",
                "width and height must be nonzero unsigned 8-bit cell counts",
            ));
        }
        for (field, value) in [
            ("max_hp", self.max_hp),
            ("max_shields", self.max_shields),
            ("max_energy", self.max_energy),
            ("start_energy", self.start_energy),
            ("energy_regen_per_tick", self.energy_regen_per_tick),
            ("life_armor", self.life_armor),
            ("shield_armor", self.shield_armor),
            ("sight_radius", self.sight_radius),
        ] {
            scalar(&self.slug, field, value)?;
        }
        for (field, value) in [
            ("max_hp", self.max_hp),
            ("max_shields", self.max_shields),
            ("max_energy", self.max_energy),
            ("start_energy", self.start_energy),
            ("energy_regen_per_tick", self.energy_regen_per_tick),
        ] {
            nonnegative(&self.slug, field, value)?;
        }
        self.validate_footprint_shape()?;
        self.validate_radius()?;
        sight(&self.slug, "sight_radius", self.sight_radius)?;
        structure_scan_range(
            &self.slug,
            self.ground_weapon.is_some() || self.air_weapon.is_some(),
            self.scan_range,
        )?;
        start_energy(&self.slug, self.start_energy, self.max_energy)?;
        energy_regen(&self.slug, self.max_energy, self.energy_regen_per_tick)?;
        shield_regen_delay(
            &self.slug,
            self.max_shields > 0.0,
            self.shield_regen_delay_ticks,
        )?;
        if self.max_hp <= 0.0 {
            return Err(invalid(&self.slug, "max_hp", "must be strictly positive"));
        }
        for (name, weapon) in [
            ("ground_weapon", &self.ground_weapon),
            ("air_weapon", &self.air_weapon),
        ] {
            if let Some(weapon) = weapon {
                weapon.validate(&self.slug, name)?;
            }
        }
        distinct_flags(&self.slug, "placement_excludes", &self.placement_excludes)?;
        Ok(())
    }
}

/// Refuses a flag list that names a flag twice (plan `2026-11-19-footprint-flags` D-2).
fn distinct_flags(definition: &str, field: &str, flags: &[PlacementFlag]) -> SimResult<()> {
    if flags
        .iter()
        .enumerate()
        .any(|(index, flag)| flags[..index].contains(flag))
    {
        return Err(invalid(definition, field, "must not name a flag twice"));
    }
    Ok(())
}

impl PlacementRulesFile {
    /// Checks every resource row (plans `2026-11-19-footprint-flags` D-2 and
    /// `2026-11-20-resource-footprints` D-1): each kind exactly once, a rectangular mask of `.`,
    /// `x` and `o` at most [`MAX_REACH_SIDE`] cells a side whose `x` cells form one rectangle of at
    /// most [`MAX_RESOURCE_SIDE`] cells a side holding the anchor, a shape that lies in those cells
    /// (an outline simple, 3 to 16 vertices on the quarter lattice, edges along axes and
    /// diagonals), a non-empty list of distinct applied flags without the engine's `Fogged`, and
    /// a harvest of at least one tick and one unit (plan `2026-11-22-harvest-timing` D-2).
    pub(super) fn validate(&self) -> SimResult<()> {
        for (index, ring) in self.resource_footprints.iter().enumerate() {
            let definition = format!("{:?}", ring.kind);
            if self.resource_footprints[..index]
                .iter()
                .any(|earlier| earlier.kind == ring.kind)
            {
                return Err(invalid(&definition, "kind", "must have at most one row"));
            }
            ring.validate(&definition)?;
        }
        // Every resource kind has a row: a node of a kind without one could not be placed.
        for kind in [ResourceKind::Mineral, ResourceKind::Gas] {
            if !self.resource_footprints.iter().any(|row| row.kind == kind) {
                return Err(invalid(
                    &format!("{kind:?}"),
                    "resource_footprints",
                    "every resource kind must have a row",
                ));
            }
        }
        Ok(())
    }
}

impl ResourceFootprintDef {
    fn validate(&self, definition: &str) -> SimResult<()> {
        if self.applies.is_empty() {
            return Err(invalid(
                definition,
                "applies",
                "must name at least one flag",
            ));
        }
        distinct_flags(definition, "applies", &self.applies)?;
        if self.applies.contains(&PlacementFlag::Fogged) {
            return Err(invalid(
                definition,
                "applies",
                "must not apply Fogged, a state of the placing player's vision",
            ));
        }
        self.validate_rows(definition)?;
        self.validate_own_cells(definition)?;
        self.validate_shape(definition)?;
        self.validate_harvest(definition)
    }

    /// The harvest: a gather that takes no time or yields nothing would loop forever without
    /// earning. A return delay of 0 is no wait.
    fn validate_harvest(&self, definition: &str) -> SimResult<()> {
        if self.harvest_time_ticks == 0 {
            return Err(invalid(
                definition,
                "harvest_time_ticks",
                "must be at least 1 tick",
            ));
        }
        if self.harvest_amount == 0 {
            return Err(invalid(definition, "harvest_amount", "must be positive"));
        }
        Ok(())
    }

    /// The mask: 1 to 15 rows of equal length 1 to 15, of `.`, `x` and `o` only.
    fn validate_rows(&self, definition: &str) -> SimResult<()> {
        let width = self.rows.first().map_or(0, |row| row.chars().count());
        if width == 0 || self.rows.len() > MAX_REACH_SIDE || width > MAX_REACH_SIDE {
            return Err(invalid(
                definition,
                "rows",
                "must hold 1 to 15 rows of 1 to 15 cells",
            ));
        }
        if self.rows.iter().any(|row| row.chars().count() != width) {
            return Err(invalid(definition, "rows", "must all have the same length"));
        }
        if self
            .rows
            .iter()
            .any(|row| row.chars().any(|mark| !matches!(mark, '.' | 'x' | 'o')))
        {
            return Err(invalid(
                definition,
                "rows",
                "must hold only `.`, `x` and `o`",
            ));
        }
        Ok(())
    }

    /// The own cells: at least one `x`, all of them one rectangle of at most 4 × 4, and the
    /// anchor on one of them.
    fn validate_own_cells(&self, definition: &str) -> SimResult<()> {
        let Some((_, _, width, height)) = self.own_rectangle() else {
            return Err(invalid(
                definition,
                "rows",
                "must hold the node's own cells, `x`",
            ));
        };
        let filled = self.marked('x').count() == width * height;
        if !filled || width > MAX_RESOURCE_SIDE || height > MAX_RESOURCE_SIDE {
            return Err(invalid(
                definition,
                "rows",
                "the own cells `x` must form one rectangle of at most 4 x 4 cells",
            ));
        }
        let anchored = usize::try_from(self.anchor.1)
            .ok()
            .and_then(|row| self.rows.get(row))
            .zip(usize::try_from(self.anchor.0).ok())
            .and_then(|(row, column)| row.chars().nth(column));
        match anchored {
            Some('x') => {}
            Some(_) => return Err(invalid(definition, "anchor", "must name an `x` cell")),
            None => return Err(invalid(definition, "anchor", "must lie inside the rows")),
        }
        Ok(())
    }

    /// The shape: every value a finite multiple of 1/4, and the resolved shape inside the own
    /// cells by `sim_map`'s placement rule (a box nonempty with a chamfer shorter than its smaller
    /// half extent; an outline simple, 3 to 16 vertices, edges along axes and diagonals).
    fn validate_shape(&self, definition: &str) -> SimResult<()> {
        if self
            .shape
            .values()
            .iter()
            .any(|value| !value.is_finite() || (value * 4.0).fract() != 0.0)
        {
            return Err(invalid(
                definition,
                "shape",
                "every value must be a finite multiple of 1/4 world unit",
            ));
        }
        let (_, _, width, height) = self.own_rectangle().unwrap_or((0, 0, 0, 0));
        let fits = self.shape.resolve().is_some_and(|shape| {
            sim_map::StructureFootprint {
                width: u8::try_from(width).unwrap_or(0),
                height: u8::try_from(height).unwrap_or(0),
                shape,
            }
            .validate()
            .is_ok()
        });
        if fits {
            Ok(())
        } else {
            Err(invalid(
                definition,
                "shape",
                "must lie in the own cells: a nonempty box, or a simple outline of 3 to 16 \
                 vertices with every edge along an axis or a diagonal",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{angle, rate_per_tick, scalar, validate_slug};

    #[test]
    fn authored_overclock_rates_reject_invalid_values_in_every_ability_location(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::{CatalogFile, StructureCatalogFile};
        use sim_types::{
            AbilityEffectSpec, AbilitySpec, AbilityTargetKind, OverclockSpec, SimScalar,
        };
        let units = include_str!("../../../../assets/catalog/units.ron");
        let structures = include_str!("../../../../assets/catalog/structures.ron");
        for rate in [0.0_f32, -1.0, 1e-30] {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file
                .units
                .iter_mut()
                .find(|unit| unit.capabilities.overclock.is_some())
                .ok_or("real catalog has no overclock capability")?;
            unit.capabilities
                .overclock
                .as_mut()
                .ok_or("capability missing")?
                .attack_rate_multiplier = rate;
            assert!(
                unit.validate().is_err(),
                "rate {rate} must fail after quantization"
            );
        }
        for rate in [SimScalar::ZERO, -SimScalar::ONE] {
            let ability = AbilitySpec {
                name: "validation-fixture".into(),
                target_kind: AbilityTargetKind::InstantSelf,
                energy_cost: SimScalar::ZERO,
                cooldown_ticks: 1,
                cast_range: SimScalar::ZERO,
                effect: AbilityEffectSpec::Overclock(OverclockSpec {
                    hp_cost: SimScalar::ZERO,
                    duration_ticks: 1,
                    speed_multiplier: SimScalar::ONE,
                    attack_rate_multiplier: rate,
                }),
                leave_name: None,
                cast_backswing_ticks: 0,
                single_caster: true,
                target_filter: None,
                origin: None,
            };
            for capability_list in [false, true] {
                let mut file: CatalogFile = ron::from_str(units)?;
                let unit = file.units.first_mut().ok_or("unit catalog empty")?;
                if capability_list {
                    unit.capabilities.abilities.push(ability.clone());
                } else {
                    unit.abilities.push(ability.clone());
                }
                assert!(unit.validate().is_err());
            }
            let mut file: StructureCatalogFile = ron::from_str(structures)?;
            let structure = file
                .structures
                .first_mut()
                .ok_or("structure catalog empty")?;
            structure.abilities.push(ability);
            assert!(structure.validate().is_err());
        }
        Ok(())
    }

    /// Asserts that `result` is the typed invalid-value error naming `expected_field`, a
    /// regeneration delay or rate field.
    fn assert_rejects_delay_field(
        result: &crate::error::SimResult<()>,
        expected_field: &str,
        case: &str,
    ) {
        let field = match result {
            Err(crate::error::SimCoreError::CatalogInvalidValue { field, .. }) => Some(field),
            _ => None,
        };
        assert_eq!(
            field.map(String::as_str),
            Some(expected_field),
            "{case}: expected a typed delay error, got {result:?}"
        );
    }

    /// Asserts that `result` is the typed invalid-value error naming the shield delay field.
    fn assert_rejects_delay(result: &crate::error::SimResult<()>, case: &str) {
        assert_rejects_delay_field(result, "shield_regen_delay_ticks", case);
    }

    #[test]
    fn a_unit_has_a_shield_regeneration_delay_exactly_when_it_has_shields(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::CatalogFile;
        let units = include_str!("../../../../assets/catalog/units.ron");
        let mut file: CatalogFile = ron::from_str(units)?;
        for unit in &file.units {
            unit.validate()?;
        }
        let shielded = file
            .units
            .iter_mut()
            .find(|unit| unit.max_shields > 0.0)
            .ok_or("real catalog has no shielded unit")?;
        shielded.shield_regen_delay_ticks = None;
        assert_rejects_delay(&shielded.validate(), "shielded unit without a delay");
        let unshielded = file
            .units
            .iter_mut()
            .find(|unit| unit.max_shields == 0.0)
            .ok_or("real catalog has no unshielded unit")?;
        unshielded.shield_regen_delay_ticks = Some(10);
        assert_rejects_delay(&unshielded.validate(), "unshielded unit with a delay");
        Ok(())
    }

    /// Plan `2026-11-23-production-queue` D-7: an auto-rally has a finite, nonnegative radius,
    /// names each kind once, and sits only on a structure that trains units. Each refusal names
    /// its field.
    #[test]
    fn an_auto_rally_is_refused_unless_its_radius_kinds_and_owner_are_valid(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::{AutoRallySpecDef, StructureCatalogFile};
        use sim_types::ResourceKind;
        let structures = include_str!("../../../../assets/catalog/structures.ron");
        let mut file: StructureCatalogFile = ron::from_str(structures)?;
        let trainer = file
            .structures
            .iter()
            .position(|structure| !structure.trains_units.is_empty())
            .ok_or("real catalog has no structure that trains units")?;
        let idle = file
            .structures
            .iter()
            .position(|structure| structure.trains_units.is_empty())
            .ok_or("real catalog has no structure that trains nothing")?;
        let cases = [
            (
                trainer,
                -0.5,
                vec![ResourceKind::Mineral],
                Some("auto_rally.radius"),
            ),
            (
                trainer,
                f64::NAN,
                vec![ResourceKind::Mineral],
                Some("auto_rally.radius"),
            ),
            (
                trainer,
                6.5,
                vec![ResourceKind::Mineral, ResourceKind::Mineral],
                Some("auto_rally.kinds"),
            ),
            (idle, 6.5, vec![ResourceKind::Mineral], Some("auto_rally")),
            (trainer, 0.0, vec![], None),
            (
                trainer,
                6.5,
                vec![ResourceKind::Mineral, ResourceKind::Gas],
                None,
            ),
        ];
        for (row, radius, kinds, rejected) in cases {
            let case = format!("row {row}, {radius} {kinds:?}");
            let structure = &mut file.structures[row];
            structure.auto_rally = Some(AutoRallySpecDef { radius, kinds });
            let refused = match structure.validate() {
                Ok(()) => None,
                Err(crate::error::SimCoreError::CatalogInvalidValue { field, .. }) => Some(field),
                Err(other) => return Err(format!("{case}: {other}").into()),
            };
            assert_eq!(refused.as_deref(), rejected, "{case}");
            structure.auto_rally = None;
        }
        Ok(())
    }

    #[test]
    fn a_structure_has_a_shield_regeneration_delay_exactly_when_it_has_shields(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::StructureCatalogFile;
        let structures = include_str!("../../../../assets/catalog/structures.ron");
        let mut file: StructureCatalogFile = ron::from_str(structures)?;
        for structure in &file.structures {
            structure.validate()?;
        }
        let shielded = file
            .structures
            .iter_mut()
            .find(|structure| structure.max_shields > 0.0)
            .ok_or("real catalog has no shielded structure")?;
        shielded.shield_regen_delay_ticks = None;
        assert_rejects_delay(&shielded.validate(), "shielded structure without a delay");
        let unshielded = file
            .structures
            .iter_mut()
            .find(|structure| structure.max_shields == 0.0)
            .ok_or("real catalog has no unshielded structure")?;
        unshielded.shield_regen_delay_ticks = Some(10);
        assert_rejects_delay(&unshielded.validate(), "unshielded structure with a delay");
        Ok(())
    }

    #[test]
    fn a_unit_without_life_regeneration_rejects_a_life_regeneration_delay(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::CatalogFile;
        let units = include_str!("../../../../assets/catalog/units.ron");
        let mut file: CatalogFile = ron::from_str(units)?;
        for unit in &file.units {
            unit.validate()?;
        }
        let static_unit = file
            .units
            .iter_mut()
            .find(|unit| unit.hp_regen_per_tick == 0.0)
            .ok_or("real catalog has no unit without life regeneration")?;
        static_unit.hp_regen_delay_ticks = 10;
        assert_rejects_delay_field(
            &static_unit.validate(),
            "hp_regen_delay_ticks",
            "unit without life regeneration with a delay",
        );
        let regenerating = file
            .units
            .iter_mut()
            .find(|unit| unit.hp_regen_per_tick > 0.0)
            .ok_or("real catalog has no unit with life regeneration")?;
        regenerating.hp_regen_delay_ticks = 10;
        regenerating.validate()?;
        Ok(())
    }

    #[test]
    fn a_structure_without_life_regeneration_rejects_a_life_regeneration_delay(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::StructureCatalogFile;
        let structures = include_str!("../../../../assets/catalog/structures.ron");
        let mut file: StructureCatalogFile = ron::from_str(structures)?;
        for structure in &file.structures {
            structure.validate()?;
        }
        let static_structure = file
            .structures
            .iter_mut()
            .find(|structure| structure.hp_regen_per_tick == 0.0)
            .ok_or("real catalog has no structure without life regeneration")?;
        static_structure.hp_regen_delay_ticks = 10;
        assert_rejects_delay_field(
            &static_structure.validate(),
            "hp_regen_delay_ticks",
            "structure without life regeneration with a delay",
        );
        let regenerating = file
            .structures
            .iter_mut()
            .find(|structure| structure.hp_regen_per_tick > 0.0)
            .ok_or("real catalog has no structure with life regeneration")?;
        regenerating.hp_regen_delay_ticks = 10;
        regenerating.validate()?;
        Ok(())
    }

    /// The shipped catalog text with every authored `hp_regen_delay_ticks` line removed, so each
    /// archetype omits the field.
    fn without_life_regeneration_delays(text: &str) -> String {
        text.lines()
            .filter(|line| !line.trim_start().starts_with("hp_regen_delay_ticks:"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn an_omitted_life_regeneration_delay_reads_as_zero() -> Result<(), Box<dyn std::error::Error>>
    {
        use crate::catalog::{CatalogFile, StructureCatalogFile};
        let units: CatalogFile = ron::from_str(&without_life_regeneration_delays(include_str!(
            "../../../../assets/catalog/units.ron"
        )))?;
        assert!(!units.units.is_empty());
        for unit in &units.units {
            assert_eq!(unit.hp_regen_delay_ticks, 0, "unit {}", unit.slug);
        }
        let structures: StructureCatalogFile = ron::from_str(&without_life_regeneration_delays(
            include_str!("../../../../assets/catalog/structures.ron"),
        ))?;
        assert!(!structures.structures.is_empty());
        for structure in &structures.structures {
            assert_eq!(
                structure.hp_regen_delay_ticks, 0,
                "structure {}",
                structure.slug
            );
        }
        Ok(())
    }

    /// Plan `2026-11-23-regeneration` D-2: a structure regenerates energy only if it has energy,
    /// and never at a negative rate. Each refusal names `energy_regen_per_tick`.
    #[test]
    fn a_structure_without_energy_rejects_an_energy_rate() -> Result<(), Box<dyn std::error::Error>>
    {
        use crate::catalog::StructureCatalogFile;
        let structures = include_str!("../../../../assets/catalog/structures.ron");
        let mut file: StructureCatalogFile = ron::from_str(structures)?;
        for structure in &file.structures {
            structure.validate()?;
        }
        let without = file
            .structures
            .iter_mut()
            .find(|structure| structure.max_energy == 0.0)
            .ok_or("real catalog has no structure without energy")?;
        without.energy_regen_per_tick = 0.0315;
        assert_rejects_delay_field(
            &without.validate(),
            "energy_regen_per_tick",
            "structure without energy with an energy rate",
        );
        let with = file
            .structures
            .iter_mut()
            .find(|structure| structure.max_energy > 0.0)
            .ok_or("real catalog has no structure with energy")?;
        with.energy_regen_per_tick = -0.0315;
        assert_rejects_delay_field(
            &with.validate(),
            "energy_regen_per_tick",
            "structure with a negative energy rate",
        );
        with.energy_regen_per_tick = 0.063;
        with.validate()?;
        Ok(())
    }

    /// Plan `2026-11-23-regeneration` D-2: the rule applies to units alike, and every shipped
    /// unit with a rate has energy.
    #[test]
    fn a_unit_without_energy_rejects_an_energy_rate() -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::CatalogFile;
        let units = include_str!("../../../../assets/catalog/units.ron");
        let mut file: CatalogFile = ron::from_str(units)?;
        for unit in &file.units {
            unit.validate()?;
            assert!(
                unit.energy_regen_per_tick == 0.0 || unit.max_energy > 0.0,
                "unit {}",
                unit.slug
            );
        }
        let without = file
            .units
            .iter_mut()
            .find(|unit| unit.max_energy == 0.0)
            .ok_or("real catalog has no unit without energy")?;
        without.energy_regen_per_tick = 0.0315;
        assert_rejects_delay_field(
            &without.validate(),
            "energy_regen_per_tick",
            "unit without energy with an energy rate",
        );
        without.energy_regen_per_tick = -0.0315;
        assert_rejects_delay_field(
            &without.validate(),
            "energy_regen_per_tick",
            "unit with a negative energy rate",
        );
        Ok(())
    }

    #[test]
    fn an_omitted_structure_energy_rate_reads_as_zero() -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::StructureCatalogFile;
        let text = include_str!("../../../../assets/catalog/structures.ron");
        let shipped: StructureCatalogFile = ron::from_str(text)?;
        for structure in shipped.structures.iter().filter(|s| s.max_energy == 0.0) {
            assert_eq!(
                structure.energy_regen_per_tick, 0.0,
                "structure {}",
                structure.slug
            );
        }
        let stripped = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("energy_regen_per_tick:"))
            .collect::<Vec<_>>()
            .join("\n");
        let omitted: StructureCatalogFile = ron::from_str(&stripped)?;
        assert!(!omitted.structures.is_empty());
        for structure in &omitted.structures {
            assert_eq!(
                structure.energy_regen_per_tick, 0.0,
                "structure {}",
                structure.slug
            );
        }
        Ok(())
    }

    #[test]
    fn fixed_point_input_bounds_are_checked_before_conversion() {
        for valid in [f64::from(i32::MIN), -0.5, 0.0, f64::from(i32::MAX)] {
            assert!(scalar("fixture", "value", valid).is_ok());
        }
        for invalid in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::from(i32::MIN) - 1.0,
            f64::from(i32::MAX) + 1.0,
        ] {
            assert!(scalar("fixture", "value", invalid).is_err());
        }
    }

    #[test]
    fn angles_must_be_finite() {
        assert!(angle("fixture", "arc", 0.0).is_ok());
        assert!(angle("fixture", "arc", 360.0).is_ok());
        assert!(angle("fixture", "arc", f32::NAN).is_err());
        assert!(angle("fixture", "arc", f32::INFINITY).is_err());
    }

    /// The field a typed invalid-value error names, or `None` for any other result.
    fn rejected_field(result: &crate::error::SimResult<()>) -> Option<&str> {
        match result {
            Err(crate::error::SimCoreError::CatalogInvalidValue { field, .. }) => Some(field),
            _ => None,
        }
    }

    #[test]
    fn rates_per_tick_lie_above_zero_and_at_most_half_a_turn() {
        for valid in [0.01_f32, 20.16, 55.99, 84.0, 180.0] {
            assert!(rate_per_tick("fixture", "rate", valid).is_ok(), "{valid}");
        }
        for invalid in [0.0_f32, -1.0, 180.5, 360.0, 999.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                rejected_field(&rate_per_tick("fixture", "rate", invalid)),
                Some("rate"),
                "{invalid}"
            );
        }
    }

    #[test]
    fn a_unit_rejects_a_hull_rate_outside_the_range_naming_the_field(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::CatalogFile;
        let units = include_str!("../../../../assets/catalog/units.ron");
        for (rate, valid) in [
            (0.0_f32, false),
            (180.5, false),
            (55.99, true),
            (180.0, true),
        ] {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file.units.first_mut().ok_or("unit catalog empty")?;
            unit.turn_rate_deg_per_tick = rate;
            let result = unit.validate();
            if valid {
                assert!(result.is_ok(), "{rate}: {result:?}");
            } else {
                assert_eq!(
                    rejected_field(&result),
                    Some("turn_rate_deg_per_tick"),
                    "{rate}"
                );
            }
        }
        Ok(())
    }

    /// The first shipped unit that moves, as authored.
    fn shipped_mobile_unit() -> Result<crate::catalog::UnitArchetypeDef, Box<dyn std::error::Error>>
    {
        use crate::catalog::CatalogFile;
        let units = include_str!("../../../../assets/catalog/units.ron");
        let file: CatalogFile = ron::from_str(units)?;
        Ok(file
            .units
            .into_iter()
            .find(|unit| unit.max_speed > 0.0)
            .ok_or("real catalog has no unit that moves")?)
    }

    #[test]
    fn turn_before_move_is_rejected_on_a_unit_that_cannot_move(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut unit = shipped_mobile_unit()?;
        unit.turn_before_move = true;
        unit.validate()?;
        unit.max_speed = 0.0;
        assert_eq!(rejected_field(&unit.validate()), Some("turn_before_move"));
        Ok(())
    }

    #[test]
    fn turn_before_move_needs_a_half_turn_within_the_stall_repath_window(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 7.2° is 1 311 units: 25 ticks to face a reversal; 7.19° is 1 309 units: 26 ticks.
        for (rate, valid) in [(7.2_f32, true), (7.19, false), (20.16, true), (180.0, true)] {
            let mut unit = shipped_mobile_unit()?;
            unit.turn_before_move = true;
            unit.turn_rate_deg_per_tick = rate;
            let result = unit.validate();
            if valid {
                assert!(result.is_ok(), "{rate}: {result:?}");
            } else {
                assert_eq!(rejected_field(&result), Some("turn_before_move"), "{rate}");
            }
            unit.turn_before_move = false;
            assert!(unit.validate().is_ok(), "{rate} unflagged");
        }
        Ok(())
    }

    #[test]
    fn a_stationary_turn_rate_outside_the_range_is_rejected_naming_the_field(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (rate, valid) in [
            (0.0_f32, false),
            (180.5, false),
            (40.32, true),
            (180.0, true),
        ] {
            let mut unit = shipped_mobile_unit()?;
            unit.stationary_turn_rate_deg_per_tick = Some(rate);
            let result = unit.validate();
            if valid {
                assert!(result.is_ok(), "{rate}: {result:?}");
            } else {
                assert_eq!(
                    rejected_field(&result),
                    Some("stationary_turn_rate_deg_per_tick"),
                    "{rate}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn an_omitted_stationary_turn_rate_reads_as_the_hull_rate(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::UnitCatalog;
        let text = include_str!("../../../../assets/catalog/units.ron")
            .lines()
            .filter(|line| {
                !line
                    .trim_start()
                    .starts_with("stationary_turn_rate_deg_per_tick:")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let factions = crate::test_support::shipped_factions();
        let catalog = UnitCatalog::from_ron_str(&factions, &text)?;
        let mut count = 0;
        for unit in catalog.archetypes() {
            assert_eq!(
                unit.stationary_turn_rate, unit.turn_rate,
                "unit {}",
                unit.slug
            );
            count += 1;
        }
        assert!(count > 0);
        Ok(())
    }

    #[test]
    fn turn_before_move_counts_the_turn_in_place_at_the_stationary_rate(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 7° is 1 274 units: 26 ticks to the first step of a reversal at that rate alone. With
        // a stationary rate of 20° (3 641 units), 9 ticks in place leave less than 1 274, so the
        // first step comes on tick 10.
        for (stationary, valid) in [(None, false), (Some(7.0_f32), false), (Some(20.0), true)] {
            let mut unit = shipped_mobile_unit()?;
            unit.turn_before_move = true;
            unit.turn_rate_deg_per_tick = 7.0;
            unit.stationary_turn_rate_deg_per_tick = stationary;
            let result = unit.validate();
            if valid {
                assert!(result.is_ok(), "{stationary:?}: {result:?}");
            } else {
                assert_eq!(
                    rejected_field(&result),
                    Some("turn_before_move"),
                    "{stationary:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn an_omitted_turn_before_move_reads_as_false() -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::CatalogFile;
        let text = include_str!("../../../../assets/catalog/units.ron")
            .lines()
            .filter(|line| !line.trim_start().starts_with("turn_before_move:"))
            .collect::<Vec<_>>()
            .join("\n");
        let file: CatalogFile = ron::from_str(&text)?;
        assert!(!file.units.is_empty());
        for unit in &file.units {
            assert!(!unit.turn_before_move, "unit {}", unit.slug);
        }
        Ok(())
    }

    #[test]
    fn a_turret_rate_is_authored_exactly_on_turreted_weapons(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::CatalogFile;
        let units = include_str!("../../../../assets/catalog/units.ron");
        let cases = [
            (false, 10.0_f32, false),
            (false, 0.0, true),
            (true, 0.0, false),
            (true, 180.5, false),
            (true, 40.32, true),
        ];
        for (has_turret, rate, valid) in cases {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file
                .units
                .iter_mut()
                .find(|unit| unit.weapon.is_some())
                .ok_or("real catalog has no weapon")?;
            let weapon = unit.weapon.as_mut().ok_or("weapon missing")?;
            weapon.has_turret = has_turret;
            weapon.turret_turn_rate_deg_per_tick = rate;
            let result = unit.validate();
            if valid {
                assert!(result.is_ok(), "{has_turret} {rate}: {result:?}");
            } else {
                assert_eq!(
                    rejected_field(&result),
                    Some("weapon.turret_turn_rate_deg_per_tick"),
                    "{has_turret} {rate}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn b29_the_acquisition_arc_lies_within_the_engaged_arc(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::CatalogFile;
        let units = include_str!("../../../../assets/catalog/units.ron");
        // (engaged, acquisition, valid): `Arc` ÷ 2 never exceeds `Arc` ÷ 2 + `ArcSlop`.
        let cases = [
            (11.25_f32, 0.0_f32, true),
            (11.25, 11.25, true),
            (360.0, 180.0, true),
            (11.25, 11.3, false),
            (11.25, -1.0, false),
            (11.25, f32::NAN, false),
        ];
        for (engaged, acquisition, valid) in cases {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file
                .units
                .iter_mut()
                .find(|unit| unit.weapon.is_some())
                .ok_or("real catalog has no weapon")?;
            let weapon = unit.weapon.as_mut().ok_or("weapon missing")?;
            weapon.firing_arc_deg = engaged;
            weapon.acquire_arc_deg = acquisition;
            let result = unit.validate();
            if valid {
                assert!(result.is_ok(), "{engaged} {acquisition}: {result:?}");
            } else {
                assert_eq!(
                    rejected_field(&result),
                    Some("weapon.acquire_arc_deg"),
                    "{engaged} {acquisition}"
                );
            }
        }
        Ok(())
    }

    /// Plan `2026-11-23-harvest-occupancy` D-4: the acquire radius is finite and nonnegative,
    /// the acquired kinds are not repeated, and an empty list is a valid worker that never moves
    /// on.
    #[test]
    fn a_harvester_acquires_within_a_finite_radius_and_names_each_kind_once(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::{CatalogFile, HarvesterSpecDef};
        use sim_types::ResourceKind::{self, Gas, Mineral};
        let units = include_str!("../../../../assets/catalog/units.ron");
        let cases: [(f32, Vec<ResourceKind>, Option<&str>); 7] = [
            (10.0, vec![Mineral], None),
            (0.0, vec![Mineral, Gas], None),
            (10.0, vec![], None),
            (
                -1.0,
                vec![Mineral],
                Some("capabilities.harvester.acquire_radius"),
            ),
            (
                f32::NAN,
                vec![Mineral],
                Some("capabilities.harvester.acquire_radius"),
            ),
            (
                f32::INFINITY,
                vec![],
                Some("capabilities.harvester.acquire_radius"),
            ),
            (
                10.0,
                vec![Gas, Mineral, Gas],
                Some("capabilities.harvester.acquires"),
            ),
        ];
        for (acquire_radius, acquires, rejected) in cases {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file.units.first_mut().ok_or("real catalog has no unit")?;
            let case = format!("{acquire_radius} {acquires:?}");
            unit.capabilities.harvester = Some(HarvesterSpecDef {
                acquire_radius,
                acquires,
            });
            let result = unit.validate();
            match rejected {
                None => assert!(result.is_ok(), "{case}: {result:?}"),
                Some(field) => assert_eq!(rejected_field(&result), Some(field), "{case}"),
            }
        }
        Ok(())
    }

    /// Both acquire fields are required: a harvester row without one fails to load.
    #[test]
    fn a_harvester_row_without_its_acquire_fields_is_refused() {
        for row in ["(acquires: [Mineral])", "(acquire_radius: 10.0)", "()"] {
            let parsed = ron::from_str::<crate::catalog::HarvesterSpecDef>(row);
            assert!(parsed.is_err(), "{row} loaded");
        }
        let parsed = ron::from_str::<crate::catalog::HarvesterSpecDef>(
            "(acquire_radius: 10.0, acquires: [])",
        );
        assert!(parsed.is_ok(), "{parsed:?}");
    }

    #[test]
    fn b41_a_barrier_absorbs_something_for_at_least_a_tick_within_its_cooldown(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::{BarrierSpecDef, CatalogFile};
        let units = include_str!("../../../../assets/catalog/units.ron");
        let cases = [
            (100.0_f32, 54, 804, None),
            (100.0, 54, 54, None),
            (0.0, 54, 804, Some("capabilities.barrier.absorb_limit")),
            (f32::NAN, 54, 804, Some("capabilities.barrier.absorb_limit")),
            (100.0, 0, 804, Some("capabilities.barrier.duration_ticks")),
            (100.0, 54, 53, Some("capabilities.barrier.cooldown_ticks")),
        ];
        for (absorb_limit, duration_ticks, cooldown_ticks, rejected) in cases {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file.units.first_mut().ok_or("real catalog has no unit")?;
            unit.capabilities.barrier = Some(BarrierSpecDef {
                absorb_limit,
                duration_ticks,
                cooldown_ticks,
            });
            let result = unit.validate();
            match rejected {
                None => assert!(result.is_ok(), "{absorb_limit}: {result:?}"),
                Some(field) => assert_eq!(rejected_field(&result), Some(field)),
            }
        }
        Ok(())
    }

    #[test]
    fn b43_an_attack_dash_speeds_up_for_at_least_a_tick_and_reaches_somewhere(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::{AttackDashSpecDef, CatalogFile};
        let units = include_str!("../../../../assets/catalog/units.ron");
        let cases = [
            (2.2_f32, 63, 0.6_f32, 4.0_f32, None),
            (2.2, 63, 0.0, 4.0, None),
            (
                0.0,
                63,
                0.6,
                4.0,
                Some("capabilities.attack_dash.speed_multiplier"),
            ),
            (
                f32::NAN,
                63,
                0.6,
                4.0,
                Some("capabilities.attack_dash.speed_multiplier"),
            ),
            (
                2.2,
                0,
                0.6,
                4.0,
                Some("capabilities.attack_dash.duration_ticks"),
            ),
            (2.2, 63, -0.1, 4.0, Some("capabilities.attack_dash.min_gap")),
            (
                2.2,
                63,
                0.6,
                0.0,
                Some("capabilities.attack_dash.max_distance"),
            ),
        ];
        for (speed_multiplier, duration_ticks, min_gap, max_distance, rejected) in cases {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file.units.first_mut().ok_or("real catalog has no unit")?;
            unit.capabilities.attack_dash = Some(AttackDashSpecDef {
                speed_multiplier,
                duration_ticks,
                cooldown_ticks: 179,
                min_gap,
                max_distance,
            });
            let result = unit.validate();
            match rejected {
                None => assert!(result.is_ok(), "{speed_multiplier}: {result:?}"),
                Some(field) => assert_eq!(rejected_field(&result), Some(field)),
            }
        }
        Ok(())
    }

    #[test]
    fn b4_mode_random_extras_are_nonnegative_and_bounded() -> Result<(), Box<dyn std::error::Error>>
    {
        use crate::catalog::CatalogFile;
        let units = include_str!("../../../../assets/catalog/units.ron");
        for (extra, valid) in [
            (0.0_f32, true),
            (8.9286, true),
            (-0.5, false),
            (f32::NAN, false),
        ] {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file
                .units
                .iter_mut()
                .find(|unit| !unit.modes.is_empty())
                .ok_or("real catalog has no mode")?;
            let name = format!(
                "modes.{}.leave_random_delay_max_ticks",
                unit.modes[0].mode_name
            );
            unit.modes[0].leave_random_delay_max_ticks = extra;
            let result = unit.validate();
            if valid {
                assert!(result.is_ok(), "{extra}: {result:?}");
            } else {
                assert_eq!(rejected_field(&result), Some(name.as_str()), "{extra}");
            }
        }
        Ok(())
    }

    #[test]
    fn b30_turret_idle_rates_are_bounded_and_hull_weapons_hold(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::{CatalogFile, TurretIdleDef};
        let units = include_str!("../../../../assets/catalog/units.ron");
        let cases = [
            (false, TurretIdleDef::Hold, true),
            (false, TurretIdleDef::FollowMovement(20.16), false),
            (true, TurretIdleDef::Hold, true),
            (true, TurretIdleDef::FollowMovement(20.16), true),
            (true, TurretIdleDef::ResetIfMoving(0.0), false),
            (true, TurretIdleDef::Reset(180.5), false),
            (true, TurretIdleDef::Spin(-2.52), true),
            (true, TurretIdleDef::Spin(0.0), false),
            (true, TurretIdleDef::FollowMovement(-5.0), false),
        ];
        for (has_turret, idle, valid) in cases {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file
                .units
                .iter_mut()
                .find(|unit| unit.weapon.is_some())
                .ok_or("real catalog has no weapon")?;
            let weapon = unit.weapon.as_mut().ok_or("weapon missing")?;
            weapon.has_turret = has_turret;
            weapon.turret_turn_rate_deg_per_tick = if has_turret { 40.32 } else { 0.0 };
            weapon.turret_idle = idle;
            let result = unit.validate();
            if valid {
                assert!(result.is_ok(), "{has_turret} {idle:?}: {result:?}");
            } else {
                assert_eq!(
                    rejected_field(&result),
                    Some("weapon.turret_idle"),
                    "{has_turret} {idle:?}"
                );
            }
        }
        Ok(())
    }

    /// Plan `2026-11-09-client-mode-commands` D-2: a mode toggle switches one of its own
    /// archetype's modes and names its leave command; no other ability names one.
    #[test]
    fn a_mode_toggle_switches_its_own_mode_and_names_its_leave_command(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::CatalogFile;
        use sim_types::{AbilityEffectSpec, AbilityTargetKind};
        type Edit = fn(&mut sim_types::AbilitySpec);
        let units = include_str!("../../../../assets/catalog/units.ron");
        let shipped: CatalogFile = ron::from_str(units)?;
        let toggles = shipped
            .units
            .iter()
            .filter(|unit| {
                unit.abilities
                    .iter()
                    .any(|a| matches!(a.target_kind, AbilityTargetKind::ToggleMode))
            })
            .count();
        assert!(toggles >= 2, "the shipped catalog has mode toggles");
        for unit in &shipped.units {
            unit.validate()?;
        }
        let cases: [(Edit, Option<&str>); 6] = [
            (|_| {}, None),
            (|a| a.leave_name = None, Some("abilities[0].leave_name")),
            (
                |a| a.leave_name = Some("  ".into()),
                Some("abilities[0].leave_name"),
            ),
            (
                |a| {
                    a.effect = AbilityEffectSpec::SwitchMode {
                        mode_name: "no_such_mode".into(),
                    };
                },
                Some("abilities[0].effect.mode_name"),
            ),
            (
                |a| {
                    a.effect = AbilityEffectSpec::CloakToggle {
                        energy_cost_per_second: sim_types::SimScalar::ZERO,
                        is_burrow: true,
                    };
                },
                Some("abilities[0].effect"),
            ),
            (
                |a| a.target_kind = AbilityTargetKind::InstantSelf,
                Some("abilities[0].leave_name"),
            ),
        ];
        for (index, (edit, rejected)) in cases.into_iter().enumerate() {
            let mut file: CatalogFile = ron::from_str(units)?;
            let unit = file
                .units
                .iter_mut()
                .find(|unit| {
                    unit.abilities
                        .first()
                        .is_some_and(|a| matches!(a.target_kind, AbilityTargetKind::ToggleMode))
                })
                .ok_or("real catalog has no mode toggle")?;
            edit(&mut unit.abilities[0]);
            let result = unit.validate();
            match rejected {
                None => assert!(result.is_ok(), "case {index}: {result:?}"),
                Some(field) => assert_eq!(rejected_field(&result), Some(field), "case {index}"),
            }
        }
        Ok(())
    }

    #[test]
    fn blank_slugs_are_rejected_without_renaming_valid_identifiers() {
        assert!(validate_slug("").is_err());
        assert!(validate_slug(" \t\n").is_err());
        assert!(validate_slug("Fixture-Identity").is_ok());
    }

    /// B-123 (plan `2026-11-17-effect-snapshot-identity` D-7): a weapon without a projectile
    /// speed is delivered instantly, so it may not author `visual_kind: Projectile`; with a speed
    /// it may. The shipped catalogs load under the rule.
    #[test]
    fn an_instant_weapon_may_not_publish_a_projectile() -> Result<(), Box<dyn std::error::Error>> {
        use crate::catalog::CatalogFile;
        let units = include_str!("../../../../assets/catalog/units.ron");
        let mut file: CatalogFile = ron::from_str(units)?;
        for unit in &file.units {
            unit.validate()?;
        }
        crate::test_support::shipped_catalogs();
        let unit = file
            .units
            .iter_mut()
            .find(|unit| {
                unit.weapon
                    .as_ref()
                    .is_some_and(|weapon| weapon.projectile_speed.is_none())
            })
            .ok_or("the shipped catalog has an instant weapon")?;
        let weapon = unit.weapon.as_mut().ok_or("armed")?;
        weapon.visual_kind = sim_types::WeaponVisualKind::Projectile;
        let field = match unit.validate() {
            Err(crate::error::SimCoreError::CatalogInvalidValue { field, .. }) => Some(field),
            _ => None,
        };
        assert_eq!(field.as_deref(), Some("weapon.visual_kind"));
        let weapon = unit.weapon.as_mut().ok_or("armed")?;
        weapon.projectile_speed = Some(1.0);
        weapon.max_projectile_lifetime_ticks = weapon.max_projectile_lifetime_ticks.max(1);
        unit.validate()?;
        Ok(())
    }

    /// The shipped unit catalog with every authored `terrain_radius` removed and
    /// `terrain_radius: <value>` authored on the first unit whose `mover_type` is `mover`,
    /// inserted before its `collision_radius`. Returns the edited text,
    /// that unit's slug and its authored collision radius; the slug is read from the file, so no
    /// identity is named here (charter §3.3).
    fn units_with_terrain_radius(
        mover: &str,
        value: &str,
    ) -> Result<(String, String, f32), Box<dyn std::error::Error>> {
        let text = include_str!("../../../../assets/catalog/units.ron");
        let mover_line = format!("mover_type: {mover},");
        let mut out = Vec::new();
        let mut slug = None;
        let mut armed = false;
        let mut found = None;
        // Authored terrain radii are dropped first, so each test sees exactly one: its own.
        for line in text
            .lines()
            .filter(|line| !line.trim_start().starts_with("terrain_radius:"))
        {
            let trimmed = line.trim_start();
            if found.is_none() {
                if let Some(rest) = trimmed.strip_prefix("slug: ") {
                    slug = Some(rest.trim_end_matches(',').trim_matches('"').to_owned());
                }
                if trimmed == mover_line {
                    armed = true;
                }
                if armed {
                    if let Some(rest) = trimmed.strip_prefix("collision_radius: ") {
                        let indent = &line[..line.len() - trimmed.len()];
                        out.push(format!("{indent}terrain_radius: Some({value}),"));
                        let radius: f32 = rest.trim_end_matches(',').parse()?;
                        found = Some((slug.clone().ok_or("no slug before the radius")?, radius));
                    }
                }
            }
            out.push(line.to_owned());
        }
        let (slug, radius) = found.ok_or("no unit of that mover type")?;
        Ok((out.join("\n"), slug, radius))
    }

    fn load_units(text: &str) -> crate::error::SimResult<crate::catalog::UnitCatalog> {
        let factions = crate::test_support::shipped_factions();
        crate::catalog::UnitCatalog::from_ron_str(&factions, text)
    }

    /// The field a catalog load refused, or the whole outcome as text when it was not a field
    /// refusal (a load that succeeded, or another error), so the assertion shows it.
    fn rejected_load_field(result: crate::error::SimResult<crate::catalog::UnitCatalog>) -> String {
        match result {
            Err(crate::error::SimCoreError::CatalogInvalidValue { field, .. }) => field,
            Err(other) => format!("other error: {other}"),
            Ok(_) => "loaded".to_owned(),
        }
    }

    #[test]
    fn a_terrain_radius_is_read_and_defaults_to_the_collision_radius(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use sim_types::SimScalar;
        let (text, slug, radius) = units_with_terrain_radius("Ground", "0.3125")?;
        assert!(
            radius > 0.3125,
            "{slug} must be larger than the authored value"
        );
        let catalog = load_units(&text)?;
        let mut others = 0;
        for unit in catalog.archetypes() {
            if unit.slug == slug {
                assert_eq!(unit.terrain_radius, SimScalar::from_f32_lossy(0.3125));
                assert_eq!(unit.collision_radius, SimScalar::from_f32_lossy(radius));
            } else {
                // Every other authored value was removed, so every other unit reads its
                // collision radius.
                assert_eq!(unit.terrain_radius, unit.collision_radius, "{}", unit.slug);
                others += 1;
            }
        }
        assert!(others > 0);
        Ok(())
    }

    #[test]
    fn a_terrain_radius_must_be_positive_and_finite() -> Result<(), Box<dyn std::error::Error>> {
        for value in ["0.0", "-0.1", "NaN", "inf", "1e-30"] {
            let (text, _, _) = units_with_terrain_radius("Ground", value)?;
            assert_eq!(
                rejected_load_field(load_units(&text)),
                "terrain_radius",
                "{value}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_terrain_radius_must_be_smaller_than_the_collision_radius(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (_, _, radius) = units_with_terrain_radius("Ground", "0.1")?;
        for value in [radius, radius + 0.25] {
            let (text, _, _) = units_with_terrain_radius("Ground", &format!("{value}"))?;
            assert_eq!(
                rejected_load_field(load_units(&text)),
                "terrain_radius",
                "{value}"
            );
        }
        // Just below the collision radius (one sixteenth less) is accepted.
        let (text, _, _) = units_with_terrain_radius("Ground", &format!("{}", radius - 0.0625))?;
        assert!(load_units(&text).is_ok());
        Ok(())
    }

    #[test]
    fn an_air_unit_may_not_have_a_terrain_radius() -> Result<(), Box<dyn std::error::Error>> {
        let (_, _, radius) = units_with_terrain_radius("Air", "0.1")?;
        let (text, _, _) = units_with_terrain_radius("Air", &format!("{}", radius / 2.0))?;
        assert_eq!(rejected_load_field(load_units(&text)), "terrain_radius");
        Ok(())
    }
}

/// Structure radius rules (plan `2026-11-18-structure-radius` D-1, D-2, D-5): the authored
/// `radius` is the structure's collision radius, required, finite and positive, a multiple of
/// 1/16, and at least half the footprint's smaller side; the terrain radius is that half.
#[cfg(test)]
mod structure_radius_tests {
    use sim_types::SimScalar;

    /// The shipped structure catalog with every authored `radius` removed and `radius:`
    /// authored after the footprint of every structure: `value` on the first structure whose
    /// footprint is `width` cells wide (none when `value` is `None`), half the footprint's
    /// smaller side on every other. Returns the edited text and that structure's slug; the slug
    /// is read from the file, so no identity is named here (charter §3.3).
    fn structures_with_radius(
        width: u32,
        value: Option<&str>,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let text = include_str!("../../../../assets/catalog/structures.ron");
        let mut out = Vec::new();
        let mut slug = None;
        let mut chosen = None;
        for line in text
            .lines()
            .filter(|line| !line.trim_start().starts_with("radius:"))
        {
            out.push(line.to_owned());
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("slug: ") {
                slug = Some(rest.trim_end_matches(',').trim_matches('"').to_owned());
            }
            let Some(rest) = trimmed.strip_prefix("footprint: (") else {
                continue;
            };
            let (w, _) = rest
                .trim_end_matches("),")
                .split_once(',')
                .ok_or("a footprint is a pair")?;
            let w: u32 = w.trim().parse()?;
            let indent = &line[..line.len() - trimmed.len()];
            if chosen.is_none() && w == width {
                chosen = Some(slug.clone().ok_or("no slug before the footprint")?);
                if let Some(value) = value {
                    out.push(format!("{indent}radius: {value},"));
                }
            } else {
                // Every other row keeps its shipped radius, which covers its shape.
                out.push(shipped_radius_after(text, &out)?);
            }
        }
        let slug = chosen.ok_or("no structure of that footprint width")?;
        Ok((out.join("\n"), slug))
    }

    fn load(structures: &str) -> crate::error::SimResult<crate::catalog::UnitCatalog> {
        crate::catalog::UnitCatalog::from_ron_str_bundle(
            &crate::test_support::shipped_factions(),
            include_str!("../../../../assets/catalog/units.ron"),
            structures,
        )
    }

    /// The field a catalog load refused, or the whole outcome as text when it was not a field
    /// refusal (a load that succeeded, or another error), so the assertion shows it.
    fn rejected_field(result: crate::error::SimResult<crate::catalog::UnitCatalog>) -> String {
        match result {
            Err(crate::error::SimCoreError::CatalogInvalidValue { field, .. }) => field,
            Err(other) => format!("other error: {other}"),
            Ok(_) => "loaded".to_owned(),
        }
    }

    fn archetype<'a>(
        catalog: &'a crate::catalog::UnitCatalog,
        slug: &str,
    ) -> Result<&'a crate::catalog::UnitArchetype, Box<dyn std::error::Error>> {
        Ok(catalog
            .archetypes()
            .find(|archetype| archetype.slug == slug)
            .ok_or("the edited structure is in the catalog")?)
    }

    #[test]
    fn a_structure_radius_is_read_into_the_collision_radius(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (text, slug) = structures_with_radius(3, Some("1.8125"))?;
        let catalog = load(&text)?;
        let structure = archetype(&catalog, &slug)?;
        assert_eq!(structure.collision_radius, SimScalar::from_num(1.8125));
        assert_eq!(structure.separation_radius, SimScalar::from_num(1.8125));
        Ok(())
    }

    /// A structure's terrain radius is its shape's smaller half-extent (plan
    /// `2026-11-18-contour-footprints` WP6; half its footprint's smaller side for a full box, as
    /// plan `2026-11-18-structure-radius` D-5 set it).
    #[test]
    fn a_structures_terrain_radius_is_its_shapes_smaller_half_extent(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (text, slug) = structures_with_radius(5, Some("2.75"))?;
        let catalog = load(&text)?;
        let structure = archetype(&catalog, &slug)?;
        assert_eq!(structure.collision_radius, SimScalar::from_num(2.75));
        assert_eq!(structure.terrain_radius, SimScalar::from_num(2.5));
        Ok(())
    }

    #[test]
    fn a_structure_radius_is_required() -> Result<(), Box<dyn std::error::Error>> {
        let (text, slug) = structures_with_radius(3, None)?;
        let result = load(&text);
        assert!(
            matches!(
                &result,
                Err(crate::error::SimCoreError::RonDeserializationError(message))
                    if message.contains("radius")
            ),
            "{slug} without `radius` must fail to parse, naming the field, got {result:?}"
        );
        Ok(())
    }

    #[test]
    fn a_structure_radius_must_be_positive_and_finite() -> Result<(), Box<dyn std::error::Error>> {
        for value in ["0.0", "-0.5", "NaN", "inf"] {
            let (text, _) = structures_with_radius(3, Some(value))?;
            assert_eq!(rejected_field(load(&text)), "radius", "{value}");
        }
        Ok(())
    }

    #[test]
    fn a_structure_radius_must_be_a_multiple_of_a_sixteenth(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (text, _) = structures_with_radius(3, Some("1.8"))?;
        assert_eq!(rejected_field(load(&text)), "radius");
        let (text, _) = structures_with_radius(3, Some("1.8125"))?;
        assert_eq!(rejected_field(load(&text)), "loaded");
        Ok(())
    }

    /// The shipped structures with the radius of the first structure whose footprint is `width`
    /// cells wide and whose shape has chamfer `chamfer` set to `value`; every other row as
    /// shipped. Returns the edited text and that structure's slug, read from the file (charter
    /// §3.3).
    fn structures_with_one_radius(
        width: u32,
        chamfer: &str,
        value: &str,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let text = include_str!("../../../../assets/catalog/structures.ron");
        let mut out = Vec::new();
        let mut slug = None;
        let mut chosen: Option<String> = None;
        let mut wide = false;
        let mut edited = false;
        for line in text.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("slug: ") {
                slug = Some(rest.trim_end_matches(',').trim_matches('"').to_owned());
            }
            if let Some(rest) = trimmed.strip_prefix("footprint: (") {
                let (w, _) = rest
                    .trim_end_matches("),")
                    .split_once(',')
                    .ok_or("a footprint is a pair")?;
                wide = w.trim().parse::<u32>()? == width;
            }
            if chosen.is_none()
                && wide
                && trimmed.starts_with("footprint_shape:")
                && trimmed.contains(&format!("chamfer: {chamfer})"))
            {
                chosen = Some(slug.clone().ok_or("no slug before the footprint")?);
            }
            if chosen.is_some() && !edited && trimmed.starts_with("radius:") {
                let indent = &line[..line.len() - trimmed.len()];
                out.push(format!("{indent}radius: {value},"));
                edited = true;
                continue;
            }
            out.push(line.to_owned());
        }
        let slug = chosen.ok_or("no structure of that footprint width")?;
        Ok((out.join("\n"), slug))
    }

    /// Plan `2026-11-18-contour-footprints` D-10: a structure radius must cover its footprint
    /// shape, `radius² ≥ max(hw² + (hh − c)², (hw − c)² + hh²)`, so a unit touching any point of
    /// the shape is within its reach; this replaces the floor at half the footprint.
    #[test]
    fn a_structure_radius_must_cover_its_shape() -> Result<(), Box<dyn std::error::Error>> {
        // The one-cell footprint, a full 1×1 box: bound √0.5 ≈ 0.7071.
        let (text, _) = structures_with_one_radius(1, "0.0", "0.75")?;
        assert_eq!(rejected_field(load(&text)), "loaded");
        let (text, _) = structures_with_one_radius(1, "0.0", "0.6875")?;
        assert_eq!(rejected_field(load(&text)), "radius");
        // A 3×3 contour cut by 0.5: bound √3.25 ≈ 1.803.
        let (text, _) = structures_with_one_radius(3, "0.5", "1.8125")?;
        assert_eq!(rejected_field(load(&text)), "loaded");
        let (text, _) = structures_with_one_radius(3, "0.5", "1.75")?;
        assert_eq!(rejected_field(load(&text)), "radius");
        Ok(())
    }

    /// The shipped line starting with `prefix` that follows the `out.len()`-th kept line of
    /// `text` (the line the filtering helpers removed there), indent included.
    fn shipped_line_after(
        text: &str,
        out: &[String],
        prefix: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let last = out.last().ok_or("a kept line")?;
        let slug = out
            .iter()
            .rev()
            .find_map(|line| line.trim().strip_prefix("slug: "))
            .ok_or("a slug before the footprint")?;
        let mut in_block = false;
        let mut after_footprint = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("slug: ") {
                in_block = rest == slug;
            }
            if in_block && line == last {
                after_footprint = true;
                continue;
            }
            if after_footprint && trimmed.starts_with(prefix) {
                return Ok(line.to_owned());
            }
        }
        Err(format!("no shipped `{prefix}` after the footprint of {slug}").into())
    }

    fn shipped_radius_after(
        text: &str,
        out: &[String],
    ) -> Result<String, Box<dyn std::error::Error>> {
        shipped_line_after(text, out, "radius:")
    }

    /// The shipped structures with `footprint_shape` removed from every structure whose footprint
    /// is `width` cells wide but the first, which gets `value` (or none), and every other set to
    /// the full box of its cells. Returns the edited text and the first structure's slug, read
    /// from the file (charter §3.3).
    fn structures_with_shape(
        width: u32,
        value: Option<&str>,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let text = include_str!("../../../../assets/catalog/structures.ron");
        let mut out = Vec::new();
        let mut slug = None;
        let mut chosen = None;
        for line in text
            .lines()
            .filter(|line| !line.trim_start().starts_with("footprint_shape:"))
        {
            out.push(line.to_owned());
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("slug: ") {
                slug = Some(rest.trim_end_matches(',').trim_matches('"').to_owned());
            }
            let Some(rest) = trimmed.strip_prefix("footprint: (") else {
                continue;
            };
            let (w, _) = rest
                .trim_end_matches("),")
                .split_once(',')
                .ok_or("a footprint is a pair")?;
            let w: u32 = w.trim().parse()?;
            let indent = &line[..line.len() - trimmed.len()];
            if chosen.is_none() && w == width {
                chosen = Some(slug.clone().ok_or("no slug before the footprint")?);
                if let Some(value) = value {
                    out.push(format!("{indent}footprint_shape: {value},"));
                }
            } else {
                // Every other row keeps its shipped shape, which its radius covers.
                out.push(shipped_line_after(text, &out, "footprint_shape:")?);
            }
        }
        let slug = chosen.ok_or("no structure of that footprint width")?;
        Ok((out.join("\n"), slug))
    }

    #[test]
    fn a_footprint_shape_is_required() -> Result<(), Box<dyn std::error::Error>> {
        let (text, slug) = structures_with_shape(3, None)?;
        let result = load(&text);
        assert!(
            matches!(
                &result,
                Err(crate::error::SimCoreError::RonDeserializationError(message))
                    if message.contains("footprint_shape")
            ),
            "{slug} without `footprint_shape` must fail to parse, naming the field, got {result:?}"
        );
        Ok(())
    }

    #[test]
    fn a_footprint_shape_must_be_on_the_quarter_lattice() -> Result<(), Box<dyn std::error::Error>>
    {
        for value in [
            "(half_width: 1.5, half_height: 1.5, chamfer: 0.3)",
            "(half_width: 1.4, half_height: 1.5, chamfer: 0.5)",
            "(half_width: 1.5, half_height: NaN, chamfer: 0.5)",
        ] {
            let (text, _) = structures_with_shape(3, Some(value))?;
            assert_eq!(rejected_field(load(&text)), "footprint_shape", "{value}");
        }
        let (text, _) = structures_with_shape(
            3,
            Some("(half_width: 1.5, half_height: 1.5, chamfer: 0.75)"),
        )?;
        assert_eq!(rejected_field(load(&text)), "loaded");
        Ok(())
    }

    #[test]
    fn a_footprint_shape_must_lie_in_its_cells() -> Result<(), Box<dyn std::error::Error>> {
        for value in [
            "(half_width: 1.25, half_height: 1.0, chamfer: 0.0)",
            "(half_width: 1.0, half_height: 0.0, chamfer: 0.0)",
            "(half_width: -1.0, half_height: 1.0, chamfer: 0.0)",
        ] {
            let (text, _) = structures_with_shape(2, Some(value))?;
            assert_eq!(rejected_field(load(&text)), "footprint_shape", "{value}");
        }
        let (text, _) = structures_with_shape(
            2,
            Some("(half_width: 0.5, half_height: 0.5, chamfer: 0.25)"),
        )?;
        assert_eq!(rejected_field(load(&text)), "loaded");
        Ok(())
    }

    #[test]
    fn a_chamfer_must_leave_every_face() -> Result<(), Box<dyn std::error::Error>> {
        for value in [
            "(half_width: 1.0, half_height: 1.0, chamfer: 1.0)",
            "(half_width: 1.0, half_height: 1.0, chamfer: -0.25)",
        ] {
            let (text, _) = structures_with_shape(2, Some(value))?;
            assert_eq!(rejected_field(load(&text)), "footprint_shape", "{value}");
        }
        let (text, _) = structures_with_shape(
            2,
            Some("(half_width: 1.0, half_height: 1.0, chamfer: 0.75)"),
        )?;
        assert_eq!(rejected_field(load(&text)), "loaded");
        Ok(())
    }

    #[test]
    fn a_footprint_shape_is_read_into_the_dome() -> Result<(), Box<dyn std::error::Error>> {
        let (text, slug) = structures_with_shape(
            3,
            Some("(half_width: 1.5, half_height: 1.5, chamfer: 0.75)"),
        )?;
        let catalog = load(&text)?;
        let structure = archetype(&catalog, &slug)?;
        assert_eq!(
            structure.shape,
            sim_types::ShapeKind::BuildingDome {
                footprint_w: 3,
                footprint_h: 3,
                shape: sim_types::FootprintShape::Box {
                    half_width_q: 6,
                    half_height_q: 6,
                    chamfer_q: 3,
                },
            }
        );
        Ok(())
    }
}

/// Plan `2026-11-19-footprint-flags` WP1: the authored placement flags and resource rings.
#[cfg(test)]
mod placement_flag_tests {
    use crate::catalog::PlacementRules;
    use crate::error::SimCoreError;
    use sim_types::{PlacementFlag, PlacementFlags, ResourceKind};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const STRUCTURES: &str = include_str!("../../../../assets/catalog/structures.ron");
    const RULES: &str = include_str!("../../../../assets/catalog/placement_rules.ron");

    fn load(structures: &str) -> crate::error::SimResult<crate::catalog::UnitCatalog> {
        crate::catalog::UnitCatalog::from_ron_str_bundle(
            &crate::test_support::shipped_factions(),
            include_str!("../../../../assets/catalog/units.ron"),
            structures,
        )
    }

    /// The shipped structures with the first row's `placement_excludes` line replaced by
    /// `line` (or removed), and that row's slug.
    fn first_row_with(line: Option<&str>) -> Result<(String, String), Box<dyn std::error::Error>> {
        let mut out = Vec::new();
        let mut slug = None;
        let mut edited = false;
        for text in STRUCTURES.lines() {
            let trimmed = text.trim_start();
            if slug.is_none() {
                slug = trimmed
                    .strip_prefix("slug: ")
                    .map(|rest| rest.trim_end_matches(',').trim_matches('"').to_owned());
            }
            if !edited && trimmed.starts_with("placement_excludes:") {
                edited = true;
                if let Some(line) = line {
                    let indent = &text[..text.len() - trimmed.len()];
                    out.push(format!("{indent}{line}"));
                }
                continue;
            }
            out.push(text.to_owned());
        }
        Ok((out.join("\n"), slug.ok_or("a slug")?))
    }

    /// A full box on a one-cell row: the shape `rules_with` gives a ring that names none.
    const ONE_CELL_SHAPE: &str =
        "shape: Box(half_width: 0.5, half_height: 0.5, chamfer: 0.0), remove_when_empty: false";

    /// A valid harvest (plan `2026-11-22-harvest-timing` D-2): what `rules_with` gives a ring
    /// that names none.
    const A_HARVEST: &str = "harvest_time_ticks: 50, harvest_amount: 5, return_delay_ticks: 0";

    /// The shipped rules with the row of `kind` replaced by `ring` (the text inside its
    /// parentheses after `kind: …,`); a ring that names no shape gets [`ONE_CELL_SHAPE`], and one
    /// that names no harvest [`A_HARVEST`].
    fn rules_with(kind: &str, ring: &str) -> Result<String, Box<dyn std::error::Error>> {
        let start = RULES
            .find(&format!("(kind: {kind},"))
            .ok_or("the shipped row of that kind")?;
        let removal = start
            + RULES[start..]
                .find("remove_when_empty: ")
                .ok_or("the row's removal flag")?;
        let end = removal + RULES[removal..].find("),").ok_or("the row's end")? + "),".len();
        let ring = if ring.contains("shape:") {
            ring.to_owned()
        } else {
            format!("{ring}, {ONE_CELL_SHAPE}")
        };
        let ring = if ring.contains("harvest_time_ticks:") {
            ring
        } else {
            format!("{ring}, {A_HARVEST}")
        };
        Ok(format!(
            "{}(kind: {kind}, {ring}),{}",
            &RULES[..start],
            &RULES[end..]
        ))
    }

    fn refused_field(result: crate::error::SimResult<PlacementRules>) -> String {
        match result {
            Err(SimCoreError::CatalogInvalidValue { field, .. }) => field,
            Err(other) => format!("other error: {other}"),
            Ok(_) => "loaded".to_owned(),
        }
    }

    #[test]
    fn placement_excludes_is_required() -> TestResult {
        let (text, slug) = first_row_with(None)?;
        let result = load(&text);
        assert!(
            matches!(
                &result,
                Err(SimCoreError::RonDeserializationError(message))
                    if message.contains("placement_excludes")
            ),
            "{slug} without `placement_excludes` must fail to parse, naming the field, got {result:?}"
        );
        Ok(())
    }

    #[test]
    fn an_unknown_flag_is_refused() -> TestResult {
        for name in ["Creep", "DropOff", "Masked", "NoBuild"] {
            let (text, _) = first_row_with(Some(&format!("placement_excludes: [{name}],")))?;
            let result = load(&text);
            assert!(
                matches!(
                    &result,
                    Err(SimCoreError::RonDeserializationError(message)) if message.contains(name)
                ),
                "{name} must be refused as an unknown flag, got {result:?}"
            );
            let rules = rules_with(
                "Mineral",
                &format!(r#"applies: [{name}], anchor: (0, 0), rows: ["x"]"#),
            )?;
            assert!(
                matches!(
                    PlacementRules::from_ron_str(&rules),
                    Err(SimCoreError::RonDeserializationError(_))
                ),
                "{name} applied by a resource"
            );
        }
        Ok(())
    }

    #[test]
    fn duplicate_flags_are_refused() -> TestResult {
        let (text, slug) = first_row_with(Some("placement_excludes: [Fogged, Fogged],"))?;
        assert!(
            matches!(
                load(&text),
                Err(SimCoreError::CatalogInvalidValue { definition, field, .. })
                    if definition == slug && field == "placement_excludes"
            ),
            "{slug}"
        );
        let (text, _) = first_row_with(Some("placement_excludes: [Fogged, NearResources],"))?;
        let catalog = load(&text)?;
        assert!(catalog
            .archetypes()
            .any(|arch| arch.placement_excludes == PlacementFlags::all()));
        let rules = rules_with(
            "Gas",
            r#"applies: [NearResources, NearResources], anchor: (0, 0), rows: ["x"]"#,
        )?;
        assert_eq!(
            refused_field(PlacementRules::from_ron_str(&rules)),
            "applies"
        );
        Ok(())
    }

    #[test]
    fn fogged_is_never_applied() -> TestResult {
        for applies in ["[Fogged]", "[NearResources, Fogged]"] {
            let rules = rules_with(
                "Mineral",
                &format!(r#"applies: {applies}, anchor: (0, 0), rows: ["x"]"#),
            )?;
            assert_eq!(
                refused_field(PlacementRules::from_ron_str(&rules)),
                "applies",
                "{applies}"
            );
        }
        Ok(())
    }

    #[test]
    fn resource_reach_rows_are_validated() -> TestResult {
        let cases = [
            (
                r#"applies: [NearResources], anchor: (0, 0), rows: ["xx", "x"]"#,
                "rows",
            ),
            (
                r#"applies: [NearResources], anchor: (0, 0), rows: ["xq"]"#,
                "rows",
            ),
            ("applies: [NearResources], anchor: (0, 0), rows: []", "rows"),
            (
                r#"applies: [NearResources], anchor: (0, 0), rows: [""]"#,
                "rows",
            ),
            (
                r#"applies: [NearResources], anchor: (0, 0), rows: ["xxxxxxxxxxxxxxxx"]"#,
                "rows",
            ),
            (
                r#"applies: [NearResources], anchor: (2, 0), rows: ["xx"]"#,
                "anchor",
            ),
            (
                r#"applies: [NearResources], anchor: (0, 1), rows: ["xx"]"#,
                "anchor",
            ),
            (
                r#"applies: [NearResources], anchor: (1, 0), rows: ["x."]"#,
                "anchor",
            ),
            (r#"applies: [], anchor: (0, 0), rows: ["x"]"#, "applies"),
            (
                r#"applies: [NearResources], anchor: (0, 0), rows: ["x"]"#,
                "loaded",
            ),
        ];
        for (ring, field) in cases {
            let rules = rules_with("Mineral", ring)?;
            assert_eq!(
                refused_field(PlacementRules::from_ron_str(&rules)),
                field,
                "{ring}"
            );
        }
        let tall = format!(
            "applies: [NearResources], anchor: (0, 0), rows: [{}]",
            vec!["\"x\""; 16].join(", ")
        );
        assert_eq!(
            refused_field(PlacementRules::from_ron_str(&rules_with("Mineral", &tall)?)),
            "rows"
        );
        // A kind twice: the gas ring renamed to the mineral kind.
        let twice = RULES.replace("(kind: Gas,", "(kind: Mineral,");
        assert_eq!(refused_field(PlacementRules::from_ron_str(&twice)), "kind");
        Ok(())
    }

    // ---- Plan `2026-11-20-resource-footprints` WP2: the footprint rows (D-1). ----

    #[test]
    fn a_resource_row_needs_own_cells() -> TestResult {
        // A ring of `o` cells only, the anchor on one of them.
        let rules = rules_with(
            "Mineral",
            r#"applies: [NearResources], anchor: (0, 0), rows: ["oo", "oo"]"#,
        )?;
        assert_eq!(refused_field(PlacementRules::from_ron_str(&rules)), "rows");
        Ok(())
    }

    #[test]
    fn own_cells_must_be_one_rectangle_with_the_anchor() -> TestResult {
        let cases = [
            // Two separate own cells.
            (r#"anchor: (0, 0), rows: ["xox"]"#, "rows"),
            // An L of three.
            (r#"anchor: (0, 0), rows: ["xx", "xo"]"#, "rows"),
            // Five wide.
            (r#"anchor: (0, 0), rows: ["xxxxx"]"#, "rows"),
            // The anchor on a ring cell.
            (r#"anchor: (1, 0), rows: ["xo"]"#, "anchor"),
            // A 4 × 4 with the anchor inside it, and a 2 × 1 anchored on its east cell.
            (
                r#"anchor: (2, 1), rows: ["xxxx", "xxxx", "xxxx", "xxxx"]"#,
                "loaded",
            ),
            (r#"anchor: (1, 0), rows: ["xx"]"#, "loaded"),
        ];
        for (ring, field) in cases {
            let rules = rules_with("Mineral", &format!("applies: [NearResources], {ring}"))?;
            assert_eq!(
                refused_field(PlacementRules::from_ron_str(&rules)),
                field,
                "{ring}"
            );
        }
        // The loaded footprint is the rectangle from the anchor.
        let rules = PlacementRules::from_ron_str(&rules_with(
            "Mineral",
            r#"applies: [NearResources], anchor: (1, 0), rows: ["xx"],
               shape: Box(half_width: 1.0, half_height: 0.5, chamfer: 0.0),
               remove_when_empty: true"#,
        )?)?;
        let footprint = rules
            .resource_footprints()
            .get(ResourceKind::Mineral)
            .ok_or("a mineral footprint")?;
        assert_eq!(footprint.cells, (-1, 0, 2, 1));
        Ok(())
    }

    #[test]
    fn a_resource_shape_must_lie_in_its_cells() -> TestResult {
        let row = |shape: &str| {
            format!(
                r#"applies: [NearResources], anchor: (0, 0), rows: ["xx"], shape: {shape},
                   remove_when_empty: true"#
            )
        };
        let cases = [
            (
                "Box(half_width: 1.25, half_height: 0.5, chamfer: 0.0)",
                "shape",
            ),
            (
                "Box(half_width: 1.0, half_height: 0.75, chamfer: 0.0)",
                "shape",
            ),
            (
                "Box(half_width: 0.0, half_height: 0.5, chamfer: 0.0)",
                "shape",
            ),
            (
                "Box(half_width: 1.0, half_height: 0.5, chamfer: 0.5)",
                "shape",
            ),
            (
                "Box(half_width: 0.9, half_height: 0.5, chamfer: 0.0)",
                "shape",
            ),
            (
                "Box(half_width: 1.0, half_height: 0.5, chamfer: 0.25)",
                "loaded",
            ),
            ("Outline([(-1.0, -0.5), (1.0, -0.5), (1.0, 0.75)])", "shape"),
            ("Outline([(-1.0, -0.5), (1.0, -0.5)])", "shape"),
            (
                "Outline([(-1.0, -0.5), (0.5, -0.5), (-0.125, 0.25)])",
                "shape",
            ),
            ("Outline([(-1.0, -0.5), (0.0, -0.5), (0.0, 0.5)])", "loaded"),
        ];
        for (shape, field) in cases {
            let rules = rules_with("Mineral", &row(shape))?;
            assert_eq!(
                refused_field(PlacementRules::from_ron_str(&rules)),
                field,
                "{shape}"
            );
        }
        Ok(())
    }

    #[test]
    fn an_outline_edge_must_be_axis_or_diagonal() -> TestResult {
        let row = |outline: &str| {
            format!(
                r#"applies: [NearResources], anchor: (1, 1), rows: ["xxx", "xxx", "xxx"],
                   shape: Outline([{outline}]), remove_when_empty: false"#
            )
        };
        let cases = [
            // An edge of slope 2.
            ("(-1.0, -1.0), (1.0, -1.0), (0.0, 1.0)", "shape"),
            // Two edges crossing.
            (
                "(-1.0, -1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, 1.0)",
                "shape",
            ),
            // Seventeen vertices.
            (
                "(-1.0, -1.0), (-0.75, -1.0), (-0.5, -1.0), (-0.25, -1.0), (0.0, -1.0), \
                 (0.25, -1.0), (0.5, -1.0), (0.75, -1.0), (1.0, -1.0), (1.0, -0.5), (1.0, 0.0), \
                 (1.0, 0.5), (1.0, 1.0), (0.5, 1.0), (0.0, 1.0), (-0.5, 1.0), (-1.0, 1.0)",
                "shape",
            ),
            // The diagonals and axes of a chamfered square.
            (
                "(-0.5, -1.0), (0.5, -1.0), (1.0, -0.5), (1.0, 0.5), (0.5, 1.0), (-0.5, 1.0), \
                 (-1.0, 0.5), (-1.0, -0.5)",
                "loaded",
            ),
        ];
        for (outline, field) in cases {
            let rules = rules_with("Gas", &row(outline))?;
            assert_eq!(
                refused_field(PlacementRules::from_ron_str(&rules)),
                field,
                "{outline}"
            );
        }
        Ok(())
    }

    #[test]
    fn both_kinds_need_a_row() -> TestResult {
        for kind in ["Mineral", "Gas"] {
            let start = RULES
                .find(&format!("(kind: {kind},"))
                .ok_or("the shipped row")?;
            let removal = start
                + RULES[start..]
                    .find("remove_when_empty: ")
                    .ok_or("the row's removal flag")?;
            let end = removal + RULES[removal..].find("),").ok_or("the row's end")? + 2;
            let without = format!("{}{}", &RULES[..start], &RULES[end..]);
            assert_eq!(
                refused_field(PlacementRules::from_ron_str(&without)),
                "resource_footprints",
                "{kind}"
            );
        }
        Ok(())
    }

    /// Whether `cells` is unchanged by mirroring `dx` about `twice_centre_x / 2` and `dy` about
    /// `twice_centre_y / 2` (the centre of the node's own cells, doubled).
    fn symmetric(cells: &[(i8, i8)], (twice_centre_x, twice_centre_y): (i8, i8)) -> bool {
        cells.iter().all(|&(dx, dy)| {
            cells.contains(&(twice_centre_x - dx, dy)) && cells.contains(&(dx, twice_centre_y - dy))
        })
    }

    #[test]
    fn the_shipped_reaches_are_the_5_0_14_rings() -> TestResult {
        let rules = PlacementRules::from_ron_str(RULES)?;
        let mineral = rules
            .reach(ResourceKind::Mineral)
            .ok_or("a mineral reach")?;
        let gas = rules.reach(ResourceKind::Gas).ok_or("a gas reach")?;
        // FootprintMineralsRounded (plan `2026-11-20-resource-footprints` D-1): the 8 × 7 rows
        // round the 2×1 field anchored on its west cell, less their four corners: 56 − 4.
        assert_eq!(mineral.cells.len(), 52);
        for dy in -3_i64..=3 {
            for dx in -3_i64..=4 {
                let corner = (dx == -3 || dx == 4) && dy.abs() == 3;
                assert_eq!(mineral.covers(dx, dy), !corner, "mineral ({dx}, {dy})");
            }
        }
        assert!(!mineral.covers(5, 0) && !mineral.covers(-4, 0) && !mineral.covers(0, -4));
        // FootprintGeyserRounded with void's rounded rows: 81 − 2 × 4 − 2 × 2.
        assert_eq!(gas.cells.len(), 69);
        assert!(gas.covers(2, 4) && !gas.covers(3, 4));
        assert!(gas.covers(3, 3) && !gas.covers(4, 3));
        assert!(gas.covers(4, 2) && !gas.covers(5, 0));
        for (reach, centre) in [(mineral, (1, 0)), (gas, (0, 0))] {
            assert!(symmetric(&reach.cells, centre), "{:?}", reach.kind);
            assert!(reach.covers(0, 0), "{:?} holds its node's cell", reach.kind);
            assert_eq!(reach.applies, PlacementFlag::NearResources.bit());
        }
        Ok(())
    }
}

/// Attribute rules (plan `2026-11-23-structure-attributes` D-2, D-3, P-4): a structure row
/// authors a non-empty list without repeats that names `Structure`; a unit row authors known bits,
/// at least one, never `Structure`, and no building shape.
#[cfg(test)]
mod attribute_tests {
    use crate::catalog::{AttributeFlagsDef, StructureCatalogFile, UnitArchetypeDef};
    use crate::error::{SimCoreError, SimResult};
    use sim_types::{Attribute, ShapeKind};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const STRUCTURES: &str = include_str!("../../../../assets/catalog/structures.ron");

    fn shipped_structures() -> Result<StructureCatalogFile, Box<dyn std::error::Error>> {
        let file: StructureCatalogFile = ron::from_str(STRUCTURES)?;
        assert!(
            !file.structures.is_empty(),
            "the shipped catalog has structures"
        );
        Ok(file)
    }

    /// The first shipped unit row, which validates as authored.
    fn shipped_unit() -> Result<UnitArchetypeDef, Box<dyn std::error::Error>> {
        let file: crate::catalog::CatalogFile =
            ron::from_str(include_str!("../../../../assets/catalog/units.ron"))?;
        let unit = file
            .units
            .into_iter()
            .next()
            .ok_or("the shipped catalog has units")?;
        unit.validate()?;
        Ok(unit)
    }

    /// Asserts that `result` is a refusal naming `field` with `reason`.
    fn assert_refused(result: &SimResult<()>, field: &str, reason: &str) {
        assert!(
            matches!(
                result,
                Err(SimCoreError::CatalogInvalidValue { field: f, reason: r, .. })
                    if f == field && *r == reason
            ),
            "expected `{field}`: {reason}; got {result:?}"
        );
    }

    #[test]
    fn a_structure_without_the_attributes_field_does_not_parse() {
        let stripped = STRUCTURES
            .lines()
            .filter(|line| !line.trim_start().starts_with("attributes:"))
            .collect::<Vec<_>>()
            .join("\n");
        let error = ron::from_str::<StructureCatalogFile>(&stripped)
            .err()
            .map(|error| error.to_string());
        assert!(
            error
                .as_deref()
                .is_some_and(|error| error.contains("attributes")),
            "a structure row without `attributes` must fail to parse naming it; got {error:?}"
        );
    }

    #[test]
    fn a_structure_with_no_attribute_is_refused() -> TestResult {
        let mut file = shipped_structures()?;
        for structure in &mut file.structures {
            structure.validate()?;
            structure.attributes.clear();
            assert_refused(
                &structure.validate(),
                "attributes",
                "must name at least one attribute",
            );
        }
        Ok(())
    }

    #[test]
    fn a_structure_attribute_listed_twice_is_refused() -> TestResult {
        let mut file = shipped_structures()?;
        for structure in &mut file.structures {
            let first = *structure
                .attributes
                .first()
                .ok_or("an authored attribute")?;
            structure.attributes.push(first);
            assert_refused(
                &structure.validate(),
                &format!("attributes.{first:?}"),
                "listed twice",
            );
        }
        Ok(())
    }

    #[test]
    fn a_structure_without_the_structure_attribute_is_refused() -> TestResult {
        let mut file = shipped_structures()?;
        for structure in &mut file.structures {
            structure
                .attributes
                .retain(|&attribute| attribute != Attribute::Structure);
            assert!(!structure.attributes.is_empty(), "{}", structure.slug);
            assert_refused(
                &structure.validate(),
                "attributes",
                "a structure has the Structure attribute",
            );
        }
        Ok(())
    }

    #[test]
    fn a_structure_may_author_any_attribute_besides_structure() -> TestResult {
        // The game's Creep Tumor is Light, Biological, Structure (plan §2.2 rule 3), so no
        // attribute but `Structure` is required or refused on a structure row.
        let mut file = shipped_structures()?;
        let structure = file.structures.first_mut().ok_or("a structure")?;
        structure.attributes = Attribute::ALL.to_vec();
        structure.validate()?;
        structure.attributes = vec![Attribute::Structure];
        structure.validate()?;
        Ok(())
    }

    #[test]
    fn a_unit_with_the_structure_attribute_is_refused() -> TestResult {
        let mut unit = shipped_unit()?;
        unit.attributes = AttributeFlagsDef::Bits {
            bits: unit.attributes.bits() | sim_types::AttributeFlags::STRUCTURE.bits(),
        };
        assert_refused(
            &unit.validate(),
            "attributes",
            "only a structure row has the Structure attribute",
        );
        Ok(())
    }

    #[test]
    fn a_unit_with_no_attribute_is_refused() -> TestResult {
        let mut unit = shipped_unit()?;
        unit.attributes = AttributeFlagsDef::Bits { bits: 0 };
        assert_refused(
            &unit.validate(),
            "attributes",
            "must name at least one attribute; an archetype without one matches no weapon's \
             target flags",
        );
        Ok(())
    }

    #[test]
    fn a_unit_with_an_unknown_attribute_bit_is_refused() -> TestResult {
        let mut unit = shipped_unit()?;
        for attributes in [
            AttributeFlagsDef::Bits { bits: 256 },
            AttributeFlagsDef::Num(unit.attributes.bits() | 256),
        ] {
            unit.attributes = attributes;
            assert_refused(
                &unit.validate(),
                "attributes",
                "names a bit no attribute has",
            );
        }
        Ok(())
    }

    #[test]
    fn a_unit_with_a_building_shape_is_refused() -> TestResult {
        let mut unit = shipped_unit()?;
        let structure = shipped_structures()?
            .structures
            .into_iter()
            .next()
            .ok_or("a structure")?;
        unit.shape = ShapeKind::BuildingDome {
            footprint_w: 2,
            footprint_h: 2,
            shape: structure.footprint_shape.resolve(),
        };
        assert_refused(
            &unit.validate(),
            "shape",
            "only a structure row has a building shape",
        );
        Ok(())
    }
}
