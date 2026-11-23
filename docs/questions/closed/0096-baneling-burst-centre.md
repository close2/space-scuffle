# Q0096: Is the Baneling's 2.2 burst measured from its centre?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** from the Baneling's centre (plan `2026-11-23-splash-centres` D-2): a unit is hit when
  its edge is within 2.2 of the Baneling's centre.

## Question

The StarCraft II 5.0.14 data centre the Baneling's burst on the Baneling itself (`ImpactLocation CasterUnit`), and
Space Scuffle now does too (bug B-83). The same data keep a flag, `OffsetByUnitRadius`, that for other weapons moves
the centre to the struck unit's edge facing the attacker (Q0063). For the Baneling the "struck unit" is the Baneling,
so the data do not say what the flag does. Three readings:

- (a) **The Baneling's centre** (the default, recommended: the only reading that needs no direction).
- (b) The Baneling's front edge, towards its target (0.375 forward).
- (c) The Baneling's edge all round (an effective radius of 2.575).

If you can, check in the editor: a Baneling attacking a Zergling, with a Marine (radius 0.375) placed 2.8 behind the
Baneling's centre (reached only under (c)) and another 2.8 in front of it, beside the Zergling (reached under (b) and
(c), not under (a)).

## Answer

- **Option (a): The Baneling's centre.** A unit is hit when its edge is within `2.2` of the Baneling's centre.

**Reasoning:** Matches StarCraft II's engine behaviour. `OffsetByUnitRadius` shifts a splash epicentre along the caster→target vector by the target's radius; when the Baneling is both caster and impact unit (`ImpactLocation CasterUnit`), that vector is zero (and Banelings can also manually detonate with no target at all), yielding an isotropic 2.2-radius circle around the Baneling's centre.
