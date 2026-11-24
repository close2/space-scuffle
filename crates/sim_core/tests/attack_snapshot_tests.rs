//! A hidden attacker leaves the player it strikes a snapshot (B-183).
//!
//! # Responsibility
//! Pins plan `2026-11-24-attacker-snapshot` P-1 to P-8 and P-10 to P-12 through the public
//! interface (`SimWorld`, `PlayerCommand`, `write_player_view`): a strike by a weapon whose
//! attack visibility is `Snapshot` (`StarCraft` II's `CEffectDamage` default, `core`
//! EffectData:62) on a unit of a player who does not see the attacker leaves that player one
//! still image of the attacker, refreshed by later strikes and dropped once the player sees the
//! image's spot or the attacker itself, and at the latest the catalog's
//! `attack_snapshot_cap_ticks` after the strike that made or last refreshed it (plan Amendment 1,
//! owner answer Q0112 2(c)). The image is never a target.
//!
//! # How it fits
//! Combat records the snapshot in the simulation's unhashed presentation workspace, the vision
//! phase drops it, and `PlayerView::attack_snapshots` publishes it. The client draws it (WP2).
//!
//! # Invariants
//! - No unit is named: archetypes are found by property and copied under fresh identities
//!   (charter §3.3).
//! - Every map is built through `sim_map`: a plateau with no ramp, so the low ground never sees
//!   up and a ground unit never climbs it.
//! - The tests of the drops by sight run on a scratch catalog whose cap is far longer than the
//!   test, so only sight ends the image there; the cap's own tests run on the shipped catalog
//!   and on scratch copies of it read from disk.

use sim_core::catalog::{CatalogLoadError, RuntimeCatalogs};
use sim_core::{SimCoreError, SimWorld, SplashSpec, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AnimState, AttackVisibility, Concealment, EntityId, MoverType, PlayerCommand, PlayerId,
    PlayerView, ShapeKind, SimScalar, SimVec2, SplashCliff, SplashImpact, SplashShape, SplashZone,
    Tick, TickBundle, UnitOrder, UnitTypeId, VisibilityState, TICKS_PER_SECOND,
};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Map width in cells.
const WIDTH: u32 = 64;
/// Map height in cells.
const HEIGHT: u32 = 32;
/// The plateau's first column: every cell with x at or above it is one cliff level up.
const PLATEAU: u32 = 16;
/// Row of the attacker's and the victim's centres.
const ROW: f64 = 16.5;
/// Ticks every walk below is allowed.
const WALK_LIMIT: u32 = 3_000;
/// The shipped units.ron's authored cap, as written there (plan Amendment 1: 1 second).
const SHIPPED_CAP: &str = "attack_snapshot_cap_ticks: Some(25),";
/// A cap far longer than any test below, so only sight ends an image under it.
const LONG_CAP: u32 = 1_000_000;

/// A copy of the shipped catalog directory with its snapshot cap authored as `cap` (the text
/// after `attack_snapshot_cap_ticks: `), loaded through the runtime loader and removed again.
fn catalogs_with_cap(label: &str, cap: &str) -> Result<RuntimeCatalogs, CatalogLoadError> {
    let directory = scratch_catalog(label, cap).map_err(|source| CatalogLoadError::Io {
        path: PathBuf::from(label),
        source,
    })?;
    let loaded = RuntimeCatalogs::load_from_directory(&directory);
    std::fs::remove_dir_all(&directory).map_err(|source| CatalogLoadError::Io {
        path: directory.clone(),
        source,
    })?;
    loaded
}

