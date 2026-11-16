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
| `SpellImpact` admitted without a fog check leaks an unseen impact | The handler's `area_sight` returns `Hidden` and lays nothing; `battle_fog_tests` covers it |
| RON cannot read `visual_kind: None` for an enum field | D-7 fallback (`rename`), recorded |
| Review rejects the shown `fungal_affliction` or the held beam | Q0080 rounds; the facts and lookups ship regardless, only the data naming waits |
| Changed `jet_jumper` / `void_goliath` looks are unseen in play (no media) | Lab and stand-in GPU coverage, as WP11 did; recorded |

---

## 10. Out of scope (recorded, Q0002)

- **Proposed new register entry (B-128; B-127 went to a gate flake first):** a `ProjectileVisual` is published at
  ground + 0.5 at both ends. An anti-air projectile therefore flies and lands below its flyer (the same cross-plane
  fault as B-125, for projectiles). The fix publishes `ends` on `ProjectileVisual` and re-aims the arrival on the target's bubble. It is
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
