//! Real authoritative trace production and cadence-faithful offline presentation.
//!
//! Saved tick outputs retain original commands/events/checksums. Replaying them uses
//! the public clock/timeline; it is provenance verification, not independent determinism.

use super::{AdmissionCase, AdmissionCorpus, AdmissionError, MotionFamily};
use crate::battle_pipeline::{
    clock::{BattleClockPolicy, BattleTickClock},
    commands::SessionGeneration,
    timeline::BattleFrameState,
};
use crate::mist::{
    emission::EmitterHistory,
    reduction::{reduce_history, ReducedWake, ReductionError, ReductionScratch},
};
use bevy::{
    math::Vec3,
    prelude::{App, Time},
    time::{Real, TimePlugin, TimeUpdateStrategy, Virtual},
};
use serde::Serialize;
use sim_core::{
    movement::MovementOutcome, production::ProductionOutcome, SimWorld, SimulationDiagnostic,
};
use sim_types::{
    BubbleRenderInstance, EntityId, PlayerCommand, PlayerId, RenderSnapshot, SimScalar, SimVec2,
    Tick, TickBundle,
};
use std::time::{Duration, Instant};

/// Stable failure groups bound the number of full retained diagnostic windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum FailureCategory {
    /// Actual world/command setup or simulation diagnostics prevented the experiment.
    Producer,
    /// Actual displayed movement or required capability behavior was absent.
    Coverage,
    /// Original retained producer data was invalid.
    InvalidHistory,
    /// Rounded controls or ideal support arithmetic are unrepresentable.
    Representation,
    /// The exact tolerances require more controls than the reviewed bound.
    ControlBudget,
    /// A real teleport was presented as subthreshold motion with emitted bridging.
    Discontinuity,
}

/// Explicit unsuccessful observation, retained with its first frame.
#[derive(Clone, Debug, Serialize)]
pub struct WindowFailure {
    /// Finite retention category; no category is silently dropped.
    pub category: FailureCategory,
    /// First observed display frame, zero for a producer/setup refusal.
    pub frame: u32,
    /// Original typed error's complete diagnostic rendering.
    pub diagnostic: String,
}

#[derive(Clone, Debug)]
pub(super) struct TickRecord {
    pub bundle: TickBundle,
    pub output: RenderSnapshot,
    pub direct_checksum: u64,
    pub movement: Vec<MovementOutcome>,
    pub production: Vec<ProductionOutcome>,
    pub diagnostics: Vec<SimulationDiagnostic>,
}

/// Exact transaction evidence from the real cast and an otherwise identical no-cast tick.
#[derive(Clone, Debug, Serialize)]
pub struct BlinkTransaction {
    /// Input tick before the real cast executes.
    pub tick: u32,
    /// Actual pre-cast position and admitted requested destination.
    pub from: SimVec2,
    /// Requested destination from the exact tick bundle.
    pub destination: SimVec2,
    /// Actual position after the complete cast tick.
    pub actual_position: SimVec2,
    /// Actual energy before the cast.
    pub before_energy: SimScalar,
    /// Actual energy after the complete cast tick.
    pub actual_energy: SimScalar,
    /// Expected post-tick energy after declared cost and actual regeneration ordering.
    pub expected_energy: SimScalar,
    /// The separately stepped no-cast output, including its checksum and events.
    pub no_cast_output: RenderSnapshot,
    /// Commands actually issued to that control world.
    pub no_cast_bundle: TickBundle,
    /// Actual control position after Stop-only executes through all tick phases.
    pub no_cast_position: SimVec2,
    /// Actual control energy after the otherwise identical tick.
    pub no_cast_energy: SimScalar,
    /// Actual post-tick cooldown.
    pub actual_cooldown: u16,
    /// Expected cooldown after the ordinary tick decrement.
    pub expected_cooldown: u16,
    /// All recorded actual transaction fields satisfy the declared mechanism contract.
    pub admitted: bool,
}

/// Independent authoritative capability observations, never inferred from reducer success.
#[derive(Clone, Debug, Serialize)]
pub struct TraceCoverage {
    /// The subject remained alive through every generated boundary.
    pub subject_alive: bool,
    /// Actual second actor remained alive, when this family uses one.
    pub target_alive: Option<bool>,
    /// Smallest observed center distance between the actual pair, in world units.
    pub minimum_pair_distance: Option<f64>,
    /// Sum of the actual pair's authored separation radii, in world units.
    pub separation_envelope: Option<f64>,
    /// Actual pair moved closer than its initial distance into that separation envelope.
    pub same_layer_near_approach: bool,
    /// The subject actually retained a pursuit/attack-move order after command admission.
    pub pursuit_order_observed: bool,
}

pub(super) struct AuthoritativeTrace {
    pub case: AdmissionCase,
    pub speed: u32,
    pub subject: EntityId,
    pub other: Option<EntityId>,
    pub initial: RenderSnapshot,
    pub initial_direct_checksum: u64,
    pub ticks: Vec<TickRecord>,
    pub dimensions: [u32; 2],
    pub elapsed_seconds: f64,
    pub failures: Vec<WindowFailure>,
    pub coverage: TraceCoverage,
    pub control_ticks: u32,
    pub blink_transaction: Option<BlinkTransaction>,
    pub blink_tick: Option<u32>,
    pub blink_displacement: Option<[f32; 2]>,
}

/// A copied retained source value; all values come from actual history getters.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceSample {
    /// Original world-space emission position.
    pub position: [f32; 3],
    /// Original emitter-local birth in presentation seconds.
    pub birth: f64,
    /// Original actor-path distance, unaffected by capacity eviction.
    pub path: f32,
}

/// One actual display observation, with independently named time domains.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DisplayFrame {
    /// Zero-based display observation after the initial zero-time update.
    pub frame: u32,
    /// Requested manual raw duration, checked to fit nanoseconds in u64.
    pub requested_ns: u64,
    /// Actual Time<Real> delta, before the virtual clamp.
    pub real_ns: u64,
    /// Actual default virtual delta consumed by history and clock observation.
    pub virtual_ns: u64,
    /// Actual elapsed virtual seconds, unscaled by battle speed.
    pub virtual_elapsed: f64,
    /// Effective surrounding battle speed; zero during the speed-zero family.
    pub speed: u32,
    /// True when battle presentation is suspended; virtual time still runs.
    pub suspended: bool,
    /// Executed complete ticks in this frame.
    pub ticks: u32,
    /// Actual latest authoritative tick after the frame.
    pub current_tick: u32,
    /// Actual displayed discrete tick.
    pub presented_tick: u32,
    /// Retained clock debt after consuming eligible ticks.
    pub debt_ns: u64,
    /// Exact scaled duration discarded in this frame.
    pub discarded_ns: u64,
    /// Actual subject position after snapshot-to-mist conversion.
    pub position: Option<[f32; 3]>,
    /// Observed 3D displacement since the previous displayed pose.
    pub displacement: f32,
    /// Actual strict continuity predicate; a cause observation, not an internal counter.
    pub discontinuity_predicate: bool,
    /// This frame crosses the actual cast tick or interpolates its adjacent tick pair.
    pub blink_transition_tick: Option<u32>,
    /// Retained samples born no later than this frame's pre-advance history clock.
    pub retained_previous_samples: usize,
    /// Actual retained samples born after this frame's pre-advance history clock.
    pub newly_emitted_samples: usize,
    /// Explicit owner/enable lifecycle reset performed before this observation.
    pub lifecycle_reset: bool,
    /// Actual emitter-local clock after this observation.
    pub history_clock: f64,
    /// Retained chronological samples, including emitted original path coordinates.
    pub samples: Vec<SourceSample>,
    /// Complete successful reducer diagnostics, when present.
    pub reduction: Option<String>,
    /// Actual first reducer rejection at this frame, if any.
    pub rejection: Option<String>,
}

