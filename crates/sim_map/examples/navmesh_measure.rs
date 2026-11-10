//! Measurement of the navigation mesh (sc2 pathfinding WP8; grid comparison retired in WP10).
//!
//! # Responsibility
//! Prints, as Markdown tables, what `docs/evidence/2026-11-09-sc2-pathfinding.md` records for
//! Part B: mesh size and build and rebuild times on both shipped maps and on a 256x256 map; and,
//! on 1,000 seeded queries per map and radius, the mesh route: found or not, legality under the
//! mesh predicate, funnel fallbacks, and microseconds per search. WP10 removed the grid corridor
//! search, so the grid-against-mesh columns of WP8's tables are no longer produced; the evidence
//! keeps their last measurement.
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
use sim_map::{GameMap, MapFile, NavMesh, PathRequest, PathRequestWithVision, Pathfinder};
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

/// Route statistics of the mesh on one map and radius.
#[derive(Default)]
struct RouteStats {
    found: u32,
    not_found: u32,
    illegal_mesh_legs: u32,
    fallbacks: u32,
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

fn route_stats(map: &GameMap, radius: SimScalar, seed: u64) -> RouteStats {
    let mut out = RouteStats::default();
    let mut pathfinder = Pathfinder::new(map);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    for _ in 0..QUERIES {
        let (start, goal) = (standable(map, &mut rng), standable(map, &mut rng));
        let request: PathRequestWithVision<'_> = PathRequest {
            start,
            goal,
            radius,
        }
        .into();
        let started = Instant::now();
        let mesh = pathfinder.find_route_mesh(map, &request);
        out.mesh_micros.push(started.elapsed().as_micros());
        match &mesh {
            Ok(mesh) => {
                out.found += 1;
                out.fallbacks += u32::from(mesh.fallback);
                out.max_expansions = out.max_expansions.max(mesh.expansions);
                let legal = mesh
                    .waypoints
                    .windows(2)
                    .all(|pair| pathfinder.segment_is_passable(map, &request, pair[0], pair[1]));
                out.illegal_mesh_legs += u32::from(!legal);
            }
            Err(_) => out.not_found += 1,
        }
    }
    out
}

fn report(name: &str, radius: SimScalar, d: &RouteStats) {
    println!(
        "| {name} | {radius} | {} | {} | {} | {} | {} | {} |",
        d.found,
        d.not_found,
        d.illegal_mesh_legs,
        d.fallbacks,
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
    println!("| Map | Radius | Found | Not found | Illegal mesh | Fallbacks | Mesh µs (median) | Max expansions |");
    println!("|---|---:|---:|---:|---:|---:|---:|---:|");
    for (name, map) in [("default_arena", &arena), ("showcase_canyon", &canyon)] {
        for radius in [SimScalar::from_num(3) / 8, SimScalar::HALF] {
            let d = route_stats(
                map,
                radius,
                0x9a7f_0000 + u64::from(radius == SimScalar::HALF),
            );
            report(name, radius, &d);
        }
    }
    Ok(())
}
