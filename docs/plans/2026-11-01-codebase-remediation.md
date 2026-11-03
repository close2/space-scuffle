# Codebase Remediation Plan

**Status:** Proposed, awaiting owner decisions in §3. No code has been changed.
**Date:** 2026-11-01. **Audited:** working tree on top of `443052c`, including uncommitted changes.
**Line numbers** refer to that working tree and will drift. Search for the quoted symbol if they do.

---

## 1. Verdict

The code *looks* rigorous and is often well built at the small scale, but much of the game does not work.
Fixed-point math, the checksum, tick pipeline, uniform layouts and module docs are good. Above that layer,
basic RTS behaviour is broken or missing, and the verification that should have caught this mostly cannot.

| Gate | Result | What it really shows |
|---|---|---|
| `cargo test --workspace` | 788 passed, 0 failed, 0 ignored | Coexists with every bug in §4. Run 2026-11-01 |
| `clippy -D warnings` | Pass | Cast, length and dead-code lints are disabled crate-wide in `sim_core`, `sim_map`, `client` |
| `cargo fmt --check` | Pass | Genuine |
| GitHub CI | 3 runs, **all failed** | Last run 2026-10-26; 17 commits are unpushed; no protection exists |
| Cross-platform determinism | Claimed by the charter | Never run on anything but x86-64 Linux |
| 10,000+ ticks/s | Claimed by the charter | Measured below: **64 to 969 ticks/s** |

### Why 788 tests missed this

1. **Tests inject state instead of playing.** `sc2_who_wins_matchup_matrix_tests.rs` has 66 direct
   `order_queue` pokes and 0 `PlayerCommand`s.
2. **Tests skip silently.** Five video tests use a path that does not exist from their working directory,
   take a `return`, and report `ok`. No video is baked in `cargo test`.
3. **Some assertions cannot fail.** `collision_and_immobility_tests.rs:308,565` compares a dot product of
   magnitude ~0.06 against -0.3, so the anti-vibration tests pass however hard units vibrate.
4. **Some tests lock in the bug.** `sc2_battle_benchmarks_tests.rs:94` relies on a Hold Position unit dying
   passively.
5. **Fixes were verified against the plan's scenario, not the feature.** No test plays a whole game loop,
   so nothing notices that the player cannot give one unit an order.

### Effort is going to the wrong place

About 20,500 lines (42%) of `crates/client/src` are diagnostics, capture and reduction code that players never
run, compared with about 16,100 lines of runtime. Rounds 8 to 11 went to mist sub-pixel evidence while
`mist/` is switched off at runtime and combat, movement and input have the defects below.

---

## 2. Evidence levels

Every finding carries one of these, so nobody has to guess how far to trust it.

| Tag | Meaning |
|---|---|
| **[R]** | **Reproduced.** A real test failed, or a real measurement was taken, on 2026-11-01. See Appendix A. |
| **[V]** | **Verified.** The orchestrator read the source and confirmed it. Not executed. |
| **[A]** | **Audited.** Traced line by line by a review agent, not independently re-checked. Reproduce first. |

**Rule:** before fixing any [V] or [A] item, write the failing test. If it passes, the finding is wrong:
record that here and move on.

### Not reproduced or corrected

- **"Crowds beyond ~60 units never finish arriving": not reproduced.** 60 and 150 `pulse_bot`s on a flat
  64×64 map all completed within 6,000 ticks. The `1.5` vs planned `2.5` mismatch at `steering.rs:516` is
  real but is not shown to cause a failure. Do not change it without a failing case (ramps and chokes are
  untested).
- **"Hold Position units never fire"** applies to non-turret units. Turret units read `current_target`.
- **F-01** in the 2026-10-26 review is fixed for LiveBattle. The flat sandbox survives only for headless
  checks and tests.
- **Reviewed and clean:** no float reaches authoritative state; no `HashMap`, clock or thread use in sim
  crates; checksum covers persistent entity state; IDs do not alias; weapon period sums exactly; tick clock
  and interpolation; uniform layouts match WGSL; no secrets are tracked by git.

---

## 3. Decisions needed from the owner

These change the scope of the phases below. Each has a recommendation, which the plan assumes.

| # | Decision | Recommendation | Why |
|---|---|---|---|
| D1 | What is the near-term product? | **A playable skirmish**: select, command, fight, win or lose | Today it is a scripted battle viewer. This sets the scope of Phase 5 |
| D2 | Which fog system survives? | **Keep `smog/`, delete runtime `mist/`** | `mist/` is permanently disabled (C-4). Smog still needs C-2 and C-7 fixed |
| D3 | Keep the ~20,500 lines of diagnostics? | **Keep what still protects the game, in a separate crate. Delete reducer research** | The reducer was rejected (3,278 of 16,296 windows failed) and is not used at runtime |
| D4 | How to settle the uncommitted work? | **Commit as-is in coherent units, then fix forward** | It passes the suite, and a partial commit breaks the build (`lib.rs` references untracked modules) |
| D5 | Economy and tech: build or cut? | **Build a minimal version** (P5.5 to P5.7), or remove the orders | `Harvest`, `ReturnCargo`, research and construction do not function, which breaks charter §1.1 |
| D6 | Are procedural flipbooks allowed? | **Test fixtures only, never at runtime paths** | §1.2 forbids stand-ins, §4 and §5 require them |
| D7 | Required media scope? | **The units in shipped loadouts, then expand** | Only 4 of 91 catalog identities have real media; 81 have none and 6 are copies of one image |
| D8 | Charter §6 model rule | **Make it tool-neutral** | It names a Gemini model and is an uncommitted edit. This audit's agents ran on Claude, so it could not be followed literally |

---

## 4. Findings

Severity is impact on someone using the game or relying on the charter.