/// Bounded per-window result; no rendering or optical acceptance is implied.
#[derive(Clone, Debug, Serialize)]
pub struct WindowSummary {
    /// Exact catalog/family identity.
    pub case: AdmissionCase,
    /// Surrounding positive battle speed.
    pub speed: u32,
    /// Display rate in frames per second.
    pub rate: u32,
    /// Initial offset around the 40ms tick in nanoseconds.
    pub phase_offset_ns: i64,
    /// Actual evaluated display frames.
    pub frames: u32,
    /// Actual TimePlugin updates attempted, including a partially failed final frame.
    pub attempted_frames: u32,
    /// True only when the whole declared effective-virtual horizon was observed.
    pub completed_horizon: bool,
    /// Declared finite upper bound on display updates.
    pub frame_bound: u32,
    /// Actually consumed saved authoritative outputs.
    pub ticks: u32,
    /// Frames with at least one positive spatial segment.
    pub moving_histories: u32,
    /// Largest returned control count among successful reductions.
    pub maximum_controls: usize,
    /// Largest normalized admitted control error.
    pub maximum_error_ratio: f64,
    /// Greatest recorded relative source/reduced arc deficit.
    pub relative_arc_deficit_range: Option<[f64; 2]>,
    /// Signed absolute source-minus-reduced arc extrema, including negative rounding.
    pub absolute_arc_deficit_range: Option<[f64; 2]>,
    /// Actual reducer scored-source count, including failed candidate passes when available.
    pub source_evaluations: u64,
    /// Actual presentation event count consumed exactly once.
    pub presented_events: u64,
    /// Observed world-space motion over displayed frames.
    pub displayed_travel: f64,
    /// Actual minimum/maximum snapshot ground height.
    pub ground_range: Option<[f32; 2]>,
    /// Actual frames whose snapshot reports the supported speed boost.
    pub boosted_frames: u32,
    /// Actual cumulative discarded scaled clock duration.
    pub discarded_ns: u64,
    /// Authoritative capability observations independent of reduction outcomes.
    pub coverage: Option<TraceCoverage>,
    /// Exact actual generic cast/no-cast result, independent of reducer acceptance.
    pub blink_transaction: Option<BlinkTransaction>,
    /// First failure, if any; remaining frames are still evaluated and recorded.
    pub failure: Option<WindowFailure>,
    /// First occurrence of every distinct observed failure category in this window.
    pub failures: Vec<WindowFailure>,
}

pub(super) struct WindowResult {
    pub summary: WindowSummary,
    pub frames: Vec<DisplayFrame>,
}

struct Setup {
    world: SimWorld,
    subject: EntityId,
    other: Option<EntityId>,
    origin: SimVec2,
    extent: SimScalar,
    radius: SimScalar,
    ticks: u32,
    block_ticks: u32,
    dimensions: [u32; 2],
}

fn arithmetic<T>(value: Option<T>) -> Result<T, AdmissionError> {
    value.ok_or(AdmissionError::Specification("fixture arithmetic overflow"))
}

fn scalar(value: u32) -> Result<SimScalar, AdmissionError> {
    Ok(SimScalar::from_i32(i32::try_from(value).map_err(|_| {
        AdmissionError::Specification("fixed integer range")
    })?))
}

fn fixed_visual(value: f32) -> Result<SimScalar, AdmissionError> {
    if !value.is_finite() || f64::from(value).abs() > f64::from(i32::MAX) {
        return Err(AdmissionError::Specification(
            "visual style cannot supply a fixed fixture value",
        ));
    }
    Ok(SimScalar::from_num(f64::from(value)))
}

fn scale(point: SimVec2, amount: SimScalar) -> Result<SimVec2, AdmissionError> {
    Ok(SimVec2::new(
        arithmetic(point.x.checked_mul(amount))?,
        arithmetic(point.y.checked_mul(amount))?,
    ))
}

fn add(a: SimVec2, b: SimVec2) -> Result<SimVec2, AdmissionError> {
    Ok(SimVec2::new(
        arithmetic(a.x.checked_add(b.x))?,
        arithmetic(a.y.checked_add(b.y))?,
    ))
}

fn point(values: [SimScalar; 2]) -> SimVec2 {
    SimVec2::new(values[0], values[1])
}

fn ceil_positive(value: SimScalar) -> Result<u32, AdmissionError> {
    let raw = u64::try_from(value.raw())
        .map_err(|_| AdmissionError::Specification("negative map extent"))?;
    u32::try_from(raw.div_ceil(1_u64 << 32))
        .map_err(|_| AdmissionError::Specification("map extent range"))
}

fn pursuit_target<'a>(
    corpus: &'a AdmissionCorpus,
    subject: &sim_core::UnitArchetype,
) -> Result<&'a sim_core::UnitArchetype, AdmissionError> {
    corpus
        .catalogs
        .units
        .archetypes()
        .filter(|target| {
            !target.is_immobile_by_default()
                && subject.weapon_against(target.mover_type, None).is_some()
        })
        .max_by_key(|target| {
            (
                target.weapon_against(subject.mover_type, None).is_none(),
                target.max_speed >= subject.max_speed,
                i128::from(target.max_hp.raw()) + i128::from(target.max_shields.raw()),
                target.max_speed,
            )
        })
        .ok_or(AdmissionError::Specification(
            "weapon has no compatible actual mobile target",
        ))
}

fn pursuit_separation(
    subject: &sim_core::UnitArchetype,
    target: &sim_core::UnitArchetype,
) -> Result<SimScalar, AdmissionError> {
    let weapon =
        subject
            .weapon_against(target.mover_type, None)
            .ok_or(AdmissionError::Specification(
                "target weapon admission changed",
            ))?;
    arithmetic(
        arithmetic(
            arithmetic(weapon.max_range.checked_add(subject.collision_radius))?
                .checked_add(target.collision_radius),
        )?
        .checked_add(SimScalar::ONE),
    )
}

