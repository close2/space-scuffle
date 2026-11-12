# Independent Review of WP8 (Concealment and Stance), Round 5

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md),
Amendment 15. Packet: [`independent_review_packet.md`](independent_review_packet.md), section "WP8 round 5".
Previous round: [`independent_review_wp8_4.md`](independent_review_wp8_4.md), which accepted every look and required
E4-1, with A4-1 and A4-3 advisory. Owner rule Q0080: this is round 5, the last of five. It is engineering only. I did
not take part in building this work or in rounds 1 to 4.

## Method

I read review 4, the packet's round-5 section, Amendment 15, the measurements in
[`wp8r5_measurements.txt`](wp8r5_measurements.txt), and the round-5 diff `ef05abd..ce47b86`. In the code I read
`queue_effects` and `cut_runs` in `crates/client/src/effects/render.rs`, the frame upload that sets
`gpu.darkening_cards`, and in `crates/client/src/effects/runtime.rs` `push_sorted_card`, the particle card writer,
`sort_cards`, the card writer that counts `darkening_cards`, `DARKENING_DEPTH` and `sort_depth`. In
`crates/client/tests/stance_gpu_tests.rs` I read the `SiegeMid` staging and the back-of-furrow assertions. I ran the
unit tests of `effects::render` and `effects::runtime` (`cargo test -p client --lib`, see §5). I ran no GPU test and
no gate.

## 1. E4-1: ordinary card runs leave the darkening prefix out: done

- **Cut range.** `queue_effects` now feeds `cut_runs` with
  `gpu.centers.iter().skip(prefix).map(distance_translation)` and `start = prefix`, where
  `prefix = gpu.darkening_cards`. `cut_runs` adds `start` to each enumerated offset, so every range it writes indexes
  the card buffer at `prefix..card_count`. The prefix's distances never reach the cursor `below`.
- **Offsets, no off-by-one.** A card at offset `o` gets index `o + start`, and a run ends at `index + 1`. The first
  ordinary card opens run `prefix..prefix + 1`. The last ends at `card_count`, because `centers` holds one centre per
  sorted card (it is rebuilt from `frame.sorted` on the same upload). The darkening item draws `0..darkening_cards`,
  so the two pass ranges meet without gap or overlap. `gpu.darkening_cards` is clamped to `card_count` on upload, so
  `skip` cannot pass the end.
- **Queued distance.** The loop reads `gpu.centers[range.start]`. Since `range.start >= prefix`, the first run is
  queued at the first *ordinary* card's distance, as E4-1 asked.
- **Empty prefix.** `skip(0)` and `start = 0` reduce to the round-3 behaviour exactly. The existing `cut` tests now
  call `cut_runs(.., 0, ..)`, and they pass unchanged.
- **All cards darkening.** The iterator is empty, `runs.cards` is cleared and stays empty, and no ordinary card item
  is queued. The darkening item still draws the whole buffer. That is correct: there is nothing ordinary to draw.
- **Other kinds unaffected.** Decals, the darkening decal and card items (all at `f32::MIN`) and distortions
  (`queue_distortions`) are unchanged. Particles are ordinary cards: they carry no `FLAG_DARKEN`, sort after the
  prefix, and are cut as before.
- **Overflow.** `index + 1` could overflow only at `u32::MAX` cards. The card buffer's capacity is far below that.

**Does `the_darkening_prefix_stays_out_of_the_ordinary_runs` test what it claims? Yes.** I traced it by hand. With
`cut_after(1, [-1, -10, -9, -8], others [-9.5, -8.5])` the cut sees −10 (no crossing, run `1..2`), −9 (crosses −9.5,
run `2..3`) and −8 (crosses −8.5, run `3..4`). That gives `[1..2, 2..3, 3..4]`: no run contains 0, and the result
equals the prefix-free cut shifted by one. The last assertion reproduces the round-4 defect. Cut with the prefix,
−1 first moves the cursor past both others, so the three later cards see no crossing and join run `0..4`. The test
therefore fails on the old call and passes on the new one. It is the case review 4 asked for.

One limit (advisory A5-1): the helper `cut_after` copies the queue's call shape (`skip(prefix)` plus
`start = prefix`) rather than calling `queue_effects`. If someone later removed the `skip` from the queue, this test
would still pass. Factoring the prefix handling into one helper that both the queue and the test call would close
that gap. It does not block: the queue code is three lines, and I checked it directly.

## 2. A4-1: finite sort depths: done, sound

- `push_sorted_card` gives a darkening card `DARKENING_DEPTH = f32::MAX`. Every ordinary card, and now every
  particle card, goes through `sort_depth`. That returns `depth.min(f32::MAX / 2)` for a finite depth and `0.0`
  otherwise. Under the descending `total_cmp` sort, NaN, +∞ and huge finite depths can no longer reach or pass the
  prefix. −∞ and huge negative values were never a threat (they sort last), and now become 0 or stay finite.
- **Can a legitimate depth equal or exceed the darkening depth?** No. The view depth is
  `(point − camera).dot(forward)` in world units: tens to hundreds for this camera, minus a small `sort_bias`.
  `f32::MAX / 2 ≈ 1.7e38` is never reached by a real scene, and the clamp keeps even a pathological finite value
  strictly below `f32::MAX`. The count `darkening_cards` therefore equals the length of the sorted prefix.
