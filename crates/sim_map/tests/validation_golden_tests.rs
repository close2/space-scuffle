//! Bit-exact characterisation of map validation, map decoding and vision-aware pathfinding.
//!
//! # Responsibility
//! Pins the observable output of three large `sim_map` entry points, `validate_map`,
//! `MapFile::decode` and `Pathfinder::find_path_with_vision`, so that their Phase 9 step 9.2
//! split into smaller functions is proven behaviour-preserving. Each test drives its entry point
//! over a fixed, seeded corpus and compares one `xxh64` digest with a constant recorded on the
//! code before the split.
//!
//! # Corpus
//! - Validation: both shipped maps unchanged, the default arena with one cell missing, the
//!   default arena walled in two by an abyss band, then 200 documents derived from
//!   `assets/maps/default_arena.scmap.ron` by a `ChaCha8Rng`-seeded mutator that flips terrain,
//!   ramps, cliff levels and flags, moves start slots, footprints, resources and watchtowers,
//!   zeroes amounts and radii, and duplicates entity IDs. Every report's `Debug` text is hashed
//!   in corpus order.
//! - Decoding: the 203 well-shaped documents of that corpus encoded to `MapFile`, damaged at the file level (bad
//!   dimensions, missing or overlong rows, invalid characters), serialised to RON and parsed back,
//!   then decoded. Every result's `Debug` text is hashed.
//! - Pathfinding: 250 seeded requests on each shipped map, with structures placed, mixing plain
//!   requests, partial player vision with and without unexplored-ramp blocking, and a known
//!   structure mask. Waypoints and lengths are hashed by their raw fixed-point bits.
//!
//! # Invariants
//! - The corpus is a pure function of the seeds below and the shipped map files, on every
//!   target: every draw names a fixed-width type, and an index is drawn as a `u32` and converted
//!   after the draw (B-140; charter check C25).
//! - Coverage assertions prove the corpus reaches the checks a split could reorder, so a golden
//!   match is meaningful (for example, at least one report holds both a blocked start footprint
//!   and two starts that are too close, the adjacent checks 13 and 14).
//! - A digest change means the observable output changed. Re-record only for an intended
//!   behaviour change, and say so in the commit.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/arena.rs"]
mod arena;

#[path = "support/draw.rs"]
mod draw;
use draw::draw_index;

#[path = "support/radii.rs"]
mod radii;
use radii::{LARGE_RADIUS, MEDIUM_RADIUS, SMALL_RADIUS};

use std::collections::BTreeSet;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_map::{
    validate_map, CellFlags, CliffLevel, GameMap, MapDocument, MapEntityId, MapFile,
    MapFormatError, MapValidationError, PathRequest, PathRequestWithVision, PathResult, Pathfinder,
    PlayerVisionMap, RampDir, TerrainKind, VisionSource,
};
use sim_types::{EntityId, MoverType, SimScalar, SimVec2};
use xxhash_rust::xxh64::xxh64;
use xxhash_rust::xxh64::Xxh64;

/// Seed of the document mutator.
const DOCUMENT_SEED: u64 = 0x0009_2a11_da7e_5eed;
/// Seed of the file-level damage applied before decoding.
const DECODE_SEED: u64 = 0x0009_2dec_0de5_eed5;
/// Seed of the path request generator.
const PATH_SEED: u64 = 0x0009_2a7e_45ee_d500;
/// Digest of every path result over a corpus with seeded 5.0.14 contour structures (plan
/// `2026-11-18-contour-footprints` WP3), pinned when the contour cut landed. Its draws use only
/// fixed-width integer ranges, so the same corpus is drawn on 32-bit and 64-bit targets.
///
/// Re-recorded for plan `2026-11-19-funnel-fallbacks` WP2 (D-3, obstacle vertices near a leg;
/// was `0x9069_95c7_29b7_3498`, which the tree reproduces with D-3 switched off). Two of the 500
/// requests change: a wedged start on the showcase (r 0.75) now rounds an obstacle vertex outside
/// its channel, legal both ways, 53.72 -> 53.54 long; and a partial-knowledge request (r 0.625)
/// that falls back both ways through another channel's midpoints (B-146). The new value is the
/// same in native debug, native release and `wasm32-wasip1` release.
///
/// Re-recorded for WP3 of the same plan (D-2, the zero-room retry, with Amendments 1 and 2; was
/// `0x6637_d70a_80d8_a859`, which the tree reproduces with D-2 switched off, and
/// `0x9069_95c7_29b7_3498` with D-2 and D-3 off). One of the 500 requests changes: a class E
/// zero-room pinch on the showcase under partial knowledge, r 1.0, (34.5, 23.5) -> (50.0, 47.5),
/// which fell back to the portal midpoints (91.45 long) and is now legal (51.27). The new value
/// is the same in native debug and release and in `wasm32-wasip1` release and debug.
///
/// Re-recorded for plan `2026-11-19-partial-knowledge-funnel` WP2 (D-1, D-4, D-5; before,
/// `0x36ff_bf4b_e967_0683`, which the tree reproduces with the rules switched off). 43 of the 500
/// requests change, all under partial knowledge: 12 fallbacks become legal routes, one wedged-start
/// fallback falls back through another channel's midpoints, and 30 legal routes get shorter (least
/// 0.913, median 0.9989); none gets longer and none starts to fall back. The new value is the
/// same in native debug and release and in `wasm32-wasip1` release and debug.
///
/// Re-recorded for WP3 of the same plan (D-2, D-3; before, `0x01f9_a949_e9c0_77ef`, which the tree
/// reproduces with D-2 and D-3 off, and `0x36ff_bf4b_e967_0683` with every rule off). Against
/// `main` 42 requests change: 15 fallbacks become legal, 27 legal routes get shorter; none gets
/// longer and none starts to fall back. The new value is the same in native debug and release and
/// in `wasm32-wasip1` release and debug (Amendment 1 applied).
///
/// Re-recorded for plan `2026-11-20-tastar-budget` WP2 (D-1, TA*'s channel-geodesic bound;
/// before, `0x1da8_6365_bcbd_3b37`, which the measurement tree reproduces with D-1 switched off).
/// Of the 258 requests TA* routes, 16 change, none all-known: one wedged-start fallback becomes
/// legal ((16.75, 24.5) -> (58, 53.5), r 1.0, 81.457 by midpoints -> 57.897), 13 legal routes get
/// shorter, and 2 get longer: (49.5, 57.5) -> (4.5, 0.5), r 0.5, fitting, 87.626 -> 88.919
/// (+1.48 %), and the wedged (26.75, 25.5) -> (54.5, 18.5), r 1.0, 30.972 -> 32.416 (+4.66 %),
/// both the visit cap's choice under the new order (B-154). None starts to fall back. The new
/// value is the same in native debug and release and in `wasm32-wasip1` release and debug.
const CONTOUR_PATH_GOLDEN: u64 = 0x30cd_bf6a_3de1_a900;
/// Documents at the head of the validation corpus: both shipped maps, the short grid and the
/// walled arena.
const FIXED_DOCUMENTS: usize = 4;
/// Number of mutated documents derived from the default arena.
const MUTATED_DOCUMENTS: usize = 200;
/// Path requests issued on each shipped map.
const PATH_REQUESTS_PER_MAP: usize = 250;

