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

## 3. WP3: the non-deepening contact rule (held: the showcase's winner flips)

**Change.** `crates/sim_core/src/movement/contact.rs`, `settle_on_terrain` (D-1, D-2), called before
`MotionProposal::commit` (both commit sites of `advance_checked_movement`, so every steering mode) and in
the collision phase's `apply_correction` (friendly and hostile pushes) (D-3).
`MotionPolicy::terrain_radius()` returns the collision radius (B-116). `WALL_CONTACT_TOLERANCE` = 2⁻²⁰,
`WALL_CONTACT_PASSES` = 3. Distances come from the query's exact squared distance by integer square root.
No early-out was written (D-5): it waits for the throughput measurement, which the stop rule deferred.

**Other position writers** (`grep "\.position = "` in `crates/sim_core/src`): the Abduct flight and landing
(`effects/step.rs`), transport unload (`transport/mod.rs`), the mode teleport (`world/abilities.rs`) are
placements; `steering::resolve_push_interaction` has no caller in the tick (tests and the public API only).
None is a walk, so none settles.

**Tests** (`crates/sim_core/tests/wall_contact_tests.rs`; T-B7 and T-B8 are unit tests in `contact.rs`,
because `settle_on_terrain` is crate-private). On the base, without the rule: T-B1 fails (body 4 pressed to
0.064 at tick 37 round the corner), T-B2 fails (pressed to 0.080), T-B5 fails (pulse bot 7 at 0.357 from
(35, 52) at trace tick 756), T-B6 fails as first written (a wedged start's route leaves through ground
narrower than its disc: the start-triangle exemption of D-4; the test now draws only starts clear of
walls). T-B3 and T-B4 pass on the base. With the rule all six pass, but T-B5 only because pulse bot 7 dies
at tick 479 in the lane fight, before the corner; the test now also requires the bot alive through ticks
740 to 800, so it is red on this tree. T-B7 (2,400 seeded moves on random wall blocks) and T-B8 (a
concave corner converges in two passes; a centre on the wall uses the open-side normal; an overflowing
projection is an error) pass.

**Measurements** (base: WP2).

- **Matrix**: bit-identical (`--compare`). **Battle 33**: `seeds.csv` byte-identical. The arenas have no
  walls a body reaches, as the plan expected.
- **Wall overlap**: the rule holds. Deeper unit-ticks 118 / 74 → 0 / 0; overlapping unit-ticks 555 / 1,079
  → 15 / 156, all at depth 0.000 (within the tolerance).
- **Showcase: the winner flips** (stop rule D-9, 4).

| | Base (WP2) | WP3 |
|---|---|---|
| Ramp W foot P0 ratio / turn (mean / max) | 1.301 / 1.473, 157° / 301° | 1.325 / 1.473, 158° / 230° |
| Ramp W foot P1 | — | 4 units, 1.513 / 1.528, 116° / 131° |
| Ramp E foot P0 | 1.198 / 1.300, 190° / 454° (6 units) | no pass: P0's army is dead |
| Ramp E foot P1 | 1.149 / 1.168, 81° / 152° | 1.209 / 1.352, 204° / 590° (pulse bot 18) |
| Reversals P0 / P1 | 17 / 7 | 4 / 28 |
| Blocked attacker ticks P0 / P1 | 137 / 107 | 305 / 323 |
| Stall give-ups | 0 / 0 | 0 / 0 |
| In range at tick 900 P0 / P1 | 0 of 6 / 0 of 0 | 0 of 0 / 0 of 4 |
| Outcome | player 0 wins at 1,693 | player 1 wins at 2,216 |

- **Seeds 1 to 8** (to the end of the match): base P0 wins all eight. WP3: P1 wins seeds 1, 4 and 6 (2,618,
  2,185, 2,199), P0 wins seed 3 (2,150), and seeds 2, 5, 7 and 8 do not end by tick 5,000.

**Trace of the flip.** The first divergence from the base is at trace tick 9 at player 1's spawn round
(53, 52) (welder drone 16, pulse bots 17 to 24: bodies spawned against the base's walls are pushed apart
by sliding instead of into the walls; 0.5 apart by ticks 36 and 44), and at tick 156 at the ramp W wall end
(29, 12), where player 0's column was pressed into the wall end on the base and now slides past it (pulse
bots 4 to 9 are 0.5 apart by ticks 169 to 190). These are the same two places, at the same ticks, as
variant E's divergence (§2), although this rule throws nothing. The lane fight then plays differently: on
the base player 0 keeps six attackers to tick 900; here player 0's ten attackers die between ticks 382 and
639 round (22–26, 32–38), player 1 loses six there (356 to 611), and player 1's four surviving pulse bots
walk to player 0's base and kill its two workers (1,159, 1,204).

**Liveness.** No match is stalled. In the four seeds that do not end, both armies have killed each other
and only the unarmed welder drones are left (seed 2: two of each side; seed 5: two and one), idle at their
bases: the scenario ends a match only when a side has no units, and workers do not attack. This is a battle
shape (an even trade), not a movement stall.

**Classification.** The flip begins with the rule itself: bodies no longer pressed into the walls at player
1's spawn and at the ramp W wall end change the timing of both columns before the lane fight (C1 is the
cited reason). But the measurements also show an implementation weakness that the plan names as R-3:
player 1's reversals rise from 7 to 28 (the acceptance allows base + 10), and pulse bot 18 near-stalls at
(38.08, 52.37), against the wall, for ten ticks (158 to 168, under 0.002 a tick) before it slides away along
it; its ramp E pass turns 590°. Steering's probes still test the centre only, so they propose steps into the
wall that the settled step then reduces to a sliver of slide. D-3 says such a probe gains a disc test in
this WP; that was not done before the stop rule applied, so the flip cannot yet be called purely
rule-caused. Held for the orchestrator.

**Quick tier on this tree.** fmt, workspace clippy, `workspace_checks`, the suite catalog and the
`wasm32-unknown-unknown` check pass. Red in `sim_core`: the two goldens (expected; WP4 re-records them),
T-B5 (above), and three walks round a wall that the rule now slows or stops, the same R-3 signature:
`attack_dash_tests::q0071_the_maximum_is_the_walk_not_the_straight_line` (no dash within 60 ticks: the
dasher does not get round the wall's end at (11, 13)),
`movement_contact_tests::melee_pursuit_takes_the_ramp_and_reaches_the_enemy` and
`movement_contact_tests::pursuit_rounds_a_single_blocked_cell_promptly`. These are implementation defects
to fix (the probe's disc test, D-3) before any acceptance decision.
