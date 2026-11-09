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
//! - Validation: both shipped maps unchanged, the default arena with one cell missing, then 200
//!   documents derived from `assets/maps/default_arena.scmap.ron` by a `ChaCha8Rng`-seeded
//!   mutator that flips terrain, ramps, cliff levels and flags, moves start slots, footprints,
//!   resources and watchtowers, zeroes amounts and radii, and duplicates entity IDs. Every
//!   report's `Debug` text is hashed in corpus order.
//! - Decoding: the 202 well-shaped documents of that corpus encoded to `MapFile`, damaged at the file level (bad
//!   dimensions, missing or overlong rows, invalid characters), serialised to RON and parsed back,
//!   then decoded. Every result's `Debug` text is hashed.
//! - Pathfinding: 250 seeded requests on each shipped map, with structures placed, mixing plain
//!   requests, partial player vision with and without unexplored-ramp blocking, and a known
//!   structure mask. Waypoints and lengths are hashed by their raw fixed-point bits.
//!
//! # Invariants
//! - The corpus is a pure function of the seeds below and the shipped map files.
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

use std::collections::BTreeSet;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_map::{
    validate_map, CellFlags, CliffLevel, GameMap, MapDocument, MapEntityId, MapFile,
    MapFormatError, MapValidationError, PathRequest, PathRequestWithVision, PathResult, Pathfinder,
    PlayerVisionMap, RampDir, TerrainKind, UnitClearance, VisionSource,
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
/// Number of mutated documents derived from the default arena.
const MUTATED_DOCUMENTS: usize = 200;
/// Path requests issued on each shipped map.
const PATH_REQUESTS_PER_MAP: usize = 250;

/// Digest of every validation report over the corpus, recorded before the step 9.2 split.
const VALIDATION_GOLDEN: u64 = 0xf028_1611_929c_52d1;
/// Digest of every decode result over the corpus, recorded before the step 9.2 split.
const DECODE_GOLDEN: u64 = 0x56c4_3eea_8701_94bd;
/// Digest of every path result over the corpus, recorded before the step 9.2 split.
const PATH_GOLDEN: u64 = 0x52cf_7f61_9e6e_88b1;

/// Builds the validation corpus: both shipped maps, then the seeded mutations.
fn document_corpus() -> Vec<MapDocument> {
    let base = arena::load_document(arena::DEFAULT_ARENA_FILE);
    let mut short_grid = base.clone();
    short_grid.cells.pop();
    let mut corpus = vec![
        base.clone(),
        arena::load_document(arena::SHOWCASE_CANYON_FILE),
        short_grid,
    ];
    let mut rng = ChaCha8Rng::seed_from_u64(DOCUMENT_SEED);
    for _ in 0..MUTATED_DOCUMENTS {
        let mut doc = base.clone();
        let mutations = rng.gen_range(1..=4);
        for _ in 0..mutations {
            mutate_document(&mut doc, &mut rng);
        }
        corpus.push(doc);
    }
    corpus
}

/// Applies one seeded mutation to `doc`.
fn mutate_document(doc: &mut MapDocument, rng: &mut ChaCha8Rng) {
    let width = doc.width;
    let height = doc.height;
    match rng.gen_range(0..20) {
        0 => {
            let terrain = [
                TerrainKind::Grass,
                TerrainKind::MetalPlate,
                TerrainKind::Dirt,
                TerrainKind::Abyss,
            ][rng.gen_range(0..4)];
            random_cell(doc, rng).terrain = terrain;
        }
        1 => {
            let ramp = [
                None,
                Some(RampDir::North),
                Some(RampDir::South),
                Some(RampDir::East),
                Some(RampDir::West),
            ][rng.gen_range(0..5)];
            random_cell(doc, rng).ramp = ramp;
        }
        2 => random_cell(doc, rng).level = CliffLevel(rng.gen_range(0..=3)),
        3 => {
            let flag = [
                CellFlags::BUILDABLE,
                CellFlags::GROUND_PATHABLE,
                CellFlags::LOS_BLOCKER,
                CellFlags::SPEED_ZONE,
            ][rng.gen_range(0..4)];
            random_cell(doc, rng).flags.toggle(flag);
        }
        4 => {
            if let Some(start) = pick(&mut doc.start_locations, rng) {
                start.cell = (rng.gen_range(0..width + 6), rng.gen_range(0..height + 6));
            }
        }
        5 => {
            if let Some(start) = pick(&mut doc.start_locations, rng) {
                start.footprint = (rng.gen_range(0..9), rng.gen_range(0..9));
            }
        }
        6 => {
            if let Some(start) = pick(&mut doc.start_locations, rng) {
                // Slots 0 and 1 are the shipped ones, so these duplicate a slot or leave the range.
                start.player_slot = [0, 1, 7, 8, 200][rng.gen_range(0..5)];
            }
        }
        7 => {
            let occupied = doc.watchtowers.first().map(|tower| tower.cell);
            if let Some(resource) = pick(&mut doc.resources, rng) {
                resource.cell = match occupied {
                    Some(cell) if rng.gen_bool(0.3) => cell,
                    _ => (rng.gen_range(0..width + 4), rng.gen_range(0..height + 4)),
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
                tower.capture_radius_tenths = [0, 1, 30, 400][rng.gen_range(0..4)];
                tower.reveal_radius_tenths = [0, 120, 4000, u16::MAX][rng.gen_range(0..4)];
            }
        }
        11 => {
            if let Some(tower) = pick(&mut doc.watchtowers, rng) {
                tower.cell = (rng.gen_range(0..width + 4), rng.gen_range(0..height + 4));
            }
        }
        12 => {
            if !doc.start_locations.is_empty() {
                let index = rng.gen_range(0..doc.start_locations.len());
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
                .cell_mut(rng.gen_range(30..34), rng.gen_range(31..34))
                .expect("invariant: the shipped ramp lies inside the 64x64 grid");
            cell.level = CliffLevel(rng.gen_range(0..=3));
        }
        18 => {
            let cell = random_cell(doc, rng);
            cell.level = CliffLevel::MAX;
            cell.ramp = Some(RampDir::East);
        }
        _ => {
            if let Some(tower) = pick(&mut doc.watchtowers, rng) {
                tower.id = MapEntityId(u32::MAX - rng.gen_range(0..4));
            }
        }
    }
}

/// A uniformly chosen cell of `doc`.
fn random_cell<'d>(doc: &'d mut MapDocument, rng: &mut ChaCha8Rng) -> &'d mut sim_map::MapCellSpec {
    let index = rng.gen_range(0..doc.cells.len());
    &mut doc.cells[index]
}

/// A uniformly chosen element of `items`, or `None` when it is empty.
fn pick<'v, T>(items: &'v mut [T], rng: &mut ChaCha8Rng) -> Option<&'v mut T> {
    if items.is_empty() {
        None
    } else {
        let index = rng.gen_range(0..items.len());
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
    let id = ids[rng.gen_range(0..ids.len())];
    match rng.gen_range(0..3) {
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
    let level = CliffLevel(rng.gen_range(0..=3));
    let abyss = rng.gen_bool(0.5);
    let horizontal = rng.gen_bool(0.5);
    let span = if horizontal { doc.height } else { doc.width };
    let at = rng.gen_range(0..span);
    let thickness = rng.gen_range(1..=3);
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
    let dx = rng.gen_range(0..12);
    let dy = rng.gen_range(0..12);
    let moved = &mut doc.start_locations[1];
    moved.cell = (anchor.0.saturating_add(dx), anchor.1.saturating_add(dy));
    if rng.gen_bool(0.5) {
        // Onto the ramp and cliff band, so the footprint is also blocked.
        moved.cell = (rng.gen_range(26..34), rng.gen_range(26..34));
        doc.start_locations[0].cell = (rng.gen_range(20..30), rng.gen_range(20..30));
    }
}

/// Moves one resource into a start footprint.
fn resource_into_start(doc: &mut MapDocument, rng: &mut ChaCha8Rng) {
    if doc.start_locations.is_empty() || doc.resources.is_empty() {
        return;
    }
    let start = doc.start_locations[rng.gen_range(0..doc.start_locations.len())].clone();
    let dx = rng.gen_range(0..u32::from(start.footprint.0.max(1)));
    let dy = rng.gen_range(0..u32::from(start.footprint.1.max(1)));
    if let Some(resource) = pick(&mut doc.resources, rng) {
        resource.cell = (start.cell.0 + dx, start.cell.1 + dy);
    }
}

/// Paints a pathable north-facing ramp column of a seeded length, possibly over the run limit.
fn ramp_column(doc: &mut MapDocument, rng: &mut ChaCha8Rng) {
    let x = rng.gen_range(0..doc.width);
    let length = rng.gen_range(2..14);
    let y0 = rng.gen_range(0..doc.height - length);
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
    assert_eq!(corpus.len(), MUTATED_DOCUMENTS + 3);

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
    let layer = rng.gen_range(0..4);
    let rows = match layer {
        0 => &mut file.levels,
        1 => &mut file.terrain,
        2 => &mut file.ramps,
        _ => &mut file.flags,
    };
    match rng.gen_range(0..8) {
        0 => {
            let row = rng.gen_range(0..rows.len());
            let mut chars: Vec<char> = rows[row].chars().collect();
            let column = rng.gen_range(0..chars.len());
            chars[column] = INVALID[rng.gen_range(0..INVALID.len())];
            rows[row] = chars.into_iter().collect();
        }
        1 => {
            rows.pop();
        }
        2 => {
            let row = rng.gen_range(0..rows.len());
            rows[row].push('0');
        }
        3 => {
            let row = rng.gen_range(0..rows.len());
            rows[row].pop();
        }
        4 => file.width = [0, 15, 257, 65][rng.gen_range(0..4)],
        5 => file.height = [0, 15, 257, 63][rng.gen_range(0..4)],
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
        for _ in 0..rng.gen_range(0..=2) {
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
    // 91 of the 202 damaged files decode today; the bound only guards against an empty loop.
    assert!(
        decoded >= 50,
        "the corpus decodes enough documents: {decoded}"
    );
}

/// Places a few seeded structures, so known-structure and occupancy paths are exercised.
fn place_seeded_structures(map: &mut GameMap, rng: &mut ChaCha8Rng) {
    for index in 0..8 {
        let x = rng.gen_range(0..map.width().saturating_sub(4));
        let y = rng.gen_range(0..map.height().saturating_sub(4));
        let w = rng.gen_range(2..=4);
        let h = rng.gen_range(2..=4);
        // Blocked placements are part of the seeded outcome and are simply skipped.
        let _ = map.place_structure(EntityId(9_000 + index), x, y, w, h);
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
            sight_radius: SimScalar::from_num(rng.gen_range(4..12)),
            is_detector: false,
        };
        vision.apply_vision_source(map, &source);
    }
    vision.end_tick_update();
    vision
}

/// A seeded world point inside `map`, on a cell centre or at a quarter offset.
fn random_point(map: &GameMap, rng: &mut ChaCha8Rng) -> SimVec2 {
    let quarter = SimScalar::from_num(rng.gen_range(0..4)) / SimScalar::from_num(4);
    SimVec2::new(
        SimScalar::from_num(rng.gen_range(0..map.width())) + quarter,
        SimScalar::from_num(rng.gen_range(0..map.height())) + SimScalar::HALF,
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

#[test]
fn find_path_with_vision_matches_the_recorded_corpus_digest() {
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
            let clearance = [
                UnitClearance::Small,
                UnitClearance::Medium,
                UnitClearance::Large,
            ][rng.gen_range(0..3)];
            let base = PathRequest {
                start: random_point(&map, &mut rng),
                goal: random_point(&map, &mut rng),
                clearance,
                radius: clearance.radius_scalar(),
            };
            let outcome = match rng.gen_range(0..4) {
                0 => pathfinder.find_path(&map, &base),
                mode => {
                    let mut request: PathRequestWithVision<'_> = base.into();
                    if mode == 3 {
                        request.known_structures = Some(&known);
                    } else {
                        request.vision = Some(&vision);
                        request.block_unexplored_ramps = mode == 1;
                    }
                    pathfinder.find_path_with_vision(&map, &request)
                }
            };
            match &outcome {
                Ok(path) if path.waypoints.len() > 2 => cornered += 1,
                Ok(_) => {}
                Err(_) => failed += 1,
            }
            hash_outcome(&mut hasher, &outcome);
        }
    }
    let digest = hasher.digest();
    eprintln!("path digest 0x{digest:016x}; {cornered} cornered paths; {failed} failures");
    assert!(
        cornered >= 50,
        "corpus must exercise portals and the funnel"
    );
    assert!(failed > 0, "corpus must exercise error paths");
    assert_eq!(digest, PATH_GOLDEN, "path output changed");
}
