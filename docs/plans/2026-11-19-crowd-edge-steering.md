# Blueprint: Crowd-Edge Steering: No Step Back at a Crowd's Edge, a Queue That Stands (B-103)

This plan finishes bug **B-103** (`DESIGN.md` §4), which plan
[`2026-11-09-friendly-blocking`](2026-11-09-friendly-blocking.md) narrowed but left open:

- a blocked unit at the edge of a crowd still steps back and forth now and then;
- a rear unit in a corridor that cannot reach range moves on far more of its ticks than a waiting unit should.

It measures both on current `main`, names the rule behind every reversal, and replaces three rules with ones that
have no asymmetry. No constant is added, and one is removed (`FLANK_RETRY_TICKS`).

**Status of this document.** This is a Stage 1 blueprint under charter §6, written on 2026-11-19 against `main` at
`720c6971` (checksum schema 30). Line numbers drift, so search for the quoted symbol. Claims not confirmed while
planning are marked **verify**. Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** Every package moves battle outcomes and `golden_replay`, and the work spans the
steering, locomotion and collision phases.

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §3, every D |
| [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) sanctioned deviations S-1 to S-6 | None of them touches crowd steering | §3.4 |
| [Q0074](../questions/closed/0074-surround-walk.md) the surround walk (`surround_walk: true` by default) | Answered, option 1. The owner wrote that boxed-in units "stand still cleanly". This plan keeps the walk and makes it, and the queue, stand cleanly | D-1, D-3 |
| [Q0079](../questions/closed/0079-group-consistent-routing.md) group lanes (S-7) | Untouched. One reversal class (R-F) comes from route adoption, which is that plan's ground | §10 |
| Q0061 hand back after `cargo gate quick`; a verifier runs `package` | Standing rule | §6 |
| Q0002 scoping is the agent's call if written down | Standing rule | §10 |
| **OQ-1, filed as [Q0089](../questions/closed/0089-queued-attacker-keeps-order.md)** may a unit queued behind its own fighting friends keep its attack order indefinitely? | Default **yes** (D-4). Filed as a question file only if the orchestrator wants the owner to decide | D-4 |

---

## 1. Summary

1. **The figures in the register are stale (§2.2).** Measured on `720c6971`:
   - **Showcase**, default seed: **13 and 10** reversals for players 0 and 1. The register says 11 and 8. The battle
     now ends at tick 1,946, with player 1 winning.
   - **Synthetic arc** (`ranged_block_reaches_the_arc`): **0** reversals. All 10 units are in range by tick 96. The
     test passes, but it is still `#[ignore]`d.
   - **Corridor** (`no_room_means_waiting_still`): **21** reversals, 2 of 6 units in range. Counted as the test
     counts (velocity exactly zero), the four waiting units stand on only **15 to 46 %** of their ticks after their
     first stop. The register's 73 to 85 % was measured on an older base.
   - **Walkers at a crowd's edge** (`walkers_at_the_crowds_edge_do_not_toggle`): 31 reversals, 17 of 17 units in
     range.
2. **Every reversal has a named cause (§2.3 to §2.6).** A scratch probe logged each unit's steering branch and
   proposed step on every tick, along with each contact push. Across the showcase's 9 seeds (235 reversals) and the
   matrix's seeds of record (232 runs, 6,154 reversals), every reversal falls into one of six classes:

   | Class | Rule that causes it | Showcase, 9 seeds | Matrix, seeds of record |
   |---|---|---|---|
   | **R-A** walk retreat | The surround walk's sideways step, after `remove_blocked_velocity`, gains a backward component. Stepping back frees the straight way in, so the walk ends; the unit steps in, is blocked, and walks again | 63 | 2,667 |
   | **R-B** stale receding allowance | `remove_blocked_velocity` gives a unit room equal to how fast the body ahead receded *last* tick. If that body stops this tick (it reached range, or it waits), the step ends inside it. The contact phase then pushes the unit back over one or two ticks | 87 (contact) | ≈ 1,835 (contact) |
   | **R-C** terrain slide into a body | `after_wall_slide`'s step is never re-checked against bodies. The slide along a wall walks the unit into a friend standing beside the wall; contact pushes it back | 34 (contact) | included in contact (`after-wallslide` is rare there) |
   | **R-D** residual overlap | Two standing friends that already overlap, because of R-B or R-C earlier, are eased apart over several ticks (`PUSH_RELAXATION`). Each tick's push can reverse the last | 30 (contact) | ≈ 1,400 (contact, of which 406 are battle 35's Vikings while firing) |
   | **R-E** crossing body | Steering's side for a single moving body that crosses the path is taken from that body's position on each tick, so it flips when the body crosses the line | 7 | ≈ 230 |
   | **R-F** route adoption | A unit takes a new route the tick after it stepped along another, for example its group's lane on the order's second tick, or a chase's first route | 14 | 11 |

3. **StarCraft II (§3).**
   - 5.0.14 has no data field for crowd steering or for giving up behind friends. `CMoverAvoid` "Ground" has no
     parameters, and `CGame` has only the acquire and leash radii.
   - Blizzard's guide: units left outside the concave **stand** until the player moves them.
   - Community sources: units are blocked by friends that are attacking or holding.
   - GDC 2011 (second-hand): unit-blind global pathing, with local avoidance.
   - No source has a unit step back and forth. Under friendly-blocking I-3, that is a defect of ours.
4. **Decisions (§4), each one removing an asymmetry, with no new constant.**
   - **D-1 (R-A).** A walk never steps back from its goal. The walk's step is projected onto the bodies' half-planes
     *and* the half-plane "no component away from the goal", so a walk that can only retreat is a closed side. The
     asymmetry it removes: the walk's start and end are judged along the way in, but its step was allowed to move
     against it.
   - **D-2 (R-B, R-C, and R-D's source).** A step never ends deeper inside a friend that stood this tick. This is a
     settle pass against bodies, after every proposal and before contact. It is the rule B-113 already applies to
     walls, and it removes the asymmetry between terrain (never deeper) and standing bodies (deeper, then pushed
     back). It also removes the asymmetry between the terrain slide (checked against walls) and bodies (not checked
     after the slide).
   - **D-3 (corridor).** A spent walk re-arms when the unit gains ground, not on a timer. `FLANK_RETRY_TICKS` is
     removed. A queue whose members never gain ground stands.
   - **D-4 (corridor).** A tick a unit spends queued behind a friend that holds its ground under attack is not a
     stalled tick. The walk already counts as progress (friendly-blocking D-3), and waiting behind the same crowd
     must not count against the order either. Before D-4, the timed retry was what kept corridor units from giving
     up (friendly-blocking Amendment 4, change 5).
5. **Out of scope, registered:** R-E (B-144), R-F (B-145), and B-103's remaining items, if any (§10).
6. **Work packages (§6).**
   - WP0: records.
   - WP1: measurement, bit-identical. The reversal meter splits proposal-caused from contact-caused reversals, and
     the arc test is un-ignored.
   - WP2: D-1.
   - WP3: D-2.
   - WP4: D-3 and D-4.
   - WP5: storyboard and bookkeeping.

   Each rule package has red tests first, control runs, the standard stop rules and goldens with proof headers.

---

## 2. Diagnosis

### 2.1 The probe (scratch, not committed)

A scratch build of `720c6971` added a thread-local tag log (`movement/probe.rs`) that records each of these per unit
and tick:
- the branch taken in `walk`, `chase`, `aim_if_in_range`, `step_toward` (the direct snap, the steered and removed
  steps, waiting, the wall slide and the final step) and `surround_walk` (each exit: forward, walk step and side,
  side closed, spent wait, retry);
- the steering decision (`steer:wall`, or the side blockers and their net weight);
- every obstacle `remove_blocked_once` considered: gap, room and approach;
- the step committed (`commit v=…`), with the walk memory;
- every contact push in `resolve_pair`: partner, overlap, delta, both anchors and both positions.

A scratch test (`tests/zz_crowd_probe.rs`) drove the showcase (`sim_fixtures::trace::route::start_battle`, default
seed and seeds 1 to 8, to the end), the four reproductions of `second_line_tests.rs`, and every matrix battle on its
seeds of record (`sim_fixtures::matchup::BattleRun`, single-threaded so the thread-local log stays whole). The
`BlockingMeter` counted reversals (D-1.4) exactly as the committed tests do. Every count the probe reports equals the
unmodified build's: the showcase gives 13 and 10, and the corridor 21, as `second_line_tests -- --include-ignored`
reports on the plain tree. So the instrumentation changed nothing.

**Classification.**
- A reversal is **proposal-caused** when the two committed proposals themselves reverse.
- It is **contact-caused** when they do not, so only the collision phase's corrections made the achieved steps
  reverse.
- A contact reversal is attributed to the step that created the overlap being undone:
  - the unit's own step into the partner, with the reason `remove_blocked_velocity` let it (receding room, or the
    wall slide after the removal);
  - the partner's step into the unit;
  - an overlap already standing on both ticks.

