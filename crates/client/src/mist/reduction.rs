//! Checked, allocation-free approximation of retained visual movement histories.
//!
//! This CPU admission algorithm preserves chronological attributes and measures its
//! returned floating-point controls against the clipped source. It does not change
//! live emission or render a field; control accuracy does not establish optical mass.

use super::emission::{EmitterHistory, TrailSample};
use bevy::math::{DVec3, Vec3};
use thiserror::Error;

/// Largest source history admitted by the proposed bounded representation.
pub const MAX_SOURCE_SAMPLES: usize = 128;
/// Endpoint-inclusive control budget; there are at most sixteen segment slots.
pub const MAX_CONTROLS: usize = 17;

/// Explicit approximation policy; distances are relative to the rendered radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReductionPolicy {
    /// Maximum retained emitted-centerline arc, in rendered radii.
    pub support_radii: f64,
    /// Maximum spatial error, in rendered radii.
    pub position_error_radii: f64,
    /// Maximum relative-age interpolation error, in presentation seconds.
    pub age_error_seconds: f64,
    /// Maximum actor-path-coordinate error, in rendered radii.
    pub path_error_radii: f64,
    /// Maximum endpoint-inclusive number of returned controls.
    pub maximum_controls: usize,
    /// Maximum original ring capacity accepted by this policy.
    pub maximum_source_samples: usize,
}

impl ReductionPolicy {
    /// Constructs the reviewed initial policy for an actual loaded lifetime in seconds.
    /// # Errors
    /// Rejects a nonpositive/nonfinite lifetime or unrepresentable derived tolerance.
    pub fn for_lifetime(lifetime_seconds: f32) -> Result<Self, ReductionError> {
        let policy = Self {
            support_radii: 8.0,
            position_error_radii: 0.05,
            age_error_seconds: f64::from(lifetime_seconds) / 64.0,
            path_error_radii: 0.05,
            maximum_controls: MAX_CONTROLS,
            maximum_source_samples: MAX_SOURCE_SAMPLES,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Checks numerical policy and fixed-storage limits without modifying scratch.
    /// # Errors
    /// Returns the name of the invalid policy field.
    pub fn validate(&self) -> Result<(), ReductionError> {
        for (field, value) in [
            ("support_radii", self.support_radii),
            ("position_error_radii", self.position_error_radii),
            ("age_error_seconds", self.age_error_seconds),
            ("path_error_radii", self.path_error_radii),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(ReductionError::Policy(field));
            }
        }
        if !(2..=MAX_CONTROLS).contains(&self.maximum_controls) {
            return Err(ReductionError::Policy("maximum_controls"));
        }
        if !(2..=MAX_SOURCE_SAMPLES).contains(&self.maximum_source_samples) {
            return Err(ReductionError::Policy("maximum_source_samples"));
        }
        Ok(())
    }
}

/// Immutable chronological source for malformed-input tests and diagnostic replay.
pub struct ReductionInput<'a> {
    /// Emitter-local presentation clock in seconds, shared by every sample.
    pub clock: f64,
    /// Retained samples, oldest first; their positions are world-space coordinates.
    pub samples: &'a [TrailSample],
    /// Declared original ring capacity, including capacity not currently occupied.
    pub capacity: usize,
}

/// Original sample or interpolated support boundary identified without ambiguity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SourceLocation {
    /// Original chronological index at the beginning of the source edge.
    pub before: usize,
    /// Original chronological index at the end; equal to `before` for an original point.
    pub after: usize,
    /// Edge fraction in zero through one; zero for an original point.
    pub fraction: f64,
}

/// Returned GPU-representable attributes plus their original temporal provenance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReducedControl {
    /// Rounded world-space position, with no emitter transform applied.
    pub position: Vec3,
    /// Rounded nonnegative age in seconds relative to [`ReducedWake::clock`].
    pub age: f32,
    /// Rounded original cumulative actor-center path distance in world units.
    pub path_distance: f32,
    /// Original/interpolated birth in emitter-local seconds; diagnostic provenance only.
    pub birth: f64,
    /// Original sample identity or actual support-cut interpolation.
    pub source: SourceLocation,
}

