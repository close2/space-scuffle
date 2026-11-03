# Round 4 — Combat light-card shadows

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

## Goal and implementation

Approved plan 37 addresses the opaque rectangular shadows visible in the actual production combat checkpoint. `CombatLightEffect` now requires Bevy's `NotShadowCaster` and is attached at all six additive-card spawn sites: hitscan, projectile, beam/lightning, melee slash, central burst and shatter cards. The classification is explicit and leaves ordinary opaque geometry shadow behavior unchanged. No shader, brightness, event timing, media or camera changes belong to this fix.

The focused integration test runs actual production setup, event spawning and lifetime update systems. It asserts exact material-branch counts `[1, 1, 1, 1, 1, 17]`, covering all 22 cards, before and after a 40 ms update. Every card must have the light classification and required non-shadow-caster component; an ordinary opaque material mesh remains shadow-capable. This proves the ECS invariant, not final rendered appearance.

## Verification evidence

- `/tmp/space-scuffle-r4-combat-shadow-tests.log`: targeted event-system regression passes.
- `/tmp/space-scuffle-r4-combat-shadow-clippy.log`: strict focused Clippy passes after replacing an explicit test panic with a returned test error.
- `/tmp/space-scuffle-r4-combat-shadow-build.log`: actual `battle_capture` executable builds successfully.
- The first exact workspace format check flagged only another track's active ramp-traversal test edit; its owner is formatting that file. Final coordinated formatting remains required.

Native comparison passes the bounded shadow goal. Root and this agent independently inspected `/tmp/space-scuffle-r4-combat-shadow-after37/04-combat.png` against `/tmp/space-scuffle-r4-runtime-captures-run06/04-combat.png`: both the strong rectangle near (900, 590) and thin streak near (900, 790) disappear, while cliff and unit shadows remain. The actual run exits 0 with all seven checkpoints and a complete manifest. Root checked 303 logical manifest fields for equality, including tick 614, checksum 14247579459550082507 and virtual time 24.56 seconds. Cliff/open-ground control pixels are byte-identical and unit-shadow mean absolute difference is 0.042/255; the former rectangular-shadow region mean increases from 104.67 to 167.34.

The unfiltered native run nevertheless exposed a separate unused-audio failure: 23,560,046 ALSA POLLERR callback messages. Root preserved the complete compressed log at `/tmp/space-scuffle-r4-combat-shadow-after37.log.gz`. Renderer validation and panic counts are zero, but the overall runtime log is not clean. Plan 42 must correct intentional audio initialization before performance baselines. The matched image proves the shadow correction, not audio correctness or overall art acceptance.

The final targeted regression and exact workspace formatting both pass after concurrent formatting settled. Remaining coordinated workspace gates are required after integration.

The preceding workspace gate is **not accepted**: a load-sensitive sim-map pathfinding performance test exceeded its debug time budget under shared-machine contention. The pathfinding owner is investigating and measuring that separately. Correcting the combat-card shadow does not accept the still-rejected mist or shell hit/attack art.