### 4.1 Combat and rules (`sim_core`)

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| S-1 | High | **[R]** | Hold Position units never fire: `_ => false`, and the weapon step reads targets only from `AttackTarget` | `world.rs:1960`, `:2507` |
| S-2 | High | **[R]** | Explicit focus fire is replaced by the nearest enemy within 3 ticks | `world.rs:1930`, `:2075` |
| S-3 | High | **[R]** | Anti-air-only structures shoot ground units: `weapon = ground.or(air)` | `catalog.rs:970`, `:177` |
| S-4 | High | **[V]** | Projectile weapons ignore `attacks_count`: one hit vs the instant path's loop, so `attacks_count: 2` projectile units are likely at half damage | `world.rs:2970` vs `:2677` |
| S-5 | High | [A] | Friendly-fire splash makes allies attack their own unit: no owner check found | `world.rs:1738`, `:1856` |
| S-6 | High | [A] | Windup fires at the current target, ignoring `Windup.target_id`, with no range recheck | `combat.rs:454`, `world.rs:2568` |
| S-7 | High | [A] | A sieged unit locks onto a target inside min range and stops firing | `world.rs:1949`, `combat.rs:355` |
| S-8 | High | **[V]** | `Build` is free, instant and unchecked, and its result is discarded. **Fixed** in `3a2865e` (5.4): transactional admission, payment on arrival, construction lifecycle | `world.rs:1139` |
| S-9 | High | **[V]** | **There is no income.** The only write to minerals is a subtraction; `Harvest` and `ReturnCargo` have no handler. **Fixed** in `3a2865e` (5.5); income through real input in `7a432f9` | `production/mod.rs:542` |
| S-10 | High | **[V]** | `research_upgrade` has no non-test caller. [A] Cost and time unread; not idempotent. **Fixed**: repeats rejected in `3052128` (3.9), research order and queue in `3a2865e` (5.6), card entry in `5482e95` | `world.rs:491` |
| S-11 | High | [A] | `Unlock*` upgrades, `sight_delta` and several overrides are never read | `world.rs:824` |
| S-12 | Med | [A] | Target dying in Windup burns a full cooldown | `combat.rs:458` |
| S-13 | Med | [A] | Ground splash hits air; stat bonus ignores falloff | `world.rs:2705`, `combat.rs:191` |
| S-14 | Med | [A] | `bonus_upgrade_per_level` only applies vs ARMORED | `combat.rs:67` |
| S-15 | Med | **[V]** | `firing_arc_deg: 360` wraps to arc 0, the strictest possible; 16 catalog entries use it. Per-shot cost not measured | `catalog.rs:732`, `math.rs:797`, `combat.rs:322` |
| S-16 | Med | [A] | Catalog validation allows crashes: `projectile_speed: 0` divides by zero on every peer | `validation.rs:213`, `world.rs:2618` |
| S-17 | Med | [A] | Attack path ignores cloak and invulnerability (latent: no shipped cloak) | `world.rs:2477` |
| S-18 | Med | [A] | Mode-transition copies have diverged; instant vs channel behave differently | `world.rs:1006`, `:1250` |
| S-19 | Med | [A] | Shielded structures never regenerate; `is_gas_extractor` ignored | `catalog.rs:1014` |
| S-20 | Med | [V] | `world.rs` is 3,205 lines; `step_weapons_and_projectiles` is ~630, with 3 splash copies that already disagree. **Fixed**: combat moved out in Phase 3 (3.1); the rest split in `8ac33ad` (9.1), leaving `world.rs` a 373-line module root | `world.rs:2458` |

### 4.2 Movement (`steering.rs`, `movement/`)

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| M-1 | Crit | **[R]** | Friendly units walk through each other: head-on centres reach 0.03 against contact distance 0.75. `goal_dir·velocity` is compared as if it were a cosine | `steering.rs:379`, `:174`; `collision.rs:43` |
| M-2 | Crit | **[R]** | Short orders on a stationary group are dropped on tick 1: 0.000 progress. Idle squadmates count as "settled" | `locomotion.rs:387`, `steering.rs:509` |
| M-3 | High | **[R]** | Unreachable orders never end: 20/20 still ordered after 500 ticks at 15.67 ms/tick. The `find_path` error is dropped and A* reruns | `locomotion.rs:349`, `:421` |
| M-4 | High | **[R]** | Enemies shove Hold Position units: 0.324 displacement. Hostile branch runs before the immobile check | `steering.rs:612`, `world.rs:2336` |
| M-5 | High | [A] | Corner waypoints cannot pop when a blocker sits on them: handoff 0.28 vs contact 0.75 | `locomotion.rs:361` |
| M-6 | High | [A] | Stuck units are invisible: velocity stays nonzero with zero displacement, no stuck detection | `locomotion.rs:593` |
| M-7 | High | [A] | Pursuit has no pathing: melee slides along cliffs instead of taking the ramp. Planned, never implemented | `locomotion.rs:562` |
| M-8 | High | [A] | Clearance is corner-anchored and asymmetric, so large units fail near +x/+y walls. Compounds M-3 | `pathfinding.rs:52`, `:479`, `:679` |
| M-9 | High | [A] | Ramp-corner slide trap is back: X is always tried before Y | `locomotion.rs:596`, `world.rs:2437` |
| M-10 | High | [A] | Route-retention fix is dead code in the live game: `vision_aware_pathfinding` is always true | `locomotion.rs:316`, `world.rs:241` |
| M-11 | High | [A] | Steering is O(N²) despite a documented spatial hash | `steering.rs:265`, `:352`, `:496` |
| M-12 | High | [A] | Smooth steering excludes final approach and pursuit, with a discontinuity at exactly 4 m | `locomotion.rs:440` |
| M-13 | Med | [A] | Buffed units lose speed on any friendly contact; cap uses base speed | `world.rs:2408` |
| M-14 | Med | [A] | Wedge still in `compute_push_displacement`; low-priority movers shove high-priority idlers | `steering.rs:699` |
| M-15 | Med | [A] | Unrelated idle friendlies and buildings end Move orders early | `locomotion.rs:399` |
| M-16 | Med | [A] | Exact head-on tie-break steers both units to the same world side | `steering.rs:296`, `:391` |
| M-17 | Med | [A] | Collision and steering ignore cliff separation | `world.rs:2236` |
| M-18 | Low | [A] | `from_f32_lossy` in tick loops, contrary to its docs; magic numbers throughout | `steering.rs:176` onward |

