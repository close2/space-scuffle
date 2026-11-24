//! Gas gatherers hidden inside their extractor (plan `2026-11-24-hidden-harvesters` P-2 to P-9,
//! B-161).
//!
//! # Responsibility
//! Proves the simulation half of the plan: a gatherer granted a node whose extractor hides its
//! harvesters is inside it, in the engine's `Hidden` state, for exactly its gather; while inside
//! it is not acquired, attacked, splashed, found by an effect search or hit by a missile, it
//! acquires and sees nothing, and no player order reaches it (Q0108's default); it survives its
//! extractor's death and comes out where it went in (Q0107's defaults); and the invariant of
//! `UnitTask::housed_in` holds on every tick of a long run. In the movement phase (WP5, D-11) it
//! is no collision pair, push partner or displacement obstacle in either contact pipeline, by the
//! hidden rule alone.
//!
//! # How it fits
//! Worlds come from `sim_fixtures`' shipped catalogs on a flat map. Every archetype is found by
//! property (a harvester, a depot, a gas extractor, an armed ground unit, a ground unit with a
//! circular splash, a unit with a projectile weapon); the area spell is found by the name of its
//! ability and unlocked by its upgrade's slug, as `psionic_storm_tests` does. Every time is read
//! from the catalog. Behaviour is driven by `PlayerCommand`s; setup may place entities, set
//! energy, or put a missile in flight. WP5's tests also clear a worker's walking flag or its
//! housing by hand and then run the movement phases alone, so no harvest step sets them again.
//!
//! # Invariants
//! Ticks are simulation ticks: a tick's number is `SimWorld::current_tick` after its step, and
//! "at the end of tick t" means after that step. Positions are fixed-point world units.

use std::error::Error;

use sim_core::catalog::{ContactRules, MovementPipeline, UnitArchetype};
use sim_core::economy::{GameCommandError, TaskEventKind};
use sim_core::movement::DisplacementAdmissionError;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell, ResourceNode};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, PlayerSlot, ResourceKind, ResourceNodeId,
    ShapeKind, SimScalar, SimVec2, SplashShape, Tick, TickBundle, UnitOrder, UnitTypeId,
    VisibilityState,
};

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);
const RIVAL: PlayerId = PlayerId(1);

/// The geyser the gas crews gather from; its extractor stands on the 3 × 3 centred on it.
const GEYSER: (u32, u32) = (20, 6);
/// A second geyser, for the long run of P-9.
const SECOND_GEYSER: (u32, u32) = (20, 16);
/// A mineral field, for the mineral control of P-2.
const FIELD: (u32, u32) = (12, 30);
/// The area spell of P-3, named as `psionic_storm_tests` names it; and its upgrade's slug.
const STORM: &str = "Psionic Storm";
const STORM_RESEARCH: &str = "psionic_storm";

fn point(x: f32, y: f32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn centre(cell: (u32, u32)) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(cell.0) + SimScalar::HALF,
        SimScalar::from_num(cell.1) + SimScalar::HALF,
    )
}

fn node(kind: ResourceKind, cell: (u32, u32)) -> ResourceNode {
    ResourceNode {
        pos: centre(cell),
        kind,
        remaining: 20_000,
        cell,
        placed: None,
    }
}

fn is_structure(arch: &UnitArchetype) -> bool {
    matches!(arch.shape, ShapeKind::BuildingDome { .. })
}

/// The first archetype of `world`'s catalog that `test` accepts.
fn find(
    world: &SimWorld,
    test: impl Fn(&UnitArchetype) -> bool,
    what: &'static str,
) -> Result<UnitTypeId, Box<dyn Error>> {
    Ok(world
        .catalog
        .archetypes()
        .find(|a| test(a))
        .map(|a| a.unit_type_id)
        .ok_or(what)?)
}

fn worker_kind(world: &SimWorld) -> Result<UnitTypeId, Box<dyn Error>> {
    find(world, |a| a.capabilities.harvester.is_some(), "a harvester")
}

/// A mobile ground unit, not a harvester, whose ground weapon strikes at once (no missile and
/// no splash): a plain armed enemy.
fn rifle_kind(world: &SimWorld) -> Result<UnitTypeId, Box<dyn Error>> {
    find(
        world,
        |a| {
            !is_structure(a)
                && a.mover_type == MoverType::Ground
                && a.capabilities.harvester.is_none()
                && a.ground_weapon
                    .as_ref()
                    .is_some_and(|w| w.splash.is_none() && w.projectile_speed.is_none() && !w.melee)
        },
        "an armed ground unit with an instant ranged weapon",
    )
}

