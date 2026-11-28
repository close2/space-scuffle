//! Canonical dynamic-state digests for lockstep verification.
//!
//! Covers persistent entities, projectiles, economy, upgrades, effects, live RNG,
//! terrain, fog memory and exposed navigation clearance. Catalogs must match
//! separately; this is not a content compatibility handshake. A noncryptographic
//! 64-bit digest can collide. Integers use fixed-width little-endian encodings;
//! collections are framed and enums have explicit tags.
//!
//! Scratch search buffers and spatial buckets are excluded: the normal tick
//! rebuilds the spatial index in target acquisition before querying it, and
//! search/vision generation markers isolate stale workspace contents. The
//! pathfinder must have been initialized for the world's map dimensions.
//! Movement layer obstacle/recipient scratch is rebuilt or cleared before use and
//! does not add authoritative policy state. Production occupancy is rebuilt before completion queries. Command outcomes and
//! blocked-exit observations are derived outputs and never consulted for decisions.
//! Renderer events, tick diagnostics and the previous digest are derived outputs,
//! not authoritative inputs. The invalid source state behind a diagnostic is hashed.

mod encoding;

use self::encoding::{Canonical, Encoder};
use crate::entity::{ActiveProjectile, Entity};
use crate::world::SimWorld;
use sim_types::EntityId;
use std::collections::BTreeMap;

