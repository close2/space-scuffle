//! # Integration Test: Collision Classes (plan `2026-11-25-collision-classes`)
//!
//! Bodies touch only when their collision classes are equal: the three built-in classes
//! (ground, air, burrowed) and one class per name `units.ron` authors (D-1, D-2). The tests are
//! the plan's pre-registered acceptance:
//! - **A-1** every shipped archetype and mode that authors no class keeps the class the movement
//!   code always had (ground 0, air 1, burrowed 2);
//! - **A-2** resolution order: active mode's class, burrowed, archetype's class, layer;
//! - **A-3** load-time validation and index order (sorted names, not file order);
//! - **A-4** no contact across classes under both pipelines: positions equal those of each body
//!   alone;
//! - **A-5** contact within an authored class equals contact within the ground class, bit for bit;
//! - **A-6** attack rings are blocked only by bodies of the chaser's class;
//! - **A-7** displacement admission refuses only a point on a body of the unit's class;
//! - **A-8** a melee unit beneath a body of another class swings on the cadence of one at the
//!   rim, and a line splash fired at a unit at the attacker's own centre lies across its heading.
//!
//! # How it fits
//! The scenes find archetypes by property, never by name: two scratch bodies authoring the
//! classes `x` and `y` are inserted into a copy of the shipped `units.ron` (loaded exactly as
//! the game loads its catalogs), and every other test unit is a clone of one of them or of a
//! shipped archetype chosen by its weapon. `ContactRules::pipeline` is switched in the catalog,
//! as `push_priority_tests.rs` does.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::uninlined_format_args
)]

use sim_core::catalog::{ContactRules, MovementPipeline, RuntimeCatalogs, UnitCatalog};
use sim_core::movement::{CollisionClass, DisplacementAdmissionError};
use sim_core::{SimCoreError, SimWorld, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, SplashLineAxis, SplashShape, Tick,
    TickBundle, UnitModeSpec, UnitOrder, UnitTypeId,
};
use std::error::Error;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn Error>>;

/// The scratch body that authors class `x`.
const X_BODY: &str = "collision_class_test_x";
/// The scratch body that authors class `y`.
const Y_BODY: &str = "collision_class_test_y";

/// The catalog files [`RuntimeCatalogs::load_from_directory`] reads.
const FILES: [&str; 8] = [
    "units.ron",
    "structures.ron",
    "upgrades.ron",
    "movement_rules.ron",
    "statuses.ron",
    "ability_effects.ron",
    "placement_rules.ron",
    "factions.ron",
];

fn catalog_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog")
}

/// One unarmed scratch body in the catalog's RON form: radius 0.5, speed 0.15, push priority 10,
/// `mover` (`Ground` or `Air`) and the authored `class` (`None` authors none).
fn scratch_body(slug: &str, id: u16, mover: &str, class: Option<&str>) -> String {
    let class = class.map_or_else(String::new, |name| {
        format!("collision_class: Some(\"{name}\"),")
    });
    format!(
        r#"(
            slug: "{slug}",
            unit_type_id: {id},
            faction: "automaton_directorate",
            display_name: "{slug}",
            lore: "Scratch archetype of collision_class_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: LightProngs,
            mover_type: {mover},
            attributes: (bits: 1),
            max_hp: 10000.0,
            max_shields: 0.0,
            max_energy: 0.0,
            start_energy: 0.0,
            life_armor: 0.0,
            shield_armor: 0.0,
            hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.0,
            energy_regen_per_tick: 0.0,
            max_speed: 0.15,
            acceleration: 0.15,
            deceleration: 0.15,
            turn_rate_deg_per_tick: 180.0,
            collision_radius: 0.5,
            separation_radius: 0.5,
            push_priority: 10,
            sight_radius: 8.0,
            scan_range: 8.0,
            attack_target_priority: 20,
            primary_weapon: Either,
            acquire_tie_break: Closest,
            mineral_cost: 50,
            gas_cost: 0,
            supply_cost: 1,
            supply_provided: 0,
            build_time_ticks: 100,
            weapon: None,
            {class}
        )"#
    )
}

