//! Offline catalog-wide admission of a proposed bounded wake representation.
//!
//! Real simulation traces are generated once per family/speed and replayed through
//! the actual presentation clock/timeline. This tool neither changes gameplay nor
//! certifies a density field. Every declared window is accounted for, including failures.

mod artifacts;
mod trace;

pub use artifacts::AdmissionSummary;
pub use trace::{
    BlinkTransaction, DisplayFrame, FailureCategory, SourceSample, TraceCoverage, WindowFailure,
    WindowSummary,
};

use super::{reduction::ReductionPolicy, MistStyle};
use crate::ResolvedAssetsRoot;
use serde::{Deserialize, Serialize};
use sim_core::catalog::RuntimeCatalogs;
use sim_types::{MoverType, UnitTypeId};
use std::{
    path::{Component, Path, PathBuf},
    time::Duration,
};

/// A declared motion or scheduling experiment; variants describe capabilities, never units.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionFamily {
    /// A sustained ordinary movement command.
    Straight,
    /// Small alternating waypoint directions, with dwell measured in authoritative ticks.
    Zigzag {
        /// Number of ticks between new destinations.
        dwell_ticks: u32,
    },
    /// Reverse the commanded direction halfway through the window.
    Reversal,
    /// Stop after the first lifetime block and observe complete expiry.
    StopFade,
    /// Stop, allow expiry, and resume in the third block.
    StopRestart,
    /// Successive destinations around the authored direction polygon.
    Curve,
    /// Two actual same-layer actors move toward crossing destinations.
    Collision,
    /// Traverse the authored route on a loaded terrain map.
    Ramp,
    /// Pause the battle clock while unscaled visual time continues.
    BattlePause,
    /// Set battle speed to zero, then restore the surrounding speed.
    ZeroSpeed,
    /// Retire the observed owner on departure and create fresh history on return.
    TabReturn,
    /// One raw hitch exercises actual virtual-time clamping and battle debt limits.
    Hitch,
    /// Disable emission while aging, then reset before enabling it again.
    Disable,
    /// Execute real target pursuit followed by an attack-move command.
    Pursuit,
    /// Turn while a real authored overclock capability is active.
    Boost,
    /// A separately authored generic ability fixture around the discontinuity threshold.
    Blink {
        /// -1,0,+1 relative to the actual radius-derived threshold.
        offset_sign: i8,
    },
}

/// Complete versioned verification specification, loaded from the actual runtime root.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionSpec {
    /// Explicit schema version; this implementation accepts one.
    pub format_version: u32,
    /// Seed passed unchanged to each authoritative world.
    pub seed: u64,
    /// Policy values; the age tolerance is supplied as a lifetime divisor below.
    pub support_radii: f64,
    /// Position tolerance in rendered radii.
    pub position_error_radii: f64,
    /// Actor-path-coordinate tolerance in rendered radii.
    pub path_error_radii: f64,
    /// Relative-age tolerance is the loaded lifetime divided by this value.
    pub age_error_lifetime_divisor: f64,
    /// Endpoint-inclusive maximum number of controls.
    pub maximum_controls: usize,
    /// Maximum source ring capacity admitted by the bounded reducer.
    pub maximum_source_samples: usize,
    /// Every declared display cadence, in whole frames per second.
    pub display_rates: Vec<u32>,
    /// Every surrounding positive integral battle speed.
    pub battle_speeds: Vec<u32>,
    /// Signed nanosecond offsets around the actual tick period for the first frame.
    pub first_frame_offsets_ns: Vec<i64>,
    /// Full declared family matrix; unsupported capability cases are explicit records.
    pub families: Vec<MotionFamily>,
    /// Relative loaded-map path, never an editor-owned resource.
    pub ramp_map: PathBuf,
    /// Actual fixed-point loaded-map start and destination coordinates.
    pub ramp_route: [[sim_types::SimScalar; 2]; 2],
    /// Direction polygon for real curved/turning movement commands.
    pub curve_directions: Vec<[sim_types::SimScalar; 2]>,
    /// Lateral component of alternating destinations relative to forward travel.
    pub zigzag_lateral_ratio: sim_types::SimScalar,
    /// Whole-world-unit clearance added to the full actor/travel extent.
    pub map_margin: u32,
    /// Explicit generic ability template; never added to the shipped catalog.
    pub blink_ability: sim_types::AbilitySpec,
    /// Initial admitted maximum energy for the separately authored generic fixture.
    pub blink_max_energy: sim_types::SimScalar,
    /// Generic Blink's near-threshold radius fraction denominator.
    pub blink_offset_divisor: u32,
}

