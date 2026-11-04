//! The "who wins" matchup matrix as data, and the mechanics of setting up and playing one battle.
//!
//! # Responsibility
//! Owns the schema of `crates/sim_core/tests/fixtures/matchup_matrix.ron`, the deterministic spawn
//! jitter of its later seeds, and [`BattleRun`], which spawns both sides, orders them on the first
//! tick and steps the world until one side is gone or the tick limit is reached.
//!
//! # How it fits
//! Two callers play the same battles and must play them identically: the matrix test
//! (`sc2_who_wins_matchup_matrix_tests.rs`), which checks each recorded outcome, and the battle
//! trace tool (`crates/sim_core/examples/matchup_trace.rs`, see [`crate::trace`]), which records
//! what happens inside a battle. Keeping the setup here gives both the one implementation. Units
//! are named only by the fixture's data (catalog slugs), never in Rust (charter §3.3).
//!
//! # Invariants
//! - Units are spawned, then ordered through `TickBundle` commands on the first tick, exactly as
//!   a player would. The one fixture a command cannot express is a hazard, placed directly.
//! - The simulation draws no randomness in these battles, so every seed after the first nudges
//!   each spawn point with [`Jitter`]; the first seed plays the authored positions unchanged.
//! - A setup that cannot be built whole (unknown unit or upgrade, a spawn off the map) is an
//!   error, never a battle with pieces silently missing.
//! - A battle is judged in one place, [`Expected::holds`] on an [`Outcome`], so the matrix test
//!   and the trace tool's matrix mode cannot disagree about which runs meet the fixture.

use serde::Deserialize;
use sim_core::catalog::RuntimeCatalogs;
use sim_core::{ActiveHazardZone, SimCoreError, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};

/// The shipped matchup matrix, `crates/sim_core/tests/fixtures/matchup_matrix.ron`.
pub const MATRIX_RON: &str = include_str!("../../sim_core/tests/fixtures/matchup_matrix.ron");

/// The blue side's player.
pub const BLUE: PlayerId = PlayerId(0);
/// The red side's player.
pub const RED: PlayerId = PlayerId(1);