/// The mobile ground unit whose ground weapon has the widest circular splash.
fn splash_kind(world: &SimWorld) -> Result<UnitTypeId, Box<dyn Error>> {
    world
        .catalog
        .archetypes()
        .filter(|a| !is_structure(a) && a.mover_type == MoverType::Ground)
        .filter_map(|a| {
            let splash = a.ground_weapon.as_ref()?.splash.as_ref()?;
            let widest = splash.zones.iter().map(|zone| zone.radius).max()?;
            (splash.shape == SplashShape::Circle).then_some((widest, a.unit_type_id))
        })
        .max()
        .map(|(_, id)| id)
        .ok_or_else(|| "a ground unit with a circular splash".into())
}

/// A flat 40 × 40 world with two geysers, a mineral field, a hall of `ME` at (2, 2), a
/// completed extractor of `ME` over each geyser and the bank emptied.
fn world() -> Result<SimWorld, Box<dyn Error>> {
    let map = GameMap::from_cells(
        40,
        40,
        vec![MapCell::default_ground(CliffLevel(0)); 1600],
        vec![
            node(ResourceKind::Gas, GEYSER),
            node(ResourceKind::Gas, SECOND_GEYSER),
            node(ResourceKind::Mineral, FIELD),
        ],
        Vec::new(),
    )?;
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut world = SimWorld::with_catalogs(map, 23, catalogs.units, catalogs.upgrades)?;
    let hall = find(
        &world,
        |a| a.is_resource_dropoff && a.requires_structures.is_empty(),
        "a depot",
    )?;
    let extractor = find(
        &world,
        |a| a.capabilities.is_gas_extractor && a.capabilities.hides_harvesters,
        "an extractor that hides its harvesters",
    )?;
    world.spawn_structure(hall, ME, 2, 2)?;
    for cell in [GEYSER, SECOND_GEYSER] {
        world.spawn_structure(extractor, ME, cell.0 - 1, cell.1 - 1)?;
    }
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    world.player_resources[PlayerSlot::ALL[0]].gas = 0;
    Ok(world)
}

/// The resource node at `cell`.
fn node_at(world: &SimWorld, cell: (u32, u32)) -> Result<ResourceNodeId, Box<dyn Error>> {
    Ok((0_u32..)
        .zip(world.map.resources())
        .find(|(_, n)| n.cell == cell)
        .map(|(i, _)| ResourceNodeId(i))
        .ok_or("no node at the cell")?)
}

/// The extractor standing over the geyser at `cell`.
fn extractor_at(world: &SimWorld, cell: (u32, u32)) -> Result<EntityId, Box<dyn Error>> {
    Ok(world
        .map
        .cell(cell.0, cell.1)?
        .occupying_structure
        .ok_or("no extractor over the geyser")?)
}

/// The gas gather time, ticks, read from the catalog.
fn gas_time(world: &SimWorld) -> Result<u16, Box<dyn Error>> {
    Ok(world
        .catalog
        .placement_rules()
        .harvest(ResourceKind::Gas)
        .ok_or("no gas harvest row")?
        .time_ticks)
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
    assert_eq!(world.diagnostics(), []);
}

fn command(player: PlayerId, entities: Vec<EntityId>, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities,
        order,
        queue: false,
    }
}

/// Spawns a worker of `ME` at `at` and orders it to gather from the node at `cell`.
fn gatherer(
    world: &mut SimWorld,
    at: SimVec2,
    cell: (u32, u32),
) -> Result<EntityId, Box<dyn Error>> {
    let worker = world.spawn_unit(worker_kind(world)?, ME, at)?;
    let target = node_at(world, cell)?;
    step(
        world,
        vec![command(ME, vec![worker], UnitOrder::Harvest { target })],
    );
    Ok(worker)
}

fn gathering(world: &SimWorld, id: EntityId) -> bool {
    world.entities[&id].task.gather_ticks > 0
}

/// Steps until `id` starts a gather; returns the grant tick.
fn until_granted(world: &mut SimWorld, id: EntityId) -> Result<u32, Box<dyn Error>> {
    for _ in 0..2_000 {
        step(world, Vec::new());
        if gathering(world, id) {
            return Ok(world.current_tick);
        }
    }
    Err("the worker never started gathering".into())
}

fn vitals(world: &SimWorld, id: EntityId) -> SimScalar {
    world
        .entities
        .get(&id)
        .map_or(SimScalar::ZERO, |e| e.hp + e.shields)
}

