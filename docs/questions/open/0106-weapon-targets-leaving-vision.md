# Q0106: When a target leaves vision, what counts as a teleport, and what part of a unit must be seen

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** a player's attack order on a unit that leaves vision walks to where the unit was last
  seen and then ends (built after the contact-solver work; until then it keeps following but holds fire); an
  automatic target that leaves vision is dropped at once, and the unit takes the best visible enemy or resumes its
  own order; every teleport effect (Blink, and later Mass Recall, Adept Shade, Hyperjump) makes missiles in flight
  miss; a unit is seen when the cell at its centre is visible (plan `2026-11-24-weapon-validation` D-4, D-5, D-6a).

## Question

Space Scuffle is adopting StarCraft II 5.0.14's weapon rules: a weapon fires only at a unit its player sees, a
missile whose target walks into the fog of war lands on the ground where the target was, and a missile at a unit that
blinks misses. The game data settle these, but not four details:

1. **A player's attack order on a unit that walks into the fog.** A Stalker is ordered to attack a Zergling, which
   runs out of the Stalker's sight. Does the Stalker (a) walk to where the Zergling was last seen and stop there,
   resuming the attack if it sees the Zergling again (the default); (b) stop at once; or (c) keep following the
   Zergling through the fog without firing?
2. **An automatic target that walks into the fog.** The same, but the Stalker was attack-moving and picked the
   Zergling itself. Does it (a) drop the Zergling at once and pick another visible enemy, or carry on attack-moving
   if there is none (the default); or (b) first walk to where the Zergling was last seen, as in part 1?
3. **Which moves make missiles miss.** Blink does. Do Mass Recall, an Adept's Shade and a Battlecruiser's Tactical
   Jump do too? (a) Yes, every teleport (the default); (b) only Blink.
4. **What part of a unit must be in vision.** A Viking (range 9, sight 10) shoots a Colossus at full range, where the
   Colossus's centre lies just outside the Viking's sight but its body inside. Can the Viking fire without another
   unit's vision? (a) No: the centre must be visible (the default, our current rule for picking targets); (b) yes:
   any part of the unit is enough.

If you can, check in the editor or a custom game: a Stalker chasing a Zergling into fog, ordered and on attack-move;
a Stalker blinking away from a Roach's or a Mass-Recalled army from Viking missiles in flight; and a lone Viking
against a lone Colossus at full range.

## Answer

- **1(a):** A player's attack order on a unit that leaves vision walks to where the unit was last seen and stops there, resuming the attack if the unit enters vision again.
- **2(a):** An automatic target that leaves vision is dropped immediately, and the unit picks the best visible enemy or resumes its order.
- **3(a):** Every teleport effect (Blink, Mass Recall, Adept Shade, Battlecruiser Tactical Jump) makes in-flight missiles miss. **Important timing requirement:** for spells with a cast/channel/wind-up time (such as Mass Recall, Tactical Jump, or Adept Shade travelling), missile disjointing must occur **only at the tick the actual teleport happens**, not when the ability is first cast.
- **4(a):** A unit is seen when the cell at its **centre** is visible.
