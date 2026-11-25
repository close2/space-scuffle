# Q0113: Enemies that block instead of being shoved: accept the crowd side effects, or keep it parked?

- **Asked:** 2026-11-25
- **Blocking:** only the merge of "enemies block" (plan `2026-11-25-enemy-contact-and-ring-retarget`, package P2, parked
  on branch `enemy-contact-plan`) and, after it, the zergling retarget fix for battle 28 (B-192). Other work continues.
- **Default until answered:** (b), keep it parked. Main keeps today's movement, where a crowd can shove the enemies it
  fights (B-193).

## Background

In StarCraft II enemy units never push each other: a unit walking into an enemy stops or slides along it (patch 1.2.0
fixed the opposite; high confidence). Our movement solver splits every overlap between enemies half and half, so a
crowd of zerglings shoves the colossi, immortals or stalkers it fights (bug B-193). Two attempts have now been made.

The second attempt's rule, "enemies block", does what the game does: every step a unit takes stops at its share of
the gap to an enemy, and it slides along the enemy instead. A second rule makes a friend held at an enemy hand the push
it cannot take on to the friend behind it. Measured on the 24 runs of the battle test (evidence file on the branch, §7):

| | Today | Enemies block |
|---|---|---|
| Largest overlap between enemies | up to 2.8 % | 0.00 % everywhere |
| Shove of the pressed side (colossi / immortals / stalkers) | 4.38 / 2.42 / 3.32 per run | 0 / 0 / 0 |
| Battle 26, zerglings vs colossi (zergling wins) | 19 of 24 | 2 of 24 |
| Battle 28, zerglings vs immortals (zergling wins) | 7 of 24 | 1 of 24 |
| Instructions on the 64×64 benchmark, against the old movement code | about +66.7 % | +71.3 % (all scenarios still ≥ 70 ticks/s) |

Both stopped because pre-registered checks failed. Most misses are by a hair (for example a shove of 0.091 against a
bound of 0.087). Two are clearer:
- **Battle 27 (ultralisks vs zerglings):** zerglings stand still while an enemy still has free room about 37 % more
  often than today (1,468 against at most 1,067 per run).
- **Battle 26:** the worst overlap between two friendly zerglings grows from 37 % to 40 %.

Battles 26 and 28 swing towards the ranged side because zerglings can no longer push their way in; in battle 26 that
is partly our own difference from the game, since in StarCraft II zerglings walk *under* a colossus (B-209, registered,
not fixed).

The defect the work found in today's solver (B-208, out-of-date corrections in its last pass) is being landed on its
own; it lowers today's enemy overlap and moves no battle result.

## Options

- **(a) Accept "enemies block" as it is.** Merge it with battles 26 and 28 restated (26 marked with B-209), the near
  misses recorded, battle 27's extra standing registered as a bug to fix next, and the cost (+71.3 % on 64×64, above
  the +70 % reporting line of Q0111) accepted.
- **(b) Keep it parked (default).** Main keeps today's shoving. A third attempt would first fix the friend chains and
  battle 27's standing, and could start with B-209 (zerglings under colossi) so battle 26 measures what it should.
- **(c) Drop it.** Record the shove as a sanctioned deviation (crowds may push enemies a little) and close B-193.

Recommendation: (b) now, then a third attempt starting with B-209; (a) if you prefer the game's rule today over the
crowd side effects.

## Answer