Material (not committed):
- `~/scratch/crowd-edge-plan/scripts/probe.diff`, the instrumentation (245 lines);
- `zz_crowd_probe.rs`;
- `classify.py`, `contact.py` and `walkpat.py` (run with `python3 -I`);
- the raw logs under `~/scratch/crowd-edge-plan/`.

WP1 makes the proposal/contact split permanent (D-5).

### 2.2 The figures on `720c6971`

**Showcase** (`assets/scenarios/showcase.ron`, to the end; reversals per owner P0 / P1; stall give-ups 0 / 0 on
every seed). These equal contour-footprints evidence §6e's WP6 column:

| Seed | Winner, tick | Reversals P0 / P1 | Blocked-attacker ticks P0 / P1 |
|---|---|---|---|
| Default | P1, 1,946 | 13 / 10 | 201 / 267 |
| 1 | P1, 1,928 | 10 / 17 | 195 / 260 |
| 2 | P1, 1,913 | 12 / 20 | 199 / 258 |
| 3 | P1, 2,194 | 12 / 10 | 205 / 340 |
| 4 | P1, 2,616 | 18 / 11 | 212 / 256 |
| 5 | P1, 1,924 | 12 / 10 | 200 / 241 |
| 6 | P1, 2,175 | 12 / 15 | 201 / 300 |
| 7 | P1, 2,187 | 17 / 5 | 209 / 265 |
| 8 | P1, 1,921 | 12 / 19 | 198 / 249 |

`showcase_second_line_reaches_range` checks attackers in range at tick 900. The armies now meet in the middle of the
map near tick 300, and by tick 900 player 0 has no attacker left, so its tick-900 criterion (from friendly-blocking
§7, written when both armies stood at the enemy halls) no longer measures B-103 (§4, D-6).

**Reproductions** (release build, `--include-ignored`):

| Test | Result on `720c6971` |
|---|---|
| `ranged_block_reaches_the_arc` (ignored) | **Passes.** All 10 in range at tick 96, 0 reversals, 0 give-ups. The friendly-blocking figure "3 reversals" is no longer true; no commit since has been traced to it (not needed: it now meets its acceptance) |
| `no_room_means_waiting_still` (ignored) | Fails. 2 of 6 in range, 21 reversals (per unit 0, 1, 3, 7, 1, 9), 0 give-ups. Ticks exactly still after the first stop, per unit in file order: 105 / 347, 139 / 340, 49 / 332, 150 / 324, 52 / 269, 94 / 316 (15 to 46 %) |
| `walkers_at_the_crowds_edge_do_not_toggle` | Passes its bound (≤ 10 per walker): 31 reversals, at most 7 per walker, 17 / 17 in range |
| `showcase_second_line_reaches_range` (ignored) | Fails on its tick-900 criterion (see above). 13 / 10 reversals up to tick 1,300, 0 give-ups |

**Matrix, seeds of record** (232 runs): 6,154 reversals. The battles with 100 zerglings dominate: battle 28 has
1,393, 29 has 1,047, 27 has 1,033 and 26 has 1,024. Then come battle 35 (668), 31 (203), 30 (165) and 34 (144).

### 2.3 R-A: the walk retreats (proposal-caused)

**Trace.** Showcase, default seed, player 0's pulse bot 4, tick 398. It walks forward `(0.0131, 0.1142)`: its walk is
paused and the straight step free. On the next tick the straight step is clipped below half. The walk starts on its
kept left side, and the across step `(−0.126, 0)` is projected by `remove_blocked_velocity` around a body ahead-left
to `(−0.1049, −0.0348)`. Its component along the way in is −0.035: a step back.

Across all R-A events of the 9 seeds:
- the forward component of the walk step is **−0.035 to −0.064** per tick, against a forward step of +0.09 to
  +0.126;
- the cosine is below −0.3, so the meter counts a reversal.

