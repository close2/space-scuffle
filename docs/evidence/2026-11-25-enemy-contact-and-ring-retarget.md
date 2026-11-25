# Evidence: Enemies Block, Never Push (B-193), and the Solver's Stale Pair Cache (B-208)

Measurements of plan [`2026-11-25-enemy-contact-and-ring-retarget`](../plans/2026-11-25-enemy-contact-and-ring-retarget.md),
packages P1 (WP0) and P2 (WP1, WP2). Branch `enemy-contact-plan`, cut from `main` `a59368a3`, `main` `802df056`
merged before measurement (plan Amendment 1, item 1); `main` `4b3f0c7d` (the snapshot cap, presentation only) merged
afterwards, with E2-T8's switch-off digests unchanged. Raw outputs are under `~/scratch/enemy-contact/` (`matrix/`,
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

Development seeds 1 to 1,000, Solver pipeline (`runs/wp0m-1`, `runs/wp2m-1`; `harness-wp2m-vs-wp0m.md`):

- **Enemy-free families byte-identical:** `gap`, `cross` and `corner` (and their relabelled runs) are byte-identical
  to the base's, so also to WP1's (with one owner E-2 is skipped, and the base equals WP2), and `corridor` too.
- **Arc** (enemy pillar): M1 2.40 → 2.42 per 1,000 moving unit-ticks (inside the base's interval [1.97, 2.97]),
  give-ups 318 → 318, fill unchanged, enemy overlap 0.0000 in both, friend overlap unchanged.
- **Mirror:** D-SHOVE-E 0.0362 → 0.0045 per scene (D-SHOVE 0.255 → 0.183); decisive battles 116 → 90, blue share
  0.543 → 0.578 (T9 holds).
- Under Amendment 8's step rule no T-figure the base passes fails, and none the base fails (T1, T4, T7 on the same
  families, by the same values) gets worse.
- Not run: the WP1 and switch-off harness arms and the held-out range (9,000,001 to 9,001,000), stopped once the
  package had stopped; the enemy-free identity above covers §7.5's harness stop rule.

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

## 7. The resumed WP2 under Amendment 2 (F-1, B2-a/B2-b, B6-max, C-1′): stops again

**Outcome: the resumed WP2 fails pre-registered checks and stops (A2.4).** No marker is written, nothing is restated,
and the goldens are not re-recorded.

### 7.1 Code and tests (`82b77a41`)

- F-1 as A2.1 in `solver.rs`; D-SHOVE-E′ and the D-δ eligibility replica in `contact_checks.rs` (two columns
  appended).
