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

## WP1: a swing ends when its target is lost (B-207), stopped for review

WP1 is `3d68031e`. Its worlds (`~/scratch/wind-up-hold/world-wp1`, binaries built from the commit, assets from its
`git archive`) against WP0's: 20 matrix battles move (10, 21, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36a, 36b, 37a,
38a, 38b, 39a, 39b, 40), battles 31 and 33 move, the showcase on the default seed and seeds 1 to 8 and the showcase
effects on the default seed and seeds 2, 3, 4, 5, 7 and 8. The matrix plays 2 misses of record (unchanged) and 27
over all seeds.

**Outcome moves** (winner changes include a win becoming a draw):

| Battle | Close (§6.4) | Seeds whose winner changes | Expectation changes |
|---|---|---|---|
| 21_lurker_vs_6_marines | no | 8, 13 | none |
| 26_100_lings_vs_6_colossi | yes | 4, 6, 8 | 13 now misses |
| 28_100_lings_vs_10_immortals | `known_bug` B-192 | 5, 9, 10, 17 | 5 now misses; 9 and 17 now hold |
| 31_15_marauders_vs_15_stalkers | yes | 4 | none |
| 32_30_marines_vs_15_banelings | no | 3203386110 (of record), 8 | none |
| 33_24_marines_vs_12_zealots | yes | none | 10 now misses |
| 35_12_vikings_vs_6_colossi | yes | 4 | none |
| 38a_fungal_turns_zerglings | yes | none | 4 now misses |

Stop rule 4 fires for battles 21 and 32 (not close, outcome moved) and stop rule 5 for battle 28 (its claim now holds
on seeds 9 and 17 and fails on seed 5). The package stops for review; no close battle is restated, WP3 (goldens,
held-out run, records) is not started.

**First differences** (`firstdiff.py` on per-unit traces of WP0 and WP1, matrix seeds):

| Battle, seed | First differing tick | Units and fields | Class |
|---|---:|---|---|
| 21, 8 and 13 | 34 | the six marines' positions | V: the marines' wind-ups on the Lurker analogue end when it burrows (cloaked, undetected) and they walk on; the attribution build (gone or dead targets only) leaves battle 21 byte-identical |
| 32, 3203386110 | 6 | blue 1 position, phase and target; blue 2 position | H: a marine whose target baneling died is no longer held |
| 32, 8 | 3 | blue 11 and 21 phase and target | N: the next swing starts a tick earlier |
| 28, 5 / 9 / 17 | 82 / 64 / 93 | red immortals' phase and target | N |
| 26, 4 / 6 / 8 / 13 | 16 / 43 / 124 / 87 | a colossus' position (4), else its phase and target | H (4), N |
| 33, 10 | 93 | blue 10 phase and target | N |
| 38a, 4 | 122 | red 19 and 24 phase and target | N |

Every first difference is a swing ended by D-1 (classes H, N, V); no second cause was found (stop rule 2 holds).

