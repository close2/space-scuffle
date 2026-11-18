# Blueprint: Effect Snapshot Identity (B-122 to B-126)

Five effect bugs registered by WP11 of plan
[`2026-11-09-effect-visualization`](2026-11-09-effect-visualization.md), all caused by facts the simulation knows but
does not publish:

- **B-122.** A status is published by kind only (`StatusFlags::damage_over_time`), so Fungal Growth's victims cannot
  be told from Psionic Storm's, and `fungal_affliction` is named by nothing.
- **B-123.** Four weapons publish no delivery event: `jet_jumper` (Reaper analogue), `void_goliath` (Immortal
  analogue), `colossus_mech`'s air weapon (Thor analogue) and `volatile_tick` (Baneling analogue). They are authored
  `visual_kind: Projectile` without a `projectile_speed`. The first three fire with no look.
- **B-124.** A spell missile's impact is laid where the client last saw the missile plus half a step, up to half a
  step from where the simulation ran it.
- **B-125.** The client finds the bubble an instant hit or a blow struck by position. Across planes it can draw a
  shot at a flyer on a ground unit's glass, about 2.5 units low.
- **B-126.** A held beam is a series of 0.46 s lines fixed where they spawned. They splay behind a moving target and
  outlive a dead one.

This plan publishes the missing facts and changes the client to read them. The facts are which status (B-122),
which attacker and target (B-125, B-126), and where and whether a missile landed (B-124). The client's position
match, plane tie-break and half-step guess are removed. It also corrects the catalog data of B-123 from the 5.0.14
game data, which turns out not to move any battle (§2.1).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-16 against `main` at
`3f5e898e` (WP11 merged, B-122 to B-126 registered). A full plan is required because the change spans three
packages (`sim_types`, `sim_core`, `client`), changes catalog data, and touches the snapshot contract. Line numbers
drift, so search for the quoted symbol. Claims not confirmed while planning are marked **verify**.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, D-7 |
| Q0052: sanctioned deviations; S-6 original looks | Standing rule | D-4, D-5 (looks stay ours; facts follow the game) |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §6 |
| Q0080: up to five independent review rounds for effect looks | Answered (option 1) | D-9, WP8 |
| Q0002: scoping is the agent's call if written down | Standing rule | §10 |
| Open questions | None open in `docs/questions/open/` | — |
| None new | — | Every choice has a default recorded as a decision |

---

## 1. Summary

| | B-125 struck bubble | B-126 held beam | B-122 status identity | B-124 missile impact | B-123 silent weapons |
|---|---|---|---|---|---|
| Missing fact | Which entity was struck | Which attacker beams which target | Which status a bubble carries | Where (and whether) a spell missile landed | The weapon's delivery kind |
| Published today | A point at ground + 0.5 | The same point, per strike | `StatusFlags` by kind | The missile while it flies | Nothing (`emit_instant_hit_visual` returns on `Projectile`) |
| Chosen contract | `ends: Option<StrikeEnds { attacker, target }>` on `HitscanTracer`, `BeamVisual`, `MeleeSlashVisual` | The same `ends` | `RenderSnapshot::statuses: Vec<StatusView { entity, status: StatusId }>` | New event `CombatEventVisual::SpellImpact { pos, missile, owner, source }` on the tick the impact runs | Data: `visual_kind` per the 5.0.14 effect (Hitscan ×3, None ×1) and a loader rule |
| Client change | Lookup by id (presented frame, then previous frame); grid, slack and plane tie-break removed | A held `Shot` whose ends follow both bubbles, released on death or when strikes stop | Looks by status name (`defaults.status_by_name`); `fungal_affliction` shipped | Impact laid from the event; `MissileSlot::landing` and `step` removed | Each weapon names its looks; coverage test admits no silent weapon |
| Hashed state | None | None | None | None | None (D-7) |
| Goldens and matrix | Bit-identical | Bit-identical | Bit-identical | Bit-identical | Bit-identical (D-7, **verify** in WP3) |
| Size | M | M | S to M | S | S |

---

## 2. What StarCraft II does

Tags: **[DATA]** SC2GameData 5.0.14 (`5444e12f13`), extracted under `~/scratch/combat_wp3_data` (mod and file named,
then lines); **[COMM]** community or editor documentation; **[INF]** an inference of this plan. Looks stay original
(S-6). The game is the reference for *which facts* a look follows, not for how it looks.

### 2.1 The silent weapons (B-123)

| # | Source | What it says |
|---|---|---|
| W1 | liberty `WeaponData.xml`:317-328; void `WeaponData.xml`:88-91 (Range 5, MinScanRange 6) | Reaper `P38ScytheGuassPistol`: `Effect P38ScytheGuassPistolBurst`, Period 1.1, DamagePoint 0 [DATA] |
| W2 | liberty `EffectData.xml`:1803-1818 | `P38ScytheGuassPistolBurst` is a `CEffectCreatePersistent` at `TargetUnit`, `Channeled`, two periodic `CEffectDamage P38ScytheGuassPistol` at periods 0 and 0.122 (4, +5 Light). No `CEffectLaunchMissile` anywhere in the chain: the damage is **instant**, two strikes 0.122 s apart [DATA] |
| W3 | liberty `WeaponData.xml`:507-511; liberty `EffectData.xml`:2430-2440; void `WeaponData.xml`:54-62 | Immortal `PhaseDisruptors` names no `Effect`, so it runs the effect of its own id, the `CEffectDamage PhaseDisruptors` (20, +30 Armored): **instant**. The disabled air variant `PhaseDisruptersAir` names `Effect PhaseDisruptors` explicitly, which confirms the convention [DATA, INF] |
| W4 | swarmmulti `WeaponData.xml`:39-52; void `WeaponData.xml`:74-87; voidmulti `WeaponData.xml`:128-132; voidmulti `EffectData.xml`:748-751 | Thor `LanceMissileLaunchers` (High Impact Payload): `Effect LanceMissileLaunchersDamage`, with the `LanceMissileLaunchersLaunchMissile` line **commented out** in every layer. 5.0.14 values: Period 1.28, Range 11, 25 +10 Massive. **Instant** [DATA] |
| W5 | liberty `WeaponData.xml`:211-223, `EffectData.xml`:2259-2270 and 1424-1429, `MoverData.xml`:215-224 | The Thor's other anti-air weapon, `JavelinMissileLaunchers` (Explosive Payload), is a real guided missile (`ThorAAWeapon`, mover `ThorAA`, Guidance, MaxSpeed 20). Our catalog does not model that mode (deviation-fix plan, "the Thor's Explosive Payload splash waits for that mode") [DATA] |
| W6 | liberty `WeaponData.xml`:463-475, `EffectData.xml`:1739-1759 | Baneling `VolatileBurst`: a melee-flagged, hidden weapon whose effect set damages an area at `CasterUnit`. No delivery line [DATA] |

**What follows for us.**

- **No weapon here needs a projectile speed.** The register's suggested fix (B-123: "a projectile speed from the
  5.0.14 unit data ... changes when damage lands") assumed these weapons fly. They do not: all four deliver their
  damage on release, as our catalog already models (`projectile_speed: None`). The bug is the authored
  `visual_kind`, which is the one input that decides the renderer event (`emit_instant_hit_visual`, `_ => return`).
- **The Colossus question does not arise.** `colossus_mech` is the **Thor** analogue (`sc2_analogue: "Thor ..."`);
  the Colossus analogue is `tripod_annihilator`, whose only weapon is ground. A Thor has anti-air in 5.0.14 (W4), so
  `colossus_mech`'s air weapon is parity, and its values (range 11, period 23 ticks = 1.28 × 17.857, 25 +10 Massive)
  match W4.
- **Kinds.** Reaper, Immortal and High Impact Payload: `Hitscan` (an instant line from muzzle to target, which our
  look vocabulary draws as a slug, bolt or streak per unit). Baneling: `None` (its `SplashVisual` carries the
  picture, as now).

### 2.2 The facts the looks follow (B-122, B-124, B-125, B-126)

| # | Source | What it says |
|---|---|---|
| F1 | liberty `BehaviorData.xml`:328-338 (`FungalGrowth`) and :709-716 (`PsiStorm`); `assets/catalog/statuses.ron` | Two distinct behaviours (buffs) with their own identities. Ours are two distinct `statuses.ron` entries that publish the same kind, `DamageOverTime` [DATA] |
| F2 | void `EffectData.xml`:1141-1146 | `FungalGrowthLaunchMissile`: `ImpactLocation TargetPoint`: the impact is exactly at the aimed point [DATA]; plan `2026-11-11-contact-and-missiles` §2.2 M3 |
| F3 | swarmmulti `WeaponData.xml`:99-116 (`VoidRaySwarm`) | Period 0.5, `CanRetargetWhileChanneling`, `AllowedMovement Moving`: a channelled weapon whose strikes repeat on one target while the channel lasts and may switch target without ending [DATA]. In the game the beam is one continuous ray from the ship to its target while it channels [COMM, common observation] |
| F4 | — | An impact model is drawn on the struck unit's own body (its attachment points), whatever stands near it [COMM, editor convention; INF for our purpose] |

**What follows for us.** A status's look belongs to the status (F1), not to its kind. A missile's impact belongs
where the impact ran (F2). A hit belongs to the struck unit's own bubble (F4). A beam belongs to the attacker and its
current target for as long as strikes keep coming (F3).

---

## 3. Diagnosis

### 3.1 B-125: what the client cannot know

`emit_instant_hit_visual` (`crates/sim_core/src/combat_resolution.rs`) publishes `to = [target x, y, ground + 0.5]`.
`effects::struck::StruckBubbles` rebuilds a uniform grid over the presented bubbles each frame with events. It takes
the nearest bubble within its radius plus `MATCH_SLACK` (0.35), preferring the slot's plane for `Air`/`Ground`
weapons and breaking ties by height for `Primary` weapons. The published height is always the ground plus 0.5, so a
`Primary` anti-air shot at a flyer above a crowd takes a ground bubble (Amendments 26 to 28). `PreparedShot` holds
`attacker` and `target`, and `apply_weapon_hit` returns only for an existing target. The ids are known at every emit
site.

A further case, not stated in the register: a **killing** hit's event is popped when the presented tick reaches the
hit's tick, and the target was reaped on that tick. Its bubble is therefore absent from the presented frame and the
client falls back to the default bubble (E3-5). The previous snapshot (`BattleFrameState::previous`) still holds it.

### 3.2 B-126

Each strike of `prismatic_saucer`'s `LaserBeam` weapon (period 9 ticks = 0.36 s; F3's 0.5 Normal s × 17.857 ≈ 8.93)
is a `BeamVisual`. The client spawns a `prism_beam` `Shot` of 0.46 s per strike (Amendment 25: longer than period
plus the largest random delay), whose line is fixed at spawn. Following a target needs its id, and ending on its
death needs the id plus the presented frame.

