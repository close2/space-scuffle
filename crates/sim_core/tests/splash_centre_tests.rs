//! # Integration Test: where an area lands (plan `2026-11-23-splash-centres`)
//!
//! # Responsibility
//! The two capability fields of `SplashSpec` and the shipped rows that use them, played through
//! `SimWorld::step` on small flat maps:
//! - P-1 ([`SplashImpact::Attacker`]): an attacker-centred area strikes every damageable enemy
//!   whose edge lies within its zones measured from the attacker, the target included and struck
//!   once by the area, with no direct strike and so no weapon hit effect; the attacker is never
//!   struck (`StarCraft` II `ImpactLocation CasterUnit`, `liberty` EffectData:1759).
//! - P-3 (`target_radius_below`): an area opens only around a target whose collision radius is
//!   below the limit; a target at or above it takes the strike alone and no `SplashVisual` is
//!   published (`CrucioShockCannonSwitch`, `swarm` EffectData:2183–2187; `TargetRadiusLarge`,
//!   `liberty` ValidatorData:824–833). A projectile landing on a point has radius zero, so it
//!   always opens its area.
//! - D-1's refusal: an upgrade may not unlock a weapon hit effect for an archetype whose every
//!   weapon has an attacker-centred area, since such a weapon makes no direct strike.
//!
//! # How it fits
//! The rules live in `combat_resolution.rs` (phase 7) and the catalog validation; this file
//! plays them through the public API. A scratch weapon is a copy of a shipped circular splash
//! weapon (the one with the largest reach), found by property, made instant and given plain
//! numbers so each expectation is computed from the copy's own zones. Bodies are scratch clones
//! of a shipped mobile ground unit, unarmed and unarmoured. Nothing is chosen by name (charter
//! §3.3); the one shipped archetype with a weapon hit effect is found through the upgrade
//! catalog's data.
//!
//! # Invariants
//! - Every tick played raises no simulation diagnostic.
//! - Boundaries are set 2⁻¹⁶ world units either side of an exact fixed-point reach.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttributeBonuses, AttributeFlags, CombatEventVisual, EntityId, MoverType, PlayerCommand,
    PlayerId, ProjectileTargetLossBehavior, RenderSnapshot, ShapeKind, SimAngle, SimScalar,
    SimVec2, SplashImpact, SplashShape, SplashZone, Tick, TickBundle, UnitOrder, UnitTypeId,
    UpgradeEffect, WeaponSpec, WeaponVisualKind,
};

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The collision radius of every scratch body and of the scratch attacker, world units.
const BODY: f64 = 0.25;
/// Where the attacker stands, both coordinates.
const CENTRE: f64 = 20.0;
/// The scratch weapon's damage: no bonus, no armour, so a strike deals `fraction × DAMAGE`.
const DAMAGE: i32 = 10;

