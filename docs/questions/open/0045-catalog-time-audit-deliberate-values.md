# Q0045: Should the catalog values that a document chose on purpose follow StarCraft II too?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** no change. The values below stay as they are, and WP12 of the combat alignment plan
  leaves them out. The audit's other corrections (classes B and C) go ahead as planned after the whole-matrix
  measurement.

## Question

The audit you asked for in Q0039 (option 3) compared every time and speed value in the catalog with the StarCraft II
5.0.14 game data. It is in
[`docs/evidence/2026-11-04-catalog-time-audit.md`](../../evidence/2026-11-04-catalog-time-audit.md).

- Most differences were never chosen by anyone, and are corrected under Q0008's standing rule.
- The values below differ **because a project document chose them for a reason of its own**, so they are yours to
  decide.
- Two related identity questions came up as well (items 3 and 4).

**1. Units turn instantly (41 values, class D).**
- **Our rule:** every unit's `turn_rate_deg` is 999, or 360 for heavy units. Doc 07 §2.3 calls 999 "instant
  alignment per tick", and plan 16 §1.2 made the loader read the field in degrees per tick, where 180 or more means
  instant. So every unit faces its target at once.
- **StarCraft II:** most units turn 56° per tick and need 4 ticks for a half turn. Units on the 720 °/s default,
  for example the Immortal, Colossus and Carrier, turn 40° per tick (5 ticks). The Siege Tank, Thor and Ultralisk
  turn 20° per tick (9 ticks).
- **Inconsistency:** doc 07 describes the field in degrees per second, while the code reads degrees per tick.
- **Options:**
  - (a) keep instant turning (default);
  - (b) adopt the reference rates, with the field defined as degrees per tick and the loader's "180 means instant"
    rule kept only for values that ask for it;
  - (c) adopt them only for the slow turners (Siege Tank, Thor, Ultralisk, and the 720 group).
- **Recommendation:** (b) under Q0008. It changes how every unit acquires targets, so it would be measured on the
  whole matrix first, like the other corrections.

**2. The worker's backswing of 6 ticks (`welder_drone`, 1 value, class D).**
- **Our rule:** plan 17 chose it for a "readable windup animation".
- **StarCraft II:** 9 ticks (0.5 Normal s).
- **Options:** (a) keep 6 (default); (b) 9.
- **Recommendation:** (b). After WP1 a backswing no longer delays the next attack, so the change matters only for
  how long the worker stays anchored.

**3. The 20-life Overclock sits on the Roach analogue, not the Marauder analogue.**
- **Today:** doc 08 says Overclock (Stimpack) unlocks "on `pulse_bot` and `shock_golem`". In the catalog the
  20-life Overclock is on `rhino_beetle`, the Roach since plan 21a §2.1, and `shock_golem` has none. The Roach has
  no Stimpack in StarCraft II.
- **Options:**
  - (a) leave it (default);
  - (b) move it to `shock_golem` with the Marauder's duration of 268 ticks (15 Normal s, the same as the marine's);
  - (c) remove it from `rhino_beetle` only.
- **Side effect:** (b) and (c) change `golden_replay.ron`, whose infantry squad uses Stimpack at tick 260, and any
  battle that stims a `rhino_beetle`.
- **Recommendation:** (b).

**4. `flying_saucer` has no clear StarCraft II counterpart.**
- **Today:** its `sc2_analogue` says "Viking / Scout", doc 07 says Phoenix, and its cost (125/75), build time (850
  ticks), weapon (10, +4 light, range 6) and speed (0.15) match neither.
- **What follows:** the audit could not check its values, apart from its shield regeneration, which every Protoss
  unit shares.
- **Options:**
  - (a) leave it as an original unit, outside the audit (default);
  - (b) make it the Phoenix and audit it like the others;
  - (c) name another counterpart.

## Answer

Owner (2026-11-04): For all answers: try to close in on SC2 behavior. We will create our own units later, but for now SC2 behavior is much easier to test and debug. Specifically: 1(b) (adopt SC2 reference turn rates), 2(b) (worker backswing 9 ticks), 3(b) (move 20-life Overclock from `rhino_beetle` to `shock_golem` with 268-tick duration), and 4(b) (align `flying_saucer` to its SC2 counterpart, the Phoenix per doc 07, and audit it like the others).