/// The shipped catalogs with `inserted` (scratch definitions, comma-terminated) placed first in
/// `units.ron`, loaded from a copy in a fresh directory named by `label`.
fn catalogs_with(label: &str, inserted: &str) -> Result<RuntimeCatalogs, Box<dyn Error>> {
    let directory = std::env::temp_dir().join(format!(
        "space-scuffle-collision-classes-{}-{label}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory)?;
    for file in FILES {
        std::fs::copy(catalog_directory().join(file), directory.join(file))?;
    }
    let units = std::fs::read_to_string(directory.join("units.ron"))?;
    let edited = units.replacen("    units: [\n", &format!("    units: [\n{inserted}\n"), 1);
    if edited == units {
        return Err("units.ron has no `units: [` line".into());
    }
    std::fs::write(directory.join("units.ron"), edited)?;
    let loaded = RuntimeCatalogs::load_from_directory(&directory);
    std::fs::remove_dir_all(&directory)?;
    Ok(loaded?)
}

/// A flat 64 × 64 world on the shipped catalogs plus the scratch bodies of `y` and `x` (in that
/// file order), under `pipeline`.
fn world(label: &str, pipeline: MovementPipeline) -> Result<SimWorld, Box<dyn Error>> {
    let inserted = format!(
        "{},\n{},",
        scratch_body(Y_BODY, 241, "Ground", Some("y")),
        scratch_body(X_BODY, 240, "Ground", Some("x")),
    );
    let catalogs = catalogs_with(label, &inserted)?;
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::with_catalogs(map, 0x00C0_11DE, catalogs.units, catalogs.upgrades)?;
    let rules = world.catalog.movement_rules();
    world
        .catalog
        .set_movement_rules(rules.with_contact(ContactRules {
            pipeline,
            ..rules.contact()
        })?);
    Ok(world)
}

/// The class the catalog gave the authored `name`.
fn class_named(catalog: &UnitCatalog, name: &str) -> Result<CollisionClass, Box<dyn Error>> {
    let position = catalog
        .collision_classes()
        .iter()
        .position(|held| held == name)
        .ok_or("the class is authored")?;
    Ok(CollisionClass(
        CollisionClass::BUILT_IN + u8::try_from(position)?,
    ))
}

/// Registers a clone of `base` under a free id, changed by `edit`.
fn register(
    world: &mut SimWorld,
    base: UnitTypeId,
    edit: impl FnOnce(&mut UnitArchetype),
) -> Result<UnitTypeId, Box<dyn Error>> {
    let mut arch = world.catalog.get(base)?.clone();
    let free = (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .ok_or("a free unit type id")?;
    arch.unit_type_id = free;
    arch.slug = format!("collision_class_test_clone_{}", free.0);
    arch.name.clone_from(&arch.slug);
    edit(&mut arch);
    world.catalog.register(arch);
    Ok(free)
}

/// The scratch `x` body's type and a ground twin of it (the same archetype, no class).
fn x_and_ground(world: &mut SimWorld) -> Result<(UnitTypeId, UnitTypeId), Box<dyn Error>> {
    let x = world.catalog.id_by_slug(X_BODY).ok_or("x body")?;
    let ground = register(world, x, |arch| arch.collision_class = None)?;
    Ok((x, ground))
}

/// A shipped melee weapon without splash that fires only at rest (found by property), its
/// random delay removed.
fn melee_weapon(world: &SimWorld) -> Result<WeaponSpec, Box<dyn Error>> {
    let mut weapon = world
        .catalog
        .archetypes()
        .filter_map(|arch| arch.ground_weapon.as_ref())
        .find(|w| {
            w.melee
                && w.splash.is_none()
                && w.min_range == SimScalar::ZERO
                && w.allowed_movement == sim_types::AllowedMovement::Stopped
        })
        .ok_or("a shipped melee weapon")?
        .clone();
    weapon.random_delay_min = SimScalar::ZERO;
    weapon.random_delay_max = SimScalar::ZERO;
    Ok(weapon)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn spawn(world: &mut SimWorld, ty: UnitTypeId, player: u8, p: (f64, f64)) -> EntityId {
    world
        .spawn_unit(ty, PlayerId(player), at(p.0, p.1))
        .expect("invariant: the scene's points are inside the flat map")
}

fn command(world: &SimWorld, unit: EntityId, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: world.entities[&unit].owner,
        entities: vec![unit],
        order,
        queue: false,
    }
}

/// Steps `ticks` ticks, the `commands` on the first, and returns each tick's positions of
/// `watched`. Fails on any diagnostic.
fn trajectories(
    world: &mut SimWorld,
    commands: Vec<PlayerCommand>,
    watched: &[EntityId],
    ticks: u32,
) -> Result<Vec<Vec<SimVec2>>, Box<dyn Error>> {
    let mut out = Vec::new();
    let mut commands = Some(commands);
    for tick in 0..ticks {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: commands.take().unwrap_or_default(),
            state_checksum: None,
        });
        if !world.diagnostics().is_empty() {
            return Err(format!("tick {tick}: {:?}", world.diagnostics()).into());
        }
        out.push(
            watched
                .iter()
                .map(|id| world.entities[id].position)
                .collect(),
        );
    }
    Ok(out)
}

fn column(rows: &[Vec<SimVec2>], index: usize) -> Vec<SimVec2> {
    rows.iter().map(|row| row[index]).collect()
}

fn move_to(world: &SimWorld, unit: EntityId, p: (f64, f64)) -> PlayerCommand {
    command(
        world,
        unit,
        UnitOrder::Move {
            target: at(p.0, p.1),
        },
    )
}

const PIPELINES: [MovementPipeline; 2] = [MovementPipeline::Solver, MovementPipeline::Legacy];

/// A-1: every shipped archetype and mode that authors no class resolves to the class the
/// movement code had before collision classes: burrowed 2, else the layer's (ground 0, air 1).
#[test]
fn a1_shipped_archetypes_keep_their_built_in_classes() -> TestResult {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 7);
    let mobile: Vec<UnitArchetype> = world
        .catalog
        .archetypes()
        .filter(|arch| !arch.is_immobile_by_default() && arch.collision_class.is_none())
        .cloned()
        .collect();
    assert!(
        mobile.len() > 20,
        "the shipped catalog has its mobile units"
    );
    let mut burrowed_seen = false;
    for arch in &mobile {
        let id = spawn(&mut world, arch.unit_type_id, 0, (20.0, 20.0));
        let stances = std::iter::once(None).chain(
            arch.modes
                .iter()
                .filter(|mode| mode.collision_class.is_none())
                .map(|mode| Some(mode.mode_name.clone())),
        );
        for stance in stances {
            world.entities.get_mut(&id).ok_or("spawned")?.active_mode = stance.clone();
            let entity = &world.entities[&id];
            let old = if entity.is_burrowed(arch) {
                burrowed_seen = true;
                2
            } else {
                u8::from(arch.mover_type == sim_types::MoverType::Air)
            };
            assert_eq!(
                world.collision_class(id),
                Some(CollisionClass(old)),
                "{} in {stance:?}",
                arch.slug
            );
        }
    }
    assert!(burrowed_seen, "a shipped mode is burrowed (B-93)");
    Ok(())
}

