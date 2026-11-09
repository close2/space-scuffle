# Q0066: Does an attack-speed buff scale a weapon's random delay, and does the first attack draw one?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** no change. Each attack's random delay (−0.0625 to 0.125 Normal s) is added, unscaled, to
  the cooldown its release charges, so a Stimpack or any other attack-rate buff shortens the period but not the
  delay, and the first attack after the weapon becomes ready is not delayed. Both points stay recorded under bug
  B-91 in `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP11 (bug B-3)
gives every weapon StarCraft II's `RandomDelayMin` −0.0625 and `RandomDelayMax` 0.125 Normal s (`core`
WeaponData:35-36, the `CWeaponLegacy` default). The SC2Mapster editor reference
([Data/Weapons/Legacy](https://sc2mapster.wiki.gg/wiki/Data/Weapons/Legacy), read 2026-11-09) says only that "a
random value between the maximum and minimum is selected and used as a delay before firing the weapon". Blizzard's
2018 balance notes say the Battlecruiser's random delay was large relative to its cooldown and made it deal less
damage than expected, so the delay lengthens the mean cycle; the negative minimum says it acts on the time between
attacks. Evidence [`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §12.

Two details are not in the data or the reference:

1. Whether a weapon-speed multiplier (Stimpack's, or any attack-rate buff) also scales the random delay.
2. Whether the first attack of an engagement, from a ready weapon, waits for a drawn delay (only a non-negative part
   could apply there).

**Why it matters.** Item 1 changes a stimmed Marine's or Marauder's mean period by about 0.01 Normal s (a fifth of a
tick); item 2 shifts the first volley of every fight by up to two ticks and starts the volleys
desynchronised one attack earlier.

**Options.**

1. **No change (default).** Unscaled, and no delay before the first attack.
2. **Scale with the attack rate**, and/or **draw before the first attack** as well.
3. **Measured.** An editor test: 20 Marines with and without Stimpack fire at a high-life target; log each attack's
   game loop.

**Recommendation.** Option 3; both are one-line rule changes once measured.

## Answer

**Option 1 (No change — unscaled cooldown jitter; do not block on an editor test).**

**Engine insight:** Keep Option 1 and close `B-91` without waiting on an editor measurement. The `~0.01` Normal s (`0.2` tick) difference from scaling `RandomDelay` by Stimpack is negligible in practice. (Optionally, if you wish to draw a non-negative delay in `[0, RandomDelayMax]` on a unit's initial ready shot so lines of Marines naturally desynchronize their muzzle flashes and audio on volley 1 as SC2's *"used as a delay before firing the weapon"* describes, you may do so, or simply keep the current implementation as-is.)