/// Digest of every validation report over the corpus, recorded before the step 9.2 split
/// (`0xf028_1611_929c_52d1`). Re-recorded for plan `2026-11-18-contour-footprints` WP2 (D-5:
/// a footprint cell must be walkable ground): 17 start footprints the corpus already refused as
/// `UnbuildableTerrain` (8 of start 30, 9 of start 31) are now refused as `UnpathableTerrain`;
/// no start footprint is newly refused, and every other report is unchanged. A scratch build
/// without the rule reproduces the old value.
///
/// Re-recorded for B-140 (plan `2026-11-18-contour-footprints` Amendment 5; before,
/// `0xc8fc_3cbd_495d_5177`): the corpus indexed with `usize` draws, which `rand` samples by the
/// pointer width, so 64-bit targets and `wasm32` validated different corpora. Every index is
/// now a `u32` draw, and the walled arena reaches check 22 by construction (the new corpus,
/// like the old `wasm32` one, has no seeded document with unreachable starts). Native debug,
/// native release and `wasm32-wasip1` release produce this same value, with the same counts:
/// 13 and 14 together in 15 reports, unreachable in 1, 2,496 warnings.
///
/// Re-recorded for plan `2026-11-19-footprint-flags` WP4 (D-8, Amendment 1; before,
/// `0x3527_8c7f_a705_ba2c`): five gas nodes of the shipped maps move by one or two cells, out of
/// their start's `NearResources` reach, and the corpus is built from those maps, so the reports
/// that print node cells changed. Proof: the same build with the previous map files reproduces
/// the previous value; with the moved maps the variant set and the counts (13 and 14 together in
/// 15 reports, unreachable in 1, 2,496 warnings) are unchanged. Native debug and release and
/// `wasm32-wasip1` release and debug produce this same value.
///
/// Re-recorded for plan `2026-11-20-resource-footprints` WP3 (D-10; before,
/// `0x78f4_a8ea_2deb_b63b`): `default_arena`'s mineral lines and two gas nodes are re-laid for
/// 2×1 fields and 3×3 geysers, and the corpus is built from that map, so the reports that print
/// node cells changed. Proof: this tree with the previous `default_arena` reproduces the previous
/// value. With the re-laid map the variant set and the error counts (13 and 14 together in 15
/// reports, unreachable in 1) are unchanged; warnings rise from 2,496 to 2,790, two more
/// `UnassignedResource` per arena document (147 documents): the slot 0 line moved west and slot
/// 1's north, beyond the association radius of the start (`validate_map` on the shipped arena:
/// 8 unassigned before, 10 after). Native debug and release and `wasm32-wasip1` release and debug
/// produce this same value.
const VALIDATION_GOLDEN: u64 = 0x4dbe_8075_1245_0f9e;
/// Digest of every decode result over the corpus, recorded before the step 9.2 split
/// (`0x56c4_3eea_8701_94bd`). Re-recorded for B-140 (plan `2026-11-18-contour-footprints`
/// Amendment 5): the document and damage draws are `u32` instead of `usize`, and the corpus
/// gains the walled arena. Native debug, native release and `wasm32-wasip1` release produce
/// this same value.
///
/// Re-recorded for plan `2026-11-19-footprint-flags` WP4 (D-8, Amendment 1; before,
/// `0x4c32_25d2_ef3a_fc87`): the decoded documents hold the moved gas nodes. Proof: the previous
/// map files reproduce the previous value, and the failure set is unchanged. Native debug and
/// release and `wasm32-wasip1` release and debug produce this same value.
///
/// Re-recorded for plan `2026-11-20-resource-footprints` WP3 (D-10; before,
/// `0x10db_ea43_6aba_f292`): the decoded documents hold the re-laid nodes. Proof: this tree with
/// the previous `default_arena` reproduces the previous value, and the failure set is unchanged.
/// Native debug and release and `wasm32-wasip1` release and debug produce this same value.
const DECODE_GOLDEN: u64 = 0x8aa3_3f5e_c0af_266e;
/// Digest of every path result over the corpus. Re-recorded for sc2 pathfinding WP9: routes
/// come from the navigation mesh at the request's radius (before, the grid corridor:
/// `0x52cf_7f61_9e6e_88b1`, recorded before the step 9.2 split). Re-recorded under Amendment 4:
/// the switch commit's `0x574b_ceb4_0a08_2c4d` was recorded before the last edits of that
/// commit, whose own tree already produced this value.
///
/// Re-recorded for B-140 (plan `2026-11-18-contour-footprints` Amendment 5; before,
/// `0xa1a3_327e_2a30_f8b7`): the radius index is a `u32` draw instead of a `usize` one. The new
/// value is the one `wasm32-wasip1` already produced on the old corpus, since `rand` drew that
/// `usize` as a `u32` there: routing never differed, the corpus did. Native debug, native
/// release and `wasm32-wasip1` release produce this same value.
///
/// Re-recorded for plan `2026-11-19-partial-knowledge-funnel` WP2 (D-1 free portal vertices
/// pulled as points, D-4, D-5; before, `0xa243_924f_8125_f760`, which the tree reproduces with
/// the rules switched off). 49 of the 500 requests change, all under partial knowledge: 11
/// fallbacks become legal routes (2 from wedged starts), and 38 legal routes get shorter (least
/// 0.823 of their length, median 0.9967); none gets longer and none starts to fall back. The new
/// value is the same in native debug and release and in `wasm32-wasip1` release and debug.
///
/// Re-recorded for WP3 of the same plan (D-2 the clamp, D-3 clearance corners kept round; before,
/// `0x254e_64e8_49c1_5bd2`, which the tree reproduces with D-2 and D-3 off, and
/// `0xa243_924f_8125_f760` with every rule off). Against `main` 49 requests change: 10 fallbacks
/// become legal, one wedged-start fallback falls back through other midpoints (57.93 -> 64.72), 38
/// legal routes get shorter; none gets longer. The new value is the same in native debug and
/// release and in `wasm32-wasip1` release and debug (Amendment 1 applied).
///
/// Re-recorded for plan `2026-11-20-tastar-budget` WP2 (D-1, TA*'s channel-geodesic bound;
/// before, `0x4062_b0b6_83dd_1afb`, which the measurement tree reproduces with D-1 switched off).
/// Of the 174 requests TA* routes, 12 change, none all-known: 11 legal routes get shorter and one
/// gets longer, (17.5, 6.5) -> (18.25, 54.5), r 1.0, fitting, 58.471 -> 58.611 (+0.24 %), the
/// visit cap's choice under the new order (B-154). No fallback changes (3, all wedged starts).
/// The new value is the same in native debug and release and in `wasm32-wasip1` release and debug.
const PATH_GOLDEN: u64 = 0xf322_b17b_9841_7b5e;

