//! # Integration Test: ranges to a structure measure to its engine radius (B-135)
//!
//! # Responsibility
//! Plan `2026-11-18-structure-radius`: a structure's `collision_radius` is its authored
//! `radius`, the analogue's 5.0.14 engine radius, and every range reads it, while its terrain
//! radius is half its footprint's smaller side and its dome is drawn at its footprint:
//! - R-1, R-2: a weapon reaches, and a scan acquires, a structure at its radius;
//! - R-3: an armed structure reaches at its own radius;
//! - R-4: a melee unit touching any part of a face within the radius's reach swings from where
//!   it stands; R-5 (data): every shipped structure face is in every shipped melee unit's reach;
//! - R-6: a projectile's published and simulated flights end at the radius;
//! - R-7: a splash zone reaches a structure's edge at its radius;
//! - R-8: a passenger boards a carrier structure at its radius;
//! - R-9: the Charge window measures the gap to the radius;
//! - R-12: a 5×5 whose radius exceeds its half-width keeps a valid motion entry when it stands
//!   flush with the map's corner (D-5).
//!
//! R-1 to R-9 are guards: the readers already read the structure's collision radius, so these
//! pin the wiring, each against a control at half the footprint. The cast and missile readers
//! (R-10, R-11) and the presentation split (R-13 to R-16) are unit tests in `world/casts.rs`,
//! `effects/step.rs`, `world/snapshot.rs`, `combat_resolution.rs`, `transport/lifecycle.rs` and
//! `catalog.rs`.
//!
//! # How it fits
//! Plays `SimWorld` on small synthetic maps with the shipped catalogs. A structure under test is
//! a scratch copy of a shipped structure, chosen by footprint size, loaded through the catalog's
//! own RON path (`UnitCatalog::load_structures_from_ron_str`) with its `radius` replaced, so it
//! passes the loader exactly as a shipped row does. Mobile bodies are scratch clones of a shipped
//! mobile unit, as in `structure_contact_tests`. Nothing is chosen by name (charter §3.3).
//!
//! # Invariants
//! - A structure is found by `ShapeKind::BuildingDome` and its footprint size only.
//! - Every tick of `step` ends with no simulation diagnostic.
//! - Boundaries are set 2⁻¹⁶ world units either side of an exact fixed-point reach.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, RenderSnapshot, ShapeKind, SimScalar, SimVec2,
    Tick, TickBundle, UnitOrder, UnitTypeId,
};

fn s(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(s(x), s(y))
}

/// A flat level-1 map of `size` × `size`.
fn flat(size: u32) -> GameMap {
    GameMap::new(size, size, MapCell::default_ground(CliffLevel(1)))
        .expect("invariant: a valid size")
}

/// A world on `map` with the shipped catalogs.
fn world_on(map: GameMap) -> SimWorld {
    sim_fixtures::world(map, 0x0B_135)
}

/// A unit type id no archetype of `world`'s catalog uses.
fn free_id(world: &SimWorld) -> UnitTypeId {
    (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("invariant: a free unit type id")
}

/// The shipped `structures.ron` split into its structure blocks, each the lines from its opening
/// `(` to its closing `),` at the list's indentation.
fn structure_blocks() -> Vec<Vec<&'static str>> {
    let mut blocks = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in sim_fixtures::STRUCTURES_RON.lines() {
        if line == "        (" {
            current = Some(vec![line]);
        } else if let Some(block) = current.as_mut() {
            block.push(line);
            if line == "        )," {
                blocks.extend(current.take());
            }
        }
    }
    blocks
}

