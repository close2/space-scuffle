# Blueprint: Effect Visualization (Burrow, Cloak and Detection, Modes, Splash, Areas, Tethers, Status)

**Owner request (chat, 2026-11-09):** "please use one round to evaluate how to visualize burrowed units, splash damage
(banelings,...) and other effects (viper pull, lurker,...)".

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-09 against `main` at `0a36b3f`.
It is an evaluation and a plan. Nothing was built or rendered while planning (read-only inspection of the code, the
catalog and the media ledger), so every claim about pixels or timings is a target that a work package measures.
Line numbers drift, so search for the quoted symbol. Claims not confirmed are marked **verify**.

**Kind of change.** A full plan (charter §6, Q0054): it spans `sim_types`, `sim_core`, `client`,
`client_diagnostics` and `gate`, adds snapshot and view records, and introduces a new presentation data file. It is
designed so that **no battle outcome, golden replay or checksum moves** (§5, D-1); a work package that sees a golden
move stops (§7).

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) / [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) follow StarCraft II; S-6 original looks | Standing rules | §3, D-3 |
| [Q0069](../questions/open/0069-lurker-burrow-sections.md) when a burrowing Lurker becomes burrowed | Answered (option 2: `Stats` at 33 ticks, `Actor` animation to 45) but not yet implemented | D-6 |
| [Q0070](../questions/open/0070-lurker-without-burrow-button.md) no Burrow button in the skirmish | Open, default option 1; plan `2026-11-09-client-mode-commands` adds the button | §2.4, WP9 |
| Q0075 (new) Veo spend for morph and mode clips | Filed by this plan, default "no Veo spend; shader only" | D-4, WP10 |
| Q0076 (new) what an enemy sees of an undetected cloaked unit | Filed by this plan, default "shimmer, as StarCraft II" | D-5 |
| Q0077 (new) the missing caster spells (Abduct, Psionic Storm, Blinding Cloud, …) | Filed by this plan, default "presentation contract only here; mechanics in a later plan" | D-8, §9 |

---

## 1. Summary

1. **What the player cannot read today.** Of the effects the owner named, two exist in the simulation and are badly
   shown, and one does not exist at all:
   - **Burrow** (the Lurker analogue `burrow_scorpion`, the only concealed state in shipped data) is published as
     `cloaked: true` and drawn as the generic refraction shimmer **to every viewer, enemies included, detected or
     not**. That is an information leak: StarCraft II shows an undetected burrowed enemy not at all. The transition
     (45 ticks plus up to 4.5 random) has no presentation; the unit simply turns translucent at the end.
   - **Splash damage** (Baneling, Siege Tank, Hellion, Colossus, Lurker, Liberator and Ultralisk analogues) has **no
     presentation at all**. The simulation resolves zones of 100/50/25 % but emits only the direct hit's tracer,
     projectile or slash; a Baneling shows a projectile from itself and its own death shatter, never a 2.2-radius
     burst.
   - **Viper pull (Abduct)** does not exist in the simulation: the Viper analogue `tongue_chameleon`, the High
     Templar analogue `cerebral_elder` and the Infestor analogue `mycelium_shaman` have no abilities in
     `units.ron`. Psionic-Storm-like hazard zones exist in `sim_core` (`SpawnHazardZone`, `active_hazards`) but no
     catalog ability uses them.
2. **Recommendation.** Real GPU rendering (option (a), §4) for every effect class: shader- and mesh-based ground
   decals, rings, ribbons and shell modulations, all sized and timed from simulation data. Veo clips are kept for
   what they are good at, the unit inside its bubble; they are a later, owner-gated polish for morph and mode
   poses (Q0075), never for areas, rings or tethers, whose size must equal the simulation's radius to the pixel and
   which Veo cannot deliver with alpha.
3. **Contract.** The simulation publishes facts, the client presents them:
   - per bubble: a `concealment` (none, cloaked, burrowed) replacing `cloaked: bool`; a `stance` (mode index and
     transition progress); `status` flags (overclock, dash, slow, barrier) replacing `speed_boosted`; a barrier ratio;
   - per tick: a new `SplashVisual` event per splash search, carrying the zone radii, the source type and the weapon
     slot;
   - per frame: a list of active areas (hazard zones today), and a list of links (tethers) for mechanics to come;
   - per viewer: `PlayerView::revealed`, the concealed enemy entities the viewer's detection reveals.
   None of these is hashed, none draws from the random generator, and the snapshot is written after the tick, so
   every checksum and golden is unchanged by construction (D-1).
