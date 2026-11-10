//! The spell catalog: `statuses.ron` and `ability_effects.ron`, flattened and validated.
//!
//! # Responsibility
//! Parses the two shared spell tables (plan `2026-11-10-spellcaster-mechanics`, Amendment 1,
//! D-3, D-17, D-18, D-20), interns statuses to [`StatusId`] and effect nodes to [`EffectId`],
//! flattens every effect tree into one read-only arena with child ranges, computes the
//! per-node [`EffectFacts`] once, and rejects every unsupported shape with a typed
//! [`CatalogError`] before any world exists.
//!
//! # How it fits
//! [`super::RuntimeCatalogs`] loads both files and hands the result to the unit catalog
//! ([`super::UnitCatalog::set_spell_catalog`]), which checks the abilities that name an effect
//! root. The engine in `crate::effects` reads the arena by id and never allocates per walk.
//!
//! # Invariants
//! - Ids are dense and assigned in name order: statuses by name; named effect nodes by name,
//!   then inline nodes depth-first pre-order under their named parent, parents in name order.
//!   Reordering the files' entries changes no id.
//! - `Use` is resolved and erased: no arena node refers to a name.
//! - After a successful load the graph of child edges plus `ApplyStatus` → status effect edges
//!   is acyclic, every synchronous chain is at most [`MAX_EFFECT_DEPTH`] nodes, no `Search`
//!   reaches another `Search` synchronously, and every reached node gets the target kind it
//!   needs (D-15).

mod checks;
mod flatten;
#[cfg(test)]
mod tests;

use crate::error::{SimCoreError, SimResult};
use serde::Deserialize;
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, AttributeBonuses, DamageResponses,
    EffectAnchor, EffectDef, EffectId, FilterAlliance, MissileView, SimScalar, StatusId,
    StatusSpec, StructureRule, VitalChange,
};
use std::collections::BTreeMap;
use thiserror::Error;

/// The longest synchronous chain of one walk, in nodes. The deepest walk of the four planned
/// spells, Psionic Storm's cast walk `Persistent → Search → ApplyStatus → Damage`, is 4; 8
/// leaves room for one more level of nesting on each side and sizes the engine's fixed stack.
pub const MAX_EFFECT_DEPTH: usize = 8;
/// The most children of one `Set`: the widest set of the planned spells has 3 (Abduct's and
/// Fungal Growth's); the editor's sets in the four chains stay below 8.
pub const MAX_SET_CHILDREN: usize = 8;
/// The most distinct nodes reachable from one named node, status effects counted: Abduct, the
/// largest planned tree, reaches 7. It bounds the work of a walk per unit found.
pub const MAX_TREE_NODES: usize = 64;
/// The most nodes of the whole flattened arena; far below `u16::MAX` so ids fit with margin.
pub const MAX_EFFECT_NODES: usize = 4096;
/// The largest search radius in world units: larger than any `StarCraft` II spell area (the
/// largest planned is 2.25) and the spatial grid's query stays local.
pub const MAX_SEARCH_RADIUS: i32 = 16;
/// The greatest `max_count` of a search.
pub const MAX_SEARCH_COUNT: u16 = 256;
/// The most pulses of a persistent (Blinding Cloud has 15).
pub const MAX_PERSISTENT_COUNT: u16 = 1024;
/// The fastest missile in world units per tick (the fastest planned is 1.05).
pub const MAX_MISSILE_SPEED: i32 = 16;
/// The longest displacement flight in ticks (ten Faster seconds; Abduct flies 18).
pub const MAX_FLIGHT_TICKS: u16 = 250;

/// The slowest missile in world units per tick: 0.01, written as a quotient so no float enters.
#[must_use]
pub fn min_missile_speed() -> SimScalar {
    SimScalar::ONE.saturating_div(SimScalar::from_i32(100))
}

