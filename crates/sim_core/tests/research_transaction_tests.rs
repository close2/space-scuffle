//! Research transactions preserve live vitals, resources and player state on errors.
//!
//! Fixtures reuse loaded authored identities while replacing effect values to
//! exercise signed aggregation and live-vital overflow through the real API.

use sim_core::{SimCoreError, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerId, SimScalar, SimVec2, UnitStatDeltas, UpgradeEffect};

fn effect(slug: &str, delta: i32, hp: SimScalar) -> UpgradeEffect {
    UpgradeEffect::ModifyUnitStat {
        target_slug: slug.into(),
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
fn research_failure_preserves_all_state_including_costs_and_live_entities(
) -> Result<(), Box<dyn std::error::Error>> {
    for (live_vital_overflow, invalid_identity) in [(false, false), (true, false), (false, true)] {
        let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = sim_fixtures::world(map, 12);
        let archetype = world.catalog.archetypes().next().ok_or("catalog empty")?;
        let unit_type = archetype.unit_type_id;
        let slug = archetype.slug.clone();
        let first = world.spawn_unit(unit_type, PlayerId(0), SimVec2::ZERO)?;
        let second = world.spawn_unit(unit_type, PlayerId(0), SimVec2::ZERO)?;
        let mut upgrade = world
            .upgrade_catalog
            .upgrades()
            .next()
            .ok_or("upgrade catalog empty")?
            .clone();
        let upgrade_slug = upgrade.slug.clone();
        upgrade.effects = vec![
            UpgradeEffect::ShieldLevel { level: 2 },
            effect(&slug, 0, SimScalar::ONE),
            effect(&slug, 1, SimScalar::ONE),
        ];
        world.upgrade_catalog = UpgradeCatalog::from_definitions([upgrade])?;
        if live_vital_overflow {
            world
                .entities
                .get_mut(&second)
                .ok_or("second absent")?
                .max_hp = SimScalar::MAX;
        } else if invalid_identity {
            let missing_type = (0..=u16::MAX)
                .map(sim_types::UnitTypeId)
                .find(|id| world.catalog.get(*id).is_err())
                .ok_or("catalog fills every ID")?;
            world
                .entities
                .get_mut(&second)
                .ok_or("second absent")?
                .unit_type_id = missing_type;
        } else {
            world
                .player_upgrade_state_mut(PlayerId(0))
                .ok_or("player 0 has no upgrade slot")?
                .unit_stat_modifiers
                .insert(
                    slug.clone(),
                    UnitStatDeltas {
                        attack_cooldown_delta: i32::MAX,
                        ..Default::default()
                    },
                );
        }
        let before_entities = world.entities.clone();
        let before_upgrades = world.player_upgrades.clone();
        let before_resources = world.player_resources;
        let error = world
            .research_upgrade(PlayerId(0), &upgrade_slug)
            .err()
            .ok_or("overflow accepted")?;
        if live_vital_overflow {
            assert!(
                matches!(error, SimCoreError::UpgradeEntityVitalsOverflow { entity } if entity == second)
            );
        } else if invalid_identity {
            assert!(matches!(error, SimCoreError::ArchetypeNotFound(_)));
        } else {
            assert!(matches!(error, SimCoreError::UpgradeApplication(_)));
        }
        assert_eq!(
            world.entities, before_entities,
            "even the earlier entity {first:?} must remain unchanged"
        );
        assert_eq!(world.player_upgrades, before_upgrades);
        assert_eq!(world.player_resources, before_resources);
        assert!(!world
            .player_upgrade_state(PlayerId(0))
            .ok_or("player 0 has no upgrade slot")?
            .completed_upgrade_slugs
            .contains(&upgrade_slug));
    }
    Ok(())
}
