# Blueprint: Investigation of Battle 33, Marines Against Zealots (Q0007)

This plan designs an investigation, not a fix. It asks why battle 33 of the who-wins matrix ends with the marines
winning and 6 to 13 of 24 alive, when plan 21a and common StarCraft II knowledge expect the zealots to win a close
fight. Its output is an evidence document with findings and measured effect sizes, plus one owner question per
proposed change. **No catalog value, combat rule or expected outcome changes in this plan.**

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-03 (UTC; the file carries the
date its work starts) against `main` at `9e693cf`. Line numbers drift; search for the quoted symbol. Claims that
could not be confirmed while planning are marked **verify**.

**Outcome (2026-11-04):** done. WP1 to WP5 are written up in
[`docs/evidence/2026-11-03-battle-33-investigation.md`](../evidence/2026-11-03-battle-33-investigation.md); its §6
ranks the causes and §7 lists the owner questions Q0035 to Q0041, each defaulting to no change.

**Owner decisions.** [Q0007](../questions/closed/0007-combat-baselines.md), answered 2026-11-03: "Keep all five as
committed. I do however want an 'investigation' why internet says that Zealots should win, and why we have 6-13 of 24
marines surviving. This is a big difference." [Q0008](../questions/closed/0008-game-rules.md): "Whenever the spec is
silent make an internet search how sc2 does it."

---

## 1. Summary

| WP | Title | Size | Depends on |
|---|---|---|---|
| WP1 | Reference research: what StarCraft II does, with sources | M (1 day) | none |
| WP2 | Side-by-side comparison of our data and rules with the reference; analytic model | S-M (half a day) | WP1 |
| WP3 | Instrumentation: a per-tick trace of a matrix battle, and a 64-seed outcome distribution | M (1 to 1.5 days) | none (parallel with WP1) |
| WP4 | Experiments: history bisection and one-variable counterfactuals on scratch catalogs | M-L (1.5 to 2 days) | WP2, WP3 |
| WP5 | Evidence document, ranked causes, questions for each proposed change | S (half a day) | WP4 |

One agent can do WP1 and WP3 in parallel worktrees, then WP2, WP4, WP5 in order. Total about 4 to 5 days.

---

## 2. What is known

### 2.1 The battle

`crates/sim_core/tests/fixtures/matchup_matrix.ron`, battle `33_24_marines_vs_12_zealots` (equal supply: marines 1,
zealots 2):

- Flat 96 × 96 map, 500 ticks (20 s at 25 ticks per second).
- Blue: 24 `pulse_bot` (marine), 4 columns × 6 rows, origin (20, 44), spacing 0.8, `AttackMove(45, 48)`.
- Red: 12 `psi_grey` (zealot), 3 columns × 4 rows, origin (45, 44), spacing 1.2, `AttackMove(20, 48)`.
- Fronts about 22.6 apart. No research: no Stimpack, no Combat Shield, no Charge, level 0 weapons and armour.
- Seeds `[3203386110, 11, 22, 33]`; the first uses exact positions, the others jitter each spawn by up to ±0.12 per
  axis. Expected `blue: (4, 16), red: (0, 0)`. Per-seed survivors were never recorded; "6 to 13" is the range over
  the four seeds (commit `d4109e2`).
- Runner: `crates/sim_core/tests/sc2_who_wins_matchup_matrix_tests.rs` (`play`, `spawn_side`, `side_commands`).