/// P-2: a gas gatherer granted on tick `g` is inside its extractor at the end of ticks `g` to
/// `g + H − 1` and out at `g + H`, the take; a second worker waiting in the line is never inside.
#[test]
fn a_gas_gatherer_is_inside_for_exactly_its_gather() -> TestResult {
    let mut world = world()?;
    let time = u32::from(gas_time(&world)?);
    let extractor = extractor_at(&world, GEYSER)?;
    let first = gatherer(&mut world, point(15.0, 6.0), GEYSER)?;
    let second = gatherer(&mut world, point(15.0, 7.0), GEYSER)?;
    let granted = until_granted(&mut world, first)?;
    let mut inside = Vec::new();
    for _ in 0..time {
        assert_eq!(
            world.entities[&second].housing(),
            None,
            "the waiter is never inside"
        );
        if world.entities[&first].housing() == Some(extractor) {
            inside.push(world.current_tick);
        }
        step(&mut world, Vec::new());
    }
    let expected: Vec<u32> = (granted..granted + time).collect();
    assert_eq!(
        inside, expected,
        "inside from the grant to the tick before the take"
    );
    assert_eq!(world.current_tick, granted + time);
    assert!(world.entities[&first].cargo.is_some(), "the take");
    assert!(!world.entities[&first].is_housed(), "out at the take");
    Ok(())
}

/// P-2: over a long run, a gas gatherer is inside exactly while it gathers, and a mineral
/// gatherer is never inside.
#[test]
fn inside_is_the_gas_gather_and_never_a_mineral_one() -> TestResult {
    let mut world = world()?;
    let gas = gatherer(&mut world, point(15.0, 6.0), GEYSER)?;
    let mineral = gatherer(&mut world, point(8.0, 27.0), FIELD)?;
    let mut gas_ticks = 0;
    let mut mineral_ticks = 0;
    for _ in 0..1_500 {
        step(&mut world, Vec::new());
        assert_eq!(world.entities[&gas].is_housed(), gathering(&world, gas));
        assert!(!world.entities[&mineral].is_housed());
        gas_ticks += u32::from(gathering(&world, gas));
        mineral_ticks += u32::from(gathering(&world, mineral));
    }
    assert!(gas_ticks > 0 && mineral_ticks > 0, "both gathered");
    Ok(())
}

/// The P-3 scene at the end of a gas gatherer's grant tick: the housed worker, an idle visible
/// worker of the same owner 0.6 west of it as the control, and the grant tick.
struct Scene {
    world: SimWorld,
    housed: EntityId,
    control: EntityId,
    granted: u32,
}

fn scene() -> Result<Scene, Box<dyn Error>> {
    let mut world = world()?;
    let housed = gatherer(&mut world, point(15.0, 6.0), GEYSER)?;
    let granted = until_granted(&mut world, housed)?;
    let at = world.entities[&housed].position - point(0.6, 0.0);
    let control = world.spawn_unit(worker_kind(&world)?, ME, at)?;
    Ok(Scene {
        world,
        housed,
        control,
        granted,
    })
}

/// Steps through the rest of the housed worker's gather, calling `each` after every tick on
/// which it is still inside.
fn while_inside(scene: &mut Scene, mut each: impl FnMut(&SimWorld) -> TestResult) -> TestResult {
    let take = scene.granted + u32::from(gas_time(&scene.world)?);
    while scene.world.current_tick + 1 < take {
        step(&mut scene.world, Vec::new());
        assert!(scene.world.entities[&scene.housed].is_housed());
        each(&scene.world)?;
    }
    Ok(())
}

/// P-3: an armed enemy in range of both acquires the control and never the housed worker, and
/// the housed worker is acquirable again on the take's tick.
#[test]
fn an_enemy_acquires_the_control_and_never_the_housed_worker() -> TestResult {
    let mut scene = scene()?;
    let rifle = rifle_kind(&scene.world)?;
    let range = scene
        .world
        .catalog
        .get(rifle)?
        .ground_weapon
        .as_ref()
        .ok_or("weapon")?
        .max_range;
    let at = scene.world.entities[&scene.control].position
        - SimVec2::new(range * SimScalar::HALF, SimScalar::ZERO);
    let enemy = scene.world.spawn_unit(rifle, RIVAL, at)?;
    step(
        &mut scene.world,
        vec![command(RIVAL, vec![enemy], UnitOrder::HoldPosition)],
    );
    let worker_vitals = vitals(&scene.world, scene.housed);
    let control_vitals = vitals(&scene.world, scene.control);
    let (housed, control) = (scene.housed, scene.control);
    while_inside(&mut scene, |world| {
        assert_ne!(world.entities[&enemy].current_target, Some(housed));
        assert_eq!(
            vitals(world, housed),
            worker_vitals,
            "the housed worker takes no damage"
        );
        assert!(!world.is_entity_available_for_combat(RIVAL, housed));
        Ok(())
    })?;
    assert!(
        vitals(&scene.world, control) < control_vitals,
        "the enemy shot the control while the worker was inside"
    );
    step(&mut scene.world, Vec::new());
    assert!(!scene.world.entities[&housed].is_housed(), "the take");
    assert!(scene.world.is_entity_available_for_combat(RIVAL, housed));
    Ok(())
}