/// Loads a scratch copy of the first shipped structure whose footprint is `size` × `size` and
/// for which `pick` holds, under a free id and the slug `slug`, with its `radius` set to `radius`
/// (world units, as authored), through the catalog's RON loader. Returns its unit type id.
fn scratch_structure(
    world: &mut SimWorld,
    size: u32,
    radius: f64,
    slug: &str,
    pick: impl Fn(&UnitArchetype) -> bool,
) -> UnitTypeId {
    let footprint = format!("footprint: ({size}, {size}),");
    let block = structure_blocks()
        .into_iter()
        .find(|block| {
            block.iter().any(|line| line.trim() == footprint)
                && block
                    .iter()
                    .find_map(|line| line.trim().strip_prefix("slug: "))
                    .and_then(|slug| {
                        world
                            .catalog
                            .get_by_slug(slug.trim_end_matches(',').trim_matches('"'))
                    })
                    .is_some_and(&pick)
        })
        .expect("invariant: the shipped catalog has such a structure");
    let id = free_id(world);
    let edited: Vec<String> = block
        .iter()
        .map(|line| {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            if trimmed.starts_with("structure_type_id:") {
                format!("{indent}structure_type_id: {},", id.0)
            } else if trimmed.starts_with("slug:") {
                format!("{indent}slug: \"{slug}\",")
            } else if trimmed.starts_with("name:") {
                format!("{indent}name: \"{slug}\",")
            } else if trimmed.starts_with("radius:") {
                format!("{indent}radius: {radius},")
            } else {
                (*line).to_owned()
            }
        })
        .collect();
    let text = format!("(structures: [\n{}\n])", edited.join("\n"));
    world
        .catalog
        .load_structures_from_ron_str(&text)
        .expect("the scratch structure loads");
    id
}

/// Registers a clone of the first shipped mobile ground unit under a free id: unarmed, with
/// collision radius `collision` and terrain radius `terrain` (world units), speed 0.2 per tick
/// reached at once, no turning delay, no capability; `edit` changes it further.
fn register(
    world: &mut SimWorld,
    slug: &str,
    collision: f64,
    terrain: f64,
    edit: impl FnOnce(&mut UnitArchetype),
) -> UnitTypeId {
    let base = world
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
    let mut arch = base;
    arch.unit_type_id = free;
    slug.clone_into(&mut arch.slug);
    slug.clone_into(&mut arch.name);
    arch.collision_radius = s(collision);
    arch.terrain_radius = s(terrain);
    arch.max_speed = s(0.2);
    arch.acceleration = s(10.0);
    arch.deceleration = s(10.0);
    arch.turn_rate = sim_types::SimAngle::from_degrees(180);
    arch.stationary_turn_rate = sim_types::SimAngle::from_degrees(180);
    arch.turn_before_move = false;
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    arch.capabilities.overclock = None;
    arch.capabilities.barrier = None;
    arch.capabilities.attack_dash = None;
    edit(&mut arch);
    world.catalog.register(arch);
    free
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

fn step(world: &mut SimWorld) -> RenderSnapshot {
    step_with(world, Vec::new())
}

fn move_to(owner: u8, id: EntityId, target: SimVec2) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(owner),
        entities: vec![id],
        order: UnitOrder::Move { target },
        queue: false,
    }
}

fn unarmed(archetype: &UnitArchetype) -> bool {
    archetype.weapon.is_none()
        && archetype.ground_weapon.is_none()
        && archetype.air_weapon.is_none()
}

/// R-12: an unarmed 5×5 with radius 2.75, a quarter more than its half-width, registered at
/// cells (0, 0) so its centre is 2.5 from two map edges, and a unit ordered past it, are stepped
/// 25 ticks with no diagnostic: the per-tick motion-entry domain check of the foundation reads
/// its terrain radius, half its footprint (D-5), not its radius.
#[test]
fn a_structure_flush_with_the_map_edge_keeps_a_valid_motion_entry() {
    let mut world = world_on(flat(32));
    let kind = scratch_structure(&mut world, 5, 2.75, "structure_radius_flush", unarmed);
    let arch = world.catalog.get(kind).expect("loaded");
    assert_eq!(arch.collision_radius, s(2.75));
    assert_eq!(arch.terrain_radius, s(2.5));
    world
        .spawn_structure(kind, PlayerId(0), 0, 0)
        .expect("open ground at the corner");
    let walker = register(&mut world, "structure_radius_walker", 0.5, 0.375, |_| {});
    let body = world
        .spawn_unit(walker, PlayerId(0), at(9.0, 2.0))
        .expect("spawn");
    step_with(&mut world, vec![move_to(0, body, at(2.0, 9.0))]);
    for _ in 0..24 {
        step(&mut world);
    }
    assert!(
        world.entities[&body].position != at(9.0, 2.0),
        "the unit walked"
    );
}

/// One 2⁻¹⁶ world unit: the margin each boundary test sets either side of a reach.
fn epsilon() -> SimScalar {
    SimScalar::from_raw(1 << 16)
}

