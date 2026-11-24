//! Detection is a plain circle (owner answer Q0101 2(b), bug B-201), and a detector detects only
//! once its construction completes (owner answer Q0101 3(b), bug B-184).
//!
//! # Responsibility
//! Pins plan `2026-11-24-owner-answers-cliff-and-vision` P-10 and P-12 end to end, through
//! `SimWorld::step`: a detector detects every cell within its detection radius, up a cliff and
//! into a sight blocker, and its side may strike a concealed unit there wherever another
//! friendly unit gives vision; a detected cell that nobody sees gives nothing (detection is not
//! sight), and without a detection radius nothing is detected. A detector foundation built
//! through a `Build` command detects nothing until the tick its construction completes, and the
//! incremental vision maps then equal a full recount.
//!
//! # How it fits
//! `sim_map::PlayerVisionMap::cast` marks detection on the disc of the detection radius and
//! sight along the rays; the concealment test of `target_acquisition.rs` asks for both. This
//! file plays the rule through the public API with a shipped detector structure that hits air,
//! an unarmed air unit set cloaked (no shipped air unit carries a cloak ability) and an unarmed
//! air spotter, each found by property and copied.
//!
//! # Invariants
//! - No unit is named: archetypes are found by property (charter §3.3).
//! - Every distance the tests rely on is read from the catalog and asserted before use.

use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, PlayerSlot, PlayerView, ShapeKind, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId, VisibilityState,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The detector's player.
const BLUE: PlayerId = PlayerId(0);
/// The concealed unit's player.
const RED: PlayerId = PlayerId(1);
/// The first column across the cliff edge or inside the blocker.
const EDGE: u32 = 16;
/// The map's side, in cells.
const SIDE: u32 = 32;
/// The most ticks a test waits for a strike.
const WAIT: u32 = 120;

/// What separates the detector from the concealed unit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Terrain {
    /// The cells with x >= [`EDGE`] stand one cliff level up.
    Cliff,
    /// A flat map with a sight blocker over columns [`EDGE`] to `EDGE + 3`, every row.
    Blocker,
}

/// The test map of `terrain`.
fn map(terrain: Terrain) -> Result<GameMap, sim_map::MapError> {
    let mut map = GameMap::new(SIDE, SIDE, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..SIDE {
        for x in EDGE..SIDE {
            match terrain {
                Terrain::Cliff => map.cell_mut(x, y)?.cliff_level = CliffLevel(1),
                Terrain::Blocker if x < EDGE + 4 => {
                    map.cell_mut(x, y)?.flags.insert(CellFlags::LOS_BLOCKER);
                }
                Terrain::Blocker => {}
            }
        }
    }
    Ok(map)
}

/// The next free unit type id.
fn free_id(world: &SimWorld) -> UnitTypeId {
    UnitTypeId(
        world
            .catalog
            .archetypes()
            .map(|a| a.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    )
}

/// Registers `arch` changed by `edit` under the next free identity and slug `slug`.
fn register(
    world: &mut SimWorld,
    arch: &UnitArchetype,
    slug: &str,
    edit: impl FnOnce(&mut UnitArchetype),
) -> UnitTypeId {
    let id = free_id(world);
    let mut copy = arch.clone();
    copy.unit_type_id = id;
    slug.clone_into(&mut copy.slug);
    edit(&mut copy);
    world.catalog.register(copy);
    id
}

/// A shipped detector structure whose weapon hits air units.
fn detector_structure(world: &SimWorld) -> Option<UnitArchetype> {
    world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.is_detector()
                && matches!(arch.shape, ShapeKind::BuildingDome { .. })
                && arch.weapon_against(MoverType::Air, None).is_some()
        })
        .cloned()
}

/// A shipped mobile air unit that is not a detector.
fn air_unit(world: &SimWorld) -> Option<UnitArchetype> {
    world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.mover_type == MoverType::Air
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
        })
        .cloned()
}

