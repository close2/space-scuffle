# Evidence: one contact solver, intent steering and the concave as a goal

Date: 2026-11-20. Plan: [`docs/plans/2026-11-20-contact-solver.md`](../plans/2026-11-20-contact-solver.md).
Base: `main` at `3ed2f768` merged into `contact-solver-plan`. Every figure below is a measurement on the
**development range** (seeds 1 to 1,000 per family, each scene family also relabelled; mirror seeds 1 to 1,000),
run one at a time under `flock ~/scratch/gate.lock` with `--jobs 4`. Nothing of the acceptance range
(100,001 to 101,000) or the held-out range (9,000,001 to 9,001,000) has been run. CSVs, binaries and placebo
patches are under `~/scratch/contact-solver/wp1/` and are not committed.

## 1. WP1: the committed harness

### 1.1 What is committed

- `crates/sim_fixtures/src/harness/` (`mod.rs` runner and command line, `corpus.rs` generator, `metrics.rs`
  M1 to M8, `mirror.rs` M10, `report.rs` aggregation, bootstrap, sign test, Wilson interval and the verdicts of
  T1 to T9), the thin binary `crates/sim_core/examples/movement_harness.rs`, and
  `crates/sim_fixtures/tests/harness_tests.rs`.
- Families: `gap`, `arc`, `cross`, `corridor`, `mirror` (§6.2) and `corner` (§7 WP1, B-152: the gap family's
  wall with a gas geyser at each side of the opening, so the mouth is bounded by the 5.0.14 geyser outline with its
  diagonal lobes; only passable openings, the narrowest width `w − 1/2` between 1.1 and 4.5 diameters).
- Runtime of the whole development corpus (six families, five relabelled): **1,084 s** with `--jobs 4`, under the
  30 minutes WP1 allows.

### 1.2 WP1's acceptance "reproduced to the last digit" cannot be met

The plan asks that the committed harness reproduce §5's baseline to the last digit (`base-1` figures). It cannot:

- The planner's scratch harness (`examples/movement_harness.rs` in a scratch worktree) **no longer exists**:
  `~/scratch/contact-solver/` holds its binaries, CSVs, analysis scripts and `proto.diff`, but `proto.diff` is a
  `git diff` of tracked files and does not contain the harness or `movement/proto.rs`, and the scratch worktree
  and its target directory have been removed.
- Reading the source back from the planning session's transcript was **refused by the permission system**
  (auto-mode classifier, "Unauthorized Access"); per the standing rule it was not retried or worked round.

The harness was therefore rebuilt from §6's text. Where §6 leaves a choice, the committed harness decides as in
§1.3; these decisions were made and committed before any solver-pipeline measurement, and no threshold depends on
them. Its figures differ from §5's in the digits but show the same failures of today's pipeline (§1.4). **Proposed
amendment (for the orchestrator):** WP1's acceptance becomes "the committed harness, built from §6's text, measures
the development baseline of today's pipeline; §5's scratch figures are superseded by it; relative thresholds compare
with today's pipeline measured by this harness on the same range". No threshold changes.

### 1.3 Definitions the harness fixes where §6 leaves room

| Item | Committed choice |
|---|---|
| Stream | One `SplitMix64` per scenario, seeded from `seed`, the family index and a salt; relabelling uses a second stream on a different salt |
| Scratch archetypes | Ground, instant acceleration (3.136) and deceleration (= top speed), turn rate 55.99° per tick, push priority 10 (gap's second archetype 20, radius `round(0.75 r · 64) / 64`), 10⁶ life; attackers carry a 6-damage hitscan weapon with period 15 and no random delay; the pillar has top speed 0 (immobile) |
| Movement rules | The shipped `movement_rules.ron` (group lanes on) |
| gap | 48 × 48 map, wall on column 24, opening of `w` rows at `24 − w/2 ± 4`; walkers in a square block at x 8 to 12, y `24 ± 6`, edges 1/4 apart; one shared `Move` to (40, `24 ± 8`). Passable iff `w > 2r` |
| corner | The gap scene with column 24 open over `w + 6` rows and gas geysers anchored on its first and last three rows |
| arc | 48 × 48 map, pillar at the centre; the block of attackers at 10 to 16 units (1/64 lattice) on one of 16 integer headings, rows facing the pillar; `C = ⌊π / asin(r / (P + r + R))⌋` |
| cross | 48 × 48 map; group A on one of 16 headings, group B rotated by θ; each group a square block 12 units before the centre, every unit its own `Move` 24 units on; spacing `2r + 1/4` |
| corridor | 64 × 16 map, a corridor of `w` rows centred on row 8, pillar of radius `w / 2` at x 48 to 54; attackers in single file from x 4 to 10, edges 1/4 apart; radius drawn until `2r ≤ w − 1/8`; bound `2 d / v + 500` with `d` from the last attacker to the pillar |
| mirror | 64 × 64 map; one or two shipped archetypes chosen by property (mobile ground, weapon against ground, no harvester, supply 1 to 4), `(4 + U[0, 11]) / supply` units of each (at least one); blue on lattice slots in x 10 to 22, y 20 to 44, red at `(64, 64) − p`; attack-move to the other side's centroid at tick 0 and every 100 ticks; random delays removed; 3,000 ticks |
| Moving / standing | A step above `v / 8` moves; at most `v / 8` stands (M1's denominator, M8's classes) |
| Near pair (M8) | Two friends whose edges are less than 1/4 apart |
| Completion (M5) | The order queue is empty, the unit gave nothing up, and in a walled scene it stands east of the wall's far edge; its tick is the first such tick |
| Deadlock (M5) | A unit with room that has not completed and has stood (step ≤ `v / 8`) for at least the last 50 ticks; give-ups count (§6.6 C-d) |
| Potential (M4) | Walled scenes: west of the wall, distance to the opening's centre plus that centre's distance to the goal; east, distance to the goal. Cross: distance to the goal. Ring scenes: distance beyond the weapon band |
| Throughput (M7) | `(n − 1) · 25 / (t_last − t_first)` over the ticks at which each unit first passes the wall's centre line |
| Terrain penetration (M8) | The terrain disc's depth into blocked cells or past the map edge; into a resource node's outline, not its cells |
| Noise rule | A relative threshold fails only when the value is above today's aggregate and above the upper end of today's 95 % bootstrap interval (§6.4) |

### 1.4 Baseline of today's pipeline (development range)

The plan's §5.2 figures (planner's scratch harness) and the committed harness's, for today's pipeline:

| Metric | §5.2 (scratch) | Committed harness |
|---|---:|---:|
| gap M1 / M1-hi | 15.38 / 3.31 | 17.70 [16.74, 18.63] / 3.79 |
| gap give-ups with room (deadlocked) | 334 (337) | 454 (459) |
| gap scenarios complete | 83.1 % | 80.5 % |
| gap η median / 5th | 0.485 / 0.240 | 0.655 / 0.300 |
| gap friend overlap > 5 % moving | 11.61 % | 14.68 % |
| cross M1 | 5.89 | 4.27 [3.76, 4.89] |
| cross give-ups with room | 73 | 152 |
| cross relabel sign test p (completion tick) | 0.000 | 0.000 |
| arc M3 / M4 | 2.221 / 4.80 % | 2.150 / 1.99 % |
| arc fill mean / 5th | 0.982 / 0.900 | 0.980 / 0.889 |
| corridor relabel sign test p (M1) | 0.000 | 0.004 |
| corridor enemy overlap max | 0.0471 | 0.0398 |
| mirror blue share | 0.423 [0.387, 0.460] | 0.438 [0.400, 0.476] |

The same thresholds fail on today's pipeline: T1 (every family), T2's M3 on arc and corridor, T3 on arc, T4 (gap
and cross give-ups with room), T7 (friends everywhere, and the enemy overlap of the arc and the corridor), T8 (cross completion tick p < 0.001,
corridor M1 p = 0.004) and T9 (blue wins 43.8 % of decisive mirror games). T6 holds with this harness's throughput
definition. The full tables follow.

Columns: `today` is `main` at `3ed2f768` with the harness (`bb5fd108`); `P1` is the placebo build
`~/scratch/contact-solver/wp1/placebo1.diff` (one raw unit, 2⁻³², added to `PUSH_RELAXATION`). Placebo P2 (one raw unit
added to `STEERING_LOOKAHEAD` alone, `placebo2.diff`; the plan's P2 had added it on top of P1, §6.6 C-b) is built
(`harness-placebo2`) but its run could not finish before hand-back: the shared gate lock was held for long stretches by
the GPU-lane measurements. P1 moves no aggregate outside its bootstrap interval; counts move by up to 3 % (gap
give-ups 454 → 467), the noise any single count carries, as in §5.2.

**gap**

| Metric | today | P1 |
|---|---:|---:|
| M1 per 1,000 moving unit-ticks | 17.70 | 17.67 |
| M1 per 1,000 moving unit-ticks 95 % CI | [16.7377, 18.6325] | [16.7288, 18.5604] |
| M1-hi | 3.79 | 3.81 |
| M2 wasted share | 0.0634 | 0.0635 |
| M2 wasted share 95 % CI | [0.0610, 0.0656] | [0.0611, 0.0657] |
| M3 standing waste per unit | 0.004 | 0.004 |
| M4 progress violations % | 0.01 | 0.01 |
| Deadlocked units (passable) | 459 | 470 |
| Give-ups | 454 | 467 |
| Scenarios complete % | 80.5 | 80.0 |
| Units complete % | 96.80 | 96.72 |
| Impassable: wrong passes | 0 | 0 |
| η median | 0.655 | 0.655 |
| η 5th percentile | 0.300 | 0.300 |
| Monotonicity drops | 0 | 0 |
| Friend overlap >5 % moving, % of near pair-ticks | 14.68 | 14.71 |
| Friend overlap >1 % standing, % | 5.59 | 5.62 |
| Friend overlap max | 0.742 | 0.742 |
| Enemy overlap max | 0.0000 | 0.0000 |
| Terrain penetration max | 0.00008 | 0.00008 |
| Relabel sign test p (M1) | 0.436 | 0.264 |
| Relabel sign test p (completion tick) | 0.248 | 0.187 |

**arc**

| Metric | today | P1 |
|---|---:|---:|
| M1 per 1,000 moving unit-ticks | 3.20 | 3.18 |
| M1 per 1,000 moving unit-ticks 95 % CI | [3.0558, 3.3360] | [3.0435, 3.3229] |
| M1-hi | 1.98 | 1.99 |
| M2 wasted share | 0.0991 | 0.0989 |
| M2 wasted share 95 % CI | [0.0970, 0.1011] | [0.0968, 0.1008] |
| M3 standing waste per unit | 2.150 | 2.144 |
| M4 progress violations % | 1.99 | 1.99 |
| Give-ups | 25 | 22 |
| Fill mean | 0.980 | 0.979 |
| Fill 5th percentile | 0.889 | 0.889 |
| Friend overlap >5 % moving, % of near pair-ticks | 1.32 | 1.32 |
| Friend overlap >1 % standing, % | 0.22 | 0.22 |
| Friend overlap max | 0.597 | 0.597 |
| Enemy overlap max | 0.0183 | 0.0183 |
| Terrain penetration max | 0.00000 | 0.00000 |
| Relabel sign test p (M1) | 0.968 | 0.807 |
| Relabel sign test p (completion tick) | 0.134 | 0.124 |

**cross**

| Metric | today | P1 |
|---|---:|---:|
| M1 per 1,000 moving unit-ticks | 4.27 | 4.28 |
| M1 per 1,000 moving unit-ticks 95 % CI | [3.7609, 4.8923] | [3.7720, 4.9058] |
| M1-hi | 1.36 | 1.36 |
| M2 wasted share | 0.0344 | 0.0344 |
| M2 wasted share 95 % CI | [0.0334, 0.0354] | [0.0334, 0.0354] |
| M3 standing waste per unit | 0.001 | 0.001 |
| M4 progress violations % | 0.00 | 0.00 |
| Deadlocked units (passable) | 152 | 152 |
| Give-ups | 152 | 152 |
| Scenarios complete % | 94.6 | 94.6 |
| Units complete % | 99.15 | 99.15 |
| Friend overlap >5 % moving, % of near pair-ticks | 3.78 | 3.78 |
| Friend overlap >1 % standing, % | 1.35 | 1.35 |
| Friend overlap max | 0.537 | 0.537 |
| Enemy overlap max | 0.0000 | 0.0000 |
| Terrain penetration max | 0.00000 | 0.00000 |
| Relabel sign test p (M1) | 0.031 | 0.026 |
| Relabel sign test p (completion tick) | 0.000 | 0.000 |

**corridor**

| Metric | today | P1 |
|---|---:|---:|
| M1 per 1,000 moving unit-ticks | 8.77 | 8.55 |
| M1 per 1,000 moving unit-ticks 95 % CI | [5.7363, 12.2422] | [5.7098, 11.8148] |
| M1-hi | 2.82 | 2.90 |
| M2 wasted share | 0.0437 | 0.0438 |
| M2 wasted share 95 % CI | [0.0392, 0.0484] | [0.0393, 0.0484] |
| M3 standing waste per unit | 0.563 | 0.569 |
| M4 progress violations % | 0.52 | 0.52 |
| Give-ups | 275 | 267 |
| Fill mean | 0.955 | 0.957 |
| Fill 5th percentile | 0.500 | 0.500 |
| Friend overlap >5 % moving, % of near pair-ticks | 7.35 | 7.36 |
| Friend overlap >1 % standing, % | 3.88 | 3.90 |
| Friend overlap max | 0.988 | 0.988 |
| Enemy overlap max | 0.0398 | 0.0398 |
| Terrain penetration max | 0.00008 | 0.00009 |
| Relabel sign test p (M1) | 0.004 | 0.001 |
| Relabel sign test p (completion tick) | 0.911 | 1.000 |

**corner**

| Metric | today | P1 |
|---|---:|---:|
| M1 per 1,000 moving unit-ticks | 20.94 | 20.88 |
| M1 per 1,000 moving unit-ticks 95 % CI | [19.9716, 21.8625] | [19.9095, 21.7928] |
| M1-hi | 4.60 | 4.61 |
| M2 wasted share | 0.0617 | 0.0616 |
| M2 wasted share 95 % CI | [0.0596, 0.0637] | [0.0595, 0.0636] |
| M3 standing waste per unit | 0.000 | 0.000 |
| M4 progress violations % | 0.00 | 0.00 |
| Deadlocked units (passable) | 0 | 0 |
| Give-ups | 0 | 0 |
| Scenarios complete % | 100.0 | 100.0 |
| Units complete % | 100.00 | 100.00 |
| Friend overlap >5 % moving, % of near pair-ticks | 14.89 | 14.90 |
| Friend overlap >1 % standing, % | 5.23 | 5.24 |
| Friend overlap max | 0.554 | 0.554 |
| Enemy overlap max | 0.0000 | 0.0000 |
| Terrain penetration max | 0.00000 | 0.00000 |
| Relabel sign test p (M1) | 0.711 | 1.000 |
| Relabel sign test p (completion tick) | 0.715 | 0.425 |

**mirror** (blue wins / red wins / draws; blue share of decisive games [95 % Wilson])

| Run | Mirror |
|---|---|
| today | 282 / 362 / 356; 0.438 [0.400, 0.476] |
| P1 | 282 / 362 / 356; 0.438 [0.400, 0.476] |

**Thresholds against today's pipeline** (the verdicts of P1, which lies within the noise of today's figures; development range, informative only: thresholds bind on the acceptance and held-out ranges)

