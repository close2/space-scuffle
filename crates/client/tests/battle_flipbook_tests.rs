//! The battle animates its flipbooks on the presented simulation clock (plan
//! `docs/plans/2026-11-04-battle-flipbook-animation.md`, WP5).
//!
//! # Responsibility
//! Boots the production client on the shipped scenarios and proves, from the ECS state the GPU
//! reads, that each live bubble shows the layer the plan's display rules (D-5) ask for: the
//! layer a bubble shows is `flipbook_layer(FlipbookClock, lanes of its material)`, the Rust mirror
//! of the shader's frame function, whose parity with the GPU `flipbook_clock_gpu_tests.rs` proves.
//!
//! # Limits
//! Shares the limits of `support/battle_harness.rs`: no window and no renderer. The harness steps
//! 40 ms of manual time per frame, so each frame runs one tick and presents the previous one at
//! interpolation fraction 0, the regime of the capture tool.
//!
//! # Invariants
//! - Setup may fund a player or spawn a unit or structure through `world_mut`, as the harness
//!   allows. Every order under test arrives through an input event.
//! - Identities are found by capability or by the scenario's composition, except the attacker and
//!   target of the sustained-fire window, which name shipped catalog entries as test data.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use client::battle_hud::card::{CardAction, CardLabel, CommandCard};
use client::battle_input::command_mode::CommandMode;
use client::battle_pipeline::session::BattleSession;
use client::bubble_shader::{BubbleDomeMaterial, BubbleUniformGpu, FlipbookRegistry};
use client::flipbook_clock::{
    flipbook_layer, FlipbookClock, FlipbookLanes, FlipbookPhase, CLOCK_UNITS_PER_TICK,
};
use client::runtime_battle::{DEFAULT_BATTLE_SCENARIO, SKIRMISH_SCENARIO};
use client::scene_controller::RenderEntityMap;
use client::ClientMode;
use sim_types::{AnimState, AttackSwing, BubbleRenderInstance, EntityId, PlayerId, Tick};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{Harness, LOCAL_PLAYER};

/// Frames a battle gets to load its media before a test gives up.
const READY_FRAMES: usize = 600;

/// The opponent in both shipped scenarios.
const ENEMY: PlayerId = PlayerId(1);

/// Shipped identity of the sustained-fire attacker: armed, with an Attack slice.
const ATTACKER: &str = "pulse_bot";

/// Shipped unarmed structure with media: the sustained-fire target and the foundation built.
const TARGET: &str = "assembly_bay_dome";

/// Minimum corner cell of open plateau in the skirmish, clear of both players' starts.
const TARGET_CELL: u32 = 2;

/// More minerals than any test spends.
const RICH: u32 = 100_000;

/// Modifications of each bubble material, counted in `Last` from Bevy's asset events.
#[derive(Resource, Default)]
struct Modified(BTreeMap<AssetId<BubbleDomeMaterial>, u64>);

fn count_modified(
    mut events: EventReader<'_, '_, AssetEvent<BubbleDomeMaterial>>,
    mut modified: ResMut<'_, Modified>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            *modified.0.entry(*id).or_default() += 1;
        }
    }
}

/// Boots `scenario` paused, installs the modification counter and waits until every live bubble
/// is drawn with its media.
fn boot(mode: ClientMode, scenario: &str) -> Harness {
    let mut harness = Harness::boot_scenario(mode, scenario);
    harness
        .app
        .init_resource::<Modified>()
        .add_systems(Last, count_modified);
    let mut frames = 0;
    while !ready(&harness) {
        harness.step(1);
        frames += 1;
        assert!(frames < READY_FRAMES, "the battle never became ready");
    }
    harness.step(10);
    harness
}