/// Removes every weapon, detection and ability from `arch`.
fn disarm(arch: &mut UnitArchetype) {
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    arch.detection_radius = None;
    arch.abilities = Vec::new();
}

/// The bench: a blue detector structure west of [`EDGE`], a red cloaked air unit east of it
/// within the detector's detection radius and air weapon range, and optionally an unarmed blue
/// air spotter beside the cloaked unit.
struct Bench {
    world: SimWorld,
    detector: EntityId,
    cloaked: EntityId,
}

/// Builds the bench on `terrain`; `detects` keeps the detector's detection radius (else a copy
/// without it), `spotter` adds the blue spotter.
fn bench(
    terrain: Terrain,
    detects: bool,
    spotter: bool,
) -> Result<Bench, Box<dyn std::error::Error>> {
    let mut world = sim_fixtures::world(map(terrain)?, 0xD37);
    let detector_arch = detector_structure(&world).ok_or("a shipped detector that hits air")?;
    let ShapeKind::BuildingDome {
        footprint_w,
        footprint_h,
        ..
    } = detector_arch.shape
    else {
        return Err("a structure has a dome".into());
    };
    let detector_type = register(&mut world, &detector_arch, "scratch_detector", |arch| {
        if !detects {
            arch.detection_radius = None;
        }
    });
    let cell_y = SIDE / 2 - u32::from(footprint_h) / 2;
    let detector =
        world.spawn_structure(detector_type, BLUE, EDGE - u32::from(footprint_w), cell_y)?;
    let centre = world.entities[&detector].position;
    assert!(
        world.entities[&detector].is_completed,
        "a complete detector"
    );

    // No shipped air unit carries a cloak ability, so the concealed unit is a shipped air unit
    // whose entity is set cloaked, as `attack_reveal_tests.rs` does.
    let cloaker_arch = air_unit(&world).ok_or("a shipped air unit")?;
    let cloaked_type = register(&mut world, &cloaker_arch, "scratch_cloaked", disarm);
    let at = SimVec2::new(SimScalar::from_num(f64::from(EDGE) + 1.5), centre.y);
    let radius = detector_arch.detection_radius.ok_or("a detector")?;
    let weapon = detector_arch
        .weapon_against(MoverType::Air, None)
        .ok_or("hits air")?;
    let distance = centre.distance(at);
    assert!(
        distance < radius - SimScalar::ONE,
        "within the detection radius"
    );
    assert!(
        distance - cloaker_arch.collision_radius - detector_arch.collision_radius
            < weapon.max_range,
        "within the detector's air weapon range"
    );
    let cloaked = world.spawn_unit(cloaked_type, RED, at)?;
    world
        .entities
        .get_mut(&cloaked)
        .ok_or("spawned")?
        .is_cloaked = true;
    hold(&mut world, cloaked)?;

    if spotter {
        let spotter_arch = air_unit(&world).ok_or("a shipped air unit")?;
        let spotter_type = register(&mut world, &spotter_arch, "scratch_spotter", disarm);
        let spot = SimVec2::new(at.x + SimScalar::ONE, at.y + SimScalar::ONE);
        let id = world.spawn_unit(spotter_type, BLUE, spot)?;
        hold(&mut world, id)?;
    }
    Ok(Bench {
        world,
        detector,
        cloaked,
    })
}

/// Orders `id` to hold its position.
fn hold(world: &mut SimWorld, id: EntityId) -> TestResult {
    world
        .entities
        .get_mut(&id)
        .ok_or("spawned")?
        .issue_order(UnitOrder::HoldPosition, false)
        .map_err(|error| format!("{error:?}"))?;
    Ok(())
}

impl Bench {
    /// Hull and shields of the cloaked unit, or zero once it is gone.
    fn vitals(&self) -> SimScalar {
        self.world
            .entities
            .get(&self.cloaked)
            .map_or(SimScalar::ZERO, |e| e.hp + e.shields)
    }

