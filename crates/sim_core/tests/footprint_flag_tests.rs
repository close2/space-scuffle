//! Footprint placement flags at admission, on arrival and in the preview (plan
//! `2026-11-19-footprint-flags` WP2, F-1 to F-8 and F-10).
//!
//! # Responsibility
//! Proves the rule of D-4: a structure whose catalog row refuses a placement flag cannot be
//! ordered, nor started, on a site with a cell that carries it. `NearResources` is applied by the
//! resource rings of `placement_rules.ron`; `Fogged` is every cell the placing player does not see
//! now. Rules that existed before keep their reasons and run first.
//!
//! # How it fits
//! Worlds are flat `sim_fixtures` maps with explicit nodes and the shipped catalogs. Which rows
//! check which flag is read from the catalog (`placement_excludes`), never named here; slugs
//! appear nowhere in this file.
//!
//! # Invariants
//! - Behaviour is reached through `PlayerCommand` and `preview_site`. Setup may place entities,
//!   set money, lift prerequisites (another rule's business) or move a node, never while a test
//!   measures.
//! - Cells are grid cells; a site is named by its minimum corner.

use sim_core::catalog::{RuntimeCatalogs, UnitArchetype};
use sim_core::economy::{GameCommandError, GameCommandReceipt, TaskEventKind};
use sim_core::production::construction::BuildRejection;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell, ResourceNode};
use sim_types::{
    EntityId, PlacementFlag, PlacementFlags, PlayerCommand, PlayerId, PlayerSlot, ResourceKind,
    ShapeKind, SimScalar, SimVec2, SiteRefusal, Tick, TickBundle, UnitOrder, UnitTypeId,
    VisibilityState,
};
use std::error::Error;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn Error>>;
type Admission = Result<GameCommandReceipt, GameCommandError>;

const ME: PlayerId = PlayerId(0);
const SIDE: u32 = 64;
const RICH: u32 = 1_000_000;
const MINERAL: (u32, u32) = (16, 32);
const GAS: (u32, u32) = (48, 32);
/// Ticks a builder beside its site may take to start the foundation.
const ARRIVAL_LIMIT: u32 = 400;

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
        remaining: 1_500,
        cell,
        placed: None,
    }
}

/// A flat `SIDE`-cell map holding `nodes`, built on `catalogs` with prerequisites lifted and
/// money that never decides.
fn world_on(
    catalogs: RuntimeCatalogs,
    nodes: Vec<ResourceNode>,
) -> Result<SimWorld, Box<dyn Error>> {
    let cells = vec![MapCell::default_ground(CliffLevel(0)); (SIDE * SIDE) as usize];
    let map = GameMap::from_cells(SIDE, SIDE, cells, nodes, Vec::new())?;
    let mut world = SimWorld::with_catalogs(map, 11, catalogs.units, catalogs.upgrades)?;
    let ids: Vec<UnitTypeId> = world.catalog.archetypes().map(|a| a.unit_type_id).collect();
    for id in ids {
        world.catalog.get_mut(id)?.requires_structures.clear();
    }
    world.player_resources[PlayerSlot::ALL[0]].minerals = RICH;
    world.player_resources[PlayerSlot::ALL[0]].gas = RICH;
    Ok(world)
}

/// The shipped world: a mineral node at `MINERAL` and a gas node at `GAS`.
fn world() -> Result<SimWorld, Box<dyn Error>> {
    world_on(
        sim_fixtures::shipped_catalogs(),
        vec![
            node(ResourceKind::Mineral, MINERAL),
            node(ResourceKind::Gas, GAS),
        ],
    )
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let bundle = TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    };
    world.step(&bundle);
}

fn footprint(arch: &UnitArchetype) -> (u32, u32) {
    match arch.shape {
        ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
            ..
        } => (u32::from(footprint_w), u32::from(footprint_h)),
        ShapeKind::UnitSphere => (0, 0),
    }
}

/// Every structure row whose `placement_excludes` holds `flag`.
fn checking(world: &SimWorld, flag: PlacementFlag) -> Vec<UnitTypeId> {
    world
        .catalog
        .archetypes()
        .filter(|arch| arch.placement_excludes.has(flag))
        .map(|arch| arch.unit_type_id)
        .collect()
}

