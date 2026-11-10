//! The mesh segment predicate (plan D-7): may a disc sweep along a straight segment?
//!
//! # Responsibility
//! Decides whether a disc of radius `r` centred on the segment `[a, b]` stays inside triangles
//! the request may enter and at least `r` from every edge that blocks the request.
//!
//! # How it fits
//! The route search validates every funnel leg with it, and WP9 makes it the predicate of
//! `Pathfinder::segment_is_passable`, so routes and movement keep agreeing.
//!
//! # Invariants
//! - Exact: geometry is compared on the Q16 grid in `i128` (`predicates.rs`); the radius rounds
//!   up to the grid, so rounding can only refuse, never admit.
//! - Complete: a blocking edge closer than `r` is found. The segment and the short path from it to
//!   that edge cross only edges within `r` of the segment, so a flood from the start triangle
//!   across non-blocking edges within `r` reaches it (or a nearer blocking edge first).
//! - Allocation-free in steady state: the flood reuses [`SegmentScratch`].

use sim_types::{SimScalar, SimVec2};

use super::predicates::{segments_within, QPoint, Q_BITS};
use super::{next, prev, Access, NavMesh};

/// Reusable flood state for [`segment_clear`].
#[derive(Debug, Default)]
pub struct SegmentScratch {
    stamp: Vec<u32>,
    generation: u32,
    stack: Vec<u32>,
}

impl SegmentScratch {
    fn begin(&mut self, triangles: usize) {
        if self.stamp.len() != triangles {
            self.stamp.clear();
            self.stamp.resize(triangles, 0);
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.stamp.fill(0);
            self.generation = 1;
        }
        self.stack.clear();
    }

    fn seen(&self, t: u32) -> bool {
        self.stamp[t as usize] == self.generation
    }

    fn visit(&mut self, t: u32) -> bool {
        let slot = &mut self.stamp[t as usize];
        if *slot == self.generation {
            return false;
        }
        *slot = self.generation;
        self.stack.push(t);
        true
    }
}

/// A radius in Q16 units, rounded up.
pub(crate) fn radius_q(radius: SimScalar) -> i64 {
    let shift = 32 - Q_BITS;
    let raw = radius.raw().max(0);
    (raw >> shift) + i64::from(raw & ((1 << shift) - 1) != 0)
}

/// Whether a disc of `radius` world units may sweep from `a` to `b` under `access`.
pub(crate) fn segment_clear(
    mesh: &NavMesh,
    access: &Access<'_>,
    scratch: &mut SegmentScratch,
    a: SimVec2,
    b: SimVec2,
    radius: SimScalar,
) -> bool {
    let Some(start) = mesh.locate(a) else {
        return false;
    };
    if mesh.locate(b).is_none() || !access.open(start) {
        return false;
    }
    let (qa, qb) = (QPoint::from_sim(a), QPoint::from_sim(b));
    let r = radius_q(radius);
    scratch.begin(mesh.triangle_count());
    scratch.visit(start);
    while let Some(t) = scratch.stack.pop() {
        for k in 0..3 {
            let n = mesh.triangle_neighbors(t)[k];
            let blocks = mesh.edge_blocks(t, k, |x| access.open(x));
            // PERFORMANCE RATIONALE:
            // Why: The exact capsule test is the predicate's main cost, and every triangle's edge
            // back to the one it was reached from would repeat it with nothing to learn.
            // What: An open edge to a triangle already reached is skipped before the test; a
            // blocking edge is always tested, so the answer and the triangles reached are
            // unchanged.
            // Trade-off: One cheap edge lookup before the test instead of after it.
            if !blocks && scratch.seen(n) {
                continue;
            }
            let (u, v) = (
                mesh.corner(t, next(k)).to_q(),
                mesh.corner(t, prev(k)).to_q(),
            );
            if !segments_within(qa, qb, u, v, r) {
                continue;
            }
            if blocks {
                return false;
            }
            scratch.visit(n);
        }
    }
    true
}
