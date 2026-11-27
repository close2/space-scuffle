//! Shared terrain-mesh fixtures and geometry helpers for the CPU terrain tests.
//!
//! # Responsibility
//! Holds what several terrain test binaries need and none should own alone:
//! - the shipped maps through the real `MapFile` loader ([`load_shipped_map`]);
//! - the seeded 37x29 synthetic map of the mesher goldens ([`synthetic_map`]), which covers all
//!   sixteen marching cases including saddles, stacked runs, abyss and ramps of eight directions;
//! - the ramp-geometry fixture of plan `2026-11-09-ramp-geometry` T0 ([`fixture_map`]): ramp
//!   stations in every direction, beside the abyss, recessed and half-recessed;
//! - the diagonal fixture of plan `2026-12-01-straight-diagonal-cliffs` §4.0
//!   ([`diagonal_fixture`]): one example of every kind of diagonal cliff, corner and fallback;
//! - [`diagonal_cuts`], [`diagonal_runs`] and [`void_runs`]: that plan's full cuts (D-1) and the
//!   straight lines they and the void diagonals (D-6) must draw, re-implemented from the plan and
//!   reading the map's cells directly, never the mesher's predicate;
//! - [`leaky_chunk_seams`], the seam comparison of `terrain_tests.rs` for any map;
//! - [`MapMesh`], every chunk triangle of a map bucketed by the cells it may touch;
//! - [`clip_to_column`], a Sutherland–Hodgman clip of a 3D triangle against an axis-aligned XZ box
//!   (the open column above a cell), interpolating the altitude along clipped edges.
//!
//! # How tests use it
//! Include it with `#[path = "support/terrain_mesh.rs"]`. Each binary uses a subset, so the
//! including module allows `dead_code`. The synthetic map and the fixture were moved here from
//! `mesher_golden_tests.rs` and `ramp_geometry_tests.rs` unchanged (plan
//! `2026-11-30-terrain-and-air-indicators` WP0), so the goldens' hashes did not move with them.
//!
//! # Coordinates
//! Simulation X is Bevy X, simulation Y is Bevy Z, and altitude is Bevy Y, in world units; a cell
//! `(x, y)` spans `[x, x + 1] x [y, y + 1]` in XZ.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::path::PathBuf;

use bevy::prelude::{Vec2, Vec3};
use client::resolve_assets_dir;
use client::terrain::mesher::{
    build_chunk_mesh, cliff_level_height, MesherScratch, CHUNK_DUAL_CELLS, RAIL_WIDTH,
};
use sim_map::grid::{
    CellFlags, CliffLevel, GameMap, MapCell, RampDir, TerrainKind, MAX_RAMP_RUN_CELLS,
};
use sim_map::map_format::MapFile;

// -----------------------------------------------------------------------------
// Shipped maps
// -----------------------------------------------------------------------------

/// The shipped map `filename` under `assets/maps/`, through the real `MapFile` loader.
pub fn load_shipped_map(filename: &str) -> GameMap {
    let direct = resolve_assets_dir().join("maps").join(filename);
    let path = if direct.exists() {
        direct
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps")
            .join(filename)
    };
    MapFile::load(&path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", path.display()))
        .decode()
        .unwrap_or_else(|e| panic!("failed to decode {}: {e}", path.display()))
        .to_game_map()
        .unwrap_or_else(|e| panic!("failed to build GameMap from {}: {e}", path.display()))
}

// -----------------------------------------------------------------------------
// The synthetic map of the mesher goldens
// -----------------------------------------------------------------------------

/// `SplitMix64`: a tiny, fully specified generator for the synthetic map.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    /// The next 64 random bits.
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A value in `0..n`.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A 37x29 seeded map: 3x3 plateau blocks at levels 0..=3, scattered abyss, and ramps in all
/// eight directions, some in straight runs of up to four cells.
pub fn synthetic_map() -> GameMap {
    const W: u32 = 37;
    const H: u32 = 29;
    const DIRS: [RampDir; 8] = [
        RampDir::North,
        RampDir::South,
        RampDir::East,
        RampDir::West,
        RampDir::NorthEast,
        RampDir::NorthWest,
        RampDir::SouthEast,
        RampDir::SouthWest,
    ];
    let base = MapCell {
        terrain_kind: TerrainKind::MetalPlate,
        cliff_level: CliffLevel(0),
        flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
        ramp: None,
        occupying_structure: None,
        occupying_resource: None,
    };
    let mut map = GameMap::new(W, H, base).expect("synthetic map dimensions are non-zero");
    let mut rng = SplitMix64(0x5ca1_ab1e_0009_0004);

    let blocks_x = W.div_ceil(3) as usize;
    let blocks_y = H.div_ceil(3) as usize;
    let mut block_levels = vec![0u8; blocks_x * blocks_y];
    for level in &mut block_levels {
        *level = rng.below(4) as u8;
    }
    for y in 0..H {
        for x in 0..W {
            let level = block_levels[(y / 3) as usize * blocks_x + (x / 3) as usize];
            let cell = map.cell_mut(x, y).expect("in bounds");
            cell.cliff_level = CliffLevel(level);
            cell.terrain_kind = if rng.below(100) < 7 {
                TerrainKind::Abyss
            } else {
                TerrainKind::Grass
            };
        }
    }

    // Straight runs: a direction, a start cell and a length, all on one cliff level.
    for _ in 0..14 {
        let dir = DIRS[rng.below(8) as usize];
        let (dx, dy): (i64, i64) = match dir {
            RampDir::North => (0, 1),
            RampDir::South => (0, -1),
            RampDir::East => (1, 0),
            RampDir::West => (-1, 0),
            RampDir::NorthEast => (1, 1),
            RampDir::NorthWest => (-1, 1),
            RampDir::SouthEast => (1, -1),
            RampDir::SouthWest => (-1, -1),
        };
        let len = 1 + rng.below(4) as i64;
        let start_x = rng.below(u64::from(W)) as i64;
        let start_y = rng.below(u64::from(H)) as i64;
        let level = rng.below(3) as u8;
        // Ramps are two cells wide across the run, like the shipped maps.
        let (wx, wy) = if dx == 0 { (1, 0) } else { (0, 1) };
        for k in 0..len {
            for w in 0..2 {
                let x = start_x + k * dx + w * wx;
                let y = start_y + k * dy + w * wy;
                let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) else {
                    continue;
                };
                if let Ok(cell) = map.cell_mut(ux, uy) {
                    cell.ramp = Some(dir);
                    cell.cliff_level = CliffLevel(level);
                    cell.terrain_kind = TerrainKind::Dirt;
                }
            }
        }
    }
    map
}

