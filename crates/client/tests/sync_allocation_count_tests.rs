//! Counts the heap allocations of the live battle synchroniser's steady state from outside the
//! process, under valgrind (remediation Phase 6 exit 1, layer 3 of
//! `docs/plans/2026-11-03-close-phase-6-7-8-exits.md` D-2; finding C-15).
//!
//! **Responsibility.** Charter §3.3 says battle synchronisation allocates nothing in steady state.
//! The workspace forbids `unsafe_code`, so no counting `#[global_allocator]` can be installed
//! (owner question Q0011). Instead the parent test re-executes this test binary under
//! `valgrind --tool=dhat`, which replaces `malloc` from outside the process and records every
//! heap block with the call stack that allocated it. Every Rust allocation reaches `malloc` through
//! `std::alloc::System`, so the record is complete for the child process.
//!
//! **Why the window is scoped by call stack.** Whole-process totals (memcheck's
//! `total heap usage`) differ by about two blocks between identical runs: Bevy builds each
//! schedule's dependency graph with `std`'s randomly seeded `RandomState` during setup. That
//! noise is outside the frames under test, so the parent counts only the blocks whose stack passes
//! through `measured_steady_frames`, the non-inlined function that runs the measured frames.
//!
//! **How it works.** Each child builds the production live synchroniser on the real default battle
//! with `MinimalPlugins` (the app of `pipeline_presentation_tests.rs`), with every schedule on the
//! single-threaded executor, warms it, runs `F` paused frames and exits. The parent runs each child
//! at two frame counts and compares the window counts `W(F)` through the difference
//! `W(LONG_FRAMES) - W(SHORT_FRAMES)`, the allocations of `LONG_FRAMES - SHORT_FRAMES` frames:
//!
//! - `live_sync_frames` adds the client's battle systems (`advance_battle_ticks`,
//!   `update_flipbook_clock_system`, `sync_live_battle_system`, `publish_combat_events`);
//! - `baseline_frames` is the same app without them, so its difference is Bevy's own per-frame
//!   work (time, events, the schedule runner, asset bookkeeping), which is reported, not asserted;
//! - `planted_allocation_is_counted` is the full app plus one test system that allocates once per
//!   frame: the positive control, without which a counter that sees nothing would pass;
//! - `running_live_sync_frames` and `running_baseline_frames` are the same pair on the shipped
//!   skirmish, resumed, so the simulation ticks and the flipbook clock advances on every other
//!   frame. Its units stand idle, so nothing spawns, despawns or fires: a steady state that runs.
//!   The running baseline keeps the tick driver, so the difference between the two is the
//!   presentation systems' own work while time passes (plan `2026-11-04-battle-flipbook-animation`
//!   WP5).
//!   Three workers of the local player gather gas through an extractor that hides its harvesters
//!   in both running children, so during the window they go inside and come out, their bubbles
//!   published `housed` and back (plan `2026-11-24-hidden-harvesters` P-10); everyone else stands
//!   idle. Both running children also hold an attack snapshot: an enemy on a cliff struck a local
//!   unit below it before the session began, and the struck unit was then made invulnerable
//!   with the catalog's snapshot cap lifted, so the image is held and unchanged through the
//!   window; the running live child adds the view
//!   writer, the fog refill and the snapshot pool (plan `2026-11-24-attacker-snapshot` P-16).
//!   They hold a spell caster's image beside it: an enemy caster on another cliff stormed a
//!   local unit below it, and the storm ended before the session began, so the pool holds two
//!   images, one of each cause (plan `2026-11-27-launch-and-spell-visibility` V-25).
//! - `brood_cycle_live_frames` and `brood_cycle_baseline_frames` are the same pair on the default
//!   battle with every mobile unit killed and a Brood Lord analogue attacking a durable unarmed
//!   target, so Broodlings arrive and expire on their own clocks; one 40 ms tick a frame, a
//!   warm-up of `4 × L_max` ticks and windows of `2 × L_max` and `4 × L_max` ticks, `L_max`
//!   being the Broodling's longest life read from the data (plan `2026-11-29-render-entity-pool`
//!   R-1v). Their parent is `brood_cycle_frames_allocate_nothing`.
//! - `hangar_cycle_live_frames` and `hangar_cycle_baseline_frames` are the same pair with two
//!   Carriers cycling their interceptors (launch, attack, dock, relaunch), driven from the
//!   presented snapshot by a system both children run; three warm-up cycles, then windows of two
//!   and four cycles, each the last warm-up cycle's length (plan `2026-11-28-carrier-interceptors`
//!   H-20). Their parent is `hangar_cycle_frames_allocate_nothing`.
//! - `smart_mixed_live_frames` and `smart_mixed_baseline_frames` (the variant `smart_mixed`) are
//!   the same pair on the shipped skirmish, resumed, after one right click of a mixed selection on
//!   the local transport, planned by `smart_plan` and queued by `submit_smart` before the warm-up:
//!   two ground soldiers board it and an armed air unit follows it, so boarders walk and the
//!   follower flies through the window (plan `2026-12-01-rallies-and-smart-command` P-6). Their
//!   parent is `smart_mixed_frames_allocate_nothing`.
//! - `razing_cycle_live_frames` and `razing_cycle_baseline_frames` are the same pair with a
//!   weaponless enemy structure carrying the shipped death spawn with the most units raised and
//!   killed every 200 ticks by a system both children run, so its units arrive and expire on
//!   their own clocks; a warm-up of 800 ticks and windows of 400 and 800 ticks (plan
//!   `2026-12-01-death-spawns-and-hangar-commands` DS-18). Their parent is
//!   `razing_cycle_frames_allocate_nothing`.
//!
//! **Invariants asserted.** Two runs of the live child at the same frame count report the same
//! window count (the measurement is deterministic); the full app's difference equals the baseline's (the client's
//! sync systems add no allocation per frame), paused and running alike; the planted control adds exactly one allocation per
//! frame; and no recorded stack reached valgrind's depth limit, so no window block can hide in a
//! truncated stack.
//!
//! **Scope and limits.** It counts the paused steady state of the live synchroniser, not the whole
//! client frame: the renderer, egui, the multi-threaded executor and the simulation tick are
//! outside it. A missing `valgrind` fails the parent test with an instruction to install it; there
//! is no skip (charter §1.2). The children return at once unless `SPACE_SCUFFLE_STEADY_FRAMES` is
//! set, so a plain `cargo test` runs them as no-ops.

#[path = "support/effect_fixtures.rs"]
mod effect_fixtures;

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/attack_snapshot_stage.rs"]
mod attack_snapshot_stage;

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/unit_cycling.rs"]
mod unit_cycling;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use bevy::ecs::schedule::ExecutorKind;
use bevy::prelude::*;
use client::battle_pipeline::{
    commands::SessionGeneration,
    driver::{advance_battle_ticks, publish_combat_events},
    session::{BattleSession, CommittedBattleMetadata},
};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO, SKIRMISH_SCENARIO};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Environment variable that turns a child test on and carries its measured frame count.
const FRAMES_ENV: &str = "SPACE_SCUFFLE_STEADY_FRAMES";
/// Frames run before the measured window, so first-use growth (event queues, render-entity
/// buffers, schedule initialisation) is outside it.
const WARM_FRAMES: u32 = 64;
/// The shorter measured window, in frames.
const SHORT_FRAMES: u32 = 64;
/// The longer measured window, in frames.
const LONG_FRAMES: u32 = 128;
/// Name of the function whose frames delimit the measured window in a recorded call stack.
const WINDOW_FUNCTION: &str = "measured_steady_frames";
/// Call-stack depth DHAT records. The deepest stack of a child is about 115 frames; a stack that
/// reaches this limit is truncated and fails the test.
const STACK_DEPTH: usize = 256;

/// Which app a child builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    /// The production live synchroniser and battle driver.
    LiveSync,
    /// The same app without the client's battle systems.
    Baseline,
    /// The live synchroniser plus one planted allocation per frame.
    Planted,
    /// The live synchroniser and battle driver on the resumed, idle skirmish.
    RunningLive,
    /// The battle driver alone on the resumed, idle skirmish.
    RunningBaseline,
    /// The live synchroniser, a splash event every frame and the effect systems (plan
    /// `2026-11-09-effect-visualization` WP4, D-7), with a live area of the viewer's, an enemy
    /// area the viewer sees in part under a real `BattleFog`, and a live tether held every
    /// frame (WP10; review 1, E1-4).
    EffectStream,
    /// The same splash stream and live synchroniser without the effect systems.
    EffectBaseline,
    /// The live synchroniser and battle driver on the default battle with every mobile unit
    /// killed and a Brood Lord analogue attacking a durable unarmed target, so Broodlings arrive
    /// and expire on their own clocks (plan `2026-11-29-render-entity-pool` R-1v, B-19).
    BroodCycleLive,
    /// The battle driver alone on the same staged battle.
    BroodCycleBaseline,
    /// The live synchroniser and battle driver on the default battle with every mobile unit
    /// killed and two Carriers cycling their interceptors through launch, attack, dock and
    /// relaunch, driven from the snapshot (plan `2026-11-28-carrier-interceptors` H-20).
    HangarCycleLive,
    /// The battle driver and the cycle's driver alone on the same staged battle.
    HangarCycleBaseline,
    /// The live synchroniser and battle driver on the resumed skirmish after a mixed right click
    /// on the local transport: boarders walk to it and an air unit follows it (plan
    /// `2026-12-01-rallies-and-smart-command` P-6, the variant `smart_mixed`).
    SmartMixedLive,
    /// The battle driver alone after the same click.
    SmartMixedBaseline,
    /// The live synchroniser and battle driver on the default battle with every mobile unit
    /// killed and an enemy structure with a death spawn raised and killed every 200 ticks, so
    /// its units arrive and expire on their own clocks (plan
    /// `2026-12-01-death-spawns-and-hangar-commands` DS-18).
    RazingCycleLive,
    /// The battle driver and the razing driver alone on the same staged battle.
    RazingCycleBaseline,
}

impl Variant {
    /// Name of the child test that builds this variant, for `--exact`.
    fn child_test(self) -> &'static str {
        match self {
            Self::LiveSync => "live_sync_frames",
            Self::Baseline => "baseline_frames",
            Self::Planted => "planted_allocation_is_counted",
            Self::RunningLive => "running_live_sync_frames",
            Self::RunningBaseline => "running_baseline_frames",
            Self::EffectStream => "effect_stream_frames",
            Self::EffectBaseline => "effect_baseline_frames",
            Self::BroodCycleLive => "brood_cycle_live_frames",
            Self::BroodCycleBaseline => "brood_cycle_baseline_frames",
            Self::HangarCycleLive => "hangar_cycle_live_frames",
            Self::HangarCycleBaseline => "hangar_cycle_baseline_frames",
            Self::SmartMixedLive => "smart_mixed_live_frames",
            Self::SmartMixedBaseline => "smart_mixed_baseline_frames",
            Self::RazingCycleLive => "razing_cycle_live_frames",
            Self::RazingCycleBaseline => "razing_cycle_baseline_frames",
        }
    }

    /// Whether the child stages the Broodlings' battle, one 40 ms tick per frame.
    fn cycles_broods(self) -> bool {
        matches!(self, Self::BroodCycleLive | Self::BroodCycleBaseline)
    }

    /// Whether the child stages the Carriers' battle, one 40 ms tick per frame.
    fn cycles_hangars(self) -> bool {
        matches!(self, Self::HangarCycleLive | Self::HangarCycleBaseline)
    }

    /// Whether the child stages the razing battle, one 40 ms tick per frame.
    fn cycles_razing(self) -> bool {
        matches!(self, Self::RazingCycleLive | Self::RazingCycleBaseline)
    }

    /// Whether units arrive and leave on the child's own clocks, one 40 ms tick per frame.
    fn cycles_units(self) -> bool {
        self.cycles_broods() || self.cycles_hangars() || self.cycles_razing()
    }

    /// Whether the child plays the mixed right click of the variant `smart_mixed`.
    fn clicks_smart_mixed(self) -> bool {
        matches!(self, Self::SmartMixedLive | Self::SmartMixedBaseline)
    }

    /// Whether a splash event is sent every frame.
    fn streams_effects(self) -> bool {
        matches!(self, Self::EffectStream | Self::EffectBaseline)
    }

    /// Whether the battle runs during the window rather than staying paused.
    fn is_running(self) -> bool {
        matches!(self, Self::RunningLive | Self::RunningBaseline)
    }

    /// Whether the app carries the presentation systems under test.
    fn presents(self) -> bool {
        !matches!(
            self,
            Self::Baseline
                | Self::RunningBaseline
                | Self::BroodCycleBaseline
                | Self::HangarCycleBaseline
                | Self::SmartMixedBaseline
                | Self::RazingCycleBaseline
        )
    }
}

