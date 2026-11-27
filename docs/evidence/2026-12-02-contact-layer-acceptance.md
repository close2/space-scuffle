# Evidence: Contact-Layer Acceptance (WP-A) — stopped at T1

Measurements of plan [`2026-12-02-contact-layer-acceptance`](../plans/2026-12-02-contact-layer-acceptance.md), WP-A.
Scratch material is under `~/scratch/contact-layer/` (scripts, trees, binaries, arm catalogs, worlds, logs, analysis)
and is never committed.

**Result: WP-A stops at T1 (plan §4.2, §7 rule 3).** G fails the owner's zig-zag condition in five harness families
and in battles 26 to 29. R does not fail the same figures in most of them, so amendment A-1's trigger does not hold,
and no amendment was used. Under §7 rule 3 the plan stops for the owner (draft QC-1). No other figure was read
after T1, and none is reported as judged.

## 1. Method

- **Tree.** `M₀` = `main` `edbac56b` (wall-clearance WP1 merged). WP-A adds no code, so the WP-A tree *is* `M₀`.
  `main` changed simulation code after the wall-clearance re-base tree `0c8969e9` (death spawns, hangar commands,
  catalog units), so the plan's §3 applies: the base is re-measured in full here, with arms S, R and L on `M₀`.
- **Binaries** (release, each in its own target directory):
  - `M₀` exported (`trees/head`, `…/cargo-target/contact-layer-a-rel`, tag `a`);
  - its Gated twin with `pipeline: Gated` in the embedded catalog (`trees/headG`, `…/contact-layer-a-relg`, tag `ag`),
    because `movement_harness --pipeline` takes only `legacy|solver` on `M₀` (plan §2.1 **verify**: confirmed);
  - a bench copy that reads its scenario table from `$BENCH_TABLE` (`…/contact-layer-a-bench`; not used, see §4).
- **Arms.** S, R and L run on tag `a`, the harness by `--pipeline` (R also `--crowd-iterations 151`), the
  matrix, battles, clashes and showcases by scratch catalog directories `assets/{S,R,L}`. G runs on tag `ag`
  with catalog `assets/G`, which is shipped `assets/` with only `pipeline: Gated`.
- **Runs** follow the Gated WP0b command lines (`scripts/measure.sh`), with `--jobs 6`. Each run waited for 15 GiB of
  headroom in the user slice, then ran under `flock ~/scratch/gate.lock`. No run went without the lock: the
  starvation fallback was prepared but never needed. No run was killed (the host's OOM events of the day hit no
  WP-A process). Every harness file has 1,000 rows.