fn setup(
    corpus: &AdmissionCorpus,
    case: &AdmissionCase,
    speed: u32,
) -> Result<Setup, AdmissionError> {
    let ticks = u32::try_from(corpus.horizon.as_nanos() * u128::from(speed) / 40_000_000)
        .map_err(|_| AdmissionError::Specification("authoritative horizon"))?;
    let block_ticks = ticks / 4;
    let mut catalogs = corpus.catalogs.clone();
    let original = catalogs.units.get(case.unit_type)?.clone();
    let radius = original.collision_radius;
    let maximum_speed = if let Some(overclock) = original.overclock {
        arithmetic(
            original
                .max_speed
                .checked_mul(overclock.speed_multiplier.max(SimScalar::ONE)),
        )?
    } else {
        original.max_speed
    };
    let mut extent = arithmetic(maximum_speed.checked_mul(scalar(ticks)?))?
        .max(arithmetic(radius.checked_mul(SimScalar::from_i32(10)))?);
    if case.family == MotionFamily::Pursuit {
        let target = pursuit_target(corpus, &original)?;
        extent = extent.max(arithmetic(
            arithmetic(
                maximum_speed
                    .max(target.max_speed)
                    .checked_mul(scalar(ticks)?),
            )?
            .checked_add(pursuit_separation(&original, target)?),
        )?);
    }
    if matches!(case.family, MotionFamily::Blink { .. }) {
        let arch = catalogs.units.get_mut(case.unit_type)?;
        arch.max_energy = corpus.spec.blink_max_energy;
        arch.abilities = vec![corpus.spec.blink_ability.clone()];
        arch.capabilities.abilities.clone_from(&arch.abilities);
    }
    let margin = arithmetic(radius.checked_add(scalar(corpus.spec.map_margin)?))?;
    let lateral = arithmetic(extent.checked_mul(corpus.spec.zigzag_lateral_ratio))?
        .max(arithmetic(radius.checked_mul(SimScalar::from_i32(6)))?);
    let origin = SimVec2::new(
        arithmetic(extent.checked_add(margin))?,
        arithmetic(lateral.checked_add(margin))?,
    );
    let width = ceil_positive(arithmetic(origin.x.checked_mul(SimScalar::from_i32(2)))?)?;
    let height = ceil_positive(arithmetic(origin.y.checked_mul(SimScalar::from_i32(2)))?)?;
    let (map, origin) = if case.family == MotionFamily::Ramp {
        (
            sim_map::MapFile::load(&corpus.root.as_path().join(&corpus.spec.ramp_map))?
                .decode()?
                .to_game_map()?,
            point(corpus.spec.ramp_route[0]),
        )
    } else {
        (
            sim_map::GameMap::new(
                width,
                height,
                sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
            )?,
            origin,
        )
    };
    let dimensions = [map.width(), map.height()];
    let mut world =
        SimWorld::with_catalogs(map, corpus.spec.seed, catalogs.units, catalogs.upgrades);
    let subject = world.spawn_unit(case.unit_type, PlayerId(0), origin)?;
    let other = match case.family {
        MotionFamily::Collision => Some(world.spawn_unit(
            case.unit_type,
            PlayerId(0),
            add(
                origin,
                SimVec2::new(
                    arithmetic(radius.checked_mul(SimScalar::from_i32(4)))?,
                    SimScalar::ZERO,
                ),
            )?,
        )?),
        MotionFamily::Pursuit => {
            let target = pursuit_target(corpus, &original)?;
            let separation = pursuit_separation(&original, target)?;
            Some(world.spawn_unit(
                target.unit_type_id,
                PlayerId(1),
                add(origin, SimVec2::new(separation, SimScalar::ZERO))?,
            )?)
        }
        _ => None,
    };
    Ok(Setup {
        world,
        subject,
        other,
        origin,
        extent,
        radius,
        ticks,
        block_ticks,
        dimensions,
    })
}

fn command(player: PlayerId, entity: EntityId, order: sim_types::UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities: vec![entity],
        order,
        queue: false,
    }
}

fn commands(
    corpus: &AdmissionCorpus,
    case: &AdmissionCase,
    setup: &Setup,
    tick: u32,
) -> Result<Vec<PlayerCommand>, AdmissionError> {
    use sim_types::UnitOrder;
    let forward = add(setup.origin, SimVec2::new(setup.extent, SimScalar::ZERO))?;
    let reverse = add(setup.origin, SimVec2::new(-setup.extent, SimScalar::ZERO))?;
    let mut result = Vec::new();
    let mut order = (tick == 0).then_some(UnitOrder::Move { target: forward });
    match &case.family {
        MotionFamily::Zigzag { dwell_ticks } if tick % dwell_ticks == 0 => {
            let lateral = arithmetic(setup.extent.checked_mul(corpus.spec.zigzag_lateral_ratio))?;
            order = Some(UnitOrder::Move {
                target: add(
                    forward,
                    SimVec2::new(
                        SimScalar::ZERO,
                        if tick / dwell_ticks % 2 == 0 {
                            lateral
                        } else {
                            -lateral
                        },
                    ),
                )?,
            });
        }
        MotionFamily::Reversal if tick == setup.block_ticks * 2 => {
            order = Some(UnitOrder::Move { target: reverse });
        }
        MotionFamily::StopFade | MotionFamily::StopRestart if tick == setup.block_ticks => {
            order = Some(UnitOrder::Stop);
        }
        MotionFamily::StopRestart if tick == setup.block_ticks * 3 => {
            order = Some(UnitOrder::Move { target: reverse });
        }
        MotionFamily::Curve | MotionFamily::Boost => {
            let interval = (setup.ticks
                / u32::try_from(corpus.spec.curve_directions.len())
                    .map_err(|_| AdmissionError::Specification("curve directions"))?)
            .max(1);
            if tick % interval == 0 {
                let index = usize::try_from(tick / interval)
                    .map_err(|_| AdmissionError::Specification("curve index"))?
                    % corpus.spec.curve_directions.len();
                let radius = arithmetic(setup.radius.checked_mul(SimScalar::from_i32(4)))?;
                order = Some(UnitOrder::Move {
                    target: add(
                        setup.origin,
                        scale(point(corpus.spec.curve_directions[index]), radius)?,
                    )?,
                });
            }
            if case.family == MotionFamily::Boost && tick == 0 {
                result.push(command(PlayerId(0), setup.subject, UnitOrder::UseStim));
            }
        }
        MotionFamily::Collision if tick == 0 => {
            if let Some(other) = setup.other {
                result.push(command(
                    PlayerId(0),
                    other,
                    UnitOrder::Move { target: reverse },
                ));
            }
        }
        MotionFamily::Ramp if tick == 0 => {
            order = Some(UnitOrder::Move {
                target: point(corpus.spec.ramp_route[1]),
            });
        }
        MotionFamily::Pursuit => {
            if let Some(other) = setup.other {
                if tick == 0 {
                    result.push(command(
                        PlayerId(1),
                        other,
                        UnitOrder::Move { target: forward },
                    ));
                    order = Some(UnitOrder::AttackTarget { target: other });
                } else if tick == setup.block_ticks * 2 {
                    order = Some(UnitOrder::AttackMove { target: forward });
                }
            }
        }
        MotionFamily::Blink { offset_sign } if tick == setup.block_ticks => {
            let offset = arithmetic(
                setup
                    .radius
                    .checked_div(scalar(corpus.spec.blink_offset_divisor)?),
            )?;
            let threshold = arithmetic(
                setup
                    .radius
                    .checked_mul(fixed_visual(corpus.style.continuity_radii)?),
            )?;
            let displacement = arithmetic(threshold.checked_add(arithmetic(
                offset.checked_mul(SimScalar::from_i32(i32::from(*offset_sign))),
            )?))?;
            let actual = setup
                .world
                .entities
                .get(&setup.subject)
                .ok_or(AdmissionError::Specification("Blink subject unavailable"))?
                .position;
            result.push(command(PlayerId(0), setup.subject, UnitOrder::Stop));
            order = Some(UnitOrder::CastAbility {
                ability_index: 0,
                target_pos: Some(add(actual, SimVec2::new(displacement, SimScalar::ZERO))?),
                target_entity: None,
            });
        }
        MotionFamily::Blink { .. } if tick == setup.block_ticks + 1 => {
            order = Some(UnitOrder::Move { target: forward });
        }
        _ => {}
    }
    if let Some(order) = order {
        result.push(command(PlayerId(0), setup.subject, order));
    }
    Ok(result)
}

