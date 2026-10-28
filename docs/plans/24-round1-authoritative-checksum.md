# Round 1: Canonical dynamic simulation checksums

## Goal and acceptance contract

Make `SimWorld::compute_checksum()` detect changes to the complete persistent dynamic gameplay state available through the existing public APIs. Equal worlds with equal catalogs and commands must produce equal checksums without consuming RNG output or allocating temporary serialized state. A changed order, weapon timer payload, projectile damage envelope, player economy, hazard, map occupancy, fog memory, or RNG position must be observable immediately, before it causes a visible divergence.

This is a dynamic-state checksum, **not** a catalog compatibility handshake or a cryptographic guarantee. Identical unit and upgrade catalogs remain a prerequisite; those catalogs are deliberately excluded from this round. A 64-bit noncryptographic digest can collide. Delete the existing claim of “100% desync detection” and replace unmeasured performance claims with an explanation of streaming storage and its trade-off.

## Evidence and ownership

`checksum.rs` currently hashes only tick, the original RNG seed, a subset of entity kinematics/vitals, weapon phase tags without their payloads, and projectile ID/owner/current position. `SimWorld` holds persistent resources, upgrades, hazards, identifiers, terrain, vision memory, and live RNG state that never enter the digest. The current checksum therefore treats many immediately behaviorally distinct worlds as identical. The existing two-instance replay test verifies repeatability but does not establish sensitivity to these omissions.

Implementation owns `crates/sim_core/src/checksum.rs`, the checksum import/call and documentation in `world.rs`, a dedicated checksum integration-test file, and optionally private checksum unit-test submodules. No changes to `sim_map`, `sim_types`, the map editor, or pre-existing unrelated dirty work. Use public map/vision/pathfinder inspection methods. Coordinate with the orchestrator before expanding scope. No gameplay mechanics change is needed.

## API and compatibility

1. Keep the existing public four-argument `compute_world_checksum(tick, rng_seed, entities, projectiles)` callable with its existing signature. Document that its inputs permit only an entity/projectile digest and that callers needing the world checksum must use `SimWorld::compute_checksum()`. It should use the same complete entity/projectile encoders as the new implementation. Do not pretend historical numerical checksum values are preserved.
2. Add a documented `compute_simulation_checksum(world: &SimWorld) -> u64` in the checksum module and route `SimWorld::compute_checksum()` through it. Reading `SimWorld` from a sibling module is permitted; only public state is needed.
3. Declare/document a checksum schema version and include it and a literal domain marker in the byte stream. Give the legacy partial function a distinct domain marker. This round intentionally changes digests: old replay/network checksum values are incompatible. It does not add replay migration or protocol negotiation.
4. Keep `last_checksum` semantics: it is refreshed at the existing end-of-tick boundary. Never include `last_checksum` itself, which would introduce recursive dependence.

## Canonical byte encoding

Use a private streaming encoder around `Xxh64` initialized with the existing fixed seed. Provide small typed methods for integers, booleans, fixed-point scalars/vectors/angles, strings, sequence lengths, and option tags. Integer byte order is explicitly little-endian; fixed point uses raw signed bits; angles use raw binary-angle bits. Encode lengths as `u64`, never architecture-sized native bytes. Strings are their UTF-8 byte length followed by exact bytes; do not normalize data identifiers.

Each section begins with a fixed tag and each collection includes its length. `Option` always writes a `0`/`1` tag before the payload. Every enum uses an explicit exhaustive match assigning documented stable tags and encoding every payload. No `Debug`, RON, memory-layout casts, platform hash traits, wildcard enum matches, or float conversion. For structs owned in sim_core, prefer exhaustive destructuring without `..` so a newly added field prompts a compiler review of the encoder.

Iterate `BTreeMap`/`BTreeSet` in their existing key order and include keys. Preserve vector/deque order: order queues, projectile application order, buffs (fixed-point multiplication), and hazards can affect future behavior. Hash both the entity-map key and `Entity.id`, detecting inconsistent public state too. Do not sort or clone collections merely for hashing. Strings and sequences require no allocations.

## Required field inventory

### World header and identifiers

