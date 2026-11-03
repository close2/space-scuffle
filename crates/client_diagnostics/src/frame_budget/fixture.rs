//! The measurement's assets root: a copy of the real one with the arena, loadouts and scenarios.
//!
//! # Responsibility
//! Builds `<work-dir>/root` ([`build_fixture_root`]): the real `textures/` mirrored by hard link
//! (or copy across file systems), the real `catalog/`, `maps/`, `scenarios/` and `shaders/`
//! copied, and then, written into the copy only:
//! - the flat arena map named by the spec, generated through the `sim_map` document API and
//!   checked by [`sim_map::validate_map`] with no error and no warning;
//! - one start loadout per scenario, appended to the copied `catalog/start_loadouts.ron`, whose
//!   units are the spec's source loadout scaled to the scenario's count ([`scale_loadout`]);
//! - one scenario file per spec scenario, `scenarios/frame_budget_<name>.ron`
//!   ([`scenario_path`]), in which every player attack-moves to its target's start.
//!
//! # How it fits
//! [`super::run`] builds the root, points `SPACE_SCUFFLE_ASSETS` at it and starts the real client
//! on one of its scenarios, so units are placed by the production scenario path. No unit is
//! named in Rust (check C5): the unit mix comes from the spec's source loadout, which is data.
//!
//! # Invariants
//! - Nothing is written below the real assets root: a work directory inside it is refused before
//!   any file is created (finding B-8's rule).
//! - An existing root is replaced only if it carries the marker file this module writes, so the
//!   tool never deletes a directory it did not make.
//! - Scaling gives each player exactly the requested count, keeps every entry within one unit of
//!   its exact proportional share, and splits counts above `u8::MAX` into several entries.

use super::FrameBudgetSpec;
use client::runtime_battle::{BattleInitialOrder, BattleScenarioFile};
use sim_core::scenario::{LoadoutUnit, PlayerSetup, StartLoadout, StartLoadoutCatalog};
use sim_map::{MapDocument, MapEntityId, MapMetadata, MapStartLocation};
use std::path::{Path, PathBuf};

/// Name of the root directory inside the work directory.
pub const ROOT_DIRECTORY: &str = "root";

/// File that marks a root as built by this module.
pub const ROOT_MARKER: &str = ".frame_budget_fixture";

/// Directories of the real assets root that are copied, so their text can be extended.
const COPIED_DIRECTORIES: [&str; 4] = ["catalog", "maps", "scenarios", "shaders"];

/// Directory of the real assets root that is mirrored by hard link.
const LINKED_DIRECTORY: &str = "textures";

/// Start loadout catalog, relative to an assets root.
const LOADOUTS_FILE: &str = "catalog/start_loadouts.ron";

