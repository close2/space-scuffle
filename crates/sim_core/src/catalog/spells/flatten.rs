//! Interning and flattening of the authored spell tables into the arena (plan D-18).
//!
//! Statuses take ids in name order. Named effect nodes take ids in name order, a named node
//! that is only a `Use` alias takes none and resolves to its target, and inline nodes follow,
//! depth-first pre-order under their named parent. Every per-node value bound of D-17 is
//! checked here, where the authored label is at hand; graph rules are in `checks`.

use super::{
    min_missile_speed, CatalogError, EffectNode, FlatNode, ResolvedFilter, ResolvedStatus,
    SpellCatalog, MAX_EFFECT_NODES, MAX_FLIGHT_TICKS, MAX_MISSILE_SPEED, MAX_PERSISTENT_COUNT,
    MAX_SEARCH_COUNT, MAX_SEARCH_RADIUS, MAX_SET_CHILDREN,
};
use sim_types::{
    AttributeBonuses, DamageDef, EffectDef, EffectId, EffectNodeDef, ModifyVitalsDef,
    PersistentDef, SearchDef, SetDef, SimScalar, StatusId, StatusSpec, TargetFilter, TargetPlane,
    VitalChange,
};
use std::collections::BTreeMap;

/// Builds the arena from parsed definitions, checking keys, duplicates and value bounds.
pub(super) fn flatten(
    statuses: Vec<StatusSpec>,
    effects: Vec<EffectDef>,
) -> Result<SpellCatalog, CatalogError> {
    let status_by_name = intern_statuses(&statuses)?;
    let mut named: BTreeMap<String, EffectNodeDef> = BTreeMap::new();
    for def in effects {
        if named.contains_key(&def.name) {
            return Err(CatalogError::DuplicateEffect { name: def.name });
        }
        named.insert(def.name, def.node);
    }
    let mut builder = Builder {
        status_by_name,
        nodes: Vec::new(),
        children: Vec::new(),
        labels: Vec::new(),
        by_name: BTreeMap::new(),
    };
    for (name, node) in &named {
        if !matches!(node, EffectNodeDef::Use(_)) {
            let id = builder.allocate(name.clone())?;
            builder.by_name.insert(name.clone(), id);
        }
    }
    for name in named.keys() {
        let id = resolve_alias(&named, &builder.by_name, name)?;
        builder.by_name.insert(name.clone(), id);
    }
    for (name, node) in &named {
        if matches!(node, EffectNodeDef::Use(_)) {
            continue;
        }
        let id = builder.by_name[name];
        let mut counter = 0_usize;
        builder.fill(node, id, name, &mut counter)?;
    }
    let mut sorted: Vec<StatusSpec> = statuses;
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let statuses = sorted
        .into_iter()
        .map(|spec| resolve_status(spec, &builder.by_name))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SpellCatalog {
        facts: vec![super::EffectFacts::default(); builder.nodes.len()],
        nodes: builder
            .nodes
            .into_iter()
            .map(|node| node.unwrap_or(PLACEHOLDER))
            .collect(),
        children: builder.children,
        labels: builder.labels,
        by_name: builder.by_name,
        statuses,
        status_by_name: builder.status_by_name,
    })
}

/// Never left in a built catalog: every allocated slot is filled before `flatten` returns.
const PLACEHOLDER: FlatNode = FlatNode {
    kind: EffectNode::CancelOrders {
        force_unburrow: false,
    },
    require: None,
};

fn intern_statuses(statuses: &[StatusSpec]) -> Result<BTreeMap<String, StatusId>, CatalogError> {
    let mut names: Vec<&str> = statuses.iter().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    for pair in names.windows(2) {
        if pair[0] == pair[1] {
            return Err(CatalogError::DuplicateStatus {
                name: pair[0].to_owned(),
            });
        }
    }
    let mut map = BTreeMap::new();
    for (index, name) in names.into_iter().enumerate() {
        let id = u16::try_from(index).map_err(|_| CatalogError::TooManyEffectNodes)?;
        map.insert(name.to_owned(), StatusId(id));
    }
    Ok(map)
}