// -----------------------------------------------------------------------------
// The ramp-geometry fixture (plan 2026-11-09-ramp-geometry, T0)
// -----------------------------------------------------------------------------

/// What lies beside a ramp run across its lateral edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Ground at the ramp's low level: a protruding ramp.
    Low,
    /// Ground at the ramp's high level: a recessed ramp on this side.
    High,
    /// Ground two levels above the ramp's low level.
    High2,
    /// Bottomless abyss.
    Abyss,
    /// A second ramp run of the same direction and width 2, one level higher.
    Twin,
}

/// One ramp run inside its own box of ground.
#[derive(Debug, Clone, Copy)]
pub struct Station {
    /// The direction the ramp ascends towards.
    pub dir: RampDir,
    /// Run length in cells.
    pub run: u32,
    /// Width across the run in cells.
    pub width: u32,
    /// The run's low cliff level.
    pub low: u8,
    /// What lies beside the run on its first lateral side.
    pub left: Side,
    /// What lies beside the run on its second lateral side.
    pub right: Side,
}

/// Cells of ground around a station's ramp on every side.
pub const STATION_MARGIN: u32 = 3;

/// Width of the second run of a [`Side::Twin`] station.
const TWIN_WIDTH: u32 = 2;

impl Station {
    /// Lateral extent of the station's ramps, without margins.
    fn lateral_cells(&self) -> u32 {
        self.width
            + if self.right == Side::Twin {
                TWIN_WIDTH
            } else {
                0
            }
    }

    /// Box size `(along the run, across it)` in cells.
    pub fn box_size(&self) -> (u32, u32) {
        (
            self.run + 2 * STATION_MARGIN,
            self.lateral_cells() + 2 * STATION_MARGIN,
        )
    }

    /// Box size in world cells `(x, y)`.
    pub fn world_size(&self) -> (u32, u32) {
        let (along, across) = self.box_size();
        match self.dir {
            RampDir::North | RampDir::South => (across, along),
            _ => (along, across),
        }
    }

    /// World cell of box-local cell `(along, across)` for a box whose lower-left cell is `origin`.
    pub fn world_cell(&self, origin: (u32, u32), along: u32, across: u32) -> (u32, u32) {
        let (len, _) = self.box_size();
        match self.dir {
            RampDir::North => (origin.0 + across, origin.1 + along),
            RampDir::South => (origin.0 + across, origin.1 + len - 1 - along),
            RampDir::East => (origin.0 + along, origin.1 + across),
            RampDir::West => (origin.0 + len - 1 - along, origin.1 + across),
            _ => panic!("stations use cardinal ramps only"),
        }
    }

    /// The cell at run coordinate `u` (0 = the run's first cell) and lateral coordinate `v`
    /// (0 = the first lateral cell of the ramp).
    pub fn cell(&self, u: i64, v: i64) -> MapCell {
        let run = i64::from(self.run);
        let width = i64::from(self.width);
        let in_run = (0..run).contains(&u);
        let twin = self.right == Side::Twin && v >= width;
        let (low, v_local, width_local) = if twin {
            (self.low + 1, v - width, i64::from(TWIN_WIDTH))
        } else {
            (self.low, v, width)
        };
        if in_run && (0..width_local).contains(&v_local) {
            return ramp_cell(low, self.dir);
        }
        if in_run {
            let side = if v_local < 0 {
                self.left
            } else if twin {
                Side::Low
            } else {
                self.right
            };
            return match side {
                Side::Low | Side::Twin => ground(low),
                Side::High => ground(low + 1),
                Side::High2 => ground(low + 2),
                Side::Abyss => abyss(),
            };
        }
        if u < 0 {
            ground(low)
        } else {
            ground(low + 1)
        }
    }
}

/// A buildable, pathable plateau cell at `level`.
pub fn ground(level: u8) -> MapCell {
    MapCell {
        terrain_kind: if level.is_multiple_of(2) {
            TerrainKind::Grass
        } else {
            TerrainKind::MetalPlate
        },
        cliff_level: CliffLevel(level),
        flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
        ramp: None,
        occupying_structure: None,
        occupying_resource: None,
    }
}

/// A bottomless abyss cell.
pub fn abyss() -> MapCell {
    MapCell {
        terrain_kind: TerrainKind::Abyss,
        cliff_level: CliffLevel(0),
        flags: CellFlags::UNPASSABLE_CLIFF,
        ramp: None,
        occupying_structure: None,
        occupying_resource: None,
    }
}

