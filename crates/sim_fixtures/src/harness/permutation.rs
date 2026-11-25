//! The permutation mode of the movement harness (plan `2026-11-27-gated-movement` §5.2, R-13,
//! H-3).
//!
//! # Responsibility
//! [`permutation_scene`] plays one scene of any family twice in lockstep: as generated, and with
//! the processing order inside every tick rotated by an offset through the world's determinism
//! probe ([`sim_core::SimWorld::set_movement_order_rotation`]). After every tick it compares
//! every unit's position and life in the two worlds, bit for bit, and reports in a
//! [`PermutationRow`] the first tick on which they parted, how many unit-ticks differed, and
//! whether that first difference is an exact tie: two living bodies whose centres coincided at
//! the start of that tick, a case the movement code breaks by order (`coincident`). Any other
//! difference is unexplained.
//!
//! # How it fits
//! `movement_harness --permute <offset>` writes one family's rows as CSV ([`permutation_csv`]).
//! Relabelling (`--relabel`) permutes entity ids, which changes id tie-breaks; the rotation keeps
//! the ids and changes only the order, so a difference here is order dependence. The plan judges
//! it only under the `Gated` pipeline (H-3), where every read is of the tick-start snapshot; for
//! the shipped pipelines it is reported.
//!
//! # Invariants
//! - Offset 0 plays the same world twice, so it reports no difference.
//! - A row is a function of `(family, seed, offset, rules)`, and rows are written in seed order.

use super::corpus::{generate, Scenario};
use super::{mirror, Family, HarnessError, RunOptions, ScenePlay};
use sim_core::SimWorld;
use sim_types::{EntityId, SimVec2, Tick, TickBundle};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// One scene played in both orders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermutationRow {
    /// Family name.
    pub family: String,
    /// Seed.
    pub seed: u64,
    /// The rotation offset of the second play.
    pub offset: u32,
    /// Ticks played.
    pub ticks: u32,
    /// The first tick (counted from 1) after which the plays differed; `None` if they never did.
    pub first_difference: Option<u32>,
    /// Unit-ticks whose position or life differed between the plays.
    pub differing_unit_ticks: u32,
    /// Whether two living bodies of the first play had coincident centres at the start of the
    /// first differing tick (an exact tie, broken by order).
    pub coincident: bool,
}

impl PermutationRow {
    /// The CSV header, in [`Self::to_csv`] order.
    pub const HEADER: &'static str =
        "family,seed,offset,ticks,first_difference,differing_unit_ticks,class";

    /// The kind of the row's result: `identical`, `tie` (the first difference followed an exact
    /// tie) or `unexplained`.
    #[must_use]
    pub const fn class(&self) -> &'static str {
        match (self.first_difference, self.coincident) {
            (None, _) => "identical",
            (Some(_), true) => "tie",
            (Some(_), false) => "unexplained",
        }
    }

    /// The row as one CSV line, without a line end; an absent first difference is empty.
    #[must_use]
    pub fn to_csv(&self) -> String {
        format!(
            "{},{},{},{},{},{},{}",
            self.family,
            self.seed,
            self.offset,
            self.ticks,
            self.first_difference
                .map_or_else(String::new, |tick| tick.to_string()),
            self.differing_unit_ticks,
            self.class()
        )
    }
}

/// `rows` as CSV, header first.
#[must_use]
pub fn permutation_csv(rows: &[PermutationRow]) -> String {
    let mut text = format!("{}\n", PermutationRow::HEADER);
    for row in rows {
        text.push_str(&row.to_csv());
        text.push('\n');
    }
    text
}

/// Every unit's id, life and position in `world`, in id order.
fn state(world: &SimWorld) -> Vec<(EntityId, bool, SimVec2)> {
    world
        .entities
        .iter()
        .map(|(&id, entity)| (id, entity.is_alive(), entity.position))
        .collect()
}

/// Whether two living bodies of `state` share a centre exactly.
fn coincident(state: &[(EntityId, bool, SimVec2)]) -> bool {
    let mut centres: Vec<SimVec2> = state
        .iter()
        .filter(|(_, alive, _)| *alive)
        .map(|&(_, _, position)| position)
        .collect();
    centres.sort_by_key(|p| (p.x.raw(), p.y.raw()));
    centres.windows(2).any(|pair| pair[0] == pair[1])
}

/// The comparison of two plays, tick by tick.
#[derive(Debug, Clone, Default)]
struct Comparison {
    ticks: u32,
    first_difference: Option<u32>,
    differing_unit_ticks: u32,
    coincident: bool,
}

impl Comparison {
    /// Adds a tick whose start was `before` (in the first play) and whose ends were `a` and `b`.
    fn add(
        &mut self,
        before: &[(EntityId, bool, SimVec2)],
        a: &[(EntityId, bool, SimVec2)],
        b: &[(EntityId, bool, SimVec2)],
    ) {
        self.ticks += 1;
        let differing = a.iter().zip(b).filter(|(x, y)| x != y).count() + a.len().abs_diff(b.len());
        if differing > 0 && self.first_difference.is_none() {
            self.first_difference = Some(self.ticks);
            self.coincident = coincident(before);
        }
        self.differing_unit_ticks = self
            .differing_unit_ticks
            .saturating_add(u32::try_from(differing).unwrap_or(u32::MAX));
    }
}

