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