Include `current_tick`, `rng_seed`, `next_entity_id`, `next_projectile_id`, `next_hazard_id`, and `vision_aware_pathfinding`. Encode the live ChaCha8 state using `get_seed()` (32 bytes), `get_stream()` (`u64`), and `get_word_pos()` (`u128`). These methods exist in the pinned local rand_chacha 0.3.1 source. They represent the actual key/stream/position even when public code reseeds or advances `rng` independently of `rng_seed`. Never call `next_u64` on the live generator; no probabilistic sample substitute.

### Entities

Encode every declared `Entity` field in declaration order:

- Identity: `id`, `unit_type_id`, `owner`.
- Motion: `position`, `pos_at_tick_start`, `velocity`, `heading`, `turret_angle`.
- Vitals: `hp`, `shields`, `energy`, `ticks_since_last_damage`.
- Orders/path: `order_queue`, `current_funnel_waypoints`.
- Weapon cycle: `weapon_phase` including target IDs and remaining timers, `weapon_cooldown_remaining`.
- Buffs: all `ActiveBuff` fields (`buff_id`, duration, speed and attack multipliers), preserving order.
- Production: every `(UnitTypeId, remaining_build_ticks)` entry, preserving order.
- Movement flags: `is_mineral_walking`, `is_holding_position`.
- Retained entity presentation state: `current_anim`, `anim_progress`, `last_hit_angle`, `shield_hit_flash_ticks`, `hull_hit_flash_ticks`. Include these for full entity-state coverage even though these values are not all gameplay inputs.
- Target/modes: `current_target`, `is_immobile`, `unpushable`, `is_sieged`, `siege_weapon`, `active_mode`.
- Abilities: `channeling_remaining_ticks`, `pending_ability` and all nested options, four `ability_cooldowns`, `last_storm_damage_tick`.
- Remaining flags/vitals: `is_cloaked`, `max_hp`, `is_completed`, `is_loaded_in_transport`, `is_invulnerable`.

Encode all 12 `UnitOrder` variants and all payloads (move/attack destinations, target handles, production archetypes, building cell coordinates, and ability index plus optional position/entity). Encode every `WeaponPhase` variant and timer/target payload.

### Projectiles and embedded weapons

Encode every `ActiveProjectile` field: ID, owner, optional attacker, source/current position, optional target ID, target position, speed, entire weapon spec, optional source slug, and remaining lifetime.

The weapon encoder is shared by projectiles and `Entity.siege_weapon`. Encode **all** `WeaponSpec` fields, including name and visual kind, target flags, range, firing arc/turret fields, damage bonuses, strike count, all timing, optional splash (all radii and friendly-fire), optional speed/class, upgrade scaling, target-loss behavior and its retarget radius, maximum lifetime, and suicide flag. Exhaustive matching covers nested enums. Including presentation fields here avoids a misleading partial weapon equality contract.

### Players and hazards

For every player slot in array order, include all resource fields: minerals, gas, supply used, supply maximum. Include all `PlayerUpgradeState` fields: weapon and armor level maps with explicitly tagged class keys, shield level, completed upgrade slug set, unit stat modifiers (every `UnitStatDeltas` field), and unlocked ability maps/sets.

For every `ActiveHazardZone` in vector order, encode ID, owner, center, radius, delay, remaining lifetime, interval, next-pulse timer, damage, and friendly-fire flag.

### Map and fog-of-war memory

Use `GameMap` existing getters. Encode dimensions and row-major cells, including cliff level, optional ramp direction, terrain kind, raw flag bits, and optional occupying structure. Encode structures in keyed order, including stored ID, top-left cell and footprint dimensions. Encode all resources (position, kind, remaining count, cell) and watchtowers (ID, position, capture/reveal radii and optional controlling player) in stored sequence order. This is runtime map state verification, not map-editor work.

For each `PlayerVisionMap`, include its own dimensions, all `states()`, `visible_counter()`, and `detector_counter()` values. Include every `known_blocked_cells` vector's length and boolean contents in player order. Fogged/unexplored history is persistent and influences vision-aware pathfinding.

