//! Measurement of the lane-restricted route search (plan `docs/plans/2026-11-10-group-routing.md`,
//! WP2).
//!
//! # Responsibility
//! Prints, as Markdown tables, each shipped map's lane cuts and `K`, and the cost of the
//! restricted search against the unrestricted one on 1,000 seeded queries per map and radius:
//! median and worst microseconds and expansions of the unrestricted search, of the restricted
//! search asked for the route's own lane, and of the restricted search asked for every other
//! one-letter lane (the other side of each cut), with how many of those it found.
//!
//! # How it fits
//! A measurement tool, not a test: it times with the wall clock, which the simulation never
//! reads. Run it in release: `cargo run --release -p sim_map --example lane_measure`.
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
use sim_map::{GameMap, LaneWord, MapFile, PathRequest, PathRequestWithVision, Pathfinder};
use sim_types::{SimScalar, SimVec2};

const QUERIES: usize = 1_000;
const SEED: u64 = 0x1a7e_3ea5_0000_0001;

fn load(file: &str) -> Result<GameMap, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps")
        .join(file);
    Ok(MapFile::load(&path)?.decode()?.to_game_map()?)
}

fn standable(map: &GameMap, rng: &mut ChaCha8Rng) -> SimVec2 {
    loop {
        let (x, y) = (
            rng.gen_range(0..map.width()),
            rng.gen_range(0..map.height()),
        );
        if map.terrain_step_is_connected((x, y), (x, y)) {
            return SimVec2::new(
                SimScalar::from_num(x) + SimScalar::HALF,
                SimScalar::from_num(y) + SimScalar::HALF,
            );
        }
    }
}

/// Timings and expansions of one kind of search.
#[derive(Default)]
struct Samples {
    micros: Vec<u128>,
    expansions: Vec<u32>,
    found: u32,
}

impl Samples {
    fn row(&self, name: &str, kind: &str) {
        let median = |mut v: Vec<u128>| {
            v.sort_unstable();
            v.get(v.len() / 2).copied().unwrap_or(0)
        };
        let mut expansions = self.expansions.clone();
        expansions.sort_unstable();
        println!(
            "| {name} | {kind} | {} | {} | {} | {} | {} | {} |",
            self.micros.len(),
            self.found,
            median(self.micros.clone()),
            self.micros.iter().max().copied().unwrap_or(0),
            expansions.get(expansions.len() / 2).copied().unwrap_or(0),
            expansions.last().copied().unwrap_or(0),
        );
    }
}

/// Runs `search` once, timing it.
fn timed<T>(samples: &mut Samples, search: impl FnOnce() -> Option<T>) -> Option<T> {
    let started = Instant::now();
    let result = search();
    samples.micros.push(started.elapsed().as_micros());
    samples.found += u32::from(result.is_some());
    result
}

fn measure(name: &str, map: &GameMap, radius: SimScalar) {
    let mut pathfinder = Pathfinder::new(map);
    let letters: Vec<i32> = pathfinder
        .lane_cuts()
        .cuts()
        .iter()
        .flat_map(|cut| {
            let letter = i32::try_from(cut.obstacle).unwrap_or(i32::MAX - 1) + 1;
            [letter, -letter]
        })
        .collect();
    let mut lanes: Vec<LaneWord> = vec![LaneWord::empty()];
    lanes.extend(letters.iter().map(|&l| LaneWord::from_letters(&[l])));
    let (mut free, mut own, mut other) =
        (Samples::default(), Samples::default(), Samples::default());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    for _ in 0..QUERIES {
        let (start, goal) = (standable(map, &mut rng), standable(map, &mut rng));
        let request: PathRequestWithVision<'_> = PathRequest {
            start,
            goal,
            radius,
        }
        .into();
        let Some(route) = timed(&mut free, || pathfinder.find_route_mesh(map, &request).ok())
        else {
            continue;
        };
        free.expansions.push(route.expansions);
        let word = pathfinder.lane_word(&route.waypoints);
        if let Some(r) = timed(&mut own, || {
            pathfinder
                .find_route_mesh_in_lane(map, &request, &word)
                .ok()
        }) {
            own.expansions.push(r.expansions);
        }
        for lane in lanes.iter().filter(|lane| **lane != word) {
            if let Some(r) = timed(&mut other, || {
                pathfinder.find_route_mesh_in_lane(map, &request, lane).ok()
            }) {
                other.expansions.push(r.expansions);
            }
        }
    }
    let label = format!("{name} r {radius}");
    free.row(&label, "unrestricted");
    own.row(&label, "restricted, own lane");
    other.row(&label, "restricted, other one-letter lanes");
}

fn main() -> Result<(), Box<dyn Error>> {
    let maps = [
        ("showcase_canyon", load("showcase_canyon.scmap.ron")?),
        ("default_arena", load("default_arena.scmap.ron")?),
    ];
    println!("| Map | Cuts | K | Cuts (x, bottom, top) |");
    println!("|---|---:|---:|---|");
    for (name, map) in &maps {
        let pathfinder = Pathfinder::new(map);
        let cuts: Vec<String> = pathfinder
            .lane_cuts()
            .cuts()
            .iter()
            .map(|c| format!("({}, {}, {})", c.x, c.bottom, c.top))
            .collect();
        println!(
            "| {name} | {} | {} | {} |",
            pathfinder.lane_cut_count(),
            pathfinder.lane_edge_bound(),
            cuts.join(" "),
        );
    }
    println!();
    println!("| Map | Search | Queries | Found | µs median | µs max | Expansions median | Expansions max |");
    println!("|---|---|---:|---:|---:|---:|---:|---:|");
    for (name, map) in &maps {
        for radius in [SimScalar::from_raw(3 << 29), SimScalar::from_raw(1 << 31)] {
            measure(name, map, radius);
        }
    }
    Ok(())
}
