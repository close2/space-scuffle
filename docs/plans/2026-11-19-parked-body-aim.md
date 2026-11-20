# Blueprint: Parked-Body Aim: A Rounding Once Begun Is Held While the Corner Stays Occupied (B-145; B-144 measured)

This plan takes bug **B-145** (`DESIGN.md` §4), class R-F of plan
[`2026-11-19-crowd-edge-steering`](2026-11-19-crowd-edge-steering.md): a unit changes its aim between two consecutive
ticks under an unchanged order, and its step reverses. That plan's WP0 traced the showcase's recurring case to the
parked-body aim of plan [`2026-11-09-sc2-pathfinding`](2026-11-09-sc2-pathfinding.md) D-9 A3
(`aim_past_parked_body`): on the order's first tick a friend still stands on the unit's first corner, so the unit
aims past it on the outside of the turn; on the second tick the friend walks, is no longer "parked", and the unit
aims at its route again. It also measures one threshold-free side rule for **B-144** (class R-E, the crossing body),
as the brief allowed "if small enough"; that rule fails (§4.5) and B-144 stays open.

**Status of this document.** This is a Stage 1 blueprint under charter §6, written on 2026-11-19 against `main` at
`bf4252cf` (checksum schema 31). Line numbers drift, so search for the quoted symbol. Claims not confirmed while
planning are marked **verify**. Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** The rule moves the showcase and `golden_replay` (checkpoint 5), and it follows
plan `2026-11-09-sc2-pathfinding`'s recorded deviation from D-9 A3 ("A3 acts only on parked bodies"), whose failure
mode has to be designed out.

**Result in one line.** One rule holds: **D-1, a rounding once begun is held** (candidate H). Once a unit has chosen
its sense round a body parked on its corner (`MovementMemory::corner_round` ≠ 0), any non-yielding body touching
that corner, moving or not, keeps it rounding until the corner is passed. It adds no constant and no state (schema
stays 31), leaves the whole matrix, battle 33 and battle 31 bit-identical, removes 13 of the showcase's 14 R-F
reversals, and lowers the showcase's reversals from 174 to 132 on its nine seeds and from 595 to 519 over 33 seeds
(paired t = −2.9). Four other candidates fail (§4).

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §3, D-1 |
| [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) sanctioned deviations | None touches route following or local avoidance between friends | §3.3 |
| [Q0074](../questions/closed/0074-surround-walk.md) the surround walk stays | Answered, option 1 | §2.3, §4.3 (the walk-start part of R-F is the walk's) |
| Q0061 hand back after `cargo gate quick`; a verifier runs `package` | Standing rule | §6 |
| Q0002 scoping is the agent's call if written down | Standing rule | §9 |
| `docs/questions/open/` at the start of this session | Empty | — |

No owner question is raised: D-1 restores the literal text of an existing rule of our own (D-9 A3) for the remainder
of a rounding, which is a bug fix under Q0051, and it costs no sourced value.

---

## 1. Summary

1. **Base (§2.1).** `main` at `bf4252cf` reproduces plan `2026-11-19-crowd-slide`'s base on `6b922732` bit for bit:
   `sim_core` has not changed since, and the funnel work in `sim_map` (plan `2026-11-19-funnel-fallbacks`) moves none
   of these runs. Matrix of record 4,659 reversals (1,762 proposal, 2,897 contact), 2 give-ups; all seeds 22,666 and
   5; battle 33 blue 39 of 68; battle 31 on 100 seeds 49 / 17 / 34; showcase 174 reversals on nine seeds, seed 6
   unfinished at 3,000.
2. **Noise (§2.2).** The three placebos, re-measured on this base, give the same band as on `6b922732`:
   **[4,633, 4,698]** of record, [22,234, 22,615] over all seeds; placebo 1 again moves battle 26 seed 33 out of its
   range. On the showcase the placebos give 174 to 175, and three more placebos at the rule's own site (one raw unit
   on the aim's x, the aim's y, or the rounding reach) leave all nine showcase runs identical: **the showcase cannot
   be perturbed without meaning by a raw unit**, so its noise is read from many seeds instead (D-3).
3. **Diagnosis (§2.3, §2.4).** The probe's R-F label is "a proposal reversal across which the route's length
   changed". On today's base it covers three different things:
   - **The aim flip (B-145 proper): 9 showcase events, 0 in the matrix.** Player 1's rhino beetle 25 at tick 2 on
     every seed, exactly as plan `2026-11-19-crowd-edge-steering`'s WP0 traced. The matrix has no corner with a parked
     body: both rules that act there (F, H) leave all 1,160 matrix runs bit-identical.
   - **A surround walk that starts or ends on the tick a route is adopted: 5 showcase events, 7 in the matrix.** Unit 9
     at tick 351 or 352 (seeds 1, 4, 6, 7) and unit 23 at tick 439 (seed 2) start a walk against two anchored friends
     that walk on the next tick; five matrix events at tick 2 start a walk on tick 1 against friends that stand only
     because their own order is one tick old (battle 28 seed 22, zergling 104, traced); two more in battle 30. The
     route change is incidental. This is B-103's "walk's start retreat" (Q0074's walk), not an aim change.
   - **Six Viking events in battle 35** after a tick without a walk or chase (air units; **verify**, not traced).
