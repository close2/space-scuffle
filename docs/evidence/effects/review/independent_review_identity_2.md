# Independent Review of Identity WP5 and WP7 (Named Status Looks, Held Beams)

Plan: [`docs/plans/2026-11-17-effect-snapshot-identity.md`](../../../plans/2026-11-17-effect-snapshot-identity.md), D-4,
D-5, D-9, WP5 and WP7, Amendments 4 and 5. Packet: [`independent_review_packet.md`](independent_review_packet.md),
section "Identity WP5/WP7". Branch `effects-identity-looks`, diff `345da93f..caa0738c`. Review of Q0080 (D-9). I did
not build this work.

## Method

I read the charter, Q0080, D-4/D-5/D-9, the WP5/WP7 sections and Amendments 4 and 5, and the packet section. I read
in full `effects/beams.rs`, the changes to `effects/stance.rs`, `effects/systems.rs`, `effects/struck.rs`,
`effects/catalog/{mod,schema,resolve,units,error}.rs`, `battle_pipeline/timeline.rs` and `assets/catalog/effects.ron`,
and the new or changed tests in `battle_fog_tests.rs`, `effect_catalog_tests.rs`, `weapon_look_tests.rs` (names and
bodies of the six beam tests), `sync_allocation_count_tests.rs` and the removed lines of every test file (to find
changed bars). I checked `EffectRuntime::{hold, release, retire, update_held_line, set_fog_clip}` and the pool's
generation handles, and the battle clock's speed handling (`battle_pipeline/clock.rs`). I looked at the four
pictures, and cut and enlarged (nearest neighbour, 2x to 4x) a late Fungal frame and the window's middle band.
I ran `cargo clippy -p client --all-targets -- -D warnings` and `cargo test -p client --lib` (results below). I ran no
GPU test and no `cargo gate`.

For the game's presentation I used what is known of StarCraft II: the Void Ray's prismatic beam is one continuous
beam that stays locked on its target, intensifying while it is held, with a bright contact point on the target and a
glow at the ship's prism; Fungal Growth bursts green-yellow on the ground and leaves its victims covered in a green
spore growth for its duration (Liquipedia, 5.0.x: 30 damage over about 3 s, 75 % slow, reveals cloaked and burrowed
units). The engine's beam visual does not pulse per attack in a way a player reads; its life comes from the flicker of
the beam and the contact flare.

## Verdict table

| Item | Verdict | Main reason |
|---|---|---|
| Held beam slot lifecycle (refresh, retarget, release on death or deadline, session change) | **met** | One slot per `(attacker, EffectSource)`, binary-searched, insert within reserved capacity; refresh moves the target and deadline; `keep` releases on a missing bubble or passed deadline; a mode change fades the old look; `follow` retires everything on a new session; generation handles make stale retires harmless |
| Slots running out | **met as specified; R2-2** | The 65th beam is refused and counted; it then draws no travel at all, only its per-strike burns |
| Fog cut every frame | **met** | `keep` recomputes `shot_clip` on the new line each frame for another player's beam, `NO_CLIP` for the viewer's own; `keep_beams` runs after the event wiring on the same frame |
| Allocation and search (§3.3) | **met** | Both `Vec`s are reserved at 64 and guarded before `insert`/`push`; per slot two O(log n) searches; per bubble `statuses_of` is two `partition_point`s and the named loop is O(k·4). Valgrind effect stream 0, baseline 0 |
| Data (C5) | **met** | No status, unit, weapon or effect name in production Rust. The only literal added in `crates/` is a test label; tests find the beam unit, the Fungal status and the storm stand-in by data |
| Loader rules | **met** | `sustain` only on `Shot` (`InvalidValue`, `sustain`); grace positive and finite (`grace`); held exit ≤ `MAX_SUSTAIN_EXIT` 0.1 s (`exit`); a held layer on an unsustained `Shot` refused. Shipped beams come in over 0.04 s and erode over 0.08 s. `status_by_name`: unknown status → `UnknownStatus`, non-`Status` look → `WrongEventKind` (slot `status_by_name`), both tested |
| Named status looks (≤ 4, fog rule) | **met** | Four slots, a fifth counted per frame (tested); released with all other held looks when presence is not `Shown`; membership from the same snapshot as the bubbles (`discrete_source`) |
| Charter rules | **met** | No `unwrap`, `todo!` or marker comments added; module and item docs present; typed errors; test-module `#[allow]`s carry reasons; clippy clean (below) |
| Tests prove the claims | **met, one bar moved with cause (R2-4)** | Unit, integration, GPU and valgrind tests cover every claim of Amendments 4 and 5. The valgrind `Nowhere` bound moves from 0 to the planted dead-target count, explained and exact |
| Amendment 4 fog test | **sound** | See R2-5 |
| Amendment 5 deviations | **accepted** | See R2-6 |
| Fungal affliction look (point 1) | **accept** | See "Look judgements" |
| Held beam look (point 2) | **accept with a condition (R2-1)** | Steady body is right; it needs a per-strike accent at the contact point |
| **Overall** | **ACCEPT WITH CONDITIONS** | The code is sound and the measurements prove the claims. One look condition (R2-1) and one behaviour to record (R2-3) |

