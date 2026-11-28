//! Straight diagonal cliffs: the full cut of a staircase's convex corner, which cuts its cell
//! along the anti-diagonal through the low cells' corners, and the geometry its dual cells share
//! (plan `docs/plans/2026-12-01-straight-diagonal-cliffs.md` D-1 to D-5, D-7 and D-9; owner
//! question Q0136, answer (c)).
//!
//! # Responsibility
//! A 45° staircase cliff alternates convex corners `C` and concave corners `V`. The only straight
//! line along it that enters no low cell passes through the `V`s, the anti-diagonal of every high
//! step cell `X`. This module decides which cells are cut that way ([`full_cut`], [`Cut`]) and
//! builds what the dual cells around a cut draw from it:
//! - **The predicate (D-1)** is the simulation's: `sim_map::diagonal` (plan
//!   `docs/plans/2026-12-02-diagonal-cliff-walkability.md` D-1) reads the 3x3 block of cells
//!   around `X` (`X` convex at the corner, the three cells behind the opposite corner high, at
//!   least one neighbouring corner a concave corner with ground below, and no ramp in the block)
//!   and gives a cell at most one cut, over the thresholds `max(L₁, L₂) < t ≤ t_hi` (its cut
//!   run). This module builds its [`Cut`] from that [`DiagonalCut`], so the drawing and the
//!   simulation read one rule.
//! - **The dual cell's table ([`DualCuts`])** reads the 4x4 block of cells around a dual cell once
//!   and records each corner cell's cut. Relative to a cut, a dual cell is the one at its corner
//!   `C` (the cut cell's bit is cleared), at one end `P` of its foot (the chamfer passes through
//!   the dual cell's centre), or at the corner `O` opposite `C` (its floor stops at the rim top).
//! - **The shared geometry**: the cut cell's centre column, the rim-top crossings of the dual
//!   edges ([`Cut::rim_point`]), the crossing columns and their toes, and the anchors on the
//!   rim-top line. Each is a pure function of the cut, so every dual cell, in any chunk, builds
//!   the same bits. The dual cell at `O` emits the wall between the two crossing columns through
//!   the centre column ([`emit_cut_chains`]); a dual cell at `P` emits the wall from a crossing
//!   column to its junction ([`emit_cut_run`]).
//!
//! # Invariants
//! - **W1:** the chamfer's foot runs through the low cells' corners and every row, rim and floor
//!   of it lies behind the foot: in the cut cell and in the plateau cells' corners beside it,
//!   below their floors, never in a low cell.
//! - **Watertight:** every vertex two dual cells share (a column at the cut cell's centre, a
//!   rim-top crossing of a dual edge, an anchor) is computed by one function from the cut alone.
//! - **No T-junction:** a floor vertex on a rim-top line or a foot is a wall vertex bit for bit.
//! - **No ramp:** a cut has no ramp in its block, so no dual cell it touches has a ramp corner, and
//!   the ramp rules R1 to R6 never meet a cut (D-7).

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "cell coordinates, levels and the ±1 components of directions stay far below the \
              range of f32 and i32, and block and corner indices are below 4"
)]

use bevy::prelude::*;
use sim_map::diagonal::{marching_level, BlockLevels, DiagonalCut};
use sim_map::grid::GameMap;

use super::{
    chamfer_mitre, cliff_level_height, corner_direction, edge_setback, emit_dual_floor,
    emit_wall_strip, wall_profile_rows, DualCell, DualCorner, DualVertexRef, EdgeMidpoint,
    FloorPolygon, MarchingCase, MeshStats, MesherScratch, ProfileFrame, WallProfileRow,
    FLOOR_POLYGON_CAPACITY, MARCHING_CASES, RIM_WIDTH,
};

/// The full cut of cell `cell` at its corner in direction `dir` (plan D-1), over the thresholds
/// `lb + 1 ..= t_hi`: `sim_map`'s [`DiagonalCut`] in the mesher's integer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cut {
    /// The cut cell `X`, in simulation cell coordinates.
    pub(super) cell: (i32, i32),
    /// The corner direction `(dx, dy)`, from `X`'s centre towards its corner `C` on the low side.
    pub(super) dir: (i32, i32),
    /// `X`'s marching level.
    pub(super) la: i32,
    /// The higher of the levels of `X`'s two neighbours towards `C`, `L₁ = X + (dx, 0)` and
    /// `L₂ = X + (0, dy)`: the chamfer's foot stands at `H(lb)`.
    pub(super) lb: i32,
    /// The highest threshold at which the cut holds.
    pub(super) t_hi: i32,
    /// The foot's integer constant `k` ([`DiagonalCut::foot_constant`]).
    foot: i64,
}

impl From<DiagonalCut> for Cut {
    fn from(cut: DiagonalCut) -> Self {
        Self {
            cell: (cut.cell.0 as i32, cut.cell.1 as i32),
            dir: (i32::from(cut.dir.0), i32::from(cut.dir.1)),
            la: i32::from(cut.la),
            lb: i32::from(cut.lb),
            t_hi: i32::from(cut.t_hi),
            foot: cut.foot_constant(),
        }
    }
}

impl Cut {
    /// Whether the cut holds at threshold `t`.
    pub(super) fn active(&self, t: i32) -> bool {
        self.lb < t && t <= self.t_hi
    }

    /// The unit normal `n = dir/√2`, from the high side towards the low side.
    pub(super) fn normal(&self) -> Vec2 {
        Vec2::new(self.dir.0 as f32, self.dir.1 as f32) * std::f32::consts::FRAC_1_SQRT_2
    }

    /// The cut cell's centre, world XZ: it lies on the foot.
    pub(super) fn centre(&self) -> Vec2 {
        Vec2::new(self.cell.0 as f32 + 0.5, self.cell.1 as f32 + 0.5)
    }