/// Workers in the gas crew of the running children.
const GAS_CREW: usize = 3;

/// The gas crew of the running children and how often, across all frames so far, one of them
/// went inside its extractor or came out, read from the presented bubbles' `housed` flag (plan
/// `2026-11-24-hidden-harvesters` P-10). Fixed-size, so watching it allocates nothing.
#[derive(Resource, Clone, Copy, Debug)]
struct GasCrew {
    ids: [sim_types::EntityId; GAS_CREW],
    housed: [bool; GAS_CREW],
    entered: u32,
    left: u32,
}

/// Places a completed extractor that hides its harvesters over the gas node nearest the local
/// base, and three workers of the local player beside it gathering from it.
fn add_gas_crew(
    simulation: &mut sim_core::SimWorld,
) -> Result<GasCrew, Box<dyn std::error::Error>> {
    let local = sim_types::PlayerId(0);
    let find = |test: &dyn Fn(&sim_core::catalog::UnitArchetype) -> bool, what: &'static str| {
        simulation
            .catalog
            .archetypes()
            .find(|a| test(a))
            .map(|a| a.unit_type_id)
            .ok_or(what)
    };
    let extractor = find(
        &|a| a.capabilities.is_gas_extractor && a.capabilities.hides_harvesters,
        "an extractor that hides its harvesters",
    )?;
    let worker = find(&|a| a.capabilities.harvester.is_some(), "a harvester")?;
    let base = simulation
        .entities
        .values()
        .find(|e| e.owner == local && e.is_immobile)
        .map(|e| e.position)
        .ok_or("a local base")?;
    let mut nodes: Vec<(sim_types::SimScalar, usize, (u32, u32))> = simulation
        .map
        .resources()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == sim_types::ResourceKind::Gas)
        .map(|(index, node)| (node.pos.distance_sq(base), index, node.cell))
        .collect();
    nodes.sort_unstable();
    let (_, index, _) = nodes
        .into_iter()
        .find(|(_, _, cell)| {
            simulation
                .spawn_structure(extractor, local, cell.0 - 1, cell.1 - 1)
                .is_ok()
        })
        .ok_or("a gas node near the local base that takes an extractor")?;
    let target = sim_types::ResourceNodeId(u32::try_from(index)?);
    let toward = base - simulation.map.resources()[index].pos;
    let side = toward.normalize_or_zero() * sim_types::SimScalar::from_num(2.5);
    let at = simulation.map.resources()[index].pos + side;
    let mut ids = [sim_types::EntityId(0); GAS_CREW];
    for (slot, id) in ids.iter_mut().enumerate() {
        let offset = sim_types::SimScalar::from_num(0.6) * sim_types::SimScalar::from_num(slot);
        *id = simulation.spawn_unit(worker, local, at + sim_types::SimVec2::new(offset, offset))?;
        simulation
            .entities
            .get_mut(id)
            .ok_or("a spawned worker")?
            .issue_order(sim_types::UnitOrder::Harvest { target }, false)?;
    }
    Ok(GasCrew {
        ids,
        housed: [false; GAS_CREW],
        entered: 0,
        left: 0,
    })
}

/// Counts the crew's comings and goings from the presented bubbles, on every frame.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn watch_gas_crew(session: Res<'_, BattleSession>, mut crew: ResMut<'_, GasCrew>) {
    let bubbles = &session.frames.current().bubbles;
    for slot in 0..GAS_CREW {
        let id = crew.ids[slot];
        let housed = bubbles
            .iter()
            .any(|bubble| bubble.entity_id == id && bubble.housed);
        if housed && !crew.housed[slot] {
            crew.entered += 1;
        } else if !housed && crew.housed[slot] {
            crew.left += 1;
        }
        crew.housed[slot] = housed;
    }
}

/// Broodlings the watcher of the Broodlings' children tracks at once; more alive is a failure of
/// the child, never a silent miscount.
const BROOD_SLOTS: usize = 32;

/// The Broodlings of the Broodlings' children and how often, across all frames so far, one
/// arrived in or left the presented snapshot (plan `2026-11-29-render-entity-pool` R-1v).
/// Fixed-size, so watching it allocates nothing.
#[derive(Resource, Clone, Copy, Debug)]
struct BroodWatch {
    /// The archetype the Brood Lord analogue's impacts create.
    kind: sim_types::UnitTypeId,
    /// Its longest life in ticks (`L_max`).
    lifetime: u32,
    /// The Broodlings presented on the previous frame.
    present: [Option<sim_types::EntityId>; BROOD_SLOTS],
    /// Arrivals seen.
    arrived: u32,
    /// Departures seen.
    departed: u32,
    /// Whether more than [`BROOD_SLOTS`] were presented at once.
    overflowed: bool,
}

/// Counts the Broodlings' arrivals and departures from the presented bubbles, on every frame.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn watch_broods(session: Res<'_, BattleSession>, mut watch: ResMut<'_, BroodWatch>) {
    let bubbles = &session.frames.current().bubbles;
    let mut now = [None; BROOD_SLOTS];
    let kind = watch.kind;
    let broods = bubbles.iter().filter(|bubble| bubble.unit_type == kind);
    for (slot, bubble) in broods.enumerate() {
        let Some(held) = now.get_mut(slot) else {
            watch.overflowed = true;
            break;
        };
        *held = Some(bubble.entity_id);
        if !watch.present.contains(&Some(bubble.entity_id)) {
            watch.arrived += 1;
        }
    }
    let departed = watch
        .present
        .iter()
        .flatten()
        .filter(|id| !now.contains(&Some(**id)))
        .count();
    watch.departed += u32::try_from(departed).unwrap_or(u32::MAX);
    watch.present = now;
}

