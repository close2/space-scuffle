//! Behaviour of the test fixtures: the sandbox scene and the embedded shipped data.
//!
//! # Responsibility
//! Pins that the battle sandbox is built whole or not at all, that its scene is unchanged from
//! the constructor it replaced, and that the embedded catalogs are the files the game loads.
//!
//! # How it fits
//! Every other crate's tests build worlds through `sim_fixtures`; these tests make sure those
//! worlds are what the shipped data and the former `sim_core` constructors produced.
//!
//! # Invariants
//! - The sandbox reports a spawn it cannot make; it never returns a world with entities missing.
//! - The embedded catalogs equal `RuntimeCatalogs::load_from_directory("assets/catalog")`.

use sim_core::catalog::RuntimeCatalogs;
use sim_fixtures::{default_battle_sandbox, default_battle_sandbox_on, SandboxError};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerId, ShapeKind};
use std::error::Error;
use std::path::PathBuf;

/// Checksum of the default sandbox's spawned state, recorded from
/// `SimWorld::new_default_battle_sandbox` before Phase 9 WP10 moved it here. Re-recorded for
/// checksum schema 10 (plan 2026-11-04-battle-flipbook-animation WP2), which stopped encoding
/// each entity's constant-zero `anim_progress`; the spawned state itself did not change.
/// Re-recorded for plan 2026-11-04-combat-sc2-alignment WP9: a new entity's hashed
/// `ticks_since_last_damage` starts at `u16::MAX` instead of 250; nothing else changed.
/// Re-recorded for checksum schema 11 (plan 2026-11-05-sc2-deviation-fixes WP4, B-11), which
/// encodes each entity's `previous_target_position`, `None` at spawn.
/// Re-recorded for checksum schema 12 (plan 2026-11-05-sc2-deviation-fixes WP5, B-2), which
/// encodes each weapon's strike spacing and each entity's `pending_strikes`, `None` at spawn.
/// Re-recorded for checksum schema 13 (WP6, B-14), which encodes each weapon's bonus against
/// every attribute in place of its Armored and Light bonuses.
/// Re-recorded for checksum schema 14 (work package 8a, B-16), which encodes each splash as its
/// list of zones in place of three fixed radii.
/// Re-recorded for checksum schema 15 (work package 8b, B-18), which encodes each splash's shape.
/// Re-recorded for checksum schema 16 (work package 9, B-82), which encodes each weapon's
/// structure strike.
/// Re-recorded for checksum schema 17 (work package 10, B-29), which encodes each weapon's
/// acquisition arc and each unit's engaged target.
/// Re-recorded for checksum schema 18 (work package 10, B-30), which encodes each weapon's turret
/// idle behaviour.
/// Re-recorded for checksum schema 19 (work package 11, B-3), which encodes each weapon's random
/// delay range and each unit's carried delay.
/// Re-recorded for checksum schema 20 (work package 13a, B-41), which encodes each unit's barrier
/// window, absorption left and cooldown.
/// Re-recorded for checksum schema 21 (work package 13c, B-43), which encodes each unit's attack
/// dash cooldown.
/// Re-recorded for checksum schema 22 (plan 2026-11-09-sc2-pathfinding WP2+WP3, B-100), which
/// encodes each unit's `corner_round`, 0 at spawn.
/// Re-recorded for checksum schema 23 (owner answer to Q0058), which encodes whether each weapon's
/// burst and each pending burst is channeled, and schema 24 (owner answer to Q0063), which encodes
/// whether each splash is centred on its target's edge.
/// Re-recorded for checksum schema 25 (plan 2026-11-09-friendly-blocking WP3, B-104, B-105), which
/// encodes each unit's surround-walk state, all at its defaults at spawn.
/// Re-recorded for checksum schema 26 (B-85), which encodes each splash line's search period
/// and each unit's pending line searches, none at spawn.
/// Re-recorded for checksum schema 27 (plan 2026-11-10-group-routing WP3), which encodes each
/// unit's count of route orders issued (0 at spawn) and the world's route groups (none).
/// Re-recorded for checksum schema 28 (plan `2026-11-10-spellcaster-mechanics`, package 1b), which
/// encodes each weapon's melee flag, each unit's cast lock and displacement (none) and the
/// world's effect state (empty).
/// Re-recorded for checksum schema 29 (the same plan, package 2b, D-12), which no longer encodes
/// the hazard zones (none), the next hazard id or each unit's last storm damage tick (none).
/// Re-recorded for checksum schema 30 (plan `2026-11-18-contour-footprints` WP2), which encodes
/// each placed structure's footprint shape; a scratch build that does not encode the shape and
/// keeps schema 29 reproduces `1_650_891_098_077_756_775`.
/// Re-recorded for plan `2026-11-18-contour-footprints` Amendment 4 (B-142), which hashes each
/// unit's leash origin; a scratch build that leaves it unhashed reproduces
/// `13_611_770_232_129_032_728`.
/// Re-recorded for plan `2026-11-18-contour-footprints` WP6 (B-134, B-138): the sandbox's
/// structures take their 5.0.14 footprint shapes, which are hashed and change their clearance.
/// Every entity stands where it stood; a scratch run with the full-square catalog reproduces
/// `17_544_490_958_066_995_222`.
/// Re-recorded for checksum schema 31 (plan `2026-11-19-crowd-edge-steering-2` WP3, B-103), which
/// encodes each unit's surround-walk retreat, 0 at spawn; a scratch build that does not encode
/// it and keeps schema 30 reproduces `15_355_001_956_314_655_944`.
/// Re-recorded for plan `2026-11-20-resource-footprints` WP3 and WP5 (B-147): `default_arena`'s
/// nodes are re-laid (node positions are hashed) and schema 32 hashes each node's placed
/// footprint and each cell's resource occupant, whose outlines also change the clearance. Every
/// entity is as before: the checksum ending after the route groups, with the schema number held
/// at 31 (a scratch switch, not committed), is `3_802_618_527_547_176_050` on `e58f1af1` and on
/// this tree.
/// Re-recorded for checksum schema 33 (plan `2026-11-22-harvest-timing` WP2, B-150, B-151), which
/// encodes each unit's return-delay countdown, 0 at spawn; a scratch build that does not encode it
/// and keeps schema 32 reproduces `6_458_594_406_894_179_644`.
/// Re-recorded for checksum schema 34 (plan `2026-11-23-production-queue` WP1, D-6), which
/// encodes each queued job's `in_production` and each entity's rally; the sandbox's town halls
/// rally themselves at creation (D-7). A scratch build that does not encode either field and keeps
/// schema 33 reproduces `3_827_951_643_646_962_361`.
/// Re-recorded for checksum schema 35 (plan `2026-11-20-contact-solver`, Amendment 10), which
/// encodes each unit's side-rule commitment and remembered gap (none under the legacy pipeline);
/// a scratch build that leaves them unhashed and keeps schema 34 reproduces
/// `16_199_664_925_193_234_964`.
/// Re-recorded for checksum schema 36 (plan `2026-11-23-splash-centres` WP1, D-6), which encodes
/// each splash's impact and target-radius condition; a scratch build that leaves them unhashed
/// and keeps schema 35 reproduces `14_665_724_469_072_757_732`.
/// Re-recorded for checksum schema 37 (plan `2026-11-23-splash-cliff-levels` WP2, D-5), which
/// encodes each splash's cliff rule and other-level target strike; a scratch build that leaves
/// them unhashed and keeps schema 36 reproduces `10_589_561_826_036_639_628`.
/// Re-recorded for checksum schema 38 (plan `2026-11-23-strike-delays` WP1, D-6), which encodes
/// each weapon's first-strike delay; a scratch build that leaves it unhashed and keeps schema 37
/// reproduces `18_404_512_489_137_112_834`.
/// Re-recorded for checksum schema 39 (plan `2026-11-23-harvest-handoff` WP1, D-3), which encodes
/// each resource node's release tick; a scratch build that leaves them unhashed and keeps schema
/// 38 reproduces `5_609_708_170_486_988_781`.
/// Re-recorded for checksum schema 40 (plan `2026-11-23-turret-wind-up` WP1, D-9), which encodes
/// each weapon's movement rule and whether it fires outside an attack order; a scratch build that
/// leaves them unhashed and keeps schema 39 reproduces `8_582_058_566_732_369_859`.
/// Re-recorded for checksum schema 41 (plan `2026-11-23-vision-and-detection` WP4, D-7), which
/// encodes each entity's attack-reveal counters and each weapon's attack visibility; a scratch
/// build that leaves them unhashed and keeps schema 40 reproduces `5_438_070_395_987_993_470`.
/// Re-recorded for checksum schema 43 (plan `2026-11-24-weapon-validation` WP1, D-7), which
/// encodes each entity's teleport count, each projectile's target teleport count and each
/// weapon's missile validation; a scratch build that leaves them unhashed and keeps schema 41
/// reproduces `1_550_645_596_559_574_539`.
/// Re-recorded for checksum schema 44 (plan `2026-11-24-hidden-harvesters` WP2, D-3, on its merge
/// with `main` `7dc356fc`), which encodes each unit task's housing structure; a build that leaves
/// it unhashed and keeps schema 43 reproduces `17_697_741_365_715_682_877`.
/// Re-recorded for checksum schema 45 (plan `2026-11-24-fire-rules` WP1, D-8, on its merge with
/// `main` `d92e40bb`), which encodes each weapon's two fire rules and each entity's own weapon
/// target; a scratch build that leaves them unhashed and keeps schema 44 reproduces
/// `8_410_876_910_713_629_694`. No turret moves in the sandbox, so the plan's other rules change
/// nothing here.
/// Re-recorded for checksum schema 46 (plan `2026-11-20-contact-solver` WP6, on its merge with
/// `main` `8a15bff9`), which encodes nothing new and marks the solver becoming the default
/// pipeline; the sandbox is checked before its first tick, so only the encoded schema number moves
/// it. A copy of the merged tree at schema 45 with the pipeline at Legacy reproduces
/// `6_724_706_104_475_870_814` on all four builds.
/// Re-recorded for checksum schema 47, provisional (plan `2026-11-24-weapon-validation` WP7,
/// Amendment 2, on its merge with `main` `a59368a3`), which encodes each unit's last seen point of
/// its attack order's target; a copy of the merged tree that leaves it unhashed and keeps schema
/// 46 reproduces `9_879_951_760_932_463_822` on all four builds.
/// Re-recorded for checksum schema 48, provisional (plan `2026-11-25-rallies` WP1), which encodes
/// each entity's rally as its routes; a copy of the WP1 tree that encodes slot 0's first waypoint
/// as the old rally and keeps schema 47 reproduces `16_195_585_963_966_674_183` on all four
/// builds.
/// Re-recorded for checksum schema 49, provisional (plan `2026-11-26-turret-tracking-weapon` WP3
/// and Amendment 1), which encodes whether each weapon's turret tracks any enemy and the world's
/// table of the enemies tracking turrets follow; a copy of the final tree that leaves both
/// unhashed and keeps schema 48 reproduces `601_552_017_353_564_848` on all four builds.
/// Re-recorded for checksum schema 50, provisional (plan `2026-11-26-follow-order` WP1), which
/// encodes each unit's follow rest and the follow order; a copy of the WP1 tree that leaves the
/// rest unhashed and keeps schema 49 reproduces `2_504_113_823_383_333_194` on all four builds.
const DEFAULT_SANDBOX_CHECKSUM: u64 = 13_849_945_286_125_516_044;

