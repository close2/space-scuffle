# Blueprint: Every Known Deviation From StarCraft II, Inventoried and Scheduled as Bugs

This plan carries out the owner's standing rule of 2026-11-05. It lists every place where the simulation, the
catalog or a project document is known to differ from StarCraft II, says for each one whether the owner has
sanctioned it, and schedules the rest as bug fixes. The packages are ordered by how much they change play.

> **Owner, chat, 2026-11-05:** "always use the SC2 mechanics unless I specifically tell to deviate and mention the
> deviation. When you see a deviation which has not been explicitly mentioned as deviation, consider it a mistake or
> bug."

**Status of this document.** Stage 1 blueprint under charter §6. Written on 2026-11-05 against `main` at
`a27ce2a`, with combat alignment WP0 to WP12 and WP15 to WP17 merged, the life-regeneration delay merged and the
turn-before-move plan proposed. Line numbers drift, so search for the quoted symbol. A claim that could not be
confirmed while planning is marked **verify**. A statement about StarCraft II without a citation is marked
**(unsourced)**. Each such statement is a research step of its package and is never a value to apply. No planning
probe was run, because a gate was running in the main checkout. Every effect named below is therefore expected,
not measured.

**What "StarCraft II" means here.** The reference is the one the audit uses: SC2Mapster SC2GameData at
`5444e12f13` ("SC2 5.0.14.94342"), layer chain `core → liberty → swarm → void → voidmulti`
(`docs/evidence/2026-11-04-catalog-time-audit.md` §2.2), cited as `layer` File:line. Liquipedia is the secondary
source, read as wikitext through `index.php?title=…&action=raw`, because the rendered pages return 403 to tools.
Where the two disagree, the data win, as in the audit.

**Questions.**

| Question | State | Where |
|---|---|---|
| The standing rule above | Given in chat, 2026-11-05. WP0 records it as closed question Q0051 and in charter §8 | WP0 |
| [Q0008](../questions/closed/) StarCraft II where the specification is silent | Superseded by the standing rule, which is stronger | WP0 |
| [Q0035](../questions/closed/0035-melee-contact-rules.md) melee contact rules | Answered "Option 1 (no change … for now), but documented/todo for future investigation" | Not a sanction; recorded bug B-50, WP16 |
| [Q0037](../questions/closed/0037-scan-ranges.md) scan ranges | Answered option 3, low priority | Combat alignment WP14; this plan moves it after WP4 here |
| [Q0050](../questions/open/0050-turn-before-move.md) turn before move | Answered (b) with the tolerance "one tick's turn" | Sanctioned rule S-3; turn-before-move plan |
| [Q0006](../questions/closed/) unreachable move | Owner's choice | S-4 (also matches StarCraft II) |
| [Q0007](../questions/closed/0007-combat-baselines.md) item 5, splash ignores cloak detection | "Keep all five as committed" | Matches StarCraft II, so it is not a deviation (S-2) |
| Q0052 (new, filed by WP0) | The sanctioned list S-1 to S-6, with default "treat as listed" | WP0 |
| Later questions | Each research step that cannot find the game's rule files one, with a default | §5, each WP |

---

## 1. Summary

1. **Inventory (§3).** 78 bug IDs (B-1 to B-78) in 13 areas. Six further items are sanctioned or are not
   deviations (S-1 to S-6, §4). Three bugs are being fixed elsewhere: B-31 `StationaryTurningRate` and B-40
   turn-before-move inside the turn-before-move package, and B-12 scan ranges as combat alignment WP14. 23 open
   questions about the game's behaviour (U-1 to U-23, §5) block a precise fix until they are researched, so every
   package that holds one begins with a research step.
2. **Work packages (§7).** WP0 to WP19, in order of gameplay impact:
   - rules that touch every battle and have a sourced fix come first (M-22, multi-strike timing, target selection);
   - then the damage model (attribute bonuses, spillover, splash geometry) and the units that today shoot
     targets they cannot shoot in StarCraft II;
   - then the non-time half of the catalog audit;
   - then capabilities, abilities and the economy, which are features with their own plans.
3. **Order against combat alignment.** This plan runs in lane B after the turn-before-move package. Combat
   alignment WP13's question closures (steps 1 to 4) are documentation and may run first. They close Q0035 and Q0045,
   and the closure texts must call what remains a recorded bug (WP0 gives the wording). WP13 step 5, the end-of-lane
   `DESIGN.md` Combat row, is folded into this plan's WP19, because WP1 to WP8 rewrite exactly what that row
   describes. Combat alignment WP14 (scan ranges) moves after this plan's WP4, because scan range decides what the
   new acquisition rule sees, and WP14 says itself that it should be measured on the final engagement rules (§6 D-1).
4. **Nothing is accepted by deferral.** A deviation that no package fixes stays in the bug register (WP0) under its
   ID until it is fixed or the owner sanctions it in words.

