# Evidence: Free Portal Vertices in the Radius Funnel (B-146)

Measurements for plan [`2026-11-19-partial-knowledge-funnel`](../plans/2026-11-19-partial-knowledge-funnel.md). Each
work package appends its section. The plan's prototype figures (its §4) were taken on `3c2f4c65`; the figures here
are taken on the implementation tree and its base, named in each section.

## WP0: records

- `DESIGN.md` §4: B-146 restated with the plan's diagnosis and pointed at this plan; the TA* budget item the plan
  drafted as B-147 is registered as **B-149** (B-147 and B-148 went to plan `2026-11-19-footprint-flags`), open.
- Plan `2026-11-19-funnel-fallbacks` Amendment 3 points its §9 B-146 row here.

## WP1: regression cases, red on `main`

Six cases, each `#[ignore]`d with the work package that fixes it. Run with `--include-ignored` on the WP1 tree
(`main`'s funnel at `bf4252cf`), every one fails:

| Test | Case | On `main` |
|---|---|---|
| `a_route_crosses_an_unknown_footprint_without_zigzag` (WP2) | N1, r 0.5, 0.75, 1.0 | r 0.5: the free turn (9, 11) is rounded by an arc, not emitted as a waypoint |
| `an_unknown_neighbour_makes_no_pinch` (WP2) | N2, r 0.5 | the zero-room retry |
| `open_ground_is_not_a_gap` (WP2) | N3, r 0.5 | the zero-room retry, eight turns |
| `a_free_end_inside_the_clearance_is_clamped_out` (WP3) | N4, r 0.75 | a portal-midpoint fallback (the plan's table says "legal but 21.03 long"; on a flat 24 × 24 map, as the test builds it, `main` falls back) |
| `a_free_end_across_a_narrow_portal_is_passed_on_its_far_side` (WP3) | the corpus request (42.75, 49.5) → (26.25, 19.5) at r 0.75 and 1.0 | a fallback |
| `golden_corpora_fall_back_only_from_wedged_starts` (WP3) | both golden path corpora | 8 and 13 fallbacks from fitting starts |

**The far-side layout (plan WP1, "verify").** The anchors `R(35, 48.5) L(35, 46.5)[FREE] R(34.5, 46)` of §3.5 are on
the showcase, and every vertex there belongs to one structure: the corpus's 5 × 5 contour (`5, 10, 6`) at cell
(30, 45), known per cell. Its chamfer corner (35, 46.5) is free (its cells are unknown) and (34.5, 46), where the
chamfer crosses the line between the known cell (34, 45) and the unknown (34, 46), is an obstacle point; the other
anchors are the showcase's ramp and cliff corners. So the test is the showcase with that one structure and the
corpus's 25 knowledge bits, at the corpus request. At the corpus's own radius 0.625 the two points are 0.707 apart,
more than `rho` 0.675, so the free corner is a plain D-1 point bend there (the prototype's adopted build turns at
it); the far-side rule applies from r 0.75 (`rho` 0.8), which the test uses, with r 1.0. On the prototype both radii
fall back with the rules off, and route without an anchor at (35, 46.5) with D-1 alone and with D-1 to D-3.

**N4's expected length.** The plan gives 15.614 for N4 under D-1 to D-3. The prototype diff
(`~/scratch/pkf/patches/proto.diff`) with its adopted switches (`PKF_POINTS`, `PKF_WIDTH`, `PKF_CLAMP`, `PKF_NEAR`)
reproduces both of the plan's would-be pins (`0x4062_b0b6_83dd_1afb`, `0x1da8_6365_bcbd_3b37`), so it is the adopted
design, and it routes N4 in 15.5103 (turns at the known (13, 10.5) and (13.5, 12)), on 24 × 24 and 28 × 28 alike.
The test expects 15.510.

The golden counting test shares each corpus's generator with its digest test (`path_corpus`,
`contour_path_corpus`); both digests are unchanged by that refactor. The unit tests `free_vertices_are_points` and
`the_clamp_clears_every_disc` exercise the free list and the clamp, which do not exist before WP2 and WP3; they land
with those work packages.