/// A-2: the active mode's authored class wins, then burrowed, then the archetype's class; a unit
/// still channelling into a mode keeps its old stance's class.
#[test]
fn a2_classes_resolve_mode_then_burrowed_then_archetype() -> TestResult {
    let mut world = world("a2", MovementPipeline::Solver)?;
    let (x, y) = (
        class_named(&world.catalog, "x")?,
        class_named(&world.catalog, "y")?,
    );
    let base = world.catalog.id_by_slug(X_BODY).ok_or("x body")?;
    let template = melee_free_mode();
    let moder = register(&mut world, base, |arch| {
        arch.modes = vec![
            UnitModeSpec {
                mode_name: "lift".into(),
                collision_class: Some(y),
                ..template.clone()
            },
            UnitModeSpec {
                mode_name: "dig".into(),
                burrowed: true,
                ..template.clone()
            },
            UnitModeSpec {
                mode_name: "plain".into(),
                ..template.clone()
            },
        ];
    })?;
    let id = spawn(&mut world, moder, 0, (20.0, 20.0));
    let class_in = |world: &mut SimWorld, mode: Option<&str>| {
        world.entities.get_mut(&id).expect("spawned").active_mode = mode.map(str::to_owned);
        world.collision_class(id)
    };
    assert_eq!(class_in(&mut world, None), Some(x), "default stance");
    assert_eq!(class_in(&mut world, Some("lift")), Some(y), "mode class");
    assert_eq!(
        class_in(&mut world, Some("dig")),
        Some(CollisionClass::BURROWED),
        "burrowed mode"
    );
    assert_eq!(
        class_in(&mut world, Some("plain")),
        Some(x),
        "archetype class"
    );
    // Channelling into `lift` (mode index 0): the switch has not settled, so the default
    // stance's class holds until it does.
    let entity = world.entities.get_mut(&id).ok_or("spawned")?;
    entity.active_mode = None;
    entity.channeling_remaining_ticks = 10;
    entity.pending_ability = Some((0, None, None));
    assert_eq!(
        world.collision_class(id),
        Some(x),
        "channelling keeps the old class"
    );
    Ok(())
}

