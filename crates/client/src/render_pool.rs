//! The render-entity pool of the live battle: which Bevy entity draws which simulation unit, and
//! the parked entities of units that left (plan `2026-11-29-render-entity-pool`, B-237).
//!
//! # Responsibility
//! Owns [`RenderEntityMap`], the retained membership of the live synchroniser
//! (`scene_controller::sync_live_battle_system`): a vector of `(EntityId, Entity)` sorted by
//! simulation id, walked in one merge with the sorted snapshot, and a stack of parked entities a
//! later arrival takes instead of spawning. It owns the session's bound on the bubbles it can
//! present at once ([`RenderPoolBound`], derived from the catalog), the counters that say how
//! the pool was used ([`RenderPoolStats`]), and the [`ParkedBattleBubble`] marker.
//!
//! # How it fits
//! The synchroniser asks the map, per frame: which entries departed ([`RenderEntityMap::depart`]),
//! which entity draws a record ([`RenderEntityMap::find_from`]), and where an arrival is drawn
//! ([`RenderEntityMap::take_parked`], [`RenderEntityMap::arrive`]); it then merges the frame's
//! arrivals ([`RenderEntityMap::merge_arrivals`]). Restyling the entities is the synchroniser's
//! and the indicators' work; this module touches no component but by despawning on retirement.
//! A parked entity carries [`ParkedBattleBubble`] and no `LiveBattleBubble`, so every consumer
//! that queries `With<LiveBattleBubble>` never meets it (the plan's §5 audit).
//!
//! # Invariants
//! - `live` is sorted by simulation id and holds each id at most once; an entity is in exactly
//!   one of `live`, `parked` and the frame's `arrivals`.
//! - Within the bound reserved when a session is bound, no membership change allocates; beyond it
//!   the vectors grow, [`RenderPoolStats::beyond_bound`] counts every such spawn and one warning
//!   is logged per session.
//! - Simulation ids restart with a session, so the map follows the session generation and is
//!   emptied (by its owner's [`RenderEntityMap::retire_all`], or here) when it changes.

use crate::battle_pipeline::commands::SessionGeneration;
use bevy::prelude::*;
use sim_core::catalog::{UnitArchetype, UnitCatalog};
use sim_types::ids::{EntityId, UnitTypeId};
use sim_types::snapshot::BubbleRenderInstance;
use sim_types::weapon::WeaponSpec;

/// Marker of a parked render entity: the bubble of a unit that left the battle, hidden and kept
/// with its materials and indicator children until an arrival reuses it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParkedBattleBubble;

/// The bound on the bubbles one session can present at once, with the terms that set it
/// (plan D-3): `total = start + players × (army + transient)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPoolBound {
    /// Records in the session's first presented snapshot (`N₀`).
    pub start: u32,
    /// Players of the session (`|P|`).
    pub players: u32,
    /// Units one player's supply can add by training (`A`): the supply cap over the cheapest
    /// trained unit's cost, a free unit counting as costing 1.
    pub army: u32,
    /// Units one player's supply can keep presented that arrive and leave without a command
    /// (`T`): hangar members and timed spawns of the host with the most per supply.
    pub transient: u32,
    /// The host archetype that set `transient`, if any.
    pub host: Option<UnitTypeId>,
}

impl RenderPoolBound {
    /// Derives the bound of a session of `players` players over `catalog` whose first presented
    /// snapshot has `start` records. Pure: reads the catalog only.
    #[must_use]
    pub fn for_session(catalog: &UnitCatalog, players: usize, start: usize) -> Self {
        let (transient, host) = transient_term(catalog);
        Self {
            start: u32::try_from(start).unwrap_or(u32::MAX),
            players: u32::try_from(players).unwrap_or(u32::MAX),
            army: army_term(catalog),
            transient,
            host,
        }
    }

    /// The bound of a session with no catalog to read (no world is presented): its start alone,
    /// with no army or transient term.
    #[must_use]
    pub fn without_catalog(players: usize, start: usize) -> Self {
        Self {
            start: u32::try_from(start).unwrap_or(u32::MAX),
            players: u32::try_from(players).unwrap_or(u32::MAX),
            army: 0,
            transient: 0,
            host: None,
        }
    }

    /// The bound `B`, in bubbles.
    #[must_use]
    pub fn total(&self) -> usize {
        let per_player = self.army.saturating_add(self.transient);
        let total = self
            .start
            .saturating_add(self.players.saturating_mul(per_player));
        usize::try_from(total).unwrap_or(usize::MAX)
    }
}

/// Supply cost of `archetype` as the bound counts it: a free unit counts as costing 1.
fn cost(archetype: &UnitArchetype) -> u32 {
    archetype.supply_cost.max(1)
}

/// `A`: the standard supply cap over the cost of the cheapest archetype any archetype trains, or
/// 0 when nothing is trained.
fn army_term(catalog: &UnitCatalog) -> u32 {
    let cap = sim_core::production::STANDARD_SUPPLY_CAP;
    catalog
        .archetypes()
        .flat_map(|trainer| trainer.trains_units.iter())
        .filter_map(|slug| catalog.get_by_slug(slug))
        .map(cost)
        .min()
        .map_or(0, |cheapest| cap / cheapest)
}

