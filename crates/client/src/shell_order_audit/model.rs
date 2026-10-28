//! Serializable bounded source and actual transparent-phase evidence.
//!
//! Numeric renderer identities are process-local correlations, never logical sort
//! priorities. Floating source values are recorded as bits and no GPU-readback claim
//! follows from a CPU source record.

use serde::{Deserialize, Serialize};

/// Explicit diagnostic capacity; absent configuration installs no observer.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditConfig {
    /// Maximum relevant render views (one through four).
    pub max_views: usize,
    /// Maximum transparent items in each view (one through4096).
    pub max_items: usize,
    /// Maximum inclusive request/closure epochs (two through256).
    pub max_epochs: usize,
    /// Maximum sum of serialized epoch/asset component bytes, at most 64 MiB.
    /// Final compact publication is also independently checked against this cap.
    /// Transient working copies are not a resident-memory bound.
    pub max_bytes: usize,
    /// Separate maximum raw CPU source sidecar bytes, at most64MiB.
    pub max_source_bytes: usize,
    /// Explicit controlled experiment; absent means passive natural observation.
    #[serde(default)]
    pub fixture: Option<FixtureSpec>,
}

/// Actual loaded Swarm participants and declared world-space centers for an experiment.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureSpec {
    /// Distinct logical renderer IDs authored in the experiment, not unit identities.
    pub participants: [u32; 2],
    /// Actual Bevy world XYZ centers; real radius, texture and optics are preserved.
    pub positions: [[f32; 3]; 2],
    /// Stable declared insertion order across the complete readback interval.
    pub order: FixtureOrder,
}

/// Controlled two-item queue order, never installed in natural observation.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum FixtureOrder {
    /// First declared participant precedes the second before the real stable sort.
    Forward,
    /// Second declared participant precedes the first.
    Reverse,
}

impl AuditConfig {
    /// Validates fixed limits before any diagnostic collection begins.
    /// # Errors
    /// Rejects zero, unbounded or unrepresentable budgets.
    pub fn validate(&self) -> Result<(), AuditError> {
        if let Some(fixture) = &self.fixture {
            if fixture.participants[0] == fixture.participants[1]
                || fixture
                    .positions
                    .iter()
                    .flatten()
                    .any(|value| !value.is_finite())
            {
                return Err(AuditError::Source(
                    "invalid controlled fixture participants/positions".into(),
                ));
            }
        }
        if !(1..=4).contains(&self.max_views)
            || !(1..=4096).contains(&self.max_items)
            || !(2..=256).contains(&self.max_epochs)
            || !(1024..=64 * 1024 * 1024).contains(&self.max_bytes)
            || !(1024..=64 * 1024 * 1024).contains(&self.max_source_bytes)
        {
            return Err(AuditError::Budget("invalid declared limits"));
        }
        Ok(())
    }
}

