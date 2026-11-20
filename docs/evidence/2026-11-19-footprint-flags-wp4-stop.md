# footprint-flags WP4 stop: trace and proposed amendment

Tree: branch `footprint-flags-wp4-wip` (WP4 work on top of WP3 `7546795e`). Stop rule: plan §7 rule 4,
"a test that fails for a reason §6.1 does not list is a finding and stops the package".

## What moved and what did not

- Matrix (1,160 runs), battle 33 (64 seeds), showcase default seed and seeds 1-8 (6,000 ticks): all 205 outputs
  byte-identical to base 41fa340d. No winner flip, no survivor change, placebos not needed.
- golden_replay, golden_replay_orders: diverge at checkpoint 0 as planned. Proof done: a scratch build hashing each
  moved node at its old cell reproduces every recorded checksum (both fixtures); without the resources section the
  per-tick checksum is identical on all 3,280 logged ticks, old map vs moved map. Re-recorded lists pass native
  debug/release and wasm32-wasip1 release/debug. (`~/scratch/footprint-flags/impl/proof/`)
- WP4 guards: every_shipped_start_clears_every_resource_reach red on WP3, green after the moves;
  every_shipped_gas_node_takes_an_extractor green.

## The failures §6.1/§6.3 do not list

1. `sim_core/full_match_tests::stim_is_locked_until_researched_through_the_whole_tech_path`
   - Error: `stalled at RaiseModule with PlayerResources { minerals: 3500, gas: 0, ... }`.
   - Trace: the test hard-codes the showcase_canyon slot-0 gas node: `EXTRACTOR_SITE = (13, 5)` ("covers the gas node
     at (14, 6)") and, in `Stage::MineGas`, looks the node up by `cell == (14, 6)`. D-8 moves that node to (15, 6).
     The extractor at (13, 5) still covers (15, 6) on its edge and is built, but the lookup finds no node, so no
     worker is sent to harvest, gas stays 0 and the path stalls. Cause: a map coordinate in a test, not a behaviour
     change of the simulation.
   - Fix (test data only): `EXTRACTOR_SITE = (14, 5)` centred on (15, 6), lookup `cell == (15, 6)`, comment updated.

2. `sim_map/validation_golden_tests::{validate_map_matches_the_recorded_corpus_digest,
   decode_matches_the_recorded_corpus_digest}`
   - validation digest 0x78f4a8ea2debb63b (recorded 0x35278c7fa705ba2c), decode
     digest 0x10dbea436abaf292 (recorded 0x4c3225d2ef3afc87). Every required variant is still raised.
   - Trace: the corpus is "both shipped maps, then seeded mutations of assets/maps/default_arena.scmap.ron" (file
     doc, line 11-13), so moving five node cells changes the decoded documents and every validation report that
     prints them. The pathfinding golden of the same file passes (nodes block no path).
   - Fix: re-record VALIDATION_GOLDEN and DECODE_GOLDEN with a proof header: the corpus built from the base maps
     reproduces the recorded digests (the base tree passes), and the variant set is unchanged. Must pass on
     wasm32-wasip1 too (sim_map goldens run there).

Client: `mesher_golden_tests` (6/6) and `ramp_geometry_tests` (10/10) pass on the WIP tree. The GPU targets that
read the shipped maps (`runtime_battle_tests`, `editor_app_tests`, `terrain_tests`) were not run on the WIP tree.

## Proposed Amendment

"## Amendment 1 (orchestrator, 2026-11-19): WP4 also updates the tests that name the moved map data.
WP4's file list gains `crates/sim_core/tests/full_match_tests.rs` (the tech path's extractor site and gas lookup
follow D-8's move of the slot-0 gas node to (15, 6); test data only, no bound changes) and
`crates/sim_map/tests/validation_golden_tests.rs` (the two corpus digests over the shipped maps are re-recorded with
the proof that the base maps reproduce the old digests and the variant set is unchanged; native and wasm32-wasip1).
§7 rule 4 is read as: a test that fails only because it names or digests the moved node cells is a WP4 consequence,
fixed with its trace in the WP4 commit; any other failure still stops the package."
