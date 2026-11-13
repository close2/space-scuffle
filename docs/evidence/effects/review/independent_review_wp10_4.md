# WP10 independent review, round 4 (the Abduct tether only)

Reviewer: independent; I neither built this work nor reviewed rounds 1 to 3. Scope: `abduct_tether` against A3-1 of
[`independent_review_wp10_3.md`](independent_review_wp10_3.md), the packet section "WP10 round 4" of
[`independent_review_packet.md`](independent_review_packet.md) and plan Amendment 23. Branch `effects-wp10`, head
`6c02b593`. Round 4 of at most five (Q0080). My crops and zooms are in `~/scratch/wp10-review4/`.

## Verdict

| Item | Verdict | Reason |
|---|---|---|
| A3-1 (a) grip and tip | **ACCEPT** | No point and no barbs on either driver. The tongue ends blunt and does not narrow into the victim (13 px 1 px outside the bubble against a 7 px median). At full range the grip is a round, opaque club pressed against the bubble. Grip 16 px, ratio 2.29 on both drivers (0 % apart) |
| A3-1 (b) offset in the grip's unit | **ACCEPT** | `LineEnd { back: 0.2 }` is a fixed distance from the victim's surface, clamped to the line, with a unit test at three lengths, a short line and no line. In the full-range shot the grip touches the bubble with no visible gap |
| A3-1 (c) visible bulges that travel | **ACCEPT** | 4 to 9 px (2.25, against 1.6). The two strips scroll at clearly different speeds (about 1.3 and 0.48 units a second). By my own profile match, the bulges move 6 px (radv, 200 ms) and 7 px (lavapipe, 240 ms) towards the caster, so 2.4 and 2.3 px per 80 ms |
| A2-2 (b) opaque edges kept | **ACCEPT** | 1 px inside each edge: (82, 25, 23) and (90, 28, 26) on radv, (90, 29, 26) on lavapipe, all inside the brief's (45, 12, 12) to (95, 30, 28) |
| P3-4 test | **ACCEPT** | See section 4 |
| Storm, cloud, fog (accepted in round 3) | **unchanged** | `git diff --stat 202063f6 6c02b593` touches no `wp10r3_*` file and no area entry. In `effects.ron` only `abduct_tether` and a new curve `hold_firm` change. The area figures are the same (0 of 194 unseen samples change on both drivers) |
| **Tether** | **ACCEPT** | It reads as a fleshy tongue at 1:1, at 3x and at full range |
| **WP10 overall** | **ACCEPT** | Nothing is reopened. The polish items below are not blocking. P4-1 should be recorded as a bug |

## 1. Does it read as a tongue?

**3x (`wp10r4_tether_{radv,lavapipe}.jpg`).** The round-3 harpoon head is gone. The tongue is now a dark red-brown,
lumpy, muscular cord. Its outline swells and pinches irregularly: major swellings of 22 to 24 px (in the picture's
pixels), with smaller secondary bumps and waists of 6 to 13 px between them. It ends in a broad, flat-ended
section against the victim. Nothing about it reads as a beam (there is no light core and no glow), a rod (the width
varies by more than two times) or a harpoon. The two drivers are almost identical, column for column.

**1:1 (the 3x shot scaled down by 3, `oneone_cmp.png`).** Compared directly with round 3, the bar has become a
knotted, organic cord with a thicker end on the victim. The bulges are visible at this size, which round 3 lacked.

**Full range (`wp10r4_tether_far_*.jpg`).** This is the strongest picture. A wavy, uneven tongue crosses about 250 px
and ends in a round, opaque club on the victim, with a thin neck behind it. That is the silhouette of a chameleon's
tongue, the right reference for Abduct. The grip touches the bubble at about 0 to 1 px.

**Bead chain?** It comes close, but no. The fast strip dominates the outline: in my column profile of the 3x shot,
the major swellings are almost equal (22 to 23 px) and evenly spaced (57 to 63 picture px, about 20 screen px). The
slow strip makes the stretches between them clearly different, though: one is a long flat waist, the next a sharp
notch followed by a small bump. So no three neighbouring segments repeat, and the round-2 condition holds. Still, the
cadence is regular enough to be felt at 3x (P4-2).

## 2. Numbers I checked

- **Grip and tip.** Column counts of opaque red (r − g and r − b both above 35) along the 3x shot. The tongue runs at
  19 to 24 px from 474 to 495 px and ends at the bubble, with no taper. The test's 16 px grip counts every changed
  pixel across the line. That includes a soft, darker, round halo of the droplet card around the opaque end (at
  x = 485 the opaque part spans 21 px, and the halo darkens a further 9 px above and 12 px below). The halo is
  genuinely visible as a swelling, so the metric is fair. But at short range the opaque part alone is only about
  1.4 times the median (P4-4).