4. **Candidates (§4).** Five rules and their combinations, each built as a run-time switch of one scratch tree
   (control bit-identical) and measured on the matrix of record, battle 33, battle 31 on 100 seeds, the showcase on
   nine seeds (and 24 more for the survivors), the reproductions and the class probe:

   | Candidate | Rule | Matrix of record (4,659) | Showcase, 9 seeds (174) | Holds? |
   |---|---|---:|---:|---|
   | **H** | a rounding once begun is held while a non-yielding body touches the corner | **4,659, bit-identical** | **132**; R-F 14 → 1 | **Yes** (§4.2) |
   | F | a friend that has not yet stepped under its order is not parked | 4,659, bit-identical | 195; R-F 15 | No: unit 25 still reverses; +17 % over 33 seeds |
   | F + H | both | 4,659 | 195 (= F) | No: as F |
   | W | such a friend does not stand anywhere (walls, walk start, parked) | 5,175; give-ups 13 over all seeds | 173; R-F 20, matrix R-F 37 | No |
   | F + W | both | 5,175 | 225 | No |
   | X (B-144) | a crossing body's side is taken from its lateral velocity (pass behind it), not its position | 5,566; give-ups 13; 2 survivor misses | 236; player 0 wins the default seed | No |

5. **Decision (§5).** D-1 lands H. D-2 records that no rule for a friend's first tick under its order holds (F, W),
   D-3 the showcase's paired many-seed reading, D-4 B-145's restatement (the walk-start events and the Viking events
   leave B-145), and D-5 that B-144's velocity side rule fails, so B-144 stays open.
6. **Work packages (§6).** WP0 records; WP1 failing tests; WP2 the rule, goldens on four builds with proof headers,
   throughput on instructions retired, the measurement of §4.1; WP3 bookkeeping.

---

## 2. Starting point and diagnosis

### 2.1 Base on `bf4252cf`

Release tools built from `bf4252cf` (scratch worktree `~/scratch/aim-flip/proto`, `CARGO_TARGET_DIR=
/home/spacescuffle/cargo-target/aim-flip-proto`), copied to `~/scratch/aim-flip/bin-base`, measured with
`~/scratch/aim-flip/scripts/measure.sh` (plan `2026-11-19-crowd-slide`'s script with its own lock,
`~/scratch/aim-flip/measure.lock`). Against `~/scratch/crowd-slide/base/`: `matrix.csv` identical in every column,
battle 33's and battle 31's `seeds.csv` identical, the nine showcase `units.csv` byte-identical.

| Measure | Base |
|---|---|
| Matrix, seeds of record (232 runs) | 4,659 reversals (1,762 proposal, 2,897 contact), 2 give-ups (battle 35); survivor ranges: 0 misses |
| Matrix, all seeds (1,160 runs) | 22,666 reversals, 5 give-ups; 4 survivor misses (none of record) |
| Battle 33 (68 runs) | blue 39 |
| Battle 31 (100 runs) | blue 49, red 17, unresolved 34 |
| Showcase, nine seeds | player 1 wins eight (1,935 / 2,141 / 2,170 / 2,542 / 1,939 / 1,939 / — / 2,175 / 1,937 for the default seed and seeds 1 to 8); seed 6 not over at 3,000; 174 reversals; 0 give-ups; 0 deeper unit-ticks |
| Showcase, seeds 9 to 32 (new here) | player 1 wins 22, two not over (seeds 22, 26); 421 reversals; 0 give-ups; mean length 2,201 ticks |
| Reproductions | as on `6b922732`: arc and walkers pass; the corridor, `no_room_means_no_reversals` and `showcase_crowd_edges_do_not_reverse` fail as recorded (all three ignored) |

### 2.2 Noise

**Matrix placebos** (`~/scratch/crowd-edge-2/patches/patches-placebo{1,2,3}.diff`, one raw unit on
`PUSH_RELAXATION`, `FIXED_BODY_TANGENT_WEIGHT`, `STEERING_LOOKAHEAD`), built from `bf4252cf`:

| Placebo | Reversals of record | All seeds | Give-ups | Survivor misses of record | Battle 31 (100) | Battle 33 | Showcase, 9 seeds |
|---|---:|---:|---|---|---|---|---:|
| P1 | 4,662 | 22,480 | 2 / 5 | 1 (battle 26 seed 33, blue 2 left: the noise seed of plan `2026-11-19-crowd-slide` D-2) | 49 / 17 / 34 | 39 | 175 (seed 2 ends at 2,165) |
| P2 | 4,633 | 22,234 | 2 / 5 | 0 | 49 / 17 / 34 | 39 | 174, identical |
| P3 | 4,698 | 22,615 | 2 / 5 | 0 | 49 / 17 / 34 | 39 | 174, identical |

**Band: [4,633, 4,698]** of record, as on `6b922732` (crowd-slide D-3 re-measured it; it happens to be unchanged).