/// Failures retain incomplete diagnostic evidence and invalidate capture acceptance.
#[derive(Clone, Debug, thiserror::Error)]
pub enum AuditError {
    /// A prior panic invalidated the cross-world evidence owner.
    #[error("shell audit evidence lock poisoned")]
    Poisoned,
    /// Declared evidence capacity was exceeded; no records may be truncated.
    #[error("shell audit budget: {0}")]
    Budget(&'static str),
    /// A monotonic identity cannot advance safely.
    #[error("shell audit identity exhausted")]
    Exhausted,
    /// A callback, phase or closure violated the explicit interval protocol.
    #[error("shell audit lifecycle: {0}")]
    Lifecycle(&'static str),
    /// A source or draw association is invalid.
    #[error("shell audit source: {0}")]
    Source(String),
    /// Relevant actual pipeline/material data was not ready.
    #[error("shell audit draw not ready: {0}")]
    NotReady(String),
    /// Complete observations do not support a unique image/order association.
    #[error("shell audit inconclusive: {0}")]
    Inconclusive(&'static str),
}

/// Scene-domain logical identity; ECS allocation and ownership are not sort keys.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct LogicalIdentity {
    /// Actual renderer producer domain.
    pub domain: &'static str,
    /// Logical snapshot EntityId, distinct from either Bevy entity namespace.
    pub entity: u32,
    /// Committed generation; zero denotes the persistent Swarm preview domain.
    pub generation: u64,
}

/// Visible main-world mesh and its actual CPU source association.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct SourceItem {
    /// Main-world Bevy entity bits, used only for cross-world correlation.
    pub main_entity: u64,
    /// Present for a validated shell producer identity.
    pub logical: Option<LogicalIdentity>,
    /// Actual PlayerId value for a shell; absent for unrelated geometry.
    pub owner: Option<u8>,
    /// Column-major world transform as exact f32 bits.
    pub world_from_local: [u32; 16],
    /// Local AABB center XYZ then half extents XYZ, as f32 bits.
    pub bounds: Option<[u32; 6]>,
    /// Actual typed mesh asset identity, resolved in the content inventory.
    pub mesh: String,
    /// Typed supported source or an explicit unresolved association.
    pub material: MaterialSource,
}

/// Material-specific evidence; unsupported draws never receive fabricated IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum MaterialSource {
    /// Real bubble material and complete 160-byte CPU uniform.
    Shell {
        /// Actual material asset ID.
        id: String,
        /// Exact initialized uniform bytes.
        uniform: Vec<u8>,
        /// Actual bound image asset identity.
        image: String,
    },
    /// Actual PBR material descriptor, with referenced images inventoried separately.
    Standard {
        /// Actual material asset ID.
        id: String,
        /// Locked engine's complete CPU material descriptor.
        description: String,
    },
    /// Actual mist material; complete selected data is in the packet witness.
    Mist {
        /// Actual material asset ID.
        id: String,
        /// Actual material-bound header, sample and frame buffers, in binding order.
        buffers: [String; 3],
    },
    /// No typed adapter exists; coverage is explicit rather than inferred.
    Unsupported {
        /// Concrete missing association or unsupported type.
        reason: &'static str,
    },
}

/// One extraction's camera-independent scene and default-Time source state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct SourceFrame {
    /// Exact default-Time elapsed duration in checked u64 nanoseconds.
    pub elapsed_nanos: u64,
    /// Exact CPU source bits of Globals.time, not private GPU uniform readback.
    pub wrapped_time_bits: u32,
    /// Default-Time delta seconds as f32 bits; accepted capture freezes it to zero.
    pub delta_bits: u32,
    /// Actual active scene name.
    pub scene: String,
    /// Sorted source descriptions for active environment, rig and camera settings.
    pub environment: Vec<String>,
    /// Actual visible geometry sorted by main-world identity for binary lookup.
    pub items: Vec<SourceItem>,
    /// Relevant unobserved source state prevents a complete causal claim.
    pub limitations: Vec<String>,
    /// Indices into the artifact's immutable deduplicated CPU asset revisions.
    pub assets: Vec<usize>,
    /// Actual total instantiated Swarm actors, distinct from visible draw count.
    pub instantiated_swarm_shells: usize,
    /// Declared controlled intervention, absent for natural observation.
    pub fixture: Option<String>,
}

/// Immutable CPU content associated with one extracted asset revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct AssetRecord {
    /// Actual process-local asset identity and kind.
    pub identity: String,
    /// Exact format/sampler/topology descriptor, not an inferred appearance.
    pub descriptor: String,
    /// Actual CPU content length, independent of serialized metadata length.
    pub byte_length: usize,
    /// BLAKE3-256 of the exact retained CPU bytes, not GPU contents.
    pub blake3_256: String,
    /// Binary sidecar path relative to the capture output directory.
    pub blob: String,
}