4. **Looks live in data.** A new client-read file `assets/catalog/effect_visuals.ron` maps each archetype's weapon
   slot to a look (acid burst, blast, flame, ground spines, cleave arc, thermal sweep) and holds the colours and
   timings of every effect. The simulation's `WeaponSpec` (which is hashed) is not touched (D-2).
5. **Work packages.** WP1–WP3 simulation contract (tests first, bit-identical goldens), WP4–WP8 client systems per
   class, WP9 mode-command integration, WP10 media (gated), WP11 showcase and storyboard, WP12 bookkeeping (§6).

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

## 4. Options per Effect Class, Trade-offs, Recommendation

### 4.1 What the charter allows

- **(a) Real GPU rendering** (shaders, meshes, decals, ribbons, shell modulation) driven by `RenderSnapshot` and
  `PlayerView` data is the normal path (§3.2) and is real rendering, not a stand-in: the effect is the product, as
  the existing tracers, beams and shatter are. §1.2 forbids faking *media* (crude shapes pretending to be a video,
  wobbling a still); a shader decal that draws a splash area at its true radius pretends to be nothing else.
- **(b) Veo flipbooks** are allowed within the $200.00 total budget (D-12 of the Veo plan). Ledger: 217
  `request_started` lines, **$86.80 spent, $113.20 left** (≈ 283 clips at $0.40). Paid generation needs the owner's
  standing permission (memory: paid media rule), so this plan makes no Veo call and gates media on Q0075.
- **Feasibility of Veo for effects.** Veo 3.1 outputs opaque RGB video on the house backdrop `#0B101B`; there is no
  alpha channel. An effect clip would need either additive blending (the backdrop adds a blue-grey veil unless keyed
  to black, which loses dark smoke) or a luma/chroma key (fringes on painterly edges). Veo frames a 3/4 isometric
  subject; ground effects in the game are seen from the battle camera at varying pitch, so a baked perspective
  never matches. A splash must be exactly its radius on the ground, a tether exactly between two moving bodies, a
  Lurker line exactly 8 long: a clip would be stretched and squashed per event, which is the "wobble a still"
  failure of §1.2 in disguise. Effects also come in hundreds per second in big fights: atlas memory for several
  effect clips (each a 2D array texture like the unit atlases) competes with the unit atlases. **Conclusion:** Veo
  is unfit for world-space effects and fit for the unit inside its bubble, which is where it already lives.
- **(c) Hybrid** = (a) for every world-space effect + (b) for unit poses that a mode changes (sieged tank, burrowed
  Lurker, unburrow morph), as extra flipbook slices.

### 4.2 Per class

