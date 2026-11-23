//! # Integration Test: area damage keeps to its cliff level (plan `2026-11-23-splash-cliff-levels`)
//!
//! # Responsibility
//! The two cliff fields of `SplashSpec`, played through `SimWorld::step` on small maps with a
//! level-1 plateau beside level-0 ground, and one with a ramp between them:
//! - P-1 ([`SplashCliff::Same`]): an area strikes only units on the cliff level of a search
//!   point that reaches them, at the fraction of the nearest such point; the primary target is
//!   struck directly on any level; a ramp counts as its lower landing; an air unit has the level
//!   of the ground below it (`StarCraft` II's `SearchFlags SameCliff`, `core` EffectData:67).
//! - P-2 ([`SplashCliff::Any`]): an area strikes exactly what it strikes on a flat map.
//! - P-3 (`strike_target_on_other_level`): an attacker-centred area whose target stands on
//!   another level than the attacker strikes that target directly, once, at the first zone's
//!   fraction (the structure strike for a structure); a target on the attacker's level is struck
//!   by the area alone; with the field off the target on another level is untouched
//!   (`BanelingVolatileBurstDirectFallbackSet`, `voidmulti` EffectData:2327–2358).
//!
//! # How it fits
//! The rule lives in `combat_resolution.rs` (phase 7) and reads `GameMap::cliff_level_at`; this
//! file plays it through the public API. A scratch weapon is a copy of a shipped circular splash
//! weapon (the one with the largest reach), found by property, made instant and given plain
//! numbers so each expectation is computed from the copy's own zones. Bodies are scratch clones
//! of a shipped mobile ground (or air) unit, unarmed and unarmoured. Nothing is chosen by name
//! (charter §3.3). The fallback's friendly and attribute filters, which no order can reach (an
//! order to attack one's own unit is an attack-move), are unit tests of `combat_resolution.rs`.
//!
//! # Invariants
//! - Every tick played raises no simulation diagnostic.
//! - Boundaries are set 2⁻¹⁶ world units either side of an exact fixed-point reach.
//! - The plateau is every cell with `x >= EDGE`; everything else is level 0.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, RampDir};
use sim_types::{
    AttributeBonuses, AttributeFlags, EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind,
    SimAngle, SimScalar, SimVec2, SplashCliff, SplashImpact, SplashLine, SplashLineAxis,
    SplashLineOrigin, SplashShape, SplashSpec, SplashZone, StructureStrike, Tick, TickBundle,
    UnitOrder, UnitTypeId, WeaponSpec, WeaponVisualKind,
};

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The collision radius of every scratch body and of the scratch attacker, world units.
const BODY: f64 = 0.25;
/// The first plateau column: cells with `x >= EDGE` are level 1, the others level 0.
const EDGE: u32 = 24;
/// The row every layout is built along, world units.
const ROW: f64 = 20.0;
/// The scratch weapon's damage: no bonus, no armour, so a strike deals `fraction × DAMAGE`.
const DAMAGE: i32 = 10;
/// The scratch weapon's range when its target is not beside it, world units.
const RANGE: i32 = 8;

fn s(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(s(x), s(y))
}

/// One 2⁻¹⁶ world unit: the margin each boundary test sets either side of a reach.
fn epsilon() -> SimScalar {
    SimScalar::from_raw(1 << 16)
}

/// `from` moved by `(dx, dy)` world units.
fn offset(from: SimVec2, dx: SimScalar, dy: SimScalar) -> SimVec2 {
    SimVec2::new(from.x + dx, from.y + dy)
}

/// The world x of the plateau's edge.
fn edge() -> f64 {
    f64::from(EDGE)
}

/// What stands between the two levels.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Terrain {
    /// Every cell level 0.
    Flat,
    /// Level 1 from column `EDGE`, a sheer cliff.
    Cliff,
    /// As `Cliff`, with a ramp ascending East over columns 21 to 23 and rows 18 to 21 into the
    /// plateau; its cells store their lower landing's level, 0.
    Ramp,
}