- **Travel.** My own profile match agrees with the reported travel. Radv: peaks at 213, 270, 333, 390 and 450 px move
  to 195, 252, 315, 372 and 432 px, which is −18 picture px, or −6 screen px. Lavapipe: about −21, or −7 screen px.
  Both shifts are towards the caster, which is on the left. The lavapipe log line "72 → 66 px" (6 px) disagrees with
  its own "7 px" travel. The 7 px is the best match of the whole profile and the 6 px tracks a single peak, so
  neither is wrong, but the record should say so.
- **Ends.** 0.52 and 0.29 px from the surfaces, on both drivers.

## 3. Is scaling 200/240 ms of travel to 80 ms sound?

Yes, for this effect. Both strips are pure constant-velocity UV scrolls, so displacement is linear in time, and the
scaling is exact as long as the longer interval does not alias. It does not. In 200 to 240 ms the fast strip moves
0.26 to 0.31 units of its 0.8 repeat, and the slow strip 0.10 to 0.12 of its 0.53, both well under half a repeat. The
tracked shift is dominated by the fast strip and agrees with its speed: about 1.3 units a second at about 25 px a
unit gives 6.5 px over 200 ms. A direct 80 ms check is not possible from the sequence sheet: its tiles are close to
1:1, where 2.5 px is lost in the JPEG. One caveat about the test itself is P4-3.

## 4. Code

- **`PlacementDef::LineEnd { back }`** (`crates/client/src/effects/placement.rs`) places the instance at
  `dir * (length - clamp(back, 0, length))`. It is correct and simple. A missing line falls back to the anchor,
  consistently with `Point`. The resolver requires a line that binds `LineLength`. The unit test covers lengths of
  1.0, 3.6 and 9.4, a line shorter than `back` (clamped to the start), no line, one instance, and visibility. The
  extraction of `placement_values` in `resolve.rs` preserves behaviour.
- **P3-4 test** (`a_tether_whose_end_leaves_sight_is_removed_at_once`). The tether is held over three frames. On the
  frame its `to` end stops being shown, `looks.held()` is (0, 0) and the pool is empty. An empty pool means nothing
  can be drawn, so this pins the behaviour round 3 asked about. Only the `to` end is exercised (P4-5).
- **GPU test.** It adds the no-narrowing, grip-ratio, bulge-ratio and travel assertions, and it requires the
  full-range shot. That shot asserts only that a tether exists, not that its length is Abduct's range of 9 (P4-5).

## 5. Polish and advisories (none blocking)

- **P4-1 (record as a bug).** When the victim arrives and the two bubbles overlap, a red wad is painted over the
  units. In the sequence on radv, the second tether shows it in frames 9 to 12 (about 160 ms), lying on the victim's
  far side, and the first tether at frame 24. The grip, clamped to the line's start, and the caster's swelling are
  both larger than the gap. When the surface points cross, the line turns round, which puts the grip on the far side
  of the victim. Suggested fixes: fade or shrink the grip and swelling once the line is shorter than the grip's
  diameter, or end the look when the bubbles touch.
- **P4-2 (taste).** The fast strip's swellings are equal in width and evenly spaced, and only the slow strip breaks the
  pattern. Varying the scale per repeat, or giving the slow strip a little more amplitude, would make it read less as
  a knotted rope and more as muscle.
- **P4-3 (test).** The travel search looks only at shifts of 0 to 16 px towards the caster, and the fast strip's
  period is about 20 px. A 6 px move the wrong way would match as 14 px towards the caster, if the slow strip did not
  disambiguate. Searching both directions (−16 to +16) and asserting a positive best match would make the direction
  check robust.
- **P4-4 (taste).** At short range, the grip's extra width is mostly a translucent halo. A slightly firmer grip curve,
  or a slightly larger grip, would make the opaque end read as clearly as it does at full range.
- **P4-5 (tests).** Cover the `from` end leaving sight as well. Assert that the full-range tether's world length is
  within a tolerance of 9.
- **P3-3 stands.** The edges are still hard and aliased at 3x.

## Conclusion

A3-1 is met in all three parts on both drivers, and the tongue now reads as a tongue: blunt, organic and visibly
travelling, and at full range a convincing club-tipped lash. **Tether: ACCEPT. WP10: ACCEPT.** P4-1 should go into
the bug register before the merge. The rest is polish.
