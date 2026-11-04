# Q0036: Should the zealot's weapon timing and range follow StarCraft II?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** no change. `psi_grey` keeps damage point 4, period 22, range 0.3, backswing 6 and
  both strikes in one tick.

## Question

**What we found.** The zealot (`psi_grey`) differs from the StarCraft II 5.0.14 game data in its weapon. Reference
values are in our ticks (25 per Faster second).

| Field | Ours | Reference | Effect on battle 33 (marine survivors, paired over 68 seeds; baseline 7.41) |
|---|---|---|---|
| Damage point (wind-up) | 4 ticks | **0** | −2.82 (SE 0.33) at the same cadence |
| Period | 22 ticks | 21.43 | −1.79 (SE 0.25) for period 21 |
| Range | 0.3 | **0.1** | **+0.82** (SE 0.42): favours the marines, fewer zealots fit round a marine |
| Backswing | 6 ticks | 8.93 | −0.12 (SE 0.12), noise |
| Two strikes of 8 | same tick | 5 ticks apart; the second only if the target is within 2.1 | not measurable: no catalog field spaces them |

Plan 17 chose the 4-tick wind-up as a "realistic SC2 frontswing"; the game data has none. Doc 07 gives period 21
and range 0.2.

**A catch in the cadence.** In `combat.rs` (`step_entity_weapon`) a weapon with damage point 0 fires every
`period + 1` ticks; with damage point 1 or more it fires every `period` ticks (measured). So damage point 0 needs
period 21 to keep today's 22-tick cadence, or period 20 for a 21-tick cadence. The reference, 21.43, lies between:

| Zealot weapon | Cadence | Battle 33: marine wins / zealot wins / unresolved (68 seeds, 500 ticks) | Marine survivors |
|---|---|---|---|
| Today (damage point 4, period 22) | 22 | 61 / 6 / 1 | 7.41 |
| Damage point 0, period 21 | 22 | 42 / 10 / 16 | 4.59 |
| Damage point 0, period 20 | 21 | 25 / 26 / 17 | 2.46 |
| Every reference value of both units, period 21 | 22 | 45 / 17 / 6 | 4.71 |

**Options.**

1. **No change (default).**
2. **Damage point 0 and range 0.1, backswing 9**, with period 21 (cadence 22, as today) or period 20 (cadence 21,
   nearer the reference). A catalog edit only.
3. **Option 2 plus the second strike 5 ticks later**, only if the target is within 2.1. This needs a combat-rule
   change and its own plan.

The standing rule from Q0008 (follow StarCraft II where the specification is silent) points to option 2; Q0007 keeps
today's baselines, hence the default.

**Side effects.** Battle 33 would fail its committed expectation (marines 4 to 16 alive, no zealots) on some seeds
of record: with every reference value it fails on 3 of 4. In the whole-matrix control (all 50 battles, every
reference value), the only other battles holding a zealot that moved were 23a (zealots alive 2.10 → 2.00 on average,
still passing) and 25b (zealot hull 32 with damage point 0 on the first seed of record, still inside 5 to 45).
The zealot changes were not played over the whole matrix on their own.

Evidence: [`docs/evidence/2026-11-03-battle-33-investigation.md`](../../evidence/2026-11-03-battle-33-investigation.md)
§1.5, §2.4, §5.3, §5.7 and §7.

## Answer

Owner (2026-11-04): Option 2 — update the zealot (`psi_grey`) to StarCraft II's weapon timing and range (`damage_point: 0`, `range: 0.1`, `backswing: 9`, `period: 21`), and also clean up the `damage_point = 0` `period + 1` cadence quirk in `combat.rs` if appropriate.

## Resolution

Carried out by plan [`2026-11-04-combat-sc2-alignment`](../../plans/2026-11-04-combat-sc2-alignment.md) (D-3, WP2,
WP4), merged to `main` in `49177b4` (WP2) and `be5a476` (WP4):

- **Cadence quirk, cleaned up** (`5c2b8e3`, WP2): a damage point of 0 now releases on the tick the swing starts, so
  every weapon's attack cycle is its `period_ticks`. Before, such a weapon held a one-tick wind-up and fired every
  `period + 1` ticks. The six shipped damage-point-0 identities each became one tick faster per cycle; battles 9a,
  10, 12a, 12b, 17, 18 and 26 moved inside their expectations, and `golden_replay_orders.ron` was re-recorded.
- **Zealot weapon** (`748f3a9`, WP4, after the test-only commit `5a798a0`): `psi_grey` takes damage point 0,
  period 21 (now also its cycle), range 0.1 and backswing 9. Battles 7, 8, 23a, 23b, 25a, 25b and 33 moved; only
  battle 33 needed re-recording (Q0044), and `golden_replay.ron` was re-recorded.

Measurements: [`docs/evidence/2026-11-04-combat-sc2-alignment.md`](../../evidence/2026-11-04-combat-sc2-alignment.md)
§2 and §3.
