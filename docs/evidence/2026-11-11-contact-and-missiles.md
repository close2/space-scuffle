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

### 3a. WP3 continued: steering's probes get the disc test (R-3; still held: the winner flips)

The orchestrator's decision after §3: fix the implementation gap of D-3 and R-3 first, then
re-measure.

**Change.** `movement/contact.rs` gains two functions, and steering's look-ahead uses them:

- `probe_permits(map, pathfinder, policy, from, to)`: the centre rule, and for a ground body the
  segment predicate (`Pathfinder::segment_is_passable`) for its disc of `terrain_radius()`, under
  the physical request; when that disc does not fit along the step, the sweep is retried at the
  body's clearance at `from` less `WALL_PROBE_SLACK` (2⁻¹⁴, four units of the predicate's Q16
  lattice, so its rounding cannot refuse a step along a wall the body touches). A body touching
  or overlapping a wall is so refused only a step that goes deeper, as `settle_on_terrain` is.
  Air bodies, a centre on the wall and a map without a mesh keep the centre rule.
- `slide_along_wall(map, pathfinder, policy, from, velocity)`: the step with its part into the
  nearest wall within `terrain_radius + |step|` removed (the tangent of a straight wall, of the
  circle round a corner's vertex), when the probe admits the result.

Probe sites moved to `probe_permits`: the snap onto a goal within one step (`upkeep_route`), the
finish onto the goal after the last waypoint (`propose_step`), `aim_past_parked_body`'s aim, the
arrival step and the steered step of `step_toward`, the surround walk's side step, and
`keep_terrain_view`'s step. In `step_toward` a disc-refused step slides along the wall; it counts
as refused (the caller drops a refused route or re-routes a chase) when the slide keeps less than
half the step, the surround walk's half step.

Left on the centre test, with the reason:

- `slide_target` and `route_is_usable` (movement admission and route rules, D-4), and
  `propose_step`'s check that drops a route whose first leg terrain refuses: a route's first leg
  is held to the centre rule (`route_is_usable`, the start-triangle rule), so a disc test there
  dropped the route of a body the crowd had pushed below it, and the body then walked straight at
  its far goal for a repath cooldown (measured: pulse bot 18, 25 ticks south along the ramp E
  wall).
- `drop_passed_waypoints`' terrain view: it already tests the disc, with the wedged exemption D-4
  keeps.
- `coast` (a glide is a step, not a probe; `walking_goal` holds `&self` and no pathfinder; the
  settle slides it) and `flee_destination` (it picks a `Move` goal, which the disc-radius route
  reaches; `compute_flee_destination` and `compute_entity_flee_destination` are `&self` public
  APIs pinned by `world_api_surface_tests`).

Per-tick cost: one predicate flood per probe of a ground body, plus a contact flood and a second
sweep only when the first sweep is refused; no allocation (the predicate's retained scratch).
Throughput is measured in WP4.

**Tests.** T-B9 `wall_contact_tests::a_body_heading_past_a_wall_end_walks_round_it_without_stalling`
(pulse bot 18's geometry: touching the end of a wall 0.072 below the line round it, ordered to a
point behind it; until it is round, its step never shrinks while it accelerates, it is round within
20 ticks, never deeper): red on `702fc9b` (tick 2: stepped 0.0032 after 0.0230), green here. Unit
tests in `contact.rs`: the probe refuses a disc sweep the centre rule admits, admits a step along a
touching wall and along a spawn's overlap, refuses one deeper, and leaves air bodies to the centre
rule; the slide keeps a step's part along a straight wall and the tangent round a vertex, and none
for a step away from the wall or in the open. The four red tests of §3:
`attack_dash_tests::q0071_the_maximum_is_the_walk_not_the_straight_line`,
`movement_contact_tests::melee_pursuit_takes_the_ramp_and_reaches_the_enemy` and
`movement_contact_tests::pursuit_rounds_a_single_blocked_cell_promptly` pass; T-B5 stays red,
because pulse bot 7 dies in the lane fight (below), not at the corner.

**Measurements** (base: WP2; tools built from this tree).

| | Base (WP2) | WP3 (§3) | WP3 with the probes |
|---|---|---|---|
| Ramp W foot P0 ratio, turn (mean / max) | 1.301 / 1.473, 157° / 301° | 1.325 / 1.473, 158° / 230° | 1.333 / 1.473, 189° / 341° |
| Ramp W foot P1 | — | 1.513 / 1.528, 116° / 131° (4) | 1.510 / 1.523, 112° / 128° (5) |
| Ramp E foot P0 | 1.198 / 1.300, 190° / 454° (6) | no pass | no pass: P0's army is dead |
| Ramp E foot P1 | 1.149 / 1.168, 81° / 152° | 1.209 / 1.352, 204° / 590° | 1.154 / 1.173, 77° / 83° |
| Reversals P0 / P1 | 17 / 7 | 4 / 28 | 20 / 25 |
| Stall give-ups P0 / P1 | 0 / 0 | 0 / 0 | 1 / 0 |
| Near-stalls on a route (≥ 6 ticks under 0.005), ticks ≤ 1,000 | bot 11 in the fight | bots 17, 18 at the ramp E corner, 11, 18, 21 in the fight | bots 4, 11 in the fight |
| Deeper unit-ticks | 118 / 74 | 0 / 0 | 0 / 0 |
| Outcome | P0 wins at 1,693 | P1 wins at 2,216 | P1 wins at 1,943 |
| Seeds 1 to 8 | P0 all eight | P1 3, P0 1, 4 never end | P1 seeds 2 to 8, P0 seed 1; all end |

Matrix: bit-identical to WP2 (`--compare`); battle 33: `seeds.csv` byte-identical.

**What the probes fixed.** The ramp E corner: P1's column now crosses it as on the base (1.154 /
77° against 1.149 / 81°; pulse bot 18 rounds the vertex (38, 52) at full speed where it crawled
for ten ticks in §3), and no body near-stalls at a wall. The four seeds that did not end now end.

**The flip remains, and is the rule's.** The run diverges from the base at the same two places as
§3 and variant E: player 1's spawn (tick 9: welder drone 16, pulse bots 17 to 24, pressed into
the spawn's walls on the base and sliding here) and the ramp W wall end (29, 12) at tick 156. On
the base player 0's column cut that wall end with its centres at about (29.0, 12.1), its discs
0.3 to 0.37 into the wall; here they go round it at the disc's radius, (29.4, 11.9), and queue in
the narrower passage (pulse bot 6 steps 0.02 to 0.05 a tick for ticks 162 to 181 behind its
friends, not against a wall). Leaving the ramp W box, pulse bots 4 to 9 are 5 to 9 ticks later
than on the base (exit ticks 221, 215, 203, 190, 208, 196 against 213, 206, 198, 184, 199, 191);
at tick 340 player 0's column is 0.6 to 1.3 behind, player 1's within 0.2 to 0.5 of the base. That
delay turns the near-tied lane fight (ticks 310 to 640, round (22–28, 30–38)): player 0's ten
attackers die between 413 and 626 and player 1 keeps five pulse bots, which walk to player 0's
base. The remaining reversals are in the fight (cosine below −0.3 on position steps, ticks 304 to
606, and welder drone 16 at its spawn), the one give-up is welder drone 2 at its own base giving up
an automatic chase of pulse bot 20 beyond its leash at tick 946, after the flip, and the ramp W
turn rises in that queue. The flip now holds on seven of eight seeds, so it is systematic, not the
fight's noise: discs that may no longer overlap the wall end narrow the lane, which is the game's
rule (C1, D-1). Held under D-9, 4 for the orchestrator's decision.

**Observation, no rule change.** In §3's tree some seeds never ended once only unarmed welder
drones remained on both sides: the scenario ends a match only when a side has no units, and
workers do not attack. StarCraft II ends a melee game when a player has no structures, a separate
topic from this plan.

## 4. WP4: goldens, T-B5 restated (in progress)

Amendment 1 accepted the rule and the probe fix with the showcase's flip.

**Goldens** re-recorded on this tree, each header naming its first changed checkpoint and the
traced cause. Traced by dumping every entity's position and life after every tick on this tree and
on WP2's (`34a92e7`):

- `golden_replay`: checkpoints 1 to 3 unchanged; first difference at tick 164, "blue_infantry"
  pulse bot entity 15 at (30.34, 30.21), whose disc the crowd pressed into the wall face x = 30;
  it now stays at x = 30.375.
- `golden_replay_orders`: checkpoints 1 to 13 unchanged; first difference at tick 131,
  "blue_column" pulse bots 8, 9 and 18 round (33.0 to 33.7, 29.1 to 29.9) on their way to
  (32, 36). The test still finds both phase-order situations.

Both pass in debug, in release and on `wasm32-wasip1` under wasmtime.

**T-B5 restated** (Amendment 1): `showcase_crowd_rounds_the_ramp_e_corner_without_wedging` places
six 0.375 bodies on the showcase map where player 0's pulse bots 5 and 7 to 11 stood at the base
trace's tick 740, and attack-moves them to (52.5, 52.5) for 60 ticks. It asserts what the rule
guarantees: no body within its radius of the corner (35, 52) on any tick, no body deeper into a
wall, and the crowd's front (3 of 6) round the corner.

**Finding (not pinned, open).** In that reproduction the second body (pulse bot 7's start)
reaches the vertex at tick 25 at its radius and then queues: a friend at its north-west and the
vertex at its north-east leave its route's first leg no room, and later friends pass first. It
creeps under 0.005 a tick to tick 59 and its heading jitters (214° in total over the 60 ticks).
Its slid step is shorter than the waiting threshold but it is not set waiting, because the slide
runs after `is_waiting` in `step_toward`. That is a queue, not a wedge; whether to wait instead of
creeping there is left to the orchestrator.