fn s(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

/// One 2⁻¹⁶ world unit: the margin each boundary test sets either side of a reach.
fn epsilon() -> SimScalar {
    SimScalar::from_raw(1 << 16)
}

/// The attacker's position.
fn centre() -> SimVec2 {
    SimVec2::new(s(CENTRE), s(CENTRE))
}

/// `from` moved by `(dx, dy)` world units.
fn offset(from: SimVec2, dx: SimScalar, dy: SimScalar) -> SimVec2 {
    SimVec2::new(from.x + dx, from.y + dy)
}

/// A world on a flat 48 × 48 map with the shipped catalogs and no random weapon delays.
fn world() -> SimWorld {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a valid size");
    let mut world = sim_fixtures::world(map, 0x5_C3E7);
    sim_fixtures::without_random_delays(&mut world.catalog);
    world
}

/// A unit type id no archetype of `world`'s catalog uses.
fn free_id(world: &SimWorld) -> UnitTypeId {
    (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("invariant: a free unit type id")
}

/// The shipped circular splash weapon with the largest reach (the first of equals), copied as
/// a scratch weapon: instant with no delivery look, aimed all round, `DAMAGE` with no bonus,
/// upgrade or structure strike, able to strike anything, not suicidal, its range its reach, and
/// its area centred on the target's centre, always open, enemies only.
fn scratch_weapon(world: &SimWorld) -> WeaponSpec {
    let mut best: Option<&WeaponSpec> = None;
    for weapon in world
        .catalog
        .archetypes()
        .flat_map(|arch| [&arch.weapon, &arch.ground_weapon, &arch.air_weapon])
        .flatten()
    {
        let Some(splash) = weapon.splash.as_ref() else {
            continue;
        };
        if splash.shape != SplashShape::Circle {
            continue;
        }
        let larger = best
            .and_then(|best| best.splash.as_ref())
            .is_none_or(|best| splash.reach() > best.reach());
        if larger {
            best = Some(weapon);
        }
    }
    let mut weapon = best
        .cloned()
        .expect("invariant: the shipped catalog has a circular splash weapon");
    weapon.visual_kind = WeaponVisualKind::None;
    weapon.target_flags = AttributeFlags::all();
    weapon.min_range = SimScalar::ZERO;
    weapon.firing_arc = SimAngle::from_degrees(360);
    weapon.acquire_arc = SimAngle::from_degrees(360);
    weapon.base_damage = SimScalar::from_i32(DAMAGE);
    weapon.attribute_bonuses = AttributeBonuses::NONE;
    weapon.attacks_count = 1;
    weapon.weapon_class = None;
    weapon.upgrade_bonus_per_level = SimScalar::ZERO;
    weapon.bonus_upgrade_per_level = SimScalar::ZERO;
    weapon.projectile_speed = None;
    weapon.suicide_on_attack = false;
    weapon.structure_strike = None;
    weapon.melee = false;
    let splash = weapon.splash.as_mut().expect("chosen for its splash");
    splash.offset_by_unit_radius = false;
    splash.friendly_fire = false;
    splash.impact = SplashImpact::Target;
    splash.target_radius_below = None;
    weapon.max_range = splash.reach();
    weapon
}

/// The splash of `weapon`, which has one.
fn splash_of(weapon: &WeaponSpec) -> &sim_types::SplashSpec {
    weapon.splash.as_ref().expect("a splash weapon")
}

/// Registers a clone of the first shipped mobile ground unit under a free id: collision radius
/// `radius` (world units), 1000 life, no shields, no armour, Light only, unarmed and without
/// capabilities; `edit` changes it further.
fn register(
    world: &mut SimWorld,
    slug: &str,
    radius: SimScalar,
    edit: impl FnOnce(&mut UnitArchetype),
) -> UnitTypeId {
    let mut arch = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !matches!(arch.shape, ShapeKind::BuildingDome { .. })
        })
        .expect("invariant: the shipped catalog has a mobile ground unit")
        .clone();
    let free = free_id(world);
    arch.unit_type_id = free;
    slug.clone_into(&mut arch.slug);
    slug.clone_into(&mut arch.name);
    arch.collision_radius = radius;
    arch.terrain_radius = radius.min(s(BODY));
    arch.separation_radius = radius;
    arch.max_hp = SimScalar::from_i32(1000);
    arch.max_shields = SimScalar::ZERO;
    arch.life_armor = SimScalar::ZERO;
    arch.shield_armor = SimScalar::ZERO;
    arch.hp_regen_per_tick = SimScalar::ZERO;
    arch.attributes = AttributeFlags::LIGHT;
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    arch.modes = Vec::new();
    arch.capabilities.overclock = None;
    arch.capabilities.barrier = None;
    arch.capabilities.attack_dash = None;
    edit(&mut arch);
    world.catalog.register(arch);
    free
}

/// Registers a scratch body of radius `radius` and spawns it for `owner` at `at`.
fn body(world: &mut SimWorld, owner: PlayerId, radius: SimScalar, at: SimVec2) -> EntityId {
    let kind = register(world, "splash_centre_body", radius, |_| {});
    world.spawn_unit(kind, owner, at).expect("open ground")
}

/// Registers a scratch attacker of radius [`BODY`] carrying `weapon` and spawns it for blue at
/// the centre.
fn gunner(world: &mut SimWorld, weapon: WeaponSpec) -> EntityId {
    let kind = register(world, "splash_centre_gunner", s(BODY), |arch| {
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon);
    });
    world.spawn_unit(kind, BLUE, centre()).expect("open ground")
}

/// Hull and shield points of `id`, or 0 once it is gone.
fn vitals(world: &SimWorld, id: EntityId) -> SimScalar {
    world
        .entities
        .get(&id)
        .map_or(SimScalar::ZERO, |entity| entity.hp + entity.shields)
}

