# Q0101: Sight blockers, detection through terrain, detectors under construction, and who sees an attack reveal

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** air units and Xel'Naga towers see into sight blockers; a detector detects only what
  it could see itself (cliffs and blockers stop detection as they stop sight); detectors under construction detect;
  an air unit's attack reveals it to the struck player only, not to that player's allies (plan
  `2026-11-23-vision-and-detection` D-2, D-3, D-5, B-184).

## Question

Space Scuffle is adopting StarCraft II 5.0.14's rules for sight blockers (nothing inside is seen from outside), the
detection radius, and the attack reveal (an air unit or Colossus that damages a unit is targetable by that unit's
player for 3 game seconds). The game data do not settle four details:

1. **Sight blockers and the air.** Does an air unit (or a Xel'Naga tower) outside a smoke or grass blocker see the
   units inside it? (a) Yes (the default; our maps treat blockers as ground-level); (b) no.
2. **Detection through terrain.** A Missile Turret stands at the foot of a cliff; a cloaked Banshee hovers over the
   high ground within 11 of the turret, and an air unit of the turret's player gives vision there. Can the turret's
   side shoot the Banshee? (a) No: a detector detects only where it can see itself (the default, our current rule);
   (b) yes: detection is a plain circle and only the vision may come from another unit.
3. **Detectors under construction.** Does a Missile Turret, Photon Cannon or Spore Crawler detect while it is still
   being built? (a) Yes (the default, our current rule); (b) no, only once complete.
4. **Attack reveal and allies.** When a Mutalisk hits a unit of player A, can player A's ally B also target the
   Mutalisk for those 3 seconds? (a) Only A (the default); (b) A and its allies.

If you can, check in the editor or a custom game: a Marine inside a smoke blocker watched by a Viking outside it; the
turret, Banshee and Overlord of part 2; a turret under construction next to a cloaked Banshee; and part 4 in a 2v2.

## Answer

- **1(a):** Air units and Xel'Naga towers **see into sight blockers**.
- **2(b):** Detection **is a plain circle** (a detector detects cloaked/burrowed units within its detection radius even up cliffs or past sight blockers whenever another friendly unit grants vision there).
- **3(b):** Detectors under construction (Missile Turret, Photon Cannon, Spore Crawler) **do NOT detect** until construction completes.
- **4(b):** An air unit's / Colossus's attack reveal is visible to **the struck player and its allies** (shared team vision).

**Reasoning:** All four choices match StarCraft II's actual behaviour (note that 2(b), 3(b), and 4(b) differ from the initial plan defaults).