/// Why the measurement's assets root could not be built.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// The work directory lies inside the real assets root.
    #[error(
        "work directory {work_dir} is inside the real assets root {assets}; choose one outside it"
    )]
    WorkDirInsideAssets {
        /// Requested work directory.
        work_dir: PathBuf,
        /// Real assets root.
        assets: PathBuf,
    },
    /// `<work-dir>/root` exists but was not made by this tool.
    #[error("{root} exists and has no {ROOT_MARKER} marker; it was not made by frame_budget, so it is left alone")]
    ForeignRoot {
        /// The existing directory.
        root: PathBuf,
    },
    /// A file system operation failed.
    #[error("frame-budget fixture I/O at {path}: {source}")]
    Io {
        /// File or directory involved.
        path: PathBuf,
        /// The failure.
        #[source]
        source: std::io::Error,
    },
    /// The spec's source loadout is not in the start loadout catalog.
    #[error("source loadout {0} is not in {LOADOUTS_FILE}")]
    UnknownSourceLoadout(String),
    /// The source loadout has no unit to scale.
    #[error("source loadout {0} has no unit to scale")]
    EmptySourceLoadout(String),
    /// A generated loadout's slug is already taken.
    #[error("start loadout {0} already exists in {LOADOUTS_FILE}")]
    DuplicateLoadout(String),
    /// The start loadout catalog could not be read.
    #[error(transparent)]
    Loadouts(#[from] sim_core::scenario::ScenarioError),
    /// The arena map could not be built or written.
    #[error(transparent)]
    Map(#[from] sim_map::MapFormatError),
    /// The generated arena fails map validation.
    #[error("the generated arena fails map validation: {0:?}")]
    InvalidMap(Vec<sim_map::MapValidationError>),
    /// The generated arena raises validation warnings.
    #[error("the generated arena raises map validation warnings: {0:?}")]
    MapWarnings(Vec<sim_map::MapValidationWarning>),
    /// A generated file could not be serialized.
    #[error("a generated fixture file could not be serialized: {0}")]
    Serialize(#[from] ron::Error),
}

/// Scenario file of scenario `name`, relative to the assets root.
#[must_use]
pub fn scenario_path(name: &str) -> PathBuf {
    PathBuf::from(format!("scenarios/frame_budget_{name}.ron"))
}

/// Slug of the start loadout of scenario `name`.
#[must_use]
pub fn loadout_slug(name: &str) -> String {
    format!("frame_budget_{name}")
}

/// Scales `source` to exactly `total` units with largest-remainder rounding.
///
/// Entry `i` gets `floor(total * c_i / S)` units, where `c_i` is its count and `S` the sum of
/// counts; the units still missing go one each to the entries with the largest remainders, the
/// earlier entry first on a tie. An entry above `u8::MAX` is split into several entries of the
/// same slug, and an entry of zero is dropped. Order follows `source`.
///
/// # Errors
/// [`FixtureError::EmptySourceLoadout`] (with an empty name) if `source` holds no unit.
pub fn scale_loadout(source: &[LoadoutUnit], total: u32) -> Result<Vec<LoadoutUnit>, FixtureError> {
    let sum: u64 = source.iter().map(|unit| u64::from(unit.count)).sum();
    if sum == 0 {
        return Err(FixtureError::EmptySourceLoadout(String::new()));
    }
    let total = u64::from(total);
    let mut shares: Vec<(u64, u64)> = source
        .iter()
        .map(|unit| {
            let exact = total * u64::from(unit.count);
            (exact / sum, exact % sum)
        })
        .collect();
    let assigned: u64 = shares.iter().map(|(floor, _)| floor).sum();
    let mut order: Vec<usize> = (0..shares.len()).collect();
    order.sort_by(|&a, &b| shares[b].1.cmp(&shares[a].1).then(a.cmp(&b)));
    // The remainders sum to a multiple of S below S * len, so at most len - 1 units are missing.
    for &index in order
        .iter()
        .take(usize::try_from(total - assigned).unwrap_or(usize::MAX))
    {
        shares[index].0 += 1;
    }
    let mut scaled = Vec::new();
    for (unit, (mut count, _)) in source.iter().zip(shares) {
        while count > 0 {
            let part = count.min(u64::from(u8::MAX));
            scaled.push(LoadoutUnit {
                slug: unit.slug.clone(),
                count: u8::try_from(part).unwrap_or(u8::MAX),
            });
            count -= part;
        }
    }
    Ok(scaled)
}

/// Builds or rebuilds `<work_dir>/root` from `assets` and `spec`, and returns its canonical path.
///
/// # Errors
/// [`FixtureError::WorkDirInsideAssets`] before anything is written, [`FixtureError::ForeignRoot`]
/// for an existing root this tool did not make, and the typed failures of each generated file.
pub fn build_fixture_root(
    spec: &FrameBudgetSpec,
    assets: &Path,
    work_dir: &Path,
) -> Result<PathBuf, FixtureError> {
    let assets = canonical(assets)?;
    let resolved_work_dir = resolve_new_path(work_dir)?;
    if resolved_work_dir.starts_with(&assets) {
        return Err(FixtureError::WorkDirInsideAssets {
            work_dir: work_dir.to_owned(),
            assets,
        });
    }
    let root = resolved_work_dir.join(ROOT_DIRECTORY);
    if root.exists() {
        if !root.join(ROOT_MARKER).is_file() {
            return Err(FixtureError::ForeignRoot { root });
        }
        std::fs::remove_dir_all(&root).map_err(io_at(&root))?;
    }
    std::fs::create_dir_all(&root).map_err(io_at(&root))?;
    let marker = root.join(ROOT_MARKER);
    std::fs::write(&marker, "built by the frame_budget tool; safe to delete\n")
        .map_err(io_at(&marker))?;
    link_tree(&assets.join(LINKED_DIRECTORY), &root.join(LINKED_DIRECTORY))?;
    for name in COPIED_DIRECTORIES {
        copy_tree(&assets.join(name), &root.join(name))?;
    }
    write_arena(spec, &root)?;
    write_loadouts(spec, &root)?;
    write_scenarios(spec, &root)?;
    Ok(root)
}

/// Generates the arena map, validates it and writes it to `spec.map.file` under `root`.
fn write_arena(spec: &FrameBudgetSpec, root: &Path) -> Result<(), FixtureError> {
    let mut document = MapDocument::new_blank(
        spec.map.width,
        spec.map.height,
        MapMetadata {
            title: spec.map.title.clone(),
            author: "frame_budget tool".into(),
            description: spec.map.description.clone(),
            theme: spec.map.theme.clone(),
        },
    )?;
    for (index, player) in (1_u32..).zip(&spec.players) {
        document.start_locations.push(MapStartLocation {
            id: MapEntityId(index),
            player_slot: player.slot,
            cell: player.start_cell,
            footprint: spec.map.start_footprint,
        });
    }
    let report = sim_map::validate_map(&document);
    if !report.errors.is_empty() {
        return Err(FixtureError::InvalidMap(report.errors));
    }
    if !report.warnings.is_empty() {
        return Err(FixtureError::MapWarnings(report.warnings));
    }
    let path = root.join(&spec.map.file);
    document.encode()?.save_atomic(&path)?;
    Ok(())
}

/// Appends one scaled loadout per scenario to the copied start loadout catalog.
fn write_loadouts(spec: &FrameBudgetSpec, root: &Path) -> Result<(), FixtureError> {
    let path = root.join(LOADOUTS_FILE);
    let mut catalog = StartLoadoutCatalog::load(&path)?;
    let source = catalog
        .get(&spec.source_loadout)
        .ok_or_else(|| FixtureError::UnknownSourceLoadout(spec.source_loadout.clone()))?
        .clone();
    for scenario in &spec.scenarios {
        let slug = loadout_slug(&scenario.name);
        if catalog.get(&slug).is_some() {
            return Err(FixtureError::DuplicateLoadout(slug));
        }
        let units =
            scale_loadout(&source.units, scenario.units_per_player).map_err(
                |error| match error {
                    FixtureError::EmptySourceLoadout(_) => {
                        FixtureError::EmptySourceLoadout(source.slug.clone())
                    }
                    other => other,
                },
            )?;
        catalog.loadouts.push(StartLoadout {
            slug,
            display_name: format!(
                "Frame budget: {} per player from {}",
                scenario.units_per_player, source.slug
            ),
            editor_default: false,
            town_hall: source.town_hall.clone(),
            units,
            starting_resources: source.starting_resources,
        });
    }
    write_ron(&path, &catalog)
}

/// Writes one scenario file per spec scenario.
fn write_scenarios(spec: &FrameBudgetSpec, root: &Path) -> Result<(), FixtureError> {
    for scenario in &spec.scenarios {
        let slug = loadout_slug(&scenario.name);
        let players = spec
            .players
            .iter()
            .map(|player| PlayerSetup {
                player_slot: player.slot,
                loadout_slug: slug.clone(),
            })
            .collect();
        // A player with no unit has no combatant to order, which the loader rejects.
        let initial_orders = if scenario.units_per_player == 0 {
            Vec::new()
        } else {
            spec.players
                .iter()
                .map(|player| BattleInitialOrder::AttackMoveToStart {
                    player_slot: player.slot,
                    target_player_slot: player.target_slot,
                })
                .collect()
        };
        let file = BattleScenarioFile {
            format_version: 1,
            map: spec.map.file.clone(),
            seed: spec.seed,
            players,
            initial_orders,
        };
        write_ron(&root.join(scenario_path(&scenario.name)), &file)?;
    }
    Ok(())
}

/// Serializes `value` as pretty RON into `path`.
fn write_ron<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), FixtureError> {
    let text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::new())?;
    std::fs::write(path, text).map_err(io_at(path))
}