The retreat opens room, so on the next tick the whole straight step is free (Amendment 7's end test). The walk ends,
and the unit steps forward into contact again. Cycles of 2 to 3 ticks (`forward → walk-across` 36,
`walk-across → forward` 27).

**Why.** D-2.3 defines the walk as a step across the way to the goal, perpendicular to it. Projecting that step
around a body that lies ahead on the walking side tilts it backward. That is the tangent of the body, and it points
away from the goal (the body at angle θ ahead of the across direction gives a backward component −sin θ cos θ). The
walk's start test (forward part below half a step) and end test (whole straight step free) are both judged along
the way in, but nothing stops the walk step itself from moving against that axis. A walk that can only retreat is not
a way round; it is the oscillation.

Corridor: 17 of 21 reversals. Walkers: 20 of 31. Matrix: 1,154 + 966 forward/walk alternations, 257 walk-to-walk,
290 steer-to-walk.

### 2.4 R-B: the stale receding allowance (contact-caused)

**Trace.** Showcase, default seed, pulse bot 11 (player 0) behind rhino beetle 12, ticks 312 to 315.
- **Tick 312.** `remove_blocked_once` finds 12 at gap 0 (they already overlap by 0.032) with
  `room = 0.0926`: 12 receded 0.102 last tick. Only `0.1253 − 0.0926` of the approach is removed. Unit 11 steps
  `(0.0031, 0.0935)` toward 12. But 12 reached range this tick: its proposal is zero, and in the collision phase it
  is anchored. The overlap is now 0.124, and `slide_around_fixed` pushes 11 back, capped by the soft push (achieved
  `(0.0331, 0.0527)`).
- **Tick 313.** 11 waits. The rest of the overlap is pushed out, an achieved `(0, −0.0809)`: a reversal.
- **Ticks 314 and 315.** Contact chatter with unit 10 (±0.006), two more reversals.

**Why.** The allowance ("a body receding at least as fast as the unit approaches is no obstacle … columns march
closed up") reads the obstacle's velocity from the tick-start snapshot. Every unit decides at once from that
snapshot, so the allowance is a prediction that the body keeps walking. When it stops, no rule stops the step. Walls
have that rule (B-113, `settle_proposal`: no step ends deeper in a wall). Bodies that stood this tick do not.

Showcase: 87 of 151 contact reversals. Matrix: about 1,835 of 3,243.

### 2.5 R-C: the terrain slide walks into a body (contact-caused)

**Trace.** Showcase seed 1, rhino beetle 22 (player 1) against the terrain edge at y = 35.375, beside firing beetle
19, ticks 573 to 617.
- `steer` gives `(−0.105, −0.070)`, which does not approach 19 (dot −0.007), so `remove_blocked_velocity` leaves
  it.
- The disc probe refuses it, and `after_wall_slide` keeps `(−0.105, 0)`. That step approaches 19 by 0.0315 and is
  taken unchecked.
- Overlap 0.13; 22 comes within range and aims. The pair-anchored separation (`overlap × PUSH_RELAXATION ÷ 2`) moves
  22 out by 0.0085 to 0.0113, out of range. It chases in again.
- So the cycle runs `aim → forward → aim`, 11 reversals in seed 1 alone.

**Why.** `step_toward` applies the body constraint, then the terrain slide, and never the body constraint again.
D-1.2 of friendly-blocking ("contact as a constraint for all bodies at once") stops at terrain.

Showcase: 34 of 151 contact reversals.

### 2.6 R-D, R-E, R-F

- **R-D, residual overlap.** 30 showcase contact reversals show an overlap already present on both ticks: the
  partners stand, and contact eases them apart, a little each tick, sometimes back and forth with a third body. Each
  traces back to an R-B or R-C step a few ticks earlier, or to a partner's step in (`partner-stepped-in`, 500 on the
  matrix). On the matrix this class includes battle 35's 406 events: Vikings firing in the air class, two firing
  bodies pair-anchored and eased apart.
  - **No rule of its own is proposed.** D-2 removes the overlaps' source.
  - Easing two firing friends apart is friendly-blocking D-4's "two units fighting shoulder to shoulder ease apart",
    which is kept.
- **R-E, crossing body.**
  - **Trace.** Showcase, default seed, unit 24 (player 1) at (17.43, 11.71), tick 951. Friend 23 walks north across
    24's westward path. In `side_blockers` it is not co-moving, so its side is the sign of its lateral offset. The
    offset changes sign as 23 crosses, and net weight goes from −0.80 to +0.85: steps `(−0.033, 0.082)` then
    `(−0.019, −0.083)`.
  - Moving bodies are left out of walls on purpose, and no body here is standing.
  - This is ordinary dodging of a crossing friend, not a crowd edge. **Out of scope, registered as B-144** (§10).
- **R-F, route adoption.**
  - Every seed shows player 1's unit 25 at tick 2: it steps east on its own first leg, then takes a route west. That
    is probably its group's lane, computed on the order's second tick (**verify**, `compute_group_lanes`).
  - A chase taking its first route after a refused step (`route 0 → 10`) shows the same pattern, with the walk
    starting on the same tick.
  - The intent (order, target, goal) is unchanged, so D-1.4 counts it.
  - **Out of scope, registered as B-145** (§10).

### 2.7 The corridor's standing

Rear units 3, 4, 5 and 7 chase the pillar (`AttackTarget`, acquired). After their first stop, by velocity on that
tick:

| Unit | Exactly still (the test's measure) | Moved ≤ `STANDING_DRIFT` (1/64), the simulation's own "standing" | Ticks on a walk step |
|---|---|---|---|
| 3 | 139 / 340 | 88 % | 42 |
| 4 | 49 / 332 | 88 % | 42 |
| 5 | 150 / 324 | 69 % | 74 |
| 7 | 94 / 316 | 65 % | 86 |

There are two causes:

1. **The timed retry** (`FLANK_RETRY_TICKS` = 24, friendly-blocking Amendment 4, change 5). A spent walk walks again
   every 24 ticks. Each retry walks 4 to 15 ticks across the corridor (lateral room 1.25), closes against the wall,
   swaps sides after `FLANK_REVERSE_TICKS` and spends again. The corridor has no way round, so every retry is
   motion with no purpose. Amendment 4 records why the retry exists: without it the corridor's waiting units ran
   into the 250-tick stall rule and gave their chase up (3 give-ups). Waiting counts as stalled (friendly-blocking
   D-3), while walking clears the stall evidence.
2. **Contact creep.** A waiting unit pressed by others is moved by contact relaxation. On most of these ticks the
   motion is below 1/64 (it is "standing" by `SteeringObstacle::is_standing`), and on many it is below 10⁻⁴: a
   geometric decay of `PUSH_RELAXATION` that does not reach exact zero for tens of ticks. With D-2 fewer overlaps
   are made. Exact zero is still not a measure of waiting (D-6).

---

## 3. What StarCraft II does

Tags as in the precedent plans: **[DOC]** Blizzard material; **[DATA]** SC2GameData 5.0.14 (`5444e12f13`, layer chain
`core → liberty → swarm → void → voidmulti`); **[COMM]** community or editor observation; **[2ND]** second-hand
account; **[INF]** this plan's inference; **verify** where nothing confirms it.

### 3.1 Data

| # | Source | What it says |
|---|---|---|
| S1 | `core` `GameData.xml`:8–10, 465 (fetched 2026-11-19 from `https://raw.githubusercontent.com/SC2Mapster/SC2GameData/5444e12f13/mods/core.sc2mod/base.sc2data/GameData/GameData.xml`, copy under `~/scratch/crowd-edge-plan/data/`) | `CGame`'s only movement fields: `AcquireMovementLimit` 21, `AcquireLeashRadius` 21, `AcquireLeashResetRadius` 5, `AirFormationDiameter` 8, `MixedFormationDiameter` 6, `FormationSeparationFactor` 3. **No** field for crowd avoidance, stuck time, repath or give-up [DATA] |
| S2 | `core` `MoverData.xml` (`~/scratch/combat_wp3_data/core/`) | `CMoverAvoid id="Ground"` carries only a placement array. Ground avoidance has no data parameters: it is engine code [DATA] |
| S3 | `core` `UnitData.xml` `CUnit` default | `Mover` Ground, `Radius` and `SeparationRadius` 0.5, `PushPriority` 10, `Mass` 0.4. Nothing about blocking or waiting [DATA] |
| S4 | `core` `AbilData.xml` `CAbilMove id="move"` | `FleeRange` 5, `FleeTime` 5, `FollowRangeSlop` 1, `FollowAcquireRange` 6, `MinPatrolDistance` 1. No crowd field; `CAbilAttack` has none either [DATA] |

So 5.0.14's crowd behaviour is not in data. What is known of it is observed behaviour.

### 3.2 Behaviour

| # | Source | What it says |
|---|---|---|
| S5 | Blizzard, [Game Guide: Unit Positioning](https://news.blizzard.com/en-us/article/6640646/game-guide-unit-positioning) (friendly-blocking fact 5) | A concave lets every unit attack. For units left outside it, the player is told to move them slightly forward and attack-move again. A unit left outside **stands** [DOC] |
| S6 | Blizzard, [5.0.14 patch notes](https://news.blizzard.com/en-us/article/24162754/starcraft-ii-5-0-14-patch-notes) | The Ultralisk was shrunk "to minimize cases where it got stuck behind other allied units". Being stuck behind allies is a known state, solved by size or push priority, not by a walk back and forth [DOC] |
| S7 | SC2Mapster data forum thread 158471 (friendly-blocking community 7) | Units "push their way through units that are idling … but get blocked by units that are holding position, attacking, patroling and other moving units" [COMM] |
| S8 | SC2Mapster thread 187903 (friendly-blocking community 9) | Units "will give up move commands if they make no progress towards their destination over some period of time as a result of unit on unit collisions". Move commands; no duration; nothing about attack orders [COMM] |
| S9 | GDC 2011 AI Summit, Anhalt, "AI Navigation: It's Not a Solved Problem – Yet", as summarised (sc2-pathfinding plan S1 to S5; friendly-blocking fact 10) | Global pathing ignores units, with local steering on top. A unit that cannot slide past others repaths. A stationary group gives way to a moving one [2ND] |
| S10 | Q0074, the owner's answer | The owner's account of `CMoverAvoidance`: second-row attack-movers slide along the back of the front line into an open arc, and completely boxed-in units stand still cleanly. Recorded as the owner's direction under Q0074, not as a source of mechanics |

### 3.3 What follows for us

- **A blocked unit at a crowd's edge holds** [DOC S5, INF].
  - It either slides along the crowd toward open room (our surround walk, Q0074) or stands.
  - No source has it step back from the crowd and then in again.
  - A step away from the goal is not a slide *along* the crowd. D-1 forbids it inside the walk.
- **Contact does not drive units back and forth** [INF from S5, S7, S9].
  - The game's avoidance keeps units from walking into friends that will not move. S7: those that are attacking or
    holding block.
  - A unit that walks into a friend that stopped this tick and is then pushed out is an artefact of our
    simultaneous update, not the game's behaviour.
  - D-2 gives standing friends the treatment our walls already get.
- **A rear unit in a corridor waits** [DOC S5, 2ND S9, INF].
  - Global pathing ignores units (S9), so a repath finds the same corridor; there is no unit-aware route to take.
  - "Repaths" (S9) is about sliding past others where a route exists. Our stall repath (`STALL_REPATH_TICKS`)
    already does it and is unchanged.
  - The unit stands, as S5's units outside the concave do.
- **Whether an attack order is given up while queued: not sourced** [**verify**].
  - S8 covers move commands only. 5.0.14's only data-defined limit on an acquired chase is the leash (S1,
    `AcquireLeashRadius` 21, B-142), and a queued unit does not move from its origin.
  - Our 250-tick stall rule for chases (`STALL_ABANDON_TICKS`, U-23) is unsourced.
  - D-4 (OQ-1) chooses not to apply it to a unit queued behind its own fighting friends. A plain `Move` keeps S8.

### 3.4 Sanctioned deviations

- Q0052's S-1 to S-6 (supply cap and selection, splash on cloaked units, the turn-before-move tolerance, moves to
  unreachable points, 25 ticks per second, original names) touch none of this.
- S-7 (group lanes, Q0079) is untouched, apart from R-F's registration (§10).
- The surround walk is Q0074's "allows more" deviation and stays. This plan changes how the walk steps, not whether
  it exists.

---

## 4. Design decisions

### D-1. A walk never steps back from its goal (R-A)

1. **Rule.** In `surround_walk`, the walk step is the projection of the across step `across × speed` onto the
   intersection of these half-planes:
   - one per body that will not yield, as `remove_blocked_velocity` builds them today;
   - **H₀ = { v : v · ahead ≥ 0 }**, where `ahead` is the unit direction to `next`.

   The projection is cyclic, in the slice's order, with H₀ last in each pass. It runs at most as many passes as
   there are constraints in reach, plus one, the same bound as today's.
2. **Consequence.** A side whose only admissible step retreats yields a step at or below `WAIT_THRESHOLD`. That is
   the existing "side closed" branch: `flank_blocked_ticks` counts, the side swaps after `FLANK_REVERSE_TICKS`, and
   the unit waits meanwhile. No new branch and no new constant.
3. **The forward step is unchanged.** The paused walk's straight step (`ahead × speed` through
   `remove_blocked_velocity`) and steering's step are untouched. A forward step may still be deflected sideways
   (that is steering), and a unit that is not walking is not constrained by H₀.
4. **Why this, not other end tests.**
   - Friendly-blocking Amendment 7 already moved the end test to a full free step. The cycle survives because the
     *retreat* frees that step.
   - Making the end stricter again would hold units in walks that have nowhere to go.
   - H₀ removes the cause: the walk can no longer make the room that ends it.
   - A clean firing arc around a target is convex toward the rear units. Walking across along its tangent meets no
     body ahead on the walking side, so H₀ is inactive there (the arc reproduction keeps 10 / 10, WP2).
5. **Determinism.** Fixed-point dot products and the existing projection arithmetic; no new state, no float, no
   allocation (the same `neighbours` slice).

### D-2. A step never ends deeper inside a friend that stood this tick (R-B, R-C)

1. **Rule.** A new last stage of `advance_checked_movement`, after every proposal (walks, pursuits and casts) has
   been committed and before `resolve_collisions_and_push`.
   - **Who is settled.** Every ground or burrowed body of the same class that **moved** this tick
     (`position != pos_at_tick_start`).
   - **Against whom.** Every friend of the same body class, within reach, that **stood** this tick
     (`position == pos_at_tick_start`), that will not step aside for the mover, and that is not a mineral-walker or
     collision-suppressed.

     "Will not step aside" is `SteeringObstacle::yields_to`, evaluated on that friend's state at the end of
     proposals with velocity zero. An idle, standing friend of no higher priority still yields and is pushed aside
     by contact, as D-4 of friendly-blocking says.
   - **The settle.**
     - **Step.** Let `s = position − pos_at_tick_start`. Recompute
       `s′ = remove_blocked_velocity(mover at pos_at_tick_start, s, those friends with zero velocity)`, so room is
       the plain gap, as for any standing body.
     - **Re-check against terrain.** If `s′ ≠ s`, the new end is settled on terrain as `settle_proposal` does
       (`settle_on_terrain`, never deeper into a wall).
     - **Alternate.** Body removal and terrain settle alternate at most `WALL_CONTACT_PASSES` times (3, the
       existing bound: one per constraint pressing at a corner, plus one). If the step still violates either
       constraint after that, the unit stays at `pos_at_tick_start`, as a refused step does in `settle_proposal`.
     - **The waiting rule.** If `|s′| ≤ WAIT_THRESHOLD × step`, the unit waits: position unchanged, velocity zero,
       anim `Idle`, order kept (D-1.3).
     - **Otherwise.** The unit commits `s′`, with velocity `s′`. Its heading follows `s′` within the hull rate already
       spent this tick; WP3 checks `turn_hull_towards` is idempotent within the tick (**verify**).
2. **Why the order does not matter.**
   - Settled units are only ever shortened toward their start.
   - They are settled only against bodies that did not move this tick, and settling never moves those.
   - So the result is the same in any processing order, and it is computed in entity order. No mover is settled
     against another mover: pairs that both move keep today's contact separation (co-moving columns, D-4's
     "moving … separates evenly").
