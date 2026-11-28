//! A player remembers the structures of other players it was shown (B-252).
//!
//! # Responsibility
//! Pins plan `2026-12-01-minimap` Amendment 1, R-1 to R-5, R-9 and R-10, through the public
//! interface (`SimWorld::step`, `write_player_view`): a structure of another player that a player
//! was shown stays in that player's view as a still image of how it last saw it
//! (`PlayerView::remembered_structures`, `StarCraft` II's `FogVisibility Snapshot`) until the
//! player sees its spot again, never while the structure itself is shown, and never for anything
//! the player was not shown. R-6 to R-8 need the simulation's internals and are unit tests of
//! `world/vision.rs` and `world/snapshot.rs`.
//!
//! # How it fits
//! `SimWorld::update_remembered_structures` keeps the records in the unhashed presentation
//! workspace once per tick, and `write_player_view` publishes a viewer's own. The client draws
//! them (minimap marks and the snapshot pool).
//!
//! # Invariants
//! - No unit is named: archetypes are found by property (charter §3.3).
//! - The viewer's unit leaves sight by dying (its hit points set to zero, reaped in the tick's
//!   own phase 8, after that tick's vision), so the last tick the structure was shown is known.

use sim_core::production::construction::Construction;
use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AnimState, BubbleRenderInstance, Concealment, EntityId, MoverType, PlayerCommand, PlayerId,
    PlayerView, RenderSnapshot, ShapeKind, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
    UnitTypeId, VisibilityState,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The viewer.
const ME: PlayerId = PlayerId(0);
/// The structure's owner.
const THEM: PlayerId = PlayerId(1);
/// Map side in cells.
const SIDE: u32 = 64;
/// The structure's top-left cell.
const CELL: (u32, u32) = (30, 30);

/// Steps `world` once with `commands` and returns its render snapshot.
fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) -> RenderSnapshot {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    })
}

fn step(world: &mut SimWorld) -> RenderSnapshot {
    step_with(world, Vec::new())
}

fn view_of(world: &SimWorld, viewer: PlayerId) -> PlayerView {
    let mut view = PlayerView::default();
    world.write_player_view(viewer, &mut view);
    view
}

/// A shipped structure archetype of at most 3 by 3 cells that is no gas extractor and trains
/// units, and one unit it trains.
fn producer(world: &SimWorld) -> Option<(UnitArchetype, UnitTypeId)> {
    world.catalog.archetypes().find_map(|arch| {
        let ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
            ..
        } = arch.shape
        else {
            return None;
        };
        if footprint_w > 3 || footprint_h > 3 || arch.capabilities.is_gas_extractor {
            return None;
        }
        let product = arch
            .trains_units
            .iter()
            .filter_map(|slug| world.catalog.id_by_slug(slug))
            .find(|id| {
                world
                    .catalog
                    .get(*id)
                    .is_ok_and(|product| product.requires_structures.is_empty())
            })?;
        Some((arch.clone(), product))
    })
}

/// A shipped mobile ground archetype that is no detector, the scout.
fn scout_type(world: &SimWorld) -> Option<UnitTypeId> {
    world
        .catalog
        .archetypes()
        .find(|arch| {
            matches!(arch.shape, ShapeKind::UnitSphere)
                && arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
                && arch.sight_radius >= SimScalar::from_i32(5)
        })
        .map(|arch| arch.unit_type_id)
}

/// The bench: a flat map with a structure of [`THEM`] at [`CELL`] and nothing of [`ME`] yet.
struct Bench {
    world: SimWorld,
    structure: EntityId,
    centre: SimVec2,
    scout: UnitTypeId,
    product: UnitTypeId,
}