/// Every distinct weapon `archetype` can fire: its own and its modes' overrides (the archetype's
/// modes and its capabilities' copy of them). `weapon` mirrors `ground_weapon` for
/// compatibility, so equal specs are counted once.
fn weapons_of(archetype: &UnitArchetype) -> Vec<&WeaponSpec> {
    let mut weapons: Vec<&WeaponSpec> = Vec::new();
    let all_modes = archetype.modes.iter().chain(&archetype.capabilities.modes);
    let modes = all_modes.flat_map(|mode| {
        [
            mode.ground_weapon_override
                .as_ref()
                .and_then(Option::as_ref),
            mode.air_weapon_override.as_ref().and_then(Option::as_ref),
        ]
    });
    let own = [
        archetype.ground_weapon.as_ref(),
        archetype.air_weapon.as_ref(),
        archetype.weapon.as_ref(),
    ];
    for weapon in own.into_iter().chain(modes).flatten() {
        if !weapons.contains(&weapon) {
            weapons.push(weapon);
        }
    }
    weapons
}

/// Units a host's timed spawns keep alive at once (`spawn(h)` of plan D-3): with a strike
/// magazine, the charges armed or building at a lifetime's start plus those built within it,
/// times the units each strike carries; without one, one release per period over a lifetime.
/// Spawns without a timed life live until killed, like trained units, and add nothing.
fn spawn_term(archetype: &UnitArchetype) -> u32 {
    let magazine = archetype.capabilities.strike_magazine;
    weapons_of(archetype)
        .into_iter()
        .filter_map(|weapon| {
            let spawn = weapon.impact_spawn?;
            let life = spawn.timed_life?;
            let lifetime = u32::from(life.ticks) + u32::from(life.random_extra_ticks);
            let per_strike = u32::from(spawn.per_strike);
            Some(magazine.map_or_else(
                || {
                    let period = u32::from(weapon.period_ticks).max(1);
                    let releases = lifetime / period + 1;
                    let strikes = per_strike.saturating_mul(u32::from(weapon.attacks_count));
                    (false, strikes.saturating_mul(releases))
                },
                |magazine| {
                    let build = u32::from(magazine.build_ticks).max(1);
                    let charges = u32::from(magazine.capacity) + lifetime.div_ceil(build);
                    (true, per_strike.saturating_mul(charges))
                },
            ))
        })
        .fold(0, |total, (magazine, units)| {
            if magazine {
                total.max(units)
            } else {
                total.saturating_add(units)
            }
        })
}

/// `T` and the host that sets it: over every host, the supply cap times its transient units
/// per supply. Because a mix of hosts within the cap keeps at most `cap × max(m / cost)`
/// transient units, this bounds every mix, not only an army of one host type.
fn transient_term(catalog: &UnitCatalog) -> (u32, Option<UnitTypeId>) {
    let cap = u64::from(sim_core::production::STANDARD_SUPPLY_CAP);
    catalog
        .archetypes()
        .filter_map(|host| {
            let hangar = host
                .capabilities
                .hangar
                .map_or(0, |hangar| u32::from(hangar.max_count));
            let members = hangar.saturating_add(spawn_term(host));
            (members > 0).then(|| {
                let units = cap * u64::from(members) / u64::from(cost(host));
                (u32::try_from(units).unwrap_or(u32::MAX), host.unit_type_id)
            })
        })
        // Ties go to the lowest id, so the named host is stable.
        .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
        .map_or((0, None), |(units, host)| (units, Some(host)))
}

/// How the pool was used in the session (plan D-8), read by tests and the frame-budget report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderPoolStats {
    /// Entities spawned for arrivals that found no parked entity.
    pub spawned: u32,
    /// Arrivals that took a parked entity.
    pub reused: u32,
    /// Entities parked now.
    pub parked_now: u32,
    /// The most entities the pool held at once (live and parked), the session's high-water mark.
    pub high_water: u32,
    /// Spawns that took the pool beyond its bound.
    pub beyond_bound: u32,
}

/// How an arrival was drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// It took a parked entity.
    Reused,
    /// It was given a newly spawned entity.
    Spawned,
}

/// Persistent mapping from authoritative simulation [`EntityId`] to Bevy render [`Entity`], with
/// the parked entities of units that left (see the module docs).
// PERFORMANCE RATIONALE:
// Why: Membership changes with every arrival and departure, and a node-based map allocates on
// those; spawning a bubble per arrival adds its materials and child entities as well.
// What: A sorted vector reserved to the session's bound, walked in one merge with the sorted
// snapshot, so an update needs no lookup and a membership change only moves entries within
// reserved capacity; departed entities are parked for the next arrival.
// Trade-off: A removal or an out-of-order arrival moves O(n) 16-byte entries (a few
// microseconds at 2,000 units), the map relies on the snapshot's id order, and the pool keeps
// the session's high-water mark of entities and materials.
#[derive(Resource, Debug, Default)]
pub struct RenderEntityMap {
    /// Presented units and their entities, sorted by simulation id.
    live: Vec<(EntityId, Entity)>,
    /// Parked entities, a stack.
    parked: Vec<Entity>,
    /// The frame's arrivals, sorted by id, merged into `live` at the end of the frame.
    arrivals: Vec<(EntityId, Entity)>,
    /// The bound of the session the map follows.
    bound: Option<RenderPoolBound>,
    /// The session the map follows.
    generation: Option<SessionGeneration>,
    /// How the pool was used in the session.
    stats: RenderPoolStats,
    /// Whether the session's beyond-bound warning was logged.
    warned: bool,
}

