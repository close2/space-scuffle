# Blueprint: TA*'s Channel Under Partial Knowledge (B-149)

This plan fixes bug **B-149** (`DESIGN.md` §4): under partial knowledge TA* settles on a channel round an unknown
footprint instead of across it. The footprint's many small triangles exhaust the anytime budget
(`NAV_TA_STAR_EXTRA_EXPANSIONS`, 256) and the visit cap (`NAV_TA_STAR_TRIANGLE_VISITS`, 4), which were measured on
known terrain only.

**Status of this document.** This is a Stage 1 blueprint under charter §6, written on 2026-11-20 against `main` at
`e58f1af1` (checksum schema 31). Line numbers drift, so search for the quoted symbol. Claims not confirmed while
planning are marked **verify**. Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** The fix changes which channel TA* returns, for every request, so it can move
battle outcomes and golden replays. It re-pins three `sim_map` digests (`PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN` and
`CONTOUR_ROUTE_CHECKSUM`). The prototype (§4) moved no battle and no golden replay; the stop rules must still hold in
the clean implementation.

**Inputs.**
- Plan [`2026-11-19-partial-knowledge-funnel`](2026-11-19-partial-knowledge-funnel.md) §4.4, §9 and its
  [evidence](../evidence/2026-11-19-partial-knowledge-funnel.md) (WP2's two longer oracle routes, WP3's observation
  of per-cell squares layout 19 q 8), where B-149 was found; its measurement method and stop rules (§5 D-8), which
  this plan follows.
- Plan [`2026-11-09-sc2-pathfinding`](2026-11-09-sc2-pathfinding.md): D-4 (the width test), D-5 (TA*), WP8 (where
  the budget was set) and its [evidence](../evidence/2026-11-09-sc2-pathfinding.md) ("Grid against mesh (WP8)").
- Plan [`2026-11-19-funnel-fallbacks`](2026-11-19-funnel-fallbacks.md) D-5 and plan
  [`2026-11-19-crowd-slide`](2026-11-19-crowd-slide.md) D-2: the battle set and the placebo-aware survivor rule.
- Plan [`2026-11-10-group-routing`](2026-11-10-group-routing.md) D-5: the lane-restricted search shares TA*'s loop.

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II 5.0.14 | Standing rule | §2 |
| Q0061 hand back after `cargo gate quick`; a verifier runs `package` | Standing rule | §6 |
| Q0002 scoping is the agent's call if written down | Standing rule | §9 |
| [Q0079](../questions/closed/0079-group-consistent-routing.md) per-unit routes | Default kept | D-7 |
| `docs/questions/open/` | Empty at planning time; nothing on pathfinding | — |

---

## 1. Summary

1. **Diagnosis (§3).** TA*'s nodes are channels (triangle sequences), ordered by `f = g + h` with `g` a lower bound
   on the route to the node's entry edge. On `main` that bound is the larger of the straight distance from the start
   and, over the last eight ancestors (`NAV_TA_STAR_BOUND_ANCESTORS`), an ancestor's `g` plus the distance between
   the two edges. Across an unknown footprint every channel that threads the footprint's lattice of small triangles
   gets nearly the same bound, whether it runs straight or zigzags, because adjacent portals share a vertex and the
   edge-to-edge distances are zero. Since plan `2026-11-19-partial-knowledge-funnel` the funnel pulls the free
   vertices of those portals as points, so the zigzag channels really are longer, but TA* cannot tell them apart
   before it pulls them. Two caps then cut the search:
   - **The anytime budget.** After the first route TA* spends 256 more expansions; in case T1 the straight channel
     needs 1,136. **S11 itself names the cause**: "many, especially small obstacles … forced them to consider a large
     number of paths to each node … these paths differed very little in their g-values, keeping the searches from
     pruning them."
   - **The visit cap.** A (triangle, entry edge) state is expanded by at most four channels, the first four in `f`
     order; with near-equal bounds those are arbitrary, and the straight channel is cut (case T2).
2. **Design (§5).**
   - **D-1, a channel-geodesic lower bound.** A node's `g` also takes the length of the shortest point path through
     its own channel, from the start to the node's exit edge: the funnel's apex path plus the distance from the apex
     to the edge (S11's "highest possible underestimate", the tightest member of its "shortest path between entry
     edges" family). Portal ends that a pull may pass beyond (a free vertex within `r + margin` of an obstacle
     vertex, which D-2 of the partial-knowledge funnel clamps outwards) do not constrain it, so it stays admissible.
     It is kept incrementally per node (apex, two funnel sides), so a child costs one funnel step.
   - **D-2, S11's own termination.** The search ends when no open node has `f` below the incumbent's length.
     `NAV_TA_STAR_EXTRA_EXPANSIONS` is removed; the work stays bounded by the visit cap (at most
     `3 × NAV_TA_STAR_TRIANGLE_VISITS × triangles` expansions per pass, a bound derived from the cap, not tuned).
   - **D-3, a legal-only search before the fallback.** When every pull of the returned channel has failed (the
     two passes and the three retries), TA* runs once more and lets a channel become the incumbent only when its pull
     is legal; the portal-midpoint fallback is taken only when that search finds none. S11's search "assume[s] that
     such a path is possible; if a channel cannot yield a valid path … it would not be considered". It runs only on
     would-be fallbacks, so no other route changes.
   - **D-4, the visit cap is unchanged**, measured alternatives rejected; its residual is recorded (B-150, proposed).
   - No new constant; `NAV_TA_STAR_EXTRA_EXPANSIONS` is removed.
