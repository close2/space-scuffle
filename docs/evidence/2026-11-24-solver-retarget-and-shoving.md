# Evidence: Retargeting Round a Full Ring and No Shoving of Enemies (B-192, B-193)

Measurements of plan [`2026-11-24-solver-retarget-and-shoving`](../plans/2026-11-24-solver-retarget-and-shoving.md).
Branch `solver-followup`, cut from `main` `e5f5c565` (contact solver default, checksum schema 46).
Raw outputs are under `~/scratch/solver-followup/` (`matrix/`, `runs/`, `perf/`, `red/`).

## 1. Method

- **Binaries.** One release build per arm (`build.sh <tag>`): the matchup trace, the movement harness and one
  single-scenario throughput binary per scenario. Switch variants of the harness (which embeds its catalog) are
  separate builds with `movement_rules.ron` edited for the build only.
- **Catalogs.** The matrix runs read a copied catalog (`--catalog-dir assets-<tag>-<pipeline>/catalog`); Legacy is
  the same copy with `pipeline: Legacy`.
- **Seeds.** "24 seeds": the four seeds of record (3203386110, 11, 22, 33) plus the tool's first 20 non-record
  seeds (`--seeds 20`: 1 to 10 and 12 to 21). "Held-out matrix seeds 21 to 44": the tool's 21st to 44th non-record
  seeds, which are 23 to 32 and 34 to 47 (22 and 33 are seeds of record). Harness: development range seeds 1 to
  1,000; held-out range 9,000,001 to 9,001,000 (solver plan Amendment 16).
- **Locking.** Every run is one command under `flock ~/scratch/gate.lock`, one at a time.
- **Checks.** `scripts/matrix_checks.py` computes §6.1's checks per battle from `matrix.csv`; "beyond noise" is the
  candidate's per-seed mean outside the base's 95 % bootstrap interval of the per-seed mean (2,000 resamples of the
  24 seeds), on the worse side.
