//! Grid-against-mesh measurement of the latent navigation mesh (sc2 pathfinding WP8).
//!
//! # Responsibility
//! Prints, as Markdown tables, what `docs/evidence/2026-11-09-sc2-pathfinding.md` records for
//! Part B: mesh size and build and rebuild times on both shipped maps and on a 256x256 map; and,
//! on 1,000 seeded queries per map and radius, the grid route against the mesh route: found or
//! not, length, legality under the mesh predicate, funnel fallbacks, and microseconds per search.
//!
//! # How it fits
//! A measurement tool, not a test: it times with the wall clock, which the simulation never
//! reads. Run it in release:
//! `cargo run --release -p sim_map --example navmesh_measure`.
//!
//! # Invariants
//! Queries come from a seeded `ChaCha8Rng`, so every run measures the same pairs.

#![expect(
    clippy::disallowed_methods,
    reason = "a timing harness measures the pathfinder from outside; the clock never enters \
              simulation state"
)]

use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sim_map::{
    GameMap, MapFile, NavMesh, PathRequest, PathRequestWithVision, Pathfinder, UnitClearance,
};
use sim_types::{EntityId, SimScalar, SimVec2};

const QUERIES: usize = 1_000;

fn load(file: &str) -> Result<GameMap, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps")
        .join(file);
    Ok(MapFile::load(&path)?.decode()?.to_game_map()?)
}

fn median_micros(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    samples.get(samples.len() / 2).copied().unwrap_or(0)
}

fn time_build(map: &GameMap, repeats: usize) -> Result<u128, Box<dyn Error>> {
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let started = Instant::now();
        let mesh = NavMesh::build(map)?;
        samples.push(started.elapsed().as_micros());
        std::hint::black_box(mesh);
    }
    Ok(median_micros(samples))
}

/// Mesh size, build time, and rebuild time after placing `structures` 3x3 structures.
fn build_row(name: &str, map: &GameMap, structures: u32) -> Result<(), Box<dyn Error>> {
    let mesh = NavMesh::build(map)?;
    let build = time_build(map, 21)?;
    let mut built = map.clone();
    let mut placed = 0;
    let origins = (4..map.height().saturating_sub(4))
        .step_by(9)
        .flat_map(|y| {
            (4..map.width().saturating_sub(4))
                .step_by(9)
                .map(move |x| (x, y))
        });
    for (x, y) in origins {
        if placed < structures && built.can_place_building(x, y, 3, 3).is_ok() {
            built.place_structure(EntityId(50_000 + placed), x, y, 3, 3)?;
            placed += 1;
        }
    }
    let rebuilt = NavMesh::build(&built)?;
    let rebuild = time_build(&built, 21)?;
    let mut pathfinder = Pathfinder::new(&built);
    let mut samples = Vec::new();
    for _ in 0..11 {
        let started = Instant::now();
        pathfinder.recompute_clearance(&built);
        samples.push(started.elapsed().as_micros());
    }
    println!(
        "| {name} | {} / {} | {build} | {placed} | {} / {} | {rebuild} | {} |",
        mesh.vertex_count(),
        mesh.triangle_count(),
        rebuilt.vertex_count(),
        rebuilt.triangle_count(),
        median_micros(samples),
    );
    Ok(())
}

/// A 256x256 map made of 4x4 copies of the showcase canyon.
fn tiled(map: &GameMap) -> Result<GameMap, Box<dyn Error>> {
    let (w, h) = (map.width(), map.height());
    let mut cells = Vec::with_capacity((16 * w * h) as usize);
    for y in 0..4 * h {
        for x in 0..4 * w {
            cells.push(map.cell(x % w, y % h)?.clone());
        }
    }
    Ok(GameMap::from_cells(
        4 * w,
        4 * h,
        cells,
        Vec::new(),
        Vec::new(),
    )?)
}

#[derive(Default)]
struct Differential {
    both: u32,
    grid_only: u32,
    mesh_only: u32,
    neither: u32,
    longer_than_one_percent: u32,
    longer_than_five_percent: u32,
    illegal_grid_routes: u32,
    longer_than_legal_grid: u32,
    illegal_mesh_legs: u32,
    fallbacks: u32,
    sum_ratio: SimScalar,
    worst_ratio: SimScalar,
    gains: Vec<(SimScalar, SimVec2, SimVec2)>,
    grid_micros: Vec<u128>,
    mesh_micros: Vec<u128>,
    max_expansions: u32,
}

fn standable(map: &GameMap, rng: &mut ChaCha8Rng) -> SimVec2 {
    loop {
        let (x, y) = (
            rng.gen_range(0..map.width()),
            rng.gen_range(0..map.height()),
        );
        let open = map
            .cell(x, y)
            .is_ok_and(|c| c.occupying_structure.is_none());
        if open && map.terrain_step_is_connected((x, y), (x, y)) {
            return SimVec2::new(
                SimScalar::from_num(x) + SimScalar::HALF,
                SimScalar::from_num(y) + SimScalar::HALF,
            );
        }
    }
}

