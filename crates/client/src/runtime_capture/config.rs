//! Validated declarative checkpoints and typed failures for production captures.
//!
//! Scripts control real camera/tab/reset and typed transport APIs with deterministic advancement;
//! they cannot create units, edit terrain, inject damage or replace rendered assets.

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// CLI options for the actual production-game capture runner.
#[derive(Parser, Debug)]
pub struct CaptureOptions {
    /// External scenario relative to the selected assets root.
    #[arg(long, default_value = crate::runtime_battle::DEFAULT_BATTLE_SCENARIO)]
    pub scenario: PathBuf,
    /// Versioned capture script relative to the selected assets root.
    #[arg(long, default_value = "captures/default_battle.ron")]
    pub script: PathBuf,
    /// New output directory; existing directories are rejected to prevent stale evidence.
    #[arg(long)]
    pub out: PathBuf,
}

/// Failures are surfaced as nonzero exit and an incomplete manifest where possible.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// Opt-in render-order evidence failed its bounded real-render contract.
    #[error(transparent)]
    ShellAudit(#[from] crate::shell_order_audit::AuditError),
    /// Production startup failed before capture began.
    #[error(transparent)]
    Startup(#[from] crate::ClientStartupError),
    /// A script or artifact could not be read/written.
    #[error("capture I/O at {path}: {source}")]
    Io {
        /// Exact file or directory involved.
        path: PathBuf,
        /// Filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// Capture script syntax was invalid.
    #[error("capture script {path}: {source}")]
    Parse {
        /// Selected script path.
        path: PathBuf,
        /// RON parse evidence.
        #[source]
        source: ron::error::SpannedError,
    },
    /// Exact authored unit metadata could not be parsed or validated.
    #[error("capture media metadata {path}: {source}")]
    MediaMetadata {
        /// Actual archetype metadata file.
        path: PathBuf,
        /// Typed baker metadata failure.
        #[source]
        source: asset_baker::BakerError,
    },
    /// Exact authored PNG could not be opened or decoded.
    #[error("capture media image {path}: {source}")]
    MediaImage {
        /// Actual archetype PNG file.
        path: PathBuf,
        /// Typed image loader failure.
        #[source]
        source: image::ImageError,
    },
    /// Loaded media/binding does not equal the authored archetype's data.
    #[error("capture media mismatch at {path}: {reason}")]
    MediaMismatch {
        /// Source file whose identity or bytes did not match.
        path: PathBuf,
        /// Specific violated representation invariant.
        reason: &'static str,
    },
    /// A script violates the supported control or timing contract.
    #[error("invalid capture script: {0}")]
    InvalidScript(String),
    /// A production state/action/pipeline failed its acceptance condition.
    #[error("capture verification failed: {0}")]
    Verification(String),
    /// GPU pixel conversion, validation or image encoding failed.
    #[error("capture readback failed: {0}")]
    Readback(String),
}

/// Required lifecycle observations; no synthetic-world action exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum Goal {
    /// Real tick-zero map and army overview.
    Overview,
    /// A moving ground combatant enters an authored ramp.
    RampEntry,
    /// The same combatant exits onto the other terrain elevation.
    RampExit,
    /// Hostile damage with an actual combat visual event.
    Combat,
    /// Normal Visual Lab tab restoration.
    Lab,
    /// Normal battle tab restoration, retaining paused state/camera.
    Return,
    /// Normal reset returns to the initial state and one owned scene.
    Reset,
    /// Actual neutral observation tower, with its loaded billboard and neutral style.
    NeutralWatchtower,
    /// Normal authored-role swarm preview, loaded through production media admission.
    Swarm,
    /// Explicit hitscan demonstration through the real preview event renderer.
    EffectHitscan,
    /// Explicit projectile demonstration through the real preview event renderer.
    EffectProjectile,
    /// Explicit laser demonstration through the real preview event renderer.
    EffectLaser,
    /// Explicit lightning demonstration through the real preview event renderer.
    EffectLightning,
    /// Same lightning event at80ms, retaining any intentional strobe-off phase.
    EffectLightningMiddle,
    /// Same lightning event at120ms, retaining any intentional strobe-off phase.
    EffectLightningLate,
    /// Explicit melee demonstration through the real preview event renderer.
    EffectMelee,
    /// Explicit death-fragment demonstration through the real preview event renderer.
    EffectShatter,
    /// Both actual participants before the load transaction.
    CargoBefore,
    /// Passenger absent after a real load and one normal publication tick.
    CargoLoaded,
    /// Passenger present at the exact endpoint after real unload and one tick.
    CargoUnloaded,
}

impl Goal {
    /// Whether this goal belongs to the explicit ownership extension.
    pub(super) fn is_ownership(self) -> bool {
        matches!(
            self,
            Self::NeutralWatchtower
                | Self::Swarm
                | Self::EffectHitscan
                | Self::EffectProjectile
                | Self::EffectLaser
                | Self::EffectLightning
                | Self::EffectLightningMiddle
                | Self::EffectLightningLate
                | Self::EffectMelee
                | Self::EffectShatter
        )
    }
    /// Whether the goal displays normal Lab/swarm preview actors rather than live battle actors.
    pub(super) fn is_preview(self) -> bool {
        self == Self::Lab || self.is_ownership() && self != Self::NeutralWatchtower
    }
    /// Selects the identity/lifecycle evidence context without changing scene state.
    pub(super) fn ownership_view(self) -> super::ownership::View {
        use super::ownership::{EffectKind, View};
        match self {
            Self::Lab => View::Lab,
            Self::Return => View::Return,
            Self::Reset => View::Reset,
            Self::NeutralWatchtower => View::NeutralTower,
            Self::Swarm => View::Swarm,
            Self::EffectHitscan => View::Effect(EffectKind::Hitscan),
            Self::EffectProjectile => View::Effect(EffectKind::Projectile),
            Self::EffectLaser => View::Effect(EffectKind::Laser),
            Self::EffectLightning | Self::EffectLightningMiddle | Self::EffectLightningLate => {
                View::Effect(EffectKind::Lightning)
            }
            Self::EffectMelee => View::Effect(EffectKind::Melee),
            Self::EffectShatter => View::Effect(EffectKind::Shatter),
            _ => View::Battle,
        }
    }

    /// Declared elapsed sample; later lightning goals retain the original event.
    pub(super) fn effect_elapsed(self) -> std::time::Duration {
        std::time::Duration::from_millis(match self {
            Self::EffectLightningMiddle => 80,
            Self::EffectLightningLate => 120,
            _ => 40,
        })
    }

    pub(super) const fn is_transport(self) -> bool {
        matches!(
            self,
            Self::CargoBefore | Self::CargoLoaded | Self::CargoUnloaded
        )
    }
}

/// Explicit participants and exact endpoint for the capture-only transport sequence.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportCaptureSpec {
    /// Actual player slot, zero through seven.
    pub owner: u8,
    /// Zero-based ordinal in ID-sorted living/completed mobile nonfoundation actors.
    pub passenger_ordinal: usize,
    /// Distinct ordinal in the same actual scenario roster.
    pub carrier_ordinal: usize,
    /// Exact integer world-space X/Y endpoint; authoritative placement validates it.
    pub destination: [i32; 2],
}

