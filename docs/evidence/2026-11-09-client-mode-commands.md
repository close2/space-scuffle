# Evidence: Mode Commands in the Skirmish Client

Measurements and research for plan [`2026-11-09-client-mode-commands`](../plans/2026-11-09-client-mode-commands.md)
(Q0070 option 1). The measurement method is the deviation-fix packages' ([evidence](2026-11-05-sc2-deviation-fixes.md)
introduction): the whole matrix (`crates/sim_core/tests/fixtures/matchup_matrix.ron`, 50 battles) on 20 seeds each
(the 4 of record plus 16 jittered) and battle 33 on 68 seeds, both from release builds of
`crates/sim_core/examples/matchup_trace.rs`; the golden replays in debug, release and `wasm32-wasip1`; and the
targets the plan names (`full_match_tests`, `burrow_attack_tests`, `sc2_who_wins_matchup_matrix_tests`, release).

```bash
matchup_trace --all-battles --seeds 16 --jobs 4 --out ~/scratch/mode-meas/<step>/matrix \
    --compare <previous>/matrix/matrix.csv
matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/mode-meas/<step>/b33
```

The binary of each step was built from that step's commit and copied to `~/scratch/mode-meas/<step>/` before the
next edit. The gate lock was held by another package's verification for the whole session, so the matrix ran
unlocked under `nice` with 4 jobs (the matrix needs little memory; the user's anonymous memory stayed under 8 GiB).

## 1. Simulation (WP1)

### 1.1 Baseline

The baseline is WP13b's data matrix (`~/scratch/sc2dev_wp13b/data/matrix/matrix.csv`, commit `7e5e524`) and its
battle 33 run (41 / 10 / 17 marine wins / zealot wins / unresolved). Between `7e5e524` and this plan's base `33a8e82`
the simulation sources are unchanged; `units.ron` changed only in two `veo_prompts` strings, which the simulation
does not read. So WP13b's figures are the state of `main`.

### 1.2 Research: the two claims the plan marked **verify**

- **A non-transient morph replaces the unit's orders; a `Transient` ability does not (D-6a).** No official Blizzard
  documentation of the editor's ability flags was found. Editor-community sources describe the flag consistently:
  "The Transient flag, when ticked, causes the ability to activate instantly, bypassing the order queue and allowing
  the unit to take other actions while the ability activates", and "with the flag unticked activating the ability
  will usually interrupt any other action (such as an ongoing Move order)" (SC2Mapster forum, Data section, as
  indexed by the search engine on 2026-11-09; the forum now redirects to CurseForge, so the thread could not be
  re-read); the Hive Workshop editor tutorial "Creating an Instant Effect Ability" ticks `Transient` so that the unit
  uses the ability "without pausing" (wiki.hiveworkshop.com, unreachable on 2026-11-09, quoted from the search
  index). An SC2Mapster thread on morphs adds that `Transient` "isn't supported for morph abilities", so a morph that
  should run on the move needs a transient wrapper ability. With the data (`Flags Transient` on `Stimpack`, `liberty`
  AbilData:1231; absent from `SiegeMode` :1301, `Unsiege` :1328 and `BurrowLurkerMPDown`, `swarm` AbilData:1654),
  this supports D-6a. The claim is accepted on these secondary sources; it is not an official statement.
- **Shift queues an order behind the current ones (D-5, D-10).** Blizzard's own game guide, "Special Control"
  (news.blizzard.com/en-us/article/4552955): "To queue commands, hold down the Shift button and issue commands to
  the unit", for "Most commands (such as left-clicking, right-clicking, Stop, Attack, Hold Position, Move, Patrol,
  and Load/Unload)", with a limited queue. Confirmed.

### 1.3 Results

| Step | Commit | Matrix (against the previous step) | Battle 33 (68 seeds) | Goldens (debug, release, `wasm32-wasip1`) | Targets |
|---|---|---|---|---|---|
| WP1a queries | `97fd318` | (no tick change: nothing in the tick calls them) | — | — | — |
| WP1b D-6a | `ea3cb75` | bit-identical to WP13b's (1000 runs, expectation failures 0 / 0) | every seed identical (41 / 10 / 17) | green, green, green | green |
| WP1c D-6b | `885002a` | bit-identical to WP1b's (1000 runs, expectation failures 0 / 0) | every seed identical (41 / 10 / 17) | green, green, green | green |

**WP1b.** The matrix battles with a `mode` side (battle 21's Lurker) now cast before the standing order; the reorder
of D-6a left every battle bit-identical, as predicted: casting and committing the side's order write disjoint state
apart from velocity and waypoints, which both clear. The order replacement mirrors `Stop`'s (movement memory and
task leg restarted, queue cleared, Hold ended) without Stop's weapon-phase change; the plan names only the queue and
the Hold flag, and the other two are what every non-queued order resets in `commit_order`. The new test
`a_mode_switch_without_shift_drops_the_walk_and_the_anchored_unit_fires` was red before the change (the walk stayed
at the front of the queue).

**WP1c.** The front cast now runs only for units that the first check of `walking_goal` used to stop (held or
anchored, not channelling); mobile units keep the existing path, so a unit winding up a fixed weapon is not changed.
The new test `a_queued_switch_behind_a_focus_order_runs_on_an_anchored_unit` was red before the change.