impl RenderEntityMap {
    /// The entity that draws simulation unit `id`, if it is presented. Binary search.
    #[must_use]
    pub fn get(&self, id: EntityId) -> Option<Entity> {
        self.live
            .binary_search_by_key(&id, |entry| entry.0)
            .ok()
            .map(|index| self.live[index].1)
    }

    /// Presented units and their entities, in simulation id order.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, Entity)> + '_ {
        self.live.iter().copied()
    }

    /// Number of presented units.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live.len()
    }

    /// Whether no unit is presented.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// The parked entities.
    #[must_use]
    pub fn parked(&self) -> &[Entity] {
        &self.parked
    }

    /// How the pool was used in the session.
    #[must_use]
    pub fn stats(&self) -> RenderPoolStats {
        self.stats
    }

    /// The bound of the session the map follows, once it is bound.
    #[must_use]
    pub fn bound(&self) -> Option<RenderPoolBound> {
        self.bound
    }

    /// The session the map follows.
    #[must_use]
    pub fn generation(&self) -> Option<SessionGeneration> {
        self.generation
    }

    /// Whether the session's beyond-bound warning was logged.
    #[must_use]
    pub fn has_warned(&self) -> bool {
        self.warned
    }

    /// The retained vectors as `(name, address, capacity)`, for the steady-state tests.
    #[doc(hidden)]
    pub fn retained_buffers(&self) -> impl Iterator<Item = (&'static str, usize, usize)> + '_ {
        [
            (
                "render_pool.live",
                self.live.as_ptr().addr(),
                self.live.capacity(),
            ),
            (
                "render_pool.parked",
                self.parked.as_ptr().addr(),
                self.parked.capacity(),
            ),
            (
                "render_pool.arrivals",
                self.arrivals.as_ptr().addr(),
                self.arrivals.capacity(),
            ),
        ]
        .into_iter()
    }

    /// Follows the presented session `generation`. When it differs from the one followed, any
    /// entity still held is despawned (its owner normally retired them already), the map is
    /// emptied (keeping capacity), the counters reset, and `bound()` is derived and reserved.
    // PERFORMANCE RATIONALE:
    // Why: Reserving once per session keeps the arrival path free of vector growth.
    // What: The three vectors are reserved to the bound when a new session generation is bound.
    // Trade-off: The reservation is an upper bound, larger than most battles use.
    pub fn follow(
        &mut self,
        commands: &mut Commands<'_, '_>,
        generation: Option<SessionGeneration>,
        bound: impl FnOnce() -> RenderPoolBound,
    ) {
        if self.generation == generation && self.bound.is_some() {
            return;
        }
        self.despawn_held(commands);
        self.generation = generation;
        self.stats = RenderPoolStats::default();
        self.warned = false;
        let bound = bound();
        let total = bound.total();
        self.bound = Some(bound);
        self.live.reserve(total);
        self.arrivals.reserve(total);
        self.parked.reserve(total);
    }

    /// Despawns every live and parked entity and empties the map, keeping its capacity; the next
    /// [`Self::follow`] binds a session afresh. Called where a battle's bubbles are all retired:
    /// a tab change, a battle reset and the editor's playtest exit.
    pub fn retire_all(&mut self, commands: &mut Commands<'_, '_>) {
        self.despawn_held(commands);
        self.generation = None;
        self.bound = None;
    }

    /// Despawns only the parked entities, so the next arrivals are spawned fresh (the first half
    /// of [`Self::retire_all`]).
    pub fn release_parked(&mut self, commands: &mut Commands<'_, '_>) {
        for entity in self.parked.drain(..) {
            commands.entity(entity).despawn_recursive();
        }
        self.stats.parked_now = 0;
    }

    /// Despawns and forgets every entity held.
    fn despawn_held(&mut self, commands: &mut Commands<'_, '_>) {
        for (_, entity) in self.live.drain(..).chain(self.arrivals.drain(..)) {
            commands.entity(entity).despawn_recursive();
        }
        self.release_parked(commands);
    }

    /// Drops every entry whose unit is not in `snapshot` (sorted by `entity_id`, as the
    /// authoritative snapshot is), in one merge and in place. `on_departed(id, entity)` is told of
    /// each; when it returns `true` the entity is parked for a later arrival, otherwise the caller
    /// has disposed of it.
    pub fn depart(
        &mut self,
        snapshot: &[BubbleRenderInstance],
        mut on_departed: impl FnMut(EntityId, Entity) -> bool,
    ) {
        let mut records = snapshot.iter().map(|record| record.entity_id).peekable();
        let parked = &mut self.parked;
        self.live.retain(|&(id, entity)| {
            while records.peek().is_some_and(|record| *record < id) {
                records.next();
            }
            let present = records.peek() == Some(&id);
            if !present && on_departed(id, entity) {
                parked.push(entity);
            }
            present
        });
        self.stats.parked_now = u32::try_from(self.parked.len()).unwrap_or(u32::MAX);
    }

    /// The entity that draws `id`, searching `live` forward from `cursor`, which it advances.
    /// Records must be asked for in increasing id order, as the snapshot lists them, so one frame's
    /// lookups walk `live` once.
    pub fn find_from(&self, cursor: &mut usize, id: EntityId) -> Option<Entity> {
        while self.live.get(*cursor).is_some_and(|entry| entry.0 < id) {
            *cursor += 1;
        }
        self.live
            .get(*cursor)
            .filter(|entry| entry.0 == id)
            .map(|entry| entry.1)
    }

    /// Takes a parked entity for an arrival, if one is parked.
    pub fn take_parked(&mut self) -> Option<Entity> {
        let entity = self.parked.pop();
        self.stats.parked_now = u32::try_from(self.parked.len()).unwrap_or(u32::MAX);
        entity
    }

    /// Notes that unit `id` arrived and is drawn by `entity`, reused or spawned. Arrivals must be
    /// noted in increasing id order. A spawn that takes the pool beyond its bound is counted, and
    /// the first one of the session logs a warning naming the bound and its terms.
    pub fn arrive(&mut self, id: EntityId, entity: Entity, how: Arrival) {
        self.arrivals.push((id, entity));
        let held = self.live.len() + self.arrivals.len() + self.parked.len();
        match how {
            Arrival::Reused => self.stats.reused += 1,
            Arrival::Spawned => {
                self.stats.spawned += 1;
                if let Some(bound) = self.bound.filter(|bound| held > bound.total()) {
                    self.stats.beyond_bound += 1;
                    if !self.warned {
                        self.warned = true;
                        warn!(
                            "the render pool holds {held} bubbles, beyond its bound of {} \
                             (start {}, players {}, army {}, transient {} set by {:?}); every unit \
                             is still drawn and the pool grows",
                            bound.total(),
                            bound.start,
                            bound.players,
                            bound.army,
                            bound.transient,
                            bound.host
                        );
                    }
                }
            }
        }
        let held = u32::try_from(held).unwrap_or(u32::MAX);
        self.stats.high_water = self.stats.high_water.max(held);
    }

    /// Merges the frame's arrivals into `live`, keeping it sorted: appended when every arrival's
    /// id is above the last presented one (new ids are the largest), else merged from the back in
    /// place. Neither allocates within the reserved capacity.
    pub fn merge_arrivals(&mut self) {
        merge_sorted(&mut self.live, &mut self.arrivals);
    }
}

