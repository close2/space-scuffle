# Q0137: Keep the corner hand-off fix (B-240), although one showcase ramp gets slightly worse?

- **Asked:** 2026-11-26
- **Blocking:** no
- **Default until answered:** (a) keep it merged

## Background

Units cut corners at walls partly because a route's corner was "handed off" (skipped) while the leg to the next
corner still grazed the wall (B-240). The fix (plan `2026-11-29-wall-clearance`, WP1) skips a corner only when the step
itself would allow the next leg.

- **It works:** counted at the moment of the decision, corner-cutting hand-offs fell from 1,586 to 0 in the crowd
  test scenes (large groups through chokepoints, corners and hairpins) and to 0 on every showcase run.
- **Its literal target was a little stricter:** it also counted legs read one step later. Those fell from 3–4 to 1 per
  showcase and from 7 to 2 on the effects showcase; they belong to the next bug, B-241, which stays open.
- **Mixed side effects:** on the effects showcase, the west ramp gets a little worse (more direction reversals,
  22.7 → 28.5), because the march splits differently from tick 192 on. Other areas improve. In the crowd scenes, about as
  many figures got slightly worse as better, mostly by under one reversal per scene. No battle winner changes, and
  all hard checks hold.
- The second fix (B-241, re-checking a held leg) made two zig-zag tests fail and is parked until the movement redesign
  has changed how friends push each other.

## Options

- **(a) Keep the fix (default).** The defect is gone; the side effects are small and mixed. B-241 and the ramp come
  back with the redesign and WP3.
- **(b) Revert it** until WP3 can fix corner hand-offs and route following together.

Recommendation: (a).

## Answer

- **Option (a): Keep the corner hand-off fix (`B-240`) merged.** Cutting corner-cutting hand-offs from `1,586 → 0` in the large-crowd chokepoint/corner/hairpin scenes and to `0` on every showcase run is a clear root-cause fix; minor mixed secondary shifts are acceptable while `B-241` and the ramp wait for the movement redesign and WP3.