/// A unit of `ME` that can raise `structure`, spawned at the centre of `cell`.
fn builder_for(
    world: &mut SimWorld,
    structure: UnitTypeId,
    cell: (u32, u32),
) -> Result<EntityId, Box<dyn Error>> {
    let target = world.catalog.get(structure)?.clone();
    let kind = world
        .catalog
        .archetypes()
        .find(|arch| arch.can_raise(&target))
        .ok_or("no builder for the structure")?
        .unit_type_id;
    Ok(world.spawn_unit(kind, ME, centre(cell))?)
}

/// Orders `builder` to raise `structure` at `site` and returns what admission said.
fn admit(
    world: &mut SimWorld,
    builder: EntityId,
    structure: UnitTypeId,
    site: (u32, u32),
) -> Result<Admission, Box<dyn Error>> {
    let order = UnitOrder::Build {
        structure_type: structure,
        cell_x: site.0,
        cell_y: site.1,
    };
    step(
        world,
        vec![PlayerCommand {
            player_id: ME,
            entities: vec![builder],
            order,
            queue: false,
        }],
    );
    let outcome = world.game_command_outcomes().first();
    Ok(outcome.ok_or("the order left no outcome")?.result.clone())
}

fn flagged(flag: PlacementFlag, cell: (u32, u32)) -> Admission {
    Err(GameCommandError::Build(BuildRejection::FlaggedSite {
        flag,
        cell,
    }))
}

/// The cell `offset` cells from `cell`.
fn offset(cell: (u32, u32), dx: i32, dy: i32) -> Result<(u32, u32), Box<dyn Error>> {
    Ok((
        cell.0.checked_add_signed(dx).ok_or("left of the map")?,
        cell.1.checked_add_signed(dy).ok_or("below the map")?,
    ))
}

/// F-1, re-derived for plan `2026-11-20-resource-footprints` WP2: the mineral ring is the game's
/// 8 × 7 rows round the 2×1 field (anchored on its west cell, the node's cell), 3 cells beyond it
/// on every side. Every town hall analogue is refused when its face is 2 cells east of the
/// field's east cell, at the first cell of its site in the ring, and admitted at a 3-cell gap;
/// on the west side the same, measured from the anchor.
#[test]
fn a_town_hall_is_refused_within_reach_of_minerals() -> TestResult {
    let mut world = world()?;
    let halls = checking(&world, PlacementFlag::NearResources);
    assert_eq!(
        halls.len(),
        7,
        "the seven drop-off rows check NearResources"
    );
    for hall in halls {
        let (width, height) = footprint(world.catalog.get(hall)?);
        let half = i32::try_from(height / 2)?;
        let builder = builder_for(&mut world, hall, offset(MINERAL, 0, -12)?)?;
        // Two empty cells between the field's east cell (anchor + 1) and the hall's left face.
        let near = offset(MINERAL, 4, -half)?;
        assert_eq!(
            admit(&mut world, builder, hall, near)?,
            flagged(PlacementFlag::NearResources, near),
            "a {width}x{height} hall two cells east of the field"
        );
        let clear = offset(MINERAL, 5, -half)?;
        assert!(admit(&mut world, builder, hall, clear)?.is_ok());
        // The same on the field's west side: the face is the hall's right column.
        let west = offset(MINERAL, -2 - i32::try_from(width)?, -half)?;
        let edge = offset(MINERAL, -3, -half)?;
        assert_eq!(
            admit(&mut world, builder, hall, west)?,
            flagged(PlacementFlag::NearResources, edge)
        );
        let west_clear = offset(MINERAL, -3 - i32::try_from(width)?, -half)?;
        assert!(admit(&mut world, builder, hall, west_clear)?.is_ok());
    }
    Ok(())
}

/// F-2, re-derived for plan `2026-11-20-resource-footprints` WP2: the mineral ring's corners are
/// cut (`.oooooo.`, liberty Footprint:1083), so a hall north-east of the field at (4, 3) from the
/// anchor stands, and ones at (4, 2) and (3, 3) do not.
#[test]
fn the_mineral_reach_has_its_corners_cut() -> TestResult {
    let mut world = world()?;
    let hall = checking(&world, PlacementFlag::NearResources)[0];
    let builder = builder_for(&mut world, hall, offset(MINERAL, -6, -6)?)?;
    let corner = offset(MINERAL, 4, 3)?;
    assert!(admit(&mut world, builder, hall, corner)?.is_ok());
    for closer in [offset(MINERAL, 4, 2)?, offset(MINERAL, 3, 3)?] {
        assert_eq!(
            admit(&mut world, builder, hall, closer)?,
            flagged(PlacementFlag::NearResources, closer)
        );
    }
    Ok(())
}