/// Why a matrix battle could not be read or set up.
#[derive(Debug, thiserror::Error)]
pub enum MatchupError {
    /// The fixture text is not a valid matrix.
    #[error("the matchup fixture does not parse: {0}")]
    Parse(#[from] ron::error::SpannedError),
    /// No battle of that name exists in the fixture.
    #[error("the matchup fixture has no battle named `{0}`")]
    UnknownBattle(String),
    /// The arena could not be created.
    #[error("cannot create the arena: {0}")]
    Map(#[from] sim_map::MapError),
    /// A squad names a unit the catalog lacks.
    #[error("the catalog has no unit `{0}`")]
    UnknownUnit(String),
    /// A side's research could not be completed.
    #[error("cannot research `{upgrade}`: {source}")]
    Research {
        /// The upgrade slug from the fixture.
        upgrade: String,
        /// The simulation's reason.
        #[source]
        source: SimCoreError,
    },
    /// A unit could not be placed.
    #[error("cannot spawn `{unit}`: {source}")]
    Spawn {
        /// The unit slug from the fixture.
        unit: String,
        /// The simulation's reason.
        #[source]
        source: SimCoreError,
    },
    /// A side ordered to focus the enemy's first unit faces no enemy.
    #[error("`AttackFirstEnemy` needs an enemy unit, and the other side has none")]
    NoEnemyToFocus,
}

/// The whole matrix: seeds, jitter and battles.
#[derive(Debug, Clone, Deserialize)]
pub struct Matrix {
    /// World seeds of record; the first plays the authored positions unchanged.
    pub seeds: Vec<u64>,
    /// Largest spawn nudge per axis on later seeds, in world units.
    pub spawn_jitter: f32,
    /// Every battle, in fixture order.
    pub battles: Vec<Battle>,
}

impl Matrix {
    /// Parses a matrix from RON text.
    ///
    /// # Errors
    /// [`MatchupError::Parse`] if the text is not a valid matrix.
    pub fn from_ron_str(text: &str) -> Result<Self, MatchupError> {
        Ok(ron::from_str(text)?)
    }

    /// The battle named `name`.
    ///
    /// # Errors
    /// [`MatchupError::UnknownBattle`] if no battle has that name.
    pub fn battle(&self, name: &str) -> Result<&Battle, MatchupError> {
        self.battles
            .iter()
            .find(|battle| battle.name == name)
            .ok_or_else(|| MatchupError::UnknownBattle(name.to_owned()))
    }

    /// The spawn jitter, in world units per axis, that `seed` plays with: none for the first
    /// seed of record, [`Self::spawn_jitter`] for every other seed.
    #[must_use]
    pub fn jitter_for(&self, seed: u64) -> f32 {
        if self.seeds.first() == Some(&seed) {
            0.0
        } else {
            self.spawn_jitter
        }
    }
}

/// One battle and its recorded result.
#[derive(Debug, Clone, Deserialize)]
pub struct Battle {
    /// Unique name, used to select and report the battle.
    pub name: String,
    /// What the battle is meant to demonstrate.
    pub claim: String,
    /// Edge length of the square flat arena, in cells.
    pub map_cells: u32,
    /// Ticks allowed before the battle is judged where it stands.
    pub ticks: u32,
    /// Player 0's forces and orders.
    pub blue: Side,
    /// Player 1's forces and orders.
    pub red: Side,
    /// An area hazard already on the map, if any.
    #[serde(default)]
    pub hazard: Option<HazardFixture>,
    /// The recorded result.
    pub expect: Expected,
}

/// One player's forces and instructions.
#[derive(Debug, Clone, Deserialize)]
pub struct Side {
    /// Rectangular groups of identical units, spawned in order.
    pub squads: Vec<Squad>,
    /// The standing order, issued once on the first tick.
    #[serde(default)]
    pub order: SideOrder,
    /// Upgrade slugs completed before anything spawns.
    #[serde(default)]
    pub research: Vec<String>,
    /// Issues `UseStim` to the whole side on the first tick.
    #[serde(default)]
    pub stim: bool,
}

/// A rectangular group of identical units.
#[derive(Debug, Clone, Deserialize)]
pub struct Squad {
    /// Catalog slug of the unit.
    pub unit: String,
    /// Number of units.
    pub count: u32,
    /// Units per row; rows grow along +y.
    pub columns: u32,
    /// World position of the first unit.
    pub origin: (f32, f32),
    /// Distance between neighbouring grid points, in world units.
    pub spacing: f32,
}

/// The side's standing order, issued once on the first tick.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub enum SideOrder {
    /// No order: units stand and acquire targets on their own.
    #[default]
    None,
    /// Attack-move to the world point.
    AttackMove(f32, f32),
    /// Focus fire on the enemy side's first unit.
    AttackFirstEnemy,
    /// Hold position.
    HoldPosition,
}

/// An area hazard already on the map, owned by the blue player.
#[derive(Debug, Clone, Deserialize)]
pub struct HazardFixture {
    /// Centre in world units.
    pub center: (f32, f32),
    /// Radius in world units.
    pub radius: i32,
    /// Ticks the hazard lasts.
    pub remaining_ticks: u16,
    /// Ticks between damage pulses.
    pub interval_ticks: u16,
    /// Damage per pulse, in hit points.
    pub damage_per_interval: i32,
}

/// The recorded result of a battle.
#[derive(Debug, Clone, Deserialize)]
pub struct Expected {
    /// Inclusive blue survivor count range.
    pub blue: (usize, usize),
    /// Inclusive red survivor count range.
    pub red: (usize, usize),
    /// Inclusive hull range of blue's first unit, when it survives.
    #[serde(default)]
    pub blue_lead_hp: Option<(f32, f32)>,
    /// Inclusive hull range of red's first unit, when it survives.
    #[serde(default)]
    pub red_lead_hp: Option<(f32, f32)>,
}

impl Expected {
    /// Whether `outcome` meets this expectation: both survivor counts inside their ranges, and
    /// each side's lead hull inside its range when both the range and a living lead unit exist.
    ///
    /// The one judgement of a battle, shared by the matrix test and the trace tool's matrix
    /// mode, so the tool reports exactly the failures the test would.
    #[must_use]
    pub fn holds(&self, outcome: &Outcome) -> bool {
        (self.blue.0..=self.blue.1).contains(&outcome.blue)
            && (self.red.0..=self.red.1).contains(&outcome.red)
            && within(self.blue_lead_hp, outcome.blue_lead_hull)
            && within(self.red_lead_hp, outcome.red_lead_hull)
    }
}

/// Whether `value` lies in the inclusive `range`; true when either is absent (no range recorded,
/// or the lead unit died).
fn within(range: Option<(f32, f32)>, value: Option<SimScalar>) -> bool {
    match (range, value) {
        (Some((low, high)), Some(value)) => (low..=high).contains(&value.to_f32()),
        _ => true,
    }
}

/// What one playthrough of a battle came to, read where [`BattleRun`] stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// Living blue units.
    pub blue: usize,
    /// Living red units.
    pub red: usize,
    /// Life of blue's first unit, in hit points (shields not included); `None` once it died.
    pub blue_lead_hull: Option<SimScalar>,
    /// Life of red's first unit, in hit points (shields not included); `None` once it died.
    pub red_lead_hull: Option<SimScalar>,
}

