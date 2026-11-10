//! Constrained Delaunay triangulation of lattice points with axis-aligned constraint runs.
//!
//! # Responsibility
//! Builds the triangle topology of the navigation mesh: incremental Delaunay insertion with
//! Lawson flips (Bowyer–Watson's cavity expressed as flips), constraint recovery by Sloan's edge
//! flipping (1993), and a final Lawson pass that makes every unconstrained edge locally Delaunay.
//!
//! # How it fits
//! `mod.rs` collects the lattice vertices and constraint runs from the map and hands them here;
//! it then labels the triangles. This module knows nothing about terrain.
//!
//! # Invariants
//! - Triangles are counter-clockwise. `adjacent[t][k]` is the triangle across the edge opposite
//!   local vertex `k`, that is the edge `(v[k + 1], v[k + 2])`, or [`NONE`] on the hull.
//! - `kinds[t][k]` is the constraint kind of the same edge and equals the kind stored on the
//!   neighbour's side. Constrained edges are never flipped.
//! - The input is a pure function of the map, every loop runs in index order, and every
//!   predicate is exact, so the triangulation is a pure function of its input (charter §3.1).
//! - The bounding rectangle's four corners are the first four points; every other point lies in
//!   the closed rectangle, so insertion never needs a super-triangle.

use std::collections::VecDeque;

use super::predicates::{incircle, orient2d, LatticePoint};
use super::EdgeKind;

/// Sentinel for "no triangle" in adjacency and lookup tables.
pub const NONE: u32 = u32::MAX;

/// Where a point lies relative to the triangulation.
enum Location {
    /// Strictly inside the triangle.
    Inside(u32),
    /// On the edge opposite local vertex `k` of the triangle.
    OnEdge(u32, usize),
    /// On an existing vertex.
    OnVertex,
}

/// Mutable triangle topology during construction.
pub struct Triangulation {
    /// Vertex positions; the first four are the rectangle corners.
    pub points: Vec<LatticePoint>,
    /// Counter-clockwise vertex indices of each triangle.
    pub triangles: Vec<[u32; 3]>,
    /// Neighbour across the edge opposite each local vertex.
    pub adjacent: Vec<[u32; 3]>,
    /// Constraint kind of the edge opposite each local vertex.
    pub kinds: Vec<[EdgeKind; 3]>,
    /// One triangle incident to each vertex.
    pub vertex_triangle: Vec<u32>,
    /// Scratch for rotations around a vertex.
    around: Vec<u32>,
}

#[inline]
fn next(k: usize) -> usize {
    (k + 1) % 3
}

#[inline]
fn prev(k: usize) -> usize {
    (k + 2) % 3
}

#[inline]
fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(NONE)
}

impl Triangulation {
    /// Two triangles covering the rectangle `[0, width] x [0, height]` in lattice cells.
    #[must_use]
    pub fn rectangle(width: i32, height: i32) -> Self {
        let points = vec![
            LatticePoint::new(0, 0),
            LatticePoint::new(width, 0),
            LatticePoint::new(width, height),
            LatticePoint::new(0, height),
        ];
        Self {
            points,
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            adjacent: vec![[NONE, 1, NONE], [NONE, NONE, 0]],
            kinds: vec![[EdgeKind::Free; 3]; 2],
            vertex_triangle: vec![0, 0, 0, 1],
            around: Vec::new(),
        }
    }

    fn point(&self, t: u32, k: usize) -> LatticePoint {
        self.points[self.triangles[t as usize][k] as usize]
    }

    /// Local index of vertex `v` in triangle `t`, if present.
    #[must_use]
    pub fn local(&self, t: u32, v: u32) -> Option<usize> {
        self.triangles[t as usize].iter().position(|&w| w == v)
    }