**Showcase placebos at the rule's site** (new here; switches `pa1`, `pa2`, `pa3` of the scratch tree: one raw unit
added to the parked-body aim's x, to its y, or to the rounding reach `r_body + r_self + WAYPOINT_HANDOFF_MARGIN`):
all nine showcase runs byte-identical to base. A step is the speed times a unit direction, so a raw unit in the aim
does not change the step. The showcase therefore has no placebo band in the matrix's sense; D-3 reads it on 33
seeds, paired with base seed by seed.

**Class probe placebos** (P1 to P3 applied to the probe tree, `probe-out/placebo{1,2,3}.classes`): showcase R-A 42,
R-B 31, R-C 37 to 38, R-D 24, R-E 26, **R-F 14** under all three; matrix battles of record R-F **13** under all three
(R-A 1,365 to 1,393, R-B 1,584 to 1,595, R-D 1,022 to 1,056, R-E 167 to 171). Every aim flip happens at tick 2,
before a contact constant acts, so R-F has no noise: a change in R-F is the rule's.

### 2.3 The probe on today's tree

The class probe of plan `2026-11-19-crowd-edge-steering` §2.1 (`~/scratch/crowd-slide/probe-main.patch`, applied
unchanged to `bf4252cf` in `~/scratch/aim-flip/probe`, with the candidate switches of §4 added) gives exactly
crowd-slide's §2.4 figures:

| Run | R-A | R-B | R-C | R-D | R-E | R-F |
|---|---:|---:|---:|---:|---:|---:|
| Showcase, nine seeds | 42 | 31 | 37 | 24 | 26 | 14 |
| Matrix of record, battles 26 to 31 and 35 | 1,355 | 1,614 | 0 | 1,035 | 166 | 13 |
| Walkers / corridor / arc | 6 / 2 / 0 | 2 / 0 / 0 | 0 | 7 / 1 / 0 | 1 / 0 / 0 | 0 |

`classify.py` labels a proposal reversal R-F when the route's length (`walk route=N`, `chase … route=N`) differs
between the two ticks. **Every R-F event of today's base**, with the mode the probe gives each tick:

| Run | Tick | Unit | Modes | What it is |
|---|---:|---:|---|---|
| Showcase, all nine seeds | 2 | 25 (player 1, rhino beetle) | forward → forward | **the aim flip** (§2.4) |
| Showcase seeds 1, 4, 6, 7 | 351 / 352 | 9 (player 0) | walk across → forward | a walk started against anchored friends 10 and 11 the tick before they walked; route adopted that tick |
| Showcase seed 2 | 439 | 23 (player 1) | forward → walk across | a walk starting at a wall, with a route adopted |
| Matrix 26 seed 22 | 2 | 12, 89 | forward → walk / walk → forward | a walk started on tick 1 against friends standing in their order's first tick |
| Matrix 28 seed 22, 30 seed 22, 31 seed 33 | 2 | 104, 28, 21 | walk → forward | the same |
| Matrix 30 seeds 3203386110, 33 | 236, 231 | 27, 21 | walk → forward | a walk ending on a route change (**verify**, not traced) |
| Matrix 35 seeds 11, 33 | 229 to 351 | Vikings 1, 2, 4, 5, 7, 11 | (no walk tag) → forward / walk | air units after a tick in another branch (**verify**, not traced) |

So the showcase's R-F is 9 aim flips and 5 walk events; the matrix's 13 are walk and Viking events, **none an aim
flip**. B-145's "11 on the matrix's seeds of record" (crowd-edge-steering's count on `720c6971`) is not the aim
mechanism; D-4 restates the row.

### 2.4 Trace: the aim flip, showcase default seed

`~/scratch/aim-flip/probe-out/base/probe_showcase_default.txt`, `base/route/units.csv`:

- **Tick 1.** Rhino beetle 25 (player 1, attack-move to (11.5, 11.5)) stands at (58.50, 56.50) and adopts a route
  whose first corner is (58.51, 56.71), 0.21 north. Friend 26 stands at (58.5, 57.5) under the same order, velocity 0
  in the tick-start snapshot because the order is one tick old; it is not idle, so it does not yield, and it
  touches the corner with both radii: `parked_on_corner` finds it. `aim_past_parked_body` picks the outside of the
  turn, sense recorded in `corner_round`, aim (59.6, 56.5). `drop_passed_waypoints` does not hand the corner off,
  because a parked corner is handed off only beyond it. Step `(0.126, 0)`, east.
- **Tick 2.** Friend 26 walked `(−0.110, 0.062)` and its snapshot velocity is 0.126: not parked.
  `drop_passed_waypoints` now hands the corner off by distance (the unit is within `handoff` of it), and the unit
  aims at the next point (58.39, 56.89). Step `(−0.066, 0.108)`: cosine −0.52 against tick 1's, a reversal.
- `corner_round` stays set until the corner is passed, but the rounding itself stopped: **the sense is held, the
  decision to round is not.** Plan `2026-11-09-sc2-pathfinding` added the held sense (schema 22) because "the
  memoryless side rule … picks the inside" once the unit stands beside the body; the same reasoning applies to the
  decision to round at all, and the as-built deviation ("A3 acts only on parked bodies") makes that decision
  memoryless.

### 2.5 Trace: a walk-start R-F event, matrix battle 28 seed 22

