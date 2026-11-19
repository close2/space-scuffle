# Blueprint: Crowd-Edge Steering 2: A Walk That Does Not End on Its Own Retreat, a Spent Walk That Waits for a Side to Open (B-103)

This plan takes over what plan [`2026-11-19-crowd-edge-steering`](2026-11-19-crowd-edge-steering.md) left open of
bug **B-103** (`DESIGN.md` §4) after its Amendments 1 to 3:

- the four reversal classes R-A (walk retreat), R-B (stale receding allowance), R-C (terrain slide into a body) and
  R-D (leftover overlap);
- re-arming a spent surround walk without the timed retry (the old D-3, risk K-6), which needs a defined "a side
  opens" test;
- the corridor's standing and reversals.

D-4 (Q0089 (a)) and the cause-split reversal meter (D-5) landed with that plan and are the base here.

**Status of this document.** This is a Stage 1 blueprint under charter §6, written on 2026-11-19 against `main` at
`76a7ac67` (checksum schema 30). Line numbers drift, so search for the quoted symbol. Claims not confirmed while
planning are marked **verify**. Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** Every rule package moves battle outcomes and `golden_replay`, D-1 adds a hashed
field (schema 30 → 31), and the work follows three withdrawn rules whose failure modes have to be designed out first.

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §3, every D |
| [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) sanctioned deviations | None touches crowd steering; the surround walk is Q0074's "allows more" | §3.3 |
| [Q0074](../questions/closed/0074-surround-walk.md) the surround walk stays; boxed-in units "stand still cleanly" | Answered, option 1 | D-1, D-2 |
| [Q0089](../questions/closed/0089-queued-attacker-keeps-order.md) a unit queued behind fighting friends keeps its attack order | Answered (a), landed (D-4 of the first plan) | D-2 relies on it |
| Q0061 hand back after `cargo gate quick`; a verifier runs `package` | Standing rule | §6 |
| Q0002 scoping is the agent's call if written down | Standing rule | §9 |
| **OQ-1 (orchestrator, blocking WP2 and WP3)** may stop rule 1 judge matrix battle 31, whose 20 matrix seeds split 6 / 7 / 7, on 100 seeds? | **Decided yes (Amendment 1).** Was: default **no** (the strict reading); then WP2 and WP3 do not start and R-A and the re-arm stay open with §4's figures | §2.3, §5 |
| **OQ-2 (orchestrator, blocking WP3)** may D-1 land although it lowers the corridor's standing from 96 to 87 % for one unit (reversals 7 → 3)? | **Decided yes, with a recorded residual (Amendment 1).** Was: default **no**; then WP3 does not start and D-1 stays open with §4's figures | §5 D-2.4 |

---

## 1. Summary