/// Plays one tick with `commands`, asserts it raised no diagnostic and returns its snapshot.
fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) -> RenderSnapshot {
    let tick = Tick(world.current_tick);
    let snapshot = world.step(&TickBundle {
        tick,
        commands,
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    snapshot
}

/// Orders `attacker` (blue) onto `target` and plays until any of `watched` loses vitals, at
/// most 80 ticks. Returns the life each of `watched` lost and the snapshot of that tick.
fn first_strike(
    world: &mut SimWorld,
    attacker: EntityId,
    target: EntityId,
    watched: &[EntityId],
) -> (Vec<SimScalar>, RenderSnapshot) {
    let before: Vec<_> = watched.iter().map(|&id| vitals(world, id)).collect();
    let order = PlayerCommand {
        player_id: BLUE,
        entities: vec![attacker],
        order: UnitOrder::attack(target),
        queue: false,
    };
    let mut snapshot = step_with(world, vec![order]);
    for _ in 0..80 {
        if watched
            .iter()
            .zip(&before)
            .any(|(&id, &full)| vitals(world, id) < full)
        {
            break;
        }
        snapshot = step_with(world, Vec::new());
    }
    let losses = watched
        .iter()
        .zip(&before)
        .map(|(&id, &full)| full - vitals(world, id))
        .collect();
    (losses, snapshot)
}

/// The number of `SplashVisual` events in `snapshot`.
fn splash_visuals(snapshot: &RenderSnapshot) -> usize {
    snapshot
        .combat_events
        .iter()
        .filter(|event| matches!(event, CombatEventVisual::SplashVisual { .. }))
        .count()
}

/// The strike a unit `distance` from the search centre (to its edge) takes from `weapon`.
fn strike_at(weapon: &WeaponSpec, distance: SimScalar) -> SimScalar {
    splash_of(weapon)
        .fraction_at(distance)
        .map_or(SimScalar::ZERO, |fraction| {
            SimScalar::from_i32(DAMAGE) * fraction
        })
}

/// P-1: an attacker-centred area strikes the enemies whose edge lies within its reach of the
/// attacker on every side, 2⁻¹⁶ inside, and not those 2⁻¹⁶ outside, even beyond its target
/// where a target-centred area would reach; the target is struck by the area, once; the attacker
/// and a friendly unit inside the reach are untouched.
#[test]
fn an_attacker_centred_area_strikes_around_the_attacker() {
    let mut world = world();
    let mut weapon = scratch_weapon(&world);
    weapon.splash.as_mut().expect("a splash weapon").impact = SplashImpact::Attacker;
    let reach = splash_of(&weapon).reach();
    let radius = s(BODY);
    let attacker = gunner(&mut world, weapon.clone());
    // The target's edge is 0.1 beyond the attacker's: well inside every shipped reach.
    let gap = s(0.1);
    let target = body(
        &mut world,
        RED,
        radius,
        offset(centre(), radius + radius + gap, SimScalar::ZERO),
    );
    let inside = reach + radius - epsilon();
    let outside = reach + radius + epsilon();
    let west = body(
        &mut world,
        RED,
        radius,
        offset(centre(), -inside, SimScalar::ZERO),
    );
    let south = body(
        &mut world,
        RED,
        radius,
        offset(centre(), SimScalar::ZERO, -inside),
    );
    let north = body(
        &mut world,
        RED,
        radius,
        offset(centre(), SimScalar::ZERO, outside),
    );
    let east = body(
        &mut world,
        RED,
        radius,
        offset(centre(), outside, SimScalar::ZERO),
    );
    let friend = body(
        &mut world,
        BLUE,
        radius,
        offset(centre(), -s(0.5) - radius, s(0.5) + radius),
    );
    // The east body would be reached by the same area centred on the target.
    let target_position = world.entities[&target].position;
    let east_from_target = world.entities[&east].position.distance(target_position) - radius;
    assert!(east_from_target <= reach, "the control geometry");

    let watched = [target, west, south, north, east, friend, attacker];
    let (losses, snapshot) = first_strike(&mut world, attacker, target, &watched);
    assert_eq!(
        losses[0],
        strike_at(&weapon, radius + gap),
        "the target, struck once by the area"
    );
    assert_eq!(
        losses[1],
        strike_at(&weapon, reach - epsilon()),
        "west, inside"
    );
    assert_eq!(
        losses[2],
        strike_at(&weapon, reach - epsilon()),
        "south, inside"
    );
    assert!(losses[1] > SimScalar::ZERO, "the reach is inclusive");
    assert_eq!(losses[3], SimScalar::ZERO, "north, outside");
    assert_eq!(
        losses[4],
        SimScalar::ZERO,
        "east, beyond the target, outside"
    );
    assert_eq!(losses[5], SimScalar::ZERO, "a friend inside the reach");
    assert_eq!(losses[6], SimScalar::ZERO, "the attacker");
    let centres: Vec<_> = snapshot
        .combat_events
        .iter()
        .filter_map(|event| match event {
            CombatEventVisual::SplashVisual { center, .. } => Some([center[0], center[1]]),
            _ => None,
        })
        .collect();
    let expected = [centre().x.to_f32(), centre().y.to_f32()];
    assert_eq!(centres, vec![expected], "one search, at the attacker");
}

/// The first shipped archetype an upgrade unlocks a weapon hit effect for, by the upgrade
/// catalog's data, with that upgrade's slug.
fn weapon_hit_effect_unlock(world: &SimWorld) -> (UnitTypeId, String) {
    world
        .upgrade_catalog
        .upgrades()
        .find_map(|upgrade| {
            upgrade.effects.iter().find_map(|effect| match effect {
                UpgradeEffect::WeaponHitEffect { target_slug, .. } => world
                    .catalog
                    .id_by_slug(target_slug)
                    .map(|id| (id, upgrade.slug.clone())),
                _ => None,
            })
        })
        .expect("invariant: the shipped upgrades unlock a weapon hit effect")
}

/// One shot of a two-zone scratch weapon (the inner zone half the reach at the whole strike,
/// the outer at half) centred as `impact` says, fired by the shipped archetype with a weapon
/// hit effect (its upgrade researched) at a body whose edge lies three quarters of the reach
/// from the attacker's centre. Returns the target's loss, the strike the outer zone deals,
/// whether the target was slowed and whether it was ordered away (its retaliation: a helpless
/// unit flees).
fn hit_effect_shot(impact: SplashImpact) -> (SimScalar, SimScalar, bool, bool) {
    let mut world = world();
    let (kind, upgrade) = weapon_hit_effect_unlock(&world);
    let mut weapon = scratch_weapon(&world);
    let reach = splash_of(&weapon).reach();
    let splash = weapon.splash.as_mut().expect("a splash weapon");
    splash.zones = vec![
        SplashZone {
            radius: reach * SimScalar::HALF,
            fraction: SimScalar::ONE,
        },
        SplashZone {
            radius: reach,
            fraction: SimScalar::HALF,
        },
    ];
    splash.impact = impact;
    let arch = world.catalog.get_mut(kind).expect("a shipped archetype");
    arch.weapon = Some(weapon.clone());
    arch.ground_weapon = Some(weapon.clone());
    arch.air_weapon = None;
    arch.modes = Vec::new();
    let attacker_radius = arch.collision_radius;
    world
        .research_upgrade(BLUE, &upgrade)
        .expect("research admitted");
    let attacker = world.spawn_unit(kind, BLUE, centre()).expect("open ground");
    let radius = s(BODY);
    let edge = reach * s(0.75);
    assert!(
        edge > attacker_radius,
        "the target stands clear of the attacker"
    );
    let target = body(
        &mut world,
        RED,
        radius,
        offset(centre(), edge + radius, SimScalar::ZERO),
    );
    let (losses, _) = first_strike(&mut world, attacker, target, &[target]);
    let entity = &world.entities[&target];
    let slowed = entity.status_modifiers().move_speed_factor < SimScalar::ONE;
    let fled = matches!(entity.order_queue.front(), Some(UnitOrder::Move { .. }));
    (losses[0], strike_at(&weapon, edge), slowed, fled)
}

/// D-1: an attacker-centred area is the only damage of its shot. A target in its outer zone
/// takes that zone's share, not a whole direct strike; no weapon hit effect runs on it, while
/// the same shot centred on the target runs it (the control); and the target still answers the
/// attack (its retaliation runs through the area).
#[test]
fn an_attacker_centred_area_makes_no_direct_strike() {
    let (loss, outer, slowed, fled) = hit_effect_shot(SplashImpact::Attacker);
    assert_eq!(loss, outer, "one strike of the outer zone");
    assert!(
        outer < SimScalar::from_i32(DAMAGE),
        "a share, not a whole strike"
    );
    assert!(!slowed, "no weapon hit effect");
    assert!(fled, "the target's retaliation ran");

    let (loss, _, slowed, _) = hit_effect_shot(SplashImpact::Target);
    assert_eq!(
        loss,
        SimScalar::from_i32(DAMAGE),
        "the control: a direct strike"
    );
    assert!(
        slowed,
        "the control: the weapon hit effect runs on a direct strike"
    );
}

/// One shot of the scratch weapon with `target_radius_below = limit` at a body of `radius`,
/// with a bystander in its first zone. Returns the target's loss, the bystander's and the
/// `SplashVisual` count of the strike's tick.
fn conditional_shot(
    limit: SimScalar,
    radius: SimScalar,
    point_impact: bool,
) -> (SimScalar, SimScalar, usize) {
    let mut world = world();
    let mut weapon = scratch_weapon(&world);
    let splash = weapon.splash.as_mut().expect("a splash weapon");
    splash.target_radius_below = Some(limit);
    let first_zone = splash.zones[0].radius;
    if point_impact {
        weapon.projectile_speed = Some(s(0.2));
        weapon.visual_kind = WeaponVisualKind::Projectile;
        weapon.target_loss_behavior = ProjectileTargetLossBehavior::ImpactLastKnownPosition;
        weapon.max_projectile_lifetime_ticks = 400;
    }
    let attacker = gunner(&mut world, weapon.clone());
    let gap = s(0.1);
    let target_at = offset(centre(), s(BODY) + radius + gap, SimScalar::ZERO);
    let target = body(&mut world, RED, radius, target_at);
    assert!(
        radius + gap <= first_zone,
        "the bystander stands in the first zone"
    );
    // The bystander's edge lies `radius + gap` from the target's centre and `gap` from its
    // edge: in the first zone of the area around the target's centre, which must reach that far.
    let bystander = body(
        &mut world,
        RED,
        s(BODY),
        offset(target_at, SimScalar::ZERO, radius + s(BODY) + gap),
    );
    if point_impact {
        let order = PlayerCommand {
            player_id: BLUE,
            entities: vec![attacker],
            order: UnitOrder::attack(target),
            queue: false,
        };
        step_with(&mut world, vec![order]);
        for _ in 0..80 {
            if !world.projectiles.is_empty() {
                break;
            }
            step_with(&mut world, Vec::new());
        }
        assert!(!world.projectiles.is_empty(), "the projectile is in flight");
        // The target dies in flight: the projectile lands on its last position, a point.
        world.entities.get_mut(&target).expect("alive").hp = SimScalar::ZERO;
        let before = vitals(&world, bystander);
        let mut visuals = 0;
        for _ in 0..80 {
            let snapshot = step_with(&mut world, Vec::new());
            visuals += splash_visuals(&snapshot);
            if world.projectiles.is_empty() {
                break;
            }
        }
        return (SimScalar::ZERO, before - vitals(&world, bystander), visuals);
    }
    let (losses, snapshot) = first_strike(&mut world, attacker, target, &[target, bystander]);
    (losses[0], losses[1], splash_visuals(&snapshot))
}

/// P-3: with `target_radius_below = L`, a target of radius `L − 2⁻¹⁶` is struck and its area
/// opens (the bystander struck, one `SplashVisual`); at `L` and `L + 2⁻¹⁶` the target takes the
/// same strike alone and nothing is published for the area.
#[test]
fn a_conditional_area_opens_only_below_its_target_radius() {
    let limit = SimScalar::ONE;
    let whole = SimScalar::from_i32(DAMAGE);
    let (target, bystander, visuals) = conditional_shot(limit, limit - epsilon(), false);
    assert_eq!(target, whole, "below the limit: the direct strike");
    assert!(
        bystander > SimScalar::ZERO,
        "below the limit: the area opens"
    );
    assert_eq!(visuals, 1, "below the limit: one SplashVisual");
    for radius in [limit, limit + epsilon()] {
        let (target, bystander, visuals) = conditional_shot(limit, radius, false);
        assert_eq!(target, whole, "radius {radius}: the same direct strike");
        assert_eq!(bystander, SimScalar::ZERO, "radius {radius}: no area");
        assert_eq!(visuals, 0, "radius {radius}: no SplashVisual");
    }
}

/// D-3: a projectile that lands on a point (`ImpactLastKnownPosition`, its target dead in
/// flight) has radius zero, so its conditional area opens even where the target it was fired
/// at is too large to open it.
#[test]
fn a_point_impact_opens_a_conditional_area() {
    let limit = SimScalar::ONE;
    let (_, bystander, visuals) = conditional_shot(limit, limit, true);
    assert!(bystander > SimScalar::ZERO, "the area opens on the point");
    assert_eq!(visuals, 1);
}

/// D-1: an upgrade may not unlock a weapon hit effect for an archetype whose every weapon has
/// an attacker-centred area: such a weapon makes no direct strike, so the effect would never
/// run. The shipped catalogs pass; the same catalogs with that archetype's weapons made
/// attacker-centred are refused, naming the unlock's target.
#[test]
fn an_unlock_for_an_attacker_centred_weapon_is_refused() {
    let mut catalogs = sim_fixtures::shipped_catalogs();
    assert!(catalogs
        .units
        .validate_upgrade_references(&catalogs.upgrades)
        .is_ok());
    let target = catalogs
        .upgrades
        .upgrades()
        .find_map(|upgrade| {
            upgrade.effects.iter().find_map(|effect| match effect {
                UpgradeEffect::WeaponHitEffect { target_slug, .. } => {
                    catalogs.units.id_by_slug(target_slug)
                }
                _ => None,
            })
        })
        .expect("invariant: the shipped upgrades unlock a weapon hit effect");
    let world = world();
    let mut weapon = scratch_weapon(&world);
    weapon.splash.as_mut().expect("a splash weapon").impact = SplashImpact::Attacker;
    let arch = catalogs.units.get_mut(target).expect("a shipped archetype");
    for slot in [
        &mut arch.weapon,
        &mut arch.ground_weapon,
        &mut arch.air_weapon,
    ] {
        if slot.is_some() {
            *slot = Some(weapon.clone());
        }
    }
    for mode in &mut arch.modes {
        for slot in [
            &mut mode.ground_weapon_override,
            &mut mode.air_weapon_override,
        ] {
            if let Some(Some(_)) = slot {
                *slot = Some(Some(weapon.clone()));
            }
        }
    }
    let refused = catalogs
        .units
        .validate_upgrade_references(&catalogs.upgrades);
    assert!(
        matches!(
            &refused,
            Err(sim_core::SimCoreError::CatalogInvalidValue { field, .. })
                if field == "WeaponHitEffect.target_slug"
        ),
        "{refused:?}"
    );
}

/// The shipped suicide weapon with an area (the Baneling's burst), found by property, with the
/// type that carries it.
fn shipped_burst(world: &SimWorld) -> (UnitTypeId, WeaponSpec) {
    world
        .catalog
        .archetypes()
        .find_map(|arch| {
            [&arch.weapon, &arch.ground_weapon]
                .into_iter()
                .flatten()
                .find(|weapon| weapon.suicide_on_attack && weapon.splash.is_some())
                .map(|weapon| (arch.unit_type_id, weapon.clone()))
        })
        .expect("invariant: the shipped catalog has a suicide weapon with an area")
}

/// The first shipped mobile ground unit, other than `except`, without shields and of collision
/// radius at most 0.5: the struck units of the shipped bursts.
fn shipped_infantry(world: &SimWorld, except: UnitTypeId) -> UnitTypeId {
    world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.unit_type_id != except
                && arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && matches!(arch.shape, ShapeKind::UnitSphere)
                && arch.max_shields == SimScalar::ZERO
                && arch.collision_radius <= SimScalar::HALF
                && !arch.attributes.contains(AttributeFlags::MASSIVE)
        })
        .map(|arch| arch.unit_type_id)
        .expect("invariant: the shipped catalog has small unshielded infantry")
}

