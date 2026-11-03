//! Whether the constraints declared on a Bevy schedule order one system before another.
//!
//! # Responsibility
//! Answers "does `first` always run before `second`?" from the constraints the client declares
//! (`.before`, `.after`, `.chain`, `.in_set` with ordered sets), not from the order one run
//! happened to take. A reversed or deleted constraint therefore fails the test that asks, even when
//! the reversal makes no schedule cycle and Bevy accepts the schedule.
//!
//! # How it fits
//! Bevy keeps two graphs per schedule: `hierarchy` (a set contains a system or a set) and
//! `dependency` (a system or set runs before another). Bevy flattens them into system-to-system
//! edges only while it builds the executable schedule, and that flattened graph is private in
//! `bevy_ecs` 0.15. This module rebuilds the same relation from the two public graphs.
//!
//! # Model
//! Every node `n` is split into `start(n)` and `end(n)`:
//! - a system runs between its two halves: `start(n) -> end(n)`;
//! - a set spans its members: `start(set) -> start(member)` and `end(member) -> end(set)`, and
//!   `start(set) -> end(set)` so an empty set still passes order through, as Bevy's flattening does;
//! - a dependency edge `a -> b` means `a` ends before `b` starts: `end(a) -> start(b)`.
//!
//! `first` runs before `second` exactly when `start(second)` is reachable from `end(first)`. A path
//! can only climb out of a set through `end` halves and descend into one through `start` halves,
//! so it must cross at least one dependency edge: membership alone never orders two systems.
//!
//! # Invariants
//! - Read only: the schedule is never modified.
//! - Systems are named by the full path Bevy reports through `System::name`, and a name must
//!   identify exactly one system in the schedule, so an ambiguous question fails loudly.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bevy::ecs::schedule::{NodeId, Schedule};

/// Which half of a node a vertex of the reachability graph stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Half {
    /// The moment the node begins.
    Start,
    /// The moment the node has finished.
    End,
}

/// A vertex of the reachability graph: one half of a system or set.
type Vertex = (NodeId, Half);

/// The node of the one system named `name` in `schedule`, an initialized schedule.
///
/// # Panics
/// If the schedule is not initialized, or if no system or more than one has that name.
pub fn system_node(schedule: &Schedule, name: &str) -> NodeId {
    let matches: Vec<NodeId> = schedule
        .systems()
        .expect("the schedule must be initialized before its systems are read")
        .filter(|(_, system)| system.name() == name)
        .map(|(node, _)| node)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "exactly one system must be named {name} in {:?}",
        schedule.label()
    );
    matches[0]
}

/// Whether the declared constraints of `schedule` make system `first` finish before `second`
/// starts. Both are full system paths, as [`system_node`] takes them.
pub fn runs_before(schedule: &Schedule, first: &str, second: &str) -> bool {
    let from = (system_node(schedule, first), Half::End);
    let to = (system_node(schedule, second), Half::Start);
    let edges = reachability_edges(schedule);
    let mut seen = BTreeSet::from([from]);
    let mut queue = VecDeque::from([from]);
    while let Some(vertex) = queue.pop_front() {
        if vertex == to {
            return true;
        }
        for next in edges.get(&vertex).into_iter().flatten() {
            if seen.insert(*next) {
                queue.push_back(*next);
            }
        }
    }
    false
}

/// The edges of the split-node model described in the module documentation.
fn reachability_edges(schedule: &Schedule) -> BTreeMap<Vertex, Vec<Vertex>> {
    let graph = schedule.graph();
    let mut edges: BTreeMap<Vertex, Vec<Vertex>> = BTreeMap::new();
    let mut add = |from: Vertex, to: Vertex| edges.entry(from).or_default().push(to);
    let nodes: BTreeSet<NodeId> = graph
        .dependency()
        .graph()
        .nodes()
        .chain(graph.hierarchy().graph().nodes())
        .collect();
    for node in nodes {
        add((node, Half::Start), (node, Half::End));
    }
    for (set, member, ()) in graph.hierarchy().graph().all_edges() {
        add((set, Half::Start), (member, Half::Start));
        add((member, Half::End), (set, Half::End));
    }
    for (before, after, ()) in graph.dependency().graph().all_edges() {
        add((before, Half::End), (after, Half::Start));
    }
    edges
}