fn bench() -> Result<Bench, Box<dyn std::error::Error>> {
    let map = GameMap::new(SIDE, SIDE, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::try_world(map, 0x252)?;
    let (arch, product) = producer(&world).ok_or("a shipped producing structure")?;
    let scout = scout_type(&world).ok_or("a shipped scout")?;
    let structure = world.spawn_structure(arch.unit_type_id, THEM, CELL.0, CELL.1)?;
    let centre = world.entities[&structure].position;
    // A unit of the viewer far away, so the viewer stays in the match.
    world.spawn_unit(scout, ME, SimVec2::from_i32(4, 4))?;
    step(&mut world);
    Ok(Bench {
        world,
        structure,
        centre,
        scout,
        product,
    })
}

impl Bench {
    /// Spawns a scout of the viewer two cells below the structure's footprint.
    fn scout_beside(&mut self) -> Result<EntityId, Box<dyn std::error::Error>> {
        let at = self.centre + SimVec2::from_i32(0, 4);
        Ok(self.world.spawn_unit(self.scout, ME, at)?)
    }

    /// Kills `unit`: it is reaped in the next tick's phase 8, after that tick's vision.
    fn kill(&mut self, unit: EntityId) {
        if let Some(entity) = self.world.entities.get_mut(&unit) {
            entity.hp = SimScalar::ZERO;
        }
    }

    fn view(&self) -> PlayerView {
        view_of(&self.world, ME)
    }

    fn shown(&self) -> bool {
        let view = self.view();
        let (x, y) = self
            .world
            .map
            .world_to_cell(self.centre)
            .unwrap_or((u32::MAX, u32::MAX));
        view.visibility
            .get(y as usize * view.map_width as usize + x as usize)
            == Some(&VisibilityState::Visible)
    }

    /// Sees the structure with a scout, which then dies; returns the render record of the last
    /// tick the structure was shown and that tick.
    fn scout_and_leave(
        &mut self,
    ) -> Result<(BubbleRenderInstance, Tick), Box<dyn std::error::Error>> {
        let scout = self.scout_beside()?;
        step(&mut self.world);
        assert!(self.shown(), "premise: the scout shows the structure");
        assert_eq!(
            self.view().remembered_structures,
            [],
            "R-2: never while shown"
        );
        self.kill(scout);
        let snapshot = step(&mut self.world);
        let bubble = snapshot
            .bubbles
            .iter()
            .find(|bubble| bubble.entity_id == self.structure)
            .cloned()
            .ok_or("the structure is presented")?;
        assert_eq!(
            self.view().remembered_structures,
            [],
            "R-2: still shown this tick"
        );
        let seen = snapshot.tick;
        step(&mut self.world);
        assert!(!self.shown(), "premise: the scout is gone");
        Ok((bubble, seen))
    }
}

/// R-1: after the scout leaves, the view lists exactly one remembered structure, last seen on the
/// last tick it was shown, whose image is that tick's render record, still.
#[test]
fn r1_the_image_is_the_structure_as_last_shown() -> TestResult {
    let mut bench = bench()?;
    // A hurt structure, so the image's health share is not trivially full.
    let entity = bench
        .world
        .entities
        .get_mut(&bench.structure)
        .ok_or("the structure")?;
    entity.hp = entity.max_hp * SimScalar::from_num(0.625);
    let (live, seen) = bench.scout_and_leave()?;
    let view = bench.view();
    let [entry] = view.remembered_structures.as_slice() else {
        return Err(format!("one image: {:?}", view.remembered_structures).into());
    };
    assert_eq!(entry.entity, bench.structure);
    assert_eq!(entry.seen, seen);
    let image = &entry.bubble;
    assert_eq!(image.unit_type, live.unit_type);
    assert_eq!(image.owner, live.owner);
    assert_eq!(image.world_pos, live.world_pos);
    assert_eq!(image.shape, live.shape);
    assert_eq!(image.radius, live.radius);
    assert_eq!(image.heading_angle_rad, live.heading_angle_rad);
    assert_eq!(image.aim_angle_rad, live.aim_angle_rad);
    assert_eq!(image.hp_ratio, live.hp_ratio);
    assert_eq!(image.shield_ratio, live.shield_ratio);
    assert_eq!(image.stance, live.stance);
    assert!((image.hp_ratio - 0.625).abs() < 1e-4, "{}", image.hp_ratio);
    assert_eq!(image.velocity, [0.0, 0.0]);
    assert_eq!(image.anim_state, AnimState::Idle);
    assert!(!image.selected && !image.housed);
    assert_eq!(image.concealment, Concealment::None);
    assert_eq!((image.shield_hit_flash, image.hull_hit_flash), (0.0, 0.0));
    Ok(())
}

/// R-3: damage or death in the fog changes nothing; seeing the spot drops the image on that
/// tick; seeing the structure again refreshes it.
#[test]
fn r3_the_image_lasts_until_the_spot_is_seen() -> TestResult {
    let mut bench = bench()?;
    let (live, seen) = bench.scout_and_leave()?;
    let entity = bench
        .world
        .entities
        .get_mut(&bench.structure)
        .ok_or("the structure")?;
    entity.hp = entity.max_hp * SimScalar::from_num(0.25);
    step(&mut bench.world);
    let image = bench.view().remembered_structures[0].bubble.clone();
    assert_eq!(
        image.hp_ratio, live.hp_ratio,
        "damage in the fog is not seen"
    );
    // Seen again alive: no image while shown; after leaving, a newer one.
    let scout = bench.scout_beside()?;
    step(&mut bench.world);
    assert_eq!(
        bench.view().remembered_structures,
        [],
        "shown again, no image"
    );
    bench.kill(scout);
    step(&mut bench.world);
    step(&mut bench.world);
    let view = bench.view();
    assert_eq!(view.remembered_structures.len(), 1);
    assert!(view.remembered_structures[0].seen > seen, "refreshed");
    assert!((view.remembered_structures[0].bubble.hp_ratio - 0.25).abs() < 1e-4);
    // Destroyed in the fog: the image stays.
    bench
        .world
        .entities
        .get_mut(&bench.structure)
        .ok_or("the structure")?
        .hp = SimScalar::ZERO;
    step(&mut bench.world);
    step(&mut bench.world);
    assert!(
        !bench.world.entities.contains_key(&bench.structure),
        "premise: reaped"
    );
    assert_eq!(
        bench.view().remembered_structures.len(),
        1,
        "death in the fog is not seen"
    );
    // The spot is seen: gone on that tick.
    bench.scout_beside()?;
    step(&mut bench.world);
    assert!(bench.shown());
    assert_eq!(
        bench.view().remembered_structures,
        [],
        "the empty spot is seen"
    );
    Ok(())
}

/// R-4: a foundation is remembered at the progress it was seen at, however far it builds since;
/// a finished structure's image shows no queue progress while its owner produces in it.
#[test]
fn r4_a_foundation_keeps_its_seen_progress_and_a_queue_never_shows() -> TestResult {
    let mut bench = bench()?;
    let entity = bench
        .world
        .entities
        .get_mut(&bench.structure)
        .ok_or("the structure")?;
    let max_hp = entity.max_hp;
    entity.construction = Some(Construction {
        remaining_ticks: 700,
        total_ticks: 1_000,
        hp_per_tick: max_hp / SimScalar::from_i32(1_000),
        final_hp_residue: SimScalar::ZERO,
        paid_minerals: 0,
        paid_gas: 0,
        last_worked_tick: None,
    });
    entity.hp = max_hp * SimScalar::from_num(0.3);
    let (live, _) = bench.scout_and_leave()?;
    let image = bench.view().remembered_structures[0].bubble.clone();
    assert_eq!(image.anim_state, AnimState::Constructing);
    assert_eq!(image.build_or_queue_progress, live.build_or_queue_progress);
    let progress = image
        .build_or_queue_progress
        .ok_or("a foundation's progress")?;
    assert!((progress - 0.3).abs() < 1e-4, "{progress}");
    assert_eq!(image.construction_hp_ratio, live.construction_hp_ratio);
    if let Some(record) = bench
        .world
        .entities
        .get_mut(&bench.structure)
        .and_then(|entity| entity.construction.as_mut())
    {
        record.remaining_ticks = 100;
    }
    step(&mut bench.world);
    let image = bench.view().remembered_structures[0].bubble.clone();
    assert_eq!(
        image.build_or_queue_progress,
        Some(progress),
        "built since, unseen"
    );
    Ok(())
}

#[test]
fn r4_a_producing_structure_shows_no_queue() -> TestResult {
    let mut bench = bench()?;
    let slot = THEM.slot().ok_or("a player slot")?;
    let resources = &mut bench.world.player_resources[slot];
    resources.minerals = 100_000;
    resources.gas = 100_000;
    // A supply structure of the owner, far from the viewer's scout.
    let supply = bench
        .world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.supply_provided > 0
                && matches!(arch.shape, ShapeKind::BuildingDome { .. })
                && !arch.capabilities.is_gas_extractor
        })
        .map(|arch| arch.unit_type_id)
        .ok_or("a shipped supply structure")?;
    bench.world.spawn_structure(supply, THEM, 50, 50)?;
    step_with(
        &mut bench.world,
        vec![PlayerCommand {
            player_id: THEM,
            entities: vec![bench.structure],
            order: UnitOrder::Produce {
                unit_type: bench.product,
            },
            queue: false,
        }],
    );
    assert!(
        !bench.world.entities[&bench.structure]
            .production_queue
            .is_empty(),
        "premise: the structure produces: {:?}",
        bench.world.production_outcomes()
    );
    let (live, _) = bench.scout_and_leave()?;
    assert!(
        live.build_or_queue_progress.is_some(),
        "premise: the live record shows it"
    );
    let image = &bench.view().remembered_structures[0].bubble;
    assert_eq!(image.build_or_queue_progress, None, "no queue leaks");
    assert_eq!(image.anim_state, AnimState::Idle);
    Ok(())
}

