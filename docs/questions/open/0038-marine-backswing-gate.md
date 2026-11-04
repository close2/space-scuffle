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