## Look judgements

### Fungal affliction (packet point 1)

In the late frames of `identity_wp5_fungal_own_radv.png` (row 3, enlarged 4x) each victim's bubble carries a clear
thin yellow-green rim over the green haze of the ground, and its body is tinted. The slow's grey `concussive_daze` is
not visible beside it: both are shell rims, and the measurement confirms the rim takes the Fungal colour on every
frame (0.30 to 0.80). That is the right priority: in the game the slow is part of Fungal Growth and the player reads
"fungaled", not "slowed". No unit outside the Fungal circle in the window picture (the storm stand-in's victims
among them, rim 0 on all 41 frames by measurement) shows any green, as required. It reads as Fungal Growth at 1:1. **Accept.**

Notes, not conditions:

- Every frame of the sheet still has the ground stain under the victims, so the rim alone (after the stain fades,
  for the rest of the status) is not shown at 1:1. At 1:1 the rim is about 2 px on a 20 px bubble; with the stain
  gone it is weaker than the game's spore growth covering the model. The rising motes are not distinguishable at 1:1.
  A later round's packet should show a frame after the stain is gone.
- The window picture shows, right of the station, a green patch with straight, axis-aligned edges (about x 975–1035,
  y 310–380): the fog-edge Fungal of E1-3 cut along fog cells. It is outside this diff (WP10/WP6's fog cut, proven by
  "0 unseen changed"), but a hard rectangular cut reads as a rendering fault. Recorded for the area-look work.

### Held beam (packet point 2)

In `identity_wp7_held_beam_radv.png` the mid-stream beam is a clean violet tube from the attacker's glass to the
target's, with a thinner bright core; its end sits on the target's rim, and three frames after the death nothing of
it remains (drawn and bare match). It tracks a walking target without lag (0.00 px over 81 px of travel). That is a
correct channelled ray, much better than the old splaying per-strike beams.

But held steady, it is flat: at 1:1 and at 2x the tube is one uniform value along its length, there is no visible
contact flare on the target and no prism glow at the muzzle in the sampled pair. The Void Ray's beam reads alive by
its flicker and its bright contact point, not by travelling pulses. A pulse riding the beam on every strike (every
0.36 s) would be busier than the game and would look like a stream of projectiles; it is the wrong fix. **Steady
with onset pulses is the right structure**, provided each strike gives the contact point a short bright swell (the
per-strike `beam_burn` is the right place; it is not visible in the sampled pair) and the held core keeps a visible
flicker. See R2-1.

No background or effect in the pictures uses rays, stripes, sunbursts or speed lines (owner's rule).

## Findings

### R2-1 (condition, look): the steady beam has no per-strike accent at 1:1

`assets/catalog/effects.ron`, `prism_beam` and `lance_beam` (held body, core and muzzle layers). Scenario: a Void Ray
analogue channels for several seconds; after the 0.43 s of onset pulses the player sees an unchanging tube with no
point of contact, which reads as a static line, not an attack. Severity: medium (Q0080 "no cheap looking effects").
Change: make the per-strike impact (`beam_burn`/`lance_burn`) a visible flare on the target's rim at 1:1 (a short
bright swell, about 0.1 to 0.15 s), and/or give the held core a modest brightness flicker; show it in the next
packet at a frame just after a strike and one just before the next.

### R2-2 (low, record): beyond 64 held beams, a beam is invisible