| WP | Title | Kind | Changes results? | Size |
|---|---|---|---|---|
| WP0 | Rule recorded, bug register, sanctioned-list question, wrong doc statements | docs | no | S |
| WP1 | M-22: a ready weapon that its turn aligns swings on the same tick | rule | yes: every battle with a hull weapon | S |
| WP2 | Air-only weapons no longer fire at the ground (Viking, Corruptor, Liberator AG) | data (+ test) | yes: battles 12c, 35 (verify); skirmish | S |
| WP3 | Stimpack only from the catalog; the life rule; start energy | rule + data | 25b only (verify) | S |
| WP4 | Target selection: threats, per-archetype attack target priority, keep the target, closest only after a loss | research + rule + data | yes: most multi-unit battles | L |
| (combat WP14) | Scan ranges (Q0037), now after WP4 | as planned there | yes | M |
| WP5 | Multi-strike timing: strikes spaced as the game spaces them, conditional second strikes | research + rule + data | yes: zealot, colossus, thor, phoenix, brood lord, carrier battles | M |
| WP6 | Bonus damage against any attribute | rule + data | yes: 18 (corruptor), 14 (thor air, none in matrix), baneling vs structure | M |
| WP7 | Shield-to-life spillover and the life-part minimum | research + rule | yes: battles with shielded targets | S–M |
| WP8 | Splash geometry: edge distance, per-weapon fractions, line, arc and bounce | research + rule + data | yes: 9a–10, 12a/b, 16a/b, 21, 26, 27, 34 | L (5 sub-packages) |
| WP9 | Catalog audit, non-time fields (D-10's field list for every identity), then data groups | research + data | yes | L |
| WP10 | Turning follow-ups: acquisition arc, the six mounts, turret idle return | research + rule + data | yes: 9a/b, 10, 2a–3, 12a/b, 21, 26 | M |
| WP11 | Random delays: weapon `RandomDelayMin/Max`, Siege Mode's random delay and the separate unsiege time | research + rule + data | yes: every battle (expected small) | M |
| WP12 | Acceleration and deceleration (air momentum) | rule + data | yes: air battles 12c, 16–19, 34, 35 | M |
| WP13 | Matrix capabilities: Immortal Barrier, Lurker attacks only burrowed, Charge, Concussive Shells, Phoenix fires while moving | separate rule + data per capability | yes: 2a–3, 21, 23b, 25b, 28, 30, (4) | L |
| WP14 | Projectile and vision rules: lifetime, what dodges a missile, revealing a hidden shooter | research + rule | expected none in the matrix | M |
| WP15 | Regeneration follow-ups: life of unfinished structures, energy regeneration delay, Stimpack and the delays, burrowed Roach | research + rule + data | none in the matrix (verify) | S–M |
| WP16 | Movement research plan: contact (Q0035), separation radius, push priority, M-19, M-21, backswing hold, leash, help and flee radii | research → own plan | — | plan only |
| WP17 | Economy and production plan: queue of 5, supply in production, rally, patch occupancy, faction construction, larva, warp-in, creep, power, cargo sizes, unload interval | → own plan | — | plan only |
| WP18 | Abilities plan: the 14 upgrades with no effect, the spell roster, modes, storm rules, Graviton Beam, Prismatic Alignment, broodlings, interceptors | → own plans | — | plan only |
| WP19 | Bookkeeping: `DESIGN.md` Combat row (combat WP13 step 5), doc 03/07/08, register, questions | docs | no | S |

---

## 2. Method

**Sources read for the inventory.**
- The combat alignment plan, Amendments 1 to 4 and §8 F-1 to F-8.
- Its evidence §12 to §14.
- The catalog time audit, every class E, **verify** and "not found" row, and §6, §7, §8, §10.
- The battle 33 investigation, §1.6, §1.8, §1.11 to §1.13, §2.3 to §2.6 and §7.1.
- Remediation plan §4.1 and §4.2. M-19 to M-22 are open; every S and M finding before them is fixed.
- The life-regeneration plan §8, F-1 to F-6.
- The turn-before-move plan (D-6, §8, R-4).
- Docs 03, 07 and 08; `DESIGN.md` §4.
- Open questions Q0007, Q0018, Q0034, Q0035, Q0037, Q0039, Q0041, Q0045 and Q0050. Q0018 and Q0034 concern
  measurement and CI layout, not game rules, so they sanction nothing here.
- The code: a read of `crates/sim_core/src` (target acquisition, combat, combat resolution, abilities, entity,
  economy, production, transport, vision, upkeep, upgrades, match state) and a grep of `sim_core`, `sim_map` and
  `sim_types` for "not modelled", "unlike", "deviat", "simplif", "approximat", "instead of" and "we do not". The
  grep found no gameplay hits: the code does not mark its own deviations. Two doc comments claim exact parity where
  the code differs: `combat.rs` ("exact shield spillover damage formulas") and `SplashSpec` ("StarCraft II
  standard").
- Liquipedia, read during planning (wikitext, 2026-11-05): [Automatic Targeting](https://liquipedia.net/starcraft2/Automatic_Targeting)
  and [Stimpack](https://liquipedia.net/starcraft2/Stimpack).

**Classes used in §3.**

| Class | Meaning | What happens |
|---|---|---|
| **S** | The owner sanctioned it in words, naming the deviation, or it is not a deviation | listed in §4; nothing changes |
| **P** | Being fixed by another package now | listed with a pointer |
| **B-rule** | Our code's rule differs from the game's, and the game's rule is sourced | fixed: rule commit (latent if possible), then data |
| **B-data** | A catalog value differs, and the reference is sourced | fixed: data commit, measured |
| **B-miss** | The game has a mechanic we do not model at all | fixed in a package or in its own plan; recorded bug until then |
| **B-?** | Ours differs from what StarCraft II probably does, but the game's rule is not sourced (§5) | research step first; then B-rule or B-data, or an owner question with a default |

**Gameplay impact.** Impact is judged by which matrix battles (`crates/sim_core/tests/fixtures/matchup_matrix.ron`,
50 battles) and which golden replays a fix can reach, and by whether skirmish play (`client`) sees it.
**Matrix-wide** means every battle with that kind of unit.

---

## 3. Inventory

IDs are stable. WP19 copies them into the bug register. "Where" names files under `crates/sim_core/src/` unless
another path is given.

### 3.1 Weapon timing

| ID | Deviation | StarCraft II (source) | Ours (where) | Class | Impact | Effort | WP |
|---|---|---|---|---|---|---|---|
| B-1 | A ready weapon that is not aligned swings one tick late, even when this tick's turn aligns it (M-22) | Turn and attack in one motion: the swing starts once the target is inside the arc, with no extra loop (remediation §4.2 M-22; `CWeapon` `Arc`/`ArcSlop`, audit §10.3) | `combat.rs` `step_ready_weapon` slews and enters `RotatingToTarget` without checking alignment again | B-rule | matrix-wide, 1 to 4 ticks per new target (evidence §14.4: 18 battles moved) | S | WP1 |
| B-2 | Every strike of a multi-strike weapon lands on the same tick | Zealot's second strike 0.2 Normal s later (5 ticks), and only if the target is within 2.1 (investigation §1.5, `liberty` EffectData:1919 chain). Phoenix: 2 missiles, one per side (`liberty` EffectData:1961, `PeriodCount 2`). Others are a per-weapon `PeriodicPeriod` in the effect chain (**verify** per weapon) | `combat_resolution.rs` `apply_weapon_hit` loops `attacks_count` in one tick | B-rule (zealot, Phoenix sourced); B-? (others) | 7, 8, 23a/b, 25a/b, 33 (zealot); 12a/b, 26, 35 (colossus); 14 (thor); 19, 20 (brood lord); combat F-4 | M | WP5 |
| B-3 | No random delay before firing | `RandomDelayMin −0.0625`, `RandomDelayMax 0.125` Normal s (`core` WeaponData:35–36), seeded per shot. Whether it lengthens the mean period is **verify** (investigation §1.4) | none; periods are whole ticks (combat F-3) | B-? | matrix-wide, expected under a tick a shot | M | WP11 |
| B-4 | Siege Mode: one fixed `cast_ticks` 71 | Siege 0.5 + 3.5417 Normal s plus `RandomDelayMax 0.5` (72 to 81 ticks); unsiege 3.5417 s with no 0.5 (63 ticks) (`liberty` AbilData:1308, :1323–1324, :1328; evidence §13.7) | `units.ron` `railgun_crawler` mode `cast_ticks: 71`; one field for both directions | B-rule + B-data | goldens and skirmish only; no matrix battle sieges | S–M | WP11 |
| B-5 | Damage point and backswing do not hold an automatically attacking unit in place | Not found (combat F-2). Damage point is "the time … when it can receive new commands" (investigation §1.8) | `locomotion.rs` holds the unit during wind-up only | B-? | matrix-wide if it differs | — | WP16 research |
| B-6 | Release slop 0.5 | `RangeSlop` 1 (investigation §2.3, `CWeapon` default; **verify** the line) | `combat.rs` `RELEASE_RANGE_SLOP = 0.5`, hard-coded for every weapon | B-data (and it should be per weapon) | matrix-wide, small | S | WP9 (field) |

### 3.2 Target acquisition

| ID | Deviation | StarCraft II (source) | Ours (where) | Class | Impact | Effort | WP |
|---|---|---|---|---|---|---|---|
| B-7 | The first criterion is "armed and holding an attack order" (20) over "armed" (19) | First criterion: a threat is a target with "a weapon that can return fire" at me, or the "Always a Threat to Attackers" flag (High Templar, Oracle, Disruptor, Widow Mine, Raven, unburrowed Lurker, Infestor, Swarm Host, Viper) (Liquipedia Automatic Targeting) | `combat.rs` `compute_threat_priority` | B-rule | matrix-wide wherever unarmed or cannot-hit units mix in (workers 22a, mixed air/ground 12c, 16a/b, 34) | M | WP4 |
| B-8 | Attack target priority is a hard-coded category: workers and unarmed units 10, detector structures 15, other structures 11 | A per-unit value (`AttackTargetPriority`): all units 20, workers included; Widow Mine and Interceptor 19; Larva and Cocoons 10; cannons, Planetary Fortress, filled Bunker and rooted Spine Crawler 20; Spore Crawler, Missile Turret and empty Bunker 19; other buildings 11 (Liquipedia Automatic Targeting; `UnitData` `AttackTargetPriority`, **verify** lines) | `combat.rs` `compute_threat_priority` | B-rule + B-data (new catalog field) | 22a (workers), skirmish: units ignore workers for armed units | M | WP4 |
| B-9 | The closest target is used for switching, with an invented margin of 1.0 | "Closest target" applies "only if the unit lost its previous target". A target is kept until it is no longer valid (dies, becomes invisible, goes up a ramp), leaves weapon range, or a higher-priority target enters (Liquipedia Automatic Targeting) | `target_acquisition.rs` `RETARGET_DISTANCE_MARGIN`, `decide_acquisition` | B-rule | matrix-wide focus pattern (investigation §5.3: the strongest single effect in battle 33 was through targeting) | M | WP4 |
| B-10 | No primary-weapon criterion | Third criterion: a unit with two weapons prefers targets for its primary weapon (Liquipedia) | none | B-rule | thor, viking, liberator, queen | S | WP4 |
| B-11 | No per-unit tie-break for step 4 | Hellbat, Thor, Battlecruiser and Ultralisk take the target that needs the least turning. Colossus, Immortal and an unsieged Siege Tank take the target closest to their previous target (Liquipedia) | none | B-rule + B-data (flag per archetype) | 2a–3, 12a/b, 14, 15, 26–28, 30, 35 | M | WP4 |
| B-12 | Scan range 8 to 12 on units, the sight radius (11) on armed structures, centre to centre | Per weapon the greater of `MinScanRange` and `Range` (Liquipedia; `WeaponData`; table in audit §11): 5 to 10.5 on units, 6 or 7 on structures, 13 in Siege Mode. Whether it is measured from the centres or the edges was not found; the owner chose edge to edge ([Q0057](../questions/closed/0057-scan-range-measuring-rule.md)) | `units.ron` `scan_range`; `catalog.rs` (structures); `target_acquisition.rs` | P | matrix-wide (26 or 15 battles move, combat evidence §15.3, §15.4) | — | Fixed by combat alignment WP14 Amendment 7 (`950b2bd`, `74d5491`, `8f77ddc`, `0a75c26`; Q0057, this plan's Amendment 7) |
| B-13 | Call-for-help radius 5, flee distance 5, pursuit leash 2 × scan range, abandoned target ignored for 2 s | Not found (§5). StarCraft II has unit alert and flee behaviour, but no radius or distance is sourced **(unsourced)** | `target_acquisition.rs` `CALL_FOR_HELP_RADIUS_SQ`, `HELPLESS_FLEE_DISTANCE`; `movement/tuning.rs` `PURSUIT_LEASH_SCAN_RANGES`, `ABANDONED_TARGET_TICKS` | B-? | skirmish; matrix where units are idle (13, held units) | — | WP16 research |

### 3.3 Damage model

| ID | Deviation | StarCraft II (source) | Ours (where) | Class | Impact | Effort | WP |
|---|---|---|---|---|---|---|---|
| B-14 | Bonus damage only against Armored and Light | Bonuses against any attribute, for example the Corruptor's +6 against Massive, the Baneling's +80 against Structure, the Archon's against Biological and the Thor's High Impact Payload against Massive (`EffectData` `AttributeBonus[...]`; **verify** each line in WP6) | `sim_types/src/weapon.rs` `bonus_armored`, `bonus_light`; `sky_leviathan` has `bonus_armored: 6` where the game has +6 Massive | B-rule + B-data | 18 (corruptor vs void ray, wrong bonus target), skirmish (baneling busts) | M | WP6 |
| B-15 | Shield break: the life part is `raw × (1 − shields ÷ mitigated)`, then life armour, then at least 0.5 | Shield armour first, then shields absorb, then the remainder minus life armour is "common knowledge but **not found** stated in a source" (investigation §1.8). Which remainder (raw-proportional or mitigated minus shields), and whether the 0.5 floor applies to the life part, is not sourced | `combat.rs` `mitigate_strike` | B-? | battles with shielded targets and shield upgrades (only once shield armour > 0: 23b, skirmish) | S–M | WP7 |
| B-16 | Splash fractions fixed at 100/50/25 % | Per-weapon fractions and radii (`EffectData` `CEffectEnumArea` `AreaArray`, **verify** per weapon) | `sim_types/src/weapon.rs` `SplashSpec` (three radii, fixed fractions) | B-rule + B-data | every splash battle | M | WP8a |
| B-17 | Splash distance is centre to centre | Area searches reach the unit's edge (`CEffectEnumArea` with the target radius **(unsourced)**, **verify**) | `combat_resolution.rs` `apply_splash` | B-? | every splash battle | S | WP8a |
| B-18 | Hellion, Lurker and Colossus splash is a circle around the target | A line along the attack (Hellion `InfernalFlameThrower`, Lurker spines, Colossus `ThermalLances` sweep) (`EffectData`, **verify** lines) | circular `SplashSpec` on `napalm_wheeler`, `burrow_scorpion`, `tripod_annihilator` | B-miss | 9a, 9b, 10, 12a, 12b, 21, 26, 35 | M | WP8b |
| B-19 | The Ultralisk's cleave is a circle | Cleave in an arc in front of the Ultralisk, at a fraction of the damage (`KaiserBlades` effect, **verify**) | `stag_behemoth` circular splash 0.5/1.5 | B-miss | 15, 27 | S–M | WP8c |
| B-20 | The Mutalisk's glaive does not bounce | 9, then 3, then 1 against up to two more targets (doc 07 row 8 states it; `GlaiveWurm` effect chain, **verify**) | `sonic_bat` single target | B-miss | 16a, 16b, 17, 34 | M | WP8d |
| B-21 | The Brood Lord's attack spawns no Broodlings | `BroodlingStrike` spawns Broodlings (`liberty` WeaponData:419; audit §8) | a 20 × 2 projectile | B-miss | 19, 20 | M | WP18 (own plan); WP8e records |
| B-22 | The Carrier's eight interceptors are one 8-strike weapon | Interceptors are units launched up to 8, `InterceptorBeam` period 3 Normal s (`liberty` WeaponData:632) | `mothership_carrier` weapon `attacks_count: 8` | B-miss | skirmish | L | WP18 |
| B-23 | Storm: hits structures; idle units flee it automatically (radius + 2) | Psionic Storm does not damage structures and units do not dodge it by themselves **(unsourced)** | `world/upkeep.rs` `step_hazards`; storm evacuation | B-? | 13 | S | WP18 (research), fixture 13 recorded |
| B-24 | Matrix battle 13's storm is radius 3, 20 every 10 ticks for 60 | Psionic Storm 80 over 2.85 Faster s, radius 1.5 **(unsourced, verify in `EffectData`/`AbilData`)** | `matchup_matrix.ron` battle 13 `hazard` | B-data (fixture) | 13 | S | WP18 |

### 3.4 Weapons a unit should not have, and mounts

| ID | Deviation | StarCraft II (source) | Ours (where) | Class | Impact | Effort | WP |
|---|---|---|---|---|---|---|---|
| B-25 | The Viking analogue shoots ground units in fighter mode | `LanzerTorpedoes` targets air only; ground attack needs Assault Mode (`liberty` WeaponData, `TargetFilters`, **verify** line) | `vector_interceptor` has `weapon` = its AA weapon; `catalog.rs` `weapon_against` falls back to `weapon` for ground targets | B-data | 12c and 35 (colossus counts as ground too), skirmish | S | WP2 |
| B-26 | The Corruptor analogue shoots ground units | `ParasiteSpore` air only (**verify** `TargetFilters`) | `sky_leviathan` `weapon` = AA weapon | B-data | skirmish | S | WP2 |
| B-27 | The Liberator analogue shoots ground units outside Defender Mode | `LiberatorAGWeapon` only in Defender Mode; `LiberatorMissileLaunchers` air only (`void` WeaponData:337 and the mode's morph, **verify**) | `sky_bastion` `ground_weapon` always active, `weapon` = AA weapon | B-data + B-miss (mode) | skirmish | S (+M for the mode) | WP2, WP18 |
| B-28 | Six mounts differ: Hellion, Immortal, Colossus, Lurker and Phoenix turrets are modelled as hull weapons; the Thor's air weapon as a turret | audit §10.5 (`liberty` UnitData:11603, :10791, :4099, :797; `swarm` UnitData:1805; `void` WeaponData:82) | `units.ron` `has_turret` and rates | B-data | 9a/b, 10, 2a–3, 28, 30, 12a/b, 26, 35, 21, 14 | S each, measured | WP10 |
| B-29 | One firing tolerance; no acquisition arc `Arc` ÷ 2 | `Arc` limits acquisition, `ArcSlop` widens it once engaged (audit §10.3, SC2Mapster editor reference) | `catalog.rs` `firing_half_angle` (one value) | B-rule | matrix-wide (small) | M | WP10 |
| B-30 | A turret keeps its last angle when idle | `CTurret` `Idle` behaviour, for example `ResetIfMoving` (`swarm` TurretData:32; audit §10.4) | `combat.rs` turret slew | B-rule + B-data | first shot after idling; tank in goldens | S | WP10 |
| B-31 | `StationaryTurningRate` not modelled | A second hull rate for a unit standing still; it differs for Siege Tank, Thor, Ultralisk and Hellion (audit §10.2) | one `turn_rate` | Fixed (Amendment 2) | 14, 15, 27; 9a/b, 10 | — | turn-before-move WP5 `2522e3f`, WP6 `408d3b7` |

### 3.5 Vitals, energy and Stimpack

| ID | Deviation | StarCraft II (source) | Ours (where) | Class | Impact | Effort | WP |
|---|---|---|---|---|---|---|---|
| B-32 | `Entity::apply_stimpack` hard-codes 10 life and 250 ticks; `issue_order` routes an immediate `UseStim` there, around the catalog and its unlock | Stimpack comes from the unit's ability data: Marine 10 life, 15 Normal s = 268 ticks (`liberty` AbilData:1229, BehaviorData:680); Marauder 20 life (`StimpackMarauder`, BehaviorData:604) | `entity.rs` `issue_order`, `apply_stimpack` (callers: the public `Entity::issue_order` path; the world path `world/commands.rs` `apply_stim` reads the catalog) | B-rule | 25b if it ever goes through `Entity::issue_order` (**verify**); scripts and tests | S | WP3 |
| B-33 | Stim is admitted at any life above 1 and leaves at least 1 | The cast needs more life than its cost (owner's answer to [Q0053](../questions/closed/0053-stimpack-at-low-life.md); the sources do not say) | `entity.rs` `prepare_overclock` (`hp <= 1`, `.max(ONE)`) | Fixed (Amendment 2) | 25b, skirmish | — | `ad0dcde` |
| B-34 | Units with energy start at maximum | `EnergyStart`, 50 for most casters, the Phoenix 50 (`liberty` UnitData:3727); others per unit (**verify** each, for example Queen and Ghost) | `entity.rs` spawn: `energy: archetype.max_energy` | B-data (new field) | no matrix battle spends energy; skirmish casters | S | WP3 |
| B-35 | No energy regeneration delay | `EnergyRegenDelay` exists; per-unit values not checked (life-regeneration plan F-3) | none | B-? | none expected | S | WP15 |
| B-36 | Life of unfinished structures regenerates | Not researched (life-regeneration F-4) | `world/upkeep.rs` | B-? | skirmish (Zerg) | S | WP15 |
| B-37 | Whether Stimpack's life cost restarts the regeneration delays | Not researched (life-regeneration F-2) | `entity.rs` `commit_overclock` does not touch `ticks_since_last_damage` | B-? | 22a/b do not stim; none | S | WP15 |
| B-38 | No burrowed Roach regeneration and no Tunneling Claws | Burrowed Roach regenerates faster (audit R2 note); Tunneling Claws allows moving while burrowed (doc 08) | no burrow ability on `rhino_beetle`; `tunneling_claws` has `effects: []` | B-miss | skirmish | M | WP15 (regeneration), WP18 (burrow) |

### 3.6 Motion

| ID | Deviation | StarCraft II (source) | Ours (where) | Class | Impact | Effort | WP |
|---|---|---|---|---|---|---|---|
| B-39 | Acceleration and deceleration are parsed but unused: speed changes are instant; the catalog copies `max_speed` into both | Ground units mostly 1000 (instant); air units, Hellion and workers have finite values, for example Mutalisk 3.5, Phoenix 3.25, Medivac 2.25 Normal (audit §4 class E rows, each with its line) | `catalog.rs` loads them; no system reads them (audit §2.3) | B-miss + B-data | air battles 12c, 16a/b, 17, 18, 19, 34, 35; 9a/b, 10 | M | WP12 |
| B-40 | Turn before move | `TurnBeforeMove` flag (audit §10.2) | — | Fixed (Amendment 2) | 14, 15, 27 | — | turn-before-move WP1 `6d7a9d5`, WP3 `b7d9f50` |

### 3.7 Unit capabilities that matrix battles use

| ID | Deviation | StarCraft II (source) | Ours (where) | Class | Impact | Effort | WP |
|---|---|---|---|---|---|---|---|
| B-41 | Immortal Barrier not modelled (M-20) | On damage, autocast absorb of up to 100 for 3 Normal s, cooldown 45 (`void` BehaviorData:97–101, `voidmulti` BehaviorData:278, AbilData:585–591) | `void_goliath` has no capability; battle 3 re-recorded to 176 with the gap named | B-miss | 2a, 2b, 3, 28, 30 | M | WP13a |
| B-42 | The Lurker attacks without burrowing, and moves while it attacks | The Lurker attacks only burrowed; burrowing takes time (`LurkerMP` weapon's requirement, **verify**) | `burrow_scorpion` armed while unburrowed; no burrow ability | B-miss | 21 | M | WP13b |
| B-43 | Charge is a speed bonus only | Charge also dashes: ×2.2 speed for up to 3.5 Normal s when an attack target is between 0.6 and 4 away, cooldown 10 Normal s, autocast (investigation §1.6, `liberty` AbilData:3825, BehaviorData:967, ValidatorData:183, :1697) | `upgrades.ron` `charge`: `move_speed_delta: 0.063` | B-miss | 23b, skirmish | M | WP13c |
| B-44 | Concussive Shells research does nothing; the slow is hard-coded and never applied | 50 % slow on hit, duration **verify** in `BehaviorData` (the code's 75 ticks is unsourced) | `upgrades.ron` `concussive_shells` `effects: []`; `entity.rs` `apply_concussive_slow` (75 ticks, called only by a test) | B-miss | 4 and 31 only with the research (none researched today); skirmish | M | WP13d |
| B-45 | The Phoenix cannot fire while moving | `AllowedMovement Moving` (`swarm` WeaponData:181); the Void Ray too (audit §10.6) | weapon state machine needs a stationary hull weapon | B-miss | 17, 18 (Void Ray); skirmish | M | WP13e |
| B-46 | The Viking cannot land (Assault Mode) | mode switch (**verify** `AbilData` `AssaultMode`) | no mode | B-miss | skirmish | M | WP18 |

### 3.8 Catalog values outside the time audit

| ID | Deviation | StarCraft II (source) | Ours (where) | Class | Impact | Effort | WP |
|---|---|---|---|---|---|---|---|
| B-47 | Non-time values were never audited except for the marine, zealot and Phoenix: damage, bonuses, strikes, ranges, life, shields, armour, attributes, costs, supply, sight, radii, cargo size, upgrade amounts | `UnitData`, `WeaponData`, `EffectData`, `UpgradeData` at `5444e12f13` | `units.ron`, `structures.ron`, `upgrades.ron` | B-data (unknown count) | potentially matrix-wide | L | WP9 |
| B-48 | Push priority 10 to 30, against the game's | `core` UnitData:59 default 10; per-unit overrides (investigation §1.13; 5.0.14 Ultralisk "push priority 0 → 1", doc 07 §1.1) | `units.ron` `push_priority` (doc 07's choice, not the owner's) | B-data | friendly crowding in every battle | S data, measured | WP16 (with contact) |
| B-49 | `separation_radius` is read by nothing; values differ from `InnerRadius` | `InnerRadius` 0.375 for marine and zealot (investigation §1.13) | `units.ron` `separation_radius` | B-? (which radius governs spacing is not found) | contact battles | — | WP16 |
| B-50 | Melee attackers pack around a solid enemy as `d4109e2` decided (K ≈ 4.25 in battle 33) | Not found (Q0035, combat F-1) | `steering.rs`, `movement/collision.rs` | B-? (deferred by the owner "for now"; not a sanction) | every melee battle; 33 | — | WP16 |
| B-51 | Neosteel Armor gives +2 armour to the Bunker only | Neosteel Armor's targets per `UpgradeData` at 5.0.14 **(unsourced; verify)** | `upgrades.ron` `neosteel_armor` | B-? | skirmish | S | WP9 |
| B-52 | Projectile speeds with several motion phases, and the High Templar and Spine Crawler speeds | Multi-phase movers (audit §8; class E); the HT and Spine Crawler movers are not in the data | `projectile_speed` fixed | B-? | 12c, 19, 35 (Viking), skirmish | M | WP14 |

### 3.9 Projectiles and vision

| ID | Deviation | StarCraft II (source) | Ours (where) | Class | Impact | Effort | WP |
|---|---|---|---|---|---|---|---|
| B-53 | Attack projectiles fuel out after 125 ticks | No generic timeout for attack missiles **(unsourced)** | `sim_types/src/weapon.rs` `max_projectile_lifetime_ticks` default 125 | B-? | none expected | S | WP14 |
| B-54 | A projectile fizzles when its target cloaks without detection | A target that dies or is loaded is not hit; whether cloaking or burrowing dodges a missile in flight is **(unsourced)** | `combat_resolution.rs` `ProjectileTargetLossBehavior::Fizzle` | B-? | skirmish | S | WP14 |
| B-55 | A shooter hidden by high ground or fog is never revealed to its victim | **(unsourced)**: doc 03 §2 says "attacking a high-ground unit reveals only a small splash around the attacker (or requires vision)", which is ambiguous | `world/vision.rs` | B-? | high-ground tests, skirmish | M | WP14 |
| B-56 | Detection uses the sight radius | `Detect` radius per unit, usually 11 **(unsourced; verify `UnitData`/`BehaviorData`)** | `world/vision.rs` (`is_detector` with sight) | B-? | skirmish | S | WP14 |
| B-57 | The edge cell of a line-of-sight blocker is seen from outside | Sight blockers hide their area from outside **(unsourced)** | `sim_map` vision | B-? | skirmish maps | S | WP14 |

### 3.10 Economy, production, construction and transport

All are B-miss or B-?, and no matrix battle uses them. Skirmish play sees them all. Each is planned in WP17's own
plan.

| ID | Deviation | Ours (where) | StarCraft II |
|---|---|---|---|
| B-58 | No production queue limit | `production/mod.rs` `queue_production_prevalidated` | 5 slots **(unsourced; verify)** |
| B-59 | Every queued job holds supply | production admission | supply is taken when production starts **(unsourced)** |
| B-60 | No rally points | none | rally points |
| B-61 | No mineral-patch occupancy: income grows linearly with workers | `economy/harvest.rs` ("a node has no worker limit") | one worker mines a patch at a time; one in an extractor |
| B-62 | One Terran-style construction model for every faction | `production/construction.rs` | Probe warps in and leaves; Drone is consumed; Protoss needs power, Zerg needs creep |
| B-63 | No larva, warp-in, creep or power | none | faction mechanics |
| B-64 | Cargo size equals supply | `transport/orders.rs` | a separate `Cargo` size (Zergling 1, Baneling 2, Thor 8) **(verify)** |
| B-65 | Unload sets every passenger down at once and stops the carrier | `transport/orders.rs` `set_passengers_down` | one at a time, also while moving **(unsourced)** |
| B-66 | A blocked exit stalls production | `production/lifecycle.rs` | **(unsourced)** |
| B-67 | Next-patch radius 12, task reach 0.75, site patience 50 ticks, boarding reach 1.0 | `economy/`, `production/`, `transport/` | **(unsourced)** |

### 3.11 Abilities and upgrades with no effect

| ID | Deviation | Where | Class | WP |
|---|---|---|---|---|
| B-68 | 14 researchable upgrades change nothing: Concussive Shells, Drilling Claws, Smart Servos, Personal Cloaking, Cloaking Field, Warp Gate Technology, Phase Blink, Shadow Stride, Psionic Storm, Tunneling Claws, Nanomuscular Swell, Adaptive Talons, Microbial Shroud, Neural Parasite | `upgrades.ron` `effects: []` | B-miss | WP13d (Concussive); WP18 (the rest) |
| B-69 | Only Siege Mode is a shipped ability. Cloak, burrow, blink and hazard mechanisms exist but no unit carries them. Missing: Medivac heal, repair, Queen inject and transfuse, the casters' spells (Feedback, Storm, Snipe, EMP, Fungal Growth, Abduct, Blinding Cloud, Guardian Shield, Force Field), Viking and Liberator modes, Warp Prism phasing, permanent cloak (Dark Templar, Observer), Graviton Beam, Prismatic Alignment, KD8 Charge | `units.ron` `abilities` (only `railgun_crawler`) | B-miss | WP18 |
| B-70 | The Phoenix's Graviton Beam (combat F-5) | `flying_saucer` | B-miss | WP18 |

### 3.12 Documents that state a wrong StarCraft II fact or a deviation as fact

| ID | Statement | Where | Fix | WP |
|---|---|---|---|---|
| B-71 | "Random Delay (`-0.06s` to `+0.06s`)" | doc 03 §1.3 item 4 | −0.0625 to +0.125 Normal s (`core` WeaponData:35–36) | WP0 |
| B-72 | Energy "starts at `50` or `75`" while the engine starts at maximum | doc 03 §1.1 | correct the engine column after WP3 | WP3 |
| B-73 | Doc 07 claims mechanics we do not have: Mutalisk "Bounce: 3, 1", Ultralisk "Cleave AoE", Brood Lord "Spawns Broodlings", Carrier "Interceptor Swarm", Colossus "Sweep", Lurker "Wave", Corruptor "Caustic Spray vs Domes" | doc 07 roster | each cell names the bug ID until its WP lands | WP0 |
| B-74 | `combat.rs` module doc "exact shield spillover damage formulas"; `SplashSpec` "(StarCraft II standard)" | code comments | restated in WP7 and WP8a | WP7, WP8a |
| B-75 | doc 03 §1.4 "Turn Rate & Stationary Turn Rate" implies both are modelled | doc 03 | none: true since B-31 was fixed (Amendment 2) | — |

### 3.13 Hard-coded gameplay values with no source

These are bugs twice over: a charter §3.3 problem (identity-free, but not data) and a deviation until sourced.

| ID | Value | Where | WP |
|---|---|---|---|
| B-76 | Threat categories 20/19/15/11/10 | `combat.rs` `compute_threat_priority` | WP4 (B-8) |
| B-77 | Concussive slow 75 ticks at 50 % | `entity.rs` `apply_concussive_slow` | WP13d |
| B-78 | Storm evacuation margin +2 | `world/upkeep.rs` | WP18 |

---

## 4. Sanctioned, or not a deviation

These are **not** bugs. WP0 lists them in Q0052 so the owner can confirm the list in one answer. The default is
"treat as listed".

| ID | Item | Why it is not a bug |
|---|---|---|
| S-1 | doc 03 §3: configurable supply cap (default 200), unlimited selection, spectator zoom, RON data instead of SC2's editor data | The owner's words are quoted there ("unless some limitations are unnecessary"), and the table names each change. These are UI or data-format choices, not simulation rules. **Q0052 asks** whether this counts as "specifically telling to deviate". |
| S-2 | Splash hits cloaked units without detection (Q0007 item 5) | Matches StarCraft II: area damage hits undetected units (doc 03 §1.6; `target_acquisition.rs` `is_damageable`). It was a deviation from finding S-17's wording, not from the game. |
| S-3 | Turn before move, with a tolerance of one tick's turn (Q0050 (b)) | The owner chose the tolerance because the data has none. |
| S-4 | A move to an unreachable point walks to the nearest reachable one (Q0006) | The owner's choice, and the game does the same. |
| S-5 | 25 ticks per Faster second, whole-tick periods with round-half-up conversion | Doc 07 §2.1. Tick quantisation is inherent to a fixed-step engine. The game's 22.4 loops per Faster second also quantise. Sub-tick cadence (combat F-3) is B-3's research. |
| S-6 | Original names, lore and looks for every identity (Q0045: "we will create our own units later") | Presentation only. Every identity with a counterpart follows its mechanics (combat F-7). |

Explicitly **not** sanctioned, although an owner answer touches them:
- Q0035's "no change … for now": a deferral, not a deviation (B-50).
- Q0036 option 2: it chose the zealot's values but did not say "deviate" on the second-strike timing (option 3). So
  B-2 stands.
- Doc 07's push priorities: a document's choice, not the owner's (B-48).

---

## 5. StarCraft II behaviour this plan could not determine from a source

Each item starts a research step in its WP. The step reads the 5.0.14 chain, then Liquipedia, then SC2Mapster. If
the rule is still not stated precisely enough to specify and test, the WP files an owner question. The question
proposes a measurement in the StarCraft II editor or a unit tester, and its default is "no change; bug stays
recorded". Nothing is guessed.

| # | Question | Bug | WP |
|---|---|---|---|
| U-1 | Does the random firing delay lengthen the mean period, and how is it drawn (per shot, uniform)? | B-3 | WP11 |
| U-2 | Strike spacing for every multi-strike weapon other than the zealot and the Phoenix (Thor, Colossus, Brood Lord, Viking, Carrier) | B-2 | WP5 |
| U-3 | Shield break: which remainder reaches life, and does the 0.5 floor apply to it? | B-15 | WP7 |
| U-4 | Does an area search measure to the target's edge? | B-17 | WP8a |
| U-5 | Does a backswing or damage point hold an automatically attacking unit in place? (combat F-2) | B-5 | WP16 |
| U-6 | Can enemies push each other, and which radius governs unit spacing? (Q0035, combat F-1) | B-49, B-50 | WP16 |
| U-7 | Ally alert radius, flee distance and leash, if any | B-13 | WP16 |
| U-8 | Can a unit stim at life equal to or below the cost? (answered: no, [Q0053](../questions/closed/0053-stimpack-at-low-life.md)) | B-33 | WP3 |
| U-9 | Does a life-cost ability restart regeneration delays? (life-regeneration F-2) | B-37 | WP15 |
| U-10 | Do unfinished structures regenerate life? (life-regeneration F-4) | B-36 | WP15 |
| U-11 | Projectile lifetime; does cloaking or burrowing dodge a missile in flight? | B-53, B-54 | WP14 |
| U-12 | Is a shooter on high ground or in fog revealed to its victim, and for how long? | B-55 | WP14 |
| U-13 | Detection radius per detector | B-56 | WP14 |
| U-14 | Does a sight blocker hide its edge cell? | B-57 | WP14 |
| U-15 | Movers with several phases (Viking, Liberator, Banshee, Missile Turret, Phoenix); the High Templar and Spine Crawler movers | B-52 | WP14 |
| U-16 | Does the target's radius count toward weapon range? (investigation §1.11) | — (ours counts it) | WP9 research |
| U-17 | Are shots fired in one game loop at a target killed earlier in that loop wasted? | — (ours wastes them) | WP14 |
| U-18 | Does Psionic Storm hit structures, and do units dodge it? | B-23 | WP18 |
| U-19 | Queue size, supply timing, blocked exits and unload interval | B-58, B-59, B-65, B-66 | WP17 |
| U-20 | Neosteel Armor's targets at 5.0.14 | B-51 | WP9 |
| U-21 | `EnergyRegenDelay` per unit | B-35 | WP15 |
| U-22 | The Siege Mode transformation: are the section delay and duration additive? (audit §8) | B-4 | WP11 |
| U-23 | The steering engine's own tuning (`movement/tuning.rs`: lookahead, flock weights, stall times, push relaxation): the game's mover is not documented | — (recorded only; replace a constant when U-6 finds the game's rule) | WP16 |

---

## 6. Design decisions

### D-1. Order by gameplay impact, cheapest certain fix first

1. Rules that act in every battle and whose fix is sourced come first: WP1 (M-22), then WP4 (targeting).
2. WP2 and WP3 slot early. They are small and sourced, and they remove wrong behaviour that skirmish players see at
   once (Vikings shooting marines; stim around its catalog).
3. Combat alignment WP14 (scan ranges) runs **after WP4**. Scan range only chooses candidates, and WP4 changes how
   a candidate is chosen. Measuring scan ranges on the old rule would explain the wrong mechanism, the risk WP14
   names itself (combat WP14 step 1).
4. Then the damage model, WP5 to WP8. Each changes how much an attack does, not whether it happens, and is
   measured.
5. **WP9 (non-time audit) comes after WP6 and WP8.** Those packages add the fields (attribute bonuses, splash
   fractions and shapes) that some audited values need. Otherwise the audit would defer many values on the
   "mechanism we do not model" ground.
6. WP10 to WP15 follow, in order of matrix reach.
7. WP16 to WP18 are research and planning packages for whole subsystems. They produce plans, not code, because each
   would need its own blueprint under charter §6.
8. **Against the requested order.** The orchestrator asked for this plan after turn-before-move and before combat
   WP13. Combat WP13's steps 1 to 4 are documentation (closing Q0035, Q0007, Q0037, Q0045) and need nothing from
   this plan, so they may run first. Holding them back would leave answered questions open. Its step 5 (the
   end-of-lane `DESIGN.md` Combat row) is absorbed into WP19 (§1 item 3).

### D-2. Mechanism before data; one cause per commit

As in combat alignment D-1:
- a rule commit is latent on shipped data where possible and proved bit-identical (matrix, battle 33, both goldens);
- data follows in its own commit, measured;
- a rule that cannot be latent (as M-22 cannot) is its own commit, measured, with the cause named;
- tests that would break for a reason other than the commit's cause are restated first, test-only, green before
  and after;
- mechanism tests use scratch archetypes in RON strings (C5).

### D-3. Measurement

Combat alignment D-5, unchanged, after every rule or data commit that can reach a unit a matrix battle spawns:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name>
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --all-battles --seeds 16 --out ~/scratch/sc2_dev/<step>/matrix --compare ~/scratch/sc2_dev/<prev>/matrix/matrix.csv
cargo run --release --locked -p sim_core --example matchup_trace -- \
    --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/sc2_dev/<step>/b33
```

**Record** in a new evidence file `docs/evidence/<date WP1 starts>-sc2-deviation-fixes.md`, one section per
commit:
- the matrix delta (battles moved, survivors, end tick, lead hull, expectation failures on the seeds of record and
  on all 20);
- battle 33 on 68 seeds;
- the first golden checkpoint that differs;
- for multi-field data commits, each field alone (D-5 item 5).

Run long measurements in the background and read their logs.

### D-4. Expectations and goldens

- **Battle 33:** combat alignment D-6 (Q0044 option 1). Re-record to the smallest ranges that hold on the seeds
  of record, and rewrite the comment's distribution in every commit that moves it.
- **Any other battle:** a battle whose winner over its seeds changes is a **stop**: report it before
  re-recording (combat WP17 step 8 as amended). A single-seed flip is listed in the evidence.
- **No widening without a measured reason.** No expectation is widened without the measured reason (combat D-1
  item 6).
- **Goldens:** `golden_replay.ron` and `golden_replay_orders.ron` are re-recorded only in the commit whose cause
  moves them. The fixture header names the cause and the first differing checkpoint. Debug, release and
  `wasm32-wasip1` must reproduce. A change to the checksum schema bumps `CHECKSUM_SCHEMA_VERSION` in its own
  latent commit (new hashed fields: attack target priority, start energy, per-strike timers).
- **A hashed field with no behaviour change** (as in combat WP17 b and e) is proved by excluding it from the
  encoding in a throwaway run.

### D-5. Research gate

Every **B-?** and **B-miss** item starts with a research step. A documentation commit adds a section to
`docs/evidence/2026-11-04-catalog-time-audit.md`, or to the new evidence file for rules. Each value is cited as
`layer` File:line at `5444e12f13`, with Liquipedia wikitext as cross-check. The research decides one of:
1. **specifiable:** the rule commit follows;
2. **not found within half a day:** file an owner question (next free number). The default is "no change; the bug
   stays recorded under its ID". Go on with the next package.

Values are never invented, and a deferral never turns a bug into an accepted difference.

### D-6. Data-driven, per archetype

Every new rule reads catalog fields: an attribute-bonus map, splash fractions and shape, attack target priority,
threat flag, strike spacing, start energy, random delays, acceleration and turret idle behaviour. No identity may
appear in production Rust (C5), and `#[serde(default)]` is allowed only where the default is the game's engine
default and its doc comment says so. Otherwise the field is required, so a stale catalog fails to load, as in combat
D-8 item 1.

### D-7. Gate

Charter §4.3:
- `cargo gate quick` is optional while working;
- **`cargo gate package` once per package**, after merging `main`, before its final commit, with the stamp in
  the hand-back (for simulation changes it runs the golden replays in release and on `wasm32-wasip1`);
- the orchestrator runs `cargo gate merge <branch>`;
- documentation-only packages run the docs tier.

Work in a worktree with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name>`, keep outputs
under `~/scratch/sc2_dev/`, and remove the target after merge (charter §7). Every commit names its WP and bug IDs.
Failing test first, with the red run in the hand-back. No `unwrap`; `expect` only with `invariant:`. Functions stay
under 100 lines (C8). Every new file begins with `//!` (C4).

---

## 7. Work packages

### WP0: The rule, the register, the sanctioned list (documentation)

1. **Q0051, closed at once.** File `docs/questions/closed/0051-sc2-mechanics-standing-rule.md`: the owner's
   sentence verbatim, dated, with "Effect: supersedes Q0008; every unsanctioned difference from StarCraft II
   5.0.14 is a bug".
2. **Charter §8.** Replace the Q0008 bullet with the standing rule (Q0051): "Use StarCraft II's mechanics. A
   deviation exists only where the owner has said so and named it; any other difference is a bug, recorded in the
   bug register until fixed." §8 is an owner-governed procedure section, and this edit carries the owner's own
   words. The orchestrator confirms the edit before merge.
3. **Bug register.** A new section in `DESIGN.md` (§4, after the table), "Known deviations from StarCraft II
   (bugs)". It has one line per open ID of §3 and links here. It does not repeat the table, and it is kept current
   by every WP. Set the Combat and Movement rows' "Known gaps" cells to point at it.
4. **Q0052** (open): the sanctioned list S-1 to S-6 of §4, with "confirm or name what to change", default "treat as
   listed".
5. **Doc fixes.** B-71 (doc 03 random delay); B-73 (doc 07 cells name their bug IDs); B-75 (doc 03 pointer).
6. **Combat WP13 hand-off wording.** For when it closes Q0035 and Q0045: the remaining items are "recorded bugs
   B-50 / B-28, B-29, B-31 in plan `2026-11-05-sc2-deviation-fixes`", not "accepted".

**Gate.** Docs tier. **Size.** S.

### WP1: M-22, a swing that its turn aligns starts on the same tick (B-1)

- **Test first** in `crates/sim_core/tests/hull_turning_tests.rs`, scratch archetypes:
  - `a_turn_that_aligns_the_weapon_starts_the_swing_on_that_tick`: rate 55.99, arc 11.25, target 40° off → the
    swing starts on tick 1, not 2;
  - `a_turn_that_does_not_align_waits` (guard);
  - the same for a turreted weapon.
- **Rule.** `combat.rs` `step_ready_weapon` checks alignment after its slew, exactly as `step_rotating_weapon`
  does. Share one helper (C8).
- **Not latent.** Measure (D-3); re-record per D-4. Expected: the melee and zergling battles of evidence §14.4 get
  back most of the 1 to 4 ticks.
- **Remediation plan.** M-22 row: "Fixed in `<sha>`".

**Size.** S (half a day).

### WP2: Air-only weapons do not fire at the ground (B-25, B-26, B-27 part)

1. **Research (in the commit message and evidence).** `TargetFilters` of `LanzerTorpedoes`, `ParasiteSpore`,
   `LiberatorMissileLaunchers` and `LiberatorAGWeapon`, and the Liberator's `AGWeapon` mode requirement. Cite the
   lines.
2. **Test first.** A shipped-data test finds every archetype whose `weapon` slot duplicates its `air_weapon`
   (same name) and asserts `weapon_against(Ground)` is `None` for those the data mark air-only. A
   `runtime_catalog_tests` test can do this by property, without naming identities.
3. **Data.** `weapon: None` for `vector_interceptor` and `sky_leviathan`. For `sky_bastion`, `weapon: None` and
   the `ground_weapon` stays only as Defender Mode's. Until WP18 adds the mode, the ground weapon is removed: the
   game's Liberator in fighter mode has no ground attack. B-27's mode part stays recorded.
4. **Measure** (12c, 35: the colossus is a cliff walker that `weapon_against` may resolve to the ground slot,
   **verify**), and re-record. Check `golden_replay*.ron` for these identities first.

**Size.** S.

### WP3: Stimpack from the catalog, its life rule, and start energy (B-32, B-33, B-34, B-72)

1. **Commit a (rule, latent).**
   - Delete `Entity::apply_stimpack`.
   - `Entity::issue_order` no longer special-cases `UseStim`. An immediate `UseStim` reaching an entity outside
     `SimWorld` is rejected with a typed error, because only the world knows the catalog and the unlock.
   - Restate tests that call it (`checked_movement_tests.rs` and others: grep) on `apply_overclock` with a scratch
     spec, first.
   - Prove bit-identical.
2. **Research U-8.** If sourced, **commit b (rule):** `prepare_overclock` refuses when `hp <= hp_cost` and never
   floors at 1. Latent if no fixture stims a unit at that life (**verify** in 25b's trace). If not sourced, file the
   question and keep today's rule.
3. **Commit c (latent).** `start_energy: f32` in `UnitArchetypeDef`, required, validated `0 ≤ start ≤ max`. Data =
   `max_energy` everywhere (bit-identical).
4. **Commit d (data).** Each caster's `EnergyStart` from `UnitData` (Phoenix 50, `liberty` UnitData:3727; the
   other ten cited). Measure: `golden_replay_orders.ron` hashes the spotter's energy, so it is re-recorded.
5. **Docs.** Doc 03 §1.1 engine column (B-72).

**Size.** S–M.

### WP4: Target selection as StarCraft II's (B-7 to B-11, B-76)

1. **Research.** For every identity:
   - `AttackTargetPriority` (`UnitData`);
   - the "Always a Threat to Attackers" flag (its `UnitData` name, **verify**);
   - which weapon is primary;
   - the step-4 tie-break class (least turning: Thor, Ultralisk, and the Hellbat if modelled; closest to the
     previous target: Colossus, Immortal, unsieged Siege Tank).

   Cite each, and cross-check with Liquipedia Automatic Targeting (quoted in §3.2). The "keep the target" rule
   comes from the same page. Write a table into the evidence file.
2. **Commit a (latent fields).** New archetype fields:
   - `attack_target_priority: u8`, required;
   - `always_a_threat: bool`;
   - `acquire_tie_break: AcquireTieBreak { Closest, LeastTurning, ClosestToPrevious }`, required;
   - which weapon slot is primary.

   Data set to reproduce today's categories exactly, so the commit is bit-identical. Checksum schema bump only if
   hashed (**verify**; the fields are archetype data, not entity state, so expected not).
3. **Commit b (rule).** The ranking becomes:
   - (1) threat: the candidate has a weapon that can hit me, or is flagged always-a-threat;
   - (2) attack target priority;
   - (3) primary weapon;
   - (4) the archetype's tie-break, applied only when the unit has no valid current target.

   Keep the current target while it is valid and in range unless a higher (1)–(2) candidate is in range. Delete
   `RETARGET_DISTANCE_MARGIN`. The id tie-break stays as the last deterministic key; it is not a game rule, but it
   only decides exact ties. Mechanism tests on scratch archetypes, one per criterion. Measure; this is the big
   one, so stop and report any battle whose winner changes.
4. **Commit c (data).** The reference priorities, workers 20 among them. Measure.

**Size.** L (3 to 4 days). **Then combat WP14** (scan ranges) runs as planned there.

### WP5: Multi-strike timing (B-2)

1. **Research U-2.** For each weapon with `attacks_count > 1`: the effect chain's per-strike delay and any
   condition (the zealot's 2.1). Cite the lines.
2. **Commit a (latent).** `strike_interval_ticks: u16` and an optional `followup_max_distance` per weapon. The data
   are 0 and none, so every strike lands on the swing's release tick as today: bit-identical. Strikes after the first
   are pending entity state. That state is hashed, so the checksum schema is bumped in this commit and the goldens
   are re-recorded with outcomes proved unchanged.
3. **Commit b, per weapon family (data).** Zealot 5 ticks within 2.1; Phoenix as cited; others as found. Measure.

**Size.** M.

### WP6: Bonus damage against any attribute (B-14)

1. **Commit a (latent).** `WeaponSpec` `bonus_armored`/`bonus_light` become `attribute_bonuses: Vec<(Attribute,
   SimScalar)>`, at most one entry per attribute, validated. `bonus_upgrade_per_level` applies per entry, as the game
   does (**verify** in `UpgradeData`). Rewrite the data to the same values, bit-identical. This is a schema change in
   `sim_types`, `catalog.rs` and the checksum encoding: bump and prove.
2. **Commit b (data).** Each weapon's bonuses from `EffectData` `AttributeBonus`: Corruptor Massive, Baneling
   Structure, Thor air Massive, and the others found. Measure (battle 18).

**Size.** M.

### WP7: Shield spillover (B-15, B-74)

- **Research U-3.**
- If sourced: a tests-first rule commit in `combat.rs` `mitigate_strike`, measured, and the module doc corrected.
- If not: file a question proposing an editor measurement (a unit with shield armour 1 and 1 shield hit by a known
  attack). The default is no change. The doc comment's "exact" claim is removed now either way (B-74).

**Size.** S–M.

### WP8: Splash geometry (B-16 to B-20), five sub-packages, each a rule then data

- **WP8a:** per-weapon fractions and edge distance (U-4).
- **WP8b:** line splash (Hellion, Lurker, Colossus), its length and width from `EffectData`.
- **WP8c:** frontal arc cleave (Ultralisk).
- **WP8d:** bounce (Mutalisk): count, decay, search radius and its re-targeting rule.
- **WP8e:** records B-21 for WP18.

Each shape is a variant of a `SplashShape` enum in the weapon's data, never an identity. Each is a latent rule
(shape unused), then one data commit per weapon, measured.

**Size.** L.

### WP9: Non-time catalog audit (B-47, B-6, B-51, U-16, U-20)

1. **Research.** Extend the audit with every non-time field of combat D-10's list, for every identity with an
   analogue: units, structures, upgrades. That covers cost, supply, life, shields, armour, attributes, sight,
   weapon damage, bonuses, strikes, ranges, target filters, splash values (after WP8), push priority (listed for
   WP16), cargo size (listed for WP17), `RangeSlop` and upgrade amounts and targets. Classes A to E as in combat D-7.
   Change groups, one field kind and one cause each.
2. **Data**, one commit per group, measured, in an order that puts groups touching no matrix identity first (as
   combat WP12). A value is deferred only on combat D-7's grounds, and each deferral becomes a bug ID in the register.

**Size.** L (research 2 days; groups 2 to 4 days).

### WP10: Turning follow-ups (B-28, B-29, B-30)

1. **Acquisition arc (rule, latent with `acquire_arc = engaged arc`).** New field `acquire_half_angle`; data from
   `Arc` ÷ 2 (audit §10.3). Measure.
2. **Six mounts (data).** One commit per identity: `has_turret`, turret rate (audit §10.5) and arc on the right
   part. Measure each.
3. **Turret idle return (rule + data).** The `Idle` behaviour and `YawIdleRate` from `TurretData`. Latent first.

**Size.** M.

### WP11: Random delays (B-3, B-4, U-1, U-22)

1. **Research.**
2. **Weapon random delay (rule).** Draws from `SimWorld`'s seeded `ChaCha8Rng` in a fixed order (entity id), so
   determinism holds. Data per weapon from `RandomDelayMin/Max`. Measure on 20 seeds. The jitter seeds already vary
   the matrix; this adds variance per shot.
3. **Siege Mode.** Separate `cast_ticks` for entering and leaving, plus the random extra delay (the M2 deferral of
   evidence §13.7). Rule latent, then data.

**Size.** M.

### WP12: Acceleration and deceleration (B-39)

- **Rule.** Locomotion changes speed by at most `acceleration` per tick toward the desired speed, and by
  `deceleration` when braking. Data: copies of `max_speed` (today's catalog), and "instant" for values ≥ the
  speed, so it is bit-identical first.
- **Data.** The audit's class E reference column (each row already cited), converted: per-tick² = Normal × 1.4² ÷
  25², **verify** the conversion in the research step.
- **Measure** the air battles. Turn-before-move's gate interacts with it, so this runs after that package.

**Size.** M.

### WP13: Capabilities used by matrix battles (B-41 to B-45, B-77)

One sub-package each, ordered by battles reached. Each has a latent capability field and rule, then data:
- **13a** Barrier (2a, 2b, 3, 28, 30);
- **13b** Lurker burrow-to-attack (21);
- **13c** Charge (23b);
- **13d** Concussive Shells: an on-hit behaviour in weapon data, unlocked by the research; the hard-coded
  `apply_concussive_slow` goes;
- **13e** firing while moving (`AllowedMovement`).

Each is cited from the data. Each sub-package may be split into its own plan if its research shows a larger rule
problem (charter §6).

**Size.** L.

### WP14: Projectiles and vision (B-52 to B-57, U-11 to U-15, U-17)

Research first, then one rule commit each for what is sourced. Expected not to touch the matrix (verify by
measurement).

**Size.** M.

### WP15: Regeneration follow-ups (B-35 to B-38, U-9, U-10, U-21)

Research, then rule and data where sourced. The burrowed Roach's regeneration needs burrow (WP18), so it is only
researched and recorded here.

**Size.** S–M.

### WP16, WP17, WP18: Research and planning packages

Each writes a blueprint under `docs/plans/` with its own owner questions, and no code.
- **WP16, movement:** B-5, B-13, B-48 to B-50, M-19, M-21, U-5 to U-7, U-23. Start with an editor or unit-tester
  measurement proposal, because the sources run out (Q0035).
- **WP17, economy and production:** B-58 to B-67.
- **WP18, abilities:**
  - B-21 to B-24, B-27 (mode), B-38 (burrow), B-46, B-68 to B-70 and B-78;
  - ordered by skirmish impact: storm and its research, Medivac heal, burrow, cloak (Dark Templar, Banshee,
    Observer), blink, the modes, then the rest.

**Size.** One planning session each.

### WP19: Bookkeeping

- `DESIGN.md` Combat and Movement rows (absorbing combat WP13 step 5) and the register.
- Doc 03, doc 07 and doc 08 cells for every fixed ID.
- Remediation M-20 and M-22.
- Combat alignment §8 F-items pointing to these IDs.
- Close Q0052 when answered.
- This plan's README row.

**Gate.** Docs tier.

---

## 8. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| WP4 moves most battles at once | a hard-to-review re-recording | latent fields first; one criterion per mechanism test; the stop rule for winner changes; each criterion measured alone on scratch data before the rule commit |
| Research cannot find a rule (§5 has 23) | packages stall | D-5: half-day bound, then an owner question with a "no change, still a bug" default; move to the next package |
| Schema changes (WP3, WP5, WP6, WP10, WP12) break the client or old catalogs | load errors | required fields fail loudly by design (D-6); client tests in `cargo gate package` |
| Checksum churn across many packages | goldens re-recorded often | one cause per re-recording, latent proofs with hashed fields excluded (D-4) |
| Random delays (WP11) make matrix outcomes noisier | expectation churn | they are seeded and deterministic; the expectations already use 20 jittered seeds |
| The plan is too large to finish soon | bugs stay open | the register makes each open bug visible; the order puts matrix-wide fixes first |
| An agent "fixes" an item from memory (**unsourced** rows) | a new deviation | §5 and D-5 forbid applying any unsourced value |
| `with-gpu-lock` or disk pressure during measurement | stalls | charter §7; measurements under `~/scratch` |

---

## 9. Out of scope

Nothing here is an accepted difference. These are not scheduled by WP0 to WP15, are recorded in the register,
and are planned by WP16 to WP18: the movement engine (B-5, B-13, B-48 to B-50, M-19, M-21), economy and production
(B-58 to B-67), and the ability roster (B-21 to B-24, B-46, B-68 to B-70, B-78). Presentation items (S-1, S-6) are
not simulation mechanics.

## Amendment 1 (orchestrator, 2026-11-05): the owner's refinement; WP0 items 1, 2 and 4 done

- The owner refined the rule the same morning (quoted in
  [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md)): not a StarCraft II clone; identical unit
  mechanics and very close behaviour first; deviations that allow more (preferably configurable) and minor detail
  differences are fine; StarCraft II's engine bugs (units pushed through walls, as one example) need not be
  reproduced.
- **Effect on §3:** WP0 re-triages every B-ID into (i) *unit mechanics and values* (stays a bug, fixed by its WP),
  (ii) *detail or "allows more"* (recorded as an accepted detail difference with its reason, not fixed unless it
  changes how a unit plays), or (iii) *a StarCraft II engine quirk* (not reproduced). Doubtful cases stay in (i).
  Every package keeps the order of §7; a package whose items all fall into (ii) or (iii) shrinks to a records commit.
- **Done by the orchestrator** (docs branch `veo-budget-only`): WP0 item 1 (Q0051, closed), item 2 (charter §8, with
  the refined wording), item 4 (Q0052, confirmed by the owner in chat and closed). WP0 keeps items 3, 5 and 6 and the
  re-triage above.

## Amendment 2 (WP0, 2026-11-05): re-triage under the refined rule

Amendment 1 asked WP0 to sort every B-ID of §3 into three classes under the owner's refined rule
([Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md)): "identical unit mechanics and very close
behavior" first, so that experienced players feel at home and missing functionality shows.

| Class | Meaning | What happens |
|---|---|---|
| (i) | Unit mechanics, values or behaviour, including mechanics the game has and we lack | Stays a bug, fixed by its WP, listed in the `DESIGN.md` bug register while open |
| (ii) | A detail, or a difference that allows more than StarCraft II | Accepted detail difference, recorded here with its reason; not fixed. If it is found to change how a unit plays, it returns to (i) |
| (iii) | A StarCraft II engine quirk | Not reproduced |
| Fixed | No longer a deviation | Commit named |

**Rule applied.** A difference a player can see in how a unit fights, moves, is produced or is countered is (i). An
item is (ii) only if it allows more than the game, or no shipped unit can reach it, or it is a trajectory or a
tolerance a player cannot act on. Where neither reading was clearly right, the item stayed in (i), as Amendment 1
requires.

### Result

| ID | Short title | Class | Reason or WP |
|---|---|---|---|
| B-1 | A swing its turn aligns starts a tick late (M-22) | Fixed | WP1 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §1) |
| B-2 | Every strike of a multi-strike weapon on one tick | Fixed, narrowed | WP5 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §5); open only whether an order between the strikes ends a `Channeled` burst ([Q0058](../questions/open/0058-channeled-burst-interruption.md)) |
| B-3 | No random firing delay | (i) | WP11; it decides how volleys desynchronize, which players see |
| B-4 | Siege Mode: one transformation time, no random part | (i) | WP11 |
| B-5 | Whether damage point and backswing hold an automatically attacking unit | (i) | WP16 research |
| B-6 | Release range slop 0.5 for every weapon | (i) | WP9 |
| B-7 | Threat criterion | Fixed | WP4 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §4.3, §4.4) |
| B-8 | Attack target priority as hard-coded categories | Fixed | WP4 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §4.5) |
| B-9 | Closest-target switching with an invented margin | Fixed | WP4 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §4.6; Amendment 5) |
| B-10 | No primary-weapon criterion | Fixed | WP4 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §4.7) |
| B-11 | No per-unit tie-break | Fixed | WP4 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §4.9, §4.10) |
| B-12 | Scan ranges | Fixed | combat alignment WP14 Amendment 7 ([evidence](../evidence/2026-11-04-combat-sc2-alignment.md) §16; Q0057; this plan's Amendment 7) |
| B-13 | Call-for-help radius, flee distance, leash, abandoned-target time | (i) | WP16 research; how idle units answer an attack is unit behaviour |
| B-14 | Bonus damage only against Armored and Light | Fixed, narrowed | WP6 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §6); open only the Reaper's Light 5, which 5.0.14 sets to 0: stopped, battle 22a's winner changes (Amendment 9) |
| B-15 | Shield break remainder | (i) | WP7 |
| B-16 | Fixed splash fractions | (i) | WP8a |
| B-17 | Splash measured centre to centre | (i) | WP8a |
| B-18 | Line splash modelled as a circle | (i) | WP8b |
| B-19 | Ultralisk cleave modelled as a circle | (i) | WP8c |
| B-20 | No glaive bounce | (i) | WP8d |
| B-21 | No Broodlings | (i) | WP18 |
| B-22 | Interceptors as one 8-strike weapon | (i) | WP18 |
| B-23 | Storm hits structures; idle units dodge it on their own | (i) | WP18; an automatic dodge changes how storm is played |
| B-24 | Battle 13's storm values | (i) | WP18 |
| B-25 | Viking analogue shoots the ground in fighter mode | Fixed | WP2 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §2) |
| B-26 | Corruptor analogue shoots the ground | Fixed | WP2 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §2) |
| B-27 | Liberator analogue shoots the ground outside Defender Mode | (i), narrowed | The ground attack outside Defender Mode is fixed by WP2 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §2); Defender Mode itself, and with it the ground attack, stays open for WP18 |
| B-28 | Six weapon mounts | (i) | WP10 |
| B-29 | No acquisition arc | (i) | WP10 |
| B-30 | A turret keeps its angle when idle | (i) | WP10; it sets the delay of the first shot after idling |
| B-31 | `StationaryTurningRate` | Fixed | turn-before-move WP5 `2522e3f`, WP6 `408d3b7` |
| B-32 | Hard-coded Stimpack path | Fixed | WP3 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §3) |
| B-33 | Stim admitted at any life above 1 | Fixed | `ad0dcde`: the owner answered [Q0053](../questions/closed/0053-stimpack-at-low-life.md) with rule (a), so the stim is refused, changing nothing, unless life is strictly greater than its cost; matrix and both golden replays bit-identical |
| B-34 | Energy starts at maximum | Fixed | WP3 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §3) |
| B-35 | No energy regeneration delay | (i) | WP15 |
| B-36 | Unfinished structures regenerate life | (i) | WP15 |
| B-37 | Whether Stimpack's life cost restarts the regeneration delays | (ii) | No shipped unit can reach it: the two archetypes with an Overclock (Stimpack) spec have `hp_regen_per_tick: 0.00`, and no other ability costs life. It returns to (i) if a catalog change gives one unit both. WP15 drops it and U-9 |
| B-38 | No burrowed Roach regeneration, no Tunneling Claws | (i) | WP15, WP18 |
| B-39 | No acceleration or deceleration | (i) | WP12 |
| B-40 | Turn before move | Fixed | turn-before-move WP1 `6d7a9d5`, WP3 `b7d9f50` |
| B-41 | No Immortal Barrier | (i) | WP13a |
| B-42 | The Lurker attacks unburrowed | (i) | WP13b |
| B-43 | Charge without the dash | (i) | WP13c |
| B-44 | Concussive Shells do nothing | (i) | WP13d |
| B-45 | No firing while moving | (i) | WP13e |
| B-46 | The Viking cannot land | (i) | WP18 |
| B-47 | Non-time catalog values unaudited | (i) | WP9 |
| B-48 | Push priorities | (i) | WP16 |
| B-49 | Which radius spaces units | (i) | WP16; how tightly armies clump is visible behaviour |
| B-50 | Melee packing around a solid enemy | (i) | WP16 |
| B-51 | Neosteel Armor's targets | (i) | WP9 |
| B-52 | Projectile speeds with several phases; High Templar and Spine Crawler speeds | (i), narrowed | WP14, for each attack's flight time from release to impact, which delays its damage. The shape of the phases inside that time (a missile that drifts, then accelerates) is (ii): attack missiles home, so a player cannot act on the path |
| B-53 | Attack projectiles fuel out after 125 ticks | (ii) | A safety bound no shipped attack reaches: the slowest projectile moves 0.56 per tick and the fastest unit 0.238 per tick, so a missile released within the longest weapon range (13) arrives in under 50 ticks. It returns to (i) if a catalog change makes it reachable. WP14 drops it and U-11's lifetime half |
| B-54 | A projectile fizzles when its target cloaks | (i) | WP14 |
| B-55 | A hidden shooter is never revealed to its victim | (i) | WP14 |
| B-56 | Detection uses the sight radius | (i) | WP14 |
| B-57 | The edge cell of a sight blocker is seen from outside | (i) | WP14. Kept: a unit at the edge of a blocker can be seen, which changes where units can hide |
| B-58 | No production queue limit | (ii) | Allows more than the game's 5 slots, which the owner allows. WP17's plan may offer the limit as a configurable option; it is not a bug |
| B-59 | Every queued job holds supply | (i) | WP17; it may allow less than the game |
| B-60 | No rally points | (i) | WP17 |
| B-61 | No mineral-patch occupancy | (i) | WP17 |
| B-62 | One construction model for every faction | (i) | WP17 |
| B-63 | No larva, warp-in, creep or power | (i) | WP17 |
| B-64 | Cargo size equals supply | (i) | WP17 |
| B-65 | Unload sets everyone down at once and stops the carrier | (i) | WP17 |
| B-66 | A blocked exit stalls production | (i) | WP17 |
| B-67 | Next-patch radius 12, task reach 0.75, site patience 50 ticks, boarding reach 1.0 | (i), narrowed | WP17 for the next-patch radius, which decides where workers go when a patch mines out. Task reach, site patience and boarding reach are (ii): tolerances of our own movers that decide when an order counts as arrived, with no observable game counterpart (as U-23) |
| B-68 | 14 upgrades with no effect | (i) | WP13d, WP18 |
| B-69 | Missing abilities | (i) | WP18 |
| B-70 | No Graviton Beam | (i) | WP18 |
| B-71 | Doc 03 random delay range | Fixed | WP0, in this amendment's commit |
| B-72 | Doc 03 start energy | Fixed | WP3 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §3) |
| B-73 | Doc 07 claims mechanics we lack | Fixed | WP0: each cell names its bug ID; the mechanics stay open as B-14, B-18 to B-22, B-26, B-42 and B-69 |
| B-74 | Code comments claim exact parity | (i) | WP7, WP8a |
| B-75 | Doc 03 "Turn Rate & Stationary Turn Rate" | Fixed | True since B-31 was fixed; no edit needed |
| B-76 | Hard-coded threat categories | Fixed | WP4 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §4.3, §4.5) |
| B-77 | Hard-coded Concussive slow | (i) | WP13d |
| B-78 | Storm evacuation margin | (i) | WP18 |
| B-82 | The Baneling's damage to structures is not its own effect (80, +5 a level, ignoring armour) | (i) | WP9. Found in WP6 (Amendment 9) |
| B-81 | A first strike the effect chain delays (Liberator 1 tick, Colossus 7) lands on the release tick | (i) | WP5 follow-up; the Colossus part with B-18 (WP8b). Found in WP5 (Amendment 8) |

