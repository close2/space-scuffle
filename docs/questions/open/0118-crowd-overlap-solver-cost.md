# Q0118: Crowds overlap far more than the 5 % you allowed. Pay for the solver to finish its work, keep it as it is, or change how crowds push?

- **Asked:** 2026-11-26
- **Blocking:** only the switch-over (WP2) of plan `2026-11-26-friend-crowd-overlap`, and through it the resumed
  "enemies block" (Q0113) and collision-classes (Q0117) packages. Measurement and latent code continue.
- **Default until answered:** (b), today's movement stays; the fix is built switched off.

## Background

You allowed friendly units to overlap by up to 5 % of their size while moving (deviation D-α). In big crowds they
overlap much more: a hundred zerglings attacking six colossi reach 39 % (812 pairs above 5 % per run), and the
400-marine benchmark reaches 47 %. The cause is a defect, not a rule: the movement solver stops after 21 rounds per
tick, and a crowd ten units deep needs 50 to 170 rounds for the push from the front to reach the back. Two parked fixes
(enemies that block, zerglings under the colossus) stopped on exactly this.

## Options

- **(a) Let the solver finish (recommended).** It keeps working only while some friendly pair is above 5 % and it is
  still making progress. Battle 26: 812 → 11 pairs above 5 % per run, worst 39 % → 7.6 %; nothing else about
  movement changes, and every situation that is fine today stays bit-for-bit the same. Cost: about +50 % to +60 %
  instructions on the two 400-unit battle benchmarks (64 × 64 and canyon), none elsewhere; projected about 70 and
  53 ticks per second, above your 50 floor, canyon with little margin.
- **(b) Keep it as it is (default).** Crowds keep overlapping; the defect is registered (B-219); enemies-block and
  collision classes stay parked.
- **(c) Change how crowds push (cheap).** A unit standing behind a friend that cannot give way may no longer push
  that friend; it takes the whole correction itself ("shock propagation", standard in physics engines). Overlap
  falls by about 90 % and the benchmark gets 10 % cheaper, but crowds stop pressing their front line aside, which
  looks more like StarCraft II units queueing behind a full surround, and moves results: battle 26's zerglings go
  from winning 9 of 12 runs to 2, and units in one test family turn back and forth more than twice as often.

Recommendation: (a), since the cost stays above 50 ticks per second and nothing but the broken crowds changes; (c)
if you prefer the queueing look and a cheaper solver over the battle claims.

## Answer
