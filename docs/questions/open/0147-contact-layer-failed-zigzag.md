# Q0147: The crowd-contact rules failed their first check on their own (units reverse their step more often). Keep today's movement?

- **Asked:** 2026-11-27 (plan `2026-12-02-contact-layer-acceptance` §7 rule 3; drafted there as QC-1)
- **Blocking:** no. Other work continues; only switching the contact layer on waits.
- **Default until answered:** (a). Today's movement (`Solver`) stays shipped; the contact layer stays merged but
  switched off on `main`.

## Background

You chose Q0143 (c): run a full acceptance pass for the redesign's contact layer alone, without the gated intent. The
pass (evidence `docs/evidence/2026-12-02-contact-layer-acceptance.md`) stopped at its first, pre-registered check,
the zig-zag measure (how often a unit reverses its own step, per 1,000 moving unit-ticks), measured on today's `main`
(`edbac56b`, after the wall-clearance fix):

| Where | Today | Contact layer | Today's upper bound |
|---|---|---|---|
| Gap family | 1.12 | 2.21 | 1.25 |
| Lanes family | 1.10 | 2.83 | 1.17 |
| Stutter family | 50.07 | 57.39 | 50.98 |
| Battle 26 (zerglings vs colossi), zerglings | 5.46 | 8.67 | 6.36 |
| Battle 29, roaches' side | 0.91 | 1.91 | 1.36 |

28 figures exceed today's bound (gap, corner, enemy press, stutter, lanes, a marginal corridor case, and battles
26–29). It holds or improves in nine families and both 400-unit clashes; in the deep queue it is far better (68.8 →
1.0).

The cause differs from the gated intent's failure (Q0143): the units' own steps do not reverse more often; the extra
reversals come from the contact layer's corrections pushing them back (battle 26: 3,820 → 14,426 correction-caused
reversals).

The plan's one allowed amendment (A-1, switching off the older crowd fix C-3′ underneath) does not apply, because
most failing figures do not fail with C-3′ alone. By the rules agreed in advance, the pass stops here: your hard
rules (Hold Position never moves, attackers pushed only within weapon range, no enemy pushing), overlap and cost were
**not judged** and the acceptance and held-out seeds stay unused for any later attempt.

## Options

- **(a) Keep today's movement (default).** B-193, B-219, B-220 and B-225 stay open; movement work waiting on this
  (wall clearance WP2/WP3, B-209 colossus walk-under, B-229, B-247) is re-planned against today's movement.
- **(b) Accept more zig-zag in exchange for your pushing rules.** Continue the pass past this check (hard rules,
  overlap, cost, the acceptance and held-out seeds), with the zig-zag figures reported instead of judged, then ship if
  the rest holds. Units would visibly jitter more in tight crowds (about 1.5–2.5× today's reversals there).
- **(c) A new design round** for the correction step itself (damping the push-back so it does not reverse a unit's
  step), with its own plan and acceptance; nothing ships until it passes.
- **(d) Something else** you would like tried.

Recommendation: (c) if your pushing rules matter more than the schedule; otherwise (a). (b) trades a visible
movement defect for rule compliance, which AGENTS.md §8 ranks below natural movement.

## Answer

- **Option (d), with a hard fallback to Option (a):** Evaluate **WP1 + WP2 (`gated-wp2`)** together through the remaining acceptance checks (hard pushing rules, crowd overlap, cost `≥ 50` TPS on every benchmark, and acceptance/held-out seeds), allowing a small practical zig-zag tolerance on the five tiny near-misses from Q0143 (e.g. Battle 26 `0.88` vs `0.64` and Battle 29 `0.34` vs `0.26` / `1.43` vs `1.36` reversals per 1,000 moving ticks — ~10× cleaner than WP1 alone and visually negligible).
- **Stop rule:** If WP1 + WP2 does **not** work out cleanly on those remaining checks, **stop here and keep today's movement (Option (a))** — the current algorithm is good enough — and immediately re-plan the waiting movement work (wall clearance WP2/WP3, Colossus walk-under `B-209`, Interceptor strafing `B-229`, `B-247`) against today's movement with no further solver redesign rounds.