**Counts.** 78 IDs: (i) 70, two of them narrowed (B-52, B-67) and one carried by another plan (B-12); (ii) 3 (B-37,
B-53, B-58); (iii) none; fixed 5 (B-31 and B-40 by the turn-before-move plan; B-71, B-73 and B-75 by WP0).

**No (iii) item.** No B-ID asks us to reproduce a StarCraft II engine quirk: each is either a game rule we lack or a
rule of our own. The class stays for later findings. For example, units pushed through walls are not ours to
reproduce, and WP16's movement plan must not copy such behaviour from the game.

### Effect on the work packages

No package lost all its items, so none shrinks to a records commit and none is dropped. The order of §7 stands, and
WP12's wait for the turn-before-move package is satisfied.

| WP | Change |
|---|---|
| WP0 | Item 5 drops B-75 (already true). Done with this amendment |
| WP14 | Shrinks: B-53 and U-11's lifetime question leave; B-52 and U-15 ask only for flight times, not the phase shape. Left: B-52 (narrowed), B-54 to B-57, U-11 (dodging), U-12 to U-15, U-17 |
| WP15 | Shrinks: B-37 and U-9 leave. Left: B-35, B-36, B-38, U-10, U-21 |
| WP17 | Shrinks: B-58 leaves the bug list (the plan may offer a configurable limit); U-19 keeps supply timing, blocked exits and the unload interval; B-67 keeps the next-patch radius only |
| WP19 | The register lists open (i) items only; (ii) items stay recorded here |
| Others | Unchanged |

