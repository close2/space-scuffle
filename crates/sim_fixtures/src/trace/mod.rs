//! A per-tick trace of a two-sided battle, built only from the world's public state.
//!
//! # Responsibility
//! [`BattleTrace`] watches a [`SimWorld`] between ticks and records, for each tick and each side:
//! how many units are alive and how much life and shields they hold; the health each side lost;
//! the swings started, the shots released, the swings cancelled (with their cause) and the shots
//! wasted on a target that an earlier shot of the same tick had already killed; the target
//! switches; and what every unit was doing ([`Activity`]: in contact, blocked, flanking,
//! approaching, walking or idle). It also keeps every unit's position, velocity, life, shields,
//! weapon phase and target per tick. [`summary`] derives the per-battle measures from those
//! records (with [`focus`] for who targets whom), and [`report`] writes them as CSV and Markdown.
//!
//! # How it fits
//! This is the instrumentation of work package 3 of the battle 33 investigation
//! (`docs/plans/2026-11-04-zealot-marine-investigation.md`, D-2). The runner
//! `crates/sim_core/examples/matchup_trace.rs` plays matrix battles with
//! [`crate::matchup::BattleRun`] and feeds each tick to a trace ([`cli`]). The simulation is not
//! changed: what it does not publish (releases, cancellations, wasted shots) is inferred from
//! consecutive states, and every inference is unit-tested here and on scripted battles in
//! `crates/sim_fixtures/tests/trace_tests.rs`. Its whole-matrix mode ([`matrix`], plan
//! `2026-11-04-combat-sc2-alignment` WP0) plays every battle without a trace and tabulates the
//! outcomes, so each step of that plan can be compared with the previous one.
//!
//! # Invariants
//! - The trace only reads the world; recording a battle does not change its outcome.
//! - Units are judged with the weapon their archetype uses against the other unit's layer, and
//!   with the authored `max_range`. Range upgrades are not applied; no battle the investigation
//!   traces researches one.
//! - Health lost by a unit that died is its whole health at the end of the previous tick:
//!   damage beyond that (overkill) is not health lost and is not counted.
//! - A living unit released a shot in a tick exactly when its weapon cooldown rose
//!   ([`swing::infer`]), whatever phases the release passed through.
//! - A unit that died in the tick it released is counted as releasing when its swing was at its
//!   last wind-up tick and its target within reach (see `count_releases_of_the_fallen`).
//! - Wasted shots are counted for instant weapons only (hitscan and melee). A projectile lands
//!   in a later tick, so its release is counted, but never as wasted.

pub mod activity;
pub mod cli;
pub mod focus;
pub mod matrix;
pub mod report;
pub mod route;
pub mod summary;
pub mod swing;

pub use activity::{classify, Activity, ActivityInput};
pub use focus::{focus_stats, FocusStats};
pub use summary::{BattleSummary, ContactStats, SecondRow, TeamTotals};
pub use swing::{infer, SwingEvent};

use sim_core::{
    calculate_hit_damage_upgraded, Entity, SimCoreError, SimWorld, UnitArchetype, WeaponPhase,
    WeaponSpec, RELEASE_RANGE_SLOP,
};
use sim_types::{
    EntityId, PlayerId, PlayerUpgradeState, SimScalar, SimVec2, UnitOrder, UnitTypeId,
};
use std::collections::BTreeMap;

/// One of the two sides of a traced battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Team {
    /// The first side (the matrix's blue, player 0).
    Blue,
    /// The second side (the matrix's red, player 1).
    Red,
}

impl Team {
    /// Both sides, in report order.
    pub const BOTH: [Self; 2] = [Self::Blue, Self::Red];

    /// Position of the side in [`Self::BOTH`] and in per-side arrays.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Blue => 0,
            Self::Red => 1,
        }
    }

    /// Lower-case name used in CSV headers and reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Red => "red",
        }
    }
}

