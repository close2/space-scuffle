//! Shared terrain-mesh fixtures and geometry helpers for the CPU terrain tests.
//!
//! # Responsibility
//! Holds what several terrain test binaries need and none should own alone:
//! - the shipped maps through the real `MapFile` loader ([`load_shipped_map`]);
//! - the seeded 37x29 synthetic map of the mesher goldens ([`synthetic_map`]), which covers all
//!   sixteen marching cases including saddles, stacked runs, abyss and ramps of eight directions;
//! - the ramp-geometry fixture of plan `2026-11-09-ramp-geometry` T0 ([`fixture_map`]): ramp
//!   stations in every direction, beside the abyss, recessed and half-recessed;
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
use client::terrain::mesher::{build_chunk_mesh, MesherScratch, CHUNK_DUAL_CELLS, RAIL_WIDTH};
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