/// P-1, P-2 on the shipped row (B-83): the Baneling analogue's burst is centred on it. Its
/// target takes exactly the strike a direct hit would deal; an enemy whose edge lies its reach
/// less 2⁻¹⁶ behind the Baneling is struck; one whose edge lies the same distance beyond the
/// target's near edge (where the former area, centred there, reached) but beyond the reach of
/// the Baneling is not.
#[test]
fn the_banelings_burst_is_centred_on_it() {
    let mut world = world();
    let (burst, weapon) = shipped_burst(&world);
    let infantry = shipped_infantry(&world, burst);
    let reach = splash_of(&weapon).reach();
    let arch = world.catalog.get_mut(burst).expect("shipped");
    // The burst must land before the infantry's own fire could end the Baneling.
    arch.max_hp = SimScalar::from_i32(100_000);
    let own = arch.collision_radius;
    let target_arch = world.catalog.get(infantry).expect("shipped").clone();
    let radius = target_arch.collision_radius;
    let baneling = world
        .spawn_unit_facing(burst, BLUE, centre(), SimAngle::ZERO)
        .expect("open ground");
    let gap = s(0.1);
    let target_at = offset(centre(), own + radius + gap, SimScalar::ZERO);
    let spawn = |world: &mut SimWorld, at: SimVec2| {
        let id = world.spawn_unit(infantry, RED, at).expect("open ground");
        world
            .entities
            .get_mut(&id)
            .expect("spawned")
            .issue_order(UnitOrder::HoldPosition, false)
            .expect("hold admitted");
        id
    };
    let target = spawn(&mut world, target_at);
    let behind = spawn(
        &mut world,
        offset(centre(), -(reach + radius - epsilon()), SimScalar::ZERO),
    );
    let near_edge = target_at.x - radius;
    let beyond = spawn(
        &mut world,
        SimVec2::new(near_edge + reach + radius - epsilon(), s(CENTRE)),
    );
    assert!(
        world.entities[&beyond].position.x - radius - centre().x > reach,
        "the far body lies beyond the Baneling's reach"
    );
    let expected = sim_core::calculate_hit_damage(
        &weapon,
        &target_arch,
        target_arch.max_shields,
        target_arch.max_hp,
    );
    let (losses, _) = first_strike(&mut world, baneling, target, &[target, behind, beyond]);
    assert_eq!(
        losses[0],
        expected.life_damage + expected.shield_damage,
        "the target takes the whole strike, once"
    );
    assert!(
        losses[1] > SimScalar::ZERO,
        "behind the Baneling, inside its reach"
    );
    assert_eq!(
        losses[2],
        SimScalar::ZERO,
        "beyond the target, outside its reach"
    );
    assert!(
        !world
            .entities
            .get(&baneling)
            .is_some_and(sim_core::Entity::is_alive),
        "the Baneling is spent"
    );
}