/// P-3: an attack ordered on a worker before it went inside is dropped on the grant tick, as
/// an attack on a unit that boards a transport is.
#[test]
fn an_attack_on_a_worker_is_dropped_when_it_goes_inside() -> TestResult {
    let mut world = world()?;
    let worker = gatherer(&mut world, point(15.0, 6.0), GEYSER)?;
    let rifle = rifle_kind(&world)?;
    let enemy = world.spawn_unit(rifle, RIVAL, point(30.0, 30.0))?;
    step(
        &mut world,
        vec![command(RIVAL, vec![enemy], UnitOrder::attack(worker))],
    );
    assert!(matches!(
        world.entities[&enemy].order_queue.front(),
        Some(UnitOrder::AttackTarget { target, .. }) if *target == worker
    ));
    let mut granted = None;
    for _ in 0..2_000 {
        step(&mut world, Vec::new());
        if gathering(&world, worker) {
            granted = Some(world.current_tick);
            break;
        }
        assert!(
            matches!(
                world.entities[&enemy].order_queue.front(),
                Some(UnitOrder::AttackTarget { target, .. }) if *target == worker
            ),
            "the attack holds while the worker is outside"
        );
    }
    assert!(granted.is_some(), "the worker went inside");
    assert!(
        !matches!(
            world.entities[&enemy].order_queue.front(),
            Some(UnitOrder::AttackTarget { target, .. }) if *target == worker
        ),
        "the attack is dropped on the grant tick"
    );
    // The chase ends the order as for a target gone into a transport (`end_attack`), which
    // leaves the attacker's last-target memory as it is; the order is what is dropped.
    Ok(())
}

/// P-3: a splash on the control spares the housed worker beside it.
#[test]
fn splash_on_the_control_spares_the_housed_worker() -> TestResult {
    let mut scene = scene()?;
    let splasher = splash_kind(&scene.world)?;
    let at = scene.world.entities[&scene.control].position - point(1.5, 0.0);
    let enemy = scene.world.spawn_unit(splasher, RIVAL, at)?;
    let (housed, control) = (scene.housed, scene.control);
    step(
        &mut scene.world,
        vec![command(RIVAL, vec![enemy], UnitOrder::attack(control))],
    );
    let worker_vitals = vitals(&scene.world, housed);
    let control_vitals = vitals(&scene.world, control);
    while_inside(&mut scene, |world| {
        assert_eq!(
            vitals(world, housed),
            worker_vitals,
            "no splash on the housed worker"
        );
        Ok(())
    })?;
    assert!(
        vitals(&scene.world, control) < control_vitals,
        "the splash struck the control"
    );
    Ok(())
}

/// P-3: an area spell centred between the control and the housed worker finds the control and
/// not the housed worker.
#[test]
fn an_area_spell_finds_the_control_and_not_the_housed_worker() -> TestResult {
    let mut scene = scene()?;
    let caster_kind = find(
        &scene.world,
        |a| a.abilities.iter().any(|ability| ability.name == STORM),
        "a caster of the area spell",
    )?;
    scene.world.research_upgrade(RIVAL, STORM_RESEARCH)?;
    let (housed, control) = (scene.housed, scene.control);
    let between = point(0.3, 0.0) + scene.world.entities[&control].position;
    let caster = scene
        .world
        .spawn_unit(caster_kind, RIVAL, between - point(5.0, 0.0))?;
    let caster_entity = scene.world.entities.get_mut(&caster).ok_or("caster")?;
    caster_entity.energy = SimScalar::from_num(200);
    let index = scene
        .world
        .catalog
        .get(caster_kind)?
        .abilities
        .iter()
        .position(|ability| ability.name == STORM)
        .ok_or("the spell")?;
    let worker_vitals = vitals(&scene.world, housed);
    let control_vitals = vitals(&scene.world, control);
    step(
        &mut scene.world,
        vec![command(
            RIVAL,
            vec![caster],
            UnitOrder::CastAbility {
                ability_index: u8::try_from(index)?,
                target_pos: Some(between),
                target_entity: None,
            },
        )],
    );
    assert_eq!(vitals(&scene.world, housed), worker_vitals);
    while_inside(&mut scene, |world| {
        assert_eq!(
            vitals(world, housed),
            worker_vitals,
            "the search skips the housed worker"
        );
        Ok(())
    })?;
    assert!(
        vitals(&scene.world, control) < control_vitals,
        "the spell struck the control"
    );
    Ok(())
}