1. **What was measured (§2).** `main` at `76a7ac67` reproduces the first plan's WP1 base bit for bit (matrix of
   record 6,154 reversals, 2 give-ups; showcase default seed 13 / 10, player 1 at tick 1,946). Two new base facts:
   - **Noise.** Three placebo builds, each changing one movement constant by 2⁻³² (one raw unit), move 4 to 9 matrix
     battles and the reversals of record by −2.2 % to +4.2 % (6,017 to 6,412). A reversal difference inside that band
     is not evidence.
   - **Battle 31 is decided by one seed.** Over its 20 matrix runs it splits blue 6, red 7, unresolved 7. Over 100
     seeds the same build gives blue 46, red 22, unresolved 32. Every rule tried that moves the matrix (D-2 and D-3 of
     the first plan; this plan's A1, A1s, A5, B1, K3, K4) "flips" its 20-run winner to blue, and on 100 seeds every
     one of them leaves blue the winner, as on base. The battle's own claim in `matchup_matrix.ron` is "a small
     change of formation can swing the winner".
2. **Lessons from the three withdrawn rules (§2.4).**
   - D-1 forbade the retreat, but the retreat is the only way out of a pocket (T1), and its bounded projection pushed
     into bodies (T3). **A rule on the retreat costs reach; a rule on what the retreat makes does not.**
   - D-2 removed the step into a friend that had just stopped, and with it collision's slide round that friend, the
     way the crowd flows round fighting units. Restricting it to anchored friends (prototype B1) is worse.
     **The overlap R-B leaves and the slide that carries units round a fighting friend are one mechanism; a fix
     needs an explicit slide first.**
   - D-3 re-armed only on ground gained, so units in an open fight stood behind a front that opened beside them
     (K-6). **"A side opens" has to be read on the side that closed, and as a whole free step, or contact creep
     re-opens a corridor wall every few ticks.**
3. **Candidates and prototypes (§4).** Eight rules were prototyped in scratch builds and measured on the matrix of
   record, battle 33, the showcase on nine seeds, battle 31 on 100 seeds, the four reproductions and the class probe:

   | Class | Candidate | Holds the stop rules? |
   |---|---|---|
   | R-A | **A1** the walk does not end on the room its own retreat made | Yes on the 100-seed reading of battle 31; no on the 20-run reading (OQ-1) |
   | R-A | A1s, the same from last tick's step only (no state) | No: give-ups over all seeds 8 (base 6), reversals of record level |
   | R-A | A5 a paused walk steps forward only on a whole free step | No: reversals of record +11 %, give-ups 9 |
   | R-B | B1 D-2 against friends that hold their ground only | No: showcase default seed flips to player 0, six seeds do not end |
   | R-C | C2 the wall slide re-checked against bodies, alternating with the wall | Not proven: matrix untouched (no walls), showcase −8 % within its spread, R-C not reduced by the probe |
   | Re-arm | K3 re-arm when the side last closed opens (any admissible step) | No: reversals of record +0.7 % (not below base) |
   | Re-arm | **K4** re-arm when the side last closed opens as a whole free step, or on ground gained; `FLANK_RETRY_TICKS` removed | Yes on the 100-seed reading; no on the 20-run reading |
   | Both | **A1 + K4** | Yes on the 100-seed reading: reversals of record −24 %, give-ups 2 (2), every winner held but battle 31's 20-run split. But the corridor's standing, which K4 alone brings to 96 to 99.7 %, falls to 87 % for one unit (OQ-2) |

4. **Decisions (§5).**
   - **D-1 (R-A) = A1.** A walk in progress ends only when the straight way in is free for a whole step *plus the
     distance the walk has stepped back from its goal*. The retreat stays (T1), and the cycle "retreat, the straight
     step frees, step back in" cannot close. One new hashed field, `MovementMemory::flank_retreat`; schema 31. It
     costs the corridor's standing for one unit (96 → 87 %) while its reversals fall 7 → 3: **OQ-2**.
   - **D-2 (re-arm, corridor) = K4.** A walk spent because both sides closed re-arms when the side that closed last
     opens for a whole across step; any spent walk re-arms when the unit gains ground or its target changes.
     `FLANK_RETRY_TICKS` is removed. No new constant and no new state.
   - **D-3 (stop rule 1 for battle 31)** is OQ-1, the orchestrator's: on the strict reading, neither D-1 nor D-2 holds
     and both stay open (§9). WP2 starts only if OQ-1 is answered (b); WP3 also needs OQ-2 yes.
   - **R-B, R-C and R-D stay open** (§9), with the best candidate's figures. R-B needs an explicit slide round a
     friend that holds its ground before any settle can remove the overlap; that is a separate plan.
5. **Work packages (§6).** WP0 records; WP1 measurement tooling (battle 31 on 100 seeds, placebo band), bit-identical;
   WP2 D-2 (K4); WP3 D-1 (A1); WP4 bookkeeping. WP2 is gated on OQ-1, WP3 on OQ-1 and OQ-2. Each rule WP has red
   tests first, rule-off controls, goldens with proof headers on native debug and release, `wasm32-wasip1` release
   and wasm debug once, and throughput.

---

## 2. Starting point

### 2.1 Base on `76a7ac67`

Release tools built from `76a7ac67` into `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/crowd-edge-2-plan` and
copied to `~/scratch/crowd-edge-2/bin-base`; measured with `~/scratch/crowd-edge-2/scripts/measure2.sh` (the first
plan's `measure.sh`, plus battle 31 on 100 seeds, §2.3). The matrix is "bit-identical to
`~/scratch/crowd-edge/wp4b/matrix/matrix.csv`" (the first plan's restated WP4 run, itself bit-identical to its WP1
base); battle 33's
and the showcase's `units.csv` match too. So the first plan's evidence §1a holds on `main`:

| Measure | Base |
|---|---|
| Matrix, seeds of record (232 runs) | 6,154 reversals (2,918 proposal, 3,236 contact), 2 give-ups (battle 35) |
| Matrix, all seeds (1,160 runs) | 30,642 reversals, 6 give-ups |
| Battles 26 / 27 / 28 / 29 / 30 / 31 / 35, reversals of record | 1,024 / 1,033 / 1,393 / 1,047 / 165 / 203 / 668 |
| Battle 33 (68 runs) | blue 47, red 16 |
| Battle 31 (20 runs / 100 runs) | blue 6, red 7, unresolved 7 / blue 46, red 22, unresolved 32 |
| Showcase, nine seeds | player 1 wins every seed (default at 1,946); 235 reversals in all; 0 give-ups; 0 deeper unit-ticks |
| Showcase default seed | 13 (3 + 10) / 10 (5 + 5) |
| Arc | 10 / 10 at tick 96, 0 reversals |
| Walkers | 17 / 17, 31 reversals (23 + 8) |
| Corridor | 2 of 6 in range, 21 reversals (18 + 3), rear units standing within `STANDING_DRIFT` on 88, 88, 69, 65 % |
| CE-7 `a_queued_chaser_keeps_its_target` (ignored) | fails: stands 386 of 580 ticks, 17 reversals |
| CE-8 `a_spent_walk_resumes_when_the_way_opens` (ignored) | fails: its walk does not re-arm without the timer |

**Probe classes, showcase nine seeds** (the first plan's §2.1 probe, re-applied to `76a7ac67`; it reproduces the
first plan's WP1 classes exactly): R-A 63, R-B 89, R-C 32, R-D 30, R-E 7, R-F 14. Default seed: 5, 12, 1, 2, 1, 2.
Corridor 18, 1, 0, 2, 0, 0. Walkers 21, 2, 0, 6, 2, 0.

### 2.2 The noise band of the matrix

A movement rule perturbs a chaotic system, so totals move even where the rule does nothing on purpose. Three placebo
builds each add one raw unit (2⁻³²) to a constant that has no business deciding anything at that
resolution:

| Placebo | Constant | Battles moved | Reversals of record | Give-ups (record / all) | Flips |
|---|---|---:|---:|---|---|
| P1 | `PUSH_RELAXATION` + 2⁻³² | 9 | 6,412 (+4.2 %) | 2 / 6 | none |
| P2 | `FIXED_BODY_TANGENT_WEIGHT` + 2⁻³² | 4 | 6,017 (−2.2 %) | 2 / 6 | none |
| P3 | `STEERING_LOOKAHEAD` + 2⁻³² | 4 | 6,310 (+2.5 %) | 2 / 6 | none |

None moved battle 31, battle 33 or the showcase. **Reading:** "total reversals of record below base" is passed by
noise alone about a third of the time. This plan therefore reports every candidate's change against the band
[6,017, 6,412] and takes as evidence of an effect only a total below 6,017. WP1 makes the band part of the method.

### 2.3 Battle 31

`31_15_marauders_vs_15_stalkers` (`crates/sim_core/tests/fixtures/matchup_matrix.ron`) expects any outcome
(`blue: (0, 15), red: (0, 15)`) and claims "Equal supply is close: a small change of formation can swing the winner".
Its history agrees: friendly-blocking WP3 moved it 17 / 3 → 7 / 13, the deviation fixes 11 / 9 → 16 / 4.

On base its 20 matrix runs give red the overall win by one seed (6 / 7 / 7). The same build over 100 seeds (the four
seeds of record and seeds 1 to 96, `matchup_trace --battle 31_15_marauders_vs_15_stalkers --seeds 96`) gives blue 46,
red 22, unresolved 32: blue is the battle's winner, and the 20-run red lead is a sampling tail. Every rule that touched
walks moved those 20 runs toward blue:

| Build | 20 runs (blue / red / unresolved) | 100 runs |
|---|---|---|
| base | 6 / 7 / 7 | 46 / 22 / 32 |
| D-2 (first plan, evidence §3) | 10 / 4 / 6 | not measured |
| D-3 + D-4 (first plan, evidence §4) | 13 / 5 / 2 | not measured |
| A1 | 9 / 1 / 10 | 43 / 13 / 44 |
| A1s | 10 / 6 / 4 | 38 / 21 / 41 |
| A5 | 11 / 3 / 6 | 46 / 21 / 33 |
| B1 | 14 / 2 / 4 | 54 / 9 / 37 |
| K3 | 11 / 3 / 6 | 50 / 22 / 28 |
| K4 | 14 / 2 / 4 | 54 / 20 / 26 |
| A1 + K4 | 14 / 2 / 4 | 49 / 17 / 34 |

No other matrix battle has a decisive split within one seed: the only other battles within two seeds are five that end
unresolved on all 20 runs (22a, 37a, 37b, 38b, 39b). So stop rule 1 read on 20 runs stops every rule that changes how a
marauder or stalker walks, whatever the rule does. Read on 100 runs, no rule tried flips battle 31. **OQ-1** asks the
orchestrator which reading binds (§5, D-3).

### 2.4 What the three withdrawn rules teach

| Rule | What it did | Traced failure | Lesson used here |
|---|---|---|---|
| D-1 the walk never steps back (`crowd-edge-wp2-wip`, `remove_blocked_velocity_ahead`) | Projected the walk's across step onto `H₀ = {v · ahead ≥ 0}` and the bodies' half-planes, cyclically, bounded | T1: a retreat out of a pocket between two firing friends was walker 12's only way in; it stood pinned. T3: the bounded projection leaves `sin²φ` of the step per pass, so the walk pressed into the body ahead (corridor contact reversals 3 → 26). T4: `H₀` binds while an arc forms. The exact projection fixed T3 but cost more reach (walkers 9 / 17) | Do not constrain the retreat. Constrain what the retreat makes: the room that ends the walk. Never project approximately where the result must be admissible |
| D-2 no step ends deeper in a friend that stood (`crowd-edge-wp3-wip`, `body_settle.rs`) | A settle stage after all proposals: `remove_blocked_velocity` from the tick-start position against friends that stood, alternated with the terrain settle | Battle 31 flipped (20 runs); proposal reversals ×1.3; showcase seeds 2, 6, 8 flipped and seeds 1, 3 did not end; corridor 1 of 6 and 2 give-ups | The step into a friend that just stopped is what feeds collision's `slide_around_fixed`, whose 0.85 tangent carries units round fighting friends. Removing the step removes the slide. B1 (this plan) confirms it: restricted to anchored friends, the showcase default seed flips (§4.2) |
| D-3 re-arm on ground gained (`crowd-edge-wp4-wip`) | Removed the timed retry; re-armed only when `note_progress` reset the stall count | Battle 31 flipped (20 runs) and gave up an order on seed 11 (first divergence tick 130/135, red stalker 23 standing spent where the retry walked it) | Re-arming needs an event for "a side opens". In a corridor a side is always open somewhere (the unit has walked away from it), so the event must be the side that closed, opening, as a whole free step (K3 versus K4, §4.3) |

---

## 3. What StarCraft II does

Tags as in the precedent plans: **[DOC]** Blizzard material; **[DATA]** SC2GameData 5.0.14; **[COMM]** community or
editor observation; **[2ND]** second-hand account; **[INF]** this plan's inference; **verify** where nothing confirms
it. Sources S1 to S10 are those of the first plan's §3 (data fetched 2026-11-19, copy under
`~/scratch/crowd-edge-plan/data/`); nothing new was found in 5.0.14's data, which has no crowd-steering field.

### 3.1 What the sources say

| # | Source | Bearing on this plan |
|---|---|---|
| S1 to S4 | `CGame`, `CMoverAvoid "Ground"`, `CUnit` defaults, `CAbilMove` / `CAbilAttack` (5.0.14 data) [DATA] | No parameter for local avoidance, stuck time, re-trying or giving up. Crowd behaviour is engine code, known only by observation |
| S5 | Blizzard, *Game Guide: Unit Positioning* [DOC] | Units left outside a concave **stand** until the player moves them |
| S6 | 5.0.14 patch notes, Ultralisk size [DOC] | Being stuck behind allies is a known state, solved by size, not by walking back and forth |
| S7 | SC2Mapster thread 158471 [COMM] | Units are blocked by friends that are attacking or holding position |
| S9 | GDC 2011, Anhalt, as summarised [2ND] | Unit-blind global pathing with local steering; a unit that cannot slide past others repaths |
| S10 | Q0074, the owner's account of `CMoverAvoidance` | Second-row attack-movers slide along the back of the front line into an open arc; completely boxed-in units stand still cleanly |

### 3.2 What follows

- **Sliding round a crowd may go backward, but does not go back and forth** [S10, INF]. A unit sliding along the back
  of a front line follows the line's shape; where the line bends back, so does the slide. No source has the unit step
  back, then forward into the same contact, then back. D-1 keeps the slide, backward parts included (T1), and forbids
  only the re-entry into the room the slide itself made. Whether the game's slide ever ends on such room is
  **verify** (engine code; no source).
- **A boxed-in unit stands** [S5, S10]. A unit whose walk has tried both sides stands. It moves again when the crowd
  changes so that it can: the side that stopped it opens, or it gains ground (the front advanced or a friend died).
  A timer that walks it every second, whatever the crowd does, is not in any source. D-2 replaces the timer by those
  two events. That the game re-evaluates on such events rather than on a period is **verify** (engine code).
- **Contact does not push units back and forth** [S7, INF]. R-B and R-C are artefacts of our simultaneous update and
  our contact stage. No rule for them is planned (§4.2, §9); they stay registered under B-103.

### 3.3 Sanctioned deviations

Q0052's S-1 to S-7 touch none of this. The surround walk is Q0074's "allows more"; this plan changes when it ends and
when it re-arms, not whether it exists.

---

## 4. Candidates and prototypes

All prototypes are scratch builds of `76a7ac67` (worktrees `~/scratch/crowd-edge-2/proto`, `proto2`, `proto3`,
detached, never committed), measured by `~/scratch/crowd-edge-2/scripts/eval.sh <name>`: release tools, the matrix
with `--compare` to base, battle 33 on 64 seeds, the showcase on the default seed and seeds 1 to 8 (3,000 ticks),
battle 31 on 100 seeds, then `cargo test --release -p sim_core --test second_line_tests --test crowd_edge_tests
--no-fail-fast -- --include-ignored`. Classes come from the probe tree `~/scratch/crowd-edge-2/probe`
(`scripts/probe.sh`). Each candidate's source diff is `~/scratch/crowd-edge-2/patches/patches-<name>.diff`, and its
measurements are under `~/scratch/crowd-edge-2/<name>/` (`summary.txt`, `repro.log`).

**The stop rules applied** (the orchestrator's brief): no winner flip in the showcase default seed or in any matrix
battle overall; give-ups at most base + 1; total reversals of record below base. Added as guards, from the failures:
arc 10 / 10 and walkers 17 / 17 (T1, K-2), every showcase seed reported with its outcome, and the noise band of §2.2.

### 4.1 R-A: the walk retreat

**Mechanism (first plan §2.3, re-probed).** The showcase's 63 R-A reversals on nine seeds are the transitions
"forward → walk-across" (36) and "walk-across → forward" (27); 24 more contact reversals are walk-across on both
ticks. The second kind is the cycle: the walk's across step, projected round a body ahead on its side, gains a
backward part (−0.035 to −0.064 per tick); that retreat frees the straight step, the walk ends (friendly-blocking
Amendment 7: whole straight step free), the unit steps forward into the same contact, and walks again.

**Designs considered, each checked against the traced failures before building.**

| Candidate | Rule | Against T1 (pocket) | Against T3 (leak into bodies) | Verdict before measuring |
|---|---|---|---|---|
| D-1 (withdrawn) | Forbid the backward part | Fails | Fails (bounded) or costs reach (exact) | — |
| Earlier scratch (first plan evidence §2c) | Never end a walk on a tick whose last step went back | Keeps the retreat, but holds a walk that has truly rounded the body | No projection | Cost reach (walkers 16 / 17, showcase not over) |
| **A1** | The walk ends only when the straight way is free for `speed + flank_retreat`, where `flank_retreat` sums the walk's backward parts since it (re)started | Keeps the retreat; a walk that has rounded the body into real room ends as before (the room exceeds the retreat) | No projection; one more `remove_blocked_velocity` over a longer reach, whose result is only compared | Build |
| A1s | A1 with the last tick's backward part only (no state) | As A1 for a one-tick retreat; a retreat over several ticks makes room the test no longer sees | As A1 | Build, to see if the state is needed |
| A5 | A paused walk (walked, not active) steps forward only on a whole free straight step, as an active walk ends | Untouched | No projection | Build: the paused walk's half-step test re-creates the hysteresis gap Amendment 7 closed |

**Prototype figures** (base in brackets):

| Measure | A1 | A1s | A5 |
|---|---|---|---|
| Matrix reversals of record | **4,459** (6,154) | 6,151 | 6,838 |
| … proposal / contact | 1,685 / 2,774 (2,918 / 3,236) | 2,815 / 3,336 | 3,379 / 3,459 |
| Matrix all seeds | 21,946 (30,642) | 27,858 | 33,959 |
| Give-ups record / all | 2 / 5 (2 / 6) | 2 / **8** | 2 / **9** |
| Battles 26 to 29 | 917, 778, 1,004, 776 (1,024, 1,033, 1,393, 1,047) | 1,022, 969, 1,669, 1,144 | 1,224, 1,362, 1,447, 1,050 |
| Battle 35 | 273 (668) | 486 | 624 |
| Battle 31, 20 / 100 runs | 9-1-10 / 43-13-44 | 10-6-4 / 38-21-41 | 11-3-6 / 46-21-33 |
| Battle 33 (blue / red of 68) | 39 / 13 (47 / 16) | 41 blue | not measured |
| Showcase default | P1 at 1,935; 8 (4 + 4) / 9 (5 + 4) | P1 at 1,915; 8 / 11 | not measured (stopped on the matrix) |
| Showcase nine seeds, reversals | 174 (235); seed 6 not over at 3,000 (below) | 170; all P1 | — |
| Arc / walkers | 10 / 10 at 96 / 17 / 17, 16 (7 + 9) | 10 / 10 / 17 / 17, 23 | — |
| Corridor | 24 (6 + 18) | 39 (9 + 30) | — |

A1s's first build was bit-identical to base everywhere: it read the unit's own last step from the neighbour slice,
which apparently never holds the unit itself. The table shows the corrected build (`StepContext` carries the actor's last achieved
step).

**A1's class figures** (probe, showcase nine seeds): R-A 63 → 42, R-B 89 → 31, R-C 32 → 37, R-D 30 → 24, R-E 7 → 26,
R-F 14 → 14. "walk-across → forward" falls 27 → 10 and the contact walk-on-walk class 24 → 3; "forward → walk-across"
stays (36 → 32): A1 does not touch the walk's start. Corridor R-A 18 → 5; walkers 21 → 6. R-B falls as a side effect
(fewer re-entries into receding friends). **R-E rises 7 → 26** (`steer → steer` 20): units that walked longer meet
crossing friends more; R-E is B-144's class, outside this plan, and WP3 traces two of these events (**verify** the
cause).

**A1's costs.**
- **Showcase seed 6** does not end by tick 3,000: player 1's army is dead and two of player 0's pulse bots stand
  idle at their attack-move cell (48.3, 52.0) from tick 1,150 while player 1's structures stand. So seed 6 changes
  side (base: player 1 at 2,175). Seed 6 is not the default seed; it is reported, as stop rule 1 asks only for the
  default seed.
- **Battle 33** moves (blue 47 → 39 of 68, red 16 → 13), no flip.
- **Corridor contact reversals** rise 3 → 18 with the timed retry still walking the queue; with D-2 (K4) they are
  1 (§4.4).

**Verdict.** A1 is the only R-A candidate below the noise band (4,459 < 6,017) that keeps give-ups and the guards.
A1s shows the state is needed (a retreat over several ticks). A5 is rejected.

### 4.2 R-B and R-D: the step into a friend that stopped, and the overlap it leaves

**Mechanism (first plan §2.4).** A unit closing on a friend that walked last tick is allowed the room the friend
receded; the friend stops this tick (it came into range), the step ends inside it, collision's `slide_around_fixed`
moves the unit half out and sideways (capped by `SOFT_PUSH_SPEED_SHARE`), and on the next tick, the unit now waiting,
the rest of the overlap is pushed straight back: a reversal. R-D is the same overlap eased over later ticks.

**Why D-2 cost reach.** The tangent part of `slide_around_fixed` (`FIXED_BODY_TANGENT_WEIGHT` 0.85) is what moves a
unit round a friend that holds its ground. D-2 stopped the step before it overlapped, so no slide happened, and units
that used to flow round the firing line stood behind it.

**Candidate B1** (`patches-B1.diff`, the first plan's `body_settle.rs` with one filter): settle only against friends
that **hold their ground** (`SteeringObstacle::anchored`), on the hypothesis that D-2's reach cost came from waiting
friends, which contact pushes evenly and which carry the crowd forward.

| Measure | B1 | Base |
|---|---|---|
| Matrix reversals of record | 7,093 (3,744 + 3,349) | 6,154 |
| Battle 31, 20 / 100 runs | 14-2-4 / 54-9-37 | 6-7-7 / 46-22-32 |
| Showcase default seed | **player 0 wins at 2,595** | player 1 at 1,946 |
| Showcase seeds 1 to 4, 6, 7 | **not over at 3,000** | player 1 |
| Showcase reversals, nine seeds | 150 | 235 |
| Corridor | 1 of 6 in range, 7 reversals | 2 of 6, 21 |
| Probe, showcase nine seeds | R-A 71, R-B 39, R-C 14, R-D 10 | 63, 89, 32, 30 |

The hypothesis is refuted: the anchored friends are exactly the ones the slide carries units round. B1 halves R-B and
R-D and stalls the showcase. **No candidate holds the stop rules; R-B and R-D stay open (§9).** The design direction
recorded for the plan that takes them: make the slide round a friend that holds its ground a rule of locomotion (the
unit steps along the friend's tangent on the side its lane exits), so that a settle can then remove the overlap
without removing the way round. That changes the contact table of friendly-blocking D-4 and is not prototyped here.

### 4.3 R-C: the wall slide into a body

**Mechanism (first plan §2.5).** `after_wall_slide`'s step is never re-checked against bodies.

| Candidate | Rule | Matrix | Showcase nine seeds | Probe R-C (nine seeds) | Corridor |
|---|---|---|---|---|---|
| C1 | Re-check the slid step; wait if it is refused by bodies or the wall | untouched (no walls) | 340 reversals (235); every battle ends 150 to 1,040 ticks earlier | — | 1 of 6, 30 |
| C2 | Re-check, alternating body removal and wall slide up to `WALL_CONTACT_PASSES`; take the result if admissible, keep today's step if it does not converge | untouched | 217 (235); seeds 1 and 3 rise +9 and +14 for player 1; all player 1 | 52 (32) | 2 of 6, 22 (7 + 15) |

The matrix has no terrain walls, so neither candidate can be judged on it; on the showcase C2's change (−18 reversals)
is within the spread of its per-seed swings, and the probe's R-C count rises (the probe tags the slide before the
re-check, so the count is not reliable for C2; **verify**). C1 waits at walls far too often. **No candidate is shown to
hold; R-C stays open (§9)** with C2's figures. A measurement for it needs a wall-bearing matrix battle (out of scope).

### 4.4 Re-arming a spent walk, and the corridor

**Mechanism.** A walk is spent when both sides have closed for `FLANK_REVERSE_TICKS` each, or when it has walked one
circle round its target (an attack-move without a target: its scan range). Today it waits `FLANK_RETRY_TICKS` (24) and
walks afresh. In the corridor that walks the queue every second (standing 65 to 88 %); in an open fight it is how a
spent unit finds a front that moved (K-6).

**Designs.**

| Candidate | Re-arm on | Corridor (why) | Open fight (why) |
|---|---|---|---|
| D-3 (withdrawn) | ground gained, target change | stands | misses a side opening beside it (K-6) |
| K3 | D-3, plus: the walk was spent by closed sides (`flank_walked` below its bound) and the side that closed last now admits an across step above the waiting share | contact creep opens a sliver at the wall, so the walk re-arms every few dozen ticks | re-arms when the side opens |
| **K4** | as K3, but the side must admit a **whole** across step (at least `speed − STANDING_DRIFT`, the walk-end test of friendly-blocking Amendment 7, turned sideways) | the side that closed is the wall or a body pressed against it; a creep never frees a whole step | a side the crowd vacates frees a whole step |

A walk spent by its circle bound re-arms only on ground gained or a target change: re-arming it on an open side would
restart the circle at once.

**Prototype figures** (base in brackets):

| Measure | K3 | K4 | A1 + K4 |
|---|---|---|---|
| Matrix reversals of record | 6,198 | 6,111 | **4,659** (6,154) |
| … proposal / contact | 2,931 / 3,267 | 2,883 / 3,228 | 1,762 / 2,897 |
| Matrix all seeds | 30,700 | 30,629 | 22,666 (30,642) |
| Give-ups record / all | 2 / 6 | **3 / 7** (battle 31 seed 11) | 2 / 5 (2 / 6) |
| Battles 26 to 29 | 1,021, 1,045, 1,408, 1,036 | 1,021, 1,045, 1,353, 1,036 | 895, 807, 1,268, 717 |
| Battle 35 | 668 | 668 | 273 |
| Battle 31, 20 / 100 runs | 11-3-6 / 50-22-28 | 14-2-4 / 54-20-26 | 14-2-4 / 49-17-34 |
| Battle 33 | 47 / 68 | 47 / 68 | 39 / 68 |
| Showcase | identical to base | identical to base | as A1 (174; seed 6 not over) |
| Arc / walkers | 10 / 10 / 17 / 17, 31 | 10 / 10 / 17 / 17, 31 | 10 / 10 / 17 / 17, 16 |
| Corridor reversals | 21 (17 + 4) | **7 (6 + 1)** | **3 (2 + 1)** |
| Corridor standing within `STANDING_DRIFT`, rear units 3, 4, 5, 7 (base 88, 88, 69, 65 %) | 81, 94, 87, 80 % | **98, 99.7, 96, 97.5 %** | 98, 99.7, **87**, **94.9** % |
| CE-7, CE-8, CE-9 | pass, pass, pass | pass, pass, pass | pass, pass, pass |

**K4's give-up.** Battle 31 seed 11 (of record) gives up one chase; its first divergence from base is tick 130 (blue
units 7 and 11), as in D-3's trace. Within stop rule 2 (base + 1); WP2 traces it to the unit and records it. With A1
too, the give-up is gone (2 / 5).

**The corridor under A1 + K4.** Units 5 and 7 stand on 283 of 324 (87 %) and 300 of 316 ticks (94.9 %), below the
test's 95 %. Unit 5's moving ticks after its first stand (probe, `full_corridor.txt`) are two walks while the queue
forms: ticks 70 to 114, in which A1 holds the walk while it steps back along the wall instead of ending it, and ticks
145 to 159, re-armed by the ground it gained stepping forward on ticks 127 to 132. From tick 160 it stands spent to
tick 400, with 1 reversal in all. This is not the queue walking on a timer (that is gone), but the test counts it.
§5 D-2.4 records the trade (OQ-2).

**Verdict.** K4 holds stop rules 2 and 3 (6,111, below base but inside the noise band) and, on the 100-seed reading,
stop rule 1, and brings the corridor's standing to the 95 % the first plan asked for. K3 does not hold stop rule 3.
A1 + K4 holds all three stop rules on the 100-seed reading, below the noise band, but gives back part of the
corridor's standing (OQ-2).

---

## 5. Design decisions

### D-1. A walk does not end on the room its own retreat made (R-A)

1. **State.** `MovementMemory::flank_retreat: SimScalar`, world units: the sum, over the walk steps taken since the
   walk last became active, of each step's backward part along the way to the goal, `max(0, −step · ahead)`, where
   `ahead` is the unit direction to `next` on that tick. It is cleared when the walk becomes inactive (the unit steps
   forward), by `end_surround_walk`, and by `MovementMemory::restart` (it is not one of the fields `restart` keeps).
2. **Rule.** In `surround_walk`, a walk in progress ends when the straight way in is free for
   `reach = speed + flank_retreat`: `remove_blocked_velocity(body, ahead × reach, neighbours)` keeps at least
   `reach − STANDING_DRIFT` along `ahead`. With `flank_retreat = 0` this is today's test, bit for bit (friendly-blocking
   Amendment 7).