/// A pathable ramp cell of a run whose low level is `low`, ascending towards `dir`.
pub fn ramp_cell(low: u8, dir: RampDir) -> MapCell {
    MapCell {
        terrain_kind: TerrainKind::Dirt,
        cliff_level: CliffLevel(low),
        flags: CellFlags::GROUND_PATHABLE.union(CellFlags::RAMP),
        ramp: Some(dir),
        occupying_structure: None,
        occupying_resource: None,
    }
}

/// The stations of the fixture. The first one is placed so that its head lies on a chunk border.
pub fn stations() -> Vec<Station> {
    use RampDir::{East, North, South, West};
    use Side::{Abyss, High, High2, Low, Twin};
    let dirs = [North, East, South, West];
    let station = |dir, run, width, low, left, right| Station {
        dir,
        run,
        width,
        low,
        left,
        right,
    };
    let mut list = vec![station(East, 3, 2, 1, Low, Low)];
    for run in 1..=u32::from(MAX_RAMP_RUN_CELLS) {
        list.push(station(
            dirs[run as usize % 4],
            run,
            1 + run % 4,
            (run % 3) as u8,
            Low,
            Low,
        ));
    }
    list.extend([
        station(North, 4, 2, 1, Abyss, Abyss),
        station(West, 2, 3, 0, Abyss, Low),
        station(South, 3, 2, 0, High, High),
        station(East, 4, 3, 1, High, High),
        station(West, 3, 2, 1, High, Low),
        station(North, 2, 1, 0, Low, High),
        station(East, 3, 2, 0, High2, Low),
        station(North, 4, 2, 1, Low, High2),
        station(South, 3, 2, 0, Low, Twin),
        station(West, 2, 1, 1, Abyss, Twin),
    ]);
    list
}

/// Width of the fixture map in cells (not a multiple of `CHUNK_DUAL_CELLS`).
pub const FIXTURE_WIDTH: u32 = 77;

/// Builds the T0 fixture: stations packed in shelves on level-0 ground, one cell apart.
pub fn fixture_map() -> GameMap {
    let list = stations();
    // Shelf packing; the first station starts at x = 10 so that its head (the last ramp cell,
    // x = 10 + margin + run - 1 = 15) and its upper landing (x = 16) lie in different chunks.
    let mut origins = Vec::with_capacity(list.len());
    let (mut x, mut y, mut shelf_height) = (10, 1, 0);
    for st in &list {
        let (w, h) = st.world_size();
        if x + w + 1 > FIXTURE_WIDTH {
            x = 1;
            y += shelf_height + 1;
            shelf_height = 0;
        }
        origins.push((x, y));
        x += w + 1;
        shelf_height = shelf_height.max(h);
    }
    let mut height = y + shelf_height + 1;
    if height % CHUNK_DUAL_CELLS == 0 {
        height += 1;
    }
    assert_ne!(FIXTURE_WIDTH % CHUNK_DUAL_CELLS, 0);

    let mut map = GameMap::new(FIXTURE_WIDTH, height, ground(0)).expect("fixture size is valid");
    for (st, &origin) in list.iter().zip(&origins) {
        let (along_len, across_len) = st.box_size();
        for along in 0..along_len {
            for across in 0..across_len {
                let (cx, cy) = st.world_cell(origin, along, across);
                let u = i64::from(along) - i64::from(STATION_MARGIN);
                let v = i64::from(across) - i64::from(STATION_MARGIN);
                *map.cell_mut(cx, cy)
                    .expect("station boxes lie inside the map") = st.cell(u, v);
            }
        }
    }

    let head = map.ramp_profile(15, origins[0].1 + STATION_MARGIN);
    assert!(
        head.is_some_and(|p| p.index + 1 == p.run_len)
            && map.ramp_profile(16, origins[0].1 + 3).is_none(),
        "the first station's head must be the last cell of chunk column 0"
    );
    map
}

/// Every ramp cell of `map`, row by row.
pub fn ramp_cells(map: &GameMap) -> Vec<(u32, u32)> {
    let mut cells = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            if map.ramp_profile(x, y).is_some() {
                cells.push((x, y));
            }
        }
    }
    cells
}

/// Lateral and run margins `(x, z)` of a ramp footprint for a ramp direction, each `epsilon`
/// wide, and the guard rail's width more across the run: the guard rails ride on the edges
/// parallel to the run (plan `2026-11-09-ramp-geometry` T2).
pub fn footprint_margins(dir: RampDir, epsilon: f32) -> (f32, f32) {
    let rail = RAIL_WIDTH + epsilon;
    match dir {
        RampDir::North | RampDir::South => (rail, epsilon),
        RampDir::East | RampDir::West => (epsilon, rail),
        _ => (rail, rail),
    }
}

// -----------------------------------------------------------------------------
// Mesh access
// -----------------------------------------------------------------------------

/// Every triangle of every chunk of a map, bucketed by the cells its XZ bounding box overlaps.
pub struct MapMesh {
    /// The triangles, in chunk order.
    pub tris: Vec<[Vec3; 3]>,
    width: u32,
    buckets: Vec<Vec<u32>>,
}