/// R-5: nothing the viewer was not shown is remembered.
#[test]
fn r5_nothing_unseen_is_remembered() -> TestResult {
    let mut bench = bench()?;
    for _ in 0..10 {
        step(&mut bench.world);
    }
    assert_eq!(bench.view().remembered_structures, [], "never seen");
    // One's own structures never are.
    let mine = bench.world.spawn_structure(
        bench.world.entities[&bench.structure].unit_type_id,
        ME,
        10,
        40,
    )?;
    for _ in 0..3 {
        step(&mut bench.world);
    }
    assert!(
        bench
            .view()
            .remembered_structures
            .iter()
            .all(|entry| entry.entity != mine),
        "one's own"
    );
    assert_eq!(view_of(&bench.world, THEM).remembered_structures, []);
    // A viewer outside the slots gets nothing.
    bench.scout_and_leave()?;
    assert_eq!(bench.view().remembered_structures.len(), 1);
    assert_eq!(
        view_of(&bench.world, PlayerId(200)).remembered_structures,
        []
    );
    Ok(())
}

/// R-5: a structure whose centre cell was never `Visible` is never remembered, even when the
/// viewer saw some of its footprint.
#[test]
fn r5_a_structure_whose_centre_was_never_seen_is_not_remembered() -> TestResult {
    let mut found = false;
    for distance in 3..12 {
        let mut bench = bench()?;
        let at = bench.centre + SimVec2::from_i32(distance, 0);
        let scout = bench.world.spawn_unit(bench.scout, ME, at)?;
        step(&mut bench.world);
        let view = bench.view();
        let visible = |x: u32, y: u32| {
            view.visibility
                .get(y as usize * view.map_width as usize + x as usize)
                == Some(&VisibilityState::Visible)
        };
        let placed = &bench.world.map.structures()[&bench.structure];
        let some_seen = (0..u32::from(placed.height)).any(|dy| {
            (0..u32::from(placed.width))
                .any(|dx| visible(placed.top_left.0 + dx, placed.top_left.1 + dy))
        });
        if some_seen && !bench.shown() {
            found = true;
            bench.kill(scout);
            step(&mut bench.world);
            step(&mut bench.world);
            assert_eq!(
                bench.view().remembered_structures,
                [],
                "its centre was never seen"
            );
            break;
        }
    }
    assert!(
        found,
        "some scouting distance sees the footprint's edge only"
    );
    Ok(())
}

