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
pub const CHECKSUM_SCHEMA_VERSION: u32 = 29;

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

    "map".encode(&mut encoder);
    world.map.width().encode(&mut encoder);
    world.map.height().encode(&mut encoder);
    world.map.cells().encode(&mut encoder);
    world.map.structures().encode(&mut encoder);
    world.map.resources().encode(&mut encoder);
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