impl MapMesh {
    /// Meshes every chunk of `map` with the production mesher.
    pub fn build(map: &GameMap) -> Self {
        let mut tris = Vec::new();
        let mut scratch = MesherScratch::default();
        let cols = (map.width() + 1).div_ceil(CHUNK_DUAL_CELLS);
        let rows = (map.height() + 1).div_ceil(CHUNK_DUAL_CELLS);
        for cy in 0..rows {
            for cx in 0..cols {
                build_chunk_mesh(map, cx, cy, &mut scratch);
                for t in scratch.indices.as_chunks::<3>().0 {
                    tris.push(t.map(|i| Vec3::from(scratch.positions[i as usize])));
                }
            }
        }
        let (width, height) = (map.width(), map.height());
        let mut buckets = vec![Vec::new(); (width * height) as usize];
        for (i, t) in tris.iter().enumerate() {
            let min_x = t.iter().map(|p| p.x).fold(f32::INFINITY, f32::min) - 1e-3;
            let max_x = t.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max) + 1e-3;
            let min_z = t.iter().map(|p| p.z).fold(f32::INFINITY, f32::min) - 1e-3;
            let max_z = t.iter().map(|p| p.z).fold(f32::NEG_INFINITY, f32::max) + 1e-3;
            let x0 = min_x.floor().max(0.0) as u32;
            let x1 = (max_x.floor() as i64).clamp(0, i64::from(width) - 1) as u32;
            let z0 = min_z.floor().max(0.0) as u32;
            let z1 = (max_z.floor() as i64).clamp(0, i64::from(height) - 1) as u32;
            if x0 >= width || z0 >= height {
                continue;
            }
            for z in z0..=z1 {
                for x in x0..=x1 {
                    buckets[(z * width + x) as usize].push(i as u32);
                }
            }
        }
        Self {
            tris,
            width,
            buckets,
        }
    }

    /// Triangles whose bounding box overlaps cell `(x, y)`.
    pub fn near(&self, x: u32, y: u32) -> impl Iterator<Item = &[Vec3; 3]> + '_ {
        self.buckets[(y * self.width + x) as usize]
            .iter()
            .map(|&i| &self.tris[i as usize])
    }

    /// Highest altitude at which a vertical line through `(px, pz)` meets a non-vertical
    /// triangle near cell `(x, y)`.
    pub fn top_at(&self, x: u32, y: u32, px: f32, pz: f32) -> Option<f32> {
        self.near(x, y)
            .filter_map(|t| vertical_hit(px, pz, t))
            .reduce(f32::max)
    }
}

/// Height at which the vertical line through `(px, pz)` meets triangle `t`, edges included.
pub fn vertical_hit(px: f32, pz: f32, t: &[Vec3; 3]) -> Option<f32> {
    let [v0, v1, v2] = *t;
    let det = (v1.z - v2.z) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.z - v2.z);
    if det.abs() < 1e-9 {
        return None;
    }
    let w0 = ((v1.z - v2.z) * (px - v2.x) + (v2.x - v1.x) * (pz - v2.z)) / det;
    let w1 = ((v2.z - v0.z) * (px - v2.x) + (v0.x - v2.x) * (pz - v2.z)) / det;
    let w2 = 1.0 - w0 - w1;
    let e = -1e-6;
    (w0 >= e && w1 >= e && w2 >= e).then_some(w0 * v0.y + w1 * v1.y + w2 * v2.y)
}

// -----------------------------------------------------------------------------
// Clipping a triangle to a column
// -----------------------------------------------------------------------------

/// Most vertices a triangle clipped by four half-spaces can have: three, plus one per plane.
pub const CLIP_CAPACITY: usize = 7;

/// A convex 3D polygon of at most [`CLIP_CAPACITY`] vertices.
#[derive(Debug, Clone, Copy)]
pub struct Clipped {
    points: [Vec3; CLIP_CAPACITY],
    len: usize,
}

impl Clipped {
    const EMPTY: Self = Self {
        points: [Vec3::ZERO; CLIP_CAPACITY],
        len: 0,
    };

    fn push(&mut self, p: Vec3) {
        assert!(
            self.len < CLIP_CAPACITY,
            "invariant: a triangle clipped by four planes has at most {CLIP_CAPACITY} vertices"
        );
        self.points[self.len] = p;
        self.len += 1;
    }

    /// The polygon's vertices; empty when nothing of the triangle lies in the box.
    pub fn points(&self) -> &[Vec3] {
        &self.points[..self.len]
    }

    /// Keeps the part where `side(p) >= 0`, cutting each crossing edge at `a + t (b - a)`.
    fn keep(&self, side: impl Fn(Vec3) -> f32) -> Self {
        let mut out = Self::EMPTY;
        for i in 0..self.len {
            let prev = self.points[(i + self.len - 1) % self.len];
            let cur = self.points[i];
            let (s_prev, s_cur) = (side(prev), side(cur));
            if (s_prev < 0.0 && s_cur > 0.0) || (s_prev > 0.0 && s_cur < 0.0) {
                out.push(prev + (cur - prev) * (s_prev / (s_prev - s_cur)));
            }
            if s_cur >= 0.0 {
                out.push(cur);
            }
        }
        out
    }
}

/// Sutherland–Hodgman clip of triangle `t` against the closed vertical column over the XZ box
/// `[min, max]` (`Vec2` is `(x, z)`). The altitude of a cut vertex is interpolated along the cut
/// edge, so every vertex of the result lies on the triangle. Vertical triangles (walls) clip
/// correctly too, because every clipping plane is vertical.
pub fn clip_to_column(t: &[Vec3; 3], min: Vec2, max: Vec2) -> Clipped {
    let mut polygon = Clipped::EMPTY;
    for &p in t {
        polygon.push(p);
    }
    polygon
        .keep(|p| p.x - min.x)
        .keep(|p| max.x - p.x)
        .keep(|p| p.z - min.y)
        .keep(|p| max.y - p.z)
}

// -----------------------------------------------------------------------------
// The diagonal fixture (plan 2026-12-01-straight-diagonal-cliffs §4.0)
// -----------------------------------------------------------------------------

/// Width of the diagonal fixture in cells: four chunks, so its last seam is the map's edge.
pub const DIAGONAL_FIXTURE_WIDTH: u32 = 64;