fn pair_distance(
    snapshot: &RenderSnapshot,
    subject: EntityId,
    other: Option<EntityId>,
) -> Option<f64> {
    let other = other?;
    let a = snapshot
        .bubbles
        .iter()
        .find(|actor| actor.entity_id == subject)?;
    let b = snapshot
        .bubbles
        .iter()
        .find(|actor| actor.entity_id == other)?;
    let dx = f64::from(a.world_pos[0]) - f64::from(b.world_pos[0]);
    let dy = f64::from(a.world_pos[1]) - f64::from(b.world_pos[1]);
    Some(dx.hypot(dy))
}

pub(super) fn generate(
    corpus: &AdmissionCorpus,
    case: &AdmissionCase,
    speed: u32,
) -> Result<AuthoritativeTrace, AdmissionError> {
    let started = Instant::now();
    let mut no_cast = if matches!(case.family, MotionFamily::Blink { .. }) {
        Some(setup(corpus, case, speed)?)
    } else {
        None
    };
    let mut setup = setup(corpus, case, speed)?;
    let mut initial = RenderSnapshot::default();
    setup.world.write_render_snapshot(&mut initial);
    let initial_direct_checksum = setup.world.compute_checksum();
    let separation_envelope = if let Some(other) = setup.other {
        let subject_arch = setup.world.catalog.get(case.unit_type)?;
        let other_actor = setup
            .world
            .entities
            .get(&other)
            .ok_or(AdmissionError::Specification("missing setup pair"))?;
        let other_arch = setup.world.catalog.get(other_actor.unit_type_id)?;
        Some(
            f64::from(subject_arch.separation_radius.to_f32())
                + f64::from(other_arch.separation_radius.to_f32()),
        )
    } else {
        None
    };
    let initial_pair_distance = pair_distance(&initial, setup.subject, setup.other);
    let mut trace = AuthoritativeTrace {
        case: case.clone(),
        speed,
        subject: setup.subject,
        other: setup.other,
        initial,
        initial_direct_checksum,
        ticks: Vec::with_capacity(
            usize::try_from(setup.ticks)
                .map_err(|_| AdmissionError::Specification("trace capacity"))?,
        ),
        dimensions: setup.dimensions,
        elapsed_seconds: 0.0,
        failures: Vec::new(),
        coverage: TraceCoverage {
            subject_alive: true,
            target_alive: setup.other.map(|_| true),
            minimum_pair_distance: initial_pair_distance,
            separation_envelope,
            same_layer_near_approach: false,
            pursuit_order_observed: false,
        },
        control_ticks: 0,
        blink_transaction: None,
        blink_tick: None,
        blink_displacement: None,
    };
    for tick in 0..setup.ticks {
        let bundle = TickBundle {
            tick: Tick(setup.world.current_tick),
            commands: match commands(corpus, case, &setup, tick) {
                Ok(commands) => commands,
                Err(error) => {
                    push_failure(
                        &mut trace.failures,
                        WindowFailure {
                            category: FailureCategory::Producer,
                            frame: 0,
                            diagnostic: format!(
                                "command generation aborted before tick {tick}: {error:?}"
                            ),
                        },
                    );
                    break;
                }
            },
            state_checksum: None,
        };
        let casting =
            matches!(case.family, MotionFamily::Blink { .. }) && tick == setup.block_ticks;
        let before = if casting {
            setup.world.entities.get(&setup.subject).cloned()
        } else {
            None
        };
        let mut output = RenderSnapshot::default();
        setup.world.step_into(&bundle, &mut output);
        let movement: Vec<_> = setup.world.drain_movement_outcomes().collect();
        let production: Vec<_> = setup.world.drain_production_outcomes().collect();
        let diagnostics: Vec<_> = setup.world.drain_diagnostics().collect();
        let direct_checksum = setup.world.compute_checksum();
        if direct_checksum != output.checksum
            || !diagnostics.is_empty()
            || movement.iter().any(|receipt| receipt.result.is_err())
            || production.iter().any(|receipt| receipt.result.is_err())
        {
            push_failure(&mut trace.failures, WindowFailure { category: FailureCategory::Producer, frame: 0, diagnostic: format!("tick {tick}: direct={direct_checksum}, published={}; movement={movement:?}; production={production:?}; diagnostics={diagnostics:?}", output.checksum) });
        }
        let observation = (|| -> Result<(), AdmissionError> {
            if let Some(control) = &mut no_cast {
                let mut no_cast_bundle = bundle.clone();
                if casting {
                    no_cast_bundle.commands.retain(|command| {
                        !matches!(command.order, sim_types::UnitOrder::CastAbility { .. })
                    });
                }
                let mut no_cast_output = RenderSnapshot::default();
                control
                    .world
                    .step_into(&no_cast_bundle, &mut no_cast_output);
                trace.control_ticks += 1;
                let no_cast_movement: Vec<_> = control.world.drain_movement_outcomes().collect();
                let no_cast_production: Vec<_> =
                    control.world.drain_production_outcomes().collect();
                let no_cast_diagnostics: Vec<_> = control.world.drain_diagnostics().collect();
                if !casting
                    && (no_cast_output != output
                        || no_cast_movement != movement
                        || no_cast_production != production
                        || no_cast_diagnostics != diagnostics)
                {
                    return Err(AdmissionError::Specification(
                        "generic no-cast control diverged before the cast",
                    ));
                }
                if casting {
                    let before = before.ok_or(AdmissionError::Specification(
                        "Blink actor unavailable before cast",
                    ))?;
                    let after = setup.world.entities.get(&setup.subject).ok_or(
                        AdmissionError::Specification("Blink actor unavailable after cast"),
                    )?;
                    let control_actor = control
                        .world
                        .entities
                        .get(&control.subject)
                        .ok_or(AdmissionError::Specification("no-cast actor unavailable"))?;
                    let arch = setup.world.catalog.get(case.unit_type)?;
                    let ability = &corpus.spec.blink_ability;
                    let destination = bundle
                        .commands
                        .iter()
                        .find_map(|command| match command.order {
                            sim_types::UnitOrder::CastAbility { target_pos, .. } => target_pos,
                            _ => None,
                        })
                        .ok_or(AdmissionError::Specification(
                            "cast command has no actual destination",
                        ))?;
                    let expected_energy = arithmetic(
                        arithmetic(before.energy.checked_sub(ability.energy_cost))?
                            .checked_add(arch.energy_regen_per_tick),
                    )?
                    .min(arch.max_energy);
                    let expected_control_energy =
                        arithmetic(before.energy.checked_add(arch.energy_regen_per_tick))?
                            .min(arch.max_energy);
                    let expected_cooldown = ability.cooldown_ticks.saturating_sub(1);
                    let admitted = after.position == destination
                        && control_actor.position == before.position
                        && after.energy == expected_energy
                        && after.ability_cooldowns[0] == expected_cooldown
                        && control_actor.energy == expected_control_energy
                        && control_actor.ability_cooldowns[0] == 0
                        && no_cast_diagnostics.is_empty();
                    trace.blink_tick = Some(output.tick.0);
                    trace.blink_displacement =
                        Some((after.position - before.position).to_f32_array());
                    trace.blink_transaction = Some(BlinkTransaction {
                        tick,
                        from: before.position,
                        destination,
                        actual_position: after.position,
                        before_energy: before.energy,
                        actual_energy: after.energy,
                        expected_energy,
                        no_cast_output,
                        no_cast_bundle,
                        no_cast_position: control_actor.position,
                        no_cast_energy: control_actor.energy,
                        actual_cooldown: after.ability_cooldowns[0],
                        expected_cooldown,
                        admitted,
                    });
                    if !admitted {
                        push_failure(
                            &mut trace.failures,
                            WindowFailure {
                                category: FailureCategory::Coverage,
                                frame: 0,
                                diagnostic: format!(
                                    "actual Blink/no-cast transaction failed: {:?}",
                                    trace.blink_transaction
                                ),
                            },
                        );
                    }
                }
            }
            if casting {
                no_cast = None;
            }
            trace.coverage.subject_alive &= setup
                .world
                .entities
                .get(&setup.subject)
                .is_some_and(sim_core::Entity::is_alive);
            if let Some(other) = setup.other {
                let alive = setup
                    .world
                    .entities
                    .get(&other)
                    .is_some_and(sim_core::Entity::is_alive);
                if let Some(previous) = &mut trace.coverage.target_alive {
                    *previous &= alive;
                }
                if let Some(distance) = pair_distance(&output, setup.subject, setup.other) {
                    trace.coverage.minimum_pair_distance = Some(
                        trace
                            .coverage
                            .minimum_pair_distance
                            .map_or(distance, |previous| previous.min(distance)),
                    );
                    if case.family == MotionFamily::Collision
                        && initial_pair_distance.is_some_and(|initial| distance < initial)
                        && trace
                            .coverage
                            .separation_envelope
                            .is_some_and(|envelope| distance <= envelope)
                    {
                        trace.coverage.same_layer_near_approach = true;
                    }
                }
            }
            trace.coverage.pursuit_order_observed |= setup
                .world
                .entities
                .get(&setup.subject)
                .is_some_and(|actor| {
                    actor.order_queue.iter().any(|order| {
                        matches!(
                            order,
                            sim_types::UnitOrder::AttackTarget { .. }
                                | sim_types::UnitOrder::AttackMove { .. }
                        )
                    })
                });
            Ok(())
        })();
        trace.ticks.push(TickRecord {
            bundle,
            output,
            direct_checksum,
            movement,
            production,
            diagnostics,
        });
        if let Err(error) = observation {
            push_failure(
                &mut trace.failures,
                WindowFailure {
                    category: FailureCategory::Producer,
                    frame: 0,
                    diagnostic: format!(
                        "actual tick {tick} retained but observation aborted: {error:?}"
                    ),
                },
            );
            break;
        }
    }
    if !trace.coverage.subject_alive
        || trace.coverage.target_alive == Some(false)
        || (case.family == MotionFamily::Collision && !trace.coverage.same_layer_near_approach)
        || (case.family == MotionFamily::Pursuit && !trace.coverage.pursuit_order_observed)
    {
        push_failure(
            &mut trace.failures,
            WindowFailure {
                category: FailureCategory::Coverage,
                frame: 0,
                diagnostic: format!(
                    "actual authoritative capability coverage failed: {:?}",
                    trace.coverage
                ),
            },
        );
    }
    trace.elapsed_seconds = started.elapsed().as_secs_f64();
    Ok(trace)
}