/// Capability-selected family identity, including the exact original catalog ID.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdmissionCase {
    /// Loaded catalog identity, recorded rather than hardcoded.
    pub unit_type: UnitTypeId,
    /// Original loaded slug for human-readable provenance.
    pub slug: String,
    /// Authored mover classification; the engine's actual layer remains authoritative.
    pub mover: MoverType,
    /// Declared experiment.
    pub family: MotionFamily,
}

/// A requested optional family not supported by an authored capability.
#[derive(Clone, Debug, Serialize)]
pub struct UnsupportedFamily {
    /// Original archetype identity.
    pub unit_type: UnitTypeId,
    /// Requested optional experiment.
    pub family: MotionFamily,
    /// Explicit unavailable capability, not an executed successful window.
    pub reason: &'static str,
}

/// Loaded immutable inputs and the derived full execution inventory.
pub struct AdmissionCorpus {
    pub(crate) root: ResolvedAssetsRoot,
    pub(crate) catalogs: RuntimeCatalogs,
    pub(crate) style: MistStyle,
    pub(crate) spec: AdmissionSpec,
    pub(crate) policy: ReductionPolicy,
    pub(crate) cases: Vec<AdmissionCase>,
    pub(crate) unsupported: Vec<UnsupportedFamily>,
    pub(crate) block: Duration,
    pub(crate) horizon: Duration,
    pub(crate) source_spec: PathBuf,
    pub(crate) loaded_input_digests: Vec<(PathBuf, String)>,
}