/// Whether a segment has spatial measure; zero measure never supplies a density puff.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SegmentKind {
    /// Coincident spatial endpoints with retained chronological attributes.
    #[default]
    ZeroMeasure,
    /// A positive representable chord with finite axis, length and reciprocal.
    Spatial,
}

/// Bounded segment geometry and source/reduced arc diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReducedSegment {
    /// Spatial or zero-measure classification.
    pub kind: SegmentKind,
    /// Rounded world-space unit-axis candidate; zero for zero measure.
    pub axis: Vec3,
    /// Rounded chord length in world units; zero for zero measure.
    pub length: f32,
    /// Actual `f32` reciprocal of `length`; zero for zero measure.
    pub inverse_length: f32,
    /// Original clipped emitted-centerline arc covered by this segment.
    pub source_arc: f64,
    /// Length between the returned rounded endpoints, evaluated in `f64`.
    pub reduced_arc: f64,
}

/// Absolute errors and their individual ratios to the declared tolerances.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReductionErrors {
    /// Spatial deviation in world units.
    pub position: f64,
    /// Relative-age deviation in presentation seconds.
    pub age: f64,
    /// Original actor-path-coordinate deviation in world units.
    pub path: f64,
    /// Spatial error divided by the allowed spatial error.
    pub position_ratio: f64,
    /// Age error divided by the allowed age error.
    pub age_ratio: f64,
    /// Path-coordinate error divided by its allowed error.
    pub path_ratio: f64,
}

impl ReductionErrors {
    /// Greatest component ratio; a value at most one meets all three bounds.
    #[must_use]
    pub fn maximum_ratio(self) -> f64 {
        self.position_ratio.max(self.age_ratio).max(self.path_ratio)
    }

    fn include(&mut self, other: Self) {
        self.position = self.position.max(other.position);
        self.age = self.age.max(other.age);
        self.path = self.path.max(other.path);
        self.position_ratio = self.position_ratio.max(other.position_ratio);
        self.age_ratio = self.age_ratio.max(other.age_ratio);
        self.path_ratio = self.path_ratio.max(other.path_ratio);
    }
}

/// Fixed-size reduction result. Only prefixes identified by counts are meaningful.
#[derive(Clone, Debug, PartialEq)]
pub struct ReducedWake {
    /// Immutable age reference in emitter-local presentation seconds.
    pub clock: f64,
    /// Original retained count, including exact duplicates.
    pub original_count: usize,
    /// Count after all-field deduplication and support clipping.
    pub source_count: usize,
    /// Number of meaningful returned controls.
    pub control_count: usize,
    /// Returned attributes; unused entries are zeroed.
    pub controls: [ReducedControl; MAX_CONTROLS],
    /// Segment records; exactly `control_count.saturating_sub(1)` are meaningful.
    pub segments: [ReducedSegment; MAX_CONTROLS - 1],
    /// Emitted arc of the original retained history before support clipping.
    pub original_arc: f64,
    /// Emitted arc after clipping, used for source-support/startup diagnostics.
    pub source_arc: f64,
    /// Sum of the chords between returned rounded controls.
    pub reduced_arc: f64,
    /// Signed source-minus-reduced arc; rounding may create a tiny negative value.
    pub arc_deficit: f64,
    /// Relative deficit when source arc is positive; absent for zero arc.
    pub relative_arc_deficit: Option<f64>,
    /// True when the source was clipped at its oldest end.
    pub clipped: bool,
    /// Largest actual returned-control approximation errors across the source.
    pub errors: ReductionErrors,
    /// Number of scored source records, including endpoint checks in each pass.
    pub source_evaluations: usize,
}