/// A typed failure of the spell catalog (plan D-15, D-17).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CatalogError {
    /// An effect key names no entry of `ability_effects.ron`.
    #[error("'{referenced_by}' names unknown effect '{name}'")]
    UnknownEffect {
        /// The unknown key.
        name: String,
        /// The effect, status or ability that names it.
        referenced_by: String,
    },
    /// A status key names no entry of `statuses.ron`.
    #[error("'{referenced_by}' names unknown status '{name}'")]
    UnknownStatus {
        /// The unknown key.
        name: String,
        /// The effect or ability that names it.
        referenced_by: String,
    },
    /// Two effects share a name.
    #[error("duplicate effect '{name}'")]
    DuplicateEffect {
        /// The repeated name.
        name: String,
    },
    /// Two statuses share a name.
    #[error("duplicate status '{name}'")]
    DuplicateStatus {
        /// The repeated name.
        name: String,
    },
    /// Effects reach themselves through children, `Use` or status effects.
    #[error("effect cycle: {}", path.join(" -> "))]
    EffectCycle {
        /// The nodes of the cycle, its first node repeated at the end.
        path: Vec<String>,
    },
    /// A synchronous chain is longer than [`MAX_EFFECT_DEPTH`].
    #[error("effect '{root}' runs {depth} nodes deep in one walk (at most {MAX_EFFECT_DEPTH})")]
    EffectTreeTooDeep {
        /// The node that starts the chain.
        root: String,
        /// The chain's length in nodes.
        depth: usize,
    },
    /// A `Set` has no child.
    #[error("set '{effect}' has no child")]
    EmptySet {
        /// The set.
        effect: String,
    },
    /// A `Set` has more than [`MAX_SET_CHILDREN`] children.
    #[error("set '{effect}' has {len} children (at most {MAX_SET_CHILDREN})")]
    SetTooWide {
        /// The set.
        effect: String,
        /// Its child count.
        len: usize,
    },
    /// More than [`MAX_TREE_NODES`] nodes are reachable from one named node.
    #[error("effect '{root}' reaches {nodes} nodes (at most {MAX_TREE_NODES})")]
    EffectTreeTooLarge {
        /// The named node.
        root: String,
        /// The nodes it reaches, itself included.
        nodes: usize,
    },
    /// The flattened arena holds more than [`MAX_EFFECT_NODES`] nodes.
    #[error("the effect catalog has more than {MAX_EFFECT_NODES} nodes")]
    TooManyEffectNodes,
    /// A `Search` is reached synchronously under another `Search`'s child.
    #[error("search '{inner}' runs inside search '{outer}'")]
    NestedSearch {
        /// The enclosing search.
        outer: String,
        /// The search it reaches.
        inner: String,
    },
    /// A search radius or count is out of bounds.
    #[error("search '{effect}': {reason}")]
    InvalidSearch {
        /// The search.
        effect: String,
        /// The violated bound.
        reason: &'static str,
    },
    /// A persistent's or status's period or count is out of bounds.
    #[error("'{definition}': {reason}")]
    InvalidPeriod {
        /// The persistent or status.
        definition: String,
        /// The violated bound.
        reason: &'static str,
    },
    /// A persistent has neither an initial nor a periodic effect.
    #[error("persistent '{effect}' runs nothing")]
    EmptyPersistent {
        /// The persistent.
        effect: String,
    },
    /// A missile speed is outside `0.01..=16` world units per tick.
    #[error("missile '{effect}' speed must be between 0.01 and {MAX_MISSILE_SPEED} per tick")]
    InvalidMissileSpeed {
        /// The missile.
        effect: String,
    },
    /// A displacement's flight is outside `1..=250` ticks.
    #[error("displacement '{effect}' must fly 1 to {MAX_FLIGHT_TICKS} ticks")]
    InvalidDisplacement {
        /// The displacement.
        effect: String,
    },
    /// A damage amount or attribute bonus is negative.
    #[error("damage '{effect}' is negative")]
    NegativeDamage {
        /// The damage node.
        effect: String,
    },
    /// A vital change names no vital or a fraction outside `-1..=1`.
    #[error("vital change '{effect}': {reason}")]
    InvalidVitalChange {
        /// The node.
        effect: String,
        /// The violated rule.
        reason: &'static str,
    },
    /// A status value is out of bounds.
    #[error("status '{status}': {reason}")]
    InvalidStatus {
        /// The status.
        status: String,
        /// The violated rule.
        reason: &'static str,
    },
    /// A node that acts on a unit is reached with a point (D-15).
    #[error("effect '{effect}' needs a unit but is reached with a point from '{reached_from}'")]
    EffectTargetMismatch {
        /// The node.
        effect: String,
        /// The ability or node the point comes from.
        reached_from: String,
    },
    /// A `Displace` to the origin is reached from an ability without origin placement.
    #[error("ability '{ability}' reaches a displacement in '{effect}' but places no origin")]
    DisplaceWithoutOrigin {
        /// The ability's root effect.
        effect: String,
        /// The ability, as `archetype/ability`.
        ability: String,
    },
    /// An `Effect` ability is neither `TargetPosition` nor `TargetEntity`.
    #[error("ability '{ability}' runs an effect but targets neither a point nor a unit")]
    EffectAbilityNeedsTarget {
        /// The ability, as `archetype/ability`.
        ability: String,
    },
}

