# Q0069: When during its burrow does the Lurker become a burrowed unit?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** no change to the rule WP13b landed. The burrowing Lurker analogue (`burrow_scorpion`)
  takes the burrowed form all at once, when the morph ends: it can be targeted and cannot attack until then.
  A difference stays recorded under bug B-42 in `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP13b (bug
B-42) makes the Lurker analogue attack only burrowed. The data (SC2GameData `5444e12f13`, chain
`core → liberty → swarm → void → voidmulti`): the unburrowed `LurkerMP` has no weapon (`liberty` UnitData:690-758),
the burrowed `LurkerMPBurrowed` carries `LurkerMP` (`void` UnitData:775), is `Cloaked` and `Buried` and collides
only with `Burrow` (`liberty` UnitData:767-768, :775). The morph `BurrowLurkerMPDown` has three sections (`swarm`
AbilData:1654-1674, `void` :149-155):

| Section | Delay | Duration | Ends at |
|---|---|---|---|
| `Actor` | 0 | 2.5 Normal s | 2.5 s |
| `Collide` | 1.8332 | 0 | 1.8332 s |
| `Stats` | 1.8332 | 0 | 1.8332 s |

The morph's time is its longest section, 2.5 Normal s (45 ticks), plus up to 0.25 s random; Adaptive Talons cuts
`Actor` by 1.0 and the two delays by 0.66 (`voidmulti` UpgradeData:780-783), and Liquipedia's 5.0.9 note ("from 0.71
seconds to 1.07 seconds") matches the `Actor` time, so the burrow takes 2.5 s. Evidence
[`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §15.

What the data does not say is which properties switch at a section's end before the morph is over: whether the
`Stats` section at 1.8332 s carries the weapon and the `Cloaked` flag, so that the Lurker can attack and becomes
invisible 0.67 s (12 ticks) before the burrow animation ends. (That a burrowed unit stops blocking ground units, the
`Collide` section, is not modelled at all: bug B-93.)

**Why it matters.** Twelve ticks of attacking and of being untargetable without detection decide how many shots a
burrowing Lurker takes in a fight (battle 21: six marines shoot it while it burrows).

**Options.**

1. **No change (default).** Everything switches at the end of the morph, as Siege Mode does (its `Actor` and
   `Stats` sections end together, so it cannot tell the two readings apart).
2. **Per section.** Weapon and cloak at the `Stats` end. This needs a per-section
   mode timing in the catalog.
3. **Measured.** An editor test: a Lurker burrowing next to an enemy Marine, logging each game loop whether the
   Lurker fires, whether the Marine can target it, and whether a unit can walk over it.

**Recommendation.** Option 3, then option 1 or 2 as it shows.

## Answer

**Option 2 (Switch weapon and cloak at the `Stats` section end at `1.8332` Normal s / `33 ticks`; no editor test needed).**

**Engine insight:** In StarCraft II's `CAbilMorph` engine, each section timer in `InfoArray` governs its specific subsystem independently: `Collide` updates collision flags at `1.8332` Normal s (`33 ticks`), `Stats` swaps the unit's `CUnit` identity (activating `LurkerMPBurrowed`'s weapon and `Cloaked`/`Buried` flags) at `1.8332` Normal s (`33 ticks`), and `Actor` continues playing the visual burrow/dirt-settling animation until `2.5` Normal s (`45 ticks`). Thus a burrowing Lurker becomes untargetable without detection and able to fire 12 ticks before its visual `Actor` animation finishes. Switch weapon and cloak at the `Stats` section end (`33 ticks`), or set the functional morph completion timer to the `Stats` duration while letting the visual transition run to `Actor` (`45 ticks`).

## Resolution

2026-11-09. Option 2. The burrowing Lurker analogue's entering channel ends at the `Stats` section's end, 33 ticks
(plus the morph's random extra), where its weapon and cloak arrive; the `Actor` section's 45 ticks are the animation,
which the client does not present yet. Data `e59a23f`. Bug B-42 is fixed. Plan `2026-11-05-sc2-deviation-fixes` Amendment 22 records it; evidence `2026-11-05-sc2-deviation-fixes.md` §17.
