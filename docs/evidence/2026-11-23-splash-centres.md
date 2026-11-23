# Splash centres: measurements per package

This file records the measurements of plan [`2026-11-23-splash-centres`](../plans/2026-11-23-splash-centres.md)
(§7; B-83, B-84, B-169): the world comparison of every package against the WP0 tree, the goldens and their proof,
the red runs of the new tests, and the traced Baneling bursts. WP0 creates it; WP1 to WP5 append.

## How to reproduce

Binaries are built once per tree and run from a copy of that tree's `assets/`, so each run reads its own catalog
(`matchup_trace` embeds the catalog it was compiled with; `route_trace` is given `--assets` explicitly). The scripts
are the structure-attributes plan's, copied to `~/scratch/splash-centres/scripts/`, with one addition to
`measure.sh`: `route_trace --scenario scenarios/showcase_effects.ron` on its default seed and seeds 1 to 8 for 6,000
ticks (`fx-*`), the one shipped scenario with sieged tanks and town halls.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/splash-centres-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/splash-centres
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
the showcase scenario and the showcase effects scenario on their default seed and seeds 1 to 8 for 6,000 ticks.

## WP0: baseline

The tree of the plan and Q0096 (`e97d826a`, behaviour-identical to `main` at `e7153b8b`), measured into
`~/scratch/splash-centres/world-wp0`.
483 files; the matrix reports "1160 runs: expectation failures 0 on the seeds of record, 4 over all seeds", and its
three Baneling battles (20 seeds each) read as the plan's baseline (§7):

| Battle | Blue wins | Red wins | Both gone | Survivors | End tick (s) |
|---|---|---|---|---|---|
| 11a, 2 Banelings vs 5 marines | 0 | 9 | 11 | 0.00 – 2.25 | 7.8 |
| 11b, 2 Banelings vs a Marauder | 0 | 20 | 0 | 0.00 – 1.00 | 7.3 |
| 32, 30 marines vs 15 Banelings | 4 | 15 | 1 | 0.75 – 0.90 | 53.8 |

## WP1: the two rules, latent

### Red run

On the WP0 code the new tests did not compile (logs `~/scratch/splash-centres/red/`):

| Target | Errors |
|---|---|
| `splash_centre_tests` (5 tests: P-1, no direct strike, P-3, point impact, the unlock refusal) | 7: no `sim_types::SplashImpact`, no field `impact` or `target_radius_below` on `SplashSpec` |
| `sim_types` lib tests (`a_splash_defaults_to_its_target_and_no_condition`, the literals) | 9, the same |
| `sim_core` lib tests (`catalog::validation::splash_centre_tests`, 6 tests; `combat_resolution::tests::an_attacker_centred_area_lands_without_its_target`) | 13: no fields on `SplashSpecDef` or `SplashSpec` |
| `checksum_state_tests::embedded_weapon_fields_and_splash_are_sensitive` | 9 at compile time; with the code in and the two fields still unhashed, it failed with "checksum omitted impact" |

### Golden proof

`~/scratch/splash-centres/proof/run.sh` replays both goldens (`golden_replay_tests`) and the sandbox
(`fixture_tests default_sandbox_spawns_both_armies_and_bases_whole`) on native debug and release and on
`wasm32-wasip1` release and debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<worktree>"`):

| Tree | Native debug | Native release | wasm32-wasip1 debug | wasm32-wasip1 release |
|---|---|---|---|---|
| WP1 code, `impact` and `target_radius_below` in `unhashed`, schema 35, fixtures as recorded on `e7153b8b` | pass | pass | pass | pass |
| The fields hashed, schema 36, both fixtures and `DEFAULT_SANDBOX_CHECKSUM` re-recorded | pass | pass | pass | pass |

`DEFAULT_SANDBOX_CHECKSUM`: `14_665_724_469_072_757_732` at schema 35 (reproduced with the fields unhashed),
`10_589_561_826_036_639_628` at schema 36.

### Worlds

`compare.sh world-wp0 world-wp1`: `files base=483 new=483 differing=0` (matrix, battles 31 and 33, showcase and
showcase effects on their default seed and seeds 1–8). Every test of `sim_types`, `sim_core`, `sim_fixtures` and
`workspace_checks` passed on the WP1 tree (109 test executables and doc tests).

## WP2: the Baneling's burst (B-83)

### Red run (on the WP1 tree, before the data)

Logs `~/scratch/splash-centres/red/wp2-*.log`:

| Test | Result | Failure |
|---|---|---|
| `weapon_catalog_rules_tests::splash_impact_and_condition_follow_starcraft` | failed | `volatile_tick`: `(Target, None)`, StarCraft II `(Attacker, None)` |
| `weapon_catalog_rules_tests::q0063_splash_offset_follows_starcraft` (Baneling row re-derived to `false`, D-2) | failed | `volatile_tick`: `true`, expected `false` |
| `splash_centre_tests::the_banelings_burst_is_centred_on_it` (P-1, P-2) | failed | "behind the Baneling, inside its reach" |
| `runtime_catalog_tests::an_edited_splash_impact_moves_the_burst` (P-7) | failed | "the shipped catalog centres an area on its attacker" |

