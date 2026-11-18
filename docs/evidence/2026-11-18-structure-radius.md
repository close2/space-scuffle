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
