# Splash cliff levels: measurements per package

This file records the measurements of plan [`2026-11-23-splash-cliff-levels`](../plans/2026-11-23-splash-cliff-levels.md)
(§7; B-170): the world comparison of every package against the WP0 tree, the probe of §7.2, the goldens and their
proof, the red runs of the new tests, and the traced first differences. WP0 creates it; WP1 to WP5 append.

## How to reproduce

Binaries are built once per tree and run from a copy of that tree's `assets/`, so each run reads its own catalog
(`matchup_trace` embeds the catalog it was compiled with; `route_trace` is given `--assets` explicitly). The scripts
are the splash-centres plan's, copied unchanged to `~/scratch/splash-cliff/scripts/`.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/splash-cliff-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/splash-cliff
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
the showcase scenario and the showcase effects scenario on their default seed and seeds 1 to 8 for 6,000 ticks.

## WP0: baseline and probe

### Baseline

The plan's tree (`6f67d7ac`, behaviour-identical to `main` at `d0fd365b`), measured into
`~/scratch/splash-cliff/world-wp0`: 483 files; the matrix reports "1160 runs: expectation failures 0 on the seeds of
record, 4 over all seeds" (as splash-centres' WP4 tree).

### Probe (§7.2)

A scratch patch, never committed (`~/scratch/splash-cliff/probe/apply.py` and the diff it produced,
`probe/probe.diff`), prints from `strike_area` every unit an area strikes, and marks each whose cell level differs from
the level of its nearest search point (`PROBE-X` for a circle, the areas WP4 keeps to their level; `PROBE-L` for a
line, which keeps `Any`), and from the attacker-centred branch every burst whose target stands on another cell level
than the attacker (`PROBE-B`). Its `route_trace` also prints each side's units by type at the end of the run and the
tick of the last death. The probe binaries (`bin-probe-wp0`) played every world of §7.1 into
`~/scratch/splash-cliff/probe-wp0`; their output files are byte-identical to the baseline's (`compare.sh`: only the
logs differ, by the probe's lines).

| World | Seeds | Area strikes (all) | Cross-level circle strikes | Cross-level line strikes | Bursts on another level |
|---|---|---:|---:|---:|---:|
| Matrix | of record and 16 more (1,160 runs) | 10,680 | 0 | 0 | 0 |
| Battle 31 | 100 | 0 | 0 | 0 | 0 |
| Battle 33 | 64 | 0 | 0 | 0 | 0 |
| Showcase | default, 1–8 | 0 | 0 | 0 | 0 |
| Showcase effects | default | 18 | 0 | 0 | 0 |
| Showcase effects | 1, 2, 3, 4, 5, 6, 7, 8 | 23, 23, 24, 25, 22, 24, 21, 21 | 0 | 0 | 0 |

**Every count is zero, so WP3 runs** (§7.2's rule): battle 40 is added before the rule's data lands, and WP4's move is
measured on it.

The plan expected nonzero counts in the showcase effects scenario (§7.2). They are zero: every one of its 18 to 25 area
strikes per seed lands between ticks 305 and 756 on units on the level of the search point (the default seed's last
death is at tick 811): no shell centred on a unit reached a unit on another cell level. So WP4 is expected to leave every measured world byte-identical, and
P-8 is judged on battle 40 alone.

Survivors at tick 6,000 (from the probe's `route_trace`; the route trace itself logs only moving units), every seed
alike: player 0 keeps 3 `railgun_crawler`, 2 `burrow_scorpion` and its town hall; player 1 keeps its town hall. Last
death: tick 811 (default), 788, 789, 785, 810, 811, 787, 819, 814 (seeds 1 to 8).

## WP1: a position's cliff level

### Red run

On the WP0 tree the new tests did not compile (`~/scratch/splash-cliff/red/wp1.log`): 22 errors E0599, "no method
named `cliff_level_at` found for struct `GameMap`", in the four `grid.rs` unit tests and
`ramp_profile_tests::cliff_level_at_reads_the_lower_landing_on_every_ramp_cell`. With the function in, all five
pass, and every other test of `ramp_profile_tests` passes as before.

### Worlds

`compare.sh world-wp0 world-wp1`: `files base=483 new=483 differing=0`. The function has no caller yet.

## WP2: the two rules, latent

### Red run

On the WP1 tree the new tests did not compile (`~/scratch/splash-cliff/red/wp2.log`, 47 errors): no
`sim_types::SplashCliff`, no field `cliff` or `strike_target_on_other_level` on `SplashSpec` or `SplashSpecDef`, in
`splash_cliff_tests` (9 tests: P-1 at a cliff edge for a target above and below, P-1 on a ramp, the target struck on
any level, P-2 against a flat map, a line kept to its level per search point, an air area over each level, P-3 across
a cliff, on its own level and at a structure), the `sim_types` unit test of the defaults, the five
`catalog::validation::splash_cliff_tests`, `combat_resolution::tests::the_other_level_strike_keeps_the_areas_filters`
and `checksum_state_tests`. With the code in and the two fields still unhashed,
`checksum_state_tests::embedded_weapon_fields_and_splash_are_sensitive` failed with "checksum omitted cliff"
(`red/wp2-checksum-unhashed.log`).

A mutation check: with the level rule and the other-level strike disabled in `combat_resolution.rs` (scratch, not
kept), 7 of the 9 `splash_cliff_tests` failed; the two that passed are P-2's (which the rule must not change) and the
structure case, which was then extended with the strike switched off (the structure untouched), so it fails too.

### Golden proof

`~/scratch/splash-cliff/proof/run.sh` replays both goldens (`golden_replay_tests`) and the sandbox
(`fixture_tests default_sandbox_spawns_both_armies_and_bases_whole`) on native debug and release and on
`wasm32-wasip1` release and debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<worktree>"`):

| Tree | Native debug | Native release | wasm32-wasip1 debug | wasm32-wasip1 release |
|---|---|---|---|---|
| WP2 code and data, `cliff` and `strike_target_on_other_level` in `unhashed`, schema 36, fixtures as recorded on `d0fd365b` | pass | pass | pass | pass |
| The fields hashed, schema 37, both fixtures and `DEFAULT_SANDBOX_CHECKSUM` re-recorded | pass | pass | pass | pass |

`DEFAULT_SANDBOX_CHECKSUM`: `10_589_561_826_036_639_628` at schema 36 (reproduced with the fields unhashed),
`18_404_512_489_137_112_834` at schema 37.