/// Why a trace could not be started or continued.
#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    /// A unit named as a side's member does not exist in the world.
    #[error("unit {0:?} is not in the world")]
    MissingUnit(EntityId),
    /// A unit is listed on both sides.
    #[error("unit {0:?} is listed on both sides")]
    DuplicateUnit(EntityId),
    /// A unit's archetype is not in the world's catalog.
    #[error("unit {unit:?} has no archetype in the catalog: {source}")]
    Archetype {
        /// The unit.
        unit: EntityId,
        /// The catalog's reason.
        #[source]
        source: SimCoreError,
    },
}

/// One living unit's observable state at the end of one tick.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitSample {
    /// The unit.
    pub id: EntityId,
    /// Its side.
    pub team: Team,
    /// Its archetype.
    pub unit_type: UnitTypeId,
    /// Its owner.
    pub owner: PlayerId,
    /// Centre, in world units.
    pub position: SimVec2,
    /// Displacement achieved during the tick, in world units per tick.
    pub velocity: SimVec2,
    /// Life, in hit points.
    pub hp: SimScalar,
    /// Shields, in hit points.
    pub shields: SimScalar,
    /// Weapon phase after the tick.
    pub phase: WeaponPhase,
    /// Ticks left on the weapon's cooldown after the tick (`Entity::weapon_cooldown_remaining`).
    /// It rises only when a shot is released, which is how [`swing::infer`] detects releases.
    pub cooldown: u16,
    /// The enemy it is attacking: the target of its front attack order, otherwise its
    /// `current_target`.
    pub target: Option<EntityId>,
    /// What it was doing during the tick.
    pub activity: Activity,
}

/// One side's counters for one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TeamTick {
    /// Living units after the tick.
    pub alive: u32,
    /// Life plus shields of the living units after the tick, in hit points.
    pub health: f64,
    /// Life and shields this side lost during the tick, in hit points (dealt by the other side).
    pub damage_taken: f64,
    /// Life and shields this side regained during the tick, in hit points.
    pub health_regained: f64,
    /// Swings this side started.
    pub swings_started: u32,
    /// Shots or swings this side released.
    pub shots_released: u32,
    /// Swings cancelled because their target died or vanished.
    pub cancelled_target_gone: u32,
    /// Swings cancelled at release because the target was outside the band plus the slop.
    pub cancelled_out_of_band: u32,
    /// Swings cancelled for any other reason (an order, an unavailable target).
    pub cancelled_other: u32,
    /// Instant shots released at a target an earlier shot of the same tick had already killed.
    pub wasted_shots: u32,
    /// Units whose target changed from one living enemy to another.
    pub target_switches: u32,
    /// Living units per [`Activity`], indexed by [`Activity::index`].
    pub activity: [u32; 6],
    /// Engaged units that moved at least the stalled threshold: pushed while in contact.
    pub pushed_engaged: u32,
    /// Total distance moved by engaged units, in world units.
    pub pushed_distance: f64,
}

impl TeamTick {
    /// Every cancelled swing, whatever the cause.
    #[must_use]
    pub const fn cancelled(&self) -> u32 {
        self.cancelled_target_gone + self.cancelled_out_of_band + self.cancelled_other
    }

    /// Living units in contact ([`Activity::Engaged`]).
    #[must_use]
    pub const fn engaged(&self) -> u32 {
        self.activity[0]
    }
}

/// Both sides' counters for one tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickRecord {
    /// Ticks stepped when the record was taken; 0 is the state before the first tick.
    pub tick: u32,
    /// Per side, indexed by [`Team::index`].
    pub teams: [TeamTick; 2],
}

/// A unit's sample at one tick, as kept for the per-unit CSV.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitRow {
    /// Ticks stepped when the sample was taken.
    pub tick: u32,
    /// The sample.
    pub sample: UnitSample,
}

/// The per-tick record of one battle.
#[derive(Debug, Clone)]
pub struct BattleTrace {
    /// Side of every traced unit.
    teams: BTreeMap<EntityId, Team>,
    /// Units present at the start, per side.
    initial_units: [u32; 2],
    /// The latest sample of every unit alive at the previous record.
    previous: BTreeMap<EntityId, UnitSample>,
    /// One record per tick, starting with tick 0.
    ticks: Vec<TickRecord>,
    /// Every living unit's sample per tick.
    units: Vec<UnitRow>,
}

