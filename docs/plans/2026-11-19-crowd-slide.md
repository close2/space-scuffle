# Blueprint: Crowd Slide: The Slide Round a Fighting Friend as a Locomotion Rule, Then a Settle (B-103, R-B and R-D)

This plan takes the two contact-caused reversal classes of bug **B-103** (`DESIGN.md` §4) that plan
[`2026-11-19-crowd-edge-steering-2`](2026-11-19-crowd-edge-steering-2.md) left open (§9 there):

- **R-B, the stale receding allowance.** A unit closing on a friend that walked last tick is allowed the room that
  friend receded; the friend stops this tick, the step ends inside it, and the collision phase pushes the unit back
  out.
- **R-D, the leftover overlap.** Overlap that R-B (or a partner's step) left is eased out over later ticks, sometimes
  back and forth with a third body.

That plan's §4.2 concluded that every "settle" stage, which stops a step before it overlaps a friend that stood,
also removes the collision phase's slide round fighting friends (`slide_around_fixed`), which carries the crowd
round its firing line. The direction it recorded: make that slide a locomotion rule first, proposed before collision,
and only then settle. This plan prototypes that direction and its variants.

**Status of this document.** This is a Stage 1 blueprint under charter §6, written on 2026-11-19 against `main` at
`6b922732` (checksum schema 31). Line numbers drift, so search for the quoted symbol. Claims not confirmed while
planning are marked **verify**. Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** Any rule for R-B or R-D moves battle outcomes and `golden_replay`, and the work
follows two withdrawn settle rules (first plan D-2, second plan B1) whose failure has to be designed out first.

**Result in one line.** **No candidate holds the stop rules.** Nine designs were prototyped and measured (§4); none
both keeps the showcase, the guards and the survivor ranges and lowers the matrix's reversals of record below the
placebo floor. This plan therefore has **no rule work package**. It records the measurements, a sharper diagnosis
and two method findings (§2.2, §2.3), and leaves R-B and R-D open under B-103 with the best candidates' figures
(§5, §9).

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §3, §5 |
| [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) sanctioned deviations | None touches contact between friends | §3.3 |
| [Q0074](../questions/closed/0074-surround-walk.md) the surround walk stays; second-row units slide along the front line | Answered, option 1 | §3, §4 |
| Q0061 hand back after `cargo gate quick`; a verifier runs `package` | Standing rule | §6 |
| Q0002 scoping is the agent's call if written down | Standing rule | §9 |
| `docs/questions/open/` at the start of this session | Empty | — |

No owner question is raised: nothing here needs the owner's decision, because no rule is proposed.

---

## 1. Summary

1. **Base (§2.1).** `main` at `6b922732` reproduces the second plan's WP3 tree bit for bit (matrix `matrix.csv`
   identical but for the two restated ranges' `expectation_met` column; battle 33, battle 31 and the nine showcase
   runs byte-identical). Matrix of record 4,659 reversals (1,762 proposal, 2,897 contact), 2 give-ups; all seeds
   22,666 and 5; survivor ranges: 0 misses of record, 4 over all seeds.
2. **Two method findings (§2.2, §2.3).**
   - **The placebo band is now narrow:** the three placebo builds give 4,633, 4,662 and 4,698 of record, so the band is
     **[4,633, 4,698]** (−0.6 % to +0.8 %), against [6,017, 6,412] (−2.2 % to +4.2 %) on the old base.
   - **A placebo leaves a survivor range of record.** Placebo P1 (`PUSH_RELAXATION` + 2⁻³²) makes the zerglings of
     battle 26 **win** seed 33 (of record) with 2 left. That is the second plan's watch item ("a win on a seed of
     record re-opens the battle's claim"), tripped by one raw unit of a constant. The stop rule "a seed of record
     leaving its survivor range is a stop" therefore fires on noise for battle 26 seed 33, and has to be read
     against the placebos (§5, D-2).