/// A 48 × 48 world with the shipped catalogs, no random weapon delays, and `terrain`.
fn world(terrain: Terrain) -> SimWorld {
    let mut map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a valid size");
    if terrain != Terrain::Flat {
        for y in 0..48 {
            for x in EDGE..48 {
                map.cell_mut(x, y)
                    .expect("invariant: inside the map")
                    .cliff_level = CliffLevel(1);
            }
        }
    }
    if terrain == Terrain::Ramp {
        for y in 18..=21 {
            for x in 21..EDGE {
                let cell = map.cell_mut(x, y).expect("invariant: inside the map");
                cell.ramp = Some(RampDir::East);
                cell.cliff_level = CliffLevel(0);
                cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
            }
        }
    }
    let mut world = sim_fixtures::world(map, 0x5_C11F);
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
/// upgrade or structure strike, able to strike anything, not suicidal, range `RANGE`, and its
/// area centred on the target's centre, always open, enemies only, kept to its level, no
/// other-level strike.
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
    weapon.max_range = SimScalar::from_i32(RANGE);
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
    weapon.has_turret = false;
    let splash = weapon.splash.as_mut().expect("chosen for its splash");
    splash.offset_by_unit_radius = false;
    splash.friendly_fire = false;
    splash.impact = SplashImpact::Target;
    splash.target_radius_below = None;
    splash.cliff = SplashCliff::Same;
    splash.strike_target_on_other_level = false;
    weapon
}

/// The splash of `weapon`, which has one.
fn splash_of(weapon: &WeaponSpec) -> &SplashSpec {
    weapon.splash.as_ref().expect("a splash weapon")
}

/// The splash of `weapon`, mutably.
fn splash_mut(weapon: &mut WeaponSpec) -> &mut SplashSpec {
    weapon.splash.as_mut().expect("a splash weapon")
}

/// Registers a clone of the first shipped mobile unit moving as `mover` under a free id:
/// collision radius [`BODY`], 1000 life, no shields, no armour, Light only, unarmed and without
/// capabilities; `edit` changes it further.
fn register(
    world: &mut SimWorld,
    mover: MoverType,
    slug: &str,
    edit: impl FnOnce(&mut UnitArchetype),
) -> UnitTypeId {
    let mut arch = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.mover_type == mover
                && arch.max_speed > SimScalar::ZERO
                && !matches!(arch.shape, ShapeKind::BuildingDome { .. })
        })
        .expect("invariant: the shipped catalog has a mobile unit of each plane")
        .clone();
    let free = free_id(world);
    arch.unit_type_id = free;
    slug.clone_into(&mut arch.slug);
    slug.clone_into(&mut arch.name);
    arch.collision_radius = s(BODY);
    arch.terrain_radius = s(BODY);
    arch.separation_radius = s(BODY);
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

/// Spawns a scratch ground body for `owner` at `position`.
fn body(world: &mut SimWorld, owner: PlayerId, position: SimVec2) -> EntityId {
    let kind = register(world, MoverType::Ground, "splash_cliff_body", |_| {});
    world
        .spawn_unit(kind, owner, position)
        .expect("open ground")
}

/// Spawns a scratch air body for `owner` above `position`.
fn flyer(world: &mut SimWorld, owner: PlayerId, position: SimVec2) -> EntityId {
    let kind = register(world, MoverType::Air, "splash_cliff_flyer", |_| {});
    world.spawn_unit(kind, owner, position).expect("open air")
}

/// Spawns a blue scratch ground attacker at `position` carrying `weapon` against ground and
/// air.
fn gunner(world: &mut SimWorld, weapon: &WeaponSpec, position: SimVec2) -> EntityId {
    let kind = register(world, MoverType::Ground, "splash_cliff_gunner", |arch| {
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon.clone());
        arch.air_weapon = Some(weapon.clone());
    });
    world.spawn_unit(kind, BLUE, position).expect("open ground")
}

/// Hull and shield points of `id`, or 0 once it is gone.
fn vitals(world: &SimWorld, id: EntityId) -> SimScalar {
    world
        .entities
        .get(&id)
        .map_or(SimScalar::ZERO, |entity| entity.hp + entity.shields)
}

/// Plays one tick with `commands` and asserts it raised no diagnostic.
fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands,
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Orders `attacker` (blue) onto `target` and plays until any of `watched` loses vitals, at
/// most 80 ticks, then one more tick so nothing of the same strike is missed. Returns the life
/// each of `watched` lost.
fn first_strike(
    world: &mut SimWorld,
    attacker: EntityId,
    target: EntityId,
    watched: &[EntityId],
) -> Vec<SimScalar> {
    let before: Vec<_> = watched.iter().map(|&id| vitals(world, id)).collect();
    let order = PlayerCommand {
        player_id: BLUE,
        entities: vec![attacker],
        order: UnitOrder::attack(target),
        queue: false,
    };
    step_with(world, vec![order]);
    for _ in 0..80 {
        if watched
            .iter()
            .zip(&before)
            .any(|(&id, &full)| vitals(world, id) < full)
        {
            break;
        }
        step_with(world, Vec::new());
    }
    watched
        .iter()
        .zip(&before)
        .map(|(&id, &full)| full - vitals(world, id))
        .collect()
}