/// Canonical dynamic-state encoding version; changing it invalidates older digests.
/// Schema 4 replaces the loaded boolean with an optional exact carrier identity.
/// Schema 5 records the terrain flags each placed structure must restore on removal.
/// Schema 6 records the locomotion layer each projectile's splash lands on.
/// Schema 7 marks whether an attack order is the player's or automatic.
/// Schema 8 adds each unit's movement memory: arrival, stall and repath state.
/// Schema 9 adds task progress, cargo, foundation records, job payments and research jobs,
/// typed resource-node harvest targets, the new orders, and the match standing.
/// Schema 10 removes each entity's animation progress, a constant zero no system wrote: the
/// attack phase now reaches the renderer from the weapon state machine, not stored state.
/// Schema 11 adds where each unit's automatic target last stood, which the tie-break "closest
/// to the previous target" reads.
/// Schema 12 adds each weapon's strike interval and follow-up distance, each entity's pending
/// strikes and each projectile's strike count: the strikes of one attack land on separate ticks.
/// Schema 13 replaces each weapon's Armored and Light bonuses with its bonus against every
/// attribute, eight values in `Attribute::ALL` order.
/// Schema 14 replaces each splash's three fixed radii with its list of zones, each a radius and
/// the fraction of the strike it deals.
/// Schema 15 adds each splash's shape: a circle, or a line with its origin, axis and offsets.
/// Schema 16 adds each weapon's structure strike: the damage, per-level growth and armour rule
/// it deals to a structure instead of its own strike (B-82).
/// Schema 17 adds each weapon's acquisition arc and each unit's engaged target (B-29).
/// Schema 18 adds each weapon's turret idle behaviour and rate (B-30).
/// Schema 19 adds each weapon's random delay range and each unit's carried delay (B-3).
/// Schema 20 adds each unit's barrier window, absorption left and cooldown (B-41).
/// Schema 21 adds each unit's attack dash cooldown (B-43).
/// Schema 22 adds the sense in which each unit rounds a body parked on its next corner (B-100).
/// Schema 23 adds whether each weapon's burst, and each pending burst, is channeled (Q0058).
/// Schema 24 adds whether each splash is centred on its target's edge (Q0063).
/// Schema 25 adds each unit's surround-walk state: whether it walks, has turned back or ended,
/// its closed ticks, the distance walked and the target it approaches (B-104, B-105).
/// Schema 26 adds each line's search period and each unit's pending line searches: their
/// points, countdown, plane, hit angle, primary target and the units already struck (B-85).
/// Schema 27 adds each unit's count of route orders issued and the world's route groups: the
/// next group id, then per group its id, player, kind, target, members with their ordinals, and
/// lane state with the lane's polyline, word, leader and radius (plan `2026-11-10-group-routing`).
/// Schema 28 adds the effect-tree engine's state (plan `2026-11-10-spellcaster-mechanics` D-13):
/// each weapon's melee flag, each unit's cast lock and displacement, and the world's effect
/// state: the next persistent and missile ids, then the statuses, persistents and missiles, each
/// with its node or status id, timing and effect context.
/// Schema 29 removes the hazard zones the effect engine replaced (plan
/// `2026-11-10-spellcaster-mechanics` D-12, D-13 item 2): the next hazard id, the hazard list
/// and each unit's last storm damage tick.
/// Schema 30 adds each placed structure's footprint shape: half-width, half-height and chamfer
/// in quarter cells (plan `2026-11-18-contour-footprints`).
/// Schema 31 adds each unit's surround-walk retreat, `MovementMemory::flank_retreat`, after the
/// distance walked (plan `2026-11-19-crowd-edge-steering-2` D-1).
/// Schema 32 adds each resource node's placed footprint and each cell's resource occupant (plan
/// `2026-11-20-resource-footprints`), and a leading variant tag on every footprint shape (0 for
/// a box, then its three values; 1 for an outline, then its vertices).
/// Schema 33 adds each unit's return-delay countdown, `UnitTask::return_ticks`, after
/// `gather_ticks` (plan `2026-11-22-harvest-timing` D-5).
/// Schema 34 adds each queued job's `ProductionJob::in_production`, after `reserved_supply`, and
/// each entity's rally, `Entity::rally`, after `captured_supply_provided` (an `Option` of a tag,
/// 0 point, 1 unit, 2 node, and its payload; plan `2026-11-23-production-queue` D-6).
/// Schema 35 adds each unit's side-rule commitment (its partner, then its side) and its
/// remembered gap on the ring round its chase target (the target, then the point) to its movement
/// memory, after the leash origin (plan `2026-11-20-contact-solver` §3.7, Amendments 7 to 10).
/// Only the solver pipeline writes them.
/// Schema 36 adds each splash's impact (0 target, 1 attacker) and its target-radius condition (an
/// `Option` of a scalar), after `offset_by_unit_radius` (plan `2026-11-23-splash-centres` D-6).
/// Schema 37 adds each splash's cliff rule (0 same level, 1 any level) and its other-level target
/// strike (a `bool`), after `target_radius_below` (plan `2026-11-23-splash-cliff-levels` D-5).
/// Schema 38 adds each weapon's first-strike delay, `WeaponSpec::first_strike_delay_ticks`, after
/// `attacks_count` (plan `2026-11-23-strike-delays` D-6).
/// Schema 39 adds each resource node's release tick, `ResourceLines::release_tick`, after the
/// map's resource nodes (plan `2026-11-23-harvest-handoff` D-3).
/// Schema 40 adds each weapon's movement rule (0 stopped, 1 slowing, 2 moving) and whether it fires
/// outside an attack order (a `bool`), `WeaponSpec::allowed_movement` and
/// `WeaponSpec::fires_outside_attack_order`, after `turret_idle` (plan `2026-11-23-turret-wind-up`
/// D-9).
/// Schema 41 adds each entity's attack-reveal counters per player, `Entity::attack_reveal_ticks`,
/// after `displacement`, and each weapon's attack visibility (0 snapshot, 1 visible with its
/// reveal ticks, 2 hidden), `WeaponSpec::attack_visibility`, after `melee` (plan
/// `2026-11-23-vision-and-detection` D-7).
/// Schema 43 adds each entity's teleport count, `Entity::teleport_count`, after
/// `attack_reveal_ticks`, each projectile's target teleport count,
/// `ActiveProjectile::target_teleport_count`, after `strikes`, and each weapon's missile
/// validation (weapon, teleport; a `bool` each), `WeaponSpec::missile_validation`, after
/// `attack_visibility` (plan `2026-11-24-weapon-validation` D-7). Number 42 is reserved for the
/// contact solver's branch; whichever merges second takes the next free number.
/// Schema 44 adds each unit task's housing structure, `UnitTask::housed_in`, after `last_node`
/// (plan `2026-11-24-hidden-harvesters` D-3). Number 42 is reserved for the contact solver's
/// branch.
/// Schema 45 adds each weapon's fire rules (a `bool` each), `WeaponSpec::fires_at_other_targets`
/// and `WeaponSpec::locks_turret_while_firing`, after `fires_outside_attack_order`, and each
/// entity's own weapon target, `Entity::weapon_target`, after `current_target` (plan
/// `2026-11-24-fire-rules` D-8). Number 44 is the hidden-harvesters
/// plan's, merged first.
/// Schema 46 changes no encoding: it marks the contact solver becoming the default movement
/// pipeline (plan `2026-11-20-contact-solver` WP6), so a replay recorded under the legacy
/// pipeline is refused by its schema instead of diverging at its first contact. Number 42, once
/// reserved for this plan, stays unused (its retarget field, G-6′, was stopped, Amendment 18).
/// Schema 47 adds each unit's last seen point of its attack order's target,
/// `MovementMemory::last_seen` (target, point and whether it is lost), after `slot` (plan
/// `2026-11-24-weapon-validation` Amendment 2, B-195). The number is the next free at merge.
/// Schema 48 replaces each entity's rally by its routes, `Entity::rallies`, in the same place: the
/// route count, then per route its slot (a `u8`), its waypoint count and each waypoint in the
/// rally's encoding (plan `2026-11-25-rallies` D-2). The number is the next free at merge.
/// Schema 49 adds whether each weapon's turret tracks any enemy (a `bool`),
/// `WeaponSpec::turret_tracks_any_enemy`, after `locks_turret_while_firing`, and the enemy each
/// unit's turret tracks, the world's `tracked_targets` table (its length, then each unit and
/// enemy id in unit id order), after the route groups (plan `2026-11-26-turret-tracking-weapon`
/// D-8, Amendment 1). The number is the next free at merge.
/// Schema 50 adds the `Follow` order (tag 19, then its target and kind, 0 `Move` and 1
/// `AcquireMove`) and each unit's follow rest, `MovementMemory::follow_rest`, after `last_seen`
/// (plan `2026-11-26-follow-order` D-1, D-4). The number is the next free at merge.
/// Schema 53 adds each weapon's launch visibility (0 snapshot, 1 visible with its reveal ticks, 2
/// hidden), `WeaponSpec::launch_visibility`, after `missile_validation` (plan
/// `2026-11-27-launch-and-spell-visibility` D-5). The number is the next free at merge: 51 and 52
/// are reserved for the Gated movement plan.
/// Schema 56 adds each weapon's missile movers, `WeaponSpec::missile_movers` (their count, then
/// per mover its range bound and its phases, each phase's start speed, acceleration, top speed
/// and end), after `launch_visibility`, and each projectile's mover index, phase and path flown,
/// `ActiveProjectile::{mover, phase, travelled}`, after `target_teleport_count` (plan
/// `2026-11-27-weapon-delivery` D-6). The number is the next free at merge: 54 and 55 are
/// reserved for the Gated movement plan.
/// Schema 57 adds each entity's hangar, `Entity::hangar` (an option tag, then its docked records'
/// count and each record's life, shields and regeneration clock, its launched and queued counts,
/// its build progress, its target and its launch channel's tries and countdown), and each
/// launched member's link home, `Entity::hangar_link` (an option tag, then the hangar unit's id
/// and the returning flag), after `teleport_count`; and each weapon's magazine launch,
/// `WeaponSpec::magazine_launch` (an option tag, then its tries and interval), after
/// `missile_movers` (plan `2026-11-28-carrier-interceptors` D-3, D-4, §5). The number is the next
/// free at merge: 54 and 55 are reserved for the Gated movement plan.
/// Schema 60 encodes each missile phase's end as `MissilePhase::ends`, an option tag, then a
/// variant tag (`Flown` 0, `Remaining` 1) and its value, in place of `ends_after` (plan
/// `2026-11-28-missile-movers-b52` D-2, D-5). The number is the next free at merge: 58 and 59
/// are reserved for the Gated movement plan.
/// Schema 61 adds each entity's strike magazine, `Entity::strike_magazine` (an option tag, then
/// its armed count, arming and build countdowns), and timed life, `Entity::timed_life` (an option
/// tag, then its ticks), after `hangar_link`; and each weapon's impact spawn,
/// `WeaponSpec::impact_spawn` (an option tag, then its unit, count per strike, search range,
/// timed life and order), after `magazine_launch` (plan `2026-11-28-broodlings` D-3 to D-5). The
/// number is the next free at merge.
/// Schema 62 adds the world's pending death spawns, `SimWorld::pending_death_spawns` (a section
/// `death-spawns`: its length, then each entry's countdown, host type, owner, point and heading),
/// after the turret tracking (plan `2026-12-01-death-spawns-and-hangar-commands` D-2). The number
/// is the next free at merge.
/// Schema 63 adds each hangar's auto-build switch, `HangarState::auto_build` (a `bool`), after
/// `channel`, and the `UnitOrder` tags 20 to 22 (`HangarBuild`, `HangarCancelLast`,
/// `SetHangarAutoBuild` and its `bool`) (plan `2026-12-01-death-spawns-and-hangar-commands` D-6,
/// D-7). The number is the next free at merge.
/// Schema 66 adds the `UnitOrder` tag 23, `LoadUnit`, and its passenger's id (plan
/// `2026-12-03-movement-work-on-solver` WP-T1, B-247). The number is provisional, the next free
/// at merge: 64 and 65 are held by the missiles and diagonal-walkability branches.
pub const CHECKSUM_SCHEMA_VERSION: u32 = 66;