/// Builds the validation corpus: both shipped maps, then the seeded mutations.
fn document_corpus() -> Vec<MapDocument> {
    let base = arena::load_document(arena::DEFAULT_ARENA_FILE);
    let mut short_grid = base.clone();
    short_grid.cells.pop();
    // An abyss band across the whole map between the two starts (rows 20 and 21; the starts'
    // footprints cover rows 4 to 8 and 52 to 56), so check 22 is reached by construction.
    let mut walled = base.clone();
    paint_band(&mut walled, CliffLevel(0), true, true, 20, 2);
    let mut corpus = vec![
        base.clone(),
        arena::load_document(arena::SHOWCASE_CANYON_FILE),
        short_grid,
        walled,
    ];
    let mut rng = ChaCha8Rng::seed_from_u64(DOCUMENT_SEED);
    for _ in 0..MUTATED_DOCUMENTS {
        let mut doc = base.clone();
        let mutations = rng.gen_range::<i32, _>(1..=4);
        for _ in 0..mutations {
            mutate_document(&mut doc, &mut rng);
        }
        corpus.push(doc);
    }
    corpus
}

/// Terrain kinds a mutation paints on one cell.
const MUTATED_TERRAINS: [TerrainKind; 4] = [
    TerrainKind::Grass,
    TerrainKind::MetalPlate,
    TerrainKind::Dirt,
    TerrainKind::Abyss,
];
/// Ramp directions (or none) a mutation sets on one cell.
const MUTATED_RAMPS: [Option<RampDir>; 5] = [
    None,
    Some(RampDir::North),
    Some(RampDir::South),
    Some(RampDir::East),
    Some(RampDir::West),
];
/// Cell flags a mutation toggles on one cell.
const MUTATED_FLAGS: [CellFlags; 4] = [
    CellFlags::BUILDABLE,
    CellFlags::GROUND_PATHABLE,
    CellFlags::LOS_BLOCKER,
    CellFlags::SPEED_ZONE,
];