- **Is 0 sound for draw order?** Yes. A card with a non-finite centre has no meaningful screen position. Its
  vertices are NaN or infinite and are clipped or degenerate, so where it sorts among the ordinary cards changes
  nothing that can be seen. 0 (the camera plane, drawn last among cards in front) is a harmless, deterministic place.
  On the renderer side, its NaN `distance_translation` never satisfies `others[below] < distance`. It just joins the
  current run and does not disturb the cursor for the cards after it. The test
  `no_card_sorts_before_a_darkening_card` covers NaN, ±∞, `f32::MAX`, 1e30 and an ordinary −3.0, and checks that an
  ordinary depth is kept.

## 3. A4-3: back-of-furrow negative control: convincing

- **The move.** `BEHIND_DISTANCE` goes from 1.62 (on the furrow line) to 1.3 world units behind the hull. The radii
  sum to 1.25, so the worker stands clear of the hull and `spawn_still` does not push it. The body now rises over the
  band on screen. The amendment's reasoning is right: on the line, the body hardly overlapped the band, and the
  implementer reports that a first control run there passed even with the old order. That is the weakness A4-3
  suspected, and they found it themselves.
- **The control.** It was a one-off radv run with the darkening card item queued at its card's own distance, which
  is the order before E3-1 and so the failure this check exists to catch. Result: "18 of 214 body pixels keep less
  than 0.85 of their light (lowest 0.332)". The threshold is 5 % of 214, about 10.7 pixels, so the test fails (exit
  101, the panic message of line 418 is recorded). With the order of record: 0 of 214, lowest 0.891 (radv) and 0.892
  (lavapipe). A lowest value of 0.332 against 0.89 is an unambiguous multiply over the body, not noise. The control
  separates the two orders cleanly. The temporary code was not kept, as review 4 allowed ("a note in the evidence is
  enough").
- **Was anything weakened?** No. The other assertion on this unit, `ground_round > 0`, is still met with margin: the
  furrow darkens 94 (radv) and 96 (lavapipe) pixels round its foot, against 78 in round 4. The worker exists only in
  the `SiegeMid` shots. Every other measured figure in `wp8r5_measurements.txt` matches round 4 within capture noise:
  the patch "empty vs burrowed: 0", the shimmer "0 to a colour not near them", "undetected burrowed enemy: 0", the
  anchor "darken 1946 / 1968, lighten 0 of 8163", the 4.5 px (0.19 radii) fade, the 1.36 s / 77 % channel, the
  57.0 / 57.2 % ring share with 0 beyond it, the shadow "0 burrowed", and the arc "0.472 → 0.450". The body
  thresholds `BODY_KEPT = 0.85` and `BODY_DARKENED_SHARE = 0.05` are unchanged.
- The measurements file states a debug build under Xvfb on both drivers. I did not rerun the GPU tests (out of scope
  for this round), so these figures are the implementer's printed output, which I checked for consistency.

## 4. No accepted look changed

`git diff --stat ef05abd ce47b86` touches six files: `effects/render.rs`, `effects/runtime.rs`,
`tests/stance_gpu_tests.rs`, the packet, the plan, and the new `wp8r5_measurements.txt`. Nothing under `assets/`
changed (no catalog, `effects.ron`, shader or flipbook). No golden, lab PNG or earlier evidence capture changed. In
`docs/evidence` only the packet section and the wp8r5 measurements are new. The code changes affect only the queue's
run ranges and the sort keys of ordinary cards. Finite depths below `f32::MAX / 2` keep their exact value, so the
order of every real card is unchanged. The shipped splash effects' draw order is now that of round 3 whenever no
darkening card exists, and it is also correct while one exists, which is what E4-1 restores.

The polish items P4-1 and P4-2, the advisory A4-2 and the burrow crest are recorded in the plan's open polish list
rather than taken. That is acceptable: they were non-blocking.

## 5. Tests run

`cargo test -p client --lib -- effects::render effects::runtime` (debug, `CARGO_TARGET_DIR` on scratch, under the gate
lock): 13 passed, 0 failed, including `the_darkening_prefix_stays_out_of_the_ordinary_runs`, the existing `cut_runs` tests and `no_card_sorts_before_a_darkening_card`.

## Verdict

| Item | Verdict | Reason |
|---|---|---|
| E4-1 (required) | **done** | Ordinary runs are cut over `prefix..card_count` only, ranges offset by `prefix`, first run queued at the first ordinary card; empty and all-darkening prefixes correct; the test fails on the old call and passes on the new one |
| A4-1 | **done** | Ordinary and particle sort depths are finite and ≤ `f32::MAX / 2`; non-finite → 0 is harmless; unit test |
| A4-3 | **done** | The worker moved to where the body overlaps the band; the recorded control fails 18/214 (lowest 0.332) against 0/214 (lowest 0.891); no other assertion weakened |
| Looks and shipped effects | **unchanged** | No asset, shader, golden or capture in the diff |
| **Overall WP8** | **ACCEPT** | Every look was accepted in round 4; the one required engineering fix is correct and tested; both advisories are addressed |

## Advisory (not blocking)

- **A5-1.** `the_darkening_prefix_stays_out_of_the_ordinary_runs` tests `cut_runs` through a helper that copies the
  queue's `skip(prefix)` and `start = prefix`, not the queue itself. A small shared function, such as
  `cut_ordinary_runs(centers, prefix, distance, others, runs)`, called by both, would make the test guard the queue's
  wiring too.
- **A5-2.** The A4-3 control is recorded as a one-off. If the back-of-furrow staging changes again, rerun the control
  before trusting the check.