3. **What it changes.** The walk's own step, its start test, its side, its closed-side and spent rules, the paused
   walk's forward step and steering are all unchanged. Only the end of an active walk is judged further ahead, by
   exactly the distance the walk stepped back. A retreat out of a pocket is still taken (T1). A walk that has rounded
   the body into real room ends as before, because the room exceeds the retreat.
4. **Why not a timer or a count.** A walk that must "keep walking for N ticks" would be a tuning constant (U-23); the
   retreat is the measured quantity that made the false room.
5. **Determinism.** Fixed-point dot products and a saturating sum; no float, no allocation.
6. **Checksum.** The field is hashed with the rest of `MovementMemory` (`checksum/encoding.rs`, `fields!`);
   `CHECKSUM_SCHEMA_VERSION` 30 → 31. Snapshot and replay formats that serialise `MovementMemory` take the field
   (**verify** each reader in WP3; serde default for older snapshots if any are read).

### D-2. A spent walk re-arms when the side that closed opens, or when the unit gains ground (re-arm, corridor)

1. **Rule.** `FLANK_RETRY_TICKS` and the spent walk's timed retry are removed. A spent walk (`flank_spent`) re-arms
   (`end_surround_walk`, keeping `flank_left`) on any of:
   - **the side that closed opens**: the walk was spent by closed sides (`flank_walked` below the walk's bound, the
     same bound `surround_walk` spends it on), and on this tick the across step on the side it kept,
     `remove_blocked_velocity(body, across × speed, neighbours)`, keeps at least `speed − STANDING_DRIFT` along
     `across` and the disc probe permits it;
   - **ground gained**: `note_progress` reports progress (`note_queued_progress`, D-4's function, re-arms when its
     `gained` is true);
   - **a new target or order** (`flank_target`, as today).
