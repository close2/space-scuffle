# Q0001: Accept a skirmish without gas, research and transport for now?

- **Asked:** 2026-11-02
- **Blocking:** yes, for any paid generation run only
- **Default until answered:** build all of it, but disable it in the real game. No paid Veo run.

## Question

A structure or unit with no approved media stops the match with a fatal error, by design (charter §1.2). The
Automaton faction, which is what the skirmish uses, has media for 3 of 17 structures and 3 of 13 trainable units.
With what exists, a player can mine, build three structure types, train two unit types, and win or lose.

- **Option A (recommended):** close Phase 5 on that mineral path. Gas, research and transport are built and
  tested, and appear in the game as disabled with "No approved media". They switch on with no code change when
  the art exists.
- **Option B:** approve a paid Veo run first. Gas and research need `plasma_siphon_dome` and `tech_module_pod`.
  Transport also needs `machining_plant_dome`, `aerospace_hangar_dome` and `repair_shuttle`.

The media for these five does not exist anywhere on the server or in git history. If you find it on the notebook,
copying it here replaces option B.

Details: `docs/plans/2026-11-02-phase-5-playable-client.md` §2.6 and §7.

## Answer

Owner (2026-11-03): Option B. Veo should be part of my ultra plan. So use it. (I think I have weekly limits and it would be a shame to waste them).

## Effect of the answer

2026-11-03. Option B is approved: paid Veo generation for `plasma_siphon_dome`, `tech_module_pod`,
`machining_plant_dome`, `aerospace_hangar_dome` and `repair_shuttle` may run on the owner's plan. The default of no
paid Veo run no longer applies. Remediation step 8.5 is unblocked, and a plan for the generation run is being
written. This question stays open until that media is baked and gas, research and transport are enabled in the
shipped game; the temporary procedural fixtures of Q0002 item 3 are then replaced by real art.

## Resolution

2026-11-05. Carried out by remediation step 8.5, plan
[`docs/plans/2026-11-04-phase-8-5-veo-media.md`](../../plans/2026-11-04-phase-8-5-veo-media.md) (amended by the
letterbox, seamless-loops and structure-action-text plans). The condition this question was kept open for is met:

- **Media baked.** The five identities this question names (`plasma_siphon_dome`, `tech_module_pod`,
  `machining_plant_dome`, `aerospace_hangar_dome`, `repair_shuttle`) got real Veo media in batch 1 (`1230155` to
  `fb1be5b`, merged `2c5ed5e`); the mineral path (`welder_drone`, `capacitor_dome`, `assembly_bay_dome`) and the
  town hall (`foundry_core_dome`) in batch 2 (`9f132d4`, `e2573de`, `70ad1d6`, `c3c3e10`, merged `473b977`).
  `asset_baker audit-media --strict --ledger docs/evidence/veo/ledger.jsonl` verifies the required set
  (`assets/media/required_set.ron`), 10 of 10, and no atlas is a copy of `concept_video.png` (finding B-1 fixed).
- **Gas, research and transport are enabled in the shipped game** since batch 1, with no code change to the media
  gate. `crates/client/tests/battle_card_path_tests.rs` plays each path on the shipped asset root and replaces the
  temporary procedural fixtures of Q0002 item 3 (`fb1be5b`).
- **Spend:** 41 generations, $16.40 at list price (Veo 3.1 Fast, 720p, with sound), within the 200 EUR of Q0024.

Evidence: [`2026-11-04-pilot.md`](../../evidence/veo/2026-11-04-pilot.md),
[`2026-11-04-batch-1.md`](../../evidence/veo/2026-11-04-batch-1.md),
[`2026-11-05-batch-2.md`](../../evidence/veo/2026-11-05-batch-2.md). Remediation Phase 8's exit is recorded as met.
Media for the rest of the Automaton faction (the plan's WP6) is a separate step. It waits for the orchestrator and
for Q0025's second ask.
