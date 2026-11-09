# Q0072: Should the command card use StarCraft II's per-ability hotkeys instead of positional keys?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** keys stay positional. A command card key belongs to a card position, never to a unit
  or ability (phase-5 plan D-6), so Siege Mode is `E` (as in StarCraft II) only because it lands on the second
  position after Attack, and Burrow is `Q`, where StarCraft II uses `R`.

## Question

**Background.** Plan [`2026-11-09-client-mode-commands`](../../plans/2026-11-09-client-mode-commands.md) adds mode
commands to the skirmish card (Siege Mode and Tank Mode for the Siege Tank analogue, Burrow and Unburrow for the
Lurker analogue), built from catalog abilities, for [Q0070](0070-lurker-without-burrow-button.md). The card's keys
are positional: fifteen slots `Q E R T Y / F G J K L / Z C V B N` in reading order, with `W A S D` panning the
camera, Stop on `X` and Hold on `H` (`crates/client/src/input_bindings.rs`). Phase 5 chose this so that "no key
belongs to any unit".

StarCraft II gives each ability its own default key (`GameHotkeys.txt`): Siege Mode `E`, Unsiege `D`, Burrow `R`,
Lurker Unburrow `R`, Stimpack `T` (`liberty` GameHotkeys.txt:942, :1199, :115, :1035; `swarm` :78). With positional
keys our keys are:

| Selection | Ours | StarCraft II |
|---|---|---|
| Siege Tank analogue, tank mode | Attack `Q`, Siege Mode `E` | Siege Mode `E` |
| Sieged | Attack `Q`, Tank Mode `E` | Unsiege `D` (`D` pans here) |
| Mixed | Attack `Q`, Siege Mode `E`, Tank Mode `R` | `E` and `D` |
| Lurker analogue, unburrowed | Burrow `Q` | Burrow `R` |
| Burrowed | Unburrow `Q` | Unburrow `R` |

**Options.**

1. **Keep positional keys (default).** No key belongs to a unit; keys never collide with the camera. Players who
   know StarCraft II's grid keys learn the card's.
2. **Per-ability hotkeys from catalog data.** Each ability names its key in `assets/catalog/units.ron` (StarCraft
   II's by default). This undoes phase-5 D-6, needs a rule for keys that collide with panning (`D`, `S`, `A`, `W`)
   and between abilities of a mixed selection, and a check that a catalog key is free.
3. **StarCraft II's grid positions.** Keep positional keys but place each entry at its StarCraft II card position
   (`LayoutButtons` row and column in UnitData), so the same key lands on the same command as in the game where the
   grid keys agree.

**Recommendation.** Option 1 until a player asks; option 3 is the smaller step if the difference matters.

## Answer