| Threshold | Family | Verdict | Values |
|---|---|---|---|
| T1 | arc | **fails** | M1 3.18, M1-hi 1.99 |
| T1 | corner | **fails** | M1 20.88, M1-hi 4.61 |
| T1 | corridor | **fails** | M1 8.55, M1-hi 2.90 |
| T1 | cross | **fails** | M1 4.28, M1-hi 1.36 |
| T1 | gap | **fails** | M1 17.67, M1-hi 3.81 |
| T2 | arc | **fails** | M2 0.0989, M3 2.144 |
| T2 | corner | holds | M2 0.0616, M3 0.000 |
| T2 | corridor | **fails** | M2 0.0438, M3 0.569 |
| T2 | cross | holds | M2 0.0344, M3 0.001 |
| T2 | gap | holds | M2 0.0635, M3 0.004 |
| T3 | arc | **fails** | M4 1.99 % |
| T3 | corner | holds | M4 0.00 % |
| T3 | corridor | holds | M4 0.52 % |
| T3 | cross | holds | M4 0.00 % |
| T3 | gap | holds | M4 0.01 % |
| T4 | corner | holds | deadlocks 0, give-ups 0, complete 100.0 % |
| T4 | cross | **fails** | deadlocks 152, give-ups 152, complete 94.6 % |
| T4 | gap | **fails** | deadlocks 470, give-ups 467, complete 80.0 % |
| T5 | arc | holds | fill 0.979, 5th 0.889 |
| T6 | gap | holds | η 0.655, 5th 0.300, drops 0 |
| T7 | arc | **fails** | enemy 0.0183, terrain 0.00000, friends 1.32 % / 0.22 %, max 0.597 |
| T7 | corner | **fails** | enemy 0.0000, terrain 0.00000, friends 14.90 % / 5.24 %, max 0.554 |
| T7 | corridor | **fails** | enemy 0.0398, terrain 0.00009, friends 7.36 % / 3.90 %, max 0.988 |
| T7 | cross | **fails** | enemy 0.0000, terrain 0.00000, friends 3.78 % / 1.35 %, max 0.537 |
| T7 | gap | **fails** | enemy 0.0000, terrain 0.00008, friends 14.71 % / 5.62 %, max 0.742 |
| T8 | arc | holds | p 0.807 / 0.124 |
| T8 | corner | holds | p 1.000 / 0.425 |
| T8 | corridor | **fails** | p 0.001 / 1.000 |
| T8 | cross | **fails** | p 0.026 / 0.000 |
| T8 | gap | holds | p 0.264 / 0.187 |
| T9 | mirror | **fails** | 0.438 [0.400, 0.476], draws 356 |

### 1.5 §5.4's hypothesis: the pursuit's live read (scratch build)

A scratch build (`~/scratch/contact-solver/wp1/pursuit-snapshot.diff`, not committed) in which `prepare_pursuit`
reads its target's position and velocity from a retained tick-start table built with the movement snapshot:

| Pipeline | Live read (today) | Tick-start read |
|---|---|---|
| Today's | 282 / 362 / 356; 0.438 [0.400, 0.476] | 141 / 148 / 711; 0.488 [0.431, 0.545] |

(blue wins / red wins / draws; blue's share of decisive games [95 % Wilson].) **The hypothesis holds**: with the
tick-start read the mirror meets T9, and most fights end in a mutual kill (711 draws). WP5 is warranted.

### 1.6 The corner family (B-152)

Today's pipeline lets every walker through in every corner scene (no give-up, no deadlock, 100 % complete) but
reverses more than in any other family: M1 20.94 [19.97, 21.86] per 1,000 moving unit-ticks, M1-hi 4.60. B-152 is
reproduced by the family; it is judged at WP6 under the same thresholds.

## 2. WP2: stopped on the C-3 property

WP2's work (the `contact` section, schema 33's three memory fields, `movement/solver.rs`, `intent.rs` and
`concave.rs` with their tests) is on the side branch `contact-solver-wp2-wip`, not on `contact-solver-plan`. Every
test the plan names for WP2 passes there except one, which the plan marked **verify** and which fails as specified.

### 2.1 What fails

C-3 claims that `K = ⌈ln(δ_move / ρ) / ln(1 − 1/c)⌉ = 14` iterations bring the overlap one tick injects (at most `ρ`
of the radius sum per pair, `ρ = 0.56` for the shipped catalog's worst pair) below `δ_move = 5 %`, and WP2's test list
asks: "after K iterations no friendly pair of a generated hexagonal crowd with injected overlap `ρ` exceeds
`δ_move`". The test (`movement::solver::tests::k_iterations_bring_injected_overlap_below_the_moving_bound`) builds a
9 × 9 hexagonal crowd of radius 0.375 standing in contact and steps every body by up to `ρ · r` = 0.21 (the worst
pair's top speed) in a random direction, so no pair gains more than `ρ`; it then runs the solver as C-1 specifies
(pairs built once within `r_i + r_j + 1/4`, averaged Jacobi, dead band 1 %, hard pass, open ground).

Over 1,000 generated crowds (scratch measurement on the WIP code):

| K | Pair slack 1/4 (C-1 as written): crowds above 5 % | worst | Pair slack 1 (every pair that can touch): crowds above 5 % | worst |
|---:|---:|---:|---:|---:|
| 14 | 182 | 0.144 | 143 | 0.077 |
| 16 | 92 | 0.150 | 41 | 0.069 |
| 18 | 69 | 0.148 | 13 | 0.061 |
| 20 | 60 | 0.153 | 1 | 0.050 |
| 22 | 60 | 0.151 | 0 | 0.046 |
| 24 | 63 | 0.151 | 0 | 0.041 |
| 28 | 64 | 0.151 | 0 | 0.032 |

No crowd stopped early: the adversarial case uses every iteration.

### 2.2 Two causes

1. **The pair list misses contacts the solve itself makes.** Built once with a 1/4 slack, the list lacks pairs that
   were farther apart than `r_i + r_j + 1/4` after the steps but that the corrections of their other neighbours
   push together. Their overlap is never corrected, so more iterations do not help: with slack 1/4 the share of
   failing crowds stops falling at about 6 % and the worst stays near 15 % (seed 14: bodies 51 and 61 end 9.5 %
   apart-overlapped with free room on their far sides).
2. **The contraction per iteration is slower than `1 − 1/c`.** With every pair present, `K = 14` still leaves 14 %
   of crowds above 5 % (worst 7.7 %); the bound holds on all 1,000 crowds from `K = 22`. Averaging over up to six
   contacts removes less than `1/6` of the deepest overlap per iteration when every neighbour is itself displaced:
   the derivation's premise "a body whose other contacts are resolved" does not hold when one tick moves them all.

### 2.3 Why this is a stop

The test is in WP2's list of failing-tests-first, and it can be made green only by changing a derived constant
(`contact.iterations`) or the solver's design (the pair list), both of which the standing rules reserve for an
amendment. Nothing was tuned.

### 2.4 Proposed amendment (for the orchestrator)

- **C-1, the pair list:** rebuild the pair list whenever a body has moved more than half the slack since the list was
  built (a Verlet list), or build it with a slack that covers the tick's largest possible motion
  (`2 · s_max + ρ_max · r_max`, about 1 world unit for the shipped catalog). Either keeps C-1a's order independence:
  the list is still a function of the set of bodies and positions.
- **C-3, the derivation:** replace `ln(1 − 1/c)` by a contraction measured by this property test, and derive `K` as
  the least count for which the property holds on the generated crowds of the shipped catalog's worst pair; on the
  data above that is **22** with a full pair list. The property test then guards the data as the formula test does.
- **Cost:** `K = 22` is the worst case; the early stop ends typical ticks sooner (WP3 measures it). §5.5's prototype
  cost was measured at `K = 14` with a slack-1/4 list; the WP6 10 % stop (K-8) becomes harder to meet and should be
  re-estimated before WP3.
- Alternatively, keep `K = 14` and restate D-α's moving bound as what 14 iterations achieve on this property (about
  8 % worst case with a full pair list); T7's thresholds would not change (they bound shares of near pair-ticks,
  measured on the families).

## 3. WP2 under Amendment 1: stopped on the confirmation of K

Amendment 1 (`1005fa5b`) decided a Verlet pair list and K as the smallest count passing C-3, confirmed on a fresh,
disjoint set of crowds, with a stop if the confirmation fails. It failed.

- **The pair list.** The solver keeps the list built within `r_i + r_j + 1/4` and rebuilds it after any iteration in
  which a body's net correction since the build exceeds 1/8 (half the slack): a pair left out was at least the
  radius sum plus 1/4 apart and each body has since moved at most 1/8, so it cannot touch.
- **Derivation.** The crowds of §2 (9 × 9 hexagonal crowd, radius 0.375, every body stepped by up to `ρ · r` = 0.21),
  seeds 1 to 1,000:

  | K | 14 | 15 | 16 | 17 | 18 | 19 | 20 | 21 |
  |---|---:|---:|---:|---:|---:|---:|---:|---:|
  | Crowds above 5 % | 143 | 78 | 41 | 23 | 13 | 4 | 1 | 0 |
  | Worst overlap | 0.077 | 0.073 | 0.069 | 0.065 | 0.061 | 0.057 | 0.050 | 0.048 |

  The smallest passing count is **K = 21** (set in `movement_rules.ron` on the WIP branch).
- **Confirmation** on the fresh crowds of seeds 1,000,001 to 1,001,000, never used to choose K: **fails**. Two crowds
  end above 5 % at K = 21, the worst at 5.99 % (the test stopped first at seed 1,000,266, 5.05 %). Measured after the
  failure, for the trace only: on this set K = 22 to 24 leave one crowd above 5 % and K = 25 passes (worst 4.74 %).

### 3.1 Reading

