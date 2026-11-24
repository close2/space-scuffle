# Q0108: Can you select or order a worker while it is inside a gas extractor?

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** (a): you cannot select it; it leaves your current selection; control
  groups keep it, so it is back in the group when it comes out; you cannot order it while it is inside.

## Question

While a worker is inside a Refinery, Assimilator or Extractor (about 1.4 seconds a trip), it is not
drawn. Plan `2026-11-24-hidden-harvesters` has to decide what the player can do with it meanwhile.

- **(a) Not selectable, kept in control groups (default).** It cannot be clicked or box-selected. If it
  was selected, it drops out of the selection. A control group keeps it: pressing the group's key while
  it is inside selects the others, and once it is out the key selects it again. You cannot order it
  while it is inside.
- **(b) It stays selected and takes orders.** It stays in the selection while inside; an order makes it
  leave the extractor at once, losing that load (the game data allow this: collecting gas is an
  interruptible step).
- **(c) As (a), but control groups also drop it,** as they drop a unit loaded into a transport here
  today.

Recommendation: (a), unless you know the game does otherwise. If you can check: put a gas SCV in a
control group and press the key while it is inside.

## Answer

- **Option (a): Not selectable while inside, leaves active selection, kept in control groups, cannot be ordered while inside.**

**Reasoning:** Matches StarCraft II verbatim — a worker hidden inside a gas extractor cannot be clicked or box-selected and leaves the current active selection, while remaining registered in its control groups so pressing the group key selects it again once it exits.