### 4.3 Foundations (`sim_types`, `sim_map`)

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| F-1 | High | **[R]** | Debug and release disagree: `length()` of (32768, 32768) panics in debug, returns 0 in release. Peers on different profiles diverge | `math.rs:358`, `:583` |
| F-2 | High | **[V]** | Watchtower capture is never called, so towers never grant vision. [A] Ownership never releases | `grid.rs:689`, `:709` |
| F-3 | Med | **[R]** | `normalize()` is wrong for small vectors: length 1.4142 | `math.rs:576`, `:605` |
| F-4 | Med | **[V]** | Ramp visibility branch is dead: `cliff_level > s` and `== s` cannot both hold | `vision.rs:248` |
| F-5 | Med | [A] | `validate_map`, the gate for untrusted files, converts before bounds-checking and can overflow | `validation.rs:622`, `:704`, `:951` |
| F-6 | Med | [A] | 160 of tenths 1..=200 fail round-trip, e.g. radius 12.3 is rejected | `map_format.rs:746`, `:851` |
| F-7 | Med | [A] | Small units are routed through zero-width diagonal gaps, so diagonal wall-offs do not work | `pathfinding.rs:590` |
| F-8 | Med | [A] | Legacy Tiled importer and `ScMapData` bypass all validation, with no production caller | `loader.rs` |
| F-9 | Med | [A] | New ramp fallback is roughly cubic and swallows errors | `pathfinding.rs:738` |
| F-10 | Med | [A] | Destroying a structure fabricates pathability | `grid.rs:679` |
| F-11 | Med | [A] | Ramp landing validation ignores pathability | `validation.rs:522`, `:560` |
| F-12 | Med | [A] | Sight radius silently capped at 16 | `vision.rs:21` |
| F-13 | Med | [A] | Faction names, armour classes and abilities in Rust enums, against charter §3.3. **Fixed** in `56f243f` (9.3): factions, weapon and armour classes are data in `assets/catalog/factions.ron`, checked by C5; ability kinds stay engine mechanics (Phase 9 decision D-7) | `enums.rs:8`, `upgrades.rs:47` |

### 4.4 Client runtime

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| C-1 | Crit | **[V]** | Selection lasts one frame: sync overwrites it from a snapshot where `selected` is always false, so rings and HP bars never show in battle. **Fixed** in `822c1b6` (5.1) | `scene_controller.rs:564`, `world.rs:3197` |
| C-2 | Crit | [A] | Smog destroys and recreates two 3D textures every frame, ~1.66 MB; the code comment claims the opposite | `smog/mod.rs:900`, `:169` |
| C-3 | High | **[V]** | No unit-level commands: no right-click outside the editor, `pick_terrain` unused, only the whole roster can be ordered. **Fixed** in `822c1b6` (5.2), context orders in `6bc1fab` | `lab_ui.rs:738` |
| C-4 | High | **[V]** | `mist/` is permanently off: smog `enabled` defaults true, so 14,677 lines do nothing at runtime while still allocating [A] | `mist/mod.rs:455` |
| C-5 | High | [A] | Every unit's material is re-prepared every frame | `scene_controller.rs:553` |
| C-6 | High | [A] | Smog startup is a chain of silent fallbacks against charter §1.2, with fabricated unit IDs | `smog/mod.rs:397`–`:454` |
| C-7 | High | [A] | Smog draws over units by a single box-centre sort key, likely a remaining cause of smog in front of units. Needs a capture | `smog/material.rs:108` |
| C-8 | Med | [A] | Visual Lab default state hides its own subject; several controls do nothing | `lab_ui.rs:167`, `:437` |
| C-9 | Med | [A] | Combat VFX buttons spawn ~22 units off-screen | `lab_ui.rs:509` |
| C-10 | Med | [A] | 50 of 200 swarm bubbles spawn inside the arena plateau | `scene_controller.rs:790` |
| C-11 | Med | [A] | Construction progress never reaches the shader; dome fill direction is also wrong. **Fixed**: progress published in `3a2865e`, fill by height in `d90e76e` (5.8); pixels on the live skirmish in `7a432f9` | `world.rs:3196`, `bubble_shader.rs:332` |
| C-12 | Med | [A] | Selection hits hidden entities and shared materials. **Fixed** in `822c1b6` (5.1); fogged enemies unpickable in `24c656f` | `selection.rs:153` |
| C-13 | Med | [A] | Smog: unordered systems, hardcoded 64×64 clamps, 16-cavity cap, velocity scale 20 vs 25 Hz tick | `smog/mod.rs:160`, `:686`, `:849`, `:805` |
| C-14 | Med | [A] | Camera snaps on cliffs; projectile cards go edge-on; melee rotated twice | `camera.rs:233`, `vfx.rs:318`, `:611` |
| C-15 | Med | [A] | Per-frame allocation and asset churn; `LabState` owns the authoritative world | `scene_controller.rs:106`, `lab_ui.rs:98` |
| C-16 | Med | [A] | Parallel implementations: `arena_render` vs `terrain/`, `bubble_shader` vs `bubble_optics`, and dead markers | `lib.rs:27` |

### 4.5 Map editor

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| E-1 | Crit | **[V]** | F5 Playtest panics and loses unsaved work: editor registers none of `BattleSession`, `PlayerPalette`, `PresentationStatus`; Bevy 0.15 defaults to panic. Confirmed in source, not run. [A] Even with them, the playtest world feeds no snapshot | `editor/app.rs:227` |
| E-2 | High | **[V]** | Window X bypasses the unsaved-changes guard: `close_when_requested` is never set | `bin/map_editor.rs:87` |
| E-3 | High | [A] | Releasing a stroke over the UI loses its undo entry | `editor/ui.rs:1459` |
| E-4 | High | [A] | Symmetric painting does not update the mirrored terrain | `editor/ui.rs:1480`, `tools.rs:130` |
| E-5 | High | [A] | Opening a different-size map leaves old chunks and garbled textures; `Err` swallowed | `terrain/mod.rs:73`, `editor/ui.rs:1130` |
| E-6 | Med | [A] | Discard clears `dirty` too early; Save never resumes the pending action | `editor/ui.rs:1393` |
| E-7 | Med | **[V]** | Ctrl+Shift+Z undoes: the Ctrl+Z branch runs first, so redo is unreachable | `editor/ui.rs:391` |
| E-8 | Med | [A] | Other logic faults: start-location symmetry, silent playtest errors, Eraser creates invalid cells, per-keystroke undo entries, `doc` marked changed every frame | `tools.rs:729`, `ui.rs:1253`, `:1589`, `:318` |
| E-9 | Med | [A] | Render verification can pass on stale PNGs from an earlier run | `verification.rs:152` |
| E-10 | Low | [A] | Plan gaps: drag ramps, entity moving, playtest HUD unreachable; panics on bad CLI input; 919-line `editor_ui_system` | `ui.rs:277`, `app.rs:117` |

