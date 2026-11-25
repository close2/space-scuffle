//! The authored collision classes of `units.ron` (plan `2026-11-25-collision-classes` D-1, D-2).
//!
//! # Responsibility
//! Collects the class names the unit definitions author (`collision_class: Some("…")` on an
//! archetype and on a mode), checks them, and resolves each name to its [`CollisionClass`]
//! index: the distinct names, **sorted**, take [`CollisionClass::BUILT_IN`], `BUILT_IN + 1`, …
//! in that order, so an index never depends on the order of the file.
//!
//! # How it fits
//! [`UnitCatalog::from_ron_str`](super::UnitCatalog::from_ron_str) builds one
//! [`CollisionClassTable`] from every unit definition before it converts any, passes it to each
//! conversion, and keeps its names (`UnitCatalog::collision_classes`). Movement reads the
//! resolved classes through `MotionPolicy` and sizes its per-class tables by the catalog's count.
//!
//! # Invariants
//! - A name is non-empty ASCII `[a-z0-9_]`; no name is reserved, so `"ground"` authored is a
//!   class of its own, distinct from [`CollisionClass::GROUND`].
//! - Every stance that resolves to one name has one movement layer, so no class spans ground
//!   and air bodies.
//! - At most `u8::MAX - BUILT_IN` names (plan D-2), so every index fits a `u8`.

use super::{UnitArchetypeDef, UnitModeSpecDef};
use crate::error::{CollisionClassLayersFault, CollisionClassNameFault, SimCoreError, SimResult};
use crate::movement::MovementLayer;
use sim_types::CollisionClass;
use std::collections::BTreeMap;

/// The sorted names of the authored collision classes; the name at position `i` is class
/// `BUILT_IN + i`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CollisionClassTable {
    names: Vec<String>,
}

/// The most authored names a catalog may hold (plan D-2: `u8::MAX - 3`).
pub(super) const MAX_AUTHORED_CLASSES: usize = (u8::MAX - CollisionClass::BUILT_IN) as usize;

impl CollisionClassTable {
    /// Collects and checks the names `units` author.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogCollisionClassName`] for an empty name or one with a character
    /// outside `[a-z0-9_]`; [`SimCoreError::CatalogCollisionClassLayers`] when one name resolves
    /// for a ground stance and an air stance; [`SimCoreError::CatalogCollisionClassCount`] for more
    /// than [`MAX_AUTHORED_CLASSES`] names.
    pub(super) fn from_units(units: &[UnitArchetypeDef]) -> SimResult<Self> {
        // Each name, with the layer and definition of the first stance that resolved to it.
        let mut seen: BTreeMap<&str, (MovementLayer, &str)> = BTreeMap::new();
        for unit in units {
            for (name, layer, field) in stances(unit) {
                check_name(&unit.slug, &field, name)?;
                match seen.get(name) {
                    Some(&(first_layer, first)) if first_layer != layer => {
                        return Err(SimCoreError::CatalogCollisionClassLayers(Box::new(
                            CollisionClassLayersFault {
                                name: name.to_owned(),
                                first: first.to_owned(),
                                second: unit.slug.clone(),
                            },
                        )));
                    }
                    Some(_) => {}
                    None => {
                        seen.insert(name, (layer, &unit.slug));
                    }
                }
            }
        }
        if seen.len() > MAX_AUTHORED_CLASSES {
            return Err(SimCoreError::CatalogCollisionClassCount { count: seen.len() });
        }
        Ok(Self {
            names: seen.into_keys().map(str::to_owned).collect(),
        })
    }

    /// The class of the authored `name`, `None` when the table does not hold it.
    pub(super) fn resolve(&self, name: &str) -> Option<CollisionClass> {
        let position = self
            .names
            .binary_search_by(|held| held.as_str().cmp(name))
            .ok()?;
        let offset = u8::try_from(position).ok()?;
        CollisionClass::BUILT_IN
            .checked_add(offset)
            .map(CollisionClass)
    }

