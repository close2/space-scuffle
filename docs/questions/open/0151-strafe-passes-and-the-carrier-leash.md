# Q0151: Interceptor strafe passes fly past the Carrier's leash. How should the leash treat a pass?

- **Asked:** 2026-11-28 (plan `2026-12-03-movement-work-on-solver`, WP-I0; evidence
  `docs/evidence/2026-12-03-interceptor-strafe.md` §3)
- **Blocking:** no
- **Default until answered:** (b). A member that has started a pass finishes it before the leash is checked. The leash
  is checked as today between passes and on every member that is not mid-pass. Recorded as a minor deviation.

## Background

In StarCraft II, interceptors attack with a strafe: they fly through the target, out to about 4 beyond it, and turn
back (B-229). Today ours hover in range instead. WP-I1 is planned to build the strafe.

Our Carrier recalls a member as soon as that member is more than 12 from the Carrier, centre to centre (the leash,
`CarrierHangar` `Leash 12`). The Carrier attacks from 8 to its target, edge to edge. So the target's centre is about
9.9 from the Carrier's centre. A pass ends about 4.9 beyond the target's centre, so it can reach up to 14.7 from the
Carrier.

So the leash would cut short every pass that heads away from the Carrier. That includes the first pass of every
launch, because members fly out from the Carrier straight at the target. Only passes that leave at more than about
76° to the Carrier-to-target line stay inside the leash. A cut pass flips the member between "return" and "attack"
every few ticks near the leash circle, and it never completes a loop. The real game does not do this: interceptors
visibly loop beyond targets while their Carrier sits at range 8.

The game data gives the leash value (12) but not what it is measured between. Liquipedia describes it as "how far away
a Hanger Unit, such as an Interceptor, can be away from its Host Unit". It also says "the Carrier can move back to a
leash range of 12 before the Interceptors disengage", which reads like the Carrier-to-target distance. This comes from
web search summaries only, since Liquipedia refused direct reads that day.

## Options

- **(a) Measure the leash from the Carrier to the target** while a member is attacking: members disengage when the
  Carrier is more than 12 from the hangar's target. A returning member is still judged Carrier to member. This fits the
  "move back to 12" sentence. It changes the hangar rule for every Carrier fight, not only for strafing.
- **(b) Let a pass finish (default).** A member in the middle of a pass (its exit point set) is not leashed until it
  reaches the exit. Between passes the leash is checked as today. The change is small, it touches only strafing
  members, and the leash still binds whenever the Carrier moves away. It is not sourced, so it is recorded as a minor
  deviation.
- **(c) Keep the leash as it is**, and let passes be cut. The strafe would then mostly not happen. Not recommended.

Recommendation: (b), because it is the smallest change that lets the pass happen. (a) is the better reading of the
one sentence we have, if you would rather follow it.

## Answer

