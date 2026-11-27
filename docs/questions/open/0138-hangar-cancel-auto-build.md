# Q0138: Cancelling an interceptor build while auto-build is on: let auto-build queue it again?

- **Asked:** 2026-12-01
- **Blocking:** no
- **Default until answered:** (a); a cancelled build is refunded, and auto-build queues (and charges) a new one on the
  next tick if the player can pay

## Question

The Carrier analogue builds its interceptors by itself while "auto-build" is on (StarCraft II 5.0.14 turns it on for
every new Carrier). The player will be able to build one by hand, cancel the newest build for a full refund, and switch
auto-build off. The game data says nothing about what a cancel does to auto-build, so with auto-build on a cancel frees
a slot that auto-build fills again at once: the cancel only helps after switching auto-build off. (Plan
`2026-12-01-death-spawns-and-hangar-commands` D-7.)

- (a) **Follow the data's reading:** cancel refunds; auto-build may queue again. **Recommended**: it is what the data
  implies, and the cancel button's tooltip says "Auto-build will queue again" while auto-build is on.
- (b) **A cancel also switches auto-build off** for that Carrier: friendlier, a recorded deviation that allows more.

## Answer

- **Option (a): Follow the data's reading.** Cancelling an interceptor build refunds it, and if auto-build is on it may queue again on the next tick, with the cancel tooltip noting *"Auto-build will queue again"* while auto-build is enabled (matches StarCraft II 5.0.14).
