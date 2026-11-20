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
