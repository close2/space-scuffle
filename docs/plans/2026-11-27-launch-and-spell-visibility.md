# Blueprint: Launch and Spell Visibility — What a Missile's Launch and a Spell's Damage Show of Their Caster (B-205, B-206)

This plan fixes two rows of `DESIGN.md` §4 that plan [`2026-11-24-attacker-snapshot`](2026-11-24-attacker-snapshot.md)
registered and deferred (its §14):

- **B-205:** "A weapon missile's launch does not snapshot its caster: 5.0.14's `CEffectLaunchMissile` default
  `Visibility Snapshot` (`core` EffectData:102) leaves the struck player an image a flight time before the impact
  does, from the launch position; ours records the impact only."
- **B-206:** "Spell damage carries no `Visibility`: `sim_types::effects::DamageDef` has no such field, so a spell's
  caster is neither revealed nor snapshotted to the player its damage strikes (5.0.14: `Snapshot` by default,
  `Hidden` for Psionic Storm, `liberty` EffectData:2099)."

In StarCraft II 5.0.14 `Visibility` is a field of two effect classes, not of the weapon. `CEffectLaunchMissile` applies
it when the missile leaves, to the player whose unit the missile is aimed at. `CEffectDamage` applies it when the
damage lands, to the player whose unit is struck. Both default to `Snapshot`. Spell trees are built from the same two
classes, so a spell's caster is snapshotted (or, where the data says `Visible`, revealed) as a weapon's attacker is.

The design in one paragraph: the type that already models the field, `sim_types::AttackVisibility` (`Snapshot`,
`Visible { reveal_ticks }`, `Hidden`), is reused in three new places with the engine default: a weapon's
`launch_visibility`, a spell `Damage` node's `visibility`, and a spell `LaunchMissile` node's `visibility`. The one hook
that already turns a strike into a reveal or a snapshot, `record_attack_presentation`, takes an *attack cause*
(visibility, attacker, its owner) instead of a weapon hit. It is called at a weapon missile's release, when a spell
damage node runs, and when a spell missile aimed at a unit is launched. The snapshot record, the 1-second cap
(Q0112 2(c)), the concealment rule (Q0112 3(a)), the reveal counter and the client's snapshot pool are all reused
unchanged. The shipped catalog needs **one** per-row edit: Psionic Storm's periodic damage authors `Hidden`, as 5.0.14
does. No shipped launch or spell damage is `Visible` in 5.0.14, so no reveal, target, outcome or measured world
moves. Only `WeaponSpec::launch_visibility` is hashed (schema next free at merge, **53** provisional), so the goldens
move by the schema bytes alone and are re-recorded once.

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `a0384b78`
(checksum schema 49).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** New bug rows start at **B-227** (B-224 to B-226 are the Gated
  plan's), the draft question is **Q0128** (Q0125 to Q0127 are the Gated plan's). The checksum schema is the next
  free number at merge: `main` is at 49, the follow-order branch takes 50, the Gated plan reserves 51 and 52, so this
  plan takes **53**. The Gated plan's WP5, a separate decision not yet scheduled, also names "53 provisional";
  whichever merges first takes 53 and the other the next free number.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S9, S10 |
| Q0101 2(b), 4(b): detection is a plain circle; a reveal reaches the struck player and its allies | Closed | D-4 (concealed casters, audience) |
| Q0106: a unit is seen when its centre cell is visible | Closed | D-4 (`shown_to` unchanged) |
| Q0112 1(a), 2(c), 3(a): no minimap (B-204); a snapshot ends when seen, at most 1 s after its last refresh; a concealed attacker only where detected | Closed | D-2, D-4: every new image obeys all three |
| Q0103: natural movement outweighs close matchup flips | Standing guidance | Not touched: no movement |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Gated plan coordination rule (`2026-11-27-gated-movement` §11): no other package edits `movement/`, `steering.rs`, acquisition's admission test or `movement_rules.ron` while a Gated package is open | Standing for this plan | §5, §8 rule 6, §10 "Not touched" |
| **Draft, not filed:** Q0128 "Images of spell casters and missile launches: three details" | Proposed (§15); defaults followed meanwhile | D-3, D-4, D-5 |

---

## 1. Summary

