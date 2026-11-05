# Q0051: How closely should the game follow StarCraft II's mechanics?

- **Asked:** 2026-11-05 (given by the owner in chat, without a question being filed)
- **Blocking:** no
- **Default until answered:** not applicable; answered at once.

## Question

Q0008 told agents to follow StarCraft II only where the specification is silent. On 2026-11-05 the owner gave a
stronger standing rule in chat, then made it precise the same morning.

## Answer

Owner (chat, 2026-11-05): "please always use the SC2 mechanics unless I specifically tell to deviate and mention the
deviation. when you see a deviation which has not been explicitly mentioned as deviation, consider it a mistake or
bug"

Owner (chat, 2026-11-05, refining it): "I do NOT want to make an sc2 clone. Deviating to allow more (preferably
configurably) or minor improvements or deviations on mechanics are fine, as long as we are talking about details.
Another example: sc2 has some "bugs" where units can be pushed through walls. We don't need to recreate them (but we
don't care if we have them too). (this is just one example!) However I would like to start with identical unit
mechanics and very close behavior. First because it will show us, where we are missing functionality, and second: it
makes it easier to evaluate the game itself, as we can ask expierenced players to play the game and give us their
opinion (they should feel right at home -- apart of the different looking units and buildings)"

## Resolution

2026-11-05. Standing rule, recorded in charter §8; it supersedes Q0008's narrower wording. In practice:

- **Unit mechanics and values follow StarCraft II 5.0.14 exactly**, and unit behaviour stays very close. A difference
  there that the owner has not named as a deviation is a bug, recorded in plan
  [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) until fixed.
- **Allowed without asking, if recorded:** deviations that allow more than StarCraft II (preferably configurable),
  and minor improvements or differences in details. StarCraft II's own engine bugs (for example units pushed through
  walls) need not be reproduced.
- The owner confirmed the list of sanctioned items in [Q0052](0052-sanctioned-sc2-deviations.md).