### 4.6 Asset baker and media

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| B-1 | High | **[V]** | Six catalog units are byte-identical copies of `concept_video.png`, and players see them | `assets/textures/` |
| B-2 | High | [V] | API key is in curl's argv and `-L` can forward it to redirect hosts. [A] Derived `Debug` prints it | `veo_client.rs:501` |
| B-3 | High | [A] | HTTP error bodies are saved as `.mp4` and cached forever: no `--fail`, no temp file, size-only cache check | `veo_client.rs:501`, `:492` |
| B-4 | High | [A] | Veo polling can hang and loses paid operations on transient errors | `veo_client.rs:390` |
| B-5 | Med | [A] | `DEFAULT_PROJECT_ID = "000000000000"` is silently substituted; `/home/cl/...` paths are hardcoded, including key lookup | `veo_client.rs:42`, `:778` |
| B-6 | Med | [A] | Default bake silently keeps ~0.67 s of each 4 s clip | `ffmpeg_importer.rs:313` |
| B-7 | Med | [A] | Importer fabricates missing actions, defeating `require_action` | `ffmpeg_importer.rs:194`, `:427` |
| B-8 | Med | [A] | `procedural` can overwrite real art via fuzzy name matching | `procedural_generator.rs:61` |
| B-9 | Med | [A] | Veo accepts unknown units and unsanitised path components | `veo_client.rs:641`, `:672` |

### 4.7 Engineering system

| ID | Sev | Evidence | Defect |
|---|---|---|---|
| Q-1 | Crit | **[V]** | CI has never passed, and 17 commits are unpushed. [A] Last failure may be missing `libxkbcommon-x11` (suspected) |
| Q-2 | Crit | [A] | Determinism across platforms is unverified: no matrix, no wasm target, no golden `SimWorld` checksum |
| Q-3 | High | **[R]** | Performance claim is false. See Appendix A; nothing in the repo measures it |
| Q-4 | High | **[V]** | Clippy is hollow: crate-wide allows remove the lints this engine most needs |
| Q-5 | High | **[V]** | Video tests skip silently: `crates/asset_baker/tmp` does not exist. Other silent `return`s [A] |
| Q-6 | High | [A] | No property testing, despite charter §4's heading |
| Q-7 | High | [A] | Vulkan validation errors do not fail tests |
| Q-8 | High | [A] | Rust policy is false: `rust-version` 1.80 vs locked `fixed` needing 1.93; floating `stable` toolchain; `fixed` on a caret range |
| Q-9 | High | [A] | `DESIGN.md` is stale: 22.4/30 Hz vs 25, NavMesh + RVO vs grid A*, nonexistent crates, absolute `file:///` links. **Fixed** in `0336642` (9.5): rewritten with a per-subsystem status table, checked by C17 |
| Q-10 | Med | [A] | Shader "syntax" tests are `.contains()`; 66 plans with colliding numbers and no status index; 545 `/tmp` references, so evidence is ephemeral. **Fixed**: shader tests in Phase 1 (`bb406f9`); plan index, unique IDs and retention lines in `10d92c5` (9.6), checked by C18 and C19 |
| Q-11 | Med | **[V]** | `tools/renderer_compatibility/target` (1.3 GB) is unignored, a `git add -A` hazard; no `[profile]` section |
| Q-12 | Med | [A] | `asset_baker → sim_core` is not in charter §5; `sim_map` needs `serde_json` only for dead code. **Fixed**: `serde_json` left `sim_map` before Phase 9; the edge is drawn and §5 is checked against Cargo by C1 in `0336642` (9.5) |

---

## 5. Phases

Order matters. Phases 0 and 1 make later claims trustworthy. Phases 2 to 4 are ordered so that movement is
not tuned on broken math and clearance. Phases 7 and 8 can run in parallel with anything after Phase 1.
Size: S under a day, M a few days, L a week or more.

**Rules for every phase**

- Failing test first, through `PlayerCommand` where the behaviour is player-visible.
- Each commit fixes one finding and names its ID.
- Simulation changes alter checksums. Update a golden only in a commit that explains why it changed.
- A phase is done when its exit criteria pass on **CI**, not locally.
- Update this file's finding table: mark items fixed, or refuted with the reason.

### Phase 0: Establish a real baseline (M)

| Step | Action |
|---|---|
| 0.1 | Fix `.gitignore` to unrooted `target/` (Q-11) |
| 0.2 | Settle the tree per D4, in order: sim changes, smog, editor, docs. Run the gate before each commit |
| 0.3 | Fix CI: add the X11 runtime library, use `--locked`, get one green run, then push |
| 0.4 | Pin the exact Rust version and `fixed = "=1.31.0"`; set `rust-version` truthfully (Q-8) |
| 0.5 | Add profiles: `[profile.dev.package."*"] opt-level = 2`, `[profile.test] debug = "line-tables-only"` |
| 0.6 | Resolve D1 to D8 and record the outcomes here |

**Exit:** green CI on `main`; clean working tree; CI required on the default branch.

### Phase 1: Make verification truthful (L)

| Step | Finding | Action |
|---|---|---|
| 1.1 | Q-5 | Commit a tiny fixture clip. Replace every silent `return` with a failure or an explicit, CI-rejected opt-out |
| 1.2 | M-tests | Fix the impossible vibration assertions by normalising both vectors. Add the missing assertions: squad reaches target, melee reaches enemy, `world.diagnostics()` empty |
| 1.3 | Q-2 | Add a replay fixture with per-N-tick golden checksums. Run on Linux, Windows, macOS ARM64, and `wasm32-wasip1` via wasmtime |
| 1.4 | Q-3 | Add a release benchmark for Appendix A's scenarios. Publish the numbers; correct the charter claim until it is met |
| 1.5 | Q-6 | Add `proptest` to `sim_types` and `sim_map`: arithmetic, angle wrap, funnel invariants |
| 1.6 | Q-10 | Replace `.contains()` shader tests with naga parse and validate |
| 1.7 | Q-7 | Fail on unlisted Vulkan validation IDs, with a documented allowlist |
| 1.8 | Q-4 | Remove crate-wide allows from `sim_core` and `sim_map`; replace with `#[expect(..., reason = "...")]` per site. Add `clippy::unreachable`. Replace casts with checked conversions. `client` follows in Phase 6 |
| 1.9 | new | Add one end-to-end test that plays a battle through `PlayerCommand` only, to game end. **Done**: `full_match_tests.rs` in `9111d8e`; through input events alone, `skirmish_match_tests.rs` in `7a432f9` |

