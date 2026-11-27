# Blueprint: Contact-Layer Acceptance (shipping `Gated` WP1's C′ without WP2's gated intent)

This plan answers the owner's choice of **Q0143 (c)**: plan and run a full acceptance pass for shipping the movement
redesign's contact layer, C′ (plan [`2026-11-27-gated-movement`](2026-11-27-gated-movement.md) WP1, merged latent on
`main` under `contact.pipeline: Gated`), **without** WP2's gated intent I′. The acceptance must:
- verify C′'s own zig-zag (step-reversal) figures, which were never measured alone;
- confirm every throughput benchmark holds at least 50 ticks per second (Q0093);
- isolate Hold Position immovability, enemy non-pushing, and the crowd-overlap reduction the owner quoted
  (44–49 % → 10–14 %);
- decide afterwards whether any WP2 follow-up is needed.

It writes no new movement rule. It measures the code already on `main` and, if the acceptance holds, ships it by
changing one data value. Every threshold below is registered before any of this plan's measurements runs.

**Status.** Written on 2026-12-02 (session date 2026-11-27) on branch `contact-layer-plan`, cut from `origin/main`
`2516a068` (checksum schema 62; `Solver` shipped; `Gated` = C′ only, latent).
- **Dependency: wall-clearance WP1 (B-240) merges to `main` first.** It changes corner hand-offs in `locomotion.rs` and
  moves the gap, corner and lanes families and matrix battle 40 (evidence `2026-11-27-gated-movement` §12 on branch
  `wall-clearance-wp1`, "Base re-measured after wall-clearance WP1"). This plan's base of record is **`main` after
  that merge** (named `M₀` below). WP-A does not start before it.
- **Line numbers drift,** so search for the quoted symbol. Claims not confirmed while writing are marked **verify**.
- **Later decisions** are appended as `## Amendment N (role, YYYY-MM-DD): …`.
- **Numbers are next free at merge.** Bug rows and questions are drafted in §11 and filed only when the step that
  needs them runs. Questions on `origin/main` end at Q0143; branch `missile-throw-plan` holds Q0144 and Q0145, so the
  next free number at this writing is **Q0146**.
- **Schema.** C′ adds no hashed field (Gated plan WP1, "the schema is unchanged"), and the catalog is not hashed. The
  switch-on therefore moves checksum **values**, not the checksum **format**, and needs **no schema bump**. If a
  step of this plan ever adds a hashed field (none is planned), it takes the next free schema at merge: `main` is at
  62, 63 is reserved for hangar commands, and 64 may go to the missile plan.

**Why a full plan (charter §6).** It changes the shipped movement, so it moves battle outcomes and every golden
replay, and it spans a measurement package and a switch-over package.