    /// Plays up to [`WAIT`] ticks; returns whether the cloaked unit lost vitals.
    fn struck(&mut self) -> bool {
        let full = self.vitals();
        for tick in 0..WAIT {
            self.world.step(&TickBundle {
                tick: Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            });
            if self.vitals() < full {
                return true;
            }
        }
        false
    }

    /// Whether blue's vision map detects the cloaked unit's cell, and whether blue sees it.
    fn detected_and_seen(&self) -> Result<(bool, bool), Box<dyn std::error::Error>> {
        let position = self.world.entities[&self.cloaked].position;
        let (x, y) = self.world.map.world_to_cell(position)?;
        let vision = &self.world.vision_maps[PlayerSlot::ALL[0]];
        Ok((
            vision.is_detected(x, y),
            vision.visibility_at(x, y) == VisibilityState::Visible,
        ))
    }

    /// Whether blue's view lists the cloaked unit as revealed.
    fn listed(&self) -> bool {
        let mut view = PlayerView::default();
        self.world.write_player_view(BLUE, &mut view);
        view.revealed.contains(&self.cloaked)
    }
}

/// P-10 (Q0101 2(b)): a detector at the foot of a cliff detects a cloaked air unit over the
/// plateau, which it cannot see; with a spotter of its player seeing that cell, the detector's
/// side strikes the cloaked unit and blue's view lists it as revealed.
#[test]
fn a_detector_below_a_cliff_detects_what_a_spotter_sees() -> TestResult {
    let mut bench = bench(Terrain::Cliff, true, true)?;
    assert!(bench.struck(), "the detector strikes the cloaked unit");
    assert_eq!(bench.detected_and_seen()?, (true, true));
    assert!(bench.listed(), "listed as revealed");
    assert!(bench.world.entities.contains_key(&bench.detector));
    Ok(())
}

/// P-10, rule 6: without the spotter the cloaked unit is detected but nobody sees its cell, so
/// it is no target and blue's view does not list it.
#[test]
fn a_detected_unit_nobody_sees_is_no_target() -> TestResult {
    let mut bench = bench(Terrain::Cliff, true, false)?;
    assert!(!bench.struck(), "never struck");
    assert_eq!(
        bench.detected_and_seen()?,
        (true, false),
        "detected, unseen"
    );
    assert!(!bench.listed(), "not listed as revealed");
    Ok(())
}

/// P-10, the control: a copy of the detector with no detection radius, with the spotter, never
/// strikes the cloaked unit, which is seen and not detected.
#[test]
fn a_detector_without_detection_cannot_strike_the_cloaked_unit() -> TestResult {
    let mut bench = bench(Terrain::Cliff, false, true)?;
    assert!(!bench.struck(), "never struck");
    assert_eq!(
        bench.detected_and_seen()?,
        (false, true),
        "seen, not detected"
    );
    assert!(!bench.listed());
    Ok(())
}

/// P-10 with a sight blocker: a detector outside the blocker detects a cloaked air unit over
/// it; with a spotter inside the blocker, its side strikes the unit; without one it is
/// detected, unseen and no target.
#[test]
fn a_detector_outside_a_blocker_detects_inside_it() -> TestResult {
    let mut spotted = bench(Terrain::Blocker, true, true)?;
    assert!(spotted.struck(), "the detector strikes the cloaked unit");
    assert_eq!(spotted.detected_and_seen()?, (true, true));

    let mut alone = bench(Terrain::Blocker, true, false)?;
    assert!(!alone.struck(), "never struck without a spotter");
    assert_eq!(
        alone.detected_and_seen()?,
        (true, false),
        "detected, unseen"
    );
    Ok(())
}

/// Plays one tick with `commands`.
fn tick(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
}

/// The construction bench: a blue detector foundation laid by a `Build` command on a flat map,
/// a red cloaked air unit three cells east of the site inside its detection radius, and an
/// unarmed blue air spotter over the cloaked unit.
struct Construction {
    world: SimWorld,
    foundation: EntityId,
    cloaked: EntityId,
    /// The cloaked unit's cell.
    cell: (u32, u32),
}

