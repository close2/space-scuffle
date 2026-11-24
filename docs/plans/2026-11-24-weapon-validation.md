# Blueprint: Weapon Validation — Missiles That Drop a Teleporting Target, and Targets Lost to Fog (B-182, B-185)

This plan settles two rows that the vision-and-detection plan registered and deferred (its §14 F-1 and F-3), against
StarCraft II 5.0.14:

- **B-182: a missile drops a target that teleports.** Every attack missile keeps the `CEffectLaunchMissile` default
  `ValidateTeleport 1` (`core` EffectData:100). When its target is relocated by a teleport effect (Blink is
  `CEffectTeleport`, `liberty` EffectData:2347–2356), the missile stops homing and, by `PointFallback 1` (:97, B-54),
  flies on to the last point it tracked and lands there. A single-target missile strikes nobody: Blink dodges
  missiles in flight. Ours keeps homing on the blinked unit.
- **B-185: a target outside its attacker's vision is lost.** Every weapon's `TargetFilters` requires `Visible`
  (`core` WeaponData:29, and every one of the 146 `TargetFilters` rows of the eight layers). A unit therefore neither
  starts nor lands a weapon strike on an enemy its player does not see, and, by `ValidateWeapon 1` (EffectData:101),
  a missile whose target walks into fog lands on the point. Ours requires vision only to acquire a new target, so a
  unit keeps tracking and firing at a non-concealed target that left its owner's vision: a sieged Siege Tank
  analogue keeps shelling a target at range 13 that its sight of 11 does not reach, and a unit below a cliff keeps
  firing at an air attacker after its 3-second attack reveal (B-55) has ended.

The data settle a third row that this plan registers and fixes with them:

- **B-194: the Brood Lord's missiles do not validate the weapon in flight.** Its three missiles clear
  `ValidateWeapon` (`liberty` EffectData:2668, :2749, :2780), and its damage effects validate the target at impact
  instead (`BroodlingEscortFilters`, `Ground,Visible;Missile,Stasis,Dead,Hidden,Invulnerable`, `liberty`
  ValidatorData:1723–1726, linked at EffectData:2679 and :2785). A Brood Lord missile keeps homing on a target that
  walks into fog or cloaks, and strikes it only if the target passes the filters when it lands. Once B-185 lands
  without this, our Brood Lord analogue's missiles would drop such targets instead.

All three become rules over catalog data: `WeaponSpec` gains `missile_validation` (the two flags), each entity a
teleport counter, each projectile the counter it saw at launch, and one weapon-filter predicate joins the existing
availability gate to vision. No code names a unit (charter §3.3, C5).

**What this plan does not do: the chase.** What an attack order does once its target is lost to fog (walk to the
last point where the target was seen, as this plan reads the game, or stop at once) is decided by pursuit under
`crates/sim_core/src/movement/`, which the contact-solver branch owns until it merges. This plan splits it:
- **In scope now:** the weapon half of B-185 (no strike, no tracking, missiles land on the point). Acquisition, which
  lives in `target_acquisition.rs`, drops an *automatic* target lost to fog and re-acquires, so no world depends on
  pursuit for it.
- **Registered as B-195:** a *player's* attack order still chases its target through fog by the target's true
  position (and holds fire while it is unseen). WP7 settles it **after the contact solver merges**, on Q0106's
  answer.