/// One named screenshot goal with a bounded simulation wait.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    /// Filename-safe unique stem for the resulting PNG.
    pub name: String,
    /// Observable production lifecycle condition.
    pub goal: Goal,
    /// Existing camera distance in world units; restoration goals retain saved poses.
    pub distance: f32,
    /// Maximum ticks spent reaching this checkpoint's condition.
    pub max_ticks: u32,
}

/// Versioned capture controls; all coordinates describe the actual output window.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureScript {
    /// Supported version is one.
    pub format_version: u32,
    /// Physical output width and height in pixels.
    pub resolution: [u32; 2],
    /// Capture camera focus for overview/reset, in the loaded map's X/Z world plane.
    pub overview_focus: [f32; 2],
    /// Owner slot used to choose the first real ground ramp actor deterministically.
    pub ramp_player: u8,
    /// Analysis region in normalized image coordinates: left, top, width, height.
    /// Must exclude UI so interface text cannot conceal a blank game scene.
    pub gameplay_region: [f32; 4],
    /// Real wall-time deadline per checkpoint, independent of frozen virtual time.
    pub timeout_seconds: u64,
    /// Conservative total rendered-frame budget for all waits.
    pub max_frames: u64,
    /// Complete lifecycle sequence, validated before running.
    pub checkpoints: Vec<Checkpoint>,
    /// Required only for the exact three-goal transport sequence.
    #[serde(default)]
    pub transport: Option<TransportCaptureSpec>,
    /// Declares the exact seven battle goals plus all ten ownership checks.
    /// Mutually exclusive with the transport-only profile.
    #[serde(default)]
    pub ownership: bool,
    /// Optional passive phase audit at the actual Swarm checkpoint only.
    #[serde(default)]
    pub shell_audit: Option<crate::shell_order_audit::AuditConfig>,
}

impl CaptureScript {
    /// Reads and validates a script without changing application state.
    ///
    /// # Errors
    /// Returns exact-path parse/I/O errors or a typed script-contract error.
    pub fn load(path: &Path) -> Result<Self, CaptureError> {
        let source = std::fs::read_to_string(path).map_err(|source| CaptureError::Io {
            path: path.into(),
            source,
        })?;
        let script: Self = ron::from_str(&source).map_err(|source| CaptureError::Parse {
            path: path.into(),
            source,
        })?;
        script.validate()?;
        Ok(script)
    }