fn shipped_catalog_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog")
}

#[test]
fn sandbox_on_a_map_too_small_for_its_scene_is_an_error() -> Result<(), Box<dyn Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let result = default_battle_sandbox_on(map);
    assert!(
        matches!(result, Err(SandboxError::Spawn { .. })),
        "a scene whose entities lie outside a 32x32 map must fail, got {:?}",
        result.map(|world| world.entities.len())
    );
    Ok(())
}

#[test]
fn default_sandbox_spawns_both_armies_and_bases_whole() -> Result<(), Box<dyn Error>> {
    let world = default_battle_sandbox()?;
    let count = |owner: PlayerId, structure: bool| {
        world
            .entities
            .values()
            .filter(|e| e.owner == owner)
            .filter(|e| {
                world
                    .catalog
                    .get(e.unit_type_id)
                    .is_ok_and(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }) == structure)
            })
            .count()
    };
    assert_eq!(
        count(PlayerId(0), false),
        12,
        "ten ranged and two armoured units"
    );
    assert_eq!(
        count(PlayerId(1), false),
        10,
        "eight melee and two turreted units"
    );
    assert_eq!(
        count(PlayerId(0), true),
        3,
        "command, supply and production"
    );
    assert_eq!(
        count(PlayerId(1), true),
        3,
        "command, supply and production"
    );
    assert_eq!(world.last_checksum(), world.compute_checksum());
    assert_eq!(world.last_checksum(), DEFAULT_SANDBOX_CHECKSUM);
    Ok(())
}