impl Outcome {
    /// The outcome of `run` as it stands (normally once [`BattleRun::is_finished`]).
    #[must_use]
    pub fn of(run: &BattleRun) -> Self {
        Self {
            blue: living(&run.world, &run.blue),
            red: living(&run.world, &run.red),
            blue_lead_hull: lead_hull(&run.world, &run.blue),
            red_lead_hull: lead_hull(&run.world, &run.red),
        }
    }
}

/// Life of the first of `units`, if it exists and is alive.
fn lead_hull(world: &SimWorld, units: &[EntityId]) -> Option<SimScalar> {
    let lead = world.entities.get(units.first()?)?;
    lead.is_alive().then_some(lead.hp)
}

/// Deterministic spawn nudges for one seed, drawn from a small xorshift generator.
#[derive(Debug, Clone)]
pub struct Jitter {
    state: u64,
    scale: f32,
}

impl Jitter {
    /// A generator for `seed` whose nudges lie in `-scale..=scale` world units.
    #[must_use]
    pub fn new(seed: u64, scale: f32) -> Self {
        Self {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            scale,
        }
    }

    /// Next nudge in `-scale..=scale` world units.
    #[expect(
        clippy::cast_precision_loss,
        reason = "spawn nudges are test scenery; any 16 bits of spread are enough"
    )]
    pub fn next_nudge(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        let unit = (self.state >> 48) as f32 / f32::from(u16::MAX);
        (unit * 2.0 - 1.0) * self.scale
    }
}

/// The fixed-point world point of fixture coordinates `(x, y)`.
#[must_use]
pub fn point(x: f32, y: f32) -> SimVec2 {
    SimVec2::new(SimScalar::from_f32_lossy(x), SimScalar::from_f32_lossy(y))
}

/// Completes `side`'s research for `owner`, then spawns its squads row by row, nudged by
/// `jitter`. Returns the spawned entities in spawn order.
///
/// # Errors
/// [`MatchupError::Research`], [`MatchupError::UnknownUnit`] or [`MatchupError::Spawn`].
pub fn spawn_side(
    world: &mut SimWorld,
    owner: PlayerId,
    side: &Side,
    jitter: &mut Jitter,
) -> Result<Vec<EntityId>, MatchupError> {
    for upgrade in &side.research {
        world
            .research_upgrade(owner, upgrade)
            .map_err(|source| MatchupError::Research {
                upgrade: upgrade.clone(),
                source,
            })?;
    }
    let mut units = Vec::new();
    for squad in &side.squads {
        let unit_type = world
            .catalog
            .id_by_slug(&squad.unit)
            .ok_or_else(|| MatchupError::UnknownUnit(squad.unit.clone()))?;
        for index in 0..squad.count {
            let (row, column) = (index / squad.columns, index % squad.columns);
            #[expect(
                clippy::cast_precision_loss,
                reason = "grid indices are far below f32's integer range"
            )]
            let position = point(
                squad.origin.0 + column as f32 * squad.spacing + jitter.next_nudge(),
                squad.origin.1 + row as f32 * squad.spacing + jitter.next_nudge(),
            );
            let unit = world
                .spawn_unit(unit_type, owner, position)
                .map_err(|source| MatchupError::Spawn {
                    unit: squad.unit.clone(),
                    source,
                })?;
            units.push(unit);
        }
    }
    Ok(units)
}