- **Verdicts.** `scripts/t1.py` (Gated WP2's script, extended to the two clashes): M1 and M1-hi per 1,000 moving
  unit-ticks against S's 95 % bootstrap upper bound (2,000 resamples, seed 7). The harness uses the ratio of sums
  over scenes, and the battles and clashes use the mean of per-seed rates, per side.

## 2. Identity (run before T1)

- **H-6.** The WP-A tree is `M₀` itself, so S and L are `M₀`'s by construction.
- **The two binaries' S arms** (plan §2.1). S on the Gated twin (`--pipeline solver`, catalog `assets/S`) against
  S on `M₀`'s binary gives:
  - harness, 13 families and the mirror, seeds 1 to 200: **14 of 14 files byte-identical**;
  - the matrix (24 seeds), both clashes, battles 31 and 33: **177 of 177 CSVs byte-identical**.
- **Base sanity.** The S, R and L matrices give 2 / 32, 1 / 30 and 3 / 22 expectation failures, the same as the
  wall-clearance re-base. S's harness M1 and M1-hi equal that re-base's in every family (§3 below). S's battle
  figures for 26 to 29 equal `gated-wp2` §12.3's S column. G's matrix gives 8 / 44 failures.

## 3. T1, the zig-zag measure (plan §4.2): **fails**

Development: harness seeds 1 to 1,000, battles 26 to 29 on the 24 judging seeds, and both clashes on their five
seeds. Bold marks a failing figure: above S's upper bound, or above 1.0 / 0.25 where S meets that bound.

| Where | S M1 (upper) | S M1-hi (upper) | R M1 / M1-hi | G M1 / M1-hi | G verdict |
|---|---|---|---|---|---|
| gap | 1.12 (1.25) | 0.28 (0.31) | 1.15 / 0.30 | **2.21 / 0.60** | fails |
| arc | 2.40 (2.98) | 0.19 (0.21) | 2.41 / 0.18 | 0.46 / 0.13 | holds |
| cross | 1.19 (1.29) | 0.49 (0.53) | 1.19 / 0.49 | 1.19 / 0.49 | holds |
| corridor | 0.00 (0.00) | 0.00 (0.00) | 0.00 / 0.00 | **0.0044 / 0.0044** (7 reversals; S 0) | fails (marginal) |
| corner | 0.41 (0.45) | 0.10 (0.11) | 0.39 / 0.09 | **0.67 / 0.17** | fails |
| hold-line | 29.06 (31.37) | 1.52 (1.89) | 30.02 / 1.88 | 30.02 / 1.88 | holds |
| enemy-press | 10.60 (11.54) | 1.53 (1.69) | 10.87 / 1.47 | **12.16** / 1.26 | fails |
| deep-queue | 68.83 (119.97) | 1.66 (2.74) | 24.95 / **5.42** | 1.04 / 0.66 | holds |
| wedge, hallway | 0 | 0 | 0 / 0 | 0 / 0 | holds |
| stutter | 50.07 (50.98) | 49.52 (50.42) | 50.07 / 49.52 | **57.39 / 57.18** | fails |
| surround-moving | 6.59 (6.88) | 2.28 (2.36) | 6.65 / 2.30 | 5.16 / 0.96 | holds |
| lanes | 1.10 (1.17) | 0.19 (0.20) | 1.12 / 0.19 | **2.83 / 0.67** | fails |
| 26 blue (zerglings) | 5.46 (6.36) | 0.54 (0.64) | 3.97 / **0.77** | **8.67 / 0.98** | fails |
| 26 red (colossi) | 0.61 (1.29) | 0.61 (1.29) | 0.59 / 0.59 | **1.39 / 1.39** | fails |
| 27 blue | 2.80 (3.19) | 0.52 (0.62) | 2.36 / 0.59 | 2.54 / **0.63** | fails |
| 27 red | 1.54 (2.26) | 0.09 (0.20) | 1.27 / 0.06 | 1.55 / 0.09 | holds |
| 28 blue | 2.32 (2.82) | 0.39 (0.44) | 1.72 / **0.49** | 2.30 / **0.57** | fails |
| 28 red | 0.65 (1.02) | 0.20 (0.40) | 0.79 / 0.19 | **1.09 / 0.50** | fails |
| 29 blue | 2.90 (3.29) | 0.22 (0.26) | 1.68 / **0.39** | 2.37 / **0.42** | fails |
| 29 red | 0.91 (1.36) | 0.30 (0.55) | **1.23 / 0.55** | **1.91 / 0.75** | fails |
| clash `pulse_bot` blue / red | 8.03 (8.74) / 6.37 (7.39) | 1.07 (1.30) / 0.92 (1.07) | **11.14** / 0.66, **11.19** / 0.61 | 5.15 / 0.59, 5.03 / 0.51 | holds |
| clash `razor_mantis` blue / red | 1.13 (1.39) / 0.85 (1.06) | 0.05 (0.07) / 0.07 (0.10) | 1.15 / **0.08**, **1.15** / 0.06 | 0.94 / 0.05, 0.89 / 0.05 | holds |

G fails 28 figures, in gap, corridor, corner, enemy-press, stutter and lanes, and in battles 26 (both sides),
27 blue, 28 (both sides) and 29 (both sides). R fails 12, in deep-queue, battles 26 blue, 28 blue and 29, and both
clashes. Full table: `~/scratch/contact-layer/analysis/t1-dev.md`.

Notes on single rows:
- In stutter, G's rate rises partly because its moving unit-ticks fall: 207,436 against S's 242,532. Its
  reversals fall too (11,905 against 12,144).
- In enemy-press, moving unit-ticks also fall (11.28 M against 14.53 M).
- The corridor failure is 7 reversals in 1.585 M moving unit-ticks, against S's 0.

The registered rule makes no exception for any of these.

**The reversal split** (`trace::blocking`; plan §4.2 **verify**: `main`'s meter gives it). Reported, not judged.
Battles 26 to 29 are summed over 24 seeds; `ext` is the mean of `solver_extended_ticks` per seed.

| Battle | S proposal / contact | R proposal / contact (ext) | G proposal / contact (ext) |
|---|---|---|---|
| 26 | 64 / 3,820 | 69 / 4,247 (79.8) | 80 / **14,426** (122.9) |
| 27 | 70 / 2,955 | 57 / 2,780 (45.1) | 72 / 4,196 (55.0) |
| 28 | 85 / 5,881 | 83 / 5,365 (40.5) | 86 / 8,561 (64.5) |
| 29 | 34 / 6,514 | 50 / 6,304 (59.1) | 52 / 10,126 (100.6) |

Under G, the units' proposals reverse no more than under S. The added reversals are **contact-caused**: the steps
C′'s corrections give them. WP2's reversals (`gated-wp2` §12.3) were the opposite kind, proposal-caused from the
gate. The clashes' meter records no split (0 / 0 in every arm).

### 3.1 Amendment A-1's trigger (plan §5): does not hold

A-1 applies only if T1 fails **only** where R fails the same figure against S and G's `solver_extended_ticks` there
is positive. Most of G's failing figures have no matching R failure:
- gap, corridor, corner, enemy-press, stutter and lanes;
- battle 26 M1 (blue) and both figures (red);
- 27 blue M1-hi;
- 28 red;
- 29 red M1-hi.

So C-3′ does not explain the failure, and A-1 is not available. A-2's trigger is F-1′ or F-2′, not T1.
**Stop rule 3 applies: stop for the owner, draft QC-1** (plan §11). Filing it is the orchestrator's.

## 4. Not run or not read after the stop

By §4 and §7 rule 3, nothing after T1 is read:
- the hard checks;
- H-3 (its permutation run was stopped during its first family; the partial file is unread);
- F-1′ and F-2′;
- the step rule and the known answers;
- battles 31 and 33 and the showcases (played; not read);
- the callgrind session (not run);
- the acceptance range 300,001 to 301,000, and the held-out sets (harness 9,100,001 to 9,101,000; matrix 45th to
  68th non-record seeds).

None of these seeds was played, so all of them stay fresh for any later attempt. Wall-clearance's HA and HB were
not touched. No scratch directory under `~/scratch/contact-layer/` holds worlds on those seeds.