2. **Why the side that closed last.** When a walk is spent by closed sides, the side it walked last is the one that
   just closed against it; the other side closed earlier, and the unit has since walked away from it, so it is
   usually open again and says nothing about the crowd. The last side reopening is the event "the crowd changed beside
   this unit" (K-6).
3. **Why a whole step.** A unit pressed against a wall or a standing body is eased off it by contact by slivers
   (first plan §2.7). A sliver above the waiting share re-opened the corridor's walls every few dozen ticks (K3,
   standing 80 to 99 %). The whole step is the walk-end test of friendly-blocking Amendment 7 applied to the side,
   so no constant is added.
4. **The corridor's standing under D-1 (OQ-2).** WP2 un-ignores `no_room_means_waiting_still` (K4 alone: rear units
   96 to 99.7 %). D-1 then breaks it: units 5 and 7 stand on 87 and 94.9 %, because a walk that steps back along the
   wall now continues instead of ending, and a second walk re-armed by ground gained does the same (§4.4). A
   restatement that counts from the first tick a unit's walk is spent was prototyped and does not save it (unit 5:
   254 of 275 ticks, 92 %), so no restatement is proposed: that would loosen the bound. The trade is real: D-1 takes
   the corridor's reversals from 7 to 3 and the matrix's by a further 24 %, and costs the corridor's standing. **OQ-2**
   asks the orchestrator whether WP3 may land with the corridor's standing bound at the measured figure (default:
   **no**, and WP3 does not start; D-1 then stays open with these figures).
