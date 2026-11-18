# Evidence: Terrain Radius (B-116)

Measurements and test results of plan [`2026-11-18-inner-radius`](../plans/2026-11-18-inner-radius.md), one
section per work package.

## How to reproduce

Each tree builds its release tools into its own target directory and copies them aside, so later builds cannot
change an earlier step's binaries (charter §7: hot builds on the home disk, large output on `~/scratch`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} <bin>/
```

Then, from the repository root:

```bash
<bin>/matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv
<bin>/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route
for s in 1 2 3 4 5 6 7 8; do <bin>/route_trace --scenario scenarios/showcase.ron --seed $s --out <dir>/route-s$s; done
diff <base>/b33/33_24_marines_vs_12_zealots/seeds.csv <dir>/b33/33_24_marines_vs_12_zealots/seeds.csv
diff <base>/route/units.csv <dir>/route/units.csv
```

Goldens: `cargo test -p sim_core --test golden_replay_tests` in debug and with `--release`, and
`cargo test -p sim_core --test golden_replay_tests --target wasm32-wasip1 --release` under wasmtime (CI's
`determinism-wasm` command). Throughput: `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`,
base and candidate alternated, three rounds.

The base of WP1 is the plan's commit with WP0 (`79f1926b`, docs only on `345da93f`).

## 1. WP1: the field and the loader

**Change.** `UnitArchetypeDef` reads an optional `terrain_radius` (`#[serde(default)]`); `UnitArchetype` carries
`terrain_radius: SimScalar`, the authored value or else `collision_radius`; structures get their radius. The
loader refuses an authored value that is not finite, not strictly positive after Q32.32 conversion, not strictly
below `collision_radius`, or on an `Air` mover, each naming `terrain_radius` (D-1, D-2). Nothing reads the field
yet.

**Tests, red first.** In `catalog/validation.rs`, over the shipped catalog with one line inserted:

| Test | Before the loader rules | After |
|---|---|---|
| `a_terrain_radius_is_read_and_defaults_to_the_collision_radius` | Does not compile on the base (no field); passes once the field is read | Passes |
| `a_terrain_radius_must_be_positive_and_finite` (0, −0.1, NaN, inf, 1e-30) | Fails: the catalog loads | Passes |
| `a_terrain_radius_must_be_smaller_than_the_collision_radius` (equal, larger; 1/16 below accepted) | Fails: loads | Passes |
| `an_air_unit_may_not_have_a_terrain_radius` | Fails: loads | Passes |

`runtime_catalog_tests`, `catalog_reference_values_tests` and `golden_replay_tests` pass unchanged. With no data
and no reader, WP1 is a no-op on every state; its bit-identity is measured together with WP2's (§2).

## 2. WP2: every terrain path reads the terrain radius

