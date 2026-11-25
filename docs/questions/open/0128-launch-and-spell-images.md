# Q0128: Images of spell casters and missile launches: three details

- **Asked:** 2026-11-27
- **Blocking:** no
- **Default until answered:** 1(a), 2(a), 3(a), as built by plan `2026-11-27-launch-and-spell-visibility`

## Question

When a ground unit shoots your units from somewhere you cannot see, you get a dimmed, still image of it for up to
1 second (your answer to Q0112). In StarCraft II the same happens in two more cases, and plan
`2026-11-27-launch-and-spell-visibility` adds them: a unit that fires a missile at your unit leaves the image as soon
as the missile leaves, not only when it lands; and a spellcaster whose spell damages your units leaves an image of
the caster (Psionic Storm only on its first hit, since its later pulses show nothing; Fungal Growth on every pulse).
The game data settles these. Three details are not settled by any source I found.

**1. Who gets the image when a missile leaves.** The game's field applies "to the target".
- (a) The player whose unit the missile is aimed at. **Recommended**; it matches how the damage's image works.
- (b) Every player whose units the missile will hit (for a missile with splash).

**2. A spell missile aimed at the ground** (Fungal Growth flies to a point, not a unit).
- (a) Nobody gets an image when it leaves; the players it hits get one when it lands, a moment later.
  **Recommended**; no unit is aimed at.
- (b) Players with units near the point get one when it leaves.

**3. How long these images stay.**
- (a) As your Q0112 answer: until you see the spot or the caster, and at most 1 second after the last launch, hit or
  pulse that made it. **Recommended**: one rule for every image.
- (b) A different cap for spell casters (say how long).

Every option changes what you see only: no battle outcome, replay or targeting moves, because no shipped spell or
missile launch makes its caster targetable.

## Answer

