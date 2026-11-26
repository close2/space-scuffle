# Q0133: The Brood Lord analogue's Veo seeds draw light shafts too; how should its media continue?

- **Asked:** 2026-11-26
- **Blocking:** yes, for the Brood Lord analogue's and the Broodling's media only (spending money); the simulation is
  done and ships
- **Default until answered:** (c); no more generations for `spore_matriarch` or `brood_mite`; the Brood Lord analogue
  stays locked in the client (no media), as it is today

## Question

Plan `2026-11-28-broodlings` gives the Brood Lord analogue (`spore_matriarch`) its Broodlings in the simulation, and
its WP6 asked Veo for real clips of it and of the Broodling (`brood_mite`). The batch stopped after 8 generations
($3.20; ledger total $120.05 of $230): of the Brood Lord analogue's four seed Idles, the three whose text asked for a
view from above drew diagonal light shafts across the backdrop (your rule: no rays or radiating stripes), and the one
that did not was seen nearly level with a wing cut off by the frame; all four flapped their wings. It is the same
staging the Carrier's saucers drew ([Q0131](0131-carrier-media-light-shafts.md)), against negative terms that already
name light rays, god rays and spotlight beams. The Broodling's seed is good and is kept as its pin, but its three
pinned Idles still nod their heads; and without the Brood Lord's media a Broodling never appears in a match. Evidence:
[`docs/evidence/veo/2026-11-28-batch-broodlings.md`](../../evidence/veo/2026-11-28-batch-broodlings.md).

- (a) **Per-identity negative terms** (Q0131's option (a)): let the baker add terms such as "light shafts, beams of
  light, volumetric light, haze" for these identities only, then retry four seeds each within the batch's remaining
  17 generations ($6.80).
- (b) **A start picture**: seed the Brood Lord from an authored still (for example the first frame of its first
  seed, recomposed so the whole beast is in the frame) instead of from text; the clips are then real Veo animation of
  that still.
- (c) **Leave both locked for now** (default): the simulation ships; the client keeps the Brood Lord analogue locked
  (truthful: no media), and its media wait for Q0131's answer, which decides the same problem for the Carrier.

Recommended: (c) now, then (a) together with the Carrier once Q0131 is answered (a).

## Answer

- **Try up to 4 more seeds (using per-identity negative terms as in Q0131 (a), or a clean start still if needed for framing), then take the best available seed** within the remaining batch budget. Accept faint light shafts / minor staging imperfections as a temporary exception so `spore_matriarch` and `brood_mite` unlock in the client now; unsatisfactory videos across the roster will be redone in a polish pass once the game is finished.

## Resolution

Done on branch `broodlings-plan` (plan `2026-11-28-broodlings` WP6 continuation): `asset_baker` gained per-identity
negative terms (`actions.negative_overrides` in `media_style.ron`, tested), used for both identities. 8 more
generations ($3.20; the batch 16, $6.40 of $10; the ledger $123.25 of $230). Accepted as the temporary exception:
`spore_matriarch`'s Idle and Walk beat their wings (an uneven zoom no seam repair locks) with faint light shafts at the
top of the backdrop, and its pin's left wing tip touches the frame edge; `brood_mite`'s Idle nods its head slightly
and its Walk's worst loop step is just under the threshold. Both Attacks are clean. Installed, baked, strict audit 32
of 32; both identities in `required_set.ron`; the client unlocks the Brood Lord analogue. Evidence:
[`docs/evidence/veo/2026-11-28-batch-broodlings.md`](../../evidence/veo/2026-11-28-batch-broodlings.md). For the polish
pass.
