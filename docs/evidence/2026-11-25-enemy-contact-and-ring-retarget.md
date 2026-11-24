# Evidence: Enemies Block, Never Push (B-193), and the Solver's Stale Pair Cache (B-208)

Measurements of plan [`2026-11-25-enemy-contact-and-ring-retarget`](../plans/2026-11-25-enemy-contact-and-ring-retarget.md),
packages P1 (WP0) and P2 (WP1, WP2). Branch `enemy-contact-plan`, cut from `main` `a59368a3`, `main` `802df056`
merged before measurement (plan Amendment 1, item 1). Raw outputs are under `~/scratch/enemy-contact/` (`matrix/`,
`perf/`, `runs/`, `red/`, `scripts/`, `trees/`).

**Outcome: P2 stops before WP3 under §7.5.** E-2 removes every enemy overlap (0.00 % in every battle and seed,
base up to 2.77 %) and every pressed side's shove but one, and no Class D battle moves; but battle 27 fails B2 (the
ultralisks' D-SHOVE-E 0.146 per seed against Legacy's upper bound 0.099) and B4 (unexplained returns 12.62 per seed
against the base's upper bound 12.42), and E2-T5 fails as specified (§5). Nothing is restated, battles 26 and 28 take
no marker, and the goldens are not re-recorded.

## 1. Method

- **Arms**, each a release build of the matchup trace, the movement harness and one single-scenario throughput binary
  per scenario, in its own target directory (`build.sh`):
  - `wp0m`: WP0's commit `d5ae87f8` merged with `main` `802df056` (scratch tree `trees/wp0m`); the base of record.
  - `wp1m`: WP1's commit `1ccccb00` merged with `802df056` (`trees/wp1m`).
  - `wp2m`: the branch head after the merge (`430d8401`, WP2); `wp2moff` the same with `enemies_block: false`
    (the harness embeds its catalog).
  - The first `wp1m` build shared a target directory with `wp0m` and was not rebuilt from its own sources (its matrix
    equalled the base byte for byte, its binary equalled `wp0m`'s); it was discarded and every merged arm rebuilt in
    a fresh target directory. The pre-merge WP0 runs (`matrix/wp0-*`) are kept and equal the merged base's byte for
    byte (Solver and Legacy): the merge moved no matrix world.
- **Catalogs:** copies under `assets-b-*` (base, no `enemies_block` field) and `assets-m-*` (Solver, Legacy, switch
  off, D-δ off).
- **Seeds:** the four seeds of record and the tool's first 20 non-record seeds ("24 seeds"); held-out: the tool's 21st
  to 44th non-record seeds (23 to 32, 34 to 47).
- **Checks:** `scripts/checks.py`, the plan's §7.1 as pre-registered (B1 not judged for battle 28, which carries the
  B-192 marker; B3 "above the base" strictly; B6's "B3 improved" when the largest enemy overlap is below the base's
  or the overlap ticks per seed fall below the base's interval). "Beyond noise": the candidate's per-seed mean outside
  the base's 95 % bootstrap interval (2,000 resamples, seed 7), on the worse side.
- **Locking:** every run one command under `flock -o ~/scratch/gate.lock`, one at a time.

## 2. WP0: diagnostics and baseline of record

### 2.1 Diagnostics (`d5ae87f8`)

The first attempt's WP0 (`32491bac`, `ffd53b31`, `f987aed5`) ported, with the plan's repairs: D-SHOVE-E
(`shove_e_of`), B7′'s engaged unit-ticks, the enemy pair-ticks above 0.5 %, and the largest friend overlap with the
friend pair-ticks above D-α's moving bound, appended last in `matrix.csv` (28 check columns); the mirror family
appends `shove_e`. Tests W0-T1′ (`shove_e_counts_only_the_outward_part_from_touching_enemies`), W0-T3, W0-T4
(`a_row_written_before_the_contact_checks_reads_without_them`) and W0-T5
(`engaged_unit_ticks_count_the_states_in_band_of_the_target`); red on the base by compilation
(`red/wp0-tests-on-base.log`).

### 2.2 Baseline (`matrix/wp0m-*`)

Solver, 24 seeds: failures 2 of record (battle 28, seeds 11 and 22, B-192), 31 over all seeds. Legacy: 3 of record,
26 over all. Held-out Solver: 20 failures. The figures equal the first attempt's `b0` (its evidence §2.1).

### 2.3 Close-battle list (pre-registered before any WP1 run)

The first attempt's rule at this base gives the first attempt's list exactly (`wp0/close-list.md`): 11a, 14, 21,
22a, 26, 28, 31, 32, 33, 35, 37a, 37b, 38b, 39b, 42. Class R: those and 38a; every other battle is Class D.

## 3. WP1: K-1, the pair cache by move count (B-208, `1ccccb00`)

- **K1-T1** red on the base (`red/k1-t1-on-base.log`): the enemy pass applied −0.1000, the correction from before the
  averaged iteration, where the fresh one is −0.0437. Its first scene let a Verlet rebuild (a correction above half
  the pair slack) empty the cache between the passes and passed on the base; that scene did not exercise the defect,
  and the scene was tightened before any code (Amendment 1, item 7).
- **Matrix** (`matrix/wp1m2-solver`, attribution only): no outcome of record moves; battles 26, 27, 28, 29, 38a and 38b
  change, exactly the planning prototype's "fix" arm. Largest enemy overlap: 26 2.59 → 2.23 %, 27 2.45 → 1.29 %,
  28 1.42 → 1.26 %, 29 2.77 → 1.69 %, 38a 0.94 → 0.56 %, 38b 1.19 → 0.47 %, 10 0.44 → 0.00 %. Wins: 26 18/6 → 17/7,
  28 5/19 → 6/18. No Class D battle flips. The pre-merge K-1 build (`matrix/wp1-solver`) gives the same table byte for
  byte.

## 4. WP2: E-2, enemies block (`74202e48`, merged `430d8401`)

### 4.1 Tests

Red first (`red/e2-tests-on-base-wp0.log` on the base's solver, `red/e2-tests-on-wp1.log` on WP1): E2-T1 (the fighter
moved 0.0319 at tick 25), E2-T2 (the post moved 0.0448), E2-T3 (the post moved 0.1067 on the first tick), E2-T4
(2:1 covered 8.86 and 6.19), E2-T5 (the post moved 0.0251); E2-T6 red with the switch off
(`red/e2-t6-switch-off.log`, an enemy moved at tick 18); E2-T7, T8 and T9 red by compilation. Green on WP2: E2-T1,
T2, T3, T4, T6, T7, T8, T9, `two_walkers_share_the_gap_by_their_walks`, `a_placed_body_is_not_bounded`, and every
guard of the plan (`anchored_bodies_stay_and_enemies_separate`, `enemy_pairs_take_no_dead_band`,
`enemy_constraints_have_the_last_word`, `a_nudged_attacker_slides_along_its_ring_only`,
`terrain_is_never_entered_and_cliffs_separate`, `k_iterations_bring_injected_overlap_below_the_moving_bound`,
`push_priority_tests.rs`). E2-T5 fails (§5).

### 4.2 Switch off and scenes without enemies

- **Switch off is WP1 bit for bit:** `matrix/wp2m-off2-solver` equals `matrix/wp1m2-solver` byte for byte (1,464
  rows); E2-T8's digests with the switch off equal WP1's merged tree's (`0x7e1cffb0addde4de` for the E2-T1 scene,
  `0x395740dd8944647d` for battle 28's first seed of record; before the merge `0xa381a7c5b8982232` and
  `0x7b608f52c55767c4`).
- **Legacy unchanged:** `matrix/wp2m-legacy` equals the base's Legacy table byte for byte.
- Harness families: §4.6.

### 4.3 Matrix, 24 seeds, against the base (`wp2m-checks.md`, `wp2m-checks-all.md`)

1,464 runs; failures of record 2 → 5 (battle 26 seed 11; battle 28 on all four), over all seeds 31 → 46.

| Battle | Base: wins b/r, fails record/all, max enemy overlap, D-SHOVE-E blue/red | K-1 (WP1) | K-1 + E-2 (WP2) |
|---|---|---|---|
| 10 | 0/24, 0/0, 0.44 %, 0.045/0.004 | 0/24, 0/0, 0.00 %, 0.045/0.004 | 0/24, 0/0, 0.00 %, 0.000/0.002 |
| 26 (R) | 18/6, 0/0, 2.59 %, 2.437/4.378 | 17/7, 0/1, 2.23 %, 2.110/4.077 | **2/22, 1/9**, 0.00 %, 0.070/0.000 |
| 27 | 0/24, 0/0, 2.45 %, 4.268/0.940 | 0/24, 0/0, 1.29 %, 5.258/0.979 | 0/24, 0/0, 0.00 %, 0.248/**0.146** |
| 28 (R, B-192) | 5/19, 2/19, 1.42 %, 0.773/2.420 | 6/18, 2/18, 1.26 %, 0.745/2.219 | 2/22, 4/24, 0.00 %, 0.184/0.000 |
| 29 | 24/0, 0/0, 2.77 %, 1.230/3.320 | 24/0, 0/0, 1.69 %, 1.355/3.264 | 24/0, 0/0, 0.00 %, 0.271/0.000 |
| 33 (R) | 24/0, 0/0, 0.00 %, 0.005/0.000 | same | 24/0, 0/1, 0.00 %, 0.000/0.000 |
| 38a (R) | 24/0, 0/0, 0.94 %, 0.011/0.000 | 24/0, 0/0, 0.56 %, 0.011/0.000 | 24/0, 0/0, 0.00 %, 0.015/0.000 |
| 38b (R) | 0/7, 0/0, 1.19 %, 0.003/0.000 | 0/7, 0/0, 0.47 %, 0.003/0.000 | 0/6, 0/0, 0.00 %, 0.002/0.000 |

Battles 9a, 15, 23a, 23b, 24a, 24b, 25a, 25b, 36a, 36b and 37a change rows (shove, churn, ticks) but not their
outcomes. Legacy's D-SHOVE-E (blue/red): 26 0.082/0.110, 27 0.250/0.085, 28 0.211/0.280, 29 0.395/0.472.

**Checks:** every battle passes every judged check (B1, B2, B3, B4, B5, B6, B7′) **except battle 27:**

- **B2 FAIL:** red (ultralisks) D-SHOVE-E 0.146 per seed against Legacy 0.085 [0.073, 0.099] (base 0.940, WP1 0.979).
  With D-δ off (`matrix/wp2m-nonudge-solver`, a diagnostic arm) it is 0.059, under Legacy's bound: what remains is the
  sanctioned nudge of an engaged ultralisk along its own target's ring by allied ultralisks, which is radial to
  another zergling touching it, so D-SHOVE-E's "largest over touching enemies" counts it.
- **B4 FAIL:** unexplained returns 12.62 per seed against the base's [7.88, 12.42] (303 against 242 in total); 12.33
  with D-δ off.

Other figures: B3 0.00 % and 0 overlap ticks in every battle. B6: friend pair-ticks above 5 % rise in 26 (2,382 per
seed, base [824, 1,167]), 27 (721, [428, 704]), 28 (522, [281, 472]) and 29 (1,135, [556, 722]), each where B3
improved, so reported, not failed; the largest friend overlap rises in 27 to 59.9 % (base 37.0 %). B7′ holds
everywhere; B7 (reported) falls in 26 (blue 0.520 per living unit-second, base [0.580, 0.617]), 27 (red) and 28 (blue).

### 4.4 Battles 26 and 28 (§7.3), not applied

- Battle 26 fails seed of record 11 (0 blue, 5 red) and 9 of 24 seeds, with every §7.1 check passing for it; §7.3 would
  give it `known_bug: Some("B-209")`. Battle 28 still fails all four seeds of record and would keep its B-192 marker.
- Neither marker is written: the package stopped (battle 27). The matrix test fails on battle 26 seed 11 on the WP2
  tree.

### 4.5 Held-out seeds (`matrix/wp2m-solver-held`)

Failures 20 → 37: battle 26 0 → 11 (wins 21/3 → 6/18), 28 17 → 23 (7/17 → 4/20), 38a 1 → 2, 38b 1 → 0. Largest
enemy overlap 0.00 % in every battle (base up to 3.23 %, 38b). No claim was restated, so §7.4 does not apply.

### 4.6 Harness

PENDING-HARNESS

### 4.7 Throughput

Instructions retired (`perf stat -e instructions:u`), three runs per arm and pipeline (`perf/wp0m.txt`,
`wp1m.txt`, `wp2m.txt`); the runs agree within 0.015 %. Ticks per second are the range of the three, on a shared
machine.

| Scenario | Legacy base | Solver base | Solver WP1 (K-1) | Legacy WP2 (C-1) | Solver WP2 (C-2, ≤ +3 %) | WP2 Solver / base Legacy | Ticks/s WP2 Solver |
|---|---:|---:|---:|---:|---:|---:|---:|
| 64 × 64 | 9.781 G | 16.189 G | 16.238 G (+0.30 %) | 9.789 G (**+0.08 %**) | 16.511 G (+1.99 %) | +68.8 % | 108 to 109 |
| 128 × 128 | 10.188 G | 10.958 G | 10.960 G (+0.01 %) | 10.195 G (**+0.07 %**) | 11.099 G (+1.29 %) | +8.9 % | 178 to 181 |
| canyon | 17.385 G | 27.240 G | 27.309 G (+0.25 %) | 17.393 G (**+0.05 %**) | 27.653 G (+1.52 %) | +59.1 % | 54 to 79 |

- **C-0 holds:** every run at or above 50 ticks per second.
- **C-2 holds:** the Solver costs at most +1.99 % over the base, with no cost step beyond the enemy-pair index kept
  from the start (Amendment 1, item 6).
- **C-1, strictly, does not:** Legacy, which runs no E-2 code, retires 0.05 % to 0.08 % more instructions than the
  base, above the three runs' spread (0.015 %). The Legacy matrix is byte-identical, so the difference is code
  generation (the larger solver and collision modules, and `ContactRules`' extra field), not behaviour; the first
  attempt's C-1 failure was 1.4 % to 3.5 %. Reported for review.
- Q0111's thresholds are not crossed (64 × 64 +68.8 % over Legacy, below +70 %; canyon +59.1 %, below +60 %); 64 × 64
  is above Q0102's accepted +66.8 %.

## 5. E2-T5 fails as specified

In a corridor two cells wide, a walker as wide as the corridor on `Move` pushes an idle friend of the same width into
a standing enemy post. Under E-2 the friend stops at the post's surface exactly (30.2500, the post never moves,
overlap 0), but the walker keeps walking into it: C-2 gives a moving body share 0 against an idle standing friend,
and the friend, blocked by the enemy, can no longer take its share. Their overlap grows to about 90 % of the radius
sum, the walker is then thrown back behind the friend, and the cycle repeats every 25 ticks (47.7 % at the test's
end). In a corridor wide enough for the smaller walker it squeezes past the friend at about 68 % and reaches the post
itself. The plan's expectation, friend overlap within D-α's moving bound, needs a friend rule (a body blocked by an
enemy counts as unable to move in its friend pairs), which §7.5 forbids within P2. The test is `#[ignore]`d with
that reason. It is the same mechanism as the matrix's B6 rise (§4.3) and the 59.9 % friend overlap in battle 27.

## 6. Stop and what follows

Under §7.2 and §7.5 a §7.1 failure stops the package before merge after at most one amendment attempt; the task's
instruction is to stop and report figures. The attempt was not made. Candidates for review, none built:

1. D-SHOVE-E's "largest over touching enemies" counts a D-δ nudge along an engaged unit's own ring when it is radial to
   another touching enemy; measured against the unit's own target only, battle 27's red figure would be the nudge-free
   0.059. This is a definition change of a pre-registered check, so it is the reviewer's call.
2. A friend-pair rule for a body E-2 blocks (§5): it would act on B6's rise and E2-T5, and changes friend rules.
3. B4 in battle 27 is 1.6 % above the base's interval (12.62 against 12.42), D-δ off 12.33.

The WP1 fix (K-1, B-208) is independent of E-2: it changes no outcome of record and lowers every enemy overlap.
