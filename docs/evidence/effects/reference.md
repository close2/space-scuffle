# Effect Reference Board

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../plans/2026-11-09-effect-visualization.md) §7.1
(work package WP6c). This board states, for each showcase effect, the StarCraft II behaviour it must read like,
the sources for that behaviour, and the production principles it applies. It holds no copyrighted frames: the
StarCraft II behaviour is described in words and linked to Blizzard's and the community's own pages. Looks stay
original (plan S-6): the board fixes what an effect must *communicate*, never what it copies.

Once an effect passes its review ([`review/slice_review.md`](review/slice_review.md)), its own approved contact
sheet becomes the bar for later effects of its kind; the approved sheets are listed in the last section.

## Principles applied to every effect

| Principle | Rule for this project | Source |
|---|---|---|
| Gameplay first | The dangerous area or line is readable at gameplay zoom within 0.1 s; the spectacle never hides units or the area. Small effects stay small. | Riot, "League's VFX style guide" (2017) <https://nexus.leagueoflegends.com/en-us/2017/10/dev-leagues-vfx-style-guide/>; GDC Vault, "Visual Effects Bootcamp: Artistic Principles of VFX" (Keyser, Riot; Chamberlain, Blizzard) <https://www.gdcvault.com/play/1024439/Visual-Effects-Bootcamp-Artistic-Principles> ("Gameplay is king") |
| Clarity in a crowd | One focal point per effect; secondary elements lower in value and saturation; minimise clutter so 200 splashes a second over a 400-unit fight still show the units. | Riot, "Clarity in League" <https://www.leagueoflegends.com/en-us/news/dev/clarity-in-league/> |
| Shape language | Circles for areas, lines for lines: the silhouette of the Critical layer is the gameplay shape (R2 of the plan). | Riot style guide (above), "shape language" section |
| Value structure | Brightest at the focal point, a ramp from hot to dark; team colour only in secondary elements. | Riot style guide (above), colour and value sections |
| Layering | Flash or light, core, smoke or debris, ground mark: several simple layers, never one flat shape. | Simon Trümpler, "Battlefront II: Layered Explosion" <https://engineered.at/articles/battlefront-ii-layered-explosion> |
| Timing | Fast attack to the peak (within two frames of the event), eased fall-off, dissipation by erosion. | VFX Apprentice, "The Soul of Effects: What is Timing in VFX?" <https://www.vfxapprentice.com/blog/the-soul-of-effects-what-is-timing-in-vfx> |
| No cheap tells | No quad edges, no hard ground intersection (soft depth), no visible repetition (random rotation, mirror, variants), no pops, no banding, no key fringes. | vfxdoc, "Alpha erosion" <https://vfxdoc.readthedocs.io/en/latest/shaders/alpha-erosion/>; NVIDIA, "Soft Particles" <https://developer.download.nvidia.com/whitepapers/2007/SDK10/SoftParticles_hi.pdf>; Shawn Hargreaves, "Premultiplied alpha" <https://www.shawnhargreaves.com/blog/premultiplied-alpha.html> |
| Fog | An effect shows where it lands if the viewer sees that point; an undetected source is never drawn (R1, R3). | Blizzard game guide "Detection" <https://news.blizzard.com/en-us/article/4546770/game-guide-detection> |

## `shell_impact` (sieged Railgun Crawler; StarCraft II analogue: Siege Tank in Siege Mode)

- **Must read like:** a heavy shell striking the ground: a short, bright detonation at the impact with dust and
  debris thrown up, and a burn mark left on the ground. No damage numbers. The area is read from the burst's size
  alone; the Siege Tank's splash is small (zones 0.4687 / 0.7812 / 1.25 in this catalog, from the 5.0.14 data
  sourced in plan `2026-11-04-combat-sc2-alignment`), so the effect stays small at gameplay zoom and its scorch
  marks the true reach.
- **Sources:** Liquipedia "Terran Unit Statistics" <https://liquipedia.net/starcraft2/Terran_Unit_Statistics>
  (Siege Tank splash); TeamLiquid "Splash and AoE in SC1 and 2"
  <https://teamliquid.net/blogs/148463-splash-and-aoe-in-sc-1-and-2>; plan §3 "Splash" row.
- **Principles:** focal point is the fireball's hot core; flash, light, fireball, sparks, debris, smoke and scorch
  as separate layers (Battlefront II layering); the shock front on the ground is the Critical layer at the reach.

## `acid_burst` (Volatile Tick; StarCraft II analogue: Baneling)

- **Must read like:** a creature bursting in a splash of green acid: a wet green burst covering the whole damage
  area at once and a green stain on the ground that dries. The Baneling's splash is one zone (2.2 in this catalog,
  Liquipedia's 2.20 AoE), so the burst and the fresh stain both read as one disc of exactly that radius.
- **Sources:** Liquipedia "Terran Unit Statistics" (Baneling 2.20 AoE listed in the comparison table)
  <https://liquipedia.net/starcraft2/Terran_Unit_Statistics>; TeamLiquid "Splash and AoE in SC1 and 2" (above);
  plan §3 "Splash" row ("Baneling acid splash on the ground with a green stain").
- **Principles:** one focal point (the burst's glowing centre), the acid front as the Critical layer at the reach,
  secondary droplets and vapour lower in value; a stain that erodes, not fades.

## `spine_line` (burrowed Burrow Scorpion; StarCraft II analogue: Lurker)

- **Must read like:** a straight line of spines erupting from the ground, one after another, from near the
  attacker towards the target and on to full range, then retracting. The spines reveal where the attack is,
  never the burrowed attacker (an undetected Lurker's spines show, the Lurker does not). Range 8 at 5.0.14.
- **Sources:** Battle.net Lurker description <https://classic.battle.net/scc/zerg/units/lurker.shtml> (spines
  "rip up through the ground in a straight line … to maximum range"); Blizzard balance update 2019-11-14
  <https://starcraft2.com/en-gb/news/23223600> (range 9 to 8, Seismic Spines 8 to 10); Blizzard forum "Fave ways to
  deal with cloaked/burrowed units w/o detector"
  <https://us.forums.blizzard.com/en/sc2/t/fave-ways-to-deal-with-cloakedburrowed-units-wo-detector/28048>
  (burrowed units "can be located by their attack effects, such as spines shoot out from Lurkers"); plan B-85
  (spines 0.125 Normal s apart).
- **Principles:** the line itself is the shape (spines and the crack, both Critical); travel along the line is the
  timing; the source end carries no layer, so fog rule R3 holds by construction.

## Approved stills (canonical bar)

None yet. An effect's contact sheet enters this list when its review in
[`review/slice_review.md`](review/slice_review.md) accepts every checklist item and the independent review of
[`review/independent_review_packet.md`](review/independent_review_packet.md) agrees.