fn ready(harness: &Harness) -> bool {
    let world = harness.app.world();
    let images = world.resource::<Assets<Image>>();
    let registry = world.resource::<FlipbookRegistry>();
    !registry.textures_by_id.is_empty()
        && registry
            .textures_by_id
            .values()
            .all(|image| images.contains(image))
        && world.resource::<BattleSession>().is_ready()
        && !world.resource::<RenderEntityMap>().0.is_empty()
}

/// The local player's harvesters, ordered by ID.
fn local_workers(harness: &Harness) -> Vec<EntityId> {
    let world = harness.world();
    world
        .entities
        .values()
        .filter(|unit| {
            unit.owner == LOCAL_PLAYER
                && world
                    .catalog
                    .get(unit.unit_type_id)
                    .is_ok_and(|archetype| archetype.capabilities.harvester.is_some())
        })
        .map(|unit| unit.id)
        .collect()
}

/// The material handle of unit `id`'s live bubble.
fn material(harness: &Harness, id: EntityId) -> AssetId<BubbleDomeMaterial> {
    let world = harness.app.world();
    let entity = *world
        .resource::<RenderEntityMap>()
        .0
        .get(&id)
        .expect("the unit has a live bubble");
    world
        .get::<MeshMaterial3d<BubbleDomeMaterial>>(entity)
        .expect("a live bubble has a material")
        .0
        .id()
}

/// The uniform unit `id`'s material holds now.
fn uniform(harness: &Harness, id: EntityId) -> BubbleUniformGpu {
    harness
        .app
        .world()
        .resource::<Assets<BubbleDomeMaterial>>()
        .get(material(harness, id))
        .expect("bubble material")
        .uniform
}

fn clock(harness: &Harness) -> u32 {
    harness.app.world().resource::<FlipbookClock>().now
}

fn modifications(harness: &Harness, id: EntityId) -> u64 {
    let handle = material(harness, id);
    harness
        .app
        .world()
        .resource::<Modified>()
        .0
        .get(&handle)
        .copied()
        .unwrap_or(0)
}

/// The presented record of unit `id`, if it is presented.
fn try_record(harness: &Harness, id: EntityId) -> Option<BubbleRenderInstance> {
    harness
        .app
        .world()
        .resource::<BattleSession>()
        .frames
        .presented()
        .bubbles
        .iter()
        .find(|bubble| bubble.entity_id == id)
        .cloned()
}

/// The presented record of unit `id`.
fn record(harness: &Harness, id: EntityId) -> BubbleRenderInstance {
    try_record(harness, id).expect("the unit is presented")
}

fn presented_tick(harness: &Harness) -> Tick {
    harness
        .app
        .world()
        .resource::<BattleSession>()
        .frames
        .presented()
        .tick
}

/// The flipbook phase of unit `id`'s live bubble.
fn phase(harness: &Harness, id: EntityId) -> FlipbookPhase {
    let world = harness.app.world();
    let entity = *world
        .resource::<RenderEntityMap>()
        .0
        .get(&id)
        .expect("the unit has a live bubble");
    *world
        .get::<FlipbookPhase>(entity)
        .expect("a live bubble has a flipbook phase")
}

/// What one live bubble draws on one frame: its phase and its whole material uniform.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Observed {
    phase: FlipbookPhase,
    lanes: FlipbookLanes,
    uniform: BubbleUniformGpu,
}

/// The clock and every live bubble's phase and uniform.
fn observe_all(harness: &Harness) -> (u32, BTreeMap<EntityId, Observed>) {
    let ids: Vec<EntityId> = harness
        .app
        .world()
        .resource::<RenderEntityMap>()
        .0
        .keys()
        .copied()
        .collect();
    let bubbles = ids
        .into_iter()
        .map(|id| {
            let uniform = uniform(harness, id);
            (
                id,
                Observed {
                    phase: phase(harness, id),
                    lanes: uniform.flipbook_lanes(),
                    uniform,
                },
            )
        })
        .collect();
    (clock(harness), bubbles)
}

