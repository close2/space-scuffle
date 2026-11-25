# Blueprint: Dense Friendly Crowds Overlap Because the Contact Solver Stops Before It Converges (B-219)

This plan diagnoses why dense crowds of friendly melee units overlap each other far beyond deviation D-α under the
contact solver, the default movement pipeline (plan [`2026-11-20-contact-solver`](2026-11-20-contact-solver.md)), and
fixes it. Two otherwise sound StarCraft II fixes stopped on this overlap:

- **Enemies block (B-193),** plan [`2026-11-25-enemy-contact-and-ring-retarget`](2026-11-25-enemy-contact-and-ring-retarget.md)
  on branch `enemy-contact-plan` (head `7f686d23`), Amendment 2 and its evidence §7. Its friend rule F-1 relieved a
  friend in direct contact with a held unit; chains of two or more friends behind it still compressed. B6-max failed
  in battle 26 (37.06 % → 40.44 %), and battle 27's largest friend overlap had reached 59.9 % before F-1. The owner's
  question [Q0113](../questions/open/0113-enemies-block-tradeoff.md) is open, default (b), parked.
- **Zerglings walk under the colossus (B-209),** plan `2026-11-25-collision-classes` and its evidence §3 and §4, on
  branch `collision-classes-wp3-wip` (head `42761cf2`). Zergling pairs overlapping by more than 5 % rose from 54.8 to
  124.1 per 1,000 zergling-ticks in battle 26 (B6 failed), with B4 and B7′. Owner question Q0117 is about to be filed.

**The finding, in one paragraph (§2).** The contact solver runs a fixed number of averaged (Jacobi) iterations per
tick, K = 21. In a crowd pressing on a front that cannot give way, the correction has to travel back through the crowd
one contact per iteration, and averaging halves it at every body it passes, so the crowd needs a number of iterations
that grows with the **square** of its depth. On `main` the solver hits its cap with friend pairs still above D-α on
most crowd ticks: in battle 26, seed 15, it ran all 21 iterations on 265 of 480 ticks. Raising the cap alone, with
nothing else changed, removes the overlap: friend pair-ticks above 5 % per seed fall from 812 to 134 (42 iterations),
0.8 (84) and 0.0 (250) in battle 26, and the same in 27 and 28. D-δ, the dead band, the order of the summed passes
and steering are not the cause (each measured, §2.4). The headline "37 % on `main`" is mostly the matrix's **spawn
jitter**: the meter counts the starting state, before any tick (§2.2).

**The fix (§4).** A **solver-side** change only (C-3′): past K, the solver keeps iterating while a friend pair is
above D-α's moving bound **and** the last iteration reduced the friends' summed excess over it, up to a cap derived by
a crowd test. It changes no share, no deviation, no steering rule, and no tick that already ends within D-α, so every
friend-only scene and battle tick that is inside D-α today is bit-identical. In the prototype battle 26's pair-ticks
above 5 % fall from 812 to 11 per seed and the largest overlap from 39 % to 7.6 %.

**Its cost is the open point.** The 400-unit throughput clashes are themselves crowds that break D-α on `main` (47.5 %
at worst), so converging them costs about +50 % to +60 % instructions on 64 × 64 and canyon (prototype), projected
at about 70 and 53 ticks per second: above Q0093's 50, but far above the +70 %-over-Legacy reporting line. The
cheap alternative, shock propagation (FC-4: −10 % on 64 × 64), removes 89 % of the overlap but changes how crowds
push, and battle 26's zerglings go from winning 9 of 12 runs to 2. Both are measured; the choice between paying and
changing the model belongs to the owner, so **Q0118 (draft, §11) should be filed before WP2**. WP0 and WP1 (latent)
proceed meanwhile.