/// The minimum cell of the 3×3 structure of R-1, R-2 and R-4 to R-9: it covers x and y in
/// [18, 21], so its centre is (19.5, 19.5) and its west face is x = 18.
const CELL: u32 = 18;
/// The centre of the 3×3 structure at [`CELL`], world units.
const CENTRE: f64 = 19.5;
/// The radius of a 3×3 contour structure in 5.0.14 (`Footprint3x3Contour` `Shape` 1.803, rounded
/// up to 1/16).
const R_3X3: f64 = 1.8125;
/// Half a 3×3 footprint: the radius before this plan, and the control.
const H_3X3: f64 = 1.5;

/// A world on a flat 48 × 48 map with no random weapon delays, and a scratch unarmed 3×3
/// structure of player 1 with radius `radius` at [`CELL`].
fn with_structure(radius: f64) -> (SimWorld, EntityId) {
    let mut world = world_on(flat(48));
    sim_fixtures::without_random_delays(&mut world.catalog);
    let kind = scratch_structure(&mut world, 3, radius, "structure_radius_target", unarmed);
    let placed = world
        .spawn_structure(kind, PlayerId(1), CELL, CELL)
        .expect("open ground");
    (world, placed)
}

/// Hull and shield points of `id`, or 0 once it is gone.
fn vitals(world: &SimWorld, id: EntityId) -> SimScalar {
    world
        .entities
        .get(&id)
        .map_or(SimScalar::ZERO, |entity| entity.hp + entity.shields)
}

/// The first shipped mobile ground unit armed against ground with a ranged weapon (range at
/// least 2, no minimum range) that `pick` admits: its type id, collision radius, weapon range
/// and weapon period.
fn ranged_unit(
    world: &SimWorld,
    pick: impl Fn(&sim_types::WeaponSpec) -> bool,
) -> (UnitTypeId, SimScalar, SimScalar, u16) {
    world
        .catalog
        .archetypes()
        .find_map(|arch| {
            if !matches!(arch.shape, ShapeKind::UnitSphere)
                || arch.mover_type != MoverType::Ground
                || arch.max_speed <= SimScalar::ZERO
            {
                return None;
            }
            let weapon = arch.weapon_against(MoverType::Ground, None)?;
            (weapon.max_range >= s(2.0) && weapon.min_range == SimScalar::ZERO && pick(weapon))
                .then_some((
                    arch.unit_type_id,
                    arch.collision_radius,
                    weapon.max_range,
                    weapon.period_ticks,
                ))
        })
        .expect("invariant: the shipped catalog has a ranged ground unit")
}

fn attack(owner: u8, id: EntityId, target: EntityId) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(owner),
        entities: vec![id],
        order: UnitOrder::attack(target),
        queue: false,
    }
}

/// How an attack order on a structure went from a standing start.
#[derive(Debug, PartialEq, Eq)]
enum Opening {
    /// The weapon started a swing on the given tick without the attacker having moved.
    SwungFromWhereItStood(u32),
    /// The attacker moved before any swing.
    MovedFirst,
    /// Neither within the ticks played.
    Neither,
}

/// Places a fresh attacker of type `kind` for player 0 at `start`, orders it to attack the
/// structure and plays up to `ticks` ticks.
fn opening(
    world: &mut SimWorld,
    structure: EntityId,
    kind: UnitTypeId,
    start: SimVec2,
    ticks: u32,
) -> Opening {
    let attacker = world
        .spawn_unit(kind, PlayerId(0), start)
        .expect("open ground");
    step_with(world, vec![attack(0, attacker, structure)]);
    for tick in 1..=ticks {
        let entity = &world.entities[&attacker];
        if entity.position != start {
            return Opening::MovedFirst;
        }
        if entity.engaged_target == Some(structure) {
            return Opening::SwungFromWhereItStood(tick);
        }
        step(world);
    }
    Opening::Neither
}

/// [`opening`] for the first ranged unit, `distance` west of the structure's centre.
fn ranged_opening(
    world: &mut SimWorld,
    structure: EntityId,
    distance: SimScalar,
    ticks: u32,
) -> Opening {
    let (kind, ..) = ranged_unit(world, |_| true);
    let start = SimVec2::new(s(CENTRE) - distance, s(CENTRE));
    opening(world, structure, kind, start, ticks)
}