/// Stages an attack snapshot in `simulation` before its session begins: runs ticks until the
/// enemy on the cliff has struck the local unit below it unseen, then makes that unit
/// invulnerable, so no later strike refreshes the image and the window holds it unchanged. The
/// catalog's snapshot cap (1 second, plan Amendment 1) is lifted first, so the unrefreshed image
/// outlasts the window: this test counts a held image's allocations, not its lifetime, which
/// `sim_core`'s `attack_snapshot_tests` pins.
fn hold_attack_snapshot(simulation: &mut sim_core::SimWorld) -> TestResult {
    simulation.catalog.set_attack_snapshot_cap(None);
    let staged = attack_snapshot_stage::stage(simulation);
    let mut view = sim_types::PlayerView::default();
    for _ in 0..400 {
        simulation.step(&sim_types::TickBundle {
            tick: sim_types::Tick(simulation.current_tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        simulation.write_player_view(attack_snapshot_stage::LOCAL_PLAYER, &mut view);
        if !view.attack_snapshots.is_empty() {
            simulation
                .entities
                .get_mut(&staged.victim)
                .ok_or("the struck unit lives")?
                .is_invulnerable = true;
            return hold_spell_image(simulation);
        }
    }
    Err("the enemy on the cliff never struck unseen".into())
}

/// Adds a spell caster's image to the held one (plan `2026-11-27-launch-and-spell-visibility`
/// V-25): an enemy caster on another cliff storms a local unit below it, and ticks run until the
/// storm has ended, so its `Hidden` pulses are over and the image is held unchanged.
fn hold_spell_image(simulation: &mut sim_core::SimWorld) -> TestResult {
    let staged = attack_snapshot_stage::stage_caster(simulation);
    let step = |simulation: &mut sim_core::SimWorld| {
        simulation.step(&sim_types::TickBundle {
            tick: sim_types::Tick(simulation.current_tick),
            commands: Vec::new(),
            state_checksum: None,
        });
    };
    step(simulation);
    attack_snapshot_stage::cast_storm(simulation, &staged);
    for _ in 0..400 {
        step(simulation);
        let effects = simulation.effects();
        if effects.persistents().is_empty() && effects.statuses_of(staged.victim).is_empty() {
            let mut view = sim_types::PlayerView::default();
            simulation.write_player_view(attack_snapshot_stage::LOCAL_PLAYER, &mut view);
            return if view
                .attack_snapshots
                .iter()
                .any(|entry| entry.entity == staged.attacker)
                && view.attack_snapshots.len() == 2
            {
                Ok(())
            } else {
                Err(format!("two held images, got {:?}", view.attack_snapshots).into())
            };
        }
    }
    Err("the storm ended within 400 ticks".into())
}

/// The local player's view, as the HUD keeps it, for the snapshot pool's fog.
#[derive(Resource, Default)]
struct HeldView(sim_types::PlayerView);

/// Writes the local player's view on every new tick and refills the fog from it, as the HUD's
/// `refresh_hud_model` and `refresh_battle_fog` do.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn refresh_snapshot_fog(
    session: Res<'_, BattleSession>,
    mut view: ResMut<'_, HeldView>,
    mut fog: ResMut<'_, client::battle_pipeline::fog::BattleFog>,
) {
    let (Some(world), Some(generation)) = (session.world(), session.commands.generation()) else {
        return;
    };
    if view.0.tick == sim_types::Tick(world.current_tick) && fog.refills() > 0 {
        return;
    }
    world.write_player_view(attack_snapshot_stage::LOCAL_PLAYER, &mut view.0);
    fog.refill(
        generation,
        &view.0,
        world.map.resources().iter().map(|node| node.remaining > 0),
    );
}

/// Adds the view writer, the fog refill and the attack-snapshot pool after the live sync.
fn add_snapshot_presentation(app: &mut App) {
    app.init_resource::<HeldView>()
        .init_resource::<client::battle_pipeline::fog::BattleFog>()
        .init_resource::<client::battle_hud::attack_snapshots::AttackSnapshotPool>()
        .add_systems(
            Update,
            (
                refresh_snapshot_fog,
                client::battle_hud::attack_snapshots::sync_attack_snapshots,
            )
                .chain()
                .after(client::sync_live_battle_system),
        );
}

/// The positive control: one heap allocation, freed in the same frame.
fn planted_allocation() {
    drop(std::hint::black_box(Vec::<u8>::with_capacity(64)));
}

/// The mixed group of the variant `smart_mixed`: two ground soldiers and an armed air unit of the
/// local player and the local transport they right-click, all presentable by their shipped
/// media.
#[derive(Resource, Clone, Copy, Debug)]
struct MixedGroup {
    soldiers: [sim_types::EntityId; 2],
    flyer: sim_types::EntityId,
    carrier: sim_types::EntityId,
}

/// Map distance, in world units, from the soldiers to the transport: their walk to board lasts
/// past the warm-up and the longer window (96 ticks, under 4 s at a soldier's speed).
const MIXED_CARRIER_DISTANCE: i32 = 22;

/// Spawns the [`MixedGroup`] near the local base of `simulation`, on ground cells both bodies
/// can stand on; unit types are chosen by capability and by shipped media under `assets`.
fn stage_mixed_group(
    simulation: &mut sim_core::SimWorld,
    assets: &Path,
) -> Result<MixedGroup, Box<dyn std::error::Error>> {
    let local = sim_types::PlayerId(0);
    let find = |test: &dyn Fn(&sim_core::catalog::UnitArchetype) -> bool, what: &'static str| {
        simulation
            .catalog
            .archetypes()
            .find(|a| {
                !a.is_immobile_by_default()
                    && assets
                        .join("textures")
                        .join(format!("{}.flipbook.ron", a.slug))
                        .is_file()
                    && test(a)
            })
            .map(|a| a.unit_type_id)
            .ok_or(what)
    };
    let soldier = find(
        &|a| {
            a.mover_type == sim_types::MoverType::Ground
                && a.capabilities.harvester.is_none()
                && !a.capabilities.can_construct
                && a.capabilities.transport_capacity == 0
        },
        "a presentable ground soldier",
    )?;
    let carrier = find(
        &|a| a.capabilities.transport_capacity > 0,
        "a presentable transport",
    )?;
    let flyer = find(
        &|a| {
            a.mover_type == sim_types::MoverType::Air
                && a.capabilities.transport_capacity == 0
                && client::battle_pipeline::commands::is_armed(a)
        },
        "a presentable armed air unit",
    )?;
    let base = simulation
        .entities
        .values()
        .find(|e| e.owner == local && e.is_immobile)
        .map(|e| e.position)
        .ok_or("a local base")?;
    let walkable = |world: &sim_core::SimWorld, at: sim_types::SimVec2| {
        (-1..=1).all(|dx: i32| {
            (-1..=1).all(|dy: i32| {
                let probe = at
                    + sim_types::SimVec2::new(
                        sim_types::SimScalar::from_i32(dx),
                        sim_types::SimScalar::from_i32(dy),
                    );
                world
                    .map
                    .world_to_cell(probe)
                    .ok()
                    .and_then(|(x, y)| world.map.cell(x, y).ok())
                    .is_some_and(|cell| {
                        cell.flags.contains(sim_map::CellFlags::GROUND_PATHABLE)
                            && !cell.flags.intersects(
                                sim_map::CellFlags::OCCUPIED_BY_STRUCTURE
                                    | sim_map::CellFlags::UNPASSABLE_CLIFF,
                            )
                    })
            })
        })
    };
    let around = |world: &sim_core::SimWorld, distance: i32| {
        (0..16)
            .map(|step| {
                let angle = f64::from(step) * std::f64::consts::TAU / 16.0;
                let offset = sim_types::SimVec2::new(
                    sim_types::SimScalar::from_num(angle.cos() * f64::from(distance)),
                    sim_types::SimScalar::from_num(angle.sin() * f64::from(distance)),
                );
                base + offset
            })
            .find(|at| walkable(world, *at))
            .ok_or("open ground near the local base")
    };
    // Each body on its own open ground, clear of structures and cliffs.
    let first = around(simulation, 6)?;
    let second = around(simulation, 8)?;
    let beside = around(simulation, 10)?;
    let far = around(simulation, 6 + MIXED_CARRIER_DISTANCE)?;
    let soldiers = [
        simulation.spawn_unit(soldier, local, first)?,
        simulation.spawn_unit(soldier, local, second)?,
    ];
    let flyer = simulation.spawn_unit(flyer, local, beside)?;
    let carrier = simulation.spawn_unit(carrier, local, far)?;
    Ok(MixedGroup {
        soldiers,
        flyer,
        carrier,
    })
}

/// The mixed right click: the client's own plan of a right click by the group on its transport
/// (`smart_plan`) queued by the client's own submission (`submit_smart`), as the pointer would.
/// Fails unless it gives the soldiers `Load` and the air unit `Follow`.
fn click_smart_mixed(app: &mut App, group: MixedGroup) -> TestResult {
    let local = sim_types::PlayerId(0);
    let mut session = app.world_mut().resource_mut::<BattleSession>();
    let (world, queue) = session.world_and_commands();
    let world = world.ok_or("invariant: the prepared session holds its world")?;
    let ground = world.entities[&group.carrier].position;
    let target = client::battle_input::context_order::PointerTarget::Friendly(
        group.carrier,
        Vec3::new(ground.x.to_f32(), 0.0, ground.y.to_f32()),
    );
    let selection = [
        group.soldiers[0],
        group.soldiers[1],
        group.flyer,
        group.carrier,
    ];
    let plan = client::battle_input::context_order::smart_plan(world, local, &selection, target);
    let orders: Vec<_> = plan.orders.iter().map(|entry| &entry.order).collect();
    let expected = [
        &sim_types::UnitOrder::Load {
            carrier: group.carrier,
        },
        &sim_types::UnitOrder::Follow {
            target: group.carrier,
            kind: sim_types::FollowKind::Move,
        },
    ];
    if orders != expected {
        return Err(format!("the mixed click planned {orders:?}").into());
    }
    queue.submit_smart(world, local, &plan, false)?;
    Ok(())
}

/// The `smart_mixed` children's check after their window of `frames` 20 ms frames, in which the
/// battle ran `ticked` ticks: one 40 ms tick every other frame, and the soldiers are still
/// walking to board and the air unit still follows, so the orders ran through the whole window.
fn check_mixed_window(app: &App, group: MixedGroup, ticked: u32, frames: u32) -> TestResult {
    if ticked != frames / 2 {
        return Err(format!("the battle ran {ticked} ticks in {frames} frames").into());
    }
    let session = app.world().resource::<BattleSession>();
    let world = session
        .world()
        .ok_or("invariant: the running session holds its world")?;
    for soldier in group.soldiers {
        let entity = &world.entities[&soldier];
        let boarding = matches!(
            entity.order_queue.front(),
            Some(sim_types::UnitOrder::Load { carrier }) if *carrier == group.carrier
        );
        if !boarding || entity.is_loaded_in_transport() {
            return Err(format!(
                "{soldier:?} must still walk to board after the window: {:?}",
                entity.order_queue
            )
            .into());
        }
    }
    let follower = &world.entities[&group.flyer];
    if !matches!(
        follower.order_queue.front(),
        Some(sim_types::UnitOrder::Follow { target, .. }) if *target == group.carrier
    ) {
        return Err(format!("the air unit must still follow: {:?}", follower.order_queue).into());
    }
    Ok(())
}

/// What [`stage_variant`] stages: the gas crew, the cycling kind's watcher, the Carriers' driver
/// and the razing driver, each where the variant has one.
type Staged = (
    Option<GasCrew>,
    Option<BroodWatch>,
    Option<HangarDriver>,
    Option<RazingDriver>,
);

/// Stages what `variant` needs in `simulation` before its session begins: the gas crew and the
/// held images of the running children, the Broodlings' battle of the brood children, or the
/// Carriers' battle of the hangar children.
fn stage_variant(
    variant: Variant,
    simulation: &mut sim_core::SimWorld,
) -> Result<Staged, Box<dyn std::error::Error>> {
    let crew = if variant.is_running() {
        let crew = add_gas_crew(simulation)?;
        hold_attack_snapshot(simulation)?;
        Some(crew)
    } else {
        None
    };
    let brood = variant.cycles_broods().then(|| {
        let stage = unit_cycling::stage_brood(simulation);
        BroodWatch {
            kind: stage.spawned,
            lifetime: stage.lifetime,
            present: [None; BROOD_SLOTS],
            arrived: 0,
            departed: 0,
            overflowed: false,
        }
    });
    let hangars = variant.cycles_hangars().then(|| {
        HangarDriver(unit_cycling::stage_hangars(
            simulation,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
        ))
    });
    let brood = brood.or_else(|| {
        hangars.map(|driver| BroodWatch {
            kind: driver.0.member,
            lifetime: 0,
            present: [None; BROOD_SLOTS],
            arrived: 0,
            departed: 0,
            overflowed: false,
        })
    });
    let razing = variant.cycles_razing().then(|| {
        RazingDriver(unit_cycling::stage_razing(
            simulation,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
        ))
    });
    // The razing children warm for four periods, as the Broodlings' children for four lives.
    let brood = brood.or_else(|| {
        razing.map(|driver| BroodWatch {
            kind: driver.0.spawned,
            lifetime: unit_cycling::RAZING_PERIOD_TICKS,
            present: [None; BROOD_SLOTS],
            arrived: 0,
            departed: 0,
            overflowed: false,
        })
    });
    Ok((crew, brood, hangars, razing))
}

/// The razing cycle of the razing children (plan `2026-12-01-death-spawns-and-hangar-commands`
/// DS-18).
#[derive(Resource, Clone, Copy, Debug)]
struct RazingDriver(unit_cycling::RazingCycle);

/// Raises and kills the razing host on its cadence, before the tick.
fn drive_razing(mut session: ResMut<'_, BattleSession>, mut driver: ResMut<'_, RazingDriver>) {
    if let Some(world) = session.world_mut() {
        driver.0.advance(world);
    }
}

/// The Carriers' cycle of the hangar children (plan `2026-11-28-carrier-interceptors` H-20).
#[derive(Resource, Clone, Copy, Debug)]
struct HangarDriver(unit_cycling::HangarCycle);

/// Cycles to complete before a hangar child's window: the first fills the magazines, the rest
/// bring the render pool to its regime.
const HANGAR_WARM_CYCLES: u32 = 3;

/// Frames a hangar child may take to complete its warm-up cycles.
const HANGAR_WARM_LIMIT: u32 = 20_000;

/// Drives the Carriers' cycle from the presented snapshot, before the tick. Reads the snapshot
/// and writes the world in place: the simulation allocates only for each new target.
fn drive_hangars(mut session: ResMut<'_, BattleSession>, mut driver: ResMut<'_, HangarDriver>) {
    let members = driver.0.members_in(&session.frames.presented().bubbles);
    if let Some(world) = session.world_mut() {
        driver.0.advance(world, members);
    }
}

/// Adds what [`stage_variant`] staged to `app`: the gas crew's and the cycling kind's watchers,
/// the Carriers' and the razing drivers (before the tick), and the battle driver of a baseline
/// child that ticks (the `smart_mixed` baseline among them).
fn add_staged(app: &mut App, variant: Variant, staged: &Staged) {
    let &(crew, brood, hangars, razing) = staged;
    if let Some(crew) = crew {
        app.insert_resource(crew).add_systems(Last, watch_gas_crew);
    }
    if let Some(watch) = brood {
        app.insert_resource(watch).add_systems(Last, watch_broods);
    }
    if let Some(driver) = hangars {
        app.insert_resource(driver)
            .add_systems(Update, drive_hangars.before(advance_battle_ticks));
    }
    if let Some(driver) = razing {
        app.insert_resource(driver)
            .add_systems(Update, drive_razing.before(advance_battle_ticks));
    }
    if matches!(
        variant,
        Variant::RunningBaseline
            | Variant::BroodCycleBaseline
            | Variant::HangarCycleBaseline
            | Variant::RazingCycleBaseline
            | Variant::SmartMixedBaseline
    ) {
        app.add_systems(Update, advance_battle_ticks);
    }
}

/// Builds the app of `variant` with a fixed time step: on the real default battle, paused, or
/// for a running variant on the shipped skirmish, resumed.
fn steady_state_app(variant: Variant) -> Result<App, Box<dyn std::error::Error>> {
    let root = client::ResolvedAssetsRoot::from_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let scenario = if variant.is_running() || variant.clicks_smart_mixed() {
        SKIRMISH_SCENARIO
    } else {
        DEFAULT_BATTLE_SCENARIO
    };
    let ticking = variant.is_running() || variant.cycles_units() || variant.clicks_smart_mixed();
    let definition = BattleDefinition::load(root.as_path(), scenario.as_ref())?;
    let mut simulation = definition.instantiate()?;
    let (crew, brood, hangars, razing) = stage_variant(variant, &mut simulation)?;
    let mixed = if variant.clicks_smart_mixed() {
        Some(stage_mixed_group(&mut simulation, root.as_path())?)
    } else {
        None
    };
    let session = BattleSession::prepare(
        SessionGeneration::FIRST,
        &mut simulation,
        CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: root.as_path().to_string_lossy().into_owned(),
            ..default()
        },
    ))
    .add_event::<AppExit>()
    .add_event::<client::CombatVisualEvent>()
    .init_asset::<Mesh>()
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    .init_asset::<client::BubbleDomeMaterial>()
    .init_asset::<client::SelectionRingMaterial>()
    .init_resource::<client::RenderEntityMap>()
    .init_resource::<client::flipbook_clock::FlipbookClock>()
    .init_resource::<client::battle_selection::BattleSelection>()
    .insert_resource(definition.palette.clone())
    .init_resource::<client::player_presentation::PresentationStatus>()
    .insert_resource(root)
    .insert_resource(client::CatalogResource(definition.catalogs.units.clone()))
    .insert_resource(session.with_world(simulation))
    .insert_resource(client::LabState {
        current_tab: client::LabTab::LiveBattle,
        battle_paused: !ticking,
        simulation_speed: 1,
        ..default()
    })
    .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(if variant.cycles_units() { 40 } else { 20 }),
    ))
    .add_systems(
        Startup,
        (
            client::setup_bubble_assets_system,
            client::setup_indicator_assets_system,
        ),
    );
    add_staged(&mut app, variant, &(crew, brood, hangars, razing));
    if let Some(group) = mixed {
        click_smart_mixed(&mut app, group)?;
        app.insert_resource(group);
    }
    if variant.presents() {
        app.add_systems(
            Update,
            (
                advance_battle_ticks,
                client::flipbook_clock::update_flipbook_clock_system,
                client::sync_live_battle_system,
                publish_combat_events,
            )
                .chain(),
        );
    }
    if variant == Variant::RunningLive {
        add_snapshot_presentation(&mut app);
    }
    if variant == Variant::Planted {
        app.add_systems(Update, planted_allocation);
    }
    if variant.streams_effects() {
        add_effect_stream(&mut app, variant == Variant::EffectStream)?;
    }
    // The multi-threaded executor allocates on every frame for its own task spawning (about 18
    // blocks a frame in the baseline, all inside bevy_tasks and async_executor), and the number
    // varies with thread timing. It is Bevy's, not the client's, so the children run on the
    // single-threaded executor, which runs the same systems in the same order without tasks.
    for (_, schedule) in app.world_mut().resource_mut::<Schedules>().iter_mut() {
        schedule.set_executor_kind(ExecutorKind::SingleThreaded);
    }
    Ok(app)
}