A full plan is required (charter §6): new rules in the simulation, a change of hashed encoding, matrix worlds that
move by behaviour, and work across `sim_types`, `sim_core`, the catalog and the fixtures.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-24 against `main` at
`cff46b37` (checksum schema 41).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations (S-5 25 ticks per Faster second, whole-tick rounding) | Closed | D-1 (no new duration) |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S12 (Liquipedia "Automatic Targeting", already the source of acquisition's invariant 3) |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.5, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0101: vision and detection details (an attack reveal shows the attacker to the struck player only) | Open; defaults built | D-4 ("seen" includes the reveal, as built) |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6, §7 |
| **Draft, not filed:** "When a target leaves vision, what counts as a teleport, and what part of a unit must be seen" | Proposed for the orchestrator to file as Q0106 (§15); defaults followed meanwhile | D-4, D-5, D-7, WP7 |

---

## 1. Summary

| | Today (`cff46b37`) | After this plan |
|---|---|---|
| Missile at a target that Blinks (or any teleport) | Keeps homing on the target's new position and strikes it | Drops it: flies on to the last point it tracked before the teleport and lands (B-54's point fallback); a splash weapon strikes its area there. `missile_validation.teleport: false` keeps today's homing as data |
| Weapon strike (swing start, pending strike, impact) on a non-concealed enemy whose cell is not `Visible` to the attacker's player | Starts and lands | Neither starts nor lands, unless the enemy is attack-revealed to that player (B-55). Concealment unchanged |
| Missile whose target walks into fog | Keeps homing and strikes | Lands on the last point it tracked (`ValidateWeapon`); `missile_validation.weapon: false` keeps homing and validates at impact |
| Brood Lord analogue's missiles | Validate concealment in flight; would also validate vision after B-185 | Validate nothing but existence in flight, the full filter at impact (B-194) |
| Automatic (acquired) attack whose target leaves vision | Kept; pursuit chases the true position, and the unit fires | The best visible candidate replaces it, or the acquired attack is dropped and the unit's own order resumes (D-5, Q0106 part 2) |
| Hold or tracking target that leaves vision | Kept while in reach | Released (as for a concealed one today) |
| Player's attack order whose target leaves vision | Kept; chases; fires | Kept and chases (B-195, WP7 after the contact solver); holds fire while the target is unseen. An immobile unit's order ends, as for a concealed target today |
| Hashed state | — | `Entity::teleport_count`, `ActiveProjectile::target_teleport_count`, `WeaponSpec::missile_validation`; schema **next free at merge** (43 if the contact solver takes 42 first) |
| Worlds that move | — | B-182: none (no shipped unit carries Blink). B-194: none expected (probe e). B-185: battle 42 (new) and only the measured worlds where WP0's probe counts a strike, a tracked missile or a kept target that the attacker's player does not see; the goldens by the schema bytes only |
| Size | — | M: seven packages now, each XS to M, and WP7 after the contact solver |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`. `liberty EffectData:N` is line N of that layer's file | Every [DATA] value |
| S2 | `CEffectLaunchMissile` default, `core EffectData:95–109`: `Flags PointFallback 1` (:97), `ValidateAbil 1` (:98), `ValidateBenign 1` (:99), `ValidateTeleport 1` (:100), `ValidateWeapon 1` (:101), `ValidatorArray ##id##TargetFilters` (:105), `ImpactLocation TargetUnit` (:107) | Every launched missile drops a teleported target and re-checks the weapon's filters, and falls back to a point when either fails |
| S3 | The only `CEffectLaunchMissile` rows of S1 that clear any of these flags: `NeuralParasiteLaunchMissile` `PointFallback 0` (`liberty EffectData:1013`); `BroodlingEscortLaunchA`, `BroodlingEscortMissile`, `BroodlingEscortFallbackMissile` `ValidateWeapon 0` (`liberty :2664–2671`, `:2741–2751`, `:2774–2782`; the flags at :2668, :2749, :2780); `ReaperKD8KnockbackPHLM` `ValidateAbil 0`, `ValidateTeleport 0`, `ValidateWeapon 0` (`void :3324–3332`, a grenade, not an attack) | No attack missile clears `ValidateTeleport`; only the Brood Lord's clear `ValidateWeapon` [DATA] |
| S4 | `CWeapon` default `TargetFilters Visible;-` (`core WeaponData:29`). All 146 `TargetFilters` rows of the eight `WeaponData` layers require `Visible` (`grep -h TargetFilters */WeaponData.xml`; typical `liberty WeaponData:9`, `Visible;Missile,Stasis,Dead,Hidden,Invulnerable`) | A weapon's target must be visible to the weapon's player [DATA]; "visible" also means detected when cloaked (the filter the vision plan's S4 reads) |
| S5 | `CAbilAttack` default, `core AbilData:115–139`: `AcquireFilters -;Player,Ally,Neutral,Dead,Hidden,Invulnerable,Benign` (:118), no `Visible`; no attack-ability flag about a target lost to fog in any layer (`grep` for `Fog`, `Lost`, `Chase`, `Leash` finds only `ClearRallyOnTargetLost`, `core UnitData:55`, a rally flag, and the Carrier's `Leash`) | What the attack *order* does when its target is lost is the engine's, not the data's (**verify**, §15 part 1 and 2) |
| S6 | `CEffectTeleport` default, `core EffectData:161–170`; `Blink` (`liberty :2347–2356`, `WhichUnit Caster`, `TeleportFlags TestCliff`; `libertymulti :56–58` adds `CasterNotFungalGrowthed`); `DarkTemplarBlink` (`voidmulti :1474–1485`); `AdeptPhaseShiftTeleport` (`void :532–537`); `MassRecallTeleport` (`liberty :695–703`), `NexusMassRecallTeleport` (`voidmulti :1909–1918`), `MothershipStrategicRecallTeleport` (`voidmulti :2242–2251`); `HyperjumpTeleport` (`void :1063–1071`) | The teleports of the game are `CEffectTeleport` effects. Which relocations raise `ValidateTeleport` is the engine's; this plan reads it as "every `CEffectTeleport`" [INF, **verify**, §15 part 3] |
| S7 | `BroodlingEscortDamageUnit` and `BroodlingEscortDamage` (`liberty EffectData:2678–2682`, `:2784–2789`): `ValidatorArray BroodlingEscortFilters` (:2679, :2785); `BroodlingEscortFilters` (`liberty ValidatorData:1723–1726`): `Ground,Visible;Missile,Stasis,Dead,Hidden,Invulnerable` | The Brood Lord's missile validates at impact what other missiles validate in flight [DATA] |
| S8 | `CrucioShockCannon` (`liberty WeaponData:255–`): `TargetFilters Ground,Visible;…` (:258), `Range 13` (:259); `SiegeTankSieged` `Sight 11` (`liberty UnitData:5171`) | The sieged Siege Tank outranges its own sight by 2: it fires beyond 11 only where another unit gives vision [DATA + rule S4] |
| S9 | `AttackReveal` (`core BehaviorData:237–240`, 3 Normal s), as built by the vision plan (B-55) | A revealed attacker is visible to the struck player; the reveal is what makes it a valid target from below (§2.2 rule 5) |
| S10 | `CEffectDamage` default (`core EffectData:61–69`): no validator; `DU_WEAP_MISSILE` children of shipped weapons add none (survey of the vision plan, S5) | A standard missile's damage checks nothing at impact beyond what the missile validated in flight [DATA] |
| S11 | `CEffectRedirectMissile` default `ImpactFilters Visible;Stasis,Dead,Hidden,Invulnerable` (`core EffectData:140`) | The engine's own phrase for "a valid missile target" includes `Visible` [DATA, consistent with S4] |
| S12 | Liquipedia, *Automatic Targeting* (as cited by `target_acquisition.rs` invariant 3): a unit keeps a target while it is valid and in reach; once it is lost, it takes the best candidate | An automatic target that fails the filters is lost and replaced [EXT]; what a unit with no candidate does is not stated (**verify**, §15 part 2) |

### 2.2 The rules that follow

1. **A missile drops a teleported target** (S2, S6). When the target is relocated by a teleport after the missile
   launched, the missile no longer homes on it and lands on the last point it tracked, which is where the target
   stood before the teleport [INF: the flag names a validation of the target unit; `PointFallback` gives the point].
   A single-target missile strikes nobody; an area missile strikes around that point (B-54's rule). A missile
   launched after the teleport tracks the target normally.
2. **A weapon strikes only a target its player sees** (S4, S11). At the start of a swing, at each strike that falls
   due, and at impact, the target must be visible to the attacker's player: its cell `Visible`, or the unit
   attack-revealed to that player (S9); if concealed, also detected (unchanged).
3. **A missile re-checks the weapon's filters in flight** (S2 `ValidateWeapon`), so one whose target walks into fog
   lands on the point. The check is per tick, as for the other causes of B-54 (the vision plan's §2.2 rule 2) [INF].
4. **The Brood Lord's missiles check at impact instead** (S3, S7): in flight they follow their target while it
   exists; on landing, the damage strikes only a target that passes `Ground,Visible;Missile,Stasis,Dead,Hidden,
   Invulnerable`. A missile at a target that died still lands on its point (it has no unit to follow) [INF].
5. **The reveal bounds B-55** (S4, S9): a struck player can fire at a hidden air attacker while the reveal lasts and,
   after it ends, only if its own vision reaches the attacker. A victim that acquired the attacker loses it then.
6. **What the order does is the engine's** (S5, S12). An automatic target that is lost is replaced by the best valid
   candidate; this plan reads "no candidate" as "the automatic attack ends and the unit's own order resumes" [INF,
   **verify**]. A player's attack order on a target lost to fog walks to where the target was last seen and then
   ends [**verify**, from play; no data]. Both are §15 questions.

### 2.3 Survey: missile validation of every shipped projectile weapon

| Ours (slug, weapon) | 5.0.14 missile | `ValidateWeapon` | `ValidateTeleport` | Source |
|---|---|---|---|---|
| `spore_matriarch` (Brood Lord), both weapons | `BroodlingEscortLaunchA` / `BroodlingEscortMissile` / `BroodlingEscortFallbackMissile` | **0** | 1 | S3, S7 |
| every other projectile weapon: `rhino_beetle`, `thorn_viper`, `hive_matron`, `mortar_stag`, `sonic_bat`, `sky_leviathan`, `warp_strider`, `shock_golem`, `astral_vagrant`, `cerebral_elder`, `cloak_gunship`, `sky_bastion`, `vector_interceptor`, `mothership_carrier`, `flak_array_dome`, `spore_spire_dome`, `tesla_sentinel_dome` | their `…LaunchMissile` rows, none overriding the flags | 1 (default) | 1 (default) | S2, S3 |

The rule this table follows is "every projectile weapon validates both, except the Brood Lord's, which does not
validate the weapon in flight". WP3's test reads it by `sc2_analogue` (as `weapon_catalog_rules_tests` reads the air-only
rows) and cites the table; the data authors the one exception explicitly and leaves the default elsewhere.

### 2.4 Survey: where ours fires beyond its own sight

The weapon-filter rule matters where a target in reach is outside the attacker's player's vision. From the catalog
(`max_range` plus both collision radii against `sight_radius`, whole-cell sight):

| Case | Units | Why |
|---|---|---|
| Range beyond sight | `railgun_crawler` sieged (13 against 11, S8) | The classic spotter rule |
| Reach beyond sight by the radii | `vector_interceptor` (9 + 0.75 + target radius against 10), `colossus_mech` (11 + 1.0 + r against 11), `spore_matriarch` (10 + 1.0 + r against 12, only against radius > 1) | A large target at full reach has its centre cell outside sight |
| Terrain | any unit below a cliff or outside a sight blocker | Ground sight does not climb (B-57 and the cliff rule); the attack reveal (B-55) opens a window |

### 2.5 Neighbouring rules: in or out

| Rule | Verdict | Why |
|---|---|---|
| B-182, B-185 (weapon half), B-194 | **In** (D-1 to D-6) | |
| A player's attack order on a target lost to fog chases by the true position — **B-195, registered here** | Split: WP7, **after the contact solver merges** | Pursuit's goal is under `movement/` (`locomotion.rs` `start_pursuit`, `chase_target`). Q0106 part 1 decides the rule |
| `ValidateAbil`, `ValidateBenign` (S2) | Out | They validate the ability and the alignment of spell missiles; attack missiles carry no ability (verify) and never turn benign |
| Effect-tree missiles (spells, `effects::ActiveMissile`) | Out (as the vision plan's F-4) | Each spell missile carries its own flags; `missile_aim` keeps `is_entity_targetable` |
| An attack *command* naming a unit the player does not see | Out, not registered | The client offers only seen units; whether the game refuses such a command from a trigger is not in the data |
| Units that move their target without teleporting (Abduct's `YoinkLaunchTarget` is a launch, `swarm` EffectData:1554–1565; transport loading makes the target `Hidden`) | Unchanged | Not a `CEffectTeleport`; a loaded target is already lost (B-54) |
| Allied vision | Out | No alliance relation in the simulation (as Q0101 part 4) |

---

## 3. Diagnosis (on `cff46b37`, read from the code)

1. **One availability gate without vision.** `SimWorld::is_entity_targetable` (`target_acquisition.rs`) is
   `is_damageable(target) && (!concealed || is_cloak_revealed)`. Its wrappers `is_entity_available_for_combat` and
   `is_target_available_for_combat` serve: `apply_weapon_hit` (`combat_resolution.rs`), `track_projectile_target`
   and `nearest_projectile_retarget`, acquisition's `reachable_current` in `decide_acquisition`, `world/attack_dash.rs`,
   `world/casts.rs`, `effects/step.rs` `missile_aim`, and pursuit (`movement/locomotion.rs` `start_pursuit`). The
   weapon phase decides swings and pending strikes by `StrikeContext::is_available` on a `TargetSnapshot` (damageable,
   cloaked), the same rule. Only `scan_candidates` (new targets), `is_attacker_visible_to` (retaliation),
   `is_seen_by` (casts) and `is_visible_enemy_target` (dashes) add `sees_enemy`. So once acquired, or named by a
   player, a target is tracked, struck and hit by missiles wherever it goes.
2. **Teleports leave no trace.** `commit_cast` (`world/abilities.rs`) applies `AbilityEffectSpec::BlinkTeleport` by
   writing `entity.position`. `track_projectile_target` refreshes `target_pos` from the target's position every tick,
   so the missile follows the blink. Nothing records that a teleport happened. No shipped archetype authors a
   `BlinkTeleport` ability (the `phase_blink` upgrade has no effect, `upgrades.ron`), so only tests reach this path.
3. **Missiles have no validation flags.** `WeaponSpec` carries `target_loss_behavior` (B-54's `PointFallback`) but
   nothing for `ValidateWeapon` or `ValidateTeleport`; every missile validates exactly `is_entity_targetable`.
4. **Acquisition keeps an unseen target.** For `Engagement::Acquired`, `decide_acquisition` replaces the target with
   the best candidate when `reachable_current` is `None`, but with no candidate returns `None`, so the attack stays and
   pursuit follows the true position. `Engagement::Explicit` ends only for an immobile unit. Hold and tracking
   targets are released when `reachable_current` is `None`.
5. **Hashed state.** `Entity`, `ActiveProjectile` and `WeaponSpec` (inside each in-flight projectile) are hashed by
   their `fields!` lists in `checksum/encoding.rs`; schema 41 on `cff46b37`.

---

## 4. Design decisions

### D-1. `MissileValidation` on `WeaponSpec`

**Type** (`crates/sim_types/src/weapon.rs`, exported from `lib.rs`):

```rust
/// What a weapon's missile re-checks about its target in flight: `StarCraft` II's
/// `CEffectLaunchMissile` flags (`core` EffectData:95–109). A check that fails makes the
/// missile lose its target, and `target_loss_behavior` decides what it does then.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissileValidation {
    /// `ValidateWeapon` (:101): the target must keep passing the weapon's filters (alive,
    /// unloaded, vulnerable, seen and, if concealed, detected). `false`: only its existence
    /// is followed, and the filters are checked at impact instead.
    pub weapon: bool,
    /// `ValidateTeleport` (:100): a target that teleports after the launch is lost.
    pub teleport: bool,
}
```

`Default` is written by hand as `{ weapon: true, teleport: true }` (the engine default). `WeaponSpec` and its catalog
def gain `#[serde(default)] pub missile_validation: MissileValidation`; a RON row authors it as
`missile_validation: (weapon: false, teleport: true)`. **Validation:** a weapon without `projectile_speed` that
authors a non-default value is refused with a typed `CatalogError` naming the weapon and the field (an instant weapon
has no missile, so the value would be silently meaningless).

### D-2. Teleports are counted

- `Entity` gains `pub teleport_count: u16`, zero at spawn: how many times the unit has been relocated by a teleport,
  wrapping. `Entity::teleport_to(&mut self, destination: SimVec2)` sets the position, zeroes the velocity, clears the
  funnel waypoints and increments the counter (`wrapping_add`). `commit_cast`'s `BlinkTeleport` arm calls it in place
  of its three writes. The method's doc states the invariant: "Every instantaneous relocation that 5.0.14 performs
  with a `CEffectTeleport` goes through this method." Displacements that fly (`Displace`), unloads and placement do
  not (§2.5).
- `ActiveProjectile` gains `pub target_teleport_count: u16`: the target's counter when the projectile was launched or
  last retargeted (`launch_projectile`; `nearest_projectile_retarget`'s caller). Zero for a projectile without a
  unit target.
- **Why a counter, not a "teleported this tick" flag:** the counter is compared, never reset, so the order of the
  ability phase and the projectile phase within a tick does not matter, and two teleports between two projectile
  steps cannot be missed. Wrapping needs 65,536 teleports during one flight (lifetime ≤ 125 ticks) to alias.

### D-3. The weapon filter

**One predicate** (`target_acquisition.rs`, `pub(crate)`), beside `is_entity_targetable`:

```text
passes_weapon_filters(target, map, vision_maps, attacker_owner) :=
       is_entity_targetable(target, map, vision_maps, attacker_owner)   # alive, unloaded, vulnerable, detected if concealed
    && sees_enemy(map, vision_maps, attacker_owner, target)               # its cell Visible, or attack-revealed (B-55)
```

with the wrapper `is_weapon_target_available(&self, attacker_owner, target_id)`. Its doc cites S4. `sees_enemy`
returns `true` for a player without a vision map (unchanged), so worlds without vision (sandbox tests) keep today's
behaviour.

**Readers** (and only these):

| Reader | Today | After |
|---|---|---|
| `StrikeContext::is_available` (swing start, pending strikes) | damageable, concealment | + seen: `TargetSnapshot` gains `attack_revealed: PerPlayer<bool>` (copied from `attack_reveal_ticks` at the phase start, like the position) and the check calls `sees_enemy`'s rule on the snapshot |
| `apply_weapon_hit` (instant shots, projectile impacts) | `is_entity_available_for_combat` | `is_weapon_target_available` |
| `track_projectile_target` | `is_entity_available_for_combat` | D-4 |
| `nearest_projectile_retarget` | `is_target_available_for_combat` | the filter (no shipped weapon retargets) |
| `decide_acquisition` `reachable_current` | `is_entity_targetable` | `passes_weapon_filters` (D-5) |
| `world/attack_dash.rs` `is_visible_enemy_target` | targetable and `sees_enemy` | `passes_weapon_filters` (the same rule, one call) |

**Unchanged on purpose:** `is_entity_available_for_combat` keeps its meaning, because pursuit
(`movement/locomotion.rs`) reads it and the chase is WP7's; `world/casts.rs` `is_seen_by` and `effects/step.rs`
`missile_aim` (spells, out of scope); `is_attacker_visible_to` (already sees). The module doc of
`target_acquisition.rs` gains: "Weapons strike only what their player sees (`passes_weapon_filters`, 5.0.14
`TargetFilters Visible`); pursuit's gate, `is_entity_available_for_combat`, does not ask for vision (plan
`2026-11-24-weapon-validation` D-3, B-195)."

**Timing.** Vision maps are those of this tick's phase 3; positions are current. A target that walks into a fogged
cell during movement is unseen by phase 7, as a target that cloaks is today.

**Instant shots.** A shot prepared and applied in the same phase sees the same vision and positions (combat
resolution's invariant 3), so the impact check adds nothing for instant weapons; it matters for projectiles (D-4).

### D-4. What a missile validates

`track_projectile_target`, for a projectile with a unit target:

```text
target := entities[target_id]
lost_teleport := spec.missile_validation.teleport
                 && target exists && target.teleport_count != proj.target_teleport_count
followed := if spec.missile_validation.weapon { passes_weapon_filters(target, …, proj.owner) }
            else { target exists && target.is_alive() && !target.is_loaded_in_transport() }
if followed && !lost_teleport: refresh target_pos and radius from the target (as today)
else: apply target_loss_behavior (as today; target_pos keeps the last tracked point)
```

- The teleport check runs **before** the position refresh, so the landing point is the target's position on the last
  tick it was followed: before the teleport (§2.2 rule 1).
- `missile_validation.weapon: false` follows the target through fog, cloak and invulnerability. At impact,
  `apply_weapon_hit` applies the full filter (D-3), which is S7's `BroodlingEscortFilters` (its `Ground` half is the
  weapon's target flags). A loaded target is not followed (its position is the transport's; **verify**, recorded as
  an [INF] in the code comment).
- `target_loss_behavior` is unchanged: the shipped default `ImpactLastKnownPosition` (B-54).
- `combat_resolution.rs` gains invariant 9: "A missile loses its target when the target teleports after the launch,
  unless its weapon clears `missile_validation.teleport`, and when the target fails the weapon filter, unless its
  weapon clears `missile_validation.weapon`; a missile that does not validate the weapon strikes only a target that
  passes it at impact (plan `2026-11-24-weapon-validation` D-4)."

### D-5. Acquisition: a target lost to fog

In `decide_acquisition`, `reachable_current` uses `passes_weapon_filters` (D-3). Then:

- **Hold and tracking** (`Engagement::Hold`, `Track`): unchanged code; an unseen target makes `reachable_current`
  `None`, so `ReleaseHoldTarget` (as for a concealed target today).
- **Explicit, immobile:** unchanged code; `AbandonAttack` (a sieged tank's attack order on a target that leaves its
  player's sight ends, as for a concealed one today).
- **Acquired:** if the scan finds a candidate, the existing rule engages the best one (an unseen target is never kept
  over it, since it is not "reachable"). If it finds none **and** the current target is *lost to fog*, meaning it
  exists, passes `is_entity_targetable` and fails `sees_enemy`, the decision is `AbandonAttack`. The acquired attack
  is popped, the order below it (an attack-move) resumes, and an idle unit stays idle. A target that is merely out of
  reach and seen is still chased, as today. A concealed target is still ended by pursuit as today, so that path's
  worlds do not move.
- **Explicit, mobile:** unchanged; the order stays, pursuit follows the true position (B-195), and the weapon holds
  fire while the target is unseen (D-3). WP7 replaces this.

This is the default of §15 part 2 (a). It is acquisition's decision on its own orders (invariant 1: "only
`AttackOrigin::Acquired` attacks are replaced"), and no file under `movement/` changes.

### D-6. Data

- `spore_matriarch`, both weapons: `missile_validation: (weapon: false, teleport: true)`, with the comment "5.0.14
  `BroodlingEscortLaunchA`, `BroodlingEscortMissile`, `BroodlingEscortFallbackMissile` clear `ValidateWeapon`
  (`liberty` EffectData:2668, :2749, :2780); the damage validates `BroodlingEscortFilters` at impact (:2679, :2785;
  ValidatorData:1723–1726); plan `2026-11-24-weapon-validation` D-4, B-194."
- Every other row keeps the default; no row authors it (the default is the data, as B-54's).

### D-6a. The chase (WP7, after the contact solver merges)

Recorded here so it can be planned against the solver's code, not built now. Under Q0106 part 1's default (a): a
player's attack order whose target is lost to fog walks to the target's last seen position and then ends; if the
target is seen again before that, the order resumes on it.
- State: the order needs the last seen point, so `UnitOrder::AttackTarget` or the unit's `MovementMemory` gains a
  hashed `last_seen: Option<SimVec2>` (the choice is the solver's layout; schema next free at merge).
- Acquisition (phase 4) writes it each tick from the target's position while `sees_enemy` holds.
- Pursuit (`movement/locomotion.rs`) aims at `last_seen` while the target is unseen, and ends the order on arrival.
- WP7 begins with an amendment to this plan that fixes these against the merged solver, and with its own probe
  (explicit orders on unseen targets, probe (c) of §7.2).

### D-7. Hashed state and checksum

- `fields!(Entity { …, teleport_count })` and `fields!(ActiveProjectile { …, target_teleport_count })` after the
  last field; `fields!(WeaponSpec { …, missile_validation })` with a `Canonical` for `MissileValidation` (the two
  booleans in order, one byte each by the existing `bool` encoding).
- **Schema: next free at merge.** 42 is reserved by the contact-solver branch, so this plan proposes **43**. The doc
  of `CHECKSUM_SCHEMA_VERSION`: "Schema N adds each entity's teleport count after `<last field>`, each projectile's
  target teleport count after `<last field>`, and each weapon's missile validation (weapon, teleport) after
  `attack_visibility` (plan `2026-11-24-weapon-validation` D-7)."
- B-185's weapon half (WP5) adds no hashed state; it moves checksums by behaviour where the probe counts.

### D-8. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | B-182 and B-185 point to this plan (status "open; plan `2026-11-24-weapon-validation`"), B-185 restated as its weapon half with B-195 split off. Register **B-194** (Brood Lord `ValidateWeapon 0`) and **B-195** (a player's attack order chases a target lost to fog by its true position; 5.0.14 walks to the last seen point, **verify**, Q0106 part 1; after the contact solver) | WP0 |
| `DESIGN.md` §4 | B-182, B-185 (weapon half) and B-194 fixed with commits and evidence; B-185's row points to B-195 for the rest. The "Combat" row gains missile validation and the weapon filter | WP6 |
| `docs/plans/2026-11-23-vision-and-detection.md` | §14 F-1 and F-3: "taken up by plan `2026-11-24-weapon-validation`" | WP6 |
| New `docs/evidence/2026-11-24-weapon-validation.md` | §7's comparisons per package, the probes' counts, the traced first differences, the golden proofs | WP0 creates; WP1 to WP6 append |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Partly done` (WP6, WP7 pending), `Done` (WP7) | plan, WP1, WP6, WP7 |

---

## 5. Determinism and checksum

- **Integer only.** Counters are `u16` with wrapping increments and equality comparisons; the flags are booleans.
  No float is added; the catalog field is two booleans, so C7's allowances do not change.
- **No new randomness, clocks or collections.** `PerPlayer<bool>` in `TargetSnapshot` is a fixed array.
- **WP1 latent proof**, the procedure of the vision plan's §5:
  1. With the three fields left out of the encoding and the schema at its current number, the tree reproduces every
     recorded checksum of both goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release and on
     `wasm32-wasip1` release and debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`).
  2. Then the fields go in, the schema becomes the next free number, and the checksums are re-recorded under
     "Re-recorded for plan 2026-11-24-weapon-validation WP1 (checksum schema M -> N): each entity also encodes its
     teleport count, each projectile its target's teleport count and each weapon its missile validation, so every
     checkpoint changed. No behaviour did: with the three fields left out of the encoding at schema M, both fixtures
     reproduce as recorded for <the previous header>." The four builds reproduce the new values.
  3. If `main` takes the number first (the contact solver's 42 is expected to), the merge repeats step 2 with the
     next one.
- **WP2 (B-182) and WP3 (B-194) move no golden:** the goldens field no Blink and no Brood Lord analogue, so no counter
  changes and no flag is read differently. A move stops the package.
- **WP5 (B-185) moves goldens only if probe (a), (b) or (c) of §7.2 counts in a golden.** Predicted: none (their
  armies are Roach and Marine analogues, whose reach is inside their sight; **verify** by the probe). If it counts,
  the dump procedure of the vision plan's §5 applies: before and after dumps per checkpoint, the first difference
  must be a counted event, and the re-record header names it.
- **wasm.** No dependency changes.
- **Cost.** One `world_to_cell` and one vision lookup per weapon check (swing start, pending strike, impact, missile
  tick, kept target); the snapshot copies one `PerPlayer<bool>` per entity per tick. No allocation. The throughput
  bench is checked in the package gates.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` (a flat arena with vision;
a plateau with no ramp), read every range, speed and radius from the catalog, and find weapons by property
(`projectile_speed`, `splash`, `missile_validation`, `max_range` against `sight_radius`) or by `sc2_analogue` for a
survey row, never by slug. Blink is authored on a scratch copy of a shipped archetype (an `AbilityEffectSpec::
BlinkTeleport` ability, as `checked_movement_tests.rs` does), since no shipped unit carries it.

**B-182**
- **P-1 (a teleport drops the missile).** A missile in flight at a unit that Blinks lands on the point where the unit
  stood before the Blink, at the missile's speed, and strikes nobody; with a splash, its area strikes around that
  point. A missile launched after the Blink tracks the unit to its new position and strikes it.
- **P-2 (the flag).** With `missile_validation.teleport: false` on a catalog copy, the missile follows the Blink and
  strikes, in the same binary (zero recompilation).
- **P-3 (the counter).** `teleport_to` increments `teleport_count` once per call and wraps; a `Displace` flight, an
  unload and ordinary movement leave it unchanged.

**B-194**
- **P-4 (Brood Lord).** A missile whose weapon clears `missile_validation.weapon` keeps homing on a target that
  cloaks undetected, or walks into fog, and lands on the target's current position; it strikes the target only if
  the target passes the weapon filter on the landing tick (detected and seen). A target that dies is not followed:
  the missile lands on its last point.
- **P-5 (shipped table).** §2.3: exactly the weapons of the archetype whose `sc2_analogue` begins "Brood Lord" clear
  `missile_validation.weapon`; every projectile weapon validates teleports; no instant weapon authors the field, and
  validation refuses one that does.

**B-185**
- **P-6 (no strike unseen).** On a flat map with vision, a unit whose target stands in a cell its player does not see
  starts no swing and lands no pending strike on it, and resumes once an allied source of the same player sees the
  cell, or once the target is attack-revealed to that player. A concealed target still needs detection. A player
  without a vision map fires as today.
- **P-7 (missile into fog).** A missile whose target walks into a cell its player does not see lands on the last point
  it tracked and strikes nobody (with `missile_validation.weapon: true`).
- **P-8 (the order).** An acquired attack whose target is lost to fog switches to the best seen candidate, or, with
  none, is dropped so that the attack-move below it resumes. A seen target out of reach is still chased. A hold or
  tracking target lost to fog is released. An immobile unit's attack order on a target lost to fog ends. A mobile
  unit's attack order is kept (B-195) and does not fire while its target is unseen.
- **P-9 (the reveal's end).** A unit below a cliff that acquired a hidden air attacker during its reveal stops
  firing at it when the reveal ends, unless its player sees the attacker's cell.
- **P-10 (spotter).** A sieged tank analogue (found by `max_range` beyond `sight_radius`) does not shell a target in
  range beyond its sight; it does once another unit of its player sees the target.
- **P-11 (confined).** WP5 changes only the worlds where WP0's probe counts an event of kind (a), (b) or (c), and in
  each, the first difference is a counted event.
- **P-12 (direction, a prediction recorded before measuring, not a threshold).** In battle 42 (§7.4), before WP5 the
  sieged tank analogue kills workers that fled beyond its sight; after WP5 it fires at none beyond its sight, and
  more workers survive.

**Latent**
- **P-13.** WP1 changes no world (goldens by the schema bytes only, proved); WP2 and WP3 change no world and no
  golden.

---

## 7. Worlds that move

### 7.1 Commands

The vision plan's scripts (`~/scratch/vision-detection/scripts/measure.sh`, `compare.sh`, `client.sh`,
`goldens.sh`, `proof.sh`), copied to `~/scratch/weapon-validation/scripts/` with their paths changed. `measure.sh`
runs `route_trace` with `--assets <tree root>/assets`, so each tree is measured with its own data. Binaries are built
once per tree:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/weapon-validation
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64, and the
showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks. Run it in the
background, one at a time across worktrees (charter §7). The client skirmish (`skirmish_match_tests`) is compared by
its stage log (`client.sh`).

### 7.2 WP0's probes

On WP0's tree a scratch patch prints five kinds of event; it is never committed and is kept under
`~/scratch/weapon-validation/probe/` with its diff. The probe plays every world of §7.1 and the two goldens (through a
scratch test). The evidence records each count per world and seed and the first event of each kind.

- **(a) Unseen strikes:** every swing start and pending strike that `StrikeContext::is_available` admits, and every
  `apply_weapon_hit` that lands, on a target that fails `sees_enemy` for the attacker's player. Tagged with the
  attacker's archetype, whether the attacker is sieged or below the target's level, and the distance against its
  sight. *Expected:* sieged tanks (battles 3, 37a, 37b, 39a, 39b, 40, and the showcase if its pilot sieges) and
  Vikings at full reach on large targets (battles 12c, 19, 35); none in battles 31, 33 or the goldens.
- **(b) Unseen missiles:** every `track_projectile_target` call that follows a target failing `sees_enemy`, tagged
  with whether the weapon is the Brood Lord's (D-6) and whether it has a splash. *Expected:* a subset of (a)'s
  battles; none with a splash.
- **(c) Kept unseen targets:** every `decide_acquisition` call whose current target exists, passes
  `is_entity_targetable` and fails `sees_enemy`, by engagement (`Acquired`, `Explicit` mobile or immobile, `Hold`,
  `Track`), with whether the scan found a candidate. The `Explicit` mobile count is B-195's exposure, recorded for
  WP7. *Expected:* as (a); battle 41 none (the Void Ray strikes every period, so its reveal never lapses: the vision
  evidence found no Hydra shot outside a window).
- **(d) Teleports:** every `BlinkTeleport` commit, and among them those with a projectile homing on the unit.
  *Expected:* zero everywhere (no shipped Blink).
- **(e) Brood Lord missiles** whose target fails the concealment half of the filter in flight. *Expected:* zero
  (battles 19 and 20 field no cloak).
- **Rule:** WP4 adds battle 42 (§7.4) whatever the counts, because it is the principled world for P-10 and P-12.
  P-11 is judged on the worlds that (a) to (c) count.

### 7.3 Expected moves

| World | WP1 (latent, schema) | WP2 (B-182) | WP3 (B-194) | WP4 (battle 42) | WP5 (B-185) |
|---|---|---|---|---|---|
| Matrix (seeds of record and 16) | byte-identical | byte-identical (probe d) | byte-identical (probe e) | byte-identical except the new battle | **moves only in battles that (a) to (c) count**, from the first counted tick; battle 42 re-recorded |
| Battles 31, 33 | byte-identical | byte-identical | byte-identical | byte-identical | byte-identical (predicted no count) |
| Showcase, default and 1–8 | byte-identical | byte-identical | byte-identical | byte-identical | moves only on seeds that (a) to (c) count (sieged tanks, **verify** from the probe) |
| Showcase effects, default and 1–8 | byte-identical | byte-identical | byte-identical | byte-identical | as the showcase |
| Client skirmish stage log | identical | identical | identical | identical | identical unless its pilot sieges a tank beyond sight (**verify** from the stage log) |
| Goldens, sandbox | **schema bytes only** (proof, §5) | unchanged | unchanged | unchanged | unchanged (predicted no count; else traced, §5) |

**What each moving package records** for every world that moved (in the evidence, all seeds, before and after): the
first differing tick and the probe event behind it, each side's survivors and the tick of the last death.

**Direction for WP5 (a prediction, not a threshold).** A side that relied on firing beyond its own vision (sieged
tanks without spotters, Vikings at full reach) deals its damage later or not at all, so its enemies lose no more
units than before. Which side remains should not change for a battle whose claim is about something else. A change
is traced (§8 rule 3), not accepted as measured. Battle 41 is predicted byte-identical.

### 7.4 Battle 42 (WP4)

Why a battle: the matrix is the world of record for combat rules and plays 20 seeds. Why this one: the sieged tank
outranging its sight is the best-known case of the rule (S8), and the helpless flee (`HELPLESS_FLEE_DISTANCE`, 5)
moves a struck unit from inside the tank's sight to outside it but inside its reach, with no new order type.

- `42_sieged_tank_vs_fleeing_workers` (or the next free battle number at merge), map 64, 900 ticks, flat. Red: one
  `railgun_crawler` at (50.0, 32.0) in `siege_mode` (the `mode` field of battle 40), no order. Blue: four
  `welder_drone`s in a column at x = 41.5 (8.5 from the tank), spacing 1.5, no order.
- The positions follow one rule: the workers stand within the sieged tank's sight (11) and reach (13 plus radii), and
  outside their own sight (8) of the tank, so a struck worker cannot answer and flees 5 directly away, to about 13.5:
  outside the tank's sight, inside its reach (13 + 0.875 + 0.375 = 14.25).
- Claim: "A sieged tank cannot shell what its side does not see (5.0.14 `TargetFilters Visible`; Siege Tank range 13,
  sight 11)". The expectation is recorded on WP4's tree (today: the tank keeps its fled targets and kills them), and
  re-recorded in WP5 with the probe's trace (P-12).
- **verify** in WP4 that a struck worker flees rather than answers, and that the flight ends between 11 and 14.25 on
  every seed. If not, the column moves along x within those bounds (one rule: inside sight and reach, outside the
  workers' sight), stated in an amendment.

---

## 8. Stop rules

1. **WP1 is latent.** Every world of §7 byte-identical; goldens moved by the schema bytes only, proved (§5).
2. **WP2 and WP3 move nothing.** Every world, golden and the sandbox byte-identical; probe (d) and (e) counted zero.
   A move stops the package until traced.
3. **WP5 moves only the worlds its probe counted** (P-11). A difference elsewhere, or on a seed with a zero count, is
   a defect of the package, not a re-record. A move whose first differing tick is not a counted event stops the
   package until traced. If the trace shows a second cause, that cause is registered as a bug, and the package lands
   only if this rule's change can be separated from it. A change of which side remains is traced to its first counted
   event and reported. It is not by itself a stop, as the rule is sourced. A matrix claim that breaks is reported to
   the orchestrator with its trace, as Q0103 did for the solver, and not rewritten in the package.
4. **No tuning constant.** Every value is the data's (the two flags) or an existing engine rule's.
5. **Nothing under `crates/sim_core/src/movement/` changes** before WP7. A package that needs it stops and reports.

---

## 9. Client consequences

- **B-182, B-185:** a missile that drops its target is drawn to its landing point, as B-54's lost missiles already
  are; nothing new to draw. Units that hold fire at an unseen target show no swing.
- No `PlayerView`, `RenderSnapshot`, shader or capture change. `effect_snapshot_tests.rs` and `battle_fog_tests.rs`
  run in WP5's package gate on their catalog triggers.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP6 | D-8 |
| `docs/plans/README.md` | plan, WP1, WP6, WP7 | Row |
| `docs/evidence/2026-11-24-weapon-validation.md` (new) | WP0 to WP6 | §7, §5 proofs |
| `crates/sim_types/src/weapon.rs`, `crates/sim_types/src/lib.rs` | WP1 | D-1 `MissileValidation`, field |
| `crates/sim_core/src/catalog.rs`, `crates/sim_core/src/catalog/validation.rs` | WP1 | Def field; the instant-weapon refusal |
| `crates/sim_core/src/entity.rs` | WP1 | `teleport_count`, `teleport_to`; `ActiveProjectile::target_teleport_count` |
| `crates/sim_core/src/world/abilities.rs` | WP1 | `BlinkTeleport` through `teleport_to` |
| `crates/sim_core/src/combat_resolution.rs` | WP1 (record the count at launch), WP2, WP3, WP5 | D-4, invariant 9; D-3 readers |
| `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/checksum.rs` | WP1 | D-7 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `sim_fixtures`' `DEFAULT_SANDBOX_CHECKSUM`; the encoding unit tests' schema byte | WP1 | §5 |
| `crates/sim_core/src/target_acquisition.rs` | WP5 | D-3 predicate, D-5 |
| `crates/sim_core/src/world/attack_dash.rs` | WP5 | D-3 (mechanical) |
| `assets/catalog/units.ron` | WP3 | D-6 |
| `crates/sim_core/tests/weapon_validation_tests.rs` (new, `//!` doc) | WP1, WP2, WP3, WP5 | §12 |
| `crates/sim_core/tests/checksum_state_tests.rs` | WP1 | The three fields change the checksum |
| `crates/sim_core/tests/weapon_catalog_rules_tests.rs` | WP1, WP3 | Refusal; `missile_validation_follows_starcraft` (P-5) |
| `crates/sim_core/tests/runtime_catalog_tests.rs` | WP2, WP3 | P-2 and P-4's zero recompilation |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP4, WP5 | Battle 42; its re-recorded expectation; any other re-recorded battle with its trace |
| `crates/gate/suites.ron` | WP1, only if needed | The new test target (**verify**: an uncovered target may fall to its crate's default class) |
| `docs/plans/2026-11-23-vision-and-detection.md` | WP6 | Pointers |
| `crates/sim_core/src/movement/locomotion.rs` and the order or memory type | **WP7 only** | D-6a |

**Overlap with the contact solver:** it owns `movement/` and reserves schema 42; it may add `Entity` or
`MovementMemory` fields. Whichever merges second rebases its `fields!` lists and takes the next schema number (R-3).

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`
(check `quota -s` and `df -h ~/scratch` first). Write the failing tests first and paste their red run in the
hand-back. Run `cargo gate quick` and the tests the change needs; a verifier runs `cargo gate package` (Q0061). Run
§7's comparison in every package against WP0's tree. No `unwrap`; `expect` only with `invariant:`; no catalog
identity in production Rust (C5); tests find weapons by property. Check `docs/questions/open/` before the last commit.
WP0 to WP6 may run as one branch and one session, one commit each, with one `package` gate on the final tree (charter
§4.3). WP7 is a separate branch after the contact solver merges.

### WP0: records, baseline and probes (XS, docs only)

- `DESIGN.md` §4: B-182 and B-185 pointed here; B-194 and B-195 registered (D-8). Evidence file created.
- Measure WP0's tree (§7.1) for the baseline. Run §7.2's probes and record their counts and first events. State in
  the evidence which worlds WP5 is expected to move.
- **Gate:** docs-only.

### WP1: missile validation and teleport state, latent (S, `sim_types`, `sim_core`, schema)

Failing tests first (red: they do not compile):
- `sim_types` unit tests: `MissileValidation` defaults to both `true`; a RON row reads both cases.
- `weapon_catalog_rules_tests`: an instant weapon authoring `missile_validation` is refused, naming the field
  (edited shipped text, as that file's other refusals).
- `checksum_state_tests`: each of the three fields changes the checksum.
- `weapon_validation_tests::a_blink_counts_one_teleport` and `entity.rs` unit tests for `teleport_to` (P-3).

Then D-1, D-2 (the counter is written but read by nothing), D-7, and the count recorded at launch. The schema proof of
§5. **Acceptance:** §8 rule 1. Plan row `Active`. Commit `feat(sim_core): missiles carry their validation flags and
units their teleport count, latent (weapon-validation WP1, schema N)`.

### WP2: a missile drops a target that teleports (S, `sim_core`, B-182)

Failing tests first (red on WP1's tree) in `weapon_validation_tests.rs`, with a scratch copy of a shipped ground unit
given a `BlinkTeleport` ability and a shipped missile weapon found by `projectile_speed`:
- `a_missile_at_a_blinker_lands_where_it_stood` (P-1: impact tick = distance / speed, nobody struck).
- `a_splash_missile_at_a_blinker_strikes_the_old_point` (P-1, a scratch splash, bystanders inside and outside).
- `a_missile_launched_after_the_blink_strikes` (P-1).
- `runtime_catalog_tests::an_authored_teleport_validation_off_follows_the_blink` (P-2).

Then D-4's teleport half. **Acceptance:** §8 rule 2. Commit `fix(sim_core): a missile drops a target that teleports,
5.0.14 ValidateTeleport (weapon-validation WP2, B-182)`.

### WP3: the Brood Lord's missiles validate at impact (S, `sim_core`, data, B-194)

Failing tests first (red on WP2's tree):
- `weapon_validation_tests::a_missile_that_skips_the_weapon_check_follows_a_cloaking_target` and
  `…_strikes_only_if_the_target_passes_at_impact` (P-4, a scratch weapon with `weapon: false`; the target cloaks, then
  a detector of the attacker's player arrives before impact, or does not).
- `…_lands_on_the_point_of_a_target_that_died` (P-4).
- `weapon_catalog_rules_tests::missile_validation_follows_starcraft` (P-5, §2.3).

Then D-4's weapon half for `weapon: false` (followed while alive and unloaded; the impact check is the existing
`apply_weapon_hit` gate, which WP5 extends to vision) and D-6's rows. **Acceptance:** §8 rule 2. Commit
`fix(sim_core): the Brood Lord's missiles follow their target and validate it at impact, 5.0.14 ValidateWeapon 0
(weapon-validation WP3, B-194)`.

### WP4: battle 42 (S, fixture)

Battle 42 (§7.4) with its expectation recorded on this tree; the matrix test passes. **Acceptance:** §8 rule 1 for
every other world. Commit `test(sim_core): battle 42, a sieged tank against workers that flee beyond its sight
(weapon-validation WP4)`.

### WP5: weapons strike only what their player sees (M, `sim_core`, B-185 weapon half)

Failing tests first (red on WP4's tree), in `weapon_validation_tests.rs`, on a flat arena with vision and on a plateau
map, with units found by property:
- `no_swing_starts_on_an_unseen_target`, `a_pending_strike_holds_on_an_unseen_target`,
  `a_spotter_lets_the_strike_land`, `an_attack_revealed_target_is_struck`, `a_world_without_vision_fires_as_before`
  (P-6).
- `a_missile_whose_target_walks_into_fog_lands_on_the_point` (P-7).
- `an_acquired_target_lost_to_fog_gives_way_to_a_seen_one`, `…_with_none_the_attack_move_resumes`,
  `a_seen_target_out_of_reach_is_still_chased`, `a_hold_target_lost_to_fog_is_released`,
  `an_immobile_units_order_on_a_lost_target_ends`, `a_players_order_is_kept_but_holds_fire` (P-8).
- `the_reveals_end_ends_the_answer` (P-9, the plateau of battle 41 with a scratch Void Ray copy that strikes once).
- `a_sieged_tank_needs_a_spotter_beyond_its_sight` (P-10).

Then D-3 and D-5. Battle 42's expectation and any battle the probe counted are re-recorded with their traces.
**Acceptance:** §8 rule 3; P-11, P-12; §7.3 column WP5. Commit `fix(sim_core): a weapon strikes only a target its
player sees, 5.0.14 TargetFilters Visible (weapon-validation WP5, B-185)`, stating which worlds moved and why.

### WP6: bookkeeping (XS)

D-8's records; the evidence completed; this plan's `## As built` for WP0 to WP6; README row `Partly done` (WP7
pending). Merge `main`; `cargo gate package` once on the final tree (by the verifier); the schema re-recorded if
`main` took the number (§5).

### WP7: the chase to the last seen point (M, `movement/`, B-195) — **after the contact solver merges**

Not started before the contact-solver branch is on `main` and Q0106 part 1 is answered or its default stands. It
begins with an amendment that fixes D-6a against the merged solver (where `last_seen` lives, the pursuit change, the
schema), its own probe (probe (c)'s `Explicit` mobile events on the then-current `main`), failing tests (a player's
order walks to the last seen point and ends; resumes if the target is seen again), the world comparison and stop
rule 3. README row `Done` after it.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where |
|---|---|---|
| Types and validation | Defaults; the instant-weapon refusal | WP1: `sim_types` unit tests, `weapon_catalog_rules_tests.rs` |
| State | `teleport_to`, the counter | WP1: `entity.rs` unit tests, `weapon_validation_tests.rs` |
| Rules | P-1, P-4, P-6 to P-10 | WP2, WP3, WP5: `weapon_validation_tests.rs` |
| Encoding | The three fields change the checksum | WP1: `checksum_state_tests.rs` |
| Data | §2.3 against 5.0.14 | WP3: `weapon_catalog_rules_tests.rs` |
| Zero recompilation | P-2, P-4 | `runtime_catalog_tests.rs` |
| Worlds | P-11 to P-13: §7.3 | every package (measure scripts); goldens and sandbox in the package gate (release and `wasm32-wasip1`) |
| Charter subjects | `sim_core`'s subjects of the guide (`damage_and_shields_tests.rs`, `full_match_tests.rs`, `golden_replay_tests.rs`, `lost_missile_tests.rs`, `attack_reveal_tests.rs`, `sc2_who_wins_matchup_matrix_tests.rs`, …) keep passing | package gate |

Re-derived (expected to change): tests that fire at a target their player does not see while vision maps exist
(search `vision_maps` set-ups in `crates/sim_core/tests/` that place a target beyond the attacker's sight, WP5, each
named in the commit); tests that blink a unit under fire (`checked_movement_tests.rs`, `movement_layers_tests.rs`;
none is expected to have a missile in flight, **verify**).

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | What an order does on a lost target (D-5, B-195) is not in the data | The acquired-attack drop or the later chase differs from the game | Q0106 parts 1 and 2; D-5 is one branch in acquisition; WP7 waits for the answer |
| R-2 | "Seen" is the target's centre cell (today's acquisition rule) | Vikings at full reach on large targets, and large structures, are lost sooner than in the game if the game counts the footprint | Q0106 part 4; one predicate to change; probe (a) counts the cases |
| R-3 | The contact solver takes schema 42 and adds `Entity` fields | Merge conflicts in `fields!` lists | "Next free at merge" (§5 step 3); the second branch rebases its lists |
| R-4 | B-185 moves matrix battles whose claims are about other rules (sieged tanks in 37, 39, 40; Vikings in 12c, 19, 35) | A recorded claim might fail | Stop rule 3: traced and reported, not rewritten in the package; the claims are expected to hold, as the moves are of timing |
| R-5 | An explicit attack order that holds fire in reach of an unseen target (below a cliff) stands still until WP7 | A new, visible deviation in the interim | Registered as B-195 with its exposure counted by probe (c); no measured world issues such an order except battles 12c and 20, whose targets stay seen (**verify** by the probe) |
| R-6 | The loaded-target case of `weapon: false` (D-4) is inferred | A Brood Lord missile at a unit that boards lands on the point instead of following the transport | Recorded [INF]; no measured world loads a unit under Brood Lord fire |

---

## 14. Out of scope and decisions recorded (Q0002)

- **B-195 (WP7): the chase.** A player's attack order on a target lost to fog walks to its last seen point and ends
  (Q0106 part 1 default). Needs pursuit under `movement/`; after the contact solver merges.
- **`ValidateAbil`, `ValidateBenign`** (S2): not modelled; attack missiles have neither an ability nor a benign
  alignment to lose.
- **Effect-tree missiles** (spells): unchanged; each would read its own flags, as the vision plan's F-4.
- **Other teleports** (Mass Recall, Adept Shade, Dark Templar Blink, Hyperjump, S6): none shipped; any later one goes
  through `Entity::teleport_to` (D-2's invariant).
- **Attack commands on unseen units:** admission is unchanged (§2.5).
- **Allied vision** (Q0101 part 4): no alliance relation.

---

## 15. Draft owner question (for the orchestrator to file as Q0106; not filed by this plan)

```markdown
# Q0106: When a target leaves vision, what counts as a teleport, and what part of a unit must be seen

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** a player's attack order on a unit that leaves vision walks to where the unit was last
  seen and then ends (built after the contact-solver work; until then it keeps following but holds fire); an
  automatic target that leaves vision is dropped at once, and the unit takes the best visible enemy or resumes its
  own order; every teleport effect (Blink, and later Mass Recall, Adept Shade, Hyperjump) makes missiles in flight
  miss; a unit is seen when the cell at its centre is visible (plan `2026-11-24-weapon-validation` D-4, D-5, D-6a).

## Question

Space Scuffle is adopting StarCraft II 5.0.14's weapon rules: a weapon fires only at a unit its player sees, a
missile whose target walks into the fog of war lands on the ground where the target was, and a missile at a unit that
blinks misses. The game data settle these, but not four details:

1. **A player's attack order on a unit that walks into the fog.** A Stalker is ordered to attack a Zergling, which
   runs out of the Stalker's sight. Does the Stalker (a) walk to where the Zergling was last seen and stop there,
   resuming the attack if it sees the Zergling again (the default); (b) stop at once; or (c) keep following the
   Zergling through the fog without firing?
2. **An automatic target that walks into the fog.** The same, but the Stalker was attack-moving and picked the
   Zergling itself. Does it (a) drop the Zergling at once and pick another visible enemy, or carry on attack-moving
   if there is none (the default); or (b) first walk to where the Zergling was last seen, as in part 1?
3. **Which moves make missiles miss.** Blink does. Do Mass Recall, an Adept's Shade and a Battlecruiser's Tactical
   Jump do too? (a) Yes, every teleport (the default); (b) only Blink.
4. **What part of a unit must be in vision.** A Viking (range 9, sight 10) shoots a Colossus at full range, where the
   Colossus's centre lies just outside the Viking's sight but its body inside. Can the Viking fire without another
   unit's vision? (a) No: the centre must be visible (the default, our current rule for picking targets); (b) yes:
   any part of the unit is enough.

If you can, check in the editor or a custom game: a Stalker chasing a Zergling into fog, ordered and on attack-move;
a Stalker blinking away from a Roach's or a Mass-Recalled army from Viking missiles in flight; and a lone Viking
against a lone Colossus at full range.

## Answer

```

The question number is the next free at filing (0106 on `cff46b37`, Q0103 to Q0105 being taken).

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`), read with `grep`, `sed` and `awk`: the `CEffectLaunchMissile` flag survey of S3 (every
  `index="PointFallback|ValidateAbil|ValidateTeleport|ValidateWeapon"` line of every layer mapped to its enclosing
  effect), the `TargetFilters` survey of S4 (146 rows, all with `Visible`), the `CEffectTeleport` survey of S6, and
  the search for attack-order fields of S5.
- The catalog survey of §2.3 and §2.4: every `projectile_speed: Some` row, and `max_range` against `sight_radius`,
  in `assets/catalog/units.ron` and `structures.ron`.
- No prototype: WP1 is latent by construction, WP2 and WP3 reach no measured world (probes d and e), and WP5's moves
  are predicted by probes (a) to (c) and traced.

---

## As built