/// Runs the default battle for `ticks` ticks at `frame` of manual time per frame, and returns
/// what every live bubble drew at each presented tick.
fn record_run(frame: Duration, ticks: u32) -> BTreeMap<Tick, BTreeMap<EntityId, Observed>> {
    let mut harness = boot(ClientMode::LiveBattle, DEFAULT_BATTLE_SCENARIO);
    harness
        .app
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame));
    harness.resume();
    let mut run = BTreeMap::new();
    while harness.world().current_tick < ticks {
        harness.step(1);
        run.insert(presented_tick(&harness), observe_all(&harness).1);
    }
    run
}

/// Steps the resumed default battle until a local unit whose atlas has an Attack slice is in a
/// swing, and returns it.
fn first_swing(harness: &mut Harness) -> EntityId {
    for _ in 0..2_000 {
        harness.step(1);
        let registry = harness.app.world().resource::<FlipbookRegistry>();
        let found = harness
            .app
            .world()
            .resource::<BattleSession>()
            .frames
            .presented()
            .bubbles
            .iter()
            .find(|bubble| {
                bubble.owner == LOCAL_PLAYER
                    && bubble.attack_swing.is_some()
                    && registry
                        .get_meta(bubble.unit_type)
                        .is_some_and(|meta| meta.actions.contains_key(&AnimState::Attack))
            })
            .map(|bubble| bubble.entity_id);
        if let Some(id) = found {
            return id;
        }
    }
    panic!("no local unit with an Attack slice ever swung");
}

fn card(harness: &Harness) -> &CommandCard {
    harness.app.world().resource::<CommandCard>()
}

fn position_of(harness: &Harness, wanted: impl Fn(&CardAction) -> bool) -> Option<usize> {
    card(harness)
        .slots
        .iter()
        .position(|slot| slot.as_ref().is_some_and(|slot| wanted(&slot.action)))
}

/// Taps the key of the card slot labelled `label`.
fn tap_label(harness: &mut Harness, label: CardLabel) {
    let position = position_of_label(harness, label);
    let key = card(harness).slots[position]
        .as_ref()
        .expect("a filled slot")
        .key;
    harness.tap_key(key);
}

fn position_of_label(harness: &Harness, label: CardLabel) -> usize {
    card(harness)
        .slots
        .iter()
        .position(|slot| slot.as_ref().is_some_and(|slot| slot.label == label))
        .unwrap_or_else(|| panic!("{label:?} is not on the card: {:?}", card(harness).slots))
}

/// First atlas layer and layer count of `action` in unit `id`'s atlas.
fn slice(harness: &Harness, id: EntityId, action: AnimState) -> (u32, u32) {
    let unit_type = record(harness, id).unit_type;
    let meta = harness
        .app
        .world()
        .resource::<FlipbookRegistry>()
        .get_meta(unit_type)
        .expect("the unit has an atlas");
    let slice = meta.actions.get(&action).expect("the atlas has the slice");
    (slice.start_frame, slice.frame_count)
}

// --- Tests ---------------------------------------------------------------------------------------