/// Maps an I/O failure to [`FixtureError::Io`] at `path`.
fn io_at(path: &Path) -> impl FnOnce(std::io::Error) -> FixtureError + '_ {
    move |source| FixtureError::Io {
        path: path.to_owned(),
        source,
    }
}

/// The canonical form of an existing `path`.
fn canonical(path: &Path) -> Result<PathBuf, FixtureError> {
    std::fs::canonicalize(path).map_err(io_at(path))
}

/// The absolute form of `path`, which need not exist: its deepest existing ancestor made
/// canonical, followed by the components that do not exist yet.
fn resolve_new_path(path: &Path) -> Result<PathBuf, FixtureError> {
    let absolute = std::path::absolute(path).map_err(io_at(path))?;
    let mut missing = Vec::new();
    let mut existing = absolute.as_path();
    while !existing.exists() {
        let (Some(name), Some(parent)) = (existing.file_name(), existing.parent()) else {
            break;
        };
        missing.push(name.to_owned());
        existing = parent;
    }
    let mut resolved = canonical(existing)?;
    resolved.extend(missing.iter().rev());
    Ok(resolved)
}

/// Copies every file beneath `from` to `to`.
fn copy_tree(from: &Path, to: &Path) -> Result<(), FixtureError> {
    mirror_tree(from, to, false)
}

/// Mirrors every file beneath `from` into `to` by hard link, or by copy where linking fails
/// (another file system).
fn link_tree(from: &Path, to: &Path) -> Result<(), FixtureError> {
    mirror_tree(from, to, true)
}