3. **What it covers.**
   - **R-B.** The friend that stopped this tick has zero velocity here, so the stale room disappears.
   - **R-C.** The slid step is re-checked against the friend beside the wall.
   - **A partner stepping into a waiting unit.** The partner is the mover and is settled.
   - **Bit-identity.** Where the snapshot was right (the friend stood at tick start and still stands), the proposal
     already removed the approach, `s′ = s`, and nothing changes.
4. **Enemies** are not settled against. Contact between enemies separates evenly and never pushes (friendly-blocking
   fact 3), and D-1.4 excuses ticks where an enemy touches the unit. Out of scope (§10).
5. **Air** units are settled within the air class by the same rule. Battle 35's Vikings are R-D, standing pairs,
   and WP3 measures them. Terrain does not apply to air, so the body removal alone decides.
6. **Performance.** One rebuild of a retained body grid over committed positions (O(N)), and one neighbour query per
   mover. The movers' list reuses `movement.entries` and the grid reuses `movement.grids`, rebuilt from the committed
   positions, so nothing allocates in steady state. The stage carries a `// PERFORMANCE RATIONALE:` block:
   - **Why:** a second pass over neighbours each tick, in a 400-unit crowd.
   - **What:** reuse the tick's retained grid and neighbour buffer, settling only movers, against standing friends
     only.
   - **Trade-off:** one more O(N) grid rebuild and O(k) per mover, for no overlap that contact must undo.

   WP3 measures it (§8).
7. **Why not drop the receding allowance?** Without it a marching column cannot stay closed up. Every follower would
   stop at its leader's tick-start edge, as friendly-blocking's `remove_blocked_velocity` doc warns. The allowance
   is right while the leader keeps walking, and D-2 corrects exactly the ticks it is wrong.

### D-3. A spent walk re-arms when the unit gains ground, not on a timer

1. **Rule.** `FLANK_RETRY_TICKS` and the spent walk's timed retry in `surround_walk` are removed. A spent walk
   (`flank_spent`) re-arms (`end_surround_walk`, keeping the side last walked) on either of these:
   - the unit gains ground: `note_progress` resets `stalled_ticks` to 0 in `check_progress` or
     `check_pursuit_progress`;
   - the target or order changes (`flank_target`, as today).
2. **Why.** The retry stood for "the crowd changed". A crowd that changes so that this unit's way opens lets it
   advance, by `STALL_PROGRESS` (1/10, the existing progress unit), and that re-arms the walk. A crowd that never
   lets it advance, as in a corridor, never re-arms it, and the unit stands.
3. **State.** No new field. `flank_blocked_ticks` keeps its other use (the side swap). Removing the retry only
   removes a branch, so the checksum schema is unchanged.

### D-4. Queued behind a fighting friend is not stalled (OQ-1)

1. **Rule.** In `check_pursuit_progress` and in `check_progress` for an `AttackMove`, a tick is a **queued** tick
   when a friend that holds its ground lies within one step ahead. "Holds its ground" means `SteeringObstacle::anchored`:
   on hold, immobile, or anchored in combat by `is_anchored_in_combat`, which covers firing and standing with a live
   target under an attack order. The test is `friend_blocks_ahead` with "anchored" in place of "standing". On a
   queued tick, `stalled_ticks` does not advance, and `progress_best` is kept. The check reads the tick-start
   neighbours already gathered for the step.
2. **Who.**
   - Chases, and attack-moves with or without a target.
   - A plain `Move` keeps friendly-blocking D-3 and S8: a move into a blocked spot is given up.
   - Terrain stalls keep the stall rules unchanged.