/// Height of the diagonal fixture in cells.
pub const DIAGONAL_FIXTURE_HEIGHT: u32 = 48;

/// The cells of [`diagonal_fixture`], row 0 (`y = 0`) first and column `x` at character `x`:
/// `0` to `2` a plateau of that level, `a` abyss, `s` a level-0 ramp cell ascending south.
///
/// The features, each in its own box on level-0 ground, at least one cell from the next
/// (numbered by the rows of plan §4.0):
/// - (1) `x 1..=8, y 1..=8`: a level-1 staircase `x + y <= 11`, both ends on straight walls; its
///   three other corners are rectangle corners (6).
/// - (14) `x 11..=21, y 1..=11`: a staircase `y <= x - 6`, whose cut cell `(15, 9)` has its centre on
///   the chunk seam `x = 15.5`.
/// - (2) `x 24..=37, y 1..=11`: a staircase that ends at the concave corner `(32, 6)` of a long wall.
/// - (3) `x 40..=50, y 1..=8`: a one-step notch at `x = 45` between two long walls.
/// - (12) `x 53..=62, y 1..=10`: a staircase whose top end `(56, 10)` lies beside the ramp `s` at
///   `(55..=56, 11)`, so the last cut before it falls back.
/// - (11) `x 4..=26, y 14..=30`: a stacked staircase 0/1/2 (`x + y <= 43` level 1, `<= 40` level 2),
///   both runs crossing the seam `x = 15.5` with a cut centre on it.
/// - (7) `x 29..=46, y 15..=21`: a one-cell diagonal ridge, a two-cell band and a three-cell band.
/// - (8) `x 51..=62, y 14..=30`: two plateaus with a one-cell diagonal low channel between them.
/// - (4) `x 29..=40, y 24..=31`: an isolated concave corner, ground (an L).
/// - (5) `x 1..=12, y 34..=46`: an isolated concave corner, void (an L around abyss).
/// - (9) `x 15..=28, y 34..=46`: a diagonal abyss canyon through level-1 ground.
/// - (10) `x 32..=43, y 35..=46`: a staircase whose low side is abyss from `y = 40` up, ground below.
/// - (13) `x 45..=62, y 35..=43`: a roof whose two staircases meet at the apex `(54, 44)`.
pub const DIAGONAL_FIXTURE_ROWS: [&str; 48] = [
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0111111110011111111111001111111111111100111111111110011111111110",
    "0111111110011111111111001111111111111100111111111110011111111110",
    "0111111110011111111111001111111111111100111111111110011111111110",
    "0111111100011111111111001111111111111100111111111110011111111110",
    "0111111000011111111111001111111111111100111111111110011111111100",
    "0111110000001111111111001111111100000000111111111110011111111000",
    "0111100000000111111111001111111000000000111111111110011111110000",
    "0111000000000011111111001111110000000000111110000000011111100000",
    "0000000000000001111111001111100000000000000000000000011111000000",
    "0000000000000000111111001111000000000000000000000000011110000000",
    "0000000000000000011111001110000000000000000000000000000ss0000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000222222222222222222222220000000000000000000000001111111111010",
    "0000222222222222222222222210010001100001110000000001111111110110",
    "0000222222222222222222222110001000110000111000000001111111101110",
    "0000222222222222222222221110000100011000011100000001111111011110",
    "0000222222222222222222211100000010001100001110000001111110111110",
    "0000222222222222222222111000000001000110000111000001111101111110",
    "0000222222222222222221110000000000100011000011100001111011111110",
    "0000222222222222222211100000000000010001100000000001110111111110",
    "0000222222222222222111000000000000000000000000000001101111111110",
    "0000222222222222221110000000000000000000000000000001011111111110",
    "0000222222222222211100000000011111111111100000000000111111111110",
    "0000222222222222111000000000011111111111100000000001111111111110",
    "0000222222222221110000000000011111111111100000000001111111111110",
    "0000222222222211100000000000011111111111100000000001111111111110",
    "0000222222222111000000000000011110000000000000000001111111111110",
    "0000222222221110000000000000011110000000000000000001111111111110",
    "0000222222211100000000000000011110000000000000000001111111111110",
    "0000000000000000000000000000011110000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "01111111111110011aaaa1111111100000000000000000000000000000000000",
    "011111111111100111aaaa111111100011111111100001111111111111111110",
    "0111111111111001111aaaa11111100011111111000000111111111111111100",
    "01111111111110011111aaaa1111100011111110000000011111111111111000",
    "01111aaaaaaaa00111111aaaa111100011111100000000001111111111110000",
    "01111aaaaaaaa001111111aaaa11100011111000000000000111111111100000",
    "01111aaaaaaaa0011111111aaaa110001111aaaaaaaa00000011111111000000",
    "01111aaaaaaaa00111111111aaaa1000111aaaaaaaaa00000001111110000000",
    "01111aaaaaaaa001111111111aaaa00011aaaaaaaaaa00000000111100000000",
    "01111aaaaaaaa0011111111111aaa0001aaaaaaaaaaa00000000011000000000",
    "01111aaaaaaaa00111111111111aa000aaaaaaaaaaaa00000000000000000000",
    "01111aaaaaaaa001111111111111a000aaaaaaaaaaaa00000000000000000000",
    "01111aaaaaaaa0011111111111111000aaaaaaaaaaaa00000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
];