impl BattleTrace {
    /// Starts a trace of `world` with sides `blue` and `red`, recording the state before the
    /// first tick as tick 0.
    ///
    /// # Errors
    /// [`TraceError`] if a listed unit is missing, listed twice, or has no archetype.
    pub fn new(world: &SimWorld, blue: &[EntityId], red: &[EntityId]) -> Result<Self, TraceError> {
        let mut teams = BTreeMap::new();
        for (units, team) in [(blue, Team::Blue), (red, Team::Red)] {
            for &id in units {
                if !world.entities.contains_key(&id) {
                    return Err(TraceError::MissingUnit(id));
                }
                if teams.insert(id, team).is_some() {
                    return Err(TraceError::DuplicateUnit(id));
                }
            }
        }
        let initial_units = [count_u32(blue.len()), count_u32(red.len())];
        let mut trace = Self {
            teams,
            initial_units,
            previous: BTreeMap::new(),
            ticks: Vec::new(),
            units: Vec::new(),
        };
        trace.record_at(world, 0)?;
        Ok(trace)
    }

    /// Records the tick `world` has just stepped. Call once after every `SimWorld::step`.
    ///
    /// # Errors
    /// [`TraceError::Archetype`] if a unit's archetype has left the catalog.
    pub fn record(&mut self, world: &SimWorld) -> Result<(), TraceError> {
        let tick = self.ticks.last().map_or(0, |record| record.tick + 1);
        self.record_at(world, tick)
    }

    /// The per-tick records, starting with tick 0.
    #[must_use]
    pub fn ticks(&self) -> &[TickRecord] {
        &self.ticks
    }

    /// Every living unit's sample, per tick, in tick then id order.
    #[must_use]
    pub fn unit_rows(&self) -> &[UnitRow] {
        &self.units
    }

    /// Units present at the start, per side.
    #[must_use]
    pub fn initial_units(&self) -> [u32; 2] {
        self.initial_units
    }

    /// The per-battle measures derived from the records.
    #[must_use]
    pub fn summary(&self) -> BattleSummary {
        BattleSummary::from_trace(self)
    }

    fn record_at(&mut self, world: &SimWorld, tick: u32) -> Result<(), TraceError> {
        let mut record = TickRecord {
            tick,
            teams: [TeamTick::default(); 2],
        };
        let mut current = BTreeMap::new();
        let mut releases: Vec<(EntityId, EntityId)> = Vec::new();
        for (&id, &team) in &self.teams {
            let Some(entity) = world.entities.get(&id).filter(|e| e.is_alive()) else {
                continue;
            };
            let sample = self.sample(world, id, team, entity)?;
            let side = &mut record.teams[team.index()];
            side.alive += 1;
            side.health += health_of(&sample);
            side.activity[sample.activity.index()] += 1;
            if sample.activity == Activity::Engaged {
                let moved = length(sample.velocity);
                side.pushed_distance += moved;
                let max_speed = to_f64(archetype(world, id, entity)?.max_speed);
                if moved >= activity::STALLED_SPEED_FRACTION * max_speed {
                    side.pushed_engaged += 1;
                }
            }
            if let Some(before) = self.previous.get(&id) {
                Self::count_weapon_events(world, before, &sample, side, &mut releases)?;
                let switched = matches!((before.target, sample.target), (Some(old), Some(new))
                    if old != new && is_alive(world, old));
                side.target_switches += u32::from(switched);
            }
            current.insert(id, sample);
        }
        self.count_releases_of_the_fallen(world, &current, &mut record, &mut releases)?;
        self.count_health_changes(&current, &mut record);
        self.count_wasted_shots(world, &releases, &mut record)?;
        self.units.extend(current.values().map(|sample| UnitRow {
            tick,
            sample: sample.clone(),
        }));
        self.previous = current;
        self.ticks.push(record);
        Ok(())
    }