/// R-1 (guard): a ranged ground unit ordered to attack a 3×3 of radius 1.8125 starts a swing
/// within its weapon's period without moving from `range + r + R − 2⁻¹⁶`, and moves first from
/// `+ 2⁻¹⁶`. With radius 1.5 the first distance is out of range too (A1, A2).
#[test]
fn a_weapon_reaches_a_structure_at_its_radius() {
    let probe = world_on(flat(8));
    let (_, r, range, period) = ranged_unit(&probe, |_| true);
    let reach = range + r + s(R_3X3);
    let ticks = u32::from(period);

    let (mut world, structure) = with_structure(R_3X3);
    let inside = ranged_opening(&mut world, structure, reach - epsilon(), ticks);
    assert!(
        matches!(inside, Opening::SwungFromWhereItStood(tick) if tick <= ticks),
        "{inside:?}"
    );
    let (mut world, structure) = with_structure(R_3X3);
    assert_eq!(
        ranged_opening(&mut world, structure, reach + epsilon(), ticks),
        Opening::MovedFirst
    );
    let (mut world, structure) = with_structure(H_3X3);
    assert_eq!(
        ranged_opening(&mut world, structure, reach - epsilon(), ticks),
        Opening::MovedFirst
    );
}

/// Places an idle ranged unit of player 0 `distance` west of the structure's centre and plays
/// `ticks` ticks; whether it took the structure as its target.
fn acquires(world: &mut SimWorld, structure: EntityId, distance: SimScalar, ticks: u32) -> bool {
    let (kind, ..) = ranged_unit(world, |_| true);
    let start = SimVec2::new(s(CENTRE) - distance, s(CENTRE));
    let unit = world
        .spawn_unit(kind, PlayerId(0), start)
        .expect("open ground");
    for _ in 0..ticks {
        step(world);
        if world.entities[&unit].current_target == Some(structure) {
            return true;
        }
    }
    false
}

/// R-2 (guard): an idle armed unit, with no other enemy about, acquires a 3×3 of radius 1.8125
/// at `scan + r + R − 2⁻¹⁶` and not at `+ 2⁻¹⁶`; with radius 1.5 not at the first distance
/// (A1's scan qualification and the catalog's largest-radius query bound).
#[test]
fn a_structure_is_scanned_at_its_radius() {
    let probe = world_on(flat(8));
    let (kind, r, range, _) = ranged_unit(&probe, |_| true);
    let scan = probe
        .catalog
        .get(kind)
        .expect("listed")
        .scan_range
        .max(range);
    let reach = scan + r + s(R_3X3);

    let (mut world, structure) = with_structure(R_3X3);
    assert!(acquires(&mut world, structure, reach - epsilon(), 25));
    let (mut world, structure) = with_structure(R_3X3);
    assert!(!acquires(&mut world, structure, reach + epsilon(), 50));
    let (mut world, structure) = with_structure(H_3X3);
    assert!(!acquires(&mut world, structure, reach - epsilon(), 50));
}

fn armed_against_ground(archetype: &UnitArchetype) -> bool {
    archetype
        .weapon_against(MoverType::Ground, None)
        .is_some_and(|weapon| weapon.min_range == SimScalar::ZERO)
}

/// Places a scratch armed 2×2 structure of player 1 with radius `radius` at cells (18, 18)
/// (centre (19, 19)) and an unarmed scratch body of player 0 `distance` west of its centre;
/// plays `ticks` ticks and returns whether the structure started a swing at the body.
fn structure_fires(radius: f64, distance: impl Fn(SimScalar, SimScalar) -> SimScalar) -> bool {
    let mut world = world_on(flat(48));
    sim_fixtures::without_random_delays(&mut world.catalog);
    let kind = scratch_structure(
        &mut world,
        2,
        radius,
        "structure_radius_tower",
        armed_against_ground,
    );
    let range = world
        .catalog
        .get(kind)
        .expect("loaded")
        .weapon_against(MoverType::Ground, None)
        .expect("armed")
        .max_range;
    let tower = world
        .spawn_structure(kind, PlayerId(1), 18, 18)
        .expect("open ground");
    let body = register(&mut world, "structure_radius_body", 0.5, 0.375, |_| {});
    let at = SimVec2::new(s(19.0) - distance(range, s(0.5)), s(19.0));
    let unit = world
        .spawn_unit(body, PlayerId(0), at)
        .expect("open ground");
    for _ in 0..60 {
        step(&mut world);
        if world.entities[&tower].engaged_target == Some(unit) {
            return true;
        }
    }
    false
}