/// Applies one seeded mutation to `doc`.
fn mutate_document(doc: &mut MapDocument, rng: &mut ChaCha8Rng) {
    let width = doc.width;
    let height = doc.height;
    match rng.gen_range::<i32, _>(0..20) {
        0 => {
            let terrain = MUTATED_TERRAINS[draw_index(rng, MUTATED_TERRAINS.len())];
            random_cell(doc, rng).terrain = terrain;
        }
        1 => {
            let ramp = MUTATED_RAMPS[draw_index(rng, MUTATED_RAMPS.len())];
            random_cell(doc, rng).ramp = ramp;
        }
        2 => random_cell(doc, rng).level = CliffLevel(rng.gen_range::<u8, _>(0..=3)),
        3 => {
            let flag = MUTATED_FLAGS[draw_index(rng, MUTATED_FLAGS.len())];
            random_cell(doc, rng).flags.toggle(flag);
        }
        4 => {
            if let Some(start) = pick(&mut doc.start_locations, rng) {
                start.cell = (
                    rng.gen_range::<u32, _>(0..width + 6),
                    rng.gen_range::<u32, _>(0..height + 6),
                );
            }
        }
        5 => {
            if let Some(start) = pick(&mut doc.start_locations, rng) {
                start.footprint = (rng.gen_range::<u8, _>(0..9), rng.gen_range::<u8, _>(0..9));
            }
        }
        6 => {
            if let Some(start) = pick(&mut doc.start_locations, rng) {
                // Slots 0 and 1 are the shipped ones, so these duplicate a slot or leave the range.
                start.player_slot = [0, 1, 7, 8, 200][draw_index(rng, 5)];
            }
        }
        7 => {
            let occupied = doc.watchtowers.first().map(|tower| tower.cell);
            if let Some(resource) = pick(&mut doc.resources, rng) {
                resource.cell = match occupied {
                    Some(cell) if rng.gen_bool(0.3) => cell,
                    _ => (
                        rng.gen_range::<u32, _>(0..width + 4),
                        rng.gen_range::<u32, _>(0..height + 4),
                    ),
                };
            }
        }
        8 => {
            if let Some(resource) = pick(&mut doc.resources, rng) {
                resource.amount = 0;
            }
        }
        9 => duplicate_entity_id(doc, rng),
        10 => {
            if let Some(tower) = pick(&mut doc.watchtowers, rng) {
                tower.capture_radius_tenths = [0, 1, 30, 400][draw_index(rng, 4)];
                tower.reveal_radius_tenths = [0, 120, 4000, u16::MAX][draw_index(rng, 4)];
            }
        }
        11 => {
            if let Some(tower) = pick(&mut doc.watchtowers, rng) {
                tower.cell = (
                    rng.gen_range::<u32, _>(0..width + 4),
                    rng.gen_range::<u32, _>(0..height + 4),
                );
            }
        }
        12 => {
            if !doc.start_locations.is_empty() {
                let index = draw_index(rng, doc.start_locations.len());
                doc.start_locations.remove(index);
            }
        }
        13 => paint_level_band(doc, rng),
        14 => crowd_starts(doc, rng),
        15 => resource_into_start(doc, rng),
        16 => ramp_column(doc, rng),
        17 => {
            // The shipped ramp spans x 30..34, y 30..34; break the level of one of its cells.
            let cell = doc
                .cell_mut(
                    rng.gen_range::<u32, _>(30..34),
                    rng.gen_range::<u32, _>(31..34),
                )
                .expect("invariant: the shipped ramp lies inside the 64x64 grid");
            cell.level = CliffLevel(rng.gen_range::<u8, _>(0..=3));
        }
        18 => {
            let cell = random_cell(doc, rng);
            cell.level = CliffLevel::MAX;
            cell.ramp = Some(RampDir::East);
        }
        _ => {
            if let Some(tower) = pick(&mut doc.watchtowers, rng) {
                tower.id = MapEntityId(u32::MAX - rng.gen_range::<u32, _>(0..4));
            }
        }
    }
}

/// A uniformly chosen cell of `doc`.
fn random_cell<'d>(doc: &'d mut MapDocument, rng: &mut ChaCha8Rng) -> &'d mut sim_map::MapCellSpec {
    let index = draw_index(rng, doc.cells.len());
    &mut doc.cells[index]
}

/// A uniformly chosen element of `items`, or `None` when it is empty.
fn pick<'v, T>(items: &'v mut [T], rng: &mut ChaCha8Rng) -> Option<&'v mut T> {
    if items.is_empty() {
        None
    } else {
        let index = draw_index(rng, items.len());
        Some(&mut items[index])
    }
}

/// Gives one entity the ID of another, across resources, watchtowers and start locations.
fn duplicate_entity_id(doc: &mut MapDocument, rng: &mut ChaCha8Rng) {
    let ids: Vec<MapEntityId> = doc
        .resources
        .iter()
        .map(|r| r.id)
        .chain(doc.watchtowers.iter().map(|w| w.id))
        .chain(doc.start_locations.iter().map(|s| s.id))
        .collect();
    if ids.is_empty() {
        return;
    }
    let id = ids[draw_index(rng, ids.len())];
    match rng.gen_range::<i32, _>(0..3) {
        0 => {
            if let Some(resource) = pick(&mut doc.resources, rng) {
                resource.id = id;
            }
        }
        1 => {
            if let Some(tower) = pick(&mut doc.watchtowers, rng) {
                tower.id = id;
            }
        }
        _ => {
            if let Some(start) = pick(&mut doc.start_locations, rng) {
                start.id = id;
            }
        }
    }
}

/// Paints a full-width or full-height band of one cliff level, which can wall bases apart.
fn paint_level_band(doc: &mut MapDocument, rng: &mut ChaCha8Rng) {
    let level = CliffLevel(rng.gen_range::<u8, _>(0..=3));
    let abyss = rng.gen_bool(0.5);
    let horizontal = rng.gen_bool(0.5);
    let span = if horizontal { doc.height } else { doc.width };
    let at = rng.gen_range::<u32, _>(0..span);
    let thickness = rng.gen_range::<u32, _>(1..=3);
    paint_band(doc, level, abyss, horizontal, at, thickness);
}