/// A mode that changes nothing but its name and class, for [`a2_classes_resolve_mode_then_burrowed_then_archetype`].
fn melee_free_mode() -> UnitModeSpec {
    UnitModeSpec {
        mode_name: String::new(),
        cast_ticks: 10,
        leave_cast_ticks: 10,
        cast_random_delay_max_ticks: SimScalar::ZERO,
        leave_random_delay_max_ticks: SimScalar::ZERO,
        is_immobile: false,
        unpushable: false,
        is_cloaked: false,
        burrowed: false,
        collision_class: None,
        mover_type_override: None,
        sight_radius_override: None,
        ground_weapon_override: None,
        air_weapon_override: None,
        energy_cost_per_second: SimScalar::ZERO,
        orders: sim_types::StanceOrders::ALL,
    }
}

/// A-3: names are checked at load, a name spans one movement layer, and indices follow the
/// sorted names whatever the file order.
#[test]
fn a3_authored_classes_are_validated_and_sorted() -> TestResult {
    let factions = sim_fixtures::shipped_factions();
    let load = |units: &[String]| {
        UnitCatalog::from_ron_str(&factions, &format!("(units: [{}])", units.join(",\n")))
    };
    for bad in ["", "Tall", "tall walker", "tall-walker"] {
        let result = load(&[scratch_body("a", 1, "Ground", Some(bad))]);
        assert!(
            matches!(result, Err(SimCoreError::CatalogCollisionClassName { ref name, .. }) if name == bad),
            "{bad:?}: {result:?}"
        );
    }
    let spans = load(&[
        scratch_body("a", 1, "Ground", Some("shared")),
        scratch_body("b", 2, "Air", Some("shared")),
    ]);
    assert!(
        matches!(spans, Err(SimCoreError::CatalogCollisionClassLayers { ref name, .. }) if name == "shared"),
        "{spans:?}"
    );
    let catalog = load(&[
        scratch_body("first", 1, "Ground", Some("zeta")),
        scratch_body("second", 2, "Ground", Some("alpha")),
        scratch_body("third", 3, "Air", Some("ground")),
        scratch_body("fourth", 4, "Ground", None),
    ])?;
    assert_eq!(catalog.collision_classes(), ["alpha", "ground", "zeta"]);
    assert_eq!(catalog.collision_class_count(), 6);
    let class_of = |slug: &str| {
        catalog
            .get(catalog.id_by_slug(slug).expect("loaded"))
            .expect("loaded")
            .collision_class
    };
    assert_eq!(class_of("second"), Some(CollisionClass(3)));
    assert_eq!(
        class_of("third"),
        Some(CollisionClass(4)),
        "an authored `ground` is a class of its own"
    );
    assert_eq!(class_of("first"), Some(CollisionClass(5)));
    assert_eq!(class_of("fourth"), None);
    Ok(())
}

/// The scenes of A-4: `x` and its ground twin, idle and overlapping by half their radius sum
/// (`walk` false) or walking through each other (`walk` true); `apart` puts the other body far
/// away. Returns the trajectories of the `x` body and the ground body.
fn a4_scene(
    pipeline: MovementPipeline,
    walk: bool,
    apart: (bool, bool),
) -> Result<(Vec<SimVec2>, Vec<SimVec2>), Box<dyn Error>> {
    let mut world = world(&format!("a4-{pipeline:?}-{walk}-{apart:?}"), pipeline)?;
    let (x, ground) = x_and_ground(&mut world)?;
    let far = (50.0, 50.0);
    let (x_at, g_at) = if walk {
        ((10.0, 20.0), (30.0, 20.25))
    } else {
        ((20.0, 20.0), (20.5, 20.0))
    };
    let a = spawn(&mut world, x, 0, if apart.0 { far } else { x_at });
    let b = spawn(&mut world, ground, 1, if apart.1 { far } else { g_at });
    let commands = if walk {
        vec![
            move_to(&world, a, (30.0, 20.0)),
            move_to(&world, b, (10.0, 20.25)),
        ]
    } else {
        Vec::new()
    };
    let rows = trajectories(&mut world, commands, &[a, b], 200)?;
    Ok((column(&rows, 0), column(&rows, 1)))
}

/// A-4: a body of class `x` and a ground body never touch, under both pipelines, idle or walking
/// through each other: each one's positions equal its positions with the other far away.
#[test]
fn a4_bodies_of_different_classes_never_touch() -> TestResult {
    for pipeline in PIPELINES {
        for walk in [false, true] {
            let (x_together, g_together) = a4_scene(pipeline, walk, (false, false))?;
            let (x_alone, _) = a4_scene(pipeline, walk, (false, true))?;
            let (_, g_alone) = a4_scene(pipeline, walk, (true, false))?;
            assert_eq!(x_together, x_alone, "{pipeline:?} walk {walk}: the x body");
            assert_eq!(
                g_together, g_alone,
                "{pipeline:?} walk {walk}: the ground body"
            );
        }
    }
    Ok(())
}

