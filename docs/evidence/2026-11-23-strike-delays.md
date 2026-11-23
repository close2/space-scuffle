# Strike delays: measurements per package

This file records the measurements of plan [`2026-11-23-strike-delays`](../plans/2026-11-23-strike-delays.md)
(§7; B-81): the effect-chain survey's material, the world comparison of every package against the WP0 tree, the
goldens and their proof, the red runs of the new tests, and the probe of the Liberator analogue. WP0 creates it;
WP1 to WP3 append.

## Survey material (§2.3)

The effect chain of every analogue weapon was traced from SC2GameData `5444e12f13` (5.0.14) by
`~/scratch/strike-delays/chain.py` (run with `python3 -I`, reading `~/scratch/combat_wp3_data/<layer>/` as data).
Its output is `~/scratch/strike-delays/chains-1.txt` and `chains-2.txt` (45 weapons) and `chains-liberator.txt`
(`LiberatorMissileLaunchers`, traced in WP0 because the planning output did not include it). The Liberator's chain
reads as the plan's S3, S5 and S6:

- `LiberatorMissileLaunchers` (`void` WeaponData:321): `Period` 1.8, `Effect` `LiberatorMissileBurstPersistent`
  (:328), no `DamagePoint`.
- `LiberatorMissileBurstPersistent` (`void` EffectData:2942): `WhichLocation TargetUnit`, `Channeled`,
  `RandomOffset`, `PeriodCount` 2, `PeriodicEffectArray` `LiberatorDamageMissileLM` twice, and one
  `PeriodicPeriodArray` entry, `0.05` (:2969).
- `LiberatorDamageMissileLM` (`void` EffectData:2979): `LaunchLocation CasterUnit`, impact
  `LiberatorMissileDamage`, no validator.

## How to reproduce

Binaries are built once per tree and run from a copy of that tree's `assets/`, so each run reads its own catalog
(`matchup_trace` embeds the catalog it was compiled with; `route_trace` is given `--assets` explicitly). The scripts
are the splash-centres plan's `measure.sh` and `compare.sh`, copied to `~/scratch/strike-delays/scripts/`, and a
`client.sh` reduced to the skirmish's stage log.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/strike-delays-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/strike-delays
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
# the probe, kept beside the world so that compare.sh compares the measured worlds only:
cd $R/tree-<wp> && $R/bin-<wp>/matchup_trace --fixture $R/liberator_probe.ron --all-battles --seeds 20 --out $R/world-<wp>-probe/probe
$R/bin-<wp>/matchup_trace --fixture $R/liberator_probe.ron --battle <battle> --seeds 20 --out $R/world-<wp>-probe/trace
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
the showcase scenario and the showcase effects scenario on their default seed and seeds 1 to 8 for 6,000 ticks.

The probe `~/scratch/strike-delays/liberator_probe.ron` is in the matrix fixture's format (seeds of record
`3203386110, 11, 22, 33`, spawn jitter 0.12; expectations open, `(0, 999)`, so that it records rather than
judges):

- `p1_4_liberators_vs_8_mutalisks`: four `sky_bastion` (2 columns, spacing 1.6, at (24, 31)) attack-move at eight
  `sonic_bat` (4 columns, spacing 0.9, at (36, 30)) attack-moving back; 1,500 ticks.
- `p2_2_liberators_vs_2_vikings_in_range`: two `sky_bastion` at (30, 31) and two `vector_interceptor` at (34, 31),
  4 apart, inside both weapons' range, each side ordered `AttackFirstEnemy`; 1,500 ticks.

## WP0: baseline

The tree of the plan (`4db76196`, behaviour-identical to `main` at `cdf4ea93`), measured into
`~/scratch/strike-delays/world-wp0`: 483 files; the matrix reports "1160 runs: expectation failures 0 on the seeds
of record, 4 over all seeds".

**Showcase unit census (§7 verify).** Over the eighteen showcase runs (`showcase.ron` and `showcase_effects.ron`,
default seed and seeds 1 to 8, 6,000 ticks), the unit samples are of five types only: `pulse_bot`,
`welder_drone`, `rhino_beetle`, `napalm_wheeler` and `railgun_crawler`. No run trains or spawns `sky_bastion`, and
the matrix spawns it in no battle.

**Probe, 24 seeds each (4 of record and 20):**

| Battle | Blue wins | Red wins | Survivors blue – red | End tick (mean) |
|---|---|---|---|---|
| p1, 4 Liberators vs 8 Mutalisks | 24 | 0 | 1.67 – 0.00 | 472.9 |
| p2, 2 Liberators vs 2 Vikings | 0 | 24 | 0.00 – 2.00 | 246.2 |

The per-seed traces (`--battle`, every tick) are in `~/scratch/strike-delays/world-wp0-probe/trace/`.

## WP1: the rule, latent

### Red runs

On the WP0 tree the new tests did not compile (`~/scratch/strike-delays/red/wp1-compile.log`): 12 × `E0609 no field
first_strike_delay_ticks` (on `WeaponSpec` and `WeaponSpecDef`, in the validation unit tests and
`checksum_state_tests`) and `E0432 unresolved import sim_fixtures::trace::strikes_on_release`. The scratch RON of
`multi_strike_timing_tests` compiled; its delay field was not read yet.

With a field-only stub (the field in `WeaponSpec`, `WeaponSpecDef`, its conversion and its encoding; the trace's
old rule moved into `strikes_on_release` unchanged; no rule), `~/scratch/strike-delays/red/wp1-stub.log`:

- `multi_strike_timing_tests`: 8 of the new tests failed, the 10 existing passed. Each delayed trace landed its
  first strike on the release tick ("a single strike, delay 1, tick 0: expected 0, got 10"; the same for the
  volley and the spaced burst), "no strike on the release tick", "premise: nothing has landed yet" (both order
  tests), and "the first strike is lost on a cloaked target, the second lands: damage 20". The projectile test
  failed on its own premise (three attacks fit the window, not two); the premise was corrected to "at least two".
- `catalog::validation::strike_delay_tests`: 3 of 5 failed (the delay counted in the bound; a delayed volley
  naming a distance; a delay on a suicide weapon). The single-strike and omitted-field tests passed on the stub,
  as they must: they pin what the stub already allows.
- `trace_tests::a_delayed_instant_weapons_release_replays_no_strike` failed.
- `checksum_state_tests::a_projectiles_first_strike_delay_changes_the_world_checksum` passed on the stub, which
  already hashed the field; it guards the encoding.

After the rule all of them pass, and WP5's existing tests in `multi_strike_timing_tests.rs` pass unchanged.

### Golden proof

1. With the field left out of the encoding (`unhashed`) at schema 36, `golden_replay_tests` (both fixtures, 4
   tests) and `fixture_tests` (`DEFAULT_SANDBOX_CHECKSUM` 10_589_561_826_036_639_628) passed on native debug,
   native release, `wasm32-wasip1` release and `wasm32-wasip1` debug
   (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<worktree>"`), logs in
   `~/scratch/strike-delays/proof-unhashed36/`.
2. With the field hashed after `attacks_count` and schema 37 (the next free number on `main` at `cdf4ea93`), both
   fixtures were re-recorded under the plan's header and the sandbox checksum became 18_404_512_489_137_112_834. All
   four builds reproduce them (`~/scratch/strike-delays/proof-hashed37/`).

### Worlds

`compare.sh world-wp0 world-wp1`: "files base=483 new=483 differing=0". The probe's `matrix.csv` is byte-identical
to WP0's. Stop rule 1 holds.