/// Plays `scenario` twice under `options`, the second time rotated by `offset`.
fn compare_scene(
    scenario: &Scenario,
    offset: u32,
    options: &RunOptions,
) -> Result<Comparison, HarnessError> {
    let plain = RunOptions {
        order_rotation: 0,
        ..*options
    };
    let rotated = RunOptions {
        order_rotation: offset,
        ..*options
    };
    let mut first = ScenePlay::new(scenario, &plain)?;
    let mut second = ScenePlay::new(scenario, &rotated)?;
    let mut comparison = Comparison::default();
    for _ in 0..scenario.ticks {
        let before = state(&first.world);
        first.step();
        second.step();
        comparison.add(&before, &state(&first.world), &state(&second.world));
    }
    Ok(comparison)
}

/// Plays the mirror battle of `seed` twice under `options`, the second time rotated by `offset`,
/// each to its own end; a play that ends earlier differs on every later tick.
fn compare_mirror(
    seed: u64,
    offset: u32,
    options: &RunOptions,
) -> Result<Comparison, HarnessError> {
    let build = |rotation: u32| {
        mirror::build(
            seed,
            &RunOptions {
                order_rotation: rotation,
                ..*options
            },
        )
    };
    let (mut first, sides) = build(0)?;
    let (mut second, _) = build(offset)?;
    let mut comparison = Comparison::default();
    for tick in 0..mirror::LIMIT_TICKS {
        let a = mirror::commands_for(&first, &sides, tick);
        let b = mirror::commands_for(&second, &sides, tick);
        if a.is_none() && b.is_none() {
            break;
        }
        let before = state(&first);
        for (world, commands) in [(&mut first, a), (&mut second, b)] {
            if let Some(commands) = commands {
                world.step(&TickBundle {
                    tick: Tick(world.current_tick),
                    commands,
                    state_checksum: None,
                });
            }
        }
        comparison.add(&before, &state(&first), &state(&second));
    }
    Ok(comparison)
}

/// The permutation row of `family`'s scene `seed` at rotation `offset` under `options`.
///
/// # Errors
/// [`HarnessError::World`] when a world cannot be built.
pub fn permutation_scene(
    family: Family,
    seed: u64,
    offset: u32,
    options: &RunOptions,
) -> Result<PermutationRow, HarnessError> {
    let comparison = if family == Family::Mirror {
        compare_mirror(seed, offset, options)?
    } else {
        compare_scene(&generate(family, seed, false), offset, options)?
    };
    Ok(PermutationRow {
        family: family.name().to_owned(),
        seed,
        offset,
        ticks: comparison.ticks,
        first_difference: comparison.first_difference,
        differing_unit_ticks: comparison.differing_unit_ticks,
        coincident: comparison.coincident,
    })
}

/// The permutation rows of `count` seeds of `family` from `from` at rotation `offset`, played on
/// `jobs` threads, in seed order.
///
/// # Errors
/// The first [`HarnessError`] of any scene.
pub fn permutation_range(
    family: Family,
    (from, count): (u64, u64),
    offset: u32,
    jobs: usize,
    options: &RunOptions,
) -> Result<Vec<PermutationRow>, HarnessError> {
    let seeds: Vec<u64> = (from..from.saturating_add(count)).collect();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, Result<PermutationRow, HarnessError>)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(&seed) = seeds.get(index) else {
                    break;
                };
                let row = permutation_scene(family, seed, offset, options);
                if let Ok(mut guard) = results.lock() {
                    guard.push((index, row));
                }
            });
        }
    });
    let mut results = results
        .into_inner()
        .map_err(|_| HarnessError::Io("a worker thread panicked".to_owned()))?;
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, row)| row).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::SimScalar;

    fn at(x: i32, y: i32) -> SimVec2 {
        SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y))
    }

    #[test]
    fn the_first_difference_is_classified_by_the_tick_start_it_followed() {
        let (one, two) = (EntityId(1), EntityId(2));
        let apart = vec![(one, true, at(0, 0)), (two, true, at(3, 0))];
        let together = vec![(one, true, at(1, 1)), (two, true, at(1, 1))];
        let moved = vec![(one, true, at(2, 1)), (two, true, at(1, 1))];
        let mut comparison = Comparison::default();
        comparison.add(&apart, &apart, &apart);
        assert_eq!(comparison.first_difference, None);
        comparison.add(&together, &moved, &together);
        assert_eq!(comparison.first_difference, Some(2));
        assert!(comparison.coincident);
        // One unit differed on the second tick, both on the third.
        comparison.add(&apart, &moved, &apart);
        assert_eq!(comparison.differing_unit_ticks, 3);
        let mut plain = Comparison::default();
        plain.add(&apart, &moved, &apart);
        assert!(!plain.coincident);
        // A dead body does not tie.
        let dead = vec![(one, false, at(1, 1)), (two, true, at(1, 1))];
        assert!(!coincident(&dead));
    }

    #[test]
    fn a_row_names_its_class_and_writes_an_absent_tick_empty() {
        let mut row = PermutationRow {
            family: "arc".to_owned(),
            seed: 4,
            offset: 7,
            ticks: 10,
            first_difference: None,
            differing_unit_ticks: 0,
            coincident: false,
        };
        assert_eq!(row.to_csv(), "arc,4,7,10,,0,identical");
        row.first_difference = Some(3);
        assert_eq!(row.class(), "unexplained");
        row.coincident = true;
        assert_eq!(row.class(), "tie");
    }
}