/// Typed refusal of unsupported input, representation or bounded control capacity.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ReductionError {
    /// The declared approximation policy cannot be evaluated or stored.
    #[error("invalid reduction policy: {0}")]
    Policy(&'static str),
    /// Clock, radius or ring capacity fails admission, including empty histories.
    #[error("invalid reduction input: {0}")]
    Input(&'static str),
    /// One source record is nonfinite, negative, unordered or outside its clock.
    #[error("invalid reduction sample {index}: {field}")]
    Sample {
        /// Original chronological sample index.
        index: usize,
        /// Invalid attribute or ordering condition.
        field: &'static str,
    },
    /// Checked intermediate arithmetic cannot be represented.
    #[error("unrepresentable reduction arithmetic: {0}")]
    Arithmetic(&'static str),
    /// The unchanged ideal cut reconstructs outside the declared support due to arithmetic.
    #[error("support cut reconstructs {reconstructed} world units beyond declared {support}")]
    SupportCut {
        /// Declared maximum emitted support in world units.
        support: f64,
        /// Actual recomputed clipped reference arc in world units.
        reconstructed: f64,
        /// Ideal cut's chronological edge and direct fraction.
        boundary: SourceLocation,
        /// Ideal interpolated world-space position before output rounding.
        position: [f64; 3],
        /// Ideal interpolated birth in emitter-local seconds.
        birth: f64,
        /// Ideal interpolated actor-path coordinate in world units.
        path: f64,
    },
    /// An interval or endpoint cannot be represented by the returned float format.
    #[error("unrepresentable reduction interval {start}..{end}: {field}")]
    Representation {
        /// First chronological clipped-source index.
        start: usize,
        /// Last chronological clipped-source index.
        end: usize,
        /// Failed numerical/attribute invariant.
        field: &'static str,
    },
    /// Meeting the exact limits would require another control beyond the policy.
    #[error("reduction needs more than {selected_count} controls for {source_count} samples at {start}..{end}: {errors:?}")]
    ControlBudgetExceeded {
        /// Clipped source sample count.
        source_count: usize,
        /// Actual selected control count when another split is required.
        selected_count: usize,
        /// Beginning of the violating source interval.
        start: usize,
        /// End of the violating source interval.
        end: usize,
        /// Measured component errors, without a relaxed acceptance epsilon.
        errors: ReductionErrors,
    },
}

#[derive(Clone, Copy, Debug, Default)]
struct ReferenceSample {
    position: DVec3,
    birth: f64,
    path: f64,
    source: SourceLocation,
    last_duplicate: usize,
}

// PERFORMANCE RATIONALE:
// Why: The offline corpus evaluates millions of histories, and future callers need
// the same bounded representation without heap churn in a presentation hot loop.
// What: Reuse fixed source/arc/selection arrays and scan only each candidate's own
// source interval on each split pass (at most sixteen source sweeps plus endpoints).
// Trade-off: A fixed 128-source/17-control storage budget and explicit typed refusal
// instead of allocating more controls or weakening an approximation bound.
/// Reusable fixed-size workspace. Construction and reduction perform no heap allocation.
#[derive(Debug)]
pub struct ReductionScratch {
    source: [ReferenceSample; MAX_SOURCE_SAMPLES],
    arc: [f64; MAX_SOURCE_SAMPLES],
    selected: [usize; MAX_CONTROLS],
    count: usize,
    evaluations: usize,
}

impl Default for ReductionScratch {
    fn default() -> Self {
        Self {
            source: [ReferenceSample::default(); MAX_SOURCE_SAMPLES],
            arc: [0.0; MAX_SOURCE_SAMPLES],
            selected: [0; MAX_CONTROLS],
            count: 0,
            evaluations: 0,
        }
    }
}

impl ReductionScratch {
    /// Actual scored source records in the most recent call, including rejected passes.
    #[must_use]
    pub fn source_evaluations(&self) -> usize {
        self.evaluations
    }

    fn reset_call(&mut self) {
        self.count = 0;
        self.evaluations = 0;
    }
}

/// Reduces an actual retained history through its public chronological getters.
/// # Errors
/// Rejects invalid producer data, unrepresentable rounded controls or excess controls.
pub fn reduce_history(
    history: &EmitterHistory,
    radius: f32,
    policy: &ReductionPolicy,
    scratch: &mut ReductionScratch,
) -> Result<ReducedWake, ReductionError> {
    scratch.reset_call();
    prepare(
        history.clock(),
        radius,
        history.len(),
        history.capacity(),
        policy,
    )?;
    load_source(history.len(), history.clock(), scratch, |index| {
        history.sample(index)
    })?;
    reduce_loaded(history.clock(), radius, policy, scratch, history.len())
}

/// Reduces checked immutable samples using the same algorithm as the live-history adapter.
/// # Errors
/// Rejects malformed chronology, capacity, numbers, representation or control budget.
pub fn reduce_samples(
    input: ReductionInput<'_>,
    radius: f32,
    policy: &ReductionPolicy,
    scratch: &mut ReductionScratch,
) -> Result<ReducedWake, ReductionError> {
    scratch.reset_call();
    prepare(
        input.clock,
        radius,
        input.samples.len(),
        input.capacity,
        policy,
    )?;
    load_source(input.samples.len(), input.clock, scratch, |index| {
        input.samples.get(index).copied()
    })?;
    reduce_loaded(input.clock, radius, policy, scratch, input.samples.len())
}

fn prepare(
    clock: f64,
    radius: f32,
    count: usize,
    capacity: usize,
    policy: &ReductionPolicy,
) -> Result<(), ReductionError> {
    policy.validate()?;
    if !clock.is_finite() || clock < 0.0 {
        return Err(ReductionError::Input("clock"));
    }
    if !radius.is_finite() || radius <= 0.0 {
        return Err(ReductionError::Input("radius"));
    }
    if capacity == 0 || capacity > policy.maximum_source_samples || count > capacity {
        return Err(ReductionError::Input("source capacity"));
    }
    for value in [
        policy.support_radii,
        policy.position_error_radii,
        policy.path_error_radii,
    ] {
        let scaled = value * f64::from(radius);
        if !scaled.is_finite() || scaled <= 0.0 {
            return Err(ReductionError::Arithmetic("radius-scaled policy"));
        }
    }
    Ok(())
}

#[allow(clippy::float_cmp)] // Only exact attribute duplicates may be removed.
fn load_source(
    count: usize,
    clock: f64,
    scratch: &mut ReductionScratch,
    mut get: impl FnMut(usize) -> Option<TrailSample>,
) -> Result<(), ReductionError> {
    scratch.count = 0;
    let mut previous: Option<TrailSample> = None;
    for index in 0..count {
        let sample = get(index).ok_or(ReductionError::Sample {
            index,
            field: "missing chronological sample",
        })?;
        let failure = if !sample.position.is_finite() {
            Some("position")
        } else if !sample.birth.is_finite() || sample.birth < 0.0 || sample.birth > clock {
            Some("birth")
        } else if !sample.distance.is_finite() || sample.distance < 0.0 {
            Some("path distance")
        } else if previous.is_some_and(|p| sample.birth < p.birth) {
            Some("decreasing birth")
        } else if previous.is_some_and(|p| sample.distance < p.distance) {
            Some("decreasing path")
        } else {
            None
        };
        if let Some(field) = failure {
            return Err(ReductionError::Sample { index, field });
        }
        let duplicate = previous.is_some_and(|p| {
            p.position == sample.position
                && p.birth == sample.birth
                && p.distance == sample.distance
        });
        previous = Some(sample);
        if duplicate {
            scratch.source[scratch.count - 1].last_duplicate = index;
            continue;
        }
        scratch.source[scratch.count] = ReferenceSample {
            position: sample.position.as_dvec3(),
            birth: sample.birth,
            path: f64::from(sample.distance),
            source: SourceLocation {
                before: index,
                after: index,
                fraction: 0.0,
            },
            last_duplicate: index,
        };
        scratch.count += 1;
    }
    Ok(())
}

fn arcs(scratch: &mut ReductionScratch) -> Result<f64, ReductionError> {
    scratch.arc[0] = 0.0;
    for index in 1..scratch.count {
        let length = (scratch.source[index].position - scratch.source[index - 1].position).length();
        let total = scratch.arc[index - 1] + length;
        if !total.is_finite() || (length > 0.0 && total <= scratch.arc[index - 1]) {
            return Err(ReductionError::Arithmetic("source arc"));
        }
        scratch.arc[index] = total;
    }
    Ok(scratch
        .count
        .checked_sub(1)
        .map_or(0.0, |last| scratch.arc[last]))
}

fn interpolate(a: ReferenceSample, b: ReferenceSample, fraction: f64) -> ReferenceSample {
    ReferenceSample {
        position: a.position.lerp(b.position, fraction),
        birth: a.birth + (b.birth - a.birth) * fraction,
        path: a.path + (b.path - a.path) * fraction,
        source: SourceLocation {
            before: a.last_duplicate,
            after: b.source.before,
            fraction,
        },
        last_duplicate: a.last_duplicate,
    }
}

#[allow(clippy::float_cmp)] // An exact cut preserves original chronological provenance.
fn clip(scratch: &mut ReductionScratch, support: f64) -> Result<(), ReductionError> {
    let last = scratch.count - 1;
    let cut = scratch.arc[last] - support;
    if cut >= scratch.arc[last] || cut <= 0.0 {
        return Err(ReductionError::Representation {
            start: 0,
            end: last,
            field: "positive support cut progress",
        });
    }
    let after = (1..scratch.count)
        .find(|&i| scratch.arc[i] >= cut)
        .ok_or(ReductionError::Arithmetic("support cut interval"))?;
    if scratch.arc[after] == cut {
        scratch.source.copy_within(after..scratch.count, 0);
        scratch.count -= after;
        return Ok(());
    }
    let a = scratch.source[after - 1];
    let b = scratch.source[after];
    let span = scratch.arc[after] - scratch.arc[after - 1];
    let fraction = (cut - scratch.arc[after - 1]) / span;
    if !fraction.is_finite() || fraction <= 0.0 || fraction >= 1.0 {
        return Err(ReductionError::Representation {
            start: after - 1,
            end: after,
            field: "interior support cut fraction",
        });
    }
    let boundary = interpolate(a, b, fraction);
    scratch.source.copy_within(after..scratch.count, 1);
    scratch.source[0] = boundary;
    scratch.count = scratch.count - after + 1;
    Ok(())
}

#[allow(clippy::cast_possible_truncation)] // Admission immediately checks the actual rounded representation.
fn control(
    source: ReferenceSample,
    clock: f64,
    index: usize,
) -> Result<ReducedControl, ReductionError> {
    let value = ReducedControl {
        position: source.position.as_vec3(),
        age: (clock - source.birth) as f32,
        path_distance: source.path as f32,
        birth: source.birth,
        source: source.source,
    };
    if !value.position.is_finite()
        || !value.age.is_finite()
        || value.age < 0.0
        || !value.path_distance.is_finite()
    {
        return Err(ReductionError::Representation {
            start: index,
            end: index,
            field: "rounded attributes",
        });
    }
    Ok(value)
}

fn parameter(
    scratch: &ReductionScratch,
    start: usize,
    end: usize,
    index: usize,
) -> Result<f64, ReductionError> {
    let arc = scratch.arc[end] - scratch.arc[start];
    let a = scratch.source[start];
    let b = scratch.source[end];
    let sample = scratch.source[index];
    let fraction = if arc > 0.0 {
        (scratch.arc[index] - scratch.arc[start]) / arc
    } else if b.birth > a.birth {
        (sample.birth - a.birth) / (b.birth - a.birth)
    } else if b.path > a.path {
        (sample.path - a.path) / (b.path - a.path)
    } else {
        return Err(ReductionError::Representation {
            start,
            end,
            field: "duplicate zero-arc interval",
        });
    };
    if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
        return Err(ReductionError::Arithmetic("candidate interpolation"));
    }
    Ok(fraction)
}

#[allow(clippy::cast_possible_truncation)] // Both actual f32 length and reciprocal are checked below.
fn segment(a: ReducedControl, b: ReducedControl, source_arc: f64) -> Option<ReducedSegment> {
    let delta = b.position.as_dvec3() - a.position.as_dvec3();
    let chord = delta.length();
    if source_arc == 0.0 && chord == 0.0 {
        return Some(ReducedSegment::default());
    }
    let length = chord as f32;
    let inverse = 1.0_f32 / length;
    if source_arc <= 0.0
        || !length.is_finite()
        || length <= 0.0
        || !inverse.is_finite()
        || inverse <= 0.0
    {
        return None;
    }
    let axis = (delta / chord).as_vec3();
    axis.is_finite().then_some(ReducedSegment {
        kind: SegmentKind::Spatial,
        axis,
        length,
        inverse_length: inverse,
        source_arc,
        reduced_arc: chord,
    })
}

#[derive(Clone, Copy)]
struct Split {
    interval: usize,
    index: usize,
    errors: ReductionErrors,
    forced: bool,
}

fn candidate_errors(
    sample: ReferenceSample,
    expected_age: f64,
    a: ReducedControl,
    b: ReducedControl,
    u: f64,
    radius: f64,
    policy: &ReductionPolicy,
) -> Result<ReductionErrors, ReductionError> {
    let position =
        (sample.position - a.position.as_dvec3().lerp(b.position.as_dvec3(), u)).length();
    let age = (expected_age - (f64::from(a.age) + (f64::from(b.age) - f64::from(a.age)) * u)).abs();
    let path = (sample.path
        - (f64::from(a.path_distance)
            + (f64::from(b.path_distance) - f64::from(a.path_distance)) * u))
        .abs();
    let errors = ReductionErrors {
        position,
        age,
        path,
        position_ratio: position / (radius * policy.position_error_radii),
        age_ratio: age / policy.age_error_seconds,
        path_ratio: path / (radius * policy.path_error_radii),
    };
    if [
        errors.position,
        errors.age,
        errors.path,
        errors.position_ratio,
        errors.age_ratio,
        errors.path_ratio,
    ]
    .iter()
    .any(|value| !value.is_finite())
    {
        return Err(ReductionError::Arithmetic("normalized candidate error"));
    }
    Ok(errors)
}

#[allow(clippy::float_cmp)] // Equal numerical scores use the declared chronological tie rule.
fn reduce_loaded(
    clock: f64,
    radius: f32,
    policy: &ReductionPolicy,
    scratch: &mut ReductionScratch,
    original_count: usize,
) -> Result<ReducedWake, ReductionError> {
    let original_arc = arcs(scratch)?;
    let clipped = original_arc > policy.support_radii * f64::from(radius);
    if clipped {
        clip(scratch, policy.support_radii * f64::from(radius))?;
    }
    let source_arc = arcs(scratch)?;
    let support = policy.support_radii * f64::from(radius);
    if clipped && source_arc > support {
        let boundary = scratch.source[0];
        return Err(ReductionError::SupportCut {
            support,
            reconstructed: source_arc,
            boundary: boundary.source,
            position: boundary.position.to_array(),
            birth: boundary.birth,
            path: boundary.path,
        });
    }
    let mut out = ReducedWake {
        clock,
        original_count,
        source_count: scratch.count,
        control_count: scratch.count.min(2),
        controls: [ReducedControl::default(); MAX_CONTROLS],
        segments: [ReducedSegment::default(); MAX_CONTROLS - 1],
        original_arc,
        source_arc,
        reduced_arc: 0.0,
        arc_deficit: 0.0,
        relative_arc_deficit: None,
        clipped,
        errors: ReductionErrors::default(),
        source_evaluations: 0,
    };
    if scratch.count == 0 {
        return Ok(out);
    }
    scratch.selected[0] = 0;
    if scratch.count == 1 {
        out.controls[0] = control(scratch.source[0], clock, 0)?;
        out.source_evaluations = 1;
        scratch.evaluations = 1;
        out.errors = candidate_errors(
            scratch.source[0],
            clock - scratch.source[0].birth,
            out.controls[0],
            out.controls[0],
            0.0,
            f64::from(radius),
            policy,
        )?;
        if out.errors.maximum_ratio() > 1.0 {
            return Err(ReductionError::Representation {
                start: 0,
                end: 0,
                field: "rounded endpoint tolerance",
            });
        }
        return Ok(out);
    }
    scratch.selected[1] = scratch.count - 1;
    loop {
        let mut next: Option<Split> = None;
        out.errors = ReductionErrors::default();
        for interval in 0..out.control_count - 1 {
            let start = scratch.selected[interval];
            let end = scratch.selected[interval + 1];
            let a = control(scratch.source[start], clock, start)?;
            let b = control(scratch.source[end], clock, end)?;
            out.controls[interval] = a;
            out.controls[interval + 1] = b;
            let geometry = segment(a, b, scratch.arc[end] - scratch.arc[start]);
            let mut interval_errors = ReductionErrors::default();
            let mut forced_index = None;
            let mut displacement = -1.0_f64;
            for index in start..=end {
                out.source_evaluations += 1;
                scratch.evaluations += 1;
                let u = parameter(scratch, start, end, index)?;
                let error = candidate_errors(
                    scratch.source[index],
                    clock - scratch.source[index].birth,
                    a,
                    b,
                    u,
                    f64::from(radius),
                    policy,
                )?;
                out.errors.include(error);
                interval_errors.include(error);
                if error.maximum_ratio() > 1.0 {
                    if index == start || index == end {
                        return Err(ReductionError::Representation {
                            start: index,
                            end: index,
                            field: "rounded endpoint tolerance",
                        });
                    }
                    let proposed = Split {
                        interval,
                        index,
                        errors: error,
                        forced: false,
                    };
                    if next.is_none_or(|old| {
                        !old.forced
                            && (error.maximum_ratio() > old.errors.maximum_ratio()
                                || (error.maximum_ratio() == old.errors.maximum_ratio()
                                    && index < old.index))
                    }) {
                        next = Some(proposed);
                    }
                }
                if index > start && index < end {
                    let distance = (scratch.source[index].position
                        - scratch.source[start].position)
                        .length_squared();
                    if distance > displacement {
                        displacement = distance;
                        forced_index = Some(index);
                    }
                }
            }
            if let Some(geometry) = geometry {
                out.segments[interval] = geometry;
            } else {
                let index = forced_index.ok_or(ReductionError::Representation {
                    start,
                    end,
                    field: "positive representable chord and reciprocal",
                })?;
                if next.is_none_or(|old| !old.forced || index < old.index) {
                    next = Some(Split {
                        interval,
                        index,
                        errors: interval_errors,
                        forced: true,
                    });
                }
            }
        }
        let Some(split) = next else {
            break;
        };
        if out.control_count == policy.maximum_controls {
            return Err(ReductionError::ControlBudgetExceeded {
                source_count: scratch.count,
                selected_count: out.control_count,
                start: scratch.selected[split.interval],
                end: scratch.selected[split.interval + 1],
                errors: split.errors,
            });
        }
        scratch
            .selected
            .copy_within(split.interval + 1..out.control_count, split.interval + 2);
        scratch.selected[split.interval + 1] = split.index;
        out.control_count += 1;
    }
    out.reduced_arc = out.segments[..out.control_count - 1]
        .iter()
        .map(|segment| segment.reduced_arc)
        .sum();
    out.arc_deficit = out.source_arc - out.reduced_arc;
    out.relative_arc_deficit = (out.source_arc > 0.0).then(|| out.arc_deficit / out.source_arc);
    Ok(out)
}

#[cfg(test)]
mod tests;
