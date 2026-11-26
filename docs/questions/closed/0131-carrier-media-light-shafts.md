# Q0131: Veo draws light shafts behind every saucer seed; how should the Carrier's media continue?

- **Asked:** 2026-11-26
- **Blocking:** yes, for the Carrier's media only (spending money); everything else of the plan goes on
- **Default until answered:** (c): no more generations; the Carrier and its interceptors stay without media, so the
  client keeps the Carrier locked ("No approved media"), which is truthful

## Question

Plan `2026-11-28-carrier-interceptors` WP5 makes Veo clips for the Carrier (`mothership_carrier`) and its new
interceptor (`saucer_interceptor`), so that players can train Carriers in the client. Four first clips ("seeds") of
each, eight generations ($3.20 of the batch's $10 cap; $116.85 of the $230 total spent), all drew diagonal light
shafts across the backdrop, the kind of rays you rejected on 2026-11-04 and that batch 5 rejected on `sky_bastion`.
The negative prompt already names light rays, god rays and spotlight beams, and two rewordings ("softly lit from the
front", "a round disc-shaped aircraft … nothing shines down on it") did not help. A saucer seems to bring the light
shafts of flying-saucer pictures with it. Everything else looks right: both designs are clear and distinct from the
Enclave's other saucers. The sheets are in `docs/evidence/veo/review/` (`saucer_interceptor_idle_*`,
`mothership_carrier_idle_*`) and the account in `docs/evidence/veo/2026-11-26-batch-carrier.md`.

- (a) **Try a different approach, within the remaining $6.80 of this batch:** add "light shafts, tractor beam,
  beam of light from above" to the negative terms of these two identities (a small baker change: per-identity
  negative terms, which `media_style.ron` does not have today), and spend at most 4 more seeds before stopping
  again. **Recommended** if you want Carriers in client matches soon.
- (b) **Accept faint shafts** in the backdrop for these two identities, as an exception to the backdrop rule, and
  continue the batch from the best seed (the Carrier's fourth seed has only faint shafts).
- (c) **Wait:** no more spending now; the simulation work ships, and the Carrier stays locked in the client until its
  media is made in a later batch.

## Answer

- **Option (a) with a hard cap of 4 more seeds, then fall back to Option (b) (take the best seed).** Add per-identity negative terms (e.g. `light shafts, tractor beam, beam of light from above`) and try up to 4 more seeds within the remaining `$6.80` of this batch. If none is completely shaft-free after 4 seeds, **take the best available seed** (accepting faint light shafts as a temporary exception) and proceed rather than blocking or spending more — the owner knows other videos also need rebuilding and plans to redo unsatisfactory videos in a polish pass once the game is finished.

## Resolution

Done on branch `carrier-media` (plan `2026-11-28-carrier-interceptors` WP5 continuation): both identities take
per-identity negative terms ("light shafts, tractor beam, beam of light from above, volumetric light, haze, light
streaks", through `actions.negative_overrides`, which the broodlings package added), and from seed 7 their Idle texts
say "seen from above … looking down on its top" on a plain dark backdrop. 13 more generations ($5.20; the batch 21,
$8.40 of $10; the ledger $128.45 of $230). The Carrier's third new seed (A7, `4b5d07ee`) drew no light shafts and was
taken as its pin; the interceptor's four new seeds all drew shafts, so its best seed, A2 (`3ddf92e4`, faint shafts,
the view right), was taken as its pin under the fallback. Accepted as the temporary exception: the interceptor's
backdrop shows faint diagonal shafts at the upper right in every clip (from its pin); its Idle's worst loop step is
just under the threshold (0.912 against 0.934; the rim lights blink); its Walk's light trails appear after the first
frame and vanish before the last (no scale lock, cross-fade 2); the Carrier's pin and clips show a soft dark shadow
falling to the lower left on a slightly graded backdrop. Both Attacks and the Carrier's loops are clean. Installed,
baked, strict audit 34 of 34; both identities in `required_set.ron`; the client unlocks the Carrier. H-20 (steady
state with interceptors cycling) waits for the render-entity pool (B-237). Evidence:
[`docs/evidence/veo/2026-11-26-batch-carrier.md`](../../evidence/veo/2026-11-26-batch-carrier.md). For the polish
pass.
