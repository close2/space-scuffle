# Blueprint: Effect Visualization (Composable Effect System; Burrow, Cloak and Detection, Modes, Splash, Areas, Tethers, Status)

## Revision 1 (owner feedback, 2026-11-09)

**Owner (chat, 2026-11-09), verbatim:** "I would love for these GPU effects to stay generic. can we create some, maybe
even combine some for unique appearances, so that unit definitions chose the effect. otoh this is something which has
to look professional, no cheap looking effects. maybe having generated images included in the effect could make this
possible? please think about this once more. are there example implementations, algorithms on the internet?"

**What changed against the first version (`e30506d`):**

1. **A generic, composable effect system replaces the per-class shader kinds.** The first version added new
   `combat_fx.wgsl` kinds (6 to 10) and a look per splash class (`AcidBurst`, `Blast`, …) chosen in a client file
   keyed by unit slug. This revision builds eight data-parameterised **building blocks** (particles, flipbook sprite,
   ground decal, ribbon, ring, shell modulation, distortion, light; §5.1), combined into named **effect definitions**
   in `assets/catalog/effects.ron` (§5.2). **Unit definitions choose their effects** in a new presentation block of
   `units.ron` / `structures.ron` (§5.3), next to `veo_prompts`; Rust names no unit and no effect (C5), and the files
   are read at run time (C6). Sizes and timings bind to simulation data (splash zones, line length, channel progress,
   area remaining time) so an effect is as large and as long as the mechanic (§5.4).
2. **Real texture content.** Professional effects are built from authored greyscale textures, flipbooks, colour
   ramps, erosion and soft blending, not from purely procedural shader maths (research §4). The textures come from
   generated images and Veo clips on pure black, baked by `asset_baker` into channel-packed atlases with provenance,
   and from offline-generated noise sheets (§6). Whole-shape procedural drawing remains only where it is the right tool
   (rings, ribbons' cores, distortion).
3. **Research section with sources** (§4): engine systems (Niagara, VFX Graph, Godot, PopcornFX, Effekseer), the Bevy
   crates (`bevy_hanabi`, `bevy_enoki`, `bevy_firework`, `berdicles`, decal crates), the standard quality techniques,
   and generated-image pipelines (models, prices, alpha).
4. **Build versus adopt** (D-10): adopt `bevy_hanabi` 0.15.x (MIT OR Apache-2.0, Bevy 0.15, wgpu 23) for GPU particle
   emitters behind a thin adapter, after a go/no-go spike; build decals, flipbook sprites, ribbons, rings, shell
   hooks and distortion ourselves on retained instance buffers.
5. **Quality bar and review** (§7): a written checklist, reference board, contact sheets for textures and for whole
   effects (as in the Veo review), golden stills with tolerance on radv and lavapipe, frame-budget limits.
6. **Work packages re-ordered** (§9): contract and leak fix first (WP1 to WP3), then the framework (WP4, WP5), then a
   **vertical slice of three showcase-quality effects** (Siege-Tank shell impact, Baneling acid burst, Lurker spine
   line, WP6) reviewed against the quality bar before anything scales out (WP7 onwards).
7. **Media budget recomputed** (§6.6) and questions renumbered: Q0075 (Veo for morph and mode clips, unchanged
   meaning), Q0076, Q0077 unchanged, **Q0078 new**: a Google image model (a new paid model) for effect textures.

Kept unchanged in substance: the inventory (§2), the StarCraft II reference (§3), the checksum-neutral
`RenderSnapshot` / `PlayerView` contract (D-1), concealment as data (D-3), the fog, detection and picking rule with
the burrow leak fix first (D-5), the burrow timing rule (D-6), links and areas as general contracts (D-8), the stop
rules (§10).

---

**Owner request (chat, 2026-11-09):** "please use one round to evaluate how to visualize burrowed units, splash damage
(banelings,...) and other effects (viper pull, lurker,...)".

**Status of this document:** Stage 1 blueprint under charter §6, first written on 2026-11-09 against `main` at
`0a36b3f`, revised the same day against `e30506d`. It is an evaluation and a plan. Nothing was built, rendered or
generated while planning (read-only inspection of the code, the catalog, the media ledger, the locked Bevy 0.15.3
sources in the Cargo registry, and web research), so every claim about pixels, timings or costs is a target that a
work package measures. Line numbers drift, so search for the quoted symbol. Claims not confirmed are marked **verify**.

**Kind of change.** A full plan (charter §6, Q0054): it spans `sim_types`, `sim_core`, `client`, `asset_baker`,
`client_diagnostics` and `gate`, adds snapshot and view records, a new client dependency (`bevy_hanabi`), new catalog
data (`effects.ron`, a presentation block in `units.ron` and `structures.ron`) and a new kind of media. It is designed
so that **no battle outcome, golden replay or checksum moves** (D-1); a work package that sees a golden move stops
(§10).

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) / [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) follow StarCraft II; S-6 original looks | Standing rules | §3, D-3 |
| [Q0020](../questions/closed/0020-smog-velocity-quiescence.md) answer: "The agent is always allowed to decide visual questions" | Standing rule | §7, D-11 |
| [Q0069](../questions/open/0069-lurker-burrow-sections.md) when a burrowing Lurker becomes burrowed | Answered (option 2: `Stats` at 33 ticks, `Actor` animation to 45) but not yet implemented | D-6 |
| [Q0070](../questions/open/0070-lurker-without-burrow-button.md) no Burrow button in the skirmish | Open, default option 1; plan `2026-11-09-client-mode-commands` adds the button | §2.4, WP12 |
| [Q0075](../questions/open/0075-veo-morph-and-mode-clips.md) (filed) Veo spend for morph and mode unit clips | Filed by WP0 (2026-11-09), default "no spend; the effect system carries burrow and siege" | D-4, WP13 |
| [Q0076](../questions/closed/0076-undetected-cloaked-enemies.md) (filed) what an enemy sees of an undetected cloaked unit | Filed by WP0 (2026-11-09), default "shimmer, as StarCraft II" | D-5 |
| [Q0077](../questions/closed/0077-caster-spells.md) (filed) the missing caster spells (Abduct, Psionic Storm, Blinding Cloud, …) | Filed by WP0 (2026-11-09), default "presentation contract only here; mechanics in a later plan" | D-8, §12 |
| [Q0078](../questions/closed/0078-image-model-for-effect-textures.md) a Google image model for effect textures (new paid model) | Filed by WP0, **Blocking: no**; default changed by the orchestrator to **"yes, within $25.25 of the $200 total"**, because the owner proposed generated images in chat (2026-11-09) | §6, D-4 |

Question numbers: `open/` and `closed/` on `main` end at Q0071; Q0072 to Q0074 are taken on the `friendly-blocking`
branches; Q0075 to Q0077 were reserved by the first version of this plan and are still unfiled; Q0078 is the next
free number (checked on every local branch on 2026-11-09).

---

## 1. Summary

1. **What the player cannot read today** (unchanged, §2): an undetected burrowed enemy is drawn to every viewer (an
   information leak); splash damage has no presentation at all; mode transitions, barrier, dash and slow are
   invisible; projectiles have no impact; Abduct and storms do not exist in the simulation.
2. **How professionals build this** (§4): every engine studied (Niagara, VFX Graph, Godot, PopcornFX, Effekseer,
   `bevy_hanabi`) composes effects from a small set of generic emitters and renderers, parameterised by data, with
   the effect bound to gameplay through exposed parameters ("user parameters", "properties", "data interfaces").
   Quality comes from a handful of well-known techniques used together: authored greyscale textures and flipbooks
   recoloured by gradient ramps, alpha erosion instead of plain fades, soft depth intersections, premultiplied
   blending that mixes additive glow and smoke in one pass, HDR cores with bloom, several layers per effect (flash,
   core, smoke, debris, decal, light) and deliberate timing (anticipation, impact, dissipation).
3. **The design** (§5): eight building blocks, effect definitions in `assets/catalog/effects.ron` that compose them
   into layers with curves and data bindings, and a presentation block in each unit's definition that picks the
   effect for each weapon, mode, status and death. The client resolves names to dense indices once at load; at run
   time every effect is an index into retained, pooled instance storage, with a per-block budget and level-of-detail
   rule, and no per-frame allocation.
4. **Real texture content** (§6): textures are generated on pure black (Veo clips now, a Google image model if Q0078
   allows), keyed by luminance or by a black/white difference matte, channel-packed by `asset_baker` into atlases
   with provenance, reviewed on contact sheets, and tinted at run time by colour ramps, so one greyscale texture
   serves many looks. Noise sheets are generated offline by the baker as authored assets.
5. **Build versus adopt** (D-10): adopt `bevy_hanabi` for particles after a spike; build the other blocks.
6. **Contract** (unchanged in substance, D-1): `concealment`, `stance`, `status`, barrier ratio per bubble;
   `SplashVisual` events; `areas` and `links` lists; `PlayerView::revealed`; and new: every weapon event carries an
   `EffectSource` tag (source type and weapon key) so the client finds the unit's chosen effect. Nothing hashed
   changes.
7. **Order** (§9): WP1 to WP3 contract, determinism proof and the leak fix; WP4 and WP5 framework and texture
   pipeline; WP6 the three-effect vertical slice and its quality review; WP7 to WP11 scale-out; WP12 to WP15
   integration, media, showcase, bookkeeping.

---

## 2. Inventory: What Exists, What Is Published, What Is Drawn

### 2.1 Sources read

- Catalog: `assets/catalog/units.ron` (weapons, splash, modes, abilities, `is_detector`), `structures.ron`
  (detectors), `media_style.ron`, `assets/media/*.ron`, `docs/evidence/veo/ledger.jsonl`.
- Simulation: `sim_types::snapshot` (`RenderSnapshot`, `BubbleRenderInstance`, `CombatEventVisual`),
  `sim_types::player_view`, `sim_types::commands` (`UnitModeSpec`, `AbilityEffectSpec`, `BarrierSpec`,
  `AttackDashSpec`), `sim_types::weapon` (`SplashSpec`, `SplashShape`), `sim_core::world::snapshot`,
  `combat_resolution.rs` (`emit_instant_hit_visual`, `apply_splash`, projectile launch), `entity.rs`
  (`apply_mode_transition`, buffs, `BarrierState`), `world/abilities.rs`, `transport/lifecycle.rs`
  (`BubbleShatter`), `published_actions.rs`, `checksum/encoding.rs`.
- Client: `vfx.rs` and `vfx/pool.rs` (combat effects, pooled materials), `assets/shaders/combat_fx.wgsl` (fx kinds
  0–5), `bubble_shader.rs` and `assets/shaders/bubble_dome.wgsl` (shell flags, cloak branch),
  `battle_pipeline/fog.rs` (`BattleFog::shows`, `reveals`), `battle_hud/fog.rs` (`apply_battle_fog`),
  `battle_pipeline/driver.rs` (`publish_combat_events`), `assets/shaders/selection_ring.wgsl` (ground ring
  precedent), the steady-state and allocation tests.

### 2.2 Inventory table

"Sim" is what `sim_core` models; "Published" what reaches `RenderSnapshot` / `PlayerView`; "Client" what is drawn
in the live battle; "Gap" what a player misses.

| # | State or effect | Catalog users (analogue) | Sim | Published today | Client today | Gap |
|---|---|---|---|---|---|---|
| 1 | Burrowed (concealed, immobile) | `burrow_scorpion` (Lurker), `burrowed` mode, `is_cloaked: true` | Yes (`SwitchMode`; `Entity::is_cloaked`, `active_mode`) | `cloaked: bool` only; no burrowed vs cloaked distinction | Shell alpha 0.15–0.40 with a sine shimmer (`bubble_dome.wgsl` §9), **for every viewer** | Enemy undetected burrowed unit is **drawn** (leak); no in-ground look; detected vs undetected indistinguishable; it can be inspected |
| 2 | Burrowing / unburrowing | same; 45 + ≤4.46 ticks in, 11 + ≤4.46 out | Yes (`channeling_remaining_ticks`, `pending_ability`) | Nothing (`anim_state` stays as set; channel not exposed) | Nothing | No transition; the change is a pop at the channel's end |
| 3 | Burrowed moving | none (no Tunneling Claws, B-38) | No | — | — | Out of scope until B-38 |
| 4 | Cloaked (not burrowed) | none in shipped data: `shadow_visitor` (Dark Templar), `stealth_eye_orb` (Observer) and `cloak_gunship` (Banshee) have no cloak; `CloakToggle` effect exists unused; upgrades `personal_cloaking`, `cloaking_field` exist | Engine yes, data no | `cloaked: bool` | Generic shimmer, every viewer | Correct look for the enemy is "shimmer, not targetable" (Q0076); own view needs a clear "I am hidden" read |
| 5 | Detection | detectors: `omni_probe_drone`, `stealth_eye_orb`, three structures | Yes (`VisionGrid::is_detected`, per player) | **Nothing**: `PlayerView` has `visibility` only | Nothing | Client cannot tell revealed from hidden |
| 6 | Siege / mode stance | `railgun_crawler` `siege_mode` (immobile, sieged weapon) | Yes | `weapon_visual` switches to the sieged weapon's kind; nothing else | Same clip, same shell | Sieged vs tank mode unreadable, especially for enemies; transition invisible |
| 7 | Mode transition progress | same | Yes (channel) | No | No | As 2 |
| 8 | Splash, circle | `railgun_crawler` sieged (0.47/0.78/1.25), `napalm_wheeler` (2.2), `sky_bastion` (1.5), `tripod_annihilator` (1.8/2.4/3.0), `burrow_scorpion` (1.5/2.0/2.5), `volatile_tick` (2.2, suicide), `stag_behemoth` (0.5/1.0/1.5) | Yes (`apply_splash`, zones) | **No event**; only the direct hit's tracer/projectile/slash | Nothing | No area read at all |
| 9 | Splash, line | engine `SplashShape::Line`; shipped data holds circles (B-18, B-19) | Engine yes | No | No | Must follow the data automatically when B-18 lands |
| 10 | Baneling burst | `volatile_tick`: `visual_kind: Projectile`, `suicide_on_attack` | Yes | A `ProjectileVisual` from the attacker and a `BubbleShatter` | Plasma orb card and generic glass shatter | No acid burst sized 2.2; centre is the target (B-83) |
| 11 | Lurker spines | `burrow_scorpion`: `visual_kind: MeleeSlash`, range 8 | Yes | `MeleeSlashVisual` at the target, radius 0.5 | A 0.2 s crescent at the target, 8 units from the Lurker | No line from the Lurker; wrong vocabulary |
| 12 | Hellion flame, Colossus lance | `napalm_wheeler` (Hitscan), `tripod_annihilator` (LaserBeam) | Yes | Tracer / beam | Tracer ribbon / laser ribbon | No flame jet, no sweep |
| 13 | Ultralisk cleave | `stag_behemoth` (MeleeSlash, circle splash) | Yes (circle, B-19) | Slash | Crescent | No cleave area |
| 14 | Hazard zones (Storm-like) | none in data | Engine yes (`active_hazards`) | **No** | No | Nothing; needed the day a spell lands |
| 15 | Liberator zone, Widow-Mine-like | `sky_bastion` has no defender mode; no mine | No | — | — | Out of scope (mechanics) |
| 16 | Abduct / pulls / tethers | `tongue_chameleon` has no abilities | No | — | — | Contract only (D-8, Q0077) |
| 17 | Channelled / beam weapons | `prismatic_saucer`, `flying_saucer`, `prism_weaver` (LaserBeam); no LightningArc user | Per strike | `BeamVisual` per strike | 0.25 s ribbon per strike | Adequate; a sustained beam could join consecutive strikes (client-only, low priority) |
| 18 | Stim / overclock | `pulse_bot`, `shock_golem` | Yes (buff 1) | `speed_boosted` | Wake intensity, `FLAG_SPEED_BOOSTED` rim pulse | Adequate; folded into `status` |
| 19 | Concussive slow | `shock_golem` shells (buff 2) | Yes | No | No | Slowed units unreadable |
| 20 | Charge (attack dash) | `psi_grey` `attack_dash` | Yes (buff 3) | No | No | Dash unreadable |
| 21 | Barrier | `void_goliath` `barrier` (absorb 100, 2 s) | Yes (`BarrierState`) | No | No | Barrier up/absorbing unreadable |
| 22 | Shield hit vs hull hit | every unit | Yes (8-tick flashes, hit angle) | `shield_hit_flash`, `hull_hit_flash`, `hull_hit_angle_rad` | Shield surge vs hull surge in the shell | Adequate; barrier absorb is neither |
| 23 | Projectile in flight | every `projectile_speed` weapon | Yes | `ProjectileVisual` at release with flight time | Orb card flies straight for the flight time, then vanishes | **No impact flash** at arrival |
| 24 | Death | every entity | Yes | `BubbleShatter` | Starburst + 16 glass shards | Adequate; no lingering scorch/wreck (optional) |
| 25 | Attack of an anchored unit | sieged tank, burrowed Lurker | — | `anim_state` never `Attack` for an immobile unit (locomotion returns early), but `attack_swing` and `is_attacking` are published (**verify** in a sieged-tank world) | Idle clip while firing | Client can time its attack clip from `attack_swing`; a sim fix would move checksums (`current_anim` is hashed), out of scope |

### 2.3 What the catalog references with no presentation at all

- Every `splash` block (8 weapons) and every splash zone fraction.
- The `burrowed` mode's `is_cloaked` (presented wrongly, see row 1), and the `siege_mode` stance as such.
- `barrier`, `attack_dash`, the concussive slow.
- `is_detector` (no detection range or reveal shown).
- `AbilityEffectSpec::SpawnHazardZone`, `BlinkTeleport`, `CloakToggle`: engine effects with no presentation (and no
  shipped user).
- `veo_prompts.Attack` of `burrow_scorpion` describes "linear ripple of jagged subterranean spikes": the unit clip
  cannot show spikes eight units away; that is a world effect.

### 2.4 Related plans and order

- `2026-11-09-client-mode-commands` (Proposed) adds `PlayerView::mode_states` for the viewer's own units and the
  Burrow/Siege buttons. This plan's `stance` is per bubble, for every visible unit including enemies; it reuses the
  same settled-mode rule (`Entity::settled_mode`) if that plan lands first, otherwise WP2 adds the read-only query
  and that plan reuses it.
