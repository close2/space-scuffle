# Q0112: The attacker snapshot: the minimap, how long it stays, and concealed attackers

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** 1(a), 2(a), 3(a), as built by plan `2026-11-24-attacker-snapshot`

## Question

In StarCraft II, a ground unit (or an armed structure) that shoots your units from somewhere you cannot see, such as
high ground or fog, leaves you a *snapshot*: a dimmed, still image of the attacker where it fired from. You cannot
target it and it gives no vision. Air units and the Colossus are different: they become fully visible and targetable
for 3 seconds (already built). Plan `2026-11-24-attacker-snapshot` builds the snapshot for the battlefield view (bug
B-183). Three details are not settled by the game data or by any source I found.

**1. The minimap.** The game also shows the snapshot on the minimap. Space Scuffle has no minimap: it was left out
in Phase 5 (Q0002).
- (a) Close B-183 for the battlefield view and keep the minimap half as a new bug, B-204, until a minimap is built.
  The simulation already publishes everything a minimap would draw. **Recommended.**
- (b) Build a minimap now, as its own plan, and draw the snapshot on it.

**2. How long the snapshot stays.** No duration is in the game data. Map-editor forum posts say a snapshot stays
until you see that spot again, or see the attacker itself.
- (a) Until you see the spot or the attacker; a later shot from the same attacker moves it. **Recommended**; it
  matches those posts.
- (b) A fixed time after each shot (for example 3 seconds, like the air units' reveal), and then it fades.
- (c) Like (a), but at most a fixed time.

**3. Cloaked or burrowed attackers.** No source says whether a cloaked or burrowed attacker leaves a snapshot.
- (a) Only if you detect the spot it fires from (with a detector in range). Otherwise you see nothing, as you
  would see nothing of it in plain sight. **Recommended**; it reveals nothing hidden.
- (b) Always, as for any other attacker.
- (c) Never.

Every option changes presentation only: no battle outcome, replay or checksum moves.

## Answer

- **1(a):** Close `B-183` for the battlefield view and track the minimap half as `B-204` until a minimap is built.
- **2(c) — start with a 1-second cap:** The snapshot stays until you see that spot again or see the attacker itself, **capped at 1 second** after each shot (a later shot from the same attacker refreshes/moves it). Starting with 1 second keeps mobile high-ground attackers from leaving stale ghosts behind and can be tuned easily later since it is presentation-only.
- **3(a):** Cloaked or burrowed attackers leave a snapshot **only if you detect the spot they fire from** (with a detector in range); otherwise you see nothing.

## Resolution

Built on branch `snapshot-cap` by plan `2026-11-24-attacker-snapshot` (WP1 to WP4 on the defaults, Amendment 1 for 2(c)).

- **1(a):** as the plan built it: B-183 is fixed for the battlefield view and the minimap half is B-204.
- **2(c):** commit `c6b2de8c`. A snapshot ends when its player sees its spot or its attacker, and at the latest
  `attack_snapshot_cap_ticks` after the strike that made or last refreshed it; a later strike by the same attacker
  moves it and restarts the cap. The cap is authored in `assets/catalog/units.ron` as `Some(25)` (1 second), read at
  run time, documented there, and refused at load when 0 (`SimCoreError::CatalogInvalidValue`); `None` restores the
  game's "until seen". No checksum, golden or schema value moved. It is a difference from StarCraft II 5.0.14, which
  names no duration, chosen by this answer.
- **3(a):** as the plan built it: a concealed attacker leaves a snapshot only where the struck player detects it.