The tail of the C-3 property is heavy: the worst crowd of a set of 1,000 sets K, and a second set of 1,000 has a
worse one. A K chosen as "the smallest passing count" on any finite set will be confirmed only by chance; a K that
passes both sets here (25) would very likely fail a third. The residual overlaps are small (5 % to 6 % of the radius
sum on 0.1 % to 0.2 % of adversarial crowds), but the amendment's rule is binary.

### 3.2 Proposed amendment (for the orchestrator)

- **A margin, not a minimum.** Derive K as the smallest count for which the worst overlap over the derivation crowds
  is at most a stated fraction of `δ_move` (for example 4 %, i.e. `0.8 δ_move`), then confirm on fresh crowds against
  `δ_move` itself. On the derivation data above, 4 % is not yet met at K = 21 (worst 4.82 %); the count would be
  measured, not chosen here.
- **Or a quantile.** State C-3 as "at most one crowd in 1,000 above `δ_move`" and confirm that rate on the fresh
  set (K = 20 on the derivation set; 21 gives 2 in 1,000 on the confirmation set).
- **Or more crowds.** Derive on 10,000 crowds and confirm on 10,000 fresh ones.
- Either way D-α's 5 % stays as it is (Amendment 1, 2 (c)), and T7 bounds the overlap actually seen on the families.

## 4. WP2 under Amendment 2: K = 21, confirmed

Amendment 2 (`f0f5694d`) restates C-3 as a rate: at most 1 crowd in 1,000 ends a solve with a friendly moving pair
above 5 %, and none ever above 8 %. K is the smallest count with at most 2 of the 2,000 seen crowds above 5 % and none
above 8 %, confirmed once on unseen crowds.

- **Derivation** on seeds 1 to 1,000 and 1,000,001 to 1,001,000 (the Verlet pair list of §3):

  | K | 18 | 19 | 20 | 21 |
  |---|---:|---:|---:|---:|
  | Crowds of 2,000 above 5 % | 27 | 14 | 7 | 2 |
  | Worst overlap | 0.068 | 0.065 | 0.063 | 0.060 |

  **K = 21** (unchanged from §3).
- **Confirmation**, once, on seeds 2,000,001 to 2,001,000 at K = 21: **0 crowds above 5 %, worst 4.95 %**. It holds.
- The test `movement::solver::tests::k_iterations_bring_injected_overlap_below_the_moving_bound` is the derivation:
  it requires the rule on the seen crowds, that K − 1 misses it, and the confirmation figures. The closed-form test
  (`contact_solver_tests`) keeps K at least `⌈ln(δ_move / ρ) / ln(5/6)⌉` = 14.

### 4.1 Schema 33 and the legacy pipeline

- `golden_replay.ron`, `golden_replay_orders.ron` and `DEFAULT_SANDBOX_CHECKSUM` re-recorded for schema 33. Every
  checkpoint changed; no behaviour did. Proof: a scratch build (`~/scratch/contact-solver/wp2-wip/schema32-proof.diff`)
  that leaves `side_partner`, `side_left` and `slot` unhashed and holds the schema at 32 reproduces the previous
  checksums at every checkpoint of both replays and the previous sandbox checksum, native debug and release.
- The new list agrees on native debug and release and on `wasm32-wasip1` release and debug (runner
  `wasmtime --dir=<worktree>`).
- The harness's rows of seeds 1 to 20 of every scene family are byte-identical to WP1's with the WP2 code.

## 5. WP3: the Solver pipeline, stopped on prescribed tests and on cost

WP3's code is on the side branch `contact-solver-wp3-wip`. Under `contact.pipeline: Solver` the step steers by intent
(I-1 to I-4, the side commitment in `MovementMemory`), skips the legacy velocity limit, the surround walk and the
waiting rule, widens the neighbour reach by the side horizon, and the collision phase runs the contact solver (Verlet
pairs, K = 21) with the legacy terrain step. Legacy is untouched: both golden replays and the sandbox checksum pass,
and the harness rows of seeds 1 to 20 of every scene family are byte-identical to WP1's. **The acceptance range was
not run**: WP3 fails its own prescribed tests first (§5.1), so spending the range now would leave nothing unseen for a
real acceptance after an amendment.

### 5.1 Prescribed tests

- `tests/solver_pipeline_tests.rs`: the restated tests of §8.1 pass (an idle friend pushed along the line of centres,
  a waiting friend's half share, the D-β-off twin, the exact head-on tie passing right to right, converging units not
  vibrating, a body spawned into a crowd leaving it within 25 ticks and never through a wall).
- **Fails:** `gaps_and_crossings_meet_t4_and_t7_under_the_solver`: 5 deadlocked units in 20 development gap scenes, all
  in mixed-priority scenes (seeds 12, 15, 16): the K-10 give-ups at the shared destination.
- **Kept tests that fail under Solver** (scratch run with the shipped pipeline flipped, `kept-under-solver.log`):
  `test_move_command_units_fan_out_and_surround_target_point` (spread 2.77, needs 3) and
  `battlefield_bubble_trajectories_are_smooth_without_micro_jitter` (one heading zig-zag). The restated-in-WP3 tests
  in their legacy form fail as expected (`test_idle_friendly_displaced_laterally`,
  `test_converging_units_respect_speed_cap_and_do_not_vibrate`, `units_blocked_by_a_holding_line_never_reverse`);
  every other kept file passes.

### 5.2 Development range (seeds 1 to 1,000), informative only

| Family | Measure | Today (WP1) | Solver |
|---|---|---:|---:|
| gap | M1 / M1-hi | 17.70 / 3.79 | 3.11 / 0.31 |
| gap | give-ups with room; scenes complete | 454; 80.5 % | 336; 83.4 % |
| gap | η median / 5th | 0.655 / 0.300 | 0.784 / 0.381 |
| gap | friends > 5 % moving / > 1 % standing / max | 14.68 % / 5.59 % / 0.742 | 1.59 % / 0.03 % / 0.905 |
| cross | M1 / M1-hi | 4.27 / 1.36 | 1.19 / 0.49 |
| cross | give-ups with room; scenes complete | 152; 94.6 % | 2; 99.8 % |
| cross | friends > 5 % moving / max | 3.78 % / 0.537 | 0.00 % / 0.098 |
| corner (B-152) | M1 / M1-hi; give-ups | 20.94 / 4.60; 0 | 0.51 / 0.11; 7 |
| arc (no slots yet) | M1; fill mean / 5th; give-ups | 3.20; 0.980 / 0.889; 25 | 29.85; 0.609 / 0.381; 4,827 |
| corridor (no slots yet) | M1; enemy overlap max; give-ups | 8.77; 0.040; 275 | 25.76; 0.217; 4,991 |
| mirror (WP5 not applied) | blue share | 0.438 [0.400, 0.476] | 0.452 [0.414, 0.490] |

On the families WP3 is judged on, the solver improves on today almost everywhere (gap reversals ÷ 5.7, crossings
complete in 99.8 % of scenes, throughput +20 %), but on this range it would still fail T1 (gap 3.11 and cross 1.19
above 1.0), T4 (gap 336 give-ups, K-10; cross 2) and T7 (gap: 1.59 % of moving near pair-ticks above 5 %, max 0.905).
Arc and corridor wait for WP4's slots, as in §5.3 of the plan ("noslot"). T8 holds on this range (§5.4).

### 5.3 Cost (instructions retired, release benchmark, `perf stat -e instructions:u`, under the gate lock)

| Scenario (400 units, 300 ticks) | Legacy | Solver | Change | Ticks/s Legacy / Solver |
|---|---:|---:|---:|---:|
| 64 × 64 flat, armies meet | 9.91 G | 18.56 G | **+87 %** | 158 / 91 |
| 128 × 128 flat, armies apart | 10.25 G | 12.40 G | **+21 %** | 161 / 53* |
| showcase canyon | 17.61 G | 31.13 G | **+77 %** | 80 / 56 |

*Wall-clock figures were taken on a loaded machine; the instruction counts are the measure. Every scenario stays above
real time (25 ticks per second).

**Iterations and rebuilds per tick** (scratch counter `contact_iterations`, `contact_rebuilds`):

| | Mean iterations | Ticks at K = 21 | Ticks with ≤ 1 | Pair-list rebuilds per tick |
|---|---:|---:|---:|---:|
| 64 × 64 flat | 15.75 | 69.9 % | 22.8 % | 1.56 |
| 128 × 128 flat | 1.00 | 0 % | 100 % | 0.00 |
| showcase canyon | 20.36 | 92.4 % | 0.3 % | 1.53 |
| Harness, all scene families (4.77 M solver ticks) | 6.54 | 90th percentile 21 | median 1 | — |

**Where the extra instructions go** (64 × 64, inclusive shares of a DWARF call-graph profile): of the +8.65 G,
- **contact +4.56 G (53 %)**: the solver is 26.6 % of the Solver run (`accumulate` 15.3 %, `apply` with the terrain step
  6.7 %, pair builds 3.6 %), against 3.8 % for the legacy collision phase;
- **intent and steering +2.30 G (27 %)**: the movement phase is 5.66 G against 3.36 G, mostly `gather_neighbours`
  (11.5 % of the run against 4.2 %) because the reach adds the side horizon `h · (r + r_max)`; the TTC rule itself is
  cheaper than legacy steering's `remove_blocked_velocity` and `surround_walk` (16.6 % of the legacy run);
- **target acquisition +1.52 G (18 %)**: a behaviour side effect (more units within scan of each other per tick);
- **slots: 0** (WP4 not built).

**Why the early stop does not help in battle.** The stop fires only in an iteration in which no body moved. A crowd
pressing into a fighting or anchored line keeps overlaps the averaged iterations cannot clear (K-12), so 70 % to 92 %
of battle ticks run all 21 iterations. A scratch variant that also stops once no pair exceeds D-α's 5 % (not committed)
barely changes it: mean 15.02 and 18.49 iterations, 66 % and 79 % of ticks still at K.

### 5.4 Relabelling (T8)

T8 holds in every family under the Solver (sign tests, M1 / completion tick): gap 0.274 / 0.025, cross 1.000 / 0.180,
arc 0.445 / 0.860, corridor 0.523 / 0.541, corner 1.000 / 0.595; today's pipeline failed it on cross (completion tick
p < 0.001) and corridor (M1 p = 0.004). The relabelled runs were first played under Legacy by a run-script error and
were re-run under the Solver; only the corrected figures are reported.

### 5.5 Options (for the orchestrator; none implemented)

The 10 % stop of WP6 is out of reach with this design: contact alone adds 46 % on the 64 × 64 battle. Each option
below is a design change by amendment; the figures are estimates from §5.3.

1. **Restrict iteration to the active set.** After the first iteration, iterate only over pairs whose overlap exceeds
   the dead band and over the bodies they touch, rebuilding that subset each iteration (order-independent: it is a
   function of positions). In a battle most pairs are within the band after one or two iterations; the cost of later
   iterations falls with the active set rather than the whole list. It does not by itself reduce the 21 iterations the
   pressed front needs, so it helps most where few pairs are deep.
