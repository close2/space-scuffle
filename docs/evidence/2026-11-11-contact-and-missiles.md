# Evidence: disc contact with terrain (B-113) and homing missile impact (B-115)

Measurements of plan [`2026-11-11-contact-and-missiles`](../plans/2026-11-11-contact-and-missiles.md), one
section per work package.

## How to reproduce

Each tree builds its release tools into its own target directory and copies them aside, so later builds
cannot change an earlier step's binaries:

```bash
export CARGO_TARGET_DIR=~/scratch/cargo-target/<tree>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} <bin>/
```

Then, from the repository root:

```bash
<bin>/matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <previous>/matrix/matrix.csv
<bin>/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route
diff <previous>/b33/33_24_marines_vs_12_zealots/seeds.csv <dir>/b33/33_24_marines_vs_12_zealots/seeds.csv
diff <previous>/route/units.csv <dir>/route/units.csv
```

The matrix runs the four seeds of record plus 16 jittered ones per battle; battle 33 runs 64 seeds plus the
four of record. The base of every package is the tree before it. The first base is branch `bugfix-113-114`
(`09c439f`, B-114 fixed) with `main` (`ce8c030`) merged, whose figures are the deviation-fix evidence §18.6
ones: the showcase's ramp E foot P0 1.198 / 1.300 and 190° / 454°, player 0 wins at tick 1,693.

## 1. WP1: missiles impact on the reaching tick (B-115)

**Change.** `step_effect_missiles` (`crates/sim_core/src/effects/step.rs`) now calls `missile_step`: if the
gap to contact is at most the missile's speed, the missile impacts on this step, at `aim` for a point and at
the target's edge on the line it came from for a unit, and that contact point is the impact walk's source;
otherwise it moves its speed towards the aim. `ARRIVAL_EPSILON` is gone: the test is `gap ≤ speed` and the
contact point is computed, so the degenerate direction needs no tolerance (a missile already inside its
target's radius impacts where it stands).

**Tests, red on the base first.**

| Test | Base | After |
|---|---|---|
| T-M1 `effects::step::tests::a_missile_impacts_on_the_step_that_reaches_contact` | Fails: the homing missile is still flying after its first step | Passes |
| T-M1 `the_step_impacts_at_the_contact_point_or_flies_its_speed` (the pure step: contact point, point target, inside the radius, a gap of exactly the speed) | — (new function) | Passes |
| T-M2 `abduct_tests::a_tongue_reaches_a_unit_walking_away_from_it` | Fails: no contact within the bound of 8 ticks (the tongue trails the unit) | Passes |
| T-M3 `abduct_tests::a_missile_reaches_a_walking_and_a_flown_receding_target` (a scratch missile at 1.05; a walking target and one flown at 0.55 a tick by a scratch `Displace`; the scratch catalog expresses both) | Fails: the walking target is not reached within 11 ticks | Passes; the flown target is reached during its flight |
| T-M4 `combat_resolution::tests::a_projectile_and_an_effect_missile_impact_on_the_same_step` | Fails: the projectile impacts on step 8, the missile never | Both on step 8 |

**Counts that moved, each by exactly one step.** A cast between ticks is first stepped on the next tick, its
cast tick (spellcaster D-16 item 3); the missile now impacts on its `ceil(d ÷ speed)`-th step, the cast
tick's included. The `1 +` of the old counts was B-115's extra step, not the cast tick.

| Test | Before | After |
|---|---|---|
| `fungal_growth_tests::the_missile_lands_ceil_distance_over_speed_later_and_is_published_while_it_flies` | `1 + 13` steps | 13 |
| `abduct_tests::the_tongue_lands_ceil_gap_over_speed_later_and_the_pull_ends_twenty_ticks_after` | contact at `cast + ceil(gap ÷ 1.05)` | one tick earlier |
| `abduct_tests::the_tether_is_a_link_from_launch_to_landing_without_a_gap` | `ceil + 1 + 2 + 18` snapshots | `ceil + 2 + 18` |
| `effect_tree_tests::a_point_missile_impacts_after_ceil_distance_over_speed` | 3 ticks after the cast tick | 2 |

**Measurements** (base: `b114`, the tree above).

- **Matrix.** Only `37a_abduct_sieged_tank` and `38a_fungal_turns_zerglings` moved, as the plan expected.
  No winner flipped. On the seeds of record every expectation still holds. 37a: blue's survivors are
  unchanged (1 on every seed); red's survivors move between 4 and 5 on three jittered seeds (1, 6,
  8), all inside the band. 38a: the zerglings still win every seed with red wiped out; one jittered seed (15,
  not of record) ends with 2 zerglings alive, one below the band `(3, 8)` (base: 4), so the matrix's
  expectation failures over all seeds go from 3 to 4. The claim, Fungal Growth turns the fight, holds on all
  20 seeds.