// PERFORMANCE RATIONALE:
// Why: Serializing a complete world into a temporary byte vector allocates per checksum.
// What: Stream canonical fields directly into xxHash64 without cloning world state.
// Trade-off: Full coverage scans map cells for each player as well as dynamic objects;
// this costs more than hashing only kinematics, but detects hidden divergence.

/// Computes a partial entity/projectile digest using ticks and an initial RNG seed.
///
/// Preserves the historical call signature, but the canonical schema changes digest
/// values. This cannot observe live RNG position or other world state. Use
/// [`SimWorld::compute_checksum`] for lockstep world verification. Its domain is
/// deliberately distinct from the world digest.
#[must_use]
pub fn compute_world_checksum(
    tick: u64,
    rng_seed: u64,
    entities: &BTreeMap<EntityId, Entity>,
    projectiles: &[ActiveProjectile],
) -> u64 {
    let mut encoder = Encoder::new("space-scuffle/entity-projectile");
    tick.encode(&mut encoder);
    rng_seed.encode(&mut encoder);
    entities.encode(&mut encoder);
    projectiles.encode(&mut encoder);
    encoder.finish()
}

/// Computes a canonical digest of persistent dynamic state without mutating `world`.
///
/// Requires identical unit/upgrade catalogs and a pathfinder initialized for the
/// world's map. Catalog definitions, transient visual events, previous checksum,
/// allocation capacities and algorithm workspaces are excluded. Persistent gameplay
/// state and retained entity presentation state are included. This schema has no
/// compatibility with the historical unversioned checksum.
#[must_use]
pub fn compute_simulation_checksum(world: &SimWorld) -> u64 {
    let mut encoder = Encoder::new("space-scuffle/dynamic-world");
    "clock-and-rng".encode(&mut encoder);
    world.current_tick.encode(&mut encoder);
    world.rng_seed.encode(&mut encoder);
    world.rng.get_seed().as_slice().encode(&mut encoder);
    world.rng.get_stream().encode(&mut encoder);
    world.rng.get_word_pos().encode(&mut encoder);
    world.next_entity_id.encode(&mut encoder);
    world.next_projectile_id.encode(&mut encoder);
    world.vision_aware_pathfinding.encode(&mut encoder);

    "entities".encode(&mut encoder);
    world.entities.encode(&mut encoder);
    "projectiles".encode(&mut encoder);
    world.projectiles.encode(&mut encoder);
    "players".encode(&mut encoder);
    world.player_resources.as_slice().encode(&mut encoder);
    world.player_upgrades.as_slice().encode(&mut encoder);
    world.match_state.encode(&mut encoder);
    "effects".encode(&mut encoder);
    world.effects.encode(&mut encoder);
    "route-groups".encode(&mut encoder);
    world.next_route_group.encode(&mut encoder);
    world.route_groups.encode(&mut encoder);
    "turret-tracking".encode(&mut encoder);
    encoder.length(world.tracked_targets.entries().len());
    for (unit, enemy) in world.tracked_targets.entries() {
        unit.encode(&mut encoder);
        enemy.encode(&mut encoder);
    }
    "death-spawns".encode(&mut encoder);
    world.pending_death_spawns.encode(&mut encoder);

    "map".encode(&mut encoder);
    world.map.width().encode(&mut encoder);
    world.map.height().encode(&mut encoder);
    world.map.cells().encode(&mut encoder);
    world.map.structures().encode(&mut encoder);
    world.map.resources().encode(&mut encoder);
    "resource-lines".encode(&mut encoder);
    world.resource_lines.release_ticks().encode(&mut encoder);
    world.map.watchtowers().encode(&mut encoder);

    "vision".encode(&mut encoder);
    encoder.length(world.vision_maps.as_slice().len());
    for vision in &world.vision_maps {
        vision.width().encode(&mut encoder);
        vision.height().encode(&mut encoder);
        vision.states().encode(&mut encoder);
        vision.visible_counter().encode(&mut encoder);
        vision.detector_counter().encode(&mut encoder);
    }
    world.known_blocked_cells.as_slice().encode(&mut encoder);

    "navigation".encode(&mut encoder);
    world.pathfinder.nav_version().encode(&mut encoder);
    world.map.width().encode(&mut encoder);
    world.map.height().encode(&mut encoder);
    for y in 0..world.map.height() {
        for x in 0..world.map.width() {
            world.pathfinder.clearance_at(x, y).encode(&mut encoder);
            world
                .pathfinder
                .static_clearance_at(x, y)
                .encode(&mut encoder);
        }
    }
    encoder.finish()
}