3. **Why.**
   - Friendly-blocking D-3 already exempts a walking unit, because it "gains ground round the crowd". A unit that
     waits behind the same crowd for lack of room is in the same situation, and today it loses its order for
     waiting.
   - Before this plan, the timed retry hid that by walking.
   - The game's only data-defined end of an acquired chase is the leash (S1), which a queued unit never reaches.
   - For an ordered attack, S5's units outside the concave keep their orders and stand.
4. **OQ-1 (owner, not blocking).** May such a unit keep its attack order indefinitely? **Default: yes.** The
   alternative is today's 10 s give-up, then reacquire after 2 s, which shows up as give-ups in every corridor.
   Either answer is one condition in the code.
5. **State.** No new field.

### D-5. Measurement: proposal-caused and contact-caused reversals

`BlockingMeter` (`sim_fixtures::trace::blocking`) counts D-1.4 reversals from achieved velocities. To tell D-1's
class from D-2's without a probe, WP1 adds:

- **The proposed step.** `MovementWorkspace` keeps each mover's committed proposal velocity for the tick, in a
  retained buffer read through a new read-only `SimWorld::proposed_step(id) -> Option<SimVec2>`. This mirrors
  `route_outcomes()`. It is not hashed, and it is not part of `SimWorld` state that a checksum, snapshot or replay
  reads.
- **The split.** `BlockingMetrics` gains `proposal_reversals`, where the two proposals themselves reverse (the
  `is_reversal` test on proposed steps), and `contact_reversals`, the rest. `reversals` stays their sum, so every
  existing figure is unchanged. `matrix.csv` and `route_trace`'s summary gain the two columns. Old tables are still
  read, as WP1 of friendly-blocking did for its columns.

Class attribution (R-A to R-F) stays with the scratch probe (§2.1). Each rule WP re-runs it on its tree and records
in the evidence the counts per class, for the default seed and the reproductions.

### D-6. The reproductions' acceptance, restated

- **`ranged_block_reaches_the_arc`.** Un-ignored in WP1. It meets its friendly-blocking §7 acceptance today.
- **`showcase_second_line_reaches_range`** is renamed `showcase_crowd_edges_do_not_reverse`.
  - Its tick-900 criterion is dropped: it no longer measures B-103 (§2.2). Attackers in range at tick 900 are still
    reported in the evidence, not asserted.
  - It asserts no diagnostics, no give-ups up to the battle's end, and reversals per owner at most the bound of
    WP4's acceptance.
  - The test runs to the end (≤ 2,700 ticks, about 4 s in release), as the showcase now does.
  - The bound is the measured residual of the out-of-scope classes R-E and R-F, never a loosened total.
- **`no_room_means_waiting_still`.**
  - **Standing** is the simulation's own: an achieved step of at most `STANDING_DRIFT`, read from
    `sim_core::movement` through the existing re-export. If none exists, `sim_fixtures` names the same 1/64 with a
    doc pointing to the constant; **verify** which.
  - Exact zero is reported in the failure message and the evidence, not asserted: contact relaxation decays
    geometrically and reaches exact zero only after tens of ticks of invisible motion (§2.7).
  - The 95 % and zero-reversal bounds stay.
  - The restatement lands test-only in WP1, still ignored, with the base figures in its reason.
- **`walkers_at_the_crowds_edge_do_not_toggle`.** The bound tightens in WP2 from ≤ 10 per walker to **0 proposal
  reversals**, with the total recorded.

### D-7. Determinism, checksum, catalog

- No floats in state (C7).
- No new collections in the simulation crates beyond a retained buffer in `MovementWorkspace`, which is not hashed
  (C9).
- No new hashed field; `CHECKSUM_SCHEMA_VERSION` stays 30.
- `FLANK_RETRY_TICKS` is removed, and its doc's mention of friendly-blocking Amendment 4 change 5 moves to this
  plan's D-3.
- No catalog field and no identity in Rust (C5): tests use scratch RON archetypes, as `second_line_tests.rs` does.
- `sim_map` is untouched, so its digests are unchanged.

### D-8. Measurement and stop rules (every rule WP)

`<base>` is the tree before the WP and `<cand>` the WP's tree. Release tools are built per tree into
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` and copied aside. Output goes under
`~/scratch/crowd-edge/<wp>/`. Heavy runs are wrapped in `flock ~/scratch/gate.lock`.

| What | Command | Expectation |
|---|---|---|
| Matrix | `matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv` | Moves (WP2 to WP4). No overall winner flips |
| Battle 33 | `matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33` | Moves, if at all, without an overall flip |
| Showcase | `route_trace --scenario scenarios/showcase.ron --ticks 3000 --out <dir>/route`, default seed and `--seed 1` to `8` | D-8 stop rules below |
| Reproductions | `cargo test --release -p sim_core --test second_line_tests -- --include-ignored` | Each WP's acceptance |
| Class probe | `probe.diff` applied to a scratch copy of `<cand>`, classification scripts (§2.1) | Each WP's class goes to zero on the reproductions and the default seed |
| Goldens | `golden_replay_tests` native debug and release; `--target wasm32-wasip1 --release` under wasmtime; wasm debug once per WP | Re-recorded only with a proof header |
| Throughput | `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`, base and candidate alternated, three rounds | Every scenario ≥ 25 ticks/s; WP3 within 5 % of base or justified |

**Stop rules.** A breach stops the WP. The agent commits nothing beyond it and reports the trace: first divergent
tick, entity and rule.

1. **Winner.** The showcase's default-seed winner (player 1) and every matrix battle's overall winner over its seeds
   are unchanged. A flip is traced to its first divergence. With the rule switched off in a scratch build, the run
   must reproduce `<base>`. The flip is then classified:
   - rule-caused: a unit that used to step back now holds, or one that walked into a standing friend now does not;
   - or a defect: a unit stuck, a wall unsettled.

   The orchestrator decides by amendment. **Battles 26 to 29** (100 zerglings) carry most of the matrix's reversals
   and were the flips of friendly-blocking WP3, so they are watched first.
2. **Give-ups** (stall give-ups per owner) on every showcase seed: at most `<base>` + 1, and on the matrix totals at
   most `<base>`'s. WP4 is expected to take the corridor's to 0 and must not raise any.
3. **Deeper unit-ticks** (`route_trace` overlap meter): 0 / 0 on every showcase seed.
4. **Reversals.**
   - On the default seed, per owner: at most `<base>`'s.
   - Seeds 1 to 8 are reported per owner. A seed that rises above `<base>` + 5 for an owner is traced with the
     probe, and its new reversals are classified.
   - Matrix totals: no rise in the WP's own class (proposal reversals for WP2, contact reversals for WP3).
5. **Deeper inside friends.** WP3 adds a scratch check, not committed: after the settle stage, no mover ends deeper
   in a standing friend than it started (raw fixed point, `WALL_CONTACT_TOLERANCE`). It runs over the default seed
   and the reproductions, and any violation is a defect.
6. **Control runs.**
   - Each rule WP's candidate with its rule switched off (a scratch flag, not committed) reproduces `<base>`
     byte-for-byte on the showcase's `units.csv` and the matrix. That proves the wiring.
   - WP3 also runs with the R-B half alone (no terrain re-check in the settle), to report how much of the change is
     R-C.
7. **Tests.** A test that changes for a reason other than its WP's rule is restated first, test-only, with its
   traced cause (friendly-blocking deviation plan D-2). No bound is loosened without a trace. If acceptance is not
   reached once the rule is shown correct by its tests, the WP stops with the class figures. No tuning constant is
   introduced (charter §1.1, Q0051).

**Goldens: re-record rules.**
- `golden_replay` and `golden_replay_orders` are re-recorded only on the WP's final tree, in debug and release, and
  checked on `wasm32-wasip1`.
- The header follows the existing form: "Re-recorded for plan 2026-11-19-crowd-edge-steering WP<n> (B-103):
  checkpoint N first changed; entity E at tick T <what the rule did>. Proof: a scratch build of WP<n> with the rule
  off reproduces the previous checksums at every checkpoint."
- `DEFAULT_SANDBOX_CHECKSUM` (`sim_fixtures`) gets the same proof if it moves.
- A golden that does not move is recorded as a finding (for example, "no walk step in `golden_replay_orders`
  retreats").

---

## 5. File ownership

| Area | Files | WP |
|---|---|---|
| Records | `DESIGN.md` §4 (B-103 restated; B-144, B-145 added), `docs/plans/README.md` | WP0, WP5 |
| Proposed step (diagnostic) | `crates/sim_core/src/movement/mod.rs` (`MovementWorkspace`, `SimWorld::proposed_step`), `crates/sim_core/src/movement/locomotion.rs` (`MotionProposal::commit` records it) | WP1 |
| Meter | `crates/sim_fixtures/src/trace/blocking.rs`, `trace/matrix.rs`, `trace/route.rs` (columns, summary), their unit tests | WP1 |
| Reproductions | `crates/sim_core/tests/second_line_tests.rs` | WP1, WP2, WP4 |
| D-1 | `crates/sim_core/src/movement/locomotion.rs` (`surround_walk`), `crates/sim_core/src/steering.rs` (a `remove_blocked_velocity` variant with an extra half-plane, or a `HalfPlane` argument), unit tests | WP2 |
| D-2 | `crates/sim_core/src/movement/locomotion.rs` (the settle stage of `advance_checked_movement`), possibly a new `crates/sim_core/src/movement/body_settle.rs` with its `//!` doc (C4), `crates/sim_core/src/movement/collision.rs` doc (the overlap it no longer has to undo) | WP3 |
| D-3, D-4 | `crates/sim_core/src/movement/locomotion.rs` (`surround_walk`, `check_progress`, `check_pursuit_progress`, `friend_blocks_ahead`), `crates/sim_core/src/movement/tuning.rs` (`FLANK_RETRY_TICKS` removed) | WP4 |
| New tests | `crates/sim_core/tests/crowd_edge_tests.rs` (new, scratch RON archetypes, `fast` unless measured slow; `crates/gate/suites.ron` rule only if slow, C22), `crates/sim_core/tests/movement_contact_tests.rs` (property additions) | WP2 to WP4 |
| Goldens | `crates/sim_core/tests/fixtures/golden_replay*.ron`, `sim_fixtures` sandbox checksum | WP2 to WP4 |
| Storyboard | `assets/captures/showcase.ron` (only if the default-seed timeline moved) | WP5 |
| Evidence | `docs/evidence/2026-11-19-crowd-edge-steering.md` (new in WP1) | WP1 to WP5 |

