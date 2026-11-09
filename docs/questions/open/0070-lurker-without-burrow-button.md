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

**Option 1 built (2026-11-09), while the question stays open.** Plan
[`2026-11-09-client-mode-commands`](../../plans/2026-11-09-client-mode-commands.md) added a Burrow and Siege Mode
command to the skirmish client: `97fd318`, `ea3cb75`, `885002a` (simulation queries and two measured fixes, every
matrix battle and golden replay bit-identical), `99a97c8` (catalog `leave_name`, `PlayerView` mode states), `11a5874`
(client admission), `a007e4c` (card entries), `372b165` (`battle_mode_tests`, through input alone). One limit
remains for the Lurker analogue itself: it has no media yet, so the skirmish cannot train or show it (its card entry
is locked "No approved media"); the input test proves the burrow-and-kill path on the presentable Siege Tank
analogue with its own weapons removed, and switches to the Lurker analogue once its media ships.

## Answer