| Class | (a) GPU | (b) Veo | (c) Hybrid | Recommendation |
|---|---|---|---|---|
| A. Concealment (burrow, cloak) and detection | Bubble sinks into the ground along the transition (vertex offset, fragment clip below the terrain height under it), dirt-ring decal and dust puff at start/end; owner sees the sunk shell with a dashed rim; detected enemy drawn sunk with a detection tint; undetected burrowed enemy not drawn; cloak keeps the refraction shimmer (owner: brighter rim; enemy: shimmer only, not pickable). Cost M (bubble shader branch, a decal instance, fog rule). | Burrow/unburrow morph clip per burrowing identity (2 × $0.40 + retries); cannot show the in-ground relation to terrain, only the creature digging inside its bubble. | Shader carries the read; a morph clip plays inside the sinking bubble. | **(a) now, (c) later if Q0075 allows.** The leak fix and the in-ground read need no media; the clip is polish. |
| B. Mode stance and transition (siege) | Stance ring at the base (anchor clamps or a ground seal per `is_immobile` mode), transition sweep around the rim driven by progress; sieged weapon's range is not drawn (game shows none). Cost S. | Sieged Idle/Attack clips (the game's tank looks different sieged): 2–3 clips per mode identity. | Shader ring + mode clips as an atlas slice keyed by mode. | **(a) now, (c) later (Q0075).** Needs an atlas extension (slice per `(state, mode)`) only when clips exist. |
| C. Splash areas (instant) | Ground decal of the reach radius with a bright full-damage core (zones drawn as concentric falloff), look per weapon from data: acid splatter stain, blast ring, flame lick, spine eruption ring, cleave arc. 0.3–0.6 s, from `SplashVisual`. Cost M. | Infeasible (radius exactness, no alpha, perspective). | — | **(a).** |
| D. Weapon delivery (spines line, flame jet, lance sweep, burst) | New `combat_fx.wgsl` kinds: ground-spine line (travels from source to the splash point and on to full range at the line's spacing), flame cone, sweep ribbon, burst flash. Look selected in data. Cost M. | Same objections as C. | — | **(a).** |
| E. Persistent areas (hazard zones; later Liberator zone, clouds) | Area decal + animated column (storm-like) from the `areas` list, fading with remaining time; drawn for every viewer who sees the centre. Cost S–M; built and tested now with a test-only catalog ability, shipped looks when a spell lands. | Infeasible (looping, radius, alpha). | — | **(a).** |
| F. Links and pulls (Abduct, future heal/drain beams) | Tether ribbon (beam ribbon shader, catenary sag, hook end) between two bubbles from the `links` list; the pulled body moves by its published position (the simulation moves it). Cost S, after the mechanic exists. | Infeasible. | — | **(a), contract only now** (D-8). |
| G. Status (overclock, dash, slow, barrier) | Shell flags: overclock pulse (exists), dash streak (velocity-stretched wake), slow frost/goo overlay, barrier golden outer shell scaled by absorb ratio, flash on absorb. Cost S. | — | — | **(a).** |
| H. Hits, impacts, deaths | Projectile impact starburst at arrival (client-only), barrier absorb flash; shatter exists; optional scorch decal (10 s fade) on death. Cost S. | Die clips exist in the vocabulary but the shatter is the house style. | — | **(a).** |

---

## 5. Design Decisions

### D-1. The simulation publishes facts; nothing hashed changes

- All additions are in `RenderSnapshot` (built after the tick in phase 10, "changes no authoritative state and no
  checksum") and `PlayerView` ("derived output: not checksummed").
- No new `Entity` field, no new catalog field on `WeaponSpec` (which is hashed through `Entity::siege_weapon` and
  `ActiveProjectile::weapon_spec`, `checksum/encoding.rs` `fields!(WeaponSpec …)`), no random draw, no change of
  iteration order. Events are appended to the existing `pending_events` buffer.
- The one catalog addition the simulation reads, `UnitModeSpec::is_burrowed` (D-3), is read only by the snapshot
  writer here. `UnitModeSpec` is not part of the hashed state: `checksum/encoding.rs` encodes the entity's
  `active_mode` name but has no `fields!(UnitModeSpec …)` (checked at `0a36b3f`). Should that change before WP2,
  the field goes in its own latent commit with a schema bump and the goldens are re-recorded with that cause only.
- **Transition progress without new state.** A mode channel publishes `remaining_ticks` (the entity's
  `channeling_remaining_ticks`) and the direction's **nominal** ticks from the catalog (`cast_ticks` or
  `leave_cast_ticks`, without the random extra). The client shows progress
  `clamp((nominal − remaining) / nominal, 0, 1)`: the random extra reads as a short hold before the morph begins,
  which is how the game's `RandomDelay` behaves (a delay, then the morph). Storing the drawn total on `Entity` was
  rejected: it is presentation state in the simulation (plan `2026-11-04-battle-flipbook-animation` D-7) and would
  bump the checksum schema.

### D-2. Looks are client data, keyed by catalog data

New file `assets/catalog/effect_visuals.ron`, read by the client only (like `map_entity_visuals.ron` and
`bubble_optics.ron`), validated at load with typed errors and a `runtime_catalog`-style test that it covers every
weapon with `splash` and every mode:

```ron
(
    splash_looks: { AcidBurst: (...colours, duration_secs, stain_secs...), Blast: (...), Flame: (...),
                    GroundSpines: (...), Cleave: (...), ThermalSweep: (...) },
    // The look of each weapon, by archetype slug and weapon slot. Data names data; Rust names none (C5).
    weapons: [ (unit: "volatile_tick", slot: Ground, delivery: Burst, splash: AcidBurst), ... ],
    default_splash: Blast,
    concealment: (owner_rim: ..., detected_tint: ..., shimmer_strength: ...),
    stance: (...), status: (...), areas: (...), links: (...),
)
```

The client resolves `(UnitTypeId, WeaponSlot)` from the event to a look once per session into a dense table, so the
per-event cost is an index. A missing entry falls back to `default_splash` and is reported by the coverage test,
not at run time. `WeaponVisualKind` in the hashed `WeaponSpec` keeps selecting the delivery for the existing kinds;
the data file may override the delivery (`Burst`, `GroundSpines`, `FlameJet`, `ThermalSweep`) for presentation
only. Changing the Lurker's `visual_kind` in `units.ron` was rejected: it is hashed and would move checksums of
every world with a burrowed Lurker.

### D-3. Concealment is data, not identity

- `BubbleRenderInstance::cloaked: bool` becomes `concealment: Concealment` with `None`, `Cloaked`, `Burrowed`.
  `Burrowed` when the entity is concealed and its active mode has `is_burrowed: true` (new `UnitModeSpec` field,
  default `false`, the game's `Buried` flag, set on `burrow_scorpion`'s `burrowed` mode); `CloakToggle { is_burrow }`
  maps likewise. `Cloaked` otherwise.
- The client never names a mode: it reads `concealment` and `stance`.

### D-4. Media only by owner decision (Q0075)

No Veo call in this plan. WP10 (morph and mode clips) runs only if Q0075 is answered yes, and adds slices keyed by
`(AnimState, mode index)` to the flipbook atlas, with the media style's action text per mode in
`media_style.ron`. Until then the shader carries every read; the unit clip inside a sunk or anchored bubble stays
the Idle/Walk/Attack clip of the default stance, which is a known, recorded limitation (proposed register entry).

### D-5. Fog, detection and picking

- `PlayerView::revealed: Vec<EntityId>`, sorted: the entities **not** owned by the viewer that are concealed and lie
  in a cell the viewer's detection covers (`VisionGrid::is_detected`) **and** in a `Visible` cell. Refilled in place
  (the view's invariant). A concealed entity not listed is hidden from that viewer.
- Client rule, in `BattleFog` (one place): an enemy bubble with `concealment != None` and not in `revealed` is
  *undetected*: `Burrowed` → `Visibility::Hidden` and not pickable; `Cloaked` → drawn as shimmer only (Q0076
  default), not pickable, no health bar, never inspected; an inspected unit that becomes undetected stops being
  inspected (as `apply_battle_fog` does for fog).
- Events are admitted as today, by where they land (`reveals`); a Lurker's spines are shown, the Lurker is not.
  `SplashVisual` anchors at its centre; areas and links are judged per frame by their centre / both ends.
- Under no fog (sandbox, editor, headless) the truth is shown, as today: concealed units are drawn with the owner
  look.

### D-6. Burrow timing and Q0069

The bubble sinks over the published progress. When Q0069 option 2 is implemented (weapon and cloak at 33 ticks, the
`Actor` dig to 45), the simulation's channel ends at 33 and the concealment flips then; the client finishes the dig
locally for the remaining `actor_ticks − channel_ticks` from a presentation field in `effect_visuals.ron`
(`burrow_settle_secs`) or, if that plan adds an `actor_ticks` catalog field, from it. Client-local presentation state
keyed by entity is allowed (§3.2); the simulation keeps none.

### D-7. Event delivery and steady state

- Simulation: `SplashVisual` is a fixed-size enum variant (no `Vec`), pushed into `pending_events`, whose capacity
  is reused (`write_render_snapshot` appends and keeps capacity). `areas` and `links` are `Vec`s in
  `RenderSnapshot`, cleared and refilled like `bubbles`. A line splash emits one event per search point, bounded by
  the authored offsets.
- Client: splash decals, impact flashes and spine lines use the existing pooled `CombatFxMaterial` slots and the
  `CombatLightEffect` spawn pattern (C-17); persistent overlays (stance ring, burrow decal, barrier shell, area
  decal, tether) are **retained instance buffers** sized to the peak and updated in place, never spawned per frame.
  `sync_steady_state_tests.rs` and `sync_allocation_count_tests.rs` gain windows with a burrowed unit, an active area
  and a stream of splashes.

### D-8. Links and areas are general contracts

`RenderSnapshot::areas: Vec<AreaView { id, owner, center: [f32; 3], radius, remaining_ticks, total_ticks,
kind: AreaKind }>` (today `AreaKind::Hazard`, filled from `active_hazards`) and
`RenderSnapshot::links: Vec<LinkView { from: EntityId, to: EntityId, owner, kind: LinkKind }>` (empty until a
mechanic fills it; `LinkKind::Pull` reserved for Abduct). A pull moves the target in the simulation; the client only
draws the tether and the published motion. Adding Abduct, Storm, Blinding Cloud, Fungal Growth, Liberator zones is
a mechanics plan (Q0077).

### D-9. CI and lavapipe

All new shaders are WGSL validated by `gpu_validation_tests.rs` (naga) and rendered in GPU tests through
`crates/client/tests/support/gpu_run.rs` (radv locally, lavapipe on CI, C21). Time-driven animation reads the
effect's own age, never wall clock, so a capture at the showcase's fixed 40 ms step is reproducible. The cloak
shimmer's `globals.time` term stays (it is already there) but the tests pin the frame time. Smog is untouched; its
host-independent goldens must stay bit-equal.

---

## 6. Work Packages (Commit Order, Tests First)

Each WP: tests first (red), then code, `cargo gate quick`, hand back; a verifier runs `cargo gate package` (Q0061).
Sizes: S ≤ 1 day, M 2–3 days, L ≥ 4 days.

| WP | Content | Tests (written first) | Exit | Size |
|---|---|---|---|---|
| WP0 | This plan's row; file Q0075–Q0077; register entries proposed in §8 | — | Committed | — |
| WP1 | `sim_types`: `Concealment`, `StanceView { mode: Option<u8>, transition: Option<ModeTransitionView { entering: Option<u8>, remaining_ticks: u16, nominal_ticks: u16 }> }`, `StatusFlags` (overclock, dash, slowed, barrier up, barrier hit), `barrier_ratio`, `CombatEventVisual::SplashVisual { center: [f32; 3], full_radius, reach, source_type: UnitTypeId, slot: WeaponSlot, owner }`, `AreaView`, `LinkView`, `PlayerView::revealed`; `UnitModeSpec::is_burrowed` | `snapshot_serialization.rs` round trip with every new field and variant; `anchor()` of `SplashVisual` is its centre; `PlayerView::clear` empties `revealed` | `sim_types` green; old RON snapshots in tests updated (serde defaults where a reader needs them) | M |
| WP2 | `sim_core` writer: `bubble_instance` fills concealment, stance (settled-mode query shared with plan `client-mode-commands`), status, barrier; `apply_splash` emits one `SplashVisual` per search point (after the hits, tagged with the source type and slot); `write_render_snapshot` fills `areas` from `active_hazards`; `write_player_view` fills `revealed`; catalog: `is_burrowed: true` on `burrow_scorpion`'s mode | New `crates/sim_core/tests/effect_snapshot_tests.rs`: a burrowing unit publishes progress 0 → 1 then `Burrowed`; a sieged unit publishes its mode; each splash weapon of the shipped catalog emits events whose radii equal its zones; a line splash (scratch archetype in RON) emits one per offset; a hazard zone (test-only ability in RON) appears in `areas` until it expires; overclock/dash/slow/barrier flags. `player_view_tests.rs`: an enemy burrowed unit is in `revealed` only under the viewer's detector, never the viewer's own, never under fog | Every golden replay and matrix battle **bit-identical**; checksum schema unchanged; `published_actions_tests` green | M |
| WP3 | Determinism proof | `golden_replay_tests` (release, `wasm32-wasip1`), full matrix; a test that snapshot writing twice from the same world gives equal records (no hidden state) | Bit-identical, recorded in the evidence file | S |
| WP4 | Client A: concealment and detection. `BattleFog` rule (D-5), picking and inspection guard, bubble shader burrow branch (sink + clip, owner rim, detected tint), burrow ground decal instance, cloak look split owner/enemy | `battle_fog_tests.rs`: hidden/shimmer/shown matrix (own, enemy undetected, enemy revealed, no fog); `battle_input_tests.rs`: an undetected enemy cannot be selected or targeted by click; GPU test `effect_visual_tests.rs` pose "burrow": above-ground pixel area of a burrowed bubble ≤ 35 % of the same bubble unburrowed, decal pixels present within the projected ring; undetected enemy pose: zero pixels differ from the empty-ground pose outside a 2-pixel tolerance | Leak closed; GPU test green on radv and lavapipe | M |
| WP5 | Client B: stance and transition. Ring at the base, progress sweep, client-local settle (D-6) | Unit tests of progress maths (hold during the random extra, clamp, leaving direction); GPU pose "siege mid-transition": ring arc length within ±5 % of progress | Green | S |
| WP6 | Client C/D: `effect_visuals.ron` loader and coverage test; splash decal (`combat_fx` kind 6 ground decal on a terrain-hugging quad), weapon deliveries (kinds 7 spine line, 8 flame jet, 9 sweep, 10 burst); projectile impact starburst at flight end (client-only) | Loader: typed errors, coverage of every `splash` weapon and every mode; `vfx` unit tests: a `SplashVisual` of reach r spawns a decal scaled 2r; GPU pose "splash": lit pixels of the decal lie inside the projected reach circle + 2 px and cover ≥ 60 % of the projected full-damage disc at peak; steady-state window with a splash stream adds no material once the pool is warm | Green; `sync_steady_state` and valgrind allocation test green | L |
| WP7 | Client E/F: area decals from `areas`, tether ribbons from `links` (driven by a test-only snapshot), both from retained instance buffers | GPU pose "area": decal radius ±2 px; fog: an area outside sight is not drawn; steady-state window with a live area | Green | M |
| WP8 | Client G/H: status overlays (dash streak, slow overlay, barrier shell and absorb flash), scorch decal on death (optional, data-switched) | Shader flag unit tests; GPU pose "barrier": outer shell pixels present only while `barrier_ratio > 0` | Green | S |
| WP9 | Integration with mode commands (after plan `2026-11-09-client-mode-commands` lands): skirmish headless test burrows and sieges through input and checks the published stance and the fog rule end to end | `skirmish_match_tests.rs` extension | Green | S |
| WP10 | Media (only if Q0075 = yes): atlas slices keyed by `(AnimState, mode)`; `media_style.ron` mode action texts; clips for `railgun_crawler` sieged Idle/Attack, `burrow_scorpion` Idle/Walk/burrowed Idle/Attack and the morph (plus `volatile_tick` Idle/Walk to field it in a showcase); dry run, then generate within the budget | `asset_baker` prompt validation; `runtime_media_tests.rs` admission of mode slices | Clips verified in review | M + spend (≈ 12 clips, $4.80, plus ≤ 5 retries) |
| WP11 | Showcase: a storyboard beat in the existing Automaton battle where the tank analogue sieges (scenario command) and splashes, a Hellion-analogue flame and a projectile impact; a Swarm beat with burrow and Baneling burst only after WP10 (media required to field those identities) | `showcase_tests.rs`: the storyboard names a beat whose take window contains ≥ 1 `SplashVisual` and one mode transition | CI showcase renders on lavapipe within its time budget | S |
| WP12 | Bookkeeping: evidence file `docs/evidence/2026-11-09-effect-visualization.md` (captures, pixel statistics, bit-identity), `DESIGN.md` presentation row, register entries closed, `crates/gate/suites.ron` rules for every new test file (C22), plan row → Done | `workspace_checks`, `suite_catalog_tests.rs` | Green | S |

Parallelism: WP4–WP8 are independent after WP2 and may run as two packages (A+B, C–H); at most two building agents
at once (Q0055).

---

## 7. Measurement and Stop Rule

- **Expected:** every golden replay checksum and every matrix battle outcome bit-identical at every WP (D-1). The
  snapshot is written after the tick and is not hashed; `PlayerView` is not hashed.
- **Stop rule:** if any golden or matrix result moves, stop the WP, find the cause (a new field read by the tick, an
  RNG draw, an iteration order change) and hand back; do not re-record. The only sanctioned re-record is the
  fallback of D-1 (`UnitModeSpec` hashed by then), in its own latent commit naming that cause.
- **Performance:** the frame-budget fixture (`client_diagnostics` `frame_budget`) before and after WP6 and WP7 on the
  400-unit swarm; target ≤ 5 % more frame time with 200 splashes per second (**measure**; Q0018 context).
  Simulation throughput benchmark unchanged within noise (snapshot writer only).

---

## 8. Risks

| Risk | Mitigation |
|---|---|
| Information leak persists through another path (health bars, minimap, selection panel, sound) | One rule in `BattleFog`, every reader asks it; WP4 tests each reader with an undetected enemy |
| Decals z-fight or float on cliffs and ramps | Terrain-hugging quad sampled from the client height field (the mesher's height), depth bias as `CombatFxMaterial`; GPU pose on a ramp |
| Too much visual noise in big fights (hundreds of splashes) | Durations and alpha in data; pooled slots cap the live count; peak measured in WP6 |
| Lavapipe too slow for the showcase with more effects | Effects are cheap screen-space quads; the showcase render time is measured in WP11 against its CI budget |
| `UnitModeSpec` becomes hashed before WP2 | D-1 fallback (latent schema commit), bit-identical outcomes still required |
| Overlap with `client-mode-commands` and `friendly-blocking` | Shared settled-mode query and `is_burrowed` field: first to land adds, the other reuses (§2.4) |
| Players misread the burrowed Lurker's attack because the unit clip stays Idle | Attack clip timed from `attack_swing` (row 25) in WP5; sim fix recorded, not done here |
| Veo clips later clash with the sinking shader | WP10 composes inside the shell; the shader stays authoritative for the in-ground read |

---

## 9. Out of Scope (Recorded, Q0002)

- New mechanics: Abduct, Psionic Storm, Blinding Cloud, Parasitic Bomb, Fungal Growth, Liberator defender zone,
  Widow-Mine-like units, permanent cloak of the Dark Templar and Observer analogues, Banshee cloak, burrow for other
  Zerg analogues, Tunneling Claws (B-38). The contracts of D-8 make their presentation a small client WP each
  (Q0077).
- Simulation fixes the inventory found: anchored units never publish `Attack` (row 25, `current_anim` hashed);
  B-18/B-19/B-83/B-85 (handled by their own plan; the presentation follows automatically).
- Detection range rings for detectors, range indicators, sound.
- Sustained beams merged across strikes (row 17).

**Proposed register entries** (numbered when filed, after the highest ID in use, B-105 on `friendly-blocking-r2`):
an undetected burrowed enemy is drawn to every viewer (closed by WP4); splash damage has no presentation (WP6);
mode transitions are invisible (WP5); barrier, dash and slow are invisible (WP8); a projectile has no impact
(WP6); the unit clip does not change with a mode (WP10 or recorded).

---

## 10. Owner Questions Filed

- **Q0075 — Veo clips for morphs and modes.** Default: no spend; the shader carries burrow and siege, the unit clip
  stays the default stance's. Option: WP10, ≈ 12 clips ($4.80 + retries) of the $113.20 left.
- **Q0076 — Undetected cloaked enemies.** Default: a faint shimmer as in StarCraft II, not selectable or
  targetable. Option: hide entirely (stronger than the game). Burrowed enemies are hidden either way.
- **Q0077 — Caster spells.** The Viper, High Templar and Infestor analogues have no abilities, so "Viper pull" cannot
  be shown. Default: this plan builds the presentation contracts (areas, links) and their tests; the mechanics go
  in a later plan in StarCraft II order of impact (Psionic Storm, Abduct, Fungal Growth, Blinding Cloud).
