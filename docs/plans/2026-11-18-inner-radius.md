# Blueprint: Terrain Radius (B-116, StarCraft II's `InnerRadius`)

One recorded bug, fixed by giving each ground body a second radius as the game does:

- **B-116.** Terrain and structures collide with a ground unit's disc at its collision radius (`Radius`). StarCraft II uses `min(InnerRadius, Radius)` (§2). Plan [`2026-11-11-contact-and-missiles`](2026-11-11-contact-and-missiles.md) made bodies discs against terrain and put the radius they use behind one accessor, `MotionPolicy::terrain_radius()`, which still returns the collision radius. This plan does four things:
  - adds the radius to the catalog;
  - feeds it to every path that tests a body against terrain or structures;
  - leaves unit-against-unit contact at `Radius`;
  - authors the 5.0.14 values.

**Scope correction to the register.** The B-116 row lists four kinds of unit: the Stalker, Marauder, Hydralisk and the workers. The 5.0.14 data (§2.2) gives a smaller terrain radius to **17 of our archetypes**:

- the three workers;
- the Zealot, Marauder, Hellion, Stalker, Sentry, Adept, Immortal, Colossus, Ravager, Hydralisk, Lurker, Ultralisk, Queen and Infestor analogues.

The Zealot analogue (`psi_grey`, 0.5 against 0.375) is in both golden replays. The goldens therefore move for that reason, not only because of the workers.

A full plan is required (charter §6): the change moves golden replays and possibly the showcase battle, and it spans data, loader and simulation.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-17 against `main` at `345da93f`. Line numbers drift, so search for the quoted symbol. Claims not confirmed while planning are marked **verify**.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, D-3, D-4 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §6 |
| Q0002: scoping is the agent's call if written down | Standing rule | §10 |
| Open: Q0083, Q0086 (GPU test paths) | Neither touches this plan | — |
| **New: Q0087**, the Zergling's unset `InnerRadius` | Filed by WP0, not blocking; default D-4 | D-4, §10 |

---

## 1. Summary

| | Today | After this plan |
|---|---|---|
| Disc against terrain and structures | `collision_radius` | `terrain_radius` = authored `terrain_radius` if set, else `collision_radius`. The loader requires it to be strictly smaller when authored |
| Disc against units (separation, push, steering walls of bodies, placement occupancy) | `collision_radius` | Unchanged |
| Weapon, scan, cast, harvest, build and boarding reach | `collision_radius` | Unchanged |
| Drawn bubble (`RenderSnapshot` radius) | `collision_radius` | Unchanged |
| Archetypes changed | — | 17 ground archetypes (§2.3). Air units and structures are unchanged |
| Hashed state, checksum schema | — | No new hashed state. Schema stays 29 |
| Expected impact | — | Matrix and battle 33 bit-identical (flat arenas; **verify**). Both goldens move (the Zealot analogue). The showcase may move (two welder drones per side, which are pressed into the spawn walls on tick 9 in contact evidence §3a) |
| Size | — | M: one catalog field, about 12 call sites, 17 data rows, measurement |

---

## 2. What StarCraft II does

Tags: **[DOC]** Blizzard or official material; **[DATA]** SC2GameData 5.0.14 (`5444e12f13`, "SC2 5.0.14.94342"), layer chain `core → liberty → swarm → void → voidmulti` (catalog-time audit evidence §2.2), read from the extraction under `~/scratch/combat_wp3_data/<layer>/UnitData.xml` and checked online for `voidmulti`; **[COMM]** community or editor observation; **[INF]** an inference of this plan.

Online file paths have the form `https://github.com/SC2Mapster/SC2GameData/blob/5444e12f13/mods/<layer>.sc2mod/base.sc2data/GameData/UnitData.xml#L<line>`. A citation "liberty:3975" means that file and line.

### 2.1 The rule