**Change.** `MotionPolicy` gains `terrain` from `UnitArchetype::terrain_radius`, and `terrain_radius()` returns
it; `resolve` refuses a terrain radius that is not positive or above the collision radius (D-2). Then D-5's
paths: T1 contact (already through the accessor), T2 route requests (`route_request`), T3 group lanes
(`group_leader`), T4 the Charge path (`is_walk_shorter_than`), T5 the domain (`MotionPolicy::contains`,
`clamp_into_domain`, `flee_destination`'s bounds; messages now say "terrain-radius"), T6 placement against
terrain (`validate_placement_terrain`), T7 production exits (`ExitRing::terrain_radius`: ring margin, the
product's map checks and both `egress_connected` calls; occupancy keeps the collision radius), T8 construction
site occupancy (`stands_on`), T9 the mobile producer's map bounds, T10 the overlap meter. D-6's paths keep the
collision radius.

**Tests, red on WP1 first** (WP2's production code reverted, the tests kept):

| Test | WP1 | WP2 |
|---|---|---|
| T-1 `a_body_rests_at_its_terrain_radius_against_a_wall` | Fails: the goal 0.05 beyond `t` does not fit `r`, retargeted to the cell centre | Passes: walks there, a 45° hostile push rests it at `t` |
| T-2 `a_gap_that_fits_the_terrain_disc_is_routed_through` | Fails: routes round | Passes |
| T-3 `corner_waypoints_keep_the_terrain_radius_and_margin` (offset `t + 0.05` exactly, the funnel's `CORNER_SAFETY_MARGIN`) | Fails: `r + 0.05` | Passes |
| T-4 `two_bodies_still_separate_at_their_full_radii` | Passes (a guard: unit contact is unchanged) | Passes |
| T-5 `the_map_edge_admits_the_terrain_disc` | Fails: `t + ε` refused `OutsideDomain` | Passes; air still at `r` |
| T-6 `a_placement_beside_a_structure_uses_the_terrain_radius` | Fails: `t + ε` refused `Terrain` | Passes |
| T-7 `a_body_at_its_terrain_radius_from_a_site_does_not_block_it` | Fails: the site is obstructed | Passes |
| T-8 `exits::tests::an_exit_ring_keeps_the_terrain_radius_from_the_footprint_and_the_collision_radius_from_bodies` | Fails: exit two cells off | Passes |
| T-9 `a_charge_dash_path_is_tested_at_the_terrain_radius` | Fails: no dash | Passes |
| T-10 `a_group_lane_is_searched_at_its_leaders_terrain_radius` | Fails: lane at `r`, round the wall | Passes |
| T-11 `contact::tests::a_settled_move_never_deepens_at_the_terrain_radius` | Does not compile (no `terrain` field) | Passes |
| T-12 `movement::tests::a_terrain_radius_above_the_collision_radius_is_an_invariant` | Does not compile | Passes |
| T-13 `sim_map` `navmesh_tests::a_smaller_disc_routes_wherever_a_larger_one_does` | Passes (pins existing behaviour) | Passes |

T-13 searches 500 seeded pairs per map and structure variant, each at one of the four radius pairs in turn (125
per pair), which keeps `navmesh_tests` fast (about 9 s in debug; all four pairs on every start and goal took
34 s). No counterexample.

**Finding (T-6): structures are also bodies.** A registered structure is an entity whose collision radius is half
its footprint width. Placement occupancy (`validate_placement_unoccupied`), the collision phase and steering's
obstacles meet it at the unit's collision radius (D-6's paths), so at the middle of a footprint face that circle,
not the footprint, keeps a unit at `r`. Near a face's ends the footprint decides, at `t`. T-6 therefore tests
points beside the west face far enough off its middle that the circle does not reach them. Whether unit-structure
pairs in those paths should use `t` (R3: inner radius "governs interaction with buildings") is left to the
orchestrator; the plan's D-6 keeps them at `r`.

**Fixture edits.** 26 sites in eight `sim_core` integration test files set a shipped archetype's
`collision_radius` through `catalog.get_mut` (or on a clone) to build scratch bodies: `attack_dash_tests`,
`acceleration_tests`, `checked_movement_tests`, `cluster_arrival_tests`, `group_routing_tests`,
`movement_layers_tests`, `production_transactions_tests`, `transport_ownership_tests`. Each now sets
`terrain_radius` to the same value, which is what the field meant for them before (it followed the collision
radius); without it a radius set below the archetype's authored terrain radius is a `resolve` invariant, and a
larger one would route at the old size. No assertion changed.

**Bit-identity** (D-12, 1), WP0 tools against WP2 tools, same assets:

| What | Result |
|---|---|
| Matrix (`--compare`) | Bit-identical (1,160 runs; expectation failures 0 on the seeds of record, 4 over all seeds, as on the base) |
| Battle 33 `seeds.csv` (64 seeds plus 4) | Byte-identical |
| Showcase `units.csv`, 1,800 ticks and to the end; seeds 1 to 8 | Byte-identical, and every `summary.md` (player 1 wins at 2,218; seeds 1 to 8 as the plan's base) |
| Overlap meter (deeper unit-ticks) | 0 / 0 |
| Goldens: debug and release native, `wasm32-wasip1 --release` under wasmtime | Pass unchanged |

**Throughput** (`throughput` bench binaries of WP0 and WP2, alternated, three rounds, under the gate lock; ticks
per second, median of three repeats each; the host was shared and loaded):

| Scenario | WP0 | WP2 |
|---|---|---|
| 64×64, 0 units | 2,480 / 2,225 / 3,168 | 2,133 / 2,310 / 3,210 |
| 64×64, 100 | 622 / 924 / 805 | 559 / 489 / 950 |
| 64×64, 400 | 157 / 116 / 85 | 89 / 106 / 164 |
| 128×128, 400 | 189 / 149 / 113 | 175 / 121 / 125 |
| 256×256, 0 | 200 / 143 / 143 | 190 / 136 / 174 |
| showcase canyon, 400 | 109 / 100 / 112 | 114 / 94 / 109 |

Within noise (the round-to-round spread of either tree exceeds any difference); every scenario above 25.

## 3. WP3: the 5.0.14 values (held on the showcase's flip; Amendment 1)

**Change.** The 17 rows of plan §2.2 in `assets/catalog/units.ron`, each with its layer and line; the ten ground
units that omit the field say why (equal to `Radius`; the Roach's 0.625 above its 0.5; the Zergling's unset
value, Q0087).

**Tests, red on WP2 first.** `catalog_reference_values_tests::b116_unit_terrain_radii_follow_starcraft` (the
table `SC2_UNIT_TERRAIN_RADIUS`, 27 ground analogues; air analogues equal their radius) failed with the SCV
analogue at 0.375; `only_ground_units_with_a_smaller_inner_radius_author_one` found 0 of 17. Both pass with the
data. The WP1 loader tests were made independent of authored data (they inserted a second `terrain_radius`).

**Measurements** (WP2 tools and data against WP3; the release tools read the catalog at run time):

| What | Result |
|---|---|
| Matrix (`--compare` WP2) | One battle moved: `35_12_vikings_vs_6_colossi`, jittered seed 8 (not of record), end tick 435 → 438, blue 12–0 in both. No recorded outcome changed; expectation failures 0 on the seeds of record, 4 over all seeds, as before |
| Battle 33 `seeds.csv` | Byte-identical: the zealots never meet a wall on the flat arena |
| Overlap meter | 0 deeper unit-ticks on every showcase run, both owners |

**Battle 35, seed 8, traced** (`matchup_trace --battle 35_12_vikings_vs_6_colossi --seed 8`, WP2 catalog through
`--catalog-dir`): positions are identical to tick 381. From tick 382 red's Colossus analogue (entity 18, at
(5.07, 52.36)) walks west to the arena's border; its destination is clamped into the movement domain at its
terrain radius, so it stops at x = 0.5625 instead of x = 1.0 (tick 420). The vikings finish it three ticks later.
An affected archetype at the arena border (T5), outcome unchanged: accepted under D-12, 2.

**Showcase** (`route_trace`, to the end of the match):

| Seed | WP2 | WP3 | Reversals P0 / P1 (WP2 → WP3) | Stall give-ups P0 / P1 |
|---|---|---|---|---|
| Default | P1 at 2,218 | **P0 at 2,599** | 19 / 18 → 24 / 16 | 1 / 0 → 0 / 0 |
| 1 | P0 at 2,152 | **no end within 6,000 ticks** | 14 / 26 → 21 / 15 | 0 / 0 → 0 / 0 |
| 2 | P1 at 2,180 | **P0 at 3,417** | 20 / 34 → 15 / 35 | 1 / 0 → 0 / 0 |
| 3 | P1 at 1,943 | **P0 at 1,615** | 21 / 30 → 24 / 22 | 1 / 0 → 0 / 0 |
| 4 | P1 at 2,597 | **P0 at 1,727** | 17 / 28 → 17 / 12 | 1 / 0 → 0 / 0 |
| 5 | P1 at 1,958 | **P0 at 2,156** | 22 / 25 → 15 / 23 | 1 / 0 → 0 / 0 |
| 6 | P1 at 1,938 | P1 at 2,617 | 11 / 24 → **23** / 10 | 1 / 0 → 0 / 0 |
| 7 | P1 at 2,603 | **P0 at 1,886** | 19 / 17 → 13 / 11 | 1 / 0 → 0 / 0 |
| 8 | P1 at 1,927 | **P0 at 2,559** | 19 / 24 → 16 / 24 | 1 / 0 → 0 / 0 |

The stop rule of D-12, 4 applied and WP3 was held. The trace from the spawn: positions are identical to tick 8;
on tick 9 player 1's welder drone 16 at (53.39, 52.63), which the pulse bots passing it press against the south
face y = 53 of player 1's hall footprint (cells 53 to 57 in x and y, near its west corner; a structure, not
terrain, and the hall's circle of radius 2.5 does not reach the drone, so the footprint decides), rests at its collision radius 0.375 below it on WP2 (y = 52.625) and moves on to its terrain
radius 0.3125 on WP3 (y = 52.6875 by tick 13), never deeper. From tick 10 pulse bots 17, 18 and 19 walking past
it take other paths, and the lane fight diverges. Classified as caused by the rule (R1, the workers' 5.0.13 inner
radius), not a defect. On seed 1 both armies destroy each other and each side keeps two idle welder drones, so no
player is defeated. Seed 6's player 0 reversals are base + 12 (bound + 10). Amendment 1 accepts all three.

**Goldens.** `golden_replay` re-recorded: every checkpoint changed from checkpoint 1. Traced against the same tree
with every terrain radius reset to the collision radius (WP2's behaviour): positions are identical until the tick
that applies the order of tick 40, red's zealots' `AttackMove(32, 20)`. Their routes turn round the cliff corner
(34, 34) at 0.375 + 0.05 instead of 0.5 + 0.05 (entity 21's corner waypoint (33.58, 34.05) instead of
(33.45, 34.06)), so all 12 zealots step differently from that tick. **`golden_replay_orders` does not move**: its
four zealots never touch a wall within their radius (a finding, plan D-12, 5); both its phase-order situations are
still found. The re-recorded list passes in debug and release native, `wasm32-wasip1` release under wasmtime, and
once in `wasm32-wasip1` debug.

**Test edits explained by D-5.** `wall_contact_tests` T-B1 to T-B5 and T-B9 measured depth at the collision
radius; they pick a 0.375 body by collision radius, which is now the SCV analogue with terrain radius 0.3125, so
the contact rule keeps it 0.0625 nearer the wall than the meter allowed. Depth and every distance the tests derive
from the body's radius (T-B2's touching start, T-B4's wedge, T-B9's start, T-B5's corner gap) now use the terrain
radius; T-B3 keeps its start 0.02 from the wall. No bound was loosened. `exits.rs`'s
`open_ground_exit_is_the_first_ring_cell_at_the_minimum_corner` now derives its margin from the terrain radius
(unchanged for the worker: `ceil(0.3125 + 0.5)` = `ceil(0.375 + 0.5)` = 1). No production, construction,
economy or `attack_dash_tests` test changed.

**Throughput** (the WP2 `throughput` binary with the WP2 catalog through `SPACE_SCUFFLE_ASSETS` against the WP3
catalog, alternated, three rounds, under the gate lock; ticks per second, median of three repeats each; the host
was shared and loaded):

| Scenario | WP2 data | WP3 data |
|---|---|---|
| 64×64, 0 units | 3,268 / 3,229 / 753 | 2,223 / 1,908 / 3,270 |
| 64×64, 100 | 941 / 951 / 709 | 763 / 429 / 949 |
| 64×64, 400 | 160 / 162 / 145 | 113 / 100 / 164 |
| 128×128, 400 | 167 / 194 / 93 | 179 / 186 / 181 |
| 256×256, 0 | 201 / 197 / 100 | 174 / 200 / 200 |
| showcase canyon, 400 | 95 / 120 / 107 | 117 / 118 / 117 |

Within noise (each tree's best round matches the other's); every scenario above 25. `DESIGN.md` §5 is unchanged.

## 4. WP4: client and presentation check

**No client code changed.** The client tests the catalog rule selects passed in `cargo gate quick` on the WP3 tree
(`asset_baker` catalog targets, `client/effect_catalog_tests`, `client/effect_runtime_tests` and the others it lists).

**Picture check** (RX 580, `showcase render --driver radv`, one `with-gpu-lock` per take; a scratch storyboard of
one take in `LiveBattle` mode, so player 1's base is not fogged, camera on (53.4, 52.7); stills under
`~/scratch/inner-radius/wp4/stills/`). Player 1's welder drone 16 is pressed against its hall's footprint from
tick 9 and rests at its terrain radius from tick 13 (§3). Observation: the drone's bubble (drawn at its radius
0.375) stands beside the dome without touching it; the point where it rests is near the footprint's corner, where the
dome's circle (half the footprint's width) does not reach, so no overlap is visible (`p39.png`, `p89.png`). At a
face's middle the structure's circle keeps the unit at its radius (B-131), so a bubble cannot overlap a dome there
either. No `t < r` unit rests against a cliff in the showcase (player 0's affected units are its idle welder
drones), so the cliff case was not captured; nothing reads as a fault, and no bug is registered.

**Storyboard** (`assets/captures/showcase.ron`), retold for player 0's win. The lane fight runs from the first shot
(tick 305) to tick 676, its last death; player 0 keeps three pulse bots, which walk to player 1's base, kill its
workers (ticks 988 and 1,022) and fire on its hall until it falls at tick 2,599. `march` now follows one of the
viewer's survivors (player 0) and its caption says so; `assault`'s caption is the viewer's siege of the enemy
hall; `clash` is 375 frames (was 370), because the first render ended it on tick 674, two ticks before the fight's
last death. Render 2 (take 1,422 frames in 237 s):

| Scene | Speed | Frames | First frame | Start tick |
|---|---|---:|---:|---:|
| title | paused | 125 | 0 | 0 |
| terrain | paused | 150 | 125 | 0 |
| units | 1x | 150 | 275 | 1 |
| smog | 1x | 100 | 425 | 151 |
| advance | 1x until combat (25 to 500) | 54 | 525 | 251 |
| clash | 1x | 375 | 579 | 305 |
| march | 4x until combat (5 to 200) | 68 | 954 | 683 |
| assault | 2x | 200 | 1,022 | 953 |
| closing | paused | 200 | 1,222 | 1,351 |

`clash` covers ticks 305 to 679, past the last death (676); `march` ends on combat at the enemy base after 68
frames; `assault` covers ticks 953 to 1,351, with both workers' deaths; `closing` pauses with the enemy hall
standing, so the video never shows the result. Stills (640 wide) under `~/scratch/inner-radius/wp4/show2/stills/`
were looked at: `f800.png` (clash, both armies firing beside the west ramp), `f950.png` (the viewer's survivors after
the fight), `f1100.png` (assault on the enemy hall, caption "Assault"), `f1300.png` (closing card). The first render
(`show1`, clash 370) assembled its site within its budgets (video 14.1 MB, teaser 4.1 MB).
`showcase_storyboard_tests` (7) pass.

**Throughput** (WP3 §3): within noise; `DESIGN.md` §5 unchanged.

## 5. Review findings F-1 to F-4

**F-1.** `MotionPolicy::resolve` refuses an air body whose terrain radius is not its collision radius ("an air
body's terrain radius must equal its collision radius"): `contains` reads the terrain radius for every layer.
Test `movement::tests::an_air_body_with_a_smaller_terrain_radius_is_an_invariant`.

**F-2.** One focused test per terrain-radius path no test pinned. Each was run with its path temporarily reverted to
the collision radius (one line each, restored after) and failed:

| Path reverted | Test | With the revert |
|---|---|---|
| `flee.rs` bounds (`policy.radius`) | `terrain_radius_tests::a_flee_goal_stops_at_the_terrain_radius_from_the_map_edge` | Fails |
| `locomotion.rs` `clamp_into_domain` | `locomotion::tests::a_cast_goal_is_clamped_into_the_terrain_radius_domain` | Fails |
| `lifecycle.rs` mobile producer bounds | `terrain_radius_tests::a_mobile_producer_is_bounded_by_its_terrain_radius` | Fails |
| `exits.rs` exit cell map bounds (`ring.radius`) | `exits::tests::an_exit_cell_at_the_map_corner_admits_the_terrain_disc` | Fails |
| `exits.rs` both egress checks (`ring.radius`) | the same test | Fails |
| `route_overlap.rs` meter (`collision_radius`) | `route_overlap::tests::the_meter_measures_the_terrain_disc` | Fails |

The ring margin and the wider-than-map check were already pinned by T-8.

**F-3.** `wall_contact_tests` T-B1 to T-B5 and T-B9 now run twice, with a 0.375 body whose terrain radius equals
its collision radius (the marine's) and with one whose terrain radius is 0.3125 (the workers'), both chosen by
their radii.

**F-4.** Doc lines wrapped. The wall drone 16 rests against in the showcase trace (§3) is the south face of player
1's hall footprint (structure 14, cells 53 to 57, face y = 53), near its west corner; the hall's circle (radius 2.5,
centre (55.5, 55.5)) is 3.5 from the drone, beyond the sum of the radii, so the footprint, at the terrain radius,
decides. R1 covers structures. §3 and Amendment 1 now say so.