5. **State.** No new field; the re-arm reads `flank_walked`, `flank_left` and the neighbours already gathered.
6. **Cost.** One `remove_blocked_velocity` and one disc probe per spent unit per tick behind a standing friend
   (only units that reach the spent branch).

### D-3. Stop rule 1 for battle 31 (OQ-1)

The orchestrator decides which reading binds:

- **(a) strict, the default:** battle 31's winner over its 20 matrix runs. Then D-1 and D-2 do not hold (§2.3), WP2 and
  WP3 do not start, and both stay open under B-103 with §4's figures.
- **(b) 100 seeds for a battle whose 20-run split is within one seed:** battle 31 is judged on
  `matchup_trace --battle 31_15_marauders_vs_15_stalkers --seeds 96`, against base's blue 46 / red 22. Today only
  battle 31 qualifies. A flip on 100 seeds still stops a package.

This plan recommends (b): the 20-run verdict is decided by one seed, every perturbation tried moves it, and the battle's
own claim allows either winner. The decision is the orchestrator's by amendment; it is not filed as an owner question
unless the orchestrator wants the owner to rule.

### D-4. Measurement method (all rule WPs)

As the first plan's D-8, with three changes:

1. Battle 31 on 100 seeds is part of every measurement (`measure2.sh`).
2. The noise band of §2.2 is re-measured on WP1's tree and printed with every comparison; a reversal claim is made
   only below its floor.