/// A [`sim_types::TargetFilter`] with its status key resolved and its planes as flags.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ResolvedFilter {
    /// Owners admitted, relative to the context's owner.
    pub alliance: FilterAlliance,
    /// Whether ground units pass.
    pub ground: bool,
    /// Whether air units pass.
    pub air: bool,
    /// The structure rule.
    pub structures: StructureRule,
    /// A status the unit must not carry.
    pub without_status: Option<StatusId>,
}

/// One flattened node kind (D-1, D-18). Children are ids into the same arena.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EffectNode {
    /// Runs `len` children from `first` of the shared child slab, in order.
    Set {
        /// First index into the child slab.
        first: u16,
        /// Number of children.
        len: u8,
    },
    /// Runs `child` per unit found.
    Search {
        /// Radius in world units.
        radius: SimScalar,
        /// Units admitted.
        filter: ResolvedFilter,
        /// Greatest number of units, nearest first.
        max_count: Option<u16>,
        /// Node run per unit.
        child: EffectId,
    },
    /// Creates a pulsing instance.
    Persistent {
        /// Where it sits.
        anchor: EffectAnchor,
        /// Run at creation.
        initial: Option<EffectId>,
        /// Run at each pulse.
        periodic: Option<EffectId>,
        /// Exact ticks between pulses.
        period_ticks: SimScalar,
        /// Number of pulses.
        count: u16,
    },
    /// Launches a missile.
    LaunchMissile {
        /// Fixed point or homing on a unit.
        to: EffectAnchor,
        /// World units per tick.
        speed: SimScalar,
        /// Presentation.
        view: MissileView,
        /// Run on arrival.
        impact: EffectId,
    },
    /// Deals spell damage.
    Damage {
        /// Damage points.
        amount: SimScalar,
        /// Extra damage per attribute.
        attribute_bonus: AttributeBonuses,
        /// Victim responses.
        responses: DamageResponses,
    },
    /// Applies a status.
    ApplyStatus {
        /// The status.
        status: StatusId,
    },
    /// Removes a status.
    RemoveStatus {
        /// The status.
        status: StatusId,
    },
    /// Changes vitals.
    ModifyVitals {
        /// Life change.
        life: Option<VitalChange>,
        /// Shield change.
        shields: Option<VitalChange>,
        /// Energy change.
        energy: Option<VitalChange>,
    },
    /// Cancels the target's orders.
    CancelOrders {
        /// Leave a burrowed mode at once.
        force_unburrow: bool,
    },
    /// Moves the target unit to the context's origin.
    Displace {
        /// Flight ticks.
        flight_ticks: u16,
        /// Present a tether.
        tether: bool,
        /// Run on landing.
        on_land: Option<EffectId>,
    },
}

/// One arena entry: the node and its validator.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FlatNode {
    /// What the node does.
    pub kind: EffectNode,
    /// The unit predicate checked before it runs.
    pub require: Option<ResolvedFilter>,
}

