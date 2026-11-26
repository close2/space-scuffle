# Q0121: Under the new movement, may allies still nudge a fighting melee unit sideways (D-δ)?

- **Asked:** 2026-11-26
- **Blocking:** no
- **Default until answered:** (a), retire it under the new movement only.

## Background

In Q0095 you allowed allies to nudge an attacking melee unit along its attack ring (D-δ), so that a corridor's
second rank and a crowded surround find room. In Q0118 you said attacking units should not be pushable if a cleaner
design handles corridors and surrounds without it. The new design makes room by choosing free spots and switching
targets instead.

## Options

- **(a) Retire D-δ under the new movement (recommended):** fighting units are never moved by allies, as in
  StarCraft II. In a corridor only the front rank fights, as before Q0095.
- **(b) Keep it as a data switch,** default off.
- **(c) Keep it on.**

## Answer

- **Option (a): Retire `D-δ` under the new movement.** Fighting units (as well as Hold Position units) are never pushed or nudged by allies, matching StarCraft II and the owner's `Q0118` rule; the new movement makes room through free-spot selection and target switching instead.

## Resolution

Answered as above. These answers (Q0120 (a), Q0121 (a), Q0122 (c), Q0123 (a), Q0124 (a)) govern the implementation
plan of the movement redesign, written next from the design round `docs/plans/2026-11-26-movement-blank-sheet.md`.

## Owner addendum (2026-11-29)

After watching StarCraft II footage, the owner revised this answer: marines in combat are pushed slightly by their
allies, so attacking units **may** be nudged by friends (never out of their weapon band), while **Hold Position units
must never move**, and enemies still never push each other. Applied to the Gated movement plan as Amendments 3a and 3b
(precedence: enemies never push, then the weapon band, then the ally's nudge).
