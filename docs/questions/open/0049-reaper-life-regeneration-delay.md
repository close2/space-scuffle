# Q0049: Should life regeneration wait after damage, as the Reaper's does in StarCraft II?

- **Asked:** 2026-11-05
- **Blocking:** no
- **Default until answered:** the Reaper analogue (`jet_jumper`) keeps its life regeneration of 0.08 per tick,
  applied always. The audit's correction to 0.112 (group R1) is deferred, as plan
  `2026-11-04-combat-sc2-alignment` D-7 allows for a value whose reference needs a mechanism we do not model.

## Question

The catalog time audit ([`docs/evidence/2026-11-04-catalog-time-audit.md`](../../evidence/2026-11-04-catalog-time-audit.md),
group R1) found that `jet_jumper`'s `hp_regen_per_tick` of 0.08 is StarCraft II's Reaper rate (2 per Normal s,
`void` UnitData:1476) copied in Normal time; converted it is 0.112 per tick.

- **StarCraft II:** the Reaper regenerates life only after it has taken no damage for 10 Normal s (7.14 Faster s,
  179 ticks; `LifeRegenDelay`, `void` UnitData:1475). Zerg life regeneration has no such delay.
- **Our rule:** life regeneration runs on every tick, damaged or not. Only shields wait (WP9 made that delay a
  catalog field, `shield_regen_delay_ticks`).
- **Measured** (audit §7): with 0.112 applied always, the reaper of battle `22b_reaper_vs_stalker` out-heals the
  stalker and the battle ends unresolved on all 20 seeds, where StarCraft II's stalker wins. That is the missing
  delay, not the rate: the Reaper heals in combat here and never does in the game.

**Options:**
- (a) keep 0.08, always on (default), and leave R1 deferred;
- (b) add a per-archetype `hp_regen_delay_ticks` beside `shield_regen_delay_ticks` (179 for `jet_jumper`, none for
  the Zerg), then apply 0.112: a code package with its own plan and whole-matrix measurement;
- (c) apply 0.112 always, accepting that battle 22b's reaper then survives the stalker.

**Recommendation:** (b), under Q0008: it is how the game behaves, and it reuses the shield delay's mechanism.

## Answer