- **New diagnostics (WP0).** `crates/sim_fixtures/src/trace/contact_checks.rs` and `harness/room.rs`:
  - D-SHOVE: distance a unit moved during a tick in which, at the tick's start, its attack target lay in its band
    (the solver's "engaged") or it had no order, `Idle` or `Stop`, it was not holding, and an enemy body of its
    class lay within the radius sum plus 1/64.
  - D-R10: unit-ticks standing (no displacement, zero velocity) on an `AttackTarget`/`Acquired` chase whose target
    lies beyond its band, while another enemy within scan range has room on its ring (radius band − r/2, as on the
    stopped branch), classified by the best candidate: free > unruled > terrain > leash. Unruled: the unit's
    bearing point on that ring is beyond the leash or not connected ground. Leash and terrain: every sampled free
    slot (every half radius of arc) is beyond the leash, or within it but not connected ground. A candidate whose
    band the unit is already in counts free. Visibility and the ring walk are not modelled.
  - D-CHURN: returns A → B → A within 25 ticks; explained when A's ring had room for the unit at the return tick's
    start or A outranks B (threat, attack target priority, primary weapon).
  - Counters for B1 (`Activity::Blocked` unit-ticks), B3 (largest enemy overlap, fraction of the radius sum, at the
    end of any tick) and B7 (releases read from rising cooldowns, and living unit-ticks).

## 2. WP0: baseline of record

**Re-derived on the merged base.** The first baseline was measured on `e5f5c565` (binaries `*-wp0`). `main` then
moved to `708576ac` (attacker snapshot; turret wind-up WP3, which changes behaviour in both pipelines), and the
coordinator asked for the merge before the golden re-record. The branch merged `main` (`485b27e5`) and every
arm was rebuilt on it: `b0` is `708576ac` plus WP0's diagnostics (no E-1), `b1` the merged branch head (E-1),
`b2` the WP2 tree. Every figure below and after is from those builds; the `*-wp0` runs are kept in
`matrix/wp0-*` and are not used. The close-battle list is the same on both bases.

### 2.1 Matrix, Solver pipeline, 24 seeds (`matrix/b0-solver`)

1,464 runs; expectation failures 2 on the seeds of record (battle 28, seeds 11 and 22, the B-192 expected
failure) and 31 over all seeds. Held-out seeds (`matrix/b0-solver-held`): 20 failures over 1,464 runs. Legacy
pipeline, 24 seeds (`matrix/b0-legacy`, the full matrix): 3 failures of record, 26 over all seeds.

Contact checks of the crowd battles (sums over the 24 seeds; shove is the per-seed mean, world units):

| Battle | seeds | blue/red wins | blue shove | red shove | R10 leash/terrain/unruled/free | churn expl/unexpl | blocked | give-ups | max enemy overlap % |
|---|---|---|---|---|---|---|---|---|---|
| 26_100_lings_vs_6_colossi | 24 | 18/6 | 36.57 | 4.89 | 0/0/0/69561 | 290/59 | 98799 | 0 | 2.59 |
| 27_100_lings_vs_8_ultralisks | 24 | 0/24 | 29.30 | 2.38 | 0/0/0/22579 | 937/242 | 27686 | 0 | 2.45 |
| 28_100_lings_vs_10_immortals | 24 | 5/19 | 17.99 | 2.87 | 0/0/0/318843 | 143/100 | 389019 | 0 | 1.42 |
| 29_100_lings_vs_12_stalkers | 24 | 24/0 | 20.11 | 3.84 | 0/0/0/292449 | 177/210 | 467317 | 0 | 2.77 |
| 32_30_marines_vs_15_banelings | 24 | 19/3 | 0.00 | 0.00 | 0/0/0/48 | 1/0 | 56 | 0 | 0.00 |
| 33_24_marines_vs_12_zealots | 24 | 24/0 | 0.01 | 0.14 | 0/0/0/11592 | 7/0 | 11141 | 0 | 0.00 |
| 36a_storm_turns_marines | 24 | 24/0 | 0.21 | 0.00 | 0/0/0/2251 | 2/0 | 1385 | 0 | 0.00 |
| 36b_marines_beat_zealots_without_storm | 24 | 0/24 | 0.04 | 0.00 | 0/0/0/2380 | 2/0 | 2500 | 0 | 0.00 |
| 38a_fungal_turns_zerglings | 24 | 24/0 | 1.87 | 0.00 | 0/0/0/9788 | 0/1 | 9706 | 0 | 0.94 |
| 38b_marines_beat_zerglings_without_fungal | 24 | 0/7 | 1.09 | 0.00 | 0/0/0/9663 | 1/1 | 9211 | 0 | 1.19 |

Legacy on the same seeds:

| Battle | seeds | blue/red wins | blue shove | red shove | R10 leash/terrain/unruled/free | churn expl/unexpl | blocked | give-ups | max enemy overlap % |
|---|---|---|---|---|---|---|---|---|---|
| 26_100_lings_vs_6_colossi | 24 | 1/23 | 0.39 | 0.13 | 0/0/0/8079 | 736/142 | 23936 | 0 | 1.93 |
| 27_100_lings_vs_8_ultralisks | 24 | 0/24 | 0.66 | 0.18 | 0/0/0/13973 | 1513/203 | 28620 | 0 | 6.52 |
| 28_100_lings_vs_10_immortals | 24 | 24/0 | 0.96 | 0.37 | 0/0/0/17608 | 1119/2098 | 64390 | 0 | 4.08 |
| 29_100_lings_vs_12_stalkers | 24 | 24/0 | 1.68 | 0.63 | 0/0/0/19251 | 715/2977 | 97866 | 0 | 3.56 |
| 32_30_marines_vs_15_banelings | 24 | 18/4 | 0.00 | 0.00 | 0/0/0/42 | 2/0 | 54 | 0 | 0.00 |
| 33_24_marines_vs_12_zealots | 24 | 13/5 | 0.00 | 0.03 | 0/0/0/292 | 42/110 | 1050 | 0 | 0.92 |
| 36a_storm_turns_marines | 24 | 24/0 | 0.11 | 0.00 | 0/0/0/182 | 5/7 | 349 | 0 | 0.53 |
| 36b_marines_beat_zealots_without_storm | 24 | 0/24 | 0.00 | 0.00 | 0/0/0/165 | 11/13 | 363 | 0 | 0.53 |
| 38a_fungal_turns_zerglings | 24 | 24/0 | 0.07 | 0.00 | 0/0/0/326 | 23/24 | 817 | 0 | 2.22 |
| 38b_marines_beat_zerglings_without_fungal | 24 | 0/0 | 0.02 | 0.00 | 0/0/0/256 | 15/18 | 585 | 0 | 2.04 |

The full per-battle tables are `matrix/b0-*-summary.md`.

### 2.2 Close-battle list (pre-registered, §6.2), written before any WP1 or WP2 run on this base

Class R is battles 26, 28, 33, 38a and 38b (named by the owner and Q0103) and every battle below: its claim says
"close", "narrowly" or "evenly" (whole words), or at the base no side wins at least 22 of the 24 seeds. A run with
survivors on both sides is no win, so the battles whose claims are survivor ranges or time limits (22a, 37a, 37b,
39b, 42) qualify under the literal rule. Every other battle is Class D.

| Battle | Claim | Blue / red wins (24 seeds) | Why close |
|---|---|---|---|
| 11a_2_banelings_vs_5_marines | Holding marines fire back and can drop a baneling, but both banelings are spent | 0 / 10 | no side wins 22 of 24 |
| 14_thor_vs_ultralisk |  | 6 / 18 | no side wins 22 of 24 |
| 21_lurker_vs_6_marines | Spines kill light infantry that cannot detect the lurker, and it survives | 12 / 0 | no side wins 22 of 24 |
| 22a_reaper_vs_2_workers | Without a bonus vs light, the Reaper kills only one of two workers in 200 ticks | 0 / 0 | no side wins 22 of 24 |
| 26_100_lings_vs_6_colossi | A hundred zerglings that pack round each colossus mostly overwhelm six colossi; when the colossi win, they win narrowly | 18 / 6 | claim: narrow; no side wins 22 of 24 |
| 28_100_lings_vs_10_immortals | Without splash, immortals are overrun | 5 / 19 | no side wins 22 of 24 |
| 31_15_marauders_vs_15_stalkers | Equal supply is close: a small change of formation can swing the winner | 19 / 5 | claim: close; no side wins 22 of 24 |
| 32_30_marines_vs_15_banelings | Banelings trade far above their cost against clumped marines | 19 / 3 | no side wins 22 of 24 |
| 33_24_marines_vs_12_zealots | A close fight: unkited zealots and marines trade evenly, either side may win narrowly | 24 / 0 | claim: close, narrow, evenly |
| 35_12_vikings_vs_6_colossi | Vikings lose nothing; colossi that flee out of the vikings' sight usually survive, though on a close run all six fall (5.0.14 TargetFilters Visible) | 1 / 0 | claim: close; no side wins 22 of 24 |
| 37a_abduct_sieged_tank | Abduct pulls a sieged tank out of its partner's cover and the zerglings kill it | 0 / 0 | no side wins 22 of 24 |
| 37b_sieged_tank_survives_without_abduct | Control of 37a: without Abduct the same zerglings never reach the first tank | 0 / 0 | no side wins 22 of 24 |
| 38b_marines_beat_zerglings_without_fungal | Control of 38a: without Fungal Growth the same marines beat the zerglings | 0 / 7 | no side wins 22 of 24 |
| 39b_tanks_beat_roaches_without_cloud | Control of 39a: without Blinding Cloud the same sieged tanks beat the roaches | 0 / 0 | no side wins 22 of 24 |
| 42_sieged_tank_vs_fleeing_workers | A sieged tank cannot shell what its side does not see (5.0.14 TargetFilters Visible; Siege Tank range 13, sight 11) | 0 / 0 | no side wins 22 of 24 |

### 2.3 Reading of the new diagnostics at the base (for the checks of §6.1)

- **D-SHOVE counts friendly nudges too.** It counts any contact-induced displacement of a unit that proposes no
  motion. On the melee side that is mostly D-δ, the sanctioned nudge along an engaged unit's ring (Legacy has
  none). B-193's shove is the other side's (immortals, colossi). Check B2 is therefore read per side, and a melee
  side above Legacy by D-δ alone is reported, not taken for B-193.
- **D-CHURN "unexplained" is not zero at the base**, because acquisition's nearest-first key returns a unit to a
  target that became nearer again. B4 is judged against the base, beyond noise, like B1, B3 and B5.
- **D-R10** has no leash, terrain or unruled cases at the base: every count is `free`.
- **§13.13's 3.61** (battle 28 seed 1, immortals): the old measure, "distance moved while an enemy is in the
  band" (`red_pushed_distance` of the battle trace, `matrix/b0-b28-s1`), gives 3.31 on this base; D-SHOVE gives
  1.16. The difference is the definition: D-SHOVE counts only ticks with an enemy body touching and no proposed
  motion, so an immortal's own steps and turns while a zergling is in its band are not counted.
