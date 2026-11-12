//! Graph rules of the spell catalog and the per-node facts (plan D-15, D-17, D-18, D-19).
//!
//! Runs on the flattened arena: cycles (through children, `Use` and `ApplyStatus` → status
//! effects), tree size, synchronous depth, nested searches and target kinds. Facts are
//! computed after the cycle check, so every recursion here terminates; the tree-size check
//! comes first and bounds every chain to [`MAX_TREE_NODES`] nodes, which bounds recursion depth.

use super::{
    CatalogError, EffectFacts, EffectNode, SpellCatalog, MAX_EFFECT_DEPTH, MAX_TREE_NODES,
};
use sim_types::{EffectAnchor, EffectId, SimScalar};

/// One edge of the effect graph: the target and whether it runs in the same walk.
#[derive(Copy, Clone)]
struct Edge {
    to: EffectId,
    sync: bool,
}

fn index(id: EffectId) -> usize {
    usize::from(id.0)
}

/// Every edge leaving `id`: children, and for `ApplyStatus` the status's effects (the initial
/// effect runs in the same walk, the periodic one starts its own).
fn edges(catalog: &SpellCatalog, id: EffectId) -> Vec<Edge> {
    let mut out = Vec::new();
    let Some(node) = catalog.node(id) else {
        return out;
    };
    let mut push = |to: Option<EffectId>, sync: bool| {
        if let Some(to) = to {
            out.push(Edge { to, sync });
        }
    };
    match node.kind {
        EffectNode::Set { first, len } => {
            for i in 0..len {
                push(catalog.set_child(first, i), true);
            }
        }
        EffectNode::Search { child, .. } => push(Some(child), true),
        EffectNode::Persistent {
            initial, periodic, ..
        } => {
            push(initial, true);
            push(periodic, false);
        }
        EffectNode::LaunchMissile { impact, .. } => push(Some(impact), false),
        EffectNode::Displace { on_land, .. } => push(on_land, false),
        EffectNode::ApplyStatus { status } => {
            if let Some(status) = catalog.status(status) {
                push(status.initial_effect, true);
                push(status.periodic_effect, false);
            }
        }
        EffectNode::Damage { .. }
        | EffectNode::RemoveStatus { .. }
        | EffectNode::ModifyVitals { .. }
        | EffectNode::CancelOrders { .. } => {}
    }
    out
}

/// Checks every graph rule and fills `catalog.facts`.
pub(super) fn validate_and_compute_facts(catalog: &mut SpellCatalog) -> Result<(), CatalogError> {
    let adjacency: Vec<Vec<Edge>> = (0..catalog.nodes.len())
        .map(|i| edges(catalog, EffectId(u16::try_from(i).unwrap_or(u16::MAX))))
        .collect();
    find_cycle(catalog, &adjacency)?;
    check_tree_sizes(catalog, &adjacency)?;
    let facts = compute_facts(catalog, &adjacency);
    for (i, fact) in facts.iter().enumerate() {
        if usize::from(fact.sync_depth) > MAX_EFFECT_DEPTH {
            return Err(CatalogError::EffectTreeTooDeep {
                root: catalog.labels[i].clone(),
                depth: usize::from(fact.sync_depth),
            });
        }
    }
    check_nested_searches(catalog, &adjacency)?;
    catalog.facts = facts;
    for status in &catalog.statuses {
        for effect in [status.initial_effect, status.periodic_effect]
            .into_iter()
            .flatten()
        {
            check_target_kinds(catalog, effect, true, &status.spec.name)?;
        }
    }
    Ok(())
}

/// Iterative three-colour depth-first search; reports the first cycle found as a label path.
fn find_cycle(catalog: &SpellCatalog, adjacency: &[Vec<Edge>]) -> Result<(), CatalogError> {
    const WHITE: u8 = 0;
    const GREY: u8 = 1;
    const BLACK: u8 = 2;
    let mut colour = vec![WHITE; adjacency.len()];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for start in 0..adjacency.len() {
        if colour[start] != WHITE {
            continue;
        }
        colour[start] = GREY;
        stack.push((start, 0));
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            if let Some(edge) = adjacency[node].get(*next) {
                *next += 1;
                let to = index(edge.to);
                match colour[to] {
                    WHITE => {
                        colour[to] = GREY;
                        stack.push((to, 0));
                    }
                    GREY => {
                        let from = stack.iter().position(|(n, _)| *n == to).unwrap_or(0);
                        let mut path: Vec<String> = stack[from..]
                            .iter()
                            .map(|(n, _)| catalog.labels[*n].clone())
                            .collect();
                        path.push(catalog.labels[to].clone());
                        return Err(CatalogError::EffectCycle { path });
                    }
                    _ => {}
                }
            } else {
                colour[node] = BLACK;
                stack.pop();
            }
        }
    }
    Ok(())
}

