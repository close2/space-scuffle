//! Checks actual simulation-to-render owner provenance through firing and death.
//! Catalog identities are discovered by capabilities; the fixture varies weapon
//! presentation kinds to cover every producer without introducing creature IDs.

use sim_core::{catalog::RuntimeCatalogs, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    CombatEventVisual, MoverType, PlayerId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, WeaponVisualKind,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture(
    kind: WeaponVisualKind,
    owner: PlayerId,
) -> Result<(SimWorld, sim_types::EntityId), Box<dyn std::error::Error>> {
    let mut catalogs = RuntimeCatalogs::load_from_directory(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog"),
    )?;
    let source = catalogs
        .units
        .archetypes()
        .find(|arch| {
            matches!(arch.shape, ShapeKind::UnitSphere)
                && arch.mover_type == MoverType::Ground
                && arch
                    .weapon_against(MoverType::Ground, None)
                    .is_some_and(|weapon| {
                        weapon.min_range == SimScalar::ZERO
                            && weapon.max_range >= SimScalar::from_num(3)
                    })
        })
        .ok_or("authored mobile ranged source")?
        .unit_type_id;
    let target = catalogs
        .units
        .archetypes()
        .find(|arch| {
            matches!(arch.shape, ShapeKind::UnitSphere)
                && arch.mover_type == MoverType::Ground
                && arch.unit_type_id != source
        })
        .ok_or("authored separate target")?
        .unit_type_id;
    let arch = catalogs.units.get_mut(source)?;
    let mut weapon = arch
        .weapon_against(MoverType::Ground, None)
        .ok_or("source weapon")?
        .clone();
    weapon.visual_kind = kind;
    weapon.projectile_speed = if kind == WeaponVisualKind::Projectile {
        Some(SimScalar::from_num(0.5))
    } else {
        None
    };
    arch.ground_weapon = Some(weapon.clone());
    arch.weapon = Some(weapon);
    let arch = catalogs.units.get_mut(target)?;
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::with_catalogs(map, 40, catalogs.units, catalogs.upgrades);
    let source = world.spawn_unit(source, owner, SimVec2::from_i32(10, 10))?;
    let target_owner = if owner == PlayerId(0) {
        PlayerId(7)
    } else {
        PlayerId(0)
    };
    let target = world.spawn_unit(target, target_owner, SimVec2::from_i32(12, 10))?;
    assert!(world
        .entities
        .get_mut(&source)
        .ok_or("source actor")?
        .issue_order(UnitOrder::AttackTarget { target }, false)?);
    Ok((world, source))
}

fn matches_kind(event: &CombatEventVisual, kind: WeaponVisualKind) -> bool {
    matches!(
        (event, kind),
        (
            CombatEventVisual::HitscanTracer { .. },
            WeaponVisualKind::Hitscan
        ) | (
            CombatEventVisual::ProjectileVisual { .. },
            WeaponVisualKind::Projectile
        ) | (
            CombatEventVisual::BeamVisual {
                is_lightning: false,
                ..
            },
            WeaponVisualKind::LaserBeam
        ) | (
            CombatEventVisual::BeamVisual {
                is_lightning: true,
                ..
            },
            WeaponVisualKind::LightningArc
        ) | (
            CombatEventVisual::MeleeSlashVisual { .. },
            WeaponVisualKind::MeleeSlash
        )
    )
}

#[test]
fn every_combat_producer_retains_actual_sparse_owner_after_source_death() -> TestResult {
    for owner in [PlayerId(0), PlayerId(2), PlayerId(7)] {
        for kind in [
            WeaponVisualKind::Hitscan,
            WeaponVisualKind::Projectile,
            WeaponVisualKind::LaserBeam,
            WeaponVisualKind::LightningArc,
            WeaponVisualKind::MeleeSlash,
        ] {
            let (mut world, source) = fixture(kind, owner)?;
            let mut observed = None;
            for _ in 0..100 {
                let snapshot = world.step(&TickBundle {
                    tick: Tick(world.current_tick),
                    commands: Vec::new(),
                    state_checksum: None,
                });
                assert!(
                    world.diagnostics().is_empty(),
                    "unexpected diagnostics: {:?}",
                    world.diagnostics()
                );
                if let Some(event) = snapshot
                    .combat_events
                    .into_iter()
                    .find(|event| matches_kind(event, kind))
                {
                    observed = Some(event);
                    break;
                }
            }
            let event = observed.ok_or("actual combat phase did not emit expected visual")?;
            assert_eq!(event.owner(), owner);
            world
                .entities
                .get_mut(&source)
                .ok_or("source remains before reap")?
                .hp = SimScalar::ZERO;
            world.reap_dead_and_emit_shatter();
            world.validate_supply_ledger()?;
            assert!(!world.entities.contains_key(&source));
            let death = world
                .build_render_snapshot()
                .combat_events
                .into_iter()
                .find(|event| matches!(event, CombatEventVisual::BubbleShatter { .. }))
                .ok_or("source death visual")?;
            assert_eq!(death.owner(), owner);
            assert_eq!(event.owner(), owner);
        }
    }
    Ok(())
}
