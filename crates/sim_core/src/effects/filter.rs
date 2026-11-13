//! Unit predicates of the effect engine: which unit a unit-targeted node may act on, and the
//! resolved [`ResolvedFilter`] of searches and validators (plan D-1, D-15).
//!
//! A unit is a valid target while it exists, lives (`hp > 0`) and is not hidden in a
//! transport, the editor's default `Dead` and `Hidden` exclusions. Filters additionally
//! exclude invulnerable units and narrow by owner, plane, structure, attribute and status.

use super::state::EffectWorld;
use crate::catalog::{ResolvedFilter, UnitCatalog};
use crate::entity::Entity;
use sim_types::{EffectContext, EntityId, FilterAlliance, MoverType, ShapeKind, StructureRule};
use std::collections::BTreeMap;

/// The unit `id` if a unit-targeted node may act on it now.
pub(crate) fn unit_target(entities: &BTreeMap<EntityId, Entity>, id: EntityId) -> Option<&Entity> {
    entities
        .get(&id)
        .filter(|entity| entity.is_alive() && !entity.is_loaded_in_transport())
}

/// Whether `entity` is in the air plane now (its mode's override, else its archetype's).
pub(crate) fn is_air(entity: &Entity, catalog: &UnitCatalog) -> bool {
    let Ok(arch) = catalog.get(entity.unit_type_id) else {
        return false;
    };
    let mover = entity
        .active_mode
        .as_deref()
        .and_then(|mode| arch.get_mode(mode))
        .and_then(|mode| mode.mover_type_override)
        .unwrap_or(arch.mover_type);
    mover == MoverType::Air
}

/// Whether `entity` is a structure (a dome footprint).
pub(crate) fn is_structure(entity: &Entity, catalog: &UnitCatalog) -> bool {
    catalog
        .get(entity.unit_type_id)
        .is_ok_and(|arch| matches!(arch.shape, ShapeKind::BuildingDome { .. }))
}

/// Whether `entity` passes `filter` for a node running with `context`.
pub(crate) fn passes_filter(
    filter: ResolvedFilter,
    context: &EffectContext,
    entity: &Entity,
    catalog: &UnitCatalog,
    effects: &EffectWorld,
) -> bool {
    if !entity.is_alive() || entity.is_loaded_in_transport() || entity.is_invulnerable {
        return false;
    }
    let alliance = match filter.alliance {
        FilterAlliance::Any => true,
        FilterAlliance::AnyButSelf => context.caster != Some(entity.id),
        FilterAlliance::Enemy => entity.owner != context.owner,
        FilterAlliance::Friendly => entity.owner == context.owner,
    };
    if !alliance {
        return false;
    }
    let plane = if is_air(entity, catalog) {
        filter.air
    } else {
        filter.ground
    };
    if !plane {
        return false;
    }
    let structure = is_structure(entity, catalog);
    let structures = match filter.structures {
        StructureRule::Include => true,
        StructureRule::Exclude => !structure,
        StructureRule::Only => structure,
    };
    let attributes = filter.without_attributes.is_empty()
        || catalog
            .get(entity.unit_type_id)
            .is_ok_and(|arch| !arch.attributes.intersects(filter.without_attributes));
    structures
        && attributes
        && filter
            .without_status
            .is_none_or(|status| !effects.has_status(entity.id, status))
}