/// Every named node reaches at most [`MAX_TREE_NODES`] distinct nodes, status effects counted.
fn check_tree_sizes(catalog: &SpellCatalog, adjacency: &[Vec<Edge>]) -> Result<(), CatalogError> {
    let mut seen = vec![false; adjacency.len()];
    let mut queue = Vec::new();
    let mut roots: Vec<EffectId> = catalog.by_name.values().copied().collect();
    roots.sort_unstable();
    roots.dedup();
    for root in roots {
        seen.fill(false);
        queue.clear();
        queue.push(index(root));
        seen[index(root)] = true;
        let mut count = 0_usize;
        while let Some(node) = queue.pop() {
            count += 1;
            for edge in &adjacency[node] {
                let to = index(edge.to);
                if !seen[to] {
                    seen[to] = true;
                    queue.push(to);
                }
            }
        }
        if count > MAX_TREE_NODES {
            return Err(CatalogError::EffectTreeTooLarge {
                root: catalog.labels[index(root)].clone(),
                nodes: count,
            });
        }
    }
    Ok(())
}

/// No search reaches another search in the same walk (D-17), so one scratch buffer suffices.
fn check_nested_searches(
    catalog: &SpellCatalog,
    adjacency: &[Vec<Edge>],
) -> Result<(), CatalogError> {
    let mut memo = vec![SearchMemo::Unknown; adjacency.len()];
    for (i, node) in catalog.nodes.iter().enumerate() {
        if let EffectNode::Search { child, .. } = node.kind {
            if let Some(inner) = first_sync_search(catalog, adjacency, index(child), &mut memo) {
                return Err(CatalogError::NestedSearch {
                    outer: catalog.labels[i].clone(),
                    inner: catalog.labels[inner].clone(),
                });
            }
        }
    }
    Ok(())
}

fn first_sync_search(
    catalog: &SpellCatalog,
    adjacency: &[Vec<Edge>],
    node: usize,
    memo: &mut [SearchMemo],
) -> Option<usize> {
    if let SearchMemo::Known(known) = memo[node] {
        return known;
    }
    let found = if matches!(catalog.nodes[node].kind, EffectNode::Search { .. }) {
        Some(node)
    } else {
        adjacency[node]
            .iter()
            .filter(|edge| edge.sync)
            .find_map(|edge| first_sync_search(catalog, adjacency, index(edge.to), memo))
    };
    memo[node] = SearchMemo::Known(found);
    found
}

/// Memo of [`first_sync_search`]: not yet computed, or the first search found (if any).
#[derive(Copy, Clone)]
enum SearchMemo {
    Unknown,
    Known(Option<usize>),
}

fn needs_unit(catalog: &SpellCatalog, node: usize) -> bool {
    let flat = catalog.nodes[node];
    flat.require.is_some()
        || match flat.kind {
            EffectNode::Persistent { anchor, .. }
            | EffectNode::LaunchMissile { to: anchor, .. } => anchor == EffectAnchor::TargetUnit,
            EffectNode::Damage { .. }
            | EffectNode::ApplyStatus { .. }
            | EffectNode::RemoveStatus { .. }
            | EffectNode::ModifyVitals { .. }
            | EffectNode::CancelOrders { .. }
            | EffectNode::Displace { .. } => true,
            EffectNode::Set { .. } | EffectNode::Search { .. } => false,
        }
}

/// Facts of every node, by memoized post-order over the (acyclic) graph.
fn compute_facts(catalog: &SpellCatalog, adjacency: &[Vec<Edge>]) -> Vec<EffectFacts> {
    let mut done = vec![false; adjacency.len()];
    let mut facts = vec![EffectFacts::default(); adjacency.len()];
    let mut sync_radius: Vec<Option<SimScalar>> = vec![None; adjacency.len()];
    for node in 0..adjacency.len() {
        fact(
            catalog,
            adjacency,
            node,
            &mut done,
            &mut facts,
            &mut sync_radius,
        );
    }
    facts
}

fn max_radius(a: Option<SimScalar>, b: Option<SimScalar>) -> Option<SimScalar> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