/// Paints `thickness` full lines of one cliff level from line `at`: rows when `horizontal`,
/// columns otherwise, as abyss when `abyss`. Lines past the grid are skipped.
fn paint_band(
    doc: &mut MapDocument,
    level: CliffLevel,
    abyss: bool,
    horizontal: bool,
    at: u32,
    thickness: u32,
) {
    let span = if horizontal { doc.height } else { doc.width };
    for offset in 0..thickness {
        let line = at + offset;
        if line >= span {
            break;
        }
        let length = if horizontal { doc.width } else { doc.height };
        for along in 0..length {
            let (x, y) = if horizontal {
                (along, line)
            } else {
                (line, along)
            };
            let cell = doc
                .cell_mut(x, y)
                .expect("invariant: band cell lies inside the grid");
            cell.level = level;
            cell.ramp = None;
            if abyss {
                cell.terrain = TerrainKind::Abyss;
                cell.flags = CellFlags::empty();
            }
        }
    }
}

/// Moves one start next to another and onto whatever terrain lies there.
fn crowd_starts(doc: &mut MapDocument, rng: &mut ChaCha8Rng) {
    if doc.start_locations.len() < 2 {
        return;
    }
    let anchor = doc.start_locations[0].cell;
    let dx = rng.gen_range::<u32, _>(0..12);
    let dy = rng.gen_range::<u32, _>(0..12);
    let moved = &mut doc.start_locations[1];
    moved.cell = (anchor.0.saturating_add(dx), anchor.1.saturating_add(dy));
    if rng.gen_bool(0.5) {
        // Onto the ramp and cliff band, so the footprint is also blocked.
        moved.cell = (
            rng.gen_range::<u32, _>(26..34),
            rng.gen_range::<u32, _>(26..34),
        );
        doc.start_locations[0].cell = (
            rng.gen_range::<u32, _>(20..30),
            rng.gen_range::<u32, _>(20..30),
        );
    }
}

/// Moves one resource into a start footprint.
fn resource_into_start(doc: &mut MapDocument, rng: &mut ChaCha8Rng) {
    if doc.start_locations.is_empty() || doc.resources.is_empty() {
        return;
    }
    let start = doc.start_locations[draw_index(rng, doc.start_locations.len())].clone();
    let dx = rng.gen_range::<u32, _>(0..u32::from(start.footprint.0.max(1)));
    let dy = rng.gen_range::<u32, _>(0..u32::from(start.footprint.1.max(1)));
    if let Some(resource) = pick(&mut doc.resources, rng) {
        resource.cell = (start.cell.0 + dx, start.cell.1 + dy);
    }
}

/// Paints a pathable north-facing ramp column of a seeded length, possibly over the run limit.
fn ramp_column(doc: &mut MapDocument, rng: &mut ChaCha8Rng) {
    let x = rng.gen_range::<u32, _>(0..doc.width);
    let length = rng.gen_range::<u32, _>(2..14);
    let y0 = rng.gen_range::<u32, _>(0..doc.height - length);
    for y in y0..y0 + length {
        let cell = doc
            .cell_mut(x, y)
            .expect("invariant: column lies inside the grid");
        cell.ramp = Some(RampDir::North);
        cell.level = CliffLevel(1);
        cell.terrain = TerrainKind::MetalPlate;
        cell.flags = CellFlags::GROUND_PATHABLE;
    }
}

/// Name of an error variant, the text of its `Debug` form before any field.
fn variant_name(error: &MapValidationError) -> String {
    let text = format!("{error:?}");
    text.split([' ', '{', '('])
        .next()
        .expect("invariant: split always yields a first piece")
        .to_owned()
}

#[test]
fn validate_map_matches_the_recorded_corpus_digest() {
    let corpus = document_corpus();
    assert_eq!(corpus.len(), MUTATED_DOCUMENTS + FIXED_DOCUMENTS);

    let mut hasher = Xxh64::new(0);
    let mut seen = BTreeSet::new();
    let mut adjacent_13_14 = 0usize;
    let mut reachability_failures = 0usize;
    let mut warnings = 0usize;
    for doc in &corpus {
        let report = validate_map(doc);
        let text = format!("{report:?}");
        hasher.update(&xxh64(text.as_bytes(), 0).to_le_bytes());

        let blocked = report
            .errors
            .iter()
            .any(|e| matches!(e, MapValidationError::StartFootprintBlocked { .. }));
        let close = report
            .errors
            .iter()
            .any(|e| matches!(e, MapValidationError::StartLocationsTooClose { .. }));
        if blocked && close {
            adjacent_13_14 += 1;
        }
        if report
            .errors
            .iter()
            .any(|e| matches!(e, MapValidationError::StartLocationsUnreachable { .. }))
        {
            reachability_failures += 1;
        }
        warnings += report.warnings.len();
        seen.extend(report.errors.iter().map(variant_name));
    }
    let digest = hasher.digest();
    eprintln!(
        "validation digest 0x{digest:016x}; variants {seen:?}; 13+14 in {adjacent_13_14} \
         reports; unreachable in {reachability_failures}; {warnings} warnings"
    );

    assert!(
        adjacent_13_14 > 0,
        "corpus must hold checks 13 and 14 together"
    );
    assert!(reachability_failures > 0, "corpus must reach check 22");
    assert!(warnings > 0, "corpus must raise warnings");
    for required in [
        "CellGridMismatch",
        "AbyssCellTraversable",
        "RampOnAbyss",
        "RampAtTopLevel",
        "RampRunTooLong",
        "RampRunLevelMismatch",
        "RampNotPathable",
        "RampBuildable",
        "RampUpperLandingMismatch",
        "RampLowerLandingMismatch",
        "TooFewStartLocations",
        "PlayerSlotOutOfRange",
        "DuplicatePlayerSlot",
        "StartFootprintBlocked",
        "StartLocationsTooClose",
        "DuplicateEntityId",
        "EntityOutOfBounds",
        "EntityIdOutOfRange",
        "ResourceOnBlockedCell",
        "ResourceInsideStartFootprint",
        "EntitiesOverlap",
        "ZeroResourceAmount",
        "ZeroWatchtowerRadius",
        "WatchtowerRevealTooLarge",
        "StartLocationsUnreachable",
    ] {
        assert!(seen.contains(required), "corpus must raise {required}");
    }
    assert_eq!(digest, VALIDATION_GOLDEN, "validation output changed");
}

