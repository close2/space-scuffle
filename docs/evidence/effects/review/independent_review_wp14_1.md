# Independent Review of WP14 (the Showcase's Effects Take; Every Effect in the Frame Budget), Round 1

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP14 row of
§9, §10.3, Amendment 31. Packet: [`independent_review_packet.md`](independent_review_packet.md), section "WP14".
Owner rule for these reviews: Q0080. Branch `effects-viz-wp14`, head `a85dfa33`. I did not build this work.

## Method

I read the charter, the packet's WP14 section, the WP14 row, §10.3 and Amendment 31, the whole
[`frame_budget_wp14.md`](frame_budget_wp14.md) and, for comparison, [`frame_budget_wp6c.md`](frame_budget_wp6c.md) and
the recorded baseline (`docs/evidence/2026-11-03-frame-budget.md`). I read `git diff main...HEAD` of `crates/`,
`assets/`, `DESIGN.md` and `.github/` in full: `sim_core::scenario` (`InitialOrder::EnterModes`, `enter_modes`,
`ScenarioError::NoModeSwitches`) and its two new tests; `frame_budget/effect_mix.rs` (all 888 lines),
`effect_stress.rs` (the air-burst stand-in, `publish_counters`), `report.rs`; the showcase's `storyboard.rs`
(`Scene::effects`, `UnknownEffect`), `take.rs` (`close_scene_effects`), `recorder.rs`; `showcase.ron`,
`showcase_effects.ron`, `start_loadouts.ron`, the `effects.ron` budget, `suites.ron`; and the three test files.
To judge the pool size I read `effects/pool.rs` (how slots are walked) and to judge the CI risk
`.github/workflows/showcase.yml` (timeouts, what gates).

Pictures: the four `wp14_*` JPGs at 1:1, plus my own frames cut from the radv mezzanine
`~/scratch/effects-viz-wp14/showcase_radv/takes/effects.mkv` (pre-trim, `siege` 300 frames): frames 30, 60, 90, 175,
200, 240 at 1280x720, and a 3x4 strip of frames 24 to 90 (every 6th) cropped to the siege line, 400x180 at 1:1.
Crops are in `~/scratch/review-wp14/` (`entrench_strip.png`, `eff_*.png`, `seq7.png`, `seq15.png`, `rainbow.png`).

Commands: none completed (see "Checks run"). No GPU command and no gate.

## Verdict table

| Question | Verdict | Main reason |
|---|---|---|
| `fx_energy` not generated, nothing spent | **sound** | Every look `fx_energy` was listed for was built and accepted from the four shipped sets; a texture with no consumer would be spend without review. Within the plan's intent (§6.3 serves looks, not a quota) and §1.2 |
| Baneling beat deferred | **sound** | The row's own condition ("once those identities have media"); `volatile_tick` has no clips, and faking its presentation would break §1.2. `acid_burst` is in the frame-budget mix and the crowd frame |
| A second take with its own battle | **sound** | The only way to add siege, flame and spines without changing the `battle` take's fight and winner (D-1). The battle take's scene ticks are unchanged (packet) |
| `InitialOrder::EnterModes` | **sound; two doc defects (W14-3, W14-4)** | Capability read from `AbilityTargetKind::ToggleMode`, no identity (C5); typed `NoModeSwitches`; `world.entities` is a `BTreeMap` and each unit's order is independent of the others and of the RNG; no shipped scenario but the new one names it, so no golden, checksum or matrix result can move |
| Effect pool 2 048 → 6 144 | **acceptable (W14-7)** | Data-first; no Critical overflow at any size; the baseline arm B was measured at 6 144 and is the fastest ever recorded. The justification should say what 2 048 refused |
| Frame budget | **acceptable as a recorded follow-up, but the attribution is wrong (W14-1, condition)** | The plan's stress meets the literal bars; "every effect" is beyond the plan. The +3 ms main-world update is already present with the splash stream alone in the evidence's own runs, so the open item targets the wrong cost |
| CI render time | **no pre-merge mitigation needed (W14-8)** | The render step has no hard timeout, the job has 90 min, the workflow never gates CI, and the estimate is conservative |
| `entrench` composition | **does not read (W14-2, condition)** | The siege line is packed behind its own town hall; none of the four asked-for looks is readable at 1:1 in any frame I sampled |
| `siege` composition | **acceptable (W14-9)** | The flame jet reads clearly; the spine line reads as a travelling row of spikes in the sequence; shell impacts read as dust bursts. Some overlap and caption crowding |
| Crowd frame, checklist item 6 | **readable, poorly framed (W14-6)** | Units stay distinct bubbles; but 28 % of the frame is off-map black and the fight is cut by the top edge |
| Code and charter rules | **met, minor items (W14-5, W14-10)** | No `unwrap`, no `expect` in production code, `//!` and `///` everywhere, every function well under 100 lines, retained buffers in the per-frame mix, C22 triggers extended |
| **Overall** | **ACCEPT WITH CONDITIONS** | Scope and engineering are sound. Two conditions are data and text only: re-stage `entrench` so its looks are seen, and correct the frame-budget attribution and register the follow-up |