/// R-3 (guard): a 2×2 armed structure of radius 1.125 fires at a unit at `range + 1.125 + r −
/// 2⁻¹⁶` and not at `+ 2⁻¹⁶`; with radius 1 not at the first distance (A1, A2 from a structure).
#[test]
fn an_armed_structure_reaches_at_its_own_radius() {
    let reach = |range: SimScalar, r: SimScalar| range + s(1.125) + r;
    assert!(structure_fires(1.125, |range, r| reach(range, r) - epsilon()));
    assert!(!structure_fires(1.125, |range, r| reach(range, r) + epsilon()));
    assert!(!structure_fires(1.0, |range, r| reach(range, r) - epsilon()));
}

/// The shortest-reaching shipped melee weapon against ground (the first of them), cloned.
fn melee_weapon(world: &SimWorld) -> sim_types::WeaponSpec {
    world
        .catalog
        .archetypes()
        .filter(|arch| matches!(arch.shape, ShapeKind::UnitSphere))
        .filter_map(|arch| {
            arch.weapon_against(MoverType::Ground, None)
                .filter(|weapon| weapon.melee)
        })
        .min_by_key(|weapon| weapon.max_range)
        .cloned()
        .expect("invariant: the shipped catalog has a melee unit")
}

/// The melee body of R-4: collision radius 0.5, terrain radius 0.375 (below it, as the Zealot
/// analogue's), armed with the shortest-reaching shipped melee weapon (range 0.1).
fn melee_body(world: &mut SimWorld) -> (UnitTypeId, sim_types::WeaponSpec) {
    let weapon = melee_weapon(world);
    let armed = weapon.clone();
    let kind = register(world, "structure_radius_melee", 0.5, 0.375, |arch| {
        arch.weapon = Some(armed.clone());
        arch.ground_weapon = Some(armed);
    });
    (kind, weapon)
}

/// R-4 (guard): a melee body whose terrain radius (0.375) is below its radius (0.5), standing
/// against the west face of a 3×3 of radius 1.8125 (its centre at its terrain radius from the
/// face) at lateral offsets 0, ±1.0 and ±1.4 from the face's middle, swings from where it stands
/// when ordered to attack, and deals damage within 100 ticks. With radius 1.5 its reach at the
/// face ends 0.95 from the middle, so from ±1.0 and ±1.4 it must move first (the pursuit band,
/// A10), while from the middle it still swings where it stands.
#[test]
fn a_melee_attacker_strikes_a_structure_along_its_face() {
    let face = f64::from(CELL) - 0.375;
    for offset in [0.0, 1.0, -1.0, 1.4, -1.4] {
        let (mut world, structure) = with_structure(R_3X3);
        let (kind, weapon) = melee_body(&mut world);
        let full = vitals(&world, structure);
        let start = at(face, CENTRE + offset);
        let swing = opening(
            &mut world,
            structure,
            kind,
            start,
            u32::from(weapon.period_ticks),
        );
        assert!(
            matches!(swing, Opening::SwungFromWhereItStood(_)),
            "offset {offset}: {swing:?}"
        );
        for _ in 0..100 {
            step(&mut world);
        }
        assert!(vitals(&world, structure) < full, "offset {offset}");

        let (mut world, structure) = with_structure(H_3X3);
        let (kind, weapon) = melee_body(&mut world);
        let start = at(face, CENTRE + offset);
        let swing = opening(
            &mut world,
            structure,
            kind,
            start,
            u32::from(weapon.period_ticks),
        );
        if offset == 0.0 {
            assert!(
                matches!(swing, Opening::SwungFromWhereItStood(_)),
                "{swing:?}"
            );
        } else {
            assert_eq!(swing, Opening::MovedFirst, "offset {offset} at radius 1.5");
        }
    }
}