/// Sends one splash event per frame, its centre walking over a 9 by 7 grid, and an instant hit,
/// a projectile and a blow at one of the presented bubbles in turn, naming it as the struck
/// entity and the next bubble as the attacker (WP11
/// review 3, E3-3; plan `2026-11-17-effect-snapshot-identity` WP4: the struck bubble's lookup
/// by id, the last frame's bubbles kept, and the move onto the glass are in the counted path).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn send_splash(
    mut counter: Local<'_, u32>,
    mut events: EventWriter<'_, client::CombatVisualEvent>,
    session: Res<'_, BattleSession>,
    source: Res<'_, StreamSource>,
) {
    *counter = counter.wrapping_add(1);
    let x = f32::from(u8::try_from(*counter % 9).unwrap_or(0));
    let y = f32::from(u8::try_from(*counter % 7).unwrap_or(0));
    let bubbles = &session.frames.presented().bubbles;
    if !bubbles.is_empty() {
        let index = usize::try_from(*counter).unwrap_or(0) % bubbles.len();
        let [bx, by, _] = bubbles[index].world_pos;
        let hit = [bx, by, 0.5];
        // The struck entity is named, as the simulation names it (plan
        // `2026-11-17-effect-snapshot-identity` WP4): the lookup by id is in the counted path.
        let ends = Some(sim_types::StrikeEnds {
            attacker: bubbles[(index + 1) % bubbles.len()].entity_id,
            target: bubbles[index].entity_id,
        });
        events.send(client::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::HitscanTracer {
                from: [bx - 3.0, by + 1.0, 0.5],
                to: hit,
                owner: sim_types::PlayerId(0),
                source: source.0,
                ends,
            },
        ));
        // A held beam (plan `2026-11-17-effect-snapshot-identity` WP7): the first bubble beams
        // at a target that changes every fourth frame (a retarget), and every seventh frame the
        // second bubble beams at an entity that is not presented (a target that died), whose
        // beam is held and released on the next keep.
        if bubbles.len() >= 3 {
            let beam = |attacker: usize, target: sim_types::EntityId| {
                let [ax, ay, _] = bubbles[attacker].world_pos;
                client::CombatVisualEvent(sim_types::snapshot::CombatEventVisual::BeamVisual {
                    from: [ax, ay, 0.5],
                    to: hit,
                    is_lightning: false,
                    owner: sim_types::PlayerId(0),
                    source: beam_source(source.0),
                    ends: Some(sim_types::StrikeEnds {
                        attacker: bubbles[attacker].entity_id,
                        target,
                    }),
                })
            };
            let target = 1 + usize::try_from(*counter / 4).unwrap_or(0) % (bubbles.len() - 1);
            events.send(beam(0, bubbles[target].entity_id));
            if counter.is_multiple_of(7) {
                events.send(beam(1, sim_types::EntityId(u32::MAX - *counter % 64)));
            }
        }
        // A projectile from the attacker's bubble at the struck one (plan
        // `2026-11-17-effect-snapshot-identity` Amendment 7, B-133): the shooter's lookup by id
        // and the start's move onto its glass are in the counted path.
        let [ax, ay, _] = bubbles[(index + 1) % bubbles.len()].world_pos;
        let flight = 0.2;
        events.send(client::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::ProjectileVisual {
                pos: [ax, ay, 0.5],
                vel: [(bx - ax) / flight, (by - ay) / flight, 0.0],
                owner: sim_types::PlayerId(0),
                duration_secs: Some(flight),
                source: source.0,
                ends,
            },
        ));
        events.send(client::CombatVisualEvent(
            sim_types::snapshot::CombatEventVisual::MeleeSlashVisual {
                pos: hit,
                angle_rad: f32::from(u8::try_from(*counter % 8).unwrap_or(0)) * 0.785,
                radius: 0.5,
                owner: sim_types::PlayerId(0),
                source: source.0,
                ends,
            },
        ));
    }
    events.send(client::CombatVisualEvent(
        sim_types::snapshot::CombatEventVisual::SplashVisual {
            center: [x, y, 0.0],
            zones: [0.4, 0.7, 1.0],
            line: None,
            source: sim_types::EffectSource::default(),
            owner: sim_types::PlayerId(0),
        },
    ));
}

/// The weapon the stream's instant hits and blows come from.
#[derive(Resource)]
struct StreamSource(sim_types::EffectSource);

/// The weapon the stream's beams come from: `source`'s unit's air weapon, whose travel is the
/// stream vocabulary's sustained `beam` (WP7).
const fn beam_source(source: sim_types::EffectSource) -> sim_types::EffectSource {
    sim_types::EffectSource {
        unit_type: source.unit_type,
        weapon: sim_types::WeaponKey {
            slot: sim_types::WeaponSlot::Air,
            mode: None,
        },
    }
}

/// The vocabulary of the effect stream: the default splash draws a card, a ring, a decal, a
/// light and a particle burst, all within 0.6 s, so at one splash per 20 ms frame about 30
/// effects live at once, inside the fixture budget's 64 slots: every splash spawns.
const STREAM_EFFECTS: &str = r#"
    "burst": (class: Minor, event: Splash, layers: [
        (block: Sprite(texture: "flip", ramp: "red_add"), size: (bind: SplashReach, scale: 1.0),
         duration: 0.6, priority: Critical),
        (block: Ring(texture: "solid", ramp: "green", orient: Ground, thickness: 0.3),
         radius: (from: (bind: SplashFull, scale: 1.0), to: (bind: SplashReach, scale: 1.0)),
         duration: 0.3),
        (block: Decal(texture: "soft_disc", ramp: "blue"), size: (bind: SplashReach, scale: 1.0),
         duration: 0.5),
        (block: Light(color: (1.0, 0.6, 0.3), intensity: 400.0), size: (scale: 2.0),
         duration: 0.2, curve: "fade"),
        (block: Particles(emitter: "chips"), size: (bind: SplashReach, scale: 1.0)),
    ]),
    "cloud": (class: Major, event: Area, emit: Some((effect: "cloud_puff", every: 0.1)), layers: [
        (block: Decal(texture: "soft_disc", ramp: "blue", outline: (edge_noise: 0.01, fill: 0.7)),
         size: (bind: AreaRadius, scale: 1.0), duration: 0.1, held: true, priority: Critical),
        (block: Sprite(texture: "flip", ramp: "red_add"), placement: Disc(count: 4,
         radius: (bind: AreaRadius, scale: 0.5)), size: (bind: AreaRadius, scale: 0.4),
         duration: 0.1, held: true),
    ]),
    "cloud_puff": (class: Minor, event: Area, layers: [
        (block: Sprite(texture: "flip", ramp: "red_add"), placement: Disc(count: 1,
         radius: (bind: AreaRadius, scale: 0.6)), size: (bind: AreaRadius, scale: 0.3),
         duration: 0.5),
    ]),
    "tongue": (class: Minor, event: Link, layers: [
        (block: Ribbon(texture: "soft_disc", ramp: "red_add", scroll: 1.0), size: (scale: 0.05),
         duration: 0.1, held: true, priority: Critical),
    ]),
    "beam": (class: Minor, event: Shot, sustain: Some((grace: 0.12)), layers: [
        (block: Ribbon(texture: "solid", ramp: "red_add", shape: Radial), size: (scale: 0.05),
         duration: 0.04, held: true, exit: 0.08, priority: Critical),
        (block: Sprite(texture: "soft_disc", ramp: "red_add", ride: true, stretch: 2.0),
         size: (scale: 0.1), duration: 0.1),
    ]),
    "bolt": (class: Minor, event: Shot, layers: [
        (block: Sprite(texture: "soft_disc", ramp: "red_add", ride: true, stretch: 2.0),
         size: (scale: 0.1), duration: 0.2, lasts: FlightTime, priority: Critical),
        (block: Ribbon(texture: "solid", ramp: "red_add", shape: Radial, trail: Some(0.5)),
         size: (scale: 0.03), duration: 0.2, lasts: FlightTime),
    ]),
    "spore": (class: Minor, event: Missile, impact: Some("splat"),
        emit: Some((effect: "spore_mote", every: 0.05)), layers: [
        (block: Sprite(texture: "soft_disc", ramp: "red_add"), size: (scale: 0.2),
         duration: 0.05, held: true, exit: 0.05, priority: Critical),
    ]),
    "spore_mote": (class: Ambient, event: Missile, layers: [
        (block: Sprite(texture: "soft_disc", ramp: "red_add"), size: (scale: 0.05), duration: 0.3),
    ]),
    "hit": (class: Minor, event: Point, layers: [
        (block: Sprite(texture: "soft_disc", ramp: "red_add"), size: (scale: 0.2), duration: 0.1,
         priority: Critical),
        (block: Ring(texture: "solid", ramp: "green", thickness: 0.2, rotation: Facing,
         arc: Some((scale: 0.4))), placement: Behind(distance: (bind: UnitRadius, scale: 1.15)),
         radius: (from: (bind: UnitRadius, scale: 1.2), to: (bind: UnitRadius, scale: 1.3)),
         duration: 0.1),
    ]),
    "splat": (class: Minor, event: Area, layers: [
        (block: Decal(texture: "soft_disc", ramp: "blue"), size: (bind: AreaRadius, scale: 1.0),
         duration: 0.5, lasts: AreaTotal, priority: Critical),
    ])