/// F-3: a one-cell row made to check `NearResources` is refused on exactly the 69 cells of the
/// void-rounded geyser ring; on the geyser's own 3×3 the node rule answers first.
#[test]
fn the_geyser_reach_is_the_void_ring() -> TestResult {
    let mut world = world()?;
    let probe = world
        .catalog
        .archetypes()
        .find(|arch| footprint(arch) == (1, 1))
        .ok_or("no one-cell structure")?
        .unit_type_id;
    world.catalog.get_mut(probe)?.placement_excludes = PlacementFlags::NEAR_RESOURCES;
    let builder = builder_for(&mut world, probe, offset(GAS, 0, -10)?)?;
    let mut refused = 0;
    for dy in -5..=5 {
        for dx in -5..=5 {
            let cell = offset(GAS, dx, dy)?;
            let outcome = admit(&mut world, builder, probe, cell)?;
            let (ax, ay) = (dx.abs(), dy.abs());
            let in_ring = ax <= 4 && ay <= 4 && ax + ay <= 6 && !(ax == 4 && ay == 3);
            // Re-derived for plan `2026-11-20-resource-footprints` WP5: the geyser stands on
            // its 3×3, so the node rule answers on all nine of its cells.
            if ax <= 1 && ay <= 1 {
                assert_eq!(
                    outcome,
                    Err(GameCommandError::Build(BuildRejection::CoversResource))
                );
                refused += 1;
            } else if in_ring {
                assert_eq!(
                    outcome,
                    flagged(PlacementFlag::NearResources, cell),
                    "({dx}, {dy})"
                );
                refused += 1;
            } else {
                assert!(outcome.is_ok(), "({dx}, {dy}): {outcome:?}");
            }
        }
    }
    assert_eq!(refused, 69);
    Ok(())
}

/// F-4: the node rule runs first, so a town hall on a node still says it covers a resource.
#[test]
fn a_town_hall_on_a_node_still_says_covers_resource() -> TestResult {
    let mut world = world()?;
    let hall = checking(&world, PlacementFlag::NearResources)[0];
    let builder = builder_for(&mut world, hall, offset(MINERAL, 0, -12)?)?;
    for site in [offset(MINERAL, -2, -2)?, offset(GAS, -1, -1)?] {
        assert_eq!(
            admit(&mut world, builder, hall, site)?,
            Err(GameCommandError::Build(BuildRejection::CoversResource))
        );
    }
    Ok(())
}

/// F-5: a row that checks no flag ignores the reach: a 3×3 production row beside a mineral node
/// and an extractor on its gas node are admitted.
#[test]
fn other_structures_ignore_the_reach() -> TestResult {
    let mut world = world()?;
    let production = world
        .catalog
        .archetypes()
        .find(|arch| {
            footprint(arch) == (3, 3)
                && !arch.trains_units.is_empty()
                && arch.placement_excludes.is_empty()
        })
        .ok_or("no 3x3 production row")?
        .unit_type_id;
    let extractor = world
        .catalog
        .archetypes()
        .find(|arch| arch.capabilities.is_gas_extractor)
        .ok_or("no extractor")?
        .unit_type_id;
    let builder = builder_for(&mut world, production, offset(MINERAL, 0, -10)?)?;
    assert!(admit(&mut world, builder, production, offset(MINERAL, 2, -1)?)?.is_ok());
    let gas_builder = builder_for(&mut world, extractor, offset(GAS, 0, -10)?)?;
    assert!(admit(&mut world, gas_builder, extractor, offset(GAS, -1, -1)?)?.is_ok());
    Ok(())
}

/// The preview's account of an admission outcome, for comparing the two.
fn as_refusal(outcome: &Admission) -> Result<(), SiteRefusal> {
    match outcome {
        Ok(_) => Ok(()),
        Err(GameCommandError::Build(BuildRejection::CoversResource)) => {
            Err(SiteRefusal::CoversResource)
        }
        Err(GameCommandError::Build(BuildRejection::FlaggedSite {
            flag: PlacementFlag::NearResources,
            ..
        })) => Err(SiteRefusal::NearResources),
        Err(GameCommandError::Build(BuildRejection::FlaggedSite {
            flag: PlacementFlag::Fogged,
            ..
        })) => Err(SiteRefusal::NotVisible),
        Err(_) => Err(SiteRefusal::InvalidRequest),
    }
}