**Exit:** the suite cannot be green with skipped real work; determinism runs on all four targets, or the
charter is corrected. Any platform divergence becomes a top-priority finding.

### Phase 2: Simulation foundations (M)

| Step | Finding | Action |
|---|---|---|
| 2.1 | F-1 | Give `SimScalar` one explicit, profile-independent overflow policy. Recommended: saturating operators, plus `checked_*` for callers that can act on failure. Check cost with the 1.4 benchmark |
| 2.2 | F-1, F-3 | Compute length, distance and normalise on exact `i128` raw squares. Remove the `Err → ZERO` fallback |
| 2.3 | F-5, S-16 | Validate bounds before converting. Complete catalog bundle validation at load |
| 2.4 | M-8 | Replace corner-anchored clearance with a centre-symmetric distance field based on real radius. Do this before Phase 4 |
| 2.5 | F-7, F-9–F-11 | Block zero-width diagonals; rewrite the ramp fallback with precomputed corridor indices; preserve base cell flags; validate landing pathability |
| 2.6 | F-2, F-4, F-12 | Wire watchtowers into the tick with release on vacancy; implement ramp vision by progress; reject oversized sight radii at load |
| 2.7 | F-6, F-8 | Fix tenths round-trip by rounding; delete `loader.rs` importer, `ScMapData` and `serde_json` unless needed |
| 2.8 | S-3, S-15 | Fix `weapon_against` fallback; treat arc ≥ 180° as unrestricted |

**Exit:** F-1, F-3 and S-3 repros pass; property tests pass; no silent fallback in `SimVec2`.

### Phase 3: Combat correctness (L)

Start by extracting one `apply_weapon_hit` and one splash path from `step_weapons_and_projectiles`
(S-20), with no behaviour change. The next fixes then land in one place.

| Step | Finding | Action |
|---|---|---|
| 3.1 | S-20 | Extract `combat_resolution` with one damage and splash path. Characterise current behaviour by test first |
| 3.2 | S-4, S-13, S-14 | Apply `attacks_count` to projectiles; filter splash by layer; scale bonuses correctly |
| 3.3 | S-1 | Hold Position acquires in-range targets without pursuit. Update the test that relies on passive holders |
| 3.4 | S-2 | Add an explicit vs auto marker to `AttackTarget`, with hysteresis for auto targets |
| 3.5 | S-5 | Do not retaliate against or alert on friendly damage |
| 3.6 | S-6, S-7, S-12 | Bind shots to `Windup.target_id` with a range recheck; retarget inside min range; no cooldown on a cancelled swing |
| 3.7 | S-17 | One availability gate at acquire, fire, hit and splash |
| 3.8 | S-18, S-19 | One `apply_mode_transition`; authored structure regen |
| 3.9 | S-10, S-11 | Reject repeated upgrades. Read or reject at load every unread upgrade field. Research wiring is in Phase 5 |

**Exit:** S-1 to S-3 repros pass; matchup matrix moved to data, run through `PlayerCommand`, on several seeds,
then re-baselined with each change explained.

### Phase 4: Movement (L)

Treat this as a redesign of the arrival and contact rules, not another tuning round. Earlier rounds patched
the same area repeatedly, and several fixes regressed or never ran (M-9, M-10).

| Step | Finding | Action |
|---|---|---|
| 4.1 | M-1, M-16 | Compare normalised velocities in co-movement; let contact cancel inward velocity; use a symmetric tie-break |
| 4.2 | M-2, M-15 | Replace velocity-inferred arrival with an explicit group or order identity and arrival state. Exclude buildings |
| 4.3 | M-3 | Propagate path errors as typed outcomes; stop or retarget to the nearest reachable cell; add a repath cooldown |
| 4.4 | M-4, M-14 | Check `cannot_be_pushed` first; delete the wedge; move only the pushable side |
| 4.5 | M-5, M-6, M-9 | Pop waypoints by passing, not distance; store achieved velocity; add no-progress detection; try the dominant axis first via one shared slide helper |
| 4.6 | M-7 | Route pursuit over terrain, with throttled repaths |
| 4.7 | M-10, M-12 | One continuous steering function, no mode split at 4 m; make the retention fix reachable |
| 4.8 | M-11 | Query a tick-start spatial grid; add reusable scratch buffers |
| 4.9 | M-13, M-17, M-18 | Use effective speed for caps; require terrain connectivity for contact; named constants or data-driven tuning |

**Exit:** M-1 to M-4 repros pass; new tests for head-on, crossing, chokes, ramps, blocked waypoints and
short orders; 400-unit benchmark at least 2× Appendix A.

### Phase 5: Playable game loop (L, depends on D1 and D5)

| Step | Finding | Action | Status |
|---|---|---|---|
| 5.1 | C-1, C-12 | Keep selection client-side, applied in sync; respect visibility; handle misses | Done, `822c1b6` |
| 5.2 | C-3 | Right-click move and attack through `BattleCommandQueue`, owner-filtered | Done, `822c1b6`; context orders `6bc1fab` |
| 5.3 | new | Box select, control groups, Hold and Stop bindings | Done, `822c1b6` |
| 5.4 | S-8 | Transactional `Build` admission with cost, tech, builder and typed outcome, plus a construction lifecycle | Done, `3a2865e`; knowledge-based admission `866563f`; card and placement `5482e95` |
| 5.5 | S-9 | Harvester state machine and income; retype `Harvest.target` | Done, `3a2865e`; right click on a node `6bc1fab` |
| 5.6 | S-10 | Research order and queue, reusing the production transaction | Done, `3a2865e`; card `5482e95`; stim admission by unlock `ced793b` |
| 5.7 | new | Production cancel and refund; load and unload orders; win and loss condition | Done, `3a2865e`; queue panel `5482e95`; result screen `760532a` |
| 5.8 | C-11 | Publish construction progress; fix dome fill | Done, `3a2865e`, `d90e76e`, `f093116` |

**Exit:** a person can play a match to a result using only mouse and keyboard, and 1.9 covers it.