- Red before F-1 (`~/scratch/enemy-contact/red2/`): F-T1 (friend overlap 10.4 % at tick 17 in the corridor), F-T2 (the
  held friend moved (0.080, 0.074), not (0.05, 0.05)), F-T3 (the attacker moved by the projection's rounding). F-T6
  was green before F-1: it asserts only E-2's enemy invariants. Its chain's largest friend overlap fell from 1.00 to
  0.50 with F-1. W0-T6 and W0-T7 were red by compilation.
- Green after F-1: every test of A2.4's list, F-T1 with no `#[ignore]` (corridor and open field), and every guard. The
  only failing sim_core and sim_fixtures tests on the tree are the golden replay and the matchup matrix (battle 26),
  which are WP3's records.

### 7.2 Base re-derived on `main` `28d875fb`

`main` moved during the run (B-207 and B-199 fixed; matrix worlds moved). The branch merged it (`d57c8b92`). Every
arm was rebuilt in a fresh target directory from scratch trees of that merge:
- base: the merge with `solver.rs`, `collision.rs`, `movement_rules.rs` and `movement_rules.ron` as on `main`;
- WP1: the base with WP1's `solver.rs`;
- control C-b: the base with `movement_rules.rs` and `.ron` as on the branch;
- WP2′: the merge itself;
- WP2′-off: the merge with the switch off.

The runs on `4b3f0c7d` (`matrix/a2*`, judged the same way) failed the same checks and were discarded with that base.

New base: 24 seeds, failures 2 of record, 33 over all; Legacy 3 and 22; held-out Solver 22, held-out Legacy 30. The
close-battle list is unchanged (`a3-close-list.md`).

### 7.3 Matrix (`matrix/a3*`; `a3-checks-all.md`, `a3-checks-held.md`)

- **Switch off:** `a3wp2p-off` equals `a3wp1-solver` byte for byte. E2-T8 is re-pinned on the new base:
  `0x7e1cffb0addde4de` and `0x2cc569e6926d895e`.
- **Legacy (C-1′ part 1):** `a3wp2p-legacy` equals `a3base-legacy` byte for byte.
- **Overall:** failures of record 2 → 6, over all seeds 33 → 51; held-out 22 → 36. No Class D battle flips. Largest
  enemy overlap 0.00 % everywhere.

| Battle | Base: wins b/r, fails record/all, max enemy, max friend, D-SHOVE-E′ b/r | WP2′ |
|---|---|---|
| 10 | 0/24, 0/0, 0.44 %, 46.76 %, 0.044/0.004 | 0/24, 0/0, 0.00 %, 46.76 %, 0.000/0.003 |
| 23a | 0/24, 0/0, 0.00 %, 0.74 %, 0.004/0.000 | 0/24, 0/0, 0.00 %, 0.74 %, 0.000/0.000 |
| 26 (R) | 19/5, 0/2, 2.59 %, 37.06 %, 2.645/4.462 | 2/22, **3/15**, 0.00 %, **40.44 %**, 0.091/0.001 |
| 27 | 0/24, 0/0, 2.45 %, 37.02 %, 4.871/0.637 | 0/24, 0/0, 0.00 %, 37.02 %, 0.305/0.139 |
| 28 (R, B-192) | 7/17, 2/18, 1.34 %, 37.02 %, 1.017/2.621 | 1/23, 3/23, 0.00 %, 37.02 %, 0.213/0.000 |
| 29 | 24/0, 0/0, 3.79 %, 37.02 %, 1.672/3.312 | 24/0, 0/0, 0.00 %, 37.02 %, 0.363/0.000 |
| 38a (R) | 24/0, 0/0, 0.91 %, 22.22 %, 0.025/0.000 | 24/0, 0/0, 0.00 %, 22.22 %, 0.018/0.000 |
| 38b (R) | 0/7, 0/0, 0.56 %, 22.22 %, 0.009/0.000 | 0/6, 0/0, 0.00 %, 22.22 %, 0.006/0.000 |

**Failing checks, 24 seeds:**
- **23a:**
  - B2-a: red 0.001 on the D-δ-off arm, against Legacy's 0.000 [0.000, 0.000]; two seeds, 0.0166 and 0.0118.
  - B6-max: 0.7424 % against the base's 0.7401 %.
- **26:**
  - B2-b: blue 0.091 against Legacy 0.074 [0.062, 0.087].
  - B6-max: 40.44 % against 37.06 %.
- **27:**
  - B1: R10 free 1,468 per seed against the base's [724, 1,067], and blocked 1,620 against [892, 1,308].
  - B2-b: blue 0.305 against [0.239, 0.337], which passes; red 0.139 against [0.082, 0.112], which fails.
- **38a:** B7′, blue 2.508 per engaged second against the base's [2.523, 2.615].

B3, B4 and B5 pass everywhere. B4 in battle 27: 11.29 per seed against the base's [7.96, 12.67].

**Held-out B2:**
- B2-b fails in 10 (red), 27 (both sides), 28 (blue) and 29 (blue).
- B2-a fails in 23a (red).

**B6-max trace (A2.4)** for battle 26 seed 15 (`diag3/b26s15`): the worst pair is 40.45 % at tick 75, between
zergling 45 (approaching colossus 103) and zergling 46 (engaged on 103, touching it). That is **a direct contact,
F-1's case**. Over the seed, the pair-ticks above the base's 37.06 % split into 1 direct contact and 2 chains. One
reading, not verified: zergling 46 is under D-δ. Its radial yield is removed by `along_ring` before F-1, and A2.1
keeps that radial part removed (F-T3), so the walker behind it takes only its own half of the radial overlap.

**D-SHOVE-E′ can exceed D-SHOVE-E** (on the old base: battle 27 blue 0.323 against D-SHOVE-E 0.223, battle 28 blue
0.213 against 0.183). Removing the own-ring tangent can raise the outward part relative to a second touching enemy.
That is a property of A2.2's definition, reported, not changed.

### 7.4 Throughput (`perf/a3*.txt`, three runs each)

| Scenario | Legacy base | C-a (WP1) | C-b | WP2′ Legacy | Solver base | WP2′ Solver (C-2) | WP2′ Solver / base Legacy |
|---|---:|---:|---:|---:|---:|---:|---:|
| 64 × 64 | 9.7839 G | +0.000 % | +0.006 % | **+0.43 %** | 16.305 G | 16.759 G (+2.78 %) | **+71.3 %** |
| 128 × 128 | 10.1862 G | +0.005 % | +0.006 % | +0.07 % | 10.956 G | 11.101 G (+1.32 %) | +9.0 % |
| canyon | 17.3841 G | −0.008 % | +0.003 % | **+0.38 %** | 27.239 G | 27.824 G (+2.15 %) | **+60.05 %** |

- **C-0 holds:** the lowest run is 70 ticks per second.
- **C-2 holds,** by +2.78 % on 64 × 64.
- **C-1′:**
  - Part 1 holds (the Legacy matrix is byte-identical; the Legacy harness was not run).
  - Part 2: WP2′'s Legacy delta lies outside the controls' band in every scenario.
  - Part 3: `perf record -e instructions:u` of Legacy 64 × 64 on the base and on WP2′ (`perfrec/`) puts the excess and
    the savings in functions whose source is unchanged (`int_sqrt` under a new symbol, `unit_components` no longer
    inlined, `remove_blocked_velocity`, `scan_candidates`, `sim_map`'s `PlayerVisionMap::cast`, `on_connected_ground`).
    That is layout and inlining, which passes under A2.3's rule.
- **Q0111's thresholds are crossed:** 64 × 64 +71.3 % over Legacy (threshold 70 %), canyon +60.05 % (threshold 60 %).
  Not filed: the package stopped.

### 7.5 Not run

The harness (development and held-out, every arm). It was stopped once the matrix checks had failed. The
enemy-free byte-identity check of A2.4 therefore rests on the solver tests (F-T5 and E2-T9) and on F-1's construction
(no walls without enemies), not on the harness families.