/// Applies one seeded file-level defect, or none.
fn damage_file(file: &mut MapFile, rng: &mut ChaCha8Rng) {
    const INVALID: [char; 6] = ['4', 'Z', 'n', '?', 'x', ' '];
    let layer = rng.gen_range::<i32, _>(0..4);
    let rows = match layer {
        0 => &mut file.levels,
        1 => &mut file.terrain,
        2 => &mut file.ramps,
        _ => &mut file.flags,
    };
    match rng.gen_range::<i32, _>(0..8) {
        0 => {
            let row = draw_index(rng, rows.len());
            let mut chars: Vec<char> = rows[row].chars().collect();
            let column = draw_index(rng, chars.len());
            chars[column] = INVALID[draw_index(rng, INVALID.len())];
            rows[row] = chars.into_iter().collect();
        }
        1 => {
            rows.pop();
        }
        2 => {
            let row = draw_index(rng, rows.len());
            rows[row].push('0');
        }
        3 => {
            let row = draw_index(rng, rows.len());
            rows[row].pop();
        }
        4 => file.width = [0, 15, 257, 65][draw_index(rng, 4)],
        5 => file.height = [0, 15, 257, 63][draw_index(rng, 4)],
        _ => {}
    }
}

/// The decoding corpus: every well-shaped document of [`document_corpus`] encoded, damaged at
/// the file level, serialised, parsed back and decoded, in corpus order.
///
/// The short-grid document is the one `MapDocument::encode` refuses, with
/// `CellCountMismatch`; any other refusal fails the calling test.
fn decoded_corpus() -> Vec<Result<MapDocument, MapFormatError>> {
    let corpus = document_corpus();
    let mut rng = ChaCha8Rng::seed_from_u64(DECODE_SEED);
    let mut results = Vec::with_capacity(corpus.len());
    for doc in &corpus {
        let mut file = match doc.encode() {
            Ok(file) => file,
            Err(MapFormatError::CellCountMismatch { .. }) => continue,
            Err(error) => panic!("a corpus document must encode: {error}"),
        };
        for _ in 0..rng.gen_range::<i32, _>(0..=2) {
            damage_file(&mut file, &mut rng);
        }
        let text = file.to_ron_string().expect("an encoded map serialises");
        let parsed = MapFile::from_ron_str(&text).expect("serialised RON parses back");
        results.push(parsed.decode());
    }
    results
}

#[test]
fn decode_matches_the_recorded_corpus_digest() {
    let mut hasher = Xxh64::new(0);
    let mut failures = BTreeSet::new();
    let results = decoded_corpus();
    assert_eq!(
        results.len(),
        document_corpus().len() - 1,
        "only the short-grid document is refused by encode"
    );
    for result in &results {
        if let Err(error) = result {
            failures.insert(
                format!("{error:?}")
                    .split([' ', '{', '('])
                    .next()
                    .expect("invariant: split always yields a first piece")
                    .to_owned(),
            );
        }
        let text = format!("{result:?}");
        hasher.update(&xxh64(text.as_bytes(), 0).to_le_bytes());
    }
    let digest = hasher.digest();
    eprintln!("decode digest 0x{digest:016x}; failures {failures:?}");
    for required in [
        "DimensionsOutOfRange",
        "RowCount",
        "RowLength",
        "InvalidCellChar",
    ] {
        assert!(failures.contains(required), "corpus must raise {required}");
    }
    assert_eq!(digest, DECODE_GOLDEN, "decode output changed");
}

/// Encodes `doc`, serialises it, parses it back and decodes it, as a save and a load do.
fn save_and_reload(doc: &MapDocument) -> Result<MapDocument, MapFormatError> {
    let text = doc.encode()?.to_ron_string()?;
    MapFile::from_ron_str(&text)?.decode()
}

/// Every document `decode` produces encodes again and survives a save and a load unchanged:
/// each shipped map under `assets/maps/`, and every successful result of the decoding corpus.
#[test]
fn encode_accepts_every_document_decode_produces() {
    let maps_dir = arena::map_path("");
    let mut shipped: Vec<_> = std::fs::read_dir(&maps_dir)
        .expect("assets/maps is readable")
        .map(|entry| entry.expect("assets/maps entry is readable").path())
        .filter(|path| path.to_string_lossy().ends_with(".scmap.ron"))
        .collect();
    shipped.sort();
    assert!(shipped.len() >= 2, "both shipped maps are found");
    for path in &shipped {
        let doc = MapFile::load(path)
            .and_then(|file| file.decode())
            .expect("a shipped map loads and decodes");
        let reloaded = save_and_reload(&doc)
            .unwrap_or_else(|error| panic!("{} must encode: {error}", path.display()));
        assert_eq!(reloaded, doc, "{} round-trips", path.display());
    }
    let mut decoded = 0_usize;
    for doc in decoded_corpus().into_iter().flatten() {
        let reloaded = save_and_reload(&doc).expect("a decoded corpus document encodes");
        assert_eq!(reloaded, doc, "a decoded corpus document round-trips");
        decoded += 1;
    }
    // 91 of the 202 damaged files decoded when this bound was set; the bound only guards against an empty loop.
    assert!(
        decoded >= 50,
        "the corpus decodes enough documents: {decoded}"
    );
}

