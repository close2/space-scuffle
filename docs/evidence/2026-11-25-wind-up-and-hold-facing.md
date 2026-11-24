# Evidence: A Swing Ends When Its Target Is Lost, and a Holding Hull Faces Its Target (B-207, B-199)

Measurements of plan [`2026-11-25-wind-up-and-hold-facing`](../plans/2026-11-25-wind-up-and-hold-facing.md): the
baseline, the close-battle list and the probes of WP0 (§6.4, §7.1), the comparisons of WP1 and WP2 against their
predecessors (§6.3, §7.3), the traced moves, the held-out run, throughput and the golden proof (§5, §7.4). Scratch
material (trees, binaries, worlds, the probe patch and its summariser, the red runs) is under
`~/scratch/wind-up-hold/`, never committed.

## WP0: baseline, close battles and probes

### The baseline (§7.2)

WP0's tree is the branch head `a439e23e` (`main` `802df056`, schema 47, merged). Its trace binaries were built with
`cargo build --release --locked -p sim_core --example matchup_trace --example route_trace` and the worlds measured
from a `git archive` copy of the tree with the turret wind-up plan's `measure.sh` (the matrix on the seeds of record
and 16 development seeds, battle 31 on 100 seeds, battle 33 on 64, the showcase and the showcase effects scenario
on the default seed and seeds 1 to 8, 6,000 ticks each):

```bash
~/scratch/wind-up-hold/scripts/measure.sh ~/scratch/wind-up-hold/bin-wp0 ~/scratch/wind-up-hold/tree-wp0 \
    ~/scratch/wind-up-hold/world-wp0
```

The matrix plays 1,220 runs: 2 expectation misses on the seeds of record (battle 28, `known_bug` B-192, seeds 11
and 22) and 25 over all seeds (28: 15; 39a: 4 (seeds 4, 5, 9, 14); 36a: 3 (4, 12, 14); 41: 2 (4, 7); 32: 1
(13)).

### The close-battle list (§6.4), fixed before WP1

A run is won by the side with survivors while the other side has none; otherwise it is a draw. A battle is close
(items 1 and 2) when the side that wins fewer of its baseline runs wins at least 10 % of them, at least one. Derived
from WP0's worlds alone by `python3 -I ~/scratch/wind-up-hold/scripts/close.py ~/scratch/wind-up-hold/world-wp0`:

| Battle | Runs | Blue wins | Red wins | Draws | Minority share | Item |
|---|---:|---:|---:|---:|---:|---|
| 14_thor_vs_ultralisk | 20 | 5 | 15 | 0 | 0.25 | 1 |
| 26_100_lings_vs_6_colossi | 20 | 15 | 5 | 0 | 0.25 | 1, 3 |
| 28_100_lings_vs_10_immortals | 20 | 5 | 15 | 0 | 0.25 | `known_bug` B-192: not close (item 4), reviewed |
| 31_15_marauders_vs_15_stalkers | 20 | 16 | 4 | 0 | 0.20 | 1 |
| 31_15_marauders_vs_15_stalkers, traced | 104 | 69 | 35 | 0 | 0.34 | 2 |
| 33_24_marines_vs_12_zealots | 20 | 20 | 0 | 0 | 0 | 3 |
| 33_24_marines_vs_12_zealots, traced | 68 | 67 | 1 | 0 | 0.01 | 3 (item 2 not met) |
| 32_30_marines_vs_15_banelings | 20 | 17 | 1 | 2 | 0.05 | not close |

Every other matrix battle has one winner on all 20 runs, or one winner and draws (11a: red 9, draws 11; 21: blue 10,
draws 10; 35: blue 1, draws 19; 38b: red 7, draws 13), or draws only (22a, 37a, 37b, 39b, 42); none of these is
close by items 1 and 2.

Item 3: Q0103 and Q0044 name battles 26, 33, 38a and 38b; `grep -n "close\|restated"
crates/sim_core/tests/fixtures/matchup_matrix.ron` finds battle 31's claim ("Equal supply is close"), battle 33's
comment and claim, and battle 35's claim and its restatement under charter §8 ("close-battle variation", plan
`2026-11-20-contact-solver` Amendment 20). Its other hits are the verb (battle 8's claim "closes", the zerglings that
"close around" in battle 10's comment, the roaches that "close" in battle 39a's comment) and name no close battle.