/// The diagonal fixture of plan `2026-12-01-straight-diagonal-cliffs` §4.0, built from
/// [`DIAGONAL_FIXTURE_ROWS`]: one example of each kind of diagonal, corner and fallback the plan's
/// table D-2 names.
pub fn diagonal_fixture() -> GameMap {
    let mut map = GameMap::new(DIAGONAL_FIXTURE_WIDTH, DIAGONAL_FIXTURE_HEIGHT, ground(0))
        .expect("the diagonal fixture's size is valid");
    for (y, row) in DIAGONAL_FIXTURE_ROWS.iter().enumerate() {
        assert_eq!(
            row.len(),
            DIAGONAL_FIXTURE_WIDTH as usize,
            "row {y} has the map's width"
        );
        for (x, symbol) in row.chars().enumerate() {
            let cell = match symbol {
                'a' => abyss(),
                's' => ramp_cell(0, RampDir::South),
                digit => ground(digit.to_digit(10).expect("a plateau level digit") as u8),
            };
            *map.cell_mut(x as u32, y as u32)
                .expect("the row lies on the map") = cell;
        }
    }
    map
}

// -----------------------------------------------------------------------------
// Diagonal runs, found independently of the mesher (plan §4.0)
// -----------------------------------------------------------------------------

/// What the diagonal analysis reads of a cell: its marching level (plateau level, -1 for the
/// abyss or off the map), or `None` for a ramp cell. Read from the map's cells directly, not
/// through the mesher.
fn analysis_level(map: &GameMap, x: i32, y: i32) -> Option<i32> {
    let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) else {
        return Some(-1);
    };
    let Ok(cell) = map.cell(ux, uy) else {
        return Some(-1);
    };
    if cell.ramp.is_some() {
        None
    } else if cell.terrain_kind == TerrainKind::Abyss {
        Some(-1)
    } else {
        Some(i32::from(cell.cliff_level.0))
    }
}

/// One full cut of plan D-1: cell `cell`, cut at its corner in direction `dir`, over the
/// thresholds `lb + 1 ..= t_hi`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagonalCut {
    /// The cut cell `X`.
    pub cell: (i32, i32),
    /// The corner direction `(dx, dy)`, each `±1`: from `X` towards the low side.
    pub dir: (i32, i32),
    /// `X`'s level.
    pub la: i32,
    /// The higher of `X`'s two low neighbours' levels: the cut's foot stands at `H(lb)`.
    pub lb: i32,
    /// The highest threshold at which the cut holds.
    pub t_hi: i32,
}

/// `1/√2` in `f64`.
const INV_SQRT2: f64 = std::f64::consts::FRAC_1_SQRT_2;

impl DiagonalCut {
    /// The cut's corner `C`, a grid point.
    pub fn corner(&self) -> (i32, i32) {
        (
            self.cell.0 + i32::from(self.dir.0 > 0),
            self.cell.1 + i32::from(self.dir.1 > 0),
        )
    }

    /// The ends of the cut's foot, `P₂` and `P₁` (plan notation), ordered along
    /// [`Self::along_axis`].
    pub fn foot_ends(&self) -> [(i32, i32); 2] {
        let (cx, cy) = self.corner();
        [(cx - self.dir.0, cy), (cx, cy - self.dir.1)]
    }

    /// The unit normal `n = dir/√2`, towards the low side.
    pub fn normal(&self) -> (f64, f64) {
        (
            f64::from(self.dir.0) * INV_SQRT2,
            f64::from(self.dir.1) * INV_SQRT2,
        )
    }

    /// The unit vector along the foot, `(dx, -dy)/√2`.
    pub fn along_axis(&self) -> (f64, f64) {
        (
            f64::from(self.dir.0) * INV_SQRT2,
            -f64::from(self.dir.1) * INV_SQRT2,
        )
    }

    /// The foot line's constant: `n·x = foot()` on the line through the low cells' corners.
    pub fn foot(&self) -> f64 {
        let (px, py) = self.foot_ends()[1];
        let (nx, ny) = self.normal();
        nx * f64::from(px) + ny * f64::from(py)
    }

    /// Whether the cut's wall has a rim: its run reaches `X`'s own level.
    pub fn rim(&self) -> bool {
        self.la == self.t_hi
    }
}

/// The full cut of cell `(x, y)`, if D-1 holds for it at some threshold: a cell has at most one.
///
/// D-1: `X` is high and its two neighbours towards the corner low (convex); the three cells
/// behind the corner's opposite are high (backed); at least one of the corner's two neighbouring
/// corners is a concave corner with ground below; and no cell of the 3×3 block is a ramp.
pub fn diagonal_cut_of(map: &GameMap, x: i32, y: i32) -> Option<DiagonalCut> {
    let level = |a: i32, b: i32| analysis_level(map, x + a, y + b);
    let mut block = [[0_i32; 3]; 3];
    for (j, row) in block.iter_mut().enumerate() {
        for (i, slot) in row.iter_mut().enumerate() {
            *slot = level(i as i32 - 1, j as i32 - 1)?;
        }
    }
    let at = |a: i32, b: i32| block[(b + 1) as usize][(a + 1) as usize];
    let la = at(0, 0);
    if la < 0 {
        return None;
    }
    for (dx, dy) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
        let (l1, l2) = (at(dx, 0), at(0, dy));
        let low_bound = l1.max(l2) + 1;
        let mut high_bound = la.min(at(-dx, 0)).min(at(0, -dy)).min(at(-dx, -dy));
        let concave = [
            (l1 >= 0).then(|| at(dx, -dy)),
            (l2 >= 0).then(|| at(-dx, dy)),
        ];
        let Some(reach) = concave.into_iter().flatten().max() else {
            continue;
        };
        high_bound = high_bound.min(reach);
        if low_bound <= high_bound {
            return Some(DiagonalCut {
                cell: (x, y),
                dir: (dx, dy),
                la,
                lb: low_bound - 1,
                t_hi: high_bound,
            });
        }
    }
    None
}