/// Places a few seeded structures, so known-structure and occupancy paths are exercised.
fn place_seeded_structures(map: &mut GameMap, rng: &mut ChaCha8Rng) {
    for index in 0..8 {
        let x = rng.gen_range::<u32, _>(0..map.width().saturating_sub(4));
        let y = rng.gen_range::<u32, _>(0..map.height().saturating_sub(4));
        let w = rng.gen_range::<u8, _>(2..=4);
        let h = rng.gen_range::<u8, _>(2..=4);
        // Blocked placements are part of the seeded outcome and are simply skipped.
        let _ = map.place_structure(
            EntityId(9_000 + index),
            (x, y),
            sim_map::StructureFootprint::square(w, h),
        );
    }
}

/// A partially explored player view of `map`.
fn seeded_vision(map: &GameMap, rng: &mut ChaCha8Rng) -> PlayerVisionMap {
    let mut vision = PlayerVisionMap::new(map.width(), map.height());
    vision.begin_tick_update();
    for _ in 0..6 {
        let source = VisionSource {
            pos: random_point(map, rng),
            mover_type: if rng.gen_bool(0.3) {
                MoverType::Air
            } else {
                MoverType::Ground
            },
            sight_radius: SimScalar::from_num(rng.gen_range::<i32, _>(4..12)),
            detection_radius: SimScalar::ZERO,
        };
        vision.apply_vision_source(map, &source);
    }
    vision.end_tick_update();
    vision
}

/// A seeded world point inside `map`, on a cell centre or at a quarter offset.
fn random_point(map: &GameMap, rng: &mut ChaCha8Rng) -> SimVec2 {
    let quarter = SimScalar::from_num(rng.gen_range::<i32, _>(0..4)) / SimScalar::from_num(4);
    SimVec2::new(
        SimScalar::from_num(rng.gen_range::<u32, _>(0..map.width())) + quarter,
        SimScalar::from_num(rng.gen_range::<u32, _>(0..map.height())) + SimScalar::HALF,
    )
}

/// Hashes one path outcome by raw fixed-point bits, or by the error's `Debug` text.
fn hash_outcome(hasher: &mut Xxh64, outcome: &Result<PathResult, sim_map::MapError>) {
    match outcome {
        Ok(path) => {
            hasher.update(&[1]);
            hasher.update(&(path.waypoints.len() as u64).to_le_bytes());
            for waypoint in &path.waypoints {
                hasher.update(&waypoint.x.raw().to_le_bytes());
                hasher.update(&waypoint.y.raw().to_le_bytes());
            }
            hasher.update(&path.total_length.raw().to_le_bytes());
        }
        Err(error) => {
            hasher.update(&[0]);
            hasher.update(format!("{error:?}").as_bytes());
        }
    }
}

/// How a corpus routes one request: `Pathfinder::find_path_with_vision`, or a probe around it.
type RouteFn<'f> = dyn FnMut(
        &GameMap,
        &mut Pathfinder,
        &PathRequestWithVision<'_>,
    ) -> Result<PathResult, sim_map::MapError>
    + 'f;

/// Routes every request of the `PATH_GOLDEN` corpus with `route`, in corpus order; returns the
/// digest of the outcomes, the number of paths with a corner and the number of failures. A plain
/// request is routed as `Pathfinder::find_path` routes it, converted with no vision and no mask.
fn path_corpus(route: &mut RouteFn<'_>) -> (u64, usize, usize) {
    let mut rng = ChaCha8Rng::seed_from_u64(PATH_SEED);
    let mut hasher = Xxh64::new(0);
    let mut cornered = 0usize;
    let mut failed = 0usize;
    for file in [arena::DEFAULT_ARENA_FILE, arena::SHOWCASE_CANYON_FILE] {
        let mut map = arena::load_game_map(file);
        place_seeded_structures(&mut map, &mut rng);
        let vision = seeded_vision(&map, &mut rng);
        let known: Vec<bool> = map
            .cells()
            .iter()
            .map(|cell| cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE) && rng.gen_bool(0.5))
            .collect();
        let mut pathfinder = Pathfinder::new(&map);
        for _ in 0..PATH_REQUESTS_PER_MAP {
            let radius = [SMALL_RADIUS, MEDIUM_RADIUS, LARGE_RADIUS][draw_index(&mut rng, 3)];
            let base = PathRequest {
                start: random_point(&map, &mut rng),
                goal: random_point(&map, &mut rng),
                radius,
            };
            let mut request: PathRequestWithVision<'_> = base.into();
            match rng.gen_range::<i32, _>(0..4) {
                0 => {}
                3 => request.known_structures = Some(&known),
                mode => {
                    request.vision = Some(&vision);
                    request.block_unexplored_ramps = mode == 1;
                }
            }
            let outcome = route(&map, &mut pathfinder, &request);
            match &outcome {
                Ok(path) if path.waypoints.len() > 2 => cornered += 1,
                Ok(_) => {}
                Err(_) => failed += 1,
            }
            hash_outcome(&mut hasher, &outcome);
        }
    }
    (hasher.digest(), cornered, failed)
}

#[test]
fn find_path_with_vision_matches_the_recorded_corpus_digest() {
    let (digest, cornered, failed) =
        path_corpus(&mut |map, pathfinder, request| pathfinder.find_path_with_vision(map, request));
    eprintln!("path digest 0x{digest:016x}; {cornered} cornered paths; {failed} failures");
    assert!(
        cornered >= 50,
        "corpus must exercise portals and the funnel"
    );
    assert!(failed > 0, "corpus must exercise error paths");
    assert_eq!(digest, PATH_GOLDEN, "path output changed");
}

