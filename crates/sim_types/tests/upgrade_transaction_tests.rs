//! Atomic checked accumulation of authored upgrade effects.
//!
//! Failures must preserve the entire player state, including unlocks, levels and
//! completion identity, even when an earlier effect in the same transaction passed.

use sim_types::{PlayerUpgradeState, SimScalar, UpgradeEffect, UpgradeStat};

fn adjustment(delta: i32, hp: SimScalar) -> UpgradeEffect {
    UpgradeEffect::ModifyUnitStat {
        target_slug: "arithmetic-fixture".into(),
        max_hp_delta: hp,
        move_speed_delta: SimScalar::ZERO,
        sight_delta: SimScalar::ZERO,
        weapon_range_delta: SimScalar::ZERO,
        armor_delta: SimScalar::ZERO,
        attack_cooldown_delta: delta,
        bonus_damage_delta: SimScalar::ZERO,
    }
}

#[test]
fn signed_limits_and_opposing_adjustments_are_exact() -> Result<(), Box<dyn std::error::Error>> {
    for boundary in [i32::MIN, i32::MAX] {
        let mut state = PlayerUpgradeState::new();
        state.apply_upgrade("boundary-fixture", &[adjustment(boundary, SimScalar::ZERO)])?;
        assert_eq!(
            state.unit_stat_modifiers["arithmetic-fixture"].attack_cooldown_delta,
            boundary
        );
        let undo = if boundary == i32::MIN {
            i32::MAX
        } else {
            -i32::MAX
        };
        state.apply_upgrade("opposing-fixture", &[adjustment(undo, SimScalar::ZERO)])?;
        assert_eq!(
            state.unit_stat_modifiers["arithmetic-fixture"].attack_cooldown_delta,
            if boundary == i32::MIN { -1 } else { 0 }
        );
    }
    let mut state = PlayerUpgradeState::new();
    state.apply_upgrade(
        "combined-fixture",
        &[
            adjustment(100, SimScalar::ZERO),
            adjustment(-100, SimScalar::ZERO),
        ],
    )?;
    assert_eq!(
        state.unit_stat_modifiers["arithmetic-fixture"].attack_cooldown_delta,
        0
    );
    Ok(())
}

#[test]
fn late_overflows_roll_back_every_effect_and_completion_identity(
) -> Result<(), Box<dyn std::error::Error>> {
    for (initial, overflow) in [(i32::MAX, 1), (i32::MIN, -1)] {
        let mut state = PlayerUpgradeState::new();
        state.apply_upgrade("initial-fixture", &[adjustment(initial, SimScalar::ZERO)])?;
        let before = state.clone();
        let effects = [
            UpgradeEffect::ShieldLevel { level: 2 },
            UpgradeEffect::UnlockAbility {
                target_slug: "arithmetic-fixture".into(),
                ability_slug: "test-unlock".into(),
            },
            adjustment(0, SimScalar::ONE),
            adjustment(overflow, SimScalar::ZERO),
        ];
        let error = state
            .apply_upgrade("rejected-fixture", &effects)
            .err()
            .ok_or("overflow accepted")?;
        assert_eq!(error.stat, UpgradeStat::AttackCooldown);
        assert_eq!(state, before);
    }
    let mut state = PlayerUpgradeState::new();
    state.apply_upgrade("initial-hp-fixture", &[adjustment(0, SimScalar::MAX)])?;
    let before = state.clone();
    let error = state
        .apply_upgrade("overflow-hp-fixture", &[adjustment(1, SimScalar::ONE)])
        .err()
        .ok_or("fixed-point overflow accepted")?;
    assert_eq!(error.stat, UpgradeStat::MaxHp);
    assert_eq!(state, before);
    Ok(())
}