/// Facts about a node computed once at load (D-18, D-19).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectFacts {
    /// Whether the node acts on a unit (a point target is a load error).
    pub needs_unit: bool,
    /// Nodes of the longest synchronous chain the node starts, itself included.
    pub sync_depth: u8,
    /// For a point-anchored persistent: the largest radius of a search its initial or periodic
    /// walk reaches. `None` for every other node.
    pub area_radius: Option<SimScalar>,
    /// The radius of the first search met in pre-order through sets, persistents and missile
    /// impacts: the targeting ring of an ability rooted here.
    pub targeting_radius: Option<SimScalar>,
    /// Whether a displacement is reachable through any edge, status effects included.
    pub needs_origin: bool,
    /// Whether the node's synchronous walk reaches a tethered displacement (a unit-anchored
    /// persistent presents a link while it waits, D-19).
    pub reaches_tether: bool,
}

/// A status with its effect keys resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedStatus {
    /// The authored specification.
    pub spec: StatusSpec,
    /// The effect run at the first application.
    pub initial_effect: Option<EffectId>,
    /// The effect run at each pulse.
    pub periodic_effect: Option<EffectId>,
}

/// The flattened, validated spell catalog. Not hashed: lockstep peers load equal files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpellCatalog {
    nodes: Vec<FlatNode>,
    children: Vec<EffectId>,
    facts: Vec<EffectFacts>,
    labels: Vec<String>,
    by_name: BTreeMap<String, EffectId>,
    statuses: Vec<ResolvedStatus>,
    status_by_name: BTreeMap<String, StatusId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusFile {
    statuses: Vec<StatusSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectFile {
    effects: Vec<EffectDef>,
}

impl CatalogError {
    /// Whether the failure lies in `statuses.ron` alone (a status's own values or names), as
    /// opposed to the effect trees or the abilities that name them.
    #[must_use]
    pub const fn is_status_only(&self) -> bool {
        matches!(
            self,
            Self::DuplicateStatus { .. } | Self::InvalidStatus { .. }
        )
    }
}

impl SpellCatalog {
    /// Parses, flattens and validates the text of `statuses.ron` and `ability_effects.ron`.
    ///
    /// # Errors
    /// [`SimCoreError::RonDeserializationError`] for malformed RON or unknown fields, and
    /// [`SimCoreError::SpellCatalog`] for every rule of plan D-15 and D-17.
    pub fn from_ron_strs(statuses_ron: &str, effects_ron: &str) -> SimResult<Self> {
        let statuses = Self::parse_statuses(statuses_ron)?;
        let effects = Self::parse_effects(effects_ron)?;
        Ok(Self::from_defs(statuses, effects)?)
    }

    /// Parses the text of `statuses.ron` without validating it.
    ///
    /// # Errors
    /// [`SimCoreError::RonDeserializationError`] for malformed RON or unknown fields.
    pub fn parse_statuses(statuses_ron: &str) -> SimResult<Vec<StatusSpec>> {
        ron::from_str::<StatusFile>(statuses_ron)
            .map(|file| file.statuses)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))
    }

    /// Parses the text of `ability_effects.ron` without validating it.
    ///
    /// # Errors
    /// [`SimCoreError::RonDeserializationError`] for malformed RON or unknown fields.
    pub fn parse_effects(effects_ron: &str) -> SimResult<Vec<EffectDef>> {
        ron::from_str::<EffectFile>(effects_ron)
            .map(|file| file.effects)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))
    }

    /// Flattens and validates already parsed definitions.
    ///
    /// # Errors
    /// The first violated rule, as a [`CatalogError`].
    pub fn from_defs(
        statuses: Vec<StatusSpec>,
        effects: Vec<EffectDef>,
    ) -> Result<Self, CatalogError> {
        let mut catalog = flatten::flatten(statuses, effects)?;
        checks::validate_and_compute_facts(&mut catalog)?;
        Ok(catalog)
    }

    /// The node `id`, or `None` past the arena.
    #[must_use]
    pub fn node(&self, id: EffectId) -> Option<FlatNode> {
        self.nodes.get(usize::from(id.0)).copied()
    }

    /// The precomputed facts of `id`.
    #[must_use]
    pub fn facts(&self, id: EffectId) -> Option<EffectFacts> {
        self.facts.get(usize::from(id.0)).copied()
    }

    /// The author-facing label of `id`: its name, or `parent/n` for an inline node.
    #[must_use]
    pub fn label(&self, id: EffectId) -> Option<&str> {
        self.labels.get(usize::from(id.0)).map(String::as_str)
    }

    /// The child `index` of a set whose children start at `first`.
    #[must_use]
    pub fn set_child(&self, first: u16, index: u8) -> Option<EffectId> {
        self.children
            .get(usize::from(first) + usize::from(index))
            .copied()
    }

    /// The id the effect `name` resolves to.
    #[must_use]
    pub fn effect_id(&self, name: &str) -> Option<EffectId> {
        self.by_name.get(name).copied()
    }

    /// The id of the status `name`.
    #[must_use]
    pub fn status_id(&self, name: &str) -> Option<StatusId> {
        self.status_by_name.get(name).copied()
    }

    /// The status `id`.
    #[must_use]
    pub fn status(&self, id: StatusId) -> Option<&ResolvedStatus> {
        self.statuses.get(usize::from(id.0))
    }

    /// `filter` with its status key resolved against this catalog, or `None` when it names a
    /// status the catalog lacks (catalog validation refuses such an ability at load).
    #[must_use]
    pub fn resolve_filter(&self, filter: &sim_types::TargetFilter) -> Option<ResolvedFilter> {
        let without_status = match filter.without_status.as_deref() {
            Some(name) => Some(self.status_id(name)?),
            None => None,
        };
        Some(ResolvedFilter {
            alliance: filter.alliance,
            ground: filter.planes.contains(&sim_types::TargetPlane::Ground),
            air: filter.planes.contains(&sim_types::TargetPlane::Air),
            structures: filter.structures,
            without_status,
        })
    }

    /// Number of nodes in the arena.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of statuses.
    #[must_use]
    pub fn status_count(&self) -> usize {
        self.statuses.len()
    }

    /// Checks that `root`, run by an ability whose target is a point (`unit_target` false) or
    /// a unit, reaches every node with the target kind it needs, and that a displacement is
    /// reached only when the ability places an origin. `ability` names the ability as
    /// `archetype/ability` in errors.
    ///
    /// # Errors
    /// [`CatalogError::EffectTargetMismatch`] or [`CatalogError::DisplaceWithoutOrigin`].
    pub fn check_ability_root(
        &self,
        root: EffectId,
        unit_target: bool,
        has_origin: bool,
        ability: &str,
    ) -> Result<(), CatalogError> {
        checks::check_target_kinds(self, root, unit_target, ability)?;
        let needs_origin = self.facts(root).is_some_and(|facts| facts.needs_origin);
        if needs_origin && !has_origin {
            return Err(CatalogError::DisplaceWithoutOrigin {
                effect: self.label(root).unwrap_or_default().to_owned(),
                ability: ability.to_owned(),
            });
        }
        Ok(())
    }
}