/// Follows a chain of `Use` aliases from `name` to a node with an id.
fn resolve_alias(
    named: &BTreeMap<String, EffectNodeDef>,
    ids: &BTreeMap<String, EffectId>,
    name: &str,
) -> Result<EffectId, CatalogError> {
    let mut path = vec![name.to_owned()];
    let mut current = name;
    loop {
        if let Some(id) = ids.get(current) {
            return Ok(*id);
        }
        let Some(EffectNodeDef::Use(next)) = named.get(current) else {
            return Err(CatalogError::UnknownEffect {
                name: current.to_owned(),
                referenced_by: path.last().cloned().unwrap_or_default(),
            });
        };
        if !named.contains_key(next.as_str()) {
            return Err(CatalogError::UnknownEffect {
                name: next.clone(),
                referenced_by: current.to_owned(),
            });
        }
        if path.iter().any(|seen| seen == next) {
            path.push(next.clone());
            return Err(CatalogError::EffectCycle { path });
        }
        path.push(next.clone());
        current = next;
    }
}

fn resolve_status(
    spec: StatusSpec,
    by_name: &BTreeMap<String, EffectId>,
) -> Result<ResolvedStatus, CatalogError> {
    let invalid = |reason| CatalogError::InvalidStatus {
        status: spec.name.clone(),
        reason,
    };
    if spec.duration_ticks <= SimScalar::ZERO {
        return Err(invalid("duration_ticks must be positive"));
    }
    if spec.move_speed_factor < SimScalar::ZERO || spec.move_speed_factor > SimScalar::ONE {
        return Err(invalid("move_speed_factor must be between 0 and 1"));
    }
    if spec.range_floor < SimScalar::ZERO {
        return Err(invalid("range_floor must not be negative"));
    }
    match (spec.period_ticks, &spec.periodic_effect) {
        (Some(period), Some(_)) if period < SimScalar::ONE => {
            return Err(CatalogError::InvalidPeriod {
                definition: spec.name.clone(),
                reason: "period_ticks must be at least 1",
            })
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(CatalogError::InvalidPeriod {
                definition: spec.name.clone(),
                reason: "period_ticks is set exactly when periodic_effect is",
            })
        }
        _ => {}
    }
    let lookup = |key: &Option<String>| {
        key.as_ref()
            .map(|name| {
                by_name
                    .get(name)
                    .copied()
                    .ok_or_else(|| CatalogError::UnknownEffect {
                        name: name.clone(),
                        referenced_by: spec.name.clone(),
                    })
            })
            .transpose()
    };
    let initial_effect = lookup(&spec.initial_effect)?;
    let periodic_effect = lookup(&spec.periodic_effect)?;
    Ok(ResolvedStatus {
        spec,
        initial_effect,
        periodic_effect,
    })
}

struct Builder {
    status_by_name: BTreeMap<String, StatusId>,
    nodes: Vec<Option<FlatNode>>,
    children: Vec<EffectId>,
    labels: Vec<String>,
    by_name: BTreeMap<String, EffectId>,
}

impl Builder {
    fn allocate(&mut self, label: String) -> Result<EffectId, CatalogError> {
        if self.nodes.len() >= MAX_EFFECT_NODES {
            return Err(CatalogError::TooManyEffectNodes);
        }
        let id = u16::try_from(self.nodes.len()).map_err(|_| CatalogError::TooManyEffectNodes)?;
        self.nodes.push(None);
        self.labels.push(label);
        Ok(EffectId(id))
    }

    /// The id of a child: a resolved name for `Use`, else a fresh inline node filled at once
    /// (pre-order).
    fn child(
        &mut self,
        def: &EffectNodeDef,
        root: &str,
        counter: &mut usize,
    ) -> Result<EffectId, CatalogError> {
        if let EffectNodeDef::Use(name) = def {
            return self
                .by_name
                .get(name)
                .copied()
                .ok_or_else(|| CatalogError::UnknownEffect {
                    name: name.clone(),
                    referenced_by: root.to_owned(),
                });
        }
        *counter += 1;
        let id = self.allocate(format!("{root}/{counter}"))?;
        self.fill(def, id, root, counter)?;
        Ok(id)
    }

    fn optional_child(
        &mut self,
        def: Option<&EffectNodeDef>,
        root: &str,
        counter: &mut usize,
    ) -> Result<Option<EffectId>, CatalogError> {
        def.map(|def| self.child(def, root, counter)).transpose()
    }

    fn status(&self, name: &str, referenced_by: &str) -> Result<StatusId, CatalogError> {
        self.status_by_name
            .get(name)
            .copied()
            .ok_or_else(|| CatalogError::UnknownStatus {
                name: name.to_owned(),
                referenced_by: referenced_by.to_owned(),
            })
    }