Zergling 104 at tick 1: friends 103 and 106 directly ahead have velocity 0 (their order is one tick old) and do not
yield, so `remove_blocked_velocity` removes the forward step (`crowded`), `friend_blocks_ahead` holds, and the walk
starts: step `(0.0171, 0.0008)` sideways. Tick 2: the friends walked 0.126, the way forward is free (`fwd_free`), the
walk ends, and the forward step `(−0.1255, 0.0114)` reverses. The route grew from 0 to 1 point on that tick, which
is why the probe calls it R-F. The cause is the same as the aim flip's (a friend standing only because its order is
fresh), but the mechanism is the surround walk's (B-103).

---

## 3. What StarCraft II does

Tags as in the precedent plans: **[DOC]** Blizzard material; **[DATA]** SC2GameData 5.0.14; **[COMM]** community or
editor observation; **[2ND]** second-hand account; **[INF]** this plan's inference; **verify** where nothing confirms
it. S1 to S11 are those of plan `2026-11-19-crowd-slide` §3.1 (fetched 2026-11-19 by plan
`2026-11-19-crowd-edge-steering`); nothing new was fetched for this plan.

### 3.1 Sources

| # | Source | Bearing on this plan |
|---|---|---|
| S1 to S4 | 5.0.14 data: `CGame`, `CMoverAvoid "Ground"`, `CUnit` defaults, `CAbilMove` / `CAbilAttack` [DATA] | No data parameter for local avoidance, rounding a body or the side to pass a crossing body on |
| S9 | GDC 2011, Anhalt, as summarised [2ND] | Unit-blind global pathing, local steering on top; a unit that cannot slide past others repaths |
| S10 | Q0074, the owner's account of `CMoverAvoidance` | Units slide laterally round stationary friends |
| S11 | Team Liquid, [The Mechanics of SC2, part 1](https://tl.net/forum/starcraft-2/132171-the-mechanics-of-sc2-part-1?view=all) [COMM] | Units avoid each other and weave past without a new path or losing momentum |

### 3.2 What follows

- **The game's route is unit-blind and its avoidance is local** [S9]. Our A3 aim is local steering on top of an
  unwritten route, consistent with that; the rule itself (round a body on a corner on the outside of the turn) is
  ours, D-9 A3, not a game value [INF].
- **A unit weaving round a friend does not lose its momentum** [S11]. A unit that steps east to round a friend and
  then north-west because the friend began walking turns back on itself, which S11's flocking does not show
  [COMM, INF]. Whether the game's avoidance commits to a side for a friend that starts moving is engine code:
  **verify** (no source).
- **No source gives a threshold for a crossing body's side** (B-144's row, [S2]). The velocity side rule X (§4.5)
  needs none, which is why it was tried; it fails on measurement, not on sourcing.
- **Units ordered together start together** (common observation, **verify**): no source says how the game reads a
  friend's velocity on the first frame of a shared order. Our snapshot shows such a friend as standing for one tick
  (§2.4, §2.5). F and W (§4.3, §4.4) tried to correct that and fail.

### 3.3 Sanctioned deviations

Q0052's S-1 to S-7 touch none of this. Group lanes (S-7) are not involved: the showcase's route groups have an empty
lane word and no `GroupLane` event (plan `2026-11-19-crowd-edge-steering` evidence §0).

---

## 4. Candidates and prototypes

### 4.1 Method

All candidates are run-time variants of one scratch tree (`~/scratch/aim-flip/cand`, detached at `bf4252cf`, never
committed; diff `~/scratch/aim-flip/patches/candidates.diff`), selected by the environment variable `AF` (a comma
list), read once into a `OnceLock`. With `AF` unset the tree is bit-identical to base: controls `ctrl` (first build)
and `ctrl2` (the build with every switch) reproduce the matrix ("bit-identical"), battle 33's and battle 31's
`seeds.csv` and all nine showcase `units.csv` byte for byte. Each candidate is measured by
`~/scratch/aim-flip/scripts/evalenv.sh <name> <AF> <bin>`: the matrix on 16 seeds with `--compare` to base, battle 33
on 64 seeds, the showcase on the default seed and seeds 1 to 8 (3,000 ticks), battle 31 on 100 seeds, and
`cargo test --release -p sim_core --test second_line_tests --test crowd_edge_tests -- --include-ignored`. The
survivors of the first round were also run on showcase seeds 9 to 32 (`scripts/xseeds.sh`) and through the class
probe (`scripts/probe.sh`). Results: `~/scratch/aim-flip/<name>/summary.txt`, `repro.log`, `xs/<name>/`,
`probe-out/<name>.classes`.

**Stop rules** (plan `2026-11-19-crowd-edge-steering-2` D-4 as amended, crowd-slide D-2 and D-3):
1. **Winner:** the showcase default seed (player 1) and every matrix battle's overall winner; battle 31 on 100 runs.
2. **Survivor ranges:** no seed of record leaves its range, read against the placebos (battle 26 seed 33 is noise).
3. **Give-ups:** matrix of record and all seeds at most base + 1 (2 / 6); every showcase seed at most base + 1.
4. **Reversals:** an effect beyond the noise: below the matrix floor (4,633) if the rule acts in the matrix; if it
   leaves the matrix bit-identical, a paired fall on the showcase's 33 seeds (D-3).