**Order:** WP0 (done), WP1, WP2, WP3, WP4, combat alignment WP14, WP5, WP6, WP7, WP8a to WP8e, WP9, WP10, WP11, WP12,
WP13a to WP13e, WP14, WP15, WP16, WP17, WP18, WP19.

**§9 amended.** "Nothing here is an accepted difference" now holds for (i) items. The (ii) items above, and S-1 to
S-6 of §4 (confirmed in [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md)), are accepted differences.

**WP0 items 3, 5 and 6** are done on the same branch: the bug register in `DESIGN.md` §4, the doc fixes B-71 and B-73
in docs 03 and 07, and the hand-off line in plan `2026-11-04-combat-sc2-alignment` WP13.

## Amendment 3 (WP1 to WP3, 2026-11-05): what the implementation changed

Evidence: [`2026-11-05-sc2-deviation-fixes.md`](../evidence/2026-11-05-sc2-deviation-fixes.md) §1 to §3.

- **Fixed:** B-1 (WP1), B-25 and B-26 (WP2), B-32 and B-34 (WP3), B-72 (WP3, doc 03). B-27 is narrowed to its mode
  part: the Liberator analogue no longer attacks the ground at all until WP18 adds Defender Mode.
- **New, found and fixed: B-79.** A unit armed only against air froze its weapon cooldown and backswing while it had
  no target: the weapon step resolved its weapon against the ground plane, found none and skipped the unit. In
  StarCraft II the cooldown runs whatever the unit does. WP2's data change exposed it (battle 35, one seed), so a rule
  commit came first and the data commit then moved nothing the matrix measures (evidence §2.2).