    /// Validates finite controls, supported zoom, safe filenames and required order.
    ///
    /// # Errors
    /// Rejects any ambiguous, unsupported or unbounded capture request.
    pub fn validate(&self) -> Result<(), CaptureError> {
        let invalid = |reason: &str| CaptureError::InvalidScript(reason.into());
        if let Some(audit) = &self.shell_audit {
            audit.validate()?;
            if !self.ownership || self.transport.is_some() {
                return Err(invalid(
                    "shell audit requires the explicit ownership Swarm profile",
                ));
            }
        }
        if self.format_version != 1 {
            return Err(invalid("unsupported format version"));
        }
        if self.resolution.iter().any(|n| *n < 320 || *n > 4096) {
            return Err(invalid(
                "resolution must be 320..=4096 pixels per dimension",
            ));
        }
        if self.timeout_seconds == 0
            || self.timeout_seconds > 600
            || self.max_frames == 0
            || self.max_frames > 100_000
        {
            return Err(invalid(
                "deadline/frame budgets must be positive and bounded",
            ));
        }
        if self.ramp_player >= 8
            || self
                .overview_focus
                .iter()
                .any(|n| !n.is_finite() || *n < 0.5)
        {
            return Err(invalid("invalid owner or overview focus"));
        }
        let [x, y, w, h] = self.gameplay_region;
        if [x, y, w, h].iter().any(|n| !n.is_finite())
            || x < 0.0
            || y < 0.0
            || w <= 0.0
            || h <= 0.0
            || x + w > 1.0
            || y + h > 1.0
        {
            return Err(invalid("gameplay analysis region must be inside the image"));
        }
        let required = [
            Goal::Overview,
            Goal::RampEntry,
            Goal::RampExit,
            Goal::Combat,
            Goal::Lab,
            Goal::Return,
            Goal::Reset,
        ];
        if self.ownership && self.transport.is_some() {
            return Err(invalid("ownership and transport profiles cannot be mixed"));
        }
        if self.ownership {
            let extra = [
                Goal::NeutralWatchtower,
                Goal::Swarm,
                Goal::EffectHitscan,
                Goal::EffectProjectile,
                Goal::EffectLaser,
                Goal::EffectLightning,
                Goal::EffectLightningMiddle,
                Goal::EffectLightningLate,
                Goal::EffectMelee,
                Goal::EffectShatter,
            ];
            if self
                .checkpoints
                .iter()
                .map(|c| c.goal)
                .ne(required.into_iter().chain(extra))
                || self.checkpoints.iter().skip(7).any(|c| c.max_ticks != 0)
            {
                return Err(invalid("ownership requires seven battle goals followed by all ten zero-tick ownership checks"));
            }
            if self.checkpoints[12..=14]
                .iter()
                .any(|point| point.distance.to_bits() != self.checkpoints[12].distance.to_bits())
            {
                return Err(invalid(
                    "lightning lifecycle samples require the same fixed camera distance",
                ));
            }
        } else if let Some(transport) = &self.transport {
            if transport.owner >= 8 || transport.passenger_ordinal == transport.carrier_ordinal {
                return Err(invalid(
                    "transport requires a valid owner and distinct ordinals",
                ));
            }
            if self.checkpoints.iter().map(|c| c.goal).ne([
                Goal::CargoBefore,
                Goal::CargoLoaded,
                Goal::CargoUnloaded,
            ]) || self.checkpoints.iter().any(|c| c.max_ticks != 1)
            {
                return Err(invalid(
                    "transport requires exactly before/loaded/unloaded with one-tick bounds",
                ));
            }
            if self
                .checkpoints
                .iter()
                .any(|c| c.distance.to_bits() != self.checkpoints[0].distance.to_bits())
            {
                return Err(invalid(
                    "transport checkpoints require the same fixed camera distance",
                ));
            }
        } else if self.checkpoints.iter().map(|c| c.goal).ne(required) {
            return Err(invalid(
                "expected the seven battle goals, or the explicit transport sequence",
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for checkpoint in &self.checkpoints {
            if checkpoint.name.is_empty()
                || !checkpoint
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                || !names.insert(&checkpoint.name)
            {
                return Err(invalid(
                    "checkpoint names must be unique safe filename stems",
                ));
            }
            if !checkpoint.distance.is_finite()
                || !(crate::MIN_CAMERA_DISTANCE..=crate::MAX_CAMERA_DISTANCE)
                    .contains(&checkpoint.distance)
                || checkpoint.max_ticks > 10_000
            {
                return Err(invalid(
                    "camera distance or simulation tick bound is unsupported",
                ));
            }
        }
        Ok(())
    }
}
