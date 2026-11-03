//! Shipped-map loaders shared by the `sim_map` integration tests.
//!
//! # Responsibility
//! Resolves the runtime map files under `assets/maps/` and loads them through the real
//! production path, `MapFile::load` then `MapFile::decode` then `MapDocument::to_game_map`.
//! It replaces the deleted `GameMap::default_arena()` code fixture (Phase 9 decision D-11),
//! so every test exercises the same bytes the game ships.
//!
//! # How it fits
//! Each test binary includes this file with `#[path = "support/arena.rs"] mod arena;` and uses
//! the subset it needs.
//!
//! # Invariants
//! - A missing or invalid shipped map fails the calling test loudly; nothing falls back to a
//!   generated map.
//! - Paths resolve from `CARGO_MANIFEST_DIR`, so they do not depend on the working directory.

#![allow(
    clippy::panic,
    reason = "test support: a broken shipped map must fail the calling test loudly"
)]

use std::path::PathBuf;

use sim_map::{GameMap, MapDocument, MapFile};

/// File name of the canonical 64x64 two-level arena under `assets/maps/`.
pub const DEFAULT_ARENA_FILE: &str = "default_arena.scmap.ron";

/// File name of the showcase canyon map under `assets/maps/`.
pub const SHOWCASE_CANYON_FILE: &str = "showcase_canyon.scmap.ron";

/// Absolute path of a shipped map file, given its file name under `assets/maps/`.
#[must_use]
pub fn map_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps")
        .join(filename)
}

/// Loads and decodes a shipped map file into its editable document.
///
/// # Panics
/// Panics with the file name if the file is missing or does not decode.
#[must_use]
pub fn load_document(filename: &str) -> MapDocument {
    MapFile::load(&map_path(filename))
        .unwrap_or_else(|err| panic!("shipped map {filename} must load: {err}"))
        .decode()
        .unwrap_or_else(|err| panic!("shipped map {filename} must decode: {err}"))
}

/// Loads a shipped map file as the simulation's runtime [`GameMap`].
///
/// # Panics
/// Panics with the file name if the file is missing, does not decode, or does not convert.
#[must_use]
pub fn load_game_map(filename: &str) -> GameMap {
    load_document(filename)
        .to_game_map()
        .unwrap_or_else(|err| panic!("shipped map {filename} must convert to a GameMap: {err}"))
}

/// The canonical 64x64 two-level arena, loaded from `assets/maps/default_arena.scmap.ron`.
///
/// Low ground (level 1) fills `y < 30`, a four-cell north-facing ramp spans `x in 30..34`,
/// `y in 30..34`, the high-ground plateau (level 2) fills `y >= 34`, a 3x3 line-of-sight
/// blocker sits at `x, y in 15..18`, and one neutral watchtower stands at `(28.5, 28.5)` with
/// entity ID `WATCHTOWER_ENTITY_ID_BASE + 21`.
#[must_use]
pub fn default_arena() -> GameMap {
    load_game_map(DEFAULT_ARENA_FILE)
}
