# Round 4 — Non-shadowing combat light effects

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: approved by the orchestrator; implementation waits for release of the coordinated gate freeze and precedes the larger plan 35 GPU storage change.

## Goal and evidence

Remove opaque geometric shadows cast by transparent combat light cards while preserving terrain/unit shadows and real combat rendering. The actual production image `/tmp/space-scuffle-r4-runtime-captures-run02/04-combat.png` contains a hard diagonal black rectangle aligned with a tracer. Root's source audit and this agent's inspection confirm `vfx.rs` creates additive `CombatFxMaterial` quads without `NotShadowCaster`. Mist already carries that marker and is a separate visual problem.

## Bounded implementation

Own `client/src/vfx.rs` and a focused `client/tests/combat_light_shadow_tests.rs`. Add a documented `CombatLightEffect` component with `#[require(NotShadowCaster)]`, representing additive light-card geometry. Attach it at every existing CombatFxMaterial spawn path: hitscan beams, projectiles, lightning beams, melee slash, central detonation and current shader-card shatter fragments. The marker is a renderer invariant, not a unit-specific branch or a change to simulation combat events.

The existing shatter fragments use the same additive shader-card material, so their current rectangle geometry must not cast solid shadows. Future real opaque debris should use an explicit different component/material with an appropriate shadow shader; do not silently make all future mesh debris shadowless by global material flags.

Do not change combat brightness, timing, shapes, unit media, depth bias, shader art, terrain shadow settings or mist. Do not disable global shadows or replace the combat renderer with a mock. Keep all editor files untouched.

## Verification round

Construct real public combat event batches for every relevant visual variant, run the actual production VFX spawning/update systems with their real assets/resources, and assert that each spawned CombatFxMaterial mesh has the marker and required `NotShadowCaster`. Verify the test spawned the expected nonzero counts and reached all branches, rather than passing on an empty query. Include central burst and shatter paths. Assert normal unit/terrain render entities retain their existing shadow behavior.

Re-run the production runtime capture driver at its deterministic combat checkpoint with the real renderer and actual assets. Require pipeline readiness, a clean validation log and completed screenshot acknowledgment. Compare `/04-combat.png` at the same camera/tick: the hard rectangular tracer/card shadow must disappear, while the cliff and unit shadows remain. Inspect the actual light effect and silhouettes; source component assertions alone do not establish visual correctness. Retain before/after PNGs and review notes.

The latest matched baseline is `/tmp/space-scuffle-r4-runtime-captures-run06/04-combat.png` at 1920×900, tick 614, virtual time 24.56 seconds, checksum 14247579459550082507. Its camera target is (37.00582, 45.356785), distance 22.0, translation (37.00582, 22.85706, 57.015007). Both root and this agent inspected the strong rectangular shadow near pixel (900, 590) and the thinner lower-right streak near (900, 790). Compare both regions, together with the upper cliff and circular unit-shadow control regions. Use the manifest to verify matched state; the older 1280×720 run02 remains historical evidence only.

Run focused tests, then coordinated workspace tests, strict all-target Clippy and formatting. This correctness fix is accepted independently of the still-rejected mist and shell damage art; do not use it to declare those effects complete.