**Status of this document.** A stage 1 blueprint under charter §6, written on 2026-11-25/26 against `main` at
`7189c6d9` (checksum schema 49).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (<role>, YYYY-MM-DD): <decision>`, numbered from 1.
- Numbers are provisional, next free at merge: bug rows from **B-219**, the question draft **Q0118**; the checksum
  schema does not change (no hashed field, §6), and if one is needed after all, it takes the next free number (50
  provisional).
- The planning measurements were made with scratch prototypes outside the repository (§2.1); none is code to merge.

**Why a full plan (charter §6).** It moves battle outcomes and golden replays (every dense crowd tick changes),
re-derives a binding constant of the solver plan (C-3), and unblocks two parked packages.

**Questions and standing rules.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §3 FC-9, §4 |
| Q0052 addendum: D-α (friends ≤ 5 % moving, ≤ 1 % at rest), D-β, D-γ, D-δ sanctioned | Standing; D-α is the bound this plan enforces; D-β and D-δ unchanged | §4.1 |
| Q0092 (a): the solver, and StarCraft II's own soft separation (Anhalt, GDC 2011, the owner's statement) | Standing | §3 |
| Q0093 (c), Q0102 (a): every throughput scenario at least 50 ticks/s; the solver's +66.8 % over Legacy accepted; "do not compromise overlap tolerances over the relative instruction ratio" | Standing | §5.4 |
| Q0103's guidance: natural movement outweighs close-matchup winners; a sound change that moves a close battle is restated with evidence; broken behaviour is a bug | Standing | §5.3 |
| The owner's guidance of 2026-11-20 (pathfinding): no fitting to the test set; simpler deviations allowed; survey first | Followed: §3 before §4; acceptance on properties, held-out seeds confirm only | §3, §5 |
| Q0002: scoping is the agent's call if written down | Standing | §10 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing | §7 |
| `docs/questions/open/` on `7189c6d9`: Q0113 (enemies block), Q0115 (rally keys), Q0116 (turret tracking) | Q0113 is unblocked by this plan's WP3 (§8); the others unrelated | §8 |
| Q0117 (B-209's battle 26, about to be filed by the orchestrator) | Unblocked by WP3 (§8) | §8 |
| **Q0118** (provisional, §11): pay C-3′'s cost, keep today's overlap, or change how crowds push (shock propagation) | Draft, not filed; recommended for filing before WP2 | §4.3, §5.5, §11 |

---

## 1. Summary

| | `main` (`7189c6d9`) | After this plan |
|---|---|---|
| Solver iterations per tick | at most K = 21 (`contact.iterations`), early stop when nothing moved | the same 21; then, **only while a friend pair is above D-α's moving bound**, more iterations of the same kind, up to `contact.crowd_iterations` (C-3′) |
| A tick that ends within D-α today | — | bit-identical (the extension never starts) |
| Battle 26, friend pair-ticks above 5 % per seed (12 seeds) | 812 | prototype 11.0 |
| Battle 26, largest friend overlap after the spawn tick | 39.1 % | prototype 7.6 % |
| The matrix's B6 maximum | counts the spawn state (jitter ±0.12 at spacing 0.7: up to 36.7 %) | counted from the first tick on; the old column kept and labelled (§2.2, WP0) |
| Shares, D-β, D-δ, dead band, steering, ring rule, enemy rules | — | unchanged |
| Checksum schema | 49 | unchanged (no hashed field) |
| Cost | solver +66.8 % over Legacy on 64 × 64 (Q0102) | paid only on ticks above D-α; prototype +62.5 % on 64 × 64, +51.7 % on canyon, +0.1 % on 128 × 128 over `main`; ≥ 50 ticks/s projected; owner question Q0118 (§5.5, §11) |

---

## 2. Diagnosis on `main`

### 2.1 Method

- **Tree.** `main` `7189c6d9`, built in `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/friend-crowd-plan`.
  Scratch prototypes (never committed) are `git archive` copies under `~/scratch/friend-crowd/trees/`
  (`proto`, `proto2`, `proto3`, `proto4`), each in its own target directory. Each prototype's rule is behind an environment
  variable, and with none set each prototype's battle outputs equal `main`'s byte for byte (arm "off", §2.4).
- **Battles.** `matchup_trace --battle <name> --seeds 8`: the four seeds of record and the tool's first eight
  others (12 seeds), for 26 (100 zerglings against 6 colossi), 27 (against 8 ultralisks) and 28 (against 10
  immortals). Data counterfactuals use scratch catalogs (`--catalog-dir`).
- **Measure.** `~/scratch/friend-crowd/scripts/arm_summary.py` reads every tick of `seed_<n>_units.csv` (positions
  to 1e-4, living blue bodies, radius 0.375): friend pair-ticks above 5 % of the radius sum **after the spawn tick**,
  the largest such overlap, the 99th percentile of the per-tick maximum, and the spawn tick's maximum separately.
  `ovl_classes.py` classifies each pair-tick above 5 % by **layer** (graph distance, through friends in contact,
  from a friend touching an enemy body) and by the trace's activities. The figures agree with the matrix's B6
  definition except for the spawn tick (§2.2). They are planning figures, not the baseline of record (WP0 measures
  that with the repository's meter).
- **Probes** (scratch prints, `FC_PROBE`, `FC_PASSES`): iterations used per tick, and the largest friend overlap
  before and after each of the solver's summed passes.
- **Cost** (§2.6): `perf stat -e instructions:u` of single-scenario throughput binaries and of the whole bench, one
  run per arm, on a host at load average 20 to 220 (ticks per second unreliable, instructions not).

### 2.2 The "37 %" on `main` is the spawn state

`ContactMeter::observe` "records the starting state" on its first call, and `count_overlaps` runs on that call too, so
B6's maximum includes the bodies as the matrix spawned them. Battle squads are 10 columns at spacing 0.7 for
zerglings of diameter 0.75 (6.7 % overlap by layout) with a spawn jitter of ±0.12 per axis on every seed but the
first; two neighbours can start 0.46 apart, a 38.7 % overlap. Measured: the spawn tick's largest friend overlap is
6.67 % on the unjittered seed and 33 % to 36.7 % on jittered ones; it is the same in battles 26, 27, 28 and 29 because
their squads are laid out alike, which is why the enemy-contact evidence §7.3 shows 37.02 % in four battles.

So "37 % → 40 %" (enemy contact) compared a spawn artefact with a dynamic maximum. The dynamic figures are still far
above D-α (§2.3). **Repair (WP0, measurement only):** B6's maximum and pair-ticks are counted from the first stepped
tick; the old maximum stays as a reported column, labelled as including the spawn state. This is decided **before**
any candidate figure is read under §5, and it applies to the base and every arm alike.

### 2.3 What `main` does in a dense crowd

| Battle (12 seeds) | Zergling / other wins | Friend pair-ticks > 5 % per seed | Largest after spawn | 99th pct of per-tick max |
|---|---|---|---|---|
| 26 | 9 / 3 | 812 | 39.1 % | 21.7 % |
| 27 | 0 / 12 | 487 | 28.9 % | 17.0 % |
| 28 | 4 / 8 | 268 | 28.4 % | 9.9 % |

**Where the pairs are** (battle 26, five seeds, `ovl_classes.py`): spread over every layer of the crowd, from the
front rank (layer 0–1) to five or more contacts behind it, with the largest overlaps at layers 1 to 4 (seed 15:
43.6 % between two engaged zerglings at layers 1–2; seed 22: 26.2 % at layers 2–3). 6 % to 19 % of the pair-ticks
are in groups with no enemy contact at all (marching blocks). By activity, the commonest pairs are
approaching–engaged, walking–walking and approaching–approaching. **Chains, not a single direct contact, carry most
of the excess.**

**Iterations used** (`FC_PROBE`, battle 26 seed 15, 480 ticks): 137 ticks stop after 1 iteration (nothing to do),
78 stop between 3 and 20, and **265 run all 21**. Of the ticks that end with a friend pair above 5 % (101 ticks), every
one ran all 21 iterations.

### 2.4 Each candidate cause, measured

Arms on battles 26 / 27 / 28, 12 seeds; friend pair-ticks above 5 % per seed after spawn; zergling wins in brackets.

| Arm | What changes | 26 | 27 | 28 | Reading |
|---|---|---|---|---|---|
| `main` | — | 812 (9) | 487 (0) | 268 (4) | base |
| off | each prototype with its switch unset | 812 (9) | 487 (0) | 268 (4) | byte-identical outputs: the prototypes are faithful |
| **Iterations 42** | `contact.iterations` only | 134 (5) | 45 (0) | 17.5 (3) | **dose–response** |
| **Iterations 63** | same | 18.7 (11) | 5.4 (0) | 3.9 (4) | |
| **Iterations 84** | same | 0.8 (6) | 0.2 (0) | 0.1 (5) | largest after spawn 6.3 / 5.2 / 5.1 % |
| **Iterations 126** | same | 0.0 (8) | 0.0 (0) | 0.0 (4) | largest 2.3 / 2.3 / 2.2 % |
| **Iterations 250** | same (the `u8` limit) | 0.0 (10) | 0.0 (0) | 0.0 (3) | largest 1.0 % everywhere: the dead band |
| D-δ off | `nudge_engaged_along_ring: false` | 1281 (0) | 1042 (0) | 485 (0) | **worse**: the nudge relieves the front; not the cause |
| Dead band 0.1 % | `dead_band_permille: 1` | 1358 (11) | 665 (0) | 582 (6) | **worse**: the band speeds the early stop; not the cause |
| D-β off | moving friends share by their own approach | 98 (2) | 94 (0) | 103 (4) | −88 %: the press of moving friends through the crowd is the injection; changes a sanctioned deviation |
| Pass order | probe: worst friend overlap before / after each summed pass | — | — | — | the two summed passes change it on 6 of 951 ticks, by ≤ 0.1 %; every tick above 5 % was above 5 % before them. **Not the cause** |
| Steering slide | intent drops the heading's part into a touching friend that holds (FC-8) | 1187 (6) | 372 (0) | 280 (2) | worse in 26, better in 27: steering injection is not the main load |
| Depth ordering | shock propagation (FC-4), front pairs as today (`ank`) | 89 (2) | 75 (0) | 86 (2) | −89 % at no iteration cost; moves the equilibrium (§4.3) |
| Depth ordering, every pair | `an` (`a`: nudge dropped) | 97 (0) | 79 (0) | 86 (1) | as above; dropping the nudge too (`a`) is no better (84 / 86 / 97) |
| Slide + depth | both | 67 (0) | 70 (0) | — | the steering half adds little |
| **C-3′** (§4) | iterate past 21 only while a friend pair is above 5 % and the excess falls | 11.0 (10) | 5.6 (0) | 4.5 (4) | the design; largest 7.6 / 7.5 / 7.6 % |

### 2.5 Why: the solver's iteration count is derived for one body, not a crowd

C-3 derives K from "one iteration removes at least the fraction 1/c of the deepest overlap of a body **whose other
contacts are resolved**". In a crowd pressed against a front that cannot give way (engaged or anchored bodies with
share 0, or the D-δ front taking only its tangential half), no body's other contacts are resolved:

1. **Injection every tick.** Every walker steps its full step (a zergling 0.165 world units, 22 % of the radius sum)
   into the crowd ahead; intent does not steer round bodies already in contact (I-3, by design), and D-β passes
   the press of moving friends on, half and half (`contact_shares`, equal priority).
2. **Jacobi with averaging is diffusion.** A correction travels one contact per iteration, and each body divides its
   summed corrections by its contact count (`ContactSolver::apply`, C-1). Along a queue of `L` bodies behind a fixed
   front, the error decays per iteration by a factor near `cos(π / (2L + 1))`, so the iterations needed grow like
   `L²` (FC-1, FC-3). The matrix's zergling squads are 10 rows deep. The 9 × 9 hexagonal test crowd with which
   the solver plan's Amendments 1 and 2 raised K from 14 to 21 is stepped by the worst injection but, as the
   catalog's header describes it, presses against no front that cannot give way (**verify** in WP1 by reading the
   test), so it does not exercise this.
3. **The residual is carried.** What a tick leaves is the next tick's starting overlap, and the next tick injects
   another step: the overlap settles where injection and 21 iterations of removal balance, 10 % to 40 % deep in the
   crowd.

The cause is therefore one register row:

- **B-219 (provisional): the contact solver's iteration count is derived for one body, not for a crowd, so dense
  friendly crowds pressing a front exceed D-α's moving bound** (battle 26 on `main`: 812 friend pair-ticks above 5 %
  per seed, largest 39 % after spawn; 265 of 480 ticks at the cap; the 400-unit `pulse_bot` benchmark clash: 47.5 %
  at worst, above 5 % on 60 % of its ticks, so ranged crowds are affected too). It is a defect of the solver plan's
  C-3 bound, not a new deviation: D-α is sanctioned as a bound, and the solver does not meet it.

Why the two parked packages hit it:
- **Enemies block** makes more friends unable to give way (held at an enemy's surface), which lengthens and stiffens
  the chains; F-1 relieved only the first link (its stated limit, A2.1).
- **Collision classes** lets zerglings fill the space under and between colossi, so the crowd round each colossus is
  denser and deeper.

### 2.6 Cost of convergence, measured

Instructions retired (`perf stat -e instructions:u`, one run each) by single-scenario throughput binaries (400
`pulse_bot` bodies, 300 ticks), built from the prototype tree `proto4`, whose switch-free build includes the
prototypes' environment reads (about +1.8 % on 64 × 64 against the enemy-contact base's 16.305 G). Ticks per second
were measured on a host at load average 20 to 220 and are not figures of record.

| Arm | 64 × 64 | 128 × 128 | canyon |
|---|---:|---:|---:|
| `proto4`, no switch (≈ `main`) | 16.597 G | 11.136 G | 27.566 G |
| Iterations 42 (data only) | +14.4 % | +0.0 % | +18.2 % |
| Shock propagation, front pairs as today (`ank`) | **−10.2 %** | +0.3 % | +4.0 % |
| C-3′ without the progress test, cap 250 | +57.0 % | +0.1 % | +43.6 % |
| **C-3′ with the progress test, cap 250** (prototype, with a separate excess scan) | **+62.5 %** | **+0.1 %** | **+51.7 %** |

Whole-bench totals (all seven scenarios, three repeats; `proto` and `proto3` trees): iterations 42 / 63 / 84 / 126:
+10.6 / +20.6 / +27.0 / +36.0 %; D-β off −6.2 %; steering slide +1.2 %; shock propagation (`an`) +1.0 %; C-3′ cap
84 / 250 without the progress test +26.5 / +30.5 %.

**The throughput scenarios are themselves the broken case.** The 400-unit `pulse_bot` clash (a ranged, marine-like
unit, so no D-δ) breaks D-α on `main` on 60 % of its ticks: the largest friend overlap at the end of the solve is 47.5 %
and its mean over ticks 18 % (`FC_PROBE` on 64 × 64; canyon: 36.7 % and 18 %). Under C-3′ the solve uses 55 iterations
per tick on average (15.6 on `main`; at most 173) and the per-tick maximum falls to 10.9 % (mean 4.3 %). Convergence
there costs what it costs: the benchmark measures exactly the crowds the fix is for. The progress test's own share is
the prototype's separate scan with a root per pair (about 5 points of the 62.5 %); WP1 records the excess inside
`accumulate` instead (§4.1).

**Projected ticks per second** (quiet host, from the solver plan's 111 / 183 / 80 and the instruction ratios less the
prototype scan): 64 × 64 about 70, 128 × 128 about 183, canyon about 53 to 55. All above Q0093's 50, canyon by a
small margin.

## 3. Survey: how dense friendly crowds are kept from interpenetrating

The owner's guidance (2026-11-20) asks for a survey before a movement rule. The movement survey
([`2026-11-20-movement-survey`](../evidence/2026-11-20-movement-survey.md), sources SC-, ST-, VO-, FF-, RT-) and the
enemy-contact plan's §3 (S-1 to S-8) are not repeated; this section adds what bears on **convergence in a crowd**,
the cause §2 measures. New sources are numbered FC-1 onwards.

| # | Approach | Source | What it does for a pressed crowd | Fit for us |
|---|---|---|---|---|
| FC-1 | **More iterations** (and sub-steps) | Bender, Müller, Macklin, "A Survey on Position Based Dynamics", Eurographics tutorial 2017; Macklin, Storey, Lu, Terdiman, Chentanez, Jeschke, Müller, "Small Steps in Physics Simulation", SCA 2019 (sub-steps converge better than iterations for the same cost) | Jacobi PBD converges geometrically, but the rate of a chain of `L` bodies falls like `1 − c/L²`; a stack or a queue needs iterations proportional to `L²` to settle | Data only (`contact.iterations`). Measured in §2.4: 84 iterations remove the problem, 250 remove it entirely; cost is §2.6's question |
| FC-2 | **Warm start** | Catto, "Iterative Dynamics with Temporal Coherence", GDC 2005 (Box2D's accumulated impulses carried across steps) | Starts each step from last step's impulses, so a resting stack does not re-solve from zero | **Already implicit** in a position solver: last tick's residual overlap is this tick's starting state. It does not help when every tick injects a fresh step (§2.3): warm start keeps a resting solution, it does not shorten the propagation of a new one |
| FC-3 | **Constraint averaging (Jacobi) and its cost** | Macklin, Müller, Chentanez, Kim, "Unified Particle Physics for Real-Time Applications", *ACM TOG* 33(4), 2014 ([pdf](https://matthias-research.github.io/pages/publications/flex.pdf)); Tonge, Benevolenski, Voroshilov, "Mass Splitting for Jitter-Free Parallel Rigid Body Simulation", *ACM TOG* 31(4), 2012 ([record](https://history.siggraph.org/?p=110584)) | Averaging makes Jacobi stable and order-free but divides every correction by the contact count, which slows propagation in dense contact; Macklin et al. add an over-relaxation factor ω (1 to 2); Tonge et al. divide the mass, not the correction, by the contact count | Our C-1 is exactly constraint averaging with ω = 1. Over-relaxation is a scalar per tick, order-free; it speeds up a chain by a constant factor only (not `L²` → `L`) and overshoots where contacts are few, so it is rejected as the fix (§4.3) |
| FC-4 | **Shock propagation / mass ordering** | Guendelman, Bridson, Fedkiw, "Nonconvex Rigid Bodies with Stacking", *ACM TOG* 22(3), 2003 ([pdf](https://www.cs.ubc.ca/~rbridson/docs/rigid_bodies.pdf)): bodies are ordered in levels from the ground up, and a level that has been solved is treated as having infinite mass for the levels above; Macklin et al. 2014 §5.1 (above): "mass scaling" by stack height, fixed per step; also the 2025 "Two-Pass Shock Propagation for Stable Stacking with Gauss-Seidel" ([crossref](https://api.crossref.org/works/10.1145%2F3747867)) | A body that rests on a support cannot be pushed into it by bodies further up: the pressure is carried by the upper body alone. A stack of `L` levels settles in `L` iterations instead of `L²` | **Prototyped, the fallback (§2.4, §4.3).** Our "ground" is the bodies that cannot give way: the engaged front, holding, fixed bodies. Levels are the graph distance in the friendly contact graph, computed once per tick from positions, so they do not depend on processing order; the ordering is frozen for the tick (S-2's lesson: no ratio that flips per iteration). Cheap and effective, but it changes the solution the solver converges to, not only how fast |
| FC-5 | **Graph-colouring Gauss–Seidel** | Fratarcangeli, Pellacini, "Scalable Partitioning for Parallel Position Based Dynamics", *CGF* 34(2), 2015; Fratarcangeli, Tibaldo, Pellacini, "Vivace: a Practical Gauss-Seidel Method for Stable Soft Body Dynamics", *ACM TOG* 35(6), 2016 ([record](https://research.chalmers.se/en/publication/507089)) | Constraints are coloured so that no two of one colour share a body; colours are solved in sequence, each colour in parallel. Gauss–Seidel convergence (a chain in `L` sweeps when the colour order runs along it) without order dependence inside a colour | Deterministic only if the colouring is a function of the body set (a greedy colouring by `EntityId` is: ids are stable). But the convergence along a chain depends on whether the colour order happens to run front-to-back; with 4 to 7 colours in a hexagonal crowd that order is arbitrary, so it gains a constant factor. It also makes the result depend on ids, which C-1a's relabelling property (T8) forbids. Rejected (§4.3); recorded for cost work |
| FC-6 | **Density-aware speed / unilateral incompressibility** | Narain, Golas, Curtis, Lin, "Aggregate Dynamics for Dense Crowd Simulation", *ACM TOG* 28(5), 2009 ([page](https://gamma.cs.unc.edu/DenseCrowds)); Treuille, Cooper, Popović, "Continuum Crowds", SIGGRAPH 2006 (FF-6); Pentheny's congestion maps (FF-7) | The crowd's velocity field is projected so that density never exceeds a maximum: agents in a full region slow down or stop *before* they press | The principle (a full region does not accept more inflow) is the steering-side half of §4. The grid machinery is out of proportion for us; the local form, "do not step into a friend that will not give way", is enough (FC-8) |
| FC-7 | **Long-range predictive constraints** | Weiss, Litteneker, Jiang, Terzopoulos, position-based crowds, *C&G* 78, 2019 (ST-4); Karamouzas, Skinner, Guy 2014 (VO-9) | Agents react to a predicted collision before it happens; contact constraints handle the rest with few iterations | Our intent's side rule (I-3) is this, for approaching bodies; it deliberately excludes bodies already in contact (§2.3), which is where the crowd presses |
| FC-8 | **Collide and slide against bodies that will not move** | PhysX character controller ("collide and slide"), [PhysX 5.4 docs](https://nvidia-omniverse.github.io/PhysX/physx/5.4.0/docs/CharacterControllers.html); Fauerby, "Improved Collision Detection and Response", 2003; our own legacy `remove_blocked_velocity` (plan `2026-11-09-friendly-blocking`) and enemy-contact's E-2 | A mover's step loses its component into a body that will not give way; the tangential part is kept (a slide) | **Considered for the steering half (§4.2), measured in §2.5.** It removes injection at the source instead of asking the solver to undo it |
| FC-9 | **Units queue behind a full surround** (StarCraft II) | SC-10 ("units outside the concave stand"); SC-8 (units are blocked by units holding, attacking, patrolling and other moving units); SC-2 ("lots of rules around who pushes who"); the owner's statement on Q0092 (soft separation with slight temporary overlap, Anhalt GDC 2011, SC-1) | Visibly: attackers that cannot reach stand in a loose second row; they do not stack inside the first row. No source gives the mechanism or an overlap bound | The target behaviour. Our ring rule already makes a unit with no free slot stand (G-6); §2 shows the overlap comes from units that still walk (approaching a gap through the crowd, or pushed by those behind), not from those that stand |
| FC-10 | **RTS practice: pushing with a priority, not avoidance** | 0 A.D. Alpha 25 (RT-1): moving units push moving units, idle push idle; jdxdev (ST-2): friendly units push each other, holding units are avoided; Factorio (RT-3): biters do not collide with biters, separation limits stacking | Dense friendly crowds are allowed to overlap softly; what stops stacking is a separation that keeps working every tick | Consistent with D-α: some transient overlap is the sanctioned model. What we lack is not a rule but convergence: our own bound (5 %) is not met (§2) |

**Conclusions.**
1. Iterating a position solver to convergence is the textbook answer (FC-1). Warm start (FC-2) is already what a
   position solver does by carrying its residual; it does not help a crowd that is pushed afresh every tick.
2. The textbook answer to "too slow in stacks and queues" is ordering by support, shock propagation (FC-4): it buys
   the order of growth (`L²` → `L`) and depends only on geometry. Order-free variants of Gauss–Seidel (FC-5) buy a
   constant factor and depend on ids. Both change *how* the solver converges; shock propagation also changes *what*
   it converges to (a support is never pushed by what rests on it).
3. Iterating only while the residual exceeds the tolerance, with a cap, is the standard practical compromise between
   the two costs (iterations until a tolerance, as every iterative solver in the PBD survey does, FC-1). It keeps the
   solution the sanctioned model defines and spends iterations only where the tolerance is broken. This plan takes it
   (§4.1) and keeps shock propagation as the measured fallback.
4. The steering half (FC-6, FC-8, FC-9) says locally that a unit should not step into a friend that will not give
   way. It is the larger change to behaviour and the one with history here (the legacy pipeline's blocked velocity),
   so it was measured (§2.4) and not adopted.

---

## 4. Design

### 4.1 C-3′: iterate until the crowd is within D-α, not a fixed count (solver side only)

**Rule.** `ContactSolver::solve` runs the averaged iterations of C-1 exactly as today for the first
`contact.iterations` (K = 21) iterations, early stop included. If the loop reaches K, it **continues with further
averaged iterations of the same kind while both hold:**
1. **some listed friend pair overlaps by more than D-α's moving bound** (`moving_overlap_permille`, 5 % of the radius
   sum), and
2. **the last iteration made progress:** the friend **excess** `E = Σ over friend pairs of max(0, overlap − bound)`
   (world units, exact Q32.32 sum) is strictly smaller than it was one iteration earlier;

up to `contact.crowd_iterations` iterations in all. The early stop ("no body moved") still ends the loop at any
point. The two summed passes (C-1b) follow unchanged.

- **Why the progress test.** Some contact sets have no solution within the bound: a body pinned by terrain with share
  1 against a partner with share 0 (the solver plan's R5, B-220 below), or a walker wedged between anchored bodies.
  There, more iterations only shuffle bodies: the prototype without the test turned harness scene corner 248 (two-cell
  passage, 23 units) from 17 % to 73 % friend overlap and 0 to 8 give-ups, and spent 84 iterations on 45 % of the
  400-unit benchmark's ticks; with the test the scene is as today (17.6 %, 0 give-ups) (§4.2). The test has no
  tolerance to tune: "strictly smaller" over exact integer sums.
- **Why the excess, not the maximum.** The maximum can stall while the crowd behind it still converges; the summed
  excess falls whenever any pair above the bound gets closer to it.

- **What it is.** More of the same projection. The fixed point the solver approaches (C-2's shares, the dead band,
  D-β, D-δ's tangential nudge, terrain) is unchanged; the solver only stops nearer to it. No share, deviation,
  steering, ring or enemy rule changes.
- **Where it changes nothing.** A tick whose solve ends before K, or reaches K with every friend pair at or below
  5 %, is bit-identical to `main`. That covers every tick of a scene that stays inside D-α today, every friend-only
  scene whose solve does not hit the cap with a pair above the bound (the corridor family hits the cap on 77 % of its
  ticks, all within the bound: unchanged), and every battle without a dense crowd.
- **Both tests are global** (an "any" and an exact sum over pairs), so they do not depend on the order of bodies or
  pairs (C-1a holds). They read the overlaps that the iteration's `accumulate` already computes for every pair
  outside the dead band (`pair_correction`): the implementer records, per accumulation, the friend excess of the
  positions it read, so the tests cost one comparison and one addition per pair already being corrected and no extra
  pass (the prototype scans the pairs separately with a root each; WP1 must not). The excess of iteration `k` is
  read by iteration `k`'s own accumulation, from the positions iteration `k − 1` produced, so "the last iteration made
  progress" compares two consecutive accumulations.
- **Why the moving bound, not the dead band.** D-α allows 5 % while moving; continuing to 1 % would spend iterations
  on overlaps the deviation sanctions. The early stop already continues to the dead band whenever a tick converges
  within K, as today.
- **Why a cap.** The cost of a tick must be bounded (Q0093). The cap is the guarantee, not the typical load: in battle
  26 the prototype's extended ticks used 46 iterations on average and 85 at most, with a cap of 250 (§4.2).

**Data** (`assets/catalog/movement_rules.ron`, `contact` section, documented in the file's header; `MovementRules`
validates it at load):

```ron
contact: (
    // ... unchanged fields ...
    // C-3′ (plan 2026-11-26-friend-crowd-overlap): after `iterations`, keep iterating while a friend pair overlaps
    // by more than `moving_overlap_permille` and the friends' summed excess over it still falls, up to this many
    // iterations in all. Derived by the crowd test P-1 for a
    // crowd `crowd_depth` bodies deep; equal to `iterations` switches the extension off.
    crowd_iterations: <derived in WP1>,
    // C-3′: the depth, in bodies, of the deepest crowd behind a front for which `crowd_iterations` is derived.
    crowd_depth: 12,
),
```

- Validation: `crowd_iterations ≥ iterations`, `crowd_iterations ≤ 1,000`, `crowd_depth ≥ 1`. Absent
  `crowd_iterations` reads as `iterations` (the extension off), so older scratch catalogs keep today's behaviour.
- `crowd_iterations` is a `u16`; `iterations` stays `u8` (C-3 unchanged). `SolverSettings` gains
  `crowd_iterations` and `moving_overlap_permille`; `SolveReport::iterations` widens to `u16`.

**The derivation of `crowd_depth` and `crowd_iterations` (C-3′a), not from battles.**
- `crowd_depth`: the deepest queue the reference army forms against one front. The throughput reference is 400
  units on one side; packed as a disc against a front, its depth in bodies is `⌈√(400 / π)⌉ = 12`. It is data, so a
  larger reference army raises it.
- `crowd_iterations`: the smallest value with which **property test P-1** holds: a hexagonal crowd `crowd_depth`
  rows deep and 12 bodies wide of the catalog's smallest-radius ground archetype, every body stepping each tick by
  the catalog's worst injection `ρ` (C-3's `jet_jumper` pair) toward a row of anchored bodies (share 0), keeps every
  friend pair within D-α's moving bound at the end of every tick from tick 25 on (one second to fill). The test
  recomputes the value from the shipped catalog and fails if the data is smaller, as C-3's test does for K.
- **Expected** (not a threshold): between about 85 (the most battle 26's extended ticks used) and about 175 (the
  most the 400-unit benchmark's ticks used with a cap of 250, §2.6). The value P-1 gives is what ships; neither the
  battles nor the benchmark choose it, and a tick that reaches the cap simply ends there.

**Where it sits.** `movement/solver.rs` (`ContactSolver::solve`, `accumulate` recording the flag, `SolverSettings`,
`SolveReport`), `movement/collision.rs` (`solve_contacts` passes the settings),
`catalog/movement_rules.rs` (fields, defaults, validation), `movement_rules.ron`. Legacy is untouched.

**Cached pairs.** The active set reuses a pair's correction while neither body moved (K-1). The excess needs that
pair's overlap too: the pair keeps the overlap its cached correction was computed from (one more `SimScalar` in
`Pair`), so a reused correction contributes its stored excess and nothing is recomputed.

### 4.2 Measured on the prototype (`proto3` / `proto4`, `FC_EXT`, `FC_STAG`)

Battles, 12 seeds (§2.1):

| Arm | 26: pair-ticks > 5 % / largest / zergling wins | 27 | 28 |
|---|---|---|---|
| `main` | 812 / 39.1 % / 9 of 12 | 487 / 28.9 % / 0 | 268 / 28.4 % / 4 |
| C-3′ without the progress test, cap 84 | 2.5 / 6.4 % / 11 | 1.8 / 7.4 % / 0 | 1.5 / 5.2 % / 6 |
| C-3′ without the progress test, cap 250 | 1.3 / 5.1 % / 11 | 1.5 / 6.0 % / 0 | 1.5 / 5.2 % / 7 |
| **C-3′ as designed (progress test, cap 250)** | **11.0 / 7.6 % / 10** | **5.6 / 7.5 % / 0** | **4.5 / 7.6 % / 4** |

In battle 26 (seed 15) without the progress test, 29 of 455 ticks were extended, using 46 iterations on average and
85 at most. The 99th percentile of the per-tick maximum is 5.0 % to 5.4 % in every arm: the extension stops at the
bound by design; what is left above it at the end of a tick is the summed passes' and the next proposal's share.

Harness, development seeds 1 to 300 per family (scenes whose `fo_move_max` exceeds 5 %; reversal amplitude
`rev_amp`; give-ups):

| Family | `main` | C-3′ without progress test (cap 84) | C-3′ as designed | Shock propagation (`an`) | Steering slide |
|---|---|---|---|---|---|
| gap | 91; rev 956; 7 | 66; rev 1,070; 7 | 70; rev 1,054; 5 | 91 (identical) | 91; rev **1,998**; 8 |
| arc | 8; rev 1,878; 87 | 1; rev 1,920; 87 | 3; rev 1,917; 87 | **18; rev 5,317**; 52 | 4; rev 2,759; 78 |
| cross | 2; rev 1,164; 0 | 1; rev 1,164; 0 | 1; rev 1,164; 0 | 2 (identical) | 0; rev 1,305; 0 |
| corridor | 0 (identical) | 0 (**identical**) | 0 (**identical**) | 0; differs | 0; differs |
| corner | 51; rev 517; 0 | 9; rev 753; **8** (scene 248: 17 % → 73 %) | 24; rev 520; 0 (scene 248: 17.6 %) | 51 (identical) | 48; rev 722; 0 |

The gap family's worst scenes (up to 96 %, mixed priorities in one-cell gaps) are R5's terrain-pinned 0/1 shares,
which no number of iterations resolves (B-220, out of scope, §10). The corner scene 248 is why the progress test is
part of the design: with it, the corner family keeps its reversals and give-ups (520 against 517; 0) while its scenes
above 5 % halve. The one figure that worsens under the design is the gap family's reversal amplitude (956 → 1,054,
+10 %); WP2 judges it under T1 against the base's noise (§5.3), and a failure there is stop rule 4.

### 4.3 Alternatives measured and not chosen

| Alternative | Measured (26: pair-ticks, zergling wins) | Why not |
|---|---|---|
| Raise `iterations` to 42, 84 or 126 | 134 / 0.8 / 0.0; 5 / 6 / 8 of 12 | Runs every capped tick to the new count whether it needs it or not, also in infeasible wedges (the corner 248 failure), and converges to the dead band, not D-α. C-3′ spends iterations only where the bound is broken and progress is made |
| **Shock propagation** (FC-4; depth from bodies that cannot give way, deeper body takes the whole correction; front pairs unchanged) | 89, 2 of 12 | −89 % at +1 % cost: the cheapest fix. But it changes the **equilibrium**, not only the convergence: a body behind a support can no longer share a correction with it, so crowds stop pressing their front aside, and battle 26's zerglings, which mostly win today and under every converged arm (10 to 11 of 12), lose 10 of 12. In the harness's arc family it more than doubles reversals (1,878 → 5,317) and scenes above 5 % (8 → 18). That is a change of model (closer to "units queue behind a full surround", FC-9) that the owner should choose, not a repair. **The alternative offered in Q0118** (§11) if the owner does not accept C-3′'s cost |
| D-β off for equal priorities (moving friends yield their own approach) | 98, 2 of 12 | Changes a sanctioned deviation (D-β, Q0092) and the harness's crossing behaviour; the solver plan's prototype found D-β off worse on every harness family (§5.3 there) |
| Steering slide against friends that hold (FC-8) | 1187, 6 of 12 | Worse in 26; doubles reversals at gaps (956 → 1,998): the load is the solver's propagation, not the proposal's injection into standing bodies; it would also bring back a form of the legacy pipeline's blocked velocity that the solver plan removed |
| Slide + shock propagation | 67, 0 of 12 | Little beyond shock propagation alone |
| Over-relaxation (FC-3), graph-colouring Gauss–Seidel (FC-5) | not prototyped | A constant factor only; colouring depends on ids (T8) |
| Smaller dead band | 1358 | Worse (§2.4) |

**Steering or solver?** Solver only. The measurements put the excess in the solver's convergence; the steering
candidate made battle 26 worse. "Queue behind a full surround" (FC-9) is already the ring rule's G-6 for units with
no free slot; the overlap is among units that still walk and among those the crowd pushes.

---

## 5. Acceptance (pre-registered, before WP1's first build)

### 5.1 Base, seeds and readings

- **Base:** WP0's tree (`main` plus WP0's measurement repair only). Every arm in its own target directory, one run at a
  time under `flock ~/scratch/gate.lock`.
- **Matrix:** the 24 seeds (four of record and the tool's first 20 others) for judging; the held-out seeds (the
  tool's 21st to 44th non-record seeds) for confirmation only. Battles 31 (100 seeds) and 33 (64 seeds) and the
  showcases as in the collision-classes evidence §1.1.
- **Harness:** families gap, arc, cross, corridor, corner, mirror; development seeds 1 to 1,000 for judging, held-out
  9,000,001 to 9,001,000 for confirmation.
- **"Beyond noise":** the candidate's per-seed mean outside the base's 95 % bootstrap interval of the per-seed mean
  (2,000 resamples, seed 7), on the worse side, as the enemy-contact and collision-classes plans.
- **B6 is read from the first stepped tick** (§2.2's repair, WP0), for base and candidate alike. The spawn-inclusive
  maximum is reported only.

### 5.2 The friend-overlap targets (stated before measuring)

| Check | Target |
|---|---|
| **F-1, pair-ticks** | Counted over pairs the solver corrects, from the first stepped tick (WP0's columns). In every matrix battle whose base has at least 10 such friend pair-ticks above 5 % per seed ("F-1's set"), the candidate's per-seed mean is at most **10 %** of the base's (a fall of 90 % or more; prototype: 98 % to 99 % in 26, 27, 28). In every other battle it is not above the base beyond noise. |
| **F-2, maximum** | In every battle of F-1's set, the largest friend overlap from the first stepped tick on, over pairs the solver corrects (WP0's column), is at most **10 %** of the radius sum (twice D-α's moving bound); in every battle it is not above the base's. |
| **F-3, harness** | In every harness family, the scenes with `fo_move_max` above 5 % are not more than the base's beyond noise, and in the arc and corner families they fall beyond noise. The gap family is judged only on "not more": its worst scenes are B-220's terrain-pinned shares, which C-3′ does not address by design (§4.2; this exception was written after the prototype showed it, and is stated as such). |
| **F-4, identity** | Every harness scene, matrix battle and showcase in which the extension never runs (probe counter of WP1 at 0) is byte-identical to the base; with `crowd_iterations` equal to `iterations` every world is byte-identical to the base. |

### 5.3 The behaviour checks (B1–B9 of the enemy-contact and collision-classes plans), against the WP0 base

| Check | Broken when | Note |
|---|---|---|
| **B1**, standing with room | D-R10 `free` or blocked unit-ticks above the base beyond noise (battle 28 reported, B-192) | Converged crowds may press the front harder |
| **B2**, shoving | D-SHOVE-E of either side above the base beyond noise | A converged friend correction can reach the front and, through C-8's even enemy split, the enemy: watched closely |
| **B3**, enemy overlap | Largest one-class enemy overlap above the base's, or enemy pair-ticks above 0.5 % above the base beyond noise | Enemy rules unchanged; their outcome may move |
| **B4**, churn | D-CHURN unexplained above the base beyond noise | |
| **B5**, give-ups | Give-ups of automatic chases above the base beyond noise | |
| **B6** | = F-1 to F-3 (§5.2) | |
| **B7′**, firing | Shots per engaged unit-second of either side below the base beyond noise (B7 reported) | |
| **B8**, standing while able to fight | Above the base beyond noise (the collision-classes probe, ported to `sim_fixtures` in WP0) | |
| **B9**, cross-class corrections | Any, anywhere | Collision classes are not on `main`; the probe reads 0 by construction and is kept for WP3 |
| **Harness T1–T9** | Each of the solver plan's thresholds that the base meets is met; none that the base fails gets worse beyond noise | Reversals, give-ups, throughput through gaps, mirror bias (T9) |

**Repairs carried over from the earlier plans** (not re-opened here): D-SHOVE-E (not D-SHOVE) for B2; B7′ per
engaged second; B3 and B6 of one body class; B6 read from the first stepped tick (this plan's §2.2).

### 5.4 Battles: the close-battle list and the held-out seeds

- **The close-battle list** is fixed in WP0 from WP0's 24-seed base alone, by the collision-classes rule
  (minority side wins at least 3 of 24; named close or restated in the fixture; the enemy-contact base's list) and
  committed to the evidence file before WP1's first build. Class R is that list; Class D every other battle.
- **Prediction (not a threshold):** the battles whose base has 10 or more friend pair-ticks above 5 % per seed move.
  On `main` (24 seeds, the meter as it is) they are 26 (1,106 per seed), 38b (630), 29 (611), 13 (553), 27 (512),
  38a (393), 28 (353), 39b (89), 11a (82), 39a (44), 34 (34), 40 (24) and 32 (12); 13 of 61 battles. Battle 13's
  held zerglings are pairs the solver never corrects (two holding friends, C-2's "none"), so it is expected to stay
  above the bound and not to move by this plan. Battle 26 is expected to move **towards its
  claim** (zerglings win 11 of 12 in the prototype, against 9 of 12 today).
- **A Class R battle** whose outcome moves is restated under charter §8 if every check of §5.2 and §5.3 passes for
  it: a first-difference trace of one moved seed to the extended solve, the range re-derived over the 24 seeds, the
  claim rewritten if the winner changes, a fixture comment citing AGENTS.md §8, Q0103, this plan and B-219.
- **A Class D battle** whose outcome moves: a stop for review, with the trace.
- **Held-out:** every restated claim holds on at least two thirds of the held-out seeds; F-1 and F-2 show the
  development seeds' direction there. A contradiction is a stop. Nothing is widened or tuned from them.

### 5.5 Cost budget

Base: the WP0 tree, single-scenario binaries (400 `pulse_bot`, 300 ticks), `perf stat -e instructions:u`, three runs
each, both pipelines. The solver on `main` is +66.8 % over Legacy on 64 × 64 (Q0102 (a)); the enemy-contact evidence
§7.4 measured 16.305 G (64 × 64), 10.956 G (128 × 128), 27.239 G (canyon).

- **C-0 (hard): every throughput scenario at least 50 ticks/s** (Q0093 (c)), on a host at load average below 8 (the
  measurement is repeated until it is; the evidence records the load). Projected for C-3′: about 70 / 183 / 53 to 55.
- **C-1 (the instruction budget): at most +60 % per scenario over the WP0 base** on 64 × 64 and canyon, and at most
  +2 % on 128 × 128 and the scenarios without a clash. The +60 % is the prototype's figure less its separate excess
  scan (§2.6), rounded up; it is a ceiling for the implementation, not a target. It puts the solver at about +165 %
  over Legacy on 64 × 64, far above the +70 % reporting line of the enemy-contact plan (Q0111's draft), which is why
  Q0118 goes to the owner **before WP2's switch-over** (§11), whatever C-1 measures.
- **C-2 (Legacy):** the Legacy matrix byte-identical to the base; Legacy instructions within the codegen band of the
  enemy-contact plan's C-1′ (its A2.3).
- **Allowed cost steps (behaviour-preserving, proved by bit identity, any number):** the excess recorded inside
  `accumulate` (no separate scan; required, §4.1); skipping the excess of a pair whose cached correction is reused
  (its overlap is the cached one); keeping the extension's iterations on the active set exactly as the first K.
- **Reporting:** the ratio to Legacy per scenario and the mean iterations per tick (probe counter) in `DESIGN.md` §5.

### 5.6 Stop rules (in advance)

1. **P-1 green on WP0's tree** with `crowd_iterations = iterations` (the crowd stays within D-α without the
   extension): the diagnosis is wrong. Stop before code.
2. **F-4 fails** (a world in which the extension never runs moves, or the switch-off is not byte-identical): a
   defect. Stop and find it.
3. **F-1, F-2 or F-3 fails:** stop. One amendment attempt within the package: raise `crowd_depth` only if P-1 shows
   the shipped value under-derived for a measured crowd (with the trace); no other knob.
4. **Any §5.3 check fails,** or a Class D battle moves, or a restated claim fails the held-out check: stop for review
   with the §5.3 table and traces. B2 or B3 failing means converged friend corrections reach the enemy: the fix is
   then the enemy-contact plan's, and the package waits for Q0113 rather than adding an enemy rule here.
5. **C-0 or C-1 fails** after the allowed cost steps: stop with the per-scenario split; Q0118 is re-asked with the
   measured figures. **WP2's switch-over also waits for Q0118's answer** (§11): (a) proceeds as written; (c) replaces
   C-3′ by shock propagation in an amendment with its own acceptance (its equilibrium change moves battle claims and
   the arc family, §4.3); (b) ends the package after WP1, with B-219 registered.
7. **WP3 (the re-measure) is records only:** if it finds the parked packages still failing B6 on the WP2 tree, that
   is reported to Q0113 / Q0117 with traces, not fixed here.
6. **Not allowed:** a constant, a per-unit or per-battle switch, a tolerance or cap chosen after seeing matrix
   figures, any change to shares, D-α, D-β, D-δ, the dead band, steering, the ring rule or any enemy rule.

---

## 6. Determinism, golden replays and checksum

- **Determinism.** Integer Q32.32 throughout; the extension's tests are a global "any" and an exact integer sum
  over pairs, so the solve is still a function of the body set (C-1a). Golden replay on the four builds (native debug and release,
  `wasm32-wasip1` release and debug under `wasmtime --dir=<real path>`).
- **Checksum schema.** No hashed field is added: the new values are catalog data, which is not hashed (§3.7 of the
  solver plan). The schema stays at `main`'s (49 today). If the implementer finds a hashed field necessary, it takes
  the next free number at merge (50 provisional) in an amendment.
- **Goldens re-recorded once**, on WP2's final tree after the held-out confirmation, in one commit
  (`golden_replay` and every scenario golden that moves), with the first-difference trace in the evidence. Any
  earlier re-record is a stop.

---

## 7. Work packages

Every package follows the charter's routine: tests red first on the package's base; `cargo gate quick` and the
tests the change needs, then hand back; a verifier runs `cargo gate package` (Q0061). One building agent at a time.

### WP0: measurement repair, base, close-battle list (S; `sim_fixtures` only; no behaviour change)

- **B6 from the first stepped tick** (`trace::contact_checks`): `count_overlaps` skips the first observation for the
  maximum and the pair-ticks; the spawn-inclusive maximum becomes a new column `friend_overlap_with_spawn`, appended
  last (`matrix.csv` readers of the old header keep working, as W0-T4). Test **W0-T8**: a two-body world that spawns
  at 30 % overlap and separates in one tick reports 0 pair-ticks and a maximum from tick 1, and 30 % in the new
  column. Red on `main`.
- **Pairs the solver corrects:** a second maximum and pair-tick count over friend pairs that are not both holding
  (`Entity::cannot_be_pushed` on both: C-2's "none" row, which D-α's solver never touches; battle 13's held zerglings
  reach 73 %). The all-pairs columns stay. Test **W0-T9**: two holding friends at 20 % overlap count in the all-pairs
  column only.
- **B8's probe** (collision-classes §10.1 (a)) ported as a `sim_fixtures` counter with a constructed test (an armed
  unit with a ready weapon and an available enemy in band that does nothing counts; one that swings does not).
- **Base of record:** matrix (24 and held-out), battles 31 and 33, showcases, harness (development and held-out),
  throughput (single-scenario binaries, three runs, both pipelines). Close-battle list committed to
  `docs/evidence/2026-11-26-friend-crowd-overlap.md` before WP1 starts.
- **Acceptance:** no world, checksum or harness figure moves; only the new and repaired columns.

### WP1: C-3′ latent (M; `sim_core`, catalog)

- The two catalog fields with validation and the header comment; `SolverSettings` and `SolveReport` as §4.1; the
  extension in `solve`; the flag recorded in `accumulate`; the probe counter (`MovementWorkspace` or
  `SolveReport`: extended ticks and iterations used, unhashed, read by the harness and the trace).
- **Shipped with `crowd_iterations` equal to `iterations`** (off). Acceptance: every world byte-identical to WP0 (F-4's
  second half).
- **Tests (red first):**
  - **P-1** `contact_solver_tests::a_crowd_pressing_a_front_stays_within_the_moving_bound` (the derivation test,
    §4.1): red on WP0 with any `crowd_iterations` equal to 21 (stop rule 1 if green); it prints the smallest
    passing value, which WP2 writes into the data.
  - **P-2** `solver::tests::the_extension_runs_only_above_the_moving_bound`: a crowd that ends within 5 % at K stops
    at K (bit-identical to the extension off); one above it continues and ends within the bound before the cap.
  - **P-7** `solver::tests::the_extension_stops_when_it_makes_no_progress`: a walker wedged between two anchored
    friends across a one-body gap, pressed from behind (no solution within the bound): the extension stops at the
    first iteration whose excess does not fall, and no friend overlap ends larger than with the extension off.
    The constructed form of harness scene corner 248 (§4.2).
  - **P-3** `solver::tests::the_extension_does_not_depend_on_body_order`: every permutation of a random mixed crowd
    gives the same positions (extends `the_bound_does_not_depend_on_body_order`).
  - **P-4** `movement_rules::tests` for the fields: absent reads `iterations`; smaller than `iterations`, above 1,000
    or a zero depth refused with typed errors.
  - **P-5** `contact_solver_tests::crowd_iterations_cover_the_catalogs_crowd`: recomputes C-3′a from the shipped
    catalog and fails if the data is smaller (as C-3's test for K).
  - **P-6** a cost guard: the extended iterations of P-1's scene stay within the cap on every tick.

### WP2: switch-over and acceptance (M; data, records)

- `crowd_iterations` set to P-1's value in `movement_rules.ron` (with `crowd_depth: 12`), in one commit with the
  header's derivation note.
- Every run of §5 on WP2's tree against WP0's base; the evidence file's tables; the restatements (Class R only, under
  §5.4); the held-out confirmation; the throughput gates.
- `DESIGN.md`: §4 register (B-219 fixed, with the figures; B-220 registered, open, §10), §5 throughput, and the
  solver plan's C-3 noted as superseded by C-3′ for crowds (an amendment to `2026-11-20-contact-solver` pointing
  here).
- **Starts only after Q0118 is answered (a)** (§5.6 rule 5). WP0 and WP1 do not wait.
- The single golden re-record (§6), last.

### WP3: re-measure the parked changes on top (S; scratch branches, records only; no merge)

See §8. Its result is evidence for Q0113 and Q0117, not a merge of either package.

---

## 8. How this unblocks Q0113 and Q0117

Both parked packages failed checks that are this plan's B-219: B6-max on chains behind a held friend (enemy
contact, A2.4 battle 26 and its §7.3 trace: "1 direct contact and 2 chains"), and B6 pair-ticks in battle 26 under
collision classes (124.1 against 54.8 per 1,000 zergling-ticks). WP3, after WP2 is on `main`:

1. **Enemies block.** A scratch merge of `enemy-contact-plan` (`7f686d23`, E-2 and F-1) into `main` after WP2, in a
   worktree of its own; the branch itself is not modified. Re-run that plan's A2.4 checks (B1, B2-a, B2-b, B3, B4,
   B5, B6, B6-max, B7′) against a base re-derived on the same `main`, with B6 read from the first stepped tick.
   **Expectation, stated now:** B6 and B6-max pass in every battle (the chains converge); battle 27's B1 (zerglings
   standing with room) and B2-b near-misses are not friend overlap and are expected to remain for that plan.
2. **Collision classes.** The same with `collision-classes-wp3-wip` (`42761cf2`): B1 to B9 of its §7.2 against a
   re-derived base. **Expectation:** B6 passes in battle 26; B4 and B7′ are not friend overlap and may remain.
3. **Records:** one evidence section per package in this plan's evidence file, and a one-paragraph addendum to each
   owner question (Q0113, and Q0117 once filed) stating which of its stopping checks are gone and which remain. No
   restatement, marker or golden of either package is made here; each package resumes under its own plan, on its own
   branch, with its own amendment.
4. If either package's F-1 rule or its B6-max check becomes redundant once crowds converge, that package's next
   amendment says so; this plan does not change it.

---

## 9. Risks

| Risk | Mitigation |
|---|---|
| Converged friend corrections push the front into enemies (B2, B3 rise), since C-8 splits enemy pairs evenly | B2, B3 judged; stop rule 4 sends it to the enemy-contact plan instead of an enemy rule here |
| Cost: the throughput clashes are the broken case, so convergence costs about half again on 64 × 64 and canyon | Measured before any code (§2.6); the owner decides with Q0118 before WP2; C-0's 50 ticks/s stays hard |
| Canyon close to 50 ticks/s | Projected 53 to 55; measured on a quiet host; stop rule 5 |
| P-1 derives a cap that the 400-unit benchmark's worst tick exceeds | P-6 and the probe counter report ticks at the cap; a tick at the cap still ends with the solver's best state (no failure), and F-2 bounds the result |
| Battle outcomes move in many battles | Expected for dense melee crowds (§5.4); judged by Class R / D, not by winners; the held-out seeds confirm |
| The harness corridor family hits the cap on 77 % of its ticks | Within the bound on all of them: the extension does not start (prototype: corridor byte-identical, §2.7 **verify** in WP1) |
| `SolveReport::iterations` widening breaks readers | The harness's `iteration_ticks` histogram grows buckets above 21; WP1 updates its reader and the CSV header test |

---

## 10. Out of scope (decisions recorded, Q0002)

- **Shock propagation, D-β for equal priorities, steering slides** (§4.3): measured, not adopted; shock propagation
  is Q0118's option (c).
- **B-220 (provisional): a friend pair with shares 0 / 1 whose share-1 body terrain holds is never resolved.** The
  solver plan's Amendment 11 named it R5 ("0/1 shares become infeasible when the share-1 body cannot move … `apply`
  and `contact_shares` never move the share to the partner, though C-6 says a body held by terrain acts as fixed after
  one iteration") and proposed P3 (shock propagation against terrain), which was not built. It is the gap family's
  worst scenes (up to 96 %, mixed priorities in one-cell gaps). Registered in `DESIGN.md` §4 in WP2 with these
  figures, not fixed here: its fix changes shares, which this plan does not.
- **The spawn layout of matrix battles** (spacing 0.7 for bodies 0.75 wide, jitter ±0.12): not changed; the
  measurement no longer counts it (§2.2). A fixture with spawn overlap is a valid test that the solver separates it.
- **The enemy-contact and collision-classes packages themselves**: unchanged; re-measured in WP3 only.
- **C-3's K = 21** stays: it governs every tick that converges within it, and changing it would move every world.
- **Gauss–Seidel or SIMD cost work** (Q0093's note): a later cost plan.

---

## 11. Owner question (draft, not filed; recommended for filing before WP2)

```markdown
# Q0118: Crowds overlap far more than the 5 % you allowed. Pay for the solver to finish its work, keep it as it is, or change how crowds push?