    /// The observable state of the living unit `entity`.
    fn sample(
        &self,
        world: &SimWorld,
        id: EntityId,
        team: Team,
        entity: &Entity,
    ) -> Result<UnitSample, TraceError> {
        let arch = archetype(world, id, entity)?;
        let target = attack_target(entity).filter(|&target| self.is_enemy(world, team, target));
        let enemy_in_band = self.teams.iter().any(|(&other, &other_team)| {
            other_team != team
                && world
                    .entities
                    .get(&other)
                    .filter(|enemy| enemy.is_alive())
                    .is_some_and(|enemy| in_band(world, entity, arch, enemy, SimScalar::ZERO))
        });
        let to_target = target
            .and_then(|target| world.entities.get(&target))
            .map(|enemy| {
                let offset = enemy.position - entity.position;
                (to_f64(offset.x), to_f64(offset.y))
            });
        let activity = classify(&ActivityInput {
            enemy_in_band,
            to_target,
            step: (to_f64(entity.velocity.x), to_f64(entity.velocity.y)),
            max_speed: to_f64(arch.max_speed),
        });
        Ok(UnitSample {
            id,
            team,
            unit_type: entity.unit_type_id,
            owner: entity.owner,
            position: entity.position,
            velocity: entity.velocity,
            hp: entity.hp,
            shields: entity.shields,
            phase: entity.weapon_phase.clone(),
            cooldown: entity.weapon_cooldown_remaining,
            target,
            activity,
        })
    }

    /// Whether `target` is a living unit of the side opposing `team`.
    fn is_enemy(&self, world: &SimWorld, team: Team, target: EntityId) -> bool {
        self.teams.get(&target).is_some_and(|&other| other != team) && is_alive(world, target)
    }

    /// Counts the swing started, released or cancelled between `before` and `after`.
    fn count_weapon_events(
        world: &SimWorld,
        before: &UnitSample,
        after: &UnitSample,
        side: &mut TeamTick,
        releases: &mut Vec<(EntityId, EntityId)>,
    ) -> Result<(), TraceError> {
        let event = infer(before, after);
        let released = match event {
            SwingEvent::Released { target } | SwingEvent::StartedAndReleased { target } => {
                Some(target)
            }
            _ => None,
        };
        if let Some(target) = released {
            side.shots_released += 1;
            if let Some(target) = target {
                releases.push((after.id, target));
            }
        }
        let (cancelled, started) = match event {
            SwingEvent::Idle | SwingEvent::WindingUp | SwingEvent::Released { .. } => (None, false),
            SwingEvent::Started { .. } | SwingEvent::StartedAndReleased { .. } => (None, true),
            SwingEvent::Cancelled { target } => (Some(target), false),
            SwingEvent::CancelledAndRestarted { cancelled, .. } => (Some(cancelled), true),
        };
        side.swings_started += u32::from(started);
        let Some(target) = cancelled else {
            return Ok(());
        };
        match world.entities.get(&target).filter(|e| e.is_alive()) {
            None => side.cancelled_target_gone += 1,
            Some(enemy) => {
                let entity = world
                    .entities
                    .get(&after.id)
                    .ok_or(TraceError::MissingUnit(after.id))?;
                let arch = archetype(world, after.id, entity)?;
                if in_band(world, entity, arch, enemy, RELEASE_RANGE_SLOP) {
                    side.cancelled_other += 1;
                } else {
                    side.cancelled_out_of_band += 1;
                }
            }
        }
        Ok(())
    }