#[test]
fn embedded_catalogs_are_the_shipped_files() -> Result<(), Box<dyn Error>> {
    let embedded = sim_fixtures::shipped_catalogs();
    let on_disk = RuntimeCatalogs::load_from_directory(shipped_catalog_dir())?;
    let slugs = |catalogs: &RuntimeCatalogs| {
        catalogs
            .units
            .archetypes()
            .map(|a| (a.unit_type_id, a.slug.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(slugs(&embedded), slugs(&on_disk));
    assert_eq!(
        embedded.units.movement_rules(),
        on_disk.units.movement_rules()
    );
    assert_eq!(
        embedded.units.placement_rules(),
        on_disk.units.placement_rules()
    );
    assert_eq!(embedded.units.spells(), on_disk.units.spells());
    assert_eq!(
        embedded
            .upgrades
            .upgrades()
            .map(|u| u.upgrade_id)
            .collect::<Vec<_>>(),
        on_disk
            .upgrades
            .upgrades()
            .map(|u| u.upgrade_id)
            .collect::<Vec<_>>()
    );
    let loadouts = sim_core::scenario::StartLoadoutCatalog::load(
        shipped_catalog_dir().join("start_loadouts.ron"),
    )?;
    assert_eq!(sim_fixtures::shipped_loadouts(), loadouts);
    Ok(())
}