/// The scenes of A-5: two bodies of `class_x` (else two ground twins), owned by players 0 and
/// `second_owner`: idle and overlapping, one holding while the other walks into it, or both
/// walking through each other. Returns both trajectories.
fn a5_scene(
    pipeline: MovementPipeline,
    class_x: bool,
    second_owner: u8,
    scene: &str,
) -> Result<Vec<Vec<SimVec2>>, Box<dyn Error>> {
    let mut world = world(
        &format!("a5-{pipeline:?}-{class_x}-{second_owner}-{scene}"),
        pipeline,
    )?;
    let (x, ground) = x_and_ground(&mut world)?;
    let ty = if class_x { x } else { ground };
    let (a_at, b_at) = match scene {
        "idle" => ((20.0, 20.0), (20.5, 20.0)),
        "hold" => ((20.0, 20.0), (15.0, 20.1)),
        _ => ((15.0, 20.0), (25.0, 20.2)),
    };
    let a = spawn(&mut world, ty, 0, a_at);
    let b = spawn(&mut world, ty, second_owner, b_at);
    let commands = match scene {
        "idle" => Vec::new(),
        "hold" => vec![
            command(&world, a, UnitOrder::HoldPosition),
            move_to(&world, b, (25.0, 20.1)),
        ],
        _ => vec![
            move_to(&world, a, (25.0, 20.0)),
            move_to(&world, b, (15.0, 20.2)),
        ],
    };
    trajectories(&mut world, commands, &[a, b], 50)
}

/// A-5: two bodies of class `x` are separated exactly as two ground bodies of the same radii and
/// priorities, friend and enemy, idle, holding and walking, under both pipelines.
#[test]
fn a5_contact_within_an_authored_class_equals_ground_contact() -> TestResult {
    for pipeline in PIPELINES {
        for second_owner in [0, 1] {
            for scene in ["idle", "hold", "walk"] {
                let authored = a5_scene(pipeline, true, second_owner, scene)?;
                let ground = a5_scene(pipeline, false, second_owner, scene)?;
                assert_eq!(
                    authored, ground,
                    "{pipeline:?} owner {second_owner} {scene}"
                );
                if scene == "idle" {
                    let last = &authored[authored.len() - 1];
                    assert_ne!(
                        last,
                        &vec![at(20.0, 20.0), at(20.5, 20.0)],
                        "{pipeline:?} owner {second_owner}: they separate"
                    );
                }
            }
        }
    }
    Ok(())
}

/// The scene of A-6 under the solver: a melee chaser of class `chaser_x` (else ground) attacks an
/// unarmed ground target; a friend of the chaser, of class `blocker_x` (else ground), stands on
/// the chaser's bearing on the ring, or far away when `blocker` is false. Returns the chaser's
/// trajectory.
fn a6_scene(
    chaser_x: bool,
    blocker_x: bool,
    blocker: bool,
) -> Result<Vec<SimVec2>, Box<dyn Error>> {
    let mut world = world(
        &format!("a6-{chaser_x}-{blocker_x}-{blocker}"),
        MovementPipeline::Solver,
    )?;
    let (x, ground) = x_and_ground(&mut world)?;
    let weapon = melee_weapon(&world)?;
    let chaser_type = register(&mut world, if chaser_x { x } else { ground }, |arch| {
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon);
    })?;
    let target = spawn(&mut world, ground, 1, (20.0, 20.0));
    let chaser = spawn(&mut world, chaser_type, 0, (14.0, 20.0));
    // The ring lies at the target's radius plus the chaser's plus the weapon's reach; the
    // blocker stands on it, on the chaser's bearing.
    let blocker_at = if blocker { (18.85, 20.0) } else { (50.0, 50.0) };
    spawn(
        &mut world,
        if blocker_x { x } else { ground },
        0,
        blocker_at,
    );
    let order = command(
        &world,
        chaser,
        UnitOrder::AttackTarget {
            target,
            origin: sim_types::AttackOrigin::Explicit,
        },
    );
    let rows = trajectories(&mut world, vec![order], &[chaser], 80)?;
    Ok(column(&rows, 0))
}

/// A-6: a chaser's attack ring is blocked by bodies of its class only.
#[test]
fn a6_rings_are_blocked_only_by_the_chasers_class() -> TestResult {
    for chaser_x in [false, true] {
        let free = a6_scene(chaser_x, chaser_x, false)?;
        let foreign = a6_scene(chaser_x, !chaser_x, true)?;
        let own = a6_scene(chaser_x, chaser_x, true)?;
        assert_eq!(
            foreign, free,
            "chaser x {chaser_x}: a body of another class does not block the ring"
        );
        assert_ne!(
            own, free,
            "chaser x {chaser_x}: a body of its own class does"
        );
    }
    Ok(())
}