/// The committed form of the plan's §2.6 probe: an idle worker plays its Idle slice in order.
#[test]
fn idle_unit_frames_advance() {
    let mut harness = boot(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = local_workers(&harness)[0];
    let (start, count) = slice(&harness, worker, AnimState::Idle);
    harness.resume();
    harness.step(5);
    let before = modifications(&harness, worker);
    // Four seconds of game time at one tick per frame.
    let mut shown: Vec<(u32, u32)> = Vec::new();
    for _ in 0..100 {
        harness.step(1);
        assert_eq!(record(&harness, worker).anim_state, AnimState::Idle);
        let now = clock(&harness);
        let layer = flipbook_layer(now, &uniform(&harness, worker).flipbook_lanes());
        shown.push((now, layer));
    }
    let excerpt: Vec<_> = shown.iter().take(12).collect();
    println!("idle worker (clock, layer), first 12 frames: {excerpt:?}");
    // Runs of equal layers, in order of appearance.
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &(now, layer) in &shown {
        if runs.last().is_none_or(|&(_, last)| last != layer) {
            runs.push((now, layer));
        }
    }
    for pair in runs.windows(2) {
        let (from, to) = (pair[0].1, pair[1].1);
        assert_eq!(
            to - start,
            (from - start + 1) % count,
            "layers advance one at a time, in order: {runs:?}"
        );
    }
    let seen: BTreeSet<u32> = runs.iter().map(|&(_, layer)| layer).collect();
    assert_eq!(
        seen,
        (start..start + count).collect(),
        "every layer of the Idle slice is shown: {runs:?}"
    );
    let period = uniform(&harness, worker).anim_period;
    let step = f64::from(period) / f64::from(count);
    for pair in runs.windows(2).skip(1) {
        let lasted = f64::from(pair[1].0 - pair[0].0);
        assert!(
            (lasted - step).abs() <= 1024.0,
            "each layer shows for period / count = {step} units within one tick, saw {lasted}"
        );
    }
    assert_eq!(
        modifications(&harness, worker) - before,
        0,
        "time alone modifies no material"
    );
}

/// A unit ordered to move shows its Walk slice's first layer on the first presented tick of
/// `Walk`, and the second layer `period / count` later.
#[test]
fn state_change_resets_the_loop() {
    let mut harness = boot(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = local_workers(&harness)[0];
    let (start, count) = slice(&harness, worker, AnimState::Walk);
    harness.look_at_unit(worker);
    harness.resume();
    // Let the idle loop run, so its phase is not the walk's by accident.
    harness.step(7);
    harness.select(worker);
    let destination = harness.open_ground(harness.simulated_position(worker), 6.0);
    let on_screen = harness.ground_on_screen(destination);
    harness.click(MouseButton::Right, on_screen);
    let mut frames = 0;
    while record(&harness, worker).anim_state != AnimState::Walk {
        harness.step(1);
        frames += 1;
        assert!(frames < 100, "the worker never walked");
    }
    let began = presented_tick(&harness);
    assert_eq!(
        phase(&harness, worker),
        FlipbookPhase::new(AnimState::Walk, began)
    );
    let lanes = uniform(&harness, worker).flipbook_lanes();
    assert_eq!(
        flipbook_layer(clock(&harness), &lanes),
        start,
        "the first presented tick of Walk shows the slice's first layer"
    );
    let step = lanes.period.div_ceil(count);
    let origin = began.0 * CLOCK_UNITS_PER_TICK;
    loop {
        harness.step(1);
        assert_eq!(record(&harness, worker).anim_state, AnimState::Walk);
        assert_eq!(
            phase(&harness, worker).since,
            began,
            "the walk keeps its start"
        );
        let now = clock(&harness);
        let layer = flipbook_layer(now, &uniform(&harness, worker).flipbook_lanes());
        if now - origin < step {
            assert_eq!(layer, start, "the first layer lasts period / count");
        } else {
            assert_eq!(layer, start + 1, "then the second layer shows");
            break;
        }
    }
}

/// On the default battle, every release of one attacker shows its Attack slice's first layer,
/// the tick before it the last layer, and the whole cycle displays `Attack`.
#[test]
fn attack_release_shows_the_release_frame() {
    let mut harness = boot(ClientMode::LiveBattle, DEFAULT_BATTLE_SCENARIO);
    // The window pins one fixed period per cycle; the random firing delay (WP11, B-3) spreads it.
    sim_fixtures::without_random_delays(&mut harness.world_mut().catalog);
    harness.resume();
    let attacker = first_swing(&mut harness);
    let (start, count) = slice(&harness, attacker, AnimState::Attack);
    let mut releases = 0;
    let mut previous: Option<(u32, AttackSwing)> = None;
    for _ in 0..600 {
        harness.step(1);
        let Some(bubble) = try_record(&harness, attacker) else {
            break;
        };
        let layer = flipbook_layer(
            clock(&harness),
            &uniform(&harness, attacker).flipbook_lanes(),
        );
        // A unit that walks during its cooldown (a stutter step) displays Walk (D-5); the
        // window checks the cycles it stands through.
        let standing = matches!(bubble.anim_state, AnimState::Attack | AnimState::Idle);
        if let Some(swing) = bubble.attack_swing.filter(|_| standing) {
            assert_eq!(
                phase(&harness, attacker).action,
                AnimState::Attack,
                "the whole cycle displays Attack, cooldown included: {swing:?}"
            );
            assert_eq!(
                uniform(&harness, attacker).anim_state,
                AnimState::Attack as u32
            );
            if swing.elapsed_ticks == swing.damage_point_ticks {
                releases += 1;
                assert_eq!(layer, start, "the release tick shows the first layer");
                if let Some((before, _)) =
                    previous.filter(|_| u32::from(swing.period_ticks) > count)
                {
                    assert_eq!(before, start + count - 1, "the tick before shows the last");
                }
            }
            previous = Some((layer, swing));
        } else {
            previous = None;
        }
    }
    assert!(releases >= 3, "the attacker released only {releases} times");
}

/// A stationary attacker firing at an unarmed structure: over ten consecutive swings at one
/// period its material is not modified once, because the lanes repeat (D-5).
#[test]
fn sustained_fire_changes_no_material() {
    let mut harness = boot(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    // Ten swings at one period: the random firing delay (WP11, B-3) would vary it.
    sim_fixtures::without_random_delays(&mut harness.world_mut().catalog);
    let attacker_type = harness
        .world()
        .catalog
        .id_by_slug(ATTACKER)
        .expect("the catalog ships the attacker");
    let target_type = harness
        .world()
        .catalog
        .id_by_slug(TARGET)
        .expect("the catalog ships the target");
    let target = harness
        .world_mut()
        .spawn_structure(target_type, ENEMY, TARGET_CELL, TARGET_CELL)
        .expect("the corner plateau is open");
    let at = harness.open_ground(harness.simulated_position(target), 3.0);
    let attacker = harness
        .world_mut()
        .spawn_unit(
            attacker_type,
            LOCAL_PLAYER,
            battle_harness::sim_position(at),
        )
        .expect("open ground beside the structure");
    harness.look_at_unit(target);
    harness.resume();
    let mut frames = 0;
    // The first swing begins and the attacker settles into its stance.
    while try_record(&harness, attacker).is_none_or(|bubble| bubble.attack_swing.is_none()) {
        harness.step(1);
        frames += 1;
        assert!(
            frames < 200,
            "the attacker never fired: {}",
            harness.status()
        );
    }
    harness.step(1);
    let before = modifications(&harness, attacker);
    let lanes = uniform(&harness, attacker);
    let period = record(&harness, attacker)
        .attack_swing
        .expect("in a swing")
        .period_ticks;
    let mut swings = 0;
    while swings < 10 {
        harness.step(1);
        let bubble = record(&harness, attacker);
        let swing = bubble.attack_swing.expect("sustained fire leaves no gap");
        assert_eq!(swing.period_ticks, period, "one period throughout");
        if swing.elapsed_ticks == 0 {
            swings += 1;
        }
        assert_eq!(uniform(&harness, attacker), lanes, "the uniform repeats");
    }
    assert!(
        harness.world().entities.contains_key(&target),
        "the target outlasts the window"
    );
    assert_eq!(
        modifications(&harness, attacker) - before,
        0,
        "ten swings modified the attacker's material"
    );
}

/// A foundation shows the layer of its build progress on every tick, and holds it while no
/// worker builds.
#[test]
fn construction_follows_progress() {
    let mut harness = boot(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = local_workers(&harness)[0];
    let structure = harness
        .world()
        .catalog
        .id_by_slug(TARGET)
        .expect("the catalog ships the structure");
    harness
        .world_mut()
        .player_resources
        .get_mut(LOCAL_PLAYER)
        .expect("invariant: LOCAL_PLAYER has a player slot")
        .minerals = RICH;
    let site = Vec2::new(TARGET_CELL as f32 + 1.5, TARGET_CELL as f32 + 1.5);
    let worker_at = harness.simulated_position(worker);
    harness.look_at((worker_at + site) * 0.5);
    harness.select(worker);
    tap_label(&mut harness, CardLabel::Build);
    let wanted = CardAction::Mode(CommandMode::Place(structure));
    let position = loop {
        if let Some(position) = position_of(&harness, |action| *action == wanted) {
            break position;
        }
        tap_label(&mut harness, CardLabel::More);
    };
    let key = card(&harness).slots[position]
        .as_ref()
        .expect("a filled slot")
        .key;
    harness.tap_key(key);
    let on_screen = harness.ground_on_screen(site);
    harness.click(MouseButton::Left, on_screen);
    harness.resume();
    let mut frames = 0;
    let foundation = loop {
        harness.step(1);
        if let Some(id) = harness
            .app
            .world()
            .resource::<BattleSession>()
            .frames
            .presented()
            .bubbles
            .iter()
            .find(|bubble| bubble.anim_state == AnimState::Constructing)
            .map(|bubble| bubble.entity_id)
        {
            break id;
        }
        frames += 1;
        assert!(frames < 900, "no foundation was laid: {}", harness.status());
    };
    let (start, count) = slice(&harness, foundation, AnimState::Constructing);
    let expected = |progress: f32| {
        start + ((f64::from(progress) * f64::from(count)).floor() as u32).min(count - 1)
    };
    let mut layers = BTreeSet::new();
    for _ in 0..120 {
        harness.step(1);
        let bubble = record(&harness, foundation);
        let progress = bubble
            .build_or_queue_progress
            .expect("a foundation has progress");
        let lanes = uniform(&harness, foundation).flipbook_lanes();
        let layer = flipbook_layer(clock(&harness), &lanes);
        assert_eq!(layer, expected(progress), "progress {progress}");
        assert_eq!(
            lanes,
            FlipbookLanes::fixed(layer),
            "time alone moves nothing"
        );
        layers.insert(layer);
    }
    assert!(layers.len() > 1, "the foundation advanced: {layers:?}");
    // The builder leaves: the foundation stalls and keeps its layer.
    harness.select(worker);
    let away = harness.open_ground(harness.simulated_position(worker), 8.0);
    let on_screen = harness.ground_on_screen(away);
    harness.click(MouseButton::Right, on_screen);
    harness.step(40);
    let stalled = record(&harness, foundation);
    let layer = flipbook_layer(
        clock(&harness),
        &uniform(&harness, foundation).flipbook_lanes(),
    );
    let before = modifications(&harness, foundation);
    harness.step(30);
    assert_eq!(
        record(&harness, foundation).build_or_queue_progress,
        stalled.build_or_queue_progress,
        "nobody builds"
    );
    assert_eq!(
        flipbook_layer(
            clock(&harness),
            &uniform(&harness, foundation).flipbook_lanes()
        ),
        layer
    );
    assert_eq!(modifications(&harness, foundation), before);
}

/// Guard: paused frames leave the clock and every lane as they are.
#[test]
fn paused_frames_freeze_the_clock() {
    let mut harness = boot(ClientMode::LiveBattle, DEFAULT_BATTLE_SCENARIO);
    harness.resume();
    harness.step(80);
    harness.pause();
    let frozen = observe_all(&harness);
    harness.step(100);
    assert_eq!(
        observe_all(&harness),
        frozen,
        "a paused battle animates nothing"
    );
}

/// At speed 2, one second of manual time advances the clock by 50 ticks.
#[test]
fn speed_scales_the_clock() {
    let mut harness = boot(ClientMode::LiveBattle, DEFAULT_BATTLE_SCENARIO);
    harness
        .app
        .world_mut()
        .resource_mut::<client::LabState>()
        .simulation_speed = 2;
    harness.resume();
    harness.step(3);
    let before = clock(&harness);
    // 25 frames of 40 ms.
    harness.step(25);
    assert_eq!(clock(&harness) - before, 50 * CLOCK_UNITS_PER_TICK);
}

/// Ticks of the default battle the grouping test compares: the armies meet within them.
const GROUPING_TICKS: u32 = 1_200;

/// The same ticks at one and at five ticks per frame give the same Attack and Constructing
/// lanes, which come from the record alone (D-5).
///
/// A free loop's start is observed per rendered frame. Where the grouped run saw the action
/// begin, it starts at most four ticks (the ticks a frame skips) after the exact start. Where the
/// action was interrupted only between two grouped frames, the grouped run never saw the
/// interruption and keeps its earlier start: then every tick it presented since that start shows
/// the same action in the exact run too.
#[test]
fn frame_grouping_does_not_change_attack() {
    let one = record_run(Duration::from_millis(40), GROUPING_TICKS);
    let five = record_run(Duration::from_millis(200), GROUPING_TICKS);
    let (mut compared, mut attacks, mut late, mut unseen) = (0, 0, 0, 0);
    for (tick, grouped) in &five {
        let Some(single) = one.get(tick) else {
            continue;
        };
        for (id, observed) in grouped {
            let reference = single.get(id).expect("the same entities at the same tick");
            let action = observed.phase.action;
            assert_eq!(action, reference.phase.action, "{id:?} at {tick:?}");
            compared += 1;
            if matches!(action, AnimState::Attack | AnimState::Constructing) {
                attacks += u32::from(action == AnimState::Attack);
                assert_eq!(observed.lanes, reference.lanes, "{id:?} at {tick:?}");
                continue;
            }
            let (grouped_since, exact_since) = (observed.phase.since, reference.phase.since);
            if grouped_since >= exact_since {
                late += 1;
                assert!(
                    grouped_since.0 - exact_since.0 <= 4,
                    "{id:?} at {tick:?}: starts {grouped_since:?} and {exact_since:?}"
                );
            } else {
                unseen += 1;
                for (between, frame) in five.range(grouped_since..=*tick) {
                    assert_eq!(
                        frame.get(id).map(|seen| seen.phase.action),
                        Some(action),
                        "{id:?} at {between:?} in the grouped run"
                    );
                    assert_eq!(
                        one.get(between)
                            .and_then(|frame| frame.get(id))
                            .map(|seen| seen.phase.action),
                        Some(action),
                        "{id:?} at {between:?}: the grouped run missed a change it presented"
                    );
                }
            }
        }
    }
    println!(
        "{compared} (bubble, tick) pairs compared: {attacks} attacking, {late} free loops \
         within four ticks of the exact start, {unseen} kept across an unseen interruption"
    );
    assert!(
        compared > 0 && attacks > 0,
        "{compared} compared, {attacks} attacks"
    );
}

/// Two captures in the capture tool's regime (one tick per 40 ms frame, then frozen time) give
/// the same clock and lanes on every frame.
#[test]
fn capture_frames_are_reproducible() {
    let capture = || {
        let mut harness = boot(ClientMode::LiveBattle, DEFAULT_BATTLE_SCENARIO);
        harness.resume();
        let mut frames = Vec::new();
        for _ in 0..150 {
            harness.step(1);
            frames.push(observe_all(&harness));
        }
        harness
            .app
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        harness.pause();
        for _ in 0..10 {
            harness.step(1);
            frames.push(observe_all(&harness));
        }
        frames
    };
    let first = capture();
    let second = capture();
    assert_eq!(first.len(), second.len());
    for (frame, (a, b)) in first.iter().zip(&second).enumerate() {
        assert_eq!(a, b, "frame {frame}");
    }
}