| # | Source | What it says |
|---|---|---|
| R1 | Blizzard, "StarCraft II 5.0.13 PTR Patch Notes", <https://news.blizzard.com/en-us/article/24061893/starcraft-ii-5-0-13-ptr-patch-notes>; the same line in the final notes, <https://news.blizzard.com/en-us/article/24078322/starcraft-ii-5-0-13-patch-notes>, and in Liquipedia [Patch 5.0.13](https://liquipedia.net/starcraft2/Patch_5.0.13) | "Worker inner radius (collision with terrain / structures) reduced from 0.375 to 0.3125." [DOC] |
| R2 | Liquipedia, [Unit Statistics (Legacy of the Void)](https://liquipedia.net/starcraft2/Unit_Statistics_(Legacy_of_the_Void)), key | "Size – Unit collision diameter or building footprint size", with the note that units may have smaller diameters for collision with structures [COMM, wiki] |
| R3 | SC2Mapster threads [161926 "what is radius"](https://www.sc2mapster.com/forums/development/data/161926-what-is-radius?comment=3), [175792 "collision size of unit"](https://sc2mapster.com/forums/development/miscellaneous-development/175792-collision-size-of-unit), [155862 "unit collision"](https://sc2mapster.com/forums/development/data/155862-unit-collision?comment=2) | Inner radius governs interaction with buildings and Radius governs interaction with units. Units "use the smaller of inner radius and normal radius" for building collision, so a larger inner radius has no effect. A pathing footprint, where a unit has one, overrides both. Read as search snippets only: the site now redirects to CurseForge [COMM] |
| R4 | `core`:82 (`CUnit` default) | `InnerRadiusSafetyMultiplier` 1. The class default sets **no** `InnerRadius` (`core`:41-83) [DATA] |
| R5 | Contact plan C2 to C5, D-1 | Paths are found at the unit's size (radius funnel), and the disc against terrain is the same radius [DOC, INF] |

**What follows for us.**

- **The terrain radius is `min(InnerRadius, Radius)`** [DOC for the meaning (R1), COMM for the `min` (R3)]. A unit whose `InnerRadius` is at least its `Radius` behaves as if it had none. The Roach is such a unit (§2.2).
- **One radius for every terrain question** [INF, R5]. The game's footprint pathing and its disc against terrain use the same unit size. So routes (radius widths, funnel), the disc contact, the map-edge domain and placement against structures all use the terrain radius.
- **Units against units keep `Radius`** [COMM R3, DOC R1 "terrain / structures" only]. That covers separation, pushes, steering's walls of bodies and placement occupancy. Weapon and ability ranges are edge-to-edge between unit radii, as now.
- **Not known.** What the engine does with an **unset** `InnerRadius` (the Zergling, §2.2). D-4 and Q0087 cover it.

### 2.2 The 5.0.14 values

Every ground analogue in `assets/catalog/units.ron` is listed. "Radius" is the value our catalog already pins in `catalog_reference_values_tests::SC2_UNIT_RADIUS` (with that table's citations). "Effective" is `min(InnerRadius, Radius)`.

| SC2 unit | Our slug | Radius (ours = game) | InnerRadius (source) | Effective | New `terrain_radius` |
|---|---|---:|---|---:|---:|
| SCV | `welder_drone` | 0.375 | 0.3125 (voidmulti:1528; was 0.375 at liberty:4745; R1) | 0.3125 | **0.3125** |
| Probe | `grav_harvester` | 0.375 | 0.3125 (voidmulti:1287; liberty:3085 0.375) | 0.3125 | **0.3125** |
| Drone | `nectar_worker` | 0.375 | 0.3125 (voidmulti:523; liberty:7050 0.375) | 0.3125 | **0.3125** |
| Zealot | `psi_grey` | 0.5 (class default) | 0.375 (liberty:3186) | 0.375 | **0.375** |
| Marauder | `shock_golem` | 0.5625 | 0.375 (liberty:11016) | 0.375 | **0.375** |
| Hellion | `napalm_wheeler` | 0.625 | 0.5 (liberty:11570) | 0.5 | **0.5** |
| Stalker | `warp_strider` | 0.625 | 0.5 (liberty:3975) | 0.5 | **0.5** |
| Sentry | `prism_weaver` | 0.5 (class default) | 0.375 (liberty:11395) | 0.375 | **0.375** |
| Adept | `astral_vagrant` | 0.5 (class default) | 0.375 (void:3233) | 0.375 | **0.375** |
| Immortal | `void_goliath` | 0.75 | 0.5 (liberty:10746) | 0.5 | **0.5** |
| Colossus | `tripod_annihilator` | 1.0 | 0.5625 (liberty:4053) | 0.5625 | **0.5625** |
| Ravager | `mortar_stag` | 0.75 | 0.5 (liberty:1019) | 0.5 | **0.5** |
| Hydralisk | `thorn_viper` | 0.625 | 0.375 (liberty:7685) | 0.375 | **0.375** |
| Lurker | `burrow_scorpion` | 0.9375 | 0.5 (void:755; liberty:741 was 0.375) | 0.5 | **0.5** |
| Ultralisk | `stag_behemoth` | 1.0 | 0.75 (liberty:8024) | 0.75 | **0.75** |
| Queen | `hive_matron` | 0.875 | 0.5 (liberty:11417) | 0.5 | **0.5** |
| Infestor | `mycelium_shaman` | 0.625 (voidmulti:830) | 0.5 (liberty:10071) | 0.5 | **0.5** |
| Marine | `pulse_bot` | 0.375 | 0.375 (liberty:4839) | 0.375 | omitted (equal) |
| Reaper | `jet_jumper` | 0.375 | 0.375 (liberty:4912) | 0.375 | omitted |
| Ghost | `specter_android` | 0.375 | 0.375 (liberty:5007) | 0.375 | omitted |
| Siege Tank | `railgun_crawler` | 0.875 | 0.875 (liberty:5096) | 0.875 | omitted |
| Thor | `colossus_mech` | 1.0 (void:1404) | 1.0 (void:1406; liberty 0.8125, swarm 0.9375, both overridden) | 1.0 | omitted |
| High Templar | `cerebral_elder` | 0.375 | 0.375 (liberty:3268) | 0.375 | omitted |
| Dark Templar | `shadow_visitor` | 0.375 | 0.375 (liberty:3362) | 0.375 | omitted |
| Baneling | `volatile_tick` | 0.375 | 0.375 (liberty:9322) | 0.375 | omitted |
| Roach | `rhino_beetle` | 0.5 (class default) | 0.625 (liberty:7197), larger than Radius | 0.5 | omitted (R3: no effect) |
| Zergling | `razor_mantis` | 0.375 | **unset** (liberty:7563 block; class default sets none) | ? | omitted; D-4, Q0087 |

- **Air units** (Phoenix, Viking, Medivac, Liberator, Banshee, Raven, Observer, Warp Prism, Void Ray, Carrier/Mothership, Mutalisk, Corruptor, Brood Lord, Viper, Overlord): terrain does not touch them. The only air-related `InnerRadius` in the data is `VikingAssault` (liberty:6788), the landed Viking, which we do not model (B-46).
- **Mode variants.** `LurkerMPBurrowed` has 0.25 (liberty:808), but a burrowed Lurker cannot move. Sieged tanks, burrowed Roaches, Infestors and Drones repeat their unsieged or unburrowed values. See D-9.
- **Online check.** The `voidmulti` values (Archon, Drone, Probe and SCV `InnerRadius`; Infestor `Radius`) were confirmed against the online file at `5444e12f13` on 2026-11-17. The other layers are the extraction already cited by plans `2026-11-04-combat-sc2-alignment` and `2026-11-05-sc2-deviation-fixes`.
- **Search note.** Liquipedia's unit pages carry no per-unit inner radius (the Stalker infobox has `cargo_size` only), so the game data is the primary source for every value. R1 independently confirms the workers' value.

---

## 3. Diagnosis

`MotionPolicy` has one radius (`crates/sim_core/src/movement/mod.rs`, `MotionPolicy::resolve` reads `arch.collision_radius`). `terrain_radius()` returns it. Every path that tests a body against terrain or structures reads it in one of three ways: through `terrain_radius()` (only `movement/contact.rs`), through `policy.radius`, or straight from `arch.collision_radius`.

The catalog has no field for a second radius. `UnitArchetypeDef` does not deny unknown fields, so a `terrain_radius` written today would be silently ignored. That is also why the tests can be written first (§6).

Call sites, by role, found by `grep -rn "collision_radius\|policy.radius\|\.radius\b"` over `crates/sim_core/src` and `crates/sim_fixtures/src`. Each is classified in D-5 and D-6.

---

## 4. Design decisions

### D-1. Data model: `terrain_radius`, an optional per-archetype field

- **Authored** (`UnitArchetypeDef`, `crates/sim_core/src/catalog.rs`): `#[serde(default)] terrain_radius: Option<f32>`, next to `collision_radius`. Omitted means "no separate terrain radius". This follows the existing `stationary_turn_rate_deg_per_tick: Option<f32>` pattern, an optional fact read by a capability-style query: no code branches on a unit identity (charter §3.3).
- **Resolved** (`UnitArchetype`): `pub terrain_radius: SimScalar`, always set: the authored value converted by `SimScalar::from_f32_lossy`, otherwise `collision_radius`. Every caller reads one value and none computes the `min`. Structure archetypes (`StructureArchetypeDef → UnitArchetype`, the `collision_radius: radius` literal) get `terrain_radius: radius`. `structures.ron` gains no field.
- **Why not under `capabilities`.** That block holds mechanics (Overclock, harvesting, transport, modes). A body dimension belongs beside `collision_radius` and `separation_radius`, and is read by movement, not by an ability.
- **Why not `inner_radius`.** The name already exists in the splash geometry (`WeaponSplash::inner_radius`, the 100 % damage zone). `terrain_radius` names what the value does in our engine and matches the accessor the contact plan introduced. Docs and comments cite the game's `InnerRadius`.
- **`///` docs** on both fields state the unit (world units), the rule (`min(InnerRadius, Radius)`, 5.0.13 notes), what reads it (D-5) and what does not (D-6).

### D-2. Loader validation (`crates/sim_core/src/catalog/validation.rs`)

For `terrain_radius: Some(t)`, each failure is a typed `invalid(slug, "terrain_radius", reason)`:

1. `scalar` (finite) and `positive` (strictly positive after Q32.32 conversion).
2. **Strictly below `collision_radius`**, compared after conversion: "must be smaller than collision_radius; omit it when equal". A value at or above the collision radius has no effect in the game (R3). Refusing it keeps the field's presence meaningful, and is what lets the reference test (WP3) catch a mis-authored row.
3. **Ground layers only**: `mover_type` must be `Ground`, `CliffJumper` or `CliffWalker`. An `Air` archetype is refused ("terrain does not touch air bodies").

In addition, `MotionPolicy::resolve` re-checks `0 < terrain_radius ≤ radius`, so that archetypes built as struct literals in tests cannot bypass the loader. A failure is a `MovementInvariant::InvalidState` with the reason "terrain radius must be positive and at most the collision radius", like the existing collision-radius check.

### D-3. The effective value is the game's `min`, authored only where it bites

`units.ron` gets the 17 rows of §2.2, each with a comment naming the layer and line, e.g. `// 5.0.14 InnerRadius 0.5 (liberty UnitData:3975); Radius 0.625.`

- Units whose `InnerRadius` equals their `Radius`, and the Roach (whose is larger), omit the field. Their comments say why, so a later reader does not "fix" them.
- The workers' comment also cites R1.

### D-4. The Zergling: unset `InnerRadius` reads as the collision radius (Q0087)

The 5.0.14 Zergling sets no `InnerRadius`, and the `CUnit` default sets none (R4). The engine therefore sees the schema default, which the data does not show. Two readings exist:

- **(a)** Unset means no separate terrain radius, so the unit collides with terrain at `Radius` 0.375.
- **(b)** Unset is 0. Under `min` that makes the Zergling a point against terrain and structures. Community advice to "set inner radius to 0" so that units pass through buildings (R3) points that way. But the same data gives `ChangelingZergling` (`parent="Zergling"`) an explicit 0.375 (liberty:12004), which suggests the designers wanted a nonzero value there.

The default is **(a)**: no change for `razor_mantis`. It keeps the B-113 disc and makes no unsourced claim. **Q0087** (filed in WP0, not blocking) asks the owner to accept (a) as a recorded minor difference, or to direct research into (b). Reading (b) would be a one-line data change plus a loader rule allowing an authored 0 meaning "centre point against terrain", and would get its own measurement.

### D-5. Paths that use the terrain radius

All read `MotionPolicy::terrain_radius()` (`policy` in movement) or `UnitArchetype::terrain_radius` (outside movement).

| # | Path | File, symbol | Change |
|---|---|---|---|
| T1 | **Contact query and non-deepening rule**: settle, probes, slide | `movement/contact.rs` (`settle_on_terrain`, `probe_permits`, `slide_along_wall`, `clearance`) | Already reads `terrain_radius()`. Only the accessor's body changes |
| T2 | **Route requests**: radius widths, radius funnel, `CORNER_SAFETY_MARGIN` offsets, the segment predicate on later legs, `nearest_reachable_point`, the per-tick `ReachCache` key | `movement/locomotion.rs` `route_request` (`radius: policy.radius` → `policy.terrain_radius()`); the retargeted request copies it | One line. Every route, `route_is_usable`, `drop_passed_waypoints`' in-view sweep and the stored `request` follow |
| T3 | **Group lanes**: the leader's lane search | `movement/groups.rs`, the `active.push((id, pos, policy.radius))` that feeds `search_lane(leader, radius, …)` | `policy.terrain_radius()`. The lane is a route, so it uses the leader's terrain disc |
| T4 | **Charge dash path** | `world/attack_dash.rs` (`PathRequestWithVision { radius: arch.collision_radius, … }`) | `arch.terrain_radius`. The dash's gap to its target (`distance − r_a − r_b`, line 111) stays at `Radius` (D-6) |
| T5 | **Domain (map edge)**: `MotionPolicy::contains`, and through it `permits_segment`, `validate_origin`, movement admission (`validate_movement_order` → `OutsideDomain`), the active-order check in locomotion, the cast approach's `clamp_into_domain`, `flee_destination`'s bounds, placement's `OutsideDomain` | `movement/mod.rs`, `movement/locomotion.rs`, `movement/flee.rs` | Use `terrain_radius()`. The map border is a wall (contact plan D-1), so a route funnelled at `t` along the border must not then be refused by a domain at `r` (R-4). Air bodies keep `radius` (their terrain radius equals it, D-2). The messages "full-radius movement domain" become "terrain-radius movement domain", and the `OutsideDomain` docs and the module doc follow |
| T6 | **Placement against terrain and structures**: Blink, unload, Abduct landing (`first_valid_placement`) | `movement/mod.rs` `validate_placement_terrain` (`ground_footprint_connected(…, policy.radius, …)`) | `policy.terrain_radius()`. `validate_placement_unoccupied` stays at `radius` (D-6) |
| T7 | **Production exits** | `production/exits.rs` `exit_ring` (ring `margin`, the product's map-bounds check) and `free_exit_point` (both `egress_connected` calls) | The ring margin, the bounds check and terrain egress use the product's `terrain_radius`. The occupancy test against other bodies (`sum = radius + arch.collision_radius`) and `production.max_radius` stay at `collision_radius`. `ExitRing` gains `terrain_radius` beside `radius` (D-8) |
| T8 | **Construction site occupancy**: a ground body standing on a site blocks it | `production/construction.rs` `stands_on` (`gap < arch.collision_radius`) | `arch.terrain_radius`. A structure meets a unit at its terrain disc |
| T9 | **Mobile producer bounds** | `production/lifecycle.rs` (`"mobile producer footprint outside map"`) | The producer's terrain radius for ground producers, so this check agrees with T5. No shipped mobile unit trains units; kept consistent for the rule's sake (**verify** no shipped hit) |
| T10 | **Overlap meter (tooling)** | `sim_fixtures/src/trace/route_overlap.rs` (`arch.collision_radius`) | `arch.terrain_radius`, so `route_trace`'s "deeper unit-ticks" measures the disc the rule keeps |

**Not changed, and why:**

- The **clearance field** (`Pathfinder::clearance_at`, `static_clearance_at`, hashed in `checksum.rs`) is a Euclidean distance field of the map and depends on no unit radius. No consumer compares it with a radius (only the checksum and `sim_map` tests read it).
- `sim_map` needs **no code change**: every query takes the radius from its request. WP2 adds a property test (§7).

### D-6. Paths that stay at `Radius` (`collision_radius`, `policy.radius`)

| Path | Where | Reason |
|---|---|---|
| Unit-unit separation and pushes | `movement/collision.rs` (`radius_a`, `radius_b`, `largest`, `reach`) | "Radius governs units" (R3) |
| Steering obstacles, neighbour reach, walls of bodies, the corridor, `aim_past_parked_body`, cluster arrival | `locomotion.rs` `obstacle_of`, `StepContext.body.radius`, `steering.rs` `forms_wall`/`wall_end`/corridor (`end.radius + unit.radius + STEERING_CORRIDOR_MARGIN`) | Bodies against bodies |
| Waypoint handoff (`radius + radius + WAYPOINT_HANDOFF_MARGIN`) | `locomotion.rs` (`pass_reached_waypoint`, `drop_passed_waypoints`, corner subsequent search) | A crowd tolerance of one body width. Whether a waypoint may be passed is still guarded by the disc in-view sweep (T2). Changing it would move routes for no game rule (§10) |
| Placement occupancy | `validate_placement_unoccupied` (`circles_overlap`) | Units against units |
| Production exit occupancy | `exits.rs` `sum`, `max_radius` | Units against units |
| Weapon, scan and acquisition ranges, projectile contact, missile contact, splash searches, cast range | `target_acquisition.rs`, `combat_resolution.rs`, `effects/step.rs`, `world/casts.rs` | Edge-to-edge ranges between unit radii, as in the game |
| Harvest, build and boarding reach | `economy/mod.rs` `body_radius`/`within_reach`, `transport/orders.rs` (`a.collision_radius + b.collision_radius`) | Interaction ranges. A worker now stands up to 0.0625 nearer a mineral footprint and is still within reach (`gap ≤ r + TASK_REACH` holds more easily) |
| Snapshot radius and client looks | `world/snapshot.rs` (`radius`), client `effects::catalog::units` (`muzzle`), `effects::struck` | The drawn body is the unit's size |
| Friendly-blocking trace | `sim_fixtures/src/trace/blocking.rs`, `trace/mod.rs` | Unit-unit gaps |

### D-7. Domain consistency

After T5 the ground domain, the funnel at the border and the contact rule all use the same `t`. A ground body may then stand with its unit disc overhanging the map edge by up to `r − t`: at most 0.4375, for the Colossus analogue. That is presentation only. Units on both sides still separate at `Radius`. Air is unchanged.

### D-8. Production exits

The ring sits `margin = ceil(t + 0.5)` cells off the footprint (today `ceil(r + 0.5)`). For the workers the margin stays 1 (0.8125 and 0.875 both round up to 1). For the Stalker, Marauder, Hydralisk, Queen, Immortal, Ravager, Infestor and Hellion analogues it drops from 2 to 1. Their exit cell centre then lies `t` (0.5 or 0.375) from the footprint edge, as `segment_overlaps_open_box` admits (open box).

Exactly where the game puts a new unit is not documented [INF]; it collides with its producer, a structure, at the terrain radius (R1). Production tests that pin exit points are re-derived (WP2).

### D-9. One terrain radius per archetype

Burrowed and sieged states share their archetype's value. The one game difference is `LurkerMPBurrowed` 0.25, and a burrowed Lurker cannot move. Burrowed bodies walk on the ground layer and so use the archetype's `t`, as the contact plan's D-1 treats them as ground bodies. Recorded in §10.

### D-10. Determinism

- No floats in state: `terrain_radius` is converted once at load (`from_f32_lossy`). Every shipped value is a multiple of 1/16, exact in Q32.32 and on the mesh's Q16 lattice.
- No new collections, no new hashed state. `MotionPolicy` and the catalog are not hashed. `CHECKSUM_SCHEMA_VERSION` stays 29. If a WP finds it must hash something, it bumps the schema and records it.
- Bit-identity of the wiring is shown separately from the data (WP2 against WP3). With every `terrain_radius` omitted, the code change must be a no-op on every golden, the matrix, battle 33 and the showcase.

### D-11. Measurement

`<base>` is the tree before the WP and `<cand>` the WP's tree. Release tools are copied per tree as in contact evidence "How to reproduce", each tree building into `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` (charter §7).

| What | Command | WP1, WP2 (no data) | WP3 (data) |
|---|---|---|---|
| Matrix | `matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv` | Bit-identical | Expected bit-identical (flat arenas, D-12) |
| Battle 33 | `matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33`; `diff` of `seeds.csv` | Byte-identical | Expected byte-identical (zealots on a flat arena) |
| Showcase | `route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route`, and to the match's end; seeds 1 to 8 with `--seed` | `units.csv` byte-identical | D-12 acceptance |
| Overlap meter | `route_trace` deeper unit-ticks | 0 / 0 | 0 / 0 at the terrain radius |
| Goldens | `golden_replay_tests` in debug and release native, and `--target wasm32-wasip1 --release` under wasmtime (CI's `determinism-wasm` command); also once in debug on wasm in WP3 | Pass unchanged | Re-recorded, each with its first changed checkpoint and traced cause |
| Throughput | `cargo bench -p sim_core --bench throughput` under the gate lock, base and candidate alternated, three rounds | Within noise | Within noise (pulse bots, unchanged); every scenario ≥ 25 ticks/s |

### D-12. Stop rules and acceptance (WP3)

General rule, as in contact plan D-9: **a winner flip anywhere stops the WP.** The agent commits nothing beyond the WP and reports the trace: first divergent tick, entity and wall. A movement that is not a flip is explained before the WP proceeds.

1. **WP1 and WP2 must be bit-identical** on everything in D-11. Any difference is a wiring defect: stop, find it, do not re-record.
2. **Matrix.** Expected bit-identical: square flat arenas whose only walls are borders.
   - Any moved battle is traced. It is accepted in the evidence only when the cause is an affected archetype at the arena border (a route, contact or domain at `t`) and no recorded outcome in `fixtures/matchup_matrix.ron` changes.
   - Any outcome change stops the WP for an orchestrator amendment.
3. **Battle 33.** Expected byte-identical. A change is traced like the matrix; an outcome-distribution change stops.
4. **Showcase.** Base: player 1 wins at tick 2,218 on the default seed; seeds 1 to 8 (contact evidence §4a): P0 seed 1 (2,152), P1 seeds 2 to 8 (2,180, 1,943, 2,597, 1,958, 1,938, 2,603, 1,927). Only the welder drones change.
   - Acceptance: same winner on the default seed and on each of seeds 1 to 8; 0 deeper unit-ticks; stall give-ups ≤ base + 1; reversals ≤ base + 10; the match ends.
   - If the winner flips: stop, trace from the spawn (tick 9, contact evidence §3a, where welder drone 16 is pressed into the spawn walls), and classify the cause as rule or defect. The cited reason for a rule-caused flip is R1. The orchestrator decides by amendment.
   - If the default-seed timeline moves at all, WP4 re-checks the storyboard.
5. **Goldens.** Expected to move (the 12 and 4 `psi_grey` zealots, `t` 0.375 against 0.5, near the default arena's cliffs, ramps and domes).
   - Each re-recorded header names the first changed checkpoint and the traced cause (entity, tick, which wall).
   - `golden_replay_orders` must still find both phase-order situations. If it loses one, choose the smallest goal change that restores it and that a mutation still catches, as in pathfinding Amendment 6, and record it.
   - A golden that does **not** move is a finding: the zealots never touch a wall at their radius. Record it; it is not an error.
6. If acceptance is not reached after the rule is shown correct by its tests, the WP stops and reports the numbers. No tuning constant is invented.

---

## 5. File ownership

| Area | Files | WP |
|---|---|---|
| Plan row, question, register text | `docs/plans/README.md`, `docs/questions/closed/0087-zergling-inner-radius.md`, `DESIGN.md` §4 | WP0 |
| Catalog model and loader | `crates/sim_core/src/catalog.rs`, `crates/sim_core/src/catalog/validation.rs` | WP1 |
| Struct-literal archetypes in tests | `crates/sim_core/src/world/snapshot.rs` (test), `crates/sim_core/tests/damage_and_shields_tests.rs`, `crates/sim_core/tests/attack_move_simulation_tests.rs` | WP1 |
| Wiring | `crates/sim_core/src/movement/{mod.rs,locomotion.rs,groups.rs,flee.rs}`, `crates/sim_core/src/world/attack_dash.rs`, `crates/sim_core/src/production/{exits.rs,construction.rs,lifecycle.rs}`, `crates/sim_fixtures/src/trace/route_overlap.rs` | WP2 |
| Behavioural tests | `crates/sim_core/tests/terrain_radius_tests.rs` (new; `fast` by default, so `crates/gate/suites.ron` needs a rule only if it measures `slow`: C22), unit tests in `exits.rs`, `construction.rs`, `movement/mod.rs`; `crates/sim_map/tests/navmesh_tests.rs` (property) | WP2 |
| Data | `assets/catalog/units.ron` | WP3 |
| Reference table | `crates/sim_core/tests/catalog_reference_values_tests.rs` | WP3 |
| Goldens | `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron` | WP3 |
| Client and showcase check | `assets/captures/showcase.ron` (only if the timeline moved) | WP4 |
| Documents | `DESIGN.md` (§4 Movement row, register, §5 if figures move), this plan, `docs/plans/README.md`, `docs/evidence/2026-11-18-inner-radius.md` (new), pointer amendment in `2026-11-11-contact-and-missiles.md` | WP0, WP3 to WP5 |

---

## 6. Work packages (commit order)

Branch `inner-radius`. Each package writes its tests first and shows them red on its base, except WP0. Implementation agents hand back after `cargo gate quick` and the tests their change needs; a verifier runs `cargo gate package` (Q0061). WP1 and WP2 form one simulation package, verified together. Each WP appends to `docs/evidence/2026-11-18-inner-radius.md`, which WP1 creates with "How to reproduce" (D-11). Sizes: S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- This plan's row in `docs/plans/README.md` (`Proposed`).
- `docs/questions/closed/0087-zergling-inner-radius.md`: D-4's two readings, the sources (R3, R4, liberty:12004), recommendation (a), and "Default until answered: (a), no data change for the Zergling analogue". Blocking: no.
- `DESIGN.md` B-116 row restated: 17 archetypes (§2.2), the Zealot included; pointer to this plan.
- Gate: docs-only `package` (fmt, `workspace_checks`, C17 and C18).

### WP1: the field and the loader (S)

- **Tests first.** These are `catalog/validation.rs` unit tests over minimal RON archetypes; on the base the field is ignored, so the tests that expect a value or a refusal are red:
  - `a_terrain_radius_is_read_and_defaults_to_the_collision_radius`;
  - `a_terrain_radius_must_be_positive_and_finite` (0, −0.1, NaN; the error names `terrain_radius`);
  - `a_terrain_radius_must_be_smaller_than_the_collision_radius` (equal and larger refused);
  - `an_air_unit_may_not_have_a_terrain_radius`;
  - the shipped catalogs load (existing `runtime_catalog_tests`).
- **Change.** D-1 and D-2. Add `terrain_radius` to the three struct-literal test sites, equal to their collision radius. `MotionPolicy` is not touched yet.
- **Exit.** Everything bit-identical (no data, no reader).

### WP2: every terrain path reads the terrain radius (M)

- **Tests first** (`crates/sim_core/tests/terrain_radius_tests.rs`, unless noted). Synthetic maps are built in the test, and a scratch catalog changes a shipped ground archetype's RON (`terrain_radius: 0.3125` with `collision_radius: 0.5`), as `route_following_tests` builds its radius variants. All are red on WP1 (the field is read but unused):
  - T-1 `a_body_rests_at_its_terrain_radius_against_a_wall`: a unit walks and is pushed into a straight cliff face. Its final distance to the face is `t` within `WALL_CONTACT_TOLERANCE`, and never below `t` on any tick (contact rule at `t`).
  - T-2 `a_gap_that_fits_the_terrain_disc_is_routed_through`: a slot of width 0.8 (`2t = 0.625 < 0.8 < 2r = 1.0`). The unit with `t` routes through it; the same archetype without the field routes round (T2: funnel and widths).
  - T-3 `corner_waypoints_keep_the_terrain_radius_and_margin`: at an L-shaped cliff every route vertex keeps at least `t + CORNER_SAFETY_MARGIN ÷ 2` from the corner and less than `r` (**verify** the funnel's exact offset and state it in the assertion).
  - T-4 `two_bodies_still_separate_at_their_full_radii`: two such units pushed together on open ground end at centre distance ≥ `r_a + r_b` (less the collision phase's tolerance). Unit-unit contact is unchanged (D-6).
  - T-5 `the_map_edge_admits_the_terrain_disc`: a `Move` to `t + ε` from the border is admitted and walked; one to `t − ε` is refused `OutsideDomain`; an air unit's domain is still `r` (T5).
  - T-6 `a_placement_beside_a_structure_uses_the_terrain_radius`: an unload or teleport point `t + ε` from a footprint is admitted, and `t − ε` is refused `Terrain`; a second unit within `r_a + r_b` still refuses it as `Occupied` (T6, D-6).
  - T-7 `a_body_at_its_terrain_radius_from_a_site_does_not_block_it` (T8).
  - T-8 (unit, `exits.rs`) `an_exit_ring_keeps_the_terrain_radius_from_the_footprint_and_the_collision_radius_from_bodies`: margin 1 for `t` = 0.5 and `r` = 0.625; a body at the first exit within `r + r_other` moves the exit to the next cell (T7, D-8).
  - T-9 `a_charge_dash_path_is_tested_at_the_terrain_radius`: a dash past a wall end that the `t` disc clears and the `r` disc does not (T4).
  - T-10 `a_group_lane_is_searched_at_its_leaders_terrain_radius`: a two-member group whose leader has `t`, with a lane through a slot only the `t` disc fits (T3).
  - T-11 (property, extends contact T-B7) `a_settled_move_never_deepens_at_the_terrain_radius`: seeded walls and pushes with `t < r`; depth measured at `t` never rises.
  - T-12 (unit, `movement/mod.rs`) `a_terrain_radius_above_the_collision_radius_is_an_invariant` (D-2's resolve check).
  - T-13 (`crates/sim_map/tests/navmesh_tests.rs`, property; passes on the base and pins existing behaviour) `a_smaller_disc_routes_wherever_a_larger_one_does`: on both shipped maps, bare and with seeded 2×2 structures, 500 seeded start and goal pairs for the radius pairs (0.3125, 0.375), (0.375, 0.5625), (0.5, 0.625) and (0.5625, 1.0). A route for the larger disc implies one for the smaller. If it fails, report it as a finding; it is not fixed in this plan unless the cause is local and stated.
- **Change.** `MotionPolicy` gains `terrain_radius: SimScalar` from `arch.terrain_radius`, and `terrain_radius()` returns it. Then T2 to T10 of D-5. Messages and docs per T5.
- **Audit in the commit message.** The output of `grep -rn "collision_radius\|policy.radius\|\.radius\b" crates/sim_core/src crates/sim_fixtures/src`, each production hit classified as D-5 or D-6, with the reason.
- **Expected test edits.** Tests that use shipped archetypes keep passing, because no data is set. A test that changes is a wiring defect.
- **Exit.** D-12, 1: everything bit-identical, including showcase `units.csv`. Throughput within noise.

### WP3: the 5.0.14 values (S to M)

- **Tests first.**
  - `catalog_reference_values_tests::b116_unit_terrain_radii_follow_starcraft`: a table `SC2_UNIT_TERRAIN_RADIUS` naming analogues, never slugs (C5), with the effective value of §2.2 and the citation for every ground analogue (the Zergling row cites D-4 and Q0087; the Roach row "InnerRadius 0.625 ≥ Radius, no effect"). Checked through `UnitArchetype::terrain_radius` to 0.001; air analogues are asserted to equal `collision_radius`. Red on WP2.
  - `catalog_reference_values_tests::only_ground_units_with_a_smaller_inner_radius_author_one`: every archetype whose `terrain_radius < collision_radius` is in the table with a smaller value, so no unsourced row can be added.
- **Change.** The 17 rows of §2.2 in `units.ron`, with comments (D-3).
- **Measure.** Everything in D-11. **Stop rules** of D-12.
- **Goldens.** Re-recorded on this tree with traced causes: dump every entity's position each tick on WP2 and WP3, as contact evidence §4 did. Pass in debug, release and `wasm32-wasip1` (wasmtime), plus wasm debug once.
- **Expected test edits outside the goldens,** each re-derived and stated in the commit:
  - production tests that pin exit cells of the eight archetypes whose margin drops (`production_transactions_tests`, `exits.rs` unit tests; **verify** which);
  - construction or economy tests with workers standing beside sites;
  - `attack_dash_tests` (the Zealot analogue's Charge: `q0071_the_maximum_is_the_walk_not_the_straight_line` walks round a wall at `t`);
  - `movement_contact_tests`, `wall_contact_tests` and `collision_and_immobility_tests` where they use an affected archetype.

  A test whose change is not explained by D-5 is a finding and stops the WP.

### WP4: client and presentation check (S)

- **No client code changes.**
  - The client draws bubbles at the snapshot radius (`Radius`) and finds struck bubbles by id.
  - It asks the simulation for placement (`SimWorld::preview_site`) and admission, so the preview and the status text follow D-5 by themselves.
  - It has no copy of any domain or radius rule (checked: `crates/client/src` reads `collision_radius` only for the muzzle in `effects::catalog::units` and the glass in `effects::struck`; it reads `OutsideDomain` only for its status text).
- **Client tests.** The catalog rule of `crates/gate/suites.ron` (`assets/catalog/**`) selects `asset_baker` catalog tests and the client targets it lists. Client battle tests compare checksums within a run, not pinned values, so none is expected to need an edit; any failure is traced to a moved battle and its assertion re-derived, never loosened. `client_diagnostics/showcase_storyboard_tests` and `showcase_tests` run if the showcase moved.
- **Picture check.** One capture on the RX 580 under `with-gpu-lock` (charter §7), lab or skirmish, of a `t < r` archetype resting against a dome and a cliff (a stand-in lent the look in the test world only if the analogue has no media). Its bubble overlaps the wall by up to `r − t`, as the game's models overlap building edges. Recorded in evidence as observation only. If it reads as a fault, it is registered as a new bug (next free number, **verify** B-131) and left out of scope (§10).
- **Storyboard.** If WP3 moved the showcase timeline, re-check `assets/captures/showcase.ron` with a real render (`showcase render --driver radv`, one GPU lock per take) and adjust scene ticks and captions, as contact evidence §4c did.
- **Throughput** (D-11) under the gate lock. `DESIGN.md` §5 is updated only if the figures move beyond noise.

### WP5: bookkeeping (S)

- **Register:** B-116 removed as fixed, with commits. The Zergling question stays with Q0087 and is not a register entry while (a) is the default: a recorded minor difference under charter §8 if the owner accepts it.
- **`DESIGN.md` Movement row:** "bodies are discs against terrain and structures at their terrain radius (`min(InnerRadius, Radius)` of 5.0.14), and against each other at their radius"; `terrain_radius_tests.rs` added to its test list.
- **Pointers:** a one-line amendment in `2026-11-11-contact-and-missiles.md` §9 pointing here; this plan's row becomes `Done` with commits and the evidence link.
- **Gate:** `merge` by the orchestrator.

---

## 7. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Loader | The field is read, defaulted, and refused when meaningless (≤ 0, ≥ radius, air) | `catalog/validation.rs` unit tests (WP1) |
| Invariant | A built archetype cannot bypass the loader | `movement/mod.rs` unit test T-12 |
| Contact | Rests at `t` and never deepens at `t` | T-1, T-11 |
| Routes | Width, funnel and lanes at `t` | T-2, T-3, T-10, T-13 (monotone in radius, `sim_map`) |
| Domain and placement | Map edge, teleport and unload, construction site, production exits at `t`; occupancy at `r` | T-5 to T-8 |
| Abilities | Charge path at `t` | T-9 |
| Unit-unit | Separation unchanged at `r` | T-4 |
| Data | Every ground analogue equals 5.0.14's effective value; no unsourced row | `catalog_reference_values_tests` (WP3) |
| Determinism | Wiring bit-identical; data moves only what D-12 allows; three targets | D-11: goldens (native debug and release, wasm32-wasip1), matrix, battle 33, showcase |
| Client | No code change; picture observation; storyboard if moved | WP4 |
| Charter | C5 (no slugs in Rust; tables name analogues), C17, C18, C22 | `workspace_checks`, `suite_catalog_tests` |

---

## 8. Determinism summary

- Exact fixed point: the radius is converted once at load, and every shipped value is a multiple of 1/16.
- No floats in state, no new hashed state, no new collections; checksum schema 29 unchanged; `sim_map` digests (`determinism_tests`, `validation_golden_tests`) unchanged, since `sim_map` code is untouched.
- WP1 and WP2 are proven no-ops by bit-identical goldens, matrix, battle 33 and showcase.
- WP3 re-records both goldens with traced causes and replays them in debug and release on Linux x86_64 and on `wasm32-wasip1` (release, as CI; debug once locally). CI's determinism jobs cover Windows and macOS ARM64 on the push.

---

## 9. Risks

| # | Risk | Mitigation |
|---|---|---|
| R-1 | The showcase's near-tied lane fight flips again: welder drones at player 1's spawn walls (tick 9) shift the early ticks | D-12, 4: stop, trace, orchestrator amendment; seeds 1 to 8. Only the drones change, and pulse bots and rhino beetles are untouched, so the lane fight's units are the same |
| R-2 | The goldens move | Expected (the Zealot analogue). Traced headers; `golden_replay_orders` situations checked |
| R-3 | The matrix moves through arena borders | Traced; accepted only without an outcome change (D-12, 2) |
| R-4 | Radii disagree between paths (domain at `r`, funnel at `t`), so a routed body is refused at the border or a wall and stalls | One accessor; WP2's audit grep classifies every site; T-5, T-6; bit-identity of WP2 shows the wiring is pure |
| R-5 | Bubbles visibly overlap cliffs or domes by up to `r − t` | WP4 picture check; register if it reads as a fault (§10) |
| R-6 | The Zergling's unset value is read the wrong way | D-4 default (a), Q0087, recorded |
| R-7 | Production spawns move one cell inward for eight archetypes and change skirmish openings | D-8; tests re-derived; skirmish outcomes are not pinned |
| R-8 | Units now slip through structure gaps that held them before (e.g. a 0.8-wide gap for a Stalker analogue) | This is the game's rule (R1); T-2 pins it |
| R-9 | `UnitArchetypeDef` ignores unknown fields, so a mistyped key in `units.ron` would be silently dropped | The WP3 reference table pins every effective value, so a dropped row fails there. Making the def deny unknown fields is §10 |
| R-10 | Throughput | No new work per tick (one field read); measured in WP2 and WP4 |

---

## 10. Out of scope and decisions recorded (Q0002)

- **The Zergling's terrain radius** under reading (b) (D-4, Q0087).
- **Per-mode terrain radii** (burrowed Lurker 0.25, D-9).
- **The waypoint handoff's body width** (D-6). It stays at `2r + margin`. Revisit only if WP3 traces a handoff defect.
- **`separation_radius` (B-49)**, which radius spaces units: unchanged and still unread.
- **Bubbles overlapping walls** (R-5): presentation, registered if it reads as a fault.
- **Making catalog definitions deny unknown fields** (R-9): a separate hardening change touching every catalog file.
- **Structures' footprints for units** (R3's "pathing footprint overrides the radius"): no shipped mobile unit has a footprint.
- **The game's placement rule for produced units** beyond D-8's terrain radius margin.

---

### Critical Files for Implementation

- crates/sim_core/src/catalog.rs
- crates/sim_core/src/catalog/validation.rs
- crates/sim_core/src/movement/mod.rs
- crates/sim_core/src/movement/locomotion.rs
- crates/sim_core/src/production/exits.rs
- assets/catalog/units.ron
- crates/sim_core/tests/catalog_reference_values_tests.rs

## Amendment 1 (orchestrator, 2026-11-18): the showcase's flip is accepted; unit–structure contact is B-131

**Decision.** WP3's data (the 17 rows of §2.2) is accepted together with its consequences for the showcase:
- **Default seed:** player 0 wins at tick 2,599. The base was player 1 at 2,218.
- **Seeds 2 to 5, 7 and 8:** player 0 wins.
- **Seed 6:** player 1 still wins, at 2,617.
- **Seed 1:** does not end within 6,000 ticks.

D-12, 4's stop applied, and this amendment is the decision it asks for. WP3 resumes from the saved patch (`~/scratch/inner-radius/wp3-uncommitted.patch`).

**Reasons.**
- **The rule is the game's.** A worker meets terrain and structures at 0.3125 (R1, the 5.0.13 notes), and the flip is a consequence of it. The trace (evidence, WP3) shows this:
  - The first divergence is tick 9: player 1's welder drone 16 rests 0.0625 nearer the south face (y = 53) of its hall's footprint, near the footprint's west corner: a structure, not terrain (corrected after review F-4; R1's "terrain / structures" covers both). It stops exactly at its terrain radius and never goes deeper (0 deeper unit-ticks on every run).
  - From tick 10, the pulse bots passing it take other paths.
  - This is the risk R-1 named, classified as rule-caused, not a defect.
- **The base outcome was fragile.** The showcase's lane fight is near-tied: plan `2026-11-11-contact-and-missiles` Amendment 1 flipped it the other way for the same class of reason. A winner that turns on a worker's 1/16 unit against a wall is not a property to preserve against the game's rule.
- **Nothing else moved materially.** The matrix keeps every recorded outcome; the one moved seed, `35_12_vikings_vs_6_colossi` seed 8, is not of record, ends at 438 instead of 435 with the same result, and is traced in WP3. Battle 33 is byte-identical.

**Seed 1 does not end.** Both armies destroy each other, and only each side's two idle welder drones remain. That is the match-end rule working as specified (a player with units left is not defeated), not a movement defect. WP3 records it. No end condition changes here. The storyboard uses the default seed.

**Seed 6 reversals at base + 12** (the bound is + 10). Accepted and recorded. Reversals at the edge of crowds are B-103's subject, and this plan changes no steering rule.

**WP3 continues as specified:**
- re-record `golden_replay` with the traced cause the WP3 report gives: the zealots turn round the cliff corner (34, 34) at 0.375 + 0.05 from tick 40;
- `golden_replay_orders` does not move, which is recorded as a finding;
- re-derive the five `wall_contact_tests` cases at the terrain radius, never loosening an assertion;
- trace battle 35, seed 8;
- run the wasm debug golden once;
- measure throughput.

WP4 must re-check the showcase storyboard and captions against the new default-seed timeline, with a real render on the RX 580, because the winner changes.

**B-131 (new register entry, out of scope here).** A registered structure is also a body: a circle of half its footprint width. Placement occupancy, the collision phase and steering obstacles meet it at the unit's collision radius (D-6). So at the middle of a structure face a ground unit stays at `Radius`; only off the face does the terrain radius apply (T-6 tests points off the face middle). The game's inner radius governs interaction with buildings (R1, R3), so unit–structure pairs should use the terrain radius. Fixing this changes the collision phase and steering for unit–structure pairs, and would move outcomes again. It gets its own short plan and measurement after this one merges, not a change folded into WP3. WP5 adds the B-131 row to `DESIGN.md` §4.

## As built (2026-11-18)

WP0 to WP5 are on branch `inner-radius`; the evidence is [`docs/evidence/2026-11-18-inner-radius.md`](../evidence/2026-11-18-inner-radius.md). After the independent review (accept, four low findings):
- **F-1:** `MotionPolicy::resolve` also refuses an air body whose terrain radius differs from its collision radius, since its domain reads the terrain radius.
- **F-2:** focused tests pin the flee goal's bounds, `clamp_into_domain`, the mobile producer's bounds, the exit ring's map-bounds and egress checks, and the overlap meter at the terrain radius; each was shown red with its path reverted to the collision radius.
- **F-3:** `wall_contact_tests` T-B1 to T-B5 and T-B9 run both a body whose terrain radius equals its collision radius and one whose terrain radius is smaller.
- **F-4:** doc lines wrapped; the showcase trace's wall is the hall's footprint, not terrain (Amendment 1's wording corrected).

## Amendment 2 (2026-11-18): B-131 fixed by plan `2026-11-18-structure-contact`

D-6's unit–structure circle, which Amendment 1 registered as B-131, is removed by plan
[`2026-11-18-structure-contact`](2026-11-18-structure-contact.md): a registered structure is no body and meets ground
units only through its footprint at their terrain radius.