/// A-7: a displacement point on a body of class `x` is admitted for a ground unit and refused for
/// an `x` unit, and the converse for a point on a ground body.
#[test]
fn a7_displacement_admission_reads_the_class() -> TestResult {
    let mut world = world("a7", MovementPipeline::Solver)?;
    let (x, ground) = x_and_ground(&mut world)?;
    let x_body = spawn(&mut world, x, 1, (20.0, 20.0));
    let ground_body = spawn(&mut world, ground, 1, (30.0, 20.0));
    let x_mover = spawn(&mut world, x, 0, (10.0, 40.0));
    let ground_mover = spawn(&mut world, ground, 0, (20.0, 40.0));
    let on_x = at(20.3, 20.0);
    let on_ground = at(30.3, 20.0);
    assert_eq!(
        world.validate_displacement_destination(ground_mover, on_x),
        Ok(())
    );
    assert_eq!(
        world.validate_displacement_destination(x_mover, on_x),
        Err(DisplacementAdmissionError::Occupied(x_body))
    );
    assert_eq!(
        world.validate_displacement_destination(x_mover, on_ground),
        Ok(())
    );
    assert_eq!(
        world.validate_displacement_destination(ground_mover, on_ground),
        Err(DisplacementAdmissionError::Occupied(ground_body))
    );
    Ok(())
}

/// The scene of A-8's melee half: a ground melee unit `offset` west of an unarmed `x` enemy's
/// centre attacks it. Returns the ticks on which the enemy lost life.
fn a8_melee_scene(offset: f64) -> Result<Vec<u32>, Box<dyn Error>> {
    let mut world = world(&format!("a8-melee-{offset}"), MovementPipeline::Solver)?;
    let (x, ground) = x_and_ground(&mut world)?;
    let weapon = melee_weapon(&world)?;
    let melee = register(&mut world, ground, |arch| {
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon);
    })?;
    let enemy = spawn(&mut world, x, 1, (20.0, 20.0));
    let attacker = spawn(&mut world, melee, 0, (20.0 - offset, 20.0));
    let order = command(
        &world,
        attacker,
        UnitOrder::AttackTarget {
            target: enemy,
            origin: sim_types::AttackOrigin::Explicit,
        },
    );
    let mut hits = Vec::new();
    let mut life = world.entities[&enemy].hp;
    let mut commands = Some(vec![order]);
    for tick in 0..120 {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: commands.take().unwrap_or_default(),
            state_checksum: None,
        });
        assert!(
            world.diagnostics().is_empty(),
            "offset {offset}, tick {tick}: {:?}",
            world.diagnostics()
        );
        let now = world.entities[&enemy].hp;
        if now < life {
            hits.push(tick);
        }
        life = now;
    }
    Ok(hits)
}

/// A-8, melee: a melee unit at, and a quarter radius from, the centre of an enemy of another
/// class strikes on the same ticks as one standing at the rim, with no diagnostic.
#[test]
fn a8_a_melee_unit_beneath_another_class_swings_as_at_the_rim() -> TestResult {
    // The rim: centres the radius sum (0.5 + 0.5) plus half the weapon's reach apart.
    let rim = a8_melee_scene(1.05)?;
    assert!(rim.len() >= 3, "the rim attacker strikes: {rim:?}");
    for offset in [0.0, 0.125] {
        assert_eq!(a8_melee_scene(offset)?, rim, "offset {offset}");
    }
    Ok(())
}

