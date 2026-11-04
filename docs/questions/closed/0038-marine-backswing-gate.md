# Q0038: Should a backswing stop delaying the next attack, so the marine can take StarCraft II's backswing?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** no change. The marine (`pulse_bot`) keeps damage point 2 and backswing 4, and the
  weapon state machine keeps its current backswing rule.

## Question

**What we found.** The marine's weapon timing differs from the StarCraft II 5.0.14 game data: damage point 2 ticks
against 0.89, backswing 4 against 13.39 (our ticks, 25 per Faster second). In battle 33 the reference values (1 and
13) change nothing measurable: +0.09 marine survivors (SE 0.39) over 68 seeds.

They do break **battle 25b** (2 upgraded, stimmed marines against 1 zealot). Its zealot should end with 5 to 45
hull; with backswing 13 it ends with 62 on every seed of record, because the marines fire 16 shots instead of 24.
The cause is a rule, not the data: in `combat.rs` (`step_entity_weapon`) a weapon leaves its backswing only when the
backswing ends, so a backswing longer than the cooldown lengthens the period. Unstimmed (period 15) a 13-tick
backswing still fits; stimmed (period 10) it does not. In StarCraft II the backswing only locks out new orders; it
does not delay the next attack, and stimmed marines fire every 0.41 s. Whether the game scales the backswing with
Stimpack was not found. No shipped unit has an unstimmed backswing longer than its cooldown, so the rule is latent
today.

**Options.**

1. **No change (default).**
2. **Change the rule first, then adopt the values.** A `sim_core` plan makes the backswing a lock on orders that
   never delays the next attack (stutter-step tests show what must keep working), then the marine takes 1 and 13.
3. **Adopt 1 and 13 without the rule change.** Not recommended: battle 25b fails, and stimmed marines would fire
   slower than in StarCraft II.

**Side effects.** Option 2 changes the weapon state machine every unit uses, though no shipped unit's results
should move until a backswing exceeds a cooldown. Stutter-step behaviour (orders cancelling the backswing) must stay
as it is.

Evidence: [`docs/evidence/2026-11-03-battle-33-investigation.md`](../../evidence/2026-11-03-battle-33-investigation.md)
§1.4, §2.3, §5.3, §5.7 and §7.

## Answer

Owner (2026-11-04): Option 2 (change the rule in `sim_core` first so backswing locks orders without delaying the next attack, then adopt the StarCraft II marine values `damage_point: 1` and `backswing: 13`).

## Resolution

Option 2, carried out by plan [`2026-11-04-combat-sc2-alignment`](../../plans/2026-11-04-combat-sc2-alignment.md)
in two steps:

- **The rule** (WP1, `7353aad`, after the test-only commit `ae2c59d`; merged in `b2beb83`): a backswing locks orders
  but never delays the next attack. When the cooldown runs out during a backswing, the weapon acts as a ready one; a
  new swing ends the backswing, and with nothing to attack it runs to its end. On the shipped data of that time it
  changed no result: the whole matrix and both golden replays stayed bit-identical (evidence §1.4).
- **The marine's values** (WP6, `7842589`): `pulse_bot` takes damage point 1 and backswing 13 on both weapons.
  Battle 25b, which this question was about, does not move: its zealot ends with 32 life on all 20 matrix seeds,
  inside 5 to 45. Ten battles with marines moved by a tick or a shot; battle 33 moved more than expected (marine
  survivors 3.09 → 4.56 on 68 seeds, the damage point alone) and was re-recorded to `blue: (0, 7), red: (0, 3)`
  under Q0044's rule; both golden replays were re-recorded.

Measurements: [`docs/evidence/2026-11-04-combat-sc2-alignment.md`](../../evidence/2026-11-04-combat-sc2-alignment.md)
§1.4 and §5.
