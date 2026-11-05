# Q0050: Should the Siege Tank, Thor and Ultralisk analogues turn before they move, and how far?

- **Asked:** 2026-11-05
- **Blocking:** no
- **Default until answered:** no unit waits for its facing before it moves. A moving unit moves along its path at
  once and its hull follows at its turn rate (plan `2026-11-04-combat-sc2-alignment` D-8 item 4).

## Question

WP17 of the combat alignment plan makes hulls turn at StarCraft II's rates (Q0045 1b). Its research
([`docs/evidence/2026-11-04-catalog-time-audit.md`](../../evidence/2026-11-04-catalog-time-audit.md) §10.2) found
that the game data has a unit flag `TurnBeforeMove`. Among our analogues only three set it:

- the Siege Tank (`railgun_crawler`, `liberty` UnitData:5084),
- the Thor (`colossus_mech`, `liberty` UnitData:5217),
- the Ultralisk (`stag_behemoth`, `liberty` UnitData:8033).

These are also the three slowest turners (20.16° per tick, 9 ticks for a half turn). Every other identity, the
marine and zealot included, leaves the flag off.

The flag is a yes or no. The data do not say how closely the unit must face its path before it starts moving, and
no source found within the research's effort bound says either. The plan allows a turn-before-move rule only if it
can be specified and tested, and otherwise asks here.

**Options:**
- (a) no gating (the default): these three units move at once and turn while moving;
- (b) a data-driven rule, `turn_before_move: bool` per archetype, true for the three: such a unit does not move on a
  tick unless its hull already faces its next step within a tolerance. The tolerance would be a decision, for
  example exactly aligned (the strictest reading of the name) or within the hull's turn rate of one tick;
- (c) as (b), after someone measures the tolerance in the StarCraft II editor or a unit tester.

**Recommendation:** (c) if a measurement is possible; otherwise (b) with "exactly aligned", which our integer
turning reaches without overshoot. Under (a) these three units leave a fight up to 9 ticks sooner than in the game
when they turn around to move.

## Answer
