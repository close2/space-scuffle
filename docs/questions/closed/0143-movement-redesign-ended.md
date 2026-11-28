# Q0143: The movement redesign failed its checks twice. Keep today's movement, or try something else?

- **Asked:** 2026-11-27 (plan `2026-11-27-gated-movement` §6.6 rule 6; drafted there as Q0126)
- **Blocking:** no
- **Default until answered:** (a). Today's movement (`Solver`) stays as it is, and the crowd fix stays off.

## Background

The redesign you approved (Q0120 (a)) was built in stages:
- Its contact layer (WP1) merged switched off. Under its own checks it held every hard rule, including the ones you
  gave on 2026-11-29: Hold Position units never move, attacking units are nudged by allies only within weapon range,
  and enemies never push each other. Friend overlap fell from 43.6 % to 13.7 % in battle 26 and from 49 % to 10 % in the
  pulse clash, at up to 1.65× today's cost on the 400-unit benchmark.
- Its second stage (WP2), the "gated intent" that lets a unit wait instead of pushing into a friend, failed its first
  check twice. It is the zig-zag measure: how often units reverse their own step.
  - **First attempt:** in the zergling battles 26 to 29, reversals rose from 0.2–0.5 per 1,000 moving ticks to
    1.1–2.8, and nine movement-test families got worse.
  - **The diagnosis:** the gate's own decisions caused at most 4 % of the reversals. Most came from an older steering
    rule that still bent units round standing neighbours the gate already handles.
  - **The amendment** used only rules the project already had: that steering rule limited to moving neighbours, a
    delay before trying the other side, a contact tolerance, and friends given the same order treated as moving.
  - **Second attempt:** the amendment cut the failures from 16 figures to 5, but battle 26 (0.88 against at most 0.64)
    and battle 29 (zerglings 0.34 against at most 0.26; stalkers 1.43 against at most 1.36) still exceed today's
    upper bound, and so does the gap family.

As agreed, no third design round starts without you. Evidence: `docs/evidence/2026-11-27-gated-movement.md` §12–§14.
The branch `gated-wp2` is kept.

The crowd fix you approved in Q0118 (C-3′, merged switched off) did not pass its own checks when switched on under
today's movement:
- one battle's worst overlap stayed at 11 % (target 10 %);
- units stood with room in two battles;
- three battles moved without a reason in their traces;
- it costs +47 % on the 400-unit benchmark (about 56 ticks per second on the canyon map, above your 50).

## Options

- **(a) Keep today's movement (default).** Crowds overlap up to about 40 % (B-219 open). Allies can push attacking
  units beyond the small nudge (B-225 open). The movement work that waited for the redesign (wall clearance WP3,
  B-241, interceptor strafing B-229, colossus walk-under B-209) is re-planned against today's movement.
- **(b) Switch the crowd fix C-3′ on** despite its failed checks.
- **(c) Ship the redesign's contact layer without the gated intent.** It enforces your pushing rules and cuts
  overlap, but its zig-zag measure and acceptance were never run on their own. It would get a new plan and a full
  acceptance run first, and it costs more (up to 1.65× on the 400-unit benchmark).
- **(d) Something else** you would like tried; say what.

Recommendation: (c) is the only option that enforces your Hold Position and pushing rules, if its own acceptance
holds. Otherwise (a).

## Answer

- **Option (c): Plan and run a full acceptance pass for shipping WP1 (the movement redesign's contact layer) without WP2's gated intent.** Verify its standalone zig-zag figures and confirm every benchmark holds `≥ 50` ticks/s (Q0093); this cleanly isolates WP1's Hold Position immovability, enemy non-pushing, and `44–49 % → 10–14 %` crowd-overlap reduction before deciding whether any WP2 follow-up is needed.

## Resolution

Option (c) was run as the contact-layer acceptance pass: WP1 alone failed its zig-zag check (Q0147), and under the
owner's Q0147 answer WP1 + WP2 was evaluated and failed too (three tests red, evidence
`docs/evidence/2026-12-02-contact-layer-acceptance.md` §5). Closed as **(a), keep today's movement**; the waiting
movement work is re-planned against `Solver`.
