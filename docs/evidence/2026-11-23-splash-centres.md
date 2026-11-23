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