**Exit status (2026-11-03): met; CI green on `main` at `7c947ad` (run 41583874524, all five jobs).** `skirmish_match_tests.rs` (`7a432f9`) wins the shipped
skirmish headless through input events alone, with a pilot that reads only the screen, and proves the HUD, the
placement and the dome fill on a real window under Vulkan validation. It runs as `cargo run -p client -- --skirmish`.
Gas, research and transport are built and tested on temporary fixture roots, and disabled in the shipped game for want
of media, per `docs/questions/open/0001-media-scope.md`. Plan and evidence:
`docs/plans/2026-11-02-phase-5-playable-client.md`, `docs/evidence/2026-11-03-result-window-victory.md`.

### Phase 6: Client performance and structure (L)

| Step | Finding | Action |
|---|---|---|
| 6.1 | C-2, C-5 | Persistent smog textures with `queue.write_texture`; dirty checks before `get_mut`. Measure before and after |
| 6.2 | C-4, D2 | Remove the losing fog system; move the depth prepass to camera setup |
| 6.3 | C-6, C-7, C-13 | Typed smog failures; correct compositing with a capture to prove it; map-derived bounds |
| 6.4 | D3 | Create `client_diagnostics`; move capture and audit there; delete rejected reducer code; remove `ControlledFixture` branches from production paths |
| 6.5 | C-8 to C-10, C-14 | Fix Lab defaults and dead controls, off-screen VFX, swarm placement, camera damping, card orientation |
| 6.6 | C-15, C-16 | Move the world from `LabState` to `BattleSession`; remove arena, dead markers and aliases; then drop `client` crate-wide allows |

**Exit:** no per-frame heap allocation or asset churn in sync systems, proved by test; one owner per concern;
frame budget measured at 200 and 400 units.

### Phase 7: Editor hardening (M, parallel)

| Step | Finding | Action |
|---|---|---|
| 7.1 | E-1 | Add an App-level test that enters Playtest. Register resources and feed the session from the editor world. Autosave before entry |
| 7.2 | E-2, E-6 | Handle window close in every phase; correct `dirty` handling; resume pending actions after save |
| 7.3 | E-3, E-7 | Finish strokes outside the pointer guard; block undo while painting; fix redo order |
| 7.4 | E-4, E-5 | Sync the rects actually touched; one `load_document` that rebuilds chunks and surfaces errors |
| 7.5 | E-8 to E-10 | Remaining logic faults; verification on fresh output; split `editor_ui_system` |

**Exit:** edit, save, playtest, return and reopen is covered by an automated App test.

### Phase 8: Asset pipeline and media (M, parallel)

| Step | Finding | Action |
|---|---|---|
| 8.1 | B-2 | Pass the key through `curl -K -` or a native client; allowlist hosts; follow redirects without the key; redact `Debug` |
| 8.2 | B-3, B-4 | `--fail-with-body`, temp file, validate with ffprobe, rename; timeouts, bounded retries, persisted operation names |
| 8.3 | B-5, B-9 | Typed error for a missing project; remove `/home/cl` paths; exact catalog slugs only |
| 8.4 | B-6, B-7, B-8 | Resample the whole clip or fail; never fabricate actions; procedural output outside runtime paths, per D6 |
| 8.5 | B-1, D7 | Remove the six copied atlases; generate real media for the required set; make `audit-media --strict` pass for that set |

Generating media spends real money and needs the owner's approval per run.

**Exit:** strict audit passes for the agreed set; a failed download cannot enter the cache.

### Phase 9: Structure and documentation (M)

| Step | Finding | Action | Status |
|---|---|---|---|
| 9.1 | S-20 | Split the rest of `world.rs`: abilities, vision, collision | Done, WP1 `8ac33ad` |
| 9.2 | large functions | Split `validate_map`, `build_chunk_mesh` and `sync_*`. Enable `too_many_lines` | Done: threshold and guard rails WP0 `808b6dd`; splits WP2 `b3e47ae`, WP3 `bdb3abf`, WP4 `ec55bf2`, WP5 `554bb81`, WP6 `821c6b0`, WP7 `295f1d9`, WP8 `ed7aa8e`; last suppressions removed and C8 in `0336642` |
| 9.3 | F-13 | Move faction and armour identity into catalog data | Done, WP12 `56f243f` |
| 9.4 | F-5 legacy, F-7 legacy | Close the open 2026-10-26 items: sandbox `let _`, `.min(7)`, compatibility `include_str!`, typed `PlayerId` | Done: WP10 `2797d4a` (F-05, F-08), WP11 `331ccb4` (F-07) |
| 9.5 | Q-9, Q-12 | Rewrite `DESIGN.md` with per-subsystem status; align charter §4 and §5 | Done, WP13 `0336642` |
| 9.6 | Q-10 | One plan status index; fix numbering; move kept evidence out of `/tmp` | Done, WP9 `10d92c5` |

**Exit:** the charter describes the code as it is, and every claim in it is checked by CI.

**Exit status (2026-11-03): met; CI green on `main` at `6b39cd2` (run 41665216936, all six jobs including `throughput`).** Every rule of `AGENTS.md`
now carries a tag: a CI check, a review rule or a procedure (charter §9). The checks are clippy lints, the CI jobs
(`check-and-test`, the determinism jobs and the new `throughput` job) and C1 to C21 in
`crates/workspace_checks/tests/charter.rs`. Claims that no check covers were reworded as review rules: the client's
read discipline (§3.2), the steady-state allocation claim of §3.3 until Q0011 is answered, documentation quality
and when a performance divergence is justified. Charter §6 is unchanged (Q0003). The owner can object to any
charter change in Q0010. Gaps found during the phase and left for later plans are listed in §9 of
`docs/plans/2026-11-03-phase-9-structure-and-docs.md` and in `DESIGN.md` §4. Plan:
`docs/plans/2026-11-03-phase-9-structure-and-docs.md`.

---

## 6. Explicitly out of scope until Phase 5 exits

- More mist or smog art-direction rounds.
- Shell-ordering work (Plan 53B) and presentation discontinuities (Plan 55).
- New units, structures or abilities.
- Networking, replay and bot API, which have no implementation yet.

A full rewrite is not recommended. The foundations are sound, and the failures are concentrated and fixable.

---

## Appendix A: Reproduction results

Run 2026-11-01 from a scratch crate depending on the real sim crates. Each test asserts the *correct*
behaviour, so a failure proves the bug. The file below is the complete suite. To run it, create a crate with
path dependencies on `sim_core`, `sim_map` and `sim_types`, copy `Cargo.lock` and `rust-toolchain.toml`, and
run `cargo test --release --test repro -- --nocapture --test-threads=1`.

