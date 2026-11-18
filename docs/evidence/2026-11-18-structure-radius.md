# Evidence: Ranges to a Structure Measure to Its Engine Radius (B-135)

Measurements and test results of plan [`2026-11-18-structure-radius`](../plans/2026-11-18-structure-radius.md),
one section per work package.

## How to reproduce

Each tree builds its release tools into its own target directory and copies them aside, so later builds cannot
change an earlier step's binaries (charter §7: hot builds on the home disk, large output on `~/scratch`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/structure-radius
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} <bin>/
```

Then, from the repository root (`<dir>` under `~/scratch/structure-radius/`):

```bash
<bin>/matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv
<bin>/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 6000 --out <dir>/route-end
for s in 1 2 3 4 5 6 7 8; do
  <bin>/route_trace --scenario scenarios/showcase.ron --seed $s --ticks 6000 --out <dir>/route-s$s
done
diff <base>/b33/33_24_marines_vs_12_zealots/seeds.csv <dir>/b33/33_24_marines_vs_12_zealots/seeds.csv
for d in route route-end route-s1 ... route-s8; do cmp <base>/$d/units.csv <dir>/$d/units.csv; done
```

Goldens: `cargo test -p sim_core --test golden_replay_tests` in debug and with `--release`, and
`cargo test -p sim_core --test golden_replay_tests --target wasm32-wasip1 --release` under wasmtime (CI's
`determinism-wasm` command). Throughput: `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`,
the trees' bench binaries alternated, three rounds.

**Sources (§2.3).** The 5.0.14 data were read as data, each download in its own directory under
`~/scratch/structure-radius/`:

- `dl-techtree/data.json`: `https://raw.githubusercontent.com/BurnySc2/sc2-techtree/c2c90e9b56/data/data.json`
  (S3, the live `radius` of each unit);