/// The first shipped detector structure for which the first builder of its faction is admitted
/// to build it on open ground, once the structures its own data requires stand: blue gets that
/// builder, those structures and the money, and the builder is ordered to build at `site`.
/// Returns the world after the foundation is laid, with the foundation's id.
fn lay_detector(
    site: (u32, u32),
) -> Result<(SimWorld, EntityId, UnitArchetype), Box<dyn std::error::Error>> {
    let candidates: Vec<UnitArchetype> = {
        let world = sim_fixtures::world(map_flat()?, 0xB18);
        world
            .catalog
            .archetypes()
            .filter(|arch| {
                arch.is_detector() && matches!(arch.shape, ShapeKind::BuildingDome { .. })
            })
            .cloned()
            .collect()
    };
    for detector in candidates {
        let mut world = sim_fixtures::world(map_flat()?, 0xB18);
        let Some(builder) = world
            .catalog
            .archetypes()
            .find(|arch| arch.capabilities.can_construct && arch.faction == detector.faction)
            .map(|arch| arch.unit_type_id)
        else {
            continue;
        };
        let mut corner = 1;
        for slug in &detector.requires_structures {
            let Some(required) = world.catalog.id_by_slug(slug) else {
                continue;
            };
            world.spawn_structure(required, BLUE, corner, 1)?;
            corner += 6;
        }
        let worker = world.spawn_unit(
            builder,
            BLUE,
            SimVec2::new(
                SimScalar::from_num(f64::from(site.0) - 2.5),
                SimScalar::from_num(f64::from(site.1) + 0.5),
            ),
        )?;
        world.player_resources[PlayerSlot::ALL[0]].minerals = 5000;
        world.player_resources[PlayerSlot::ALL[0]].gas = 5000;
        tick(
            &mut world,
            vec![PlayerCommand {
                player_id: BLUE,
                entities: vec![worker],
                order: UnitOrder::Build {
                    structure_type: detector.unit_type_id,
                    cell_x: site.0,
                    cell_y: site.1,
                },
                queue: false,
            }],
        );
        for _ in 0..600 {
            if let Some(foundation) = world
                .entities
                .values()
                .find(|e| e.unit_type_id == detector.unit_type_id)
                .map(|e| e.id)
            {
                return Ok((world, foundation, detector));
            }
            tick(&mut world, Vec::new());
        }
    }
    Err("no shipped detector structure could be built".into())
}

/// A flat [`SIDE`] × [`SIDE`] map.
fn map_flat() -> Result<GameMap, sim_map::MapError> {
    GameMap::new(SIDE, SIDE, MapCell::default_ground(CliffLevel(0)))
}

/// Builds the construction bench.
fn construction() -> Result<Construction, Box<dyn std::error::Error>> {
    let (mut world, foundation, detector) = lay_detector((14, 14))?;
    assert!(!world.entities[&foundation].is_completed, "a foundation");
    let centre = world.entities[&foundation].position;
    let at = SimVec2::new(centre.x + SimScalar::from_i32(3), centre.y);
    let radius = detector.detection_radius.ok_or("a detector")?;
    assert!(
        centre.distance(at) < radius - SimScalar::ONE,
        "within the radius"
    );
    let flyer = air_unit(&world).ok_or("a shipped air unit")?;
    let cloaked_type = register(&mut world, &flyer, "scratch_cloaked", disarm);
    let cloaked = world.spawn_unit(cloaked_type, RED, at)?;
    world
        .entities
        .get_mut(&cloaked)
        .ok_or("spawned")?
        .is_cloaked = true;
    hold(&mut world, cloaked)?;
    let spotter_type = register(&mut world, &flyer, "scratch_spotter", disarm);
    let spotter = world.spawn_unit(spotter_type, BLUE, at)?;
    hold(&mut world, spotter)?;
    let cell = world.map.world_to_cell(at)?;
    Ok(Construction {
        world,
        foundation,
        cloaked,
        cell,
    })
}

