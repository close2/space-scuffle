# Blueprint: One Contact Solver, Intent Steering and the Concave as a Goal (Q0092)

**Status of this document.** Draft, pre-registration commit. This commit fixes the evaluation harness (§6) and its
thresholds **before any measurement is taken**, so that the baseline and prototype figures added in the next commit
cannot have shaped them. The rest of the plan follows in that commit.

## 6. The evaluation harness (WP1, no behaviour change)

### 6.1 Seed ranges

| Range | Seeds per family | Use |
|---|---|---|
| Development | 1 to 1,000 | Every measurement during development, the baseline of this plan, every prototype |
| Acceptance | 100,001 to 101,000 | Once per work package that changes behaviour, at its acceptance; never for tuning |
| Held-out | 9,000,001 to 9,001,000 | Once, at the switch-over work package, for both pipelines side by side; nobody runs it before |

Mirror matches use the same three ranges (1,000 seeds each, at least the 500 the survey asks for).

### 6.2 Scenario families with known answers

Every scenario is generated from `(family, seed)` by a SplitMix64 stream; every coordinate and size lies on a 1/64
lattice, so the conversion to `SimScalar` is exact and the corpus is identical on every platform. Units are scratch
archetypes generated per scenario (charter §3.3: no shipped identity is named); radius `r` is drawn from
{0.375, 0.5, 0.5625, 0.625, 0.75, 0.875, 1.0}, speed `v` from {0.1, 0.125, 0.15625, 0.1875} world units per tick.