## Findings

### W14-1 (Medium, condition): the frame-budget cost is the splash stream's, not the areas' and held looks'

Amendment 31 and `frame_budget_wp14.md` say the rise in main-world update over today's baseline (B 9.6 ms, P 12.8,
M 14.9 at p50) is "the CPU side of 20 live areas, 50 held looks and their emitted children", and the open item says
to profile with `--effect-mix` and take `area_sight` first. The evidence's own runs do not support that. The S arm
(splashes only, no areas, no held looks) was measured twice at moderate load next to a B run:

| Pair | Load | B p50 / update p50 | S p50 / update p50 | S over B, update |
|---|---|---|---|---|
| DB-01 / DS-01 (both with timestamp queries) | 7 to 10 | 12.30 / 9.64 | 15.55 / 13.25 | **+3.6 ms** |
| B-03 / S-03 | 9 to 42 | 11.90 / 9.76 | 16.80 / 13.87 | **+4.1 ms** |
| B median / P median (of record) | 4 to 14 | 11.86 / 9.59 | P: 14.97 / 12.84 | +3.3 ms |

So the splash stream alone costs about as much CPU as the plan's whole stress; adding the 20 areas and 50 held looks
adds little or nothing on top. In WP6c's quietest triple the same 200 splashes a second cost +1.1 % at p50 and moved
the update by nothing (8.44 → 8.41 ms), with three splash looks. Since then the vocabulary gained seven splash looks
with heavier Critical ground marks (flame bands, lance grooves, cleave fronts), the decal pool went to 6 144, and the
crowd frame carries 3 154 decals. The likeliest cost is in the splash path (decal reservation, the erosion above 85 %
of the pool, per-frame decal instance building), not in `area_sight`.

Is it a defect to fix now? No. The plan's bars are met as written (P: 14.97 ≤ 17.1, 25.20 ≤ 26.0), M is beyond the
plan's stress, and WP14 adds no look and changes no production cost path (only the pool size, which the B arm
already carries). The regression, if it is one, came in with WP7 to WP11 and belongs to a profiling item. But a
textbook project must not record a wrong cause: relative to today's baseline the plan's stress is +26 % at p50,
five times the plan's intended +5 %, and that only passes because the recorded baseline is 27 % slower than today's.

Requested change (text only, before merge):
1. Amendment 31 and `frame_budget_wp14.md` state that the S arm's non-record runs show the same update cost as P,
   so the cost is attributed to the splash stream (seven looks, 200 a second), not to the areas and held looks.
2. The open item is reordered: first a quiet B/S/P series (at least two triples, no timestamp queries) to confirm the
   attribution, then a `perf` profile of the S arm; `area_sight` only if P exceeds S.
3. The item is registered where the next plan will find it (the frame-budget C-list or `DESIGN.md` §4), with the
   relative bar it should restore: P within +5 % (p50) of a re-recorded baseline of today's client.

### W14-2 (Medium, condition): `entrench` shows none of the looks it asks for

The take's own check passes (`burrow_dig`, `burrow_mound`, `mode_seal_sweep`, `siege_anchor` were spawned), but on
screen the scene does not show them. The orbit is centred on (9, 9) and starts at -20°; from there the three
crawlers and two scorpions stand in a tight row directly behind the town hall's dome, half of them
overlapped by it, and the four pulse bots in a column beside it (`eff_60.png`, `eff_90.png`). In my 12-frame strip of
frames 24 to 90, which spans every burrow (ticks 34, 36) and every siege (76 to 81), I can see the scorpions' clip
change pose and a faint brown smudge at the row's foot; I cannot find a mound, a seal sweep or an anchor mark at 1:1.
The ground looks are drawn under the bubbles and behind the dome. Half the frame is empty floor and the field of
crystals. This is the beat whose caption says "Crawlers anchor and scorpions dig in", and it is the first thing the
effects take shows.