"#;

/// The emitter of [`STREAM_EFFECTS`].
const STREAM_EMITTERS: &str = r#""chips": (spawn: Burst(count: (base: 8, max: 8)),
    speed: (1.0, 2.0), cone_deg: 45.0, gravity: 9.0, lifetime: (0.2, 0.5), size: (0.03, 0.06),
    texture: "soft_disc", orient: VelocityStretched(1.0), ramp: "red_add")"#;

/// Adds the splash sender and, when `with_effects`, the production effect systems over a
/// test-only vocabulary written into a temporary directory (kept for the process's life).
fn add_effect_stream(app: &mut App, with_effects: bool) -> TestResult {
    use client::effects::catalog::{EffectCatalog, UnitEffectTables};
    use client::effects::runtime::EffectRuntime;
    use client::effects::systems::{build_effect_frame, spawn_effects_from_events};
    // The instant hits and blows come from a unit's primary weapon, which the stream names.
    let source = app
        .world()
        .resource::<client::CatalogResource>()
        .0
        .archetypes()
        .find(|archetype| archetype.weapon.is_some())
        .map(|archetype| sim_types::EffectSource {
            unit_type: archetype.unit_type_id,
            weapon: sim_types::WeaponKey {
                slot: sim_types::WeaponSlot::Primary,
                mode: None,
            },
        })
        .ok_or("the shipped catalog has an armed unit")?;
    app.insert_resource(StreamSource(source))
        .add_systems(Update, send_splash.before(publish_combat_events));
    if !with_effects {
        return Ok(());
    }
    let directory = tempfile::tempdir()?.keep();
    let manifest = effect_fixtures::write_fixture_set(&directory);
    let text = effect_fixtures::vocabulary(
        &manifest,
        STREAM_EMITTERS,
        STREAM_EFFECTS,
        r#"splash: "burst""#,
    );
    let catalog = EffectCatalog::from_text(&text, &directory.join("effects.ron"), &directory)?;
    // The shipped presentation blocks name shipped effects this test vocabulary does not
    // define; the stream comes from its default splash instead.
    let mut units = app.world().resource::<client::CatalogResource>().0.clone();
    let ids: Vec<_> = units
        .archetypes()
        .map(|archetype| archetype.unit_type_id)
        .collect();
    // Every weapon shoots the test's `bolt` along its path (plan WP11: a shot's travel).
    let shot = |weapon| sim_core::catalog::WeaponEffects {
        weapon,
        mode: None,
        launch: None,
        // The air weapon's travel is the sustained beam of the beam stream (WP7).
        travel: Some(
            if weapon == sim_types::WeaponSlot::Air {
                "beam"
            } else {
                "bolt"
            }
            .to_owned(),
        ),
        impact: Some("hit".to_owned()),
        splash: Some("burst".to_owned()),
    };
    for id in ids {
        if let Ok(archetype) = units.get_mut(id) {
            archetype.effects = sim_core::catalog::ArchetypeEffects {
                weapons: vec![
                    shot(sim_types::WeaponSlot::Primary),
                    shot(sim_types::WeaponSlot::Ground),
                    shot(sim_types::WeaponSlot::Air),
                ],
                ..sim_core::catalog::ArchetypeEffects::default()
            };
        }
    }
    let views = held_views(app, &mut units)?;
    let tables = UnitEffectTables::build(&units, &catalog)?;
    app.insert_resource(EffectRuntime::new(&catalog))
        .insert_resource(tables)
        .insert_resource(catalog)
        .insert_resource(views)
        .init_resource::<client::effects::areas::AreaLooks>()
        .init_resource::<client::effects::struck::StruckBubbles>()
        .init_resource::<client::effects::beams::HeldBeams>()
        .add_systems(
            Update,
            (
                spawn_effects_from_events.after(publish_combat_events),
                hold_area_and_link.after(publish_combat_events),
                client::effects::beams::keep_beams.after(spawn_effects_from_events),
            ),
        )
        .add_systems(PostUpdate, build_effect_frame);
    Ok(())
}

/// Two published areas (the viewer's, and an enemy's across the viewer's sight edge) and one
/// published tether, held every frame of the stream under `fog` (WP10).
#[derive(Resource)]
struct HeldViews {
    /// The areas.
    area: [sim_types::AreaView; 2],
    /// The viewer's fog: every cell seen but a block the enemy area reaches into.
    fog: client::battle_pipeline::fog::BattleFog,
    /// The tether.
    link: [sim_types::LinkView; 1],
    /// A spell missile's ability and where it flies (plan WP11, F1): a new missile every few
    /// frames, the previous one landing its impact.
    missile: (sim_types::AbilitySource, sim_types::PlayerId, [f32; 3]),
}

/// Gives the first archetype with two abilities the stream vocabulary's area and tether for
/// them, and builds an area of the first and a tether of the second between the battle's first
/// two presented units.
#[allow(
    clippy::too_many_lines,
    reason = "test setup: one area, tether and missile staged in place"
)]
fn held_views(
    app: &App,
    units: &mut sim_core::catalog::UnitCatalog,
) -> Result<HeldViews, Box<dyn std::error::Error>> {
    let caster = units
        .archetypes()
        .find(|archetype| archetype.abilities.len() >= 2)
        .map(|archetype| archetype.unit_type_id)
        .ok_or("the shipped catalog has a unit with two abilities")?;
    // A second caster's first ability launches the stream vocabulary's missile.
    let launcher = units
        .archetypes()
        .find(|archetype| archetype.unit_type_id != caster && !archetype.abilities.is_empty())
        .map(|archetype| archetype.unit_type_id)
        .ok_or("the shipped catalog has a second unit with an ability")?;
    let missile_name = units.get(launcher)?.abilities[0].name.clone();
    units.get_mut(launcher)?.effects.abilities =
        std::iter::once((missile_name, "spore".to_owned())).collect();
    let archetype = units.get_mut(caster)?;
    let names: Vec<String> = archetype
        .abilities
        .iter()
        .take(2)
        .map(|ability| ability.name.clone())
        .collect();
    archetype.effects.abilities = [
        (names[0].clone(), "cloud".to_owned()),
        (names[1].clone(), "tongue".to_owned()),
    ]
    .into_iter()
    .collect();
    let bubbles = &app
        .world()
        .resource::<BattleSession>()
        .frames
        .current()
        .bubbles;
    let (Some(first), Some(second)) = (bubbles.first(), bubbles.get(1)) else {
        return Err("the default battle presents two units".into());
    };
    let owner = first.owner;
    let enemy = sim_types::PlayerId(owner.0 + 1);
    let map = &app
        .world()
        .resource::<BattleSession>()
        .world()
        .ok_or("a ready session has a world")?
        .map;
    let (width, height) = (map.width(), map.height());
    // The enemy area stands 8 units from the first unit, towards the map's middle, on the edge
    // of a block of unseen cells (columns from its centre's on, rows round it).
    let toward = if f64::from(first.world_pos[0]) < f64::from(width) / 2.0 {
        8.0
    } else {
        -8.0
    };
    let enemy_center = [first.world_pos[0] + toward, first.world_pos[1]];
    let unseen = |column: u32, row: u32| {
        let (x, y) = (f64::from(column), f64::from(row));
        let (cx, cy) = (f64::from(enemy_center[0]), f64::from(enemy_center[1]));
        x >= cx.floor() && (y - cy).abs() < 4.0 && x < cx + 4.0
    };
    let view = sim_types::PlayerView {
        viewer: owner,
        map_width: width,
        map_height: height,
        visibility: (0..height)
            .flat_map(|row| (0..width).map(move |column| (column, row)))
            .map(|(column, row)| {
                if unseen(column, row) {
                    sim_types::VisibilityState::Fogged
                } else {
                    sim_types::VisibilityState::Visible
                }
            })
            .collect(),
        ..sim_types::PlayerView::default()
    };
    let mut fog = client::battle_pipeline::fog::BattleFog::default();
    fog.refill(SessionGeneration::FIRST, &view, std::iter::empty());
    let area = |id: u32, owner, center: [f32; 2]| sim_types::AreaView {
        id,
        owner,
        center: [center[0], center[1], first.ground_z],
        radius: 1.5,
        remaining_ticks: 60,
        total_ticks: 61,
        source: Some(sim_types::AbilitySource {
            unit_type: caster,
            ability: 0,
        }),
    };
    Ok(HeldViews {
        area: [
            area(1, owner, [first.world_pos[0] + 2.0, first.world_pos[1]]),
            area(2, enemy, enemy_center),
        ],
        fog,
        link: [sim_types::LinkView {
            from: first.entity_id,
            to: second.entity_id,
            owner,
            source: sim_types::AbilitySource {
                unit_type: caster,
                ability: 1,
            },
        }],
        missile: (
            sim_types::AbilitySource {
                unit_type: launcher,
                ability: 0,
            },
            enemy,
            [
                first.world_pos[0] + 3.0,
                first.world_pos[1] + 1.0,
                first.ground_z,
            ],
        ),
    })
}

/// Keeps the held area and tether in step every frame through the production code that the
/// battle's `present_areas_and_links` runs on the presented snapshot.
#[expect(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "Bevy passes system parameters by value, each a separate declared dependency"
)]
fn hold_area_and_link(
    views: Res<'_, HeldViews>,
    session: Res<'_, BattleSession>,
    catalog: Res<'_, client::effects::catalog::EffectCatalog>,
    tables: Res<'_, client::effects::catalog::UnitEffectTables>,
    mut runtime: ResMut<'_, client::effects::runtime::EffectRuntime>,
    mut looks: ResMut<'_, client::effects::areas::AreaLooks>,
    time: Res<'_, Time<Virtual>>,
    mut frame: Local<'_, u32>,
) {
    *frame = frame.wrapping_add(1);
    let mut keep = client::effects::areas::Keep {
        catalog: &catalog,
        tables: &tables,
        runtime: &mut runtime,
        now: time.elapsed_secs_f64(),
        delta: time.delta_secs_f64(),
    };
    client::effects::areas::keep_views(
        &mut looks,
        (&views.area, &views.link),
        session.frames.presented().bubbles.as_slice(),
        Some(&views.fog),
        &mut keep,
    );
    // A new missile every eight frames, flying a little each frame; the previous one lands.
    let (source, owner, [x, y, z]) = views.missile;
    let flown = 0.1 * f32::from(u8::try_from(*frame % 8).unwrap_or(0));
    let missile = [sim_types::SpellMissileView {
        id: *frame / 8,
        owner,
        position: [x + flown, y, z],
        source,
    }];
    client::effects::areas::keep_missiles(
        &mut looks,
        (&missile, &missile, 0.0),
        Some(&views.fog),
        &mut keep,
    );
    // The previous missile's impact, as the simulation's `SpellImpact` lays it (plan
    // `2026-11-17-effect-snapshot-identity` WP6), where it flew to.
    if *frame >= 8 && frame.is_multiple_of(8) {
        client::effects::areas::lay_spell_impact(
            &sim_types::snapshot::CombatEventVisual::SpellImpact {
                pos: [x + 0.8, y, z],
                missile: *frame / 8 - 1,
                owner,
                source,
            },
            Some(&views.fog),
            &mut keep,
        );
    }
}