2. **Gauss–Seidel with a fixed, order-independent colouring.** Colour the pair graph (or the bodies by grid cell parity)
   by a rule that depends only on positions and ids, and sweep colour by colour; corrections propagate within an
   iteration instead of one neighbour per iteration, which typically cuts the iterations needed by 2× to 4×. Needs a
   new C-3 derivation and confirmation (Amendment 2's method).
3. **A lower K with a stated residual for pressed crowds.** The infeasible wedges of K-12 cannot be cleared at any K;
   cap the iterations (for example K = 8) and accept that pressed fronts keep their residual until the press ends,
   with T7 measuring what is visible. Contact cost would fall by about 60 % on the battle scenarios (8 / 15.75 to
   8 / 20.36 of today's iterations), still leaving roughly +20 % to +30 % overall with the neighbour reach.
4. **Do not widen the neighbour reach for all units.** Gather the horizon reach only for units whose step is
   unblocked by a co-moving friend, or cap it at today's reach plus one step; saves most of the +2.30 G of §5.3.
5. **Cheaper normals.** `direction` divides in 128 bits per pair per iteration (`__udivti3` is 13 % of the Solver run);
   computing `1 / |d|` once per pair per iteration and multiplying, or reusing the normal across iterations while a
   pair moves less than a tolerance, is an implementation change with no behaviour change if done exactly.

A combination of 1, 4 and 5 is an implementation-level saving that keeps the plan's behaviour; 2 and 3 change the
solver's convergence and need C-3 re-derived. Whether any combination reaches +10 % is not known; the K-12 press keeps
the battle scenarios at K whatever the per-iteration cost.

## 6. WP3 under Amendment 3: cost step done; stopped on the pressed-front rule

Base: `contact-solver-plan` at `fc6966e7` (main `dd17f4c5`) with the WP3 side branch `contact-solver-wp3-wip`
(`47585fb2`) merged. Every figure is on the **development range** (seeds 1 to 1,000 per family, scene families also
relabelled, mirror 1 to 1,000), one run at a time under `flock ~/scratch/gate.lock`, `--jobs 4`. Neither the
acceptance nor the held-out range was run. Binaries, CSVs, traces and profiles are under
`~/scratch/contact-solver/wp3b/`.

| Step (Amendment 3) | State | Branch / commit |
|---|---|---|
| 1. Cost, behaviour-preserving (options 1, 4, 5) | **Done**: rows byte-identical | `62431054` (`contact-solver-wp3-wip2`) |
| 2. Pressed-front rule | **Stop**: T5 worse on development (§6.2) | `7d9e51f1`, probe `ad8c7111` (`contact-solver-wp3-wip2-pressed`) |
| 3. Gauss–Seidel | Not started (the stop comes first) | — |
| 4. WP3b, arrival rule | Rule as written fails its own constraint; a variant measured (§6.3) | `a33a697a`, `5d8440fe` (`contact-solver-wp3-wip2`) |
| 5. The two kept tests | Traced and classified (§6.4); one defect fixed | `12333567` (`contact-solver-wp3-wip2`) |

`contact-solver-plan` carries no WP3 code: WP3's prescribed test (`gaps_and_crossings_meet_t4_and_t7_under_the_solver`)
is still red, so the plan branch stays at WP2 with main merged. **Schema:** main (`f399d403`) took schema 33 for the
harvest return delay; the solver's movement-memory fields move to **34** (WP7 to 35). With the three fields unhashed
and the schema held at 33, main's goldens (20 + 60 checkpoints) and sandbox checksum reproduce on native debug and
release and `wasm32-wasip1` release and debug; the re-recorded lists agree on all four.

### 6.1 Cost, behaviour-preserving (step 1)

Three exact changes, each tested against the arithmetic it replaces:

- **Active set (option 1).** A pair keeps its last correction and reuses it while neither body moved since; a body
  whose terrain step left it in place is not stepped again toward the same target. Both are functions of positions,
  so the result is bit-identical.
- **Neighbour reach (option 4).** The side rule's wider reach no longer costs a ground test per far body: a body
  beyond the legacy (near) reach is listed without its ground test, and the side rule makes the test only for a body
  that is a threat or the committed partner. The side rule's co-moving test (a normalisation) also runs only for
  such bodies. Each is an exclusion that never fails, so the threat found is the same.
- **Normals (option 5).** A pair in the dead band needs no root (`⌊√s⌋ ≥ T ⟺ s ≥ T²`); an active pair takes one
  root for its distance and its normal. `integer_sqrt` takes the 64-bit root of the top bits and one 64-bit division
  instead of a Newton loop of 128-bit divisions, and the normal's quotient is long division on 64-bit hardware
  division. Both are tested equal to the replaced code (200,000 values each plus edge cases).

**Byte-identity.** All eleven CSVs of the development corpus under the Solver (five scene families, each relabelled,
and the mirror) are byte-identical to WP3's (`runs/c3-1` against `wp3/runs/solver-1`), and the Legacy rows of seeds
1 to 20 of every scene family are identical to WP1's.

**Instructions retired** (release benchmark, 400 units, 300 ticks, `perf stat -e instructions:u`; G = 10⁹):

| Scenario | Legacy | WP3 (`47585fb2`) | Step 1 (`62431054`) | Step 2, pressed (`7d9e51f1`) | Candidate (`5d8440fe`) |
|---|---:|---:|---:|---:|---:|
| 64 × 64 flat, armies meet | 9.91 G | 18.56 G (+87 %) | 15.34 G (**+55 %**) | 15.46 G (+56 %) | 15.32 G (+55 %) |
| 128 × 128 flat, armies apart | 10.25 G | 12.40 G (+21 %) | 11.08 G (**+8 %**) | 11.10 G (+8 %) | 11.10 G (+8 %) |
| showcase canyon | 17.61 G | 31.13 G (+77 %) | 26.67 G (**+51 %**) | 26.62 G (+51 %) | 27.06 G (+54 %) |

Percentages are against Legacy at `fc6966e7`. The exact arithmetic also serves Legacy, whose rows do not change and
whose count moves by under 1 % (9.83 G, 10.29 G, 17.47 G at `62431054`).

**Where the rest goes** (64 × 64, cycles, inclusive, `perf record --call-graph=dwarf`, step 1 against Legacy at the
same commit): contact +3.4 G (the solver is 24 % of the run: pair sums 12 %, terrain steps 7 %, pair builds 4.5 %);
target acquisition +1.6 G (39 % of the run against 44 % of a smaller one: more units in scan of each other, a
behaviour effect); the movement phase +0.6 G. Contact is therefore not the whole gap: even a free solver would leave
the 64 × 64 battle near +25 %.

### 6.2 The pressed-front rule (step 2): stop

**Rule as built** (`7d9e51f1`): under Solver, a step that closes on a friend that holds its ground
(`SteeringObstacle::anchored`: hold, immobile, unpushable, or anchored in combat) and ends touching it stands
instead (`Idle`, no velocity); for an attacker such a tick adds no stall (Q0089, D-4). Test first:
`a_unit_pressed_into_a_holding_line_stands_and_keeps_its_order` (red without the rule).

**Development figures** (gap, cross and corner rows are byte-identical to step 1: nobody there is anchored):

| Family | Measure | Step 1 | Pressed |
|---|---|---:|---:|
| arc | M1 / M1-hi | 29.85 / 0.71 | 1.86 / 0.63 |
| arc | M2 / M3 | 0.2474 / 1.442 | 0.0170 / 0.011 |
| arc | give-ups | 4,827 | 23 |
| arc | **fill mean / 5th (T5)** | **0.609 / 0.381** | **0.163 / 0.050** |
| arc | friends > 5 % moving / > 1 % standing / max | 0.93 % / 3.13 % / 0.997 | 0.47 % / 0.11 % / 0.996 |
| corridor | M1 / M1-hi | 25.76 / 4.76 | 0.03 / 0.00 |
| corridor | give-ups | 4,991 | 0 |
| corridor | fill mean | 0.945 | 0.815 |
| corridor | enemy max; friends > 5 % moving / > 1 % standing / max | 0.217; 7.53 % / 30.06 % / 1.000 | 0.000; 0.00 % / 0.00 % / 0.010 |
| mirror | blue share | 0.452 [0.414, 0.490] | 0.461 [0.422, 0.499] |
| harness | solver iterations per tick: mean, 90th | 6.54, 21 | 2.19, 5 |

T1, T2 and T7 improve on arc and corridor, but **T5 gets much worse** (arc fill 0.609 → 0.163). By Amendment 3,
item 2 that is a stop.

**Trace** (arc seed 5: pillar 0.5, range 1, r 0.5625, 28 attackers, capacity 11). Under the rule one attacker
reaches range and fires; the other 27 stand `Idle` with their order for 680 to 703 of the scene's ticks, in their
block, 1.6 to 7.3 world units from the band (`trace/arc5-p1.txt`; step 1: all 28 walk, 9 in range). The stand
cascades: `is_anchored_in_combat` counts a unit **standing still under an attack order** whose target lives as
anchored, so a unit that stood because it pressed is anchored on the next tick, and the unit behind it presses into
it and stands, and so on down the column. The rule meets the legacy definition of anchored, which was written for
units that stand because they fire.

**The rule does not deliver the cost it was meant for.** On the release benchmark it barely moves the iterations:
64 × 64 mean 15.75 → 15.42 (ticks at K 69.9 % → 68.5 %), canyon 20.36 → 20.36 (92.4 % at K); instructions within
1 %. In those battles the overlaps that keep the solver at K are not a crowd pressing into anchored friends (K-12)
but dense moving crowds, in which some pair always exceeds the 1 % dead band after an averaged iteration, so the
early stop (no body moved) never fires.

**Probe, not adopted** (`ad8c7111`): only friends on hold, immobile, unpushable or in a weapon cycle stop a step.
Arc seeds 1 to 100: fill 0.365 against 0.549 (step 1) and 0.129 (pressed), give-ups 269 against 554 and 0, M1 35.5
against 29.2 and 4.1. It removes the cascade but not the loss of fill, and it is worse than step 1 on M1.

### 6.3 WP3b, the group-arrival chain (step 4)

**Trace of B-158 under the Solver** (gap seeds 12, 15, 16, `trace/gap-giveups-p1.txt`): every give-up is a unit of
push priority 10 at the edge of the clump of arrived priority-20 units, 3.2 to 4.7 from the shared point, touching
an arrival 2.5 to 3.1 from it. The L14 witness must lie within `r + r_o + 1.5` (2.16 for seed 12); the unit cannot
push the arrival (C-2 row 6) and gives up after 10 s.

**The rule as written** (`a33a697a`): under Solver a give-up records no arrival (the legacy `finish` set
`arrived_at` for a `NoProgress` give-up too, so give-ups were witnesses), and a witness needs no bound on its
distance from the point. Tests first: `mixed_priority_walkers_all_arrive_at_the_shared_point` and
`a_give_up_is_no_witness_of_the_arrival_chain` (both red on the previous commit). It removes the give-ups of seeds
12, 15 and 16, but on gap seeds 1 to 20 it **ends orders short of the wall's far side** in single-file scenes
(seeds 2, 6, 9, 14; r 0.875 to 1; no give-ups, 6 units not complete). Trace of seed 6 (15 units, r 1): the units
arrive one behind the other and each stops on touching the one before, so the arrivals form a line from the point
back toward the gap, 0.12 to 17.49 from the point. Through arrivals alone, the chain still runs back along the
column: excluding give-ups is necessary but not sufficient. On gap seeds 1 to 20 it trades 5 give-ups (seeds 12, 15, 16) for 6 units short of the far side (seeds 2, 6, 9, 14).

**Variant measured** (`5d8440fe`, proposed in §6.5): beyond L14's bound, only an arrival the unit cannot push
(higher push priority, C-2 row 6) is a witness; with every other arrival the unit pushes on, as today. Gap seeds 1
to 20: no deadlock, no give-up.

### 6.4 The two kept tests (step 5)

- **`battlefield_bubble_trajectories_are_smooth_without_micro_jitter`: a defect, fixed.** Trace (canyon, unit 4,
  `trace/bubble-solver.txt`): at tick 1 the parked-corner aim (L24, kept) finds unit 6 standing on the unit's corner,
  2.0 away, a body the legacy pipeline never sees: it lies beyond the legacy reach (1.35) and inside the side rule's
  wider one. Rounding then follows the bodies on the corner as they walk on (`corner_round` 1, the parked body
  switching from unit 6 to unit 5 at tick 6), and the aim jumps: a 59° turn and a 13° turn back (the zig-zag).
  WP3 widened the reach for the side rule only (I-3a); the other rules were not meant to read it. Fix
  (`12333567`): every rule but the side rule reads the near reach. Test first:
  `the_canyon_march_has_no_zig_zag_under_the_solver` (red without the fix).
- **`test_move_command_units_fan_out_and_surround_target_point`: a consequence of the rule, to be restated.**
  Trace (`trace/fan-*.txt`): eight units with their own `Move` to a point occupied by an immobile friend; at the
  judged tick (58) the spread is 3.79 under Legacy and 2.77 under the Solver. Both crowds converge on the point and
  end around it with spreads 1.97 and 1.74; under the Solver they are 0.3 to 0.5 further along at tick 58 and closer
  together, because converging co-moving friends are ignored by intent (I-4) and separated by contact instead of
  blocked at proposal time (D-β). WP4's slots do not apply (a `Move`, not an attack). The property the test names,
  fanning out around an occupied point without overlap, holds; the fixed spread at a fixed tick measures the legacy
  blocking. Proposed restatement at WP6, when the Solver becomes the shipped pipeline: every unit ends within its
  radius sum plus the touch margin of the occupied point's clump, the clump surrounds the point (the units' bearings
  from it span more than a half-turn), and no pair overlaps beyond D-α. Not loosened: unchanged until then.
- **Kept files under Solver** (shipped pipeline flipped by hand, candidate `5d8440fe`): of the 16 kept movement files, 12 pass whole. `battlefield_bubble_trajectories_are_smooth_without_micro_jitter`
  now **passes** under the Solver. Failing: `test_move_command_units_fan_out_and_surround_target_point` (above) and
  the three tests §8.1 already marks *restated* in WP3, in their legacy form (`test_converging_units_respect_speed_cap_and_do_not_vibrate`,
  `units_blocked_by_a_holding_line_never_reverse`, `test_idle_friendly_displaced_laterally`), whose restated forms
  pass in `solver_pipeline_tests.rs` (`trace/kept-under-solver-a2.log`).

### 6.5 Candidate state and development figures

Candidate `5d8440fe` = step 1 + WP3b variant + the near-reach fix, without the pressed rule:

Rows of arc, cross, corridor, corner and the mirror are **byte-identical** to step 1; only the gap family moves.

| Family | Measure | Legacy (WP1) | Step 1 | Candidate | Threshold |
|---|---|---:|---:|---:|---|
| gap | M1 / M1-hi | 17.70 / 3.79 | 3.11 / 0.31 | **1.11 / 0.28** | T1 ≤ 1.0 / ≤ 0.25: fails, just |
| gap | M1 95 % interval | [16.74, 18.63] | [2.57, 3.72] | [1.00, 1.23] | |
| gap | M2 / M3 | 0.0634 / 0.004 | 0.0330 / 0.003 | 0.0286 / 0.000 | T2 holds |
| gap | deadlocks; give-ups; scenes complete | 459; 454; 80.5 % | 332; 336; 83.4 % | **46; 46; 97.8 %** | T4 fails |
| gap | η median / 5th | 0.655 / 0.300 | 0.784 / 0.381 | 0.784 / 0.381 | T6 holds |
| gap | friends > 5 % moving / > 1 % standing / max | 14.68 % / 5.59 % / 0.742 | 1.59 % / 0.03 % / 0.905 | 1.62 % / 0.02 % / 0.905 | T7 fails |
| gap | relabel p (M1 / completion) | 0.436 / 0.248 | 0.274 / 0.025 | 0.176 / 0.036 | T8 holds |
| cross | M1 / M1-hi; give-ups | 4.27 / 1.36; 152 | 1.19 / 0.49; 2 | same | T1, T4 fail |
| corner | M1; give-ups | 20.94; 0 | 0.51; 7 | same | T4, T7 (max 0.758) fail |

The prescribed test `gaps_and_crossings_meet_t4_and_t7_under_the_solver` now passes T4 on its 20 seeds and fails on
T7 (gap: 1.91 % of moving near pair-ticks above 5 %). WP3's development figures are therefore **not met**: T1 (gap
1.11, cross 1.19), T4 (gap 46, cross 2, corner 7 give-ups) and T7 (gap, corner) fail. Cost: 64 × 64 +55 %,
128 × 128 +8 %, canyon +54 %; iterations unchanged from step 1 (64 × 64 mean 15.75, canyon 20.35, 94 % of ticks
at K).