/// Moves every entry of `arrivals` (sorted by id) into `live` (sorted by id), keeping `live`
/// sorted, and leaves `arrivals` empty with its capacity. Ids must be distinct across both.
fn merge_sorted(live: &mut Vec<(EntityId, Entity)>, arrivals: &mut Vec<(EntityId, Entity)>) {
    let (Some(first), last) = (arrivals.first(), live.last()) else {
        return;
    };
    if last.is_none_or(|last| last.0 < first.0) {
        live.append(arrivals);
        return;
    }
    let old = live.len();
    live.resize(old + arrivals.len(), (EntityId(0), Entity::PLACEHOLDER));
    let (mut from_live, mut from_arrivals) = (old, arrivals.len());
    for slot in (0..live.len()).rev() {
        let take_live = from_arrivals == 0
            || (from_live > 0 && live[from_live - 1].0 > arrivals[from_arrivals - 1].0);
        live[slot] = if take_live {
            from_live -= 1;
            live[from_live]
        } else {
            from_arrivals -= 1;
            arrivals[from_arrivals]
        };
        if from_arrivals == 0 {
            break;
        }
    }
    arrivals.clear();
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;
    use bevy::ecs::world::CommandQueue;
    use sim_core::production::STANDARD_SUPPLY_CAP as CAP;
    use sim_types::weapon::TimedLifeSpec;

    // --- R-9: the bound ---------------------------------------------------------------------

    /// The shipped catalog with every hangar, strike magazine and impact spawn removed: no
    /// archetype is a host.
    fn without_hosts() -> UnitCatalog {
        let mut catalog = sim_fixtures::shipped_catalogs().units;
        let ids: Vec<UnitTypeId> = catalog.archetypes().map(|a| a.unit_type_id).collect();
        for id in ids {
            let archetype = catalog.get_mut(id).expect("invariant: listed id");
            archetype.capabilities.hangar = None;
            archetype.capabilities.strike_magazine = None;
            let own = [
                archetype.weapon.as_mut(),
                archetype.ground_weapon.as_mut(),
                archetype.air_weapon.as_mut(),
            ];
            for weapon in own.into_iter().flatten() {
                weapon.impact_spawn = None;
            }
            for mode in archetype
                .modes
                .iter_mut()
                .chain(&mut archetype.capabilities.modes)
            {
                let overrides = [
                    mode.ground_weapon_override
                        .as_mut()
                        .and_then(Option::as_mut),
                    mode.air_weapon_override.as_mut().and_then(Option::as_mut),
                ];
                for weapon in overrides.into_iter().flatten() {
                    weapon.impact_spawn = None;
                }
            }
        }
        catalog
    }

    /// The shipped archetype that `has` picks, by property.
    fn shipped(has: impl Fn(&UnitArchetype) -> bool) -> UnitArchetype {
        sim_fixtures::shipped_catalogs()
            .units
            .archetypes()
            .find(|a| has(a))
            .expect("a shipped archetype with the property")
            .clone()
    }

    /// The shipped Brood Lord analogue: the archetype with a strike magazine.
    fn magazine_host() -> UnitArchetype {
        shipped(|a| a.capabilities.strike_magazine.is_some())
    }

    /// The shipped Carrier analogue: the archetype with a hangar.
    fn hangar_host() -> UnitArchetype {
        shipped(|a| a.capabilities.hangar.is_some())
    }

    /// `catalog` with `archetype` registered over its own id.
    fn with(mut catalog: UnitCatalog, archetype: UnitArchetype) -> UnitCatalog {
        let id = archetype.unit_type_id;
        *catalog.get_mut(id).expect("invariant: a shipped id") = archetype;
        catalog
    }

    /// The spawning weapon of the magazine host.
    fn spawning(archetype: &mut UnitArchetype) -> &mut WeaponSpec {
        archetype
            .ground_weapon
            .as_mut()
            .filter(|weapon| weapon.impact_spawn.is_some())
            .expect("the magazine host's ground weapon spawns")
    }

    #[test]
    fn a_catalog_without_hosts_has_no_transient_term() {
        let bound = RenderPoolBound::for_session(&without_hosts(), 2, 30);
        assert_eq!((bound.transient, bound.host), (0, None));
        assert_eq!(bound.total(), 30 + 2 * bound.army as usize);
    }

    #[test]
    fn a_hangar_keeps_its_size_per_host_supply() {
        let host = hangar_host();
        let members = u32::from(host.capabilities.hangar.expect("a hangar").max_count);
        let bound = RenderPoolBound::for_session(&with(without_hosts(), host.clone()), 2, 0);
        assert_eq!(bound.transient, CAP * members / host.supply_cost.max(1));
        assert_eq!(bound.host, Some(host.unit_type_id));
    }

    #[test]
    fn a_magazine_spawn_keeps_its_charges_and_those_built_within_a_life() {
        let mut host = magazine_host();
        let magazine = host.capabilities.strike_magazine.expect("a magazine");
        let spawn = spawning(&mut host).impact_spawn.expect("a spawn");
        let life = spawn.timed_life.expect("a timed life");
        let lifetime = u32::from(life.ticks) + u32::from(life.random_extra_ticks);
        let units = u32::from(spawn.per_strike)
            * (u32::from(magazine.capacity) + lifetime.div_ceil(u32::from(magazine.build_ticks)));
        let bound = RenderPoolBound::for_session(&with(without_hosts(), host.clone()), 2, 0);
        assert_eq!(bound.transient, CAP * units / host.supply_cost.max(1));
        assert_eq!(bound.host, Some(host.unit_type_id));
    }

    #[test]
    fn a_magazine_free_spawn_keeps_one_release_per_period_of_a_life() {
        let mut host = magazine_host();
        host.capabilities.strike_magazine = None;
        let weapon = spawning(&mut host).clone();
        let spawn = weapon.impact_spawn.expect("a spawn");
        let life = spawn.timed_life.expect("a timed life");
        let lifetime = u32::from(life.ticks) + u32::from(life.random_extra_ticks);
        let units = u32::from(spawn.per_strike)
            * u32::from(weapon.attacks_count)
            * (lifetime / u32::from(weapon.period_ticks) + 1);
        let bound = RenderPoolBound::for_session(&with(without_hosts(), host.clone()), 2, 0);
        assert_eq!(bound.transient, CAP * units / host.supply_cost.max(1));
    }

    #[test]
    fn an_untimed_spawn_adds_nothing() {
        let mut host = magazine_host();
        let weapon = spawning(&mut host);
        let mut spawn = weapon.impact_spawn.expect("a spawn");
        spawn.timed_life = None;
        weapon.impact_spawn = Some(spawn);
        if let Some(weapon) = host.weapon.as_mut() {
            weapon.impact_spawn = Some(spawn);
        }
        let bound = RenderPoolBound::for_session(&with(without_hosts(), host), 2, 0);
        assert_eq!((bound.transient, bound.host), (0, None));
    }

    #[test]
    fn a_free_trained_unit_counts_as_costing_one() {
        let catalog = without_hosts();
        let trained = catalog
            .archetypes()
            .flat_map(|trainer| trainer.trains_units.iter())
            .find_map(|slug| catalog.get_by_slug(slug))
            .expect("a trained unit")
            .clone();
        let mut free = trained;
        free.supply_cost = 0;
        let bound = RenderPoolBound::for_session(&with(catalog, free), 2, 0);
        assert_eq!(bound.army, CAP);
    }

    #[test]
    fn the_army_term_is_the_cap_over_the_cheapest_trained_unit() {
        let catalog = sim_fixtures::shipped_catalogs().units;
        let cheapest = catalog
            .archetypes()
            .flat_map(|trainer| trainer.trains_units.iter())
            .filter_map(|slug| catalog.get_by_slug(slug))
            .map(|a| a.supply_cost.max(1))
            .min()
            .expect("something is trained");
        assert_eq!(
            RenderPoolBound::for_session(&catalog, 2, 0).army,
            CAP / cheapest
        );
    }

    /// A mix of hosts within the cap keeps more transient units than the largest army of one
    /// host type does (the single-type bound `HeldBeams` uses), and the derived bound still
    /// covers it.
    #[test]
    fn the_bound_covers_a_mix_of_hosts_beyond_the_single_type_bound() {
        let mut carrier = hangar_host();
        carrier.supply_cost = 6;
        let mut hangar = carrier.capabilities.hangar.expect("a hangar");
        hangar.max_count = 8;
        carrier.capabilities.hangar = Some(hangar);
        let mut small = magazine_host();
        small.capabilities.strike_magazine = None;
        small.capabilities.hangar = Some(HangarSpecFixture::one(hangar));
        small.supply_cost = 2;
        for weapon in [
            small.weapon.as_mut(),
            small.ground_weapon.as_mut(),
            small.air_weapon.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            weapon.impact_spawn = None;
        }
        let catalog = with(with(without_hosts(), carrier), small);
        let bound = RenderPoolBound::for_session(&catalog, 1, 0);
        // 33 carriers (198 supply) and one small host (2 supply): 33 × 8 + 1 = 265 transients.
        let mix = 33 * 8 + 1;
        let single_type = ((CAP / 6) * 8).max(CAP / 2);
        assert!(mix > single_type, "the mix exceeds the single-type bound");
        assert_eq!(bound.transient, CAP * 8 / 6);
        assert!(bound.transient >= mix, "the derived bound covers the mix");
    }

    /// A one-member hangar like `hangar`.
    struct HangarSpecFixture;

    impl HangarSpecFixture {
        fn one(mut hangar: sim_types::HangarSpec) -> sim_types::HangarSpec {
            hangar.max_count = 1;
            hangar
        }
    }

    #[test]
    fn the_shipped_bound_names_a_host_and_covers_each_hosts_supply() {
        let catalog = sim_fixtures::shipped_catalogs().units;
        let bound = RenderPoolBound::for_session(&catalog, 2, 40);
        let host = catalog
            .get(bound.host.expect("the shipped catalog has hosts"))
            .expect("the host is in the catalog");
        assert!(
            host.capabilities.hangar.is_some() || host.capabilities.strike_magazine.is_some(),
            "the host has a hangar or a strike magazine"
        );
        for hangar_host in catalog
            .archetypes()
            .filter(|a| a.capabilities.hangar.is_some())
        {
            let members = u32::from(hangar_host.capabilities.hangar.expect("a hangar").max_count);
            assert!(bound.transient >= CAP * members / hangar_host.supply_cost.max(1));
        }
        let per_player = bound.army as usize + bound.transient as usize;
        assert_eq!(bound.total(), 40 + 2 * per_player);
    }

    #[test]
    fn a_timed_life_with_random_extra_counts_its_longest_life() {
        let mut host = magazine_host();
        let weapon = spawning(&mut host);
        let mut spawn = weapon.impact_spawn.expect("a spawn");
        let build = u32::from(
            host.capabilities
                .strike_magazine
                .expect("a magazine")
                .build_ticks,
        );
        let life = |ticks: u32| TimedLifeSpec {
            ticks: u16::try_from(ticks).expect("a short life"),
            random_extra_ticks: u16::try_from(build).expect("a short build"),
        };
        spawn.timed_life = Some(life(build));
        spawning(&mut host).impact_spawn = Some(spawn);
        // `weapon` mirrors the ground weapon.
        if let Some(weapon) = host.weapon.as_mut() {
            weapon.impact_spawn = Some(spawn);
        }
        let capacity = u32::from(
            host.capabilities
                .strike_magazine
                .expect("a magazine")
                .capacity,
        );
        let bound = RenderPoolBound::for_session(&with(without_hosts(), host.clone()), 1, 0);
        // A life of `build` plus up to `build` more spans two builds.
        let units = u32::from(spawn.per_strike) * (capacity + 2);
        assert_eq!(bound.transient, CAP * units / host.supply_cost.max(1));
    }

    // --- R-10 and R-6: the map and its decisions ------------------------------------------------

    /// A world with `count` empty entities, and those entities.
    fn entities(count: usize) -> (World, Vec<Entity>) {
        let mut world = World::new();
        let spawned = (0..count).map(|_| world.spawn_empty().id()).collect();
        (world, spawned)
    }

    /// Runs `act` with commands on `world`, then applies them.
    fn with_commands(world: &mut World, act: impl FnOnce(&mut Commands<'_, '_>)) {
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, world);
        act(&mut commands);
        queue.apply(world);
    }

    /// A record of simulation unit `id`; only its id matters to the map.
    fn record(id: u32) -> BubbleRenderInstance {
        use sim_types::enums::{AnimState, ShapeKind, SilhouetteClass, WeaponVisualKind};
        BubbleRenderInstance {
            entity_id: EntityId(id),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::LightProngs,
            unit_type: UnitTypeId(1),
            owner: sim_types::PlayerId(0),
            world_pos: [0.0, 0.0, 0.5],
            ground_z: 0.0,
            radius: 0.5,
            velocity: [0.0, 0.0],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: AnimState::Idle,
            attack_swing: None,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            concealment: sim_types::Concealment::None,
            speed_boosted: false,
            stance: sim_types::StanceView::default(),
            status: sim_types::StatusFlags::default(),
            construction_hp_ratio: None,
            barrier_ratio: 0.0,
            housed: false,
        }
    }

    /// A bound of `total` bubbles, all of them start.
    fn bound_of(total: usize) -> RenderPoolBound {
        RenderPoolBound::without_catalog(1, total)
    }

    /// A map bound to the first session with room for `total`, presenting `ids` on `entities`.
    fn map_with(world: &mut World, total: usize, ids: &[u32], drawn: &[Entity]) -> RenderEntityMap {
        let mut map = RenderEntityMap::default();
        with_commands(world, |commands| {
            map.follow(commands, Some(SessionGeneration::FIRST), || bound_of(total));
        });
        for (id, entity) in ids.iter().zip(drawn) {
            map.arrive(EntityId(*id), *entity, Arrival::Spawned);
        }
        map.merge_arrivals();
        map
    }

    fn ids(map: &RenderEntityMap) -> Vec<u32> {
        map.iter().map(|(id, _)| id.0).collect()
    }

    #[test]
    fn departures_in_the_middle_are_parked_in_place() {
        let (mut world, drawn) = entities(6);
        let mut map = map_with(&mut world, 16, &[1, 2, 3, 4, 5, 6], &drawn);
        let snapshot: Vec<_> = [1, 2, 4, 6].map(record).into();
        let mut departed = Vec::new();
        map.depart(&snapshot, |id, entity| {
            departed.push((id.0, entity));
            true
        });
        assert_eq!(ids(&map), [1, 2, 4, 6]);
        assert_eq!(departed, [(3, drawn[2]), (5, drawn[4])]);
        assert_eq!(map.parked(), [drawn[2], drawn[4]]);
        assert_eq!(map.stats().parked_now, 2);
        assert_eq!(map.get(EntityId(4)), Some(drawn[3]));
        assert_eq!(map.get(EntityId(3)), None);
    }

    #[test]
    fn a_departure_the_caller_disposes_of_is_not_parked() {
        let (mut world, drawn) = entities(2);
        let mut map = map_with(&mut world, 16, &[1, 2], &drawn);
        map.depart(&[record(2)], |_, _| false);
        assert_eq!(ids(&map), [2]);
        assert_eq!(map.parked().len(), 0);
    }

    #[test]
    fn arrivals_above_every_presented_id_are_appended() {
        let (mut world, drawn) = entities(4);
        let mut map = map_with(&mut world, 16, &[1, 2], &drawn[..2]);
        map.arrive(EntityId(7), drawn[2], Arrival::Spawned);
        map.arrive(EntityId(9), drawn[3], Arrival::Spawned);
        map.merge_arrivals();
        assert_eq!(ids(&map), [1, 2, 7, 9]);
        assert_eq!(map.get(EntityId(9)), Some(drawn[3]));
    }

    #[test]
    fn arrivals_among_presented_ids_are_merged_from_the_back() {
        let (mut world, drawn) = entities(5);
        let mut map = map_with(&mut world, 16, &[1, 5, 9], &drawn[..3]);
        map.arrive(EntityId(3), drawn[3], Arrival::Spawned);
        map.arrive(EntityId(7), drawn[4], Arrival::Spawned);
        map.merge_arrivals();
        assert_eq!(ids(&map), [1, 3, 5, 7, 9]);
        let entities: Vec<Entity> = map.iter().map(|(_, entity)| entity).collect();
        assert_eq!(entities, [drawn[0], drawn[3], drawn[1], drawn[4], drawn[2]]);
        let first = map_with(&mut world, 16, &[4, 6], &drawn[..2]);
        let mut first = first;
        first.arrive(EntityId(1), drawn[2], Arrival::Spawned);
        first.arrive(EntityId(2), drawn[3], Arrival::Spawned);
        first.merge_arrivals();
        assert_eq!(ids(&first), [1, 2, 4, 6]);
    }

    #[test]
    fn a_frame_of_departures_and_arrivals_reuses_the_parked_entities() {
        let (mut world, drawn) = entities(4);
        let mut map = map_with(&mut world, 16, &[1, 2, 3, 4], &drawn);
        let snapshot: Vec<_> = [1, 3, 5, 6].map(record).into();
        map.depart(&snapshot, |_, _| true);
        let mut cursor = 0;
        let mut arrived = Vec::new();
        for record in &snapshot {
            if let Some(entity) = map.find_from(&mut cursor, record.entity_id) {
                arrived.push((record.entity_id.0, entity, None));
            } else {
                let entity = map.take_parked().expect("a parked entity");
                map.arrive(record.entity_id, entity, Arrival::Reused);
                arrived.push((record.entity_id.0, entity, Some(Arrival::Reused)));
            }
        }
        map.merge_arrivals();
        assert_eq!(ids(&map), [1, 3, 5, 6]);
        assert_eq!(map.stats().reused, 2);
        assert_eq!(map.parked().len(), 0);
        // The stack gives the last parked first.
        assert_eq!(map.get(EntityId(5)), Some(drawn[3]));
        assert_eq!(map.get(EntityId(6)), Some(drawn[1]));
        assert_eq!(arrived.len(), 4);
    }

    #[test]
    fn an_empty_snapshot_parks_everything() {
        let (mut world, drawn) = entities(3);
        let mut map = map_with(&mut world, 16, &[1, 2, 3], &drawn);
        map.depart(&[], |_, _| true);
        assert_eq!(map.len(), 0);
        assert_eq!(map.parked().len(), 3);
        assert_eq!(map.stats().parked_now, 3);
    }

    #[test]
    fn membership_changes_within_the_bound_keep_every_buffer() {
        let (mut world, drawn) = entities(8);
        let mut map = map_with(&mut world, 8, &[1, 2, 3, 4], &drawn[..4]);
        let buffers: Vec<_> = map.retained_buffers().collect();
        let mut spare = drawn[4..].to_vec();
        for frame in 0..50_u32 {
            // The oldest leaves and a new one arrives with the largest id, but every fifth frame
            // it arrives below the largest (an out-of-order arrival, merged from the back). New
            // ids are even, the out-of-order ones odd, so no id is ever reused.
            let next = if frame % 5 == 4 {
                2 * (5 + frame) - 3
            } else {
                2 * (5 + frame)
            };
            let mut snapshot: Vec<u32> = ids(&map)[1..].to_vec();
            snapshot.push(next);
            snapshot.sort_unstable();
            let records: Vec<_> = snapshot.iter().copied().map(record).collect();
            map.depart(&records, |_, _| true);
            let mut cursor = 0;
            for record in &records {
                if map.find_from(&mut cursor, record.entity_id).is_none() {
                    let entity = map
                        .take_parked()
                        .or_else(|| spare.pop())
                        .expect("an entity");
                    map.arrive(record.entity_id, entity, Arrival::Reused);
                }
            }
            map.merge_arrivals();
            assert!(ids(&map).is_sorted(), "frame {frame}: sorted");
        }
        assert_eq!(map.retained_buffers().collect::<Vec<_>>(), buffers);
        assert_eq!(map.len(), 4);
    }

    #[test]
    fn a_spawn_beyond_the_bound_is_counted_and_warned_of_once() {
        let (mut world, drawn) = entities(6);
        let mut map = map_with(&mut world, 3, &[1, 2, 3], &drawn[..3]);
        assert_eq!(map.stats().beyond_bound, 0);
        assert!(!map.has_warned());
        for (id, entity) in [(4, drawn[3]), (5, drawn[4]), (6, drawn[5])] {
            map.arrive(EntityId(id), entity, Arrival::Spawned);
        }
        map.merge_arrivals();
        assert_eq!(map.len(), 6, "every unit is drawn");
        assert_eq!(map.stats().beyond_bound, 3);
        assert!(map.has_warned());
        assert_eq!(map.stats().high_water, 6);
        assert_eq!(map.stats().spawned, 6);
    }

    #[test]
    fn a_reuse_is_never_beyond_the_bound() {
        let (mut world, drawn) = entities(3);
        let mut map = map_with(&mut world, 3, &[1, 2, 3], &drawn);
        map.depart(&[record(2), record(3)], |_, _| true);
        let parked = map.take_parked().expect("one parked");
        map.arrive(EntityId(4), parked, Arrival::Reused);
        map.merge_arrivals();
        assert_eq!(map.stats().beyond_bound, 0);
        assert_eq!(map.stats().reused, 1);
        assert!(
            map.take_parked().is_none(),
            "exhausted: the next arrival spawns"
        );
    }

    #[test]
    fn a_new_session_empties_the_map_and_binds_its_own_bound() {
        let (mut world, drawn) = entities(3);
        let mut map = map_with(&mut world, 8, &[1, 2, 3], &drawn);
        map.depart(&[record(1)], |_, _| true);
        let next = SessionGeneration::FIRST
            .next()
            .expect("invariant: the second generation exists");
        with_commands(&mut world, |commands| {
            map.follow(commands, Some(next), || bound_of(5));
        });
        assert_eq!((map.len(), map.parked().len()), (0, 0));
        assert_eq!(map.generation(), Some(next));
        assert_eq!(map.bound().map(|bound| bound.total()), Some(5));
        assert_eq!(map.stats(), RenderPoolStats::default());
        assert!(drawn
            .iter()
            .all(|entity| world.get_entity(*entity).is_err()));
    }

    #[test]
    fn retiring_despawns_live_and_parked_entities_and_unbinds() {
        let (mut world, drawn) = entities(3);
        let mut map = map_with(&mut world, 8, &[1, 2, 3], &drawn);
        map.depart(&[record(1), record(2)], |_, _| true);
        with_commands(&mut world, |commands| map.retire_all(commands));
        assert_eq!((map.len(), map.parked().len()), (0, 0));
        assert_eq!((map.generation(), map.bound()), (None, None));
        assert!(drawn
            .iter()
            .all(|entity| world.get_entity(*entity).is_err()));
    }

    #[test]
    fn releasing_parked_entities_keeps_the_live_ones() {
        let (mut world, drawn) = entities(3);
        let mut map = map_with(&mut world, 8, &[1, 2, 3], &drawn);
        map.depart(&[record(1)], |_, _| true);
        with_commands(&mut world, |commands| map.release_parked(commands));
        assert_eq!(ids(&map), [1]);
        assert_eq!(map.parked().len(), 0);
        assert!(world.get_entity(drawn[0]).is_ok());
        assert!(world.get_entity(drawn[1]).is_err());
    }
}