All four pass with the data of D-4.

### Worlds

`compare.sh world-wp0 world-wp2`: `files base=483 new=483 differing=2`, the two matrix files, and in them only
battle 32's rows. Battles 11a and 11b are byte-identical on all 20 seeds, as P-8 predicted (11b: the Marauder is
always inside the burst; 11a: the five marines stand within the burst under either centre). Every other battle,
battles 31 and 33, the showcase and the showcase effects scenario are byte-identical. Goldens and the sandbox are
unchanged.

| Battle 32, 20 seeds | Blue (marines) wins | Red (Banelings) wins | Both gone | Survivors (blue – red) | End (s) |
|---|---|---|---|---|---|
| WP0 | 4 | 15 | 1 | 0.75 – 0.90 | 53.8 |
| WP2 | 16 | 3 | 1 | 3.80 – 0.20 | 53.0 |

The direction is P-8's: fewer marines struck per burst, more marine survivors, fewer Baneling wins. The matrix
still reports "expectation failures 0 on the seeds of record, 4 over all seeds"; battle 32's expectation (blue 0 to
15 survivors) holds on every seed, so it is not re-recorded. Its claim, "Banelings trade far above their cost
against clumped marines", still describes the result: 15 Banelings (750 resources) kill on average 26.2 of 30
marines (1,310 resources).

### Traced bursts

`~/scratch/splash-centres/tools/burst.py` on `matchup_trace --battle` of both trees (seeds of record; full output
`~/scratch/splash-centres/trace-b32.txt`). On every seed the first differing tick is a Baneling burst, the target
takes the same 35 (16 + 19 Light) in both trees, and the units no longer struck are exactly those whose edge lies
beyond 2.2 of the Baneling's centre while within 2.2 of the target's near edge.

| Seed | First differing tick | Marines struck before | After | Not struck any more (edge from the Baneling's centre / from the target's near edge) |
|---|---|---|---|---|
| 11 | 14 (Baneling 40 on marine 30) | 10 | 7 | 18 (2.700 / 2.189), 23 (2.530 / 1.897), 28 (2.381 / 1.664) |
| 22 | 13 (two Banelings) | 17 | 12 | 4, 8, 13, 18, 23 (edges 2.339 to 2.851 from the nearer Baneling) |
| 33 | 12 | 9 | 6 | three marines beyond 2.2 of the Baneling |
| 3203386110 | 13 (two Banelings) | 13 | 8 | five marines beyond 2.2 of the Banelings |

The 11a and 11b traces of the seeds of record are byte-identical between the trees.

## WP3: the Siege Tank's large targets (B-84)

### Red run (on the WP2 tree)

Logs `~/scratch/splash-centres/red/wp3-*.log`:

| Test | Result | Failure |
|---|---|---|
| `weapon_catalog_rules_tests::splash_impact_and_condition_follow_starcraft` (Siege Tank cell `Some(1.25)`) | failed | `railgun_crawler` Crucio Shock Cannon: `(Target, None)`, StarCraft II `(Target, Some(1.25))` |
| `splash_centre_tests::a_sieged_shot_at_a_large_structure_has_no_area`, `…_at_a_small_structure_splashes`, `no_shipped_ground_unit_is_too_large_for_the_sieged_area` (P-3, P-4) | failed | "the sieged cannon's area is conditional (B-84)": the shipped cannon had no condition |
| `runtime_catalog_tests::an_edited_target_radius_condition_opens_the_sieged_area` (P-7), run against the WP2 `units.ron` | failed | "the shipped catalog has the condition" |
| `client` `effect_catalog_tests::every_shipped_weapon_names_the_looks_of_the_event_it_publishes` (D-8 item 2), with the condition in the data and no impact look | failed | "railgun_crawler Primary Some("siege_mode") Hitscan: a conditional area names an impact for the shots it does not open on" |

All pass with the data of D-4 and D-8 item 2. P-4 holds on the shipped data: every open-ground structure below
radius 1.25 opens the area (one `SplashVisual`, a bystander touching its footprint struck); the Supply Depot
analogue and every larger one open none (no `SplashVisual`, the bystander untouched where one fits within 1.25 of the
centre); no shipped mobile ground unit has radius 1.25 or more.

### Worlds

`compare.sh world-wp2 world-wp3`: `files base=483 new=483 differing=0`. No measured world moved, the showcase effects
scenario included (no sieged tank there fires at a town hall in 6,000 ticks on any of its nine seeds). Against WP0 the
only differences are WP2's battle 32 rows.