fn nanos(duration: Duration) -> Result<u64, AdmissionError> {
    u64::try_from(duration.as_nanos())
        .map_err(|_| AdmissionError::Specification("duration exceeds recorded u64 nanoseconds"))
}

/// Actual client conversion: snapshot altitude is already authoritative ground/air height.
#[must_use]
pub fn mist_position(instance: &BubbleRenderInstance) -> Vec3 {
    let radius_offset = if matches!(instance.shape, sim_types::ShapeKind::BuildingDome { .. }) {
        0.0
    } else {
        instance.radius
    };
    Vec3::new(
        instance.world_pos[0],
        instance.world_pos[2] + radius_offset,
        instance.world_pos[1],
    )
}

fn failure(error: &ReductionError, frame: u32) -> WindowFailure {
    let category = match error {
        ReductionError::ControlBudgetExceeded { .. } => FailureCategory::ControlBudget,
        ReductionError::SupportCut { .. } | ReductionError::Representation { .. } => {
            FailureCategory::Representation
        }
        ReductionError::Policy(_)
        | ReductionError::Input(_)
        | ReductionError::Sample { .. }
        | ReductionError::Arithmetic(_) => FailureCategory::InvalidHistory,
    };
    WindowFailure {
        category,
        frame,
        diagnostic: format!("{error:?}"),
    }
}

fn push_failure(failures: &mut Vec<WindowFailure>, failure: WindowFailure) {
    if !failures
        .iter()
        .any(|existing| existing.category == failure.category)
    {
        failures.push(failure);
    }
}

fn record_failure(summary: &mut WindowSummary, failure: WindowFailure) {
    if summary.failure.is_none() {
        summary.failure = Some(failure.clone());
    }
    push_failure(&mut summary.failures, failure);
}

fn include_range(range: &mut Option<[f64; 2]>, value: f64) {
    if let Some(range) = range {
        range[0] = range[0].min(value);
        range[1] = range[1].max(value);
    } else {
        *range = Some([value; 2]);
    }
}

fn include_reduction(summary: &mut WindowSummary, reduced: &ReducedWake) {
    summary.maximum_controls = summary.maximum_controls.max(reduced.control_count);
    summary.maximum_error_ratio = summary
        .maximum_error_ratio
        .max(reduced.errors.maximum_ratio());
    include_range(&mut summary.absolute_arc_deficit_range, reduced.arc_deficit);
    if let Some(relative) = reduced.relative_arc_deficit {
        include_range(&mut summary.relative_arc_deficit_range, relative);
    }
    if reduced.reduced_arc > 0.0 {
        summary.moving_histories += 1;
    }
}

struct FrameSchedule {
    rate: u32,
    remainder: u32,
    first: bool,
    phase: i64,
    hitch_pending: bool,
}