**History.** The zealots won on all four seeds through `1349c67` (Phase 3 exit, claim "Unkited marines lose to
zealots"). `d4109e2` (remediation Phase 4, arrival and contact rules, M-1 to M-18) flipped it; its message says:
"Zealots won by overlapping onto the marine line." That commit made hostile bodies impenetrable (uncapped hostile
overlap correction, `movement/collision.rs`), added flanking around blocked attackers (`flank_if_blocked`), and
stopped units standing inside each other. Related battles still favour zealots: 25a (2 marines vs 1 zealot), 25b
(upgraded marines vs 1 zealot), and `sc2_battle_benchmarks_tests.rs` (1 zealot beats 2 marines; 4 marines beat 1
zealot).

### 2.2 Our data (`assets/catalog/units.ron`)

| Field | `pulse_bot` | `psi_grey` |
|---|---|---|
| Attributes | Light, Biological | Light, Psionic |
| Life / shields / armour | 45 / 0 / 0 | 100 / 50 / 1 (shield armour 0), shield regen 2/s after 10 s |
| Speed | **0.09 per tick = 2.25 per s** | 0.126 per tick = 3.15 per s |
| Collision / separation radius | 0.375 / 0.45 | 0.5 / 0.6 |
| Weapon | 6 damage, range 5, hitscan, arc 15° | 8 × 2, range **0.3**, melee, arc 180°, both strikes in one tick |
| Period / damage point / backswing (ticks) | 15 / 2 / 4 | 22 / 4 / 6 |
| Push priority | 20 | 25 |

Time convention: 25 ticks per StarCraft II "Faster" second (`docs/07-sc2-patch-5-0-14-roster-and-faction-mapping.md`).

### 2.3 Our rules (`sim_core`)

- Weapon state machine (`combat.rs`, `step_entity_weapon`): turn, wind-up for the damage point, release, backswing,
  cooldown `period - damage_point`. A swing is bound to its target and is **cancelled without cooldown** if the
  target dies or leaves the band (`max_range + both radii`, release slop 0.5).
- Damage (`combat.rs`, `mitigate_strike`): shield armour first, spillover minus life armour, minimum 0.5.
- Hits (`combat_resolution.rs`): instant for hitscan and melee; shots prepared in entity-id order; a target killed
  earlier in the same phase wastes the later shots. No overkill prevention.
- Targeting (`target_acquisition.rs`): threat priority, then distance, then id; switching margin 1.0; a blocked
  attacker switches to an equal enemy it can reach; call for help only to idle allies within 5.
- Movement: pursuit stops inside the band; `flank_if_blocked` walks blocked attackers sideways; hostile bodies never
  overlap; friendly push capped at the unit's speed; a chase stalled 250 ticks is abandoned. Acceleration is parsed
  but unused (instant speed).
- No automatic micro: marines do not kite.

### 2.4 First observations (to be tested, not conclusions)

Two of our deviations favour the zealots (marine speed 2.25 instead of 3.15 lets zealots close faster and chase;
zealot reach 0.3 instead of 0.1), so they do not explain a marine win. The mechanisms most likely to favour marines
are geometric: how many zealots reach a marine at once (radii, separation radii, impenetrable bodies, flanking), and
swings cancelled when a target is pushed or dies. A back-of-envelope Lanchester estimate (WP2 makes it exact):
marine DPS against a zealot is 6/0.6 = 10 on shields and 5/0.6 = 8.3 on life, so a zealot takes about 17
marine-seconds; a zealot's DPS is 16/0.88 = 18.2, so a marine takes about 2.5 zealot-seconds. With all 24 marines
firing and all 12 zealots in contact, the zealots win clearly; the outcome flips when only a fraction of zealots is
in contact at a time. WP2 computes that fraction, and WP3 measures it.

---

## 3. Design decisions

### D-1. Research first, with sources, and only what the comparison needs

Every value is recorded with its source URL, the patch it applies to (the catalog targets 5.0.14 per doc 07), and the
time base (Normal or Faster). Primary sources, in order: the game's balance data as extracted by community tools
(for example the `sc2-techtree` data or the Liquipedia data pages, which quote the editor values), Blizzard patch
notes for changes, Liquipedia unit pages, then unit-tester videos and articles for observed outcomes. A value found in
only one secondary source is marked so.

**The lookup list (WP1 records each item):**

1. **Marine**: life, armour, attributes, movement speed, acceleration and deceleration, turn rate, collision radius
   (and separation or "inner radius" if published), weapon damage, cooldown, range, damage point and backswing (the
   editor's "Damage Point" and "Backswing"), whether the attack is instant, target acquisition range, sight. Combat
   Shield (+10 life) and Stimpack (cost 10 life, duration, +50 % speed, attack speed factor) for battle 25b and for
   completeness.
2. **Zealot**: life, shields, armour, shield armour, attributes (is the zealot Biological? the catalog says Psionic),
   movement speed, acceleration, collision radius, weapon (8 × 2), cooldown, **range 0.1** and how melee range is
   measured (edge to edge), the timing of the two strikes (both on one frame or separated, and the delay), damage
   point, backswing, shield regeneration rate and delay.
3. **Charge** in 5.0.14: speed bonus, the charge ability (trigger range, speed during charge, any damage on impact,
   cooldown, autocast) and when it changed (patch notes 4.7.1, 4.11, 5.0).
4. **Armour upgrades**: per-level armour and weapon bonus for both units; confirm none apply in battle 33.
5. **Time base**: the Normal-to-Faster factor (1.4) and which published numbers are in which base; game loops per
   Faster second (22.4).
6. **Mechanics** that shape the fight: melee units' surround behaviour and how many attackers fit around a marine;
   whether units can push enemies (they cannot; enemies are solid in StarCraft II); separation or "spread" of a
   moving group; whether the attack-move acquisition prefers the closest target; overkill (whether simultaneous
   marine shots overkill; there is no smart-fire for marines); whether a melee attack started on a target that moves
   away is cancelled and its cooldown spent.
7. **Observed outcomes** with their setup: unit-tester results for "24 marines vs 12 zealots" (or equal supply) with
   no upgrades, no stim, no Charge: who wins, how many survive, the formation (clumped, line, concave), the start
   distance, attack-move or a-click, and whether marines were spread. At least three independent sources, each
   with its setup; a source whose setup is unstated is recorded but weighted low. If no source matches battle 33's
   setup exactly, the closest ones and their differences are recorded.
8. **The internet claim itself**: find the statements behind "zealots should win" (Liquipedia matchup pages, guides)
   and what conditions they assume (often Charge and a surround, or upgrades).

### D-2. Instrumentation is a reusable test-support tool, committed

A trace tool that only lives in a scratch directory cannot be rerun by a reviewer. So WP3 adds, in the test-support
crate `crates/sim_fixtures` (already used by the matrix runner), a `trace` module and a runner:

- `sim_fixtures::trace::BattleTrace`: given a `SimWorld` and the two sides' entity ids, records per tick, from the
  public state (`SimWorld.entities`, `pending_events`, `drain_movement_outcomes`): alive counts; per unit position,
  velocity, life, shields, weapon phase, current target; per side, damage dealt this tick (from life and shield
  deltas, attributed to the side that was hit), shots or swings released, swings cancelled (wind-up left without a
  release), shots wasted on a target already dead this tick; per zealot, whether it is in contact (inside its band of
  some marine), blocked (has a target out of band and speed below 10 % of maximum), flanking, or walking.
- Derived per battle: time of first contact; per tick the number of zealots in contact; zealot-seconds in contact,
  blocked and walking; marine-seconds firing; damage per side over time; survivors and remaining life.
- Runner: `crates/sim_core/examples/matchup_trace.rs` (examples can use `sim_fixtures` as a dev-dependency,
  **verify**), arguments `--battle <name> --seed <n> [--catalog-dir <dir>] [--seeds <count>] --out <dir>`; writes
  one CSV per run and a Markdown summary. It reads the matrix fixture, so it names no unit in Rust.
- Unit tests of the derived measures on a tiny scripted battle (two units, known contact tick). No change to
  `sim_core` itself; if a needed value is not observable from public state (for example a cancelled swing), the
  tool infers it from consecutive weapon phases, and the inference is unit-tested.

### D-3. A wider outcome distribution

Four seeds give a range, not a distribution. WP3 runs battle 33 on 64 jitter seeds (the runner's `Jitter`
generator, seeds 1 to 64 plus the four of record) and reports the survivor histogram, the mean and the spread. It
does the same for 25a, 25b and the benchmark battles, so a cause found in 33 can be checked against fights that
still favour zealots.

### D-4. Experiments change data in scratch copies only

Counterfactuals use catalog injection (a catalog directory passed to the runner; `--catalog-dir`), never the shipped
catalog, and run with the runner, never with the matrix test (whose expectations stay as committed). Rules
counterfactuals that need code (for example allowing hostile overlap again) are measured by **history bisection**,
not by patching: build the runner at `1349c67` and at `d4109e2`, and at `d4109e2`'s parent, in separate worktrees
(`git worktree add ~/scratch/wt-b33-<sha> <sha>`), and run the same battle and seeds. If `d4109e2` bundles several
rule changes, a throwaway branch reverting one at a time is allowed **in `~/scratch` only, never merged**, and its
diff is attached to the evidence.

### D-5. Output

`docs/evidence/<date>-battle-33-investigation.md`:

1. The reference (WP1's table, with sources).
2. The comparison (WP2's table: each field ours vs reference, the direction it pushes the outcome).
3. The analytic model and the measured contact fraction.
4. The trace findings (time series plots are not needed; tables at 1-second resolution suffice).
5. The experiments: each one variable, its outcome distribution on 64 seeds, its effect against the baseline.
6. Ranked causes, each with its measured effect.
7. Proposed changes. Each proposal is a separate question in `docs/questions/open/` (the next free number) with
   the evidence, the expected effect on battle 33 and on every other matrix battle it would move (measured by
   running the whole matrix on the scratch catalog), and the default "no change until answered", because Q0007
   keeps the baselines. A proposal the owner accepts gets its own blueprint later; this plan writes none.

---

## 4. Work packages

Common rules: worktree per WP, `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/b33-wpN`, logs
`~/scratch/logs/b33_wpN_*`, scratch data `~/scratch/b33/`; `df -h / ~/scratch` first; target removed after merge.
No GPU is needed. Gate for code: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D
warnings`, `cargo test --workspace --locked`; for documentation only: `cargo fmt --all -- --check`, `cargo test
--locked -p workspace_checks`. No catalog identity in Rust; no change to `assets/catalog/*`, `sim_core/src/*` or
`matchup_matrix.ron`.

### WP1: Reference research

Web research per D-1's list. Output: a section of the evidence file (started here) with one table per unit, one for
Charge and mechanics, one for observed outcomes, each row with source and patch. Mark contradictions between sources
and say which is used and why. Effort is bounded: if an item cannot be sourced in reasonable time, record "not
found" and the best secondary value.

### WP2: Comparison and analytic model

1. The comparison table (D-5 item 2), field by field, including the time base of every value. Flag each difference
   that is not explained by a documented project choice (for example the Psionic tag, the marine's 0.09 speed that
   dates from the first commit `3d9a9c4`, range 0.3, period 22 against the reference cooldown in Faster seconds × 25).
2. A small analytic model in the evidence file (a table, computed by hand or by a throwaway script in
   `~/scratch/b33/`): time-to-kill per unit pair, the closing time under our speeds and the reference speeds, marine
   damage dealt before contact, and the fraction of zealots that must be in contact on average for the zealots to
   win (Lanchester square law with the contact fraction as the parameter).

### WP3: Instrumentation and the distribution

1. Tests first for the trace's derived measures (D-2), on scripted two- and three-unit battles.
2. Implement `sim_fixtures::trace` and `examples/matchup_trace.rs`.
3. Run battle 33 on the four seeds of record and on 64 more (D-3); record the per-seed table and histogram. Run 25a,
   25b and the zealot benchmarks the same way.
4. Report, for battle 33: the time of first contact; the number of zealots in contact over time (mean, maximum);
   zealot-seconds blocked or flanking; swings cancelled; marine shots wasted; damage per side per second.

### WP4: Experiments

Each experiment changes one thing and reports battle 33's 68-seed distribution and its change against WP3's
baseline. The list, ordered by expected information:

1. **History:** the runner at `d4109e2`'s parent and at `d4109e2` (D-4); the trace shows what "overlapping onto
   the marine line" meant (zealots in contact over time before and after).
2. **Reference data**, one value at a time on a scratch catalog: marine speed 3.15; zealot range 0.1; zealot period
   from the reference cooldown; marine and zealot damage points and backswings from the reference; separation radius
   equal to the collision radius for both; zealot attributes as in the reference. Then all reference values together.
3. **Formation and setup:** zealots in a line facing the marines rather than a 3 × 4 block; marines spread at 1.2
   instead of 0.8; start distance halved; the setup of the best-matched observed outcome from WP1, if it differs.
4. **Rules that may be off, measured by trace only** (no patch): how often a zealot swing is cancelled because its
   target was pushed out of band, and how many zealots wait behind another zealot (blocked) rather than flank.
5. **Control:** run the whole matrix on the all-reference catalog of item 2 to see which other battles move; this
   is the cost side of any proposal.

### WP5: Evidence and questions

Write D-5's sections 5 to 7. Rank causes by effect size. For each proposed change, file a question (D-5 item 7). Add
the evidence file to the plan index's evidence links where the checks require it (C17 to C20). Update this plan's row
in `docs/plans/README.md`. Mention in `DESIGN.md`'s battle-33 note (around "await the owner (Q0007)") that the
investigation's evidence exists.

---

## 5. Risks

| Risk | Mitigation |
|---|---|
| Internet sources disagree or describe other setups | D-1 records setup and source per value; WP4 item 3 reproduces the best-matched setup |
| The trace's inferred values (cancelled swings) are wrong | Unit tests on scripted battles with known answers (WP3 step 1) |
| History bisection needs builds of old commits | Separate worktrees and targets on `~/scratch`; sequential, one at a time |
| The investigation drifts into a fix | No change to catalog, `sim_core/src` or expectations; proposals become questions |
| A cause is found in a shared rule and moves many battles | WP4 item 5 measures the whole matrix before any proposal |

---

## 6. Decisions for the owner

None now. WP5 files one question per proposed change, each defaulting to "no change", because Q0007 keeps the
baselines.
