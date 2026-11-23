# Q0093: What cost may the new movement solver have?

- **Asked:** 2026-11-23
- **Blocking:** no. WP4 and WP5 proceed meanwhile; the switch-over (WP6) waits.
- **Default until answered:** keep the plan's bar. WP6 switches over only if the Solver costs at most +10 %
  instructions on every throughput scenario.

## Background

The new movement (plan `docs/plans/2026-11-20-contact-solver.md`, approved in Q0092) behaves far better on the
development scenarios:

| Measure | Legacy | Solver |
|---|---:|---:|
| Back-and-forth at gaps | 17.7 | 1.1 |
| Back-and-forth at corners | 20.9 | 0.5 |
| Give-ups in crossings | 152 | 2 |
| Throughput through gaps | 0.66 | 0.78 |
| Friends above 5 % overlap at gaps | 14.7 % | 1.6 % |

The plan set itself a cost bar of +10 % instructions per tick. After every behaviour-preserving optimisation it
costs:

| Throughput scenario | Instructions |
|---|---:|
| 400-unit 64×64 battle | +55 % |
| 128×128 | +8 % |
| Showcase canyon | +51 % |

Every scenario still runs well above real time; the lowest is about 70 ticks per second against the charter's 25.
About a quarter of the excess is a change in how often units acquire targets, not the solver itself. Dense fighting
crowds keep the solver near its full iteration count every tick.

Reaching +10 % would need a different solver (Gauss–Seidel with a fixed colouring: uncertain gain, more design work)
or fewer iterations (more overlap). The charter only requires real time (25 ticks per second) for every scenario.

## Options

- **(a)** Keep +10 % (the default). Build Gauss–Seidel; if it cannot reach the bar, the Solver does not switch over.
- **(b)** Replace the bar with a real-time margin: every throughput scenario at 4× real time (100 ticks per second)
  or more on the reference machine, and instructions at most +60 %. Gauss–Seidel then becomes an optional later
  optimisation.
- **(c)** Another bar you name.
## Answer

- **Option (c):** Replace the `+10 %` instruction bar with a real-time margin of **at least 2× real time (50 ticks per second, ≤ 20 ms/tick)** on every throughput scenario on the reference machine, and **instructions at most +60 %**.

**Reasoning:** The new contact solver delivers a massive quality improvement across all movement metrics (back-and-forth `-94 %` to `-98 %`, crossing give-ups `-99 %`, gap overlap `-89 %`, gap throughput `0.66 → 0.78`), and the slowest stress scenario (`400-unit 64×64 battle`) still runs at `~70` TPS (`~2.8×` real time, `~14.3 ms`/tick). Setting the floor to `100` TPS (`4×` real time, Option b) risks accidentally blocking WP6 if the reference machine measures near `~70` TPS on that worst-case clump. A `50` TPS (`2×` real time) floor with a `+60 %` instruction cap preserves at least 50 % single-core CPU headroom at normal `25` TPS (`40 ms`/tick) gameplay while unblocking WP6 immediately. Gauss–Seidel (or other single-threaded optimisations such as SoA/SIMD or sleeping non-overlapping units) remains an optional later optimisation.
