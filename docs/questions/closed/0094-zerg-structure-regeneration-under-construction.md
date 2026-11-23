# Q0094: Do Zerg structures regenerate life while under construction?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** no: a structure under construction gains life only from the building work; it
  regenerates life (and energy) from the tick after it completes (plan `2026-11-23-regeneration` D-1).

## Question

Space Scuffle's Zerg structures regenerate 0.38 life per second when finished. Until now they also regenerated
while being built, so a damaged Hatchery foundation healed while it was built, and even while its builder was away.
The StarCraft II 5.0.14 data has no flag for this either way, and no web source we found says what the game does.

If you can, check in the game: start a Hatchery (or Spine Crawler), have a Zergling hit it a few times, and watch
whether its life rises faster than the build progress between hits (or simply compare its life at completion with
its maximum).

- (a) **No regeneration under construction** (the default, recommended: it is what the bug register assumed and how
  our shields already behave).
- (b) Regeneration under construction, as before.

The choice is one condition in the code; either is easy to switch.

## Answer

- **Option (a): No passive life regeneration while under construction.** A structure under construction gains life only from build progress; passive life (and energy) regeneration starts on the tick after completion.

**Reasoning:** Matches StarCraft II's behaviour (where `UnderConstruction` suppresses passive vital regeneration on structures), matches how Space Scuffle shields already behave, and matches the bug register's expectation. Because this is a single guard in the regeneration pass, exposing a per-unit-type data flag later (defaulting to `false`) remains trivial if a future unit or structure ever needs it.