impl Construction {
    /// Whether blue detects the cloaked unit's cell, and whether it sees it.
    fn detected_and_seen(&self) -> (bool, bool) {
        let vision = &self.world.vision_maps[PlayerSlot::ALL[0]];
        (
            vision.is_detected(self.cell.0, self.cell.1),
            vision.visibility_at(self.cell.0, self.cell.1) == VisibilityState::Visible,
        )
    }

    /// Whether blue's view lists the cloaked unit as revealed.
    fn listed(&self) -> bool {
        let mut view = PlayerView::default();
        self.world.write_player_view(BLUE, &mut view);
        view.revealed.contains(&self.cloaked)
    }

    /// Plays until the foundation completes, checking every tick before that the cloaked unit
    /// is seen and not detected, not listed, and that the foundation sees its own cell.
    /// Returns the tick on which it completed.
    fn build_out(&mut self) -> Result<u32, Box<dyn std::error::Error>> {
        let full = self.world.entities[&self.cloaked].hp;
        let site = self
            .world
            .map
            .world_to_cell(self.world.entities[&self.foundation].position)?;
        for _ in 0..5000 {
            tick(&mut self.world, Vec::new());
            if self.world.entities[&self.foundation].is_completed {
                return Ok(self.world.current_tick);
            }
            assert_eq!(
                self.detected_and_seen(),
                (false, true),
                "a foundation detects nothing"
            );
            assert!(!self.listed(), "not revealed while it builds");
            assert_eq!(self.world.entities[&self.cloaked].hp, full, "not struck");
            assert_eq!(
                self.world.vision_maps[PlayerSlot::ALL[0]].visibility_at(site.0, site.1),
                VisibilityState::Visible,
                "the foundation sees as before"
            );
        }
        Err("the foundation never completed".into())
    }
}

/// P-12 (Q0101 3(b)): a detector foundation next to a cloaked enemy that a spotter of its
/// player sees detects nothing, reveals nothing and strikes nothing while it builds.
#[test]
fn a_detector_foundation_detects_nothing() -> TestResult {
    let mut bench = construction()?;
    assert_eq!(
        bench.detected_and_seen(),
        (false, true),
        "laid, not detecting"
    );
    bench.build_out()?;
    Ok(())
}

/// P-12: the detector detects from the tick its construction completes. Construction advances
/// in the production phase, before the vision phase of the same tick, so the vision update of
/// the completing tick already detects, and the view lists the cloaked unit.
#[test]
fn a_detector_detects_from_its_completion() -> TestResult {
    let mut bench = construction()?;
    bench.build_out()?;
    assert_eq!(
        bench.detected_and_seen(),
        (true, true),
        "detected on the completing tick"
    );
    assert!(bench.listed(), "revealed once complete");
    Ok(())
}

/// P-12: after completion the incrementally recast vision maps equal a full recount: a touched
/// map cell bumps the map's cell revision, which makes the next vision phase recount every
/// source, and with every unit holding still every counter and state is unchanged by it.
#[test]
fn a_completed_detector_recasts_to_a_full_recount() -> TestResult {
    let mut bench = construction()?;
    bench.build_out()?;
    tick(&mut bench.world, Vec::new());
    let incremental: Vec<_> = PlayerSlot::ALL
        .iter()
        .map(|&slot| bench.world.vision_maps[slot].clone())
        .collect();
    bench.world.map.cell_mut(0, 0)?;
    tick(&mut bench.world, Vec::new());
    for (slot, before) in PlayerSlot::ALL.iter().zip(&incremental) {
        let after = &bench.world.vision_maps[*slot];
        assert_eq!(
            after.detector_counter(),
            before.detector_counter(),
            "{slot:?}"
        );
        assert_eq!(
            after.visible_counter(),
            before.visible_counter(),
            "{slot:?}"
        );
        assert_eq!(after.states(), before.states(), "{slot:?}");
    }
    assert_eq!(bench.detected_and_seen(), (true, true));
    Ok(())
}
