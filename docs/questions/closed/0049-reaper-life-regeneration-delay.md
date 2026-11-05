# Q0049: Should life regeneration wait after damage, as the Reaper's does in StarCraft II?

- **Asked:** 2026-11-05
- **Blocking:** no
- **Default until answered:** option (b), under the standing rule Q0008 (charter §8): plan
  [`2026-11-05-life-regeneration-delay`](../../plans/2026-11-05-life-regeneration-delay.md) adds the delay and
  then applies R1.

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

## Resolution

Carried out as option (b) under the standing rule Q0008 (charter §8), without a separate answer, by plan
[`2026-11-05-life-regeneration-delay`](../../plans/2026-11-05-life-regeneration-delay.md): per-archetype
`hp_regen_delay_ticks` (WP1 `72be09a`), `jet_jumper` 179 ticks (WP2 `a302eb5`), R1 0.112 (WP3 `f93ec20`);
measurement in [`2026-11-05-life-regeneration-delay.md`](../../evidence/2026-11-05-life-regeneration-delay.md).
Battle `22b_reaper_vs_stalker` stays a stalker win on all 20 seeds (now at tick 139 instead of 172), and
`22a_reaper_vs_2_workers` a reaper win with the reaper at 40 life instead of about 46; nothing else moved. The owner
may reopen this to choose (a) or (c): a delay of 0 and a rate of 0.08 restore the old behaviour.