/// The commands that give `side`'s `units` their standing order (and stim) on the first tick.
///
/// # Errors
/// [`MatchupError::NoEnemyToFocus`] if the order is `AttackFirstEnemy` and `enemies` is empty.
pub fn side_commands(
    owner: PlayerId,
    side: &Side,
    units: &[EntityId],
    enemies: &[EntityId],
) -> Result<Vec<PlayerCommand>, MatchupError> {
    let command = |order| PlayerCommand {
        player_id: owner,
        entities: units.to_vec(),
        order,
        queue: false,
    };
    let order = match side.order {
        SideOrder::None => None,
        SideOrder::AttackMove(x, y) => Some(UnitOrder::AttackMove {
            target: point(x, y),
        }),
        SideOrder::AttackFirstEnemy => Some(UnitOrder::attack(
            *enemies.first().ok_or(MatchupError::NoEnemyToFocus)?,
        )),
        SideOrder::HoldPosition => Some(UnitOrder::HoldPosition),
    };
    let mut commands: Vec<_> = order.into_iter().map(command).collect();
    if side.stim {
        commands.push(command(UnitOrder::UseStim));
    }
    Ok(commands)
}

/// Places `hazard`, owned by the blue player, directly on the map.
fn place_hazard(world: &mut SimWorld, hazard: &HazardFixture) {
    world.active_hazards.push(ActiveHazardZone {
        id: 1,
        owner: BLUE,
        center: point(hazard.center.0, hazard.center.1),
        radius: SimScalar::from_num(hazard.radius),
        delay_ticks: 0,
        remaining_ticks: hazard.remaining_ticks,
        interval_ticks: hazard.interval_ticks,
        ticks_until_next_interval: 1,
        damage_per_interval: SimScalar::from_num(hazard.damage_per_interval),
        targets_friendly: false,
    });
}

/// Number of `units` that exist and are alive in `world`.
#[must_use]
pub fn living(world: &SimWorld, units: &[EntityId]) -> usize {
    units
        .iter()
        .filter(|id| {
            world
                .entities
                .get(id)
                .is_some_and(sim_core::Entity::is_alive)
        })
        .count()
}

/// One battle in progress: the world, both sides' units and the first tick's commands.
pub struct BattleRun {
    /// The battle's world; read it between steps, never mutate it.
    pub world: SimWorld,
    /// Blue's units in spawn order.
    pub blue: Vec<EntityId>,
    /// Red's units in spawn order.
    pub red: Vec<EntityId>,
    /// Commands issued on the next tick: the standing orders before the first tick, then none.
    commands: Vec<PlayerCommand>,
    /// Ticks stepped so far.
    ticks_played: u32,
    /// The tick limit.
    tick_limit: u32,
}

impl BattleRun {
    /// Builds `battle`'s world with `catalogs` and RNG `seed`, nudging spawns by up to `jitter`
    /// world units per axis, and prepares the first tick's commands.
    ///
    /// # Errors
    /// Every [`MatchupError`] of the arena and the two sides' setup.
    pub fn new(
        battle: &Battle,
        catalogs: RuntimeCatalogs,
        seed: u64,
        jitter: f32,
    ) -> Result<Self, MatchupError> {
        let map = GameMap::new(
            battle.map_cells,
            battle.map_cells,
            MapCell::default_ground(CliffLevel(0)),
        )?;
        let mut world = SimWorld::with_catalogs(map, seed, catalogs.units, catalogs.upgrades);
        let mut nudges = Jitter::new(seed, jitter);
        let blue = spawn_side(&mut world, BLUE, &battle.blue, &mut nudges)?;
        let red = spawn_side(&mut world, RED, &battle.red, &mut nudges)?;
        // Rule fixture: no shipped unit casts a hazard, so the storm is placed directly.
        if let Some(hazard) = &battle.hazard {
            place_hazard(&mut world, hazard);
        }
        let mut commands = side_commands(BLUE, &battle.blue, &blue, &red)?;
        commands.extend(side_commands(RED, &battle.red, &red, &blue)?);
        Ok(Self {
            world,
            blue,
            red,
            commands,
            ticks_played: 0,
            tick_limit: battle.ticks,
        })
    }

