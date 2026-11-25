# Q0124: May the new movement cost as much as today's movement with the crowd fix, if it stays above 50 ticks per second?

- **Asked:** 2026-11-26
- **Blocking:** no
- **Default until answered:** (a).

## Background

Your bar is at least 50 ticks per second on every benchmark (Q0093). The crowd fix you chose in Q0118 costs about
+50 % to +60 % on the two 400-unit benchmarks. The new movement adds a check per unit but should let the crowd fix
run less often.

## Options

- **(a) Budget = today's movement with the crowd fix, and ≥ 50 ticks/s (recommended).**
- **(b) Budget = today's movement without the crowd fix** (stricter; may need a faster solver, with the trade-offs of
  Q0120 (b)).

## Answer

- **Option (a): Budget = today's movement with the `Q0118` crowd fix, and ≥ 50 ticks/s on every benchmark.** Preserves the ≥ 50 TPS real-time floor while letting the redesign prove its net solver-iteration savings organically.

## Resolution

Answered as above. These answers (Q0120 (a), Q0121 (a), Q0122 (c), Q0123 (a), Q0124 (a)) govern the implementation
plan of the movement redesign, written next from the design round `docs/plans/2026-11-26-movement-blank-sheet.md`.