    fn filter(&self, filter: &TargetFilter, label: &str) -> Result<ResolvedFilter, CatalogError> {
        Ok(ResolvedFilter {
            alliance: filter.alliance,
            ground: filter.planes.contains(&TargetPlane::Ground),
            air: filter.planes.contains(&TargetPlane::Air),
            structures: filter.structures,
            without_status: filter
                .without_status
                .as_deref()
                .map(|name| self.status(name, label))
                .transpose()?,
        })
    }

    fn require(
        &self,
        require: Option<&TargetFilter>,
        label: &str,
    ) -> Result<Option<ResolvedFilter>, CatalogError> {
        require.map(|filter| self.filter(filter, label)).transpose()
    }

    /// Writes the node of `def` into slot `id`, allocating its inline children first.
    fn fill(
        &mut self,
        def: &EffectNodeDef,
        id: EffectId,
        root: &str,
        counter: &mut usize,
    ) -> Result<(), CatalogError> {
        let label = self.labels[usize::from(id.0)].clone();
        let node = match def {
            // A named alias never reaches here, and an inline `Use` resolves in `child`.
            EffectNodeDef::Use(name) => {
                return Err(CatalogError::UnknownEffect {
                    name: name.clone(),
                    referenced_by: root.to_owned(),
                })
            }
            EffectNodeDef::Set(set) => self.set_node(set, label, root, counter)?,
            EffectNodeDef::Search(search) => self.search_node(search, label, root, counter)?,
            EffectNodeDef::Persistent(persistent) => {
                self.persistent_node(persistent, label, root, counter)?
            }
            EffectNodeDef::LaunchMissile(missile) => {
                if missile.speed < min_missile_speed()
                    || missile.speed > SimScalar::from_i32(MAX_MISSILE_SPEED)
                {
                    return Err(CatalogError::InvalidMissileSpeed { effect: label });
                }
                let require = self.require(missile.require.as_ref(), &label)?;
                let impact = self.child(&missile.impact, root, counter)?;
                FlatNode {
                    kind: EffectNode::LaunchMissile {
                        to: missile.to,
                        speed: missile.speed,
                        view: missile.view,
                        impact,
                    },
                    require,
                }
            }
            EffectNodeDef::Damage(damage) => self.damage_node(damage, label)?,
            EffectNodeDef::ApplyStatus(apply) => FlatNode {
                kind: EffectNode::ApplyStatus {
                    status: self.status(&apply.status, &label)?,
                },
                require: self.require(apply.require.as_ref(), &label)?,
            },
            EffectNodeDef::RemoveStatus(remove) => FlatNode {
                kind: EffectNode::RemoveStatus {
                    status: self.status(&remove.status, &label)?,
                },
                require: self.require(remove.require.as_ref(), &label)?,
            },
            EffectNodeDef::ModifyVitals(vitals) => self.vitals_node(vitals, label)?,
            EffectNodeDef::CancelOrders(cancel) => FlatNode {
                kind: EffectNode::CancelOrders {
                    force_unburrow: cancel.force_unburrow,
                },
                require: self.require(cancel.require.as_ref(), &label)?,
            },
            EffectNodeDef::Displace(displace) => {
                if displace.flight_ticks == 0 || displace.flight_ticks > MAX_FLIGHT_TICKS {
                    return Err(CatalogError::InvalidDisplacement { effect: label });
                }
                let require = self.require(displace.require.as_ref(), &label)?;
                let on_land = self.optional_child(displace.on_land.as_deref(), root, counter)?;
                FlatNode {
                    kind: EffectNode::Displace {
                        flight_ticks: displace.flight_ticks,
                        tether: displace.tether,
                        on_land,
                    },
                    require,
                }
            }
        };
        self.nodes[usize::from(id.0)] = Some(node);
        Ok(())
    }

    fn set_node(
        &mut self,
        set: &SetDef,
        label: String,
        root: &str,
        counter: &mut usize,
    ) -> Result<FlatNode, CatalogError> {
        if set.children.is_empty() {
            return Err(CatalogError::EmptySet { effect: label });
        }
        if set.children.len() > MAX_SET_CHILDREN {
            return Err(CatalogError::SetTooWide {
                effect: label,
                len: set.children.len(),
            });
        }
        let require = self.require(set.require.as_ref(), &label)?;
        let ids = set
            .children
            .iter()
            .map(|child| self.child(child, root, counter))
            .collect::<Result<Vec<_>, _>>()?;
        let first =
            u16::try_from(self.children.len()).map_err(|_| CatalogError::TooManyEffectNodes)?;
        let len = u8::try_from(ids.len()).map_err(|_| CatalogError::TooManyEffectNodes)?;
        self.children.extend(ids);
        Ok(FlatNode {
            kind: EffectNode::Set { first, len },
            require,
        })
    }