    /// Counts the shots of units that released and died in the same tick.
    ///
    /// Every weapon advances before any shot is applied (`combat_resolution`), so a unit killed
    /// this tick still fired if its swing was due. Its last phase is gone with it, so the release
    /// is inferred: a swing at its last wind-up tick (`remaining <= 1`) whose bound target was in
    /// the band plus the release slop, measured from the unit's previous position (it stands still
    /// while winding up) to the target's latest position.
    ///
    /// This is the one release still read from a phase, because a dead unit publishes no
    /// cooldown. A swing that would start and release in the tick its unit dies (damage point 0
    /// after plan `2026-11-04-combat-sc2-alignment` D-3) left no wind-up behind and is not
    /// counted; the living units' releases come from the cooldown ([`swing::infer`]).
    fn count_releases_of_the_fallen(
        &self,
        world: &SimWorld,
        current: &BTreeMap<EntityId, UnitSample>,
        record: &mut TickRecord,
        releases: &mut Vec<(EntityId, EntityId)>,
    ) -> Result<(), TraceError> {
        for (id, before) in &self.previous {
            if current.contains_key(id) {
                continue;
            }
            let WeaponPhase::Windup {
                target_id,
                remaining_ticks,
            } = before.phase
            else {
                continue;
            };
            if remaining_ticks > 1 {
                continue;
            }
            let Some(target) = current
                .get(&target_id)
                .or_else(|| self.previous.get(&target_id))
            else {
                continue;
            };
            let arch = archetype_of(world, before)?;
            let enemy_arch = archetype_of(world, target)?;
            let Some(weapon) = arch.weapon_against(enemy_arch.mover_type, None) else {
                continue;
            };
            let distance = before.position.distance(target.position);
            if within_band(weapon, arch, enemy_arch, distance, RELEASE_RANGE_SLOP) {
                record.teams[before.team.index()].shots_released += 1;
                releases.push((*id, target_id));
            }
        }
        Ok(())
    }

    /// Adds every unit's health lost or regained since the previous record.
    fn count_health_changes(
        &self,
        current: &BTreeMap<EntityId, UnitSample>,
        record: &mut TickRecord,
    ) {
        for (id, before) in &self.previous {
            let side = &mut record.teams[before.team.index()];
            let now = current.get(id).map_or(0.0, health_of);
            let change = now - health_of(before);
            if change < 0.0 {
                side.damage_taken -= change;
            } else {
                side.health_regained += change;
            }
        }
    }

    /// Counts instant shots released at a target that died this tick after the shot that
    /// killed it.
    ///
    /// Shots are applied in attacker id order (`combat_resolution`, invariant 2), so the shots at
    /// one target are replayed in that order from its health at the previous record, with the
    /// simulation's own damage formula, until one kills it; every later shot was wasted.
    fn count_wasted_shots(
        &self,
        world: &SimWorld,
        releases: &[(EntityId, EntityId)],
        record: &mut TickRecord,
    ) -> Result<(), TraceError> {
        let mut by_target: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
        for &(attacker, target) in releases {
            by_target.entry(target).or_default().push(attacker);
        }
        for (target, mut attackers) in by_target {
            let Some(victim) = self.previous.get(&target) else {
                continue;
            };
            if is_alive(world, target) {
                continue;
            }
            attackers.sort_unstable();
            let killing = self.killing_shot(world, victim, &attackers)?;
            if let Some(killing) = killing {
                for attacker in &attackers[killing + 1..] {
                    if let Some(&team) = self.teams.get(attacker) {
                        record.teams[team.index()].wasted_shots += 1;
                    }
                }
            }
        }
        Ok(())
    }

    /// Index in `attackers` of the instant shot that kills `victim`, replaying their strikes from
    /// `victim`'s previous health; `None` if a projectile is involved or no shot kills it.
    ///
    /// Victim and attackers may have been reaped during the tick, so their archetypes and owners
    /// come from their previous samples.
    fn killing_shot(
        &self,
        world: &SimWorld,
        victim: &UnitSample,
        attackers: &[EntityId],
    ) -> Result<Option<usize>, TraceError> {
        let target_arch = archetype_of(world, victim)?;
        let no_upgrades = PlayerUpgradeState::default();
        let upgrades = |owner| world.player_upgrades.get(owner).unwrap_or(&no_upgrades);
        let (mut shields, mut hp) = (victim.shields, victim.hp);
        for (index, attacker_id) in attackers.iter().enumerate() {
            let Some(attacker) = self.previous.get(attacker_id) else {
                return Ok(None);
            };
            let arch = archetype_of(world, attacker)?;
            let siege = world
                .entities
                .get(attacker_id)
                .and_then(Entity::active_siege_weapon);
            let Some(weapon) = arch.weapon_against(target_arch.mover_type, siege) else {
                return Ok(None);
            };
            if weapon.projectile_speed.is_some() {
                return Ok(None);
            }
            // A release lands every strike only when they land together; a weapon that spaces
            // them lands one now and the rest on later ticks, which are not releases and so are
            // not replayed here (plan `2026-11-05-sc2-deviation-fixes` WP5).
            let strikes = if weapon.strike_interval_ticks == 0 {
                weapon.attacks_count
            } else {
                1
            };
            for _ in 0..strikes {
                let hit = calculate_hit_damage_upgraded(
                    weapon,
                    Some(&arch.slug),
                    upgrades(attacker.owner),
                    target_arch,
                    upgrades(victim.owner),
                    shields,
                    hp,
                );
                shields -= hit.shield_damage;
                hp -= hit.life_damage;
                if hit.killed {
                    return Ok(Some(index));
                }
            }
        }
        Ok(None)
    }
}