// PERFORMANCE RATIONALE:
// Why: Whole-process allocation records mix media decoding and ECS setup with the steady state.
// What: Keep the measured window in a named, non-inlined function, so the parent can attribute
// each recorded allocation to the window by its call stack.
// Trade-off: A function boundary kept solely for measurement scope.
#[inline(never)]
fn measured_steady_frames(app: &mut App, frames: u32) {
    for _ in 0..frames {
        app.update();
    }
}

/// Warms the child's app: [`WARM_FRAMES`] frames, or for the Broodlings' children `4 × L_max`
/// ticks, one tick a frame, a number read from the data, so both children of the pair measure
/// the same ticks. Fails if the battle failed or, for a presenting child, nothing is presented.
fn warm_up(app: &mut App, variant: Variant) -> TestResult {
    if variant.cycles_hangars() {
        let cycles = |app: &App| app.world().resource::<HangarDriver>().0.cycles;
        let mut frames = 0;
        while cycles(app) < HANGAR_WARM_CYCLES {
            app.update();
            frames += 1;
            if frames > HANGAR_WARM_LIMIT {
                return Err(format!(
                    "{HANGAR_WARM_CYCLES} hangar cycles took more than {HANGAR_WARM_LIMIT} frames"
                )
                .into());
            }
        }
    }
    let warm = app
        .world()
        .get_resource::<BroodWatch>()
        .map_or(WARM_FRAMES, |watch| 4 * watch.lifetime);
    // A hangar child has warmed by whole cycles above; its watcher's lifetime is 0.
    for _ in 0..warm {
        app.update();
    }
    if let Some(failure) = &app.world().resource::<client::LabState>().battle_failure {
        return Err(failure.clone().into());
    }
    if variant.presents() && app.world().resource::<client::RenderEntityMap>().is_empty() {
        return Err("the live synchroniser presented nothing after warm-up".into());
    }
    Ok(())
}

/// Runs one child: returns at once without `FRAMES_ENV`, otherwise warms the app of `variant`
/// and runs the requested number of frames, paused or, for a running variant, ticking.
fn run_child(variant: Variant) -> TestResult {
    let Some(frames) = std::env::var_os(FRAMES_ENV) else {
        println!(
            "{}: {FRAMES_ENV} is not set, so this child does nothing; \
             steady_state_frames_allocate_nothing runs it under valgrind",
            variant.child_test()
        );
        return Ok(());
    };
    let frames: u32 = frames
        .to_str()
        .ok_or_else(|| format!("{FRAMES_ENV} is not UTF-8"))?
        .parse()?;
    let mut app = steady_state_app(variant)?;
    warm_up(&mut app, variant)?;
    // A hangar child's window is counted in cycles: that many times the last warm-up cycle's
    // ticks, one tick a frame, the same in both children of the pair.
    let frames = app
        .world()
        .get_resource::<HangarDriver>()
        .map_or(frames, |driver| frames * driver.0.last_cycle_ticks);
    let tick = |app: &App| {
        app.world()
            .resource::<BattleSession>()
            .frames
            .current()
            .tick
            .0
    };
    let before = tick(&app);
    let ticked_from = before;
    let spawned_before = effects_spawned(&app);
    let crew_before = app.world().get_resource::<GasCrew>().copied();
    let brood_before = app.world().get_resource::<BroodWatch>().copied();
    let pool_before = app.world().resource::<client::RenderEntityMap>().stats();
    measured_steady_frames(&mut app, frames);
    if matches!(
        variant,
        Variant::BroodCycleLive | Variant::HangarCycleLive | Variant::RazingCycleLive
    ) {
        // The warm-up reached the regime: no arrival of the window spawned a render entity.
        let pool_after = app.world().resource::<client::RenderEntityMap>().stats();
        if pool_after.spawned != pool_before.spawned {
            return Err(format!(
                "the render pool spawned in the window: {pool_before:?} then {pool_after:?}"
            )
            .into());
        }
    }
    if let Some(before) = brood_before {
        return check_brood_window(&app, frames, &before, tick(&app) - ticked_from);
    }
    if variant == Variant::EffectStream {
        check_effect_stream(&app, frames, spawned_before)?;
    }
    let after = tick(&app);
    if let Some(group) = app.world().get_resource::<MixedGroup>().copied() {
        return check_mixed_window(&app, group, after - before, frames);
    }
    if !variant.is_running() {
        if after != 0 {
            return Err(format!("the battle must stay paused, but reached tick {after}").into());
        }
        return Ok(());
    }
    // 20 ms frames: one 40 ms tick every other frame.
    if after - before != frames / 2 {
        return Err(format!("the battle ran {} ticks in {frames} frames", after - before).into());
    }
    if variant == Variant::RunningLive {
        check_held_snapshot(&mut app)?;
    }
    let crew = app
        .world()
        .get_resource::<GasCrew>()
        .copied()
        .zip(crew_before)
        .ok_or("invariant: a running child has its gas crew")?;
    if crew.0.entered <= crew.1.entered || crew.0.left <= crew.1.left {
        return Err(format!(
            "the gas crew must go inside and come out in the window: {:?} then {:?}",
            crew.1, crew.0
        )
        .into());
    }
    let busy = app
        .world()
        .resource::<BattleSession>()
        .frames
        .current()
        .bubbles
        .iter()
        .find(|bubble| {
            bubble.anim_state != sim_types::AnimState::Idle
                && !crew.0.ids.contains(&bubble.entity_id)
        })
        .map(|bubble| bubble.entity_id);
    if let Some(id) = busy {
        return Err(format!(
            "the running window must stay idle but for the gas crew, and {id:?} is not"
        )
        .into());
    }
    Ok(())
}

/// The Broodlings' children's check after their window of `frames` frames, in which the battle
/// ran `ticked` ticks: one tick a frame, and at least two Broodlings arrived and two left, all
/// tracked.
fn check_brood_window(app: &App, frames: u32, before: &BroodWatch, ticked: u32) -> TestResult {
    if ticked != frames {
        return Err(format!("the battle ran {ticked} ticks in {frames} frames").into());
    }
    let after = app
        .world()
        .get_resource::<BroodWatch>()
        .copied()
        .ok_or("invariant: a Broodlings' child has its watcher")?;
    if after.overflowed {
        return Err(format!(
            "more than {BROOD_SLOTS} units of the cycling kind were presented at once"
        )
        .into());
    }
    if after.arrived < before.arrived + 2 || after.departed < before.departed + 2 {
        return Err(format!(
            "units of the cycling kind (Broodlings or interceptors) must arrive and leave in the window: {before:?} then {after:?}"
        )
        .into());
    }
    Ok(())
}

/// The running live child's check after its window: the fog lists the two held images (a strike's
/// and a spell caster's) and the pool draws each on one visible entity.
fn check_held_snapshot(app: &mut App) -> TestResult {
    let listed = app
        .world()
        .resource::<client::battle_pipeline::fog::BattleFog>()
        .attack_snapshots()
        .len();
    let world = app.world_mut();
    let shown = world
        .query_filtered::<&Visibility, With<client::battle_hud::attack_snapshots::AttackSnapshotBubble>>()
        .iter(world)
        .filter(|visibility| **visibility != Visibility::Hidden)
        .count();
    // A strike's image and a spell caster's (plan `2026-11-27-launch-and-spell-visibility` V-25).
    if (listed, shown) != (2, 2) {
        return Err(format!(
            "the window must hold two attack images, each drawn once: {listed} listed, {shown} shown"
        )
        .into());
    }
    Ok(())
}

/// The effect stream's own checks after its window: the held areas and tether, the spawns,
/// the hits and blows, and their struck bubbles found by id.
fn check_effect_stream(app: &App, frames: u32, spawned_before: u64) -> TestResult {
    let held = app
        .world()
        .resource::<client::effects::areas::AreaLooks>()
        .held();
    if held != (2, 1) {
        return Err(format!(
            "the stream must hold two areas (one seen in part) and one tether, held {held:?}"
        )
        .into());
    }
    let spawned = effects_spawned(app) - spawned_before;
    if spawned < u64::from(frames) {
        return Err(
            format!("the effect stream spawned {spawned} effects in {frames} frames").into(),
        );
    }
    // The instant hits and blows reached the struck-bubble path (review 3, E3-3).
    let world = app.world();
    let catalog = world.resource::<client::effects::catalog::EffectCatalog>();
    let hits = catalog.effect_id("hit").map_or(0, |hit| {
        world
            .resource::<client::effects::runtime::EffectRuntime>()
            .spawned_of(hit)
    });
    if hits < frames {
        return Err(format!("the effect stream struck {hits} hits in {frames} frames").into());
    }
    // The lookup by id finds every presented bubble, and every hit and blow of the stream found
    // its struck bubble in the presented frame, none fell back to the default one (review 4,
    // E4-3; plan `2026-11-17-effect-snapshot-identity` WP4).
    let bubbles = &world.resource::<BattleSession>().frames.presented().bubbles;
    let lost = bubbles
        .iter()
        .filter(|bubble| {
            client::effects::struck::struck_by_id(bubbles, bubble.entity_id)
                .is_none_or(|found| (found.radius - bubble.radius).abs() > 1e-6)
        })
        .count();
    if bubbles.is_empty() || lost > 0 {
        return Err(format!(
            "the lookup by id lost {lost} of {} presented bubbles",
            bubbles.len()
        )
        .into());
    }
    // The held beams (WP7): one held for the retargeting stream, the beams at dead targets held
    // and released, none refused.
    let beams = world.resource::<client::effects::beams::HeldBeams>();
    let beam_holds = catalog.effect_id("beam").map_or(0, |beam| {
        world
            .resource::<client::effects::runtime::EffectRuntime>()
            .spawned_of(beam)
    });
    // Since review round 1 (R2-2) a hold the effect pool cannot place is refused and counted:
    // the stream's fixture pool (64 effects) is at times full when a dead-target beam is
    // struck, so at most those strikes, one every seventh frame since the app started, may be
    // refused. The retargeting beam is held throughout.
    let dead_strikes = (u64::from(WARM_FRAMES) + u64::from(frames)) / 7 + 1;
    if beams.slots().len() != 1 || beams.refused > dead_strikes || beam_holds < 2 {
        return Err(format!(
            "the beam stream must hold one beam and refuse only dead-target beams, held {} \
             (refused {}), \
             {beam_holds} held in all",
            beams.slots().len(),
            beams.refused
        )
        .into());
    }
    let struck = world.resource::<client::effects::struck::StruckBubbles>();
    // Every frame's projectile finds its shooter's bubble in the presented frame (B-133).
    let shooters = struck.shooter_found_in(client::effects::struck::StruckFrom::Presented);
    if shooters < u64::from(frames) {
        return Err(format!(
            "the stream's projectiles found {shooters} shooters' bubbles in the presented frame \
             in {frames} frames"
        )
        .into());
    }
    let (presented, nowhere) = (
        struck.found_in(client::effects::struck::StruckFrom::Presented),
        struck.found_in(client::effects::struck::StruckFrom::Nowhere),
    );
    // A beam at a target that died (every seventh frame since the app started, the counts
    // being the run's) finds its bubble nowhere, as a killing hit's would after its target
    // left every frame kept; nothing else may.
    if presented < 2 * u64::from(frames)
        || nowhere > (u64::from(WARM_FRAMES) + u64::from(frames)) / 7 + 1
    {
        return Err(format!(
            "the stream's hits and blows found {presented} struck bubbles in the presented \
             frame and {nowhere} nowhere in {frames} frames"
        )
        .into());
    }
    Ok(())
}