/// A-8, line splash: an `x` attacker whose line splash lies across the attack fires at an enemy
/// at its own centre: the line lies across the attacker's heading (+x), so a bystander a radius
/// sum to the side (+y) is struck and one a radius sum ahead (+x) is not.
#[test]
fn a8_a_line_splash_at_a_coincident_target_lies_across_the_heading() -> TestResult {
    let mut world = world("a8-line", MovementPipeline::Solver)?;
    let (x, ground) = x_and_ground(&mut world)?;
    let mut lance = world
        .catalog
        .archetypes()
        .filter_map(|arch| arch.ground_weapon.as_ref())
        .find(|w| {
            w.splash.as_ref().is_some_and(|s| {
                matches!(&s.shape, SplashShape::Line(line) if line.axis == SplashLineAxis::Across)
            })
        })
        .ok_or("a shipped line splash across the attack")?
        .clone();
    lance.random_delay_min = SimScalar::ZERO;
    lance.random_delay_max = SimScalar::ZERO;
    lance.fires_at_other_targets = false;
    lance.fires_outside_attack_order = false;
    let attacker_type = register(&mut world, x, |arch| {
        arch.weapon = Some(lance.clone());
        arch.ground_weapon = Some(lance);
    })?;
    let attacker = spawn(&mut world, attacker_type, 0, (20.0, 20.0));
    let victim = spawn(&mut world, ground, 1, (20.0, 20.0));
    let side = spawn(&mut world, ground, 1, (20.0, 21.0));
    let ahead = spawn(&mut world, ground, 1, (21.0, 20.0));
    let full = world.entities[&side].hp;
    let order = command(
        &world,
        attacker,
        UnitOrder::AttackTarget {
            target: victim,
            origin: sim_types::AttackOrigin::Explicit,
        },
    );
    trajectories(&mut world, vec![order], &[attacker], 60)?;
    assert!(
        world.entities[&victim].hp < full,
        "the coincident target is struck"
    );
    assert!(
        world.entities[&side].hp < full,
        "the line lies across the heading"
    );
    assert_eq!(world.entities[&ahead].hp, full, "and not along it");
    Ok(())
}

/// A flat 64 × 64 world on the shipped catalogs under `pipeline`, and the shipped archetype that
/// authors a collision class (found by property), cloned without weapons and with ample life.
fn shipped_with_authoring(
    pipeline: MovementPipeline,
) -> Result<(SimWorld, UnitTypeId), Box<dyn Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 0x00C0_1055);
    let rules = world.catalog.movement_rules();
    world
        .catalog
        .set_movement_rules(rules.with_contact(ContactRules {
            pipeline,
            ..rules.contact()
        })?);
    let authoring = world
        .catalog
        .archetypes()
        .find(|arch| !arch.is_immobile_by_default() && arch.collision_class.is_some())
        .map(|arch| arch.unit_type_id)
        .ok_or("a shipped archetype authors a collision class")?;
    let walker = register(&mut world, authoring, |arch| {
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.max_hp = SimScalar::from_num(100_000);
    })?;
    Ok((world, walker))
}

/// A shipped mobile ground archetype that authors no class (found by property), unarmed, of
/// collision radius at most 0.5.
fn plain_ground(world: &mut SimWorld) -> Result<UnitTypeId, Box<dyn Error>> {
    let base = world
        .catalog
        .archetypes()
        .find(|arch| {
            !arch.is_immobile_by_default()
                && arch.mover_type == sim_types::MoverType::Ground
                && arch.collision_class.is_none()
                && arch.modes.is_empty()
                && arch.collision_radius <= SimScalar::HALF
        })
        .map(|arch| arch.unit_type_id)
        .ok_or("a shipped small ground unit")?;
    register(world, base, |arch| {
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
    })
}

/// A-11's walk: the authoring archetype walks from (10, 32) to (40, 32) through a packed 6 × 6
/// block of idle ground units of `owner` centred on (25, 32), or placed far away when `block`
/// is false. Returns the tick it arrives (its order done) and whether any block member moved.
fn a11_walk(
    pipeline: MovementPipeline,
    owner: u8,
    block: bool,
) -> Result<(u32, bool), Box<dyn Error>> {
    let (mut world, walker) = shipped_with_authoring(pipeline)?;
    let member = plain_ground(&mut world)?;
    let spacing = 2.0 * f64::from(world.catalog.get(member)?.collision_radius.to_f32());
    let centre = if block { (25.0, 32.0) } else { (50.0, 55.0) };
    let mut members = Vec::new();
    for row in 0..6 {
        for col in 0..6 {
            let x = centre.0 + (f64::from(col) - 2.5) * spacing;
            let y = centre.1 + (f64::from(row) - 2.5) * spacing;
            members.push(spawn(&mut world, member, owner, (x, y)));
        }
    }
    let start: Vec<SimVec2> = members.iter().map(|m| world.entities[m].position).collect();
    let id = spawn(&mut world, walker, 0, (10.0, 32.0));
    let mut commands = Some(vec![move_to(&world, id, (40.0, 32.0))]);
    for tick in 0..600 {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: commands.take().unwrap_or_default(),
            state_checksum: None,
        });
        if !world.diagnostics().is_empty() {
            return Err(format!("tick {tick}: {:?}", world.diagnostics()).into());
        }
        if world.entities[&id].order_queue.is_empty() {
            let moved = members
                .iter()
                .zip(&start)
                .any(|(m, s)| world.entities[m].position != *s);
            return Ok((tick, moved));
        }
    }
    Err("the walker never arrived".into())
}