- **Battle 33** (68 seeds): `seeds.csv` byte-identical.
- **Showcase** (`route_trace`, 1,800 ticks): `units.csv` byte-identical (no spells), player 0 wins at tick
  1,693.
- **Throughput.** Measured with the contact rule in WP4 (§4), which covers the spell scenario; this change
  replaces one comparison with another and adds no work per missile.

## 2. WP2: the contact query and the overlap meter (no behaviour change)

**Query.** `Pathfinder::wall_contact` (`crates/sim_map/src/navmesh/segment.rs`, `nearest_blocking_point`)
floods from the point's triangle like the segment predicate, measures every blocking edge it meets within
the radius exactly in `i128` and keeps the nearest: its point, its squared distance in raw Q32.32 units and
the edge's normal into the open triangle. The flood prunes with the Q16 predicate at the radius plus two Q16
units, which covers the predicate's rounding of the point, so pruning never drops an edge in reach.
`Pathfinder::wall_contact_exhaustive` is the reference: every blocking edge of every open triangle, no flood.

**Tests** (`crates/sim_map/tests/wall_contact_tests.rs`, all green on the first run): a straight wall, a
convex corner (the vertex), a concave corner (the nearer wall wins, both ways), a structure's edges, a
ramp's foot, slope and top (no wall) beside a cliff (a wall), the map border, a centre on the border (the
normal points into the map), no wall in reach, and the property test: 2,000 seeded points per shipped map
with radii 0.25 to 1.5, the flood's distance equal to the reference's (at least 100 points touching a wall
per map).

**Overlap meter.** `route_trace` now measures, after every tick, each walking ground body's depth into the
walls (`radius − distance` under its physical request) and writes `overlap.csv` (one row per overlapping
unit-tick) and a summary table: overlapping unit-ticks, unit-ticks on which a body's depth rose by more than
2⁻²⁰ since the previous tick, and the deepest overlap. `units.csv` is unchanged. The base's figures:

| Owner | Overlapping unit-ticks | Deeper unit-ticks | Max depth (entity, tick, position) |
|---:|---:|---:|---|
| 0 | 555 | 118 | 0.405 (13, 366, (25.90, 32.94)) |
| 1 | 1,079 | 74 | 0.426 (26, 370, (28.93, 38.00)) |

Both sides rely on overlap: about 1,600 unit-ticks, 192 of which pressed a body deeper. The B-113 episode is
visible in it: pulse bot 7's depth rises on every trace tick from 756 to 761 (0.018, 0.072, 0.101, 0.142,
0.255, 0.354; trace ticks count from 0), then it walks along the wall 0.0004 to 0.06 from it.

**Seeds.** The showcase draws its weapons' random delays (`random_delay_ticks`) from the world's RNG, so
`route_trace` gained `--seed`, which replaces the scenario's seed. On seeds 1 to 8, run to the end of the
match, player 0 wins every one, at ticks 2,136, 2,143, 3,436, 1,875, 1,498, 1,572, 1,487 and 2,146. WP3
repeats this.

**Variant E's divergence** (`~/scratch/bugfix-113/v1/` against `base/`). E diverges from tick 9, at player
1's spawn cluster round (53, 52): bodies spawned overlapping a wall are thrown out (welder drone 16 is 0.52
off by tick 36). Player 0's west column diverges at tick 156 at the ramp W foot's wall end (29, 12), more
than 0.5 by ticks 169 to 190 (pulse bots 4 to 9). So E changed the battle twice before the lane fight: at
the spawn and at the column's first wall contact. The non-deepening rule throws nothing, so WP3 watches the
same two places: a spawn overlap must stay where it is, and the column at (29, 12) may only slide.

**Bit-identical.** The matrix (`--compare` against WP1: bit-identical), battle 33's `seeds.csv` and the
showcase's `units.csv` are unchanged; the goldens pass unchanged.
