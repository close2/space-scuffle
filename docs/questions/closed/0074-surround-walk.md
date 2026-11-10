# Q0074: May a blocked second-line attacker walk around a firing line it cannot pass?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** yes, as an "allows more" deviation (Q0051). A blocked attacker, chasing a target or
  attack-moving without one, walks along the crowd on a side it keeps until it has a clear way in, then closes to
  range. The behaviour is a per-archetype catalog flag, `surround_walk: bool`, defaulting to `true`. Whatever the
  answer, a unit that has no room never vibrates in place: it stands still and waits (plan
  [`2026-11-09-friendly-blocking`](../../plans/2026-11-09-friendly-blocking.md) D-1 and D-2, WP3).

## Question

**Background.** The owner reported on 2026-11-09 that in the showcase video "some units only vibrate when reaching
the target but don't come close enough to actually fire (blocked by friendly units)". The plan measured it: of each
side's 10 attackers, 4 stand one body behind the firing line and flip direction every tick until they give up.

What StarCraft II does with a unit stuck behind its own firing line is not fully documented:

- Blizzard's guide [Unit Positioning](https://news.blizzard.com/en-us/article/6640646/game-guide-unit-positioning)
  calls the concave "a curved position which allows all your units to attack", and tells the player to "move them
  slightly forward before attack-moving once again" for units left outside it. So the game does not promise that
  every unit reaches range by itself.
- The game's avoidance mover slides units around stationary friends, so units in a free arc do reach range by
  walking around; a unit with no room stands (community sources, plan §3).
- No source has a unit oscillate in place: that part is a defect of ours, fixed either way.

**Options.**

1. *(default)* Walk around: a blocked attacker walks along the crowd on one side it keeps until the way in is
   clear. More units fire than the game would guarantee, which is an "allows more" deviation; an archetype can opt
   out with `surround_walk: false`.
2. Stand: the default becomes `false`, and a blocked attacker waits still behind the line as the game's guide
   implies, until the player moves it. The field stays, so an archetype can opt in.

**Recommendation.** Option 1, because the owner's report asks for units to reach range, and the flag keeps option 2
one data change away.

## Answer

**Option 1 (Walk around — `surround_walk: true` by default).**

**Engine insight:** In real StarCraft II gameplay, SC2's avoidance steering (`CMoverAvoidance`) naturally slides attack-moving second-row units laterally along the back of stationary front-line friendly units until they reach an open arc in weapon range, automatically forming a concave (players only need to manually stutter-step in narrow chokes or with very large armies). Keeping `surround_walk: true` by default matches how SC2 armies flow into a concave, eliminates the 1-tick direction oscillation, and ensures completely boxed-in units stand still cleanly.

## Resolution

2026-11-10. Option 1: no change. `surround_walk` already defaults to `true` per archetype, and a
unit with no room already stands still (plan `2026-11-09-friendly-blocking` WP3). Plan
`2026-11-05-sc2-deviation-fixes` Amendment 24, item 5.
