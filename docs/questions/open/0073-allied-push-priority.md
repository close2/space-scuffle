# Q0073: How does the game's `AlliedPushPriority` combine with `PushPriority`?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** our one `push_priority` field takes the game's `PushPriority`, 10 for every shipped
  archetype. The Ultralisk analogue's 5.0.14 change ("pushes allied units out of the way when moving") is modelled as
  priority 11, the smallest value that makes a moving unit push an idle friend of priority 10 under our push table
  (plan [`2026-11-09-friendly-blocking`](../../plans/2026-11-09-friendly-blocking.md) D-4 and D-5, WP5).

## Question

**Background.** StarCraft II's unit data has two push fields. `PushPriority` defaults to 10 on `CUnit` (`core`
UnitData:59) and is overridden only for critters (5) and one campaign unit (11). `AlliedPushPriority` appears only
on the Ultralisk, set to 1 by patch 5.0.14 (`voidmulti` UnitData:1943), whose notes say: "Increased allied push
priority from 0 to 1 (now pushes allied units out of the way when moving)". Patch 5.0.15, after our patch of record,
raised it for Thors and Siege Tanks too. Neither Blizzard nor the editor documentation says how the engine combines
the two values.

Our simulation has one field per archetype, `push_priority`. A moving unit pushes an idle friend of no higher
priority sideways out of its lane, and a firing or holding unit is never pushed (plan D-4).

**Options.**

1. *(default)* `push_priority` is `PushPriority` (10 everywhere); a unit with a non-zero `AlliedPushPriority` gets
   `PushPriority + AlliedPushPriority` (the Ultralisk analogue 11). Simple, and only the Ultralisk analogue differs.
2. Add a second field, `allied_push_priority`, compared first between friends, with `push_priority` as the tie-break
   (closer to the data's shape, if the engine works that way). Needs a schema change and a rule commit.
3. Something else the owner knows of.

**Recommendation.** Option 1 until a source says otherwise; it changes one number for one archetype.

## Answer