    fn search_node(
        &mut self,
        search: &SearchDef,
        label: String,
        root: &str,
        counter: &mut usize,
    ) -> Result<FlatNode, CatalogError> {
        if search.radius <= SimScalar::ZERO
            || search.radius > SimScalar::from_i32(MAX_SEARCH_RADIUS)
        {
            return Err(CatalogError::InvalidSearch {
                effect: label,
                reason: "radius must be above 0 and at most 16",
            });
        }
        if search
            .max_count
            .is_some_and(|count| count == 0 || count > MAX_SEARCH_COUNT)
        {
            return Err(CatalogError::InvalidSearch {
                effect: label,
                reason: "max_count must be between 1 and 256",
            });
        }
        let filter = self.filter(&search.filter, &label)?;
        let require = self.require(search.require.as_ref(), &label)?;
        let child = self.child(&search.child, root, counter)?;
        Ok(FlatNode {
            kind: EffectNode::Search {
                radius: search.radius,
                filter,
                max_count: search.max_count,
                child,
            },
            require,
        })
    }

    fn persistent_node(
        &mut self,
        persistent: &PersistentDef,
        label: String,
        root: &str,
        counter: &mut usize,
    ) -> Result<FlatNode, CatalogError> {
        if persistent.initial.is_none() && persistent.periodic.is_none() {
            return Err(CatalogError::EmptyPersistent { effect: label });
        }
        let invalid = |reason| CatalogError::InvalidPeriod {
            definition: label.clone(),
            reason,
        };
        if persistent.count > MAX_PERSISTENT_COUNT {
            return Err(invalid("count must be at most 1024"));
        }
        if persistent.periodic.is_some() != (persistent.count >= 1) {
            return Err(invalid("periodic is set exactly when count is at least 1"));
        }
        if persistent.count >= 1 && persistent.period_ticks < SimScalar::ONE {
            return Err(invalid("period_ticks must be at least 1"));
        }
        let require = self.require(persistent.require.as_ref(), &label)?;
        let initial = self.optional_child(persistent.initial.as_deref(), root, counter)?;
        let periodic = self.optional_child(persistent.periodic.as_deref(), root, counter)?;
        Ok(FlatNode {
            kind: EffectNode::Persistent {
                anchor: persistent.anchor,
                initial,
                periodic,
                period_ticks: persistent.period_ticks,
                count: persistent.count,
            },
            require,
        })
    }

    fn damage_node(&self, damage: &DamageDef, label: String) -> Result<FlatNode, CatalogError> {
        if damage.amount < SimScalar::ZERO
            || damage
                .attribute_bonus
                .iter()
                .any(|(_, bonus)| *bonus < SimScalar::ZERO)
        {
            return Err(CatalogError::NegativeDamage { effect: label });
        }
        let attribute_bonus = damage.attribute_bonus.iter().fold(
            AttributeBonuses::NONE,
            |acc, (attribute, bonus)| {
                acc.with(*attribute, acc.get(*attribute).saturating_add(*bonus))
            },
        );
        Ok(FlatNode {
            kind: EffectNode::Damage {
                amount: damage.amount,
                attribute_bonus,
                responses: damage.responses,
            },
            require: self.require(damage.require.as_ref(), &label)?,
        })
    }

    fn vitals_node(
        &self,
        vitals: &ModifyVitalsDef,
        label: String,
    ) -> Result<FlatNode, CatalogError> {
        let changes = [vitals.life, vitals.shields, vitals.energy];
        if changes.iter().all(Option::is_none) {
            return Err(CatalogError::InvalidVitalChange {
                effect: label,
                reason: "names no vital",
            });
        }
        if changes
            .iter()
            .flatten()
            .any(|change| !fraction_in_bounds(*change))
        {
            return Err(CatalogError::InvalidVitalChange {
                effect: label,
                reason: "change_fraction must be between -1 and 1",
            });
        }
        Ok(FlatNode {
            kind: EffectNode::ModifyVitals {
                life: vitals.life,
                shields: vitals.shields,
                energy: vitals.energy,
            },
            require: self.require(vitals.require.as_ref(), &label)?,
        })
    }
}

fn fraction_in_bounds(change: VitalChange) -> bool {
    change.change_fraction >= -SimScalar::ONE && change.change_fraction <= SimScalar::ONE
}