/// Serializable selection proof; string IDs are formatted only at observation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct MistEvidence {
    /// Whether any complete packet has been selected.
    pub ready: bool,
    /// Real packet sequence, allowed to advance with byte-identical contents.
    pub sequence: u64,
    /// Whether this render frame selected a fresh staging packet.
    pub freshly_selected: bool,
    /// Packed header/sample/frame bytes, not GPU-readback data.
    pub bytes: Vec<u8>,
    /// Persistent storage buffers actually bound by the uploader.
    pub targets: [String; 3],
    /// Actual staging buffer identities.
    pub staging: Vec<String>,
}

impl From<crate::mist::packet_witness::PacketProof> for MistEvidence {
    fn from(proof: crate::mist::packet_witness::PacketProof) -> Self {
        Self {
            ready: proof.ready,
            sequence: proof.sequence,
            freshly_selected: proof.freshly_selected,
            bytes: proof.bytes,
            targets: proof.targets.map(|id| format!("{id:?}")),
            staging: proof.staging.iter().map(|id| format!("{id:?}")).collect(),
        }
    }
}

/// Audited built-in material tuple whose item queries use the main-world identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum DrawAdapter {
    /// Actual registered bubble material draw tuple.
    Shell,
    /// Actual registered StandardMaterial draw tuple.
    Standard,
    /// Actual registered mist material draw tuple.
    Mist,
}

/// Explicit admission outcome; unsupported commands never gain a proxy exemption.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum AssociationKind {
    /// A marker-proven temporary proxy and its actual supported draw tuple.
    Temporary(DrawAdapter),
    /// A persistent render entity with matching MainEntity and supported draw.
    Synchronized(DrawAdapter),
    /// Required actual registration has not yet appeared.
    Pending,
    /// Mapping is observed but its draw behavior is not covered by this audit.
    Unsupported(&'static str),
}

/// Per-view proof observed independently at both queue/sort boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct RenderAssociation {
    /// Exact raw render/main pair occurs once in this view's actual mesh list.
    pub visible_pair_confirmed: bool,
    /// Actual render-world TemporaryRenderEntity component presence.
    pub temporary_marker: bool,
    /// Actual render-world MainEntity component, absent on marker-only proxies.
    pub render_main_entity: Option<u64>,
    /// Checked draw and lifecycle classification; no guessed identity.
    pub kind: AssociationKind,
}

/// Complete actual Transparent3d item plus its explicit source association.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PhaseEntry {
    /// Position in this queued or sorted phase vector.
    pub ordinal: usize,
    /// Actual render-world entity bits.
    pub render_entity: u64,
    /// Actual per-view mapping, marker and supported draw proof.
    pub association: RenderAssociation,
    /// Actual MainEntity bits carried by the queued draw.
    pub main_entity: u64,
    /// Validated shell identity when this is a supported shell draw.
    pub logical: Option<LogicalIdentity>,
    /// Exact view-space Z radix key bits; signed zero is preserved.
    pub distance_bits: u32,
    /// Actual cached specialized pipeline ID, process-local only.
    pub pipeline: String,
    /// Checked published descriptor; explicit None means not published yet.
    pub pipeline_description: String,
    /// Actual draw-function ID, process-local only.
    pub draw_function: String,
    /// Resolved material source or explicit unsupported reason.
    pub material: MaterialSource,
    /// Supported source mesh identity if available.
    pub mesh: Option<String>,
    /// Actual phase batch start/end before batching preparation.
    pub batch_range: [u32; 2],
    /// Complete engine phase-extra-index representation.
    pub extra_index: String,
}

impl PhaseEntry {
    /// Compares every payload field except vector ordinal and render proxy ID.
    fn same_payload(&self, other: &Self) -> bool {
        self.association == other.association
            && self.main_entity == other.main_entity
            && self.logical == other.logical
            && self.distance_bits == other.distance_bits
            && self.pipeline == other.pipeline
            && self.pipeline_description == other.pipeline_description
            && self.draw_function == other.draw_function
            && self.material == other.material
            && self.mesh == other.mesh
            && self.batch_range == other.batch_range
            && self.extra_index == other.extra_index
    }