/// Recreates the tree `from` at `to`, linking files if `link`, otherwise copying them.
fn mirror_tree(from: &Path, to: &Path, link: bool) -> Result<(), FixtureError> {
    std::fs::create_dir_all(to).map_err(io_at(to))?;
    for entry in std::fs::read_dir(from).map_err(io_at(from))? {
        let entry = entry.map_err(io_at(from))?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(io_at(&source))?.is_dir() {
            mirror_tree(&source, &target, link)?;
        } else if !(link && std::fs::hard_link(&source, &target).is_ok()) {
            std::fs::copy(&source, &target).map_err(io_at(&source))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(slug: &str, count: u8) -> LoadoutUnit {
        LoadoutUnit {
            slug: slug.into(),
            count,
        }
    }

    /// A 2 : 8 : 2 mix, the shape of the shipped source loadout, under neutral names.
    fn mix() -> Vec<LoadoutUnit> {
        vec![unit("first", 2), unit("second", 8), unit("third", 2)]
    }

    fn per_slug(units: &[LoadoutUnit]) -> Vec<(String, u32)> {
        let mut totals: Vec<(String, u32)> = Vec::new();
        for entry in units {
            match totals.iter_mut().find(|(slug, _)| *slug == entry.slug) {
                Some((_, count)) => *count += u32::from(entry.count),
                None => totals.push((entry.slug.clone(), u32::from(entry.count))),
            }
        }
        totals
    }

    #[test]
    fn scaling_sums_to_exactly_the_requested_count() -> Result<(), FixtureError> {
        for total in [0, 1, 25, 50, 100, 200, 255, 256, 1000] {
            let scaled = scale_loadout(&mix(), total)?;
            let sum: u32 = scaled.iter().map(|entry| u32::from(entry.count)).sum();
            assert_eq!(sum, total, "total {total}: {scaled:?}");
            assert!(scaled.iter().all(|entry| entry.count > 0));
        }
        Ok(())
    }

    #[test]
    fn scaling_keeps_each_share_within_one_unit_of_the_source_ratio() -> Result<(), FixtureError> {
        let source = mix();
        let sum: f64 = source.iter().map(|entry| f64::from(entry.count)).sum();
        for total in [0, 1, 25, 50, 100, 200, 255, 256, 1000] {
            for (slug, count) in per_slug(&scale_loadout(&source, total)?) {
                let weight = source
                    .iter()
                    .find(|entry| entry.slug == slug)
                    .map_or(0.0, |entry| f64::from(entry.count));
                let exact = f64::from(total) * weight / sum;
                assert!(
                    (f64::from(count) - exact).abs() < 1.0,
                    "total {total}, {slug}: {count} against {exact}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn largest_remainders_win_and_ties_go_to_the_earlier_entry() -> Result<(), FixtureError> {
        // 25 * (2, 8, 2) / 12 = (4.17, 16.67, 4.17): the middle entry has the largest remainder.
        assert_eq!(
            per_slug(&scale_loadout(&mix(), 25)?),
            vec![
                ("first".into(), 4),
                ("second".into(), 17),
                ("third".into(), 4)
            ]
        );
        // 50 * (2, 8, 2) / 12 = (8.33, 33.33, 8.33): three equal remainders, one unit missing.
        assert_eq!(
            per_slug(&scale_loadout(&mix(), 50)?),
            vec![
                ("first".into(), 9),
                ("second".into(), 33),
                ("third".into(), 8)
            ]
        );
        Ok(())
    }

    #[test]
    fn counts_above_u8_are_split_into_several_entries() -> Result<(), FixtureError> {
        let scaled = scale_loadout(&[unit("only", 1)], 256)?;
        assert_eq!(scaled, vec![unit("only", 255), unit("only", 1)]);
        let scaled = scale_loadout(&[unit("only", 3)], 255)?;
        assert_eq!(scaled, vec![unit("only", 255)]);
        let scaled = scale_loadout(&mix(), 1000)?;
        assert_eq!(
            per_slug(&scaled),
            vec![
                ("first".into(), 167),
                ("second".into(), 667),
                ("third".into(), 166)
            ]
        );
        assert_eq!(
            scaled.iter().filter(|entry| entry.slug == "second").count(),
            3
        );
        Ok(())
    }

    #[test]
    fn an_empty_source_cannot_be_scaled() {
        assert!(matches!(
            scale_loadout(&[], 10),
            Err(FixtureError::EmptySourceLoadout(_))
        ));
        assert!(matches!(
            scale_loadout(&[unit("none", 0)], 10),
            Err(FixtureError::EmptySourceLoadout(_))
        ));
    }

    #[test]
    fn a_new_path_resolves_through_its_existing_ancestor() -> Result<(), FixtureError> {
        let directory = tempfile::tempdir().map_err(io_at(Path::new("tempdir")))?;
        let resolved = resolve_new_path(&directory.path().join("a/b"))?;
        assert_eq!(resolved, canonical(directory.path())?.join("a").join("b"));
        assert!(!resolved.exists());
        Ok(())
    }
}
