//! Bit-exact characterisation goldens for the terrain mesher, the terrain picker and the terrain
//! texture-array loader.
//!
//! # Responsibility
//! Pins the exact output of [`build_chunk_mesh`], [`pick_terrain`] and
//! [`load_theme_texture_arrays`] so that structural refactors of
//! `crates/client/src/terrain/{mesher,picking,theme_gpu}.rs` (Phase 9, WP4, step 9.2) are provably
//! behaviour-preserving. Every `f32` is hashed by its raw bits (`to_bits`), so a reordered float
//! expression, a reordered vertex or a reordered index changes the hash and fails the test.
//!
//! # Fit
//! These tests run on the CPU only: no window, no GPU, no Bevy `App`. They complement the semantic
//! terrain tests in `terrain_tests.rs` (counts, winding, watertightness), which accept many meshes;
//! the goldens accept exactly one.
//!
//! # Inputs
//! - The two shipped maps, `assets/maps/default_arena.scmap.ron` and `showcase_canyon.scmap.ron`,
//!   loaded through the real `MapFile` loader.
//! - One seeded synthetic map whose cells cover what the shipped maps do not: all eight ramp
//!   directions (including the diagonal ones the mesher skips walls for), ramps beside abyss,
//!   saddle cases and multi-level cliff stacks, at a size that is not a multiple of the chunk size.
//! - For the texture arrays: the shipped first theme of `assets/catalog/terrain_themes.ron`,
//!   pointed at a temporary tree of seeded 8x8 PNGs so the golden exercises the real loader, mip
//!   generator and layer-major packing in milliseconds.
//!
//! # Invariants
//! - The golden constants were recorded on `main` at `808b6dd`, before any WP4 edit. The three
//!   mesh hashes were re-recorded at `3baab60`, plan `2026-11-09-ramp-geometry` WP1: ramp corners no
//!   longer round into plateau levels, so the slabs, hidden floors and duplicated walls over ramp
//!   footprints are gone and ramp cells wall their own edges. The synthetic mesh hash was
//!   re-recorded again at `ef26242`, plan `2026-11-09-ramp-geometry` WP2: flank walls beside
//!   higher ground and walls between ramps of different height. The pick hashes did not change.
//! - A golden may only change together with a deliberate, documented change in terrain output.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names
)]

use std::path::PathBuf;

use asset_baker::terrain_theme::{TerrainTheme, TerrainThemeCatalog, TerrainThemeError};
use bevy::image::{Image, ImageSampler};
use bevy::prelude::Vec3;
use client::resolve_assets_dir;
use client::terrain::mesher::{build_chunk_mesh, MeshStats, MesherScratch, CHUNK_DUAL_CELLS};
use client::terrain::picking::pick_terrain;
use client::terrain::theme_gpu::{
    load_theme_texture_arrays, terrain_anisotropy_clamp, TERRAIN_ANISOTROPY_OVERRIDE_ENV,
};
use sim_map::grid::{CellFlags, CliffLevel, GameMap, MapCell, RampDir, TerrainKind};
use sim_map::map_format::MapFile;

/// Rays cast per map axis; the picker golden casts `RAY_GRID * RAY_GRID` rays per map.
const RAY_GRID: u32 = 64;

/// 64-bit FNV-1a, written out so the golden does not depend on a hasher whose algorithm may
/// change between Rust or crate releases.
struct Fnv64(u64);

impl Fnv64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
}