    /// The class of the authored `name` of the definition `definition` (field `field`).
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] when the table does not hold `name`, which only a
    /// table built from other definitions can cause.
    pub(super) fn resolve_authored(
        &self,
        definition: &str,
        field: &str,
        name: Option<&str>,
    ) -> SimResult<Option<CollisionClass>> {
        name.map(|name| {
            self.resolve(name)
                .ok_or_else(|| SimCoreError::CatalogInvalidValue {
                    definition: definition.to_owned(),
                    field: field.to_owned(),
                    reason: "names a collision class the catalog did not collect",
                })
        })
        .transpose()
    }

    /// The names, in index order.
    pub(super) fn into_names(self) -> Vec<String> {
        self.names
    }
}

/// The modes a unit definition authors: its capabilities' when it has any, else its own, as
/// the conversion reads them.
pub(super) fn authored_modes(unit: &UnitArchetypeDef) -> &[UnitModeSpecDef] {
    if unit.capabilities.modes.is_empty() {
        &unit.modes
    } else {
        &unit.capabilities.modes
    }
}

/// Every stance of `unit` that resolves to an authored name: the name, the stance's movement
/// layer and the field path that authored the name. A mode that authors no name and is not
/// burrowed takes the archetype's (D-2, rule 3).
fn stances(unit: &UnitArchetypeDef) -> Vec<(&str, MovementLayer, String)> {
    let mut out = Vec::new();
    let archetype = unit.collision_class.as_deref();
    if let Some(name) = archetype {
        out.push((
            name,
            MovementLayer::from_mover(unit.mover_type),
            "collision_class".to_owned(),
        ));
    }
    for mode in authored_modes(unit) {
        let layer = MovementLayer::from_mover(mode.mover_type_override.unwrap_or(unit.mover_type));
        let field = format!("modes.{}.collision_class", mode.mode_name);
        match (mode.collision_class.as_deref(), mode.burrowed, archetype) {
            (Some(name), _, _) => out.push((name, layer, field)),
            (None, false, Some(name)) => out.push((name, layer, "collision_class".to_owned())),
            (None, _, _) => {}
        }
    }
    out
}

/// Checks one authored name: non-empty ASCII `[a-z0-9_]`.
fn check_name(definition: &str, field: &str, name: &str) -> SimResult<()> {
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if valid {
        Ok(())
    } else {
        Err(SimCoreError::CatalogCollisionClassName(Box::new(
            CollisionClassNameFault {
                definition: definition.to_owned(),
                field: field.to_owned(),
                name: name.to_owned(),
            },
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_in_sorted_order_after_the_built_in_classes() {
        let table = CollisionClassTable {
            names: vec!["a".into(), "b".into(), "tall".into()],
        };
        assert_eq!(table.resolve("a"), Some(CollisionClass(3)));
        assert_eq!(table.resolve("b"), Some(CollisionClass(4)));
        assert_eq!(table.resolve("tall"), Some(CollisionClass(5)));
        assert_eq!(table.resolve("ground"), None);
    }

    #[test]
    fn names_outside_the_alphabet_are_refused() {
        for bad in ["", "Tall", "tall walker", "tall-walker", "é"] {
            assert!(
                matches!(
                    check_name("u", "collision_class", bad),
                    Err(SimCoreError::CatalogCollisionClassName(_))
                ),
                "{bad:?}"
            );
        }
        for good in ["tall_walker", "x", "class_2", "ground"] {
            assert_eq!(check_name("u", "collision_class", good), Ok(()), "{good:?}");
        }
    }

    #[test]
    fn the_largest_table_fits_a_byte() {
        assert_eq!(MAX_AUTHORED_CLASSES, 252);
        let names: Vec<String> = (0..MAX_AUTHORED_CLASSES)
            .map(|i| format!("c{i:03}"))
            .collect();
        let table = CollisionClassTable { names };
        assert_eq!(table.resolve("c251"), Some(CollisionClass(u8::MAX - 1)));
    }
}