fn differential(map: &GameMap, radius: SimScalar, seed: u64) -> Differential {
    let mut out = Differential::default();
    let mut pathfinder = Pathfinder::new(map);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let clearance = UnitClearance::for_radius(radius);
    for _ in 0..QUERIES {
        let (start, goal) = (standable(map, &mut rng), standable(map, &mut rng));
        let grid_request = PathRequest {
            start,
            goal,
            clearance,
            radius,
        };
        let started = Instant::now();
        let grid = pathfinder.find_path_grid(map, &grid_request.into());
        out.grid_micros.push(started.elapsed().as_micros());
        let request: PathRequestWithVision<'_> = grid_request.into();
        let started = Instant::now();
        let mesh = pathfinder.find_route_mesh(map, &request);
        out.mesh_micros.push(started.elapsed().as_micros());
        match (&grid, &mesh) {
            (Ok(grid), Ok(mesh)) => {
                out.both += 1;
                let (g, m) = (grid.total_length, mesh.total_length);
                let ratio = if g > SimScalar::ZERO {
                    m / g
                } else {
                    SimScalar::ONE
                };
                out.sum_ratio += ratio;
                out.worst_ratio = out.worst_ratio.max(ratio);
                let longer = ratio > SimScalar::from_num(101) / 100;
                out.longer_than_one_percent += u32::from(longer);
                out.longer_than_five_percent += u32::from(ratio > SimScalar::from_num(105) / 100);
                // The grid's legs are checked against cell clearance only; under the exact disc
                // predicate many of them cut a corner, which no legal route can match.
                let grid_legal = grid
                    .waypoints
                    .windows(2)
                    .all(|pair| pathfinder.segment_is_passable(map, &request, pair[0], pair[1]));
                out.illegal_grid_routes += u32::from(!grid_legal);
                out.longer_than_legal_grid += u32::from(longer && grid_legal);
                out.gains.push((g - m, start, goal));
                out.fallbacks += u32::from(mesh.fallback);
                out.max_expansions = out.max_expansions.max(mesh.expansions);
                let legal = mesh
                    .waypoints
                    .windows(2)
                    .all(|pair| pathfinder.segment_is_passable(map, &request, pair[0], pair[1]));
                out.illegal_mesh_legs += u32::from(!legal);
            }
            (Ok(_), Err(_)) => out.grid_only += 1,
            (Err(_), Ok(_)) => out.mesh_only += 1,
            (Err(_), Err(_)) => out.neither += 1,
        }
    }
    out
}

fn report(name: &str, radius: SimScalar, d: &mut Differential) {
    println!(
        "| {name} | {radius} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        d.both,
        d.grid_only,
        d.mesh_only,
        d.neither,
        d.sum_ratio / i32::try_from(d.both.max(1)).unwrap_or(1),
        d.worst_ratio,
        d.longer_than_one_percent,
        d.longer_than_five_percent,
        d.illegal_grid_routes,
        d.longer_than_legal_grid,
        d.illegal_mesh_legs,
        d.fallbacks,
        median_micros(d.grid_micros.clone()),
        median_micros(d.mesh_micros.clone()),
        d.max_expansions,
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let arena = load("default_arena.scmap.ron")?;
    let canyon = load("showcase_canyon.scmap.ron")?;
    let big = tiled(&canyon)?;
    println!("| Map | Vertices / triangles | Build (µs) | Structures | With structures: vertices / triangles | Rebuild (µs) | `recompute_clearance` (µs) |");
    println!("|---|---:|---:|---:|---:|---:|---:|");
    build_row("default_arena 64x64", &arena, 6)?;
    build_row("showcase_canyon 64x64", &canyon, 6)?;
    build_row("canyon 4x4 tiled 256x256", &big, 40)?;
    println!();
    println!("| Map | Radius | Both | Grid only | Mesh only | Neither | Mean mesh/grid | Worst | > 1 % | > 5 % | Illegal grid | > 1 % over a legal grid route | Illegal mesh | Fallbacks | Grid µs (median) | Mesh µs (median) | Max expansions |");
    println!(
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    );
    let mut all_gains = Vec::new();
    for (name, map) in [("default_arena", &arena), ("showcase_canyon", &canyon)] {
        for radius in [SimScalar::from_num(3) / 8, SimScalar::HALF] {
            let mut d = differential(
                map,
                radius,
                0x9a7f_0000 + u64::from(radius == SimScalar::HALF),
            );
            report(name, radius, &mut d);
            all_gains.extend(
                d.gains
                    .drain(..)
                    .map(|(gain, s, g)| (gain, name, radius, s, g)),
            );
        }
    }
    all_gains.sort_by_key(|gain| std::cmp::Reverse(gain.0));
    println!();
    println!("| Gain (grid - mesh) | Map | Radius | Start | Goal |");
    println!("|---:|---|---:|---|---|");
    for (gain, name, radius, s, g) in all_gains.iter().take(10) {
        println!("| {gain} | {name} | {radius} | {s:?} | {g:?} |");
    }
    Ok(())
}