    /// How far behind the foot the high floor at threshold `tb` stops, world units: the setback
    /// of a ground edge, and the rim's width where the floor is the cut cell's top (as
    /// `DualCell::floor_inset` on an axis edge).
    pub(super) fn inset(&self, tb: i32) -> f32 {
        let rim = if self.la == tb { RIM_WIDTH } else { 0.0 };
        edge_setback(self.lb, false) + rim
    }

    /// Whether the floor at threshold `tb` meets the cut's rim top.
    pub(super) fn rim(&self, tb: i32) -> bool {
        self.la == tb
    }

    /// The constant `c` of the line `dx·x + dy·y = c` that lies `inset` behind the foot.
    fn line_constant(&self, inset: f32) -> f32 {
        self.foot as f32 - std::f32::consts::SQRT_2 * inset
    }

    /// Where the line `inset` behind the foot crosses the axis-aligned line `x = e`
    /// (`vertical`) or `y = e`, world XZ, in closed form: every dual cell that meets the crossing
    /// computes the same bits (D-4).
    pub(super) fn rim_point(&self, inset: f32, vertical: bool, e: f32) -> Vec2 {
        let (dx, dy) = (self.dir.0 as f32, self.dir.1 as f32);
        let c = self.line_constant(inset);
        if vertical {
            Vec2::new(e, (c - dx * e) * dy)
        } else {
            Vec2::new((c - dy * e) * dx, e)
        }
    }

    /// The signed distance-like side of `p` against the line `inset` behind the foot, in units of
    /// `√2` world units: negative behind the line (on the plateau), positive beyond it.
    fn side(&self, inset: f32, p: Vec2) -> f32 {
        self.dir.0 as f32 * p.x + self.dir.1 as f32 * p.y - self.line_constant(inset)
    }

    /// The rim-top row of the cut cell's centre column: the anchor its rim-top line carries in
    /// the dual cell at `O`. The same expression as the rim-top row of `wall_profile_rows`.
    fn anchor(&self) -> Vec2 {
        self.centre() + self.normal() * -(edge_setback(self.lb, false) + RIM_WIDTH)
    }

    /// The column at the cut cell's centre (D-3): frame `(centre, n, n)`, the cut's levels, built
    /// over the whole cut run so both dual cells that end a strip there build the same rows, and
    /// restricted to the heights of the dual cell's run `ta..=tb`.
    fn centre_column(&self, ta: i32, tb: i32) -> Vec<WallProfileRow> {
        let frame = ProfileFrame {
            p: self.centre(),
            offset: self.normal(),
            facing: self.normal(),
        };
        restrict(
            wall_profile_rows(frame, self.la, self.lb, self.lb + 1, self.t_hi, false),
            ta,
            tb,
        )
    }

    /// The column where the rim-top line crosses the dual edge at `crossing` (D-4): its rim-top
    /// row is the crossing itself, and its other rows stand along `n` from its toe
    /// `crossing + n·inset` on the foot, with the cut's levels over the cut run.
    fn crossing_column(&self, crossing: Vec2, ta: i32, tb: i32) -> Vec<WallProfileRow> {
        let frame = ProfileFrame {
            p: self.toe(crossing),
            offset: self.normal(),
            facing: self.normal(),
        };
        let mut rows = wall_profile_rows(frame, self.la, self.lb, self.lb + 1, self.t_hi, false);
        if let Some(rim_top) = rows.first_mut().filter(|_| self.la == self.t_hi) {
            rim_top.xz = crossing;
        }
        restrict(rows, ta, tb)
    }

    /// The toe of the crossing column at `crossing`, on the foot.
    fn toe(&self, crossing: Vec2) -> Vec2 {
        crossing + self.normal() * self.inset(self.t_hi)
    }
}

/// The rows of `rows` (a profile built over a cut run) within the heights of the run `ta..=tb`:
/// every level height inside a cut run is a canonical row of its profile, so no row is sampled
/// and every dual cell that cuts the profile at a run boundary keeps the same rows.
fn restrict(rows: Vec<WallProfileRow>, ta: i32, tb: i32) -> Vec<WallProfileRow> {
    let (bottom, top) = (
        cliff_level_height(ta - 1) - 1e-4,
        cliff_level_height(tb) + 1e-4,
    );
    rows.into_iter()
        .filter(|r| r.y >= bottom && r.y <= top)
        .collect()
}

/// Whether cell `cell` has a full cut at its corner in direction `dir` (`(dx, dy)`, each `±1`)
/// at threshold `t` (plan `docs/plans/2026-12-01-straight-diagonal-cliffs.md` D-1): a convex
/// corner of a staircase whose straight diagonal is drawn through the low cells' corners. It reads
/// the 3x3 block of cells around `cell` through [`GameMap::diagonal_cut`].
#[must_use]
pub fn full_cut(map: &GameMap, cell: (i32, i32), dir: (i32, i32), t: i32) -> bool {
    let (Ok(x), Ok(y)) = (u32::try_from(cell.0), u32::try_from(cell.1)) else {
        return false;
    };
    map.diagonal_cut(x, y)
        .map(Cut::from)
        .is_some_and(|cut| cut.dir == dir && cut.active(t))
}

/// What a dual cell is to a cut of one of its corner cells (plan D-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    /// The dual cell centred on the cut's corner `C`: the cut cell's quadrant lies beyond the
    /// foot, so its bit is cleared.
    Corner,
    /// A dual cell centred on an end `P` of the foot: the chamfer runs from the cut cell's centre
    /// through the dual cell's centre. `low` is the corner index of the low cell `L` that shares
    /// the edge through `P`, `back` that of the high cell `B` behind the chamfer.
    Side { low: usize, back: usize },
    /// The dual cell centred on the corner `O` opposite `C`: its floor stops at the rim top.
    Opposite,
}

/// The cuts of a dual cell's four corner cells, read once from the 4x4 block of cells around it.
#[derive(Debug, Clone, Copy)]
pub(super) struct DualCuts {
    corners: [Option<(Cut, Role)>; 4],
}