### 6.6 Proposed amendment (for the orchestrator)

1. **Drop the pressed-front rule** (step 2). It worsens T5 by a stand cascade through the legacy definition of
   anchored, and it does not reduce the battle benchmarks' iterations, which it was meant to do. The concave's
   standing units are WP4's (G-6), where a unit stands only when it has no free slot.
2. **Adopt the near-reach fix** (`12333567`) as a WP3 defect fix: only the side rule reads beyond the legacy reach.
3. **Restate WP3b's rule:** "A unit moving to a shared point completes its order once it touches a friend that has
   arrived at the same point (not given up) and stands, within L14's bound; beyond the bound, only if that friend
   is one it cannot push (C-2 row 6)." Reason: through arrivals alone the chain still runs back along a single-file
   column (§6.3); the push table already says which arrivals a unit must stop at.
4. **Cost.** After step 1 the 64 × 64 battle is +55 % and the canyon +51 %; contact is about 60 % of the excess and
   target acquisition about 30 %. Gauss–Seidel (step 3) attacks only the first part. Before building it, decide
   whether WP6's +10 % applies to the battle scenarios as they stand, or whether the target-acquisition excess,
   a behaviour effect outside the solver, is measured and judged separately.
5. **T1 and T4 on gap** remain just above their thresholds with the candidate (§6.5); the acceptance range stays
   unrun.

## 10. WP4 rebuilt under Amendment 7: stopped on Arm A, and on Arm B by T3 alone

Base: `contact-solver-plan` at `080fccc0` (main `8f91e0d4` merged). Development range only (seeds 1 to 1,000 per
family, scene families relabelled, mirror 1 to 1,000), one run at a time under `flock ~/scratch/gate.lock`,
`--jobs 4`. The acceptance and held-out ranges were not run. CSVs, binaries, traces and the comparator are under
`~/scratch/contact-solver/wp4b/`.

**Baseline.** WP3 is the branch head's Solver pipeline: run `wp3b/runs/a2-1`. It is byte-identical to `c3-1` in
arc, cross, corridor, corner and the mirror. Its gap rows carry the WP3b arrival variant (46 give-ups); `c3-1`'s
gap rows are step 1's (336 give-ups).

**Comparator (E2), fixed before any arm was measured.** `wp4b/compare.py`:
- each T-figure of WP3, with its 95 % bootstrap interval (scenarios resampled, 2,000 resamples);
- a figure is **worse** when it lies beyond that interval on the worse side. The Solver's placebos are silent
  (§6.6 C-c), so the interval is the whole band.
- Maxima (friend, enemy, terrain) are extreme-value statistics: their bootstrap interval ends at the maximum itself,
  so the rule cannot judge them. They are judged by the number of scenes over the T7 bound instead (friends
  > 0.15, enemy > 0.005, terrain > 2⁻¹⁰); the raw maxima are reported.
- T8 is judged by its threshold (p < 0.01 where WP3 holds); T9 by whether the blue share leaves WP3's Wilson
  interval away from 0.5.

| Item | State | Commit |
|---|---|---|
| E1 and E4 in the harness | Committed on the plan branch | `fce01ed6` |
| Arm A | **Stop**: arc worse beyond noise on T1, T3, T4 (E1) and T7 | `08a25b0a` (`contact-solver-wp4b-wip`) |
| Arm B (measured as information after A's stop) | **Stop**: arc M4 (T3) worse beyond noise; every other T-figure equal or better | `cd594ab8` (`contact-solver-wp4b-wip`) |
| TTC term, wall-sliding hard pass, WP5, acceptance | Not started | — |

### 10.1 What was built

- **One definition of anchored** (`SimWorld::engaged`, Solver only): hold, immobile, unpushable, or in range of a
  live target it attacks (current target or `AttackTarget`). Contact reads it (C-2's `engaged`), and so does the
  snapshot's `anchored`. Under the Solver, the only reader of the snapshot's `anchored` is intent's idle-yield
  test, which asks whether contact would push the body aside.
- **The stall rules** (walk and chase) no longer read `friend_holds_ahead` under the Solver.
- **The ring rule** (`movement/ring.rs`), asked once per chase and tick:
  - The ring is the band's outer edge round the target's tick-start position.
  - Blocking arcs come from every tick-start body of the unit's class whose disc reaches the ring (G-2's
    `blocking_interval`), except the unit and its target.
  - An angle is admissible when the unit's terrain disc can walk the ring to it from its own bearing point (mesh
    disc sweeps, sampled every half radius, extended lazily) and it lies within the leash.
  - The rule applies only to a unit outside its band whose own bearing point is admissible.
  - Outcomes:
    - own bearing free: chase straight;
    - no free admissible angle: stand with the order kept and no stall;
    - **Arm A:** otherwise chase straight;
    - **Arm B:** otherwise walk to the nearest free angle (ties clockwise), keeping last tick's gap while it is free
      (one point in `MovementMemory::slot`), braking onto it and measuring progress against it.
- **Deleted:** `resolve_claims`, `SlotRequest`, `ClaimFate`, `claim_fate`. The claims machinery of
  `contact-solver-wp4-wip` (`KeptClaim`, own-angle claims, `queued_toward`, the per-tick buffers) was never
  brought over.
- **Legacy:** bit-identical. Its rows on seeds 1 to 20 of gap, arc, cross and corridor equal WP1's.
- **Gap, cross and corner rows** are byte-identical to WP3's under both arms (no pursuits).

### 10.2 Development figures against WP3 (95 % bootstrap intervals)

| Family | Figure | WP3 | Arm A | Arm B |
|---|---|---:|---:|---:|
| arc | M1 (T1) | 29.85 [28.46, 31.22] | 25.86 [24.72, 27.15] | **3.77** [2.97, 4.70] |
| arc | M1-hi (T1) | 0.714 [0.648, 0.785] | **1.42** [1.30, 1.56] worse | 0.220 [0.191, 0.250] |
| arc | M1 under an order / idle (E4) | — | 26.07 / 20.78 | 3.77 / 4.55 |
| arc | M2 / M3 (T2) | 0.2474 / 1.44 | 0.2180 / 1.21 | 0.0568 / 0.060 |
| arc | M4 % (T3) | 0.0153 [0.0115, 0.0190] | **0.1252** worse | **0.0513** [0.0441, 0.0591] worse |
| arc | give-ups (E1) | 4,827 [4,599, 5,061] | **9,402** worse | 1,204 [1,062, 1,355] |
| arc | fill mean / 5th (T5) | 0.609 / 0.381 | 0.633 / 0.400 | **0.806 / 0.615** |
| arc | friends > 5 % moving / > 1 % standing (T7) | 0.93 % / 3.13 % | **3.96 % / 3.76 %** worse | 0.09 % / 0.24 % |
| arc | scenes: friend > 0.15 / enemy > 0.005 (T7) | 317 / 91 | **665 / 166** worse | 29 / 40 |
| arc | relabel p (M1 / completion) | 0.445 / 0.860 | 0.886 / 0.957 | 0.560 / 1.000 |
| corridor | M1 (T1) | 25.76 [22.95, 28.69] | 0.31 | **0.003** |
| corridor | give-ups (E1) | 4,991 | 233 | **16** |
| corridor | M3 (T2) | 1.08 | — | 0.000 |
| corridor | friends > 5 % moving / > 1 % standing | 7.53 % / 30.06 % | — | 0.00 % / 0.00 % |
| corridor | scenes: enemy > 0.005 | 29 | — | 0 |
| corridor | fill mean (not a T-figure, §6.6 C-e) | 0.945 [0.930, 0.960] | 0.842 | 0.869 lower |
| mirror | blue share (T9) | 0.452 [0.414, 0.490] | 0.415 [0.377, 0.453] | 0.458 [0.420, 0.496] |

Full tables: `wp4b/cmp-a0.md` and `wp4b/cmp-b0.md`.

### 10.3 Traces

- **Arm A, arc give-ups** (`trace/arc1-a0.txt`; arc seed 1: P 2, R 4, r 0.75, k 15, capacity 28).
  - Every give-up is a unit whose bearing point on the ring is blocked by one to three bodies, while only 7 of 28
    places are taken.
  - The ring has room, so the unit does not stand. It chases straight, presses into the front rank, and with no
    exemption from the stall rule it abandons after 10 s.
  - The review expected Arm A to bring arc give-ups near 0. That held only where the ring is full. Where the
    ring has room, the rear units need the gap aim (Arm B).
  - The give-ups then cascade. Abandoned units are idle and are shoved by their moving friends: M1-hi and the
    overlaps rise.
- **Arm B, arc M4** (`trace/arc-m4-b0.txt`; seeds 89, 28, 36).
  - Every violation window is a unit under its unchanged attack order, walking to a gap.
  - Its potential rises by more than its radius in one second (for example 0.34 → 1.14, r 0.625) as it rounds the
    front rank: the side rule and contact move it outward round the bodies between it and the gap.
  - A straight walk to a point on the ring never raises the potential, so the rise is the detour round the crowd.
  - The share is 0.051 % of windows, against T3's 1 %. It is worse than WP3 beyond the noise rule.
- **Arm B, corridor fill** (`trace/corridor4-b0.txt`, `trace/corridor2-b0.txt`).
  - Every short scene has capacity 2. One attacker stands in range on the corridor's axis, and the rest stand with
    their orders.
  - With the first unit on the axis, the ring's nearest free angle lies 2r off the axis. That needs `3r ≤ w/2` to
    fit, while the capacity counts two abreast at ±(r + 1/32). Examples:
    - seed 2: w 2, r 0.375, disc 1.125 > 1;
    - seed 4: w 3, r 0.5, exactly 1.5 = 1.5, refused.
  - WP3 reached the second place by squeezing in contact. Under the ring rule the unit stands for want of a free
    angle.
  - This is no T-figure (T5 is arc only). But M5's known answer, `min(k, ⌊w/(2r + 1/16)⌋)` in range, is missed in
    about 29 % of corridor scenes (development seeds 1 to 100).

### 10.4 Reading (for the orchestrator; no rule change proposed)

- By Amendment 7, item 7, both arms stop.
- **Arm A** is worse on T1 (M1-hi), T3, T4/E1 and T7 in the arc family.
- **Arm B** is worse only on arc M4 (T3), and stays 20 times below T3's absolute bound. It is better beyond noise
  on every other arc and corridor T-figure.