### 3.3 B-122

`SimWorld::raise_status_flags` (`world/snapshot.rs`) ORs each status's `presentation` kinds into `StatusFlags`.
`EffectWorld::statuses` is already sorted by `(entity, status)` and each entry holds its `StatusId`, a dense index
into the spell catalog that the client can read (`SpellCatalog::status_id`, `status`).

### 3.4 B-124

`step_effect_missiles` (`effects/step.rs`) runs the impact on the tick whose step reaches contact and removes the
missile. The snapshot of that tick no longer lists it. `effects::areas::MissileSlot::landing` guesses half a
published step past the last position. A missile that fizzles (its unit target died or became untargetable,
`missile_aim` returns `None`) also lays the impact today, although no impact ran.

### 3.5 B-123

`emit_instant_hit_visual` matches `Hitscan`, `LaserBeam`, `LightningArc` and `MeleeSlash` and returns for
`Projectile` and `None`. The launch path (`dispatch_shot`) is chosen by `projectile_speed`, not by the kind. Four
weapons are instant with kind `Projectile` (§2.1). A scan of the shipped catalogs finds two inverse mismatches,
`hive_matron` (`MeleeSlash` with a speed) and `tesla_sentinel_dome` (`LaserBeam` with a speed). They do publish
(a `ProjectileVisual`) and are out of scope (§10).

---

## 4. Design decisions

### D-1. Presentation-only contract; nothing hashed changes

- All additions are output of tick phase 10 (`write_render_snapshot`) or unhashed provenance in
  `world::presentation::PresentationWorkspace`. No `Entity`, `EffectWorld`, `ActiveProjectile` or `WeaponSpec`
  field is added. There is no new random draw and no iteration-order change. `crates/sim_core/src/checksum.rs`
  (`CHECKSUM_SCHEMA_VERSION = 29`) and `checksum/encoding.rs` are not edited.
- **Serde compatibility.** Every new field is `#[serde(default, skip_serializing_if = ...)]` (`Option::is_none`,
  `Vec::is_empty`). Older RON records still read, and a frame without the new facts keeps its bytes, as
  `spell_missiles` does. `CombatEventVisual::SpellImpact` is a new variant, so old records cannot contain it.
- **Floats** enter only in `sim_types` and the snapshot writer. `PresentationWorkspace` records the landing in fixed
  point (`SimVec2`), and `write_render_snapshot` converts it. The C7 ratchet entries for
  `crates/sim_types/src/snapshot.rs` and `presentation.rs` (and `crates/sim_core/src/world/snapshot.rs` if the
  conversion counts) are raised by the exact new count, with their reasons unchanged.

### D-2. B-125: publish the struck entity, keep the point

- `sim_types::presentation::StrikeEnds { attacker: EntityId, target: EntityId }` (`Copy`, `deny_unknown_fields`).
  `HitscanTracer`, `BeamVisual` and `MeleeSlashVisual` gain `ends: Option<StrikeEnds>`. The simulation always fills
  it; `None` remains for synthetic events (lab, diagnostics, tests) and old records.
- `to` / `pos` keep their meaning (the struck unit's centre at ground + 0.5).
- **Rejected:** writing the drawn altitude into `to[2]`. It changes the meaning of a field the lab, the legacy
  `vfx` fallback and many tests read. It gives no radius or dome, and it cannot tell two stacked flyers apart. The
  id serves B-126 as well.

### D-3. B-125: the client's rule

`effects::struck` places a hit as follows.

- **Horizontal position.** Taken from the published point. It is the struck unit's exact centre on the hit's tick,
  and the event is popped when the presented tick reaches that tick.
- **Radius, drawn height and dome.** Taken from the bubble whose `entity_id == ends.target`, searched in this order:
  - the presented frame (sorted by entity id: `BattleFrameState::present` merges id-ordered sequences, so a binary
    search applies);
  - else the previous snapshot (`frames.previous()`): the killing-hit case of §3.1;
  - else the default bubble of E3-5.
- **Removed:** `StruckBubbles`' grid (`fill`, `cells_of`, `CELL`, `MOST_CELLS_PER_SIDE`), `MATCH_SLACK`,
  `struck_plane` and the tie-break. `StruckBubble` and `onto_glass` / `onto_outline` stay. `at_struck_surface`
  takes a `struck_by_id(EntityId) -> Option<StruckBubble>` lookup. A plain function over slices replaces the
  resource, so nothing is retained and nothing allocates.
- **Fog.** Unchanged. An event is admitted by its published point (`BattleFog::reveals`), and placing it on the
  bubble's outline shows no more than that point shows (an undetected cloaked target's position is already its
  published point).

### D-4. B-126: a held beam keyed on attacker and weapon

- **Data.** A `Shot` effect may declare `sustain: Some((grace: <s>))` (`effects.ron`). The loader refuses it on any
  other event kind, and the coverage test refuses it on a weapon whose delivery is not an instant line.
  `prism_beam` and `lance_beam` gain it (`grace: 0.12`, above the 0.09 s largest random delay of the shipped beams).
- **Runtime.** A new `effects::beams` module holds `HeldBeams`: slots sorted by `(attacker, EffectSource)`, capacity
  64, with a `refused` counter, in the style of `AreaLooks`. A `BeamVisual` with `ends` whose travel look sustains:
  - **refreshes** a matching slot instead of spawning a shot. It moves the slot's target to `ends.target` (a
    retarget, F3) and sets `deadline = now + period + grace`. The period is the attacker's presented
    `attack_swing.period_ticks / 25` (so attack-rate buffs count). Without a swing, the look's own duration is used;
  - **or holds** a new instance (`EffectRuntime::hold`).
