//! # Integration Test: Blinding Cloud (plan `2026-11-10-spellcaster-mechanics` WP5)
//!
//! The shipped Viper analogue's Blinding Cloud, cast through `SimWorld::execute_ability` and
//! played through `SimWorld::step`, against the `StarCraft` II 5.0.14 rules of plan §3.4 and
//! §6.4: a cloud of radius 2 opens on the cast tick at the target point and searches 16 times,
//! on `round(k × 8.929)` ticks for k = 0 … 15, for ground units and structures of every owner
//! but the caster itself. A unit found gets `blinding_cloud` (every ranged weapon's range −20,
//! down to the 0.1 floor of SU-2; melee weapons untouched), a structure
//! `blinding_cloud_structure` (every weapon −20 and no detection). Both last 11.16 ticks and are
//! refreshed by every search that finds the unit again, so a unit keeps the status 12 ticks
//! after the last search that found it. The caster pays 100 energy; there is no cooldown and no
//! lock.
//!
//! The tests read the shipped catalog (`units.ron`, `structures.ron`, `ability_effects.ron`,
//! `statuses.ron`), so a data edit that breaks a rule fails here. Units are named by slug in
//! this test only (charter §3.3 governs production code). The Viper analogue has no weapon, so
//! it never hits a unit.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::catalog::UnitArchetype;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySource, AttackOrigin, EntityId, PlayerId, PlayerView, SimScalar,
    SimVec2, StatusKind, Tick, TickBundle, UnitOrder, WeaponSpec,
};

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The ability's name in the shipped catalog.
const CLOUD: &str = "Blinding Cloud";

/// The search ticks of plan §3.4 counted from the cast: `round_half_up(k × 8.928571)`, k = 0 …
/// 15 (`BlindingCloudCP`, `swarm` EffectData:236-242, `PeriodCount` 15 at `voidmulti` :158).
const SEARCHES: [u32; 16] = [
    0, 9, 18, 27, 36, 45, 54, 63, 71, 80, 89, 98, 107, 116, 125, 134,
];

/// Ticks after the last search that found a unit on which its status is gone: 11.16 elapsed,
/// so the 12th (`Duration` 0.625 Normal s, `swarm` BehaviorData:997).
const LINGER: u32 = 12;

/// The range a reduced weapon keeps (SU-2, `range_floor`'s default).
fn floor() -> SimScalar {
    SimScalar::ONE.saturating_div(n(10))
}

fn world() -> SimWorld {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a flat map");
    sim_fixtures::world(map, 0xB11D)
}