5. **Guards:** arc 10 / 10, walkers within `walkers_at_the_crowds_edge_do_not_toggle`'s bounds, corridor not worse
   than base, every showcase seed reported, none of the nine newly unfinished or flipped, deeper unit-ticks 0.
6. **No new tuning constant** without a source or a derivation.

### 4.2 H: a rounding once begun is held

**Rule.** `parked_on_corner(context, corner, rounding)` gains one input, whether the unit is already rounding a body
at this corner (`MovementMemory::corner_round != 0`). When it is, the velocity test is dropped: any neighbour that
will not yield to the unit and touches the corner with both radii keeps the unit aiming past it. When it is not,
the test is today's (only a standing body starts a rounding). Both callers pass it: `aim_past_parked_body` (the aim)
and `drop_passed_waypoints` (a corner with a body on it is handed off only beyond it). `corner_round` is already
cleared when the corner is passed or the route replaced, so the rounding ends where the held sense ends today.

**Why it is the rule D-9 A3 meant.** A3's text was "a non-yielding body touching it"; the as-built deviation narrowed
the start of a rounding to parked bodies because rounding moving flockmates from the outset made the showcase worse
and flipped it. H keeps that narrowing for the start and restores A3's text only for a rounding already begun,
exactly as schema 22 holds the sense: a decision taken against a snapshot is not undone by the next snapshot while
its cause (a body on the corner) persists. No constant, no state.

**Measured** (base in brackets):

| Measure | H |
|---|---|
| Matrix, record / all seeds | **bit-identical**: 4,659 / 22,666, give-ups 2 / 5, 0 battles moved |
| Battle 33 / battle 31 (100) | identical: 39 of 68 / 49, 17, 34 |
| Showcase, nine seeds | player 1 on all nine; **seed 6 now ends** (1,925); default seed **2,622** (1,935); 132 reversals (174), every seed at or below base (13, 14, 19, 15, 13, 15, 15, 15, 13 against 17, 27, 22, 17, 23, 15, 16, 22, 15); 0 give-ups; 0 deeper |
| Showcase seeds 9 to 32 | 387 reversals (421); 13 seeds lower, 9 higher; player 1 wins 21, not over 3 (seeds 17, 28, 31; base 22 and 26, which now end); mean length 2,278 (2,201); 0 give-ups; 0 deeper |
| 33 seeds, paired | 519 against 595 (−12.8 %); per seed −2.30 ± 4.57, t = −2.90; 21 lower, 9 higher |
| Probe, showcase nine seeds | R-A 65 (42), R-B 22 (31), R-C 10 (37), R-D 20 (24), R-E 14 (26), **R-F 1 (14)**; the one left is unit 9's walk event on seed 6 |
| Probe, matrix, walkers, corridor, arc | identical to base |
| Reproductions | arc and walkers pass; CE-1 to CE-14 pass; the three ignored tests fail as on base |
| `sim_core`'s whole suite (`AF=hold`, release, `--no-fail-fast`) | only `golden_replay_reproduces_every_checkpoint` fails (below) |
| Goldens (`AF=hold`, release) | `golden_replay` diverges at checkpoint 5 (ticks ≤ 300); `golden_replay_orders` and `DEFAULT_SANDBOX_CHECKSUM` unchanged |

**Unit 25 under H** (`hold/route/units.csv`): it keeps the corner (route 24 points) and keeps rounding on the outside
on ticks 2 and 3, steps `(0.123, 0.027)`, `(0.088, 0.090)`, `(0.061, 0.110)`,
passes the corner on tick 4 and turns west over the next ticks in steps of at most about 65°; no reversal. The
rounding costs it a detour of under one world unit.

**Reading.** H removes the aim flip on every seed (R-F 14 → 1) and holds every stop rule. Most of the showcase's fall
is not R-F: once unit 25 rounds instead of flipping, player 1's column leaves its start differently, and R-C (the
slide along a wall into a body) falls 37 → 10 while R-A (the walk retreat) rises 42 → 65. That redistribution is the
rule's effect too (it is consistent over 33 seeds, t = −2.9), but it is not a mechanism the rule targets: §10 K-1.

### 4.3 F: a friend in its order's first tick is not parked

**Rule.** The snapshot marks a body `starting` when it is under an order and has not yet been through a movement
step of it (scratch proxy: `progress_best` is `None`, `stalled_ticks` 0, no walk active). Such a body, unless
anchored, is not parked. The idea: its zero velocity says nothing about whether it will walk.

**Measured:** matrix, battle 33 and battle 31 bit-identical; showcase 195 (+21), every seed changed, seed 6 now ends
but three seeds take 2,600 ticks; seeds 9 to 32: 492 (421), 19 higher, 3 lower, **player 0 wins seed 25**; 33 seeds
paired +2.79 per seed (t = +3.9). Probe R-F 15 (14): **unit 25 still reverses**: aiming at the corner, its steering
deflects round friend 26, which still stands in the snapshot (`(0.126, −0.005)` then `(−0.065, 0.108)`). F + H is
identical to F (with F the rounding never starts). **Rejected.** The scratch proxy also fires for one tick after
every target acquisition (`restart` clears the memory), which a production field would have to avoid; no such field
was built, since the rule fails on its own figures.