/// P-3: a missile in flight at a worker that goes inside loses its target and lands on the
/// point, harming no one there.
#[test]
fn a_missile_at_a_worker_that_goes_inside_lands_on_the_point() -> TestResult {
    let mut scene = scene()?;
    let shooter_kind = find(
        &scene.world,
        |a| {
            !is_structure(a)
                && a.ground_weapon
                    .as_ref()
                    .is_some_and(|w| w.projectile_speed.is_some() && w.splash.is_none())
        },
        "a unit with a missile weapon",
    )?;
    let arch = scene.world.catalog.get(shooter_kind)?;
    let weapon = arch.ground_weapon.clone().ok_or("weapon")?;
    let slug = arch.slug.clone();
    let housed = scene.housed;
    let target_pos = scene.world.entities[&housed].position;
    scene.world.projectiles.push(sim_core::ActiveProjectile {
        id: 1,
        owner: RIVAL,
        attacker_id: None,
        source_pos: target_pos - point(4.0, 0.0),
        current_pos: target_pos - point(1.0, 0.0),
        target_id: Some(housed),
        target_pos,
        target_mover: MoverType::Ground,
        speed: SimScalar::ONE,
        weapon_spec: weapon,
        source_slug: Some(slug),
        remaining_lifetime_ticks: 20,
        strikes: 1,
        target_teleport_count: scene.world.entities[&housed].teleport_count,
    });
    let worker_vitals = vitals(&scene.world, housed);
    for _ in 0..5 {
        step(&mut scene.world, Vec::new());
        assert_eq!(vitals(&scene.world, housed), worker_vitals);
    }
    assert!(scene.world.projectiles.is_empty(), "the missile landed");
    Ok(())
}

/// P-3: the housed worker acquires nothing, even with an enemy in its reach.
#[test]
fn the_housed_worker_acquires_nothing() -> TestResult {
    let mut scene = scene()?;
    let rifle = rifle_kind(&scene.world)?;
    let at = scene.world.entities[&scene.housed].position - point(0.8, 0.8);
    let enemy = scene.world.spawn_unit(rifle, RIVAL, at)?;
    step(
        &mut scene.world,
        vec![command(RIVAL, vec![enemy], UnitOrder::Stop)],
    );
    let housed = scene.housed;
    while_inside(&mut scene, |world| {
        assert_eq!(world.entities[&housed].current_target, None);
        Ok(())
    })
}

/// P-4 (Q0107 default (2a)): a housed worker is no sight source. On the tick after the grant the
/// owner sees exactly the cells it sees in the same world without that worker. The extractor's
/// own sight is cut to one unit for the test, so the worker's sight is not covered by it.
#[test]
fn a_housed_worker_sees_nothing() -> TestResult {
    let blind = |world: &mut SimWorld| -> TestResult {
        let extractor = world.entities[&extractor_at(world, GEYSER)?].unit_type_id;
        world.catalog.get_mut(extractor)?.sight_radius = SimScalar::ONE;
        Ok(())
    };
    let mut with = world()?;
    blind(&mut with)?;
    let worker = gatherer(&mut with, point(15.0, 6.0), GEYSER)?;
    let granted = until_granted(&mut with, worker)?;
    step(&mut with, Vec::new());
    assert!(with.entities[&worker].is_housed());
    let mut without = world()?;
    blind(&mut without)?;
    while without.current_tick < granted + 1 {
        step(&mut without, Vec::new());
    }
    let visible = |world: &SimWorld| -> Result<Vec<bool>, Box<dyn Error>> {
        Ok(world
            .vision_maps
            .get(ME)
            .ok_or("vision")?
            .states()
            .iter()
            .map(|state| *state == VisibilityState::Visible)
            .collect())
    };
    assert_eq!(visible(&with)?, visible(&without)?);
    Ok(())
}

/// P-5 (Q0108 default (a), as filed): no order reaches a housed worker. An order naming only
/// it, immediate or queued, is refused with `HousedRecipient`, and it gathers on; an order
/// naming it and a worker outside reaches only the one outside.
#[test]
fn no_order_reaches_a_housed_worker() -> TestResult {
    let mut world = world()?;
    let time = u32::from(gas_time(&world)?);
    let worker = gatherer(&mut world, point(15.0, 6.0), GEYSER)?;
    let other = world.spawn_unit(worker_kind(&world)?, ME, point(4.0, 12.0))?;
    let granted = until_granted(&mut world, worker)?;
    let away = point(10.0, 35.0);
    for queue in [false, true] {
        let mut order = command(ME, vec![worker], UnitOrder::Move { target: away });
        order.queue = queue;
        step(&mut world, vec![order]);
        assert!(
            world
                .game_command_outcomes()
                .iter()
                .any(|outcome| outcome.result == Err(GameCommandError::HousedRecipient(worker))),
            "queue {queue}: {:?}",
            world.game_command_outcomes()
        );
        assert!(world.entities[&worker].is_housed());
        assert_eq!(
            world.entities[&worker].order_queue.len(),
            1,
            "queue {queue}"
        );
    }
    step(
        &mut world,
        vec![command(
            ME,
            vec![worker, other],
            UnitOrder::Move { target: away },
        )],
    );
    assert!(world.entities[&worker].is_housed());
    assert!(matches!(
        world.entities[&other].order_queue.front(),
        Some(UnitOrder::Move { .. })
    ));
    while world.current_tick < granted + time {
        step(&mut world, Vec::new());
    }
    assert!(
        world.entities[&worker].cargo.is_some(),
        "the gather ran its time"
    );
    Ok(())
}

