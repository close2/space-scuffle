# Q0102: The movement solver is 2.6 points over the cost bar on one scenario. Accept it, or change behaviour to save cost?

- **Asked:** 2026-11-23
- **Blocking:** no.
- **Default until answered:** (a). WP6 (the held-out run and the switch-over package) is prepared on the
  `contact-solver-plan` branch. It is **not merged into `main`** until you answer.

## Background

Under option (b) of [Q0097](../closed/0097-movement-solver-next-step.md), the solver now consists of step 1, the ring
cache, the overlap fix O1 and the nudge D-δ (plan `docs/plans/2026-11-20-contact-solver.md`, Amendment 16). Measured
against the bar you set in Q0093 (c), which allows at most +60 % instructions and requires at least 50 ticks per
second:

| Scenario | Instructions against Legacy | Ticks per second (quiet machine) | Bar |
|---|---:|---:|---|
| 400 units, 64 × 64, armies meet | **+62.6 %** | about 100 | fails by 2.6 points on instructions; holds on speed |
| 128 × 128, armies apart | +7.2 % | about 150 | holds |
| Showcase canyon | +54.7 % | about 80 | holds |

Where the extra 6.5 G instructions on 64 × 64 go:
- the contact solver itself: +3.7 G;
- target acquisition: +1.4 G, because units pack more densely;
- movement intent and the ring scan: +1.1 G.

The only change that keeps results byte-identical, integer square roots in intent, saves 0.03 %. Making work that
both movement systems share faster would lower Legacy as well, so it would raise the ratio. Getting under +60 %
means cutting about 0.27 G from solver-only work, and that requires changing behaviour.

Separately: the nudge you sanctioned in Q0095 applies to **melee** attackers, as you worded it. The corridor scenes
in the test harness use ranged attackers, so the two-abreast miss there is unchanged at 301 of 301.

## Options

- **(a) Accept +62.6 % on the 64 × 64 scenario.** Every scenario stays at or above 2× real time, so the speed half
  of your bar holds. Proceed: one held-out run, then the switch-over, then deletion of the old movement code (whose
  own cost then disappears from the codebase). This is the default.
- **(b) A cost step that changes behaviour, judged by the usual step rule.** For example, fewer solver iterations
  or over-relaxation (Macklin et al. 2014). The iteration count is derived from the overlap tolerance, so this means
  re-deriving it, and some figures would move.
- **(c) Keep the bar strict and keep Legacy** until a later attempt (all solver branches are kept on GitHub).

## Update, 2026-11-24

The gap is now larger, though the solver has not changed. Main's turret wind-up merge made the old movement
code cheaper (10.35 G to 9.47 G instructions on 64 × 64), so the same solver now measures **+66.8 %**
(15.79 G). Speed still holds on every scenario:

| Scenario | Instructions against Legacy | Ticks per second |
|---|---:|---:|
| 400 units, 64 × 64, armies meet | +66.8 % | 111 |
| 128 × 128, armies apart | +7.6 % | 183 |
| Showcase canyon | +57.6 % | 80 |

The held-out run (seeds never used before) confirmed the development table: the solver is worse than the old
code on the same figures in both seed ranges, and on no others. Those figures are now open bugs B-188 to B-191.
Two battle claims break under the solver; they are asked about separately in
[Q0103](0103-solver-matrix-claims.md). The merge waits for both answers.

## Answer

- **Option (a): Accept `+66.8 %` instructions on the 400-unit 64×64 scenario.**

**Reasoning:** The ratio increase from `+62.6 %` to `+66.8 %` is an artifact of Legacy getting faster from the turret wind-up merge (`10.35 G → 9.47 G`), while actual speed passes Q0093 (c)'s 50 TPS bar with wide headroom on every scenario (`111` TPS = 4.4× real time on 64×64, `183` TPS on 128×128, `80` TPS on Showcase canyon). Do not compromise overlap tolerances over the relative instruction ratio; merge remains gated only on Q0103.

## Resolution

Accepted (a): the instruction bar no longer blocks the switch-over. The solver's WP6 (plan
`2026-11-20-contact-solver`, Amendments 17 and 18, branch `contact-solver-plan`) now waits only on
[Q0103](../open/0103-solver-matrix-claims.md). No overlap tolerance or iteration count is changed for cost.