**Coexistence.** No other open plan owns `steering.rs` or the locomotion functions above. Friendly-blocking's
Amendment 1 (the ramp E turn) stays with that plan. If a pathfinding package opens `locomotion.rs` at the same time,
the one merged second rebases and re-measures, as friendly-blocking §4 set out.

---

## 6. Work packages (commit order)

Branch `crowd-edge`. Implementation agents hand back after `cargo gate quick` and the tests their change needs. A
verifier runs `cargo gate package` on the final tree of WP1 to WP4 together (Q0061). Each rule WP appends to the
evidence file. Sizes: S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- This plan's row in `docs/plans/README.md` (`Proposed`).
- **`DESIGN.md` B-103 restated** with §2.2's figures:
  - showcase default seed 13 and 10, seeds 1 to 8 from 5 to 20 per owner;
  - the synthetic arc at 0;
  - the corridor's waiting units still on 15 to 46 % of ticks (65 to 88 % by `STANDING_DRIFT`);
  - "plan `2026-11-19-crowd-edge-steering`".
- **New rows:**
  - **B-144.** Steering takes the side for a single moving body crossing a unit's path from that body's position on
    each tick, so the unit's step flips as the body crosses (R-E; 7 showcase and about 230 matrix reversals). Open;
    a later plan.
  - **B-145.** A unit that steps along its own first route leg and takes another route (its group's lane, or a
    chase's first route) on the next tick steps back (R-F; 14 showcase reversals, 11 matrix). Open; group-routing's
    ground. The WP0 agent confirms the group-lane cause on tick 2 (**verify**) before writing the row, or writes
    the cause as traced.
- **Gate:** docs-only `package`.

### WP1: measurement, bit-identical (S)

- **Tests first.**
  - `blocking.rs` unit tests:
    - two steps whose proposals reverse count as a proposal reversal;
    - two achieved steps that reverse while the proposals do not count as a contact reversal;
    - `reversals` is their sum;
    - a table without the new columns still parses.
  - A `sim_core` unit test: `proposed_step` returns the committed proposal, and `None` for a body that took no part
    in movement.
- **Change.** D-5.
  - `ranged_block_reaches_the_arc` is un-ignored.
  - `no_room_means_waiting_still` is restated per D-6, still ignored, with the base figures (standing by drift
    88 / 88 / 69 / 65 %, 21 reversals) in its reason.
  - The showcase test is renamed and restated per D-6, still ignored, with the base figures (13 / 10, of which
    proposal and contact as WP1 measures).
- **Measure.** The baseline of D-8 on WP1's tree: matrix, battle 33, showcase nine seeds, reproductions, probe
  classes, throughput. It is recorded as the base of every later WP.
- **Bit-identical everywhere:**
  - matrix outcome columns;
  - battle 33 `seeds.csv`;
  - showcase `units.csv`;
  - both goldens and the sandbox checksum, unchanged and not re-run beyond `quick`.

  `proposed_step` is written, never read by the simulation.

### WP2: D-1, the walk never steps back (M)

- **Tests first** (`crowd_edge_tests.rs`, red on WP1).
  - **CE-1** `a_walk_never_steps_back_from_its_goal`.
    - **Setup:** the pillar and a firing line of five shooters (the walkers layout), plus one short-ranged walker
      placed so that a body lies ahead-left on its kept side. The placement is derived from the trace of §2.3
      (pulse bot 4's geometry), with that side forced through the side rule's parity.
    - **Assertions:** over 150 ticks, every committed walk step has a non-negative component along the way to its
      goal (read through `proposed_step` and the unit's goal), and the walker has 0 reversals.
    - **On WP1:** the walk step's forward component reaches about −0.035.
  - **CE-2** `a_walk_on_a_clean_arc_is_unchanged`. A guard: in the arc reproduction, the walk steps of every unit
    are bit-identical to WP1's (recorded positions over 120 ticks). Expected to pass on WP1 and after, which shows
    H₀ inactive on a convex arc. If it moves, that is a finding to trace, not a failure to loosen.
  - **Unit tests** (`steering.rs` or `locomotion.rs`):
    - an across step against a body at 45° ahead-left projects to a step with zero backward component, below the
      waiting share;
    - a body beside the walk (lateral, not ahead) leaves the step whole;
    - H₀ alone leaves an across step unchanged.
  - **The walkers test** tightens to 0 proposal reversals (D-6).
- **Change.** D-1.
- **Measure.** D-8. The probe's R-A count goes to 0 on the reproductions and the default seed. The corridor's
  reversals are expected to fall from 21 to about 4 (§2.3: 17 are R-A).
- **Expected:** the matrix moves (walks are everywhere in battles 26 to 29), and `golden_replay` moves (the
  attack-moving squads walk in ticks 1 to 50, friendly-blocking Amendment 7).

### WP3: D-2, no step ends inside a friend that stood (M)

- **Tests first** (`crowd_edge_tests.rs`, red on WP2).
  - **CE-3** `a_unit_does_not_step_into_a_leader_that_stopped`.
    - **Setup:** a column of two scratch shooters walking at full speed toward the pillar. The leader comes into
      range on tick T and stops (anchored).
    - **Assertions:** on tick T the follower's end position is no deeper inside the leader than at the tick's
      start, and the follower has 0 reversals over ticks T to T + 10.
    - **On WP2:** it overlaps by about 0.09 and is pushed back (the R-B trace).
  - **CE-4** `a_slide_along_a_wall_does_not_enter_a_friend_beside_it`.
    - **Setup:** a corridor wall, a firing friend standing against it, and a chaser whose steered step is refused
      by the wall and slides along it toward the friend (the R-C geometry).
    - **Assertion:** the committed step does not deepen the overlap with the friend.
    - **On WP2:** it does, by about 0.03.
  - **CE-5** `a_column_still_marches_closed_up`. A guard: a column of six walking units keeps its spacing exactly
    as on WP2 (positions bit-identical over 100 ticks), because no body stands. Expected to pass on WP2.
  - **CE-6** (property, `movement_contact_tests.rs`, 64 cases) `no_mover_ends_deeper_in_a_standing_friend`.
    - **Setup:** seeded layouts of 4 to 12 scratch bodies on a flat map, with seeded orders (attack-move,
      hold, idle).
    - **Assertion:** after `step_movement_and_steering`, no body that moved ends deeper in a friend that stood and
      does not yield than it started, within `WALL_CONTACT_TOLERANCE`.
    - **On WP2:** it fails on layouts with a stopping leader.
  - **Unit test** for the settle stage: two movers and one standing friend give the same result in either processing
    order.
- **Change.** D-2, with its PERFORMANCE RATIONALE block. The `collision.rs` module doc gains: "Overlaps with a
  friend that stood this tick are not made by steps (plan `2026-11-19-crowd-edge-steering` D-2); what remains to
  separate is between bodies that both moved, or with enemies."
- **Measure.** D-8, with the R-B-only control. The probe's R-B and R-C counts go to 0 on the default seed and the
  reproductions, and R-D falls (its source is gone; what remains is traced). The throughput row is reported.
- **Expected:** the matrix, both goldens and the sandbox checksum move.

### WP4: D-3 and D-4, the queue stands (S to M)

- **Tests first** (red on WP3).
  - `no_room_means_waiting_still` (D-6 form) is un-ignored. It must pass:
    - 95 % standing by `STANDING_DRIFT` after the first stop;
    - 0 reversals;
    - 0 give-ups;
    - at least one unit left out.
  - **CE-7** `a_queued_chaser_keeps_its_target`.
    - **Setup:** the corridor, with one firing shooter at the front and one chaser behind it.
    - **Assertions:** over 600 ticks (beyond `STALL_ABANDON_TICKS` + `ABANDONED_TARGET_TICKS`), 0
      `PursuitAbandoned`, and the chaser stands on ≥ 95 % of its ticks.
    - **On WP3:** it walks every 24 ticks. With only the retry removed (a scratch step that shows D-4 is needed), it
      gives up at tick 250 + its first stop.
  - **CE-8** `a_spent_walk_resumes_when_the_way_opens`.
    - **Setup:** the walkers layout with a short firing line. Ticks are run until a walker is spent, then the firing
      shooter in front of it is killed (`life` set to zero through the test's world access, as other tests do;
      **verify** the helper).
    - **Assertions:** the walker advances, its walk re-arms, and it reaches range within 200 ticks.
    - **On WP3:** passes, through the timed retry. After the change it must pass through the progress re-arm. This
      is a guard, written first.
  - **CE-9** `a_blocked_move_is_still_given_up`. A guard for S8: a plain `Move` into a spot plugged by a holding
    friend ends with `NoProgress` after `STALL_ABANDON_TICKS`, unchanged from WP3.
- **Change.** D-3 and D-4. `FLANK_RETRY_TICKS` is removed; the `surround_walk` doc and `tuning.rs` are updated.
- **Measure.** D-8. Corridor and CE-7 give-ups are 0. Showcase and matrix give-ups do not rise (stop rule 2).
  Battles 26 to 29 are watched: zerglings that used to retry now wait, unless they gain ground.
- **Expected:** the matrix moves where walks were spent (the zergling battles). The goldens move only if a golden
  unit's walk was spent (**verify**; recorded either way).

### WP5: storyboard and bookkeeping (S)

- **Storyboard.** If the default-seed timeline moved in WP2 to WP4, `assets/captures/showcase.ron` is re-checked
  with a real render on the RX 580, `with-gpu-lock` per take (charter §7), as structure-contact WP3 did.
  `showcase_storyboard_tests` and `showcase_tests` pass.
- **`DESIGN.md`.**
  - B-103 is removed as fixed, with commits, or narrowed to what WP2 to WP4's evidence leaves, each residual
    reversal named by class.
  - The Movement row: "… a blocked unit at a crowd's edge never steps back (its walk keeps its ground), no step
    ends inside a friend that stood that tick, and a queue that cannot advance stands (B-103 fixed, plan
    `2026-11-19-crowd-edge-steering`)", with `crowd_edge_tests.rs` added.
  - §5 only if throughput moved beyond noise.
- **Plans.**
  - Friendly-blocking gets a one-line pointer amendment: B-103 is carried here, and its Amendment 4, change 5 (the
    timed retry) is superseded by D-3.
  - This plan's row becomes `Done` with commits.
  - OQ-1 is recorded as answered by default, or filed if the orchestrator asked for a file.
- **Gate:** `merge` by the orchestrator.

---

## 7. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Meter | Proposal and contact reversals told apart; old tables read | WP1 unit tests |
| Walk | Never steps back; unchanged on a convex arc; the walkers test has 0 proposal reversals | CE-1, CE-2, unit tests, `walkers_at_the_crowds_edge_do_not_toggle` |
| Settle | No step into a friend that stopped; slide re-checked; columns still close up; order-independent; never deeper (property) | CE-3 to CE-6, unit test |
| Queue | The corridor stands; a queued chaser keeps its target; a spent walk resumes when the way opens; a blocked `Move` is still given up | `no_room_means_waiting_still`, CE-7 to CE-9 |
| Reproductions | Arc 0 reversals (un-ignored); showcase to the end within bound | `second_line_tests.rs` |
| Determinism | Wiring controls; goldens with proof headers; three targets | D-8 |
| Charter | C4, C5 (scratch RON), C7, C9, C11 (PERFORMANCE RATIONALE format), C12, C17, C18, C22 | `workspace_checks`, `suite_catalog_tests` |

---

## 8. Determinism and throughput

- **No new hashed state; schema 30 is kept.** D-1 is arithmetic on existing values. D-2 is a stage that only shortens
  steps, order-independent by construction (D-2.2). D-3 and D-4 read existing fields.
- **The proposed-step buffer (WP1)** is outside every checksum, snapshot and replay. A test asserts that the
  checksum of a world is equal with and without it being read.
- **Cross-platform.** Fixed-point only; CI's determinism jobs cover Linux, Windows, macOS ARM64 and `wasm32-wasip1`
  on the push.
- **Throughput.**
  - D-1 adds one constraint to an existing projection, for walking units only.
  - D-2 adds one grid rebuild and one neighbour query per mover. Its expected cost is below collision's own pair
    pass, which visits every pair.
  - D-3 removes work.
  - The bench (`throughput_scenarios.ron`) is measured in WP1 (base) and WP3 and WP4. Every scenario must stay
    ≥ 25 ticks/s (charter §3.1).

---

## 9. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| K-1 | The zergling battles (26 to 29) flip. Their surround depends on walks, and friendly-blocking WP3 flipped 28 once | Stop rule 1 | Watched first. D-1 only removes retreating walk steps, and a flip is traced to a unit that now holds where it stepped back. Rule-caused flips go to the orchestrator by amendment |
| K-2 | H₀ closes sides that a retreat-then-forward used to open, so fewer units reach range | Fewer shooters, longer battles | CE-2 (convex arc unchanged), walkers 17 / 17 kept, attackers in range reported per seed |
| K-3 | D-2 slows columns behind a leader that stops and starts | Marches end later | CE-5 (moving pairs untouched); matrix end ticks compared |
| K-4 | D-2's terrain re-check leaves units refused at corners | Units wait at a wall beside a friend | The waiting rule applies, D-1.4 holds; deeper unit-ticks stay 0 (stop rule 3) |
| K-5 | D-4 keeps units queued for good where a game player would expect a give-up | Units stand behind a choke indefinitely under attack orders | That is S5's "stand"; OQ-1 asks the owner; a plain `Move` keeps S8 (CE-9) |
| K-6 | D-3 misses a crowd change that opens a side but not the way ahead | A spent unit stands next to an opening | Re-arming on ground gained covers deaths in front; a side opening without any advance is recorded if seen (probe: spent units with an open side), and only an amendment adds a further re-arm |
| K-7 | Throughput of D-2 in 400-unit fights | Below real time | Retained grid and buffer; bench in WP3; the stage settles movers only |
| K-8 | The showcase's default-seed winner flips (the lane fight is close; it flipped under contact, inner-radius and structure-contact before) | Stop rule 1 | Trace from the first divergence with controls; orchestrator amendment, as before |
| K-9 | Residual reversals stay in R-D (pairs eased apart while firing, battle 35) | Totals do not reach 0 | Reported by class. A rule for easing firing pairs apart is not in this plan (friendly-blocking D-4 keeps it), and the register is narrowed honestly |

---

## 10. Out of scope and decisions recorded (Q0002)

Nothing left out is accepted as a deviation by being left out.

- **R-E, crossing body (B-144).** A side rule for moving bodies (pass behind the body, by its lateral velocity)
  needs a threshold for "moving across" that no source gives. Deferred to its own note or plan.
- **R-F, route adoption (B-145).** Group-routing's or pathfinding's ground.
- **Enemy contact.** D-2 does not settle against enemies. M-21 (melee rings, excused by D-1.4) stays with the
  deviation plan's WP16 and B-50 (Q0035).
- **Easing two firing friends apart** (pair-anchored relaxation, R-D in battle 35): kept as friendly-blocking D-4.
- **`SeparationRadius`** (B-49), **lateral acceleration** (B-92) and **U-23** (steering tuning): unchanged. This
  plan adds no constant and removes `FLANK_RETRY_TICKS`.
- **Friendly-blocking Amendment 1** (ramp E turn, 211° against at most 115°): stays with that plan.
- **The probe** stays scratch. Only the proposal/contact split is committed (D-5).
- **The showcase test's tick-900 criterion** is replaced (D-6), because the scenario changed under it, not to make
  it pass.

## Amendment 1 (orchestrator, 2026-11-19): WP2 (D-1) deferred to a re-plan; WP3 and WP4 run next on WP1's tree

**Why.** WP2 stopped under the acceptance rule (evidence §2). The stop was caused by four things:

- **T1:** D-1 forbids the only way out of a pocket behind a firing ring.
- **T3:** the bounded cyclic projection does not close the sides D-1.2 says it closes, so the cost moves into
  contact.
- **T4:** CE-2's premise does not hold while the arc forms.
- **T2:** the walkers' remaining reversals are walk against steering, which D-1 does not address.

The exact projection and a changed walk-end test were tried as scratch variants, and both cost reach. D-1 as written
is therefore withdrawn. Its side branch `crowd-edge-wp2-wip` is kept for reference, and it is not built on.

**Decision.**

1. **WP3 (D-2) is next, on WP1's tree.** D-2 does not depend on D-1. It targets R-B and R-C, the largest
   contact-caused classes. WP3's own tests, controls and stop rules apply unchanged. The R-B-only control is kept.
2. **WP4 (D-3, D-4) follows on WP3.** Q0089's default is yes. Where a WP4 test or acceptance figure assumed D-1's
   effect, restate it from the WP3 tree's own measurement before the code changes, and record each restatement in
   the evidence. Bounds are never loosened in the process: a figure is restated only when it was derived from D-1.
3. **WP5 then closes the package without D-1.** R-A stays open under B-103 with the measured residuals. The B-103
   row records the evidence §2 traces and the WP4 tree's figures.
4. **A new D-1 is a separate Stage 1 plan,** written after this package merges, from the merged tree's
   measurement. It starts from evidence §2d: a rule on the walk's end rather than on its retreat, CE-2 restated
   from tick 98, and walk-against-steering reversals (T2).
5. **Acceptance for WP3 and WP4.**
   - Reversals are compared against WP1's base, per the plan.
   - The walkers test keeps 17 of 17. If D-2 or D-4 costs reach there, that is a stop with a trace.
   - The arc and corridor tests are judged on their D-6 restatements.

## Amendment 2 (orchestrator, 2026-11-19): D-2 withdrawn; WP4 (D-3, D-4) runs on WP1's tree; R-A to R-C go to a new plan

**Why.** WP3 stopped on stop rule 1: matrix battle 31's overall winner flips (evidence §3, on `crowd-edge-wp3-wip`).

- The flip is rule-caused: a marauder no longer walks into a friend that has just stopped.
- D-2 does not pay for that flip. Reversals of record rise from 6,154 to 7,013, with proposal-caused reversals up
  ×1.3. Battles 26 to 29 rise.
- Showcase seeds 2, 6 and 8 flip, and seeds 1 and 3 do not end by tick 3,000.
- The corridor gets worse: 1 of 6 in range and 2 give-ups.

The flip is therefore not accepted, and D-2 as written is withdrawn. With D-1 withdrawn too (Amendment 1), the
plan's diagnosis stands but its rules for R-A, R-B and R-C do not. These classes go to a new Stage 1 plan, written
from evidence §2 and §3 after this package merges. Its starting points:

- the walk's end test (§2d);
- why shortened steps raise proposal reversals (§3);
- whether a contact-stage rule can stop units walking into stopped friends without changing battle outcomes.

**Q0089 is answered (a).** A unit queued behind fighting friends keeps its attack order. A plain `Move` stall-out
and the 21 acquisition leash stay. The question is closed.

**Decision.**

1. **WP4 (D-3 and D-4) runs next, on WP1's tree (`6874f25f` plus the main merge),** without D-1 or D-2. D-4
   implements Q0089's answer. D-3 replaces the timed retry, which was what kept corridor units from giving up and
   which D-4 now makes unnecessary.
   - Restate any WP4 test or acceptance figure that assumed D-1 or D-2 from WP1's own measurement first, and record
     it. Never loosen a bound.
   - The stop rules are unchanged. A winner flip stops the package with a trace, as before.
2. **WP5 then closes the package.**
   - Bring evidence §2 and §3 onto the branch from the two side branches, as documentation only.
   - The B-103 row is restated: what WP4 fixed, R-A to R-C open with the measured residuals, and a pointer to the
     new plan.
   - The side branches stay until the new plan is written. Then they are deleted.

## Amendment 3 (orchestrator, 2026-11-19): D-4 lands alone; D-3 goes to the new plan; WP5 closes the package

**Why.** WP4 stopped on stop rules 1 and 2 (evidence §4, on `crowd-edge-wp4-wip`). Matrix battle 31's overall winner
flips, and give-ups of record rise from 2 to 3.

- **The cause is D-3.** With D-3 off, the traced seeds reproduce WP1 exactly. Without the timed retry, a spent walk
  never re-arms when a side opens without the unit advancing (risk K-6, red stalker 23 at tick 135).
- **D-4 alone** (Q0089 answer (a)) moved nothing measurable on the traced seeds or in the corridor. It moves
  `golden_replay` from tick 397, with a proof header.

The flip is not accepted. Re-arming a spent walk needs a defined "a side opens" test, which belongs to the new R-A
to R-C plan.

**Decision.**

1. **WP4 is restated as D-4 alone.** The timed retry (`FLANK_RETRY_TICKS`) stays.
   - Take D-4, CE-9 and the D-4 parts of the evidence from `crowd-edge-wp4-wip`.
   - Re-run the full WP4 measurement for D-4 alone against WP1, under the unchanged stop rules: the matrix of
     record, all seeds' give-ups, battle 33, the showcase on 9 seeds, throughput, and the goldens with proof
     headers.
   - CE-7 and CE-8 (D-3's tests) and the corridor's standing bound move to the new plan as ignored tests or as
     recorded figures. They are not committed red.
   - If D-4 alone breaches a stop rule, stop again with a trace.
2. **WP5 closes the package.**
   - Carry over evidence §2 to §4 as documentation.
   - Restate the B-103 row: D-4 landed, while R-A, R-B, R-C, the walk re-arm (D-3, K-6) and the corridor's
     standing stay open, with the measured residuals, under the new plan, which is still to be written.
   - Add the throughput figures to the evidence.
   - The WP5 storyboard re-check applies only if the showcase moves.
3. The side branches stay until the new plan is filed.

## Amendment 4 (orchestrator, 2026-11-19): open items carried to plan crowd-edge-steering-2

R-A to R-D, the walk's re-arm (this plan's D-3, risk K-6) and the corridor's standing are carried by plan
[`2026-11-19-crowd-edge-steering-2`](2026-11-19-crowd-edge-steering-2.md). The side branches `crowd-edge-wp2-wip`,
`crowd-edge-wp3-wip` and `crowd-edge-wp4-wip` are kept as diffs under `~/scratch/crowd-edge-2/patches/` and are
deleted once that plan's WP0 is merged.