- **Arm B still fails these thresholds on development:**
  - T5: arc fill 0.806 / 0.615 against 0.90 / 0.7476;
  - E1: 1,204 arc give-ups and 16 corridor give-ups;
  - T7: arc enemy over 0.005 in 40 scenes;
  - T1: arc M1 3.77;
  - T9: the mirror.

## 12. Step 4, WP5 and the acceptance run (Amendment 9)

Base: `contact-solver-plan`. Every development figure is on seeds 1 to 1,000. The acceptance run is the single run
of Amendment 9, seeds 100,001 to 101,000, both pipelines from one binary (`bin/harness-w5`). The held-out range was
not run. Data: `~/scratch/contact-solver/wp4b/` (`runs/s4-1`, `runs/w5-1`, `runs/acc-*-100001`,
`acc-aggregate.md`, `cmp-*.md`, `perf/`, `trace/acc/`).

### 12.1 Arm B's goldens

Arm B's tree (`d9c2e763`) reproduces the committed schema 34 lists and `DEFAULT_SANDBOX_CHECKSUM` unchanged on all
four builds. The hashed field set is WP2's (`side_partner`, `side_left`, `slot`), and the legacy pipeline never
writes the gap.

### 12.2 Step 4: wall slide and prioritised hard pass (`949d517a`)

- A correction that terrain refuses whole slides along the wall (`slide_along_wall`).
- The hard pass runs in two parts: the hard friend pairs first, then the enemy and fixed constraints, last.
- Test `enemy_constraints_have_the_last_word` is red before.
- `a_refused_correction_slides_along_the_wall` passes before too: `settle_on_terrain` already projects most
  refused corrections. It pins the behaviour.
- **Development, against Arm B:** nothing worse beyond noise. Arc scenes over the enemy bound fall from 40 to 25.

### 12.3 WP5: tick-start pursuit (`6f967150`, Solver only)

- The pursuit reads the target's tick-start position and motion.
- Test `a_mirrored_pair_reaches_range_and_fires_on_the_same_tick` fails without the read: range on ticks 73 and 74.
- **Development, against step 4:** scene families byte-identical. Mirror blue share 0.458 → 0.506 [0.403, 0.608];
  draws 347 → 913, of which 907 end with both sides dead. The symmetric fights now end symmetrically.
- The prescribed WP3 test `gaps_and_crossings_meet_t4_and_t7_under_the_solver`, judged at WP5: T4 holds on its 20
  seeds; T7 fails (gap 1.91 % of moving near pair-ticks above 5 %). It stays ignored, with that reason.

### 12.4 Acceptance (T1 to T9 with E1; today = Legacy on the same range)

| Threshold | Family | Legacy | Solver | Verdict |
|---|---|---|---|---|
| T1 | gap | M1 17.97, M1-hi 3.82 | 1.00 / **0.26** | fails (M1-hi > 0.25) |
| T1 | arc | 3.15 / 1.97 | **4.36** / **0.26** | fails |
| T1 | cross | 3.86 / 1.23 | **1.29** / **0.51** | fails |
| T1 | corridor, corner | 11.23, 20.38 | 0.00, 0.48 | holds |
| T2 | all | | | holds |
| T3 | all | | arc 0.05 %, cross 0.46 % | holds |
| T4 | gap | 322 give-ups, 84.3 % complete | **29**, 98.0 % | fails |
| T4 | cross | 121 | 0, 100 % | holds |
| T4 | corner | 0 | **11**, 99.7 % | fails |
| T4 (E1) | arc | 41 | **1,290** | fails |
| T4 (E1) | corridor | 325 | **17** | fails |
| T5 | arc | fill 0.978, 5th 0.889 | **0.802, 0.600** | fails |
| T6 | gap | η 0.640 | 0.764, 5th 0.385, no drop | holds |
| T7 | gap | friends > 5 % moving 14.78 % | **1.40 %**, max **0.990** | fails |
| T7 | arc | enemy 0.0196 | enemy **0.0100**, friend max **0.715** | fails |
| T7 | corner | max 0.575 | max **0.632** | fails |
| T7 | cross | max 0.538 | max **0.240** | fails |
| T7 | corridor | enemy 0.0484, max 0.966 | 0, 0.047 | holds |
| T8 | all | cross p 0.000, corridor p 0.019 | lowest arc 0.020 | holds |
| T9 | mirror | 0.452 [0.415, 0.490] | 0.544 [0.427, 0.657], 932 draws | holds |

The relative thresholds, M1 and M2 not above Legacy's, hold everywhere except arc M1 (3.15 → 4.36).

- **Solver iterations per tick:** mean 2.55, median 1, 99th 21.
- **Corridor two-abreast (open item, not a T-figure):** every capacity-2 corridor scene misses its known answer
  under the Solver: 301 of 301, against 210 of 301 under Legacy. With the first attacker on the axis, the ring has no
  free angle beside it (§10.3).

### 12.5 Traces of the failures (`trace/acc/`)

- **Arc give-ups and T5** (seed 100313: P 1.5, R 1, k 23, capacity 15; 11 in band at the end).
  - The abandoning units stand 0.5 to 1.7 from the ring with their bearing point blocked: walkers to a gap.
  - The gap aim walks the straight chord toward the gap, which runs into the front rank. Contact holds the unit
    there, it makes no progress toward the gap point, and it abandons after 10 s.
  - The give-ups are the shortfall of T5: Legacy, whose units walk round the crowd, has 41 give-ups and fills 0.978.
- **Arc T1** (seed 100804; 459 reversals).
  - Every reversal is a gap walker under its order, with a fixed gap point, stepping ±0.04 for hundreds of ticks.
  - It steps toward a gap point on the band edge beside a front-rank body and is pushed back each tick: a limit cycle
    between the aim and contact.
- **Arc T7 enemy.** All 46 scenes over 0.005 lie in (0.005, 0.00995]: inside the solver's dead band,
  `dead_band_permille: 10`, which never corrects an overlap of at most 1 % (C-4). T7's enemy bound (0.5 %) is below
  the solver's own dead band.
- **Gap T4** (seed 100166: w 2, r 0.875, single file).
  - `NoProgress` give-ups of walkers queued 15 to 17 from the goal, moving slowly (0.03 to 0.06 a tick) behind the
    file.
  - These are the residual of B-158 / WP3b (46 on development).
- **Corner T4 and T7** (seed 100390).
  - Walkers at the mouth give up (`NoProgress`).
  - The idle units they leave are then pressed above 0.15 by walkers (0.16 to 0.28).
- **Gap T7** (seed 100667: w 6, mixed priorities): moving pairs of the starting block pressed above 0.15 in the
  first 50 ticks. 230 scenes go over the friend maximum.
- **Cross T7** (seed 100996, θ 90°): an arrived, idle unit pressed to 0.17 by passing walkers; 2 scenes.
- **Cross T1** (seed 100050, θ 180°): reversals at ticks 113 to 174 as the head-on groups meet. This is WP3's side
  rule's residual (Amendment 9 kept it).
- **Corridor E1** (seed 100016, capacity 3): one unit gives up 1.05 from the ring, with two in band. It is a gap
  walker that cannot reach the third place.

### 12.6 Cost (release benchmark, 400 units, 300 ticks, `perf stat -e instructions:u`, under the gate lock)

| Scenario | Legacy | Solver | Change |
|---|---:|---:|---:|
| 64 × 64, armies meet | 9.86 G | 16.82 G | **+71 %** |
| 128 × 128, armies apart | 10.33 G | 11.08 G | +7 % |
| showcase canyon | 17.49 G | 27.22 G | +56 % |

**Split on 64 × 64.** The method: `perf record --call-graph=dwarf`, inclusive cycles, each part's share scaled by
the run's instructions retired.

| Part | Legacy | Solver | Change |
|---|---:|---:|---:|
| Contact (`resolve_collisions_and_push`; the solver is 22.2 % of the run) | 0.27 G | 3.86 G | +3.6 G |
| Target acquisition | 4.41 G | 5.95 G | +1.5 G |
| Movement phase | 3.22 G | 4.92 G | +1.7 G |
| — of which intent | — | 1.03 G | |
| — of which the ring scan | — | 0.95 G | |

The ring rule raises the 64 × 64 battle from +55 % (WP3) to +71 %. Every scenario still runs faster than real time
(ticks per second at the median: 71, 132 and 72).

## 13. Amendment 11: the second review's steps

Base: `contact-solver-plan` at `e6ceed93` (main `091b85cc` merged, schema 35). Development range only (seeds 1 to
1,000). The fresh acceptance range, 200,001 to 201,000, is untouched. The baseline of every step comparison is the
previous step's development run; the first baseline is WP5's run, `wp4b/runs/w5-1`. Data:
`~/scratch/contact-solver/wp4b/`.

### 13.1 Step 0: diagnostics (no behaviour change)

- **D-S in the harness** (`ScenarioRow::stuck_with_room`, CSV column `stuck_with_room`, table row "Stuck with room").
  - It counts ring-family unit-ticks of attackers under an order whose potential has not fallen by 0.1 for 250
    ticks, while a point of the ring, reachable from the unit's bearing without its terrain disc entering terrain,
    has no other body's disc touching the unit's.
  - Test: `an_attacker_held_short_of_an_empty_ring_is_stuck_with_room`.
- **Per-tick log** (scratch tool `trace/zz_wp4b_trace.rs`, not committed). For each reversal it logs:
  - the remembered gap (the ring decision);
  - the side commitment;
  - the proposed step (`SimWorld::proposed_step`);
  - the solver's correction (the achieved step less the proposal);
  - the gaps to the bodies near the unit.
- **The ±0.04 alternation of acceptance seed 100804 reproduces on development seeds 563, 498 and 698**
  (`trace/d0/`).
  - It is the same mechanism in every one of their 1,069 reversals:
    - the unit is a gap walker;
    - it touches a body (gap between −0.03 and 0);
    - its committed side partner changes from tick to tick (for example 22 → 16 → 22) with the side flipping;
    - the proposal swings by about 90° between ticks (for example (0.024, −0.097) → (−0.042, −0.091)), and the
      solver's correction pushes back.
  - Of the three candidates it is the second: **the side rule switches off at contact.** `threat_of` drops a body
    whose gap is at most zero, so the earliest threat becomes another body on the other side, the commitment moves
    to it and the side flips. Contact then separates the pair, and the first body is a threat again.
  - The aim–contact cycle is how the swing becomes a reversal, but the swing comes from the side rule.
  - The hard pass does not appear: the corrections are friendly and averaged.

### 13.2 Step 1: P1 exact blocking geometry, P7 no enemy dead band (`f27acf84`, `0cc50327`)

- **P1.** `blocking_interval` takes the exact half-width `acos((ρ² + d² − R²) / (2ρd))`.
  - Test `a_ring_filled_greedily_holds_its_capacity` runs over all 175 arc rings and is red before (for example
    P 0.5, R 0.1, r 0.5625: 4 placed against a capacity of 6).
- **P7.** Enemy pairs take no dead band.
  - Test `enemy_pairs_take_no_dead_band` is red before: a 0.8 % enemy overlap stays.
  - The slide test now allows the wall rule's rounding, which the enemy's last word reaches.

**Development, against WP5** (`runs/w5-1` → `runs/p17-1`): nothing is worse beyond noise.

| Family | Figure | WP5 | Step 1 |
|---|---|---:|---:|
| arc | M1 / M1-hi | 3.64 / 0.223 | **2.48 / 0.192** |
| arc | give-ups (E1) | 1,208 | **305** |
| arc | fill mean / 5th (T5) | 0.805 / 0.600 | **0.967 / 0.857** |
| arc | fill, scenes without give-ups | 0.804 | 0.979 |
| arc | M4 | 0.052 % | 0.006 % |
| arc | scenes over the friend / enemy bound | 32 / 25 | **1 / 0** |
| arc | D-S, unit-ticks | — | 102,537 |
| corridor | fill / give-ups | 0.869 / 16 | 0.908 / 20 |
| corridor | D-S | — | 2,502 |
| gap, cross, corner | | | byte-identical |
| mirror | blue share | 0.506 [0.403, 0.608] | 0.554 [0.453, 0.652] |

Against WP3's thresholds, arc T5 and arc T7 now hold on development. Arc M1-hi (0.192) is within T1's 0.25, but arc M1
(2.48) still fails T1's 1.0. `cargo gate quick` is green on this tree.

### 13.3 Step 2: P2, the detour for gap walkers: stopped by Amendment 8's rule (side branch `contact-solver-p2-wip`)

**Built.**
- When the segment from a gap walker to its gap point passes a tick-start body closer than the two radii, the unit
  aims at the point of the outer circle of radius `max(|x − T|, ρ + r + r_max)` one diameter of arc on from its
  bearing toward the gap, never past the gap.