fn at(x: f32, y: f32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn n(value: i32) -> SimScalar {
    SimScalar::from_i32(value)
}

/// Spawns the shipped unit `slug` for `owner` at `(x, y)`.
fn spawn(world: &mut SimWorld, slug: &str, owner: PlayerId, x: f32, y: f32) -> EntityId {
    let kind = world.catalog.id_by_slug(slug).expect("a shipped unit");
    world
        .spawn_unit(kind, owner, at(x, y))
        .expect("invariant: a free spot")
}

/// Spawns `slug` for `owner` at `(x, y)` holding position, so it neither flees nor walks.
fn held(world: &mut SimWorld, slug: &str, owner: PlayerId, x: f32, y: f32) -> EntityId {
    let id = spawn(world, slug, owner, x, y);
    order(world, id, UnitOrder::HoldPosition);
    id
}

fn order(world: &mut SimWorld, id: EntityId, order: UnitOrder) {
    world
        .entities
        .get_mut(&id)
        .expect("alive")
        .issue_order(order, false)
        .expect("order admitted");
}

/// Spawns a Viper analogue for blue at `(x, y)` with full energy.
fn viper(world: &mut SimWorld, x: f32, y: f32) -> EntityId {
    let id = spawn(world, "tongue_chameleon", BLUE, x, y);
    world.entities.get_mut(&id).expect("spawned").energy = n(200);
    id
}

/// Spawns a cloaked Dark Templar analogue for red at `(x, y)`, holding position.
fn cloaked(world: &mut SimWorld, x: f32, y: f32) -> EntityId {
    let id = held(world, "shadow_visitor", RED, x, y);
    world.entities.get_mut(&id).expect("spawned").is_cloaked = true;
    id
}

/// Spawns the shipped structure `slug` for `owner` with its corner on cell `(x, y)`.
fn structure(world: &mut SimWorld, slug: &str, owner: PlayerId, x: u32, y: u32) -> EntityId {
    let kind = world.catalog.id_by_slug(slug).expect("a shipped structure");
    world
        .spawn_structure(kind, owner, x, y)
        .expect("a free site")
}

/// The index of Blinding Cloud in `caster`'s archetype.
fn cloud_index(world: &SimWorld, caster: EntityId) -> u8 {
    let kind = world.entities[&caster].unit_type_id;
    let arch = world.catalog.get(kind).expect("registered");
    let index = arch
        .abilities
        .iter()
        .position(|ability| ability.name == CLOUD)
        .expect("the Viper analogue has Blinding Cloud");
    u8::try_from(index).expect("few abilities")
}

/// Casts Blinding Cloud by `caster` at `(x, y)` now; whether it was admitted.
fn cloud(world: &mut SimWorld, caster: EntityId, x: f32, y: f32) -> bool {
    let index = cloud_index(world, caster);
    world.execute_ability(caster, index, Some(at(x, y)), None)
}

/// Steps one tick; panics on a diagnostic.
fn step(world: &mut SimWorld) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands: Vec::new(),
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

fn steps(world: &mut SimWorld, count: u32) {
    for _ in 0..count {
        step(world);
    }
}

/// Whether `id` carries the shipped status `name`.
fn has(world: &SimWorld, id: EntityId, name: &str) -> bool {
    let status = world
        .catalog
        .spells()
        .status_id(name)
        .expect("a shipped status");
    world.effects().has_status(id, status)
}

fn blinded(world: &SimWorld, id: EntityId) -> bool {
    has(world, id, "blinding_cloud")
}

fn archetype(world: &SimWorld, id: EntityId) -> UnitArchetype {
    let kind = world.entities[&id].unit_type_id;
    world.catalog.get(kind).expect("registered").clone()
}

/// The weapon `id` fires at ground targets now: a mode's weapon, else its ground weapon, else
/// its single weapon.
fn ground_weapon(world: &SimWorld, id: EntityId) -> WeaponSpec {
    let entity = &world.entities[&id];
    if let Some(weapon) = entity.active_siege_weapon() {
        return weapon.clone();
    }
    let arch = archetype(world, id);
    arch.ground_weapon.or(arch.weapon).expect("an armed unit")
}

/// The range of `weapon` on `id` now, its statuses included.
fn range(world: &SimWorld, id: EntityId, weapon: &WeaponSpec) -> SimScalar {
    let entity = &world.entities[&id];
    let arch = archetype(world, id);
    world.entity_weapon_range(entity, weapon, &arch.slug)
}

/// Life plus shields of `id`, 0 once it is gone.
fn vitals(world: &SimWorld, id: EntityId) -> SimScalar {
    world
        .entities
        .get(&id)
        .map_or(SimScalar::ZERO, |e| e.hp + e.shields)
}

/// The index of `id`'s ability that switches it into a mode that is not burrowed (siege).
fn siege_index(world: &SimWorld, id: EntityId) -> u8 {
    let arch = archetype(world, id);
    let index = arch
        .abilities
        .iter()
        .position(|ability| match &ability.effect {
            AbilityEffectSpec::SwitchMode { mode_name } => {
                arch.get_mode(mode_name).is_some_and(|mode| !mode.burrowed)
            }
            _ => false,
        })
        .expect("a siege ability");
    u8::try_from(index).expect("few abilities")
}

/// Sieges `id` and steps until the mode has settled.
fn siege(world: &mut SimWorld, id: EntityId) {
    let index = siege_index(world, id);
    assert!(world.execute_ability(id, index, None, None), "mode switch");
    let settled = (0..120).any(|_| {
        step(world);
        let entity = &world.entities[&id];
        entity.active_mode.is_some() && entity.channeling_remaining_ticks == 0
    });
    assert!(settled, "the siege settles within 120 ticks");
}

#[test]
fn the_cloud_searches_sixteen_times_on_the_plan_ticks_and_refreshes_one_status() {
    let mut world = world();
    let caster = viper(&mut world, 8.0, 20.0);
    let inside = held(&mut world, "pulse_bot", BLUE, 16.0, 20.0);
    let start = world.current_tick;
    assert!(cloud(&mut world, caster, 16.0, 20.0));
    let status = world
        .catalog
        .spells()
        .status_id("blinding_cloud")
        .expect("shipped");
    let entry = |world: &SimWorld| {
        let entries: Vec<_> = world
            .effects()
            .statuses_of(inside)
            .iter()
            .filter(|entry| entry.status == status)
            .copied()
            .collect();
        assert!(entries.len() <= 1, "one status, refreshed, never stacked");
        entries.first().copied()
    };
    let first = entry(&world).expect("the first search runs on the cast tick");
    let mut searches = vec![0];
    let mut last = first.expires_at;
    let mut gone = None;
    for _ in 0..160 {
        step(&mut world);
        let elapsed = world.current_tick - start;
        match entry(&world) {
            Some(now) => {
                assert_eq!(now.applied_tick, first.applied_tick, "refreshed in place");
                if now.expires_at != last {
                    searches.push(elapsed);
                    last = now.expires_at;
                }
            }
            None => {
                gone.get_or_insert(elapsed);
            }
        }
    }
    assert_eq!(searches, SEARCHES, "16 searches at round(k x 8.929)");
    assert_eq!(
        gone,
        Some(SEARCHES[15] + LINGER),
        "the status outlives the last search by 11.16 ticks"
    );
    assert!(world.effects().persistents().is_empty(), "the cloud closed");
}

#[test]
fn ground_units_of_every_owner_are_hit_and_air_units_and_the_caster_are_not() {
    let mut world = world();
    let caster = viper(&mut world, 15.0, 20.0);
    let own = held(&mut world, "pulse_bot", BLUE, 16.0, 21.0);
    let enemy = held(&mut world, "pulse_bot", RED, 17.0, 19.0);
    let air = held(&mut world, "vector_interceptor", RED, 16.5, 20.5);
    let cloaked = cloaked(&mut world, 15.5, 19.0);
    let near = held(&mut world, "shock_golem", RED, 17.9, 20.0);
    let far = held(&mut world, "shock_golem", RED, 16.0, 22.4);
    // The caster hovers over the cloud's edge, 1 from its centre.
    assert!(cloud(&mut world, caster, 16.0, 20.0));
    assert!(blinded(&world, own), "own units are hit");
    assert!(blinded(&world, enemy), "enemy units are hit");
    assert!(blinded(&world, cloaked), "a cloaked unit is hit undetected");
    assert!(blinded(&world, near), "1.9 from the centre is inside");
    assert!(!blinded(&world, far), "2.4 from the centre is outside");
    assert!(!blinded(&world, air), "air units are never hit");
    assert!(!blinded(&world, caster), "nor the caster itself");
    let snapshot = world.build_render_snapshot();
    let flags = |id: EntityId| {
        snapshot
            .bubbles
            .iter()
            .find(|bubble| bubble.entity_id == id)
            .expect("published")
            .status
    };
    assert!(flags(enemy).has(StatusKind::RangeReduced));
    assert!(!flags(far).has(StatusKind::RangeReduced));
}

#[test]
fn ranged_weapons_drop_to_the_floor_and_melee_weapons_keep_their_range() {
    let mut world = world();
    let caster = viper(&mut world, 8.0, 20.0);
    let marine = held(&mut world, "pulse_bot", BLUE, 16.0, 20.0);
    let zergling = held(&mut world, "razor_mantis", BLUE, 16.0, 21.0);
    let rifle = ground_weapon(&world, marine);
    let claws = ground_weapon(&world, zergling);
    assert!(!rifle.melee && claws.melee, "Options Melee, plan D-14");
    let full = [
        range(&world, marine, &rifle),
        range(&world, zergling, &claws),
    ];
    assert_eq!(full[0], n(5));
    assert!(cloud(&mut world, caster, 16.0, 20.5));
    assert!(blinded(&world, marine) && blinded(&world, zergling));
    assert_eq!(range(&world, marine, &rifle), floor(), "5 - 20, floored");
    assert_eq!(
        range(&world, zergling, &claws),
        full[1],
        "RangedWeaponRange spares melee weapons (voidmulti BehaviorData:1068)"
    );
}

#[test]
fn a_blinded_marine_cannot_hit_a_target_one_away_but_hits_one_in_contact() {
    let mut world = world();
    let caster = viper(&mut world, 8.0, 20.0);
    let marine = held(&mut world, "pulse_bot", BLUE, 16.0, 20.0);
    // The Infestor analogue has no weapon, so it never answers. Edges 1 apart.
    let gap = SimScalar::ONE
        + archetype(&world, marine).collision_radius
        + world
            .catalog
            .get(
                world
                    .catalog
                    .id_by_slug("mycelium_shaman")
                    .expect("shipped"),
            )
            .expect("registered")
            .collision_radius;
    let x = 16.0 + gap.to_f32();
    let target = held(&mut world, "mycelium_shaman", RED, x, 20.0);
    // Cast before the first tick, so the marine never fires at full range.
    assert!(cloud(&mut world, caster, 16.0, 20.0));
    let full = vitals(&world, target);
    steps(&mut world, 60);
    assert!(blinded(&world, marine));
    assert_eq!(
        vitals(&world, target),
        full,
        "1 away is beyond the 0.1 floor"
    );

    // A zergling runs into the cloud and strikes the marine in contact; the marine answers.
    let zergling = spawn(&mut world, "razor_mantis", RED, 16.0, 23.0);
    order(
        &mut world,
        zergling,
        UnitOrder::AttackTarget {
            target: marine,
            origin: AttackOrigin::Explicit,
        },
    );
    let start = vitals(&world, zergling);
    steps(&mut world, 60);
    assert!(blinded(&world, marine) && blinded(&world, zergling));
    assert!(
        vitals(&world, marine) < n(45),
        "the zergling's melee weapon keeps its range"
    );
    assert!(
        vitals(&world, zergling) < start,
        "the blinded marine still hits in contact"
    );
    assert_eq!(vitals(&world, target), full);
}

#[test]
fn a_unit_outside_shooting_into_the_cloud_keeps_its_range() {
    let mut world = world();
    let caster = viper(&mut world, 8.0, 24.0);
    let shooter = held(&mut world, "pulse_bot", BLUE, 12.0, 20.0);
    let target = held(&mut world, "mycelium_shaman", RED, 16.0, 20.0);
    let rifle = ground_weapon(&world, shooter);
    let full = vitals(&world, target);
    steps(&mut world, 2);
    assert!(cloud(&mut world, caster, 16.5, 20.0));
    assert!(blinded(&world, target) && !blinded(&world, shooter));
    assert_eq!(range(&world, shooter, &rifle), n(5));
    steps(&mut world, 40);
    assert!(vitals(&world, target) < full, "shot from outside the cloud");
}

#[test]
fn a_unit_leaving_the_cloud_keeps_the_status_twelve_ticks_after_the_last_search_that_found_it() {
    let mut world = world();
    let caster = viper(&mut world, 8.0, 24.0);
    let marine = spawn(&mut world, "pulse_bot", BLUE, 17.5, 20.0);
    let rifle = ground_weapon(&world, marine);
    assert!(cloud(&mut world, caster, 16.0, 20.0));
    assert!(blinded(&world, marine), "1.5 from the centre on the cast");
    order(
        &mut world,
        marine,
        UnitOrder::Move {
            target: at(30.0, 20.0),
        },
    );
    // The second search, 9 ticks on, finds it beyond 2: only the first search found it.
    for elapsed in 1..=40 {
        step(&mut world);
        let distance = world.entities[&marine].position.x - SimScalar::from_num(16.0);
        if elapsed == SEARCHES[1] {
            assert!(distance > n(2), "out of the cloud by its second search");
        }
        if elapsed < LINGER {
            assert!(
                blinded(&world, marine),
                "tick {elapsed}: the status lingers"
            );
            assert_eq!(range(&world, marine, &rifle), floor());
        } else {
            assert!(!blinded(&world, marine), "tick {elapsed}: the status ended");
            assert_eq!(range(&world, marine, &rifle), n(5), "range restored");
        }
    }
}

#[test]
fn a_sieged_tank_in_the_cloud_cannot_fire_at_any_distance() {
    let mut world = world();
    let caster = viper(&mut world, 8.0, 24.0);
    let tank = held(&mut world, "railgun_crawler", BLUE, 16.0, 20.0);
    siege(&mut world, tank);
    let cannon = ground_weapon(&world, tank);
    assert!(
        cannon.min_range > floor(),
        "the sieged weapon has a minimum range"
    );
    assert!(cloud(&mut world, caster, 16.0, 20.0));
    let cast = world.current_tick;
    assert_eq!(range(&world, tank, &cannon), floor(), "13 - 20, floored");
    // Unarmed targets: one beyond the minimum range, one in contact.
    let far = held(&mut world, "mycelium_shaman", RED, 21.0, 20.0);
    let close = held(&mut world, "mycelium_shaman", RED, 16.0, 21.5);
    let full = [far, close].map(|id| vitals(&world, id));
    let ends = SEARCHES[15] + LINGER;
    while world.current_tick - cast < ends {
        step(&mut world);
        assert!(blinded(&world, tank) || world.current_tick - cast >= ends);
    }
    assert_eq!(
        [far, close].map(|id| vitals(&world, id)),
        full,
        "the reduced range lies inside the minimum range: no shot while clouded"
    );
    steps(&mut world, 40);
    assert!(
        vitals(&world, far) < full[0],
        "the range is back once the status ends"
    );
    assert_eq!(vitals(&world, close), full[1], "inside the minimum range");
}

#[test]
fn a_structure_loses_every_weapon_range_and_its_detection() {
    let mut world = world();
    let caster = viper(&mut world, 8.0, 24.0);
    // A 2 × 2 Photon Cannon analogue on cells 16 and 17, centre (17, 21).
    let cannon = structure(&mut world, "tesla_sentinel_dome", BLUE, 16, 20);
    let arch = archetype(&world, cannon);
    let weapons = [
        arch.ground_weapon.clone().expect("a ground weapon"),
        arch.air_weapon.clone().expect("an air weapon"),
    ];
    let full = weapons.clone().map(|weapon| range(&world, cannon, &weapon));
    let hidden = cloaked(&mut world, 21.0, 21.0);
    steps(&mut world, 2);
    let mut view = PlayerView::default();
    world.write_player_view(BLUE, &mut view);
    assert_eq!(view.revealed, [hidden], "the detector reveals");
    assert!(cloud(&mut world, caster, 17.0, 21.0));
    assert!(has(&world, cannon, "blinding_cloud_structure"));
    assert!(!blinded(&world, cannon), "the structure branch only");
    for weapon in &weapons {
        assert_eq!(range(&world, cannon, weapon), floor(), "{}", weapon.name);
        let melee = WeaponSpec {
            melee: true,
            ..weapon.clone()
        };
        assert_eq!(
            range(&world, cannon, &melee),
            floor(),
            "WeaponRange -20 reaches melee weapons too"
        );
    }
    let start = vitals(&world, hidden);
    steps(&mut world, 2);
    world.write_player_view(BLUE, &mut view);
    assert_eq!(view.revealed, [], "no detection while clouded");
    steps(&mut world, 100);
    assert_eq!(vitals(&world, hidden), start, "no shot while clouded");
    steps(&mut world, SEARCHES[15] + LINGER - 102);
    assert!(!has(&world, cannon, "blinding_cloud_structure"));
    assert_eq!(weapons.clone().map(|w| range(&world, cannon, &w)), full);
    step(&mut world);
    world.write_player_view(BLUE, &mut view);
    assert_eq!(
        view.revealed,
        [hidden],
        "detects again once the status ends"
    );
}

#[test]
fn one_hundred_energy_range_ten_from_the_casters_edge_no_cooldown() {
    let mut world = world();
    let caster = viper(&mut world, 8.0, 20.0);
    let index = cloud_index(&world, caster);
    world.entities.get_mut(&caster).expect("alive").energy = SimScalar::from_num(99.9);
    assert!(!cloud(&mut world, caster, 16.0, 20.0), "needs 100 energy");
    world.entities.get_mut(&caster).expect("alive").energy = n(200);
    // 10 + the caster's radius 1.0.
    let reach = SimScalar::from_num(11.0);
    let inside = SimVec2::new(n(8) + reach, n(20));
    let outside = SimVec2::new(inside.x + SimScalar::from_num(0.01), n(20));
    assert!(!world.would_admit_cast(caster, index, Some(outside), None));
    assert!(world.execute_ability(caster, index, Some(inside), None));
    assert_eq!(world.entities[&caster].energy, n(100), "100 paid");
    assert_eq!(
        world.entities[&caster].ability_cooldowns[usize::from(index)],
        0,
        "no cooldown"
    );
    assert!(
        cloud(&mut world, caster, 16.0, 24.0),
        "a second cast at once"
    );
    assert_eq!(world.entities[&caster].energy, n(0));
    assert_eq!(world.effects().persistents().len(), 2);
}

#[test]
fn the_cloud_is_an_area_from_its_cast_to_its_last_search() {
    let mut world = world();
    let caster = viper(&mut world, 8.0, 20.0);
    let index = cloud_index(&world, caster);
    let kind = world.entities[&caster].unit_type_id;
    assert!(cloud(&mut world, caster, 16.0, 20.0));
    let mut published = 0;
    for _ in 0..160 {
        let snapshot = world.build_render_snapshot();
        let Some(area) = snapshot.areas.first() else {
            break;
        };
        assert!((area.radius - 2.0).abs() < 1e-6);
        assert_eq!(u32::from(area.total_ticks), SEARCHES[15]);
        assert_eq!(area.owner, BLUE);
        assert_eq!(area.center[..2], [16.0, 20.0]);
        assert_eq!(
            area.source,
            Some(AbilitySource {
                unit_type: kind,
                ability: index,
            })
        );
        published += 1;
        step(&mut world);
    }
    assert_eq!(published, SEARCHES[15], "shown until it closes at 134");
}