/// R-5 (guard, data): for every shipped structure and every shipped ground melee weapon, a unit
/// touching the middle of a face (centre `min(w, h) ÷ 2 + t` from the structure's centre) is in
/// reach (`R + r + range`): the floor of D-4 on the shipped data, so no structure is unhittable
/// by melee.
#[test]
fn every_shipped_structure_face_is_in_melee_reach() {
    let world = world_on(flat(8));
    let mut pairs = 0;
    for structure in world.catalog.archetypes() {
        let ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
            ..
        } = structure.shape
        else {
            continue;
        };
        let half = s(f64::from(footprint_w.min(footprint_h)) * 0.5);
        for unit in world.catalog.archetypes() {
            let Some(weapon) = unit
                .weapon_against(MoverType::Ground, None)
                .filter(|weapon| weapon.melee)
            else {
                continue;
            };
            if unit.mover_type != MoverType::Ground {
                continue;
            }
            assert!(
                half + unit.terrain_radius
                    <= structure.collision_radius + unit.collision_radius + weapon.max_range,
                "{} against {}",
                unit.slug,
                structure.slug
            );
            pairs += 1;
        }
    }
    assert!(
        pairs > 0,
        "the shipped catalog has structures and melee units"
    );
}

/// The first projectile weapon of a shipped ranged ground unit against ground: its unit, radius,
/// range and projectile speed.
fn projectile_unit(world: &SimWorld) -> (UnitTypeId, SimScalar, SimScalar, SimScalar) {
    let (kind, r, range, _) = ranged_unit(world, |weapon| weapon.projectile_speed.is_some());
    let speed = world
        .catalog
        .get(kind)
        .expect("listed")
        .weapon_against(MoverType::Ground, None)
        .and_then(|weapon| weapon.projectile_speed)
        .expect("chosen for its projectile");
    (kind, r, range, speed)
}

/// Fires one projectile from a shooter `range + r + 1.5 − 2⁻¹⁶` west of the centre of a 3×3 of
/// radius `radius`, ordered to attack it. Returns the ticks its visual is published to fly
/// (`duration_secs · 25`, read from the render snapshot) and the number of steps at whose end the
/// simulated projectile is in `SimWorld::projectiles`.
fn projectile_flight(radius: f64) -> (u32, u32) {
    let (mut world, structure) = with_structure(radius);
    let (kind, r, range, _) = projectile_unit(&world);
    let distance = range + r + s(H_3X3) - epsilon();
    let start = SimVec2::new(s(CENTRE) - distance, s(CENTRE));
    let shooter = world
        .spawn_unit(kind, PlayerId(0), start)
        .expect("open ground");
    let mut snapshot = step_with(&mut world, vec![attack(0, shooter, structure)]);
    let (mut published, mut alive) = (None, 0_u32);
    for _ in 0..200 {
        if published.is_none() {
            published = snapshot.combat_events.iter().find_map(|event| match event {
                sim_types::CombatEventVisual::ProjectileVisual {
                    duration_secs: Some(duration),
                    ..
                } => Some(*duration),
                _ => None,
            });
        }
        if world.projectiles.is_empty() {
            if alive > 0 {
                break;
            }
        } else {
            alive += 1;
        }
        snapshot = step(&mut world);
    }
    let duration = published.expect("a projectile was launched");
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a flight of a few dozen ticks, recovered from its published seconds"
    )]
    let published = (duration * f32::from(sim_types::TICKS_PER_SECOND)).round() as u32;
    (published, alive)
}

/// R-6 (guard): a projectile fired at a 3×3 flies to its radius. Its published flight is
/// `⌈max(d − R − r, 0.05) ÷ speed⌉` ticks (A3, which drives only the visual's duration), and the
/// simulated projectile, advancing its speed a step from the shooter's centre, lands on the
/// `⌈(d − R) ÷ speed⌉`-th step counting the launch step, once within its speed of the radius
/// (A6). Both are shorter at radius 1.8125 than at 1.5.
#[test]
fn a_projectile_to_a_structure_flies_to_its_radius() {
    let probe = world_on(flat(8));
    let (_, r, range, speed) = projectile_unit(&probe);
    let distance = range + r + s(H_3X3) - epsilon();
    let expected = |radius: f64| {
        let published = ((distance - s(radius) - r).max(s(0.05)) / speed)
            .saturating_ceil()
            .saturating_to_u32()
            .max(1);
        // The launch step also advances the projectile once (weapon cycles, then projectiles),
        // so it is still in flight at the end of one step fewer than the step it lands on.
        let landed = ((distance - s(radius)) / speed)
            .saturating_ceil()
            .saturating_to_u32()
            - 1;
        (published, landed)
    };
    for radius in [R_3X3, H_3X3] {
        assert_eq!(
            projectile_flight(radius),
            expected(radius),
            "radius {radius}"
        );
    }
    assert_ne!(
        expected(R_3X3),
        expected(H_3X3),
        "the radius moves the flight"
    );
}