/// Converts a count of units to `u32`, saturating; battles hold far fewer units.
fn count_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// A fixed-point value as `f64`, for reporting only.
fn to_f64(value: SimScalar) -> f64 {
    f64::from(value.to_f32())
}

/// Length of a fixed-point vector as `f64`, for reporting only.
fn length(vector: SimVec2) -> f64 {
    to_f64(vector.x).hypot(to_f64(vector.y))
}

/// Life plus shields of a sample, in hit points.
fn health_of(sample: &UnitSample) -> f64 {
    to_f64(sample.hp) + to_f64(sample.shields)
}

/// Whether `id` exists and is alive.
fn is_alive(world: &SimWorld, id: EntityId) -> bool {
    world.entities.get(&id).is_some_and(Entity::is_alive)
}

/// The archetype of the unit `id`.
fn archetype<'w>(
    world: &'w SimWorld,
    id: EntityId,
    entity: &Entity,
) -> Result<&'w UnitArchetype, TraceError> {
    world
        .catalog
        .get(entity.unit_type_id)
        .map_err(|source| TraceError::Archetype { unit: id, source })
}

/// The archetype recorded in `sample`.
fn archetype_of<'w>(
    world: &'w SimWorld,
    sample: &UnitSample,
) -> Result<&'w UnitArchetype, TraceError> {
    world
        .catalog
        .get(sample.unit_type)
        .map_err(|source| TraceError::Archetype {
            unit: sample.id,
            source,
        })
}

/// The weapon `attacker` (of archetype `arch`) uses against a unit of archetype `target`.
fn weapon_against<'a>(
    arch: &'a UnitArchetype,
    attacker: &'a Entity,
    target: &UnitArchetype,
) -> Option<&'a WeaponSpec> {
    arch.weapon_against(target.mover_type, attacker.active_siege_weapon())
}

/// The enemy `entity` is attacking: its front attack order's target, otherwise its
/// `current_target`.
fn attack_target(entity: &Entity) -> Option<EntityId> {
    match entity.order_queue.front() {
        Some(UnitOrder::AttackTarget { target, .. }) => Some(*target),
        _ => entity.current_target,
    }
}

/// Whether `enemy` lies inside the engagement band of `entity`'s weapon against it, with `slop`
/// extra reach: `min_range + radii <= distance <= max_range + radii + slop`, centre to centre.
fn in_band(
    world: &SimWorld,
    entity: &Entity,
    arch: &UnitArchetype,
    enemy: &Entity,
    slop: SimScalar,
) -> bool {
    let Ok(enemy_arch) = world.catalog.get(enemy.unit_type_id) else {
        return false;
    };
    let Some(weapon) = weapon_against(arch, entity, enemy_arch) else {
        return false;
    };
    within_band(
        weapon,
        arch,
        enemy_arch,
        entity.position.distance(enemy.position),
        slop,
    )
}

/// Whether centres `distance` apart lie in `weapon`'s band between units of archetypes `arch`
/// and `enemy_arch`, with `slop` extra reach.
fn within_band(
    weapon: &WeaponSpec,
    arch: &UnitArchetype,
    enemy_arch: &UnitArchetype,
    distance: SimScalar,
    slop: SimScalar,
) -> bool {
    let radii = arch.collision_radius + enemy_arch.collision_radius;
    let closest = if weapon.min_range > SimScalar::ZERO {
        weapon.min_range + radii
    } else {
        SimScalar::ZERO
    };
    distance <= weapon.max_range + radii + slop && distance >= closest
}
