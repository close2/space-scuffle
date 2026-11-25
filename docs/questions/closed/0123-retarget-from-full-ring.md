# Q0123: Should a zergling whose target is fully surrounded switch to the nearest enemy that still has room?

- **Asked:** 2026-11-26
- **Blocking:** no
- **Default until answered:** (a).

## Background

Today a zergling whose automatically chosen target is surrounded stands behind the crowd while another enemy close
by has free space (bug B-192); in battle 28 the immortals win most runs because of it, though in StarCraft II
zerglings overrun immortals. The earlier fix (switch to the nearest enemy with room) worked for battle 28 but was
built on a solver that shoved enemies and was stopped. StarCraft II's exact rule for this is not documented.

## Options

- **(a) Switch to the nearest enemy with room within scan range, and keep it until reached (recommended);** recorded
  as a minor deviation (D-ε) unless a source shows the game does the same.
- **(b) Keep standing (today).**

## Answer

- **Option (a): Switch to the nearest enemy with free room within scan range, and keep it until reached (`D-ε`).** Directly fixes Battle 28 (`B-192`) and eliminates rear melee units stalling idle behind a surrounded front rank.

## Resolution

Answered as above. These answers (Q0120 (a), Q0121 (a), Q0122 (c), Q0123 (a), Q0124 (a)) govern the implementation
plan of the movement redesign, written next from the design round `docs/plans/2026-11-26-movement-blank-sheet.md`.