/// The first shipped weapon with a circular splash and a range of at least 1, cloned, its splash
/// centred on the struck unit (not offset by its radius) and able to strike anything.
fn splash_weapon(world: &SimWorld) -> sim_types::WeaponSpec {
    let mut weapon = world
        .catalog
        .archetypes()
        .flat_map(|arch| [&arch.weapon, &arch.ground_weapon, &arch.air_weapon])
        .flatten()
        .find(|weapon| {
            weapon.max_range >= s(1.0)
                && weapon
                    .splash
                    .as_ref()
                    .is_some_and(|splash| splash.shape == sim_types::SplashShape::Circle)
        })
        .cloned()
        .expect("invariant: the shipped catalog has a circular splash weapon");
    weapon.target_flags = sim_types::AttributeFlags::all();
    weapon.min_range = SimScalar::ZERO;
    if let Some(splash) = weapon.splash.as_mut() {
        splash.offset_by_unit_radius = false;
    }
    weapon
}

/// A splash attacker of player 0 attacks a passive body of player 1 standing south of a 3×3 of
/// player 1 with radius `radius`, so that the structure's centre is `reach + offset` from the
/// body's centre, the splash's search point. Returns whether the structure lost vitals within 80
/// ticks.
fn splash_strikes(radius: f64, offset: impl Fn(SimScalar) -> SimScalar) -> bool {
    let (mut world, structure) = with_structure(radius);
    let weapon = splash_weapon(&world);
    let reach = weapon
        .splash
        .as_ref()
        .expect("chosen for its splash")
        .reach();
    let range = weapon.max_range;
    let gunner = register(&mut world, "structure_radius_gunner", 0.5, 0.5, |arch| {
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon);
    });
    let body = register(&mut world, "structure_radius_struck", 0.5, 0.5, |arch| {
        arch.max_hp = s(1000.0);
    });
    let point = SimVec2::new(s(CENTRE), s(CENTRE) - offset(reach));
    let struck = world
        .spawn_unit(body, PlayerId(1), point)
        .expect("open ground");
    let firing = SimVec2::new(point.x, point.y - s(1.0) - range * SimScalar::HALF);
    let attacker = world
        .spawn_unit(gunner, PlayerId(0), firing)
        .expect("open ground");
    let full = vitals(&world, structure);
    let before = vitals(&world, struck);
    let hold = PlayerCommand {
        player_id: PlayerId(1),
        entities: vec![struck],
        order: UnitOrder::HoldPosition,
        queue: false,
    };
    step_with(&mut world, vec![attack(0, attacker, struck), hold]);
    for _ in 0..80 {
        step(&mut world);
        assert_eq!(
            world.entities[&struck].position, point,
            "the search point holds"
        );
    }
    assert!(
        vitals(&world, struck) < before,
        "the primary target was struck"
    );
    vitals(&world, structure) < full
}

/// R-7 (guard): a splash reaches a structure by its radius. A circular splash centred on its
/// primary target strikes a 3×3 of radius 1.8125 whose centre lies `reach + R − 2⁻¹⁶` from the
/// search point, not one at `+ 2⁻¹⁶`, and not the first with radius 1.5 (A5).
#[test]
fn splash_reaches_a_structure_by_its_radius() {
    assert!(splash_strikes(R_3X3, |reach| reach + s(R_3X3) - epsilon()));
    assert!(!splash_strikes(R_3X3, |reach| reach + s(R_3X3) + epsilon()));
    assert!(!splash_strikes(H_3X3, |reach| reach + s(R_3X3) - epsilon()));
}