/// The §2.3 contours as `(cells, half extent, chamfer)` in quarter cells.
const CONTOURS: [(u8, u16, u16); 5] = [(5, 10, 6), (3, 6, 2), (3, 6, 3), (2, 4, 2), (2, 2, 1)];

/// Places twelve seeded contour structures where they fit; every draw is a `u32` range.
fn place_seeded_contours(map: &mut GameMap, rng: &mut ChaCha8Rng) -> u32 {
    let mut placed_contours = 0;
    for index in 0..12_u32 {
        let pick = rng.gen_range::<u32, _>(0..5);
        let (size, half, chamfer) = CONTOURS[usize::try_from(pick).unwrap_or(0)];
        let x = rng.gen_range::<u32, _>(0..map.width().saturating_sub(5));
        let y = rng.gen_range::<u32, _>(0..map.height().saturating_sub(5));
        let footprint = sim_map::StructureFootprint {
            width: size,
            height: size,
            shape: sim_types::FootprintShape::Box {
                half_width_q: half,
                half_height_q: half,
                chamfer_q: chamfer,
            },
        };
        // A blocked placement is part of the seeded outcome and leaves the map as it was.
        if map
            .place_structure(EntityId(9_100 + index), (x, y), footprint)
            .is_ok()
        {
            placed_contours += 1;
        }
    }
    placed_contours
}

/// Routes every request of the `CONTOUR_PATH_GOLDEN` corpus with `route`, in corpus order; returns
/// what [`path_corpus`] returns.
fn contour_path_corpus(route: &mut RouteFn<'_>) -> (u64, usize, usize) {
    const RADII: [i64; 6] = [5, 6, 8, 10, 12, 16];
    let mut rng = ChaCha8Rng::seed_from_u64(PATH_SEED ^ 0xc0_47_0e);
    let mut hasher = Xxh64::new(0);
    let mut cornered = 0usize;
    let mut failed = 0usize;
    for file in [arena::DEFAULT_ARENA_FILE, arena::SHOWCASE_CANYON_FILE] {
        let mut map = arena::load_game_map(file);
        assert!(place_seeded_contours(&mut map, &mut rng) > 0);
        let known: Vec<bool> = map
            .cells()
            .iter()
            .map(|cell| cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE) && rng.gen_bool(0.5))
            .collect();
        let mut pathfinder = Pathfinder::new(&map);
        for _ in 0..PATH_REQUESTS_PER_MAP {
            let pick = rng.gen_range::<u32, _>(0..6);
            let radius = SimScalar::from_raw(RADII[usize::try_from(pick).unwrap_or(0)] << 28);
            let base = PathRequest {
                start: random_point(&map, &mut rng),
                goal: random_point(&map, &mut rng),
                radius,
            };
            let mut request: PathRequestWithVision<'_> = base.into();
            if rng.gen_range::<u32, _>(0..2) != 0 {
                request.known_structures = Some(&known);
            }
            let outcome = route(&map, &mut pathfinder, &request);
            match &outcome {
                Ok(path) if path.waypoints.len() > 2 => cornered += 1,
                Ok(_) => {}
                Err(_) => failed += 1,
            }
            hash_outcome(&mut hasher, &outcome);
        }
    }
    (hasher.digest(), cornered, failed)
}

/// Plan `2026-11-18-contour-footprints` WP3: routes on both shipped maps among seeded contour
/// structures, at the shipped terrain radii, with and without knowledge rules.
#[test]
fn find_path_with_vision_on_contours_matches_the_recorded_corpus_digest() {
    let (digest, cornered, failed) = contour_path_corpus(&mut |map, pathfinder, request| {
        pathfinder.find_path_with_vision(map, request)
    });
    eprintln!("contour path digest 0x{digest:016x}; {cornered} cornered paths; {failed} failures");
    assert!(
        cornered >= 50,
        "corpus must exercise portals and the funnel"
    );
    assert!(failed > 0, "corpus must exercise error paths");
    assert_eq!(digest, CONTOUR_PATH_GOLDEN, "contour path output changed");
}

/// Plan `2026-11-19-partial-knowledge-funnel` (B-146): in both path corpora a route falls back
/// to the portal midpoints only from a start where the disc does not fit (a wedged start,
/// pathfinding plan D-3). Before the plan, 8 fallbacks of `PATH_GOLDEN`'s corpus and 13 of
/// `CONTOUR_PATH_GOLDEN`'s came from fitting starts, every one rounding a free vertex.
/// How many routes of `corpus` fall back to the portal midpoints from a start where the disc
/// fits, read from `Pathfinder::mesh_funnel_fallbacks` around each request.
fn fitting_start_fallbacks(corpus: fn(&mut RouteFn<'_>) -> (u64, usize, usize)) -> usize {
    let mut count = 0usize;
    corpus(&mut |map, pathfinder, request| {
        let fits = pathfinder.segment_is_passable(map, request, request.start, request.start);
        let before = pathfinder.mesh_funnel_fallbacks();
        let outcome = pathfinder.find_path_with_vision(map, request);
        if fits && pathfinder.mesh_funnel_fallbacks() > before {
            count += 1;
        }
        outcome
    });
    count
}

#[test]
fn golden_corpora_fall_back_only_from_wedged_starts() {
    assert_eq!(
        (
            fitting_start_fallbacks(path_corpus),
            fitting_start_fallbacks(contour_path_corpus)
        ),
        (0, 0),
        "fallbacks from fitting starts in the PATH_GOLDEN and CONTOUR_PATH_GOLDEN corpora"
    );
}
