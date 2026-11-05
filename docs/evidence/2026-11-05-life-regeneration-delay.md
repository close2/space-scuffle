# Life regeneration delay: measurements per step

This file records the measurement of every step of plan
[`2026-11-05-life-regeneration-delay`](../plans/2026-11-05-life-regeneration-delay.md) (D-8): the whole matchup
matrix on 20 seeds compared with the step before, battles `22a_reaper_vs_2_workers` and `22b_reaper_vs_stalker`
traced tick by tick on the same 20 seeds, battle 33 on 68 seeds, and the golden replays. WP1 creates it with the
baseline (section 1) and its own section; WP2 and WP3 append theirs in the commit that makes the change.

## How to reproduce

The tool and its outputs are those of the combat alignment evidence,
[How to reproduce](2026-11-04-combat-sc2-alignment.md#how-to-reproduce): `crates/sim_core/examples/matchup_trace.rs`,
always a release build. Per step:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/life-regen
cargo build --release --locked -p sim_core --example matchup_trace
TOOL=$CARGO_TARGET_DIR/release/examples/matchup_trace
# Whole matrix: the four seeds of record plus 16 more, 20 seeds per battle; outcomes only.
$TOOL --all-battles --seeds 16 --jobs 8 --out ~/scratch/life_regen/<step>/matrix \
    [--compare ~/scratch/life_regen/<previous step>/matrix/matrix.csv]
# Battle 33: the four seeds of record plus 64 more, 68 seeds.
$TOOL --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/life_regen/<step>/b33
# The two reaper battles, traced tick by tick on the matrix's 20 seeds.
$TOOL --battle 22a_reaper_vs_2_workers --seeds 16 --out ~/scratch/life_regen/<step>/b22a
$TOOL --battle 22b_reaper_vs_stalker --seeds 16 --out ~/scratch/life_regen/<step>/b22b
```

`--seeds <n>` plays the fixture's seeds of record and then the first `n` positive integers that are not seeds of
record, so `--seeds 16` gives 20 seeds per battle and `--seeds 64` gives battle 33's 68.

- **Regained life of the reaper.** Blue is the reaper's side in both battles. From each seed's
  `seed_<n>_ticks.csv`: the sum of `blue_health_regained` over the whole battle, the same sum over the ticks after
  the first tick with `blue_damage_taken > 0` (life regained after the reaper was first hit), and the first tick
  after that hit on which it regains any. `blue_health` in `seeds.csv` is the reaper's life at the end, rounded to
  one decimal; `matrix.csv` has it exactly.
- **Battle 33's figures** come from its `seeds.csv` (wins, survivors and their mean and SD, the seeds of record).
- The script that computes these (`summarize.py`) is a throwaway kept beside the outputs on the development server;
  it only sums and averages those columns.

## 1. Baseline

**Commit.** `main` at `b862c29` (combat alignment WP17 merged, battle 33 expectation `blue: (0, 9), red: (0, 3)`),
measured on this plan's branch before WP1 (WP0 changes only a question file). Outputs under
`~/scratch/life_regen/base/` on the development server.

- **Whole matrix, 20 seeds:** 50 battles, 1000 runs; expectation failures 0 on the seeds of record and 2 over all
  seeds (both battle 33's). `matrix.csv` is **byte-identical** to the last measurement of combat alignment WP17
  (commit f, [§14.7](2026-11-04-combat-sc2-alignment.md#147-commit-f-the-42-hull-rates)), so the branch reproduces
  `main`.
- **Battle 33, 68 seeds:** byte-identical to WP17 commit f: 39 / 20 / 9 (marine wins / zealot wins / unresolved),
  marine survivors 3.87 (SD 3.43), zealot survivors 1.01 (SD 1.37). Seeds of record (marines – zealots):
  3203386110 0-3, 11 2-0, 22 1-2 (unresolved), 33 9-0.
- **Battle 22a (reaper against 2 workers), 20 seeds:** reaper win on every seed at tick 101, the reaper's life at
  the end 46.22 on average (of 60). It is first hit on tick 22 to 25 and regains life on the next tick or the one
  after, and from then on every tick: 5.84 to 6.08 life regained per seed (0.08 per tick), all of it in combat.
- **Battle 22b (reaper against a stalker), 20 seeds:** stalker win on every seed at tick 172, the stalker's life at
  the end 80.00 (shields gone). The reaper is first hit on tick 7 and regains 0.08 life on every tick from tick 8
  on: 12.80 per seed, all of it in combat.
- **Golden replays:** unchanged (this is `main`).

## 2. WP1: life regeneration waits for a per-archetype catalog delay, 0 everywhere (Q0049)

**Commit.** The WP1 commit of the plan, on the baseline of §1. `hp_regen_delay_ticks: u16` (default 0, ticks at 25
per Faster second) in `UnitArchetypeDef`, `StructureArchetypeDef` and `UnitArchetype`; validation rejects a
nonzero delay on an archetype whose `hp_regen_per_tick` is zero (typed `CatalogInvalidValue` naming the field). In
`upkeep_entity` the `ticks_since_last_damage` increment moves to the top of the vitals block, and life regenerates
only when the counter is at least the archetype's delay (plan D-4). No shipped archetype sets the field, so every
archetype reads 0. Outputs under `~/scratch/life_regen/wp1/`.

**Tests first.** Before the field existed none of the new tests compiled. With the field added and neither the
validation rule nor the upkeep change in place:

- `catalog::validation` unit tests: `a_unit_without_life_regeneration_rejects_a_life_regeneration_delay` and
  `a_structure_without_life_regeneration_rejects_a_life_regeneration_delay` failed (a unit and a structure without
  life regeneration validated with a delay of 10); `an_omitted_life_regeneration_delay_reads_as_zero` passed.
- `damage_and_shields_tests.rs`: `a_scratch_archetype_with_a_10_tick_life_delay_regenerates_from_the_10th_tick_after_damage`
  and `shield_damage_alone_restarts_the_life_regeneration_delay` failed with `life must hold on tick 1`;
  `life_regeneration_without_a_delay_runs_on_the_tick_after_damage` (the Zerg guard) and
  `a_unit_never_damaged_regenerates_life_at_once` passed, as the plan expected.

All seven pass with the change.

**Measured.**

- **Whole matrix, 20 seeds: bit-identical** to §1 (all 1000 rows of `matrix.csv`). Expectation failures stay 0 on the
  seeds of record and 2 over all seeds (battle 33).
- **Battles 22a and 22b: bit-identical** (every output file of both traces byte-identical to §1).
- **Battle 33, 68 seeds: bit-identical** (every output file byte-identical to §1).
- **Golden replays and the sandbox checksum: unchanged.** `golden_replay_tests` and `sim_fixtures`'
  `fixture_tests` pass as recorded; `CHECKSUM_SCHEMA_VERSION` stays 10 (no new entity state, plan D-7). Release and
  `wasm32-wasip1` are run by `cargo gate package` at the end of the plan.

**Why nothing moved** (plan D-4): between the counter's old and new position the block reads only life, energy,
the active mode and the archetype, so the shield rule sees the same counter, and with a delay of 0 the life
condition `counter >= 0` always holds.

## 3. WP2: the Reaper analogue regenerates life only after 179 ticks without damage (Q0049)

**Commit.** The WP2 commit of the plan, after WP1 (§2). One catalog value:

| Field | Before | After | Reference |
|---|---|---|---|
| `jet_jumper` `hp_regen_delay_ticks` | 0 (omitted) | **179** | Reaper `LifeRegenDelay` 10 Normal s (`void` UnitData:1475, also `swarmmulti` UnitData:893) = 7.14 Faster s = 178.57 ticks, rounded halves up |

The rate stays 0.08 per tick (2.0 per Faster second) until WP3. Doc 07 §3.1 row 4 now states the regeneration
and its delay. Outputs under `~/scratch/life_regen/wp2/`.

**Test first.** `damage_and_shields_tests::test_life_regeneration_waits_for_the_catalog_delay` finds the shipped
archetype with the largest `hp_regen_delay_ticks` (by property, not by slug), damages its life by 20 and requires
life to hold for `delay - 1` ticks and rise by exactly the archetype's rate on tick `delay`. Before the data edit
it failed with `some shipped archetype must wait before regenerating life`; it passes after it (179 ticks, 0.08
gained on tick 179).

**Test restated** in the same commit: WP1's `an_omitted_life_regeneration_delay_reads_as_zero` asserted that every
shipped archetype reads 0, true only before this commit. It now removes every authored `hp_regen_delay_ticks` line
from the shipped text and asserts that all archetypes then read 0, which is the property its name states. It passes
before and after the data edit.

**Measured.**

- **Whole matrix, 20 seeds:** against §2, exactly the two reaper battles moved, every seed of each. Expectation
  failures stay 0 on the seeds of record and 2 over all seeds (battle 33's). No winner changes.

  | Battle | Seeds moved | Survivors blue – red | End tick | Lead hull blue / red |
  |---|---|---|---|---|
  | 22a_reaper_vs_2_workers | 20 | 1.00 – 0.00, same | 101.0, same | 46.22 → 40.00 / — |
  | 22b_reaper_vs_stalker | 20 | 0.00 – 1.00, same | 172.0 → 139.0 | — / 80.00, same |

- **Battle 22a:** reaper win with both workers dead on all 20 seeds, as before, at tick 101. The reaper ends at
  exactly 40.00 life on every seed (60 less the workers' 20 damage) instead of 46.08 to 46.32: it regains nothing
  after its first hit (tick 22 to 25), where it regained 0.08 on every tick from the next tick on. The fall is
  0.08 × 76 to 79 ticks per seed (for example seed 3203386110: first hit on tick 25, 76 ticks to 101, 6.08), mean
  6.22, the plan's "few points". The per-tick `blue_health_regained` sum read 5.84 to 6.08 before, slightly less
  than the life actually regained, because the column misses ticks on which a hit and a regeneration coincide; it
  reads 0 on every tick of every seed after this commit.
- **Battle 22b:** stalker win on all 20 seeds, as before, at tick **139** instead of 172 (33 ticks sooner, within the
  baseline's end tick as the plan required); the stalker ends at 80.00 life, its shields gone, as before. The reaper
  regained 12.80 life in combat before (0.08 per tick from tick 8, after its first hit on tick 7); it now regains
  none: `blue_health_regained` is 0 on every tick of every seed.
- **Battle 33, 68 seeds: bit-identical** to §2 (every output file byte-identical): no reaper takes part.
- **Golden replays and the sandbox checksum: unchanged** (no golden replay spawns or trains `jet_jumper`; the sandbox
  checksum is of the spawned state at full life, plan D-7). No expectation in `matchup_matrix.ron` moved.

## 4. WP3: audit R1, the Reaper analogue's life regeneration rate (Q0039)

**Commit.** The WP3 commit of the plan, after WP2 (§3). The catalog-time audit's deferred group R1
([`2026-11-04-catalog-time-audit.md`](2026-11-04-catalog-time-audit.md) §5 R1; combat alignment evidence
[§13.11](2026-11-04-combat-sc2-alignment.md#1311-r1-the-reapers-life-regeneration-deferred)), applied after its
delay, which the audit's §7 measurement required:

| Field | Before | After | Reference |
|---|---|---|---|
| `jet_jumper` `hp_regen_per_tick` | 0.08 | **0.112** | Reaper `LifeRegenRate` 2 per Normal s (`void` UnitData:1476, also `swarmmulti` UnitData:894) = 2.8 per Faster s = 2 × 1.4 ÷ 25 = 0.112 per tick (exact); class B, a Normal-time value copied unconverted (Q0039) |

Doc 07 §3.1 row 4 now says 2.8 life per second (WP2 wrote the old 2.0). Outputs under `~/scratch/life_regen/wp3/`.

**Test.** No new test (plan WP3 step 1): `test_life_regeneration_waits_for_the_catalog_delay` reads the rate from
the archetype and keeps passing. It now records the new rate: the reaper analogue's life holds for 178 ticks after
20 damage and rises on tick 179 by exactly its `hp_regen_per_tick`, 0.112 authored, 0.1120000034570694 after
`SimScalar::from_f32_lossy` (the `f32` nearest 0.112, which Q32.32 holds exactly).

**Measured.**

- **Whole matrix, 20 seeds: bit-identical** to §3 (all 1000 rows). Expectation failures stay 0 on the seeds of record
  and 2 over all seeds (battle 33's).
- **Battles 22a and 22b: bit-identical** (every output file of both traces byte-identical to §3). Both end at a kill
  well inside 179 ticks of the reaper's last hit, so the faster rate never applies: `blue_health_regained` is 0 on
  every tick of every seed, and 22b stays a stalker win on all 20 seeds at tick 139. The audit's §7 failure (22b
  unresolved on all 20 seeds with 0.112 and no delay) does not occur.
- **Battle 33, 68 seeds: bit-identical** to §3.
- **Golden replays and the sandbox checksum: unchanged.**

## 5. Summary

- **Applied:** the per-archetype life-regeneration delay (WP1, latent), `jet_jumper`'s 179 ticks (WP2) and the
  audit's group R1, 0.112 per tick (WP3). Doc 03 §1.1 and doc 07 §3.1 row 4 describe both; Q0049 is closed.
- **Moved:** battles 22a and 22b only, both in WP2, every seed, no winner and no expectation; nothing in WP1 or
  WP3. Battle 33 is bit-identical through the whole plan (39 / 20 / 9, marine survivors 3.87), so its comment in
  `matchup_matrix.ron` is unchanged.
- **Golden replays and the sandbox checksum:** unchanged in every commit. `cargo gate package` on the final tree ran
  both golden replays in debug, release and `wasm32-wasip1` and `cargo check` on `wasm32-unknown-unknown`, all
  green.
- **Against the plan's expectations:** none. WP2's 22a fall in lead life is 6.22 on average (the plan expected "a
  few points"), and 22b ends 33 ticks sooner.