/// A scratch unarmed 3×3 of player 0 with radius `radius` at [`CELL`] made a carrier (capacity
/// 4; no shipped structure carries), and a passenger of player 0 (radius 0.5) `distance` west
/// of its centre ordered to load into it. Returns whether it is aboard after the order's tick,
/// without having moved.
fn boards_at_once(radius: f64, distance: impl Fn(SimScalar) -> SimScalar) -> bool {
    let mut world = world_on(flat(48));
    let kind = scratch_structure(&mut world, 3, radius, "structure_radius_bunker", unarmed);
    world
        .catalog
        .get_mut(kind)
        .expect("loaded")
        .capabilities
        .transport_capacity = 4;
    let carrier = world
        .spawn_structure(kind, PlayerId(0), CELL, CELL)
        .expect("open ground");
    let walker = register(&mut world, "structure_radius_passenger", 0.5, 0.375, |_| {});
    let start = SimVec2::new(s(CENTRE) - distance(s(0.5)), s(CENTRE));
    let passenger = world
        .spawn_unit(walker, PlayerId(0), start)
        .expect("open ground");
    step_with(
        &mut world,
        vec![PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![passenger],
            order: UnitOrder::Load { carrier },
            queue: false,
        }],
    );
    let entity = &world.entities[&passenger];
    entity.is_loaded_in_transport() && entity.position == start
}

/// R-8 (guard): a passenger boards a carrier structure at its radius: from `R + r + 1 − 2⁻¹⁶`
/// (the boarding reach, 1, beyond both edges) it is aboard on the order's tick, from `+ 2⁻¹⁶` it
/// is not; with radius 1.5 it is not from the first distance either (A11). (Out of reach, the
/// order would walk to the carrier's centre cell, which a structure's footprint covers, so it
/// ends as unreachable: no shipped structure carries, and the approach is outside this plan.)
#[test]
fn a_bunker_boards_at_its_radius() {
    let reach = |r: SimScalar, radius: f64| s(radius) + r + s(1.0);
    assert!(boards_at_once(R_3X3, |r| reach(r, R_3X3) - epsilon()));
    assert!(!boards_at_once(R_3X3, |r| reach(r, R_3X3) + epsilon()));
    assert!(!boards_at_once(H_3X3, |r| reach(r, R_3X3) - epsilon()));
}

/// A dasher of player 0 (radius 0.5, the shortest-reaching shipped melee weapon, a dash with
/// trigger gap 0.6 within centre distance 4) `distance` west of a 3×3 of player 1 with radius
/// `radius`, ordered to attack it. Returns whether the dash started within two ticks.
fn dashes(radius: f64, distance: impl Fn(SimScalar) -> SimScalar) -> bool {
    let (mut world, structure) = with_structure(radius);
    let weapon = melee_weapon(&world);
    let spec = sim_types::AttackDashSpec {
        speed_multiplier: s(2.0),
        duration_ticks: 10,
        cooldown_ticks: 100,
        min_gap: s(0.6),
        max_distance: s(4.0),
    };
    let kind = register(&mut world, "structure_radius_dasher", 0.5, 0.375, |arch| {
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon);
        arch.capabilities.attack_dash = Some(spec);
    });
    let start = SimVec2::new(s(CENTRE) - distance(spec.min_gap), s(CENTRE));
    let dasher = world
        .spawn_unit(kind, PlayerId(0), start)
        .expect("open ground");
    step_with(&mut world, vec![attack(0, dasher, structure)]);
    step(&mut world);
    world.entities[&dasher].attack_dash_cooldown > 0
}

/// R-9 (guard): the Charge window to a structure uses its radius. The dash starts when the gap
/// to a 3×3 of radius 1.8125, `d − r − R`, exceeds the trigger gap (at `d = r + R + 0.6 +
/// 2⁻¹⁶`), not at `− 2⁻¹⁶`; with radius 1.5 it starts at the second distance (A9).
#[test]
fn a_charge_window_to_a_structure_uses_its_radius() {
    let at_gap = |gap: SimScalar| s(0.5) + s(R_3X3) + gap;
    assert!(dashes(R_3X3, |gap| at_gap(gap) + epsilon()));
    assert!(!dashes(R_3X3, |gap| at_gap(gap) - epsilon()));
    assert!(dashes(H_3X3, |gap| at_gap(gap) - epsilon()));
}
