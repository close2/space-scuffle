# Evidence: the attacker snapshot (B-183)

Plan: [`docs/plans/2026-11-24-attacker-snapshot.md`](../plans/2026-11-24-attacker-snapshot.md). Measured on the package
branch `attacker-snapshot-plan` on top of `main` `8a15bff9` (plan commits `1ad05854`, `b010d0a6`), 2026-11-24, on the
shared VM. Scratch material lives under `~/scratch/attacker-snapshot/` (`scripts/`, `red/`, `proof-<label>/`).

## 1. WP0: baseline

### 1.1 Goldens and the sandbox checksum on four builds

`scripts/proofcopy.sh <label> <commit>` exports the commit with `git archive` into a fresh tree, builds it in a fresh
target directory (`~/cargo-target/attacker-snapshot-proof-<label>`) and runs `golden_replay_tests` (both fixtures,
4 tests) and `fixture_tests` (`DEFAULT_SANDBOX_CHECKSUM`, 3 tests) on native debug, native release, `wasm32-wasip1`
release and `wasm32-wasip1` debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<tree>"`).

Baseline on `b010d0a6` (`proof-base/`): all eight runs pass. The recorded values every later proof must reproduce:

| What | Value |
|---|---|
| `crates/sim_core/tests/fixtures/golden_replay.ron` (sha256) | `6972016488fc6aa8b5738d39817ef78ae7747c0c006fea3b48eac2e3942a5988` |
| `crates/sim_core/tests/fixtures/golden_replay_orders.ron` (sha256) | `5406121eb6ad588fbd094b2b4b395b5d7be11575aa1fbd2f18f7055c6c7eb9a4` |
| `DEFAULT_SANDBOX_CHECKSUM` | 6_724_706_104_475_870_814 |
| Checksum schema | 45 |

A later proof passes only on these fixtures unchanged: the tests compare every checkpoint of both replays with the
fixture files, and the sandbox world with the constant.

### 1.2 Does the showcase run under a player's fog? (plan §7)

Yes. Both takes of `assets/captures/showcase.ron` (`battle`, `effects`) are `mode: Skirmish`, which
`client::ClientApp` runs under `BattleProfile::Player` (`crates/client/src/lib.rs`, `battle_profile`), so the
showcase draws the fog of its viewer and will draw that viewer's attack snapshots. The showcase video is rendered
by CI from the storyboard and is not committed, so nothing is re-recorded by hand. Which of its ticks hold a
snapshot for the viewer is measured once the simulation records them (§2), because no earlier tree can tell.

## 2. WP1: the snapshot in the simulation and the view

### 2.1 Red run (`red/wp1-red.log`)

On the WP1 tree with the three hooks cut (`record_attack_presentation` not calling `record_attack_snapshot`,
`update_vision` not calling `prune_attack_snapshots`, `write_player_view` not calling `record_attack_snapshots`; the
types, the workspace's list, `shown_to` and `snapshot_bubble` in place):

| Test | Red |
|---|---|
| `attack_snapshot_tests` (P-1, P-5 to P-8, P-10 to P-12) | 8 of 10 FAILED; `a_strike_the_struck_player_sees_records_nothing` (P-3) and `other_visibilities_record_no_snapshot` (P-4) pass, as guards of what must stay empty |
| `player_view_tests::only_the_struck_players_view_lists_a_snapshot_and_refills_it_in_place` (P-2) | FAILED |
| `combat_resolution::tests::each_condition_of_the_snapshot_rule_decides_alone` | FAILED |
| `combat_resolution::tests::a_detected_concealed_attacker_leaves_a_snapshot` (D-3) | FAILED |
| `world::vision::tests::a_snapshot_is_dropped_when_its_spot_is_seen`, `…_when_its_attacker_is_seen` | FAILED |
| `world::snapshot::tests::forgetting_attack_snapshots_changes_no_checksum` (P-9) | FAILED on its premise (no snapshot was recorded) |
| `world::vision::tests::a_snapshot_outlives_its_attacker` | passes (nothing drops it either way) |
| `world::presentation::tests::*`, `world::snapshot::tests::a_snapshot_of_an_idle_unit_is_its_live_record`, `…_in_a_mode_keeps_the_modes_stance_and_weapon`, `sim_types` `a_view_without_attack_snapshots_keeps_its_bytes` | pass: the data structure and the record builder are in place before the hooks |

With the hooks (`red/wp1-green.log`), `seeing_the_attacker_drops_the_snapshot` (P-7) still failed: "tick 255: attacker
shown true, 1 snapshots". The attacker walks into the viewer's sight during the movement phase of a tick, after that
tick's vision phase pruned. The view of that tick then showed both the live unit and its image (the overlap of plan
R-7). The fix: `write_player_view` leaves out a snapshot whose attacker it shows now (`snapshot_attacker_shown`, the
drop rule's second half); the vision phase drops the record on the next tick. Recorded in the plan's As built. All
WP1 tests pass after it.

### 2.2 Gate

`cargo gate quick` on the WP1 tree: 112 targets, all PASS (`~/scratch/attacker-snapshot/gate-quick-wp1.log`). The new
`attack_snapshot_tests` is a `fast` target like the other vision tests, so `crates/gate/suites.ron` needs no entry.
C7: the float allowance of `crates/sim_core/src/world/snapshot.rs` rises from 3 to 7 for the shared placement and share
conversions (`Placed`, `published_share`), with this plan's reference.