- **Asked:** <date filed>
- **Blocking:** only the switch-over (WP2) of plan `2026-11-26-friend-crowd-overlap`, and through it the resumed
  "enemies block" (Q0113) and collision-classes (Q0117) packages. Measurement and latent code continue.
- **Default until answered:** (b), today's movement stays; the fix is built switched off.

## Background

You allowed friendly units to overlap by up to 5 % of their size while moving (deviation D-α). In big crowds they
overlap much more: a hundred zerglings attacking six colossi reach 39 % (812 pairs above 5 % per run), and the
400-marine benchmark reaches 47 %. The cause is a defect, not a rule: the movement solver stops after 21 rounds per
tick, and a crowd ten units deep needs 50 to 170 rounds for the push from the front to reach the back. Two parked fixes
(enemies that block, zerglings under the colossus) stopped on exactly this.

## Options

- **(a) Let the solver finish (recommended).** It keeps working only while some friendly pair is above 5 % and it is
  still making progress. Battle 26: 812 → 11 pairs above 5 % per run, worst 39 % → 7.6 %; nothing else about
  movement changes, and every situation that is fine today stays bit-for-bit the same. Cost: about +50 % to +60 %
  instructions on the two 400-unit battle benchmarks (64 × 64 and canyon), none elsewhere; projected about 70 and
  53 ticks per second, above your 50 floor, canyon with little margin.