/// The strike a unit whose edge lies `distance` from the search centre takes from `weapon`.
fn strike_at(weapon: &WeaponSpec, distance: SimScalar) -> SimScalar {
    splash_of(weapon)
        .fraction_at(distance)
        .map_or(SimScalar::ZERO, |fraction| {
            SimScalar::from_i32(DAMAGE) * fraction
        })
}

/// The losses of one shot of `weapon` from a blue gunner on the plateau, six units east of the
/// edge, at a red target 0.6 from the edge, on the plateau when `target_high`, else below.
/// Bystanders (red): `same_in` on the target's level with its edge 2⁻¹⁶ inside the reach of
/// the target's centre (north), `same_out` 2⁻¹⁶ outside it (south), and `across_in` on the
/// other level at the same distance as `same_in`, across the edge (on `Terrain::Flat` the same
/// place, on level 0). Returns `[target, same_in, same_out, across_in]`.
fn edge_shot(terrain: Terrain, cliff: SplashCliff, target_high: bool) -> Vec<SimScalar> {
    let mut world = world(terrain);
    let mut weapon = scratch_weapon(&world);
    splash_mut(&mut weapon).cliff = cliff;
    let reach = splash_of(&weapon).reach();
    let inside = reach + s(BODY) - epsilon();
    let outside = reach + s(BODY) + epsilon();
    let (target_x, across) = if target_high {
        (edge() + 0.6, -inside)
    } else {
        (edge() - 0.6, inside)
    };
    let target_at = at(target_x, ROW);
    let across_at = offset(target_at, across, SimScalar::ZERO);
    assert_eq!(
        across_at.x < s(edge()),
        target_high,
        "the across body stands on the other side of the edge"
    );
    let attacker = gunner(&mut world, &weapon, at(edge() + 6.0, ROW));
    let target = body(&mut world, RED, target_at);
    let same_in = body(&mut world, RED, offset(target_at, SimScalar::ZERO, inside));
    let same_out = body(
        &mut world,
        RED,
        offset(target_at, SimScalar::ZERO, -outside),
    );
    let across_in = body(&mut world, RED, across_at);
    first_strike(
        &mut world,
        attacker,
        target,
        &[target, same_in, same_out, across_in],
    )
}

/// P-1: an area kept to its level strikes a unit on the target's level whose edge lies 2⁻¹⁶
/// inside its reach and not one 2⁻¹⁶ outside, and spares a unit at the same distance across the
/// cliff edge; for a target on the plateau and for one below it; the target itself takes the
/// whole direct strike.
#[test]
fn a_same_level_area_spares_units_on_another_level() {
    for target_high in [true, false] {
        let world = world(Terrain::Cliff);
        let weapon = scratch_weapon(&world);
        let reach = splash_of(&weapon).reach();
        let losses = edge_shot(Terrain::Cliff, SplashCliff::Same, target_high);
        let side = if target_high { "plateau" } else { "below" };
        assert_eq!(
            losses[0],
            SimScalar::from_i32(DAMAGE),
            "{side}: the direct strike"
        );
        assert_eq!(
            losses[1],
            strike_at(&weapon, reach - epsilon()),
            "{side}: same level, inside"
        );
        assert!(
            losses[1] > SimScalar::ZERO,
            "{side}: the reach is inclusive"
        );
        assert_eq!(losses[2], SimScalar::ZERO, "{side}: same level, outside");
        assert_eq!(
            losses[3],
            SimScalar::ZERO,
            "{side}: across the edge, inside"
        );
    }
}