- **B-33 not sourced (U-8):** [Q0053](../questions/closed/0053-stimpack-at-low-life.md), default no change; B-33 stays
  in the register. *Later the same day:* the owner answered Q0053 (rule (a), the stim needs more life than it
  costs); fixed in `ad0dcde`, matrix and goldens bit-identical, B-33 removed from the register.
- **WP2's test** finds the three archetypes through their `sc2_analogue` and checks them against the cited
  `TargetFilters`. The catalog has no field that marks a weapon air-only, so a test "by property" (§7 WP2 item 2)
  could only restate the data it checks.
- **WP3 c and d** cover structures as well as units: commit c's required `start_energy` is on both definitions, and
  commit d sets the reference value for the three structures with energy (Orbital Command, Nexus, Shield Battery).
- **Matrix effect:** only WP1 moved results (19 battles, no battle's winner over its seeds); battle 33's expectation
  held and only its comment's distribution changed.

## Amendment 4 (WP4, 2026-11-05): target selection, partly landed

Evidence: [`2026-11-05-sc2-deviation-fixes.md`](../evidence/2026-11-05-sc2-deviation-fixes.md) §4.

- **Fixed:** B-7 (threat: a weapon that can hit me, `AlwaysThreatens`, and the workers' `Defensive` acquire level),
  B-8 and B-76 (per-archetype `attack_target_priority` from `AttackTargetPriority`), B-10 (the primary weapon).
- **Stopped: B-9.** The retention rule moved battle 33's winner over its seeds (68 seeds: 37 / 20 / 11 → 20 / 35 /
  13). Per the stop rule it is not committed; the patch and the numbers are in evidence §4.6. B-11 waits for it,
  because its tie-break acts only after the previous target is lost. Its research is done (§4.1): the data put
  `ByAngle` on the Thor and Ultralisk weapons and `ByDistanceFromTarget` only on the Colossus; Liquipedia's Immortal
  and unsieged Siege Tank are not in the 5.0.14 data, so they keep the closest target.
- **Rank of a challenger.** WP4 step 3 said a kept target gives way to a higher (1)–(2) candidate. The source says
  "according to the first 3 criteria", so `TargetPriorityKey::rank` includes the primary weapon (B-10).
- **New field beyond step 2:** `acquire_level` (`DefaultAcquireLevel`), because the source's threat criterion
  excludes defensive units unless they attack-move.
- **New, found: B-80** (class (i)): a defensive unit (a worker) acquires targets automatically while idle; in
  `StarCraft` II it acquires only while attack-moving (Liquipedia "Automatic Targeting", Notes). Open; it belongs
  with B-9 and B-11 in the rest of WP4.
- **Matrix effect:** only B-7's rule moved results (battle 35, no winner change).

## Amendment 5 (orchestrator, 2026-11-05): B-9 is applied, battle 33 re-recorded

> **Orchestrator decision on the B-9 stop:** "apply B-9. Reason: it is StarCraft II's keep-target rule with a cited
> source, the owner's standing rule (Q0051) asks for identical unit mechanics, and the owner's Q0007 answer asked why
> the internet says zealots should beat the marines in battle 33 — B-9 moves battle 33 toward that. Plan D-4 already
> says battle 33 is re-recorded, not stopped; my brief's 'any battle's winner flipping stops' was stricter than the
> plan, and this reported stop is exactly what it was for."

- B-9 is committed with its tests first. Battle 33's expectation is re-recorded to the smallest ranges that hold on
  all 20 matrix seeds, its comment states the 68-seed distribution, and `golden_replay.ron` is re-recorded with the
  cause traced ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §4.6).