/// Checks one ability of `archetype` against `spells` (see `UnitCatalog::set_spell_catalog`).
pub(super) fn check_ability(
    spells: &SpellCatalog,
    archetype: &str,
    ability: &AbilitySpec,
) -> Result<(), CatalogError> {
    let label = || format!("{archetype}/{}", ability.name);
    if let Some(name) = ability
        .target_filter
        .as_ref()
        .and_then(|filter| filter.without_status.as_ref())
    {
        if spells.status_id(name).is_none() {
            return Err(CatalogError::UnknownStatus {
                name: name.clone(),
                referenced_by: label(),
            });
        }
    }
    let AbilityEffectSpec::Effect(key) = &ability.effect else {
        return Ok(());
    };
    let unit_target = match ability.target_kind {
        AbilityTargetKind::TargetPosition => false,
        AbilityTargetKind::TargetEntity => true,
        AbilityTargetKind::InstantSelf | AbilityTargetKind::ToggleMode => {
            return Err(CatalogError::EffectAbilityNeedsTarget { ability: label() })
        }
    };
    let root = spells
        .effect_id(key)
        .ok_or_else(|| CatalogError::UnknownEffect {
            name: key.clone(),
            referenced_by: label(),
        })?;
    spells.check_ability_root(root, unit_target, ability.origin.is_some(), &label())
}
