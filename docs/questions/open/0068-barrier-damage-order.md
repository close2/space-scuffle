# Q0068: Where in a hit does the Immortal's Barrier act?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** no change to the rule WP13a landed. The hit that raises the barrier is absorbed; the
  barrier absorbs damage before armour, counting that damage against its 100; a hit it absorbs whole deals nothing
  (no 0.5 minimum) and does not restart the regeneration delays. A difference stays recorded under bug B-41 in
  `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP13a (bug
B-41) gives the Immortal analogue (`void_goliath`) StarCraft II's Barrier. The data (SC2GameData `5444e12f13`, chain
`core → liberty → swarm → void → voidmulti`) states the parts: the Immortal carries `BarrierDamageResponse` and
`ImmortalOverload` (`voidmulti` UnitData:775-776). The first answers any damage (`Chance` 1) with a 45 s cooldown
(`voidmulti` BehaviorData:577-590) by applying `TakenDamage` (3 s, :599-609) and `ImmortalBarrierSupressed` (45 s,
:592-598; EffectData:2395-2399, :2408-2410, :1314-1318). `ImmortalOverload` absorbs (`ModifyFraction` 0, `void`
BehaviorData:104) up to `ModifyLimit` 100 (`voidmulti` BehaviorData:278), is enabled only while that cooldown runs
(`BarrierNoGravitonBeamOrBarrierOnCooldown`, `voidmulti` ValidatorData:698-703, :905-908), and is removed when
`TakenDamage` expires or the limit is exhausted (EffectData:2400-2407). Evidence
[`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §14.

The order in which the engine runs these within one hit is engine behaviour, not data:

1. **The raising hit.** `ImmortalOverload` is disabled until `BarrierDamageResponse` (array index 0) starts its
   cooldown. Whether the validator is re-read for the same hit decides whether that first hit is absorbed.
2. **Before or after armour.** Liquipedia's "Damage Calculation" caps damage with Hardened Shield, also a
   `DamageResponse`, before subtracting defence ("Damage Capped", then "Damage Inflicted"); WP13a reads Barrier the
   same way, so the 100 counts damage before armour.
3. **A hit absorbed whole.** `ModifyFraction` 0 leaves 0; Liquipedia's 0.5 minimum ("whichever is greater … or
   0.5") would still let 0.5 through, and `void` sets `ModifyMinimumDamage` 1 (BehaviorData:106), whose meaning is
   not documented. Whether such a hit counts as damage for the shield and life regeneration delays is not stated
   either.

**Why it matters.** Each changes how much a raised barrier saves by a few hit points per volley: against an
armoured Immortal (life armour 1, shield armour 0) reading 2 matters only after the shields are gone, and reading 3
adds 0.5 per absorbed hit.

**Options.**

1. **No change (default).** As above.
2. **Measured.** An editor test: one Marine (6 damage, Immortal shield armour 0) shooting a fresh Immortal, logging
   shields and life each game loop: whether the first shot lands, whether 0.5 gets through each absorbed shot, and
   on which shot the 100 runs out; then the same with a Marauder (20 against Armored) after the shields are gone.

**Recommendation.** Option 2; any change it shows is a few lines in `BarrierState::absorb` and its caller.

## Answer