`crates/client/src/effects/beams.rs:252-255`. A strike refused for a full table returns `true`, so no travel is drawn
for it. Scenario: more than 64 sustained-beam weapons firing at once (both players' Void Ray and Colossus analogues
in a 400-unit battle, or a unit with two sustained weapons counting twice): the 65th attacker shows only burns for as
long as the others hold. The refusal is counted and a held layer cannot be spawned as a one-shot, so no fallback
exists; this is acceptable, but `HELD_BEAMS` should be argued from the catalog (beam units × max supply) in the plan
or sized from the effect budget, and the refusal surfaced in the diagnostics overlay like `AreaLooks::refused`.
Related: when `EffectRuntime::hold` returns `None` (`beams.rs:264-282`) the strike is also taken, draws nothing, and
is not counted by `HeldBeams`.

### R2-3 (low, record): beams vanish at battle speed 0

`beams.rs:224` and `keep` (line 310). The deadline is in `Time<Virtual>` seconds, while the strikes come from the
battle clock, which supports speed 0 (`battle_pipeline/clock.rs:112-116`). Scenario: the player pauses while a Void
Ray channels; the presented frame freezes, no strike comes, and 0.48 s later the beam is released and erodes out,
although the frozen attacker is still channelling; on resume the next strike re-holds it and replays the onset
pulses. Held status and area looks do not vanish on pause, so the beam is the only held look that does. Either take
the deadline in presented ticks, or record the behaviour in Amendment 5. The same holds for a presentation hitch longer
than the grace.

### R2-4 (info): the valgrind `Nowhere` bound

`sync_allocation_count_tests.rs`, `check_effect_stream`: the bound moves from 0 to
`(WARM_FRAMES + frames) / 7 + 1`, because the stream now plants a beam at an unpresented entity every seventh frame.
The count of planted strikes is at most that bound, so no other unfound strike can hide in it beyond one. Justified.

### R2-5 (info): Amendment 4's fog test is a sound replacement

The plan's case (an undetected cloaked enemy under Fungal Growth) cannot occur with the shipped data, because
`fungal_growth` reveals (`reveal: true`). `a_named_status_look_shows_only_on_a_unit_the_viewer_sees` instead stages
the case that can occur and checks its truth: the cloaked victim is published `Concealment::None` and keeps its look;
moved out of sight it keeps its published status and has no named look. The bubble lookup (`bubble`) panics if the
bubble is missing, so "no look" is not a trivial pass. The undetected (`Shimmer`) and `Hidden` paths are covered by
`a_unit_the_viewer_does_not_see_as_it_is_holds_no_named_look`, which exercises the same branch
(`presence != Presence::Shown`) a future non-revealing named status would take. Sound.

### R2-6 (info): Amendment 5's deviations

- *A released beam ends after its exit*: correct; otherwise the 0.43 s onset pulses would outlive a target that died
  early. The fading list is bounded (a full list retires at once). Accepted.
- *Only beams are held*: a hitscan tracer is a per-shot streak in the game too; holding it would be wrong. Accepted.
- *The beam starts at the attacker's bubble*: a flyer's beam now leaves its own glass (unit test). This matches the
  game's Void Ray, whose beam leaves the ship. Accepted; the projectile launch from a flyer (open from Amendment 3)
  remains.

### R2-7 (info): a strike with no travel line holds a beam on a zero line at the origin

`beams.rs:256`. `keep` moves it onto the bubbles on the same frame, or releases it if either is not presented; in
the latter case a zero-length instance at the world origin erodes for 0.08 s. Not visible in practice; a `return
true` without holding would be cleaner.

## Cargo results

- `cargo clippy -p client --all-targets -- -D warnings`: clean for `client` (the only warnings are the vendored
  `third_party/naga`'s, which `-D warnings` does not apply to as a dependency).
- `cargo test -p client --lib`: 464 passed, 0 failed.

## Summary

**ACCEPT WITH CONDITIONS.** WP5 and WP7 do what Amendments 4 and 5 say, without allocation, quadratic search,
catalog identities or charter breaches, and the tests prove it. Condition before WP8 closes B-126: R2-1 (a per-strike
contact flare or flicker on the held beam, shown in the next packet). R2-2 and R2-3 are to be recorded (plan or
register) or fixed. The Fungal affliction is accepted.