/// Steps `world` until `structure`'s foundation stands or the order is abandoned.
fn arrival(world: &mut SimWorld, structure: UnitTypeId) -> Result<Admission, Box<dyn Error>> {
    for _ in 0..ARRIVAL_LIMIT {
        step(world, Vec::new());
        if world
            .entities
            .values()
            .any(|entity| entity.unit_type_id == structure)
        {
            return Ok(Ok(GameCommandReceipt::Accepted { recipients: 1 }));
        }
        if let Some(event) = world.task_events().first() {
            if let TaskEventKind::BuildAbandoned(rejection) = &event.kind {
                return Ok(Err(GameCommandError::Build(rejection.clone())));
            }
            return Err(format!("unexpected event {event:?}").into());
        }
    }
    Err("the builder neither built nor gave up".into())
}

/// F-6: for every site of a town hall analogue on a 13 × 13 grid around a mineral node and a gas
/// node, the preview and admission agree, and every admitted order is built on arrival.
#[test]
fn admission_arrival_and_preview_agree() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut world = world_on(
        catalogs.clone(),
        vec![
            node(ResourceKind::Mineral, MINERAL),
            node(ResourceKind::Gas, GAS),
        ],
    )?;
    let hall = checking(&world, PlacementFlag::NearResources)[0];
    let (width, height) = footprint(world.catalog.get(hall)?);
    let builder = builder_for(&mut world, hall, (32, 4))?;
    let mut admitted = Vec::new();
    let mut verdicts = [0_u32; 3];
    for centre_cell in [MINERAL, GAS] {
        for dy in -6..=6 {
            for dx in -6..=6 {
                let half = (i32::try_from(width / 2)?, i32::try_from(height / 2)?);
                let site = offset(centre_cell, dx - half.0, dy - half.1)?;
                let preview = world.preview_site(ME, hall, site.0, site.1);
                let outcome = admit(&mut world, builder, hall, site)?;
                assert_eq!(preview, as_refusal(&outcome), "site {site:?}");
                match preview {
                    Ok(()) => {
                        verdicts[0] += 1;
                        admitted.push(site);
                    }
                    Err(SiteRefusal::NearResources) => verdicts[1] += 1,
                    Err(_) => verdicts[2] += 1,
                }
            }
        }
    }
    assert!(verdicts.iter().all(|&count| count > 0), "{verdicts:?}");
    for site in admitted {
        let mut fresh = world_on(
            catalogs.clone(),
            vec![
                node(ResourceKind::Mineral, MINERAL),
                node(ResourceKind::Gas, GAS),
            ],
        )?;
        let beside = if site.1 >= 2 {
            (site.0, site.1 - 2)
        } else {
            (site.0, site.1 + height + 1)
        };
        let worker = builder_for(&mut fresh, hall, beside)?;
        assert!(admit(&mut fresh, worker, hall, site)?.is_ok(), "{site:?}");
        assert!(arrival(&mut fresh, hall)?.is_ok(), "{site:?} was not built");
    }
    // The arrival check runs the rule too: a node moved beside an admitted site ends the order.
    let mut moved = world_on(
        catalogs,
        vec![
            node(ResourceKind::Mineral, MINERAL),
            node(ResourceKind::Gas, GAS),
        ],
    )?;
    let site = offset(MINERAL, 8, -2)?;
    let worker = builder_for(&mut moved, hall, offset(site, 2, -3)?)?;
    assert!(admit(&mut moved, worker, hall, site)?.is_ok());
    // The field leaves the map and stands again five cells east (plan
    // `2026-11-20-resource-footprints` D-4: a node applies its ring while it stands).
    let cell = offset(MINERAL, 5, 0)?;
    moved
        .map
        .remove_resource_footprint(sim_types::ResourceNodeId(0))?;
    moved.map.resources_mut()[0] = node(ResourceKind::Mineral, cell);
    let table = moved
        .catalog
        .placement_rules()
        .resource_footprints()
        .clone();
    moved
        .map
        .place_one_resource_footprint(sim_types::ResourceNodeId(0), &table)?;
    assert_eq!(
        arrival(&mut moved, hall)?,
        flagged(PlacementFlag::NearResources, site)
    );
    Ok(())
}

/// A copy of the shipped catalog directory, removed by [`Bundle::finish`].
struct Bundle {
    directory: PathBuf,
}