| Family | Scene | Known answer |
|---|---|---|
| **gap** | A one-cell wall across a 48 × 48 map with a gap of `w` ∈ {1, …, 6} cells, `w / 2r` in [0.7, 4.5] and never within 6 % of 1 (the predicate's rounding band); 6 to 24 units in a block, a quarter of the scenarios mixed with a second archetype of 0.75 `r` and push priority 20; one `Move` to a point beyond the wall | Passable iff `w > 2r`. Passable: every unit is past the wall with its order complete by the bound `2 · (50 / v + n · 2r / (lanes · v)) + 100` ticks, `lanes = ⌊(w − 2r) / 2r⌋ + 1`. Impassable: nobody passes |
| **arc** | An immobile enemy pillar of radius `P` ∈ {0.5, 1, 1.5, 2, 2.5} at the centre; 4 to 30 attackers with range `R` ∈ {0.1, 0.5, 1, 2, 4}, 10 to 16 units away; one `Attack` on the pillar | Ring capacity at the band's outer edge, `C = ⌊π / asin(r / (P + r + R))⌋`, is a lower bound on how many fit in range; `min(k, C)` attackers reach range by the bound `2 · d / v + 500` ticks; the others stand |
| **cross** | Two friendly groups of 4 to 14, each unit with its own `Move` through the centre, crossing at 90°, 135° or 180° | Every unit completes its order by the bound `2 · (24 + 2n · spacing) / v + 100` ticks |
| **corridor** | A corridor of `w` ∈ {2, 3, 4} cells, no way round, plugged by an enemy pillar; 3 to 11 attackers with range 0.25 to 1 attack it | `min(k, ⌊w / (2r + 1/16)⌋)` attackers reach range; the rest queue, keep the order and stand |
| **mirror** | Random compositions of one or two shipped ground armies (supply 1 to 4 each), spawned on a point mirror of a 64 × 64 map; both attack-move, and every 100 ticks re-attack-move to the other side's centroid; random weapon delays removed; 3,000-tick limit | Blue (spawned first, lower entity IDs) wins half the decisive games |

Every scenario family except mirror is also run **relabelled**: the same scene with the spawn order shuffled by a
seeded permutation, which permutes the entity IDs and nothing else.

### 6.3 Metrics

All metrics are measured outside the simulation, from positions after each tick, in `f64`. A unit's step is its
achieved displacement over one tick.

| ID | Metric | Definition |
|---|---|---|
| M1 | Visible reversals | Two consecutive steps both at least `v / 4` long with cosine below −0.5; per 1,000 moving unit-ticks (a moving tick: step above `v / 8`). M1-hi counts those with both steps at least `v / 2` |
| M2 | Wasted path | Over each one-second window: path length minus net displacement; summed and divided by total path length |
| M3 | Standing waste | Over the last 250 ticks of each scenario: path length minus net displacement per unit, world units. Units that should stand (no room, or arrived) should have none |
| M4 | Progress | Potential `Φ`: geodesic distance to the goal (gap: through the gap; cross: straight), or distance to the weapon band (arc, corridor). A violation is a one-second window, under an unchanged order, in which `Φ` rises by more than the unit's radius; share of such windows |
| M5 | Completion and deadlock | Units that complete (passable gap, cross) or reach range (arc, corridor) by the bound, against the known answer; a **deadlock** is a unit with room that has not completed by the bound and has not moved for the last 50 ticks; give-ups (`NoProgress`, `PursuitAbandoned`) |
| M6 | Arc fill | In range at the end over `min(k, C)` |
| M7 | Chokepoint throughput | Passable gaps: units per second through the gap between the first and the last unit through, divided by the capacity model `lanes · 25v / 2r` (η); and its dependence on `w` at fixed `r` |
| M8 | Overlap | Friend overlap `(r_a + r_b − d) / (r_a + r_b)`: maximum, and the share of near pair-ticks above 5 % while either moves and above 1 % while both stand (step below `v / 8`); enemy overlap with the pillar; terrain penetration of the disc into blocked cells and the map edge |
| M9 | Relabelling invariance | Per family, the original and the relabelled corpus: difference of every aggregate in M1 to M8, and a two-sided sign test on the paired per-scenario values of M1 and the mean completion tick |
| M10 | Mirror symmetry | Blue's share of decisive games, with its 95 % Wilson interval; draws reported |

### 6.4 Noise: placebo builds

For the baseline, two placebo builds of today's pipeline each add one raw unit (2⁻³²) to one constant
(`PUSH_RELAXATION`, `STEERING_LOOKAHEAD`), as plan `2026-11-19-crowd-slide` §2.2 did. For the new pipeline, the
placebos add one raw unit to the solver's overlap tolerance and to the side rule's time horizon. **An effect counts
only when it lies outside the range spanned by the placebo builds and outside the 95 % bootstrap interval of the
aggregate** (scenarios resampled with replacement, 2,000 resamples).

### 6.5 Thresholds (pre-registered, binding on the switch-over)

Absolute thresholds hold for the new pipeline on the acceptance range at every behaviour work package and on the
held-out range at the switch-over. Relative thresholds compare it with today's pipeline on the same range.

| ID | Property | Threshold | Why this value |
|---|---|---|---|
| T1 | No oscillation | M1 ≤ 1.0 per 1,000 moving unit-ticks in every family, M1-hi ≤ 0.25; and M1 not above today's in any family (beyond the noise rule) | One visible reversal per 40 unit-seconds of motion is rarer than a viewer can attribute to a unit; half-speed reversals are the ones a viewer sees at once |
| T2 | No wasted motion | M2 not above today's in any family; M3 ≤ 0.5 world units per unit over 10 s | 0.5 over 250 ticks is a jitter of 0.002 per tick, below a pixel at the default camera |
| T3 | Progress | M4 ≤ 1 % of active windows in every family | A one-radius retreat sustained over a second is a visible step back; one window in a hundred allows a push by a higher-priority body |
| T4 | No deadlock | M5: 0 deadlocks and 0 give-ups in the passable gap and cross families; every unit with room completes by the bound in ≥ 99.5 % of scenarios; nobody passes an impassable gap | The bounds are twice a free-flow estimate; with room, a unit that has not arrived by then is stuck |
| T5 | Concave | M6 mean ≥ 0.90 and its 5th percentile ≥ 0.7476 | 0.7476 is Rényi's parking constant, the saturation density of random sequential placement on a line: a ring filled by arrival order alone reaches it, so a slot choice must beat it |
| T6 | Throughput | M7: η median ≥ 0.5 and 5th percentile ≥ 0.25; for each radius, median throughput non-decreasing in `w` (each step within its bootstrap interval) | Half the capacity model is a queue that keeps moving; a collapse at high load would put the 5th percentile near 0 |
| T7 | Overlap (D-α) | Enemy overlap ≤ 0.5 % of the radius sum (rounding); terrain penetration ≤ 2⁻¹⁰ world units; friends: share of moving near pair-ticks above 5 % ≤ 0.1 %, share of standing near pair-ticks above 1 % ≤ 0.1 %, maximum ≤ 15 % | D-α's figures; the maximum caps a transient of three bodies pressed by a heavier one |
| T8 | Relabelling | M9: sign tests p ≥ 0.01 in every family (Bonferroni over the families still gives a family-wise rate below 5 %); aggregate differences within the noise rule | An ID bias shows as a one-sided shift between paired runs |
| T9 | Mirror | M10: the 95 % Wilson interval of blue's share contains 0.5 | The survey's symmetry criterion |

A work package passes when every threshold its scope touches holds; the switch-over passes when all of T1 to T9
hold on the held-out range. Battle winners of named seeds are not a criterion.