| Test | Finding | Result |
|---|---|---|
| r01 hold fires | S-1 | **FAIL**: enemy at distance 2.0 undamaged after 200 ticks |
| r02 idle fires | control | pass, so r01 is not a setup fault |
| r03 head-on | M-1 | **FAIL**: min centre distance 0.030, contact is 0.75 |
| r04 short order | M-2 | **FAIL**: 0.000 progress in 100 ticks |
| r05 focus fire | S-2 | **FAIL**: order replaced by the nearer enemy |
| r06 anti-air | S-3 | **FAIL**: `flak_array_dome` damaged a ground unit |
| r07 hold shove | M-4 | **FAIL**: holder displaced 0.324 |
| r08 crowd of 60 | steering arrival | pass, not reproduced |
| r13 crowd of 150 | steering arrival | pass, not reproduced |
| r09 unreachable | M-3 | **FAIL**: 20/20 still ordered, 15.67 ms/tick |
| r10 large length | F-1 | **FAIL**: release returns 0; debug panics with add overflow |
| r11 small normalise | F-3 | **FAIL**: 1.4142 |

Throughput, release build, one run on the audit host, attack-move scenario (r12):

| Map | Units | Ticks/s | Against 10,000 |
|---|---:|---:|---:|
| 64×64 | 0 | 969 | 10× short |
| 64×64 | 100 | 490 | 20× short |
| 64×64 | 400 | 112 | 89× short |
| 128×128 | 400 | 81 | 123× short |
| 256×256 | 0 | 64 | 156× short |

All still exceed real time at 25 Hz. The empty-map rows show cost that scales with map area, not units, which
fits the per-tick full-state checksum and vision scan [A]. Profile before optimising.

