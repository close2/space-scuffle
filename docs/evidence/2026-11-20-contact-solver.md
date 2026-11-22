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
| 1. Cost, behaviour-preserving (options 1, 4, 5) | **Done**: rows byte-identical | `62431054` (on `contact-solver-plan`) |
| 2. Pressed-front rule | **Stop**: T5 worse on development (§6.2) | `7d9e51f1`, probe `ad8c7111` (`contact-solver-wp3-wip2-pressed`) |
| 3. Gauss–Seidel | Not started (the stop comes first) | — |
| 4. WP3b, arrival rule | Rule as written fails its own constraint; a variant measured (§6.3) | `a33a697a`, `5d8440fe` (`contact-solver-wp3-wip2`) |
| 5. The two kept tests | Traced and classified (§6.4); one defect fixed | `12333567` (`contact-solver-wp3-wip2`) |

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