- `dl-footprint/FootprintData.xml`:
  `https://raw.githubusercontent.com/SC2Mapster/SC2GameData/5444e12f13/mods/liberty.sc2mod/base.sc2data/GameData/FootprintData.xml`
  (S2, each footprint's `<Shape><Radius>`);
- `~/scratch/combat_wp3_data/liberty/UnitData.xml` (S1, each structure's `Radius` and `Footprint`).

`~/scratch/structure-radius/scripts/check_radius.py <UnitData.xml> <FootprintData.xml> <data.json>` prints, for
every analogue, `Radius` and its line, `Footprint` and its line, the footprint's `Shape` radius and its line,
`E = max(Radius, ⌈16·Shape⌉/16)` and S3's live value.

The base of WP1 is the plan's tree with WP0 (docs only on `a9cf4804`, which files the plan on `main` `c06d8f5c`).

## 0. WP0: records

**Verified (§2.3, all 49 rows).** Every analogue named by `sc2_inspiration` in `assets/catalog/structures.ron`
(49 structures; footprints 5×5, 3×3 and 2×2 as the plan's "Ours" column) was checked with `check_radius.py`:
its liberty `Radius` and line, its `Footprint` and line, the footprint's `Shape` radius and line, and S3's live
radius. For all 49, `E = max(Radius, ⌈16·Shape.Radius⌉/16)` equals S3's live value, and every value and line number
equals the plan's table (the Lurker Den analogue reads `LurkerDenMP`, liberty:676; the Templar Archives analogue
reads `TemplarArchive`). No amendment is needed.

| Analogue group | `Radius` (liberty) | Footprint, `Shape` radius | E | S3 live |
|---|---|---|---:|---:|
| Command Center, Orbital Command, Planetary Fortress | 2.5 (10377, 10523, 10661) | 5x5Contour 2.6926 (Footprint:2015) | 2.75 | 2.75 |
| Nexus | 2 (4184) | 5x5Contour 2.6926 | 2.75 | 2.75 |
| Hatchery, Lair, Hive | 2 (8208, 8290, 8370) | 6x5DropOffCreepSourceContour 2.6926 (:2261) | 2.75 | 2.75 |
| 17 Terran and Protoss 3×3 buildings | 1.25 to 1.75 | 3x3Contour 1.803 (:1935) | 1.8125 | 1.8125 |
| 9 Zerg 3×3 buildings | 1.5 | 3x3CreepContour 1.803 (:2205) | 1.8125 | 1.8125 |
| Refinery, Assimilator, Extractor | 1.5 (5776, 4122, 8149) | GeyserRoundedBuilt 1.6772 (:959) | 1.6875 | 1.6875 |
| Supply Depot | 1.25 (5663) | 2x2Contour 1.1181 (:1950) | 1.25 | 1.25 |
| Missile Turret, Tech Lab, Reactor, Pylon, Photon Cannon, Shield Battery | 0.75 to 1 | 2x2Contour 1.1181 | 1.125 | 1.125 |
| Dark Shrine | 1.5 (9510) | 2x2Contour 1.1181 | 1.5 | 1.5 |
| Spine Crawler | 0.875 (8615) | 2x2ZergSpineCrawler 1.1181 (:5369) | 1.125 | 1.125 |
| Spore Crawler | 0.875 (8749) | 2x2Contour2 0.559 (:162) | 0.875 | 0.875 |
| Sensor Tower | 0.75 (9452) | 1x1 0.7072 (:1106) | 0.75 | 0.75 |
| Spire, Greater Spire | 1 (9011, 9082) | 2x2CreepContour, 2x2IgnoreCreepContour 1.1181 (:2073, :1238) | 1.125 | 1.125 |

**Records.** `DESIGN.md` §4: B-135 restated with the engine rule and pointed to this plan (the dome sentence and
"Command Center analogues match at 2.5" withdrawn, D-6 and §2.3); B-138 added for the footprint mismatches (B-137
is taken by another branch). The plan's row in `docs/plans/README.md` is `Proposed`.

## 1. WP1: the field and the loader, data unchanged

**Change.** `StructureArchetypeDef::radius` (required, `f64`, world units) is read into `collision_radius` and
`separation_radius`; the structure's `terrain_radius` is half the footprint's smaller side (D-5). `validate_radius`
refuses a value that is not finite, not strictly positive, not a multiple of 1/16, or below half the footprint's
smaller side, each as `CatalogInvalidValue` naming `radius`. Docs of `UnitArchetype::collision_radius` and
`terrain_radius` say what they mean for a structure. All 49 rows of `structures.ron` carry
`radius: <footprint.0 ÷ 2>` with the plan's comment.

**Tests, red on WP0 first** (WP0 tree with WP1's tests; the field is ignored there):

| Test | WP0 | WP1 |
|---|---|---|
| `a_structure_radius_is_read_into_the_collision_radius` | Fails: collision radius 1.5, expected 1.8125 | Passes |
| `a_structures_terrain_radius_is_half_its_footprints_smaller_side` | Fails: collision radius 2.5, expected 2.75 | Passes (terrain radius 2.5) |
| `a_structure_radius_is_required` | Fails: the row without `radius` loads | Passes: `RonDeserializationError` naming `radius` |
| `a_structure_radius_must_be_positive_and_finite` (0, −0.5, NaN, inf) | Fails: loaded | Passes |
| `a_structure_radius_must_be_a_multiple_of_a_sixteenth` (1.8 refused, 1.8125 admitted) | Fails: loaded | Passes |
| `a_structure_radius_may_not_be_below_half_the_footprint` (1.4375 on 3×3, 0.75 on 2×2 refused; 1.5 admitted) | Fails: loaded | Passes |
| R-12 `structure_radius_tests::a_structure_flush_with_the_map_edge_keeps_a_valid_motion_entry` | Fails: collision radius 2.5, expected 2.75 | Passes |

**R-12 is written in WP1** (the plan's order is kept: WP1 sets D-5). **Shown red against the hazard:** on the WP1
tree with `terrain_radius: radius` restored in `into_archetype` and the test's terrain-radius assertion removed,
the first tick reports `MovementInvariant { tick: Tick(1), source: InvalidState { entity: EntityId(1), reason:
"actor is outside its current terrain-radius movement domain" } }` for the 5×5 at cells (0, 0). With D-5 it passes.

**Verified (inline structure RON in tests).** `grep -rn "footprint: (" crates --include=*.rs`: every hit is a map
start location (`MapStartLocation`, map documents, the editor) or the catalog's own field; no test carries an inline
structure catalog. Tests that edit structures read the shipped file (`weapon_catalog_rules_tests`,
`faction_catalog_tests`, `catalog_serialization_compatibility`), and pass unchanged, as do `runtime_catalog_tests`
(`sim_core` 31, `asset_baker` 2) and `catalog_reference_values_tests` (14).

**Bit-identity** (D-11, 1), WP0 tools against WP1 tools:

| What | Result |
|---|---|
| Matrix (`--compare` WP0) | Bit-identical (1,160 runs; expectation failures 0 on the seeds of record, 4 over all seeds) |
| Battle 33 `seeds.csv` | Byte-identical |
| Showcase `units.csv`, `summary.md`, `overlap.csv`: 1,800 ticks, to the end, seeds 1 to 8 | Byte-identical |
| Goldens | Pass unchanged in debug and release native and in `wasm32-wasip1` release under wasmtime |

The base reproduces structure-contact Amendment 1: the default seed ends with player 1 winning at tick 2,193
(reversals 25 / 10, stall give-ups 0 / 0, deeper unit-ticks 0 / 0); seeds 1 to 6 and 8 player 1 (at 2,203, 2,187,
1,880, 5,688, 1,593, 3,419 and 1,716), seed 7 player 0 at 2,554.

## 2. WP2: simulation readers and the presentation split

**Change.** `UnitArchetype::drawn_radius()`: half the footprint's smaller side for a dome, the collision radius for a
unit. Read by the render snapshot's `radius` (P1), the projectile visual's spawn and arrival points (A3v, through
`SimWorld::drawn_radii`, looked up as the flight's radii are) and the death shatter (P2,
`prepare_reap_with_resources`). No range reader changed.

**Tests, red on WP1 first** (WP1 tree with WP2's tests; `drawn_radius` absent, the three sites reading
`collision_radius`):

| Test | WP1 | WP2 |
|---|---|---|
| R-13 `world::snapshot::tests::a_dome_is_drawn_at_its_footprint_not_its_radius` | Fails: the 3×3 at radius 1.8125 publishes 1.8125 | Passes: 1.5; a unit publishes its collision radius |
| R-14 `combat_resolution::tests::a_projectile_visual_to_a_dome_arrives_on_its_drawn_dome` | Fails: arrival (19.6875, 21.5), 1.8125 from the centre | Passes: arrival (20.0, 21.5); published flight ticks follow radius 1.8125 |
| R-15 `transport::lifecycle::tests::a_dying_structure_shatters_at_its_drawn_radius` | Fails: shatter radius 1.8125 | Passes: 1.5 |
| R-16 `catalog::tests::drawn_radius_is_half_the_footprints_smaller_side_for_a_dome_and_the_radius_for_a_unit` | Does not compile (no `drawn_radius`) | Passes, including an oblong 6×5 dome (2.5) |

**Guards** (pass on WP1 and WP2; each asserts a control at half the footprint, every integration tick asserts no
diagnostic): `structure_radius_tests` R-1 weapon reach, R-2 scan, R-3 an armed 2×2's own reach, R-4 melee along a
face, R-5 every shipped face in every shipped melee reach, R-6 projectile flight, R-7 splash, R-8 boarding, R-9
Charge window; R-10 `world::casts::tests::a_cast_on_a_structure_is_in_range_at_its_radius`; R-11
`effects::step::tests::a_missile_stops_short_of_a_structure_by_its_radius`.

**Verified and recorded while writing the guards** (each a test-design finding, no production change):

- **The structures in the tests are scratch copies of shipped structures** chosen by footprint (and weapon) and loaded
  through `load_structures_from_ron_str` with `radius` replaced, so they pass the loader (R-1 to R-9, R-12). The unit
  tests (R-10, R-11, R-13 to R-15) set the archetype's `collision_radius` in memory.
- **R-4.** An attack-ordered melee unit walking in never touches the face: its pursuit heads for the centre and stops
  at its band (damage within 28 to 31 ticks from 6 west of the face at either radius). The plan's control (±1.4 fails
  at radius 1.5) does not hold for that setup, so R-4 starts the body touching the face (centre at its terrain
  radius, 0.375). With the shortest shipped melee weapon (range 0.1): at radius 1.8125 it swings where it stands at
  offsets 0, ±1.0, ±1.4 (reach on the face to ±1.517) and deals damage within 100 ticks; at radius 1.5 (reach to
  ±0.946) it swings where it stands only from the middle and moves first from ±1.0 and ±1.4.
- **R-6, A3 is presentation.** `ticks_to_impact` (`⌈max(d − R − r, 0.05) ÷ speed⌉`) only sets the published
  `duration_secs` of the visual. The simulated projectile advances its speed a step from the shooter's centre and
  lands once within its speed of the target radius (A6): on the `⌈(d − R) ÷ speed⌉`-th step counting the launch step,
  which also advances it once. R-6 pins both at radii 1.8125 and 1.5 (published 4, in flight 3 at 1.8125).
- **R-7** holds its primary target in place (an unheld target is moved by its retaliation, which shifted the search
  point).
- **R-8.** No shipped structure carries (`into_archetype` sets `transport_capacity: 0`), so the carrier is a scratch
  3×3 with capacity 4. Out of boarding reach the `Load` order does not walk: `pursue_carrier` aims at the carrier's
  centre cell, which a structure's footprint covers, and the order ends as unreachable. R-8 therefore pins only that a
  passenger is aboard on the order's tick from `R + r + 1 − 2⁻¹⁶` and not from `+ 2⁻¹⁶`. Recorded as an open item
  (no shipped content reaches it).
- **R-10, R-11 (verify).** The only shipped unit-target effect ability (Abduct analogue) excludes structures
  (`structures: Exclude`), so R-10 asks `cast_in_range` of a copy of it without its filter; R-11 flies the first
  shipped homing missile node at a structure.

**Audit** (`grep -rn "collision_radius\|policy.radius\|\.radius\b" crates/sim_core/src crates/sim_fixtures/src`,
production hits): range readers A1 `target_acquisition.rs` 415 (largest-radius bound), 527 (`reaches_from_here`),
658 (scan query), 690 (scan qualification); A2 `combat_resolution.rs` 413 (`capture_target_snapshots`), 515
(`combined_radius`); A3 742, 748 (flight ticks, presentation duration); A4 854 (`ResolvedHit.radius`), 954 (splash
offset); A5 1037 (`strike_area`); A6 1254, 1326 (projectile tracking, retarget); A7 `effects/step.rs` 148; A8
`world/casts.rs` 180; A9 `world/attack_dash.rs` 111; A10 `movement/mod.rs` 277 (`MotionPolicy::resolve`) and
`movement/locomotion.rs` 1173 (pursuit band); A11 `transport/orders.rs` 162. Domain A12: `movement/mod.rs` 277 with
`terrain_radius` (D-5). Presentation: `world/snapshot.rs` (`drawn_radius`), `combat_resolution.rs`
`drawn_radii`, `production/lifecycle.rs` (`drawn_radius`), `combat_resolution.rs` 243 (a splash zone's radius, not
an entity's). Bodies and units only (U2 and B-131): `movement/collision.rs`, `steering.rs`, `movement/locomotion.rs`
404 to 2306 (obstacles, waypoints, bodies; no foundation is a body or obstacle), `movement/mod.rs` 607 (placement
occupancy, structures skipped), `production/mod.rs` 373 and `production/exits.rs` (products and exit ring bodies),
`economy/mod.rs` 494 (the worker's own radius in `within_reach`, U1). Positive-radius invariants:
`production/lifecycle.rs` 308, 420, `production/mod.rs` 346, `production/exits.rs` 34, 96. Catalog and validation:
`catalog.rs`, `catalog/validation.rs`, `catalog/spells/flatten.rs` (a search's radius). Tooling U3:
`sim_fixtures/src/trace/blocking.rs` 238, `trace/mod.rs` 702 (follow the field, D-8).

**Bit-identity** (D-11, 1), WP1 tools against WP2 tools: matrix bit-identical (1,160 runs; expectation failures 0
on the seeds of record, 4 over all seeds); battle 33 `seeds.csv` byte-identical; showcase `units.csv`, `summary.md`
and `overlap.csv` byte-identical at 1,800 ticks, to the end and on seeds 1 to 8.
The whole `sim_core` suite passes on WP2 (949 tests, every target; no test changed beyond the new ones). Goldens pass
unchanged in debug and release native and in `wasm32-wasip1` release under wasmtime.

## 3. WP3: client

**Change.** `UnitEffectTables`' muzzle is `drawn_radius()`; the docs of `projectile_onto_struck`, `StruckBubble::radius`
and the muzzle field say the simulation publishes the arrival at the target's drawn radius (D-6). **Verified:**
`grep -rn "collision_radius" crates/client/src` finds no other reader (only the new test).

| Test | WP2 | WP3 |
|---|---|---|
| `effects::catalog::units::tests::a_structures_muzzle_is_its_drawn_dome` | Fails: an armed 2×2 at radius 1.125 has muzzle 1.125 | Passes: 1.0; a unit's muzzle is its collision radius |

The client's `struck` unit tests and `sync_steady_state_tests` (8) pass; `cargo clippy -p client --lib --tests` is
clean. The simulation is unchanged from WP2, so its matrix, battle 33, showcase and goldens are WP2's.

## 4. WP4: the 5.0.14 values

**Change.** The 49 `radius` values of §2.3 in `structures.ron`, each with its source comment; the four floored rows
(Sensor Tower, Spore Crawler, Spire, Greater Spire analogues) say "floor: half the footprint (B-138)" with the game's
value. Values per analogue: 2.75 (Command Center, Orbital Command, Planetary Fortress, Nexus, Hatchery, Lair, Hive),
1.8125 (26 contour 3×3 buildings), 1.6875 (three geysers), 1.25 (Supply Depot), 1.125 (Missile Turret, Tech Lab,
Reactor, Pylon, Photon Cannon, Shield Battery, Spine Crawler), 1.5 (Dark Shrine), 1 (Spore Crawler, Sensor Tower:
floor), 1.5 (Spire, Greater Spire: floor).

**Tests, red on WP3 first** (WP3 tree with WP4's tests):

| Test | WP3 | WP4 |
|---|---|---|
| `catalog_reference_values_tests::b135_structure_radii_follow_starcraft` (49 rows) | Fails: Command Center analogue 2.5, expected 2.75 | Passes |
| `catalog_reference_values_tests::structure_radii_follow_the_engine_rule` (49 rows `[Radius, Shape, E]`; `E` = rule; authored = `max(E, floor)`) | Fails: Command Center analogue 2.5, expected `max(2.75, 2.5)` | Passes |

**Test edit (one), explained by §3 A12 / D-5.** `catalog_reference_values_tests::only_ground_units_with_a_smaller_inner_radius_author_one`
(B-116) failed on the WP4 data: "foundry_core_dome (Command Center): unsourced terrain radius". Its loop took every
archetype whose terrain radius is below its collision radius to be an authored unit row; since WP1 a structure's
terrain radius is half its footprint (D-5), below its radius wherever the engine radius exceeds the half-width (45
structures from WP4). The test now checks each structure's terrain radius equals half its footprint's smaller side
(a stricter check, not a looser one) and keeps the unit rule and its count of 17 unchanged.

**Structure-free worlds** (D-11, 2), WP2 tools against WP4 tools: matrix bit-identical (1,160 runs; expectation
failures 0 on the seeds of record, 4 over all seeds); battle 33 `seeds.csv` byte-identical; `golden_replay_orders`
passes unchanged. The catalog's largest radius rises from 2.5 to 2.75 (D-9) and changes nothing there.

**`golden_replay` does not move** (D-11, 5, a finding): it passes unchanged in debug and release native, in
`wasm32-wasip1` release under wasmtime and once in `wasm32-wasip1` debug. No unit of its scenario scans, aims at,
chases or shoots a hall within the 0.25 the CC analogue's radius gained, in a way that changes a checkpoint. It is
not re-recorded.

**Showcase** (`route_trace` to the end of the match; reversals and stall give-ups per owner P0 / P1; deeper
unit-ticks from the overlap meter). Base: the WP2 tree's own run (byte-identical to WP0, which reproduces
structure-contact Amendment 1).

| Seed | Base | WP4 | Reversals | Stall give-ups | Deeper | First divergence (`units.csv` tick) |
|---|---|---|---|---|---|---|
| Default | P1 at 2,193 | P1 at 2,191 | 25 / 10 → 25 / 10 | 0 / 0 → 0 / 0 | 0 / 0 | 939, P1 pulse bot 23 |
| 1 | P1 at 2,203 | P1 at 2,200 | 23 / 10 → 23 / 10 | 0 / 0 → 0 / 0 | 0 / 0 | 957, P1 pulse bot 20 |
| 2 | P1 at 2,187 | P1 at 2,185 | 22 / 12 → 22 / 12 | 0 / 0 → 0 / 0 | 0 / 0 | 944, P1 pulse bot 23 |
| 3 | P1 at 1,880 | P1 at 1,886 | 22 / 15 → 22 / 15 | 1 / 0 → 0 / 0 | 0 / 0 | 925, P1 pulse bot 23 |
| 4 | P1 at 5,688 | P1 at 5,700 | 11 / 11 → 11 / 11 | 0 / 0 → 0 / 0 | 0 / 0 | 991, P1 pulse bot 20 |
| 5 | P1 at 1,593 | P1 at 1,580 | 22 / 20 → 22 / 20 | 1 / 0 → 0 / 0 | 0 / 0 | 895, P1 rhino beetle 26 |
| 6 | P1 at 3,419 | P1 at 3,419 | 19 / 23 → 19 / 23 | 0 / 0 → 0 / 0 | 0 / 0 | 964, P1 pulse bot 23 |
| 7 | P0 at 2,554 | P0 at 2,552 | 11 / 10 → 11 / 10 | 0 / 0 → 0 / 0 | 0 / 0 | 1,007, P0 pulse bot 4 |
| 8 | P1 at 1,716 | P1 at 1,712 | 40 / 14 → 40 / 14 | 0 / 0 → 0 / 0 | 0 / 0 | 907, P1 rhino beetle 25 |

**No winner flips; acceptance (D-11, 4) holds** on the default seed: same winner (player 1), 0 deeper unit-ticks,
stall give-ups 0 / 0 (bound base + 1), reversals unchanged (bound base + 10), the match ends. The 1,800-tick run
differs from tick 939 on, as the run to the end does.

**Trace of the first divergence (default seed).** The lane fight runs from tick 348 to 634 (unit counts in
`units.csv`); by tick 801 player 0 has 2 units left and player 1 six, marching on player 0's hall (Command Center
analogue at cells 6 to 10, centre (8.5, 8.5)). At `units.csv` tick 938 player 1's pulse bot 23 (radius 0.375, scan
5.5) stands at (16.5005, 11.5281), 8.554 from the hall's centre: beyond the old scan reach 5.5 + 0.375 + 2.5 = 8.375,
within the new 5.5 + 0.375 + 2.75 = 8.625. It acquires the hall (reader **A1**, the scan qualification of
`scan_candidates`), drops its route on tick 939 instead of 941, and the hall first loses vitals two ticks earlier
(944 instead of 946, counted in steps). Every seed is the same: the first divergence is the first tick a unit
acquires an enemy hall, after the armies met, two ticks earlier than on the base.

**Control run** (D-11, 3): the WP4 tools on a scratch copy of `assets/` with only the Command Center row reverted to
2.5 (`route_trace --assets ~/scratch/structure-radius/assets-ctl`): `units.csv` byte-identical to the base on the
default seed and seeds 1 to 8. The CC analogue's radius alone moves the showcase; the other 44 changed rows and the
wider query bound move nothing. (Note: `route_trace` reads the catalogs from the assets directory at run time, so a
control needs `--assets`, not a rebuilt binary.)

**Seeds 3 and 4 end later (6 and 12 ticks), explained.** The plan expected every hall kill to come earlier or at the
same tick. A scratch trace (`~/scratch/structure-radius/scripts/scratch_hall_trace.rs`, a temporary example not
committed) reports per run the step a unit first targets an enemy hall, the step the hall first loses vitals and
the end, with and without the weapons' random firing delays (`sim_fixtures::without_random_delays`):

| Seed | Targeted, 2.5 → 2.75 | First damage, 2.5 → 2.75 | End, 2.5 → 2.75 | End without random delays, 2.5 → 2.75 |
|---|---|---|---|---|
| Default | 942 → 940 | 946 → 944 | 2,193 → 2,191 | 2,130 → 2,129 |
| 1 | 960 → 958 | 964 → 962 | 2,203 → 2,200 | 2,130 → 2,129 |
| 2 | 947 → 945 | 951 → 949 | 2,187 → 2,185 | 2,130 → 2,129 |
| 3 | 928 → 926 | 932 → 930 | 1,880 → 1,886 | 2,130 → 2,129 |
| 4 | 994 → 992 | 998 → 996 | 5,688 → 5,700 | 2,130 → 2,129 |
| 5 | 898 → 896 | 906 → 904 | 1,593 → 1,580 | 2,130 → 2,129 |
| 6 | 967 → 965 | 971 → 969 | 3,419 → 3,419 | 2,130 → 2,129 |
| 7 | 1,010 → 1,008 | 1,014 → 1,012 | 2,554 → 2,552 | 2,130 → 2,129 |
| 8 | 910 → 908 | 918 → 916 | 1,716 → 1,712 | 2,130 → 2,129 |

On every seed the hall is targeted and first damaged two steps earlier. Without random delays the seed no longer
matters, and the hall falls one step sooner. The random delay of each weapon release is drawn from the world's one
seeded RNG in release order; an engagement two ticks earlier shifts which release draws which delay, so the hall's
remaining life is dealt over a reshuffled sequence of delays, and the end moves by a few ticks either way (−13 to
+12). The rule's own effect is earlier or equal; the later ends are this reshuffle, not a defect (no pursuit fails
its band, no unit oscillates at a face, no hit lands from beyond `E + r + range`: reversals and give-ups do not rise).

The whole `sim_core` suite passes on WP4 (951 tests) with the one test edit above; none of the plan's "expected test
edits" (`combat_correctness_tests`, `target_selection_tests`, `attack_dash_tests`, `load_orders_tests`,
`transport_ownership_tests`, `full_match_tests`, `match_result_tests`, `effect_snapshot_tests`) needed a change.

## 5. WP5: presentation check, storyboard and throughput

**Picture check and storyboard render: not done in this session (pending).** The scratch picture storyboard
`~/scratch/structure-radius/wp5/picture.ron` (player 1's pulse bots firing projectiles at player 0's hall, the
showcase's only domes: a 5×5 Command Center analogue whose `E − h`, 0.25, is the largest of any shipped dome) and the
re-check render of `assets/captures/showcase.ron` were prepared, and `showcase` was built
(`cargo build --release -p client_diagnostics --bin showcase`). Both `showcase render --driver radv` runs (which take
`with-gpu-lock` per take) were refused by this session's permission policy, so no frame was rendered. The
orchestrator, or a session allowed to use the GPU, runs:

```bash
WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json \
  "$CARGO_TARGET_DIR"/release/showcase render --driver radv \
  --storyboard ~/scratch/structure-radius/wp5/picture.ron --out ~/scratch/structure-radius/wp5/picture
# at least 60 s later:
WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json \
  "$CARGO_TARGET_DIR"/release/showcase render --driver radv --out ~/scratch/structure-radius/wp5/show1
```

Scoping (Q0002): the showcase holds no Photon Cannon analogue and no 3×3 dome, so the plan's cannon shot and 3×3 dome
cannot appear in it; the hall is the strongest case of D-6 (projectiles drawn to 2.5 while flying for the 2.75 edge).
A melee unit at a dome face is not in the showcase's hall siege either (the survivors are pulse bots and a rhino
beetle, all ranged). R-13, R-14 and the WP3 test pin the drawn ends in code.

**Storyboard text.** The default-seed timeline moved by a few ticks after the lane fight (§4): the enemy's
survivors acquire the viewer's hall two ticks sooner, kill its workers at ticks 1,160 and 1,200 (1,171 and 1,216
before) and the hall falls at 2,191 (2,193). The storyboard's header comment says so; no scene length changed. The
scene ticks of a render are to be recorded here once rendered: `march` ends on combat at the viewer's base, which can
now come up to two ticks sooner (half a frame at 4x), and `assault` (ticks from about 949 for 400 ticks at 2x) still
covers both worker deaths.

**Throughput** (D-10): the `throughput` bench binaries of WP0 (`7b718151`, built in a temporary worktree removed
afterwards) and WP4 (`b4f1269b`), alternated, three rounds, under the gate lock; ticks per second, each the bench's
median of three repeats; the host was shared:

| Scenario | WP0 | WP4 |
|---|---|---|
| 64×64, 0 units | 3,278 / 2,642 / 3,264 | 3,256 / 3,280 / 3,266 |
| 64×64, 100 | 961 / 667 / 953 | 885 / 954 / 951 |
| 64×64, 400 | 165 / 112 / 164 | 162 / 163 / 162 |
| 128×128, 400 | 189 / 137 / 194 | 195 / 193 / 198 |
| 256×256, 0 | 201 / 204 / 205 | 204 / 205 / 203 |
| showcase canyon, 400 | 118 / 117 / 122 | 115 / 124 / 119 |
| 64×64, 67 casters, 400 | 229 / 222 / 241 | 153 / 238 / 231 |

Within noise (WP0's round 2 and WP4's round 1 casters row are host dips; each tree's best rounds match); every
scenario far above 25 ticks per second. `DESIGN.md` §5 is unchanged.

## 6. WP6: bookkeeping

B-135 removed from the register as fixed; B-138 stays open (with B-134). The Combat row says ranges to and from a
structure measure to its engine radius while its dome is drawn at its footprint, and lists
`structure_radius_tests.rs`. Structure-contact Amendment 2 corrects its §2.2 bullet. This plan's row is `Partly done` until the WP5 renders are made.

## 7. Merge of main `477c740e`

Conflicts: `DESIGN.md`'s register (main's B-137 and this branch's B-138 both kept) and the header comment of
`assets/captures/showcase.ron` (both comments kept). Tools of main (`477c740e`, built in a temporary worktree,
removed) against tools of the merged tree:

| What | Result |
|---|---|
| Matrix (`--compare` main) | Bit-identical (1,160 runs; expectation failures 0 on the seeds of record, 4 over all seeds) |
| Battle 33 `seeds.csv` | Byte-identical |
| Showcase, default seed to the end | Main: player 1 at 2,193 (byte-identical to WP2); merged: player 1 at 2,191 (byte-identical to WP4) |
| `scenarios/showcase_effects.ron` (main's new effects take battle), 6,000 ticks | Diverges at `units.csv` tick 319 (see below); neither run ends within the trace (outcome "in progress" both) |

**Effects battle, traced.** At tick 318 player 1's napalm wheeler 13 (radius 0.625, scan 5.5) stands at
(16.7194, 11.4863), 8.745 from player 0's hall centre (8.5, 8.5): beyond the old scan reach 5.5 + 0.625 + 2.5 =
8.625, within the new 8.875. It acquires the hall (reader A1) and drops its route on tick 319. Control: the merged
tools on a scratch `assets/` copy with only the Command Center row at 2.5 give `units.csv` byte-identical to main's.
The raid's deaths shift by a few ticks (player 1's ninth to sixteenth deaths at 633 to 718 instead of 643 to 742;
player 0's last three at 652, 688, 707 instead of 639, 683, 690). The divergence falls inside the effects take's
`siege` scene (1x from about tick 304 for 240 frames), whose scene checks require each listed effect to spawn during
the scene; that check needs a render (GPU), pending with §5's renders.