3. **Prototype (§4, scratch).** One release binary, base with the switches off, every figure against it:
   - Partial-knowledge oracle corpus (whole squares): **1,804 → 1,815 of 1,820 within 1.01, worst 1.1343 → 1.0193**
     (the 1.0193 route has the same ratio on a map without the unknown structures, so it is not B-149's); 53 routes
     shorter, 5 longer (at most +0.24 %), none shorter than the oracle.
   - All-known oracle corpus (the same layouts, every structure known): unchanged, 1,743 of 1,744, worst 1.0193;
     one wedged-start route of 1,889 changes, shorter.
   - Fallbacks: no new fallback in any corpus. Partial-knowledge stress search **10 → 0**; per-cell contour layouts
     **5 → 2** from fitting starts (including layout 58 q 13, the partial-knowledge funnel's accepted residual); one
     golden-corpus wedged fallback becomes legal; the all-known corpora (150, 1,000, 2,000 flush layouts) stay at 0.
   - Per-cell squares layout 19 q 8 (the 31 % observation): **26.195 → 19.876** (19.951 before B-146's fix).
   - Path-search cost (instructions retired, the partial-knowledge corpora): **46.47 G → 46.26 G (−0.5 %)**.
   - Throughput (each scenario alone, instructions retired): showcase canyon **+0.50 %**, every other scenario
     within ±0.0001 %; whole benchmark +0.13 %.
   - Battles: the matrix (1,160 runs), battle 33 (68), battle 31 (`--seeds 100`) and the showcase on nine seeds are
     **bit-identical**; `golden_replay_tests` pass unchanged.
   - Pins: `PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN` and `CONTOUR_ROUTE_CHECKSUM` move (would-be values in §4.6); every
     other `sim_map` test passes.
4. **Work packages (§6).** WP0 records; WP1 failing tests; WP2 D-1; WP3 D-2; WP4 D-3; WP5 tests and pins; WP6
   bookkeeping.

---

## 2. What StarCraft II does

Tags as in the pathfinding plan §2: **DOC** stated in a source, **2ND** secondhand, **INF** this plan's inference.

| Point | What is known | Tag |
|---|---|---|
| Search | A* over the triangles of a constrained Delaunay mesh, then a funnel (pathfinding plan §2, S2 to S4). Whether it is Demyen and Buro's TA* is not documented | 2ND; TA* **verify** |
| Search budget | No source gives StarCraft II's node or time budget, its anytime rule or a per-triangle visit cap | **verify** (none found) |
| S11's `g` | "In order to achieve the highest possible underestimate for the g-value, we take the maximum of a number of known admissible values": the start's distance to the entry edge, the start-goal distance minus `h`, the parent's `g` plus the drop in `h`, and "the parent node's g-value plus the shortest path between its entry edge and that of the current triangle" (S11, p. 943) | DOC (paper) |
| S11's termination | "we continue searching … updating our best path each time we find one shorter, until our best path is no greater than the sum of the g- and h-values of the remaining search nodes, at which point we know we have the shortest possible path" (p. 943). Its budgeted variant TA*(F) stops at F times the time to the first path; F = 10 "was optimal in most cases" (p. 946) | DOC (paper) |
| S11 on small obstacles | "Due to the aforementioned estimations and the need to consider all paths to each node, one thing that posed a problem … was the presence of many, especially small obstacles … forced them to consider a large number of paths to each node. In the case of small obstacles, these paths differed very little in their g-values, keeping the searches from pruning them" (p. 946). Their remedy: "not expand a triangle twice until the first path was found" | DOC (paper) |
| S11 on channels without a valid path | "We assume that such a path is possible; if a channel cannot yield a valid path for an object of radius r, it would not be considered by the search" (p. 944) | DOC (paper) |
| S5, predictability | StarCraft II "prioritized predictability in the behavior of its agents" (Anhalt, as cited) | DOC (private communication as cited) |
| Unseen structures | Whether StarCraft II paths through an enemy structure the player has not seen is undocumented (pathfinding D-2; partial-knowledge funnel §2) | **verify** |
| Structure knowledge granularity | In StarCraft II a structure under fog appears as a whole snapshot once seen; our knowledge is per cell (`update_known_structures`). Not sourced here | **verify**; out of scope (§9) |

**What follows.** D-1 is S11's own `g` rule taken to its tightest admissible member: the shortest path through the
channel, of which S11's "parent g plus the shortest path between entry edges" is a relaxation. D-2 is S11's own
termination rule, without the budget the pathfinding plan's WP8 added. The visit cap is ours (S11 has no cap after
the first path); it is kept (D-4). D-3 restores S11's assumption that only a channel with a valid path is a
candidate, as a last step before our fallback. No sanctioned deviation of
[Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) is touched and no new one is introduced: route
quality and search order are implementation detail beneath the documented "A* on triangles plus funnel".

---

## 3. Diagnosis

### 3.1 Method

A detached scratch worktree at `e58f1af1`, since removed; its diff is kept at `~/scratch/tab/proto.diff`, the scratch
tests and scripts beside it. It carried:

1. The partial-knowledge funnel's measurement patch (`~/scratch/pkf/impl/measure3.diff`: the fallback class probe
   and the per-request log of the golden corpora) and its scratch tests `zz_pkf_corpus.rs` (the §4.3 / §4.4 layout
   corpora and oracle of that plan; an added switch `PKF_ALLKNOWN` makes every structure known), `zz_pkf_probe.rs`
   (stress search) and `zz_b141_probe.rs` (the all-known corpora), plus `zz_tab_contour.rs` (the
   `CONTOUR_ROUTE_CHECKSUM` corpus, one line per request).
2. Switches in `search.rs` and `funnel.rs`, off by default, read from the environment once: `TAB_EXTRA` (the budget,
   0 unbounded), `TAB_VISITS` (the cap), `TAB_F` (S11's TA*(F)), `TAB_G` (S11's two other bounds), `TAB_ONCE` (S11's
   once-per-triangle rule), `TAB_Y` (the ancestor window), `TAB_SKIP` (coarse free space in the funnel), `TAB_APEX`
   (a dominance cap), `TAB_FG` (the channel bound: 1 every portal end, 2 free ends widened, 3 free ends skipped, 4
   clampable ends skipped, 5 the same incrementally), `TAB_LEGAL` (1 a legal pull only as the incumbent of every
   search, 2 a legal-only search after the retries fail), and `TAB_STAT` (per-search counts and an admissibility check:
   along every channel that reaches the goal, each node's `g + h` against the channel's pull length).
3. One release binary per tree for every measure, so base is the same binary with the switches off. The battle set
   is `~/scratch/tab/measure.sh` (the partial-knowledge funnel's, battle 31 on 100 seeds); throughput the bench with
   the `BENCH_ONLY` switch (`~/scratch/pkf/impl/bench-only.diff`) under `perf stat -e instructions:u`.

### 3.2 How TA* orders and cuts channels on `main`

- **Nodes are channels.** `MeshSearch::expand` pushes a child per non-blocking edge whose triangle admits the disc
  (pathfinding D-4); a triangle is never entered twice on one channel (`on_channel`). Popped in `(f, triangle, node)` order.
- **The bound.** `lower_bound`: `max(|start, edge|, max over ≤ 8 ancestors k of g_k + dist(edge_k, edge))`. Inside
  a lattice of cell-sized triangles consecutive portals share a vertex, so `dist(edge_k, edge)` is zero for the
  nearest ancestors and small for the rest. A channel that zigzags across an unknown 4 × 4 gets the bound of the
  straight one to within a cell.
- **The cut.** The first channel to reach the goal sets the incumbent (its `Inscribed` pull's length). The loop then
  breaks at `f ≥ incumbent` or after 256 more pops (`extra`, which counts every pop, also those the cap discards).
  Separately, the fifth and later arrivals at a `(triangle, entry edge)` state are discarded.
- **Why unknown footprints.** A known structure's triangles are closed and never searched. An unknown one is open
  ground cut into two triangles per cell (its outline and interior cell lines are mesh constraints, so knowledge stays
  per cell): a 5 × 5 is 50 triangles, each with up to three entry states, and the channels across it are
  combinatorially many.

### 3.3 Concrete cases

Default arena (64 × 64, its central obstacle at cells 30 to 34), squares or contours of `zz_pkf_corpus`, half the
structures unknown. Lengths in world units. They become WP1's tests (§6).

| Case | Request | `main` | Mechanism | Prototype (adopted design) |
|---|---|---|---|---|
| **T1** layout 25 q 11 (whole squares) | (39.5, 11.5) → (48.5, 56.5), r 0.3125 | 57.104, 10 waypoints, oracle 1.1343 (the corpus's worst); point bends at the free corners (48, 38), (49, 41), (49, 43) | First route after 51 expansions; budget spent (`extra` 256); the straight channel needs 1,136 more pops | 50.382 (oracle 1.0008) after **29** expansions: the first channel found is the straight one |
| **T2** layout 92 q 13 (whole squares) | (28.5, 49.5) → (49.5, 4.5), r 0.5 | 50.904 (1.0131), point bend at the free (44, 19) | **Visit cap**: unbounded budget still gives 50.904 (the open list empties after 576 expansions); 64 visits per state give 50.263 | 50.263 (1.0003) |
| **T3** per-cell squares layout 19 q 8 (wedged start, r 1.0) | — | 26.195; 19.951 before B-146's fix (the partial-knowledge funnel's 31 % observation) | D-2 and D-3 of that plan change the pulls' lengths, so the incumbent and the budget's cut-off change: the channel south of a structure wins | **19.876** |
| **T4** layout 23 q 0 (whole squares) | (42.5, 55.5) → (16.5, 28.5), r 0.75 | 42.975 | (a pitfall of D-1) the start lies on the first portal's line; Simple Stupid Funnel's strict collapse test then collapses onto the portal's far end, the bound overestimates (46.128 found instead) and the search stops early | 42.975 with the collinear rule of D-1 |
| **T5** per-cell squares layout 142 q 0 | (21.5, 20.5) → (40.5, 48.5), r 1.0 | 34.324 | (a pitfall of D-1) the free vertex (26, 26) is 1.0 from the known corner (25, 26), within `rho` 1.05: D-2 of the partial-knowledge funnel clamps the portal end to (26.05, 26), beyond the segment. A bound that treats (26, 26) as a funnel side is 34.47 at depth 3, above the route | Skipping clampable ends (D-1) keeps it below |
| **T7** layout 113 q 1 (whole squares) | (13.5, 61.5) → (59.5, 2.5), r 1.0 | 76.571 (1.0164), a point bend at the free (45, 12) | **Budget**: with D-1 alone still 76.571 (first route after 97 expansions, `extra` 256); the better channel needs 1,041 pops | 75.372 (1.0005) |
| **T6** per-cell contours layout 140 q 8 | (57.5, 53.5) → (40.5, 28.5), r 0.875 | 43.477 | (the residual) with D-1 the visit cap now discards the channel `main` found: 45.349 (+4.3 %) | 45.349; 64 visits give 43.477 (D-4, R-2) |

### 3.4 The corpora on `main`

| Corpus | `main` |
|---|---|
| Partial-knowledge oracle (150 layouts, whole squares, fitting starts) | 1,804 of 1,820 within 1.01, worst 1.1343, none shorter; of the 16 above, the partial-knowledge funnel traced 15, and 14 come within 1.003 on a map without the unknown structures |
| All-known oracle (the same generator, every structure known) | 1,743 of 1,744, worst 1.0193 |
| Partial-knowledge fallbacks (whole squares, whole contours, per-cell squares, per-cell contours) | 0, 1, 2 (1 fitting), 6 (5 fitting) |
| Stress search (known and unknown contour pairs) | 10 fallbacks |
| All-known corpora (150, 1,000, 2,000 flush) | 0 |
| Golden corpora (`PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN`) | fitting-start fallbacks 0 and 0; wedged 3 and 6 |
| Path-search cost (whole squares, whole contours, per-cell contours) | 12.32 + 14.42 + 19.55 = 46.47 G instructions (the measure of the partial-knowledge funnel's §4.6, which recorded 46.54 G on its WP3 tree) |

---

## 4. Prototype results (scratch, not committed)

### 4.1 Candidates measured on the partial-knowledge oracle corpus

Whole-square layouts, 1,820 routes from fitting starts; "longer" counts routes longer than on `main` (and the most).
Cost is the three-corpus path-search measure of §3.4.

| Candidate | Within 1.01 | Worst | Shorter / longer than `main` | Cost | Verdict |
|---|---|---|---|---|---|
| `main` | 1,804 | 1.1343 | — | 46.47 G | — |
| Budget 1,024 / 4,096 | 1,807 / 1,809 | 1.1035 / 1.0549 | — | — | An absolute number untied to the mesh: rejected |
| **Budget unbounded (S11's rule), cap 4** | 1,809 | 1.0549 | 7 / 0 | 52.18 G | Kept as D-2; not enough alone |
| Budget unbounded, cap 8 / 16 / 64 | 1,811 / 1,812 / 1,817 | 1.0549 / 1.0396 / 1.0396 | — | 76.5 / 128.0 / 415.7 G | Cost × 1.6 to × 9: rejected (D-4) |
| S11's TA*(F), F = 10 (budget = 10 × expansions to the first route) | 1,806 | 1.1343 | — | — | F is S11's empirical constant and T1 needs F ≈ 22: rejected |
| S11's other two bounds (start-goal minus `h`; parent `g` plus the drop in `h`) | 1,733 | 1.2828 | — | — | Worse: both bound `f`, not the distance to the edge, and the ancestor chain reads `g` as that distance: rejected |
| S11's "not expand a triangle twice until the first path", then unbounded | 1,750 | 1.1590 | — | — | Worse first routes: rejected |
| Ancestor window unbounded (the whole chain) | 1,810 | 1.0256 | 28 / 7 | 52.14 G | Weaker than D-1, as costly: rejected |
| Coarse free space in the funnel (portals with two free ends dropped) | 1,807 | 1.1343 | — | — | Whole squares gain 3 new fallbacks (0 → 3): rejected |
| Channel bound, every portal end a side (`TAB_FG=1`) | 1,814 | 1.0193 | 49 / 7 | 50.27 G | Inadmissible beyond clamped ends (T5): rejected |
| Channel bound, free ends skipped (`TAB_FG=3`) | 1,810 | 1.1265 | — | — | Admissible but blind inside unknown footprints: rejected |
| Channel bound, free ends widened by `2 rho` (`TAB_FG=2`) | 1,813 | 1.0419 | — | — | Widened portals cross each other and break the funnel: rejected |
| Channel bound with clampable ends skipped, budget 256 | 1,814 | 1.0193 | 49 / 7 (≤ +0.65 %) | 52.61 G (recomputed per expansion) | D-1 alone |
| D-1 + D-2 | 1,815 | 1.0193 | 53 / 5 (≤ +0.24 %) | 45.11 G (incremental) | One new fallback (per-cell contours 92 q 12, §4.2); completed by D-3 |
| D-1 + D-2 + a legal pull as the only incumbent of every search (`TAB_LEGAL=1`) | 1,815 | 1.0193 | — | — | The all-known corpus loses: layout 8 q 6 (r 0.5) 1.0000 → 1.0765, because the retries repair an illegal pull of the better channel on `main`: rejected |
| **D-1 + D-2 + D-3, a legal-only search before the fallback (`TAB_LEGAL=2`, adopted)** | **1,815** | **1.0193** | **53 / 5** | **46.26 G** | Changes only would-be fallbacks (§4.2) |
| D-1 + D-2 without the ancestor window | 1,815 | 1.0284 | 67 / 11 (≤ +2.6 %) | 45.67 G (recomputed) | More routes longer: rejected; the window is kept |
| D-1 + D-2, the cap per (state, funnel apex), 1 / 2 visits | 1,812 / 1,816 | 1.0396 / 1.0193 | 69 / 70, 79 / 19 | — | More routes longer: rejected (D-4) |
| D-1 + D-2, cap 64 (diagnostic) | 1,818 | 1.0193 | — | — | Shows the residual is the cap (§4.5) |

### 4.2 The adopted design on every corpus

D-1 + D-2 + D-3 (`TAB_FG=5 TAB_G=66 TAB_EXTRA=0 TAB_LEGAL=2`). D-3 changes nothing but would-be fallbacks, so every
row except the fallback rows equals D-1 + D-2 alone, request by request.

| Corpus | `main` | Adopted |
|---|---|---|
| Partial-knowledge oracle | 1,804 / 1,820 within 1.01, worst 1.1343, none shorter | **1,815**, worst **1.0193** (layout 117 q 0, 1.0193 also on the known-only map), none shorter; above 1.01: 44 q 12 (1.0140), 59 q 9 (1.0185), 94 q 13 (1.0155), 129 q 13 (1.0184), 117 q 0 |
| All 1,964 routed requests of that corpus (wedged starts included) | — | 69 change: 61 shorter, 6 longer (at most +0.77 %, layout 81 q 10, a wedged start) |
| All-known oracle | 1,743 / 1,744, worst 1.0193 | identical; 1 of 1,889 routes changes (layout 68 q 5, a wedged start, 77.285 → 77.010) |
| Partial-knowledge fallbacks from fitting starts (whole squares, whole contours, per-cell squares, per-cell contours) | 0, 1, 1, 5 | **0, 1, 1, 2**; no new fallback. Per-cell contours 58 q 13 (68.115 midpoints → 50.403), 104 q 3 (66.997 → 54.239) and 104 q 15 (59.445 → 33.711) become legal through D-3. Left: whole contours 59 q 9, per-cell 8 q 2 (both kinds) and 59 q 9, the partial-knowledge funnel's class N (a free anchor beside a known face where the disc has no room; the legal-only search finds no legal channel either) |
| Stress search | 10 | **0** |
| All-known corpora | 0 / 0 / 0 | 0 / 0 / 0 |
| `PATH_GOLDEN` corpus (174 routed) | — | 12 change, none all-known: 11 shorter, 1 longer (+0.24 %, fitting); fallbacks unchanged (0 fitting, 3 wedged) |
| `CONTOUR_PATH_GOLDEN` corpus (258 routed) | — | 16 change, none all-known: 13 shorter, 2 longer (+1.48 % fitting r 0.5; +4.66 % wedged r 1.0), 1 wedged fallback → legal; fitting fallbacks 0 |
| `CONTOUR_ROUTE_CHECKSUM` corpus (1,000 all-known contour routes) | — | 1 changes, shorter: (61.75, 19.5) → x 2.0, r 0.5, 62.090 → 61.497. Either rule alone gives the same: the budget cut it on `main` |
| Layouts, routes changed (whole contours, per-cell squares, per-cell contours) | — | 87 (76 shorter, 10 longer, at most +1.22 %), 45 (40 / 5, at most +1.52 %), 80 (3 fallbacks → legal, 59 shorter, 15 longer, at most +4.30 %, T6) |
| Per-cell contours 92 q 12 (r 1.0) | 69.905 | 70.863 (+1.37 %): with D-1 + D-2 alone the first channel's pull is illegal and shorter (65.377) than every other channel's bound, so it ends the search, the retries fail and the request falls back (93.082 by midpoints); D-3's legal-only search finds 70.863 |
| Per-cell squares 19 q 8 (T3) | 26.195 | **19.876** |
| Path-search cost | 46.47 G | **46.26 G (−0.5 %)**; D-1 + D-2 alone 45.11 G: D-3's searches cost 1.15 G, on the corpora's would-be fallbacks |

### 4.3 Admissibility (the check of `TAB_STAT`)

Along every channel that reached the goal in the four layout corpora, each node's `g + h` was compared with the
channel's pull length. The adopted bound (clampable ends skipped) exceeds it on no legal pull; on one illegal pull of
the per-cell contours (layout 92 q 12's first channel) it does, which is harmless: an illegal pull is not a route.
The unconditional bound (`TAB_FG=1`) exceeds a legal pull on 6 channels (T5's class), the existing eight-ancestor
bound on 1.

### 4.4 Battles and goldens

`~/scratch/tab/measure.sh`, base and candidate from one binary:

| What | Base | Adopted |
|---|---|---|
| Matrix, 1,160 runs | reversals of record 4,659, all seeds 22,666; give-ups 2 / 5 | **identical `matrix.csv`** |
| Battle 33, 68 runs | blue 39 / 68 | identical `seeds.csv` |
| Battle 31, `--seeds 100` (104 rows) | blue 49, red 20, unresolved 35 | identical `seeds.csv` |
| Showcase, nine seeds | player 1 wins on every seed (default seed at 2,622); 132 reversals | identical `units.csv` on all nine |
| `golden_replay_tests`, `fog_of_war_pathfinding_tests`, `wall_contact_tests` (`sim_core`, release) | pass | pass |

The base reproduces `main`'s published figures (matrix 4,659 of record; showcase 132 reversals, plan
`2026-11-19-parked-body-aim`), so the placebo band [4,633, 4,698] and battle 26 seed 33's P1 finding (crowd-slide
D-2) apply.

### 4.5 The residual is the visit cap

With the cap raised to 64 (diagnostic only), the adopted design reaches 1,818 of 1,820; T6 and the four B-149 routes
above 1.01 come back (44 q 12 1.0010, 59 q 9 1.0041, 94 q 13 1.0085, 129 q 13 1.0122). D-1 reorders the open list,
so the cap's four survivors at a state change; most changes are gains (53 against 5 in the oracle corpus). Raising
the cap costs × 1.6 to × 9 (§4.1). D-4 keeps the cap and records the residual.

### 4.6 Cost and pins

**Throughput** (`cargo bench -p sim_core --bench throughput`, each scenario alone through `BENCH_ONLY`,
`perf stat -e instructions:u`, under `flock ~/scratch/gate.lock`):

| Scenario | Base (G) | Adopted (G) | Change |
|---|---|---|---|
| 64×64, 0 units | 1.86097 | 1.86097 | +0.0000 % |
| 64×64, 100 units | 6.20260 | 6.20260 | +0.0000 % |
| 64×64, 400 units | 30.90521 | 30.90521 | −0.0000 % |
| 128×128, 400 units | 31.22848 | 31.22849 | +0.0000 % |
| 256×256, 0 units | 29.14178 | 29.14178 | +0.0000 % |
| Showcase canyon, 400 units | 53.69932 | 53.96556 | **+0.50 %** |
| 64×64, 67 casters, 400 units | 48.06449 | 48.06448 | −0.0000 % |
| Whole benchmark | 201.103 | 201.369 | +0.13 % |

The budget alone changes no instruction of the showcase scenario (its searches never reach 256 extra pops). The
bound recomputed per expansion from the whole channel cost +11.8 % there; recomputed from the apex +6.4 %; kept
incrementally per node +0.47 % (D-1's performance rationale); D-3 adds 0.02 %.

**Pins** (native release, would-be values):

| Pin | `main` | Adopted |
|---|---|---|
| `PATH_GOLDEN` (`validation_golden_tests.rs`) | `0x4062_b0b6_83dd_1afb` | `0xf322_b17b_9841_7b5e` |
| `CONTOUR_PATH_GOLDEN` (`validation_golden_tests.rs`) | `0x1da8_6365_bcbd_3b37` | `0x30cd_bf6a_3de1_a900` |
| `CONTOUR_ROUTE_CHECKSUM` (`determinism_tests.rs`) | `0x7ac0_f0b9_f8b4_2057` | `0xec9a_71b8_655a_01c2` |

Every other `sim_map` test passes in release with the adopted switches: `VALIDATION_GOLDEN`, `DECODE_GOLDEN`,
`SHOWCASE_MESH_DIGEST`, `test_cross_platform_determinism_checksum`, the lane tests, the funnel tests, the proptests
and the three oracle tests (figures in §4.7). Not run: the `wasm32-wasip1` builds (WP2 to WP4).

### 4.7 Oracle tests of `pathfinding_properties.rs`

Release, `--nocapture`, base and the adopted design (incremental form):

| Test | Base | Adopted |
|---|---|---|
| `partial_knowledge_routes_agree_with_the_oracle` (40 layouts, whole squares) | 485 of 489 within 1 %, worst 1.1343 | **489 of 489, worst 1.0054** |
| `mesh_route_lengths_are_within_one_percent_of_the_oracle` (all-known, 120 maps) | 522 of 523, worst 1.0220, 0 fallbacks | identical |
| `exact_fit_routes_agree_with_the_oracle` (all-known) | 293 routes, worst 1.0443, 153 of 159 zero-room within 1 % | identical |

The incremental form gives the same three pin values as the recomputed form, and the same routes on the oracle and
per-cell contour corpora, request by request.

---

## 5. Design decisions

### D-1. A channel-geodesic lower bound

**Rule.** For a node `n` whose channel crosses portals `p_1 … p_k` (each `(left, right)`, start first) and the
child entered through edge `e`:

- `L(n)`, `A(n)`: the length and the end of the shortest point path from the start through `p_1 … p_k`, cut at its
  last bend: Simple Stupid Funnel's apex and the length of the path to it.
- `g(child) = max(g_main(child), L(n) + dist(A(n), e))`, where `g_main` is today's bound (the start's distance and
  the eight-ancestor chain, kept, §4.1).
- **Sides.** A portal end constrains the funnel unless it is **clampable**: a free vertex (`vertex_blocks` false for
  the request) with an obstacle vertex strictly closer than `rho_max = r + CORNER_SAFETY_MARGIN`. Such an end's side
  is not updated by that portal. `rho_max` is the largest radius any pull uses (the inscribed pull; the fitted retry
  uses `r` plus at most `CORNER_SAFETY_MARGIN` (`fitted_margin`) and the zero-room retry `r + ZERO_ROOM_SLACK`), so
  every end any pull may clamp is skipped.
- **Collinear points.** A portal end on the line of the funnel's other side does not collapse the funnel (the
  collapse tests are strict in the crossing direction: `≤ 0` / `≥ 0` where Mononen's test is `< 0` / `> 0`). T4 is
  the case.

**Why it is admissible.** A pull of the channel crosses every portal in order, inside the segment at every end that
is not clamped, and keeps every obstacle vertex on its side; the point geodesic through the same constraints is no
longer, and the remaining route to the goal is at least the distance from `e` (`h`). A clampable end is the one
place where a pull passes beyond a portal's end (the partial-knowledge funnel's D-2: at most `2 rho` beyond), and the
bound gives it no side. Measured: no legal pull below its own channel's bound (§4.3).

**Incremental form.** Each node stores the funnel state after its entry portal: the apex (a vertex index or the
start), the length to it, and the two sides (vertex index and depth). A child applies one funnel step with its entry
edge; only when a side crosses the other (the apex moves) are the portals since the new apex re-read from the
node's ancestors. Clampable flags are cached per vertex for one search in a generation-stamped array.

- `// PERFORMANCE RATIONALE:` on the node fields (Why: the bound is read for every child, and recomputing the funnel
  from the start is O(depth) per child, +11.8 % on the showcase scenario; What: the funnel state is carried per
  node and advanced by one portal; Trade-off: four more `u32` and a `SimScalar` per node, and a re-read of the
  portals since the apex when it moves).
- `// PERFORMANCE RATIONALE:` on the per-vertex stamps (Why: the clampable test is a fan walk and a flood, asked for
  the same vertex by many channels; What: a `Vec<u32>` of search stamps sized to the mesh's vertices, a new stamp
  per search instead of clearing; Trade-off: one word per vertex kept between searches).

**What the bound is not.** It is a point bound: the inscribed pull rounds obstacle vertices at `rho`, so it is longer
by the arcs; the gap is what remains of the equal-bound problem, and the visit cap (D-4) decides there.

### D-2. S11's termination; `NAV_TA_STAR_EXTRA_EXPANSIONS` removed

**Rule.** The anytime loop runs while the open list's least `f` is below the incumbent's length. There is no
expansion budget.

**Why bounded.** Every expansion is the first, second, third or fourth at its `(triangle, entry edge)` state
(`NAV_TA_STAR_TRIANGLE_VISITS`), so a pass expands at most `3 × 4 × T` nodes (`T` the mesh's triangles) and pushes
at most two children each: the cost bound scales with the mesh, as B-149 asked, and follows from the cap without a
new number. The lane-restricted search counts per `(state, word)` (group-routing D-5); its words are pruned beyond `K`
letters of excess, which bounds the words per state (**verify** the bound's form at WP3 and state it in the doc).

**Measured.** Alone: 7 oracle routes shorter, none longer, worst 1.0549; no instruction of the showcase scenario
changes. With D-1: §4.2.

The module doc of `search.rs` (its "Invariants") and D-5 of the pathfinding plan (a one-line pointer amendment) say
the loop is S11's rule.

### D-3. A legal-only search before the fallback

**Rule.** In `route`, when the zero-room retry has failed too (no pull of the returned channel is legal), TA* runs once
more with one change: a channel whose inscribed pull fails the segment predicate (`is_legal`) is not taken as the
incumbent and does not end the search. If that search returns a legal route, it is the route (`fallback` false);
otherwise the request falls back to the portal midpoints of the first channel, as today.

**Why.** S11 considers only channels that yield a valid path (§2). Our anytime loop takes the first pull as the
incumbent whether legal or not, and the retries can only re-pull that channel. With D-1's tighter bounds an illegal
pull shorter than every other channel's bound ends the search at once (§4.2, per-cell contours 92 q 12). The
legal-only search is the missing step: a channel with a legal pull is searched for before the midpoints are walked.

**Why not in every search.** Measured (`TAB_LEGAL=1`, §4.1): the retries repair illegal pulls that are better than
any legal one (all-known layout 8 q 6: the second pass and the pinned retry make the best channel legal at 46.379;
a legal-only incumbent takes another channel at 49.923). So the normal search, its second pass and the three retries
are kept, and D-3 runs only where they all failed.

**Cost.** One more search per would-be fallback: 1.15 G of the path-search corpora (§4.2), 0.02 % of the showcase
scenario. `MeshSearch::fallbacks` keeps counting only real fallbacks.

**Measured.** No route changes but would-be fallbacks: per-cell contours 5 → 2 from fitting starts (58 q 13, 104 q 3,
104 q 15 legal), stress search 1 → 0, and 92 q 12 legal (70.863, +1.37 % against `main`).

### D-4. The visit cap stays at 4

The cap bounds the path-based search (§3.2). Every measured replacement is worse or costlier (§4.1): 8, 16 or 64
visits cost × 1.6, × 2.8 and × 8.9 on the path-search corpus; a cap per (state, funnel apex) lengthens 19 to 70
routes; S11's once-before-the-first-path rule gives worse first routes. The cap's value is a WP8 measurement on known
terrain, not a derivation. Its residual under partial knowledge (§4.5: four oracle routes above 1.01 at most
1.0185; T6 +4.3 % on the per-cell contours) is registered as **B-150** (proposed; **verify** the next free number at
WP0), open, for a later plan with a dominance rule that has a derivation. *(WP0: registered as **B-154**; B-150 to B-153 went to plan
`2026-11-20-resource-footprints`. B-150 in this plan means that row.)*

### D-5. One rule for every request

D-1 to D-3 apply to all-known requests, lanes and both TA* passes. Under full knowledge no portal vertex of the
shipped maps is free (the partial-knowledge funnel §3.2), so the bound's sides are every portal end; one all-known
contour route and one all-known wedged route of the corpora change, both shorter (§4.2). No request falls back that
did not.

### D-6. Determinism and state

- Fixed point only: funnel steps are `SimVec2::cross_2d` sign tests, lengths `SimVec2::distance`
  (`SimScalar::sqrt`). No float; no `HashMap` (C7, C9).
- Rounding. The bound's distances and the pulls' lengths are both Q32.32 square roots. If WP1's property test finds
  a bound above its legal pull by rounding alone, each distance term of the bound is padded down by one raw unit
  (`2⁻³²`), a derived pad stated with its unit test, as the clamp's `CLAMP_PAD` is; the prototype needed none.
- The node fields and the stamps are derived search state, not hashed; the checksum schema stays 31.
- The golden replays hash route waypoints, so a changed route would move them; none moved in the prototype.

### D-7. Scope kept

Per-unit routes (Q0079's default) and the knowledge model (per-cell, pathfinding D-2) are unchanged. The funnel
(the partial-knowledge funnel's D-1 to D-5) is unchanged.

### D-8. Measurement method and stop rules

The method is the partial-knowledge funnel's D-8: the matrix on 16 seeds, battle 33 on 64 seeds, battle 31 on 100
seeds, the showcase to tick 3,000 on the default seed and seeds 1 to 8, plus:

1. **Oracle corpora:** §4.2's partial-knowledge corpus and its all-known twin (`PKF_ALLKNOWN`), and the three oracle
   tests of `pathfinding_properties.rs`.
2. **Fallback corpora:** the two golden corpora with the class probe; the four partial-knowledge layout corpora; the
   stress search; the three all-known corpora.
3. **Path-search cost:** §3.4's three corpora, instructions retired.

**Stop rules.** A breach stops the WP; nothing beyond it is committed; the trace names the request (or the first
divergent tick, entity and request) and the rule that changed it.

1. **Winner.** No overall winner flips in the matrix, on the showcase default seed or in battle 33. Battle 31 is
   judged on 100 seeds: the winner is unchanged and its share does not fall by more than the placebo band.
2. **Survivor ranges.** No seed of record leaves its battle's survivor range, read against the placebos (crowd-slide
   D-2): a miss that placebo P1, P2 or P3 of this base also shows (battle 26 seed 33 under P1) is reported, not
   stopped on.
3. **Give-ups.** At most base + 1 on the matrix of record, over all seeds, and on every showcase seed.
4. **Reversals.** Expected bit-identical (§4.4). If the matrix moves, its reversals of record stay at or below the
   placebo band's top (4,698), and every moved battle is traced to a route whose channel changed.
5. **Control runs.** Each rule is switched off by a scratch flag, never committed, and must reproduce `main` byte for
   byte: the matrix, battle 33 `seeds.csv`, the showcase `units.csv` on nine seeds, the corpora and the three pins.
6. **Goldens and pins.**
   - `golden_replay_tests` is expected unchanged. A change is re-recorded only with a proof header: the first
     divergent tick, the entity, the request and the rule, and the control build reproducing the old value.
   - `PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN` and `CONTOUR_ROUTE_CHECKSUM` are re-recorded with proof headers: the
     requests that changed by class (fallback → legal, shorter, longer, with the longer ones named), and the old
     value reproduced with the rules off.
   - Every re-recorded value is identical in **native debug, native release, `wasm32-wasip1` release and
     `wasm32-wasip1` debug** (runner `wasmtime --dir=<real path of the worktree>`).
   - `VALIDATION_GOLDEN`, `DECODE_GOLDEN`, `EXPECTED_CHECKSUM`, `SHOWCASE_MESH_DIGEST` and
     `test_cross_platform_determinism_checksum` must not move.
7. **Throughput**, judged on instructions retired (each scenario alone, `perf stat -e instructions:u`, base and
   candidate, under `flock ~/scratch/gate.lock`), as funnel-fallbacks Amendment 1 point 3 settled for a loaded host:
   - every scenario within 2 % of base (the prototype: +0.50 % at most);
   - the path-cost measure no worse than base (the prototype: −0.5 %);
   - every scenario at least 25 ticks/s in a wall-clock run, taken as the host allows (a quiet-host run stays an
     open confirmation item, not a blocker).
8. **Oracle.**
   - No route shorter than the oracle by more than 10⁻³, in either oracle corpus.
   - Partial-knowledge corpus: at least 1,815 of 1,820 within 1.01, worst at most 1.0193; all-known corpus at
     least 1,743 of 1,744.
   - Routes longer than on base: at most the prototype's counts and amounts, corpus by corpus (§4.2: oracle 5 at
     most +0.24 %; whole contours 10 at most +1.22 %; per-cell squares 5 at most +1.52 %; per-cell contours 15 at
     most +4.30 %, 92 q 12 among them at +1.37 %; golden corpora 1 and 2 as listed), each traced to the visit cap
     (the cap at 64 restores it) or named otherwise.
9. **Fallbacks.** No request of any corpus falls back that did not on base. Fitting-start fallbacks at most 0, 1, 1
   and 2 in the four layout corpora and 0 in both golden corpora; the stress search 0; the all-known corpora 0.
   Each residual is traced.
10. **Admissibility.** WP1's property test (no legal pull below its channel's bound) holds on every corpus run.
11. **Tests.** A test changed for another reason than its WP's rule is restated first, test-only, with the trace. No
    bound is loosened, no tuning constant is added, `CORNER_SAFETY_MARGIN` stays 0.05 and
    `NAV_TA_STAR_TRIANGLE_VISITS` stays 4.

---

## 6. Work packages (commit order)

Branch `tastar-budget`. Implementation agents hand back after `cargo gate quick` and the tests their change needs; a
verifier runs `cargo gate package` on the final tree (Q0061). Each WP appends to a new evidence file,
`docs/evidence/2026-11-20-tastar-budget.md`. Sizes: S ≤ 1 day, M 2 to 3 days. The measurement patch (switches and
probes) lives in the evidence's scratch directory, never on the branch.

### WP0: records (S, docs only)

- `DESIGN.md` §4, B-149: the pointer becomes this plan, the diagnosis restated (equal bounds across the lattice; the
  budget and the cap; S11's own statement of the problem).
- `DESIGN.md` §4: register **B-150** (proposed, open; **verify** the number): "the TA* visit cap
  (`NAV_TA_STAR_TRIANGLE_VISITS`, 4) is a WP8 measurement on known terrain; under partial knowledge it discards a
  better channel at a state (after this plan: 4 routes of 1,820 above 1.01 of the oracle, at most 1.0185; a per-cell
  contour route +4.3 % against `main`)". Pointer: this plan §4.5 and D-4.
- The pathfinding plan gets a one-line pointer amendment at D-5 (the budget is replaced by S11's rule here); the
  group-routing plan one at D-5 (its "unchanged anytime budget").
- **Gate:** docs-only `package`.

### WP1: failing tests first (S, tests only)

Each test is deterministic and red on `main` where marked, `#[ignore]`d with the reason "B-149, WP2", "B-149, WP3" or
"B-149, WP4" until its WP lands, so `main` stays green. A test helper regenerates a layout of `zz_pkf_corpus`'s
generator by number (the generator of `partial_knowledge_routes_agree_with_the_oracle`, with a per-cell knowledge switch added).

| Test | File | Case | Asserts | Red on `main` | Lands with |
|---|---|---|---|---|---|
| `the_straight_channel_across_an_unknown_footprint_is_found` | `pathfinding_properties.rs` | T1 | length within 1.001 of 50.382; no turn at a free vertex | yes (57.104) | WP2 (D-1 alone finds it in 29 expansions; D-2 alone would too, after 1,136 pops) |
| `a_capped_state_keeps_the_straight_channel` | `pathfinding_properties.rs` | T2 | length within 1.001 of 50.263 | yes (50.904; D-2 alone does not fix it) | WP2 |
| `a_wedged_start_beside_unknown_cells_takes_the_short_channel` | `pathfinding_properties.rs` | T3 | length within 1.001 of 19.876 | yes (26.195; D-2 alone does not fix it) | WP2 |
| `the_anytime_loop_runs_to_the_incumbent` | `pathfinding_properties.rs` | T7 | length within 1.001 of 75.372 | yes (76.571; D-1 alone does not fix it) | WP3 |
| `a_start_on_the_first_portal_line_does_not_inflate_the_bound` | unit, `search.rs` | T4's channel, rebuilt from its portals | the bound at each node ≤ 42.975 − h | — (new code) | WP2 |
| `a_clamped_free_end_bounds_nothing` | unit, `search.rs` | T5's channel | the bound ≤ the pull's length at every node | — | WP2 |
| `channel_bounds_never_exceed_a_legal_pull` | unit, `search.rs` | the four partial-knowledge layout corpora at test size (40 layouts each) and the all-known corpus | for every channel reaching the goal with a legal pull, every node's `g + h` ≤ the pull's length | — | WP2 |
| `partial_knowledge_routes_agree_with_the_oracle` (tightened) | `pathfinding_properties.rs` | its 40 layouts | add: worst ≤ `MESH_RATIO_BOUND` (1.10) | yes (1.1343 is its layout 25) | WP2 (prototype: 489 of 489 within 1.01, worst 1.0054) |
| `the_search_ends_on_the_incumbent` | unit, `search.rs` | T1 | after the first route, the loop ends with every open `f ≥` the incumbent (no pop counter) | yes (ends on the budget) | WP3 |
| `a_legal_channel_is_sought_before_the_midpoints` | `pathfinding_properties.rs` | per-cell contours layout 58 q 13, (42.5, 44.5) → (52.5, 7.5), r 0.9375 (the partial-knowledge funnel's accepted residual) | no fallback; legal; length within 1.001 of 50.403 | yes (falls back) | WP4 |
| `an_illegal_first_pull_does_not_end_the_search` | `pathfinding_properties.rs` | per-cell contours layout 92 q 12, (60.5, 13.5) → (60.5, 52.5), r 1.0 | no fallback; length within 1.001 of 70.863 | no on `main` (69.905); red on the WP2 and WP3 trees (falls back, §4.2) | lands un-ignored in WP1 (green on `main`); WP2 ignores it with the reason "B-149, WP4" and WP4 un-ignores it |

- **Gate:** `quick`; `package` by the verifier.

### WP2: D-1, the channel-geodesic bound (M, `sim_map`)

- **Change.** `search.rs`: the funnel state per `Node`; the clampable flags (generation-stamped, D-1); the funnel
  step with the collinear rule; `lower_bound` takes the max with `L + dist(A, e)`; both `// PERFORMANCE RATIONALE:`
  blocks; the module doc's invariants. The funnel step is shared with no other code (the radius funnel works on
  shrunk points and circles); a doc comment says why it is a separate, point-only funnel.
- **Tests.** Un-ignore T1, T2, T3, T4, T5, the tightened oracle test and `channel_bounds_never_exceed_a_legal_pull`;
  ignore `an_illegal_first_pull_does_not_end_the_search` with the reason "B-149, WP4".
- **Measure.** The D-8 set against `main` with D-1 alone (budget 256): §4.1's "clampable ends skipped, budget 256"
  row is the expectation (1,814 / 1.0193; 49 shorter, 7 longer at most +0.65 %), recomputed for every corpus;
  pins expected to move; battles expected bit-identical; throughput (rule 7 against `main`). Rule 9's one known
  exception until WP4: per-cell contours 92 q 12 falls back (§4.2), recorded, not stopped on.
- **Exit.** The stop rules with D-1's own corpus figures and that exception.

### WP3: D-2, S11's termination (S, `sim_map`)

- **Change.** `ta_star`: the `extra` counter and `NAV_TA_STAR_EXTRA_EXPANSIONS` removed; the loop condition is
  `f < incumbent`; the doc comments of `ta_star`, `find_route_in_lane` and the module state the bound of D-2.
- **Tests.** Un-ignore T7 (`the_anytime_loop_runs_to_the_incumbent`) and `the_search_ends_on_the_incumbent`.
- **Measure.** The full D-8 set; §4.1's "D-1 + D-2" row and §4.2 without D-3's fallback rows are the expectation.
  Rule 9's exception of WP2 (per-cell contours 92 q 12) still holds; WP4 removes it. Control: the rules off reproduce `main` byte for byte.
- **Exit.** The stop rules, with that exception.

### WP4: D-3, a legal-only search before the fallback (S, `sim_map`)

- **Change.** `search.rs`: `ta_star` takes whether an illegal pull may be the incumbent (a `Query` field, not a
  `MeshSearch` flag); `route` runs the legal-only search after the zero-room retry fails and keeps the first
  channel's portals for the midpoint fallback when it finds none; the module doc states the order of the attempts.
- **Tests.** Un-ignore `a_legal_channel_is_sought_before_the_midpoints` and `an_illegal_first_pull_does_not_end_the_search`.
- **Measure.** The full D-8 set; §4.2's figures are the expectation. The three pins re-recorded with proof headers on
  four builds (D-3 moves none of them: §4.2's golden rows equal D-1 + D-2's). Control: each of D-1, D-2 and D-3 off.
- **Exit.** The stop rules, rule 9 without exception.

### WP5: tests and pins (S, tests only)

- The three pins, if WP2 to WP4 did not re-record them, each header naming the requests that changed by class and
  the old value reproduced with the rules off.
- `pathfinding_properties.rs`: `partial_knowledge_routes_agree_with_the_oracle` gains a per-cell knowledge half (its
  doc comment says the oracle is not exact there and asserts only "none shorter, no fallback from a fitting start").
- If a change to `crates/gate/suites.ron` is needed, it is measured (C22).

### WP6: measurement and bookkeeping (S)

- Evidence: the final D-8 figures, the corpus tables of §4 recomputed, throughput, the pins with their headers.
- `DESIGN.md` B-149 leaves the register as fixed, with the figures; B-150 stays open with the residual's figures.
  B-146's row notes that its accepted residual layout 58 q 13 and its stress residual are gone (D-3).
- `docs/plans/README.md`: this plan's row updated.
- No storyboard check is needed unless the showcase moved.

---

## 7. File ownership

| Area | Files | WP |
|---|---|---|
| Records | `DESIGN.md` §4 (B-146's note, B-149, B-150), `docs/plans/README.md`, pointer amendments to `2026-11-09-sc2-pathfinding` and `2026-11-10-group-routing` | WP0, WP6 |
| Evidence | `docs/evidence/2026-11-20-tastar-budget.md` (new) | WP1 to WP6 |
| D-1, D-2, D-3 | `crates/sim_map/src/navmesh/search.rs` (`Node`, `MeshSearch`, `Query`, `expand`, `lower_bound`, `ta_star`, `route`, module doc) | WP2, WP3, WP4 |
| Possibly | `crates/sim_map/src/navmesh/segment.rs` (`obstacle_vertices_near`, read only, unless the clampable test needs a point-only entry) | WP2 |
| Tests | `crates/sim_map/tests/pathfinding_properties.rs`, `validation_golden_tests.rs`, `determinism_tests.rs`, unit tests in `search.rs` | WP1 to WP5 |
| Pins | `PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN`, `CONTOUR_ROUTE_CHECKSUM` | WP2 to WP5 |

`funnel.rs` is not touched. **Coexistence.** At planning time a resource-footprint planner works in parallel and may
touch `sim_map` (B-147: resource nodes as obstacles), which would change meshes and so the pins and corpora. If it
lands first, this plan re-measures on the new `main` and re-records on top of it, both changes named in each pin's
header (**verify** its file list at WP2's start).

---

## 8. Determinism, checksum and throughput

- Exact lattice predicates and Q32.32 only (D-6); no float, no `HashMap` (C7, C9).
- No allocation in steady state: the funnel state lives in the nodes, the re-read portals and the stamps in
  `MeshSearch` buffers that keep their capacity.
- The checksum schema is unchanged (31).
- **Cross-platform.** The re-pinned digests are proven identical on four builds (D-8 rule 6); `golden_replay_tests`
  runs on `wasm32-wasip1` release in WP2 to WP4.
- **Throughput.** Measured +0.50 % on the showcase scenario, 0 elsewhere, path-search −0.5 % (§4.6); rule 7 bounds
  the clean form.
- **Charter.** No identity in Rust (C5). Functions under 100 lines: the funnel step, the re-read and the clampable
  test are their own functions. No new error path. No global mutable state (the prototype's environment switches are
  not carried over). One constant removed (`NAV_TA_STAR_EXTRA_EXPANSIONS`, a public item of `sim_map::navmesh::search`;
  its only readers are `search.rs` and documents), none added.

---

## 9. Out of scope and decisions recorded (Q0002)

| Item | State | Why not here |
|---|---|---|
| **The visit cap** (B-150, proposed) | Open | No measured replacement is better at an acceptable cost (D-4); a dominance rule with a derivation is a later plan |
| **A per-structure knowledge model** (a structure known as a whole once seen; its interior then needs no cell lines, and the lattice B-149 searches disappears) | Not done | Changes the knowledge model of pathfinding D-2 and `sim_core`'s `update_known_structures`; whether StarCraft II reveals structures whole is not sourced (**verify**). A candidate for a later plan, possibly an owner question |
| **Coarse free space in the mesh** (a per-request mesh without unknown structures) | Rejected | The pathfinding plan rejected per-request meshes (D-8); the funnel-level form was measured and adds fallbacks (§4.1) |
| Layout 117 q 0 (1.0193) | Unchanged | Not B-149: the same on a map without the unknown structures (the funnel's turn emission, the pathfinding plan's WP8 finding) |
| Wedged starts on a wall line | Unchanged | Pathfinding D-3 by design |
| The partial-knowledge funnel's remaining fallbacks (whole contours 59 q 9; per-cell 8 q 2 in both kinds; per-cell contours 59 q 9) | Unchanged | Class N of that plan: no channel has a legal pull (D-3's search finds none); a funnel matter, that plan's R-2. This plan removes its stress fallbacks (10 → 0) and three per-cell contour ones (58 q 13, 104 q 3, 104 q 15) |
| A legal-only incumbent in every search | Rejected | The retries repair illegal pulls better than any legal one (§4.1, all-known 8 q 6); D-3 applies it only before the fallback |
| An illegal pull as incumbent in the normal search (a shorter illegal pull can end the search; the retries may then return a longer route than another channel's) | Kept | The retries repair such pulls on `main`'s corpora more often than they lose (the all-known 8 q 6 case); D-3 catches the case that falls back. A later plan if a measured route needs it |

---

## 10. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | A battle route on another seed or map takes another channel | Outcomes move | Stop rules 1 to 5 with traces and controls; the prototype moved no battle and no golden replay |
| R-2 | The cap's survivors change with the new order and some routes get longer (T6, +4.3 %) | Slightly longer routes beside unknown cells | Rule 8 bounds them corpus by corpus; B-150 records the cause (D-4); the cap at 64 is the diagnostic |
| R-3 | The bound is inadmissible in a case the corpora do not hold (a pull outside a portal for another reason than D-2's clamp) | The search stops early, a better channel is missed | WP1's property test over every corpus; the bound only reorders and stops, it never makes a route illegal |
| R-4 | Without the budget a pathological mesh expands up to `12 T` nodes per pass | A slow search on a large map | D-2's bound; rule 7 on every scenario; the 256 × 256 scenario is unchanged |
| R-5 | The incremental funnel state diverges from the funnel recomputed from the start | Wrong bounds | The prototype's two forms produced identical corpora; WP2's unit test compares them on every node of the corpora |
| R-6 | Removing a public constant breaks a reader | Build failure | Only `search.rs` reads it (grep at WP3) |
| R-7 | Pins re-recorded on one platform only | CI's determinism jobs fail | Rule 6 requires four builds |
| R-8 | The resource-footprint work changes meshes and pins first | Re-measure and merge conflicts on the pin constants | §7 coexistence |
| R-9 | D-3's extra search on a map with many would-be fallbacks | Cost | It runs only after five failed pulls; measured 0.02 % on the showcase scenario and 1.15 G on the corpora built to provoke fallbacks; rule 7 |

---

## 11. Prototype material (scratch, not committed)

- Diff: `~/scratch/tab/proto.diff` (on top of `~/scratch/pkf/impl/measure3.diff` and `bench-only.diff`, both
  included): the switches of §3.1. The adopted design is `TAB_FG=5 TAB_G=66 TAB_EXTRA=0 TAB_LEGAL=2` (the
  incremental bound, clampable ends skipped, no budget, the legal-only search before the fallback; `TAB_G=66` selects
  the incremental form inside `TAB_FG=5`); `TAB_FG=4 TAB_EXTRA=0` is the same bound recomputed per expansion and
  gives identical routes.
- Scratch tests: `~/scratch/tab/zz_pkf_corpus.rs` (with `PKF_ALLKNOWN`), `zz_pkf_probe.rs`, `zz_b141_probe.rs`,
  `zz_tab_contour.rs`.
- Scripts: `~/scratch/tab/variant.sh <tag> [ENV=…]` (every `sim_map` corpus of D-8 for one variant),
  `cost.sh [ENV=…]` (path-search instructions), `measure.sh <out> [ENV=…]` (the battle set), `fb.py` and `chk.py`
  (T4 and T5 checked by hand).
- Results: `~/scratch/tab/v-{base,c1,c2,final3}/` (corpora; `c1` the bound recomputed with no budget, `c2` no
  budget alone, `final3` the adopted design), `o-*.txt` and `leg2-*.txt` (oracle and layout corpora per candidate),
  `bat-{base,c1,final,final3}/` (battle set; `final3` the adopted design), `throughput.txt`, `glog-*.txt` (golden
  corpora per request), `cr-*.txt` (`CONTOUR_ROUTE_CHECKSUM` per request), `final-tests.txt`, `oracle-final3.txt`,
  `simmap-final3.log`, `simcore-final3.log`.
- The prototype worktree and its target directory were removed after measurement.

## Amendment 1 (orchestrator, 2026-11-20): G-1 restated on Q0079's intent; group-routing fixtures join the measurement set

**Why.** The package gate failed on `group_routing_tests::g1_the_q0079_split_closes`. The trace is at
`~/scratch/tab/impl/g1-trace.md` and is to be copied into the evidence.

- **D-1 alone causes it.** With D-1, player 1's group lane is the west side of the plateau: 92.061 long, against
  92.419 for the east lane `main` takes. That lane is shorter and legal.
- **`main` can never find it.** Even with no budget it loses that channel to the visit cap among near-equal
  bounds, which is B-149's own mechanism.
- **What G-1 asserts.** The whole group still takes one lane, and every member's first route carries its word.
  G-1 also asserted that the word is non-empty and that all ten units cross y = 32 within 600 ticks. Both hold only
  for the east lane, which avoids player 0's army.
- **Classification:** rule-caused.

**Decision.**

1. **Restate G-1 on Q0079's intent, as a test-only change** carrying this trace in its doc comment:
   - the group has one lane;
   - every member's first route carries the lane's word;
   - every member that crosses y = 32 within 600 ticks crosses on one side.

   The two east-lane assertions are dropped. No rule changes.
2. **Measurement set.** The group-routing fixtures, `group_routing_tests` and its scenarios, join the measurement
   set of this plan's evidence. They also join the stop rules that later plans copy from it whenever a plan touches
   TA* or the funnel. Record that in the evidence and in the pathfinding plan's pointer amendment.
3. Re-run `sim_core` and `sim_map` in release. The whole suites must be green before the hand-back.