/// The shipped mode weapon with a circular area (the sieged Siege Tank analogue's cannon), found
/// by property, as a scratch copy that fires at once from where it stands: no minimum range,
/// aimed all round, no turret delay. Its damage, area and condition are as shipped.
fn shipped_sieged_cannon(world: &SimWorld) -> WeaponSpec {
    let mut weapon = world
        .catalog
        .archetypes()
        .flat_map(|arch| &arch.modes)
        .filter_map(|mode| {
            mode.ground_weapon_override
                .as_ref()
                .and_then(Option::as_ref)
        })
        .find(|weapon| {
            weapon
                .splash
                .as_ref()
                .is_some_and(|splash| splash.shape == SplashShape::Circle)
        })
        .cloned()
        .expect("invariant: the shipped catalog has a mode weapon with a circular area");
    weapon.min_range = SimScalar::ZERO;
    weapon.firing_arc = SimAngle::from_degrees(360);
    weapon.acquire_arc = SimAngle::from_degrees(360);
    weapon.has_turret = false;
    weapon
}

/// Every shipped structure that stands on open ground (a gas extractor stands only over a
/// geyser), with its collision radius and its footprint's half height in world units.
fn open_ground_structures(world: &SimWorld) -> Vec<(UnitTypeId, SimScalar, SimScalar)> {
    world
        .catalog
        .archetypes()
        .filter(|arch| !arch.capabilities.is_gas_extractor)
        .filter_map(|arch| match arch.shape {
            ShapeKind::BuildingDome { footprint_h, .. } => Some((
                arch.unit_type_id,
                arch.collision_radius,
                SimScalar::from_num(footprint_h) * SimScalar::HALF,
            )),
            ShapeKind::UnitSphere => None,
        })
        .collect()
}

