//! Safe authored-data validation before lossy catalog DTO conversion.
//!
//! Preserves existing valid f32/f64 quantization while rejecting nonfinite and
//! out-of-range fixed-point input, narrowing identifier/footprint conversions and
//! blank identities. It also rejects every value the tick loop cannot handle: a
//! zero divisor, an empty attack cadence, an inverted range or a body with no size
//! fails here, once, instead of on every peer during a match. This is safety
//! validation, not a balance audit: odd but runnable authored values remain legal.

use super::{StructureArchetypeDef, UnitArchetypeDef, UnitModeSpecDef, WeaponSpecDef};
use crate::error::{SimCoreError, SimResult};
use crate::movement::tuning::STALL_REPATH_TICKS;
use crate::weapon_timing::AttackRate;
use sim_map::MAX_SIGHT_RADIUS_CELLS;
use sim_types::{
    AbilityEffectSpec, AbilitySpec, ProjectileTargetLossBehavior, SimAngle, SimScalar,
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

/// Checks `turn_before_move` against the unit's speed and hull rate. Runs after the hull rate
/// itself is validated, so `turn_rate_deg_per_tick` lies in `0 < rate <= 180`.
///
/// A flag on a unit that cannot move is a value nothing reads. A flagged unit gains no ground
/// while it turns in place, so the stall counter of an order counts those ticks: a hull that
/// needed more than `STALL_REPATH_TICKS` ticks to face a reversal would drop its route mid-turn
/// (and at very low rates abandon the order). The ticks to the first step of a reversal are
/// `ceil(32 768 ÷ rate)` in binary units per tick, computed with the loader's own conversion.
fn turn_before_move(
    definition: &str,
    flagged: bool,
    max_speed: f32,
    turn_rate_deg_per_tick: f32,
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
    let rate = u32::from(super::binary_angle(turn_rate_deg_per_tick).raw());
    let half_turn = u32::from(SimAngle::DEG_180.raw());
    if rate == 0 || half_turn.div_ceil(rate) > u32::from(STALL_REPATH_TICKS) {
        return Err(invalid(
            definition,
            "turn_before_move",
            "requires a hull rate that turns half a turn within the stall repath window (25 ticks)",
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
    /// A harvester that carries nothing would loop forever without earning.
    fn validate_harvester(&self) -> SimResult<()> {
        if self
            .capabilities
            .harvester
            .as_ref()
            .is_some_and(|spec| spec.harvest_capacity == 0)
        {
            return Err(invalid(
                &self.slug,
                "capabilities.harvester.harvest_capacity",
                "must be positive",
            ));
        }
        Ok(())
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
        for (field, value) in [
            ("max_hp", self.max_hp),
            ("max_shields", self.max_shields),
            ("max_energy", self.max_energy),
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
        let hull_rate = self.turn_rate_deg_per_tick;
        rate_per_tick(&self.slug, "turn_rate_deg_per_tick", hull_rate)?;
        turn_before_move(&self.slug, self.turn_before_move, self.max_speed, hull_rate)?;
        self.validate_regeneration_delays()?;
        for (name, weapon) in [
            ("weapon", &self.weapon),
            ("ground_weapon", &self.ground_weapon),
            ("air_weapon", &self.air_weapon),
        ] {
            if let Some(weapon) = weapon {
                weapon.validate(&self.slug, name)?;
            }
        }
        self.validate_harvester()?;
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
        ability_rates(&self.slug, "abilities", &self.abilities)?;
        ability_rates(
            &self.slug,
            "capabilities.abilities",
            &self.capabilities.abilities,
        )?;
        for mode in self.modes.iter().chain(&self.capabilities.modes) {
            mode.validate(&self.slug)?;
        }
        Ok(())
    }
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

    /// Checks the turret rate: a turreted weapon turns at a rate in `0 < rate <= 180` degrees
    /// per tick; a hull-mounted weapon turns with the hull and authors exactly 0.
    fn validate_turret(&self, definition: &str, prefix: &str) -> SimResult<()> {
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
            ("bonus_armored", self.bonus_armored),
            ("bonus_light", self.bonus_light),
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
        angle(
            definition,
            &format!("{prefix}.firing_arc_deg"),
            self.firing_arc_deg,
        )?;
        if !(0.0..=360.0).contains(&self.firing_arc_deg) {
            return Err(invalid(
                definition,
                &format!("{prefix}.firing_arc_deg"),
                "must lie in 0..=360 degrees",
            ));
        }
        self.validate_turret(definition, prefix)?;
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
            for (field, value) in [
                ("inner_radius", splash.inner_radius),
                ("middle_radius", splash.middle_radius),
                ("outer_radius", splash.outer_radius),
            ] {
                nonnegative(
                    definition,
                    &format!("{prefix}.splash.{field}"),
                    f64::from(value),
                )?;
            }
            if splash.inner_radius > splash.middle_radius
                || splash.middle_radius > splash.outer_radius
            {
                return Err(invalid(
                    definition,
                    &format!("{prefix}.splash.middle_radius"),
                    "splash radii must not decrease from inner to outer",
                ));
            }
        }
        Ok(())
    }
}

impl UnitModeSpecDef {
    fn validate(&self, definition: &str) -> SimResult<()> {
        let prefix = format!("modes.{}", self.mode_name);
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
    pub(super) fn validate(&self) -> SimResult<()> {
        validate_slug(&self.slug)?;
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
        ] {
            nonnegative(&self.slug, field, value)?;
        }
        sight(&self.slug, "sight_radius", self.sight_radius)?;
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
        Ok(())
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

    /// Asserts that `result` is the typed invalid-value error naming `expected_field`, one of
    /// the two regeneration-delay fields.
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
    fn blank_slugs_are_rejected_without_renaming_valid_identifiers() {
        assert!(validate_slug("").is_err());
        assert!(validate_slug(" \t\n").is_err());
        assert!(validate_slug("Fixture-Identity").is_ok());
    }
}