/// Effects spawned so far, 0 without the effect runtime.
fn effects_spawned(app: &App) -> u64 {
    app.world()
        .get_resource::<client::effects::runtime::EffectRuntime>()
        .map_or(0, |runtime| runtime.stats.spawned)
}

#[test]
fn effect_stream_frames() -> TestResult {
    run_child(Variant::EffectStream)
}

#[test]
fn effect_baseline_frames() -> TestResult {
    run_child(Variant::EffectBaseline)
}

#[test]
fn live_sync_frames() -> TestResult {
    run_child(Variant::LiveSync)
}

#[test]
fn baseline_frames() -> TestResult {
    run_child(Variant::Baseline)
}

#[test]
fn planted_allocation_is_counted() -> TestResult {
    run_child(Variant::Planted)
}

#[test]
fn running_live_sync_frames() -> TestResult {
    run_child(Variant::RunningLive)
}

#[test]
fn running_baseline_frames() -> TestResult {
    run_child(Variant::RunningBaseline)
}

#[test]
fn brood_cycle_live_frames() -> TestResult {
    run_child(Variant::BroodCycleLive)
}

#[test]
fn brood_cycle_baseline_frames() -> TestResult {
    run_child(Variant::BroodCycleBaseline)
}

#[test]
fn hangar_cycle_live_frames() -> TestResult {
    run_child(Variant::HangarCycleLive)
}

#[test]
fn hangar_cycle_baseline_frames() -> TestResult {
    run_child(Variant::HangarCycleBaseline)
}

#[test]
fn smart_mixed_live_frames() -> TestResult {
    run_child(Variant::SmartMixedLive)
}

#[test]
fn smart_mixed_baseline_frames() -> TestResult {
    run_child(Variant::SmartMixedBaseline)
}

#[test]
fn razing_cycle_live_frames() -> TestResult {
    run_child(Variant::RazingCycleLive)
}

#[test]
fn razing_cycle_baseline_frames() -> TestResult {
    run_child(Variant::RazingCycleBaseline)
}

/// The parts of a DHAT profile (`dhatFileVersion` 2, JSON) this test reads.
#[derive(serde::Deserialize)]
struct DhatProfile {
    /// Program points: one per distinct allocation stack.
    pps: Vec<ProgramPoint>,
    /// Frame table: the text of every stack frame, indexed by `ProgramPoint::fs`.
    ftbl: Vec<String>,
}

/// One allocation stack of a DHAT profile.
#[derive(serde::Deserialize)]
struct ProgramPoint {
    /// Total blocks allocated at this stack over the whole run.
    tbk: u64,
    /// The stack, innermost frame first, as indices into `DhatProfile::ftbl`.
    fs: Vec<usize>,
}

/// What one child's profile says about the measured window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowCount {
    /// Blocks allocated with `WINDOW_FUNCTION` on the stack.
    window: u64,
    /// Blocks allocated over the whole process.
    total: u64,
}

/// Counts the blocks of `profile` allocated inside the measured window. Fails if a stack reached
/// the recorded depth limit. DHAT names only frames of allocating stacks, so a window that
/// allocates nothing leaves no trace of `WINDOW_FUNCTION`; the planted control proves the scoping
/// sees the window when it does allocate.
fn window_count(profile: &DhatProfile) -> Result<WindowCount, String> {
    let window_frames: Vec<bool> = profile
        .ftbl
        .iter()
        .map(|frame| frame.contains(WINDOW_FUNCTION))
        .collect();
    let mut count = WindowCount {
        window: 0,
        total: 0,
    };
    for point in &profile.pps {
        if point.fs.len() >= STACK_DEPTH {
            return Err(format!(
                "a stack of {} frames reached the depth limit {STACK_DEPTH}",
                point.fs.len()
            ));
        }
        count.total += point.tbk;
        if point
            .fs
            .iter()
            .any(|&frame| window_frames.get(frame).copied().unwrap_or(false))
        {
            count.window += point.tbk;
        }
    }
    Ok(count)
}

/// Reads a DHAT profile. DHAT writes JSON; every JSON document it writes is also valid RON, so the
/// workspace's existing `ron` reader parses it without a new dependency.
fn read_profile(path: &Path) -> Result<DhatProfile, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value: ron::Value = ron::from_str(&text)
        .map_err(|error| format!("{} is not a DHAT profile: {error}", path.display()))?;
    value
        .into_rust()
        .map_err(|error| format!("{} has no `pps`/`ftbl`: {error}", path.display()))
}

/// One child run under valgrind.
#[derive(Clone, Copy, Debug)]
struct Run {
    variant: Variant,
    frames: u32,
    repeat: u32,
}

/// Runs this test binary's child for `run` under DHAT, writing its profile into `directory`, and
/// returns the child's window count.
fn count_allocations(run: Run, directory: &Path) -> Result<WindowCount, String> {
    let binary = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
    let profile = directory.join(format!(
        "{}.{}.{}.json",
        run.variant.child_test(),
        run.frames,
        run.repeat
    ));
    let output = Command::new("valgrind")
        .arg("--tool=dhat")
        .arg(format!("--num-callers={STACK_DEPTH}"))
        .arg(format!("--dhat-out-file={}", profile.display()))
        .arg(&binary)
        .args([
            "--exact",
            run.variant.child_test(),
            "--nocapture",
            "--test-threads=1",
        ])
        .env(FRAMES_ENV, run.frames.to_string())
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "valgrind is not installed. Install it (Debian or \
                 Ubuntu: `sudo apt-get install valgrind`); this test counts allocations with it \
                 and has no skip"
                .to_owned(),
            _ => format!("cannot start valgrind: {error}"),
        })?;
    if !output.status.success() {
        return Err(format!(
            "{run:?} failed under valgrind ({}):\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    window_count(&read_profile(&profile)?).map_err(|error| format!("{run:?}: {error}"))
}

/// Runs every child of `runs` under valgrind, at most `available_parallelism` at a time, and
/// returns their counts in the same order.
fn count_all(runs: &[Run], directory: &Path) -> Result<Vec<WindowCount>, String> {
    let width = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .clamp(1, runs.len().max(1));
    let mut counts = Vec::with_capacity(runs.len());
    for batch in runs.chunks(width) {
        let results: Vec<Result<WindowCount, String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|&run| scope.spawn(move || count_allocations(run, directory)))
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err("a valgrind runner thread panicked".to_owned()))
                })
                .collect()
        });
        for result in results {
            counts.push(result?);
        }
    }
    Ok(counts)
}

/// The window counts of one variant: every run at the short window, then every run at the long.
#[derive(Debug)]
struct Counts {
    variant: Variant,
    short: Vec<WindowCount>,
    long: Vec<WindowCount>,
}

impl Counts {
    /// Allocations the longer window adds over `LONG_FRAMES - SHORT_FRAMES` more frames.
    fn difference(&self) -> Option<i128> {
        let short = self.short.first()?.window;
        let long = self.long.first()?.window;
        Some(i128::from(long) - i128::from(short))
    }

    /// Whether every run at each frame count reported the same window count.
    fn is_reproducible(&self) -> bool {
        let same =
            |runs: &[WindowCount]| runs.windows(2).all(|pair| pair[0].window == pair[1].window);
        same(&self.short) && same(&self.long)
    }

    /// One report line: window and whole-process counts of every run, and the difference.
    fn line(&self) -> String {
        let show = |runs: &[WindowCount]| {
            runs.iter()
                .map(|run| format!("{} (process {})", run.window, run.total))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "  {:<9?} {SHORT_FRAMES} frames: {}; {LONG_FRAMES} frames: {}; difference {:?}",
            self.variant,
            show(&self.short),
            show(&self.long),
            self.difference()
        )
    }
}

/// Runs every variant's children and groups their counts. The live child runs twice at each frame
/// count, to show the asserted count is deterministic; the baseline and the planted control once.
/// Twelve children fit three batches on a four-core CI runner.
fn measure_all_variants(directory: &Path) -> Result<Vec<Counts>, String> {
    let plan = [
        (Variant::LiveSync, 2),
        (Variant::Baseline, 1),
        (Variant::Planted, 1),
        (Variant::RunningLive, 1),
        (Variant::RunningBaseline, 1),
        (Variant::EffectStream, 1),
        (Variant::EffectBaseline, 1),
    ];
    let runs: Vec<Run> = plan
        .iter()
        .flat_map(|&(variant, repeats)| {
            [SHORT_FRAMES, LONG_FRAMES]
                .into_iter()
                .flat_map(move |frames| {
                    (0..repeats).map(move |repeat| Run {
                        variant,
                        frames,
                        repeat,
                    })
                })
        })
        .collect();
    let counts = count_all(&runs, directory)?;
    Ok(plan
        .iter()
        .map(|&(variant, _)| {
            let of = |frames: u32| {
                runs.iter()
                    .zip(&counts)
                    .filter(|(run, _)| run.variant == variant && run.frames == frames)
                    .map(|(_, &count)| count)
                    .collect()
            };
            Counts {
                variant,
                short: of(SHORT_FRAMES),
                long: of(LONG_FRAMES),
            }
        })
        .collect())
}

#[test]
fn steady_state_frames_allocate_nothing() -> TestResult {
    let directory = tempfile::tempdir()?;
    let counts = measure_all_variants(directory.path())?;
    let [live, baseline, planted, running_live, running_baseline, effect_stream, effect_baseline] =
        counts.as_slice()
    else {
        return Err("invariant: measure_all_variants returns one entry per variant".into());
    };
    let report = format!(
        "blocks allocated inside {WINDOW_FUNCTION} after {WARM_FRAMES} warm frames:\n{}",
        counts
            .iter()
            .map(Counts::line)
            .collect::<Vec<_>>()
            .join("\n")
    );
    println!("{report}");
    let (
        Some(live_difference),
        Some(baseline_difference),
        Some(planted_difference),
        Some(running_live_difference),
        Some(running_baseline_difference),
        Some(effect_stream_difference),
        Some(effect_baseline_difference),
    ) = (
        live.difference(),
        baseline.difference(),
        planted.difference(),
        running_live.difference(),
        running_baseline.difference(),
        effect_stream.difference(),
        effect_baseline.difference(),
    )
    else {
        return Err(format!("a variant has no run at one of the windows\n{report}").into());
    };
    let window = i128::from(LONG_FRAMES - SHORT_FRAMES);
    assert!(
        live.is_reproducible(),
        "identical runs must count identical window totals\n{report}"
    );
    assert_eq!(
        planted_difference - live_difference,
        window,
        "the planted control must add exactly one counted allocation per frame\n{report}"
    );
    assert_eq!(
        live_difference - baseline_difference,
        0,
        "the client's battle sync systems allocated {} times over {window} steady frames \
         beyond Bevy's own work\n{report}",
        live_difference - baseline_difference,
    );
    assert_eq!(
        running_live_difference - running_baseline_difference,
        0,
        "the client's battle sync systems allocated {} times over {window} running frames \
         beyond the tick driver's and Bevy's own work\n{report}",
        running_live_difference - running_baseline_difference,
    );
    assert_eq!(
        effect_stream_difference - effect_baseline_difference,
        0,
        "the effect systems allocated {} times over {window} frames of a splash stream\n{report}",
        effect_stream_difference - effect_baseline_difference,
    );
    Ok(())
}

