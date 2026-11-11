# Evidence: spellcaster mechanics (plan `2026-11-10-spellcaster-mechanics`)

Measurements of the plan's data commits (§12), one section per package. Builds in
`~/scratch/cargo-target/spells-storm`; the matrix with `matchup_trace --all-battles --seeds 16 --jobs 8`
(the four seeds of record plus 16 jittered ones, 20 per battle), battle 33 with `--seeds 64`.

## 1. WP2b part 1: hazard zones removed (checksum schema 29)

Base: branch `spells-d` at `5a79962` (WP1a to WP1e with `main` merged, schema 28).

- **Goldens.** Re-recorded for the schema only. A temporary test (not committed) wrote, after every tick of
  both golden scripts, the `Debug` form of every entity, projectile, player's resources, the effect state and
  the render snapshot. Base and new dumps (`golden_replay` 3,821,123 lines, `golden_replay_orders` 3,253,807)
  are byte-identical once the base's lines of the removed `last_storm_damage_tick` (19,051 and 17,293) are
  dropped, except each snapshot's `checksum` field, which is the world checksum of the new schema. Both
  goldens pass in debug, release and `wasm32-wasip1` (wasmtime). The battle sandbox's spawn checksum was
  re-recorded for the same cause.
- **Matrix.** Every battle bit-identical to the base (`--compare` reports only battle 13, which this commit
  takes out of the matrix until the storm data brings it back). Expectation failures: 0 on the seeds of
  record, 3 over all seeds, as on the base (all three predate this plan).
- **Battle 33**, 64 seeds plus the four of record: every per-seed trace file and summary byte-identical to the
  base.
- **Throughput.** Not measured: with no hazard zone open, the removed phase 2 step iterated an empty list and
  its two `Vec::new()` calls allocated nothing, so the removal changes no hot path of the benchmark scenarios.

## 2. WP2b part 2: Psionic Storm as data

The storm is data only: the `cerebral_elder` ability (`units.ron`), the tree in `ability_effects.ron`, the
`psionic_storm` status in `statuses.ron` and the `UnlockAbility` of the `psionic_storm` research
(`upgrades.ron`). Every value is cited in the RON from SC2GameData 5.0.14 (`5444e12f13`), checked against the
local copy (`~/scratch/combat_wp3_data/`):