/// Every full cut of `map`, row by row.
pub fn diagonal_cuts(map: &GameMap) -> Vec<DiagonalCut> {
    let mut cuts = Vec::new();
    for y in 0..map.height() as i32 {
        for x in 0..map.width() as i32 {
            cuts.extend(diagonal_cut_of(map, x, y));
        }
    }
    cuts
}

/// One straight diagonal: a maximal chain of full cuts with one direction and the same levels,
/// each sharing a ground concave corner with the next.
#[derive(Debug, Clone)]
pub struct DiagonalRun {
    /// The cuts in order along the run.
    pub cuts: Vec<DiagonalCut>,
    /// How many of the four cells at each end of the foot are high at the run's thresholds: 3 at
    /// a concave corner, 2 at a straight wall.
    pub end_high_cells: [usize; 2],
}

impl DiagonalRun {
    /// The run's first cut, which carries its direction and levels.
    pub fn first(&self) -> DiagonalCut {
        self.cuts[0]
    }

    /// The interval of positions along the run (`along_axis·x`) its foot covers.
    pub fn along(&self) -> (f64, f64) {
        let (ux, uy) = self.first().along_axis();
        let at = |(px, py): (i32, i32)| ux * f64::from(px) + uy * f64::from(py);
        let last = self.cuts[self.cuts.len() - 1];
        (at(self.first().foot_ends()[0]), at(last.foot_ends()[1]))
    }
}

/// How many of the four cells around grid point `g` have a level of at least `t`.
fn high_cells_around(map: &GameMap, g: (i32, i32), t: i32) -> usize {
    [(-1, -1), (0, -1), (0, 0), (-1, 0)]
        .into_iter()
        .filter(|&(a, b)| analysis_level(map, g.0 + a, g.1 + b).is_some_and(|l| l >= t))
        .count()
}

/// The straight diagonals of `map` (plan §4.0): chains of [`diagonal_cuts`] linked through
/// shared ground concave corners.
pub fn diagonal_runs(map: &GameMap) -> Vec<DiagonalRun> {
    let cuts = diagonal_cuts(map);
    let find = |cell: (i32, i32)| cuts.iter().find(|c| c.cell == cell).copied();
    let same = |a: &DiagonalCut, b: &DiagonalCut| {
        a.dir == b.dir && a.la == b.la && a.lb == b.lb && a.t_hi == b.t_hi
    };
    // `next` follows the run along (dx, -dy) through P₁, `prev` back through P₂.
    let next = |c: &DiagonalCut| {
        let ground = analysis_level(map, c.cell.0 + c.dir.0, c.cell.1).is_some_and(|l| l >= 0);
        find((c.cell.0 + c.dir.0, c.cell.1 - c.dir.1)).filter(|n| ground && same(c, n))
    };
    let mut runs = Vec::new();
    for cut in &cuts {
        let starts_run = cuts.iter().all(|p| next(p) != Some(*cut));
        if !starts_run {
            continue;
        }
        let mut chain = vec![*cut];
        while let Some(n) = next(&chain[chain.len() - 1]) {
            chain.push(n);
        }
        let t = cut.lb + 1;
        let ends = [
            high_cells_around(map, chain[0].foot_ends()[0], t),
            high_cells_around(map, chain[chain.len() - 1].foot_ends()[1], t),
        ];
        runs.push(DiagonalRun {
            cuts: chain,
            end_high_cells: ends,
        });
    }
    runs
}

/// A void diagonal: a chain of concave corners whose low cell is void, along a 45° staircase, with
/// no full cut through them (plan D-6).
#[derive(Debug, Clone)]
pub struct VoidRun {
    /// The concave corners (grid points), in order along the run.
    pub corners: Vec<(i32, i32)>,
    /// The direction `(qx, qy)` from each corner to the centre of its void cell, each `±1`.
    pub towards_void: (i32, i32),
    /// The level of the three high cells around each corner.
    pub level: i32,
}

impl VoidRun {
    /// The unit normal towards the void.
    pub fn normal(&self) -> (f64, f64) {
        let (qx, qy) = self.towards_void;
        (f64::from(qx) * INV_SQRT2, f64::from(qy) * INV_SQRT2)
    }

    /// The unit vector along the run.
    pub fn along_axis(&self) -> (f64, f64) {
        let (qx, qy) = self.towards_void;
        (f64::from(qx) * INV_SQRT2, -f64::from(qy) * INV_SQRT2)
    }

    /// The midpoint line's constant: `n·x = midpoint_line()` through the edge midpoints of every
    /// void cell's two edges with the plateau.
    pub fn midpoint_line(&self) -> f64 {
        let (gx, gy) = self.corners[0];
        let (qx, qy) = self.towards_void;
        f64::from(qx * gx + qy * gy) * INV_SQRT2 + 0.5 * INV_SQRT2
    }

    /// The interval of positions along the run from its first to its last corner.
    pub fn along(&self) -> (f64, f64) {
        let (ux, uy) = self.along_axis();
        let at = |(gx, gy): (i32, i32)| ux * f64::from(gx) + uy * f64::from(gy);
        (
            at(self.corners[0]),
            at(self.corners[self.corners.len() - 1]),
        )
    }
}

/// The void concave corner at grid point `g`, if it is one: exactly one of its four cells is
/// void, the other three are plateaus of one level, and none is a ramp. Returns the direction to
/// the void cell and the level.
fn void_corner(map: &GameMap, g: (i32, i32)) -> Option<((i32, i32), i32)> {
    let quadrants = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
    let mut void = None;
    let mut levels = Vec::with_capacity(3);
    for (qx, qy) in quadrants {
        let cell = (g.0 + (qx - 1) / 2, g.1 + (qy - 1) / 2);
        let level = analysis_level(map, cell.0, cell.1)?;
        if level < 0 {
            if void.is_some() {
                return None;
            }
            void = Some((qx, qy));
        } else {
            levels.push(level);
        }
    }
    let q = void?;
    (levels.iter().all(|&l| l == levels[0])).then_some((q, levels[0]))
}