    /// Ordered comparison exempts only two fully admitted temporary proxy IDs.
    pub(super) fn same_draw(&self, other: &Self) -> bool {
        self.ordinal == other.ordinal
            && self.same_payload(other)
            && (self.render_entity == other.render_entity
                || (self.association.visible_pair_confirmed
                    && self.association.temporary_marker
                    && matches!(self.association.kind, AssociationKind::Temporary(_))))
    }
}

/// Actual per-view state and phase vectors surrounding the engine's stable sort.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ViewEvidence {
    /// Actual render-world camera entity bits.
    pub render_entity: u64,
    /// Associated main-world camera identity.
    pub main_entity: u64,
    /// Column-major camera transform, exact f32 bits.
    pub world_from_view: [u32; 16],
    /// Actual extracted projection, exact f32 bits.
    pub clip_from_view: [u32; 16],
    /// Optional actual precomputed projection/world matrix.
    pub clip_from_world: Option<[u32; 16]>,
    /// Physical viewport origin XY and size XY in pixels.
    pub viewport: [u32; 4],
    /// Actual render target HDR policy.
    pub hdr: bool,
    /// Actual extracted color-grading settings.
    pub color_grading: String,
    /// Prepared light-buffer association and real extracted lighting source state.
    pub lighting: Vec<String>,
    /// Actual vector fed to the engine sorter.
    pub queued: Vec<PhaseEntry>,
    /// Only controlled fixtures retain the original vector before intervention.
    pub original_queue: Option<Vec<PhaseEntry>>,
    /// Actual vector after the engine sorter and before GPU batching.
    pub sorted: Vec<PhaseEntry>,
}

impl ViewEvidence {
    /// Compares all actual view source state without either observed draw vector.
    pub(super) fn same_metadata(&self, other: &Self) -> bool {
        self.render_entity == other.render_entity
            && self.main_entity == other.main_entity
            && self.world_from_view == other.world_from_view
            && self.clip_from_view == other.clip_from_view
            && self.clip_from_world == other.clip_from_world
            && self.viewport == other.viewport
            && self.hdr == other.hdr
            && self.color_grading == other.color_grading
            && self.lighting == other.lighting
    }
}

/// Completed extraction/queue/sort observation, never an asserted screenshot GPU frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct EpochEvidence {
    /// Checked monotonic extraction identity.
    pub epoch: u64,
    /// Actual source state of this same extraction.
    pub source: SourceFrame,
    /// Every relevant populated 3D view, ordered by render identity.
    pub views: Vec<ViewEvidence>,
    /// All exact queued pipeline/material/packet requirements were ready.
    pub ready: bool,
    /// Retained concrete pending states; armed intervals never discard them.
    pub pending: Vec<String>,
    /// Actual selected packet bytes/buffers, including retained-packet frames.
    pub mist: Option<MistEvidence>,
}

impl EpochEvidence {
    /// Compares visible source/order, allowing byte-identical packet republication.
    pub(crate) fn same_sources_and_order(&self, other: &Self) -> bool {
        self.source == other.source
            && match (&self.mist, &other.mist) {
                (Some(a), Some(b)) => {
                    a.ready == b.ready && a.bytes == b.bytes && a.targets == b.targets
                }
                (None, None) => true,
                _ => false,
            }
            && self.views.len() == other.views.len()
            && self.views.iter().zip(&other.views).all(|(a, b)| {
                // Queue order is intentionally observed, not required to be stable
                // when the real distance sorter produces identical draw order.
                a.same_metadata(b)
                    && a.sorted.len() == b.sorted.len()
                    && a.sorted.iter().zip(&b.sorted).all(|(a, b)| a.same_draw(b))
            })
    }
}