/// P-5's other half: a queued order given to a worker outside waits behind the `Harvest`
/// order and the gather keeps its time (the line, not the hiding, decides).
#[test]
fn a_queued_order_waits_for_the_delivery() -> TestResult {
    let mut world = world()?;
    let worker = gatherer(&mut world, point(15.0, 6.0), GEYSER)?;
    let mut order = command(
        ME,
        vec![worker],
        UnitOrder::Move {
            target: point(10.0, 35.0),
        },
    );
    order.queue = true;
    step(&mut world, vec![order]);
    assert_eq!(world.entities[&worker].order_queue.len(), 2);
    until_granted(&mut world, worker)?;
    assert!(matches!(
        world.entities[&worker].order_queue.front(),
        Some(UnitOrder::Harvest { .. })
    ));
    Ok(())
}

/// P-6 and P-7 (Q0107 defaults (1a), (3a)): when its extractor dies, the housed worker is out
/// at the end of that tick, alive, damageable, where it went in and empty; on the next tick its
/// order ends with `ExtractorLost`. A worker that takes comes out where it went in.
#[test]
fn a_worker_survives_its_extractors_death_and_comes_out_where_it_went_in() -> TestResult {
    let mut world = world()?;
    let time = u32::from(gas_time(&world)?);
    let extractor = extractor_at(&world, GEYSER)?;
    let worker = gatherer(&mut world, point(15.0, 6.0), GEYSER)?;
    let granted = until_granted(&mut world, worker)?;
    let went_in = world.entities[&worker].position;
    while world.current_tick < granted + time {
        step(&mut world, Vec::new());
    }
    assert!(world.entities[&worker].cargo.is_some(), "the take");
    assert_eq!(world.entities[&worker].position, went_in, "P-7");
    // The next trip, then an enemy kills the extractor while the worker is inside.
    let granted = {
        let mut next = None;
        for _ in 0..2_000 {
            step(&mut world, Vec::new());
            if gathering(&world, worker) && world.entities[&worker].cargo.is_none() {
                next = Some(world.current_tick);
                break;
            }
        }
        next.ok_or("a second gather")?
    };
    let went_in = world.entities[&worker].position;
    let rifle = rifle_kind(&world)?;
    let range = world
        .catalog
        .get(rifle)?
        .ground_weapon
        .as_ref()
        .ok_or("weapon")?
        .max_range;
    let at = centre(GEYSER)
        + SimVec2::new(
            SimScalar::from_num(1.5) + range * SimScalar::HALF,
            SimScalar::ZERO,
        );
    let enemy = world.spawn_unit(rifle, RIVAL, at)?;
    world.entities.get_mut(&extractor).ok_or("extractor")?.hp = SimScalar::ONE;
    step(
        &mut world,
        vec![command(RIVAL, vec![enemy], UnitOrder::attack(extractor))],
    );
    let mut died = None;
    while world.current_tick < granted + time - 1 {
        assert!(
            world.entities[&worker].is_housed(),
            "inside until the death"
        );
        step(&mut world, Vec::new());
        if !world.entities.contains_key(&extractor) {
            died = Some(world.current_tick);
            break;
        }
    }
    assert!(
        died.is_some(),
        "the extractor died while the worker was inside"
    );
    let actor = &world.entities[&worker];
    assert!(actor.is_alive() && !actor.is_housed() && !actor.is_hidden());
    assert!(world.is_entity_available_for_combat(RIVAL, worker));
    assert_eq!(actor.position, went_in, "out where it went in");
    assert_eq!(actor.cargo, None, "the gather is lost");
    step(&mut world, Vec::new());
    assert!(world
        .task_events()
        .iter()
        .any(|event| event.entity == worker
            && event.kind == TaskEventKind::ExtractorLost
            && event.tick == Tick(world.current_tick)));
    Ok(())
}