/// The void diagonals of `map` (plan §4.0 and D-6): chains of at least two void concave corners,
/// each one step `(qx, -qy)` from the next, that no full cut passes through.
pub fn void_runs(map: &GameMap) -> Vec<VoidRun> {
    let cut_corners: std::collections::BTreeSet<(i32, i32)> = diagonal_cuts(map)
        .iter()
        .flat_map(DiagonalCut::foot_ends)
        .collect();
    let corner = |g: (i32, i32)| void_corner(map, g).filter(|_| !cut_corners.contains(&g));
    let mut runs = Vec::new();
    for gy in 0..=map.height() as i32 {
        for gx in 0..=map.width() as i32 {
            let Some((q, level)) = corner((gx, gy)) else {
                continue;
            };
            let step = (q.0, -q.1);
            let before = (gx - step.0, gy - step.1);
            if corner(before) == Some((q, level)) {
                continue;
            }
            let mut corners = vec![(gx, gy)];
            loop {
                let (lx, ly) = corners[corners.len() - 1];
                let n = (lx + step.0, ly + step.1);
                if corner(n) != Some((q, level)) {
                    break;
                }
                corners.push(n);
            }
            if corners.len() >= 2 {
                runs.push(VoidRun {
                    corners,
                    towards_void: q,
                    level,
                });
            }
        }
    }
    runs
}

// -----------------------------------------------------------------------------
// Chunk seams
// -----------------------------------------------------------------------------

/// The vertices of `mesh` on the seam plane `axis == seam` (`axis` 0 for constant X, 2 for
/// constant Z), as sorted `[y, along]` bit pairs.
fn seam_vertices(mesh: &MesherScratch, axis: usize, seam: f32) -> Vec<[u32; 2]> {
    let along = 2 - axis;
    let mut verts: Vec<[u32; 2]> = mesh
        .positions
        .iter()
        .filter(|p| (p[axis] - seam).abs() < 1e-4)
        .map(|p| [p[1].to_bits(), p[along].to_bits()])
        .collect();
    verts.sort_unstable();
    verts
}

/// Whether `y_bits` is a floor altitude: a cliff level's, or the abyss wall bottom.
fn is_floor_height(y_bits: u32) -> bool {
    (-1..=4).any(|level| cliff_level_height(level).to_bits() == y_bits)
}

/// The seam between the chunk with the map's last interior dual column (or row) and the chunk
/// holding only the edge dual column beyond it: every cliff-wall vertex matches bit for bit and
/// both sides carry the same floor altitudes (the edge dual cells' lower floor runs extend under
/// the higher plateau, so floor vertices need not match one for one).
fn edge_seam_meets(a: &[[u32; 2]], b: &[[u32; 2]]) -> bool {
    let walls = |v: &[[u32; 2]]| -> Vec<[u32; 2]> {
        v.iter()
            .copied()
            .filter(|p| !is_floor_height(p[0]))
            .collect()
    };
    let floors = |v: &[[u32; 2]]| -> Vec<u32> {
        let mut ys: Vec<u32> = v
            .iter()
            .map(|p| p[0])
            .filter(|&y| is_floor_height(y))
            .collect();
        ys.dedup();
        ys
    };
    !a.is_empty() && !b.is_empty() && walls(a) == walls(b) && floors(a) == floors(b)
}

/// Every chunk seam of `map` at which the two chunks' vertices differ: the comparison of
/// `terrain_tests::test_chunk_borders_are_watertight`, for any map. Each entry names the seam.
pub fn leaky_chunk_seams(map: &GameMap) -> Vec<String> {
    let cols = (map.width() + 1).div_ceil(CHUNK_DUAL_CELLS);
    let rows = (map.height() + 1).div_ceil(CHUNK_DUAL_CELLS);
    let mut chunks = Vec::new();
    let mut scratch = MesherScratch::default();
    for cy in 0..rows {
        for cx in 0..cols {
            build_chunk_mesh(map, cx, cy, &mut scratch);
            chunks.push(scratch.clone());
        }
    }
    let chunk = |cx: u32, cy: u32| &chunks[(cy * cols + cx) as usize];
    let mut leaks = Vec::new();
    for cy in 0..rows {
        for cx in 0..cols {
            for (axis, next) in [(0, (cx + 1, cy)), (2, (cx, cy + 1))] {
                if next.0 >= cols || next.1 >= rows {
                    continue;
                }
                let index = if axis == 0 { cx } else { cy };
                let seam = ((index + 1) * CHUNK_DUAL_CELLS) as f32 - 0.5;
                let a = seam_vertices(chunk(cx, cy), axis, seam);
                let b = seam_vertices(chunk(next.0, next.1), axis, seam);
                let extent = if axis == 0 { map.width() } else { map.height() };
                let meets = if (index + 1) * CHUNK_DUAL_CELLS == extent {
                    edge_seam_meets(&a, &b)
                } else {
                    a == b
                };
                if !meets {
                    let name = if axis == 0 { "X" } else { "Z" };
                    leaks.push(format!(
                        "seam {name} = {seam} between chunks ({cx}, {cy}) and {next:?}: \
                         {} and {} vertices",
                        a.len(),
                        b.len()
                    ));
                }
            }
        }
    }
    leaks
}
