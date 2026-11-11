# Q0070: The skirmish client cannot burrow the Lurker; keep the fix until it can?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** keep the fix as landed. The Lurker analogue (`burrow_scorpion`) attacks only burrowed,
  as in StarCraft II, so in a skirmish, where the client has no button for abilities, a trained Lurker can move but
  never attack until the client can cast its Burrow toggle.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP13b fixed
bug B-42: StarCraft II's unburrowed Lurker has no weapon (`liberty` UnitData:690-758), the burrowed one has
(`void` UnitData:775). The simulation now models that with the Lurker analogue's `burrowed` mode and a Burrow toggle
ability (commit `7e5e524`; evidence
[`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §15).

The skirmish client refuses `CastAbility` on purpose: "abilities have no UI in Phase 5" (plan
[`2026-11-02-phase-5-playable-client`](../../plans/2026-11-02-phase-5-playable-client.md), Orders;
`crates/client/src/battle_pipeline/commands.rs`). The Siege Tank analogue has the same gap but still fights in tank
mode. The Lurker has no unburrowed weapon, so in a skirmish it is now useless, where before the fix it attacked
unburrowed (wrongly) and was usable.

**Options.**

1. **Keep the fix (default).** The missing button is a missing client feature, visible as such, in line with
   "missing functionality shows" (Q0051). A client package adds a Burrow (and Siege Mode) command: one hotkey and
   card button that submits `CastAbility` for the selection's mode toggle.
2. **Revert the data until the client can burrow.** The Lurker attacks unburrowed again (B-42 reopened) until the
   client package lands.
3. **Burrow automatically in skirmish.** A client-side rule that burrows an idle Lurker near enemies. StarCraft II
   has no such rule for players (the `TacticalBurrowLurkerMP` validators of `void` ValidatorData:1021-1049 serve
   the computer AI), so this would be a deviation.

**Recommendation.** Option 1, and schedule the client command next to WP13's remaining packages.

## Answer

**Option 1 (Keep the simulation fix and schedule the client command button/hotkey for mode toggles).**

**Engine insight:** Simulation rules should stay faithful to SC2 (`LurkerMP` unarmed above ground, armed only when burrowed as `LurkerMPBurrowed`) rather than carrying temporary rule hacks for missing UI controls. Scheduling a small client work package to add a mode-toggle command button and hotkey (`CastAbility`) to the skirmish HUD cleanly unblocks both the Lurker (`Burrow` / `Unburrow`) and the Siege Tank (`Siege Mode` / `Tank Mode`) at the same time.

## Resolution

Option 1, as the owner answered: the simulation fix stays, and the skirmish client got the mode commands. Plan
[`2026-11-09-client-mode-commands`](../../plans/2026-11-09-client-mode-commands.md) built them from catalog
`ToggleMode` abilities, so Siege Mode / Tank Mode and Burrow / Unburrow (and any later mode toggle) appear on the
command card without naming a unit: `97fd318` (read-only simulation queries), `ea3cb75` (a mode switch without Shift
replaces the unit's orders) and `885002a` (a queued switch reaches an anchored unit), both measured bit-identical on
the whole matrix, battle 33 and the golden replays; `99a97c8` (the catalog's `leave_name`, `PlayerView` mode states and
ability locks); `11a5874` (client admission with the direction checked again at the tick); `a007e4c` (card entries
and the `Transforming` lock); `372b165` (`battle_mode_tests`, through input alone); `c0d4ca8` (records). Keys stay
positional meanwhile ([Q0072](0072-per-ability-hotkeys.md)).

**Remaining gap: media.** The Lurker analogue (`burrow_scorpion`) has no media yet
(`assets/textures/burrow_scorpion.flipbook.ron` is absent), and the client stops a battle that would present it, so a
skirmish still cannot train or show it: its train entry is locked "No approved media". The input test therefore
proves the burrow-and-kill path on the presentable Siege Tank analogue with its own weapons removed from the
session's catalog, and uses the Lurker analogue unchanged once its media ships (plan Amendment 1). Its media belongs
to the Veo plan.
