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
use crate::weapon_timing::AttackRate;
use sim_types::{AbilityEffectSpec, AbilitySpec, ProjectileTargetLossBehavior, SimScalar};

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
            ("sight_radius", self.sight_radius),
            ("scan_range", self.scan_range),
        ] {
            nonnegative(&self.slug, field, f64::from(value))?;
        }
        angle(&self.slug, "turn_rate_deg", self.turn_rate_deg)?;
        for (name, weapon) in [
            ("weapon", &self.weapon),
            ("ground_weapon", &self.ground_weapon),
            ("air_weapon", &self.air_weapon),
        ] {
            if let Some(weapon) = weapon {
                weapon.validate(&self.slug, name)?;
            }
        }
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
        angle(
            definition,
            &format!("{prefix}.turret_turn_rate_deg"),
            self.turret_turn_rate_deg,
        )?;
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
            nonnegative(
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
        Ok(())
    }
}

impl StructureArchetypeDef {
    pub(super) fn validate(&self) -> SimResult<()> {
        validate_slug(&self.slug)?;
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
            ("sight_radius", self.sight_radius),
        ] {
            nonnegative(&self.slug, field, value)?;
        }
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
    use super::{angle, scalar, validate_slug};

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
    fn angles_preserve_finite_legacy_rates_but_reject_nonfinite_values() {
        assert!(angle("fixture", "rate", 999.0).is_ok());
        assert!(angle("fixture", "rate", 0.0).is_ok());
        assert!(angle("fixture", "rate", f32::NAN).is_err());
        assert!(angle("fixture", "rate", f32::INFINITY).is_err());
    }

    #[test]
    fn blank_slugs_are_rejected_without_renaming_valid_identifiers() {
        assert!(validate_slug("").is_err());
        assert!(validate_slug(" \t\n").is_err());
        assert!(validate_slug("Fixture-Identity").is_ok());
    }
}