/// R-10: two viewers keep separate records of one structure; the list is sorted by entity; a
/// refill of a warmed view keeps its buffer (R-9).
#[test]
fn r10_viewers_keep_separate_sorted_records() -> TestResult {
    let mut bench = bench()?;
    let kind = bench.world.entities[&bench.structure].unit_type_id;
    let second = bench
        .world
        .spawn_structure(kind, THEM, CELL.0 - 6, CELL.1)?;
    let third_party = PlayerId(2);
    bench
        .world
        .spawn_unit(bench.scout, third_party, SimVec2::from_i32(60, 4))?;
    let scout = bench.scout_beside()?;
    let other = bench.world.spawn_unit(
        bench.scout,
        third_party,
        bench.centre + SimVec2::from_i32(-3, 4),
    )?;
    step(&mut bench.world);
    bench.kill(scout);
    step(&mut bench.world);
    step(&mut bench.world);
    let mine: Vec<EntityId> = bench
        .view()
        .remembered_structures
        .iter()
        .map(|entry| entry.entity)
        .collect();
    assert!(
        mine.contains(&bench.structure) && mine.contains(&second),
        "{mine:?}"
    );
    assert!(
        mine.windows(2).all(|pair| pair[0] < pair[1]),
        "sorted: {mine:?}"
    );
    assert_eq!(
        view_of(&bench.world, third_party).remembered_structures,
        [],
        "the other viewer still sees them"
    );
    bench.kill(other);
    step(&mut bench.world);
    step(&mut bench.world);
    assert_eq!(
        view_of(&bench.world, third_party)
            .remembered_structures
            .len(),
        2
    );
    // R-9: a warmed view refills in place.
    let mut view = PlayerView::default();
    bench.world.write_player_view(ME, &mut view);
    let (address, capacity) = (
        view.remembered_structures.as_ptr(),
        view.remembered_structures.capacity(),
    );
    step(&mut bench.world);
    bench.world.write_player_view(ME, &mut view);
    assert_eq!(view.remembered_structures.as_ptr(), address);
    assert_eq!(view.remembered_structures.capacity(), capacity);
    Ok(())
}
