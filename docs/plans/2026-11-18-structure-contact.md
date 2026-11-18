# Blueprint: Structures Meet Units by Their Footprint (B-131)

This plan fixes one recorded bug by matching StarCraft II's rule:

- **B-131.** A registered structure is also a body, a circle whose radius is half its footprint width. Three paths meet that circle at a ground unit's collision radius (`Radius`): placement occupancy (`validate_placement_unoccupied`), the collision phase, and steering's obstacles. Production exit occupancy does the same. So at the middle of a structure face, a unit stays `Radius` from the structure. Elsewhere the footprint decides, at the terrain radius. In StarCraft II a unit meets a structure through the structure's footprint and the unit's inner radius (§2).

  Plan [`2026-11-18-inner-radius`](2026-11-18-inner-radius.md) (B-116) gave every ground body a terrain radius and wired it into every path that tests a body against the footprint cut. That plan left the circle in place (its D-6), and its WP2 found the problem (finding T-6, Amendment 1).

  This plan takes registered structures out of the body set. A structure then meets a unit only through its footprint, which is already applied at the unit's terrain radius by:
  - the navigation mesh cut;
  - the contact rule;
  - placement against terrain;
  - production egress.

A full plan is required (charter §6). The change moves the showcase battle and possibly `golden_replay`, and it spans four simulation paths and the tooling evidence.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-18 against `main` at `b71e4e50`. Line numbers drift, so search for the quoted symbol. Claims not confirmed while planning are marked **verify**. Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1. Each amendment states the decision, its reasons and what continues. An `## As built` section follows the merge, as in the precedent plan.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, D-1 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §6 |
| Q0002: scoping is the agent's call if written down | Standing rule | §10 |
| Q0087 (the Zergling's unset `InnerRadius`) | Open, untouched. The Zergling analogue keeps terrain radius = radius, so for it this plan only removes the circle | — |
| Q0083, Q0086 (GPU test paths) | Neither touches this plan | — |
| **None new** | The rule follows the game under Q0051, so nothing is the owner's to choose. The two related differences found while planning are registered as bugs (B-134, B-135; WP0), not asked | §10 |

---

## 1. Summary

| | Today | After this plan |
|---|---|---|
| Unit against a structure's **footprint** (navigation mesh cut, contact rule, routes, placement against terrain, egress, construction site) | Terrain radius `t` (B-116) | Unchanged |
| Unit against a structure's **circle** (radius `h` = footprint width ÷ 2) in collision, steering, placement occupancy and exit occupancy | Collision radius `r` | **No circle.** A registered structure is not a body |
| Unit against unit, and against immobile or unpushable *units* (sieged, rooted, channelling) | `r`, fixed weights (plan 49) | Unchanged |
| Ranges to a structure (weapon, scan, cast, projectile, splash, harvest, build, boarding) | To the structure's `collision_radius` or footprint | Unchanged (B-135 is the separate range question) |
| Drawn dome and bubble | Unchanged | Unchanged |
| Hashed state, checksum schema | — | No new hashed state. Schema stays 29 |
| Expected impact | — | Matrix, battle 33 and `golden_replay_orders` are bit-identical (no structures). `golden_replay` moves only if a unit comes within steering reach of one of its two halls. The showcase moves: units leave the spawn rings around the halls and walk past their faces |
| Size | — | M: four exclusion sites behind one flag, tests, measurement, goldens |

---

## 2. What StarCraft II does

Tags as in the precedent plan:
- **[DOC]** Blizzard or official material.
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`), layer chain `core → liberty → swarm → void → voidmulti`. `UnitData.xml` is read from the extraction under `~/scratch/combat_wp3_data/<layer>/`. `FootprintData.xml` is read online at `https://raw.githubusercontent.com/SC2Mapster/SC2GameData/5444e12f13/mods/liberty.sc2mod/base.sc2data/GameData/FootprintData.xml` (checked 2026-11-18).
- **[COMM]** Community or editor observation.
- **[INF]** An inference of this plan.

"liberty:4184" means that layer's `UnitData.xml`, line 4184. "Footprint:1993" means `FootprintData.xml`, line 1993.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | Blizzard, "StarCraft II 5.0.13 PTR Patch Notes", <https://news.blizzard.com/en-us/article/24061893/starcraft-ii-5-0-13-ptr-patch-notes>; final notes <https://news.blizzard.com/en-us/article/24078322/starcraft-ii-5-0-13-patch-notes> | "Worker inner radius (collision with terrain / structures) reduced from 0.375 to 0.3125." A unit's collision with **structures** is governed by its inner radius, not by its `Radius` [DOC] |
| S2 | SC2Mapster threads [161926 "what is radius"](https://www.sc2mapster.com/forums/development/data/161926-what-is-radius?comment=3), [175792](https://sc2mapster.com/forums/development/miscellaneous-development/175792-collision-size-of-unit), [155862](https://sc2mapster.com/forums/development/data/155862-unit-collision?comment=2) (snippets; the site now redirects) | "Inner radius" governs interaction with buildings and `Radius` governs interaction with units; units use `min(InnerRadius, Radius)` against buildings [COMM] |
| S3 | `liberty` UnitData, structures | A structure's `Radius` is **not** its footprint's half-width, and is authored independently of it. Five examples [DATA]: <ul><li>Command Center: `Radius` 2.5 (liberty:10377), `Footprint5x5Contour` (liberty:10379)</li><li>Nexus: 2 (liberty:4184), the same `Footprint5x5Contour` (liberty:4186)</li><li>Hatchery: 2 (liberty:8208), `Footprint6x5DropOffCreepSourceContour` (liberty:8210)</li><li>Gateway and Barracks: 1.75 (liberty:4394, 5838) on `Footprint3x3Contour`</li><li>Supply Depot: 1.25 (liberty:5663) on `Footprint2x2Contour`</li></ul> |
| S4 | `FootprintData.xml`, the contour footprints (Footprint:1923, 1940, 1955, 1993) | Each footprint has `Check` and `Place` cell layers and a `Shape`: an octagon given by `Offsets` and `Borders` [DATA]. Examples: <ul><li>`Footprint5x5Contour`: `Offsets -1,-2.5; -2.5,-1; -2.5,1; -1,2.5; 1,2.5; 2.5,1; 2.5,-1; 1,-2.5` (Footprint:2016). A 5 × 5 square with corners cut by 1.5</li><li>`Footprint3x3Contour` (Footprint:1936) and `Footprint2x2Contour` (Footprint:1951): corners cut by 0.5</li><li>`Footprint4x4Contour`: corners cut by 1</li></ul> |
| S5 | Liquipedia, [Walling](https://liquipedia.net/starcraft2/Walling) | A Protoss wall leaves a 1×1 gap that units pass through and a Zealot on Hold Position plugs [COMM] |
| S6 | Pathfinding and contact plans (C4, C5 there; GDC 2011 summary) | Buildings are in the navigation mesh; moving units are handled by steering and pushing [2ND, COMM] |

### 2.2 What follows for us

- **A structure meets units by its footprint, at the unit's inner radius** [DOC S1, COMM S2]. That is B-116's terrain radius, which our footprint paths already use.
- **The structure's `Radius` is not its body against units** [DATA S3, INF].
  - If it were, the Nexus (2) and the Command Center (2.5) would hold units at different distances from the same footprint.
  - The Gateway's circle (1.75 on a 3-wide footprint) would stick 0.25 out of its faces. So would the Supply Depot's (1.25 on a 2-wide footprint).
  - Two gateways one cell apart would then leave 0.5 at the face middles, which no Zealot fits (it needs 2 × 0.375). A 1×1 wall gap is a standard way for units to pass (S5).
  - The data gives structures a separate collision shape, the footprint `Shape` (S4). The `Radius` remains what the game measures ranges to (B-135, §10).
- **Units against units keep `Radius`** (S2). Immobile *units*, such as a sieged tank, are units and keep their circle.
- **Flyers do not meet structures** (`PlaneArray` Ground only on structures, liberty:10403). This is unchanged, since ours never did.
- **Not adopted here.** The contour shapes (S4) cut every shipped footprint's corners. Our footprints are full squares of cells. That is a navigation mesh change in `sim_map` and gets its own plan (B-134, §10).

---

## 3. Diagnosis

A structure is an `Entity` with a `MotionEntry`. It is immobile and in `map.structures()`. Its `collision_radius` is `footprint.0 × 0.5` (`StructureArchetypeDef::into_archetype`, `catalog.rs`). Every shipped footprint is square (`structures.ron`: 2×2, 3×3, 5×5), so the circle is the footprint's inscribed disc.

**Lemma (why the circle only matters at face middles).** Let a square footprint have half-width `h` and centre `c`. If a point `p` lies at least `t` from the square, then `|p − c| ≥ h + t`, because the inscribed disc lies inside the square. So a circle at `t` never binds where the footprint at `t` holds. The circle at `r` binds at distance `d` (with `t ≤ d < r`) from a face wherever the lateral offset from the face middle satisfies `|y| < √((h + r)² − (h + d)²)`. Examples at `d = t`:
- a worker beside a 5×5 hall: ±0.60;
- a Stalker analogue beside a 3×3: ±0.72.

This is B-131's window.

**The four paths.** Each reads the structure as a circle at `r`:

| # | Path | File, symbol | What the circle does |
|---|---|---|---|
| B1 | Collision phase | `movement/collision.rs` `resolve_collisions_and_push`, `collision_body` (`fixed` includes `map.structures().contains_key`) | A structure is a fixed body of the ground class. A unit within `h + r` of its centre takes the whole overlap as a hard correction. The structure also raises `largest[0]` to 2.5, which widens every ground pair search |
| B2 | Steering obstacles | `movement/locomotion.rs` `prepare_movement_snapshot` → `obstacles[class]`, `largest_radius`; consumers in `steering.rs` (`steer_body`, `side_blockers`, `remove_blocked_velocity`, `forms_wall`/`wall_end`/`round_wall`, `surround_side`) and `check_cluster_arrival_with` | Units bend around the circle, remove velocity into it, treat it as one end of a wall of bodies, and count a step it cuts as "crowded". Unseen enemy structures are in the list too, because the obstacle list has no vision filter |
| B3 | Placement occupancy | `movement/mod.rs` `validate_placement_unoccupied` (Blink, unload) | `Occupied(structure)` within `h + r`, at the face middle, even where `validate_placement_terrain` (footprint at `t`) admits the point. The T-6 finding |
| B4 | Production exit occupancy | `production/exits.rs` `rebuild_production_occupancy` (`max_radius` includes structures) and `free_exit_point` (`sum = radius + arch.collision_radius` over every neighbour, the producer included) | With B-116's margin `ceil(t + 0.5)`, the eight archetypes with `r > 0.5` and margin 1 have face-middle ring cells 0.5 from the face but within `h + r` of the producer's centre. Those cells are refused, contrary to the module doc ("only that producer's occupancy is waived"). The archetypes are the Marauder, Hellion, Stalker, Immortal, Ravager, Hydralisk, Queen and Infestor analogues. A 3×3 producer refuses the middle cell; a Queen analogue at a 5×5 refuses the middle three |

**Already footprint-only (no change).** Contact (`movement/contact.rs`: "true terrain, every structure, no vision") and routes (navigation mesh, funnel at `t`). Also placement against terrain (`validate_placement_terrain`, square envelope at `t`), construction sites (`stands_on`, at `t`) and egress (`egress_connected`, at `t`). Burrowed bodies (class 2) and air bodies (class 1) never met the circle; they are the existing precedent for footprint-only contact.

**Ordering facts relied on (§8).**
- `BodyGrid::for_each_near` visits buckets row by row and, within a bucket, in insertion order. Removing bodies, or shrinking the reach, therefore keeps the relative order of every remaining neighbour.
- Pair corrections are exact fixed-point sums.
- So a run in which no unit comes within reach of a structure's circle is bit-identical.

---

## 4. Design decisions

### D-1. A registered structure is not a body

Three options were weighed:

| Option | Effect | Verdict |
|---|---|---|
| A. Structures are not bodies; footprint only, at `t` | Matches S1 to S4. One rule for every structure, whatever its shape. Cheaper: fewer and smaller grid searches | **Chosen** |
| B. Structures stay circles, met at `t` | By the lemma (§3), redundant wherever the footprint holds. What remains is steering by a circle the game does not have, and a hard push out of the inscribed disc for a wedged body. It also breaks for any non-square footprint, where `w ÷ 2` exceeds the other half-width (B-134 would bring the Hatchery's 6×5) | Rejected |
| C. Keep `r` (today) | B-131 | Rejected |

**Identity.** A structure is **registered** when `map.structures().contains_key(id)`. That is plan 49's decisive foundation identity. It is not the shape or the slug, so C5 holds, and a dome is always registered while it lives (`spawn_structure`, `place_foundation`). The flag is carried as `MotionEntry::foundation: bool`, filled by `motion_entry` from the lookup it already makes (`"registered foundation cannot author self-motion…"`). No per-tick lookup is added. Placement and exits, which are not per-tick, query the map directly.

### D-2. The four paths

| # | Change |
|---|---|
| B1 | `resolve_collisions_and_push` pushes no `CollisionBody` for an entry with `foundation`, so the structure is not in `bodies`, the grid or `largest`. The foundation term of `collision_body.fixed` becomes unreachable and is removed. `fixed` stays for `is_immobile` and `unpushable`. Plan 49's invariant (a foundation never moves) now holds by exclusion and is pinned by S-3. The module's `//!` invariants gain: "A registered structure is not a body: it meets ground units through its footprint (contact rule) at their terrain radius (plan `2026-11-18-structure-contact`, B-131)" |
| B2 | `prepare_movement_snapshot` pushes no `SteeringObstacle` for a foundation and leaves `largest_radius` at the largest non-structure radius. The entry stays in `entries`: the structure is still validated and still has no motion. The `SteeringObstacle` doc says obstacles are unit bodies |
| B3 | `validate_placement_unoccupied` skips registered structures. Their footprint is judged by `validate_placement_terrain` at `t`. `Occupied` never names a structure, and its doc says so |
| B4 | `rebuild_production_occupancy` skips registered structures, so they are neither in `occupancy` nor in `max_radius`. Footprints, the producer's included, are judged by both `egress_connected` calls at `t`. The producer is exempt only on the anchor-to-exit leg, as today; other structures are never exempt. The module doc is corrected |

**Not changed:**
- the structure's `collision_radius` and every range read from it;
- harvest, build and boarding reach (`economy::within_reach` uses the footprint gap; transport sums radii as an interaction range);
- `stands_on`;
- the snapshot radius;
- `sim_map`.

The `sim_fixtures` friendly-blocking trace (`blocking.rs` `edge_gap`) keeps its circle gap. It only excuses reversals near enemies, and a unit at `t` from a face still measures as touching (`gap = t − r ≤ 0`). It is recorded, not changed.

### D-3. Steering consequences, accepted

Structures now enter steering only as terrain does: through the disc probe and slide (`probe_permits`, `slide_along_wall`, `after_wall_slide`).

- A unit no longer bends early around a building. It walks its route, which the funnel keeps `t + 0.05` off the footprint's corners, and slides along a face.
- `forms_wall` no longer sees a parked unit and a building as two ends of a wall. A gap between them narrower than the unit is handled as a gap between a unit and a cliff is today: by the probe, the slide and the waiting rule.

This is the game's model (S6) and our own for cliffs. The risk is measured (R-3), not pre-empted.

### D-4. Interaction with B-116

- T-6 (`terrain_radius_tests::a_placement_beside_a_structure_uses_the_terrain_radius`) tests points off the face middle only to avoid the circle. WP1 moves it to the face middle and removes the workaround.
- The DESIGN Movement row loses "structures as bodies still meet units at `Radius`, B-131".
- The inner-radius plan gets a pointer amendment (WP4).

### D-5. Worker mining paths (workers at their hall)

- A worker delivers when the footprint gap is at most `r + TASK_REACH` (0.375 + 0.75). Its approach goal is a ring cell centre 0.5 off a face (`approach_point`).
- Delivery never depended on the circle. While mining, a worker is a ghost in collision (`is_mineral_walking`) and no one's obstacle, but it still steered around the hall's circle as a neighbour.
- After the change it follows its route to the ring cell, with nothing bending it. Trips may get shorter by a few ticks. No income test is expected to change (**verify**: `harvest_income_tests`, `gathering_takes_the_authored_time`). Any change is re-derived from a trace, never loosened.

### D-6. Building exits

- The first candidate, the minimum corner cell, is never inside a producer's circle (for a 3×3 it is 2.83 from the centre, against `h + r ≤ 2.375`). So exits change only when earlier cells are occupied.
- Then the face-middle cells of §3 B4 become admissible for the eight archetypes. Each such cell centre is 0.5 ≥ `t` from the footprint, which egress already checks.
- An air product is unaffected: structures are ground bodies, and its occupancy test skips them by layer today.

### D-7. Flyers, burrowed bodies, structures with an air mover

- Air and burrowed classes are unchanged.
- A registered structure whose archetype has been given an `Air` mover exists only in tests (`movement_layers_tests::air_foundation_collision_preserves_registered_footprint_position`). It is now no body for anything. The test's assertion (the dome and its footprint never move) still holds, and its doc is updated.
- Lifted structures are not modelled. In StarCraft II a lifted Barracks is a separate unit without a footprint.

### D-8. Wedged bodies are not thrown out

Today, a ground body whose disc at `t` already overlaps a footprint is pushed out by the circle with a hard correction. Such a body can come only from a setup or placement error:
- scenario spawns use ring cells 0.5 off the face;
- construction refuses sites with bodies within `t` (T8);
- placement refuses points within `t` (T6).

After the change, the contact rule applies: such a body never moves deeper and may move out, as contact D-1 decided for terrain. S-8 pins this.

### D-9. Fog

Contact uses every structure without vision, as before, so an unseen structure still blocks physically. Routes still use knowledge. Removing structures from the steering obstacles removes a small leak: today a unit steers around an unseen enemy dome's circle. That is recorded as a consequence, and `cluster_arrival_tests::order_onto_a_foundation_ends_beside_it_without_revealing_it` guards knowledge.

### D-10. Determinism

- **No arithmetic changes.** No floats (C7), no new collections, no new hashed state. `MotionEntry` is not hashed. `CHECKSUM_SCHEMA_VERSION` stays 29.
- **Order is preserved.** Excluding bodies keeps the visiting order of the remaining neighbours (§3) and the exact sums of the pair corrections. So any world with no structure is bit-identical: the matrix, battle 33 and `golden_replay_orders` (`structures: []`). So is any world in which no unit comes within `r + h + STEERING_LOOKAHEAD + 2·speed` of a structure.

### D-11. Measurement

`<base>` is the tree before the WP and `<cand>` the WP's tree. Release tools are built per tree into `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` and copied aside, as in inner-radius evidence "How to reproduce" (charter §7). Large output goes to `~/scratch/structure-contact/`.

| What | Command | WP1 (placement, exits) | WP2 (collision, steering) |
|---|---|---|---|
| Matrix | `matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv` | Bit-identical | Bit-identical (no structures; **verify** the arenas spawn none) |
| Battle 33 | `matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33`; `diff` of `seeds.csv` | Byte-identical | Byte-identical |
| Showcase | `route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route`, to the end, and seeds 1 to 8 with `--seed` | `units.csv` byte-identical (no Blink, unload or production in it; **verify**) | D-12 acceptance |
| Overlap meter | `route_trace` deeper unit-ticks | 0 / 0 | 0 / 0 |
| Liveness | `route_trace` reversals and stall give-ups per owner | Unchanged | D-12, 4 bounds |
| Goldens | `golden_replay_tests` native debug and release; `--target wasm32-wasip1 --release` under wasmtime (CI's `determinism-wasm`); wasm debug once in WP2 | Pass unchanged | `golden_replay_orders` passes unchanged; `golden_replay` per D-12, 5 |
| Throughput | `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`, base and candidate alternated, three rounds | Within noise | Within noise or faster; every scenario ≥ 25 ticks/s |

### D-12. Stop rules and acceptance

The general rule is as in contact plan D-9 and inner-radius D-12: **a winner flip anywhere stops the WP.** The agent commits nothing beyond the WP and reports the trace: the first divergent tick, the entity, the structure and which path (B1 to B4) caused it. A movement that is not a flip is explained before the WP proceeds.

1. **WP1 is bit-identical** on everything in D-11. A difference means placement or exits were used somewhere they were not expected. Trace it. Accept it only if the trace shows a Blink, unload or exit at a face middle that the circle refused, and report it; otherwise it is a defect.
2. **Structure-free worlds are bit-identical in WP2:** the matrix, battle 33 and `golden_replay_orders`. Any difference is a wiring defect (D-10). Stop, find it, never re-record.
3. **The control run identifies the cause.** For every moved structure world in WP2 (showcase, `golden_replay`), the agent runs the candidate with B1 alone reverted and with B2 alone reverted (scratch builds, not committed). It reports which exclusion first moves the run.
4. **Showcase.** The base is measured on WP2's base tree. Inner-radius evidence §3 gives the expected values:
   - default seed: player 0 wins at tick 2,599;
   - seeds 2 to 5, 7 and 8: player 0 wins;
   - seed 6: player 1 wins at 2,617;
   - seed 1: no end within 6,000 ticks.

   If the base tree differs, its own run is the reference, and the evidence says so.
   - **Expected direction.** Units leaving the spawn rings, and pulse bots passing their own hall, keep up to `r − t` nearer the faces and are not bent by the 2.5 circle. The first divergence is expected within the first 20 ticks, beside a hall.
   - **Acceptance:**
     - the same winner on the default seed;
     - 0 deeper unit-ticks;
     - stall give-ups ≤ base + 1;
     - reversals ≤ base + 10 per owner;
     - the match ends on the default seed.
   - **Seeds 1 to 8** are reported with their winners and end ticks. A seed that does not end is accepted only if the reason is that of inner-radius Amendment 1 (both armies destroyed, idle workers left), shown in its summary.
   - **If the default-seed winner flips:** stop. Trace from the spawn and classify the cause as rule (a body now at `t` from a face, S1) or defect (a stall, a body wedged between a unit and a building, a refused slide). The orchestrator decides by amendment. The showcase has flipped twice before for this class of cause (contact Amendment 1, inner-radius Amendment 1); the base outcome is a near-tie, and a 0.0625 to 0.125 shift at a hall face is the game's rule, not something to tune away.
   - **If the default-seed timeline moves at all,** WP3 re-checks the storyboard.
5. **`golden_replay`** has two `foundry_core_dome` halls at (6, 6) and (52, 52).
   - **If it moves,** the re-recorded header names the first changed checkpoint and the traced cause, in the form of the existing headers: "Re-recorded for plan 2026-11-18-structure-contact WP2 (B-131): checkpoint N first changed; entity E at tick T beside the hall at cells (…), which the hall's circle steered (or pushed) at its collision radius, now walks at its terrain radius from the face". The control run (3) names the path.
   - **If it does not move,** that is a finding: no unit comes within steering reach of a hall. Record it.
   - It is re-recorded only on WP2's final tree. It must pass in debug and release native, in `wasm32-wasip1` release under wasmtime, and once in wasm debug.
6. **Test edits.** A test that changes is re-derived from a trace and stated in the commit; no bound is loosened. A change not explained by D-2 is a finding and stops the WP.
7. If acceptance is not reached after the rule is shown correct by its tests, the WP stops and reports the numbers. No tuning constant is invented.

---

## 5. File ownership

| Area | Files | WP |
|---|---|---|
| Plan row, register | `docs/plans/README.md`, `DESIGN.md` §4 (B-131 restated, B-134 and B-135 added) | WP0 |
| Placement occupancy | `crates/sim_core/src/movement/mod.rs` (`validate_placement_unoccupied`, `DisplacementAdmissionError::Occupied` doc) | WP1 |
| Exit occupancy | `crates/sim_core/src/production/exits.rs` (`rebuild_production_occupancy`, module doc, unit tests) | WP1 |
| Collision | `crates/sim_core/src/movement/collision.rs` | WP2 |
| Steering snapshot | `crates/sim_core/src/movement/locomotion.rs` (`MotionEntry::foundation`, `motion_entry`, `prepare_movement_snapshot`, unit tests), `crates/sim_core/src/steering.rs` (`SteeringObstacle` doc only) | WP2 |
| Behavioural tests | `crates/sim_core/tests/structure_contact_tests.rs` (new; `fast` by default, so `crates/gate/suites.ron` needs a rule only if it measures `slow`, C22), `crates/sim_core/tests/terrain_radius_tests.rs` (T-6 revised, WP1), `crates/sim_core/tests/movement_layers_tests.rs` (doc of the air-foundation test, WP2) | WP1, WP2 |
| Goldens | `crates/sim_core/tests/fixtures/golden_replay.ron` (only if it moves) | WP2 |
| Showcase | `assets/captures/showcase.ron` (only if the timeline moved) | WP3 |
| Documents | `DESIGN.md` (§4 Movement row, register, §5 only if figures move), this plan, `docs/plans/README.md`, `docs/evidence/2026-11-18-structure-contact.md` (new), pointer amendment in `2026-11-18-inner-radius.md` | WP0 to WP4 |

---

## 6. Work packages (commit order)

The branch is `structure-contact`. Each package writes its tests first and shows them red on its base, except WP0. Implementation agents hand back after `cargo gate quick` and the tests their change needs. WP1 and WP2 form one simulation package, which a verifier runs through `cargo gate package` together (Q0061). WP1 creates `docs/evidence/2026-11-18-structure-contact.md` with "How to reproduce" (D-11), and each WP appends to it. Sizes: S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- This plan's row in `docs/plans/README.md` (`Proposed`).
- **`DESIGN.md` B-131 restated:** "…fixed by making registered structures no bodies; they meet units through their footprint at the terrain radius (plan `2026-11-18-structure-contact`)", open until WP4.
- **New register rows**, each open and pointing to §10 of this plan:
  - **B-134.** Footprints are full squares of cells. 5.0.14's contour footprints cut their corners: `Footprint5x5Contour` by 1.5, the 4×4 by 1, the 3×3 and 2×2 by 0.5 (Footprint:1936, 1951, 1970, 2016). So units round our buildings' corners wider than the game's.
  - **B-135.** A structure's collision radius is half its footprint width, not the analogue's 5.0.14 `Radius`, so ranges to structures (weapon, scan, cast, splash) and the drawn dome measure to the wrong edge. Examples: Gateway and Barracks 1.75 on 3×3, Nexus and Hatchery 2 on 5×5 and 6×5, Supply Depot 1.25 on 2×2 (liberty:4394, 5838, 4184, 8208, 5663).
  - The WP0 agent checks each B-135 example against the analogue named by `sc2_inspiration` in `structures.ron` before writing the row, and lists only confirmed mismatches. It also records that the Hatchery analogue's footprint is 5×5 against the game's 6×5 (liberty:8210), if confirmed.
- **No question file:** Q0051 decides the rule.
- **Gate:** docs-only `package` (fmt, `workspace_checks`, C17 and C18).

### WP1: placement and exit occupancy ignore structure circles (S)

- **Tests first** (red on the base):
  - **S-2.** `terrain_radius_tests::a_placement_beside_a_structure_uses_the_terrain_radius` is revised to the **middle** of the west face. The `off` computation and its comment are removed.
    - `left − T − ε` is admitted;
    - `left − T + ε` is refused `Terrain`;
    - a second unit within `R + R` still refuses it as `Occupied(other)`.

    On the base, the first assertion fails with `Occupied(structure)`.
  - **S-5** (unit, `exits.rs`): `a_producers_circle_does_not_block_its_face_middle_exit`. A 3×3 producer (chosen by `ShapeKind::BuildingDome { footprint_w: 3, .. }`) and a scratch product with `collision_radius` 0.625 and `terrain_radius` 0.5. `free_exit_point` on the west face's middle ring cell returns that cell's centre. On the base it returns `None`. A second assertion covers occupancy, which still works for units: a holding unit of radius 0.375 placed 0.9 from that centre makes it `None`.
  - **S-9** (property, `structure_contact_tests.rs`, `proptest`, 64 cases): `no_placement_is_refused_as_occupied_by_a_structure`.
    - **Setup:** a seeded flat map; a seeded shipped structure of footprint 2, 3 or 5; a scratch body with `r ∈ {0.375, 0.5, 0.625, 0.875}` and `t` from `{r, r − 0.0625, r − 0.125, r − 0.25}` (with `t > 0`); 200 seeded points within `h + r + 1` of the centre.
    - **Assertions:** `validate_displacement_destination` never returns `Occupied(structure)`. It returns `Terrain` exactly when the square of half-side `t` around the point meets the footprint's open box. That is the envelope of `ground_footprint_connected`; **verify** the exact open or closed convention in `geometry.rs` and state it in the assertion.
    - **On the base:** it fails at the face middles.
- **Change.** D-2 B3 and B4, with their docs.
- **Measure.** D-11, WP1 column. **Stop rule** D-12, 1.
- **Expected test edits:** none outside S-2. `production_transactions_tests` pins exit cells, but its products' first corner cells are free (**verify**). Any change is traced (D-12, 6).

### WP2: collision and steering ignore structure circles (M)

- **Tests first** (`structure_contact_tests.rs` unless noted). All use small synthetic maps, the shipped catalogs, and scratch bodies registered as in `terrain_radius_tests::register`. The slim body has `R` = 0.5 and `T` = 0.3125. Structures are chosen by shape and footprint size, never by slug (C5). Every tick asserts no diagnostic.
  - **S-1** `a_unit_rests_at_its_terrain_radius_at_the_middle_of_a_structure_face`.
    - **Setup:** a 3×3 structure at cells (18, 18), west face x = 18, middle y = 19.5. The slim body walks to `(18 − T − 0.05, 19.5)`, then a hostile slim body holding at 45° pushes it into the face, as T-1 does.
    - **Assertions:** on every tick the distance to the face is at least `T − 2⁻²⁰`, and it rests at `T` ± 1e-3.
    - **On the base:** it rests at `R` (the circle's hard correction).
  - **S-3** `a_structure_neither_moves_nor_pushes_a_body_outside_its_footprint_envelope`.
    - **Setup:** an idle slim body at `(18 − 0.4, 19.5)`: outside the footprint at `T`, inside the circle at `R`.
    - **Assertions:** five ticks of `step` leave its position bit-identical. The structure's position and `PlacedStructure` are unchanged (plan 49's invariant).
    - **On the base:** it is pushed to x = 17.5.
  - **S-4** `a_unit_walks_straight_past_a_structure_face_inside_its_old_circle`.
    - **Setup:** the slim body moves from (12, 17.6) to (27, 17.6), along the south face (y = 18) at 0.4: between `T` and `R`, and clear of the funnel's corner offset `T + 0.05`.
    - **Assertions:** on every tick `|y − 17.6| < 1e-3`, and it arrives.
    - **On the base:** steering and collision bend it below y = 17.5 near x = 19.5 (centre distance 1.9 < `h + R` = 2.0).
  - **S-6** (guard) `two_units_beside_a_structure_still_separate_at_their_radii`: two slim bodies pressed together at the face middle end at centre distance ≥ `2R` less the collision phase's tolerance. Passes on the base.
  - **S-7** (guard) `an_immobile_unit_is_still_a_fixed_circle_body`. A mobile archetype's entity with `is_immobile` set (as `fixed_body_transactions_tests` does) still pushes an overlapping slim body to `R + r_immobile` and does not move itself. Passes on the base.
  - **S-8** `a_body_wedged_against_a_footprint_is_not_thrown_out_and_walks_away`.
    - **Setup:** a slim body is spawned 0.1 from the west face. If `spawn_unit` refuses it, it is spawned clear and its `position` is written in the test, as `cluster_arrival_tests` does; **verify** which.
    - **Assertions:** idle for five ticks, it does not move. Ordered west, it leaves, and its depth at `T` never rises.
    - **On the base:** it is thrown to the circle.
  - **S-10** (property, 48 cases) `a_lone_body_beside_a_structure_is_never_corrected`. A seeded structure and a seeded lone idle slim body anywhere outside the footprint's `T`-envelope and within `h + R + 0.5` of the centre: one `resolve_collisions_and_push` leaves it bit-identical. Fails on the base inside the face-middle window.
  - **Unit, `locomotion.rs`:** `a_registered_structure_is_no_steering_obstacle`. After `prepare_movement_snapshot`, `movement.obstacles[0]` holds no foundation id, and `largest_radius[0]` equals the largest unit radius.
  - **Unit, `collision.rs`:** `a_registered_structure_is_no_collision_body` (`movement.bodies` after the phase).
- **Change.** D-2 B1 and B2, the `MotionEntry::foundation` flag, and the module and type docs (D-2). The `air_foundation_collision_preserves_registered_footprint_position` doc says the structure is now no body.
- **Audit in the commit message.** The output of `grep -rn "structures()\|obstacles\[\|largest_radius\|circles_overlap\|\.collision_radius" crates/sim_core/src`, with each production hit classified as changed (B1 to B4) or unchanged (D-2's list), with the reason.
- **Measure.** D-11, WP2 column, including the control runs of D-12, 3. **Stop rules** D-12, 2 to 7.
- **Goldens:** D-12, 5.
- **Expected test edits.** Each must be re-derived and stated; none is expected (**verify** by running):
  - `movement_contact_tests::building_beside_the_goal_does_not_end_a_move` (still passes);
  - `cluster_arrival_tests::order_onto_a_foundation_ends_beside_it_without_revealing_it`;
  - `checked_movement_tests::corrupted_registered_air_foundation_never_authors_motion_or_flee`;
  - `harvest_income_tests` (D-5);
  - `construction_lifecycle_tests` builders beside sites;
  - `production_transactions_tests`;
  - `load_orders_tests` and `transport_ownership_tests` (bunker analogue unload rings, 0.5 off the face).

### WP3: client and presentation check (S)

- **No client code changes.** The client draws bubbles at `Radius` and domes at the footprint, and asks the simulation for placement. It has no copy of the body set. **Verify** with `grep -rn "SteeringObstacle\|validate_displacement" crates/client/src`.
- **Client tests** selected by the `crates/sim_core/**` rule run in `quick` and `package`. They compare checksums within a run, so none is expected to change.
- **Picture check.** One capture on the RX 580 under `with-gpu-lock` (charter §7, `showcase render --driver radv` or the lab) of a `t < r` unit at a dome's face middle. Its bubble overlaps the dome's edge by up to `r − t`: 0.0625 for workers, 0.125 for the Stalker analogue. This is recorded as an observation. If it reads as a fault, it is registered (next free number) and left out of scope, as inner-radius R-5 was.
- **Storyboard.** If WP2 moved the default-seed timeline, re-check `assets/captures/showcase.ron` with a real render, one GPU lock per take, and adjust scene ticks and captions as inner-radius evidence §4 did. `client_diagnostics/showcase_storyboard_tests` and `showcase_tests` must pass.
- **Throughput** under the gate lock. `DESIGN.md` §5 is updated only if the figures move beyond noise.

### WP4: bookkeeping (S)

- **Register:** B-131 removed as fixed, with commits. B-134 and B-135 stay open.
- **`DESIGN.md` Movement row:** "…and against each other at their radius; a registered structure is no body: it meets ground units through its footprint at their terrain radius (B-131 fixed, plan `2026-11-18-structure-contact`)", with `structure_contact_tests.rs` added to the row's tests.
- **Pointers:** a one-line amendment in `2026-11-18-inner-radius.md` (after "As built") pointing here. This plan's row becomes `Done` with commits and the evidence link.
- **Gate:** `merge` by the orchestrator.

---

## 7. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Placement | Face middle judged at `t` by the footprint; never `Occupied(structure)`; unit occupancy unchanged | S-2, S-9 (property) |
| Exits | The producer's circle no longer refuses face-middle cells; unit occupancy still does | S-5 |
| Collision | Rest at `t` at the face middle; no push inside the old circle; structures never move; wedged bodies not thrown | S-1, S-3, S-8, S-10 (property), `collision.rs` unit |
| Steering | No bend around the circle; structures are not obstacles | S-4, `locomotion.rs` unit |
| Guards | Unit–unit separation and immobile units unchanged | S-6, S-7 |
| Determinism | Structure-free worlds bit-identical; moved worlds traced with control runs; three targets | D-11, D-12 |
| Client | No code change; picture; storyboard if moved | WP3 |
| Charter | C4 module docs, C5 (no slugs; structures by shape), C7, C17, C18, C22 | `workspace_checks`, `suite_catalog_tests` |

---

## 8. Determinism summary

- **No new state.** No floats, no new collections and no new hashed state; the checksum schema stays 29. `sim_map` is untouched, so its digests (`determinism_tests`, `validation_golden_tests`) are unchanged.
- **Order and sums are kept.** Excluding bodies keeps the neighbour visiting order and the exact correction sums. Structure-free worlds are therefore bit-identical, and they are the wiring proof (D-12, 2).
- **Re-recording.** `golden_replay` is re-recorded only with a traced first divergence and a control run. It is replayed in debug and release on Linux x86_64 and on `wasm32-wasip1` (release as CI does, debug once locally). CI covers Windows and macOS ARM64 on the push.

### Throughput

- The bench scenarios spawn no structures (no `spawn_structure` in `crates/sim_core/benches`), so they are expected within noise. The flag reuses an existing lookup.
- In worlds with halls (the showcase, skirmish), ground pair and neighbour searches shrink from a reach of `r + 2.5` to `r + 1.0`, so those worlds can only get cheaper.
- Measured in WP1 and WP2 as in D-11, base and candidate alternated, three rounds under the gate lock. Every scenario must stay ≥ 25 ticks/s.

### Charter implications

- **C7:** the change is boolean selection only; no float reaches state.
- **C5:** the predicate is map registration, not identity. Tests select structures by `ShapeKind::BuildingDome` and footprint size, and products by capability. The B-135 row and any later reference table name analogues, never slugs.
- **C4 and C11:** the docs of `collision.rs`, `exits.rs` and `movement/mod.rs` are updated. The PERFORMANCE RATIONALE block in `resolve_collisions_and_push` keeps its text, and gains no new block.
- **C12:** no `let _ =`.
- **C22:** the new test file is `fast` unless measured slow.

---

## 9. Risks

| # | Risk | Mitigation |
|---|---|---|
| R-1 | The showcase's near-tied lane fight flips again | D-12, 4: stop, trace from the spawn, control runs, orchestrator amendment; S1 is the cited reason for a rule-caused flip; WP3 re-checks the storyboard |
| R-2 | `golden_replay` moves | Traced header with first checkpoint, entity, hall and path; `golden_replay_orders` must not move (no structures) |
| R-3 | Without structures in `forms_wall`, units step into gaps between a parked unit and a building that they cannot pass, and wait or reverse | Same model as cliffs (D-3). Liveness bounds of D-12, 4. If exceeded, the WP stops with the trace. A disc test in `forms_wall` against footprints is a candidate fix and needs an amendment, not an ad hoc patch |
| R-4 | A hidden dependence on structures as obstacles (attack approach to a structure, cluster arrival, surround) | Structures were never arrival witnesses (`settled_on` is `None`) and an attacked structure is already `ignored`. The WP2 audit grep; the expected-test list runs in `package` |
| R-5 | Plan 49's invariant (a foundation never moves) is lost | Excluded bodies cannot be corrected; S-3 and `air_foundation_collision_preserves_registered_footprint_position` pin it |
| R-6 | A wedged body is no longer ejected | D-8; S-8; no shipped path creates one |
| R-7 | Bubbles overlap dome edges by up to `r − t` at face middles | WP3 picture; register only if it reads as a fault |
| R-8 | Exit cells move for the eight archetypes when earlier cells are occupied | D-6; S-5; skirmish openings are not pinned |
| R-9 | Melee reach to a structure changes | At a face middle a body at `t` is `h + t` from the centre, within `h + r + range`. At corners nothing changes (the footprint already decided). Ranges are B-135's subject |
| R-10 | Throughput | Fewer and smaller searches; measured |

---

## 10. Out of scope and decisions recorded (Q0002)

- **B-134, contour footprints** (S4): chamfered corners need a navigation mesh cut and placement layers that differ (`Place` cells against `Shape`), in `sim_map`. Separate plan.
- **B-135, structure `Radius`** (S3): ranges to structures and the drawn dome. This needs a catalog field and reference values. It moves combat, so it gets its own plan.
- **Non-square footprints** (the Hatchery's 6×5): with B-134 and B-135.
- **Lifted structures** (flying Barracks and others): not modelled.
- **`separation_radius` (B-49):** unchanged and still unread.
- **The friendly-blocking trace's circle gap for structures** (`sim_fixtures` `blocking.rs`): tooling only, recorded in D-2.
- **A footprint-aware wall test in steering** (R-3): only if measurement demands it, by amendment.

---

## Proposed Amendment 1 (implementation agent, 2026-11-18; for the orchestrator to decide): accept the showcase's flip under WP2

**Not in force until the orchestrator adopts it.** WP0 to WP2 are committed on `structure-contact`; WP2 is held
under D-12, 4, and WP3 and WP4 are not started.

**What happened.** WP1 is bit-identical everywhere. WP2 keeps every structure-free world bit-identical: the matrix,
battle 33 and `golden_replay_orders`. `golden_replay` does not move. The showcase's default seed flips, from player
0 at 2,599 to player 1 at 2,193. The other acceptance items on that seed hold: 0 deeper unit-ticks, stall give-ups
0 / 0, reversals 25 / 10 against 24 / 16, and the match ends. Seven of nine seeds now end for player 1. Seed 1 now
ends. Seed 6's player 1 reversals (+13) and seed 8's player 0 reversals (+24) exceed base + 10. Evidence §2 has the
table.

**Cause (traced, control runs).** With B2 reverted, the run is byte-identical to WP1. With B1 reverted, it is
identical to WP2 until tick 979, and the outcome is the same. So the steering exclusion moves the run.
- At tick 21, player 1's rhino beetle 26 walks round its own hall's north face, 0.53 from it, toward its corner
  waypoint.
- The hall's 2.5 circle is within steering reach but not contact (3.21 from the centre). On the base it bent the
  rhino north. On WP2 the rhino keeps its route.
- The showcase spawns both armies on the south and east rings of their halls. Only player 1's east-ring units walk
  past their hall's faces toward the enemy, so only player 1's lane changes at first.

This is D-3's accepted consequence and the game's model (S1, S6). It is not a defect: no wedge, no stall at the
divergence, no refused slide.

**Proposed decision.**
1. Accept the flip as rule-caused, as contact Amendment 1 and inner-radius Amendment 1 did. Also accept the two
   reversal excesses on seeds 6 and 8, which are not the default seed. Or have them traced first, if the
   orchestrator prefers.
2. WP2 continues to its acceptance: `golden_replay` needs no re-recording.
3. WP3 re-checks the storyboard (`assets/captures/showcase.ron`) for player 1's win at 2,193, with real renders on
   the RX 580. It also takes the picture check.
4. WP4 as planned.

No tuning constant is proposed (D-12, 7).
