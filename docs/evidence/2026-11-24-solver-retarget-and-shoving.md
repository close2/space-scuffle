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

## 3. WP1: E-1 (binaries `b1`, merged branch head `485b27e5`)

### 3.1 Tests

Red on the base, green after (`red/`): E-T1, E-T2, E-T3, E-T5 red on the base; E-T4 and E-T6 need E-1's API
(red by compilation). E-T4: with the switch on, the per-tick digests equal the base's on the merged tree
(`0xa381a7c5b8982232`, `0x6b074574a51eff4b`). Guards green: `anchored_bodies_stay_and_enemies_separate`,
`enemy_pairs_take_no_dead_band`, `enemy_constraints_have_the_last_word`, `a_nudged_attacker_slides_along_its_ring_only`,
`push_priority_tests.rs`, the order-independence test with enemies under both switch values.

### 3.2 Switch on: bit-identical

`matrix/b1on-solver` is byte-identical to `matrix/b0-solver` (every column, 1,464 rows).

### 3.3 Matrix, 24 seeds, against the base (`matrix/b1-checks.md`): **B3 fails**

1,464 runs; failures of record 6 (base 2), over all seeds 45 (base 31). Held-out: 33 failures (base 20). Rows that
moved:

| Battle | Base wins b/r | Cand wins b/r | Fail of record (base→cand) | Fail all | Moved | B1 | B2 | B3 | B4 | B5 | B7 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 9a_hellion_vs_4_lings | 0/24 | 0/24 | 0→0 | 0→0 | yes | ok | FAIL | ok | ok | ok | FAIL |
| 10_2_hellions_vs_8_lings | 0/24 | 0/24 | 0→0 | 0→0 | yes | FAIL | FAIL | ok | ok | ok | ok |
| 23a_zealot_vs_6_lings | 0/24 | 0/24 | 0→0 | 0→0 | yes | FAIL | FAIL | ok | ok | ok | ok |
| 23b_upgraded_zealot_vs_6_lings | 24/0 | 24/0 | 0→0 | 0→0 | yes | FAIL | ok | ok | ok | ok | ok |
| 24a_roach_vs_5_lings | 0/24 | 0/24 | 0→0 | 0→0 | yes | ok | ok | ok | ok | ok | ok |
| 24b_upgraded_roach_vs_5_lings | 24/0 | 24/0 | 0→0 | 0→0 | yes | ok | ok | ok | ok | ok | ok |
| 26_100_lings_vs_6_colossi | 18/6 | 3/21 | 0→2 | 0→8 | yes | FAIL | FAIL | FAIL | ok | ok | FAIL |
| 27_100_lings_vs_8_ultralisks | 0/24 | 0/24 | 0→0 | 0→0 | yes | FAIL | FAIL | FAIL | ok | ok | ok |
| 28_100_lings_vs_10_immortals | 5/19 | 0/24 | 2→4 | 19→24 | yes | FAIL | FAIL | ok | ok | ok | FAIL |
| 29_100_lings_vs_12_stalkers | 24/0 | 24/0 | 0→0 | 0→0 | yes | FAIL | FAIL | FAIL | ok | ok | ok |
| 33_24_marines_vs_12_zealots | 24/0 | 24/0 | 0→0 | 0→1 | yes | FAIL | FAIL | ok | ok | ok | ok |
| 36a_storm_turns_marines | 24/0 | 24/0 | 0→0 | 3→3 | yes | FAIL | FAIL | ok | ok | ok | ok |
| 36b_marines_beat_zealots_without_storm | 0/24 | 0/24 | 0→0 | 0→0 | yes | FAIL | FAIL | ok | ok | ok | ok |

Battle 28 alone is no longer a stalling case only: the zerglings win 0 of 24 (base 5); G-6′ is WP2's.