/// A-11 (WP3): the archetype that authors a collision class walks through a packed block of
/// idle ground units, friendly then enemy, arriving on the tick a walk past no block arrives,
/// and no block member moves, under both pipelines.
#[test]
fn a11_the_authoring_archetype_walks_over_a_crowd() -> TestResult {
    for pipeline in PIPELINES {
        let (solo, _) = a11_walk(pipeline, 0, false)?;
        for owner in [0, 1] {
            let (through, moved) = a11_walk(pipeline, owner, true)?;
            assert_eq!(through, solo, "{pipeline:?} owner {owner}: arrival tick");
            assert!(!moved, "{pipeline:?} owner {owner}: no block member moves");
        }
    }
    Ok(())
}

/// A-12 (WP3): twenty shipped melee units (found by property) attack a lone enemy of the
/// authoring archetype, unarmed and holding position (an unarmed unit is no automatic target, so
/// the order names it): none is held off by its body, every one that reaches its band
/// swings, every other one walks or stands behind friends of its class (the target's body holds
/// none), and
/// no tick reports a diagnostic, however deep the overlap.
#[test]
fn a12_a_crowd_attacks_from_beneath() -> TestResult {
    for pipeline in PIPELINES {
        let (mut world, walker) = shipped_with_authoring(pipeline)?;
        let (melee, weapon) = world
            .catalog
            .archetypes()
            .filter(|arch| !arch.is_immobile_by_default() && arch.collision_class.is_none())
            .find_map(|arch| {
                let w = arch.ground_weapon.as_ref()?;
                (w.melee
                    && w.splash.is_none()
                    && w.min_range == SimScalar::ZERO
                    && w.allowed_movement == sim_types::AllowedMovement::Stopped)
                    .then(|| (arch.unit_type_id, w.clone()))
            })
            .ok_or("a shipped melee unit")?;
        let target = spawn(&mut world, walker, 1, (30.0, 32.0));
        let mut crowd = Vec::new();
        for k in 0..20 {
            let (row, col) = (f64::from(k / 5), f64::from(k % 5));
            crowd.push(spawn(&mut world, melee, 0, (20.0 + row, 30.0 + col)));
        }
        let order = PlayerCommand {
            player_id: PlayerId(0),
            entities: crowd.clone(),
            order: UnitOrder::AttackTarget {
                target,
                origin: sim_types::AttackOrigin::Explicit,
            },
            queue: false,
        };
        let hold = command(&world, target, UnitOrder::HoldPosition);
        let band = |world: &SimWorld, id: EntityId| {
            let unit = &world
                .catalog
                .get(world.entities[&id].unit_type_id)
                .expect("loaded");
            let tall = &world.catalog.get(walker).expect("loaded");
            weapon.max_range + unit.collision_radius + tall.collision_radius
        };
        let mut reached = vec![None; crowd.len()];
        let mut swung = vec![false; crowd.len()];
        let mut commands = Some(vec![order, hold]);
        for tick in 0..400 {
            world.step(&TickBundle {
                tick: Tick(tick),
                commands: commands.take().unwrap_or_default(),
                state_checksum: None,
            });
            assert!(
                world.diagnostics().is_empty(),
                "{pipeline:?} tick {tick}: {:?}",
                world.diagnostics()
            );
            let centre = world.entities[&target].position;
            for (k, id) in crowd.iter().enumerate() {
                let unit = &world.entities[id];
                if reached[k].is_none() && unit.position.distance(centre) <= band(&world, *id) {
                    reached[k] = Some(tick);
                }
                swung[k] |= unit.weapon_cooldown_remaining > 0;
            }
        }
        // A unit that never reached its band still walks (the legacy surround walk) or stands
        // behind friends of its class, never against the target's body: it touches a friend.
        let r_unit = f64::from(world.catalog.get(melee)?.collision_radius.to_f32());
        for (k, id) in crowd.iter().enumerate() {
            if reached[k].is_some() {
                continue;
            }
            let here = world.entities[id].position;
            let behind_friend = crowd.iter().any(|other| {
                other != id
                    && f64::from(world.entities[other].position.distance(here).to_f32())
                        <= 2.0 * r_unit + 0.05
            });
            let walking = world.entities[id].velocity != SimVec2::ZERO;
            assert!(
                behind_friend || walking,
                "{pipeline:?}: unit {k} stands short of its band by no friend"
            );
        }
        for (k, at_tick) in reached.iter().enumerate() {
            if let Some(at_tick) = at_tick {
                assert!(
                    swung[k],
                    "{pipeline:?}: unit {k} reached its band at {at_tick} and never swung"
                );
            }
        }
    }
    Ok(())
}