/// P-2: an area reaching every level strikes on a map with a cliff exactly what it strikes on a
/// flat map: the unit across the edge too, at the same fraction.
#[test]
fn an_any_level_area_strikes_as_before() {
    for target_high in [true, false] {
        let cliff = edge_shot(Terrain::Cliff, SplashCliff::Any, target_high);
        let flat = edge_shot(Terrain::Flat, SplashCliff::Any, target_high);
        assert_eq!(cliff, flat, "target on the plateau: {target_high}");
        assert!(
            cliff[3] > SimScalar::ZERO,
            "the unit across the edge is struck"
        );
        assert_eq!(
            cliff[3], cliff[1],
            "at the same fraction as on its own side"
        );
        let same_on_flat = edge_shot(Terrain::Flat, SplashCliff::Same, target_high);
        assert_eq!(same_on_flat, flat, "on a flat map the two rules agree");
    }
}

/// P-1 with D-2: a ramp counts as its lower landing. A shot at a target on the plateau at the
/// ramp's top strikes a unit on the plateau beside it and spares one on the ramp's top cell at
/// the same distance; reaching every level, it strikes both alike (the control).
#[test]
fn a_same_level_area_counts_a_ramp_as_its_lower_landing() {
    let probe = world(Terrain::Ramp);
    assert_eq!(probe.map.cliff_level_at(at(23.5, 19.5)), CliffLevel(0));
    assert!(
        probe.map.ramp_profile(23, 19).is_some(),
        "the ramp's top cell"
    );
    for cliff in [SplashCliff::Same, SplashCliff::Any] {
        let mut world = world(Terrain::Ramp);
        let mut weapon = scratch_weapon(&world);
        splash_mut(&mut weapon).cliff = cliff;
        let target_at = at(edge() + 0.5, 19.5);
        let attacker = gunner(&mut world, &weapon, at(edge() + 6.0, 19.5));
        let target = body(&mut world, RED, target_at);
        let on_ramp = body(&mut world, RED, at(edge() - 0.5, 19.5));
        let beside = body(&mut world, RED, at(edge() + 0.5, 20.5));
        let distance = s(1.0) - s(BODY);
        assert!(distance <= splash_of(&weapon).reach(), "both in reach");
        let losses = first_strike(&mut world, attacker, target, &[target, on_ramp, beside]);
        assert_eq!(losses[2], strike_at(&weapon, distance), "{cliff:?}: beside");
        let ramp = if cliff == SplashCliff::Same {
            SimScalar::ZERO
        } else {
            strike_at(&weapon, distance)
        };
        assert_eq!(losses[1], ramp, "{cliff:?}: on the ramp's top cell");
    }
}

/// P-1: the reference is the search centre, not the attacker. A gunner below the plateau (a
/// blue body on the plateau gives it sight) strikes its target on the plateau directly, the
/// whole strike as on a flat map; its area strikes a unit on the plateau and spares one below,
/// on the gunner's own level.
#[test]
fn the_target_is_struck_on_any_level() {
    let mut world = world(Terrain::Cliff);
    let weapon = scratch_weapon(&world);
    let reach = splash_of(&weapon).reach();
    let inside = reach + s(BODY) - epsilon();
    let target_at = at(edge() + 0.6, ROW);
    let attacker = gunner(&mut world, &weapon, at(edge() - 5.0, ROW));
    let _spotter = body(&mut world, BLUE, at(edge() + 4.0, ROW + 4.0));
    let target = body(&mut world, RED, target_at);
    let high = body(&mut world, RED, offset(target_at, SimScalar::ZERO, inside));
    let low = body(&mut world, RED, offset(target_at, -inside, SimScalar::ZERO));
    assert!(world.entities[&low].position.x < s(edge()), "below");
    let losses = first_strike(&mut world, attacker, target, &[target, high, low]);
    assert_eq!(losses[0], SimScalar::from_i32(DAMAGE), "the direct strike");
    assert_eq!(losses[1], strike_at(&weapon, reach - epsilon()), "plateau");
    assert_eq!(losses[2], SimScalar::ZERO, "below, the attacker's level");
}