Requested change (data only): stage the line in the open. Either move it to a post in front of the hall before it
switches (an `AttackMoveToCell` for player 0 before `EnterModes`; the queued path exists and is tested by
`enter_modes_queues_behind_earlier_orders`, and the scenario's comment already describes it, W14-3), or keep it in
place and centre the orbit on the line with the hall behind it relative to the camera. Then re-render and add to the
packet a strip of `entrench` at 1:1 in which the dig, the mound, the sweep and the anchor can each be pointed at.
Because the scenario is new, neither change can touch a golden; `the_showcase_effects_battle_anchors_burrows_and_fights`
asserts properties, not ticks, and should still hold (re-run it).

### W14-3 (Low, condition): the scenario's comment describes orders it does not have

`assets/scenarios/showcase_effects.ron` says "Player 0 fields a siege line: it attack-moves to a post in front of its
base and, once there, every unit with a mode switch anchors or burrows (EnterModes, a cast queued behind the move)".
The file gives player 0 only `EnterModes`; the line switches at once where it spawned (Amendment 31: "at its base").
Requested change: make the comment true, by adding the move (W14-2) or by rewording it.

### W14-4 (Low, condition): `apply_initial_orders` does not document the new order or its error

Its doc says which units receive the two attack-move orders and lists the errors, but names neither
`InitialOrder::EnterModes` nor `ScenarioError::NoModeSwitches`. Requested change: one sentence each in the summary
and in `# Errors`.

Note on the same test (no change asked): the `NoModeSwitches` case builds the skirmish's world but passes the
showcase canyon's `MapDocument` to `apply_initial_orders`. It works because `start_approaches` only needs start
locations that both maps have, but it is the kind of mismatch a reader copies; loading the skirmish's own map would
be cleaner.

### W14-5 (Low, recorded): a scene's `effects` proves a spawn, not a sight

`SceneRecord::effects` and the take's failure check count effects the runtime placed for the viewer during the
scene, anywhere on the map. W14-2 is the proof that this is weaker than "the scene shows it": all four `entrench`
looks were spawned and none is readable. The field's doc and Amendment 31 say "must show"; that overstates it.
Requested change: say "spawned for the viewer (anywhere; visibility is judged on the pictures)" in `Scene::effects`'
doc and the amendment. Optional, as a later item: count only instances whose anchor projects inside the frame.

### W14-6 (Low): the crowd frame is badly framed for checklist item 6

`wp14_crowd_mix_400.jpg` and the baseline: the left 360 px (28 %) are off the map, and the two armies are cut by the
top edge, which is where the beams, explosions and storm discs are densest. In what is visible, the units stay
readable: every bubble of the baseline is a distinct bubble in the stressed frame, the green area discs sit under
them, the violet beams cross over them without hiding a cluster. So I judge item 6 **met** on this frame, but on the
less busy half of it. Requested change (not blocking): retake both frames with the camera centred on the clash (or
at the WP6c far view, so the coverage is comparable), and replace the two JPGs.

### W14-7 (Low): the effect pool's tripling is sound; say what 2 048 refused

Raising `budget.effects` to 6 144 is data, reversible and charter-clean. Its cost in an ordinary battle is the
per-frame walk of every slot (`free_ended` and `iter` in `pool.rs` both run over the capacity, live or not) and about
a megabyte of slots; the B arm, run at 6 144, is the fastest baseline recorded, so that cost is below noise. My one
reservation is the reasoning: §10.3 says a miss "shrinks budgets in data first", and a budget exists to refuse under
overload. At 2 048 there was no Critical overflow, so the refusals were the designed degradation, under a load
(20 storms and clouds at once) no battle produces. Growing the pool so that the synthetic run refuses nothing is a
reasonable choice for a measure of "every effect drawn", but the amendment should say so. Requested change: one
sentence in Amendment 31 (and the `effects.ron` comment) that 2 048 refused only non-Critical spawns, which classes
they were, and that 6 144 is chosen so the every-effect measure draws everything, not because a battle needs it.

### W14-8 (Info): CI render time needs no pre-merge mitigation

The render step in `showcase.yml` has no step timeout; the job's limit is 90 min; the workflow runs after CI has
passed and never gates CI or a merge. A render of 41 minutes would miss a budget, not break anything. The estimate is
also conservative: it adds all 591 frames of the effects take to the old 1 422-frame battle time, but the 200-frame
`closing` moved out of the battle take (now 1 222 frames), so about 200 of the paused frames are counted twice. The
recorded trigger ("if the first CI render exceeds 35 min, shorten `siege`, then `closing`") is the right follow-up;
WP15 should record the measured step time.

### W14-9 (Low): `siege` reads, with some crowding

In the 24-frame sequence and my full frames 175 to 240, the trikes' flame jet is the clearest look in the take: a
bright, tapering, flickering band that reads as fire at 1:1. The spine line reads as a row of chitin spikes running
from the burrowed scorpions to the raiders, its newest clump bright; at distance 16 the clumps are small, and because
the flame and the spines run along the same axis (raiders to the line and back), they overlap in about half the
frames and read as one stripe there. Sieged shells read as brown dust bursts among the raiders; their sieged origin
is off at the frame's top-left, partly under the caption box (sheet frames 3 and 6). Rounds and wrecks do not blur
the three. No change required; if W14-2 moves the line, a post that puts the fight across rather than along the
screen's diagonal would separate flame and spines, and the caption could move to the bottom for this scene.

### W14-10 (Low): the mix's determinism invariant is overstated

`effect_mix.rs` states "Every decision is a function of the frame's presented snapshot and a sequence number, so two
runs of one battle stream the same events." The number of shots sent per frame comes from `Time<Virtual>`'s delta,
which is the measured frame interval; two runs therefore land a given sequence number on different frames and
different snapshots, so the units chosen differ. The sources and their order are the same. Requested change: reword
to "the same sources in the same order; which units they join depends on the frame".

Other code checks, all met: no `unwrap` and no production `expect`; `#[must_use]`, `//!` and `///` on every new item;
the longest new function (`keep_held_views`) is about 60 lines; the mix's per-frame state lives in retained `Vec`s
built once, `MixSummary` is published with `clone_from` into existing buffers, `live_from` is a bounded scan; the
50-look `keep_unit_looks` search is O(looks × units) in a diagnostic, not in a battle sync system (§3.3 does not
apply). `stand_in_air_burst` is a diagnostic placement of a real event, not a fake asset, and it is unit-tested. The
`suites.ron` change adds `effects/**`, both showcase scenarios and the three catalogs as triggers of
`client_diagnostics/showcase_tests` (C22). No name of a unit or effect appears in Rust (C5); the two loadouts and
the asked-for effects live in RON.

### Note: unrelated commit on the branch

`f40e9bbd` (the owner's media cap raise to $230, Q0024 note, `veo_budget.ron`, two test constants) rides on this
branch. It is the owner's instruction and correctly recorded; mention it in the merge message so it is not read as
part of WP14.

## Checks run

- `cargo test -p sim_core --test scenario_tests` (worktree target dir, under `~/scratch/gate.lock`): queued for
  about ten minutes behind another worktree's `gate package` and stopped unrun, to leave the lock to it. I relied on
  the implementer's logs instead: `~/scratch/effects-viz-wp14/scen.log` (25 passed, including
  `the_showcase_effects_battle_anchors_burrows_and_fights`) and `gate_quick.log` (GREEN, after `2632ea82`, the last
  commit that touches `crates/`). The package gate must still run on the final tree.

## Conditions

| ID | Condition |
|---|---|
| C1 (W14-1) | Amendment 31 and `frame_budget_wp14.md` attribute the update cost to the splash stream on the S arm's evidence; the open item starts with a quiet B/S/P series and a profile of S, and is registered with a relative bar on a re-recorded baseline |
| C2 (W14-2, W14-3) | `entrench` re-staged so the line is in the open; a 1:1 strip in the packet in which the dig, mound, seal sweep and anchor are each visible; the scenario's comment matches its orders |
| C3 (W14-4, W14-5) | `apply_initial_orders` documents `EnterModes` and `NoModeSwitches`; `Scene::effects` and the amendment say "spawned", not "shown" |

C1 and C3 are text. C2 is data and one re-render. None needs another full review round if the orchestrator checks
the new `entrench` strip against W14-2. W14-6, W14-7 and W14-10 are advised in the same change; W14-8 and W14-9 need
nothing.