fn fact(
    catalog: &SpellCatalog,
    adjacency: &[Vec<Edge>],
    node: usize,
    done: &mut [bool],
    facts: &mut [EffectFacts],
    sync_radius: &mut [Option<SimScalar>],
) {
    if done[node] {
        return;
    }
    let mut depth = 0_u8;
    let mut radius = None;
    let mut needs_origin = false;
    let mut tether = false;
    for edge in &adjacency[node] {
        let to = index(edge.to);
        fact(catalog, adjacency, to, done, facts, sync_radius);
        needs_origin |= facts[to].needs_origin;
        if edge.sync {
            depth = depth.max(facts[to].sync_depth);
            radius = max_radius(radius, sync_radius[to]);
            tether |= facts[to].reaches_tether;
        }
    }
    let kind = catalog.nodes[node].kind;
    let mut area_radius = None;
    let targeting_radius = match kind {
        EffectNode::Search { radius: own, .. } => {
            radius = max_radius(radius, Some(own));
            Some(own)
        }
        EffectNode::Set { first, len } => (0..len)
            .filter_map(|i| catalog.set_child(first, i))
            .find_map(|child| facts[index(child)].targeting_radius),
        EffectNode::Persistent {
            anchor,
            initial,
            periodic,
            ..
        } => {
            let walk_radius = |id: Option<EffectId>| id.and_then(|id| sync_radius[index(id)]);
            if anchor == EffectAnchor::TargetPoint {
                area_radius = max_radius(walk_radius(initial), walk_radius(periodic));
            }
            if anchor == EffectAnchor::TargetUnit {
                tether |= periodic.is_some_and(|id| facts[index(id)].reaches_tether);
            }
            let ring = |id: Option<EffectId>| id.and_then(|id| facts[index(id)].targeting_radius);
            ring(initial).or_else(|| ring(periodic))
        }
        EffectNode::LaunchMissile { impact, .. } => facts[index(impact)].targeting_radius,
        EffectNode::Displace { tether: own, .. } => {
            needs_origin = true;
            tether |= own;
            None
        }
        EffectNode::Damage { .. }
        | EffectNode::ApplyStatus { .. }
        | EffectNode::RemoveStatus { .. }
        | EffectNode::ModifyVitals { .. }
        | EffectNode::CancelOrders { .. } => None,
    };
    sync_radius[node] = radius;
    facts[node] = EffectFacts {
        needs_unit: needs_unit(catalog, node),
        sync_depth: depth.saturating_add(1),
        area_radius,
        targeting_radius,
        needs_origin,
        reaches_tether: tether,
    };
    done[node] = true;
}

/// Propagates the target kind from `root`, reached with a unit when `unit` is true, and fails
/// on the first node that needs a unit but is reached with a point (D-15). Status effects are
/// not followed: they always run on a unit and are checked from each status.
pub(super) fn check_target_kinds(
    catalog: &SpellCatalog,
    root: EffectId,
    unit: bool,
    reached_from: &str,
) -> Result<(), CatalogError> {
    // Bit 0: reached with a point; bit 1: reached with a unit.
    let mut seen = vec![0_u8; catalog.nodes.len()];
    let mut stack: Vec<(EffectId, bool, Option<EffectId>)> = vec![(root, unit, None)];
    while let Some((id, unit, parent)) = stack.pop() {
        let bit = if unit { 2 } else { 1 };
        let Some(slot) = seen.get_mut(index(id)) else {
            continue;
        };
        if *slot & bit != 0 {
            continue;
        }
        *slot |= bit;
        if !unit && needs_unit(catalog, index(id)) {
            return Err(CatalogError::EffectTargetMismatch {
                effect: catalog.labels[index(id)].clone(),
                reached_from: parent
                    .map_or(reached_from, |p| catalog.labels[index(p)].as_str())
                    .to_owned(),
            });
        }
        let Some(node) = catalog.node(id) else {
            continue;
        };
        let here = Some(id);
        match node.kind {
            EffectNode::Set { first, len } => {
                for i in 0..len {
                    if let Some(child) = catalog.set_child(first, i) {
                        stack.push((child, unit, here));
                    }
                }
            }
            EffectNode::Search { child, .. } => stack.push((child, true, here)),
            EffectNode::Persistent {
                anchor,
                initial,
                periodic,
                ..
            } => {
                let child_unit = anchor == EffectAnchor::TargetUnit;
                for child in [initial, periodic].into_iter().flatten() {
                    stack.push((child, child_unit, here));
                }
            }
            EffectNode::LaunchMissile { to, impact, .. } => {
                stack.push((impact, to == EffectAnchor::TargetUnit, here));
            }
            EffectNode::Displace { on_land, .. } => {
                if let Some(child) = on_land {
                    stack.push((child, true, here));
                }
            }
            EffectNode::Damage { .. }
            | EffectNode::ApplyStatus { .. }
            | EffectNode::RemoveStatus { .. }
            | EffectNode::ModifyVitals { .. }
            | EffectNode::CancelOrders { .. } => {}
        }
    }
    Ok(())
}