impl Bundle {
    fn new(label: &str) -> Result<Self, std::io::Error> {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
        let directory = std::env::temp_dir().join(format!(
            "space-scuffle-footprint-flags-{}-{label}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory)?;
        for entry in std::fs::read_dir(source)? {
            let path = entry?.path();
            if let Some(name) = path.file_name() {
                std::fs::copy(&path, directory.join(name))?;
            }
        }
        Ok(Self { directory })
    }

    fn edit(&self, file: &str, change: impl FnOnce(&str) -> String) -> TestResult {
        let path = self.directory.join(file);
        let text = std::fs::read_to_string(&path)?;
        std::fs::write(&path, change(&text))?;
        Ok(())
    }

    fn finish(self) -> Result<(), std::io::Error> {
        std::fs::remove_dir_all(self.directory)
    }
}

/// `structures.ron` with the first 3×3 row that trains units refusing `NearResources`.
fn production_row_refusing(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_three = false;
    let mut done = false;
    let mut block: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("structure_type_id:") {
            out.append(&mut block);
            in_three = false;
        }
        if trimmed.starts_with("footprint: (3, 3)") {
            in_three = true;
        }
        block.push(line.to_owned());
        if !done && in_three && trimmed.starts_with("trains_units: [\"") {
            for kept in &mut block {
                if kept.trim_start() == "placement_excludes: []," {
                    *kept = kept.replace("[]", "[NearResources]");
                    done = true;
                }
            }
        }
    }
    out.append(&mut block);
    out.join("\n")
}

/// F-7: the rule reads the catalog. A production row made to refuse `NearResources` is refused
/// in reach, and a one-cell mineral ring admits a hall one cell from a mineral node.
#[test]
fn the_rule_reads_the_catalog() -> TestResult {
    let bundle = Bundle::new("rule")?;
    bundle.edit("structures.ron", production_row_refusing)?;
    bundle.edit("placement_rules.ron", |text| {
        let start = text.find("(kind: Mineral,").unwrap_or(text.len());
        let removal = text[start..]
            .find("remove_when_empty: ")
            .map_or(text.len(), |at| start + at);
        let end = text[removal..]
            .find("),")
            .map_or(text.len(), |at| removal + at + "),".len());
        format!(
            "{}(kind: Mineral, applies: [NearResources], anchor: (0, 0), rows: [\"x\"], \
             shape: Box(half_width: 0.5, half_height: 0.5, chamfer: 0.0), \
             remove_when_empty: true),{}",
            &text[..start],
            &text[end..]
        )
    })?;
    let catalogs = RuntimeCatalogs::load_from_directory(&bundle.directory)?;
    bundle.finish()?;
    let mut world = world_on(catalogs, vec![node(ResourceKind::Mineral, MINERAL)])?;
    let production = world
        .catalog
        .archetypes()
        .find(|arch| {
            footprint(arch) == (3, 3) && arch.placement_excludes.has(PlacementFlag::NearResources)
        })
        .ok_or("the edited production row")?
        .unit_type_id;
    let hall = world
        .catalog
        .archetypes()
        .find(|arch| {
            footprint(arch) == (5, 5) && arch.placement_excludes.has(PlacementFlag::NearResources)
        })
        .ok_or("a town hall row")?
        .unit_type_id;
    let builder = builder_for(&mut world, production, offset(MINERAL, 0, -10)?)?;
    // Under the one-cell ring only the node's own cell is flagged: the node rule takes it, and
    // every site beside it is clear.
    assert!(admit(&mut world, builder, production, offset(MINERAL, 1, -1)?)?.is_ok());
    let hall_builder = builder_for(&mut world, hall, offset(MINERAL, 0, -12)?)?;
    assert!(admit(&mut world, hall_builder, hall, offset(MINERAL, 2, -2)?)?.is_ok());
    // Under the shipped ring the same production row is refused where it stood.
    let mut shipped = world_on(
        sim_fixtures::shipped_catalogs(),
        vec![node(ResourceKind::Mineral, MINERAL)],
    )?;
    shipped.catalog.get_mut(production)?.placement_excludes = PlacementFlags::NEAR_RESOURCES;
    let builder = builder_for(&mut shipped, production, offset(MINERAL, 0, -10)?)?;
    // Re-derived for plan `2026-11-20-resource-footprints` WP5: under the shipped rows the field
    // is 2×1 (its anchor and the cell east of it), so the site one cell east of the anchor
    // covers the field (`CoversResource`); one cell east of the field it is flagged.
    let site = offset(MINERAL, 1, -1)?;
    assert_eq!(
        admit(&mut shipped, builder, production, site)?,
        Err(GameCommandError::Build(BuildRejection::CoversResource))
    );
    let site = offset(MINERAL, 2, -1)?;
    assert_eq!(
        admit(&mut shipped, builder, production, site)?,
        flagged(PlacementFlag::NearResources, site)
    );
    Ok(())
}

/// A crawler site's minimum corner and its first hidden cell, if any.
type CrawlerSite = ((u32, u32), Option<(u32, u32)>);

/// The visibility of `cell` to `ME` now.
fn seen(world: &SimWorld, cell: (u32, u32)) -> Result<VisibilityState, Box<dyn Error>> {
    Ok(world
        .vision_maps
        .get(ME)
        .ok_or("no vision map")?
        .visibility_at(cell.0, cell.1))
}

/// The first 2×2 site east of `from`, on the builder's row, whose cells hold exactly `hidden`
/// cells the player does not see.
fn crawler_site(
    world: &SimWorld,
    from: (u32, u32),
    hidden: usize,
) -> Result<CrawlerSite, Box<dyn Error>> {
    for x in from.0 + 2..SIDE - 2 {
        let site = (x, from.1);
        let mut fogged = Vec::new();
        for dy in 0..2 {
            for dx in 0..2 {
                let cell = (x + dx, from.1 + dy);
                if seen(world, cell)? != VisibilityState::Visible {
                    fogged.push(cell);
                }
            }
        }
        if fogged.len() == hidden {
            return Ok((site, fogged.first().copied()));
        }
    }
    Err(format!("no site with {hidden} hidden cells").into())
}

/// F-8: a crawler analogue's site must be in sight. A site with one fogged cell is refused at
/// that cell, in admission and in the preview; a site in sight is admitted.
#[test]
fn a_crawler_needs_sight() -> TestResult {
    let crawlers = checking(&world()?, PlacementFlag::Fogged);
    assert_eq!(crawlers.len(), 2, "the two crawler rows check Fogged");
    for crawler in crawlers {
        let mut world = world()?;
        let from = (8, 8);
        let builder = builder_for(&mut world, crawler, from)?;
        // Vision is measured at the end of a tick; admission reads the last one.
        step(&mut world, Vec::new());
        let (edge, cell) = crawler_site(&world, from, 1)?;
        let cell = cell.ok_or("the hidden cell")?;
        assert_eq!(
            world.preview_site(ME, crawler, edge.0, edge.1),
            Err(SiteRefusal::NotVisible)
        );
        assert_eq!(
            admit(&mut world, builder, crawler, edge)?,
            flagged(PlacementFlag::Fogged, cell)
        );
        let (inside, _) = crawler_site(&world, from, 0)?;
        assert_eq!(world.preview_site(ME, crawler, inside.0, inside.1), Ok(()));
        assert!(admit(&mut world, builder, crawler, inside)?.is_ok());
        // On tick zero of a fresh world nothing is seen yet, so every site is refused.
        let mut fresh = self::world()?;
        let worker = builder_for(&mut fresh, crawler, from)?;
        assert_eq!(
            fresh.preview_site(ME, crawler, inside.0, inside.1),
            Err(SiteRefusal::NotVisible)
        );
        assert_eq!(
            admit(&mut fresh, worker, crawler, inside)?,
            flagged(PlacementFlag::Fogged, inside)
        );
    }
    Ok(())
}

/// Whether `text` holds a `Build` order, written as RON writes an enum variant.
fn holds_build(text: &str) -> bool {
    text.match_indices("Build").any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + "Build".len()..].trim_start().chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && matches!(after, Some('(' | '{'))
    })
}

/// F-10 (guard): no shipped scenario, start loadout, matrix battle or golden fixture orders a
/// `Build`, which is why the rule moves no shipped world (plan §7 rule 1). A world that adds one
/// must be measured under the rule.
#[test]
fn no_shipped_world_orders_a_build() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![
        root.join("assets/catalog/start_loadouts.ron"),
        root.join("crates/sim_core/tests/fixtures/matchup_matrix.ron"),
        root.join("crates/sim_core/tests/fixtures/golden_replay.ron"),
        root.join("crates/sim_core/tests/fixtures/golden_replay_orders.ron"),
    ];
    for entry in std::fs::read_dir(root.join("assets/scenarios"))? {
        files.push(entry?.path());
    }
    assert!(files.len() > 4, "the scenarios were read");
    for file in files {
        let text = std::fs::read_to_string(&file)?;
        assert!(!holds_build(&text), "{} orders a Build", file.display());
    }
    assert!(holds_build("UnitOrder::Build { structure_type: 1 }"));
    assert!(holds_build("Build(structure_type: 1)"));
    assert!(!holds_build("BuildAbandoned(x) Building ( OrderBuild("));
    Ok(())
}