impl FrameSchedule {
    fn next(
        &mut self,
        elapsed: Duration,
        corpus: &AdmissionCorpus,
        maximum_delta: Duration,
    ) -> Result<Duration, AdmissionError> {
        if self.hitch_pending && elapsed == corpus.block {
            self.hitch_pending = false;
            if maximum_delta >= corpus.block {
                return Err(AdmissionError::Specification(
                    "hitch crosses an authored block",
                ));
            }
            return maximum_delta
                .checked_mul(2)
                .ok_or(AdmissionError::Specification("raw hitch overflow"));
        }
        let ordinary = if self.first {
            self.first = false;
            let value = 40_000_000_i64
                .checked_add(self.phase)
                .ok_or(AdmissionError::Specification("first phase overflow"))?;
            Duration::from_nanos(
                u64::try_from(value)
                    .map_err(|_| AdmissionError::Specification("nonpositive first phase"))?,
            )
        } else {
            let quotient = 1_000_000_000 / self.rate;
            self.remainder += 1_000_000_000 % self.rate;
            let carry = self.remainder / self.rate;
            self.remainder %= self.rate;
            Duration::from_nanos(u64::from(quotient + carry))
        };
        let next_cut = (1..=4)
            .map(|index| corpus.block * index)
            .find(|cut| *cut > elapsed)
            .ok_or(AdmissionError::Specification("frame beyond horizon"))?;
        Ok(ordinary.min(arithmetic(next_cut.checked_sub(elapsed))?))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PresentationProof {
    pub snapshot: RenderSnapshot,
    pub events: Vec<crate::battle_pipeline::timeline::PresentedCombatEvent>,
}

fn publish_saved(
    frames: &mut BattleFrameState,
    record: &TickRecord,
    direct: &mut Option<Setup>,
    corpus: &AdmissionCorpus,
    case: &AdmissionCase,
) -> Result<(), AdmissionError> {
    let destination = frames.begin_tick()?;
    if let Some(setup) = direct {
        let bundle = TickBundle {
            tick: Tick(setup.world.current_tick),
            commands: commands(corpus, case, setup, setup.world.current_tick)?,
            state_checksum: None,
        };
        if bundle != record.bundle {
            return Err(AdmissionError::Specification(
                "direct and saved tick commands differ",
            ));
        }
        setup.world.step_into(&bundle, destination);
        let movement: Vec<_> = setup.world.drain_movement_outcomes().collect();
        let production: Vec<_> = setup.world.drain_production_outcomes().collect();
        let diagnostics: Vec<_> = setup.world.drain_diagnostics().collect();
        if destination != &record.output
            || movement != record.movement
            || production != record.production
            || diagnostics != record.diagnostics
            || setup.world.compute_checksum() != record.direct_checksum
        {
            return Err(AdmissionError::Specification(
                "direct and saved authoritative outputs/events/checksums/outcomes differ",
            ));
        }
    } else {
        destination.clone_from(&record.output);
    }
    frames.finish_tick()?;
    Ok(())
}

pub(super) fn replay(
    corpus: &AdmissionCorpus,
    trace: &AuthoritativeTrace,
    rate: u32,
    phase: i64,
    direct_world: bool,
    mut proof: Option<&mut Vec<PresentationProof>>,
) -> Result<WindowResult, AdmissionError> {
    let bound = 8_u128 + (corpus.horizon.as_nanos() * u128::from(rate)).div_ceil(1_000_000_000);
    let frame_bound =
        u32::try_from(bound).map_err(|_| AdmissionError::Specification("frame bound"))?;
    let mut summary = WindowSummary {
        case: trace.case.clone(),
        speed: trace.speed,
        rate,
        phase_offset_ns: phase,
        frames: 0,
        attempted_frames: 0,
        completed_horizon: false,
        frame_bound,
        ticks: 0,
        moving_histories: 0,
        maximum_controls: 0,
        maximum_error_ratio: 0.0,
        relative_arc_deficit_range: None,
        absolute_arc_deficit_range: None,
        source_evaluations: 0,
        presented_events: 0,
        displayed_travel: 0.0,
        ground_range: None,
        boosted_frames: 0,
        discarded_ns: 0,
        coverage: Some(trace.coverage.clone()),
        blink_transaction: trace.blink_transaction.clone(),
        failure: trace.failures.first().cloned(),
        failures: trace.failures.clone(),
    };
    let mut app = App::new();
    app.add_plugins(TimePlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.update();
    if !app.world().resource::<Time>().delta().is_zero() {
        return Err(AdmissionError::Specification(
            "initial time priming must be zero",
        ));
    }
    let mut direct = if direct_world {
        Some(setup(corpus, &trace.case, trace.speed)?)
    } else {
        None
    };
    let mut frames = BattleFrameState::default();
    let initial = frames.initialize(SessionGeneration::FIRST)?;
    if let Some(setup) = &mut direct {
        setup.world.write_render_snapshot(initial);
        if initial != &trace.initial
            || setup.world.compute_checksum() != trace.initial_direct_checksum
        {
            return Err(AdmissionError::Specification(
                "initial direct/published provenance differs",
            ));
        }
    } else {
        initial.clone_from(&trace.initial);
    }
    frames.finish_initialization()?;
    let mut clock = BattleTickClock::new(BattleClockPolicy::default())?;
    let mut history = EmitterHistory::new(&corpus.style)?;
    let mut scratch = ReductionScratch::default();
    let mut records = Vec::with_capacity(
        usize::try_from(frame_bound).map_err(|_| AdmissionError::Specification("frame storage"))?,
    );
    let mut schedule = FrameSchedule {
        rate,
        remainder: 0,
        first: true,
        phase,
        hitch_pending: trace.case.family == MotionFamily::Hitch,
    };
    let mut elapsed = Duration::ZERO;
    let mut last_position: Option<Vec3> = None;
    let mut inactive_last = false;
    let mut consumed = 0_usize;
    let observation = (|| -> Result<(), AdmissionError> {
        while elapsed < corpus.horizon {
            let frame_index = summary.frames;
            if summary.frames >= frame_bound {
                return Err(AdmissionError::Specification(
                    "actual frame schedule exceeds declared bound",
                ));
            }
            let maximum_delta = app.world().resource::<Time<Virtual>>().max_delta();
            let requested = schedule.next(elapsed, corpus, maximum_delta)?;
            app.insert_resource(TimeUpdateStrategy::ManualDuration(requested));
            summary.attempted_frames += 1;
            app.update();
            let time = *app.world().resource::<Time>();
            let real_delta = app.world().resource::<Time<Real>>().delta();
            let delta = time.delta();
            let before_elapsed = elapsed;
            elapsed = arithmetic(elapsed.checked_add(delta))?;
            if delta.is_zero() || elapsed > corpus.horizon {
                return Err(AdmissionError::Specification(
                    "actual virtual frame makes no progress or exceeds horizon",
                ));
            }
            let inactive = before_elapsed >= corpus.block && before_elapsed < corpus.block * 2;
            let suspension = inactive
                && matches!(
                    trace.case.family,
                    MotionFamily::BattlePause | MotionFamily::ZeroSpeed | MotionFamily::TabReturn
                );
            let disabled = inactive && trace.case.family == MotionFamily::Disable;
            let departed = inactive && trace.case.family == MotionFamily::TabReturn;
            let lifecycle_reset = inactive_last
                && !inactive
                && matches!(
                    trace.case.family,
                    MotionFamily::Disable | MotionFamily::TabReturn
                );
            if lifecycle_reset {
                history.reset();
                last_position = None;
            }
            let speed = if inactive && trace.case.family == MotionFamily::ZeroSpeed {
                0
            } else {
                trace.speed
            };
            let previously_published_tick = frames.current().tick.0;
            let ticks = if suspension {
                clock.reset_phase();
                frames.hold_exact();
                0
            } else {
                let budget = clock.observe(delta, speed)?;
                for _ in 0..budget.ticks {
                    let record = trace
                        .ticks
                        .get(consumed)
                        .ok_or(AdmissionError::Specification(
                            "authoritative trace exhausted",
                        ))?;
                    publish_saved(&mut frames, record, &mut direct, corpus, &trace.case)?;
                    clock.consume_tick()?;
                    consumed += 1;
                }
                budget.ticks
            };
            frames.present(if suspension { 1.0 } else { clock.fraction() })?;
            let mut events = Vec::new();
            while let Some(event) = frames.pop_presented_event() {
                summary.presented_events += 1;
                if proof.is_some() {
                    events.push(event);
                }
            }
            if let Some(proof) = proof.as_deref_mut() {
                proof.push(PresentationProof {
                    snapshot: frames.presented().clone(),
                    events,
                });
            }
            let instance = frames
                .presented()
                .bubbles
                .iter()
                .find(|value| value.entity_id == trace.subject);
            let mut displacement = 0.0;
            let mut discontinuity_predicate = false;
            let mut position = None;
            let mut retained_previous_samples = 0;
            let mut newly_emitted_samples = 0;
            let blink_transition_tick = trace.blink_tick.filter(|&tick| {
                frames.current().tick.0 >= tick
                    && (previously_published_tick < tick
                        || frames
                            .previous()
                            .is_some_and(|previous| previous.tick.0 < tick))
            });
            if let Some(instance) = instance {
                let current = mist_position(instance);
                displacement = last_position.map_or(0.0, |previous| current.distance(previous));
                discontinuity_predicate =
                    displacement > instance.radius * corpus.style.continuity_radii;
                summary.displayed_travel += f64::from(displacement);
                summary.boosted_frames += u32::from(instance.speed_boosted);
                if let Some(range) = &mut summary.ground_range {
                    range[0] = range[0].min(instance.ground_z);
                    range[1] = range[1].max(instance.ground_z);
                } else {
                    summary.ground_range = Some([instance.ground_z; 2]);
                }
                let previous_history_clock = history.clock();
                if departed || disabled {
                    history.age(delta.as_secs_f64(), &corpus.style)?;
                } else {
                    history.advance(
                        current,
                        instance.radius,
                        delta.as_secs_f64(),
                        &corpus.style,
                    )?;
                }
                for index in 0..history.len() {
                    let sample = history.sample(index).ok_or(AdmissionError::Specification(
                        "chronological history getter failed",
                    ))?;
                    if sample.birth > previous_history_clock {
                        newly_emitted_samples += 1;
                    } else {
                        retained_previous_samples += 1;
                    }
                }
                if blink_transition_tick.is_some()
                    && !discontinuity_predicate
                    && retained_previous_samples > 0
                    && newly_emitted_samples > 0
                {
                    record_failure(&mut summary, WindowFailure {
                    category: FailureCategory::Discontinuity,
                    frame: frame_index,
                    diagnostic: format!("actual cast transition emitted {newly_emitted_samples} new samples while retaining {retained_previous_samples} previous samples; observed displacement={displacement}, strict threshold={}; teleport-safe field admission remains unresolved", instance.radius * corpus.style.continuity_radii),
                });
                }
                last_position = Some(current);
                position = Some(current.to_array());
            } else {
                history.age(delta.as_secs_f64(), &corpus.style)?;
                record_failure(
                    &mut summary,
                    WindowFailure {
                        category: FailureCategory::Coverage,
                        frame: frame_index,
                        diagnostic: "subject missing from actual presented snapshot".into(),
                    },
                );
            }
            let radius = instance.map_or_else(
                || {
                    corpus
                        .catalogs
                        .units
                        .get(trace.case.unit_type)
                        .map(|a| a.collision_radius.to_f32())
                },
                |instance| Ok(instance.radius),
            )?;
            let reduction = reduce_history(&history, radius, &corpus.policy, &mut scratch);
            summary.source_evaluations = arithmetic(
                summary.source_evaluations.checked_add(
                    u64::try_from(scratch.source_evaluations())
                        .map_err(|_| AdmissionError::Specification("source evaluation count"))?,
                ),
            )?;
            let mut rejection = None;
            match &reduction {
                Ok(reduced) => include_reduction(&mut summary, reduced),
                Err(error) => {
                    rejection = Some(format!("{error:?}"));
                    let observed = failure(error, summary.frames);
                    record_failure(&mut summary, observed);
                }
            }
            let samples = (0..history.len())
                .map(|index| {
                    history
                        .sample(index)
                        .map(|sample| SourceSample {
                            position: sample.position.to_array(),
                            birth: sample.birth,
                            path: sample.distance,
                        })
                        .ok_or(AdmissionError::Specification(
                            "chronological history getter failed",
                        ))
                })
                .collect::<Result<Vec<_>, _>>()?;
            records.push(DisplayFrame {
                frame: frame_index,
                requested_ns: nanos(requested)?,
                real_ns: nanos(real_delta)?,
                virtual_ns: nanos(delta)?,
                virtual_elapsed: time.elapsed_secs_f64(),
                speed,
                suspended: suspension,
                ticks,
                current_tick: frames.current().tick.0,
                presented_tick: frames.presented().tick.0,
                debt_ns: nanos(clock.debt())?,
                discarded_ns: nanos(clock.diagnostics().discarded_last_frame)?,
                position,
                displacement,
                discontinuity_predicate,
                blink_transition_tick,
                retained_previous_samples,
                newly_emitted_samples,
                lifecycle_reset,
                history_clock: history.clock(),
                samples,
                reduction: if summary
                    .failures
                    .iter()
                    .any(|failure| failure.frame == summary.frames)
                {
                    Some(format!("result={reduction:?}; scratch={scratch:?}"))
                } else {
                    None
                },
                rejection,
            });
            summary.frames += 1;
            inactive_last = inactive;
        }
        Ok(())
    })();
    summary.completed_horizon = observation.is_ok();
    if let Err(error) = observation {
        let frame = summary.frames;
        record_failure(&mut summary, WindowFailure {
            category: FailureCategory::Producer,
            frame,
            diagnostic: format!("partial display frame retained through last completed record; elapsed={elapsed:?}, current_tick={}, history={history:?}, scratch={scratch:?}; {error:?}", frames.current().tick.0),
        });
    }
    let frame_index = summary.frames;
    summary.ticks = u32::try_from(consumed)
        .map_err(|_| AdmissionError::Specification("consumed tick count"))?;
    summary.discarded_ns = nanos(clock.diagnostics().discarded_total)?;
    if summary.displayed_travel <= 0.0 || summary.moving_histories == 0 {
        record_failure(
            &mut summary,
            WindowFailure {
                category: FailureCategory::Coverage,
                frame: frame_index,
                diagnostic:
                    "no actual moving history; a stationary window cannot satisfy motion coverage"
                        .into(),
            },
        );
    }
    match trace.case.family {
        MotionFamily::Ramp
            if summary
                .ground_range
                .map_or(true, |range| range[1] <= range[0]) =>
        {
            record_failure(
                &mut summary,
                WindowFailure {
                    category: FailureCategory::Coverage,
                    frame: frame_index,
                    diagnostic: "loaded ramp produced no actual ground-height transition".into(),
                },
            );
        }
        MotionFamily::Boost if summary.boosted_frames == 0 => {
            record_failure(
                &mut summary,
                WindowFailure {
                    category: FailureCategory::Coverage,
                    frame: frame_index,
                    diagnostic: "authored overclock was not observed active".into(),
                },
            );
        }
        MotionFamily::StopFade if !history.is_empty() => {
            record_failure(
                &mut summary,
                WindowFailure {
                    category: FailureCategory::Coverage,
                    frame: frame_index,
                    diagnostic: "stop window did not reach actual full history expiry".into(),
                },
            );
        }
        _ => {}
    }
    Ok(WindowResult {
        summary,
        frames: records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn corpus() -> Result<AdmissionCorpus, AdmissionError> {
        AdmissionCorpus::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            Path::new("captures/mist_reduction.ron"),
        )
    }

    #[test]
    fn out_of_range_real_blink_has_identical_stop_only_output() -> TestResult {
        let corpus = corpus()?;
        for mover in [sim_types::MoverType::Ground, sim_types::MoverType::Air] {
            let case = corpus
                .cases
                .iter()
                .find(|case| {
                    case.mover == mover && case.family == MotionFamily::Blink { offset_sign: 1 }
                })
                .ok_or("actual Blink representative absent")?;
            let mut cast = setup(&corpus, case, 1)?;
            let mut control = setup(&corpus, case, 1)?;
            let before = cast
                .world
                .entities
                .get(&cast.subject)
                .ok_or("actual actor absent")?
                .position;
            let distance = arithmetic(
                corpus
                    .spec
                    .blink_ability
                    .cast_range
                    .checked_add(SimScalar::ONE),
            )?;
            let target = add(before, SimVec2::new(distance, SimScalar::ZERO))?;
            let mut bundle = TickBundle {
                tick: Tick(0),
                state_checksum: None,
                commands: vec![
                    command(PlayerId(0), cast.subject, sim_types::UnitOrder::Stop),
                    command(
                        PlayerId(0),
                        cast.subject,
                        sim_types::UnitOrder::CastAbility {
                            ability_index: 0,
                            target_pos: Some(target),
                            target_entity: None,
                        },
                    ),
                ],
            };
            let mut actual = RenderSnapshot::default();
            cast.world.step_into(&bundle, &mut actual);
            bundle.commands.pop();
            let mut no_cast = RenderSnapshot::default();
            control.world.step_into(&bundle, &mut no_cast);
            assert_eq!(actual, no_cast);
            assert_eq!(
                cast.world.compute_checksum(),
                control.world.compute_checksum()
            );
            assert_eq!(
                cast.world
                    .entities
                    .get(&cast.subject)
                    .ok_or("actual actor absent")?
                    .position,
                before
            );
            assert_eq!(cast.world.diagnostics(), control.world.diagnostics());
        }
        Ok(())
    }

    #[test]
    fn truncated_trace_retains_executed_frames_and_explicit_partial_failure() -> TestResult {
        let corpus = corpus()?;
        let case = corpus
            .cases
            .iter()
            .find(|case| {
                case.mover == sim_types::MoverType::Ground && case.family == MotionFamily::Straight
            })
            .ok_or("actual straight representative absent")?;
        let mut trace = generate(&corpus, case, 1)?;
        trace.ticks.truncate(4);
        let result = replay(&corpus, &trace, 144, -1, false, None)?;
        assert_eq!(result.summary.ticks, 4);
        assert!(!result.summary.completed_horizon);
        assert!(result.summary.frames > 0);
        assert_eq!(result.summary.attempted_frames, result.summary.frames + 1);
        assert_eq!(result.frames.len(), usize::try_from(result.summary.frames)?);
        assert!(result.summary.failures.iter().any(|failure| {
            failure.category == FailureCategory::Producer
                && failure.diagnostic.contains("authoritative trace exhausted")
        }));
        Ok(())
    }

    #[test]
    fn snapshot_conversion_preserves_authoritative_height_and_axis_contract() -> TestResult {
        let corpus = corpus()?;
        let case = corpus.cases.first().ok_or("actual corpus empty")?;
        let mut setup = setup(&corpus, case, 1)?;
        let mut snapshot = RenderSnapshot::default();
        setup.world.write_render_snapshot(&mut snapshot);
        let mut actor = snapshot
            .bubbles
            .first()
            .ok_or("actual actor absent")?
            .clone();
        actor.world_pos = [7.0, 11.0, 13.0];
        actor.ground_z = 101.0;
        actor.radius = 2.0;
        actor.shape = sim_types::ShapeKind::UnitSphere;
        assert_eq!(mist_position(&actor), Vec3::new(7.0, 15.0, 11.0));
        actor.shape = sim_types::ShapeKind::BuildingDome {
            footprint_w: 2,
            footprint_h: 3,
        };
        assert_eq!(mist_position(&actor), Vec3::new(7.0, 13.0, 11.0));
        Ok(())
    }

    #[test]
    fn failure_categories_and_signed_arc_extrema_preserve_later_evidence() -> TestResult {
        let corpus = corpus()?;
        let case = corpus
            .cases
            .iter()
            .find(|case| case.family == MotionFamily::Straight)
            .ok_or("actual corpus empty")?;
        let trace = generate(&corpus, case, 1)?;
        let mut result = replay(&corpus, &trace, 30, 1, false, None)?;
        result.summary.failure = None;
        result.summary.failures.clear();
        for (category, frame) in [
            (FailureCategory::Representation, 3),
            (FailureCategory::Discontinuity, 7),
            (FailureCategory::Representation, 9),
        ] {
            record_failure(
                &mut result.summary,
                WindowFailure {
                    category,
                    frame,
                    diagnostic: format!("observed {category:?} at {frame}"),
                },
            );
        }
        assert_eq!(result.summary.failures.len(), 2);
        assert_eq!(
            result
                .summary
                .failure
                .as_ref()
                .ok_or("first failure absent")?
                .frame,
            3
        );
        assert_eq!(result.summary.failures[1].frame, 7);
        let history = EmitterHistory::new(&corpus.style)?;
        let mut reduced = reduce_history(
            &history,
            1.0,
            &corpus.policy,
            &mut ReductionScratch::default(),
        )?;
        reduced.source_arc = 1.0;
        reduced.arc_deficit = -0.0001;
        reduced.relative_arc_deficit = Some(-0.0001);
        result.summary.absolute_arc_deficit_range = None;
        result.summary.relative_arc_deficit_range = None;
        include_reduction(&mut result.summary, &reduced);
        reduced.arc_deficit = 0.2;
        reduced.relative_arc_deficit = Some(0.2);
        include_reduction(&mut result.summary, &reduced);
        assert_eq!(
            result.summary.absolute_arc_deficit_range,
            Some([-0.0001, 0.2])
        );
        assert_eq!(
            result.summary.relative_arc_deficit_range,
            Some([-0.0001, 0.2])
        );
        Ok(())
    }
}
