# Q0095: May an engaged melee unit be nudged sideways along its attack ring?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** no. Units that are attacking stay fixed (StarCraft II: units holding, attacking or
  patrolling block, SC-8 of `docs/evidence/2026-11-20-movement-survey.md`). The miss described below stays a
  reported figure and is not a pass/fail threshold.

## Question

In a corridor two units wide, two melee attackers could stand side by side at an enemy. Under the new movement
solver (plan `docs/plans/2026-11-20-contact-solver.md`, Amendment 11, R8) the first attacker usually arrives on the
corridor's centre line and starts attacking. Because attacking units never move, the second attacker finds no
room beside it and waits behind. On the test scenes this happens in 301 of 301 two-wide corridors; the current
movement misses 210 of 301.

The only principled fix is to let an attacking unit be **pushed a little sideways along its attack ring**. It
would keep its range and keep attacking, but it would make room for a neighbour. That departs from StarCraft II's
rule that attacking units are not pushed.

- **(a)** No, attacking units stay fixed (the default). Corridors fight one abreast until the front unit dies.
- **(b)** Yes, as a sanctioned deviation (added to Q0052), behind a data switch that defaults to on: an attacking
  unit may be pushed along its ring by friends who need the space, never out of range and never by enemies.
- **(c)** Yes, but behind a data switch that defaults to off, so the StarCraft II behaviour stays the default.

## Answer

- **Option (b): Yes, as a sanctioned deviation (added to Q0052), behind a data switch that defaults to ON.** An attacking melee unit may be nudged sideways along its attack ring by allies who need room, never pushed out of attack range and never pushed by enemies.

**Caveat & reasoning:** Units explicitly on **Hold Position** must remain immovable and must not be nudged sideways; only ordinary attacking units slide along the ring. The 301/301 miss rate is largely a side-effect of the new contact solver eliminating steering jitter so the lead attacker marches dead-centre down a 2-wide corridor. Gentle allied-only sliding along the attack ring fixes melee choke responsiveness while keeping strict StarCraft II behaviour selectable via the data switch.