- Its progress is still measured to the gap point. It dives in once the way is clear.
- The design is stateless.

**Development, against step 1** (`runs/p2-1`).

| Family | Figure | Step 1 | P2 | Verdict |
|---|---|---:|---:|---|
| arc | give-ups (E1) | 305 | **1** | P2 criterion met: falls beyond noise |
| arc | fill, scenes without give-ups | 0.979 | 0.924 | P2 criterion met: at least 0.85 |
| arc | fill mean / 5th | 0.967 / 0.857 | 0.924 / 0.786 | worse, still within T5 |
| arc | D-S | 102,537 | 4,588 | |
| arc | M1 / M1-hi (T1) | 2.48 / 0.192 | **9.85 / 8.15** | **stop**: arc M1-hi fails T1 and is worse than WP3's 0.71 |
| arc | M4 | 0.006 % | 0.231 % | within T3 |
| corridor | M1 / give-ups / D-S | 0.00 / 20 / 2,502 | **12.96 / 618 / 53,260** | far worse than step 1; not worse than WP3 |
| corridor | scenes over the friend / enemy bound | 0 / 0 | **114 / 16** | |
| gap, cross, corner | | | | byte-identical |

**Traces** (`trace/p2/`).
- **Arc, seed 150** (M1-hi 0 → 934): units 5 and 2 alternate every tick between two states.
  - One tick they are a gap walker on a detour: their bearing point is blocked and they step out along the outer
    circle at full speed, proposal (0.033, −0.184).
  - The next tick their bearing point is free, so they chase straight in: no gap, proposal (−0.041, 0.183).
  - The detour step itself moves the bearing onto a free point, and the straight chase moves it back.
  - This is a two-tick limit cycle between the detour and the ring rule's "own bearing free, chase straight". Each
    swing is a full-speed reversal.
- **Corridor, seed 976** (M1-hi 0 → 283, give-ups 0 → 2):
  - In the corridor the outer circle lies in the walls beyond the corridor's width.
  - Gap walkers aim at points they cannot reach, are corrected back (corrections up to 0.14 a tick), and stall into
    give-ups.
  - The detour has no terrain test.