### 4.4 W: a friend in its order's first tick stands nowhere

**Rule.** F's `starting` bodies, unless anchored, are not standing in `SteeringObstacle::is_standing`: not wall
bodies, not "a friend that stands ahead" for the walk's start test, and (with F) not parked. Aimed at the tick-2
walk events of §2.5.

**Measured:** matrix of record **5,175** (+11 %; 1,944 + 3,231), all seeds 24,295, give-ups 3 / **13**; one survivor
miss of record (battle 26 seed 33, the noise seed); battle 31 (100) 40 / 24 / 36; battle 33 40; showcase 173;
probe matrix R-F **37** (13), R-B 1,931 (1,614); showcase R-F 20; `contact_creep_does_not_re_arm_a_walk` and CE-14 fail. With F
(F + W): matrix 5,175, showcase 225. A unit that does not see its starting neighbours as standing steps into them
(they have no receding allowance either), and contact undoes it. **Rejected.**

### 4.5 X: B-144's side from the crossing body's velocity

**Rule.** In `side_blockers`, a body that moves (more than `STANDING_DRIFT`, an existing constant) and is not
co-moving is put on the side its lateral velocity points to, so the unit passes behind it, instead of on the side
of its lateral offset; no threshold is added (B-144's row: no source gives one).

**Measured:** matrix of record **5,566** (+19 %), all seeds 26,034, give-ups 2 / **13**, **2 survivor misses of
record** (battle 26 seed 33, noise; battle 35 seed 33, 12 Vikings and 2 colossi left, outside its range and not a
placebo miss); battle 31 (100) 48 / 25 / 27; showcase 236 and **player 0 wins the default seed** (1,897) and seeds 4 and 5;
the walkers' bound fails; and R-E itself, the class it aims at, **rises** 26 → 49 on the showcase and 166 → 298 in
the matrix. **Rejected.** Passing behind every crossing body turns units against the flow of a crowd
whose members cross each other at every tick (the zerglings' battles 26 to 29 rise by 84, 340, 7 and 320).

---

## 5. Decisions

### D-1. A rounding once begun is held while the corner stays occupied (B-145)

1. `parked_on_corner` (`crates/sim_core/src/movement/locomotion.rs`) takes `rounding: bool`. With `rounding`
   false it is today's test (standing, will not yield, touching the corner with both radii). With `rounding` true the
   standing condition is dropped; the yielding and touching conditions stay.
2. `aim_past_parked_body` passes `*sense != 0`; `drop_passed_waypoints` passes `proposal.memory.corner_round != 0`.
   The two must agree, or a held rounding would hand its corner off by distance while still aiming past the body.
3. Nothing else changes: the side is chosen as today on the first aim, `corner_round` is cleared where it is today,
   and the probe that refuses an aim terrain will not let it reach (`probe_permits`) still falls back to the
   corner.
4. Docs: `parked_on_corner`, `aim_past_parked_body`, `drop_passed_waypoints` and `MovementMemory::corner_round` say
   that the sense and the decision to round are both held, with this plan's reference. Plan
   `2026-11-09-sc2-pathfinding` gets a one-line pointer amendment under its "A3 acts only on parked bodies"
   deviation.
5. No constant, no hashed field: checksum schema stays **31**. `golden_replay` is re-recorded (it moves at checkpoint
   5); the others are re-recorded only if they move on the final tree.

### D-2. No rule for a friend's first tick under its order

F and W, the two readings of "a friend that stands only because its order is fresh", fail (§4.3, §4.4). The
walk-start events of R-F (§2.3, §2.5) stay under B-103 with the walk's start retreat; the aim flip does not need
them (H removes it without touching the snapshot).

### D-3. Method: a rule the matrix cannot see is judged on 33 paired showcase seeds

When a candidate leaves all 1,160 matrix runs bit-identical, stop rule 4 is read on the showcase: the default seed
and seeds 1 to 32, each paired with base, and the claim is a fall of the total with the per-seed differences'
t below −2 and more seeds lower than higher. The nine seeds of record stay the stop rules' seeds (winner,
unfinished, give-ups); seeds 9 to 32 are reported with their outcomes. Reason: one raw unit cannot perturb the
showcase (§2.2), so there is no placebo band there, and the battle's length and per-seed counts swing with any
rule that acts on its first ticks (F and H both move every seed). This is a method rule for plans, like
crowd-slide's D-2; no test bound changes.

### D-4. B-145 is restated

The row keeps the aim flip (9 showcase reversals, fixed by D-1) and drops what is not an aim change: the walk events
the probe labels R-F (5 in the showcase, 7 in the matrix) go to B-103's "walk's start retreat" with their figures,
and the six Viking events of battle 35 are recorded under B-145 as an untraced residual (**verify**: WP2 traces one).
B-145's matrix figure "11" is replaced by "0 aim flips; the matrix's 13 R-F events are walk and Viking events".

### D-5. B-144 stays open

X, the only threshold-free side rule found, fails four stop rules (§4.5). B-144's row gains its figures and the
finding that passing behind every crossing body works against a crowd's own flow; a further attempt would need a
rule that tells a body crossing the unit's path from a crowd member flowing round it, which needs a source or a
derivation this plan does not have.

---

## 6. Work packages (commit order)

Branch `parked-body-aim`. Implementation agents hand back after `cargo gate quick` and the tests their change needs; a
verifier runs `cargo gate package` on the final tree (Q0061). Each WP appends to a new evidence file
`docs/evidence/2026-11-19-parked-body-aim.md`. Sizes: S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- This plan's row in `docs/plans/README.md` is added with the plan (`Proposed`); WP0 sets it to `In progress`.
- `DESIGN.md` B-145 restated per D-4 (pointer to this plan); B-144 gains D-5's figures and pointer; B-103's
  walk-start item gains the R-F walk events (5 showcase, 7 matrix) with §2.5's trace.
- Plan `2026-11-09-sc2-pathfinding` gets the pointer amendment of D-1.4.
- **Gate:** docs-only `package`.

### WP1: failing tests first (S)

Red on `bf4252cf`, green after WP2:

- **T1 (`crates/sim_core/tests/second_line_tests.rs`), `showcase_parked_corner_aim_does_not_flip`:** the showcase,
  default seed, three ticks; rhino beetle 25's achieved steps of ticks 1 and 2 do not reverse (the blocking meter's
  test, cosine ≥ −0.3), and its `corner_round` is set on tick 1. Base: `(0.126, 0)` then `(−0.066, 0.108)`. Locate
  the unit by its start position (58.5, 56.5) and owner, not by the literal entity id, so the test survives a
  scenario reorder (**verify** the fixture helper to use).