/// The cases of one threshold of a dual cell that its runs group by: the marching case with the
/// bits of cut cells whose corner this dual cell is cleared, and which corners' cuts hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RunKey {
    /// The effective marching case index.
    pub(super) case: usize,
    /// Corners (marching-case bit order) whose cut holds at the threshold, in any role.
    pub(super) active: u8,
    /// Of those, the corners whose cut runs through this dual cell's centre ([`Role::Side`]).
    pub(super) side: u8,
}

impl RunKey {
    /// The key of a flat dual cell's low floor: case 0, no cut.
    pub(super) const FLAT: Self = Self {
        case: 0,
        active: 0,
        side: 0,
    };
}

/// The corner index of the corner horizontally adjacent to corner `k` (bottom-left, bottom-right,
/// top-right, top-left).
fn horizontal_neighbour(k: usize) -> usize {
    [1, 0, 3, 2][k % 4]
}

/// The corner index of the corner vertically adjacent to corner `k`.
fn vertical_neighbour(k: usize) -> usize {
    [3, 2, 1, 0][k % 4]
}

/// The edge midpoint between corner `k` and its neighbour `n`.
fn edge_between(k: usize, n: usize) -> EdgeMidpoint {
    match (k.min(n), k.max(n)) {
        (0, 1) => EdgeMidpoint::Bottom,
        (1, 2) => EdgeMidpoint::Right,
        (2, 3) => EdgeMidpoint::Top,
        _ => EdgeMidpoint::Left,
    }
}

impl DualCuts {
    /// No cut touches the dual cell.
    pub(super) const NONE: Self = Self { corners: [None; 4] };

    /// The cuts of dual cell `(di, dj)`'s corner cells. A dual cell with a ramp corner has none:
    /// every cell a cut touches lies in the cut cell's 3x3 block, which holds no ramp (D-7).
    // PERFORMANCE RATIONALE:
    // Why: Whether a corner cell is cut depends on its 3x3 block, so a dual cell depends on the
    // 4x4 block around it, and the four corners' blocks overlap.
    // What: The 16 cells are classified once into a stack array, and each corner's predicate reads
    // its 3x3 window of it: 16 cell lookups per dual cell instead of 36.
    // A flat dual cell can only be the `O` of a cut, whose two low cells are the outward edge
    // neighbours of one corner: 8 lookups rule that out first for nearly all flat ground.
    // Trade-off: A dual cell with a contour pays 12 lookups more than the 2x2 read of
    // `dual_corner_levels`, a flat one 8, on the chunk rebuild path only; nothing is allocated.
    pub(super) fn read(map: &GameMap, cell: &DualCell, di: u32, dj: u32) -> Self {
        if cell.ramp_mask != 0 {
            return Self::NONE;
        }
        let (Ok(x0), Ok(y0)) = (i32::try_from(di), i32::try_from(dj)) else {
            return Self::NONE;
        };
        if cell.lo == cell.hi && !may_be_opposite(map, (x0, y0), cell.lo) {
            return Self::NONE;
        }
        // Block cell (i, j) is map cell (x0 - 2 + i, y0 - 2 + j).
        let mut block = [[None; 4]; 4];
        for (j, row) in block.iter_mut().enumerate() {
            for (i, slot) in row.iter_mut().enumerate() {
                *slot = marching_level(
                    map,
                    i64::from(x0 - 2 + i as i32),
                    i64::from(y0 - 2 + j as i32),
                );
            }
        }
        let mut corners = [None; 4];
        for (k, slot) in corners.iter_mut().enumerate() {
            let o = corner_direction(k);
            // Corner k's cell, in block coordinates: (1 or 2, 1 or 2).
            let (ci, cj) = (usize::from(o.x > 0.0) + 1, usize::from(o.y > 0.0) + 1);
            let mut window: BlockLevels = [[None; 3]; 3];
            for (b, row) in window.iter_mut().enumerate() {
                for (a, level) in row.iter_mut().enumerate() {
                    *level = block[cj + b - 1][ci + a - 1];
                }
            }
            // A corner cell off the map (west or south of it) is the void, which no cut has.
            let (Ok(x), Ok(y)) = (
                u32::try_from(x0 - 2 + ci as i32),
                u32::try_from(y0 - 2 + cj as i32),
            ) else {
                continue;
            };
            *slot = DiagonalCut::of_block((x, y), &window)
                .map(Cut::from)
                .map(|cut| (cut, role(k, cut.dir)));
        }
        Self { corners }
    }

    /// Corner `k`'s cut and this dual cell's role for it, if its cell is cut.
    pub(super) fn corner(&self, k: usize) -> Option<(Cut, Role)> {
        self.corners[k % 4]
    }

    /// Whether the floor at `H(lo)` stops at the rim top of a cut whose opposite corner `O` this
    /// dual cell is: the cut holds at `lo` (a dual cell is only ever the `O` of a cut at
    /// thresholds up to its lowest corner, since the cut's backing cells are high there).
    pub(super) fn clips_floor_at(&self, lo: i32) -> bool {
        self.corners
            .iter()
            .flatten()
            .any(|(cut, role)| *role == Role::Opposite && cut.active(lo))
    }

    /// The run key of threshold `t` (D-5): the case of `cell` at `t` with the bits of cuts whose
    /// corner `C` this dual cell is cleared, and the corners whose cuts hold.
    pub(super) fn key(&self, cell: &DualCell, t: i32) -> RunKey {
        let mut key = RunKey {
            case: cell.case_for_t(t),
            active: 0,
            side: 0,
        };
        for (k, entry) in self.corners.iter().enumerate() {
            let Some((_, role)) = entry.filter(|(cut, _)| cut.active(t)) else {
                continue;
            };
            key.active |= 1 << k;
            match role {
                Role::Corner => key.case &= !(1 << k),
                Role::Side { .. } => key.side |= 1 << k,
                Role::Opposite => {}
            }
        }
        key
    }
}

