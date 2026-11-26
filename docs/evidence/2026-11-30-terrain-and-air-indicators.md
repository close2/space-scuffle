# Evidence: Cliff Feet on the Simulation's Boundary, and Air Indicators that Follow the Ground (B-243, B-238)

Measurements of plan [`2026-11-30-terrain-and-air-indicators`](../plans/2026-11-30-terrain-and-air-indicators.md):
the base of record, the WP0 measurements, the red runs of every new test, and (from WP1 on) the green runs, the
golden re-records and the review captures. Scratch material (logs, captures, the scratch mesher check) is under
`~/scratch/terrain-indicators/`; none of it is committed.

## 1. WP0

### 1.1 Base of record

- `main` at `e5ebc043`, merged into branch `terrain-indicators-plan` (plan commits `a42b7d4b`, `183a763a`).
- No production code differs from `main` at the WP0 commit: WP0 adds tests, test support, the gate's catalog entries
  and one unused public constant, `indicators::SHADOW_LIFT` (0.02), which U-1 and U-2 name.

### 1.2 The scratch mesher check (WP0 step 1)

`~/scratch/terrain-indicators/mesher_check/` extends the wall-clearance check (`~/scratch/wall-clearance/mesher_check/`):
it calls the client's `generate_wall_profile` for every contour midpoint of every threshold run of every dual cell, as
`emit_dual_cell` and `emit_cliff_walls` do, and classifies the rows. Output `result_main.txt`:

| Map | Foot toes, ground low side (off the edge) | Canonical rows, ground low side (towards it) | Canonical rows, void low side (towards it) | Lone-low dual cells (nook area) | Saddles |
|---|---:|---:|---:|---:|---:|
| `showcase_canyon` | 318 (318, all at 0.2200) | 1,050 (534, largest 0.0600) | 10,816 (4,967, largest 0.0600) | 110 (13.75 cells) | 0 |
| `default_arena` | 0 | 0 | 10,304 (4,694) | 0 | 0 |

The two canonical-row columns of `showcase_canyon` add up to the wall-clearance figure (11,866 rows, 5,501 towards
the low side). Most walls border the abyss or the map's edge, where the low side is void and D-1 sets nothing back.
`default_arena` has no cliff between two walkable plateaus, so its T-B tests pass on `main` and guard against
regressions only.

### 1.3 The catalog (WP0 step 2)

**Air archetypes**, drawn radius and the shadow's height above the ground on `main` (B-244: the record's altitude is
`ground_z + 2.5`; the shadow is drawn at `ground_z + r` for `r >= 0.4`, `ground_z + 2.5 + r − 2.5·r/0.4` below):

| Archetype | Drawn radius | Media | Shadow above ground |
|---|---:|---|---:|
| `saucer_interceptor` | 0.25 | yes | 1.1875 |
| `stealth_eye_orb`, `sonic_bat` | 0.50 | no | 0.50 |
| `omni_probe_drone` | 0.625 | yes | 0.625 |
| `flying_saucer`, `sky_leviathan` | 0.625 | no | 0.625 |
| `vector_interceptor`, `repair_shuttle`, `sky_bastion`, `cloak_gunship` | 0.75 | yes | 0.75 |
| `phase_prism_saucer` | 0.875 | no | 0.875 |
| `spore_matriarch` | 1.00 | yes | 1.00 |
| `prismatic_saucer`, `tongue_chameleon`, `amber_pollen_dome` | 1.00 | no | 1.00 |
| `mothership_carrier` | 1.25 | yes | 1.25 |

**Ground archetypes against D-4** (the rim top recedes to `FOOT_WIDTH + RIM_WIDTH` = 0.37 behind the edge). A body on
the high side is held at its terrain radius `r_t` from the edge, so it overhangs the rim chamfer by
`RIM_DROP·(0.37 − r_t)/RIM_WIDTH` when `r_t < 0.37`. The smallest terrain radius in the catalog is 0.3125
(`welder_drone` with media; `grav_harvester` and `nectar_worker` without): **largest brink overhang 0.0767**, below
stop rule 6's 0.2. Every other ground archetype has `r_t >= 0.375` and no overhang. The plan's "collision radius 0.25"
is `saucer_interceptor`, an air unit.

### 1.4 Support extraction (WP0 step 3)

`MapMesh`, the synthetic map, the ramp-geometry fixture (stations, `fixture_map`, `ramp_cells`, `footprint_margins`)
and the shipped-map loader moved to `crates/client/tests/support/terrain_mesh.rs`, with a 3D triangle-to-column clip.
`mesher_golden_tests` passes with its hashes unchanged after the move, and `ramp_geometry_tests` T0 to T3 pass
(`red/wp0_mesher_golden_after_move.log`, `red/wp0_ramp_geometry_after_move.log`).

### 1.5 Red runs (WP0 step 4)

Each new target run alone on the WP0 tree; logs under `~/scratch/terrain-indicators/red/`.

| Test | Result on WP0 | Log |
|---|---|---|
| T-A `wall_rows_never_cross_an_edge_towards_ground` | fails: 34,350 of 72,704 rows or toes on the low side; toes at `p + 0.22·d`, canonical rows up to +0.06 | `wp0_T-A_T-A2_U-1.log` |
| T-A2 `jitter_amplitude_fits_behind_the_setback` | passes (premise, compile-time) | same |
| U-1 `the_air_shadow_lies_on_the_ground_for_every_radius` | fails: 120 cases, e.g. r 0.15, h 2.5: shadow 1.7125 above the ground | same |
| T-B1 `no_profile_rises_inside_walkable_ground_*` | fails on `showcase_canyon` (2,146 triangles in 135 cells, up to 0.0600 deep), synthetic (8,608 in 346), ramp fixture (13,606 in 573); passes on `default_arena` | `wp0_T-B.log` |
| T-B2 `no_terrain_rises_inside_walkable_ground_*` | fails on `showcase_canyon` (2,342 triangles: 196 more, the nooks), synthetic, ramp fixture; passes on `default_arena` | same |
| U-2 `the_shadow_follows_a_change_of_altitude` | fails: shadow +0.605 above the ground at arrival, +2.105 after the climb, −1.595 and still shown after the landing | `wp0_U-2_I-1.log` |
| I-1 `an_air_unit_crossing_a_cliff_keeps_its_shadow_on_the_ground` | fails: 50 of 50 frames, shadow +0.605 above the record's ground (`omni_probe_drone`, r 0.625) | `wp0_I-1.log` |
| G-1 `a_body_drawn_tangent_to_a_wall_is_not_cut_by_the_cliff` (radv) | fails: the cliff hides 283 of 3,184 silhouette pixels within 0.30 world units of the wall (91.1 % changed); control 5,332 of 5,332 | `wp0_G-1.log`; captures `g1/main/radv/` |

G-1's staging on the shipped battle (`showcase_canyon`): the wall `x = 26`, contact row `y0 = 31`, low side `x < 26`
(level 1 against level 2), the unit `pulse_bot` (drawn and terrain radius 0.375) at `x = 25.625`. The wall projects
onto canvas column 820.0 (the battle view's viewport starts right of its side panel), and 0.30 world units at the
sphere's depth span 41.8 px: **139.2 px per world unit**, so the foot (0.22) covers about 31 px and the jitter (0.06)
about 8 px.
