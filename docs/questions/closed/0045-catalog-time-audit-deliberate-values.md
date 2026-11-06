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

## Resolution

Carried out by plan [`2026-11-04-combat-sc2-alignment`](../../plans/2026-11-04-combat-sc2-alignment.md) (D-7 to D-10),
one line per item:

- **1(b) hull turning:** WP17 commits a to f, merged in `8960a3e`: rates in degrees per tick (a `b672e86`), a hull
  turns at most its rate per tick (b `5802ab5`), matrix sides spawn facing each other (c `cbcabe8`), StarCraft II
  firing arcs (d `35e0e47`, `c67de53`), turret rates W8 (e `6148c4f`) and the 42 hull rates (f `a51dee1`). Commit g,
  turning before a step for the Siege Tank, Thor and Ultralisk analogues, went to
  [Q0050](0050-turn-before-move.md) and was carried out by plan
  [`2026-11-05-turn-before-move`](../../plans/2026-11-05-turn-before-move.md) WP1 (`6d7a9d5`) and WP3 (`b7d9f50`).
  The remaining turning differences are recorded bugs B-28 (six mounts) and B-29 (acquisition arc) in plan
  [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) (WP10); B-31
  (`StationaryTurningRate`) is fixed by plan `2026-11-05-turn-before-move` WP5 (`2522e3f`) and WP6 (`408d3b7`).
- **2(b) worker backswing 9:** WP12 group D2 (`04bd426`), merged in `0f4e7fc`.
- **3(b) Overclock to the Marauder analogue:** WP15 (`2057881`), merged in `6ef8fe9`: the 20-life Overclock left
  `rhino_beetle` for `shock_golem`, 268 ticks.
- **4(b) `flying_saucer` becomes the Phoenix:** WP16 (`7b91323`), merged in `a3cb950`, and audited like the others.

The general direction, "close in on SC2 behavior", was applied through WP12's deferral rule (D-7): a reference value
was applied where the game data settle it, and a value it could not settle was deferred and recorded (Q0039's
resolution names the one left, bug B-4).

Closed.