**B2 by side (the shove of B-193).** The immortals of battle 28 fall from 2.94 to 0.075 per seed, below Legacy's
0.374. The colossi of battle 26 fall from 5.21 to 1.56 and the ultralisks of 27 from 2.38 to 1.07, but both stay
above Legacy (0.13, 0.18). The melee side rises (battle 26's zerglings 39 to 19, 28's 18 to 13.6, all far above
Legacy's under 2): D-δ's nudges, §2.3.

### 26_100_lings_vs_6_colossi
- B1 FAIL: R10 free 60681; blocked 3600/seed vs base [3637, 4610]
- B2 FAIL: blue 18.847 vs legacy 0.394 [0.356, 0.431]; red 1.558 vs legacy 0.130 [0.115, 0.145]
- B3 FAIL: max 12.71 % (base 2.59 %); mean 4.652 % vs base [1.004, 1.409]; T7 0.5 %: above
- B4 ok: unexplained 20 (base 59); mean 0.83 vs [1.71, 3.29]
- B5 ok: give-ups 0 (base 0); mean 0.00 vs [0.00, 0.00]
- B7 FAIL: blue 0.542/unit-s vs base [0.580, 0.617] LOWER; red 0.917/unit-s vs base [0.909, 0.918]
### 27_100_lings_vs_8_ultralisks
- B1 FAIL: R10 free 24508; blocked 1218/seed vs base [934, 1380]
- B2 FAIL: blue 26.491 vs legacy 0.665 [0.600, 0.732]; red 1.066 vs legacy 0.178 [0.136, 0.239]
- B3 FAIL: max 10.37 % (base 2.45 %); mean 2.517 % vs base [0.444, 0.825]; T7 0.5 %: above
- B4 ok: unexplained 271 (base 242); mean 11.29 vs [7.88, 12.42]
- B5 ok: give-ups 0 (base 0); mean 0.00 vs [0.00, 0.00]
- B7 ok: blue 0.398/unit-s vs base [0.371, 0.406]; red 0.540/unit-s vs base [0.533, 0.557]
### 28_100_lings_vs_10_immortals
- B1 FAIL: R10 free 322741; blocked 16070/seed vs base [15834, 16526]
- B2 FAIL: blue 13.580 vs legacy 0.963 [0.865, 1.075]; red 0.075 vs legacy 0.374 [0.327, 0.432]
- B3 ok: max 1.74 % (base 1.42 %); mean 0.687 % vs base [0.490, 0.716]; T7 0.5 %: above
- B4 ok: unexplained 61 (base 100); mean 2.54 vs [3.17, 5.25]
- B5 ok: give-ups 0 (base 0); mean 0.00 vs [0.00, 0.00]
- B7 FAIL: blue 0.401/unit-s vs base [0.401, 0.437] LOWER; red 0.785/unit-s vs base [0.777, 0.785]
### 29_100_lings_vs_12_stalkers
- B1 FAIL: R10 free 282894; blocked 19043/seed vs base [18674, 20266]
- B2 FAIL: blue 20.443 vs legacy 1.683 [1.534, 1.826]; red 0.661 vs legacy 0.628 [0.570, 0.689]
- B3 FAIL: max 10.51 % (base 2.77 %); mean 4.418 % vs base [0.756, 1.196]; T7 0.5 %: above
- B4 ok: unexplained 136 (base 210); mean 5.67 vs [7.12, 10.75]
- B5 ok: give-ups 0 (base 0); mean 0.00 vs [0.00, 0.00]
- B7 ok: blue 0.270/unit-s vs base [0.263, 0.274]; red 0.666/unit-s vs base [0.662, 0.668]

**B3, enemy overlap: E-1 itself.** The largest enemy overlap at the end of a tick rises to 12.7 % of the radius sum
in battle 26 (base 2.6 %), 10.4 % in 27 and 10.5 % in 29. Battle 26 seed 19 (`diag/b1`, `diag/zz-*`): the worst
pairs are a colossus walking at about 0.1 a tick into engaged zerglings round it. An experimental build of the
same tree with environment toggles (not committed) gives 12.72 % with the committed-step fallback off, and 12.72 %
with the held rule off, but 1.37 % with enemy pairs split evenly. So the plan's closing shares themselves leave
the overlap, not this package's two additions. A reading, not verified: a body ringed by enemies that do not
close takes every pair's whole correction, from both sides, and the Jacobi average and the summed enemy pass
cannot satisfy both; the even split let the ring give way.

## 4. WP2: G-6′ (binaries `b2`, the WP2 tree; branch `solver-followup-wp2-wip` `dc3b62ab`)

### 4.1 Tests

G-T1 red on WP1 (`red/merged-wp2-tests-on-wp1.log`: attacker 11 stands at full ring 3 at tick 93 while far ring
2 has room), and G-T3b, G-T4, G-T5, G-T7, G-T8 red there too; G-T1 to G-T9 green on WP2. G-T7: 49 hand-overs, one
return, after the kill, with room. G-T9: the shared admission test equals the base's scan on 40 generated scenes,
twelve scopes each. Switch off: `matrix/b2off-solver` is byte-identical to `matrix/b1-solver`; the unhashed
schema-46 build gives WP1's digests (`0x1095d9cd774585ae`, `0xc0e740fe07f66a90`) and, with both switches at the
base's behaviour, the base's (`red/merged-wp2-proof-unhashed46.log`).

### 4.2 Matrix, 24 seeds, against the base (`matrix/b2-checks.md`): **stop**

1,464 runs; failures of record 5 (base 2): battle 26 on 3203386110 and 11, battle 28 on **33** (6 immortals
left), 38a and 38b on 3203386110. Over all seeds 56 (base 31). Held-out 46 (base 20). Rows that moved:

| Battle | Base wins b/r | Cand wins b/r | Fail of record (base→cand) | Fail all | Moved | B1 | B2 | B3 | B4 | B5 | B7 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 9a_hellion_vs_4_lings | 0/24 | 0/24 | 0→0 | 0→0 | yes | ok | FAIL | ok | ok | ok | FAIL |
| 10_2_hellions_vs_8_lings | 0/24 | 0/24 | 0→0 | 0→0 | yes | FAIL | FAIL | ok | ok | ok | ok |
| 23a_zealot_vs_6_lings | 0/24 | 0/24 | 0→0 | 0→0 | yes | FAIL | FAIL | ok | ok | ok | ok |
| 23b_upgraded_zealot_vs_6_lings | 24/0 | 24/0 | 0→0 | 0→0 | yes | FAIL | ok | ok | ok | ok | ok |
| 24a_roach_vs_5_lings | 0/24 | 0/24 | 0→0 | 0→0 | yes | ok | ok | ok | ok | ok | ok |
| 24b_upgraded_roach_vs_5_lings | 24/0 | 24/0 | 0→0 | 0→0 | yes | ok | ok | ok | ok | ok | ok |
| 26_100_lings_vs_6_colossi | 18/6 | 8/16 | 0→2 | 0→8 | yes | FAIL | FAIL | FAIL | ok | ok | ok |
| 27_100_lings_vs_8_ultralisks | 0/24 | 0/24 | 0→0 | 0→0 | yes | FAIL | FAIL | FAIL | ok | ok | FAIL |
| 28_100_lings_vs_10_immortals | 5/19 | 22/2 | 2→1 | 19→2 | yes | FAIL | FAIL | FAIL | FAIL | ok | FAIL |
| 29_100_lings_vs_12_stalkers | 24/0 | 24/0 | 0→0 | 0→0 | yes | FAIL | FAIL | FAIL | ok | ok | FAIL |
| 32_30_marines_vs_15_banelings | 19/3 | 17/4 | 0→0 | 1→1 | yes | FAIL | ok | ok | ok | ok | ok |
| 33_24_marines_vs_12_zealots | 24/0 | 2/19 | 0→0 | 0→9 | yes | FAIL | FAIL | FAIL | FAIL | ok | FAIL |
| 36a_storm_turns_marines | 24/0 | 24/0 | 0→0 | 3→0 | yes | FAIL | FAIL | ok | FAIL | ok | FAIL |
| 36b_marines_beat_zealots_without_storm | 0/24 | 0/24 | 0→0 | 0→0 | yes | FAIL | FAIL | ok | FAIL | ok | ok |
| 38a_fungal_turns_zerglings | 24/0 | 24/0 | 0→1 | 0→17 | yes | FAIL | FAIL | FAIL | FAIL | ok | FAIL |
| 38b_marines_beat_zerglings_without_fungal | 0/7 | 9/1 | 0→1 | 0→11 | yes | FAIL | FAIL | FAIL | FAIL | ok | ok |

Contact checks of the crowd battles under WP2:

| Battle | seeds | blue/red wins | blue shove | red shove | R10 leash/terrain/unruled/free | churn expl/unexpl | blocked | give-ups | max enemy overlap % |
|---|---|---|---|---|---|---|---|---|---|
| 26_100_lings_vs_6_colossi | 24 | 8/16 | 28.89 | 2.89 | 0/0/0/1323 | 379/2 | 14796 | 0 | 20.59 |
| 27_100_lings_vs_8_ultralisks | 24 | 0/24 | 34.14 | 1.71 | 0/0/0/4747 | 1160/73 | 5774 | 0 | 14.20 |
| 28_100_lings_vs_10_immortals | 24 | 22/2 | 45.77 | 0.88 | 0/0/0/77863 | 2309/200 | 213787 | 0 | 15.61 |
| 29_100_lings_vs_12_stalkers | 24 | 24/0 | 29.91 | 0.25 | 0/0/0/75653 | 1789/221 | 248978 | 0 | 9.21 |
| 33_24_marines_vs_12_zealots | 24 | 2/19 | 0.00 | 2.92 | 0/0/0/889 | 75/18 | 146 | 0 | 0.31 |
| 38a_fungal_turns_zerglings | 24 | 24/0 | 4.79 | 0.00 | 0/0/0/225 | 197/28 | 485 | 0 | 1.21 |
| 38b_marines_beat_zerglings_without_fungal | 24 | 9/1 | 4.37 | 0.00 | 0/0/0/751 | 157/6 | 231 | 0 | 0.92 |

### 26_100_lings_vs_6_colossi
- B1 FAIL: R10 free 1323; blocked 616/seed vs base [3637, 4610]
- B2 FAIL: blue 28.890 vs legacy 0.394 [0.356, 0.431]; red 2.890 vs legacy 0.130 [0.115, 0.145]
- B3 FAIL: max 20.59 % (base 2.59 %); mean 7.785 % vs base [1.004, 1.409]; T7 0.5 %: above
- B4 ok: unexplained 2 (base 59); mean 0.08 vs [1.71, 3.29]
- B5 ok: give-ups 0 (base 0); mean 0.00 vs [0.00, 0.00]
- B7 ok: blue 0.618/unit-s vs base [0.580, 0.617]; red 0.915/unit-s vs base [0.909, 0.918]
### 28_100_lings_vs_10_immortals
- B1 FAIL: R10 free 77863; blocked 8908/seed vs base [15834, 16526]
- B2 FAIL: blue 45.774 vs legacy 0.963 [0.865, 1.075]; red 0.882 vs legacy 0.374 [0.327, 0.432]
- B3 FAIL: max 15.61 % (base 1.42 %); mean 3.546 % vs base [0.490, 0.716]; T7 0.5 %: above
- B4 FAIL: unexplained 200 (base 100); mean 8.33 vs [3.17, 5.25]
- B5 ok: give-ups 0 (base 0); mean 0.00 vs [0.00, 0.00]
- B7 FAIL: blue 0.526/unit-s vs base [0.401, 0.437]; red 0.767/unit-s vs base [0.777, 0.785] LOWER
### 33_24_marines_vs_12_zealots
- B1 FAIL: R10 free 889; blocked 6/seed vs base [440, 490]
- B2 FAIL: blue 0.005 vs legacy 0.003 [0.003, 0.004]; red 2.923 vs legacy 0.031 [0.020, 0.043]
- B3 FAIL: max 0.31 % (base 0.00 %); mean 0.024 % vs base [0.000, 0.000]; T7 0.5 %: within
- B4 FAIL: unexplained 18 (base 0); mean 0.75 vs [0.00, 0.00]
- B5 ok: give-ups 0 (base 0); mean 0.00 vs [0.00, 0.00]
- B7 FAIL: blue 1.195/unit-s vs base [1.228, 1.233] LOWER; red 0.596/unit-s vs base [0.376, 0.412]
### 38a_fungal_turns_zerglings
- B1 FAIL: R10 free 225; blocked 20/seed vs base [374, 434]
- B2 FAIL: blue 4.795 vs legacy 0.074 [0.047, 0.105]; red 0.000 vs legacy 0.000 [0.000, 0.000]
- B3 FAIL: max 1.21 % (base 0.94 %); mean 0.546 % vs base [0.033, 0.283]; T7 0.5 %: above
- B4 FAIL: unexplained 28 (base 1); mean 1.17 vs [0.00, 0.12]
- B5 ok: give-ups 0 (base 0); mean 0.00 vs [0.00, 0.00]
- B7 FAIL: blue 0.372/unit-s vs base [0.334, 0.343]; red 0.922/unit-s vs base [0.991, 1.008] LOWER
### 38b_marines_beat_zerglings_without_fungal
- B1 FAIL: R10 free 751; blocked 10/seed vs base [356, 413]
- B2 FAIL: blue 4.371 vs legacy 0.023 [0.016, 0.031]; red 0.000 vs legacy 0.000 [0.000, 0.000]
- B3 FAIL: max 0.92 % (base 1.19 %); mean 0.333 % vs base [0.000, 0.188]; T7 0.5 %: above
- B4 FAIL: unexplained 6 (base 1); mean 0.25 vs [0.00, 0.12]
- B5 ok: give-ups 0 (base 0); mean 0.00 vs [0.00, 0.00]
- B7 ok: blue 0.534/unit-s vs base [0.318, 0.369]; red 0.960/unit-s vs base [0.839, 0.969]

**Battle 28** (§6.2: must hold on every seed of record): it holds on 3 of 4; zerglings win 22 of 24 (base 5,
Legacy 24); held-out 22 wins, the claim holding on 21 of 24 (needed: 22). Zerglings in contact / blocked, mean over
the 24 seeds (`diag/b0-24`, `diag/b2-24`):

| Tick | 100 | 150 | 200 | 250 | 300 | 350 | 400 |
|---|---|---|---|---|---|---|---|
| Base | 7.9 / 2.4 | 12.0 / 65.1 | 12.5 / 61.8 | 12.2 / 51.1 | 11.8 / 37.9 | 12.7 / 35.0 | 12.5 / 26.0 |
| WP2 | 9.0 / 0.2 | 23.0 / 21.7 | 26.8 / 40.4 | 25.9 / 28.5 | 25.9 / 22.6 | 23.7 / 13.5 | 21.0 / 7.5 |

Ring occupancy, seed 33 under WP2 (`diag/b2-s33`; band 1.225, a ring point free when every other body clears it):

| Tick | Blocked zerglings | Immortals: zerglings in band, free ring |
|---|---|---|
| 150 | 8 of 86 | 101: 4, 0°; 102: 3, 80°; 103: 4, 10°; 104: 0, 136°; 105: 5, 10°; 106: 0, 98°; 107: 3, 0°; 108: 1, 0°; 109: 5, 0°; 110: 5, 0° |
| 250 | 7 of 66 | 101: 4, 0°; 102: 4, 0°; 103: 3, 10°; 104: 2, 23°; 105: 2, 12°; 106: 1, 55°; 107: 2, 0°; 108: 1, 14°; 109: 5, 0°; 110: 3, 34° |
| 350 | 7 of 55 | 101: 5, 0°; 102: 4, 0°; 103: 3, 10°; 104: 3, 15°; 105: 3, 12°; 106: 2, 14°; 107: 1, 0°; 108: 2, 0°; 109: 4, 0°; 110: 4, 2° |

The battle ends at tick 734 with 6 immortals. On this seed few zerglings stand; the loss is attrition with the far
column's rings partly free, not R10.

**Failing checks:** B1 (D-R10 `free` still 77,863 unit-ticks in battle 28, 1,323 in 26; blocked falls), B3 (largest
enemy overlap 20.6 % in 26, 15.6 % in 28, 14.2 % in 27), B4 (unexplained returns above the base in 28, 33, 36a,
36b, 38a, 38b), B7 (lower firing rates in 27, 28, 29, 33, 38a). Class D battles: none flips or leaves its range
(9a, 10, 23a/b, 24a/b, 27, 29, 32, 36a/b change rows only). Class R battles 26, 33, 38a and 38b leave their
ranges; with checks failing, none is restated.

## 5. Throughput (`perf/b0.txt`, `b1.txt`, `b2.txt`; instructions retired, `perf stat -e instructions:u`)

Three runs each; instruction counts agree to 0.001 %. Ticks per second are noisy on the shared machine (load
average up to 139 during the runs) and are reported as the range of the three.

| Scenario | Legacy b0 | Solver b0 | Solver b1 (E-1) | Legacy b2 | Solver b2 | b2 Solver vs b0 Solver (C-2, ≤ 3 %) | b2 Legacy vs b1 Legacy (C-1) | b2 Solver vs b0 Legacy / own Legacy | Ticks/s b2 Solver |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 64 × 64 | 10.205 G | 16.681 G (+63.5 %) | 16.909 G (+1.4 %) | 10.559 G | 17.343 G | **+4.0 %** | **+3.5 %** | +69.9 % / +64.2 % | 69 to 99 |
| 128 × 128 | 10.531 G | 11.304 G (+7.3 %) | 11.311 G (+0.1 %) | 10.709 G | 11.485 G | +1.6 % | **+1.7 %** | +9.1 % / +7.2 % | 85 to 132 |
| canyon | 17.831 G | 27.675 G (+55.2 %) | 27.992 G (+1.1 %) | 18.076 G | 28.220 G (one run) | +2.0 % | **+1.4 %** | +58.3 % / +56.1 % | 65 |

- **C-0:** every WP2 run is at or above 50 ticks per second. The base's own 64 × 64 Solver runs gave 41 to 49
  under a load average above 100, so the ticks/s column is not a reliable reading on this machine today.
- **C-1 fails:** the Legacy pipeline, which never calls G-6′, costs 1.4 % to 3.5 % more on the WP2 tree. The shared
  admission test is behaviour-identical (G-T9, the switch-off byte identity) but not cost-neutral; the one cost
  step of §7 (the covered-ring early answer) addresses G-6′'s own cost, not this.
- **C-2 fails on 64 × 64** (+4.0 %); 128 × 128 and canyon are within 3 %.
- **Reporting:** 64 × 64 rises to +69.9 % over the WP0 base's Legacy, above Q0102's accepted +66.8 % but below
  §9's +70 % trigger; canyon is +58.3 %, below +60 %. Q0111 is not triggered; Q0110's background would say so.

## 6. Stop

The package stops at WP2 under §7 (battle 28 does not hold on every seed of record; §6.1 checks fail) and under the
coordinator's rule (failed checks, Class R battles moving). Nothing is restated, battle 28's marker stays, the goldens
are not re-recorded, and the harness families were not run on the merged base (the queued runs were cancelled once
the stop was certain). WP2's code is on `solver-followup-wp2-wip`; `solver-followup` ends at WP1 plus documents.