/// D-1 per point: a line kept to its level compares each search with its own point. A gunner on
/// the plateau fires a two-point line west over the edge, one point on the plateau and one
/// below (inner zone 1.5 at the whole strike, outer 2.5 at half). A plateau unit reached only by
/// the low point is spared; a low unit is struck by the low point; a plateau unit in the low
/// point's inner zone and the plateau point's outer zone takes the outer fraction. Reaching
/// every level, the line strikes both plateau units at the inner fraction (the control).
#[test]
fn a_same_level_line_keeps_each_search_to_its_point() {
    for cliff in [SplashCliff::Same, SplashCliff::Any] {
        let mut world = world(Terrain::Cliff);
        let mut weapon = scratch_weapon(&world);
        let splash = splash_mut(&mut weapon);
        splash.cliff = cliff;
        splash.zones = vec![
            SplashZone {
                radius: s(1.5),
                fraction: SimScalar::ONE,
            },
            SplashZone {
                radius: s(2.5),
                fraction: SimScalar::HALF,
            },
        ];
        splash.shape = SplashShape::Line(SplashLine {
            origin: SplashLineOrigin::Attacker,
            axis: SplashLineAxis::Along,
            offsets: vec![s(0.5), s(4.5)],
            period_ticks: 0,
        });
        // Points at x = 27.5 (plateau) and 23.5 (below).
        let attacker = gunner(&mut world, &weapon, at(28.0, ROW));
        let target = body(&mut world, RED, at(21.0, ROW));
        let high_near_low_point = body(&mut world, RED, at(24.3, ROW));
        let low = body(&mut world, RED, at(22.5, ROW + 1.0));
        let high_between = body(&mut world, RED, at(25.0, ROW));
        let losses = first_strike(
            &mut world,
            attacker,
            target,
            &[target, high_near_low_point, low, high_between],
        );
        let whole = SimScalar::from_i32(DAMAGE);
        let half = whole * SimScalar::HALF;
        assert_eq!(losses[0], whole, "{cliff:?}: the direct strike");
        assert_eq!(losses[2], whole, "{cliff:?}: below, by the low point");
        if cliff == SplashCliff::Same {
            assert_eq!(losses[1], SimScalar::ZERO, "reached by the low point only");
            assert_eq!(losses[3], half, "the plateau point's outer zone");
        } else {
            assert_eq!(losses[1], whole, "control: the low point's inner zone");
            assert_eq!(losses[3], whole, "control: the low point's inner zone");
        }
    }
}

/// D-2's default: an air area keeps to the level of the ground below its centre. A shot at an
/// air target over the plateau strikes an air unit over the plateau and spares one over the
/// low ground at the same distance.
#[test]
fn an_air_area_keeps_to_the_level_below() {
    let mut world = world(Terrain::Cliff);
    let weapon = scratch_weapon(&world);
    let reach = splash_of(&weapon).reach();
    let inside = reach + s(BODY) - epsilon();
    let target_at = at(edge() + 0.6, ROW);
    let attacker = gunner(&mut world, &weapon, at(edge() + 6.0, ROW));
    let target = flyer(&mut world, RED, target_at);
    let over_plateau = flyer(&mut world, RED, offset(target_at, SimScalar::ZERO, inside));
    let over_low = flyer(&mut world, RED, offset(target_at, -inside, SimScalar::ZERO));
    let losses = first_strike(
        &mut world,
        attacker,
        target,
        &[target, over_plateau, over_low],
    );
    assert_eq!(losses[0], SimScalar::from_i32(DAMAGE), "the direct strike");
    assert_eq!(
        losses[1],
        strike_at(&weapon, reach - epsilon()),
        "over the plateau"
    );
    assert_eq!(losses[2], SimScalar::ZERO, "over the low ground");
}

/// The scratch weapon made attacker-centred (a burst), with its range its reach and the
/// other-level strike as `fallback` says.
fn burst_weapon(world: &SimWorld, fallback: bool) -> WeaponSpec {
    let mut weapon = scratch_weapon(world);
    weapon.max_range = splash_of(&weapon).reach();
    let splash = splash_mut(&mut weapon);
    splash.impact = SplashImpact::Attacker;
    splash.strike_target_on_other_level = fallback;
    weapon
}

/// One burst from a blue gunner on the plateau 0.4 from the edge at a red target 1.0 west of
/// it, below; a red unit below 1.0 north of the target, and a red unit on the plateau 1.2
/// north of the gunner. Returns `[target, below, plateau]` and the weapon.
fn burst_across(fallback: bool) -> (Vec<SimScalar>, WeaponSpec) {
    let mut world = world(Terrain::Cliff);
    let weapon = burst_weapon(&world, fallback);
    let origin = at(edge() + 0.4, ROW);
    let attacker = gunner(&mut world, &weapon, origin);
    let target = body(&mut world, RED, at(edge() - 0.6, ROW));
    let below = body(&mut world, RED, at(edge() - 0.6, ROW + 1.0));
    let plateau = body(&mut world, RED, at(edge() + 0.4, ROW + 1.2));
    let reach = splash_of(&weapon).reach();
    assert!(
        s(2.0_f64.sqrt()) - s(BODY) <= reach,
        "the unit below is in the burst's reach"
    );
    let losses = first_strike(&mut world, attacker, target, &[target, below, plateau]);
    (losses, weapon)
}