/// Whether the flat dual cell centred on grid point `centre`, all of whose corners have level
/// `level`, can be the opposite corner `O` of a cut: some corner cell's two outward edge
/// neighbours are both lower plateaus or void (a cut's `L₁` and `L₂`, convex at `C`), and neither
/// is a ramp.
fn may_be_opposite(map: &GameMap, centre: (i32, i32), level: i32) -> bool {
    (0..4).any(|k| {
        let o = corner_direction(k);
        let (ox, oy) = (o.x as i32, o.y as i32);
        // Corner k's cell, and its two neighbours away from the dual cell.
        let cell = (centre.0 + (ox - 1) / 2, centre.1 + (oy - 1) / 2);
        let low = |x: i32, y: i32| {
            marching_level(map, i64::from(x), i64::from(y)).is_some_and(|l| i32::from(l) < level)
        };
        low(cell.0 + ox, cell.1) && low(cell.0, cell.1 + oy)
    })
}

/// What a dual cell is to a cut of its corner `k` in direction `dir` (plan D-2): the corner `C` when
/// `dir` points from the cell to the dual cell's centre, the opposite corner `O` when it points
/// away, an end `P` of the foot otherwise.
fn role(k: usize, dir: (i32, i32)) -> Role {
    let o = corner_direction(k);
    let (ox, oy) = (o.x as i32, o.y as i32);
    if dir == (-ox, -oy) {
        Role::Corner
    } else if dir == (ox, oy) {
        Role::Opposite
    } else if dir == (-ox, oy) {
        // P₁: L₁ = X + (dx, 0) is the horizontal neighbour, the backing cell X - (0, dy) the
        // vertical one.
        Role::Side {
            low: horizontal_neighbour(k),
            back: vertical_neighbour(k),
        }
    } else {
        Role::Side {
            low: vertical_neighbour(k),
            back: horizontal_neighbour(k),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Emission
// -------------------------------------------------------------------------------------------------

/// The `DualCorner` of corner index `k` (bottom-left, bottom-right, top-right, top-left).
fn dual_corner(k: usize) -> DualCorner {
    match k % 4 {
        0 => DualCorner::BottomLeft,
        1 => DualCorner::BottomRight,
        2 => DualCorner::TopRight,
        _ => DualCorner::TopLeft,
    }
}

/// The corner index of `c`.
fn corner_index(c: DualCorner) -> usize {
    match c {
        DualCorner::BottomLeft => 0,
        DualCorner::BottomRight => 1,
        DualCorner::TopRight => 2,
        DualCorner::TopLeft => 3,
    }
}

/// A region of the marching table rewritten for the cuts through a dual cell, on the stack.
#[derive(Debug, Clone, Copy)]
struct Refs {
    items: [DualVertexRef; FLOOR_POLYGON_CAPACITY],
    len: usize,
}

impl Refs {
    const EMPTY: Self = Self {
        items: [DualVertexRef::Centre; FLOOR_POLYGON_CAPACITY],
        len: 0,
    };

    /// Appends `r` unless it repeats the last vertex.
    fn push(&mut self, r: DualVertexRef) {
        if self.len > 0 && self.items[self.len - 1] == r {
            return;
        }
        debug_assert!(
            self.len < FLOOR_POLYGON_CAPACITY,
            "invariant: a rewritten region has at most {FLOOR_POLYGON_CAPACITY} vertices"
        );
        if self.len < FLOOR_POLYGON_CAPACITY {
            self.items[self.len] = r;
            self.len += 1;
        }
    }

    /// Rotates the vertices to start at the low cell `L` of a side cut, if one is listed: a low
    /// region with a chamfer's triangle added is star-shaped from `L`'s corner, which sees the
    /// whole foot, so the floor's triangle fan from it turns no triangle over.
    fn start_at_low_corner(&mut self, cuts: &DualCuts, key: RunKey) {
        let len = self.as_slice().len();
        let start = (0..4)
            .filter_map(|k| match cuts.corner(k) {
                Some((_, Role::Side { low, .. })) if key.side & (1 << k) != 0 => Some(low),
                _ => None,
            })
            .find_map(|low| {
                let corner = DualVertexRef::Corner(dual_corner(low));
                self.items[..len].iter().position(|&r| r == corner)
            });
        if let Some(start) = start {
            self.items[..len].rotate_left(start);
            self.len = len;
        }
    }

    /// The vertices, with a last vertex that repeats the first dropped.
    fn as_slice(&self) -> &[DualVertexRef] {
        let mut len = self.len;
        while len > 1 && self.items[len - 1] == self.items[0] {
            len -= 1;
        }
        &self.items[..len]
    }
}

/// The cut running through the dual cell's centre whose edge midpoint (between the cut cell and
/// its low cell `L`) is `m`, at a run with key `key`: its corner, cut and low and back corners.
fn side_cut_at(
    cuts: &DualCuts,
    key: RunKey,
    m: EdgeMidpoint,
) -> Option<(usize, Cut, usize, usize)> {
    (0..4).find_map(|k| match cuts.corner(k) {
        Some((cut, Role::Side { low, back }))
            if key.side & (1 << k) != 0 && edge_between(k, low) == m =>
        {
            Some((k, cut, low, back))
        }
        _ => None,
    })
}

/// The high region `part` of the effective case with every side cut's midpoint replaced by the
/// dual cell's centre: the chamfer runs from the cut cell's corner of the dual cell (its centre)
/// through the dual cell's centre (D-2).
fn high_refs(part: &[DualVertexRef], cuts: &DualCuts, key: RunKey) -> Refs {
    let mut refs = Refs::EMPTY;
    for &r in part {
        match r {
            DualVertexRef::Midpoint(m) if side_cut_at(cuts, key, m).is_some() => {
                refs.push(DualVertexRef::Centre);
            }
            _ => refs.push(r),
        }
    }
    refs
}

/// The low region `part` of the effective case with every side cut's midpoint replaced by the
/// cut cell's corner and the dual cell's centre, in the order that keeps the polygon simple: the
/// low side gains the triangle between the chamfer and the edge (D-2).
fn low_refs(part: &[DualVertexRef], cuts: &DualCuts, key: RunKey) -> Refs {
    let mut refs = Refs::EMPTY;
    let n = part.len();
    for (i, &r) in part.iter().enumerate() {
        let DualVertexRef::Midpoint(m) = r else {
            refs.push(r);
            continue;
        };
        let Some((k, _, low, _)) = side_cut_at(cuts, key, m) else {
            refs.push(r);
            continue;
        };
        let (cut_corner, centre) = (DualVertexRef::Corner(dual_corner(k)), DualVertexRef::Centre);
        if part[(i + 1) % n] == DualVertexRef::Corner(dual_corner(low)) {
            refs.push(centre);
            refs.push(cut_corner);
        } else {
            refs.push(cut_corner);
            refs.push(centre);
        }
    }
    refs
}

/// Where the rim-top line of side cut `cut` at corner `k` crosses the dual edge from the cut
/// cell's centre to its back corner `back`, for the floor at threshold `tb`.
fn crossing(cut: &Cut, k: usize, back: usize, tb: i32) -> Vec2 {
    let centre = cut.centre();
    if back == vertical_neighbour(k) {
        cut.rim_point(cut.inset(tb), true, centre.x)
    } else {
        cut.rim_point(cut.inset(tb), false, centre.y)
    }
}

/// Whether side cut `k`'s wall reaches this dual cell through a crossing column on the dual edge
/// to its back corner `back`: unless the back corner is a side cut of its own (two side cuts
/// meeting at the 90° apex keep their rim tops in the dual cell at `O`, and the apex's strips run
/// from the cut cells' centre columns).
fn has_crossing(back: usize, key: RunKey) -> bool {
    key.side & (1 << back) == 0
}

/// The wall profile at the dual cell's centre where a chamfer meets a chamfer or an axis wall
/// (D-3): the two walls' low-side normals, levels and cuts.
#[derive(Debug, Clone, Copy)]
struct Junction {
    first: Vec2,
    second: Vec2,
    la: i32,
    lb: i32,
    cuts: [Option<Cut>; 2],
}

impl Junction {
    /// The junction of segment `start → end` of a run with key `key` at threshold `tb`: a
    /// chamfer's side carries its cut's normal and levels, an axis wall's its edge's.
    fn of(cell: &DualCell, cuts: &DualCuts, key: RunKey, ends: [EdgeMidpoint; 2], tb: i32) -> Self {
        let half = |m: EdgeMidpoint| {
            if let Some((_, cut, _, _)) = side_cut_at(cuts, key, m) {
                (cut.normal(), cut.la, cut.lb, Some(cut))
            } else {
                let (la, lb) = cell.midpoint_levels(m, tb);
                (cell.midpoint_d(m, tb), la, lb, None)
            }
        };
        let (n1, la1, lb1, c1) = half(ends[0]);
        let (n2, la2, lb2, c2) = half(ends[1]);
        Self {
            first: n1,
            second: n2,
            la: la1.min(la2),
            lb: lb1.max(lb2),
            cuts: [c1, c2],
        }
    }

    fn mitre(&self) -> Vec2 {
        chamfer_mitre(self.first, self.second)
    }

    /// The high floor's vertex at the junction: the centre inset along the mitre by the wall's
    /// setback and rim, the junction profile's rim-top row.
    fn floor_point(&self, centre: Vec2, tb: i32) -> Vec2 {
        let rim = if self.la == tb { RIM_WIDTH } else { 0.0 };
        centre - self.mitre() * (edge_setback(self.lb, false) + rim)
    }

    /// The junction's profile rows for the run `ta..=tb`. Where two chamfers of different normals
    /// meet (the 90° apex), the rim-top row is the two rim-top lines' intersection in closed form,
    /// the vertex the dual cell at `O` clips its floor to.
    fn rows(&self, centre: Vec2, ta: i32, tb: i32) -> Vec<WallProfileRow> {
        let frame = ProfileFrame {
            p: centre,
            offset: self.mitre(),
            facing: (self.first + self.second).normalize(),
        };
        let mut rows = wall_profile_rows(frame, self.la, self.lb, ta, tb, false);
        if let ([Some(a), Some(b)], Some(rim_top)) = (self.cuts, rows.first_mut()) {
            if a.dir != b.dir && self.la == tb {
                rim_top.xz = apex(&a, a.inset(tb), &b, b.inset(tb));
            }
        }
        rows
    }
}

/// The intersection of the lines `inset_a` behind cut `a`'s foot and `inset_b` behind cut `b`'s,
/// whose normals are perpendicular: `M = [dir_a; dir_b]` has `M·Mᵀ = 2I`, so the point is
/// `Mᵀ·c / 2`. Symmetric in `a` and `b`, so every caller gets the same bits.
fn apex(a: &Cut, inset_a: f32, b: &Cut, inset_b: f32) -> Vec2 {
    let (ca, cb) = (a.line_constant(inset_a), b.line_constant(inset_b));
    let (ax, ay, bx, by) = (
        a.dir.0 as f32,
        a.dir.1 as f32,
        b.dir.0 as f32,
        b.dir.1 as f32,
    );
    Vec2::new(ax * ca + bx * cb, ay * ca + by * cb) * 0.5
}

/// One wall profile station along a contour through a dual cell with a chamfer.
#[derive(Debug, Clone, Copy)]
enum Station {
    /// An axis wall's edge midpoint (the mesher's own profile).
    Midpoint(EdgeMidpoint),
    /// The centre column of a side cut.
    CutCentre(Cut),
    /// A side cut's crossing column at this rim-top crossing.
    Crossing(Cut, Vec2),
    /// The junction at the dual cell's centre.
    Junction,
}

/// The stations of segment `start → end` of a run with key `key`, in contour order: a side cut's
/// end starts at its crossing column, where the strip of the dual cell at `O` ends (or at its
/// centre column at the apex), and runs to the junction at the dual cell's centre.
fn stations(cuts: &DualCuts, key: RunKey, ends: [EdgeMidpoint; 2]) -> ([Station; 3], usize) {
    let mut list = [Station::Junction; 3];
    let mut len = 0;
    let mut push = |s: Station| {
        list[len] = s;
        len += 1;
    };
    let end_station = |m: EdgeMidpoint| match side_cut_at(cuts, key, m) {
        Some((k, cut, _, back)) if has_crossing(back, key) => {
            Station::Crossing(cut, crossing(&cut, k, back, cut.t_hi))
        }
        Some((_, cut, _, _)) => Station::CutCentre(cut),
        None => Station::Midpoint(m),
    };
    push(end_station(ends[0]));
    push(Station::Junction);
    push(end_station(ends[1]));
    (list, len)
}

/// The profile rows of `station` for the run `ta..=tb`.
fn station_rows(
    cell: &DualCell,
    junction: &Junction,
    station: Station,
    ta: i32,
    tb: i32,
) -> Vec<WallProfileRow> {
    match station {
        Station::Midpoint(m) => cell.midpoint_profile(m, ta, tb),
        Station::CutCentre(cut) => cut.centre_column(ta, tb),
        Station::Crossing(cut, at) => cut.crossing_column(at, ta, tb),
        Station::Junction => junction.rows(cell.centre, ta, tb),
    }
}

/// Section 1 of `build_chunk_mesh` for one threshold run `ta..=tb` of a dual cell through whose
/// centre a chamfer runs (`key.side` is set): the effective case's high region with the
/// chamfers' triangles removed, as floor at `H(tb)` stopping at their rim tops, and the walls
/// along its contour, a chamfer's columns joined at the junction (D-2 to D-4).
#[expect(
    clippy::too_many_arguments,
    reason = "the mesher's emitters take the map, the dual cell and the output buffers alike"
)]
pub(super) fn emit_cut_run(
    map: &GameMap,
    cell: &DualCell,
    cuts: &DualCuts,
    key: RunKey,
    ta: i32,
    tb: i32,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    let mcase: &MarchingCase = &MARCHING_CASES[key.case];
    // A side cut has two adjacent high cells and a low one in the dual cell, so the case has one
    // contour segment, which the side cut's edge midpoint ends.
    let Some(first) = mcase.segments.first() else {
        return;
    };
    let junction = Junction::of(cell, cuts, key, [first.start, first.end], tb);
    for part in mcase.regions {
        let refs = high_refs(part, cuts, key);
        let mut polygon = FloorPolygon::EMPTY;
        for &r in refs.as_slice() {
            if let Some(p) = high_position(cell, cuts, key, &junction, r, tb) {
                polygon.push(p);
            }
        }
        emit_dual_floor(map, cell, &polygon, cliff_level_height(tb), scratch, stats);
    }
    for seg in mcase.segments {
        let ends = [seg.start, seg.end];
        if ends.iter().all(|&m| side_cut_at(cuts, key, m).is_none()) {
            super::emit_cliff_walls(cell, std::slice::from_ref(seg), ta, tb, scratch, stats);
            continue;
        }
        let junction = Junction::of(cell, cuts, key, ends, tb);
        let (list, len) = stations(cuts, key, ends);
        let mut previous: Option<Vec<WallProfileRow>> = None;
        for &station in &list[..len] {
            let rows = station_rows(cell, &junction, station, ta, tb);
            if let Some(prev) = &previous {
                emit_wall_strip(prev, &rows, scratch, stats);
            }
            previous = Some(rows);
        }
    }
}

/// Where high region vertex `r` of a run with key `key` lies on the floor at `H(tb)`: a side
/// cut's corner moves to its rim-top crossing of the dual edge to its back corner (or drops out
/// where that corner is a side cut too, the apex), a contour midpoint is inset as on an axis
/// wall, and the centre is the junction's floor point.
fn high_position(
    cell: &DualCell,
    cuts: &DualCuts,
    key: RunKey,
    junction: &Junction,
    r: DualVertexRef,
    tb: i32,
) -> Option<Vec2> {
    match r {
        DualVertexRef::Corner(c) => {
            let k = corner_index(c);
            match cuts.corner(k) {
                Some((cut, Role::Side { back, .. })) if key.side & (1 << k) != 0 => {
                    (key.side & (1 << back) == 0).then(|| crossing(&cut, k, back, tb))
                }
                _ => Some(cell.corner_coord(c)),
            }
        }
        DualVertexRef::Midpoint(m) => {
            Some(cell.midpoint_coord(m) - cell.midpoint_d(m, tb) * cell.floor_inset(m, tb))
        }
        DualVertexRef::Centre => Some(junction.floor_point(cell.centre, tb)),
    }
}

/// The low floor at `H(lo)` of a dual cell a cut touches: the effective case's low region at
/// `lo + 1` (key `key`), with each side cut's triangle added and its crossing column's toe on the
/// foot, then clipped behind the rim-top line of every cut whose opposite corner `O` this dual
/// cell is (D-2, D-4).
pub(super) fn emit_cut_low_floor(
    map: &GameMap,
    cell: &DualCell,
    cuts: &DualCuts,
    key: RunKey,
    lo: i32,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    for part in MARCHING_CASES[key.case].low_regions {
        let mut refs = low_refs(part, cuts, key);
        refs.start_at_low_corner(cuts, key);
        let slice = refs.as_slice();
        let mut polygon = FloorPolygon::EMPTY;
        for (i, &r) in slice.iter().enumerate() {
            polygon.push(match r {
                DualVertexRef::Corner(c) => cell.corner_coord(c),
                DualVertexRef::Midpoint(m) => cell.midpoint_coord(m),
                DualVertexRef::Centre => cell.centre,
            });
            let next = slice[(i + 1) % slice.len()];
            if let Some(toe) = toe_between(cuts, key, r, next) {
                polygon.push(toe);
            }
        }
        let mut earlier: [Option<(Cut, f32)>; 4] = [None; 4];
        for k in 0..4 {
            let Some((cut, Role::Opposite)) = cuts.corner(k) else {
                continue;
            };
            if cut.active(lo) {
                polygon = clip_behind(&polygon, &cut, lo, &earlier);
                earlier[k] = Some((cut, cut.inset(lo)));
            }
        }
        emit_dual_floor(map, cell, &polygon, cliff_level_height(lo), scratch, stats);
    }
}

/// The toe of side cut `k`'s crossing column, when low region vertices `a` and `b` are that cut
/// cell's corner and the dual cell's centre (the foot's two ends in this dual cell) and the cut
/// has the column here: the low floor's edge along the foot then carries it, so the floor meets
/// the wall vertex for vertex (D-4).
fn toe_between(cuts: &DualCuts, key: RunKey, a: DualVertexRef, b: DualVertexRef) -> Option<Vec2> {
    (0..4).find_map(|k| {
        let Some((cut, Role::Side { back, .. })) = cuts.corner(k) else {
            return None;
        };
        let corner = DualVertexRef::Corner(dual_corner(k));
        let on_foot = (a == corner && b == DualVertexRef::Centre)
            || (a == DualVertexRef::Centre && b == corner);
        (key.side & (1 << k) != 0 && on_foot && has_crossing(back, key))
            .then(|| cut.toe(crossing(&cut, k, back, cut.t_hi)))
    })
}

/// How far from a line a polygon vertex may lie and still count as on it, world units scaled by
/// `√2` (the units of `Cut::side`): eight units in the last place of the largest coordinate. A
/// vertex placed on a line (a crossing, an anchor, a junction's floor point) is a few `f32`
/// operations on such coordinates; every other vertex of a dual cell lies at least `0.023` from
/// the line (the rim-top crossing's distance past an edge midpoint).
fn on_line_tolerance(polygon: &FloorPolygon) -> f32 {
    let extent = polygon
        .points()
        .iter()
        .map(|p| p.x.abs().max(p.y.abs()))
        .fold(1.0_f32, f32::max);
    8.0 * f32::EPSILON * extent
}

/// `polygon` clipped behind the rim-top line of cut `cut` at threshold `t`, for the dual cell at
/// the cut's opposite corner `O` (D-4): Sutherland–Hodgman, with every new vertex on an
/// axis-aligned edge (a dual edge) at its closed-form crossing ([`Cut::rim_point`]), a new vertex
/// on a line an earlier cut of `earlier` clipped at the two lines' intersection ([`apex`]), and,
/// where the floor meets the rim top, the cut cell's centre column's rim-top row inserted on the
/// clipped edge, so the floor and the walls share every vertex on the line.
fn clip_behind(
    polygon: &FloorPolygon,
    cut: &Cut,
    t: i32,
    earlier: &[Option<(Cut, f32)>; 4],
) -> FloorPolygon {
    let inset = cut.inset(t);
    let tol = on_line_tolerance(polygon);
    let side = |p: Vec2| {
        let s = cut.side(inset, p);
        if s.abs() <= tol {
            0.0
        } else {
            s
        }
    };
    let on_earlier = |p: Vec2, q: Vec2| {
        earlier.iter().flatten().find(|(other, other_inset)| {
            other.side(*other_inset, p).abs() <= tol && other.side(*other_inset, q).abs() <= tol
        })
    };
    let mut out = FloorPolygon::EMPTY;
    let points = polygon.points();
    for (i, &cur) in points.iter().enumerate() {
        let prev = points[(i + points.len() - 1) % points.len()];
        let (s_prev, s_cur) = (side(prev), side(cur));
        if (s_prev < 0.0 && s_cur > 0.0) || (s_prev > 0.0 && s_cur < 0.0) {
            let q = if prev.x.to_bits() == cur.x.to_bits() {
                cut.rim_point(inset, true, prev.x)
            } else if prev.y.to_bits() == cur.y.to_bits() {
                cut.rim_point(inset, false, prev.y)
            } else if let Some((other, other_inset)) = on_earlier(prev, cur) {
                apex(cut, inset, other, *other_inset)
            } else {
                prev + (cur - prev) * (s_prev / (s_prev - s_cur))
            };
            out.push(q);
        }
        if s_cur <= 0.0 {
            out.push(cur);
        }
    }
    if cut.rim(t) {
        insert_on_line(&mut out, cut, inset, cut.anchor(), tol);
    }
    out
}

/// Inserts `anchor` into the edge of `polygon` that runs along cut `cut`'s line `inset` behind its
/// foot and passes over it, if there is one.
fn insert_on_line(polygon: &mut FloorPolygon, cut: &Cut, inset: f32, anchor: Vec2, tol: f32) {
    let along = |p: Vec2| cut.dir.0 as f32 * p.x - cut.dir.1 as f32 * p.y;
    let on = |p: Vec2| cut.side(inset, p).abs() <= tol;
    let points = polygon.points();
    let n = points.len();
    let at = (0..n).find(|&i| {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let (sa, sb, sx) = (along(a), along(b), along(anchor));
        on(a) && on(b) && (sa.min(sb) < sx) && (sx < sa.max(sb))
    });
    if let Some(i) = at {
        polygon.insert(i + 1, anchor);
    }
}

/// The wall of every cut whose opposite corner `O` this dual cell is, between its two crossing
/// columns through the cut cell's centre column (D-4, D-5): one strip whose middle column's
/// vertices both halves share. The band between the crossings lies mostly in this dual cell, and
/// emitting the centre column here, once, gives the dual cells round the cut cell's centre the
/// same vertices on any chunk seam through it: one floor corner each in the dual cells at `C`,
/// `P₁` and `P₂`, one toe here. Built over the whole cut run, as the strips that meet it in the
/// dual cells at `P₁` and `P₂`. An end whose backing cell is cut too (the 90° apex) has no
/// crossing; the apex's strip ends at the centre column instead.
pub(super) fn emit_cut_chains(cuts: &DualCuts, scratch: &mut MesherScratch, stats: &mut MeshStats) {
    for k in 0..4 {
        let Some((cut, Role::Opposite)) = cuts.corner(k) else {
            continue;
        };
        let (ta, tb) = (cut.lb + 1, cut.t_hi);
        let centre = cut.centre();
        // The strip runs along (-dy, dx), so its low side is on its right as every wall's is.
        let along = |p: Vec2| -(cut.dir.1 as f32) * p.x + cut.dir.0 as f32 * p.y;
        // The centre column (`None`) and a crossing column on each dual edge to a backing cell.
        let mut stops: [Option<(f32, Option<Vec2>)>; 3] = [Some((along(centre), None)), None, None];
        for (slot, (neighbour, vertical)) in stops[1..].iter_mut().zip([
            (vertical_neighbour(k), true),
            (horizontal_neighbour(k), false),
        ]) {
            if !matches!(cuts.corner(neighbour), Some((_, Role::Opposite))) {
                let e = if vertical { centre.x } else { centre.y };
                let at = cut.rim_point(cut.inset(cut.t_hi), vertical, e);
                *slot = Some((along(at), Some(at)));
            }
        }
        stops.sort_by(|a, b| match (a, b) {
            (Some(a), Some(b)) => a.0.total_cmp(&b.0),
            _ => b.is_some().cmp(&a.is_some()),
        });
        let profiles: Vec<Vec<WallProfileRow>> = stops
            .iter()
            .flatten()
            .map(|&(_, at)| match at {
                Some(p) => cut.crossing_column(p, ta, tb),
                None => cut.centre_column(ta, tb),
            })
            .collect();
        let chain: Vec<&[WallProfileRow]> = profiles.iter().map(Vec::as_slice).collect();
        emit_column_chain(&chain, scratch, stats);
    }
}

/// Emits a wall strip through the profiles `columns` in order, with one vertex per column per
/// distinct row height of any column (heights within `1e-4` merge), top to bottom, so an inner
/// column's vertices are shared by the quads on both its sides: `emit_wall_strip` for more than
/// two columns. Degenerate triangles are dropped.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a chunk holds far fewer than u32::MAX vertices"
)]
fn emit_column_chain(
    columns: &[&[WallProfileRow]],
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    if columns.len() < 2 {
        return;
    }
    let mut heights: Vec<f32> = Vec::with_capacity(columns.iter().map(|c| c.len()).sum());
    for r in columns.iter().flat_map(|c| c.iter()) {
        if !heights.iter().any(|&h| (h - r.y).abs() < 1e-4) {
            heights.push(r.y);
        }
    }
    heights.sort_by(|a, b| b.total_cmp(a));
    if heights.len() < 2 {
        return;
    }
    let width = columns.len();
    stats.wall_segments += width - 1;
    let base = scratch.positions.len() as u32;
    for &y in &heights {
        for column in columns {
            let row = super::sample_profile_at_height(column, y);
            scratch.positions.push([row.xz.x, row.y, row.xz.y]);
            scratch
                .normals
                .push([row.normal.x, row.normal.y, row.normal.z]);
            scratch.uv0.push([row.xz.x, row.xz.y]);
            scratch.uv1.push([row.ao, 0.0]);
        }
        stats.wall_vertices += width;
    }
    let at = |row: usize, column: usize| base + (row * width + column) as u32;
    for row in 0..heights.len() - 1 {
        for column in 0..width - 1 {
            let quad = [
                at(row, column),
                at(row, column + 1),
                at(row + 1, column + 1),
                at(row + 1, column),
            ];
            for tri in [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]] {
                let [p0, p1, p2] = tri.map(|i| Vec3::from(scratch.positions[i as usize]));
                if (p1 - p0).cross(p2 - p0).length_squared() > 1e-10 {
                    scratch.indices.extend_from_slice(&tri);
                    stats.wall_indices += 3;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{surface_class, SurfaceClass};
    use super::*;
    use sim_map::grid::{CliffLevel, MapCell, RampDir, TerrainKind};

    /// `sim_map::diagonal::marching_level`, which the cut rule reads, classifies every cell as
    /// `surface_class` does: a plateau by its level, the abyss and every cell off the map as the
    /// void, a ramp cell as a ramp.
    #[test]
    #[expect(clippy::expect_used, reason = "a failed test is a panic")]
    fn cut_level_classifies_as_surface_class() {
        let mut map = GameMap::new(4, 3, MapCell::default_ground(CliffLevel(2)))
            .expect("invariant: 4x3 is a valid map size");
        map.cell_mut(1, 1)
            .expect("invariant: the cell lies on the map")
            .terrain_kind = TerrainKind::Abyss;
        map.cell_mut(2, 1)
            .expect("invariant: the cell lies on the map")
            .ramp = Some(RampDir::East);
        map.cell_mut(3, 2)
            .expect("invariant: the cell lies on the map")
            .cliff_level = CliffLevel(0);
        for y in -1..=3 {
            for x in -1..=4 {
                let expected = match surface_class(&map, x, y) {
                    SurfaceClass::Void => Some(-1),
                    SurfaceClass::Plateau(level) => Some(level),
                    SurfaceClass::Ramp(_) => None,
                };
                assert_eq!(
                    marching_level(&map, i64::from(x), i64::from(y)).map(i32::from),
                    expected,
                    "cell ({x}, {y})"
                );
            }
        }
    }
}
