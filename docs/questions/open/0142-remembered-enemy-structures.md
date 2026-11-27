# Q0142: Should enemy structures stay on the minimap and in the fog once they are out of sight?

- **Asked:** 2026-11-27 (draft by plan `2026-12-01-minimap`)
- **Blocking:** no
- **Default until answered:** (a)

## Question

In StarCraft II 5.0.14, an enemy structure that you saw once stays drawn where it was, in the fog and on the
minimap, as a still image (`FogVisibility Snapshot`), until you see that spot again. Space Scuffle shows nothing
of an enemy structure once it is out of sight. That was put out of scope in Phase 5 (Q0002) and is not recorded as
a bug. The new minimap makes the gap easy to see: after scouting a base, the minimap shows only fog where it
stood.

- (a) Keep it out of scope. The minimap shows what the simulation lets you see now, plus attacker snapshots.
  **Recommended for now**: it needs a per-viewer memory of structures in the simulation's presentation workspace,
  like the attack snapshots (no checksum change, but a new published list), and its own plan.
- (b) Register it as a bug and plan it next. The attacker-snapshot record and pool and this minimap's marks list
  are the natural base, so the client work is small; the simulation side is a new unhashed record per viewer and
  structure.
- (c) Something else.

## Answer

- **Option (b): Register remembered enemy structures (`FogVisibility Snapshot` in fog and on the minimap) as a bug and plan it next.** Essential RTS scouting readability, and cleanly reuses the existing presentation-only attacker-snapshot and minimap marks infrastructure with no simulation checksum impact.