/// P-3: a burst whose target stands on another level strikes the target directly, once, at the
/// first zone's fraction (the strike at distance zero); a unit on the target's level is
/// untouched; a unit on the attacker's level is struck by the area. With the other-level strike
/// off, the target is untouched.
#[test]
fn an_attacker_centred_area_strikes_a_target_on_another_level_directly() {
    let (losses, weapon) = burst_across(true);
    assert_eq!(
        losses[0],
        strike_at(&weapon, SimScalar::ZERO),
        "the target, once"
    );
    assert!(losses[0] > SimScalar::ZERO);
    assert_eq!(losses[1], SimScalar::ZERO, "below, beside the target");
    assert_eq!(
        losses[2],
        strike_at(&weapon, s(1.2) - s(BODY)),
        "the plateau, by the area"
    );

    let (losses, weapon) = burst_across(false);
    assert_eq!(losses[0], SimScalar::ZERO, "without the other-level strike");
    assert_eq!(losses[1], SimScalar::ZERO);
    assert_eq!(losses[2], strike_at(&weapon, s(1.2) - s(BODY)));
}

/// P-3: a burst whose target stands on its own level strikes it by the area alone, once, at the
/// fraction of its distance, with the other-level strike on; a unit below in reach is untouched.
#[test]
fn an_attacker_centred_area_strikes_a_target_on_its_own_level_once() {
    let mut world = world(Terrain::Cliff);
    let weapon = burst_weapon(&world, true);
    let origin = at(edge() + 0.6, ROW);
    let attacker = gunner(&mut world, &weapon, origin);
    let target = body(&mut world, RED, at(edge() + 1.6, ROW));
    let below = body(&mut world, RED, at(edge() - 0.4, ROW));
    let losses = first_strike(&mut world, attacker, target, &[target, below]);
    assert_eq!(
        losses[0],
        strike_at(&weapon, s(1.0) - s(BODY)),
        "the area alone"
    );
    assert_eq!(losses[1], SimScalar::ZERO, "below");
}

/// The smallest shipped structure that stands on open ground (a gas extractor stands only over
/// a geyser), with its footprint width in cells.
fn small_structure(world: &SimWorld) -> (UnitTypeId, u32) {
    world
        .catalog
        .archetypes()
        .filter(|arch| !arch.capabilities.is_gas_extractor)
        .filter_map(|arch| match arch.shape {
            ShapeKind::BuildingDome { footprint_w, .. } => Some((
                arch.collision_radius,
                arch.unit_type_id,
                u32::from(footprint_w),
            )),
            ShapeKind::UnitSphere => None,
        })
        .min_by_key(|&(radius, id, _)| (radius, id))
        .map(|(_, id, width)| (id, width))
        .expect("invariant: the shipped catalog has structures")
}

/// P-3 with a structure: a burst at a structure below the plateau strikes it directly with the
/// structure strike at the first zone's fraction; with the other-level strike off, the structure
/// is untouched (the area keeps to the plateau).
#[test]
fn an_attacker_centred_area_strikes_a_structure_on_another_level_with_its_structure_strike() {
    for fallback in [true, false] {
        let mut world = world(Terrain::Cliff);
        let mut weapon = burst_weapon(&world, fallback);
        let strike = SimScalar::from_i32(7);
        weapon.structure_strike = Some(StructureStrike {
            damage: strike,
            upgrade_per_level: SimScalar::ZERO,
            ignores_armor: true,
        });
        let (kind, width) = small_structure(&world);
        let target = world
            .spawn_structure(kind, RED, EDGE - width, 18)
            .expect("open ground below the plateau");
        let target_y = world.entities[&target].position.y;
        let attacker = gunner(&mut world, &weapon, SimVec2::new(s(edge() + 0.4), target_y));
        let losses = first_strike(&mut world, attacker, target, &[target]);
        let expected = if fallback {
            strike * splash_of(&weapon).zones[0].fraction
        } else {
            SimScalar::ZERO
        };
        assert_eq!(
            losses[0], expected,
            "the structure strike, once: {fallback}"
        );
    }
}