    /// Walks from `hint` to a triangle containing `p` (visibility walk).
    // PERFORMANCE RATIONALE:
    // Why: Points arrive in row order, so the triangle of the previous point is next to the new
    // one; a scan of every triangle would make construction quadratic.
    // What: A visibility walk from the last triangle, starting its edge tests at a rotating index
    // so that it cannot cycle on degenerate inputs; a step cap falls back to a scan.
    // Trade-off: Two code paths, in exchange for O(1) expected location during construction.
    fn locate(&self, p: LatticePoint, hint: u32) -> Location {
        let mut t = hint;
        let cap = 4 * self.triangles.len() + 16;
        'walk: for step in 0..cap {
            for j in 0..3 {
                let k = (j + step) % 3;
                let (a, b) = (self.point(t, next(k)), self.point(t, prev(k)));
                if orient2d(a, b, p) < 0 {
                    let n = self.adjacent[t as usize][k];
                    if n == NONE {
                        break 'walk;
                    }
                    t = n;
                    continue 'walk;
                }
            }
            return self.classify(t, p);
        }
        for t in 0..self.triangles.len() {
            let t = index(t);
            if (0..3).all(|k| orient2d(self.point(t, next(k)), self.point(t, prev(k)), p) >= 0) {
                return self.classify(t, p);
            }
        }
        Location::OnVertex
    }

    fn classify(&self, t: u32, p: LatticePoint) -> Location {
        let mut zero_count = 0;
        let mut zero_edge = 0;
        for k in 0..3 {
            if orient2d(self.point(t, next(k)), self.point(t, prev(k)), p) == 0 {
                zero_count += 1;
                zero_edge = k;
            }
        }
        match zero_count {
            0 => Location::Inside(t),
            1 => Location::OnEdge(t, zero_edge),
            _ => Location::OnVertex,
        }
    }

    /// Inserts the lattice point `p`, keeping the triangulation Delaunay. Returns the triangle
    /// to use as the next hint. A duplicate point is ignored.
    pub fn insert_point(&mut self, p: LatticePoint, hint: u32) -> u32 {
        match self.locate(p, hint) {
            Location::OnVertex => hint,
            Location::Inside(t) => {
                let v = self.push_point(p);
                let created = self.split_triangle(t, v);
                self.legalize(&created);
                self.vertex_triangle[v as usize]
            }
            Location::OnEdge(t, k) => {
                let v = self.push_point(p);
                let created = self.split_edge(t, k, v);
                self.legalize(&created);
                self.vertex_triangle[v as usize]
            }
        }
    }

    fn push_point(&mut self, p: LatticePoint) -> u32 {
        self.points.push(p);
        self.vertex_triangle.push(NONE);
        index(self.points.len() - 1)
    }

    fn push_triangle(&mut self, v: [u32; 3], adj: [u32; 3], kinds: [EdgeKind; 3]) -> u32 {
        self.triangles.push(v);
        self.adjacent.push(adj);
        self.kinds.push(kinds);
        index(self.triangles.len() - 1)
    }

    /// Points the neighbour `n`'s link that referred to `old` at `new` instead.
    fn relink(&mut self, n: u32, old: u32, new: u32) {
        if n == NONE {
            return;
        }
        for slot in &mut self.adjacent[n as usize] {
            if *slot == old {
                *slot = new;
                return;
            }
        }
    }

    fn touch(&mut self, t: u32) {
        for v in self.triangles[t as usize] {
            self.vertex_triangle[v as usize] = t;
        }
    }

    /// Splits `t` at its interior vertex `p`; returns the three `(triangle, k)` edges opposite `p`.
    fn split_triangle(&mut self, t: u32, p: u32) -> [(u32, usize); 3] {
        let [a, b, c] = self.triangles[t as usize];
        let [na, nb, nc] = self.adjacent[t as usize];
        let [ka, kb, kc] = self.kinds[t as usize];
        let t1 = index(self.triangles.len());
        let t2 = t1 + 1;
        self.triangles[t as usize] = [a, b, p];
        self.adjacent[t as usize] = [t1, t2, nc];
        self.kinds[t as usize] = [EdgeKind::Free, EdgeKind::Free, kc];
        self.push_triangle([b, c, p], [t2, t, na], [EdgeKind::Free, EdgeKind::Free, ka]);
        self.push_triangle([c, a, p], [t, t1, nb], [EdgeKind::Free, EdgeKind::Free, kb]);
        self.relink(na, t, t1);
        self.relink(nb, t, t2);
        for tri in [t, t1, t2] {
            self.touch(tri);
        }
        [(t, 2), (t1, 2), (t2, 2)]
    }

    /// Splits the edge opposite local vertex `k` of `t` at `p`; returns the new edges opposite `p`.
    fn split_edge(&mut self, t: u32, k: usize, p: u32) -> Vec<(u32, usize)> {
        let tv = self.triangles[t as usize];
        let (a, b, c) = (tv[k], tv[next(k)], tv[prev(k)]);
        let split_kind = self.kinds[t as usize][k];
        let (t_nb, t_nc) = (
            self.adjacent[t as usize][next(k)],
            self.adjacent[t as usize][prev(k)],
        );
        let (k_b, k_c) = (
            self.kinds[t as usize][next(k)],
            self.kinds[t as usize][prev(k)],
        );
        let n = self.adjacent[t as usize][k];
        let t1 = index(self.triangles.len());
        if n == NONE {
            self.triangles[t as usize] = [a, b, p];
            self.adjacent[t as usize] = [NONE, t1, t_nc];
            self.kinds[t as usize] = [split_kind, EdgeKind::Free, k_c];
            self.push_triangle(
                [a, p, c],
                [NONE, t_nb, t],
                [split_kind, k_b, EdgeKind::Free],
            );
            self.relink(t_nb, t, t1);
            self.touch(t);
            self.touch(t1);
            return vec![(t, 2), (t1, 1)];
        }
        let j = self.local(n, b).map_or(0, next);
        // In `n`, the vertex opposite the shared edge is `d`, followed by `c` then `b`.
        let d = self.triangles[n as usize][j];
        let (n_nc, n_nb) = (
            self.adjacent[n as usize][next(j)],
            self.adjacent[n as usize][prev(j)],
        );
        let (nk_c, nk_b) = (
            self.kinds[n as usize][next(j)],
            self.kinds[n as usize][prev(j)],
        );
        let n1 = t1 + 1;
        self.triangles[t as usize] = [a, b, p];
        self.adjacent[t as usize] = [n, t1, t_nc];
        self.kinds[t as usize] = [split_kind, EdgeKind::Free, k_c];
        self.push_triangle([a, p, c], [n1, t_nb, t], [split_kind, k_b, EdgeKind::Free]);
        self.triangles[n as usize] = [d, p, b];
        self.adjacent[n as usize] = [t, n_nc, n1];
        self.kinds[n as usize] = [split_kind, nk_c, EdgeKind::Free];
        self.push_triangle([d, c, p], [t1, n, n_nb], [split_kind, EdgeKind::Free, nk_b]);
        self.relink(t_nb, t, t1);
        self.relink(n_nb, n, n1);
        for tri in [t, t1, n, n1] {
            self.touch(tri);
        }
        vec![(t, 2), (t1, 1), (n, 1), (n1, 2)]
    }

    /// Flips the unconstrained edge opposite local vertex `k` of `t`.
    ///
    /// With `t = (p, q, s)` and its neighbour `(o, s, q)`, the result is `t = (p, q, o)` and
    /// `n = (p, o, s)`. Returns `n`.
    fn flip(&mut self, t: u32, k: usize) -> u32 {
        let tv = self.triangles[t as usize];
        let (p, q, s) = (tv[k], tv[next(k)], tv[prev(k)]);
        let n = self.adjacent[t as usize][k];
        let j = self.local(n, q).map_or(0, next);
        let o = self.triangles[n as usize][j];
        let (t_nq, t_ns) = (
            self.adjacent[t as usize][next(k)],
            self.adjacent[t as usize][prev(k)],
        );
        let (t_kq, t_ks) = (
            self.kinds[t as usize][next(k)],
            self.kinds[t as usize][prev(k)],
        );
        let (n_ns, n_nq) = (
            self.adjacent[n as usize][next(j)],
            self.adjacent[n as usize][prev(j)],
        );
        let (n_ks, n_kq) = (
            self.kinds[n as usize][next(j)],
            self.kinds[n as usize][prev(j)],
        );
        self.triangles[t as usize] = [p, q, o];
        self.adjacent[t as usize] = [n_ns, n, t_ns];
        self.kinds[t as usize] = [n_ks, EdgeKind::Free, t_ks];
        self.triangles[n as usize] = [p, o, s];
        self.adjacent[n as usize] = [n_nq, t_nq, t];
        self.kinds[n as usize] = [n_kq, t_kq, EdgeKind::Free];
        self.relink(n_ns, n, t);
        self.relink(t_nq, t, n);
        self.touch(t);
        self.touch(n);
        n
    }

    /// Whether the unconstrained edge opposite `k` in `t` violates the empty-circle property.
    fn is_illegal(&self, t: u32, k: usize) -> bool {
        let n = self.adjacent[t as usize][k];
        if n == NONE || self.kinds[t as usize][k] != EdgeKind::Free {
            return false;
        }
        let q = self.triangles[t as usize][next(k)];
        let j = self.local(n, q).map_or(0, next);
        let o = self.points[self.triangles[n as usize][j] as usize];
        incircle(self.point(t, 0), self.point(t, 1), self.point(t, 2), o) > 0
    }

    /// Lawson flips from the given edges until every touched edge is locally Delaunay.
    fn legalize(&mut self, edges: &[(u32, usize)]) {
        let mut stack: Vec<(u32, usize)> = edges.to_vec();
        while let Some((t, k)) = stack.pop() {
            if self.is_illegal(t, k) {
                let n = self.flip(t, k);
                // After the flip the new vertex is local 0 of both triangles.
                stack.push((t, 0));
                stack.push((n, 0));
            }
        }
    }

    /// Makes every unconstrained edge locally Delaunay (the constrained Delaunay property).
    pub fn make_delaunay(&mut self) {
        let mut stack: Vec<(u32, usize)> = (0..self.triangles.len())
            .flat_map(|t| (0..3).map(move |k| (index(t), k)))
            .collect();
        while let Some((t, k)) = stack.pop() {
            if self.is_illegal(t, k) {
                let n = self.flip(t, k);
                stack.extend([(t, 0), (t, 2), (n, 0), (n, 1)]);
            }
        }
    }

    /// Collects the triangles around vertex `v` into `self.around`, in rotational order.
    fn collect_around(&mut self, v: u32) {
        self.around.clear();
        let start = self.vertex_triangle[v as usize];
        if start == NONE {
            return;
        }
        let mut t = start;
        loop {
            self.around.push(t);
            let i = self.local(t, v).unwrap_or(0);
            let n = self.adjacent[t as usize][next(i)];
            if n == NONE || n == start {
                if n == NONE {
                    let mut back = start;
                    loop {
                        let i = self.local(back, v).unwrap_or(0);
                        let m = self.adjacent[back as usize][prev(i)];
                        if m == NONE {
                            break;
                        }
                        self.around.push(m);
                        back = m;
                    }
                }
                return;
            }
            t = n;
        }
    }

    /// Finds the edge `(u, w)`: a triangle containing it and the local index opposite it.
    pub fn find_edge(&mut self, u: u32, w: u32) -> Option<(u32, usize)> {
        self.collect_around(u);
        for &t in &self.around {
            if let (Some(_), Some(j)) = (self.local(t, u), self.local(t, w)) {
                let i = self.local(t, u).unwrap_or(0);
                return Some((t, 3 - i - j));
            }
        }
        None
    }

    /// Sets the constraint kind of the edge opposite `k` in `t` on both of its sides.
    fn set_kind(&mut self, t: u32, k: usize, kind: EdgeKind) {
        self.kinds[t as usize][k] = kind;
        let n = self.adjacent[t as usize][k];
        if n != NONE {
            let q = self.triangles[t as usize][next(k)];
            if let Some(j) = self.local(n, q).map(next) {
                self.kinds[n as usize][j] = kind;
            }
        }
    }

    /// Inserts the constraint segment between vertices `a` and `b` (Sloan 1993).
    ///
    /// Requires that no vertex lies in the open segment and that it crosses no constraint.
    /// Returns `false` if that requirement was violated and the edge could not be recovered.
    pub fn insert_constraint(&mut self, a: u32, b: u32, kind: EdgeKind) -> bool {
        if let Some((t, k)) = self.find_edge(a, b) {
            self.set_kind(t, k, kind);
            return true;
        }
        let mut crossing = self.crossing_edges(a, b);
        let (pa, pb) = (self.points[a as usize], self.points[b as usize]);
        let mut guard = 0usize;
        let limit = 64 * (crossing.len() + 1) * (crossing.len() + 1);
        while let Some((u, w)) = crossing.pop_front() {
            guard += 1;
            if guard > limit {
                return false;
            }
            let Some((t, k)) = self.find_edge(u, w) else {
                continue;
            };
            let n = self.adjacent[t as usize][k];
            if n == NONE {
                return false;
            }
            let tv = self.triangles[t as usize];
            let (p, q, s) = (tv[k], tv[next(k)], tv[prev(k)]);
            let o = self.triangles[n as usize][self.local(n, q).map_or(0, next)];
            let (pp, pq, ps, po) = (
                self.points[p as usize],
                self.points[q as usize],
                self.points[s as usize],
                self.points[o as usize],
            );
            let convex = orient2d(pp, po, pq).signum() * orient2d(pp, po, ps).signum() < 0;
            if !convex {
                crossing.push_back((u, w));
                continue;
            }
            self.flip(t, k);
            if orient2d(pa, pb, pp).signum() * orient2d(pa, pb, po).signum() < 0 {
                crossing.push_back((p, o));
            }
        }
        match self.find_edge(a, b) {
            Some((t, k)) => {
                self.set_kind(t, k, kind);
                true
            }
            None => false,
        }
    }

    /// The edges properly crossed by the open segment `(a, b)`, in order from `a`.
    fn crossing_edges(&mut self, a: u32, b: u32) -> VecDeque<(u32, u32)> {
        let mut result = VecDeque::new();
        let (pa, pb) = (self.points[a as usize], self.points[b as usize]);
        self.collect_around(a);
        let mut current = None;
        for &t in &self.around {
            let i = self.local(t, a).unwrap_or(0);
            let (u, w) = (
                self.triangles[t as usize][next(i)],
                self.triangles[t as usize][prev(i)],
            );
            let (pu, pw) = (self.points[u as usize], self.points[w as usize]);
            if orient2d(pa, pu, pb) > 0 && orient2d(pa, pw, pb) < 0 {
                current = Some((t, i, w, u));
                break;
            }
        }
        let Some((mut t, mut k, mut left, mut right)) = current else {
            return result;
        };
        for _ in 0..self.triangles.len() {
            result.push_back((left, right));
            let n = self.adjacent[t as usize][k];
            if n == NONE {
                break;
            }
            let Some(&o) = self.triangles[n as usize]
                .iter()
                .find(|&&v| v != left && v != right)
            else {
                break;
            };
            if o == b {
                break;
            }
            if orient2d(pa, pb, self.points[o as usize]) > 0 {
                left = o;
            } else {
                right = o;
            }
            let (Some(li), Some(ri)) = (self.local(n, left), self.local(n, right)) else {
                break;
            };
            k = 3 - li - ri;
            t = n;
        }
        result
    }
}
