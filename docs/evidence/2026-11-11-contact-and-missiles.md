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
