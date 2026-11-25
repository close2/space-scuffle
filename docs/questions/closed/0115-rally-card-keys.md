# Q0115: Should the rally card entries use StarCraft II's Y and G keys?

- **Asked:** 2026-11-25
- **Blocking:** no
- **Default until answered:** (a) positional keys, like every other card entry

## Question

Plan `2026-11-25-rallies` adds card entries to set a structure's rally ("Set Rally Point") and, on the Hatchery
analogues, its worker rally ("Set Worker Rally Point"). In StarCraft II 5.0.14 their standard keys are Y and G
(Blizzard, Game Guide: Zerg Basics: "press the G key and left-click"). This project's card gives every position a
fixed key (Q E R T Y / F G J K L / Z C V B N) whatever the entry, so the rally entries get the key of the position
they land on, recorded as minor difference B-214.

- (a) Keep positional keys (default). Simple and consistent; the right click, which sets most rallies, is
  unchanged.
- (b) Reserve Y and G for the rally entries whenever a rally-capable structure is selected, displacing the
  entries that would sit there.
- (c) Put the rally entries at the positions whose keys are Y and G (row 0 column 4, row 1 column 1), shifting
  other entries.

Recommendation: (a).

## Answer

- **Option (a): Keep positional keys.**

**Reasoning:** Keeps Space Scuffle's Grid-style positional command card consistent across all structures, and right-clicking sets most rallies anyway.

## Resolution

Answered (a), the default already built by plan `2026-11-25-rallies` (merged `91350af1`): rally card entries keep the
card's positional keys. B-214 stays a recorded minor difference.