- `2026-11-09-friendly-blocking` WP6 (R6, B-93) needs a "burrowed" property of a mode. This plan adds it as catalog
  data (`UnitModeSpec::is_burrowed`, the game's `Buried` flag, D-3); whichever lands first adds the field, the other
  reads it.
- `2026-11-05-sc2-deviation-fixes` B-18/B-19/B-85 (line splash, cleave, spaced line searches): the `SplashVisual`
  event is emitted per search point at the tick of the search, so those fixes change the picture with no client
  change.

---

## 3. StarCraft II Reference (Readability Target; Looks Stay Original, S-6)

Liquipedia blocked automated fetches during planning (HTTP 403), so the reference rests on Blizzard's own pages and
patch notes, on the community wiki and on community threads; each claim below names its source. Behaviour that only
the game client shows (exact shimmer strength, decal timings) is to be checked in the game or a replay by the
implementer where it matters and is marked **verify**.

| Class | StarCraft II presentation | Sources |
|---|---|---|
| Burrowed | The owner sees the unit in a hole of dirt, lower than the ground, with a visible outline. An enemy sees **nothing** unless a detector covers it; detected, the unit is drawn normally, partly sunk, and can be targeted. Splash and areas still hurt it undetected. The burrow is an animated dig with a dust burst; the Lurker's dig is long (2.5 Normal s, `Actor` section, Q0069). | Blizzard game guide "Detection" (<https://news.blizzard.com/en-us/article/4546770/game-guide-detection>): detectors reveal "cloaked or burrowed units", damage through "area of effect abilities like psionic storm, or … splash damage"; Liquipedia Spore Crawler (<https://liquipedia.net/starcraft2/Spore_Crawler>): sees "cloaked, burrowed and hallucinated units"; Q0069 evidence (`swarm` AbilData:1654-1674) |
| Cloaked | The enemy sees a faint refractive **shimmer** where the unit is, cannot select or target it; the owner sees it semi-transparent. Detected, it is drawn normally. | Blizzard forum "Fave ways to deal with cloaked/burrowed units w/o detector" (<https://us.forums.blizzard.com/en/sc2/t/fave-ways-to-deal-with-cloakedburrowed-units-wo-detector/28048>): "if you can make out the distortion of cloaked units, just fire upon that"; burrowed units "can be located by their attack effects, such as spines shoot out from Lurkers"; StarCraft Wiki "Detector" (<https://starcraft.fandom.com/wiki/Detector>) |
| Lurker spines | A straight line of spines erupting from the Lurker towards the target and on to full range; the spines reveal where the Lurker is, not the Lurker. Range 8 (5.0.14), Seismic Spines +2. | Battle.net Lurker description (<https://classic.battle.net/scc/zerg/units/lurker.shtml>): spines "rip up through the ground in a straight line … to maximum range"; Blizzard balance update 2019-11-14 (<https://starcraft2.com/en-gb/news/23223600>): range 9 → 8, Seismic Spines 8 → 10; B-85 (spines 0.125 Normal s apart) |
| Splash (Baneling, tank, Hellion, Colossus, Ultralisk) | No damage numbers. A burst sized to the area: Baneling acid splash on the ground with a green stain; Siege Tank shell explosion at the impact; Hellion flame line; Colossus lance sweep line with burn marks; Ultralisk cleave arc. The area is readable by the effect's size alone; no range ring. | Liquipedia unit statistics (<https://liquipedia.net/starcraft2/Terran_Unit_Statistics>, Baneling 2.20 AoE in the table); TeamLiquid "Splash and AoE in SC1 and 2" (<https://teamliquid.net/blogs/148463-splash-and-aoe-in-sc-1-and-2>); catalog radii (`units.ron`, sourced in plan `2026-11-04-combat-sc2-alignment`) |
| Persistent areas (Psionic Storm, Blinding Cloud, Liberator zone) | Storm: a flickering psionic area of exactly its radius (1.5 at 5.0.14, later 2 or 2.25), visible to both players when in sight. Liberator: a ground circle drawn for both players. | Blizzard 5.0.15 patch notes (<https://news.blizzard.com/en-us/article/24225313/starcraft-ii-5-0-15-patch-notes>): "Psionic Storm radius increased from 1.5 to 2"; Liquipedia "Spells" (<https://liquipedia.net/starcraft2/Spells>) |
| Abduct (pull) | A tongue from the Viper hooks the target and drags it to the Viper's position over a short time; 0.57 s cast finish. | Liquipedia "Spells": "Pulls a target unit to the location of the Viper"; Blizzard 5.0.11 patch notes (<https://news.blizzard.com/en-gb/article/23893118/starcraft-ii-5-0-11-patch-notes>), cast finish time; TL.net "Vipers, flying and unnerving casters" (<https://tl.net/forum/starcraft-2-hots/368971-vipers-flying-and-unnerving-casters>) |
| Status | Stim: red tint and fumes; Concussive: slowed units show a stun overlay; Charge: a short blur trail; Barrier (Immortal): a golden shell over the unit while it absorbs. Shield hits ripple the shield bubble, hull hits spark. | Observed game behaviour (**verify** strength in a replay); `liberty`/`void` data cited in plan `2026-11-05-sc2-deviation-fixes` (B-41, B-43) |
| Damage text | None. | Game default |

**Readability rules derived for this plan.** (R1) An undetected concealed enemy shows nothing that the game would not
show: burrowed nothing, cloaked a shimmer (Q0076). (R2) Every area effect is drawn at the simulation's radii, with
the full-damage zone readable inside the reach. (R3) Effects are judged where they land for fog (existing
`reveals`); an effect by an undetected unit is shown, its source is not. (R4) A mode switch is visible for its whole
channel, and the stance is visible afterwards. (R5) No damage numbers.


---

## 4. Research: How Professional Games Build Composable, High-Quality Effects

All sources were read on **2026-11-09**. Prices and model availability change often; a work package that spends money
re-reads the price page on the day and records it in the ledger (§6.6).

### 4.1 Engine and middleware systems

| System | What it shows for this plan | Source |
|---|---|---|
| Unreal Niagara | An effect (*system*) is a set of *emitters*; an emitter is a stack of *modules* run top to bottom in spawn and update groups; *parameters* expose values to gameplay, and *data interfaces* feed external data (meshes, curves, collision) into the simulation. Generic modules plus exposed parameters are the composition model this plan copies at a smaller scale. | Epic, "Overview of Niagara Effects" <https://dev.epicgames.com/documentation/en-us/unreal-engine/overview-of-niagara-effects-for-unreal-engine>; "Niagara key concepts" (UE 4.26) <https://docs.unrealengine.com/4.26/RenderingAndGraphics/Niagara/NiagaraKeyConcepts>; `UNiagaraDataInterface` <https://docs.unrealengine.com/4.27/API/Plugins/Niagara/UNiagaraDataInterface/index.html> |
| Unity Visual Effect Graph | Contexts (Spawn, Initialize, Update, Output) and *blocks* inside them; outputs are separate renderers (quad, mesh, strip/trail, decal). Separates *what particles do* from *how they are drawn*, as this plan's emitter versus renderer split does. | Unity, "Contexts" (VFX Graph 10.5) <https://docs.unity.cn/Packages/com.unity.visualeffectgraph@10.5/manual/Contexts.html>; "Systems, Contexts and Blocks" (6.7) <https://docs.unity3d.com/Packages/com.unity.visualeffectgraph@6.7/manual/Systems-Contexts-and-Blocks> |
| Godot `GPUParticles3D` | GPU emitter with a process material, sub-emitters (sparks on collision, fireworks) and ribbon/tube trails. Same building-block set, one node per emitter. | Godot class reference 4.2 <https://godot.readthedocs.io/en/4.2/classes/class_gpuparticles3d.html> |
| PopcornFX | Free editor; engine integrations source-available under a community licence; the runtime SDK is commercial and C++. Not usable here (no Rust/wgpu runtime, commercial runtime). | Game Developer, "PopcornFX makes its products and services more accessible and open" <https://gamedeveloper.com/art/popcornfx-makes-its-products-and-services-more-accessible-and-open>; plugin repository <https://github.com/PopcornFX/UnrealEnginePopcornFXPlugin> |
| Effekseer | MIT-licensed editor and C++ runtime with its own renderers (DirectX, Metal, Vulkan, OpenGL, WebGL); no WebGPU backend and no Rust or wgpu binding found. Integrating it would mean a C++ FFI runtime that cannot share Bevy's wgpu device. Rejected; its editor's layer model (sprite, ribbon, ring, track, model nodes) confirms the block list. | <https://github.com/effekseer/Effekseer> |

### 4.2 Bevy crates (Bevy 0.15.3 and wgpu 23.0.1 are locked in this workspace)

| Crate | Version for Bevy 0.15 | Fit | Source |
|---|---|---|---|
| `bevy_hanabi` | **0.14 (2024-12-09) and 0.15.0 (2025-04-01), 0.15.1 (2025-04-23)**; 0.15.x depends on wgpu ^23.0.1 | MIT OR Apache-2.0. GPU compute particles; spawners (rate, once, burst), position/velocity modifiers, colour and size over lifetime (gradients), camera-facing or velocity-aligned orientation, `FlipbookModifier` (since 0.8), texture slots via `EffectMaterial` (since 0.13; a multi-texture codegen bug fixed in 0.15.0), alpha modes including mask, ribbons via `Attribute::RIBBON_ID` (0.15.0), GPU spawn events and parent effects (0.15.0), `EffectAsset::prng_seed` (0.15.0, per instance only from 0.16), `EffectProperties` set per instance, its own `Time<EffectSimulation>` clock. Works on wasm only through WebGPU (compute shaders; no WebGL2). Its CI ran GPU tests on lavapipe (since 0.2.0). No soft-particle (depth fade) feature listed. Presentation only: it lives in `client`, never in a simulation crate (C2). | README <https://github.com/djeedai/bevy_hanabi>; CHANGELOG <https://raw.githubusercontent.com/djeedai/bevy_hanabi/main/CHANGELOG.md>; docs.rs 0.15.1 <https://docs.rs/bevy_hanabi/0.15.1/bevy_hanabi/> |
| `bevy_enoki` | 0.3.3 | A 2D particle system (sprites in the 2D pipeline); not for our 3D battle camera. Rejected. | lib.rs <https://lib.rs/crates/bevy_enoki> |
| `bevy_firework` | 0.6 | CPU-simulated particles batched into one draw per system; custom particle textures appear only in a later README (0.9), the 0.8 README still lists "can't use custom images". Rejected for 0.15 (no textures). | docs.rs 0.6.0 <https://docs.rs/crate/bevy_firework/0.6.0>; 0.8.0 README <https://docs.rs/crate/bevy_firework/0.8.0/source/README.md>; 0.9.1 README <https://docs.rs/crate/bevy_firework/0.9.1/source/README.md> |
| `berdicles` | 0.3 for Bevy 0.15 | Small CPU particle crate with a trail module. Not needed if hanabi passes the spike; noted as a fallback reference. | <https://docs.rs/berdicles> |
| Decals | Bevy's built-in `ForwardDecal` (a quad that reads the depth prepass and fades at grazing angles) arrives in **Bevy 0.16**; it is absent from the locked `bevy_pbr-0.15.3` sources (checked in the registry). `bevy_contact_projective_decals` (after the Path of Exile 2 talk) targets Bevy 0.13. | Bevy decal example <https://bevy.org/examples/3d-rendering/decal/>; Bevy 0.16 release notes <https://bevy.org/news/bevy-0-16/>; <https://github.com/naasblod/bevy_contact_projective_decals> |
| Distortion | Bevy 0.15 already offers a copy of the opaque scene to materials that opt in (`Material::reads_view_transmission_texture`, `view_transmission_texture` at view binding 29), rendered in the `Transmissive3d` phase. A screen-space distortion block needs no new engine feature. | Locked sources: `bevy_pbr-0.15.3/src/material.rs`, `render/mesh_view_bindings.wgsl` |

### 4.3 Techniques that make effects look professional

| Technique | What it is and why it matters here | Source |
|---|---|---|
| Layering | A professional explosion is several simple layers: a flash, a fireball flipbook, separate smoke and emissive fire layers coloured by a lookup ramp, small debris and spark particles, a scorch, a light. Battlefront II's explosion uses looping and one-shot flipbooks, smoke and fire in separate alpha channels, colour from a LUT and one shared atlas. | Simon Trümpler, "Battlefront II: Layered Explosion" (2023-04-25) <https://engineered.at/articles/battlefront-ii-layered-explosion>; "How (not) to create textures for VFX" (GDC 2022) <https://simonschreibt.de/gat/how-not-to-create-textures-for-vfx/> |
| Flipbooks and frame blending | Sprite sheets of a simulated or filmed element, played over the particle's life; blending two frames, or warping them along baked motion vectors, hides the low frame count. | vfxdoc, "Flipbooks and Texture Sheets" <https://vfxdoc.readthedocs.io/en/latest/textures/flipbooks/>; Real-Time VFX forum, "Motion Vectors (for animated textures)" <https://realtimevfx.com/t/motion-vectors-for-animated-textures-how-do-you-generate-them/575>; Epic community tutorial on motion-vector flipbooks <https://forums.unrealengine.com/t/community-tutorial-level-up-your-flipbooks-with-motion-vectors-and-subuv-blending-unreal-5-tutorial/2694470> |
| Alpha erosion (dissolve) | Instead of fading opacity, raise a threshold through a greyscale map so the shape breaks up from its thin parts first; a soft step avoids aliasing; a gradient multiplied in makes the erosion directional. Used for smoke, lightning and decals. | vfxdoc, "Alpha erosion" <https://vfxdoc.readthedocs.io/en/latest/shaders/alpha-erosion/>; Real-Time VFX, "Battlerite erosion" <https://realtimevfx.com/t/battlerite-erosion-ue4/4706> |
| Gradient (colour-ramp) mapping | Store shape and shading in greyscale; map luminance through a 1D colour ramp at run time. One texture becomes acid, fire, psionic or frost; team colour enters only through secondary elements. | Same Battlefront II article (LUT colouring); Riot VFX style guide (colour and value rules) below |
| Premultiplied alpha | One blend state (`ONE, ONE_MINUS_SRC_ALPHA`) draws both alpha-blended smoke (alpha > 0) and additive glow (alpha = 0) in one pass and one sort, and filters correctly at edges. | Shawn Hargreaves, "Premultiplied alpha" <https://www.shawnhargreaves.com/blog/premultiplied-alpha.html>; Inigo Quilez, "Premultiplied alpha" <https://iquilezles.org/articles/premultipliedalpha>; Wikipedia "Alpha compositing" <https://en.wikipedia.org/wiki/Alpha_compositing> |
| Soft particles (depth fade) | Fade a card where it meets opaque geometry, `fade = saturate((scene_depth − particle_depth) · k)`, so cards do not show a hard line where they cut the ground. Needs the depth prepass, which the battle camera already has (`smog_volume.wgsl` reads it). | NVIDIA, "Soft Particles" (2007) <https://developer.download.nvidia.com/whitepapers/2007/SDK10/SoftParticles_hi.pdf>; vfxdoc "Fading" <https://vfxdoc.readthedocs.io/en/latest/shaders/fading/> |
| Readability and clarity | Riot's VFX style guide: gameplay clarity first, minimise clutter, a clear focal point with primary and secondary elements, the dangerous area must read; spectacle may be noisy only for big effects. GDC 2017 bootcamp (Riot and Blizzard): "Gameplay is king", "we must be able to see what is happening". | Riot, "League's VFX style guide" (2017) <https://nexus.leagueoflegends.com/en-us/2017/10/dev-leagues-vfx-style-guide/> and PDF <https://nexus.leagueoflegends.com/wp-content/uploads/2017/10/VFX_Styleguide_final_public_hidpjqwx7lqyx0pjj3ss.pdf>; Riot, "Clarity in League" <https://www.leagueoflegends.com/en-us/news/dev/clarity-in-league/>; GDC Vault, "Visual Effects Bootcamp: Artistic Principles of VFX" (Keyser, Riot; Chamberlain, Blizzard) <https://www.gdcvault.com/play/1024439/Visual-Effects-Bootcamp-Artistic-Principles> |
| Timing | Wind-up (anticipation), climax, follow-through, dissipation; a fast attack to the peak and a long eased fall-off is what reads as "punchy"; timing is tuned by eye. | VFX Apprentice, "The Soul of Effects: What is Timing in VFX?" <https://www.vfxapprentice.com/blog/the-soul-of-effects-what-is-timing-in-vfx>; "Master Timing and Punch" <https://www.vfxapprentice.com/blog/learn-timing-for-vfx-with-jason-keyser> |
| HDR cores and bloom | Emissive cores above 1.0 bloom into a glow that additive sprites alone cannot fake. Bevy 0.15 offers `Bloom` on an HDR camera; the battle camera is not HDR today. | `bevy_hanabi` README (HDR camera with bloom in its examples) <https://github.com/djeedai/bevy_hanabi>; decision D-11 |
| Decals | Ground marks projected through the depth buffer, faded at grazing angles; the area of effect is read from the ground. | Bevy decal example (0.16) <https://bevy.org/examples/3d-rendering/decal/> |

### 4.4 Generated images as effect content

| Topic | Finding | Source |
|---|---|---|
| Models on the project's credential path | `asset_baker` calls Vertex AI (`us-central1-aiplatform.googleapis.com`) with an Express key and project (`veo_client.rs`). Imagen 4 (Fast $0.02, Standard $0.04, Ultra $0.06 per image) was retired on Vertex AI on 2026-06-30 and in the Gemini API from 2026-08-17. Its successors are the Gemini image models ("Nano Banana"): `gemini-nano-banana-2.1` $0.0336 per 1K image ($0.0504 at 2K), `gemini-3.1-flash-image` $0.067 at 1K ($0.101 at 2K), `gemini-3.1-flash-lite-image` $0.0336 at 1K, `gemini-3-pro-image` $0.134; batch is about half. `gemini-2.5-flash-image` ($0.039) shuts down on 2027-03-15. **Verify** on the day which ids the Vertex Express key may call. | Google AI Studio pricing <https://aistudio.google.com/docs/pricing>; tokencost, "Imagen 4 shutdown" (2026-08-10) <https://tokencost.app/blog/imagen-4-shutdown-gemini-3-1-flash-image-cost>; Google, "Imagen 4 Fast" <https://developers.googleblog.com/en/announcing-imagen-4-fast-and-imagen-4-family-generally-available-in-the-gemini-api/> |
| Alpha | Image models do not emit usable alpha: a benchmark found 23 of 26 models score zero on real transparency; Gemini returns white or generated backgrounds for "transparent". The workable methods are generation on a flat key colour and deterministic removal afterwards. For effects the industry-standard key is **pure black** (emissive elements are additive, luminance is the alpha), and for lit, semi-transparent elements a **difference matte** from the same image on black and on white (Smith and Blinn's triangulation matting) recovers colour and alpha exactly where the two renders agree. Chroma green spills into soft smoke edges and is rejected. | img.ly, "Transparency" benchmark <https://img.ly/ai-benchmarks/findings/transparency/>; Google AI forum, "Unable to create transparent PNGs" <https://discuss.ai.google.dev/t/unable-to-create-transparent-pngs/92868>; Phil Schmid, "Transparent PNG stickers with Nano Banana Pro" <https://www.philschmid.de/generate-stickers>; A. R. Smith, J. F. Blinn, "Blue Screen Matting", SIGGRAPH 96, pp. 259–268 <https://www.alvyray.com/Papers/CG/blusig96.pdf> (triangulation needs the two backings to differ at every pixel); premultiplied-alpha sources above (additive textures must sit on black) |
| Licensing of generated output | Google states it will not claim ownership of generated content and may generate similar content for others; the developer is responsible for use. Paid use is not used for training. | Gemini API Additional Terms <https://ai.google.dev/gemini-api/terms> (archived 2025-12-18 <https://ai.google.dev/gemini-api/terms-archive/terms_12_18_25>) |
| Veo for effect flipbooks | Veo 3.1 Fast (already approved and budgeted, $0.40 a clip) produces opaque video; an element on pure black, filmed side-on or top-down, keyed by luminance, is a usable flipbook source for camera-facing elements (fireball, smoke plume, acid crown, spines erupting). It is unfit for size-exact ground areas, which stay decals and rings sized by the simulation (first version §4.1, still right). | Project Veo plan and ledger; `assets/media/veo_budget.ron` |
| Offline simulation tools | EmberGen (JangaFX) simulates and exports flipbooks; paid (indie perpetual licence about $300) and a downloaded binary, which the owner's standing rule ("do not run binaries downloaded from the network") excludes. Blender's fluid simulation is free but also a downloaded binary. Not used; recorded in §12. | JangaFX pricing <https://jangafx.com/software/pricing>; CG Channel on EmberGen pricing (2023-10-18) <https://www.cgchannel.com/2023/10/jangafx-ships-embergen-1-0-7/>; owner standing rule |
| CC0 libraries | Kenney's "Particle Pack" (80 files) and "Smoke Particles" (70 files) are CC0. Usable as review references and, if ever needed, as source; the project prefers its own generated and authored assets, so none is shipped by default. | <https://kenney.nl/assets/particle-pack>; <https://kenney.nl/assets/smoke-particles> |

### 4.5 What the research means for the design

1. **Compose, don't special-case.** Every system studied reaches variety by combining a few generic emitters and
   renderers with data; none has an "acid burst kind". The first version's per-class shader kinds are replaced.
2. **Texture content carries the quality.** The cheap look comes from pure procedural shapes, plain alpha fades,
   hard intersections and single layers. The fix is authored greyscale content, gradient ramps, erosion, soft depth,
   premultiplied blending, layering and timing; all are standard, documented and buildable on Bevy 0.15.
3. **Generated images fit as greyscale elements on black**, never as finished coloured effects: the engine
   supplies colour, size, timing and composition; the image supplies the organic shape. This also keeps every
   effect sized by the simulation.
4. **Particles are the one block worth adopting**: GPU particle simulation, spawners and ribbons are a large,
   solved problem, and `bevy_hanabi` matches our Bevy, wgpu and licence and is tested on lavapipe. Decals, flipbook
   sprites with soft depth, rings, shell hooks and distortion are small, specific to our camera and terrain, and
   share one instanced renderer we control.

---

## 5. The Effect System

### 5.1 Building blocks

Each block is generic, data-parameterised and rendered by the client only. "Instanced" means a retained instance
buffer drawn with one draw call per block and blend mode (the pattern of Bevy's `custom_shader_instancing`
example), never an entity per instance.

| Block | Draws | Main parameters (data) | Renderer |
|---|---|---|---|
| `Particles` | Sparks, debris, droplets, embers, dust, fumes | Spawn (burst count, rate, duration), shape (point, disc, ring, sphere, cone, line), initial speed and spread, drag, gravity, lifetime, size curve, colour ramp, texture or flipbook, orientation (camera, velocity-stretched, ground), blend | `bevy_hanabi` through an adapter (D-10); fallback: CPU particles on the `Sprite` instancer |
| `Sprite` | Flipbook or still cards: fireball, smoke puff, acid crown, spine eruption, muzzle flash, psionic arc | Texture or flipbook (frames, fps or "fit duration", frame blend), ramp, erosion curve, size curve, rotation (random, along line, face camera), orientation (camera, ground, vertical-billboard), soft-depth distance, blend (premultiplied, additive) | Own instanced quad renderer |
| `Decal` | Ground marks: splash stain, scorch, crater, crack line, area circle, dirt mound | Texture, ramp, radius or length × width, erosion over life, fade-in, linger time, grazing fade | Own instanced box projector reading the depth prepass (as `smog_volume.wgsl` does), reconstructing the world position and sampling in the box's local XZ; no geometry follows the terrain |
| `Ribbon` | Beams, tethers, flame jets' cores, lance sweep, dash streaks, trails | Endpoints or polyline source, width curve along length and over life, UV scroll speed, texture and ramp, noise distortion, sag (catenary), taper, segments | Own instanced strip renderer (camera-facing strip per segment); hanabi ribbons only for particle trails |
| `Ring` | Shockwaves, expanding fronts, stance seals, detection pings | Radius from → to, thickness curve, ramp, texture (noise-broken ring), orientation (ground, camera) | `Sprite` renderer with a ring mask (procedural shape is the right tool here) |
| `Shell` | Modulation of the unit's own bubble: stim pulse, barrier outer shell, cloak shimmer, burrow sink, detected tint, slow overlay | Tint, rim (fresnel) strength and colour, pulse curve, sink depth, refraction strength, overlay texture and ramp | Existing `bubble_dome.wgsl`, extended with an effect-modulation block in its per-instance data (no new entity) |
| `Distortion` | Heat haze, shockwave refraction, cloak shimmer around a body, storm air | Strength curve, noise texture and scroll, mask (disc, ring, sprite alpha) | Material in the `Transmissive3d` phase reading `view_transmission_texture` (§4.2) |
| `Light` | A short point light: muzzle, impact, burn | Colour, intensity curve, radius, duration | Existing `CombatLightEffect` pool (non-shadowing, plan 37) |

Shared shader functions live in one WGSL import (`assets/shaders/effects/common.wgsl`): ramp lookup, erosion step,
soft depth, flipbook frame blend, premultiply. `combat_fx.wgsl` stays for the existing tracers, beams and shatter
until WP11 migrates them.

### 5.2 Effect definitions (`assets/catalog/effects.ron`)

An effect is a named list of **layers**; a layer is one block plus timing, placement and bindings. Textures, ramps,
curves and particle emitters are named once and shared.

```ron
// assets/catalog/effects.ron (sketch; the loader's types are the authority, typed errors on every unknown name)
(
    format_version: 1,
    // Baked texture sets (atlases written by asset_baker, §6); each names its frames and its channel layout.
    textures: [ "assets/effects/fx_blast.texture.ron", "assets/effects/fx_acid.texture.ron",
                "assets/effects/fx_ground.texture.ron", "assets/effects/fx_noise.texture.ron" ],
    // Colour ramps over luminance 0..1, linear RGB with values above 1.0 for HDR cores; alpha 0 means additive.
    ramps: {
        "blast_fire": [ (0.00, (0.0, 0.0, 0.0, 0.0)), (0.30, (0.35, 0.07, 0.02, 0.0)),
                        (0.70, (1.60, 0.70, 0.20, 0.0)), (1.00, (4.00, 3.20, 2.20, 0.0)) ],
        "blast_smoke": [ (0.00, (0.03, 0.03, 0.03, 0.0)), (0.60, (0.16, 0.15, 0.14, 0.85)), (1.00, (0.40, 0.38, 0.35, 0.9)) ],
        "acid":        [ (0.00, (0.0, 0.0, 0.0, 0.0)), (0.40, (0.10, 0.32, 0.02, 0.8)),
                         (0.80, (0.55, 1.10, 0.20, 0.6)), (1.00, (2.20, 3.00, 1.20, 0.0)) ],
        "acid_stain":  [ (0.00, (0.0, 0.0, 0.0, 0.0)), (1.00, (0.10, 0.22, 0.03, 0.75)) ],
    },
    // Timing curves: (time 0..1, value, easing to the next key).
    curves: {
        "punch":   [ (0.00, 0.0, EaseOutCubic), (0.06, 1.0, Linear), (0.15, 1.0, EaseInQuad), (1.00, 0.0, Linear) ],
        "linger":  [ (0.00, 0.0, EaseOutQuad), (0.10, 1.0, Linear), (0.70, 1.0, EaseInQuad), (1.00, 0.0, Linear) ],
        "rise":    [ (0.00, 0.0, EaseOutCubic), (1.00, 1.0, Linear) ],
    },
    emitters: {
        "debris_chunks": ( spawn: Burst(count: (base: 10, per_unit_radius: 8, max: 40)), shape: Disc,
                           speed: (3.0, 7.0), cone_deg: 55, gravity: 18.0, drag: 1.5, lifetime: (0.5, 0.9),
                           size: (0.05, 0.12), texture: "debris_atlas", orient: VelocityStretched(1.5),
                           ramp: "blast_smoke", blend: Premultiplied ),
        "acid_droplets": ( spawn: Burst(count: (base: 12, per_unit_radius: 10, max: 48)), shape: Disc,
                           speed: (2.0, 5.5), cone_deg: 70, gravity: 14.0, drag: 0.8, lifetime: (0.35, 0.7),
                           size: (0.06, 0.14), texture: "droplet", orient: VelocityStretched(1.2),
                           ramp: "acid", blend: Premultiplied ),
    },
    effects: {
        "shell_impact": (
            class: Major,                       // budget class (§5.6)
            event: Splash,                      // which bindings exist: Splash, Line, Point, Area, Link, Status, Mode, Death
            layers: [
                ( block: Light(color: (1.0, 0.6, 0.3), intensity: 1800.0, radius: (bind: SplashReach, scale: 3.0)),
                  start: 0.0, duration: 0.25, curve: "punch", priority: Detail ),
                ( block: Sprite(texture: "fireball_flipbook", ramp: "blast_fire", orient: Camera, soft_depth: 0.4,
                                frame_blend: true, rotation: Random),
                  size: (bind: SplashReach, scale: 1.6), start: 0.0, duration: 0.55, curve: "punch", priority: Critical ),
                ( block: Ring(texture: "ring_noise", ramp: "blast_fire", orient: Ground, thickness: 0.18),
                  radius: (from: (bind: SplashFull, scale: 0.5), to: (bind: SplashReach, scale: 1.0)),
                  start: 0.0, duration: 0.22, curve: "punch", priority: Critical ),
                ( block: Distortion(mask: Ring, strength: 0.025, texture: "noise_tile"),
                  radius: (from: (bind: SplashFull, scale: 0.5), to: (bind: SplashReach, scale: 1.3)),
                  start: 0.0, duration: 0.25, curve: "punch", priority: Detail ),
                ( block: Particles(emitter: "debris_chunks"), start: 0.02, priority: Detail ),
                ( block: Sprite(texture: "smoke_puff", ramp: "blast_smoke", orient: Camera, soft_depth: 0.6,
                                erosion: "rise", drift: (0.0, 0.6, 0.0)),
                  placement: Disc(count: 3, radius: (bind: SplashFull, scale: 1.0)),
                  size: (bind: SplashReach, scale: 1.2), start: 0.12, duration: 1.8, priority: Detail ),
                ( block: Decal(texture: "scorch", ramp: "blast_smoke", erosion: "rise"),
                  size: (bind: SplashReach, scale: 1.1), start: 0.05, duration: 8.0, curve: "linger", priority: Detail ),
            ],
        ),
        // further effects: §5.5
    },
    // Pool capacities per block and live caps per effect class (§5.6); sized at load, never grown in a battle.
    budget: ( particles: 30000, sprites: 2048, decals: 512, ribbons: 256, rings: 256, distortion: 64,
              classes: { Major: 96, Minor: 256, Ambient: 128 }, min_screen_px: 6.0, crowd_threshold: 24 ),
    // Fallbacks used when a unit's presentation block names nothing for a slot; the coverage test reports each use.
    defaults: ( impact: "spark_hit", death: "glass_shatter",
                status: { Overclock: "stim_surge", Slowed: "concussive_daze", Dash: "charge_streak", Barrier: "barrier_shell" },
                concealment: ( burrow_enter: "burrow_dig", burrow_leave: "burrow_emerge", burrowed: "burrow_mound",
                               cloaked_own: "cloak_own", cloaked_enemy: "cloak_shimmer", detected: "detected_mark" ),
                mode: ( transition: "mode_seal_sweep", anchored: "anchor_seal" ) ),
)
```

Rules of the format:

- Every name is resolved at load into a dense index; an unknown name, a binding the effect's `event` kind does not
  provide (for example `SplashReach` in a `Death` effect), a texture missing from the baked sets, or a curve whose
  keys are not ordered is a typed `EffectCatalogError` naming the file, effect and layer.
- `priority: Critical` marks the layers that carry gameplay information (the area's true size, the line's path, the
  tether); they are never dropped by the budget (§5.6). `Detail` layers may be.
- Times are seconds of presentation time (the client's `Time<Virtual>`, paused with the battle), never wall clock.

### 5.3 Units choose their effects (`units.ron`, `structures.ron`)

A new optional `effects` block on each archetype, read by `sim_core::catalog` as opaque presentation data (names,
like `veo_prompts`) and never stored on `Entity`, so it is not hashed:

```ron
// in the `volatile_tick` (Baneling analogue) entry of assets/catalog/units.ron
effects: (
    weapons: [ (weapon: Primary, mode: None, launch: None, travel: None, impact: None, splash: Some("acid_burst")) ],
    death: Some("acid_pop"),
),
// in `railgun_crawler` (Siege Tank analogue)
effects: (
    weapons: [ (weapon: Primary, mode: None,              launch: Some("cannon_muzzle"), impact: Some("tank_round_hit")),
               (weapon: Primary, mode: Some("siege_mode"), launch: Some("siege_muzzle"), splash: Some("shell_impact")) ],
    modes:   [ (mode: "siege_mode", transition: Some("siege_lock_sweep"), stance: Some("siege_anchor")) ],
),
// in `burrow_scorpion` (Lurker analogue)
effects: (
    weapons: [ (weapon: Primary, mode: Some("burrowed"), splash: Some("spine_line")) ],
    modes:   [ (mode: "burrowed", enter: Some("burrow_dig"), leave: Some("burrow_emerge"), stance: Some("burrow_mound")) ],
),
// in `shock_golem` (Marauder analogue, overclock capable)
effects: ( status: { Overclock: "stim_surge" } ),
```

- The block's keys are `weapons`, `modes`, `status`, `abilities` (ability name → effect for its area or link,
  D-8) and `death`; all optional, all falling back to `defaults`.
- `weapon` is `Primary`, `Ground` or `Air`, matching the archetype's `weapon`, `ground_weapon` and `air_weapon`;
  `mode` names the mode whose weapon override it is (or `None` for the default stance).
- The client builds a dense table `(UnitTypeId, WeaponKey) → effect indices` once per session (`WeaponKey` =
  slot + optional mode index), and `(UnitTypeId, StatusKind)`, `(UnitTypeId, mode index)`, `UnitTypeId → death`.
  Per event the cost is one index lookup.
- Coverage (test, not run-time check): every weapon with `splash` has a `splash` effect whose Critical layers bind
  `SplashReach`; every mode with `is_burrowed` or `is_immobile` has enter/leave or transition effects (own or
  default); every name resolves; every default used is listed in the test's report.
- Rejected: an `effect` field on `WeaponSpec` (it is hashed through `Entity::siege_weapon` and
  `ActiveProjectile::weapon_spec`; the `fields!` macro's `unhashed` list could exclude it, but presentation data
  does not belong on a simulation struct copied into every entity); a client-only file keyed by unit slug (the
  first version; the owner asked for the unit definitions to choose).

### 5.4 Bindings: sizes and timings from the simulation

| Binding | Value | Published by |
|---|---|---|
| `SplashFull`, `SplashMid`, `SplashReach` | Zone radii of the search (100 %, middle, outermost) | `SplashVisual` |
| `LineFrom`, `LineTo`, `LineLength`, `LineSpacing` | Source point, end point at full range, its length, the authored search spacing | `SplashVisual` of a line search (one per point, B-18/B-85) plus the weapon's line spec for the whole line |
| `AreaRadius`, `AreaRemaining`, `AreaTotal` | Persistent area | `RenderSnapshot::areas` |
| `LinkFrom`, `LinkTo`, `LinkLength` | Tether ends, per frame | `RenderSnapshot::links` + bubble positions |
| `UnitRadius`, `UnitHeight` | The bubble's radius and shape | `BubbleRenderInstance` |
| `ChannelProgress` | 0..1 transition progress (D-1) | `stance.transition` |
| `StatusActive`, `BarrierRatio` | Status on and barrier fraction | `status`, `barrier_ratio` |
| `FlightTime` | Projectile flight time | `ProjectileVisual::duration_secs` |

A `size` or `radius` is `(bind, scale)` or a constant; a `duration` may be `bind: AreaRemaining` or `bind:
StatusActive` (the layer lives while the fact holds and plays its exit curve when it ends). Critical layers must
bind their size; the GPU tests check the drawn footprint against the bound value (§10.2).

### 5.5 Effect sketches for the cases the owner named

Each is a composition of the blocks above; textures are named by the asset list of §6.3.

| Effect | Layers (block: purpose) | Bound to |
|---|---|---|
| `acid_burst` (Baneling analogue) | Light: green flash. Sprite (camera, flipbook `acid_crown`, ramp `acid`): the burst crown, size = reach. Ring (ground): fast acid front from full to reach radius. Particles `acid_droplets`: arcs falling inside the reach. Decal `acid_splat` (ramp `acid_stain`, erosion over 6 s): the stain, size = reach. Sprite (ground, `acid_bubbles` flipbook): fizz on the stain for 1 s. | `SplashFull`, `SplashReach` |
| `shell_impact` (Siege Tank analogue, sieged) | As in §5.2: light, fireball flipbook, shockwave ring and distortion, debris, three smoke puffs, scorch decal. | Zones 0.47 / 0.78 / 1.25 |
| `spine_line` (Lurker analogue) | Sprite placed along the line (`placement: Line(spacing: bind LineSpacing, travel_speed: from the weapon's search spacing)`), flipbook `spine_erupt` oriented vertical-billboard along the line, each instance delayed by its distance; Decal `ground_crack` stretched along the line (length = `LineLength`), erosion over 3 s; Particles dust per spine; no source layer, so an undetected Lurker's spines show and it does not (D-5, R3). | `LineFrom`, `LineTo`, `LineSpacing` |
| `flame_line` (Hellion analogue) | Ribbon core along the line (fire noise scrolling, ramp `fire`); Sprite fire flipbooks placed along the line with staggered start; Particles embers; Light moving along the line; Decal `scorch_streak` along the line. | Line bindings (circle splash today, B-18 line later: the effect follows the data) |
| `lance_sweep` (Colossus analogue) | Two Ribbons (beams) from the weapon to a sweep point moving along the line over the damage window; Decal `burn_line` written progressively along the line (erosion reveals with the sweep); Particles sparks at the sweep point; Light at the sweep point; Distortion heat haze over the line for 0.8 s. | Line bindings, damage window |
| `burrow_dig` / `burrow_emerge` | Shell: sink from 0 to the burrowed depth over `ChannelProgress` (vertex offset and clip below terrain). Particles: dirt clods ring. Sprite (camera, `dust_burst` flipbook) at start and end. Decal `dirt_mound` fades in at the end and stays as `burrow_mound` while burrowed (owner and detecting viewers only). | `ChannelProgress`, `UnitRadius` |
| `cloak_shimmer` (enemy) / `cloak_own` | Enemy: Distortion only (refraction of the bubble's silhouette, noise scroll), no colour, no Shell; own: Shell (translucent, cool rim) plus faint Distortion. | `UnitRadius` |
| `abduct_tether` (Viper analogue, once the mechanic exists) | Ribbon from caster to target (texture `tongue_strand`, sag curve, UV scroll toward the caster), Sprite hook at the target, Particles drips; the pull itself is the target's published motion. | `LinkFrom`, `LinkTo` |
| `psionic_storm` (area, once a spell exists) | Decal area disc (noise-broken edge exactly at `AreaRadius`, ramp `psionic`); Sprite `psionic_arc` flipbooks scattered in the disc (`placement: Disc(count: 8)`, re-spawned while active); Distortion over the disc; Light flicker; the whole loops for `AreaRemaining` and dissipates in 0.3 s. | `AreaRadius`, `AreaRemaining` |
| `stim_surge` (Marine/Marauder analogue) | Shell: red rim pulse (period 0.6 s) and warmer tint; Particles: thin fumes from the bubble's top while active; one-shot Ring pulse on activation. | `StatusActive`, `UnitRadius` |

### 5.6 Runtime: pools, retained buffers, budget and level of detail

- **Load (once per session):** parse `effects.ron` and the presentation blocks; resolve names; upload texture
  arrays (one per channel layout), the ramp atlas (one 256-texel row per ramp, built from the data), and one
  `bevy_hanabi` `EffectAsset` per emitter definition (built from data through the adapter).
- **Instances:** a live effect is a slot in a fixed-capacity `EffectInstancePool` (generation-checked index, birth
  time, bound values, anchor, seed); each layer writes its block instance into that block's retained instance buffer
  slot. Freed slots go to a free list. Capacities are sized at load from data (`effects.ron` `budget` section) and
  never grow during a battle; growth is a load-time error report, not a run-time allocation.
- **Particles:** a fixed pool of hanabi effect entities per emitter definition, spawned at load; an event takes a
  free entity, sets its `EffectProperties` (position, radius, seed) and re-arms its spawner (**verify** the exact
  0.15 API in the spike: `EffectSpawner` reset or a one-shot `SpawnerSettings`).
- **Determinism of the picture:** randomness in layers (rotation, frame offset, disc placement) comes from a hash
  of the event's tick, sequence and layer index, so a capture of the same battle is the same on every run of the
  same driver; hanabi uses a fixed `prng_seed`.
- **Budget (data):** per block a cap (initial values, measured in WP6: particles 30 000 live, sprites 2 048, decals
  512, ribbons 256, rings 256, distortion 64, lights the existing pool) and per effect a `class` (`Major`, `Minor`,
  `Ambient`) with a live-count cap. When a cap is reached: Critical layers always spawn (their pools are sized to the
  measured peak, and the peak is a test); Detail layers of the oldest `Minor` and `Ambient` effects are retired
  first (eroded out over 0.1 s, never popped); a new Detail layer that finds no slot is skipped.
- **Level of detail:** a layer whose projected size is below `min_screen_px` (data, default 6 px) is skipped;
  particle counts scale with projected size between 0.25 and 1.0; at more than `crowd_threshold` live Major effects
  on screen, Detail smoke and debris counts halve. The rules are pure functions with unit tests.
- **Sort and blend:** premultiplied blending for every sprite, ribbon and particle so additive and alpha parts
  share one pass; decals draw before transparent sprites; sprite instances are sorted back to front in place
  (`sort_unstable_by` on the retained index list, no allocation) once per frame.
- **Fog:** an effect is admitted where it lands (existing `reveals`); areas and links per frame by their centre and
  both ends (D-5).

---

## 6. Content Pipeline: Generated and Authored Textures

### 6.1 Principle

Textures are **greyscale shape and shading on pure black**, never finished coloured effects. Colour, size, timing,
placement and composition come from the engine and the data. This makes one texture serve many effects, keeps the
effect's size exactly the simulation's, and lets a reviewer judge the texture on its own.

### 6.2 Sources, in order of preference

1. **Generated stills** (if Q0078 allows): a Google image model on the project's credential path, at 1K or 2K,
   for decals (stains, scorches, cracks, mounds), single-frame elements (smoke puffs, spikes, droplets, hooks) and
   keyframes for Veo. Model: the cheapest generally available Gemini image model the Vertex Express key can call on
   the day, `gemini-nano-banana-2.1` first ($0.0336 at 1K), `gemini-3.1-flash-image` second ($0.067 at 1K);
   budgeted at the higher price.
2. **Veo clips on black** (approved service, within the $200 total): flipbook sources for camera-facing animated
   elements (fireball, smoke plume, acid crown, spine eruption, dust burst, psionic arc). Optionally started from a
   generated keyframe (image-to-video), which the Veo client already supports (`keyframe_sha256` in provenance).
3. **Offline authored noise** (`asset_baker effect-noise`): tileable value, Perlin, Worley and curl-noise sheets,
   erosion gradients and ring masks, generated by the baker from recorded parameters. These are authored assets,
   not stand-ins for media: noise is what the shader needs, and the charter's ban covers procedural *flipbooks*
   pretending to be media (§1.2).

### 6.3 Initial texture list (vertical slice first)

| Set | Elements | Source | Slice |
|---|---|---|---|
| `fx_blast` | `fireball_flipbook` (32 frames), `smoke_puff` (4 variants), `debris_atlas` (16 chunks), `scorch` (2) | Veo ×2 (fireball, smoke plume), stills ×6 | Yes |
| `fx_acid` | `acid_crown` (24 frames), `acid_splat` (3), `droplet`, `acid_bubbles` (16 frames) | Veo ×2, stills ×4 | Yes |
| `fx_ground` | `spine_erupt` (24 frames), `ground_crack` (2, tiling along length), `dirt_mound`, `dust_burst` (24 frames) | Veo ×2, stills ×4 | Yes (spines, crack) |
| `fx_noise` | `noise_tile`, `ring_noise`, `fire_noise`, erosion gradients | Offline baker | Yes |
| `fx_energy` | `psionic_arc` (16 frames), `fire` jet flipbook, `burn_line`, `tongue_strand`, `hook` | Veo ×3, stills ×4 | No |

### 6.4 Keying and baking (`asset_baker effect-bake`)

- **Black check:** the border band and the darkest 5 % must be ≤ 3/255; a frame with a lit floor, horizon or
  stripes is rejected (the Veo review's "no radiating stripes" rule applies).
- **Emissive elements** (fire, energy, acid glow): greyscale = luminance after black-point and white-point levels;
  drawn additive (premultiplied with alpha 0), coloured by the ramp.
- **Lit semi-transparent elements** (smoke, dust, goo, stains): from a **difference matte** when a black/white pair
  exists (the image model renders the same element on black and, by an edit request, on white; alpha =
  1 − (white − black), colour = black / alpha; the pair is rejected when the matte residual exceeds 4/255 on 0.5 % of
  pixels); otherwise **luminance key** (density = smoothstep of luminance, shading = luminance / density).
- **Channel packing** (one RGBA8 texel): R = shading, G = density (alpha), B = erosion order (derived offline: a
  blurred distance-to-edge times a noise sheet, so the thin parts erode first), A = emissive mask. One layout for all
  sets, so one shader path.
- **Flipbooks:** frames chosen at the clip's effect span (onset to dissipation), centred on the element's centroid,
  padded 2 px, packed into a texture array of fixed cell size (256² or 512²); a per-set `.texture.ron` manifest lists
  elements, frames, cell size and pivot; mip chains written by the baker (**verify** whether the client's loader
  builds mips for the existing atlases; the baker writes them if not).
- **Provenance** (`.provenance.ron`, as for unit atlases): per element the origin (`Veo { … }`,
  `GeneratedImage { model, prompt_sha256, request_id, ledger_line }`, `Authored { tool: "asset_baker effect-noise",
  params }`), source hashes, keying method and matte residual. `bake_integrity_tests.rs` gains the rule that every
  shipped effect texture has a real origin and that no `procedural_generator` output reaches `assets/effects/`.

### 6.5 Review (as for Veo clips, Q0026)

- **Texture contact sheet** per set (`asset_baker effect-review`, written to `docs/evidence/effects/review/`): each
  element on black, on mid-grey, on two terrain swatches, gradient-mapped through two ramps, its erosion at 25/50/75
  %, its alpha histogram; flipbooks as a frame strip and a loop GIF. Accept or reject per element with a reason, in
  the review log, like the Veo clip review.
- **Effect contact sheet** per effect (`client_diagnostics effect_lab`, a new mode of `battle_capture`): the effect
  at eight ages (0, 0.03, 0.08, 0.15, 0.3, 0.5, 0.75, 1.0 of its duration) from the battle camera at default and
  close zoom, on bright and dark terrain, with a 1-unit grid and the bound radius drawn as a thin reference circle in
  a second copy; plus a 3-second capture at the showcase's 40 ms step.

### 6.6 Budget (counts against the owner's $200 Google-media limit)

| Item | State on 2026-11-09 |
|---|---|
| Ledger on `main` (`docs/evidence/veo/ledger.jsonl`) | 217 `request_started` × $0.40 = **$86.80** |
| Lurker Veo round in flight (worktree `agent-a7b63ff3fedffa1e1`, last start 20:08 UTC) | 24 starts so far = $9.60, so **$96.40 committed, $103.60 left**; recount from the merged ledger before any effect run |
| Veo effect source clips (approved service) | Slice ≤ 8 clips ($3.20) + retries; full set ≤ 30 clips (**$12.00**) |
| Image model (only if Q0078 = yes) | Slice ≤ 60 images; full set ≤ 250 images including black/white pairs and rejects; budgeted at $0.101 (2K, the dearer model) = **≤ $25.25** |
| Veo morph and mode clips (only if Q0075 = yes) | ≈ 12 clips + 5 retries = $6.80 |
| **Total if all are approved** | **≤ $44.05**, leaving ≥ $59.55 |

`asset_baker`'s budget file gains a price per image-model id (`price_per_generation_usd_cents`), the ledger records
image requests like Veo requests, and the same total cap applies; a model id without a price is refused (existing
rule). No generation happens in this plan.

---

## 7. Quality Bar ("Professional, No Cheap-Looking Effects")

### 7.1 Reference board

`docs/evidence/effects/reference.md` (WP6) lists, per showcase effect, the StarCraft II behaviour it must read like
(§3) with links to Blizzard's own pages and videos (no copyrighted frames committed), the Riot and GDC principles
(§4.3) it applies, and, once accepted, our own approved stills as the canonical bar for later effects.

### 7.2 Review checklist (every effect, before it ships)

1. **Reads at gameplay zoom:** the dangerous area or line is identifiable within 0.1 s; a Critical layer marks its
   true size (±2 px at default zoom); one focal point; secondary elements do not compete with it.
2. **Layered:** at least three layers for a Major effect (for example flash or light, core, smoke or debris, ground
   mark); no single flat shape.
3. **Timing:** peak within 2 frames of the event (≤ 80 ms at 25 Hz presentation), eased fall-off, a dissipation that
   erodes rather than fades, nothing lingers past its duration except authored decals.
4. **No tells of cheapness:** no visible quad edges, no hard line where a card meets the ground (soft depth), no
   visible tiling or repetition (random rotation, flip, frame offset, variants), no pops on spawn or despawn, no
   banding in gradients, no colour fringes from keying.
5. **Value and colour:** brightest at the focal point, HDR core where the element emits, ramp from hot to dark; team
   colour only in secondary elements; consistent with `media_style.ron`'s palette.
6. **Crowd behaviour:** 200 splashes per second over a 400-unit fight still shows the units (overdraw and coverage
   measured, §10.3); the budget's degradation is not visible as popping.
7. **Fog:** nothing shows that the viewer may not see (D-5); an undetected Lurker's spines show, the Lurker does not.

### 7.3 Who judges

The implementing agent reviews against the checklist with both contact sheets and records accept or reject per item
in `docs/evidence/effects/review/`. The vertical slice (WP6) additionally gets an independent review by a second
agent given only the contact sheets, the captures and this checklist (not the implementer's notes). The owner sees
the result in the showcase video and may object at any time (Q0020); no owner sign-off blocks the work.

---

## 8. Design Decisions

### D-1. The simulation publishes facts; nothing hashed changes

- All additions are in `RenderSnapshot` (built after the tick in phase 10, "changes no authoritative state and no
  checksum") and `PlayerView` ("derived output: not checksummed").
- No new `Entity` field, no new field on `WeaponSpec` (hashed through `Entity::siege_weapon` and
  `ActiveProjectile::weapon_spec`, `checksum/encoding.rs` `fields!(WeaponSpec …)`), no random draw, no change of
  iteration order. Events are appended to the existing `pending_events` buffer.
- Catalog additions the simulation crates read: `UnitModeSpec::is_burrowed` (D-3), read only by the snapshot writer;
  and the archetype's `effects` presentation block (§5.3), parsed by `sim_core::catalog` like `veo_prompts` and
  stored on the archetype only. `UnitModeSpec` and the archetype are not part of the hashed state
  (`checksum/encoding.rs` encodes the entity's `active_mode` name but has no `fields!(UnitModeSpec …)`, checked at
  `0a36b3f` and again at `e30506d`). Should that change before WP2, the field goes in its own latent commit with a
  schema bump and the goldens are re-recorded with that cause only.
- **Transition progress without new state.** A mode channel publishes `remaining_ticks` (the entity's
  `channeling_remaining_ticks`) and the direction's **nominal** ticks from the catalog (`cast_ticks` or
  `leave_cast_ticks`, without the random extra). The client shows progress
  `clamp((nominal − remaining) / nominal, 0, 1)`: the random extra reads as a short hold before the morph begins,
  which is how the game's `RandomDelay` behaves. Storing the drawn total on `Entity` was rejected: it is presentation
  state in the simulation (plan `2026-11-04-battle-flipbook-animation` D-7) and would bump the checksum schema.
- **Effect source tags.** Every weapon event (`HitscanTracer`, `ProjectileVisual`, `BeamVisual`, `MeleeSlashVisual`,
  the new `SplashVisual`) gains `source: EffectSource { unit_type: UnitTypeId, weapon: WeaponKey }`, and
  `BubbleShatter` gains `unit_type`. `WeaponKey { slot: Primary | Ground | Air, mode: Option<u8> }` is `Copy` and
  fixed-size; serde defaults keep old RON snapshots readable. The writer knows the source entity at every emit site
  (**verify** per site in WP1; a projectile's arrival already carries its spec).

### D-2. Looks are catalog data chosen by the unit; the client owns the vocabulary

`assets/catalog/effects.ron` (§5.2) holds the vocabulary (textures, ramps, curves, emitters, effects, defaults); the
presentation block in `units.ron` / `structures.ron` (§5.3) chooses. Both are read at run time (C6) and validated
at load with typed errors; no Rust names a unit, weapon class, mode or effect (C5). The client's `vfx` module keeps
spawning the existing tracer, beam and shatter kinds until WP11 moves them into effect definitions; from then on
every weapon look is data. `WeaponVisualKind` in the hashed `WeaponSpec` keeps its meaning (which delivery event
the simulation emits) and is not changed (a change would move checksums).

### D-3. Concealment is data, not identity

- `BubbleRenderInstance::cloaked: bool` becomes `concealment: Concealment` with `None`, `Cloaked`, `Burrowed`.
  `Burrowed` when the entity is concealed and its active mode has `is_burrowed: true` (new `UnitModeSpec` field,
  default `false`, the game's `Buried` flag, set on `burrow_scorpion`'s `burrowed` mode); `CloakToggle { is_burrow }`
  maps likewise. `Cloaked` otherwise.
- The client never names a mode: it reads `concealment` and `stance`, and the unit's presentation block names the
  effects.

### D-4. Media only within the owner's rules

- Veo clips as **effect texture sources** (§6.2) use the approved service within the $200 total (owner, Q0001,
  Q0024; standing rule "the only limit you need to take into account are the 200$"); this plan caps them at 30
  clips ($12.00) and logs them in the ledger like unit clips.
- A **Google image model** is a new paid model: no image is generated until Q0078 is answered yes. Default
  meanwhile: Veo stills and flipbooks on black plus offline noise; the slice proceeds on that default, and generated
  stills replace Veo-derived stills element by element when allowed (the provenance records which).
- **Morph and mode unit clips** (sieged tank, burrowed Lurker, unburrow morph) stay gated on Q0075 (WP13); until
  then the effect system carries the burrow and siege reads and the unit clip stays the default stance's, a known,
  recorded limitation.

### D-5. Fog, detection and picking (the leak fix)

- `PlayerView::revealed: Vec<EntityId>`, sorted: the entities **not** owned by the viewer that are concealed and lie
  in a cell the viewer's detection covers (`VisionGrid::is_detected`) **and** in a `Visible` cell. Refilled in place.
  A concealed entity not listed is hidden from that viewer.
- Client rule, in `BattleFog` (one place): an enemy bubble with `concealment != None` and not in `revealed` is
  *undetected*: `Burrowed` → `Visibility::Hidden` and not pickable; `Cloaked` → drawn as the `cloaked_enemy` effect
  only (Q0076 default), not pickable, no health bar, never inspected; an inspected unit that becomes undetected stops
  being inspected. Health bars, the selection panel, the minimap and sound ask the same rule.
- Events are admitted as today, by where they land (`reveals`); a Lurker's spines are shown, the Lurker is not; the
  effect resolver never spawns a layer anchored on an undetected source.
- Under no fog (sandbox, editor, headless) the truth is shown with the owner look.

### D-6. Burrow timing and Q0069

The bubble sinks over the published progress. When Q0069 option 2 is implemented (weapon and cloak at 33 ticks, the
`Actor` dig to 45), the simulation's channel ends at 33 and the concealment flips then; the `burrow_dig` effect
finishes the dig for the remaining time from its own duration in `effects.ron`, or from an `actor_ticks` catalog
field if that plan adds one. Client-local presentation state keyed by entity is allowed (§3.2); the simulation keeps
none.

### D-7. Event delivery and steady state

- Simulation: `SplashVisual` is a fixed-size variant pushed into `pending_events` (capacity reused); `areas` and
  `links` are `Vec`s refilled in place like `bubbles`. A line splash emits one event per search point, bounded by the
  authored offsets.
- Client: every effect instance, layer instance and particle effect entity comes from pools sized at load (§5.6);
  retained GPU buffers are written in place during extraction (the `vfx/gpu_upload.rs` pattern). Steady combat
  allocates nothing in the main world: `sync_steady_state_tests.rs` and the valgrind-counted
  `sync_allocation_count_tests.rs` gain windows with a splash stream, a burrowed unit, an active area, a status and a
  tether.

### D-8. Links and areas are general contracts

`RenderSnapshot::areas: Vec<AreaView { id, owner, center: [f32; 3], radius, remaining_ticks, total_ticks, source:
EffectSource }>` (filled from `active_hazards`) and `RenderSnapshot::links: Vec<LinkView { from: EntityId, to:
EntityId, owner, source: EffectSource }>` (empty until a mechanic fills it). The area's and link's effect is chosen
by the source unit's presentation block (`abilities: { "<ability name>": "<effect>" }`), so a new spell needs data
only. Adding Abduct, Storm, Blinding Cloud, Fungal Growth, Liberator zones is a mechanics plan (Q0077).

### D-9. CI and lavapipe

All new shaders are WGSL validated by `gpu_validation_tests.rs` (naga) and rendered in GPU tests through
`crates/client/tests/support/gpu_run.rs` (radv locally, lavapipe on CI, C21). Effect time is the effect's own age
from `Time<Virtual>` (and hanabi's `Time<EffectSimulation>` driven from it), never wall clock, so a capture at the
showcase's fixed 40 ms step is reproducible. Smog is untouched; its goldens must stay bit-equal.

### D-10. Build versus adopt

- **Adopt `bevy_hanabi` 0.15.x for the `Particles` block**, behind `client::effects::particles` (the only module that
  names hanabi types), after the WP4a spike passes all of: (1) builds against the locked Bevy 0.15.3 and wgpu 23.0.1
  with no second wgpu or Bevy in `Cargo.lock`; (2) a GPU-harness test renders a burst on radv and on lavapipe with
  pixel statistics within tolerance of each other; (3) pooled re-arming of a spawned effect adds no main-world
  allocation in the valgrind window; (4) textured, flipbooked, ramp-coloured, velocity-stretched particles work with
  premultiplied blending; (5) 30 000 live particles cost ≤ 1.0 ms GPU on the RX 580 and the 400-unit frame budget
  moves ≤ 2 %; (6) its render ordering composes with our sprites (no particle drawn over a closer opaque bubble).
  Reasons: GPU simulation, spawners, gradients, flipbooks and ribbons are solved there; licence MIT OR Apache-2.0;
  it tracks Bevy releases (0.16 to 0.20 exist), so a Bevy upgrade does not strand us.
- **If the spike fails**, particles are CPU-simulated in the client (fixed SoA pool, no allocation) and drawn by the
  `Sprite` instancer; no other block changes. The spike's evidence decides; no question for the owner.
- **Build** decals, sprites, ribbons, rings, shell hooks and distortion: Bevy 0.15 has no decal; the projected-decal
  technique is the one `ForwardDecal` (0.16) uses and the smog already reads the same depth prepass; ribbons and
  rings are small; the shell is ours.
- **Rejected:** Effekseer and PopcornFX (C++ runtimes with their own renderers; PopcornFX's runtime is commercial),
  `bevy_enoki` (2D), `bevy_firework` 0.6 (no custom textures), `bevy_contact_projective_decals` (Bevy 0.13).

### D-11. HDR and bloom

Emissive cores read much better with bloom (§4.3). Enabling `Camera { hdr: true }` and `Bloom` changes every frame's
look, the existing visual goldens and possibly the frame budget, so it is its own step in the slice (WP6c): A/B
contact sheets of the battle with and without, frame budget before and after, a threshold so only emissive values
above 1.0 bloom (terrain and bubbles unchanged within the visual tests' tolerance, or re-baselined with that cause
only). The agent decides (Q0020) and records the evidence; if it is rejected, ramps cap at 1.0 and cores rely on
additive layering and the light block.

**Decided in WP6c (2026-11-10): rejected.** The threshold kept the scene unchanged, but the post-pass tone mapper
washed out every effect (saturation of the acid front 85 → 50) for a halo visible only during flashes, at 0.28 ms
of bloom per frame; ramps are capped at 1.0 and the loader enforces it (`RampOutOfRange`). Evidence:
[`docs/evidence/effects/review/hdr_bloom_decision.md`](../evidence/effects/review/hdr_bloom_decision.md).

### D-12. Composition stays bounded

An effect has at most 12 layers and 4 placements per layer; a placement replicates at most 32 instances. These
limits are load-time checks with typed errors, so pools can be sized from data and a typo cannot spawn thousands of
instances.

---

## 9. Work Packages (Commit Order, Tests First)

Each WP: tests first (red), then code, `cargo gate quick`, hand back; a verifier runs `cargo gate package` (Q0061).
Sizes: S ≤ 1 day, M 2–3 days, L ≥ 4 days. At most two building agents at once (Q0055).

| WP | Content | Tests (written first) | Exit | Size |
|---|---|---|---|---|
| WP0 | This plan's row; file Q0075–Q0078; register entries proposed in §12 | — | Committed | — |
| **Contract and leak fix** | | | | |
| WP1 | `sim_types`: `Concealment`, `StanceView { mode: Option<u8>, transition: Option<ModeTransitionView { entering: Option<u8>, remaining_ticks: u16, nominal_ticks: u16 }> }`, `StatusFlags` (overclock, dash, slowed, barrier up, barrier hit), `barrier_ratio`, `EffectSource`/`WeaponKey` on every weapon event and `unit_type` on `BubbleShatter`, `CombatEventVisual::SplashVisual { center, zones: [f32; 3], line: Option<LineView>, source, owner }`, `AreaView`, `LinkView`, `PlayerView::revealed`; `UnitModeSpec::is_burrowed` | `snapshot_serialization.rs` round trip with every new field and variant; old RON snapshots still read; `anchor()` of `SplashVisual` is its centre; `PlayerView::clear` empties `revealed` | `sim_types` green | M |
| WP2 | `sim_core` writer: concealment, stance (settled-mode query shared with plan `client-mode-commands`), status, barrier; one `SplashVisual` per search point; source tags at every emit site; `areas` from `active_hazards`; `revealed`; catalog: `is_burrowed: true` on `burrow_scorpion`'s mode, the optional `effects` block parsed (empty everywhere for now) | New `crates/sim_core/tests/effect_snapshot_tests.rs` (burrow progress 0 → 1 then `Burrowed`; sieged mode; each shipped splash weapon's events carry its zones; a line splash, scratch archetype in RON, emits one per offset; a hazard zone from a test-only ability appears in `areas` until it expires; status flags; every weapon event carries the right `EffectSource`); `player_view_tests.rs` (`revealed` only under the viewer's detector, never own units, never under fog); determinism: writing the snapshot twice from one world gives equal records | Every golden replay (release and `wasm32-wasip1`) and matrix battle **bit-identical**; checksum schema unchanged; evidence in the WP15 file | M |
| WP3 | Client leak fix: the `BattleFog` rule of D-5 for every reader (bubble, health bar, selection, inspection, minimap, picking); undetected enemy cloaked units keep today's shimmer until WP8 | `battle_fog_tests.rs` matrix (own, enemy undetected, enemy revealed, no fog) × (burrowed, cloaked); `battle_input_tests.rs`: an undetected enemy cannot be selected or targeted by click; GPU pose: an undetected burrowed enemy changes zero pixels against empty ground (2-pixel tolerance) | Leak closed; register entry closed | S |
| **Framework** | | | | |
| WP4a | `bevy_hanabi` spike (D-10 criteria) in a scratch branch, evidence only | The six criteria as tests or measurements | Go or no-go recorded in the evidence file | S |
| WP4 | `client::effects`: `effects.ron` and presentation-block loader with typed `EffectCatalogError`; name resolution into dense tables; curves, bindings and placements; `EffectInstancePool`, budget and LOD (§5.6); the instanced renderer with `Sprite` (flipbook, frame blend, ramp, erosion, soft depth, premultiplied), `Ring`, `Decal` (depth-projected), `Light` (existing pool); `Particles` adapter (hanabi or fallback); shared `effects/common.wgsl` | Loader: every error variant from a broken RON fixture; a runtime-catalog test that edits a copy of `effects.ron` in a temp dir and sees a different effect with no rebuild (C6); curve, binding, placement, budget and LOD pure-function unit tests (drop order, Critical never dropped, LOD thresholds); GPU tests per block on a test-only effect file with fixture textures in a temp dir (never under `assets/`): sprite soft-depth fade (no pixel step > 24/255 along the ground intersection), decal footprint (lit pixels inside the projected box + 2 px, none on a vertical wall face beyond its grazing fade), ring radius ±2 px, premultiplied additive/alpha mix; steady-state and valgrind windows with an effect stream | Green on radv and lavapipe; no main-world allocation | L |
| WP5 | `asset_baker`: `effect-noise` (offline authored noise and erosion sheets), `effect-bake` (black check, luminance key, difference matte, channel packing, flipbook frame selection, atlas and `.texture.ron`, mips, provenance), `effect-review` (texture contact sheets); Veo prompt class "effect element" in `media_style.ron` (pure black, isolated, side-on or top-down, no floor, no horizon, no rays); image-model client and ledger entries behind a `--model` flag, refused unless the model has a price in the budget file (lands dark until Q0078) | `media_tests.rs`/`bake_integrity_tests.rs`: keying on synthetic test frames with known alpha (a matte from a black/white pair reproduces the known alpha within 1/255); black-check rejects a lit floor; manifest round trip; provenance required; a procedural fixture can never be written under `assets/effects/`; budget refuses an unpriced model and counts image lines | Green; no generation in this WP | M |
| **Vertical slice** | | | | |
| WP6a | Generate and review the slice's sources (§6.3, Veo ≤ 8 clips; image-model stills only if Q0078 = yes) and bake `fx_blast`, `fx_acid`, `fx_ground`, `fx_noise` | Contact sheets and accept/reject log | Accepted sets committed with provenance and ledger lines | M + ≤ $3.20 (+ ≤ $5 images if allowed) |
| WP6b | Author `shell_impact`, `acid_burst`, `spine_line` in `effects.ron`; presentation blocks for `railgun_crawler`, `volatile_tick`, `burrow_scorpion`; `effect_lab` capture mode in `client_diagnostics` | GPU tests per effect (§10.2): footprint against the bound radius or line, coverage ≥ 60 % of the full-damage disc at peak, no pixels after duration except the decal, no pop (frame-to-frame mean luminance change outside the impact frame ≤ 6/255), golden stills per driver within tolerance | Effects render; tests green | M |
| WP6c | HDR and bloom decision (D-11); quality review (§7) by the implementer and an independent reviewer; frame-budget measurement with the slice at 200 splashes per second | Review log; frame budget evidence | **Gate:** all checklist items pass. Two failed review rounds → stop and hand back to the orchestrator with the contact sheets before any scale-out | S |
| **Scale-out** (after WP6c passes) | | | | |
| WP7 | `Ribbon` and `Distortion` blocks; `flame_line`, `lance_sweep`; projectile travel and impact (`ProjectileVisual` arrival spawns the unit's `impact` effect) | Ribbon width and endpoints ±2 px; distortion displaces only inside its mask; GPU stills; review | Reviewed — **done** (Amendment 9; travel effects moved to WP11) | M |
| WP8 | Concealment and stance: `Shell` hook (sink, rim, tint, refraction), `burrow_dig`/`burrow_emerge`/`burrow_mound`, `cloak_shimmer`/`cloak_own`, `detected_mark`, `mode_seal_sweep`, `siege_anchor`; client-local settle (D-6) | Progress maths (hold during the random extra, clamp, leaving direction); GPU pose "burrow": above-ground pixel area of a burrowed bubble ≤ 35 % of the unburrowed one, mound decal inside its ring; "siege mid-transition": sweep arc within ±5 % of progress; enemy undetected cloaked: only distortion pixels, no tint | Reviewed | M |
| WP9 | Status and death: `stim_surge`, `concussive_daze`, `charge_streak`, `barrier_shell` (absorb flash), death effects per presentation block (default stays the glass shatter) | Shell-flag unit tests; GPU pose "barrier": outer shell only while `barrier_ratio > 0` | Reviewed | S |
| WP10 | Areas and links: `psionic_storm`-style area from a test-only ability, `abduct_tether` from a test-only snapshot; `abilities` map in presentation blocks | Area edge at `AreaRadius` ±2 px; fog per frame; tether ends on both bubbles ±2 px; steady-state window with a live area and tether | Reviewed | M |
| WP11 | Migrate every existing weapon look (tracers, beams, lightning, slashes, plasma orbs, shatter) into effect definitions and presentation blocks for all shipped units and structures; retire the matching `combat_fx` kinds | Coverage test over the whole catalog with no defaults left for splash weapons; existing visual tests (`round2_visual_tests.rs`, `combat_fx_slot_gpu_tests.rs`) green or re-baselined with that cause only | Every unit chooses its effects in data | M |
| **Integration, media, showcase, bookkeeping** | | | | |
| WP12 | Mode-command integration (after plan `2026-11-09-client-mode-commands` lands): skirmish headless test burrows and sieges through input, checks stance and the fog rule end to end | `skirmish_match_tests.rs` extension | Green | S |
| WP13 | Morph and mode unit clips (only if Q0075 = yes): atlas slices keyed by `(AnimState, mode)`; ≈ 12 clips | `asset_baker` prompt validation; `runtime_media_tests.rs` admission of mode slices | Clips accepted in review | M + ≤ $6.80 |
| WP14 | Remaining texture sets (`fx_energy`, variants), showcase beats (tank sieges and fires, Hellion flame, Baneling burst and Lurker spines once those identities have media), frame budget with every effect | `showcase_tests.rs`: the take contains each showcase effect at least once; frame-budget evidence (§10.3) | CI showcase renders on lavapipe within its budget | M + ≤ $8.80 Veo (+ ≤ $20 images if allowed) |
| WP15 | Bookkeeping: evidence `docs/evidence/2026-11-09-effect-visualization.md` (bit-identity, captures, pixel statistics, frame budget, spend), `DESIGN.md` presentation row, register entries closed, `crates/gate/suites.ron` rules and GPU classes for every new test (C22), plan row → Done | `workspace_checks`, `suite_catalog_tests.rs` | Green | S |

Parallelism: WP3 and WP4a may run beside WP2; WP4 and WP5 are independent; after WP6c, WP7 to WP10 are
independent and run at most two at a time.

---

## 10. Tests, Measurement and Stop Rule

### 10.1 Determinism (simulation)

- **Expected:** every golden replay checksum and matrix battle outcome bit-identical at every WP (D-1).
- **Stop rule:** if any golden or matrix result moves, stop the WP, find the cause (a new field read by the tick,
  an RNG draw, an iteration order change) and hand back; do not re-record. The only sanctioned re-record is the
  fallback of D-1 (`UnitModeSpec` hashed by then), in its own latent commit naming that cause.

### 10.2 Picture (GPU harness, radv locally and lavapipe on CI)

- **Geometric statistics** (driver-independent): footprint of Critical layers against the projected bound radius,
  line or tether (±2 px at 1280×720 default zoom); coverage at peak; zero lit pixels after the duration except
  decals; undetected units change no pixels.
- **Temporal statistics:** at the 40 ms step, mean luminance change between consecutive frames outside the impact
  frame ≤ 6/255 (no pops); the last frame of every layer is within 2/255 of the background.
- **Golden stills:** per effect, ages 0.08, 0.3 and 0.75 of its duration, one golden set per driver (radv, lavapipe)
  in `crates/client/tests/data/effects/`; pass when ≥ 99 % of pixels differ by ≤ 8/255 and the mean absolute
  difference is ≤ 1.5/255. Re-baselining needs the contact sheet in the same commit.
- **Shader validation:** every new WGSL file through `gpu_validation_tests.rs`.

### 10.3 Performance

- `frame_budget` (`client_diagnostics`) on the 400-unit scenario with an effect stress mode (200 splashes per second
  across the slice's effects, 20 live areas, 50 statuses): p50 frame interval ≤ 5 % above the recorded baseline
  (16.3 ms), p99 ≤ 10 %; GPU time of the effect passes ≤ 1.5 ms on the RX 580 (Bevy's render diagnostics). Measured
  after WP6c and WP14; a miss shrinks budgets in data first.
- Overdraw: the fraction of screen pixels covered by effect layers in the stress frame is reported; the checklist
  item "units stay readable" is judged on that frame's capture.
- Simulation throughput benchmark unchanged within noise (snapshot writer only).

### 10.4 Catalog and assets

- Coverage over the shipped catalog (§5.3); runtime-catalog edit test (C6); C5 scan stays green (no unit or
  effect name in Rust; effect names live only in RON).
- `runtime_assets_tests.rs` loads `effects.ron` and every texture set from the runtime paths the binary uses.
- `bake_integrity_tests.rs`: provenance for every shipped effect texture; no procedural fixture under `assets/`.

---

## 11. Risks

| Risk | Mitigation |
|---|---|
| Information leak persists through another path (health bars, minimap, selection panel, sound) | One rule in `BattleFog`, every reader asks it; WP3 tests each reader |
| Generated textures look generic or "AI-like" | Greyscale elements only, coloured and composed by the engine; contact-sheet review with rejection reasons; regenerate rather than rescue (owner's media rule) |
| Veo will not keep a pure black background for effect elements | Black check rejects; prompt class tuned on the slice's 8 clips first; image-to-video from a black-background keyframe if Q0078 allows |
| Difference matte fails because the model changes the element between the black and white renders | Residual check rejects the pair; luminance key is the fallback |
| `bevy_hanabi` misbehaves on lavapipe or allocates per frame | WP4a spike criteria; CPU fallback on the sprite instancer (D-10) |
| Decals smear on cliffs and ramps | Depth-projected box with grazing fade; GPU pose on a ramp and a cliff edge |
| Too much noise in big fights | Budget and LOD in data, Critical/Detail split, measured stress frame (§10.3) |
| HDR and bloom change the whole game's look | Separate decision with A/B evidence (D-11) |
| Lavapipe too slow for the showcase | Effects are instanced quads; showcase render time measured in WP14 |
| `UnitModeSpec` becomes hashed before WP2 | D-1 fallback |
| Overlap with `client-mode-commands` and `friendly-blocking` | Shared settled-mode query and `is_burrowed` field: first to land adds, the other reuses (§2.4) |
| Players misread the burrowed Lurker's attack because the unit clip stays Idle | Attack clip timed from `attack_swing` (row 25) in WP8; the spines carry the read |
| Spend overruns | Every run charged up front against the ledger (existing rule); caps in §6.6 |

---

## 12. Out of Scope (Recorded, Q0002)

- New mechanics: Abduct, Psionic Storm, Blinding Cloud, Parasitic Bomb, Fungal Growth, Liberator defender zone,
  Widow-Mine-like units, permanent cloak of the Dark Templar and Observer analogues, Banshee cloak, burrow for other
  Zerg analogues, Tunneling Claws (B-38). The contracts of D-8 make their presentation data only (Q0077).
- Simulation fixes the inventory found: anchored units never publish `Attack` (row 25, `current_anim` hashed);
  B-18/B-19/B-83/B-85 (their own plan; the effects follow the data).
- Motion-vector flipbook blending (optical flow baked offline): frame blending first; motion vectors only if the
  review finds visible stepping at the chosen frame counts.
- Offline fluid simulation tools (EmberGen, Blender): excluded by the owner's rule on downloaded binaries; could be
  revisited only by an owner decision.
- An in-game effect editor; detection range rings; range indicators; sound; sustained beams merged across strikes.

**Proposed register entries** (numbered when filed, after the highest ID in use, B-105 on `friendly-blocking-r2`):
an undetected burrowed enemy is drawn to every viewer (closed by WP3); splash damage has no presentation (WP6, WP7);
mode transitions are invisible (WP8); barrier, dash and slow are invisible (WP9); a projectile has no impact (WP7);
the unit clip does not change with a mode (WP13 or recorded).

---

## 13. Owner Questions Filed (WP0)

- **Q0075 — Veo clips for morphs and modes.** Blocking: no. Default: no spend; the effect system carries burrow and
  siege, the unit clip stays the default stance's. Option: WP13, ≈ 12 clips ($6.80 with retries).
- **Q0076 — Undetected cloaked enemies.** Blocking: no. Default: a faint distortion shimmer as in StarCraft II, not
  selectable or targetable. Option: hide entirely. Burrowed enemies are hidden either way.
- **Q0077 — Caster spells.** Blocking: no. Default: this plan builds the presentation contracts (areas, links) and
  their tests; the mechanics go in a later plan in StarCraft II order of impact (Psionic Storm, Abduct, Fungal
  Growth, Blinding Cloud).
- **Q0078 — A Google image model for effect textures.** Blocking: no (only the spend waits). The question: may
  agents call a Gemini image model (`gemini-nano-banana-2.1`, else `gemini-3.1-flash-image`; $0.034 to $0.10 per
  image) through the same Google credential as Veo, for greyscale effect stills and Veo keyframes, up to $25.25 of
  the $200 total, logged in the same ledger? Default until answered: no image generation; textures come from Veo
  clips on black (already approved) and offline noise; generated stills replace Veo-derived ones element by element
  if the answer is yes. Recommendation: yes, because decals and single-frame elements are where a still model is
  strongest and cheapest.

---

## Amendment 1: review gate outcome, 2026-11-10

**Verdicts.** The WP6c gate (§9, §7.3) ran two independent review rounds on the vertical slice:

| Effect | Round 1 ([`independent_review.md`](../evidence/effects/review/independent_review.md)) | Round 2 ([`independent_review_round2.md`](../evidence/effects/review/independent_review_round2.md)) | Contact sheet |
|---|---|---|---|
| `shell_impact` (sieged Siege-Tank analogue splash) | reject | **accept** (with three optional improvements) | [`shell_impact_contact.png`](../evidence/effects/review/shell_impact_contact.png) |
| `acid_burst` (Baneling analogue) | reject | **reject** (items 4 and 5: a flat single-value green stain with polka dots, a button-like pop) | [`acid_burst_contact.png`](../evidence/effects/review/acid_burst_contact.png) |
| `spine_line` (Lurker analogue) | reject | **reject** (items 4 and 5: hook-shaped spines that read as glyphs, sparse clumps, hard clip edges, no focal point) | [`spine_line_contact.png`](../evidence/effects/review/spine_line_contact.png) |

**Stop rule applied.** WP6c's exit says that two failed review rounds stop the effect work before any scale-out.
`acid_burst` and `spine_line` failed both rounds, so the orchestrator stopped the effect work at the gate and landed
only what passed (decision note, 2026-11-10).

**What landed.**

- The presentation contract and the leak fix: WP1 (`sim_types`), WP2 (`sim_core` writer, every golden replay and
  matrix battle bit-identical), WP3 (`BattleFog`; B-106 fixed).
- The framework and the pipeline: WP4 (`client::effects`: vocabulary, unit tables, runtime, budget and level of
  detail, instanced renderer, effect lab), WP4a (`bevy_hanabi` no-go), WP5 (`asset_baker` effect-texture pipeline),
  and the WP6c framework extensions (breakup noise, `Front` rings, radial glows, landing particles, upright clumps,
  line clips) and decisions (D-11 HDR and bloom rejected, frame budget).
- `shell_impact`, named by the sieged Siege-Tank analogue's presentation block, as accepted in round 2. The
  reviewer's optional improvements are not applied now.

**What shipped unused (until Amendment 3).** `acid_burst`, its `acid_fleck` and `spine_line` stayed in
`assets/catalog/effects.ron` with their texture sets in `assets/effects/`, valid catalog data under the catalog
tests, named by no shipped unit and bound to `volatile_tick` and `burrow_scorpion` only by lab-only test blocks,
until review round 5 accepted them and Amendment 3 put their presentation blocks back in `units.ron`.

**One burrow flag.** Plan `2026-11-09-friendly-blocking` (R6, B-93) landed `UnitModeSpec::burrowed` on `main` first,
for the burrowed body's collision. As §2.4 and §11 provide (the first to land adds the field, the other reuses it),
the merge drops this plan's `is_burrowed` and the snapshot writer's concealment reads `burrowed`; `burrow_scorpion`'s
burrowed mode carries `burrowed: true` once. The presentation is the same, and the field is no longer read by the
presentation only.

**Stopped.** Scale-out (WP7 to WP15) does not start until the owner answers
[Q0080](../questions/closed/0080-effect-review-outcome.md): a third round for the two rejected effects, a changed
approach (for example owner-provided art direction or reference images), or leaving them out. Register entries
B-107 (now only partly addressed), B-108, B-109, B-110 and B-111 stay open, scheduled behind that answer.

## Amendment 2: owner answers Q0075, Q0076, Q0078 and Q0080; review round 3, 2026-11-10

**Decision note (charter §6, short note).** The owner answered the four questions this plan filed or depends on.
This amendment records what changes and how it is tested; it changes no simulation state, golden replay or
checksum (D-1 holds: every change below is in the client, the catalog's presentation data and the effect
textures).

| Question | Answer | What this plan does |
|---|---|---|
| [Q0080](../questions/closed/0080-effect-review-outcome.md) | Option 1: continue the review loop for `acid_burst` and `spine_line`, **up to five rounds in all** before asking again; the WP7+ scale-out is unblocked | The WP6c stop rule of two failed rounds becomes **five**. Rounds 1 and 2 are spent; **round 3** works from the round-2 reviewer's ranked changes as the owner restated them (below). When round 3 accepts both effects, the scale-out re-enables them for `volatile_tick` and `burrow_scorpion` (presentation blocks back in `units.ron`); until then they stay unused (Amendment 1). |
| [Q0076](../questions/closed/0076-undetected-cloaked-enemies.md) | Option 1 plus StarCraft II's cues for burrowed units | D-5 is extended by rules R4 to R6 (below). |
| [Q0078](../questions/closed/0078-image-model-for-effect-textures.md) | Yes: target about **$25** for effect textures, cap **below $50**, within the $200 total; every call in the ledger | §6.6's image line becomes "target $25.25, cap < $50". Every request goes through `asset_baker effect-image`, which ledgers it. |
| [Q0075](../questions/open/0075-veo-morph-and-mode-clips.md) | Option 2, up to $20 (target < $10) for morph and mode clips | WP13 is approved at that cap. **Not in round 3**: it runs in the scale-out. |

**Round 3 changes (from [`independent_review_round2.md`](../evidence/effects/review/independent_review_round2.md)).**

- `spine_line`: straight or gently tapering conical chitin spikes (new image-model stills), three to five per
  clump splayed outward from a shared base, 0.6 to 0.9 units tall; a bright bone-ivory tip and rim light on the
  newest clump for its first two to three frames, with a short light dust burst (the travelling focal point);
  the churned earth and the fissure feathered over about 0.3 units at both ends (a per-layer `clip_feather` on
  line-clipped decals); the leftover marks broken into one irregular scar (scattered, turned and varied crack
  marks without the ring-shaped variant, churned earth stretched along the line so the patches merge); lighter,
  shorter-lived churned earth for the crowd.
- `acid_burst`: a liquid material for the stain (a `liquid` option on decals: value and thickness varied by
  noise, a relief from the texture's thickness lit by an upper-left key light as a wet sheen that dries away,
  darker pooling in the thick parts, a darker and more saturated wet rim); no evenly spaced dots; drying towards
  olive from about 0.6 s; a ragged, noise-broken burst with larger dark-bodied globs instead of the bevelled pop;
  a low vapour cloud rising at the centre; the stain kept (Critical) when the crowd budget retires Detail layers,
  so no lone ring reads as a selection circle; no pale fringe.
- `shell_impact` (accepted; must not regress): only the round-2 optional changes that are cheap and safe.

**Concealment and visibility rules (D-5 extended, Q0076).**

- **R4, cloaked:** an undetected cloaked enemy is drawn only as a faint refractive shimmer; it is not
  selectable, targetable or inspectable and has no health bar (D-5 as before; tested again).
- **R5, burrowed and moving:** an undetected burrowed enemy that is stationary and not attacking stays hidden.
  A burrowed unit that moves underground leaves a ground-disturbance trail where the viewer sees the ground: a
  new optional `moving` slot in a mode's presentation block (`modes: [(mode: "burrowed", moving:
  Some("<effect>"))]`) names a `Mode` effect that the client lays at the unit's position at a fixed period while
  its published velocity is not zero; the unit itself stays hidden and unpickable. No shipped burrowed mode can
  move today, so no shipped unit names a trail yet; the rule is tested on a lab fixture.
- **R6, burrowed attacker:** the attack effect of an undetected burrowed attacker (the spine line) is shown to
  the viewer wherever the viewer sees it, from its origin, while the attacker stays hidden. The old rule (the
  line spawned only if its first search point was in sight, because only point 0 spawned the effect) hid a line
  whose first point was fogged but whose later points were in sight. Now every admitted search point of a line
  contributes: the effect spawns once per line from any admitted point, anchored at the line's first point, and
  each of its point-placed instances is drawn only at search points the viewer sees (a per-line visibility mask).

**Tests.** Unit tests of the line grouping and the mask; `battle_fog_tests.rs` for R4 to R6; GPU tests through
the harness where pixels matter (an undetected burrowed attacker's line draws pixels on visible ground and none
on the attacker; a moving burrowed enemy draws its trail and no bubble); the slice's GPU tests with goldens
re-recorded only for the effects whose look changed, each with its traced cause.

**Gate.** After round 3's fixes the implementer writes the round-3 section of
[`independent_review_packet.md`](../evidence/effects/review/independent_review_packet.md) (each round-2 finding
mapped to what changed) and hands back; the orchestrator launches the independent round-3 review.

**Round-3 implementation (2026-11-10, commits `ca082cd` to `a69acdb`).** Done as above, with these decisions
(recorded under Q0002, reversible):

- `shell_impact`: only the darker char core is applied; the shock ring's width variation and a less regular
  streak fringe are deferred to the scale-out, because they change the accepted Critical layer and need a new
  blast-mark texture.
- `acid_burst`: the landing flecks are removed (inside the stain they read as the round-2 "polka dots"), and the
  `acid_fleck` effect with them; the stain is sized to the reach (scale 1.0) so its Critical footprint stays
  inside it.
- `spine_line`: a short heave of churned soil over each point's full-damage disc (gone within 0.7 s) keeps the
  slice's coverage test (at least 60 % of the full-damage disc at peak) while the lasting mark is one continuous
  scar.
- The decal end feather lies 60 % beyond the line's end, so a mark is about two thirds strong at the end; the
  faint tip reaches at most 0.18 before the first search point (0.82 from the attacker's centre, inside its
  hidden body, which the owner's R6 allows: the attack shows from its origin).
- The round-3 crowd capture ran on lavapipe (debug, Xvfb) through a scratch copy of `assets/` that names the
  two effects for their units, because no shipped unit names them and the server's disk was saturated.

Evidence: [`slice_review.md`](../evidence/effects/review/slice_review.md) ("Round 3"),
[`texture_review.md`](../evidence/effects/review/texture_review.md) ("Review round 3"), the packet's "Round 3"
section. The independent round-3 review is next.

**Round 4 (2026-11-10, short note, charter §6).** The independent round-3 review
([`independent_review_round3.md`](../evidence/effects/review/independent_review_round3.md)) accepted
`shell_impact` again and rejected `acid_burst` and `spine_line` (narrowly); it asked for a fog capture. Round 4
applies its ranked changes in the client, the catalog's presentation data, the effect shaders and one texture
(no simulation state, golden replay or checksum changes; D-1 holds). Decisions (Q0002, reversible):

- `spine_line`: the single leaning spikes are **dropped** rather than re-timed, because the new massive clumps
  already carry three to five spikes each and every single spike read as a lone stroke; the ruptures use the
  torn-earth texture along the line (`spine_crack` and `chitin_spine` stay baked but unused); the scar decal's
  box may be wider than long (a decal's `width` now ranges over (0, 4]).
- `acid_burst`: the radial ground flash is dropped (it read as a glowing gel oval at frame 0); the stain itself is
  there, thick and at 0.85 of its size, from frame 0. The wet rim is drawn by the liquid material along every edge
  instead of by the ramp's top, which the drying ramp curve had turned into the brightest emerald. The stain
  stays inside the reach (0.98 of it): a larger stain (1.14) failed the Critical footprint test by up to 11 px,
  and a Critical layer must not mark ground beyond the damage area, so the stain was fixed rather than the test.
  The footprint test also holds the lasting Critical marks to the reach once every non-decal layer has ended, and
  the lingering check measures a decal to its box's corner.
- `shell_impact` (accepted): only the shock front's width variation (a shader change shared with the acid front)
  and the dust starting after the front; the streak fringe stays recorded for the scale-out.
- Fog (R6): a partly seen line's ground-hugging layers are clipped to the run of seen points around each
  instance, half a spacing beyond the outermost seen point (the cell edge); a standing clump is drawn whole when
  its point is seen, so it is never cut in half at the fog's edge; the effect lab can draw the viewer's fog as the
  battle terrain does, for the review's fog capture.

Evidence: [`slice_review.md`](../evidence/effects/review/slice_review.md) ("Round 4"),
[`texture_review.md`](../evidence/effects/review/texture_review.md) ("Review round 4"), the packet's "Round 4"
section. The independent round-4 review is next.

**Round 5 (2026-11-11, short note, charter §6; the last of Q0080's five rounds).** The independent round-4
review ([`independent_review_round4.md`](../evidence/effects/review/independent_review_round4.md)) accepted
`shell_impact` and `spine_line` and rejected `acid_burst` narrowly: its stain's visible edge lay at 0.87 of the
reach (the texture's ragged outline and deep noise bites inside a box of 0.98), and a pale, see-through band lay
between the stain and the front's lip. Round 5 changes the client, the catalog's presentation data, the decal
shader and one texture element (no simulation state, golden replay or checksum change; D-1 holds). Decisions
(Q0002, reversible):

- `acid_burst`: a decal may have a **disc outline** (`outline: (edge_noise, fill)`): its visible edge is a disc
  of the box's half length whose shallow noise edge wavers by `edge_noise` (±2 % of the radius for the stain),
  and the texture is read from its middle inside it, so its own outline lies outside. The stain is sized so its
  mean edge lies on the reach and holds it without erosion until about 1.8 s; the drying erosion's deep bites
  come after. The footprint test now measures the held stain's visible edge against the reach (±2 px on most
  rays), not only that it stays inside. The front's lip is thickened (intensity above 1) and its inner side is
  the stain's wet emerald. Globs get their own element of three variants (three image-model stills, $0.33),
  the fumes are thinner and yellow-green, the crown is larger.
- `spine_line` (accepted; polish only): a standing card clipped to its line is made smaller and moved inward
  at the line's ends instead of being cut (the first clump no longer reads as a lone spike); the clumps fade
  over their last 15 % as they sink; the scar's ramp darkens any ground (colours below its opacity times the
  teal ground's darkest channel).
- `shell_impact` (accepted; optional): the blast mark comes in once the shock ring has passed the reach
  (0.12 s), ending as before.

Evidence: [`slice_review.md`](../evidence/effects/review/slice_review.md) ("Round 5"),
[`texture_review.md`](../evidence/effects/review/texture_review.md) ("Review round 5"), the packet's "Round 5"
section. The independent round-5 review is next.

## Amendment 3: review gate passed (round 5, 2026-11-11)

**Decision note (charter §6, short note: catalog data, a test fixture and documentation).** The independent round-5
review ([`independent_review_round5.md`](../evidence/effects/review/independent_review_round5.md)) accepts all three
effects of the vertical slice, so the WP6c gate has passed within Q0080's five rounds. This amendment ships them; it
changes no simulation state, golden replay or checksum (D-1 holds: presentation blocks are read by the client only),
and no effect definition, texture or GPU golden (the effect lab already rendered these blocks with the shipped
units' weapons).

**Verdicts per round.**

| Round | Review | `shell_impact` | `acid_burst` | `spine_line` |
|---|---|---|---|---|
| 1 | [`independent_review.md`](../evidence/effects/review/independent_review.md) | reject | reject | reject |
| 2 | [`independent_review_round2.md`](../evidence/effects/review/independent_review_round2.md) | **accept** | reject | reject |
| 3 | [`independent_review_round3.md`](../evidence/effects/review/independent_review_round3.md) | accept | reject (narrowly) | reject (narrowly) |
| 4 | [`independent_review_round4.md`](../evidence/effects/review/independent_review_round4.md) | accept | reject (narrowly) | **accept** |
| 5 | [`independent_review_round5.md`](../evidence/effects/review/independent_review_round5.md) | accept | **accept** | accept |

Round 5 also passes the cross-effect consistency, the crowd frame and the fog edge of the burrowed attacker's line.

**What ships.**

- `acid_burst` is named again by the Baneling analogue `volatile_tick` (`weapons: [(weapon: Primary, splash:
  Some("acid_burst"))]`), and `spine_line` by the Lurker analogue `burrow_scorpion`'s burrowed weapon
  (`weapons: [(weapon: Primary, mode: Some("burrowed"), splash: Some("spine_line"))]`), in
  `assets/catalog/units.ron`, beside the sieged Siege-Tank analogue's `shell_impact`. The blocks are those the
  effect lab and the GPU tests rendered in every round.
- `burrow_trail` (Amendment 2, R5) stays defined and named by no shipped unit, because no shipped burrowed mode can
  move. `crates/client/tests/data/effect_lab_blocks.ron` keeps only its entry, which the tests add to
  `burrow_scorpion`'s shipped block (an entry repeating a shipped one fails the fixture).
- `effect_catalog_tests.rs`: `accepted_slice_effects_are_named_by_their_shipped_units` checks that each accepted
  effect is named by its shipped unit's block, resolves, and is the GPU-test subject bound to that unit's weapon;
  `lab_only_effects_are_defined_and_named_by_no_shipped_unit` now covers every slot of the lab-only entries.
- B-107 is narrowed: splash weapons beyond the slice's three still have no area drawn (WP7, WP11).

**Scale-out polish items.** The reviewer's ranked remaining improvements, none blocking. They are recorded here
under Q0002 and taken up in the scale-out (WP7 onwards), each with its own review like any other effect change:

- `acid_burst`, in the reviewer's order:
  1. Edge material inside the disc outline: a dark wet meniscus band about 0.05 to 0.1 of the radius wide with a
     broken specular highlight on the upper-left side and, while drying, a slightly lighter cracked crust ring. The
     outline stays on the reach; only the inner band changes.
  2. Remove the pale double streak inside the upper-left edge (the stain texture's own inner rims, now cut inside
     the outline), by masking those rims or reading a region without them.
  3. Landed globs must not read as letters at close zoom ("J", "S", "13"): splat them into short blots or fade them
     on landing, and avoid the edge-on neck variant at small sizes.
  4. Crowd: vary each stain's interior (rotation, pooling offset, drying phase) and soften overlaps, so a field reads
     as one liquid and not as stacked discs; recheck the green share (7.0 % of the crowd frame, 5.5 % in round 4).
  5. More dried-crust texture after 1.8 s (cracks, pooling), so the dried oval is not a nearly flat fill.
  6. Frame 0: drop the concentric inner ring and the sparkle dots.
- `spine_line`: torn clods in the scar and less periodic blotches (they repeat at about the point spacing), so it
  reads as torn earth and not as a smudge at close zoom.
- `shell_impact`: an irregular streak fringe from a new blast-mark texture (the fringe is still a regular sunburst
  at close zoom), and about 2x width variation in the shock ring with one or two breaks.

**Next.** The WP7 to WP15 scale-out (§9), unblocked by Q0080 (Amendment 2), with B-107 to B-111 open behind it.

## Amendment 4: scale-out WP7 (2026-11-11, short note, charter §6)

**What changes.** WP7 of §9 in the client, the effect shaders and the catalog's presentation data (no simulation
state, golden replay or checksum changes; D-1 holds):

- **`Ribbon` block** (§5.1): a strip along the event's line, drawn by the card renderer as an axis billboard
  (`orient: Camera`) or flat on the ground (`Ground`); its head races out at `travel`, its tail follows after
  `tail_delay`, so it can sweep along the line as a short segment; a tiled texture (u along the strip) or a computed
  core (`shape: Radial`), tapers at both ends, noise breakup that repeats and scrolls with the strip. Only `Line`
  effects may use it, with the one `Point` placement (typed load errors). A card record gains a ninth 16-byte row.
- **`Distortion` block** (§5.1): a camera-facing or ground disc that bends the opaque scene behind it by scrolling
  noise, read from the view's transmission texture (Bevy's copy of the opaque scene before the transmissive phase),
  queued as one `Transmissive3d` item; budget `distortions` (default 64).
- **Line events.** A line's search points are merged by the far end they all publish (the flame line's last search
  lies a double spacing beyond the one before, which split it into two spawns); a line whose searches land on
  several ticks (the spine line, B-85, period 2 ticks) joins its live effect while its travel still lays the line
  out, instead of spawning the whole line again per search (in battle each search had spawned a second, later copy
  of the line); line placements spaced wider than the line ask the fog for the search points they stand on.
- **Random choices salted by name.** An effect instance's seed was salted by the effect's dense id, which every
  effect added before it in name order shifts: adding the four effects changed `shell_impact`'s random rotations
  and its golden stills (86.7 % of pixels within 8/255). The salt is now an FNV-1a hash of the effect's name, so a
  vocabulary edit never moves another effect's picture again. The goldens of the three shipped effects are
  re-recorded once with this cause (both drivers), and their sheets re-taken.
- **Effects:** `flame_line` (Hellion analogue `napalm_wheeler`), `lance_sweep` (Colossus analogue
  `tripod_annihilator`), `flak_burst` (Liberator analogue `sky_bastion`, air weapon) and `cleave_impact` (Ultralisk
  analogue `stag_behemoth`), named by lab-only entries (`crates/client/tests/data/effect_lab_blocks.ron`) until the
  independent review accepts each; then the entry moves to `units.ron` (as for the slice, Amendment 1).
- **Polish of the shipped three** (Amendment 3's list, cheap and safe items only): `acid_burst` item 6 (the front
  and its dark edge start at 0.3 of the full-damage radius, inside the crown, so frame 0 shows no concentric inner
  ring; the fine spray takes the glob's dark wet ramp, so no sparkle dots); `spine_line` (the scar's pieces scattered
  0.3 along and across the line, so its blotches no longer repeat at the point spacing; the churned patches are
  kept as they were, because scattering them more or varying their size lowered the full-damage coverage from 75 %
  to 62 %).

**Decisions (Q0002, reversible).**

- The Colossus beams from the weapon to the sweep point need the attacker's position in the line event, which
  `LineView` does not carry for a target-origin line; the legacy per-strike beam (`vfx`) still draws from the
  weapon to the struck point and the sweep is drawn on the ground. Recorded for WP11 (migration of the legacy
  beams), with the option of a presentation-only `LineView` field.
- The Hellion's legacy team-coloured tracer (`vfx`, 0.12 s) still draws under the flame until WP11.
- Line effects carry no `Point`-placed light: a light at the line's anchor would light ground the viewer may not
  see (R6). Their glow comes from additive layers.
- `flak_burst` bursts at the air units' plane; in the effect lab it is captured on the ground plane.
- `shell_impact`'s polish (a new blast-mark texture, twice the ring's width variation) is not taken: it needs a new
  texture and a change of the front shader shared with the accepted `acid_burst`.
- Distortion discs are unsorted and see only the opaque scene (not other effects or the bubbles), as Bevy's
  transmission does.
- WP8 (concealment and stance) is not started in this round.

**Tests.** `effect_gpu_tests.rs`: ribbon ends and width within 2 px (camera-facing, ground, fog-cut), distortion
bends pixels only inside its disc; `effect_slice_gpu_tests.rs`: seven subjects (footprint, coverage, pop, lingering,
goldens per driver) and every line subject's fog mask; `effect_catalog_tests.rs`: ribbon and distortion range
checks; `effect_runtime_tests.rs`: the two line merges; `gpu_validation_tests.rs`: `distortion.wgsl`.

**Gate.** Each new effect and each polished one gets its implementer checklist in
[`slice_review.md`](../evidence/effects/review/slice_review.md) ("Scale-out round 1") and a packet section; the
orchestrator runs the independent review.

## Amendment 5: scale-out round 2 (2026-11-11, short note, charter §6)

**Why.** The independent review of scale-out round 1
([`independent_review_scaleout1.md`](../evidence/effects/review/independent_review_scaleout1.md)) accepted
`flak_burst`, rejected `flame_line`, `lance_sweep` and `cleave_impact`, found that `shell_impact` had regressed (its
scoured blast mark 45 to 55 % weaker) and asked for a crowd frame with all seven effects and a fog sheet for
`flame_line`. This round acts on each point. It changes presentation data, the effect lab and the stress tool only
(no simulation state, golden replay or checksum; D-1 holds).

**`shell_impact` restored.** Its regression came from Amendment 4's name-hash salt, which gave it a new draw of its
random choices. An effect definition may now pin its salt (`seed: Some(n)`, schema `EffectDefRaw::seed`; without it
the salt stays the FNV-1a hash of the name). `shell_impact` pins 2, its dense id when round 5 accepted it, so its
pictures equal `main`'s pixel for pixel; the other round-1 fixes (the line merges, the name salt for every other
effect) are kept. Proof: its goldens are `main`'s files again, unchanged, and pass on both drivers with every pixel
equal (mean difference 0.000/255); its lavapipe contact sheet and sequence equal `main`'s round-5 files with 0
differing pixels.

**Effects reworked** (each point of the review; textures unchanged, nothing spent):

- `flame_line`: the jet and the body of fire have opaque, saturated orange ramps (the additive cream core washed out
  on bright ground and under the tone mapper), yellow-white on the axis only for its first two frames (`flame_cool`);
  the bubbly fireball tongues are replaced by soft-eroding billows of the smoke element in a fire ramp that dies to
  soot (`flame_tongue`, `flame_die`) and by flame tongues, smoke-element particles stretched along their rising
  velocity (`napalm_tongues`, buoyant); the streak is one continuous scorched band of uneven width (`earth_scar`
  pieces more than twice the spacing long, `scorch_band`) with napalm embers glowing in it for 0.6 s; every fire
  layer is cut at the line's ends, so nothing burns behind the first search point; every line placement travels
  with the jet (40 units per second), so no card appears ahead of its tip.
- `lance_sweep`: the scar is a burnt groove of torn ground whose pieces vary twofold in length and width, in a
  fading scorched halo, glowing from the inside (its thin parts erode first) and through short glowing cracks that
  outlast the glow; the stroke and the flares at its head are torn by the fire noise from their first frame
  (`torch_break`, no capsule; the wide round glow ribbon is removed) and a saturated orange (`lance_hot`); molten
  spatter is thrown off every point (the burst's level-of-detail radius was too small to spawn it); smoke puffs vary
  in size, place, time and drift.
- `cleave_impact`: frame 0 shows the blow (a warm flash, a short light, sparks); a crack and churned earth under it,
  dark clods on arcs; the front is a noise-broken dust skirt, wider, held thick (intensity 1.8) so its edge stays
  on the reach, then thinning out over its last six frames while it erodes (`skirt_fade`, no pop), with small
  dust puffs kicked out behind it; the dust is a dense burst at the struck point and puffs of their own size and place,
  barely pushed outward (no radial star), drawn per hit.

**Decisions (Q0002, reversible).** The review asked for the cleave's dust to be thrown in a cone along the blow's
direction. A splash event carries no direction (`SplashVisual` has a centre and zones); the dust is instead
irregular per hit. Recorded for WP11 with the option of a presentation-only facing in `SplashVisual`. Real flame
flipbooks are not made (no media this round); the fire is built from the smoke element and computed shapes.

**Tools.** The effect lab can spawn its subject several times on one spot (`LabSubject::repeat`, `battle_capture
--effect-lab <effect> --repeat <count>x<seconds>`, sheets `<effect>_repeat_*`), each with its own instance seed as
consecutive events in a battle have. The stress stream of `frame_budget` bursts an air weapon's splash only at
airborne units and a ground weapon's only on the ground (`strikes_plane`).

**Gate.** The implementer checklist is in [`slice_review.md`](../evidence/effects/review/slice_review.md) ("Scale-out
round 2"), the material in the packet's section "Scale-out round 2"; the orchestrator runs the independent review.

## Amendment 6: scale-out round 3 (2026-11-11, short note, charter §6)

**Why.** The independent review of round 2
([`independent_review_scaleout2.md`](../evidence/effects/review/independent_review_scaleout2.md)) accepted
`flame_line` and `lance_sweep`, confirmed `shell_impact` equal to `main`, and rejected `cleave_impact`: its front was a
translucent disc with a smooth, unbroken elliptical rim (a "glass bubble"), the same circle on every hit, and it
popped out in one frame (frame 7 to 8). It also asked for degradation frames in the crowd, a frame with jets burning,
and headroom in the decal pool. Presentation data, one shader branch, the effect lab, the stress tool and one
footprint rule change; no simulation state, golden replay or checksum (D-1 holds).

**`cleave_impact`.**

- **A torn front.** A `Front` ring may declare `edge_noise` (schema `BlockDef::Ring`, in [0, 1)): where a third read of
  its own noise is low, its leading edge is bitten back by up to that share of the band's width, so the front is a
  run of dust clumps whose farthest parts stay on the reach and whose outline differs per hit (the noise phase is the
  instance's own). The card record gains a tenth row (`front_edge` and padding); with `edge_noise` 0 the shader takes
  the old path, so the round fronts of `shell_impact`, `acid_burst` and `flak_burst` draw exactly as before (their
  goldens pass unchanged, mean difference 0.000/255, on both drivers). The cleave's front is thinner (0.3 of the
  radius), torn from its first frame (`skirt_break`), with no tinted disc inside.
- **No pop.** The front's intensity no longer holds above 1 and drops at the end: it falls linearly over the last
  60 % of its 0.5 s while its erosion eats it (`skirt_fade`). Measured per pixel (pixels changing by more than 3 %
  between consecutive 40 ms frames, over the cell the sheet shows, full-resolution lavapipe frames): round 2 had
  719 at frame 6→7 and 2 638 at 7→8 (the pop); now every step after the impact (frame 4 on) lies between 696 and
  1 034, each within 1.3 times its neighbours ([`cleave_impact_pops.txt`](../evidence/effects/review/cleave_impact_pops.txt)).

**The footprint trade-off, and the proposal.** The slice test asks that at least 85 % of 72 rays end within ±2 px of
the projected reach. At the default zoom the cleave's 1.5 reach is about 29 px, so ±2 px is 7 % of the radius: any
torn edge deeper than that fails. Measured on radv:

| Front | Rays within ±2 px | Look |
|---|---|---|
| round 2 (thickness 0.5, intensity 1.6–1.8, no bite) | 93 % | the "glass bubble" the review rejected |
| thin (0.3) and broken inside, intensity 1.35, round edge | passes | still a smooth elliptical outline around the clumps |
| torn, `edge_noise` 0.5 (shipped here) | 0 % under the old rule; 89 % under the new one (farthest ray −1.68 px) | a ragged skirt of clumps, a different outline per hit |
| an edge on the reach with notched gaps | 83 % (gaps in about a fifth of the circle) to 85 % (fewer gaps, and the outline is smooth again) | a cog wheel with straight-sided notches |

So under the old rule the look must give: only a nearly round edge passes. **Proposal (taken here, reversible):**
the test gives, narrowly. A front that declares `edge_noise` may end inside the reach by its declared bite
(`edge_noise × thickness` of the reach, projected per ray) on most rays, while (a) no ray may end beyond the reach
by more than 2 px, as before, and (b) its farthest-reaching clumps must lie within 2 px of the reach (the largest
ray error is at least −2 px). The contract a player relies on (the drawn area never claims more than the reach, and
the reach's outline is shown by the outermost dust) is kept; what is given up is that every ray touches the reach.
Round fronts (bite 0) keep the old rule unchanged. If the orchestrator or the owner prefers the old rule, the
fallback is the second row (thin and broken, no bite), which passes but keeps a smooth outline.

**Crowd and budget.**

- `frame_budget` can take review captures at a fixed step (`--frame-step-ms 40`: one simulation tick per frame,
  whatever the frame took; never a figure of record) and several consecutive screenshots (`--capture-frames N`,
  written `<stem>_<k>.png`), each logged with the frame's decal count and the running budget counters.
- **The decal pool is now 6 144** (was 4 096). At one tick per frame the stress load draws 4 206 to 4 289 decals a
  frame (round 2's 3 921 was taken at five ticks a frame, which spawns fewer per drawn frame), which is above the old
  pool; 6 144 leaves about 30 % headroom. At the cap, Detail decals give way first; Critical ones are then written up
  to the pool's capacity and the rest of them are dropped for that frame. A scratch run with the pool forced to
  2 048 shows what that means: about 2 000 Critical decals dropped every frame, and which ones changes from frame to
  frame with the order of the live instances, so lasting marks flicker in and out. That is why the pool must not be
  reached at the stress load ([`stress_capture.txt`](../evidence/effects/review/stress_capture.txt), "Scale-out round 3").
- **Jets in a real battle draw only their Critical layers (finding; proposal, not taken).** A capture of the battle
  without the stream, at the battle camera's default distance (`--camera-distance 22`), ten consecutive frames
  from tick 200 ([`stress_jets_400.png`](../evidence/effects/review/stress_jets_400.png)), shows the jets as a thin
  orange core with the legacy tracer and their scorched bands, but no billowing fire. Cause: the class cap of live
  `Major` effects (96). A flame line lives 3.5 s for its band, so the battle's napalm wheelers and anti-air bursts
  keep more than 96 `Major` instances alive, and a jet born over the cap has its Detail layers (the fire) retired
  at birth. Both effects look as reviewed in the lab; this is the budget at work. Options: count an instance
  against its class cap only while it has live Detail layers (a lasting band then holds no slot), or raise the
  `Major` cap. Either changes the budget rules the slice's crowd was accepted under, so it waits for the
  orchestrator. (At the frame-budget camera, 75 units away, the fire's cards are also below the 6-pixel
  level-of-detail floor.)
- **Acid stains in a crowd (review suggestion): recorded as a proposal, not taken.** Fading the stain's saturation
  or opacity after its first second would change `acid_burst`'s accepted look on its own sheets (the stain holds its
  full disc and wet colour to about 1.8 s by design, rounds 4 and 5). Options for a later round: a crowd-only rule
  (fade a stain's Detail layers when many stains overlap a cell), or a shorter hold reviewed as an acid change.

**Not taken (optional notes of round 2, recorded under Q0002):** `flame_line`'s jet rod with regular notches in
frames 1–4, the oxblood pockets in its billows, `lance_sweep`'s wedge-shaped cooled groove and its larger spatter.
Each would reopen an accepted effect's review; they wait for a polish round.

**Tests.** `effect_slice_gpu_tests.rs`: the footprint rule above (`front_bite`), all seven subjects on radv and
lavapipe, `cleave_impact`'s goldens re-recorded on both; `effect_catalog_tests.rs`: `edge_noise` must be a fraction
and only on a `Front` ring; `instances.rs`: the card record is ten rows; `effect_stress.rs`: consecutive capture paths.

**Gate.** Implementer checklist in [`slice_review.md`](../evidence/effects/review/slice_review.md) ("Scale-out round
3"), packet section "Scale-out round 3"; the orchestrator runs the independent review.

## Amendment 7: scale-out round 4, decided by the orchestrator after independent review 3 (2026-11-11)

**Decision note (charter §6, short note; the decisions are the orchestrator's, after
[`independent_review_scaleout3.md`](../evidence/effects/review/independent_review_scaleout3.md)).** Review 3 accepted
`cleave_impact` (with notes), the 6 144 decal pool and the degradation frames, and found no regression. It accepted the
footprint rule of Amendment 6 with one clause strengthened, judged the behaviour at the decal cap unacceptable as a
design, and found a defect: in a battle the napalm jet reads as a thin beam. The orchestrator decided:

1. **Footprint rule for fronts with `edge_noise`:** the reviewer's change is adopted. The "farthest ray within 2 px of
   the reach" clause must hold in each of eight 45° sectors, not once somewhere. The declared bite
   (`edge_noise × thickness`) is capped at 20 % of the radius, and the catalog refuses more
   (`MAX_FRONT_BITE`). A test shows that a front squashed on one side, or missing a quarter, fails
   (`a_torn_front_must_reach_the_reach_in_every_sector`).
2. **Stable decals at the cap:**
   - An effect reserves its decal slots at birth and keeps them to its end (`client::effects::ledger`).
   - Its decals draw as a whole set or not at all.
   - Above 85 % of the pool, the oldest lasting marks erode out over 0.25 s to make room. An effect that still finds
     no room draws no decal at all.
   - Test: across consecutive frames no drawn decal disappears unless its effect is eroding it out or its layer has
     ended, and every effect draws its marks whole
     (`lasting_marks_at_the_decal_cap_never_flicker_and_draw_whole`, pool of 8).
   - The forced-cap capture is re-taken.
3. **Jets (option 1):**
   - An effect counts against its class cap only while its fire (its non-decal layers) is alive.
   - A newborn effect is never the one stripped: over the cap, the oldest burning effect of the class gives its detail
     up (`over_its_class_cap_the_oldest_burning_effect_gives_its_detail_up`,
     `an_effect_whose_fire_has_ended_no_longer_counts_against_its_class_cap`).
   - The jet's always-kept layers must read as fire: `flame_line`'s body of fire and its burning ground are now
     `Critical` beside the jet's core. Their look in the lab is unchanged; priority only decides what a crowd keeps.
     A jet stripped of its detail is a broad orange body over burning ground, not a beam.
   - The jets are re-taken at the default and the far camera.
4. **Optional cleave note taken:** the torn edge's bite reads two octaves of its noise, so its clumps are not spaced
   at one regular period. Its value on teal is unchanged: a lighter front loses its contrast on the bright ground,
   which the footprint needs.

Presentation data, one shader line, the runtime's budget rules, the effect tests, and the stress tool's report. No
simulation state, golden replay or checksum (D-1 holds). Implementer checklist: [`slice_review.md`](../evidence/effects/review/slice_review.md)
("Scale-out round 4"). Packet: section "Scale-out round 4". This is round 4 of the owner's five (Q0080).

## Amendment 8: scale-out round 5, decided by the orchestrator after independent review 4 (2026-11-12)

**Decision note (charter §6, short note; the decisions are the orchestrator's, after
[`independent_review_scaleout4.md`](../evidence/effects/review/independent_review_scaleout4.md)).** Review 4 accepted
the footprint rule, the decal reservations at the forced cap, the early erosion at the stress load, the jets at
both cameras and the `Critical` body of fire, and found no regression. It rejected `cleave_impact`: to pass the
per-sector clause, round 4 had made the front round again. Its edge was about 40 % smoother, it had no gaps, its
bite was halved, and the four hits shared one outline. This is round 5, the owner's last review round (Q0080).

1. **`cleave_impact`: guaranteed crests (the reviewer's construction).**
   - **Round 3's bite is restored, and a little deeper.** The crest is back at 1.0. The bite's second octave is
     re-contrasted after the mix, so it keeps one octave's spread. `edge_noise` is 0.66, so the bite is
     0.66 × 0.3 = 0.198 of the radius, inside the 0.2 cap.
   - **Twelve guaranteed crests** (`client::effects::front`, mirrored in `sprite.wgsl`'s `front_crest`). The edge
     is on the radius over each crest: 2.5° half wide plus a 1.5° soft edge. The crests sit 30° apart, all turned
     by a per-hit rotation, each jittered by up to ±7°. The rotation and jitter come from an integer hash of the
     ring card's own seed, the same in Rust and WGSL.
   - **Guarantee by construction.** The largest gap between crest centres is at most 30 + 2 × 7 = 44°. With the
     2.5° half width (half the footprint test's 5° ray spacing), every 45° sector of the footprint test has a ray on
     a crest, whatever the hit's noise phase. Between the crests the noise bites freely.
   - **Proof across seeds.** Unit tests over 64 consecutive hits' card seeds check three things:
     - no gap of 45° or more (`the_largest_gap_between_crests_is_below_a_sector_for_every_seed`);
     - every footprint sector has a ray on a crest (`every_footprint_sector_has_a_ray_on_a_crest_for_every_seed`);
     - two consecutive hits place their crests differently (`two_hits_place_their_crests_differently`): the mean
       distance from each crest of one hit to the nearest crest of the next is at least 1° for every pair and at
       least 5° on average. A rotation alone gives 7.5° on average.
   - **In the capture**, the outline correlation between the four hits of the repeat take is measured: the
     72-bin outer-radius profiles, pairwise Pearson correlation (figures in `slice_review.md`).
2. **The pressure mark counts reservations.** The 85 % erosion threshold of Amendment 7 is measured in reserved
   decal slots, not in decals drawn. A reservation includes decals that are too small to draw at the current
   camera. Counting only drawable decals would make the ledger depend on the camera: a zoom-in would suddenly make
   unreserved decals drawable, and they would then either stay hidden or force a burst of erosions. At the
   frame-budget camera, the mark is reached at about 3 900 drawn decals. The real battle stays far below it. The
   round-4 packet's wording is corrected: at the forced cap, no decal that was granted is ever dropped from a frame,
   but the 444 effects refused room drew none of their marks, Critical ones included.
3. **Optional, not taken: a minimum on-screen width for the flame's burning ground at the far camera.** The
   reviewer accepted the far-camera jets as torn fire strips. A screen-size floor on a decal's width would be a new
   rule of the decal shader. It is recorded here for a polish round.

No simulation state, golden replay or checksum (D-1 holds). Implementer checklist:
[`slice_review.md`](../evidence/effects/review/slice_review.md) ("Scale-out round 5"). Packet: section "Scale-out
round 5".

## Amendment 9: the scale-out's four effects shipped; WP7 closed (2026-11-12, short note, charter §6)

**Decision note (charter §6, short note: catalog data, test fixtures and documentation).** The independent review of
scale-out round 5 ([`independent_review_scaleout5.md`](../evidence/effects/review/independent_review_scaleout5.md))
accepts `cleave_impact`. Across the five scale-out rounds this makes all seven effects ready to ship, with no
regressions. This amendment ships the four new ones the way Amendment 3 shipped the slice. It changes no simulation
state, golden replay or checksum (D-1 holds: presentation blocks are read by the client only), and no effect
definition, texture or GPU golden.

**Verdicts per scale-out round.**

| Round | Review | `flak_burst` | `flame_line` | `lance_sweep` | `cleave_impact` | shipped three |
|---|---|---|---|---|---|---|
| 1 | [`independent_review_scaleout1.md`](../evidence/effects/review/independent_review_scaleout1.md) | **accept** | reject | reject | reject | `shell_impact` regressed |
| 2 | [`independent_review_scaleout2.md`](../evidence/effects/review/independent_review_scaleout2.md) | accept | **accept** | **accept** | reject | restored, no regression |
| 3 | [`independent_review_scaleout3.md`](../evidence/effects/review/independent_review_scaleout3.md) | accept | accept | accept | **accept** | no regression |
| 4 | [`independent_review_scaleout4.md`](../evidence/effects/review/independent_review_scaleout4.md) | accept | accept (jets in battle) | accept | reject (round again) | no regression |
| 5 | [`independent_review_scaleout5.md`](../evidence/effects/review/independent_review_scaleout5.md) | accept | accept | accept | **accept** | no regression |

**What ships.**

- **The four new effects** are named in `assets/catalog/units.ron` by their units' own weapon slots:
  - `napalm_wheeler` (Hellion analogue): `(weapon: Primary, splash: Some("flame_line"))`;
  - `tripod_annihilator` (Colossus analogue): `lance_sweep`;
  - `sky_bastion` (Liberator analogue): `(weapon: Air, splash: Some("flak_burst"))`;
  - `stag_behemoth` (Ultralisk analogue): `cleave_impact`.

  Their blocks are those the effect lab and the GPU tests rendered in every round. No Rust names a unit or an effect
  (C5, C6).
- `crates/client/tests/data/effect_lab_blocks.ron` keeps only `burrow_trail`, which no shipped burrowed mode can
  use yet (Amendment 3).
- `effect_catalog_tests.rs`:
  - `accepted_slice_effects_are_named_by_their_shipped_units` now covers all seven effects with their weapon slots.
  - New: `shipped_torn_fronts_hold_every_sector_for_many_hits`. It checks every shipped torn front (today the
    cleave's) for 64 consecutive hits. The seeds are drawn with the effect's real salt and the ring's real layer
    index, through the runtime's own seed functions (`instance_seed`, `placed_seed`). This was review 5's note on
    the test seeds. The unit tests in `client::effects::front` now use those same functions on an arbitrary effect.
- **Units without media stay unpresented.** `tripod_annihilator`, `stag_behemoth` and `volatile_tick` have no unit
  media, so the client cannot present them in a battle. Their effects are shipped and reviewed in the lab and in
  the stress stream, and they will draw once those units have media (WP13, WP14).

**WP7 status: done.**

- The `Ribbon` and `Distortion` blocks and the four splash effects have shipped.
- The projectile impact wiring (a `ProjectileVisual` arrival spawns the unit's `impact` effect) has been in
  `client::effects::systems` since WP4.
- The budget rules of Amendments 7 and 8 (decal reservations, class caps counted by live fire, torn fronts with
  guaranteed crests) are in.
- Travel effects for projectiles are not authored. They move to WP11 with the migration of the legacy looks.
- WP8 (concealment and stance visuals, the Q0075 Veo clips) is not started.

**Open polish (non-blocking, from the scale-out reviews; to be picked up in a polish round, each with its own
review).**

| Effect or area | Item | First raised |
|---|---|---|
| `cleave_impact` | Narrow crest teeth at close zoom on teal: soften their flanks (soft edge about 5–6°) so that they read as shoulders. | scale-out 5 |
| `cleave_impact` | The front's dark value on teal reads as a beaded outline around light dust. | scale-out 3 |
| `cleave_impact` | The front is faint at the default zoom on bright ground. | scale-out 3 |
| `cleave_impact` | Square clods; the crack is not readable. | scale-out 1 |
| `cleave_impact` | The GPU footprint test checks one seed per driver; render more than one seed. | scale-out 5 |
| `flame_line` | A screen-size floor for the burning ground's width at the far camera (Amendment 8). | scale-out 4 |
| `flame_line` | Flare and break up the jet rod in frames 1–4; oxblood pockets in the billows; a blunt cut at the fog edge at 0.104 s. | scale-out 2 |
| `flame_line` (crowd) | Most `Major` effects give up their billows early in a big fight; revisit only if play-testing shows jets thinning too soon. | scale-out 4 |
| `lance_sweep` | Spread the groove's width variation along the line instead of a wedge; larger spatter; the airborne part is weak at the default zoom. | scale-out 2 |
| `flak_burst` | A regular scalloped inner edge (about ten even lobes), with a slight glass-rim read at close zoom; smoke that drifts and grows; shrapnel streaks. | scale-out 1 |
| `shell_impact` | An irregular streak fringe for the blast mark (a new texture); more ring width variation; a draw space in which no instance loses its blast mark. | slice round 4, scale-out 1 |
| `acid_burst` | Meniscus, inner rims, glob shapes, crowd variation, dried crust. Stains dominate the crowd frame (Amendment 6's proposal to fade them in crowds). | slice rounds; scale-out 2–4 |
| `spine_line` | Torn clods; clods a step lighter than the furrows; one detached scar piece. | slice round 5, scale-out 1 |
| Decal ledger | Optional, camera-independent: leave out of the reservations the layers that can never reach the 6-pixel floor, computed once at load. | scale-out 4 |
| Crowd evidence | An annotated crop of one soft retirement would strengthen the degradation take. | scale-out 3 |

**Next.** WP8 to WP15 (§9), with B-107 to B-111 open behind them.

## Amendment 10: WP8 concealment and stance, round 1 (2026-11-12, short note, charter §6)

**What changes.** WP8 of §9 in the client, its shaders and the catalog's presentation data. No simulation state,
golden replay or checksum changes (D-1 holds: the client reads `concealment` and `stance`, which WP2 already
publishes).

- **Held effects.** A `Mode` effect can be *held* on a unit (`EffectRuntime::hold`, `update_held`, `release`): its
  layers marked `held: true` come in over their `duration` and stay until the look is released, then erode out
  over their `exit` (default 0.35 s); its other layers play once. Its anchor and bound values (`UnitRadius`,
  `ChannelProgress`) follow the unit every frame. A held effect counts against no class cap, is never retired by
  the budget and keeps its decal reservation (it shows a unit's state, at most one of each kind per unit). A unit
  first seen in a state is shown settled in it (`hold(.., settled)`: its one-shot layers are not replayed).
- **`Shell` block** (§5.1): `sink` (share of the diameter), `tint`, `rim`, `veil`, `soil`, and `progress` (life =
  the switch's progress instead of age). It is evaluated by `client::effects::stance` and written into a new,
  separate bubble uniform at binding 4 (`ShellUniformGpu`), so `BubbleUniformGpu` keeps its 176 bytes and the
  smog goldens, which hash it, stay unchanged. A vertex stage in `bubble_dome.wgsl` (Bevy's mesh vertex plus the
  sink) moves the bubble into the ground, whose depth hides what is below it. A sunk or veiled bubble casts no
  shadow. Only `Mode` effects may hold layers or have a `Shell` (load errors).
- **Ring arc** (`arc: (bind: ChannelProgress)`, `FLAG_ARC`): a ring drawn over that share of its turn with a lit
  head; **distortion lens** (`lens`): a glass body's radial bend, strongest just inside the rim.
- **Looks** (`effects.ron`): `burrow_dig` (span 1.8 s), `burrow_mound`, `burrow_emerge`, `cloak_own`,
  `cloak_shimmer`, `detected_mark`, `mode_seal_sweep`, `siege_anchor`. `defaults.concealment` names the first six
  and `defaults.mode.transition` the seal; `UnitEffectTables` fills every mode of every archetype from its flags
  (`burrowed`, `is_immobile`) before the unit's block replaces slots. `railgun_crawler` (Siege-Tank analogue)
  names `mode_seal_sweep` and `siege_anchor`; `burrow_scorpion` uses the defaults (its block keeps no mode entry,
  so the lab fixture's `burrow_trail` entry does not repeat one).
- **Progress and settle (D-1, D-6).** `channel_progress = clamp((nominal - remaining) / nominal, 0, 1)` holds at 0
  during the random extra. A morph whose effect states a `span` longer than its channel plays at its own pace:
  `presented = channel_progress × channel / span` during the channel, then the client-local settle continues at
  `1 / span` per second until complete (`StanceTrack`, a component of the bubble; nothing in the simulation). The
  burrow's stats change at 33 ticks and the dig ends at 45 (Q0069 option 2). A cancelled switch releases its morph
  and shows the stance settled again.
- **Fog (D-5, Q0076 R4).** An undetected cloaked enemy's bubble is no longer drawn (`Presence::draws_bubble`); the
  viewer gets only `cloak_shimmer`, a colourless lensed distortion held at its body. Nothing is held on a
  `Hidden` unit. Stances, morphs and marks are held only on a unit shown as it is.

**Decisions (Q0002, reversible).**

- A burrowed unit has no cover look: its stance (sunk in its mound) reads to its owner and to an enemy that detects
  it alike. `detected_mark` marks detected *cloaked* enemies only.
- An enemy seen starting to burrow disappears when its concealment flips at the channel's end (33 ticks), about
  three quarters sunk; the dig's one-shot ground layers spawned while it was visible play out. Holding the
  remaining 12 ticks of the dig for an enemy would draw a unit the fog hides.
- The bubble shader's old sine-stripe cloak stays only for the Visual Lab preview's checkbox; in a battle the
  effects drive every concealment look (`ShellUniformGpu::driven`).
- No pulse in `detected_mark` (the `Shell` block has no period); a static cold outline light and a slight lens.
- Own concealed units stay readable (Q0076; input from the WP13 review, whose burrowed-mode clip was nearly
  invisible under the old whole-bubble cloak fade): the burrowed dome shows the unit's picture unveiled, with
  only a thin soil band at the ground line (`soil: 0.12`, no media tint, no veil) and an earth-coloured outline;
  `cloak_own` veils the picture to about half (`veil: 0.45`). The old fade no longer applies in a battle.
- R5 (burrowed trail) and R6 (a burrowed attacker's line drawn where seen) are unchanged from Amendment 2.
- No new texture and no spend: the looks use `smoke_puff`, `debris_atlas`, `scorch`, `earth_scar`,
  `ground_crack` and `noise_tile`.

**Tests.** `effects::stance` unit tests (hold during the random extra, clamp, leaving direction, a morph longer or
shorter than its channel, the shell sum); `crates/client/tests/stance_gpu_tests.rs` on the production skirmish
(radv and lavapipe): the burrowed bubble's above-ground area at most 35 % of the standing one and its looks inside
1.8 radii; the undetected cloaked enemy bends what is behind it with no tint (the patch's mean colour moves at most
1.5 levels; at most 5 % of bent pixels take a colour not on the ground within 12 px) and without its looks changes
at most 2 pixels; the siege switch's arc within ±5 % of its progress; and through real mode commands, the dig goes
on after the channel ends until fully sunk, never rises, and the emerge never sinks. `battle_fog_tests.rs` now
expects the shimmer look and no drawn bubble for an undetected cloaked enemy, and the cover look of each case.

**Gate.** The packet's section "WP8 round 1"; the orchestrator runs the independent review (Q0080, up to five
rounds).