/// The invariant of `UnitTask::housed_in` for every unit of `world`: housed in `s` implies a
/// front `Harvest` order, a gather under way, and `s` alive and hiding its harvesters; and a
/// gas gather under way at such a structure implies housed in it.
fn check_invariant(world: &SimWorld) -> TestResult {
    for entity in world.entities.values() {
        let gathering_at = match entity.order_queue.front() {
            Some(UnitOrder::Harvest { target }) if entity.task.gather_ticks > 0 => {
                let node = &world.map.resources()[target.0 as usize];
                (node.kind == ResourceKind::Gas)
                    .then(|| {
                        world
                            .map
                            .cell(node.cell.0, node.cell.1)
                            .ok()?
                            .occupying_structure
                    })
                    .flatten()
            }
            _ => None,
        };
        if let Some(structure) = entity.housing() {
            let site = world.entities.get(&structure).ok_or("housing is gone")?;
            assert!(
                site.is_alive(),
                "tick {}: {:?}",
                world.current_tick,
                entity.id
            );
            assert!(
                world
                    .catalog
                    .get(site.unit_type_id)?
                    .capabilities
                    .hides_harvesters
            );
            assert_eq!(gathering_at, Some(structure), "tick {}", world.current_tick);
        } else {
            assert_eq!(
                gathering_at, None,
                "tick {}: {:?} gathers outside",
                world.current_tick, entity.id
            );
        }
    }
    Ok(())
}

/// P-9: in a 6,000-tick run with three workers on each of two geysers and an enemy force that
/// kills one extractor half-way, every tick satisfies the invariant for every unit.
#[test]
fn the_housing_invariant_holds_over_a_long_run() -> TestResult {
    let mut world = world()?;
    let worker = worker_kind(&world)?;
    let mut commands = Vec::new();
    for (row, cell) in [GEYSER, SECOND_GEYSER].into_iter().enumerate() {
        let y = if row == 0 { 5.0 } else { 15.0 };
        let crew = [0.0, 0.7, 1.4]
            .into_iter()
            .map(|dy| world.spawn_unit(worker, ME, point(15.0, y + dy)))
            .collect::<Result<Vec<_>, _>>()?;
        commands.push(command(
            ME,
            crew,
            UnitOrder::Harvest {
                target: node_at(&world, cell)?,
            },
        ));
    }
    step(&mut world, commands);
    let mut housed_ticks = 0_u32;
    let doomed = extractor_at(&world, SECOND_GEYSER)?;
    for tick in 0..6_000 {
        if tick == 3_000 {
            let rifle = rifle_kind(&world)?;
            let squad = [13.0, 13.8, 14.6, 15.4, 16.2, 17.0]
                .into_iter()
                .map(|y| world.spawn_unit(rifle, RIVAL, point(26.0, y)))
                .collect::<Result<Vec<_>, _>>()?;
            step(
                &mut world,
                vec![command(RIVAL, squad, UnitOrder::attack(doomed))],
            );
        } else {
            step(&mut world, Vec::new());
        }
        check_invariant(&world)?;
        housed_ticks += u32::try_from(world.entities.values().filter(|e| e.is_housed()).count())?;
    }
    assert!(
        !world.entities.contains_key(&doomed),
        "the enemy killed the extractor"
    );
    assert!(housed_ticks > 0, "workers were inside");
    Ok(())
}

/// D-9 (WP3): the snapshot publishes a housed worker's bubble with `housed` set, on exactly the
/// ticks it is inside, rather than leaving it out, so a client keeps its render entity.
#[test]
fn the_snapshot_publishes_a_housed_worker_as_housed() -> TestResult {
    let mut world = world()?;
    let worker = gatherer(&mut world, point(15.0, 6.0), GEYSER)?;
    let mut inside = 0;
    for _ in 0..600 {
        let tick = world.current_tick;
        let snapshot = world.step(&TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        let bubble = snapshot
            .bubbles
            .iter()
            .find(|bubble| bubble.entity_id == worker)
            .ok_or("the worker is published on every tick")?;
        assert_eq!(bubble.housed, world.entities[&worker].is_housed());
        inside += u32::from(bubble.housed);
    }
    assert!(inside > 0, "the worker went inside");
    Ok(())
}

/// How the WP5 scenes leave the housed worker before the movement phases run alone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Housing {
    /// As the harvest step leaves it: housed and mining-walking (a ghost), the reference.
    AsHarvested,
    /// Housed, with `is_mineral_walking` cleared by hand: only the hidden rule keeps it out.
    HiddenOnly,
    /// Neither housed nor mining-walking: a plain body, the control that the scene touches it.
    Body,
}

/// Switches `world`'s contact phase to `pipeline`, as `push_priority_tests` does.
fn use_pipeline(world: &mut SimWorld, pipeline: MovementPipeline) -> TestResult {
    let rules = world.catalog.movement_rules();
    world
        .catalog
        .set_movement_rules(rules.with_contact(ContactRules {
            pipeline,
            ..rules.contact()
        })?);
    Ok(())
}