fn resolve_map_path(relative: &str) -> PathBuf {
    let assets = resolve_assets_dir();
    let direct = assets.join("maps").join(relative);
    if direct.exists() {
        return direct;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps")
        .join(relative)
}

fn load_shipped_map(filename: &str) -> GameMap {
    let path = resolve_map_path(filename);
    MapFile::load(&path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", path.display()))
        .decode()
        .unwrap_or_else(|e| panic!("failed to decode {}: {e}", path.display()))
        .to_game_map()
        .unwrap_or_else(|e| panic!("failed to build GameMap from {}: {e}", path.display()))
}

/// `SplitMix64`: a tiny, fully specified generator for the synthetic map.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A 37x29 seeded map: 3x3 plateau blocks at levels 0..=3, scattered abyss, and ramps in all
/// eight directions, some in straight runs of up to four cells.
fn synthetic_map() -> GameMap {
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

fn hash_stats(h: &mut Fnv64, s: &MeshStats) {
    for v in [
        s.floor_vertices,
        s.floor_indices,
        s.wall_segments,
        s.wall_vertices,
        s.wall_indices,
        s.ramp_vertices,
        s.rail_vertices,
    ] {
        h.u64(v as u64);
    }
}

/// Builds every chunk that covers the map's dual grid (`(width + 1) x (height + 1)` dual cells)
/// and hashes stats, positions, normals, both UV channels and indices by raw bits.
fn mesh_hash(map: &GameMap) -> u64 {
    chunk_hash(map, |_, _| true).0
}

/// Whether no dual cell of chunk `(chunk_x, chunk_y)` has a ramp corner: the chunk's dual cells
/// `16 * chunk .. 16 * (chunk + 1)` touch the cells `16 * chunk - 1 ..= 16 * chunk + 15` on each
/// axis, and its ramp pass reads a subset of them.
fn chunk_is_ramp_free(map: &GameMap, chunk_x: u32, chunk_y: u32) -> bool {
    let span = |chunk: u32, len: u32| {
        let start = (chunk * CHUNK_DUAL_CELLS).saturating_sub(1);
        let end = ((chunk + 1) * CHUNK_DUAL_CELLS).min(len);
        start..end
    };
    span(chunk_y, map.height())
        .all(|y| span(chunk_x, map.width()).all(|x| map.ramp_profile(x, y).is_none()))
}

/// [`mesh_hash`] over the chunks `include` accepts; also returns how many it hashed.
fn chunk_hash(map: &GameMap, include: impl Fn(u32, u32) -> bool) -> (u64, u32) {
    let mut h = Fnv64::new();
    let mut count = 0;
    let mut scratch = MesherScratch::default();
    let cols = (map.width() + 1).div_ceil(CHUNK_DUAL_CELLS);
    let rows = (map.height() + 1).div_ceil(CHUNK_DUAL_CELLS);
    for chunk_y in 0..rows {
        for chunk_x in 0..cols {
            if !include(chunk_x, chunk_y) {
                continue;
            }
            count += 1;
            let stats = build_chunk_mesh(map, chunk_x, chunk_y, &mut scratch);
            h.u32(chunk_x);
            h.u32(chunk_y);
            hash_stats(&mut h, &stats);
            h.u64(scratch.positions.len() as u64);
            for &c in scratch.positions.iter().flatten() {
                h.f32(c);
            }
            for &c in scratch.normals.iter().flatten() {
                h.f32(c);
            }
            for &c in scratch.uv0.iter().flatten() {
                h.f32(c);
            }
            for &c in scratch.uv1.iter().flatten() {
                h.f32(c);
            }
            h.u64(scratch.indices.len() as u64);
            for &i in &scratch.indices {
                h.u32(i);
            }
        }
    }
    (h.0, count)
}

/// One origin/direction pair for grid point `(i, j)` of a `RAY_GRID x RAY_GRID` grid.
///
/// The direction family rotates with the grid index so that every family samples the whole map:
/// vertical rays, axis-aligned oblique rays (one horizontal component exactly zero, so the DDA
/// has a zero step), diagonal rays, shallow grazing rays, rays entering from outside the map's
/// bounding box, rays that miss it entirely, and a degenerate zero direction.
fn ray(map: &GameMap, i: u32, j: u32) -> (Vec3, Vec3) {
    let w = map.width() as f32;
    let hgt = map.height() as f32;
    let x = (i as f32 + 0.5) / RAY_GRID as f32 * w;
    let z = (j as f32 + 0.5) / RAY_GRID as f32 * hgt;
    match (i * 7 + j * 3) % 10 {
        0 => (Vec3::new(x, 25.0, z), Vec3::new(0.0, -1.0, 0.0)),
        1 => (Vec3::new(x, 20.0, z), Vec3::new(0.0, -1.0, 0.6)),
        2 => (Vec3::new(x, 20.0, z), Vec3::new(-0.7, -1.0, 0.0)),
        3 => (Vec3::new(x, 18.0, z), Vec3::new(0.8, -1.3, -0.45)),
        4 => (Vec3::new(x, 9.0, z), Vec3::new(-0.9, -0.3, 0.4)),
        5 => (Vec3::new(-6.0, 10.0, z), Vec3::new(1.0, -0.35, 0.07)),
        6 => (Vec3::new(x, hgt + 8.0, 11.0), Vec3::new(0.05, -0.4, -1.0)),
        7 => (Vec3::new(x, 30.0, z), Vec3::new(0.3, 1.0, 0.2)),
        8 => (Vec3::new(x, 14.0, z), Vec3::new(-0.25, -1.0, -0.8)),
        _ => (Vec3::new(x, 14.0, z), Vec3::ZERO),
    }
}

/// Casts the ray grid through [`pick_terrain`] and hashes every result by raw bits.
fn pick_hash(map: &GameMap) -> (u64, u32) {
    let mut h = Fnv64::new();
    let mut hits = 0;
    for j in 0..RAY_GRID {
        for i in 0..RAY_GRID {
            let (origin, dir) = ray(map, i, j);
            match pick_terrain(map, origin, dir) {
                Some(hit) => {
                    hits += 1;
                    h.u32(1);
                    h.u32(hit.cell.0);
                    h.u32(hit.cell.1);
                    h.f32(hit.point.x);
                    h.f32(hit.point.y);
                    h.f32(hit.point.z);
                }
                None => h.u32(0),
            }
        }
    }
    (h.0, hits)
}

/// Prints the measured values so a deliberate re-record is a copy, not a guess.
fn check(name: &str, mesh: u64, pick: (u64, u32), expected: (u64, u64, u32)) {
    println!(
        "{name}: mesh = 0x{mesh:016x}, pick = 0x{:016x}, hits = {}",
        pick.0, pick.1
    );
    assert_eq!(
        (mesh, pick.0, pick.1),
        expected,
        "{name}: terrain output changed (mesh hash, pick hash, hit count)"
    );
}

#[test]
fn default_arena_mesh_and_picking_are_bit_exact() {
    let map = load_shipped_map("default_arena.scmap.ron");
    check(
        "default_arena",
        mesh_hash(&map),
        pick_hash(&map),
        (0x0f0d_39ef_7f97_58db, 0x0d13_089f_628a_56b8, 2321),
    );
}

#[test]
fn showcase_canyon_mesh_and_picking_are_bit_exact() {
    let map = load_shipped_map("showcase_canyon.scmap.ron");
    check(
        "showcase_canyon",
        mesh_hash(&map),
        pick_hash(&map),
        (0x771c_cfc1_98c0_befd, 0xd059_17a4_edfe_bf3a, 2262),
    );
}

#[test]
fn synthetic_map_mesh_and_picking_are_bit_exact() {
    let map = synthetic_map();
    check(
        "synthetic",
        mesh_hash(&map),
        pick_hash(&map),
        (0xce78_5e16_375b_bd4c, 0xe991_3ab4_d9ba_9745, 1532),
    );
}

/// Chunks with no ramp corner in any dual cell are untouched by ramp geometry: their hashes were
/// recorded before plan `2026-11-09-ramp-geometry` WP1 changed the mesher's ramp handling, and
/// must not move with any later ramp change (plan section 4, D-3 and D-5).
#[test]
fn ramp_free_chunks_are_bit_exact() {
    let maps = [
        (
            "default_arena",
            load_shipped_map("default_arena.scmap.ron"),
            (0x1d88_581e_ad3f_f398, 21),
        ),
        (
            "showcase_canyon",
            load_shipped_map("showcase_canyon.scmap.ron"),
            (0xdbb5_9ede_15b0_9c12, 21),
        ),
        ("synthetic", synthetic_map(), (0x4b31_db42_9fac_1b7d, 1)),
    ];
    let mut changed = Vec::new();
    for (name, map, expected) in maps {
        let measured = chunk_hash(&map, |x, y| chunk_is_ramp_free(&map, x, y));
        println!(
            "{name}: ramp-free chunks = 0x{:016x} over {} chunks",
            measured.0, measured.1
        );
        if measured != expected {
            changed.push(name);
        }
    }
    assert!(
        changed.is_empty(),
        "ramp-free chunk output changed (hash, chunk count) on {changed:?}"
    );
}

/// Edge length of the seeded test textures; small so the mip chain has four levels.
const THEME_TEST_SIZE: u32 = 8;

/// The first shipped theme, re-rooted at `baked_dir = "baked"` with 8x8 textures.
fn small_theme() -> TerrainTheme {
    let path = resolve_assets_dir().join("catalog/terrain_themes.ron");
    let catalog = TerrainThemeCatalog::load(&path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", path.display()));
    let mut theme = catalog
        .themes
        .into_iter()
        .next()
        .expect("the shipped theme catalog lists at least one theme");
    theme.texture_size = THEME_TEST_SIZE;
    "baked".clone_into(&mut theme.baked_dir);
    theme
}

/// Writes seeded RGBA PNGs for every baked path of `theme` under `root`.
fn write_seeded_textures(theme: &TerrainTheme, root: &std::path::Path) {
    std::fs::create_dir_all(root.join(&theme.baked_dir)).expect("create baked dir");
    let mut rng = SplitMix64(0x7e47_0123_0009_0004);
    for layer in theme.ordered_layers() {
        let paths = theme.baked_paths(layer);
        for rel in [&paths.albedo, &paths.normal, &paths.orm] {
            let mut img = image::RgbaImage::new(THEME_TEST_SIZE, THEME_TEST_SIZE);
            for px in img.pixels_mut() {
                let bits = rng.next().to_le_bytes();
                *px = image::Rgba([bits[0], bits[1], bits[2], bits[3]]);
            }
            img.save(root.join(rel)).expect("write seeded png");
        }
    }
}

fn hash_image(h: &mut Fnv64, image: &Image) {
    let desc = &image.texture_descriptor;
    h.u32(desc.size.width);
    h.u32(desc.size.height);
    h.u32(desc.size.depth_or_array_layers);
    h.u32(desc.mip_level_count);
    h.bytes(format!("{:?}", desc.format).as_bytes());
    h.bytes(format!("{:?}", desc.dimension).as_bytes());
    h.u64(image.data.len() as u64);
    h.bytes(&image.data);
}

#[test]
fn theme_texture_arrays_are_bit_exact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let theme = small_theme();
    write_seeded_textures(&theme, dir.path());

    let textures = load_theme_texture_arrays(&theme, dir.path()).expect("seeded theme loads");
    let mut h = Fnv64::new();
    hash_image(&mut h, &textures.albedo);
    hash_image(&mut h, &textures.normal);
    hash_image(&mut h, &textures.orm);
    println!("theme arrays: 0x{:016x}", h.0);
    assert_eq!(
        h.0, 0x2b3a_25b6_f4e8_46ce,
        "terrain texture array bytes or layout changed"
    );

    // The sampler clamp follows the override variable exactly as production reads it.
    let override_value = std::env::var(TERRAIN_ANISOTROPY_OVERRIDE_ENV).ok();
    let clamp = terrain_anisotropy_clamp(override_value.as_deref()).expect("valid override");
    for image in [&textures.albedo, &textures.normal, &textures.orm] {
        let ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("terrain arrays carry an explicit sampler descriptor");
        };
        assert_eq!(sampler.anisotropy_clamp, clamp);
    }
}

#[test]
fn theme_texture_arrays_report_the_first_failure_in_load_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let theme = small_theme();
    write_seeded_textures(&theme, dir.path());
    let layers = theme.ordered_layers();
    // Load order is layer by layer, albedo, normal, orm within a layer, and every existence check
    // of a layer runs before any decode of it. Remove a file of a later layer and the last file of
    // an earlier layer, and corrupt that earlier layer's albedo: the earlier missing file must be
    // reported, not the corrupt albedo and not the later missing file.
    let later = dir.path().join(theme.baked_paths(layers[2]).albedo);
    let earlier = dir.path().join(theme.baked_paths(layers[1]).orm);
    let corrupt = dir.path().join(theme.baked_paths(layers[1]).albedo);
    std::fs::remove_file(&later).expect("remove later");
    std::fs::remove_file(&earlier).expect("remove earlier");
    std::fs::write(&corrupt, b"not a png").expect("corrupt albedo");

    match load_theme_texture_arrays(&theme, dir.path()) {
        Err(TerrainThemeError::MissingSource(path)) => assert_eq!(path, earlier),
        Err(other) => panic!("expected MissingSource({}), got {other}", earlier.display()),
        Ok(_) => panic!("a theme with missing files must not load"),
    }

    // With every file present again, the corrupt albedo is the first failure.
    write_seeded_textures(&theme, dir.path());
    std::fs::write(&corrupt, b"not a png").expect("corrupt albedo");
    match load_theme_texture_arrays(&theme, dir.path()) {
        Err(TerrainThemeError::Image(message)) => {
            assert!(
                message.contains(&corrupt.display().to_string()),
                "{message}"
            );
        }
        Err(other) => panic!("expected an Image error, got {other}"),
        Ok(_) => panic!("a theme with a corrupt file must not load"),
    }
}