    /// Whether the battle is over: the tick limit is reached, or, when both sides started with
    /// units, one side has none alive.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        let two_sided = !self.blue.is_empty() && !self.red.is_empty();
        self.ticks_played >= self.tick_limit
            || (two_sided
                && (living(&self.world, &self.blue) == 0 || living(&self.world, &self.red) == 0))
    }

    /// Ticks stepped so far.
    #[must_use]
    pub fn ticks_played(&self) -> u32 {
        self.ticks_played
    }

    /// Steps one tick, issuing the standing orders on the first, and returns the diagnostics the
    /// simulation raised during it. Does nothing once [`Self::is_finished`] is true.
    pub fn step(&mut self) -> Vec<sim_core::SimulationDiagnostic> {
        if self.is_finished() {
            return Vec::new();
        }
        self.world.step(&TickBundle {
            tick: Tick(self.ticks_played),
            commands: std::mem::take(&mut self.commands),
            state_checksum: None,
        });
        self.ticks_played += 1;
        self.world.drain_diagnostics().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_matrix_parses_and_names_its_battles_uniquely() -> Result<(), MatchupError> {
        let matrix = Matrix::from_ron_str(MATRIX_RON)?;
        let mut names: Vec<_> = matrix.battles.iter().map(|b| b.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), matrix.battles.len());
        assert!(matches!(
            matrix.battle("no such battle"),
            Err(MatchupError::UnknownBattle(_))
        ));
        Ok(())
    }

    #[test]
    fn only_the_first_seed_of_record_plays_without_jitter() -> Result<(), MatchupError> {
        let matrix = Matrix::from_ron_str(MATRIX_RON)?;
        let first = matrix.seeds[0];
        assert!(matrix.jitter_for(first).abs() < f32::EPSILON);
        for &seed in &matrix.seeds[1..] {
            assert!((matrix.jitter_for(seed) - matrix.spawn_jitter).abs() < f32::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn nudges_stay_within_scale_and_differ_between_seeds() {
        let mut first = Jitter::new(11, 0.12);
        let mut second = Jitter::new(22, 0.12);
        let a = first.next_nudge();
        assert!(a.abs() <= 0.12);
        assert!((a - second.next_nudge()).abs() > f32::EPSILON);
    }

    fn outcome(blue: usize, red: usize, blue_lead: Option<f32>) -> Outcome {
        Outcome {
            blue,
            red,
            blue_lead_hull: blue_lead.map(SimScalar::from_f32_lossy),
            red_lead_hull: None,
        }
    }

    #[test]
    fn an_expectation_holds_inside_its_ranges_and_ignores_a_dead_lead() {
        let expected = Expected {
            blue: (1, 2),
            red: (0, 0),
            blue_lead_hp: Some((10.0, 20.0)),
            red_lead_hp: Some((1.0, 2.0)),
        };
        assert!(expected.holds(&outcome(1, 0, Some(10.0))));
        assert!(expected.holds(&outcome(2, 0, Some(20.0))));
        // A dead lead unit has no hull to judge; the survivor counts still apply.
        assert!(expected.holds(&outcome(2, 0, None)));
        assert!(!expected.holds(&outcome(3, 0, Some(15.0))));
        assert!(!expected.holds(&outcome(1, 1, Some(15.0))));
        assert!(!expected.holds(&outcome(1, 0, Some(9.5))));
        assert!(!expected.holds(&outcome(1, 0, Some(20.5))));
    }

    #[test]
    fn focusing_an_empty_enemy_side_is_an_error() {
        let side = Side {
            squads: Vec::new(),
            order: SideOrder::AttackFirstEnemy,
            research: Vec::new(),
            stim: false,
        };
        assert!(matches!(
            side_commands(BLUE, &side, &[], &[]),
            Err(MatchupError::NoEnemyToFocus)
        ));
    }
}
