# Q0130: Interceptors hover instead of strafing until the movement work ends; is that acceptable for now?

- **Asked:** 2026-11-28
- **Blocking:** no
- **Default until answered:** (a); the Carrier ships with interceptors that stop in range and fire, and the strafe is
  recorded as bug B-229

## Question

In StarCraft II a Carrier's interceptors attack by strafing: each flies through its target, turns, and comes back for
another pass. Building the strafe means changing how units move, and the movement code is being rebuilt (the Gated
plan); no other work may change it until that ends. Everything else about interceptors can be built now: up to 8 per
Carrier, built one by one, launched, shot down, recalled, and destroyed with their Carrier. Until the strafe exists,
an interceptor would stop at its attack range (2) and fire from there, like any small ranged flyer. It deals the same
damage per second; it is easier to hit with splash and short-range weapons while it hovers.

- (a) **Ship interceptors now, hovering**, with the strafe recorded as bug B-229 and built after the movement work.
  **Recommended**: the Carrier works as a Carrier at once; the difference is one of position, and it is written down.
- (b) **Wait**: keep today's missile salvo until the strafe can be built too.

## Answer

- **Option (a): Ship interceptors now, hovering at range 2**, with strafing fly-by passes tracked as bug `B-229` and implemented after the Gated movement redesign finishes.

## Resolution

Answered (a), the default built by plan `2026-11-28-carrier-interceptors` (merged `4ff8402f`): interceptors hover at
range 2 under existing orders; the strafe (`CWeaponStrafe`) stays B-229, to be built after the Gated movement redesign.
