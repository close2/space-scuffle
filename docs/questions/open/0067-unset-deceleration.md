# Q0067: How fast does a unit with no `Deceleration` in its data brake?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** no change. A unit whose StarCraft II data sets no `Deceleration` keeps stopping within
  one tick: its catalog `deceleration` stays a copy of its `max_speed`, which the locomotion rule reads as instant.
  Only the two analogues whose data sets the field brake as the data says (High Templar `1000`, instant; Overlord
  `1.625` Normal per second squared). The difference stays recorded under bug B-39 in `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP12 (bug
B-39) adds acceleration and deceleration to locomotion. Every shipped analogue's `Acceleration` is in the data
(SC2GameData `5444e12f13`, chain `core → liberty → swarm → void → voidmulti`): 1000 (instant) for ground units, and
finite values for workers and air units, for example Mutalisk 3.5 (`void` UnitData:3092), Medivac 2.25 (`swarm`
UnitData:1706), Carrier 1.0625 (`liberty` UnitData:3483). `Deceleration`, however, is set by almost no unit: no
`CUnit` default sets it, and among the shipped analogues only the High Templar (1000, `liberty` UnitData:3259) and
the Overlord (1.625, `voidmulti` UnitData:1199) do. Elsewhere only the Mothership (1, `liberty` UnitData:4260), the
Tempest (3, `voidmulti` UnitData:1714), the Guardian (1, `void` UnitData:2411) and the overlord transport form set
it. Evidence [`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §13.

The field's meaning when unset (engine value 0) is engine behaviour, not data. Neither the SC2Mapster editor
references nor Liquipedia state it; a forum tutorial claims a unit with zero deceleration circles its destination,
which cannot describe the shipped ground units (acceleration 1000, deceleration unset), which stop at once. Two
readings fit the data:

1. **Unset means instant**: an air unit such as a Mutalisk or Medivac stops within a game loop on arrival, and the
   explicit values on the Overlord, Mothership, Tempest and Guardian make those slow units drift to a stop.
2. **Unset means "use `Acceleration`"**: a Mutalisk brakes at 3.5, a Medivac at 2.25, a Carrier at 1.0625; the
   explicit values make the Overlord brake faster than it accelerates and the Tempest and Mothership differ.

**Why it matters.** Under reading 2, every air unit and worker in the matrix brakes over about as many ticks as it
takes to accelerate (a Medivac over about 20 ticks, a Carrier about 31): their arrival on a move order is later, and
their speed at arrival lower. Under reading 1 (today's catalog), nothing changes.

**Options.**

1. **No change (default).** Unset reads as instant; the catalog copies `max_speed`.
2. **Unset reads as `Acceleration`.** Copy each analogue's converted acceleration into `deceleration`, and extend
   braking (today only into a move order's destination) to orders that end without arriving and to pursuit.
3. **Measured.** An editor test: order a Medivac to a point and log its speed each game loop as it arrives.

**Recommendation.** Option 3; the change is data only once measured.

## Answer