/// Leaves `id` as `housing` says.
fn set_housing(world: &mut SimWorld, id: EntityId, housing: Housing) -> TestResult {
    let actor = world.entities.get_mut(&id).ok_or("the worker is alive")?;
    match housing {
        Housing::AsHarvested => {}
        Housing::HiddenOnly => actor.is_mineral_walking = false,
        Housing::Body => {
            actor.is_mineral_walking = false;
            actor.task.housed_in = None;
        }
    }
    Ok(())
}

/// One tick of the movement and contact phases alone, so no harvest step sets the walking flag
/// again.
fn movement_tick(world: &mut SimWorld) {
    world.step_movement_and_steering();
    world.resolve_collisions_and_push();
    assert_eq!(world.diagnostics(), []);
}

/// The positions, world units, of the housed worker, an idle friend overlapping it and a friend
/// walking through it, over 24 ticks of the movement phases under `pipeline`, with the housed
/// worker left as `housing` says.
fn crossing(pipeline: MovementPipeline, housing: Housing) -> Result<Vec<SimVec2>, Box<dyn Error>> {
    let mut world = world()?;
    use_pipeline(&mut world, pipeline)?;
    let housed = gatherer(&mut world, point(15.0, 6.0), GEYSER)?;
    until_granted(&mut world, housed)?;
    let at = world.entities[&housed].position;
    let worker = worker_kind(&world)?;
    let friend = world.spawn_unit(worker, ME, at - point(0.25, 0.0))?;
    let walker = world.spawn_unit(worker, ME, at - point(0.0, 2.5))?;
    let target = at + point(0.0, 2.5);
    step(
        &mut world,
        vec![command(ME, vec![walker], UnitOrder::Move { target })],
    );
    assert!(world.entities[&housed].is_housed());
    set_housing(&mut world, housed, housing)?;
    let mut trace = Vec::new();
    for _ in 0..24 {
        movement_tick(&mut world);
        trace.extend([housed, friend, walker].map(|id| world.entities[&id].position));
    }
    Ok(trace)
}

/// WP5 (D-11, Amendment 1): a housed worker is no collision pair and no push partner in either
/// pipeline, even with its mining walk's flag cleared by hand: an idle friend standing in it and
/// a friend walking through it move exactly as they do past the ghost the harvest leaves, while
/// the same worker as a plain body moves them.
#[test]
fn a_housed_worker_is_no_body_in_either_pipeline() -> TestResult {
    for pipeline in [MovementPipeline::Solver, MovementPipeline::Legacy] {
        let reference = crossing(pipeline, Housing::AsHarvested)?;
        assert_ne!(
            crossing(pipeline, Housing::Body)?,
            reference,
            "{pipeline:?}: a plain body there is met"
        );
        assert_eq!(
            crossing(pipeline, Housing::HiddenOnly)?,
            reference,
            "{pipeline:?}: the housed worker is no body"
        );
    }
    Ok(())
}

/// WP5 (D-11): a housed worker is no obstacle to a displacement's landing point, while the same
/// worker as a plain body is.
#[test]
fn a_housed_worker_is_no_displacement_obstacle() -> TestResult {
    for housing in [Housing::HiddenOnly, Housing::Body] {
        let mut scene = scene()?;
        set_housing(&mut scene.world, scene.housed, housing)?;
        let landing = scene.world.entities[&scene.housed].position - point(0.2, 0.0);
        let admitted = scene
            .world
            .validate_displacement_destination(scene.control, landing);
        if housing == Housing::Body {
            assert_eq!(
                admitted,
                Err(DisplacementAdmissionError::Occupied(scene.housed))
            );
        } else {
            assert_eq!(admitted, Ok(()), "the housed worker takes no room");
        }
    }
    Ok(())
}

/// WP5 (D-11): the pairwise push of `resolve_push_interaction` leaves a housed worker and the
/// friend standing in it where they are, while the same worker as a plain body is pushed.
#[test]
fn a_housed_worker_is_no_push_partner() -> TestResult {
    for housing in [Housing::HiddenOnly, Housing::Body] {
        let mut scene = scene()?;
        set_housing(&mut scene.world, scene.housed, housing)?;
        let mut housed = scene.world.entities[&scene.housed].clone();
        let mut friend = scene.world.entities[&scene.control].clone();
        friend.position = housed.position - point(0.2, 0.0);
        let before = (housed.position, friend.position);
        let radius = scene
            .world
            .catalog
            .get(housed.unit_type_id)?
            .collision_radius;
        let distance = sim_core::movement::checked_distance(housed.position, friend.position)
            .ok_or("the distance fits")?;
        sim_core::resolve_push_interaction(&mut housed, &mut friend, distance, radius + radius)?;
        let after = (housed.position, friend.position);
        if housing == Housing::Body {
            assert_ne!(after, before, "a plain body is pushed");
        } else {
            assert_eq!(after, before, "the housed worker is no push partner");
        }
    }
    Ok(())
}
