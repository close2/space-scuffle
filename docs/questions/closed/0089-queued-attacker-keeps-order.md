# Q0089: May a unit queued behind its own fighting friends keep its attack order indefinitely?

- **Asked:** 2026-11-19
- **Blocking:** no
- **Default until answered:** yes. Plan `docs/plans/2026-11-19-crowd-edge-steering.md` D-4 is implemented with
  that answer.

## Question

A unit with an attack order (a chase, or an attack-move) can stand behind friends who are holding their ground and
firing, for example in a corridor or at the back of a concave. Today, after 10 s without progress, it gives the
order up and reacquires 2 s later. That shows up as give-ups and stepping in every corridor fight (B-103).

The plan proposes that such a waiting tick does not count as a stall:

- the unit keeps its order for as long as the friend ahead holds its ground;
- a plain `Move` into a blocked spot is still given up, as today;
- the 5.0.14 leash (21 from where the target was acquired) still ends a chase.

StarCraft II's data has no give-up rule for this case, and Blizzard's guide says units outside the concave stand.
The game's behaviour for a queued attack order is otherwise unsourced.

## Options

- **(a)** Yes: a unit queued behind fighting friends keeps its attack order (the default).
- **(b)** No: keep today's 10 s give-up and 2 s reacquire.
- **(c)** Something else, such as a longer give-up.

## Answer

**Option (a) — Yes: a unit queued behind fighting allies keeps its attack order indefinitely (while plain `Move` stall-out and the 21-cell acquisition leash remain unchanged).**

**StarCraft II engine & gameplay insight:** In actual StarCraft II gameplay, `AttackMove` and `AttackUnit` orders never time out into `Idle` simply because friendly firing units occupy the front line of a choke or concave. Back-line units keep their attack order indefinitely (standing smoothly in place once their local surround/avoidance arc is saturated, without oscillating or dropping orders) so that the very tick a front-line ally dies or advances, the queued unit immediately steps forward into weapon range and fires. By contrast, only plain `Move` orders into a permanently blocked spot settle/give up (so moving units do not push forever against a closed wall or friendly ball), and the acquisition leash still caps auto-acquired chases. Treating ticks spent queued behind fighting friends as non-stall matches SC2's combat flow and eliminates the 10 s give-up / 2 s reacquire stepping defect (`B-103`).