| | Today (`a0384b78`) | After this plan |
|---|---|---|
| A weapon missile is released at p's unit by an attacker p does not see | Nothing until the missile lands; then the impact's snapshot (ground) or reveal (air-plane) | On the release tick p gets the attacker's snapshot, from the release pose; the impact then refreshes it (`Snapshot`) or reveals the live unit (`Visible`) as today |
| An instant weapon (hitscan, beam, melee) | Snapshot or reveal at the strike | Unchanged: 5.0.14 has no launch effect for it, and validation refuses a launch visibility on it |
| A spell's damage (Psionic Storm's first hit, Fungal Growth's impact and pulses) lands on p's unit from an unseen caster | Nothing | p gets the caster's snapshot, refreshed by every later `Snapshot` damage of that caster; Psionic Storm's periodic pulses are `Hidden` and refresh nothing |
| A spell missile aimed at p's unit (Abduct's tongue) | Nothing | p gets the caster's snapshot on the launch tick |
| A spell missile aimed at a point (Fungal Growth) | Nothing | Nothing at the launch (no unit is aimed at; **verify**, Q0128 part 2); its impact's damage snapshots as above |
| A `Visible` launch or spell damage (none shipped; a catalog may author it) | Not expressible | Reveals the caster to p for `reveal_ticks` through the existing counter, so p's units may target it (§2.2 rule 4) |
| Concealed caster | — | An image only where p detects it (Q0112 3(a)); a reveal counts as sight only where detected (Q0101 2(b) resolution), as for weapons |
| Lifetime | Seen spot, seen attacker, or 25 ticks | The same for every image, whatever made it (Q0112 2(c) applies to launch and spell images; Q0128 part 3) |
| Client | Snapshot pool for weapon strikes | The same pool, unchanged code; tests extended |
| Hashed state | — | `WeaponSpec::launch_visibility`, one byte per projectile; schema 53 provisional. Spell node fields are catalog data, never hashed. No reveal counter moves in shipped data |
| Worlds | — | Every measured world byte-identical; goldens and the sandbox by the schema bytes only, re-recorded once |
| Size | — | M: WP0 (records, census, probes), WP1 (latent data, schema), WP2 (the rule), WP3 (client tests), WP4 (confirmation, re-record, records) |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`; `core EffectData:N` is line N of that layer's file | Every [DATA] value |
| S2 | `CEffectDamage` default, `core EffectData:61–70`: `Visibility Snapshot` (:62) | Every damage effect, weapon or spell, snapshots its caster unless overridden [DATA] |
| S3 | `CEffectLaunchMissile` default, `core EffectData:95–109`: `Visibility Snapshot` (:102), beside `ImpactLocation TargetUnit` (:107), `Flags PointFallback`, `ValidateWeapon`, `ValidateTeleport` | Every missile launch, weapon or spell, snapshots its caster unless overridden [DATA] |
| S4 | The `Visibility` survey of every layer of S1 (re-run for this plan, 2026-11-25; every `<Visibility` line mapped to its enclosing top-level effect): 2 defaults (S2, S3), 51 `CEffectDamage` overrides and 5 `CEffectLaunchMissile` overrides. Damage: 47 `Visible`, 4 `Hidden` (`PsiStormDamage`, `liberty :2099`; `KillRemove`, `swarm :1296`; `90mmCannonsDummy`, `voidmulti :1195` and `balancemulti :1070`). Launch: `CorruptionLaunchMissile` (`liberty :672`), `FrenzyLaunchMissile` (`:2946`), `ContaminateLaunchMissile` (`:2976`), `EyeStalk` (`swarm :2632`), `Doom` (`:2642`), all `Visible`, all spells | No weapon launch is overridden: **every weapon missile launch is `Snapshot`** [DATA]. The `Visible` spell damages are `SeekerMissileDamage` (`liberty :1173`), `YamatoU` (`:1853`), `RavenShredderMissileDamage` (`voidmulti :1845`), `GravitonBeamDummy` (`liberty :801`), `VortexDummy` (`:1525`) and the air units' weapons [DATA] |
| S5 | Psionic Storm, `liberty EffectData:2069–2101`: `PsiStormDamageInitial` (:2089–2096, no `Visibility`, so `Snapshot`; `ResponseFlags Acquire, Flee`) and `PsiStormDamage` (:2097–2101, `Visibility Hidden`). No later layer overrides either | The storm's first hit on a unit snapshots the High Templar; its pulses show nothing [DATA] |
| S6 | Fungal Growth: `FungalGrowthLaunchMissile` (`void :1141–1146`, no `Visibility`, `ImpactLocation TargetPoint`), `FungalGrowthSearchDummy` (`liberty :736–741`, a damage with no `Amount`, `Snapshot`), `FungalGrowthDamage` (`liberty :603–607`, `Snapshot`, run by the behaviour's pulse) | A point-aimed launch; a dummy damage at the impact and periodic damage, all `Snapshot` [DATA] |
| S7 | Abduct: `YoinkLaunchMissile` (`swarm :1486–1494`, no `Visibility`, aimed at the target unit) and `YoinkLaunchTarget` (`:1554–1565`, a `CEffectLaunchMissile` whose *launched* unit is the target, `ImpactLocation` the placeholder point) | The tongue's launch snapshots the Viper to the target's owner; the pull is aimed at a point [DATA + INF] |
| S8 | `AttackReveal`, `core BehaviorData:237–240`: `CBehaviorReveal`, `Duration 3` (Normal s = 54 ticks at our rate, as B-55 built). No data row links it to `Visibility`; the engine applies it for `Visible` | `Visible` reveals the caster, and a revealed unit is a target, whatever effect class carries it [DATA for the behaviour, INF for the link, as plan `2026-11-23-vision-and-detection` §2 established] |
| S9 | Hive Workshop wiki, *Visibility (CEffectDamage)* (search abstract, 2026-11-25; the page answered HTTP 502): the field "controls how the visibility of the damage source is modified with respect to the target"; `Hidden`: "the damage source remains hidden from the target"; `Snapshot`: "the location of the damage source is revealed to the target and a snapshot appears …, but the source still may not be targeted"; `Visible`: "fully revealed … and can be seen and targeted" | The three values' meaning, and that the audience is the target's player [EXT] |
| S10 | SC2Mapster, *StarCraft 1 style attack up cliffs* (sc2mapster.com/forums/resources/tutorials/179756, search abstract): to make a Hydralisk's attack up a cliff reveal it, the tutorial changes the *damage* effect's `Visibility` from `Snapshot` to `Visible` | The damage effect, not the launch, is what makes a missile attacker targetable [EXT]; the launch's own field is not discussed |
| S11 | `GravitonBeamDummy`, `VortexDummy`, `PointDefenseLaserDummy` (`liberty :801`, `:1525`, `:1222`): damage effects with no `Amount` that author `Visible` | Blizzard sets `Visibility` on zero-amount dummies, which only makes sense if a dummy damage applies it: **a zero damage still counts** [INF, strong] |

Plan [`2026-11-24-attacker-snapshot`](2026-11-24-attacker-snapshot.md) §2 (S5 to S10 there) remains the source for
what a snapshot is (a dimmed, still image, never a target, no vision) and how long it lasts in the game.

### 2.2 The rules that follow

1. **A missile weapon applies `Visibility` twice** [DATA, S3, S2]. At the release, the launch effect's value (always
   `Snapshot` for a weapon, S4); at the impact, the damage effect's value (`Snapshot` for ground weapons, `Visible`
   for air-plane attackers). An instant weapon has no launch effect, so only the damage applies.
2. **Who sees what, and when, at a launch** [DATA for the value, INF for the audience, **verify**]. When the missile
   leaves, the owner of the unit it is aimed at, if that player does not already see the attacker, gets a snapshot
   of the attacker in its release pose. The impact a flight time later refreshes that image (`Snapshot`) or reveals
   the live attacker (`Visible`). A missile that never lands (its target lost, B-54) still left the launch's image.
   The audience is the launch's target player, by the field's meaning (S9: "with respect to the target") and by
   symmetry with the damage effect; no source names it for the launch.
3. **A point-aimed launch** has no target unit, so no player receives its image [INF, **verify**, Q0128 part 2]. The
   only shipped one is Fungal Growth (S6), whose impact damage snapshots right after. Abduct's pull (S7) is aimed at a
   point too.
4. **Spell damage** follows S2 exactly as weapon damage does: the default `Snapshot` leaves the struck player an image
   of the caster; `Hidden` shows nothing; `Visible` reveals the caster for `AttackReveal`'s 3 s, and a revealed caster
   is a target for that player (S8, S9) [DATA + INF]. In the shipped catalog: Psionic Storm's first hit `Snapshot`,
   its pulses `Hidden` (S5); Fungal Growth's dummy and pulses `Snapshot` (S6). No shipped spell is `Visible`; in 5.0.14
   the `Visible` spells are Seeker Missile, Yamato, the Raven's anti-armour missile, Graviton Beam and Vortex (S4),
   none of which Space Scuffle ships.
5. **The caster** is the unit that cast the tree (the effect context's caster) for every node of it, including the
   nodes a status runs on its pulses: Fungal Growth's periodic damage snapshots the Infestor [INF from the context
   chain, **verify**]. A caster that has died, or is hidden (loaded), shows nothing, as a dead weapon attacker does.
6. **A zero damage counts** (S11): Fungal Growth's dummy, which deals nothing, still snapshots. As for weapons, a
   damage reduced to nothing by a barrier, shields or armour counts.
7. **Cloak and burrow** [INF, owner rule]. The game sources are silent. Q0112 3(a) chose for weapon strikes that a
   concealed attacker leaves an image only where the struck player detects it at that moment; the same rule applies
   to a launch and to spell damage, so no image ever shows what the player could not see in plain sight. A `Visible`
   effect on a concealed caster raises the reveal counter as a weapon does; under Q0101 2(b)'s resolution the reveal
   counts as sight of a concealed unit only where its player detects it. So a revealed but undetected cloaked caster
   is not a target (the existing `is_cloak_revealed` rule, unchanged).
8. **Lifetime.** 5.0.14 keeps an image until the spot or the unit is seen; the owner capped it at 1 s after its last
   refresh (Q0112 2(c)). The cap is a rule of the image, not of its cause, so it applies to launch and spell images
   alike (Q0128 part 3, default).

### 2.3 Findings and confidence

| Finding | Confidence | Basis |
|---|---|---|
| Every weapon missile launch in 5.0.14 is `Snapshot` (default, never overridden) | High | S3, S4 [DATA] |
| A launch's image goes to the aimed-at unit's owner, at the release | Medium, **verify** | S9 (the audience is "the target") and symmetry with S2; no source for the launch itself |
| An air-plane missile attacker shows a launch image before its impact reveal | Medium | Follows from S3 + S4 (its launch is not overridden, its damage is `Visible`) [INF] |
| A point-aimed launch shows nothing | Low-medium, **verify** | No target player exists [INF] |
| Spell damage defaults to `Snapshot`; Psionic Storm's pulses are `Hidden`, its first hit `Snapshot` | High | S2, S5 [DATA] |
| No shipped spell or launch is `Visible` | High | S4, S5, S6, S7 against `ability_effects.ron` [DATA] |
| A `Visible` spell makes its caster targetable to the struck player for 3 s | High for the data, effect identical to B-55 | S8, S9 |
| A zero-amount damage applies its `Visibility` | Medium-high | S11 [INF] |
| A status pulse's damage snapshots the original caster | Medium, **verify** | The effect chain's caster [INF] |
| Concealed casters follow Q0112 3(a) | Owner rule extended, **verify** | No source; nothing hidden leaks |

---

## 3. Diagnosis (on `a0384b78`, read from the code)

1. **The type exists.** `sim_types::AttackVisibility` (`crates/sim_types/src/weapon.rs`) has `Snapshot` (serde
   default), `Visible { reveal_ticks: u16 }` and `Hidden`; `WeaponSpec::attack_visibility` carries the damage's value.
   Validation refuses `Visible { reveal_ticks: 0 }` (`catalog/validation.rs`, "a visible attack reveals its attacker
   for at least one tick"). The encoding is a tag and the ticks (`checksum/encoding.rs`, `impl Canonical for
   AttackVisibility`).
2. **The hook is bound to a weapon hit.** `record_attack_presentation(source: HitSource<'_>, victim_id)`
   (`crates/sim_core/src/combat_resolution.rs`) calls `record_attack_reveal` and `record_attack_snapshot`, and both
   read `source.weapon.attack_visibility`, `source.attacker` and `source.owner`, nothing else of the weapon. It is
   called from `apply_weapon_hit` and `strike_area`.
3. **The launch has its own place.** `launch_projectile(shot, speed)` builds the `ActiveProjectile` on the release
   tick in the combat phase (phase 7), with `shot.attacker`, `shot.owner`, `shot.target` and `shot.weapon` at hand. It
   records nothing of visibility. `MissileValidation` (`weapon.rs`) already models two other `CEffectLaunchMissile`
   fields, and validation refuses a non-default `missile_validation` on an instant weapon.
4. **Spell damage has its own place.** `EffectNode::Damage { amount, attribute_bonus, responses }`
   (`catalog/spells.rs`) is run by `enter_node` (`effects/eval.rs`) through `apply_spell_damage(unit, amount, bonus,
   responses, context)` (`effects/actions.rs`), which deals the damage and then raises the responses against
   `context.caster`. `EffectContext` carries `owner` (the caster's owner at the cast) and `caster`. Nothing is
   recorded of visibility. `DamageDef` (`sim_types/src/effects.rs`) is `deny_unknown_fields`.
5. **Spell launches have their own place.** `launch_missile(id, to, context)` (`effects/eval.rs`) resolves the target
   (`EffectTarget::Unit` for `TargetUnit`, `Point` for `TargetPoint`) and pushes an `ActiveMissile`.
6. **When spells run.** Effects step in phase 2 (`update_production_and_buffs` → `step_effects`), and a cast commits
   in phase 1 or when an approaching caster arrives (`abilities.rs` `commit_cast`); vision is phase 3. A spell image
   recorded before phase 3 is tested against the previous tick's vision maps, and the same tick's phase-3 prune tests
   it again against this tick's (D-2).
7. **The spell catalog is not hashed.** `checksum.rs`'s module doc: "Catalogs must match separately"; effect state
   hashes node and status ids, contexts and timing (schema 28), never a node's fields. `WeaponSpec` is hashed in full
   with every projectile (`fields!(ActiveProjectile { …, weapon_spec, … })`), and every behaviour field added to it
   since schema 38 was hashed with a schema step (38, 40, 41, 43, 45, 49).
8. **The image and its end are cause-free.** `AttackSnapshotRecord { viewer, attacker, struck, pose }`
   (`world/presentation.rs`) and `prune_attack_snapshots` (`world/vision.rs`: seen cell, `snapshot_attacker_shown`,
   the cap `attack_snapshot_cap_ticks`) do not ask what made the image. `PlayerView::attack_snapshots` and the client
   pool (`battle_hud/attack_snapshots.rs`) draw whatever the record lists.
9. **The reveal path is cause-free.** `Entity::attack_reveal_ticks` (hashed, schema 41) is counted down in phase 3
   and read by `sees_enemy` through `is_attack_revealed_to`. Acquisition reads it there; nothing in its admission
   test needs to change for a reveal of another cause.

---

## 4. Design decisions

### D-1. Where the field lives: three uses of one type, engine defaults

| Field | Type, default | Meaning | Validation |
|---|---|---|---|
| `WeaponSpec::launch_visibility` (`sim_types/src/weapon.rs`, after `missile_validation`) | `AttackVisibility`, `#[serde(default)]` = `Snapshot` | `CEffectLaunchMissile` `Visibility` (S3): what the weapon's missile release shows of its attacker to the aimed-at unit's owner | `Visible { reveal_ticks: 0 }` refused; a non-default value on a weapon without `projectile_speed` refused ("an instant weapon launches no missile"), as `missile_validation` is |
| `DamageDef::visibility` (`sim_types/src/effects.rs`) | `AttackVisibility`, `#[serde(default)]` = `Snapshot` | `CEffectDamage` `Visibility` (S2): what the damage shows of the caster to the struck unit's owner | `Visible { reveal_ticks: 0 }` refused, naming the node |
| `LaunchMissileDef::visibility` (`sim_types/src/effects.rs`) | `AttackVisibility`, `#[serde(default)]` = `Snapshot` | `CEffectLaunchMissile` `Visibility` (S3) for a spell missile | As above |

The resolved nodes gain the same field: `EffectNode::Damage { …, visibility }`, `EffectNode::LaunchMissile { …,
visibility }` (`catalog/spells.rs`).

**Why one type.** The game has one field with three values in both classes; `AttackVisibility` already carries its
values, defaults, reveal duration, validation and encoding. Its doc becomes "What an effect shows of its caster to the
player it is aimed at or strikes: `StarCraft` II's `Visibility` of `CEffectDamage` and `CEffectLaunchMissile`". The
name stays (renaming a public type crosses `client` and every fixture for no gain); the doc names both uses.

**Why not inside `MissileValidation`.** That struct is the launch's *flight checks*; visibility is not a check, and
mixing them would make `MissileValidation::default()` mean two things. A sibling field is plainer.

**Per-row edits.** Every default matches 5.0.14 for the shipped catalog (S4), save one: `psionic_storm_damage`
authors `visibility: Hidden` with its citation (`liberty EffectData:2099`). No weapon row and no other spell row
changes. The census of WP0 confirms, row by row, that each of the 25 shipped missile weapons (21 unit weapons,
4 structure weapons) is a `CEffectLaunchMissile` in 5.0.14 and each instant weapon is not (§2.2 rule 1). A mismatch
is not fixed here (a weapon's delivery moves battle outcomes); it is registered as **B-227** (conditional, §14) and
the row keeps the default.

### D-2. The rule: one hook, three new callers

`record_attack_presentation` takes an **attack cause** instead of a weapon hit:

```rust
/// What an effect that lands on, or is aimed at, a unit shows of its caster (plan
/// `2026-11-27-launch-and-spell-visibility` D-2): the effect's visibility, the caster, and the caster's owner.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AttackCause {
    pub(crate) visibility: AttackVisibility,
    pub(crate) attacker: Option<EntityId>,
    pub(crate) owner: PlayerId,
}

impl AttackCause {
    /// The damage of a weapon hit: `source.weapon.attack_visibility`.
    pub(crate) fn strike(source: HitSource<'_>) -> Self;
    /// A weapon missile's release: `shot.weapon.launch_visibility`.
    pub(crate) fn launch(shot: &PreparedShot) -> Self;
    /// A spell node's visibility, with the context's caster and owner.
    pub(crate) fn spell(visibility: AttackVisibility, context: &EffectContext) -> Self;
}

fn record_attack_presentation(&mut self, cause: AttackCause, victim_id: EntityId);
```

`record_attack_reveal` and `record_attack_snapshot` read `cause.visibility`, `cause.attacker` and `cause.owner`
instead of the weapon; their conditions are otherwise unchanged (victim of another player; attacker alive and, for the
snapshot, not hidden; not already `shown_to` the viewer; a concealed attacker only where detected; one record per
viewer and attacker, refreshed; the reveal counter raised, never lowered). The two existing callers pass
`AttackCause::strike(source)`; their behaviour is byte-identical (WP2's unit test proves the refactor alone moves
nothing).

**New callers:**

1. **Weapon launch** (`launch_projectile`, after the `ActiveProjectile` is pushed):
   `self.record_attack_presentation(AttackCause::launch(&shot), shot.target)`. The victim is the aimed-at unit; if it
   no longer exists, nothing is recorded (the existing lookups return early).
2. **Spell damage** (`apply_spell_damage`, after the damage and before the responses, as `apply_weapon_hit` records
   before retaliation): `self.record_attack_presentation(AttackCause::spell(visibility, context), unit)`. It runs
   whatever the amount, including 0 (§2.2 rule 6). `apply_spell_damage` gains the `visibility` parameter from the
   node.
3. **Spell launch** (`launch_missile`, after the `ActiveMissile` is pushed), only when the resolved target is
   `EffectTarget::Unit(unit)`: `self.record_attack_presentation(AttackCause::spell(visibility, &context), unit)`. A
   point target records nothing (§2.2 rule 3). `Displace` (Abduct's pull) is not a launch of this kind and records
   nothing (§14).

**Phase order.** A weapon launch runs in phase 7, as strikes do: the image is listed in the view of the same tick and
pruned from phase 3 of the next. A spell's damage or launch may run in phase 1 or 2 (or when a walking caster arrives
in range, in movement); its `shown_to` test reads the vision maps of the previous tick's phase 3, and phase 3 of the
same tick prunes it at once if the viewer now sees the spot or the caster. So the view lists a spell image only if,
after this tick's vision, the viewer still sees neither. A `Visible` spell's counter raised before phase 3 is counted
down in the same tick's phase 3, so it stays positive for the same number of acquisition phases as a weapon's raised
in phase 7, starting one tick earlier; WP2's test V-14 states the exact ticks.

### D-3. The image: the record, the cap and the view, unchanged

A launch or spell image is an `AttackSnapshotRecord` like any other: `struck` is the tick of the event that made or
last refreshed it (its doc and `AttackSnapshotView::struck`'s doc say "the tick of the strike, launch or spell damage
the image is from"); the pose is the caster's at that event. The cap `attack_snapshot_cap_ticks` (25 shipped) runs from
`struck`, so a launch image whose missile flies longer than the cap lapses and the impact makes it again (§2.2
rule 8). The record has no cause field and the view publishes none: nothing reads one, and a minimap (B-204) draws an
image the same way whatever made it. If an image of the same caster is already held, a launch, strike or spell damage
refreshes it.

### D-4. Concealment and the audience: reused

Both follow the existing rules with no new code: a concealed caster leaves an image only where the viewer detects its
cell (Q0112 3(a)); a `Visible` cause raises `attack_reveal_ticks`, which makes a concealed caster a target only where
detected (`is_cloak_revealed`, Q0101 2(b) resolution). The audience is the struck (or aimed-at) unit's owner and its
allies (Q0101 4(b)); the simulation has no teams, so the owner alone. A spell striking the caster's own player's units
(Psionic Storm) records nothing for that player.

### D-5. What is hashed, and the schema

| Value | Hashed? | Why |
|---|---|---|
| `WeaponSpec::launch_visibility` | **Yes**, after `missile_validation`: the tag and, for `Visible`, the ticks (`AttackVisibility`'s encoding). Schema **53** provisional | A `Visible` launch writes `attack_reveal_ticks`, which acquisition reads: the field is decision-bearing. `WeaponSpec` is hashed in full with each projectile, and every behaviour field since schema 38 rode it with a schema step (§3 item 7); leaving one out would need a soundness argument the macro's rule ("its declaration states why that is sound") cannot give for a field a tick decision reads |
| `DamageDef::visibility`, `LaunchMissileDef::visibility` | No (catalog data) | The spell catalog is not hashed (§3 item 7); peers must match catalogs separately. Their effect on hashed state, a `Visible` reveal, lands in `Entity::attack_reveal_ticks`, already hashed |
| Images (`AttackSnapshotRecord`) | No | `PresentationWorkspace`, read by no tick decision (attacker-snapshot plan D-1; its P-9 test extended in WP2) |
| `Entity::attack_reveal_ticks` | Already hashed (schema 41) | Unchanged encoding; in shipped data no new cause raises it |

**Rejected:** leaving `launch_visibility` out of the encoding to avoid the re-record. Sound only if no tick decision
read it, which is false for `Visible`; and restricting the field to `Snapshot`/`Hidden` to make it presentation-only
would refuse data 5.0.14 has (the five `Visible` spell launches, S4) in the weapon's twin field.

`checksum.rs` gains: "Schema 53 adds each weapon's launch visibility (0 snapshot, 1 visible with its reveal ticks, 2
hidden), `WeaponSpec::launch_visibility`, after `missile_validation` (plan `2026-11-27-launch-and-spell-visibility`
D-5). The number is the next free at merge."

### D-6. The client: nothing new

The pool draws whatever `PlayerView::attack_snapshots` lists, with the `attack_snapshot` look, never as a
`LiveBattleBubble`. A launch or spell image is the same record, so no client code changes. WP3 adds tests only: a
spell-caster image and a launch image are drawn by the pool (no other entity kind), never picked, and a held image
allocates nothing in steady state. If WP3 finds that client code must change, that is stop rule 7 (the design says it
need not), and the orchestrator decides with an amendment.

### D-7. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | B-205 and B-206 restated with §2's sources; status "open; plan `2026-11-27-launch-and-spell-visibility`". B-227 registered only if WP0's census finds a delivery mismatch | WP0 |
| `DESIGN.md` §4 | B-205 and B-206 **fixed**, with commits, tests and evidence. The "Combat" row names the launch image and spell visibility beside the snapshot and reveal | WP4 |
| `docs/plans/2026-11-24-attacker-snapshot.md` §14 | B-205 and B-206 bullets gain "*Taken up by plan `2026-11-27-launch-and-spell-visibility`.*" | WP4 |
| New `docs/evidence/2026-11-27-launch-and-spell-visibility.md` | Census, probes, close-battle list, baseline on four builds, latent proof, worlds, held-out, goldens, bench | WP0 creates; WP1 to WP4 append |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Done` (WP4) | plan, WP1, WP4 |

---

## 5. Determinism, checksum and coordination

- **Integers only.** No float enters: the cause is an enum, ids and a player; poses are the existing fixed-point
  `SnapshotPose`. C7's allowances do not grow.
- **Order.** Records are made in the order the existing walks run (entity order in combat, depth-first in the effect
  walk); the record list is sorted by `(viewer, attacker)`, so insertion order never shows.
- **No new collection, randomness or clock.**
- **Latent proof (WP1).** With `launch_visibility` left out of the encoding and the schema at `main`'s number, the
  tree reproduces every recorded checkpoint of both goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release
  and `wasm32-wasip1` debug and release (`~/scratch/turret-tracking/scripts/proof.sh` and `goldens.sh`, copied to
  `~/scratch/launch-spell-visibility/scripts/`). Then the field is encoded at schema 53. WP1 to WP3 do not re-record;
  their golden tests fail by design, and each hand-back says so.
- **One re-record (WP4, final tree after merging `main`).** `golden_replay.ron`, `golden_replay_orders.ron` and
  `DEFAULT_SANDBOX_CHECKSUM` (`crates/sim_fixtures/tests/fixture_tests.rs`), under the header "Re-recorded for plan
  2026-11-27-launch-and-spell-visibility (checksum schema N -> 53): each weapon encodes its launch visibility; with the
  field left out at schema N every checkpoint reproduced. Behaviour: none." Proved on the four builds. If `main` has
  taken 53 first, the merge repeats the re-record with the next free number.
- **Throughput.** `cargo bench -p sim_core --bench throughput`, release, on WP0's tree and the final tree: every
  scenario at **50 ticks per second or more**; **instructions within +0.5 %** per scenario (`perf stat -e
  instructions:u`, `DESIGN.md` §5's method). The cost is one call per missile release, per spell damage and per
  unit-aimed spell launch, each an early return unless the viewer does not see the caster. Above budget is stop rule 5.
- **Gated coordination.** This plan edits nothing under `crates/sim_core/src/movement/`, nor `steering.rs`,
  `movement_rules.ron` or `target_acquisition.rs` (acquisition's admission test included). `shown_to`,
  `sees_enemy` and `is_cloak_revealed` in `target_acquisition.rs` are **read, not edited**. A `Visible` cause changes
  targetability only by raising `Entity::attack_reveal_ticks` in `combat_resolution.rs`'s `record_attack_reveal`,
  the path B-55 built; acquisition sees it through `sees_enemy`'s existing read of `is_attack_revealed_to`. No conflict
  with an open Gated package arises. If WP2 finds it must touch any of those files, that is stop rule 6.

---

## 6. Acceptance (fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` (a plateau with no ramp in
reach, as `attack_snapshot_tests.rs` stages it) and use shipped units from `sim_fixtures`, found by property
(`projectile_speed`, `attack_visibility`, an ability whose effect tree holds a `Damage` or `LaunchMissile`), never by
slug, except the shipped-values table (V-19). A scratch catalog copy supplies values the shipped data lacks
(`Hidden` or `Visible` launches, a `Visible` spell). Unless a test is about the cap, it runs with the cap lifted
(`UnitCatalog::set_attack_snapshot_cap`, as P-5 to P-8 do).

### 6.1 Weapon launch (WP2, new `crates/sim_core/tests/launch_and_spell_visibility_tests.rs`)

- **V-1 (launch image).** A missile weapon on the plateau fires at p's unit below, at a distance whose flight is at
  least 3 ticks. On the release tick, p's `attack_snapshots` lists the attacker once, at its release position and
  heading, `struck` = the release tick, while the victim's life and shields are unchanged. On the impact tick the
  entry is refreshed (`struck` = impact tick).
- **V-2 (instant weapon).** An instant `Snapshot` weapon in the same stage: the first entry appears on the strike tick,
  exactly as on `main` (the attacker-snapshot tests stay green unchanged).
- **V-3 (not when shown).** With a unit of p seeing the attacker's cell, a launch records nothing.
- **V-4 (missile lost).** The target dies, or is loaded, between release and impact: the launch's entry is listed
  from the release tick and ends by the usual rules.
- **V-5 (`Hidden` launch).** A scratch copy with `launch_visibility: Hidden`: nothing on the release tick; the impact
  records as before.
- **V-6 (`Visible` launch).** A scratch copy with `launch_visibility: Visible(reveal_ticks: 54)` on a ground missile
  weapon: from the release tick, `is_attack_revealed_to(p)` holds, and a unit of p in reach with no other enemy
  acquires and fires at the attacker on the next acquisition phase; the counter lasts as a strike's would. Without the
  scratch value (shipped), the same unit never acquires it.
- **V-7 (air-plane missile attacker).** A shipped missile weapon whose `attack_visibility` is `Visible`: an image from
  the release, then the reveal at the impact; no view ever lists the attacker both as revealed (or in sight) and as an
  image (the R-7 rule of the attacker-snapshot plan).
- **V-8 (concealed).** A cloaked missile attacker, undetected by p: no image at the launch. Inside p's detection
  circle but outside its sight: an image.
- **V-9 (cap).** Under the shipped cap, a launch image with no later strike is gone exactly `cap` ticks after the
  release; with a flight longer than the cap (scratch slow missile), it lapses and the impact records it again.

### 6.2 Spell damage and spell launches (WP2, same file)

- **V-10 (Psionic Storm).** A caster p does not see storms p's units: on the tick the storm's first search applies its
  status, p lists the caster once; the pulses (`Hidden`) never refresh it, so under the shipped cap it is gone 25
  ticks after the first hit even while the storm still pulses. A unit of p walking into the storm later (its own first
  hit) refreshes it.
- **V-11 (Fungal Growth).** The impact's zero-damage dummy lists the caster on the impact tick; the periodic damage
  refreshes it on each pulse; the launch tick lists nothing (point target).
- **V-12 (Abduct).** The tongue's launch lists the caster for the target's owner on the launch tick.
- **V-13 (owner's own units).** A storm on the caster's own units records nothing for the caster's player.
- **V-14 (`Visible` spell).** A scratch copy whose storm first hit authors `visibility: Visible(reveal_ticks: 54)`:
  from the hit's tick the caster is attack-revealed to p, and a unit of p in reach acquires it; the counter is positive
  on exactly the acquisition phases D-2 states. A scratch `Visible` cast on a cloaked caster that p does not detect
  reveals nothing targetable (V-6's unit does not acquire it).
- **V-15 (caster gone).** The caster dies (or is loaded) after the cast and before a damage: no image, no reveal.
- **V-16 (only the struck player).** A third player whose units the spell missed lists nothing; nobody's `revealed`
  or targetable set gains the caster under shipped data.

### 6.3 Data, validation, encoding, determinism

- **V-17 (validation; WP1, `catalog/validation.rs` and `catalog/spells/tests.rs`).** `launch_visibility` non-default on
  an instant weapon, `Visible(reveal_ticks: 0)` in any of the three fields: each refused with a typed error naming the
  field and the definition. Omitted fields read `Snapshot`.
- **V-18 (encoding; WP1, `checksum_state_tests.rs`).** A projectile whose weapon's `launch_visibility` differs changes
  the checksum; the spell nodes' field does not (it is not encoded).
- **V-19 (shipped values; WP1, `weapon_catalog_rules_tests.rs` and `catalog_reference_values_tests.rs`).** Every
  shipped weapon omits `launch_visibility`; `psionic_storm_damage` is `Hidden`, every other shipped `Damage` and
  `LaunchMissile` node `Snapshot`; each with its citation.
- **V-20 (zero recompilation; WP2, `runtime_catalog_tests.rs`).** In a catalog directory copy, setting
  `psionic_storm_damage_initial`'s `visibility: Hidden` removes V-10's image in the same binary.
- **V-21 (refactor neutral; WP2 unit test in `combat_resolution.rs`).** For each `AttackVisibility` and each condition
  of the record rule, `AttackCause::strike(source)` records what the old `HitSource` path recorded.
- **V-22 (no tick decision reads images; WP2, extends the attacker-snapshot P-9).** Two worlds run a storm, a Fungal
  and a missile duel; one clears every image after each tick: checksums agree on every tick.

### 6.4 Client (WP3)

- **V-23 (drawn by the pool).** In a real skirmish session under the `Player` profile (the stage of
  `crates/client/tests/support/attack_snapshot_stage.rs`, with a missile attacker), the launch image is one visible
  `AttackSnapshotBubble` on the release tick's frame, before any hit flash on the victim. No other entity kind is
  spawned (the pool's entity count equals the view's list length).
- **V-24 (never picked).** A click on that image selects nothing; a right-click submits a point order.
- **V-25 (steady state).** With a launch or spell image held and the list unchanged, `sync_steady_state_tests` sees no
  new entity, asset event or buffer growth, and `sync_allocation_count_tests` (valgrind) counts no allocation in the
  live battle sync.

### 6.5 World check (pre-registered)

Every world that differs from WP0's baseline is explained by its **first difference** (`firstdiff.py` and a per-entity
trace) and classed:

- **S (schema bytes).** The goldens and `DEFAULT_SANDBOX_CHECKSUM` differ by the encoding alone: with the field left
  out (WP1's proof) every checkpoint reproduces.
- **P (presentation).** A `PlayerView` or the showcase's fog frames gain images (probes (l), (s), (k)); nothing a
  measured output records.
- **R (reveal).** A reveal counter differs first, from a `Visible` launch or spell. **No shipped value is `Visible`
  (§2.3), so class R is impossible in shipped data; any R first difference is stop rule 2.**
- **Anything else** (a target, order, position, damage, RNG draw) first: a defect, stop rule 2.

**Close battles.** WP0 fixes the close-battle list from its baseline alone, by the rule of the hold-facing plan
§6.4 (as the turret-tracking plan §6.4 copies it): every matrix battle whose minority side wins at least 2 of its 20
baseline runs; battles 31 and 33 at the same share over their 100 and 64 seeds; every battle a closed owner question
or fixture comment names close or restated; no other; a `known_bug` battle is not close. The list is not edited after
WP1 starts. A close battle whose outcome moves through class R would be restated under charter §8 with the first
differing tick of one flipped seed traced; since class R cannot occur in shipped data, **any outcome move at all,
close or not, is stop rule 4**.

### 6.6 Predictions (recorded before measuring, not thresholds)

| World | WP1 (latent) | WP2 (rule) |
|---|---|---|
| Matrix (seeds of record and 16 development seeds) | byte-identical | byte-identical; probes count P only: battles with missile weapons (most), 13 and 36a (storm) |
| Battles 31, 33 | byte-identical | byte-identical |
| Showcase, showcase effects (default and 1–8) | byte-identical | byte-identical in measured outputs; the `battle` take under the player's fog gains images (P), listed in the evidence, not re-recorded (CI renders the video) |
| Goldens, sandbox | S, proved, not re-recorded | S; re-recorded once in WP4 |
| Client skirmish stage log | identical | identical |

---

## 7. Measurement

### 7.1 WP0 census and probes (scratch, not committed)

- **(c) Census.** For each shipped weapon: its 5.0.14 weapon's `Effect`, whether that is (or begins with) a
  `CEffectLaunchMissile`, against our `projectile_speed`; mismatches go to B-227 (§14). For each shipped spell node of
  kind `Damage` or `LaunchMissile`: its 5.0.14 effect and `Visibility` (§2.1 S5 to S7), as V-19's table.
- **A scratch probe patch** on WP0's tree (`~/scratch/launch-spell-visibility/probe/`, with its diff; it may name
  slugs, being scratch) logs, per world and seed, without changing any state:
  - **(l) launch images:** releases where the aimed-at unit's owner p does not have the attacker `shown_to` and the
    concealment rule admits it; split into *new* (no image of that attacker held for p) and *refresh*; and the ticks
    gained (impact tick − release tick) for new ones;
  - **(a) air-plane launch images:** the subset of (l) whose weapon's `attack_visibility` is `Visible`;
  - **(s) spell images:** spell `Damage` executions by node, with the node's 5.0.14 visibility, where the caster is
    alive, unhidden and not shown to the struck unit's owner;
  - **(k) spell launch images:** unit-aimed spell launches under the same test;
  - **(v) reveals:** causes with `Visible` other than weapon damage. **Predicted zero everywhere.** A nonzero count
    stops WP0 (the census is wrong).
- **Baseline** on four builds: both goldens' checkpoints and `DEFAULT_SANDBOX_CHECKSUM`.
- **Throughput and instructions** on WP0's tree (§5).

### 7.2 Commands

The turret-tracking plan's scripts, copied to `~/scratch/launch-spell-visibility/scripts/` (`measure.sh`,
`compare.sh`, `firstdiff.py`, `proof.sh`, `heldout.sh`, `bench.sh`) with their paths changed:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/launch-spell-visibility-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/launch-spell-visibility
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-<prev> $R/world-<wp>
```

One at a time across worktrees, in the background, never beside a `cargo gate` or a workspace `cargo test`
(charter §7).

### 7.3 Held-out seeds (WP4, confirmation only)

Once, on WP0's tree and the final tree: matrix seeds 7001 to 7016, battles 31 and 33 on 7001 to 7064, the showcase
and showcase effects on seeds 9 to 16. Byte-identical expected; any difference is stop rule 3. Nothing is restated or
tuned from them.

---

## 8. Stop rules

1. WP1's latent proof fails (a checkpoint differs with the field left out): stop; something other than the encoding
   moved.
2. A measured world's first difference is class R or anything other than S or P (§6.5): stop, trace, report.
3. A held-out seed differs: stop and report.
4. Any battle outcome moves: stop for review, with the trace. Shipped data cannot move one (§6.5).
5. Throughput below 50 ticks/s in any scenario, or instructions above +0.5 %: stop and report.
6. Anything under `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`
   would need an edit (the Gated coordination rule): stop and report to the orchestrator.
7. The client needs a code change (D-6), or a steady-state or allocation test fails: stop; the image is not the same
   record.
8. V-21 fails (the refactor alone moves a record): stop; fix the refactor, never the test.
9. Probe (v) counts a reveal in shipped data: stop WP0; the census or §2's survey is wrong.

---

## 9. Client consequences

None in code (D-6). `battle_fog_tests.rs`, `sync_steady_state_tests.rs` and `sync_allocation_count_tests.rs` gain
cases; the stage support file gains a missile-attacker variant and a spell-caster variant (a storm cast from a cliff
the viewer does not see). No GPU test is added: the look, pool and shader are those `attack_snapshot_gpu_tests.rs`
already proves, and it runs in WP3's package gate through its catalogue triggers.

---

## 10. File ownership

| File | Change | WP |
|---|---|---|
| `DESIGN.md` | §4 rows (D-7) | WP0, WP4 |
| `docs/evidence/2026-11-27-launch-and-spell-visibility.md` | New | WP0 to WP4 |
| `crates/sim_types/src/weapon.rs` | `WeaponSpec::launch_visibility`; `AttackVisibility` doc; unit tests (default, RON forms) | WP1 |
| `crates/sim_types/src/effects.rs` | `DamageDef::visibility`, `LaunchMissileDef::visibility`; module doc invariant ("every `Damage` and `LaunchMissile` node carries its `Visibility`") | WP1 |
| `crates/sim_types/src/player_view.rs` | `AttackSnapshotView` and `attack_snapshots` docs: the image's cause may be a launch or spell | WP2 |
| `crates/sim_core/src/catalog/spells.rs` | Resolved nodes carry `visibility`; validation | WP1 |
| `crates/sim_core/src/catalog/validation.rs` | `launch_visibility` rules | WP1 |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | Schema 53 line; `launch_visibility` after `missile_validation` | WP1 |
| `assets/catalog/ability_effects.ron` | `psionic_storm_damage`: `visibility: Hidden` with citation; header comment names the field and its default | WP1 |
| `assets/catalog/units.ron` | Header comment: `launch_visibility` and its default (no row edit) | WP1 |
| `crates/sim_core/src/combat_resolution.rs` | `AttackCause`; `record_attack_presentation`, `record_attack_reveal`, `record_attack_snapshot` read it; the launch call in `launch_projectile`; module doc | WP2 |
| `crates/sim_core/src/effects/actions.rs`, `effects/eval.rs` | The spell damage and unit-aimed spell launch calls; module docs | WP2 |
| `crates/sim_core/src/world/presentation.rs` | `AttackSnapshotRecord::struck` doc | WP2 |
| `crates/sim_core/tests/launch_and_spell_visibility_tests.rs` | New: V-1 to V-16, V-22 | WP2 |
| `crates/sim_core/tests/{checksum_state_tests,weapon_catalog_rules_tests,catalog_reference_values_tests,runtime_catalog_tests}.rs` | V-18, V-19, V-20 | WP1, WP2 |
| `crates/sim_core/src/catalog/spells/tests.rs` | V-17 for the spell nodes | WP1 |
| `crates/client/tests/{battle_fog_tests,sync_steady_state_tests,sync_allocation_count_tests}.rs`, `tests/support/attack_snapshot_stage.rs` | V-23 to V-25 | WP3 |
| `crates/gate/suites.ron` | Catalogue the new test file with the vision tests' class; triggers `effects/`, `combat_resolution.rs`, `ability_effects.ron` | WP2 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures/tests/fixture_tests.rs` | Re-recorded once (§5) | WP4 |
| `docs/plans/2026-11-24-attacker-snapshot.md` | §14 pointers | WP4 |
| `docs/plans/README.md` | Row | plan, WP1, WP4 |

**Not touched:** `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron`, `target_acquisition.rs`,
`world/vision.rs` (the prune is cause-free), every `client/src` file, `effects.ron`, `structures.ron` rows.

---

## 11. Work packages (commit order)

Each package is red first: its tests are written and fail on the previous package's tree, then the code makes them
pass. The implementation agent hands back after `cargo gate quick` and the tests its change needs; a verifier runs
`cargo gate package` on the final tree after merging `main` (Q0061, `docs/guides/test-gates.md`). Builds go to
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`; logs, probes and proofs go under
`~/scratch/launch-spell-visibility/`. GPU tests lock the GPU themselves; `cargo test` is never wrapped in
`with-gpu-lock`.

### WP0: records, census, probes, baseline (XS, docs and scratch)

D-7's WP0 rows; §7.1's census, probes (l), (a), (s), (k), (v), baseline on four builds, close-battle list, throughput
and instructions; B-227 registered only if the census finds a mismatch. **Acceptance:** probe (v) is zero (stop rule
9); the evidence lists every world's (l), (a), (s), (k) counts and the showcase ticks that will gain images. Commit
`docs(design): restate B-205 and B-206 (plan 2026-11-27-launch-and-spell-visibility WP0)`.

### WP1: the data, latent (S, `sim_types`, `sim_core` catalog, data)

D-1 and D-5. Red first: V-17, V-18, V-19. Then the fields, resolution, validation, the storm's `Hidden`, and the
encoding at schema 53 after §5's latent proof. No rule reads the fields. **Acceptance:** V-17 to V-19 pass; every
measured world byte-identical to WP0's; goldens differ by the schema only (proved, not re-recorded); stop rule 1
holds. Commit `feat(sim_types): launch and spell visibility as catalog data (B-205, B-206; latent)`.

### WP2: the rule (M, `sim_core`)

D-2 to D-4. Red first: V-1 to V-16, V-20 to V-22 (on WP1's tree every new image is missing; V-21 is written against
the old path and kept). Then `AttackCause`, the three calls, the docs. Worlds measured against WP1's (§6.5, §6.6).
**Acceptance:** V-1 to V-22 pass; every measured world byte-identical; stop rules 2, 4, 6, 8 hold. Commit
`feat(sim_core): a missile's launch and a spell's damage show their caster (B-205, B-206)`.

### WP3: client verification (S, `client` tests only)

D-6. V-23 to V-25 against WP2's tree. **Acceptance:** they pass with no `client/src` change (stop rule 7);
`attack_snapshot_gpu_tests.rs` and `smog_golden_tests.rs` unchanged in the package gate. Commit `test(client): launch
and spell images use the snapshot pool`.

### WP4: confirmation, re-record, records (S)

Held-out seeds (§7.3); throughput and instructions (§5); goldens and sandbox re-recorded once on the final tree after
merging `main`, proved on four builds; D-7's WP4 rows; `## As built`. **Acceptance:** stop rules 3 and 5 hold; B-205
and B-206 fixed. Commit `docs: close B-205 and B-206 (plan 2026-11-27-launch-and-spell-visibility WP4)`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Level | File | Covers |
|---|---|---|
| Unit | `sim_types` `weapon.rs`, `effects.rs` | Defaults and RON forms of the three fields |
| Unit | `sim_core` `catalog/spells/tests.rs`, `catalog/validation.rs` | V-17 |
| Unit | `sim_core` `combat_resolution.rs` | V-21: the cause refactor; each condition of the record rule for each cause |
| Integration | `crates/sim_core/tests/launch_and_spell_visibility_tests.rs` (new) | V-1 to V-16, V-22 through `SimWorld`, `PlayerCommand`, `write_player_view` |
| Integration | `checksum_state_tests.rs`, `weapon_catalog_rules_tests.rs`, `catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs` | V-18, V-19, V-20 |
| Determinism | goldens, sandbox, `sim_map` determinism on four builds | §5: latent proof, one re-record |
| Integration | `crates/client/tests/battle_fog_tests.rs` | V-23, V-24 |
| Steady state | `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` (valgrind) | V-25 |
| Bench | `cargo bench -p sim_core --bench throughput`; `perf stat` | ≥ 50 ticks/s, ≤ +0.5 % instructions |

---

## 13. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| R-1: the launch audience is not the aimed-at unit's owner in the game | Images go to the wrong player | §2.2 rule 2 marked **verify**; Q0128 part 1; a one-line change in the caller, presentation only |
| R-2: a spell image is made before phase 3 and shown a tick late or not at all | An image the player should see is pruned at once | D-2's phase rule; V-10 and V-11 state the ticks; the image is presentation |
| R-3: `apply_spell_damage`'s new call raises a reveal before the responses read it | A `Visible` spell's victim retaliates differently | Records before responses, as weapons record before retaliation; V-14 |
| R-4: storms and Fungal refresh images every pulse and churn the client pool | Steady-state failures | Refresh changes only `struck` and the pose; the pool restyles only on `(entity, struck)` change, as for repeated strikes today; V-25 |
| R-5: schema 53 is taken first | Merge conflict in `fields!` and the goldens | "Next free at merge"; WP4 re-records once on the final tree |
| R-6: a census mismatch (weapon delivery) tempts a data fix here | Outcomes move | Registered as B-227 and deferred (§14) |
| R-7: the extra call costs throughput in missile-heavy scenarios | Bench regression | Early returns; budget +0.5 %, stop rule 5 |

---

## 14. Out of scope and decisions recorded (Q0002)

- **B-227 (conditional, provisional).** "A shipped weapon's delivery differs from 5.0.14: <rows>; the launch image
  therefore appears where the game has none, or is missing where it has one." Filed in WP0 only if the census finds
  one; deferred, since a delivery change moves flight times and outcomes. *Taken up by plan
  [`2026-11-27-weapon-delivery`](2026-11-27-weapon-delivery.md).*
- **`Displace` (Abduct's pull).** 5.0.14's `YoinkLaunchTarget` is a `CEffectLaunchMissile` whose launched unit is the
  target and whose impact is a point (S7): it aims at no unit, so by §2.2 rule 3 it snapshots no one. Our `Displace`
  node gets no `visibility` field. The tongue's launch (V-12) already showed the Viper to the target's owner.
- **The image's cause in the view.** Not published: nothing reads it (D-3). A minimap (B-204) or a later look per cause
  would add it then.
- **Point-aimed launches** snapshot no one (§2.2 rule 3, Q0128 part 2). If the owner prefers that players whose units
  are near the point see the caster, it is a caller change in `launch_missile`.
- **Other effect classes.** 5.0.14 carries `Visibility` only on `CEffectDamage` and `CEffectLaunchMissile` (S4's survey
  finds no other class with the field), so `ApplyStatus`, `ModifyVitals`, `CancelOrders` and `Search` need none.
- **Spells not shipped** (Seeker Missile, Yamato, the Raven's missile, Graviton Beam, Vortex, the five `Visible`
  launches): the fields can express them; authoring them belongs to the plans that ship those spells.
- **Teams:** as Q0101 4(b), the record goes to the owner alone until teams exist.

---

## 15. Owner question (filed as [Q0128](../questions/closed/0128-launch-and-spell-images.md))

````markdown
# Q0128: Images of spell casters and missile launches: three details

- **Asked:** 2026-11-27
- **Blocking:** no
- **Default until answered:** 1(a), 2(a), 3(a), as built by plan `2026-11-27-launch-and-spell-visibility`

## Question

When a ground unit shoots your units from somewhere you cannot see, you get a dimmed, still image of it for up to
1 second (your answer to Q0112). In StarCraft II the same happens in two more cases, and plan
`2026-11-27-launch-and-spell-visibility` adds them: a unit that fires a missile at your unit leaves the image as soon
as the missile leaves, not only when it lands; and a spellcaster whose spell damages your units leaves an image of
the caster (Psionic Storm only on its first hit, since its later pulses show nothing; Fungal Growth on every pulse).
The game data settles these. Three details are not settled by any source I found.

**1. Who gets the image when a missile leaves.** The game's field applies "to the target".
- (a) The player whose unit the missile is aimed at. **Recommended**; it matches how the damage's image works.
- (b) Every player whose units the missile will hit (for a missile with splash).

**2. A spell missile aimed at the ground** (Fungal Growth flies to a point, not a unit).
- (a) Nobody gets an image when it leaves; the players it hits get one when it lands, a moment later.
  **Recommended**; no unit is aimed at.
- (b) Players with units near the point get one when it leaves.

**3. How long these images stay.**
- (a) As your Q0112 answer: until you see the spot or the caster, and at most 1 second after the last launch, hit or
  pulse that made it. **Recommended**: one rule for every image.
- (b) A different cap for spell casters (say how long).

Every option changes what you see only: no battle outcome, replay or targeting moves, because no shipped spell or
missile launch makes its caster targetable.

## Answer

````

---

## 16. Material (scratch, not committed)

- Game data `~/scratch/combat_wp3_data/` (`5444e12f13`): every `<Visibility` line of every layer's `EffectData.xml`
  mapped to its enclosing top-level effect with `awk` (§2.1 S4: 2 defaults, 51 damage overrides, 5 launch overrides);
  Psionic Storm `liberty :2069–2101`; Fungal Growth `liberty :603–607`, `:728–741`; Abduct `swarm :1472–1565`;
  `AttackReveal` `core BehaviorData:237–240`.
- Outside sources (Q0008), 2026-11-25: S9 from a search abstract (the Hive Workshop page answered HTTP 502); S10
  from a search abstract. No source found on the launch effect's audience; hence **verify** and Q0128 part 1.
- Code read on `a0384b78`: `combat_resolution.rs` (`launch_projectile`, `apply_weapon_hit`,
  `record_attack_presentation`, `record_attack_snapshot`, `record_attack_reveal`), `effects/eval.rs` (`enter_node`,
  `launch_missile`), `effects/actions.rs` (`apply_spell_damage`), `world/presentation.rs`, `world/vision.rs`
  (`count_down_attack_reveals`, `update_vision`), `world.rs` (`step_into`), `target_acquisition.rs` (`shown_to`),
  `checksum.rs` and `checksum/encoding.rs`, `catalog/spells.rs`, `assets/catalog/ability_effects.ron`,
  `statuses.ron`, the missile rows of `units.ron` and `structures.ron`.
- No prototype: the rule is three calls of an existing hook, and no hashed state moves in shipped data.

---

## As built

Built on branch `launch-spell-visibility-plan` against Q0128's defaults 1(a), 2(a), 3(a); `main` 2f922a50
(follow-order, schema 50) merged before WP4. Evidence:
[`docs/evidence/2026-11-27-launch-and-spell-visibility.md`](../evidence/2026-11-27-launch-and-spell-visibility.md).

| Package | Commit | What |
|---|---|---|
| WP0 | `7a37cdac` | B-205, B-206 restated; **B-227 registered** (the census finds three delivery mismatches: `flying_saucer` and `thorn_lasher_dome` instant where 5.0.14 launches a missile, `mothership_carrier` a missile where it does not); probe (v) zero everywhere; baseline 8/8; close battles 14, 26, 31, 33, 35, 38a, 38b |
| WP1 | `d52957e3` | The three fields, validation (`CatalogError::ZeroReveal` for the spell nodes), the storm pulse `Hidden`, schema 53; latent proof 8/8 at schema 49; worlds byte-identical |
| WP2 | `99bf0be7` | `AttackCause`; the release, spell damage and unit-aimed spell launch calls; worlds byte-identical; the showcase gains launch images on seeds 1, 7, 8 exactly as WP0 predicted (class P) |
| WP3 | `bd2a188e` | Client tests only (V-23 to V-25); no `client/src` change |
| Merge | `89bfb0e8` | `main` 2f922a50 merged; latent proof re-run at main's schema 50: 8/8 |
| WP4 | `a04d71ce` and the records commit | Goldens and sandbox re-recorded at 53, proved on four builds; held-out, throughput, records |

**Decisions recorded while building (Q0002).**
- The census ran with a resolver over the eight layers (`~/scratch/launch-spell-visibility/census/sc2.py`); its
  `Visibility` survey counts 50 damage overrides (46 `Visible`), one fewer than §2.1 S4, which changes no finding.
- V-22 is a unit test in `world/snapshot.rs` (the image-clearing hook is crate-private); V-15's reveal half and the
  loaded caster are V-21's dead and hidden rows for every cause.
- No `suites.ron` entry: the new test file is a `fast` target, selected by every `sim_core` change.
- The client stage casts the storm with a soldier that has media as the walk's caster, since no shipped spellcaster
  has flipbook media and the client refuses to present one without it; the image is the caster's whatever caused it.
- Held-out seeds and throughput compare the final tree with `main` 2f922a50 rather than WP0's tree, since the merge
  brings the follow order's own changes; this plan's changes are the only difference between the two.