/// One sieged shot at a red `structure` (radius `radius`, half height `half`) placed at cells
/// (20, 20), with a red bystander touching its footprint's north face when the area's reach
/// could find it. Returns the `SplashVisual` count of the strike's tick and whether the
/// bystander lost life (`None` when none was placed).
fn sieged_shot_at(
    structure: UnitTypeId,
    radius: SimScalar,
    half: SimScalar,
) -> (usize, Option<bool>) {
    let mut world = world();
    let weapon = shipped_sieged_cannon(&world);
    let reach = splash_of(&weapon).reach();
    let target = world
        .spawn_structure(structure, RED, 20, 20)
        .expect("open ground");
    let at = world.entities[&target].position;
    let attacker = {
        let kind = register(&mut world, "splash_centre_cannon", s(BODY), |arch| {
            arch.weapon = Some(weapon.clone());
            arch.ground_weapon = Some(weapon.clone());
        });
        world
            .spawn_unit(
                kind,
                BLUE,
                offset(at, -(radius + s(BODY) + s(3.0)), SimScalar::ZERO),
            )
            .expect("open ground")
    };
    let gap = s(0.05);
    let bystander = (half + gap <= reach).then(|| {
        body(
            &mut world,
            RED,
            s(BODY),
            offset(at, SimScalar::ZERO, half + s(BODY) + gap),
        )
    });
    let watched: Vec<_> = std::iter::once(target).chain(bystander).collect();
    let (losses, snapshot) = first_strike(&mut world, attacker, target, &watched);
    assert!(losses[0] > SimScalar::ZERO, "the structure is struck");
    (
        splash_visuals(&snapshot),
        bystander.map(|_| losses[1] > SimScalar::ZERO),
    )
}