Include `Pathfinder::nav_version()` and the dynamic/static clearance values exposed by `clearance_at(x,y)` and `static_clearance_at(x,y)` across the world map dimensions. These are derived data but can affect behavior if out of sync with terrain, so hashing them catches stale rebuilds. The private pathfinder dimensions are not publicly inspectable: the checksum contract assumes a pathfinder initialized for the world's map, as required by the existing world construction invariant. Do not claim detection of deliberately corrupted private internals.

## Explicit exclusions and reasons

- `catalog` and `upgrade_catalog`: static simulation rules; equal catalog content is an explicit prerequisite. Content fingerprinting is a separate future round, not silently claimed complete here.
- `pending_events`: transient renderer output, not next-tick authoritative input.
- `last_checksum`: derived digest output.
- `scratch_neighbors`, `scratch_targets`, `scratch_dead_entities`: workspaces cleared/repopulated by their owning phases.
- `spatial_hash`: rebuilt spatial query accelerator; verify the rebuild-before-read invariant from the actual tick implementation and document it. If that invariant fails, report to orchestrator rather than silently omit future-affecting state.
- Private pathfinding search buffers and vision source-generation bookkeeping: algorithm workspaces with generation isolation/reset, not semantic state. Inspect their lifecycle; do not hash allocation capacities or stale scratch bytes.

## Implementation sequence

1. Implement the canonical primitive, option, sequence and enum encoders with unit tests for framing/tag behavior.
2. Implement complete entity, order, phase, buff, weapon and projectile encoding. Route the old helper through these encoders under its partial-domain marker.
3. Add world sections: live RNG/counters, player resources/upgrades, hazards, map, vision, known obstacles and clearance. Route the public world method to the new helper; update scope/version documentation.
4. Add mutation-sensitivity tests through public APIs and synthetic data-driven fixtures. Select archetypes by required capabilities or first available suitable entry; no hardcoded production unit IDs or slugs. Preserve existing dirty gameplay code.
5. Run targeted tests, then the required workspace checks through the orchestrator's shared gate. A separate verification round reviews the inventory against current declarations and checks that no field was omitted or mislabeled as scratch.

## Verification requirements

Unit tests must prove distinction between phase variants and payloads, order variant/payload differences, option `None` versus a zero-valued `Some`, vector/string boundaries, and representative nested weapon/splash/target-loss mutations. Include a small documented canonical/golden digest test based on literal primitive bytes and explicit tags, independent of catalog assets, to pin byte order and schema framing.

Integration tests create independent identical worlds and assert equality, then mutate exactly one state component and assert inequality **without stepping first**. Use table-driven labelled mutations to cover every entity field, every projectile field, every hazard field, every player state family, all identifier generators, the live RNG key/stream/position, map cell/structure/resource/watchtower state, fog history/counters, known obstacles, and clearance rebuild version. Small typed helpers keep tests readable; do not create a test that merely copies the encoder implementation.

Specific regressions: equal `rng_seed` with one advanced live RNG must differ; windup timers and target IDs must differ; equal projectile positions with different damage/targets/lifetimes must differ; reordered queued commands must differ; BTree insertion order must not differ; repeated checksums must leave RNG state unchanged; different `last_checksum` and renderer event queues must not change the dynamic digest. Invalid intentionally public state is allowed for mutation coverage but behavioral replay fixtures must be valid.

Add or extend a capability-selected replay test to run at least 2,000 ticks in two independently constructed worlds with the same command schedule, assert every tick digest and final full entity/projectile state equality, and include queued movement/attack commands. Do not rely exclusively on checksum equality to prove correctness. Exercise resource/upgrades/hazards through focused real subsystem tests or direct public mutation sensitivity where commanding those systems is outside this task. Existing behavior tests must continue to pass.

Required gates: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`. No visual changes are made; screenshot review is not applicable. Report any unavailable platform coverage honestly: canonical encoding tests support portability but running on one host does not demonstrate all-platform replay equality.

## Cost and remaining risk

The new checksum scans map/vision/clearance state as well as entities and is therefore O(map cells × players + dynamic objects), with allocation-free streaming. This intentionally prioritizes meaningful verification over the current cheap incomplete hash. Avoid unmeasured timing guarantees. Catalog hashing/negotiation, stronger digests, incremental dirty tracking, and desync diagnostics are separate goals; do not enlarge this round with them or conceal these limitations.
