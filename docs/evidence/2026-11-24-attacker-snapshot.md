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