/// The target radius condition of the shipped sieged cannon.
fn sieged_limit(world: &SimWorld) -> SimScalar {
    splash_of(&shipped_sieged_cannon(world))
        .target_radius_below
        .expect("the sieged cannon's area is conditional (B-84)")
}

/// P-3, P-4 (B-84): a sieged shot at a shipped structure of radius 1.25 or more (the Supply Depot
/// analogue and larger) opens no area: no `SplashVisual`, and a bystander touching its footprint,
/// inside the reach of an area around its centre, is untouched.
#[test]
fn a_sieged_shot_at_a_large_structure_has_no_area() {
    let world = world();
    let limit = sieged_limit(&world);
    let large: Vec<_> = open_ground_structures(&world)
        .into_iter()
        .filter(|&(_, radius, _)| radius >= limit)
        .collect();
    assert!(
        !large.is_empty(),
        "the shipped catalog has large structures"
    );
    for (kind, radius, half) in large {
        let (visuals, bystander) = sieged_shot_at(kind, radius, half);
        assert_eq!(visuals, 0, "{kind:?} (radius {radius}): no area");
        assert_ne!(
            bystander,
            Some(true),
            "{kind:?} (radius {radius}): nobody else struck"
        );
    }
}

/// P-3, P-4 (B-84): a sieged shot at a shipped structure of radius below 1.25 opens its area:
/// one `SplashVisual`, and a bystander touching its footprint is struck.
#[test]
fn a_sieged_shot_at_a_small_structure_splashes() {
    let world = world();
    let limit = sieged_limit(&world);
    let small: Vec<_> = open_ground_structures(&world)
        .into_iter()
        .filter(|&(_, radius, _)| radius < limit)
        .collect();
    assert!(
        !small.is_empty(),
        "the shipped catalog has small structures"
    );
    for (kind, radius, half) in small {
        let (visuals, bystander) = sieged_shot_at(kind, radius, half);
        assert_eq!(visuals, 1, "{kind:?} (radius {radius}): the area opens");
        assert_eq!(
            bystander,
            Some(true),
            "{kind:?} (radius {radius}): the bystander is struck"
        );
    }
}

/// P-4: no shipped mobile ground unit has a radius of 1.25 or more, so the sieged cannon's
/// condition takes its area from no unit.
#[test]
fn no_shipped_ground_unit_is_too_large_for_the_sieged_area() {
    let world = world();
    let limit = sieged_limit(&world);
    for arch in world.catalog.archetypes() {
        if arch.mover_type == MoverType::Ground
            && !matches!(arch.shape, ShapeKind::BuildingDome { .. })
        {
            assert!(arch.collision_radius < limit, "{} is too large", arch.slug);
        }
    }
}