/// Fatal inability to execute or retain the declared experiment; never a successful fallback.
#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    /// Exact runtime root admission failed.
    #[error(transparent)]
    Assets(#[from] crate::AssetsDirError),
    /// The real external catalogs failed admission.
    #[error(transparent)]
    Catalog(#[from] sim_core::catalog::CatalogLoadError),
    /// The real style or emitted history failed admission.
    #[error(transparent)]
    Mist(#[from] super::MistError),
    /// The proposed reduction policy/input failed admission.
    #[error(transparent)]
    Reduction(#[from] super::reduction::ReductionError),
    /// An exact file operation failed.
    #[error("admission file {path}: {source}")]
    Io {
        /// Exact path being operated on.
        path: PathBuf,
        /// Original operating-system failure.
        #[source]
        source: std::io::Error,
    },
    /// A specification or artifact could not be decoded.
    #[error(transparent)]
    Parse(#[from] ron::error::SpannedError),
    /// Evidence serialization failed before complete publication.
    #[error(transparent)]
    Serialize(#[from] ron::Error),
    /// A declared finite-work or experiment invariant is invalid.
    #[error("invalid reduction admission specification: {0}")]
    Specification(&'static str),
    /// Actual simulation initialization failed.
    #[error(transparent)]
    Simulation(#[from] sim_core::SimCoreError),
    /// Actual map construction failed.
    #[error(transparent)]
    Map(#[from] sim_map::MapError),
    /// An actual authored map failed decoding/admission.
    #[error(transparent)]
    MapFormat(#[from] sim_map::MapFormatError),
    /// Actual presentation scheduling failed.
    #[error(transparent)]
    Clock(#[from] crate::battle_pipeline::clock::BattleClockError),
    /// Actual snapshot publication/interpolation failed.
    #[error(transparent)]
    Timeline(#[from] crate::battle_pipeline::timeline::TimelineError),
    /// The full run completed with explicit unsuccessful windows.
    #[error("reduction admission completed with {failed} unsuccessful windows out of {attempted}; inspect {output}")]
    Rejected {
        /// Every declared window, including unexecuted producer failures.
        attempted: u64,
        /// Windows that did not satisfy their actual coverage/control contract.
        failed: u64,
        /// Complete report directory, never overwritten.
        output: PathBuf,
    },
}

impl AdmissionCorpus {
    /// Loads and validates every input and derives all capability-selected cases.
    /// # Errors
    /// Rejects invalid roots/files, incomplete matrices, policy or checked duration bounds.
    pub fn load(root: &Path, spec: &Path) -> Result<Self, AdmissionError> {
        let root = ResolvedAssetsRoot::from_directory(root)?;
        relative_path(spec)?;
        let path = root.as_path().join(spec);
        let bytes = std::fs::read(&path).map_err(|source| AdmissionError::Io {
            path: path.clone(),
            source,
        })?;
        let spec_value: AdmissionSpec = ron::de::from_bytes(&bytes)?;
        spec_value.validate()?;
        let input_paths = [
            PathBuf::from("catalog/units.ron"),
            PathBuf::from("catalog/structures.ron"),
            PathBuf::from("catalog/upgrades.ron"),
            PathBuf::from("catalog/mist.ron"),
            spec.to_owned(),
            spec_value.ramp_map.clone(),
        ];
        let loaded_input_digests = read_input_digests(root.as_path(), &input_paths)?;
        if loaded_input_digests
            .iter()
            .find(|(path, _)| path == spec)
            .map(|(_, digest)| digest.as_str())
            != Some(blake3::hash(&bytes).to_hex().as_str())
        {
            return Err(AdmissionError::Specification(
                "specification changed during loading",
            ));
        }
        let catalogs = RuntimeCatalogs::load_from_directory(root.as_path().join("catalog"))?;
        let style = MistStyle::load(&root.as_path().join("catalog/mist.ron"))?;
        let policy = ReductionPolicy {
            support_radii: spec_value.support_radii,
            position_error_radii: spec_value.position_error_radii,
            age_error_seconds: f64::from(style.lifetime_seconds)
                / spec_value.age_error_lifetime_divisor,
            path_error_radii: spec_value.path_error_radii,
            maximum_controls: spec_value.maximum_controls,
            maximum_source_samples: spec_value.maximum_source_samples,
        };
        policy.validate()?;
        let period = crate::battle_pipeline::clock::BattleClockPolicy::default().tick_period;
        let lifetime = Duration::try_from_secs_f64(f64::from(style.lifetime_seconds))
            .map_err(|_| AdmissionError::Specification("lifetime duration"))?;
        let blocks = lifetime.as_nanos().div_ceil(period.as_nanos());
        let block = period
            .checked_mul(
                u32::try_from(blocks)
                    .map_err(|_| AdmissionError::Specification("lifetime tick ceiling"))?,
            )
            .ok_or(AdmissionError::Specification("lifetime block overflow"))?;
        let horizon = block.checked_mul(4).ok_or(AdmissionError::Specification(
            "presentation horizon overflow",
        ))?;
        if read_input_digests(root.as_path(), &input_paths)? != loaded_input_digests {
            return Err(AdmissionError::Specification(
                "runtime inputs changed during actual catalog/style loading",
            ));
        }
        let (cases, unsupported) = inventory(&catalogs, &spec_value);
        if cases.is_empty() {
            return Err(AdmissionError::Specification("no mobile catalog actors"));
        }
        Ok(Self {
            root,
            catalogs,
            style,
            spec: spec_value,
            policy,
            cases,
            unsupported,
            block,
            horizon,
            source_spec: spec.to_owned(),
            loaded_input_digests,
        })
    }

    /// All capability-selected executable families, in catalog/spec order.
    #[must_use]
    pub fn cases(&self) -> &[AdmissionCase] {
        &self.cases
    }

    /// Explicit unsupported optional family records, before display expansion.
    #[must_use]
    pub fn unsupported(&self) -> &[UnsupportedFamily] {
        &self.unsupported
    }

    /// Checked full window count; no cadence, speed or phase is implicitly sampled.
    /// # Errors
    /// Rejects an unrepresentable cardinality.
    pub fn window_count(&self) -> Result<u64, AdmissionError> {
        [
            self.cases.len(),
            self.spec.display_rates.len(),
            self.spec.battle_speeds.len(),
            self.spec.first_frame_offsets_ns.len(),
        ]
        .into_iter()
        .try_fold(1_u64, |n, factor| {
            n.checked_mul(
                u64::try_from(factor).map_err(|_| AdmissionError::Specification("window count"))?,
            )
            .ok_or(AdmissionError::Specification("window count overflow"))
        })
    }

    /// Executes one explicitly selected diagnostic window and checks full direct/replay parity.
    /// This focused API is for regression verification, never full-corpus acceptance.
    /// # Errors
    /// Rejects an undeclared case/schedule, actual producer failure or any parity mismatch.
    pub fn verify_window(
        &self,
        case_index: usize,
        speed: u32,
        rate: u32,
        phase: i64,
    ) -> Result<(WindowSummary, Vec<DisplayFrame>), AdmissionError> {
        if !self.spec.battle_speeds.contains(&speed)
            || !self.spec.display_rates.contains(&rate)
            || !self.spec.first_frame_offsets_ns.contains(&phase)
        {
            return Err(AdmissionError::Specification(
                "undeclared focused window schedule",
            ));
        }
        let case = self
            .cases
            .get(case_index)
            .ok_or(AdmissionError::Specification("focused case index"))?;
        let source = trace::generate(self, case, speed)?;
        let mut saved_proof = Vec::new();
        let mut direct_proof = Vec::new();
        let saved = trace::replay(self, &source, rate, phase, false, Some(&mut saved_proof))?;
        let direct = trace::replay(self, &source, rate, phase, true, Some(&mut direct_proof))?;
        if saved_proof != direct_proof || saved.frames != direct.frames {
            return Err(AdmissionError::Specification(
                "direct and saved presented snapshots/events/clock/history differ",
            ));
        }
        Ok((saved.summary, saved.frames))
    }

    /// Executes the complete declared matrix into a new directory, retaining all outcomes.
    /// # Errors
    /// Any unavailable evidence, producer failure or rejected legal window is non-success.
    pub fn run(&self, output: &Path) -> Result<AdmissionSummary, AdmissionError> {
        artifacts::run(self, output)
    }
}

impl AdmissionSpec {
    fn validate(&self) -> Result<(), AdmissionError> {
        if self.format_version != 1
            || self.display_rates != [30, 60, 144]
            || self.battle_speeds != [1, 2, 3, 4]
            || self.first_frame_offsets_ns != [-1, 1]
        {
            return Err(AdmissionError::Specification(
                "version or mandatory rate/speed/phase matrix",
            ));
        }
        if !self.age_error_lifetime_divisor.is_finite()
            || self.age_error_lifetime_divisor <= 0.0
            || self.map_margin < 2
            || self.blink_offset_divisor == 0
            || self.curve_directions.len() < 4
            || self.zigzag_lateral_ratio <= sim_types::SimScalar::ZERO
        {
            return Err(AdmissionError::Specification(
                "motion envelope or age divisor",
            ));
        }
        if self.curve_directions.iter().any(|direction| {
            direction.iter().any(|component| {
                *component < -sim_types::SimScalar::ONE || *component > sim_types::SimScalar::ONE
            }) || *direction == [sim_types::SimScalar::ZERO; 2]
        }) || self
            .curve_directions
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(AdmissionError::Specification(
                "curve directions must be distinct nonzero bounded components",
            ));
        }
        let ability = &self.blink_ability;
        if ability.target_kind != sim_types::AbilityTargetKind::TargetPosition
            || ability.energy_cost <= sim_types::SimScalar::ZERO
            || ability.energy_cost > self.blink_max_energy
            || ability.cooldown_ticks == 0
            || ability.cast_range <= sim_types::SimScalar::ZERO
            || !matches!(ability.effect, sim_types::AbilityEffectSpec::BlinkTeleport { max_range, cooldown_ticks }
                if max_range > sim_types::SimScalar::ZERO && cooldown_ticks > 0)
        {
            return Err(AdmissionError::Specification("generic Blink requires actual position target, positive costs/ranges and sufficient fixture energy"));
        }
        relative_path(&self.ramp_map)?;
        let required = [
            MotionFamily::Straight,
            MotionFamily::Zigzag { dwell_ticks: 1 },
            MotionFamily::Zigzag { dwell_ticks: 2 },
            MotionFamily::Zigzag { dwell_ticks: 3 },
            MotionFamily::Reversal,
            MotionFamily::StopFade,
            MotionFamily::StopRestart,
            MotionFamily::Curve,
            MotionFamily::Collision,
            MotionFamily::Ramp,
            MotionFamily::BattlePause,
            MotionFamily::ZeroSpeed,
            MotionFamily::TabReturn,
            MotionFamily::Hitch,
            MotionFamily::Disable,
            MotionFamily::Pursuit,
            MotionFamily::Boost,
            MotionFamily::Blink { offset_sign: -1 },
            MotionFamily::Blink { offset_sign: 0 },
            MotionFamily::Blink { offset_sign: 1 },
        ];
        if self.families.len() != required.len()
            || required.iter().any(|family| {
                self.families
                    .iter()
                    .filter(|value| *value == family)
                    .count()
                    != 1
            })
        {
            return Err(AdmissionError::Specification(
                "incomplete or duplicated mandatory families",
            ));
        }
        Ok(())
    }
}

fn relative_path(path: &Path) -> Result<(), AdmissionError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(AdmissionError::Specification(
            "input paths must be ordinary relative paths",
        ));
    }
    Ok(())
}

fn inventory(
    catalogs: &RuntimeCatalogs,
    spec: &AdmissionSpec,
) -> (Vec<AdmissionCase>, Vec<UnsupportedFamily>) {
    let mut cases = Vec::new();
    let mut unsupported = Vec::new();
    let mut blink_movers = Vec::new();
    for arch in catalogs
        .units
        .archetypes()
        .filter(|arch| !arch.is_immobile_by_default())
    {
        let blink_representative = !blink_movers.contains(&arch.mover_type);
        if blink_representative {
            blink_movers.push(arch.mover_type);
        }
        for family in &spec.families {
            let unavailable = match family {
                MotionFamily::Boost if arch.overclock.is_none() => Some("no authored overclock"),
                MotionFamily::Pursuit
                    if arch.weapon.is_none()
                        && arch.ground_weapon.is_none()
                        && arch.air_weapon.is_none() =>
                {
                    Some("no authored weapon")
                }
                MotionFamily::Blink { .. } if !blink_representative => continue,
                _ => None,
            };
            if let Some(reason) = unavailable {
                unsupported.push(UnsupportedFamily {
                    unit_type: arch.unit_type_id,
                    family: family.clone(),
                    reason,
                });
            } else {
                cases.push(AdmissionCase {
                    unit_type: arch.unit_type_id,
                    slug: arch.slug.clone(),
                    mover: arch.mover_type,
                    family: family.clone(),
                });
            }
        }
    }
    (cases, unsupported)
}

fn read_input_digests(
    root: &Path,
    paths: &[PathBuf],
) -> Result<Vec<(PathBuf, String)>, AdmissionError> {
    paths
        .iter()
        .map(|relative| {
            let path = root.join(relative);
            let bytes =
                std::fs::read(&path).map_err(|source| AdmissionError::Io { path, source })?;
            Ok((relative.clone(), blake3::hash(&bytes).to_hex().to_string()))
        })
        .collect()
}