| Value | Game data | Ours |
|---|---|---|
| Energy | `liberty` AbilData:2067 (75; `swarm` :1110 the same) | 75 |
| Cooldown | `swarm` AbilData:1111, 2 Normal s (over `liberty`'s 2.5) | 36 ticks |
| Range | `liberty` AbilData:2061, 9 from the caster's edge | 9 |
| Backswing | `liberty` AbilData:2062, `CastOutroTime` 0.5 | 9 ticks |
| Persistent | `liberty` EffectData:2069-2076: initial and 6 periodic searches every 0.5712 | period 10.2, count 6 |
| Search | :2077-2084: radius 1.5; filters exclude structures, hidden, invulnerable | radius 1.5, any owner, ground and air, structures excluded |
| Status | `liberty` BehaviorData:709-716: 0.6665 s, period 0.571, initial and periodic damage | 11.901786 / 10.196429 ticks |
| Damage | EffectData:2089-2101: 10 initial with `Acquire` and `Flee`, 10 periodic without | the same |
| Research | `PsiStormTech` (`liberty` UpgradeData:1808) meets `UsePsiStorm` (AbilData:2070) | `UnlockAbility` |

No later layer (`void`, `voidmulti`) overrides any storm entry. No engine change was needed.

- **Tests.** `crates/sim_core/tests/psionic_storm_tests.rs` (14 tests): 80 on ticks 0, 10, 20, 31, 41, 51, 61, 71
  with armour ignored; overlapping storms still 80; a unit that leaves after the first search takes 20; shields
  first; own and air units hit, structures not; cloaked and burrowed units hit without detection; carried units
  untouched, their carrier hit; a barrier absorbs the first 60; held units stay; an idle unit that cannot answer
  flees the centre; an armed idle unit answers the caster; refused without research, under 75 energy and for 36
  ticks of cooldown; range 9 from the caster's edge; the 9-tick lock; the area published from the cast until its
  last search (61 ticks), with its source and radius 1.5. `catalog_reference_values_tests.rs` checks every value
  above as authored, `runtime_catalog_tests.rs` shows an edited `ability_effects.ron` changing a storm without
  recompiling.
- **Battle 13** (rewritten, B-24): the High Templar analogue (start energy 75, researched) storms eight held
  zerglings' centroid on the first tick. All die to the fourth pulse; the battle ends on tick 32 on all 20 matrix
  seeds, blue 1 – red 0.
- **Battles 36a and 36b** (new; the plan's armies, untuned): one High Templar analogue and six zealots, idle,
  against sixteen marines attack-moving into them, in contact from tick 109. With a storm on the marines'
  centroid on tick 150 (36a) the zealots win all 20 seeds, 4 or 5 of blue's 7 left (11 and 9 seeds), ending on
  ticks 248 to 325. Without it (36b) the marines win all 20, 9 to 11 left (1, 8 and 11 seeds), ending on ticks 345
  to 368.
- **Matrix.** Against the base (`5a79962`), `--compare` reports only 13, 36a and 36b; every other battle is
  bit-identical. Expectation failures: 0 on the seeds of record, 3 over all seeds (26 on seeds 10 and 13, 33 on
  seed 9), the same three as on the base.
- **Battle 33**, 64 seeds plus the four of record: every trace file byte-identical to the base.
- **Goldens** unchanged by the data (no caster in either script): they pass in debug, release and
  `wasm32-wasip1` with the schema 29 lists of part 1.

## 3. WP3: Abduct

Abduct is data: the `tongue_chameleon` ability (`units.ron`), four named nodes in `ability_effects.ron` and the
`abducted` status in `statuses.ron`. One generic engine fix came first, in its own commit (plan Amendment 5):
origin placement and landing ignore units, as `PlacementIgnoreBlockers` does. Every value is cited in the RON
from SC2GameData 5.0.14 (`5444e12f13`), checked against the local copy:

| Value | Game data | Ours |
|---|---|---|
| Energy, range | `swarm` AbilData:1230, :1233 (75; 9) | 75; 9 edge to edge |
| Cooldown | none in `swarm` AbilData:1222-1239 | 0 |
| Backswing | `voidmulti` AbilData:1754, `CastOutroTime` 0.8 | 14 ticks |
| Targets | `voidmulti` AbilData:1755; `NoYoink`, `swarm` EffectData:1468 | any but self, ground and air, no structures, not already abducted |
| Landing point | `YoinkStartCreatePlaceholder`, `swarm` EffectData:1447-1471 (offset 0,0.1 :1460; range 2 :1459; ignores units :1453) | origin 0.1 ahead, ring within 2, terrain and structures only |
| Tongue | `YoinkLaunchMissile` :1486-1494; `YoinkMissile` MaxSpeed 18.75, MoverData:147 | homing tether, 1.05 per tick |
| Contact | `YoinkSet` :1498-1509: cancel orders, instant unburrow, `Yoink` buff, delay persistent | `CancelOrders(force_unburrow)`, `ApplyStatus(abducted)`, `abduct_delay` |
| Delay | `YoinkDelayPersistent` :1539-1547, period 0.1 | one pulse of 1.785714 ticks: 2 |
| Flight | `YoinkLaunchTarget` :1554-1565; `YoinkMover` FlightTime 1, MoverData:167 | `Displace` 18 ticks, tether, `on_land` removes the status |
| Status | `Yoink`, `swarm` BehaviorData:1074-1088: stun, suppress cloak, collision, passenger; 2 s | `abducted`: 35.714286 ticks, stun, reveal, no collision, no boarding |

- **Tests.** `crates/sim_core/tests/abduct_tests.rs` (18 tests): contact on cast + ceil(gap / 1.05), the 2-tick
  delay stunned in place, 18 ticks of straight flight, landing on contact + 20 within 0.1 of the caster's cast
  position with the status removed; the point fixed at the cast though the caster flies off or dies; a ground
  unit pulled up a cliff lands on the caster's level; an air unit and a massive unit pulled; own units allowed,
  the caster itself and a structure refused; contact clears orders, hold and target, stuns, lifts collision,
  forbids boarding, and the target does not attack during the pull; orders given meanwhile start after landing
  (SU-3); a sieged tank lands sieged and a siege in progress completes; a detected burrowed unit is unburrowed at
  contact and revealed, an undetected one cannot be targeted; an undetected cloaked enemy refused with no payment,
  allowed once detected; a tongue at a target that boards a transport is dropped and the energy stays spent; no
  landing point (caster over unpathable ground) refuses the cast with no payment (SU-4); the landing found on the
  ring 2 away; a unit under the caster does not move the landing point; the 14-tick lock; range 9 edge to edge; a
  second Abduct on an abducted unit refused; the tether in `RenderSnapshot::links` on every tick from the cast to
  the landing. `effect_tree_tests.rs` gains the engine test of the fix. `catalog_reference_values_tests.rs` checks
  every value as authored; `runtime_catalog_tests.rs` shortens the flight in a copied `ability_effects.ron` (18 to
  6 ticks) without recompiling.
- **Red runs.** The tests' first run with the data failed 9 of 18 on the tests' own setup (a cast before the
  first tick computes vision is refused; a siege takes 72 ticks plus its random part, beyond 80 steps; regeneration
  moves energy); fixed in the tests. Without the engine fix the air-target and blocked-landing tests fail, and so
  does the engine test.
- **Matrix.** Against the WP2b measurement of `4b47bef` (`matrix.csv` of its §2 run), `--compare` reports the
  whole matrix bit-identical; expectation failures 0 on the seeds of record, 3 over all seeds (the same three).
- **Battle 33**, 64 seeds plus the four of record: every trace file both runs wrote is byte-identical to the base.
- **Goldens** unchanged (no caster in either script), passing in debug, release and `wasm32-wasip1`. The DHAT
  allocation test passes.
- **Battles 37a/37b not landed.** Arrangements measured on the 20 matrix seeds, with the fixture extensions of
  Amendment 5: (1) the plan's armies, zerglings and Viper attack-moving together: the Viper sees the tank only
  from about 9.5 away, inside the tanks' range 13 of the zerglings beside it, so the first volley kills four or
  five zerglings before the cast, and the pulled tank lands behind zerglings that ran on; (2) the Viper waiting on
  a flank, the zerglings timed to meet the landing: the pulled tank dies on most seeds, but the second sieged
  tank, within 13 of the landing point, and the marines kill the rest; (3) the tanks 6 apart so the landing lies
  out of the second tank's range: with 8 zerglings red keeps 5 of 6 on 19 seeds (4 on one) with Abduct and all 6
  on every seed without it; with 12 to 20 zerglings red keeps 1 to 5 with Abduct and 2 to 6 without. The claim
  (opposite winners) needs a further arrangement or army size; Abduct's effect, the pulled tank killed, is clear
  in (3).
