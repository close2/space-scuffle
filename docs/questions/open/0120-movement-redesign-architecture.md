# Q0120: Rebuild movement as "aim well, never step into what won't move, then separate", keep today's solver, or swap in a different solver?

- **Asked:** 2026-11-26
- **Blocking:** only the implementation plan of the movement redesign; the interim crowd fix (C-3′, Q0118 (a))
  proceeds either way.
- **Default until answered:** (a) is planned but not built; today's movement with C-3′ stays.

## Background

You asked for a blank-sheet look at movement. Every movement failure of the last weeks has one source: a unit walks
its full step into units that will not move out of its way (fighting allies, enemies, units queued in front), and
the solver that separates units afterwards has to undo it. With your rules (fighting units and Hold Position are
never pushed, enemies never push), there is often no way to undo it at all, so crowds overlap.

## Options

- **(a) Restructure (recommended).** Units choose free spots round a target and switch to an enemy with room when
  theirs is full; a unit never steps into a unit that will not move (it steps round it or waits); the solver stays
  as it is and only cleans up small overlaps. Close to how StarCraft II describes its own movement. Built beside
  today's movement and switched over only if it passes the tests written in advance. In a rough prototype it cut
  crowd overlap by two thirds or more while hardly moving fighting units, but units changed direction more often
  than today and still overlapped enemies slightly, which the build must fix first.
- **(b) A faster solver (Gauss–Seidel with shock propagation).** Cheaper and converges in one pass, but crowds queue
  behind their front line (battle 26: zerglings win 2 of 12 instead of 9), and results depend on the order units are
  processed in.
- **(c) Stop here.** Keep today's movement with the crowd fix you approved in Q0118.

## Answer

- **Option (a): Restructure movement as "aim well, never step into what won't move, then separate."** Build side-by-side with today's movement and switch over only once pre-registered acceptance tests pass (including resolving the prototype's extra direction changes and slight residual enemy overlap).

**Reasoning:** Directly addresses the root architectural cause (stepping into immovable bodies and relying on a Jacobi post-pass to undo impossible overlaps) and aligns closely with StarCraft II's movement architecture.
