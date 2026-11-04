# Q0040: Should the zealot be tagged Biological, as in StarCraft II, instead of Psionic?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** no change. `psi_grey` stays Light and Psionic.

## Question

**What we found.** The catalog tags the zealot (`psi_grey`) Light and Psionic (`bits: 17`). The StarCraft II 5.0.14
game data and Liquipedia give Light and Biological. No document chose Psionic; it matches the unit's lore text
("psionic exo-carapace").

It changes nothing today. Battle 33 is identical on all 68 seeds with either tag, and no weapon or rule in
`sim_core` reads the Biological or Psionic flag for units (weapons have bonuses against Armored and Light only). It
would start to matter when an ability targets Biological units or Psionic ones, as several StarCraft II abilities
do.

**Options.**

1. **No change (default).** Keep the lore-based tag.
2. **Light, Biological** as in StarCraft II, following Q0008.
3. **Light, Biological and Psionic**, keeping the lore. StarCraft II's zealot is not Psionic, so a future
   anti-Psionic ability would hit it where the game's would not.

**Side effects.** None measurable today. The choice fixes how future attribute-filtered abilities treat the zealot.

Evidence: [`docs/evidence/2026-11-03-battle-33-investigation.md`](../../evidence/2026-11-03-battle-33-investigation.md)
§1.5, §2.4, §5.3 and §7.

## Answer

Owner (2026-11-04): Option 2 (`Light, Biological` as in StarCraft II, following Q0008).