**The list: 14, 26, 31, 33, 35, 38a, 38b.** Battle 28 carries `known_bug` (B-192) and is reviewed under stop rule 5,
not restated. The list is not edited after WP1 starts.

### Probes (§7.1), stored

The scratch probe (`~/scratch/wind-up-hold/scripts/probe.rs`, applied to a copy of the tree by `apply_probe.py`,
logged per world on standard error) counts (a) every hold by `wind_up_holds`, classified by its bound target: live,
gone (no such entity), dead, hidden or unavailable (the combat phase's availability test fails), and after every
tick the unit-ticks where an alive, completed, armed unit has an available enemy in its weapon band, its weapon is
`Ready` with cooldown 0, its hull and turret did not turn and its velocity is zero (W-1); (b) for hull-weapon units
on Hold Position, each cooldown that ends with an available target (the bearing error then, in binary units) and the
ticks until the next swing starts, and hull turns on ticks with no available target at either end (W-5); (d) a
census of holders and immobile units with a hull weapon. A static census of the catalog (a test in the probe copy)
lists every immobile mode and immobile archetype with its weapons' mounts. The probe copy's worlds are byte-identical
to WP0's (only the logs differ).

| Family | Live holds | Gone | Dead | Unavailable | Standing while able (W-1) |
|---|---:|---:|---:|---:|---:|
| Matrix | 222,198 | 1,857 | 12 | 5 | 9,837 |
| Battle 31 | 33,299 | 79 | 0 | 0 | 38 |
| Battle 33 | 23,858 | 415 | 0 | 0 | 415 |
| Showcase (default, 1–8) | 4,650 | 33 | 0 | 9 | 14 |
| Showcase effects | 878 | 28 | 0 | 0 | 119 |
| Goldens | 516 | 8 | 0 | 0 | 5 |

The stale holds of B-207 are the gone, dead and unavailable columns: 2,446 unit-ticks over all worlds. The standing
count includes units that stand for reasons of their own (a sieged tank with an enemy inside its minimum range or
outside its arc, a hull weapon outside an attack order waiting for its arc); it is compared, not judged.

Probe (b), hull-weapon holders, matrix only: 3,564 cooldowns ended with an available target (3,332 Marine analogue,
232 Void Ray analogue), followed by 414 ticks of turn wait in all (Marine 414, Void Ray 0), a mean bearing error at
the end of the cooldown of 597 binary units (3.3°); 24 hull turns on ticks with no available target at either end
(battles 37a, 37b, 38a and 38b). Holders seen (d): the Marine analogue in 11a, 37a, 37b, 38a and 38b, the Marauder,
Zergling, Brood Lord, Roach and Void Ray analogues in 11b, 13, 19, 20 and 41, and the Colossus analogue (turret) in
12c; in the goldens, eight Zealot analogues (hull) hold.

Census (d): no shipped immobile mode or immobile archetype carries a hull weapon. The sieged tank's and the burrowed
Lurker's weapons are turreted, and so are the five armed structures' (their stationary rate is 0). D-4's immobile
case is therefore covered by the constructed tests only.

### Attribution build (§7.1 (c))

The naive alternative (`wind_up_holds` holds nothing for a gone or dead target; nothing else) on WP0's tree moves 18
matrix battles (10, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36a, 36b, 37a, 38a, 38b, 39a, 40), battles 31 and 33,
the showcase on seeds 1 to 8 and the showcase effects on seeds 2, 4, 5, 7 and 8; the default seeds of both showcase
scenarios are byte-identical. Winner changes: 26 (seeds 33, 4, 10, 12), 31 (seed 4), 32 (seed of record
3203386110), 35 (seed 4); expectation changes: 26 seed 13 and 36a seed 2 now miss. Its probe counts no stale hold of
a gone or dead target; the standing count falls (matrix 9,045, battle 31 4, battle 33 0). It is the hold half of
WP1, used below to class each moved world's first difference.
