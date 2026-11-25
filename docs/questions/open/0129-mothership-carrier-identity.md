# Q0129: Is the Mothership Carrier the Carrier, the Mothership, or a mix?

- **Asked:** 2026-11-27
- **Blocking:** no
- **Default until answered:** (a); until interceptors are built (bug B-22), the unit keeps its missile salvo, shows
  no image of itself when it fires, and keeps its current life, shields and cost

## Question

The Cosmic Enclave's "Mothership Carrier" names two StarCraft II units, and its values come from both. Its life and
shields (350 and 350) are the Mothership's; its cost (350 minerals, 250 gas), supply (6), build time, attack range (8),
weapon name ("Interceptor Fleet Salvo") and description ("deploying swarms of autonomous saucer interceptors") are the
Carrier's. The two attack very differently: a Carrier sends out up to 8 small interceptor craft, which it builds
itself and which can be shot down; a Mothership fires an instant beam. Today ours fires one salvo of 8 missiles,
which neither does.

- (a) **The Carrier.** Interceptors get built (bug B-22): a hangar of up to 8, built one by one, flying out to attack
  and recalled. Life and shields become the Carrier's (300 and 150). **Recommended**: most of its values, its name for
  the weapon and its description already say Carrier, and the Enclave has no other Carrier.
- (b) **The Mothership.** Its attack becomes an instant beam (6 damage, range 7), cost 400/400 and supply 8; it gains
  the Mothership's abilities later. No interceptors.
- (c) **Keep the mix** as a deliberate deviation, written down: Mothership toughness with a Carrier attack, the
  missile salvo standing in for interceptors.

Only matches with this unit change; it is in none of the test battles.

## Answer

- **Option (a): The Carrier.** Align life and shields to the Carrier (`300` life / `150` shields) and build the Carrier's interceptor hangar (`B-22`: up to 8 interceptors built one by one, flying out to attack and recalled).

**Reasoning:** Its cost (`350/250`), supply (`6`), build time, range (`8`), weapon name, and description already match the Carrier, and the Cosmic Enclave has no other Carrier; a separate Mothership unit can always be added independently later if desired.
