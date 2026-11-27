# Q0141: How many rally waypoints may a building hold, and what does one more Shift-click do?

- **Asked:** 2026-12-01
- **Blocking:** no
- **Default until answered:** (b), keep 4 waypoints and refuse a further Shift-click; keep B-215 open

## Question

Shift-clicking with a production building selected adds rally waypoints; a new unit walks them in order. Space
Scuffle allows 4 per rally, read from data (`max_waypoints` in `structures.ron`), and refuses a fifth. The 4 comes
from a 2011 community tip ("shift-click on several units (up to four)"); StarCraft II 5.0.14's data has no such
field, and no current source gives the number or says what a Shift-click past it does.

- (a) Accept 4 and the refusal as a recorded deviation (Q0052), and close B-215.
- (b) Keep them as an open minor difference (default).
- (c) Tell us what you see in StarCraft II: how many Shift-click rally targets a building keeps, and whether one
  more is ignored, replaces the last, or pushes out the first. We change the data (and, if it is not "ignored", add
  a data switch for the rule).

Recommendation: (c) if you can check in game; otherwise (a).

## Answer