**Behaviour checks.** W-1: the probe on WP1's tree counts no stale hold in any world (WP0: 2,446), and standing while
able falls in every family (matrix 9,837 to 8,643, battle 31 38 to 4, battle 33 415 to 0, showcase 14 to 0,
showcase effects 119 to 103, goldens 5 to 0). W-2: matrix stall give-ups 11 to 12, the one more on battle 31 seed 4,
the close seed whose winner flipped (divergence after tick 4's first difference); no battle's
`blocked_attacker_ticks` rises by 5 %. W-3: shots released per 1,000 alive unit-ticks, battle 31 19.12 to 19.10,
battle 33 40.13 to 40.14; on the traced battles 21 (−0.35 %), 26 (−1.12 %), 28 (+2.02 %), 32 (+0.68 %), 33
(−0.10 %), 38a (+0.56 %), aggregate 23.81 to 23.93. W-4: mean end tick falls (matrix sum 321,822 to 320,906;
battle 31 650.3 to 649.3; battle 33 370.0 to 369.1). Battle 26's drop of 1.12 % is above the 1 % trace threshold and
is not yet traced.

## WP2: a holding hull faces its target between shots (B-199), measured while WP1 is under review

WP2 is `3e333d6c`, measured against WP1's worlds. Only matrix battles 38a and 38b move (both close, every seed
differs); battles 31 and 33, both showcase scenarios on every seed and the remaining matrix battles are
byte-identical, as are the held Brood Lord, Roach, Zergling and Void Ray analogue battles (13, 19, 20, 41: their
holders' targets do not change bearing between shots). No winner changes; battle 38a's seed 4, which WP1 made a
miss, holds its claim again. W-2: stall give-ups 12 to 12, `blocked_attacker_ticks` 1,403,193 to 1,402,826. W-3 and
W-4: battles 31 and 33 unchanged; matrix end-tick sum 320,906 to 320,813. W-5 (probe (b), matrix): ticks from the
end of a cooldown to the next swing 381 to 29 over 3,561 cooldowns, bearing error at the end of a cooldown 1,960,582
to 226,533 binary units in sum (mean 3.0° to 0.36°); W-1 standing while able 8,643 to 8,642. **W-5 fails in one
part:** hull turns of a holder on ticks with no available target at either end rise from 24 to 58 (battles 11b,
37a, 37b, 38a, 38b; the first in 11b seed 22 at tick 9, a Marauder analogue). Not yet traced; the probe reads the
target at the ends of ticks, so a target acquired and killed within one tick counts.

## Amendment 1: the three traced items and the restatements

The probe gained two tick-exact hooks (`~/scratch/wind-up-hold/scripts/apply_probe2.py`): the weapon phase records,
per unit, whether its weapon step turned the hull and whether the step's target had passed the availability test at
that moment; the blocking meter prints every stall give-up with its tick, unit, kind and order. Probe copies of WP0,
WP1 and the head (`6f78fc88`) were built in a fresh target directory with fresh file times (`a1.md5` lists three
binaries; an earlier attempt that reused the probe target directory with archived file times rebuilt nothing and
was discarded).

1. **W-2, the extra give-up on battle 31 seed 4, follows the divergence.** The per-unit traces of seed 4 first differ
   on tick 255 (stalker 16 no longer stands for a wind-up whose target died, stalkers 18, 20 and 21 then move
   differently: class H); the armies stay level to tick 400 (8 v 9 on WP0, 8 v 8 on WP1). WP1's one new give-up is
   on tick 409, stalker 21 chasing marauder 2 under an acquired attack (`NoProgress`) in an 8 v 7 fight that exists
   only on WP1's run, 154 ticks after the first difference; WP0's run of the same seed has no give-up and the other
   eleven give-ups of battle 31 are identical in tick, unit and position on both trees. The seed's winner flips
   (WP0 red 4 left at tick 700, WP1 blue 4 left at tick 671). Not a new stall.
2. **W-3, battle 26's rate drop of 1.12 %, is the denominator.** Over its 20 traced seeds the zerglings release
   9,546 → 9,687 strikes in 393,193 → 403,620 alive unit-ticks (24.28 → 24.00 per 1,000) and the colossi 1,353 →
   1,344 in 36,779 → 36,502 (36.79 → 36.82). The zerglings' strike count is set by the colossi's life they must take,
   while more zerglings survive (mean 12.20 → 14.55 left; the colossi die sooner, seeds 4 and 6 flip to the
   zerglings), so their alive unit-ticks grow 2.7 % against 1.5 % more strikes. Per alive colossus the firing rate
   does not drop. No unit fires less often for the change.
3. **W-5, holder hull turns with no available target (24 → 58), are artifacts of the end-of-tick probe.** All 58 on
   the head's matrix are weapon steps that turned the hull towards a target that passed the availability test at
   that moment (`weapon-at-live-target` 58, `weapon-without-target` 0, `not-the-weapon` 0); the target was lost later
   in the same tick (killed in the same combat phase, mostly the abduct and Baneling battles 11b, 37a, 37b, 38a and
   38b). By construction (`face_target_between_shots` needs the target `swing_target_live` passed) the weapon never
   turns a hull towards nothing; nothing to fix.

**Restatements** (charter §8, Amendment 1). No seed of record misses on the final tree; the matrix's misses are
battle 28 (11 and 22 of record, as before, and 1, 2, 3, 4, 5, 6, 8, 10, 13, 14, 15, 16), 26 seed 13, 32 seed 13
(as before), 33 seed 10, 36a seeds 4, 12, 14, 39a seeds 4, 5, 9, 14 and 41 seeds 4 and 7 (all as before). So every
expectation is kept and the fixture comments of battles 21, 32, 26 and 33 record the moved runs with their first
differences (WP1 table above). Battle 28 keeps `known_bug: Some("B-192")`: its claim still fails on both seeds of
record; it now holds on seeds 9 and 17 and fails on 5.