- **(b) Keep it as it is (default).** Crowds keep overlapping; the defect is registered (B-219); enemies-block and
  collision classes stay parked.
- **(c) Change how crowds push (cheap).** A unit standing behind a friend that cannot give way may no longer push
  that friend; it takes the whole correction itself ("shock propagation", standard in physics engines). Overlap
  falls by about 90 % and the benchmark gets 10 % cheaper, but crowds stop pressing their front line aside, which
  looks more like StarCraft II units queueing behind a full surround, and moves results: battle 26's zerglings go
  from winning 9 of 12 runs to 2, and units in one test family turn back and forth more than twice as often.

Recommendation: (a), since the cost stays above 50 ticks per second and nothing but the broken crowds changes; (c)
if you prefer the queueing look and a cheaper solver over the battle claims.

## Answer
```

---

## 12. File ownership

| Package | Files |
|---|---|
| WP0 | `crates/sim_fixtures/src/trace/contact_checks.rs`, `trace/matrix.rs`, `crates/sim_fixtures/tests/trace_tests.rs`, evidence file |
| WP1 | `crates/sim_core/src/movement/solver.rs`, `movement/collision.rs`, `catalog/movement_rules.rs`, `assets/catalog/movement_rules.ron` (header and fields, extension off), `crates/sim_core/tests/contact_solver_tests.rs`, harness reader of `iteration_ticks` |
| WP2 | `assets/catalog/movement_rules.ron` (value), `crates/sim_core/tests/fixtures/matchup_matrix.ron` (restatements), goldens, `DESIGN.md`, evidence, the solver plan's amendment |
| WP3 | evidence only; scratch worktrees |

## 13. Material (scratch, not committed)

`~/scratch/friend-crowd/`: `scripts/` (`arm_summary.py`, `ovl_classes.py`, `harness_sum.py`, `run_arms.sh`,
`bench.sh`), `trees/proto*` (the prototypes: `FC_DEPTH`, `FC_SLIDE`, `FC_EXT`, `FC_PROBE`, `FC_PASSES`), `arms*/`,
`proto*/` (battle traces), `harness/`, `bench-*.txt`, `perf-*.txt`, `arms-*.txt`, scratch catalogs `cat-*`.