3. **Diagnosis (§2.4, §2.5).** The class probe of the earlier plans, carried to today's tree, gives on the showcase's
   nine seeds R-A 42, R-B 31, R-C 37, R-D 24, R-E 26, R-F 14 (as the second plan's WP3), and on the matrix battles 26
   to 31 and 35 of record **R-B 1,614 and R-D 1,035 of 3,999**. Three facts the earlier plans did not have:
   - In the matrix, **a third of R-B is not about a fighting friend at all.** On the tick of the reversal the
     friend whose receding room was used is anchored in 1,050 of the 1,614 events (65 %), but in 532 (33 %)
     neither body is anchored: the friend stopped to wait under its order, and collision separates the pair evenly,
     so `slide_around_fixed` never acted there. In 32 (2 %) the friend did not stop but turned into the unit, which
     no settle against standing friends can see.
   - A slide that turns a forward step sideways is itself counted as a reversal whenever any backward push is added
     to it (the meter's cosine is −0.3): in the prototypes, a slid step of `(0.0318, 0.0002)` after a forward step of
     `(0.0009, 0.1194)` plus a contact push of −0.018 reverses.
   - Battle 35's reversals are mostly R-D (Vikings, 210 of its 273): two firing friends eased apart
     (friendly-blocking D-4, kept on purpose). Elsewhere R-D follows R-B a tick or two later.
4. **Candidates (§4).** Nine builds of one scratch tree with run-time switches, each measured on the matrix of record,
   battle 33, the showcase on nine seeds, battle 31 on 100 seeds and the reproductions:

   | Candidate | Rule | Matrix of record | Holds? |
   |---|---|---:|---|
   | S0 = D-2 on today's base | settle against every standing friend, no slide | 4,725 | No: showcase default seed flips to player 0; three seeds do not end; give-ups 8 over all seeds |
   | B0 = B1 on today's base | settle against anchored friends, no slide | **4,294** | No: showcase seeds 2, 3 not over and seed 6 flips; walkers 19 (bound 16); corridor 1 of 6 |
   | **S85** | settle against every standing friend; the removed approach slides along the lane exit (0.85 of it) | 5,637 | No: reversals +21 %, give-ups 14 over all seeds, battle 38a seed 33 out of range |
   | A85 | S85 against anchored friends only | 4,999 | No: above base; walkers 20 |
   | T85 | S85 against friends that walked at tick start only (the stale ones) | 5,625 | No: showcase flips to player 0 on seven seeds |
   | TA85 | T85 and anchored | 5,424 | No: showcase flips on seven seeds |
   | S100 | S85 with the whole removed approach slid | 5,699 | No: give-ups 13; battle 33 seed of record out of range |
   | Z | A85 plus the same slide at proposal time against tick-start anchored friends | 6,202 | No: the arc fails; reversals +33 % |
   | **P2** | a second proposal pass: units that stepped deeper into a friend that walked at tick start and stood this tick propose again, seeing it as standing | **4,302** | No: give-ups 21 over all seeds; battle 38b out of range on two seeds of record; showcase seeds 3 to 5 unfinished, seed 8 flips |

5. **Conclusion (§5).** No rule is planned. The slide as a locomotion rule (S85 and its variants) keeps the showcase
   when it acts on every standing friend, but it raises the matrix's reversals by a fifth and its give-ups nearly
   threefold, because it acts on far more steps than R-B's and turns forward steps sideways. The plain settle (B0)
   lowers the matrix's reversals below the floor but stalls the showcase, and so does the narrowest rule, a second
   proposal pass for exactly the R-B pairs (P2, 4,302). R-B and R-D stay open under B-103 with
   these figures; §9 names what a further attempt would need.
6. **Work packages (§6).** WP0 records only (this plan's row, the B-103 row's pointer and figures, the second plan's
   watch item made a method rule). No simulation change, so no golden, schema or throughput work.

---

## 2. Starting point and diagnosis

### 2.1 Base on `6b922732`

Release tools built from `6b922732` (scratch worktree `~/scratch/crowd-slide/proto`, `CARGO_TARGET_DIR=
/home/spacescuffle/cargo-target/crowd-slide-proto`), copied to `~/scratch/crowd-slide/bin-base`, measured with
`~/scratch/crowd-slide/scripts/measure.sh` (the second plan's `measure2.sh` with its own lock,
`~/scratch/crowd-slide/measure.lock`). The matrix differs from the second plan's WP3 run
(`~/scratch/crowd-edge-2/wp3/matrix/matrix.csv`) only in the `expectation_met` column of battle 26 (the restated
range of Amendment 2); battle 33, battle 31 and the showcase are byte-identical.

| Measure | Base |
|---|---|
| Matrix, seeds of record (232 runs) | 4,659 reversals (1,762 proposal, 2,897 contact), 2 give-ups (battle 35); survivor ranges: 0 misses |
| Matrix, all seeds (1,160 runs) | 22,666 reversals, 5 give-ups; 4 survivor misses (none of record) |
| Battles 26 / 27 / 28 / 29 / 30 / 31 / 35, reversals of record | 895 / 807 / 1,268 / 717 / 106 / 117 / 273 |
| Battle 33 (68 runs) | blue 39 |
| Battle 31 (20 runs / 100 runs) | 14 / 2 / 4 / blue 49, red 17, unresolved 34 |
| Showcase, nine seeds | player 1 wins eight; seed 6 not over at 3,000 (player 0's slow siege, second plan Amendment 1); 174 reversals; 0 give-ups; 0 deeper unit-ticks |
| Showcase default seed | player 1 at 1,935; 8 (4 + 4) / 9 (5 + 4) |
| Arc / walkers / corridor | 10 / 10 at 96, 0 reversals / 17 / 17, 16 reversals / 2 of 6 in range, 3 reversals |

### 2.2 The placebo band on today's base

The three placebo patches of the second plan (`~/scratch/crowd-edge-2/patches/patches-placebo{1,2,3}.diff`, one raw
unit added to `PUSH_RELAXATION`, `FIXED_BODY_TANGENT_WEIGHT`, `STEERING_LOOKAHEAD`), built from `6b922732`:

| Placebo | Battles moved | Reversals of record (proposal + contact) | All seeds | Give-ups | Survivor misses, record / all | Battle 31, 100 runs | Battle 33 | Showcase |
|---|---:|---|---:|---|---|---|---|---|
| P1 | 6 | 4,662 (1,798 + 2,864) | 22,480 | 2 / 5 | **1** / 4 (battle 26 seed 33: blue wins, 2 left) | 49 / 17 / 34 | 39 | seed 2 ends at 2,165 (2,170); 175 reversals |
| P2 | 4 | 4,633 (1,778 + 2,855) | 22,234 | 2 / 5 | 0 / 4 | 49 / 17 / 34 | 39 | identical |
| P3 | 4 | 4,698 (1,802 + 2,896) | 22,615 | 2 / 5 | 0 / 4 | 49 / 17 / 34 | 39 | identical |

**Band: [4,633, 4,698]** of record. An effect on reversals counts only below 4,633.

### 2.3 Battle 26 seed 33 is decided by noise

Under P1, battle 26 (`26_100_lings_vs_6_colossi`) seed 33 ends at tick 566 with the zerglings the winners (2 left,
colossi 0). On base the colossi win it with 2 left (tick 525), the very seed the second plan's Amendment 2 restated
the range for. So this seed of record sits on the knife edge between the two sides, and one raw unit of a contact
constant decides it. Every candidate below that moves battle 26 can move this seed either way without any rule
meaning it. §5 D-2 records how this plan read it: a candidate's miss on battle 26 seed 33 counts as a stop only if no
placebo shows the same miss. (None of the candidates below was stopped on this seed alone.)

### 2.4 The probe on today's tree

The class probe of the first plan (§2.1 there: a thread-local tag log, `zz_crowd_probe.rs`, `classify.py`,
`contact.py`, `classes.sh`) was carried to `6b922732`: the second plan's probe tree diff
(`~/scratch/crowd-edge-2/patches/removed-trees/probe.diff`) minus WP2 and WP3, which `main` already has, kept as
`~/scratch/crowd-slide/patches/probe-main.patch`. The probe changes no count: its showcase, arc, corridor and walkers
figures equal base's. Outputs: `~/scratch/crowd-slide/probe-out/base/`.

| Run | R-A | R-B | R-C | R-D | R-E | R-F |
|---|---:|---:|---:|---:|---:|---:|
| Showcase, nine seeds | 42 | 31 | 37 | 24 | 26 | 14 |
| Showcase, default seed | 4 | 3 | 3 | 2 | 4 | 1 |
| Walkers / corridor / arc | 6 / 2 / 0 | 2 / 0 / 0 | 0 | 7 / 1 / 0 | 1 / 0 / 0 | 0 |
| Matrix of record, battle 26 | 291 | 375 | 0 | 179 | 48 | 2 |
| 27 | 264 | 333 | 0 | 181 | 29 | 0 |
| 28 | 413 | 526 | 0 | 280 | 48 | 1 |
| 29 | 259 | 277 | 0 | 147 | 34 | 0 |
| 30 | 47 | 38 | 0 | 17 | 1 | 3 |
| 31 | 53 | 41 | 0 | 21 | 1 | 1 |
| 35 | 28 | 24 | 0 | 210 | 5 | 6 |
| **Battles 26 to 31 and 35** | 1,355 | **1,614** | 0 | **1,035** | 166 | 13 |

The showcase figures equal the second plan's WP3 figures exactly. The matrix has no terrain walls, so R-C is 0 there.
R-B and R-D together are 66 % of the contact reversals of these battles and 2,649 of the matrix's 4,659 reversals
of record.

**Contact sub-classes** (`contact.py`, the step that made the overlap on the tick before and the tick of the
reversal, and the partner's anchoring as `resolve_pair` saw it), matrix battles above, largest first:

| Events | Tick before | Tick of reversal | Class |
|---:|---|---|---|
| 441 | stepped in on receding room; partner then anchored | standing overlap, both anchored | R-B |
| 178 | stepped in on receding room; neither anchored | standing overlap, both anchored | R-B |
| 170 | stepped in on receding room; neither anchored | standing overlap, neither anchored | R-B |
| 169 | standing overlap | stepped in on receding room again | R-B |
| 168 | standing overlap, partner anchored | standing overlap, both anchored | R-D |
| 109 | standing overlap, both anchored | stepped in on receding room, partner anchored | R-B |
| 101 | stepped in on receding room, partner anchored | standing overlap, partner anchored | R-B |
| 95 | partner stepped in | partner stepped in | R-D |
| 89 | partner stepped in | standing overlap | R-D |

### 2.5 Traces

- **R-B, showcase default seed, player 0's pulse bot 11 behind rhino beetle 12, ticks 312 to 315** (the first plan's
  trace, unchanged on today's tree). Tick 312: 12 receded 0.10 last tick, so `remove_blocked_once` grants room 0.0926
  at gap 0; 11 steps `(0.0031, 0.0935)`. 12 came into range and stands, anchored. Overlap 0.124;
  `slide_around_fixed` moves 11 half out and sideways, achieved `(0.0331, 0.0527)`. Tick 313: 11 waits, and the
  rest of the overlap is pushed straight back, `(0, −0.0809)`: a reversal. Ticks 314 and 315: chatter with unit 10
  (±0.006), R-D.
- **R-B against a friend that waits (not anchored), showcase default seed, player 1's unit 24, tick 337.** Friend 23
  at gap 0.0677 receded 0.124 last tick, so room is 0.1914 against an approach of 0.1245; 24 steps
  `(−0.0613, −0.1101)`. 23 stops, waiting under its order: it is not anchored, so collision separates the pair evenly
  (0.35 of the 0.098 overlap). Tick 338: 24's steering turns away from the overlapping 23, `(0.0714, −0.0402)`, and
  the achieved steps reverse. A slide round 23 would not have been collision's either: `slide_around_fixed` acts
  only on anchored friends.
- **R-B in a walk, matrix battle 26 seed 11, zergling 25, ticks 84 and 85.** Its surround-walk step
  `(0.0358, −0.0389)` is allowed receding room against zerglings 14 (room 0.064) and 24 (0.018). Zergling 24 stops,
  anchored (overlap 0.085, slid); on tick 85 the walk's side is closed and 25 waits, and 14, now anchored too,
  pushes it straight back by the 0.189 overlap: `(−0.0331, −0.0003)`.
- **R-B with a moving partner, matrix battle 29 seed 11, zergling 93, tick 157 (under candidate S85, §4.2).** The
  partner (96) whose receding room 93 used did not stop: it turned and walked into 93 (`itv = (−0.074, −0.009)`). No
  settle against standing friends can see this pair, and the probe still files it R-B ("stepped in on receding
  room"). On base such pairs are 2 % of R-B (§1); under S85 they grow with the extra walking.
- **R-D, battle 35.** 210 of its 273 reversals of record are Vikings firing shoulder to shoulder: two anchored
  friends eased apart by `PUSH_RELAXATION ÷ 2` per tick (friendly-blocking D-4 "two units fighting shoulder to
  shoulder ease apart", kept on purpose).

### 2.6 What the slide does today

`slide_around_fixed` (`crates/sim_core/src/steering.rs`) fires only in the collision phase, only for a pair where
one friend is anchored and the other is not (`checked_dynamic_push`, rule 2), and only for an overlap that exists
after every proposal. Its correction is `normalize(0.5 n + 0.85 t) × overlap`, where `n` is the line of centres and
`t` the lane exit: perpendicular to the mover's step, on the side of the mover relative to the friend. So the
tangent part is 0.862 of the overlap and the normal part 0.507, then capped by `SOFT_PUSH_SPEED_SHARE` (0.65 of the
speed). It is not a rule a unit chooses: it is a by-product of having stepped in. Steps into a friend that stood
at tick start are already removed in the proposal (`remove_blocked_velocity`, room = plain gap), so the slide acts
mostly on the overlaps R-B makes, on overlaps contact chains make, and on a partner's step in. That is why every
settle removes it, and why making it a rule of its own changes far more than R-B (§4.2).

---

## 3. What StarCraft II does

Tags as in the precedent plans: **[DOC]** Blizzard material; **[DATA]** SC2GameData 5.0.14; **[COMM]** community or
editor observation; **[2ND]** second-hand account; **[INF]** this plan's inference; **verify** where nothing confirms
it. S1 to S10 are those of the first plan's §3 (fetched 2026-11-19).

### 3.1 Sources

| # | Source | Bearing on this plan |
|---|---|---|
| S1 to S4 | 5.0.14 data: `CGame`, `CMoverAvoid "Ground"`, `CUnit` defaults, `CAbilMove` / `CAbilAttack` [DATA] | No data parameter for local avoidance or contact. `CUnit` has `PushPriority` and `Radius` / `SeparationRadius` only |
| S5 | Blizzard, [Game Guide: Unit Positioning](https://news.blizzard.com/en-us/article/6640646/game-guide-unit-positioning) [DOC] | Units outside a concave stand until moved |
| S6 | 5.0.14 patch notes [DOC]; the 5.0.0 notes as summarised by the [StarCraft Wiki](https://starcraft.fandom.com/wiki/StarCraft_II_version_history/Patch_5.0.0) [2ND] | Being stuck behind allies is a known state; Blizzard raised allied push priority for Thors and Siege Tanks to help bulky units through crowds of small friends, and shrank the Ultralisk |
| S7 | SC2Mapster thread 158471 [COMM] | Units are blocked by friends that are attacking or holding position |
| S8 | SC2Mapster thread [187903](https://www.sc2mapster.com/forums/development/data/187903-unit-bumps-into-oncoming-unit-making-it-stop?comment=1) [COMM] | A unit bumping into an oncoming one stops; reducing the movement radius lets them slide past each other |
| S9 | GDC 2011, Anhalt, as summarised [2ND] | Unit-blind global pathing, local steering on top; a unit that cannot slide past others repaths |
| S10 | Q0074, the owner's account of `CMoverAvoidance` | Second-row attack-movers slide laterally along the back of the stationary front line into an open arc |
| S11 | Team Liquid, [The Mechanics of SC2, part 1](https://tl.net/forum/starcraft-2/132171-the-mechanics-of-sc2-part-1?view=all) [COMM] | Movement is flocking with steering: units avoid each other and weave past without a new path or losing momentum |

### 3.2 What follows

- **Units slide round friends that hold their ground** [S7, S10, S11]. The slide is a steering behaviour of the
  mover (S11: "without … losing momentum"), not a correction after overlap. That supports the second plan's
  direction: the slide belongs to locomotion. Our locomotion has two such behaviours already: steering's deflection
  round a body ahead (`steer`, `round_wall`) and the surround walk (Q0074). §4 tests whether a third, the lane-exit
  slide of `slide_around_fixed` moved into locomotion, helps.
- **A unit does not step into a friend that stops and get pushed back out** [S5, S7, INF]. Whether the game's
  avoidance predicts a neighbour's velocity (as our receding allowance does) and how it reconciles a neighbour that
  stops on the same frame is engine code: **verify** (no source). That small overlaps occur in the game between
  friends in a crowd and are pushed out is commonly observed but not sourced: **verify**.
- **Shoulder-to-shoulder firing units ease apart** [INF]: R-D's largest part (battle 35) is friendly-blocking D-4's
  kept rule; no source says the game holds two firing Vikings exactly still against each other.
- **Push priority, not walking back and forth, is what the game changed for stuck units** [S6]. Our push priorities
  are 5.0.14's and are unchanged here.

### 3.3 Sanctioned deviations

Q0052's S-1 to S-7 touch none of this. The surround walk is Q0074's "allows more" and is unchanged.

---

## 4. Candidates and prototypes

### 4.1 Method

All candidates are run-time variants of one scratch tree (`~/scratch/crowd-slide/cand`, detached at `6b922732`,
never committed; diff `~/scratch/crowd-slide/patches/prototype-settle-slide-two.diff`), selected by the environment
variable `CS`, read once into a `OnceLock`. With `CS` unset the tree is bit-identical to base (control `ctrl`: the
matrix "bit-identical to base", battle 33's and battle 31's `seeds.csv` and all nine showcase `units.csv`
byte-identical; the second build, `ctrl2`, likewise, §4.4). Each candidate is measured by
`~/scratch/crowd-slide/scripts/evalenv.sh <name> <CS>`: the matrix with `--compare` to base, battle 33 on 64 seeds,
the showcase on the default seed and seeds 1 to 8 (3,000 ticks), battle 31 on 100 seeds, and
`cargo test --release -p sim_core --test second_line_tests --test crowd_edge_tests -- --include-ignored`.
Results: `~/scratch/crowd-slide/<name>/summary.txt`, `matrix.log`, `repro.log`.

**Stop rules applied** (the brief, the second plan's D-4 and Amendments 1 and 2):
1. **Winner:** the showcase default seed (player 1) and every matrix battle's overall winner; battle 31 on 100 runs.
2. **Survivor ranges:** no seed of record leaves its battle's range (read against §2.3 for battle 26 seed 33).
3. **Give-ups:** matrix of record and all seeds at most base + 1 (2 / 6); every showcase seed at most base + 1.
4. **Reversals:** matrix of record below the placebo floor, 4,633.
5. **Guards:** arc 10 / 10, walkers 17 / 17 within `walkers_at_the_crowds_edge_do_not_toggle`'s bounds (≤ 3 per
   walker, ≤ 16 in all), corridor not worse than base (2 of 6, 0 give-ups), every showcase seed reported with its
   outcome, none newly unfinished or flipped.

### 4.2 The settle, with and without the slide

**The stage** is the first plan's `body_settle.rs` (`crowd-edge-wp3-wip`, reapplied from
`~/scratch/crowd-edge-2/patches/patches-B1.diff`): after every proposal (walks, pursuits, casts) and before
`resolve_collisions_and_push`, every body that moved by its own proposal is settled from its tick-start position
against the friends of its class that stood this tick, at zero velocity, by `remove_blocked_velocity`, alternating
with `settle_on_terrain` at most `WALL_CONTACT_PASSES` times; a step left at or below `WAIT_THRESHOLD` of the step
waits. Order-independent (movers are settled only against bodies that did not move).

**The slide** (new here): when the settle cuts a step `s` by an excess `e` (the approach removed), the cut part
becomes a step along the lane exit, the same direction `slide_around_fixed` uses: `t` perpendicular to `s`, on the
side away from `e` (left on a tie, as `lane_exit` with `flip_on_tie = false`). The settled step is
`s′ = remove_blocked_velocity(admissible + t × w|e|)`, clamped to `|s|`, then settled on terrain. `w` is
`FIXED_BODY_TANGENT_WEIGHT` (0.85, about collision's 0.862 tangent share) or 1. So the unit gets, in the proposal
and without overlap, the sideways motion collision would have given it over two ticks, and keeps its speed (no
shortened step feeds the next tick's acceleration, the first plan's evidence §3c).

**The friend sets** tried: every standing friend that will not step aside (`all`, the first plan's D-2), anchored
ones only (`anch`, the second plan's B1), only friends that walked at tick start (`stale`: moved more than
`STANDING_DRIFT` in the snapshot, the exact R-B friends whose room was granted), and stale and anchored.

| Measure (base) | S0 `all:0` (D-2) | B0 `anch:0` (B1) | S85 `all:85` | A85 `anch:85` | T85 `stale:85` | TA85 `staleanch:85` | S100 `all:100` |
|---|---|---|---|---|---|---|---|
| Matrix of record (4,659) | 4,725 | **4,294** | 5,637 | 4,999 | 5,625 | 5,424 | 5,699 |
| … proposal / contact (1,762 / 2,897) | 2,035 / 2,690 | 1,842 / 2,452 | 2,148 / 3,489 | 1,916 / 3,083 | 2,083 / 3,542 | 2,026 / 3,398 | 2,043 / 3,656 |
| All seeds (22,666) | 21,631 | 21,954 | 26,345 | 25,119 | 27,276 | 25,444 | 27,072 |
| Give-ups record / all (2 / 5) | 2 / **8** | 2 / 5 | 2 / **14** | 2 / 6 | 2 / **7** | 2 / 6 | 2 / **13** |
| Survivor misses of record (0) | 1 (26 seed 33, colossi 1 left: the noisy seed, §2.3) | 0 | **1** (38a seed 33) | **1** (38a seed 33) | 0 | 0 | **1** (33 seed 3203386110) |
| Battles 26 / 27 / 28 / 29 (895 / 807 / 1,268 / 717) | 863 / 718 / 1,090 / 1,224 | 726 / 796 / 1,002 / 987 | 943 / 1,008 / 1,430 / 1,413 | 658 / 923 / 1,368 / 1,179 | 849 / 958 / 1,217 / 1,583 | 797 / 1,047 / 1,269 / 1,425 | 923 / 974 / 1,544 / 1,181 |
| Battles 30 / 31 / 35 (106 / 117 / 273) | 72 / 99 / 261 | 86 / 114 / 155 | 99 / 93 / 140 | 91 / 126 / 185 | 78 / 135 / 298 | 88 / 130 / 228 | 91 / 118 / 339 |
| Matrix overall flips | none | none | none | none | none | none | none |
| Battle 31, 100 runs (49 / 17 / 34) | 47 / 17 / 36 | 49 / 15 / 36 | 52 / 18 / 30 | **42 / 24** / 34 | **39 / 29** / 32 | **39 / 22** / 39 | 46 / 15 / 39 |
| Battle 33, blue of 68 (39) | 50 | 48 | 46 | 48 | 47 | 39 | 41 |
| Showcase default seed (P1 at 1,935) | **P0 at 2,620** | P1 at 2,635 | P1 at 1,956 | P1 at 2,187 | **P0 at 2,179** | **P0 at 2,159** | P1 at 1,765 |
| Showcase seeds 1 to 8 | P0 on 1; **not over** on 3, 4, 7 | **not over** on 2, 3; **P0** on 6 | P1 on all eight (seed 6 now ends, 2,182) | P1 on all | **P0** on six, not over on 5, 7 | **P0** on six, not over on 2 | P1 on all |
| Showcase reversals, nine seeds (174) | 141 | 118 | 152 | 126 | 178 | 187 | 217 |
| Walkers (17 / 17, 16) | 17 / 17, 15, **4 for one walker** | 17 / 17, **19** | 17 / 17, within bounds | 17 / 17, **20** | within bounds | 17 / 17, **18** | within bounds |
| Corridor (2 of 6, 3) | **1 of 6**, 5 | **1 of 6**, 5 | **1 of 6**, 4 | **1 of 6**, 4 | 2 of 6, **10** | 2 of 6, **10** | **1 of 6**, 6 |
| Arc (10 / 10 at 96) | holds | holds | holds | holds | holds | holds | holds |

(Where the table says "within bounds" the walkers test passed and printed no figure. CE-14
`a_walk_without_retreat_ends_as_before` fails on every candidate: it pins the arc's proposed steps of ticks 1 to 62
to a fold, and every settle changes some of them; that test would be restated, not a stop.)

**Reading.**
- **The plain settle against anchored friends (B0) is the only design below the floor (4,294, −7.8 %),** and it
  removes 15 % of the contact reversals. It is what the second plan's B1 was; on today's base it no longer flips the
  default seed, but it slows it by 700 ticks, leaves two seeds unfinished at 3,000, flips seed 6, breaks the
  walkers' bound and loses a corridor unit. Its cost is the one the second plan named: the slide round fighting
  friends is gone.
- **Putting the slide back into the settle keeps the showcase** (S85, A85, S100: player 1 on every seed, seed 6
  even ends), so the diagnosis "the settle removes the slide, and the slide carries the crowd" is confirmed.
- **But the slide made a rule acts on every cut step, not on R-B's.** S85 raises the matrix's reversals by 21 % and
  its give-ups from 5 to 14. The probe on S85 (`~/scratch/crowd-slide/probe-out/all85/`, built from the candidate
  with the probe and a `settle-short` tag) shows why: showcase R-B rises 31 → 52 and battle 29's R-B 277 → 573. A
  forward step that the settle turns sideways reverses against any backward contact push on the same or the next
  tick (§1 item 3), and the zerglings of battles 27 to 29, which flow round each other at every tick, are turned
  sideways far more often than collision ever slid them (collision slid only on overlap with an anchored friend).
- **Restricting the set to the stale friends (T85, TA85)**, the exact R-B pairs, is the worst for the showcase: the
  settle then acts on the units that arrive at the front, and player 0 wins seven seeds. **verify** (not traced
  further: it stops on rule 1 at once).
- **Sliding the whole cut (S100)** speeds the showcase (every seed ends 150 to 550 ticks sooner) and costs give-ups
  (13) and battle 33's range on a seed of record.

### 4.3 The slide at proposal time (Z)

The second plan's wording ("the unit steps along the friend's tangent on the side its lane exits") also reads as a
proposal rule: in `step_toward`, after `remove_blocked_velocity`, the approach that tick-start *anchored* friends
removed becomes a lane-exit step (`proposal_slide` in the prototype), with the A85 settle after it. Measured
(`CS=anch:85:z`): matrix of record **6,202** (+33 %; 2,303 + 3,899), all seeds 26,309, give-ups 2 / 5, survivor
misses 0 of record (6 over all seeds), battle 35 597 (273), battle 31 49 / 21 / 30 on 100 runs, battle 33 42,
showcase player 1 on the default seed at 1,921 and seed 1 **not over**, 123 reversals on nine seeds, and **the arc
fails** (`ranged_block_reaches_the_arc`). A unit blocked head-on by a firing friend now slides every tick instead of
waiting or starting its surround walk, and the slide competes with the walk (its sideways step is not "forward",
so the walk logic reads it as blocked). **Rejected.**

### 4.4 A second proposal pass (P2)

**Rule.** After all proposals, find the friends that walked at tick start (snapshot velocity above
`STANDING_DRIFT`) and stood this tick. Every unit whose committed step ends deeper inside such a friend than it
started (by more than `WALL_CONTACT_TOLERANCE`) is restored to its tick-start state and proposes again, with those
friends' snapshot velocities set to zero. It then decides with its own rules (steering, the surround walk's start
test `friend_blocks_ahead`, the waiting rule) knowing that the friend stands: the "slide" is whatever locomotion
already does round a standing friend, and the stale allowance is gone for exactly the R-B pairs. Bounded to two
passes; deterministic (both passes read one snapshot, and only units that stood in pass one are changed in it).
`CS=none:0:two`.

**Measured** (base in brackets):

| Measure | P2 |
|---|---|
| Matrix of record | **4,302** (4,659): proposal 2,066 (1,762), contact **2,236** (2,897) |
| All seeds | 20,788 (22,666) |
| Give-ups, record / all seeds | 2 / **21** (2 / 5) |
| Survivor misses of record | **2**: battle 38b seeds 11 and 33, the marines win with 1 left (0) |
| Battles 26 to 31, 35 | 672, 685, 1,115, 1,111, 75, 111, **90** (895, 807, 1,268, 717, 106, 117, 273) |
| Battle 31, 100 runs | blue 40, red 20, unresolved 40 (49 / 17 / 34): blue still ahead, its share down 9 points (no placebo moves it) |
| Battle 33 | blue 40 of 68 (39) |
| Showcase default seed | player 1 at **2,620** (1,935); 10 (5 + 5) / 7 (5 + 2) |
| Showcase seeds 1 to 8 | player 1 on 1, 2, 6, 7; **not over** on 3, 4, 5; **player 0** on 8; 193 reversals in all (174) |
| Walkers / corridor / arc | 17 / 17 but **7 for one walker** (bound 3); **1 of 6**, 3 reversals; arc holds |

**Reading.** P2 is the narrowest rule tried: it touches only the R-B pairs, and it gives the largest fall of
contact reversals (−23 %) of any candidate, with battle 35's R-D nearly gone (273 → 90). But it fails four stop
rules: give-ups, survivor ranges, the showcase (three seeds unfinished, one flipped, the default seed 685 ticks
slower) and the walkers' bound. A unit that re-proposes knowing its friend stands now starts its surround walk, or
waits, a tick earlier than it did, which is the reach the collision slide used to give; so the trade the second
plan named (no overlap ⇔ no slide) holds for this design too. The give-up count of 21 may be inflated by the
prototype (a re-proposing unit records its route outcomes twice; **verify** before any reuse), but the showcase and
range failures do not depend on it.

---

## 5. Decisions

### D-1. No rule for R-B or R-D is planned

No candidate holds every stop rule of §4.1 and shows an effect below the placebo floor:

| Candidate | Below 4,633? | Breaks |
|---|---|---|
| B0 (anchored settle, B1) | **Yes, 4,294** | showcase (default seed 700 ticks slower, seeds 2 and 3 unfinished, seed 6 flips), walkers 19, corridor 1 of 6 |
| S0 (D-2) | No, 4,725 | showcase default seed flips; three seeds unfinished; give-ups 8 |
| S85 / A85 / S100 (settle + slide) | No | reversals +7 to +22 %; give-ups up to 14; survivor ranges; walkers (A85) |
| T85 / TA85 (stale only) | No | showcase flips on seven seeds |
| Z (slide at proposal) | No, 6,202 | arc; showcase seed 1 unfinished |
| P2 (second pass) | **Yes, 4,302** | give-ups 21 over all seeds; two survivor misses (38b); showcase default seed 685 ticks slower, seeds 3 to 5 unfinished, seed 8 flips; walkers (7 for one walker); corridor 1 of 6 |

So R-B and R-D stay open under B-103 with the figures of §2.4 and §4, and this plan has no rule work package.

### D-2. Method: the survivor-range stop rule is read against the placebos

The second plan's Amendment 2 made "a seed of record leaving its survivor range" a stop. Placebo P1 does exactly
that on battle 26 seed 33 (§2.3). For later movement plans: a candidate's survivor miss on a seed of record is a
stop **unless one of the placebo builds of the same base shows the same miss**; that seed is then reported, not
stopped on, and the battle's claim is re-examined only if the candidate's miss is a winner change that no placebo
shows. Battle 26 seed 33 is recorded as decided by noise on `6b922732`. This is recorded in B-103's row (WP0); it is
a method rule for plans, not a test bound, so no fixture changes.

### D-3. The placebo band is re-measured on every base

On `6b922732` it is [4,633, 4,698], a third as wide as on `76a7ac67` in relative terms. A plan that measures
against another base re-measures it, as the second plan's D-4.2 says; a band is never carried across bases.

---

## 6. Work packages

### WP0: records (S, docs only)

- This plan's row in `docs/plans/README.md` is added with the plan (`Proposed`); WP0 sets it to `Done` (a measured
  plan whose result is that no rule holds; the row says so).
- `DESIGN.md` B-103: the open item "R-B (31) and R-D (24), which need the slide … made a locomotion rule first"
  becomes "R-B and R-D, for which plan `2026-11-19-crowd-slide` measured the settle, the settle with a locomotion
  slide, the slide at proposal time and a second proposal pass, none of which holds (its §4 and §9)", with the matrix
  figures of §2.4 (R-B 1,614, R-D 1,035 of record in battles 26 to 31 and 35) and D-2's noise finding for battle 26
  seed 33.
- The second plan gets a one-line pointer amendment ("R-B and R-D: see plan `2026-11-19-crowd-slide`").
- **Gate:** docs-only `package`.

No other WP. If the orchestrator wants a rule landed regardless (for example B0, with its showcase cost accepted by
the owner), that is a new decision and a new plan amendment; the work-package shape it would need is in §7.

---

## 7. If a rule is later admitted: the work-package shape

Recorded so that an amendment admitting a candidate has its shape ready. It is not scheduled.

- **Tests first** (red on the base): a reproduction of the R-B trace of §2.5 with scratch archetypes in
  `crowd_edge_tests.rs` (a follower closing on a leader that walks and then stops in range; no achieved reversal for
  the follower over the 3 ticks after the stop), a moving-partner control (the leader turns instead of stopping;
  unchanged from base), and unit tests of the rule's arithmetic (order independence, as the first plan's
  `two_movers_settle_the_same_in_either_order`). The guards stay green: arc, walkers bounds, corridor, CE-7 to CE-13;
  CE-14 restated test-only with its new fold and the trace of the first changed step.
- **Control runs:** the rule off by a scratch flag reproduces the base byte for byte (matrix, battle 33 and 31
  `seeds.csv`, nine showcase `units.csv`).
- **Measure:** §4.1 with the base's placebo band; class probe on the default seed, the reproductions and battles 26
  to 31 and 35.
- **Goldens:** `golden_replay`, `golden_replay_orders`, `DEFAULT_SANDBOX_CHECKSUM` re-recorded only if they move, on
  the final tree, native debug and release, `wasm32-wasip1` release and debug once, each with the proof header
  ("Re-recorded for plan 2026-11-19-crowd-slide WP<n> (B-103): checkpoint N first changed; entity E at tick T ….
  Proof: a scratch build with the rule off reproduces the previous checksums at every checkpoint."). A settle adds no
  hashed state, so the schema stays 31.
- **Throughput:** `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`, base and candidate
  alternated, three rounds; every scenario ≥ 25 ticks per second. A settle adds one O(N) grid rebuild and one
  neighbour query per mover; a second pass re-runs whole proposals for its units and needs a `// PERFORMANCE
  RATIONALE:` block and a measured share of re-proposing units.

---

## 8. File ownership

| Area | Files | WP |
|---|---|---|
| Records | `docs/plans/2026-11-19-crowd-slide.md`, `docs/plans/README.md`, `DESIGN.md` §4 (B-103), the second plan's pointer amendment | WP0 |

No crate file is owned. No other open plan is touched.

---

## 9. Out of scope and open classes (Q0002)

Nothing left out is accepted as a deviation by being left out; each stays under B-103 with its figure.

| Item | State | Best candidate and figures | What a further attempt would need |
|---|---|---|---|
| **R-B** stale receding allowance | Open | B0: matrix of record 4,294, P2: 4,302 (contact −23 %), both stalling the showcase; S85: showcase holds, matrix 5,637 | Removing R-B's overlap costs reach in every form tried, from the general settle to a re-proposal of exactly the R-B pairs (P2); restoring the reach with a general lane-exit slide (S85, Z) costs more reversals than it saves. A further attempt needs a way to keep the unit's progress round a fighting friend that is neither the overlap nor a sideways turn of its forward step: for example a surround walk that starts from the friend's predicted stop, which is a change to the walk's start test (Q0074) and its own plan. R-B with a moving partner (2 %) is out of any settle's reach |
| **R-D** leftover overlap | Open | B0: contact reversals 2,897 → 2,452 | Downstream of R-B, and in battle 35 of friendly-blocking D-4's easing of firing pairs, which is kept |
| The meter's reading of a sideways step | Not a class | — | A slid step that turns 90° and meets any backward push counts as a reversal (cosine −0.3). Whether that is a reversal a player sees is a question for the meter's definition (first plan D-1.4), not changed here |
| R-C, R-E, R-F, the walk's start retreat, the corridor's standing | Open, as the second plan's §9 | — | Unchanged |
| `SeparationRadius` (B-49), lateral acceleration (B-92), U-23 tuning | Unchanged | — | — |

---

## 10. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| K-1 | R-B and R-D stay open | B-103 keeps two residual classes (31 and 24 on nine showcase seeds) | Recorded with figures; §9 says what a further attempt needs |
| K-2 | Battle 26 seed 33 flips for any movement change | Later plans stop on noise | D-2: read survivor misses against the placebos |
| K-3 | The probe's R-B label covers waiting and moving partners, not only fighting ones | A later plan aims at the wrong pairs | §1 item 3 and §2.5 split them (65 % anchored, 33 % waiting, 2 % moving on base) |
| K-4 | A future plan reuses this base's band on another base | Effects misread | D-3 |

---

## 11. Prototype material (scratch, not committed)

- Scripts: `~/scratch/crowd-slide/scripts/` (`eval.sh`, `evalenv.sh`, `measure.sh`, `summary.py`, `line.sh`,
  `placebos.sh`, `queue1.sh`, `queue2.sh`).
- Diffs: `~/scratch/crowd-slide/patches/prototype-settle-slide-two.diff` (every candidate, switched by `CS`) and
  `probe-main.patch` (the class probe on `6b922732`).
- Results: `~/scratch/crowd-slide/{base,placebo1,placebo2,placebo3,ctrl,ctrl2,all0,anch0,all85,anch85,stale85,
  staleanch85,all100,anch85z,two}/` (`summary.txt`, `matrix.log`, `repro.log`, `variant.txt`); probe outputs
  `~/scratch/crowd-slide/probe-out/{base,all85}/`.
- The scratch worktrees and their target directories were removed at hand-back; the diffs above rebuild them on
  `6b922732`.

## Amendment 1 (orchestrator, 2026-11-19): accepted; D-2 adopted; WP0 lands

No rule package is run. D-2 is adopted: a survivor-range miss stops a plan only if no placebo on the same base shows
the same miss; it is stated as a measurement rule in plan `2026-11-19-crowd-edge-steering-2` Amendment 3. WP0 sets this
plan's row to `Done`, restates B-103, and records battle 26 seed 33's placebo finding in the B-103 row and in the
comment of battle 26 in `crates/sim_core/tests/fixtures/matchup_matrix.ron` (comment only, range unchanged).