**Inputs read.**
- AGENTS.md (§3.1 determinism and throughput, §4, §6, §7, §8), `docs/questions/README.md`, Q0143 and its answer.
- Plan `2026-11-27-gated-movement`: §1, §3.1 (C′), §3.4, §6 (acceptance and stop rules), §7 WP1 and WP4, §9 (cost),
  §10, and Amendments 3, 3a and 3b (the owner's 2026-11-29 rules and their precedence).
- Evidence `2026-11-27-gated-movement`: §1 (method), §3.3 (the frozen close-battle list), §7 (throughput), §11 (WP1
  after Amendment 3b) on `main`; §12 (the wall-clearance re-base) on `wall-clearance-wp1`; §12 to §14 (WP2, both
  stops, the reversal diagnosis) on the parked branch `gated-wp2` (`0fee7be9`, reference only, never deleted).
- Evidence `2026-12-01-death-spawns-and-hangar-commands` Amendment 1 and WPA4 (the callgrind method and the
  64x64/400-unit row's 54.44 M instructions per measured tick).
- Plan `2026-11-29-wall-clearance` (on `wall-clearance-wp1`): its held-out sets HA and HB, and its window X, which
  waits for the end of the movement redesign.
- `DESIGN.md` §4 (B-192, B-193, B-209, B-219, B-220, B-225, B-236, B-240, B-241) and §5.
- Code at `2516a068`: `assets/catalog/movement_rules.ron` (`contact`), `catalog/movement_rules.rs`,
  `movement/solver.rs`, `movement/collision.rs`, `contact_solver_tests.rs` (the two tests that pin the shipped
  pipeline to `Solver`), `crates/sim_core/benches/throughput_scenarios.ron` (ten rows), `checksum.rs` (schema 62).

**Questions and standing rules.**

| Question or rule | State | Where |
|---|---|---|
| Q0143 (c): ship C′ without I′ after a full acceptance; ≥ 50 ticks/s on every benchmark; isolate H-1, H-2 and the overlap fall; then decide on WP2 | Followed | the whole plan |
| Owner, 2026-11-29 (Gated Amendment 3a): Hold Position units never move; allies may nudge attacking units only within their weapon band; enemies never push each other | Hard checks H-1, H-1b, H-2 | §4.1 |
| AGENTS.md §8, Q0103, owner 2026-11-24: natural movement outweighs a close matchup's winner; restate flipped claims with traces; a broken behaviour is a bug | Followed | §4.5 |
| Q0093 (c), Q0102 (a): every throughput scenario ≥ 50 ticks/s; overlap tolerances are not traded for instruction ratio | Followed: C-0 | §4.4 |
| Q0124 (a): `Gated`'s instructions at most `Solver` + C-3′ (R) | Superseded for this ship by Q0143 (c), whose option text stated "up to 1.65×" and whose answer names only ≥ 50 ticks/s: reported, not judged (decision D-3) | §4.4 |
| Gated §6.6 rule 6: no third design round without the owner | Followed: WP-C files a question rather than start one | §6 WP-C |
| Owner, 2026-11-20: no fitting to the test set; simpler deviations allowed if recorded | Followed: fresh acceptance and held-out seeds, run once | §3 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing | §6 |
| Q0002: scoping is the agent's call if written down | Followed | §2, §10 |

---

## 1. Summary

1. **What ships, if accepted.** `contact.pipeline: Gated` in `assets/catalog/movement_rules.ron`, with the code
   already on `main`:
   - Hold Position units are never moved by another body;
   - attacking units may be nudged by friends along their ring, never out of their weapon band (D-δ under `Gated`,
     melee and ranged);
   - enemies block and never push (E-2);
   - a body terrain holds passes its share to its partner (C′-3, B-220);
   - C-3′, the crowd extension, runs under `Gated` at its derived cap of 151.

   I′ (the gate) and G′ (orbit approach, retarget from a full ring) are not part of it. `gated-wp2` stays parked.
2. **Three packages.** WP-A measures `Gated` as it stands on `M₀` against `Solver` and judges it (no code). WP-B
   switches it on, re-records the goldens with the four-build proof and restates the matrix. WP-C applies a
   pre-registered rule to the shipped figures and decides whether a WP2 follow-up is needed; if one is, it goes to the
   owner.
3. **Zig-zag first.** T1 (M1 and M1-hi, the owner's churn condition of Q0120) is read before any other figure and is
   WP-A's first stop, exactly as WP2's was.
4. **Throughput by instructions.** Each benchmark row is counted by callgrind per measured tick in one session that
   also measures the 64x64/400-unit `Solver` anchor (54.44 M). A projection from each row's own quiet-host rate must
   clear 55 ticks/s (a 10 % margin over 50). No wall-clock figure is of record above load average 8.
5. **Seeds.** Development: the seeds WP1 already read. Acceptance (harness 300,001 to 301,000) and held-out (harness
   9,100,001 to 9,101,000; matrix the tool's 45th to 68th non-record seeds) were registered by the Gated plan and never
   run (**verify** in WP-A that no scratch directory holds worlds on them). Each runs once.
6. **One amendment in all,** chosen from two registered candidates (§5). A second failure ends the plan with `Solver`
   still shipped and a question to the owner.

---

## 2. What is shipped, exactly

### 2.1 The arms

All arms are played from one release binary per tree, selected by a scratch catalog directory (matrix, battles,
clashes, showcases, throughput) or by the harness's flags, as Gated evidence §1 does:

| Arm | Catalog | What it is |
|---|---|---|
| **S** | shipped | `Solver`, C-3′ off: today's movement, the base every figure is judged against |
| **R** | `crowd_iterations: 151` | `Solver` with C-3′ on (the Q0118 crowd fix): reported, to separate C-3′'s share of the overlap fall from C′'s |
| **L** | `pipeline: Legacy` | reported only (identity and cost) |
| **G** | `pipeline: Gated` | the candidate: C′ with C-3′ at 151, `moving_friends_push: true` (D-β as shipped) |
| **Gk** | `pipeline: Gated`, `gated.crowd_iterations: 21` | the registered amendment candidate A-1 (§5), played only if A-1 is triggered |

The harness's `--pipeline` flag gains no value in this plan; if it cannot select `Gated` on `M₀` (**verify**), WP-A
uses the Gated evidence's method, a release binary of the same tree with `pipeline: Gated` in the embedded catalog
(`…/contact-layer-relg`), and proves the S arm of both binaries byte-identical first.

### 2.2 Decisions (Q0002, recorded)

- **D-1: D-β stays on under `Gated`.** Q0122's measurement belonged to WP2 (moving friends that block need the gate).
  Without the gate, C′ uses the shipped `moving_friends_push: true`, as WP1 was measured.
- **D-2: Battle 28 keeps `known_bug: Some("B-192")`.** B-192 (standing behind a full ring) is G′-3's, which is not
  shipped. If battle 28's claim holds under G on all four seeds of record, the marker would become stale
  (`StaleKnownBug`); WP-B then removes it and restates B-192 with its D-R10 figures (the bug stays open while B1
  shows units standing with room in battle 28).
- **D-3: Cost bar.** C-0 (≥ 50 ticks/s, projected with margin) is the bar. C-1 (`Gated` ≤ R) is reported. On the
  wall-clearance re-base G already exceeds R on five of six measured rows (flat64 +11.9 %, casters +11.6 %), and Q0143
  presented option (c) to the owner at "up to 1.65×". The owner may reverse this by reopening Q0143.
- **D-4: No WP2 code.** Nothing from `gated-wp2` is cherry-picked. WP-C may only recommend.

---

## 3. Seeds, base, noise

- **Base of record.** `M₀`, arms S, R and L. The wall-clearance agent's re-base (its evidence §12) measured these arms
  on `0c8969e9`. WP-A re-plays S on `M₀` for every figure it judges, from the same binary as G. If `main` has changed
  simulation code since `0c8969e9` (beyond the merge), the step is recorded and the base is re-measured in full.
- **Development** (judging, already read by Gated WP1 and WP2): harness seeds 1 to 1,000, all 13 families, the mirror,
  and the permutation mode (offset 7); the matrix's 24 seeds (four of record and the tool's first 20 others); battle
  31 (seeds of record plus 100), battle 33 (plus 64); both 400-unit clashes (`pulse_bot`, `razor_mantis`) on seed 7
  and seeds 1 to 4; the two showcases on the default seed and seeds 1 to 8, 1,500 ticks.
- **Acceptance, run once at the end of WP-A:** harness 300,001 to 301,000, all families and the mirror, arms S and G.
  Every judged figure is judged on it with the same thresholds.
- **Held-out, run once in WP-B before the switch commit:** harness 9,100,001 to 9,101,000 and the matrix's 45th to
  68th non-record seeds, arms S and G. They confirm the direction of every judged figure and the restated claims
  (§4.5). Nothing is widened or tuned from them.
- **Not used:** wall-clearance's HA (1,001 to 1,024; 9,200,001 to …; read by its window W) and HB (reserved for its
  WP3).
- **Beyond noise.** The candidate's per-seed (or per-scene) mean lies outside S's 95 % bootstrap interval of the mean
  (2,000 resamples, seed 7) on the worse side. The harness verdicts are `movement_harness --aggregate` with the
  contact-solver plan's Amendment 8 step rule.
- **Every run** is played alone under `flock ~/scratch/gate.lock`. Builds use
  `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` (plus suffixes for the arm binaries). Scratch material
  lives under `~/scratch/contact-layer/`; it is never committed.

---

## 4. Acceptance (pre-registered)

The order of reading is binding: §4.2 (T1) first; if it holds, §4.1, §4.3, §4.4, the step rule, then the battles.

### 4.1 Hard checks (zero tolerance, never traded)

These are the Gated plan's §6.2 under Amendments 3, 3a and 3b, unchanged.

| ID | Check | Exclusions, reported apart |
|---|---|---|
| **H-1** | No **holding** unit is moved by another body: `held_moved` = 0 | overlap present at a pair's first contact (spawn, unload); an ability's displacement (Abduct, battle 37a) |
| **H-1b** | No attacking unit leaves its weapon band because of a friend (in its band at the tick's start round where its target ends, out of it at the tick's end): 0. The count of attackers nudged and the largest nudge are reported | a unit spawned on its target (battle 20's Broodlings, B-236) |
| **H-2** | Enemies never push (repaired meter, Amendment 3 item 4): 0; the largest one-class enemy overlap ≤ 0.5 % of the radius sum at the end of every tick | spawn ticks (B-236) |
| **H-3** | Order independence: permutation mode, unexplained differences = 0 (judged here; WP1 reported 14,000 of 14,000 identical) | — |
| **H-4** | Determinism: every golden and pinned digest identical on native debug and release and `wasm32-wasip1` release and debug | — |
| **H-5** | Disc penetration into terrain ≤ 2⁻¹⁰ world units | — |
| **H-6** | While `Gated` is latent (WP-A), S and L worlds of WP-A's tree are byte-identical to `M₀`'s | — |
| **H-7** | Stutter: a lag of one tick and the resumed fire in every `stutter` scene | — |

Where: every harness family, the matrix, both clashes, battles 31 and 33, the showcases. S and R are expected to fail
H-1, H-1b and H-2 (D-δ's radial share, B-225; B-193's enemy split). Their counts are reported beside G's: that is the
isolation the owner asked for.

### 4.2 T1, the zig-zag measure: the first stop

M1 is reversals per 1,000 moving unit-ticks (a moving tick is a step above `v / 8`; a reversal is two consecutive
steps of at least `v / 4` whose cosine is below −0.5); M1-hi is the same for steps of at least `v / 2`
(`trace/contact_checks.rs` and `harness/metrics.rs`, one definition, W0-T8). As registered by the Gated plan §6.3 for the owner's Q0120 condition, in
**every** harness family and in battles 26 to 29 (each side) and both clashes:
- G's M1 and M1-hi are not above S's beyond noise;
- G's M1 is ≤ 1.0 and M1-hi ≤ 0.25 wherever S meets those bounds.

The battle meter's split of reversals into proposal-caused and contact-caused (`trace::blocking`) is reported for S
and G (**verify** that `main`'s module gives the split as `gated-wp2` §12.3 read it; if not, the column is reported
from the trace tree, never added as a judged figure).

**Known risk, registered now.** On the wall-clearance re-base, R (C-3′ on under `Solver`) has deep-queue M1-hi 5.42
against S's 1.66. G carries C-3′ at the same cap. If G fails T1 in a family where R also fails it and G's
`solver_extended_ticks` there is positive, the failure is attributed to C-3′, and amendment A-1 applies (§5).

### 4.3 Crowd overlap (the owner's 44–49 % → 10–14 %)

Friend overlap is measured from the first stepped tick, over pairs the solver corrects, in friend-crowd WP0's columns.
"Largest" is a seed's largest friend overlap as a share of the radius sum; "above bound" counts friend pair-ticks above
D-α's moving bound (5 %).

| ID | Check | WP1's development figures (Gated evidence §11.5), for reference |
|---|---|---|
| **F-1′** | Battles 26 to 29 and both clashes: G's friend pair-ticks above the bound at most **20 % of S's** (a fall of at least 80 %). Every other battle and family: not above S's beyond noise | 10.8 %, 3.7 %, 11.1 %, 6.0 %, 0.2 %, 3.4 % of S's |
| **F-2′** | Battles 26 to 29 and both clashes: the mean over seeds of the largest friend overlap ≤ **10 %**, and no seed above **15 %** (three times D-α's bound) | means 8.42, 7.63, 7.10, 7.89, 8.81, 7.92 %; largest 13.66 % |
| **F-2 strict** (reported) | The Gated plan's F-2, largest ≤ 10 % on every seed | fails in battles 26 to 29 on development |
| **F-3′** | Harness: scenes with `fo_move_max` above 5 % not more than S's beyond noise in every family; the T7 maxima of arc and corner not above S's | not measured for G at WP1 |

The thresholds are round bounds under the owner's quoted range, set from the development figures WP1 already
published and registered before the acceptance and held-out seeds are run. R is reported in every row, so the share
of the fall that C-3′ alone gives (R against S) and the share C′ adds (G against R) are both on the table.

### 4.4 Throughput (Q0093: ≥ 50 ticks/s on every benchmark)

**Method (load-independent; the death-spawns evidence's).** One session under one hold of the gate lock, release
build of the tree, a scratch bench copy that reads its scenario table from a file (never committed). For each of the
ten rows of `throughput_scenarios.ron` and for arms S and G (and R, reported), `valgrind --tool=callgrind` counts
instructions (Ir) at 0 and at 300 measured ticks with the same warm-up. The difference divided by 300 is **Ir per
measured tick**, `I(arm, row)`.

- **Anchor.** `I(S, 64x64/400)` must reproduce **54.44 M** within 0.5 %. If it does not, the session's tree differs
  from the anchor's; the step is recorded and the session's own S figure is the anchor.
- **Projection.** For each row, `Q(row)` is the row's quiet-host instruction rate under S of record: its quiet ticks
  per second (Gated evidence §7: 107 for 64x64/400, 178 for 128x128/400, 82 for the canyon, 200 for casters, 388 for
  hangar) times `I(S, row)` of this session. A row with no quiet rate of record (64x64/0, 64x64/100, 256x256/0, brood,
  razed) takes the lowest `Q` of the recorded rows (64x64/400's, about 5.9 G/s). The projected rate is
  `P(G, row) = Q(row) / I(G, row)`.
- **C-0 (hard): `P(G, row)` ≥ 55 ticks/s on every row.** The 10 % above 50 covers a lower instructions-per-cycle in
  `Gated`'s solver than in the row's `Solver` reference. For scale, the wall-clearance re-base's perf counts project
  64x64/400 to about 65 and the canyon to about 60.
- **Native confirmation.** If the host's 1-minute load stays below 8 for a whole run, one native run per row under G is
  recorded, and it must be ≥ 50. If no quiet window occurs during WP-A and WP-B, the projection stands as the figure of
  record and the evidence says so. No figure taken above load 8 is of record.
- **Reported:** `I(G)/I(S)` and `I(G)/I(R)` per row (C-1, D-3); mean solver iterations per tick and
  `solver_extended_ticks`; CI's `throughput` job (floor 25) on the switch commit.
- **Allowed cost steps** (behaviour-preserving, proved by bit identity of every harness CSV and matrix world): the
  enemy-pair index of E-2, and skipping C-3′'s excess scan on ticks whose solve ends within the bound (Gated §9). Used
  only if C-0 fails, and none counts as the amendment.

### 4.5 Step rule, known answers, battles

- **Step rule** (Gated §6.3) against S on the development range: B1 to B9 (with every repair carried over) and T2 to
  T9. A threshold S meets must be met; a figure S fails must not get worse beyond noise. B1 is judged in every battle
  but 28 (D-2: B-192 stays open), where it is reported.
- **Known answers of the new families** (Gated §5.2). Their hard parts (H-1, H-2, H-3, H-7) admit no exception.
  An answer S meets in at least 99.5 % of scenes must be met by G at 99.5 %; one S fails is reported. Answers that
  depend on I′ (the wedge's "routes round", the gate's stand line) are reported, not judged.
- **The matrix.** G's 24-seed matrix is played through a scratch catalog in WP-A and judged on §4.1 to §4.3 and the
  step rule. In WP-B, battles move by design, as the Gated plan §6.5 registered:
  1. **Survivor ranges** are restated from G's 24 seeds, each listed with its old and new range in the evidence.
  2. **A winner that flips** needs a first-difference trace of one flipped seed. A trace showing a broken behaviour
     (standing with room, shoving, oscillation, a wedge entered, a hard check) is a **bug and a stop**. A clean trace:
     - restates a Class R battle (the frozen list, Gated evidence §3.3: 8, 9b, 10, 11a, 14, 21, 22a, 26, 27, 28, 31,
       32, 33, 35, 36a, 36b, 37a, 37b, 38a, 38b, 39a, 39b, 42) under AGENTS.md §8: claim rewritten, fixture comment
       citing §8, Q0103, this plan and the mechanism;
     - sends a Class D battle whose claim is a StarCraft II statement to the owner (draft QC-3, §11);
     - restates a Class D battle whose claim is not one.
  3. **Held-out:** every restated claim holds on at least two thirds of the held-out matrix seeds. Otherwise stop and
     rewrite it from both ranges in an amendment.
- **Battle 26** (zerglings overwhelm six colossi; S 20 of 24): the Gated plan's registered rule stands. If it fails a
  seed of record **and** every check of §4.1 to §4.3 passes for it with a clean trace, it takes
  `known_bug: Some("B-209")` (zerglings walk under colossi in 5.0.14); the claim is not rewritten.
- **Battle 28:** D-2.

---

## 5. Amendment candidates (registered now; at most one is used in the whole plan)

| ID | Trigger | Change | Source |
|---|---|---|---|
| **A-1** | T1 fails (§4.2) or C-0 fails (§4.4) only in rows or families where R fails the same figure against S and G's `solver_extended_ticks` is positive there | Gk: `gated.crowd_iterations` equal to `iterations` (21), C-3′ off under `Gated`. A data value with two existing settings; no new constant. Gk must then pass **all** of §4 from the start, F-1′ and F-2′ included | Q0118's evidence: C-3′ under `Solver` failed its own checks (friend-crowd §5); the Gated plan made it part of C′ only to absorb what the gate leaves |
| **A-2** | F-1′ or F-2′ fails, with traces showing chains of friends pressed behind bodies E-2 holds | C′-3's held set extended to bodies whose step E-2 bounded to zero in the previous application | Gated plan S-7 (enemy-contact Amendment 2 F-1; shock propagation restricted to held bodies) |

An amendment re-runs WP-A in full on development, then the acceptance range. A-2 is a code change: it gets its own
red-first tests (a pressed chain behind an E-2-held body, order independence of the extended set), `cargo gate quick`,
and a verifier's `package` gate. No other amendment is allowed: no tie-break, hysteresis, tolerance or constant chosen
after a trace (Gated Amendment 14's bar), no rule from `gated-wp2`.

---

## 6. Work packages

Every package follows the charter's routine: tests red first where a test is new; `cargo gate quick` and the tests the
change needs, then hand back; a verifier runs `cargo gate package` on the final tree (Q0061). One building agent at a
time. **No other package edits `movement/`, `steering.rs`, `target_acquisition.rs`' admission test or
`movement_rules.ron` while WP-A or WP-B is open**; a bug fix that must touch them lands on `main` first and this plan
re-bases (recorded as an amendment, base re-measured).

### WP-A: measure and judge `Gated` on `M₀` (M; measurement and records; no production code unless A-2)

- **Precondition:** wall-clearance WP1 is merged to `main`.
- **Identity first (H-6).** S and L of the WP-A tree against `M₀` (they are the same commit unless A-2 adds code):
  harness seeds 1 to 200 every family, the matrix, both clashes.
- **T1 (§4.2) first.** Battles 26 to 29 on the 24 seeds and both clashes, then every harness family on development.
  If T1 fails: the stop of §7 rule 3, before any other figure is read.
- **Then:** §4.1 everywhere, H-3 by permutation, §4.3, the step rule and known answers (§4.5), the 24-seed matrix,
  battles 31 and 33, the showcases, and the callgrind session (§4.4).
- **Acceptance range** (300,001 to 301,000) once, arms S and G, on every judged harness figure.
- **Evidence:** a new file `docs/evidence/2026-12-02-contact-layer-acceptance.md`: method, tree, the base check, every
  table with S, R and G (L for identity and cost), red runs if A-2, the verdict. The isolation table: H-1 holding,
  H-1b out of band and nudged counts, H-2, the largest enemy overlap, and friend overlap, per arm, in hold-line,
  enemy-press, battles 26 to 29 and both clashes.
- **Tests (no new production test unless A-2):** the existing guards stay green: `contact_solver_tests.rs` (C1-T1 to
  C1-T16, the A3b tests), `push_priority_tests.rs`, `collision_and_immobility_tests.rs`,
  `fixed_body_transactions_tests.rs`, `harness_tests::enemy_press_scenes_of_record_keep_the_hard_checks_under_gated`.
- **Acceptance:** every item of §4 on development and on the acceptance range. The result is recorded in the evidence
  and this plan's row; `main` is unchanged unless A-2 adds latent code (H-6 then judged against `M₀`).

### WP-B: the switch-on (M; data, tests, goldens, records; `full` gate before push)

Only after WP-A's acceptance holds.

- **Held-out run, once,** before the switch commit, from WP-A's binaries: harness 9,100,001 to 9,101,000 and the
  matrix's 45th to 68th non-record seeds, arms S and G. Every judged figure keeps its development direction; restated
  claims hold on two thirds (§4.5 item 3).
- **The data.** `contact.pipeline: Gated` in `assets/catalog/movement_rules.ron`, with the file's header comment
  stating what `Gated` ships (C′ only; I′ and G′ not built on `main`) and citing this plan. `nudge_engaged_along_ring`
  stays (it is read by `Solver` only).
- **Tests re-pointed** (`contact_solver_tests.rs`):
  - `the_shipped_rules_run_the_solver_pipeline_since_the_switch_over` becomes
    `the_shipped_rules_run_the_gated_contact_layer`, asserting `Gated`, D-β on, `gated.crowd_iterations` 151;
  - `the_gated_section_ships_with_the_derived_cap_and_stays_latent` loses its latency assertion;
  - every test that asserts `Solver`'s behaviour through the shipped catalog (rather than through `rules_on`,
    `RunOptions::with_pipeline` or `world(…, MovementPipeline::Solver)`) is pinned to `Solver` through a test catalog
    or re-pointed (**verify** the list with `cargo test --workspace` on the switched tree; it is mechanical).
- **Golden re-record with the four-build proof (H-4).**
  1. Before the switch: the tree reproduces `main`'s golden values (`golden_replay.ron`, `golden_replay_orders.ron`,
     `DEFAULT_SANDBOX_CHECKSUM`, and every pinned digest) on native debug, native release, `wasm32-wasip1` release and
     debug (`wasmtime --dir=<real path>`, never a symlinked path).
  2. After the switch, re-record **once**, bytes only, with proof headers naming this plan, WP-B and the first changed
     checkpoint; the four builds agree on the new values.
  3. Schema unchanged (no hashed field; §Status). The commit message states that the checksum values move and the
     format does not.
- **Matrix restatements** (§4.5): survivor ranges, flipped winners with traces, Class R claims restated, battle 26's
  rule, battle 28 (D-2), in the matrix fixture and the evidence.
- **Showcase storyboard** re-checked with a real render on the RX 580 (radv), `with-gpu-lock` per take, with
  `WGPU_BACKEND=vulkan` and the radv `VK_ICD_FILENAMES`. `assets/captures/showcase.ron` is re-timed if the default
  seed's timeline moved; `showcase_storyboard_tests` and `showcase_tests` pass.
- **Throughput of record:** the §4.4 session on the switched tree (S now through a scratch catalog), and `DESIGN.md`
  §5 gains a paragraph with the `Gated` figures and their ratios to S and R.
- **Records:**
  - `DESIGN.md` §4 Subsystem Status, the Movement and pathfinding row: the shipped pipeline and its rules.
  - `DESIGN.md` §4: **B-193** closed (H-2 = 0); **B-220** closed (C′-3); **B-225** closed (H-1b = 0 under the owner's
    rule); **B-219** closed if F-2 strict holds on development, acceptance and held-out, otherwise restated with G's
    figures and kept open for WP-C; B-192 per D-2; B-190 and B-191 restated on T4 and T7.
  - Q0052: the D-δ addendum restated: D-δ is the owner's band rule under `Gated`, for melee and ranged attackers.
  - Plan `2026-11-27-gated-movement`: an amendment pointing here (C′ shipped alone; I′, G′ and WP4 not done); its
    README row becomes `Partly done`.
  - Q0143's `## Resolution`, and its move to `closed/`, in the switch commit.
  - Plan `2026-11-29-wall-clearance`: a note that window X's condition ("after Gated WP4 or its end") is met by this
    switch, and B-241's parking reason ("until the redesign changes how friends push") is resolved.
- **Gate:** the verifier's `cargo gate package` on the branch; then the orchestrator's `cargo gate merge` and
  `cargo gate full` on the merged tree before push.

### WP-C: whether a WP2 follow-up is needed (S; records only)

On the shipped tree's figures (development, acceptance and held-out, from WP-A and WP-B), registered now:

1. **No follow-up** if all of these hold: F-2 strict on every seed set; B1 not above S beyond noise in every battle
   but 28; no family whose T1 passed only within S's interval while above 1.0 / 0.25. The evidence records "no WP2
   follow-up" and the redesign closes with `Gated` = C′.
2. **Otherwise a question to the owner** (draft QC-2, §11), with the failing figures, what I′ was meant to remove
   (RC-1: walkers pressing friends that will not give way) and what `gated-wp2` showed (its evidence §13: most
   reversals came from I-3 bending round standing neighbours, not from the gate). Default: no follow-up; the residual
   is recorded against B-219 or the relevant row. No design round starts without the answer (Gated §6.6 rule 6).
3. The decision of WP-C never re-opens what WP-B shipped.

---

## 7. Stop rules (in advance)

1. **Identity (H-6) fails:** a defect. Stop and find it. It does not count as an amendment.
2. **A hard check (§4.1) fails under G:** stop, for the owner (Gated Amendment 3b: no amendment after 3b for a hard
   check). Draft QC-1.
3. **T1 fails:** stop at once. If A-1's trigger holds, A-1 is the one amendment; otherwise stop for the owner, draft
   QC-1.
4. **F-1′ or F-2′ fails:** A-2 if its trigger holds and no amendment has been used; otherwise stop, QC-1.
5. **The step rule or a judged known answer fails:** stop, QC-1 (no amendment candidate is registered for it).
6. **C-0 fails** after the allowed cost steps: A-1 if its trigger holds and unused; otherwise stop, and QC-1 carries
   the per-phase split (intent, solver iterations, C-3′ extension, E-2).
7. **The acceptance range** fails a judged figure: stop, QC-1. It is not an amendment trigger (no fitting to it).
8. **The held-out run** contradicts a development direction, or a restated claim fails two thirds: stop, QC-1.
9. **A flipped battle's trace** shows a broken behaviour: a bug (row drafted in §11) and a stop.
10. **A second failure after an amendment** ends the plan: `Solver` stays shipped, `Gated` stays latent on `main`
    (H-6 holds), and QC-1 goes to the owner.
11. **Not allowed** in any package: a constant, per-unit, per-family or per-battle switch, or a tolerance chosen after
    seeing figures; any change to D-α, the dead band, K, C-3′'s P-1 derivation or the global layer; a change to
    `Solver` or `Legacy` behaviour; any code from `gated-wp2`; deleting `gated-wp2`.

---

## 8. Determinism

C′ is on `main` and already proved: Q32.32 only, every read from the tick-start snapshot, exact order-independent
sums, H-3's 14,000 identical permutation scenes (Gated evidence §11.4). WP-B's switch changes data only. The four-build
proof of §6 WP-B is the check of record. If A-2 adds code, its fields are unhashed solver scratch (as C′-3's flag), and
its order-independence is proved by the permutation mode and a unit test over every body order.

---

## 9. Cost of this plan's own measurement

The development run is the Gated WP0b command set for two arms (S, G) plus R where reported, the acceptance range for
two arms, the held-out set for two arms, and one callgrind session of ten rows for three arms (about 30 runs; a
400-unit row is roughly 20 minutes under callgrind). Every run takes the gate lock for itself, one at a time; heavy
runs start only within the memory headroom of `docs/guides/gpu-and-disk.md`.

---

## 10. Risks and scope

### 10.1 Risks

- **C-3′ in T1 and cost** (§4.2): R fails deep-queue M1-hi; A-1 is registered for exactly this.
- **RC-3 without the gate:** a pressing crowd behind an attacker or an enemy defines constraints C′ cannot all meet
  (Gated design §5.3 item 2, K-8). WP1's development figures show the overlap fell rather than rose; the step rule's B1
  and B6 judge it now.
- **Battle 26 may move toward the colossi** (D-δ no longer slides engaged allies past each other); its rule is
  pre-registered (§4.5).
- **Wall-clearance's merge** may move the harness further; the base is `M₀`, re-measured if needed.
- **Load:** the host often runs at 50 to 140; C-0 is judged by instructions for that reason.

### 10.2 Out of scope (Q0002)

- I′ (the gate), G′-2 (orbit approach), G′-3 (D-ε, B-192), and Q0122's D-β measurement: WP-C decides only whether to
  ask the owner about a follow-up.
- Deleting `Solver` or `Legacy` (Gated WP5): a separate decision.
- B-209 (colossus walk-under) re-measure: due after the switch under the Gated plan §10; its own package.
- B-236 spawn placement (and Q0145 on `missile-throw-plan`): excluded from H-1b and H-2 and reported.
- Wall-clearance WP2 (B-241) and window X: resume under their own plan after WP-B.

---

## 11. Drafts (filed only when the step that needs them runs, at the next free number)

**No question is filed by this plan's commit.** The plan follows the owner's answer to Q0143 and needs no decision
before WP-A.

```markdown
# QC-1 (draft): The crowd-contact rules failed their acceptance on their own. Keep today's movement?

- **Blocking:** no
- **Default until answered:** (a). `Solver` stays shipped; the contact layer stays switched off on `main`.

## Background
<which check failed, its figure against today's, whether the one amendment (A-1 or A-2) was used, the trace>

## Options
- (a) Keep today's movement (default). B-193, B-219, B-220 and B-225 stay open.
- (b) Ship the contact layer despite the failed check <named>.
- (c) Something else you would like tried.
```

```markdown
# QC-2 (draft): The crowd-contact rules ship. Is a follow-up on crowd waiting (WP2) wanted?

- **Blocking:** no
- **Default until answered:** (a) no follow-up; the residual is recorded on B-219.

## Background
<the WP-C figures that failed rule 1, against today's; what the gated intent was meant to fix; what `gated-wp2` showed>

## Options
- (a) No follow-up (default).
- (b) A new design round for crowd waiting, starting from `gated-wp2`'s evidence §13.
```

```markdown
# QC-3 (draft): The crowd-contact rules change who wins a battle whose claim is a StarCraft II fact

As the Gated plan's Q0127 draft: the battle, its old and new winners on the 24 seeds, the first-difference trace,
options (a) restate the claim as a deviation, (b) record a bug and stop.
```

Bug rows are drafted only if a flipped battle's trace shows a broken behaviour (§7 rule 9), at the next free B-number.