/// The Broodlings' longest life `L_max`, in ticks, read from the shipped default battle as the
/// children stage it.
fn brood_lifetime() -> Result<u32, Box<dyn std::error::Error>> {
    let root = client::ResolvedAssetsRoot::from_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut simulation = definition.instantiate()?;
    Ok(unit_cycling::stage_brood(&mut simulation).lifetime)
}

/// R-1v (plan `2026-11-29-render-entity-pool` §6.1, B-19 under valgrind): on the staged
/// Broodlings' battle, after a warm-up of `4 × L_max` ticks, the live synchroniser's windows of
/// `2 × L_max` and `4 × L_max` ticks differ by exactly what the battle driver's alone differ by:
/// Broodlings arriving and expiring cost the client no allocation. The baseline keeps the tick
/// driver, so the simulation's own allocations for spawning and expiry cancel.
#[test]
fn brood_cycle_frames_allocate_nothing() -> TestResult {
    let lifetime = brood_lifetime()?;
    let (short, long) = (2 * lifetime, 4 * lifetime);
    let runs: Vec<Run> = [Variant::BroodCycleLive, Variant::BroodCycleBaseline]
        .into_iter()
        .flat_map(|variant| {
            [short, long].map(|frames| Run {
                variant,
                frames,
                repeat: 0,
            })
        })
        .collect();
    let directory = tempfile::tempdir()?;
    let counts = count_all(&runs, directory.path())?;
    let [live_short, live_long, base_short, base_long] = counts.as_slice() else {
        return Err("invariant: count_all returns one count per run".into());
    };
    let report = format!(
        "blocks allocated inside {WINDOW_FUNCTION} after {} warm ticks; windows of {short} and \
         {long} ticks:\n  live {} (process {}), {} (process {})\n  baseline {} (process {}), \
         {} (process {})",
        4 * lifetime,
        live_short.window,
        live_short.total,
        live_long.window,
        live_long.total,
        base_short.window,
        base_short.total,
        base_long.window,
        base_long.total
    );
    println!("{report}");
    let live = i128::from(live_long.window) - i128::from(live_short.window);
    let baseline = i128::from(base_long.window) - i128::from(base_short.window);
    assert_eq!(
        live - baseline,
        0,
        "the client's battle sync systems allocated {} times over {} ticks of Broodlings \
         arriving and expiring, beyond the tick driver's and Bevy's own work\n{report}",
        live - baseline,
        long - short
    );
    Ok(())
}

/// H-20 under valgrind (plan `2026-11-28-carrier-interceptors` §6.4, plan
/// `2026-11-29-render-entity-pool` §6.5): on the staged Carriers' battle, after a warm-up of
/// three cycles, the live synchroniser's windows of two and four cycles (each the last warm-up
/// cycle's length, measured in the child) differ by exactly what the battle driver's and the
/// cycle driver's alone differ by: interceptors launching, docking and relaunching cost the
/// client no allocation.
#[test]
fn hangar_cycle_frames_allocate_nothing() -> TestResult {
    let runs: Vec<Run> = [Variant::HangarCycleLive, Variant::HangarCycleBaseline]
        .into_iter()
        .flat_map(|variant| {
            [2, 4].map(|frames| Run {
                variant,
                frames,
                repeat: 0,
            })
        })
        .collect();
    let directory = tempfile::tempdir()?;
    let counts = count_all(&runs, directory.path())?;
    let [live_short, live_long, base_short, base_long] = counts.as_slice() else {
        return Err("invariant: count_all returns one count per run".into());
    };
    let report = format!(
        "blocks allocated inside {WINDOW_FUNCTION} after {HANGAR_WARM_CYCLES} warm cycles; \
         windows of 2 and 4 cycles:\n  live {} (process {}), {} (process {})\n  baseline {} \
         (process {}), {} (process {})",
        live_short.window,
        live_short.total,
        live_long.window,
        live_long.total,
        base_short.window,
        base_short.total,
        base_long.window,
        base_long.total
    );
    println!("{report}");
    let live = i128::from(live_long.window) - i128::from(live_short.window);
    let baseline = i128::from(base_long.window) - i128::from(base_short.window);
    assert_eq!(
        live - baseline,
        0,
        "the client's battle sync systems allocated {} times over two cycles of interceptors \
         launching, docking and relaunching, beyond the drivers' and Bevy's own work\n{report}",
        live - baseline
    );
    Ok(())
}

/// DS-18 under valgrind (plan `2026-12-01-death-spawns-and-hangar-commands` §6.3): on the staged
/// razing battle (an enemy structure with the shipped death spawn with the most units raised and
/// killed every 200 ticks), after a warm-up of 800 ticks, the live synchroniser's windows of 400
/// and 800 ticks differ by exactly what the battle driver's and the razing driver's alone differ
/// by: units arriving from a death and expiring cost the client no allocation.
#[test]
fn razing_cycle_frames_allocate_nothing() -> TestResult {
    let period = unit_cycling::RAZING_PERIOD_TICKS;
    let (short, long) = (2 * period, 4 * period);
    let runs: Vec<Run> = [Variant::RazingCycleLive, Variant::RazingCycleBaseline]
        .into_iter()
        .flat_map(|variant| {
            [short, long].map(|frames| Run {
                variant,
                frames,
                repeat: 0,
            })
        })
        .collect();
    let directory = tempfile::tempdir()?;
    let counts = count_all(&runs, directory.path())?;
    let [live_short, live_long, base_short, base_long] = counts.as_slice() else {
        return Err("invariant: count_all returns one count per run".into());
    };
    let report = format!(
        "blocks allocated inside {WINDOW_FUNCTION} after {} warm ticks; windows of {short} and \
         {long} ticks:\n  live {} (process {}), {} (process {})\n  baseline {} (process {}), \
         {} (process {})",
        4 * period,
        live_short.window,
        live_short.total,
        live_long.window,
        live_long.total,
        base_short.window,
        base_short.total,
        base_long.window,
        base_long.total
    );
    println!("{report}");
    let live = i128::from(live_long.window) - i128::from(live_short.window);
    let baseline = i128::from(base_long.window) - i128::from(base_short.window);
    assert_eq!(
        live - baseline,
        0,
        "the client's battle sync systems allocated {} times over {} ticks of death-spawned \
         units arriving and expiring, beyond the drivers' and Bevy's own work\n{report}",
        live - baseline,
        long - short
    );
    Ok(())
}

#[test]
fn window_count_attributes_blocks_by_stack_and_rejects_truncation() {
    let profile = DhatProfile {
        ftbl: vec![
            "[root]".to_owned(),
            "0x1: client::sync (scene_controller.rs:1)".to_owned(),
            format!("0x2: tests::{WINDOW_FUNCTION} (tests.rs:2)"),
            "0x3: tests::steady_state_app (tests.rs:3)".to_owned(),
        ],
        pps: vec![
            ProgramPoint {
                tbk: 5,
                fs: vec![1, 2],
            },
            ProgramPoint {
                tbk: 7,
                fs: vec![3],
            },
        ],
    };
    assert_eq!(
        window_count(&profile),
        Ok(WindowCount {
            window: 5,
            total: 12
        })
    );
    let truncated = DhatProfile {
        ftbl: profile.ftbl.clone(),
        pps: vec![ProgramPoint {
            tbk: 1,
            fs: vec![2; STACK_DEPTH],
        }],
    };
    assert!(window_count(&truncated).is_err());
    let quiet = DhatProfile {
        ftbl: vec![
            "[root]".to_owned(),
            "0x3: tests::setup (tests.rs:3)".to_owned(),
        ],
        pps: vec![ProgramPoint {
            tbk: 4,
            fs: vec![1],
        }],
    };
    assert_eq!(
        window_count(&quiet),
        Ok(WindowCount {
            window: 0,
            total: 4
        })
    );
}

#[test]
fn dhat_json_reads_through_ron() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("profile.json");
    std::fs::write(
        &path,
        "{\"dhatFileVersion\":2\n,\"mode\":\"heap\",\"bklt\":true\n,\"pps\":\n \
         [{\"tb\":472,\"tbk\":3\n  ,\"acc\":[294,-2,211]\n  ,\"fs\":[1,2]\n  }\n ]\n\
         ,\"ftbl\":\n [\"[root]\"\n ,\"0x4: tests::measured_steady_frames (a.rs:1)\"\n ]\n}\n",
    )?;
    let profile = read_profile(&path)?;
    assert_eq!(
        window_count(&profile)?,
        WindowCount {
            window: 3,
            total: 3
        }
    );
    Ok(())
}

/// P-6 under valgrind (plan `2026-12-01-rallies-and-smart-command`, the variant `smart_mixed`):
/// after a mixed right click on the local transport, queued before the warm-up, the live
/// synchroniser's windows of [`SHORT_FRAMES`] and [`LONG_FRAMES`] running frames differ by
/// exactly what the battle driver's alone differ by, while two boarders walk and an air unit
/// follows: the mixed orders cost the live battle sync no allocation. The baseline keeps the tick
/// driver and the same click, so the simulation's own allocations cancel.
#[test]
fn smart_mixed_frames_allocate_nothing() -> TestResult {
    let runs: Vec<Run> = [Variant::SmartMixedLive, Variant::SmartMixedBaseline]
        .into_iter()
        .flat_map(|variant| {
            [SHORT_FRAMES, LONG_FRAMES].map(|frames| Run {
                variant,
                frames,
                repeat: 0,
            })
        })
        .collect();
    let directory = tempfile::tempdir()?;
    let counts = count_all(&runs, directory.path())?;
    let [live_short, live_long, base_short, base_long] = counts.as_slice() else {
        return Err("invariant: count_all returns one count per run".into());
    };
    let report = format!(
        "blocks allocated inside {WINDOW_FUNCTION} after {WARM_FRAMES} warm frames; windows of \
         {SHORT_FRAMES} and {LONG_FRAMES} frames:\n  live {} (process {}), {} (process {})\n  \
         baseline {} (process {}), {} (process {})",
        live_short.window,
        live_short.total,
        live_long.window,
        live_long.total,
        base_short.window,
        base_short.total,
        base_long.window,
        base_long.total
    );
    println!("{report}");
    let live = i128::from(live_long.window) - i128::from(live_short.window);
    let baseline = i128::from(base_long.window) - i128::from(base_short.window);
    assert_eq!(
        live - baseline,
        0,
        "the client's battle sync systems allocated {} times over {} frames of mixed smart \
         orders running, beyond the tick driver's and Bevy's own work\n{report}",
        live - baseline,
        LONG_FRAMES - SHORT_FRAMES
    );
    Ok(())
}