**Reading.**
- P2 meets the two stop criteria that Amendment 11 gives it.
- But under Amendment 8's rule it stops: arc T1 (M1-hi) gets worse than WP3.
- The wavefront fallback is conditional on P2's own criteria failing, and they did not fail, so it was not run.
- Hand-back point (orchestrator's request): after step 2's verdict.

### 13.4 Step 1b (Amendment 12): a closing body in contact stays a threat: stopped by the step rule (side branch `contact-solver-1b-wip`)

**Built** (`56cb6c1d` on the side branch).
- `threat_of` returns a body in contact that the unit still closes on as a threat with time to contact zero (ORCA's
  colliding case). A body in contact that the unit moves away from stays no threat.
- Test `a_closing_body_in_contact_stays_the_threat` is red before.

**Development, against step 1** (`runs/p17-1` → `runs/p1b-1`).

| Family | Figure | Step 1 | 1b | Rule |
|---|---|---:|---:|---|
| arc | give-ups (E1) | 305 | **2** | better |
| arc | D-S | 102,537 | 19,626 | |
| arc | M1 / M1-hi (T1) | 2.48 / 0.192 | **7.86 / 5.21** | **stop**: M1-hi passed T1 and now fails |
| arc | relabel p, completion (T8) | 0.871 | **0.001** | **stop**: T8 passed and now fails |
| gap | M1 / M1-hi (T1, WP3 fails) | 1.11 / 0.283 | **3.64 / 2.17** | **stop**: worse |
| cross | M1 / M1-hi (T1, WP3 fails) | 1.19 / 0.486 | **2.42 / 1.88** | **stop**: worse |
| cross | give-ups (T4, WP3 fails) | 2 | **16** | **stop**: worse |
| corner | M1 / M1-hi (T1) | 0.499 / 0.111 | **1.95 / 1.11** | **stop**: T1 passed and now fails |
| corridor | give-ups | 20 | 0 | better |

**Trace** (`trace/1b/`; gap seed 481, cross seed 721, corner seed 852, the largest rises in M1-hi).
- **Counts:** 581 of the 642 reversals are units touching a body (gap −0.05 to 0), and 483 come after a change of
  side partner or side since the unit's previous reversal.
- **Ties at contact.** In a dense crowd (the gap mouth, the corner, two groups meeting head-on) a unit touches
  several bodies it still closes on. Every one is now a threat with time to contact zero. The earliest threat is a
  tie, decided by the smallest id, so the partner and its geometric side change as contacts come and go.
- **A full right angle at contact.** `tangent_heading` turns onto the tangent of a disc the unit already overlaps,
  which is the perpendicular to the line of centres (sin α clamps to 1). So each change of partner is a 90° sideways
  step, which contact pushes back. For example, gap seed 481 unit 16: proposal (0, 0.099), correction (0, −0.069).

**Reading.**
- The fix removes the arc give-ups (305 → 2), as step 0's diagnosis predicted for the ring families.
- But treating contact as a threat with time zero hands the one-partner rule a tie wherever several bodies touch,
  and the rule then turns fully sideways. That defect is outside the ring families' single-contact case.
- By Amendment 12, this is a stop.
- Step 2′ (the wavefront) is built (`contact-solver-1b-wip`, commit after `56cb6c1d`) on top of 1b, but was not
  measured: its baseline step stopped, and a run on top of 1b would be confounded. The run started in the queue was
  cancelled.

### 13.5 Step 2′ (Amendments 12 and 13): the wavefront fallback, measured once on step 1: stopped by the step rule (side branch `contact-solver-w2-wip`)

**Built** (`75871a42` on the side branch; module `movement/wavefront.rs`).
- **Field:** one integer breadth-first distance field per contested target and walker radius per tick, built on
  first request and shared.
  - Cell size `h = r/2`. Square of half-size `ρ + 3r + 2·r_max`, at most 512 cells a side.
  - Seeded from the free ring points of step 1's exact intervals.
  - Cells blocked by terrain and by the inflated discs of the target, enemies and anchored friends.
  - 4-connected spread; the walker steps to the lowest of its 8 neighbours, ties in a fixed order.
- **Use:** only while the walker's straight segment to its gap point is blocked. Progress is measured to the gap.
- **Tests:** `the_wave_goes_round_a_wall_and_the_walker_follows_it`, `a_disc_blocks_the_cells_inside_it`.

**Development, against step 1** (`runs/p17-1` → `runs/w2b-1`).

| Family | Figure | Step 1 | 2′ | Verdict |
|---|---|---:|---:|---|
| arc | give-ups (E1) | 305 | **2** | P2 criterion met: falls beyond noise |
| arc | fill, scenes without give-ups | 0.979 | 0.957 | P2 criterion met: at least 0.85 |
| arc | D-S | 102,537 | 4,396 | |
| arc | M1 / M1-hi (T1) | 2.48 / 0.192 | 2.40 / **1.28** | **stop**: M1-hi passed T1 and now fails (worse than WP3's 0.71) |
| arc | fill mean / 5th (T5) | 0.967 / 0.857 | 0.957 / 0.857 | holds |
| corridor | M1 / M1-hi (T1) | 0 / 0 | **1.58 / 1.54** | **stop**: T1 passed and now fails |
| corridor | give-ups / D-S | 20 / 2,502 | 12 / 6,365 | |
| gap, cross, corner | | | | byte-identical |
| mirror | blue share | 0.554 [0.453, 0.652] | 0.463 [0.418, 0.508], draws 529 | holds |

**Traces** (`trace/w2/`).
- **Arc, seed 211** (M1-hi 0 → 32): a gap walker following the field along the front rank touches its bodies (gap
  −0.016 to 0). Its side partner switches between 3 and 9, and the proposal reverses each tick.
- **Corridor, seed 458** (M1-hi 0 → 655): 655 of the 656 reversals are gap walkers with a fixed gap point. The
  side partner of the touching body (gap 0.000) comes and goes (None ↔ 2), and the proposal flips between two
  directions each tick.
- Both are step 0's mechanism, the side rule switching at contact. The field brings walkers to the front rank's
  edge, where that mechanism operates far more often. Step 1b, the fix for it, was stopped (§13.4).

**Cost** (release benchmark, 400 units, 300 ticks, instructions retired; `Legacy` re-measured on this tree, whose
catalog has changed since §12.6).

| Scenario | Legacy | Step 1 | Step 2′ | Q0093 (c): ≤ +60 % and ≥ 50 ticks per second |
|---|---:|---:|---:|---|
| 64 × 64, armies meet | 10.35 G | 17.06 G (+65 %, 98 tps) | 25.22 G (**+144 %**, 65 tps) | step 1 fails by 5 points |
| 128 × 128, armies apart | 10.74 G | 11.49 G (+7 %, 171 tps) | 11.49 G (+7 %, 161 tps) | holds |
| showcase canyon | 18.03 G | 27.75 G (+54 %, 78 tps) | 27.74 G (+54 %, 82 tps) | holds |

**The field's share on 64 × 64** (`perf record`, inclusive cycles of the run):
- the bodies' discs: `block_holding_bodies` 11.4 % with `block_disc` 11.2 %;
- cell centres: 6.4 %;
- the spread: 3.8 %;
- seeding: 1.8 %.

That is about 35 % of the run, roughly +8.2 G.

**Reading.** Step 2′ meets P2's two criteria but stops under Amendment 8's rule. By Amendments 12 and 13 the
remaining steps are not run, and the trade-off goes to the owner with step 1's figures.

### 13.6 Legacy against step 1 on the development range (for the owner)

**Runs.** Development seeds 1 to 1,000 per family, each scene family also relabelled, mirror 1 to 1,000.
- **Legacy:** run on this tree, plan head `dd9118ab` (`runs/legacy-head-1`). Its rows reproduce WP1's baseline
  (gap M1 17.70, M1-hi 3.79, give-ups 454).
- **Step 1:** P1 and P7 on WP5's Solver (`runs/p17-1`). Main's later merges change no movement code.

**How the table is judged.**
- Verdicts are the harness's (`movement_harness --aggregate`), with E1 added to T4. Relative thresholds compare with
  Legacy.
- "Worse than Legacy" means beyond §6.4's noise rule: outside Legacy's 95 % bootstrap interval (comparator rules of
  §10). Maxima are read through the number of scenes over the bound.

| T | Threshold | Family | Legacy | | Step 1 | | Step 1 worse than Legacy beyond noise |
|---|---|---|---|---|---|---|---|
| T1 | M1 ≤ 1.0, M1-hi ≤ 0.25, M1 not above Legacy | gap | 17.70 / 3.79 | fail | 1.11 / 0.28 | fail | no (much better) |
| | | arc | 3.20 / 1.98 | fail | 2.48 / 0.19 | fail | no |
| | | cross | 4.27 / 1.36 | fail | 1.19 / 0.49 | fail | no |
| | | corridor | 8.77 / 2.82 | fail | 0.00 / 0.00 | pass | no |
| | | corner | 20.94 / 4.60 | fail | 0.50 / 0.11 | pass | no |
| T2 | M2 not above Legacy; M3 ≤ 0.5 | gap | 0.0634 / 0.004 | pass | 0.0286 / 0.000 | pass | no |
| | | arc | 0.0991 / 2.15 | fail | 0.0487 / 0.034 | pass | no |
| | | cross | 0.0344 / 0.001 | pass | 0.0216 / 0.000 | pass | no |
| | | corridor | 0.0437 / 0.56 | fail | 0.0040 / 0.000 | pass | no |
| | | corner | 0.0617 / 0.000 | pass | 0.0186 / 0.000 | pass | no |
| T3 | M4 ≤ 1 % | gap | 0.01 % | pass | 0.21 % | pass | **yes** (0.009 → 0.214 %, within T3) |
| | | arc | 1.99 % | fail | 0.01 % | pass | no |
| | | cross | 0.00 % | pass | 0.50 % | pass | **yes** (0.0006 → 0.499 %, within T3) |
| | | corridor | 0.52 % | pass | 0.00 % | pass | no |
| | | corner | 0.00 % | pass | 0.04 % | pass | **yes** (0.003 → 0.041 %, within T3) |
| T4 | 0 deadlocks and give-ups, ≥ 99.5 % complete, no wrong pass | gap | 459 / 454, 80.5 % | fail | 46 / 46, 97.8 % | fail | no |
| | | cross | 152 / 152, 94.6 % | fail | 2 / 2, 99.8 % | fail | no |
| | | corner | 0 / 0, 100 % | pass | 7 / 7, 99.7 % | fail | **yes** (0 → 7 give-ups) |
| | E1: 0 give-ups | arc | 25 | fail | 305 | fail | **yes** (25 → 305) |
| | | corridor | 275 | fail | 20 | fail | no |
| T5 | fill mean ≥ 0.90, 5th ≥ 0.7476 | arc | 0.980 / 0.889 | pass | 0.967 / 0.857 | pass | **yes** (0.980 → 0.967; 0.889 → 0.857, both within T5) |
| T6 | η median ≥ 0.5, 5th ≥ 0.25, no drop | gap | 0.655 / 0.300 / 0 | pass | 0.784 / 0.381 / 0 | pass | no |
| T7 | enemy ≤ 0.5 %; terrain ≤ 2⁻¹⁰; friends > 5 % moving ≤ 0.1 %, > 1 % standing ≤ 0.1 %, max ≤ 15 % | gap | 0 / 0.00008 / 14.68 % / 5.59 % / 0.742 | fail | 0 / 0.00002 / 1.62 % / 0.02 % / 0.958 | fail | no (scenes over the friend maximum 907 → 242) |
| | | arc | 0.0183 / 0 / 1.32 % / 0.22 % / 0.597 | fail | 0 / 0 / 0.01 % / 0.01 % / 0.300 | fail (1 scene over max) | no (274 → 1 scene) |
| | | cross | 0 / 0 / 3.78 % / 1.35 % / 0.537 | fail | 0 / 0 / 0.00 % / 0.00 % / 0.098 | pass | no |
| | | corridor | 0.0398 / 0.00008 / 7.35 % / 3.88 % / 0.988 | fail | 0 / 0 / 0.00 % / 0.00 % / 0.010 | pass | no |
| | | corner | 0 / 0 / 14.89 % / 5.23 % / 0.554 | fail | 0 / 0 / 0.03 % / 0.01 % / 0.520 | fail | no (920 → 23 scenes) |
| T8 | relabel sign tests p ≥ 0.01 | gap | 0.436 / 0.248 | pass | 0.198 / 0.041 | pass | no |
| | | arc | 0.968 / 0.134 | pass | 0.597 / 0.871 | pass | no |
| | | cross | 0.031 / **0.000** | fail | 1.000 / 0.180 | pass | no |
| | | corridor | **0.004** / 0.911 | fail | 1.000 / 1.000 | pass | no |
| | | corner | 0.711 / 0.715 | pass | 0.948 / 0.560 | pass | no |
| T9 | blue share's 95 % Wilson interval contains 0.5 | mirror | 0.438 [0.400, 0.476], 356 draws | fail | 0.554 [0.453, 0.652], 908 draws | pass | no |
| D-S | stuck with room, unit-ticks (reported) | arc | 10,959 | — | 102,537 | — | **yes** |
| | | corridor | 5,624 | — | 2,502 | — | no |

**Totals:**
- **Legacy** passes 13 of the 33 rows judged against a threshold.
- **Step 1** passes 22.
- **Step 1 fails, which Legacy passes:**
  - T4 on corner;
  - T7 maximum on arc (one scene);
  - E1 on arc, which both fail, though step 1 is much further from zero.
- **Legacy fails, which step 1 passes:**
  - T1 on corridor and corner;
  - T2 on arc and corridor;
  - T3 on arc;
  - T7 on cross and corridor;
  - T8 on cross and corridor;
  - T9.

**Step 1 is worse than Legacy beyond noise on these figures only:**
- arc give-ups (E1): 25 → 305;
- arc fill: 0.980 / 0.889 → 0.967 / 0.857 (T5 holds for both);
- arc D-S: 10,959 → 102,537;
- corner T4: 0 → 7 give-ups, 100 % → 99.7 % complete;
- M4 in gap, cross and corner (within T3 for both);
- corridor fill: 0.955 → 0.908 (no T-figure; the two-abreast miss, Q0095).

**Cost** (release benchmark, 400 units, 300 ticks, instructions retired, this tree; §13.5). The Q0093 (c) bar is at
most +60 % instructions and at least 50 ticks per second.

| Scenario | Legacy | Step 1 | Bar |
|---|---:|---:|---|
| 64 × 64, armies meet | 10.35 G | 17.06 G (+65 %), 98 ticks per second | **fails** (+65 % > +60 %) |
| 128 × 128, armies apart | 10.74 G | 11.49 G (+7 %), 171 ticks per second | holds |
| showcase canyon | 18.03 G | 27.75 G (+54 %), 78 ticks per second | holds |

### 13.7 Step 6, P8: the ring interval cache (Q0097's default)

**Built.**
- The ring rule's blocking intervals are computed once per (target, walker radius, ring radius) per tick, every
  body's but the target's, each tagged with its body. A chaser copies them less its own interval.
- The grid query is the scan's own, so the order and the first-fault rule are the scan's.
- `SimWorld::set_ring_interval_cache` switches the cache off for the test that proves it exact.

**Exactness.**
- Test `the_ring_interval_cache_changes_nothing`: arc seeds 1, 5 and 563 and corridor seed 4 give the same
  positions and checksum on every tick, with the cache on and off. A deliberately broken cache, keeping the walker's
  own interval, fails the test.
- The development corpus (`runs/p8-1`): all 11 CSVs are byte-identical to step 1's (`runs/p17-1`).

**Cost** (release benchmark, 400 units, 300 ticks, `perf stat -e instructions:u`, under the gate lock, Legacy on the
same tree).
- Ticks per second come from a second measurement, taken while the shared machine's load was 23 and then 15. The
  first measurement, at load 240, read 46, 84 and 52 for the Solver, so wall-clock figures move with other agents'
  load. Instructions repeat to within a few thousand.

| Scenario | Legacy | Step 1 | Step 1 + P8 | Q0093 (c): ≤ +60 % and ≥ 50 ticks per second |
|---|---:|---:|---:|---|
| 64 × 64, armies meet | 10.35 G, 164 tps | 17.06 G (+64.8 %) | **16.69 G (+61.2 %)**, 104 tps | **fails** on instructions, by 1.2 points; holds on speed |
| 128 × 128, armies apart | 10.74 G, 161 tps | 11.49 G (+7.0 %) | 11.49 G (+7.0 %), 154 tps | holds |
| showcase canyon | 18.03 G, 123 tps | 27.75 G (+53.9 %) | 27.75 G (+53.9 %), 80 tps | holds |

**Split on 64 × 64.** `perf record --call-graph=dwarf`, inclusive cycles, each share scaled by the run's
instructions retired.

| Part | Legacy | Solver + P8 | Change |
|---|---:|---:|---:|
| Contact (`resolve_collisions_and_push`; the solver is 20.7 % of the run) | 0.20 G | 3.60 G | +3.40 G |
| Target acquisition | 4.73 G | 6.61 G | +1.88 G |
| Movement phase | 3.27 G | 4.56 G | +1.29 G |
| — of which intent | — | 1.08 G | |
| — of which the ring scan, cached (≈ 0.95 G before P8) | — | 0.52 G | |
| Total | 10.35 G | 16.69 G | +6.34 G |

**Reading.**
- P8 saves 0.37 G on the 64 × 64 battle and changes nothing.
- The battle stays 1.2 points above Q0093 (c)'s +60 %.
- What remains is the contact solver (+3.4 G), which Amendment 11 did not touch (no over-relaxation), and target
  acquisition (+1.9 G), a behaviour effect of denser packing.
- No further change is made (Q0097).

### 13.8 Ablation A0 (Amendment 14): intent is the preferred velocity: a measurement, stopped by the step rule

**Build.**
- A scratch build on `contact-solver-a0-wip` (`b714b294`): under the solver pipeline, `proposed_heading` returns
  the straight heading to the next point, with no side rule and no side memory. Nothing is committed to the pipeline.
- **Baseline:** step 1 + P8 (`runs/p8-1`). The plan head reproduces its first 20 seeds per family byte for byte.

**Development (seeds 1 to 1,000), against step 1 + P8 and against the thresholds.**

| Family | Figure | Step 1 + P8 | A0 | Prediction | Step rule |
|---|---|---:|---:|---|---|
| arc | M1 / M1-hi (T1) | 2.48 / 0.192 | **0.06 / 0.010** | falls: **yes** | T1 now holds |
| arc | give-ups (E1) | 305 | 61 | | better, still fails |
| arc | fill / 5th (T5) | 0.967 / 0.857 | 0.984 / 0.889 | | holds, better |
| arc | friends > 5 % moving / > 1 % standing | 0.009 % / 0.011 % | 0.027 % / 0.029 % | | worse beyond noise, within T7 |
| arc | max / T7 | 0.300, fails | 0.149, **holds** | | |
| arc | D-S | 102,537 | 67,398 | | |
| gap | M1 / M1-hi (T1) | 1.11 / 0.283 | **0.51 / 0.247** | falls: **yes** | T1 now holds |
| gap | give-ups (T4, WP3 fails) | 46 | **124** | | **stop**: worse |
| gap | scenes over the friend max (T7, WP3 fails) | 242 | **277** | | **stop**: worse |
| gap | η 5th | 0.381 | 0.359 | | worse, within T6 |
| cross | M1 / M1-hi (T1, WP3 fails) | 1.19 / 0.49 | **25.61 / 25.42** | | **stop**: worse |
| cross | M3 (T2, passes) | 0.000 | **0.692** | | **stop**: now fails |
| cross | M4 (T3, passes) | 0.50 % | **1.58 %** | rises: **yes** | **stop**: now fails |
| cross | give-ups (T4) | 2 | **1,274** | rises: **yes** | **stop**: worse |
| cross | friends > 5 % moving; max (T7, passes) | 0.001 %; 0.098 | **6.33 %; 0.990** | rises: **yes** | **stop**: now fails |
| corridor | give-ups (E1) | 20 | **0** | | holds |
| corridor | D-S | 2,502 | 29,128 | | |
| corner | M1 (T1) | 0.50 | 0.14 | | holds |
| corner | friend max (T7) | 0.520 | 0.991 | | fails both |
| all | T8 | | | | holds everywhere |
| mirror | blue share (T9) | 0.554 [0.453, 0.652] | 0.589 [0.474, 0.695] | | holds |

**Verdict.**
- Every prediction is borne out: arc and gap T1 fall; cross T4, M4 and T7 rise.
- But A0 **fails the step rule** on:
  - cross T1 to T4 and T7;
  - gap T4 and T7.
- It is not a candidate on its own.

**Cost** (release benchmark, 400 units, 300 ticks, instructions retired, the same tree for all three).
- Ticks per second are not reliable here: the shared machine's load average was 42 to 70.

| Scenario | Legacy | Step 1 + P8 | A0 | A0 against Legacy |
|---|---:|---:|---:|---|
| 64 × 64, armies meet | 10.35 G | 16.69 G (+61.2 %) | **16.00 G** | +54.6 % (within +60 %) |
| 128 × 128, armies apart | 10.74 G | 11.49 G (+7.0 %) | 10.90 G | +1.5 % |
| showcase canyon | 18.03 G | 27.75 G (+53.9 %) | 26.60 G | +47.5 % |

The prediction for 64 × 64 was about −1 G; the measured change is −0.69 G.

**Split on 64 × 64** (A0, inclusive cycles scaled by instructions).

| Part | Step 1 + P8 | A0 |
|---|---:|---:|
| Contact | 3.60 G | 3.99 G |
| Target acquisition | 6.61 G | 6.36 G |
| Movement phase | 4.56 G | 3.66 G (no intent) |
| Ring scan (cached) | 0.52 G | 0.60 G |

**Traces** (`trace/a0/`).
- **Cross, seed 967** (θ 180°, n 12; give-ups 0 → 24, M1-hi 2,064 reversals). All 1,274 cross give-ups are in the
  180° scenes.
  - The two columns meet head-on. Bodies on the line of centres have no sideways component, so contact cannot pass
    them, and the walkers give up with `NoProgress` at tick 341.
  - The idle units they leave are deeply overlapped (gaps −0.58 to −1.17).
  - With a zero proposal, the solver moves them ±0.41 per tick, back and forth, from tick 350 to 607: the solver's
    correction of coincident idle pairs alternates sign every tick.
- **Gap, seed 463** (w 2, r 0.75, n 24; give-ups 0 → 10): the same mechanism. Walkers queue in the narrow gap and
  give up with `NoProgress`. Their idle bodies, deeply overlapped (−0.19 to −1.05), are moved ±0.45 per tick.

**Reading.**
- Without a side rule the ring families improve on every figure but D-S. The side rule costs arc and gap their T1.
- Some anticipation is needed for head-on crossings, as the review expected. Without it, collinear pairs stall,
  give up and are left deeply overlapped.
- A second mechanism shows up here: a limit cycle of the solver for deeply overlapped idle pairs (±0.4 a tick).
- Rule S waits for Q0097.