/// Copies the shipped catalog directory to a scratch directory and replaces the authored cap in
/// its `units.ron`.
fn scratch_catalog(label: &str, cap: &str) -> std::io::Result<PathBuf> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
    let directory = std::env::temp_dir().join(format!(
        "space-scuffle-attack-snapshot-{}-{label}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory)?;
    for entry in std::fs::read_dir(source)? {
        let path = entry?.path();
        if let Some(name) = path.file_name() {
            std::fs::copy(&path, directory.join(name))?;
        }
    }
    let units = directory.join("units.ron");
    let text = std::fs::read_to_string(&units)?;
    if !text.contains(SHIPPED_CAP) {
        return Err(std::io::Error::other(
            "the shipped units.ron authors the cap as SHIPPED_CAP",
        ));
    }
    let edited = text.replace(SHIPPED_CAP, &format!("attack_snapshot_cap_ticks: {cap},"));
    std::fs::write(&units, edited)?;
    Ok(directory)
}

/// A map whose cells with x >= [`PLATEAU`] stand one cliff level up, with no ramp.
fn plateau_map() -> Result<GameMap, sim_map::MapError> {
    let mut map = GameMap::new(WIDTH, HEIGHT, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..HEIGHT {
        for x in PLATEAU..WIDTH {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
        }
    }
    Ok(map)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

/// Steps `world` once with `commands`.
fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

fn step(world: &mut SimWorld) {
    step_with(world, Vec::new());
}

/// One order for one unit of `player`.
fn order(player: PlayerId, unit: EntityId, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities: vec![unit],
        order,
        queue: false,
    }
}

fn view_of(world: &SimWorld, viewer: u8) -> PlayerView {
    let mut view = PlayerView::default();
    world.write_player_view(PlayerId(viewer), &mut view);
    view
}

/// Whether `view` has the cell of `world`'s map under `position` (world units) `Visible`.
fn sees(world: &SimWorld, view: &PlayerView, position: SimVec2) -> bool {
    world.map.world_to_cell(position).is_ok_and(|(x, y)| {
        let index = y as usize * view.map_width as usize + x as usize;
        view.visibility.get(index) == Some(&VisibilityState::Visible)
    })
}

/// The first archetype passing `keep`, with the weapon `pick` chooses from it.
fn find(
    world: &SimWorld,
    keep: impl Fn(&UnitArchetype) -> bool,
    pick: impl Fn(&UnitArchetype) -> Option<&WeaponSpec>,
) -> Option<(UnitArchetype, WeaponSpec)> {
    world
        .catalog
        .archetypes()
        .filter(|arch| keep(arch))
        .find_map(|arch| pick(arch).map(|weapon| (arch.clone(), weapon.clone())))
}

/// A shipped mobile ground archetype, not a detector and without modes, whose weapon against
/// ground keeps the default `Snapshot` visibility, strikes one target and reaches at least 4.
fn snapshot_ground_attacker(world: &SimWorld) -> Option<(UnitArchetype, WeaponSpec)> {
    find(
        world,
        |arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
                && arch.modes.is_empty()
                && matches!(arch.shape, ShapeKind::UnitSphere)
        },
        |arch| {
            arch.weapon_against(MoverType::Ground, None).filter(|w| {
                w.attack_visibility == AttackVisibility::Snapshot
                    && w.splash.is_none()
                    && w.max_range >= SimScalar::from_i32(4)
            })
        },
    )
}

/// A shipped mobile air archetype.
fn air_archetype(world: &SimWorld) -> Option<UnitArchetype> {
    world
        .catalog
        .archetypes()
        .find(|arch| arch.mover_type == MoverType::Air && arch.max_speed > SimScalar::ZERO)
        .cloned()
}

/// Registers `arch` with `weapon` as its only weapon and no detection, under the next free
/// identity.
fn register(world: &mut SimWorld, arch: &UnitArchetype, weapon: Option<WeaponSpec>) -> UnitTypeId {
    let id = UnitTypeId(
        world
            .catalog
            .archetypes()
            .map(|a| a.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    let mut copy = arch.clone();
    copy.unit_type_id = id;
    format!("scratch_snapshot_{}", id.0).clone_into(&mut copy.slug);
    copy.weapon.clone_from(&weapon);
    copy.ground_weapon.clone_from(&weapon);
    copy.air_weapon = weapon.filter(|_| arch.air_weapon.is_some());
    copy.detection_radius = None;
    world.catalog.register(copy);
    id
}

/// The bench: an attacker of player 0 on the plateau's edge and a victim of player 1 below it,
/// well inside each other's weapon reach, the victim holding and made tough enough to live
/// through every test, and the attacker ordered to attack it.
struct Bench {
    world: SimWorld,
    attacker: EntityId,
    victim: EntityId,
    /// The shipped attacker archetype, for unarmed copies of it.
    arch: UnitArchetype,
}

/// Builds the bench on the shipped catalogs with the attacker's weapon changed by `edit`.
fn bench(edit: impl FnOnce(&mut WeaponSpec)) -> Result<Bench, Box<dyn std::error::Error>> {
    bench_on(sim_fixtures::try_world(plateau_map()?, 0x183)?, edit)
}

/// Builds the bench on a scratch copy of the shipped catalogs whose snapshot cap is authored as
/// `cap`, with the attacker's weapon changed by `edit`.
fn bench_with_cap(
    label: &str,
    cap: &str,
    edit: impl FnOnce(&mut WeaponSpec),
) -> Result<Bench, Box<dyn std::error::Error>> {
    let catalogs = catalogs_with_cap(label, cap)?;
    let world = SimWorld::with_catalogs(plateau_map()?, 0x183, catalogs.units, catalogs.upgrades)?;
    bench_on(world, edit)
}

/// The bench on a scratch catalog with [`LONG_CAP`]: an image there ends only by sight.
fn uncapped_bench(label: &str) -> Result<Bench, Box<dyn std::error::Error>> {
    bench_with_cap(label, &format!("Some({LONG_CAP})"), |_| {})
}

/// Builds the bench in `world` with the attacker's weapon changed by `edit`.
fn bench_on(
    mut world: SimWorld,
    edit: impl FnOnce(&mut WeaponSpec),
) -> Result<Bench, Box<dyn std::error::Error>> {
    let (arch, mut weapon) = snapshot_ground_attacker(&world)
        .ok_or("a shipped ground attacker with a Snapshot weapon")?;
    edit(&mut weapon);
    let reach = weapon.max_range + arch.collision_radius * SimScalar::from_i32(2)
        - SimScalar::from_num(1.5);
    let attacker_type = register(&mut world, &arch, Some(weapon));
    let attacker_x = SimScalar::from_num(f64::from(PLATEAU) + 0.5);
    let attacker = world.spawn_unit(
        attacker_type,
        PlayerId(0),
        SimVec2::new(attacker_x, SimScalar::from_num(ROW)),
    )?;
    let victim = world.spawn_unit(
        arch.unit_type_id,
        PlayerId(1),
        SimVec2::new(attacker_x - reach, SimScalar::from_num(ROW)),
    )?;
    toughen(&mut world, victim)?;
    world
        .entities
        .get_mut(&victim)
        .ok_or("spawned")?
        .issue_order(UnitOrder::HoldPosition, false)
        .map_err(|error| format!("{error:?}"))?;
    world
        .entities
        .get_mut(&attacker)
        .ok_or("spawned")?
        .issue_order(UnitOrder::attack(victim), false)
        .map_err(|error| format!("{error:?}"))?;
    Ok(Bench {
        world,
        attacker,
        victim,
        arch,
    })
}

/// Gives `id` health enough for every test.
fn toughen(world: &mut SimWorld, id: EntityId) -> Result<(), String> {
    let entity = world.entities.get_mut(&id).ok_or("spawned")?;
    entity.max_hp = SimScalar::from_i32(100_000);
    entity.hp = entity.max_hp;
    Ok(())
}

impl Bench {
    fn vitals(&self, id: EntityId) -> Option<(SimScalar, SimScalar)> {
        self.world.entities.get(&id).map(|e| (e.hp, e.shields))
    }

    /// Steps until `id` is next struck; returns that tick.
    fn until_struck(&mut self, id: EntityId) -> Result<u32, String> {
        let full = self.vitals(id);
        for _ in 0..400 {
            step(&mut self.world);
            if self.vitals(id) != full {
                return Ok(self.world.current_tick);
            }
        }
        Err(format!("{id:?} was struck within 400 ticks"))
    }

    /// Spawns an unarmed copy of the attacker archetype for `owner` at `position`, holding.
    fn spotter(
        &mut self,
        owner: u8,
        position: SimVec2,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let arch = self.arch.clone();
        let kind = register(&mut self.world, &arch, None);
        let id = self.world.spawn_unit(kind, PlayerId(owner), position)?;
        self.world
            .entities
            .get_mut(&id)
            .ok_or("spawned")?
            .issue_order(UnitOrder::HoldPosition, false)
            .map_err(|error| format!("{error:?}"))?;
        Ok(id)
    }

    /// Spawns an unarmed air scout of player 1 in the map's south-west corner, out of sight of
    /// the plateau's edge.
    fn scout(&mut self) -> Result<EntityId, Box<dyn std::error::Error>> {
        let arch = air_archetype(&self.world).ok_or("a shipped air unit")?;
        let kind = register(&mut self.world, &arch, None);
        Ok(self.world.spawn_unit(kind, PlayerId(1), at(1.5, 1.5))?)
    }

    /// Orders `unit` of `owner` to move to `target` and steps until it stands within a tenth of
    /// a unit of it, checking `each` after every tick.
    fn walk(
        &mut self,
        owner: u8,
        unit: EntityId,
        target: SimVec2,
        mut each: impl FnMut(&SimWorld) -> Result<(), String>,
    ) -> TestResult {
        step_with(
            &mut self.world,
            vec![order(PlayerId(owner), unit, UnitOrder::Move { target })],
        );
        for _ in 0..WALK_LIMIT {
            each(&self.world)?;
            let arrived = self
                .world
                .entities
                .get(&unit)
                .is_some_and(|e| e.position.distance(target) < SimScalar::from_num(0.1));
            if arrived {
                return Ok(());
            }
            step(&mut self.world);
        }
        Err(format!("{unit:?} reached {target:?} within {WALK_LIMIT} ticks").into())
    }

    fn snapshots(&self, viewer: u8) -> Vec<(EntityId, Tick)> {
        view_of(&self.world, viewer)
            .attack_snapshots
            .iter()
            .map(|entry| (entry.entity, entry.struck))
            .collect()
    }
}

/// P-1, P-2: a strike from where the struck player does not see the attacker leaves that player
/// one still image of it, in its pose at the strike; nobody else gets one, and the attacker is
/// not revealed, detected or targeted for it.
#[test]
fn a_hidden_attacker_leaves_the_struck_player_one_snapshot() -> TestResult {
    let mut bench = bench(|_| {})?;
    let bystander = bench.spotter(2, at(2.5, 2.5))?;
    step(&mut bench.world);
    assert!(bench.snapshots(1).is_empty(), "nothing before the strike");
    let struck = bench.until_struck(bench.victim)?;
    let view = view_of(&bench.world, 1);
    assert_eq!(view.tick, Tick(struck));
    let attacker = &bench.world.entities[&bench.attacker];
    let [entry] = view.attack_snapshots.as_slice() else {
        return Err(format!("one snapshot, got {:?}", view.attack_snapshots).into());
    };
    assert_eq!(entry.entity, bench.attacker);
    assert_eq!(entry.struck, Tick(struck), "the strike's tick");
    let image = &entry.bubble;
    assert_eq!(image.entity_id, bench.attacker);
    assert_eq!(image.owner, PlayerId(0));
    assert_eq!(image.unit_type, attacker.unit_type_id);
    assert_eq!(
        [image.world_pos[0], image.world_pos[1]],
        [attacker.position.x.to_f32(), attacker.position.y.to_f32()],
        "where it stood"
    );
    assert_eq!(image.heading_angle_rad, attacker.heading.to_radians_f32());
    assert_eq!(image.aim_angle_rad, attacker.turret_angle.to_radians_f32());
    assert_eq!(image.velocity, [0.0, 0.0], "still");
    assert_eq!(image.anim_state, AnimState::Idle, "still");
    assert!(!image.is_attacking && image.attack_swing.is_none(), "still");
    assert!(!image.selected && !image.housed);
    assert_eq!(image.concealment, Concealment::None);
    assert_eq!(image.shield_hit_flash, 0.0);
    assert_eq!(image.hull_hit_flash, 0.0);
    // P-2: only the struck player.
    assert!(bench.snapshots(0).is_empty(), "never its own player");
    assert!(
        bench.snapshots(2).is_empty(),
        "not a player it did not strike"
    );
    assert!(bench.world.entities.contains_key(&bystander));
    for viewer in 0..4 {
        let view = view_of(&bench.world, viewer);
        assert!(
            !view.attack_revealed.contains(&bench.attacker),
            "player {viewer}"
        );
        assert!(!view.revealed.contains(&bench.attacker), "player {viewer}");
    }
    assert_ne!(
        bench.world.entities[&bench.victim].current_target,
        Some(bench.attacker),
        "not targetable"
    );
    Ok(())
}

/// P-3: a player who sees the attacker's cell gets no snapshot: it sees the unit itself.
#[test]
fn a_strike_the_struck_player_sees_records_nothing() -> TestResult {
    let mut bench = bench(|_| {})?;
    let attacker_at = bench.world.entities[&bench.attacker].position;
    bench.spotter(1, at(f64::from(PLATEAU) + 3.5, ROW + 2.0))?;
    step(&mut bench.world);
    assert!(
        sees(&bench.world, &view_of(&bench.world, 1), attacker_at),
        "premise: the spotter sees the attacker's cell"
    );
    bench.until_struck(bench.victim)?;
    assert!(bench.snapshots(1).is_empty(), "seen: no snapshot");
    Ok(())
}

/// P-4: a `Visible` weapon reveals its attacker and records no snapshot; a `Hidden` weapon
/// records nothing and reveals nothing.
#[test]
fn other_visibilities_record_no_snapshot() -> TestResult {
    let mut visible = bench(|weapon| {
        weapon.attack_visibility = AttackVisibility::Visible { reveal_ticks: 54 };
    })?;
    visible.until_struck(visible.victim)?;
    assert!(visible.snapshots(1).is_empty(), "Visible: no snapshot");
    assert!(
        view_of(&visible.world, 1)
            .attack_revealed
            .contains(&visible.attacker),
        "Visible: revealed"
    );
    let mut hidden = bench(|weapon| weapon.attack_visibility = AttackVisibility::Hidden)?;
    hidden.until_struck(hidden.victim)?;
    let view = view_of(&hidden.world, 1);
    assert!(view.attack_snapshots.is_empty(), "Hidden: no snapshot");
    assert!(view.attack_revealed.is_empty(), "Hidden: no reveal");
    Ok(())
}

/// P-5: a later strike from a new spot replaces the image: one entry, the later tick, the new
/// pose. Under a long cap, so the image the second strike finds is the first one.
#[test]
fn a_later_strike_refreshes_the_snapshot() -> TestResult {
    let mut bench = uncapped_bench("refresh")?;
    let first = bench.until_struck(bench.victim)?;
    let first_at = bench.world.entities[&bench.attacker].position;
    let moved = at(f64::from(PLATEAU) + 0.5, ROW + 1.0);
    bench.walk(0, bench.attacker, moved, |_| Ok(()))?;
    step_with(
        &mut bench.world,
        vec![order(
            PlayerId(0),
            bench.attacker,
            UnitOrder::attack(bench.victim),
        )],
    );
    let second = bench.until_struck(bench.victim)?;
    assert!(second > first);
    let attacker_at = bench.world.entities[&bench.attacker].position;
    assert_ne!(attacker_at, first_at, "premise: it struck from a new spot");
    let view = view_of(&bench.world, 1);
    let [entry] = view.attack_snapshots.as_slice() else {
        return Err(format!("one snapshot, got {:?}", view.attack_snapshots).into());
    };
    assert_eq!(entry.struck, Tick(second));
    assert_eq!(
        [entry.bubble.world_pos[0], entry.bubble.world_pos[1]],
        [attacker_at.x.to_f32(), attacker_at.y.to_f32()]
    );
    Ok(())
}

/// Steps `bench` with the attacker walking far east along the plateau, out of reach of every
/// later sight of the snapshot's spot; returns the spot.
fn struck_then_gone(bench: &mut Bench) -> Result<SimVec2, Box<dyn std::error::Error>> {
    bench.until_struck(bench.victim)?;
    let spot = bench.world.entities[&bench.attacker].position;
    let far = at(f64::from(WIDTH) - 4.5, ROW);
    bench.walk(0, bench.attacker, far, |_| Ok(()))?;
    assert_eq!(bench.snapshots(1).len(), 1, "the image stays while unseen");
    Ok(spot)
}

/// Flies a scout of player 1 to `spot` and back, checking that the image is listed exactly
/// until the spot is first `Visible` to player 1 and never again after.
fn scout_the_spot(bench: &mut Bench, spot: SimVec2) -> TestResult {
    let scout = bench.scout()?;
    let mut seen = false;
    bench.walk(1, scout, spot, |world| {
        let view = view_of(world, 1);
        seen |= sees(world, &view, spot);
        match (seen, view.attack_snapshots.len()) {
            (false, 1) | (true, 0) => Ok(()),
            (seen, count) => Err(format!(
                "tick {}: spot seen {seen}, {count} snapshots",
                world.current_tick
            )),
        }
    })?;
    assert!(seen, "the scout saw the spot");
    bench.walk(1, scout, at(1.5, 1.5), |world| {
        let view = view_of(world, 1);
        if view.attack_snapshots.is_empty() {
            Ok(())
        } else {
            Err(format!("tick {}: the image came back", world.current_tick))
        }
    })?;
    assert!(
        !sees(&bench.world, &view_of(&bench.world, 1), spot),
        "fogged again"
    );
    Ok(())
}

/// P-6: the image is dropped on the tick its spot is first seen, and does not come back when the
/// spot is fogged again.
/// Under a long cap, so only sight ends the image.
#[test]
fn seeing_the_spot_drops_the_snapshot() -> TestResult {
    let mut bench = uncapped_bench("spot")?;
    let spot = struck_then_gone(&mut bench)?;
    scout_the_spot(&mut bench, spot)
}

/// P-7: the image is dropped on the tick the attacker itself is shown to the player, wherever it
/// stands.
/// Under a long cap, so only sight ends the image.
#[test]
fn seeing_the_attacker_drops_the_snapshot() -> TestResult {
    let mut bench = uncapped_bench("attacker")?;
    let watch = at(f64::from(WIDTH) - 6.5, ROW);
    bench.spotter(1, watch)?;
    bench.until_struck(bench.victim)?;
    assert_eq!(bench.snapshots(1).len(), 1, "premise: the image is listed");
    let attacker = bench.attacker;
    let mut shown = false;
    bench.walk(0, attacker, at(f64::from(WIDTH) - 12.5, ROW), |world| {
        let view = view_of(world, 1);
        shown |= sees(world, &view, world.entities[&attacker].position);
        match (shown, view.attack_snapshots.len()) {
            (false, 1) | (true, 0) => Ok(()),
            (shown, count) => Err(format!(
                "tick {}: attacker shown {shown}, {count} snapshots",
                world.current_tick
            )),
        }
    })?;
    assert!(shown, "the attacker walked into sight");
    Ok(())
}

/// P-8: the attacker's death out of sight does not drop the image; seeing the spot does.
/// Under a long cap, so only sight ends the image.
#[test]
fn an_attacker_dead_out_of_sight_keeps_its_snapshot_until_the_spot_is_seen() -> TestResult {
    let mut bench = uncapped_bench("dead")?;
    let spot = struck_then_gone(&mut bench)?;
    bench
        .world
        .entities
        .get_mut(&bench.attacker)
        .ok_or("alive")?
        .hp = SimScalar::ZERO;
    for _ in 0..30 {
        step(&mut bench.world);
    }
    assert!(
        bench
            .world
            .entities
            .get(&bench.attacker)
            .is_none_or(|e| !e.is_alive()),
        "premise: the attacker is dead"
    );
    assert_eq!(bench.snapshots(1).len(), 1, "kept after the death");
    scout_the_spot(&mut bench, spot)
}

/// P-10: while the image is listed, the struck player's units neither acquire nor strike the
/// attacker, and an explicit attack order on it strikes nothing.
/// Under a long cap, so only sight ends the image.
#[test]
fn the_snapshot_is_never_a_target() -> TestResult {
    let mut bench = uncapped_bench("target")?;
    bench.until_struck(bench.victim)?;
    let full = bench.vitals(bench.attacker);
    for _ in 0..60 {
        step(&mut bench.world);
        assert_ne!(
            bench.world.entities[&bench.victim].current_target,
            Some(bench.attacker)
        );
    }
    step_with(
        &mut bench.world,
        vec![order(
            PlayerId(1),
            bench.victim,
            UnitOrder::attack(bench.attacker),
        )],
    );
    for _ in 0..120 {
        step(&mut bench.world);
        assert_eq!(bench.snapshots(1).len(), 1, "still listed");
    }
    assert_eq!(bench.vitals(bench.attacker), full, "never struck");
    Ok(())
}

/// A shipped detector structure, as an unarmed copy under the next free identity.
fn unarmed_detector(world: &mut SimWorld) -> Option<(UnitTypeId, UnitArchetype)> {
    let arch = world
        .catalog
        .archetypes()
        .find(|arch| arch.is_detector() && matches!(arch.shape, ShapeKind::BuildingDome { .. }))?
        .clone();
    let id = UnitTypeId(
        world
            .catalog
            .archetypes()
            .map(|a| a.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    let mut copy = arch.clone();
    copy.unit_type_id = id;
    "scratch_snapshot_detector".clone_into(&mut copy.slug);
    copy.weapon = None;
    copy.ground_weapon = None;
    copy.air_weapon = None;
    world.catalog.register(copy);
    Some((id, arch))
}

/// P-11 (D-3): an undetected cloaked attacker leaves no image; the same attacker inside the
/// struck player's detection circle, though out of its sight, leaves one, drawn unconcealed.
#[test]
fn a_concealed_attacker_leaves_a_snapshot_only_where_detected() -> TestResult {
    let cloak = |bench: &mut Bench| -> TestResult {
        bench
            .world
            .entities
            .get_mut(&bench.attacker)
            .ok_or("spawned")?
            .is_cloaked = true;
        Ok(())
    };
    let mut undetected = bench(|_| {})?;
    cloak(&mut undetected)?;
    undetected.until_struck(undetected.victim)?;
    assert!(undetected.snapshots(1).is_empty(), "undetected: nothing");

    let mut detected = bench(|_| {})?;
    cloak(&mut detected)?;
    let (detector_type, detector_arch) =
        unarmed_detector(&mut detected.world).ok_or("a shipped detector structure")?;
    let ShapeKind::BuildingDome { footprint_w, .. } = detector_arch.shape else {
        return Err("a structure has a dome".into());
    };
    let detector = detected.world.spawn_structure(
        detector_type,
        PlayerId(1),
        PLATEAU - u32::from(footprint_w),
        19,
    )?;
    let attacker_at = detected.world.entities[&detected.attacker].position;
    let detector_at = detected.world.entities[&detector].position;
    assert!(
        detector_at.distance(attacker_at)
            < detector_arch.detection_radius.ok_or("a detector")? - SimScalar::ONE,
        "premise: the attacker is within the detection radius"
    );
    detected.until_struck(detected.victim)?;
    let view = view_of(&detected.world, 1);
    assert!(
        !sees(&detected.world, &view, attacker_at),
        "premise: out of sight"
    );
    let [entry] = view.attack_snapshots.as_slice() else {
        return Err(format!("detected: one snapshot, got {:?}", view.attack_snapshots).into());
    };
    assert_eq!(entry.entity, detected.attacker);
    assert_eq!(entry.bubble.concealment, Concealment::None, "drawn plainly");
    assert!(!view.revealed.contains(&detected.attacker), "not revealed");
    Ok(())
}

/// P-12: an area strike that hits several units of one player leaves that player one image, and
/// one for each other player it strikes.
#[test]
fn an_area_strike_leaves_one_snapshot_per_struck_player() -> TestResult {
    let mut bench = bench(|weapon| {
        weapon.splash = Some(SplashSpec {
            zones: vec![SplashZone {
                radius: SimScalar::from_i32(2),
                fraction: SimScalar::ONE,
            }],
            shape: SplashShape::Circle,
            friendly_fire: false,
            offset_by_unit_radius: false,
            impact: SplashImpact::Target,
            target_radius_below: None,
            cliff: SplashCliff::Same,
            strike_target_on_other_level: false,
        });
    })?;
    let victim_at = bench.world.entities[&bench.victim].position;
    let beside = bench.spotter(1, victim_at + at(0.0, 1.0))?;
    let other = bench.spotter(2, victim_at - at(0.0, 1.0))?;
    toughen(&mut bench.world, beside)?;
    toughen(&mut bench.world, other)?;
    let struck = bench.until_struck(bench.victim)?;
    assert_ne!(
        bench.vitals(beside).map(|v| v.0),
        Some(SimScalar::from_i32(100_000)),
        "premise: the area struck the second unit"
    );
    assert_eq!(bench.snapshots(1), [(bench.attacker, Tick(struck))]);
    assert_eq!(bench.snapshots(2), [(bench.attacker, Tick(struck))]);
    assert!(bench.snapshots(3).is_empty(), "a player it did not strike");
    Ok(())
}

/// Orders the attacker east along the plateau, out of every sight of player 1, and steps until
/// player 1's image is gone. After every tick, a listed image must be younger than `cap` ticks.
/// Returns the strike tick of the last image listed and the first tick without one.
fn walk_off_until_gone(bench: &mut Bench, cap: u32) -> Result<(u32, u32), String> {
    let far = at(f64::from(WIDTH) - 4.5, ROW);
    step_with(
        &mut bench.world,
        vec![order(
            PlayerId(0),
            bench.attacker,
            UnitOrder::Move { target: far },
        )],
    );
    let mut last = None;
    for _ in 0..WALK_LIMIT {
        let now = bench.world.current_tick;
        match bench.snapshots(1).as_slice() {
            [(_, struck)] if now.wrapping_sub(struck.0) < cap => last = Some(struck.0),
            [(_, struck)] => {
                return Err(format!(
                    "tick {now}: the image of the strike on {struck:?} outlived the cap {cap}"
                ))
            }
            [] => {
                return last
                    .map(|struck| (struck, now))
                    .ok_or_else(|| format!("tick {now}: no image when the walk began"))
            }
            more => return Err(format!("tick {now}: one image, got {more:?}")),
        }
        step(&mut bench.world);
    }
    Err(format!("the image ended within {WALK_LIMIT} ticks"))
}

/// Amendment 1 (Q0112 2(c)): under the shipped catalog, an image left alone (its attacker walks
/// off out of sight and strikes no more) ends exactly 1 second (25 ticks) after its strike, and
/// not a tick earlier.
#[test]
fn a_snapshot_left_alone_ends_when_the_cap_is_reached() -> TestResult {
    let mut bench = bench(|_| {})?;
    let cap = u32::from(TICKS_PER_SECOND);
    bench.until_struck(bench.victim)?;
    let (struck, gone) = walk_off_until_gone(&mut bench, cap)?;
    assert_eq!(gone, struck + cap, "ends on the tick the cap is reached");
    Ok(())
}

/// Amendment 1: a later strike before the cap moves the image to the attacker's new pose and
/// restarts the cap, so it outlives the first strike's cap and ends the cap after the last strike.
/// The cap here is a scratch catalog's 200 ticks, longer than the attacker's walk between the two
/// strikes.
#[test]
fn a_later_strike_before_the_cap_restarts_it() -> TestResult {
    let cap = 200;
    let mut bench = bench_with_cap("restart", &format!("Some({cap})"), |_| {})?;
    let first = bench.until_struck(bench.victim)?;
    let first_at = bench.world.entities[&bench.attacker].position;
    let moved = at(f64::from(PLATEAU) + 0.5, ROW + 1.0);
    bench.walk(0, bench.attacker, moved, |_| Ok(()))?;
    step_with(
        &mut bench.world,
        vec![order(
            PlayerId(0),
            bench.attacker,
            UnitOrder::attack(bench.victim),
        )],
    );
    let second = bench.until_struck(bench.victim)?;
    assert!(second < first + cap, "premise: struck again before the cap");
    let second_at = bench.world.entities[&bench.attacker].position;
    assert_ne!(second_at, first_at, "premise: it struck from a new spot");
    let view = view_of(&bench.world, 1);
    let [entry] = view.attack_snapshots.as_slice() else {
        return Err(format!("one snapshot, got {:?}", view.attack_snapshots).into());
    };
    assert_eq!(entry.struck, Tick(second), "refreshed by the second strike");
    assert_eq!(
        [entry.bubble.world_pos[0], entry.bubble.world_pos[1]],
        [second_at.x.to_f32(), second_at.y.to_f32()],
        "moved to the new pose"
    );
    while bench.world.current_tick < first + cap {
        step(&mut bench.world);
    }
    assert_eq!(
        bench.snapshots(1).len(),
        1,
        "listed past the first strike's cap"
    );
    let (last, gone) = walk_off_until_gone(&mut bench, cap)?;
    assert!(last >= second);
    assert_eq!(gone, last + cap, "ends the cap after the last strike");
    Ok(())
}

/// Amendment 1: under the shipped cap, seeing the image's spot (here also the attacker standing
/// on it) still ends the image at once, long before the cap.
#[test]
fn sight_still_ends_a_snapshot_before_the_cap() -> TestResult {
    let mut bench = bench(|_| {})?;
    let struck = bench.until_struck(bench.victim)?;
    let spot = bench.world.entities[&bench.attacker].position;
    assert_eq!(bench.snapshots(1).len(), 1, "premise: the image is listed");
    bench.spotter(1, at(f64::from(PLATEAU) + 3.5, ROW + 2.0))?;
    step(&mut bench.world);
    assert!(
        sees(&bench.world, &view_of(&bench.world, 1), spot),
        "premise: the spotter sees the spot"
    );
    assert!(bench.snapshots(1).is_empty(), "ended by sight");
    assert!(bench.world.current_tick < struck + u32::from(TICKS_PER_SECOND));
    Ok(())
}

/// Amendment 1: the cap is read from the catalog at run time. A scratch copy of the shipped
/// catalog authoring 40 ticks makes an image left alone last 40 ticks; one authoring `None`
/// keeps it until it is seen, as before the amendment.
#[test]
fn the_cap_is_read_from_the_catalog() -> TestResult {
    let mut forty = bench_with_cap("forty", "Some(40)", |_| {})?;
    forty.until_struck(forty.victim)?;
    let (struck, gone) = walk_off_until_gone(&mut forty, 40)?;
    assert_eq!(gone, struck + 40);

    let mut none = bench_with_cap("none", "None", |_| {})?;
    none.until_struck(none.victim)?;
    let struck = none.snapshots(1);
    let far = at(f64::from(WIDTH) - 4.5, ROW);
    none.walk(0, none.attacker, far, |_| Ok(()))?;
    for _ in 0..300 {
        step(&mut none.world);
    }
    assert_eq!(none.snapshots(1), struck, "no cap: kept until seen");
    Ok(())
}

/// Amendment 1: a cap of 0, which would drop every image before it is shown, is refused at load
/// with a typed error naming `units.ron` and the field.
#[test]
fn a_cap_of_zero_is_refused() -> TestResult {
    match catalogs_with_cap("zero", "Some(0)").err() {
        Some(CatalogLoadError::Invalid {
            path,
            source: SimCoreError::CatalogInvalidValue { field, .. },
        }) => {
            assert_eq!(path.file_name(), Some(std::ffi::OsStr::new("units.ron")));
            assert_eq!(field, "attack_snapshot_cap_ticks");
            Ok(())
        }
        other => Err(format!("a cap of 0 is refused, got {other:?}").into()),
    }
}
