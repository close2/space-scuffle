# Q0144: How should the Brood Lord analogue's two escorts look when they are drawn?

- **Asked:** 2026-11-27 (plan `2026-11-27-missile-throws-and-escorts` D-8, bug B-235)
- **Blocking:** no
- **Default until answered:** (a). The escorts are drawn with the Broodling analogue's (`brood_mite`) installed
  flipbook; no new clip is generated and nothing is spent.

## Question

In StarCraft II a Brood Lord carries two Broodlings that hover behind it, at about 150° to either side of its facing
(`BroodLordHangar`, `ExternalAngle ±149.99`, `liberty` AbilData:3808–3824). They are thrown at the target as its attack.
Space Scuffle models them as a count, so today nothing is drawn behind the Brood Lord analogue (`spore_matriarch`).
The plan draws them as two small bubbles in those positions, appearing when one is rebuilt and vanishing when it is
thrown.

The game's escort unit, `BroodlingEscort`, is a Broodling in flight (its data inherits from the Broodling's,
`liberty` UnitData:11194–11214). Our Broodling analogue, `brood_mite`, already has real Veo clips (Idle, Walk, Attack).
The choice is what to show in the escort bubbles:

- (a) The `brood_mite`'s own Idle clip, at its own size (default). Free and honest about what the escort is, but it is
  a ground creature's clip shown in the air.
- (b) A new Veo clip of the creature in flight, generated through `asset_baker` under the usual review rules: about 4
  generations, at most $4.00 of the media budget (the ledger had $123.25 of $230 spent after the broodlings batch).
- (c) Do not draw the escorts; keep B-235 open as a recorded presentation difference.

Recommendation: (a) now; (b) later if the look in (a) reads wrong in a capture.

## Answer

- **Option (b): Generate a new Veo clip of the escort creature in flight** through `asset_baker` under the usual review rules (about 4 generations, at most `$4.00` of the media budget; stay within that 4-generation cap and take the best available result rather than overspending).