- **Following.** Each frame each slot looks up both bubbles by id in the presented frame (D-3's lookup):
  - the line starts on the attacker's bubble (`tables.muzzle`) and ends on the target's outline
    (`struck::onto_outline`), set with `EffectRuntime::update_held_line`, as tethers follow their bubbles
    (`areas::keep_link`);
  - the fog clip is recomputed (`shot_clip`, fail-closed), because a held line, unlike a shot, lives indefinitely.
- **Release** happens when either bubble is not presented (death, load, gone), or `now > deadline` (strikes
  stopped). The layers' own `exit` fades it; `exit ≤ 0.1 s` is required by the loader for `sustain` looks. Per-strike
  impacts (`beam_burn`) stay one `Point` per strike, on the struck surface (D-3).
- **Rejected:**
  - publishing a per-bubble channel target from the weapon phase. It would show the beam during the wind-up,
    before any strike.
  - filling the unused `BubbleRenderInstance::attack_target_pos`. Same objection; recorded in §10.

### D-5. B-122: status identity and looks by status name

- **Contract.** `RenderSnapshot::statuses: Vec<StatusView { entity: EntityId, status: StatusId }>`, refilled in
  place from `EffectWorld::statuses()` (already sorted by `(entity, status)`). It lists only entities published as
  bubbles (skipping those loaded in a transport) and is skipped when empty. `StatusFlags` is unchanged and still
  drives the kind looks.
- **Looks.** `effects.ron` `defaults.status_by_name: { "fungal_growth": "fungal_affliction" }`, keyed by
  `statuses.ron` names. The client resolves these against the spell catalog at load (`SpellCatalog::status_id`),
  with typed errors (`UnknownStatus { name }`, and the existing wrong-event-kind error for a non-`Status` look). No
  Rust names a status (C5).
- **Kind looks stay.** A named look is held **in addition** to the kind looks: a fungaled unit shows
  `fungal_affliction` and, through its separate `fungal_growth_slow` status (`Slowed`), `concussive_daze`, as
  Amendment 24 intended. There is still no `DamageOverTime` default, so storm victims gain nothing. A storm look
  would be one data line (§10).
- **Holding.** The client holds named looks per bubble in `StanceTrack`: `named: [Option<(StatusId, HeldLook)>;
  MAX_NAMED_STATUSES]` with 4 slots, refusals counted. `keep_statuses` walks the presented frame's `statuses` with a
  cursor merged against the id-ordered bubbles. Membership follows the bubbles: a new
  `BattleFrameState::presented_statuses()` returns the previous or current list, as `presented_areas_and_links`
  does. The fog rule of the bubble applies (an undetected enemy shows no status look).
- **Rejected:**
  - keying by the applying ability (`AbilitySource` in the status context). One ability applies several statuses
    (Fungal Growth applies two) and one status may come from several abilities; the look belongs to the behaviour
    (F1).
  - keying by the bearer's block. The bearer did not cast the spell.

### D-6. B-124: the impact is an event

- **Simulation.** At `MissileStep::Impact`, for a `LaunchMissile` node whose `view` is `MissileView::Projectile`,
  `step_effect_missiles` records `(missile id, owner, contact, ability)` in `PresentationWorkspace` (fixed point).
  `write_render_snapshot` drains it into `CombatEventVisual::SpellImpact { pos: [x, y, terrain altitude], missile:
  u32, owner: PlayerId, source: AbilitySource }`, appended after the tick's other events. For Fungal Growth the
  contact is the aimed point (F2). Tether missiles (`MissileView::Tether`) publish none: their landing is a
  displacement with its own presentation.
- **Event API.** `anchor()` returns `pos`, `owner()` returns the owner, and `source()` returns `None` (an ability,
  not a weapon, as for a shatter).
- **Client.**
  - `keep_missiles_seen` only holds and releases missile looks and no longer lays impacts.
  - A new handler lays the impact of the missile look named by `source` (`ability_effect(..., EventKind::Missile)`
    then its `impact`), with the existing bindings (`missile_extent`) and area fog cut (`area_sight`).
  - `BattleFog::reveals` admits `SpellImpact` unconditionally, so the handler's area rule decides. This keeps
    today's "drawn where any of it is seen" (E1-3); a point rule would drop impacts centred in fog.
  - `MissileSlot::{landing, step}` are removed.
  - A missile that fizzles lays nothing.
- **Why an event, not a frame list.** The timeline queues every tick's `combat_events` (`stage_events`) and pops
  them in order, but presents only two snapshots. A frame list written on a tick the presentation skips (several
  ticks per frame at high battle speed) would never be seen, and the impact would be lost.
- **Residual (recorded).** During the interpolation into the landing tick the missile look stays at its last
  published position (the next frame no longer lists it), so the look may stop up to one tick short before the
  exact impact appears. Steering it into the impact would need the client to peek the next tick's events (§10).

### D-7. B-123: data per §2.1, a loader rule, no battle change

- **`units.ron`.**
  - `jet_jumper.weapon.visual_kind`, `void_goliath.weapon.visual_kind` and `colossus_mech.air_weapon.visual_kind`:
    `Projectile` to `Hitscan`;
  - `volatile_tick.weapon.visual_kind`: `Projectile` to `None` (**verify** that RON reads the enum variant `None`
    for this non-`Option` field. If it does not, add `#[serde(rename = "NoVisual")]`, `alias = "None"` is not
    enough, and record it);
  - comments citing W1 to W4 and W6.
- **Loader rule** (`WeaponSpecDef::validate`, `catalog/validation.rs`): a weapon without `projectile_speed` must not
  have `visual_kind: Projectile`. The error is `CatalogInvalidValue` with field `visual_kind` and reason "an instant
  weapon publishes no projectile". The inverse (a speed with another kind) is not enforced, because such kinds are
  hashed in flight (§10).
- **Looks** (presentation blocks, existing accepted looks only, no texture):
  - `jet_jumper` Primary: `gauss_tracer` / `bullet_hit` (kinetic rounds);
  - `void_goliath` Primary: `cannon_tracer` / `plasma_hit`;
  - `colossus_mech` Air: `cannon_tracer` / `rocket_hit`.

  Units without media stay lab-only (Amendment 24).
- **Why no golden moves.** `visual_kind` is read by no tick decision (only by `emit_instant_hit_visual` and the
  snapshot's `weapon_visual`). It is hashed only inside a `WeaponSpec` held by `Entity::siege_weapon` or
  `ActiveProjectile::weapon_spec` (`fields!(WeaponSpec …)`). None of the four units has a mode, and their weapons
  have no speed, so neither holder ever carries them. **Verify** in WP3 by the unchanged goldens; the stop rule of
  §8 applies.
- **Register correction.** B-123's text ("changes when damage lands ... needs its own plan") is restated: an
  outcome-neutral data fix, per W1 to W4.

### D-8. Determinism verification

At every simulation WP (WP1 to WP3):

- `git diff --exit-code <base> -- crates/sim_core/src/checksum.rs crates/sim_core/src/checksum/
  crates/sim_core/tests/fixtures/` is empty;
- `golden_replay_tests.rs` passes unchanged in release and on `wasm32-wasip1` (selected by `cargo gate package` for
  simulation changes);
- `sc2_who_wins_matchup_matrix_tests.rs` (against `fixtures/matchup_matrix.ron`), `sc2_battle_benchmarks_tests.rs`,
  `checksum_state_tests.rs` and `determinism_lockstep_tests.rs` pass unchanged;
- new unit test `writing_a_snapshot_changes_no_checksum` (`world/snapshot.rs`): a world holding statuses, a landing
  missile and hits has the same `compute_checksum()` before and after `write_render_snapshot`;
- the release throughput benchmark stays within noise (the writer adds an O(statuses) pass).

### D-9. Looks and review

- WP3 reuses accepted looks.
- WP5 (`fungal_affliction` now shown in play) and WP7 (held beams) change what the player sees, so their packets go
  to the independent review of Q0080 (up to five rounds) before WP8 closes the register entries.
- WP4 and WP6 move existing looks onto exact facts and are judged by their measurements in the same review packet.

---

## 5. File ownership

| WP | Files |
|---|---|
| WP1 | `crates/sim_types/src/presentation.rs` (`StrikeEnds`, `StatusView`), `snapshot.rs` (fields, variant, `anchor`/`owner`/`source`), `crates/sim_types/tests/snapshot_serialization.rs`; every struct literal of the three variants (`ends: None`): `sim_core` (`combat_resolution.rs`, tests `render_owner_tests.rs`, `transport_ownership_tests.rs`, `checksum_state_tests.rs`), `client` (`effects/struck.rs`, `effects/systems.rs`, `vfx.rs`, `lab_ui.rs`, `battle_pipeline/fog.rs`, tests), `client_diagnostics` (`runtime_capture/driver.rs`, `ownership/*`, `frame_budget/effect_stress.rs`); `crates/workspace_checks/tests/charter.rs` (C7 counts) |
| WP2 | `crates/sim_core/src/combat_resolution.rs` (`emit_instant_hit_visual`), `world/presentation.rs` (landing records), `effects/step.rs` (record), `world/snapshot.rs` (statuses, impacts); tests `effect_snapshot_tests.rs`, `fungal_growth_tests.rs`, `effect_tree_allocation_tests.rs` |
| WP3 | `assets/catalog/units.ron`, `crates/sim_core/src/catalog/validation.rs`, `effect_snapshot_tests.rs`, `crates/client/tests/effect_catalog_tests.rs` |
| WP4 | `crates/client/src/effects/struck.rs`, `effects/systems.rs`; tests `weapon_battle_gpu_tests.rs`, `sync_allocation_count_tests.rs`, `effect_slice_gpu_tests.rs` (binding call only) |
| WP5 | `effects/catalog/{schema,resolve,units}.rs`, `effects/stance.rs`, `battle_pipeline/timeline.rs`, `assets/catalog/effects.ron`; tests `effect_catalog_tests.rs`, `area_link_gpu_tests.rs`, `status_gpu_tests.rs`, `battle_fog_tests.rs` |
| WP6 | `effects/areas.rs`, `effects/systems.rs`, `battle_pipeline/fog.rs`, `vfx.rs` (ignore arm); tests `weapon_look_tests.rs`, `area_link_gpu_tests.rs` |
| WP7 | `effects/beams.rs` (new), `effects/mod.rs`, `effects/catalog/*` (`sustain`), `effects.ron`; tests `weapon_look_tests.rs`, `weapon_battle_gpu_tests.rs`, `sync_allocation_count_tests.rs`, `sync_steady_state_tests.rs` |
| WP8 | `DESIGN.md`, `docs/plans/README.md`, `docs/plans/2026-11-09-effect-visualization.md` (pointer amendment), evidence `docs/evidence/2026-11-17-effect-snapshot-identity.md`, `crates/gate/suites.ron` if a test file was added |

---

## 6. Work packages (commit order)

Each WP: tests first (red), then code, `cargo gate quick`, hand back; a verifier runs `cargo gate package` (Q0061).
S ≤ 1 day, M 2 to 3 days. Branch `effects-identity`. WP1 to WP3 form one simulation package, verified together; WP4
to WP7 may run two at a time after it (WP7 after WP4, which it reuses); WP8 last.

| WP | Content | Tests (written first) | Gate and exit | Size |
|---|---|---|---|---|
| WP0 | This plan's row; B-123 row restated (D-7) | — | Docs-only `package` (fmt, `workspace_checks`) | — |
| WP1 | `sim_types` contract (D-2, D-5, D-6) and every literal updated | See below | `package`: workspace builds, `sim_types` green; goldens and matrix bit-identical | S to M |
| WP2 | `sim_core` writer: `ends`, `statuses`, `SpellImpact` | See below | `package` with golden replays (release, `wasm32-wasip1`), matrix and D-8 diff check | M |
| WP3 | B-123 data, loader rule, looks | See below | As WP2; coverage test lists no silent weapon | S |
| WP4 | Client B-125 | See below | `package` with GPU (`weapon_battle_gpu_tests`) and valgrind (`sync_allocation_count_tests`) triggers | M |
| WP5 | Client B-122 | See below | `package` with `area_link_gpu_tests`, `status_gpu_tests`; review packet | S to M |
| WP6 | Client B-124 | See below | `package` with `area_link_gpu_tests` | S |
| WP7 | Client B-126 | See below | `package` with GPU and valgrind; review packet | M |
| WP8 | Independent review (Q0080), bookkeeping | `workspace_checks` (C17, C18), `suite_catalog_tests.rs` (C22) | `merge`; plan row Done | S |

### WP1: the contract (`sim_types`)

- Add `StrikeEnds`, `StatusView` (both `Copy`, `deny_unknown_fields`, documented with units and invariants),
  `RenderSnapshot::statuses`, `ends` on the three variants, and `CombatEventVisual::SpellImpact`.
- Update `owner()`, `source()` (`None` for `SpellImpact`) and `anchor()` (`pos`).
- Every struct literal across the workspace gains `ends: None`. Every exhaustive match gains a `SpellImpact` arm:
  the legacy `vfx` ignores it, and `struck`/`systems` pass it through until WP6.
- **Tests.**
  - `snapshot_serialization.rs`: round trip with `ends: Some`, a non-empty `statuses` and a `SpellImpact`;
    `events_and_snapshots_written_before_ends_still_read` (records without the keys read `None`/empty);
    `a_frame_without_the_new_facts_keeps_its_bytes` (serialize, compare with the text before the fields existed);
  - `snapshot.rs` unit tests: `a_spell_impact_is_anchored_where_it_landed_and_is_no_weapons`;
  - `presentation.rs` unit tests: `StatusView` order is `(entity, status)`.

### WP2: the writer (`sim_core`)

- `emit_instant_hit_visual` fills `ends` from `PreparedShot { attacker, target }`.
- `write_render_snapshot` fills `statuses` (D-5) and drains landings into `SpellImpact` (D-6).
- `PresentationWorkspace` gains `spell_impacts: Vec<SpellImpactRecord>` with `record_spell_impact`,
  `drain_spell_impacts`. Capacity is retained; it is drained on every snapshot, which `step_into` writes every tick.
- **Tests** (`effect_snapshot_tests.rs` unless noted).
  - `every_instant_hit_and_blow_names_its_attacker_and_target`: over `shipped_weapons()`, every `HitscanTracer`,
    `BeamVisual` and `MeleeSlashVisual` has `ends == Some(StrikeEnds { attacker, target })` of the duel.
  - `statuses_list_each_status_of_each_published_unit_in_order`: a fungaled unit lists `fungal_growth` and
    `fungal_growth_slow` (ids from `SpellCatalog::status_id`); a storm victim lists `psionic_storm`; a unit loaded
    in a transport lists none; the list is sorted; a frame without statuses serializes without the key.
  - `fungal_growth_tests.rs::the_impact_is_published_where_it_ran`: exactly one `SpellImpact` on the impact tick
    (13 in the existing scenario), at the aimed point (±1e-4), with the caster's owner and ability; none on other
    ticks. `abduct_tests.rs`: a tether missile publishes none.
  - `a_missile_that_loses_its_target_publishes_no_impact`: a scratch `LaunchMissile` (Projectile view) at a unit
    that dies in flight.
  - `effect_tree_allocation_tests.rs`: a warmed snapshot with twenty statuses and a landing allocates nothing.
  - D-8 unit test `writing_a_snapshot_changes_no_checksum`.

### WP3: B-123 (`units.ron`, loader)

- Data and rule per D-7; the three looks named in the units' `effects` blocks (`colossus_mech` gains an `Air` entry).
- **Tests.**
  - `catalog/validation.rs` unit test `an_instant_weapon_may_not_publish_a_projectile` (the error names
    `visual_kind`); the shipped catalogs load.
  - `effect_snapshot_tests::every_shipped_weapon_publishes_its_delivery_or_splash`: replaces the 75 % tolerance of
    `every_weapon_event_carries_its_source`. Every shipped weapon with a target fires at least one event, and its
    kind matches its delivery (instant `Hitscan` → `HitscanTracer`, `LaserBeam`/`LightningArc` → `BeamVisual`,
    `MeleeSlash` → `MeleeSlashVisual`, a speed → `ProjectileVisual`, `None` → only `SplashVisual`). This closes
    Amendment 25's P1-4.
  - `effect_catalog_tests::every_shipped_weapon_names_the_looks_of_the_event_it_publishes`: `silent` must be empty.
- **Determinism.** D-8 at this commit specifically. If any golden or matrix result moves, stop and hand back (§8).

### WP4: B-125 (client)

- D-3 in `effects::struck`; `spawn_effects_from_events` passes the presented and previous bubbles.
- **Tests.**
  - `struck` unit tests rewritten:
    - `a_hit_takes_its_targets_bubble_by_id_not_its_neighbours`: a bigger neighbour nearer the point is ignored;
    - `a_primary_shot_at_a_flyer_over_a_crowd_is_drawn_on_the_flyer` (the B-125 cross-plane case: the hit height is
      the flyer's glass, 2.5 units above the ground units');
    - `a_killing_hit_finds_its_target_in_the_previous_frame`;
    - `an_event_without_ends_counts_as_the_default_bubble`;
    - the existing glass, outline, dome and pull-limit tests, kept.
  - `weapon_battle_gpu_tests`, new last wave `cross_plane`: a presentable `Primary` anti-air shooter (or a stand-in
    lent its look in the test world's catalog only, as WP11's waves do) fires at a flyer hovering over a packed
    ground group. The impact must pass the E4-1 rule beside the **flyer's** glass band, and change no more than its
    blank beside any ground bubble.
  - `sync_allocation_count_tests` (valgrind): the E3-3/E4-3 stream now sends hits with `ends` and checks that the
    lookup finds every presented bubble by id. The grid's check is removed with the grid.

### WP5: B-122 (client)

- D-5: `status_by_name` in the schema and resolver; `UnitEffectTables::named_status(StatusId)`;
  `presented_statuses`; named slots in `StanceTrack`; `effects.ron` names `fungal_affliction` (comment B-122
  replaced).
- **Tests.**
  - `effect_catalog_tests`: `named_status_looks_resolve_against_the_spell_catalog`; an unknown status name and a
    non-`Status` look are each refused with their typed error.
  - `stance.rs` unit tests: a bubble with `fungal_growth` holds `fungal_affliction`; one with `psionic_storm` holds
    nothing named; a fifth named status is refused and counted; release when the status leaves.
  - Timeline unit test: `presented_statuses` follows bubble membership.
  - `battle_fog_tests`: an undetected cloaked enemy under Fungal Growth shows no status look.
  - `area_link_gpu_tests::fungal_growth_flies_lands_and_afflicts`: the victims wear `fungal_affliction` (its shell
    rim measured on each victim), and a storm stand-in's victims do not.
  - `status_gpu_tests` re-baselined only if its subjects carry a named status (cause stated).

### WP6: B-124 (client)

- D-6: the `SpellImpact` handler, `reveals` admission, `keep_missiles_seen` without impacts, `landing`/`step`
  removed.
- **Tests.**
  - `areas.rs` unit tests:
    - `an_impact_is_laid_where_the_event_says`;
    - `a_vanished_missile_without_an_impact_lays_nothing`;
    - `impacts_of_skipped_ticks_are_still_laid` (two ticks' events staged, one presentation).
  - `weapon_look_tests`: the missile-landing expectation moves to the event's point (replacing "half a step").
  - `area_link_gpu_tests`: the impact's `Critical` ring is centred within 2 px of the projected aimed point, from
    ±(half a step + 2 px). The fog checks of E1-3 still pass.

### WP7: B-126 (client)

- D-4: the `sustain` field and loader rules, `effects::beams::HeldBeams`, and the wiring in
  `spawn_effects_from_events` and a per-frame `keep_beams` system ordered after presentation.
- **Tests.**
  - `effect_catalog_tests`: `sustain` refused on a non-`Shot` effect, on an `exit > 0.1` look, and in the coverage
    test on a weapon that is not an instant line.
  - `weapon_look_tests`:
    - `a_stream_of_strikes_holds_one_beam`;
    - `the_beam_follows_both_bubbles` (ends within 1e-3 of the moved bubbles' muzzle and outline points);
    - `a_retarget_moves_the_end_without_a_second_beam`;
    - `the_beam_ends_when_the_target_is_gone`;
    - `the_beam_ends_a_period_and_grace_after_the_last_strike` (period from `attack_swing`);
    - `a_full_table_refuses_and_counts`.
  - `weapon_battle_gpu_tests`, wave `held_beam`: a beam shooter fires at a target walking across the screen. In
    every frame of the stream the beam's `Critical` end lies within 3 px of the target's projected outline point.
    After the target's death no beam pixel remains beyond its exit plus one frame.
  - `sync_steady_state_tests` and valgrind `sync_allocation_count_tests`: a held-beam stream with retargets and
    deaths allocates nothing.

### WP8: review and bookkeeping

- The independent review (D-9), with an evidence packet: the cross-plane, held-beam and Fungal sequences at 1:1 and
  2x on radv and lavapipe, and the measurements.
- On acceptance:
  - `DESIGN.md`: B-122 to B-126 marked fixed (with commits), the "Effect presentation" row's known-issues cell
    updated, and the proposed new entry of §10 registered;
  - this plan's row set to Done;
  - a pointer amendment in the effect-visualization plan;
  - `crates/gate/suites.ron` rules for any new test file (C22).

---

## 7. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Serialization | New facts round-trip; old records read; frames without them keep their bytes | `sim_types/tests/snapshot_serialization.rs`, unit tests |
| Writer facts | `ends`, `statuses`, `SpellImpact` are exact and complete; tether missiles and fizzles publish none | `effect_snapshot_tests.rs`, `fungal_growth_tests.rs`, `abduct_tests.rs` |
| Determinism | Nothing hashed moved | D-8: goldens (release, `wasm32-wasip1`), matrix, benchmarks, checksum tests, the writer's checksum unit test, diff check |
| Data | No instant weapon is `Projectile`; every shipped weapon publishes | `validation.rs` unit test, `effect_snapshot_tests.rs`, `effect_catalog_tests.rs` |
| Client logic | Lookup by id, named statuses, impact by event, held beams | `struck`, `stance`, `areas`, `beams` unit tests; `weapon_look_tests.rs`; timeline tests |
| Picture | Cross-plane hit on the flyer; beam follows and ends; affliction on fungal victims only; impact on the aimed point | `weapon_battle_gpu_tests.rs`, `area_link_gpu_tests.rs` (radv locally, lavapipe on CI) |
| Steady state | No allocation with statuses, impacts, hits and beams | `effect_tree_allocation_tests.rs`, `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` (valgrind) |
| Charter | C5 (no status or effect names in Rust), C7 ratchet, C22 | `workspace_checks`, `suite_catalog_tests.rs` |

---

## 8. Determinism summary and stop rule

- **Expected:** every golden replay checksum and matrix battle bit-identical at every WP. The checksum schema stays
  29, and the encoding and fixtures are untouched.
- **Stop rule:** if a golden or matrix result moves in WP1 to WP3, stop the WP. Find the cause (a presentation record
  read by a tick decision, an iteration order, a hashed `visual_kind`) and hand back; do not re-record. No
  re-recording is sanctioned by this plan. Should D-7's premise fail (a changed weapon reaching a hashed
  `WeaponSpec`), B-123's data change leaves this plan for one of its own with justified goldens, and the loader rule
  waits with it.

---

## 9. Risks

| Risk | Mitigation |
|---|---|
| WP1's field addition breaks many struct literals across four crates at once | One mechanical commit (`ends: None`); `cargo gate quick` per crate before hand-back; the list in §5 |
| A killing hit's target absent from both frames (presentation jumped several ticks) | Default bubble of E3-5, as today; counted in the evidence |
| `RenderSnapshot::statuses` grows the client's per-frame work | Merge cursor over two id-ordered sequences, O(bubbles + statuses); steady-state and valgrind windows |
| A held beam's fog clip recomputed every frame costs time | At most 64 beams; `shot_clip` samples are bounded by length and reach; frame budget measured in WP8's packet |
| A beam lingers after its attacker stops firing | Deadline is the attacker's own period plus 0.12 s, shorter than today's 0.46 s fixed life |
| `SpellImpact` admitted without a fog check leaks an unseen impact | The handler's `area_sight` returns `Hidden` and lays nothing; the unit test `effects::areas::tests::a_real_fog_hiding_the_whole_impact_lays_nothing` covers it through a real `BattleFog` (Amendment 3) |
| RON cannot read `visual_kind: None` for an enum field | D-7 fallback (`rename`), recorded |
| Review rejects the shown `fungal_affliction` or the held beam | Q0080 rounds; the facts and lookups ship regardless, only the data naming waits |
| Changed `jet_jumper` / `void_goliath` looks are unseen in play (no media) | Lab and stand-in GPU coverage, as WP11 did; recorded |

---

## 10. Out of scope (recorded, Q0002)

- **Proposed new register entry (B-128):** a `ProjectileVisual` is published at ground + 0.5 at both ends. An
  anti-air projectile therefore flies and lands below its flyer (the same cross-plane fault as B-125, for
  projectiles). The fix publishes `ends` on `ProjectileVisual` and re-aims the arrival on the target's bubble. It is
  a follow-up, not done here, so as not to publish a field nothing reads.
- **Inverse kind mismatches:** `hive_matron` (`MeleeSlash` with a speed) and `tesla_sentinel_dome` (`LaserBeam` with
  a speed). `visual_kind` is hashed in `ActiveProjectile::weapon_spec`, so changing it re-records goldens; it
  belongs with B-128 or a data plan.
- **The Thor's Explosive Payload mode** (Javelin missiles, W5): a mechanics change, already recorded by the
  deviation-fix plan.
- **Own looks** for the Reaper, Immortal and High Impact Payload analogues: they reuse accepted looks (D-7). A
  dedicated look is an art task with its own review.
- **Storm and other named status looks;** per-caster or bearer overrides of named status looks.
- **Steering a missile's last interpolation step into its impact** (D-6 residual).
- **The unused `BubbleRenderInstance::attack_target_pos`** (always `None`): removal or use is a separate cleanup.
- **The cleave's facing on `SplashVisual`** (Amendments 5 and 24): unchanged.

---

## Amendment 1: WP1 to WP3 as built, and the verify items (2026-11-16, short note, charter §6)

The simulation package (WP1 `1f72b8f8`, WP2 `a2cf5c5a`, WP3 on branch `effects-identity`) was
implemented as planned. This note records the **verify** items' outcomes and the small choices
the plan left open.

**Verify items.**

| Item | Outcome |
|---|---|
| D-7: RON reads `visual_kind: None` for the non-`Option` `WeaponVisualKind` field | Yes. The shipped `units.ron` loads with `volatile_tick`'s `visual_kind: None` and the weapon reads `WeaponVisualKind::None` (it publishes only its splash in `every_shipped_weapon_publishes_its_delivery_or_splash`). No `rename` was needed |
| D-7 / §8: no changed weapon reaches a hashed `WeaponSpec`; no golden or matrix result moves | Confirmed at WP2 and again at WP3 (data in place): `golden_replay_tests` in debug, release and on `wasm32-wasip1` under wasmtime, `sc2_who_wins_matchup_matrix_tests` (against `fixtures/matchup_matrix.ron`), `sc2_battle_benchmarks_tests`, `checksum_state_tests` and `determinism_lockstep_tests` pass unchanged; `git diff main -- crates/sim_core/src/checksum.rs crates/sim_core/src/checksum/ crates/sim_core/tests/fixtures/` is empty. Nothing was re-recorded |
| D-1: the C7 ratchet | `crates/sim_types/src/snapshot.rs` 30 → 31 (`SpellImpact::pos`), reason unchanged. `presentation.rs` gains no float (`StrikeEnds` and `StatusView` hold ids). `crates/sim_core/src/world/snapshot.rs` needs no change: the conversion is `to_f32()` calls, which the ratchet does not count |
| D-8: the writer changes no checksum | `world::snapshot::tests::writing_a_snapshot_changes_no_checksum`: on every tick of a world with a scratch status, a landing scratch missile and instant hits, `compute_checksum()` after the tick's snapshot equals the checksum the tick computed before writing it, and a second write moves nothing |

**Choices made while building.**

- **Statuses are written per bubble.** `raise_status_flags` became `publish_statuses`, which raises
  the kind flags and appends the bubble's `StatusView`s from `EffectWorld::statuses_of` in the
  same pass. Bubbles are written in entity order and the engine keeps its statuses sorted and
  unique, so the list is sorted with no sort of its own, and a carried unit (no bubble) lists
  nothing. A status the spell catalog cannot resolve is skipped for both, as before.
- **The loader error's field** carries the slot prefix, as every weapon field does
  (`weapon.visual_kind`, `ground_weapon.visual_kind`, `air_weapon.visual_kind`).
- **WP3's coverage test** `every_shipped_weapon_publishes_its_delivery_or_splash` replaces
  `every_weapon_event_carries_its_source` and keeps its source check. It covers every shipped
  weapon for which the catalog has a target; none is silent.
- **The allocation test** (`effect_tree_allocation_tests.rs`) counts the new writer frames
  (`publish_statuses`, `record_spell_impact`, `drain_spell_impacts`) as engine frames, and its
  children assert that twenty units' statuses are published and that Fungal Growth lands in the
  shipped child's window. Both children allocate nothing in the engine.
- **Abduct** gains a test of its own (`a_tether_missile_publishes_no_impact`) rather than a
  change to an existing one.
- **Plan row status.** `docs/plans/README.md` accepts no "In progress" status (C18); the row is
  `Active`. The WP1 commit carried "In progress", corrected in WP2.
- **Client.** Beyond compile fixes nothing reads the new facts yet (WP4 to WP7). The legacy
  `vfx`, `effects::systems` and `effects::struck` pass `SpellImpact` through without a spawn.
- **Throughput (D-8, deferred in part).** One release run of `cargo bench -p sim_core --bench
  throughput` at WP3, at load average 39 to 56 on the shared server: 2598, 939, 134, 163 and 190
  ticks/s on the flat rows, 112 on `showcase_canyon` and 163 with 67 casters, every row far above
  the floor of 25. At that load the figures cannot show "within noise" against `DESIGN.md` §5, so
  the alternated comparison with `main` (three runs each, as §5 measures) moves to WP8's
  evidence packet. The writer's additions are one pass over the published bubbles' statuses and
  one over the tick's landings, with no allocation in steady state.

## Amendment 2: WP4 and WP6 as built (2026-11-16, short note, charter §6)

WP4 (`29b978f3`) and WP6 (`593811ab`) are on branch `effects-identity-client`, made from
`effects-identity` at WP3. They change the client only; no simulation crate, golden replay or
checksum is touched. This note records the choices the plan left open, one correction to D-3
and the evidence. The review packet is the section "Identity WP4/WP6" of
`docs/evidence/effects/review/independent_review_packet.md`.

**D-3 corrected: a third place to look for a killing hit's target.** §3.1 says the previous
snapshot still holds a killing hit's target. That holds only when the presentation shows the
hit's tick exactly (paused, or alpha 1). The event of tick T is popped once the presented tick
reaches T. In play that happens on the frame that runs tick T+1, while the pose is interpolated
from T towards T+1. Then `previous` is snapshot T, and the target was reaped on T, so neither
snapshot lists it. `effects::struck::StruckBubbles` therefore keeps, refilled in place every
frame, the id, radius, drawn height and dome flag of the bubbles presented on the frame before.
The lookup order is: the presented frame, the previous snapshot, the last frame's bubbles, the
default bubble (E3-5). Each lookup is a binary search over an id-ordered slice. Each place
counts its finds (`StruckFrom`). The unit test
`a_killing_hit_shown_while_interpolating_finds_its_target_in_the_last_frame` drives the real
`BattleFrameState`: tick 1 kills, tick 2 runs, and the event pops with both snapshots lacking
the target. The retained buffer is the one allocation the plan's "nothing is retained" did not
foresee. It is sized by the presented bubbles and refilled without allocation in steady state:
the valgrind effect stream's window difference is 0.

**WP4: the cross-plane case and the published slot.** `sim_core::world::presentation::weapon_source`
publishes a strike by the archetype's air weapon under `WeaponSlot::Air`, and
`weapon_against(Air)` never returns the primary `weapon`. So a strike at a flyer from the
shipped data is never a `Primary` strike. The removed rule preferred the flyer's plane for an
`Air` slot, so it was not wrong in this exact staging. The register's `Primary` case (B-125)
arises only for a synthetic or future event. The unit test
`a_primary_shot_at_a_flyer_over_a_crowd_is_drawn_on_the_flyer` covers it. The GPU wave
`cross_plane` proves the lookup by id on real pixels: the impact on the flyer and nothing on the
ground units under it. It does not reproduce the old fault, which the shipped data could not
produce.

**WP4: the `cross_plane` wave.** The anti-air shooter (the first presentable unit whose block
names looks for its `air_weapon`, found by data) fires at the largest presentable flyer. Three
of the largest presentable ground units stand packed under the flyer, 0.05 apart, straight
under it and either side up and down the screen. The flyer's impact must pass E4-1 beside the
flyer's glass. Beside the ground units, outside the flyer's own zone, it may change at most
its blank's count plus `UNDER_ALLOWANCE` (8 px).

**WP4: two fixes to the staged battle's staging.** These were found while running it on the
base branch, whose WP3 data changed the waves.
- *Spots off the map's edge.* A wave staged `shock_golem` (radius 0.5625) at map x = 0.54. The
  simulation stopped with "actor is outside its current full-radius movement domain", and the
  child failed. This is not caused by WP4: the wave's subjects are planned as on the base
  branch. `find_spots` now probes the ground `BODY_REACH` (0.5) beyond each end of the widest
  pair, with the probe's own half unit round it, so a body of radius up to 1 stays on walkable
  ground. On the skirmish's camera this finds 3 spots a wave instead of 5. The test then runs
  9 waves instead of 6 (the approach subjects take two).
- *The approach subjects were truncated to a wave's capacity.* With 3 spots, the `AtFlyer`
  subject and one blow were silently dropped. They are now staged in as many waves as they
  need (`approach_subjects`, chunked).

**WP4: a lab family that no longer shows by the E4-1 bar (open, not changed).** In the
re-planned waves, `needle_volley`+`needle_hit` (lent to `rhino_beetle`, a projectile) changes
23 px beside the struck bubble against a blank of 7. The bar is 34. In WP11 round 5, in another
wave and spot, it changed 43. The run is deterministic: two runs gave 23. Projectile impacts
land at the struck unit's centre at ground + 0.5, inside its masked interior (the proposed
B-128, §10). Only their spill beyond 0.8 of the radius counts, so they are the most sensitive to
the spot and the target. The bar and the look are the review's (Q0080) and the owner's, not
this WP's. The failure is handed back with the evidence (`identity_wp4_needle_volley_radv.png`),
and no threshold was changed.

**WP6: as planned.** `effects::areas::lay_spell_impact` (and `_seen`, which takes the fog's
question) lays the `impact` of the `Missile` look the caster's block names for the event's
`source`. It lays it at the event's `pos`, bound to `missile_extent`, with the area fog cut
over the impact's whole reach, or not at all when the viewer sees none of it.
`spawn_effects_from_events` calls it for each `SpellImpact`. `keep_missiles_seen` only holds and
releases. `MissileSlot` lost `landing`, `step`, and `published`, `source`, `owner` and `effect`,
which only the landing guess read. `BattleFog::reveals` returns true for `SpellImpact`.

**WP6: measuring the impact's centre.** Once the viewer's impact is 0.18 s old (its ring is at
the search's radius), the Fungal child stops `Time<Virtual>`. It pictures the `Critical` layers
alone and then no effect layer. Along 32 ground rays out of the aimed point, each ray's reach is
the mean distance of the samples the ring changes from 0.7 radii out (inside lie the victims'
own looks). Opposite rays give the offset along their axis by least squares on the ground,
projected to pixels. Two points of method:
- A window point is read from the pixel whose square holds it (`floor`). The edge tests'
  `changed_at` rounds, which shifts every sample half a pixel right and down.
- The band's mean is steadier than the outermost sample, which the ring's eroded edge moves.

With the outermost sample and rounding, the same picture measured 2.02 px. With the band's mean
and `floor`, it measured 0.86 px.

**Evidence (radv, the RX 580, debug build, Xvfb 1280x720).**

| Check | Result |
|---|---|
| `struck` unit tests (11) | pass |
| `cross_plane` wave | impact 271 px (272 in an earlier run) beside the flyer, its blank 24 (bar 68); beside the 3 ground units 7 px, its blank 7 (allowed 15) |
| Fungal impact | laid 0.00000 world units from the aimed point; `Critical` ring centred 0.86 px from it over 16 ray pairs (tolerance 2 px) |
| Fungal fog edge (E1-3) | 0 of 357 unseen ground samples changed; 488 of 619 seen ones |
| Valgrind effect stream (hits with `ends`, impacts by event) | window difference 0, as its baseline; planted control 64 |
| `areas` unit tests, `weapon_look_tests` (24), `battle_fog_tests` (10), `lab_scene_tests` | pass |
| `area_link_gpu_tests` (3 measured tests) | pass |
| `weapon_battle_gpu_tests` | red on `needle_volley` only, as above |

## Amendment 3: B-128 brought into scope, and the WP4/WP6 review conditions (2026-11-16, short note, charter §6)

The independent review of WP4/WP6 accepted the code with conditions. `weapon_battle_gpu_tests` is red on
`needle_volley`+`needle_hit` (23 px beside the struck bubble against a bar of 34). WP4 does not touch the projectile
path; the red comes from re-staging (WP3's data and the `find_spots` fix put the subject in another wave, spot,
shooter and target). Its cause is the fault §10 listed as out of scope: a `ProjectileVisual` is published at
ground + 0.5 and its impact is laid at the struck unit's centre, inside the bubble's masked interior, for ground
targets as well as flyers. A red test is not merged, and a bar is not lowered to pass it (charter §1.1). The
follow-up is therefore done here as WP4b.

**Register.** The entry §10 proposed as B-127 is **B-128** (B-127 is now the radv test stall on `main`). DESIGN.md
gains the B-128 row, worded for every target: *a projectile's flight and impact are not aimed at the struck
bubble; the impact is laid at the target's centre at ground + 0.5, inside a ground bubble's masked interior and
below a flyer.* Every "proposed B-127" in this plan and in the review packet becomes B-128.

**WP4b (sim_types, sim_core, client).**
- `CombatEventVisual::ProjectileVisual` gains `ends: Option<StrikeEnds>`, with the same serde attributes and
  meaning as on `HitscanTracer` (`None` for synthetic events and old records). `sim_core`'s presentation writer
  fills it from the projectile's source and target entity. This is presentation output only: no authoritative
  state, no checksum field and no golden replay may change (stop rule §8; run the goldens in debug, release and on
  `wasm32-wasip1`).
- The client aims the projectile at the struck bubble by id through WP4's lookup (`effects::struck`): its arrival
  point and impact lie on that bubble's surface facing the shooter, at its drawn height. This applies to ground and
  air targets alike, as `at_struck_surface` already does for instant hits. If the lookup finds nothing, the current
  behaviour stays, and it is counted under `Nowhere`.
- Tests: a unit test per plane (ground target and flyer) shows the impact on the glass, not at the centre. The GPU
  wave `cross_plane` gains a projectile subject if the data offers an anti-air projectile shooter; otherwise a unit
  test covers it, and that is recorded. `weapon_battle_gpu_tests` must pass on radv with its bars unchanged.
- The inverse kind mismatches of §10 (`hive_matron`, `tesla_sentinel_dome`) stay out of scope: they re-record
  goldens.

**Review conditions, also done here.**
1. A system-level test proves that `spawn_effects_from_events` remembers the presented bubbles only after its
   lookups. Swapping the order must fail it.
2. A unit test of `lay_spell_impact_seen` with a real `BattleFog`, which hides the whole impact, shows that nothing
   is laid. Alternatively, §9's risk row is corrected to name the test that covers it.
3. `StruckBubbles` is cleared when the battle session changes, because entity ids restart in a new `SimWorld`.

**As built (2026-11-17).** WP4b and the three review conditions are on branch `effects-identity-client`.
- *Contract and writer.* `ProjectileVisual::ends` as specified; `launch_projectile` fills it from the prepared
  shot's attacker and target. No float is added, so the C7 ratchet is unchanged. The sim_core test
  `every_projectile_names_its_attacker_and_target` covers every shipped projectile weapon. Golden replays pass
  unchanged in debug, release and on `wasm32-wasip1` (wasmtime); nothing was re-recorded.
- *Client.* `effects::struck::at_struck_surface` re-aims a projectile with `ends` whose target bubble is found
  (`projectile_onto_struck`). The simulation publishes the arrival at `target - direction * collision_radius`, and a
  bubble's published radius is its collision radius, so the target's centre on the launch tick is the arrival moved
  on by the found bubble's radius. From there the arrival goes onto the bubble's outline (or glass, without a
  camera) towards the shooter, at its drawn height, at most `MOST_PULL` of the way back to the launch. The flight
  time is kept and the velocity is recomputed, so the travel and the impact both end there. The struck radius is
  returned for the impact's `UnitRadius`, as for instant hits. A projectile without `ends`, or whose bubble is
  found nowhere (counted `Nowhere`), keeps its published flight. Unit tests:
  `a_projectile_at_a_ground_unit_lands_on_its_glass`, `a_projectile_at_a_flyer_lands_on_its_glass_not_below_it`
  and `a_projectile_whose_target_is_unknown_keeps_its_flight`.
- *GPU wave.* The data offers an anti-air projectile shooter with looks on a presentable unit, so
  `weapon_battle_gpu_tests` gains a second `cross_plane` wave. It is a wave of its own, because each subject packs
  three ground units round its spot. The shooter is the first such unit by data, which fires `rocket_trail` and
  `rocket_hit` today. The test now runs 10 waves.
- *Condition 1.* `effects::systems::tests::the_presented_bubbles_are_remembered_only_after_the_frames_lookups`
  runs `spawn_effects_from_events` in an `App` on the shipped vocabulary. A killing hit pops while neither snapshot
  lists its target. The target must be found under `LastFrame`, which fails if `remember` runs before the lookups.
  A frame later the same event finds it `Nowhere`, which shows that the remember did run.
- *Condition 2.* `effects::areas::tests::a_real_fog_hiding_the_whole_impact_lays_nothing` refills a real
  `BattleFog` from a `PlayerView`. An enemy impact whose whole reach is unseen lays nothing, while one whose edge is
  seen, and the viewer's own, are laid. §9's risk row names this test.
- *Condition 3.* `StruckBubbles::follow(Option<SessionGeneration>)` runs at the top of `spawn_effects_from_events`
  with the presented session's generation. When the generation differs, or no session is presented, it forgets the
  last frame's bubbles and keeps the buffer's capacity. Unit test:
  `the_last_frame_is_forgotten_when_the_session_changes`.
- *Clippy, found on the branch.* The test modules of `struck`, `areas` and `systems` (WP4/WP6) used `expect` without
  the `allow` other test modules carry, and `sync_allocation_count_tests` had a manual `is_multiple_of`. Both
  failed `clippy --all-targets -D warnings` and are fixed here.
- *Evidence (radv, two runs, bars unchanged).* `needle_volley`+`needle_hit` changes 34 px beside the struck bubble
  against its blank's 7 (needs 34; 23 before WP4b). It passes exactly at the bar in both runs, which are
  deterministic. The projectile `cross_plane` subject shows 401 px beside the flyer (blank 1, needs 22) and 1 px
  beside the ground units (blank 1, allowed 9). The other projectile subjects changed as follows (Amendment 2
  → now): `acid_spit` 112 → 102, `grenade_lob` 188 → 254, `rocket_trail` 236 → 259, `plasma_bolt` 109 → 188 and
  `spore_bolt` 135 → 139. All pass with room. The instant and melee subjects are within 1 px of Amendment 2. See
  `docs/evidence/effects/review/identity_wp4b_measurements.txt`, `identity_wp4b_needle_volley_radv.png` and
  `identity_wp4b_cross_plane_projectile_radv.png`.
- *Open.* A projectile's *launch* is still published at the shooter's ground + 0.5. An anti-air flyer's
  projectile therefore starts under its bubble; the cross-plane sheet shows this. Amendment 3 scoped only the
  struck end. Aiming the launch from the shooter's bubble (the same lookup, by `ends.attacker`) is a follow-up. The
  needle look's margin over its bar is 0 px: the look is faint, and the bar and the look remain the review's
  (Q0080).

## Amendment 4: WP5 as built (2026-11-17, short note, charter §6)

WP5 (B-122) is on branch `effects-identity-looks`, made from `main` at `345da93f` (commit `dc9e438b`; its clippy
fixes landed with the WP7 commit `963b9a80`). Client and data only: no simulation crate is touched, and the golden
replays pass unchanged.

**As planned.** `effects.ron` gains `defaults.status_by_name: { "fungal_growth": "fungal_affliction" }`, which
replaces the B-122 comment. The resolver reads the effect names (an unknown one is the vocabulary's `UnknownName`);
`UnitEffectTables::build` resolves the status names against the unit catalog's spell catalog, refusing an unknown
one with the new `EffectCatalogError::UnknownStatus { path, name }` and a look of another event kind with
`WrongEventKind` (slot `status_by_name`). `UnitEffectTables::named_status(StatusId)` is a binary search over the
pairs sorted by status id. `BattleFrameState::presented_statuses()` shares the source choice of
`presented_effect_views` (a private `discrete_source`). `StanceTrack` gains `named: [Option<(StatusId, HeldLook)>;
MAX_NAMED_STATUSES]` (4) and `named_refused`, counted per frame a named status finds no slot, as `AreaLooks::refused`
counts. Named looks are held in addition to the kind looks, under the bubble's fog rule, lay their `emit` effect as
kind looks do, and erode out through the track's `releasing` slots.

**Choices made while building.**

- *No merge cursor.* `present_stances` iterates a Bevy query, whose order is not the entity order, so a cursor merged
  against id-ordered bubbles (D-5) does not apply. Each bubble takes its run of the presented list by binary search
  (`stance::statuses_of`, two `partition_point`s): O(log n) a bubble, nothing copied or allocated.
- *The fog test.* The plan's case "an undetected cloaked enemy under Fungal Growth" cannot be staged with the
  shipped data: `fungal_growth` has `reveal: true` (the game's `SuppressCloak`), so a fungaled cloaked unit is not
  concealed. `battle_fog_tests::a_named_status_look_shows_only_on_a_unit_the_viewer_sees` checks instead that a seen
  victim wears the look; that a victim cloaked while it bears the status is revealed (published `Concealment::None`)
  and keeps it; and that a victim moved out of the viewer's sight wears none while the status is still published.
  The `Shimmer` and `Hidden` paths are also covered by the unit test
  `a_unit_the_viewer_does_not_see_as_it_is_holds_no_named_look`.
- *The rim measurement.* `area_link_gpu_tests::fungal_growth_flies_lands_and_afflicts` reads each victim's bubble
  material shell rim every frame (as `status_gpu_tests` reads the stim's rim), and pictures each frame for the
  review. A Fungal victim must, from 0.4 s after it first bears the status (the slow's own onset flash is over by
  then), hold the named look on every frame, with the look's rim colour (±0.02 a channel) at strength 0.2 or more.
  The storm stand-in is found by data: an ability whose block names an `Area` look and whose effect tree applies a
  status of a kind the named status is also published as, with no named look (the Psionic Storm analogue). It is
  cast, without its look, by a second caster on two further enemy stand-ins; they must bear their status, hold no
  named look and never show the named rim. The old "afflicted exactly when the vocabulary names a `DamageOverTime`
  default" check is now "no look is keyed on the kind" (0 such spawns).
- `status_gpu_tests` is unchanged: none of its subjects bears a named status.

**Evidence** (radv, the RX 580, debug build, Xvfb 1280x720; `identity_wp5_wp7_measurements.txt` and the packet
section "Identity WP5/WP7").

| Check | Result |
|---|---|
| Fungal victims wearing the named look from 0.4 s after the status | 4 of 4 on every frame (41 each); rim strength 0.30 to 0.80 |
| Storm stand-in victims (status borne on 41 frames each) | no named look, rim 0 |
| Fungal impact centre (WP6) | 1.37 px from the aimed point (tolerance 2) |
| Fungal fog edge (E1-3) | 0 of 357 unseen ground samples changed; 488 of 619 seen |
| `effect_catalog_tests` (39), `battle_fog_tests` (25), stance and timeline unit tests | pass |

The first radv run failed E1-3 (223 of 357 unseen samples changed): the storm stand-in's caster stood out of range and
walked, moving the viewer's sight over the fog edge. It now stands within the ability's range; no bar was changed.

**Review round 1 (2026-11-18; `independent_review_identity_2.md`, accepted).** The Fungal affliction was accepted.
Two notes are answered here, and neither changes WP5's code:

- *A late frame.* The Fungal child now pictures 5.0 s from the casts instead of 3.6 s, so it covers the whole status
  and its look's exit. `identity_wp5_fungal_late_radv.png` shows six consecutive 40 ms frames at 1:1 after the ground
  stain has faded, with only the victims' rims left, and the same victims at 3x. On radv each victim bears the status
  on 71 frames and wears the named look on every one of the 62 frames from 0.4 s on. The impact centre is still
  1.37 px from the aimed point, and the fog edge still changes 0 of 357 unseen samples. No bar was changed.
- *The fog-edge stain cut along fog cells* (window x 975 to 1035, y 310 to 380 in
  `identity_wp5_fungal_window_radv.png`) is outside this plan's diff. It is registered as **B-132** in `DESIGN.md`
  §4 and is not fixed here.

## Amendment 5: WP7 as built (2026-11-17, short note, charter §6)

WP7 (B-126) is on branch `effects-identity-looks` (commits `9dfaaf34`, `963b9a80`, `68587cda` and this note). Client and data
only; the golden replays pass unchanged.

**As planned.** A `Shot` effect may declare `sustain: Some((grace: <s>))`. The loader refuses it on any other event
kind (`InvalidValue`, field `sustain`), with a grace that is not a positive number of seconds (`grace`), and when its
held layers' longest exit exceeds `MAX_SUSTAIN_EXIT` = 0.1 s (`exit`). A held layer on a `Shot` is allowed only when
it is sustained. The coverage test `every_shipped_weapon_names_the_looks_of_the_event_it_publishes` refuses a
sustained travel on a weapon that does not publish an instant beam. The new `effects::beams` module holds
`HeldBeams`: at most 64 slots sorted by `(attacker, EffectSource)`, found by binary search, with a `refused` counter.
`spawn_effects_from_events` hands each admitted strike to `HeldBeams::strike` before it asks for a travel shot; a
`BeamVisual` with `ends` whose travel is sustained refreshes its slot (a retarget moves the far end) or holds a new
instance, and spawns no shot. The deadline is the strike's time plus the attacker's presented
`attack_swing.period_ticks / 25` (the look's longest layer without a swing) plus the grace. `keep_beams` runs after
the event wiring: it puts each line from the attacker's bubble to the target's outline (`beam_line`, through the new
`struck::onto_struck_outline`), recomputes the fog cut with `shot_clip` for another player's beam, and releases a beam
whose attacker or target is not presented or whose deadline has passed. Per-strike burns stay one `Point` per strike.

**Data.** `prism_beam` and `lance_beam` gain `sustain: Some((grace: 0.12))`. Their body and core ribbons and their
muzzle glows become held layers (`duration: 0.04` to come in, along the new curves `body_beam_on` and `beam_on`, which
are `body_beam`'s and `beam_steady`'s first frames; `exit: 0.08`). The riding pulses stay one-shot layers, so a beam
shows its pulses as it comes on and then holds steady, its core scrolling. This is a look change for the review
(D-9).

**Choices made while building.**

- *A beam ends once its exit is over.* A released held instance lives on until its one-shot layers end, so the onset
  pulses (up to 0.43 s) would outlive a target that dies early. `HeldBeams` keeps released beams in a second
  64-slot list and ends each (`EffectRuntime::retire`) on the first keep after its exit; a full list ends one at
  once. A session change ends every beam at once (`HeldBeams::follow`, as `StruckBubbles::follow`), because entity
  ids restart.
- *Beams only.* D-4 holds `BeamVisual` strikes; a `HitscanTracer` is never held, so the coverage rule reads "an
  instant beam" (`LaserBeam` or `LightningArc` without a projectile speed), narrower than "an instant line".
- *The beam leaves the attacker's bubble.* The line starts at the attacker's drawn centre, so a flyer's beam no
  longer starts at its ground + 0.5 (unit test `a_flyers_beam_leaves_its_own_bubble_and_ends_on_the_targets_glass`).
  A strike's first frame uses the event's travel line; `keep_beams` moves it onto the bubbles on the same frame.
- *A refused strike draws no travel*, as a refused area draws nothing; it is counted.
- *The GPU test is a child of its own* in `weapon_battle_gpu_tests.rs` (`held_beam_child`, parent
  `a_held_beam_follows_its_target_and_ends_with_it`), not a wave of the family waves: it needs a walking target, a
  kill and paused drawn/bare picture pairs, which the waves' staging does not have. No shipped beam unit has media,
  so, as the waves lend lab families, the child lends the first shipped sustained look (`prism_beam` with its burn)
  to a presentable ground shooter in the test world's catalog only, its weapon made an instant `LaserBeam` with a
  range of at least 6 (so the walking target stays in range). Every frame, after transforms propagate, it records the
  held beam's line end and the drawn target bubble's outline point towards the shooter (from the bubble's
  `GlobalTransform` and the camera), both projected. Both follow the same geometry, so this check proves that the
  beam follows the drawn bubble on the same frame (a first version, recorded before propagation, measured a constant
  one-frame lag of 5 px, the test's own); the picture pair proves the drawn end. The parent requires one beam and one live instance for the
  whole stream, the end within 3 px of the outline point on every frame, beam pixels along the path and within 3 px
  of its end in the mid-stream pair, no live instance after the exit plus one frame from the target's death, and no
  changed pixel along the former path (from 0.2 to 0.6 of its length, clear of the death look) in the pair taken
  then.
- *Steady state.* The valgrind effect stream sends, every frame, a beam from one bubble at a target that changes
  every fourth frame, and every seventh frame a beam at an entity that is not presented (a dead target). The window
  difference is 0. Those dead-target strikes find their struck bubble nowhere, so the stream's `Nowhere` bound is
  their count, not 0. `sync_steady_state_tests` tracks `HeldBeams::buffers` and gives every air weapon of its stream
  a sustained beam.

**Evidence** (radv, debug build, Xvfb 1280x720; `identity_wp5_wp7_measurements.txt`, `identity_wp7_held_beam_radv.png`).

| Check | Result |
|---|---|
| Beams held and live through the stream | 1 and 1 on all 40 frames |
| Beam end against the drawn target's outline point | 0.00 px on 40 frames; the end travelled 81 px |
| Mid-stream pair: changed pixels along the path / within 3 px of its end | 306 / 9 |
| After the death (target gone on frame 47, exit 0.08 s): live instances after exit + 1 frame; changed pixels on the former path | none; 0 |
| `weapon_look_tests` (14, six new), `effect_catalog_tests` (39), `beams` unit tests (3) | pass |
| Valgrind effect stream with held beams, retargets and dead targets | window difference 0 (baseline 0) |
| `sync_steady_state_tests` combat streams | pass |
| Whole binaries on radv: `area_link_gpu_tests` (3), `status_gpu_tests` (3, not re-baselined), `weapon_battle_gpu_tests` (the family waves and the held beam) | pass; the waves' first run waited out the GPU lock (held 290 s by another user) and passed on the rerun with bars unchanged. In the waves `prism_beam` and `lance_beam` are lent to a hitscan shooter, so they are not held there: impact 94 px against a need of 46, and 136 against 44 |

**Review round 1 (2026-11-18; `independent_review_identity_2.md`, accepted with conditions).** Branch
`effects-identity-looks`; the packet section "Identity WP5/WP7, round 2" answers each finding.

- **R2-1 (condition): the steady beam's per-strike accent.** `beam_burn` and `lance_burn` each gain a first layer: a
  bright `Critical` flare of the beam's own ramp, 0.6 of the struck unit's scale, over 0.14 s (`flash` curve), on the
  target's rim where the strike lands. The held beam keeps its structure: no pulse per strike, and the body is still
  held steady. The held core gets no flicker; the flare is the per-strike accent. To show it at 1:1, the held-beam GPU
  child lends the cadence of the shipped beam weapon with the shortest period (9 ticks, the Void-Ray analogue's)
  together with its look. It now records each frame's youngest burn and pictures a 96 px crop round the beam's end on
  every frame from the 4th to the 19th of the stream. On radv strikes landed on frames 15 and 23 (burn age 0.00).
  `identity_wp7_beam_strikes_radv.png` shows frames 13 to 24 at 1:1 and, at 3x, the frame before a strike, the
  strike, the frame after, and the frame before the next.
- **R2-2: the table's size and its refusals.** `HeldBeams::capacity_for(units, effects)` sizes a battle's table at
  load (`load_effect_catalog`). The catalog bound is `MAX_PLAYERS × STANDARD_SUPPLY_CAP / supply cost × sustained
  weapons`, taking the largest beam unit type; the shipped catalog's cheapest beam units cost 2 supply, which gives
  800. The table is then limited to a quarter of the effect pool (`budget.effects / 4`, 512 shipped), because every
  held beam is a live effect instance. A table made without a catalog keeps `HELD_BEAMS` (64). A strike that
  `EffectRuntime::hold` cannot place (pool full) is now refused and counted, as a full table is. The effect-stress
  diagnostics (`StressSummary::beams_refused`, `beams_capacity`, printed in the frame-budget report's effect-stress
  line next to the runtime's refusals) report it. `the_table_is_sized_from_the_catalog_and_the_budget` and
  `a_full_effect_pool_refuses_and_counts` test both. The valgrind beam stream's fixture pool (64 effects) is at
  times full when a dead-target beam is struck. Its check therefore allows those refusals, at most one per planted
  dead-target strike, where it allowed none before (R2-2 now counts them); the window difference is still 0.
- **R2-3: pause.** Deadlines run on the presented battle clock (`beams::battle_seconds` of
  `BattleFrameState::presented_instant`), not on `Time<Virtual>`. A paused battle (speed 0) or a presentation that
  waits for a tick therefore freezes them, as every other held look's facts freeze. A channelling beam neither erodes
  while paused nor replays its onset on resume. Its onset and exit still run on `Time<Virtual>`.
  `a_paused_battle_freezes_the_beam_and_resumes_it_without_a_new_onset` checks this: ten seconds of effect time
  paused, the same beam afterwards, one hold in all, and release a period and the grace of battle time after the
  last strike.
- **R2-7.** A strike without a travel line is taken but holds nothing; the next strike with a line holds the beam
  (`a_strike_without_a_line_holds_no_beam_at_the_origin`).
- *Seen in the strike sequence, open:* a burn is a `Point` effect laid where the strike landed. On a target that walks
  fast, its 0.46 s body therefore stays behind the moving contact point, about 36 px by the next strike (review round
  2, R3-1; the purple dot right of the target in `identity_wp7_beam_strikes_radv.png`). This is the burns' behaviour
  since WP11, not a held-beam change. Fixed in review round 2 (below).

*Evidence, round 2 (radv).* Held beam: one beam and one live instance on all 40 frames; end 0.00 px from the target's
outline point while it travelled 163 px; mid-stream pair 653 px along the path and 19 px within 3 px of its end;
after the death no live instance and 0 px on the former path. Valgrind: effect stream window difference 0 (baseline
0). `weapon_look_tests` 18, `effect_catalog_tests` 39, `client_diagnostics` lib 117, the `beams` unit tests,
`cargo gate quick` GREEN, `clippy -p client -p client_diagnostics --all-targets -D warnings` clean.

**Review round 2 (2026-11-18; `independent_review_identity_3.md`, accepted with conditions).**

- **R3-1 (condition): a held beam's burns follow its end.** There were two candidate designs. One keeps the burns in
  the held-beam table. The other anchors every `Point` impact that names `StrikeEnds` to its struck bubble while that
  bubble is presented. The table was chosen. It is the only place that already knows the beam's end on the frame it
  is drawn, and it already holds one slot per beam. It changes no other weapon's impacts: a projectile's or a blow's
  hit stays where it landed, as the game draws it. It needs no per-impact lookup either.
  - Each `BeamSlot` keeps the handles of its last two strikes' burns (`burns: [Option<EffectHandle>; 2]`). Two,
    because a burn (0.46 s shipped) outlives the next strike (0.36 s).
  - The event wiring hands the burn it spawns for a held beam's strike to `HeldBeams::attach_burn`.
  - Every frame, `keep` moves both burns to the beam's end (`line.to`, the target's outline point) with the new
    `EffectRuntime::move_anchor`, on the same frame and geometry as the line. A burn that has ended drops its stale
    handle.
  - No allocation, and no search beyond the slot's own binary search. Older burns and the burns of a released beam
    stay where they last were.
  - Tests: unit test `a_held_beams_burns_follow_its_end`. The held-beam GPU child records the youngest burn's
    projected centre every frame, and the parent requires it within 3 px of the beam's end on every frame of the
    stream with a burn.
  - On radv: 0.00 px over 40 frames (was about 36 px by the next strike). The other measurements are unchanged: end
    0.00 px, mid-stream pair 656 / 20 px, nothing left after the death.
  - `identity_wp7_beam_burn_follows_radv.png` shows the same 1:1 sequence and 3x frames as round 2, with no burn left
    behind.
- **R3-2: the table-size test is exact.** `the_table_is_sized_from_the_catalog_and_the_budget` now computes the
  catalog bound on its own and pins it for the shipped data: 8 × 200 / 2 × 1 = 800. It asserts the table is the
  smaller of that and the pool share (512). With a pool of a million effects the bound wins (800). With every
  `sustain` removed the size is 0.
- **R3-3:** the Fungal child's fourth count is now the named status looks held, not the kind-keyed look that WP5
  removed and that always printed 0. The parent asserts at least one per Fungal victim, and checks from the vocabulary
  that no look is keyed on `DamageOverTime`. Its fog-edge message is joined without the run of spaces.
- **R3-4:** no change.

## Amendment 6: WP8 as built, and the plan closed (2026-11-18, short note, charter §6)

WP8 is on branch `effects-identity-wp8`, made from `main` at `8662f22b`. It changes documentation and evidence
only, with no code or test. The plan is Done.

**The evidence packet.** [`docs/evidence/2026-11-17-effect-snapshot-identity.md`](../evidence/2026-11-17-effect-snapshot-identity.md)
sets the radv figures the reviews accepted beside a lavapipe run of the same three tests, at `8662f22b`, with
`SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` (CI's driver):

- `a_held_beam_follows_its_target_and_ends_with_it`;
- `fungal_growth_flies_lands_and_afflicts`;
- `weapon_looks_fire_in_a_staged_battle`, which includes both `cross_plane` waves.

Every bar passes on lavapipe on the first run, and no bar or staging was changed:

- *Held beam.* The end lies 0.00 px from the target's outline point, and the burns 0.00 px from the end, on all
  40 frames. One beam is held through the stream, and nothing is left after the death.
- *Fungal.* The ring is centred 0.90 px from the aimed point (radv 0.94). No unseen fog sample changes. All four
  victims wear the named look on all 62 frames, and the storm stand-in's victims wear none.
- *Cross-plane.* The instant hit shows 274 px beside the flyer (needs 74) and 9 px beside the ground units (allowed
  17). The projectile shows 402 px (needs 22) and 1 px (allowed 9).
- *`needle_volley`.* It passes exactly at its bar on lavapipe too (34 px, needs 34), as on radv (Amendment 3).

The packet's sheets show the three sequences at 1:1 and 2x on lavapipe, next to the radv sheets.

**No further review round.** The lavapipe pictures match the accepted radv ones frame for frame. The beam and burn
positions agree to 0.00 px, and the counts differ by a few pixels of rasterisation. lavapipe showed nothing the radv
review could not have seen, so under Q0080 no round was opened.

**Bookkeeping.**
- `DESIGN.md` §4:
  - B-122 to B-126 and B-128 are marked fixed, with their commits and merges.
  - The "Effect presentation" row describes the lookup by id, the named statuses, the impacts laid by event and the
    held beams. Its known-issues cell now lists B-132, B-133 and B-107; the WP11 review note is gone, since WP11 was
    accepted.
  - The new entry this plan named in §10 is B-128, registered by Amendment 3; it is now marked fixed.
  - Amendment 3's open item, a projectile launched from the shooter's ground + 0.5, is registered as **B-133**.
- `docs/plans/README.md`: this plan's row is Done.
- Plan `2026-11-09-effect-visualization` gains Amendment 30, which points from its WP11 follow-ups to this plan.
- `crates/gate/suites.ron`: the plan added no test file (`effects/beams.rs` is a source file, and its unit tests
  run in the `client` lib). C22 (`suite_catalog_tests`) and `workspace_checks` pass unchanged.

**Throughput (Amendment 1's deferred comparison).** The packet's §4 compares the identity simulation merge
`c4bf2f16` with its first parent `bf4a7d2d`: five alternated release runs each, under the gate lock. Every row is
within noise. On the flat rows the before and after medians are 3264/3282, 950/947, 163/168, 191/198 and 204/204
ticks/s. `showcase_canyon` is 119/122, and the 67-caster battle (about 100 statuses a tick) is 236/239. The D-8
item "the release throughput benchmark stays within noise" is met.

**Out of scope, recorded (Q0002).**
- *A frame-budget run with held beams* (§9's risk row). The `frame_budget` scene fights with two armies of one unit
  type. It holds no beam unless a beam unit is chosen, so a run would not measure the per-frame fog cut of held
  beams. Two things bound that cost instead. `HeldBeams` is sized from the catalog and the effect pool (R2-2: at most
  512 shipped). The effect-stress diagnostics report `beams_capacity` and `beams_refused`. A measured
  beam-heavy frame budget belongs with the next effect frame-budget pass.

## Amendment 7 (orchestrator, 2026-11-18): B-133, a projectile's flight starts on the shooter's bubble (short note, charter §6)

**What changes.** B-133: the client starts a projectile's flight at `ProjectileVisual::pos` on its launch tick,
which the simulation publishes at the shooter's ground + 0.5, so an anti-air flyer's projectile leaves from about
2.5 units below its own bubble (`identity_wp4b_cross_plane_projectile_radv.png`). The fix is presentation only:
the client starts the flight on the shooter's bubble, found by `ends.attacker` through the same lookup that B-128
uses for the struck end (`effects::struck`), at the bubble's glass towards the target, the mirror of WP4b's
landing rule. When the attacker is not found (dead or unseen on the launch frame), the flight starts where it does
today, and that fallback is counted, as the struck end's is. The simulation, the published records and every
checksum are unchanged.

**Why.** It is the open half of B-128 (WP4b aimed only the struck end); the same lookup and the same geometry
apply, so no new design is needed.

**How it is tested.**
- Unit tests beside the launch code: a flyer shooter starts its flight on its bubble's glass towards the target; a
  ground shooter's start moves only to its own glass; an unknown attacker keeps today's start and is counted.
- The GPU test that produced the B-133 evidence (`weapon_battle_gpu_tests`, the cross-plane projectile subject)
  gains a bar: the projectile's first drawn position lies within 3 px of the shooter's drawn bubble outline, on
  radv; the evidence picture is regenerated beside the old one.
- Steady state: the launch lookup allocates nothing (`sync_steady_state_tests` / the valgrind count stay at 0).
- Determinism: `git diff main -- crates/sim_*` is empty; goldens untouched.

On acceptance B-133 is marked fixed in `DESIGN.md` §4 with its commits.

**As built (2026-11-18, `d5449de9`, branch `projectile-launch`).** `effects::struck::launch_from_shooter` moves the
start, after `at_struck_surface` has aimed the arrival, onto the shooter's outline (glass without a camera) towards
the target at `LAUNCH_MARGIN` = 0 (on the glass itself, so the start lies on the drawn outline), the shooter's centre
recovered from the published start and its drawn radius, the move at most `MOST_PULL` of the way to the arrival; the
arrival and flight time are kept. The shooter is looked up by `StruckBubbles::find_shooter`, the same places as the
struck end, counted apart (`shooter_found_in`); `on_bubbles` does both ends for `spawn_effects_from_events`. Unit
tests `a_flyer_shooters_flight_starts_on_its_glass_towards_the_target`,
`a_ground_shooters_start_moves_only_to_its_own_glass`, `a_projectile_whose_shooter_is_unknown_keeps_its_start_and_is_counted`.
On radv the cross-plane projectile's flight starts 24.70 px from the shooter's drawn centre, its outline 24.70 px out:
0.00 px off (bar 3 px; exact because the wave fires along the screen's x axis, which the test's outline radius also
uses); every other bar passes, `rocket_trail` (Across) 259 → 242 px against needs 22, the rest within 1 px of
Amendment 3 except `lance_beam` 115 → 143 and `prism_beam` 86 → 109. The valgrind effect stream now sends a
projectile a frame; its window difference stays 0. `git diff main -- crates/sim_*` is empty. Evidence:
`identity_b133_cross_plane_projectile_radv.png` beside `identity_wp4b_cross_plane_projectile_radv.png`.