- B-11 (with its checksum schema bump in a latent commit of its own) and B-80 follow, each measured. Battle 33 may
  move again; any other battle whose winner over its seeds changes stops the step.
- **Done after Amendment 5:** B-9 ([evidence](../evidence/2026-11-05-sc2-deviation-fixes.md) §4.6), B-11 with
  checksum schema 11 (§4.9, §4.10) and B-80 (§4.11) are fixed. B-80 is limited to automatic acquisition; how an idle
  worker answers an attack (retaliation, the alert of idle allies) stays with B-13 (WP16).

## Amendment 6 (combat alignment WP14, 2026-11-05): scan ranges researched and measured, B-12 stays open

Evidence: [combat alignment evidence](../evidence/2026-11-04-combat-sc2-alignment.md) §15; reference table in the
[audit](../evidence/2026-11-04-catalog-time-audit.md) §11.

- **Done:** WP14 steps 1 to 3 and the report (step 4): the battle 33 mechanism, the reference table for every armed
  identity, and the whole matrix on a scratch catalog with every reference value. The trace tool gained targeting
  measures (`sim_fixtures::trace::focus`, commit `b1d28e6`).
- **Not applied: B-12 stays open.** The game's value per weapon is sourced (the greater of `MinScanRange` and
  `Range`), but how the game measures it, from the centres or from the edges, is not found, and it decides what the
  values do: copied under our centre rule, a ranged unit scans less than its weapon reaches and acquires only what
  it can already shoot. Filed as [Q0057](../questions/closed/0057-scan-range-measuring-rule.md), default no change
  (the brief's rule for an unsourced measuring rule).
- **B-12 widened:** armed structures scan to their sight radius (`catalog.rs`), not a catalog field; the reference
  is their weapon range (audit §11.3). The Siege Mode scan (13) is not expressible as one value per unit.
- **Matrix effect if applied (scratch, 20 seeds):** 26 battles move under our centre rule, 15 under an edge rule; no
  battle's winner over its seeds changes under either; battle 33 is §15.2's table.

## Amendment 7 (combat alignment WP14 Amendment 7, 2026-11-06): B-12 fixed

The owner answered [Q0057](../questions/closed/0057-scan-range-measuring-rule.md) with option 2: scan range measured
edge to edge, the audited StarCraft II values across the catalog, the moved battles re-recorded. Carried out by plan
[`2026-11-04-combat-sc2-alignment`](2026-11-04-combat-sc2-alignment.md) WP14 Amendment 7
([evidence](../evidence/2026-11-04-combat-sc2-alignment.md) §16):

- `950b2bd`: the edge rule (centre distance minus both collision radii), values unchanged.
- `74d5491`: a weapon scans at least to its own effective range, which covers Siege Mode (13) and upgraded ranges
  without a per-mode value.
- `8f77ddc`: the 35 armed units' `scan_range` to the audit's §11.2 reference; battle 33 re-recorded to `blue: (0,
  10), red: (0, 5)`; both goldens re-recorded.
- `0a75c26`: armed structures read `scan_range` from `structures.ron` (the widened part of Amendment 6).

No battle's winner over its seeds changed in any step. B-12 is fixed and leaves the `DESIGN.md` register; the
pursuit leash, which is a multiple of the scan range, stays with B-13.

## Amendment 8 (WP5, 2026-11-06): multi-strike timing landed

Evidence: [`2026-11-05-sc2-deviation-fixes.md`](../evidence/2026-11-05-sc2-deviation-fixes.md) §5.

- **Fixed: B-2**, narrowed. `strike_interval_ticks` (required) and `followup_max_distance` per weapon; the strikes
  after the first wait in `Entity::pending_strikes` (checksum schema 12, in the latent commit a). The data follow
  each weapon's `CEffectCreatePersistent` and validators (§5.1): Zealot 5 ticks within 2.1, Thor ground 7, Viking 4
  within 15, Reaper 2 within 7, Banshee 3 within 15, Liberator 1, Missile Turret 2 within 15, Queen 5 within 5; the
  Phoenix's and the Colossus' strikes land together. Open: whether an order between the strikes ends a `Channeled`
  burst, filed as [Q0058](../questions/open/0058-channeled-burst-interruption.md) (default no change).
- **Plan WP5 item 2 as built:** a later strike is also dropped when its target has left the weapon's plane (the
  data's `MultipleHitGroundOnlyAttackTargetFilter`), a projectile weapon launches one missile per strike, and a
  strike with splash splashes. The plan's "0.2 Normal s" for the Zealot is 0.28 Normal s (0.2 Faster s), 5 ticks
  as the plan says.
- **New, found: B-81** (class (i)). The effect chain delays the first strike of two weapons, which land on the
  release tick here: the Liberator's first missile 0.05 Normal s (1 tick), and both of the Colossus' strikes on its
  target 0.375 Normal s (7 ticks, the sweep's `ExpireDelay`). The Colossus part belongs with B-18 (WP8b).
- **Battle 33 re-recorded** (D-4): on top of the scan ranges (Amendment 7), the Zealot's spaced blade moves it from
  21 / 37 / 10 to 54 / 7 / 7 on 68 seeds (marines / zealots / unresolved); expectation `blue: (0, 13), red: (0, 3)`. The mechanism is the delay alone (the
  2.1 condition decides no blade): every second blade lands 5 ticks later, so each marine kill takes 5 ticks
  longer. No other battle's winner over its seeds changed (battles 7, 8, 14, 19, 23b, 25a, 25b, 35 moved; 35's
  blue wins went 16 → 12 of 20 with the Viking's spaced torpedoes, the rest unresolved).
- **Register:** B-2 narrowed to Q0058's part; B-81 added.

## Amendment 9 (WP6, 2026-11-06): attribute bonuses landed, the Reaper's value stopped

Evidence: [`2026-11-05-sc2-deviation-fixes.md`](../evidence/2026-11-05-sc2-deviation-fixes.md) §6.

- **Fixed: B-14**, narrowed. `attribute_bonuses` (one value per attribute, required) replaces `bonus_armored` and
  `bonus_light`; each applying bonus gains `bonus_upgrade_per_level` (checksum schema 13, in the latent commit a).
  Data from each damage effect's `AttributeBonus` (§6.1): the Corruptor's 6 and the Thor high-impact payload's 10
  are against Massive; the Spore Crawler gains Biological 10. Every other armed analogue's bonuses already matched.
- **Stopped: the Reaper.** 5.0.14 sets its Light bonus to 0 (`void` EffectData:1499). Applied, battle
  `22a_reaper_vs_2_workers` (claim "The bonus vs light kills workers") goes from a Reaper win on all 20 seeds to 20
  unresolved at its 200-tick limit (§6.3). Per the stop rule it is not committed; the catalog keeps Light 5 and
  B-14 keeps it as its open part until the orchestrator decides (apply and re-record 22a, or not).
- **The plan's Baneling example is not an attribute bonus.** Its +80 against structures is a separate effect,
  `VolatileBurstU2` (80, +5 a level, ignoring armour): recorded as **B-82** (class (i), WP9).
- **Rebased.** WP5 and WP6 were first built on `d1876fa` and rebased onto `9df767e` (the scan ranges, Amendment 7);
  every number in evidence §5 and §6 is measured on the rebased commits.
