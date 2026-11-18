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