3. The class probe's patch is kept in step with the tree (`~/scratch/crowd-edge-2/probe-base.patch`, conflict
   resolver `scripts/resolve.py`), so each WP reports classes.

**Stop rules** (a breach stops the WP; nothing beyond it is committed; the trace names first divergent tick, entity
and rule):
1. **Winner:** the showcase default seed (player 1) and every matrix battle's overall winner, battle 31 read per OQ-1.
2. **Give-ups:** matrix of record and all seeds at most base + 1; every showcase seed at most base + 1.
3. **Reversals:** matrix of record below base, and below the noise floor for D-1.
4. **Guards:** arc 10 / 10, walkers 17 / 17, deeper unit-ticks 0 on every showcase seed.
5. **Controls:** the rule switched off by a scratch flag reproduces the previous tree byte for byte (matrix, battle 33
   `seeds.csv`, showcase `units.csv` on nine seeds).
6. **Tests:** a test that changes for another reason than its WP's rule is restated first, test-only, with the trace.
   No bound is loosened without a trace, and no tuning constant is added.

---

## 6. Work packages (commit order)

Branch `crowd-edge-2`. Implementation agents hand back after `cargo gate quick` and the tests their change needs; a
verifier runs `cargo gate package` on the final tree (Q0061). Each WP appends to a new evidence file
`docs/evidence/2026-11-19-crowd-edge-steering-2.md`. Sizes: S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- This plan's row in `docs/plans/README.md` is added with the plan (`Proposed`).
- `DESIGN.md` B-103: the pointer "a new plan … is still to be written" becomes this plan.
- The first plan gets a one-line pointer amendment (its open items are carried here).
- Side branches `crowd-edge-wp2-wip`, `crowd-edge-wp3-wip`, `crowd-edge-wp4-wip` are deleted once WP0 is merged
  (the first plan's Amendment 3, item 3); their diffs are kept as files under `~/scratch/crowd-edge-2/patches/` and
  their findings are in the first plan's evidence.
- **Gate:** docs-only `package`.

### WP1: measurement tooling, bit-identical (S)

- **Change (scratch scripts made reproducible, no simulation change).** The evidence file's "How to reproduce"
  documents `measure2.sh` (battle 31 on 100 seeds) and the placebo band; the band is re-measured on WP1's tree.
- **Tests first.** None in code: WP1 changes no crate. If the orchestrator wants battle 31's wide sample in
  `matchup_trace` (a `--battle-seeds` override per battle), that is a `sim_fixtures` change with unit tests, and only
  then is it a code WP.
- **Gate:** docs-only `package` unless the option above is taken.

### WP2: D-2, re-arm on the side that closed (M) — gated on OQ-1 (b)

- **Tests first** (red on WP1):
  - `a_queued_chaser_keeps_its_target` (CE-7) and `a_spent_walk_resumes_when_the_way_opens` (CE-8) are un-ignored,
    with CE-8's re-arm assertion (the walk re-arms within six ticks of the way opening). On WP1: CE-7 stands 386 of
    580 ticks; CE-8's walk does not re-arm.
  - `no_room_means_waiting_still` is un-ignored as it stands (it passes on the K4 prototype: 96 to 99.7 %).
  - **CE-10** `a_spent_walk_re_arms_when_its_closed_side_opens` (`crowd_edge_tests.rs`): two firing shooters at the
    pillar and a chaser behind them whose walk is spent with its kept side closed by a third friend standing beside
    it (scratch archetypes); the third friend is then ordered away. The walk re-arms on the tick its side frees a
    whole step, and not before; on WP1 it re-arms on the timer (24 ticks after spending), whatever the friend does.
  - **CE-11** `contact_creep_does_not_re_arm_a_walk`: the corridor, one chaser spent against the wall, a friend
    pressing it so contact eases it off the wall by slivers. Over 300 ticks its walk never re-arms. Fails on a K3
    tree; passes on K4 (guards §5 D-2.3).
  - Unit test in `locomotion.rs`: a walk spent by its circle bound does not re-arm on an open side.
  - `a_blocked_move_is_still_given_up` (CE-9) stays green.
- **Change.** D-2. `FLANK_RETRY_TICKS` removed from `tuning.rs`; `surround_walk`'s doc, `MovementMemory::flank_spent`'s
  doc and friendly-blocking Amendment 4 change 5's pointer updated.
- **Measure.** D-4, base WP1. Prototype expectation (K4): matrix 6,111 of record, give-ups 3 / 7 (battle 31 seed 11),
  showcase bit-identical, battle 31 on 100 seeds blue 54 / red 20, corridor 7 reversals. **Trace** battle 31 seed 11's
  give-up to its unit and rule and record it (stop rule 2 allows it).
- **Goldens.** `golden_replay` and `golden_replay_orders` are re-recorded only if they move, on the final tree, native
  debug and release, `wasm32-wasip1` release and debug once, with the header form of the first plan ("Re-recorded for
  plan 2026-11-19-crowd-edge-steering-2 WP2 (B-103): checkpoint N first changed; entity E at tick T …. Proof: a scratch
  build of WP2 with the rule off reproduces the previous checksums at every checkpoint."). `DEFAULT_SANDBOX_CHECKSUM`
  likewise. A golden that does not move is recorded as a finding.
- **Throughput.** `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`, base and candidate
  alternated, three rounds; every scenario ≥ 25 ticks/s.

### WP3: D-1, the walk does not end on its own retreat (M) — gated on OQ-1 (b), OQ-2 yes and WP2

- **Tests first** (red on WP2):
  - **CE-12** `a_walk_does_not_end_on_the_room_its_retreat_made` (`crowd_edge_tests.rs`): the walkers layout, with
    one walker placed from pulse bot 4's trace (first plan §2.3) so its walk steps back round a body ahead on its side.
    Assertions: over 150 ticks no "walk step → forward step" pair reverses for that walker (read through
    `proposed_step` and its memory), and its retreat is still taken (its walk's first step has a backward part:
    T1 kept). On WP2 the pair reverses.
  - **CE-13** `a_walk_out_of_a_pocket_still_reaches_range` (T1 guard): walker 12's pocket of the first plan's
    evidence §2b (T1), reproduced with scratch archetypes; it reaches range by tick 300. Passes on WP2; must pass after.
  - **CE-14** `a_walk_without_retreat_ends_as_before`: the arc reproduction's walk steps are bit-identical to WP2's
    over 120 ticks (its walks do not step back once the arc has formed: from tick 98, first plan evidence §2b T4).
  - Unit tests: `flank_retreat` sums only backward parts; it is cleared by a forward step, `end_surround_walk` and
    `restart`; with `flank_retreat = 0` the end test equals today's.
  - `walkers_at_the_crowds_edge_do_not_toggle` tightens from ≤ 10 per walker to ≤ 3 per walker and ≤ 16 in total
    (A1 + K4 prototype: per walker at most 3, total 16), recorded as measured, never loosened later.
  - `no_room_means_waiting_still`: kept as WP2 left it unless OQ-2 is answered yes; then its standing bound is
    restated in a test-only commit first, to the figure the orchestrator sets, with D-2.4's trace in its doc.
    `no_room_means_no_reversals` stays ignored with the measured figure (prototype 3).
- **Change.** D-1; `CHECKSUM_SCHEMA_VERSION` 31 with its test; `MovementMemory` doc.
- **Measure.** D-4, base WP2. Prototype expectation (A1 + K4): matrix 4,659 of record (1,762 + 2,897), give-ups 2 / 5,
  battles 26 to 29 at 895, 807, 1,268, 717, battle 35 at 273, battle 31 on 100 seeds blue 49 / red 17, battle 33 blue
  39 of 68, showcase default player 1 at 1,935 with 8 / 9 reversals, nine seeds 174, seed 6 not over (traced and
  recorded as above). Class probe on the default seed and the reproductions. **Trace** two of the new R-E events
  (§4.1) and record the cause.
- **Goldens.** As WP2; `golden_replay` is expected to move (its attack-moving squads walk in ticks 1 to 50,
  friendly-blocking Amendment 7). The schema bump re-records every checksum fixture that encodes the schema
  (**verify** which in WP3).
- **Throughput.** As WP2.

### WP4: storyboard and bookkeeping (S)

- **Storyboard.** WP3 moves the showcase's default-seed timeline (ends at 1,935, not 1,946), so
  `assets/captures/showcase.ron` is re-checked with a real render on the RX 580, `with-gpu-lock` per take (charter §7);
  `showcase_storyboard_tests` and `showcase_tests` pass.
- **`DESIGN.md`.** B-103 restated with what landed and what stays open (R-B, R-C, R-D, the walk's start retreat
  "forward → walk-across", R-E's rise), each with its figure; the Movement row names D-1 and D-2 and the new tests;
  §5 only if throughput moved beyond noise.
- **Plans.** This plan's row `Done` or `Partly done` with commits; friendly-blocking's Amendment 4 change 5 marked
  superseded by D-2.
- **Gate:** `merge` by the orchestrator.

If OQ-1 is answered (a), WP2 to WP4 are replaced by one docs WP: B-103 restated with §4's figures and this plan
`Partly done` (WP0, WP1 only).

---

## 7. File ownership

| Area | Files | WP |
|---|---|---|
| Records | `DESIGN.md` §4, `docs/plans/README.md`, first plan's pointer amendment | WP0, WP4 |
| Evidence | `docs/evidence/2026-11-19-crowd-edge-steering-2.md` (new) | WP1 to WP4 |
| D-2 | `crates/sim_core/src/movement/locomotion.rs` (`surround_walk`, `note_queued_progress`), `crates/sim_core/src/movement/tuning.rs` (`FLANK_RETRY_TICKS` removed), `crates/sim_core/src/entity.rs` (`flank_spent` doc) | WP2 |
| D-1 | `crates/sim_core/src/movement/locomotion.rs` (`surround_walk`, `end_surround_walk`), `crates/sim_core/src/entity.rs` (`MovementMemory::flank_retreat`), `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/checksum.rs` (schema 31) | WP3 |
| Tests | `crates/sim_core/tests/crowd_edge_tests.rs`, `crates/sim_core/tests/second_line_tests.rs`, unit tests in `locomotion.rs`; `crates/gate/suites.ron` only if a test is measured slow (C22) | WP2, WP3 |
| Goldens | `crates/sim_core/tests/fixtures/golden_replay*.ron`, `sim_fixtures` sandbox checksum | WP2, WP3 |
| Storyboard | `assets/captures/showcase.ron` | WP4 |

**Coexistence.** No other open plan owns `surround_walk` or the checksum schema (**verify** at WP2's start against
`docs/plans/README.md`'s `Active` rows). A package that bumps the schema in parallel is merged first and WP3 takes the
next number.

---

## 8. Determinism, checksum and throughput

- D-1 adds one hashed `SimScalar` (schema 31); D-2 adds nothing and removes a branch and a constant.
- Fixed-point only, no new collection, no allocation (C7, C9); no identity in Rust (C5): tests use scratch RON
  archetypes as `second_line_tests.rs` does.
- Cross-platform: CI's determinism jobs on the push; WP2 and WP3 run `golden_replay_tests` on `wasm32-wasip1` release
  and debug once.
- Throughput: D-1 adds one `remove_blocked_velocity` call over a longer reach per active walking unit on ticks it
  could end its walk; D-2 adds one per spent unit behind a standing friend and removes the timed walks. The prototypes
  were not benched; each WP benches (§6).

---

## 9. Out of scope and open classes (Q0002)

Nothing left out is accepted as a deviation by being left out; each stays under B-103 with its figure.

| Item | State | Best candidate and figures | Why not planned |
|---|---|---|---|
| **R-B** stale receding allowance | Open | B1 (anchored-only settle): matrix 7,093 of record, showcase default seed flips to player 0, six seeds not over | Every settle removes collision's slide round fighting friends (§4.2). Needs a plan that makes that slide a locomotion rule first |
| **R-D** leftover overlap | Open | B1: probe 30 → 10 on nine seeds, with B1's costs | Downstream of R-B (and of friendly-blocking D-4's easing of firing pairs, kept) |
| **R-C** wall slide into a body | Open | C2: matrix untouched, showcase 217 (235), probe R-C not reduced | Not measurable on the matrix (no walls); needs a wall-bearing battle in the matrix or a reproduction of its own |
| **The walk's start retreat** ("forward → walk-across", 32 of nine seeds under A1) | Open | — | Bounding the start's backward part is D-1 of the first plan (T1, T3); not retried |
| **R-E** crossing body (B-144), and its rise under D-1 | Open, B-144 | — | Its own plan; WP3 traces two events of the rise |
| **R-F** route adoption (B-145) | Open, B-145 | — | Group-routing's ground |
| **If OQ-1 is (a)**: R-A and the re-arm | Open | A1 + K4: §4.4 | Battle 31's 20-run winner |
| **If OQ-2 is no**: R-A | Open | A1 on WP2's tree (prototype A1 + K4) | The corridor's standing (unit 5: 96 → 87 %) |
| `SeparationRadius` (B-49), lateral acceleration (B-92), U-23 tuning | Unchanged | — | — |

---

## 10. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| K-1 | OQ-1 is answered (a) | No rule lands; B-103 keeps its residuals | §4's figures are recorded; the gated WPs are dropped cleanly (§6) |
| K-2 | Battle 31 flips on 100 seeds in the real implementation | Stop rule 1 (b) | The prototypes left blue ahead by 27 to 45 seeds; a flip is traced with the rule-off control |
| K-3 | D-1 lengthens walks where the way round goes backward along a wall | Corridor units 5 and 7 at 87 and 94.9 % | OQ-2 (default: WP3 does not start); corridor reversals fall 21 → 3 |
| K-4 | D-1 changes which side wins showcase seed 6 | A non-default seed changes side | Reported; the storyboard (default seed) is re-checked in WP4 |
| K-5 | D-1 moves battle 33 (blue 47 → 39 of 68) | Close marine–zealot balance moves | No flip; recorded |
| K-6 | D-2 misses a crowd change that frees neither the closed side nor ground | A spent unit stands beside an opening on its other side | Ground gained and target changes still re-arm; CE-10 and CE-11 bound the two sides of the test; a further event only by amendment |
| K-7 | R-E rises under D-1 (7 → 26 on nine seeds) | More crossing-body reversals | Below the totals' fall; traced in WP3; B-144 owns the class |
| K-8 | Schema 31 collides with another package's bump | Merge conflict in fixtures | Coexistence rule, §7 |
| K-9 | The prototypes' figures do not reproduce in the clean implementation | Expectations wrong | The diffs are kept (`patches-A1K4.diff`); a mismatch is traced against them before any bound is set |

---

## 11. Prototype material (scratch, not committed)

- Worktrees: `~/scratch/crowd-edge-2/proto`, `proto2`, `proto3` (detached at `76a7ac67`), `probe` (with the probe).
- Scripts: `~/scratch/crowd-edge-2/scripts/eval.sh`, `measure2.sh`, `summary.py`, `probe.sh`, `resolve.py`.
- Diffs: `~/scratch/crowd-edge-2/patches/patches-{A1,A1s2,A5,B1,C2,K3,K4,A1K4,placebo1,placebo2,placebo3}.diff`
  (C1's is reproduced by C2's without the alternation).
- Results (A1s under the name `A1s2`): `~/scratch/crowd-edge-2/<name>/summary.txt`, `repro.log`, `b31/`; probe classes
  `~/scratch/crowd-edge-2/probe-out/<name>-9.classes`.

## Amendment 1 (orchestrator, 2026-11-19): OQ-1 and OQ-2 decided

**OQ-1: yes, the 100-seed reading.**

- **When it applies.** Stop rule 1 judges a matrix battle on 100 seeds when its 20-run base split is within one
  seed between the two sides. Today that is battle 31 only, at 6 / 7 / 7. WP1 adds the method and names the
  battles it covers.
- **When the rule is met.** It holds when the 100-seed winner is unchanged and the winner's share does not fall by
  more than the placebo band.
- **Why.** Every candidate that moves the matrix, the placebos included, flips battle 31's 20-run winner, while
  over 100 seeds blue wins 46 to 22 in the base and in every candidate. The 20-run reading of a battle that is
  decided by one seed measures the seed draw, not the rule. The battle's own claim says either side may win.
- **The placebo band.** An effect on reversals counts only below the band's lower edge (6,017 of record), as the
  plan says.

**OQ-2: yes, with a recorded residual.**

- **What D-1 costs.** D-1 (A1) may land although one corridor unit's standing falls from 96 to 87 %. That is still
  above the 65 to 88 % on main before D-2, and corridor reversals fall from 7 to 3.
- **How the corridor test is handled.**
  - WP2 un-ignores the corridor standing test at K4's figures.
  - If WP3 makes the test fail for that unit, WP3 does not loosen the bound. It re-ignores the test with the
    measured figures in its reason, and records the residual in the B-103 row as open.
  - Every other corridor and walker bound stays as planned.

**Added to WP3: trace showcase seed 6.** A1 leaves it unfinished by tick 3,000, with player 1's army dead. WP3
traces why player 0 does not end the match. A stall at a structure, a give-up loop or a wedge is a defect and
stops the WP. A slow but progressing siege is reported as rule-caused.

**Housekeeping.** The prototype worktrees under `~/scratch/crowd-edge-2/` and their target directories are removed
once WP3 is measured. WP0 deletes the three `crowd-edge-wp*-wip` side branches, as planned.

## Amendment 2 (orchestrator, 2026-11-19): two survivor ranges restated; WP3 and WP4 land

**Why.** WP3 stopped on `every_battle_keeps_its_recorded_outcome_on_every_seed`, which the plan's stop rules did
not name. Two seeds of record leave the battle's survivor range without a winner flip:

- **Battle 26, seed 33:** the colossi win with 2 left, against at least 4.
- **Battle 36b, seed 22:** the marines win with 12 left, against 9 to 11.

Both are traced to D-1 (evidence on `crowd-edge-2-wp3-wip`). With D-1 off, the matrix reproduces WP2 byte for byte.
In both, a unit's walk continues where it used to step forward. This is the rule as designed. WP3 otherwise holds
every amended stop rule:

- reversals of record fall from 6,154 to 4,659, below the placebo floor;
- give-ups are 2 / 5;
- no overall matrix flip;
- battle 31 on 100 runs gives blue 49 / 17 against the base's 46 / 22;
- the showcase default seed holds;
- seed 6's change is a slow, progressing siege.

**Decision: option 1.** The two ranges are restated, each with its trace in the fixture comment, as the fixture's
header asks.

- **Battle 26** (`26_100_lings_vs_6_colossi`): red `(2, 999)`. The claim "six colossi vaporize a hundred zerglings"
  still holds on every seed of record, and the colossi win on all four. A zergling crowd that surrounds instead of
  retreating is closer to the game.
  - **Watch item:** on all seeds the zerglings win seed 16 (not a seed of record) with 8 left. That is recorded in
    the B-103 row and the evidence. If a later change makes the zerglings win a seed of record, the claim itself
    must be re-examined, not the range widened again.
- **Battle 36b** (`36b_marines_beat_zealots_without_storm`): red `(9, 12)`. Its range was empirical: the control's
  recorded spread, not a StarCraft II figure.
- **The other misses outside the seeds of record** (battle 26 seeds 4, 10 and 16; battle 33 seeds 8 and 9; battle
  36b seed 8; battle 38a seed 2) are listed in the evidence. They are not bounds of the test.

WP3 and WP4 then land from `crowd-edge-2-wp3-wip` onto `crowd-edge-2-plan`. The plan's stop rules gain one line for
future movement plans: a seed of record leaving a battle's survivor range is a stop, like a flip, and is decided by
amendment.