```rust
use sim_core::SimWorld;
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::*;

fn s(v: f64) -> SimScalar { SimScalar::from_num(v) }
fn v(x: f64, y: f64) -> SimVec2 { SimVec2::new(s(x), s(y)) }
fn flat(n: u32) -> SimWorld {
    SimWorld::new(GameMap::new(n, n, MapCell::default_ground(CliffLevel(0))).unwrap(), 7)
}
fn unit(w: &mut SimWorld, slug: &str, p: u8, x: f64, y: f64) -> EntityId {
    let id = w.catalog.id_by_slug(slug).unwrap();
    w.spawn_unit(id, PlayerId(p), v(x, y)).unwrap()
}
fn order(w: &mut SimWorld, t: &mut u32, p: u8, es: Vec<EntityId>, o: UnitOrder) {
    let b = TickBundle { tick: Tick(*t), commands: vec![PlayerCommand { player_id: PlayerId(p), entities: es, order: o, queue: false }], state_checksum: None };
    w.step(&b); *t += 1;
}
fn run(w: &mut SimWorld, t: &mut u32, n: u32) {
    for _ in 0..n { w.step(&TickBundle { tick: Tick(*t), commands: vec![], state_checksum: None }); *t += 1; }
}
fn dist(w: &SimWorld, a: EntityId, b: EntityId) -> f64 {
    let (p, q) = (w.entities[&a].position, w.entities[&b].position);
    let (dx, dy) = ((p.x - q.x).to_f32() as f64, (p.y - q.y).to_f32() as f64);
    (dx * dx + dy * dy).sqrt()
}

#[test] fn r01_hold_position_fires_at_enemy_in_range() {
    let mut w = flat(32); let mut t = 0;
    let a = unit(&mut w, "pulse_bot", 0, 10.0, 10.0);
    let b = unit(&mut w, "rhino_beetle", 1, 12.0, 10.0);
    let hp0 = w.entities[&b].hp;
    order(&mut w, &mut t, 0, vec![a], UnitOrder::HoldPosition);
    run(&mut w, &mut t, 200);
    assert!(w.entities.get(&b).map_or(true, |e| e.hp < hp0), "holder never damaged enemy 2.0 away (hp unchanged)");
}

#[test] fn r02_control_idle_unit_does_fire() {
    let mut w = flat(32); let mut t = 0;
    let _a = unit(&mut w, "pulse_bot", 0, 10.0, 10.0);
    let b = unit(&mut w, "rhino_beetle", 1, 12.0, 10.0);
    let hp0 = w.entities[&b].hp;
    run(&mut w, &mut t, 200);
    assert!(w.entities.get(&b).map_or(true, |e| e.hp < hp0));
}

#[test] fn r03_head_on_friendlies_do_not_interpenetrate() {
    let mut w = flat(64); let mut t = 0;
    let a = unit(&mut w, "pulse_bot", 0, 20.0, 30.0);
    let b = unit(&mut w, "pulse_bot", 0, 40.0, 30.0);
    order(&mut w, &mut t, 0, vec![a], UnitOrder::Move { target: v(40.0, 30.0) });
    order(&mut w, &mut t, 0, vec![b], UnitOrder::Move { target: v(20.0, 30.0) });
    let mut min = 99.0f64;
    for _ in 0..500 { run(&mut w, &mut t, 1); min = min.min(dist(&w, a, b)); }
    println!("head-on min centre distance = {min:.3}");
    assert!(min > 0.5, "contact distance 0.75; min centre distance {min:.3}");
}

#[test] fn r04_short_move_order_on_stationary_pair_moves() {
    let mut w = flat(32); let mut t = 0;
    let a = unit(&mut w, "pulse_bot", 0, 10.0, 10.0);
    let b = unit(&mut w, "pulse_bot", 0, 10.8, 10.0);
    order(&mut w, &mut t, 0, vec![a, b], UnitOrder::Move { target: v(10.4, 11.5) });
    run(&mut w, &mut t, 100);
    let (pa, pb) = (w.entities[&a].position, w.entities[&b].position);
    let moved = f64::max((pa.y - s(10.0)).to_f32() as f64, (pb.y - s(10.0)).to_f32() as f64);
    println!("short-order max progress = {moved:.3}");
    assert!(moved > 0.5, "units barely moved: {moved:.3}");
}

#[test] fn r05_explicit_focus_fire_survives_nearby_enemy() {
    let mut w = flat(32); let mut t = 0;
    let a = unit(&mut w, "pulse_bot", 0, 10.0, 10.0);
    let near = unit(&mut w, "psi_grey", 1, 12.0, 10.0);
    let far = unit(&mut w, "rhino_beetle", 1, 18.0, 10.0);
    run(&mut w, &mut t, 2); // establish vision
    order(&mut w, &mut t, 0, vec![a], UnitOrder::AttackTarget { target: far });
    run(&mut w, &mut t, 3);
    let front = w.entities[&a].order_queue.front().cloned();
    println!("front after focus order = {front:?} (far={far:?} near={near:?})");
    assert!(matches!(front, Some(UnitOrder::AttackTarget { target }) if target == far));
}

#[test] fn r06_anti_air_structure_ignores_ground() {
    let mut w = flat(32); let mut t = 0;
    let dome = w.catalog.id_by_slug("flak_array_dome").unwrap();
    w.spawn_structure(dome, PlayerId(0), 10, 10).unwrap();
    let b = unit(&mut w, "rhino_beetle", 1, 16.0, 11.0);
    let hp0 = w.entities[&b].hp;
    run(&mut w, &mut t, 300);
    assert!(w.entities.get(&b).is_some_and(|e| e.hp == hp0), "air-only dome damaged a ground unit");
}

#[test] fn r07_enemy_cannot_shove_hold_position() {
    let mut w = flat(32); let mut t = 0;
    let h = unit(&mut w, "pulse_bot", 0, 15.0, 15.0);
    let m = unit(&mut w, "rhino_beetle", 1, 10.0, 15.0);
    order(&mut w, &mut t, 0, vec![h], UnitOrder::HoldPosition);
    order(&mut w, &mut t, 1, vec![m], UnitOrder::Move { target: v(25.0, 15.0) });
    let p0 = w.entities[&h].position; let mut d = 0.0f64; let mut ticks = 0;
    for _ in 0..300 { run(&mut w, &mut t, 1); let Some(e) = w.entities.get(&h) else { break }; ticks += 1;
        d = d.max(f64::hypot((e.position.x - p0.x).to_f32() as f64, (e.position.y - p0.y).to_f32() as f64)); }
    println!("holder max displacement {d:.3} over {ticks} ticks");
    assert!(d < 0.01, "HoldPosition unit displaced {d:.3}");
}

#[test] fn r13_crowd_of_150_finishes_move() {
    let mut w = flat(64); let mut t = 0;
    let mut ids = vec![];
    for i in 0..150 { ids.push(unit(&mut w, "pulse_bot", 0, 5.0 + (i % 15) as f64, 5.0 + (i / 15) as f64)); }
    order(&mut w, &mut t, 0, ids.clone(), UnitOrder::Move { target: v(40.0, 40.0) });
    run(&mut w, &mut t, 6000);
    let busy = ids.iter().filter(|i| !w.entities[i].order_queue.is_empty()).count();
    println!("150 units: after 6000 ticks, {busy}/150 still have an order");
    assert_eq!(busy, 0);
}

#[test] fn r08_crowd_of_60_finishes_move() {
    let mut w = flat(64); let mut t = 0;
    let mut ids = vec![];
    for i in 0..60 { ids.push(unit(&mut w, "pulse_bot", 0, 10.0 + (i % 10) as f64, 10.0 + (i / 10) as f64)); }
    order(&mut w, &mut t, 0, ids.clone(), UnitOrder::Move { target: v(40.0, 40.0) });
    run(&mut w, &mut t, 6000);
    let busy = ids.iter().filter(|i| !w.entities[i].order_queue.is_empty()).count();
    println!("after 6000 ticks, {busy}/60 still have an order");
    assert_eq!(busy, 0);
}

fn walled(n: u32) -> GameMap {
    let mut m = GameMap::new(n, n, MapCell::default_ground(CliffLevel(0))).unwrap();
    let cx = n / 2;
    for y in 0..n {
        let idx = (y * n + cx) as usize;
        m.cells_mut()[idx].flags = CellFlags::empty();
    }
    m
}

#[test] fn r09_unreachable_order_terminates_or_reports() {
    let mut w = SimWorld::new(walled(64), 7); let mut t = 0;
    let mut ids = vec![];
    for i in 0..20 { ids.push(unit(&mut w, "pulse_bot", 0, 5.0 + (i % 5) as f64 * 1.5, 5.0 + (i / 5) as f64 * 1.5)); }
    order(&mut w, &mut t, 0, ids.clone(), UnitOrder::Move { target: v(50.0, 50.0) });
    let start = std::time::Instant::now();
    run(&mut w, &mut t, 500);
    let ms = start.elapsed().as_secs_f64() * 1000.0 / 500.0;
    let busy = ids.iter().filter(|i| !w.entities[i].order_queue.is_empty()).count();
    println!("unreachable: {busy}/20 still ordered after 500 ticks, {ms:.2} ms/tick");
    assert_eq!(busy, 0);
}

#[test] fn r10_length_of_large_vector_is_not_zero() {
    let l = v(32768.0, 32768.0).length();
    println!("length = {l:?}");
    assert!(l > SimScalar::ZERO);
}

#[test] fn r11_normalize_small_vector_is_unit() {
    let raw = SimScalar::from_raw(92681);
    let n = SimVec2::new(raw, SimScalar::ZERO).normalize();
    println!("normalize small = {n:?}");
    let l = n.unwrap().x.to_f32() as f64;
    assert!((l - 1.0).abs() < 0.01, "normalised x = {l}");
}

fn bench(n: u32, units: usize, ticks: u32) -> f64 {
    let mut w = flat(n); let mut t = 0;
    let (mut a, mut b) = (vec![], vec![]);
    for i in 0..units { let (c, r) = ((i % 20) as f64, (i / 20) as f64);
        if i % 2 == 0 { a.push(unit(&mut w, "pulse_bot", 0, 5.0 + c, 5.0 + r)); }
        else { b.push(unit(&mut w, "pulse_bot", 1, n as f64 - 6.0 - c, n as f64 - 6.0 - r)); } }
    order(&mut w, &mut t, 0, a, UnitOrder::AttackMove { target: v(n as f64 - 8.0, n as f64 - 8.0) });
    order(&mut w, &mut t, 1, b, UnitOrder::AttackMove { target: v(8.0, 8.0) });
    let st = std::time::Instant::now();
    run(&mut w, &mut t, ticks);
    ticks as f64 / st.elapsed().as_secs_f64()
}
#[test] fn r12_throughput_report() {
    for (n, u) in [(64, 0), (64, 100), (64, 400), (128, 400), (256, 0)] {
        println!("map {n}x{n}, {u} units: {:.0} ticks/s", bench(n, u, 300));
    }
}
```