- **T2 (`crates/sim_core/tests/movement_contact_tests.rs`, next to M-5), `a_rounding_begun_is_kept_when_the_body_walks`:**
  `wall_with_gap` geometry as M-5; the unit placed within one body radius of its route's first corner, a friend on
  the corner beyond it given a `Move` on the same tick as the unit (so it stands in the tick-1 snapshot and walks
  from tick 2); the unit's proposed steps (`SimWorld::proposed_step`) over the first 10 ticks never reverse, and it
  reaches its goal. The implementer tunes positions until base reverses at tick 2 and records them; if no placement
  reproduces it with `wall_with_gap`, T1 alone is the red test and the evidence says so.
- **Controls (green before and after):** M-5 `route_continues_past_a_blocker_on_its_corner_waypoint` (a body held on
  the corner is still rounded and passed); **T3** `a_body_walking_at_first_sight_is_not_rounded` (the same geometry,
  the friend already walking when the unit first reaches the corner: `corner_round` stays 0 and the unit aims at the
  corner: the D-9 deviation's guard); `route_following_tests.rs` unchanged.
- **Unit tests** in `locomotion.rs`'s `mod tests`: `parked_on_corner` with a moving non-yielding body on the corner
  returns `None` with `rounding` false and the body with `rounding` true; a yielding (idle, standing) body is never
  returned; a body off the corner is never returned.

### WP2: the rule, goldens, throughput, measurement (M)

- **Implement D-1** (one function signature, two call sites, docs).
- **Control:** a scratch flag (never committed) that forces `rounding` false reproduces `bf4252cf` byte for byte
  (matrix, battle 33 and 31 `seeds.csv`, nine showcase `units.csv`).
- **Measure** per §4.1 with this plan's band (re-measured if the base moved, crowd-slide D-3) and D-3's 33 showcase
  seeds; class probe on the showcase, the reproductions and the matrix battles 26 to 31 and 35; the placebo probes'
  R-F figures (§2.2). **Stop rules** §4.1; a breach stops the WP and the evidence names the first divergent tick,
  entity and rule. Expected: §4.2's figures exactly (the rule is H's code without the switch).
- **Trace** one Viking R-F event of battle 35 (D-4) and record it.
- **Goldens:** `golden_replay` re-recorded on the final tree, native debug and release, `wasm32-wasip1` release and
  debug once, with the proof header: "Re-recorded for plan 2026-11-19-parked-body-aim WP2 (B-145): checkpoint 5 first
  changed; entity E at tick T rounds the body parked on its corner for N more ticks (D-1). Proof: a scratch build
  with `rounding` forced false reproduces the previous checksums at every checkpoint." `golden_replay_orders` and
  `DEFAULT_SANDBOX_CHECKSUM` are checked and re-recorded only if they move (under H they do not).
- **Throughput:** the rule adds no loop; judged on instructions retired (`perf stat -e instructions:u`, each
  scenario alone through a scratch `BENCH_ONLY` switch of the bench, never committed, as plan
  `2026-11-19-funnel-fallbacks` evidence §WP2), base and candidate; every scenario stays ≥ 25 ticks per second and
  the instruction count within 1 % of base. A wall-clock run on a quiet host is an open confirmation item, not a
  blocker.
- `showcase_crowd_edges_do_not_reverse` stays ignored; its figures (default seed 8 / 5 under H against bounds 0 and
  3) are recorded. No other bound changes.
- **Gate:** `quick` and the tests above; the verifier runs `package`.

### WP3: bookkeeping (S)

- `DESIGN.md`: B-145 **fixed** for the aim flip with D-1's figures; the Movement and pathfinding row's "a unit steers
  past a body parked on its corner on the outside of the turn" gains "and keeps rounding it while the corner stays
  occupied (plan `2026-11-19-parked-body-aim` D-1)"; its test files list gains any new file.
- B-103's showcase figures re-stated from WP2's probe (R-A rises, R-C falls; §10 K-1).
- `docs/plans/README.md` row `Done` with commits; evidence complete.
- **Gate:** docs-only `package`.

---

## 7. File ownership

| Area | Files | WP |
|---|---|---|
| Records | this plan, `docs/plans/README.md`, `DESIGN.md` §4 (B-103, B-144, B-145) and the Movement row, plan `2026-11-09-sc2-pathfinding` (pointer amendment), `docs/evidence/2026-11-19-parked-body-aim.md` | WP0, WP3 |
| Tests | `crates/sim_core/tests/second_line_tests.rs` (T1), `crates/sim_core/tests/movement_contact_tests.rs` (T2, T3), `locomotion.rs` `mod tests` | WP1 |
| Rule | `crates/sim_core/src/movement/locomotion.rs` (`parked_on_corner`, `aim_past_parked_body`, `drop_passed_waypoints`), `crates/sim_core/src/entity.rs` (doc of `corner_round` only) | WP2 |
| Goldens | `crates/sim_core/tests/golden_replay_tests.rs` (and `crates/sim_fixtures/tests/fixture_tests.rs` only if it moves) | WP2 |

No other open plan owns these functions; plan `2026-11-19-crowd-slide` changed none of them.

---

## 8. Determinism, checksum and throughput

- No float, no new collection, no new iteration order: `parked_on_corner` keeps its `find` over the snapshot's
  neighbours in their existing order.
- No hashed state is added; `corner_round` is already hashed (schema 22). Schema stays 31.
- The rule reads the tick-start snapshot and the unit's own memory only, so it is independent of processing order.
- Cost: one boolean per call; no new loop, allocation or query.

---

## 9. Out of scope and open classes (Q0002)

| Item | State | Figures | What a further attempt would need |
|---|---|---|---|
| R-F walk events (a walk started against friends that stand only in their order's first tick, or the tick before they walk) | To B-103 (D-2, D-4) | 5 showcase, 7 matrix of record | A first-tick reading of a friend's intent that does not make units step into each other (W failed); belongs with the walk's start retreat |
| Battle 35's Viking R-F events | B-145 residual, untraced | 6 of record | WP2's trace |
| **B-144**, the crossing body (R-E) | Open (D-5) | showcase R-E 26 (14 under H, by redistribution); matrix 166 | A rule that tells a body crossing the path from a crowd member flowing round the unit, with a source or derivation; X fails |
| The redistribution under H (R-A 42 → 65, R-C 37 → 10) | Reported, not a target | §4.2 | B-103's classes, re-measured in WP2 |
| B-103's R-B, R-C, R-D, the corridor's standing | Open, as plan `2026-11-19-crowd-slide` §9 | — | Unchanged |

---

## 10. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| K-1 | Most of H's showcase gain is redistribution, not R-F (R-C −27, R-A +23) | A later movement change may give part of it back | The claim is made on 33 paired seeds (D-3), not on nine; B-103's figures are restated from WP2's probe |
| K-2 | H lengthens some battles: default seed 1,935 → 2,622; three of seeds 9 to 32 unfinished (two on base) | Slower showcase captures | Reported; not a stop rule (the battle's length swings between about 1,930, 2,150 and 2,600 ticks on base already, §2.1); WP2 re-measures |
| K-3 | A held rounding follows a body that walks along the unit's own way | A detour of up to about a world unit (unit 25: two extra ticks east) | The rounding ends when the corner is passed or the route replaced, as the held sense does today; give-ups 0 and deeper 0 on 33 seeds |
| K-4 | A stream of walkers through a corner keeps a rounding alive | A unit circles a busy corner | The half-plane hand-off still passes the corner; WP2 watches give-ups and stalls (0 on 33 seeds and the matrix) |
| K-5 | `golden_replay` moves | Re-record on four builds | Proof header with the rule forced off |

---

## 11. Prototype material (scratch, not committed)

- Scripts: `~/scratch/aim-flip/scripts/` (`eval.sh`, `evalenv.sh`, `measure.sh`, `summary.py`, `show.py`,
  `xseeds.sh`, `xshow.py`, `probe.sh`, `cls.py`, `queue1.sh` to `queue3.sh`).
- Diffs: `~/scratch/aim-flip/patches/candidates.diff` (every candidate and the `pa` placebos, switched by `AF`) and
  `probe-tree.diff` (the class probe plus the candidates, with a `hold-kept` tag).
- Results: `~/scratch/aim-flip/{base,placebo1,placebo2,placebo3,ctrl,ctrl2,fresh,hold,freshhold,wide,freshwide,
  cross,pa1,pa2,pa3}/`, `xs/{base,hold,fresh,freshhold}/` (seeds 9 to 32), `probe-out/<name>/` and
  `probe-out/<name>.classes`.
- The scratch worktrees and their target directories are removed at hand-back; the diffs rebuild them on
  `bf4252cf`.
